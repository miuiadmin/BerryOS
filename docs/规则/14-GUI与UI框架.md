# 14-GUI 与 UI 框架

> 一句话定位：动 berry-gfx / berry-ui / berry-room / gui-wmd / guifbd / gui-inputd / berry-launcher、普通 GUI app、专业 CanvasSurface 客户端或游戏 immersive 客户端前先读本文。执行序单一事实源 = [../GUI框架地基路线-v1.4.md](../GUI框架地基路线-v1.4.md)。

## 域铁律

- **地基优先**：按 F0..F5、P0 Surface Mode、F6..F9、C1 嵌套裁剪顺序建设；上一出口门未过，不得为视觉效果或新 app 绕开下一层。Camera、MMS 等新 GUI app 必须直接走 BerryUI；普通 Retained 页面不得回到自建 WM/VMO/像素路径。
- **P0 编号局部化**：GUI P0 只指 F5 与 F6 之间的 Surface Mode 协议门，不改变路线图、调点调度或 Berry 设备壳的同名阶段号。
- **分层不越界**：`berry-gfx` 只做光栅；`berry-ui` 只做树、布局、状态、主题与组件；`berry-room` 只做应用运行室；Berry Session 服务只做窗口、呈现、输入、焦点、层序与帧率仲裁；app 只做业务语义或自有文档/引擎渲染。
- **一个系统契约，多个 UI 形态**：BerryUI 是默认声明式工具包，不是特权或强制渲染器。普通应用走 BerryUI；专业应用走 BerryUI 外壳 + CanvasSurface + 应用文档渲染器；游戏/实时应用走 immersive surface 与自有 renderer。
- **app 不私接系统机制**：任何 GUI app 不得直接调用 `berry_channel_*`、`berry_vmar_map`、`berry_vmo_*`，不得解析 WM RPC、计算 frame stride 或直写全局 framebuffer/display MMIO。普通 BerryUI app 还不得出现 `put_px/fill/round_rect/circle/glyph` 像素循环；专业/游戏 renderer 的像素算法只能写 P0 授权的 CanvasSurface / immersive surface。
- **语义旁车不可降级**：专业 canvas 与游戏 immersive 可以绕过 BerryUI 控件，但不能绕过 Berry Room 的 input class、IME composition、a11y sidecar、safe area/scale factor、帧率意图、焦点与进程死亡清理。
- **视觉常量单源**：颜色、字号、间距、圆角、阴影、按压/禁用/选中态进主题 token；BerryUI 组件样式进 `berry-ui`。专业/游戏引擎内部艺术风格可自有资产，但宿主外壳不得散装复制系统 token。
- **BUI 可选且单 AST**：BUI 只是 BerryUI 的可选声明式入口；F4/F5 先用 Rust `ViewSpec`/`DescBuilder` 与 AST TLV。`.bui` 现行仅是包资源占位，不解析、不执行、不构成 GUI app 必需文件；未来 BUI 不得承载业务流程、WM/VMO/syscall、像素绘制或命令式副作用。
- **结构性裁剪单源**：普通 Retained 子树裁剪用 BerryUI `ViewKind::Clip`；BerryGFX 只提供 DisplayList Push/PopClip 光栅语义。普通 app 不得直录 Push/PopClip，专业 canvas/immersive 的裁剪仍归 P0 授权 surface。
- **系统字体单源**：app 不得自带字体表。v1 映射恒 UserRw，字形 VMO 由系统按 app 私有供给，防跨 app 写穿；普通 BerryUI guest 启用 `guest-font-vmo` 并注入 FontSource，DisplayList 光栅化归 Berry Room。未来引入只读映射或字体服务时先改 GUI 地基路线。
- **真链不降级**：任何 UI SDK、Surface Mode 或视觉改动不得削弱 APP_SPAWN、WM 所有权、owner-local 输入、composition、frame lifecycle、进程死亡清理与真实数据链。
- **测试门齐全**：F4/F5 走 Host、离屏、Guest、迁移回归四层证据；P0 另须覆盖 Surface role/mode、input class、IME、a11y、metrics 的编解码、round-trip、拒面与不变量。
- **预算先于美观**：每里程碑检查 rootfs 与 `.bap` 尺寸；超帽即停。不得降低壁纸或图标质量换空间，不得让每个 app 复制字体、图标解码或大图缓存。
- **临时件已退役**：`berry-nshfont` 只存在于 F6..F8 迁移期；F9 后映射归 `berry-room`、绘制归 `berry-gfx`、文本消费归 `berry-ui`，该 crate 已删除，`just gui-boundary-guard` 禁止回潮。

## 开工清单

- [ ] 当前改动落在 F0..F5、P0 Surface Mode 或 F6..F9 哪一门？上一门出口是否已过？
- [ ] 普通 BerryUI app 是否只声明结构、状态、消息与授权数据能力？
- [ ] 专业 canvas / 游戏 immersive 是否使用 P0 冻结的 Surface role/mode，而非自造窗口或全局屏写？
- [ ] 自绘路径是否保留 input class、IME composition、a11y sidecar、safe area/scale factor、帧率意图、焦点与死亡清理？
- [ ] 是否让 app 直接接触 syscall、WM RPC、VMO、stride、全局 framebuffer 或 display MMIO？
- [ ] 视觉变化是否进入主题 token / BerryUI 组件 / `berry-gfx` 基元，而不是普通 app 私有代码？
- [ ] 是否把 `.bui` 当成 GUI app 必需入口、第二事实源或副作用脚本？
- [ ] 是否有 Host 单测、离屏 golden、Guest 真链、P0 协议拒面与迁移回归证据？
- [ ] rootfs、`.bap` 尺寸与既有 GUI8/smoke-image 门是否全部复跑？
- [ ] 分层、顺序或边界变化是否先更新 [../GUI框架地基路线-v1.4.md](../GUI框架地基路线-v1.4.md)？

## 深读

- [../GUI框架地基路线-v1.4.md](../GUI框架地基路线-v1.4.md) —— F0..F5、P0 Surface Mode、F6..F9 执行序、分层契约、App 禁止清单、测试门与预算纪律。
- [../BerryOS命名体系-v1.1.md](../BerryOS命名体系-v1.1.md) —— 公共架构名与 BUI 可选入口/同一 AST 边界。
- [../GUI里程碑规划-2026-10-v1.0.md](../GUI里程碑规划-2026-10-v1.0.md) —— M-GUI 目标形态、GUI-0..9 长期架构与既有 ADR 依据。
- [../GUI-3实施设计草案-v1.1.md](../GUI-3实施设计草案-v1.1.md) —— `berry-ui` 保留树、脏区、帧循环与 impl 契约。
- [../BerryUI-F2核心盘点-v1.0.md](../BerryUI-F2核心盘点-v1.0.md) —— berry-ui core 现状、布局/命中诊断与 SDK 最小冻结面。
- [../BerryDesign-F3主题与基元-v1.0.md](../BerryDesign-F3主题与基元-v1.0.md) —— Berry Design token、密度换算、交互状态与 BerryGFX 表面基元单源。
- [../BerryUI-F4组件第一版-v1.1.md](../BerryUI-F4组件第一版-v1.1.md) —— F4 组件 API、Panel 基元、状态/输入语义、离屏 golden 与预算。
- [../BerryUI-F5系统验收-v1.0.md](../BerryUI-F5系统验收-v1.0.md) —— F5 真实 guest 消费者、系统 FontSource、Room DisplayList 光栅化、GUI8 真链验收与预算边界。
- [../BerrySession-P0SurfaceMode协议-v1.0.md](../BerrySession-P0SurfaceMode协议-v1.0.md) —— P0 Surface role/mode、input class、IME composition、a11y sidecar、safe area/scale、WM 0x67 codec 与三路径不变量。
- [../BerrySettings-F6迁移-v1.0.md](../BerrySettings-F6迁移-v1.0.md) —— F6 首个 Native Shell 迁移：Berry Room + BerryUI、真实 WLAN/Bluetooth 状态重绘、GUI8 layout-driven 验收与两帽预算。
- [../BerryFilesContacts-F7迁移-v1.0.md](../BerryFilesContacts-F7迁移-v1.0.md) —— F7 Files/Contacts 迁移、真实数据链、动态 layout 验收与并发 per-window destroy 语义。
- [../BerryBrowserPhotos-F8迁移-v1.0.md](../BerryBrowserPhotos-F8迁移-v1.0.md) —— F8 Browser/Photos 迁移、图像 IconSource 链、诚实 Browser 壳、ReleaseWait RETURN-ALL 清账与回收帽修正。
- [../BerryGUILanding-F9收口与守卫-v1.0.md](../BerryGUILanding-F9收口与守卫-v1.0.md) —— F9 nshfont 删除、普通 GUI app 静态守卫、SDK/文档收口与全量回归。
[../BerryUIClipping-C1嵌套视口-v1.0.md](../BerryUIClipping-C1嵌套视口-v1.0.md) —— C1 BerryGFX Push/PopClip、BerryUI 结构性 Clip、ScrollView/Swipe 裁剪、hit/damage 与 GUI8 真链验收。
- [../../research/GUI-12-复杂应用与游戏UI形态深研-2026-11.md](../../research/GUI-12-复杂应用与游戏UI形态深研-2026-11.md) —— 专业软件、工业框架、游戏 UI 与 OS 分层证据；P0 Surface Mode 的裁决依据。
- [../GUI渲染调研-2026-10-v1.0.md](../GUI渲染调研-2026-10-v1.0.md) —— 渲染路线与自研光栅决策依据。
