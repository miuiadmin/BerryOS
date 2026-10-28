# GUI-07 声明式 UI 框架对比——「容易开发 + 渲染高效 + 正确」的现代解法与 BerryOS SDK 形态建议

> **调研定位**：M-GUI 线（`docs/GUI里程碑规划-2026-10.md`）专项问题报告。用户 2026-10-28 裁决六条（最先进渲染方案 / 移动设备为基准 / 吸取安卓渲染响应慢教训 / 又低功耗又优秀 / 先进渲染 API 兼容但原生适配才算数 / 开发套件要「容易渲染、高效、正确」）中，本报告主答**编程模型与 SDK 语言形态**（裁决第 ⑥ 条 + 第 ③④ 条的模型面）；渲染后端（GPU/合成器）归 GUI 线其它报告。
>
> **证据纪律**：联网论断标 URL；本地论断标仓库路径。确定度分三级：**高**（官方文档/一手源码/多篇独立佐证）、**中**（单一高质量来源或社区共识）、**低**（推断，需原型验证）。
>
> **协议洁净**：Slint 为 GPL/商业双许可、Flutter/Compose/React 各有非 Apache 许可——**全部只看思路与机制，不搬代码**（CLAUDE.md 协议洁净铁律）。

---

## 1. 问题定义：状态→UI 更新模型是声明式 UI 的心脏

所有现代声明式 UI 框架回答同一个问题：**应用状态变化后，屏幕上哪些像素需要重算、由谁决定、成本几何**。差异全部落在三轴上：

1. **变更检测**：怎么知道「什么变了」——人工调 invalidate / 框架 diff / 编译期依赖图；
2. **传播粒度**：变化重算的范围——组件级（整个函数体重跑）vs 绑定级（直达具体某个文本节点）；
3. **谁来做**：运行时做（diff/订阅图）vs 编译器做（反应性展开、自动 memo）。

2019–2026 年的行业演化可以一句话概括：**从「运行时组件级 diff」走向「编译期细粒度反应性 + 编译器自动记忆化」**——这恰好是「高效又正确又容易开发」三条要求的合流点，也是本报告最重要的结论（详见 §4）。

---

## 2. 七大模型精确对比（必答问题 1）

### 2.1 SwiftUI：值类型 View + 属性级依赖追踪 + 反射 diff

**机制（两段式）**：
- **何时重评**：`@Observable` 宏（Observation 框架，WWDC23 起）在 view 的 `body` 执行期间于**属性访问级**登记依赖——body 读了哪个可观察属性，就只订阅哪个属性；属性变了才重评该 body。这与旧 `ObservableObject`（任何 `@Published` 变更刷新所有观察视图）是代差。证据：Apple 官方迁移指南明确「updates a view only when an observable property that the view's body actually reads changes」([developer.apple.com](https://developer.apple.com/documentation/swiftui/migrating-from-the-observable-object-protocol-to-the-observable-macro))；WWDC23 Session 10149 ([developer.apple.com](https://developer.apple.com/videos/play/wwdc2023/10149))；Donny Wals 的机制拆解（[donnywals.com](https://www.donnywals.com/observable-in-swiftui-explained)）。
- **重评后怎么办**：body 产出的 View 是**轻量值类型描述**，SwiftUI 用**基于反射的属性 diff** 比较新旧 view 的存储属性，`Equatable` 一致则短路整棵子树。证据：Airbnb 工程博客「SwiftUI uses a reflection-based diffing algorithm to compare each view's stored properties」([airbnb.tech](https://airbnb.tech/mobile/understanding-and-improving-swiftui-performance))。

**评价**：失效粒度 = 属性级（依赖登记）+ body 级（重执行）；重评 body 时分配一整棵描述值树（分配压力中高，靠编译器值语义优化与 diff 收窄）。易错点：超大 body 未拆分（一次失效重评半个屏）、`body` 内副作用、`@State`/`@Binding` 所有权混淆、列表 `Identifiable` 错误。

### 2.2 Jetpack Compose：slot table + 位置记忆化 + 重组作用域 + Strong Skipping

**机制**：编译器把 `@Composable` 函数改写为带 `$composer`/`$changed` 参数的可中断函数，组合状态存进 **SlotTable**（两组平行数组 `groups`/`slots` + gap buffer，支持结构变化时高效挪洞）。`remember` 按**代码位置**（不是变量名）读写槽位——这是「位置记忆化」（positional memoization）。权威一手来源：Google Compose 团队 Leland Richardson 的两篇内部机制文（[part 1](https://medium.com/androiddevelopers/understanding-jetpack-compose-part-1-of-2-ca316fe39050)、[part 2](https://medium.com/androiddevelopers/under-the-hood-of-jetpack-compose-part-2-of-2-37b2c20c6cdd)）；SlotTable/gap buffer 细节见社区源码走读（[stackademic](https://blog.stackademic.com/jetpack-compose-internals-how-slot-table-gap-buffer-make-ui-fast-83fe7ad12494)、[devgenius](https://blog.devgenius.io/jetpack-compose-runtime-part-7-the-slot-table-how-compose-remembers-the-composition-c1de6a7f9db5)）。

- **重组作用域**（recomposition scope）：状态读取把包围它的**可跳过 lambda 边界**标脏，下次帧只重执行该作用域；「甜甜圈洞跳过」（donut-hole skipping）让作用域内部未读状态的子调用仍可跳过（[Jorge Castillo 指南](https://newsletter.jorgecastillo.dev)、[thecodingshef](https://thecodingshef.in)）。
- **Strong Skipping**（1.5.4 引入，Kotlin 2.0.20 时代起成为 Compose 编译器默认——前者多篇社区佐证、后者为 Google 官方公告口径，确定度高）：**所有 composable 一律 skippable**，不稳定参数（如普通 `List`）自动记忆化并按实例相等性比较，lambda 自动记忆——「注解地狱」（`@Stable`/`@Immutable`/`ImmutableList`）大幅退场。证据：[patilshreyas/Medium](https://medium.com/@patilshreyas/stop-ignoring-compose-stability-yes-even-with-strong-skipping-mode-085bbd55fe98)、[kacper.kalinowski/Medium](https://medium.com/@kacper.kalinowski/mastering-recomposition-in-jetpack-compose-284e2ce9f4e1)、[dev.to](https://dev.to/myougatheaxo/compose-stability-and-recomposition-optimization-stableimmutableskippable-3cf4)。

**评价**：diff 成本 = slot table 位置比对 + 参数相等检查；分配压力中（lambda 记忆化产生包装对象）；失效粒度 = 重组作用域级。易错点：不稳定参数引用不等导致白比对（Strong Skipping 后仍存在）、`key()` 缺失导致列表/分支状态错位、在组合里做重活（组合可能执行多次——**组合函数必须无副作用**是心智模型的核心纪律）。

### 2.3 Flutter：三棵树 + Element diff + 自绘

**机制**：Widget（不可变轻量配置，`setState` 即重建子树配置）/ Element（持久化的协调层，持有状态）/ RenderObject（布局绘制）三棵树；Element 树是 diff 主体，`Widget.canUpdate`（runtimeType + key 相等）决定 Element 复用，只有配置真变才下推到 RenderObject。证据：Flutter 官方 Inside Flutter（[docs.flutter.dev](https://docs.flutter.dev/resources/inside-flutter)，RenderObject 树为 Element 树子集的同构关系）；三树职责与重建时机拆解（[hashnode](https://ishaquehassan.hashnode.dev)、[bilalahmad72.com](https://bilalahmad72.com)）。

**评价**：diff/重建成本中——**Widget 重建是整子树配置对象重新分配**（分配压力七模型中最高），换来的是「widget 便宜可丢弃」的心智简单；失效粒度 = `setState` 所在 State 的 Element 子树级（组件级，最粗一档）。易错点：父 `setState` 整子树 rebuild 链、缺 `const` 构造、build 里做重活、`GlobalKey` 滥用。缓解手段是 `Selector`/`Bloc` 等外部状态库把失效收窄——等于承认组件级粒度不够细，需要在应用层再叠一层。

### 2.4 React 18/19：reconciliation + 并发 + React Compiler

**机制**：`setState` 调度重渲染，组件函数整跑，Fiber 架构把渲染拆成可中断单元（时间分片/transition，React 18）；reconciler 按 key + 类型 diff 虚拟树。React 19 时代配套 **React Compiler**：编译期在**表达式级**自动插入 memoization（等价自动 `useMemo`/`useCallback`/`React.memo`），依据是对组件代码的依赖分析——官方口径从 Floyd Hines 时代的「rendering is cheap」**转向**「自动记忆化」。（机制综述：[zestgeek/Medium](https://medium.com/zestgeek/react-compiler-the-end-of-manual-memoization-in-react-26c07341168e)；立场转变史：[pnkz/Medium](https://medium.com/@pnkz/why-react-changed-its-stance-on-renders-from-rendering-is-cheap-to-automatic-memoization-with-a233ba2ae7d2)。）

**评价**：diff 成本高（组件级重跑 + 树 diff），靠 Compiler 把无效重跑压掉；失效粒度 = 组件级（Compiler 后细到值/JSX 元素级）；心智模型在 Compiler 之前是七模型中最重的（依赖数组、闭包陈旧值、`useEffect` 语义）。易错点（Compiler 前）：依赖数组漏项/多项、无限重渲染、列表 key、stale closure。**React Compiler 的最大意义是把这套纪律从人转到编译器**——与 Compose Strong Skipping 隔洋同构（§4）。

### 2.5 Solid.js：细粒度信号 + 无组件级 diff

**机制**：组件函数**只执行一次**（构建期建模板），JSX 编译为真实 DOM 模板 + 每个插值表达式编译成独立的 effect 节点接入信号依赖图；状态变更沿信号图**直达具体绑定点**，无组件重渲染、无 VDOM diff。证据：官方文档 Fine-grained reactivity（[docs.solidjs.com](https://docs.solidjs.com/advanced-concepts/fine-grained-reactivity)）；作者 Ryan Carniato 访谈（[The New Stack](https://thenewstack.io/solidjs-creator-on-fine-grained-reactivity-as-next-frontier)）；三框架反应性对比（[vrite.io](https://vrite.io/blog/solid-js-react-and-vue-reactivity-systems-compared)）。

**评价**：更新路径最短（信号→绑定，跳过 diff），运行时开销 = 维护订阅图的细粒度成本；分配压力低（无整树重建）；失效粒度 = **绑定级**（最细一档）；代价是心智反转——「组件是模板不是函数」：解构 props 会丢反应性、组件体里的信号读取不建立绑定，是 React 使用者的高频翻车点。

### 2.6 Svelte 5：runes 显式编译期反应性

**机制**：`$state`/`$derived`/`$effect` 三个编译器关键字（runes）取代 Svelte 4 的隐式 `let` 反应性；编译器把 runes 直接编译为基于信号的细粒度更新代码，无 VDOM、运行时极小；服务端渲染时反应性整体剥除（$state 退化为普通变量）。证据：runes 机制拆解（[aiherrera](https://blog.aiherrera.com/part-2-svelte-5-reactivity-and-the-role-of-runes)、[flavio.pinnelli/Medium](https://medium.com/@flavio.pinnelli/svelte-5-how-runes-change-the-game-dc7773549c7e)、[richard-a-brown/Medium](https://richard-a-brown.medium.com/from-magic-to-mechanics-a-senior-architects-guide-to-svelte-5-runes-2506f1774128)）；「编译后零运行时」讨论（[r/sveltejs](https://www.reddit.com/r/sveltejs/comments/1dbvse0/compile_svelte_5_components_with_no_runtime)）；跨模块通用反应性提案（[sveltejs/svelte#12947](https://github.com/sveltejs/svelte/discussions/12947)）。

**评价**：Svelte 4→5 的转折点是**从「魔法隐式」走向「显式关键字」**——隐式反应性在规模变大后不可推断（哪个变量是响应的？编译器说了算，人猜不准），runes 把反应性变成可见语法。失效粒度 = 绑定级 + `$derived` 精确依赖；易错点：`$effect` 滥用（本该用 `$derived` 的派生值写成副作用）、SSR 进程状态残留（[sveltejs/kit#12507](https://github.com/sveltejs/kit/issues/12507)）。

### 2.7 ArkUI（OpenHarmony）：状态装饰器两代（V1 对象级 → V2 属性级）

**机制**：V1 = `@State`/`@Prop`/`@Link`/`@Provide` 族，类观测为**对象级**（`@Observed`+`@Track` 才能到属性级，且无深度观测）；V2 = `@ObservedV2`+`@Trace` **属性级精确观测 + 深度观测**、`@Local`/`@Param`/`@Monitor`，状态变量独立于 UI 组件。证据：华为官方《状态管理 V1 和 V2 更新机制差异》（[developer.huawei.com](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/arkts-v1-v2-update-difference)）、官方总览（[developer.huawei.com/en](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/arkts-state-management-overview)）、社区迁移实战（[掘金](https://juejin.cn/post/7536109341255925760)、[segmentfault](https://segmentfault.com/a/1190000046864738)）。源码结构同样两代并存：ace_engine 仓 `frameworks/bridge/declarative_frontend/state_mgmt/src/lib/` 下 `full_update`/`partial_update`/`v2` 三目录（本地转引：`docs/GUI里程碑规划-2026-10.md` 附录 B L2「状态管理装饰器」条，源码基线 `~/Documents/harmony/openharmony/`）。

**评价**：V1 的痛点（嵌套对象不触发刷新、未 @Track 全对象重刷、V1/V2 不能混用）正是华为自造 V2 的原因——**OpenHarmony 自己用两代状态管理的迭代证明了「组件/对象级失效不够，属性级细粒度才是终局」**，与 SwiftUI Observable 的代差方向完全一致。差量更新管线（UINode 最小差量 + ForEach/LazyForEach 懒加载 + 任务合批）在完整档 ace_engine 落地（GUI 规划附录 B 转引：`frameworks/core/components_ng/` + `pipeline_ng/ui_task_scheduler.cpp`）。

### 2.8 六维评估总表（必答问题 2）

| 模型 | diff/重建成本 | 运行时开销 | 分配压力 | 失效粒度 | 心智模型复杂度 | 典型易错点 |
|---|---|---|---|---|---|---|
| SwiftUI | body 重评后**反射属性 diff**（Equatable 短路） | Observation 登记薄 | 中高（值树整棵重分配） | 属性级依赖 + body 级重评 | 低中（值语义天然单向） | 巨型 body、body 副作用、Identifiable 错 |
| Compose | slot table 位置比对 + 参数相等检查 | 编译器改写 + 记忆化包装 | 中 | 重组作用域级（Strong Skipping 收窄） | 中（无副作用纪律 + 稳定性概念） | 不稳定参数白比对、key() 缺失、组合期副作用 |
| Flutter | Widget 整子树重建 + Element canUpdate diff | 三树维护 | **高**（配置对象洪泛） | setState 子树级（最粗） | 低（「widget 便宜」） | rebuild 链、缺 const、build 重活 |
| React 19+Compiler | 组件重跑 + reconciler，Compiler 压无效重跑 | Fiber 调度 + memo 缓存 | 中高（Compiler 后下降） | 组件级（Compiler 后值级） | Compiler 前**最高**，后大降 | 依赖数组、stale closure、key |
| Solid | **无 diff**（信号直达绑定） | 订阅图维护（细而多） | **低** | **绑定级**（最细） | 中（组件=模板的心智反转） | 解构丢反应性、组件体读信号 |
| Svelte 5 | 无 diff（编译产物直达） | 极小运行时 | 低 | 绑定级 + $derived | 低（显式 runes） | $effect 滥用、SSR 状态残留 |
| ArkUI V2 | 属性级观测 + UINode 差量管线 | 装饰器编译/运行时双轨 | 中 | 属性级（V2）/对象级（V1） | 中高（两代并存迁移痛） | V1 嵌套不刷新、V1/V2 混用、@Track 漏标 |

---

## 3. 「高效又正确」机制清单（必答问题 3）

1. **编译期反应性：diff 非必需**（确定度：高）。Solid 与 Svelte 5 双实例证明：把依赖关系在编译期展开成信号图，更新直达绑定点，组件级 diff 整层可删。SolidJS 作者将细粒度反应性称为「next frontier」（[The New Stack](https://thenewstack.io/solidjs-creator-on-fine-grained-reactivity-as-next-frontier)）；Svelte 5 编译后运行时趋零（[r/sveltejs](https://www.reddit.com/r/sveltejs/comments/1dbvse0/compile_svelte_5_components_with_no_runtime)）。
2. **编译器替人做 memo：两大阵营同时收敛**（确定度：高）。React Compiler（表达式级自动 memo，[zestgeek](https://medium.com/zestgeek/react-compiler-the-end-of-manual-memoization-in-react-26c07341168e)、[pnkz](https://medium.com/@pnkz/why-react-changed-its-stance-on-renders-from-rendering-is-cheap-to-automatic-memoization-with-a233ba2ae7d2)）与 Compose Strong Skipping（全部 skippable + 不稳定参数自动记忆化，[patilshreyas](https://medium.com/@patilshreyas/stop-ignoring-compose-stability-yes-even-with-strong-skipping-mode-085bbd55fe98)）在 2024–2025 年做了同一件事：**把「手动 memo 的纪律」从开发者转移到编译器**。行业含义：新框架若还要求人手写 memoization 才能跑得快，就是代际落后。
3. **失效粒度持续细化是行业方向**（确定度：高）。SwiftUI ObservableObject→Observable、ArkUI V1→V2、React 组件级→Compiler 值级、Flutter 子树级→应用层叠加选择器——四家全在从粗粒度走向属性/绑定级。粗粒度的代价不只是性能：**过度刷新 = 无效渲染（ArkUI V2 官方优化文档的直接动机）**，也是正确性噪声（动画被无关刷新打断）。
4. **不可变状态 + 单向数据流的正确性收益**（确定度：高，机制论证）。状态变更 = 显式消息/事件 → 唯一更新函数 → 新状态 → 视图纯函数推导。收益三件：① 状态转换可枚举可回放（调试/时间旅行）；② 视图函数纯 → 同输入同输出 → **可确定性测试**；③ 并发安全（无共享可变态）。Elm/Redux→SwiftUI/Compose 的 `state hoisting` 全是此谱系。**对 BerryOS 额外重要**：纯函数模型与「AI 测试友好化」（确定性可复现）直接同构。
5. **显式优于隐式（Svelte 5 runes 的教训）**（确定度：中高）。隐式反应性（Svelte 4 的 `let`、早期 Vue）在人无法推断哪些变量响应时崩溃；Svelte 5 转向显式关键字换可推断性。设计新 DSL 时：**反应性边界必须是可见语法**。
6. **组合期无副作用纪律**（确定度：高）。Compose 的「composable 可能以任意顺序、任意多次执行」与 React StrictMode 双调用都是同一执法：把视图构建当纯函数。违反即产生不可复现 bug—— BerryOS 的 UI 构建函数应当同规。

---

## 4. 即时模式 vs 保留模式（必答问题 4）

**egui 代表的即时模式**：每帧从零重跑整个 UI 构建代码（根在游戏开发，画面本来每帧重画）。心智最简（UI = 状态的纯函数，无同步 bug）、单帧重建成本微秒级。证据：[egui GitHub](https://github.com/emilk/egui)、教程（[coddykit](https://www.coddykit.com/courses/rust/immediate-mode-ui-basics-9542082)）。

**结构性代价**：
- **空闲也在画**：无输入无变化时仍逐帧重跑（空闲功耗敌人）——社区明确指出保留模式「只在输入事件时重绘」对 idle 应用是数量级优势（[HN 讨论](https://news.ycombinator.com/item?id=44395421)）；
- **无稳定对象标识**：每帧重建 → 无障碍树/自动化测试的稳定锚点弱（Slint 的 AI 验证方案恰恰靠无障碍树驱动 UI，见 §6.4——对照出即时模式的缺口）；
- 每帧全量布局：树大后成本线性涨，靠缓存层（egui 内部也有 pass 缓存）部分缓解，但架构上仍是全帧重算。

**保留模式**：UI 是持久树 + 失效标记 + 按需重绘（invalidate/脏区）。OpenHarmony ui_lite 是嵌入式保留模式的现成实证：**ROM 900KB / RAM ≈90KB** 跑完整组件集 + 脏区刷新（本地证据：`docs/GUI里程碑规划-2026-10.md` 附录 B L0/L2 多条带 UIL 源码路径；社区综述 [users.rust-lang.org](https://users.rust-lang.org/t/egui-vs-iced-in-regards-to-game-engine-integration/74569/6) 亦指出连 Unity 都从即时转回保留）。

**选择依据（落到 BerryOS）**：
- **常驻系统 UI / 移动设备主屏 = 保留模式 + 脏区，无悬念**：低功耗是 BerryOS 身份支柱（CLAUDE.md），空闲零重绘 + 事件驱动是硬需求；现行规划 GUI-3 berry-ui（View/ViewGroup/Invalidate + 脏区合并 + 软 vsync 帧循环）已按此裁决（`docs/GUI里程碑规划-2026-10.md` §4 GUI-3 行、§2 闭包第 5 件）——**本报告确认该裁决与行业证据一致**。
- **即时模式的合理生态位**：调试 overlay、DFX 工具、boot 画面、游戏内 HUD——每帧本来就要重画、生命周期短、无省电约束的面。BerryOS 可在 DFX 三件（GUI-9）里给一个小型即时模式面，但**不进主线 UI 框架**。
- fbcon 现状定位：`user/berry-fbcon`（28 host 测，font.rs/render.rs 43 个函数）本质是立即模式绘制原语集——它升级为 berry-gfx 六原语 trait 层（GUI-2 规划在册）后正好同时服务两种模式（保留树的 Render 遍历与即时工具都消费同一组原语）。

**吸取安卓渲染响应慢的教训（用户裁决 ③，必答）**：Android jank 的三个机制性根源——① **vsync 管线的硬预算**：60Hz 下超 16ms 即错过脉冲变 32ms（帧率腰斩），输入跟手性直接受此管线对齐拖累（[Unity 论坛实证讨论](https://discussions.unity.com/t/removing-touch-drag-lag-for-android/778730)、[BrowserStack 渲染性能指南](https://www.browserstack.com/docs/app-performance/app-performance-guides/android/ui-rendering-performance)）；② **深 View 树遍历**：measure/layout/draw 逐级全树走，层级深即慢（[vocal.media](https://vocal.media/journal/how-to-handle-slow-rendering-in-complex-android-ui-layouts)）；③ **主线程重活**（布局/反射/序列化挤占帧预算）。对 BerryOS 的直接转译：**帧预算 day-one 进断言**（AI 测试友好化：每帧 Measure+Render 的周期数走 B-2 icount 通道定标——项目已有此基建）；**输入→首像素变化短路径**（手势 referee 直接标脏目标视图，不经全局调度绕行）；**布局两遍但树浅**（lite 档线性+网格两算法即可表达桌面，规划已在册）；**逻辑/渲染分离留缝**（v1 单线程协作循环起步，v2 动画服务端执行——OpenHarmony「动画移合成服务」同构，GUI 规划附录 B L0 已记）。

---

## 5. AI 生成 UI 友好度（必答问题 5，BerryOS 特有重点）

### 5.1 表达形态谱系与 LLM 适配

| 形态 | 例 | LLM 生成可靠性 | 静态验证 | 运行时效率 | 备注 |
|---|---|---|---|---|---|
| 类 HTML/JSX | React JSX、v0 产物、rsx! | **最高**（预训练语料洪泛区） | 中（JSX 靠 TS/eslint；rsx! 靠 rustc） | 中（需框架 diff/memo） | v0/Figma Make 全在此档（[Vercel 社区](https://community.vercel.com/t/v0-chat-completion-api-with-a-custom-ui-builder-preview-code-area/12107)） |
| 类型化 DSL | .slint、SwiftUI（Swift 内嵌 DSL） | 中（语法紧凑但语料少；Slint 需发 AI skill 补课） | 高（DSL 专用编译器） | 高（编译期展开） | Slint 官方 AI 方案 = 项目技能 + MCP 运行时验证（见 §5.3） |
| 结构化 schema（组件名+props JSON） | server-driven UI、生成式 UI intent | 高（受控解码 + schema 校验兜底） | **形式上最高**（schema 即验证器）但「合法 JSON ≠ 语义正确」（[Towards AI](https://pub.towardsai.net/structured-output-from-an-llm-can-be-perfect-json-and-completely-wrong-cbd630be2306)） | 取决于解释器 | daily.dev 论点：emit UI intent 而非 raw code（[daily.dev](https://daily.dev/posts/a-better-approach-to-generative-ui-hwpqccf9w)）；Builder.io「UIs over APIs」（[builder.io](https://www.builder.io/blog/dev-drop-10)） |
| 纯命令式代码 | 手写保留树构建 | 中（语法合法性强，但树深时生成漂移大） | 高（rustc） | 高 | 无声明式语法糖时的兜底 |

### 5.2 业界实践盘点

- **v0（Vercel）**：自然语言 → React + Tailwind 组件（JSX 档）；其 API 返回 markdown 混合解释与代码块（[Vercel 社区实录](https://community.vercel.com/t/v0-chat-completion-api-with-a-custom-ui-builder-preview-code-area/12107)）。
- **Builder.io Fusion/Visual Copilot**：Figma 设计 → 真实组件映射（消费设计系统 token），走「结构化中间表示 + 组件映射」而非自由代码（[builder.io 论坛](https://forum.builder.io/t/design-to-code-using-figma-to-builder-io/6405)、[codeclouds 评测](https://www.codeclouds.com/blog/builder-io-visual-copilot-from-design-to-code-lightning-fast)）。
- **Figma Make**：Figma 内直产 React/Next 代码，绑定既有设计系统（[fryga 对比文](https://spin.fryga.io/glossary/figma-make-vs-v0-which-is-better-for-non-designers)）。
- **生成式 UI 的生产化共识**（综合 daily.dev + Builder.io + 结构化输出实践）：**受约束的组件词表 + 结构化输出 > 自由代码生成**——LLM 从预定义组件清单里选并填 props，渲染器负责映射（[daily.dev](https://daily.dev/posts/a-better-approach-to-generative-ui-hwpqccf9w)、[superjson 结构化输出指南](https://superjson.ai/blog/2025-08-17-json-schema-structured-output-apis-complete-guide)）。

### 5.3 「AI 可靠生成 → 编译器验证 → 运行时高效」三全论证

三全的矛盾点：语料最丰的形态（JSX/HTML）验证最弱（JS 无编译期类型）；验证最强的形态（结构化 schema）语义最浅（合法 ≠ 正确，[Towards AI](https://pub.towardsai.net/structured-output-from-an-llm-can-be-perfect-json-and-completely-wrong-cbd630be2306)）；效率最高的形态（编译期反应性）往往需要专用 DSL（语料稀薄）。

**BerryOS 的解法：Rust 宏 DSL 居中缝合**（确定度：中高，论证如下）：
1. **生成端**：宏 DSL 语法面刻意贴近 JSX（元素树 + 属性键值 + 花括号表达式），LLM 的 React/HTML 知识近迁移；语法域窄（单一宏内、无自由语句层）→ 生成漂移面小；生成后**立即有 rustc 诊断反馈闭环**——错误信息回喂 LLM 迭代，这正是 Claude Code 式 agent 的最优工作流（AI 测试友好化的「AI 能独立跑测试、读结果、报 bug」同构）。
2. **验证端**：宏展开为普通 Rust 构造代码 → **rustc 全强度类型系统执法**（属性类型、枚举穷尽匹配、状态机模式匹配、借用检查）——比任何独立 DSL 的专用校验器强（SwiftUI/rsx 均证此路可行；对照 .slint 独立语言则需要在语言内重建一套类型检查）。
3. **效率端**：宏在编译期可见全部静态结构 → 静态部分（不变的树骨架/样式常量）可提纯为模板，运行时只留动态槽位（Solid/Dioxus 的模板化思路）+ 属性级失效位。这正是 §3「编译期反应性 + 编译器 memo」两条行业趋势在 Rust 里的自然落点。
4. **运行时侧的 AI 面（v2+ 缝）**：另设**结构化 schema 组件树**（组件名+props 的受约束 JSON）作为 AI 动态生成 UI 的运行时消费面——与「编译期宏 DSL」共用同一套组件注册表与渲染器，形态上即 server-driven UI（builder.io 同构）。编译期/运行时两轨共用组件词表，AI 的「可生成组件集」与开发者的「可手写组件集」恒同源。

---

## 6. Rust 生态的声明式形态现状（必答问题 6）

### 6.1 Slint：独立标记语言路线

`.slint` 声明式标记（类 QML）+ Rust/C++/Node/Python 绑定；可 AOT 编译（纯函数编译期优化）也可 `slint-interpreter` 动态加载；工具链是 Rust GUI 生态最完备的：**slint-lsp（LSP 实现）+ VS Code 扩展 + live-preview 设计模式（1.6 起支持拖拽进出布局）**（[docs.rs/slint-lsp](https://docs.rs/crate/slint-lsp/latest/features)、[slint 1.6 讨论](https://github.com/slint-ui/slint/discussions/5165)、[Medium 评述](https://grepix.medium.com/unveiling-slint-a-new-era-of-gui-toolkits-3aeff43a18f9)）。**AI 面**：官方发布「Slint skill」给 AI agent 提供项目级约定 + 内嵌 MCP server——agent 可通过运行中应用的无障碍树检查 UI、用鼠标/触摸/键盘驱动验证（[docs.slint.dev](https://docs.slint.dev/latest/docs/slint/guide/tooling/ai-coding-assistants)）——**「AI 生成 + 运行时 MCP 驱动验证」是业界首个成体系的 GUI AI 工具链，值得 BerryOS DFX 三件（GUI-9 事件注入器 + 视图树 dump）直接对标此形态**。**但**：① 许可为 GPL/royalty-free 商业双许可（社区综述，[docs.slint.dev](https://docs.slint.dev) 为权威口径，确定度中高）——BerryOS 只能看思路；② 独立语言 = .slint 内部不受 rustc 检查，需要自建一整套编译器/LSP/文档栈。

### 6.2 Dioxus：Rust 宏 DSL（rsx!）路线

React 心智 + `rsx!` 宏内类 JSX 语法；热重载专门化——`dioxus-rsx` 解析 DSL 传播字面量/模板热重载信息，0.7 的 Subsecond 进一步做通用 Rust 热补丁（[crates.io/dioxus-hot-reload](https://crates.io/crates/dioxus-hot-reload/0.5.6/code)、[Medium/Subsecond](https://codethoughts.medium.com/hot-reloading-in-rust-subsecond-and-dioxus-to-the-rescue-fdc3d74f7ec9)）。已知缺口：宏内属性类型检查不完整（[dioxus#2398](https://github.com/DioxusLabs/dioxus/issues/2398)）、条件块热重载错位（[dioxus#1995](https://github.com/DioxusLabs/dioxus/issues/1995)）、热补丁与热重载互斥边界（[dioxus#5317](https://github.com/DioxusLabs/dioxus/issues/5317)）。

### 6.3 Leptos：细粒度信号 + view! 宏（Solid 思路）

细粒度反应性在 Rust 里的代表作；但工具链痛点集中：rust-analyzer 在 `view!` 宏内补全被 `#` 语法干扰（[leptos#1781](https://github.com/leptos-rs/leptos/issues/1781)）、proc-macro 重分析拖慢 IDE/编译（[leptos 讨论 #3682](https://github.com/leptos-rs/leptos/discussions/3682)）——后者是 Rust 生态结构性问题：**proc-macro 展开不被增量编译缓存，宏重的项目 DX 显著变差**（[rust-lang internals](https://internals.rust-lang.org/t/caching-of-proc-macro-expansions/21466)）；rust-analyzer 对 proc-macro 内补全的支持是长期已知缺口（[rust-analyzer#18834](https://github.com/rust-lang/rust-analyzer/issues/18834)）。

### 6.4 对比结论

| | Slint | Dioxus | Leptos | 纯代码（iced/egui 族） |
|---|---|---|---|---|
| 形态 | 独立 .slint 标记 | rsx! 宏（类 JSX） | view! 宏 + 信号 | builder/函数调用 |
| rustc 验证 | 宏外全量、语言内自建 | 宏展开后全量（属性面有缺口） | 全量 | 全量 |
| LSP/工具 | **最完备**（LSP+live preview+AI skill/MCP） | VS Code 扩展 + 热重载 | 弱（rust-analyzer 痛） | 原生（就是 Rust） |
| 热重载 | interpreter 天然动态加载 | 字面量级热重载 + Subsecond | 无故事 | 无（重启编译） |
| 许可 | GPL/商业双许可（Apache-2.0 仓不可依赖） | MIT/Apache | MIT | 各异（egui MIT/Apache） |

**要点**：① 独立标记语言买工具链自主（LSP/live-preview 不受 proc-macro 生态拖累）但付「两套语言/类型系统」税；② 宏 DSL 买 rustc 免费验证但付 IDE/热重载税（结构性，短期无解）；③ **AI 时代新增变量：LLM 语料对 JSX 形态的压倒性熟悉让宏 DSL 的「生成端」劣势消失**，而其「验证端」优势（rustc）相对放大——这改变了三条路线的性价比。

---

## 7. BerryOS SDK UI 语言形态建议（必答问题 7）

### 7.1 候选对比

| 候选 | 优点 | 缺点 | 判定 |
|---|---|---|---|
| A. 纯 Rust 代码构建（builder/构造器） | 零新面、rustc 全验证、最快落地（GUI-3 现行路线） | 树深时视觉噪声大、无声明式表意、AI 生成漂移面大（无固定语法骨架） | **v1 采用**（且 API 按数据驱动设计） |
| B. Rust 宏 DSL（rsx 风格声明树） | JSX 语料近迁移（AI 生成友好）+ rustc 验证 + 编译期模板化空间 | IDE 补全/热重载受 proc-macro 生态限制（业界已知，可缓解：DSL 域窄 + cargo expand + rustc 诊断闭环） | **v1.5 叠加，终态主形态** |
| C. 独立标记语言 + 编译器（.slint 式） | 工具链自主（LSP/live preview 不受限）、解释器天然热重载 | 自建语言/编译器/文档栈成本；绕开 rustc；现成参照（Slint）GPL 不可依赖 | 不采纳为独立语言；**其「运行时解释」能力以 JSON schema 面获得** |
| D. 类型化 schema（JSON 组件树） | AI 受控生成最稳、可线上/跨进程递送（server-driven UI）、天然热重载 | 语义浅（合法≠正确）、表达力受限（无控制流抽象）、性能靠解释器 | **v2+ 作为 AI 动态 UI 缝**（与 B 共用组件注册表） |

### 7.2 推荐与论证

**推荐：B 为主、A 为基、D 为缝的分层形态——「同一组件注册表，三个消费面」。**

1. **核心内核 = Elm 式单向数据流 + 保留树脏区渲染**（确定度：高）：`Model（不可变状态）+ Msg（枚举消息）+ update(Model, Msg) -> Model + view(Model) -> 树`。理由：a) Rust 值语义 + 无 GC 天然适合不可变消息传递；b) view/update 纯函数 = 确定性可测（与 fbcon 28 个 host 离屏像素断言测同一方法论，AI 测试友好化支柱的直接兑现）；c) 正确性结构保证（状态转换全枚举，`match msg` 穷尽性由 rustc 执法）；d) 失效推导简单：update 产出新 Model → view 重导出 → **键路径 diff 到属性级失效**（不引入运行时信号图，规避 Rust 借用检查与订阅图的冲突——Leptos 的挣扎是前车之鉴）。
2. **表意层 = 宏 DSL 语法糖**（v1.5）：宏只做「树字面量 → v1 构造 API 调用」的机械展开，保证两轨全量互操作、rustc 全检查、AI 生成友好（§5.3 三全论证）。DSL 域刻意窄：元素 = 组件注册表枚举/构造函数，属性 = 强类型 struct 字段，子节点/插值 = 花括号 Rust 表达式——**不给宏内塞语句层**（防语法漂移、防工具链失控）。
3. **动态缝 = JSON schema 组件树**（v2）：AI 在线生成 UI/远程 UI 更新的运行时消费面，解释器消费同一注册表。Slint interpreter、Builder.io server-driven UI、OpenHarmony 卡片（form）三实例同构（GUI 规划附录 B L4「服务卡片 form」条已留 v2 议位）。
4. **失效粒度路线图**：v1 视图级 Invalidate（现行 GUI-3 规划）→ v1.5 属性级 dirty 位（对齐 ArkUI V2/SwiftUI Observable 终局，§2.7/§2.1 双证据）→ v2 编译期模板提纯（静态骨架一次构建，动态槽位 patch——Solid/Dioxus 模板化思路）。
5. **低功耗对齐**（用户裁决 ④）：保留模式事件驱动按需重绘 + 空闲零重绘（§4）；帧预算断言进 smoke（icount 通道）守住 vsync 教训（§4 安卓三根源）。

### 7.3 分阶段路线（与现行 GUI 刀序对齐，不推翻已裁决范围盒）

| 阶段 | 内容 | 增量面 |
|---|---|---|
| **v1 = GUI-3..8（现行刀序不动）** | berry-ui 保留树 API = 纯 Rust 构造（候选 A）；View 属性全 struct 化、事件全枚举化、**零闭包副作用**（回调只投递 Msg）；手势/动画/布局按规划 | 设计纪律而非代码量：API 形状即声明式可叠加的底座 |
| **v1.5 = GUI-6 后或生态线窗口** | berry-ui-decl 宏层（候选 B）：树字面量糖 + Model/Msg/update 三件套样板收窄 + 属性级 dirty 位；host 纯函数测同 fbcon 法 | 一个宏 crate + berry-ui 内部失效粒度细化，零服务面改动 |
| **v2 = 终态形态** | 编译期模板提纯 + JSON schema 动态 UI 面（候选 D，AI 在线生成/卡片/远程 UI）+ DFX：视图树 dump 扩「组件注册表枚举」+ 事件注入器对齐 Slint MCP 形态（agent 驱动 UI 验证） | 渲染后端（virtio-gpu/wgpu 面）另行调研报告接续 |

### 7.4 与用户六条裁决的对账

| 裁决 | 本报告回应 |
|---|---|
| ① 最先进方案不将就 | 细粒度失效 + 编译器 memo 是行业终局方向（§3），路线图 v1.5/v2 直达；不将就不等于跳步——底座（保留树+脏区）先行是 Solid/Svelte 也绕不开的物理层 |
| ② 移动设备为基准 | 帧预算/低功耗/手势优先全按移动主战场设计；手表 = 同族低功耗端自然受益（空闲零重绘） |
| ③ 吸取安卓教训 | §4 三根源转译四对策（帧断言/短路径/浅树/逻辑渲染分离缝） |
| ④ 又低功耗又优秀 | 保留模式按需重绘 + 属性级失效（无效渲染是 jank 与功耗的共同敌人——ArkUI V2 官方动机） |
| ⑤ API 兼容≠适配好 | 宏 DSL 是**原生**形态（rustc 执法、值语义、无 JS 引擎），不移植任何现成框架；「兼容」体现在形态选型向行业最大语料（JSX 族）靠拢而非运行时兼容层 |
| ⑥ 开发套件三全 | §5.3 三全论证 + §7.2 分层形态（生成/验证/运行时各取所长） |

---

## 8. 关键发现

1. **行业两线收敛于「编译器替人做 memo」**：React Compiler（表达式级自动 memo）与 Compose Strong Skipping（全 skippable + 不稳定参数自动记忆化）在 2024–2025 同步落地——新 UI 框架若依赖手动 memoization 达标性能即为代际落后。证据：[zestgeek/Medium](https://medium.com/zestgeek/react-compiler-the-end-of-manual-memoization-in-react-26c07341168e)、[patilshreyas/Medium](https://medium.com/@patilshreyas/stop-ignoring-compose-stability-yes-even-with-strong-skipping-mode-085bbd55fe98)。
2. **diff 非必需已被双实例证明**：Solid.js（组件只跑一次、信号直达绑定）与 Svelte 5（runes 编译为信号代码、运行时趋零）绕过整层组件级 diff。证据：[docs.solidjs.com](https://docs.solidjs.com/advanced-concepts/fine-grained-reactivity)、[r/sveltejs](https://www.reddit.com/r/sveltejs/comments/1dbvse0/compile_svelte_5_components_with_no_runtime)。
3. **失效粒度细化的代差在两家巨头同向发生**：SwiftUI ObservableObject→@Observable（属性访问级依赖）、ArkUI V1 对象级→V2 属性级+深度观测——OpenHarmony 用自家两代并存自证了 V1 粒度不够。证据：[Apple 迁移指南](https://developer.apple.com/documentation/swiftui/migrating-from-the-observable-object-protocol-to-the-observable-macro)、[华为 V1/V2 差异文档](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/arkts-v1-v2-update-difference)。
4. **Flutter 是七模型中分配压力与失效粒度最差一档**（setState 整子树配置重建 + 组件级失效），靠 Element diff 与应用层状态库补救。证据：[docs.flutter.dev](https://docs.flutter.dev/resources/inside-flutter)、[hashnode](https://ishaquehassan.hashnode.dev)。
5. **Android jank 三机制根源**：vsync 16ms 硬预算（超时帧翻倍 32ms）、深树全遍历、主线程重活——BerryOS 可用帧预算断言 + 输入→首像素短路径 + 浅树对策 day-one 化。证据：[Unity 论坛](https://discussions.unity.com/t/removing-touch-drag-lag-for-android/778730)、[vocal.media](https://vocal.media/journal/how-to-handle-slow-rendering-in-complex-android-ui-layouts)。
6. **即时模式（egui）微秒级重建但与低功耗结构性冲突**（空闲重绘 + 无稳定标识弱化自动化验证）；OpenHarmony ui_lite（900KB ROM/90KB RAM + 脏区）是移动/嵌入式保留模式的现成实证。证据：[HN](https://news.ycombinator.com/item?id=44395421)、本地 `docs/GUI里程碑规划-2026-10.md` 附录 B。
7. **AI UI 生成业界双轨**：v0/Figma Make 直产 JSX 代码（语料最丰），Builder.io/daily.dev 主张受约束组件词表 + 结构化 UI intent（受控生成、可校验）；「完美 JSON ≠ 语义正确」是结构化路线的公认残余风险。证据：[daily.dev](https://daily.dev/posts/a-better-approach-to-generative-ui-hwpqccf9w)、[Towards AI](https://pub.towardsai.net/structured-output-from-an-llm-can-be-perfect-json-and-completely-wrong-cbd630be2306)。
8. **Slint 的 AI 工具链（skill + 内嵌 MCP：无障碍树检查 + 事件驱动验证）是 GUI AI 验证的业界首例**，形态上与 BerryOS DFX 三件（视图树 dump + 事件注入器）直接同构，可对标；但 Slint GPL/商业双许可，BerryOS 只可看思路。证据：[docs.slint.dev](https://docs.slint.dev/latest/docs/slint/guide/tooling/ai-coding-assistants)。
9. **Rust 宏 DSL 的 IDE/热重载/增量编译是结构性已知税**（rust-analyzer proc-macro 补全缺口、Leptos 宏内补全弱、proc-macro 展开不进增量缓存）——选宏 DSL 必须同时设计「域窄 DSL + rustc 诊断闭环」的缓解策略。证据：[rust-analyzer#18834](https://github.com/rust-lang/rust-analyzer/issues/18834)、[leptos#1781](https://github.com/leptos-rs/leptos/issues/1781)。
10. **BerryOS 现行规划（GUI-3 保留树+脏区、纯 Rust 构建）与行业终局兼容**：保留树是 Solid/Svelte 之下共同的物理层；在其上叠宏 DSL + 属性级失效即可达「AI 生成/编译器验证/运行时高效」三全，无需推翻任何已裁决刀序。证据：本地 `docs/GUI里程碑规划-2026-10.md` §4、`user/berry-fbcon/src/`（渲染核种子）。

## 9. 对 BerryOS 的架构启示

1. **berry-ui API 从第一天按「数据驱动」设计**（GUI-3 落地时的设计纪律）：View 属性全 struct 化、事件全枚举化、回调只投递 Msg 不执行闭包副作用——这是声明式宏层（v1.5）能零重构叠加的唯一前提。
2. **采用 Elm 式 Model/Msg/update 纯函数内核**：view/update 做成纯函数并全部 host 离屏测试（与 `user/berry-fbcon` 28 测同法）——确定性可复现直接兑现「AI 测试友好化」支柱，rustc 穷尽性匹配执法状态机。
3. **失效粒度按两阶段走**：v1 视图级 Invalidate（现行规划）→ v1.5 属性级 dirty 位（键路径比较，不引入运行时信号图——规避 Leptos 式借用/订阅图冲突）；对齐 ArkUI V2/SwiftUI Observable 的终局粒度。
4. **SDK 表意层选 Rust 宏 DSL（rsx 风格窄域糖），不引入独立标记语言**：宏机械展开到既有构造 API，rustc 当全强度验证器；DSL 域刻意窄（无宏内语句层）控生成漂移与工具链税；JSX 近形换 LLM 语料红利。
5. **JSON schema 组件树留 v2 作 AI 动态 UI 缝**：与宏 DSL 共用同一组件注册表（AI 可生成集 ≡ 开发者可手写集），形态即 server-driven UI，兼得热重载与跨进程递送（卡片/远程 UI 同构）。
6. **帧预算 day-one 进断言**：每帧 Measure+Render 成本走 B-2 icount 通道定标上限进 smoke（吸取 Android vsync 教训）；输入事件→目标视图标脏的短路径作为 gui-inputd→berry-ui 契约的设计约束。
7. **即时模式只给 DFX/调试 overlay 小面**，常驻 UI 全走保留+脏区（空闲零重绘 = 低功耗身份支柱的 UI 面兑现）；fbcon 原语层升格 berry-gfx 后同时服务两模式。
8. **DFX 三件对标 Slint MCP 形态升级**：GUI-9 的事件注入器 + 视图树 dump 加上「无障碍/语义树 + agent 可编程驱动」，即为 BerryOS 版「AI 生成 → 运行时验证」闭环（Slint 业界首例证明可行）。
9. **协议洁净红线**：Slint（GPL/商业）、Flutter（BSD 但自绘栈庞大）、Compose/React（Apache-2.0/MIT 可对照）——形态与机制可学，代码与 DSL 语法设计零拷贝；自有 DSL 语法独立设计并留文档（AI 生成用 system prompt/skill 覆盖语料缺口，Slint 已验证此路径有效）。

## 10. 开放问题

1. **宏 DSL 与 rust-analyzer 的协作深度**：窄域 DSL + rustc 诊断起步是否够用？还是需要自写简易 LSP（Slint 路线的最小子集）？建议 v1.5 刀内用真实开发体验定标。
2. **属性级失效的最优 Rust 形态**：键路径 diff vs 编译期生成的 setter 位 vs 轻量信号——三者借用检查友好度与 TCG 下性能需原型刀对拍（与 icount 定标联动）。
3. **TCG 软渲染的帧预算现实值**：800×600 RGB32 全屏 ≈1.9MB/帧在 TCG 下的脏区策略最优切分（该数值已在 GUI 规划 §5 风险表），直接影响失效粒度的工程选择上限。
4. **JSON 动态 UI 与能力模型的边界**：AI 运行时生成/递送的 UI 是否受 .bap 能力声明约束（GUI 进程能否渲染未声明组件）——纯能力模型下需要新裁决。
5. **「先进渲染 API 兼容」（裁决⑤）的口径**：指 wgpu/Skia 类绘制 API 面还是更大的 WebGPU/图形栈兼容层，影响 berry-gfx trait 面积设计——建议单列报告（渲染后端篇）。
6. **Svelte 式编译期反应性在 Rust 的完全体**（编译器直接生成失效代码而非运行时 diff）是否值得自研——v2 模板提纯刀前需再评估投入产出。
