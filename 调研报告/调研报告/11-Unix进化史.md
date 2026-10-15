# Unix 进化史：诞生与商业化时代（1969–1990s 中期）

> 调研日期 2026-10-15 · 视角：每个阶段真正在优化什么 / 踩坑与调整 / 被推翻的决策 · 方法：联网检索 + 全文抓取核实（Ritchie 一手回忆录、multicians.org、LWN、OSTEP、RFC、USENIX/ACSAC 论文、Wikipedia 多条目交叉，共 18 次）· 查不到确证处标「存疑」· 与 04（seL4/L4）、05（Zircon）、11（Rust 系 OS 四家）号报告重叠主题只交叉引用

## 0. 一句话定位

Unix 是一场从失败开始的减法：把 Multics 想一次做完的事（统一命名、保护环、动态链接、分时多用户）砍到只剩树形文件系统 + 字节流 + 进程管道三个正交原语，靠 C 重写获得可移植性，靠反托拉斯法令意外造就的源码开放拿下学术界，靠 BSD 拿下 VAX 与以太网时代——然后在商业化与标准战争中耗散技术红利，以「免费血统（BSD）与 GPL 路线（Linux）谁登基」的方式收官。它 25 年里的坑几乎都是同一类：**把当下的硬件与组织约束固化成了永久接口**。

## 1. 一页总览

**TL;DR**：Multics 死于资源错配（硬件与工具链喂不起愿景）；Unix 生于把同一愿景缩进几千字内存；C 重写用「最后 20% 性能」买了五十年可移植性，代价是五十年内存安全债；v6/v7 借 1956 年法令的副产品（源码近乎免费进大学）完成生态冷启动；BSD 用 DARPA 的钱吸收硬件跃迁（虚拟内存、TCP/IP、FFS）；AT&T 1984 年解禁后强行商业化，制造 System V 与 BSD 双头分裂；Unix 战争（1987–1993）以 SVR4 技术合流开始、以 COSE 政治合流与 USL v. BSDi 诉讼真空结束，免费 Unix 王位在真空期被 Linux 拿走。技术侧六大债——fsck、无状态 NFS、ioctl、setuid、X Window、SMP 巨锁——全部源于「为彼时硬件条件做一次性设计」。

| # | 年代/版本 | 在优化什么（核心矛盾） | 踩的坑 | 关键调整 |
|---|---|---|---|---|
| 0 | 1965–69 Multics（GE-645） | 多用户分时 + 统一命名（单级存储/保护环/动态链接） | 过度设计：PL/I 无生产级编译器、重编译数小时、与 GE-645 深度耦合 | Bell Labs 1969 退出 → 做减法 |
| 1 | 1969–71 PDP-7→PDP-11（汇编） | 在几千字内存上保住分时体验 | 无 fork/管道/重定向；chdir bug；文件偏移共享 | fork（27 行汇编）/exec/wait/管道/重定向定型 |
| 2 | 1973 v4（C 重写） | 可读、可改、可移植 vs 性能 | 性能损失争议；C 不安全埋五十年债 | 内核全 C + 少量汇编，多道程序定型 |
| 3 | 1975–79 v6/v7 | 学术传播（教学/研究） | 无商业支持；Lions 注释只能地下影印 | consent decree 反成传播引擎 |
| 4 | 1977–94 BSD/CSRG | 硬件跃迁：VAX 虚拟内存、以太网互联 | 32V 无 VM；System V 起初无 TCP/IP | DARPA 资助 → 4.2BSD（sockets+FFS） |
| 5 | 1981–87 System III/V | 产品化与收入（解禁后） | 双头分裂；授权费吓跑中小厂商 | Xenix 授权矩阵；产品线强化 |
| 6 | 1987–93 Unix 战争 | 统一与标准（止碎片化） | 阵营对抗；标准武器化；测试税 | SVR4 合流 → POSIX 止血 → COSE 妥协 |
| 7 | 1992–96 收官 | 源码归属与免费 Unix 出路 | USL v. BSDi 近两年法律真空 | 1994 和解 → 4.4BSD-Lite；Linux/GPL 登基 |
| 8 | 1979–2000s 技术债 | （贯穿）崩溃恢复/远程 FS/设备接口/特权/显示/多核 | fsck、NFS 无状态、ioctl、setuid、X、巨锁 | journaling、NFSv4、（未解）、capabilities（部分）、Wayland、细粒度锁 |

## 2. 逐阶段深挖

### 2.1 Multics（1965–1969）：资源喂不起的愿景

**优化什么**：MIT + Bell Labs + GE 合作的 Multics 想一步解决分时多用户的核心矛盾——多人共享一台机器时的命名、隔离与保护：层次文件系统、动态链接、保护环、单级存储（内存与文件统一编址）。这些今天全是常识，当时全是首创（multicians.org）。

**坑**：1967 年交付 MIT 的 GE-645 占地如一间房、造价数百万美元级，吞吐仅数十万指令/秒量级（Computer History Wiki 称约 435 KIPS，口径存疑）；PL/I 多年没有生产级编译器，重编译以小时计；系统与 GE-645 的分段/环硬件深度耦合，移植被当事人形容为 worse-than-Herculean。预算与进度失控后 Bell Labs 于 1969 年正式退出（Ritchie 回忆录）。Multics 本身没死：Honeywell 6180 上的版本 1970s 逐步可用，全球装机数十台，MIT 最后一台 2000 年才关机。

**怎么解决/代价**：Thompson、Ritchie 把「多人共享一台大机器」改写成「一台小机器上一个舒适的工作环境」。Multics 的统一命名留了下来并极端化为「一切皆文件」，保护环塌缩成用户/内核两态，动态链接与单级存储被砍——前者要到 1990s 共享库普及才以别的形式回归。这是 OS 史上最著名的一次「减法换生存」，常被当作第二代系统效应（second system effect）的教科书案例。

### 2.2 1969–1973：在几千字内存里重造分时

**优化什么**：资源约束下的体验。Thompson 的 Space Travel 在实验室 GECOS 机器上跑一局要 75 美元，他找到一台落灰的 PDP-7（带图形显示）用汇编重写；为了给游戏配工具，文件系统、汇编器、shell 逐步成型——Unix 是游戏的副产品（Ritchie 回忆录，本节事实多出于此）。文件系统在黑板上由 Thompson、Canaday、Ritchie 设计。

**坑与调整（连环踩、连环改）**：
- 早期形态极简陋：无路径名、目录是可写的普通文件、链接为任意有向图、每终端一进程、进程间靠 smes/rmes 原语传消息。
- **fork 只有 27 行汇编**——这是 Unix 最幸运的一次便宜：进程模型因此走 fork+exec 而非 vfork/线程先行。引入 fork 立刻暴露 chdir 的 bug（子进程改目录不影响父进程）→ shell 内建命令诞生；多进程共享文件偏移 → 内核必须维护打开文件表。两个补丁都成了永久语义。
- IO 重定向的思路直接来自 Multics 的 iocall/attach。
- 1972 年管道落地（McIlroy 从 1964 年起就念叨「像水管一样串起来」），最初记法 `sort input >pr>opr>` 只活了几个月——组合威力立刻压倒兼容性。
- PDP-11/20 以 65,000 美元购入，申请预算的名义是「专利部门的文字处理系统」，24KB 内存（16K 系统 + 8K 用户）、512KB 磁盘。

**取舍代价**：每个机制都要便宜，安全性（无 ACL）与可靠性（无崩溃一致性）被整体推给未来——这是 §2.9 全部技术债的共同起点。

### 2.3 1973 C 重写：可移植性 vs 性能的第一次大取舍

Thompson 本想给 PDP-11 写 Fortran，「撑了一周」产出了 B（经 BCPL/TMG），Ritchie 加类型演进成 C。1973 年内核用 C 重写，随 v4（1973 年 11 月）发布。Ritchie 自评：系统从此具备现代形态，最深远的变化是多道程序设计的引入；Unix 的成功很大程度上来自软件的可读性、可修改性与可移植性（回忆录原文大意）。

**坑与对价**：当时即有性能质疑（具体百分比无可靠数字，存疑）；更深的对价是 C 的类型与内存不安全给 setuid 程序、ioctl 族、缓冲区溢出埋了五十年的雷。用「最后 20% 性能 + 内存安全」换「五十年可移植性」，这笔账要到 Rust 才有语言级的重新结算（与 BerryOS 直接相关，见 §5；Rust 系 OS 当代对照详见 11 号报告）。

### 2.4 v6/v7 与 Lions：传播的意外引擎（1975–1979）

1956 年反托拉斯和解（consent decree）禁止 AT&T 从软件获利，Unix 源码只能收工本费随授权流出：大学近乎免费（数十至数百美元级），商业授权 20,000 美元（Wikipedia 口径），且含全部源码。Thompson 1975–76 学术假去 Berkeley，v6（手册 1975 年 5 月）随他登陆校园。John Lions 在 UNSW 为操作系统课写了 v6 内核逐行注释（1976–77），因授权禁止翻印，以影印本地下流传二十年，1996 年才正式出版（ethw.org）。1974 年 Ritchie & Thompson 的 CACM 论文让授权请求爆发；1983 年两人获图灵奖。v7（1979）收入 Bourne shell、UUCP、lint，是研究版集大成者，也是最后一个单一血统版本。

**坑**：无官方支持，各地自行修补——分叉从 v6 时代就开始，既是活力也是后来战争的种子。

### 2.5 BSD 与 CSRG（1977–1994）：硬件跃迁的吸收者

**优化什么**：VAX（32 位、VM 硬件）与以太网。32V（1978）把 v7 搬上 VAX 但没有虚拟内存；Bill Joy 与 Ozalp Babaoglu 在 3BSD（1979 前后）为 VAX 加上请求分页，打动 DARPA——后者自 1979/80 年起资助 Berkeley CSRG 至 1980 年代末（McKusick「Twenty Years of Berkeley Unix」）。产出：1BSD（1977，vi + Pascal 编译器）、4.2BSD（1983 年 8 月：TCP/IP + sockets API + FFS 快速文件系统）。4.2BSD 与 System V 同年发布且网络能力完胜——System V 当时没有内置 TCP/IP，Berkeley 挂出「4.2 > V」的海报（Unix wars 条目）。此后 4.3BSD（1986）、Tahoe（1988）、Net/2（1991，剔除 AT&T 代码的「干净」版）、4.4BSD（1994 年 6 月）；CSRG 于 1995 年发布 4.4BSD-Lite Release 2 后解散。

**坑**：CSRG 是资助驱动的研究组而非产品组织；Bill Joy 1982 年离开创办 Sun（SunOS 即 BSD 血统）——人才与代码双外流是 BSD 商业化的常态；Net/2 的「干净度」最终被 AT&T 送上法庭（§2.8）。

### 2.6 AT&T 商业化与 System III/V（1981–1987）

1984 年 1 月 1 日生效的 divestiture 解除了 1956 法令对 AT&T 卖计算机/软件的限制。AT&T 内部 Unix 组织（USG 一系）转产品化：System III 于 1981 年底官宣、1982 年首次对实验室之外商业发布；System V 于 1983 年 1 月接棒（其间的「System IV」仅存在于内部从未对外——unix.org/维基交叉）；1989 年再剥离为子公司 USL。策略失误：源码授权价格陡升把中小厂商推向 BSD；硬件尝试（Unix PC/3B1，1984–85）失败；微软 Xenix（1980 年起授权）走低端 OEM 自成一支。到 1980 年代中，市场已是 System V、BSD、Xenix 三大方言加数百家变体——Informix 称要支持超过 100 种 Unix 系统、逾 1000 个产品版本（Unix wars 条目）。

### 2.7 Unix 战争（1987–1993）：SVR4、OSF、POSIX

**爆发**：1987 年底至 1988 年初，AT&T 与 Sun 结盟（AT&T 并购入 Sun 股份），宣布以 System V 为基础统一 Unix，Sun 下一代 OS 将基于 SVR4。此举吓坏 IBM/DEC/HP——产品多为 BSD/自有血统，不愿把 OS 命脉交给既当裁判又下场的 AT&T：1988 年 5 月 OSF 成立（DEC/HP/IBM 牵头，要做 OSF/1、Motif、DCE），AT&T 阵营同年成立 Unix International 应战。

**技术合流**：SVR4（开发始于 1987，1989 年对外发布，个别资料记 1988 年末）合并 System V、4.3BSD 与 SunOS（含 NFS/OpenWindows）并吸收 Xenix 兼容性，技术上是成功的统一——Solaris 2、UnixWare 皆出于此；但阵营对立使它成为武器而非标准。

**标准**：POSIX.1（IEEE 1003.1-1988，名字出自 Stallman）定义可移植 API 底线，随后被 FIPS 151 变成政府采购强制项——这是解药；坑在于标准只能标准化「旧共识」，对网络细节、图形、实时、SMP 反应迟缓，而测试套件与商标（UNIX 95 / SUS 1994）成了新的入场税。1990 年 Barton Miller 组的 fuzzing 研究给乱象补刀：随机输入下四分之一到三分之一的 Unix 工具会崩溃或挂死。

**收官**：1993 年 3 月，昔日两阵营（HP/IBM/SCO/Sun/Novell-USL 等）成立 COSE——战争事实上结束；1993 年 6 月 AT&T 把 Unix 资产卖给 Novell，随后将商标移交 X/Open（1993 年秋宣布、1994 年完成，口径不一）；1994–95 年 X/Open 发布 Single UNIX Specification；1996 年 X/Open 与 OSF 合并为 The Open Group；1995 年 Novell 再把 UnixWare 卖给 SCO——商业 Unix 从此再无统一主角。

### 2.8 USL v. BSDi（1992–1994）：诉讼真空与 Linux 登基

1991 年 BSDi 成立，卖基于 Net/2 的商业 BSD；1992 年 4 月 USL 起诉 BSDi 与加州大学，主张 Net/2 含 AT&T 代码。戏剧性的是 AT&T 自己的专家证人最终只在约一万八千个文件量级的 Net/2 里找到极小部分争议代码（常被引用为 186 行/文件，口径不一，存疑）；UC 反诉并指出 System V 反而含大量未署名 BSD 代码（若成立 AT&T 可能失去 Unix 商标——据各方回忆录）。接手 USL 的 Novell 急于脱手，1994 年 2 月 4 日庭外和解：删除争议文件后发布 4.4BSD-Lite。FreeBSD、NetBSD（均 1993）以 Net/2 起家、1994–95 年重建于 Lite 之上，OpenBSD 1995 年从 NetBSD 分出。

**代价**：近两年法律真空正撞上 386BSD 停滞与 Linux（1991，GPL）起步——普遍认为这是免费 Unix 王座易主的关键窗口：BSD 阵营冻结不敢分发，Linux 无诉讼包袱地吸收了开发者。「源码授权模式」这个当初的法务细节，决定了此后三十年服务器世界的版图。

### 2.9 技术债清单（贯穿全程）

1. **fsck 之痛**：FFS/UFS 掉电后必须全盘扫描校验，工作量与盘容量线性增长——「能用但太慢」（OSTEP 崩溃一致性章），直接催生 1990s 的 journaling 浪潮与 McKusick 的 Soft Updates（fsck 论文，1990）。教训：崩溃一致性从 day-one 设计与事后修补，成本差一个量级。
2. **NFS 无状态**：Sun 于 1984–85 年设计 NFS，1989 年以 RFC 1094 规范化。刻意做成无状态——服务端崩溃恢复最简单（客户端超时重试）。代价：写必须同步落盘（慢）、非幂等操作（mkdir/remove）重试语义有歧义、一致性与缓存复杂度全部转移到客户端。NFSv4（RFC 3010/3530）最终回归有状态（租约 + compound 操作）。教训：状态复杂度守恒，不消失只转移。
3. **ioctl 杂物抽屉**：v7（1979）引入 ioctl（前身是 stty/gtty 程序），本想统一终端之外的设备控制，结果成为「每个设备自己发明系统调用」的后门：varargs 类型不安全、无统一命名空间、无法审计。LWN Neil Brown「Ghosts of Unix past」四部曲将其列为「conflated design」典型（part 2）。
4. **setuid 安全洞**：setuid（Ritchie 发明，曾获美国专利 4,135,240/1979）让进程「同时活在两个身份里」，是 Unix 权限模型的万能胶也是最大软肋：Bishop 1987 年写《How to Write a Setuid Program》，2009 年又在《Reflections on UNIX Vulnerabilities》里统计 25 年漏洞史；Chen 等（USENIX Security 2002《Setuid Demystified》）实证其语义混乱造成大量可利用漏洞。POSIX file capabilities（1990s 末起）只部分替代了它——至今没有完全推翻。
5. **X Window 妥协**：X 起于 MIT Project Athena（1984，Gettys + Scheifler；X11 于 1987 年 9 月），为「工作站 + 集中主机 + 网络显示」时代设计：网络透明 + mechanism-not-policy（什么策略都不定）。代价：本地渲染背负网络协议抽象、剪切板/拖放的 ICCCM 兼容地狱、安全模型停留在 xhost/xauth。1990 年就有人写《Why X Is Not Our Ideal Window System》，真正的清算要到 2008 年的 Wayland。
6. **SMP 无准备**：从 v6 到 SVR4/BSD，内核默认单 CPU——巨锁（一次只允许一个 CPU 进内核）是加 SMP 的最快补丁。商业 Unix（Sequent DYNIX、Solaris、IRIX）在 1980s 末–1990s 被迫重写为细粒度锁内核；Linux 的 BKL 从引入拖到 2011 年（2.6.39）才移除。教训：并发结构是地基级决定，事后改造成本以十年计。

## 3. 被推翻的决策清单

| # | 当初的郑重决策 | 当初的理由 | 推翻触发器 | 教训 |
|---|---|---|---|---|
| 1 | Multics 式全功能（单级存储/保护环/动态链接） | 分时安全的完整愿景 | Bell Labs 退出 + 硬件喂不起 → 减法 | 功能清单要跟着资源走 |
| 2 | 内核用汇编写 | 几 KB 内存，汇编即性能 | C 编译器成熟 + 1973 重写成功 | 可读/可移植 > 最后 20% 性能 |
| 3 | 假设目标机永远是 PDP-11（字节寻址、16 位、栈方向等假设散落代码） | 只有一种目标机很自然 | 1977 年 Interdata 8/32 移植暴露全部隐藏假设（Ritchie 1978 可移植性论文） | 硬件假设必须显式收敛进 HAL |
| 4 | a.out 二进制格式 | 简单够用 | 共享库/动态链接需求 → System V 改用 COFF，SVR4 定 ELF（USL+Sun 设计） | 格式/ABI 要为尚未出现的链接方式留结构 |
| 5 | 交互设备 = 字符终端（内核内行编辑、tty/termios 栈） | 终端就是串口 | 位图屏、X 终端、网络登录（pty 打补丁） | 别把外设形态烧进内核 I/O 栈 |
| 6 | NFS 无状态协议 | 崩溃恢复最简 | 重试歧义 + 性能 + 缓存复杂度 → NFSv4 回归有状态 | 状态复杂度守恒 |
| 7 | 单 CPU 巨锁 | 单机单核天经地义 | SMP 商用机出现 → Solaris/IRIX 细粒度锁重写 | 并发结构 day-one 决定十年成本 |
| 8 | setuid 全域提权 | 实现最省事 | 25 年漏洞史（Bishop/Chen）→ capabilities 部分替代 | 特权模型不能靠「临时变成 root」 |
| 9 | 源码随授权近乎免费扩散 | 1956 法令不许获利的被动选择 | 1984 解禁后收紧 → 需求真空由 BSD/GPL 两条免费路线填补，诉讼分出胜负 | 授权模式是战略决策不是法务细节 |

补充观察：被「彻底推翻」的是 3/4/5/6/7；1/2 是被继承者推翻；8 至今只被部分替代；9 是「被动决策造就盛世、主动回收失去天下」的最讽刺案例。另一条贯穿线：a.out → COFF → ELF 在 Linux 上重演（1993–94 年引入 ELF，2019 年内核移除 a.out 支持，细节口径存疑）。

## 4. 关键人物与组织因素

**人与性格**：每个转折都有强烈的个人动因——Thompson 为玩游戏找到 PDP-7；PDP-11 预算靠「专利部门文字处理系统」的伪装拿到（Ritchie 回忆录）；管道是 McIlroy 从 1964 年念到 1972 年的执念；Lions 用一份注释把 v6 变成全球操作系统课教材；Bill Joy 一人贡献 vi、csh 与 BSD 大半工具，1982 年带走 BSD 血统创办 Sun，又把 SunOS/NFS 反向塞回 Unix 主线；McKusick 掌舵 CSRG 十年（FFS/fsck/Soft Updates）；Thompson 1975–76 在 Berkeley 的学术假被普遍视为 BSD 火种的直接来源。

**组织与政治三条主线**：
1. **法律塑造传播**：1956 consent decree 禁 AT&T 卖软件 → 源码进大学 → 人才与补丁回流 → 生态冷启动；1984 divestiture 解禁 → 商业化 → 授权费与封闭倾向 → 把生态推向 BSD 与后来的免费路线。
2. **政府资助塑造接口**：DARPA 选择资助 Berkeley（而非任何公司）→ TCP/IP 以 BSD sockets 的形态成为事实标准——资助结构直接决定了今天互联网编程接口的形状。
3. **战争结构**：AT&T+Sun（1987 结盟、AT&T 入股）vs OSF（1988，DEC/IBM/HP 等）——硬件厂商不愿把 OS 命脉交给一个既当裁判又下场的对手；最终由 Novell（1993 买 USL）、X/Open（拿商标）与 COSE（1993）合力拆弹，1996 并入 The Open Group。Richard Gabriel 事后提出的「worse is better」（1989–91 年间文章）是对这个时代的回顾式标签：牺牲完整性换简单与可传播性——后见之明，但传神。

**许可证政治**：BSD 宽松条款与 GPL 的分岔发生在 1989–1992（GPL v1 1989/v2 1991；BSD 四条款定型约 1990；386BSD 1992），USL v. BSDi 的真空期（1992–94）让「无诉讼风险的 GPL 路线」胜出服务器市场——一条技术含量为零的决定，权重超过此前二十年的大部分技术决策。

## 5. 对 BerryOS 的启示

只列五条，均直接对应本报告史实：

1. **原语要少而正交，接口纪律 day-one**。Unix 的不死身来自三个正交原语（文件/管道/进程）而非任何单点技术；ioctl 的教训是接口一旦失去统一模型就永远收不回来。BerryOS v1 约 26 个 syscall + object_* 合并（系统调用ABI设计）应坚守「新需求先问能否用既有原语组合」；设备/终端/显示控制不要长出 ioctl 式后门——Zircon devfs 的弯路是同一条坑的现代复刻（详见 05 号报告）。
2. **硬件假设显式化，可移植性一次付清**。Interdata 移植暴露的散落假设正是「架构无关优先 + 架构相关/<arch> 收敛 HAL」的历史依据；C 重写换可移植性但欠下五十年内存安全债，全 Rust 是同一取舍的现代解——别在性能压力下绕过 HAL 局部硬写。对照 04 号报告（seL4 用形式化验证偿还同类债）与 11 号报告（Rust 系路线）。
3. **并发与省电结构不能后补**。巨锁清理让 Linux 花了十五年，fsck 到 journaling 差一代——对应 BerryOS 细粒度锁分阶段 + G0–G5 关卡验证（测试策略），以及 tickless/NO_HZ day-one：为明天的硬件形态设计，别把「QEMU virt + 单核」的当下条件固化进接口（capacity 0..1023 抽象正是反例预防）。
4. **状态复杂度守恒，选址要跟失败恢复一起设计**。NFS 无状态把复杂度推给客户端缓存，NFSv4 还是回到有状态+租约。BerryOS 的 IPC（同步 channel + PM/AI 遥测走共享环形缓冲）本质是在「状态放内核还是放用户态服务」之间做显式选址——每处选址都应配套失败恢复路径（PM 可崩、内核降级保活、监督重启即为此而设）。
5. **授权与兼容层是战略，不是法务细节**。consent decree 的意外开放造就 Unix 生态，解禁后的收紧又把它让给 Linux；Fuchsia 演示了自有 ABI 无兼容层的七年孤岛（05 号报告警示）。BerryOS 的协议洁净（GPL 不进内核谱系）与 POSIX 兼容库「早于生态预期落地」（v1 无 fork 的边界要明示）是把别人的历史教训直接写进自家决策表的两处。

## 6. 参考资料

一手与官方：
1. D. M. Ritchie, The Evolution of the Unix Time-sharing System（1979）：https://www.bell-labs.com/usr/dmr/www/hist.html （webReader 全文抓取，本报告 §2.1–2.3 多处直接依据）
2. Dennis Ritchie 纪念页（含早期授权价目存档）：https://www.nokia.com/bell-labs/about/dennis-m-ritchie/licenses.html
3. multicians.org 项目史：https://www.multicians.org/history.html
4. D. M. Ritchie, Portability of C Programs and the UNIX System（Bell Labs CSTR，1978；经检索核实，未直接抓取）
5. IEEE Spectrum, The Strange Birth and Long Life of Unix（2019）：https://spectrum.ieee.org/the-strange-birth-and-long-life-of-unix
6. Computer History Museum, The Unix Revolution（Unix 50 周年）：https://computerhistory.org/blog/the-unix-revolution/
7. M. K. McKusick, Twenty Years of Berkeley Unix（Open Sources, 1999）：https://www.oreilly.com/openbook/opensources/book/kirkmck.html
8. The Open Group 官方 Unix 大事年表：https://www.unix.org/unix_history.html

论文/教科书/规范：
9. RFC 1094（NFSv2）：https://www.rfc-editor.org/rfc/rfc1094 ；RFC 3530（NFSv4.0）：https://www.rfc-editor.org/rfc/rfc3530
10. OSTEP「Crash Consistency: FSCK and Journaling」与分布式文件系统/NFS 章：https://pages.cs.wisc.edu/~remzi/OSTEP/
11. M. K. McKusick, Fsck—the UNIX File System Check Program（Computing Systems, 1990；经检索核实）
12. H. Chen 等, Setuid Demystified（USENIX Security 2002）；M. Bishop, How to Write a Setuid Program（1987）、Reflections on UNIX Vulnerabilities（ACSAC 2009）（均经检索核实）
13. Neil Brown, Ghosts of Unix past 四部曲（LWN, 2010）：part 2（ioctl）：https://lwn.net/Articles/412131/ ；part 4（setuid）：https://lwn.net/Articles/416494/

百科交叉（年代/数字均经多来源对照，存疑处已在正文标注）：
14. History of Unix：https://en.wikipedia.org/wiki/History_of_Unix
15. Unix wars（fuzzing 研究、COSE、Novell/X/Open 时间线）：https://en.wikipedia.org/wiki/Unix_wars
16. Berkeley Software Distribution：https://en.wikipedia.org/wiki/Berkeley_Software_Distribution
17. USL v. BSDi：https://en.wikipedia.org/wiki/USL_v._BSDi
18. X Window System：https://en.wikipedia.org/wiki/X_Window_System
19. Giant lock（BKL 与 Linux 2.6.39）：https://en.wikipedia.org/wiki/Giant_lock
20. a.out / ELF：https://en.wikipedia.org/wiki/A.out ；https://en.wikipedia.org/wiki/Executable_and_Linkable_Format
21. John Lions（ETHW）：https://ethw.org/John_Lions
22. Multics（交叉 multicians.org）：https://en.wikipedia.org/wiki/Multics

另：P. Salus《A Quarter Century of UNIX》（1994）为该时期叙事常用通史参照（未直接引用）。
