# Linux 进化史（1991–2026）调研报告

> 调研日期 2026-10-15 · 视角：进化在优化什么 / 踩了什么坑 / 哪些郑重决策被推翻 · 事实经 WebSearch + web_reader 联网核实，无法确证处标「存疑」。
> 交叉引用：微内核家族谱系见 04 号报告（seL4/L4），Zircon 见 05 号，MINIX 3 见 10 号，工业 RT 系统见 07 号，Rust 系 OS 见 11 号。本篇聚焦 Linux 自身的「进化史/坑史」。

## 0. 一页总览（TL;DR）

Linux 用 35 年证明了一件事：**长期胜负不由初始架构的「理论正确性」决定，而由系统的「翻案能力」决定**。Tanenbaum 1992 年从微内核理论出发宣判 Linux「过时」，论战本身 Tanenbaum 论点占优；但 Linux 靠三件非架构因素赢了市场：①GPLv2 + 互联网协作聚起史上最大开发者池；②「无稳定内核 ABI」让内部代码可以无限次重构；③时间型发布节奏让每次翻案的成本可控。它一次次推翻自己：换掉 VM、换掉三代谢度器、废掉 devfs、删掉 a.out、肢解 BKL——每次翻案的触发器几乎都是「硬件或工作负载变了，旧设计的前提失效」。**微内核阵营赢得的概念（策略外移、消息驱动、最小特权）后来以 udev、uevent、eBPF、cgroup 的形式逐条被 Linux 吸收**——这是比「谁赢」更重要的结论。对 BerryOS：微内核不是「被 Linux 证伪的路线」，而是「Linux 花 20 年绕远路才部分抵达的位置」；但 Linux 的教训是——演进治理（谁有权翻案、多久翻一次）比初始设计更值得设计。

### 进化阶段表

| 年代/版本 | 在优化什么 | 踩的坑 | 关键调整 |
|---|---|---|---|
| 1991–93（0.x→1.0） | 在 386 上跑一个自由的 Unix（学习+自由软件动机，非商业） | Tanenbaum 宣判「过时」；MINIX 许可证限制改动分发 | 换 GPLv2；邮件列表+「独裁者+副维护者」治理成型 |
| 1994–96（1.x→2.0） | 「能用」：网络栈、SMP、按需加载 | SMP=一把 BKL 大锁；全编译进内核浪费内存 | LKM 可加载模块——既非纯宏也非微的第三条路 |
| 1997–2001（2.2→2.4） | 服务器化：大内存、企业负载 | **2.4 VM 大崩溃**（2001 夏）；维护者政治撕裂 | 2.4.10 稳定系列中途换 VM；催生 2.6 开发模式改革 |
| 2002–03（2.5→2.6.0） | SMP 可扩展性 | BKL 与全局锁在多核上失效；全局 runqueue O(n) | 细粒度锁冲刺 + RCU 合入（2002-10）+ O(1) 调度器；改时间型发布 |
| 2003–11（2.6 长跑） | 吞吐 + 桌面交互 + 企业采用 | O(1) 交互启发式腐烂；devfs 不可救药；tick 浪费功耗 | CFS（2007）；devfs→udev/sysfs（2005–06）；NO_HZ tickless（2.6.21, 2007） |
| 2004–24（RT 长跑） | 硬实时（工业/音频/汽车） | 主线 15 年抵制 RT 补丁集 | 逐块上游化（generic IRQ 2005 → rt-mutex 2006 → …），6.12（2024）完全合入 |
| 2008–16（容器时代） | 隔离与资源管控（Google 数据中心） | cgroup v1 多层级是设计错误 | cgroup v2 统一层级（4.5, 2016）；Docker/k8s 把内核原语重组为「云 OS」 |
| 2017–26（云原生+异构） | 地址空间、延迟敏感负载、安全、内存安全 | 47 位 VA 假设渗入用户态；CFS 启发式再次腐烂；C 文化抵制新语言 | 5 级页表（2017，默认仍保守）；EEVDF（6.6, 2023）；sched_ext（6.12）；Rust 进内核（6.1, 2022）但人事摩擦不断 |

## 1. 诞生与 1992 论战：理论赢面大、工程被市场判决

**动机**。1991-08-25 Linus Torvalds 在 comp.os.minix 发帖：「just a hobby, won't be big and professional like gnu」——做 Linux 的直接动机是（a）学 OS、（b）386 上缺一个可自由改动的 Unix（MINIX 许可证禁止修改后分发，直到 2000 年才重新授权），（c）GNU 自己的内核 Hurd 迟迟不出。技术上是 Tanenbaum《操作系统：设计与实现》读者的「出师作」。GPL Hurd 跳票是关键背景：Linus 在论战原话里明说「If the GNU kernel had been ready last spring, I'd not have bothered to even start my project」（若 GNU 内核去年春天就绪，我根本不会开工）。

**论战**。1992-01-29 Tanenbaum 发帖《LINUX is obsolete》。他的论点（按 Wikipedia 存档原文归纳）：①微内核（MINIX/Amoeba/Mach）是更优设计，宏内核是「a giant step back into the 1970s」（向 1970 年代的大倒退）；②可移植性才是自由 OS 的命脉，绑死 386 没前途；③自由阵营应合力做一个微内核核心。他还不忘补刀：「Be thankful you are not my student. You would not get a high grade for such a design :-)」。Linus 的反驳（同日）：①可移植性不是目标——「portability is for people who cannot write new programs」（可移植性是给写不出新程序的人的）；②微内核的理论优势（模块化、隔离）在工程上兑现不了，Mach 类系统的 IPC 开销吃掉收益；③Hurd 的进度反证了「设计先行」路线做不出能用的东西。Ken Thompson 等随后下场，整体对微内核路线不屑（大意：能用的系统比纯设计重要）。

**30 年再评估**。表面是 Linus 赢：Linux 占据服务器、云（TOP500 自 2017-11 起全部跑 Linux）、移动（Android 内核）与嵌入式。深层是「双赢」：Tanenbaum 的概念清单逐条被 Linux 吸收（见 §5 devfs→udev 与 §7），而微内核的性能质疑被 L4/seL4 的 fastpath 工作逐步化解（详见 04 号报告）。2006 年 IEEE Computer 杂志论战重开（Tanenbaum《Can We Make Operating Systems Reliable and Secure?》+ Torvalds/Shapiro 反驳），Torvalds 的立场已变为：争论从来不是「微 vs 宏」，而是「切线画在哪」——他讥讽 NT/OS X 的「混合内核」是营销词，真正的问题是系统里有越来越多本可移出的东西被迫留在内核（GPU、电源、安全策略），这正是微内核论点的翻版。最辛辣的讽刺是 2017 年被披露 Intel ME 里跑的是 MINIX 3（详见 10 号报告）：**Tanenbaum 的内核没进你的操作系统，却进了每一颗 CPU**。

**为什么理论占优的一方输了市场**（对微内核团队最重要的段落）：①时间窗口——1992–2001 年恰是服务器爆炸期，Linux「先能跑再重构」恰好赶上，Hurd/QNX 风格的「先设计后交付」错过；②协作结构——GPLv2 + 「补丁流」治理让成千上万公司能共付演进成本，微内核项目普遍是单点研究团队；③第三条路 LKM（见 §2）让 Linux 拿到了微内核 80% 的「运行时可扩展」好处而只付 5% 的性能代价，微内核的差异化被抽空。**这不是「微内核被证伪」，而是「微内核的卖点被分拆盗用」。**

## 2. 1.x→2.0：LKM——既非纯宏也非微的第三条路

模块机制在 1.x 早期已有雏形（精确起点存疑，0.9x 时代已可动态加载），1996-06 发布的 Linux 2.0 把它变成主流形态：几乎所有驱动都可编为模块，kerneld 守护进程按需自动装卸，配合 SMP 支持成为 2.0 的两大招牌。工程意义：内核态执行的驱动（零 IPC 开销，纯宏内核性能）+ 运行时装配（微内核的灵活性）——**用「链接时刻」换「地址空间」：既不动微内核最贵的 IPC 路径，又得到运行时可扩展**。Linux 从此在「宏 vs 微」的二元论里开辟第三格，这是它能吸收两者好处的结构性前提。

代价与坑：①模块与内核之间没有稳定 ABI（见 §6），二进制模块跨版本即坏，为后来三十年的 NVIDIA 之痛埋下伏笔；②GPL 边界问题——闭源模块是否是「衍生作品」悬而未决，社区以 EXPORT_SYMBOL_GPL（2.4 时代引入，仅 GPL 符号对闭源模块不可见）+ 内核 taint 标记做软执行；③模块加载路径本身成为安全攻击面（后期 livepatch/签名机制都是在补这个坑）。

## 3. 并发进化主线：BKL → 细粒度锁 → RCU → PREEMPT_RT

**BKL（1996–2011，一把锁背了 15 年）**。2.0 的 SMP 实现就是一把全局自旋锁：进入内核即持锁，等于「多核上跑单核内核」。这是务实的过渡桥——当时代码全都默认单处理器独占访问内核数据。坑在于桥变成了房：2.5 系列（2002–03）为 8 路以上服务器做细粒度锁大冲刺（Ingo Molnar 等人逐子系统拆锁），但 BKL 的残余用户一直拖到 2009–2011 的专项「Killing the Big Kernel Lock」战役（Frederic Weisbecker 起头、Arnd Bergmann 收尾），2.6.39（2011-05）才彻底移除。Arnd 的策略值得记住：先把 BKL 逐子系统替换成「很多把小 BKL」（等价语义锁），再做真细粒度化——**用语义等价的小步换正确性可审计的中间态**。

**RCU（2002-10 合入 2.5 系列）**：Paul McKenney 把 1980 年代 Sequent DYNIX/ptx 上的老想法带进 Linux（Sequent 1999 被 IBM 收购，动机正是 NUMA 大机上读侧锁的缓存行弹跳）。读写锁在多核/NUMA 下的死亡公式：读者也要原子写锁字 → 缓存行在核间弹跳 → 扩展性归零。RCU 的答案：读侧完全没有原子操作、没有写内存屏障、甚至没有内存写——读者拿「旧版本的指针」直接读，写者复制-更新，宽限期（grace period）后回收旧版。读多写少路径（路由表、dentry、fd 表）从此可以无锁扩展。**推广本身就是一部坑史**：2002 年合入时几乎没有用户，社区花了近十年才普遍接受「延迟回收换读侧零开销」的心智模型，用量从个位数涨到数千处（McKenney 2013 APSMs《RCU Usage In the Linux Kernel: One Decade Later》），再为万核 NUMA 长出层级化 Tree RCU（arXiv:1610.03052 有形式化验证）。教训：**新同步原语的接受周期以十年计；读路径零开销必须 day-one 设计，事后改是重构级工程**。

**PREEMPT_RT（2004–2024，二十年长跑）**。Ingo Molnar 与 Thomas Gleixner（其公司 Linutronix 由工业客户养着）2004 年起做 realtime-preempt 补丁集：把内核里所有不可抢占区（自旋锁、IRQ 关闭区）改造成可抢占（rt-mutex、线程化 IRQ、优先级继承）。主线的长期抵制理由：复杂度、吞吐损失、「通用内核不需要硬实时」。翻案策略是本段的核心教训——**不做一次性的大合并，而是把 RT 基建逐块上游化**：generic IRQ 层（2.6.11, 2005）→ rt-mutex（2.6.16, 2006）→ hrtimers → 线程化中断 → ……每一块对主线独立有益，同时 RT 补丁集变成主线的常驻压力测试（暴露了数以千计的延迟 bug）。2020 年 Gleixner 宣布最后冲刺，2024-09 宣告收尾：6.12 起 x86/ARM64/RISC-V 全架构支持主线 PREEMPT_RT，Linux 正式成为 RTOS。（任务书原话「2020 合入主线」不准确：2020 是宣布冲刺，完全合入是 6.12/2024。）注：本篇不展开 Linux RT 与 VxWorks/Integrity 的工程对比，见 07 号报告。

## 4. 调度器三代：O(1) → CFS → EEVDF——每次替换的触发器

- **O(n)（2.0–2.4）**：全局 runqueue，每次切换线性扫描重算优先级。触发替换的因素：SMP——全局队列锁 + O(n) 在多核大进程数下崩溃。
- **O(1)（2.5.2 合入，2003 年随 2.6.0 交付）**：Ingo Molnar 作。每 CPU runqueue + 优先级位数组，常数时间。**坑：交互启发式**（sleep_avg 等一大堆「猜这个任务是不是交互式」的魔法数）不可预测、不可维护、corner case 无穷，桌面用户怨声载道，也养出了 Con Kolivas 的 -ck 补丁集文化。
- **2007 事件**：Kolivas 3 月发布 RSDL（无饿死、严格公平 O(1)），主线争议中搁置；7 月 Ingo 以 RSDL 的公平思想为底**数周内**写出 CFS，8 月进 2.6.23；Kolivas 愤而退出内核开发（Ars Technica当时题为「parting shot at desktop computing」），成为「维护者政治决定技术路线」的标志事件——**思想被收编，作者被出局**。
- **CFS（2.6.23, 2007-10）**：红黑树按 vruntime 排序，「理想公平多任务 CPU」模型，砍掉全部启发式。讽刺点：以 O(log n) 换 O(1)——公平性比常数时间重要。**16 年后它自己腐烂成它取代的东西**：为交互任务偷偷调整 vruntime、为容器加 task group、place_entity 修正……启发式重新爬满代码。
- **EEVDF（6.6, 2023-10）**：Peter Zijlstra 把 1995 年 Stoica & Abdel-Wahab 的学术算法（Earliest Eligible Virtual Deadline First）实现进内核，用「合格性（lag）+ 虚拟截止时间」两个有原理的量替换所有启发式，天然支持 per-task 延迟敏感度（latency-nice，CFS 时代吵了十年没做成的特性）。触发器：云时代的延迟敏感负载（会议、游戏、微服务尾延迟）+ CFS 启发式再次不可维护。**模式总结：每代调度器死因都一样——启发式在负载变迁下腐烂；解药都是换成有形式语义的模型；而「下一跳」往往在 20 年前的论文里**。另注 6.12（2024）合入 sched_ext：允许用 BPF 在用户态写调度策略热加载进内核——「策略可插拔」最终以安全插件形态进入主线，这对 BerryOS 的可插拔调度 trait 是直接先例。

## 5. 2.4 VM 大崩溃（2001）：维护者政治如何决定技术路线

2.2 的 VM 尚可但有病（Rik van Riel 的 USENIX ATC 2001 论文做了病理分析）；2.4.0（2001-01）带着 Rik 主导重写的 VM 出厂，夏天在高负载换页/数据库场景下大崩溃（史称 VM meltdown）。裂变：Rik van Riel 主张概念性重写（rmap 反向映射）；Andrea Arcangeli 主张对 2.4 现有 VM 做增量修复。Linus 先信重写派，崩溃后 2001-09 的 **2.4.10 在稳定系列中途把整个 VM 换成 Andrea 的版本**——稳定版用户被迫吞下行为突变的子系统，本身又是一层坑。2002 年内核峰会两人同台（Daniel Phillips 主持，意外地文明）。结局有讽刺的对称感：2.6 的 VM 主干来自 Andrea 线，但 Rik 的 rmap 思想（以页为中心的反向映射）也被吸收，2004 年 Andrea 的 anon_vma 又把 rmap 简化——**两人的遗产各留一半，输赢由「Linus 的测试机 + 谁先给出能跑的代码」决定，而非设计评审**。政治教训：①单点仲裁者（Linus）+ 无正式设计评审的制度下，子系统路线由人肉博弈决定，出局者直接离开项目（Rik 后期转向，Kolivas 出走同款剧情）；②2.4 系列的惨状直接催生 2.6 的开发模式改革——放弃「稳定系列大分支」，改时间型发布（2.6.0, 2003-12）+ 2.6.x.y 稳定流（2.6.8.1 于 2004 首次出现，存疑精确版本，2005 年由 Greg K-H 固化流程）。

## 6. 驱动模型与 ABI：devfs 之死、「内核只暴露事件」、NVIDIA 例外

**devfs → udev/sysfs**。devfs（Richard Gooch，1998 年前后进开发内核）在内核里生成 /dev、管理设备名，解决了大磁盘阵列下 /dev 目录爆炸。判定死因（内核文档 Documentation/ABI/removed/devfs 原文三条）：多年无人维护、存在不可修复的竞态、**命名策略放进了内核**。2005-08（2.6.13）标记废弃，2006（2.6.18）删除。继任者 udev（Greg K-H/Kay Sievers，OLS 2003 论文；LWN 2004-01《udev and devfs — The final word》）把架构倒过来：**内核只通过 sysfs 导出设备树与热插拔事件（uevent），命名/权限/别名等全部策略在用户态守护进程里做**。这与 BerryOS 的微内核哲学完全同构（BerryOS CLAUDE.md「微内核里把东西塞进内核是反模式」），且是 Linux 自己用五年撞墙后抵达的同一结论——宏内核也逃不过「策略出内核」的引力。连带效应：驱动模型统一（2.5 系列 kobjects/设备树，Pat Mochel）、固件加载也移到用户态。

**无稳定 ABI**。Greg K-H 的《The Linux Kernel Driver Interface》（Documentation/process/stable-api-nonsense.html）把「内核内部无稳定 API/ABI」立为铁律：换来的是三十年来内部代码无限重构自由。两面性：生态面上驱动全部被迫进主线，造就「什么硬件都有人写驱动」的护城河（AMD 最终放弃闭源 Catalyst、以 amdgpu 进主线是标准路径）；代价面上是 NVIDIA 式例外长期流血（二进制 shim 追内核接口，2012-06 Linus 镜头前对 NVIDIA 竖中指）、Android 厂商 BSP 长期落后主线（2010 年前后「Android mainlining gap」成为内核峰会常设议题，才逐步收窄）。**这是一条「当初被预言会失败、最终被市场判为成功」的决策——注意它成立的前提：主线有足够强的引力（开发者+用户），小系统抄这条会直接死于没人跟。**

## 7. cgroup/namespace → 容器/云：内核被「云」重新定义

时间线：mount namespace 2.4.19（2002）；cgroups 由 Google（Paul Menage、Rohit Seth）以「process containers」名义合入 2.6.24（2008-01）；其余 namespace 陆续补齐（user namespace 3.8, 2013）；LXC 2008、Docker 2013、Kubernetes 2014-15。要点不是时间线而是**权力结构变化**：Google 把数据中心运维需求直接写成内核原语，Docker/k8s 在用户态重组这些原语，「操作系统」的定义权从发行版转移到编排层——内核成为云的固件。两个坑与翻案：①cgroup v1 的多层级（每控制器独立挂树）当初以为是灵活性，「事后公认是坏主意」，v2 统一层级（4.5, 2016）翻案，systemd 以 hybrid 模式迁移了十年才消化；②「调度器死了吗」——粗粒度放置（k8s scheduler）上移用户态后，内核调度器仍是每核公平与延迟的最终裁判（cgroup cpu share 即其接口），并以 EEVDF/sched_ext/EAS（energy aware scheduling，5.x 时代为 ARM 大小核合入，存疑精确版本）回应新时代；同期 io_uring（5.1, 2019）与 eBPF（3.18, 2014 起）证明内核的下一战场是「安全可编程性」——**eBPF 本质上是「内核提供受限安全接口让外部策略介入运行时行为」，正是 BerryOS「AI 原生调度接口」设计的业界先例**。

## 8. 其他关键翻案补遗

- **a.out → ELF（1995）**：a.out 的共享库机制笨重、格式无扩展性，1.3.x 时代切换 ELF（Linus 当初并不热心，存疑其原话细节），glibc 同步支持。遗骸清理拖到 5.1（2019）标记废弃、5.18/5.19（2022）才删完——**一个被推翻的格式花了 27 年才离场，ABI 决策的惯性远超想象**。
- **5 级页表（4.12, 2017）**：x86-64 的 4 级页表/47 位 VA 被当成永久事实，大量用户态程序把 tagged pointer 元数据塞进高位比特。Intel LA57 到来后内核加第 5 级，却被迫默认仍在 47 位以下分配地址、用户态显式 hint 才给高位（docs.kernel.org/arch/x86/x86_64/5level-paging.html）——**内核迁就用户态的错误假设，因为 ABI 假设比硬件世代活得久**。
- **tickless（NO_HZ，2.6.21, 2007）**：Linux 出生 16 年后才砍掉空闲核的周期 tick——笔记本续航逼出来的。BerryOS 把它定为 day-one 基线，等于跳过 Linux 的整个前 16 年功耗债。

## 9. 关键人物与组织因素

- **Linus Torvalds**：工程师式仲裁者，「代码先于设计、能跑先于优雅」；多次公开表达对微内核理论的轻蔑却在实践中逐条吸收其主张。2018-09 行为守则事件后短暂退出并道歉——治理也在进化。
- **公司军团**：Red Hat（Ingo Molnar 等一大批 2.5/2.6 主力）、IBM（收购 Sequent 带来 RCU/McKenney）、Google（cgroups、Andrew Morton）、Intel（页表/调度）、Linutronix（工业客户供养 PREEMPT_RT 二十年）——**Linux 的技术路线图本质是这些雇主需求的最大并集**。
- **许可证政治**：Linus 拒绝升级 GPLv3（2006-07），内核永久停在 GPLv2-only；EXPORT_SYMBOL_GPL 成为对抗闭源模块的软武器；MINIX 当年受限的教材许可证（2000 年才放开）直接催生了 Linux。
- **Rust 进内核（6.1, 2022 合入基建）**：最新的「政治塑造技术」现场——C 维护者（如 DMA 维护者 Christoph Hellwig）阻挠 Rust 抽象层，2024-09 Rust for Linux 核心维护者 Wedson Almeida Filho 辞职，原话「nontechnical nonsense」；2025-02 Asahi 的 Hector Martin 亦退出；2025 年底 Rust 支持才摘掉「实验」标签。**三十年的 C 文化惯性是比任何技术问题都大的坑**——BerryOS 全 Rust day-one 恰好绕开此坑。

## 10. 对 BerryOS 的启示（克制版）

1. **别把论战读成「微内核赢了/输了」**。Linux 的市场胜利由协作结构（GPLv2+补丁流+无 ABI 承诺）而非架构优势驱动；而微内核的概念（策略外移、事件驱动、最小特权）被 Linux 花二十年逐条吸收（udev/eBPF/sched_ext）。BerryOS 以微内核+能力 day-one 起步，等价于跳过 Linux 的绕路，但要清醒：**BerryOS 缺的是 Linux 赢的原因——海量协作者与硬件引力**。结论：单仓小团队路线下，「机制/策略分离」的价值（可翻案性）比「微」本身更值得坚持；AI 调度接口（策略注入）正是把「可翻案」做成一等公民。
2. **读路径零开销 day-one 设计**（RCU 教训）。BerryOS 的 per-CPU 无锁遥测环、capacity 拓扑表、AI 调度遥测都是读多写少路径——按「读侧无原子、无屏障」标准设计；同时**克制发明新同步原语的数量**：RCU 用了十年才被社区消化，BerryOS 撑不起多个「需要十年教育的原语」。
3. **外部 ABI 与内部 API 分界纪律**（无稳定 ABI 教训）。Linux「内部随便动、入口 syscall 冻结」是它无限重构的前提；BerryOS 的 `berry_*` syscall ABI 一旦有第三方消费者，就必须把「冻结范围」白纸黑字写进《系统调用ABI设计.md》，内核内部接口明确宣布不稳定——这行字就是三十年 NVIDIA 之痛的疫苗。
4. **事件语义先于事件通道**（udev 教训）。「内核只暴露事件、策略在用户态」正是 BerryOS 哲学，但 udev 十年的坑都在事件可靠性/顺序/竞态上：uevent 丢失、冷启动顺序、命名规则的可预测性。BerryOS 定义热插拔/能力撤销事件时，day-one 就要写清丢失语义、顺序保证与重放机制，否则用户态策略层会重走 devfs→udev 的弯路。
5. **翻案要有制度化通道**（调度器与 VM 教训）。Linux 每次换调度器都伴随「思想被收编、作者被出局」的政治内耗。BerryOS 的调度策略是可插拔 trait + 多个 AI 策略注定竞争上岗——预先在文档里定好「什么证据（benchmark/关卡 G2-G4）能让一个策略默认」的客观标准，避免重演 O(1)→CFS 与 Kolivas 事件。PREEMPT_RT 的「激进分支长期压力测试+逐块上游化」则是 BerryOS 全可抢占阶段（G4）的现成路径模板。

## 11. 参考资料

- Wikipedia: Tanenbaum–Torvalds debate — https://en.wikipedia.org/wiki/Tanenbaum–Torvalds_debate （论战原文档案见 O'Reilly《Open Sources》附录 https://www.oreilly.com/openbook/opensources/book/appa.html）
- USENIX ATC 2001, Rik van Riel《Page Replacement in Linux 2.4 Memory Management》 — https://www.usenix.org/conference/2001-usenix-annual-technical-conference/page-replacement-linux-24-memory-management
- USENIX 2002 Kernel Summit VM session（van Riel/Arcangeli/Phillips） — https://www.usenix.org/conference/2002-linux-kernel-developers-summit/virtual-memory
- LWN 2004-01《udev and devfs — The final word》（Greg Kroah-Hartman） — https://lwn.net/Articles/44862/ ；OLS 2003 udev 论文与 USENIX 2003 峰会记录 — https://www.usenix.org/conference/2003-linux-kernel-developers-summit/managing-dynamic-device-naming
- 内核文档：devfs 移除说明 — https://docs.kernel.org/ABI/removed/devfs ；stable-api-nonsense — https://docs.kernel.org/process/stable-api-nonsense.html ；5-level paging — https://docs.kernel.org/arch/x86/x86_64/5level-paging.html ；CFS 设计 — https://docs.kernel.org/scheduler/sched-design-CFS.html ；EEVDF — https://docs.kernel.org/scheduler/sched-eevdf.html ；cgroup v2 — https://docs.kernel.org/admin-guide/cgroup-v2.html
- LWN 2007-12《What is RCU, Fundamentally?》（Paul McKenney） — https://lwn.net/Articles/262464/ ；McKenney《RCU Usage In the Linux Kernel: One Decade Later》(APSM 2013) — https://pdos.csail.mit.edu/6.828/2025/readings/rcu-decade-later.pdf ；Tree RCU 验证 — https://arxiv.org/abs/1610.03052
- kernelnewbies: BigKernelLock — https://kernelnewbies.org/BigKernelLock ；Linux.com 2011《Ding Dong, the Big Kernel Lock is Dead》
- Wikipedia: PREEMPT_RT — https://en.wikipedia.org/wiki/PREEMPT_RT ；realtime-linux.org（6.12 主线说明） — https://realtime-linux.org/getting-started-with-preempt_rt-guide ；FOSDEM 2025《Linux Kernel Mainline Real-Time History》slides；Linux.com 2021《In the trenches with Thomas Gleixner》 — https://www.linux.com/news/in-the-trenches-with-thomas-gleixner-real-time-linux-kernel-patch-set
- LWN 2023-03 EEVDF patchset 报道 — https://lwn.net/Articles/925371 ；Phoronix 2023-07 EEVDF lands in 6.6 — https://www.phoronix.com/news/Linux-6.6-EEVDF-Likely ；OSNews 2007 CFS/2.6.23 — https://www.osnews.com/story/18240/ ；Ars Technica 2007-07 Con Kolivas 退出 — https://arstechnica.com/information-technology/2007/07/con-kolivas-takes-parting-shot-at-desktop-computing-as-he-quits-linux-development/ ；LKML 2007-03 RSDL 公告 — https://lkml.org/lkml/2007/3/4/51
- LWN 2016-12/2017 5-level paging — https://lwn.net/Articles/716916 ；LKML RFC 2016-12 — https://lkml.iu.edu/hypermail/linux/kernel/1612.1/00383.html
- Wikipedia: a.out（5.1 废弃/5.18-5.19 删除） — https://en.wikipedia.org/wiki/A.out ；Wikipedia: Completely Fair Scheduler / Earliest Eligible Virtual Deadline First / Loadable kernel module
- The Register 2025-02《Rust for Linux project, despite doubt, has some support》 — https://www.theregister.com/software/2025/02/11/rust-for-linux-project-despite-doubt-has-some-support/
- 容器史：Aqua《A Brief History of Containers》 — https://www.aquasec.com/blog/a-brief-history-of-containers-from-1970s-chroot-to-docker-2016 ；cgroups(7) man page — https://man7.org/linux/man-pages/man7/cgroups.7.html
