# GUI F6 Settings 迁移 BerryUI

Status: implemented — 2026-11-08

## Problem

P0 后，Settings 仍是迁移前 Native Shell：自己解析 WM RPC、创建/映射 frame VMO、计算 stride，并用 berry-nshfont 的 fill/round_rect/circle/text 循环绘制控件。若把它作为模板继续扩张，GUI 地基路线会被旧手写 UI 重新侵蚀。

## Decision

将 Settings 迁到 Berry Room + BerryUI 正常路径：

1. 生命周期归 `AppRoom`：APP env、WM session、窗口、frame VMO、owner-local 输入、submit、destroy 与系统字形 VMO。
2. UI 归 `berry_ui::sdk`：Scaffold/AppBar/ScrollView/Card/Button/Switch/ListItem/Divider/Text/Row/Column，视觉只来自 Berry Design Theme 与 ThemeMetrics。
3. app 只持有 `wlan/bluetooth/toggles/frame_seq`；Switch/Button 的 Msg 由 app 处理并写回业务状态，组件不携带业务回调。
4. 启用 `guest-font-vmo`，S32 文本走 `SYSTEM_FONT_ATLAS`；DisplayList 由 BerryUI 发射、`AppRoom::render_display_list` 光栅化。
5. GUI8 Settings 段改为解析 `SETTINGSLAYOUT` 的真实 bounds 并动态点击 Back/WLAN/Bluetooth；截图改锚 Berry Theme 与系统 A8，不再使用旧手写颜色和固定坐标。ListItem detail 用 Body，确保系统 32×48 字形真实上屏。

## Alternatives considered

- 保留旧 Settings 页面，只替换底层 WM 调用：仍会在 app 内保留第二套布局/像素/控件状态，违背 F6 出口；否决。
- 用 BerryUI 低阶 Col/Rect/Text 手组合页面：会绕开 F4 组件语义，形成新的散装 UI；否决。
- 让 Switch 持有业务回调：组件将拥有设置业务状态，破坏 app/组件边界；否决。
- 为控预算降低图标/文本质量：违反 GUI 路线预算纪律；改按 A1 单卷/总量两帽记账，否决。

## Consequences

- Settings 源码不再直接引用 berry-sys/berry-guiproto/berry-nshfont，也无 WM RPC、VMO、stride 或像素循环。
- `SETTINGS.bap` 27,520B→68,896B；rootfs 总量 33,594,856B，rootfs0 恰为 33,554,432B 单卷帽，rootfs1 40,424B，总量仍在 128MiB 帽内。
- GUI8 normal 证明两次真实 owner-local Switch 点击、精确 `wlan/bluetooth` 状态、三帧真实 submit、Berry Theme 像素锚、系统 A8 色数、Back hit 与 destroy/exit 全链不回退。
- v1 仍未接驱动或持久化；WLAN/Bluetooth 只改变进程内状态。系统级设置服务归后续应用模型/服务刀。
- 规格见 [BerrySettings-F6迁移-v1.0](../../../docs/BerrySettings-F6迁移-v1.0.md)。
