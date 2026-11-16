# 对标底稿：Windows（10/11 桌面 + UWP/WinUI + MSIX）应用规范全景

> 采集：2026-11-08（应用规范立法调研·子代理底稿五号）。
> 方法注记：web_search 接口当时不可用（API 认证故障），改用直连抓取 learn.microsoft.com（含官方搜索 API + toc.json 做 URL 发现），下列 URL 除注明外均已验证 HTTP 200；个别无法落官方页 URL 的事实已标"未核实（URL）"。
> 消费面：`docs/应用规范-v1.0.md` §形态/§权限/§生命周期/§自启动/§通知/§UI/§互操作 + PC 双形态分岔专节。

## 一、包与安装模型

**1.1 双轨制**
- Win32 经典：MSI（Windows Installer/WiX）或 EXE（Inno/NSIS/InstallShield/Burn）安装器；无清单、无包身份，装到 Program Files/注册表，升级/卸载语义靠安装器自律；"包身份"（package identity）是能否使用现代 OS 特性（通知/后台任务/扩展/文件关联/启动任务/隐私开关）的分水岭——见 [稀疏包文档](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps)
- MSIX/UWP：单一 appxmanifest.xml；清单随包数字签名，签后改动即失效签名；部署状态机由 DeploymentService 管理（[清单文档](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/appx-package-manifest)）
- 清单核心字段（实抓）：`Identity`(Name/Publisher=证书 Subject/Version/ProcessorArchitecture=neutral) / `Properties`(DisplayName/PublisherDisplayName/Logo) / `Dependencies`(TargetDeviceFamily Name="Windows.Desktop" MinVersion/MaxVersionTested) / `Resources` / `Capabilities` / `Applications/Application`(Id/Executable/`uap10:TrustLevel`=`appContainer`|`mediumIL`/`uap10:RuntimeBehavior`=`win32App`) / `uap:VisualElements`(DisplayName/Description/BackgroundColor/Square44x44Logo/Square150x150Logo/AppListEntry) / `Extensions`
- 扩展点枚举（packaged 桌面/UWP 通用，[打包扩展文档](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/desktop-to-uwp-extensions)）：`windows.protocol`、`windows.fileTypeAssociation`、`windows.appExecutionAlias`、`windows.startupTask`、`windows.firewallRules`、`windows.cloudfiles`、`windows.autoPlayHandler`、`windows.appService`、`windows.shareTarget`、`windows.desktopAppMigration`、`windows.comServer`、`windows.fileExplorerContextMenus`、`windows.loaderSearchPathOverride`（≤5 路径）、后台任务 `BackgroundTasks` 声明（out-of-process 必须声明，in-process 不用）

**1.2 能力声明（三档 + 自定义）** — [能力全表](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations)
- 通用 `uap:Capability`：internetClient（仅出站）、internetClientServer、privateNetworkClientServer（内网出入站）、musicLibrary/picturesLibrary/videosLibrary/documentsLibrary、removableStorage（限已声明文件类型）、appointments、contacts、userAccountInformation、chat/blockedChatMessages、voipCall/phoneCall/phoneCallHistoryPublic、objects3D、codeGeneration（JIT 内存 API）、backgroundMediaPlayback、remoteSystem、spatialPerception 等
- 设备 `DeviceCapability`：webcam、microphone、location、proximity、bluetooth、USB、humanInterfaceDevice、lowLevel(GPIO/I2C/SPI/PWM)、pointOfService 等
- 受限 `rescap:Capability`（Store 提交需 Partner Center 审批；旁加载不审）：**runFullTrust**（打包 Win32 必须）、unvirtualizedResources、allowElevation、enterpriseAuthentication（域凭据/Windows 集成认证）、sharedUserCertificates、confirmAppClose、packageManagement/packageQuery、inputInjectionBrokered、extendedBackgroundTaskTime、extendedExecutionUnconstrained、email/emailSystem、chatSystem、phoneCallHistory(System)、screenDuplication、networkingVpnProvider、appCaptureSettings、userDataSystem 等（表中大量标注"不建议提交，多数不批"）
- 自定义 `customCapabilities:Capability`（企业专属场景，自定义能力 SID，需微软审批）
- 关键规则：mediumIL（full trust）打包应用也**必须声明 runFullTrust**；隐私敏感能力（标 * 者）即便 full trust 也建议声明——安装即在设置里出现该 app 的隐私开关

**1.3 渐进打包**
- 稀疏包（packaging with external location）：只做身份的 MSIX 清单 + 外部安装目录；`uap10:AllowExternalContent=true` + runFullTrust + unvirtualizedResources + mediumIL/win32App；`Add-AppxPackage -ExternalLocation`；最低 Win10 19041；exe 的 side-by-side manifest 加 `<msix>` 元素绑定身份；自签证书须导入 Trusted People 否则 0x800B0109
- 修改包（MSIX modification package，Win10 1809+）：企业定制 overlay，不重打包主应用；`uap4:MainPackageDependency`(可带 Publisher)、1903+ 须 `rescap6:ModificationPackage=true`；注册表定制走 User.dat(HKCU\Software)/Userclass.dat(HKCU\Software\Classes)/Registry.dat(HKLM)，运行时 VREG 与主包合并；文件 VFS overlay；多修改包冲突按包名字母序；卸载即还原（[修改包](https://learn.microsoft.com/en-us/windows/msix/modification-packages)）
- MSIX App Attach（AVD/企业桌面）：MSIX 装进 CimFS/VHDX/VHD 磁盘镜像，登录时挂载+注册；推荐 CimFS（平均挂载 255ms/卸载 36ms/内存 2%，vs VHD 356ms/1615ms/6%）；注册分 on-demand（默认，首启才全注册，不阻塞登录）与 log-on blocking；同主机可多版本并存；证书链必须全信任+建议加时间戳（[app attach](https://learn.microsoft.com/en-us/azure/virtual-desktop/app-attach-overview)）
- 其他：optional packages、streaming install（content group map）、flat bundles、framework packages、asset packages、shared package container（[MSIX 文档树](https://learn.microsoft.com/en-us/windows/msix/)）

**1.4 分发与签名** — [签名选项](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)
- Store：MSIX 提交免费，**微软重签名**→永不触发 SmartScreen；MSI/EXE 提交 Store 不重签，须自签且链到微软 Trusted Root Program CA
- 非 Store：Azure Artifact Signing（原 Trusted Signing）≈$9.99/月（组织限美加欧盟英国，个人限美加）；OV 证书 $150–300/年（2023-06 起 CA/B Forum 强制 HSM 存私钥）；EV $400+/年 **2024 起不再即时豁免 SmartScreen**；自签仅开发/企业内推信任
- winget：YAML 清单（[manifest schema](https://learn.microsoft.com/en-us/windows/package-manager/package/manifest)），InstallerType 枚举 `exe/msi/msix/inno/wix/nullsoft/appx/font`（新 schema 另有 zip/portable，未核实到枚举页）；InstallerUrl+InstallerSha256 校验；双源 = 社区 winget-pkgs + msstore；非 Store 应用 App Installer `.appinstaller` 文件配置自动更新

## 二、权限与安全模型

**2.1 UAC** — [How UAC works](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/how-it-works)
- 管理员登录产生双 token（standard + admin），shell 用 standard token 启动；子进程继承父 token 且须同完整性级别
- 触发：进程请求 admin token（exe 清单 requestedExecutionLevel = asInvoker/highestAvailable/requireAdministrator）→ 标准用户出 credential prompt，AAM 管理员出 consent prompt
- 弹窗分级：**灰底** = Windows 组件或已验证签名发布者；**黄底** = 未签名或签名不受信（"Publisher: Unknown"）；弹窗在 secure desktop（仅系统进程可绘制，剪贴板不可粘贴）
- 非提升场景不弹窗——UAC 只管"管理员边界"，不资源配置

**2.2 AppContainer** — [AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation)
- 身份 = 用户 SID+应用 SID 合成的 lowbox（S-1-15-3-...），凭据按 用户×应用 唯一，不能冒充用户
- 能力即 SID：授权按 capability SID 检查；摄像头/麦克风/位置等敏感设备默认阻断，经 broker（Runtime Broker）以代理身份放行
- 隔离面：文件/注册表（写仅限授权项+包内 LocalCache，读宽松）、网络（granular：互联网/内网/服务器各能力）、进程内核对象、窗口（不影响他人 UI）；键盘鼠标永远可用
- WinUI 3 打包桌面应用默认 mediumIL（full trust）**不在** AppContainer，除非 `uap10:TrustLevel="appContainer"`；MSIX 容器化（flexible virtualization）可调隔离强度

**2.3 SmartScreen / Defender** — [SmartScreen 信誉](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation) · [SmartScreen 概览](https://learn.microsoft.com/en-us/windows/security/operating-system-security/virus-and-threat-protection/microsoft-defender-smartscreen/)
- 双信号：发布者信誉（签名证书是否知名）+ 文件哈希信誉（该文件被下载历史是否有恶意迹象）；任一未知即"Windows protected your PC"，用户"更多信息→仍要运行"；无人工信誉提交通道，靠下载量自然积累（数周/数百次干净安装）
- EV 曾即时豁免，2024 已取消；未签名=每个新版本从零积累
- Win11 Smart App Control 可超越 SmartScreen：无信誉未签名直接阻断（不只限下载文件）；Defender 云查（block at first sight）叠加在下载执行链上

**2.4 UWP 能力=权限模型（与 Android 对比）**
- 同：清单声明、商店展示、系统强制
- 异：UWP **安装时一次性授予**，无运行时逐权限弹窗；取而代之是(1)隐私敏感资源首次调用时 broker 单次询问并记住，(2)设置页可随时全局/单 app 撤销（privacy settings 动态撤权，app 必须容忍资源不可用）
- 原因（微软文档反复强调）：桌面用户多任务流不习惯安装后被连环弹窗打断；"撤权"比"申请"对桌面心智更友好——移动（Android runtime permissions）与桌面在此分岔

## 三、生命周期与后台

**3.1 UWP 生命周期** — [app lifecycle](https://learn.microsoft.com/en-us/windows/uwp/launch-resume/app-lifecycle)
- 状态：NotRunning / Running-in-background / Running-in-foreground / Suspended / Terminated / ClosedByUser（PreviousExecutionState 五值决定恢复策略）
- 最小化或切走"几秒"后挂起（线程冻结、驻内存）；锁屏也挂起；内存不足可无通知终止（terminate 无回调）→ 状态必须保存在 Suspending
- Suspending 事件有 **deadline**（`SuspendingOperation.Deadline`，文档明说因设备而异，无固定秒数）+ GetDeferral 仅延迟不追加时间；要更多时间用 ExtendedExecutionSession（reason=SavingData/Unspecified/LocationTracking，**不保证批准**）[延伸执行](https://learn.microsoft.com/en-us/windows/uwp/launch-resume/run-minimized-with-extended-execution)
- EnteredBackground/LeavingBackground（1607+）：后台降内存（MemoryManager.AppMemoryUsageLevel）；prelaunch 预启动（需 opt-in）
- 状态持久化：ApplicationData 四区 Local/LocalCache/Roaming/Temporary + LocalSettings（键值）；卸载即删 Local，不动公共库文件

**3.2 后台任务** — [background tasks](https://learn.microsoft.com/en-us/windows/uwp/launch-resume/support-your-app-with-background-tasks)
- BackgroundTaskBuilder + 触发器 + 条件；in-process（1607+，同进程）与 out-of-process（backgroundtaskhost.exe）两种宿主
- SystemTriggerType 枚举：InternetAvailable、NetworkStateChange、OnlineIdConnectedStateChange、SmsReceived、TimeZoneChange、UserPresent、UserAway、ControlChannelReset、SessionConnected、LockScreenApplicationAdded/Removed
- 其他触发器：TimeTrigger（**最短 15 分钟**）、MaintenanceTrigger（**仅 AC 供电**）、PushNotificationTrigger、ControlChannelTrigger、SocketActivityTrigger、DeviceUseTrigger（仅前台可注册）、DeviceServicingTrigger（特权应用）、ApplicationTrigger、MediaProcessingTrigger、ToastNotificationActionTrigger 等
- 条件 SystemConditionType：InternetAvailable/InternetNotAvailable、SessionConnected/Disconnected、UserPresent/UserNotPresent
- 资源限制：**30 秒 wall-clock**、内存上限（低内存设备另有装机数限制）、Battery Saver 开启即抑制、ApplicationTrigger/MediaProcessingTrigger/DeviceUseTrigger 每实例限注册 1 次；ControlChannel/Push 有保证配额；注册前必须 RequestAccessAsync
- packaged 桌面应用用 Windows App SDK 后台任务（同概念不同命名空间）

**3.3 Win32 桌面后台自由态——自启动四条路径**（经典；枚举见 [Sysinternals Autoruns](https://learn.microsoft.com/en-us/sysinternals/downloads/autoruns)）
1. 启动文件夹：`shell:startup`（用户）/%ProgramData% 对应公共目录，放 .lnk
2. 注册表 Run 键：HKCU 与 HKLM 的 `…\CurrentVersion\Run`（+RunOnce 一次性）
3. 任务计划程序：登录/空闲/时间/事件触发，可最高权限运行（[Task Scheduler](https://learn.microsoft.com/en-us/windows/win32/taskschd/task-scheduler-start-page)）
4. 服务：SCM 托管、SYSTEM 会话、无 UI（[services](https://learn.microsoft.com/en-us/windows/win32/services/services)）
5. （现代第五条）MSIX 清单 `windows.startupTask` 扩展：TaskId/Enabled/DisplayName；须用户先启动过一次；出现在任务管理器"启动"页，用户禁用后**无法编程重新启用**
- 自由态=无生命周期管理：最小化不挂起不冻结，全靠 QoS 降权（见 3.4）节流

**3.4 资源管理（QoS）** — [Quality of Service](https://learn.microsoft.com/en-us/windows/win32/procthread/quality-of-service)
- 线程级 QoS 7 档：High(1709)/Medium/Low/Utility(22H2,后台服务)/**Eco(Win11,总是 E 核+最低频)**/Media(2004)/Deadline(2004)
- 窗口状态自动映射：In focus=High、Visible=Medium、**Minimized/完全遮挡=Low**（电池上选最效率频点+小核）；可闻声进程=High
- 2025.05 起：电池下前台无输入一段时间自动降 Medium（可注册表 PowerThrottling\DisableUserPresenceQos 关闭）
- 显式打标：SetProcessInformation/SetThreadInformation 切 PROCESS_POWER_THROTTLING_EXECUTION_SPEED（HighQoS/EcoQoS）；任务管理器"效率模式"=用户侧手动降权

## 四、通知与交互面

**4.1 Toast（app notification）** — [通知内容](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)
- XML payload（schema 或 Windows App SDK 1.2+ AppNotificationBuilder）
- 结构：attribution 区（Win11 起强制显示应用图标+名称+关闭+快捷禁用入口，shell 控制不可覆盖）/ visual / actions / audio
- 文本：必选 1 + 可选 2 个 AdaptiveText；标题最多 2 行、两段描述合计最多 4 行；支持数据绑定事后更新
- 图片：appLogoOverride 48×48@100%（可圆裁）、hero 364×180、inline 全宽、按钮图标 16×16 白色透明无内边距（全部按钮必须同给）；远程图限 3MB（计费网络 1MB）
- 交互：**按钮+输入合计 ≤5 个**（含上下文菜单项）；快速回复文本框（按钮 SetInputId 关联）、ComboBox 选择、系统 Snooze/Dismiss（XML activationType）
- Scenario 枚举：**Reminder**（常驻+必须有按钮）、**Alarm**（循环音频）、**IncomingCall**（预展开+铃声循环）、**Urgent**（可穿透 DND，需用户逐应用授权）
- 通知中心内可分组（headers）、进度条（数据绑定）、自定义时间戳、上下文菜单自定义项

**4.2 通知中心/勿扰**
- Win11：通知进通知中心（Action Center 演化为右下角通知中心+快速设置）；专注助手（Focus assist，ms-settings:quiethours）在 22H2 被**勿扰模式 + 专注会话**取代（URL 未核实，事实见 [设置深链表](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings) 含 ms-settings:notifications/quiethours）
- Urgent 场景通知可申请突破 DND；app 可被用户整体禁横幅/禁声音/设优先级

**4.3 徽章/磁贴**
- Live tiles 在 Win11 退场：开始菜单改"固定网格+推荐"，不再有动态磁贴；[图标规范](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction) 明言 "Windows 11 does not use the tile assets"，但 **Store 发布仍强制要求 Medium tile 100% 资产**（兼容 Win10）
- Badge（数字/glyph）仍存续：任务栏应用图标角标（设置可关）+ 锁屏 badge logo（Win10）；文档 [badges](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/badges)

**4.4 Jump List / 任务栏** — [Taskbar extensions](https://learn.microsoft.com/en-us/windows/win32/shell/taskbar-extensions)
- Jump List 内置类别：Recent/Frequent（SHAddToRecentDocs 自动喂养）/Pinned + 自定义类别 + Tasks（上下文无关的静态动作，IShellLink）；项数上限由系统按分辨率动态定，溢出从底部截断
- 任务栏 API（ITaskbarList3，需等 TaskbarButtonCreated 消息）：SetProgressState（TBPF_NOPROGRESS/INDETERMINATE/NORMAL/ERROR/PAUSED）+ SetProgressValue 进度条入任务栏按钮；SetOverlayIcon 角标 overlay；缩略图工具栏 **≤7 按钮**（右截断）；RegisterTab 支持"每标签页一个缩略图/切换目标"；分组靠 AppUserModelID
- **编程固定任务栏被禁止**，只能用户手动固定

## 五、UI 规范（Windows 11）

**5.1 WinUI 3 / Fluent Design**
- 材质：**Mica**（不透明、取桌面壁纸色、作窗口底层背景，省电替代 Acrylic）[Mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica) ；**Acrylic**（半透明+模糊，用于菜单/flyout 等临时表面）[Acrylic](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic)
- 圆角：窗口与控件 4/8px（rounded-corner 文档）；[WinUI 3](https://learn.microsoft.com/en-us/windows/apps/winui/winui3/) 为现役 UI 栈（Win32 桌面打包/免打包皆可用）
- 字体：**Segoe UI Variable**（变量字重轴 wght 100–700 + 光学轴 opsz 8–36pt 自动）；常用档 Light300/Semilight350/Regular400/Semibold600/Bold700；**用 Semibold 替代 Bold，不用 Italic**（可读性）[字阶](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography)
- 字阶（epx/行高）：Caption 12/16、Body 14/20、Body Strong 14/20、Body Large 18/24、Subtitle 20/28、Title 28/36、Title Large 40/52、Display 68/92；最小 12px Regular / 14px Semibold；左对齐+sentence case；简中用 Microsoft YaHei UI
- 暗色模式与高对比度主题为系统级三态（对比主题细节页 URL 未核实，总纲见无障碍概览）

**5.2 图标规格** — [图标构造](https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction)
- 最低集：**16/24/32/48/256 px**（保证只缩小不放大）
- targetsize 全枚举：16,20,24,30,32,36,40,48,60,64,72,80,96,256；且**明/暗主题 unplugged 双套必须**（缺省会被系统加背板）
- 展示基准（100%）：上下文菜单/标题栏/托盘=16px；**任务栏/搜索/开始列表=24px；开始固定=32px**；缩放系数 100/125/150/200/250/300/400%（如任务栏 24→30→36→48→60→72→96）
- MSIX 映射：Square44x44Logo/Square150x150Logo/Wide310x150/Square310x310/Square71x71/StoreLogo(50px) 为基准尺寸，配 .scale-100/125/150/200/400 变体；Win32 .ico 常规 16/32/48/256

**5.3 DPI** — [DPI awareness](https://learn.microsoft.com/en-us/windows/win32/hidpi/setting-the-default-dpi-awareness-for-a-process)
- 推荐 manifest 声明 `<dpiAwareness>PerMonitorV2</dpiAwareness>`（1607+，v2 相比 v1 增非客户区/Alt-Tab/子 HWND 自动缩放）；旧 `<dpiAware>true/pm</dpiAware>` 回退
- 混合模式：每顶层窗口可不同 awareness（每线程默认=进程默认）；改默认须在创建任何 HWND 前（SetProcessDpiAwarenessContext）

**5.4 键盘/无障碍** — [无障碍概览](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessibility-overview) · [键盘交互](https://learn.microsoft.com/en-us/windows/apps/design/input/keyboard-interactions)
- 键盘先行：一切操作键盘可达；Alt 访问键/keytips；遵循系统快捷键；焦点指示必须可见
- 辅助功能走 **UI Automation（UIA）** 协议，Narrator 等读屏依赖；toast 按钮图标需提供高对比白版本；XAML 控件默认暴露 UIA

**5.5 窗口管理** — [Win11 总览](https://learn.microsoft.com/en-us/windows/whats-new/windows-11-overview) · [Snap 支持](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/apply-snap-layout-menu)
- Snap Layouts：悬停最大化按钮 / Win+Z / 22H2 拖窗口到屏幕顶部出布局浮层；Snap Groups 成组记忆，任务栏悬停整组恢复
- 多窗口：每顶层窗口独立任务栏缩略图；MDI/标签应用用 RegisterTab 把"标签"提升为独立缩略图与切换目标；22H2 起系统应用（资源管理器/记事本）自带标签页

## 六、系统应用与预装

- 内置应用（Win11）：Store、设置、Windows 安全中心、Edge、照片、计算器、时钟、记事本、画图、截图工具、媒体播放器、Windows Terminal（随系统）、小组件、Phone Link 等；多数预装 UWP/Store 应用可卸载（计算器/相机/地图等），Store/设置/安全中心等核心不可卸载——**精确可卸载清单未核实到官方单页**（总览：[apps on Windows](https://learn.microsoft.com/en-us/windows/application-management/overview-windows-apps)）
- Win32 附件 Store 化：记事本/画图/截图工具/媒体播放器在 Win11 重写为 WinUI/Store 分发更新（记事本 22H2 加标签页）——公告 URL 未核实（blogs.windows.com/windows-insider）
- 默认应用 2023 新规（官方落点）：Win11 21H2/22H2 自 **2023-04 累积更新**起 `ms-settings:defaultapps` 支持 `registeredAppMachine=<名>` / `registeredAppUser=<名>` / `registeredAUMID=<AUMID>` 深链**直达某应用的默认设置页**；默认浏览器专用页 `ms-settings:defaultbrowsersettings` 已弃用；逐文件类型/链接类型设定（[ms-settings URI 表](https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-settings)）
- UserChoice 注册表带哈希防劫持（应用不能静默改默认，只能引导用户去设置页）——细节页 URL 未核实
- 生态注脚：WSA（安卓子系统）2025-03 已停用（URL 未核实）；企业预装走 provisioning/Add-AppxProvisionedPackage

## 七、跨应用互操作

- 协议注册：MSIX `windows.protocol` 扩展（Name+可选 Parameters，full trust 可传参，UWP 只收 activation 事件）[打包扩展](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/desktop-to-uwp-extensions) ；UWP 侧 OnActivated（[URI activation](https://learn.microsoft.com/en-us/windows/uwp/launch-resume/handle-uri-activation) 301→新址）；Win32 传统=HKCR 注册 URL Protocol
- 文件关联：MSIX `windows.fileTypeAssociation`（SupportedFileTypes/SupportedVerbs 带 Id+Parameters+Extended/迁移用 MigrationProgIds/UseUrl 直开 URL/缩略图、属性、KindMap、上下文菜单 handler）；Win32 传统=ProgID+注册表；**默认打开者由用户 UserChoice 决定**，应用只能声明"可处理"并引导设置
- 共享目标：share contract `windows.shareTarget` 扩展 + DataPackage（来源 DataTransferManager.ShowShareUI）——Win32 打包应用也能当共享目标 [share](https://learn.microsoft.com/en-us/windows/uwp/app-to-app/share-data)
- AppService（应用服务/后台通信）：清单 `windows.appService` 扩展（`uap3:AppService Name` 建议反域名，`uap4:SupportsMultipleInstances` 15063+；实现为后台任务或同进程）[AppService](https://learn.microsoft.com/en-us/windows/uwp/launch-resume/how-to-create-and-consume-an-app-service)
  - 客户端：AppServiceConnection(AppServiceName+PackageFamilyName)→OpenAsync→SendMessageAsync(ValueSet 双向)
  - 生命周期：调用者前台=与调用者同寿；调用者后台=**30s 窗口，deferral 追加一次性 5s**；取消时机=连接释放/调用者挂起/系统关机或资源不足
  - 支持 OpenRemoteAsync 跨设备（remoteSystem 能力）

## 八、桌面形态与移动形态规范分岔点（给 BerryOS 的双形态启示）

1. **自启动语义**：移动=后台常驻是被管资源（挂起/配额/电池节流）；桌面=自启动是**用户明示的期望**（启动文件夹/Run 键/计划任务/服务），OS 只登记+可禁用（任务管理器启动页/启动影响分析），不预设"后台=恶意"。BerryOS PC 档应把"自启动"做成清单声明的、用户可见可禁的注册表——照 MSIX startupTask：首次运行才激活、用户禁用后不可编程复活。
2. **权限弹窗时机**：移动=安装时清单授权+运行时可撤（UWP 实际同此）；桌面用户不习惯连环弹窗→微软用 broker **首次使用时询问**（用摄像头才弹摄像头），而非安装时全量弹。BerryOS 双形态可统一"安装时静默登记、首用时 broker 询问、设置页随时撤"三段式，移动档再加安装前能力确认页。
3. **后台限制**：移动式挂起（suspended→terminate 无通知）直接搬到桌面会毁掉 IDE/下载器/音乐播放器等"最小化即工作"的心智。微软的双轨答案：UWP 挂起+按触发器租赁式后台；Win32 自由态+**QoS 分级节流**（前台 High/可见 Medium/最小化 Low/Eco 显式）——资源治理从"冻结"换成"降权降频"，对 BerryOS（省电支柱+PC 高性能支柱）这是现成的调和器。
4. **窗口=桌面身份**：桌面的任务分组（AppUserModelID）、每窗口缩略图、Snap Layouts、进度条入任务栏、Jump List——移动无对应物；BerryOS PC 壳必须为 .bap 定义"窗口声明面"（多窗口/标签/任务栏集成/固定），移动档无此节。
5. **分发信任**：桌面分发天然多渠道（Store/winget/企业镜像/旁加载），签名与信誉体系（重签名免 SmartScreen、哈希信誉、EV 失效）是"能不能被用户装上"的第一道闸；移动档商店是默认唯一渠道。BerryOS .bap 单一格式但 PC 档要预留"旁加载+签名信任链"策略。
6. **默认应用博弈**：桌面"默认浏览器/默认播放器"是竞争高地，微软 2023 被改为单应用深链+逐类型设定（且 EU DMA 进一步开放）——BerryOS 若做 PC 形态，默认应用设定必须**用户主权+防劫持哈希**从第一天进规范。

## 九、BerryOS PC 形态若照抄会踩的坑（≤6）

1. **照抄 UWP 挂起模型强加给全部应用**：桌面关键场景（下载/编译/播放/外设会话）不能"最小化几秒即冻结"；必须像 Windows 那样区分"受管生命周期应用"与"自由态桌面应用+QoS 节流"两档，或给 .bap 声明 backgroundClass（音频/下载/设备）豁免面。
2. **把安装时权限全量弹窗搬到桌面**：桌面装机频率低、应用多，安装时弹 8 个权限必被吐槽；应学 broker"首次使用询问+可撤"，安装时只静默登记清单。
3. **清单锁定过死、无渐进打包层**：Win32 三十年存量靠稀疏包（外部位置身份包）一步上船；BerryOS 若 .bap 清单要求"全有才能跑"，旧式/移植应用（POSIX 兼容层目标）没有过渡通道——需要"身份子集/最小清单"档。
4. **通知通道无优先级分层**：只有一种 toast 会被 IM/营销推送淹没并被用户全局关闭；必须有 Scenario 式分层（普通/提醒/闹钟/来电/紧急穿透勿扰）+ 用户逐应用降噪开关+勿扰模式。
5. **默认应用/文件关联做成"最后安装者得"**：桌面文件生态（双击打开）是核心体验；不引入 UserChoice 式"用户选择+哈希防改"，装机工具一跑默认全被劫持，口碑即死。
6. **图标/资产只定义一套尺寸**：桌面多 DPI（100–400%）+明暗主题+任务栏/开始/上下文菜单多展示位，一套 PNG 必糊/必加背板；.bap 资产规范应直接内建 targetsize 枚举+主题变体要求（16/24/32/48/256 最低集+unplated 双主题），免得后期补规范。
