# 其余系统启动视觉与进度条人因调研（BerryOS 引导视觉设计弹药）

Type: survey
Status: current
Updated: 2026-11-14

一句话说明：为 BerryOS「统一模拟器入口 + 引导视觉（verbose 滚日志 / logo 遮罩 + 事件映射进度条）」设计补齐两块弹药：其一，SteamOS、Switch、PS5、车机（QNX/U-Boot/AGL/树莓派）、ChromeOS 五家从固件到桌面的 splash 链与时长；其二，进度条感知研究（Harrison UIST 2007 全文）、never-decrement、99% 停滞、事件驱动 + 时间插值混合模型、假进度取舍与启动仪式感，每条结论带出处 URL，未证实项如实标注。

## 问题

BerryOS 图形引导模式的进度条要「映射真实启动事件、粒度越细越好」，需要两块依据：其他系统怎么做启动视觉（链路多长、交接怎么无缝、仪式感怎么取舍），以及进度条人因与工程模式（什么让条显得快、为什么不许回退、事件之间怎么插值）。本文只管这两块；Apple 双模式见 [apple 启动视觉调研](2026-11-14-apple启动视觉双模式调研.md)，Plymouth/systemd 事件面与 BGRT 交接见 [plymouth 调研](2026-11-14-survey-plymouth-systemd-boot-splash.md)，OpenHarmony 事件体系见 [openharmony 调研](2026-11-14-survey-openharmony-boot-animation-events.md)。

## 方法与来源

本会话 web_search 后端 401 认证故障（与 apple 调研记录一致），改用 DuckDuckGo lite HTML 版检索 + 直取已知一手 URL；一手优先级：官方文档与源码（Valve 镜像、switchbrew、QNX/U-Boot/Chromium 官方文档、WebKit 源码、微软 Learn）> 厂商博客 > 社区实测。Reddit、VGC、StackExchange 部分页面反爬 403，相关引文以检索摘要为准并标注。

## 第一章：其余系统启动视觉

### Steam Deck / SteamOS

**分区与引导链。** SteamOS 3 是 8 分区 A/B 布局：esp（64 MiB，存 stage1 引导器与描述 A/B 槽如何选择的元数据）→ efi-A/efi-B（各 32 MiB，各含 stage2 GRUB）→ rootfs-A/B（各 5 GiB 只读 btrfs）→ var-A/B（256 MiB）→ home 兜底共享；rootfs 只读、/etc 是 overlayfs，更新走 RAUC 签名束 + casync 增量写入非活动槽、改 esp 引导配置后重启换槽。出处：https://iliana.fyi/blog/build-your-own-steamos-updates/ 、https://github.com/randombk/steamos-teardown/blob/master/docs/partitions.md 、https://www.collabora.com/news-and-blog/news-and-events/steamos-3-6-how-the-steam-deck-atomic-updates-are-improving.html

**splash 链与启动动画。** 视觉链是固件 → GRUB（安静）→ 内核与用户态 → gamescope 会话拉起 Steam 客户端（Game Mode），最后由 Steam 客户端播放 startup movie：webm 容器（VP8/VP9 编码），自定义动画放 `~/.steam/root/config/uioverrides/movies/`，在 Settings → Customization 里选固定一条或开 shuffle 每次随机；官方还以 Steam 点数售卖启动动画，社区仓 steamdeckrepo 聚合第三方动画——即「启动动画」不在内核 splash 层，而在最上层的客户端。出处：https://steamcommunity.com/app/1675200/discussions/0/684117136053950983/ 、https://steamdeckrepo.com/

**时长。** 无官方数字；社区数据（未证实）：logo 动画本身约 10 秒量级，但到「能开游戏」还叠加 Steam 客户端启动、更新检查与登录——Wi-Fi 异常时总时长可拖到约 2 分钟，离线模式则游戏立即可开。出处：https://steamcommunity.com/app/1675200/discussions/0/804592995637765679/ 、https://steamcommunity.com/app/1675200/discussions/0/4695658272798149025/ 、https://www.reddit.com/r/SteamDeck/comments/1578aij/boot_times_are_insane/

### Nintendo Switch

**引导链。** 一手（switchbrew）：BootROM → package1（装在 eMMC boot 分区 0/1，Package1ldr 在 NVIDIA boot 处理器 BPMP 上跑）→ 后续阶段 → 内核；boot 分区还有 normal/safe 两套加备份供修复比较。社区大量「卡在 Nintendo logo」案例印证 logo 屏属引导早期而非主菜单层。出处：https://www.switchbrew.org/wiki/Package1 、https://www.switchbrew.org/wiki/Boot 、https://gbatemp.net/threads/modded-switch-stuck-on-nintendo-logo-screen.635290/

**logo 归属引导链。** 原版 logo 是 308x350 位图，社区用 IPS 补丁在 boot 链上整图替换（Atmosphere/hekate 生态的 logo patcher 必须与原尺寸一致），说明 Nintendo logo 由引导组件绘制、不归应用层管。出处：https://github.com/friedkeenan/switch-logo-patcher 、https://www.gamebrew.org/wiki/NX-BootLogo-Generator_Switch

**上电到主菜单时长。** GameSpot 实测（VGC 转述）：从 boot 起约 20 秒到主菜单，新旧硬件相同（新版甚至慢不到 1 秒）。出处：https://www.videogameschronicle.com/news/new-nintendo-switch-doesnt-improve-loading-times-tests-suggest/

**「约 1 秒 logo」与极简哲学。** logo 时值无任何一手资料（未证实）；佐证性材料是任天堂官方支持页把 Sleep Mode 当电源主路径（Turn Off/Restart/Sleep 三选一里睡眠是日常态），以及 Windows 8 团队援引手机用法「几乎从不重启、永远睡眠态」——sleep-first 降低冷启频次，冷启才敢极简。出处：https://en-americas-support.nintendo.com/app/answers/detail/a_id/22319 、https://learn.microsoft.com/en-us/archive/blogs/b8/delivering-fast-boot-times-in-windows-8

### PlayStation 5

**日常冷启动形态。** 一手描述（Audiovisual Identity Database）：PS logo（与 PS4/PS Vita 同款）→ 淡出 → 健康与安全警告 → 蓝色 bokeh 光斑爆成金色火花 → 收在灰色底；rest mode 唤醒则跳过 logo 与健康警告——冷启与唤醒两档仪式。出处：https://www.avid.wiki/PlayStation_5/Startup_Screens

**首次开机单独设计。** 首启或恢复出厂用另一支完整 pre-rendered CGI（2020-03-12 制作完成，60/120fps）：logo、雾团、火花、3D 主机与手柄登场、提示按 PS 键——首启仪式感是独立资产；启动音还在 Astro's Playroom 里作为彩蛋复现。出处：https://www.avid.wiki/PlayStation_5/Startup_Screens

**时长与「白点」。** 无官方秒数（社区讨论 PS5 启动比 PS4 略慢，未证实）；「白点/白光」应即上述 bokeh 光斑或首启白光雾团，官方无逐帧命名。出处：https://www.reddit.com/r/playstation/comments/1bduw7v/ps5_takes_slightly_longer_to_startup/

**品牌时刻。** 社区有「希望首启动画每次都放」的帖子——该动画被玩家当资产而非负担。出处：https://www.reddit.com/r/PS5/comments/jvto3k/i_wish_the_ps5_first_time_startup_happened_all/

### 车机与嵌入式（QNX / U-Boot / AGL / 树莓派）

**QNX：上电到出画面是硬指标。** QNX CAR 平台官方优化目标原文四条：early splash screen and camera（尽早加载 IFS、尽早跑 Screen 图形子系统与图形应用）、early audio、early HMI display（10 秒内）、last audio playing（10 秒内，关机存状态开机恢复）；QNX 7.1/8.0 fastboot 文档还建议 splash 应用别用 GPU 以压缩出图时间，并强调 Screen 的起点受 IPL → 内核 → Core OS 基础启动时长制约。出处：https://www.qnx.com/developers/docs/6.6.0.update/com.qnx.doc.fastboot/topic/best_fast_practices.html 、https://qdn.qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.fastboot/topic/cscreen_fastboot_about.html 、https://www.qnx.com/developers/docs/8.0/com.qnx.doc.neutrino.fastboot/topic/cscreen_optimize_screen_startup.html

**U-Boot splash 与交接。** 官方文档：CONFIG_SPLASH_SCREEN 从环境变量 splashimage 指定的内存地址画 BMP，splashpos 定位（`m,m` 居中），splashsource/splashfile 支持从 MMC/USB/SPI/FIT 等选源；默认 (0,0) 位置会叠印 U-Boot 版本横幅，需 CONFIG_HIDE_LOGO_VERSION 抑制；SPL 阶段可再提前（TI AM62x 官方实测 SPL splash 比 U-Boot proper 早约 1.4 秒）——「越早的 loader 出图越好」是嵌入式惯例。Toradex 则已弃用 U-Boot splash 改 Plymouth 单一维护路径；OS 侧无缝接管 framebuffer 的机制不在此重复（见 [plymouth 调研](2026-11-14-survey-plymouth-systemd-boot-splash.md)）。出处：https://docs.u-boot-project.org/en/latest/develop/splash.html 、https://software-dl.ti.com/processor-sdk-linux/esd/AM62X/09_00_00_03/exports_AM62SIP/docs/linux/Foundational_Components/U-Boot/UG-Splash-Screen.html 、https://developer.toradex.com/linux-bsp/os-development/boot/splash-screen-on-u-boot/

**AGL：Yocto psplash。** AGL demo 平台（meta-agl-demo）用 Yocto 标配 psplash 配方；psplash 自述「very simple boot splash」，framebuffer 上画 logo + 进度条、进度经 FIFO 写入驱动——车机 Linux 侧同样是「真事件进条」路线。出处：https://github.com/automotive-grade-linux/meta-agl-demo/tree/trout-sodev/recipes-core/psplash 、https://git.yoctoproject.org/psplash/

**树莓派彩虹屏：GPU 固件期显示先例。** 彩虹屏在 GPU 固件（start.elf）加载后出现，本质是 4 个像素被 GPU 放大到全屏（官方论坛：彩虹属固件、不支持换图，只能 config.txt 加 `disable_splash=1` 隐藏）；之后的画面才归 Linux——官方 rpi-splash-screen-support 包即「尽早画全屏图替代默认四颗树莓」。这是「OS 未起、显示先亮」的固件期显示经典先例。出处：https://forums.raspberrypi.com/viewtopic.php?t=329940 、https://raspberrypi.stackexchange.com/questions/19354/raspberry-pi-with-boots-up-with-rainbow-screen 、https://github.com/raspberrypi/rpi-splash-screen-support

### ChromeOS

**固件链与固件 UI。** 设计文档：RO boot stub 持根公钥验 A/B 固件 → firmware boot loader 验 A/B 内核签名，任一环节失败落只读 recovery 固件；开发者内核显示「非官方系统」警告屏并等 30 秒超时——这些屏由固件自己画：bmpblk 位图块（RO/RW 两份、按语言分文件、有 DPI 约束与 fallback 彩条/文本兜底），是 depthcharge（coreboot payload）的 UI 面。出处：https://new.chromium.org/chromium-os/chromiumos-design-docs/firmware-boot-and-recovery/ 、https://new.chromium.org/chromium-os/developer-library/guides/debugging/firmware-ui/

**内核后开机画面。** 实测机制（开发者模式取证）：boot-splash 服务（`/etc/init/boot-splash.conf`）按 `--frame-interval 25`（25 毫秒/帧）逐帧播放 `/usr/share/chromeos-assets/images_100_percent/boot_splash_frameNN.png`——现代 ChromeOS 的开机画面是 Chrome logo 帧动画（动效），不是百分比进度条；资产住在 common-assets/chromeos-assets 仓。出处：https://gist.github.com/supechicken/c57f8bb4b9dad2a29611ce05b1324b5c 、https://chromium.googlesource.com/chromiumos/platform/assets/

**「固件白屏 → logo + 进度条」的核对。** 固件期短白/黑屏属社区常见描述，官方文档只定义了 recovery/开发者模式等异常屏，正常路径的固件屏未见文档定义（未证实）；现行实现没有传统意义进度条，OOBE/更新场景另有百分比（未取证）。

## 第二章：进度条人因与设计原则

### Harrison 等《Rethinking the Progress Bar》

**出处勘误。** 实际论文是 Harrison、Amento、Kuznetsov、Bell，UIST 2007（https://www.chrisharrison.net/index.php/Research/ProgressBars ，全文 PDF https://chrisharrison.net/projects/progressbars/ProgBarHarrison.pdf ）；任务书提及的「CHI 2004/2007、It's not nihilism…」未检索到对应文献，属记忆偏差（未证实）。

**方法。** 22 名参与者、9 种进度函数（线性基线 + 8 种非线性，含 Early/Late Pause、快慢 Wavy、Power、Inverse Power 等）、每条固定 5.5 秒、全对成对比较 + 逻辑回归。

**核心发现。** 三条：其一，暂停被感知为更久，且越靠近结尾伤害越大（peak-and-end 峰值-结尾效应；Slow Wavy、Fast Wavy、Late Pause 全部显著慢于线性）；其二，加速显著讨喜——Power 与 Fast Power（指数型、结尾最快）是仅有的两个显著快于线性的函数；其三，Early Pause 与线性几乎打平——开头慢/停最被容忍。另有顺序效应：先看到的条占优（α=0.42）。

**设计建议（论文原文）。** 一是开头缓存进度以补偿后段负行为（cache progress，即「向前补偿 position jump」——条先跳一段垫背）；二是前段低调、结尾加速（rapid conclusion 最讨喜）；三是多阶段任务重排：慢或不可变的阶段（如联网取更新）放最前，本地可预测阶段放后。

**引文脉络。** 「进度指示本身有用」的奠基是 Myers CHI 1985 percent-done（长任务下用户强烈偏好进度指示）；Conn CHI 1995 提出理想进度条属性与 time tolerance window；二者经 Harrison 文内引注转引（https://chrisharrison.net/projects/progressbars/ProgBarHarrison.pdf ）。

### never-decrement 与不重置

**微软 Win32 UX Guide 明文两条。** "Don't back up progress… Always increase progress monotonically"（进度不许倒退、必须单调增）与 "Don't restart progress"（多步任务不许重置条——各步按占比共享同一根条、一次性走满；重置后用户无法估算剩余）。出处：https://learn.microsoft.com/en-us/windows/win32/uxguide/progress-bars

**Fluent 2 现行口径。** "When the bar appears to move backward, people lose confidence in it"，并给正例：检查更新 → 安装更新共用一条条，阶段切换只换状态文案、从同一点继续前进。出处：https://fluent2.microsoft.design/components/web/react/core/progressbar/usage

**数学佐证。** WebKit 的 ProgressTracker 增量恒非负、只 clamp 上限，结构上保证单调（见下条与源码注释）。出处：https://github.com/WebKit/WebKit/blob/main/Source/WebCore/loader/ProgressTracker.cpp

### 90%/99% 停滞现象与对策

**现象被官方点名。** Win32 UX Guide 原文："progress bars that immediately shoot to 99 percent completion, then stay there for a long time are particularly uninformative and annoying"，并给量化对策：此类场景初值至多 33%，让条始终能表达「处于开头/中段/结尾」——与 Harrison 的 Early Pause 容忍同构。出处：https://learn.microsoft.com/en-us/windows/win32/uxguide/progress-bars

**对策族（全部一手）。** 非线性刻度提示更长等待；不满格直到真完成（"Don't let a progress bar go to 100 percent unless the operation has completed"）；标签动态换当前任务（如 Copying… 变化，用户才不会以为锁死）；超过 10 秒给剩余时间估计且至少 5 秒一刷（Win32 + NN/g 双出处）；总量未知先用 indeterminate、可测后转 determinate；超过 2 分钟改后台通知不打扰。出处：https://learn.microsoft.com/en-us/windows/win32/uxguide/progress-bars 、https://www.nngroup.com/articles/progress-indicators/

**工程化封顶。** WebKit 把终值上限定 0.9，源码注释："always leave space at the end. This helps show the user that we're not done until we're done"——满格只属于完成事件。出处：https://github.com/WebKit/WebKit/blob/main/Source/WebCore/loader/ProgressTracker.cpp

### 事件驱动 + 时间插值混合模型

**WebKit 一手实现（浏览器加载条的公开原型）。** 初值恒 0.1（注释 "Always start progress at 0.1… helps provide feedback as soon as a load starts"）；每笔字节到达时 increment =（上限 − 当前值）× 本次字节占剩余估计比——对真实事件做渐近插值、永不触顶；首次 layout 完成前上限压到 0.5（把排版当里程碑）；100 毫秒心跳判停滞（连续 4 拍每拍不足 1 KiB 判 stalled）；UI 通知 200 毫秒节流；finalProgressComplete 才一次性置 1。出处：https://github.com/WebKit/WebKit/blob/main/Source/WebCore/loader/ProgressTracker.cpp

**与 Harrison 建议同构。** 0.1 起跳 = 向前补偿 position jump；渐近减速 = 结尾留白不冒进；真实事件（字节、layout、完成）= 跳正锚点。启动场景的等价物——systemd 事件面 + plymouth「时间渐近插值 + 跨启动校准」——已在 [plymouth 调研](2026-11-14-survey-plymouth-systemd-boot-splash.md) 源码级实证，结论一致。

### 假进度 vs 真事件的取舍

**Harrison 划界（论文讨论节原文）。** 完成条件静态已知、进度稳定的任务（拷贝文件、扫描照片、播放音频）不该上感知把戏——标准线性条配准确剩余时间即可；完成条件动态、时长只能粗估的任务（碎片整理、安装）才做感知增强（缓存进度、结尾加速、阶段重排）。出处：https://chrisharrison.net/projects/progressbars/ProgBarHarrison.pdf

**官方阈值。** Win32：约 5 秒内用 activity indicator 不用进度条、2 秒以上必须有反馈、bounded 任务即便估不准也用 determinate（估不准不是用 indeterminate 的理由）；NN/g：超过 10 秒要给估计，内布拉斯加大学-林肯分校研究显示有动态进度条的用户平均愿多等约 3 倍时间。出处：https://learn.microsoft.com/en-us/windows/win32/uxguide/progress-bars 、https://www.nngroup.com/articles/progress-indicators/

**映射到启动。** 系统启动事件可枚举（内核 → init → 服务 → 图形栈），属「bounded」类——QNX/AGL/ChromeOS 都把 splash 绑真实启动序列；web 页加载总量不可知才需要纯渐近假进度。BerryOS 的「真事件映射 + 区间插值」站在两类之间，应把可枚举事件当锚、插值只补事件间空隙。

### 启动画面时长下限与仪式感

**历史视角。** Adafruit 指南：启动画面最初是纯功能（报告启动状态），后来厂商与市场部门发现这块「第一印象地产」的品牌价值——通常 20-30 秒以内、含 logo 与音效，还顺带展示图形能力。出处：https://learn.adafruit.com/startup-screens/what-are-they

**厂商仍在投资仪式。** Xbox 2026 年 5 月推送全新启动动画、并允许用户选用历代经典动画（怀旧成选项）；PS5 首启 CGI 独立制作、启动音做进游戏彩蛋；玩家社区希望 PS5 首启动画常驻。出处：https://www.slashgear.com/2256911/xbox-nostalgic-startup-animations-series-x-s/ 、https://www.avid.wiki/PlayStation_5/Startup_Screens 、https://www.reddit.com/r/PS5/comments/jvto3k/i_wish_the_ps5_first_time_startup_happened_all/

**反向张力：快启时代在砍动画。** Windows 8 快速启动（关机即休眠 session 0、多核并行读 hiberfile、UEFI 缩短 pre-boot）把测试系统冷启 25-72 秒压到 15-33 秒、SSD 演示约 8 秒——启动快到动画放不完全是真实工程状态。出处：https://learn.microsoft.com/en-us/archive/blogs/b8/delivering-fast-boot-times-in-windows-8/

**「太短显得廉价/不可信」。** 未找到任何学术或厂商一手资料支持该命题（未证实）；证据面是双向的——QNX 车机追求极早出画面（工程压倒仪式），Xbox/PS5 反向加码动画（仪式即品牌），说明时长下限是产品定位决策而非普适规律。BerryOS 模拟器「打开即引导」宜默认快路径，动画时长下限若要设，当以 PS5 的两档（冷启全仪式 / 唤醒轻仪式）为参照做产品决策。

## 适用边界

未证实清单：Steam Deck 启动时长官方数字与固件 logo 的 BGRT 续画用法（BGRT 通用机制见 [plymouth 调研](2026-11-14-survey-plymouth-systemd-boot-splash.md) ，Deck 专门文档缺失）；Switch logo 约 1 秒时值与任天堂「极简哲学」的一手表述；PS5 启动秒数与「白点」官方命名；ChromeOS 正常路径固件屏的官方定义与其 OOBE 百分比；「启动动画太短显得廉价」命题；「It's not nihilism」论文名；nprogress/pace 等 web 假进度库细节（仓库存档路径失效未展开）。检索通道受限（web_search 401，DDG lite 兜底），Reddit/VGC/StackExchange 引文以检索摘要为准；avid.wiki、switchbrew、技术博客为社区维护，时效以各站为准。本文结论截至 2026-11-14。
