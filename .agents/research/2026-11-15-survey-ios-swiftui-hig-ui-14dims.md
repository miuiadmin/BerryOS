# iOS（SwiftUI 加 HIG）UI 框架 14 维对标调研

Type: survey
Status: current
Updated: 2026-11-15

一句话说明：按统一维度框架对 iOS（SwiftUI 加 HIG）逐维评 14 条——状态管理与视图身份、组件、动画、手势、输入法、多窗口、无障碍、暗色与取色、自适应布局、国际化、掉帧治理、开发工具、设计系统（Liquid Glass 2025 至 2026 演进）、特色能力；全部结论取 Apple 官方一手文档，逐条带出处 URL 与 API 名；SwiftUI 声明式模型与反射 diff、Preview 进程机制已在基线（GUI-07 §2.1、GUI-08 §131）成文的部分只引用不重复。

## 问题

owner 要求对 iOS（SwiftUI 加 HIG）做 14 维 UI 框架对标，重点覆盖：SwiftUI 声明式与 @State @Binding @Observable、视图身份与 diff、动画体系（隐式动画、Core Animation、可中断弹簧、转场）、HIG 2025 至 2026 演进（Liquid Glass 与材料层次）、无障碍（VoiceOver 一等公民、Dynamic Type 全系统）、暗色与 Appearance、控件定制深度、Instruments 渲染调优、Xcode Previews 与热迭代。为 BerryOS GUI 线提供可换算的机制名与 API 名。

## 方法与来源

developer.apple.com 文档站与 HIG 站均为 SPA 直抓回壳，全部走官方 JSON 数据端点 `developer.apple.com/tutorials/data/documentation/<slug>.json` 与 `.../data/design/human-interface-guidelines/<slug>.json` 抓取（共 60 余份，含 SwiftUI 根集合与 Xcode 根集合定位 slug），本地提取正文与 topicSections 后逐维核对；每条结论都能追到下文 URL。采集时间 2026-11-15，全部为当前线上版本（iOS 26 / Xcode 26 世代文档）。二手来源（Airbnb 反射 diff、Preview 进程拆解）只在基线已录处引用，不新引入二手结论。

## 结论

### 一、声明式与状态管理：@State @Binding @Observable 三层 + 属性级失效

- 状态分层铁律（官方原文）：state 存放在需要它的视图的「最低公共祖先」以建立跨视图共享的单一真相源（single source of truth）；只读数据用普通 Swift 属性，双向连接用 `@Binding`（引用别处存储、无自有存储）；`$` 前缀取 property wrapper 的 projectedValue 把 State 投影为 Binding，可任意层级传递，且支持作用域化绑定如 `$episode.isFavorite`。state 生命周期与视图一致，禁止当持久化用。出处：https://developer.apple.com/documentation/swiftui/managing-user-interface-state
- `@Observable` 宏（iOS 17 起 Observation 框架）是失效粒度的代际跃迁：编译期展开生成观察代码，属性无需 `@Published` 包装；官方明确「SwiftUI updates a view only when an observable property changes and the view's body reads the property directly」——即 body 执行期间登记属性级依赖，body 没读的属性变更不刷新视图（对比 ObservableObject 任何 @Published 变更全刷）。不需要跟踪的属性标 `@ObservationIgnored()`；模型实例可直接用 `@State`/`@Environment` 持有（替代 StateObject/EnvironmentObject），需要绑定时用 `@Bindable`。支持 Observable 与 ObservableObject 混用渐进迁移。出处：https://developer.apple.com/documentation/swiftui/migrating-from-the-observable-object-protocol-to-the-observable-macro
- 视图身份与 diff：View 是轻量值类型描述，身份分隐式（结构位置）与显式（`id(_:)`、`tag(_:)`、ForEach 的 `Identifiable`）；`equatable()` 修饰符与 `EquatableView` 让相等视图短路重算，`AnyView` 做类型擦除（官方列入 Managing the view hierarchy 主题）。diff 内部实现（AttributeGraph 反射比较存储属性）Apple 未文档化，二手结论见基线 GUI-07 §2.1（Airbnb 工程博客），本文不重复展开。出处：https://developer.apple.com/documentation/swiftui/view-fundamentals

### 二、组件丰富度与质量：样式协议定制 + 系统组件自动升级

- 控件全集（Controls and indicators 集合）：交互类 Button/Link/ShareLink/TextFieldLink/PasteButton/RenameButton/EditButton；选择类 Picker/toggleStyle 全家、Slider/Stepper/Toggle、DatePicker/MultiDatePicker、ColorPicker；指示类 Gauge/ProgressView/ContentUnavailableView（系统级空态组件）；反馈类 `sensoryFeedback(_:trigger:)` 触觉反馈修饰符；尺寸类 `controlSize(_:)` 与 ControlSize。出处：https://developer.apple.com/documentation/swiftui/controls-and-indicators
- 控件定制深度 = 样式协议体系：每个控件族有配对 style 协议与修饰符（`buttonStyle(_:)`/`pickerStyle(_:)`/`toggleStyle(_:)`/`progressViewStyle(_:)`/`gaugeStyle(_:)`），样式成为可插拔策略而非子类化——UIKit 时代靠继承 UISlider 定制，SwiftUI 时代靠实现 ButtonStyle 注入外观与手势响应。iOS 26 新增 `buttonSizing(_:)`/ButtonSizing。出处：https://developer.apple.com/documentation/swiftui/controls-and-indicators
- 质量保证机制：官方明确「Standard components in SwiftUI use Liquid Glass」——系统组件自动获得 Liquid Glass 外观与行为（含触摸指针实时反应），应用不自定义也随系统设计语言升级；自定义走 `buttonStyle(.glass)` 或 `glassEffect()`。出处：https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views

### 三、动画体系：状态驱动三层入口 + 可中断弹簧 + 关键帧多轨

- 三层入口（官方 Animations 集合页）：`withAnimation(_:_:)` 包住状态变更动画全部受影响视图（含 completionCriteria 完成回调变体）；`animation(_:value:)` 视图级隐式动画只动画指定值变化的特定视图；`Binding.animation(_:)` 在绑定层注入动画。自定义可动画值走 `Animatable` 协议 + `VectorArithmetic`（animatableData 插值通道）。出处：https://developer.apple.com/documentation/swiftui/animations
- 弹簧家族与可中断性：`spring(response:dampingFraction:blendDuration:)` 物理参数弹簧；`interactiveSpring` 专为交互动画；`interpolatingSpring(mass:stiffness:damping:initialVelocity:)` 带初速度参数——弹簧天然可中断重定向（目标变化时以当前速度续接，无跳变）；iOS 17 预设 `.bouncy/.smooth/.snappy`（各带 duration:extraBounce: 参数）。更深的中断合并扩展点是 `CustomAnimation` 协议的 `velocity(value:time:context:)` 与 `shouldMerge(previous:value:time:context:)`——框架把「保留速度跨目标合并动画」做成官方协议面。出处：https://developer.apple.com/documentation/swiftui/animation
- iOS 17 两个新动画容器：`keyframeAnimator(initialValue:trigger:content:keyframes:)` 多轨关键帧（每属性一轨 KeyframeTrack，轨内混排 `LinearKeyframe`/`CubicKeyframe`/`SpringKeyframe`/`MoveKeyframe`，如点赞表情 scale 轨线性+弹簧混合）；`phaseAnimator` 逐相动画（每相配独立 Animation）。配套文章 Controlling the timing and movements of your animations。出处：https://developer.apple.com/documentation/swiftui/controlling-the-timing-and-movements-of-your-animations
- 转场体系：`transition(_:)` + Transition/AnyTransition/asymmetric（进出不对称）；`contentTransition(_:)` 内容级转场（如数字翻牌）；`navigationTransition(_:)`（iOS 18 zoom 导航转场）；`matchedGeometryEffect(id:in:)` 几何匹配飞行、`matchedTransitionSource` 配对源、`geometryGroup()` 几何同步分组。出处：https://developer.apple.com/documentation/swiftui/animations
- Core Animation 底座：SwiftUI 动画最终经 Core Animation commit 提交到独立 render server 进程光栅化（见第十一维 hitch 文档对 commit pipeline 的官方拆解）；UIKit 侧隐式动画层（CABasicAnimation）仍可用于 representable 桥接。

### 四、手势与输入：值类型组合代数 + UIKit 识别器桥接

- Gesture 是值类型组合代数（官方 Gestures 集合）：基础有 TapGesture/SpatialTapGesture/LongPressGesture/DragGesture/MagnifyGesture/RotateGesture/RotateGesture3D/WindowDragGesture；组合算子 sequenced/simultaneously/exclusively（SequenceGesture/SimultaneousGesture/ExclusiveGesture）；层级仲裁 `highPriorityGesture(_:including:)` 与 `simultaneousGesture(_:)`、`GestureMask` 控制阻断范围；`defersSystemGestures(on:)` 让位系统边缘手势；`GestureState` 专管瞬态手势状态（手势结束自动复位）。出处：https://developer.apple.com/documentation/swiftui/gestures
- Apple Pencil 一等支持：`onPencilDoubleTap`/`onPencilSqueeze` + `preferredPencilDoubleTapAction` 环境值读取用户系统级笔势偏好。出处：https://developer.apple.com/documentation/swiftui/gestures
- iOS 18 桥接协议 `UIGestureRecognizerRepresentable`：任何 UIGestureRecognizer 可包进 SwiftUI 层级（makeUIGestureRecognizer/updateUIGestureRecognizer 生命周期 + `CoordinateSpaceConverter` 两坐标系统一转换）——新手势框架不重造轮子而是打通存量生态。出处：https://developer.apple.com/documentation/swiftui/uigesturerecognizerrepresentable
- visionOS/iOS 26 操纵协议族 `Manipulable`（ManipulableModifier/ManipulationGestureModifier 等）把 3D 拖拽操纵做成声明式修饰符。出处：https://developer.apple.com/documentation/swiftui/view-fundamentals

### 五、输入法框架：声明式焦点 + 系统键盘定制

- 焦点即状态：`FocusState` property wrapper 读写「场景内焦点位置」，`focused(_:)` 绑定到视图，焦点变化驱动 SwiftUI 更新——键盘弹起/收起成为状态机而非代理回调。出处：https://developer.apple.com/documentation/swiftui/focus
- 键盘定制修饰符族：`keyboardType(_:)`（键盘布局）、`textinputautocapitalization(_:)`（大小写策略）、autocorrectionDisabled、submitLabel、`keyboardShortcut(_:modifiers:localization:)`（硬件键盘快捷键，含本地化键位）。出处：https://developer.apple.com/documentation/swiftui/view/keyboardtype(_:) 与 https://developer.apple.com/documentation/swiftui/view/keyboardshortcut(_:modifiers:localization:)
- 深水区由 UIKit `UITextInput` 协议承载：与系统输入法（选区、标记文本、dictation）交互的方法集（inputDelegate 等），SwiftUI TextField/TextEditor 内建走此底座。出处：https://developer.apple.com/documentation/uikit/uitextinput

### 六、多窗口与合成：Scene 声明 + render server 进程合成

- 声明式窗口：`WindowGroup` 多窗口模板（每窗口同根视图定义、独立保留视图状态，环境值 `supportsMultipleWindows` 可探测、`openWindow` action 打开）；`Window` 单例窗口、UtilityWindow 工具窗；场景修饰符 `windowStyle(_:)`/`defaultPosition(_:)`/`presentedWindowStyle(_:)`。出处：https://developer.apple.com/documentation/swiftui/windows
- UIKit 层 `UIWindowScene` 管理「一个或多个窗口」，是 iPad 多窗口/Stage Manager 的承载单元。出处：https://developer.apple.com/documentation/uikit/uiwindowscene
- 合成管线：app 主线程 commit 变更到独立 render server 进程，render server 分两步把 UI 表示转成位图，显示驱动在下一 vsync 取帧——多阶段流水线并行多帧（详见第十一维 hitch 文档官方拆解）。出处：https://developer.apple.com/documentation/xcode/understanding-hitches-in-your-app

### 七、无障碍：内建自省 + 全套环境开关 + VoiceOver 一等公民

- 零成本基线：SwiftUI 自动内省导航视图、列表、文本框、滑块、按钮等常见元素并生成默认无障碍标签与值（官方原文「comes with built-in accessibility support…provides basic accessibility labels and values by default. You don't have to do any extra work」）。出处：https://developer.apple.com/documentation/swiftui/accessibility-fundamentals
- 修饰符族：`accessibilityLabel(_:)`/accessibilityValue、`accessibilityElement(children:)` 合并子元素、`accessibilityChildren`/`accessibilityRepresentation(representation:)`（装饰视图暴露替代语义树）、accessibilityHidden/accessibilityIdentifier。出处：https://developer.apple.com/documentation/swiftui/accessibility-fundamentals
- 全套环境开关（Accessible appearance 集合）：`accessibilityReduceMotion`/`accessibilityReduceTransparency`/`accessibilityDifferentiateWithoutColor`/`accessibilityShowButtonShapes`/`legibilityWeight`/`accessibilityDimFlashingLights`/`accessibilityLargeContentViewerEnabled`（大内容查看器）/AssistiveAccess（认知无障碍简化模式，含 assistiveAccessNavigationIcon）。出处：https://developer.apple.com/documentation/swiftui/accessible-appearance
- HIG 口径：Dynamic Type 是系统级设置，应用应支持至少放大 200%（watchOS 140%）；对比度按 WCAG Level AA 校验（Xcode Accessibility Inspector 内建，正文最低 4.5:1）；VoiceOver/Voice Control/Switch Control/Full Keyboard Access/Pointer Control 全列为必须测试的辅助技术；手势必须提供等效非手势路径。出处：https://developer.apple.com/design/human-interface-guidelines/accessibility

### 八、主题暗色与动态取色：系统级 Appearance + 语义色三态 + 材料采样

- 暗色是系统全局设置：iOS/iPadOS/macOS/tvOS 用户默认全系统暗色，HIG 明令「Avoid offering an app-specific appearance setting」——应用不得自设外观开关；Auto appearance 随时间切换、可能在应用运行中改变；暗色测试矩阵含 Increase Contrast 与 Reduce Transparency 单开与同开。出处：https://developer.apple.com/design/human-interface-guidelines/dark-mode
- 语义色体系：动态系统色按用途（层级背景/label/link/separator）而非色值定义，自动适配 light/dark/increase-contrast 三上下文；HIG 明令禁止硬编码系统色值（documented values 仅设计参考，随版本波动）；自定义色在 Asset Catalog 建 Color Set 提供 Any/Dark 变体；正文对比 4.5:1 起步、力求 7:1。出处：https://developer.apple.com/design/human-interface-guidelines/color 与 https://developer.apple.com/design/human-interface-guidelines/dark-mode
- SwiftUI 层：`preferredColorScheme(_:)` 声明偏好、`Material` 六档系统模糊材料（ultraThin/thin/regular/thick/ultraThick/bar）+ `materialActiveAppearance(_:)`；每档材料配 vibrancy 前景（label 四级/fill 三级/separator 一级，见 Materials 平台节）。出处：https://developer.apple.com/documentation/swiftui/material 与 https://developer.apple.com/design/human-interface-guidelines/materials
- 动态取色 = Liquid Glass 采样机制：官方明确 Liquid Glass「has no inherent color, and instead takes on colors from the content directly behind it」——无固有颜色、实时采样其后内容着色；小元素（工具栏/标签栏）系统自动随底内容明暗翻转（底亮则符号变深、底暗则变浅）；大元素（侧栏）更不透明保可读性；tint 可做「彩色玻璃」强调（系统 prominent 按钮即背景 tint 而非文字 tint）。出处：https://developer.apple.com/design/human-interface-guidelines/color

### 九、自适应布局断点折叠多屏：size class 决策 + Layout 协议 + 声明式自适应

- size class 是布局决策唯一依据（HIG 原文）：水平/垂直各分 compact/regular 两档，由设备类型、窗口配置、多任务状态（全屏/Slide Over/iPhone Mirroring 到 Mac）共同决定；「Determine layout based on size classes, not device type or orientation」——朝向与设备 idiom 不携带可用空间信息，禁止作为布局判据；所有 compact/regular 组合都可能出现；功能随尺寸不增减、可见的功能量可增减（大空间 tab bar 换 sidebar）。出处：https://developer.apple.com/design/human-interface-guidelines/layout
- 自定义布局容器协议：`Layout` 协议（`sizeThatFits(proposal:subviews:cache:)` + `placeSubviews(in:proposal:subviews:cache:)` + cache 生命周期 + spacing/explicitAlignment）——第三方容器与系统容器同权；`AnyLayout` 类型擦除运行时切布局；`ViewThatFits` 空间择优选第一个放得下的子视图。出处：https://developer.apple.com/documentation/swiftui/layout 与 https://developer.apple.com/documentation/swiftui/anylayout 与 https://developer.apple.com/documentation/swiftui/viewthatfits
- 多栏导航 `NavigationSplitView`：双/三栏（sidebar/content/detail），选择联动、columnVisibility 可控、preferredCompactColumn 指定紧凑态列；配合 size class 自动折叠为 push 栈。出处：https://developer.apple.com/documentation/swiftui/navigationsplitview
- 2025 新自适应材料件：scroll edge effect（滚动边缘模糊渐隐抬高控件）、`backgroundExtensionEffect()`（背景延伸效果：翻转模糊镜像补齐被侧栏遮住的图）。出处：https://developer.apple.com/design/human-interface-guidelines/layout

### 十、国际化与字体：String Catalog 构建期提取 + SF 字体家族 + SF Symbols

- String Catalog：单一可视化编辑器管理全部可本地化串；构建时（Product > Build）自动从代码提取可本地化字符串入目录并随每次构建增量同步；SwiftUI 视图内字符串默认可本地化（LocalizedStringKey）；字符串插值自动生成复数变体（Vary by Plural），语言级差异自动处理（英语 One/Other，俄语自动 One/Few/Many/Other）；支持设备变体。出处：https://developer.apple.com/documentation/xcode/localizing-and-varying-text-with-a-string-catalog
- `Text` 本地化默认开、`Text(verbatim:)` 显式跳过本地化；`Text(_:format:)`（DateStyle/数字格式随 locale）。出处：https://developer.apple.com/documentation/swiftui/text
- 字体：系统字体家族 SF（SF Pro/SF Compact/SF Arabic/SF Armenian/SF Georgian/SF Hebrew/SF Mono + 各自 rounded 变体）与衬线 New York（NY），全部支持 Dynamic Type 缩放；HIG 给出各平台推荐默认与最小字号表，明令避免 Ultralight/Thin/Light 细字重、字号放大时优先内容层级（tab 标题不随正文放大）。出处：https://developer.apple.com/design/human-interface-guidelines/typography
- SF Symbols：6000+ 图标系统，自动适配暗色与 Dynamic Type，配 `symbolEffect` 动效（bounce/pulse 等，WWDC23 起）。出处：https://developer.apple.com/design/human-interface-guidelines/sf-symbols

### 十一、性能与掉帧治理：hitch 量化分级 + 因果图 + 官方设计纪律

- hitch 定义与硬阈值（官方数值）：连续运动（滚动/拖拽/动画）中的卡顿为 hitch；Xcode Organizer Hitches metric 覆盖全部动画类型，按「毫秒停顿/秒（ms/s）」计量：≤10 good、≤25 warning、≤50 critical、>50 立即处理。hitch 分 commit hitch（主线程提交延迟）与 render hitch（渲染服务端延迟）两类；帧不等于掉帧——掉帧只是迟到帧的系统恢复手段之一。出处：https://developer.apple.com/documentation/xcode/understanding-hitches-in-your-app
- 渲染管线三时标（官方拆解）：render loop 多阶段并行多帧，120Hz 即每 8.3ms 一帧、每阶段 8.3ms 预算；从 presentation time 反推 commit deadline 与 begin time，通常对齐 vsync；render server 是独立进程但替 app 干活，查 hitch 必须同时看主线程与 render server。离散交互的 hang 阈值：<100ms 难察觉、几百 ms 即感无响应，几乎总是主线程长任务。出处：https://developer.apple.com/documentation/xcode/understanding-hitches-in-your-app 与 https://developer.apple.com/documentation/xcode/understanding-hangs-in-your-app
- Instruments SwiftUI 专项（不是通用 profiler 的贴牌）：SwiftUI track 内分 View Body Updates/Platform View Updates/Other Updates 三车道按模块（自家/第三方 SDK/系统）分色；Update Groups 聚合高频短更新；点击单个更新 Show Causes 出因果图（cause-and-effect graph）——节点是视图 body/环境对象/事务（蓝=应用代码、灰=系统），边是因果关系（如 Observable 属性到读它的 body）；官方直接把「迁移到 @Observable 宏按属性失效」列为性能处方之一。出处：https://developer.apple.com/documentation/xcode/understanding-and-improving-swiftui-performance
- 官方 SwiftUI 性能纪律：body 内禁长计算（异步+缓存）；不在 View 初始化器/onAppear/onChanged 里做重活；避免视图存闭包（闭包捕获父视图状态会放大失效范围，能在初始化器里调用就只存返回值）；GeometryReader/ScrollViewReader 观察父布局会放大更新范围，收窄依赖布局的状态子树；Liquid Glass 容器数量与容器外效果数量要克制（官方明文）。出处：https://developer.apple.com/documentation/xcode/understanding-and-improving-swiftui-performance 与 https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views

### 十二、开发者工具预览热重载调试：#Preview 宏 + 画布四模式

- `#Preview` 宏族（Xcode 15 起 Swift 宏，替代旧 PreviewProvider 协议）：`Preview(_:body:)` 基础形；traits 形传设备/朝向/字号等配置特性；`Preview(_:traits:body:cameras:)` 多视角；immersionStyle/windowStyle 场景形（visionOS/多窗口预览）；`@Previewable` 宏让 `@State`/`@Binding` 直接在预览闭包内声明（免 wrapper 视图样板）；`PreviewModifier` 协议把昂贵对象（网络/磁盘初始化）做成跨预览共享上下文；development assets 目录让预览专用资源不进发布包。适用 SwiftUI/UIKit/AppKit 三框架。出处：https://developer.apple.com/documentation/xcode/adding-previews-to-your-interface-files 与 https://developer.apple.com/documentation/swiftui/previews-in-xcode
- 画布交互四模式：live 模式（等价真机运行，可测动画/文本输入/异步）；selectable 模式双击画布元素双向高亮源码并即时生效；variant 模式多配置并排对比（朝向、无障碍字号、设备族如 iPhone Duo）；resizable 任意拖拽改屏幕尺寸验证自适应。出处：https://developer.apple.com/documentation/xcode/interacting-with-previews-in-the-canvas
- 预览进程机制（独立进程 + XPC + Swift dynamic replacement 函数级热替换、状态注入）已在基线 GUI-08 §131 成文（fatbobman/NSHipster/Guardsquare 三方拆解），本文不重复；官方文档侧的对应物是 DebugReplaceableView（debug 构建可替换视图）。出处：https://developer.apple.com/documentation/swiftui/previews-in-xcode
- 调试面：LLDB 断点与变量检查（stepping-through-code-and-inspecting-variables）、View Debugger 诊断运行中视图外观（diagnosing-issues-in-the-appearance-of-your-running-app）、Metal debugger、Instruments、Xcode Organizer 度量。出处：https://developer.apple.com/documentation/xcode/debugging

### 十三、设计系统与规范文档：HIG 2025 至 2026 演进 = Liquid Glass 双层材料体系

- 材料二分法（HIG Materials 页现行口径）：平台材料分两类——Liquid Glass 是动态材料、统一全平台设计语言、承载「控件与导航功能层」（tab bar/侧栏浮于内容之上，内容滚动透出、保持控件可读）；standard materials（UIBlurEffect/UIVibrancyEffect/SwiftUI Material）承载「内容层」结构表达。明令「Don't use Liquid Glass in the content layer」——内容层禁用玻璃（唯一例外：滑块/开关等瞬时交互控件在激活态取玻璃外观强调交互性）。出处：https://developer.apple.com/design/human-interface-guidelines/materials
- Liquid Glass 两变体与硬数值：regular 变体模糊+亮度调整保可读（多数系统组件用它，文本多的 alert/sidebar/popover 用它）；clear 变体高度透明（媒体背景上的浮动控件）；clear 上遇亮底建议加 35% 不透明度黑色调光层（官方数值）；滚动边缘效果（scroll edge effect）辅助可读性。材料对系统设置响应：用户在设置里选 Liquid Glass 外观偏好、开 Reduce Transparency/Increase Contrast 都会改变材料呈现——设计必须预设这些态。出处：https://developer.apple.com/design/human-interface-guidelines/materials
- API 面（Applying Liquid Glass to custom views）：`glassEffect(_:in:)` 修饰符（默认 regular 变体 + Capsule 形状；可换形状、`.tint()` 着色、`.interactive()` 获得与系统按钮同款触摸/指针实时反应）；`GlassEffectContainer(spacing:)` 容器统一渲染多个玻璃元素（官方明示性能最优路径）且 spacing 控制形状融合/变形阈值；`glassEffectUnion(id:namespace:)` 多视图几何合并为单胶囊；`glassEffectID(_:in:)` + Namespace 做玻璃元素间 matchedGeometry/materialize 变形转场（GlassEffectTransition）；`buttonStyle(.glass)`。出处：https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views 与 https://developer.apple.com/documentation/swiftui/view/glasseffect(_:in:)
- 设计系统配套规范完整度：vibrancy 层级表（label 四级/fill 三级/separator）、语义色用途表、typography 默认/最小字号、motion 规范（尊重 Reduce Motion）、buttons/layout/accessibility 各组件页；HIG 与开发者文档双向互链（每个 HIG 页尾带 Developer documentation 链接，每个 API 页带 For design guidance 回链）——设计规范与 API 文档一体的维护模式本身是对标点。出处：https://developer.apple.com/design/human-interface-guidelines/materials（页尾 Resources 结构）

### 十四、特色能力：系统能力面（WidgetKit/App Intents/Swift Charts/SwiftData/TipKit/触觉）

- WidgetKit 一框架四形态：widgets、watch complications、Live Activities（锁屏/灵动岛实时活动）、controls（iOS 控制中心控件），全部 SwiftUI 声明式构建、系统级刷新调度。出处：https://developer.apple.com/documentation/widgetkit
- App Intents：动作与内容对 Apple Intelligence/Siri/Spotlight/Shortcuts/widgets 全系统可发现——AI 时代的入口标准化层。出处：https://developer.apple.com/documentation/appintents
- Swift Charts（全平台声明式图表）、SwiftData（「Write your model code declaratively」声明式模型持久化，与 @Observable 同一代际）、TipKit（功能发现提示系统）、`sensoryFeedback` 触觉反馈修饰符（SwiftUI 原生、按状态触发）。出处：https://developer.apple.com/documentation/charts 、https://developer.apple.com/documentation/swiftdata 、https://developer.apple.com/documentation/tipkit 、https://developer.apple.com/documentation/swiftui/sensoryfeedback

## 证据与出处

全部一手出处已在正文各条内联给出，主 URL 汇总：

- 状态管理：https://developer.apple.com/documentation/swiftui/managing-user-interface-state ；https://developer.apple.com/documentation/swiftui/migrating-from-the-observable-object-protocol-to-the-observable-macro ；https://developer.apple.com/documentation/observation
- 视图基础与身份：https://developer.apple.com/documentation/swiftui/view-fundamentals ；https://developer.apple.com/documentation/swift/identifiable
- 组件：https://developer.apple.com/documentation/swiftui/controls-and-indicators
- 动画：https://developer.apple.com/documentation/swiftui/animations ；https://developer.apple.com/documentation/swiftui/animation ；https://developer.apple.com/documentation/swiftui/controlling-the-timing-and-movements-of-your-animations ；https://developer.apple.com/documentation/swiftui/withanimation(_:_:) ；https://developer.apple.com/documentation/swiftui/view/animation(_:value:)
- 手势：https://developer.apple.com/documentation/swiftui/gestures ；https://developer.apple.com/documentation/swiftui/uigesturerecognizerrepresentable ；https://developer.apple.com/documentation/uikit/uigesturerecognizer
- 输入法：https://developer.apple.com/documentation/swiftui/focus ；https://developer.apple.com/documentation/swiftui/view/keyboardtype(_:) ；https://developer.apple.com/documentation/swiftui/view/keyboardshortcut(_:modifiers:localization:) ；https://developer.apple.com/documentation/uikit/uitextinput
- 多窗口：https://developer.apple.com/documentation/swiftui/windows ；https://developer.apple.com/documentation/swiftui/windowgroup ；https://developer.apple.com/documentation/uikit/uiwindowscene
- 无障碍：https://developer.apple.com/documentation/swiftui/accessibility-fundamentals ；https://developer.apple.com/documentation/swiftui/accessible-appearance ；https://developer.apple.com/design/human-interface-guidelines/accessibility
- 主题暗色取色：https://developer.apple.com/design/human-interface-guidelines/dark-mode ；https://developer.apple.com/design/human-interface-guidelines/color ；https://developer.apple.com/documentation/swiftui/material ；https://developer.apple.com/documentation/swiftui/view/preferredcolorscheme(_:)
- 自适应布局：https://developer.apple.com/design/human-interface-guidelines/layout ；https://developer.apple.com/documentation/swiftui/layout ；https://developer.apple.com/documentation/swiftui/anylayout ；https://developer.apple.com/documentation/swiftui/viewthatfits ；https://developer.apple.com/documentation/swiftui/navigationsplitview
- 国际化字体：https://developer.apple.com/documentation/xcode/localizing-and-varying-text-with-a-string-catalog ；https://developer.apple.com/documentation/swiftui/text ；https://developer.apple.com/design/human-interface-guidelines/typography ；https://developer.apple.com/design/human-interface-guidelines/sf-symbols
- 性能：https://developer.apple.com/documentation/xcode/understanding-hitches-in-your-app ；https://developer.apple.com/documentation/xcode/understanding-hangs-in-your-app ；https://developer.apple.com/documentation/xcode/understanding-and-improving-swiftui-performance ；https://developer.apple.com/documentation/swiftui/performance-analysis
- 开发工具：https://developer.apple.com/documentation/xcode/adding-previews-to-your-interface-files ；https://developer.apple.com/documentation/xcode/interacting-with-previews-in-the-canvas ；https://developer.apple.com/documentation/swiftui/previews-in-xcode ；https://developer.apple.com/documentation/xcode/debugging
- 设计系统：https://developer.apple.com/design/human-interface-guidelines/materials ；https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views ；https://developer.apple.com/documentation/swiftui/view/glasseffect(_:in:)
- 特色能力：https://developer.apple.com/documentation/widgetkit ；https://developer.apple.com/documentation/appintents ；https://developer.apple.com/documentation/charts ；https://developer.apple.com/documentation/swiftdata ；https://developer.apple.com/documentation/tipkit ；https://developer.apple.com/documentation/swiftui/sensoryfeedback

基线互链（不重复调研的部分）：SwiftUI 声明式模型与反射 diff 对比 = [GUI-07 声明式 UI 框架对比](../../research/GUI-07-声明式UI框架对比.md) §2.1；@Observable 属性级失效与四家收敛 = [GUI-10 移动优先渲染深研综合裁决](../../research/GUI-10-移动优先渲染深研综合裁决.md) §518 前后；SwiftUI Preview 独立进程机制 = [GUI-08 输入管线与开发套件](../../research/GUI-08-输入管线与开发套件.md) §131；OpenHarmony 侧对照规格 = [2026-11-14-survey-openharmony-ui-specs.md](2026-11-14-survey-openharmony-ui-specs.md)。

## 适用边界

全部取 2026-11-15 线上当前版本文档（iOS 26/Xcode 26 世代），未锁版本快照，Apple 文档随版本漂移需复核；SwiftUI diff 的内部实现（AttributeGraph）与预览进程机制 Apple 均未官方文档化，正文相应条目已标注并只引基线二手结论；HIG 为设计规范不含 API 参数全表（如 typography 页的字号数值表随平台版本变化，本文未抄录具体数值，BerryOS 取数时应直接核 HIG Typography 页）；Instruments 截图操作步骤在 JSON 化提取中丢失配图说明，操作级细节以 Xcode 文档原文为准；本调研为 BerryOS UI 框架对标提供机制参照，不构成移植方案。
