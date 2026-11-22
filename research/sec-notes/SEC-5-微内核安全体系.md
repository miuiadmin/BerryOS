# SEC-5 微内核安全体系：业界现状调研（2026-10-29）

> 范围：seL4（形式化验证）、Fuchsia/Zircon（verified execution + job policy）、QNX（认证驱动）、Genode/NOVA（架构驱动）、Linux（单内核对照组）。纪律：每条硬事实给来源 URL（见 §6 编号）；【证】= 一手来源可查，【推】= 本报告推断。业界现状（§1–§4）与「对 BerryOS 的启示」（§5）严格分开。

## 1. 概述

微内核的安全叙事 = 「把驱动/FS/网络移出内核，TCB 收缩到调度 + IPC + 地址空间 + 能力」。但业界二十年的实践表明，移出内核只是换了攻击面，真正决定安全性的是四件事：

1. **内核自身凭什么可信**——seL4 的答案是全内核形式化验证（2004–2009，20 人年）；QNX 的答案是 Common Criteria EAL4+ 认证（文档 + 流程 + 测试）；Genode 明确拒绝全系统验证路线、押注架构与沙箱。
2. **信任如何从 ROM 传导到每个服务**——Fuchsia/Android 的答案是 hash-and-sign 验证启动链（AVB/vbmeta → Merkle 树 → 内容寻址包存储）+ 抗回滚。seL4 特意不做这件事：验证只覆盖内核本体，系统构建交给用户态。
3. **能力如何授予、收窄、吊销**——seL4 的 MDB 派生树 + untyped retype 吊销；Zircon 的 job 树级联杀 + 策略表；两者代表「逐能力吊销」与「逐进程子树治理」两个极端。
4. **时间也是攻击面**——seL4 MCS（mixed-criticality）把 CPU 时间本身做成能力（scheduling context），补「功耗/时序隔离」这个经典验证盲区。

信任边界的关键事实：微内核把**机制**留在内核（可验证、可认证），把**策略**推到用户态（root task、驱动管理器、组件框架）——seL4 甚至把「初始无条件授予全部能力给 root task」本身列为系统安全的最大假设之一，需再用 ~100 万行证明去验证一个静态系统配置 [17]。

两条贯穿全文的分界线，后文反复引用：
- **「内核可信」的三种来源**：证明（seL4）、认证流程（QNX）、架构纪律（Genode）——互不排斥但投入产出完全不同（§3.1/§3.5/§4）。
- **「信任传导」与「能力治理」是两件独立的事**：验证启动管「什么代码允许跑」（ROM → 内核 → 服务全链），能力/策略管「跑起来之后能碰什么」（授予/收窄/吊销/级联杀）——业界没有任何一家同时把两件都做到顶，seL4 只做后者（且机制层），Fuchsia 侧重前者【推-综合 §2/§3】。

## 2. 各家现状对照表

| 系统 | 内核 TCB 策略 | 形式化验证 | 验证启动 | 能力执行/吊销 | 认证/评估 |
|---|---|---|---|---|---|
| seL4 | ~8.7k 行 C + 600 行 asm，机制最小化 | 全内核功能正确性（Isabelle，~20 万行证明）[3]；完整性/机密性证明；32 位 ARM/RISC-V 到二进制级 [7][8]；AArch64 安全证明 2026-08 落地 [10] | 不做（内核外，交给用户态 verified loader 生态）[7] | CSpace + MDB 派生树；mint/badge 单向收窄；revoke 沿树吊销；untyped retype [15] | 未走 CC 路线；验证即卖点 |
| Fuchsia/Zircon | C++ 内核 + 组件框架，176 个系统调用 [32] | 无内核级证明；协议层有 fuzzing 基建 [30][31] | FVX 四阶段：ROM/OTP → 引导链验签 → AVB 验 ZBI → 已验系统验包存储，只跑预授权代码 + 抗回滚 [19] | handle 即能力；无逐能力 revoke syscall，靠 job 树级联 kill + 策略表治理 [20] | 无公开 CC 认证 |
| QNX Neutrino | 微内核 + 授权配置裁剪（Secure Kernel profile） | 无公开形式化验证 | 不在认证范围内 | 进程/通道能力（非 seL4 式细粒度） | EAL4+（CC）；POSIX PSE52；IEC 61508/ISO 26262 供安全侧 [24][25][26] |
| Genode/NOVA | 架构驱动：无全局命名空间、显式能力传递、递归沙箱 | 官方对「验证整个系统」持怀疑立场，未采用 [28] | 外包给各 base 平台（NOVA/硬件）[29] | 能力 = 框架一等公民，父子会话显式授权 [28] | 无 |
| Linux（对照组） | 单内核，~40M 行量级、驱动占大头 [33] | 无（局部符号执行/模糊测试基建） | 有（UEFI Secure Boot/dm-verity/Lockdown， retrofit） | 无能力系统（uid/capabilities 位掩码， retrofit） | 各发行版单独认证（如 SUSE EAL4 [27]） |

## 3. 机制细节

### 3.1 seL4 验证栈与成本（可直接对比的硬数字）

证明层次：abstract spec（Isabelle）→ executable spec（Haskell）→ C 实现 refinement；总证明 ~20 万行 Isabelle（摘要层 ~7.5 万 LOP + 执行层 ~8 万 LOP + C 实现层 ~5.5 万 LOP，另含 Haskell→Isabelle 翻译 ~1.3 万行、C 解析骨架 ~1.5 万行）[3]。内核本体 8,700 行 C + 600 行 asm [3]。

人力成本（SOSP'09 官方数据）[3]：

| 项 | 规模/成本 |
|---|---|
| 摘要规范 | 4 人月（~4,900 行） |
| Haskell 可执行规范 | ~2 人年（~5,700 行） |
| C 实现 + 集成 | 2 人月（8,700 行 C + 600 行 asm） |
| 证明总计 | ~20 人年（9 人年框架/工具 + 11 人年 seL4 专属） |
| 「重来一次」估计 | 6 人年（kernel+proof 合计 8 人年 vs SLOCCount 纯代码 4 人年） |
| 认证换算对照 | CC EAL6 经验规则 $10k/LOC × 8.7k 行 ≈ $87M——团队以此论证验证比高保证级认证便宜一个量级 |

- 验证收益：常规测试共发现 16 缺陷后，证明过程又抓出 144 个【证】[3]。

**增量改动成本**（§5.3，任务点名项）[3]：
- 局部底层优化（fastpath 手写汇编级）：重证明 < 1 人周；
- 跨切面特性（如新增中断通路/ARM 页表属性）：**重验证 1.5–2 人年 ≈ 原验证投入的 32%**，且当时修改了 ~12% 证明、新增 37%——「验证成本前置，但改动税永久 ~30%」【证-数字】【推-概括】。
- 验证收益：常规测试共发现 16 缺陷后，证明过程又抓出 144 个【证】[3]。

**验证范围与排除项**（决定「verified」标签的成色）[1][2][7]：
- 排除：时序侧信道（明示）、boot 代码、机器接口/asm、DMA（假设关闭或可信）、**SMMU/IOMMU（证明缺口，长期在补）**、MMU 假设完全掌控；debug/profile/print 路径不验证。
- 二进制级：32 位 ARM/RISC-V 经 translation validation 覆盖到机器码（不信任编译器）；**AArch64/x86_64 长期只到 C 级**；Proofcraft 2023 起逐步推进 64 位二进制验证，2026-08 AArch64 完整性/机密性证明落地 [8][10]；2025 证明架构拆分把证明行数砍 57%（增量成本在下降，但量级未变）[9]。
- **验证按配置成立**：官方 verified-configurations 页逐架构列出被验证的精确配置组合；改一个配置项即出包络。FAQ 给出判别法：预处理出 `kernel_all.c_pp` 与被验版本 diff，无 diff 即仍被覆盖【证】[6][7]。多核（SMP）验证长期搁置 [11]。

### 3.2 seL4 能力执行与初始授能

- 能力模型：CSpace/CNode 存 handle；**MDB（capability derivation tree）记录每次 mint/copy 的派生关系**——revoke 沿树递归吊销全部后代并把内存收回 untyped；mint+badge 是单向收窄（badge 不可逆拆）；cancelBadgedSends 清 badge 匹配的挂起消息【证】[15]。
- **初始无条件授能**（任务点名项）：内核只创建唯一 root task，经 seL4_BootInfo 把**全部物理内存的 untyped、CSpace/VSpace 根、IRQ control** 无条件交给它【证】[16]。系统安全性 = root task 的正确性——为验证一个静态系统配置（capDL 初始器）另花 ~100 万行证明 [17]。这是「微内核验证了内核、把最大信任转移推给第一支用户态程序」的结构性事实【推-概括】。
- MCS（mixed-criticality）：scheduling context = CPU 时间的**能力化**（无 SC 不能跑）；reply object 把「应答权」做成能力；notification 替代异步端点；目标成为默认配置【证】[12][13]。RISC-V 上 MCS 功能正确性 2026 年完成 [10]。价值：把「时间隔离」从调度器善意变成可验证的机制面【推】。

MCS 四对象速查（照着留接口位用）[13][14]：
- **SchedContext**：预算（budget）+ 周期（period）+ 优先级绑定；可跨线程**转让/借出**——CPU 时间成了可传递、可吊销的能力，而非调度器内部变量；
- **SchedControl**：创建/调整 SC 的元能力（通常只给系统管理服务）；
- **Reply**：一次 IPC 调用的「应答权」句柄——服务端持 reply 才能应答，应答一次即耗尽（防伪造应答/应答劫持）；
- **Notification**：字位组信号对象（word-sized badge 掩码），替代旧异步端点，可绑定线程做事件注入。
教训：v1 的 seL4 经典配置里「时间不是能力」，MCS 用了十年才补上——新内核若不在第一版把「时间/应答权」对象化，日后补要动 ABI【推】。

### 3.3 Fuchsia verified execution（FVX）与 AVB 数据结构

FVX = hash-and-sign 模型，四阶段链 [19]：

- 锚点 = 设备 ROM/OTP 里的公钥哈希（硬件信任根，软件不可改写）；被验对象 = 下一阶段镜像的哈希（hash-and-sign 而非逐块加密——启动链只关心完整性，机密性由上层卷加密另行负责）【推-模型解读；阶段事实见 [19]】。
- Phase 0：设备 ROM/OTP 锚点验证首段引导；
- Phase 1：已验引导段验证后续引导（含 AVB 对 ZBI 的验证）；
- Phase 2：已验 ZBI 验证包系统（BlobFS Merkle 根 = 内容寻址存储）；
- Phase 3：只有**预授权（preauthorized）代码**允许执行；base 包作为系统镜像一部分直接验证，universe 包逐次装载验证；**抗回滚：拒绝过时版本**【证】。

AVB 2.0 vbmeta 结构（BerryBoot 若消费 vbmeta 可照抄的槽位）[21][22][23]：
- `hash` 描述符：整镜像单哈希（boot/dtb）——字段：partition_name、salt、digest；
- `hashtree` 描述符：dm-verity 树（system/vendor）——字段：root digest、salt、tree offset/size、data offset/size、hash algorithm；
- `chained partition` 描述符：把某分区的验证委托给另一公钥（密钥分层）；
- **rollback index 数组**：每 slot 一个单调计数器，抗回滚；
- 设备锁定态：locked（完整执行）/unlocked（黄/橙/红状态机，解锁即擦用户数据）。
- 包存储配合：BlobFS 以 Merkle 根为内容地址（blobs 不可变、按内容寻址、重复内容去重）；包的 `meta.far` 元数据自身的哈希即包标识——base 包作为启动镜像一部分验证，universe 包装载时逐个验签【证】[19][21]。
- 生态位：avbtool 造结构、U-Boot 有 AVB2 支持——**采用兼容格式 = 免费获得工具链与引导生态**【证-机制】【推-生态结论】。

### 3.4 Zircon job policy（进程子树级最小特权）

- job 树根 = root job（交给 userboot）；policy 只能在**空 job**上设置，向下传播给新建子进程/job；合并规则 = **父胜**（effective policy = 父 effective + 自身设置，冲突取更严）【证】[20]。
- 策略面：十余个 PolicyCondition × PolicyAction（allow / allow-with-exception / kill），代表性条件【证-语义见 jobs 文档】[20]：
  - `ZX_POL_VMAR_WX`——禁止同页可写+可执行映射（W^X 执法）；
  - `ZX_POL_AMBIENT_MARK_VMO_EXEC`——禁止无 exec 来源的对象自我可执行化（拦 JIT/代码注入）；
  - `ZX_POL_NEW_VMO` / `ZX_POL_NEW_ANYIO` / `ZX_POL_NEW_PROCESS`——按对象类型逐类禁止创建（最小特权白名单化）；
  - kill 动作 = 违规进程直接被内核处决，exception 动作 = 上抛给 job 层异常处理器裁决。
- `zx_job_set_critical(proc, retcode)`：把进程标记为 job 关键——父 job 死则该进程以 `ZX_TASK_RETCODE_CRITICAL_PROCESS_KILL` 陪葬（进程监督的内核捷径）【证】[20]。
- **吊销语义与 seL4 相反**：无逐能力 revoke syscall，能力随进程死亡、随 job kill 级联消亡【证-「无该 syscall」由 API 文档面可得】【推-对比定性】。

### 3.5 QNX：认证作为产品

- QNX Neutrino Secure Kernel 通过 CC **EAL4+**；QNX 是最早追求 CC 评估的 RTOS 厂商之一；同时持有 POSIX PSE52（1003.13-2003）认证与 IEC 61508/ISO 26262 安全认证（安全侧与安全认证是两条线）【证】[24][25][34]。
- EAL4+ 通过的原因（评估保证级本质）：完备的开发文档与配置管理、系统性测试证据、生命周期控制 + **裁剪的攻击面**（Secure Kernel profile 收窄功能面）——是「流程 + 证据 + 小内核」三件套，不是数学证明【证-EAL 定义于 CC 门户】【推-归因】[26]。
- EAL4+ 实际要求什么（对照表，供估算用）【证-定义见 CC 门户，粒度为概括】[26]：开发全周期文档（设计到测试可追溯）、配置管理/交付程序、漏洞分析（对抗性攻击面论证文档化）、独立实验室评估；EAL5+ 加半形式化设计与更深度脆弱性分析；EAL6/7 要求半形式化验证级对应的设计证明——OS 全内核长期无人做到（seL4 之外）。
- QNX 认证矩阵的构成：安全侧（CC EAL4+）与功能安全侧（IEC 61508 SIL 3、ISO 26262 ASIL D、IEC 62304 医疗）是**两条独立证据链**，共用「小内核 + 可裁剪配置 + 文档流程」底座【证】[24][25]。
- 市场价值判断：EAL4+ 在政府/国防/医疗/轨交采购中是**准入门槛**（无证出局），但不是技术保证；主流 Linux 也能拿到 EAL4（SUSE SLES15 SP4 案例），说明该级别区分的是「流程证据是否齐备」而非「系统是否更难攻破」【证-SUSE 案例】【推-市场价值定性】[27]。EAL5+ 起要求半形式化设计与脆弱性分析，OS 级长期无人问津【推】。

### 3.6 攻击面治理：fuzzing 与真实漏洞样本

- Zircon 有 **176 个系统调用**（`ZX_SYS_COUNT=176`）——PT 2022 年直接批评「并不追求最小性：远超典型微内核」【证】[32]。
- **CVE-2022-0882**（任务点名的服务提权/能力检查绕过类）：`sys_debuglog_create` 的资源能力检查写成 `if (rsrc != ZX_HANDLE_INVALID)`——传**非法句柄值 0** 反而跳过校验，拿到内核日志读取权（信息泄露）。能力系统的安全性取决于**能力检查代码本身**，而非纸面上的能力模型【证-漏洞机制】【推-教训定性】[32]。
- 同研究还发现：Zircon 的 KASLR 当时形同虚设（每次 boot 内核地址相同）、C++ 虚表给内核利用留下控制流劫持面、FIFO 对象可做内核堆喷射原语【证】[32]。
- fuzzing 基建现实：syzkaller 曾支持 Zircon 但 2020 年后断线，RFC-0078（内核 sanitizer coverage）即为修复通道；Fuchsia 另在协议层（FIDL + libFuzzer）做结构化 fuzzing【证】[30][31]。**运维教训：fuzz 基建会烂——CI 化是唯一活路**【推】[32]。

### 3.7 服务冒名与驱动爆炸半径（任务点名项）

**服务冒名（fake service names）**：「未找到一手来源」——本次调研未找到针对微内核 name service 冒名攻击的系统研究或公开 CVE。机制层推断【推】：
- 能力系统中「绑定一个服务」= 「拿到该服务的 channel/handle」——安全锚点从**名字**（可抢注）移到**能力授予路径**（谁把 handle 给你）。若存在全局名字服务且授予路径不受限，名字服务本身就成为信任枢纽：抢注者 = 冒名者。
- Fuchsia 的工程答案是组件框架 capability routing：每个组件能拿到什么服务由其清单静态声明、由父级显式路由，无全局名字广播——把「冒名」压缩成「父级配错路由」的配置错误【推-基于 Fuchsia 组件框架公开设计通识；本次未单独取一手页，其 verified execution 页 [19] 的 preauthorized-code 原则同源】。
- 对内核的接口含义：命名/授予路径若在用户态（如 svcmgr），则它需要**可被测量、可被审计的授予记录**——「谁在什么时刻拿到了哪类服务的 handle」应当是一等遥测事件【推】。

**驱动服务漏洞爆炸半径**：业界共识做法 = 驱动全部用户态 + 每驱动独立地址空间 + 能力收窄（驱动只能碰被授予的通道/内存），把「驱动被 exploited」降级为「一个服务崩」；真正的残留半径是 **DMA**——用户态驱动若可对物理内存编程 DMA，地址空间隔离形同虚设，必须 SMMU/IOMMU 把设备 DMA 困在被授权的内存区（seL4 证明至今未覆盖 IOMMU，恰说明这是最难啃也最必须啃的面）【推-综合 [1][2][11][32]】。QNX 侧同型：驱动各自进程 + 授权内核配置裁剪（Secure Kernel profile 的减面逻辑）【证-机制在认证文档面】【推-对比】[24]。

### 3.8 接口位速查（业界机制 → 可预留的 ABI/数据结构槽位）

| 业界机制 | 载体 | 可照抄预留的槽位 |
|---|---|---|
| AVB vbmeta [21] | 签名头结构 | image 头部保留：hash/hashtree/chained 三类描述符区 + rollback index 数组 + 锁定态标志 |
| FVX 四阶段 [19] | 引导链协议 | BerryBoot 输出「下一阶段哈希」固定字段（哪怕 v1 只填 0 不校验） |
| seL4 BootInfo [16] | 初始授能表 | boot 期 handle 直插面收敛为带版本域的显式授予表 |
| Zircon job policy [20] | spawn 参数 | `SpawnSpec` 预留 policy 位图（W^X/exec/对象创建白名单 + 违规动作 kill/deny） |
| Zircon set_critical [20] | 进程标记 | spawn 面预留 critical 标志位（父死陪葬语义） |
| MCS SchedContext [13] | 线程对象 | thread/process 预留 budget/period 字段位（恒 0 = 不启用） |
| MCS reply object [13] | IPC 会话 | 通道会话预留「应答权」槽（v1 会话模型先不消费） |
| PCR 类测量槽（TPM 通行做法） | 测量链 | 测量槽位编号表：kernel / initrd / rootfs / 每个 .bap manifest，现在定死分配 |

## 4. 演进史与教训

1. **从「渗透测试」到「证明」**：L4 家族 1998–99 年靠学生渗透测试找洞（找得到 = 假设还有更多），2004 年 NICTA 启动 seL4 项目直接走全验证——「审计到头就只剩证明」是路线分叉的动因【证-L4 讲义记此沿革】[35]。里程碑年表：2009 功能正确性 [3] → 2011 完整性 → 2013 机密性 → 2013 二进制级（32 位 ARM）→ 2016– MCS 研究线（9.0.0-mcs 2019 转 主线可测 [12]）→ 多核验证尝试后搁置 [11] → 2023–26 Proofcraft 工业化续推（证明拆分 -57% [9]、AArch64 安全证明 2026-08 [10]）[4]。
2. **「verified」永远是有范围限定词的**：时序信道明示排除、DMA/SMMU 假设可信或缺口、boot 代码不覆盖、按配置成立、64 位二进制级姗姗来迟——seL4 官方页对排除项的坦诚本身是行业最佳实践【证】[1][2][7]。宣称「经验证因此安全」都是对范围的面纱【推】。
3. **能力系统的真正漏洞在检查代码**：CVE-2022-0882 的句柄校验绕过、Zircon 的 176 syscall 面、fuzzer 断线——模型再干净，执行面的一行 `!= INVALID` 就能掀翻【证-案例】【推-结论】[32]。
4. **验证启动是微内核自己不做的**：seL4 把 boot/系统构建留在验证包络外，Fuchsia 用整套 FVX 工业化补齐——两个生态按各自用户（高保证嵌入式 vs 消费电子）各取一半【证-两侧事实】【推-归因】[7][19]。
5. **认证是流程生意**：QNX 靠 EAL4+ + 安全认证矩阵卖了二十年「assurance」；SUSE 证明 EAL4 与内核形态无关、与文档/流程投入有关【证】[24][27]。
6. **Genode 的反面一票**：16.05 发布注记明确表达对「验证完整系统」可行性的怀疑——Genode 认为沙箱架构 + 小 TCB + 二十年一致性比追证明更划算。这不是否认 seL4，而是指出证明的适用边界（内核机制 vs 活的系统）【证-立场出自其发布注记】【推-解读】[28]。
7. **对照组（一句）**：Linux 单内核源码树 6.14（2025）前后突破 4000 万行、7.x 仍在涨（驱动为大头）——攻击面随代码量线性扩张，是微内核路线论证的整个反面【证-量级多源一致（Phoronix 等长期跟踪；具体里程碑文 URL 本次未锚定）】【推-攻击面解读】[33]。
8. **演进的时间形状（横切观察）**：各家都不是 day-one 完整体——seL4 用了 20 年从功能正确性走到 AArch64 安全证明 [4][10]；Fuchsia 的验证链从引导一路长到包系统与抗回滚 [19]；Zircon 从「能力内核」长出 job policy/set_critical 这些「治理层」[20]。共同规律：**机制面（能力、验证、签名结构）先立，治理与运维面（策略表、fuzz、吊销）后被真实攻击与采购压力逼出来**——晚补的都能补，但补的时候不改早期 ABI 才补得动【推-综合】。

## 5. 对 BerryOS 的启示（建议，非现状）

以下为对照 §3/§4 的落点建议；tag 对应 docs/路线图-v1.0.md 的 v2/v3/v4 与 day-one 留缝：

- **【day-one 留缝】句柄 0 = INVALID 立约 + 能力拒面遥测**：CVE-2022-0882 的直接教训——现在就在 ABI 文档钉死 handle 0 永为非法值，并把每次 `-EBADF/-EACCES` 作为结构化遥测事件落环（handle/rights/调用号）。成本近零，未来审计/入侵检测的地基。对应既有「AI 测试友好化」原则。
- **【day-one 留缝】授予记录一等化**（§3.7 冒名分析的落点）：svcmgr（未来的名字/服务管理面）每次把服务通道授予进程时落一条结构化授予事件（谁、何时、哪类服务、经谁路由）——配合上一条拒面遥测，构成「能力系统自己的审计日志」；Fuchsia 用静态清单路由解决的问题，我们用「可测量 + 可回放」覆盖到动态授予面【推】。
- **【day-one 留缝】初始授能表显式化**：内核给 svcmgr 的 boot 期 handle 直插面（现散在 main.rs 定值断言）收敛为一张带版本域的显式授予表（seL4 BootInfo 模式）——未来测量链/远程证明可对「谁出生时拿到什么」直接哈希。
- **【day-one 留缝】syscall fuzz 面 = abi.toml 可再生产物**：从 ABI 单源生成 syzkaller 描述的通道先立项（哪怕只生成骨架）——Zircon fuzzer 烂掉的教训是 fuzz 基建必须 CI 化、且与 ABI 同源演进。
- **【v2】job/policy 表进 svcmgr spawn 面**：照 Zircon 模式给进程子树配策略（禁 WX 映射、禁自我可执行化、违规 kill/告警）+ critical 标记（父死陪葬）——在纯能力检查之外补「子树级最小特权」层，接口位现在就能在 `SpawnSpec` 预留 policy 字段。
- **【v2】SMMU = 不可拖的顶线**：seL4 证明至今把 IOMMU 留作缺口，业界没有现成答案可抄——我们的用户态 virtio 驱动（vblk/vnet）在 SMMU 落地前就是事实 TCB，v2 的 per-device domain 设计（docs/安全与硬件隔离设计-v1.0.md）正踩在业界前沿上，须按「自研负担」预留人力而不是等参考实现。
- **【v3】verified boot 直接采 AVB 兼容 vbmeta**：测量链升级为验证链时，头结构照抄 §3.3 四描述符 + rollback index 数组 + 锁定态机——BerryBoot 消费 vbmeta = 免费拿 avbtool/U-Boot 生态；测量槽位编号（kernel/initrd/rootfs/每个 .bap manifest）现在就定进文档。
- **【v4】抗回滚与密钥分层**：rollback index 的存储介质（RPMB/安全元件）与 chained partition 式密钥委托（授权分区用独立密钥）一并设计。
- **【day-one 留缝】「时间/应答权」对象化预埋**：MCS 的教训（§3.2）——第一版不把 CPU 预算与「应答权」做成显式对象，日后补要动 ABI。现 `intent` u8 已是调度输入；在 thread/process 对象里预留 budget/period 字段位（可以恒 0 不启用），IPC 会话里预留 reply 右位，让 v3 想上 MCS 式隔离时无需 ABI 断裂。
- **【路线裁定】不追全内核形式化验证**：20 人年 + 跨切面改动 32% 回归税，与既有 research/04 结论一致。可选项：对 handle 表/MDB 类纯函数面做轻量模型检测（Rust + kani/SMT）点缀；把 ADR/测试可追溯性当认证证据持续积累（QNX 教训：认证贵在补文档，流程证据 day-one 攒最便宜）。若 v2 攻工业 RT（ADR #39），IEC 61508 类功能安全证据链与安全证据链可共用同一套 ADR/测试台账——这套台账本仓库已经在攒。

## 6. 来源清单

1. https://sel4.systems/Verification/proofs.html — seL4 证明总览（层次/扩展/排除项）
2. https://sel4.systems/Verification/assumptions.html — 验证假设清单
3. https://www.sigops.org/s/conferences/sosp/2009/papers/klein-sosp09.pdf — SOSP'09 seL4 论文（成本表/144 缺陷/改动成本，PDF 直读）
4. https://trustworthy.systems/projects/OLD/seL4-verification/ — 验证项目页与时间线
5. https://github.com/seL4/l4v — l4v 证明仓库
6. https://docs.sel4.systems/projects/sel4/verified-configurations.html — 被验证配置清单
7. https://sel4.systems/About/FAQ.html — FAQ（二进制级/编译器信任/IOMMU/config diff 判别）
8. https://proofcraft.systems/news-2023.html — Proofcraft 2023（64 位二进制验证进展）
9. https://proofcraft.systems/news-2025.html — Proofcraft 2025（证明架构拆分 -57%）
10. https://proofcraft.systems/news-2026.html — Proofcraft 2026（AArch64 安全证明、MCS RISC-V 功能正确性）
11. https://trustworthy.systems/projects/OLD/multicore-sel4/ — 多核验证状态（搁置）
12. https://docs.sel4.systems/releases/9.0.0-mcs.html — 9.0.0-mcs 发布
13. https://docs.sel4.systems/projects/sel4-tutorials/mcs.html — MCS 教程（sched context/reply）
14. https://docs.sel4.systems/projects/sel4/api-doc.html — seL4 API 文档
15. https://docs.sel4.systems/projects/sel4-tutorials/part-2 — capabilities/untyped/revoke 教程
16. https://docs.sel4.systems/projects/rust/root-task/kernel-objects — root task/BootInfo/初始授能
17. https://sel4.org/Summit/2023/slides/day1-03-frost.pdf — Summit'23（capDL 初始化器 ~1M LOP）
18. https://trustworthy.systems/projects/RTA/ — 实时/MCS 相关项目
19. https://fuchsia.dev/fuchsia-src/concepts/security/verified_execution — Fuchsia verified execution（FVX 四阶段）
20. https://fuchsia.dev/fuchsia-src/concepts/process/jobs — Zircon jobs/policy/set_critical
21. https://android.googlesource.com/platform/external/avb/+/refs/heads/main/README.md — AVB 2.0 README（描述符/rollback/锁定态）
22. https://source.android.com/docs/security/features/verifiedboot — Android verified boot
23. https://docs.u-boot.org/en/latest/usage/avb2.html — U-Boot AVB2 支持
24. https://www.qnx.com/why-qnx/standards/ — QNX 标准与认证（EAL4+/PSE52/61508/26262）
25. https://qnx.software/ — QNX 官网（safety 定位）
26. https://www.commoncriteriaportal.org/ — Common Criteria 门户（EAL 定义）
27. https://www.suse.com/c/suse-linux-enterprise-server-15-sp4-achieves-common-criteria-eal4-certification/ — SUSE SLES15 SP4 EAL4 案例
28. https://genode.org/documentation/release-notes/16.05 — Genode 16.05（对形式化验证的官方立场）
29. https://genode.org/documentation/release-notes/13.02 — Genode 13.02（NOVA base 系统）
30. https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0078_kernel_sanitizer_coverage — Fuchsia RFC-0078 内核覆盖率
31. https://github.com/google/syzkaller — syzkaller
32. https://ptswarm.com/blog/a-kernel-hacker-meets-fuchsia-os/ — Positive Technologies Zircon 研究（CVE-2022-0882/176 syscall/KASLR）
33. https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/ — Linux 内核源码树（LOC 量级一手可核）
34. https://posix.opengroup.org/ — POSIX 认证（PSE52/1003.13）
35. https://esb.ics.hawaii.edu/l4/ — L4 讲义（L4 家族渗透测试沿革）

（注：seL4 证明层次中「~20 万行 Isabelle」为论文摘要口径；表 1 明细四舍五入求和为 ~21 万，差异在框架/自动生成证明的口径归属【证-两数均出自 [3]】。）
