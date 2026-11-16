//! oneshot 收紧律（忙核 arm-to-nearest-deadline）——tick/授片两臂共用的
//! 纯决策律单源，host 可测钉板（`sched/host/oneshot_law_test.rs` 经
//! `#[path]` mod 同源编译；无 HAL/asm 依赖——本文件保持纯 core 面）。
//!
//! **背景（02 号调研 §1.3 尾「留后」注 + timer.rs 模块头「授片处取 min 仍
//! 留后（Phase 4+）」）**：idle/park 路径（runqueue 空）早已按 SLEEPQ 最近
//! deadline 用 `arm_at` oneshot 精确武装（sched.rs idle_loop / secondary_main
//! park 环）；但**忙核**（有任务在跑）上 CVAL 只在 tick 入口 rearm 成满片
//! 终点（`timer::handle` → `arm_slice`），`expire_timeouts` 唯一驱动 =
//! on_irq_tick——睡眠者的唤醒粒度 = 1 slice（名义 100ms、TCG 实效 ≈200ms）。
//!
//! **律**：已知最近到期超时 `nearest` 与现编程截止 `armed`，**仅当 nearest
//! 严格早于 armed 才收紧到 nearest**；否则保持现臂（无到期/不更早 → 周期
//! tick 兜底已在场）。方向性与 `arm_slice` 的 S-6 单调夹取同向——**只在
//! 「更早」方向移动已编程截止，永不推迟**：
//! - 收紧后 slice 终点仍是硬底：每 ≤1 slice 必有一次 tick（RR.tick 记账 /
//!   slice 燃尽抢占链不丢——S-6「deadline 跑步机饿死 tick」判例的镜像防线：
//!   这里同样不允许把 CVAL 推到片终点之外）；
//! - `nearest ≤ now`（过期值）合法：`arm_at` compare 立即成立即醒，回环
//!   `expire_timeouts` 承接（park 路径同款语义——见 timer.rs `arm_at` 注）；
//! - `nearest == armed` 不重臂（同值幂等，省一次系统寄存器写）。
//!
//! 消费点（收紧律同律两臂，sched.rs；配套第三件 = on_irq_tick 超时唤醒出口
//! 检查点——expire 摘出过人且可切时 try_preempt，让严格更高 eff 的超时醒者
//! 即时直切而非等片尽）：
//! 1. `on_irq_tick` 的 `expire_timeouts` 之后——tick 处理器 oneshot 化：expire
//!    后 SLEEPQ 队头即最近到期，早于现臂（入口 rearm 的满片终点）即收紧；
//! 2. `block_current` 的忙核授片臂（出选有货支）——新睡眠者刚把 deadline
//!    推进 SLEEPQ，这是唯一看得见该截止的授片点（tick 臂只能看见「上个
//!    tick 时刻已在队」的截止——片内中途入睡的短睡者无人臂到，粒度仍
//!    1 slice；两臂合璧后 CVAL 恒 ≤ 最近已知截止）。
pub fn tighten_deadline(nearest: Option<u64>, armed: u64) -> Option<u64> {
    match nearest {
        Some(d) if d < armed => Some(d),
        _ => None,
    }
}
