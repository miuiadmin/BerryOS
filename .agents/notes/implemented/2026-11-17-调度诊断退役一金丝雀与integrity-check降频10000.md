# 调度诊断退役一:金丝雀与 integrity-check 降频 10000

Status: implemented

## Problem

`bump_switches` 金丝雀打印（`SOAK_PRINT_EVERY`）与 `integrity_check` 消费点同频运行在切换/IRQ 上下文：每 100 次切换触发一次 `PRINT_LOCK` + UART 输出行，是 IRQ 窗内的周期性 µs 级尖峰（切换点可能在 IRQ 上下文，打印在 IRQ 屏蔽窗内拼行原子——S-2 E0 纪律）。G1/S-6 里程碑已过，bring-up 级诊断密度对现役系统是纯税。

出处：[04 调度热路径 Top-5 审计](../../../research/内核深度进化/04-调度热路径-Top5优化机会-v1.0.md) 机会二（诊断退役）。落笔笔：0ea3d11ad（内核优化 Top-3 快赢，沿革二百九十四）。

## Decision

- `SOAK_PRINT_EVERY` 100→10000：每 100 切换的 `PRINT_LOCK`+UART 行是 IRQ 窗内周期性 µs 级尖峰；金丝雀语义（计数持续涨）在 10000 档完整保留。
- 计数面零变化：`SWITCHES` 每 switch 递增不动；watchdog/soakmon 走 `switches()` 计数器直读，与打印频率解耦。
- `integrity_check` 消费点同频降：`on_irq_tick` 金丝雀分支随 `bump_switches` 跨点，2026-11-17 起每 10000 切换一次（O(n) 极小）；ktest 调用面不动。

## Verification

- 金丝雀语义验证：10000 档下跨点打印仍在位（`sw % SOAK_PRINT_EVERY == 0` 分支零改动），smoke soakmon 走 `switches()` 直读不受影响。
- 沿革二百九十四节记载三快赢之一；本笔记补落配对（0ea3d11ad 分笔三件中原两篇笔记在位，本篇补齐第三面）。

## Related

- 同批快赢：[zero_frame-DC-ZVA清零臂](2026-11-17-zero_frame-DC-ZVA清零臂.md)、[preempt-checkpoint-len门控](2026-11-17-preempt-checkpoint-len门控-SVC尾检查点免锁前置过滤.md)
- 机会出处：[04 调度热路径 Top-5 审计](../../../research/内核深度进化/04-调度热路径-Top5优化机会-v1.0.md) 机会二
