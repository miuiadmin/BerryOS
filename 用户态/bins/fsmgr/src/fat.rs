//! fat.rs —— P6 A-5 FAT12 只读层（消费 `berry-fatfs` 纯解析 + vblk 块服务）。
//!
//! 拓扑（svcmgr A-5 预拉序）：vblk 由 svcmgr **先于本服务**拉起且自验已过
//! （BLKD 门），blk 服务端端 blk_a 经本服务的 env 块 gift[3] 交付（libc
//! `blk_h()`）——本层是 vblk 的**唯一 client**（blkproto 严格串行 RPC，无
//! 回复抢答面）。eager init：服务启动即定几何、整读 FAT0 + 根目录（≈6 扇
//! 区）；此后数据 GET 按簇读盘（单簇缓存，顺序读零重复 RPC）。
//!
//! 覆盖层（overlay）语义：RAM 表 = 写通道（do_put/do_trunc 不变），FAT =
//! 只读底座——GET 先 RAM 后 FAT；PUT/TRUNC 命中 FAT-only 名先**物化**整文
//! 件进 RAM（此后 RAM 副本影子 FAT 项，`>` 覆盖语义成立）；LIST = RAM 名
//! + 未被影子的 FAT 名。名字空间按构造隔离（`name_to_fat11` 只收 [A-Z0-9]
//! 8.3 形）：RAM 小写名（t64.bin 等）永不落 FAT 面，反向同然。
//!
//! 会话纪律（bsh fs_rpc 同款）：**收侧**超时/错码 = 会话毒化（迟到应答会
//! 错配下一请求 → 静默错数据；确定性降级 ST_BAD loud 优于无告警错数据）；
//! 发送侧失败不毒化（请求未离表）。卷定位 = 探测 {LBA0, LBA16} + BPB
//! `hidden` 一致性对账（mkfat 卷恒 @16、hidden=16；LBA0 保留区按构造无
//! 55AA 签名，parse_bpb 必拒）。

use berry_blkproto::{encode_request, parse_reply, OP_READ, SECTOR_SZ, ST_OK};
use berry_fatfs::{
    dir_entry, fat11_display, fat12_entry, name_to_fat11, parse_bpb, Bpb, DirFile, DirKind,
    Fat12Entry,
};
use berry_fsproto::{
    encode_reply_get, CHUNK, FILE_COUNT_MAX, FILE_SIZE_MAX, LIST_MAX, ST_BAD, ST_FULL,
};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_one, berry_vmo_commit, berry_vmo_create,
    berry_vmo_read, Handle, RIGHT_READ, RIGHT_WRITE, SIGNAL_READABLE, SIGNAL_WRITABLE,
    BERRY_ERR_BUSY, CLOCK_MONOTONIC,
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

/// GET 兜底结果（RAM 未命中后的 FAT 面三态 + 命中）。
pub enum GetRes {
    /// 命中（rep 已落字，值为应答长）。
    Served(usize),
    /// 不在 FAT（含非 FAT 名字空间）——调用方回落 ST_NOENT。
    Miss,
    /// 文件在盘上但超出协议文件尺寸帽（ST_FULL 诚实降级 ≠ NOENT）。
    Full,
    /// 服务面坏（会话毒化 / 链断 / RPC 失败）。
    Bad,
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

impl Fat {
    /// GET 兜底：FAT 名查找 + 簇界分片服务（chunk = 协议窗、文件尾、簇界
    /// 三取小——dlen < CHUNK 合法，more 链由 client 侧 off 前进收敛）。
    pub fn handle_get(&mut self, name: &[u8], off: u32, rep: &mut [u8; 256]) -> GetRes {
        if !self.sess_ok {
            return GetRes::Bad;
        }
        let n11 = match name_to_fat11(name) {
            Some(n) => n,
            None => return GetRes::Miss, // RAM 名字空间（小写等）——非 FAT 面
        };
        let f = match self.lookup11(&n11) {
            Some(f) => f,
            None => return GetRes::Miss,
        };
        if f.size as usize > FILE_SIZE_MAX {
            return GetRes::Full;
        }
        self.serve(&f, off as usize, f.size as usize, rep)
    }

    /// PUT/TRUNC 前置：FAT-only 名先物化整文件进 RAM（RAM = 写通道，此后
    /// RAM 副本影子 FAT 项）。None = 可继续正常 do_put/do_trunc（已在 RAM
    /// / 非 FAT 名 / 盘上无此名）；Some(st) = 拒（st 直接回 client——物化
    /// 不了还硬走会静默建空文件影子真数据，诚实拒）。
    pub fn materialize(&mut self, files: &mut Vec<(Vec<u8>, Vec<u8>)>, name: &[u8]) -> Option<u8> {
        if files.iter().any(|(n, _)| n.as_slice() == name) {
            return None;
        }
        let on_disk = name_to_fat11(name).and_then(|n11| self.lookup11(&n11));
        if !self.sess_ok {
            // 会话毒化但名字在盘上——内容不可知，拒；不在盘上 → 正常新建。
            return match on_disk {
                Some(_) => Some(ST_BAD),
                None => None,
            };
        }
        let f = on_disk?;
        if f.size as usize > FILE_SIZE_MAX || files.len() >= FILE_COUNT_MAX {
            return Some(ST_FULL);
        }
        let data = match self.read_all(&f) {
            Some(d) => d,
            None => return Some(ST_BAD), // 链断/RPC 失败
        };
        files.push((name.to_vec(), data));
        None
    }

    /// LIST 续列：未被 RAM 影子的 FAT 文件名（显示形）接在 RAM 名后，
    /// LIST_MAX 同帽截断（截断可见，client 侧不误判穷尽）。
    pub fn append_names(&self, files: &[(Vec<u8>, Vec<u8>)], buf: &mut [u8], n0: usize) -> usize {
        let mut n = n0;
        for i in 0..self.bpb.root_entries as usize {
            let e = &self.root[i * 32..i * 32 + 32];
            let f = match dir_entry(e) {
                Some(DirKind::File(f)) => f,
                Some(DirKind::End) => break,
                _ => continue, // 卷标/LFN/删除项/畸形项
            };
            let mut tmp = [0u8; 12]; // 8.3 显示名最长 12（8+1+3）
            let dn = match fat11_display(&f.name, &mut tmp) {
                Some(dn) => dn,
                None => continue,
            };
            let disp = &tmp[..dn];
            if files.iter().any(|(rn, _)| rn.as_slice() == disp) {
                continue; // RAM 副本已影子
            }
            let need = disp.len() + if n > 0 { 1 } else { 0 };
            if n + need > LIST_MAX {
                break;
            }
            if n > 0 {
                buf[n] = b' ';
                n += 1;
            }
            buf[n..n + disp.len()].copy_from_slice(disp);
            n += disp.len();
        }
        n
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

    /// 整文件读出（物化用；簇界步进——与 serve 同几何，无 CHUNK 限）。
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
        let first = self.bpb.hidden as u64
            + self.bpb.data_sector as u64
            + (clus as u64 - 2) * self.bpb.spc as u64;
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
}

/// now = clock_read（失败 trap——时钟是启动关键原语，main 同款）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}
