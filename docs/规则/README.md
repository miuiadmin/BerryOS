# 规则/（二级规则目录）README

> 定位：BerryOS 规则正文的家 = 本目录域文件 + `docs/命令速览.md` + `docs/架构决策.md`；根 `AGENTS.md` 归 Curia 上游模板（2026-11-12 纯 1:1 终裁，本地零编辑零回写），`CLAUDE.md` 是指向它的符号链接桥（上游 2.5 契约）。
> 判例瘦行落各域文件「判例瘦行」节，全文在 `docs/雷册-判例库.md`——编号反查 → [判例索引.md](判例索引.md)。

## 结构（谁装什么）

| 层 | 文件 | 装什么 | 约束 |
|---|---|---|---|
| 入口桥 | `CLAUDE.md` | 指向 AGENTS.md 的符号链接（非副本） | 上游契约，docs_guard G5 执法 |
| 入口 | `AGENTS.md` | Curia 上游模板：通用工作规则 + 桥 + 结构速览 | 1:1 跟随上游（现势 45 行）；G1 ≤200 行 / G6 ≤3400 字符防本地膨胀 |
| 技能桥 | `.claude/skills` `.codex/skills` | 指向 `../.agents/skills` 的符号链接 | 上游契约，docs_guard G7 执法 |
| 域规则 | `docs/规则/01–15-*.md` | 域铁律 + 判例瘦行 + 深读指针 | 无帽，勿灌水 |
| 判例全文 | `docs/雷册-判例库.md` | §A/§B/§C 逐条全文（单一事实源） | 不设帽 |

阅读纪律：域文件按需**读**；自动注入面只剩 AGENTS.md（Curia 通用规则）——BerryOS 域铁律**不再自动注入**，动代码前先查下方路由表。

## 路由表（展开版）

（路由单一事实源 = 本表——AGENTS.md 已不载 BerryOS 路由。`docs/` 直达行保留。）

| 动什么 | 先读 | 为什么 |
|---|---|---|
| kernel/ mm/ sched/ ipc/ arch/ 内核态代码 | `规则/01-内核编码.md` | 纪律 |
| 内存/页表/VMO/地址空间 | `规则/02-内存与页表.md` · `docs/内存管理设计-v1.1.md` | 地形 |
| syscall 号/ABI/berry-sys | `规则/03-系统调用.md` · `docs/系统调用ABI设计-v1.0.md` | 契约 |
| 调度器/睡眠队列/IPC/信号位 | `规则/04-调度与IPC.md` · `docs/调度器设计-v1.2.md` | 时序 |
| SMP/锁/per-CPU（S-3 线） | `规则/04-调度与IPC.md` · `docs/SMP-S3实施设计草案-v1.0.md` | 并发 |
| 常驻服务/svcmgr/libc 桩/收集方 | `规则/05-服务监督.md` | 活性 |
| FS/盘面/FAT 写路径 | `规则/05-服务监督.md` · `docs/文件系统设计-v1.0.md` | 写序 |
| 用户程序/编译链/.bap/rootfs | `规则/06-用户程序与编译链.md` · `docs/应用开发指南.md` | 产链 |
| smoke 标记/测试断言/bench | `规则/07-测试与冒烟.md` · `docs/测试策略-v1.0.md` | 可复现 |
| QEMU 机型/引导参数/时率解读 | `规则/08-QEMU与仿真.md` | 仿真差 |
| 中断/GIC/PL011/virtio 驱动 | `规则/09-中断与设备.md` · `docs/驱动模型设计-v1.0.md` | 硬件序 |
| x86_64 最小集（arch/x86_64/） | `规则/10-x86移植.md` | 异构 |
| 构建脚本/target dir/产物字节 | `规则/11-构建与产物.md` | 零漂移 |
| 引导/EFI/镜像/装载器 | `规则/12-引导与镜像.md` | 装载序 |
| 引入参考/加依赖（本仓 Apache-2.0，ADR #37） | `规则/13-工作流程与协议.md` | 协议洁净 |
| 版本号/升版/打 v* tag/新 crate 入 workspace | `规则/14-版本管理.md` · `docs/版本管理标准v1.0.md` | 承诺分轨 |
| 触摸响应/输入收割/帧循环/damage/呈现路径 | `规则/15-触摸响应与渲染管线.md` · `docs/触摸响应设计-v1.0.md` | 延迟预算 |
| 写文档/定归属/决策笔记 | `.agents/README.md`（Curia 规范全文）· `docs/README.md`（上游模板：docs/ 两制说明） | 归属 |
| GUI 线（berryui/显示链） | `docs/GUI里程碑规划-2026-10-v1.0.md` | 刀序 |
| AI 调度接口 | `docs/AI调度设计-v1.0.md` | 护栏 |

## 命令速览

全表 → [`docs/命令速览.md`](../命令速览.md)：命令表唯一家就是它；新增/改名 just 配方只改该表——AGENTS.md 归上游，任何情况不回写。

## 架构决策全表

75 条 ADR 单一事实源 = `docs/架构决策.md`，改相关代码前先对齐（计数随立法滚新，以台账与 `docs/索引.md` 行为准——2026-11-16 立法三件 #66-68 并回后口径）：

- **内核形态**：Zircon 务实派；纯能力（Zircon 内核态 handle）；VMO + 用户态 pager + 每进程独立 AS；IPC：同步 channel + 异步通知 + 共享内存 + 路径式命名空间。
- **并发 / syscall**：细粒度锁 + 全可抢占（实施分阶段）；~30–45（v1 40 在册，C-ABI——`tools/abi.toml` `nr_total` 单源，+1.1 四号另计）；FS/网络走 IPC。
- **驱动 / 启动 / 软件**：用户态为主，启动关键（GIC/timer/UART）暂留内核；QEMU 直加载（`-kernel`）；自有 ABI + POSIX 兼容库（不跑 Linux 二进制，**v1 无 fork**）。
- **调度 / 电源**：双线并行；AI 原生调度（ADR #21 修订版）；intent = 第一类调度输入（u8）·两段式：v1/v2 查表键，M4 ×4 带+预算兜底，凭遥测入场券（ADR #42）。
- **语言 / 架构**：**全 Rust**（内核 `#![no_std]` + 用户态）+ C-ABI + ARM64 首发。
- **实时 / FS**：软实时；MVP 用 FAT。
- **分阶段基线**：v1=单核+抢占点+无 demand paging+最小能力/IPC，逐版推向满配（关卡 G0–G5）。
- **端定位**：见 `docs/架构决策.md` 与 `docs/架构综合纲领.md`（身份与原则的家）。

## 设计文档详表

唯一详表 = `docs/索引.md`（分类/状态标签/维护待办）；动子系统前先读对应设计文档：

- **总纲与路线**：`docs/架构决策.md` · `docs/架构综合纲领.md`（排除 8 问）· `docs/路线图-v1.2.md`（现行刀序）· `docs/优化议程-v1.0.md`。
- **内核核心**：`docs/能力模型设计-v1.0.md` · `docs/IPC设计-v1.0.md` · `docs/内存管理设计-v1.1.md` · `docs/系统调用ABI设计-v1.0.md`（abi.toml 单源）· `docs/调度器设计-v1.2.md` · `docs/AI调度设计-v1.0.md` · `docs/可观测性设计-v1.0.md` · `docs/安全与硬件隔离设计-v1.0.md`（挂 v2）。
- **服务与生态**：`docs/文件系统设计-v1.0.md` · `docs/驱动模型设计-v1.0.md` · `docs/shell设计-v1.0.md` · `docs/应用模型设计-v1.0.md` · `docs/GUI里程碑规划-2026-10-v1.0.md` · 挂 v2 四份：`docs/存储布局与沙箱设计-v1.0.md` / `docs/兼容子系统设计-v1.0.md` / `docs/更新与OTA设计-v1.0.md` / `docs/AI工程面设计-v1.0.md`。
- **验证与流程**：`docs/测试策略-v1.0.md` · `docs/开发体验.md`。
- **在途拆解**（现势 → `docs/路线图-v1.2.md`）：`docs/VM里程碑设计草案-v1.0.md`（M-G2 进行中——V-5a 工具面/AAVMF 基线已落 2026-10-28 + V-6 全线收案 2026-10-29，余 Fusion 实机会话 + V-5b/V-7 冻结）· `docs/调点调度-v1.3.md`（九刀已落 · 部分回填）。其余原列件已收官/已落地 2026-10-29..11-06（SMP 线拆步全毕 G2 ✅ · M-ECO A 系 ×11 · G3/G4）+ app-2 刀1+2 已落地 2026-11-10（5cc2c5f；刀3 文书收官挂主会话），状态标签列详 `docs/索引.md`，勿凭本行旧时点引用（2026-11-10 W2-Q29 随刀刷新；app-2 面同日 Q33 二次刷新；VM 面 2026-11-11 灾后重放刷新）。
- **演进与归档**（时点记录，勿当现状引用）：`docs/阶段沿革.md`（新进展追加末尾，勿回写本文件）· `docs/Phase4收官盘点.md` / `docs/P6收官盘点.md` · `docs/战役复盘-2026-10.md` · 审查报告 ×2 · `docs/战略裁决-2026-10-15.md`。
- **已落地草案**（冻结候选）：`docs/TCP栈设计草案-v1.0.md` · `docs/P6-A8网络服务化设计草案-v1.0.md` · `docs/G-eco编译侧设计草案-v1.0.md` · `docs/4-6出口演示设计草案-v1.0.md` · `docs/app-0实施设计草案-v1.0.md`。

## 仓库布局（完整树）

```
berryos/
├── kernel/             Rust no_std：调度 / IPC / 地址空间 / 能力
├── mm/                 Rust no_std：物理页帧 / 虚拟内存 / 页表
├── sched/              Rust no_std：调度器 / 线程 / 上下文切换
├── ipc/                Rust no_std：消息传递 / 共享内存 / 信号量（占位）
├── arch/               Rust：HAL + 内联汇编（aarch64/ 现役 + x86_64/ 最小集）
├── wqmatch/ iprofile/ vmoledger/ piface/ chanface/ evface/ hdlface/ itally/ telface/ sliceface/ ringface/ intface/ emface/ hofface/ hdrface/ vslotface/ dtbface/ bandface/ polface/ safeck/ utrace/ dpgface/ frzface/ lkface/ trapface/ heapface/ rootface/ susface/ aokface/ fwcfgface/ psface/ wakeface/ tickface/ balface/ billface/ memface/ numface/ dmface/ wsrcface/ svcface/
│                       Rust no_std 纯函数面 crate 族（W3-Q1..Q86 外迁族，2026-11-10..16 入库：判定律/编码律/映射表自内核与 sched 源外迁，零 unsafe + host 钉板测试；safeck = sa_parse 结构解码校验〔kernel/syscall.rs 外迁〕、utrace = util EWMA 窗口机盒纯面镜像钉板〔sched/util.rs 原子壳保持现状〕，均随 #148 2026-11-11 入库；dpgface = dpg EL0 槽池认领制判定律〔kernel/dpg.rs 外迁，W3-Q24 随 #155 2026-11-11 入库〕；frzface = sched 冻结面纯函数外迁〔W3-Q27 随 #169 2026-11-11 入库〕；lkface = lockdep 检查面三检判序〔arch/aarch64/lockdep.rs 外迁，W3-Q36 随 #222 2026-11-14 入库，附 mm/ramtop〕；trapface = AArch64 异常分类面〔arch/aarch64/trap.rs 外迁，W3-Q37 随 #223 2026-11-14 入库〕；heapface = heap 分配路由算术面〔kernel/src/heap.rs size-class/order 算术段外迁，route/dealloc_route 对称路由律 + PAGE_SIZE 双源 const assert 钉 berry-mm 漂移，W3-Q39 随 #301 2026-11-15 入库〕；rootface = rootfs 装载门四判定门单源〔kernel/src/main.rs 外迁，W3-Q85 随 #305 2026-11-15 入库〕；susface = suspend 冻结深度判序纯函数面〔kernel/src/syscall.rs 0x3E 两臂准入判序外迁，W3-Q38 随 #299 2026-11-16 入库〕；aokface = access_ok 用户缓冲准入判定律〔kernel/src/syscall.rs 外迁，W3-Q70 随 #357 2026-11-16 入库，workspace path 依赖形非显式 member〕；fwcfgface = fw_cfg 目录行走/顺序读推进/ramfb 域门纯函数面〔arch/aarch64/src/fwcfg.rs 外迁，W3-Q86 随 #307 2026-11-16 入库〕；psface = PSCI 版本门/路由门/FEATURES 侦察纯面〔arch/aarch64/src/psci.rs 判定段外迁，W3-Q72 随 #362 2026-11-16 入库〕；wakeface = 唤醒归因链纯函数面〔arch/aarch64/src/wake.rs 判定律外迁，W3-Q66 随 #365 2026-11-16 入库〕；tickface = CNTV 滴答账纯函数面〔arch/aarch64/src/timer.rs 外迁，W3-Q31 随 #375 2026-11-16 入库〕；balface = on_irq_tick 内嵌 SMP 门与偷取过滤纯函数面〔sched/src/sched.rs 五门五常量单源外迁，W3-Q82 随 #388 2026-11-16 入库〕；billface = telemetry billing 域账户+压力窗折卷判定律〔kernel/src/telemetry.rs 外迁，W3-Q76 随 51f6d8fd 追平合流 2026-11-16 入库〕；memface = mem* 整数版语义模型〔kernel/src/mem.rs 外迁，W3-Q80 随 51f6d8fd 追平合流 2026-11-16 入库〕；numface = UART 整数格式化族纯面〔arch/aarch64 HAL 三文件四消费点壳化，W3-Q83 随 51f6d8fd 追平合流 2026-11-16 入库〕；dmface = fw_cfg `opt/berry/display` 显示模式请求解析纯面〔kernel/src/main.rs display_mode_req 五门判定链外迁，W3-Q84 随 51f6d8fd 追平合流 2026-11-16 入库〕；wsrcface = 唤醒源分类/白名单/槽位钳位纯函数面〔kernel/src/telemetry.rs 三谓词外迁，W3-Q28 随 51f6d8fd 追平合流 2026-11-16 入库〕；svcface = SVC 入口锁类位集与号→锁类注册表纯函数面〔HAL kern_lock.rs 六常量单源迁此，W3-Q67 随 51f6d8fd 追平合流 2026-11-16 入库，附 svc_registry_check 三方对拍执法〕；调度器 API 面引用 → `docs/调度器API参考-v1.15.md`）
├── bootloader/         Rust：BerryBoot EFI 引导
├── drivers/            Rust：用户态驱动独立工作区（berry-virtio 框架；真机驱动批次直接进 bins/<名>/）
├── fs/                 Rust：VFS / 具体 FS（用户态服务，占位）
├── net/                Rust：协议栈（用户态服务，占位）
├── user/               Rust：berry-sys/libc/shell/系统服务/协议 crate
├── tools/              构建脚本（cargo + just）/ smoke / 确定性打包 / ABI 生成
├── docs/               设计文档 / 笔记（文件名中文）
├── research/           调研报告（仅私密仓——公开镜像不带）
├── reference/          第三方参考实现，独立存放、与自研分离（仅私密仓；见其内 README.md）
├── CLAUDE.md
└── AGENTS.md
```

> 14 目录曾用中文名（2026-10 英文化）——中英映射与验证实录 → `docs/阶段沿革.md` 追记。
>
> 全 Rust 分层与 syscall 边界 C-ABI 约定 → `docs/架构决策.md` 与 `03-系统调用.md`，此处不重复。

## 技术栈

- **全 Rust**：内核核心是 `#![no_std]` Rust（只含调度器、IPC、地址空间、能力派发）；用户态服务与工具也是 Rust。
- **syscall 边界用 C-ABI 约定**（Rust ABI 不稳定）：入口 `extern "C"` + `#[repr(C)]`，两端皆 Rust，无跨语言 FFI 风险。
- **`unsafe` 收敛**到页表 / 上下文切换 / MMIO / 分配器，外层包安全 API。
- **首批架构：ARM64**。内核 EL1，用户态 EL0，**启动入口 EL 随 QEMU 配置而变，勿假设固定**（两种机型展开详见 `08-QEMU与仿真.md`），EL 检测/降级归口 `arch/aarch64/`；调试主用 `qemu-system-aarch64 -machine virt`。
## 构建前置与 test-host（域文件引用的口径原文）

- **前置（macOS，一次性）**：`brew install qemu just` + `rustup target add aarch64-unknown-none`（stable，无需 nightly/build-std）；另需 `x86_64-unknown-none`（x86_64 最小集）与 `aarch64-unknown-linux-musl`（build-user 用户态 std 面：bsh/uu 家族与 .bap 同链）——均 stable 预编译 core。
- `just test-host` —— host 单元测试主入口（不依赖 QEMU）；mkapp 门禁自检 + mkfat --check-fixture 挂前置；台账 → 雷册 §C

## 维护合同（拆分契约正文）

1. **改约定**（新架构/语言边界）→ `docs/架构决策.md`（新 ADR）+ 对应域文件回写；**新 just 配方** → `docs/命令速览.md`；阶段进展/标记沿革只写 `docs/阶段沿革.md`；**AGENTS.md 归上游，任何情况不回写**（回写即破 1:1）。
2. **新判例** → ① 雷册对应节追加全文（§A 续号；§B 冻结不再新增）；② 归属域文件「判例瘦行」节加行（行尾锚点直达雷册）；③ [判例索引.md](判例索引.md) 反查表补行。三处一动齐动。
3. **新域文件**（子系统长大到 13 域装不下时）→ 本 README 路由表加行 + 索引.md 登记；不动 AGENTS.md。
4. **执法** → `tools/docs_guard.py`（挂主要 build 入口与 CI 独立步，2026-11-12 适配纯 1:1 形态）：AGENTS.md 行数帽（G1 ≤200）与字符帽（G6 ≤3400，>3000 警）、探针串在位（G3——AGENTS 契约锚 + 域文件口径串）、上游三桥契约（G5 CLAUDE.md / G7 技能双桥）、瘦行锚点可解析（G4——域文件行尾 `→ 雷册 §Axx` 且雷册有对应节）、迁出目标在位（G8）、索引三不变量（G10）。
5. **死判例** → 域文件删行 + 雷册节标注 + 判例索引删行（A16 先例：编号退役不再复用）。
