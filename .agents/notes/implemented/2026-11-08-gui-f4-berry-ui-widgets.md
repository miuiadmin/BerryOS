# Agent Note: GUI F4 落地 BerryUI 组件第一版

Status: implemented — 13 组件、Panel 基元、SDK 导出与 Host/离屏证据已全绿

## Problem

F2/F3 后 BerryUI 已有可测 core 与主题 token，但 F6 迁移普通 app 前仍缺稳定组件面。若直接迁移 Settings/Files，每个 app 会重新组合 Col/Row/Rect/Text 并复制主题状态，形成新的手写 UI 层。专业 CanvasSurface 与 immersive 又尚未定协议，组件刀不能借“未来复杂软件”扩界。

## Decision

按 [GUI框架地基路线-v1.10.md](../../../docs/GUI框架地基路线-v1.10.md) 完成 F4 普通应用组件集：Scaffold、AppBar、Card、Column、Row、Text、Icon、Button、ListItem、Switch、Divider、Scroll、EmptyState。新增 add-only `ViewKind::Panel`（kind=10，23B props）作为可绘背景容器；组件模块只组合 ViewSpec/ThemeMetrics/a11y 与 Msg 辅助，不新增第二套布局/渲染/输入算法；FillRoundRect 与 F3 direct surface 共用 coverage 单源。所有组件经 `berry_ui::sdk` 导出；Button/Switch 不承载业务回调，Click 后由 app 更新状态；ScrollView 使用 offset clamp，明确无 clipping，裁剪归 P0。

## Alternatives considered

- **直接用 Col/Rect/Text 组合迁移 app**：会复制组件语义和主题状态，重复 F6 前手写 UI；否决。
- **为 Card/Button 新增私有背景绘制循环**：违普通 app 禁像素与 core 单源；否决，新增 Panel 保留树 kind。
- **修改既有 Rect/Col props 承载背景容器**：破坏 GUI-6 Props/TLV 冻结面；否决，按新 kind add-only。
- **在 F4 实现 CanvasSurface/immersive**：P0 未定协议，归后续；否决。
- **让 Button/Switch 持业务闭包**：引入回调/分配和业务泄漏；否决，Msg-only。

## Consequences

普通 BerryUI app 有了稳定组件 API，但 F5 uitest 仍是首个真实 guest 消费者，F4 不宣称应用迁移完成。Panel 进入 AST 值域后未知 kind 拒面顺延为 11；旧 kind 0..=9 流不变。ScrollView 当前不裁剪溢出，F5 页面必须 fitting，跨视口 clipping 等 P0 协议定形。组件未被 guest 引用时 LTO 裁剪保持 rootfs 在帽内；F5 消费后需重出包体与内存账。

## Related

- [BerryUI-F4组件第一版-v1.1.md](../../../docs/BerryUI-F4组件第一版-v1.1.md)
- [BerryUI-F2核心盘点-v1.0.md](../../../docs/BerryUI-F2核心盘点-v1.0.md)
- [BerryDesign-F3主题与基元-v1.0.md](../../../docs/BerryDesign-F3主题与基元-v1.0.md)
