Type: standard
Version: 1.0.0
Status: current
Updated: 2026-11-15

# BerryUI 三方共性控件第一版

## 1. 范围

本规格约束 Android Material/Compose、Apple SwiftUI/UIKit 与 OpenHarmony ArkUI 三方均提供、且 BerryUI 产品应用先需的共性控件面。完整三方差距矩阵与后续 P1/P2 路线见 [../research/BerryUI-三方控件体系对照-2026-11-v1.0.md](../research/BerryUI-三方控件体系对照-2026-11-v1.0.md)。

## 2. 控件契约

| 控件 | 语义 | 状态所有权 |
|---|---|---|
| `Checkbox` | 双态选择，48dp 级命中面，语义含 checked/disabled | app 持有；Click 只翻转组件视觉并返回激活 |
| `Radio` | 单选组中的一个圆形选项 | 单选组状态归 app；控件只呈现 checked |
| `Slider` | 0..=1000 permille 连续值调节，支持 Press/Drag 实时更新 | app 读取 `permille()`；组件不产生回调 |
| `ProgressIndicator` | 定量线性进度条 | app 持有值并调用 `set_permille` |
| `LoadingIndicator` | 不定进度三点相位 | app 驱动 tick 并调用 `set_phase` |
| `Tabs` / `TabBarStyle` / `TabItemSpec` | 顶部分页或底部导航，最多 6 项（Berry 扩展覆盖 Demo 六页；移动最佳实践仍优先 3..5 项） | app 分发 Click；组件维护 selected 视觉 |
| `Section` | 列表/表单分组标题与内容容器 | 内容与选择状态归 app |
| `Grid` | 非零行列网格包装，失败 loud | 只包装既有 `GridProps`，不新增第二布局算法 |
| `SafeInsets` | left/top/right/bottom/ime 的 Plain Data 汇聚 | 真值仍唯一来自 Berry Room；app 不得自造 |

所有构造函数保持 `&mut Ui<N>` + Plain Data + NodeId 句柄；组件不接触 Berry Room、WM、VMO、present 或业务回调。

Settings、Contacts、Files、Photos、Browser 与 Berry Demo 均遵守系统壳骨架；Berry Demo 额外在六个页面持久使用 `Tabs` 底栏，真实 Click 可在 Home/Widgets/Inputs/Lists/Theme/System 间切换，并消费 Checkbox/Radio/Slider/Progress/Loading 展示新控件。

## 3. 状态与语义

- `Checkbox` 语义状态复用 `SEM_STATE_CHECKED` 与 `SEM_STATE_DISABLED`。
- `Slider` 的拖动值按 track 权威 bounds 裁剪，输入越界只产生 0..=1000。
- `ProgressIndicator` 不按墙钟推进；无新真值时像素保持不变。
- `LoadingIndicator` 只提供纯相位应用，不内置 timer 或伪造活动指示。
- `Tabs` 选中视觉由 Click 后 app 分发消息驱动；组件不抢导航栈所有权。

## 4. 验收

- BerryUI host 值级测试覆盖构造、几何、状态切换、Tab Click、进度宽度和非法行列拒面。
- SDK ledger 必须逐项登记公开 facade。
- 应用迁移时必须分发 Press/Release/Cancel/Click/Focus 消息，禁止只过滤 Click 造成按压反馈丢失。

## 5. 边界

- 本版不含 overlay 依赖的 Dialog、Menu、Toast、Sheet；这些控件必须等待系统 overlay 层先落地。
- 不引入第二套布局、光栅、焦点或语义算法。
- 不宣称覆盖三方全部控件；P1/P2 差距以三方矩阵为准继续推进。
