# 规则/（二级规则目录）README

> 定位：规则单源 = 根 `AGENTS.md`（硬帽 200 行，只载当前状态 + 铁律路由）；本目录承载规则正文展开。
> CLAUDE.md 是指向 AGENTS.md 的 `@import` 指针（同文导入，同步合同作废）。
> 判例瘦行落各域文件「判例瘦行」节，全文在 `docs/雷册-判例库.md`——编号反查 → [判例索引.md](判例索引.md)。

## 四层结构（谁装什么）

| 层 | 文件 | 装什么 | 行预算 |
|---|---|---|---|
| 入口指针 | `CLAUDE.md` | 仅 `@AGENTS.md` 导入行 + 说明 | ~8 行 |
| 规则单源 | `AGENTS.md` | 当前状态 + 铁律路由 + 命令速览 + 维护合同 | ≤200 行（>100 字/行警、>120 拒） |
| 域规则 | `docs/规则/01–13-*.md` | 域铁律 + 判例瘦行 + 深读指针 | 无帽，勿灌水 |
| 判例全文 | `docs/雷册-判例库.md` | §A/§B/§C 逐条全文（单一事实源） | 不设帽 |

阅读纪律：域文件按需**读**、不进 `@import` 链（imports 不省上下文）；AGENTS.md 是唯一自动注入面。

## 路由表（展开版）

（AGENTS.md「总闸」表原文镜像——改路由两处同步；`docs/` 直达行保留。）

| 动什么 | 先读 | 为什么 |
|---|---|---|
| kernel/ mm/ sched/ ipc/ arch/ 内核态代码 | `规则/01-内核编码.md` | 纪律 |
| 内存/页表/VMO/地址空间 | `规则/02-内存与页表.md` · `docs/内存管理设计.md` | 地形 |
| syscall 号/ABI/berry-sys | `规则/03-系统调用.md` · `docs/系统调用ABI设计.md` | 契约 |
| 调度器/睡眠队列/IPC/信号位 | `规则/04-调度与IPC.md` · `docs/调度器设计.md` | 时序 |
| SMP/锁/per-CPU（S-3 线） | `规则/04-调度与IPC.md` · `docs/SMP-S3实施设计草案.md` | 并发 |
| 常驻服务/svcmgr/libc 桩/收集方 | `规则/05-服务监督.md` | 活性 |
| FS/盘面/FAT 写路径 | `规则/05-服务监督.md` · `docs/文件系统设计.md` | 写序 |
| 用户程序/编译链/.bap/rootfs | `规则/06-用户程序与编译链.md` · `docs/应用开发指南.md` | 产链 |
| smoke 标记/测试断言/bench | `规则/07-测试与冒烟.md` · `docs/测试策略.md` | 可复现 |
| QEMU 机型/引导参数/时率解读 | `规则/08-QEMU与仿真.md` | 仿真差 |
| 中断/GIC/PL011/virtio 驱动 | `规则/09-中断与设备.md` · `docs/驱动模型设计.md` | 硬件序 |
| x86_64 最小集（arch/x86_64/） | `规则/10-x86移植.md` | 异构 |
| 构建脚本/target dir/产物字节 | `规则/11-构建与产物.md` | 零漂移 |
| 引导/EFI/镜像/装载器 | `规则/12-引导与镜像.md` | 装载序 |
| 引入参考/加依赖（本仓 Apache-2.0，ADR #37） | `规则/13-工作流程与协议.md` | 协议洁净 |
| GUI 线（berryui/显示链） | `docs/GUI里程碑规划-2026-10.md` | 刀序 |
| AI 调度接口 | `docs/AI调度设计.md` | 护栏 |

## 架构决策全表

57 条 ADR 单一事实源 = `docs/架构决策.md`，改相关代码前先对齐：

- **内核形态**：Zircon 务实派；纯能力（Zircon 内核态 handle）；VMO + 用户态 pager + 每进程独立 AS；IPC：同步 channel + 异步通知 + 共享内存 + 路径式命名空间。
- **并发 / syscall**：细粒度锁 + 全可抢占（实施分阶段）；~30–45（v1 仅 ~26，`object_*` 合并）；FS/网络走 IPC。
- **驱动 / 启动 / 软件**：用户态为主，启动关键（GIC/timer/UART）暂留内核；QEMU 直加载（`-kernel`）；自有 ABI + POSIX 兼容库（不跑 Linux 二进制，**v1 无 fork**）。
- **调度 / 电源**：双线并行；AI 原生调度（ADR #21 修订版）；intent = 第一类调度输入（u8）·两段式：v1/v2 查表键，M4 ×4 带+预算兜底，凭遥测入场券（ADR #42）。
- **语言 / 架构**：**全 Rust**（内核 `#![no_std]` + 用户态）+ C-ABI + ARM64 首发。
- **实时 / FS**：软实时；MVP 用 FAT。
- **分阶段基线**：v1=单核+抢占点+无 demand paging+最小能力/IPC，逐版推向满配（关卡 G0–G5）。
- **端定位**：见 AGENTS.md「项目身份」。

## 设计文档详表

唯一详表 = `docs/索引.md`（分类/状态标签/维护待办）；动子系统前先读对应设计文档：

- **总纲与路线**：`docs/架构决策.md` · `docs/架构综合纲领.md`（排除 8 问）· `docs/路线图.md`（现行刀序）· `docs/优化议程.md`。
- **内核核心**：`docs/能力模型设计.md` · `docs/IPC设计.md` · `docs/内存管理设计.md` · `docs/系统调用ABI设计.md`（abi.toml 单源）· `docs/调度器设计.md` · `docs/AI调度设计.md` · `docs/可观测性设计.md` · `docs/安全与硬件隔离设计.md`（挂 v2）。
- **服务与生态**：`docs/文件系统设计.md` · `docs/驱动模型设计.md` · `docs/shell设计.md` · `docs/应用模型设计.md` · `docs/GUI里程碑规划-2026-10.md` · 挂 v2 四份：`docs/存储布局与沙箱设计.md` / `docs/兼容子系统设计.md` / `docs/更新与OTA设计.md` / `docs/AI工程面设计.md`。
- **验证与流程**：`docs/测试策略.md` · `docs/开发体验.md`。
- **在途拆解**（现势 → `docs/路线图.md`）：`docs/VM里程碑设计草案.md` · `docs/SMP设计草案.md` · `SMP-S2实施设计草案.md` · `SMP-S3实施设计草案.md` · M-ECO A 系草案 ×7（`容量工程前置-A1设计草案.md`+`事件循环A2..A7设计草案.md`）· `事件循环A7-{2,3}实施设计草案.md` · `事件循环A7-4信号面实施设计草案.md`。
- **演进与归档**（时点记录，勿当现状引用）：`docs/阶段沿革.md`（新进展追加末尾，勿回写本文件）· `docs/Phase4收官盘点.md` / `docs/P6收官盘点.md` · `docs/战役复盘-2026-10.md` · 审查报告 ×2 · `docs/战略裁决-2026-10-15.md`。
- **已落地草案**（冻结候选）：`docs/TCP栈设计草案.md` · `docs/P6-A8网络服务化设计草案.md` · `docs/G-eco编译侧设计草案.md` · `docs/4-6出口演示设计草案.md` · `docs/app-0实施设计草案.md`。

## 仓库布局（完整树）（完整树）

```
berryos/
├── kernel/             Rust no_std：调度 / IPC / 地址空间 / 能力
├── mm/                 Rust no_std：物理页帧 / 虚拟内存 / 页表
├── sched/              Rust no_std：调度器 / 线程 / 上下文切换
├── ipc/                Rust no_std：消息传递 / 共享内存 / 信号量（占位）
├── arch/               Rust：HAL + 内联汇编（aarch64/ 现役 + x86_64/ 最小集）
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
> 全 Rust 分层与 syscall 边界 C-ABI 约定 → AGENTS.md「架构决策速览」与 `03-系统调用.md`，此处不重复。

## 技术栈

- **全 Rust**：内核核心是 `#![no_std]` Rust（只含调度器、IPC、地址空间、能力派发）；用户态服务与工具也是 Rust。
- **syscall 边界用 C-ABI 约定**（Rust ABI 不稳定）：入口 `extern "C"` + `#[repr(C)]`，两端皆 Rust，无跨语言 FFI 风险。
- **`unsafe` 收敛**到页表 / 上下文切换 / MMIO / 分配器，外层包安全 API。
- **首批架构：ARM64**。内核 EL1，用户态 EL0，**启动入口 EL 随 QEMU 配置而变，勿假设固定**（两种机型展开详见 `08-QEMU与仿真.md`），EL 检测/降级归口 `arch/aarch64/`；调试主用 `qemu-system-aarch64 -machine virt`。
## 构建前置与 test-host（域文件引用的口径原文）

- **前置（macOS，一次性）**：`brew install qemu just` + `rustup target add aarch64-unknown-none`（stable，无需 nightly/build-std）；另需 `x86_64-unknown-none`（x86_64 最小集）与 `aarch64-unknown-linux-musl`（build-user 用户态 std 面：bsh/uu 家族与 .bap 同链）——均 stable 预编译 core。
- `just test-host` —— host 单元测试主入口（不依赖 QEMU）；mkapp 门禁自检 + mkfat --check-fixture 挂前置；台账 → 雷册 §C

## 维护合同（拆分契约正文）

1. **改约定**（新架构/语言边界/新命令）→ AGENTS.md 对应速览节回写；阶段进展/标记沿革只写 `docs/阶段沿革.md`。
2. **新判例** → ① 雷册对应节追加全文（§A 续号；§B 冻结不再新增）；② 归属域文件「判例瘦行」节加行（行尾锚点直达雷册）；③ [判例索引.md](判例索引.md) 反查表补行。三处一动齐动。
3. **新域文件**（子系统长大到 13 域装不下时）→ AGENTS.md 路由表加行 + 本 README 路由表镜像 + 索引.md 登记。
4. **执法** → `tools/docs_guard.py`：AGENTS.md 行数帽（≤200）、行长（>100 警 / >120 拒）、探针串在位、瘦行锚点可解析（域文件行尾 `→ 雷册 §Axx` 且雷册有对应节）。
5. **死判例** → 域文件删行 + 雷册节标注 + 判例索引删行（A16 先例：编号退役不再复用）。
