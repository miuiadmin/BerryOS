Type: standard
Version: 1.1.0
Status: superseded
Superseded-by: [BerryUI-TriPlatformControls-v1.2.md](BerryUI-TriPlatformControls-v1.2.md)
Updated: 2026-11-15

# BerryUI 三方共性控件与浮层第一版

## 1. 范围

本规格约束 Android Material/Compose、Apple SwiftUI/UIKit 与 OpenHarmony ArkUI 三方均提供、且 BerryUI 产品应用先需的共性控件与浮层。完整三方差距矩阵与后续 P1/P2 路线见 [../research/BerryUI-三方控件体系对照-2026-11-v1.0.md](../research/BerryUI-三方控件体系对照-2026-11-v1.0.md)。

## 2. 控件契约

| 控件 | 语义 | 状态所有权 |
|---|---|---|
| `Checkbox` | 双态选择，48dp 级命中面，语义含 checked/disabled | app 持有；Click 只翻转组件视觉并返回激活 |
| `Radio` | 单选组中的一个圆形选项，含按压/焦点态 | 单选组状态归 app；控件只呈现 checked |
| `Slider` | 0..=1000 permille 连续值调节，支持 Press/Drag 实时更新 | app 读取 `permille()`；组件不产生回调 |
| `ProgressIndicator` | 定量线性进度条 | app 持有值并调用 `set_permille` |
| `LoadingIndicator` | 不定进度三点相位 | app 驱动 tick 并调用 `set_phase` |
| `Tabs` / `TabBarStyle` / `TabItemSpec` | 顶部分页或底部导航，最多 6 项；panel/icon/text 同命中面 | app 分发消息；组件维护 selected/pressed/focused 视觉 |
| `Section` | 列表/表单分组标题与内容容器 | 内容与选择状态归 app |
| `Grid` | 非零行列网格包装，失败 loud | 只包装既有 `GridProps`，不新增第二布局算法 |
| `SafeInsets` | left/top/right/bottom/ime 的 Plain Data 汇聚 | 真值仍唯一来自 Berry Room；app 不得自造 |
| `OverlaySurface` | 全屏 scrim + 可偏移内容的顶层浮层宿主 | app 调用 `dismiss` 移除整棵浮层 |
| `AlertDialog` / `DialogAction` | 标题、正文、确认/取消模态 | app 分发消息并解释 Confirm/Cancel |
| `Toast` | 非阻塞短消息，可带动作按钮 | 显示时长与 dismiss 归 app |
| `Menu` / `MenuItemSpec` | 锚定下拉菜单，支持危险项颜色 | 外部点击关闭与动作归 app |
| `Sheet` | 底部 detent 容器 | 高度、显示/关闭归 app |

所有构造函数保持 `&mut Ui<N>` + Plain Data + NodeId 句柄；组件不接触 Berry Room、WM、VMO、present 或业务回调。

## 3. 浮层与状态语义

- `OverlaySurface` 必须挂 Ui root。Root 直接子树各拿全 viewport 且后画兄弟在上；这复用唯一布局/命中/paint 引擎，不引入第二 z-order 真相源。
- Dialog 的 scrim 点击返回 `DialogAction::Cancel`；确认/取消按钮复用 Button 的按压与焦点态。
- Toast 不内置 timer；没有真实超时事件时不得伪造消失。
- Menu 命中只覆盖菜单自身；点击外部由 app 关闭。
- Checkbox 语义状态复用 `SEM_STATE_CHECKED` 与 `SEM_STATE_DISABLED`。
- Slider 拖动值按 track 权威 bounds 裁剪，越界输入只产生 0..=1000。
- Progress/Loading 不按墙钟推进；无新真值时像素保持不变。

Settings、Contacts、Files、Photos、Browser 与 Berry Demo 均遵守系统壳骨架；Berry Demo 六页持久使用 `Tabs` 底栏，并在 Widgets 页消费 Checkbox/Radio/Slider/Progress/Loading。

## 4. 验收

- BerryUI host 值级测试覆盖构造、几何、状态切换、Tab Click/Press、进度宽度、浮层居中、Dialog 动作、Toast 动作、Menu 选择、Sheet 底部几何与非法参数拒面。
- SDK ledger 必须逐项登记公开 facade。
- 应用迁移时必须分发 Press/Release/Cancel/Click/Focus 消息，禁止只过滤 Click 造成按压反馈丢失。

## 5. 边界

- 浮层目前覆盖 Dialog、Toast、Menu、Sheet 的确定性布局；半透明 alpha、动画过渡、popover、上下文手势与无障碍 modal 语义留后续版本。
- 不引入第二套布局、光栅、焦点或语义算法。
- 不宣称覆盖三方全部控件；P1/P2 差距以三方矩阵为准继续推进。
