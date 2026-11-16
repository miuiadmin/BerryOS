# Android/Windows/OOBE 启动进度与开机引导调研（BerryOS 引导视觉设计弹药）

Type: survey
Status: current
Updated: 2026-11-14

一句话说明：为 BerryOS「统一模拟器入口 + 引导视觉（verbose 滚日志 / 图形 logo 遮罩 + 真实事件进度条）」与「首次开机引导」设计取材，对 Android（bootanimation 双段机制、BOOT_PROGRESS 事件锚点、progress 属性、system_server boot phases、GMS Setup Wizard）与 Windows（BGFX 转圈、verbose 状态消息、Windows Update 百分比、OOBE unattend）及 iOS/macOS/Android OOBE 流程做的一手源码级调研，并总结「进度条映射真实事件」的工程范式。Plymouth/systemd、Apple、OpenHarmony 三域见姊妹篇。

## 问题

六个问题。其一，Android bootanimation.zip 的两阶段（part0 循环 + part1 收尾）机制与退出握手；其二，BOOT_PROGRESS 事件锚点全集（boot_progress_start/preload/pms/ams_ready/enable_screen…）各自含义与打点位置，以及有没有把事件映射到开机动画进度；其三，`service.bootanim.progress` 谁写谁读；其四，Windows 启动转圈与「正在处理更新 xx%」是不是真实进度、verbose 等价物是什么；其五，首次开机引导（OOBE）：Android GMS setup wizard、iOS Hello 界面、macOS Setup Assistant 的步骤结构与跳过策略；其六，跨系统「进度条映射真实事件」工程范式总结。只取一手材料（AOSP googlesource、Apple 支持与部署文档、Microsoft Learn、Google 支持），逐条带出处。

## 方法与来源

web_search 通道部分可用（结果多次含大段原文），AOSP 侧一律以 android.googlesource.com 的 `?format=TEXT` base64 直抓为准（main 分支，2026-11-14 抓取验证；BootAnimation.cpp 取到 bbb00e78 引入 progress 机制的 diff 佐证）；Microsoft Learn 部分页面 curl 返回 404 占位页，改以 learn.microsoft.com 现存页 + web_search 快照取证并注明。iOS/macOS OOBE 取 support.apple.com 直抓 HTML 转文本。Android OOBE 的 GMS SetupWizard 闭源，以 AOSP 开源侧（frameworks/opt/setupwizard 库、Provision 应用）+ Google/三星支持页佐证流程。检索日期 2026-11-14。

## 结论

### ① Android bootanimation：属性握手的两段式动画，退出权在系统侧

架构三层：init 服务（`frameworks/base/cmds/bootanimation/bootanim.rc`，`service bootanim /system/bin/bootanimation`，`disabled` + `oneshot`，class core animation，ioprio rt 0）→ SurfaceFlinger 拉起（`initBootProperties()`：`property_set("service.bootanim.exit", "0"); property_set("service.bootanim.progress", "0"); property_set("ctl.start", "bootanim")`，因 property_set 可能被 init 的 mount_all --late 阻塞而特意放异步线程）→ bootanimation 进程（OpenGL ES 自绘，等 SurfaceFlinger 的 binder 就绪后跑动画）。

动画包 bootanimation.zip（FORMAT.md）：zip 不压缩（PNG 已压缩，`zip -0qry`）；`desc.txt` 首行 `WIDTH HEIGHT FPS [PROGRESS]`（第四列非零 = 最后一段显示进度百分比），随后每行 `TYPE COUNT PAUSE PATH [FADE [#RGBHEX [CLOCK1 [CLOCK2]]]]`。TYPE 三种：`p` = 可被启动完成打断，`c` = 必须播完（收尾段），`f` = p + 淡出 N 帧（只有第一个被打断的 f 段淡出，其余跳过）。COUNT=0 = 无限循环直到启动完成——这就是「两阶段」：part0 `p 0 ...` 无限循环当等待环，part1 `c 1 ...` 当收尾动画；系统发退出信号后循环段停止、播完 c 段再退进程。FORMAT.md 原文：系统在启动完成时置 `service.bootanim.exit` 为非零来结束动画（先播完未完成的 c 段）。另有 `$SYSTEM` 特殊 TYPE 引用系统包、trim.txt 裁剪定位、audio.wav 每段起播音效、dynamic_colors 动态染色行、手表 clock_font.png / 百分比 progress_font.png（16 列 6 行网格字模，ASCII 32-127，上排常规下排粗体）。

退出握手（谁写 exit=1）：两处。其一 SurfaceFlinger.cpp:767——`stopBootAnimation()`（等 WindowManagerService 的 binder 死亡监测路径）注释原话「formerly we would just kill the process, but we now ask it to exit so it can choose where to stop the animation」，随后打 event tag 60110；其二 WindowManagerService.java:3948-3967——`performEnableScreen()`（所有系统装饰窗绘制完毕后）`SystemProperties.set("service.bootanim.exit", "1")` + Trace「Stop bootanim」。bootanimation 侧 `checkExit()` 每帧（电影段每 50ms 上限）轮询 `EXIT_PROP_NAME`，非零即 `requestExit()` 让循环自然收尾。AOSP 内部书实证该时点早于 PHASE_BOOT_COMPLETED（boot 动画退出不等 boot 完成，等的是「桌面已画」）。

资源搜索序（BootAnimation.cpp:74-86）：`/oem/media/bootanimation.zip` → `/product/media/`（bootanimation-dark.zip 暗色优先）→ `/system/media/bootanimation.zip` → APEX `com.android.bootanimation`；shutdown 与 userspace-reboot 动画同构（userspace-reboot.zip 由 `sys.init.userspace_reboot.in_progress` 区分，:736）。

### ② BOOT_PROGRESS 事件锚点：11 个 30xx 事件 + event-log-tags 权威定义

权威定义在 `system/logging/logcat/event.logtags`（main，2026-11-14）与 frameworks/base 各 EventLogTags.logtags。全集与打点者：

| tag | 名称 | 打点者（main 分支实证） | 语义 |
|---|---|---|---|
| 3000 | boot_progress_start | AndroidRuntime.cpp:1228（LOG_BOOT_PROGRESS_START，runtime 启动） | Android runtime（app_process/Zygote）开始 |
| 3010 | boot_progress_system_run | SystemServer.java:839（BOOT_PROGRESS_SYSTEM_RUN） | SystemServer.run() 开始 |
| 3020 | boot_progress_preload_start | ZygoteInit.java:883 | Zygote 类预加载开始 |
| 3030 | boot_progress_preload_end | ZygoteInit.java:886 | Zygote 类预加载结束 |
| 3040 | boot_progress_ams_ready | ActivityManagerService.systemReady() | AMS 就绪 |
| 3050 | boot_progress_enable_screen | ActivityManagerService enableScreenAfterBoot | 允许亮屏（→触发停动画链） |
| 3060-3100 | boot_progress_pms_start / pms_system_scan_start / pms_data_scan_start / pms_scan_end / pms_ready | PackageManagerService 五连 | 包扫描五个里程碑 |

注意三点。其一，事件语义注释里 3040 写「ActivityManagerService.systemReady() starts」、3050 写「ActivityManagerService calls enableScreenAfterBoot()」（am/EventLogTags.logtags 注释原文）；其二，3000 的打点不在 Zygote 的 Java 层（ZygoteInit 里只有 3020/3030），而在 native 的 AndroidRuntime.cpp（:1228 常量定义）；其三，老版本（donut~eclair 的 logcat/event-log-tags）在 3100 后还有一行注释「+ check activity_launch_time for Home app」——Home（launcher）首帧时间用 activity_launch_time 事件另测，不在 30xx 序列。这些事件用 `logcat -b events` 读，时间戳是单调钟，官方用于 boot 耗时分析（bootanalyze.py、systrace 配套，见 source.android.com Optimize boot times）。

粒度度量：AOSP 原生事件面就是这 11 个粗粒度里程碑（外加 boot phases，见下），没有 per-service 细粒度进度事件——但这是刻意设计：boot phases（SystemService.java）以 100/480/500/520/550/600/1000 七档相位让全部系统服务分批解锁，每个服务在 onBootPhase(phase) 收到回调，天然是「事件 → 分档进度」的骨架。AOSP 内部书给出七档全表（PHASE_WAIT_FOR_DEFAULT_DISPLAY=100 … PHASE_BOOT_COMPLETED=1000）并实证 AMS.finishBooting() 驱动 1000 档、随后置 `sys.boot_completed=1` 与 `dev.bootcomplete=1`；且明确「boot 动画退出与 launcher 启动都发生在 PHASE_BOOT_COMPLETED 之前」。

### ③ service.bootanim.progress：AOSP 里「机制在、默认无人写」的真进度通道

机制完整（bbb00e78 diff 实证引入）：bootanimation 每帧读 `service.bootanim.progress`（0..100 整数），若 desc.txt 开了 PROGRESS 且是最后一段则把百分比画在动画下方 1/3 处（居中）。显示侧有刻意防跳变：每帧固定 `usleep(100000)`（0.1s）节流 + `lastDisplayedProgress` 每帧最多 +1（新值突跳也只 +1 递增），100 封顶不回落——即「显示层单调递增、限速追赶真实值」。

写入侧：AOSP 主线只有 SurfaceFlinger 初始化时清零（exit=0、progress=0）；sepolicy 里 `set_prop(system_server, bootanim_system_prop)` 与 `set_prop(surfaceflinger, bootanim_system_prop)` 都被授权，但 frameworks/base 主线源码里除清零外无其他写 progress 的调用点（RecoverySystemService 路径下无 bootanim 引用；cs.android.com 检索未能证实任何主线写入者）。真实使用者是更新链：关机更新场景 ShutdownThread 的 `uncrypt()` 监听 RecoverySystem 进度回调（uncrypt.cpp 按已拷贝字节数算 `progress = 100 * pos/file_size`，经 socket 回传，注释「progress must be between [0, 99]」），ShutdownThread 把它映射到关机进度对话框 [MOUNT_SERVICE_STOP_PERCENT=20, 100) 区间（radio 18%、mount 20% 等分段常量注释「roughly estimated based on timeout」）——但那是 ProgressDialog 而非 bootanimation 属性。开机动画侧的 progress 属性消费组合（Pixel 的「正在优化应用 xx%」等场景）属 GMS/私有组件，AOSP 开源树内无写入者，标注未证实。对 BerryOS 的直接启示：Android 的答案就是「粗里程碑事件（30xx）喂日志与测量、动画本身基本伪进度（循环等待 + c 段收尾）、唯一真进度通道（progress 属性）留给更新/恢复等有真实事务的场景，且显示层做限速+单调化」。

### ④ Windows：转圈是纯不确定动画；百分比只在更新/安装事务里出现

启动转圈：Windows 8 引入（E7 博客「Engineering the Windows 7 Boot Animation」给出前世：Win7 用 firmware framebuffer + CPU 软渲染 1024x768 32bpp 动画、write-combined 加速、减少图形模式切换次数；B8 时代换 BGFX）。BGFX（Boot Graphics Library，betawiki 汇总）：winload/内核早期经 VESA VBE 2.0 自绘，资源在 `bootres.dll`（内嵌 WIM 里的多分辨率 logo 位图），转圈每帧是 Segoe Boot 字体里的 PUA 字符；支持 BGRT（OEM 固件 logo 原样留在屏上，与 Plymouth 机制同源）；Win11 22H2+ 换 10X 遗产的渐进环（progressive ring）新转圈，注册表 `BootControl\BootProgressAnimation=1` 可启用（社区/MS Q&A 记载，标注非官方文档）。关键结论：正常启动的转圈不映射任何真实进度——没有百分比、没有底层事件馈送，纯不确定性动画；因为 Win8+ fast startup（hiberfil 恢复，B8「Delivering fast boot times」）后正常启动 7 秒级，微软刻意不给映射。

verbose 等价物：注册表 `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System` 的 `VerboseStatus=1`（GPO「Verbose vs normal status messages」/「Display highly detailed status messages」），开启后启动/关机/登录/注销显示每步状态文本（「RPCSS is starting」「Waiting for machine group policies to finish…」），`DisableStatusMessages=1` 可全隐藏（learn.microsoft.com 官方文档，题页自 Server 2003 时代延续至今）。这与 Plymouth ESC→details、Apple Cmd+V 同构：图形模式与文本模式共用同一事件流，开关切换。

更新百分比：「Working on updates, xx% complete」来自 Windows Update Orchestrator/arbiter 的真实事务进度（scan→download→install→commit 四阶段，每阶段内按包/字节推进），官方 how-windows-update-works 页描述四区；社区实测确认百分比可长时间停在一段（正常）、重启后可能「Undoing changes」回滚。它只在关机/重启的更新事务中出现——与 Android uncrypt 进度、Plymouth system-update 模式、OpenHarmony BootCompileProgress 同一范式：真百分比只出现在「有确切事务对象」的场景。

### ⑤ OOBE 三家：流程结构与跳过策略

Android（GMS Setup Wizard，闭源；开源侧证据 + Google 支持）：首次开机（或恢复出厂后）开机动画结束、launcher 起来之前，SetupWizard（com.google.android.setupwizard，Pixel 上叫 Pixel Setup）以 HOME 首个 launcher 身份接管首屏。流程（Google 支持 6193424/13761358 + 三星 ANS10004542 佐证）：Start（语言/视觉无障碍选项）→ Wi-Fi/移动网络 → 数据迁移（QR/线缆/无线，旧 Android/iPhone，可选「不复制」）→ Google 账号登录 → Google 服务条款/备份开关 → 屏幕锁（可 Skip）→ Google Assistant/Google Pay 等 →（三星侧再加 Samsung 账号与条款）→ Finish。关键工程细节：AOSP 的 setupwizardlib（frameworks/opt/setupwizard）定义 setup wizard 的 intent 协议——`firstRun` extra 标识首次流程、`deferredSetup` 标识「推迟设置」（用户跳过后 SetupWizard 常驻，Settings 顶部「Finish setup」入口 + 通知召回，WizardManagerHelper.isDeferredSetupWizard/EXTRA_IS_DEFERRED_SETUP 实证）；OEM（三星 SecSetupWizard）经 SETUP_COMPAT_SERVICE 权限与 Google 向导并行共存。AOSP 裸机无 GMS 时用 packages/apps/Provision 播一个极简首启流程（开源侧仅做 device provisioning 标记）。
跳过策略：多数步骤可 Skip（屏幕锁、生物识别、Assistant、数据迁移），语言与 Wi-Fi 不可跳（激活需要网络）；企业 fully managed 场景由 MDM（device owner provisioning）注入流程。

iOS（support.apple.com 105132，2026-11 抓取）：Hello 多语言屏（可用侧键三连击开 VoiceOver、三指双击开 Zoom）→（可选 Quick Start 近场迁移 / Set Up Without Another Device）→ 语言/地区 → Wi-Fi 激活（必须联网，可 Continue Without Wi-Fi 走蜂窝）→ 设备用途（自己/儿童）→ Face ID/Touch ID → 锁屏密码（可 Passcode Options 降级四位数/自定义/无）→ Apple Account 登录（可稍后）→ 自动更新等开关 → Siri 训练 → 深浅外观 → Screen Time → 数据隐私选择 → Apple Intelligence（可选）→ 安全功能介绍 → 完成。首启必经：语言、激活（联网）、密码（可降级不可全跳）；Quick Start 可把旧设备配置经近场搬运大幅缩短流程。
macOS Setup Assistant（support.apple.com 部署指南 depdeff4a547）：连线外设配对 → 国家/地区 → 键盘布局 → Wi-Fi/以太网 → 数据迁移（从 Mac/Time Machine/PC，可稍后）→ 激活锁/MDM（企业）→ Apple ID → 条款 → 创建电脑账户（用户名/密码）→ iCloud/Files、FileVault、诊断共享等开关 → 进桌面。企业侧可经 Apple School Manager/Business + MDM 的 Setup Assistant profile 跳过任意 pane（文档给出全量 pane 清单与 Skip Keys：Welcome、Accessibility、App Analytics、Appearance、Apple Account、Location、Migration、FileVault、Diagnostics…），跳过时取「更隐私保守」默认值；macOS 15.4+「Set Up as New or Restore」pane 不再允许隐藏；Auto Advance 支持零触碰走完（联网 + MDM 可达时全部 pane 自动跳）。
对照结论：三家 OOBE 共同骨架 = 语言/地区 → 网络 → 激活/账号 → 恢复或新机 → 生物识别/锁屏（可跳）→ 增值服务开关（可跳）→ 收尾；不可跳项各家都收敛到「语言 + 联网激活」；企业路径全部提供配置文件级跳过（Windows unattend OOBE 的 HideEULAPage/HideOnlineAccountScreens/HideWirelessSetupInOOBE/ProtectYourPC 同族，官方明令不要用 SkipMachineOOBE）。

### ⑥ 范式总结：进度条映射真实事件的工程谱系

把五域（Plymouth、Android、iOS/macOS、Windows、OpenHarmony，后三者引姊妹篇）证据放一起，成熟系统只有三种进度形态，且都做了「显示层」与「真值层」分离：

形态 A——纯伪进度/不确定动画（正常启动的主流）。Plymouth spinner/Fedora 默认、Android 两段式循环、Windows 转圈、iOS 正常启动无条。理由一致：启动时长方差太大（HDD→SSD、冷启→恢复），真实百分比要么误导要么难看；各家都选「动画不断 = 系统活着」的语义，把「卡没卡」交给诊断（Windows 用停转检测 + 自动 WinRE failover；Apple 用「数分钟无条 = 恢复模式」指征）。
形态 B——时间插值伪装真进度（跨启动校准）。Plymouth ply-progress 的双曲线插值 + boot-duration 缓存锚点校准（永远到不了 100%、护栏 0.999）；Apple XNU 内核进度 meter 的 8 段递减增量表 {10000,10000,8192,4096,2048,384,384,64}（初期快后期慢、上限 512/1024、SSD 系数 ×7）。共同点：单调、渐慢、留头部空间给真实事件接管，绝不回退。
形态 C——真事务百分比（更新/恢复/优化场景专用）。Plymouth system-update --progress（PackageKit 逐包）、Android uncrypt 字节百分比（[0,99] 经 socket 回传 + ShutdownThread 分段映射）、Windows Update 四阶段、OpenHarmony BootCompileProgress（时间线性兜底 + 事件确认后每帧 +1 冲刺）。共同点：存在确切可数的原子对象（包/字节/应用数），才有真 0..100。
工程配方（BerryOS 可直接抄）：事件层选 8-15 个粗粒度里程碑（Android 30xx 的 11 个是最佳模板），显示层做三件事——限速追赶（Android progress 每帧 +1 上限、0.1s 节流）、单调不回退（所有系统一致）、事件间隙用形态 B 插值填缝（Plymouth/XNU 都用渐减增量表）；真百分比通道单独留给 OTA/首次格式化等事务场景（形态 C），与启动条分离。防卡死兜底：时间上限（OpenHarmony duration 定时器、Windows 两小时社区共识、Android uncrypt 15 分钟超时）+ 诊断出口（ESC/VerboseStatus/停在事件流）。「越细越好」的正确解读：不是把进度条切成 100 个刻度，而是事件面做细（verbose 滚动日志天然细粒度——Plymouth kmsg/console 双流、Windows verbose 状态消息、Android logcat events 都在），进度条保持粗锚点 + 平滑插值——这是全部五个成熟系统的共同选择。

## 证据与出处

以下 URL 均为 2026-11-14 抓取验证。

AOSP bootanimation：bootanim.rc（service bootanim 定义、disabled/oneshot）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/cmds/bootanimation/bootanim.rc ；bootanimation_main.cpp（waitForSurfaceFlinger 后 run）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/cmds/bootanimation/bootanimation_main.cpp ；FORMAT.md（desc.txt 两段式 p/c/f、PROGRESS 列、progress_font.png、exit 机制原文）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/cmds/bootanimation/FORMAT.md ；BootAnimation.cpp（EXIT_PROP_NAME/PROGRESS_PROP_NAME/checkExit/drawProgress 限速+1/资源搜索序/userspace-reboot）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/cmds/bootanimation/BootAnimation.cpp ；progress 机制引入 diff（bbb00e78）https://android.googlesource.com/platform/frameworks/base/+/bbb00e78ef87e45ffb25b9347c648a04fac00b6e%5E%21/ 。

AOSP 退出握手与启动：SurfaceFlinger.cpp stopBootAnimation（property_set exit=1 + 注释「now ask it to exit so it can choose where to stop」+ event 60110）https://android.googlesource.com/platform/frameworks/native/+/refs/heads/main/services/surfaceflinger/SurfaceFlinger.cpp ；initBootProperties（三属性清零/ctl.start、异步原因 b/34499826）同文件；WindowManagerService.java performEnableScreen（:3948-3967 set exit=1、Trace Stop bootanim）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/wm/WindowManagerService.java ；SystemService.java（七档 PHASE_ 常量）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/SystemService.java ；ActivityManagerService.java（finishBooting→PHASE_BOOT_COMPLETED、ACTION_BOOT_COMPLETED 广播、enableScreen）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/am/ActivityManagerService.java 。

BOOT_PROGRESS 事件：system/logging event.logtags（3000/3020/3030 权威定义）https://android.googlesource.com/platform/system/logging/+/refs/heads/main/logcat/event.logtags ；am/EventLogTags.logtags（3040/3050 注释）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/am/EventLogTags.logtags ；server/EventLogTags.logtags（3010/3060-3100）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/EventLogTags.logtags ；AndroidRuntime.cpp:1228（3000 打点）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/jni/AndroidRuntime.cpp ；SystemServer.java:839（3010）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/java/com/android/server/SystemServer.java ；ZygoteInit.java:100-101,883-886（3020/3030）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/com/android/internal/os/ZygoteInit.java ；历史清单（3100 后「+ check activity_launch_time for Home app」）https://android.googlesource.com/platform/system/core/+/eclair-sholes-release/logcat/event-log-tags 。

sepolicy（谁可写 progress）：property_contexts（service.bootanim.progress → bootanim_system_prop exact int）https://android.googlesource.com/platform/system/sepolicy/+/refs/heads/main/private/property_contexts ；system_server.te:755 与 surfaceflinger.te:60（set_prop 授权）、bootanim.te（set_prop(bootanim, bootanim_system_prop)）https://android.googlesource.com/platform/system/sepolicy/+/refs/heads/main/private/system_server.te 、https://android.googlesource.com/platform/system/sepolicy/+/refs/heads/main/private/surfaceflinger.te 、https://android.googlesource.com/platform/system/sepolicy/+/refs/heads/main/private/bootanim.te 。

更新链真进度：uncrypt.cpp（协议图、progress = 100*pos/size、[0,99]、socket 回传）https://android.googlesource.com/platform/bootable/recovery/+/refs/heads/main/uncrypt/uncrypt.cpp ；ShutdownThread.java（三路径 UI 注释、BROADCAST_STOP_PERCENT=2…MOUNT_SERVICE_STOP_PERCENT=20 分段、uncrypt() ProgressListener 映射 [20,100)、MAX_UNCRYPT_WAIT_TIME 15 分钟、config_showPercentageTextDuringRebootToUpdate）https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/power/ShutdownThread.java ；reboot.cpp（关机动画模式切换 exit/progress 清零）https://android.googlesource.com/platform/system/core/+/android17-release/init/reboot.cpp 。

Windows：Engineering the Windows 7 Boot Animation（framebuffer 软渲染、32bpp、write-combined、减少模式切换）https://learn.microsoft.com/en-us/archive/blogs/e7/engineering-the-windows-7-boot-animation ；Designing for PCs that boot faster than ever before（7 秒启动、F8 窗口 <200ms、boot options menu、自动 WinRE failover）https://learn.microsoft.com/en-us/archive/blogs/b8/designing-for-pcs-that-boot-faster-than-ever-before ；Delivering fast boot times（hiberfil 快启原理）https://learn.microsoft.com/en-us/archive/blogs/b8/delivering-fast-boot-times-in-windows-8 ；How Windows Update works（scan/download/install/commit 四区）https://learn.microsoft.com/en-us/windows/deployment/update/how-windows-update-works ；Enable verbose startup…status messages（VerboseStatus 注册表键、DisableStatusMessages、示例消息）https://learn.microsoft.com/en-us/troubleshoot/windows-server/performance/enable-verbose-startup-shutdown-logon-logoff-status-messages ；BGFX/bootres.dll/Segoe Boot PUA/BGRT/Win11 progressive ring（betawiki 汇总，二手）https://betawiki.net/wiki/Boot_screen ；Win11 新 spinner 与 BootProgressAnimation（pureinfotech + MS Q&A，社区证据）https://pureinfotech.com/windows-11-new-spinner-replaces-windows-8-loading-animation/ 、https://learn.microsoft.com/en-us/answers/questions/6053821/how-to-enable-the-modern-progressive-ring-boot-ani ；Windows startup issues（启动阶段表）https://learn.microsoft.com/en-us/windows/client-management/img-boot-sequence 。

OOBE：Set up your iPhone or iPad（Hello 流程全步骤）https://support.apple.com/en-us/105132 ；Manage Setup Assistant（pane 全清单、Skip Keys、Auto Advance、macOS 15.4 变更）https://support.apple.com/guide/deployment/manage-setup-assistant-depdeff4a547/web ；Switch to a new Android device（QR/线缆/无线迁移）https://support.google.com/android/answer/6193424 ；Copy apps & data（Start→Wi-Fi→Copy 三步结构）https://support.google.com/android/answer/13761358 ；Samsung Setup Wizard 步骤（21 步实例）https://www.samsung.com/us/support/answer/ANS10004542/ ；WizardManagerHelper.java（firstRun/deferredSetup intent 协议）https://android.googlesource.com/platform/frameworks/opt/setupwizard/+/refs/heads/main/library/main/src/com/android/setupwizardlib/util/WizardManagerHelper.java ；Provision 应用（AOSP 裸机首启）https://android.googlesource.com/platform/packages/apps/Provision/+/refs/heads/main/ ；Automate OOBE（unattend Hide* 开关、SkipMachineOOBE 禁令）https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/automate-oobe 。

姊妹篇（ Plymouth/systemd、Apple、OpenHarmony 细节不再重复）：[Plymouth 调研](2026-11-14-survey-plymouth-systemd-boot-splash.md)、[Apple 调研](2026-11-14-apple启动视觉双模式调研.md)、[OpenHarmony 调研](2026-11-14-survey-openharmony-boot-animation-events.md)。

## 适用边界

其一，`service.bootanim.progress` 的主线写入者未证实：sepolicy 授权 system_server 与 surfaceflinger 写入，但 frameworks/base main 源码检索只见清零调用；cs.android.com 代码搜索页需登录无法穷尽（返回登录墙），Pixel「正在优化」类真进度显示的写入者（GmsCore/UpdateEngine 私有）只能标注未证实。其二，GMS Setup Wizard 内部步骤为闭源，流程结构以 Google/三星支持页与 setupwizardlib 协议佐证，屏幕顺序在不同 OEM/版本有差异。其三，Windows BGFX 细节（bootres.dll 结构、Segoe Boot PUA）来自 betawiki 二手汇总，微软无公开一手文档；Win11 progressive ring 默认启用状态各渠道说法不一（预览版 hidden feature、稳定版需注册表）。其四，Windows Update 百分比的内部计算（arbiter 分阶段权重）无公开一手文档，只有四阶段框架与社区行为观察。其五，本调研只取证不改码；BerryOS 侧落地设计（事件面选型、进度折算、OOBE 步骤）属后续 plans。
