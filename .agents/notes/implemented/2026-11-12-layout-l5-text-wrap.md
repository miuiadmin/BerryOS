# Agent Note: LAYOUT-L5 per-node box 与 Text wrap

Status: implemented — 2026-11-12

## Problem

L4 的四元组只作用于 Root boundary，Text 仍是约束盲单行测量；TextField 只能固定宽度，launcher 又私有 600×1800 密度阈值。后续表单、语义与 IME 都需要文本在父容器边界内可测、可绘、可恢复。

## Decision

在一次后序 measure walk 中按父容器投影 incoming BoxConstraints。只有显式登记 wrap 的 Text 消费 `max_w`；wrap 状态放在 32 项 runtime-only 侧表，不进 Node/Props/AST/wire。测量与绘制共用固定容量 `WrapPlan`，贪心支持空格/CJK/紧急断行。TextField 新增 `shrink_to_width`，以 token-derived 下限和构造宽度上限走既有 `PanelProps.w` damage 链。launcher 删除私有阈值，统一 `Density::for_viewport`，并由 gui_guard 静态执法。

## Alternatives considered

- **直接实现完整 layout v2 negotiation**：需要 flex grow/shrink、嵌套协商和 per-node cache，破坏本刀单遍测量与 L1..L4 零漂；被否。
- **wrap 布尔进 TextProps**：AST bytes=860 与旧流零漂立即破坏；被否。
- **place 后把实际 bounds 回灌 measure**：首帧不确定且引入帧滞后；被否。
- **flex slot 当 measure box**：L2/L3 place 分配会被伪装成约束，golden 漂移；被否。
- **TextField 直接改 Clip/port 尺寸**：绕开 Panel 几何 damage 单源；被否。

## Consequences

wrap 未登记的树保持 L4 语义；登记 wrap 的 Text 每次显式 dirty layout 只调用一次 `measure_with_box`。paint 仍发一个 GlyphRun，多行由 GlyphPos 自带 x/y 表达。TextField 仍是单行；TextArea/多行光标/比例字体高级断行后置。600..1000 宽且高 ≥1800 的桌面分叉区现在 fail-closed 为 Compact。

Adversarial audit corrections landed with this note:

- Pad now subtracts both sides before projecting child boxes.
- Grid projects one place-equivalent track, not full grid content width.
- Swapping TextMetrics dirties the full tree and records full damage.
- Parent geometry changes explicitly re-dirty wrapped descendants.
- Trailing/break spaces do not contribute measured width and are not emitted.

Final budget and exact image SHA are recorded in the L5 specification.

## Related

- [BerryUI-LAYOUTL5PerNodeBoxTextWrap-v1.0.md](../../../docs/BerryUI-LAYOUTL5PerNodeBoxTextWrap-v1.0.md)
- [GUI架构协议完善与Rust实施计划-v1.0.md](../../../docs/GUI架构协议完善与Rust实施计划-v1.0.md)
- [架构决策.md](../../../docs/架构决策.md) ADR #86
