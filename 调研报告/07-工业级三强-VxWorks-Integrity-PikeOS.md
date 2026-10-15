# VxWorks / INTEGRITY / PikeOS 调研报告（工业级安全关键 RTOS 三强）

> 调研日期 2026-10-15 · 基于公开资料与源码快照 · 许可证：三家均为商业闭源（专有 EULA，具体条款未公开；本报告全部证据来自官方页面/白皮书/标准文档/学术论文，证据等级随文标注）

## 0. 一句话定位

- **VxWorks（Wind River，Aptiv 全资子公司）**：宏内核形态的硬实时 RTOS（wind kernel + 可选用户态 RTP 进程），靠"模块化 + 认证证据包"横跨航天 DAL A 到工业；装机量约 20 亿设备。
- **INTEGRITY（Green Hills Software）**：分离内核（separation kernel）路线的极致——内核不动态分配内存、资源"保证可用"，INTEGRITY-178 tuMP 是全球首个通过 CAST-32A 多核目标的 DO-178C DAL A RTOS，宣称唯一达 EAL6+（SKPP）的商业 OS。
- **PikeOS（SYSGO）**：L4 血统分离微内核 + Type-1 虚拟化，用"分区 + 人格（personality）"在一块芯片上同栖 Linux 与硬实时 RTOS，欧洲航空航天/铁路/工业主力。
- **横向专题**：三者共同支撑的范式是**混合关键性（mixed-criticality）**——空间分区（MMU/MPU 隔离）+ 时间分区（ARINC 653 周期窗口），用静态化换取确定性，用认证证据包换商业护城河。

## 1. 谁在做：组织/社区/治理/活跃度/商业背书

三家均为私营/集团内闭源公司，无开源社区，"活跃度"体现为版本节奏与认证/订单新闻。

- **Wind River**：1981 年创立；2009 年被 Intel 收购；2018 年卖身 TPG；2022-01-11 Aptiv 宣布以 43 亿美元现金收购（2022-12-23 以修订后 35 亿美元交割），收购时年收入约 4 亿美元（路透/官方新闻稿，已核实）。**注意甄别**：2025 年 Aptiv 分拆上市的是 EDS 线束业务 Versigent，**不是** Wind River；截至 2025Q3 Wind River 仍是 Aptiv 全资子公司并保持 mid-teens 收入增长（Aptiv 财报电话会）。VxWorks 7 为当前主力版本，2025 年仍在发版（Wind River 博客）。
- **Green Hills Software（GHS）**：1982 年 Dan O'Dowd 与 Carl Rosenberg 创立，私营，总部美国加州圣巴巴拉，无外部融资记录（待核实股权结构）。创始人 O'Dowd 因"The Dawn Project"（斥资约 1000 万美元投放 Tesla FSD 儿童假人撞击广告、2023 超级碗广告、2022 竞选参议员）深陷舆论争议，被指利用安全叙事为自家安全关键软件业务导流（CNBC/Fox Business 报道）。工程背书极硬：F-35、B-2、A380、787（经 Honeywell）、C-130J 特种任务显示处理器、Terma 2021 电子战控制器、Northrop Grumman EGI-M（2026 首批量产交付，GHS 新闻页）。
- **SYSGO**：1991 年创立于德国（美因茨），私营。PikeOS 5.1.x 为当前线：5.1.3（2023-09 通过 atsec 完成针对 NXP LS1023A/LS1043A 的 CC EAL5+ 认证、TÜV SÜD 安全预认证），5.1.5 扩展 RISC-V（SYSGO 官网/博客）。2023 年 ESA BRAVE Days 上展示 PikeOS 用于宇航级 NG-Ultra SoC（ESA 演讲 PDF）。客户以欧洲航空航天/铁路/工业为主（Airbus/Thales 等具体型号清单：待核实）。

## 2. 总体架构：内核形态、分层与组件

- **VxWorks**：**宏内核 RTOS**（官方与学术文献一致，勿被个别营销的"microkernel"措辞误导）：wind kernel 提供抢占式优先级调度（256 级优先级 + 同级可选 round-robin）、任务数不限（vxworks6.com 对比文、DiVA 2008 层级调度论文）。传统形态是所有任务与内核同处单一平坦地址空间（特权态）；VxWorks 6.x 起引入 **RTP（Real-Time Process）**——MMU 保护的用户态进程，崩溃只死自己、内核存活，"looks remarkably like a Linux user-space process right down to the stack guard pages"（Wind River Learning、Raima 文档、Stack Overflow 讨论）。VxWorks 7 的模块化靠工程体系：**VSB（源码构建）/VIP（镜像工程）/DKM（可下载内核模块）** 三级裁剪 + **VxBus** 驱动框架与组件描述语言（learning.windriver.com）。安全关键线是独立的 **VxWorks Cert Edition**（DO-178C DAL A 证据包、IEC 61508 SIL 3 TÜV、ISO 26262 ASIL D）与 **VxWorks 653**（ARINC 653 分区版）。
- **INTEGRITY**：**分离内核**。设计取向与 VxWorks 相反：内核极小且**不做动态内存分配**，对象与内存在系统生成期固定（维基/GHS 技术页），从而对每条任务给出"保证的资源"（guaranteed resources）而非尽力而为；地址空间隔离为第一原则，目标 NSA 分离内核保护轮廓（SKPP），EAL6+（GHS 宣称、维基引述，证据等级：厂商声明）。多核版 tuMP 提供 AMP/SMP/BMP 形态及 Safe* 认证变体（SafeAMP/SafeSMP/SafeBAMP，官方页面术语，细节待核实）。
- **PikeOS**：**L4 血统分离微内核 + Type-1 虚拟化**。微内核"最初按 Liedtke 的 L4 建模"（EE Times 2007《The PikeOS Concept - History and Design》、ResearchGate《Evolution of the PikeOS Microkernel》），直接跑在硬件上，负责隔离、CPU 与物理内存管理；其上是分区（partition）与**人格**抽象——每个分区可挂不同 API：PikeOS 原生、POSIX、ARINC 653 APEX、或整只 Linux 客户机（ELinOS）。虚拟化两模式：半虚拟化（改客户机内核"主要在内存管理"，全架构可用）与全虚拟化（需硬件支持）（sysgo.com）。SYSGO 另有微内核形式化正确性工作公开演示。

## 3. 核心机制深挖

**（a）ARINC 653 两级调度与 MAF——VxWorks 653 的确定性来源**。Wind River IMA 白皮书原文："In VxWorks 653, the module OS performs ARINC 653 scheduling of individual partitions; within each time slot, the partition OS uses the VxWorks scheduler"——模块级 OS 按**主时间框架（MAF）**的固定周期表调度分区，分区内才是普通 wind 调度器（白皮书 514450、MDPI Applied Sciences 2020 分层调度研究）。MAF 表在集成时离线算好并冻结：每个分区的执行窗口、偏移、周期全部静态，WCET 被预算窗口直接封顶。这正是确定性三源"静态配置 + WCET 分析 + 限制动态行为"的教科书实现。代价见 §6：窗口刚性、利用率损失。

**（b）INTEGRITY-178 tuMP 的 BAM——把"干扰通道"变成可监控对象**。多核共享资源（LLC、内存带宽、总线）会破坏分区幻觉，FAA 为此发 CAST-32A（后被 AC 20-193 咨询通告吸收）。tuMP 的回答是 **BAM（Bandwidth Allocation and Monitoring）**：按 DAL A 目标开发，"观察干扰通道并缓解之"，确保关键应用拿到**保证份额**的共享资源带宽（GHS 官方 AC 20-193 页面）；由此成为全球首个含 CAST-32A 多核目标的 DO-178C DAL A 认证（Vertical Magazine 报道）。这是"时间分区"从 CPU 时间向**内存系统时间**的推广——软实时系统里少见的机制级创新。

**（c）PikeOS：分区 + 人格 + 健康监控**。CC 安全目标（v4.2.3/v5.1.3 公开 PDF）定义分区资源 = 空间分区（物理内存）+ 时间分区（分配的 CPU 时间）；时间分区调度器有多核自适应版本且经 EN 50128 认证（SYSGO 2016 白皮书）。**健康监控**实现 ARINC 653 的 HM 模型：检测分区故障与硬件错误，执行预定义恢复动作（重启/隔离/失效安全切换）（sysgo.com/rtos-hypervisor）。半虚拟化 Linux 客户机（ELinOS）只改内存管理相关代码，使 Linux 与 DAL A 级分区同芯共存——三强中唯一把"通用 OS 当分区里的一个应用"产品化的。

**（d）混合关键性范式总览**。学术定义（Rufino et al., SEUS 2010）：时间分区 = 分区互不干扰彼此的实时保证；空间分区 = 内存与互连隔离。DO-178C 的 DAL A–E 分级让不同关键性软件**同硬件共存而互不拖累认证等级**。批评方向也存在：ECRTS 2019 论文指出严格时间片切分并非分区调度的唯一形式，纯 ARINC 653 常态化浪费窗口带宽。

**（e）认证经济学**。DAL A 生产率仅 **3–12 SLOC/天**（Koopman，Better Embedded Software 博客 2018）；DO-178B 已使认证成本上升 20–40%（Semantic Scholar 论文）；DAL A 还要求目标码覆盖。于是**证据包本身成为商品**：买 VxWorks Cert Edition/INTEGRITY-178 实质是买"复用认证证据"的权利——这是三强的商业模式核心，也是闭源的原因。

## 4. 源码走读（闭源替代方案：公开文档/标准 API/血统源码）

三家内核源码均不公开，无目录/结构体可引；以下为等效"走读"路径，证据等级如实标注：
1. **ARINC 653 APEX API**（三家均实现的公开标准，函数名真实可查）：`CREATE_PROCESS` / `START` / `PERIODIC_WAIT` / `SET_PARTITION_MODE`——`PERIODIC_WAIT` 即进程向分区调度器交还剩余窗口的同步点，MAF 确定性在此 API 层显形（ARINC 653 P1-5 标准、NASA Orion IV&V 论文）。
2. **VxWorks 公开 API 语义**：`taskSpawn()`/`semTake()` 等几十年来稳定的内核任务接口；RTP 与内核任务的特权级/地址空间差异见 Wind River Learning 与 Raima 文档（证据等级：官方文档）。
3. **INTEGRITY-178 tuMP**：机制术语（BAM、AC 20-193、guaranteed resource availability）取自 ghs.com 产品页（证据等级：官方营销页，无代码）。
4. **PikeOS 血统开源替代**：其微内核建模自 L4，直系开源参照为 L4Ka::Pistachio（github.com/l4ka/pistachio，可读内核源码研究其 IPC/调度原语）；PikeOS 本体仅 CC 安全目标 PDF 公开（atsec 2023 证书附件含功能需求条款）。

## 5. 亮点

- **两级调度的清晰分层**：MOS 管分区、POS 管进程，职责与认证边界一一对应——机制与策略分离的认证友好版。
- **INTEGRITY 的"无动态分配"内核**：用构造期静态化把"资源保证"变成数学性质而非运行时承诺。
- **BAM 把多核干扰通道纳入预算与监控**：时间分区从 CPU 推广到内存系统，2026 年视角看是解决大小核/共享 LLC 问题的先声。
- **PikeOS 的人格抽象**：一个隔离单元多种 ABI/Linux 客机，混合关键性的工程最优解之一。
- **认证证据包商品化**：3–12 SLOC/天的 DAL A 成本被摊薄为"买证据"，产业效率设计巧妙。

## 6. 不足与争议

- **URGENT/11（2019，Armis）**：VxWorks IPnet TCP/IP 栈 11 个漏洞、6 个可远程代码执行（CVE-2019-12256，CVSS 9.8），波及约 20 亿设备；此前 32 年仅 13 个公开 CVE——暴露闭源 RTOS"低 CVE 数"可能是低披露而非高安全，且固件化部署使补丁分发极慢（Armis、RunSafe 2025 报告）。注意 Ripple20 属 Treck 栈，扫描器常误报混淆。
- **ARINC 653 窗口刚性**：静态 MAF 对动态负载（尤其 AI 类）极不友好，带宽浪费被 ECRTS 2019 等研究点名。
- **闭源锁定与证据不可审计**：认证证据只对适航当局开放，外部无法复核"unhackable"式宣传；GHS 的 EAL6+ 表述属厂商声明。
- **GHS 创始人争议**：Dawn Project 反 Tesla 运动被广泛批评为利益冲突（自家卖安全关键软件），Fox Business 称无法核实其多项说法。
- **治理单点**：Wind River 十年内三度易主（Intel→TPG→Aptiv），路线图受资本运作牵动。

## 7. 对 BerryOS 的借鉴与警示（对照四支柱）

**微内核支柱（借鉴为主）**：
- 借 INTEGRITY 的"内核内无动态分配、对象构造期固定"思想收敛到 BerryOS 调度器/IPC 关键路径：runqueue、任务表、能力表用静态池起步，把"保证的最坏路径"做成构造性质（与 BerryOS 机制在内核/策略可插拔完全兼容）。
- 借 PikeOS 验证的事实：L4 血统微内核可以走到 CC EAL5+/铁路 SIL 4——BerryOS 的 Zircon 务实派路线与可认证性不冲突。
- 警示：别学 VxWorks 把所有服务塞进内核平坦空间的老形态；也别为形式化认证提前缴税（那是市场行为，不是工程必需）。

**省电支柱（部分借鉴，部分冲突）**：
- 借"预算"不借"窗口"：ARINC 653 周期唤醒与 BerryOS 的 tickless NO_HZ 直接冲突，绝不能搬 MAF 循环表；但把分区预算改造成**按任务/服务的带宽与时间预算**（类 BAM 的 runtime/period 配额），空闲时仍可深度睡——预算与 tickless 可共存（预算到期才强制仲裁）。
- 警示：三强的静态全局配置表需要离线工具链，BerryOS 不该引入"编译期冻结调度表"这种与动态电源管理互斥的机制。

**AI 原生调度支柱（最值得借鉴的一节）**：
- BAM 的"观测干扰通道→按预算仲裁"与 BerryOS AI 调度的遥测 ABI + 用户态策略服务同构：内核提供每任务带宽/CPU 占用的硬预算与遥测（慢循环），AI 服务只在预算框架内重排放置/频率/优先级（快循环）——这正是三强"护栏内自由"哲学的 AI 版。
- 借 DO-178C 的分级思想给任务打关键性等级，AI 调度器的权限按等级递减：高关键任务的预算 AI 只能加不能减（硬护栏），低关键任务全开放。
- 警示：别让 AI 策略获得无界动态权——三强的教训是确定性来自"限制动态行为"，AI 调度若想可解释，其动作空间必须像 MAF 一样可枚举、可审计。

**PC 高性能支柱（借鉴有限）**：
- 借两级调度框架：BerryOS 的 per-core runqueue 之上可加一层"服务组/分区"仲裁（类 MOS/POS），为未来 mixed-criticality 场景（跑 RT 任务 + 桌面负载）留机制位。
- 警示：多核干扰（CAST-32A 主题）在 PC 上同样存在（LLC/内存带宽），但 BerryOS 软实时定位不需要 BAM 级硬件监控，先留遥测点即可，别过度设计。

**工程文化**：三强证明"认证证据即产品"，BerryOS 应学的不是认证流程，而是**可追溯性习惯**——每个调度/IPC 机制改动附最小可验证证据（对应 BerryOS G0–G5 关卡），这正是关卡式验证与 DO-178C 的精神同源处。

## 8. 参考资料链接

- VxWorks IMA 白皮书（MOS/POS 两级调度原文）：https://www.windriver.com/sites/default/files/2023-06/514450%20-%20Safety-Critical%20Software%20Development%20for%20Integrated%20Modular%20Avionics_Whitepaper.pdf
- VxWorks 分层调度学术研究（MDPI 2020）：https://www.mdpi.com/2076-3417/10/7/2464
- VxWorks Cert Edition 数据表（DAL A/SIL 3/ASIL D）：https://www.windriver.com/sites/default/files/2022-04/656967853.pdf
- VxWorks 调度对比（256 优先级）：https://www.vxworks6.com/app/comparison-of-cpu-scheduling-in-vxworks-and-lynxos
- RTP 官方课程：https://learning.windriver.com/path/vxworks7-essentials-application-development/real-time-processes
- VxWorks 内核/用户空间（Raima 文档）：https://docs.raima.com/rdm/15_2/ug/dev/_vxw/vxw7Overview.htm
- URGENT/11（Armis）：https://www.armis.com/research/urgent-11
- INTEGRITY-178 tuMP 产品页：https://www.ghs.com/products/safety_critical/integrity_178_tump.html
- AC 20-193 多核干扰与 BAM：https://www.ghs.com/products/safety_critical/integrity_178_multi_interference.html
- 首个 CAST-32A 多核认证报道：https://verticalmag.com/press-releases/worlds-first-multicore-avionics-certification-to-cast-32a-uses-the-integrity-178-tump-multicore-rtos
- Terma 选型新闻（2021）：https://www.terma.com/news-events/news/news-archive/2021/terma-selects-integrity-178-tump-rtos-for-next-generation-electronic-warfare-controller
- PikeOS 产品页（5.1.3 TÜV SÜD 预认证）：https://www.sysgo.com/pikeos
- PikeOS 架构页（分区/健康监控/虚拟化）：https://www.sysgo.com/rtos-hypervisor
- PikeOS 5.1.5 RISC-V（SYSGO 博客）：https://www.sysgo.com/blog/article/pikeos-515-risc-v-support-expands-the-reach-of-certifiable-real-time-virtualization
- PikeOS CC 安全目标 v5.1.3（atsec 2023）：https://atsec.com/webfiles/Common_Criteria_Certificates/2023_Certificates/2023-09-18_PikeOS_Separation_Kernel_v5.1.3_for_the_NXP_LS_1023A-LS1043A_Processor/1185b_pdf.pdf
- PikeOS 概念与历史（EE Times 2007，L4 血统）：https://www.eetimes.com/the-pikeos-concept-history-and-design
- L4Ka::Pistachio（PikeOS 血统开源参照）：https://github.com/l4ka/pistachio
- ESA BRAVE Days 2023（PikeOS 用于宇航 NG-Ultra）：https://indico.esa.int/event/456/attachments/5847/9745/BRAVE-Days-2023_Presentation_SYSGO_nov2023.pdf
- 时间/空间分区学术定义（SEUS 2010）：https://dl.ifip.org/seus/seus2010/RufinoCV10
- 分区调度批评（ECRTS 2019）：https://drops.dagstuhl.de/LIPIcs.ECRTS.2019.8
- NASA Orion ARINC 653 IV&V：https://www.nasa.gov/wp-content/uploads/2016/10/482470main_2530_-_ivv_on_orions_arinc_653_flight_software_architecture_100913.pdf
- DAL A 成本 3–12 SLOC/天（Koopman）：https://betterembsw.blogspot.com/2018/10/cost-of-highly-safety-critical-software.html
