//! fsmgr —— P6 A-1 RAM-fs 服务 + A-5 FAT 只读层（`文档/文件系统设计.md`
//! §6 P6 注记：tmpfs before disk——A-5 起 disk 到位）。
//!
//! stdhello/bsh 同链（musl target + berry-libc shim）。通道拓扑（svcmgr
//! P6 段搭）：本服务持 env 块 gift[2]（libc `gift2_h()`——fs 服务端端
//! fs_b）+ gift[3]（libc `blk_h()`——A-5 起 blk 服务端端 blk_a，vblk 经
//! svcmgr 预拉+自验门后交付）；bsh 持对端 fs_a 的 SEND|RECV dup（gift[3]，
//! libc `fs_h()`）发请求。协议 = berry-fsproto（PUT/TRUNC/GET/LIST，
//! **严格串行 RPC**——一请求一应答，队列深 1 用量，无乱序死锁面）；
//! 存储 = `Vec<(名, 内容)>` 线性表（RAM-fs：FILE_COUNT_MAX=8 ×
//! FILE_SIZE_MAX=16KiB = 128KiB < libc 静态堆 256KiB）+ **FAT12 只读底座**
//! （`fat.rs`——GET 先 RAM 后 FAT、PUT/TRUNC 物化、LIST 合并，覆盖层语义
//! 见该模块头）。
//!
//! 服务循环 = echo 同形但 **wait 超时重臂**（关键差异）：echo 的 client
//! （svcmgr）每节流拍必 ping——10s 帽到点 = 断链，trap；本服务的 client
//! （bsh）是交互节奏，长空闲合法——超时重臂续睡（10s 帽续命循环，睡等
//! 不烧 CPU；真断链面 = recv 非 -EAGAIN 错码，照 trap）。
//!
//! **静默无输出**（服务不吵）：诊断走应答 st 码（ST_NOENT/ST_FULL/
//! ST_BAD）；例外 = FAT 层 init 失败/会话毒化各打一行（链断级事件，
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
    encode_reply_get, encode_reply_list, encode_reply_status, parse_request, FILE_COUNT_MAX,
    FILE_SIZE_MAX, LIST_MAX, Req, CHUNK, ST_BAD,
};

use fat::{Fat, GetRes};

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
    let mut files: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
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
            Some(Req::Put { name, off, data }) => {
                // FAT-only 名先物化进 RAM（覆盖层写语义），再走 RAM 通道；
                // 物化拒（尺寸/计数帽、会话毒化）= st 直接回。
                let st = match fat.as_mut().and_then(|f| f.materialize(&mut files, name)) {
                    None => do_put(&mut files, name, off, data),
                    Some(st) => st,
                };
                encode_reply_status(&mut rep, st).unwrap_or_else(|| trap())
            }
            Some(Req::Trunc { name, size }) => {
                // 同 Put：物化后 truncate（`echo > FAT名` 的建/截断面）。
                let st = match fat.as_mut().and_then(|f| f.materialize(&mut files, name)) {
                    None => do_trunc(&mut files, name, size),
                    Some(st) => st,
                };
                encode_reply_status(&mut rep, st).unwrap_or_else(|| trap())
            }
            Some(Req::Get { name, off }) => match handle_get(&files, name, off, &mut rep) {
                Some(n) => n, // rep 已编码（handle_get 内部落字）
                None => match fat.as_mut().map(|f| f.handle_get(name, off, &mut rep)) {
                    Some(GetRes::Served(n)) => n,
                    Some(GetRes::Full) => encode_reply_status(&mut rep, berry_fsproto::ST_FULL)
                        .unwrap_or_else(|| trap()),
                    Some(GetRes::Bad) => {
                        encode_reply_status(&mut rep, ST_BAD).unwrap_or_else(|| trap())
                    }
                    // RAM/FAT 双未命中（或 FAT 层缺席）——NOENT。
                    Some(GetRes::Miss) | None => encode_reply_status(
                        &mut rep,
                        berry_fsproto::ST_NOENT,
                    )
                    .unwrap_or_else(|| trap()),
                },
            },
            Some(Req::List) => {
                // 名列暂存走独立 scratch（msg 的借用跨整个 match 存活）；
                // FAT 未影子名续列（LIST_MAX 同帽）。
                let mut names = [0u8; 256];
                let n = render_list(&files, &mut names);
                let n = match fat.as_ref() {
                    Some(f) => f.append_names(&files, &mut names, n),
                    None => n,
                };
                encode_reply_list(&mut rep, &names[..n]).unwrap_or_else(|| trap())
            }
            None => encode_reply_status(&mut rep, ST_BAD).unwrap_or_else(|| trap()),
        };
        send_reply(h, &rep, rep_len);
    }
}

/// 分片写：off 起写入 data（off 超现长 = 零填充扩容）；未命中 = 建（文件
/// 数帽内）。尺寸帽在两端统一检（end = off+len > FILE_SIZE_MAX → ST_FULL）。
fn do_put(files: &mut Vec<(Vec<u8>, Vec<u8>)>, name: &[u8], off: u32, data: &[u8]) -> u8 {
    let off = off as usize;
    let end = off + data.len(); // u32 + ≤CHUNK：64 位下无溢出面
    if end > FILE_SIZE_MAX {
        return berry_fsproto::ST_FULL;
    }
    match files.iter_mut().find(|(n, _)| n.as_slice() == name) {
        Some((_, c)) => {
            if end > c.len() {
                c.resize(end, 0); // 零填充扩容（PUT 语义）
            }
            c[off..end].copy_from_slice(data);
            berry_fsproto::ST_OK
        }
        None => {
            if files.len() >= FILE_COUNT_MAX {
                return berry_fsproto::ST_FULL;
            }
            let mut c = vec![0u8; end];
            c[off..end].copy_from_slice(data);
            files.push((name.to_vec(), c));
            berry_fsproto::ST_OK
        }
    }
}

/// 定长（size=0 兼 create——`echo > f` 的建文件面，open(O_TRUNC) 式
/// create-or-resize；POSIX truncate-fails-vs-open(O_TRUNC) 取后者，v1）。
fn do_trunc(files: &mut Vec<(Vec<u8>, Vec<u8>)>, name: &[u8], size: u32) -> u8 {
    let size = size as usize;
    if size > FILE_SIZE_MAX {
        return berry_fsproto::ST_FULL;
    }
    match files.iter_mut().find(|(n, _)| n.as_slice() == name) {
        Some((_, c)) => {
            c.resize(size, 0);
            berry_fsproto::ST_OK
        }
        None => {
            if files.len() >= FILE_COUNT_MAX {
                return berry_fsproto::ST_FULL;
            }
            files.push((name.to_vec(), vec![0u8; size]));
            berry_fsproto::ST_OK
        }
    }
}

/// 分片读：off ≥ len → 空片 + more=0（文件尾/空文件面）；否则 ≤CHUNK 窗 +
/// more。命中落 GET 应答返 `Some(len)`（rep 已落字）；未命中返 None（调用方
/// 打 ST_NOENT）。编码 None（算术出界）= 服务端 bug → trap（不静默）。
fn handle_get(
    files: &[(Vec<u8>, Vec<u8>)],
    name: &[u8],
    off: u32,
    rep: &mut [u8; 256],
) -> Option<usize> {
    let (_, c) = files.iter().find(|(n, _)| n.as_slice() == name)?;
    let off = off as usize;
    let (slice, more) = if off >= c.len() {
        (&c[c.len()..], false) // 空片收尾
    } else {
        let end = (off + CHUNK).min(c.len());
        (&c[off..end], end < c.len())
    };
    // 服务端不变量：more=true ⇒ dlen ≥ 1（end>off 依构造成立）。
    encode_reply_get(rep, off as u32, slice, more).or_else(|| trap())
}

/// 名列渲染（空白连接，LIST_MAX=200 容纳截断——截断 = 后续文件不列，
/// 调用方可见）。
fn render_list(files: &[(Vec<u8>, Vec<u8>)], buf: &mut [u8; 256]) -> usize {
    let mut n = 0usize;
    for (name, _) in files {
        let need = name.len() + if n > 0 { 1 } else { 0 };
        if n + need > LIST_MAX {
            break; // 截断：帽内尽量多列
        }
        if n > 0 {
            buf[n] = b' ';
            n += 1;
        }
        buf[n..n + name.len()].copy_from_slice(name);
        n += name.len();
    }
    n
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
