# 三方共性控件第一版与应用底栏

Status: implemented

## Problem

对齐 Android Material/Compose、Apple SwiftUI/UIKit 与 OpenHarmony ArkUI 后，BerryUI 公共面缺少三方都有的基础选择与反馈件：Checkbox、Radio、Slider、定量/不定进度、Tabs、分组容器、Grid 包装与统一 SafeInsets。预制应用也仍靠手写进度条，缺少可在页面间切换的持久底栏。

## Decision

依据 [BerryUI-三方控件体系对照-2026-11-v1.0.md](../../../research/BerryUI-三方控件体系对照-2026-11-v1.0.md) 与 [BerryUI-TriPlatformControls-v1.2.md](../../../docs/BerryUI-TriPlatformControls-v1.2.md)，新增 `Checkbox`、`Radio`、`Slider`、`ProgressIndicator`、`LoadingIndicator`、`Tabs`、`Section`、`Grid` 与 `SafeInsets`。全部保持 Plain Data + `&mut Ui<N>` + NodeId，不新增布局/光栅/焦点算法；状态归 app，组件只应用视觉与语义。Tabs 支持 Top/Bottom 两档，最多 6 项，panel/icon/text 都是同一激活面。

Berry Demo 六个页面持久使用 `Tabs::Bottom`，真实 Click 在 Home/Widgets/Inputs/Lists/Theme/System 间切换；Widgets 页消费 Checkbox、Radio、Slider、ProgressIndicator 与 LoadingIndicator。系统 BACK/HOME/TASK 三键保留在 SYSTEM 底部保留区，应用底栏位于 safe_bottom 之上。

第二段补上浮层族：`OverlaySurface` 以 Ui root 后画兄弟作为唯一 z-order 事实，新增 `AlertDialog`、`Toast`、`Menu` 与 `Sheet`。Dialog/Toast 动作复用 Button 按压态，Menu 条目有 pressed/危险色状态；浮层颜色走 theme token，不新增第二色源。应用内 modal 生命周期接入暂不冒充完成：实验证明在同一 BerryDemo 页内直接挂 Dialog 会撞 a11y 32 侧表帽，正确方案是后续把 Dialog 作为独立模态页交换 a11y/节点生命周期。

第三段落 P1 基础件与选中语义：a11y wire 增加 `A11Y_STATE_SELECTED`，BerryUI 导出 `SEM_STATE_SELECTED`，用于分段/Filter Chip/TextPicker 当前项；Checkbox 与 Switch 的双态仍用 Checked。新增 `SegmentedControl`、`Stepper`、`Chip`、`Badge`、`TextPicker` 与 `SearchField`，全部保持 Plain Data 与既有消息/主题/密度路径。Berry Demo 按页分布消费：Widgets 用 Chip/Badge，Inputs 用 SearchField，Lists 用 SegmentedControl，Theme 用 TextPicker，System 用 Stepper；每页 SEM 用量保持在 8 帽内，真实 e2e 分别点击 chip/search-clear/segment/picker/stepper 全命中。

## Alternatives considered

- 继续只保留 Button/Switch/TextField：三方基础件缺口无法靠 app 手写补齐，拒绝。
- 让 Tabs 持有导航栈：破坏 BerryUI 既裁决“路由/返回栈归 app 状态机”，拒绝。
- 用 wall-clock 驱动 Loading/Progress：违反停帧与真值合同，拒绝。
- 为每个 app 自制底部按钮行：重复命中/状态/语义逻辑，拒绝。
- 在本版直接做 Dialog/Menu/Toast/Sheet：共同依赖的 overlay 系统层尚未落地，先在规格中标为边界而非伪装完成。
- 把 Selected 复用 Checked：读屏无法区分“当前项”与“双态开”，拒绝；两者成为相邻语义位。
- 把所有 P1 件塞进同一 BerryDemo 页：会撞 SEM_MAX=8 并重演 modal 实验的侧表帽，按页分布消费更贴近真实应用。

## Consequences

BerryUI host 全套、产品 Recents/frame-cache e2e、GUI8 vgpu `--only-a`、smoke-image 147/147、gui-wmd/berry-guiproto/tools-berry host、SDK ledger 149 项与 `just check` 通过。SDK facade 从 119 项扩到 149 项；TextArea、DatePicker、Refresher、横向滚动、按压缩放、ContextMenu 与 P2 件仍未完成，不能宣称三方全部控件已完成。

## Related

- [OpenHarmony 式产品系统壳](2026-11-15-openharmony-product-shell.md)
- [BerryUI AppBar D3](2026-11-13-berryui-appbar-d3.md)
