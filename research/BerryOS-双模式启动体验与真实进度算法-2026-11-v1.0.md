# BerryOS 双模式启动体验与真实进度算法调研

Type: survey
Status: current
Updated: 2026-11-15

一句话说明：对照 Linux（quiet/splash + Plymouth）、macOS（verbose 启动与 Apple 标志进度条）、Android（bootanimation 与 boot_progress 里程碑）、OpenHarmony（bootanimation 与 init 三阶段）的一手官方材料，梳理「详尽文本模式 vs 图形标志 + 白色胶囊条」双模式与「真实里程碑加权进度」的干法，落到 BerryOS 现有 BootProgress P1 框架的里程碑清单、缺口、算法与验收标准。

## 1. 方法与证据纪律

本调研全部结论来自一手官方文档或官方源码直读（kernel.org、man7.org、freedesktop.org、Apple Support、source.android.com、android.googlesource.com AOSP 源、OpenHarmony 官方 GitHub 镜像），每条论断附 URL；会话内 web_search 工具不可用（鉴权 401），所有 URL 均以直接抓取核对过内容。本地结论标工作树路径（gui-1080p）。引用原句只取短语级，不复制长段私有文本。OpenHarmony 的 Gitee 源有反爬墙，改用官方 GitHub 镜像（内容同源）。

## 2. 来源清单

| 编号 | 来源 | URL | 用途 |
|---|---|---|---|
| K1 | Linux 内核参数官方文档 | https://docs.kernel.org/admin-guide/kernel-parameters.html | quiet / loglevel / boot_delay / logo.nologo / earlycon |
| K2 | 内核参数原文（torvalds 树） | https://raw.githubusercontent.com/torvalds/linux/master/Documentation/admin-guide/kernel-parameters.txt | 行号级核对：quiet@L5568、loglevel=@L3705、boot_delay=@L646、logo.nologo@L3730 |
| K3 | man7 kernel-command-line(7) | https://man7.org/linux/man-pages/man7/kernel-command-line.7.html | quiet/debug 双读者（内核 + systemd）、plymouth.enable=、systemd.show_status= |
| P1 | Plymouth 官方 wiki | https://www.freedesktop.org/wiki/Software/Plymouth/ | 启动早期图形动画、DRM 同模式防闪烁、Esc 看日志、boot.log、发行版喂数据 |
| A1 | Apple：Mac 启动键组合 | https://support.apple.com/en-us/102603 | Command-V 详尽模式（Intel Mac） |
| A2 | Apple：Apple 标志或进度条 | https://support.apple.com/en-us/102535 | 进度条出现时机、更新期慢速/停顿、卡死处置 |
| A3 | Apple：启动各屏幕总览 | https://support.apple.com/en-us/102675 | 锁图标/登录窗口/FileVault 中途交互 |
| N1 | AOSP：Bootloader overview | https://source.android.com/docs/core/architecture/bootloader | 验证 → 装载内核/ramdisk 流程 |
| N2 | AOSP：BootAnimation.cpp（main 分支） | https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/cmds/bootanimation/BootAnimation.cpp | 进度属性读取、防跳变、退出强置 100、多显示与旋转 |
| N3 | AOSP：bootanimation FORMAT.md | https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/cmds/bootanimation/FORMAT.md | desc.txt 的 `WIDTH HEIGHT FPS [PROGRESS]` 与段落类型 |
| N4 | AOSP：WindowManagerService.java | https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/wm/WindowManagerService.java | enableScreenAfterBoot 置 service.bootanim.exit=1 |
| N5 | AOSP：EventLogTags.logtags | https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/EventLogTags.logtags | boot_progress_* 里程碑编号 3010..3100 |
| N6 | AOSP：ZygoteInit.java | https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/core/java/com/android/internal/os/ZygoteInit.java | boot_progress_preload_start/end |
| N7 | AOSP：SystemServiceManager.java | https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/SystemServiceManager.java | 启动阶段梯（onBootPhase/PHASE_BOOT_COMPLETED） |
| H1 | OpenHarmony：boot_animation.cpp | https://github.com/openharmony/graphic_standard/blob/master/frameworks/bootanimation/src/boot_animation.cpp | 帧动画、退出条件参数、无进度条 |
| H2 | OpenHarmony：startup_init README | https://github.com/openharmony/startup_init/blob/master/README.md | init 三阶段作业梯与进程守护 |
| B1 | BerryOS 现行规格 | [docs/BerryBoot-BootProgress-P1-v1.0.md](../docs/BerryBoot-BootProgress-P1-v1.0.md) | BootProgress P1 权重与守门 |

## 3. 四系统启动 UX 对照

| 维度 | Linux（含 Plymouth） | macOS | Android | OpenHarmony |
|---|---|---|---|---|
| 详尽/图形选择 | 内核命令行 quiet（关掉多数日志）、loglevel=0..7 过滤、earlycon 先行控制台；systemd 侧同读 quiet/debug；plymouth.enable=0 直接禁 splash（K1/K2/K3/P1） | 开机按住 Command-V 进详尽模式（仅 Intel Mac；Apple Silicon 只有启动选项屏幕，无 verbose 开关）（A1） | 引导器内核命令行（androidboot.*）；用户层无每启动切换键；动画本身可被系统属性控制（N1/N2） | 启动参数/cfg 作业决定；开源参考实现无用户可选 verbose |
| splash 与文本关系 | Plymouth 在图形与文本两态都遮住启动消息；根文件系统可写挂载后消息落 /var/log/boot.log；Esc 运行中切换消息视图（P1） | 详尽模式整屏替换图形界面（Apple 标志 + 进度条），二者互斥（A1/A2） | bootanimation 是 SurfaceFlinger 的 EGL 客户端，画在内核/固件 logo 之后；背后有文本控制台（N2） | BootAnimation 是 Rosen 窗口（WINDOW_TYPE_BOOT_ANIMATION），黑底 Skia 绘制（H1） |
| 进度真值来源 | 发行版「poke plymouth」喂真实事件；主题多为定时动画，进度可选（P1） | 未公开；官方只说「启动继续时出现进度条」（A2） | 真值 = boot_progress_* 事件梯 + SystemService 启动阶段；显示侧读 service.bootanim.progress（0..100），动画帧本身是定时的（N2/N5/N6/N7） | 无进度条；纯定时 PNG 帧序列（bootpic.zip，30/60fps）（H1） |
| 阶段/进度算法 | 主题脚本 + 发行版集成决定保真度；内核侧无进度真值 | 不公开；条粗粒度，官方承认会跳/停 | desc.txt 第 4 域开百分比显示（末段）；每帧读属性；防跳变 = 每帧最多 +1%（100ms 人为睡眠）；退出时强置 100 再收尾（N2/N3） | init 三阶段作业（early/init/post-init）+ bootevent.* 参数；动画停在末帧轮询退出（H1/H2） |
| 完成/交接 | Plymouth 保持到显示管理器接管，全程同一 DRM 模式消灭闪烁（P1） | 进度走完 → 登录窗口；FileVault 密码可在中途出现（A2/A3） | WMS enableScreenAfterBoot（启动窗口就绪后）置 service.bootanim.exit=1；动画淡出（f 段）后退出（N4） | 轮询 persist.window.boot.inited==1 → 停 runner 毁窗口；先发 bootevent.bootanimation.started=true（H1） |
| 旋转/分辨率 | 原生 DRM 模式全程保持；文本回退走 /dev/fb（P1） | 固件定死；无旋转 | 多显示支持；每显示旋转属性 ro.bootanim.set_orientation_<id>（0/90/180/270 及 logical 变体）；按 ratioW/H 缩放居中（N2） | InitPicCoordinates 取短边做正方形动画居中黑边；窗口取当前显示宽高；无旋转事件（H1） |
| 取消/用户交互 | Esc 切换日志视图（运行中） | 无（卡死时电源键 10 秒强关，A2） | splash 上无交互；音量键在引导器层 | 无 |
| 无障碍 | 依赖主题；文本回退即无障碍路径 | 启动屏无 | 手表可在 splash 画时钟（desc.txt CLOCK 坐标）；百分比字体 progress_font.png（N2/N3） | 开源参考无 |
| 假进度/跳变/停顿坑 | 主题假进度常见；消息被遮会藏故障直到 Esc | 官方承认更新期「进度条可能缓慢移动并长时间停顿」；屏幕会与黑屏交替数次（A2） | 属性跳变靠 +1%/100ms 限速消化；退出强置 100 属「化妆性收尾」；用户态重启用关机动画（N2） | 纯时间帧：init 慢时动画循环/停末帧零信息；退出靠逐帧轮询参数（H1） |

## 4. 各系统要点细读

Linux：quiet 的官方定义就是「Disable most log messages」（K2 L5568），loglevel= 是控制台日志级别过滤器（0 KERN_EMERG 到 7 KERN_DEBUG，K2 L3705），boot_delay= 给每条 printk 加毫秒延迟便于人工读（K2 L646），logo.nologo 关掉 fbcon 左上角内核 logo 以腾出日志空间（K2 L3730）——即内核侧「详尽/安静」是日志过滤问题，图形 splash 完全交给用户态。Plymouth 在根分区挂载前就起（打进 initrd），用 DRM 原生模式并全程保持以消灭闪烁，没有 DRM 时回退文本模式；两态下启动消息都被遮，事后落 /var/log/boot.log，用户随时按 Esc 看；进度靠发行版「poke」，主题自由度大也正因此假进度泛滥（P1）。

macOS：Command-V 详尽模式是 Intel Mac 的启动键组合（A1）；详尽模式是整屏文本控制台，不是叠加层。图形态官方描述：找到启动盘后显示 Apple 标志，「启动继续时会出现进度条，带或不带 Apple 标志；这块屏幕可能与黑屏交替数次」；装更新/升级时「进度条可能比平常持续久得多，可能缓慢移动并长时间停顿」，官方建议晚间安装过夜；卡死判据是「从未越过 Apple 标志或进度条」→ 电源键 10 秒强关再排查外设（A2）。FileVault 开启时登录窗口即解锁磁盘——启动中途允许一次真实用户交互（A3）。

Android：bootanimation 的 zip 搜索序为 oem → product（含暗色变体）→ system → apex（N2）；desc.txt 首行 `WIDTH HEIGHT FPS [PROGRESS]`，第 4 域非 0 才在「最后一段」显示百分比（x 居中、y 取动画高 1/3，N3）；段落类型 p=可被启动完成打断、c=必须播完、f=被打断时按指定帧数淡出，COUNT=0 表示循环到启动完成（N3）——即动画时长与真实启动解耦。真值侧是事件梯：3010 boot_progress_system_run、3060..3100 包扫描四点（N5）、preload 两点（N6）、SystemServiceManager 的 onBootPhase 阶段梯到 PHASE_BOOT_COMPLETED（N7）。显示侧每帧读 service.bootanim.progress，跳变时仍「每次只 +1」并人为睡 100ms（约 10%/s 上限）；退出挂起时若进度非 0/100 先强置 100 再收尾（N2）；退出条件由 WMS enableScreenAfterBoot 置 service.bootanim.exit=1（N4）。

OpenHarmony：开源参考实现（graphic_standard）读 /system/etc/init/bootpic.zip 帧序列，vsync 驱动，帧率配置只认 60/30（CheckFrameRateValid）；满帧后逐拍 CheckExitAnimation：首次置 bootevent.bootanimation.started=true，随后轮询 persist.window.boot.inited==1 才停（H1）。动画按短边做正方形居中、黑底 letterbox，无旋转处理、无进度条——init 慢时用户看到的是零信息循环。init 侧是三阶段作业梯（early/init/post-init）加服务守护重启（H2）。商用 HarmonyOS 开机动画闭源，公开材料止于此。

## 5. BerryOS 现状：可用里程碑与缺口

现行规格见 [docs/BerryBoot-BootProgress-P1-v1.0.md](../docs/BerryBoot-BootProgress-P1-v1.0.md)，真值模型在 [berry-bootprogress](../bootloader/berryboot/berry-bootprogress/src/lib.rs)：permille 0..=1000、span(start,end,done,total) 对零总量/超额/区间倒退一律失败闭嘴、渲染器拒绝回退、全程无墙钟插值。以下里程碑逐点核对过源码，全部是状态谓词触发。

装载器纪元（[berryboot/app/src/lib.rs](../bootloader/berryboot/app/src/lib.rs)，面 = ramfb/GOP）：

| permille | Stage | 触发谓词 |
|---|---|---|
| 0 | Display | mode-set 完成（fw_cfg opt/berry/display，产品 1080×1920；打 BOOTPROG ok display WxH） |
| 30 | Verify | HELLO.TXT 逐字节对拍通过 |
| 30..300 | Kernel | span(30,300,got/fsize)，KERNEL.ELF 按 1MiB 粒度读 |
| 300..800 | Ramdisk | span(300,800,done/isize)，INITRD.BIN 分段读 |
| 800 | Prepare | init 读毕；820 = ELF 解析过；820..900 = span 逐段拷贝 |
| 900 | Handoff | handoff 页写毕；930 = CurrentEL 预检过；950 = GetMemoryMap 过；1000 = EBS 过 + PL011 打 BERRYBOOT ok + dcache 清刷后跳 entry |

系统纪元（[svcmgr](../user/bins/svcmgr/src/main.rs) 与 [gui-wmd relay](../user/bins/gui-wmd/src/relay.rs) 经 OP_BOOT_PROGRESS 喂 [vgpud](../drivers/bins/vgpud/src/main.rs) 私有 VMO boot surface）：

| permille | 常量 | 触发谓词 |
|---|---|---|
| 450 | M_VGPU_READY | vgpud 起活 + vgpu_ready +（有 gpuhdi.on 时）HDI 自测过 |
| 500 | M_PRESENT_BACKEND_READY | gui-wmd spawn Ok + per-boot seed ack 后 |
| 520 | M_WMD_SPAWNED | 同上（相邻两连发） |
| 560 | M_INPUT_ARMED | wmd 装配臂完成 |
| 620 | M_INPUTD_SPAWNED | gui-inputd spawn Ok |
| 700 | M_PROGRESS_CONTROL_HANDOFF | svcmgr 关私有 dup，控制权交 wmd |
| 800 | M_SYSTEM_UI_CONNECTED | SYSTEM launcher WM_CONNECT 过 per-boot seed（[guest.rs](../user/bins/gui-wmd/src/guest.rs) SYSTEM_UI_PROGRESS_READY） |
| 900 | M_FIRST_REAL_SUBMIT | 首个 SYSTEM surface-0 SUBMIT 过几何/尺寸硬门 |
| 1000 | M_DESKTOP_HANDOFF | 聚合桌面 SUBMIT 转发前一刻；随后首帧自然替换 boot surface |

缺口（按影响排序）：

1. 内核纪元零真值：EBS 后到 vgpud 450 之间没有任何上屏生产者；内核标记（HANDOFF ok、FB ok、DISP ok）只走串口（[kernel/src/main.rs](../kernel/src/main.rs) GUI 模式 console 不上屏）。
2. 纪元接缝回退：装载器画满 1000，vgpu 面从 450 重起——用户看到满条回跳 45%（现行计划文档把宿主单 console 切换归 P2，但 permille 重置本身是真值模型问题）。
3. 无屏上详尽面：全部诊断走串口；失败时图形面只会停在冻结条，无文字线索，也无自动转详尽。
4. 启动期无用户交互：没有 Plymouth 式 Esc 切换；inputd 620 才存在。
5. 无无障碍变体：百分比文字被有意排除（host 测试钉住条下黑场），无大字/声音选项。
6. Verify 段只有 30 permille 的 HELLO.TXT 对拍，无真实磁盘校验权重。
7. 无停顿处置：条冻结即沉默，无停顿计时器、无屏上失败态。
8. 分辨率/旋转：layout() 原生支持竖屏 1080×1920 与横屏 800×600，最小 480×480、请求帽 4096、仅 XRGB8888（raw 0），无动态旋转、其他格式一律拒绝（fail-closed）。
9. 化妆瑕疵：berry-bootprogress 里 PERMILLE_CEILING 与 PERMILLE_MAX 同值重复。
10. 无关机/重启进度面（Android 有 shutdownanimation 可参）。

## 6. 双模式架构建议

保留现有单一真值模型，加一个「表现位」而不是分叉两套进度：

- 模式源：沿用 BerryBoot 已有的 fw_cfg 读取（opt/berry/display 先例）扩一个 opt/berry/bootmode ∈ {graphical（默认）, verbose, auto}；auto = 图形态直到任何失败诊断行锁存转详尽（把 Plymouth 的 Esc 哲学改成自动版，fail-loud 把失败行亮到屏上）。运行中 Esc 切换只能在 M_INPUT_ARMED（560）后实现（此前 inputd 不存在），列为 P2。
- Face 不分叉真值：berry-bootprogress::Renderer 增 Face 枚举 {Capsule, VerboseText}，initialize() 时选定；两面消费同一 (Stage, permille) 流。VerboseText 复用该 crate 已链接的 berry_fbcon 字形：一行 Stage 标签（Stage::label() 现成）+ 小号胶囊 + 最近 N 条诊断行（BOOTPROG、fs:、kfile:、init:、elf:、load:、mmap:、ebs:、BERRYBOOT ok、HANDOFF ok、FB ok、DISP 等，正是现在 print_line/uart 已产出的串）。span 的 fail-closed 与单调契约共享，绝不按 face 分叉。
- 诊断总线：装载器侧给 ProgressRuntime 加一个小环形缓冲收诊断行；内核早期日志同理；系统纪元把 vgpud/wmd/svcmgr 已打印的行喂进 boot VMO。
- 纪元连续性（核心修复）：全启动一条单调 permille。装载器改封顶 390（内核入口 ≠ 1000），系统里程碑映射到 400..1000（450 起步保留，390..450 成为有文档的内核纪元 dwell）；并把装载器终值经 BerryHandoff gop_* 块捎带一个 u16（[kernel/src/handoff.rs](../kernel/src/handoff.rs) 已带版本），vgpud 首帧 initialize 于 max(到达值, 携带值)，从「同一填充度」起画而不是归零重画。
- 失败策略：谓词失败后任何生产者不得前进（现已是）；失败即冻结胶囊（无假爬行）、auto-verbose 亮诊断；「等待」指示只允许文字形态（如闪烁光标），绝不允许条的移动。
- 关机/重启面预留：Face 化设计让 shutdown face 未来可复用同一 crate。

## 7. 真实阶段加权进度算法

在现有真值脊柱（span 失败闭嘴、里程碑严格单调、渲染器拒回退）之上加显示侧滤波与权重规则：

1. 权重来自实测中位数：各阶段 permille 跨度 ∝ 最慢支持目标（HVF 产品 + TCG 调试）上的中位墙钟；smoke 每次都给标记打时间戳，可由工具从串口时间差自动推导权重并防漂移（偏差超 20% 即 CI 警告）。现有 KERNEL 30..300 / INITRD 300..800 字节权重是正例；内核纪元应补真实子里程碑而不是宽 dwell。
2. 显示限速（Android 验证过的干法）：displayed_permille 向 truth_permille 收敛，每拍最多前进 max(1%, Δ≥8% 时直接 snap）——小步真相以 ≤10%/s 爬行不瞬移；大里程碑跳变 ≥8% 立即贴齐（快启动：HVF 秒级到桌面也能在 handoff 显出真值）。实现为纯函数 slew(prev, truth, dt) 可 host 测试。
3. 单调性：displayed = max(displayed, slew(...))；即使生产者因重试回退字节计数也不回退显示（渲染器已忽略回退，滤波同契约）。
4. dwell：图形态 1500ms 无真值前进则条保持不动（像素差为空）——只有真值，无假爬行；verbose 面可让阶段标签慢脉冲。dwell 超 10s 触发 auto-verbose 锁存 + 串口诊断，把沉默停顿变成可见停顿（Apple 更新期停顿的教训）。
5. 封顶：900..1000 只属于 SYSTEM surface-0 事件（M_FIRST_REAL_SUBMIT、M_DESKTOP_HANDOFF）——常量已如此编码，再加 debug_assert 禁止 wmd relay 以下生产者发 ≥901。
6. 快启动行为：里程碑可能突发成簇；限速器必须在 truth==1000 时收敛（snap 规则保证完成帧永不迟到）。
7. 慢启动行为：长 INITRD（帽 128MiB）已有 1MiB 粒度字节真值；全程无墙钟插值；内核纪元补 ≥2 个真实生产者（handoff 解析完成、rootfs/init 装配）而非定时爬行。
8. 完成交接：1000 只在桌面交接（relay once-only AtomicBool 已保证恰一次）；首个 wmd SUBMIT 自然切走 scanout（现行为已对）。

## 8. 验收标准

Host 纯函数（berry-bootprogress tests）：

- A1 单调梯：里程碑数组严格递增且末项 1000（已有）；扩展为性质测试——任意 (done/total) 不减序列经 span 产出不减 permille。
- A2 slew：(a) 永不回退；(b) 每拍增量 ≤ 限速、Δ≥8% 直接贴齐；(c) truth==1000 时下一拍收敛；(d) dt 大而 truth 不变 ⇒ 输出不变（无假爬行）。
- A3 Face：initialize(VerboseText) 画阶段标签 + ≥1 条诊断行；Capsule 面条下零文字行（推广现有黑场测试）。
- A4 布局：1080×1920、800×600、720×1280、1280×800 四几何过；<480 与非 XRGB 拒（扩展现有拒面测试）。

Boot/QEMU（产品镜像 smoke 守门）：

- B1 图形默认：首屏 = 黑底 + 白 wordmark + 0 胶囊；序列恰为 450/500/520/560/620/700/800/900/1000 后 LAUNCH ok；1000 只在首个 SYSTEM surface-0 SUBMIT 之后（串口标记顺序断言）。
- B2 纪元连续（修复后）：handoff 捎带值生效；vgpu 首帧胶囊填充 ≥ 装载器终值；接缝前后截图条区域无回退。
- B3 verbose 模式：fw_cfg opt/berry/bootmode=verbose ⇒ 屏上出现诊断文本行（行数 ≥ 内核入口前串口诊断行数）且胶囊在；纯图形布局像素缺席。
- B4 auto-verbose：破坏 HELLO.TXT fixture ⇒ 失败锁存详尽面并可见 fs: mismatch 行；胶囊冻结于末值（≤30）；串口有 FAIL 标记；此后零 VGPUBOOT 推进行。
- B5 dwell：停顿注入（慢 initrd 或 QEMU 暂停）⇒ 图形态两帧截图（间隔 ≥2s）像素差为空。
- B6 快启动（HVF）：全里程碑 <1s 到桌面 ⇒ 末帧条 100% 与 LAUNCH ok 同帧邻，无可见限速滞后。
- B7 回退拒收：向 vgpud 发更小 permille 的 OP_BOOT_PROGRESS ⇒ VGPUBOOT fail 行 + 像素不变（守护接缝）。
- B8 权重漂移：CI 比对实测标记时间差推导权重与编译常量，偏差 >20% 警告。

手工/产品：

- C1 Esc 切换（P2）：inputd 就绪后注入 Esc ⇒ face 切换标记 + 截图。
- C2 1080×1920 竖屏原生无 letterbox、条 756px @ y≈1305（现有布局测试 + Cocoa 窗口目验）。

实施落点：[berry-bootprogress](../bootloader/berryboot/berry-bootprogress/src/lib.rs)（Face + slew + 权重常量）、[berryboot/app](../bootloader/berryboot/app/src/lib.rs)（ProgressRuntime 诊断环 + 模式读取 + 捎带值）、[kernel/src/handoff.rs](../kernel/src/handoff.rs)（携带字段）、[svcmgr](../user/bins/svcmgr/src/main.rs) + [gui-wmd relay](../user/bins/gui-wmd/src/relay.rs)（系统纪元接续）、[vgpud](../drivers/bins/vgpud/src/main.rs)（首帧 initialize 于携带值）、规格升版 [docs/BerryBoot-BootProgress-P1-v1.0.md](../docs/BerryBoot-BootProgress-P1-v1.0.md) → v1.1（按仓库版本规则改名同步索引）。
