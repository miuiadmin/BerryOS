# BSD 谱系进化史（1977–2026）：增强、诉讼、分叉与三条长跑路线

> 调研日期 2026-10-15 · 视角：进化在优化什么 / 踩了什么坑 / 被推翻的决策 · 与本系列交叉：能力模型见 04（seL4/L4）、Zircon 血缘见 05（Fuchsia 用户态大量吸收 FreeBSD 代码）、许可证对比见 11（Rust 系）
> 主要一手来源：McKusick《Twenty Years of Berkeley Unix》（O'Reilly Open Sources, 1999）、freebsd.org / netbsd.org / openbsd.org 官方文档、USL v. BSDi 判例资料、FreeBSD Foundation 论文（Dawidek 的 ZFS 回忆、治理演进）

## 0. 一页总览（TL;DR）

BSD 谱系是「**学术机构在别人地基上加楼层，最后被迫自建地基**」的最佳标本：CSRG 十八年在 AT&T 源码上做增量增强（虚拟内存、TCP/IP、FFS），增强到法律地雷引爆（USL 诉讼），两年诉讼真空把「免费类 Unix」生态位让给了 Linux——此后 BSD 分裂为三条路线：FreeBSD（性能/商业可用性长跑）、NetBSD（可移植性宗教）、OpenBSD（安全纯度 + 不做加法）。它给后来者的最大教训不是技术，而是：**许可证洁净度、发布纪律、接口收缩，都是与调度器同级别的工程约束**。

| 年代/版本 | 在优化什么 | 踩的坑 | 关键调整 |
|---|---|---|---|
| 1977–79：1/2/3BSD | VAX 内存利用（VM）、Pascal/vi 工具链 | 增强建立在 AT&T 32V 授权地基上 | DARPA 合同（1980-04）使 CSRG 成建制 |
| 1980–83：4BSD→4.2BSD | 网络研究基础设施 | 4.2BSD 拖期两年；BBN TCP 之争 | sockets+FFS+TCP/IP 一体交付；自研 TCP 在 bake-off 险胜 |
| 1986–90：4.3→Tahoe→Reno | 可移植性、VM 现代化 | 机型耦合（Tahoe）、NFS 授权风险 | 机器相关/无关代码分离；引入 Mach VM |
| 1989–91：Net/1→Net/2 | 摆脱 AT&T 授权依赖 | 只敢发「网络子集」，内核 6 文件拿不掉 | Bostic 网络重写运动；Net/2 缺 6 文件发布 → 催生 386BSD |
| 1992–94：USL v. BSDi | （被动）法律生存 | 两年生态真空；禁令威慑吓走商业用户 | 和解 + 4.4BSD-Lite（1994-06）认证洁净；三家换基重生 |
| 1993：FreeBSD/NetBSD 分叉 | 386BSD patchkit 失控 | 单人守门（Jolitz）成为瓶颈 | 社区接管：NetBSD 先行（可移植），FreeBSD 随后（x86 性能+安装体验） |
| 1995：OpenBSD 分叉 | 安全审计路线 | Theo de Raadt 被逐出 NetBSD core | 「安全纯度」成为独立项目的合法性来源 |
| 1998–2005：SMPng | 多核可扩展性 | Giant lock 锁死内核并发；5.0 拖期两年半 | 细粒度锁七年长跑；RELENG 发布纪律重建 |
| 2000s：工具链/存储赌注 | 摆脱 GPLv3 冻结（GCC 4.2.1） | 陈旧编译器拖累系统 | clang/LLVM 迁移（2009–2014）；ZFS 移植（2008）；soft updates/su+j |
| 2015–：收缩接口 | 攻击面最小化 | 依赖外部组件（OpenSSL、闭源固件） | pledge/unveil、LibreSSL、无线驱动逆向 |

## 1. CSRG 增强之路（1977–1995）：站在 AT&T 地基上的加楼工程

**优化什么**：CSRG（Computer Systems Research Group）每一阶段的核心矛盾都由资助方与硬件变迁决定，而非「做操作系统」本身。1BSD（1977，Pascal 系统 + ex）和 2BSD（1978，vi/termcap）是给 PDP-11 用户「修 bug 加工具」；3BSD（1979 末）给 32V（V7 的 VAX 移植）加上**虚拟内存支持**——Babaõglu 与 Bill Joy 的工作让 VAX 的内存不再是奢侈资源，kernel 镜像从此叫 `/vmunix`。1980 年 4 月 DARPA 合同落地后，CSRG 的任务变成「给 ARPANET 研究社区造基础设施」：4.2BSD（1983-08）交付 **TCP/IP sockets API + Berkeley Fast File System**，这是互联网史上最重要的一次「API 定锚」—— sockets 至今仍是所有主流操作系统的网络接口。

**坑**：其一，**BBN 之争**——DARPA 要求采用 BBN 开发的官方 TCP 实现，Berkeley 自己写了一套更快的；DARPA 只好出资做对比测试（McKusick 回忆中由 BRL 的 Mike Muuss 主持），Berkeley 实现险胜。这说明「资助方指定实现」与「实现者自负」的张力 day-one 就存在。其二，**工期失控**：4.2BSD 做了两年多，首次暴露「大版本=大拖延」的 BSD 遗传病。其三，也是最致命的：**所有增强都以 AT&T 授权源码为地基**，AT&T 在 1980 年代末大幅收紧授权价格（常被引为 Net/1、Net/2 自由化的直接动机），CSRG 的「学校许可证」红利随时可被收回。

**调整与代价**：4.3BSD-Tahoe（1988）做了机器相关/无关代码分离——这份「可移植性资产」后来成为 NetBSD 的直接血统；Net/1（1989-06，网络代码子集自由发布）和 Net/2（1991-06）是 Keith Bostic 发起的「净室化运动」：数百个用户态工具被志愿者重写。但内核仍有 **6 个文件**被认为受 AT&T 版权污染，Net/2 干脆缺着它们发布——这个「缺口」由 Bill Jolitz 在数月内补齐，即 386BSD 的雏形。代价：Net/2 名义自由、法律上悬而未决，为 1992 年诉讼埋下引信。

## 2. 诉讼真空期（1992–1994）：USL v. BSDi——生态位如何让给 Linux（本节重点）

这是「新 OS 诞生 = 填历史债」的最佳案例，值得逐帧复盘。

**时间线**：1991 年，BSDi（Berkeley Software Design, Inc.）由 Rick Adams 发起、CSRG 老班底（Karels/McKusick/Bostic 任董事）加盟，以 995 美元销售基于 Net/2 的 BSD/386，并打出 1-800-ITS-UNIX 的电话。1992 年 4 月，AT&T 旗下 USL 起诉 BSDi 侵犯版权与商业秘密，随后把加州大学（UC）追加为被告；UC 反诉 USL 未按授权条款给 BSD 代码署名。1992 年 12 月，联邦法官 Debevoise 就禁令动议举行听证；约六周后**禁令被拒、绝大部分诉请被驳回**（仅剩两项）。诉讼转战加州州法院后陷入拉锯。转折点是 1993 年 6 月 **Novell 从 AT&T 手中买下 USL**——Ray Noorda 明确表示宁愿在市场竞争也不想在法庭上耗。1994 年初和解（McKusick 记为 1 月，维基记为 2 月 4 日签署）：18000 个文件中仅 **3 个**需删除、约 70 个加上 USL 版权标注但**仍可自由再分发**；**4.4BSD-Lite（1994-06 发布）被认证为洁净基线**，USL 承诺不起诉任何以 Lite 为基的使组织；作为对价，BSDi/NetBSD/FreeBSD 必须**以 Lite 为基础重启代码库**（扔掉 Net/2 时代的存量）。

**真空期如何让位给 Linux**：1992–1994 恰是 PC 类 Unix 需求爆发窗口。386BSD（0.1 发布于 1992-07）本比同期 Linux 更完整，但被 Jolitz 夫妇的发布节奏卡死，又顶着「Net/2 血统」的法律疑云；BSD/386 被禁令威胁笼罩；**没有任何企业敢在此期间把产品押在 BSD 上**。而 Linux（1991-09 首发，1994-03 发布 1.0）从零写成、无任何 AT&T 血统，法律上完全干净。结果：开发者、用户、厂商在两年内完成了向 Linux 的迁移惯性——等 1994 年 FreeBSD 2.0（1994-11，基于 4.4BSD-Lite 重生）和 NetBSD 1.0（1994-10-26）洗白上岸时，窗口已经关闭。Linus 那句广为流传的话（出处常引为 1990 年代初访谈、《Rebel Code》一书）总结了反事实：「如果 386BSD 当时可用，Linux 大概永远不会发生。」更深一层：**Linux 赢的不是代码，是「无历史债」这个属性本身**——McKusick 在同一篇文章里酸楚地指出，Linux 发行版里约一半用户态工具直接取自 BSD 发行物：胜利者吃掉了失败者的应用层资产。

**换基的代价与意外收益**：FreeBSD/NetBSD 被迫扔掉 Net/2 存量、以 Lite 重启，短期是惨痛延迟；McKusick 却称之为 blessing in disguise——换基让他们免费同步了 CSRG 三年的最终成果（Mach VM、新网络栈等），否则要自己迁移。这是「债务清算」罕见的正收益案例：**前提是债主（USL/Novell）最终给了一条干净的出口**。

## 3. 三国分立（1993/1995）：386BSD 遗产的三次分叉

386BSD 的死因不是技术而是**治理**：Jolitz 夫妇单人守门，社区补丁汇成庞大的「patchkit」（需按序打数百个补丁），Jolitz 又撤销了对 patchkit 的背书、发布节奏停滞。两次接管先后发生：

- **NetBSD（1993 春，0.8）**：Chris Demetriou、Theo de Raadt、Adam Glass、Charles Hannum 创立，**优先级=支持尽可能多的平台**，研究向。血统直接继承 Tahoe 的机器无关层。
- **FreeBSD（1993 末，1.0）**：patchkit 协调者 Nate Williams、Rod Grimes、Jordan Hubbard 创立，**优先级=仅支持 PC 架构、跑得快、装得上**（精心安装脚本 + Walnut Creek CDROM 发行渠道），瞄准「更大、更不技术化的用户群」。McKusick 直言 FreeBSD 靠 Linux 兼容层搭上了 Linux 热度的便车。
- 1993-11 两派谈过合并，未果——分裂永久化。
- **OpenBSD（1995-10 分叉，1996-07 首发 1.2）**：Theo de Raadt 1994 年 12 月被 NetBSD core 要求辞职并撤销提交权限（人际冲突为主因，其沟通风格是直接诱因），十个月后带人分叉 NetBSD 1.0。戏剧性的是：这场「性格事故」反而催生了 BSD 谱系里路线最清晰、纪律最严的项目——**安全纯度**成了独立旗号。

三次分叉的共同教训：**分叉原因全部是治理/优先级冲突，没有一次是纯技术不可行**。社区项目死于「守门人节奏」甚于死于「代码质量」。

## 4. FreeBSD 长跑：Giant lock → SMPng → 5.x 危机 → clang/ZFS

**优化什么**：FreeBSD 的主线矛盾自 1990 年代末起就是「**商业可用的高性能 x86 服务器 OS**」。三个长跑工程都为此服务。

**SMPng（1998–2005+）**：FreeBSD 2.x–4.x 的 SMP 是「巨锁 SMP」——整个内核一把 Giant lock，多核只能跑一个核的内核代码。1998-200起 John Baldwin、Jeff Roberson 等推动 SMPng（SMP next generation），2000-06 正式立项（两天启动会议，Greg Lehey 留有案例研究）：目标是内核全可并发/可抢占。这是一场**七年长跑**：5.0（2003-01）首次带入细粒度锁，5.2 网络栈大部分完成，5.3（2004-11）才是第一个可生产的 5.x，6.0（2005）主体成熟。教训与 Linux 对照鲜明：Linux 拆 BKL 拆到 2011（2.6.39）才拆完——「巨锁拆除」是单体内核最难的长跑工程，没有捷径。

**5.x 分支策略失误与恢复**：5.0 原定 2001-11，先滑到 2002-11，实际 2003-01 才发布（官方公告自述「近三年工作」），且发布时官方文档自己承认「相对 4.x 可能在稳定性、性能甚至功能上有回退」。后果三连：用户大规模留在 4.x（4.11 拖到 2005-01 才收官）；2003 年夏 Matt Dillon 从 FreeBSD 4.8 分叉出 DragonFly BSD，公开主张 5.x 的 SMP/线程路线是错的；项目声誉受损。**恢复动作**：承认 5.0/5.1 只是「早期采用者」版本，把 5.3 树为首个 -STABLE；随后把 RELENG 分支纪律成文化（冻结期、HEAD/-STABLE 分离、缩短大版本周期），6.0 快速跟进以修复信心。这是教科书级的「大爆炸重构式发布 → 纪律化发布工程」转折。

**GCC→clang 赌注**：FreeBSD 曾深度绑定 GCC（多年贡献回归上游）。2007 年 GPLv3 发布后，FreeBSD 因许可证立场拒绝 GPLv3，被冻结在最后一个 GPLv2 版本 **GCC 4.2.1**（2007）上——陈旧编译器开始拖累整个系统。2009 年起押注当时远未成熟的 clang/LLVM（2010 年 ClangBSD 自举实验），2012 宣布 FreeBSD 10 起默认 clang，2014-01 FreeBSD 10.0 落地，GCC 4.2.1 于 FreeBSD 13 前后彻底移出 base（基金会 2020 年确认）。触发器是**许可证而非技术**——但赌注赢在技术（错误信息、编译速度、BSD 许可的供应商亲和力；Apple 的真金白银同源）。Dawidek 的 ZFS 回忆（FreeBSD Foundation PDF）展示了同一逻辑的另一面：Sun 的 CDDL 与 GPL 不兼容，反而让 FreeBSD 成为 ZFS 唯一的「原生级」归宿——Paweł Dawidek 2006 年夏 10 天即获惊人进展，2008 年随 FreeBSD 7.0 以可加载模块形态出货。

**存储栈演进**：UFS→FFS（4.2BSD 血统）→soft updates（McKusick/Ganger，元数据依赖排序代替日志，2000-06 集成、FreeBSD 5.0 可用，换来 background fsck）→journaled soft updates（FreeBSD 9，2011，Jeff Roberson，彻底免掉崩溃后 fsck）→并行引入 ZFS（2008）。值得注意的是 FreeBSD 没有推倒 UFS，而是**给它续了三次命**——与 Linux 「ext2→ext3→ext4 + 另起 btrfs」的双轨对照。

**Linuxulator 三段式（被推翻的决策样板）**：1995 年引入 Linux 二进制兼容层，动机部分就是「想玩 Doom、要用 Mathematica」——早期是纯粹实用主义拥抱；此后二十年长期低维护（社区更希望用户用原生 ports）；2021 年 FreeBSD Foundation 专项注资翻新（对齐现代 syscall/容器生态），到 2025–26 年被社区重新评价为「在现代工作流里保住 FreeBSD 可用性」的关键资产。结论：**生态引力无法对抗，只能选择拥抱的姿势**。

## 5. NetBSD：可移植性的宗教与资源摊薄

**优化什么**：单一目标——「在任何硬件上都能跑」（官方口号 "Of course it runs NetBSD"，正式支持的架构长期在 50+，官方页面现记 53 个）。副产品可观：pkgsrc（1997 年从 FreeBSD ports 演化，成为跨平台包管理的事实标准之一）、rump 内核（任何态运行内核组件）。

**坑**：其一，**资源摊薄**——53 个架构意味着每个架构分到的测试、维护人力都薄，Quora/社区长期争论「为什么 NetBSD 是最冷门 BSD」；其口号的当代讽刺是 Linux 内核本身支持的架构范围已碾压。其二，**治理危机**：2006 年，联合创始人 Charles Hannum 公开抨击基金会治理偏离初衷（只做行政/法律事务的本分），core team **集体辞职**，2006 年 Q2/Q3 连状态报告都发不出来，年底才重组新 core。NetBSD 没死，但证明了「无限横向优先级 + 稀释的人力」会周期性引爆。其三，两次人才出走（1995 Theo 分叉、2006 Hannum 决裂）都指向同一病根：守门文化与沟通问题。

**价值**：架构无关层的工程质量（Tahoe 血统）使 NetBSD 成为教学/嵌入/复古硬件的常青树——2025 年的 NetBSD 10/11 仍在新平台（如 Apple Silicon）上落地。

## 6. OpenBSD：小团队如何靠「不做加法」活得久

**优化什么**：不是功能，是**攻击面的单调递减**。OpenBSD 把「安全」从口号做成三层工程：①代码审计（早年苦功）；②exploit 缓解叠加——W^X（2003，OpenBSD 3.3）、栈保护、ASLR、malloc 随机化（Thierry Deval 首创，Otto Moerbeek 2008 年在 OpenBSD 4.4 整体重写，比旧分配器还快约 30%）；③**接口收缩**——pledge(2)（OpenBSD 5.9，2016）让进程声明「从此只许用这些 syscall」，unveil(2)（OpenBSD 6.4，2018）让文件系统视野单向收窄。2023 年 EuroBSDCon 论文统计：base 系统 ~86% 进程已 pledge、~39% 已 unveil。pledge/unveil 后来被多方移植（含 Justine Tunney 的 Linux 移植），证明「**默认安全=减法设计**」这条路线的可迁移性。

**坑与调整**：外部依赖屡次背叛——Tatu Ylönen 的 SSH 转私有催生 OpenSSH（1999，fork 自 Björn Grönvall 的 OSSH/ssh-1.2.12，OpenBSD 需要一个不受瑞典出口法与商业许可污染的实现）；IPFilter 的许可证禁改催生 pf（2001，Daniel Hartmeier 等数周写成，OpenBSD 3.0 引入）；Heartbleed 催生 LibreSSL（2014）。无线网卡的闭源固件则招来 2005–2007 年的**驱动逆向运动**（Reyk Floeter 等逆向 ral/ath 等，2005 年 3.7 起陆续进树），Theo 2007 年公开信炮轰芯片厂。政治风险也真实存在：2003 年 Theo 因反战言论失去 DARPA 的 POSSE 资助——项目学会靠小额捐赠活。

**纪律**：六个月一个版本、25 年不辍（每年 5 月/10 月如钟表）。这本质上是把 BerryOS 式「关卡验证」制度化为组织生命节律——**小步、可验证、从不欠发布债**。OpenBSD 团队规模始终不大，靠「不做加法」（不做桌面、不做云、不做规模）换来了三十年路线不漂移。

## 7. 「被推翻的决策」清单

| # | 决策 | 当初理由 | 推翻触发器 | 教训 |
|---|---|---|---|---|
| 1 | Net/2 缺 6 文件发布（1991） | 拿不干净的先不发，社区（Jolitz）自己补 | 诉讼和解 + 4.4BSD-Lite 全面洁净化（1994） | 「缺口的临时方案」会被当正式产品用（386BSD），债务要主动清算而非绕开 |
| 2 | r-commands（rlogin/rsh）作为「临时」方案进入 4.2BSD | DARPA 网络环境互信假设 | 互联网开放化后 .rhosts 成三十年安全隐患 | 「临时」接口的寿命以几十年计；安全假设是接口设计的一部分 |
| 3 | FreeBSD 绑定 GCC 多年 | 成熟、免费、社区熟 | GPLv3（2007）把 FreeBSD 冻结在 GCC 4.2.1 | 基础工具链的许可证变化=平台级战略风险；迁移要提前一代启动 |
| 4 | FreeBSD 5.0「大爆炸式」重构发布 | SMPng/GEOM/KSE 改动太深，无法增量发布 | 用户留 4.x、DragonFly 分叉（2003）、5.0 自认性能回退 | 重构长跑必须与「主干随时可发布」并行；否则分叉与信任流失 |
| 5 | Linuxulator 引入（1995）后长期冷维护 | 只是玩 Doom/Mathematica 的权宜；应推原生 ports | Linux 生态引力失控，2021 Foundation 专项翻新 | 兼容层不是态度问题是生存问题；要么认真维护要么明确弃养 |
| 6 | 386BSD 单人守门 patchkit | 作者权威 + 质量控制 | patchkit 失控 + 背书撤销 → 1993 双分叉 | 守门节奏即项目生命线；治理要能承接社区动能 |
| 7 | OpenBSD 沿用 FreeBSD 的 phkmalloc | 血统遗产，够用 | 攻击面需求（随机化）与性能 → 2008 年整体重写 | 继承来的组件按「自己的威胁模型」重新估值 |
| 8 | OpenBSD 采用 IPFilter（外部许可） | 功能成熟 | 许可证禁改条款（2001）→ 数周内自研 pf | 关键路径上的外部依赖必须过「许可证+治理」双审查 |
| 9 | CSRG 解散（1995）以为使命终结 | 和解达成、Lite 洁净、资金到期 | FreeBSD/NetBSD/OpenBSD 三线全部活到今天，BSD 代码无处不在（macOS/Darwin、PS4/5、Fuchsia 用户态、OpenSSH） | 机构的「关门」不等于基因的消亡——代码与 API 的寿命远长于组织 |

## 8. 关键人物与组织因素

- **CSRG 班底**：Bill Joy（3BSD VM/4.2BSD 总设计，后创 Sun）、Kirk McKusick（FFS/soft updates，历史书写者）、Keith Bostic（Net/2 净化运动发起人）、Mike Karels。学术机构 + DARPA 资金 = 「资助方需求驱动功能」的原始形态。
- **政治塑造技术**：AT&T 的授权涨价催生 Net/1、Net/2；USL 诉讼的真正终结者不是法院而是 **Novell 收购**（商业决策）；DARPA 2003 年对 OpenBSD 的撤资直接固化其「零政府资金」路线。
- **许可证是第二宪法**：GPLv3 逼走 GCC；CDDL 反而送来 ZFS/DTrace 独家归宿；BSD 宽容许可让 FreeBSD 代码进入 Apple Darwin（Jordan Hubbard 2001 年被 Apple 招募即此链条）——**每一代 BSD 的技术路线图上都叠着一张许可证地图**。
- **商业载体变迁**：Walnut Creek CDROM（发行+基础设施）→ 2000-03 与 BSDi 合并 → 后归 iXsystems；2000-03 Justin Gibbs 创立 FreeBSD Foundation（501(c)(3)）——商业依托崩解后，独立基金会成为项目存续的关键调整。
- **性格即路线**：Theo de Raadt 的对抗性既造成了 1995 分叉的代价，也成就了 OpenBSD 三十年的路线纯度——治理的人格因素在 BSD 史上的权重远超一般软件史。

## 9. 对 BerryOS 的启示

1. **把「法律/许可证关卡」纳入测试策略**。Net/2 六文件与两年诉讼真空证明：源码洁净度审计与调度器正确性同级别——一次疏漏可断送两年窗口期。BerryOS 的「协议洁净」（GPL 不进内核、参考只看不抄）应固化为 G 关卡式的显式检查（来源清点、许可证扫描进 CI），而不是口头原则。
2. **长跑改造不能以「主干不可发布」为代价**。SMPng 用七年拆巨锁，但 5.0 的「大爆炸发布」险些杀死项目（用户滞留 4.x、DragonFly 分叉）。BerryOS 的 Phase 2b（高半翻转→每进程页表）正是同类「地基级改造」：每一步保持 QEMU 可验证、可发布（G1 式过关），比一次到位重要得多——这正是 BerryOS 已采用而 FreeBSD 当年违反的纪律。
3. **接口减法是安全与 AI 护栏的共同语法**。pledge/unveil 的价值在于「声明式、单调收紧、默认安全」——BerryOS 的 AI 调度护栏（慢/快循环切分、RT 屏蔽、能力收窄-only 复制）应照此设计：让 AI 策略服务只能**收窄**权限与资源面，内核机制侧不提供任何「放宽」路径。OpenBSD 还证明 ~26 个 v1 syscall 的窄面（BerryOS 目标）足够养活一个严肃系统。
4. **兼容层要早表态**。Linuxulator 三段式（实用主义拥抱→冷维护→被迫翻新）说明生态引力不可对抗。BerryOS v1 无 fork、POSIX 子集是自觉收缩，正确；但应在 ABI 设计上预留「外来二进制兼容层」的位置（哪怕是 syscall 号空间与 C-ABI 边界的干净分区），避免未来被迫做侵入式改造。
5. **聚焦即生存**。NetBSD 的 53 架构摊薄 vs OpenBSD 的「不做加法」三十年长青，是同一枚硬币的两面。BerryOS「单核+抢占点起步、机制在内核策略可插拔」的聚焦路线，应警惕过早铺平台（x86_64/RISC-V 移植位留接口即可）与过早铺生态——先把一条 ARM64 路径走到 G2/G3。

## 10. 参考资料

- McKusick, *Twenty Years of Berkeley Unix: From AT&T-Owned to Freely Redistributable*（O'Reilly Open Sources, 1999）——CSRG 与诉讼的一手回忆：https://www.oreilly.com/openbook/opensources/book/kirkmck.html
- Wikipedia: *UNIX System Laboratories, Inc. v. Berkeley Software Design, Inc.*：https://en.wikipedia.org/wiki/UNIX_System_Laboratories,_Inc._v._Berkeley_Software_Design,_Inc.
- Wikipedia: *Berkeley Software Distribution*（版本年表）：https://en.wikipedia.org/wiki/Berkeley_Software_Distribution
- Klara Systems, *The History of FreeBSD* 系列（386BSD patchkit / 分叉 / 发行周期）：https://klarasystems.com/articles/ 与 https://www.reddit.com/r/BSD/comments/1ss7tdo/（1993-11 NetBSD/FreeBSD 拒绝合并的原帖讨论）
- FreeBSD 5.0-RELEASE Announcement（2003-01-19，「近三年工作」）：https://www.freebsd.org/releases/5.0R/announce ；Early Adopter's Guide（性能回退警告）：https://www.freebsd.org/releases/5.0R/early-adopter ；5.3-RELEASE Notes（首个 production 5.x）：https://www.freebsd.org/releases/5.3R/relnotes
- Baldwin & Roberson, *Locking in the Multithreaded FreeBSD Kernel*（USENIX BSDCon 2002）：https://papers.freebsd.org ；Lehey SMPng 案例研究（2000-06 立项会议）
- FreeBSD Foundation, Dawidek, *How ZFS Made Its Way into FreeBSD*：https://freebsdfoundation.org/wp-content/uploads/2023/06/Dawidek_ZFS_2.pdf ；OpenZFS History（FreeBSD 7.0/2008）：https://openzfs.org/wiki/History
- McKusick soft updates 页（2000-06-21 集成）：https://www.mckusick.com/softdep ；USENIX ;login: 2013 fsck 性能文
- FreeBSD Foundation, *Toolchain Modernization*（GCC 4.2.1 移除，2020）：https://freebsdfoundation.org/blog/project-update-toolchain-modernization ；Chisnall, *LLVM in the FreeBSD Toolchain*（AsiaBSDCon 2014）
- FreeBSD Handbook Ch.12 Linux Binary Compatibility（1995 引入 / Doom）：https://docs.freebsd.org/en/books/handbook/linuxemu ；FreeBSD Foundation 2021 Linuxulator 投入公告
- Wikipedia: *DragonFly BSD*（2003 自 4.8 分叉）：https://en.wikipedia.org/wiki/DragonFly_BSD ；Dillon 2003-07 分叉宣言（freebsd-hackers 存档）
- NetBSD 官方历史与可移植性页（53 架构）：https://www.netbsd.org/about/history.html 、https://www.netbsd.org/about/portability.html ；LWN, *The future of NetBSD*（2006-08 core 辞职危机）：https://lwn.net ；NetBSD 2006 Q2-Q3 报告缺失与新 core 组建记录：https://www.netbsd.org/changes/
- Wikipedia: *Theo de Raadt* / *OpenBSD*（1994-12 被逐、1995-10 分叉、1.2 于 1996-07 发布）；OpenBSD Innovations 页（malloc 随机化沿革）：https://www.openbsd.org/innovations.html ；Moerbeek, *A new malloc(3) for OpenBSD*（EuroBSDCon 2009）：http://www.openbsd.org/papers/eurobsdcon2009/otto-malloc.pdf
- Theo Buehler/de Raadt 等, *OpenBSD Security Mitigations*（EuroBSDCon 2023，86%/39% pledge/unveil 统计）：https://openbsd.org/papers/eurobsdcon2023-bluhm-mitigations.pdf ；LWN unveil 报道（2018-09）
- OpenSSH 官方历史（OSSH/1.2.12 溯源）：https://www.openssh.org/history.html ；Wikipedia: *OpenSSH*
- Undeadly, *OpenBSD reverse engineers more wireless drivers*（2006-09）：https://undeadly.org/cgi?action=article;sid=20060917020241 ；Floeter, *Proactive Wireless Networks With OpenBSD*（BSDCan 2006）：https://www.openbsd.org/papers/bsdcan06-wlan/
- FreeBSD press release 2000-03-09（BSDi 与 Walnut Creek 合并）：https://www.freebsd.org/press/press-rel-4 ；TFiR 对基金会 20 周年访谈（Justin Gibbs, 2000-03 创立）：https://tfir.io/freebsd-foundation-celebrates-20th-anniversary
