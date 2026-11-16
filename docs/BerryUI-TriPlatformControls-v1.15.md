Type: standard
Version: 1.15.0
Status: current
Updated: 2026-11-16

# BerryUI 三方共性控件、浮层、P1 基础件与Safe区色延伸

## 1. 范围

本规格约束 Android Material/Compose、Apple SwiftUI/UIKit 与 OpenHarmony ArkUI 三方均提供、且 BerryUI 产品应用先需的共性控件与浮层。完整三方差距矩阵与后续 P1/P2 路线见 [../research/BerryUI-三方控件体系对照-2026-11-v1.0.md](../research/BerryUI-三方控件体系对照-2026-11-v1.0.md)。

## 2. 控件契约

| 控件 | 语义 | 状态所有权 |
|---|---|---|
| `Checkbox` | 双态选择，48dp 级命中面，语义含 checked/disabled | app 持有；Click 只翻转组件视觉并返回激活 |
| Card `set_pressed` / `on_msg` | 可按压表面容器：Pressed token 换底 + 2dp paint offset，宽高与兄弟布局零漂 | Click 业务归 app；Card 只消费自身表面 Press/Release/Cancel 视觉 |
| `Radio` | 单选组中的一个圆形选项，含按压/焦点态 | 单选组状态归 app；控件只呈现 checked |
| `Slider` | 0..=1000 permille 连续值调节，支持 Press/Drag 实时更新 | app 读取 `permille()`；组件不产生回调 |
| `ProgressIndicator` | 定量线性进度条 | app 持有值并调用 `set_permille` |
| `LoadingIndicator` | 不定进度三点相位 | app 驱动 tick 并调用 `set_phase` |
| `Tabs` / `TabBarStyle` / `TabItemSpec` / `set_badge` | 顶部分页或底部导航，最多 6 项；panel/icon/text/badge 同命中面，badge 为 danger/on_primary 计数位 | app 分发消息并持计数真值；组件维护 selected/pressed/focused 视觉 |
| `NavigationIconSource` / `NAV_ICON_*` | app 顶栏与 SYSTEM 三大金刚共享同一导航 glyph 源；像素由 AOSP SystemUI 原始 VectorDrawable pathData 光栅化，非 BerryOS 自绘几何 | 颜色 normal/active/white 由供件面生成；AOSP 许可与溯源归产品壳规格 |
| Scaffold bottom 槽 / Safe色延伸 / `body_height` | bottom slot 位于 safe_bottom 之上；top/bottom safe reserve 用应用 chrome 色连续填充，交互内容仍按 safe inset 内缩 | 栏内容、液态玻璃浮动留白与 safe_bottom floor 策略归 app；几何与安全区颜色延伸归 Scaffold |
| `Section` | 列表/表单分组标题与内容容器 | 内容与选择状态归 app |
| `Grid` | 非零行列网格包装，失败 loud | 只包装既有 `GridProps`，不新增第二布局算法 |
| `SafeInsets` | left/top/right/bottom/ime 的 Plain Data 汇聚 | 真值仍唯一来自 Berry Room；app 不得自造 |
| `OverlaySurface` | 全屏 scrim + 可偏移内容的顶层浮层宿主 | app 调用 `dismiss` 移除整棵浮层 |
| `AlertDialog` / `DialogAction` | 标题、正文、确认/取消模态 | app 分发消息并解释 Confirm/Cancel |
| `Toast` | 非阻塞短消息，可带动作按钮 | 显示时长与 dismiss 归 app |
| `Menu` / `MenuItemSpec` | 锚定下拉菜单，支持危险项颜色 | 外部点击关闭与动作归 app |
| `Sheet` | 底部 detent 容器 | 高度、显示/关闭归 app |
| `SegmentedControl` / `SegmentedItemSpec` / `SEGMENTED_MAX` | 2..4 段单选分段控件，含 disabled/pressed/focused | selected 视觉归组件；业务含义归 app |
| `Stepper` | 有界整数步进器 | app 给 min/max；越界输入被夹取 |
| `Chip` / `ChipKind` | Assist 动作芯片与 Filter 过滤芯片 | Filter selected 归组件视觉；动作归 app |
| `Badge` | 圆点或最多显示 999 的计数徽标 | count 与锚定偏移归 app |
| `TextPicker` / `TEXT_PICKER_MAX` | 上一项/当前值/下一项的紧凑 TextPicker | selected 视觉归组件；选项数据归 app |
| `SearchField` | 搜索 icon + TextField + 清除按钮组合件 | 查询文本与 selection 仍归 app 的 TextInputState |
| `TextFieldSlots` / `TextFieldSlotsSpec` / `FieldMessage` | label、supporting、error 外裹槽 | TextField原焦点/语义锚零漂；校验与文案归 app |
| `ScrollViewAxis` | ScrollView 的 Vertical/Horizontal 轴向 | 偏移与 clamp 归既有 ScrollView；不新增滚动引擎 |
| `ButtonEmphasis` | Standard/High/Medium/Low 四档按钮强调 | 映射 theme token；业务动作仍归 app |
| AppBar `actions` 槽 | 现代顶栏右侧动作容器 | app 挂载 IconButton；命中/焦点仍走唯一引擎 |
| `image()` | 静态位图叶子 facade | 像素经唯一 Image/Icon 资源缝；label 进 a11y |
| `SwipePager` / Swiper 常量 | 1..8 页跟手分页、速度/位移翻页、QuadOut 240ms 吸附与圆点指示 | 页内容归 app；不新增 ViewKind 或 autoplay |
| ScrollView `max_offset` / `near_edge` | 纵/横滚动触缘确定性查询 | 加载更多与 Refresher 状态机由 app 驱动 |
| `PullRefresh` / `PullRefreshSpec` / `PullRefreshPhase` / `PullRefreshEvent` | 纵向 ScrollView 顶部下拉刷新：指示器停泊 Clip 上方、阻尼拉动、Release 触发、QuadOut 240ms 吸附、真实 busy 指示 | 触发与完成业务归 app；app 显式 tick(now_ns)，无 wall-clock/timer |
| `TextArea` / `TextAreaSpec` / `TEXT_AREA_LINES_MAX` | 多行纯文本：显式 U+000A 与自动换行、跨行 selection 三段、caret 行列定位、垂直滚动保 caret | 业务文本与 selection 归 app 的 `TextInputState`；Enter/Up/Down/LineHome/LineEnd 由 `Multiline` 键路由收敛；IME 直入与点击定位 caret 后置 |
| `DatePicker` / `DatePickerValue` / `days_in_month` | 年/月/日三列紧凑日期选择，含闰年与大小月 clamp | 值域 1970..2100；月历格与滚轮惯性不做 |
| `TimePicker` / `TimePickerValue` | 时/分两列紧凑时间选择 | 01:59 + 1 分钟确定性进位到 02:00 |
| `NavStack` / `NavEvent` / `NAV_STACK_MAX` | 非空纯路由栈状态机 | 只管 route 与事件，不持 NodeId、不建导航容器 |
| `LongPressGesture` / `LongPressOutcome` / `LONG_PRESS_NS` | 长按纯手势 helper | app 帧循环注入唯一 now_ns；触发后尾随 Click 恰抑制一次 |
| `ContextMenu` / `ContextMenuOutcome` | 全屏 scrim + 锚定菜单 | 外部点击/Cancel 关闭；条目选择返回索引 |
| AppBar `large` / `expanded_height` / Scaffold `large_with_back_safe` | 56dp 操作行 + 大标题 + 可选副标题 | 静态几何；不做滚动折叠或动画 |

所有构造函数保持 `&mut Ui<N>` + Plain Data + NodeId 句柄；组件不接触 Berry Room、WM、VMO、present 或业务回调。

## 3. 浮层与状态语义

- `OverlaySurface` 必须挂 Ui root。Root 直接子树各拿全 viewport 且后画兄弟在上；这复用唯一布局/命中/paint 引擎，不引入第二 z-order 真相源。
- Dialog 的 scrim 点击返回 `DialogAction::Cancel`；确认/取消按钮复用 Button 的按压与焦点态。
- Toast 不内置 timer；没有真实超时事件时不得伪造消失。
- Menu 命中只覆盖菜单自身；点击外部由 app 关闭。
- Checkbox 语义状态复用 `SEM_STATE_CHECKED` 与 `SEM_STATE_DISABLED`。
- 分段、Filter Chip 与 TextPicker 的当前项使用新 `SEM_STATE_SELECTED` 位；Checkbox/Switch 的双态仍使用 `SEM_STATE_CHECKED`，两词不得混用。
- Stepper、TextPicker 与 Slider 声明 `SEM_ACTION_ADJUST`；数值/选项是真值，不按墙钟或惯性推进。Slider Press/Drag 按权威 track bounds 取值，语义 value 随 permille 同步。
- ListItem/ListTile 的当前选中行使用 `SEM_STATE_SELECTED`；Checkbox/Switch 的双态继续使用 `SEM_STATE_CHECKED`。
- Swiper 吸附推进只由 app 显式 `tick(now_ns)` 驱动；无 autoplay、无墙钟自推进。
- Stepper 越界输入被 clamp 到 app 给定的 min/max；min>max loud 拒绝。
- SearchField 不请求软键盘、不持有输入状态；Click Clear 只返回激活，清空业务值由 app 执行。
- Slider 拖动值按 track 权威 bounds 裁剪，越界输入只产生 0..=1000。
- Progress/Loading 不按墙钟推进；无新真值时像素保持不变。
- Scaffold safe_top/safe_bottom 不是透明洞：前者用 AppBar `surface` 连续填充到摄像头/挖孔下方，后者用应用底栏 `surface` 连续填充到屏幕底。AppBar/Body/Tabs 的可交互节点仍严格位于 safe inset 内。
- Scaffold `bottom_h` 叠加在 Berry Room `safe_bottom` 预留带之上，不得把栏高并入 `safe_bottom` 造成双扣。`body_height()` 是构造时快照；viewport 改变须整页重建，不新增第二布局真相。
- NavigationIconSource 的透明像素允许 RGB 携带 ink 色而非全零；消费面只以 A 位判定覆盖。AOSP 源文件与 NOTICE 由产品壳规格单源管理。
Tab badge 是视觉子节点：不新增 a11y/SEM 锚，badge 命中归所属 Tab；计数真值归 app。显示值钳到 999，`set_badge(index, 0)` 移除视觉节点。

Settings、Contacts、Files、Photos、Browser 与 Berry Demo 均遵守系统壳骨架；Berry Demo 六页持久使用 `Tabs` 底栏，并在 Widgets 页消费 Checkbox/Radio/Slider/Progress/Loading。

## 4. 验收

- BerryUI host 值级测试覆盖构造、几何、状态切换、Tab Click/Press、进度宽度、浮层居中、Dialog 动作、Toast 动作、Menu 选择、Sheet 底部几何、Segmented selected 语义、Stepper clamp、Chip 过滤切换、Badge 密度几何、TextPicker 调整、DatePicker 闰年/大小月 clamp、TimePicker 01:59→02:00 进位、NavStack 非空/满栈/非法 route、LongPress 一次触发与尾随 Click 抑制、ContextMenu 锚定钳位/选择/外部关闭、TextFieldSlots label/message/error 状态、PullRefresh 阻尼/触发/吸附/完成状态机、TextArea 显式/自动换行与行列编辑、SearchField 清除与非法参数拒面。
- SDK ledger 必须逐项登记公开 facade。
- 应用迁移时必须分发 Press/Release/Cancel/Click/Focus 消息，禁止只过滤 Click 造成按压反馈丢失。Card/Dock/Recents 的按压反馈必须保持宽高不变；host 钉板需证明 measured=0 且兄弟 bounds 零漂。

## 5. 边界

- 浮层目前覆盖 Dialog、Toast、Menu、Sheet 的确定性布局；半透明 alpha、动画过渡、popover、上下文手势与无障碍 modal 语义留后续版本。
- TextPicker、DatePicker 与 TimePicker 都是确定性紧凑选择器，不冒充滚轮惯性或月历格。
- LongPressGesture 不读 wall-clock；Msg 无时戳，起拍锚定 Press 后首次 tick。ContextMenu 触发手势归 app，组件只负责浮层与选择/关闭。
- Berry Room 提供 app 可见 `now_ns()` 单调时钟。Files 长按用它锚定 DOWN 并在 UP 定谳；该 API 不授 WM/present/HDI/SYSTEM 能力，也不把时钟藏进控件。
- Files 列表行真实消费 LongPress + ContextMenu：长按行弹出 Open/Properties，Properties 留在列表，Open 进入详情；尾随 Click 被抑制，普通短点击仍直接打开。
- Large AppBar 是静态几何：普通 56dp 操作行 + Title + 可选 Body 副标题 + trailing actions；滚动折叠、enterAlways 与过渡动画不做。Contacts 列表页已迁移为 “Contacts / 4 contacts”。
- PullRefresh 只接受纵向 ScrollView；指示器阈值即自身高度。拖动位移按 1/2 阻尼并封顶两倍阈值；Release armed 才 Triggered。Files 集成用真实 FS 重读作为刷新完成，不用 sleep 伪造 busy 时长。
- TextArea 的换行真相只有一份：app `TextInputState` 字节。渲染域视觉行由 `text_wrap::plan` 派生，显式 U+000A 不计宽、不出字形；编辑域逻辑行/粘滞列由 `text_input` 纯 helper 派生。TextField 拒绝 `Multiline` 构造档。
- TextArea v1 用最多三段 Rect 表示跨行 selection（头行、中带、尾行）；短中间行的行尾空白可能被中带覆盖，这是视觉近似，不改变文本状态。点击定位 caret、拖拽选择、IME multiline 直入、光标闪烁与富文本显式后置。BerryDemo Inputs 页已消费 TextArea，页面 SEM 预算为 8/8；该页用 Retained Surface 增加 Keyboard 输入类，QMP 真键链已覆盖 Enter 换行、字符插入、空格、Backspace 与 Up 粘滞列。
- NavStack 是纯 route 状态机；页面树、返回业务与 Route→UI 映射仍归 app。
- TextFieldSlots 的 label/message 不额外占 a11y/SEM；panel 仍是唯一焦点与语义锚。Error 只换 danger token 与消息类别，不在 P0 wire 伪造新状态位。
- Card 按压反馈用 token 换底 + paint offset，不用 width/height 缩放；直接改变尺寸会触发 GEOM 组重排并移动兄弟/内容，拒绝。BerryDemo Widgets 页已消费 Card 表面消息，launcher Dock/Recents 的同类反馈见产品壳规格。
- Scaffold bottom 槽只提供固定高容器，不建 Tabs/ToolBar、不持栏选中态、不做滚动隐藏。BerryDemo 六页已迁移到标准 bottom 槽，底栏 1818→1812 修正旧手写 column 6px 越界。
- Tab badge 不做出现/消失动画、不做 a11y/SEM 朗读词汇、不自动清除计数；这些后续需显式规格。BerryDemo Inputs 位已持久显示 count=3。
- 通用按压缩放 transform、swipe actions 与 P2 件仍未完成；TextArea v1 与 Refresher v1 已完成，TextArea 的点击定位、拖拽选择与 IME 直入继续后置。
- DatePicker 的六枚调节按钮加一个聚合语义共 7 项 SEM；TimePicker 共 5 项。应用不得把两者堆进已近 SEM_MAX=8 的页面。
- 横向 ScrollView 复用 Clip/Row/offset/Msg 原语，不引入惯性、嵌套滚动仲裁或第二滚动物理。
- SwipePager 是 app 侧普通库代码；attach 页宽冻结在当前 viewport，重复 attach 由 app 管旧树生命周期。
- image() 只包静态位图叶子；视频、异步加载与像素资源管理不在控件内伪造。
- 不引入第二套布局、光栅、焦点或语义算法。
- 不宣称覆盖三方全部控件；P1/P2 差距以三方矩阵为准继续推进。
