//! 生命周期三口（app-2）——规范 §4.1-5 的 berry-libc 封装面。
//!
//! `docs/app-2实施设计草案-v1.0.md` §2 立法：三口（注册 pause 回调 /
//! 写 state / 读 state）+ 配套协议机械件（poll dispatch / beat 节拍）
//! 从 svcmgr/apptomb 裸协议面升为库封装——**显式 poll dispatch + 库代持
//! 32B 容器头（含 CRC32）+ 帧面库代发**；apptomb（刀2 迁 libc 路）为
//! 首个现役消费者，svcmgr tomb 面（帧码/对账/帽）零改。
//!
//! 分层：纯函数面（头编解码/CRC32/hlen 分派/三分型 + 8B 帧编解码 +
//! counter 状态机）在 `berry-lifecycle` 独立 crate（host 测试零 libc
//! 符号面——berry-tls/berry-signal 先例）；本模块 = syscall 胶水面
//! （句柄获取 io.rs `lifecycle_h`/`state_h` + channel/VMO 收发 + 幕序
//! 编排）。
//!
//! ## dispatch 形态（§2.1 裁决：显式 poll 为 v1 唯一形态）
//!
//! 两拍循环（poll 拍 + 心跳拍）与 apptomb hello 形手写 poll 同构——
//! freeze 载荷条件（阻塞等任何 handle 的任务使 process_suspend 恒返
//! -EBUSY）下唯一合法形态；libc 阻塞点自动泵/信号面借用两案已否决
//! （覆盖幻觉/回调上下文随机化/bit 合并三病）。
//!
//! ## 帧发送纪律（§2.2 立法）
//!
//! 库代发五帧（UP/RESTORED/SAVED/RESUMED/BEAT，另 AM-1.5 WAKE 臂
//! PROBED 回环）send 错码三分：`0` 成功；`-EBUSY` **非阻塞自旋重发**
//! （满队瞬态由轮转兜底必达——**明文禁 `wait_one(lifecycle_h, ·)`**：
//! lifecycle app 是冻结判据载体进程本身，句柄入 wait 集即全进程拒冻；
//! 雷 2 判例对立豁免——lifecycle 通道每 app 单通道低频 8B 帧，非
//! console 多生产者竞争面）；`-EAGAIN` send 面结构性不发生（chan send
//! 满语义为 -EBUSY、-EAGAIN 仅 recv 空队）按不可达落死；他错 =
//! `lifecycle dead` loud 单行诊断后 park（失败归因双通道 D9——app 侧
//! 诊断行 + svcmgr eco 降级行）。state VMO 写面错码**不 park**——返
//! `LcSave::IoErr` 归调用方（幕④走零写 exit(0)，安全侧不入表）。
//!
//! ## 诊断口纪律（§2.2 立法）
//!
//! 库内 loud 诊断行（`lifecycle dead`/BadFmt 分词行）走本模块非阻塞
//! best-effort 口（64-cap try-spin、超帽丢行——hello `print_text` 同款
//! 契约），**不经 `io::console_write`**（其 EBUSY 阻塞等 10s/臂 =
//! 冻结窗第二阻塞暴露面，§8.3 R3）；丢行无害根据 = console 行仅辅证、
//! 协议判据不依赖 console 行（H1）。迁移侧（apptomb）敏感窗诊断行同
//! 走此口。
//!
//! ## 全口契约（§2.2 立法）
//!
//! 五口 = **应用主循环线程独占**（lifecycle 库态 counter/回调槽为无锁
//! static mut，未入 libc 跨线程共享面——T-1 部分真身化不含五口）；
//! 回调内可调集 = {`state_save`}；poll/register/beat 回调内重入 = UB；
//! worker 线程并发调任一口 = UB。
//!
//! ## 零义务兼容律（§2.3 立法）
//!
//! GIFT_H 槽位是多产面（rootfs/fs/lifecycle 同槽不同名）——`lifecycle_h()
//! == 0` ⇒ Absent 仅对无 fs 词 app 成立（单向下行安全）；带 fs 词 app
//! （现役反例 APPDEMO）GIFT_H[0]/[1] 载错对象，三口对该面 v1 定义为
//! 未定义行为（absence 判据重构挂 v2）。结构性缺席面（无 fs 词 ∧ 未
//! 装配 gift）：五口全 absent no-op——`register` 返 Absent、`poll` 立返
//! 0、`state_save` 返 Absent、`state_restore` 返 Fresh 且不发帧、
//! `beat` 立返 0 不发帧 counter 不递增——零义务 app 零代码改动零行为
//! 漂移（验证锚 = appquiet absent 探针）。

use core::ptr;

use berry_lifecycle::{
    check_len, classify_head, crc_matches, crc32, decode_frame, encode_frame, encode_head_v1,
    Counter, FRAME_SZ, HeadKind, LenIssue, TAG_BEAT, TAG_PAUSESV1, TAG_PROBE,
    TAG_PROBED, TAG_RESUME, TAG_RESUMED, TAG_RESTORED, TAG_SAVED, TAG_UP,
};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_vmo_read, berry_vmo_write, BERRY_ERR_AGAIN,
    BERRY_ERR_BUSY,
};

use crate::c_int;
use crate::io::{lifecycle_h, state_h};

// 纯面常量再导出（消费者按「单依赖 berry-libc」形态取满帽 buf 尺寸——
// 草案 §4 迁移形态对照）。
pub use berry_lifecycle::{HEAD_V1, PAYLOAD_CAP, STATE_VMO_SZ};

// —— LcEvent 自有枚举（§2.2：勿复用帧 tag——API 与线协议解耦，v2 事件
//    词表扩展零协议联动）——

/// pause 回调事件（同步执行 + appd 侧 10s 单钟 deadline 内完成——
/// UWP「GetDeferral 立即完成」等效形；deadline 查询口不做）。
pub const LC_EV_PAUSE: c_int = 1;
/// resume 回调事件。
pub const LC_EV_RESUME: c_int = 2;

/// 回调形（C ABI 冻结挂 SDK-2——本刀只立 Rust 面）。
pub type LcCallback = extern "C" fn(ev: c_int);

// —— 返回值域（§2.2 签名立法）——

/// 注册结果（单槽覆写式；`cb=None` = 注销）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LcReg {
    Ok,
    /// lifecycle gift 缺席（未在生命周期 spawn 面——零义务面合法态）。
    Absent,
}

/// 保存结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LcSave {
    Ok,
    /// payload 越帽（> 64KiB-32B——库侧前置拒，诊断前移于 VMO 定尺寸
    /// 结构性执法之上）。
    TooLarge,
    /// lifecycle gift 缺席。
    Absent,
    /// vmo_write 他错（不 park——归调用方；幕④视同未 save 走零写
    /// exit(0)）。
    IoErr,
}

/// 读档三分型（apptomb v0 启动判读 :203-224 的库化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LcRestore {
    /// 全新启动（零态或 BadFmt 弃档按零态——两径库均发 UP c=0）。
    Fresh,
    /// 读档成功（n ≡ 头中 payload_len——满帽下实填量恒等，消歧义）。
    Restored(usize),
    /// 坏档弃档（已按零态发 UP——app 侧无需再打 badfmt 行）。
    BadFmt,
}

// —— 库态（无锁 static mut：五口 = 主循环线程独占——全口契约）——

/// 回调槽（1 槽覆写式——§7 帽表）。
static mut CB: Option<LcCallback> = None;
/// counter 代持（协议对账域——应用 payload 不含协议域）。
static mut COUNTER: Counter = Counter::new();

// —— 非阻塞诊断口（§2.2 诊断口纪律）——

/// 诊断 try-spin 帽（hello `print_text` 同款：超帽丢行——尽力而为）。
const DIAG_TRIES: u32 = 64;

/// 非阻塞 best-effort 诊断整行（无计数）：console 通道直接 send，64 次
/// 帽内重试、超帽丢行——不经 `console_write` 的阻塞重试（R3）。
pub fn diag_text(line: &[u8]) {
    let h = crate::io::console_h();
    if h == 0 {
        return; // console 面未装配（boot 破缺语境）——纯丢行
    }
    let mut tried = 0;
    while berry_channel_send(h, line.as_ptr(), line.len(), ptr::null(), 0) != 0 {
        tried += 1;
        if tried > DIAG_TRIES {
            return; // 满队瞬态超帽：丢行无害（H1——判据不依赖 console 行）
        }
    }
}

/// 非阻塞诊断行（`前缀` + 十进制 + `\n`——apptomb v0 `print_line` 同形，
/// 行宽先数后写）。
pub fn diag_line(prefix: &[u8], v: u32) {
    let mut line = [0u8; 40];
    let n = prefix.len().min(line.len() - 12);
    line[..n].copy_from_slice(&prefix[..n]);
    let mut i = n;
    let mut val = v as u64;
    let mut digits = [0u8; 10];
    let mut dn = 0;
    loop {
        digits[dn] = b'0' + (val % 10) as u8;
        dn += 1;
        val /= 10;
        if val == 0 {
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
    let h = crate::io::console_h();
    if h == 0 {
        return;
    }
    while berry_channel_send(h, line.as_ptr(), total, ptr::null(), 0) != 0 {
        tried += 1;
        if tried > DIAG_TRIES {
            return;
        }
    }
}

// —— 帧发送（三分纪律）与死亡面 ——

/// 通道面破缺收口：loud 单行诊断后 park（apptomb v0 `dead()` 的库化；
/// park = stdout EOF 先行 + 10s 周期睡眠——trap 是 watchdog 错杀违约）。
fn dead() -> ! {
    diag_text(b"lifecycle dead\n"); // 9+1+4+1 = 15B
    crate::park()
}

/// 库代发一帧（帧发送纪律三分：0/-EBUSY 自旋/他错 park）。
fn send_frame(tag: u8, c: u32) {
    let lc = lifecycle_h();
    let mut f = [0u8; FRAME_SZ];
    encode_frame(&mut f, tag, c);
    loop {
        let s = berry_channel_send(lc, f.as_ptr(), FRAME_SZ, ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s == BERRY_ERR_AGAIN {
            // 不可达断言（kernel channel.rs 头注：send 满语义 -EBUSY、
            // -EAGAIN 仅 recv 空队）——落死 loud 不静默。
            dead();
        }
        if s != BERRY_ERR_BUSY {
            dead(); // 他错 = lifecycle dead park
        }
        // -EBUSY：非阻塞自旋重发（禁 wait_one——F2 冻结载荷条件 send 面
        // 落地；自旋者由轮转兜底必达）。
    }
}

// —— 规范三口 ——

/// 幕②：注册 pause/resume 回调（单槽覆写；`cb=None` = 注销）。零义务
/// 面（gift 缺席）返 [`LcReg::Absent`]。
pub fn lifecycle_register(cb: Option<LcCallback>) -> LcReg {
    if lifecycle_h() == 0 {
        return LcReg::Absent;
    }
    // SAFETY: 主循环线程独占（全口契约）；无并发写者。
    unsafe { CB = cb };
    LcReg::Ok
}

/// 幕④ cb 内调用：盖 32B v1 头（counter 当拍值 + payload_len + CRC32）
/// → 写 state VMO → 发 SAVED（**写成功才发**）。
///
/// 写序立法：payload 先写、头后写——BTMB magic 即提交点，半途失败
/// （payload 成/头败）落非 BTMB 态 ⇒ svcmgr 记表 gate（首 4B 验）拒入
/// 表 = 安全侧。零长 payload 合法（「空态保存」——有头有 SAVED = 入
/// 墓碑表、relaunch 按零 payload 恢复）。
pub fn state_save(payload: &[u8]) -> LcSave {
    let lc = lifecycle_h();
    let st = state_h();
    if lc == 0 {
        return LcSave::Absent;
    }
    if st == 0 {
        dead(); // 通道在而档 VMO 缺 = 协议破缺（gift 对原子迁入——不可达臂 loud）
    }
    if payload.len() > PAYLOAD_CAP {
        return LcSave::TooLarge;
    }
    // SAFETY: 主循环线程独占。
    let c = unsafe { COUNTER.current() }; // save 写头取当拍值（不递增）
    let n = payload.len();
    if n > 0 && berry_vmo_write(st, payload.as_ptr(), HEAD_V1 as u64, n) != 0 {
        return LcSave::IoErr;
    }
    let mut head = [0u8; HEAD_V1];
    encode_head_v1(&mut head, c, payload.len() as u32, crc32(payload));
    if berry_vmo_write(st, head.as_ptr(), 0, HEAD_V1) != 0 {
        return LcSave::IoErr;
    }
    send_frame(TAG_SAVED, c);
    LcSave::Ok
}

/// 幕①：读档三分型 + 库代发 UP/RESTORED。调用方契约 = `buf ≥
/// 64KiB-32B` 满帽（不足帽对合法档返 [`LcRestore::BadFmt`]——不截断
/// 不半档）。absent 面返 Fresh 且不发帧（无对端无从等帧）。
pub fn state_restore(buf: &mut [u8]) -> LcRestore {
    let lc = lifecycle_h();
    let st = state_h();
    if lc == 0 {
        return LcRestore::Fresh;
    }
    if st == 0 {
        dead(); // 同 state_save 破缺臂
    }
    let mut head = [0u8; HEAD_V1];
    if berry_vmo_read(st, head.as_mut_ptr(), 0, HEAD_V1) != 0 {
        dead(); // commit 面 appd 已保证——读失败 = 协议破缺
    }
    match classify_head(&head) {
        HeadKind::Zero => {
            // SAFETY: 主循环线程独占。
            let c = unsafe { COUNTER.on_up() };
            send_frame(TAG_UP, c);
            LcRestore::Fresh
        }
        HeadKind::V0 { counter } => {
            // v0 旧档兼容读：counter 位续接、payload 域不存在（v0 blob
            // 只有头）→ Restored(0)。
            // SAFETY: 同上。
            let c = unsafe { COUNTER.on_restored(counter) };
            send_frame(TAG_RESTORED, c);
            LcRestore::Restored(0)
        }
        HeadKind::V1 { counter, payload_len, crc } => {
            match check_len(payload_len, buf.len()) {
                Err(LenIssue::OverCap) => {
                    // len 超界：档坏弃档按零态（§3.2 表）。
                    diag_text(b"lifecycle badfmt len\n"); // 16+1+3+1 = 21B
                    let c = unsafe { COUNTER.on_up() };
                    send_frame(TAG_UP, c);
                    LcRestore::BadFmt
                }
                Err(LenIssue::BufShort) => {
                    // buf 不足帽：调用方契约臂——分词 ≠ 档坏，不截断不半档。
                    diag_text(b"lifecycle buf-short\n"); // 9+1+9+1 = 20B
                    let c = unsafe { COUNTER.on_up() };
                    send_frame(TAG_UP, c);
                    LcRestore::BadFmt
                }
                Ok(()) => {
                    let n = payload_len as usize;
                    if n > 0 && berry_vmo_read(st, buf.as_mut_ptr(), HEAD_V1 as u64, n) != 0 {
                        dead(); // commit 全幅——读失败 = 协议破缺
                    }
                    if !crc_matches(crc, &buf[..n]) {
                        // crc 不符：坏档弃档按零态（CRC 首个真实消费者：
                        // 静默坏档 → 显式红）。
                        diag_text(b"lifecycle badfmt crc\n"); // 16+1+3+1 = 21B
                        let c = unsafe { COUNTER.on_up() };
                        send_frame(TAG_UP, c);
                        return LcRestore::BadFmt;
                    }
                    let c = unsafe { COUNTER.on_restored(counter) };
                    send_frame(TAG_RESTORED, c);
                    LcRestore::Restored(n)
                }
            }
        }
        HeadKind::Bad => {
            // ver 超前/hlen 异值：弃档按零态（apptomb v0 badfmt 臂同义）。
            diag_text(b"lifecycle badfmt head\n"); // 16+1+4+1 = 22B
            let c = unsafe { COUNTER.on_up() };
            send_frame(TAG_UP, c);
            LcRestore::BadFmt
        }
    }
}

// —— 配套协议机械件（三口的 dispatch/节拍载体，非第四第五口语义）——

/// 拍①：非阻塞排空 lifecycle 通道（返处理帧数——诊断辅证）。分派：
/// - RESUME → `cb(Resume)` + 库发 RESUMED c=当拍（判据 = 帧本身；
///   未注册 cb 则不调分派、照发 RESUMED——注册只是回调分派的前置）；
/// - PAUSESV1 → `cb(Pause)`（cb 内可调 `state_save`）→ cb 返回后库
///   `exit(0)`（EOF 先行）——**本函数不返回**（幕④收束：已 save 则
///   SAVED 已发；未 save = 零写拒绝保存安全侧不入表；save IoErr 视同
///   未 save 同 exit(0)）；未注册 cb 收 PAUSESV1 → 库直走零写
///   exit(0)（「默认可杀可墓碑」的诚实死法）；
/// - PROBE（AM-1.5 WAKE 臂冻结窗探针）→ 库代发 PROBED c=当拍（回环
///   判据 = 帧本身——app-2 §6 承诺 FROZEN/WAKE 零漂的承接点）；
/// - 未知 tag 忽略续跑（前向兼容面归本刀——apptomb v0 :252-254 承接）。
///
/// absent 面立返 0。recv 他错 = 通道面破缺 → `lifecycle dead` park。
pub fn lifecycle_poll() -> u32 {
    let lc = lifecycle_h();
    if lc == 0 {
        return 0;
    }
    let mut drained: u32 = 0;
    loop {
        let mut f = [0u8; FRAME_SZ];
        let mut nb = f.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(lc, f.as_mut_ptr(), &mut nb, ptr::null_mut(), &mut nh);
        if s == BERRY_ERR_AGAIN {
            return drained; // 空队 → 排空毕
        }
        if s != 0 {
            dead(); // recv 面他错 = 通道破缺（apptomb v0 :245-247 同款）
        }
        drained = drained.wrapping_add(1);
        // SAFETY: 主循环线程独占（全口契约——回调槽/counter 无并发面）。
        let (cb, cur) = unsafe { (CB, COUNTER.current()) };
        match decode_frame(&f).tag {
            TAG_RESUME => {
                if let Some(cb) = cb {
                    cb(LC_EV_RESUME);
                }
                send_frame(TAG_RESUMED, cur);
            }
            TAG_PAUSESV1 => {
                if let Some(cb) = cb {
                    cb(LC_EV_PAUSE);
                }
                // 幕④：库收束 exit(0)（EOF 先行——io.rs exit 同 apptomb
                // v0 save_and_exit 尾）。
                crate::io::exit(0)
            }
            TAG_PROBE => {
                send_frame(TAG_PROBED, cur);
            }
            _ => {} // 未知 tag 忽略续跑（v1 封闭协议对端恒 svcmgr）
        }
    }
}

/// 拍②：counter 库代持递增 + 发 BEAT，返当拍值（诊断辅证）。absent 面
/// 立返 0 不发帧 counter 不递增（与 poll 立返 0 同构）。
pub fn lifecycle_beat() -> u32 {
    let lc = lifecycle_h();
    if lc == 0 {
        return 0;
    }
    // SAFETY: 主循环线程独占。
    let c = unsafe { COUNTER.on_beat() };
    send_frame(TAG_BEAT, c);
    c
}
