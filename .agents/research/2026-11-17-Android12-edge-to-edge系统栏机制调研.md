# Android 12+ edge-to-edge 与系统栏颜色跟随机制调研（BerryOS 改造参照蓝图）

Type: survey
Status: current
Updated: 2026-11-17

一句话说明：拆解 Android 12+ 的 edge-to-edge 布局、WindowInsets、系统栏颜色/对比度机制与 SystemUI/合成器层序实现，逐件映射到 BerryOS 现有与波3 已设计构件，给出 owner 2026-11-16 两项诉求（①导航键透明图标 ②应用 edge-to-edge + 系统栏跟随）的改造优先级。

## 问题

owner 2026-11-16 裁定两项核心诉求：其一，导航键当前是「深底圆角块 + 白符号」的不透明块（`user/berry-appshell/src/icons.rs:139` 资产自绘 + `user/bins/berry-launcher/src/desktop.rs:132,746` 按压面 `NAV_FACE_BG`），要改成白色符号直接浮在壁纸上、无任何底色块；其二，应用打开后底部没填满、导航栏恒显示桌面壁纸而不跟随应用内容。两项都需要以 Android 12 的 edge-to-edge + system bar 机制为参照确定目标形态。本文回答：Android 这套机制的完整分层拆解是什么、每一件在 BerryOS 对应哪个构件（现状与缺口）、改造按什么优先级排。

## 方法与来源

Android 侧证据两级：官方文档（edge-to-edge 指南与 `android.view.Window` 参考，2026-11-17 抓取，URL 见「证据与出处」）与 AOSP 源码直读（`android12-release` 分支为主、`main` 分支补 Android 15 弃用面；`android.googlesource.com` `?format=TEXT` 原文，行号为当日直数）。BerryOS 侧证据为主树工作树（HEAD `f52d834b4` 后续改动现场）`file:line` 直读：`user/berry-gfx/src/{raster,cmd,lib}.rs`、`user/bins/berry-launcher/src/{desktop,frameloop}.rs`、`user/berry-appshell/src/icons.rs`、`user/bins/gui-wmd/src/table.rs`、`user/berry-guiproto/src/lib.rs`，以及波3 设计稿与相关 plan 的现行版头块核验。

## 结论

### 一、Android 机制完整拆解

**1. 分层模型：系统栏根本不在应用进程里。** 系统栏是 SystemUI 进程持有的独立 Window：状态栏 `TYPE_STATUS_BAR = 2000`（= `FIRST_SYSTEM_WINDOW`），导航栏 `TYPE_NAVIGATION_BAR = 2019`，手势柄面板 `TYPE_NAVIGATION_BAR_PANEL = 2024`，壁纸 `TYPE_WALLPAPER = 2013`；应用窗类型域 `1..=99`（`TYPE_APPLICATION = 2`）。WindowManagerService 给每个窗算基线 Z 层：`mBaseLayer = policy 层 × TYPE_LAYER_MULTIPLIER(10000) + TYPE_LAYER_OFFSET(1000)`（`WindowState.java:1105-1118`、`WindowManagerService.java:343-347`）——系统窗类型值结构性高于应用窗。因此「应用 SUBMIT 帧时系统栏怎么保持在上层」在 Android 里是一个不存在的问题：应用帧与系统栏是两个进程各自提交到 SurfaceFlinger 的两个独立 layer，Z 序是合成期属性，帧提交只更新各自 layer 的 buffer，互相不可触碰（Android 12 应用侧经 BLASTBufferQueue 直提 SurfaceFlinger，也不经过 SystemUI）。

**2. 内容怎么「钻到」栏后面：不是应用盖住系统栏，而是应用窗本身全屏铺开。** `setDecorFitsSystemWindows(false)`（`WindowCompat.setDecorFitsSystemWindows` 兼容封装；`WindowCompat.enableEdgeToEdge()` 一站式）让 DecorView 不再按系统栏内缩，WindowManager 给应用的 window frame 直接是全屏 parent frame——应用内容画到屏幕物理边缘，系统栏 surface 以更高 Z 叠在上面。配套三件事：避让（insets 分发）、图标对比度（appearance）、背景策略（谁在栏区铺底色）。

**3. WindowInsets：避让量的分发协议。** insets 按类型查询：`WindowInsetsCompat.Type.systemBars()`（状态/导航/caption 栏，最常用——可点控件避让）、`displayCutout()`（异形屏，常与 systemBars 取逻辑或）、`systemGestures()`（系统手势优先区）、`tappableElement()`、`ime()`。消费模式是监听器：`ViewCompat.setOnApplyWindowInsetsListener(view) { v, insets -> val bars = insets.getInsets(Type.systemBars() or Type.displayCutout()); v.updatePadding/updateMargins(...); WindowInsetsCompat.CONSUMED }`——四边各自独立（左右导航栏、横屏 cutout 各自给值）。典型布局策略：滚动内容在栏后滚动（RecyclerView `clipToPadding=false` + padding）；FAB/底栏等可点件用 margin 让出；AppBar 顶到屏幕边、在状态栏后显示。沉浸模式 = `WindowInsetsController.hide(Type.systemBars())`。这套的分寸感是关键：**栏后的内容继续渲染（内容贯通），只有可点/可读不可遮的件让位**——这正是 owner 说的「应用开发会为导航栏留出距离」的标准形态。

**4. 系统栏颜色跟随的三个时代。**
- **时代 A（API 21-29，应用主导）**：`Window.setStatusBarColor(int)` / `setNavigationBarColor(int)` / 主题属性 `android:statusBarColor`——应用把颜色写进自家 LayoutParams，WMS 转交 SystemUI，`NavigationBarTransitions`/`BarTransitions` 状态机（OPAQUE/TRANSLUCENT/…）把导航栏窗背景设为该色；浅色栏配 `APPEARANCE_LIGHT_NAVIGATION_BARS` 深色图标（暗色栏配浅色图标）。
- **时代 B（Android 12，应用主导 + 系统兜底对比度）**：透明请求之上系统强制可读性。`Window.setNavigationBarContrastEnforced` / 主题属性 `android:enforceNavigationBarContrast`（API 29 起有，Android 12 起默认开）：仅当应用请求**全透明**导航栏时生效，系统给三键导航加一层半透明 scrim 保证键可辨（官方语义原文：仅当导航栏颜色 alpha 为零时该属性才起作用）。图标侧 SystemUI 双保险：`KeyButtonDrawable` 生来支持「双色 tint + 内建 shadow」（类注释原文：supports tinting between two colors, rotation and shows a shadow；`BlurMaskFilter` 软阴影），运行期可在明/暗图标色间插值；`LightBarController` 依栏后 scrim alpha（阈值 `NAV_BAR_INVERSION_SCRIM_ALPHA_THRESHOLD = 0.1f`）与内容明暗自动翻转图标明暗，另有 `RegionSamplingHelper` 对栏后内容做亮度采样。**这就是「白色符号浮在壁纸上」的 Android 原生实现：glyph 是透明底 drawable，栏窗背景透明，glyph 独立 tint，对比度靠 scrim/阴影/亮度采样三件兜底，而不是给 glyph 垫一块底色。**
- **时代 C（Android 15，targetSdk 35，系统主导，edge-to-edge 强制）**：`PRIVATE_FLAG_EDGE_TO_EDGE_ENFORCED` 强制全屏铺设；`setStatusBarColor` / `setNavigationBarColor` / `setNavigationBarDividerColor` / `setStatusBarContrastEnforced` 全部 deprecated，官方处置原文：Draw proper background behind `WindowInsets.Type.navigationBars()` or `tappableElement()` instead——**「导航栏颜色跟随应用」的正解从「应用告诉系统栏画什么色」翻转为「系统栏恒透明，应用在自己的 insets 区自己铺底」**。默认形态：状态栏与手势导航透明、三键导航半透明 scrim（`setNavigationBarContrastEnforced(false)` 可关）；新 API `ProtectionLayout`/`GradientProtection` 让应用画渐变保护。
- 演进逻辑对 BerryOS 的总启示：终点形态 = 系统栏窗口透明 + 图标双色 tint + 应用内容经 insets 自己铺底；**不要照抄时代 A 的「应用→系统栏上色」链路，那是 Google 自己拆掉的过渡态**。

**5. SystemUI 实现细节（android12-release 现场直读）。** 状态栏窗：`TYPE_STATUS_BAR`，flags `FLAG_NOT_FOCUSABLE | FLAG_SPLIT_TOUCH | FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS`，`layoutInDisplayCutoutMode = ALWAYS`（`StatusBarWindowController.java:124-135`）。导航栏窗：`TYPE_NAVIGATION_BAR`，flags `FLAG_TOUCHABLE_WHEN_WAKING | FLAG_NOT_FOCUSABLE | FLAG_NOT_TOUCH_MODAL | FLAG_WATCH_OUTSIDE_TOUCH | FLAG_SPLIT_TOUCH | FLAG_SLIPPERY`，cutout `ALWAYS`（`NavigationBar.java:525-536`）；手势柄独立窗 `TYPE_NAVIGATION_BAR_PANEL`（同文件 :758-766）。键图标：`KeyButtonDrawable.create(mLightContext, mLightIconColor, mDarkIconColor, iconRes, true /* hasShadow */, null)`（`NavigationBarView.java:622-625`）。注意状态栏窗自带 `FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS`——系统栏自绘背景，应用侧设了该 flag 时系统不替应用画色。

**6. 合成器层面。** SurfaceFlinger 每个 Window 一个 layer（SurfaceControl），Z 由 WMS 赋值后随事务下发；系统栏与应用各自独立渲染提交。BerryOS 单 fb CPU blit 模型里「应用 SUBMIT 后系统栏仍在上面」要靠**重放**达成（波3 带合成 v1），Android 里靠**合成期 Z**达成——语义等价，见映射表。

### 二、BerryOS 映射表

| Android 构件 | 机制一句话 | BerryOS 对应 | BerryOS 现状（file:line） | 缺口与改造 |
|---|---|---|---|---|
| 系统栏独立窗（TYPE_STATUS_BAR=2000 / TYPE_NAVIGATION_BAR=2019） | 系统栏住常驻系统进程，结构性高于应用 | wmd `WM_LAYER_SYSTEM=1` 窗，launcher 双窗化（状态栏+三键导航条） | `user/berry-guiproto/src/lib.rs:170`；wmd SYSTEM 层从未建窗（[波3 三栏侦察](2026-11-15-波3三栏侦察.md) §二）；launcher 单窗内容层 | [波3 设计](../plans/2026-11-15-波3桌面三栏与系统窗实施设计.md) 批C 落地双窗；launcher 常驻即 BerryOS 的 SystemUI 宿主 |
| WMS 层序（×10000+1000，类型决定 Z） | 帧提交无关的合成期 Z 序 | guifbd 带合成 v1 重放序 SERVICE>SYSTEM>APP | blit-at-SUBMIT 现状：应用全屏 SUBMIT 即盖一切（`user/bins/guifbd/src/guest.rs` submit 面） | 波3 批B；重放序即 BerryOS 的 Z 序，语义与 Android per-layer Z 等价 |
| SurfaceFlinger per-layer buffer | 各窗独立提交、合成期合成 | present SUBMIT（CPU blit 直写 fb）+ 带缓存 | `guifbd` guest.rs SUBMIT 臂 | v1 等价已设计；真 per-layer（vgpu 多资源）归显示加速 B 系后续刀，本改造不依赖 |
| `setDecorFitsSystemWindows(false)` | 应用窗全屏铺开、内容到栏后 | 应用窗全屏不避让 | wmd 现避让 = AVOID_SYS 裁几何（`user/bins/gui-wmd/src/table.rs:263` recompute、:75 clip_rect） | 应用窗默认全屏（Android 15 形态），避让责任移到应用侧；F_IMMERSIVE 旗（六波方案波3 条）从「示范位」升为常态语义 |
| `WindowInsets.Type.systemBars()` 四边独立分发 | 避让量交给应用自处理 | WM_GETINFO 几何 + wmd 表快照 | WM_GETINFO 三域全屏几何（`lib.rs:106`）；无 inset 域 | WM_CREATE 应答/TABLE_GET 快照增四边 system bar inset 域（add-only）；壁纸/手势区未来可加域 |
| `OnApplyWindowInsetsListener` | 应用消费 inset 自布局 | berry-ui/appshell 布局面消费 | launcher 桌面几何动态化先例（WM_GETINFO 注入 viewport） | appshell 提供 insets 注入口；AppBar/BottomBar 等 pattern 件吃底 inset padding；滚动内容贯通不 pad |
| `setNavigationBarColor`（时代 A/B，35 已废） | 应用告知导航栏底色 | 无对应 | — | **不建此协议**（Google 已废）；「跟随应用颜色」由透明栏 + 应用内容透出天然达成（时代 C） |
| `enforceNavigationBarContrast` | 全透明栏时系统强制半透明 scrim | 无 | — | 波3 带合成后：三键态默认轻 scrim 双档（对齐 Android 12 默认 true）；owner 若要全裸透明白键则关 scrim、靠键影 |
| `APPEARANCE_LIGHT_NAVIGATION_BARS` + `KeyButtonDrawable` 双色 tint/阴影 | 图标明暗随内容翻转、透明底 glyph | Icon fg tint；NAV 资产自带深底块 | `nav_face(glyph, bg)` 66×66 圆角块底+白符号（`user/berry-appshell/src/icons.rs:428-454`）；按压面 `NAV_FACE_BG`（`desktop.rs:132,746`） | 资产改 glyph-only：块底像素全 0（A=0 透明键）、仅 glyph 白；按压反馈改 glyph 缩放/明暗而非底块换色；可选 1px 深描边等价 Android 键影 |
| `RegionSamplingHelper` 亮度采样 | SystemUI 采栏后内容保对比度 | 无 | — | 远期：guifbd 带合成处已持有 APP 帧，可低成本采样底带亮度 → 决定 scrim 档/图标明暗（launcher `set_props` 换面 id 先例：power armed） |
| 壁纸 TYPE_WALLPAPER=2013（应用之下） | 壁纸独立层，被应用盖 | 壁纸 = launcher APP 层窗内容（Icon id 0xFFFE rodata 直供） | `icons.rs` WALLPAPER 面 | 现状语义已对齐（应用不透明即盖壁纸）；无需改 |
| Android 15 强制 edge-to-edge | 系统主导透明栏+应用铺底 | — | — | BerryOS 目标形态直接取此终点，跳过时代 A 过渡协议 |

### 三、两项 owner 诉求的共同技术根因

两条诉求最终收敛到**同一个原语：1-bit alpha 透明键在两层生效**。第一层（诉求①）：raster 已经支持——`argb_blit` 明文「A==0 跳过不写（区外画面穿透）、A!=0 一律不透明覆盖，这不是 blend」（`user/berry-gfx/src/raster.rs:415-425` 起；v1 无 blend 铁律见 `cmd.rs:61`），导航面圆角外的透明今天就工作。第二层（诉求②）：波3 带缓存重放若按「带像素原样回拷」实现，透明栏的带重放会把 0 像素写回 fb、把应用内容涂黑——**带缓存重放必须采用 argb_blit 同律（缓存带 A==0 的像素跳过不写）**，这样重放只在 glyph/实际栏内容处落笔，应用内容在栏区透出＝导航栏「跟随应用颜色」，且不破 v1 无 blend 铁律（1-bit 键不是合成）。这是本调研对波3 批B 设计的唯一实质增补。

[LAN 桌面沉浸化三件套](../plans/2026-11-16-桌面沉浸化三件套.md)「为什么不是」节记有「客机渲染管线不支持 alpha=0 透明像素（实测图标整体消失）」——与 raster 现状（圆角透明在工作）矛盾，该实验现象需在 P0 开工时复测定位（疑点：当时改的是按压面 Rect 的 bg 而非资产生成、或资产把 glyph 也写成 0）；glyph-only 面在现 raster 上应当直接可渲。

### 四、改造优先级建议

**P0（解诉求①，零协议改动，独立可交付）**
1. 导航键 glyph-only 化：`nav_face` 调用面把块底参数换透明（块位像素 0、glyph 白 `NAV_GLYPH_FG` 不动），撤 `desktop.rs` 按压面 `NAV_FACE_BG` 底块；按压反馈改 glyph 缩放或明暗。前置核查：复测「alpha=0 图标整体消失」现象并定位（见上节疑点）；状态栏三图标（signal/wifi/battery）与 dock 面同族顺带评估。
2. 壁纸亮壁纸下白键可辨性：v1 用 glyph 1px 深描边（Android 键影等价）；不引 scrim。

**P1（解诉求②主体，踩在波3 批B/批C 上）**
3. guifbd 带合成 v1 落地，**带缓存重放按 1-bit alpha 键跳写实现**（本调研第三节增补）——这是「应用开后系统栏仍在上面且栏区透出应用色」的机器前提。
4. launcher 双窗化（波3 批C）：状态栏+三键导航条迁 SYSTEM 层窗，壁纸留 APP 层窗。
5. 应用窗全屏化 + inset 分发：WM_CREATE/TABLE_GET 增四边 inset 域（add-only）；appshell/appdemo 消费——底部滚动内容贯通到屏边、可点件让位（Android listener 模式等价）。F_IMMERSIVE 语义随之定为「隐藏 SYSTEM 栏」而非「避让豁免」。

**P2（对齐 Android 12 对比度兜底）**
6. 三键态轻 scrim 双档开关（对齐 `enforceNavigationBarContrast` 默认 true 的兜底哲学），launcher 常量或设置页项。
7. 图标明暗跟随内容：guifbd 采样底带亮度 → launcher 换 tint 面（时代 B 的 `APPEARANCE` 等价，远期项）。

**P3（时代 C 完全体与远期）**
8. 应用铺底范式立法进 `docs/应用规范`：应用在 insets 区画自己的底色、系统栏恒透明——Android 15 官方处置的 BerryOS 化。
9. per-layer 真合成与 RegionSampling 等价件归 B 系与后续波次，本改造不阻塞。

**明确不抄的部分**：时代 A 的 `WM_SET_NAV_COLOR` 类「应用→系统栏上色」op（Google 已弃，避免造了再拆）；predictive back 与 desktop windowing caption bar（BerryOS 无对应场景）；display cutout 协议面（QEMU 无 cutout，insets 域留四边即可覆盖未来）。

## 证据与出处

Android 官方文档（2026-11-17 抓取）：edge-to-edge 指南 https://developer.android.com/develop/ui/views/layout/edge-to-edge （enableEdgeToEdge 默认形态、三键导航 scrim 与 setNavigationBarContrastEnforced(false)、insets 监听器模式、Android 15 强制声明）；`android.view.Window` 参考 https://developer.android.com/reference/android/view/Window （setStatusBarColor/setNavigationBarColor/setStatusBarContrastEnforced API 35 deprecated 及处置原文）。
AOSP 源码（`android12-release`，`android.googlesource.com`，行号 2026-11-17 直数）：`core/java/android/view/WindowManager.java`（FIRST_APPLICATION_WINDOW=1 :2174、FIRST_SYSTEM_WINDOW=2000 :2272、TYPE_STATUS_BAR :2280、TYPE_INPUT_METHOD :2371、TYPE_WALLPAPER :2385、TYPE_NAVIGATION_BAR :2439、TYPE_NAVIGATION_BAR_PANEL :2469）；`core/java/android/view/Window.java:2559-2578`（setNavigationBarContrastEnforced 与 enforceNavigationBarContrast 主题属性、alpha 非零时无效语义）；`services/core/java/com/android/server/wm/WindowState.java:1105-1118` 与 `WindowManagerService.java:343-347`（层序公式与常量）；SystemUI `NavigationBar.java:525-536,758-766`（导航栏窗与手势柄窗）、`StatusBarWindowController.java:124-135`（状态栏窗）、`NavigationBarView.java:622-625`（KeyButtonDrawable 创建）、`buttons/KeyButtonDrawable.java`（双色 tint+shadow 类注）、`phone/LightBarController.java:52,175-185`（scrim 阈值与明暗翻转）。AOSP `main`（补 Android 15 面）：`WindowManager.java:3403`（PRIVATE_FLAG_EDGE_TO_EDGE_ENFORCED）、`Window.java:2693-2738,2793-2812`（deprecated 面与空实现）。
BerryOS 仓内（2026-11-17 工作树直读）：`user/berry-gfx/src/raster.rs:378-425`（draw_image 分派与 argb_blit 1-bit 透明键）、`user/berry-gfx/src/cmd.rs:61-62`（v1 无 alpha 铁律）、`user/berry-appshell/src/icons.rs:134-163,428-454`（导航面资产生成与 NAV id 域）、`user/bins/berry-launcher/src/desktop.rs:132,746`（按压面深底块）、`user/bins/berry-launcher/src/frameloop.rs:185`（surface_id=0xFFFF 系统面豁免）、`user/bins/gui-wmd/src/table.rs:75,263`（AVOID_SYS 避让）、`user/berry-guiproto/src/lib.rs:106-172`（wm 域 op 与 layer/旗位）。规划文档：[波3 桌面三栏与系统窗实施设计](../plans/2026-11-15-波3桌面三栏与系统窗实施设计.md)、[体验大改造六波实施方案](../plans/2026-11-15-体验大改造六波实施方案.md)、[波3 三栏侦察](2026-11-15-波3三栏侦察.md)、[UI 现状十四维盘点](2026-11-15-UI现状十四维盘点.md)、[LAN 桌面沉浸化三件套](../plans/2026-11-16-桌面沉浸化三件套.md)。

## 适用边界

Android 事实取自 android12-release 与 main 分支 2026-11-17 快照，Android 13/14 的逐版差异未单独核对（对本结论无影响——时代 B/C 分界与层序自 12 起稳定）；SystemUI 带缓存/导航栏窗行号随上游演进会漂移，引用以当日快照为准。BerryOS 行号以 2026-11-17 主树工作树为准，波3 批B/批C 合入后现状行号漂移，以「机制描述 + 件名」为准追新。「alpha=0 图标整体消失」的历史实验根因未定位，属 P0 前置核查项，本文只给疑点不给结论。本文不构成波3 设计稿的修订——带缓存重放 1-bit 键跳写的增补建议归波3 批B 开工时裁决入稿。
