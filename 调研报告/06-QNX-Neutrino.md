# QNX Neutrino 调研报告

> 调研日期 2026-10-15 · 基于公开资料与源码快照 · 许可证：专有商业闭源（按设备收取 runtime 版税）；2024-12 起提供非商业免费授权（QNX Everywhere），其中开源移植件按原开源许可证发布

**闭源声明（证据等级总则）**：QNX Neutrino 内核与 procnto 均不公开源码，本报告无法进行真实源码走读。所有机制结论来自官方在线文档（qnx.com/developers/docs，SDP 6.4/7.1/8.0 三版交叉）、公开 C 库 ABI 文档、官方新闻稿与社区论坛，引用处均给出具体文档路径；无法核实的数字一律标注（待核实）。

## 0. 一句话定位

QNX Neutrino 是商用微内核硬实时 OS 的工业与车载事实标杆：内核只保留线程/消息传递/时钟/中断/POSIX 同步原语这一最小集，其余一切（进程管理服务、驱动、文件系统、网络栈）都是用户态进程，靠同步消息传递（MsgSend/MsgReceive/MsgReply）粘合成完整 POSIX 系统；2024 年底起随"QNX Everywhere"对非商业用途免费开放。

## 1. 谁在做：组织/社区/治理/活跃度/商业背书

- **谱系**：1980 年 Dan Dodge 与 Gordon Bell 在滑铁卢大学读书时创立 Quantum Software Systems，1982 年发布第一版 QNX（Intel 8088）；QNX Neutrino（QNX 6 / RTP）是 1999-2000 年为 SMP 与 MMU 时代全新重写的内核。2004 年被 Harman 以约 1.38 亿美元收购，2010 年 RIM（BlackBerry）以约 2 亿美元购入（官方新闻稿：qnx.com/news/pr_4114_1.html；Wikipedia: Dan Dodge）。
- **当前归属**：BlackBerry Limited 的 IoT 分部（QNX 品牌独立运营，qnx.software）。2023 年 10 月 BlackBerry 宣布拟将 IoT 业务拆分子公司并 IPO，截至本报告日该 IPO 未完成，QNX 仍为 BlackBerry 全资业务（2026 年最新状态：待核实）。
- **商业体量**：官方口径 2024 年 10 月起 QNX 软件装载于超过 2.55 亿辆汽车（ai-online.com 转官方新闻稿），2026 年报口径 2.75 亿辆（未读原文，待核实）；Mordor Intelligence 估 2025 年车载 OS 市场份额约 38.2% 居首。FY2025 QNX 收入约 2.49 亿美元、royalty backlog 约 8.65 亿美元（BlackBerry 财报转述）。
- **治理与社区**：闭源单向开发，无公开仓库治理；社区在 community.qnx.com 论坛（官方工程师答疑，例如确认 APS 在 8.0 移除的帖子 post122632）与 openqnx.com。文档全部公开且质量极高——这本身就是其获客手段。
- **2025-2026 动向**：CES 2025 启动"QNX Everywhere"，SDP 8.0 + Hypervisor 8.0 非商业免费、免审批，附 Raspberry Pi 镜像（hackster.io 报道）；2026 年推出面向 SDV 的"QNX Alloy Kore"基础软件包与面向初创的 QNX Launchpad；确定性调度转由合作方 MotionWise Schedule 补位（官方博客 2026-09）。
- **认证**：QNX OS for Safety 8.0 经 TÜV Rheinland 认证满足 ISO 26262 ASIL D、IEC 61508 SIL 3、IEC 62304 Class C（官方产品页 qnx.software）。

## 2. 总体架构：内核形态、分层与组件

**形态：微内核（带一个著名的务实折衷——见下）。** SDP 8.0 在线 System Architecture（`com.qnx.doc.neutrino.sys_arch/topic/kernel_SERVICES.html`）明列内核调用仅覆盖：threads、message passing、signals、clocks、timers、interrupt handling、semaphores、mutexes、condvars、barriers——"The entire OS is built upon these calls"。服务进内核的唯一判据是"执行路径短"，长路径工作（如进程加载）一律外置为进程/线程。官方宣称内核全抢占（消息传递中途被抢占后从断点续传），关中断/抑抢占区间"typically in the order of hundreds of nanoseconds"。

关键折衷：**procnto = 微内核 + Process Manager 的单一模块**（`proc.html`：进程创建/销毁/uid/gid、内存保护、共享内存、**路径名空间管理**都在 procnto 里）。官方特意说明：procnto 内的线程调用微内核的方式与其他进程完全一致，"共享同一进程地址空间并不意味着私有接口"，所有线程进内核都做特权级切换。也就是说 QNX 的"微内核"并不追求 seL4 式的内核极小化，procnto 是个庞然大物（8.0 的 procnto 工具页列了 30+ 命令行选项），但它对驱动/FS/网络仍是服务器。

分层：微内核+procnto → 用户态资源管理器（驱动/FS/字符设备）→ io-sock 网络栈（FreeBSD API，内核外共享库模块）→ QNX Hypervisor（独立产品，混合关键性隔离）。8.0 仅支持 x86_64 与 AArch64（8.0.5 Release Notes），从 7.0 起弃 32 位；官方产品册称内核"augmented to support Armv8/Armv9 and x86 64"。分布式 Qnet（/net 节点透明消息传递）随 io-pkt 栈在 8.0 停用（见 §3.5）。

## 3. 核心机制深挖

### 3.1 同步消息传递：MsgSend/MsgReceive/MsgReply + pulse

主 IPC 是**同步+复制**语义（`ipc.html`），完整 API 十个函数：MsgSend/MsgReceive/MsgReceivePulse/MsgReply/MsgError/MsgRead/MsgWrite/MsgInfo/MsgSendPulse/MsgDeliverEvent（`ipc_Message_API.html`），v 后缀变体走 IOV。要点：

- **通道/连接模型**（`ipc_Channels.html`）：server 用 `ChannelCreate()` 建通道，client 用 `ConnectAttach()` 连接；**client 连接直接映射为文件描述符**——"we have eliminated yet another layer of translation"，`read(fd)` 无需再翻译 fd 到目标，fd 即连接 ID。通道挂三条队列：Receive（LIFO）、Send（按优先级 FIFO）、Reply（无序），等待线程分别为 RECEIVE/SEND/REPLY 阻塞态。服务端标准事件循环：`ChannelCreate → for(;;){ MsgReceivev → switch(msg.type) → MsgReplyv }`。
- **复制语义的工程判断**（`ipc_Message_copying.html`）：消息直接从发送方地址空间拷到接收方地址空间、无中间缓冲，"性能逼近内存带宽"；**小消息反而故意经中间缓冲拷两次**——原文承认"copying such messages twice through an intermediate buffer is faster than manipulating page tables"，所以消息可以随手在栈上构造（文档贴了 lseek 库函数实现：栈上 `io_lseek_t msg` → `MsgSend(fd,...)` 一段真实代码）。IOV 多段消息最多 524288 段，文件系统读直接用 n 段消息从 cache 散射拷贝进应用。
- **消息驱动的优先级继承**（`ipc_Priority_inheritance_messages.html`）：server 收到的消息按发送者优先级排序；server 线程继承最高优先级发送者的优先级，且**提升发生在 MsgSend 时刻**（若发送者优先级更高）以堵住第三种反转路径；优先级保持到下次更高消息或显式设置，可用 `_NTO_CHF_FIXED_PRIORITY` 标志关掉。
- **pulse**：固定尺寸非阻塞小消息，8 位 code + 64 位 payload（`ipc_Pulses.html`），用于 server→client 通知、定时器到期等；events 子系统（SIGEV_*）统一异步通知。
- 其他 IPC 形态全部叠加在消息传递上：POSIX 消息队列在内核外、pipe/FIFO 是外部进程、共享内存由 procnto 管理（mmap）。

### 3.2 resmgr 资源管理器：驱动=用户态服务器 + 路径名空间挂载

这是与 BerryOS「路径式命名空间」最直接可比的机制（`resource.html`、`resource_WHATISRESMGR.html`、`resource_RESMGRARCH.html`）：

- **驱动零特权耦合**："a resource manager looks just like any other user-level program"，"there's no magic or private interface between the kernel and a resource manager"。串口驱动 devc-ser* 就是个普通用户程序，把 `/dev/ser1` 注册进路径空间。
- **挂载原语**：resmgr 通过告知 procnto"我负责某挂载点（及其以下）"来绑定路径；client `open("/dev/ser1")` 由 procnto 解析到该 resmgr 的连接。伪代码骨架（官方原文）：initialize dispatch → register pathname → for(;;){ receive; switch; }。
- **真实 ABI**（`resmgr_attach()` 库参考页，`#include <sys/resmgr.h> <sys/dispatch.h> <sys/iofunc.h>`）：

```c
int resmgr_attach ( dispatch_t *dpp, resmgr_attr_t *attr, const char *path,
    enum _file_type file_type, unsigned flags,
    const resmgr_connect_funcs_t *connect_funcs,
    const resmgr_io_funcs_t *io_funcs, RESMGR_HANDLE_T *handle );

typedef struct _resmgr_attr {
    unsigned flags;          /* RESMGR_FLAG_ATTACH_LOCAL: 只建框架不进全局路径树 */
    unsigned nparts_max;     /* IOV 分段数 */
    size_t   msg_max_size;
    int    (*other_func)(resmgr_context_t *, void *msg);
    unsigned reserved[4];
} resmgr_attr_t;
```

  connect_funcs/io_funcs 两张函数表让 iofunc 层提供全部 POSIX 语义默认实现（`iofunc_attr_t` 为每路径对象头）；flags 里 `_RESMGR_FLAG_BEFORE/_AFTER` 支持**同路径叠层**（多驱动排队，如过滤器/加密层叠在磁盘驱动上）——与 Plan 9 union mount 同思想。
- **对照**：BerryOS `open("/svc/fs") → 返回 Channel 端点 handle`、服务把端点挂到 `/svc/*`、namespace 服务持根做查找授权（`文档/IPC设计.md` §5）——语义同构；差异在宿主：**QNX 的路径树宿主是 procnto（与内核同地址空间），BerryOS 是独立用户态 namespace 服务**，后者失败域更小、更符合能力模型（你能看到哪些路径=持有的 namespace handle 权限，QNX 则靠 uid/gid+abilities）。

### 3.3 调度：FIFO/RR/Sporadic + 自适应分区 AP（6.x/7.x）

- 基线（8.0 `kernel_SCHEDULING.html`、`kernel_SchedulingAlgorithms.html`）：每次进内核（调用/异常/中断）都可能做调度决策，线程跨进程全局调度；每线程可任选 FIFO/RR/sporadic 策略（非全局）；"the sporadic method employs a budget for a thread's execution"（POSIX 端发服务器：补算周期+预算）。
- **自适应分区 APS**（6.4 `sys_arch/adaptive.html` + 7.1 Adaptive Partitioning User's Guide）：按线程组保证最低 CPU 百分比（budget），在 100ms 平均窗口内计量；分区数上限 8（官方解释：每次调度决策要扫描全部分区，700MHz x86 上调度操作可达每秒 5 万次，必须限制分区数压开销）。
  - **自适应性**：空闲预算按策略分发——默认给"有最高优先级就绪线程的分区"（保严格实时语义），或置 `SCHED_APS_SCHEDPOL_FREETIME_BY_RATIO` 按预算比例分配；`SCHED_APS_SCHEDPOL_LIMIT_CPU_USAGE` 限制分区超借上限（max_budget_percent）。
  - **分区继承（partition inheritance）**——最精妙的机制：server 处理消息期间**计费到发送方所在分区**，"all resource managers, such as drivers and filesystems, automatically bill CPU time (except overhead) to the budget of their clients"。server 自身可以零预算运行。
  - **关键线程/critical budget**：分区可设临界预算（ms）与临界优先级；达到临界优先级的线程即使在分区超支时也照跑（保证硬实时延迟），临界时间耗尽即"bankruptcy"处置；把临界预算设为 CPU 数×窗口大小等于无限（官方警告有安全风险）。管理面：`aps` 工具与 `SchedCtl(SCHED_APS_*)` 系列内核调用。
  - 设计动机官方直言两条：满载下仍正确运行 + **防不可信应用垄断系统（DoS 遏制）**。
- **8.0 的重大变化**：APS 被移除。官方 IDE 文档（`APS_view.html`）："QNX OS 8.0 does not support adaptive partitioning"；QNX 员工在社区论坛（post122632, 2024-07-23）："APS is not available in 8.0... the primary one is that it doesn't fit with the new scheduler design"。确定性调度缺口由第三方 MotionWise Schedule 扩展包补位（官方博客 2026-09："MCU-class determinism on microprocessors"）。
- 时钟与 tickless（8.0 procnto 工具页）："**Although the system is always tickless**, it still supports the concept of a periodic tick"——8.0 全程 tickless，tick 只是 1kHz 粒度概念（HRT 高精度定时器直接用硬件粒度）；时钟中断服务线程（clock IST）默认优先级 254，恰低于核间中断 IST——高于它的线程不能依赖软定时器。另有 CPU offlining（特权应用可让 procnto 不在某核调度）。

### 3.4 中断与实时性证据

- 8.0 中断模型（`kernel_INTERRUPTHANDLING.html`）：内核异常处理程序识别中断→**屏蔽该中断**→投递内部信号量把关联线程标 READY→向 PIC 发 EOI→按常规调度规则调度用户态 **IST（Interrupt Service Thread）** 处理设备→IST 处理完再解除屏蔽。设计原则明言"avoidance of run-time allocation or queueing"。即 8.0 连中断上下文驱动代码都不进内核，全线程化（对延迟敏感者靠高优先级 IST 抢占保证，配 `-C tick,IST优先级` 调节）。
- 实时性定量证据：官方仅给定性界限——内核不可抢占窗口"数百纳秒"级（kernel_SERVICES.html），且 what_is_realtime.html 只给方法论（可抢占内核+优先级继承+中断延迟固定上界等要求清单）。第三方实测：OpenQNX 论坛用户量到裸中断延迟 25-35μs（2008，方法学存疑）；经销商材料称现代 SoC 最坏中断延迟 <10μs（低证据等级）。**QNX 从不公开微基准表，硬实时口碑主要靠认证与车载量产背书，而非公开数据**——这是与 seL4（形式化证明）和 Linux（PREEMPT_RT 反复公开对比）截然不同的证据结构。

### 3.5 Qnet 分布式与网络栈

Qnet（`lsm-qnet.so`，6.x/7.x）把 MsgSend 透明延伸到网络：远端节点出现在 `/net/<node>` 路径空间，跨机 open/消息传递与本地无差别——微内核消息传递天然可分布的历史卖点。但 8.0 Component Discontinuation Notice 停用了 io-pkt 栈（Qnet 宿主），io-sock（FreeBSD 栈）成为唯一支持栈，**Qnet 实质死亡**，替代品为普通 TCP/IP 服务。透明分布式 IPC 未通过安全/成本/维护的长期筛选——对 BerryOS 是一条重要先例。

### 3.6 安全模型

无能力系统：访问控制=Unix uid/gid+POSIX 权限位，7.0 起引入 `procmgr_ability()` 细粒度特权能力（仍非对象能力）；安全加固靠独立产品线（Secure Boot、Hypervisor 隔离、OS for Safety 认证变体——procnto 的 safety 版与商业版"部分默认值不同"，见 procnto 工具页注记）。内核自带 tracing 变体 procnto-smp-instr（instrumentation），实时监控内核行为。

## 4. 源码走读：目录结构、关键文件、关键数据结构（贴真实代码路径）

闭源，无源码可走读。替代走读对象为官方在线文档树与公开 C ABI（两者都是真实可引用路径）：

- **文档树"路径"**（docs.qnx.com 结构，本报告直接抓取核实）：`com.qnx.doc.neutrino.sys_arch/topic/` 下 `kernel.html / kernel_SERVICES / kernel_SCHEDULING / kernel_SchedulingAlgorithms / kernel_INTERRUPTHANDLING / ipc.html / ipc_Channels / ipc_Pulses / ipc_Priority_inheritance_messages / proc.html / resource.html` 等；库参考 `com.qnx.doc.neutrino.lib_ref/topic/r/resmgr_attach.html`、`.../c/channelcreate.html`、7.1 `.../s/schedctl.html`；工具参考 `com.qnx.doc.neutrino.utilities/topic/p/procnto.html`；7.1 另有整本 `com.qnx.doc.adaptivepartitioning.userguide/`。
- **关键数据结构（摘自官方库参考，可直接核对）**：`resmgr_attr_t`（见 §3.2 代码块）；`ChannelCreate()` 标志集 `_NTO_CHF_DISCONNECT/_NTO_CHF_COID_DISCONNECT/_NTO_CHF_UNBLOCK/_NTO_CHF_FIXED_PRIORITY/_NTO_CHF_PRIVATE/_NTO_CHF_THREAD_DEATH/_NTO_CHF_INHERIT_RUNMASK`（含 8.0 新增 `ChannelCreatePulsePool()` 私有 pulse 池）；`SchedCtl` 的 `SCHED_APS_CONFIG/SCHED_APSADD_PARTITION/SCHED_APSJOIN_PARTITION` 与 `SCHED_APS_SCHEDPOL_*` 策略位、`windowsize_ms/num_partitions/max_budget_percent` 字段；服务端身份结构 `iofunc_attr_t`、函数表 `resmgr_connect_funcs_t/resmgr_io_funcs_t`。
- **源码可得性核实**：GitHub `qnx` 组织公开仓库仅约 2 个（开源移植件；2026-10 经 API 查询，遇限流，数量级可信）；QNX Everywhere 明确"publishes source code for open-source ports under their original licenses"——**内核/procnto 源码仍仅向付费商业授权客户开放**（商业 source license 条款细节：待核实）。第三方 Ferrocene（认证 Rust 工具链）已支持 QNX 8 目标（x86_64/aarch64-pc-qnx），说明 Rust 与 QNX 已可互操作。

## 5. 亮点：好在哪（逐条给证据）

1. **连接即文件描述符**：client 连接直接映射 FD，消灭一层翻译（ipc_Channels.html 原文"eliminated yet another layer of translation"），统一了 IPC 与 POSIX I/O 的世界观。
2. **对复制语义的诚实工程**：小消息"拷两次快过改页表"是官方文档白纸黑字（ipc_Message_copying.html），换来消息可在栈上构造、无专用页对齐内存池——复杂度换性能的教科书级权衡。
3. **消息驱动优先级继承默认开启且时序精确**（发送时提升或接收时继承，堵住 T3 中插反转），微内核服务化后仍保严格实时语义——服务器队列天然按优先级排。
4. **resmgr 三层框架（dispatch/resmgr/iofunc）把"写驱动"降维成"填 POSIX 函数表"**，且叠层挂载（_RESMGR_FLAG_BEFORE/AFTER）原生支持过滤器栈；驱动与内核零特权耦合。
5. **APS 是商业上最成熟的"带宽保证+借用"调度器**：预算包络+空闲借用+临界预算豁免+DoS 遏制四合一，尤其分区继承把驱动/FS 的服务时间自动计费给发起方——归因问题的内置解。
6. **8.0 全面 tickless + 中断全线程化（IST）**：连中断都不进内核执行驱动代码，可抢占内核的不可抢占窗口压到百纳秒级（官方口径）。
7. **45 年连续演进 + 2.55 亿车载 + ASIL D/SIL 3/IEC 62304 Class C 认证**：微内核路线在安全关键市场的最强商业存在证明。
8. **文档即产品**：System Architecture 每版公开、机制级细节（队列结构、计费规则、停用清单）全部可查，discontinuation notice 连"砍了什么"都成文公示。

## 6. 不足与争议：不好在哪（逐条给证据）

1. **闭源+版税模式**：按设备收 runtime royalty，源码审计需商业授权；社区只能读文档不能读代码（本报告"源码走读"被迫降级为 ABI 走读即为直接后果）。
2. **procnto 折衷腐蚀了微内核承诺**：进程/内存/路径名管理住在与内核同地址空间的 procnto 里，procnto 崩=系统亡；BerryOS 所宗的 Zircon/seL4 纯路径明显更干净。官方文档自己也需专门解释"共享地址空间不意味着私有接口"。
3. **8.0 砍掉 APS 是自伤式简化**：近二十年差异化卖点（分区预算）因不适配新调度器整体移除（论坛官方答复），补位方案是外部商业伙伴 MotionWise——引擎内复杂度（每次调度扫描全部分区、上限 8 个）说明当初把策略焊进了机制；历史缺陷如"SCHED_RR 线程在 4% 分区可能不能正确轮转"（官方技术文章 Ref# 28035）也显示策略叠加的脆弱。
4. **无对象能力模型**：uid/gid+abilities 的 Unix 式授权，跨信任边界传递细粒度权限笨拙；Qnet 透明网络消息历史上即因安全边界模糊被诟病，最终在 8.0 随 io-pkt 一起停用。
5. **实时数据不公开**：无官方微基准，第三方测量零散（25-35μs 用户自测/<10μs 经销商口径），采信只能依赖认证与量产记录。
6. **生态锁定**：Momentics IDE/qcc 专有工具链（近年才补 VS Code Toolkit），Rust 等新语言只能经 Ferrocene 等第三方认证工具链进入；8.0 仅剩两架构，老嵌入式平台（PPC/SH-4/ARMv7）用户被断供。
7. **文档收缩迹象**：8.0 在线 System Architecture 的 High Availability 章仅余两段引言，HAM 监督框架细节不再展开（收缩程度：待核实）；Qnet、io-pkt、fs-cifs 等连续进入停用清单（discontinuation notice）。

## 7. 对 BerryOS 的借鉴与警示（对照四支柱）

**支柱① 微内核/IPC**
- 借鉴 1——**同步 channel 必须带发送者优先级继承**：QNX 证明这是微内核服务化后软实时的命门（服务器以客户优先级工作）。BerryOS `文档/IPC设计.md` 的 Channel 设计尚未写优先级继承语义，建议补为内核机制（并学其精确时序：高优先级发送者在 send 时刻即提升 server，而非 recv 时刻）。
- 借鉴 2——**通道三队列语义可抄**：Send 队列按优先级 FIFO、Receive 队列 LIFO、Reply 无序列表，这是被 45 年实践打磨的最小完备结构；BerryOS 的"有界队列 FIFO"宜升级为"按优先级入队"。
- 借鉴 3——**"连接即 handle"验证了 BerryOS open→handle 设计**：QNX 连接=FD 与 BerryOS open("/svc/fs")→Channel 端点 handle 是同一洞察，BerryOS 还更进一步把 handle 做成能力（QNX 做不到）。
- 借鉴 4——**小消息复制、大块共享**的分层被 QNX 数据背书（拷两次胜过页表操作），BerryOS inline≤256B + VMO 大块的双层设计与其同构，可安心。
- 警示 1——**别学 procnto 把进程/路径管理塞进内核地址空间**：BerryOS 决策（namespace 服务在用户态持根）已正确避开；QNX 的失败域就是反例。
- 警示 2——**别让任何 IPC 形态统管一切**：Qnet"透明分布式消息"曾是 QNX 招牌，最终死于安全与维护成本，8.0 停用——直接印证 `架构综合纲领.md` 排除第 4 问。BerryOS 共享环独占热路径、channel 不统管的设计方向正确。

**支柱② 省电**
- 借鉴——**"always tickless + 时钟 IST 优先级位"**：QNX 8.0 全程 tickless、时钟 IST 固定 254（恰低于 IPI IST）且"高于它的线程不得依赖软定时器"的契约，与 BerryOS NO_HZ day-one 同路；其 CPU offlining（用户态特权应用可摘核）是手表/手机场景可参考的机制原型。
- 警示——QNX 不是电池驱动系统，无深度睡眠/wakelock 体系可抄；BerryOS 的 tickless idle（Phase 2a 已落地）+ PM 用户态服务路线仍应对照 Linux/Zircon 而非 QNX。

**支柱③ AI 原生调度（对照 APS 最深）**
- 借鉴 1——**APS=BerryOS 护栏思想的直系原型**：预算包络（budget%）+突发借用（freetime 分发）+RT 豁免（critical budget≈"class_rt 屏蔽"）+越界处置（bankruptcy）与 `文档/AI调度设计.md` §5 的护栏表逐行同构；BerryOS 可直接引用这套词汇与边界条件定义。
- 借鉴 2——**分区继承=per-task 归因的现成答案**：服务/驱动时间计费给发起方分区，等价于 BerryOS 遥测 ABI 中 per-task 运行时长分布的内核侧归因规则，值得在遥测环设计里落实"服务时间记到触发者"。
- 警示——**APS 之死是 BerryOS 最重要的反面教材**：把预算策略焊进调度热路径（每次决策扫描全部分区、分区上限 8、策略互相打架出 Ref# 28035），最终连原作者都砍掉。BerryOS"机制在内核 O(1) 查表、策略（含 AI）在用户态慢循环、越界拒收"的架构恰好规避了死因——应视为路线正确性的独立佐证，并把"快路径开销与策略复杂度无关"列为不可妥协的验收线。
- 借鉴 3——QNX 用"安全变体不同默认值"（procnto safety 版）实现认证与通用版同源分叉，BerryOS 未来认证可采同法。

**支柱④ PC 高性能**
- 借鉴——线程跨进程全局调度+全内核可抢占（不可抢占窗口百纳秒级）+SMP/BMP 双模式历史，佐证 BerryOS"每核 runqueue+负载均衡+全可抢占"目标在微内核上可达；`_NTO_CHF_INHERIT_RUNMASK` 类的 runmask 亲和微调也可入 capacity 抽象的词汇表。
- 警示——QNX 没有 fair-share/EAS 类能耗感知调度的公开实践（APS 之外），大小核能效放置这条路 QNX 帮不上，BerryOS 仍须对照 Linux EAS/Zircon。

**通用警示**：闭源文档再好也不可复制实质（协议洁净原则：只学思路）；QNX 的成功一半在工程（消息传递一致性），另一半在 40 年认证与客户关系——后者不可借鉴，BerryOS 应走的仍是"关卡 G0–G5 可验证的正确性"路线。

## 8. 参考资料链接

官方文档（本次调研直接抓取核实）：
- System Architecture 8.0：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.neutrino.sys_arch/topic/kernel_SERVICES.html （同目录 kernel_SCHEDULING / kernel_SchedulingAlgorithms / kernel_INTERRUPTHANDLING / ipc.html / ipc_Channels / ipc_Pulses / ipc_Priority_inheritance_messages / ipc_Message_copying / proc.html / resource.html / what_is_realtime.html）
- procnto 8.0 工具参考：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.neutrino.utilities/topic/p/procnto.html
- resmgr_attach()：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.neutrino.lib_ref/topic/r/resmgr_attach.html
- ChannelCreate()：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.neutrino.lib_ref/topic/c/channelcreate.html
- SchedCtl()（7.1，含 SCHED_APS_* 全量定义）：https://www.qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.lib_ref/topic/s/schedctl.html
- Adaptive Partitioning User's Guide 7.1：https://www.qnx.com/developers/docs/7.1/com.qnx.doc.adaptivepartitioning.userguide/topic/aps_details_Base_.html （子页 aps_details_Divided_ / aps_details_Critical_threads_ / aps_details_Inheritance_）
- APS 架构章（6.4）：https://www.qnx.com/developers/docs/6.4.0/neutrino/sys_arch/adaptive.html
- 8.0 不支持 APS（官方 IDE 文档）：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.ide.userguide/topic/APS_view.html
- 8.0 组件停用公告（Networking/Qnet/io-pkt）：https://www.qnx.com/developers/docs/relnotes8.0/com.qnx.doc.discontinuation_notice/topic/networking.html
- SDP 8.0.5 Release Notes（x86_64+AArch64）：https://www.qnx.com/developers/docs/relnotes8.0/com.qnx.doc.release_notes/topic/8.0.5_prod_release_notes/8.0.5_rn.html

官方与商业资料：
- QNX Everywhere（非商业免费）：https://qnx.software/en/developers/get-started/qnx-everywhere
- QNX OS for Safety（认证）：https://qnx.software/en/software/products-and-solutions/qnx-os-and-os-for-safety
- SDP 8.0 产品册 PDF：https://qnx.software/content/dam/qnx-xwalk/pdf/product-briefs/qnx-sdp8-product-brief.pdf
- MotionWise Schedule 博客（2026-09）：https://qnx.software/en/blog/2026/achieving-mcu-class-determinism-on-microprocessors-with-motionwise-schedule-and-qnx-sdp-8-0
- 2.55 亿车辆新闻稿转述：https://ai-online.com/2024/12/qnx-embedded-technology-powers-255-million-vehicles-on-the-road-today
- RIM 收购 QNX 新闻稿：https://www.qnx.com/news/pr_4114_1.html

社区与第三方：
- APS 移除官方答复（论坛）：https://community.qnx.com/sf/go/post122632
- OpenQNX 论坛延迟实测：https://forums.openqnx.com/t/topic/5497
- 非商业授权社区讨论：https://forums.openqnx.com/t/topic/47580 、https://www.hackster.io/news/blackberry-s-qnx-seeks-hobbyists-and-makers-with-free-non-commercial-license-raspberry-pi-image-7c53320cac11
- Dan Dodge/Wikipedia：https://en.wikipedia.org/wiki/Dan_Dodge
- Dissecting the QNX Adaptive Partitioning Scheduler：https://www.researchgate.net/publication/353094266
- Adaptive partition scheduler 概念：https://en.wikipedia.org/wiki/Adaptive_partition_scheduler
- Ferrocene QNX8 手册（Rust 工具链支持）：https://public-docs.ferrocene.dev/main/targets/qnx8-manual/targets/x86_64-pc-qnx.html

BerryOS 内部对照文件：`文档/IPC设计.md`、`文档/AI调度设计.md`、`文档/架构综合纲领.md`、`CLAUDE.md`（本次调研已通读核对）。
