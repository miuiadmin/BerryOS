# CLAUDE.md

> 本文件由 Claude Code 每次会话自动加载，是本项目的工作上下文。
> 跨工具通用版（Cursor/Copilot/Codex 等）见 `AGENTS.md`，两者保持一致。

## 项目身份

**BerryOS** —— 自研的微内核 + 跨平台操作系统。
- 内核核心：**Rust**（`#![no_std]`，最小特权，只做调度 / IPC / 地址空间 / 能力）
- 用户态服务与工具：**Rust**
- 首批主线架构：**ARM64 (AArch64)**（v1 主战场 = 手表/穿戴 rich 档单线走到底，ADR #39）；**x86_64 提前启动 QEMU 最小集移植**（串口+定时器+异常向量+上下文切换+SVC 入口，范围盒子 + kill criteria，ADR #41）；RISC-V 留位。MCU 档（KB 级内存）v1 排除（ADR #40，覆盖面 = MMU 设备 MB 级起）
- 当前阶段：**Phase 2b（地址空间 + 首个 EL0）已完成——2b-0 ~ 2b-4 全落地，出口关卡 G1.5 已过（2026-10）**。高半翻转：内核主体链到 `KERNEL_VA_BASE` 高半（镜像布局 VMA=KBASE+LMA → `.data/.bss` 零拷贝；`.text.boot` + 早期栈 + `.bss.boot` 留低半；`phys_to_virt` 加基址、va.rs 运行时注入；boot 串口 `KVA ok` 自校验 + smoke 程序头断言）。每进程页表 + ASID（16 位实测、0 保留、回卷全量失效）+ **低半身份表退役**（低半访问必 fault）。匿名 VMO/Vmar（`内核/src/vmo.rs`/`vmar.rs`，map 即填用户页表）。**2b-4 首个 EL0 + G1.5（收官）**：`TaskContext::new_user` 弹床 eret 降 EL0（SPSR=EL0t + DAIF 解蔽 → 用户态可被 timer 抢占）；异常走 lower-EL 向量 0x400/0x480（帧宏 `*_el0` 的 sp 槽=SP_EL0）；用户页 AP=0b01；三用户进程矩阵——`user_good` 打 'u'（syscall 往返 + 数据 canary），`user_rogue_ptr`/`user_rogue_svc` 各自 dump 后经 `user_fault_exit` 槽**只死自己**，watchdog 打 `G1.5 ok`（smoke 第 11 标记）。TCB 表示法字段（prio 双份/duration-hint/EPP/preemption-threshold）+ 时间片**周期数记账**（CNTVCT 增量扣减，DVFS 预埋）随 2b-4 落地。此前 Phase 2a（调度与任务，G1）/Phase 0-1（G0）已完成。**Phase 3（能力与系统调用）进行中——3-0 已落地**：`内核/src/handle.rs` 内核对象壳 + handle 表（位域 = 槽号 16 位 + 代号 14 位、bit31 预留、槽 0 恒空；rights 12 位 + `RIGHTS_SAME` 哨兵；`KernelObj` 壳 v1 先 Vmo/Vmar + `ObjRef` 引用计数；五操作 insert/duplicate/close/replace/get——**权限在 handle 不在对象**、dup 只紧不松、close 后代号 +1 旧值不复活；门控顺序 查表→类型→权限）——纯内核层、外部行为零变化，ktest `HANDLE ok`（smoke 第 10 标记）。**3-1 真 syscall ABI 骨架已落地**：`内核/src/syscall.rs` 语义层（HAL `install` 注册口，同 IRQ 槽反转）——v1 27 号在册，实现 `handle_close/duplicate/replace`（syscall 全链路能力门控首次打通）+ `clock_read`(0x61)（MONOTONIC 纳秒）；**撤 2b-4「畸形号杀」政策**——未知/未实现号一律 -ENOTSUP 存活、EL0/EL1 同权（对齐 ABI §8.0「永不 panic」）；调试槽重排 NR_DEBUG_ECHO=0xFE / NR_DEBUG_PUTCHAR=0xFF（非 ABI，P4 退役）；引导表雏形 BOOTSTRAP_HT（3-2 挂进程）+ copy_out 种子（用户区经当前 TTBR0；内核高半仅 EL1 调用方，防 EL0 伪造内核写）；用户矩阵改板——good 改 clock_read 单调自检打 'u'，rogue_svc → **abuser**（真号+垃圾 handle → -EBADF 存活打 'S'），watchdog 断言杀数==1（只 rogue_ptr 死）。**3-2 VMO/Vmar syscall 全量门控已落地**：9 号全实现（0x11–0x17 create/commit/decommit/read/write/map/unmap/protect/allocate）——rights 矩阵（create 授 R|W|MAP|DUP、vmar map/unmap 需 MAP、protect/allocate 需 MANAGE、map 的 rights 须恰 READ|WRITE）+ `copy_from/to_user`（EL0 界检 `[0x1000,2^39)` + 逐页走查当前 TTBR0 验 AP 位；**无 TOCTOU**：SVC 入口硬件置 DAIF → syscall 主体不可抢占，表/对象免锁）+ **悬空 PTE 防线**（Vmo 每页 mapcnt；**销毁性 close（refs==1）遇映射未解除拒 -EBUSY、别名 close 放行**；decommit 拒映射中页；Drop 对映射中页只泄漏）+ no-panic 防线（vmo_create 上限 16MiB → 巨 size -ENOMEM，防 Vec abort）；**首能力实证**——good 的 root Vmar 进 BOOTSTRAP_HT 经 spawn arg 交用户，user_good 一次性验收链 `create→commit→write→map@0x5000→VA 读写→别名回读→close 拒-EBUSY→RO dup 写拒-EACCES→别名 close→unmap→decommit→close` 全绿才进稳态循环（链只跑一次：14 位代号经不起循环 create/close 耗 gen）；watchdog 升级 `last_killed()` **错杀断言**（kills==1 但死 good/abuser ≠ PASS——盲点曾真实放过一次错杀）。**3-3 process/thread 最小对象已落地**：`内核/src/process.rs`（Process = root Vmar 壳 + ttbr0/asid 快照；Thread = 未启动执行流描述，entry/sp 随 process_start 填）+ 三号 syscall（0x31 create / 0x32 start / 0x34 thread_create）——**对象间钉住**（Thread 钉 Process、Process 钉 root Vmar、start 再替任务钉 Process：`ObjRef::retain/release` 与 handle 计共用归零释放点，Drop 链不回表）；**`process_create` 双出参**（`*process,*vmar`——ABI §8.3 v1 修正，Zircon 同形：父须拿子地址空间根才能映代码）；v1 参数面：job 恒 0（**创建权未设防的已知豁口**，Job v2 收紧）、bootstrap 恒 0（per-process 表落地前无载体）；门控 process/thread 各 MANAGE + 归属校验（跨进程 start → -EACCES）；**引导表仍全局单表**（3-3 不拆表——3-4 channel 的 handle 迁移才是拆表硬前置）。验收 = 第 4 个 EL0 进程 `user_spawner` 经真 syscall 全链创建子进程 `user_child`（子打 'C'、spawner 打 's'，子代码字节内嵌在 spawner 自身代码页 `adr` 同页取址）；ktest F 段盖对象/错误面（ktest 在 sched::init 前跑、start 成功路径不在此验）。**3-4a per-process handle 表 + bootstrap 迁移已落地**：Process 自有 `ht: HandleTable`（出生为空，一切能力待父授入）、boot 手搭用户进程各挂 `Box::leak` 常驻表、`BOOTSTRAP_HT` 降格内核侧表（ktest EL1 面 + 未来 init 雏形）；**派发**：`Task::proc_tag`（sched 存不透明 usize、0=内核任务，语义归内核 crate——EL0 syscall `caller_table` 经 tag 查所属进程表，crate 边界干净）；**迁移原语 `take`/`insert_ref`**（take 摘槽代号 +1 原值作废、**refs 不变**=引用令牌迁移非销毁，映射中 VMO 照样可迁，EPTE 防线仍由最终 close 把守；消费者 = process_start bootstrap + 3-4b channel_send）；**bootstrap 迁移转正**（Zircon `zx_process_start` 同形）：非 0 时调用者表 take → 子表 insert → 子入口 x0=子表新 handle 值，迁移后发送方原值 -EBADF；验收 = spawner 建**礼物 VMO**（写 USER_MAGIC）作 bootstrap 送给子进程，`user_child` 凭**自己表里**的 handle `vmo_read` 出魔数比对打 'C'、spawner 迁移后 close 原值必 -EBADF 打 's'（链断自陷被杀 → watchdog 错杀断言 FAIL）；ktest G 段盖两表隔离/take 作废/insert_ref 可查/子进程出生空表。下一步 3-4b channel 三号（非阻塞：空 recv -EAGAIN/满 send -EBUSY）→ 3-4c event + 遥测环 instrumentation + 空策略表等价测试（Phase 3 收官），拆步表见 `文档/能力模型设计.md` 阶段化目标。
- **身份支柱**：微内核 + 省电（手表/手机）+ **AI 原生调度**（内核留第一类接口让用户态 AI 干涉放置/频率/优先级，见 `文档/AI调度设计.md`）+ PC 高性能。

## 设计总原则（三条，钦定）

1. **人无我有，人有我强**：博采众长的落地口诀——别人验证过的优点学过来，别人踩过的坑绕开。证据库 = `调研报告/`（各家好在哪/不好在哪/怎么做的）；借鉴任何参考前先过 `文档/架构综合纲领.md` 排除 8 问，防杂糅。软件行业积累到今天是做 OS 最好的时代：成熟参考实现全部开源可得，站在巨人肩上是常态而非例外。
2. **AI 友好化**：把 AI 当系统的一等公民——不止 AI 原生调度（`文档/AI调度设计.md`），一切接口对 AI 可读、可推断：ABI 单一事实源生成（`文档/系统调用ABI设计.md` §8）、结构化 tracing（`文档/可观测性设计.md`）、语义化错误码、机器可读文档。
3. **AI 测试友好化**：为"以后都是 AI 测软件"的时代预埋基建——测试基建是产品的一部分，不是附属品：确定性可复现（同输入同输出）、可编程故障注入、机器可读遥测/断言、QEMU 自动化关卡（`文档/测试策略.md` G0–G5）；目标是 AI agent 能独立跑测试、读结果、报 bug。

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

Phase 2b 已收官（G1.5 过关）、Phase 3-0（handle 表）/3-1（syscall ABI 骨架）/3-2（VMO/Vmar syscall 全量门控）/3-3（process/thread 最小对象）/**3-4a（per-process handle 表 + bootstrap 迁移）已落地**：stable Rust + `aarch64-unknown-none`，`just run` 在 QEMU `virt` 串口依次打印 `BerryOS boot` → `KVA ok`（TTBR1 高半别名自校验，2b-2 起只走高半）→ `GIC ok / FRAME ok / HEAP ok / SYSCALL ok`（3-2 起含 VMO/Vmar 九号全链 E 段；3-3 起加 F 段：process_create 双出参/参数面/thread_create 门控/process_start 参数与归属面/钉住释放链；3-4a 起加 G 段：两表隔离/take 迁移原值作废/insert_ref 落新表可查/子进程出生空表）→ `PING TASK ok`（switch asm 乒乓）→ 低半读期望故障恢复（`-> expected D-abort`）→ `ASPACE ok`（两张用户页表同 VA 隔离 + asid=16b）→ `VMO ok`（VMO/Vmar 六操作 + unmap 后读 fault、RO 写 fault 两次期望故障）→ `HANDLE ok`（3-0 handle 表五操作 + 槽位重用代号 +1）→ `ktest done` + brk 可恢复 dump → **两忙循环任务 `A`/`B` 与 EL0 用户进程 `user_good`（打 'u'）/`user_abuser`（打 'S'）/`user_spawner`（打 's'，**3-3 验收：经真 syscall 创建第二个进程**）及其子进程 `user_child`（打 'C'——**3-4a 验收：凭自己表里的 handle `vmo_read` 出父进程 bootstrap 迁入的礼物 VMO 魔数**）在 CNTV timer slice 下交替（A/B 每次打印先读私有页断言隔离）**，周期打 `switches=N` 金丝雀；user_good 入口先跑 3-2 一次性验收链（经真 syscall create→commit→write→map→VA 读写→别名回读→close 拒/RO 拒/unmap→decommit→close）全绿才进稳态循环；spawner 一次性链 process_create（双 handle）→vmo 三步→vmar_map 进子空间→thread_create→建礼物 VMO（写 USER_MAGIC）→process_start(bootstrap=礼物)→迁移后 close 原值必 -EBADF；恶意用户进程 rogue_ptr（野指针）dump + 被杀（**只死自己**），abuser（坏 handle 滥用）-EBADF 存活——watchdog 断言杀数==1 **且死者是 rogue_ptr**（`last_killed()` 错杀断言）打 `G1.5 ok`；`just run-el2` 同（EL2→EL1 eret 降级路径）。MMU 开（高半 TTBR1 内核表 `KERNEL_VA_BASE`，**2b-1 起内核主体高半执行**、异常 dump PC 应为 `0xffffff80…`；**2b-2 起低半身份表退役**——TTBR0 常态为空表或用户表，低半访问必 fault）、buddy/slab 堆、异常向量（current-EL + lower-EL 双路）+ panic dump、GICv2、SVC syscall（EL1/EL0 双入口 + 语义层注册 + copy_from/to_user 界检）、内核线程 + 上下文切换 + tickless idle + IRQ 抢占（含 EL0 运行中抢占）全通。smoke 12 标记（`HANDLE ok` 第 10）+ switches 金丝雀严格递增。

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
- **2b-2 起低半已退役**：MMIO（GIC/UART）一律走高半别名（gic.rs/uart 内部常量 `KERNEL_VA_BASE + PA`），**PA 直访必 fault**——只有 boot asm 的早期打印例外（开 MMU 前直写 PA）。内核空表 `NULL_L1` 放普通 `.bss`（高半 VMA），TTBR0 写入按 `VMA - KERNEL_VA_BASE` 折算 PA。
- **用户页表叶必须 nG=1，全量失效用 `tlbi vmalle1is`**：`tlbi aside1*` 按号失效**够不着 global 项**（boot 块描述符全 nG=0）——用户叶不标 nG 则同 VA 映射跨 ASID 经 TLB 泄漏；退役/回卷要清 global 项只能全量 `vmalle1is`（`aside1all` 不是真实指令，设计文档 §5.2 原写法已更正）。
- **2b-4 起 EL0 异常走 lower-EL 向量 0x400（sync）/0x480（IRQ）**，帧宏 `save/load_frame_el0` 与 current-EL 版**仅 `sp` 槽差异**（存取 **SP_EL0** = 用户 SP；异常自 EL0 入 EL1 硬件强制 SPsel=1，帧仍落内核栈）。不变量：**EL0 运行期间 SP_EL1 留着本任务内核栈位置**（trampoline/上下文切换装入），lower-EL 异常入口才能精确落回该栈。
- **3-1 起 syscall 错误一律错误码自理、永不杀调用者**：未知/越界/未实现编号 → `-ENOTSUP`，坏 handle → `-EBADF`，EL0/EL1 同权（ABI §8.0「永不 panic」承诺的内核执法；2b-4「畸形号杀」临时政策已撤——杀只留给 CPU 故障：D-abort/I-abort/BRK 走 `user_fault_exit`）。用户页 AP=0b01 且未设 UXN = **用户页当前可执行**（NX/只读档留 v2）。
- **内核字节串 `b"..."` 只能 ASCII**：塞中文直接编译错，且下游 crate 可能耗旧 rlib 报出**误导性错误**（如 "cannot find xxx in crate bos_sched"）——见到莫名符号缺失先查上游 crate 真实编译输出；中文文案用 `println!`/`uart_puts(str.as_bytes())` 路径。
- **2b-4 起时间片按周期数记账**（`slice_left` 存 CNTVCT 周期、tick 按增量扣减）：TCG 的 2× tick 率效应原样继承（观测仍 ~5Hz/slice），行为与 tick 计数版一致。
- **`FrameAllocator::alloc()` 返回 PFN 不是 PA**（PA = pfn << PAGE_SHIFT），且帧内容不保证干净（buddy 只管元数据）——新页先清零再用。
- **3-2 起映射中 VMO 的「销毁性 close」拒 -EBUSY、别名 close 放行**：只有 refs==1（本次 close 会 drop 对象）且 mapcnt>0 才拒；dup 出的别名 close 不死对象、正常放行——写成「映射中一律拒」会把合法别名链（RO dup→close RO）误伤（3-2 调试期实踩）。decommit 同理拒映射中页；`Vmo::Drop` 对映射中页只泄漏。
- **syscall 主体不可抢占 = v1 免锁根基**：SVC 异常入口硬件置 DAIF 屏蔽 IRQ 且 handler 全程不解蔽 → 引导表/对象访问单核串行；**任何新 syscall handler 不得解蔽 IRQ、不得忙等跨任务**（3-3 拆 per-process 表前都靠这条）。
- **`vmo_create` v1 上限 16MiB（`VMO_MAX_SIZE`）**：巨 size 的 `Vec` 分配 abort 是 EL0 可触发 panic（违 ABI §8.0），故超限先拒 -ENOMEM——新 syscall 凡按用户 size 做分配的都要照此设上限。
- **user_good 的 3-2 验收链是一次性的**（进稳态循环前跑一遍）：14 位代号经不起循环 create/close 耗 gen；改链时记住 `movz x3,#0x2018` 槽基会被 commit 步的 len 参数借走，须重载（曾因此把 pattern 写进自己代码页 0x1008——恰好自洽没炸，纯运气）。
- **用户模板 svc 后复用参数寄存器，先想清值是什么**（3-4a 实踩）：派发只回写 x0、x1..x6 保留的是**你传进去的参数值**——vmo_read 后拿缓冲地址要用 x2（buf 参数），拿 x3 得到的是 offset（=0，野读被杀，watchdog `杀数 2 > 1` panic 拦下）。套路口诀：哪个寄存器装的什么，svc 前后一个样。
- **3-3 起内核对象间跨界引用必须 `ObjRef::retain/release` 钉住**（Thread 钉 Process、Process 钉 root Vmar、运行中任务钉 Process）：只存裸指针 = 父 handle 关光后 use-after-free。Drop 链（close→drop Thread→release Process→drop Process→release Vmar）纯指针操作不回表，无重入。另一边界：**ktest 在 `bos_sched::init` 之前跑 → 任何会 spawn 的成功路径（如 process_start）不能在 ktest 验**，只能盖参数/归属/状态错误面，happy path 交 EL0 用户进程 + watchdog 错杀断言兜底。
- **用户模板可用 `adr` 取同页内嵌数据**（3-3 spawner 的子代码字节）：自相对寻址整块搬迁后偏移不变，与「仅局部数字标号 + movz 绝对地址」的拷贝安全约定同源——但仅限**同页**内（adr ±1MiB 且不得跨模板边界引用）。
- 微内核里"把东西塞进内核"是反模式 —— 先问：这能不能做成用户态服务？
