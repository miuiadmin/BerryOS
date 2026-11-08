# Apple（iOS 16–18）应用规范对标底稿（A–M）

> **更正注（2026-11-08·第二轮补集）**：本文两处按补集更正——①年龄分级现为 4+/9+/13+/16+/18+/Unrated（旧 4+/9+/12+/17+ 已退役）；②2026-10 起欧盟条款 CTF 已被 Core Technology Commission 5% 取代。详见 `应用规范-Apple-iOS对标补深-2026-11.md` §10/§11。


> 调研方法：web_search 因凭证故障不可用，改用 web_fetch 直抓 developer.apple.com 官方文档（含官方文档 JSON 数据端点，内容=文档全文）。标 ✅ 的条目为本次直接抓取验证；标「未核实」的为知识库结论。抓取于 2026 年（官方页已含 2026-10 欧盟新条款，iOS 16–18 主体事实稳定）。供《BerryOS 应用规范 v1》对标用。

## A. 包结构与清单
- .ipa = ZIP 容器：必需 `Payload/<Bundle>.app/`；开发/Ad Hoc 分发另有 `SwiftSupport/`（Swift 运行库）；App Store 渠道由 Apple 重签。来源：https://developer.apple.com/documentation/bundleresources
- .app bundle：静态链接 Mach-O + Info.plist、PkgInfo(`APPL`)、`_CodeSignature/`、Frameworks/、Plugins/（扩展，各自独立 plist+签名）、Assets.car、`embedded.mobileprovision`（非商店分发时内嵌）。
- Info.plist 关键键（https://developer.apple.com/documentation/bundleresources/information-property-list）：
  - `CFBundleIdentifier` 反域名；`CFBundleName` ≤15 字符；`CFBundleShortVersionString`（X.Y.Z）与 `CFBundleVersion`（build 号，推荐三段非负整数；watchOS 配对 app 强制三段）；`MinimumOSVersion`。
  - `LSRequiresIPhoneOS=true`：禁止在 macOS（催化剂除外）运行 iOS 包。
  - `UIApplicationSceneManifest`：`UIApplicationSupportsMultipleScenes`（iPad 多窗）、`UISceneConfigurations`（delegate 类名/storyboard）；iOS 13+ UI 生命周期迁到 UIScene。
  - `UIBackgroundModes`（见 C）；`BGTaskSchedulerPermittedIdentifiers`（反向 DNS，任务 ID 必须以 bundle ID 为前缀，否则提交即报错）。
  - `NSExtension*`（见 L）：`NSExtensionPointIdentifier` + `NSExtensionAttributes` + activation rules。
  - `UILaunchScreen`（UIColorName/UIImageName，iOS 14+ 取代静态图）；隐私用途字符串键（见 B）全在 Info.plist。
- entitlements 文件（https://developer.apple.com/documentation/bundleresources/entitlements）：签名时嵌入二进制。常见：`aps-environment`(development|production)、`com.apple.security.application-groups`、`com.apple.developer.associated-domains`、`keychain-access-groups`(`TeamID.xxx`)、`com.apple.developer.usernotifications.critical-alerts`。双重校验：代码签名声明 ∩ provisioning profile 白名单，多出的 entitlement 直接拒装。
- provisioning profile（.mobileprovision = CMS 签名的 plist）：development / ad-hoc（含设备 UDID 表，每会员年每类型 100 台——未核实精确数）/ in-house（企业）/ App Store（无设备表）。开发证书 1 年有效；免费个人团队 profile 7 天过期、限 3 个 app（未核实精确配额）。
- bitcode：Xcode 14（2022）起弃用、默认不再生成，其后上传不再接受；现视为死特性（精确截止日期未核实）。
- **BerryOS 借鉴**：① .bap = cpio+TOML 对标 Info.plist，但 BerryOS 可做得更强——appd 安装时静态校验 manifest schema（用途串缺失=拒装），而非 Apple 的运行时 TCC 崩溃兜底。② entitlements∩profile 双校验值得照搬：manifest 声明能力 ⊆ 签名链授权能力，交集才是 appd 实际可授句柄集。③ CFBundleVersion 与营销版本双轨简单可靠，可直抄。

## B. 权限模型（TCC）
- 机制：TCC（Transparency, Consent & Control）由 `tccd` 守护进程管理，按「服务」授权（非按文件）；app 首次触发对应 API 时弹系统授权框；结果存每设备数据库，可在设置→隐私与安全性逐项改。来源：https://developer.apple.com/documentation/bundleresources/information-property-list
- 用途字符串键（purpose string）全表——缺键即用敏感 API 的后果：**app 直接崩溃**（TCC 拒绝 + crash，控制台报 "attempted to access privacy-sensitive data without a usage description"），且审核必拒：
  - 相机 `NSCameraUsageDescription`；麦克风 `NSMicrophoneUsageDescription`
  - 相册读写 `NSPhotoLibraryUsageDescription`；仅添加 `NSPhotoLibraryAddUsageDescription`（两键独立，读/写分离）
  - 定位 `NSLocationWhenInUseUsageDescription` / `NSLocationAlwaysAndWhenInUseUsageDescription`（iOS 11 起拆 Always 为二次升级弹）；临时精确定位 `NSLocationTemporaryUsageDescriptionDictionary`（iOS 15，按用途键值对）；`NSLocationDefaultAccuracyReduced`
  - 通讯录 `NSContactsUsageDescription`；日历 iOS 17 拆分 `NSCalendarsFullAccessUsageDescription`/`NSCalendarsWriteOnlyAccessUsageDescription`；提醒事项 `NSRemindersFullAccessUsageDescription`/`NSRemindersWriteOnlyAccessUsageDescription`（旧单键弃用）
  - 蓝牙 `NSBluetoothAlwaysUsageDescription`（iOS 13+，旧 NSBluetoothPeripheralUsageDescription 弃用）；本地网络 `NSLocalNetworkUsageDescription` + `NSBonjourServices`（iOS 14+，需列服务名）
  - 运动 `NSMotionUsageDescription`；健康 `NSHealthShareUsageDescription`/`NSHealthUpdateUsageDescription`；HomeKit `NSHomeKitUsageDescription`；媒体库 `NSMediaLibraryUsageDescription`/`NSAppleMusicUsageDescription`；语音识别 `NSSpeechRecognitionUsageDescription`；追踪 `NSUserTrackingUsageDescription`；面容 ID `NSFaceIDUsageDescription`
- 授权选项枚举：定位 iOS 13+ 三档「仅本次 Allow Once / 使用期间 While Using / 始终 Always」（仅本次次日清零）；照片 iOS 14+ 三档「完全访问 / 受限（选中的照片，可管理）/ 仅添加」；联系人 iOS 18 新增「选定的联系人」受限档（未核实文档原文）；其余多为二元 allow/deny。
- Local Network：独立弹窗；拒绝后该 app 全部本地网络（mDNS/单播发现）失效，无白名单细化。
- 精确定位：iOS 14 起用户可全局或按 app 降为模糊定位（approximate）。
- **BerryOS 借鉴**：① purpose string 机制直接对标「申请能力的用途说明」——BerryOS 可升级为 manifest 结构化字段（capability id → reason 文案 + reason code），appd 校验后才弹窗；② 照片三档证明「数据授权可以带范围句柄」——BerryOS 纯能力模型天然支持（授予"选中照片集合"的细分 handle）；③ Apple 缺字符串=运行时崩是反面教材：校验应前移到安装期。

## C. 生命周期与后台执行
- 状态机 5 态：Not Running / Inactive（瞬态：被通知中心/来电等遮挡）/ Active / Background（退后台后数秒~数十秒余量，`beginBackgroundTask` 可延长，无承诺值）/ Suspended（冻结：驻留内存但不调度，随时可被杀且无通知）。来源：https://developer.apple.com/documentation/uikit/managing-your-app-s-life-cycle
- Scene 生命周期：`sceneWillConnectToSession` → `sceneDidBecomeActive` ↔ `sceneWillResignActive`；`sceneWillEnterForeground` / `sceneDidEnterBackground`；多场景（iPad 同 app 多窗）由 `UIApplicationSupportsMultipleScenes` 声明。
- 后台模式 `UIBackgroundModes` 合法值（✅官方 Possible Values 全枚举）：`audio` / `location` / `voip` / `external-accessory` / `bluetooth-central` / `bluetooth-peripheral` / `fetch`（弃用→BGTaskScheduler）/ `processing`（弃用）/ `remote-notification`（静默推送）/ `nearby-interaction` / `network-authentication` / `push-to-talk` / `screen-capture` / `newsstand-content`（遗留）。声明即被审核追问用途，滥用拒审。来源：https://developer.apple.com/documentation/bundleresources/information-property-list/uibackgroundmodes
- Background App Refresh：用户可按 app 开关（并可分 Wi-Fi/蜂窝）；系统按充电、使用习惯、预算调度；静默推送（`content-available:1`）也受该开关与预算约束。
- BGTaskScheduler（iOS 13+，https://developer.apple.com/documentation/backgroundtasks）：`BGAppRefreshTask`（短任务，约 30 秒量级）与 `BGProcessingTask`（分钟级；可要求 `requiresExternalPower`/`requiresNetworkConnectivity`）；Info.plist `BGTaskSchedulerPermittedIdentifiers` 注册；`earliestBeginDate` 只是下限、时机完全系统决定；必须实现 expiration handler。
- 冻结与墓碑：Suspended 进程占内存不执行；Jetsam 按优先级 SIGKILL 回收（前台 > 扩展 > 挂起），app 无从感知；内存上限按设备机型不同（无公开常数）；磁盘紧张时系统可「卸载 app 保留数据」（Offload）。
- App Switcher 快照：退后台时系统截屏作为切换器缩略图；官方要求在快照前遮挡敏感信息（如余额/聊天），可用 `UIApplication.ignoreSnapshotOnNextApplicationLaunch`。来源：https://developer.apple.com/documentation/uikit/preparing-your-ui-to-run-in-the-background
- **BerryOS 借鉴**：① 「manifest 声明后台档位枚举 → 系统配额调度」与 BerryOS 路线完全同构：.bap manifest 声明 background class，appd 授对应调度能力，v1 枚举要少而正交（Apple 13 年攒出 14 个值、弃用 3 个的教训）；② 快照遮挡可做成系统强制（appd 在截图前注入遮罩）而非道德要求；③ 冻结而非杀=省电卖点，BerryOS 微内核 suspend 进程语义更干净。

## D. 自启动
- iOS 无 BOOT_COMPLETED 等价物：普通 app 不能开机自启、不能定时自我唤醒；设备启动后只有系统服务常驻。
- 合法「被唤醒」路径枚举：① VoIP push（PushKit：收到必须在 `didReceiveIncomingPushWith` 返回前上报 CallKit，否则系统杀 app 并**停止向其投递 VoIP push**——官方文档明文）；② 静默推送（remote-notification，受预算与用户开关）；③ BGTaskScheduler（时机系统定）；④ `audio`/`location` 等长驻后台模式；⑤ 位置显著变化监控唤醒；⑥ 本地通知（仅提示，不启进程）。来源：https://developer.apple.com/documentation/pushkit/responding-to-voip-notifications-from-pushkit
- 面向用户口径：iOS 把「app 能否活着」全部收归用户与系统——后台刷新、通知、定位、后台音频各有关卡；自启动不是 app 的权利，是系统意图（intent）的执行。
- **BerryOS 口径建议**：照此写死规范——.bap 无"开机自启"能力；一切后台执行=系统调度器持有的 intent 触发 + appd 预授的受限句柄，配额与开关全在系统 UI。这同时服务省电与 AI 原生调度叙事。

## E. 通知
- APNs：provider→APNs→设备二段投递；payload 上限 4KB（4096 字节，HTTP/2 API）；需 `aps-environment` entitlement；device token 可轮换。
- 授权（UNUserNotificationCenter）选项枚举：badge/sound/alert/carPlay/**provisional**（静默授权：通知直接进通知中心，用户可「保留/关闭」决定去留）/**criticalAlert**/**timeSensitive**/providesAppNotificationSettings；拒绝过授权框不能强行重弹。来源：https://developer.apple.com/documentation/usernotifications
- categories/actions：`UNNotificationCategory`（identifier+actions+options）+ `UNNotificationAction`/`UNTextInputNotificationAction`（文本回复）；锁屏/横幅/通知中心/CarPlay 一套声明全端呈现；每类可配 hiddenPreviews 行为。
- Critical Alerts（关键提醒）：绕过静音与专注模式；需特殊 entitlement + 向 Apple 单独申请批准（限医疗/安防等场景）；payload 声明 critical 声音 `{"critical":1,"name":"...","volume":0.0~1.0}`；HIG 要求仅真正危及安全/健康时使用。
- 专注模式（iOS 15+）：普通通知被压制或聚合；可穿透的仅 Time Sensitive（需授权+声明）与 Critical；通知摘要（Scheduled Summary）按时段打包非紧急通知。
- 呈现：锁屏可见性 per-app 可设（显示内容/隐藏预览，隐藏预览需认证后展示）；横幅点按深链入 app；通知完全由系统 UI 渲染，app 不自绘。
- **BerryOS 借鉴**：① 「通知=系统能力、统一渲染」在纯能力模型下天然成立：app 只拿到「发通知」句柄，内容渲染/去重/勿扰策略全在系统服务；② provisional 授权是降低弹窗压力的好设计；③ critical alert 的「entitlement+人工审批」双层，BerryOS 可用能力签名链+商店分级实现。

## F. UI 规范（HIG 要点）
- App 图标：商店资产 1024×1024px，PNG/JPEG，**无 Alpha 通道**、不带圆角（系统裁连续圆角 squircle）；iOS 18 起用户可自选深色/染色/清晰变体，开发者提供对应变体资产（Xcode 16 可由单张 1024 派生）；alternate 图标经 API 切换需包内全套尺寸。来源：https://developer.apple.com/design/human-interface-guidelines/app-icons
- 启动画面：必须 Launch Screen（storyboard 或 UILaunchScreen 字典）；仅允许静态布局（系统借位首帧），禁止当广告 splash。
- SF Symbols：系统矢量图标库，随 OS 版本演进（iOS 16→SF Symbols 4、17→5、18→6，6000+ 符号——数量未核实）；禁止导出单独分发；支持 9 档字重、可变颜色、多色分层。
- Dynamic Type 字阶（iPhone 默认 pt）：largeTitle 34 / title1 28 / title2 22 / title3 20 / headline 17(半粗) / body 17 / callout 16 / subheadline 15 / footnote 13 / caption1 12 / caption2 11；辅助功能档 AX1–AX5 可放大至 ~200%+，布局须自适应。来源：https://developer.apple.com/design/human-interface-guidelines/typography
- 触摸目标：最小可点击区域 **44×44pt**（HIG Buttons/Layout；视觉尺寸可小但命中区不得小于 44）。
- 安全区：Safe Area 覆盖刘海/灵动岛/Home Indicator（`safeAreaInsets`）；灵动岛区域由系统 Live Activities 呈现，普通 app 无绘制 API，只需避让。
- 深色模式：semantic colors（systemBackground/secondarySystemBackground、label/secondaryLabel、separator…）+ 资产目录明暗变体；禁止硬编码黑白两套。
- 偏好与无障碍：VoiceOver（accessibilityLabel/hint/traits）；Increase Contrast、Reduce Transparency（减毛玻璃层叠）、Reduce Motion（转场改 crossfade）、Differentiate Without Color；Dynamic Type 全适配是质量要求。来源：https://developer.apple.com/design/human-interface-guidelines/accessibility
- 键盘与输入：键盘进程边界=系统所有，app 只接收提交结果，无法旁路采集；自定义键盘是独立扩展（见 L）；文本框以 keyboardType/autocorrectionType 等声明式提示；外接键盘 keyCommands。
- 屏幕适配：@2x/@3x 位图或 PDF 矢量；size classes（compact/regular）；iPad：Split View/Slide Over、多窗口（`UIApplicationSupportsMultipleScenes`）、Stage Manager（M 芯片/大内存机型，窗口自由缩放+外接屏）。
- **BerryOS 借鉴**：① 44pt/字阶表/语义色三件套是可直接写进规范的可执行硬指标（数值全公开）；② 「图标只交 1024 方图、圆角/暗色由系统派生」把碎片尺寸问题消灭在源头，BerryOS 应照抄（单一 icon 资产+系统渲染）；③ 无障碍作为上架质量门槛而非附加项。

## G. 桌面组件
- WidgetKit（iOS 14+，SwiftUI 声明式 + TimelineProvider 快照序列）：尺寸档位枚举 `systemSmall`/`systemMedium`/`systemLarge`（iPhone/iPad）+ `systemExtraLarge`（**仅 iPad**）；锁屏/手表 `accessoryCircular`/`accessoryRectangular`/`accessoryInline`（iOS 16 锁屏、watchOS、StandBy）；刷新由系统预算分配（社区实测约 40–70 次/日，无官方数——未核实）。来源：https://developer.apple.com/documentation/widgetkit
- Interactive widgets（iOS 17+）：仅 `Button`/`Toggle` 两类控件，经 App Intent 在 widget 进程内执行（不拉起主 app）；动画由系统自动插值。
- App Clips（iOS 14+）：✅官方尺寸档（未压缩二进制）：iOS 15 及更早 **10MB** / iOS 16 **15MB** / iOS 17+ **100MB**（100MB 须：仅数字触发、不支持 NFC/QR/App Clip Codes 物理触发、可靠网络场景、且仅支持 iOS 17+；demo 链接可用 100MB 档）。触发：NFC、App Clip Codes、QR 码、Safari 智能横幅、地图、信息。受限清单（✅抓取）：不能请求 ATT 追踪、无任何后台模式/后台网络、定位仅 when-in-use 且**次日凌晨 4:00 自动重置**、不能注册自定义 URL scheme、不能包含扩展（可带 widget 发 Live Activity）、`identifierForVendor`/设备名返回空、数据只可与母 app 共享。来源：https://developer.apple.com/documentation/appclip/choosing-the-right-functionality-for-your-app-clip
- 快捷指令集成：App Intents（见 L）；App Shortcuts 经 `AppShortcutsProvider` 静态注册，装好即出现在快捷指令 App/Siri，无需用户手建。
- **BerryOS 借鉴**：① widget「不拉进程、持 intent 句柄即交互」是能力模型典范——BerryOS 可让组件持受限 handle 由系统托管渲染；② App Clip 的"瞬时档"可映射为 .bap 的轻量子集（限定能力白名单+自动过期），其 4:00 重置式临时授权是好语义；③ 尺寸档随 OS 反复变（10→15→100MB）是教训：BerryOS 若做轻量档，v1 冻结常量入 ADR。

## H. 系统应用/预装
- 不可卸载（核心，随版本微调——完整名单未核实）：App Store、相机、信息、电话、照片、设置。来源：https://support.apple.com/en-us/HT208094（现跳转 101264）
- 可卸载（约 28 个；卸载=删图标+其数据与关联设置，可随时从商店重装）：计算器、日历、时钟、指南针、通讯录、FaceTime、文件、查找、健身记录、健康、家庭、iTunes Store、邮件、地图、测距仪、音乐、新闻、备忘录、播客、提醒事项、快捷指令、股市、提示、TV、翻译、语音备忘录、手表、天气。
- 默认应用选择器：iOS 14 起可换默认浏览器/邮件（需对应 entitlement）；iOS 18 汇总为设置→App→「默认 App」页，后续版本逐步扩展电话/短信/翻译等（区域差异大、欧盟先行——具体矩阵未核实）。
- 系统应用免权限口径：Apple 自家 app 被长期指出享受 TCC 豁免/系统级数据访问（2020 年美国众议院反垄断报告、Facebook 信件等争议），Apple 口径为「系统功能，非 app 数据收集」；无公开机制让第三方取得同等权限。
- **BerryOS 借鉴**：① 「系统应用同走能力模型、少量豁免清单公开」应写成 BerryOS 卖点，避免 Apple 式双重标准；② 可卸载≠可缺失：核心链路（设置/商店）保留、其余皆可换默认——BerryOS v1 可定 6–8 个"骨架 app"。

## I. 隐私合规
- 隐私营养标签（App Privacy Details，2020-12 起）：三类声明——Data Used to Track You / Data Linked to You / Data Not Linked to You；数据类型枚举（位置/联系人/标识符/使用数据/敏感信息/财务…）；「收集」与「追踪」定义区分；随版本更新强制。来源：https://developer.apple.com/app-store/app-privacy-details/
- ATT（iOS 14.5+）：跨 app 追踪必须先 `requestTrackingAuthorization` 弹窗；键 `NSUserTrackingUsageDescription`；拒绝后 IDFA=0、声明为 tracking 的域名请求直接失败；未弹窗即追踪→拒审（准则 5.1.1）。来源：https://developer.apple.com/documentation/apptrackingtransparency
- App 隐私报告（iOS 15.1+，设置→隐私与安全性）：按 app 记录最近 7 天的传感器与数据访问（照片/联系人/位置）+ 全部网络域名（含 SDK 域名）。
- 剪贴板：iOS 14 起后台读剪贴板触发状态栏提示；iOS 16 起程序化读字符串触发「允许粘贴」弹窗（设置可按 app 设允许/询问/拒绝）；`UIPasteControl` 一次授权统一粘贴。
- PHPicker（iOS 14+）：`PHPickerViewController` 系统进程外照片选择器，**零相册权限**即可选图；`selectionLimit`（0=不限）；旧 UIImagePicker 弃用。来源：https://developer.apple.com/documentation/photokit/phpickerviewcontroller
- 指示灯：任意 app 使用摄像头→状态栏绿点、麦克风→橙点，控制中心可查最近使用方；不可关闭。
- Lockdown Mode（iOS 16+，https://support.apple.com/en-us/HT212650）：极端保护档——消息附件类型封禁、Web JIT/部分特性关、有线配对锁定、禁装描述文件；iOS 17 起可按域设例外；另有 Sensitive Content Analysis/敏感内容警告（iOS 17，本地裸露检测，`SCSensitivityAnalyzer` 可检测自家内容）。父任务提到的「iOS 17 Domain: sensitivity」未核实所指，最贴近上述二者。
- **BerryOS 借鉴**：① PHPicker 模式（系统选择器替代库级权限）应成为 BerryOS 默认——picker 发一次性句柄，能力模型下实现成本极低；② 粘贴弹窗证明剪贴板可收敛为「按次租借能力」；③ 营养标签可平移为 .bap manifest 的数据用途声明字段+商店渲染，机器可查。

## J. 分发与商店
- 审核指南要点（https://developer.apple.com/app-store/review/guidelines/）：4.2 最低功能、不得仅是套壳网站/WebView 应用；2.5.1 私有 API 零容忍、公共 API 不得绕权限弹窗；2.5.2 禁止下载可执行代码；1.2 隐私政策 URL 强制；1.3 儿童类（无行为广告、购买需家长门、数据最小化）；5.1.1/5.1.2 收集须知情同意且与声明用途一致；4.5.4 推送不得滥用营销；3.1 数字商品支付走 IAP（外链规则区域化）。
- TestFlight（✅官方页，https://developer.apple.com/testflight/）：内部测试 **100 人**（限团队角色成员）；外部 **10,000 人**；同时可共享 **100 个 build**；每测试者最多 **30 台设备**；外部首发 build 需过 Beta 审核；build 有效期 90 天（未核实）。
- 企业分发（ADEP）：in-house 证书签名、无商店审核；仅限本企业员工使用，公开分发会被吊销证书（历史上发生过大规模 revocation）。
- 欧盟 DMA（✅官方页，https://developer.apple.com/support/apps-in-the-eu/）：2024 起（iOS 17.4+）开放替代应用市场、Web Distribution（官网直发）、替代支付、NFC/浏览器引擎（受限）；市场运营/官网直发资格：D&B 财务评分/上市或被上市公司控股/风投/审计/政府教育非营利/100 万美元备用信用证/百万级年装机任一即可。2026-10 新条款：统一商业条款、5% Core Technology Commission 取代 Core Technology Fee（历史上 17.4 曾为 €0.50/首年装机 CTF——未逐字核实）。
- Notarization for iOS（✅官方页）：所有替代分发渠道统一基线审查（自动+人工）：准确性/功能性/安全性/隐私四类（不得含已知恶意软件、不得读容器外、不得诱导降安全设置）；Apple 统一加密签名 + 安装前展示安装单（开发者/描述/截图/分级）+ 安装校验 + 事后恶意拦截。App Store 审核 = Notarization + 更高内容/商业标准。
- **BerryOS 借鉴**：① 「notarization=安全基线（机器可查）+ 商店=内容/商业策略」双层分离架构值得照抄，BerryOS 签名链从第一天就同权支持官方仓库与第三方仓库；② 私有 API 零容忍在纯能力模型下自然成立——无句柄即无法调用，审核退化为能力清单比对；③ TestFlight 数字可抄：100/10000/90 天。

## K. 数据与存储
- 沙箱容器：每 app 唯一 home `…/<UUID>/Data/{Documents, Library/{Application Support, Caches, Preferences}, tmp}`；bundle 只读；无任何全局共享路径；照片/通讯录/文件均经系统 broker 进程而非直接文件授权。
- iCloud 备份规则：Documents 与 Library（多数）默认入备份；可再生成内容应设 `isExcludedFromBackup`（`NSURLIsExcludedFromBackupKey`）；Caches/tmp 不备份且系统可清；删除 app=容器整体删除（Offload 保留容器）。
- 文件数据保护类：`NSFileProtectionComplete`（解锁才读）/ `CompleteUnlessOpen` / `CompleteUntilFirstUserAuthentication`（首解后可读，默认）/ `None`；Keychain 对应 `kSecAttrAccessibleWhenUnlocked/AfterFirstUnlock/…ThisDeviceOnly` 系列。
- App Group：`group.<teamID>.<id>`；共享容器目录 + UserDefaults suite + 可共享 Keychain；主 app/widget/扩展经同一 entitlement 加入；这是 iOS 唯一合法的 app 间共享数据机制（同一开发者内）。
- Keychain：securityd 托管，按 access group 隔离（TeamID 前缀）；**卸载 app 后 keychain 项保留，重装可读**；跨设备随加密备份迁移（ThisDeviceOnly 除外）。
- 剪贴板：全局单例 `UIPasteboard.general`，无权限模型，靠 iOS 14/16 提示与弹窗约束；后台读取受限。
- **BerryOS 借鉴**：① App Group 可映射为「组能力 token：appd 签发并核验各成员签名链」，比 Apple 的 entitlement 方案更显式；② 「数据保护类」四个枚举简明正交，可直接平移为 BerryOS 容器加密档位；③ keychain 卸载保留语义要写明，避免"删不干净"事故。

## L. 跨应用互操作
- URL scheme：`CFBundleURLTypes` 声明；**冲突无仲裁**（多 app 注册同名 scheme 时行为未定义——未核实）；探测他家 app 须 `LSApplicationQueriesSchemes` 白名单（约 50 个上限——未核实）；审核禁止抢注他人 scheme。
- Universal Links（✅官方抓取，https://developer.apple.com/documentation/xcode/supporting-associated-domains）：https 域名 + `https://<域名>/.well-known/apple-app-site-association`（无扩展名、有效证书、无重定向）；JSON 结构 `applinks.details[].appIDs=["TeamID.BundleID"]` + `components` 路径匹配（`/` 路径、`?` 查询项（支持 `????` 定长通配）、`#` 锚点、`exclude` 排除）；entitlement `applinks:example.com`（子域各需独立条目；`*.` 通配仅限非 appclips 服务）；iOS 14 起 AASA 经 Apple CDN 拉取（**24 小时内抓取、设备约每周复查**；开发期 `?mode=developer` 直连私域）。验证通过后链接直达 app，长按可选"打开网站"。
- Share/Action Extension：扩展点 `com.apple.share-services`；activation rules 谓词（禁 `TRUEPREDICATE`）；内存上限远低于普通 app（数十 MB 级，具体未公开——未核实）；与宿主经 NSItemProvider（UTI 类型）交换数据。
- 自定义键盘扩展：扩展点 `com.apple.keyboard-service`；`RequestsOpenAccess=true` 需用户授予 Full Access 才有网络/共享容器（附隐私提示）；无 Full Access 时强沙箱；必须保留切换回系统键盘的地球键；采集按键数据→拒审。来源：https://developer.apple.com/documentation/uikit/creating-a-custom-keyboard
- **App Intents（iOS 16+，声明式意图模型——重点）**（https://developer.apple.com/documentation/appintents）：`@AppIntent` 结构体（`perform()`）+ `@Parameter`（类型安全、支持 Entity/动态 Query 结果集）；`AppShortcutsProvider` 静态注册短语——系统可离线索引、零用户配置即出现在 Siri/快捷指令/Spotlight；iOS 18 引入 `AssistantSchemas`（Apple Intelligence 语义槽位域：照片/邮件/电子表单等），Siri 可跨 app 组合调用；同一意图被 widget 按钮、控制中心控件、Live Activity 复用；老 SiriKit 域框架被其逐步替代。
- **BerryOS 借鉴**：① App Intents 是「声明式意图+系统中介」最成熟样本，与 BerryOS intent(u8) 一等公民调度直接对口：意图应作为 .bap manifest 静态字段（appd 可校验、系统 UI 可直调），而非运行时注册；② 「静态声明→系统可索引」是 Siri/快捷指令生态成功的根因，BerryOS 照此即得 AI 友好化；③ AASA 的"域名↔身份"强绑定是深链安全的基础，BerryOS 深链应内建同级别验证。

## M. Apple 特有补充
- Widget 交互限制：仅 Button/Toggle 两类控件（无滚动/文本输入/视频/地图）；交互经 App Intent 在扩展进程执行，不唤主 app；动画系统插值、禁自定义过渡曲线。
- Live Activities（iOS 16.1+，ActivityKit，https://developer.apple.com/documentation/activitykit）：锁屏卡片 + 灵动岛；更新经本地 ActivityKit 或远程 Live Activity push（push type `liveactivity`）；时长规则：活跃期最长 8 小时，结束后锁屏停留最长 4 小时（合计 12h 上限——未逐字核实）；灵动岛形态：compact（leading/trailing 两段）/minimal（圆点）/expanded；审核要求有明确结束时刻、禁持续监控类用途。
- 控制中心控件（iOS 18，ControlWidget）：与 WidgetKit 同源（`ControlWidgetButton`/`ControlWidgetToggle` + `ControlWidgetTemplate`）；可放控制中心、锁屏快捷位、Action Button；经 App Intent 切换状态、可推送更新；每 app 可提供多个控件，添加时用户逐个授权。
- Apple Pencil：PencilKit/PKCanvasView；低延迟墨迹（预测采样）、压感/倾角/悬停（M2 iPad Pro+，约 9ms 悬停延迟——未核实）；Scribble 手写转文本（作为间接文本输入，多语言）；Pencil 2 双击手势交 app 自行解释。来源：https://developer.apple.com/documentation/pencilkit
- 隐私清单（✅官方抓取，https://developer.apple.com/documentation/bundleresources/privacy-manifest-files）：`PrivacyInfo.xcprivacy`；顶层键 `NSPrivacyTracking`（bool）/ `NSPrivacyTrackingDomains`（未获追踪授权时这些域名请求**直接失败**）/ `NSPrivacyCollectedDataTypes`（收集数据类型）/ `NSPrivacyAccessedAPITypes`（required-reason API：UserDefaults、文件时间戳、磁盘空间、系统启动时间、活跃键盘等类别，每类必须填 reason code 如 CA92.1）；2024 年春起上传商店强制；「常用三方 SDK 清单」（数百个）必须自带 manifest+签名。
- name/图标 A/B 限制：App Store Connect 产品页优化（PPO）可对图标/截图/描述做 A/B（最多 3 个实验组——未核实）；app 名称频繁更换/关键词堆砌违反 2.3.7 拒审；iOS 18 主屏图标暗/染色是用户侧选择，非开发者实验渠道。
- **BerryOS 借鉴**：① required-reason API 是「拿能力必须声明理由码、机器可审」的制度化——BerryOS 每次授予 handle 都带 reason code + 写审计日志，直接内建；② NSPrivacyTrackingDomains「未授权则域名请求失败」证明网络层能力可以声明式管控（BerryOS net 服务持域名白名单句柄）；③ Live Activity 的 8h+4h 硬时限说明"持续型 UI 也要有配额"，BerryOS 常驻卡片类能力应同样限时。

## BerryOS 若照抄会踩的坑（8 条）
1. **用途字符串靠运行时崩溃兜底**：Apple 缺键=crash。BerryOS 应安装期 schema 校验，manifest 缺 reason 字段直接拒装。
2. **权限语义双轨制**：TCC 在系统层、数据在 broker 服务层，Apple 靠特例缝合。BerryOS 起步就应「一切数据=服务=能力」，不留第二套通道。
3. **后台枚举只增不改**：14 个 UIBackgroundModes、3 个已弃用、语义漂移。BerryOS 后台档位要少而正交且版本冻结（入 ADR）。
4. **弹窗疲劳**：数十个二/三态弹窗导致用户无脑允许。BerryOS 用 intent 场景收敛：一次授权按场景复用，弹窗计数进系统指标。
5. **系统应用双标**：自家免 TCC 是监管与信任负债。系统应用同模型、豁免清单公开且常量冻结。
6. **尺寸/时限档反复变动**：App Clip 10→15→100MB、Live Activity 时限多次调整。BerryOS 轻量档与配额在 v1 定为常量，变更走升版。
7. **中心化重签依赖**：欧盟 Notarization 一切替代分发仍经 Apple 签名/安装校验，中心化单点。BerryOS 签名链设计即同权支持第三方仓库，官方仓库只是策略层。
8. **意图运行时注册**：老 SiriKit 靠 donate 运行时学习、效果差，App Intents 靠静态声明胜出。BerryOS 意图表必须是 .bap manifest 静态字段（编译期可查、安装期可验、系统可索引）。

---
主要来源（除各节内联）：✅=本次直接抓取验证
- ✅ UIBackgroundModes：https://developer.apple.com/documentation/bundleresources/information-property-list/uibackgroundmodes
- ✅ TestFlight：https://developer.apple.com/testflight/
- ✅ 欧盟替代分发/Notarization：https://developer.apple.com/support/apps-in-the-eu/
- ✅ AASA/Universal Links：https://developer.apple.com/documentation/xcode/supporting-associated-domains
- ✅ App Clip 尺寸/限制：https://developer.apple.com/documentation/appclip/choosing-the-right-functionality-for-your-app-clip 、https://developer.apple.com/documentation/appclip/creating-an-app-clip-with-xcode
- ✅ 隐私清单：https://developer.apple.com/documentation/bundleresources/privacy-manifest-files
- Info.plist/entitlements：https://developer.apple.com/documentation/bundleresources/information-property-list 、https://developer.apple.com/documentation/bundleresources/entitlements
- 生命周期/后台：https://developer.apple.com/documentation/uikit/managing-your-app-s-life-cycle 、https://developer.apple.com/documentation/backgroundtasks
- 通知/推送：https://developer.apple.com/documentation/usernotifications 、https://developer.apple.com/documentation/pushkit/responding-to-voip-notifications-from-pushkit
- HIG：https://developer.apple.com/design/human-interface-guidelines/
- 审核/隐私/分发：https://developer.apple.com/app-store/review/guidelines/ 、https://developer.apple.com/app-store/app-privacy-details/ 、https://support.apple.com/en-us/HT212650
