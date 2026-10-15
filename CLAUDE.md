# CLAUDE.md

> 本文件由 Claude Code 每次会话自动加载，是本项目的工作上下文。
> 跨工具通用版（Cursor/Copilot/Codex 等）见 `AGENTS.md`，两者保持一致。

## 项目身份

**BerryOS** —— 自研的微内核 + 跨平台操作系统。
- 内核核心：**Rust**（`#![no_std]`，最小特权，只做调度 / IPC / 地址空间 / 能力）
- 用户态服务与工具：**Rust**
- 首批主线架构：**ARM64 (AArch64)**（v1 主战场 = 手表/穿戴 rich 档单线走到底，ADR #39）；**x86_64 提前启动 QEMU 最小集移植**（串口+定时器+异常向量+上下文切换+SVC 入口，范围盒子 + kill criteria，ADR #41）；RISC-V 留位。MCU 档（KB 级内存）v1 排除（ADR #40，覆盖面 = MMU 设备 MB 级起）
- 当前阶段：**Phase 2b（地址空间 + 首个 EL0）进行中——2b-0/2b-1 已落地**。2b-1 高半翻转：内核主体已链到 `KERNEL_VA_BASE` 高半（镜像布局 VMA=KBASE+LMA → `.data/.bss` 零拷贝）；`.text.boot` + 早期栈 + `.bss.boot`（4 张 boot 表）留低半；`phys_to_virt` 翻为加基址（va.rs 运行时注入）；boot 串口 `KVA ok` = RAM 魔数 + GIC PIDR2 双别名自校验；smoke 固化程序头断言（`size.py --assert-flip`）。**外部行为与 2b-0 零变化（本步验收线）**。此前 Phase 2a（调度与任务）已完成、过关卡 G1（单核 + 抢占点）：内核线程 + 上下文切换（纯叶子 asm）+ 协作/抢占调度器（机制在内核、策略可插拔 trait）+ tickless idle（NO_HZ day-one）+ IRQ 驱动抢占（两忙循环任务 timer slice 分时，无 yield）；buddy/slab、异常向量 + panic dump、GICv2 + CNTV、SVC syscall（G0 全过）。下一步 2b-2 每进程页表 + ASID + 低半退役 → 2b-3 匿名 VMO/Vmar → 2b-4 首个 EL0 + G1.5（五步拆步见 `内存管理设计.md` §5.1）。
- **身份支柱**：微内核 + 省电（手表/手机）+ **AI 原生调度**（内核留第一类接口让用户态 AI 干涉放置/频率/优先级，见 `文档/AI调度设计.md`）+ PC 高性能。

## 架构决策（已定，详见 `文档/架构决策.md`）

四轮设计问答锁定。改任何相关代码前先对齐：

| 维度 | 决策 |
|------|------|
| 哲学 | Zircon 务实派（内核有对象/分配器） |
| 权限 | 纯能力（Zircon 内核态 handle） |
| 内存 | VMO + 用户态 pager + 每进程独立 AS |
| IPC | 全支持：同步 channel + 异步通知 + 共享内存；路径式命名空间 |
| 并发 | 细粒度锁 + 全可抢占（实施分阶段） |
| syscall | 中等偏上 ~30–45（v1 仅 ~26，用 `object_*` 合并）；FS/网络走 IPC |
| 驱动 | 用户态为主，启动关键（GIC/timer/UART）暂留内核 |
| 启动 | QEMU 直加载（`-kernel`） |
| 软件 | 自有 ABI + POSIX 兼容库（不跑 Linux 二进制，**v1 无 fork**） |
| 调度/电源 | 双线并行（见 `调度器设计.md`） |
| 语言/架构 | **全 Rust**（内核 `#![no_std]` + 用户态）；syscall 边界用 C-ABI 约定；ARM64 首发 |
| **端定位** | v1 = 手表 rich 档（MB 级内存）ARM64 单线；MCU 档排除；v2 攻工业 RT（对齐 6.12 PREEMPT_RT 量级）；x86 = 兼容验收 + QEMU 最小集提前（XNU 同源分端；ADR #39–#41） |
| 实时/FS | 软实时；MVP 用 FAT |
| **分阶段基线** | v1=单核+抢占点+无 demand paging+最小能力/IPC，逐版推向满配（见各文档「阶段化目标」+ `测试策略.md` 关卡 G0–G5） |
| **AI 原生调度** | 内核留接口让用户态 AI 策略服务干涉放置/频率/优先级；慢/快循环切分、RT 屏蔽、护栏、平台分层（见 `AI调度设计.md`） |

## 微内核设计原则（写代码前必须遵守）

1. **内核极简**：内核只保留不可委托的能力 —— 调度器、IPC、地址空间管理、能力（capability）派发。文件系统、网络、设备驱动**统统放用户态**，作为服务进程。
2. **IPC 是中枢**：服务之间、服务与内核之间通过显式消息传递通信，禁止内核内直接共享可变状态。
3. **能力即权限**：资源访问通过 capability 句柄传递，而非全局 UID/权限位检查。
4. **硬件抽象隔离**：所有架构相关代码收敛在 `架构相关/<arch>/` 下，核心逻辑写成与架构无关的 Rust，移植新架构只动 HAL 层。
5. **失败隔离**：单个用户态服务崩溃不应拖垮系统，内核需提供重启/监督机制。

## 调度器与电源（设计已定，详见 `文档/调度器设计.md`）

改动任何调度 / 电源相关代码前，先读设计文档。硬约束：
- **机制在内核（Rust），策略可插拔**：内核只做上下文切换 / 每核 runqueue / 派发框架；调度策略做成可注册 trait（`class_rt` / `class_fair` / `class_deadline` / `class_idle`）。
- **capacity(0..1023) 抽象**：每个 CPU 一个算力值，统一手表(单核小算力)→手机(大小核)→PC(全对称)；拓扑由 `架构相关/<arch>/` 填表，核心逻辑架构无关。一条放置路径三端通用。
- **Tickless（NO_HZ）从 day-one 落地**：空闲核不收周期定时中断，续航第一杠杆，不是后期补丁。
- **每核 runqueue + 负载均衡**：多核吞吐主力；单/少核平台近零开销。
- **电源管理是用户态 Rust 服务**：DVFS governor / 热管理 / 休眠策略推用户态；内核↔PM 走**共享环形缓冲**（禁通用 IPC 消息拷贝，否则吃延迟）；PM 服务可崩，内核降级保活、监督重启。
- **每任务 EPP hint**（`perf`/`efficiency`）指引大小核放置与改频。
- **双线并行**：省电（tickless/idle/PM）与多核性能同为 v1 一等公民，路线见设计文档 M1a/M1b。

## 仓库布局

```
berryos/
├── 内核/              Rust no_std：调度 / IPC / 地址空间 / 能力
├── 内存管理/          Rust no_std：物理页帧 / 虚拟内存 / 页表
├── 进程调度/          Rust no_std：调度器 / 线程 / 上下文切换
├── 进程间通信/        Rust no_std：消息传递 / 共享内存 / 信号量
├── 架构相关/          Rust：HAL + 内联汇编（规划 架构相关/aarch64/）
├── 引导加载器/        Rust：boot / 启动协议
├── 驱动/              Rust：用户态驱动服务
├── 文件系统/          Rust：VFS / 具体 FS（用户态服务）
├── 网络/              Rust：协议栈（用户态服务）
├── 用户态/            Rust：libc / shell / 系统服务
├── 工具链/            构建脚本（cargo + just）/ Rust target 配置
├── 文档/              设计文档 / 笔记
├── 参考源码/          第三方参考实现，独立存放（见其内 README.md）
├── CLAUDE.md
└── AGENTS.md
```

> 全 Rust：内核类目录是 `#![no_std]` freestanding Rust（不链接 std/alloc，自带分配器），服务类目录是普通 Rust。**syscall 边界用 C-ABI 约定**（Rust ABI 不稳定），两端都是 Rust——数据用 `#[repr(C)]`。`架构相关/` 内按平台分子目录（如 `架构相关/aarch64/`）。

## 设计文档

- `文档/路线图.md` —— 宏观开发路线（Phase 0–8，每阶段一个 QEMU 里程碑）。
- `文档/调度器设计.md` —— 调度器与电源管理详细设计（机制/策略/capacity/tickless/PM 服务）。
- `文档/架构决策.md` —— 全部决策的单一事实来源（ADR）。
- `文档/能力模型设计.md` —— 纯能力 + Zircon handle。
- `文档/IPC设计.md` —— channel/通知/共享内存 + 路径式命名空间。
- `文档/内存管理设计.md` —— VMO + 用户态 pager + 每进程 AS。
- `文档/系统调用ABI设计.md` —— 中等偏上 ~30–45 syscall（v1~26，具体编号表）+ POSIX 子集边界（v1 无 fork）。
- `文档/测试策略.md` —— 关卡式验证（G0–G5）：单核正确 → 抢占 → SMP → 全抢占 → demand paging。高并发可抢占内核的命门。
- `文档/AI调度设计.md` —— AI 原生调度：策略注入/遥测 ABI、安全护栏、平台分层、(B)启发式→(A)学习两路线。
- `文档/安全与硬件隔离设计.md` —— IOMMU/SMMU + PAC/BTI/MTE + 度量启动（能力安全的硬件后盾）。
- `文档/可观测性设计.md` —— day-one 内置 tracing（静态点 + per-CPU 无锁环 + trace 服务）。
- `文档/文件系统设计.md` —— FAT 仅 MVP；目标现代 FS（CoW/日志/校验）。
- `文档/架构综合纲领.md` —— **综合视角（博采众长不杂糅）**：六个参考（Redox/Theseus/rCore/hubris/seL4/Zircon）的取/留、矛盾矩阵 10 轴的唯一 PICK、防杂糅的排除 8 问。借鉴任何参考前先读。

## 语言策略与 syscall 边界（全 Rust）

| 层 | 形态 | 备注 |
|----|------|------|
| 微内核核心、HAL、引导早期 | **Rust `#![no_std]`** | 不链接 std/alloc；自带分配器；`panic=halt`；汇编用 `core::arch::asm!` / `global_asm!`（EL 检测、MMU、异常向量、上下文切换） |
| 用户态系统服务、库、工具链、用户程序 | **Rust** | 服务 `#![no_std]` 起步，高层工具可上 `std` |
| **syscall 边界** | **C-ABI 约定**（两端皆 Rust） | Rust ABI 不稳定，故 syscall 入口/数据结构用 `extern "C"` + `#[repr(C)]`；内核与用户态都是 Rust，无跨语言 FFI 风险 |

约定：
- **`unsafe` 策略**：`unsafe` 收敛到小块——页表、上下文切换/寄存器保存、MMIO、分配器、硬件原子操作；外层包安全 API。内核 = "安全 Rust 外壳 + 小 unsafe 核"。
- Rust 侧绝不持有跨 syscall 边界的引用生命周期假设；跨越边界的数据按拥有的/复制的传递。
- 公共 ABI（`bos_*` + 结构）用 `#[repr(C)]`，内核与 `bos-sys` 镜像同步（见 `系统调用ABI设计.md`）。

## ARM64 目标约定

- 默认目标：`aarch64-unknown-none`（内核 + 用户态 no_std）；如需特殊段布局用自定义 `target.json` + linker script。
- 内核运行在 EL1，用户态 EL0；**启动入口 EL 随 QEMU 配置而变**（默认 `-machine virt`+`-kernel` ELF 起始 EL1；`-machine virt,virtualization=on` 则 EL2），EL 检测/降级在 `架构相关/aarch64/` 集中处理（`mrs CurrentEL` 判别 → 按需 eret 到 EL1）。异常级别切换亦在此。
- 调试与验证主用 **QEMU `qemu-system-aarch64`**（virt 机型）。

## 构建与测试

Phase 2a（G1）已落地 + Phase 2b-0（高半准备）：stable Rust + `aarch64-unknown-none`，`just run` 在 QEMU `virt` 串口依次打印 `BerryOS boot` → `KVA ok`（2b-0：TTBR1 高半别名自校验）→ `GIC ok / FRAME ok / HEAP ok / SYSCALL ok` → `PING TASK ok`（switch asm 乒乓）→ `ktest done` + brk 可恢复 dump → **两忙循环任务 `A`/`B` 在 CNTV timer slice 下交替（纯 IRQ 驱动抢占，无 yield）**，并周期打 `switches=N` 金丝雀；`just run-el2` 同（EL2→EL1 eret 降级路径）。MMU 开（低半身份 + 高半 TTBR1 内核表 `KERNEL_VA_BASE`；**2b-1 起内核主体高半执行**，异常 dump PC 应为 `0xffffff80…`）、buddy/slab 堆、异常向量 + panic dump、GICv2、SVC syscall、内核线程 + 上下文切换 + tickless idle + IRQ 抢占全通。

**前置（macOS，一次性）**：

```bash
brew install qemu just                          # qemu-system-aarch64 + just
rustup target add aarch64-unknown-none          # stable，拉预编译 core（无需 nightly/build-std）
```

**日常命令**（仓库根；justfile 已固化 `~/.cargo/bin` 到 PATH）：

```bash
just build          # = cargo build（内核 ELF，target=aarch64-unknown-none）
just run            # QEMU virt 冷启动 → 串口打印 1BerryOS boot（默认起始 EL1），halt
just run-el2        # -machine virt,virtualization=on → 21BerryOS boot（强制 EL2→EL1 eret 降级路径）
just run-release    # release 构建 + 同样 QEMU
just smoke          # 无人值守冒烟（CI 同款）：boot 标记断言 + switches 金丝雀严格递增；SMOKE_KEEP_LOG=1 留日志
just size           # 内核体积基线报告（文件量 / PT_LOAD 镜像与静态内存 / 段明细；CI 同步打印）
just check / just clean
```

要点 / 已踩坑：

- **产物不在 `./target`**：本机全局 `~/.cargo/config.toml` 设 `target-dir=~/.cargo-target`（sccache + 共享 target）。justfile 已用 `cargo metadata` 动态解析真实路径，无需关心。
- **QEMU ≥11.x 不接受 `-bios none`**（报 `Could not find ROM image 'none'`）；`virt -kernel <ELF>` 直加载即可，故 justfile / `.cargo/config.toml` 都不带 `-bios`。
- **QEMU halt 后不退出**：内核 `wfe` 死循环，串口打印完即停在那。退出 QEMU 用 `Ctrl-A` 然后 `X`（macOS 无 GNU `timeout`，自动化里用后台 + `kill`）。
- 调试：`-s -S` + lldb 单步；`-d cpu_reset -D qemu.log` 看启动 CurrentEL。
- 测试入口：`just smoke`（QEMU 无人值守冒烟，CI 同款，`工具链/smoke.sh`）+ `just test-host`（host 单元，不依赖 QEMU）+ QEMU 集成（`ktest` 框架，见 `文档/测试策略.md`）。

## Claude 在本仓库的工作方式

1. **先查参考**：涉及某子系统（IPC、调度、页表…）时，先看 `参考源码/` 是否已有对应对照项目，参照成熟实现再写。
2. **小步提交**：OS 调试极难，每个改动尽量小且可独立验证；能在 QEMU 跑通的里程碑单独成一个提交。
3. **不污染内核态**：内核 `#![no_std]`，禁用 `std`/`alloc`（在自有分配器就绪前）；用内核自有的分配器/日志；新增 crate 前确认 `no_std` 兼容。
4. **协议洁净**：主参考是 **Rust OS**（Redox / Theseus / zCore，MIT 可对照）；hubris = **MPL-2.0 语义可借、文件不搬**；rCore-Tutorial = GPL 只看思路；参照 seL4/Zircon/Linux（C/C++/GPL）时只看思路、不复制实质代码。本仓库许可证 Apache-2.0（ADR #37）。
5. **架构无关优先**：新功能先写成架构无关 Rust，确实需要特权指令时再落到 `架构相关/aarch64/` 并留好接口。
6. **博采众长不杂糅**：中心 = **可注入的「机制/策略分离」+ 硬件信任边界**；借鉴任何参考前先过 `架构综合纲领.md` 的**排除 8 问**（轴 PICK？冲突中心？Theseus 仅进程内？IPC 不统管？seL4 只锁不变量？hubris 静态只借 boot 层？GPL 不进？哪关 G0–G5 验证？）。
7. **改了约定就更新本文件**：新增目标架构、改变语言边界、确定构建命令时，同步回写 `CLAUDE.md` 与 `AGENTS.md`。

## 约束与陷阱（Gotchas）

- 内核 `#![no_std]`：禁 `std`/`alloc`（分配器就绪前）；`panic=halt`（自实现 panic handler + 栈回溯）；**浮点寄存器默认不保存**，用 FPU 的路径需显式保存恢复。
- **`unsafe` 纪律**：只在页表/上下文切换/MMIO/分配器/硬件原子用 `unsafe`，包薄安全层；绝不散落。
- **syscall 边界无 panic**：跨 syscall 的 Rust 函数返回 `Result`/状态码，绝不 panic（内核态 panic = 系统停）。
- ARM64 下注意 Cache 一致性、屏障指令（`dmb`/`dsb`/`isb`）、大端小端（默认小端）。
- **QEMU aarch64 `virt` 起始 EL 随配置而变**：默认 `-machine virt`+`-kernel` ELF 起始 **EL1**（实测 `just run`→`1`）；`-machine virt,virtualization=on` 起始 **EL2**（`just run-el2`→`21`，走 EL2→EL1 eret）。boot 第一行（`global_asm!`）`mrs CurrentEL` 检测并按需降到 EL1（关 MMU/cache、写 `sctlr`、`eret` 到 EL1）——**别假设固定 EL**。
- **PAC/BTI/MTE 目标 day-one 开**，但 Phase 0/1 推迟——`cortex-a72`（ARMv8.0）无此扩展；**v2** 切 `-cpu max` 时开 target feature（源码零改动，见 `安全与硬件隔离设计.md` §3/§6）。
- **QEMU TCG 下 CNTV 实测 tick 率 ≈ SLICE_HZ/2**：`CNTFRQ_EL0`=62.5MHz、`SLICE=FREQ/SLICE_HZ`=6.25M（配置正确，`SLICE_HZ=10` 应得 10Hz），但 `just run` 实测抢占 ~5Hz（200s 稳定 1000 切换、60s 稳定 300）——`CNTVCT` 在 TCG `virt`/`cortex-a72` 下推进速率 ≈ `CNTFRQ/2`。**非内核 bug**：真机或加 `-icount` 应为 10Hz。看 tick 率别被这 2x 误导。
- **2b-1 起内核主体链在高半（VMA = `KERNEL_VA_BASE` + LMA 镜像）**：符号/PC 显示 `0xffffff80…` 而镜像实加载在低半（QEMU `-kernel` ELF 按 **p_paddr** 落内存）——看布局跑 `python3 工具链/size.py --assert-flip`（smoke 已固化断言）。**ADRP 只有 ±4GB、双向坑**：低半早期代码（`.text.boot`）引用不了高半静态量（故 4 张 boot 表放 `.bss.boot` 低半）；高半代码取低半绝对符号地址同样越界（故 mm.rs 导入高半 `__kernel_end` 运行时减基址）。
- 微内核里"把东西塞进内核"是反模式 —— 先问：这能不能做成用户态服务？
