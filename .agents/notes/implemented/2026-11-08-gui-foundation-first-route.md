# Agent Note: GUI 转向地基优先路线

Status: implemented — 2026-11-08 用户裁决地基优先；路线已入 docs 与规则，后续代码按 F0..F9 执行

## Problem

五个 Native Shell 已验证 WM、所有权、输入、合成与生命周期真链，但 UI 层由各 app 手写 WM client、VMO、像素、字体与布局。该做法能产出样例，却会复制五套临时框架，导致视觉质量、预算和 SDK 演进不可持续。

## Decision

GUI 执行序改为地基优先：先建 `berry-shell`、夯实 `berry-ui` core、主题与基础组件，再用 `uitest` 验收框架，最后迁移 Settings、Files/Contacts、Photos/Browser。`docs/GUI框架地基路线-v1.1.md` 是执行序权威，`docs/规则/14-GUI与UI框架.md` 是开工铁律。现有五个 Native Shell 只作迁移前兼容基线，不能作为新 app 模板。

## Alternatives considered

- **继续逐 app 手写 Native Shell**：能快速增加样例，但每 app 复制系统客户端与 UI 绘制，视觉与预算债持续扩大；否决。
- **先做 Camera/MMS 扩面**：新增 app 会强化错误模板；在 `uitest` 与首个迁移 app 收口前冻结。
- **直接重写 berry-ui**：会忽略 GUI-3/GUI-6 已冻结契约与既有实现；改为先盘点差距、再小步补齐。

## Consequences

GUI 目标形态与既有 ADR 不变；变化的是实施顺序。F6 起 GUI app 不得直接接触 WM RPC、VMO、stride 或像素。每个里程碑必须通过 Host 单测、离屏 golden、Guest 真链与迁移回归四层证据，并检查 rootfs 与 `.bap` 预算。`berry-nshfont` 是迁移期临时件，F9 后拆归 shell/gfx/ui 并删除。

## Related

- [GUI框架地基路线-v1.1.md](../../../docs/GUI框架地基路线-v1.1.md)
- [14-GUI与UI框架.md](../../../docs/规则/14-GUI与UI框架.md)
