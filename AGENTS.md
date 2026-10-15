# AGENTS.md

> 跨工具通用的项目说明，任何 AI 编码代理（Cursor / Copilot / Codex / Aider / Claude Code …）在读到本仓库时应先读此文件。
> Claude Code 专用的会话工作规范见 `CLAUDE.md`，两者内容保持一致，本文为单一事实来源（source of truth）。

## 这是什么

**BerryOS** —— 自研的微内核（microkernel）+ 跨平台操作系统。目标是极简内核 + 用户态服务化，首批跑在 **ARM64 (AArch64)** 上。端定位（ADR #39–#41）：v1 主战场 = 手表/穿戴 rich 档（MB 级内存）单线走到底；MCU 档（KB 级）排除；v2 攻工业 RT；x86_64 提前启动 **QEMU 最小集**移植（范围盒子 + kill criteria），长期为兼容验收目标（XNU 同源分端模式）。

**身份支柱**：微内核 + 省电（手表/手机）+ **AI 原生调度**（内核留第一类接口让用户态 AI 策略服务干涉调度，见 `文档/AI调度设计.md`）+ PC 高性能。

## 技术栈

- **全 Rust**：内核核心是 `#![no_std]` Rust（只含调度器、IPC、地址空间、能力派发）；用户态服务与工具也是 Rust。
- **syscall 边界用 C-ABI 约定**（Rust ABI 不稳定）：入口 `extern "C"` + `#[repr(C)]`，两端皆 Rust，无跨语言 FFI 风险。
- **`unsafe` 收敛**到页表 / 上下文切换 / MMIO / 分配器，外层包安全 API。
- **首批架构：ARM64**。内核 EL1，用户态 EL0，**启动入口 EL 随 QEMU 配置而变**（默认 `-machine virt`+`-kernel` ELF 起始 EL1；`virtualization=on` 则 EL2），EL 检测/降级在 `架构相关/aarch64/` 处理；调试用 `qemu-system-aarch64 -machine virt`。

## 架构决策（已定，详见 `文档/架构决策.md`）

| 维度 | 决策 |
|------|------|
| 哲学 | Zircon 务实派（内核有对象/分配器） |
| 权限 | 纯能力（Zircon 内核态 handle） |
| 内存 | VMO + 用户态 pager + 每进程独立 AS |
| IPC | 同步 channel + 异步通知 + 共享内存；路径式命名空间 |
| 并发 | 细粒度锁 + 全可抢占（分阶段） |
| syscall | 中等偏上 ~30–45（v1~26，`object_*` 合并）；FS/网络走 IPC |
| 驱动 | 用户态为主，启动关键暂留内核 |
| 启动 | QEMU 直加载 |
| 软件 | 自有 ABI + POSIX 兼容库（不跑 Linux 二进制，**v1 无 fork**） |
| 调度 | 双线并行（见 `调度器设计.md`） |
| 语言/架构 | **全 Rust**（内核 `#![no_std]` + 用户态）；syscall 边界用 C-ABI 约定；ARM64 首发 |
| **端定位** | v1 = 手表 rich 档 ARM64 单线；MCU 档排除；v2 攻工业 RT；x86 = 兼容验收 + QEMU 最小集提前（ADR #39–#41） |
| 实时/FS | 软实时；MVP 用 FAT |
| **分阶段基线** | v1=单核+抢占点+无 demand paging+最小能力/IPC，逐版推向满配（见各文档「阶段化目标」+ `测试策略.md` 关卡 G0–G5） |
| **AI 原生调度** | 内核留接口让用户态 AI 策略服务干涉放置/频率/优先级；慢/快循环切分、RT 屏蔽、护栏、平台分层（见 `AI调度设计.md`） |

## 不可妥协的设计原则

1. **内核极简**：FS / 网络 / 驱动都不进内核，做成用户态服务。写代码前先问"这个能不能放到用户态？"
2. **IPC 为中枢**：服务间只通过显式消息通信，内核内不搞全局可变共享。
3. **能力即权限（capability）**：资源访问靠句柄，不靠全局权限位。
4. **硬件抽象隔离**：架构相关代码只活在 `架构相关/<arch>/`（规划中 `架构相关/aarch64/`），核心逻辑保持架构无关。
5. **失败隔离**：服务崩溃由内核监督重启，不波及系统。

## 调度器与电源（设计已定，详见 `文档/调度器设计.md`）

改动任何调度 / 电源相关代码前先读设计文档。硬约束：
- **机制在内核（Rust），策略可插拔**：内核只做上下文切换 / 每核 runqueue / 派发框架；策略做成可注册 trait（`class_rt` / `class_fair` / `class_deadline` / `class_idle`）。
- **capacity(0..1023) 抽象**：每个 CPU 一个算力值，统一手表 → 手机大小核 → PC 全对称；拓扑由 `架构相关/<arch>/` 填表，核心逻辑架构无关。
- **Tickless（NO_HZ）从 day-one 落地**：空闲核不收周期定时中断，续航第一杠杆。
- **每核 runqueue + 负载均衡**：多核吞吐主力，单/少核近零开销。
- **电源管理是用户态 Rust 服务**：governor / 热管理 / 休眠推用户态；内核↔PM 走**共享环形缓冲**（禁通用 IPC 拷贝）；PM 可崩，内核降级保活。
- **每任务 EPP hint**（`perf`/`efficiency`）指引大小核放置与改频。
- **双线并行**：省电与多核性能同为 v1 一等公民。

## 仓库布局

```
berryos/
├── AGENTS.md          ← 你正在读
├── CLAUDE.md          Claude Code 会话规范
├── 内核/  内存管理/  进程调度/  进程间通信/      Rust no_std（内核本体）
├── 架构相关/  引导加载器/                        Rust（HAL + 内联汇编 / 引导）
├── 驱动/  文件系统/  网络/  用户态/              Rust（用户态服务）
├── 工具链/  文档/                                构建 / 文档
└── 参考源码/          第三方参考实现，独立存放（见其内 README.md）
```

> 全 Rust：内核类目录 `#![no_std]` freestanding，服务类目录普通 Rust；**syscall 边界用 C-ABI 约定**，两端皆 Rust。`架构相关/` 内按平台分子目录（如 `架构相关/aarch64/`）。`参考源码/` 与自研代码严格分离，只放别人的源码。

## 设计文档

- `文档/路线图.md` —— 宏观开发路线（Phase 0–8）。
- `文档/调度器设计.md` —— 调度器与电源详细设计。
- `文档/架构决策.md` —— 全部决策的单一事实来源（ADR）。
- `文档/能力模型设计.md` / `IPC设计.md` / `内存管理设计.md` / `系统调用ABI设计.md` —— 各子系统详细设计（均含「阶段化目标」节）。
- `文档/系统调用ABI设计.md` 另含具体 syscall 编号表 + POSIX 子集边界（v1 无 fork）。
- `文档/测试策略.md` —— 关卡式验证 G0–G5：单核正确 → 抢占 → SMP → 全抢占 → demand paging。
- `文档/AI调度设计.md` —— AI 原生调度：策略注入/遥测 ABI、安全护栏、平台分层。
- `文档/安全与硬件隔离设计.md` —— IOMMU/SMMU + PAC/BTI/MTE + 度量启动。
- `文档/可观测性设计.md` —— day-one 内置 tracing。
- `文档/文件系统设计.md` —— 现代 FS 目标（FAT 仅 MVP）。
- `文档/架构综合纲领.md` —— **综合视角（博采众长不杂糅）**：六个参考的取/留、矛盾矩阵 10 轮的唯一 PICK、防杂糅的排除 8 问。借鉴任何参考前先读。

## 编码标准

### Rust（内核 / HAL / 引导）—— `#![no_std]`
- 不链接 `std`/`alloc`（分配器就绪前）；`panic=halt`（自实现 panic handler + 栈回溯）。
- 汇编用 `core::arch::asm!` / `global_asm!`（EL 检测、MMU、异常向量、上下文切换）。
- **`unsafe` 收敛**到页表/上下文切换/MMIO/分配器/硬件原子，外层包安全 API。
- 不在内核内放文件系统 / 网络栈 / 驱动主体逻辑。
- 命名：`snake_case` 函数与变量，模块前缀（如 `sched::`、`ipc::`）。

### Rust（用户态服务 / 工具）
- 服务 `#![no_std]` 起步；高层工具可上 `std`。
- 跨 syscall 的函数返回 `Result` + 显式错误码，绝不 panic（内核态 panic = 系统停）。
- 跨 syscall 结构体加 `#[repr(C)]`，与 `bos-sys` 镜像同步。

## 改动前清单（任何代理执行任务前过一遍）

- [ ] 是否查过 `参考源码/` 里对应参考项目的成熟实现？
- [ ] **借鉴是否过了「架构综合纲领.md」的排除 8 问**？（中心=可注入机制/策略分离+硬件信任边界；冲突中心→拒；找不到 G0–G5 关卡=范围泄漏）
- [ ] 这个功能属于内核还是用户态服务？（默认用户态）
- [ ] 是否把架构相关代码收敛到了 `架构相关/`？
- [ ] 是否避开了 GPL 传染？（参照 GPL 代码时只看思路，不复制实质代码）
- [ ] 改动是否能在 QEMU 上独立验证、小步提交？
- [ ] 是否需要回写 `CLAUDE.md` / `AGENTS.md`（新增架构 / 改语言边界 / 确定构建命令）？

## 构建与测试

Phase 2a（G1）+ Phase 2b-0/2b-1（高半翻转）+ 2b-2（每进程页表/ASID/低半退役）已落地：stable Rust + `aarch64-unknown-none`，`just run` 在 QEMU `virt` 串口依次打印 `BerryOS boot` → `KVA ok`（TTBR1 高半别名自校验，2b-2 起只走高半）→ `GIC ok / FRAME ok / HEAP ok / SYSCALL ok` → `PING TASK ok`（switch asm 乒乓）→ 低半读期望故障恢复 → `ASPACE ok`（两张用户页表同 VA 隔离 + asid=16b）→ `ktest done` + brk 可恢复 dump → **两忙循环任务 `A`/`B` 各带自己的地址空间在 CNTV timer slice 下交替（每次打印先读私有页断言隔离）**，并周期打 `switches=N` 金丝雀；`just run-el2` 同（EL2→EL1 eret 降级路径）。MMU 开（高半 TTBR1 内核表 `KERNEL_VA_BASE`，**2b-1 起内核主体高半执行**、异常 dump PC 应为 `0xffffff80…`；**2b-2 起低半身份表退役**——低半访问必 fault；GIC/UART MMIO 全走高半别名；smoke 固化程序头断言 `size.py --assert-flip`）、buddy/slab 堆、异常向量 + panic dump、GICv2、SVC syscall、内核线程 + 上下文切换 + tickless idle + IRQ 抢占全通。

**前置（macOS，一次性）**：`brew install qemu just` + `rustup target add aarch64-unknown-none`（stable，无需 nightly/build-std）。

**日常命令**（仓库根）：

```bash
just build          # = cargo build（内核 ELF）
just run            # QEMU virt → 1BerryOS boot（默认起始 EL1），halt
just run-el2        # virt,virtualization=on → 21BerryOS boot（EL2→EL1 eret 降级路径）
just smoke          # 无人值守冒烟（CI 同款）：boot 标记断言 + switches 金丝雀严格递增
just size           # 内核体积基线报告（文件量 / PT_LOAD 镜像与静态内存 / 段明细；CI 同步打印）
just run-release / just check / just clean
```

要点：产物在 `~/.cargo-target`（本机全局 target-dir 覆盖，justfile 已动态解析）；QEMU ≥11.x 不用 `-bios none`；halt 后用 `Ctrl-A X` 退出 QEMU。详见 `CLAUDE.md`「构建与测试」与 `文档/路线图.md` Phase 0。

## 参考项目与协议注意

- 优先对照 **Rust OS（MIT，语言匹配）**：Redox、Theseus、zCore（zCore=MIT 可对照；rCore-Tutorial=GPL 只看思路）。
- hubris = **MPL-2.0**：监督重启等语义可借、文件不搬。
- seL4 / Fuchsia Zircon 作**思路参考**（C/C++，机制通用，不复制代码；seL4 内核 GPL-2.0-only，代码一字不进）。
- **GPL/AGPL 项目（Linux、xv6、Genode、rCore-Tutorial）只作思路参考，不复制代码**，以免协议传染。本仓库许可证 Apache-2.0（ADR #37）。
- 详细对照建议见 `参考源码/README.md`。

## Do / Don't

**Do**
- 把新功能先写成架构无关的 Rust。
- 每个可启动里程碑单独成一个提交。
- 在 `参考源码/` 里沉淀对照实现，写清出处与协议。

**Don't**
- 不要为了方便把 FS / 驱动塞进内核。
- 不要在内核里散落 `unsafe`（收敛到页表/上下文切换/MMIO/分配器）。
- 不要跨 syscall 边界 panic。
- 不要假设 QEMU 启动在固定 EL：默认 `-machine virt`+`-kernel` ELF 起始 EL1，`virtualization=on` 起始 EL2——boot 第一行 `mrs CurrentEL` 检测并按需 eret 降级到 EL1。
- 不要复制 GPL/AGPL/MPL 文件进自研内核（主参考用 Rust OS：Redox/Theseus/zCore；hubris 只借语义）。
