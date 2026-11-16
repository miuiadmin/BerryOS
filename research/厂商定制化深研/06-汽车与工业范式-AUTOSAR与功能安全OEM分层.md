# 06 号底稿:汽车与工业范式——AUTOSAR 与功能安全世界的 OEM 分层定制

> **采集日期**:2026-11-10(外部页面为当日抓取快照)
> **性质**:研究底稿(证据供给,非立法)。为「厂商定制化」深研系列第 06 篇;站在 01(现状与缺口)/02(安卓反面)/03(正面范式)/04(收口提案)之上,解剖重监管行业做了 20 余年的「平台-厂商分层定制」标准范式——AUTOSAR(经典+自适应)与 ISO 26262 功能安全体系,条款级/字段级拆解到可复用的结构。
> **消费面**:04 号四层模型(配置/扩展/替换/禁改)与 BCTS(暂名)的工业证据补强;「v2 攻工业 RT」(AGENTS.md:11)的认证路径输入(安全假设清单/SEooC 机制);「厂商定制面立法 ADR」的汽车范式对照材料。
> **方法**:外部一手优先(autosar.org 官方页 / vector.com / elektrobit.com / dspace.com / qnx.software / ghs.com;ISO 标准原文付费不可抓取,以 iso.org 目录页 + 多家独立二手交叉并如实标注);数值/枚举/版本号类事实尽量两源交叉,单源或两源不一致处标「待核实」;事实与推断分开,推断标「推断」。仓库内引用给相对路径+行号。
> **版本**:v1.0(文件名按任务指定不带版本号,版本记于本行)。
> **来源索引**:见文末;引用本稿请以文末索引为准,标注「待核实」处不得作为立法依据。

---

## §0 摘要(一屏版)

- **AUTOSAR 是「定制合同化」的工业级完全体**:可定制面全部编码为机器可校验的交换格式(ARXML,XML Schema + 语义约束),方法论把整车厂/供应商的分工固化为三段工件流(系统配置描述 → ECU 提取 → ECU 配置描述),「平台 vs 增值」的分界线被一句话立法:**RTE 之上是 OEM 专属与竞争的应用,RTE 之下是标准化、与 OEM 无关的基础软件**(autosar.org Classic 页原文)——这是 BerryOS「冻住下三路、开放上三路」(01 号 §4.3)在 2003 年的汽车原版。
- **标准即接口,多家工具商在同一份合同上竞争**:Vector(MICROSAR+DaVinci)、EB(tresos AutoCore+Studio)、dSPACE(SystemDesk+TargetLink)各自的工具链靠同一份 ARXML 互操作,官方甚至发布跨工具中立的工作流示例工件与「确保规范可实现」的参考实现(CAPI);AUTOSAR 自己维护验收测试(acceptance tests)——BCTS 想做的事,这里有一份 20 年的答卷。
- **安全论证以「假设清单」跨层继承**:ISO 26262 的 SEooC(独立安全单元)机制让平台方(OS/BSW/芯片)在不知道具体车型的情况下按**假设的安全需求+使用假设(AoU)**做认证,把安全论证打包成安全手册交给 OEM,OEM 在集成时逐条核对假设——QNX 官方明文把 QNX OS for Safety 定位为 SEooC。这是 BerryOS v2 工业认证最直接的路径模板。
- **对 BerryOS 的总判断(推断)**:该继承的是「合同哲学」(schema 化配置/槽位即标准/验收测试/认证变体/假设清单机器化),该抛弃的是「流程重量」(ARXML 全量格式的复杂度税、联盟式治理的会费与 FTE、全员年度重发版节奏);BerryOS 的机会是把 AUTOSAR 用文档承载的合同(假设清单/配置)下沉为**代码可执法的合同**(manifest 字段 + 能力签名 + BCTS 断言)——能力模型(handle+rights)恰好是 AoU 的天然机器载体。

---

## §1 AUTOSAR Classic:三段方法论与分层架构(条款级)

### 1.1 分层架构:RTE 这条线就是「平台/增值」的产权分界

AUTOSAR Classic 在最高抽象层区分三层跑在微控制器上的软件(autosar.org Classic 页 About 节,与 Wikipedia AUTOSAR 条目两源一致):

- **应用软件层(ASW)**:大部分硬件无关;以软件组件(SWC)为单位。
- **运行时环境(RTE)**:应用的**完整接口**——「The RTE represents the full interface for applications」;SWC 之间、SWC 与 BSW 的一切通信经 RTE。
- **基础软件(BSW)**:再分**三个大层 + 复杂驱动(CDD)**:服务层(Services,又分系统/存储/通信等功能组)、ECU 抽象层、微控制器抽象层(MCAL)。R25-11 官方模块图把槽位开到字段级:系统服务/存储服务/密码服务/车外通信服务/通信服务/IO 硬件抽象/复杂驱动/车载设备抽象/存储硬件抽象/密码硬件抽象/无线通信硬件抽象/通信硬件抽象/微控制器驱动/存储驱动/密码驱动/无线通信驱动/通信驱动/IO 驱动/微控制器(autosar.org Classic 页模块清单,每项链接到对应 SWS 规格文档)。

关键立法句(autosar.org Classic 页 Architecture 节原文,**加粗为本文所加**):

> 「On the upper side of the RTE, this abstraction layer enables the development of **OEM-specific and competitive** software applications. On the lower side of the RTE, it enables the **standardization and OEM-independence** of basic software.」

即:RTE 之上=OEM 的差异化产权区,RTE 之下=跨 OEM 通用化的平台区。**这一句话就是「平台与产品正交」(03 号 §9.2)在汽车世界的规范原文**。配套特性还有:ECU 软件跨车系/变体的可伸缩性、功能跨 ECU 分布部署、多来源软件模块集成(同页)。

另一个核心概念是**虚拟功能总线(VFB)**:尚未部署到具体 ECU 的一组抽象 RTE,经专用端口通信,把应用与基础设施解耦——「From an application point of view, no detailed knowledge of lower-level technologies or dependencies is required」(Classic 页 Concept 节)。VFB 是「消费者只认协议不认实现」(`docs/驱动模型设计-v1.0.md` §1,经 01 号 §1.2 转引)的汽车同构:SWC 只认端口,不认对面是本 ECU 还是总线那头的另一个 ECU。

底统渊源:Classic 平台的 OS 基于 OSEK(Wikipedia AUTOSAR 条目);Vector 产品说明互证——「OS 模块是把 OSEK 操作系统面向 AUTOSAR ECU 扩展,支持多核,可在安全相关环境运行(也可作为 Linux 与 QNX 的 GuestOS 获得),变体按可伸缩性等级(SC)分级」(cdn.vector.com MICROSAR 产品信息日文版 PDF 摘录,经 DuckDuckGo 索引;SC 分级细节**待核实**)。

### 1.2 方法论三段:ARXML 作为机器可校验的配置合同

AUTOSAR 方法论(现归 Foundation 档)把跨组织协作固化为**三段工件流**(Wikipedia AUTOSAR 条目 Methodology 节;AUTOSAR 官方 TPS 规格摘录两源一致):

1. **System Configuration Description(系统配置描述)**:整车级全部系统信息与跨 ECU 约定(如总线信号定义)。由 OEM 主导。
2. **ECU Extract(ECU 提取)**:从系统配置描述中抽出**单个 ECU 需要的那部分**(如该 ECU 可访问的信号)——官方 ECUConfiguration 规格原文:「The configuration process of an ECU starts with the splitting of the System Description into several descriptions, whereas each contains all information about one single ECU… The creation of an Ecu Extract is described in detail in the System Template specification」(AUTOSAR_TPS_ECUConfiguration.pdf,R22-11 版,经搜索索引摘录;PDF 全文未逐条核)。
3. **ECU Configuration Description(ECU 配置描述)**:单个 ECU 的全部 BSW 配置信息,据此生成 BSW 模块代码、SWC 代码与可执行软件(Wikipedia;dSPACE SystemDesk 产品目录互证——系统描述建模后须「transferred to a basic software configuration tool」)。

**合同的技术形态**(autosar.org Classic 页 Methodology and Templates 节原文):

> 「The descriptions (.arxml) are based on the AUTOSAR Templates which define the **formal exchange format (AUTOSAR Schema)** and the **semantic constraints** which go along with the exchange format. … Various generators can utilize the information from the descriptions to support the configuration and generation of the RTE and the AUTOSAR basic software (including the operating system).」

拆开即三个可复用要素:**①交换格式有 XML Schema(机器可校验语法);②语义约束随格式同发(机器可校验语义);③任意厂商的生成器都可消费同一份描述(工具中立)**。这正是「配置合同」区别于「配置惯例」的三件套(对照 03 号 §9.1 的归纳)。

**官方还诚实记录了方法论的现实退化**(Classic 页 Workflow-Example 节原文):「The original method proposed by AUTOSAR is a top-down approach… However, we are aware that this is not always possible in practice. Some projects also specify individual control units based solely on an existing communication matrix and manual settings.」——自顶向下的三段流在真实项目里经常塌缩成「拿着通信矩阵手配单 ECU」。为此 AUTOSAR 发布了**工具中立的工作流示例工件**(Iteration 01:两个 ECU + CAN 总线的全套 ARXML 工件 + 说明 PDF,官方声明用多工具链评估以保证中立性,并邀请伙伴共建 Iteration 02:更多节点/以太网/对接 Adaptive ECU)。「用参考工件证明方法论可落地、且不偏向任何工具商」——这一姿态本身值得 BerryOS 学(对照 §4.4 CAPI)。

### 1.3 OEM / Tier1 / Tier2 三级分工:「平台 vs 增值」落在哪一格

分工的证据链(全部一手/两源):

| 层级 | 干什么 | 证据 |
|---|---|---|
| **OEM(整车厂)** | 定义系统配置描述(功能分配到 ECU、通信矩阵、信号定义);在 RTE 之上开发 OEM 专属与竞争性应用 | autosar.org Classic 页 Architecture 节;Wikipedia Methodology 节 |
| **Tier1(系统供应商)** | 拿 ECU Extract 做 ECU 配置,集成 BSW+SWC 交付整个 ECU;交付「预集成+项目定制」的形态 | Vector:「MICROSAR Classic is the production-ready AUTOSAR Classic platform **used by OEMs and Tier-1 suppliers worldwide**, that goes beyond the standard」「Receive **pre-integrated software with project-specific extensions and services**, delivered as a complete solution」「Available as **generic or customer-specific configurations**」(vector.com MICROSAR Classic 页) |
| **Tier2(芯片/软件/工具供应商)** | 供 MCAL(微控制器抽象)、BSW 模块、工具;半导体厂加入联盟 | AUTOSAR 定位原文:「alliance of OEM manufacturers, Tier 1 automotive suppliers, semiconductor manufactures, software suppliers, tool suppliers and others」(history 页);Vector:「By sharing your required combination of derivatives, compilers and basic software modules at an early stage… Vector to deliver a tailored solution」+「Seamless integration of the microcontroller abstraction layer (MCAL), supported by a fully integrated set of hardware and peripheral drivers」;EB 评估包直接落在 Infineon AURIX TC38XQ/TC4D、NXP S32K14X、Renesas RH850/F1KM、Synopsys Silver、dSPACE VEOS 上(elektrobit.com 评估包页,两处页面同文) |

分工的**动机账本**写在会员权益页(autosar.org partners 页原文):OEM 得到——「Establish development distribution among suppliers / Compete on innovative functions with increased design flexibility / Simplify software and system integration / Reduce overall software development costs」;供应商得到——「**Reduce version proliferation** / **Reuse software modules across OEMs** / Increase efficiency of application development」。「跨 OEM 复用、削版本繁殖」就是「平台保升级主权」(03 号 §9.4)的供给侧表述。

**推断**:注意这个分工与安卓的镜像差异——AUTOSAR 里 OEM 是**标准组织的共有人**(Core Partner 多为 OEM)兼系统定义者,Tier1 是平台的组装者;安卓里 OEM 只是标准的接收端。BerryOS 未来对 OEM 的定位更接近「系统集成者」(应用规范 §11.1)而非「标准共有人」,但 AUTOSAR 证明:**只要合同的机器化执法在,「接收端」也可以安全地定制**。

### 1.4 ARXML 对照 Fuchsia product config:同是配置合同,作用域不同

| 维度 | AUTOSAR ARXML | Fuchsia product configuration(03 号 §1.2) | BerryOS 产品清单(04 号 §2.1 提案) |
|---|---|---|---|
| 合同作用域 | **跨组织**:OEM↔Tier1↔工具商之间交换 | **平台↔产品**:组装期注入平台工件 | 平台↔OEM(单平台方) |
| 校验机制 | XML Schema + 语义约束(Templates 规范同发) | platform 键 schema(`platform_settings.rs` 源码+build_type 收窄) | 提案:schema 进 L2、未知键硬拒(berry-manifest add-only 纪律同构) |
| 覆盖面 | 整车系统/通信矩阵/每 ECU 配置/诊断/标定(极重) | 产品镜像组装(轻) | 提案:预装/品牌/策略表/板级(中) |
| 生成物 | 各家工具生成 BSW/RTE 代码 | Bazel 规则组装镜像 | berry-assemble 产 rootfs/镜像 |
| 重量代价 | schema 巨大→工具即门槛(§6.4) | 轻→表现力受平台枚举限制 | 待定(开放问题 Q1) |

**推断**:ARXML 证明「配置可以成为跨公司法律级合同」;Fuchsia 证明「配置合同可以轻到只描述一个产品」。两者是同一机制在不同信任半径下的形变——BerryOS 单平台方,信任半径更接近 Fuchsia,但 v2 工业 OEM 若带自家工具链进来,会出现 ARXML 式「工具对工具」需求(开放问题 §7-Q1)。

### 1.5 AUTOSAR 三层 × BerryOS 四层:逐层对照

AUTOSAR 的「三层」是**运行时栈分层**,BerryOS 四层是**定制权限分层**(04 号 §2.0),不是同轴概念;对照按「定制动作落在哪」映射:

| BerryOS 定制面(04 号 §2) | AUTOSAR 对应物 | 机制对照 | 差异要点 |
|---|---|---|---|
| **配置面(a)**:声明式产品清单(选什么/默认什么/长什么样) | 方法论三工件:System Config → ECU Extract → ECU Config;全部 ARXML | 都是「产品=数据不是代码」;AUTOSAR 的三段流把配置从整车拆到 ECU,BerryOS 从镜像清单拆到服务 | AUTOSAR 配置描述**生成代码**(BSW 静态配置),BerryOS 产品清单只**选件与调参**不生成内核态代码——重配置面在 BerryOS 由替换面承担 |
| **扩展面(c)**:新增驱动/服务/系统应用 | SWC(SWC 加入 ECU 组合)+ CDD(复杂驱动:标准 BSW 之外的自由区) | SWC≈.bap 扩展包(加应用);CDD≈「不标准化的逃生舱」 | AUTOSAR 给 CDD 留了**明面逃生舱**(绕开标准接外设/旧代码),BerryOS 对应物是扩展包+上游化政策(04 号 §2.3,采 Zephyr 政策)——**留不留明面逃生舱是开放问题(§7-Q5)** |
| **替换面(b)**:按稳定契约换用户态服务实现 | BSW 模块槽位可换实现:同一 SWS(Software Specification)合同,Vector MICROSAR / EB tresos AutoCore / 芯片厂 MCAL 互相竞争 | 槽位合同=SWS 文档+验收测试;BerryOS 槽位合同=wire 协议+conformance 测试(BCTS L1) | AUTOSAR 替换发生在**编译前源码/预编译库**;BerryOS 替换是**运行期服务进程**(wire 协议),隔离与热换更强(#28 代号链) |
| **禁改面(d)**:内核/IPC 核心/syscall/wire 语义/能力模型/intent 词表 | 规范本身:SWS/TPS 规格文本、RTE 语义、ARXML schema;OS 内核(OSEK 扩展)语义 | 规范=合同的「宪法层」;BerryOS 禁改面=L0 永冻+L1 add-only+能力模型唯一机制 | AUTOSAR 靠**文档标准+合同**执法;BerryOS 靠**二进制统一+工具执法**执法(abi-check/mkapp 门禁,01 号 §2)——执法强度更高,但外化未做(01 号缺口 4) |

---

## §2 AUTOSAR Adaptive:面向 SOC 的服务化平台,与微内核形态的相似度

### 2.1 形态总览

AUTOSAR Adaptive Platform(AP)面向高性能处理器(µ-processor)的运行时环境 **ARA(AUTOSAR Runtime for Adaptive Applications)**:标准化接口与服务,以 ARXML 语言描述(autosar.org Adaptive 页,两源:页面正文 + 同站 CAPI 节)。要点:

- **POSIX 底座**:「The AUTOSAR Adaptive Platform runs on a POSIX-based operating system… AUTOSAR recommends the **PSE51 profile**(IEEE 1003.13)」(Adaptive 页 POSIX OS 节;Wikipedia 互证)。PSE51=最小实时控制档(有线程/优先级调度/同步,无复杂内存管理)。
- **服务化通信 ara::com**:Communication Management 建立/监视应用间通信路径,「locally and across different machines」;路径可设计期定义、启动期初始化或**运行期动态建立**;规范定义 API 语法、与系统模型的关系、行为语义(状态机+前置/后置条件),**刻意留内部软件架构开放**(Adaptive 页 Communication 节原文:「It intentionally leaves the internal software architecture open」)。传输绑定:SOME/IP 为主(Wikipedia;官方页仅通过 DDS security/MACsec 等链接侧证,**SOME/IP 绑定细节待核实**)。
- **功能集群(Functional Cluster)而非分层**:「these clusters are logically grouped but **not hierarchically stacked**」——集群在 OS 之上、应用之下,但集群彼此不构成抽象层(Adaptive 页;Wikipedia 同口径)。集群**每(虚拟)机至少一实例,服务可分布在整车网络**——「the libraries of the functional clusters are available in one instance per (virtual) machine, while services can be distributed in the vehicle network」(Adaptive 页;Wikipedia 互证)。R25-11 集群清单(官方模块图):Platform Foundation=通信管理/执行管理/诊断/入侵检测管理器(IdSM)/网络管理/持久化/时间同步/密码/原始数据流/平台健康管理(PHM)/日志追踪/防火墙/Core;Platform Services=更新与配置管理(UCM)/状态管理(SM);标准化应用接口=传感器接口;Vehicle Services=V-UCM/Automotive API(R24-11 起)。
- **与 Classic 的桥**:Signal-to-Service Translation 机制连接信号世界与服务世界(Adaptive 页)。

### 2.2 执行管理(EM):编排即标准件

Adaptive 页 Runtime 节(一手原文):EM 实现含三个组件——**ExecutionClient**(应用上报启动成功 Running)、**StateClient**(状态管理触发 Function Group 迁移)、**Execution Manager** 本体。**EM 是 OS 启动的第一个进程**;负责启停应用——但**只在 State Management 请求下**启停,故障报告给 SM、恢复动作也归 SM;EM 自己不做。Function Group 是状态迁移的单位。

**推断(与 BerryOS 逐点对位)**:这个切法与 BerryOS 现役/规划几乎逐点同构——

| AP 件 | BerryOS 对应物(仓库事实) |
|---|---|
| EM=OS 第一个进程、按清单拉起应用 | svcmgr 流式拉起(rootfs cpio 条目,`justfile:309-368`;预拉 arm 手写 `user/bins/svcmgr/src/main.rs`)——差别在 AP 是**清单驱动**、svcmgr 现役**源码硬编**(01 号缺口 2) |
| 执行清单/机器清单(ARXML,声明应用执行条件)【清单名目见 PlatformDesign/SWS 文档,本稿未逐条核,**待核实**】 | .bap manifest(声明型,`user/berry-manifest`)+ v2 devmgr 数据驱动装卸(驱动模型 §5) |
| 状态管理(SM)=策略,EM=机制 | 「机制在内核、策略可插拔」同构;监督重启=appd/svcmgr(ADR #46 §3) |
| ara::com 服务发现/动态路径 | 路径命名空间 `/svc` 决策 #6(**v3 才有运行面**,01 号 §1.3)——AP day-one 有、BerryOS 延后 |
| UCM/V-UCM(更新与配置管理,整车更新战役) | 四层 OTA 设计冻结、无代码面(01 号缺口 11) |
| PHM(平台健康管理) | 失败隔离+监督重启(ADR #46);健康监控面未立 |

**相似度判断(推断)**:AP 与「微内核+用户态服务」在**服务拓扑**上同构(平台件全是可分布的标准服务、应用按清单编排、状态机管理生命周期),差异在**底座与权限**:AP 跑在通用 POSIX OS 上(QNX/Linux 均可,§5)、无能力模型,安全靠 IAM/防火墙集群**后补**;BerryOS 能力模型(handle+rights)是 day-one 的权限地基。BerryOS 相对 AP 的真实缺项是**清单驱动编排与命名空间运行面**(都在 v2/v3 议程内)。

### 2.3 AP 的自我验收:参考实现与「规范可实施性」

Adaptive 页 Safety 节原文:「**AUTOSAR is maintaining an exemplary implementation of the adaptive platform. It contains also some unit- and integration tests. It is used to make sure that the specification is implementable.**」——参考实现+单元/集成测试,用途是**证明规范可实现**。该实现现为 CAPI(Common Adaptive Platform Implementation,autosar.org 专设栏目,CAPI 1.0 已发布)。Wikipedia 互证:「the adaptive platform contains both specification and code… AUTOSAR develops an implementation to shorten the validation cycle」。

**推断**:这是 BCTS 设计的最有力外部证据——**平台方自己先过一遍自己的合同**(规范→参考实现→测试),工具链与生态才有中立锚点。BerryOS 的对应物:官方服务实现(fsmgr/vnet/guifbd…)就是天然「参考实现」,它们对 wire 协议的 host 可测单测(01 号 §2 L1 行)就是「规范可实施性」测试的种子;缺的是把这套东西**当作标准工件发布**而非仓内测试(04 号 §2.4 执法外化)。

---

## §3 ISO 26262 安全分解与 SEooC:安全案例如何跨层继承

### 3.1 安全生命周期:item → HARA → ASIL → 分解 → 分配

ISO 26262《Road vehicles – Functional safety》:2011 年首版(限 ≤3.5t 乘用车),2018 年第二版扩到全部道路车辆(不含助力车),共 12 部(10 个规范性部分 + 第 10/11 部为指南;第 11 部半导体、第 12 部摩托车为 2018 新增)(Wikipedia ISO 26262 条目;iso.org 目录页佐证存在)。概念阶段流程(Wikipedia Parts 3-7 节,汇总转述——**条款级原文付费未核,标待核实**):

1. **Item Definition**:确定 item(实现车辆级功能的系统/系统组合,是安全开发的最高对象与起点——Part 1 词汇定义)与顶层功能需求。
2. **HARA(危害分析与风险评估)**:对每个 hazardous event(危害×运行场景)按 **S(severity 伤害严重度,S0–S3)× E(exposure 暴露率,E0–E4)× C(controllability 可控性,C0–C3)** 评级;**ASIL D = S3+E4+C3**,任一维降一档则 ASIL 降一级,ASIL A 以下为 **QM**(仅质量管理)。
3. **Safety Goal(安全目标)**:每个 hazardous event 一个顶层安全需求,继承该危害的 ASIL。
4. **功能安全概念(FSC)→ 技术安全需求**:安全目标细化分配到架构组件;**ASIL 分解**(Part 9)——在充分独立的冗余组件上可把一条需求拆成低 ASIL 的冗余需求。
5. **Part 8 支持过程**:分布式开发的**受控供应商接口**(DIA/开发接口协议——Wikipedia 称 controlled corporate interfaces,iso26262.academy 称 Development Interface Agreement,两源同指)、配置管理、变更管理、**软件工具置信(按用途鉴定工具)**、既有组件的鉴定/proven in use。

### 3.2 SEooC 机制细节:假设清单怎么写、OEM 怎么确认

**定义**:SEooC(Safety Element out of Context,独立安全单元)=不针对特定 item/车型开发的安全相关元素——「A SEooC is a safety-related element developed without a specific item or vehicle in mind, based on assumptions about its future use. The developer assumes safety requirements and an operating context, develops the element under ISO 26262 against those assumptions, and states the capability the element can support, often expressed as an ASIL.」(iso26262.academy SEooC 概念页;Intertek 博客互证:「the supplier creates justified assumptions for the missing information」)。指南位置:ISO 26262-10(2018 版第 10 部「Guidelines on ISO 26262」)——**clause 9**(iso26262.academy 页首标注「ISO 26262-10 §9」+ LinkedIn 技术文 + Embitel 博客「Part-10 of the ISO 26262 standard has the recommendations for development of SEooC」三处二手一致;**标准原文付费,条款号待核实**)。典型 SEooC:微控制器、软件库、可配置软件组件(多源一致)。

**机制的三件核心交付**(iso26262.academy,详列为该站单源归纳,行业安全手册普遍同构——**单源详列,标注**):

| 件 | 内容 | 要点 |
|---|---|---|
| **假设的安全需求(assumed safety requirements)** | 平台方替未来用户「假设」出的安全需求,按它走完整 ISO 26262 流程 | 与真实需求的差口由集成方负责 |
| **使用假设(Assumptions of Use,AoU)** | 安全主张成立的前提条件:「the documented conditions under which the safety claims of the SEooC hold: the assumed environment, the assumed system context and the measures the integrator must provide, such as timing limits or protection from interference」 | **每条必须具体到可在集成时核对**(「specific enough to be checked at integration」);模糊假设是经典缺陷——「an assumption nobody can validate gives the integrator nothing to work with」 |
| **能力主张(capability claim)** | 元素能支撑什么(常以 ASIL 表达)、在哪些假设之下 | 「Claim capability honestly… without overclaiming」 |

**安全手册(集成文档)应含八件**(iso26262.academy FAQ:「ISO 26262 does not prescribe one universal safety manual format, but the integrator needs the same core information」):①元素范围与功能;②假设的安全需求与能力;③**受控的假设清单+每条的理由**;④接口规格与约束;⑤**要求的配置与禁止的配置**;⑥安全机制及其局限与故障信号方式;⑦时序约束;⑧集成验证指南。(可分散在约定的工作产品中。)

**OEM/集成方侧的确认动作**(同源+Intertek):拿真实 item 与假设清单做 **gap analysis(差距分析)**→逐条 **AoU 校验**(校验严格度应随「假设被突破的后果」升级)→不匹配则升级(改集成方案/改配置/要求平台方改)→结果记录;供需双方责任用 **DIA** 划分。该站教学结构还点出两个工程化概念:**FTTI 预算账本**(故障容忍时间间隔在元素与 item 间如何分账)与**假设包含包络**(真实运行环境是否落在假设包络之内)——这两个概念恰是把「假设」变成可计算对象的方向。

**推断**:SEooC 本质是**安全案例的分层继承协议**——平台方无法预知 item,就把「合同」从需求换成「假设」,把「验收」换成「集成时逐条核对假设」。它与 AUTOSAR 的 ARXML 合同同构:都是「下游拿着上游无法预知的自由度,用可校验的文本约束住」。**BerryOS 对应物(推断,机制映射)**:

| SEooC 件 | BerryOS 可能载体 |
|---|---|
| 假设的安全需求 | 平台安全目标文档(内核/IPC 核的失稳假设:调度护栏、失败隔离语义) |
| AoU | **manifest/槽位合同的机器字段**(时序上限=调度参数包络;FFI 保护=服务隔离假设;能力签名=「集成者必须提供/不得提供」的 handle 集)——**能力模型(handle+rights)是 AoU 的天然机器可校验载体**(推断,开放问题 §7-Q4) |
| 能力主张 | BCTS 断言面:「本内核在以下护栏下支撑 X 类负载」的测试化表达 |
| 集成方确认 | OEM 侧 BCTS + 度量链对账:假设字段进镜像清单,组装期机器校验「AoU 全部核对」 |

### 3.3 QNX 的 SEooC 实践:认证作为产品形态

QNX 把上述机制做成了明码标价的产品形态(qnx.software 官方页,一手):

- **官方定位原文**:「**Aligns to SEooC Design — As a Safety Element out of Context (SEooC), the QOS can be integrated into a safety-critical system as a foundational building block.**」
- **认证包内容**:迁移 FAQ——「a migration experience from QNX SDP 8.0 to QOS 8.0 mainly involves familiarizing oneself with the knowledge documented in **the key safety artifacts delivered with the QOS, including the Safety and Security Manuals**」→ 认证变体随附**安全手册+安全手册级文档包**;再加可下载的 TÜV 证书(certifications 页给 TÜV Assurance 公开证书链接)、TCL3/T3 资质工具链。
- **口号即商业模式**:「**Reliably Certify Your System, Not Your Toolchains**——You'll need to certify only the parts of the system you build, not the OS you build it on or the toolchains you use to build it.」——客户只认证自己的部分,OS 与工具链的认证成本由平台方预付。
- **认证变体与商业版二进制兼容**:「QNX SDP 8.0 and QOS 8.0 are **completely binary compatible**」+「Transition designs into different SOCs or hardware architectures **without recompiling and recertifying** the underlying software components」——认证变体=同源二进制+安全默认值+文档包,不是代码分叉(03 号 §2.2-4 的判断在此得到原文级确认)。
- **认证矩阵**(certifications 页,证书可点):预认证=QNX OS for Safety/QNX Hypervisor for Safety/Advanced Virtualization Frameworks ×(ISO 26262 ASIL D、IEC 61508 SIL 3、IEC 62304 Class C、ISO/SAE 21434);MotionWise Schedule(TTTech 第三方调度引擎)对 QOS 8.0 另持 TÜV Nord ASIL B+21434 证书——**第三方组件在认证 OS 上再认证**的层叠案例;**可认证(certifiable)档**:IEC 62061(机械安全)、ISO 25119(农林机械)、IEC 61513(核电)、ISO 13849-1:2015(机械安全 SRP/CS)——**四个工业/非汽车标准,正是 v2 工业 RT 的入口谱系**(§6.3)。

---

## §4 治理面:会员制、Release 节奏、标准分档与「标准即接口」

### 4.1 组织:三级决策 + 工作组

autosar.org Organization 页(一手):**Executive Board**(Core Partner 高管,定战略路线图,约每年一次)→ **Steering Committee**(Core Partner 技术管理层,管战略/伙伴准入/公关/对外信息,约每六周)→ **Project Leader Team(PLT)**(Core Partner + Premium Partner Plus 的项目经理层,对标准开发全面负责:项目计划/预算/程序规则/设组/技术裁决,约每六周)。**Working Groups** 成员=Core/Premium/Development Partner + Attendee;任务清单原文含:「Specify the AUTOSAR RTE… / Define requirements… basic software modules and automotive operating systems / **Define methodology and data exchange formats** / Define standardized interfaces across the different vehicle domains / **Define acceptance tests** / Integrate an exemplary AUTOSAR software implementation for the Adaptive Platform」。另有 User Groups(基于已发布文档做专题)、Chairperson(九个月任期)、Spokesperson(对外)。

**推断**:注意 WG 任务清单就是一份「平台工程的六件事」:运行时接口/BSW+OS/方法论与交换格式/跨域应用接口/验收测试/参考实现——BerryOS 对应:wire 协议/服务+内核/API 治理与清单 schema/槽位合同/BCTS/官方服务实现。**六件事一一有主**就是治理的最小闭环。

### 4.2 会员分档:权利与价钱全部公开

partners 页(一手;Wikipedia 互证,其行文称「六档」但列了七项,以官网表七档为准——口径差注明):

| 档 | 定位 | 年行政费 | 年人力贡献 | 权利(license/进组/带组/进 PLT) |
|---|---|---|---|---|
| Core Partner | 联盟所有人(2026 起 10 家:AUMOVIO、BMW、Bosch、DENSO、GM、Huawei、Mercedes-Benz、Toyota、Vector、VW) | — | — | 全部(EB/SC/PLT 席位) |
| Premium Partner Plus | 驱动标准化的市场领导者 | 90,000€ | **5 FTE + 1 FTE(PLT)** | license ✓ 进组 ✓ 带组 ✓ 进 PLT ✓ |
| Premium Partner | 设计并使用标准 | 31,000€ | 1.5 FTE | license ✓ 进组 ✓ 带组 ✓(不进 PLT) |
| Development Partner | 小公司/初创(<100 人) | 10,000€ | 0.5 FTE | license ✓ 进组 ✓ |
| Associate Partner | 跟随使用者 | 21,000€ | 无 | license ✓ |
| Associate Partner [Light] | 只用选定份额(总线兼容互操作件) | 免费 | 无 | license(限范围)✓ |
| Attendee | 大学/非营利 | 免费 | 个案约定 | 参与 |
| Subscriber | 个人 | 3,000€ | 无 | 列席 WG 会议 |

两处关键事实:①**标准文档公开下载但仅限「INFORMATION ONLY」,商业利用需入伙伴关系拿 license**(每个标准页横幅原文);②**2026 年 1 月新增三家 Core Partner:DENSO、Huawei、Vector**(history 页)——Tier1(电装)、ICT 巨头(华为)与**工具/BSW 软件商(Vector)升格为联盟所有人**,「实现方进入立法层」的信号。会员规模:官网称「350+ partners」,Wikipedia 记「309 Companies (02/2026)」——**两源不一致,待核实**。另:Vector 自家产品页仍自称「AUTOSAR Premium Partner」(与 2026-01 升格新闻时差,以 history 页为准,**待核实**)。

### 4.3 Release 节奏:从「三年相」到年度流水线

history 页 + Wikipedia 两源拼出的时间线(交集处为两源一致):

| 时期 | 事件 |
|---|---|
| 2002-08 | BMW/Bosch/Continental/DaimlerChrysler/VW(+Siemens VDO)启动 |
| 2003-07 | Classic 架构起草、Core Partner 签开发协议;2003-09 VDI 会议官宣;2003-11 Ford、2003-12 PSA/Toyota、2004-11 GM 加入 |
| **Phase I(2004–2006)** | 基础开发:1.0(2005-05)/2.0(2006-05)/2.1(2006-11) |
| **Phase II(2007–2009)** | 架构+方法论扩展:3.0(2007-12)/3.1(2008-08)/4.0(2009-12) |
| **Phase III(2010–2013)** | 维护+选择性改进:3.2(2011-05)/4.1(2013-03) |
| 2013 | 转入 continuous 工作模式;**验收测试 Release 1.0(2014-06)/1.1(2015-10)** |
| 2016 | **Foundation 1.0.0 与 CP 4.3.0 分离发布**(2016-10);AP 开工 |
| 2017-03 | **AP 首个 Release 2017-03**;同页 2017 记「Start Adaptive Platform, Stabilization Classic Platform」+开发 AP 示例软件(Wikipedia 另记 2017-10 有 17-10 版,**history 页未列,待核实**) |
| 2018 | AP 18-03(2018-03)/18-10(2018-10);CP 4.4.0 + Foundation 1.5.0(2018-10) |
| 2019-11 起 | **改为年度 RYY-11**:19-11/20-11/21-11/22-11/23-11/24-11/25-11 三平台(CP/AP/Foundation)同步发,每年 11 月(25-11 实际 12 月初官宣) |

**推断**:任务书所谓「三年节奏」是 Phase 时代的大版本相(1.0→4.0 各阶段约 2-4 年);真正值得移植的规律是**「重标准→轻维护→年度流水线」的节奏演化**与**「发布物从单一标准长成三平台+验收测试+参考实现」的工件谱系**——BerryOS 的对应问题:工件的「版本化单元」是什么(内核?wire?槽位合同?BCTS?),以及是否需要跟版窗口纪律(04 号 §4.5-4 的 N 值开放问题)。

### 4.4 「标准即接口」:多家工具商在同一份合同上竞争

**竞争同一合同的证据链(全部一手)**:

- **同一合同**:所有工具吃同一 ARXML schema(§1.2);dSPACE SystemDesk 产品目录明文:「**SystemDesk can be connected to any AUTOSAR-compliant basic software configuration tool** for generating and configuring basic software modules such as OS and COM」——系统建模工具(dSPACE)与 BSW 配置工具(EB/Vector)是**不同厂商的不同环节**,接口就是 AUTOSAR 格式本身。
- **跨厂商工具链是常态**:dSPACE×EB 官方应用说明(EB 站 PDF + digitalengineering247 报道两源):「SystemDesk for modeling the architecture of the application software, TargetLink for generating code for the application software, and EB tresos Studio for configuring and generating the basic software as well as for RTE… **Data exchange between the tools is based on AUTOSAR XML files**」。
- **同槽位多实现竞争**:BSW 实现有 Vector MICROSAR(「production-ready AUTOSAR Classic platform used by OEMs and Tier-1 suppliers worldwide, that goes beyond the standard」)vs EB tresos AutoCore(EB 产品线)vs 芯片厂配套——同一 SWS 合同下的商品竞争;MICROSAR 的差异化卖点是标准外扩展:「Project-Specific Extensions: OEM-specific modules and additional functionality beyond standard scope」+ ISO 26262 ASIL D/Automotive SPICE/ISO 21434 认证证书可下载(vector.com)。
- **工具生态的工程现实**:「AUTOSAR files are usually stored centrally in a version control system, such as git」+ 专门的 **dSPACE AUTOSAR Compare** 差异比较工具(SystemDesk 页)——配置合同进版本管理、合同 diff 成独立产品,这是「合同即代码资产」的成熟度标志。
- **版本跟上的义务**:「SystemDesk 2025-B supports the latest AUTOSAR Release R24-11 for the AUTOSAR Classic Platform and AUTOSAR Adaptive Platform」(SystemDesk 页)——工具商逐版追标准。
- **开源补充**:Artop(AUTOSAR 工具平台,跨标准 WG 下设,autosar.org Working Groups 栏目在列;artop.org 本稿抓取失败,细节**待核实**)。

**对照 BCTS 与槽位合同(推断)**:AUTOSAR 证明「标准即接口」成立的三个前提——①合同**机器可校验且公开**(schema+语义约束);②**验收测试由中立方(联盟)维护**,不由任一工具商维护;③**参考工件/参考实现**让新进者可对表。BerryOS 现状:①的对应(abi.toml 五面/mkapp 门禁)只在仓内;②③完全缺位(01 号缺口 4)。**BCTS 立项时若把「槽位合同的 conformance 测试集」当作与合同同版本的工件发布,就同时拿到②③**——这也是 Android CTS(Fuchsia CTS 同)与 AUTOSAR 验收测试殊途同归之处(03 号 §1.5、02 号 §3.1)。

### 4.5 AUTOSAR 的「开放边界」:非汽车应用条款(2016 起)

「Openness for non-Automotive Applications has existed since 2016. It allows applicants and AUTOSAR partners to benefit from over 20 years of embedded software development with focus on functional safety and cybersecurity. This applies to applications derived from automotive applications as well as all others that are not in an ultra-hazardous-category.」示例领域明文列:移动基础设施、农业机械、海运、**铁路**、城市出行、**工业自动化**、楼宇自动化、家电、医疗技术;同时声明该清单不能豁免监管法/进出口/军贸等法律责任,有约束力的表述在开发协议内(autosar.org Openness 页,一手;Wikipedia 组织节互证大方向)。

**推断**:这是「汽车标准溢出到工业」的官方通道——BerryOS v2 从移动跳到工业 RT 是反向路径,但机制同款:**用一套冻结合同+安全论证覆盖第二个行业,而不是为第二行业 fork 第二套平台**。BerryOS 对应物:「同一内核/服务/wire,工业档=配置面档位+安全假设清单换册+认证变体」,而非「工业版 BerryOS」(开放问题 §7-Q6)。

---

## §5 QNX / INTEGRITY 在此范式中的位置(补 03 号 §2)

### 5.1 QNX:认证 OS 作为 SEooC 商品 + 第三方平台件的宿主

03 号 §2 已给「内核二进制×BSP」「认证变体而非代码分叉」的框架;本稿补三块新证据(qnx.software 一手):

1. **认证包怎么卖**:见 §3.3——预认证证书(TÜV Rheinland,公开可查)+ Safety/Security Manuals 文档包 + TCL3/T3 工具链资质 + 「只认证你写的部分」责任切分。认证的商业本质=**平台方预付认证成本,OEM 按件摊销**。
2. **平台件层叠**:MotionWise Schedule(TTTech 的调度引擎)对 QOS 8.0 单独认证(TÜV Nord,ASIL B + ISO 21434)——认证 OS 之上再跑第三方认证平台件,安全论证逐层叠加;Vector 侧则有 MICROSAR OS「可作为 Linux 与 QNX 的 GuestOS」(Vector 产品信息 PDF 摘录,**待核实**)与 **Alloy Kore**(「jointly developed foundational software platform by Vector and QNX」,vector.com 产品目录)——AUTOSAR 系工具商与认证 OS 厂合资做 SDV 基础软件包。
3. **工业谱系**:QNX 可认证面含 IEC 62061/ISO 25119/IEC 61513/ISO 13849-1(§3.3),另 POSIX PSE52(1003.13-2003)完全符合、FACE 航空符合性、CC EAL4+(certifications 页)——一个二进制覆盖汽车+医疗+机械+核电+轨交的可论证复用。

### 5.2 Green Hills INTEGRITY:分离内核 + 两段板级交付

ghs.com INTEGRITY 页(一手):

- **形态**:分离内核(separation kernel)微内核架构;**保证资源**——「INTEGRITY can assign **fixed budgets of CPU time and memory** to each process」「bounding kernel service times regardless of any actions performed by the application」;内存配额制、内核内存不用于进程请求的对象(防内核内存耗尽);AMP/SMP/BMP 多核模式;安全虚拟化(Multivisor,客户机 Linux/Android/Windows 与原生任务混跑)。
- **认证清单**:FAA DO-178B/C **DAL A**(INTEGRITY-178/178 tuMP);NSA SKPP 高鲁棒 + **Common Criteria EAL 6+**(「商业 OS 史上最高安全等级」);FDA Class II/III;工业:**EN 50128/50657 SIL 4、IEC 61508 SIL 3**;汽车:**ISO 26262 ASIL D** + ISO/SAE 21434 CAL 4 + UNECE WP.29 CSMS。
- **对 AUTOSAR 的姿态**:「standards-based support for **POSIX and AUTOSAR**」;运行时抽象层清单明列「**AUTOSAR Adaptive, AUTOSAR Classic**」(还有 Linux/Android/POSIX/Java/ROS 2)——即 INTEGRITY 不是 AUTOSAR 系玩家,而是**给 AUTOSAR 中间件当认证底座**的替代内核:标准中间件×认证内核的自由组装,正是「wire 协议(标准)×内核(实现)」分离的活样本。
- **BSP 交付(补 03 号 §2.1)**:**ASP(Architecture Support Package)= CPU 初始化/异常处理/上下文切换**;**BSP 在 ASP 之上=板级内存/中断/加速器/外设/中间件支持**;与 COTS 板卡厂商广泛合作(官方页)。两级板级抽象(CPU 档/板档)对照 QNX 的「参考 BSP 起步」(03 号 §2.1)——**板级件=平台方交付的「数据+薄胶水」,不是 OEM 改内核的入口**。
- 按行业打包的「integrated platforms」:Automotive/Secure IoT/Avionics/Industrial safety/Medical——**同一内核按行业出集成平台包**,与 QNX 认证矩阵同思路。

### 5.3 本范式对 BerryOS 的定位含义(推断)

功能安全世界的 OS 商业形态可以概括为:**「认证内核(SEooC)+ 安全手册(假设清单)+ 认证变体(同源二进制)+ 板级件(ASP/BSP)+ 按行业平台包」五件套**。BerryOS 若进 v2 工业 RT,这五件套就是竞品的完整形态;其中 BerryOS 已有的结构性优势是内核极简+失败隔离+能力模型(论证面小),最大的洞是**认证信用为零**(没有任何 TÜV 类第三方背书)与**安全文档体系不存在**(§6.3)。

---

## §6 收口:AUTOSAR 范式对 BerryOS 的可移植机制清单与四层映射

### 6.1 可移植机制清单(每条给 AUTOSAR 出处,是否采纳归 ADR)

1. **配置合同三件套**:交换格式 schema(机器校验语法)+ 语义约束随格式同发 + 工具中立生成器(§1.2,Classic 页 Methodology 节)。→ 对应 04 号 §2.1 产品清单 schema 进 L2 + 未知键硬拒。
2. **方法论分层数据流**:系统级描述→按 ECU 切分提取→本地配置生成(§1.2)。→ 对应 berry-assemble:平台工件→产品清单→镜像;「提取」一步是 BerryOS 现在缺的抽象(镜像级直达)。
3. **平台/增值分界立法句**:「RTE 之上=OEM 竞争区,RTE 之下=标准化 OEM 无关区」(§1.1)。→ BerryOS 版:「wire 协议之上=OEM 竞争区,L0/L1/能力模型=OEM 无关区」——04 号 P2 正交公理的文本化。
4. **槽位即标准(SWS 文档+验收测试)**:每个 BSW 模块一份 SWS 规格,AUTOSAR 维护验收测试(WG 任务原文+验收测试 Release,§4.1/§4.3)。→ 槽位合同=wire 契约+conformance 测试,合同与测试**同版本发布**(BCTS)。
5. **参考实现证明规范可实现**(AP exemplary implementation/CAPI,§2.3)。→ 官方服务实现=参考实现,host 单测=可实施性测试种子;需要的是「作为标准工件发布」的姿态(04 号 §2.4 执法外化)。
6. **官方中立工作流示例工件**(Workflow Example Iteration 01,多工具链评估中立性,§1.2)。→ 「BerryOS 产品组装示例包」:用官方工件+第三方替换件各跑通一遍组装,证明合同不偏向官方实现。
7. **SEooC 假设清单机制**:assumed safety requirements+AoU+capability claim 三件交付,集成方 gap analysis+逐条校验+DIA(§3.2)。→ BerryOS 安全合同:护栏参数(调度包络/隔离假设)进 manifest 字段,BCTS 断言化(开放问题 Q4)。
8. **认证变体而非代码分叉**:SDP↔QOS 二进制兼容+安全手册迁移(§3.3/§5.1,03 号 §2.2-4)。→ 未来 berryos-safety 变体:同源二进制+默认值收紧+文档包。
9. **「只认证你写的部分」责任切分**(平台预付 OS/工具链认证,§3.3)。→ BerryOS 若认证,认证边界=内核+IPC 核(SEooC 边界最小化,OEM 件自证)。
10. **Foundation 档:跨平台公共合同单列**(2016 起 Foundation 独立成档:公共需求+协议+方法论,§1.2/§4.3)。→ BerryOS 的「Foundation 档」候选:manifest schema/签名链/度量 TLV/abi.toml——跨产品形态稳定的合同单列冻结(开放问题 Q8)。
11. **应用接口标准化**:跨整车域的标准化应用接口是 AUTOSAR 独有重资产(Classic 页 Application Interfaces 栏目+WG 任务;Vehicle Services/Automotive API for AP)。→ BerryOS 对应:系统应用间的 wire 词典(时钟/设置/通知类跨应用协议)——目前全靠单服务协议 crate,无「跨域应用接口」层(推断:可缓,但生态变大后会出现)。
12. **非汽车开放条款**:单一标准+假设清单复用到第二行业(§4.5)。→ 工业 RT 档=配置面档位+安全册替换,不 fork 平台。
13. **配置进版本管理+合同 diff 工具化**(dSPACE AUTOSAR Compare,§4.4)。→ 产品清单/槽位合同的可 diff 性作为设计要求(schema 稳定、规范形重序列化——mkapp 已有此纪律,`tools/mkapp.py`)。
14. **验收测试独立版本化**(Acceptance Test Release 与标准版本并行发,§4.3)。→ BCTS 与 BerryOS 版本绑定关系立法(04 号 §7-Q4)。

### 6.2 与 04 号四层模型的映射表

| AUTOSAR 机制 | BerryOS 四层归属 | 04 号对应位置 | 现役仓库对应/缺口 |
|---|---|---|---|
| 方法论三工件+ARXML schema | 配置面 | §2.1 产品清单(platform/product 双键) | 缺口 1:配置层不存在(justfile 硬编,01 号 §3) |
| SWS 槽位规格+验收测试 | 替换面 | §2.2 契约三件套(能力/wire/ABI)+conformance | 架构可(wire add-only)、工程无门(缺口 2/4) |
| SWC/CDD 扩展+应用接口标准化 | 扩展面 | §2.3 扩展包+上游化政策 | .bap 轨道在;APP_SLOTS=9 帽+devmgr v2(缺口 5/8) |
| 规范文本/RTE 语义/schema 本身 | 禁改面 | §2.4(L0 永冻/L1 add-only/能力模型) | 执法在仓未外化(缺口 4) |
| Foundation 档(公共合同单列) | 禁改面(子档) | §2.4 建议单列 | 未立法(开放问题 Q8) |
| SEooC 假设清单/安全手册 | 横切(配置面字段+BCTS 断言) | §4 BCTS+未来安全立法 | 不存在;能力模型可承载(推断) |
| 认证变体 | 禁改面变体(非定制面) | §6.2 v2+ 件(03 号 §2.2-4 同构) | 不存在 |
| 验收测试 Release | BCTS | §4 全节 | 种子在(smoke/abi-check),非认证面 |

### 6.3 v2 工业 RT:继承什么、抛弃什么

**继承(机制级)**:

1. **合同哲学全套**:schema 化配置、槽位即标准、验收测试、参考实现、中立示例工件(§6.1-1/2/4/5/6)——全部是「把承诺做成机器可校验工件」的具体形态。
2. **SEooC 路径**:平台方按假设认证、OEM 逐条核对继承(§3.2/§3.3)。BerryOS 的增量机会:**用 manifest 字段+能力签名+BCTS 断言把纸面假设清单机器化**——AoU 写成「必须持 X 能力/时序在包络内」的字段,组装期硬校验(推断;AUTOSAR/QNX 都做不到这一步,因为它们没有能力模型)。
3. **认证变体+二进制兼容+「只认证你写的部分」**(§3.3/§5.1)。
4. **工业标准入口谱系**:QNX/INTEGRITY 的可认证面(IEC 62061/ISO 13849-1/IEC 61508 SIL 3/EN 50128)证明工业 RT 的认证地标不是 ASIL D 一条路——**机械安全(ISO 13849 PL d / IEC 62061 SIL 2)与过程工业(IEC 61508 SIL 2/3)档位比 ASIL D 轻**,可能是 BerryOS 第一认证的合理落点(推断,开放问题 Q10)。
5. **分离内核的资源保证思路**:CPU/内存固定预算、有界内核服务时间(INTEGRITY,§5.2)——与 BerryOS 调度护栏(动作空间四类封顶+±64 夹紧,01 号 §1.1)同向;工业档可把「预算保证」升为合同语义(推断)。

**抛弃(或轻量化)**:

1. **ARXML 全量交换格式的重量**:schema 覆盖整车系统/通信/诊断/标定,复杂到「工具即门槛」——需要专门配置工程师与工具链。BerryOS 单平台方+轻生态,产品清单应保持 Fuchsia 式轻量(枚举+开关+参数),**不为想象中的跨组织交换预付复杂度**(推断;真出现工具对工具需求再长,合同可加不可减——add-only 纪律本来就支持后长)。
2. **联盟式治理流程**:会费/FTE/三级委员会/全员表决是 300+ 家公司的协调成本;BerryOS 是平台方立法,决策面应是「公开合同+单向演进窗口」而非会员共治(推断;与 04 号 §3.4「治理题无先例可抄」一致)。
3. **全员年度同步重发版的节奏税**:RYY-11 三平台同步意味着生态每年集体迁移;BerryOS 生态小是优势,**按合同各自版本化(L1 per-interface version 已立法)+顶层慢节奏**即可(推断)。
4. **OSEK 静态配置形态**:Classic 全静态(配置生成代码、无运行装载)是 MCU 时代遗产,AP 自己都已转向 POSIX+服务化;BerryOS 用户态服务+动态装载路线不用回头(推断)。
5. **纸面安全手册宇宙**:ISO 26262 的文档量是认证行业的真实成本(QNX/GHS 用数十年与专职团队支付);BerryOS 应把安全论证**尽量代码化/测试化**,文档只承载无法机器化的部分(推断;这条本身有认证风险——评估机构接受度未验证,开放问题 Q10)。

**诚实代价声明(推断)**:重流程对轻生态的代价不可忽略——①认证信用只能靠时间与第三方买,QNX 的 45 年/INTEGRITY 的 25+ 年是护城河,BerryOS 从零起步;②假设清单机制的前提是**平台方先把安全论证做出来**, BerryOS 目前连安全目标文档都没有(仓库无此类档,01 号 §5 类似空白的又一例);③工业客户问的第一句是「谁认证过你」,技术架构优势排在信用之后——**v2 工业 RT 的瓶颈大概率不是架构,是认证策略与证据积累的启动时点**。

---

## §7 开放问题(原样列出,不替拍板)

1. **配置合同的作用域**:BerryOS 产品清单走「平台↔OEM 组装单」(Fuchsia 式)还是预留「工具↔工具交换格式」(ARXML 式)?若后者,谁可能成为 BerryOS 生态的「第三方工具商」(ODM?芯片厂?),现在要不要为其留 schema 中立位?
2. **「ECU 提取」式中间层**:平台工件→产品镜像之间是否需要显式的「按设备提取」工件(多设备产品线共享一份系统级清单)?
3. **BCTS 是否独立版本化**:验收测试与标准版本绑定(AUTOSAR Acceptance Test Release 模式)还是随仓滚动?发布物形态(套件+证书清单+跟版窗口)如何定(接 04 号 §7-Q4)?
4. **AoU 的机器化**:假设清单进 manifest 字段+能力签名承载,具体哪些假设可机器化(时序包络/隔离/能力持有),哪些必须留在文档?谁校验(组装工具/BCTS/boot 对账)?
5. **CDD 式明面逃生舱**:BerryOS 是否给「不合标准形态的厂商件」留一个 AUTOSAR CDD 式的官方逃生区(带义务与边界),还是坚持一切走 .bap 轨道(04 号 §2.3 上游化政策的严格版)?
6. **工业档的形态**:v2 工业 RT 是配置面档位(同一平台换册换默认值,AUTOSAR Openness 模式)还是独立产品线?对应 IEC/ISO 标准谱系先攻哪一档(ISO 13849 PL d?IEC 61508 SIL 2?ASIL D 后置?)?
7. **参考实现的中立性**:官方服务实现兼任「合同参考实现」时,如何避免「合同被官方实现私有行为污染」(Hyrum 定律反向)——BCTS 用例只测承诺面的纪律如何贯彻到参考实现自身?
8. **Foundation 档单列**:manifest schema/abi.toml/度量 TLV/签名链 schema 是否单列「跨形态公共合同档」独立冻结,先于产品形态分化?
9. **槽位合同的「跨域应用接口」层**:系统应用间协作协议(设置/时钟/通知词典)是否需要 AUTOSAR Application Interfaces 式的标准化层,还是留给生态约定?
10. **认证启动时点与 SEooC 边界**:若走认证,认证边界划在哪(内核?内核+IPC?内核+IPC+关键服务?);文档体系与代码化论证的比例;评估机构对「测试即证据」的接受度(**待核实,需与认证机构对话**)。
11. **第三方在认证 OS 上的层叠**(MotionWise 模式):未来 BerryOS 上是否允许/鼓励第三方认证件(如 OEM 调度策略服务过功能安全认证)?槽位合同的语义合同是否要预留安全语义档?
12. **工具置信(TCL)的 BerryOS 版**:mkapp/abi-check/berry-assemble 这些「执法工具」自身要不要置信声明(它们错了谁负责)?AUTOSAR/ISO 的工具置信思路是否引入 CI 治理?

---

## 来源索引

### 仓库内文档(相对路径+行号,2026-11-10 采集时点)

- `AGENTS.md`:11(v1 移动 rich 档/v2 攻工业 RT)、19(syscall v1 37 在册 C-ABI)
- `research/厂商定制化深研/01-现状盘点-BerryOS定制基础与缺口.md`:27-32(调度四层接缝)、94-111(冻结面表)、115-132(12 项缺口表)、155-157(冻下三路开上三路总评)
- `research/厂商定制化深研/02-安卓OEM定制教训-碎片化机制根因.md`:§3.1 CTS(经 04 号 §4.1 转引)、§3.7(经 04 号 §3.3 转引)
- `research/厂商定制化深研/03-正面模式全景-定制范式对比.md`:39-55(Fuchsia product assembly)、95-122(QNX 章:2.1 BSP/2.2 认证变体/四元组)、286-293(五条共性)、296-313(可移植机制清单)、330-397(外部 URL 索引)
- `research/厂商定制化深研/04-BerryOS厂商定制化架构提案.md`:52-59(四层总览)、61-75(配置面)、77-95(替换面)、97-111(扩展面)、112-131(禁改面+总表)、189-244(BCTS)、300-315(开放问题 13 条)、318-329(反面自检)
- `research/厂商定制化深研/README.md`(系列定位与消费去向)
- 经 01/03/04 号转引未直验的仓库文件:`docs/驱动模型设计-v1.0.md` §1/§5、`docs/应用规范-v1.0.md` §11、`docs/SDK-2-分发与签名链设计-v1.1.md`、`docs/更新与OTA设计-v1.0.md`、`justfile`、`user/berry-manifest/src/lib.rs`(原始行号见各稿来源索引)

### AUTOSAR 官方(autosar.org,一手,2026-11-10 快照)

- Classic Platform(三层架构/RTE 分界原文/BSW 模块清单/方法论与 Templates/.arxml/Workflow Example):https://www.autosar.org/standards/classic-platform
- Adaptive Platform(ARA/PSE51/功能集群清单/EM 三组件/ara::com 开放架构/ara::diag UDS+SOVD/UCM/V-UCM/参考实现自述/Automotive API R24-11 起/当前 Release R25-11):https://www.autosar.org/standards/adaptive-platform
- Foundation(定位:跨平台互操作性/公共需求与协议):https://www.autosar.org/standards/foundation
- History(2002-2026 时间线:Phase 各版日期/验收测试 1.0/1.1/Foundation 分离/AP 17-03/年度 RYY-11/2026-01 新 Core Partner DENSO+Huawei+Vector):https://www.autosar.org/about/history
- Partners(Core Partner 十家清单/七档会员表:费用+FTE+权利/OEM 与供应商权益原文):https://www.autosar.org/about/partners
- Organization(EB/SC/PLT/WG 职责;WG 六任务含 acceptance tests 与参考实现):https://www.autosar.org/about/organization
- Openness for non-Automotive Applications(2016 起/九领域清单/法律边界):https://www.autosar.org/about/openness-for-non-automotive-applications
- CAPI(Common Adaptive Platform Implementation 栏目,参考实现成品化):https://www.autosar.org/capi
- AUTOSAR_TPS_ECUConfiguration.pdf(R22-11;「ECU 配置起始于系统描述切分…Ecu Extract 见 System Template」摘录经搜索索引,PDF 未逐条核):https://www.autosar.org/fileadmin/standards/R22-11/CP/AUTOSAR_TPS_ECUConfiguration.pdf

### 工具商/BSW 厂商(一手)

- Vector MICROSAR Classic(产品定位/OEM generic vs customer-specific/ASIL D+ASPICE+21434 认证/格式支持 AUTOSAR-XML/DBC/LDF/FIBEX/CDD/ODX/DaVinci 工具族):https://www.vector.com/en/product/microsar-classic/
- Vector 产品目录(197 件;Alloy Kore=Vector×QNX):https://www.vector.com/en/product/
- DaVinci Configurator Classic 手册(配置/校验/生成 BSW+RTE):https://help.vector.com/davinci-configurator-classic/en/latest/user-manual/index.html
- MICROSAR 产品信息 PDF(OSEK 扩展/多核/GuestOS/SC 分级;日文版,经搜索索引摘录,**待核实**):https://cdn.vector.com/cms/content/products/microsar/Docs/MICROSAR_ProductInformation_JP.pdf
- Elektrobit 产品导航(tresos AutoCore/Studio/Integration-as-code/Safety Fail-operational/Embedded Hypervisor/ACO Bootloader/Rust 支持;corbos Linux/AdaptiveCore/Studio;评估包落 Infineon/NXP/Renesas/Synopsys/dSPACE 硬件):https://www.elektrobit.com/products/ecu/eb-tresos/bsw/ 、https://www.elektrobit.com/products/ecu/eb-tresos/studio/
- dSPACE SystemDesk(SIL/V-ECU 三级/AUTOSAR 建模/FMI-FMU/ARXML 进 git/AUTOSAR Compare/2025-B 支持 R24-11):https://www.dspace.com/en/ltd/home/products/sw/system_architecture_software/systemdesk.cfm
- dSPACE×EB×TargetLink 应用说明(跨厂商工具链;ARXML 交换;SystemDesk→「any AUTOSAR-compliant basic software configuration tool」):https://d2368tcediwknr.cloudfront.net/bkm/systemdesk_en/catalogs/systemdesk_en/pdf/complete.pdf 、https://d23rjziej2pu9i.cloudfront.net/wp-content/uploads/2014/11/28025450/SystemDesk_EB_tresos_Studio_TargetLink_Application_Note_EN.pdf 、https://www.digitalengineering247.com/article/dspace-and-eb-create-autosar-development-environment-by-integrating-tools/
- TI 侧 MICROSAR 互证(AUTOSAR classic+adaptive/ISO 26262 ASIL D):https://www.ti.com/tool/VCTR-3P-MICROSAR

### QNX / Green Hills(一手)

- QNX OS and OS for Safety(SEooC 定位原文/二进制兼容迁移/Safety & Security Manuals/TCL3-T3/QFS ASIL B/MotionWise):https://qnx.software/en/software/products-and-solutions/qnx-os-and-os-for-safety
- QNX Safety Certifications(认证矩阵+TÜV 公开证书链接;certifiable 档 IEC 62061/ISO 25119/IEC 61513/ISO 13849-1;PSE52/FACE/EAL4+):https://qnx.software/en/developers/developer-roadmap/certifications-and-compliance
- Green Hills INTEGRITY(分离内核/资源预算/EAL6+/DO-178C DAL A/EN 50128 SIL4/IEC 61508 SIL3/ISO 26262 ASIL D/ASP 与 BSP 两段/AUTOSAR Classic+Adaptive 抽象层):https://www.ghs.com/products/rtos/integrity.html

### ISO 26262 / SEooC

- ISO 26262-10:2018 目录页(官方存在性;正文付费,抓取被拒):https://www.iso.org/standard/68392.html
- Wikipedia: ISO 26262(12 部结构/2011 vs 2018/item 与词汇/HARA 的 S-E-C 与 ASIL 组合/Part 8/Part 9——二手汇总,条款级细节标待核实):https://en.wikipedia.org/wiki/ISO_26262
- Wikipedia: AUTOSAR(方法论三工件/Phase I-III/会员与 Core Partners 309 家 02-2026/OSEK/PSE51/功能集群):https://en.wikipedia.org/wiki/AUTOSAR
- iso26262.academy SEooC 概念页(SEooC 定义/Part 10 §9/AoU 定义与可校验性/安全手册八件/gap analysis/DIA/FTTI 账本——独立培训方二手详源):https://iso26262.academy/features/concepts/seooc
- Intertek 博客(SEooC=供应商对缺失信息建立经论证的假设,二手):https://www.intertek.com/blog/2026/01-15-exploring-safety-elements-out-of-context/
- Embitel/piembsystech SEooC 解析(Part 10 指南/AoU+safety manual 词汇互证,二手):https://www.embitel.com/blog/embedded-blog/what-is-safety-element-out-of-context-seooc-in-automotive-functional-safety/ 、https://piembsystech.com/seooc-iso-26262/
- Renesas SEooC 博客(半导体厂商 SEooC 软件实践;正文 JS 渲染未取得,标题与摘要两源之一):https://www.renesas.com/en/blogs/renesas-functional-safety-support-automotive-3-applying-iso-26262-standard-seooc-software

### 待核实项汇总

- ISO 26262 HARA 的 S0-S3/E0-E4/C0-C3 分级表与 ASIL 组合细则:Wikipedia 单源详列,标准原文付费未核(§3.1);
- SEooC 条款号「ISO 26262-10 §9」:三处二手一致,原文未核(§3.2);
- AUTOSAR AP 17-10 Release:Wikipedia 记、官方 history 页未列(§4.3);
- AUTOSAR 会员数:官网「350+」vs Wikipedia「309(02/2026)」(§4.2);Vector 会籍口径(产品页 Premium Partner vs 2026-01 Core Partner 公告)(§4.2);
- AP 执行清单/机器清单的具体名目与字段:在 PlatformDesign/SWS 文档,本稿未逐条核(§2.2);
- ara::com 的 SOME/IP 绑定:Wikipedia 单源(§2.1);
- MICROSAR OS 的 GuestOS(Linux/QNX)与 SC 分级:Vector 日文产品信息 PDF 摘录单源(§1.1/§5.1);
- 安全手册八件清单:iso26262.academy 单源详列(§3.2);
- 认证机构对「测试即证据」的接受度:未与任何机构对话(§6.3/§7-Q10);
- Artop 开源工具平台细节:artop.org 抓取失败,仅 autosar.org WG 栏目佐证存在(§4.4)。

> **本稿边界**:不替拍板任何开放问题;不覆盖认证费率/商业合同/工具商商务关系;AUTOSAR/ISO 规格的条款级引用以各 PDF 原文为准(本稿标注「未逐条核」处不得作为立法依据);对 BerryOS 的全部映射与取舍判断均为「推断」,供 ADR 采择。
