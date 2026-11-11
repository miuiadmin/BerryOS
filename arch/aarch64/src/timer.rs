//! CNTV 虚拟定时器 — tickless（NO_HZ）驱动（plan Phase 2a Step 4 重写）。
//!
//! 用 CNTV（PPI IRQ 27），**非物理定时器**：CNTV 不经 `CNTHCTL_EL2`（EL1 直访），
//! `run`/`run-el2` 统一工作。频率读 `CNTFRQ_EL0`（QEMU `virt` 62.5MHz，**不硬编码**）。
//!
//! **Tickless（day-one）**：空闲（runqueue 空）时 [`mask`] 屏蔽 + `wfi`——空闲核不收
//! 周期中断（续航第一杠杆，非后期补丁）；有 runnable 任务时 [`arm_slice`] 武装一个
//! 时间片（Step 6 驱动抢占）。替代 Phase 1 的 1Hz 自停 demo。
//!
//! **电平触发 + rearm-before-EOIR**：IRQ handler 须在 EOIR 前 [`arm_slice`]（重写
//! 未来时刻的 CVAL 去活电平，语义不变），否则 EOIR 后电平仍高立即重投。
//! `exception_irq`（Step 5）保此序。
//!
//! **绝对截止时间重编程（纪律，非可选 · `调度器设计.md` §4.1）**：定时器永远编程到
//! 绝对到期时刻——[`arm_slice`] 以计数器现读（CNTVCT）重算，绝不基于上次编程值外推。
//! v1 无其他定时事件源，min(最早超时事件, 时间片终点) 退化为时间片终点；S-3a 起
//! tickless park 引入第二截止源（[`arm_at`] = SLEEPQ 最近 deadline，park 武装面）；
//! 授片处取 min 仍留后（Phase 4+）。
//!
//! **热路径零分配**：本模块无堆、无锁（G0 单核）。

/// 时间片频率（每秒切片数）。10Hz → 每 100ms 一 tick（slice=6.25M cycles @62.5MHz）。
/// 调小→切片长（切换少、开销低）；调大→更公平但切换多。Step 6 demo 以此驱动抢占。
pub const SLICE_HZ: u64 = 10;

static mut FREQ: u64 = 0;
static mut SLICE: u64 = 0;

/// 读 `CNTFRQ_EL0` + 缓存 FREQ/SLICE。返回频率（供上层打印验）。**不武装**——
/// 武装由 [`arm_slice`] 显式（tickless：空闲不 arm）。
///
/// # Safety
/// 写模块静态 + 系统寄存器；EL1、单核启动期、仅一次。
pub unsafe fn init() -> u64 {
    FREQ = read_cntfrq();
    SLICE = FREQ / SLICE_HZ;
    // B-1 前置修复（首跑实踩）：EL0 直读 CNTVCT（bench1 `mrs cntvct_el0` 计时）
    // 需 CNTKCTL_EL1.EL0VCTEN=1——复位 0 = EL0 读即 EC=0x18 陷阱 →
    // user_fault_exit 静默杀（fault 杀码 0、无 EOF——「跑完却零输出」死状）。
    // 显式开位 1（Linux 同款 enable）；RMW 保其它位（EVNT* 本核未用复位 0）；
    // 只开虚拟计数器——EL0PTEN（物理）无消费面不开（最小特权）。
    let kctl: u64;
    core::arch::asm!(
        "mrs {}, cntkctl_el1",
        out(reg) kctl,
        options(nostack, preserves_flags)
    );
    core::arch::asm!(
        "msr cntkctl_el1, {}",
        in(reg) kctl | 1 << 1,
        options(nostack, preserves_flags)
    );
    core::arch::asm!("isb", options(nostack, preserves_flags));
    FREQ
}

/// 武装到**绝对截止时刻**（S-3a tickless park：浅 park 的 SLEEPQ 最近
/// deadline 武装面——子案 §9.5 步 A）。与 [`arm_slice`] 同纪律同形态（CVAL
/// 绝对编程 + CTL=ENABLE 不屏蔽），仅截止来源不同：arm_slice = now+SLICE
///（授片），arm_at = 调用者给定的绝对 deadline（SLEEPQ 队头到期时刻——
/// 过期值合法，compare 立即成立即醒，回环 expire 承接）。
///
/// # Safety
/// 写 CNTV 系统寄存器；EL1。
pub unsafe fn arm_at(deadline: u64) {
    core::arch::asm!(
        "msr cntv_cval_el0, {0}",
        in(reg) deadline,
        options(nostack, preserves_flags)
    );
    // CNTV_CTL: bit0=ENABLE、bit1=IMASK（1=屏蔽）。开+不屏蔽 = 0b01（非 0b11）。
    let ctl: u64 = 0b01;
    core::arch::asm!(
        "msr cntv_ctl_el0, {0}",
        in(reg) ctl,
        options(nostack, preserves_flags)
    );
    core::arch::asm!("isb", options(nostack, preserves_flags));
}

/// 武装一个时间片：**绝对编程**（`调度器设计.md` §4.1「绝对截止时间重编程」纪律，
/// 非可选）——读 `CNTVCT` 现值、算 `want = now + SLICE`，**但不把已编程的更早
/// 截止推迟**（S-6 判例：deadline 单调夹取）：`cur = CVAL` 现读，若
/// `now < cur < want` 则保留 `cur` 不写；否则（已过期 `cur<=now`，或 `cur` 比
/// 整片更远，如 park 遗留的 SLEEPQ 截止 > now+SLICE 之外的远值）全新武装
/// `want`。兼任 IRQ handler 内的 rearm（rearm-before-EOIR）——tick 入口时
/// CVAL 刚过期（`cur<=now`），走全新武装支 = 原语义零漂移。
///
/// **为什么必须夹取（S-6 楔死定谳）**：无夹取时每个切换点无条件写
/// `now+SLICE`，块驱动的快速切换（console-sender 重试环：spin→svc→block→
/// switch，切换间隔 ≪ SLICE）形成 **deadline 跑步机**——CVAL 被无限前推
/// 永不到期 ⇒ CNTV 永不 assert ⇒ 本核永不收 tick ⇒ 无抢占 ⇒ home 核的
/// 常驻任务（watchdog/服务）全饿死（实测楔死核 ticks 冻结=4、GIC RPR/HPPIR/
/// pend 全 idle、CVAL 恒 ≈ now+74~96ms < SLICE）。夹取后最近一次武装的截止
/// 仍是本核 tick 的硬底——最早 `SLICE` 内必响一次 tick。
///
/// 附带兑现「授片处取 min」（模块头 Phase 4+ 留后）：park 唤醒派发时若
/// SLEEPQ 截止早于整片终点，保留之 = min(最早超时, 片终点) 的提前落地。
///
/// # Safety
/// 读 CNTVCT/CVAL + 写 CNTV 系统寄存器；EL1。
pub unsafe fn arm_slice() {
    let now = cntvct();
    let want = now + SLICE;
    let cur = cval();
    // S-6：deadline 单调夹取——只在「更早」方向移动已编程截止。
    if cur > now && cur < want {
        return;
    }
    unsafe { arm_at(want) };
}

/// 屏蔽 CNTV 中断（IMASK=1，ENABLE 保持）——tickless idle 用。倒计仍在跑，但中断不投。
///
/// # Safety
/// 写 CNTV_CTL；EL1。
pub unsafe fn mask() {
    let ctl: u64 = 0b11; // ENABLE=1, IMASK=1
    core::arch::asm!(
        "msr cntv_ctl_el0, {0}",
        in(reg) ctl,
        options(nostack, preserves_flags)
    );
    core::arch::asm!("isb", options(nostack, preserves_flags));
}

/// 关 CNTV（ENABLE=0）——彻底停（区别于 [`mask`] 的仅屏蔽）。
///
/// # Safety
/// 写 CNTV_CTL；EL1。
#[allow(dead_code)]
pub unsafe fn disable() {
    let ctl: u64 = 0b00;
    core::arch::asm!(
        "msr cntv_ctl_el0, {0}",
        in(reg) ctl,
        options(nostack, preserves_flags)
    );
    core::arch::asm!("isb", options(nostack, preserves_flags));
}

/// IRQ handler 钩子（`exception_irq` 在 EOIR 前调）：rearm 时间片（去活电平）。
/// Step 4 不触发（IRQ 未解蔽）；Step 5 起规律调用。
/// **S-3a F-E11 执法点**：rearm（CVAL 绝对重编程）唯一合法入口 = 本 timer
/// IRQ 入口——非 tick 尾（授片后遗 rearm = 与 CVAL 绝对编程脱钩）。授片
/// arm / rearm 二分：授片 arm = 调度侧自武装（park_recheck 出选真任务时
/// `arm_slice`，≠ rearm）；rearm = 本处去活电平铁序（电平触发 CNTV 先重编
/// 程 CVAL 再 EOIR，否则 EOIR 后线仍高立即重投）。
pub fn handle() {
    // SAFETY: IRQ 上下文、单核无并发。
    unsafe { arm_slice() }
}

/// 读虚拟计数器 `CNTVCT_EL0`（周期数）。调度器时间片按**周期数记账**（2b-4 起，
/// 路线图 Phase 2b：LiteOS-M `OsSchedTimeConvertFreq` 式——DVFS 改频后按新频率
/// 重新换算，时间片实际时长不随频率漂移）；`clock_read`（3-1）由此换算纳秒。
pub fn cntvct() -> u64 {
    let v: u64;
    // SAFETY: 只读计数器。
    unsafe { core::arch::asm!("mrs {0}, cntvct_el0", out(reg) v, options(nostack, preserves_flags)) };
    v
}

/// 读已编程的比较截止 `CNTV_CVAL_EL0`（周期数）。[`arm_slice`] 的 deadline
/// 单调夹取输入（S-6：防块驱动快速切换的 deadline 跑步机饿死 tick）。
pub fn cval() -> u64 {
    let v: u64;
    // SAFETY: 只读比较寄存器。
    unsafe { core::arch::asm!("mrs {0}, cntv_cval_el0", out(reg) v, options(nostack, preserves_flags)) };
    v
}

/// 读计数器频率 `CNTFRQ_EL0`（Hz；QEMU virt 实测 62.5MHz，**不硬编码**）。
/// 独立于 [`init`](fn.init.html) 的缓存——`clock_read` 在 boot 早期（timer init
/// 前）也可能被调，直接读寄存器无顺序依赖。
pub fn cntfrq() -> u64 {
    let v: u64;
    // SAFETY: 只读计数器频率寄存器。
    unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, options(nostack, preserves_flags)) };
    v
}

/// 一个时间片的周期数（`FREQ / SLICE_HZ`；[`init`](fn.init.html) 后有效）。
/// 时间片授予量（grant）的唯一来源——class 层不再持有 tick 数常量。
pub fn slice_cycles() -> u64 {
    // SAFETY: 单核顺序读；init 先于一切调度路径。
    unsafe { SLICE }
}

fn read_cntfrq() -> u64 {
    let v: u64;
    unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, options(nostack, preserves_flags)) };
    v
}
