# 对标补深：Apple iOS 应用规范硬数值（第二轮·二号）

> 采集：2026-11-08（第二轮深集）。方法：curl 直抓 developer.apple.com 文档数据端点（`/tutorials/data/documentation/<slug>.json`，旧 `.json` 端点已 404）+ 帮助页/支持页 HTML 全文提取。
> 定位：`research/应用规范-Apple-iOS对标-2026-11.md`（第一轮 A–M）的补深与补缺，不重复已核实条目；本档只列事实/表格/数值/枚举。
> ✅=本次直接抓取官方原文验证；未核到官方口径的一律标「未核实」。

## 1. App Review Guidelines 全量结构化（2026-10-16 更新版）

来源 ✅ https://developer.apple.com/app-store/review/guidelines/

### 1.1 五大节逐条索引（条号+一句话要点）

| 节 | 条目 |
|---|---|
| 1 Safety | 1.1 Objectionable Content（1.1.1 歧视/1.1.2 暴力写实/1.1.3 武器/1.1.4 色情/1.1.5 宗教/1.1.6 虚假信息/1.1.7 发灾难财）；1.2 UGC（过滤+举报+拉黑+联系方式，1.2.1 Creator Content）；1.3 Kids Category；1.4 Physical Harm（1.4.1 医疗/1.4.2 药量计算/1.4.3 烟酒毒/1.4.4 DUI/1.4.5 危险挑战）；1.5 Developer Information；1.6 Data Security；1.7 报案类 app 须执法参与 |
| 2 Performance | 2.1 App Completeness（拒绝崩溃/占位）；2.2 Beta Testing（beta 只准走 TestFlight）；2.3 Accurate Metadata（2.3.1 诚实描述/2.3.2 IAP 标注/2.3.3 截图为 app 实况/2.3.4 预览只用 app 录屏/2.3.5 类目/2.3.6 分级问卷/2.3.7 名称≤30 字符禁关键词堆砌/2.3.8 元数据按 4+ 标准/2.3.10 禁他平台元素/2.3.12 What's New 须具体/2.3.13 in-app events/2.3.14 creative assets）；2.4 Hardware Compatibility（2.4.2 省电禁挖矿/2.4.5 Mac 沙箱等 9 款）；2.5 Software Requirements（2.5.1 公共 API/2.5.2 容器外禁读写禁下可执行码/2.5.5 IPv6/2.5.6 强制 WebKit/2.5.13 面容 ID 须 LocalAuthentication 且 <13 岁备选/2.5.16 App Clip 功能须全量入母 app 二进制/2.5.18 展示广告限主二进制/2.5.19 禁隐藏功能） |
| 3 Business | 3.1 Payments（3.1.1 数字解锁必须 IAP；3.1.1(a) 外链 entitlement；3.1.2 订阅≥7 天、全设备；3.1.3 例外清单 a–g Reader/多平台/企业/人人/实物/独立免费/广告管理；3.1.4 硬件绑定；3.1.5 加密货币 5 款）；3.2 Other（3.2.1 可接受 8 款/3.2.2 不可接受含诱导评分、APR≤36% 贷款） |
| 4 Design | 4.1 Copycats；4.2 Minimum Functionality（4.2.2 禁营销物料/链接集；4.2.3 须独立运行+下载量披露；4.2.6 模板 app；4.2.7 远程桌面规则）；4.3 Spam（禁同 app 多 Bundle ID；约会/手电/壁纸/算命类默认拒新）；4.4 Extensions（4.4.1 键盘规则）；4.5 Apple Sites/Services（4.5.3 禁 spam Game Center/Push/Live Activities；4.5.4 推送；4.5.6 emoji）；4.7 小程序/流游戏/模拟器（4.7.2 禁暴露原生 API、4.7.4 须 universal links 索引、4.7.5 年龄门）；4.8 第三方登录须等价隐私选项（例外清单）；4.9 Apple Pay 披露；4.10 禁变现系统能力（推送/相机/陀螺仪） |
| 5 Legal | 5.1 Privacy（5.1.1 收集与存储：隐私政策强制/用途串/最小化/拒权替代方案/app 内账号删除/受监管行业须法人提交；5.1.2 数据使用与共享：未经许可不得共享、跨用途须再同意、禁建用户画像、禁 Contacts/Photos 滥用；5.1.3 健康；5.1.4 Kids（COPPA/GDPR、禁三方分析广告、家长门≠法定同意）；5.1.5 定位）；5.2 IP；5.3 博彩（真钱游戏须牌照+地理围栏+免费上架、禁 IAP 购真钱筹码）；5.4 VPN（须 NEVPNManager+组织账号）；5.5 MDM（须向 Apple 申请能力）；5.6 开发者行为准则（5.6.1 只准系统 API 请求评分） |

### 1.2 重点条原文关键句（✅ 直抓）

- **4.2 最低功能**："Your app should include features, content, and UI that elevate it beyond a repackaged website. If your app is not particularly useful, unique, or 'app-like,' it doesn't belong on the App Store."（纯网站套壳=拒）
- **2.5.1 公共 API**："Apps may only use public APIs and must run on the currently shipping OS. … Apps should use APIs and frameworks for their intended purposes."（公共 API 专用途；私有 API 零容忍体现于此条+审核技术扫描）
- **2.5.2 可执行代码**："Apps should be self-contained in their bundles, and may not read or write data outside the designated container area, nor may they download, install, or execute code which introduces or changes features or functionality of the app."（教育类教学码例外：源码必须完全可见可编辑）
- **5.1.1 隐私（用途与同意）**："Apps that collect user or usage data must secure user consent for the collection… Paid functionality must not be dependent on or require a user to grant access to this data."；"(iii) Data Minimization: Apps should only request access to data relevant to the core functionality…use the out-of-process picker or a share sheet rather than requesting full access"；"(v) …If your app supports account creation, you must also offer account deletion within the app."
- **5.1.2 数据使用**："you may not use, transmit, or share someone's personal data without first obtaining their permission… You must clearly disclose where personal data will be shared with third parties, including with third-party AI, and obtain explicit permission";"(ii) Data collected for one purpose may not be repurposed without further consent"
- **3.1.1 IAP**："If you want to unlock features or functionality within your app…you must use in-app purchase. Apps may not use their own mechanisms to unlock content or functionality, such as license keys, augmented reality markers, QR codes, cryptocurrencies…"
- **1.3 儿童类**："These apps must not include links out of the app, purchasing opportunities, or other distractions to kids unless reserved for a designated area behind a parental gate. …Kids Category apps may not send personally identifiable information or device information to third parties. Apps in the Kids Category should not include third-party analytics or third-party advertising."（例外：不采 IDFA/身份/位置且人工审广告素材的 context 广告）
- **4.5.4 推送**："Push Notifications must not be required for the app to function, and should not be used to send sensitive personal or confidential information. Push Notifications should not be used for promotions or direct marketing purposes unless customers have explicitly opted in…"
- 作弊条款（Introduction）："If you attempt to cheat the system…your apps will be removed from the store and you will be expelled from the Apple Developer Program."
- 审核时间："it can take up to 24-hours for your app to appear on all selected storefronts"（After You Submit 节）
- 替代分发入口："Guidelines that include 🔑 apply to Notarization for iOS and iPadOS apps"（页面含 Notarization Review Guidelines 高亮过滤）

## 2. 上架素材规格全表

### 2.1 截图 ✅（https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications）

- 数量：**每设备尺寸 1–10 张**；格式 .jpeg/.jpg/.png；**不得含 alpha/透明通道**；须过 App Review Guidelines。
- 必需尺寸：iPhone 至少 1 张 **Dynamic Island (medium display)** 档；iPad 支持则至少 1 张 **13-inch** 档。高分辨率可自动缩放派生低档。
- iPhone 档位表（竖屏 px；横屏对调）：

| 显示档 | 代表机型 | 截图尺寸（竖/横） | 要求 |
|---|---|---|---|
| iPhone Duo（折叠） | iPhone Duo | 外屏 1398×2034 / 2034×1398；内屏 2007×2853 / 2853×2007 | 2027-04 起 iOS 27.1 SDK app 必须提交；有专属 badge |
| Dynamic Island 大屏 | 18/17/16 Pro Max、16/15 Plus、14 Pro Max | 1320×2868、1290×2796、1260×2736 三选 | 必需（否则降档缩放） |
| Face ID 大屏 | 14 Plus、13/12 Pro Max、11/XS Max/XR | 1284×2778 或 1242×2688 | 备选 |
| Dynamic Island 中屏 | 18/17 Pro、17/16 Pro、16/15、15 Pro、14 Pro | 1206×2622、1179×2556 | 必需档 |
| Face ID 中屏 | 17e/16e、14/13、13/12 mini、XS/X | 1080×2340、1170×2532、1125×2436 | 备选 |
| Home 键大屏 | 8/7/6S/6 Plus | 1242×2208 | 备选 |
| Home 键中屏 | SE2/3、8/7/6S/6 | 750×1334 | 备选 |
| 4 寸 | SE1、5s/5c/5 | 640×1136（含状态栏）/640×1096 | 备选 |
| 3.5 寸 | 4s/4 | 640×960/640×920 | 备选 |

- iPad 档：13"（M5/M4 Pro、Pro 3–6、Air M2–M4）= 2064×2752 或 2048×2732（必需）；12.9" = 2048×2732；11"（Pro/Air/mini/A16）= 1488×2266、1512×2316、1640×2360、1668×2388、1668×2420；10.5" = 1668×2224；9.7" = 1536×2048 等。

### 2.2 预览视频 ✅（…/app-preview-specifications）

- 数量上限同截图（每尺寸最多 10 个预览——页面规格表）；处理时长至多 24 小时；展示顺序：预览永远排在截图前。
- 时长：**15 秒 – 30 秒**；文件 ≤**500MB**；帧率 ≤**30fps**；方向：iOS 横/竖皆可，macOS/tvOS/visionOS 仅横屏。
- 编码：.mov/.m4v/.mp4（**H.264**，High Profile ≤L4.0，目标码率 10–12 Mbps）或 .mov（**ProRes 422 HQ only**，VBR ~220 Mbps）；音频立体声 256kbps AAC，44.1/48kHz。
- iPhone 接受分辨率：**886×1920**（竖）/ **1920×886**（横，19.5:9）——与设备原生分辨率解耦，统一 19.5:9 标准档。

### 2.3 App 图标 ✅

- HIG App icons（https://developer.apple.com/design/human-interface-guidelines/app-icons）：**分层图标为新默认**——背景层+1..n 前景层（iOS/iPadOS/macOS/watchOS）；tvOS 2–5 层视差；visionOS 背景层+1–2 层；经 **Icon Composer**（随 Xcode 分发）制作分层 .icon 文件，系统施加 Liquid Glass（高光/折射/半透明）；非分层位图提交时系统自动分割为两层并派生暗/染色变体；提供方形画布、圆角由系统裁切。
- Xcode 侧（✅ https://developer.apple.com/tutorials/data/documentation/xcode/configuring-your-app-icon.json → 页面 /documentation/xcode/configuring-your-app-icon）：iOS/iPadOS/tvOS/watchOS **单张 1024×1024px 自动派生全变体（Single Size 默认）**；visionOS 单张 1024×1024；macOS 需逐尺寸；暗色变体须透明背景、染色变体灰度图。
- App Store Connect：图标随 build 走 asset catalog；上架后改图标必须发新版本（✅ add-an-app-icon 帮助页）。

### 2.4 元数据字数 ✅（…/reference/app-information/app-information + platform-version-information）

| 字段 | 上限 |
|---|---|
| App Name | ≥2 且 **≤30 字符**（Guidelines 2.3.7 同） |
| Subtitle | **≤30 字符** |
| Promotional Text（可随时改，iOS 11+ 显示） | **≤170 字符** |
| Description | **≤4000 字符**，纯文本禁 HTML |
| Keywords | **≤100 字节**（每个 >2 字符） |
| What's New | **≤4000 字符**（Guidelines 2.3.12 要求具体列变更） |
| In-App Event 内部名/事件名/短描述/长描述 | 64 / 30 / 50 / 120 字符（✅ offer-in-app-events） |
| 隐私政策 URL | iOS/macOS 必填；tvOS 需内嵌文本 |

## 3. WidgetKit / Live Activity 硬数值

- **刷新预算（官方公开口径，第一轮未核实项已补）** ✅（https://developer.apple.com/documentation/widgetkit/keeping-a-widget-up-to-date）："WidgetKit uses a budget to distribute widget reloads over the course of the day…For a widget the user frequently views, a daily budget typically includes **from 40 to 70 refreshes**. This rate roughly translates to widget reloads every **15 to 60 minutes**"；每 widget 独立预算（同 app 两个实例各算各的）；24h 窗口随用户作息漂移；前几日学习期可能多发。**不计数场景**：母 app 在前台、活跃 audio/navigation 会话、Now Playing（MPNowPlayingInfoCenter）、 widget 不在当前可见桌面页时（可见时补偿刷新）、系统外观/语言/Dynamic Type/无障碍设置变化（这些系统自动刷新，app 不得主动请求）、StandBy 内系统自定刷新。位置显著变化会触发带定位 widget 刷新。
- **Live Activity 时限原文** ✅（https://developer.apple.com/documentation/activitykit/displaying-live-data-with-live-activities）："A Live Activity can be active for **up to eight hours** unless its app or a person ends it… After the eight-hour limit, the system automatically ends the Live Activity, and immediately removes it from the Dynamic Island. However, the Live Activity remains on the Lock Screen until a person removes it or for **up to four additional hours**… a Live Activity remains on the Lock Screen for a maximum of **12 hours**."；dismissible：默认结束后锁屏留 4h；`immediate` 立即移除；`after(date)` 4h 窗口内取先到者。
- **更新通道**：本地 ActivityKit / 远程 ActivityKit push（`event: start|update|end`、`content-state`、`timestamp`、`stale-date`、`dismissal-date`，✅ 同页+APNs payload 表）；每个 Live Activity 独立沙箱，**不能联网、不能收位置更新**（更新必须由 app/push 驱动）；图片资产超过展示尺寸可能启动失败。
- **推送预算口径** ✅（starting-and-updating-live-activities-with-activitykit-push-notifications）：个人通知有更新预算，超预算被节流；`apns-priority:5` 的低优先级更新**不占预算**（混用 5/10 防节流）；`NSSupportsLiveActivitiesFrequentUpdates=YES` 开频繁档，用户可在设置里关（`frequentPushesEnabled` 可读）。
- **控件尺寸表（pt）** ✅（HIG widgets Specifications）：

| 屏宽(pt) | Small | Medium | Large | 锁屏 Circular | Rectangular | Inline |
|---|---|---|---|---|---|---|
| 430×932 | 170×170 | 364×170 | 364×382 | 76×76 | 172×76 | 257×26 |
| 428×926 | 170×170 | 364×170 | 364×382 | 76×76 | 172×76 | 257×26 |
| 414×896 | 169×169 | 360×169 | 360×379 | 76×76 | 160×72 | 248×26 |
| 393×852 | 158×158 | 338×158 | 338×354 | 72×72 | 160×72 | 234×26 |
| 375×812 | 155×155 | 329×155 | 329×345 | 72×72 | 157×72 | 225×26 |

iPad 加 Extra Large（如 820×1180 档 Canvas 715.5×342 / Device 628×300）；widget 内边距标准 16pt（紧凑可 11pt）；**widget 文本 ≥11pt**；支持 Dynamic Type Large→AX5（✅ HIG widgets）。containertool：现行文档已不提及（历史 CLI，未核实其存续）；widget 扩展内存上限无公开数值（未核实）。

- **Live Activity 尺寸表（pt）** ✅（HIG live-activities Specifications）：iPhone 430×932 档 compact（leading/trailing）62.33×36.67、minimal 36.67–45×36.67、expanded 408×84–160、Lock Screen 408×84–160、限宽圆 **28×28**（横屏灵动岛须能塞进 28pt 直径圆）；393×852 档 compact 52.33×36.67、expanded/锁屏 371×84–160；iPad 锁屏 500×84–160（13"）等；动画上限 **2 秒**（Always-On 屏不做动画）；锁屏呈现可自定义背景色，compact/minimal/expanded 背景黑且不可改。

## 4. HIG 硬数值页

- **Motion** ✅（/design/human-interface-guidelines/motion）：游戏 **30–60fps** 流畅线；visionOS 外周振荡**避开 ~0.2Hz**；反馈动效应"brief and precise"、不得阻塞交互；watchOS 布局动画自带缓动不可关。⚠️ 官方**未提供逐场景动效时长表**（0.2s/0.35s 等具体 ms 数值表在现行 HIG 不存在，社区数值均非官方——未核实）。
- **Materials** ✅（/materials）：新版主推 **Liquid Glass** 两变体：regular（模糊+亮度调节，滚动边缘效果）与 clear（高透明，悬浮媒体上）；亮底内容建议 **35% 不透明度暗化层**；标准材质仍 4 档：**ultraThin / thin / regular（默认）/ thick**；vibrancy 标签层级 label（最高对比）→ secondaryLabel → tertiaryLabel → quaternaryLabel（禁用于 thin/ultraThin）；fill 三级 fill/secondaryFill/tertiaryFill；具体 blur 半径数值官方未公开（未核实）。
- **Layout** ✅（/layout）：尊重系统 safe area/margins/layout guides（灵动岛/刘海/Home Indicator/Dock）；预置布局引导限定文本可读宽度；visionOS 可设窗口/卷最小最大尺寸。⚠️ 最小可点区不在 layout 页——现行口径在 **buttons** 页 ✅："a button needs a hit region of **at least 44x44 pt** — in visionOS, **60x60 pt**"（https://developer.apple.com/design/human-interface-guidelines/buttons）；按钮形状尺寸表 Mini 28 / Small 32 / Regular 44 / Large 52 / XL 64 pt。
- **App Icon 规格页** ✅（/app-icons）：见 §2.3——分层+Liquid Glass+Icon Composer 为现行规范；方形画布系统裁形；禁预裁切（破坏高光/边缘锯齿）；前景层避免软羽化边缘；tvOS 2–5 层、visionOS 1–2 层+背景层。
- **Typography** ✅（/typography）：系统字号默认/最小 iOS 17pt/11pt（mac 13/10、tv 29/23、vision 17/12、watch 16/12）；**iOS Dynamic Type 全档（Body/Large Title，pt）**：

| 档 | Body | Large Title |
|---|---|---|
| xSmall | 14 | 31 |
| Small | 15 | 32 |
| Medium | 16 | 33 |
| **Large（默认）** | **17** | **34** |
| xLarge | 19 | 36 |
| xxLarge | 21 | 38 |
| xxxLarge | 23 | 40 |
| **AX1** | **28** | **44** |
| **AX2** | **33** | **48** |
| **AX3** | **40** | **52** |
| **AX4** | **47** | **56** |
| **AX5** | **53** | **60** |

（第一轮"AX1–AX5 数值未核实"已补齐；每档含 headline/callout/subhead/footnote/caption 全行距，tracking 表 6–96pt 逐号给出。）

## 5. MetricKit 全指标清单（2026 新 API 口径）

来源 ✅ https://developer.apple.com/documentation/metrickit（框架页 topicSections 全量）：

- 框架定位："Measure your app's performance using **daily** metric and diagnostic reports from real users."
- **指标域全清单**（新 `MX*Metric` 类）：在用时长：TotalForegroundTime / TotalBackgroundTime / TotalBackgroundAudioTime / TotalBackgroundLocationTime / LocationActivityTime / CellularConditionTime。**启动与响应**：TimeToFirstDraw / OptimizedTimeToFirstDraw / ApplicationResumeTime / ExtendedLaunch / **HangTime** / **HitchTime**。CPU/内存：CPUTime / CPUInstructionsCount / CPUExceptionDiagnostic / **PeakMemory** / SuspendedMemory / MemoryExceptionDiagnostic。GPU/显示：GPUTime / MetalFrameRate / PixelLuminance / AveragePixelLuminance。网络：TotalWiFiUpload/Download、TotalCellularUpload/Download。磁盘：LogicalDiskWrites / **DiskWriteExceptionDiagnostic** / TotalDiskSpaceCapacity / TotalFileCount / TotalFileSize。终止：**ForegroundTermination / BackgroundTermination**。自定义：SignpostIntervalMetric + mxSignpost 系列。
- **字段结构**（✅ 各类页面）：
  - `HangTimeMetric`：histogram（hang 时长直方图；**>9s 落末桶**）；intervalEntries+stateEntries 双来源。
  - `TimeToFirstDrawMetric`：histogram；测量终点=**第一次 Core Animation commit**（首帧提交显示管线时刻）。
  - `HitchTimeMetric`：ratio + totalHitchTime + totalAnimationTime（感知加权 hitch 率）。
  - `PeakMemoryMetric`：value（intervalEntries only）。
  - `ForegroundTerminationMetric`：6 计数——normalTerminationCount / **memoryLimitTerminationCount** / badAccessTerminationCount / abnormalTerminationCount / illegalInstructionTerminationCount / **watchdogTerminationCount**。
  - `CPUExceptionDiagnostic`：callStackTree + totalCPUTime + totalSampledTime。
  - `DiskWriteExceptionDiagnostic`：callStackTree + **totalBytesWritten**。
  - `CrashDiagnostic` / `HangDiagnostic` / `AppLaunchDiagnostic`：callStackTree 为主结构（✅ 各页）。
  - 经典 `MXDiagnosticPayload`（✅ /documentation/metrickit/mxdiagnosticpayload）：crashDiagnostics、cpuExceptionDiagnostics、hangDiagnostics、appLaunchDiagnostics、diskWriteExceptionDiagnostics 五数组 + timeStampBegin/timeStampEnd + jsonRepresentation()/dictionaryRepresentation()。（经典 MXAppMetricPayload 页已 404——被新指标类体系取代。）
- excessive wakeup 指标：现行 MetricKit 指标清单中**无独立 wakeup 指标**（未核实其口径；Xcode Organizer 历史有 CPU 唤醒诊断，不在 MetricKit payload 清单内）。

## 6. APNs 深挖

来源 ✅ /documentation/usernotifications/generating-a-remote-notification、sending-notification-requests-to-apns、pushing-background-updates-to-your-app、registering-your-app-with-apns。

- **payload 上限**：普通 **4KB（4096B）**；VoIP **5KB（5120B）**；禁压缩 JSON body；自定义键只能作 `aps` 的兄弟键（放 `aps` 内会被忽略），值须原始类型。
- **`aps` 字典键全集（Table 1）**：alert（Dictionary|String）/ badge（Number，0=清除）/ sound（String 或 critical Dictionary）/ thread-id / category / content-available / mutable-content / target-content-id / **interruption-level**（"passive"|"active"|"time-sensitive"|"critical"）/ relevance-score（0–1，最高分进摘要；Live Activity 更新可用任意 Double）/ filter-criteria（专注模式过滤，对应 SetFocusFilterIntent）/ Live Activity 专用：stale-date、content-state、timestamp、event（start|update|end）、dismissal-date、attributes-type、attributes。
- **alert 字典键（Table 2）**：title / subtitle / body / launch-image / title-loc-key / title-loc-args / subtitle-loc-key / subtitle-loc-args / loc-key / loc-args。
- **critical sound 字典键（Table 3）**：critical（1 启用）/ name（bundle 或 Library/Sounds 内音频，"default"=系统音）/ volume（0.0–1.0）。
- **静默推送预算官方口径**："The system treats background notifications as **low priority**… the system doesn't guarantee their delivery. In addition, the system may throttle… **don't try to send more than two or three per hour**."；要求 `content-available:1` 且 payload 不含 alert/badge/sound；POST 头 `apns-push-type: background`、`apns-priority: 5`；系统可能延迟/合并/丢弃；受后台刷新开关约束。
- **HTTP 头（POST /3/device/<token>）**：`:method`/`:path` 必填；`authorization`（token 认证必填，bearer）；`apns-push-type`（watchOS 6+ 必填，其余建议：alert/background/voip/…，与 payload 不符可能报错/延迟/丢弃）；`apns-id`（UUID）；`apns-expiration`（UNIX 秒；0=只送一次不存储；非 0=存储重试至该时刻，**存储上限 30 天**）；`apns-priority`（省略=10；**10 立即 / 5 按电况 / 1 电况优先且不唤醒设备**）；`apns-topic`（必填，一般=bundle ID）；`apns-collapse-id`（合并同值通知，**≤64 字节**）。连接：HTTP/2 + TLS 1.2+，api.development/api.push.apple.com:443 或 **2197**；每 bundle ID 只存 1 条离线通知（通常最新者胜）；GOAWAY 帧带 reason。
- **token 轮换官方口径**："Never cache device tokens in local storage. **APNs issues a new token** when the user restores a device from a backup, **when the user installs your app on a new device**, and **when the user reinstalls the operating system**. You get an up-to-date token each time you ask the system to provide the token."（每次注册回调取最新并向自有服务器转发；失效 token 响应 410 Unregistered——响应错误串见 handling-notification-responses-from-apns。）

## 7. App Store 营销面规格

- **Custom Product Pages** ✅（https://developer.apple.com/help/app-store-connect/create-custom-product-pages/configure-multiple-product-page-versions）：每 app **最多 70 个** CPP；每页可配独立截图/预览/推广文本/关键词，可本地化，自动生成唯一 URL；须过审核才可见；可挂 app deep link（iOS 18+ 点 Open 直达内容）；App Analytics 在该页 ≥5 次首装后出指标（展示/下载/转化率）。
- **In-App Events** ✅（offer-in-app-events/overview-of-in-app-events + reference/in-app-events/in-app-event-media-and-audio-specifications）：App Store 与 Apple Games 同时发布 **≤10 个**事件；ACC 内已批准 **≤15 个/app**；提交须 App Review。媒体规格：Event Card 图/视频 **16:9 横**（min 1920×1080，max 3840×2160，≤500MB，图 .jpg/.png，视频 .mov/.m4v/.mp4 30/60fps）；Event Details **9:16 竖**（1080×1920–2160×3840，≤500MB）；音频立体声 256kbps AAC（Details 另接受 PCM 16/24/32-bit），44.1/48kHz。文本：参考名 64 / 事件名 30 / 短描述 50 / 长描述 120 字符。event 徽章（挑战/竞赛/首映/直播等）与状态（未开始/进行中/已结束）见 reference/in-app-events。
- **Product Page Optimization** ✅（create-product-page-optimization-tests/overview-of-product-page-optimization）：对现产品页测 **最多 3 个处理组**（app 图标、截图、预览三资产类型；第一轮"3 组未核实"已补）；变体随机分配给自定义用户群，可本地化；基于转化率估计提升量+置信算法判胜；要求 app 处于 Ready for Distribution；**不支持 CPP、watchOS/iMessage 页**；需 iOS/iPadOS 15+ 展示；角色 Account Holder/Admin/App Manager/Marketing。

## 8. 第一轮未核实项补齐

| 项 | 结论 | 来源 |
|---|---|---|
| TestFlight build 有效期 | ✅ "You can test a build for **up to 90 days**"；"Your build becomes unavailable for testers after 90 days." | /help/app-store-connect/test-a-beta-version/testflight-overview |
| TestFlight 其余数字复验 | ✅ 100 builds 共享、每测试者 30 设备、内部 100 人（限角色）、外部 10,000 人、外部首发需 Beta 审核 | developer.apple.com/testflight/ |
| App Clip 例外链接口径 | ✅ 四种经验/链接矩阵：default App Clip link（免服务器改动，支持 URL 参数，覆盖 Messages 链接/QR/Map/其他 app Link 调用）；关联网站（Smart App Banner+App Clip card，须 AASA）；**demo link**（允许**更大未压缩二进制**、无 URL 参数、Messages 中仅受限预览，须过审且 app 已上架）；advanced experience（全调用源+iOS 16.3 前全支持、每经验绑定 URL）。装母 app 后一切调用转母 app | /documentation/appclip/configuring-the-launch-experience-of-your-app-clip |
| SF Symbols 数量 | ✅ "a library of **over 7,000 symbols**"；9 字重×3 尺度；20+ 文字体系本地化；新符号随 iOS 27 / SF8（页面口径） | developer.apple.com/sf-symbols/ |
| Dynamic Type AX1–AX5 | ✅ 见 §4 表（AX1 28/44 → AX5 53/60 pt） | HIG typography Specifications |
| 企业计划配额 | ✅ 资格：**≥100 名员工**+法人实体+D-U-N-S+仅内部专用+验证面谈与持续评估+续期复审（$299/年，页面 Agreement 节）；设备数/证书数配额官方页面未列（未核实） | developer.apple.com/programs/enterprise/ |
| Ad Hoc 每会员年每类 100 台 | 未核实（现行帮助无专页；历史口径 100 台/会员年/类别） | — |
| widget 扩展内存上限 / containertool | 未核实（无公开数值；containertool 已退出现行文档） | — |

## 9. 性能指南（官方文档）

- **Hangs** ✅（/documentation/xcode/understanding-hangs-in-your-app + improving-app-responsiveness）：主线程/runloop 单通道模型（事件分发→handler→CA commit）；"A delay of less than **100 ms** in a discrete user interaction is rarely noticeable, but even a few hundred milliseconds can make people feel that an app is unresponsive"——**离散交互主线程同步工作预算 <100ms，且假设你只能占其中不到一半**；连续交互/帧更新：120Hz=每 8.3ms、60Hz=每 16.7ms 一帧，**主线程帧工作 >5ms 即需检视**；XCTest 性能基线建议 **100ms**（离散交互代码）/ **5ms**（数据源等连续交互代码）；不在主线程调度非必需工作（含 dispatch_async/await MainActor）。
- **Launch** ✅（/documentation/xcode/reducing-your-app-s-launch-time）：activation=launch|resume；**warm launch**（进程重开快）vs **cold launch**（重启后/被逐出内存后，框架守护进程重载）；launch time 口径=点图标→静态闪屏后**首屏绘制**（Xcode Organizer Launch Time pane，50/90 分位）；MetricKit 提供 `TimeToFirstDrawMetric` + `ApplicationResumeTimeMetric` 直方图；watchdog 超时即终止（iOS 无具体秒数公开——未核实）。
- **Memory** ✅（/documentation/xcode/reducing-your-app-s-memory-use + reduce-terminations-in-your-app）：度量=内存页数×页大小（**典型 16KB/页**，写 1 字节可涨 16KB）；**clean/dirty 之分，dirty 计入**；两指标=峰值内存+挂起时内存（Organizer 50/90 分位）；超限前发 `EXC_RESOURCE`（MEMORY 子类型）警告，超限即终止；挂起 app 可被移入固态存储（回前台变慢）。终止分类（reduce-terminations）：crash（SIGSEGV/SIGABRT 等）与**用户/系统正常退出**需区分统计。
- **Disk writes** ✅（/documentation/xcode/reducing-disk-writes）：过量写盘有专门指南（对齐 MetricKit DiskWriteExceptionDiagnostic/totalBytesWritten；Guidelines 2.4.2 明禁 excessive write cycles）。

## 10. 欧盟 DMA 现行条款（2026-10-01 生效版）

来源 ✅ https://developer.apple.com/support/apps-in-the-eu/（Changes for apps in the European Union）：

- **条款沿革**：2026-08-18 更新 ADPDA、**2026-10-01 生效统一条款**（Attachment 14）；旧 Alternative Terms Addendum 与 StoreKit External Purchase Link Entitlement (EU) Addendum 废止。
- **Core Technology Fee → Core Technology Commission**："The Core Technology Fee, a per-install fee…has been replaced by the Core Technology Commission, **a simple 5% commission on digital transactions in apps distributed outside the App Store**. The new terms also **eliminate the Initial Acquisition Fee and Store Services Fee**."（历史 €0.50/首年装 CTF 已死——第一轮"未逐字核实"已补为现行口径。）
- **现行佣金全表（EU 店面）**：

| 费目 | 费率 | 适用 |
|---|---|---|
| App Store commission（Apple IAP） | **26%** / 15% | 标准 / 小企业计划·Mini Apps·Video Partner、订阅次年起 |
| App Store commission（app 内替代支付） | **20%** / 10% | 标准 / 同上优惠档 |
| Store services commission（出 app 链接） | **15%** / 10% | 可操作链接引至 web/他市场/他 app，**仅链接点击后 7 天内成交计入** |
| **Core Technology Commission** | **5%** | 替代市场内、经替代市场分发的 app、Web Distribution 直发：付费 app+数字商品/服务（含订阅）；外链 7 天归因同样计入 |

  替代支付交易须**月报 15 天内**报送 Apple（含未成交交易）；小额市场运营者 CTC 豁免：近 12 月全球营收 **<€10M** 且市场下载/订阅费历史总收入 **<€1M**。
- **资格（市场运营与 Web Distribution 同表，满足任一）**：D&B 中等财务稳定性评分 / 上市或被上市公司持有 / 获成熟机构风投 / 持牌会计师完成财务审计 / 获费用豁免的政府·教育·非营利 / **USD 1,000,000 备用信用证** / **100 万全球首年安装**；**不再要求 EU 法人实体**（2026-10 新政）。
- **技术要求**：Notarization 基线审查（自动+人工）五维——准确性/功能性/安全性/安全（禁恶意软件、**禁下载可执行码**、**禁读容器外**、禁诱导降安全）/隐私；Apple 统一加密签名+安装单（开发者/描述/截图/分级）+安装校验+事后恶意软件拦截。市场运营走 ACC/API：注册市场+Developer ID、marketplace token、声明可替代分发 app、推送更新通知。市场分发 app 无 Apple IAP。
- **iOS 27.2 安装体验更新**：新安装流；开发者可从自家 app 内发起自家网站分发下载；**EU 用户出境 90 天内仍可装替代市场与其 app**。
- **儿童保护（替代支付）**：Kids 类 app 替代支付购买流必须家长门后且**禁出 app 购买链接**；<13 岁替代支付须家长门+禁外链；13–17 岁 app 内替代支付与外链均须家长门（成员国法定年龄更高时按更高值，如 16）。
- 其他：默认应用 12 类可设（浏览器/邮件/市场/非接触/电话/信息/密码管理器/键盘/来电垃圾过滤/导航/翻译等）；EU 可删 App Store、信息、相机、照片、Safari（仅设置与电话不可删）；HCE NFC entitlement（EEA 内店支付/车钥匙/门票等）；替代浏览器引擎（专用 browser app+app 内浏览）；互操作请求正式流程（25 万+ API 生态）。

## 11. 儿童相关：分级与家长同意

- **年龄分级体系已改版** ✅（https://developer.apple.com/help/app-store-connect/reference/app-information/age-ratings-values-and-definitions）：全球分级现为 **4+ / 9+ / 13+ / 16+ / 18+ / Unrated**（旧 4+/9+/12+/17+ 退役；页面含 "age-ratings-on-os-versions-earlier-than-26" 旧档锚点）。要点定义（每档"may contain…not suitable for children under the age of X"）：
  - **4+**：无不良内容；可含家长控制、年龄保证、UGC、消息聊天、广告、偶发竞赛。
  - **9+**：偶发粗口/低俗幽默、偶发恐怖主题、健康话题、偶发暗示主题、**偶发卡通/幻想暴力**、偶发枪械武器、**战利品箱**。
  - **13+**：社交媒体能力（<13 须禁用）、频繁粗口/恐怖、偶发烟酒药物、偶发医疗信息、偶发性内容、频繁卡通暴力、偶发写实暴力、频繁武器、偶发模拟赌博、频繁竞赛。
  - **16+**：** unrestricted web access**、频繁医疗信息、频繁暗示主题等。
  - **18+**：频繁烟酒药物、频繁性内容、频繁写实暴力、**真钱赌博**、频繁模拟赌博。
  - **Unrated**：图形性内容/长期血腥写实暴力——**不可上 App Store，可上替代市场或网站**。
  - 区域表：澳洲（16+/R18+ 因 loot box/模拟赌博上调）、巴西（MJSP 官方/自评 pictogram、真钱投注须 SPA 牌照）、韩国（GRAC 分级号，KR-19/Casino 17+/频繁模拟赌博等须上报）。
- **分级机制**：ACC 年龄分级问卷（内容描述符频次）→自动算全球分级+区域分级；分级为 app 级跨平台属性；**4+/9+ 可选 Made for Kids** 进儿童类目，过审后不可改、后续更新永久受儿童类规则约束（✅ set-an-app-age-rating）。visionOS 无儿童类目。
- **Kids 类目规则**（§1 引 1.3/5.1.4）：外链/购买/引流必须锁家长门后指定区；禁向三方发 PII/设备信息；禁三方分析/广告（有限例外：不采 IDFA/身份/位置且广告素材人工审核）；义务覆盖 COPPA/GDPR；"For Kids/For Children"字样保留给儿童类目。
- **Ask to Buy**（家长同意流程，用户侧机制）✅（https://www.apple.com/family-sharing/）："With Ask to Buy, you can make accidental or surprise purchases a thing of the past. **You'll get a notification every time your kids try to buy an app from the App Store, a show or movie on Apple TV, or a book on Apple Books. You'll then have the option to decline or accept the request**."（家人共享内组织者对儿童账户启用；搭配 Screen Time；开发者侧 SKQueue 等收据状态会出现 pending 状态——细则未在本次页面核到，未核实。）

## 12. 辅助功能官方要点

来源 ✅ https://developer.apple.com/design/human-interface-guidelines/accessibility（+ §4 typography）：

- **对比度（WCAG AA 口径，Accessibility Inspector 采用值）**：正文 **≤17pt（任意字重）最低对比 4.5:1**；**≥18pt 或任意加粗 3:1**；默认不达标则开启"提高对比度"时必须提供高对比配色；深浅模式都要校验。
- **文字放大**："give people the option to enlarge text by **at least 200 percent (or 140 percent in watchOS apps)**"；支持 Dynamic Type（iOS 支持至 AX5，见 §4）；最大字号下尽量保留同等信息量，避免截断（scroll 区可截但须可展开）。
- **VoiceOver**：所有可交互元素须可被朗读（label/hint/traits/自定义动作）；界面元素须恰当标注；配套 AssistiveTouch、Full Keyboard Access、Pointer Control、Switch Control 均需可用性验证（performing accessibility testing）。
- **色彩**：不得仅靠颜色传达信息（色盲红绿/蓝橙）；允许自定义配色（图表/角色）。
- **听觉**：字幕（caption 同步）/字幕翻译（subtitle）/口述影像三通道；关键信息不得仅以音频传达。
- **动效敏感**：响应 Reduce Motion（缩减自动/重复动画、缩放、外周运动；收紧 spring 弹跳、动效直接跟随手势）；响应 Dim Flashing Lights（视频闪烁检测与缓解）； motion 只作锦上添花不作唯一信息通道（配合触觉/音频反馈）。
- **语音**：Siri/快捷指令可自动化重复任务（Action 按钮/Home Screen 快捷方式）。
- 字号下限：各平台系统样式默认/最小值（iOS 17/11pt）见 §4；自定义字体细字重需加大尺寸补偿。
- 变更记录：2025-03 页面重构（Dynamic Type 移入 typography、VoiceOver 独立页）；2025-06 增 Assistive Access、Switch Control、Accessibility Nutrition Labels 指引。

## 主要来源清单（✅=本次直抓验证）

✅ App Review Guidelines 全文（765 行提取）：developer.apple.com/app-store/review/guidelines/
✅ 截图/预览/图标/本地化/字数：developer.apple.com/help/app-store-connect/reference/app-information/{screenshot,app-preview,creative-assets}-specifications、…/app-information、…/platform-version-information
✅ CPP/PPO/IAE：…/create-custom-product-pages/configure-multiple-product-page-versions、…/create-product-page-optimization-tests/overview-of-product-page-optimization、…/offer-in-app-events/{offer-in-app-events,overview-of-in-app-events}、…/reference/in-app-events/in-app-event-media-and-audio-specifications
✅ WidgetKit/LiveActivity：/documentation/widgetkit/keeping-a-widget-up-to-date、/documentation/activitykit/{displaying-live-data-with-live-activities,starting-and-updating-live-activities-with-activitykit-push-notifications}、HIG widgets/live-activities Specifications
✅ HIG：/design/human-interface-guidelines/{motion,materials,layout,app-icons,typography,accessibility,buttons}
✅ MetricKit：/documentation/metrickit 及各 MX* 指标/诊断子页（tutorials/data JSON）
✅ APNs：/documentation/usernotifications/{generating-a-remote-notification,sending-notification-requests-to-apns,pushing-background-updates-to-your-app,registering-your-app-with-apns}
✅ 性能：/documentation/xcode/{understanding-hangs-in-your-app,improving-app-responsiveness,reducing-your-app-s-launch-time,reducing-your-app-s-memory-use,reduce-terminations-in-your-app,configuring-your-app-icon}
✅ TestFlight：developer.apple.com/testflight/ + /help/app-store-connect/test-a-beta-version/testflight-overview
✅ App Clip：/documentation/appclip/configuring-the-launch-experience-of-your-app-clip
✅ EU DMA：developer.apple.com/support/apps-in-the-eu/
✅ 年龄分级：/help/app-store-connect/reference/app-information/age-ratings-values-and-definitions + …/set-an-app-age-rating
✅ 企业计划：developer.apple.com/programs/enterprise/；Ask to Buy：apple.com/family-sharing/
