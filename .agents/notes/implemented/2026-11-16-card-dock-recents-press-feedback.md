# Card、Dock 与 Recents 按压反馈

Status: implemented

## Problem

列表行、按钮与 Tabs 已有真实换色反馈，但 `Card` 只是静态容器；常驻 Dock 的 Argb 图标不能进 press_map（legal visual 只覆盖 Rect/Text），SYSTEM Recents 复合卡片也没有 Press/Release/Cancel 视觉。用户点名的“按压效果、卡片反馈”在系统壳两个高频表面上仍缺席。

## Decision

`Card` 保持表面容器职责，新增 `set_pressed` 与 `on_msg`：Pressed 主题 token 换底，并给 Panel 施加 2dp paint offset；宽高和兄弟布局零漂。Click 仍由 app 解释，Card 不拥有业务动作。BerryDemo Widgets 页保存并分发 Card 表面消息。

launcher 增加 `desktop::set_surface_press`：Dock Argb 图标按命中节点施加 2px offset；Recents 卡片按 Panel 换底并施加 2px offset。该路径不新增 a11y/SEM 锚、不扩 `PRESS_MAP_MAX`、不改命中表；wmd/launcher 仍是 SYSTEM 壳语义唯一真相。Press/Release/Cancel/Click 都会先走该视觉面，再进入原有 spawn/task select 业务。

产品 e2e 增加 `shell-press` suite：按住 Dock Contacts 位，断言 `SHELLPRESS pressed=true`、局部像素变化、释放后 `pressed=false` 两次、真实 Contacts 启动与 HOME 恢复。Recents suite 同步断言任务卡 Press on/off 标记。

## Alternatives considered

- 缩小 Card/Dock/Recents 的宽高：会触发 GEOM 重排并移动兄弟与嵌套内容，拒绝。
- 给 Argb Dock 图标套 Rect press_map：press_map 的 legal visual 不含 Icon，且会伪造第二视觉表，拒绝。
- 在 wmd 中改 SYSTEM 壳颜色：跨过 launcher 对自有 SYSTEM 表面的所有权，拒绝。
- 用 wall-clock 动画做涟漪/缩放：当前帧引擎没有 scale transform 和通用动画语义，会伪造反馈，拒绝。

## Consequences

Host 钉板证明 Card Press 使用真实 theme token、offset 后 `measured=0`、宽高与兄弟 bounds 不变，Release 恢复。产品 `shell-press` 与 Recents e2e 通过，Contacts 真启动并 HOME 恢复，无 panic。通用 scale transform、涟漪、swipe actions 与长按卡片多选仍未完成，不能宣称三方按压体系全覆盖。

## Related

- [OpenHarmony 式产品系统壳](2026-11-15-openharmony-product-shell.md)
- [三方共性控件体系](2026-11-15-tri-platform-controls.md)
- [产品 e2e rig 收编](2026-11-16-product-e2e-rig.md)
