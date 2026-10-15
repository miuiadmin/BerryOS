# openEuler 调研报告

> 调研日期 2026-10-15 · 基于公开资料与源码快照 · 许可证：发行版为多许可混合（Linux 内核 GPL-2.0；社区自研组件多为 MulanPSL-2.0，UniProton 已实证 LICENSE=MulanPSL-2.0；A-Tune 为 Mulan 协议 v2 家族（待核实））

## 0. 一句话定位

openEuler 是华为主导、2021 年捐赠给开放原子开源基金会的企业级 **Linux 发行版（宏内核）**与数字基础设施根社区——它不是 BerryOS 在内核架构上的同类（不是微内核），真正的调研价值在四条侧线：**发行版工程与社区治理、内核增强补丁方法论、AI 自调优引擎 A-Tune、混合关键性侧的 UniProton 硬实时 RTOS**。

## 1. 谁在做：组织/社区/治理/活跃度/商业背书

- **出身**：华为 2019-12 基于 EulerOS 开源，2021-11 捐赠开放原子开源基金会，是基金会「基础设施操作系统」明星项目。
- **治理**：委员会制（用户委员会、技术委员会、品牌委员会等）+ 100 余个 SIG「责任田」。实证：`gitee.com/openeuler/kernel` 由 Kernel SIG 维护（OLK-5.10 / OLK-6.6 分支）；`gitee.com/openeuler/UniProton` 属 sig-embedded。
- **版本列车（2025–2026 最新）**：24.03 LTS（2024-05-30 发布，内核 6.6）→ 25.03 / 25.09 创新版（25.09 于 2025-09-29 发布）→ 24.03 LTS SP3（2025-12-30）、SP4（2026-07-01）→ 26.09（2026-09 发布，主打 AI 原生基础设施与 EulerCopilot；内核版本待核实）。节奏 = 6 个月创新版 + 2 年一代 LTS + SP 滚动维护。2025-08 社区调整了 LTS 生命周期政策（4 年 → 2+2 延长支持模式，细节待核实）。
- **规模（社区官方口径）**：20 万+ 开发者、1800+ 企业会员、140 万+ 社区用户；装机量 2023 底破千万。
- **商业**：南向整机 OSV（华为、麒麟、统信、超聚变等）出货；IDC 口径 2024 年中国服务器 OS 新装份额约 36–37.4% 居第一（待核实）。
- **2025–2026 活跃度信号**：主干仓库仍活跃，但部分自研 crate（A-Tune、UniProton）挂出「已迁移至 AtomGit」横幅，Gitee/AtomGit 双轨。

## 2. 总体架构：内核形态、分层与组件

**宏内核**（upstream Linux + openEuler 增强）。分层：

1. **内核层**：`gitee.com/openeuler/kernel`，OLK-6.6（24.03 LTS）携带混部调度、内存分级、安全加固等增强补丁；RT 场景提供 `kernel-rt`（PREEMPT_RT 补丁集）。
2. **系统层**：systemd、iSula 容器、secPave/安全子系统、A-Tune 自调优服务。
3. **交付形态**：服务器/云/边缘发行版 + **openEuler Embedded**（Yocto 构建，`gitee.com/openeuler/yocto-meta-openeuler`，含 `recipes-kernel/`、`recipes-bsp/`、`meta-openeuler-bsp/`）+ **UniProton**（独立硬实时 RTOS，单核非微内核，单地址空间）。

即：一个宏内核主线 + 嵌入式分支 + 独立 RTOS，用「多 OS 混合部署」而非微内核化解决实时与隔离。

## 3. 核心机制深挖

### 3.1 内核增强 vs upstream（24.03 LTS，内核 6.6）

- **内存**：引入 folio（大页粒度的分配/回收管理单元，减少锁竞争与 TLB 压力，openEuler 在其上继续做冷热识别）；内存分级扩展（memory tiering）：按访问频次识别冷页，异步下放到低速内存层（PMEM/CXL 等大容量层），并控制回收带宽避免冲击前台业务——本质是把「页面放置」变成显式的层级调度问题。
- **云原生混部调度**：在线/离线业务混部——业务优先级感知（QOS，任务带业务标签参与调度决策）、**在线任务可实时抢占离线任务**（离线任务让出 CPU 的路径被加速，而非只靠 CFS 权重缓慢收敛）、抖动（jitter）抑制（压制内核后台活动——统计、RCU 回调等对在线核的干扰）、业务优先级 OOM（内存不足时按业务优先级而非纯 oom_score 选牺牲者）。这是 openEuler 内核补丁中最成体系的一块，直接对着云厂商降本增效的场景。
- **安全**：堆喷防护（堆混淆加固）；sysSentry（官方口径：已知 CPU 故障 100% 检出）；SP4 新增海光 SM4-XTS 磁盘加密与 SM4-GCM AEAD。

### 3.2 openEuler RT（软实时路线）

- **路线**：不自研实时内核，直接吃 PREEMPT_RT 上游补丁。22.03 LTS 首次引入（Industrial-Control SIG 主导，Kernel/Embedded/Yocto SIG 协同，见 `github.com/openeuler-mirror/Preempt_RT`），内核 `CONFIG_PREEMPT_RT=y`，机制 = 硬中断/软中断线程化（IRQ 变成可被高优任务抢占的 SCHED_FIFO 线程）+ 自旋锁转 rt_mutex（天然带优先级继承，破解优先级反转）+ 高精度定时器 + 最小化不可抢占临界区。代价是本地 IPC 吞吐（如 pipe）下降——官方特性解读亦承认此点。
- **分发**：24.03 LTS SP1 起 `dnf install kernel-rt` 即装即用（包级交付是亮点：用户不需要换发行版、不需要重编内核，普通内核与 RT 内核可并存按需启动）。
- **数据**：官方白皮书口径最大调度延迟从 ms 级降到 **<100μs**（待核实，未见第三方复现；社区论坛与 issue 区可见树莓派、瑞芯微等板卡上 cyclictest 实测的公开讨论，数据离散度较大）。历史上另有 Xenomai 双内核与 Jailhouse 变体（4.19 时代「创新项目总览」白皮书），主线已收敛到 PREEMPT_RT——**双内核路线被 openEuler 试过后放弃**，这个决策本身对 BerryOS 很有信息量。

### 3.3 混合关键性（openEuler Embedded / MCS / MICA）

官方方法论一句话：**「混合关键性系统 = 部署 + 隔离 + 调度」**（2022-06 官方博客）——先解决多个 OS 二进制怎么放到一颗 SoC 上（部署），再解决它们互相不被拖垮（隔离，硬件资源分区），最后解决跨 OS 的任务协同（调度）。落地为 MICA 框架（23.09 起）：基于 **OpenAMP** 标准件——remoteproc 负责 Linux 主核对远端核固件的加载/启动/停止（生命周期管理），rpmsg 基于 virtio 提供核间消息通道（对开发者呈现为字符设备/端点），另保留 Jailhouse 虚拟机管理器路线（Linux 静态划分硬件给裸机实时系统）；`yocto-meta-openeuler` 一键构建出 Linux + Zephyr/UniProton 同片多 OS 镜像，QEMU 与真硬件均支持。关键认知：**openEuler 选择「多 OS 拼装」而不是把实时性做进 Linux 内核深处**，与 BerryOS 微内核单系统路线是两种不同的解法，但面对的是同一个问题。

### 3.4 A-Tune（AI 性能自调优引擎，深挖）

**形态**：AI 在**用户态**，内核只被动提供指标与参数接口。三服务：`atuned`（采集守护）、`atune-rest`（API）、`atune-engine`（决策引擎）。两种模式：在线静态（`atune-adm list/profile/analysis` 按场景切 profile）+ 离线动态（`atune-adm tuning`，贝叶斯/黑箱优化搜索参数）。三层架构：智能决策层（感知+决策）→ 系统画像层 → 交互系统层。工作流程是「管理员发现问题 → atune-adm 触发 → 引擎采集分类 → 出参数」，人的参与度和触发粒度都停留在运维节奏，而非运行时节奏。

**闭环机制**：采集 60+ 系统指标（CPU/内存/IO/网络）→ RandomForest 场景分类（10+ workload：数据库/大数据/中间件/web 等）→ 匹配或搜索出的参数写入配置持久化（如 `/etc/atuned/engine.cnf` 中 `backlog.ack_backlog = 4096`）→ 应用 profile。**模型与精度公开**：仓库 `analysis/models` + `generate_models.py`，依赖 xgboost/scikit-learn/scikit-optimize/pandas；README 精度表：offcpu_1 分类精度 0.9879、memload_1 ≈0.98。

**2.0 演进**：单内核 tuning 模块拆成 engine/client 两层，新增 anomaly_detection 独立模块（异常检测与调优解耦）。

**现状（2025–2026）**：仓库约一年无实质提交、迁 AtomGit（401 star/1124 commits/79 贡献者；Python 42.5%/Go 39.8%/Shell 16.7%）；社区重心转向 EulerCopilot 与 Agent Infra（Agent + Skills 的 LLM 架构）。

## 4. 源码走读：目录结构、关键文件、关键数据结构

### UniProton（`gitee.com/openeuler/UniProton`，已迁 AtomGit；MulanPSL-2.0；C 88.8%；730 commits/55 贡献者）

```
build/  cmake/  demos/  doc/  platform/  testsuites/
src/
  arch/ component/ config/ core/ drivers/ extended/ fs/
  host/log/ include/{posix,uapi}/ kal/ libc/ mem/ net/
  om/ osal/ security/ shell/ utility/
src/core/
  extend/ ipc/ kernel/
src/core/kernel/
  include/ irq/ kexc/ module/ sched/ spinlock/ sys/ task/ tick/ timer/
```

关键实证（提交记录即证据）：`!486 task: support task periodic execution`（任务周期执行）、`sched: add Round-Robin policy`（在抢占优先级之外补 RR 策略）、`UniProton: adapt tickless`（tick/ 与 timer/ 目录）、`UniProton: adapt smp`、perf 支持、`coredump: add coredump support`（!483）、`script: Add ARMv7-R Support`、`script: Add Function PRT_ClkCycle2Ns`（对外 API 前缀 **PRT_**）。分支 `openEuler-24.09-HarmonyIndustry` 是其与 OpenHarmony 工业发行版绑定的直接证据。设计要点（23.09 官方文档）：单进程多线程（task=thread）、抢占优先级调度（非时间片轮转）、us 级延迟、ARM Cortex-A/R/M + RISC-V。

### A-Tune（`gitee.com/openeuler/A-Tune`，已迁 AtomGit）

顶层目录：`analysis/`（`dataset/` 训练 CSV + `models/` + `generate_models.py`）、`anomaly_detection/`、`api/profile/`、`cmd/`、`common/`、`database/`、`modules/`、`optcfg/`、`profiles/`、`rules/`、`tuning/`、`vendor/`、`scripts/`、`tests/`。运行时配置 `/etc/atuned/atuned.cnf` 与 `/etc/atuned/engine.cnf`。

### openEuler kernel 与 Embedded

`gitee.com/openeuler/kernel` 分支 OLK-5.10（22.03 LTS）/ OLK-6.6（24.03 LTS）；`gitee.com/openeuler/yocto-meta-openeuler` 含 `recipes-kernel/`、`recipes-bsp/`、`meta-openeuler-bsp/`（BSP 层按板卡分目录）。

## 5. 亮点：好在哪

1. **发行版工程成熟**：6 个月创新版 + 2 年 LTS + SP 滚动的版本列车，配合 SIG 责任田，能同时喂饱激进特性和企业稳定性（版本页实证）。
2. **内核增强方法论清晰**：混部调度/内存分级都是「企业需求驱动、上游没有或滞后」的场景补丁，OLK 分支是需求孵化器（24.03 白皮书与 key_features 文档实证）。
3. **RT 路线极度务实**：不重复造实时内核，PREEMPT_RT + `kernel-rt` 包级交付（dnf 一条命令），历史双内核方案（Xenomai）试过后收敛（Preempt_RT 仓库与论坛实证）。
4. **混合关键性框架化**：MCS 拆成「部署+隔离+调度」三步，MICA 用 OpenAMP/rpmsg 标准件而非私有协议（官方博客与 23.09/24.03 Embedded 文档实证）。
5. **A-Tune 闭环完整且可审计**：模型、精度、数据集、参数配置全部在仓库公开，决策产物是可回滚的命名 profile 与配置文件（A-Tune 仓库实证）。
6. **UniProton 是 BerryOS 真正量级可比的对象**：轻量、us 级、也做了 tickless 与 SMP，且提供 POSIX KAL 适配层（源码树实证）。

## 6. 不足与争议：不好在哪

1. **强华为色彩**：治理已社区化，但技术路线（内核补丁方向、版本主题）仍明显由华为战略牵引。
2. **OLK 与 upstream 漂移成本高**：长期携带大量补丁，跟随上游大版本慢（22.03 用 5.10，24.03 才到 6.6，落后主线 2–3 个大版本）。
3. **A-Tune 活跃度衰退**：约一年无实质提交、迁 AtomGit，官方重心转向 EulerCopilot/Agent Infra，原引擎事实上进入维护态（仓库提交时间线实证）。
4. **UniProton 活跃度一般**：730 commits/55 人，最后实质提交约在 2025 年，社区体量与宣传的「工业/车规」定位不匹配。
5. **RT 延迟数据可信度存疑**：<100μs 均为官方白皮书口径，缺独立第三方 cyclictest 复现（待核实）。
6. **主战场在服务器/云**：Embedded/RT/UniProton 是配套角色，投入与主线不对等。

## 7. 对 BerryOS 的借鉴与警示（对照四支柱）

**微内核支柱**
- 借鉴：UniProton 的 `src/kal/posix` KAL 分层——内核原生 API 与 POSIX 兼容层解耦，与 BerryOS「自有 ABI + POSIX 兼容库」完全同构，可对照其 `src/include/uapi` 与 `src/include/posix` 的头文件组织方式设计 bos-sys 镜像；其「单进程多线程、task 即线程」的裁剪也说明：轻量系统的 POSIX 子集必须一开始就划边界（哪些 API 静默不支持），而不是事后补。
- 借鉴：MCS 的「部署+隔离+调度」三步走验证了 BerryOS 在嵌入式形态的正确路径——用隔离（地址空间/能力）而非特权级混部解决实时性。
- 警示：openEuler 本体是宏内核，其混部调度补丁直接长在 CFS 和内核锁上，**照搬实现无意义**；该学的是场景定义（在线/离线混部、抖动抑制、业务优先级 OOM）。

**省电支柱**
- 借鉴：UniProton 也做了 `adapt tickless`（提交实证）——RTOS 圈 tickless 已是标配，佐证 BerryOS NO_HZ day-one 的正确性。
- 借鉴：openEuler 内存分级的「冷热分层」思路可映射到 BerryOS 未来大小内存设备（手表）的页面放置策略。

**AI 原生调度支柱（与 A-Tune 逐点对照，重点）**
- 差异本质：A-Tune 的 AI 决策终点是**配置文件与 profile 切换**（改 sysctl 类参数），粒度是系统级、时机是离线/半在线；BerryOS 设计是内核留第一类 ABI，AI 服务**运行时直接干涉放置/频率/优先级**，粒度到任务、时机是常态在线。openEuler 没有做到后者——这正是 BerryOS 的差异化空间。逐项对照：

| 维度 | A-Tune 实测形态 | BerryOS 设计目标 |
|------|----------------|-----------------|
| 决策粒度 | 系统级参数（网络 backlog、中断亲和等） | 任务级放置/频率/优先级 |
| 决策通路 | 用户态改配置文件 → 重载生效 | 内核 ABI 直达调度器决策点 |
| 决策时机 | 切 profile（离线/半在线） | 常态在线闭环 |
| 内核角色 | 被动提供指标与参数接口 | 第一类接口 + 遥测 ABI + 护栏 |
| 异常保护 | anomaly_detection 模块 + profile 可回滚 | 策略快照 + 护栏白名单 + 服务崩溃降级保活 |

- 借鉴 1：**profile 制**——AI 决策物化为命名、可审计、可一键回滚的 profile（A-Tune 的 10+ workload profile），对应 BerryOS AI 策略服务的「策略快照+护栏白名单」。
- 借鉴 2：**离线训练 + 在线推理拆分**（`analysis/models` 离线 RandomForest，在线只推理）——正好映射 BerryOS (B)启发式→(A)学习两路线的演进接口。
- 借鉴 3：**anomaly_detection 独立模块**——把「检测异常」与「执行调优」解耦，对应 BerryOS 护栏分层（检测可独立降级，不拖垮决策路径）。
- 借鉴 4：**决策持久化**——AI 服务崩溃后系统按最近已知最优 profile 继续运行，对应 BerryOS「PM/AI 服务可崩、内核降级保活」原则。
- 警示：A-Tune 的教训是只做「参数推荐」价值感知弱、难以持续投入；BerryOS 必须把 AI 做成调度路径的常态参与者而非旁路工具。

**PC 高性能支柱**
- 借鉴：openEuler 混部场景的「在线抢占离线 + 业务优先级 OOM」给了 capacity(0..1023)+放置策略最直接的目标场景画像；其 QOS hint 与 BerryOS 每任务 EPP hint 同型。
- 借鉴：南向芯片兼容矩阵（鲲鹏/飞腾/龙芯/申威/海光/兆芯/x86/RISC-V）作为长期认证清单的管理方式，BerryOS 未来多架构移植可沿用。

**工程与发布**
- 借鉴：版本列车 + SIG 责任田 + SP 滚动修复模式；EulerCopilot 的 Agent+Skills 路线值得持续跟踪（AI 原生 OS 的正面竞品）。

## 8. 参考资料链接

- 官网与治理：https://www.openeuler.org/zh/
- 24.03 LTS 技术白皮书：https://www.openeuler.org/whitepaper/en/openEuler%2024.03%20LTS%20Technical%20White%20Paper.pdf
- 创新项目总览（UniProton/RT/Xenomai 历史）：https://openeuler-website.obs.ap-southeast-1.myhuaweicloud.com/pdf/openEuler操作系统（创新项目总览）.pdf
- 24.03 LTS SP1 特性文档：https://docs.openeuler.org/zh/docs/24.03_LTS_SP1/key_features/
- 内核仓库（OLK 分支）：https://gitee.com/openeuler/kernel
- Preempt_RT 镜像：https://github.com/openeuler-mirror/Preempt_RT
- openEuler Embedded（Yocto）：https://gitee.com/openeuler/yocto-meta-openeuler
- 多 OS 混合部署框架博客：https://www.openeuler.org/zh/blog/20220628-em-duoos/embedded-duoos
- MICA 框架文档（24.03 LTS）：https://embedded.pages.openeuler.org/openEuler-24.03-LTS/features/mica/intro.html
- UniProton：https://gitee.com/openeuler/UniProton （已迁 https://atomgit.com/openeuler/UniProton ）
- A-Tune：https://gitee.com/openeuler/A-Tune （已迁 https://atomgit.com/openeuler/A-Tune ）
- MCS 混合部署源码：https://github.com/openeuler-mirror/mcs
