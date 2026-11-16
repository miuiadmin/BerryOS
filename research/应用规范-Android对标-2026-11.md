# 对标底稿：Android（13–15）应用规范全景（A–M 全维度）

> 采集：2026-11-08（应用规范立法调研·子代理底稿一号）。
> 方法注记：web_search 工具当时认证失败，改用 curl/web_fetch 直接抓取 developer.android.com、source.android.com、m3.material.io、support.google.com 官方页（40+ 页）核实，事实均有来源 URL；未核实处已标注。
> 消费面：`docs/应用规范-v1.0.md` 对标矩阵与各节证据。

## A. 包结构与清单
- APK=ZIP：classes.dex、res/、assets/、lib/<abi>/、META-INF/ 签名、resources.arsc、二进制 AndroidManifest.xml；AAB 为发布格式、APK 生成延迟到商店。https://developer.android.com/guide/app-bundle
- Manifest 必须声明四大组件（activity/service/receiver/provider，不声明系统无法启动）、uses-permission、permission（自定义）、uses-feature、intent-filter；application 级 icon/label 为组件默认值。https://developer.android.com/guide/topics/manifest/manifest-intro
- AAB 强制：2021-08 新 app、2023-06 TV；>200MB 走 Play Feature/Asset Delivery。https://developer.android.com/guide/app-bundle
- compileSdk=编译用 API；minSdk=最低运行版（挡安装）；targetSdk=行为开关（系统按它门控行为变更）。https://developer.android.com/studio/publish/versioning
- Play targetSdk 政策：2026-08-31 起新 app/更新 target≥36；存量须 ≥35 否则只对旧 OS 用户可见；可延至 2026-11-01。https://support.google.com/googleplay/android-developer/answer/11926878
- 借鉴：.bap TOML 对标 manifest 三段声明；v1 即留 spec_version 做"行为按版本门控"；用商店 targetSdk 下限倒逼生态。

## B. 权限模型
- 保护级别：install-time（normal 低风险/signature 同证书）→runtime（dangerous 弹窗）→special（appop 级，设置内开关，如 SYSTEM_ALERT_WINDOW）；全集另有 knownSigner/vendorPrivileged/internal（未逐项核实原页）。https://developer.android.com/guide/topics/permissions/overview
- 运行时弹窗（位置/麦克风/相机）：Allow while using（仅前台）/Only this time（一次性：activity 可见期可用、进程死即失效、两次拒绝后封堵）/Don't allow。https://developer.android.com/training/permissions/requesting
- 权限组：同组合并弹窗；分组可跨版本变，勿硬编码。
- auto-reset：数月未用自动撤销权限+停后台+清缓存+停通知（"Pause app activity if unused"，用户可单 app 关；精确月数未核实，社区观察 3–6 个月）。https://support.google.com/android/answer/6271667
- Photo Picker：零权限访问所选照片/视频；A13 内建、旧版回移；A14 加"仅选部分照片"模式。https://developer.android.com/media/mediastore/photopicker
- Special app access：overlay/所有文件/使用情况/安装未知应用/精确闹钟/DND/通知访问等用户开关。App Ops=底层"操作即审计"（AOSP 文档页已下线）。
- 借鉴：while-in-use/only-this-time/auto-reset 三档时间语义与能力模型天然契合（句柄租约/TTL）；photo picker 证明"选择器即权限"路线应 v1 采用。

## C. 生命周期与后台执行
- Activity：onCreate→onStart→onResume→onPause→onStop→onDestroy+onSaveInstanceState；config change 默认销毁重建（或 configChanges 自处理）。https://developer.android.com/guide/components/activities/activity-lifecycle
- 进程 5 级：foreground>visible>service（startService；≥30 分钟可降 cached）>cached（随时杀）>空。https://developer.android.com/guide/components/activities/process-lifecycle
- A11 起 cached 进程用 cgroup freezer 整体冻结（官方专页已下线，细节未核实原 URL）。
- App Standby Buckets（A9+）：active/working set/frequent/rare/restricted(+never)，限制 jobs/alarms/网络；仅电池供电生效，充电不限；ML 预测分桶。https://developer.android.com/topic/performance/appstandby
- A8+ 后台禁 startService（startForegroundService 须 5 秒内 startForeground）。
- FGS 类型（A14 强制 type+FOREGROUND_SERVICE_<TYPE> normal 权限）：A14 共 13 种 camera/connectedDevice/dataSync/health/location/mediaPlayback/mediaProjection/microphone/phoneCall/remoteMessaging/shortService/specialUse/systemExempted，A15 +mediaProcessing=14。https://developer.android.com/develop/background-work/services/fg-service-types
- shortService ≈3 分钟（可再延一次 3 分钟）；A15 dataSync/mediaProcessing 24h 池 6h 上限、超时抛异常；while-in-use 权限的 FGS 禁止后台/BOOT_COMPLETED 启动。https://developer.android.com/about/versions/15/behavior-changes-15
- 借鉴：冻结即回收句柄是 BerryOS 优势；"长时运行必声明类型+配额"v1 写死，不留无类型后台。

## D. 自启动与关联启动
- force-stop 进 stopped state：收不到任何广播（含 BOOT_COMPLETED），直至用户显式启动。https://developer.android.com/guide/components/broadcast-exceptions
- A8+ 隐式广播 manifest 注册白名单：BOOT_COMPLETED、TIME/TIMEZONE/LOCALE_CHANGED、USB、蓝牙连接等约 15 项。同上
- A14：隐式 intent 只达 exported 组件；A15：BOOT_COMPLETED 禁启 dataSync/camera/mediaPlayback/phoneCall/mediaProjection/microphone FGS。https://developer.android.com/about/versions/14/behavior-changes-14 + /15/behavior-changes-15
- 后台启动 Activity 全面禁止（除全屏 intent 通知/overlay 等）；WorkManager periodic 最小 15 分钟（未核实原页）；精确闹钟 A12+ 需 SCHEDULE_EXACT_ALARM（A14 新装默认拒绝）。
- 借鉴：stopped state=appd 撤销全部句柄；"开机自启=manifest 声明+用户授予的独立 handle"一步到位。

## E. 通知
- NotificationChannel：O(26)+ 强制，无渠道不显示+logcat 错误；渠道建后开发者不可改 importance（用户终局控制）。https://developer.android.com/develop/ui/views/notifications/channels
- importance：NONE=0/MIN=1/LOW=2/DEFAULT=3/HIGH=4（heads-up）/MAX=5（OS 保留，app 上限 HIGH）。https://developer.android.com/reference/android/app/NotificationManager
- bubbles：对话浮层，用户逐会话启用；锁屏退回普通通知。trampoline 禁令（A12+）：通知点击须 PendingIntent 直达，禁 service/receiver 中转。https://developer.android.com/about/versions/12/behavior-changes-12
- 全屏 intent 通知 A14 起仅呼叫/闹钟（Play 2024-05-31 清退）；每 app 活跃通知限额约 25 条（未核实）。
- 借鉴：渠道"先注册后用户锁死"；通知点击直达目标 handle，禁借道提权。

## F. UI 规范
- 自适应图标：3 层各 108×108dp，视口 72dp、安全区 66×66dp、logo≥48dp、外围每边 18dp 留 mask/视差；单色层供主题化。https://developer.android.com/guide/practices/ui_guidelines/icon_design_adaptive
- SplashScreen（A12 全强制）：动画建议 ≤1000ms、branded image 200×80dp、冷/温启动显示热启动不显示。https://developer.android.com/develop/ui/views/launch/splash-screen
- M3：45 个色彩角色（primary/secondary/tertiary/error × on/container/variant+surface 族）、配对对比 ≥3:1、动态取色（壁纸取色生成整包方案+三档用户对比度）；字阶 15 基础+15 强调（Display/Headline/Title/Body/Label × L/M/S，具体 sp 未逐项核实）。https://m3.material.io/styles/color/roles
- 触控目标 ≥48dp（minWidth/minHeight 保底）。https://support.google.com/accessibility/android/answer/7101858
- 边到边：target 35 在 Android 15 默认强制（须处理 insets）。https://developer.android.com/develop/ui/views/layout/edge-to-edge
- 深色模式系统级+M3 token 双值；大屏 canonical layouts（list-detail/supporting pane/feed）；sp 随 font scale、A14 超 200% 非线性缩放（未核实）。
- 无障碍：TalkBack+contentDescription；对比度小字 ≥4.5:1、大字 ≥3:1。https://developer.android.com/guide/topics/ui/accessibility/apps
- 借鉴：图标安全区/48dp/insets 契约 v1 定死；M3 只取角色名+对比度约束。

## G. 桌面组件
- AppWidget：AppWidgetProvider+RemoteViews（受限声明式 UI）；尺寸按桌面网格 cells（A12+ targetCellWidth/Height 直给格数；旧公式 n×70dp-(n+1)×8dp 未核实）；resize horizontal/vertical/none；previewImage/A12 previewLayout；官方示例小=3×2、中=5×2 cells。https://developer.android.com/develop/ui/views/appwidgets
- Shortcuts：static（XML）/dynamic（运行时）/pinned（用户确认）；launcher 通常最多显示 4 条（static+dynamic 合计），上限 getMaxShortcutCountPerActivity()；pinned 无限、app 只能 disable；元数据 launcher 可读。https://developer.android.com/guide/topics/ui/shortcuts
- 借鉴：widget=受限声明式布局+launcher 持渲染能力（widget 进程不必活）；展示配额（系统）与创建配额（用户）分开。

## H. 系统应用/预装
- priv-app：Android 9+ 三分区（/system、/product、/vendor 下 priv-app）；特权权限须 <分区>/etc/permissions/privapp-permissions*.xml 白名单，**未白名单则不开机**（A9+，build 时校验）；白名单只管同分区；/vendor 只能拿 vendorPrivileged；OEM 自定义特权权限自动授予。https://source.android.com/docs/core/permissions/perms-allowlist
- 系统应用商店更新：versionCode 高即覆盖（常识，未核实专页）。APEX：Android 10 起更新低层模块（HAL/ART/类库）、可早于 PackageManager 在 boot 早期激活。https://source.android.com/docs/core/ota/apex
- Default apps=Roles（A10 RoleManager）：ASSISTANT/BROWSER/DIALER/SMS/HOME/EMERGENCY/CALL_SCREENING/COMPANION_DEVICE_*/DEVICE_POLICY_MANAGEMENT/SYSTEM_* 等，每角色绑行为义务+特权。https://source.android.com/docs/core/permissions/android-roles
- 借鉴：privapp 白名单=BerryOS 系统应用特权清单模板（构建期校验+启动对账）；默认应用做成 role（能力+义务职位，用户可换人）。

## I. 隐私合规
- A12+：麦克风/摄像头 in-use 状态栏指示器+快捷设置一刀切开关；剪贴板跨 app 首读 toast「APP pasted from your clipboard.」。https://developer.android.com/about/versions/12/behavior-changes-all
- Privacy Dashboard：设置内权限使用时间线（24h；细节未核实，原 support 页 404）。广告 ID：用户可删除；Play 要求声明、儿童禁用（未核实）。
- Package Visibility（A11+）：默认看不到他包，<queries> 声明扩权，QUERY_ALL_PACKAGES 需 Play 审批。https://developer.android.com/training/package-visibility
- Scoped storage：A10 引入 A11 强制；MediaStore 表；A13 READ_MEDIA_IMAGES/VIDEO/AUDIO 替代 READ_EXTERNAL_STORAGE（同请求图+视频只弹一个框）；A14 部分照片访问（READ_MEDIA_VISUAL_USER_SELECTED，未直接核实）。https://developer.android.com/training/data-storage/shared/media + /about/versions/13/behavior-changes-13
- 借鉴：枚举应用本身=显式 handle；按资源类型拆细能力（media.photo/audio 分立）避免日后大迁移。

## J. 分发与商店
- targetSdk 下限同 A；64 位强制（每个 32 位 ABI 须配 64 位；2019-08 新 app/2021-08 更新，日期未逐字核实）。https://developer.android.com/distribute/best-practices/develop/64-bit
- 数据安全表单：全员必填收集/共享/加密申报（含三方 SDK），不符可执法。https://support.google.com/googleplay/android-developer/answer/10787469
- Sideload：A8.0 起 per-app"安装未知应用"授权（未核实专页）。Instant apps：免安装、≤15MB、API 子集。https://developer.android.com/topic/google-play-instant/overview
- 借鉴：.bap+静态 ELF 天然 instant（下载即用）；商店 v1 要求 manifest 自带机器可读数据申报段。

## K. 数据与存储
- 沙箱：/data/data/<pkg>（内部）+ /sdcard/Android/data/<pkg>；每用户 /data/user/<n>/<pkg>。https://developer.android.com/training/data-storage
- 三路径：专属目录（免权限）/MediaStore（READ_MEDIA_*）/SAF（ACTION_OPEN_DOCUMENT/TREE/CREATE+persistable URI）。https://developer.android.com/training/data-storage/shared/documents-files
- Auto Backup：≥6.0，Google Drive，**25MB/app/用户**，A9+ 端到端加密；默认含私有目录+prefs+DB，排除 cache/codeBackup/noBackup；backup_rules.xml 排除；A12 云备份与 D2D 分离。https://developer.android.com/guide/topics/data/autobackup
- 借鉴：25MB+规则文件=BerryOS 云备份参数起点；备份=appd 持句柄的服务。

## L. 跨应用互操作
- Intent 显/隐式；A14 隐式只达 exported。App Links：http/https+autoVerify+域名 /.well-known/assetlinks.json（SHA256 指纹）→ 验证后免歧义直达（页面抓取不完整，机制通行）。Custom Tabs=复用浏览器进程的内嵌标签（未核实专页）。
- 分享：统一系统 Sharesheet（ACTION_SEND），官方明令勿自绘（ranking 系统独占；A10+ 富文本预览）。https://developer.android.com/training/sharing/send
- IME：InputMethodService 子类+BIND_INPUT_METHOD（signature 级仅系统 bind）+subtype。https://developer.android.com/develop/ui/views/touch-and-input/creating-input-method
- 无障碍服务：AccessibilityService+BIND_ACCESSIBILITY_SERVICE+用户设置显式开启。
- 借鉴：assetlinks 式包身份-域名绑定验证闭环；IME/无障碍"系统才能 bind"=appd 签发专用服务句柄。

## M. Android 特有补充
- Work profile：BYOD 双 persona（badge）；Private space（A15）：独立 profile+可选独立锁，锁定即整 profile 停止（通知/后台全冻）、媒体账号隔离、同 app 双副本。https://developer.android.com/about/versions/15/features + /work
- 克隆应用主要 OEM 功能（A15 无官方 clone API，未核实）。大屏 canonical layouts+多窗口；DeX 式桌面=厂商扩展（未核实）。https://developer.android.com/guide/topics/large-screens/large-screens-canonical-layouts
- Privacy Sandbox（Topics/Attribution Reporting/Protected Audience）：截至 2025 仍在季度反馈迭代（2025-Q2 报告在档），落地节奏未定。https://privacysandbox.google.com + developer.android.com/privacy-sandbox
- 借鉴：Private space"锁=停=冻全部句柄"与能力模型同构；广告标识符只保证=可撤销 handle 的机制位，替代方案不进 v1。

## BerryOS 若照抄会踩的坑（8 条）
1. 权限字符串思维：应让"权限=具体资源句柄"，仅保留 manifest 声明段做静态审计。
2. 无类型后台：Android 后补 14 种 FGS+6h 配额+3min 超时——BerryOS v1 直接"长时运行必有类型+配额，无豁免"。
3. 时间语义后补：only-this-time/auto-reset 是 10 年后补的洞；句柄 TTL/闲置回收应为基础语义。
4. 默认可见性：枚举/订阅默认最小化，都是显式 handle（勿重走 Package Visibility 收紧老路）。
5. 系统栏/图标/触控契约后补强制会碎一屏应用：安全区、48dp、insets 进 v1 toolkit 默认值。
6. 通知渠道不可删改逼应用建新渠道重置用户选择：允许归档/合并+用户确认迁移。
7. 两套特权体系（XML 白名单只管预装 vs 字符串弹窗管三方）：统一为"manifest 声明→appd 签发句柄"。
8. 深链双轨劫持：只做验证版深链（包身份-域名绑定），自定义 scheme 不给隐式路由地位。
