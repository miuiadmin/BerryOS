//! fsmgr —— P6 A-1 RAM-fs 服务 + A-5/A-6 FAT 读写层（`docs/文件系统设计.md`
//! §6 P6 注记：tmpfs before disk——A-5 起读、A-6 起写）。
//!
//! stdhello/bsh 同链（musl target + berry-libc shim）。通道拓扑（svcmgr
//! P6 段搭）：本服务持 env 块 gift[2]（libc `gift2_h()`——fs 服务端端
//! fs_b）+ gift[3]（libc `blk_h()`——A-5 起 blk 服务端端 blk_a，vblk 经
//! svcmgr 预拉+自验门后交付）；bsh 持对端 fs_a 的 SEND|RECV dup（gift[3]，
//! libc `fs_h()`）发请求。协议 = berry-fsproto（PUT/TRUNC/GET/LIST + v2
//! 五 op LOOKUP/MKDIR/UNLINK/READDIR/STAT，**严格串行 RPC**——一请求一
//! 应答，队列深 1 用量，无乱序死锁面）；存储 = berry-fstree 目录树
//! （RAM-fs——C-0 刀 2 起多段 NUL 路径，单段名 v1 语义零漂移；
//! FILE_COUNT_MAX=8 × FILE_SIZE_MAX=16KiB + DIR_COUNT_MAX=16 静态堆
//! 预算内）+ **FAT12 读写层**（`fat.rs`——A-6 起 8.3 大写名 staging +
//! TRUNC 即 commit 真写扇区，小写名/多段名走 RAM 树；分层路由与 commit
//! 序见该模块头；v2 五 op 分层（C-0 刀 3 起）：LOOKUP/STAT 未命中回落
//! FAT——单段 8.3 根面 + 多段子目录链（读面），READDIR 未命中回落 FAT
//! 目录，GET 多段同走子目录链；MKDIR/UNLINK 纯 RAM 面——FAT 建删目录
//! 不在 C-0 盒、FAT 文件删除留 v2、`>` 只落根）。
//!
//! 服务循环 = echo 同形但 **wait 超时重臂**（关键差异）：echo 的 client
//! （svcmgr）每节流拍必 ping——10s 帽到点 = 断链，trap；本服务的 client
//! （bsh）是交互节奏，长空闲合法——超时重臂续睡（10s 帽续命循环，睡等
//! 不烧 CPU；真断链面 = recv 非 -EAGAIN 错码，照 trap）。
//!
//! **静默无输出**（服务不吵）：诊断走应答 st 码（ST_NOENT/ST_FULL/
//! ST_BAD/ST_EXISTS/ST_NOEMPTY）；例外 = FAT 层 init 失败/会话毒化各打一
//! 行（链断级事件，
//! `FAT ok` 缺失是 smoke 面，原因先行）。链断 → trap 只死自己（`FS ok`
//! 缺失 = smoke FAIL 兜底）。stdin 是 svcmgr 占位 dummy（空 feeder + EOF
//! ——gift[2] 位前的占位项，见 berry-exec gift 注），留队无害、永不读。

mod fat;

use berry_libc::io::{blk_h, gift2_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_object_wait_one, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_fsproto::{
    encode_reply_get, encode_reply_list, encode_reply_stat, encode_reply_status, parse_request,
    Req, ST_BAD, TYPE_DIR,
};
use berry_fstree::{ReadDirRes, ReadRes, StatRes, Tree};

use fat::{Fat, FatList, FatStat, GetRes};

// 链接种子（stdhello/bsh 同款）：rlib/LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// 空闲重臂帽：wait READABLE 超时 = 重臂续睡（交互 client 长空闲合法），
/// 非死亡面（echo 的 trap-on-timeout 反例见模块头）。
const TIMEOUT_NS: u64 = 10 * 1_000_000_000;

fn main() {
    let h = gift2_h();
    if h == 0 {
        // env 块无 gift[2] = svcmgr 没走 P6 拓扑（旧归档）——服务无端可听，
        // park 挂起（标记缺失 loud 兜底，不 trap 污染 dump）。
        berry_libc::park();
    }
    let mut tree = Tree::new();
    // A-5：FAT 只读层 eager init（vblk 由 svcmgr 预拉且自验已过，此刻必在
    // 服务——FAT init 的首读不会撞服务未起）；blk_h()==0 = 无块设备面，
    // RAM-only 降级续走（init 失败同理，诊断行见 fat.rs）。
    let mut fat: Option<Fat> = fat::init(blk_h());
    let mut msg = [0u8; 256];
    let mut rep = [0u8; 256];
    loop {
        let mut nb = msg.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s != 0 {
            if s == BERRY_ERR_AGAIN {
                // 空队：阻塞等 READABLE（10s 帽，超时**重臂**——空闲合法面）。
                let mut obs = 0u64;
                let _ = berry_object_wait_one(h, SIGNAL_READABLE, now() + TIMEOUT_NS, &mut obs);
                continue;
            }
            trap(); // 非 -EAGAIN 错码 = 通道链断（真死亡面，不重臂）
        }
        let rep_len: usize = match parse_request(&msg[..nb]) {
            Some(Req::Put { path: name, off, data }) => {
                // A-6 路由：8.3 大写名 → FAT 层 staging（TRUNC 点落盘）；其余
                // （小写/多段）RAM 树不变。and_then 展平——无 FAT 层（fat==None，
                // RAM-only 降级）与非 FAT 名（stage_put 返 None——多段 NUL 串
                // 构造性落此）同走 RAM 通道。
                let st = match fat.as_mut().and_then(|f| f.stage_put(name, off, data)) {
                    None => tree.put(name, off, data),
                    Some(fat::StageRes::Staged) => berry_fsproto::ST_OK,
                    Some(fat::StageRes::Deny(st)) => st,
                };
                encode_reply_status(&mut rep, st).unwrap_or_else(|| trap())
            }
            Some(Req::Trunc { path: name, size }) => {
                // TRUNC = FAT 名的 commit 触发点（bsh fs_put 尾随 TRUNC 恰合：
                // 分片 PUT 累积 → TRUNC(len) 落盘）；RAM 名照旧。
                let st = match fat.as_mut().and_then(|f| f.stage_trunc(name, size)) {
                    None => tree.trunc(name, size),
                    Some(st) => st,
                };
                encode_reply_status(&mut rep, st).unwrap_or_else(|| trap())
            }
            Some(Req::Get { path: name, off }) => match tree.read(name, off) {
                // 服务端不变量（fstree read 构造保证）：more=true ⇒ dlen ≥ 1；
                // 编码 None（算术出界）= 服务端 bug → trap（不静默）。
                ReadRes::Win(slice, more) => {
                    encode_reply_get(&mut rep, off, slice, more).unwrap_or_else(|| trap())
                }
                // RAM Miss → FAT 回落（v1 流不变——单段 8.3 名由 name_to_fat11
                // 门自筛进 FAT；刀 3 起多段 NUL 串走 FAT 子目录链读面）。
                ReadRes::Miss => match fat.as_mut().map(|f| f.handle_get(name, off, &mut rep)) {
                    Some(GetRes::Served(n)) => n, // rep 已编码（handle_get 内部落字）
                    Some(GetRes::Full) => encode_reply_status(&mut rep, berry_fsproto::ST_FULL)
                        .unwrap_or_else(|| trap()),
                    Some(GetRes::Bad) => {
                        encode_reply_status(&mut rep, ST_BAD).unwrap_or_else(|| trap())
                    }
                    // FAT 面命中的是目录（单段目录名/多段终段目录）——与
                    // RAM IsDir 同义（刀 3）。
                    Some(GetRes::IsDir) => {
                        encode_reply_status(&mut rep, ST_BAD).unwrap_or_else(|| trap())
                    }
                    // RAM/FAT 双未命中（或 FAT 层缺席）——NOENT。
                    Some(GetRes::Miss) | None => encode_reply_status(
                        &mut rep,
                        berry_fsproto::ST_NOENT,
                    )
                    .unwrap_or_else(|| trap()),
                },
                // 目录不是读面（类型冲突，协议头 v2 注）。
                ReadRes::IsDir => encode_reply_status(&mut rep, ST_BAD).unwrap_or_else(|| trap()),
            },
            Some(Req::List) => {
                // 名列暂存走独立 scratch（msg 的借用跨整个 match 存活）；
                // FAT 未影子名续列（LIST_MAX 同帽——影子源 = 根名全表**含
                // 目录**，overlay 上层遮下层：RAM 目录名同影子 FAT 同名项）。
                let mut names = [0u8; 256];
                let n = tree.list_root(&mut names);
                let n = match fat.as_ref() {
                    Some(f) => f.append_names(&tree.root_names(), &mut names, n, b' '),
                    None => n,
                };
                encode_reply_list(&mut rep, &names[..n]).unwrap_or_else(|| trap())
            }
            // v2 五 op（C-0 刀 2 服务面接树 / 刀 3 FAT 子目录读面）：
            // LOOKUP/STAT 的 RAM Miss 回落 FAT——单段 8.3 面（内存镜像）+
            // 多段子目录链（刀 3，盘上子项表）；MKDIR/UNLINK 纯 RAM 面
            // （FAT 建删目录不在 C-0 盒、FAT 文件删除留 v2）；READDIR 的
            // RAM Miss 回落 FAT 目录（刀 3）。
            Some(Req::Lookup { path: name }) => {
                // 根（空 wire，C-1 刀 5 立法）：RAM 根构造性存在——恒 OK
                //（libc `chdir` 的根定向探针走 STAT，此面供 LOOKUP 根问询）。
                let st = if name.is_empty() {
                    berry_fsproto::ST_OK
                } else {
                    match tree.stat(name) {
                        StatRes::Ok(..) => berry_fsproto::ST_OK,
                        StatRes::Noent => match fat.as_mut().map(|f| f.stat(name)) {
                            Some(FatStat::Ok(..)) => berry_fsproto::ST_OK,
                            Some(FatStat::Bad) => ST_BAD,
                            _ => berry_fsproto::ST_NOENT,
                        },
                    }
                };
                encode_reply_status(&mut rep, st).unwrap_or_else(|| trap())
            }
            Some(Req::Mkdir { path: name }) => {
                encode_reply_status(&mut rep, tree.mkdir(name)).unwrap_or_else(|| trap())
            }
            Some(Req::Unlink { path: name }) => {
                encode_reply_status(&mut rep, tree.unlink(name)).unwrap_or_else(|| trap())
            }
            Some(Req::Readdir { path: name }) => {
                let mut names = [0u8; 256];
                // C-1 刀 4：根（空路径）= overlay 全景——RAM 根 + FAT 未影子
                // 名续列（与 LIST 同源同序，分隔 NUL = READDIR blob 形）。
                // bsh 裸 `ls` 迁 libc opendir("/") 后 READDIR 是根列名的
                // 唯一活路径（LIST 无客户端留守——协议冻结不删）；fstree
                // resolve 把空段当名查 → 根恒 Noent，故根并合在此执法
                // 而非树层（刀 5 起空 wire 立法进 fsproto，STAT/LOOKUP
                // 同门）。
                if name.is_empty() {
                    let n = root_overlay(&tree, &fat, &mut names);
                    encode_reply_list(&mut rep, &names[..n]).unwrap_or_else(|| trap())
                } else {
                    match tree.readdir(name, &mut names) {
                        // NUL 连接子名（READDIR 应答 blob = LIST 同形）。
                        ReadDirRes::Names(n) => {
                            encode_reply_list(&mut rep, &names[..n]).unwrap_or_else(|| trap())
                        }
                        // RAM 未命中 → FAT 目录回落（刀 3：FAT 卷挂 / 的子目录
                        // 读面；RAM 同名项在树上时恒走上面 Names 臂——overlay
                        // 上层遮下层，目录内容不并合）。
                        ReadDirRes::Noent => {
                            match fat.as_mut().map(|f| f.readdir(name, &mut names)) {
                                Some(FatList::Names(n)) => {
                                    encode_reply_list(&mut rep, &names[..n])
                                        .unwrap_or_else(|| trap())
                                }
                                Some(FatList::NotDir) | Some(FatList::Bad) => {
                                    encode_reply_status(&mut rep, ST_BAD).unwrap_or_else(|| trap())
                                }
                                Some(FatList::Noent) | None => encode_reply_status(
                                    &mut rep,
                                    berry_fsproto::ST_NOENT,
                                )
                                .unwrap_or_else(|| trap()),
                            }
                        }
                        ReadDirRes::NotDir => {
                            encode_reply_status(&mut rep, ST_BAD).unwrap_or_else(|| trap())
                        }
                    }
                }
            }
            Some(Req::Stat { path: name }) => {
                // 根（空 wire，C-1 刀 5 立法）：(TYPE_DIR, overlay 全景项数)
                // ——size 口径与子目录 STAT 同义（子项数），项数与 READDIR
                // 根并合同源（root_overlay 单一实现）。fstree resolve 把空
                // wire 当名查 → 根恒 Noent，故根语义在分派层执法。
                if name.is_empty() {
                    let mut names = [0u8; 256];
                    let n = root_overlay(&tree, &fat, &mut names);
                    encode_reply_stat(&mut rep, TYPE_DIR, nul_count(&names[..n]))
                        .unwrap_or_else(|| trap())
                } else {
                    match tree.stat(name) {
                        StatRes::Ok(ty, size) => {
                            encode_reply_stat(&mut rep, ty, size).unwrap_or_else(|| trap())
                        }
                        // FAT 回落面：文件 (TYPE_FILE, size) / 目录 (TYPE_DIR, 0)
                        // （刀 3 起含子目录链；毒化/链断诚实 ST_BAD）。
                        StatRes::Noent => match fat.as_mut().map(|f| f.stat(name)) {
                            Some(FatStat::Ok(ty, sz)) => {
                                encode_reply_stat(&mut rep, ty, sz).unwrap_or_else(|| trap())
                            }
                            Some(FatStat::Bad) => {
                                encode_reply_status(&mut rep, ST_BAD).unwrap_or_else(|| trap())
                            }
                            _ => encode_reply_status(&mut rep, berry_fsproto::ST_NOENT)
                                .unwrap_or_else(|| trap()),
                        },
                    }
                }
            }
            None => encode_reply_status(&mut rep, ST_BAD).unwrap_or_else(|| trap()),
        };
        send_reply(h, &rep, rep_len);
    }
}

/// 根 overlay 名列（NUL 连接）：RAM 根全列 + FAT 未影子名续列——
/// READDIR 根并合（刀 4）与 STAT 根项数（刀 5）的**单一实现**（同源
/// 同序，两应答面不各自演化）。
fn root_overlay(tree: &Tree, fat: &Option<Fat>, names: &mut [u8; 256]) -> usize {
    let n = tree.list_root_nul(names);
    match fat.as_ref() {
        Some(f) => f.append_names(&tree.root_names(), names, n, 0),
        None => n,
    }
}

/// NUL 连接 blob 的段计数（STAT 根 size 口径 = 子项数；空 blob = 0）。
fn nul_count(blob: &[u8]) -> u32 {
    if blob.is_empty() {
        0
    } else {
        1 + blob.iter().filter(|&&b| b == 0).count() as u32
    }
}

/// 应答发送（EBUSY → wait WRITABLE 10s 重臂——严格串行 RPC 下应答队列
/// 恒空位（bsh 发请求后必收），此路是纪律性防御；其余错码 trap）。
fn send_reply(h: Handle, rep: &[u8; 256], len: usize) {
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, rep.as_ptr(), len, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            trap();
        }
        if deadline == 0 {
            deadline = now() + TIMEOUT_NS;
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs);
        deadline = 0; // 重臂（超时也重试——服务端不死面只有链断 trap）
    }
}

/// now = clock_read（失败 trap——时钟是启动关键原语）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （svcmgr/bsh 同款——链断 loud，`FS ok` 缺失 = smoke FAIL 兜底；
/// fat.rs 经 `crate::trap` 共用）。
#[cold]
pub(crate) fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（0x4000_0000 与用户布局无交集——家族惯例值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}
