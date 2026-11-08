# 14-GUI 与 UI 框架

> 一句话定位：动 berry-gfx / berry-ui / berry-room / berry-launcher / Berry Room app 前先读本文。执行序单一事实源 = [../GUI框架地基路线-v1.3.md](../GUI框架地基路线-v1.3.md)。

## 域铁律

- **地基优先**：按 F0..F5、P0 Surface Mode、F6..F9 顺序建设；上一出口门未过，不得为视觉效果或新 app 绕开下一层。Camera、MMS 等新 Native Shell 在 `uitest` 与首个迁移 app 收口前不得继续扩张。
- **分层不越界**：`berry-gfx` 只做光栅；`berry-ui` 只做树、布局、状态、主题与组件；`berry-room` 只做 Berry Room 应用运行室；app 只做业务状态与页面语义。禁止反向依赖。
- **app 不画像素、不接 WM**：F6 起，GUI app 源码禁止 `berry_channel_*`、`berry_vmar_map`、`berry_vmo_*`、WM RPC 编解码、`put_px/fill/round_rect/circle`、glyph 循环与 stride 计算。迁移前五个 Native Shell 仅作兼容基线，只允许阻断性修复。
- **视觉常量单源**：颜色、字号、间距、圆角、阴影、按压/禁用/选中态进主题 token；组件样式进 `berry-ui`。禁止在 app 内散装硬编码视觉常量。
- **BUI 可选且单 AST**：BUI 只是 BerryUI 的可选声明式入口；F4/F5 先用 Rust `ViewSpec`/`DescBuilder` 与 AST TLV。`.bui` 现行仅是包资源占位，不解析、不执行、不构成 GUI app 必需文件；未来 BUI 也不得承载业务流程、WM/VMO/syscall、像素绘制或命令式副作用。
- **一个系统契约，多个 UI 形态**：BerryUI 是默认声明式工具包，不是特权或强制渲染器。普通应用走 BerryUI；专业应用走 BerryUI 外壳 + CanvasSurface；游戏/实时应用走 immersive surface 与自有 renderer。自绘路径仍必须经 Berry Room 获得 surface/input/IME/a11y/帧率能力，不得直写屏或绕过焦点与进程清理。
- **系统字体单源**：app 不得自带字体表。v1 映射恒 UserRw，字形 VMO 由系统按 app 私有供给，防跨 app 写穿；未来引入只读映射或字体服务时先改 GUI 地基路线。
- **真链不降级**：任何 UI SDK 或视觉改动不得削弱 APP_SPAWN、WM 所有权、owner-local 输入、composition、frame lifecycle、进程死亡清理与真实数据链。
- **测试四层齐全**：Host 单元、离屏 golden、Guest `uitest`、迁移 app 回归缺一不可。视觉验收必须同时有 marker、真实帧、截图像素/色数锚、布局 dump 与输入命中证据。
- **预算先于美观**：每里程碑检查 rootfs 与 `.bap` 尺寸；超帽即停。不得降低壁纸或图标质量换取空间，不得让每个 app 复制字体、图标解码或大图缓存。
- **临时件有退役日**：`berry-nshfont` 只存在于迁移期；F9 后映射归 `berry-room`、绘制归 `berry-gfx`、文本消费归 `berry-ui`，该 crate 删除。

## 开工清单

- [ ] 当前改动落在 F0..F5、P0 Surface Mode 或 F6..F9 哪一门？上一门出口是否已过？
- [ ] 是否让 app 直接接触 syscall、WM、VMO、stride 或像素？
- [ ] 视觉变化是否进入主题 token / 组件 / `berry-gfx` 基元，而不是 app 私有代码？
- [ ] 是否把 `.bui` 当成 GUI app 必需入口、第二事实源或副作用脚本？
- [ ] 专业 canvas / 游戏 immersive 路径是否仍经 Berry Room surface 契约，并保留输入、IME、无障碍、帧率与安全区语义？
- [ ] 是否有 Host 单测、离屏 golden、Guest 真链与迁移回归四层证据？
- [ ] rootfs、`.bap` 尺寸与既有 GUI8/smoke-image 门是否全部复跑？
- [ ] 分层、顺序或边界变化是否先更新 [../GUI框架地基路线-v1.3.md](../GUI框架地基路线-v1.3.md)？

## 深读

- [../GUI框架地基路线-v1.3.md](../GUI框架地基路线-v1.3.md) —— F0..F5、P0 Surface Mode、F6..F9 执行序、分层契约、App 禁止清单、测试门与预算纪律。
- [../BerryOS命名体系-v1.1.md](../BerryOS命名体系-v1.1.md) —— 公共架构名与 BUI 可选入口/同一 AST 边界。
- [../GUI里程碑规划-2026-10-v1.0.md](../GUI里程碑规划-2026-10-v1.0.md) —— M-GUI 目标形态、GUI-0..9 长期架构与既有 ADR 依据。
- [../GUI-3实施设计草案-v1.1.md](../GUI-3实施设计草案-v1.1.md) —— `berry-ui` 保留树、脏区、帧循环与 impl 契约。
- [../BerryUI-F2核心盘点-v1.0.md](../BerryUI-F2核心盘点-v1.0.md) —— berry-ui core 现状、布局/命中诊断与 SDK 最小冻结面。
- [../BerryDesign-F3主题与基元-v1.0.md](../BerryDesign-F3主题与基元-v1.0.md) —— Berry Design token、密度换算、交互状态与 BerryGFX 表面基元单源。
- [../GUI-12-复杂应用与游戏UI形态深研-2026-11.md](../../research/GUI-12-复杂应用与游戏UI形态深研-2026-11.md) —— 专业软件、工业框架、游戏 UI 与 OS 分层证据；P0 Surface Mode 的裁决依据。
- [../GUI渲染调研-2026-10-v1.0.md](../GUI渲染调研-2026-10-v1.0.md) —— 渲染路线与自研光栅决策依据。
