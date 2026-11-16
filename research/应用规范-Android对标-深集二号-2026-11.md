# 应用规范·Android 对标·第二轮深集二号（数值与清单补深）

> 采集：2026-11-08（子代理二号·Android 深集）。方法：curl/web_fetch 直抓 developer.android.com / source.android.com / support.google.com / m3.material.io / developers.google.com / AOSP(aosp-mirror@GitHub) 官方页与官方源码；m3.material.io 官网纯 JS 渲染不可读，字阶数值取自 androidx 官方 tokens 源码（Material 3 官方实现）。
> 消费面：`docs/应用规范-v1.0.md`；不重复第一轮（`research/应用规范-Android对标-2026-11.md`）已核实条目。纯事实档，无建议。

## 1. Material 3 完整字阶（30 档：15 标准 + 15 Emphasized）

来源（数值）：androidx Compose M3 `TypeScaleTokens.kt`（VERSION 31.0.11，官方 tokens）https://raw.githubusercontent.com/androidx/androidx/androidx-main/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/tokens/TypeScaleTokens.kt ；角色名来源：https://m3.material.io/styles/typography/type-scale-tokens（官网需 JS，本次未能直读，数值经 androidx 官方实现交叉核实）

| 样式 | 字重 | 字号 sp | 行高 sp | 字距 sp |
|---|---|---|---|---|
| DisplayLarge | Regular | 57 | 64 | -0.25* |
| DisplayMedium | Regular | 45 | 52 | 0 |
| DisplaySmall | Regular | 36 | 44 | 0 |
| HeadlineLarge | Regular | 32 | 40 | 0 |
| HeadlineMedium | Regular | 28 | 36 | 0 |
| HeadlineSmall | Regular | 24 | 32 | 0 |
| TitleLarge | Regular | 22 | 28 | 0 |
| TitleMedium | Medium | 16 | 24 | 0.2 |
| TitleSmall | Medium | 14 | 20 | 0.1 |
| BodyLarge | Regular | 16 | 24 | 0.5 |
| BodyMedium | Regular | 14 | 20 | 0.2* |
| BodySmall | Regular | 12 | 16 | 0.4 |
| LabelLarge | Medium | 14 | 20 | 0.1 |
| LabelMedium | Medium | 12 | 16 | 0.5 |
| LabelSmall | Medium | 11 | 16 | 0.5 |

*实测 androidx tokens：DisplayLarge=-0.2sp、BodyMedium=0.2sp（m3 规范表历史版本为 -0.25/0.25，以 androidx 实现为准记录）。

Emphasized 变体（同一 15 档，字重+字距变化，同源）：Display/Headline 全系 Medium+0 字距；TitleLargeEmph=Medium/0；TitleMedium/SmallEmph=**Bold**（0.15/0.1）；Body 全系 Medium（0.15/0.25/0.4）；Label 全系 **Bold**（0.1/0.5/0.5）。字号/行高与标准档相同。

## 2. 权限全集（危险权限 / 权限组 / special app access）

- 保护级别分类（normal/signature/runtime(dangerous)/special(appop)）：https://developer.android.com/guide/topics/permissions/overview
- **危险权限全集 42 条**（protectionLevel 含 dangerous，AOSP main `core/res/AndroidManifest.xml`）：https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/main/core/res/AndroidManifest.xml
  ACCESS_BACKGROUND_LOCATION(dangerous|instant)、ACCESS_COARSE_LOCATION、ACCESS_FINE_LOCATION、ACCESS_MEDIA_LOCATION、ACCEPT_HANDOVER、ACTIVITY_RECOGNITION、ANSWER_PHONE_CALLS、BLUETOOTH_ADVERTISE、BLUETOOTH_CONNECT、BLUETOOTH_SCAN、BODY_SENSORS、BODY_SENSORS_BACKGROUND、CALL_PHONE、CAMERA、GET_ACCOUNTS、NEARBY_WIFI_DEVICES、POST_NOTIFICATIONS、PROCESS_OUTGOING_CALLS（已弃用）、RANGING、READ_CALENDAR、READ_CALL_LOG、READ_CELL_BROADCASTS、READ_CONTACTS、READ_EXTERNAL_STORAGE、READ_MEDIA_AUDIO、READ_MEDIA_IMAGES、READ_MEDIA_VIDEO、READ_MEDIA_VISUAL_USER_SELECTED、READ_PHONE_NUMBERS、READ_PHONE_STATE、READ_SMS、RECEIVE_MMS、RECEIVE_SMS、RECEIVE_WAP_PUSH、RECORD_AUDIO、SEND_SMS、USE_SIP、UWB_RANGING、WRITE_CALENDAR、WRITE_CALL_LOG、WRITE_CONTACTS、WRITE_EXTERNAL_STORAGE
- **官方权限组 15 个**（`Manifest.permission_group` 参考，API 36 在册）：ACTIVITY_RECOGNITION / CALENDAR / CALL_LOG / CAMERA / CONTACTS / LOCATION / MICROPHONE / NEARBY_DEVICES / NOTIFICATIONS / PHONE / READ_MEDIA_AURAL / READ_MEDIA_VISUAL / SENSORS / SMS / STORAGE。https://developer.android.com/reference/android/Manifest.permission_group
- **重要事实**：AOSP 10 起 dangerous 权限在 platform manifest 中 `permissionGroup` 一律为 `UNDEFINED`（组映射离开 manifest，运行时另管），manifest 层不再可推导"组→权限"映射。同上 AOSP 源。
- 历史"组→权限"官方表（API 28 时代文档存档，user-facing Permission Manager 分类至今沿用）：CALENDAR=READ/WRITE_CALENDAR；CALL_LOG=READ/WRITE_CALL_LOG+PROCESS_OUTGOING_CALLS；CAMERA=CAMERA；CONTACTS=READ/WRITE_CONTACTS+GET_ACCOUNTS；LOCATION=FINE/COARSE；MICROPHONE=RECORD_AUDIO；PHONE=READ_PHONE_STATE+READ_PHONE_NUMBERS+CALL_PHONE+ANSWER_PHONE_CALLS+ADD_VOICEMAIL+USE_SIP；SENSORS=BODY_SENSORS；SMS=SEND/RECEIVE/READ_SMS+RECEIVE_WAP_PUSH+RECEIVE_MMS；STORAGE=READ/WRITE_EXTERNAL_STORAGE。来源存档：https://web.archive.org/web/20181101000000/https://developer.android.com/guide/topics/permissions/overview （现行页此表已删除）
- 新增组归属（官方零散页，并入上表）：NEARBY_DEVICES(31+)=BLUETOOTH_SCAN/ADVERTISE/CONNECT+NEARBY_WIFI_DEVICES+UWB_RANGING；READ_MEDIA_VISUAL(33+)=READ_MEDIA_IMAGES/VIDEO+READ_MEDIA_VISUAL_USER_SELECTED；READ_MEDIA_AURAL(33+)=READ_MEDIA_AUDIO；NOTIFICATIONS(33+)=POST_NOTIFICATIONS。https://developer.android.com/about/versions/12/behavior-changes-12 + /about/versions/13/behavior-changes-13
- 用户侧 Permission Manager 类目（15 项，现行）：Calendar、Call logs、Camera、Contacts、Files、Health/Wellness/Fitness（A15- 显示为 Body Sensors）、Location、Microphone、Music and audio、Nearby devices、Notifications、Phone、Photos and videos、Physical activity、SMS。https://support.google.com/android/answer/9431959
- **special app access**（定义：须用户进 Settings>Apps>Special app access 手动开，无弹窗，protectionLevel=appop）：https://developer.android.com/training/permissions/requesting-special （页面只举 3 例：exact alarms / overlay / all files access，无官方全表——以下为 AOSP 源码核实清单）
  - AOSP `appop` 保护级权限 22 条（AOSP 14 manifest，含组合标志）：MANAGE_EXTERNAL_STORAGE、MANAGE_MEDIA、SYSTEM_ALERT_WINDOW、WRITE_SETTINGS、SCHEDULE_EXACT_ALARM、USE_FULL_SCREEN_INTENT、REQUEST_INSTALL_PACKAGES、PACKAGE_USAGE_STATS、LOADER_USAGE_STATS、ACCESS_NOTIFICATIONS、INTERACT_ACROSS_PROFILES、MANAGE_ONGOING_CALLS、MANAGE_IPSEC_TUNNELS、TURN_SCREEN_ON、SMS_FINANCIAL_TRANSACTIONS、MEDIA_ROUTING_CONTROL、INSTANT_APP_FOREGROUND_SERVICE、FOREGROUND_SERVICE_SPECIAL_USE、USE_ICC_AUTH_WITH_DEVICE_IDENTIFIER、RUN_BACKUP_JOBS、RECEIVE_SANDBOX_TRIGGER_AUDIO、RECEIVE_SANDBOXED_DETECTION_TRAINING_DATA。https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/android-14.0.0_r67/core/res/AndroidManifest.xml
  - Settings "special access" 独立屏（AOSP Settings 源码 fragment）：deviceadmin、pictureinpicture、interactacrossprofiles、zenaccess(DND)、notificationaccess、premiumsms、vrlistener、turnscreenon。https://raw.githubusercontent.com/aosp-mirror/platform_packages_apps_Settings/android-14.0.0_r67/AndroidManifest.xml

## 3. App Widget 尺寸与限制数值

来源页（现行）：https://developer.android.com/develop/ui/views/appwidgets/layouts

- 官方估算公式（典型 5×4 网格手机，Pixel 4 实测口径）：**portrait = (73n−16) × (118m−16) dp；landscape = (142n−15) × (66m−15) dp**（n=宽格数、m=高格数）。
- 官方格数→dp 表（portrait/landscape）：1×1=57×102 / 127×51；2×1=130×102 / 269×51；3×1=203×102 / 412×51；4×1=276×102 / 554×51；5×1=349×102 / 697×51；5×2=349×220 / 697×117；5×3=349×337 / 697×184；5×4=349×455 / 697×250。
- 旧公式 n×70dp−(n−1)×8dp：现行文档已不载（未在现行页核实到；40dp/110dp/180dp/250dp 旧表亦同，标注：现行文档已移除）。
- A12+ 默认尺寸用 `targetCellWidth/Height`（格数，API 31+），A11- 用 minWidth/minHeight（dp）；二者同时声明时 A12+ 取格数。AppWidgetProviderInfo：https://developer.android.com/reference/android/appwidget/AppWidgetProviderInfo
- resize 区间：minResizeWidth/Height ↔ maxResizeWidth/maxResizeHeight；示例 180~530dp×110~450dp=3×2~5×2 cells。官方示例小=3×2、中=5×2。layouts 页（同上）+ overview 页 https://developer.android.com/develop/ui/views/appwidgets/overview
- `updatePeriodMillis`：系统最短 30 分钟才投递一次（AppWidgetProviderInfo 参考页原文）。
- cells 数值上限：官方未公布 targetCellWidth/Height 的最大值上限（网格上限由 launcher 决定，多数手机 5×4）——未核实到官方数值。
- RemoteViews 位图内存上限（Android 17 行为变更）：每 widget `6 × width × height` 字节（system_server 强制）。https://developer.android.com/develop/ui/views/appwidgets
- A12+ 支持有状态组件（CheckBox/Switch/RadioButton，仍无状态需 app 自存）；圆角 radius 系统 API（A12+）。同上。

## 4. 隐式广播豁免白名单（manifest 注册，A8+ 全量逐条）

来源：https://developer.android.com/guide/components/broadcast-exceptions （现行 URL：/develop/background-work/background-tasks/broadcasts/broadcast-exceptions）

1. ACTION_LOCKED_BOOT_COMPLETED、ACTION_BOOT_COMPLETED（开机一次性）
2. ACTION_USER_INITIALIZE、USER_ADDED、USER_REMOVED（特权权限保护）
3. TIME_SET、ACTION_TIMEZONE_CHANGED、ACTION_NEXT_ALARM_CLOCK_CHANGED
4. ACTION_LOCALE_CHANGED
5. USB：ACTION_USB_ACCESSORY_ATTACHED/DETACHED、ACTION_USB_DEVICE_ATTACHED/DETACHED
6. 蓝牙：BluetoothHeadset.ACTION_CONNECTION_STATE_CHANGED、BluetoothA2dp.ACTION_CONNECTION_STATE_CHANGED、ACTION_ACL_CONNECTED、ACTION_ACL_DISCONNECTED
7. 电话：ACTION_CARRIER_CONFIG_CHANGED、TelephonyIntents.ACTION_*_SUBSCRIPTION_CHANGED、TelephonyIntents.SECRET_CODE_ACTION、ACTION_PHONE_STATE_CHANGED、ACTION_PHONE_ACCOUNT_REGISTERED/UNREGISTERED
8. LOGIN_ACCOUNTS_CHANGED_ACTION（已弃用）
9. ACTION_ACCOUNT_REMOVED
10. ACTION_PACKAGE_DATA_CLEARED
11. ACTION_PACKAGE_FULLY_REMOVED（注意 PACKAGE_REPLACED 等不豁免）
12. ACTION_NEW_OUTGOING_CALL（已弃用）
13. ACTION_DEVICE_OWNER_CHANGED
14. ACTION_EVENT_REMINDER（日历 provider）
15. 存储：ACTION_MEDIA_MOUNTED/CHECKING/UNMOUNTED/EJECT/UNMOUNTABLE/REMOVED/BAD_REMOVAL
16. SMS_RECEIVED_ACTION、WAP_PUSH_RECEIVED_ACTION

（第一轮记"约 15 组"，本轮按页逐条核清为 16 组，如上。）

## 5. Android 15/16：16KB page size、predictive back、edge-to-edge

- **16KB page size**（https://developer.android.com/guide/practices/page-sizes）：
  - Android 15 起 AOSP 支持以 16KB 页大小配置的设备；含 NDK 代码（直接或经 SDK）的 app 须重编译适配；纯 Java/Kotlin 无需改动。
  - **Play 要求：targeting Android 15 (API 35)+ 的 app 须支持 16KB（64 位）；2027-02-01 起不支持 16KB 的更新将无法发布。**
  - 技术要求：ELF LOAD 段 `(VirtAddr+MemSiz)` 16KB(0x4000) 对齐；APK zipalign `-P 16`；`zipalign -v -c -P 16 4` 校验。官方性能数据：内存压力下启动时间平均 ↓3.16%（部分 app 至 30%）、启动功耗 ↓4.56%、相机热启 ↓4.48%/冷启 ↓6.60%、开机时间 ↓8%（约 950ms）。
- **predictive back**：
  - Android 13 须 manifest `android:enableOnBackInvokedCallback="true"` 选择加入；**target 36 (Android 16)+ 在 Android 16+ 设备默认启用** predictive back 系统动画（back-to-home/cross-task/cross-activity），onBackPressed 与 KEYCODE_BACK 不再派发；可 `enableOnBackInvokedCallback=false` 临时退出（application 或 activity 级）。https://developer.android.com/about/versions/16/behavior-changes-16 + https://developer.android.com/guide/topics/manifest/activity-element
  - 自定义动画：AndroidX Activity 1.8.0-alpha01+ Progress API（handleOnBackStarted/Progressed/Cancelled/Pressed）；平台侧 OnBackAnimationCallback（A14 DP1+）。https://developer.android.com/about/versions/14/features/predictive-back
- **edge-to-edge**：
  - target 35 在 Android 15+ 强制启用；target 35 可 `windowOptOutEdgeToEdgeEnforcement` 退出，**target 36 起该 opt-out 属性在 Android 16 设备上废弃并禁用（不可退出）**。https://developer.android.com/develop/ui/views/layout/edge-to-edge + behavior-changes-16（同上）
  - insets 三类：systemBars（可点内容避让）、displayCutout（挖孔）、systemGestures（手势区）；ViewCompat.setOnApplyWindowInsetsListener + WindowInsetsCompat.Type.systemBars()；enableEdgeToEdge() 默认透明系统栏（3 键导航下导航栏半透明 scrim）。

## 6. SplashScreen / 自适应图标 / 通知全参数

**SplashScreen**（https://developer.android.com/develop/ui/views/launch/splash-screen + /migrate）：
- A12 起冷启动与温启动均显示系统 splash（热启动不显示）；默认=launcher icon+纯色 windowBackground。
- 图标尺寸：无 icon 背景 **288×288dp、内切圆直径 192dp**；有 icon 背景 **240×240dp、内切圆 160dp**；icon 背景=可选（增强对比）；**branded image 200×80dp**。
- 动画图标：推荐 **≤1000ms**（可更长但不推荐）；AVD 尺寸须为自适应图标的 4 倍：全幅 432dp（=4×108dp），可见内区 288dp（=4×72dp）。A13+ windowSplashScreenBehavior=icon_preferred（默认 0=仅静态图标）。
- 参数：windowSplashScreenBackground / AnimatedIcon / AnimationDuration / IconBackgroundColor；退出可 setKeepOnScreenCondition 延时、退出动画 onExitAnimationListener；A12+ 全面替代自绘 splash Activity。

**自适应图标**（https://developer.android.com/develop/ui/compose/system/icon_design_adaptive，旧 URL /guide/practices/ui_guidelines/icon_design_adaptive 同内容）：
- 彩色版**两层**（foreground+background，vector 优先），theming 需**额外一层 monochrome**（单层）；三层均 108×108dp；掩码视口 72dp 内"安全区"**66×66dp**；logo ≥48dp 且 ≤66dp；每边外围 18dp 保留给 mask/视差。
- XML：`<adaptive-icon>` 含 `<foreground>/<background>/<monochrome>`。A13+ 主题化取色；Android 16 QPR2 起无 monochrome 层的 app 图标系统自动主题化。

**通知**（https://developer.android.com/develop/ui/compose/notifications/create-notification + /expanded + /progress-centric；渠道/importance/trampoline 见第一轮）：
- 动作按钮 **≤3 个**（addAction）；A10+ 系统可自动生成建议回复/动作（setAllowGeneratedReplies/​setAllowSystemGeneratedContextualActions 可关）。
- 展开样式：BigTextStyle（长文）、BigPictureStyle（大图，可缩略图换 largeIcon）、InboxStyle（**addLine 最多 6 行**，超出只显示前 6）、MessagingStyle（对话：addMessage(text,time,Person)、setConversationTitle 仅群聊；A24+，compat 自动降级；setContentTitle/Text 被忽略）。
- 进度条：setProgress(indeterminate/确定值)；A16 新模板 **Notification.ProgressStyle**（Point/Segment 表示里程，用于打车/外卖/导航，置顶+更高可见性）。
- 声音限速：**每秒最多 1 条**发声音（多通知仅首条有声）。
- A13+ POST_NOTIFICATIONS runtime 权限（dangerous|instant，AOSP manifest 同 §2）。
- 通知 parcel 大小上限：官方文档未公布固定数值（未核实）。
- 每 app 活跃通知配额（第一轮未核实项）：AOSP `NotificationManagerService.MAX_PACKAGE_NOTIFICATIONS` = **A11:25 → A13+:50**（当前 main=50）；另 DEFAULT_MAX_NOTIFICATION_ENQUEUE_RATE=5/s、MAX_PACKAGE_TOASTS=5。https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/main/services/core/java/com/android/server/notification/NotificationManagerService.java （A11 值核对 android-11.0.0_r1 同文件）

## 7. Play 政策深挖

**数据安全表单**（https://support.google.com/googleplay/android-developer/answer/10787469）：
- 全员必填（含封闭/开放/生产测试轨道；仅 internal 测试豁免；系统服务/私有 app 豁免）；不含收集也须填+挂隐私政策。
- 表单结构：Data collection（"Collect"=传出设备，含三方 SDK/webview；ephemeral 处理可不在前台披露但需申报；纯端上处理与端到端加密不计）→ Data sharing（"Share"=传第三方，service provider/法律义务/用户主动/匿名化豁免）→ 每数据类型 optional/required → 安全实践：**传输加密 + 数据删除请求机制**；徽章：Families 承诺 / MASA 独立安全评审（OWASP MASVS）/ UPI（印度）。
- 数据类型 14 类：Location(近似≥3km²/精确<3km²)、Personal info(9)、Financial info(4)、Health and fitness(2)、Messages(3)、Photos and videos(2)、Audio(4)、Files and docs、Calendar、Contacts、App activity(4)、Web browsing、App info and performance(3)、Device or other IDs。
- 用途 7 枚举：App functionality / Analytics / Developer communications / Advertising or marketing / Fraud prevention, security, and compliance / Personalization / Account management。

**权限申报与演示视频**：
- Permissions Declaration Form 七步：核对权限→声明核心功能→给审查说明→**必须提交演示视频**（YouTube 链接或 mp4 云存储链接）→受限内容访问凭据→旧 APK 例外→确认；审核可达数周。https://support.google.com/googleplay/android-developer/answer/9214102
- 需 Console 声明（多含视频）的受限权限/政策全集（https://support.google.com/googleplay/android-developer/answer/16558241 "Permissions and APIs that Access Sensitive Information"）：
  - SMS/Call Log 组：须为默认 SMS/Phone/Assistant handler 才能声明使用；
  - 后台位置 ACCESS_BACKGROUND_LOCATION（专页 9799150：核心功能+显著收益+视频演示+显著披露+隐私政策，禁纯广告/分析用途；FGS 定位须用户发起且即完即止）；
  - MANAGE_EXTERNAL_STORAGE（A11+，SAF 不足时+声明）；
  - READ_MEDIA_IMAGES/VIDEO（A13+，Photo Picker 不足时+声明）；
  - QUERY_ALL_PACKAGES（broad visibility 限互操作核心用途）；
  - AccessibilityService API（需演示视频+声明；isAccessibilityTool 判定；禁绕安全/录音/自主决策）；
  - REQUEST_INSTALL_PACKAGES（限核心功能：浏览器/文件传输/企业/备份/迁移/配对）；
  - BODY_SENSORS/BODY_SENSORS_BACKGROUND 及 A16+ 细粒度 health.*（人工审查用例）；
  - Health Connect 数据类型。
  - 已排期变更：2027-01-27 起 target A17(API37+) 用 ACCESS_FINE_LOCATION 须声明"为何粗定位/位置按钮不足"+联系人改 Contact Picker；2027-04-14 AppFunctions 欺骗 AI 禁令；2027-08-25 Manage Contacts/Local Network 声明化。
- **Families 政策**（https://support.google.com/googleplay/android-developer/answer/9893335）：目标含儿童须 COPPA/GDPR 合规；纯儿童 app 禁传 AAID/SIM Serial/Build Serial/BSSID/MAC/SSID/IMEI/IMSI、A33+ 不应请求 AD_ID、禁精确定位、CDM 请求蓝牙；仅认证 ad SDK+禁个性化广告；混合受众须中性年龄门；社交功能须成人动作门；AR 须安全提醒。
- **Payments/金融**（https://support.google.com/googleplay/android-developer/answer/10281818）：数字商品/云服务内购须 Google Play 结算；例外=实物/物理服务/信用卡还款/水电费/P2P/博彩；保险/证券/投顾/报税、受监管临床医疗、政府数字证书豁免；app 内不得引导站外支付（§3/8/9 例外除外），站外沟通自由；reader/consumption-only app 允许。
- **App Identity（Impersonation 政策）**（https://support.google.com/googleplay/android-developer/answer/9888374）：禁冒充其他开发者/公司/app；禁暗示未授权关联；图标/标题不得近似既有产品误导；"Official"命名须有权利；禁违反 Android 品牌规范。

## 8. Android Vitals 全指标（定义+阈值+统计窗）

主表：https://developer.android.com/topic/performance/vitals ；子页 /google/play/vitals/*（crash/anr/…）；Reporting API（统计窗口）：https://developers.google.com/play/developer/reporting

**坏行为阈值总表**（主表页）：

| Vital | Overall | Per phone model | Per watch model |
|---|---|---|---|
| User-perceived crash rate | 1.09% | 8% | 4% |
| User-perceived ANR rate | 0.47% | 8% | 5% |
| Excessive battery usage | 1% | - | 1% |
| Excessive partial wake locks | 5%（>5% sessions/28 天） | - | - |

- 核心三族：Stability（crash+ANR）/Battery（wake locks+表盘耗电）/Memory（Memory usage=匿名 RSS+Swap、Bitmap memory usage；**超内存/位图/代码优化阈值 2027-02 起影响商店可见性**）。
- **crash**（/google/play/vitals/crash）：未捕获异常(Throwable)或未处理信号(SIGSEGV)；user-perceived=活跃使用（显示 activity 或跑 FGS）时崩溃；Wear OS 含后台。DAU=单日单设备去重。多崩溃率=≥2 次/DAU。统计窗=Play Console 按 28 天滚动（API 另有 7d/28d 用户加权）。
- **ANR**（/anr）：user-perceived ANR **仅含 "Input dispatching timed out" 类型**；DAU 口径同上。
- **excessive partial wake locks**（/excessive-wakelock）：24h 窗内后台+FGS 持有 partial wake lock 合计 ≥2 小时=excessive（audio/location/JobScheduler user-initiated 豁免）；>5% 电池会话（28 天）触发坏行为。
- **stuck partial wake locks**（/stuck-wakelock）：24h 内出现 ≥1 次后台持锁 ≥1 小时=stuck。
- **excessive wakeups**（/wakeup + Reporting API）：**>10 次 AlarmManager 唤醒/小时**（按用户计百分比）；DAILY 聚合（America/Los_Angeles 时区），7d/28d 用户加权滚动均值。
- **slow rendering**（/render）：slow frame >16ms（60fps 基准）；**frozen frame >700ms**（View 体系才有统计；Compose 非 View 工具包不采集，adb dumpsys gfxinfo 可查）。
- **slow startup**（/launch-time，TTID 口径）：**冷启 ≥5s / 温启 ≥2s / 热启 ≥1.5s** 判为过度。
- **slow sessions**（/slow-session，游戏）：帧呈现距上帧 >50ms（20FPS）为慢帧；另有 34ms（30FPS）目标指标；将引导用户远离达不到 20FPS 的游戏；仅活跃会话监控。
- **LMK**（/lmk）：user-perceived LMK 终止率 >1%=需立即处理。
- **bg Wi-Fi scans**（/bg-wifi）：后台 >4 次/小时=excessive；battery session 定义 A10=两次充电区间聚合、A11+=固定 24h。
- **bg network usage**（/bg-network-usage）：后台收发合计 **>50MB/天**=excessive。
- **excessive battery usage**（/excessive-battery-usage，表盘）：>4.44%/小时=超标（目标 <3.2%/小时）；效率要求：前台服务 ≤90s/小时、partial wake lock ≤18.5s/小时。
- **Memory usage**（主表页，Apps/游戏分表 ×RAM 档×app 状态）：如 Apps 4GB 档 Foreground 2.00GB/Services 1.00GB；16GB 档 4.25/2.00/2.00GB；Bitmap：Foreground - / Services 200MB / Background 200MB / Cached 400MB。
- **DEX code optimization**（主表页）：DEX>10MB 的 app 与 >50MB 的游戏，优化/混淆/缩减率须 **>25%**（"keep above"阈值）。

## 9. Core App Quality / 大屏（现为 Adaptive）App Quality

- **Core App Quality**（https://developer.android.com/docs/quality-guidelines/core-app-quality）：分区=Standards compliance→User experience(Usability/User interface/Visual quality/Navigation/Notifications/Accessibility)→Functionality(Audio/Video/Sharing/Background services)→Performance and stability(Performance/Stability/SDK)→Battery→Privacy and security(Permissions/Data and files/Backup/Identity/App components/Networking/WebViews/Execution/Cryptography)→Google Play(Policies/App details/User support)+Test environment+StrictMode+Tests 映射表。关键量化条目：
  - 触控目标 ≥48dp；对比度大字/图形 3:1、小字(<18pt 或 <14pt bold) 4.5:1；行宽 45–75 字符；启动 >2s 须反馈；渲染 16ms/60fps；视频编码 HEVC；音频 1 秒内起播或示忙；后台音频须 FGS+常驻通知；PiP（视频 app）；系统 Sharesheet；StrictMode 无红闪；SSL 全流量+网络安全配置；AAB（2021-08 起新 app 强制）；Auto Backup+dataExtractionRules；Credential Manager/passkeys；exported 显式声明；禁非 SDK 接口；生产包禁 debug 库。
  - 测试环境最低组合：7.6" 折叠（含外屏）、Pixel C 9.94"(2560×1800)、Wear OS Round 1.84"；StrictMode detectAll()+penaltyFlashScreen()。
- **大屏清单已升级为 Adaptive App Quality**（https://developer.android.com/docs/quality-guidelines/adaptive-app-quality ，旧 large screen app quality 于 2026-01-21 归档并入）：
  - 三档：**Tier 3 Adaptive ready**（全屏不 letterbox、关键流程可用、外设基本支持）/ **Tier 2 Adaptive optimized**（全屏尺寸布局优化）/ **Tier 1 Adaptive differentiated**（多任务/折叠姿态/拖放/手写笔）。
  - Tier 3 条目（/tier-3）：Config_Changes（无溢出/不 letterbox/配置变更保状态：滚动位、输入、播放续）、Config_Combinations、Multi-Window_Functionality、Multi-Resume（失焦仍更新 UI，处理相机/麦克风独占丢失）、Camera_Preview、Media_Projection、Keyboard_Input、Mouse_Trackpad_Basic（click/select/scroll）、Stylus_Basic、Stylus_Text_Input（A14 手写输入默认支持）。
  - 测试设备最低集：Googlebook(160ppi)、Foldable(841×701dp)、8" 平板(1024×640dp)、10.5" 平板(1280×800dp)、13" Chromebook(1600×900dp)。

## 10. 启动指标与 Baseline Profiles

- 冷/温/热定义：冷=进程从零创建；温=子集（进程在但 activity 重建等）；热=最快（activity 全在，仅走 onRestart）。https://developer.android.com/topic/performance/vitals/launch-time
- **TTID**（Time To Initial Display）：首帧呈现，含冷启进程初始化+activity 创建；系统自动上报。**TTFD**（Time To Full Display）：完全可交互；须 app 调 `reportFullyDrawn()` 上报。
- Vitals 过度阈值（TTID 口径）：冷 ≥5s / 温 ≥2s / 热 ≥1.5s（见 §8）。
- **Baseline Profiles**（https://developer.android.com/topic/performance/baselineprofiles/overview）：ART 按其 AOT 预编译指定路径，**首启提速约 30%**（多家实测）；无 baseline 时解释执行+JIT，空闲时后台写 odex；Startup profiles（编译期优化 DEX 布局）与之配合推荐同时使用；Cloud Profiles 单独依赖不够快。

## 11. WorkManager / AlarmManager / JobScheduler 约束数值

- **WorkManager**（https://developer.android.com/develop/background-work/background-tasks/persistent/getting-started/define-work + androidx reference）：
  - PeriodicWorkRequest 最小周期 **15 分钟**（MIN_PERIODIC_INTERVAL_MILLIS，文档明示"与 JobScheduler 相同"）；flex 区间最小 **5 分钟**（MIN_PERIODIC_FLEX_MILLIS；flex 窗=[interval−flex, interval]）。
- **AlarmManager**（https://developer.android.com/training/scheduling/alarms）：
  - inexact 批量投递；setWindow() 窗口 **<600000ms(10min) 会被截到 10min**（系统至少推迟 10min 交付 window 型非精确闹钟）；setInexactRepeating 同理窗口化。
  - exact（setExact/setAlarmClock 等）：A12+ 需 SCHEDULE_EXACT_ALARM（A14 新装默认拒绝，见第一轮）；**setAndAllowWhileIdle/setExactAndAllowWhileIdle 每 app 每 9 分钟至多 1 次**。
- **Doze**（https://developer.android.com/training/monitoring-device-state/doze-standby）：静止+熄屏进 Doze：挂起网络、忽略 wake lock、推迟标准闹钟（setExact/setWindow → 下个维护窗）、停 Wi-Fi 扫描/sync adapter/JobScheduler（含 WorkManager）；setAlarmClock 例外并提前退出 Doze；维护窗随时间拉长变稀。豁免：用户在 Settings>Battery>Battery Optimization 手动豁免；Play 政策禁非核心功能申请直接豁免（可豁免用例表：无法用 FCM 的 IM/VOIP、安全 app、任务自动化、外设 companion 等）。
- **JobScheduler**（https://developer.android.com/reference/android/app/job/JobInfo.Builder）：setPeriodic 最小周期 15 分钟（getMinPeriodMillis 强制）；flex ≥ max(getMinFlexMillis, 5% period)；expedited job 仅可设网络/存储不低/持久化约束，配额内保底执行 **≥1 分钟**；S+ 前台 app expedited 保证同步启动（T+）。配额由 App Standby 桶决定（见 §12）。
- Widget 更新周期 ≥30min（§3）。

## 12. 第一轮未核实项补深

- **App Standby Bucket 限制数值表**（https://developer.android.com/topic/performance/power/power-details ；近似值官方注明可变）：

| 桶 | Regular jobs | Expedited jobs | Alarms | Network |
|---|---|---|---|---|
| Active | 60min 滚动窗内 ≤20min（A16 前无限额） | 24h 内 ≤30min | 无限 | 无限制 |
| Working set | 4h 滚动 ≤10min | 24h ≤15min | ≤10/小时 | 无限制 |
| Frequent | 12h 滚动 ≤10min | 24h ≤10min | ≤2/小时 | 无限制 |
| Rare | 24h 滚动 ≤10min | 24h ≤10min | ≤1/小时 | 禁用 |
| Restricted | 1 次/天 ≤10min | 24h ≤5min | 1 次/天（精确或不精确） | 禁用 |

  - A13 起桶不再决定高优先级 FCM 配额；A16 起 job 配额再按桶/top 状态/FGS 调整（行为变更）。仅电池供电生效、充电不限（第一轮已核）。
- **auto-reset/hibernation 时限**（https://developer.android.com/topic/performance/app-hibernation）：官方措辞为 **"a few months"（数月）未交互进入 hibernation，精确月数未公布**（未核实到官方数值）。效果按 target 分层：target A12+ 且设备 A12+：权限重置+禁后台 job/闹钟+停推送（含高优先级 FCM）+清缓存（近似 force-stop）；A11 设备：仅权限重置；A6.0–A10+Play services：2021-12 起权限重置。算作"使用"：activity resumed、widget 交互、通知交互（划走不算）、他 app/OS 绑定其 service/provider、收到显式广播；job/隐式广播/闹钟不算。豁免：不在 launcher 的 app、work profile app、DPC、运营商特权 app、三方应用商店。
- **cached app freezer**：官方专页已下线（旧 URL /topic/performance/cache-cached-app-freezer 404；Android 11 behavior-changes-all 与 source.android.com power 系列页均无）。AOSP 源码核实（https://raw.githubusercontent.com/aosp-mirror/platform_frameworks_base/main/services/core/java/com/android/server/am/CachedAppOptimizer.java）：cached 进程 cgroup 冻结；DEFAULT_FREEZER_DEBOUNCE_TIMEOUT=**10s**（入冻去抖）；binder 冻结参数 DEFAULT_FREEZER_BINDER_ENABLED=true、DIVISOR=4、OFFSET=500、THRESHOLD=1000（异步 binder 待处理超阈值提前回调冻结）、CALLBACK_THROTTLE=10s。
- **每 app 活跃通知限额**：见 §6（25→50，AOSP 源码）。
- **Private space**（https://developer.android.com/about/versions/15/features）：独立 user profile；设备锁或独立锁；锁定=profile 停止（前后台活动全停、通知不显）；launcher 独立容器，锁定时对 recents/通知/设置/他 app 隐藏；媒体与账号隔离；系统 sharesheet/photo picker 在解锁时可跨空间给内容；不能迁入既有 app，只能新装副本；需重点自查：按 work profile 假设分支的 app、医疗、launcher、应用商店、Selected Photos Access 最近选择。
- **Photo Picker**（https://developer.android.com/training/data-storage/shared/photo-picker）：
  - 零权限；单选 PickVisualMedia（半屏）；多选 PickMultipleVisualMedia(N)，平台最大可选数 **设备相关，经 `MediaStore.getPickImagesMaxLimit()` 查询**（固定默认值官方未公布——未核实）；EXTRA_PICK_IMAGES_MAX 须为 >1 且 ≤该上限，否则 RESULT_CANCELED；设备不可用时回退 ACTION_OPEN_DOCUMENT 并忽略上限。
  - 可用性：A11+ 经 Google System Updates（Mainline）模块下发，A13+ 内建；云媒体 provider 支持远端选择。视频转码：长视频 >1 分钟不自动转码。
  - A14+ Selected Photos Access（READ_MEDIA_VISUAL_USER_SELECTED）与最近选择高亮（§5 特性页）。

## 13. 文本与本地化

- **per-app language**（https://developer.android.com/guide/topics/resources/app-languages）：A13+ 系统设置内逐 app 语言（LocaleConfig 自动生成）；API=setApplicationLocales/getApplicationLocales（LocaleManager），与系统设置双向同步；Auto Backup 会存语言选择；AndroidX AppCompat 回移（AppCompatActivity context）；推荐以系统设置为准、in-app picker 走同 API。
- **RTL**（https://developer.android.com/training/basics/supporting-devices/languages）：manifest `android:supportsRtl="true"`；布局属性 left/right → start/end（target<17 时 RTL 镜像不生效）；双向文本（数字/标点开头、嵌套反向文本）需用格式化 API 处理方向。
- 字符串规范：默认 strings.xml+locale 目录（res/values-<locale>/）；行宽 45–75 字符可读性要求（§9）；伪本地化测试（pseudolocale，第一轮语种外补充未单独核页）。Google Play 要求 app 名称≤30 字符、简短描述≤80、完整描述≤4000（商店图 guid，官方图形规范页本轮未抓，未逐字核实——标注）。

## 14. App 内更新 / Play Asset / Feature Delivery

- **In-app updates**（https://developer.android.com/guide/playcore/in-app-updates + /kotlin-java）：两种流——Flexible（后台下载安装+状态监听）/ Immediate（全屏强制更新重启）；数据收集：设备元数据+版本号+模块列表（加密、不分三方）。参数：clientVersionStalenessDays() 陈旧天数；inAppUpdatePriority 0–5（Play Developer API Edits.tracks 设置，只升不改，取安装版→最新版最大优先级）；AppUpdateOptions.AllowAssetPackDeletion（空间不足时允许删 asset pack）。
- **Play Feature Delivery**（https://developer.android.com/guide/playcore/feature-delivery）：feature module（com.android.dynamic-feature 插件）三种模式：install-time/conditional（按设备特性交付）/on-demand（运行时请求）；签名/minify/versionCode 只在 base 模块。
- **Play Asset Delivery**（https://developer.android.com/guide/playcore/asset-delivery）：>200MB 游戏替代 OBB；三种模式 install-time（随装、算入商店大小、不可删）/fast-follow（装后自动拉、不算）/on-demand（运行时；instant app 仅支持 on-demand）；install-time 需 2 倍 asset 总量空闲空间，fast-follow/on-demand 仅需数百 MB 额外空间；纹理压缩格式定向按格式分别计限。
- **尺寸档**（https://support.google.com/googleplay/android-developer/answer/9859372，压缩下载尺寸口径）：

| 组件 | 下载尺寸上限 |
|---|---|
| Base module | 500MB |
| 单个 feature module | 500MB |
| 单个 asset pack | 1.5GB |
| 全部模块+install-time asset packs 累计 | 4GB |
| on-demand/fast-forward asset packs 累计 | 30GB |
| app 压缩下载总上限 | **34GB** |

  - >1GB 须 minSdk≥21；feature module 建议上限 100 个（minSdk≥26）/50 个（更低 minSdk）；asset pack ≤100 个/AAB；>200MB 移动数据安装弹非阻断提示；仍发 APK 的沿用旧限 100MB。

## 附：本轮未核实项清单（诚实标注）

- m3.material.io 原站数值（JS 渲染不可直读，以 androidx 官方 tokens 源码代替；TitleMedium/BodyMedium 字距两处与历史规范表有 ±0.05sp 差异）。
- widget targetCellWidth/Height 官方最大格数上限（无公开数值）。
- 通知 parcel/单条尺寸上限（无公开数值）。
- auto-reset 精确月数（官方仅 "a few months"）。
- Photo Picker 平台默认 max limit 固定值（仅 API 查询）。
- cached app freezer 官方专页（已下线，用 AOSP 源码数值）。
- Play 商店图文字数上限（本轮未抓图形规范页）。
