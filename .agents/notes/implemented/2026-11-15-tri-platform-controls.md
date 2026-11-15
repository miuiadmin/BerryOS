# 三方共性控件第一版与应用底栏

Status: implemented

## Problem

对齐 Android Material/Compose、Apple SwiftUI/UIKit 与 OpenHarmony ArkUI 后，BerryUI 公共面缺少三方都有的基础选择与反馈件：Checkbox、Radio、Slider、定量/不定进度、Tabs、分组容器、Grid 包装与统一 SafeInsets。预制应用也仍靠手写进度条，缺少可在页面间切换的持久底栏。

## Decision

依据 [BerryUI-三方控件体系对照-2026-11-v1.0.md](../../../research/BerryUI-三方控件体系对照-2026-11-v1.0.md) 与 [BerryUI-TriPlatformControls-v1.7.md](../../../docs/BerryUI-TriPlatformControls-v1.7.md)，新增 `Checkbox`、`Radio`、`Slider`、`ProgressIndicator`、`LoadingIndicator`、`Tabs`、`Section`、`Grid` 与 `SafeInsets`。全部保持 Plain Data + `&mut Ui<N>` + NodeId，不新增布局/光栅/焦点算法；状态归 app，组件只应用视觉与语义。Tabs 支持 Top/Bottom 两档，最多 6 项，panel/icon/text 都是同一激活面。

Berry Demo 六个页面持久使用 `Tabs::Bottom`，真实 Click 在 Home/Widgets/Inputs/Lists/Theme/System 间切换；Widgets 页消费 Checkbox、Radio、Slider、ProgressIndicator 与 LoadingIndicator。系统 BACK/HOME/TASK 三键保留在 SYSTEM 底部保留区，应用底栏位于 safe_bottom 之上。

第二段补上浮层族：`OverlaySurface` 以 Ui root 后画兄弟作为唯一 z-order 事实，新增 `AlertDialog`、`Toast`、`Menu` 与 `Sheet`。Dialog/Toast 动作复用 Button 按压态，Menu 条目有 pressed/危险色状态；浮层颜色走 theme token，不新增第二色源。应用内 modal 生命周期接入暂不冒充完成：实验证明在同一 BerryDemo 页内直接挂 Dialog 会撞 a11y 32 侧表帽，正确方案是后续把 Dialog 作为独立模态页交换 a11y/节点生命周期。

第三段落 P1 基础件与选中语义：a11y wire 增加 `A11Y_STATE_SELECTED`，BerryUI 导出 `SEM_STATE_SELECTED`，用于分段/Filter Chip/TextPicker 当前项；Checkbox 与 Switch 的双态仍用 Checked。新增 `SegmentedControl`、`Stepper`、`Chip`、`Badge`、`TextPicker`、`SearchField` 与 `ScrollViewAxis`，全部保持 Plain Data 与既有消息/主题/密度路径；横向滚动复用 Clip/Row/offset 原语，不新增滚动物理。Berry Demo 按页分布消费：Widgets 用 Chip/Badge，Inputs 用 SearchField，Lists 用 SegmentedControl，Theme 用 TextPicker，System 用 Stepper，Widgets 的 icon 条使用横向 ScrollView；每页 SEM 用量保持在 8 帽内，真实 e2e 分别点击 chip/search-clear/segment/picker/stepper 全命中。
第四段按用户指定的 workflow 模式重新并查 Android/iOS/OpenHarmony 与 BerryUI/预制应用，优先落“隐藏能力归位 + P0 补漏”：`SwipePager` 从 launcher 私有实现提升进 SDK facade，成为 Swiper/ViewPager/paging 共同面；`ViewKind::Image` 引擎链路补上 `image()` facade；AppBar 增真实 trailing actions Row；Button 增 Standard/High/Medium/Low emphasis 档并在构造期直接落 token；ScrollView 暴露 `max_offset` 与 `near_edge`；Slider 补 `SEM_ACTION_ADJUST`、语义 value、Press 跳值与焦点视觉；ListItem/ListTile 选中项改用 `SEM_STATE_SELECTED`。BerryDemo Theme 页顶栏真实挂载 IconButton 动作位，产品 e2e 点击 `info` 命中。随后补紧凑结构化选择器：`DatePicker` 以年/月/日三列和 `days_in_month` 纯函数执行闰年/大小月 clamp；`TimePicker` 以时/分两列执行 01:59→02:00 确定性进位。二者都是 Plain Data 调节器，不伪造滚轮惯性或月历格。随后补交互与导航助手：NavStack 是非空纯 route 栈，不持 NodeId、不接管页面树；LongPressGesture 以 app 注入的唯一 now_ns 锚定500ms长按并抑制尾随 Click；ContextMenu 用 Root 兄弟全屏 scrim + 锚定 Menu，支持外部点击与 Cancel 关闭。TextFieldSlots 以 additive 外裹件提供 label/supporting/error：TextField 原构造、焦点锚与 SEM 唯一锚零漂，Error 用 danger token，不伪造 P0 wire 状态位；BerryDemo Inputs 页真实消费并在点击时切换 “Name required”。

## Alternatives considered

- 继续只保留 Button/Switch/TextField：三方基础件缺口无法靠 app 手写补齐，拒绝。
- 让 Tabs 持有导航栈：破坏 BerryUI 既裁决“路由/返回栈归 app 状态机”，拒绝。
- 用 wall-clock 驱动 Loading/Progress：违反停帧与真值合同，拒绝。
- 为每个 app 自制底部按钮行：重复命中/状态/语义逻辑，拒绝。
- 在本版直接做 Dialog/Menu/Toast/Sheet：共同依赖的 overlay 系统层尚未落地，先在规格中标为边界而非伪装完成。
- 把 Selected 复用 Checked：读屏无法区分“当前项”与“双态开”，拒绝；两者成为相邻语义位。
- 把所有 P1 件塞进同一 BerryDemo 页：会撞 SEM_MAX=8 并重演 modal 实验的侧表帽，按页分布消费更贴近真实应用。
- 为 Swiper 另写一个分页控件：launcher 的 SwipePager 已有跟手、速度/位移翻页、吸附与 host 测试，重复实现只会制造第二真相源，拒绝。
- 让 AppBar 自己持有动作业务： AppBar 只提供 trailing 容器与布局，IconButton 消息仍归 app 分发，拒绝。

## Consequences

BerryUI host 全套、产品 Recents/frame-cache e2e、GUI8 vgpu `--only-a`、smoke-image 147/147、gui-wmd/berry-guiproto/tools-berry host、SDK ledger 177 项与 `just check` 通过。SDK facade 从 119 项扩到 177 项；TextArea、Refresher、按压缩放、ContextMenu 与 P2 件仍未完成，不能宣称三方全部控件已完成。

## Related

- [OpenHarmony 式产品系统壳](2026-11-15-openharmony-product-shell.md)
- [BerryUI AppBar D3](2026-11-13-berryui-appbar-d3.md)
