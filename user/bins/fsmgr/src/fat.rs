//! fat.rs —— P6 A-5 FAT12 只读底座 / A-6 落盘读写层（消费 `berry-fatfs`
//! 纯编解码 + vblk 块服务）。
//!
//! 拓扑（svcmgr A-5 预拉序）：vblk 由 svcmgr **先于本服务**拉起且自验已过
//! （BLKD 门），blk 服务端端 blk_a 经本服务的 env 块 gift[3] 交付（libc
//! `blk_h()`）——本层是 vblk 的**唯一 client**（blkproto 严格串行 RPC，无
//! 回复抢答面）。eager init：服务启动即定几何、整读 FAT0 + 根目录（≈6 扇
//! 区）；此后数据 GET 按簇读盘（单簇缓存，顺序读零重复 RPC）。
//!
//! 分层路由（A-6 + C-0 刀 3）：`name_to_fat11` 认的 8.3 大写名 → FAT 层
//! （盘上真文件）；其余（小写/多段等 RAM 名）→ RAM 树（berry-fstree）——
//! 名字空间按构造隔离，同一名字按写法天然只命中一层。**多段 NUL 路径的
//! FAT 面只读**（刀 3）：`DATA\0NOTE.TXT` 逐段走查——中段须目录（DIR 属性
//! 项即目录文件、簇链 = 子项表）、中段文件 = Miss（fstree 同义）；写面
//! （PUT/TRUNC/staging）恒单段——多段构造性走 RAM 树，**FAT 建删目录不在
//! C-0 盒、FAT 文件删除留 v2、`>` 只落根**（诚实缺口）；盘上同名目录作
//! 文件写目标 = ST_BAD（类型冲突）。**staging + TRUNC 即 commit**（与
//! bsh fs_put 的「分片 PUT + 尾随 TRUNC」协议恰合）：PUT 分片累积进单一
//! staging 槽、TRUNC(len) 触发落盘（近原子——commit 前读者只见旧版/无文
//! 件）；跨名写自动先 commit 旧槽（bsh 串行 client 每段重定向一个目标）。
//! GET 先 staging 后盘（读到正在写的文件）；LIST = RAM 名 + FAT 名（含
//! 目录名）+ 未落盘的 staging 名。
//!
//! commit 序（crash 窗口 v1 文档化——FAT 无原子多扇区写）：**工作副本**
//! fat2/root2 上 collect 旧链 →（旧链标记未释放下）分配新链（防重用）→
//! 写数据扇区（簇尾零填）→ 释放旧链 → 写双 FAT → 写根目录项（同名覆写/
//! 首个 End·删除槽/无槽 = ST_FULL）→ 根扇区回读对账 → 全过才安装内存态。
//! 任一步失败 = 内存态零变化（client 重 PUT 重试）；盘上只多孤儿数据簇
//! （FAT/根未更新的写入，无簇号泄漏）。掉电落在「FAT 已写、根未写」窗口
//! = 单文件可坏（旧链已释放仍被根引用）——跨文件不传染，覆写路径下次
//! commit 自愈（旧链从盘面 collect 到 Free 即止）；v1 接受，journaling 非
//! 本阶段面。
//!
//! 会话纪律（bsh fs_rpc 同款）：**收侧**超时/错码 = 会话毒化（迟到应答会
//! 错配下一请求 → 静默错数据；确定性降级 ST_BAD loud 优于无告警错数据）；
//! 发送侧失败不毒化（请求未离表）。卷定位 = 探测 {LBA0, LBA16} + BPB
//! `hidden` 一致性对账（mkfat 卷恒 @16、hidden=16；LBA0 保留区按构造无
//! 55AA 签名，parse_bpb 必拒）。

use berry_blkproto::{encode_request, parse_reply, OP_READ, OP_WRITE, SECTOR_SZ, ST_OK};
use berry_fatfs::{
    dir_entry, encode_file_entry, fat11_display, fat12_entry, fat12_set, find_free_cluster,
    name_to_fat11, parse_bpb, Bpb, DirFile, DirKind, Fat12Entry, FAT12_EOC,
};
use berry_fsproto::{
    encode_reply_get, CHUNK, FILE_SIZE_MAX, LIST_MAX, ST_BAD, ST_FULL, TYPE_DIR, TYPE_FILE,
};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_one, berry_vmo_commit, berry_vmo_create,
    berry_vmo_read, berry_vmo_write, Handle, RIGHT_READ, RIGHT_WRITE, SIGNAL_READABLE,
    SIGNAL_WRITABLE, BERRY_ERR_BUSY, CLOCK_MONOTONIC,
};

use crate::trap;

/// blk RPC 帽（send 侧 EBUSY 重臂 / 应答等待）：vblk 已过自验门在服务，
/// 10s 宽帽。
const RPC_CAP_NS: u64 = 10 * 1_000_000_000;
/// RPC bounce VMO 尺寸：单扇区 512，但 commit 按页粒度（BLKD 实踩——
/// commit 512 只授 0 页）。
const VMO_SZ: u64 = 4096;
/// FAT0 静态副本帽（mkfat fat_sz=2 → 1024B 恰合；几何超帽 = init 拒）。
const FAT_CAP: usize = 1024;
/// 根目录静态副本帽（root_secs=1 → 512B）。
const ROOT_CAP: usize = 512;
/// 单簇缓存帽（spc=4 → 2048B）。
const CLUS_CAP: usize = 2048;

/// 目录簇链走查帽（resolve/readdir——环状 FAT 钉死；mkfat 单簇目录，16
/// 簇 = 32KiB 子项表面，v1 卷几何内充裕）。
const DIR_CHAIN_MAX: u32 = 16;

/// GET 兜底结果（RAM 未命中后的 FAT 面三态 + 命中）。
pub enum GetRes {
    /// 命中（rep 已落字，值为应答长）。
    Served(usize),
    /// 不在 FAT（含非 FAT 名字空间）——调用方回落 ST_NOENT。
    Miss,
    /// 文件在盘上但超出协议文件尺寸帽（ST_FULL 诚实降级 ≠ NOENT）。
    Full,
    /// 命中的是目录（类型冲突——调用方回 ST_BAD，fstree IsDir 同义）。
    IsDir,
    /// 服务面坏（会话毒化 / 链断 / RPC 失败）。
    Bad,
}

/// FAT 面路径解析结果（读面，C-0 刀 3）。
enum FatPath {
    /// 终段是文件（簇链可 serve）。
    File(DirFile),
    /// 终段是目录（size 恒 0；first_clus = 子项表簇链头）。
    Dir(DirFile),
    /// 路径不在 FAT（非 8.3 段 / 中段文件 / 未命中）。
    Noent,
    /// 走查坏（会话毒化 / RPC 失败 / 链断）。
    Bad,
}

/// 目录簇链扫描结果（resolve 走查内部）。
enum ChainScan {
    Found(DirKind),
    Missing,
    Bad,
}

/// v2 STAT/LOOKUP 的 FAT 回落结果（刀 2 起单段、刀 3 起多段泛化）。
pub enum FatStat {
    /// (type, size)——文件 (TYPE_FILE, size) / 目录 (TYPE_DIR, 0)。
    Ok(u8, u32),
    /// 非 FAT 名或盘上无此路径。
    Noent,
    /// 走查坏（多段链面：会话毒化 / RPC 失败 / 链断）。
    Bad,
}

/// v2 READDIR 的 FAT 回落结果（刀 3）。
pub enum FatList {
    /// 子名 NUL 连接已落 buf（值为总长）。
    Names(usize),
    /// 路径不在 FAT。
    Noent,
    /// 路径是文件（非目录）。
    NotDir,
    /// 走查坏（会话毒化 / RPC 失败 / 链断）。
    Bad,
}

/// FAT 层 PUT 分片结果（None = 非 FAT 名——调用方回落 RAM 通道）。
pub enum StageRes {
    /// 已入 staging 槽（TRUNC 点 commit 落盘）。
    Staged,
    /// 拒（st 直接回 client：ST_FULL/ST_BAD）。
    Deny(u8),
}

/// FAT 只读层（单例，fsmgr main 持有；几何静态化在 init）。
pub struct Fat {
    /// blk 服务端端（blk_a 经 env 块迁移，本表唯一持有）。
    h: Handle,
    /// RPC bounce VMO（本体常驻本表；每请求 dup 别名随行迁移，driver 用后
    /// close——blkproto client 契约）。
    vmo: Handle,
    /// 会话活性（false = 收侧曾超时/错码，FAT 面此后确定性 ST_BAD）。
    sess_ok: bool,
    /// 卷几何（bps==512 与三缓冲容量帽已在 init 断言）。
    bpb: Bpb,
    fat: [u8; FAT_CAP],
    root: [u8; ROOT_CAP],
    cache: [u8; CLUS_CAP],
    /// 单簇缓存当前簇号（0 = 无效——簇号域 ≥ 2）。
    cache_clus: u32,
    /// 单槽 staging（A-6：FAT 名的 PUT 分片累积；TRUNC = commit 触发点，
    /// 槽内 (8.3 名, 内容)——跨名写自动先落盘旧槽）。
    staging: Option<([u8; 11], Vec<u8>)>,
}

/// eager init：vmo bounce → 探测卷 → 整读 FAT0 + 根目录。失败 = None
/// （RAM-only 降级续走——诊断行先行，服务本体静默纪律的例外面）。
pub fn init(h: Handle) -> Option<Fat> {
    if h == 0 {
        return None; // 拓扑未交付块设备端（无 vblk 面）——静默 RAM-only
    }
    let mut vmo = 0 as Handle;
    if berry_vmo_create(VMO_SZ, 0, &mut vmo) != 0
        || vmo == 0
        || berry_vmo_commit(vmo, 0, VMO_SZ) != 0
    {
        diag("vmo");
        return None;
    }
    // 卷定位：候选 {0, 16} 逐个试（LBA0 = vblk 测试保留区，按构造无签名）；
    // parse_bpb 过 + hidden == 探测位 双验才算命中（hidden != 位 = 非 mkfat
    // 卷——后续扇区折算会整体错位，拒）。
    let mut boot = [0u8; SECTOR_SZ];
    let mut bpb: Option<Bpb> = None;
    let mut rpc_failed = false;
    for &cand in &[0u64, 16u64] {
        if rpc_read(h, vmo, cand, &mut boot) != RpcRes::Ok {
            rpc_failed = true;
            break; // 设备面失败——换 LBA 重试无意义（同一设备）
        }
        if let Some(g) = parse_bpb(&boot) {
            if g.hidden as u64 == cand {
                bpb = Some(g);
                break;
            }
        }
    }
    let bpb = match bpb {
        Some(g) => g,
        None => {
            let _ = berry_handle_close(vmo);
            diag(if rpc_failed { "rpc" } else { "bpb" });
            return None;
        }
    };
    // 消费方断言（berry-fatfs 模块头约定）：bps==512（vblk 单扇区粒度）；
    // 三静态缓冲容量帽（超帽几何 = 非 mkfat 盘，拒之不缩之）。
    if bpb.bps != 512
        || bpb.fat_sz as usize * SECTOR_SZ > FAT_CAP
        || bpb.root_secs as usize * SECTOR_SZ > ROOT_CAP
        || bpb.spc as usize * SECTOR_SZ > CLUS_CAP
    {
        let _ = berry_handle_close(vmo);
        diag("geom");
        return None;
    }
    let mut fat_buf = [0u8; FAT_CAP];
    let mut root_buf = [0u8; ROOT_CAP];
    let mut sec = [0u8; SECTOR_SZ];
    let base = bpb.hidden as u64 + bpb.fat_sector as u64;
    for i in 0..bpb.fat_sz as u64 {
        if rpc_read(h, vmo, base + i, &mut sec) != RpcRes::Ok {
            let _ = berry_handle_close(vmo);
            diag("rpc");
            return None;
        }
        let o = i as usize * SECTOR_SZ;
        fat_buf[o..o + SECTOR_SZ].copy_from_slice(&sec);
    }
    let base = bpb.hidden as u64 + bpb.root_sector as u64;
    for i in 0..bpb.root_secs as u64 {
        if rpc_read(h, vmo, base + i, &mut sec) != RpcRes::Ok {
            let _ = berry_handle_close(vmo);
            diag("rpc");
            return None;
        }
        let o = i as usize * SECTOR_SZ;
        root_buf[o..o + SECTOR_SZ].copy_from_slice(&sec);
    }
    // FAT1 一致性不核（mkfat 双副本同写；v1 信任面文档化）。
    Some(Fat {
        h,
        vmo,
        sess_ok: true,
        bpb,
        fat: fat_buf,
        root: root_buf,
        cache: [0u8; CLUS_CAP],
        cache_clus: 0,
        staging: None,
    })
}

/// init 失败诊断（console 一行——链断级事件，`FAT ok` 缺失是 smoke 面，
/// 人读得懂的原因先行）。
fn diag(why: &str) {
    println!("fsmgr: fat init failed: {why}");
}

/// RPC 单读结果三态（毒化判定归调用方）。
#[derive(PartialEq, Eq)]
enum RpcRes {
    /// ST_OK 且数据已收割进 out。
    Ok,
    /// 干净失败（请求未离表 / 应答已收但非 OK）——会话仍同步，不毒化。
    Fail,
    /// 会话不可知（应答帽超时 / recv 错码）——迟到应答会错配下一请求。
    Poison,
}

/// 严格串行 RPC 读一扇区（svcmgr blk_rpc 同形、只读面）：dup VMO 别名随
/// send 迁移（EBUSY → wait WRITABLE 重臂；非 EBUSY 失败 = 别名未 take，
/// 显式回收防表槽泄漏）→ 应答帽 wait READABLE → recv 恰 1B → ST_OK 才
/// 从 VMO 收割。
fn rpc_read(h: Handle, vmo: Handle, lba: u64, out: &mut [u8; SECTOR_SZ]) -> RpcRes {
    let mut req = [0u8; 16];
    let n = match encode_request(&mut req, OP_READ, lba) {
        Some(n) => n,
        None => return RpcRes::Fail, // 扇区域恒合法——编码失败 = 调用方 bug 面
    };
    let mut dup = 0 as Handle;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_WRITE, &mut dup) != 0 {
        return RpcRes::Fail;
    }
    let hs = [dup];
    loop {
        let s = berry_channel_send(h, req.as_ptr(), n, hs.as_ptr(), 1);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            let _ = berry_handle_close(dup); // 未被 take——回收
            return RpcRes::Fail;
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, now() + RPC_CAP_NS, &mut obs);
        // 超时重臂（常驻服务不自杀——真死由应答帽判）
    }
    let mut obs = 0u64;
    if berry_object_wait_one(h, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
        return RpcRes::Poison; // 应答帽到点：请求在途，会话状态不可知
    }
    let mut rep = [0u8; 8];
    let mut nb = rep.len();
    let mut nh = 0usize;
    if berry_channel_recv(h, rep.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh) != 0 {
        return RpcRes::Poison;
    }
    if parse_reply(&rep[..nb]) != Some(ST_OK) {
        return RpcRes::Fail; // ST_RANGE/ST_BAD/野值：应答已收，会话仍干净
    }
    if berry_vmo_read(vmo, out.as_mut_ptr(), 0, SECTOR_SZ) != 0 {
        return RpcRes::Fail;
    }
    RpcRes::Ok
}

/// 严格串行 RPC 写一扇区（rpc_read 的写侧镜像）：数据**先**入 VMO 本体
/// （driver 收别名后才从别名读——send 后再写 = driver 可能读旧字节）→
/// dup 别名随 send 迁移（EBUSY → wait WRITABLE 重臂；非 EBUSY 失败 = 别名
/// 未 take，显式回收防表槽泄漏）→ 应答帽 → ST_OK。
fn rpc_write(h: Handle, vmo: Handle, lba: u64, src: &[u8; SECTOR_SZ]) -> RpcRes {
    let mut req = [0u8; 16];
    let n = match encode_request(&mut req, OP_WRITE, lba) {
        Some(n) => n,
        None => return RpcRes::Fail, // 扇区域恒合法——编码失败 = 调用方 bug 面
    };
    if berry_vmo_write(vmo, src.as_ptr(), 0, SECTOR_SZ) != 0 {
        return RpcRes::Fail; // 数据未离表——干净失败，不毒化
    }
    let mut dup = 0 as Handle;
    if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_WRITE, &mut dup) != 0 {
        return RpcRes::Fail;
    }
    let hs = [dup];
    loop {
        let s = berry_channel_send(h, req.as_ptr(), n, hs.as_ptr(), 1);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            let _ = berry_handle_close(dup); // 未被 take——回收
            return RpcRes::Fail;
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, now() + RPC_CAP_NS, &mut obs);
        // 超时重臂（常驻服务不自杀——真死由应答帽判）
    }
    let mut obs = 0u64;
    let ws = berry_object_wait_one(h, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs);
    if ws != 0 {
        return RpcRes::Poison; // 应答帽到点：请求在途，会话状态不可知
    }
    let mut rep = [0u8; 8];
    let mut nb = rep.len();
    let mut nh = 0usize;
    if berry_channel_recv(h, rep.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh) != 0 {
        return RpcRes::Poison;
    }
    if parse_reply(&rep[..nb]) != Some(ST_OK) {
        return RpcRes::Fail; // ST_RANGE/ST_BAD/野值：应答已收，会话仍干净
    }
    RpcRes::Ok
}

impl Fat {
    /// GET：多段路径走 FAT 子目录链（刀 3 读面）；单段 = staging 优先
    /// （读到正在写的文件；staged 名不经盘——会话毒化下也照常服务，数据
    /// 在内存）→ 盘上 FAT 面 + 簇界分片服务（chunk = 协议窗、文件尾、簇
    /// 界三取小——dlen < CHUNK 合法，more 链由 client 侧 off 前进收敛）。
    pub fn handle_get(&mut self, name: &[u8], off: u32, rep: &mut [u8; 256]) -> GetRes {
        if name.contains(&0) {
            // 多段：子目录走查（写面恒单段——多段 PUT/TRUNC 在 main 层
            // 构造性走 RAM 树，到不了这里）。
            return match self.resolve(name) {
                FatPath::File(f) => {
                    if f.size as usize > FILE_SIZE_MAX {
                        return GetRes::Full;
                    }
                    self.serve(&f, off as usize, f.size as usize, rep)
                }
                FatPath::Dir(_) => GetRes::IsDir, // 目录不是读面（类型冲突）
                FatPath::Noent => GetRes::Miss,
                FatPath::Bad => GetRes::Bad,
            };
        }
        let n11 = match name_to_fat11(name) {
            Some(n) => n,
            None => return GetRes::Miss, // RAM 名字空间（小写等）——非 FAT 面
        };
        if let Some((n, buf)) = self.staging.as_ref() {
            if *n == n11 {
                return self.serve_slice(buf, off, rep);
            }
        }
        if !self.sess_ok {
            return GetRes::Bad;
        }
        match self.find_root(&n11) {
            Some(DirKind::File(f)) => {
                if f.size as usize > FILE_SIZE_MAX {
                    return GetRes::Full;
                }
                self.serve(&f, off as usize, f.size as usize, rep)
            }
            // 单段 8.3 目录名：类型冲突（ST_BAD）而非 NOENT——fstree read
            // 的 IsDir 同义（刀 3）。
            Some(DirKind::Dir(_)) => GetRes::IsDir,
            _ => GetRes::Miss, // 未命中（Skip/End 不作名匹配面）
        }
    }

    /// FAT 名 PUT 分片入 staging 槽。None = 非 FAT 名（调用方回落 RAM 通道
    /// do_put）；跨名写先自动 commit 旧槽（当前 buf 长即终长——串行 client
    /// 一段重定向一个目标的节奏；commit 失败 = 旧槽保留 + 新写拒）。
    pub fn stage_put(&mut self, name: &[u8], off: u32, data: &[u8]) -> Option<StageRes> {
        let n11 = name_to_fat11(name)?;
        if matches!(self.find_root(&n11), Some(DirKind::Dir(_))) {
            // 盘上同名目录——文件写目标类型冲突（目录读面 v1 不可写；不拒
            // 会经 commit 的 End 槽路径写出目录/文件同名双项）。
            return Some(StageRes::Deny(ST_BAD));
        }
        let end = off as usize + data.len(); // u32 + ≤CHUNK：64 位下无溢出
        if end > FILE_SIZE_MAX {
            // 尺寸帽先于一切槽操作拒（不触碰既有 staging——拒绝语义干净）。
            return Some(StageRes::Deny(ST_FULL));
        }
        match self.staging.take() {
            Some((cur, buf)) if cur == n11 => {
                let mut buf = buf;
                if end > buf.len() {
                    buf.resize(end, 0); // off 超现长 = 零填充扩容（PUT 语义）
                }
                buf[off as usize..end].copy_from_slice(data);
                self.staging = Some((cur, buf));
                Some(StageRes::Staged)
            }
            Some((cur, buf)) => {
                let st = self.commit(cur, &buf);
                if st != berry_fsproto::ST_OK {
                    self.staging = Some((cur, buf)); // 旧槽保留，新写拒
                    return Some(StageRes::Deny(st));
                }
                self.staging = Some((n11, stage_buf(off, data, end)));
                Some(StageRes::Staged)
            }
            None => {
                self.staging = Some((n11, stage_buf(off, data, end)));
                Some(StageRes::Staged)
            }
        }
    }

    /// FAT 名 TRUNC = commit 触发点。None = 非 FAT 名（RAM 通道 do_trunc）。
    /// 同名 staged → resize 后落盘（len = 终长，`echo > F` 的截断/建空面）；
    /// 跨名 → 旧槽先落盘再处理目标；未 staged → 盘上现内容 apply 截断后
    /// 落盘（盘上无此名 = 建空文件——`echo > NEWF` 的 PUT 先行场景）。
    pub fn stage_trunc(&mut self, name: &[u8], size: u32) -> Option<u8> {
        let n11 = name_to_fat11(name)?;
        if matches!(self.find_root(&n11), Some(DirKind::Dir(_))) {
            return Some(ST_BAD); // 同名目录——stage_put 同面（类型冲突）
        }
        let size = size as usize;
        if size > FILE_SIZE_MAX {
            return Some(ST_FULL);
        }
        match self.staging.take() {
            Some((cur, buf)) if cur == n11 => {
                let mut buf = buf;
                buf.resize(size, 0); // 截断或零扩（TRUNC 语义，do_trunc 同形）
                let st = self.commit(cur, &buf);
                if st != berry_fsproto::ST_OK {
                    self.staging = Some((cur, buf)); // 落盘失败槽保留
                }
                Some(st)
            }
            Some((cur, buf)) => {
                let st = self.commit(cur, &buf);
                if st != berry_fsproto::ST_OK {
                    self.staging = Some((cur, buf));
                    return Some(st);
                }
                Some(self.stage_trunc_fresh(n11, size))
            }
            None => Some(self.stage_trunc_fresh(n11, size)),
        }
    }

    /// 未 staged 名的 TRUNC：盘上现文件读全 → 截断 → 落盘；盘上无此名 =
    /// 空内容落盘（建空文件）。盘上内容读不出（链断/RPC 失败）= 诚实拒
    /// ST_BAD——内容不可知时不盲写。
    fn stage_trunc_fresh(&mut self, n11: [u8; 11], size: usize) -> u8 {
        let mut buf = Vec::new();
        if let Some(f) = self.lookup11(&n11) {
            // 盘上目录项 size 帽（handle_get 同款守卫）：野值直读全量 =
            // Vec::with_capacity(f.size) 巨分配——berry-libc 静态堆耗尽
            // malloc null → abort 杀常驻服务。A-5 materialize 原有此帽，
            // A-6 换轨时丢了（对抗审查实抓回归）。
            if f.size as usize > FILE_SIZE_MAX {
                return ST_FULL;
            }
            match self.read_all(&f) {
                Some(mut d) => {
                    d.resize(size, 0);
                    buf = d;
                }
                None => {
                    return ST_BAD;
                }
            }
        }
        self.commit(n11, &buf)
    }

    /// staging 落盘（commit 点；序与 crash 窗口见模块头）。工作副本
    /// fat2/root2 全程不改 self——任一步失败内存态零变化（client 重 PUT
    /// 重试路径成立）。返回 ST_OK / ST_FULL（数据簇·根目录满）/ ST_BAD。
    fn commit(&mut self, n11: [u8; 11], data: &[u8]) -> u8 {
        if !self.sess_ok {
            return ST_BAD;
        }
        let csz = self.cluster_sz();
        let nclus = if data.is_empty() {
            0 // 空文件 first=0 无链（mkfat 空文件占簇惯例不采用——两端都解析）
        } else {
            (data.len() + csz - 1) / csz
        };
        let max = 2 + self.bpb.cluster_count;
        let mut fat2 = self.fat;
        let mut root2 = self.root;
        // ① 旧链 collect（覆写面；新建/空文件无链。环状 FAT 被 max 轮钉死，
        // 重复项释放幂等无害）。
        let old_first = self
            .lookup11(&n11)
            .map(|f| f.first_clus)
            .filter(|&c| c >= 2);
        let mut old_chain: Vec<u32> = Vec::new();
        if let Some(mut c) = old_first {
            for _ in 0..max {
                old_chain.push(c);
                match fat12_entry(&self.fat, c) {
                    Some(Fat12Entry::Next(v)) if (2..max).contains(&v) => c = v,
                    _ => break, // EOC 链尾（正常）/Free/Bad/畸形——collect 止
                }
            }
        }
        // ② 新链分配（旧链标记未释放 → 不重用旧簇；每簇先标 EOC 占位再回填
        // prev 链接；find 从 last+1 续扫 = 簇号升序确定性）。
        let mut chain: Vec<u32> = Vec::with_capacity(nclus);
        for _ in 0..nclus {
            let from = chain.last().map(|&c| c + 1).unwrap_or(2);
            let c = match find_free_cluster(&fat2, from, self.bpb.cluster_count) {
                Some(c) => c,
                None => return ST_FULL, // 数据簇耗尽——内存态未动
            };
            if fat12_set(&mut fat2, c, FAT12_EOC).is_none() {
                return ST_BAD; // 域内簇号必在 FAT_CAP 内——内部不变量破
            }
            if let Some(&prev) = chain.last() {
                if fat12_set(&mut fat2, prev, c).is_none() {
                    return ST_BAD;
                }
            }
            chain.push(c);
        }
        // ③ 数据扇区落盘（簇尾零填；空文件零簇零写）。
        for (i, &c) in chain.iter().enumerate() {
            let lba = self.clus_lba(c);
            for s in 0..self.bpb.spc as u64 {
                let src = i * csz + s as usize * SECTOR_SZ;
                let mut sec = [0u8; SECTOR_SZ];
                let take = data.len().saturating_sub(src).min(SECTOR_SZ);
                // take==0 = src 已越过数据尾（尾簇零填扇区）——零长切片也
                // 要求 start ≤ len，跳过拷贝（sec 预零）。
                if take > 0 {
                    sec[..take].copy_from_slice(&data[src..src + take]);
                }
                if !self.write_sector(lba + s, &sec) {
                    return ST_BAD;
                }
            }
        }
        // ④ 旧链释放（新链已占且数据已落——旧簇此后可再分配）。
        for &c in &old_chain {
            if fat12_set(&mut fat2, c, 0).is_none() {
                return ST_BAD;
            }
        }
        // ⑤ 双 FAT 落盘（FAT1 不读不核——mkfat 双副本同写惯例，v1 信任面）。
        let fat_base = self.bpb.hidden as u64 + self.bpb.fat_sector as u64;
        for k in 0..self.bpb.nfats as u64 {
            for i in 0..self.bpb.fat_sz as u64 {
                let o = i as usize * SECTOR_SZ;
                let mut sec = [0u8; SECTOR_SZ];
                sec.copy_from_slice(&fat2[o..o + SECTOR_SZ]);
                if !self.write_sector(fat_base + k * self.bpb.fat_sz as u64 + i, &sec) {
                    return ST_BAD;
                }
            }
        }
        // ⑥ 根目录项：同名覆写 → 首个 End·删除槽 → 无槽 = ST_FULL（写入
        // End 槽后其后者仍为 0x00——目录终结标记不丢；末槽无后继 = 目录
        // 边界即止，lookup 迭代同界）。
        let mut slot = None;
        for i in 0..self.bpb.root_entries as usize {
            let e = &self.root[i * 32..i * 32 + 32];
            if let Some(DirKind::File(f)) = dir_entry(e) {
                if &f.name == &n11 {
                    slot = Some(i);
                    break;
                }
            }
        }
        if slot.is_none() {
            for i in 0..self.bpb.root_entries as usize {
                let e = &self.root[i * 32..i * 32 + 32];
                if e[0] == 0x00 || e[0] == 0xE5 {
                    slot = Some(i);
                    break;
                }
            }
        }
        let slot = match slot {
            Some(s) => s,
            None => return ST_FULL, // 根目录满（16 项）
        };
        let first = chain.first().copied().unwrap_or(0);
        let mut e32 = [0u8; 32];
        if encode_file_entry(&n11, first, data.len() as u32, &mut e32).is_none() {
            trap(); // first 域内簇号 + 尺寸帽已检——内部不变量破，不静默
        }
        root2[slot * 32..slot * 32 + 32].copy_from_slice(&e32);
        // ⑥′ 根扇区落盘（整扇区写回——root2 该段的同扇区邻槽一并带上，
        // 内存副本与盘面唯一的落盘交汇点；此前只写 ③ 数据 ⑤ FAT，根目录
        // 缺写盘 = ⑦ 回读旧扇区必 mismatch，首版实踩）。
        let root_lba = self.bpb.hidden as u64
            + self.bpb.root_sector as u64
            + (slot * 32 / SECTOR_SZ) as u64;
        let o = slot * 32 / SECTOR_SZ * SECTOR_SZ;
        let mut sec = [0u8; SECTOR_SZ];
        sec.copy_from_slice(&root2[o..o + SECTOR_SZ]);
        if !self.write_sector(root_lba, &sec) {
            return ST_BAD;
        }
        // ⑦ 根扇区回读对账（写路径便宜一致性验——错位写立即显形 ST_BAD）。
        let root_lba = self.bpb.hidden as u64
            + self.bpb.root_sector as u64
            + (slot * 32 / SECTOR_SZ) as u64;
        let o = slot * 32 / SECTOR_SZ * SECTOR_SZ;
        let mut back = [0u8; SECTOR_SZ];
        if !self.read_sector(root_lba, &mut back) || back[..] != root2[o..o + SECTOR_SZ] {
            return ST_BAD;
        }
        // ⑧ 全过——安装内存态（此刻起读面见新文件；簇缓存失效——簇内容
        // 已变）。staging 槽的处置归调用方（commit 不知调用语境）。
        self.fat = fat2;
        self.root = root2;
        self.cache_clus = 0;
        berry_fsproto::ST_OK
    }

    /// LIST 续列：未被 RAM 影子的 FAT 根名（文件 + 目录，显示形）接在
    /// RAM 名后，LIST_MAX 同帽截断（截断可见，client 侧不误判穷尽）。
    /// 影子源 = RAM 根名全表**含目录**（C-0 刀 2 起 mkdir 可建 8.3 形目录
    /// 名——overlay 上层遮下层通则，LIST 去重不破）。
    /// `sep` = 续列分隔符：LIST 面 `b' '`（v1 同形）；READDIR 根面 NUL
    /// （C-1 刀 4——与调用方首段渲染的分隔符一致）。
    pub fn append_names(&self, ram_root: &[&[u8]], buf: &mut [u8], n0: usize, sep: u8) -> usize {
        let mut n = n0;
        for i in 0..self.bpb.root_entries as usize {
            let e = &self.root[i * 32..i * 32 + 32];
            let f = match dir_entry(e) {
                Some(DirKind::File(f)) => f,
                // 目录名同列（刀 3——FAT 卷挂 /：根名全量；显示形无区分，
                // STAT/LOOKUP 可查 TYPE_DIR）。
                Some(DirKind::Dir(d)) => d,
                Some(DirKind::End) => break,
                _ => continue, // 卷标/LFN/删除项/畸形项
            };
            let mut tmp = [0u8; 12]; // 8.3 显示名最长 12（8+1+3）
            let dn = match fat11_display(&f.name, &mut tmp) {
                Some(dn) => dn,
                None => continue,
            };
            let disp = &tmp[..dn];
            if ram_root.iter().any(|rn| *rn == disp) {
                continue; // RAM 副本（文件/目录）已影子
            }
            let need = disp.len() + if n > 0 { 1 } else { 0 };
            if n + need > LIST_MAX {
                break;
            }
            if n > 0 {
                buf[n] = sep;
                n += 1;
            }
            buf[n..n + disp.len()].copy_from_slice(disp);
            n += disp.len();
        }
        // 未落盘的 staging 名（盘上无此名才列——已 commit 的不重复；staged
        // 名恒 8.3 显示形，RAM 根目录名（含 8.3 形目录——刀 2 起新面）同样
        // 影子它：staging 撞 RAM 目录名时不双列）。
        if let Some((n11, _)) = self.staging.as_ref() {
            if self.lookup11(n11).is_none() {
                let mut tmp = [0u8; 12];
                if let Some(dn) = fat11_display(n11, &mut tmp) {
                    let disp = &tmp[..dn];
                    if !ram_root.iter().any(|rn| *rn == disp) {
                        let need = disp.len() + if n > 0 { 1 } else { 0 };
                        if n + need <= LIST_MAX {
                            if n > 0 {
                                buf[n] = b' ';
                                n += 1;
                            }
                            buf[n..n + disp.len()].copy_from_slice(disp);
                            n += disp.len();
                        }
                    }
                }
            }
        }
        n
    }

    /// v2 STAT/LOOKUP 回落面（C-0 刀 2 单段 / 刀 3 多段）：单段 = staging
    /// 优先于已 commit（读写一致性按最新内容计）+ 内存根镜像——零块 RPC，
    /// 会话毒化面天然不涉及（stat11 语义零漂移）；多段 = resolve 链走查
    /// （子项表在盘上——毒化/链断诚实 Bad）。目录回 TYPE_DIR（size 0）。
    pub fn stat(&mut self, path: &[u8]) -> FatStat {
        if !path.contains(&0) {
            let n11 = match name_to_fat11(path) {
                Some(n) => n,
                None => return FatStat::Noent,
            };
            if let Some((sn, buf)) = self.staging.as_ref() {
                if *sn == n11 {
                    return FatStat::Ok(TYPE_FILE, buf.len() as u32);
                }
            }
            return match self.find_root(&n11) {
                Some(DirKind::File(f)) => FatStat::Ok(TYPE_FILE, f.size),
                Some(DirKind::Dir(_)) => FatStat::Ok(TYPE_DIR, 0),
                // find_root 只产 File/Dir/None——Skip/End 是其内部迭代面，
                // 防御性兜底与未命中同义。
                _ => FatStat::Noent,
            };
        }
        match self.resolve(path) {
            FatPath::File(f) => FatStat::Ok(TYPE_FILE, f.size),
            FatPath::Dir(_) => FatStat::Ok(TYPE_DIR, 0),
            FatPath::Noent => FatStat::Noent,
            FatPath::Bad => FatStat::Bad,
        }
    }

    /// 根目录 8.3 名查找（类型感知——目录项也命中；End 即停；畸形项跳过）。
    /// [`Self::lookup11`] 的读面泛化（刀 3）；写面（commit 旧链 collect /
    /// staging 判定）仍用 lookup11 的 File-only 语义。
    fn find_root(&self, n11: &[u8; 11]) -> Option<DirKind> {
        for i in 0..self.bpb.root_entries as usize {
            let e = &self.root[i * 32..i * 32 + 32];
            match dir_entry(e) {
                Some(DirKind::File(f)) if &f.name == n11 => return Some(DirKind::File(f)),
                Some(DirKind::Dir(d)) if &d.name == n11 => return Some(DirKind::Dir(d)),
                Some(DirKind::End) => return None,
                _ => {}
            }
        }
        None
    }

    /// FAT 面多段路径走查（NUL 分段；C-0 刀 3 读面）：根（内存镜像）→ 逐
    /// 中段须目录（簇链逐簇载入扫子项表）→ 终段定类。中段文件 / 非 8.3 段
    /// / 未命中 = Noent（fstree mid_segment_file_unreachable 同义）；毒化 /
    /// RPC 失败 / 链断 = Bad。子项表走查复用单簇缓存（load_clus）——与
    /// serve 同一缓存面，顺序请求无重复 RPC。
    fn resolve(&mut self, path: &[u8]) -> FatPath {
        if !self.sess_ok {
            return FatPath::Bad; // 会话毒化——子项表在盘上，不走查就是编数据
        }
        let segs: Vec<&[u8]> = path.split(|&b| b == 0).collect();
        if segs.iter().any(|s| s.is_empty()) {
            return FatPath::Noent; // 防御面（parse 层 nlen/段帽在前）
        }
        let last = segs.len() - 1;
        let mut cur: Option<DirFile> = None; // 当前目录（None = 根）
        for (i, seg) in segs.iter().enumerate() {
            let n11 = match name_to_fat11(seg) {
                Some(n) => n,
                None => return FatPath::Noent, // 非 8.3 段 = RAM 名字空间
            };
            let kind = match cur {
                None => self.find_root(&n11),
                Some(d) => match self.scan_chain(&d, &n11) {
                    ChainScan::Found(k) => Some(k),
                    ChainScan::Missing => None,
                    ChainScan::Bad => return FatPath::Bad,
                },
            };
            match kind {
                Some(DirKind::Dir(d)) if i < last => cur = Some(d), // 中段目录：下行
                Some(DirKind::File(f)) if i == last => return FatPath::File(f),
                Some(DirKind::Dir(d)) if i == last => return FatPath::Dir(d),
                // 中段文件（不可达）/ 未命中 / 空目录作中段
                _ => return FatPath::Noent,
            }
        }
        FatPath::Noent // 不可达（segs 非空必进循环且必 return）——防御面
    }

    /// 目录子项表扫描：簇链逐簇载入（单簇缓存）找 8.3 名。空目录（首簇
    /// 0）= Missing；链长帽 DIR_CHAIN_MAX（环状 FAT 钉死）；EOC 表尽 =
    /// Missing；Free/Bad/域外 = Bad（链断）。
    fn scan_chain(&mut self, dir: &DirFile, n11: &[u8; 11]) -> ChainScan {
        let mut c = dir.first_clus;
        if c == 0 {
            return ChainScan::Missing; // 空目录（解码合法形）
        }
        let csz = self.cluster_sz();
        let max = 2 + self.bpb.cluster_count;
        if !(2..max).contains(&c) {
            return ChainScan::Bad;
        }
        for _ in 0..DIR_CHAIN_MAX {
            if !self.load_clus(c) {
                return ChainScan::Bad;
            }
            for i in 0..csz / 32 {
                let e = &self.cache[i * 32..i * 32 + 32];
                match dir_entry(e) {
                    Some(DirKind::File(f)) if &f.name == n11 => {
                        return ChainScan::Found(DirKind::File(f))
                    }
                    Some(DirKind::Dir(d)) if &d.name == n11 => {
                        return ChainScan::Found(DirKind::Dir(d))
                    }
                    Some(DirKind::End) => return ChainScan::Missing, // 表终结
                    _ => {}
                }
            }
            match fat12_entry(&self.fat, c) {
                Some(Fat12Entry::Next(v)) if (2..max).contains(&v) => c = v, // 下一簇
                Some(Fat12Entry::Eoc) => return ChainScan::Missing, // 链尾表尽
                _ => return ChainScan::Bad, // Free/Bad/保留值/域外——链断
            }
        }
        ChainScan::Missing // 链长越帽——按未中收（确定性优先）
    }

    /// v2 READDIR 的 FAT 回落（刀 3）：路径解析为 FAT 目录 → 子项显示名
    /// NUL 连接落 buf（fstree readdir 同形、LIST_MAX 同帽截断——截断可
    /// 见）；文件 = NotDir；未命中 = Noent；毒化/RPC/链断 = Bad。
    pub fn readdir(&mut self, path: &[u8], buf: &mut [u8]) -> FatList {
        let d = match self.resolve(path) {
            FatPath::Dir(d) => d,
            FatPath::File(_) => return FatList::NotDir,
            FatPath::Noent => return FatList::Noent,
            FatPath::Bad => return FatList::Bad,
        };
        if d.first_clus == 0 {
            return FatList::Names(0); // 空目录
        }
        let csz = self.cluster_sz();
        let max = 2 + self.bpb.cluster_count;
        let cap = LIST_MAX.min(buf.len());
        let mut n = 0usize;
        let mut c = d.first_clus;
        for _ in 0..DIR_CHAIN_MAX {
            if !self.load_clus(c) {
                return FatList::Bad;
            }
            let mut done = false;
            for i in 0..csz / 32 {
                let e = &self.cache[i * 32..i * 32 + 32];
                let f = match dir_entry(e) {
                    Some(DirKind::File(f)) => f,
                    Some(DirKind::Dir(d)) => d, // 子目录名同列（名字面无区分）
                    Some(DirKind::End) => {
                        done = true;
                        break;
                    }
                    _ => continue, // 卷标/LFN/删除项/畸形项
                };
                let mut tmp = [0u8; 12]; // 8.3 显示名最长 12（8+1+3）
                let dn = match fat11_display(&f.name, &mut tmp) {
                    Some(dn) => dn,
                    None => continue,
                };
                let disp = &tmp[..dn];
                let need = disp.len() + if n > 0 { 1 } else { 0 };
                if n + need > cap {
                    done = true; // 帽截断（可见——client 不误判穷尽）
                    break;
                }
                if n > 0 {
                    buf[n] = 0; // NUL 分隔（fstree readdir 同形）
                    n += 1;
                }
                buf[n..n + disp.len()].copy_from_slice(disp);
                n += disp.len();
            }
            if done {
                break;
            }
            match fat12_entry(&self.fat, c) {
                Some(Fat12Entry::Next(v)) if (2..max).contains(&v) => c = v,
                Some(Fat12Entry::Eoc) => break, // 链尾表尽
                _ => return FatList::Bad, // Free/Bad/域外——链断
            }
        }
        FatList::Names(n)
    }

    /// 根目录 8.3 名查找（End 即停；畸形项跳过）。
    fn lookup11(&self, n11: &[u8; 11]) -> Option<DirFile> {
        for i in 0..self.bpb.root_entries as usize {
            let e = &self.root[i * 32..i * 32 + 32];
            match dir_entry(e) {
                Some(DirKind::File(f)) if &f.name == n11 => return Some(f),
                Some(DirKind::End) => return None,
                _ => {}
            }
        }
        None
    }

    /// off 起一片：协议窗（CHUNK）、文件尾、簇界三取小；簇不在缓存则整簇
    /// 读入（单簇缓存给顺序读局部性）。编码 None（算术出界）= 服务端 bug
    /// → trap（不静默）。
    fn serve(&mut self, f: &DirFile, off: usize, size: usize, rep: &mut [u8; 256]) -> GetRes {
        if off >= size {
            return match encode_reply_get(rep, off as u32, &[], false) {
                Some(n) => GetRes::Served(n),
                None => trap(),
            };
        }
        let csz = self.cluster_sz();
        let clus = match self.nth_clus(f.first_clus, (off / csz) as u32) {
            Some(c) => c,
            None => return GetRes::Bad, // 链断（EOC 早到/坏簇/簇号域外）
        };
        if !self.load_clus(clus) {
            return GetRes::Bad;
        }
        let in_off = off % csz;
        let end = (off + CHUNK).min(size).min(off - in_off + csz);
        let more = end < size;
        let d = &self.cache[in_off..in_off + (end - off)];
        match encode_reply_get(rep, off as u32, d, more) {
            Some(n) => GetRes::Served(n),
            None => trap(),
        }
    }

    /// staging GET 切片（RAM handle_get 同语义：off ≥ len → 空片收尾；
    /// 否则 ≤CHUNK 窗 + more。服务端不变量 more=true ⇒ dlen ≥ 1）。
    /// 编码 None（算术出界）= 服务端 bug → trap（不静默）。
    fn serve_slice(&self, buf: &[u8], off: u32, rep: &mut [u8; 256]) -> GetRes {
        let off = off as usize;
        let (slice, more) = if off >= buf.len() {
            (&buf[buf.len()..], false) // 空片收尾
        } else {
            let end = (off + CHUNK).min(buf.len());
            (&buf[off..end], end < buf.len())
        };
        match encode_reply_get(rep, off as u32, slice, more) {
            Some(n) => GetRes::Served(n),
            None => trap(),
        }
    }

    /// 整文件读出（stage_trunc 截断面用；簇界步进——与 serve 同几何）。
    fn read_all(&mut self, f: &DirFile) -> Option<Vec<u8>> {
        let size = f.size as usize;
        let csz = self.cluster_sz();
        let mut out = Vec::with_capacity(size);
        let mut off = 0usize;
        while off < size {
            let clus = self.nth_clus(f.first_clus, (off / csz) as u32)?;
            if !self.load_clus(clus) {
                return None;
            }
            let in_off = off % csz;
            let end = (off - in_off + csz).min(size); // 簇界或文件尾
            out.extend_from_slice(&self.cache[in_off..in_off + (end - off)]);
            off = end;
        }
        Some(out)
    }

    /// 链走 n 步（n ≤ FILE_SIZE_MAX/最小簇 = 有界；环状 FAT 也被步数钉死）。
    fn nth_clus(&self, first: u32, n: u32) -> Option<u32> {
        let max = 2 + self.bpb.cluster_count;
        if !(2..max).contains(&first) {
            return None;
        }
        let mut c = first;
        for _ in 0..n {
            c = match fat12_entry(&self.fat, c) {
                Some(Fat12Entry::Next(v)) if (2..max).contains(&v) => v,
                _ => return None, // EOC 早到/Free/Bad/保留值/切片界外
            };
        }
        Some(c)
    }

    /// 整簇入缓存（spc 个扇区顺序 RPC；命中即免）。
    fn load_clus(&mut self, clus: u32) -> bool {
        if self.cache_clus == clus {
            return true;
        }
        let max = 2 + self.bpb.cluster_count;
        if !(2..max).contains(&clus) {
            return false;
        }
        let first = self.clus_lba(clus);
        let mut sec = [0u8; SECTOR_SZ];
        for i in 0..self.bpb.spc as u64 {
            if !self.read_sector(first + i, &mut sec) {
                return false;
            }
            let o = i as usize * SECTOR_SZ;
            self.cache[o..o + SECTOR_SZ].copy_from_slice(&sec);
        }
        self.cache_clus = clus;
        true
    }

    /// 经 vblk 读一个扇区（Poison = 会话毒化——此后 FAT 面确定性 ST_BAD）。
    fn read_sector(&mut self, lba: u64, out: &mut [u8; SECTOR_SZ]) -> bool {
        if !self.sess_ok {
            return false;
        }
        match rpc_read(self.h, self.vmo, lba, out) {
            RpcRes::Ok => true,
            RpcRes::Fail => false, // 干净失败——会话仍同步，不毒化
            RpcRes::Poison => {
                self.sess_ok = false;
                println!("fsmgr: fat rpc lost"); // loud 一次（此后 ST_BAD 降级）
                false
            }
        }
    }

    fn cluster_sz(&self) -> usize {
        self.bpb.spc as usize * SECTOR_SZ
    }

    /// 经 vblk 写一个扇区（毒化语义同 read_sector：Poison → sess_ok=false
    /// + loud 一行，此后 FAT 写面确定性 ST_BAD）。
    fn write_sector(&mut self, lba: u64, src: &[u8; SECTOR_SZ]) -> bool {
        if !self.sess_ok {
            return false;
        }
        match rpc_write(self.h, self.vmo, lba, src) {
            RpcRes::Ok => true,
            RpcRes::Fail => false, // 干净失败——会话仍同步，不毒化
            RpcRes::Poison => {
                self.sess_ok = false;
                println!("fsmgr: fat rpc lost"); // loud 一次（此后 ST_BAD 降级）
                false
            }
        }
    }

    /// 簇号 → 盘绝对 LBA（hidden 前缀 + 卷 data 区 + 簇偏；load_clus 同式
    /// 收敛于此——commit 数据落盘与簇读单一折算点）。
    fn clus_lba(&self, clus: u32) -> u64 {
        self.bpb.hidden as u64
            + self.bpb.data_sector as u64
            + (clus as u64 - 2) * self.bpb.spc as u64
    }
}

/// 新 staging 槽内容（off 起写 data，前段零填）。
fn stage_buf(off: u32, data: &[u8], end: usize) -> Vec<u8> {
    let mut buf = vec![0u8; end];
    buf[off as usize..end].copy_from_slice(data);
    buf
}

/// now = clock_read（失败 trap——时钟是启动关键原语，main 同款）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}
