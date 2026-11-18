# GLOSSARY — 术语表（种子）

本表统一工作区术语：**一概念一词**，用新词前先查、写进正文前先登记。条目格式：`**术语（英文）**：定义。_Avoid_: 易混叫法`；`_Avoid_` 只作命名指引，机器不查。

## 制度

**Curia（枢机院）**：本套文档管理制度的总称——两根布局、10 类文档、四句口诀与轻量自检脚本；定位为可移植到任何项目的文档管理方案。_Avoid_: 文档平台、知识库系统

**两根（two roots）**：受管文档的封顶布局——`.agents/` 装过程与过程产物，`docs/` 装外部事实、自由知识与现行标准。_Avoid_: 双目录方案

**四句口诀**：放哪的最后裁决——没做的→`plans/`、做完的→`research/`、为什么→`notes/`、过时的→翻状态或归档。_Avoid_: 目录规范

**家（home）**：类型对应的固定归属目录——`.agents/` 下五个（`plans/`、`research/`、`notes/`、`skills/`、`archive/`）；`docs/` 是 `standard` 的家。_Avoid_: 分类目录

**类型化文档（typed doc）**：带 `Type:` / `Status:` / `Updated:` 三行头块、按 10 类之一定义、住对应家或 `docs/` 的文档；`docs/` 只允许 `standard`，目录说明文件、笔记、`skill` 与 `docs/` 自由知识不适用。_Avoid_: 正式文档

**头块（header block）**：文档开头的固定字段行块——类型化文档三行固定（`standard` 另加 `Version:`），笔记两行（标题与 `Status:`），`skill` 用 frontmatter；`docs/` 自由知识无头块。_Avoid_: frontmatter（只有技能用）

**标准（standard）**：`docs/` 下带 `Type: standard`、`Version:` 与 `Status:` 的现行契约——设计规范、UI 设计系统、API/数据契约、编码约定、写作与排版约定；状态取 `current` / `historical` / `superseded`。_Avoid_: 规格（`spec` 住 `plans/`）、普通说明

**docs 自由知识（free-form docs）**：`docs/` 下没有 `Type:` 的文档——外部事实、接入说明、架构导览与 FAQ；无状态机，当前文件即真相。_Avoid_: 把所有 `docs/` 文档都叫无头文档

**标准候选（standard candidate）**：`--suggest-standards` 按文件名与头块线索列出的、可考虑升成 `docs/` `standard` 的自由知识；只是提示，升格由人确认，检查器不改写。_Avoid_: 准标准、待定标准

**契约版本（contract version）**：模块设计文档与 `docs/` 标准头块里的 `Version: MAJOR.MINOR.PATCH`——记这份文档自己的契约演进；与包版本、取代链各管一件事。_Avoid_: 文档版本、发布版本

**笔记（note）**：`notes/` 下的决策因果记录，头块两行，取值与所在 lifecycle 目录一致。_Avoid_: 日记、流水账

**桥（bridge）**：把 `.agents/` 暴露给各 CLI 的入口（`CLAUDE.md`、`.claude/skills`、`.codex/skills`）——`init` 在目标路径空闲时建符号链接，被占用时保留原物并打印手工命令。_Avoid_: 适配层、同步脚本

**工作区健康（health）**：`--health` 的只读体检——报告载荷齐全度、三条桥状态、`AGENTS.md` 合并态与 docs 标准/候选计数；不改文件，正常退出 0。_Avoid_: 自检报告、状态页

**本地补丁基线（local patch baseline）**：本地修改检查器前记录的 create-curia 版本与检查器 sha256（前 8 位），升级时用来区分上游改动与本地补丁。_Avoid_: 校验和、指纹

**技能（skill）**：`skills/<name>/SKILL.md`——按需加载的工作流；frontmatter 的 `description` 决定触发时机。_Avoid_: 提示词模板、脚本

**归档四步**：移动、加 `Archived:` 行、重写自身相对链接、修入链；第 3、4 步缺一步都会留下断链或静默错链。_Avoid_: 备份、冷存储

**取代（superseded）**：文档被取代后的终态；必须带 `Superseded-by:` 指向取代者，不得迁出。_Avoid_: 废弃、作废

**存量树只读纳管**：采用本规范时存量文档保持原样、规则不回溯、下次实质修改时自然补齐。_Avoid_: 迁移、批量修复

**原始语料豁免（raw marker）**：目录内放空标记文件 `.agents-raw` 即整目录免检，用于外部镜像等批量采集物。_Avoid_: 白名单、忽略目录

## 写作

**slop 清单**：写作自查的九类脏东西（重复规则、状态注解、推理记录等），见 [prose-standard](./skills/prose-standard/SKILL.md)。_Avoid_: 检查表

**三档示例法**：校准写作规则的方法——每条规则配「过度裁剪 / 平衡 / 过度展开」对照。_Avoid_: 正反例

## 引导、微内核、调度与诊断

**引导装载器（BerryBoot）**：系统 Stage 1/2 引导与硬件交接器，执行 A/B 槽位仲裁与 Verified Boot 签名度量延伸。_Avoid_: GRUB、裸固件引导

**启动交接帧（BerryHandoff v2）**：Bootloader 向微内核 `meihe` 传递的单物理页（4KiB）只读引导上下文（内存切片、显示帧缓冲基址、硬件随机数种子与 16KiB Ramoops 控制块）。_Avoid_: 启动命令行参数字符串

**莓核（Meihe）**：BerryOS 的纯能力 16 核无大锁微内核底座，仅承载地址空间、线程调度、能力句柄、通道/端口 IPC 与最小硬件中断/IOMMU 门卫。_Avoid_: 宏内核、混合内核

**端口（Port）**：内核一等 $O(1)$ 异步事件多路聚合对象（承载信号观察者、中断与定时器投递），取代固定上限的 `wait_many` 轮询表。_Avoid_: epoll 表、全局事件总线

**作业（Job）**：包裹一组进程并强制执行物理内存硬配额、进程数上限与级联终止/冻结的层级沙箱容器对象。_Avoid_: cgroup、进程组

**受限态（Restricted Mode）**：内核通过 `0x4A/0x4B` 提供的用户态硬件陷阱分层执行机制，在无内核特权情况下仿真原生 Linux ELF 系统调用。_Avoid_: 内核态 Linux 驱动补丁

**帧令牌（FrameToken）**：合成器在每个 VSync 节拍向当前交互窗口配发的受控单拍 6ms EDF 硬实时截止期与大核亲和继承凭证。_Avoid_: 永久实时特权、客户端自授高优

**墓碑（Tombstone）**：应用在 L3 内存回收前保存 ≤ 256KiB 页面导航状态与缩略快照并落入加密存储、待重开时原地无感恢复的生命周期机制。_Avoid_: 用「墓碑」指代崩溃转储记录（崩溃记录统一称黑匣子 `cbox`）

**黑匣子（cbox）**：跨 EL0 用户异常、EL1 内核 `ramoops` 热重启遗言及跨进程 ANR 无响应死锁全线程抓栈的故障取证账本体系。_Avoid_: 崩溃墓碑、裸串口转储

**进程外编译服务（aotd / jitd）**：在严格硬件 `W^X` 约束下，于零网络独立沙箱内将 Wasm/JS 编译为机器码、刷净指令缓存并调用 `vmo_seal` 永久剥离写权后只读移交的系统服务。_Avoid_: 沙箱内 RWX 自修改代码

## 图形与客户端 SDK

**场景图合成器（berry-compositor）**：统一执掌分层场景图、每客户端独占表面会话、双模表面（`SceneStream` 与 `Swapchain`）合成、多屏独立 VSync 与手势/焦点路由的单进程系统服务，合流并取代过渡期分离的 `guifbd` 与 `gui-wmd`。_Avoid_: 帧缓冲中继器、带裁剪服务

**绘制中间表示（SceneIR）**：零堆分配、同时供 CPU 16×16 瓦片 NEON 软光栅、GPU Uber-Shader 硬卸载与原生矢量 PDF 后端同源消费的紧凑 2D 矢量与材质指令集。_Avoid_: 逐像素绘制命令、私有绘图脚本

**场景流表面（SceneStream）**：常规 SDK 应用默认采用的不分配客户端像素帧缓冲、仅向合成器提交结构化 `SceneIR` 增量以节省 90% 显存并支持矢量跨屏流转的表面模式。_Avoid_: 客户端软绘位图上传

**表面交换链（Swapchain）**：面向视频硬解、相机预览与 3D 自绘画布，按窗口实际目标像素宽高分配的 2–3 槽只读共享 VMO/`GpuSurface` 轮转缓冲与显式 `Acquire/Release Fence` 同步机制。_Avoid_: 全屏单帧 VMO、行弹舱拷贝

**虚拟像素（vp）**：与物理屏幕密度解耦的唯一 UI 逻辑度量单位（`px = round(vp * density_q8 / 256)`），所有组件内在几何、间距、圆角与字号均以 `vp` 声明。_Avoid_: 物理像素硬编码、手乘密度常数

**预测性返回（Predictive Back）**：按「收起软键盘 → 关闭系统浮层 → 应用内导航栈跟手侧滑回退 → 根页面窗口平滑缩回桌面图标并转入后台冻结」执行的四级有序返回协议。_Avoid_: 外部直接 `DESTROY` 强拆窗口

**快照速启（QuickBoot Snapshot）**：宿主模拟器基于镜像内容哈希缓存桌面就绪基准 VM 状态、实现 `< 500ms` 瞬时亮屏唤醒并同步实时时钟的启动机制。_Avoid_: 每次冷跑固件自检

**连续曲率超椭圆（Continuous Squircle）**：曲率二阶连续（$G^2$）的圆角轮廓曲线（Apple `CALayerCornerCurve.continuous` 等价隐式场 $(|x|/a)^n + (|y|/b)^n \le 1$），全系统严禁使用曲率突变的传统正圆弧圆角。_Avoid_: 正圆角、阶梯内缩圆角

**四阶增艳毛玻璃（Vibrancy Material）**：由「降采样 Dual-Kawase 模糊 + 色彩饱和度矩阵提拉（`1.8x`）+ 明暗语义纱幕 + `0.5pt` 高光微晶内描边与前景 Vibrancy 透色混色」组成的 iOS 光学半透明材质管线。_Avoid_: 单纯高斯模糊、死灰半透明色块

**灵动岛实时胶囊（Dynamic Island）**：位于状态栏正中的自适应超椭圆交互胶囊——静默态为紧凑纯黑胶囊，后台存在计时、媒体播放、录屏、传输或 AI 语义代操作时以临界阻尼弹簧实时生长为双槽摘要或展开控制卡片。_Avoid_: 静态刘海块、常驻横幅条

## 存储、安全、发布与救援

**按需只读根卷（BerryROFS）**：4KiB 页对齐压缩并内嵌块级 SHA-256 Merkle 校验树的只读系统根文件系统，取代全量吃内存的 `rootfs.cpio`。_Avoid_: 内存盘 cpio、未验签裸 squashfs

**写时复制文件系统（BerryFS）**：基于 CoW B+Tree 双根超级块原子切换、原生内建 DE/CE 双级文件静态加密（FBE）、每应用目录句柄隔离与 $O(1)$ 快照的现代读写文件系统。_Avoid_: FAT12 数据盘、无事务日志文件系统

**锁屏门禁层（Keyguard）**：驻留合成器最高安全 Z 序、在未通过密码或生物认证令牌（`HardwareAuthToken`）前物理阻断常规应用表面与 CE 敏感数据的系统安全层。_Avoid_: 普通应用态锁屏弹窗

**独立救援系统（BerryRecovery）**：体积 `< 4MiB`、与主系统及用户数据分区故障完全隔离的自包含图形救援环境，支撑双槽全毁重刷与 100ms 毁钥恢复出厂。_Avoid_: 依赖主系统存活的恢复脚本

**断电撕裂竞技场（torn-arena）**：拦截虚拟块设备 `CACHE_FLUSH` 屏障间写请求并穷举任意乱序子集掉电镜像以暴力验证存储崩溃原子性的自动化测试靶场。_Avoid_: 仅测正常卸载路径

**可复现构建（Reproducible Build）**：通过规范化构建环境、固定时间戳（`SOURCE_DATE_EPOCH`）与消除编译残留，保证两次独立构建产出位对位（Bit-for-bit）完全一致二进制镜像。_Avoid_: 仅版本号匹配的伪复现

**软件物料清单（SBOM）**：描述整机操作系统所有二进制与库依赖、版本、SHA-256 哈希与开源许可证的机器可读清单（CycloneDX / SPDX 格式）。_Avoid_: 手工软件列表

## 智能、视听、外设、治理与生产力

**端侧推理守护进程（airtd）**：以全局单份只读按需映射 VMO 跨进程共享 `.bmod` 模型权重、统筹三阶异构张量算子与 Paged KV-Cache 的系统级本地 AI 引擎。_Avoid_: 每应用内置独立大模型推理库

**个人语义记忆库（berry-vault）**：受 FBE CE 密钥与细粒度域能力句柄保护、数据 100% 不出端的本地向量与知识图谱存储。_Avoid_: 明文全局历史库、强制云端记忆同步

**实时音频图服务（audiod）**：运行于周期性 EDF 硬实时调度类、全程零锁零堆分配的无锁共享环低延迟混音与音频焦点闪避服务。_Avoid_: 阻塞式音频混音器

**分布式软总线（berry-link）**：基于同账号 BLE 滚动加密信标近场发现与双向认证 QUIC 多路流的跨设备零配置协同通道（支撑键鼠穿越、隔空投送与 `SceneStream` 矢量跨屏投射）。_Avoid_: 局域网明文广播投屏

**零联网输入法服务（imed）**：在内核 `Job` 层永久剥夺一切网络能力句柄、结合端侧 `airtd` 离线联想听写并在密码框自动退让给内置 `SecureKeypad` 的标准输入法守护进程。_Avoid_: `softimed`（已废弃旧暂名）、云输入法

**硬件钱包守护（walletd）**：直通 eSE 安全芯片与 NFC 控制器，承载交通卡、门禁卡、数字车钥匙并支持设备低电关机后 5 小时断电保刷的系统服务。_Avoid_: 软件模拟钱包

**专注模式（Focus Mode）**：根据时间、地理位置或车载状态多态重塑桌面小组件与页面，并执行通知白名单过滤的治理状态机。_Avoid_: 单纯系统静音

**企业双域隔离（Work Profile）**：在同一物理设备上基于独立 CE 加密密钥与 UID 隔离划分出的企业办公沙箱，强制执行防复制外泄（DLP）与远程擦除。_Avoid_: 简单应用双开

**伪终端守护（ptyd）**：提供符合 POSIX 语义的虚拟主从伪终端（Master/Slave PTY），支持 ANSI/VT100 终端序列与窗口尺寸变更事件广播。_Avoid_: 裸串口直接桥接

**终端多路复用器（bmux）**：BerryOS 原生终端多窗多面板分屏会话复用器，支持会话持久分离与挂载。_Avoid_: 必须依赖外部 tmux 移植
