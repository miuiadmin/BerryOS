# GUI-11 设计系统硬数值 token 表-v1.0

> 二轮深集·六号产物。纯资料档：只录事实/数值/来源，不做分析不建议。
> 来源等级：**A** = 官方站/官方仓直接抓取核验；**B** = 官方 SPA 站无法直抓，经 DuckDuckGo 摘录/官方仓代码镜像核验；**C** = 第三方转述（均已标注）。
> 采集时间 2026-11。m3.material.io 与 developer.huawei.com 文档中心均为 SPA，直抓返回壳，全部改走官方代码仓/JSON 端点。

## 1. Material 3

### 1.1 色彩角色全表（名称+用途）
m3.material.io SPA 不可直抓（等级B）。角色清单按官方 token 仓 `material-web/tokens/versions/v0_192/_md-sys-color.scss`（等级A，文件含 light/dark 双份全部角色名）。
URL: https://github.com/material-components/material-web/blob/main/tokens/versions/v0_192/_md-sys-color.scss

核心角色族（每族通常含 on- 及 -container/on-container 共 4~5 个）：

| 角色族 | 覆盖角色 | 用途（官方语义） |
|---|---|---|
| primary | primary, onPrimary, primaryContainer, onPrimaryContainer, inversePrimary | 品牌主色/其上内容/容器级 |
| secondary | secondary, onSecondary, secondaryContainer, onSecondaryContainer | 次要强调 |
| tertiary | tertiary, onTertiary, tertiaryContainer, onTertiaryVariant | 对比强调/品牌第三色 |
| error | error, onError, errorContainer, onErrorContainer | 错误 |
| background(旧) | background, onBackground | 已并入 surface 族（deprecated） |
| surface | surface, onSurface, surfaceVariant, onSurfaceVariant, surfaceTint, surfaceDim, surfaceBright, surfaceContainerLowest/Low/High/Highest（surfaceContainer 为基准） | 表面层级 5 档 + 色调投射 |
| outline | outline, outlineVariant | 描边/分隔 |
| inverse | inverseSurface, inverseOnSurface | 反色表面（Snackbar 等） |
| other | scrim, shadow | 遮罩/阴影色 |
| fixed(扩展) | primaryFixed, primaryFixedDim, onPrimaryFixed, onPrimaryFixedVariant（secondary/tertiary 同构 12 个） | 亮暗固定色（大屏/嵌入式） |

计数：基础 34 + surface 容器细分 5 + fixed 族 12 ≈ 51 个角色名（用户口径"45 个"为 m3 站点旧版列表）。**具体 hex 随种子色动态生成，官方不发布固定值表**；基准紫种子对应值未核实（需 Material Theme Builder 生成，等级B）。

### 1.2 字阶全表（15 档，dp/sp 同源；web 版 rem=px/16）
来源：`tokens/versions/v0_192/_md-sys-typescale.scss`（等级A）。
URL: https://github.com/material-components/material-web/blob/main/tokens/versions/v0_192/_md-sys-typescale.scss
字重值来源 `_md-ref-typeface.scss`：weight-regular=400, weight-medium=500, weight-bold=700（等级A）。

| 档位 | 尺寸 | 行高 | 字重 | 字距 tracking | 字体 |
|---|---|---|---|---|---|
| display-large | 57 (3.5625rem) | 64 | 400 | -0.25 (-0.015625rem) | Roboto brand |
| display-medium | 45 (2.8125rem) | 52 | 400 | 0 (0rem) | brand |
| display-small | 36 (2.25rem) | 44 | 400 | 0 | brand |
| headline-large | 32 (2rem) | 40 | 400 | 0 | brand |
| headline-medium | 28 (1.75rem) | 36 | 400 | 0 | brand |
| headline-small | 24 (1.5rem) | 32 | 400 | 0 | brand |
| title-large | 22 (1.375rem) | 28 | 400 | 0 | brand |
| title-medium | 16 (1rem) | 24 | 500 | 0.15 (0.009375rem) | plain |
| title-small | 14 (0.875rem) | 20 | 500 | 0.1 (0.00625rem) | plain |
| body-large | 16 | 24 | 400 | 0.5 (0.03125rem) | plain |
| body-medium | 14 | 20 | 400 | 0.25 (0.015625rem) | plain |
| body-small | 12 (0.75rem) | 16 | 400 | 0.4 (0.025rem) | plain |
| label-large | 14 | 20 | 500 | 0.1 | plain |
| label-medium | 12 | 16 | 500 | 0.5 | plain |
| label-small | 11 (0.6875rem) | 16 | 500 | 0.5 | plain |

注：label-large/medium 另有 weight-prominent=700 变体；tracking 为 px。共 15 档（"30 档"=15 样式×字号/行高口径）。

### 1.3 间距体系
**M3 无全局间距 token 阶梯**（token 库只有 shape/color/motion/elevation/typescale/state）。官方口径：4dp 基线网格（4dp baseline grid），组件间距散落在各 comp token 中。来源：m3.material.io/foundations/layout（SPA，等级B，未能直抓原文数值表）；token 目录清单（等级A）佐证无 spacing 域。M2 的 8dp 网格/16dp 边距等数值未核实。

### 1.4 形状角标全表
来源：`tokens/versions/v0_192/_md-sys-shape.scss`（等级A）。
URL: https://github.com/material-components/material-web/blob/main/tokens/versions/v0_192/_md-sys-shape.scss

| token | 值 |
|---|---|
| corner-none | 0px |
| corner-extra-small | 4px |
| corner-small | 8px |
| corner-medium | 12px |
| corner-large | 16px |
| corner-extra-large | 28px |
| corner-full | 9999px |
| corner-extra-small-top | 4px 4px 0 0 |
| corner-extra-large-top | 28px 28px 0 0 |
| corner-large-top | 16px 16px 0 0 |
| corner-large-start | 16px 0 0 16px |
| corner-large-end | 0 16px 16px 0 |
海拔：level0=0, level1=1, level2=3, level3=6, level4=8, level5=12（`_md-sys-elevation.scss`，等级A）。

### 1.5 动效 token
来源：`tokens/versions/v0_192/_md-sys-motion.scss`（等级A）。
URL: https://github.com/material-components/material-web/blob/main/tokens/versions/v0_192/_md-sys-motion.scss

| duration token | ms | | easing token | cubic-bezier |
|---|---|---|---|---|
| duration-short1 | 50 | | easing-linear | (0,0,1,1) |
| duration-short2 | 100 | | easing-standard | (0.2,0,0,1) |
| duration-short3 | 150 | | easing-standard-accelerate | (0.3,0,1,1) |
| duration-short4 | 200 | | easing-standard-decelerate | (0,0,0,1) |
| duration-medium1 | 250 | | easing-emphasized | (0.2,0,0,1) |
| duration-medium2 | 300 | | easing-emphasized-accelerate | (0.3,0,0.8,0.15) |
| duration-medium3 | 350 | | easing-emphasized-decelerate | (0.05,0.7,0.1,1) |
| duration-medium4 | 400 | | easing-legacy | (0.4,0,0.2,1) |
| duration-long1 | 450 | | easing-legacy-accelerate | (0.4,0,1,1) |
| duration-long2 | 500 | | easing-legacy-decelerate | (0,0,0.2,1) |
| duration-long3 | 550 | |  |  |
| duration-long4 | 600 | |  |  |
| duration-extra-long1 | 700 | |  |  |
| duration-extra-long2 | 800 | |  |  |
| duration-extra-long3 | 900 | |  |  |
| duration-extra-long4 | 1000 | |  |  |

## 2. HIG（iOS）

### 2.1 色彩 system colors 全表（RGB 深浅双值）
来源：HIG color 页 JSON 端点（等级A）。
URL: https://developer.apple.com/tutorials/data/design/human-interface-guidelines/color.json （页面 https://developer.apple.com/design/human-interface-guidelines/color）
官方声明："避免硬编码 system color 值，数值仅供设计参考，随版本浮动"。

常规色（light / dark）：

| 色名 | light RGB | dark RGB |
|---|---|---|
| systemRed | 255,56,60 | 255,66,69 |
| systemOrange | 255,141,40 | 255,146,48 |
| systemYellow | 255,204,0 | 255,214,0 |
| systemGreen | 52,199,89 | 48,209,88 |
| systemMint | 0,200,179 | 0,218,195 |
| systemTeal | 0,195,208 | 0,210,224 |
| systemCyan | 0,192,232 | 60,211,254 |
| systemBlue | 0,136,255 | 0,145,255 |
| systemIndigo | 97,85,245 | 109,124,255 |
| systemPurple | 203,48,224 | 219,52,242 |
| systemPink | 255,45,85 | 255,55,95 |
| systemBrown | 172,127,94 | 183,138,102 |

无障碍增强变体（Accessible，light / dark）：

| 色名 | light RGB | dark RGB |
|---|---|---|
| accessibleRed | 233,21,45 | 255,97,101 |
| accessibleOrange | 197,83,0 | 255,160,86 |
| accessibleYellow | 161,106,0 | 254,223,67 |
| accessibleGreen | 0,137,50 | 74,217,104 |
| accessibleMint | 0,133,117 | 84,223,203 |
| accessibleTeal | 0,129,152 | 59,221,236 |
| accessibleCyan | 0,126,174 | 109,217,255 |
| accessibleBlue | 30,110,244 | 92,184,255 |
| accessibleIndigo | 86,74,222 | 167,170,255 |
| accessiblePurple | 176,47,194 | 234,141,255 |
| accessiblePink | 231,18,77 | 255,138,196 |
| accessibleBrown | 149,109,81 | 219,166,121 |

语义灰阶（systemBackground/label/separator 等）：名称清单已核（systemBackground, secondary/tertiarySystemBackground, systemGroupedBackground×3, label/secondaryLabel/tertiaryLabel/quaternaryLabel, placeholderText, separator, opaqueSeparator, link, systemGray~systemGray6 等，见 JSON 用途表）；**具体深浅 hex 官方已不在 HIG 发布数值表（"Use APIs"）→ 未核实**。

### 2.2 Dynamic Type 完整表（iOS/iPadOS 12 档，pt，字重/行高/强调字重）
来源：HIG typography 页 JSON（等级A）。
URL: https://developer.apple.com/tutorials/data/design/human-interface-guidelines/typography.json
样式序列 = 样式 | 字重 | 字号 | 行高 | 强调(emphasized)字重。

| 档 | LargeTitle | Title1 | Title2 | Title3 | Headline | Body | Callout | Subhead | Footnote | Cap1 | Cap2 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| xSmall | 31/38 | 25/31 | 19/24 | 17/22 | 14/19 | 14/19 | 13/18 | 12/16 | 12/16 | 11/13 | 11/13 |
| Small | 32/39 | 26/32 | 20/25 | 18/23 | 15/20 | 15/20 | 14/19 | 13/18 | 12/16 | 11/13 | 11/13 |
| Medium | 33/40 | 27/33 | 21/26 | 19/24 | 16/21 | 16/21 | 15/20 | 14/19 | 12/16 | 11/13 | 11/13 |
| **Large(默认)** | **34/41** | **28/34** | **22/28** | **20/25** | **17/22** | **17/22** | **16/21** | **15/20** | **13/18** | **12/16** | **11/13** |
| xLarge | 36/43 | 30/37 | 24/30 | 22/28 | 19/24 | 19/24 | 18/23 | 17/22 | 15/20 | 14/19 | 13/18 |
| xxLarge | 38/46 | 32/39 | 26/32 | 24/30 | 21/26 | 21/26 | 20/25 | 19/24 | 17/22 | 16/21 | 15/18 |
| xxxLarge | 44/52 | 38/46 | 34/41 | 31/38 | 28/34 | 28/34 | 26/32 | 25/31 | 23/29 | 22/28 | 20/24* |
| AX1 | 48/57 | 43/51 | 39/47 | 37/44 | 33/40 | 33/40 | 32/39 | 30/37 | 27/33 | 26/32 | 24/30 |
| AX2 | 52/61 | 48/57 | 44/52 | 43/51 | 40/48 | 40/48 | 38/46 | 36/43 | 33/40 | 32/39 | 29/35 |
| AX3 | 56/66 | 53/62 | 50/59 | 49/58 | 47/56 | 47/56 | 44/52 | 42/50 | 38/46 | 37/44 | 34/41 |
| AX4 | 56/66 | 53/62 | 50/59 | 49/58 | 47/56 | 47/56 | 44/52 | 42/50 | 38/46 | 37/44 | 34/41 |
| AX5 | 60/70 | 58/68 | 56/66 | 55/65 | 53/62 | 53/62 | 51/60 | 49/58 | 44/52 | 43/51 | 40/48 |

*xxLarge/xxxLarge 的 Cap2 行高为截断推断。字重：LargeTitle/T1/T2=Regular(强调 Bold)；T3=Regular(强调 Semibold)；Headline=Semibold；其余 Regular(强调 Semibold)。各平台默认/最小字号：iOS 17/11pt；macOS 13/10pt；tvOS 29/23pt；visionOS 17/12pt；watchOS 16/12pt（同页表格，等级A）。

### 2.3 SF Symbols 规格
来源：sf-symbols.json（等级A）。URL: https://developer.apple.com/tutorials/data/design/human-interface-guidelines/sf-symbols.json
- 字重 9 档：Ultralight→Black（对应 SF 字体同名字重）。
- 尺寸 3 档：small / medium(默认) / large，**相对 SF 字体大写高（cap height）定义，无绝对 pt 数值表**。
- 渲染模式：Monochrome / Hierarchical / Palette / Multicolor + SF7 起 Gradient；变宽色（variable color）0~100%。
- 动效集：Appear/Disappear/Bounce/Scale/Pulse/Variable color/Replace(Down-up,Up-up,Off-up)/Magic Replace/Wiggle/Breathe/Rotate/Draw On-Off。具体每效 ms 数值官方未发布 → 未核实。

### 2.4 布局度量
- 最小点击区 44×44pt：HIG 多页口径（等级B，44pt 数字见 HIG Buttons/Gestures 相关页与 UIKit hit-test 惯例；本轮未直抓到含 44 的原文页）。
- 网格/边距建议页（Layout）为 SPA 数值未直抓 → 未核实。
- Materials（等级A，materials.json）：
  - Liquid Glass：regular（模糊+亮度调整，系统组件默认）/ clear（高透明，媒体上用；亮背景建议加 **35% 不透明度暗化层**——官方数值）。
  - 标准材质 4 级：ultraThin / thin / regular(默认) / thick（SwiftUI Material API 对应 .ultraThin/.thin/.regular/.thick）。
  - Vibrancy 层级：label(默认)/secondaryLabel/tertiaryLabel/quaternaryLabel（quaternary 禁用于 thin/ultraThin 上）；fill/secondaryFill/tertiaryFill；separator 单级。
  URL: https://developer.apple.com/tutorials/data/design/human-interface-guidelines/materials.json
- 动效时长：**Apple 不发布公开 ms 建议表**（HIG Motion 页无数值档）→ 未核实（社区常引 0.2~0.35s 非官方）。

## 3. HarmonyOS Design / OpenHarmony ArkUI

### 3.1 色彩
来源：openharmony/global_system_resources 仓 color.json（等级A，412 条 token）。
URL: https://gitee.com/openharmony/global_system_resources/blob/master/systemres/main/resources/base/element/color.json （暗色在 color_dark.json）

核心语义 token（亮色值；#RRGGBBAA）：

| token | 值 | 用途 |
|---|---|---|
| ohos_id_color_foreground | #ff182431 | 前景/主文本 |
| ohos_id_color_background | #ffffffff | 页面底 |
| ohos_id_color_sub_background | #fff1f3f5 | 次级底 |
| ohos_id_color_emphasize / activated | #ff007dff | 强调/选中（品牌蓝） |
| ohos_id_color_text_primary | #ff182431 | 一级文本 |
| ohos_id_color_text_secondary | #99182431 | 二级（60% α） |
| ohos_id_color_text_tertiary | #66182431 | 三级（40% α） |
| ohos_id_color_text_hint | #99182431 | 提示 |
| ohos_id_color_list_separator | #33182431 | 分隔线（20% α） |
| ohos_id_color_warning | #fffa2a2d | 警示 |
| ohos_id_color_alert | #ffff7500 | 提醒 |
| ohos_id_color_handup | #ffe84826 | 挂断/严重 |
| ohos_id_color_connected | #ff00cb87 | 已连接/成功 |
| black / white | #182431 / #ffffff | 基础 |
| gray_01~04 | #f1f3f5 / #e5e5ea / #d1d1d6 / #c7c7cc | 灰阶 |
| palette1~11 | #ff3c56ea #ff00aaee #ff00bfc9 #ff47cc47 #ff8cd600 #ff8a2be2 #ffe40078 #fffa2a2d #ffff7500 #ffff9800 #ffffbf00 | 品牌板 11 色 |
| palette_aux1~11 | 亮化辅助板 11 色（如 #ff7e8f00 #ff86c5e3 …） | 辅助 |
| shadow_default_xs/sm/md/lg | #26000000/#26000000/#33000000/#4d000000（dark 版带 050505 色调） | 阴影 |

### 3.2 字号梯度（fp/vp 两套并存）
来源：float.json（等级A）。URL: https://gitee.com/openharmony/global_system_resources/blob/master/systemres/main/resources/base/element/float.json

**新 token 体系（ArkUI 通用层）**：

| 档位 | fp | | 档位 | fp |
|---|---|---|---|---|
| Display_L | 56 | | Subtitle_S | 14 |
| Display_M | 48 | | Body_L | 16 |
| Display_S | 38 | | Body_M | 14 |
| Title_L | 30 | | Body_S | 12 |
| Title_M | 24 | | Caption_L | 12 |
| Title_S | 20 | | Caption_M | 10 |
| Title_extra_S | 16 | | Caption_S | 8 |
| Subtitle_L | 18 | | Subtitle_M | 16 |

**旧 ohos_id 体系（系统组件默认）**：headline1~9 = 96/72/60/48/38/30/24/20/18 vp；sub_title1/2/3 = 18/16/14 fp；button1/2/3 = 16/14/12 fp；body1/2/3 = 16/14/12 fp；caption = 10 vp；caption1 = 10 fp；dialog_tittle = 20 fp；toggle = 12 fp。字重枚举：thin=0, ultralight=1, light=2, regular=3, medium=4, semibold=5, bold=6, heavy=7, black=8。

### 3.3 栅格（断点/列数/边距/gutter）
来源：openharmony/docs 仓 arkts-layout-development-grid-layout.md（等级A）。
URL: https://gitee.com/openharmony/docs/blob/master/zh-cn/application-dev/ui/arkts-layout-development-grid-layout.md

| 断点 | 取值范围(vp) | 设备 | 默认列数(API20+) |
|---|---|---|---|
| xs | [0, 320) | 手持竖屏最小 | 2 |
| sm | [320, 600) | 手持竖屏 | 4 |
| md | [600, 840) | 折叠屏/平板竖 | 8 |
| lg | [840, +∞) | 平板横/PC | 12 |
| xl(可启用) | 自定义(如[1440,…)) | 大屏 | 12 |
| xxl(可启用) | 自定义(如[1600,…)) | 大屏 | 12 |

- 默认断点 value=['320vp','600vp','840vp']，可扩展至 6 断点（示例加 1440/1600vp）。
- columns API≤19 默认 12 列；API20 起默认 {xs:2, sm:4, md:8, lg:12, xl:12, xxl:12}。
- **gutter 默认 0**、margin 默认无（均可按断点配 x/y）；组件级推荐边距见 3.4（margin 12/8vp 等）。HarmonyOS Design 官网（SPA）另有的 gutter/margin 设计推荐值未能直抓 → 未核实。

### 3.4 圆角/间距/阴影/模糊数值（float.json，等级A）

圆角通用阶梯（新体系）：corner_radius_none=0, level1~12=2/4/6/8/10/12/14/16/18/20/22/24, level16=32vp。
旧控件圆角：button=20, small_button=14, switch=10, toggle_button=14, piece=14, checkbox=4, dialog=24, panel=32, notification=16, card=24, grid=12, menu=20, banner=18, icon=8, text_field=20, mark=4, subtab=4, clicked=8, tips/badge=18/14, progress_bar=2vp；default_xs/s/m/l/xl=4/12/18/24/36vp。

间距（新体系）：padding_level0/1/2/…/12/16/24/32/36 = 0/2/4/6/8/10/12/14/16/18/20/22/24/32/48/64/72vp；margin_left/right=16vp。
旧体系：default_padding_start/end/top=12vp（max 24）、bottom_fixed=24/flexible=0；dialog margin 12/12/16；card margin 12；text_paragraph_margin xs/s/m/l/xl=4/8/16/24/48；elements_margin s/m/l=4/8/16；line_space s/l=1.1/1.4vp。

阴影（shadow 值/半径/偏移）：default_xs=20, sm=40(+y10), md=60(+y10), lg=100(半径80,+y20)；floating_sm=120(半径80)、floating_md=160(半径100)；枚举 ohos_id_default_shadow_none/xs/s/m/l=6/0/1/2/3、floating_s/m=4/5；button shadow=6。

模糊层 blur_style（半径/饱和度/亮度，含 dark 变体）：Thin 23px/1.05、Regular 50px/1.8、Thick 57px/1.2、Background Thin 23px、Background Regular 29px、Background Thick 57px、Ultra Thick 80~120px（component ultra_thick window radius 0、saturation 1.9）。

动效（float.json 等级A）：hover_animation_duration_default=250、hover_to_press=100、press=250、focus_animation=100、click_animation=200（ms）。曲线 token 未在 float.json（在主题/代码中）→ 具体曲线公式未核实。

### 3.5 无障碍字号缩放（OH）
float.json：ohos_id_normal_font_size_scale=1.0、big=1.75、large=2.0、max=3.2（dialog/subtab 各自镜像同值）（等级A）。

## 4. Fluent 2 / WinUI 3

### 4.1 Type ramp 全表（Segoe UI Variable）
来源：MS Learn typography 页 + XAML 主题资源页（等级A）。
URL: https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography ；https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-theme-resources

| 样式 | 字重 | 字号/行高 (epx) |
|---|---|---|
| Caption | Regular | 12/16 |
| Body | Regular | 14/20 |
| Body Strong | Semibold | 14/20 |
| Body Large | Regular | 18/24 |
| Body Large Strong | Semibold | 18/24 |
| Subtitle | Display Semibold | 20/28 |
| Title | Display Semibold | 28/36 |
| Title Large | Display Semibold | 40/52 |
| Display | Display Semibold | 68/92 |

字体轴：wght 100(Thin)~700(Bold)（常用 300 Light/350 Semilight/400/600/650?—官方表为 300/350/400/600/700）；opsz 光学轴 8~36pt 自动。最小字号：14px Semibold / 12px Regular。XAML 静态资源：CaptionTextBlockStyle…DisplayTextBlockStyle 同值。

### 4.2 色彩 token（Dark=Default / Light 对）
来源：microsoft-ui-xaml 仓 Common_themeresources_any.xaml（等级A）。
URL: https://github.com/microsoft/microsoft-ui-xaml/blob/main/controls/dev/CommonStyles/Common_themeresources_any.xaml

| token | Light | Dark(Default) |
|---|---|---|
| TextFillColorPrimary | #E4000000 | #FFFFFF |
| TextFillColorSecondary | #9E000000 | #C5FFFFFF |
| TextFillColorTertiary | #72000000 | #87FFFFFF |
| TextFillColorDisabled | #5C000000 | #5DFFFFFF |
| TextFillColorInverse | #FFFFFF | #E4000000 |
| ControlFillColorDefault | #B3FFFFFF | #0FFFFFFF |
| ControlFillColorSecondary | #80F9F9F9 | #15FFFFFF |
| ControlFillColorTertiary | #4DF9F9F9 | #08FFFFFF |
| ControlFillColorDisabled | #4DF9F9F9 | #0BFFFFFF |
| ControlFillColorInputActive | #FFFFFF | #B31E1E1E |
| ControlStrongFillColorDefault | #72000000 | #8BFFFFFF |
| ControlSolidFillColorDefault | #FFFFFF | #454545 |
| SubtleFillColorSecondary/Tertiary | #09000000 / #06000000 | #0FFFFFFF / #0AFFFFFF |
| ControlAltFillColorSecondary | #06000000 | #19000000 |
| ControlOnImageFillColorDefault~Tertiary | #C9FFFFFF / #F3F3F3 / #EBEBEB | #B31C1C1C / #1A1A1A / #131313 |
| AccentFillColorDefault(brush) | SystemAccentColorLight2 | SystemAccentColorLight2 |
| ControlStrokeColorDefault/Secondary | #0F000000 / #29000000 | #12FFFFFF / #18FFFFFF |
| CardStrokeColorDefault | #0F000000 | #19000000 |
| CardStrokeColorDefaultSolid | #EBEBEB | #1C1C1C |
| SurfaceStrokeColorDefault | #66757575 | #66757575 |
| SurfaceStrokeColorFlyout | #0F000000 | #33000000 |
| DividerStrokeColorDefault | #0F000000 | #15FFFFFF |
| FocusStrokeColorOuter/Inner | #E4000000 / #B3FFFFFF | #FFFFFF / #B3000000 |
| CardBackgroundFillColorDefault~Tertiary | #B3FFFFFF / #80F6F6F6 / #FFFFFF | #0DFFFFFF / #08FFFFFF / #12FFFFFF |
| SmokeFillColorDefault | #4D000000 | #4D000000 |
| LayerFillColorDefault/Alt | #80FFFFFF / #FFFFFF | #4C3A3A3A / #0DFFFFFF |
| LayerOnAcrylicFillColorDefault | #40FFFFFF | #09FFFFFF |
| LayerOnMicaBaseAltFillColorDefault~Tertiary | #B3FFFFFF / #0A000000 / #F9F9F9 | #733A3A3A / #0FFFFFFF / #2C2C2C |
| **SolidBackgroundFillColorBase** | **#F3F3F3** | **#202020** |
| SolidBackgroundFillColorSecondary | #EEEEEE | #1C1C1C |
| SolidBackgroundFillColorTertiary | #F9F9F9 | #282828 |
| SolidBackgroundFillColorQuarternary | #FFFFFF | #2C2C2C |
| SolidBackgroundFillColorQuinary/Senary | #FDFDFD / #FFFFFF | #333333 / #373737 |
| SolidBackgroundFillColorBaseAlt | #DADADA | #0A0A0A |
| SystemFillColorSuccess | #0F7B0F | #6CCB5F |
| SystemFillColorCaution | #9D5D00 | #FCE100 |
| SystemFillColorCritical | #C42B1C | #FF99A4 |
| SystemFillColorNeutral | #72000000 | #8BFFFFFF |
| SystemFillColorSolidNeutral | #8A8A8A | #9D9D9D |
| SystemFillColorSuccessBackground | #DFF6DD | #393D1B |
| SystemFillColorCautionBackground | #FFF4CE | #433519 |
| SystemFillColorCriticalBackground | #FDE7E9 | #442726 |

accent：SystemAccentColor（用户设置）；brush 映射 AccentFillColorDefault=SystemAccentColorLight2 等。高对比走 SystemColor* 系（用户配色）。8 位 hex 前两位为 alpha。

### 4.3 圆角与控件尺寸
来源：MS Learn Geometry 页（等级A）。URL: https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry

| 圆角 | 用途 |
|---|---|
| 8px | 顶层容器（窗口/flyout/对话框；snap 或最大化时不圆角） |
| 4px | 页内元素（Button/CheckBox/TextBox/ListView/进度条滑块等；ToolTip 例外 4px） |
| 0px | 相邻直边相交处 |

资源键：ControlCornerRadius=4px、OverlayCornerRadius=8px。控件尺寸（xaml-theme-resources 页，等级A）：导航返回按钮 40×40px / 小号 30×30px。

### 4.4 Mica/Acrylic 层级
来源：Layering & elevation 页 + Acrylic 页（等级A）。
URL: https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering ；.../style/acrylic ；Mica: .../style/mica

海拔表（elevation value / 描边宽）：

| 对象 | elevation | stroke |
|---|---|---|
| 窗口/对话框 | 128 | 1px |
| Flyout | 32 | 1px |
| Tooltip | 16 | 1px |
| Card | 8 | 1px |
| Control（rest/hover） | 2 | 1px |
| Control pressed | 1 | 1px |
| Layer | 1 | 1px |

层级：base 层（导航/命令）+ content 层；材质序（低→高）：Solid → Mica（窗口底，不透桌面壁纸、省电）→ Background Acrylic（透壁纸，用于 transient：菜单/flyout）→ In-app Acrylic（App 内层叠，如 NavigationView）。Acrylic 配方：背景+模糊+exclusion 混合+色彩 tint+噪声（页述定性，tint hex 见 Layer/Acrylic token 项）。省电模式/关透明/失活自动回退纯色。

### 4.5 Fluent motion（时长/曲线）
来源：MS Learn Motion 页（等级A）+ Common_themeresources_any.xaml（等级A）。

| 用途 | Ease | Timing |
|---|---|---|
| Direct Entrance | cubic-bezier(0,0,0,1) | 167 / 250 / 333 ms |
| Existing Elements (Point to Point) | cubic-bezier(0.55,0.55,0,1) | 167 / 250 / 333 ms |
| Direct Exit | cubic-bezier(0,0,0,1) | 167 ms |
| Gentle Exit | cubic-bezier(1,0,1,1) | 167 ms |
| Bare Minimum (fade in+out) | linear | 83 ms |
| Strong Entrance K1 | cubic-bezier(0.85,0,0,1) | 167 ms |
| Strong Entrance K2 | cubic-bezier(0.85,0,0.75,1) | 167 ms |
| Strong Entrance K3 | cubic-bezier(0.85,0,0,1) | 333 ms |

XAML 资源键：ControlNormalAnimationDuration=250ms、ControlFastAnimationDuration=167ms、ControlFasterAnimationDuration=83ms、ControlFastOutSlowInKeySpline=0,0,0,1。

## 5. OpenHarmony 公开常量补充（UI 侧）

（§3 已含主值；此处补 float.json 其余控件常量，等级A，URL 同 §3.2）

| 常量 | 值 |
|---|---|
| ohos_id_search_height | 56vp |
| ohos_id_items_size | 76vp（列表项基准） |
| ohos_id_ideal_size | 64vp |
| toggle_height / toggle 文字 | 28vp / 12fp |
| stepper_control_height / button_pressed | 48vp / 40vp |
| datapanel_circle_thickness | 24vp |
| slider 滑块热区阴影 out/in | 4vp / 6vp；focus 侧距 2vp |
| badge_age_size / little_badge | 23vp / 6vp；badge 边框 1vp |
| 文本选择手柄 外/内柄 | 14vp / 12vp |
| subtab 行高/间距 | 线高 2vp、gap 8vp、上下 padding 17/18vp |
| dialog 最大宽（横/竖） | maxsize_scale 0.9 / 0.8；big_dialog_width 216 / max 256 |
| 滚动摩擦/回弹 | friction 0.75、fling scale 1.5、over-edge 1.848、spring response 0.417 |
| calendar 格 | 周/日格 48×48、19/41 高、列距 39vp |
| 边框宽 | border extra_small 0.5px / small 1px / medium 2px / larger 1vp / extra_larger 2vp |

## 6. 图标规格各家全表

| 家 | 应用图标源尺寸 | 自适应/分层 | 圆角策略 | 单色层 | 桌面显示档 | 来源(级) |
|---|---|---|---|---|---|---|
| Android | 自适应层各 108×108dp | 前景/背景 2 层+可选 monochrome；启动器显中心 72dp，安全区 66dp；主题图标 Android 13+ | OEM mask（圆/超椭圆等） | monochrome 层 | 由 launcher 定（48dp 基准显示未核实） | developer.android.com/develop/ui/compose/system/icon_design_adaptive（B：SPA 未直抓，108/72/66 经 DDG 多源摘录一致，含官方页摘要） |
| iOS/iPadOS/macOS | 1024×1024px | 背景+1..n 前景（Icon Composer 分层，Liquid Glass 效果） | 系统统一圆角遮罩（squircle，连续曲率；具体半径数值官方未发布→未核实） | dark/clear/tinted 6 外观变体 | 系统自动缩放（Spotlight/设置等） | app-icons.json（A） |
| tvOS | 800×480px | 2~5 层视差 | 圆角矩形（同心边） | N/A | 焦点放大+安全区 | 同上（A） |
| visionOS | 1024×1024px | 背景+1~2 层 3D | 圆形遮罩 | N/A | — | 同上（A） |
| watchOS | 1088×1088px | 分层 | 圆形遮罩 | N/A | — | 同上（A） |
| HarmonyOS | app.json5 icon（AGC 上架图；具体 px 档未核实） | 静态图（无官方分层规范可直抓） | 系统不强制统一圆角（桌面层未核实） | 未核实 | 未核实 | developer.huawei.com（SPA，未能核到数值） |
| Windows/WinUI | 应用列表图标（多尺寸 ico/png；官方 UI 图标档 16/20/24/32 未核实具体表） | 无分层；Win11 系统对窗口统一 8px 圆角；开始菜单磁贴未核实 | 系统 8px（窗口）/控件 4px | 无 | 未核实 | learn Geometry（A：仅圆角已核） |


## 7. 触控/指针目标

| 家 | 最小目标 | hover/指针 | 来源(级) |
|---|---|---|---|
| Android/M3 | 48×48dp（Wear OS 特例 40dp） | 指针目标可小于 48dp（桌面指针交互） | developer.android.com/guide/topics/ui/accessibility/views/apps-views、/training/wearables/accessibility、/design/ui/desktop/guides/interaction/pointer-interactions（B：DDG 官方页摘录） |
| iOS | 44×44pt（惯用口径） | 未核实（PointerPad/鼠标 hover 数值无官方表） | HIG（B，未直抓原文） |
| OH | 组件默认含热区（slider 热区 4/6vp 等）；统一最小触控目标数值未在 float.json → 未核实（社区引 48vp 非官方） | hover 动效 250ms（A，float.json） | gitee float.json（A）/未核实项标注 |
| Windows/WinUI | 交互控件最小目标未在已抓页面给单值（UWP 时代 40×40epx / 推荐 92×92 未核到现行页）→ 未核实；导航按钮 40×40、小 30×30（A） | focus 动效 100ms（OH）对照；WinUI hover 见动效表 | learn（A/未核实标注） |
| WCAG 2.5.8 | AA 24×24 CSS px 最小；AAA 2.5.5 44×44（引用值，未单独抓页） | — | w3.org（B：标准号引用） |

## 8. 无障碍数值

| 项 | 值 | 来源(级) |
|---|---|---|
| WCAG 1.4.3 对比度 AA | 正文 ≥4.5:1；大字（≥18pt 或 ≥14pt 粗体，≈24px/18.5px）≥3:1 | w3.org/WAI/WCAG22/Understanding/contrast-minimum.html（A） |
| WCAG 1.4.6 AAA | 正文 ≥7:1；大字 ≥4.5:1 | 同上（A，页内交叉引用） |
| WCAG 1.4.11 非文本 | UI 组件/图形 ≥3:1 | 同上（A，页内引用） |
| Android fontScale | 设置档数值官方表未直抓 → 未核实（Pixel 滑杆 1.0~2.0、13+ 非线性为社区口径） | 未核实 |
| iOS 字号档 | 12 标准档（xSmall~xxxLarge）+5 AX 档（见 §2.2 全表） | typography.json（A） |
| OH 无障碍字号档 | 1.0 / 1.75 / 2.0 / 3.2 | float.json（A） |
| TalkBack 焦点框 | 数值规范未核实（官方无公开 px 规格） | 未核实 |
| VoiceOver 焦点框 | 数值规范未核实（HIG 无 px 规格） | 未核实 |
| Windows 高对比 | HighContrast 主题映射 SystemColor* 8 键（Window/WindowText/ButtonFace/ButtonText/Highlight/HighlightText/Hotlight/GrayText） | Common_themeresources_any.xaml（A） |
| Windows Narrator | 焦点框数值未核实 | 未核实 |

## 9. 安全区/避让（系统栏高度）

| 家 | 数值 | 来源(级) |
|---|---|---|
| Android | insets 类型：statusBars/navigationBars/systemBars/ime(键盘)/displayCutout/tappableElement（类型名口径）；具体默认高度 dp 官方无常量表（随设备）→ 状态栏 24dp/手势条 48dp(3 键) 等社区值未核实 | developer.android.com edge-to-edge 文档（B：类型名口径；高度数值未核实） |
| iOS | safe area 组成：顶部刘海/Dynamic Island + 底部 Home Indicator；官方未发布 pt 常量表（社区值：状态栏 44/47/59pt、home indicator 34pt）→ 未核实 | developer.apple.com（B/未核实） |
| OH | avoidArea（系统栏/键盘/手势）运行时获取；float.json 无固定高度常量（"44vp" 为社区口径）→ 未核实 | ArkUI 文档（B：API 名已核；数值未核实） |
| Windows | 任务栏高度无常量（自动隐藏/缩放可变）；标题栏标准 32px（Win32 SM_CYCAPTION，社区口径）→ 未核实；WinUI ExtendsContentIntoTitleBar 自绘 | learn（未核实标注） |
| 键盘避让 | Android ime inset / iOS keyboardAvoidance / OH avoidArea RECT.keyboard / WinUI InputPane（数值均运行时） | 各家 API 文档（B） |

## 附：未核实清单
M3 种子色具体 hex（官方动态生成不发布）；M2 间距阶梯原文；iOS 语义灰阶 hex（HIG 已移除数值表）；iOS 44pt 原文页/SF Symbols 各效 ms/HIG 网格边距/Apple 动效表（无公开表）；HarmonyOS Design 官网栅格推荐值、品牌板官方页、图标 px 档、动效曲线；Android adaptive icon 原文直抓（数值多源摘录一致）、fontScale 设置档表；各家屏幕阅读器焦点框像素规格；iOS/Android/OH/Windows 系统栏高度官方常量。
