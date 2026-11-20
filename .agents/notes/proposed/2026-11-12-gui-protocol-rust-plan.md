# Agent Note: GUI 协议完善总线与 Rust 实施计划

Status: proposed — PLAN-G0 计划已落档，RUN-K1 实施未开工

## Problem

GUI-T1、SEM-A1、DESIGN-D2 与 LAYOUT-L4 的规格各自留下“后置”清单，GUI-14/GUI-15 还存在 keyed re-apply、生命周期性质测试、窗口 capability record、presented-frame 反馈、输入活性 watchdog、显示元数据词表与 SDK 文档分层等未排程义务。LAYOUT-L4 落地后，原 GUI-15 优先队列耗尽，路线与规则 14 没有下一门 owning milestone；继续把后置项散落在各规格中会导致下一刀程序上不可开工，也会让 IME、语义、主题与布局在未固化的树重应用和焦点生命周期上重复堆协议。

## Decision

采用 `docs/GUI架构协议完善与Rust实施计划-v1.0.md` 作为 PLAN-G0 后置项总账，并在路线 v1.20 中排入后续 ladder：RUN-K1、LAYOUT-L5、LAYOUT-L6、WM-R4、DESIGN-D3、SEM-A2、IME-1、NAV-1、WM-R5、GFX-F1、DOC-D3。计划把每项归类为协议刀、Rust 实现刀、contract-lite SDK 面或明确延后，并给出文件面、验收门、预算策略与停止条件。

第一实施刀固定为 RUN-K1：stable-key re-apply、key 冲突 loud reject、settle cap、apply/reset/remove/focus 生命周期性质测试、UIContext seam 盘点与 gui-wmd host harness 第一版。后续协议扩张必须等这些 runtime 性质被机器夹具证明后再开始。

## Alternatives considered

- **把所有后置项合成一次大 GUI 协议重构**：会同时改 berry-ui、Room、wmd、wire、主题与 app，违反小步可验证与上一出口门未过不得开下一门；被否。
- **直接先做 IME 或多窗口**：两者都消费稳定的树重应用、焦点生命周期、capability 与 insets 契约，跳过 RUN-K1 会把同类回归分散到后续每刀；被否。
- **只在各规格保留后置节，不建总账**：无法表达依赖方向、协议/实现边界与预算停止条件，下一刀仍无程序入口；被否。
- **先扩 UI 视觉与应用面**：继续在未固化协议上堆界面，违背用户“从底层夯实”的裁决；被否。

## Consequences

PLAN-G0 本身不引入代码、opcode、AST 字段、SDK facade 或运行时行为。后续每一刀仍须先改路线与规则 14，再写规格与账本，最后实施；协议刀必须提供纯数据模型、host codec、旧流零漂与拒面，Rust 实现刀不得顺手改 AST/wire。rootfs0 已到 32MiB 单卷帽，后续字形与大资产只能走 rootfs1 或独立卷，不得降低壁纸或图标质量换空间。

若 RUN-K1 证明 stable-key 身份无法在 settle cap 内保持 full-rebuild oracle 等价，应先修订本计划与路线 v1.20，再进入 LAYOUT-L5；若更早出现真实协议消费者，也不得静默跳过 RUN-K1。

## Related

- [GUI架构协议完善与Rust实施计划-v1.0.md](../../../docs/GUI架构协议完善与Rust实施计划-v1.0.md)
- [GUI框架地基路线-v1.20.md](../../../docs/GUI框架地基路线-v1.20.md)
- [规则/14-GUI与UI框架.md](../../../docs/规则/14-GUI与UI框架.md)
- [架构决策.md](../../../docs/架构决策.md) ADR #84
