//! app-1 段 b-1——墓碑协议 v0 自验 app（`docs/app-1实施设计草案-v1.2.md`
//! §4/§5——段 a freeze-first 序列续接 pause(save)→记表→relaunch 读档复活；
//! hello 模型：no_std ELF 本体，env 块 x0 自解析，单依赖 berry-sys）。
//!
//! 判据载体 = env 块 extras 两枚（gift 绝对位 [4]/[5]）：[4] = lifecycle
//! 通道端 B（up/beat/resumed/saved/restored 上报与 RESUME/PAUSESV1 下行；
//! 对抗审查 H1 修：console=true 的 stdout 走 console 通道 SEND 别名、
//! svcmgr 只发不读——串口行 appd 侧不可观测，判据全走通道，console 行仅
//! 诊断辅证）+ [5] = state VMO（appd 预建 64KiB 定尺寸 dup 授子——v1.2
//! §4 放弃只读收紧：复活者再入 pause(save) 循环需写权，循环天然闭合）。
//!
//! 生命周期三态（§4 relaunch 启动判读）：
//! - 全新启动：state VMO 首 12B 非 BTMB（零态）→ 发 `UP c=0` → 稳态循环；
//! - 恢复启动：BTMB 且 ver=0/hlen=12 → counter+1 发 `RESTORED c=N+1` →
//!   ≥1 心跳后自退 exit(0)（v0 复活即证不常驻——范围盒子，真策略 v2）；
//! - 坏档：BTMB 但 ver/hlen 不符 → 弃档按零态启动 + badfmt 诊断行（坏档
//!   分型归 appd 侧 relaunch 降级面——收 UP 而非 RESTORED）。
//!
//! 稳态两拍循环（§5 立法）：①recv(lc) 非阻塞——得 RESUME → 打 resumed +
//! 回 RESUMED（D2 消息化：a-1 的 clock 差值启发式退役，TCG oversleep 假
//! 触发/阈值漏触发两险同除）；得 PAUSESV1 → §4 契约〔12B 头直写 state
//! VMO → 发 SAVED → exit(0)〕；-EAGAIN → ②sleep 至下一心跳拍发 BEAT。
//! **freeze 载荷条件（§5 立法）**：等待只许「非阻塞 recv + sleep」两拍，
//! 禁 object_wait_one(lc, …) 阻塞等——阻塞等通道的任务使 process_suspend
//! 恒返 -EBUSY（freeze_precheck 检查项）；此约束随 app-2 回调封装原样
//! 继承，阻塞化不是无害 API 演进。
//!
//! 契约（§5）：argv0 = entry 断言（`bin/apptomb`）/ EOF 先行退出（feeder
//! 零长 EOF 恒在，无阻塞面）/ gift 0 值 = 协议破缺 → 单行诊断后 park
//! （D9：失败归因双通道——app 侧诊断行 + svcmgr eco 降级行）。
#![no_std]
#![no_main]

use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_process_exit,
    berry_thread_sleep, berry_vmo_read, berry_vmo_write, Handle, BERRY_ERR_AGAIN,
    BERRY_ERR_BUSY, CLOCK_MONOTONIC,
};

/// 心跳周期（绝对 deadline 制：t0 + HEARTBEAT_NS）。2s 亦即 PAUSESV1 投递
/// 余量的根（§4：poll 型应用自留 ≥1 轮询周期 <10s deadline）。
const HEARTBEAT_NS: u64 = 2 * 1_000_000_000;

/// lifecycle 消息（8B 定长：tag u8 + pad 3 + counter u32 LE——与 svcmgr
/// tomb_selftest 同一套常量；文本行是 console 诊断面，不进协议）。
const MSG_SZ: usize = 8;
const TAG_UP: u8 = 1;
const TAG_BEAT: u8 = 2;
const TAG_RESUMED: u8 = 3;
/// D2（调和书 §二）：解冻通知（appd close(token) 后下行）——tag 4 起为
/// 段 b 面。
const TAG_RESUME: u8 = 4;
const TAG_RESTORED: u8 = 5;
const TAG_PAUSESV1: u8 = 6;
const TAG_SAVED: u8 = 7;
/// AM-1.5 WAKE 臂（svcmgr tomb_selftest 冻结窗探针）：冻结期消息照常
/// 入队不丢弃（非阻塞有界队列天然排队），解冻后稳态拍照收——回
/// PROBED 回环帧（帧本身即判据，counter 带当拍值）。
const TAG_PROBE: u8 = 8;
const TAG_PROBED: u8 = 9;

/// state blob 头（v1.2 §4/§7 立法）：12B 定长 {magic u32 LE, ver u8,
/// hlen u8, rsv u16=0, counter u32 LE}——ver = blob 格式版本（协议侧）非
/// app 版本；v1 CRC32/长度域经 hlen 扩展追加于头后，不塞 rsv。头部保留
/// 区归 berry-libc lifecycle 三口代持（§5 app-2 前置立法）——v0 头布局
/// 为自验件试验格式、非平台 ABI。
const BTMB_MAGIC: u32 = u32::from_le_bytes(*b"BTMB");
const BLOB_VER: u8 = 0;
const BLOB_HLEN: usize = 12;

fn clock_now(con: Handle) -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        dead(con);
    }
    t
}

/// 非零即死（错码自陷纪律：echo/hello 同款——静默死 = svcmgr 侧标记缺失
/// + watchdog 断言暴露，loud 优于装活）。D9（调和书 §四）：park 前先打
/// 单行诊断——失败归因双通道；con=0 = env 块未解出（console 面尚不可达，
/// 纯 park）。
fn dead(con: Handle) -> ! {
    if con != 0 {
        print_text(con, b"apptomb dead\n"); // 7+1+4+1 = 13B——行宽先数后写
    }
    loop {
        berry_thread_sleep(u64::MAX);
    }
}

fn send_msg(lc: Handle, tag: u8, c: u32) {
    let mut m = [0u8; MSG_SZ];
    m[0] = tag;
    m[4..8].copy_from_slice(&c.to_le_bytes());
    // 自旋重发（hello 同款契约：启动期饱和窗内阻塞等待会输掉竞速，自旋者
    // 由轮转兜底必达；-EBUSY 满队重试，其他错码死）。
    loop {
        let s = berry_channel_send(lc, m.as_ptr(), MSG_SZ, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            dead(0); // 通道面破缺——lc 生命周期内无 con 依赖面，纯 park
        }
    }
}

/// console 诊断行（无计数整行变体：dead/badfmt 单行面；print_line 同款
/// 64 次帽——诊断面尽力而为，协议判据不依赖 console 行〔H1〕）。
fn print_text(con: Handle, line: &[u8]) {
    let mut tried = 0;
    while berry_channel_send(con, line.as_ptr(), line.len(), core::ptr::null(), 0) != 0 {
        tried += 1;
        if tried > 64 {
            return;
        }
    }
}

/// console 诊断行（`前缀` + 十进制 + `\n`——thr exit 降级行同款手排）。
fn print_line(con: Handle, prefix: &[u8], c: u32) {
    let mut line = [0u8; 40];
    let n = prefix.len().min(line.len() - 12);
    line[..n].copy_from_slice(&prefix[..n]);
    let mut i = n;
    let mut v = c as u64;
    let mut digits = [0u8; 10];
    let mut dn = 0;
    loop {
        digits[dn] = b'0' + (v % 10) as u8;
        dn += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    while dn > 0 {
        dn -= 1;
        line[i] = digits[dn];
        i += 1;
    }
    line[i] = b'\n';
    let total = i + 1;
    let mut tried = 0;
    while berry_channel_send(con, line.as_ptr(), total, core::ptr::null(), 0) != 0 {
        tried += 1;
        if tried > 64 {
            return; // 诊断面尽力而为——协议判据不依赖 console 行（H1 修）
        }
    }
}

/// §4 契约：pause(save) 收束——12B 头直写 state VMO（berry_vmo_write 内核
/// 中介直写零映射——vblk 数据面同构先例；blob 帽 = VMO 64KiB 定尺寸结构
/// 性执法，越写物理不可）→ 发 SAVED（**写成功才发**——「我存了」断言，
/// 零写拒绝保存语义不 applicable：本自验件恒有态）→ console 打 saved →
/// exit(0)（EOF 先行，退出权自此归还 appd）。
fn save_and_exit(con: Handle, lc: Handle, st: Handle, c: u32) -> ! {
    let mut b = [0u8; BLOB_HLEN];
    b[0..4].copy_from_slice(&BTMB_MAGIC.to_le_bytes());
    b[4] = BLOB_VER;
    b[5] = BLOB_HLEN as u8;
    // b[6..8] = rsv 恒 0（v1 CRC 经 hlen 扩展追加，不塞 rsv）。
    b[8..12].copy_from_slice(&c.to_le_bytes());
    if berry_vmo_write(st, b.as_ptr(), 0, b.len()) != 0 {
        dead(con);
    }
    send_msg(lc, TAG_SAVED, c);
    print_line(con, b"apptomb saved c=", c);
    berry_process_exit(0)
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env: Handle) -> ! {
    // env 块（d-3a）：argv0 = `bin/apptomb`（SpawnSpec.name = manifest entry，
    // 契约断言面）+ handles 前缀紧致 [stdout 别名, stdin 端, self_proc,
    // self_vmar, extras…]——stdin_feed=Some(&[]) 在位（M1 修：tomb_selftest
    // 直传），extras = [lifecycle 端 B, state VMO dup]（v1.2 §4 双 gift，
    // nhandles = 6）。收窗 8 槽全开。
    let mut blob = [0u8; 64];
    let mut nb = blob.len();
    let mut hs = [0 as Handle; 8];
    let mut nh = hs.len();
    if berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh) != 0 {
        dead(0); // env 块本体破缺——console 面尚不可达
    }
    let con = hs[0]; // stdout 别名（SEND）——诊断行
    if nh < 6 {
        dead(con); // 双 gift 缺席 = 协议破缺（§5 0 值语义同族）
    }
    // 契约①：argv0 = entry 断言。
    if !blob.starts_with(b"bin/apptomb") {
        dead(con);
    }
    let lc = hs[4]; // lifecycle 端 B（SEND|RECV fresh 端）
    let st = hs[5]; // state VMO dup（R|W——appd 授子面）
    if lc == 0 || st == 0 {
        dead(con); // §5：0 值 = 协议破缺 → 单行诊断后 park
    }

    // 启动判读（§4）：vmo_read 首 12B——BTMB 且 ver=0/hlen=12 → 恢复路径
    // （counter+1）；非 BTMB（零态）→ 全新路径；BTMB 坏 ver/hlen → 弃档
    // 按零态 + badfmt 诊断（§4 badfmt 面——appd 侧收 UP 而非 RESTORED 即
    // 降级）。首轮（全新）state VMO 只读用于判读不写——写只发生在收
    // PAUSESV1 后（§4 契约）。
    let mut head = [0u8; BLOB_HLEN];
    if berry_vmo_read(st, head.as_mut_ptr(), 0, head.len()) != 0 {
        dead(con); // commit 面 appd 已保证——读失败 = 协议破缺
    }
    let mut c: u32 = 0;
    let mut short_run = false;
    if u32::from_le_bytes([head[0], head[1], head[2], head[3]]) == BTMB_MAGIC {
        if head[4] == BLOB_VER && head[5] as usize == BLOB_HLEN {
            // 恢复路径（感知契约：跨生死读档 restored c=N+1）。
            c = u32::from_le_bytes([head[8], head[9], head[10], head[11]]).wrapping_add(1);
            send_msg(lc, TAG_RESTORED, c);
            print_line(con, b"apptomb restored c=", c);
            short_run = true; // v0 复活即证不常驻：≥1 心跳后自退 0
        } else {
            print_text(con, b"apptomb badfmt\n"); // 7+1+6+1 = 15B
        }
    }
    if !short_run {
        send_msg(lc, TAG_UP, 0);
        c = 0;
        print_line(con, b"app apptomb up c=", 0);
    }

    // 稳态两拍循环（§5）：poll 拍（非阻塞 recv）+ 心跳拍（sleep → BEAT）。
    // sleep 绝对时刻式（t0 + HEARTBEAT_NS，a-1 同形——冻结窗吞掉的
    // deadline 在 thaw 后立即到期续跑，beat 计数不重置〔L1 机制注记〕）。
    loop {
        // 拍①：非阻塞 poll lifecycle（freeze 载荷条件：禁阻塞等）。
        loop {
            let mut m = [0u8; MSG_SZ];
            let mut mnb = m.len();
            let mut mnh = 0usize;
            let s =
                berry_channel_recv(lc, m.as_mut_ptr(), &mut mnb, core::ptr::null_mut(), &mut mnh);
            if s == BERRY_ERR_AGAIN {
                break; // 空队 → 拍②
            }
            if s != 0 {
                dead(con);
            }
            match m[0] {
                TAG_RESUME => {
                    // D2：解冻通知回执（counter 带当拍值；判据 = 帧本身）。
                    send_msg(lc, TAG_RESUMED, c);
                    print_line(con, b"apptomb resumed c=", c);
                }
                TAG_PAUSESV1 => {
                    save_and_exit(con, lc, st, c);
                }
                TAG_PROBE => {
                    // AM-1.5：探针回环（与 RESUMED 同款——判据 = 帧本身，
                    // counter 带当拍值仅诊断辅证）。
                    send_msg(lc, TAG_PROBED, c);
                    print_line(con, b"apptomb probed c=", c);
                }
                // 未知 tag：v0 封闭协议（对端恒 svcmgr）——忽略续跑，前向
                // 兼容面归 app-2 回调三口。
                _ => {}
            }
        }
        // 拍②：sleep 至下一心跳拍 + BEAT。
        let t0 = clock_now(con);
        if berry_thread_sleep(t0.wrapping_add(HEARTBEAT_NS)) != 0 {
            dead(con);
        }
        c = c.wrapping_add(1);
        send_msg(lc, TAG_BEAT, c);
        print_line(con, b"apptomb beat c=", c);
        if short_run {
            // 恢复路径自退（≥1 心跳已过 = 复活者活转的通道实证）。
            berry_process_exit(0);
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    dead(0)
}
