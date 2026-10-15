# CP/M→DOS→Windows：历史债滚雪球五十年（1974–2026）

> 调研日期 2026-10-15 · 关键年代/金额/事件均经联网核实（见 §10），查不到确证处标「存疑」 · 与 01–11 号报告重叠的主题（微内核 IPC 成本、Zircon 对象模型、QNX 架构等）只做交叉引用，篇幅留给「进化史/坑史」视角。

## 0. 一页总览

**TL;DR**：这条谱系不是一部「技术进步史」，而是一部「历史债复利史」。五十年里每个阶段真正在优化的都不是「更好的操作系统」，而是**上市时间 / 兼容性 / 生态**这三样商业变量；每次都把技术上的欠账往后滚，直到某个阶段还不起（Win3.x 的协作调度→Win9x 的 DOS 根→Longhorn 的整体失控），再由一条平行产品线（NT）或一次外科手术式重置（2004 Longhorn reset）来「破产重组」。重组从未清偿全部债务——今天 Windows 11 里仍同时住着 1981 年的 DOS 文件名语义、1993 年的 NT 注册表、1996 年入核的 win32k、和 2019 年内嵌的 Linux 内核。对自研 OS 最大的启示是：**ABI 一旦发布就是四十年合同；高频路径跨特权界必输；大爆炸重写必翻车；生态打不过就内嵌**。

| 年代/版本 | 在优化什么 | 踩的坑 | 关键调整 |
|---|---|---|---|
| 1974–1981 CP/M→86-DOS→MS-DOS | 上市时间（IBM 期限）与授权结构 | 为赶期限买来的「快速脏」系统：16 位实模式、640KB、无内存保护 | 无——债从第一天滚起 |
| 1985–1992 Windows 1.0–3.x | 在 DOS 壳上长出 GUI+多任务，且兼容一切 DOS 软件 | 协作调度+共享地址空间，一个应用可挂死/写挂全系统 | 386 增强模式引入分页与 VM86，但 Windows 应用间仍不隔离 |
| 1988–1996 NT 3.1→4.0 | 可移植性、安全、C/S 子系统架构（企业市场） | 子系统消息传递拖慢图形；多架构端口维护不动 | **GDI/USER 移回内核**（微内核倾向落成混合内核）；RISC 架构逐个弃守 |
| 1995–2000 Win95/98/ME | 带 DOS 兼容的 32 位化（消费市场） | 16 位代码全局串行化+VxD 全核权限=蓝屏文化 | ME 只是「藏起」实模式 DOS；最终整条 9x 产品线退役 |
| 2001–2007 XP→Longhorn→Vista | 双线合一；然后想一次清完所有债 | 大爆炸重写失控，2004 年推倒重来 | 基于 Server 2003 重启；WinFS（数据库文件系统）第二次死亡 |
| 2012–2013 Win8/Windows RT | 触屏+ARM+封闭新生态 | 无桌面兼容也无手机生态，两头落空 | 放弃 RT 锁定策略，回归 x86 |
| 2016–2020 WSL1→WSL2 | 开发者生态（Linux） | syscall 翻译层永远追不完 Linux 语义 | **内嵌真 Linux 内核**进轻量 VM |
| 2017–2026 Windows on ARM 复兴 | 功耗/NPU/Always-Connected | RT 教训重学一遍：模拟性能+原生软件荒 | x64 模拟（2020）→Prism（2024）→Copilot+ PC 原生优先 |

## 1. 起源债：CP/M→86-DOS→MS-DOS（1974–1985）

**在优化什么**：IBM 1980 年决定一年内做出 PC，操作系统必须立刻有。微软自己没有 OS，而 Digital Research 的 CP/M-86 迟迟交不出来（IBM 与 Kildall 的会面因保密协议与授权模式谈崩，细节各源说法不一，存疑）。微软的解法不是「写一个」而是「买一个」：先以 **2.5 万美元**拿到西雅图电脑产品公司（SCP）86-DOS 的非独占授权，1981 年 7 月再花 **5 万美元买断全部权利**（合计 7.5 万美元）。Tim Paterson 在 SCP 用几个月写出 QDOS（"Quick and Dirty Operating System"），API 刻意模仿 CP/M 以便移植软件——**「兼容 CP/M」这个临时决定，就是后来一切兼容债的第一笔**。

**三个商业事实决定了四十年**：
1. IBM 当年 PC DOS 售价 **40 美元**，而迟到的 CP/M-86 卖 **240 美元**，用户用脚投票（有统计称约 96% 选了 DOS，数字存疑但量级可信）。
2. 微软与 IBM 的合同是**非独占**的：IBM 不能转授权 DOS，微软却可以自由卖给任何厂商。兼容机浪潮一起来，MS-DOS 自动成为整个行业标准——这被认为是对手 IBM 最致命的合同失误。
3. SCP 后来知道真相后于 1986 年起诉微软索赔 6000 万美元（隐瞒 IBM 关系低价买断），同年 12 月以 **92.5 万美元和解**。

**技术债清单（第一天就埋下）**：8088 实模式 1MB 地址空间，顶部 384KB 留给 ROM/显存 → **640KB conventional memory** 上限（「640K 对谁都够了」的名言广为流传，但盖茨本人否认、无原始出处，存疑）；**零内存保护**——任何程序可直接改写系统区域和中断向量表；**TSR**（驻留程序）靠截获中断实现「多任务」魔法，钩子冲突成为整个 DOS 时代的日常。后来 EMS 扩展内存（1985 年 Lotus/Intel/Microsoft 银行切换）、XMS/HMA、QEMM 内存重排工具，全是在给这 640KB 打补丁——**为一个 1980 年的地址空间布局，行业打了十年补丁**。

## 2. 壳上盖楼：Windows 1.0–3.x（1985–1992）

**在优化什么**：Macintosh 逼出来的 GUI，但硬约束是「必须跑在既有 DOS 机器和 DOS 软件上」。Windows 1.0（1985-11）到 3.0（1990-5，销量爆炸）、3.1（1992-4），本质都是**一个 DOS 程序**。

**为什么注定脆弱**（架构层面，非实现层面）：
- **协作式调度**：调度只在应用调用 `GetMessage` 取消息时发生。一个忘了让出的应用就冻结整个 UI；Ctrl-Alt-Del 经常也无济于事。
- **共享地址空间**：所有 Windows 应用住在同一内存空间，互相之间零隔离，一个野指针就是一次 GPF（General Protection Fault，3.0 时代叫 UAE）弹窗带走全会话。
- 286 标准模式/386 增强模式的分化是为了用上保护模式与分页（DOS 沙盒走 VM86），但**Windows 应用之间依然共享空间**——分页硬件在，隔离策略不在。

**取舍代价**：微软完全清楚这套设计的脆弱，但「跑用户的 286 和 DOS 软件」压倒一切。这是「兼容性 > 正确性」第一次成为平台的显式工程原则，此后四十年反复重演。

## 3. 半步真核：Windows 95/98/ME（1995–2000）

**在优化什么**：消费市场的 32 位化（抢占式多任务、长文件名、即插即用），同时**一行 DOS 软件都不能牺牲**。Win95（1995-8-24）做到了对 32 位应用的抢占调度——但仅仅是 32 位部分。

**坑的解剖——蓝屏文化的技术根源**：
- 16 位代码没有消失：大量系统组件与驱动仍是 16 位，**16 位模块被全局串行化**（著名的 Win16 全局锁），一个 16 位应用能卡住所有 32 位线程。
- 引导链仍是 DOS：IO.SYS、CONFIG.SYS、AUTOEXEC.BAT、实模式驱动照样加载，DOS 的内存布局原封不动。
- **VxD 驱动运行在 ring 0** 且无任何隔离，一个有 bug 的显卡/声卡驱动直接蓝屏。「蓝屏」从罕见事故变成大众文化符号，就是 9x 时代的事——不是 Windows「烂」，是**驱动生态被授予了全核权限而内核无力自卫**。
- Windows ME（2000-9）号称移除实模式 DOS，实际只是**藏掉了进 DOS 的入口**（注册表 NoRealMode，可被解开），引导仍走 DOS 路径。「去掉 DOS」是营销，兼容债一分未还。

**最终调整**：不是修 9x，而是**整条产品线退役**——2001 年 XP 把消费用户整体迁到 NT 内核。兼容债的最后清偿方式是「换地基」。

## 4. 平行宇宙的真内核：Windows NT（1988–2001）

**在优化什么**：企业市场——可移植、安全、健壮、32 位。这条路与前述消费线平行跑了八年，是全谱系里唯一「先设计后编码」的内核。

**VMS 血统与组织因素**：Dave Cutler 是 RSX-11M 与 VAX/VMS 的架构师，其在 DEC 的下一代项目 PRISM/Mica 1988 年被砍，**他 1988 年 10 月带核心团队投微软**——DEC 亲手把 NT 送给了对手。项目最初叫 **NT OS/2**（与 IBM 联合），1990–91 年微软-IBM 因 Windows 3.0 的成功决裂后才改挂 Windows 品牌；「NT」一说来自最初目标处理器 i860 的代号 **N-Ten**（Cutler 等原始开发者自述），而且**刻意先移植到非 x86 处理器以逼出干净的 HAL 抽象**。业界流传 VMS→WNT 每个字母进一位的谱系笑话（W+1=V… 实为民间 Lore）。DEC 1997 年起诉微软（NT 涉用 VMS 商业机密），很快和解：微软支付约 **1 亿美元量级**（各源口径不一，存疑）并承诺维持 Alpha 版 NT——而 Compaq 收购 DEC 后 1999 年 8 月自己砍掉了 Alpha NT。

**微内核倾向为何落成混合内核——微内核史上的重要数据点**：NT 3.x 把环境做成用户态 C/S 子系统（Win32/OS2/POSIX 经 LPC 消息与内核通信），图形 GDI/USER 也在用户态 win32k 前身里。代价是**每次图形调用都要跨特权界做上下文切换**，交互性能被 Win95 惨烈对比。NT 4.0（1996-8）把窗口管理器与 GDI 整体移入内核驱动 **win32k.sys**，图形性能立竿见影——代价是：图形驱动错误现在等于内核错误（BSOD），且 win32k 后来膨胀为**内核攻击面重灾区**，迫使微软在 Win8.1/10 再做 win32k lockdown 往回补。这正是纯微内核路线在桌面负载上的经典反例（成本量化对照见 04 号报告 seL4/L4 家族的 IPC 开销分析；Zircon 如何用「务实派对象内核」回避此坑见 05 号报告）。

**API 债的账本**：Win32 成为赢家后，NT 的三个子系统中——**OS/2 子系统**只为字符模式 1.x 应用存在（为兑现 IBM 联盟遗产），Windows 2000 移除；**POSIX 子系统**是为美国政府订单的合规摆设，几乎无人真用，其后身 Interix/SUA 一路续命到 Windows 8 才移除——「为合同做的兼容层不会自我维持」。**WOW64**（x64 上跑 Win32）则证明兼容层一旦承载真实生态就删不掉了。**注册表**从 Windows 3.1 的 INI 文件乱象中诞生，最终自己变成新的单点故障与「registry rot」。

## 5. 债务总清算与再欠债：XP→Longhorn 大重置→Vista（2001–2009）

**XP（2001-10）**：双线合一，9x 用户搬到 NT。随后的 **XP SP2（2004）** 把两年产能押在安全上——这是「先欠安全债、后停产还债」的第一次大规模演习。

**Longhorn（2001–2004）**：微软决定一次清完所有债，同时上马三大件——WinFS（关系型存储）、Avalon（WPF 新图形）、Indigo（WCF 通信）。到 2004 年代码库已失控（连每日自举编译都困难），2004 年 8 月宣布**重置**：丢掉现有代码，基于 Windows Server 2003 SP1 代码库重新起步（媒体讥为 "Shorthorn"）。「三年开发、一键清零」是软件工程史上最著名的大爆炸重写失败案例之一。

**WinFS 与「数据库文件系统」迷思**：这个梦做了十五年、死了两次——Cairo 项目的 OFS 对象文件系统（1991–1996，从未发布）→ 中间还有 Storage+ 尝试 → WinFS（基于 SQL Server 技术）2006 年 6 月正式取消，部分思路化入后续搜索/元数据特性。「用数据库统一所有数据」每次听起来都是下一代范式，每次都死在**迁移成本、性能、与既有文件语义的兼容**这三座山下。

**Vista（2007-1）**：重置后的产物。为支撑新图形栈强推 **WDDM 新驱动模型**，与旧 XPDM 不兼容，而显卡厂商早期驱动极不成熟——微软自家崩溃数据显示 **2007 年近三成 Vista 崩溃来自 NVIDIA 驱动**（居各原因之首，微软自家组件约 18% 居次）。Vista 的口碑就此殉葬，Win7（2009）靠「Vista 修好版」翻身。教训：**驱动模型重构的正确性最终由 ISV 生态的成熟度决定，发布日期决定不了**。

## 6. 现代和解：WSL1→WSL2、Windows on ARM 两起两落（2012–2026）

**Windows RT（2012-10-26）**：第一起。ARMv7 上的 Win8，锁死只能装商店应用+Office——既没有桌面 x86 兼容（Windows 的立身之本），又没有手机生态，两头落空；Surface RT 留下约 **9 亿美元库存减记**（2013，广泛报道）。同期 Windows Phone/Windows 10 Mobile 线也在 2017 年官宣放弃（支持延至 2019 年底收尾）——第一落。

**WSL1（2016-8-2，随 Win10 周年版）**：NT 内核的 pico process + **lxcore.sys 系统调用翻译层**，把 Linux syscall 即时译成 NT 原语。技术上堪称 NT 子系统思想的最后一次豪华演出，但两个硬伤无解：syscall 覆盖率永远追不上 Linux 内核的演进（eBPF、namespace 语义细节……），且 Windows 文件路径上的 I/O 性能惨淡。

**WSL2（2020-5，随 Win10 2004 版 GA）**：推翻翻译层，**在轻量工具 VM 里内嵌一个真 Linux 内核**——官方对比文档明说目标就是「完整 syscall 兼容 + 文件系统性能」。这是架构哲学的代际转变：NT 用了 25 年证明「别人家的 ABI 可以用子系统/翻译层模拟」（OS/2、POSIX、Interix、SUA、Astoria、WSL1 六连败），最后承认**生态不可模拟，只能内嵌**。「打不过就内嵌」。

**Windows on ARM 第二起**：2017 年宣布回归（骁龙平台，Always Connected PC），2020-11 补上 x64 模拟，2024 年 Prism 模拟器+Copilot+ PC（骁龙 X，原生 ARM64 优先）把 ARM 从「省电乞丐版」重新定位成「AI 原生一等公民」。两起两落的完整弧线：**1993 年 NT 原生支持 RISC → 1999 年全部弃守 → 2024 年 ARM 以 NPU 为筹码回归**——被推翻的架构决策，三十年后以别的理由翻案。

**浏览器债的终局**：IE 从 Win98 起与 shell 深度捆绑（美国诉微软案 1999 年事实认定书直指 IE 不可卸载），打赢了浏览器战争却错过了移动时代；最终 Edge 弃用自研引擎改投 **Chromium（2020-1-15 发布）**，IE11 于 **2022-6-15 退役**。「浏览器是 OS 的一部分」这个 1996 年的郑重决策，在 26 年后被彻底推翻——和 WSL2 一样，都是「打不过就加入」。

## 7. 被推翻的决策清单

| 决策 | 当初理由 | 推翻触发器 | 教训 |
|---|---|---|---|
| NT 首发 i860/MIPS，全盛期四架构（x86/MIPS/Alpha/PPC） | Cutler 强制可移植性；RISC 工作站是企业市场主流 | RISC 工作站市场消亡、Compaq 1999 砍 Alpha、维护成本压垮销量 | HAL 抽象值得做；但别为不存在的客户维护端口 |
| GDI/USER 放用户态（NT 3.x C/S 架构） | 子系统隔离、可移植、贴近微内核学术潮流 | 图形性能被 Win95 碾压，NT 4.0 移入内核 | 高频路径跨特权界必输；但入核的 win32k 变成 25 年的安全税 |
| OS/2 子系统 | 兑现与 IBM 的联盟（NT 本名 OS/2） | 1990–91 联盟决裂，Win32 转正 | 为政治/合同做的兼容层，失去靠山那天就死了 |
| POSIX 子系统/Interix/SUA | 政府订单合规 | 形同虚设三十年，Win8 移除；WSL 才真正解决 | 合规用兼容层 = 没有兼容；真需求只能内嵌真内核 |
| WinFS/Cairo OFS | 统一数据存储的「下一代范式」愿景 | 2004 Longhorn 重置；2006 正式取消 | 同一个梦 15 年死两次：迁移成本+性能+兼容三座山，愿景赢不了 |
| IE 与内核/shell 捆绑 | 击败 Netscape，护住 API 入口 | 反垄断案+移动时代归零，Edge 改用 Chromium | 绑定赢得战役、输掉战争；对手变成开源引擎时捆绑无物可绑 |
| 9x 线保 DOS 兼容 | 数亿用户与软件资产 | ME 假装告别失败后，XP 直接淘汰整条产品线 | 兼容债无法续期清偿时，只能连产品线一起退役 |
| WSL1 系统调用翻译层 | 轻量、快启动、与 NT 深度集成 | syscall 追不完+文件性能差，WSL2 内嵌真内核 | ABI 全集不可模拟；生态级目标直接内嵌真实现 |

## 8. 关键人物与组织因素

- **Gary Kildall / DRI**：错过 IBM 会面（细节存疑）+ CP/M-86 迟到 + 高定价，技术领先者输给了时间窗口——**OS 市场只奖励准时的人**。
- **Tim Paterson / SCP**：四个月写出 QDOS 的工程奇才，9.25 万美元卖掉改变世界的系统，1986 年再拿 92.5 万和解——信息不对称下的交易没有「公平价」。
- **Dave Cutler**：RSX→VMS→NT 三代内核一脉相承，DEC 砍 PRISM 直接促成人才转移；DEC 再以诉讼追认这笔损失。**组织砍项目 = 给对手送架构师**。
- **IBM–微软联盟与决裂**：决定了 NT 的双重身份（OS/2→Windows）、Win32 的转正、以及 IBM 自己 OS/2 的边缘化。平台公司的联盟结局由上一代产品的销量（Win 3.0）单方面改写。
- **司法部反垄断（US v. Microsoft）**：1999 年事实认定 + 2001 年和解（拆分被上诉推翻），捆绑策略活了下来——但真正的清算二十年后由市场（移动时代）执行。
- **商业节奏**：圣诞/开学档期驱动发布日期，与内核重构的三年周期天然冲突——Longhorn 重置就是「营销日历绑架工程」的总爆发；重置后微软建立的分叉管理/ship 节奏纪律（Sinofsky 时代）才是 Win7 翻身的地基。

## 9. 对 BerryOS 的启示

1. **ABI 是四十年合同，签之前想清楚**。DOS 证明「为赶期限拼装的最小接口」会被市场冻结成几代人的标准；Win32 证明接口面只增不减、每版都在为上版付租。BerryOS 的 `berry_*` ~26 个 syscall + `#[repr(C)]` 镜像同步（见 `系统调用ABI设计.md`）应在 v1 就带**版本化与扩展保留位**，POSIX 兼容库严格独立于内核 ABI——「v1 无 fork」这类边界要写成可审计的决策而不是默契。
2. **高频路径不许跨特权界**。NT 3.x→4.0 的 GDI 入核是微内核史上最贵的性能学费（对照 04 号报告的 IPC 成本量化）。BerryOS 已把「PM 走共享环形缓冲、禁通用 IPC 消息拷贝」写进决策，AI 调度的遥测环必须同样对待：慢环可丢可聚合、快环零拷贝，且这个区分应固化在 ABI 层而非某个驱动的实现细节里。
3. **别让 AI 调度成为 BerryOS 的 WinFS**。WinFS/Cairo 的教训不是「愿景错」而是「把最大的不确定项和主线绑死」。BerryOS 的 AI 原生调度已规划 B（启发式）先行、A（学习型）后置、护栏与遥测 ABI 先落地（`AI调度设计.md`）——这是对的顺序；守住「内核只留第一类接口、策略在用户态」的边界，避免任何「等 AI 策略服务成熟再发版」的路径依赖。
4. **兼容策略要写「清偿触发器」**。WSL1→WSL2 证明 ABI 全集不可模拟、内嵌真实现是合法出路。BerryOS 决定不跑 Linux 二进制，但应在 ADR 里预先写明反转条件（例如：某类生态缺口成为采用瓶颈时，Linux 兼容走**用户态服务/VM 方案而非内核 syscall 翻译层**，并引用 WSL1 六连败作为否决理由）——把「当初为什么不做」文档化，20 年后才不必重学。
5. **多架构的冷热循环**。NT 先为可移植性选 i860，再回 x86 独大三十年，ARM 直到有 NPU 这个新筹码才翻案。BerryOS 的做法（核心逻辑架构无关 + `架构相关/<arch>/` 收敛 + v1 只出 ARM64、预留 x86_64/RISC-V 位）与之同构：**HAL 边界要可验证（「移植只动 HAL」作为测试断言），但不为不存在的客户提前付端口税**。

## 10. 参考资料

- 86-DOS/QDOS 与收购：https://en.wikipedia.org/wiki/86-DOS ；SCP 诉讼与和解（UPI 1986 报道索引）：https://en.wikipedia.org/wiki/Seattle_Computer_Products
- CP/M-86 与 $40/$240 定价：https://en.wikipedia.org/wiki/CP/M-86
- MS-DOS 非独占授权结构：https://retrocomputing.stackexchange.com/questions/8344/did-ibm-encourage-bill-gates-to-retain-the-rights-over-pc-dos
- Win 3.x 协作多任务与共享地址空间：https://en.wikipedia.org/wiki/Cooperative_multitasking ；https://retrocomputing.stackexchange.com/questions/791/how-did-windows-3-1-implement-multitasking
- Windows ME 与实模式 DOS：https://en.wikipedia.org/wiki/Windows_Me
- Windows NT / Cutler / N-Ten 得名与多架构时间线：https://en.wikipedia.org/wiki/Windows_NT ；https://en.wikipedia.org/wiki/Dave_Cutler
- NT 4.0 GDI 入核与 win32k 攻击面：https://unit42.paloaltonetworks.com/win32k-analysis-part-1 ；Mandt, "win32k.sys"（BlackHat USA 2011）：https://media.blackhat.com/bh-us-11/Mandt/BH_US_11_Mandt_win32k_WP.pdf
- 子系统存废（OS/2 子系统 Win2000 移除、POSIX→Interix→SUA→Win8 移除）：https://news.ycombinator.com/item?id=33770141 ；https://en.wikipedia.org/wiki/POSIX_subsystem_for_Windows
- Longhorn 重置与 Vista 开发史：https://en.wikipedia.org/wiki/Development_of_Windows_Vista
- Cairo/OFS 与 WinFS 谱系：https://en.wikipedia.org/wiki/Cairo_(operating_system) ；Sinofsky 回忆录章节：https://hardcoresoftware.learningbyshipping.com/p/020-innovation-versus-shipping-the
- Vista 2007 崩溃归因（NVIDIA ~30%）：https://www.reddit.com/r/programming/comments/6dobn/nvidia_drivers_responsible_for_nearly_30_percent （原始报道 Engadget/OSNews 2008-03）
- Windows RT：https://en.wikipedia.org/wiki/Windows_RT ；Windows on ARM 通史：http://www.emulators.com/docs/abc_history_of_woa.htm
- WSL 版本对比（官方）：https://learn.microsoft.com/en-us/windows/wsl/compare-versions ；https://en.wikipedia.org/wiki/Windows_Subsystem_for_Linux
- 反垄断事实认定书：https://www.justice.gov/atr/us-v-microsoft-courts-findings-fact
- IE 退役与 Edge：https://blogs.windows.com/windowsexperience/2021/05/19/the-future-of-internet-explorer-on-windows-10-is-in-microsoft-edge

（交叉引用：微内核 IPC 性能见 04 号报告；Zircon「务实派」对照见 05 号报告；OpenHarmony/QNX 侧写见 03/06 号报告。）
