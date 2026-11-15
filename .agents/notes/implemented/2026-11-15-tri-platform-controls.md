# 三方共性控件第一版与应用底栏

Status: implemented

## Problem

对齐 Android Material/Compose、Apple SwiftUI/UIKit 与 OpenHarmony ArkUI 后，BerryUI 公共面缺少三方都有的基础选择与反馈件：Checkbox、Radio、Slider、定量/不定进度、Tabs、分组容器、Grid 包装与统一 SafeInsets。预制应用也仍靠手写进度条，缺少可在页面间切换的持久底栏。

## Decision

依据 [BerryUI-三方控件体系对照-2026-11-v1.0.md](../../../research/BerryUI-三方控件体系对照-2026-11-v1.0.md) 与 [BerryUI-TriPlatformControls-v1.0.md](../../../docs/BerryUI-TriPlatformControls-v1.0.md)，新增 `Checkbox`、`Radio`、`Slider`、`ProgressIndicator`、`LoadingIndicator`、`Tabs`、`Section`、`Grid` 与 `SafeInsets`。全部保持 Plain Data + `&mut Ui<N>` + NodeId，不新增布局/光栅/焦点算法；状态归 app，组件只应用视觉与语义。Tabs 支持 Top/Bottom 两档，最多 6 项，panel/icon/text 都是同一激活面。

Berry Demo 六个页面持久使用 `Tabs::Bottom`，真实 Click 在 Home/Widgets/Inputs/Lists/Theme/System 间切换；Widgets 页消费 Checkbox、Radio、Slider、ProgressIndicator 与 LoadingIndicator。系统 BACK/HOME/TASK 三键保留在 SYSTEM 底部保留区，应用底栏位于 safe_bottom 之上。

第二段补上浮层族：`OverlaySurface` 以 Ui root 后画兄弟作为唯一 z-order 事实，新增 `AlertDialog`、`Toast`、`Menu` 与 `Sheet`。Dialog/Toast 动作复用 Button 按压态，Menu 条目有 pressed/危险色状态；浮层颜色走 theme token，不新增第二色源。应用内 modal 生命周期接入暂不冒充完成：实验证明在同一 BerryDemo 页内直接挂 Dialog 会撞 a11y 32 侧表帽，正确方案是后续把 Dialog 作为独立模态页交换 a11y/节点生命周期。

## Alternatives considered

- 继续只保留 Button/Switch/TextField：三方基础件缺口无法靠 app 手写补齐，拒绝。
- 让 Tabs 持有导航栈：破坏 BerryUI 既裁决“路由/返回栈归 app 状态机”，拒绝。
- 用 wall-clock 驱动 Loading/Progress：违反停帧与真值合同，拒绝。
- 为每个 app 自制底部按钮行：重复命中/状态/语义逻辑，拒绝。
- 在本版直接做 Dialog/Menu/Toast/Sheet：共同依赖的 overlay 系统层尚未落地，先在规格中标为边界而非伪装完成。

## Consequences

BerryUI host 全套、产品 Recents/frame-cache e2e、GUI8 vgpu `--only-a`、smoke-image 147/147、gui-wmd/berry-guiproto/tools-berry host、SDK ledger 138 项与 `just check` 通过。SDK facade 从 119 项扩到 138 项；后续 P1/P2 控件仍以三方矩阵为准，不能宣称三方全部控件已完成。

## Related

- [OpenHarmony 式产品系统壳](2026-11-15-openharmony-product-shell.md)
- [BerryUI AppBar D3](2026-11-13-berryui-appbar-d3.md)
