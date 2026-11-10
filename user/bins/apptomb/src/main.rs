//! apptomb —— app-2 刀2：生命周期三口自验件（libc 形）。
//!
//! `docs/app-2实施设计草案-v1.0.md` §4 迁移（app-1 段 b-1 的 hello 形
//! 退役）：独立小工作区 musl target，单依赖 berry-libc；env 解析/argv/
//! console 全交 libc crt0（`berry_libc_boot`——argv0 断言面不变）。协议
//! 面（TAG 手排 + `send_msg`/recv 手写 + 12B 头手排 + counter 局部变量）
//! 全数改经 berry-libc `lifecycle` 三口/机械件：显式 poll dispatch +
//! 库代持 32B 容器头（ver=1 含 CRC32）+ 帧面库代发——**帧码/counter
//! 对账对 svcmgr tomb 面零改**（迁移不改协议字节 = 刀2 判据线）。
//!
//! 四幕（草案 §2.2 调用时序）：
//! - 幕① `state_restore` 三分型（Fresh/Restored/BadFmt——库发 UP
//!   c=0 或 RESTORED c=N+1）；恢复路径 ≥1 心跳后自退 exit(0)（v0
//!   复活即证不常驻——范围盒子不变）；
//! - 幕② `lifecycle_register(Some(on_lc))`；
//! - 幕③ 稳态两拍 `loop { lifecycle_poll(); sleep(拍); lifecycle_beat(); }`
//!   ——freeze 载荷条件（§5 立法：禁阻塞等任何 handle）随库继承：
//!   帧发送 -EBUSY 非阻塞自旋（明文禁 wait_one）、sleep 经 libc
//!   `nanosleep`（同底 `berry_thread_sleep`——SLEEPQ 无 wait_signals
//!   形态零漂，R3）；
//! - 幕④ PAUSESV1 收束在库内（cb 内 `state_save` → 写成功才发 SAVED
//!   → cb 返回 → 库 `exit(0)` EOF 先行——`save_and_exit` 的库化同构；
//!   未调 save / save IoErr = 零写拒绝保存安全侧不入表）。
//!
//! 业务档面：v0 blob 本就只有头无 payload——v1 同义「空态保存」
//! （len=0 ∧ crc(空)=0 过验，§3.2 写侧规则末行首个现役样本），counter
//! 续接经头 8..12（跨版本同位）。
//!
//! 诊断行：全走 berry-libc 非阻塞诊断口（§2.2 诊断口纪律——冻结敏感
//! 窗内不经 `console_write` 的阻塞重试，R3 对策主案；64-cap try-spin
//! 与 v0 hello 形逐字节同款）。console 行仅辅证（H1——判据全走通道）。
//!
//! 契约面（不变式，§4 迁移形态对照末行）：argv0 = entry 断言（错值 →
//! assert 炸 → abort(134) → reap 非 0 → 标记缺失 loud）；gift 0 值破缺
//! 被 absent no-op + svcmgr UP 超时兜底取代（库化后 lifecycle_h()==0
//! 是合法零义务面，破缺的 loud 化由对端判据链承接——v0 `dead()` 的
//! 语义迁居）。

use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use berry_libc::c_int;
use berry_libc::lifecycle::{self, LcRestore, LcSave, LC_EV_PAUSE};

// 链接种子（appdemo/appquiet 同款）：无引用则 berry-libc 的 crt0（含
// `_start`）不进链接；`#[used]` 防死码消除（fat-LTO 丢 crt0 先例）。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// 心跳周期。2s 亦即 PAUSESV1 投递余量的根（§4：poll 型应用自留
/// ≥1 轮询周期 <10s deadline——v0 值原样继承）。
const HEARTBEAT_NS: u64 = 2 * 1_000_000_000;

/// 幕① 读档 buf（§3.2 读侧契约：满帽 64KiB-32B——**落 static 不入栈**
/// （65504B > 栈 4 页 16KiB，launcher 穿保护页先例同警；manifest
/// stack_pages=4 现值保留待实跑复核——草案 §7 帽表/§4 步3）。
static mut STATE_BUF: [u8; lifecycle::PAYLOAD_CAP] = [0; lifecycle::PAYLOAD_CAP];

/// 幕② pause 回调（同步执行 + appd 侧 10s 单钟 deadline 内完成）。
/// 回调内可调集 = {state_save}（全口契约）。
extern "C" fn on_lc(ev: c_int) {
    if ev == LC_EV_PAUSE {
        // 空态保存（v0 同义——v0 blob 只有头无 payload）：盖 32B v1 头
        // （counter 当拍值 + len=0 + crc(空)=0）→ 写 state VMO → 发
        // SAVED（写成功才发）。
        if lifecycle::state_save(&[]) == LcSave::Ok {
            lifecycle::diag_text(b"apptomb saved\n"); // 7+1+5+1 = 14B
        } else {
            // IoErr：视同未 save（幕④零写路径）——SAVED 不发、appd 侧
            // 降级面接手；库随 cb 返回收束 exit(0)。
            lifecycle::diag_text(b"apptomb nosave\n"); // 7+1+6+1 = 15B
        }
    }
    // LC_EV_RESUME：无业务档面（payload 恒空）——cb 空转（RESUMED 由
    // 库代发，counter 当拍值——判据 = 帧本身）。
}

fn main() -> ExitCode {
    // 契约①：argv0 = entry 断言（appdemo/appquiet 同款——错值 →
    // assert 炸 → abort(134) → reap 非 0 → 标记缺失 loud）。
    assert_eq!(
        std::env::args().next().as_deref(),
        Some("bin/apptomb"),
        "argv0 should be manifest entry"
    );

    // 幕①启动判读（库化三分型）：Fresh/BadFmt → 库已发 UP c=0；
    // Restored → 库已发 RESTORED c=N+1（counter 续接对账由库兑现——
    // app 侧 c 变量消亡，草案 §4 步2）。
    // SAFETY: main 线程独占（全口契约——单线程 app 的启动期面）。
    let buf: &mut [u8] = unsafe { &mut *core::ptr::addr_of_mut!(STATE_BUF) };
    let short_run = match lifecycle::state_restore(buf) {
        LcRestore::Restored(n) => {
            // v0/v1 兼容读——n ≡ 头中 payload_len（本件恒 0：空态保存）。
            lifecycle::diag_line(b"apptomb restored n=", n as u32);
            true // v0 复活即证不常驻：≥1 心跳后自退 0
        }
        LcRestore::Fresh => {
            lifecycle::diag_line(b"app apptomb up c=", 0);
            false
        }
        LcRestore::BadFmt => {
            // 库已打 badfmt 分词行 + 已发 UP——badfmt 行与 up 行分家
            //（库诊断/业务行各归其主）；up 行保 v0 惯例（全新启动恒打）。
            lifecycle::diag_line(b"app apptomb up c=", 0);
            false
        }
    };

    // 幕②注册（单槽覆写）。
    lifecycle::lifecycle_register(Some(on_lc));

    // 幕③稳态两拍循环（§5 立法原样继承）：poll 拍（非阻塞排空）+
    // 心跳拍（sleep → BEAT）。PAUSESV1 收束在库内 exit(0)——
    // lifecycle_poll 不返回（幕④）；PROBE 探针回环（WAKE 臂判据）
    // 亦在库内代发 PROBED。
    loop {
        lifecycle::lifecycle_poll();
        // 拍②：sleep 至下一心跳 + BEAT（底层同 berry_thread_sleep——
        // SLEEPQ 无 wait_signals，FROZEN 判据链零漂 R3 首验点）。
        thread::sleep(Duration::from_nanos(HEARTBEAT_NS));
        let c = lifecycle::lifecycle_beat(); // counter 库代持——返当拍值
        lifecycle::diag_line(b"apptomb beat c=", c);
        if short_run {
            // 恢复路径自退（≥1 心跳已过 = 复活者活转的通道实证）。
            // EOF 先行真退出（reap 真码 0 = TOMB ok 谓词链末环）。
            std::process::exit(0);
        }
    }
}
