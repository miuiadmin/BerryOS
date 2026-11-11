# Agent Note: GUI F3 落地 Berry Design 主题与基元

Status: implemented — Theme/ThemeMetrics、BerryGFX surface、berry-nshfont 单源复用与双几何 golden 已全绿

## Problem

F2 后 BerryUI core 可测，但视觉常量仍散在调用方；五个迁移前 Native Shell 的 XRGB 混合、圆角、圆与阴影算法重复在 `berry-nshfont`，BerryGFX 命令路径另有圆 coverage。F4 组件若直接消费这些散装常量或临时 API，会提前形成第三套 UI 框架。

## Decision

按 [GUI框架地基路线-v1.14.md](../../../docs/GUI框架地基路线-v1.14.md) 完成 F3。新增 `berry-ui/src/theme.rs`：`Theme::light()` 提供语义色，`InteractionState` 提供 Normal/Pressed/Disabled/Selected 三件状态色，`ThemeMetrics` 以 quarter-vp token 统一 @2x/@3x 间距、圆角与排版换算，并导出到 `berry-ui::sdk`。新增 `berry-gfx/src/surface.rs`：XRGB little-endian straight-alpha 混合、3×3 超采样圆角矩形/圆和双层轻阴影；DisplayList 圆命令与直接表面路径共用 `circle_coverage` 单源。`berry-nshfont` 保留迁移期签名，但 HD A8 文本混合与形状基元全部转发 BerryGFX。

## Alternatives considered

- **在 app 中继续散装常量**：违背地基优先路线，且 F4 组件会复制更多色值；否决。
- **把主题放进 berry-gfx**：让纯光栅层认识视觉语义，破坏分层；否决。
- **只写文档不迁移 berry-nshfont**：形状算法仍有两套，F9 拆分时会产生行为分叉；否决。
- **直接实现 Button/Card/Switch**：组件属 F4，F3 缺 token 与基元时测试只能 mock；否决。

## Consequences

F4 组件必须从 `berry-ui::sdk` 消费 Theme/ThemeMetrics，不得复制颜色、字号、间距、圆角字面量。现有五个 Native Shell 仍是迁移前兼容基线，其页面字面量待 F6..F8 迁移，不作为新模板。`berry-nshfont` 仍保留 HD 字形 VMO 映射与文本定位，F9 删除前不再新增基元。主题当前只有 light 档；dark 主题后续以同级构造口加入，不改变组件形状。

## Related

- [BerryDesign-F3主题与基元-v1.0.md](../../../docs/BerryDesign-F3主题与基元-v1.0.md)
- [BerryUI-F2核心盘点-v1.0.md](../../../docs/BerryUI-F2核心盘点-v1.0.md)
