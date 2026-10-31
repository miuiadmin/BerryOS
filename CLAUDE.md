# CLAUDE.md

> 本文件由 Claude Code 每次会话自动加载，是本项目的工作上下文。
> 跨工具通用版（Cursor/Copilot/Codex 等）见 `AGENTS.md`——**规则层同步维护**（约定/铁律/命令口径两文件一致）；判例与构建标记全文只在 `docs/雷册-判例库.md`（本文件 Gotchas/构建节瘦行 → 雷册 §A/§B/§C 锚点）。

## 项目身份

**BerryOS** —— 自研的微内核 + 跨平台操作系统。
- 内核核心：**Rust**（`#![no_std]`，最小特权，只做调度 / IPC / 地址空间 / 能力）
- 用户态服务与工具：**Rust**
- 首批主线架构：**ARM64 (AArch64)**（v1 主战场 = **移动设备（手机）** rich 档单线走到底——主战场 = 手机 → PC；手表/穿戴 = 同族低功耗端（省电设计直接受益、又低功耗又优秀，随时可落），ADR #39 2026-10-28 修订）；**x86_64 提前启动 QEMU 最小集移植**（串口+定时器+异常向量+上下文切换+SVC 入口，范围盒子 + kill criteria，ADR #41）；RISC-V 留位。MCU 档（KB 级内存）v1 排除（ADR #40，覆盖面 = MMU 设备 MB 级起）
- 阶段沿革：详史逐字归档 → `docs/阶段沿革.md`（Phase 0–6 / P7 / app-0 / V 系 / C 系 / T-1 / A 系 / S-2a / GUI-0..1 全刀程在册；2026-10-31 起本节只留基线——原里程碑摘要 bullets 整体迁该文件末尾追记区）；新记录追加到该文件末尾，勿回写本文件。
- 当前基线：`just smoke` 八跑全绿 + EL2 同绿（相构成见下方「构建与测试」速览）；syscall 34 号在册/33 实现（在册未实现唯 0x85 `berry_attest` 恒 -ENOTSUP；0x86/0x87 信号面已落）；host 测试台账与沿革 → 雷册 §C，总口径以 `just test-host` 输出为准；现行刀序与下一步 → `docs/路线图.md`。
- **身份支柱**：微内核 + 省电（移动设备/手表）+ **AI 原生调度**（内核留第一类接口让用户态 AI 干涉放置/频率/优先级，见 `docs/AI调度设计.md`）+ PC 高性能——主战场 = 移动设备（手机）与 PC，手表为同族低功耗端。

## 设计总原则（三条，钦定）

1. **人无我有，人有我强**：博采众长的落地口诀——别人验证过的优点学过来，别人踩过的坑绕开。证据库 = `research/`（各家好在哪/不好在哪/怎么做的）；借鉴任何参考前先过 `docs/架构综合纲领.md` 排除 8 问，防杂糅。软件行业积累到今天是做 OS 最好的时代：成熟参考实现全部开源可得，站在巨人肩上是常态而非例外。
2. **AI 友好化**：把 AI 当系统的一等公民——不止 AI 原生调度（`docs/AI调度设计.md`），一切接口对 AI 可读、可推断：ABI 单一事实源生成（`docs/系统调用ABI设计.md` §8）、结构化 tracing（`docs/可观测性设计.md`）、语义化错误码、机器可读文档。
3. **AI 测试友好化**：为"以后都是 AI 测软件"的时代预埋基建——测试基建是产品的一部分，不是附属品：确定性可复现（同输入同输出）、可编程故障注入、机器可读遥测/断言、QEMU 自动化关卡（`docs/测试策略.md` G0–G5）；目标是 AI agent 能独立跑测试、读结果、报 bug。

## 架构决策（已定，详见 `docs/架构决策.md`）

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
| **端定位** | v1 = **移动设备（手机）** rich 档（MB 级内存）ARM64 单线（主战场 = 手机 → PC）；手表/穿戴 = 同族低功耗端随时可落（勿以手表锚死范围）；MCU 档排除；v2 攻工业 RT（对齐 6.12 PREEMPT_RT 量级）；x86 = 兼容验收 + QEMU 最小集提前（XNU 同源分端；ADR #39–#41） |
| 实时/FS | 软实时；MVP 用 FAT |
| **分阶段基线** | v1=单核+抢占点+无 demand paging+最小能力/IPC，逐版推向满配（见各文档「阶段化目标」+ `测试策略.md` 关卡 G0–G5） |
| **AI 原生调度** | 内核留接口让用户态 AI 策略服务干涉放置/频率/优先级；慢/快循环切分、RT 屏蔽、护栏、平台分层（见 `AI调度设计.md`）。**2026-10-16 修订**：干涉面限带内、AI 降级为机制预留+实验轨道+kill criteria |
| **调度 intent** | intent=第一类调度输入（u8）·两段式：v1/v2 查表键，M4 ×4 带+预算兜底，凭遥测入场券（ADR #42） |

## 微内核设计原则（写代码前必须遵守）

1. **内核极简**：内核只保留不可委托的能力 —— 调度器、IPC、地址空间管理、能力（capability）派发。文件系统、网络、设备驱动**统统放用户态**，作为服务进程。
2. **IPC 是中枢**：服务之间、服务与内核之间通过显式消息传递通信，禁止内核内直接共享可变状态。
3. **能力即权限**：资源访问通过 capability 句柄传递，而非全局 UID/权限位检查。
4. **硬件抽象隔离**：所有架构相关代码收敛在 `arch/<arch>/` 下，核心逻辑写成与架构无关的 Rust，移植新架构只动 HAL 层。
5. **失败隔离**：单个用户态服务崩溃不应拖垮系统，内核需提供重启/监督机制。

## 调度器与电源（设计已定，详见 `docs/调度器设计.md`）

改动任何调度 / 电源相关代码前，先读设计文档。硬约束：
- **机制在内核（Rust），策略可插拔**：内核只做上下文切换 / 每核 runqueue / 派发框架；调度策略做成可注册 trait（`class_rt` / `class_fair` / `class_deadline` / `class_idle`）。
- **capacity(0..1023) 抽象**：每个 CPU 一个算力值，统一手机(大小核)→PC(全对称)→手表(单核小算力)——同一路径覆盖，端无主次；拓扑由 `arch/<arch>/` 填表，核心逻辑架构无关。一条放置路径三端通用。
- **Tickless（NO_HZ）从 day-one 落地**：空闲核不收周期定时中断，续航第一杠杆，不是后期补丁。
- **每核 runqueue + 负载均衡**：多核吞吐主力；单/少核平台近零开销。
- **电源管理是用户态 Rust 服务**：DVFS governor / 热管理 / 休眠策略推用户态；内核↔PM 走**共享环形缓冲**（禁通用 IPC 消息拷贝，否则吃延迟）；PM 服务可崩，内核降级保活、监督重启。
- **每任务 EPP hint**（`perf`/`efficiency`）指引大小核放置与改频。
- **双线并行**：省电（tickless/idle/PM）与多核性能同为 v1 一等公民，路线见设计文档 M1a/M1b。
- **intent 两段式（ADR #42，2026-10-16）**：**公平是机制默认，intent 是策略输入，叠加永不替代**。v1/v2 intent = u8 查表键（IntentProfile→latency-nice/EPP/uclamp/timer slack；TCB `intent_declared/effective` P3 预埋、`berry_intent` 编码冻结 0x00 UNSPECIFIED(XNU 式不设限)–0x05 MAINTENANCE(冻结不启用)）；M4 起 class_fair ×4 带实例 + 预算包络 + deadline-server 兜底，**凭 v1 遥测入场券**（不满足则单实例即终态）；三层拓扑 class 链 > band 序 > 带内 EEVDF；`SchedClass` trait 签名零改动；band/intent **不入 AI 动作空间**。

## 仓库布局

```
berryos/
├── kernel/             Rust no_std：调度 / IPC / 地址空间 / 能力
├── mm/                 Rust no_std：物理页帧 / 虚拟内存 / 页表
├── sched/              Rust no_std：调度器 / 线程 / 上下文切换
├── ipc/                Rust no_std：消息传递 / 共享内存 / 信号量（占位）
├── arch/               Rust：HAL + 内联汇编（aarch64/ 现役 + x86_64/ 最小集）
├── bootloader/         Rust：BerryBoot EFI 引导（V-0 起）
├── drivers/            Rust：用户态驱动独立工作区（2026-10-24 提前启用——berry-virtio 框架 + bins/{vprobe,vblk,vnet} 自 user/ 迁入；真机驱动批次直接进 bins/<名>/）
├── fs/                 Rust：VFS / 具体 FS（用户态服务，占位）
├── net/                Rust：协议栈（用户态服务，占位）
├── user/               Rust：berry-sys/libc/shell/系统服务/协议 crate
├── tools/              构建脚本（cargo + just）/ smoke / 确定性打包 / ABI 生成
├── docs/               设计文档 / 笔记（文件名中文）
├── research/           调研报告（仅私密仓——公开镜像不带）
├── reference/          第三方参考实现，独立存放（仅私密仓；见其内 README.md）
├── CLAUDE.md
└── AGENTS.md
```

> **目录已英文化（2026-10-24）**：14 个中文目录一次改名（内核→kernel、内存管理→mm、进程调度→sched、进程间通信→ipc、架构相关→arch、引导加载器→bootloader、驱动→drivers、文件系统→fs、网络→net、用户态→user、工具链→tools、文档→docs、调研报告→research、参考源码→reference），映射与验证实录见 `docs/阶段沿革.md` 追记。

> 全 Rust：内核类目录是 `#![no_std]` freestanding Rust（不链接 std/alloc，自带分配器），服务类目录是普通 Rust。**syscall 边界用 C-ABI 约定**（Rust ABI 不稳定），两端都是 Rust——数据用 `#[repr(C)]`。`arch/` 内按平台分子目录（如 `arch/aarch64/`）。

## 设计文档

唯一详表 = `docs/索引.md`（59 份文档分类/状态标签/双阅读路径/维护待办）；本节只留分类速览。动相关子系统前先读对应设计文档：

- **总纲与路线**：`docs/架构决策.md`（57 条 ADR 单一事实源）· `docs/架构综合纲领.md`（取舍纪律 + 排除 8 问——借鉴任何参考前先读）· `docs/路线图.md`（Phase 0–8，现行 M-VM）· `docs/优化议程.md`（五弱题动作表）。
- **内核核心**：`docs/能力模型设计.md`（纯能力 handle）· `docs/IPC设计.md`（channel/Event/intent）· `docs/内存管理设计.md`（VMO/pager）· `docs/系统调用ABI设计.md`（v1 冻结，abi.toml 单源）· `docs/调度器设计.md`（机制 + M 里程碑权威）· `docs/AI调度设计.md`（ADR #21 修订版）· `docs/可观测性设计.md` · `docs/安全与硬件隔离设计.md`（挂 v2）。
- **服务与生态**：`docs/文件系统设计.md` · `docs/驱动模型设计.md`（驱动=持证应用）· `docs/shell设计.md`（bsh + 命令梯队）· `docs/应用模型设计.md`（包 + 清单 + 能力）· `docs/GUI里程碑规划-2026-10.md`（M-GUI 刀序 GUI-0..9 + 裁决 A–I——ADR #50–#54：B 形态协议 / pass 形态 API / berryui 声明式 / 低功耗缝 / SDK 序）· 挂 v2 四份：`docs/存储布局与沙箱设计.md` / `docs/兼容子系统设计.md` / `docs/更新与OTA设计.md` / `docs/AI工程面设计.md`。
- **验证与流程**：`docs/测试策略.md`（G0–G5 关卡）· `docs/开发体验.md`（四级阶梯 + 日常循环）。
- **在途拆解**：`docs/VM里程碑设计草案.md`（现行主里程碑 M-G1..G3）· `docs/SMP设计草案.md`（G2 拆步，S-0/S-1/S-2 已落——S-2b 副核参与 2026-11-01 收官：E0 BKL「内核串行+用户并行」端到端立证，smoke-smp 双相 PASS + 主 smoke 零漂移，详 `SMP-S2实施设计草案.md`；S-3 per-CPU 化立稿中——`SMP-S3实施设计草案.md` 立稿初版 2026-11-01，侦察+三镜审查待） · M-ECO 事件循环线 A 系草案 ×7（`容量工程前置-A1设计草案.md` + `事件循环A2..A7设计草案.md`——A1-A6 已落地 2026-10-30 收官（六标记 POLL/LSN/EVS/SKT/EPL/NETS 入 rootfs 相 37-42，A6-0 盘点在册）、A7 POSIX 原语批已收官 2026-10-31（A7-1 匿名 mmap/A7-2 timer/A7-3 文件映射+mprotect dormant 已落地 2026-10-30——MMP n=9/CLK n=6；信号面门 C 已裁定 A 2026-10-30（add-only 加号放行）；A7-4 信号面已落地 2026-10-31——方案 B 协同式用户态投递（0x86/0x87 内核两号 + libc 投递循环/sigaction 表/掩码/held 再投 + bsh 六步 `SIG ok n=6` 入 rootfs 相 45，`docs/事件循环A7-4信号面实施设计草案.md`）；FPU 出局归 M-GUI 线））· `事件循环A7-{2,3}实施设计草案.md`、`事件循环A7-4信号面实施设计草案.md`。
- **演进与归档**（时点记录，勿当现状引用）：`docs/阶段沿革.md`（详史归档 + 追记区——新进展追加其末尾，勿回写本文件）· `docs/Phase4收官盘点.md` / `docs/P6收官盘点.md` · `docs/战役复盘-2026-10.md`（同型地雷册）· 审查报告 ×2 · `docs/战略裁决-2026-10-15.md`（裁决已吸收进 ADR）。
- **已落地草案**（冻结候选）：`docs/TCP栈设计草案.md` · `docs/P6-A8网络服务化设计草案.md` · `docs/G-eco编译侧设计草案.md` · `docs/4-6出口演示设计草案.md` · `docs/app-0实施设计草案.md`。

## 语言策略与 syscall 边界（全 Rust）

| 层 | 形态 | 备注 |
|----|------|------|
| 微内核核心、HAL、引导早期 | **Rust `#![no_std]`** | 不链接 std/alloc；自带分配器；`panic=halt`；汇编用 `core::arch::asm!` / `global_asm!`（EL 检测、MMU、异常向量、上下文切换） |
| 用户态系统服务、库、工具链、用户程序 | **Rust** | 服务 `#![no_std]` 起步，高层工具可上 `std` |
| **syscall 边界** | **C-ABI 约定**（两端皆 Rust） | Rust ABI 不稳定，故 syscall 入口/数据结构用 `extern "C"` + `#[repr(C)]`；内核与用户态都是 Rust，无跨语言 FFI 风险 |

约定：
- **`unsafe` 策略**：`unsafe` 收敛到小块——页表、上下文切换/寄存器保存、MMIO、分配器、硬件原子操作；外层包安全 API。内核 = "安全 Rust 外壳 + 小 unsafe 核"。
- Rust 侧绝不持有跨 syscall 边界的引用生命周期假设；跨越边界的数据按拥有的/复制的传递。
- 公共 ABI（`berry_*` + 结构）用 `#[repr(C)]`，内核与 `berry-sys` 镜像同步（见 `系统调用ABI设计.md`）。

## ARM64 目标约定

- 默认目标：`aarch64-unknown-none`（内核 + 用户态 no_std）；如需特殊段布局用自定义 `target.json` + linker script。
- 内核运行在 EL1，用户态 EL0；**启动入口 EL 随 QEMU 配置而变**（默认 `-machine virt`+`-kernel` ELF 起始 EL1；`-machine virt,virtualization=on` 则 EL2），EL 检测/降级在 `arch/aarch64/` 集中处理（`mrs CurrentEL` 判别 → 按需 eret 到 EL1）。异常级别切换亦在此。
- 调试与验证主用 **QEMU `qemu-system-aarch64`**（virt 机型）。

## 构建与测试

**启动输出速览**（标记清单单一事实源 = `tools/smoke.sh`；标记沿革逐字归档 → `docs/阶段沿革.md`；**标记语义注解与命令注释全文快照 → `docs/雷册-判例库.md` §C**）：

- `just run` / `just run-el2`：裸相 27 标记（行首 1/21 = 起始 EL）——`BerryOS boot` → `KVA ok` → `ROOTFS ok/absent` + `ROOTFS vmo=` 双行 → ktest 各段 `… ok` → BENCH 六行 + `BENCHCHURN ok`/`BENCHXH ok` 金丝雀 + `BENCH ok n=6` → 稳态矩阵（`switches=N` 金丝雀严格递增）→ watchdog `G1.5 ok` → `BENCHWAKE` 直方图。
- `just run-smp`（S-1 起 bring-up + **S-2b 起副核参与态**，**全仓唯一 -smp 入口**）：`SMP: cpu_on entry=<hex>` → `sSMP: sec up el=1 …`（行首 `s` = 副核哨兵）→ `SMP ok ncores=2`（心跳状态谓词）→ S-2b 起 +`PAR gate on`（参与门自证——副核探测/参与环开始拉 `pinned!=0` 用户任务，裸相无货时空转；`PAR ok`/`PAR absent d1=` 行由 par_probe 60s 窗打出）；单核/EL2 机型/EFI 三降级路径 boot 全标记零扰动。
- `just run-rootfs`：rootfs 相 45 标记——svcmgr 消费链全谱：`ECO ok`…`APP ok n=2` + `FS`/`VIRTIO`/`VBLK`/`BLKD`/`FAT`/`FATW`/`NET`/`DNS`/`TCP`/`TCPS ok` + C 系 `DIR`/`CD`/`U1`–`U4`/`B1`/`DF`/`TAB`/`INT`/`PS`/`BC`/`M3DEMO` + T-1 `THR` + A 系 `POLL`/`LSN`/`EVS`/`SKT`/`EPL`/`NETS`/`MMP`/`CLK`/`SIG`；A 系排 fat 族前 + netd 拓扑降级双门（netd_h()==0/ENODEV 静默跳过）；裸跑双行降级（`ROOTFS absent`/`vmo=none`）零扰动。逐标记语义 → `tools/smoke.sh` + 雷册 §C。
- `just smoke`：八跑 CI 冒烟——裸相 27 + rootfs 相 45 + RX 注入相（rootfs+FIFO 合相 72，含 Ctrl-C 杀段）+ FAT 子目录相 + FAT 持久化相（脏盘跨 boot 读回 f42t + `cat T3.TXT` 断言 x1y）+ B4 掉电相（独立盘三 boot 采样 + `pwrcls` 毒环注入，guest 标记与宿主盘面双面不变量）+ 金丝雀严格递增；`SMOKE_KEEP_LOG=1` 留日志。`smoke-x86`（7 标记）/`smoke-smp`（双相）独立关卡不并入主 smoke。
- `just run-efi`：AAVMF 挂 `out/berryos.img` 自举——BerryBoot 十段（`fw:`/`gop:`/`fs:`/`kfile:`/`init:`/`elf:`/`load:`/`mmap:` → `BERRYBOOT ok`）→ 内核接续（`HANDOFF ok` → `MMAP ok` → `ROOTFS ok src=handoff`）→ 裸相 27 + EFI 路生态 27（无 virtio 面；LSN/EVS/SKT/EPL/NETS 拓扑门静默不打 = 拓扑真值；GUI-1 起 +FBW/FBSERVE 双路上屏）；盘形制 = MBR + FAT16 96MiB + 0xDA 预留槽 16MiB（`just image` 单口 + `manifest.txt` 侧产物）。十段全文 → 雷册 §C。

**前置（macOS，一次性）**：

```bash
brew install qemu just                          # qemu-system-aarch64 + just（brew 的 qemu 一包含全 arch 的 qemu-system-*）
rustup target add aarch64-unknown-none          # stable，拉预编译 core（无需 nightly/build-std）
rustup target add x86_64-unknown-none           # P7-x0 起 x86_64 最小集（stable 预编译 core，免 nightly）
rustup target add aarch64-unknown-linux-musl    # 用户态 std 面（stdhello/bsh/uu 家族与 .bap 应用同链——musl target + berry-libc shim + berry-ld；开发者文档 docs/应用开发指南.md）
```

**日常命令**（仓库根；justfile 已固化 `~/.cargo/bin` 到 PATH）：

```bash
just build          # = cargo build（内核 ELF，target=aarch64-unknown-none）；先自动跑 build-user 补产物
just build-user     # G-eco：用户程序编译 → 平像 + mkapp 打包 .bap + mkcpio 产 rootfs.cpio（run-rootfs/smoke rootfs 相输入）
just run            # QEMU virt 冷启动 → 串口打印 1BerryOS boot（默认起始 EL1），halt
just run-el2        # -machine virt,virtualization=on → 21BerryOS boot（强制 EL2→EL1 eret 降级路径）
just run-release    # release 构建 + 同样 QEMU
just build-release  # K-2 release 链接守卫（挂 CI，只构建不跑）：abi-check + build-user + cargo build --release + size.py --assert-flip——release 独有断链形态（fat LTO ADRP 引 .rodata.cst16 越 ±4GB）此前 CI 零覆盖
just run-rootfs     # G-eco：fw_cfg 挂 rootfs + virtio-blk/net——ROOTFS ok 双行 → svcmgr 消费链全谱（标记见上方速览）；每跑 mkfat 重造盘，跨 boot 持久化验证走 smoke 持久化相（echod 由 recipe 起/收）
just abi-check     # ABI 单源守卫：gen_abi.py --check 逐字节比对 abi.toml 三面产物；忘再生成/手改即 FAIL（挂 build/check/smoke 前置）
just abi-gen       # ABI 再生：abi.toml → 内核/berry-sys abi_gen.rs + 文档 marker 块（改 ABI = 改源 → abi-gen → diff 随源入册）
just smoke          # 无人值守冒烟（CI 同款）：八跑全相标记断言 + 金丝雀严格递增——相构成见上方速览；标记清单 = tools/smoke.sh
just build-x86      # x86_64 最小集构建（独立工作区 arch/x86_64/，**必须显式 --target**——根 config 锁 aarch64 且跨 workspace 边界仍命中）
just run-x86        # QEMU pc 机 + PVH ELF（-no-reboot）——七标记 PVH/TIMER/EXC/CTX/SYS/X86 ok（P7-x1..x4 四刀链）；退出 Ctrl-A X
just smoke-x86      # x86 独立冒烟（7 标记恰一次；不并入主 smoke；已挂 CI——ci.yml 两条 x86 步）
just run-smp        # 副核 bring-up + 参与态手跑（默认机型 + -smp 2——全仓唯一 -smp 入口；S-2b 起 gate 置位副核拉 EL0 任务）；退出 Ctrl-A X
just smoke-smp      # S-2b 副核参与双相冒烟（独立关卡不进 CI）：相 1 裸相 27 + SMP 握手 → 相 2 rootfs 生态 45 + EVS/SKT 宿主对拍 + PAR 活证（`PAR absent` 行 `d1=` 非零即副核 park 派发活证；`PAR ok` 提前命中放行）；`SMP ok` 按相各恰一次 + 金丝雀按相严格递增；时限帽按相（boot 240/canary 400/rootfs 840/PAR 300）+ 脚本尾打 R3 定标三数；`SMP: FAIL`/`SMP: skip` 出现即 FAIL；挂 build-user 依赖（缺 rootfs 产物 loud 快败）
just smoke-image    # V-4 img-boot 独立冒烟（M-G1 关卡，已挂 CI）：EFI 十段恰一次 + 接续 + 生态 27 + kfile/init 字节对拍 + FIFO 注入 RX ok + 活性复验；EFI 路无盘 = bsh park 在 fat 自验，交互杀段结构性不打（C-4b 裁定）
just build-boot     # BerryBoot EFI 应用构建链：staticlib → rust-lld 静态全链接（链入 berry-loader）→ elf2pe.py 转 PE32+；V-3 起纯 EFI 产链，消费内核产物
just image          # V-3 单口整盘镜像 → out/berryos.img（MBR + FAT16 96MiB + 0xDA 预留槽 + 四件套，同输入同字节）+ 侧产物 manifest.txt（SHA-256 清单 v0 冻结）；--key-hash 只落 manifest
just run-efi        # AAVMF（-pflash 对）挂 berryos.img + ramfb——十段引导 + 内核接续（详见上方速览）；退出 Ctrl-A X
just size           # 内核体积基线报告（文件量 / PT_LOAD 镜像与静态内存 / 段明细；CI 同步打印）
just bench         # B-0/B-1 基准单口（docs/基准与压力测试设计.md）：默认 B-0 裸跑收割 BENCH 六行 + 基线对拍（3x）；--el0 = B-1 EL0 四项；基线双段 --update 分段刷新
just bench-rt       # R-1 实时基准（bench.py --rt）：fw_cfg opt/berry/rt 门控，三档 δ × 32 样本 oversleep → BENCHRT + RT ok n=96；门控缺席零扰动；--update/--el0 组合拒绝
just bench-icount   # B-2 -icount 确定性通道（bench.py --icount）：周期数→指令数代理，独立基线 bench_baseline_icount.txt；L2 紧门禁由 --prove 五跑烘焙；跨机不可比
just soak [分钟]    # S-1 soak 长跑单口（soak.sh，默认 60）：fw_cfg opt/berry/soak 门控心跳 + 四断言（t/sw/pages/lost）；不进 CI（小时级）；勿与 smoke/run-rootfs 并行（echod 18080 冲突）
just check / just clean
```

要点 / 已踩坑：

- **产物不在 `./target`**：本机全局 `~/.cargo/config.toml` 设 `target-dir=~/.cargo-target`（sccache + 共享 target）。路径解析收敛在 `tools/paths.sh`（单一事实源，justfile/smoke.sh/CI 同源，内部 `cargo metadata` 动态取真实路径），无需关心。
- **QEMU ≥11.x 不接受 `-bios none`**（报 `Could not find ROM image 'none'`）；`virt -kernel <ELF>` 直加载即可，故 justfile / `.cargo/config.toml` 都不带 `-bios`。
- **QEMU halt 后不退出**：内核 `wfe` 死循环，串口打印完即停在那。退出 QEMU 用 `Ctrl-A` 然后 `X`（macOS 无 GNU `timeout`，自动化里用后台 + `kill`）。
- 调试：`-s -S` + lldb 单步；`-d cpu_reset -D qemu.log` 看启动 CurrentEL。
- 测试入口：`just smoke`（QEMU 无人值守冒烟，CI 同款——八相构成见上方速览；标记清单单一事实源 = `tools/smoke.sh`）+ `just test-host`（host 单元 665 测，不依赖 QEMU；mkapp 门禁自检挂前置，mkfat --check-fixture 同位；逐 crate 台账与沿革 → 雷册 §C，总口径以 `just test-host` 输出为准）+ QEMU 集成（`ktest` 框架，见 `docs/测试策略.md`）。

## Claude 在本仓库的工作方式

1. **先查参考**：涉及某子系统（IPC、调度、页表…）时，先看 `reference/` 是否已有对应对照项目，参照成熟实现再写。
2. **小步提交**：OS 调试极难，每个改动尽量小且可独立验证；能在 QEMU 跑通的里程碑单独成一个提交。
3. **不污染内核态**：内核 `#![no_std]`，禁用 `std`/`alloc`（在自有分配器就绪前）；用内核自有的分配器/日志；新增 crate 前确认 `no_std` 兼容。
4. **协议洁净**：主参考是 **Rust OS**（Redox / Theseus / zCore，MIT 可对照）；hubris = **MPL-2.0 语义可借、文件不搬**；rCore-Tutorial = GPL 只看思路；参照 seL4/Zircon/Linux（C/C++/GPL）时只看思路、不复制实质代码。本仓库许可证 Apache-2.0（ADR #37）。
5. **架构无关优先**：新功能先写成架构无关 Rust，确实需要特权指令时再落到 `arch/aarch64/` 并留好接口。
6. **博采众长不杂糅**：中心 = **可注入的「机制/策略分离」+ 硬件信任边界**；借鉴任何参考前先过 `架构综合纲领.md` 的**排除 8 问**（轴 PICK？冲突中心？Theseus 仅进程内？IPC 不统管？seL4 只锁不变量？hubris 静态只借 boot 层？GPL 不进？哪关 G0–G5 验证？）。
7. **改了约定就更新本文件**：新增目标架构、改变语言边界、确定构建命令时，同步回写 `CLAUDE.md` 与 `AGENTS.md`。**但历史/沿革类内容（阶段进展、标记流水账、战役复盘）只写 `docs/阶段沿革.md` 等文档，不回写本文件**——本文件每轮会话全文注入 AI 上下文，只承载「当前状态 + 铁律」（2026-10-22 实测：沿革回写曾把两文件灌到 328KB ≈ 数万 token，已归档瘦身）。

## 约束与陷阱（Gotchas）

- **内核 `#![no_std]`**：禁 `std`/`alloc`（分配器就绪前）；`panic=halt`。FPSIMD 已全量保存（GUI-0 起，TaskContext 640B、switch 点无条件 q0..q31+FPCR/FPSR）——用户态浮点/NEON 随切换自动保留；但内核自身 SIMD-free（三层合同：`-neon` 旗 + `kernel/src/mem.rs` mem*/bcmp 整数覆写 + ktest `FP ok` canary 每次 boot 执法）——内核新代码禁引入 FP/SIMD；core fmt 链 NEON 仅 panic(=halt) 路径可达属豁免。先问：内核新代码引入 FP/SIMD 了吗？ → 雷册 §A1
- **`unsafe` 纪律**：只在页表/上下文切换/MMIO/分配器/硬件原子用 `unsafe`，包薄安全层；绝不散落。 → 雷册 §A2
- **syscall 边界无 panic**：跨 syscall 的 Rust 函数返回 `Result`/状态码，绝不 panic（内核态 panic = 系统停）。 → 雷册 §A3
- ARM64 下注意 Cache 一致性、屏障指令（`dmb`/`dsb`/`isb`）、大端小端（默认小端）。 → 雷册 §A4
- **QEMU aarch64 `virt` 起始 EL 随配置而变**：默认 `-machine virt`+`-kernel` ELF 起始 EL1；`virtualization=on` 起始 EL2（走 EL2→EL1 eret）。boot 第一行 `global_asm!` `mrs CurrentEL` 检测并按需降 EL1（关 MMU/cache、写 `sctlr`、`eret`）——**别假设固定 EL**。先问：当前机型配置下起始 EL 是哪个？ → 雷册 §A5
- **QEMU 非 Linux ELF `-kernel` 不走 Linux boot 协议**：x0=0、RAM 无 DTB（0xD00D_FEED 零命中，LE 应读 0xD000_EDFE）、`-initrd` 不加载、fw_cfg INITRD_ADDR/SIZE 键空。blob 递送走 fw_cfg 具名文件 `-fw_cfg name=opt/berry/rootfs,file=…`（FILE_DIR 自描述；selector 收 BE u16 须整字 `write_u16`——MMIO 宽度敏感；data 口 u32 低字节 = 项首字节）。DTB x0 仅真机/Linux 协议生效（x19 预埋 `dtb::boot_probe`）；切 Linux Image 协议 = ADR 级重构留 v2。先问：blob 递送是否在赌 Linux boot 协议？ → 雷册 §A6
- **PAC/BTI/MTE 目标 day-one 开，但 Phase 0/1 推迟**：`cortex-a72`（ARMv8.0）无此扩展；v2 切 `-cpu max` 时三档分刀（SEC-01 C4）：BTI≈零改动 / PAC=key 管理小刀 / MTE=分配器中心点刀（需 `-machine virt,mte=on` + slab/buddy 改造）——旧「源码零改动」仅对 BTI 严格成立（见 `安全与硬件隔离设计.md` §3/§6）。先问：这个安全扩展在当前 `-cpu` 下存在吗？ → 雷册 §A7
- **QEMU TCG 下 CNTV 实测 tick 率 ≈ SLICE_HZ/2**：`CNTFRQ_EL0`=62.5MHz、SLICE=`FREQ/SLICE_HZ`=6.25M 配置正确（`SLICE_HZ=10` 应得 10Hz）但实测抢占 ~5Hz——TCG `virt`/`cortex-a72` 下 `CNTVCT` 推进速率 ≈ `CNTFRQ/2`。**非内核 bug**：真机或 `-icount` 应为 10Hz。先问：tick 率减半是内核 bug 还是 TCG 2x 效应？ → 雷册 §A8
- **EL0 读 CNTVCT 需 CNTKCTL_EL1.EL0VCTEN=1（复位 0 = EC=0x18 陷阱，B-1）**：没开 = user_fault_exit 静默杀（退出码 0 + 无 EOF）；EL2 场另需 CNTHCTL_EL2.EL0VCTEN。先问：新用户态代码读系统计数器，这扇门开了吗？ → 雷册 §A9
- **2b-1 起内核主体链在高半（VMA = `KERNEL_VA_BASE` + LMA 镜像）**：符号/PC 显示 `0xffffff80…` 而镜像实按 p_paddr 落低半——看布局跑 `python3 tools/size.py --assert-flip`。**ADRP 只有 ±4GB、双向坑**：低半 `.text.boot` 引用不了高半静态量（4 张 boot 表放 `.bss.boot` 低半）；高半代码取低半绝对符号地址同样越界（mm.rs 导入高半 `__kernel_end` 运行时减基址）。先问：这次跨半引用 ADRP（±4GB）够得着吗？ → 雷册 §A10
- **2b-2 起低半已退役**：MMIO（GIC/UART）一律走高半别名（`KERNEL_VA_BASE + PA`），**PA 直访必 fault**——唯一例外 = boot asm 开 MMU 前早期打印直写 PA；内核空表 `NULL_L1` 放普通 `.bss`（高半 VMA），TTBR0 写入按 `VMA - KERNEL_VA_BASE` 折算 PA。先问：这个 MMIO/符号访问走的是高半别名还是 PA 直访？ → 雷册 §A11
- **用户页表叶必须 nG=1，全量失效用 `tlbi vmalle1is`**：`tlbi aside1*` 按号失效**够不着 global 项**（boot 块描述符全 nG=0）——用户叶不标 nG 则同 VA 映射跨 ASID 经 TLB 泄漏；退役/回卷清 global 项只能全量 `vmalle1is`（`aside1all` 不是真实指令）。先问：要失效的项里有没有 nG=0 的 global 项？ → 雷册 §A12
- **2b-4 起 EL0 异常走 lower-EL 向量 0x400（sync）/0x480（IRQ）**：帧宏 `save/load_frame_el0` 与 current-EL 版仅 `sp` 槽差异（存取 SP_EL0 = 用户 SP；异常自 EL0 入 EL1 硬件强制 SPsel=1，帧仍落内核栈）。不变量：**EL0 运行期间 SP_EL1 留着本任务内核栈位置**（trampoline/上下文切换装入），lower-EL 异常入口才能精确落回该栈。先问：异常入口落栈是不是依赖 SP_EL1 预置？ → 雷册 §A13
- **3-1 起 syscall 错误一律错误码自理、永不杀调用者**：未知/越界/未实现编号 → `-ENOTSUP`，坏 handle → `-EBADF`，EL0/EL1 同权（ABI §8.0「永不 panic」的内核执法）；杀只留给 CPU 故障（D-abort/I-abort/BRK 走 `user_fault_exit`）。用户页 AP=0b01 且未设 UXN = **用户页当前可执行**（NX/只读档留 v2）。先问：这个 syscall 失败路径返回错误码还是杀调用者？ → 雷册 §A14
- **内核字节串 `b"..."` 只能 ASCII**：塞中文直接编译错，且下游 crate 可能耗旧 rlib 报**误导性错误**（如 cannot find xxx in crate berry_sched）——莫名符号缺失先查上游 crate 真实编译输出；中文文案走 `println!`/`uart_puts(str.as_bytes())` 路径。先问：内核 `b"..."` 里是不是塞了非 ASCII？ → 雷册 §A15
- **2b-4 起时间片按周期数记账**（`slice_left` 存 CNTVCT 周期、tick 按增量扣减）：TCG 的 2× tick 率效应原样继承（观测仍 ~5Hz/slice），行为与 tick 计数版一致。 → 雷册 §A16
- **`FrameAllocator::alloc()` 返回 PFN 不是 PA**（PA = pfn << PAGE_SHIFT），且帧内容不保证干净（buddy 只管元数据）——新页先清零再用。 → 雷册 §A17
- **3-2 起映射中 VMO 的「销毁性 close」拒 -EBUSY、别名 close 放行**：仅 refs==1（本次 close 会 drop 对象）且 mapcnt>0 才拒；dup 出的别名 close 不死对象、正常放行——写成「映射中一律拒」会误伤合法别名链（RO dup→close RO）。decommit 同理拒映射中页；`Vmo::Drop` 对映射中页只泄漏。先问：这次 close 会销毁对象吗（refs==1 且 mapcnt>0）？ → 雷册 §A18
- **syscall 主体不可抢占 = v1 免锁根基**：SVC 异常入口硬件置 DAIF 屏蔽 IRQ 且 handler 全程不解蔽 → 引导表/对象访问单核串行；**任何新 syscall handler 不得解蔽 IRQ、不得忙等跨任务**（3-3 拆 per-process 表前都靠这条）。先问：新 handler 解蔽 IRQ 或忙等跨任务了吗？ → 雷册 §A19
- **`vmo_create` v1 上限 32MiB（`VMO_MAX_SIZE`，C-2c 起 16→32）**：巨 size 的 `Vec` 分配 abort 是 EL0 可触发 panic（违 ABI §8.0），故超限先拒 -ENOMEM——新 syscall 凡按用户 size 做分配的都要照此设上限。先问：这个 syscall 按用户 size 分配前设上限了吗？ → 雷册 §A20
- **内核堆大对象 = 单块连续 buddy 块（heap.rs 整页路径，`2^order` 对齐）**：>16MiB 量级一次性分配在碎片化地形结构性失败（EFI 路固件保留区 + init blob 自占下 21.3MiB rootfs 的 order-13 块不可得；fw_cfg 冷启 RAM 完整属侥幸可达）。boot 期大缓冲优先**借用已在 DRAM 的源**（handoff/dtb 零拷贝 `RootfsBuf::Borrowed`）或逐页 VMO commit（物理不连续无碍）。先问：给这条路径写大 `Vec` 前问过内存地形吗？ → 雷册 §A21
- **user_good 的 3-2 验收链是一次性的**（进稳态循环前跑一遍）：14 位代号经不起循环 create/close 耗 gen；改链时记住 `movz x3,#0x2018` 槽基会被 commit 步的 len 参数借走、须重载。先问：改 ktest 验收链时，被借走的寄存器槽重载了吗？ → 雷册 §A22
- **spawner 内嵌子代码的 `movz x4, #<len>` 必须精确等于子模板字节数**：改子模板后重数指令（每行字符串 = 1 指令 ×4B）再改常量——拷多带尾随零垃圾、拷少截断尾部，两种错都静默（`vmo_write` 只管拷不管语义）；ktest 的 `svc6` 是 nr+6 参共 7 形参——5 参 syscall（`channel_send`/`recv` 等）末尾要补 0 占位。先问：子模板改了，len 常量重数了吗？ → 雷册 §A23
- **用户模板 svc 后复用参数寄存器，先想清值是什么（3-4a）**：派发只回写 x0，x1..x6 保留的是**你传进去的参数值**——vmo_read 后拿缓冲地址要用 x2（buf 参数），拿 x3 得到的是 offset（=0）。套路口诀：哪个寄存器装的什么，svc 前后一个样。先问：svc 后读的寄存器里，装的还是你传的参数值吗？ → 雷册 §A24
- **3-3 起内核对象间跨界引用必须 `ObjRef::retain/release` 钉住**（Thread 钉 Process、Process 钉 root Vmar、运行中任务钉 Process）：只存裸指针 = 父 handle 关光后 use-after-free；Drop 链（close→drop Thread→…→release Vmar）纯指针不回表、无重入。另一边界：ktest 在 `berry_sched::init` 之前跑——会 spawn 的成功路径（如 `process_start`）不能在 ktest 验，只能盖参数/归属/状态错误面，happy path 交 EL0 用户进程 + watchdog 错杀断言兜底。先问：跨界引用钉住了吗？ → 雷册 §A25
- **用户模板可用 `adr` 取同页内嵌数据**（3-3 spawner 的子代码字节）：自相对寻址整块搬迁后偏移不变，与「仅局部数字标号 + movz 绝对地址」的拷贝安全约定同源——但仅限**同页**内（adr ±1MiB 且不得跨模板边界引用）。先问：这个 `adr` 引用是否出了同页/模板边界？ → 雷册 §A26
- **ktest B 段「未实现真号」样本勿冒充（T-1）**：0x35 等在册号真语义全实现后 B 段仅剩未知号（0x99）-ENOTSUP 断言；勿拿不在册 0x38/0x39 冒充「未实现真号」（其 -ENOTSUP 成因与未知号相同）；「在册未实现」覆盖点 = SEC-01 件 11 起 0x85 `berry_attest`（ktest 样本挂 kernel 解冻后立）；新 NR 常量入册若暂未实现再立新样本——勿拿已实现号（0x33 直接杀掉 ktest）、勿拿 0x99（成因不同）。先问：这个样本号是真「在册未实现」吗？ → 雷册 §A27
- **`object_wait_one` 阻塞化（4-1）**：poll-first 命中即返；未命中且 deadline≠0 睡 SLEEPQ 至绝对时刻，醒来重验；到点 -ETIMEDOUT、`deadline==0` 仍 poll（-EAGAIN）；命中回写全满足集（current 非请求子集）；等待不需权利位；`object_signal` 仅 Event、`wake_matching` 键=ObjRef 地址（同对象跨表 handle 互通）。睡眠钉住：全程 `ObjRef::retain` 一次、唯一出口 release——全表关光不销毁；逐轮 retain/release 归零再循环即 UAF。先问：retain/release 唯一成对吗？ → 雷册 §A28
- **信号位空间分半（4-2）**：低 32 位=内核派生（`SIGNAL_READABLE`/`SIGNAL_WRITABLE`，Channel wait 面）、高 32 位=用户（Event `object_signal` 专用）；clear/set 掩码只收用户半，越出 `SIGNAL_USER_MASK` 一律 -EINVAL（检在 get_raw 后，坏 handle/类型仍 -EBADF 优先）；清零掩码用用户半全集 `0xFFFF_FFFF_0000_0000`（旧 `!0` 非法）；模板/ktest 用户位一律 `1<<32` 起编；wait 低 32 位请求 READABLE/WRITABLE 外的位永不命中。先问：掩码越出用户半了吗？ → 雷册 §A29
- **用户模板尾部内嵌数据（`.ascii` banner 等）后必须 `.balign 4`**：`UserProcess::build` 断言模板长度 4 字节倍数——banner 9 字节直接 boot 期 KERNEL PANIC（4-2a）；长度恰为倍数也保留该行（纪律一致性）。先问：模板改完后长度是 4 的倍数吗？ → 雷册 §A30
- **多对象等待挂 `WAIT_ANY` 哨兵、不挂真实对象地址（4-2b）**：Task 只有单一 `wait_obj` 匹配键，`wait_many` 睡眠传 `berry_sched::WAIT_ANY`（usize::MAX）+ 全位掩码——`wake_matching` 对哨兵者「任何对象、任何新置位」都命中，醒来由 syscall 层重验循环定夺（虚假唤醒无害）；纯清位（set_signals==0）不误醒哨兵者；数组出参 `*idx` 预清 u64::MAX = 未命中哨兵值。先问：多对象等待挂的是哨兵还是真实地址？ → 雷册 §A31
- **smoke 标记不得建立在时序竞速上（4-5a）**：标记条件必须是状态谓词而非竞速结果——队空/非空、排水速率都是竞速；必要时入口先发一次 wait_many（队空睡到首发/非空即中）保证两路必然命中。先问：这是状态谓词还是竞速结果？ → 雷册 §A32
- **boot 直插 handle 定值可被模板硬编，但 boot 必须断言（4-2b/4-5a）**：main.rs `assert_eq!` 拦表序漂移——错值 → wait_many -EBADF → 自陷被杀 → 标记缺失 = smoke FAIL（boot 期炸响而非静默错 handle）。定值：4-2b ponger 表第 2 插=槽 2=0x0001_0002；4-5a endU good/waiter/pinger=0x0001_0002（槽 2）、ponger=0x0001_0003（槽 3）、abuser/spawner=0x0001_0001（首插，经 spawn arg x0）。先问：模板硬编的 handle 定值有 boot 断言拦着吗？ → 雷册 §A33
- **PI 共享表粒度级联饥饿（4-5a）**：proc_tag=0 内核任务睡共享表 → 睡 channel 派生位（READABLE/WRITABLE）即 donor，受捐覆盖所有持 peer 端点的睡眠者；叠跨层严格优先可把 prio-0 常驻任务永久饿死。新常驻内核服务两道防线都要上：① `spawn_with_tag` 挂 leak 私有表；② 稳态等待勿 wait_one 直睡 channel 端点——wait_many（WAIT_ANY 哨兵 → `pi::donor_edge` 返回 None）+ 永不置位哑 Event 对照项退出 donor 面；高优 client 阻塞服务满的真实受捐不受影响。先问：常驻内核服务睡了共享表或 channel 派生位吗？ → 雷册 §A34
- **`wake_matching` 返回计数勿当收工判据（4-2b 哨兵起）**：WAIT_ANY 等待者匹配**任何**对象的唤醒——返回值混入误醒票不再可信；waker 类任务收工判据用被唤方置的旗标（sleeper 侧 `BLOCK_BOOT_DONE` 模式）。先问：收工判据靠返回计数还是被唤方旗标？ → 雷册 §A35
- **调试槽 0xFE/0xFF 已退役（4-5a）**：回落 -ENOTSUP（ktest A 段断言 -95）；用户打印一律 `channel_send(endU)` 经 console 服务落串口（-EBUSY/-EAGAIN 全参重建立重试）；新内核侧调试直接 uart 打（EL1 语境），勿复活调试槽。先问：这个打印走 console 通道还是 uart？ → 雷册 §A36
- **遥测环/切换钩子写入零分配**：`telemetry.rs` 静态预储环 + `sched.rs` `note_switch` 三切换位是 IRQ 屏蔽热路径——OOM-panic 面不得从这里重开，新钩子须「零分配、零切换、短平快」。`duplicate` 到零权利被表拒（-EACCES，空 rights 拒面）——「只读观察」handle 得 dup 到非零权利位（如 DUPLICATE）再靠「wait 不查权利」观察。先问：新钩子在屏蔽窗里会分配吗？ → 雷册 §A37
- **`block_current` 续行时 IRQ 仍屏蔽（4-0）**：切回点在唤醒方屏蔽窗/IRQ 窗口内，之后无自动解蔽点——SVC 语境勿解蔽（eret 自然恢复 SPSR，syscall 主体不可抢占免锁根基）；内核任务语境续行后自行 `enable_irq`（与 `yield_now` 的 disable→switch→enable 对称包络）。先问：这个续行点在哪种语境、谁负责解蔽？ → 雷册 §A38
- **`SleepQueue::unlink` 不幂等（4-0）**：非环形双向链表无法廉价判「在队上」——对不在队者调用会误清 head/tail 扬掉整队；block/wake/expire 三个调用点都由屏蔽窗契约保证在队。先问：调 `unlink` 前有屏蔽窗契约保证它真在队上吗？ → 雷册 §A39
- **带内核派生位（READABLE/WRITABLE）的等待 ⟹ `wait_obj` 必须是真 channel ObjRef（4-3b）**：`pi::donor_edge` 靠这条不变量直接解引用 wait_obj——合成对象睡它（含派生位）即解引用 1 mod 8 地址对齐 panic；合成/非对象等待一律只等**用户位**（内核永不置位）；现役位集中登记在 `kernel/src/main.rs` 的 `USER_SIGNAL_BITS` 表（`1<<32` 起编），新增用户位必先登记该表（编译期去重断言执法）。先问：这个新用户位登记进 `USER_SIGNAL_BITS` 了吗？ → 雷册 §A40
- **intent 传播的数值序（4-3c）**：u8 越小档越高（0x00 UNSPECIFIED 顶），「借入取 max」= 数值 `min`；每消息重导 `eff = min(declared, msg_intent)` 非单调，勿按历史 max 累积；三道帽超顶降为不传播（非 kill）；转发降级≠自身回落：slow 超预算 send 装自己 declared、自身 `intent_effective` 保持，仅 recv 结算改档；`channel_create` 4-3c 起 options 首参，旧签名静默错位；ktest 超帽样本=伪造消息直压收端队列；`Task::new_kernel` 返回 `Box<Task>`。先问：结算按历史 max 累积吗？ → 雷册 §A41
- **fastpath 检查链含「全队无哨兵」链项（4-4）**：`find_single_waiter` 遍历中撞到任一 WAIT_ANY 等待者立即 None——哨兵者被任何非零唤醒命中，单点唤醒集合 ≠ `wake_matching` 全匹配集，快路漏醒哨兵 = missed wakeup；多匹配者同理退 slowpath（唤醒集恒等 =「不长第二套」执法点）。先问：快路唤醒集合与全匹配集恒等吗？ → 雷册 §A42
- **`try_preempt` 严格更高才切（4-4）**：直切=选者 `prio_effective` 严格高于当前——同层 FIFO 不因 IPC 插队；协议同 `block_current`（屏蔽窗内可切走、切回续行、IRQ 勿解蔽），授片即 `arm_slice` 重编。幻影出选防线：`pick_next` 非纯查询（选中即 `advance_past` 耗 head）——「先 pick 再判」白耗 head、锁死出选相位 → 结构性饿死；故以 `Runqueue::has_eff_above` 只读探峰前置，不可能切就不 pick；「pick 了又不用」同罪（tick `next==cur` 自耗合法——cur 真被服务过）。先问：会不会 pick 了又不用？ → 雷册 §A43
- **ktest 绝不可在手搭睡眠者在队时走 SVC send（4-4）**：fastpath 会 `fast_wake` 把睡眠者塞进 `sched::init` 前未启用的 RUNQUEUE + `try_preempt` 解引用 null CURRENT = 挂死；SVC 错误面只在 SLEEPQ 清场后测（慢路无等待者）。量 IPC cycle 用专用点对点 bench 对（boot 手搭通道 + 高优任务对 fa_ping/fa_pong），勿拿稳态矩阵当基线（console 排队噪声）；bench 高于矩阵 prio 独占 CPU = 与时序敏感任务（10s 帽、intent probe）零纠缠。先问：测 SVC 错误面时 SLEEPQ 清场了吗？ → 雷册 §A44
- **常驻内核服务必须挂私有 handle 表（4-5）**：PI 受捐按持有表反查，共享 `BOOTSTRAP_HT` 上的睡眠者视作持有表内一切 handle（v1 粒度）——常驻服务睡共享表 = 全体睡眠者错捐级联 + prio-0 饿死。挂法：`Box::leak` 私有表 + `spawn_with_tag(entry, arg, name, prio, 表指针)`；`caller_table` EL1 分支 tag≠0 走私有表，tag==0（ktest/boot/短命 bench）仍共享零扰动；自证：`task_svc_tag` 双断言（私有表 poll → -EAGAIN，共享表 → -EBADF，打 `TAGTAB ok`）。先问：常驻服务睡共享表了吗？ → 雷册 §A45
- **长命睡眠内核任务不睡共享表——错捐自伤形态（4-6）**：被错捐高 eff 的睡眠者自己每轮醒来以严格优先抢片，把自己的断言对象推过自身重试预算（自己就是延迟源）。修法 = 步进重试的验收司机也挂私有表（表内仅 E_rel 一个 Event = 零受捐面）+ 预算按「最坏轮转 + 自己也在偷片」放宽（64；条件驱动重试、窗口由被等方 deadline 帽敞开——不赌竞速）。halt 之后「全系统冻结」别当调度器 bug，先查 panic 尾巴——排队中标记全缺 = 单点 halt 的连带面。先问：步进重试者的受捐面清零、预算含自己偷的片了吗？ → 雷册 §A46
- **`for_each_sleeper` 契约 = IRQ 屏蔽窗内调用**（sched.rs 文档注释）：tick 超时摘队会动睡眠链接、遍历中接线即扬队；任务体经 SVC eret 后 IRQ 已解蔽——新消费者须照 ai_probe 手动 disable/enable 包络扫描窗。先问：调用点有没有手动屏蔽 IRQ 包络？ → 雷册 §A47
- **用户程序编译链 stable 免 nightly（G-eco-a，勿回退 custom target）**：builtin `aarch64-unknown-none` + build.rs 绝对路径注入 `link-user.ld`（0x1000 基址=USER_CODE_VA，相对路径随 CWD 漂移）+ `-zmax-page-size=4096`（rust-lld 默认 64K 把 PT_LOAD 抬到 0x10000）；custom target JSON 推迟确需时再上；std 面走 musl 同免 nightly。`tools/elf2flat.py` 取整按相对长度，`--max` 是单页墙执法点。先问：link-arg 用绝对路径了吗？ → 雷册 §A48
- **`kernel/src/userbins/*.bin` 是 gitignored 编译产物（G-eco-a）**：内核 `include_bytes!` 直嵌，缺产物 = 裸 `cargo build` 编译期炸响（loud 非静默回退）；`just build`/`check`/`build-release` 已挂 build-user 自动补产。改用户程序源码在 `user/bins/*`（good/spawner/waiter/pinger/ponger 仍是 `kernel/src/user.rs` 的 asm 模板——两套产源并存）。先问：改的是源还是 gitignored 平像产物？ → 雷册 §A49
- **asm 模板换轨时重试形态（自旋 vs 阻塞）是契约的一部分（G-eco-b）**：同一家族两种形态并存（asm svcmgr console 发送 `b 6b` 纯自旋 vs talker EBUSY→wait WRITABLE 阻塞），按家族归纳会混记——换轨前逐发送点核对 asm 标号的 EBUSY 分支去向（`b Nb` 回环自旋 vs clock+wait 阻塞）。TCG 启动期 console 队列饱和窗：阻塞者醒来须等一整圈轮转（~16 任务 × 200ms）、自旋友商持续抢槽 → 三场竞速即耗尽 10s 帽 -ETIMEDOUT；自旋者由轮转兜底必达（EL0 可抢占 = 自旋有界）。先问：换轨后每个发送点的 EBUSY 分支形态对齐原契约了吗？ → 雷册 §A50
- **console 通道纯自旋 send = 长跑饱和下结构性饿死（雷 2：自旋赢 boot 竞速、输长跑公平）**：自旋者从不阻塞 → 永不出 runqueue → RR 环序冻结、drain 窗被环相位占优者垄断 → 饿死。新用户程序打印一律阻塞形态：EBUSY → clock_read + wait_one(console, WRITABLE, now+10s) → 重发，-ETIMEDOUT 重臂，其它错照旧自陷；阻塞者经 recv 侧 `wake_matching(WRITABLE)` 公平分槽（svcmgr 豁免亦 P5 A-2 撤销）。先问：打印点遇 -EBUSY 是阻塞等 WRITABLE 还是自旋？ → 雷册 §A51
- **wait_many 恒满足项使常驻服务永不入睡 = drain 相位钉死 RR 环（P5 A-2，雷 2 姊妹坑）**：等待数组里「对端队列永不满 → WRITABLE 恒真」的通道项（无接收者的转发端）使 poll-first 每次立即命中 → 服务从不进 SLEEPQ → drain 相位冻结在 RR 环固定节点、环序固化 → 填槽序末位者结构性饿死（连自旋都不是必要条件——但自旋者最先死）。修法 = 恒满足项条件化入列：仅当确有待处理状态（plen>0）才入列该项，常态少一项真睡。先问：这项在稳态下会不会恒满足？ → 雷册 §A52
- **两遍构建 + rerun-if-changed 全列纪律（G-eco-b）**：`include_bytes!` 内嵌平像由 pass 1 产出（`user/userbins/` 中转区），单遍 workspace 构建无顺序保证；build.rs 须显式 rerun-if-changed 盖 link-user.ld + 自身源 + 全部 staged 平像——任何 rerun-if 指令都替换 cargo 默认全量重编，漏列 = 改了子程序、内核还嵌旧字节静默上线。先问：每个被内嵌的产物都在 rerun-if-changed 清单上吗？ → 雷册 §A53
- **elf2flat 平像页补零——磁盘 .bin 是页倍数（G-eco-b/D3）**：铺平 PT_LOAD 后补零盖 .bss 并取整到页；`vmo_write(len)` 传平像全长（含补零，零字节无害）。`--max` 是墙执法点：svcmgr `--max 131072`（= D3 `USER_CODE_MAX` 128KiB），单页 bin `--max 4096`；多页 spawn 的 sp 读 `UserProcess::stack_top` 字段（data/stack 随 code 页数上移），别读单页常量。先问：多页 spawn 的栈顶是从字段算还是沿用单页常量？ → 雷册 §A54
- **SVC 寄存器映射：x0 = 号、x1..x6 = 参、返回只回写 x0（G-eco-b）**：dump 里 x1..x3 即 wait_one 的 (handle, signals, deadline)；berry-sys 已按此镜像。诊断用户态死亡先按寄存器反推卡在哪个 syscall 的哪个参数，再按 dump 顺序对时间线。先问：死因能从寄存器反推到具体 syscall 参数吗？ → 雷册 §A55
- **std 程序 bare `abort` 静默死 = libc 桩被启动关键路径判死（G-eco-d d-1c）**：std `sys::init`→`sanitize_standard_fds` 先 `poll(3)` 验 fd 0/1/2，非 EINTR 失败直接 `libc::abort()`——不走 panic、零文案。新桩若可能被 std 启动路径引用，先查 std 对失败是否 abort，能谎报成功就谎报。排障：bare `abort` 先 grep berry-libc 自家打印认领；audit 构建（strip=false）+ llvm-objdump 找 `bl abort` 调用点。先问：新桩在 std 启动路径上失败会触发 abort 吗？ → 雷册 §A56
- **syscall handle 出参宽度 = 4 字节（`copy_out_u32`，G-eco-c）**：ABI §8 冻结 handle=u32——内核写 handle 出参一律 `copy_out_u32`（`kernel/src/syscall.rs`），按 u64 写 8 字节会砸穿调用方 4 字节槽的邻接局部；asm 模板手排槽碰巧免疫——编译用户程序 = ABI 宽度类 bug 的检测器。非 handle 出参（ns/size/vaddr/nbytes/idx/observed = usize）仍 8 字节：宽度跟 ABI 类型走，不跟「凑巧能跑」走。先问：这个出参宽度是按 ABI 类型定的吗？ → 雷册 §A57
- **真实链接器的 PT_LOAD 不页对齐（G-eco-c/D5）**：`off ≡ vaddr (mod align)` 即合法（RO 段可与 text 共享页）——解析器（`berry-loader`）不收页对齐；消费端（svcmgr `spawn_elf`）用页铺平单 VMO 消化（逐段独立 vmar_map 会撞已映页）。cpio 容量 = `vmo_get_size` 页对齐含尾零页——walk 靠 `TRAILER!!!` 自终止，别假设缓冲尾巴是干净数据。先问：装载器把页对齐当默认合法条件了吗？ → 雷册 §A58
- **std 程序栈按程序面给 + 映像/栈之间必设保护页（G-eco-d d-3c）**：栈页不足时下行栈会静默扎进映像 RW 页腐化 .bss，死状与死因南辕北辙。修法：`spawn_rootfs_elf` 栈页数参数化（hexyl 32 页 = musl 128KiB 默认）+ `stack_va = base+len+PAGE_SIZE` 隔保护页——栈穿 = 干净 D-abort，dump SP/FAR 直指祸首。先问：给这个程序的栈够用吗？映像与栈之间有保护页吗？ → 雷册 §A59
- **多分片用户输出不可直接当 smoke 标记（G-eco-d d-3c）**：>192B 输出经 console 通道分片，稳态矩阵打印会在分片边界撕行——grep 表行 = 时序竞速。确定性验收面 = svcmgr 私有 stdout 通道收齐（libc `park` 先补发零长 EOF）→ 与内嵌宿主基准逐字节比对 → 才打单消息原子标记（`HEXYL ok` 模式）；全表 relay 到 console 只是化妆面。先问：这个标记的判定文本可能被分片撕行吗？ → 雷册 §A60
- **模板握手信号用专用位、清位豁免——模板不烤调度时序假设（P5-0）**：一次性无确认 signal 与对端多段前置窗口天然竞速，别把调度时序当契约；修法 = 握手专用位 U4=1<<38 + 全部清位掩码豁免该位（`#0xFFFF`→`#0xFFBF`）+ observed 断言前 `bfc x8,#38,#1` 掩掉——任意相位皆成立。现役用户位：1<<32 哑Event / 1<<33 SLEEP_OBJ / 1<<34 ktest·AI_EV_DONE / 1<<35 U3 / 1<<36 E_rel / 1<<37 U2 / 1<<38 U4 / 1<<39 RX Event——新增用户位前先查碰撞。先问：模板里烤了调度时序假设吗？新用户位查过碰撞吗？ → 雷册 §A61
- **GIC SPI 段 ITARGETSR 复位 0 = 静默不投任何核（P5 A-1）**：PPI 段只读、复位即投 cpu0 的惯例不可照搬——SPI 段可写且复位 0，不真写 target 则中断 pending 挂着永远无人收（完全静默、不炸不 dump）。新 SPI 源一律走 `gic::enable_spi(irq, prio, target, cfg)`（prio/target/config + 清 pending + 使能一次齐）。先问：新 SPI 源真写过 ITARGETSR target 吗？ → 雷册 §A62
- **电平触发源 handler 去活电平必须在 EOIR 前 + `exception_irq` 按 INTID 分流（P5 A-1）**：PL011 RX = drain FIFO 到 FR.RXFE（不排空 → EOIR 后线仍高立即重投 = 同优先级活锁、timer 饿死；与 CNTV「rearm CVAL 在 EOIR 前」同一条铁序）。tick 槽（`on_irq_tick` 授片/slice 记账）只对 CNTV 调——新源进 tick 路径会把授片与 CVAL 绝对编程脱钩；新中断源加 match 臂，未知源首 4 个 loud `irq? id=N` 后静默。先问：新源的去活电平在 EOIR 前？走对分流臂了吗？ → 雷册 §A63
- **PL011 FEN=0（复位默认）有效 FIFO 深 1 字节（P5 A-1）**：v1 不写 LCR_H——真机突发第二字节在 IRQ 延迟窗内即 overrun 丢字（QEMU 无此面：唯一丢字向量 = 内核环满 `lost`）；消费面按打字速率设计（console 16B 行缓冲、smoke 0.5s 逐字节注入）；粘贴突发/`FEN=1`+RT 中断留 A-2，别盲开。console 任务 spawn arg 已满（x0 = 端对打包）——新 handle 走静态槽（`RX_EV_H`/`RX_EV_REF`；ObjRef `retain` 后永不 release）。先问：console 输入消费面按什么速率设计？ → 雷册 §A64
- **PL011 `rx_irq`/`rx_init` 尾 ICR 勿清 RXIC（写 `UART_ICR_STICKY`=0x7ef 非 0x7ff；P5 A-1）**：INT_RX 只在入队时「严格 count==read_trigger(=1)」置位、DR 排空读清——drain 末次 FR 读与 ICR 写之间的竞速字节锁存被 ICR 抹掉 = 字节滞留 FIFO、INT_RX 永不再置、交互输入面永久静默挂起（慢注入打不中，burst 才复现）；修法 = ICR 掩 RXIC，竞速字节保住 → EOIR 后电平仍高 → GIC 电平重投再排空。教训：仿真器外设「写 1 清粘滞」寄存器，先核对该位的置位条件再决定是否全清。先问：全清 ICR 前核对该位置位条件了吗？ → 雷册 §A65
- **退出钩子顺序即正确性**：`task_exit_hook` 四步 mark_exited → `wake_matching(TERMINATED)` → drain 子表 → release 钉住，序不可换：release 先于 drain = UAF、drain 先于 wake 等待者见已清表；drain 仅 pinned 表（pinned=0 直通）；drain 无 -EBUSY 拒检但映射中 VMO Box 不可放（Drop 只泄漏帧不覆盖 Box）→ any_mapped 整对象泄漏兜；`process_start` 每进程至多一次（`started`+`state==Alive` 双检 → -EBADSTATE）。先问：退出路径动表/放引用序对吗？ → 雷册 §A66
- **libc exit 族 EOF 先行（Plan B v0.5）**：`exit/_exit/abort/__assert_fail` 先 `send_stdout_eof()`（零长消息 = 收集方终点）再 `berry_process_exit`——顺序颠倒 = 进程死了 EOF 永不到达，收集方帽误判断链（0 字节对真 console 端无扰）。std 真死路径 lang_start→`libc::exit` 全走此面；crt0 `berry_libc_park_after_main` 只是 bypass 兜底，别在它后面排逻辑。先问：新退出路径先发零长 EOF 了吗？ → 雷册 §A67
- **`-> !` 的 syscall 永不返回（Plan B v0.5）**：0x33/0x36 经 `exit_current_with` 内 `context_switch` 切走不归——HAL 派发「回写 x0」路径天然不可达；新加 `-> !` 号勿在 handler 尾补清理逻辑（不可达 = 死码）。先问：这个新号真永不返回吗？handler 尾排了死码吗？ → 雷册 §A68
- **REAPQ 消费点契约（Plan B v0.5）**：`reap_drain` 只在 idle_loop 顶 + yield_now 尾消费——新增「永不 yield 也不进 idle」的自旋任务会推迟回收（不丢：cap 32 满则泄漏 + 首 4 loud）；ktest 期（sched::init 前）不产生 REAPQ 流量，勿在 ktest 断言回收数。先问：新任务会 yield 或进 idle 吗？回收断言放在哪个相位？ → 雷册 §A69
- **TPIDR_EL0 用户 TLS 基址必须每任务保存（P6 A-3）**：寄存器每 CPU 一份——TaskContext 不随行 = 抢占后拿别人的 TLS 指针裸奔（musl errno/栈保护探针全经它）。修法 = TaskContext +tpidr@104（112B）+ switch asm 存/取对（const assert 偏移），新任务出生 0。新加 per-CPU 用户态寄存器照此模式（字段 + asm 对 + const assert）。先问：新 per-CPU 用户态寄存器随 TaskContext 随行了吗？ → 雷册 §A70
- **常驻服务的 wait 超时是正常空闲、不是链断（P6 A-3）**：服务端 wait 帽到点 -ETIMEDOUT 必须重臂续等，只有非 -EAGAIN 错码才自陷（真链断 loud）；链断判定交给发问方的 roundtrip 超时——服务端超时自杀 = 早夭 + 下游连坐；等数据的超时是心跳，不是死线。先问：这顶帽子等的是「数据」还是「活性」？ → 雷册 §A71
- **virtqueue used ring 的 idx 在 used_off+2——used_off 本身是 flags 位恒 0（P6 A-3）**：`get_u16(ring, used_off)` 读的是 flags（无 EVENT_IDX 协商时恒 0）→ poll 永不前进恒假超时；berry-virtio 的 `used_idx()`（=used_off+2）才是 idx。诊断同源原则：写 poll/诊断前先对齐同一 API——差一个偏移的两次「同一缓冲读」会制造「内存视图分裂」假象；纯函数 crate 的正确 API 在册就别手写偏移。先问：poll 与诊断打印用的是同一个 API 吗？ → 雷册 §A72
- **微内核里"把东西塞进内核"是反模式**：先问：这能不能做成用户态服务？ → 雷册 §A73
- **常驻子勿 reap + recv 容量富余 + 随行 handle -EBUSY 分流**：常驻子 `ChildHandles` 不进 reap——reap 首步 wait 30s 帽必超时（未确认退出绝不 unmap/close；有界泄漏换免白等）；服务端 `channel_recv` 收口容量按「协议定值 + 富余」，`nh != 协议值 → ST_BAD`——恰量收的 -EINVAL 误入链断 trap 杀服务；回收随行 handle 撞 -EBUSY（销毁性 close 拒映射中 VMO）降级 ST_BAD 勿 trap，随行必须 dup 别名；严格串行 RPC 应答帽超时不重发（重发 = 重复写，v1 无请求序号防重）。先问：坏 client 能杀服务端吗？ → 雷册 §A74
- **跨实现共识测试的基准漂移面（P6 A-5）**：双实现共识测试对着进 git 的 fixture（`tests/volume_fixture.bin`）——改了生成器（mkfat）忘重产 fixture，测试照跑照绿（对陈旧基准自证，「双绿即独立收敛」退化为单实现自洽）。守卫 = `mkfat --check-fixture` 内存重产逐字节比对挂 `just test-host` 前置；hexyl 的 `hexyl-plain.bin` 同族但无机器守卫、靠注释纪律。先问：基准的重产义务有没有机器守卫？ → 雷册 §A75
- **client 收侧 RPC 帽按服务端 worst 路径成本定值（服务慢 ≠ 服务死，P6 A-6）**：TRUNC 触发整链 commit = 一串串行扇区 RPC，TCG 下单次往返 ~0.8s——收侧帽 10s 结构性误杀（超时打 fs unreachable 时 fsmgr 仍在走 commit）。修法 = fs 写请求收侧帽 90s（16KiB worst ≈ 22 次仍 4x 余量）；分片 PUT 与 TRUNC 同帽（跨名 PUT 先对旧 staging 槽隐式重跑整链 commit 才应答）；真挂死交外层 smoke 标记帽兜底；晚到应答滞留已毒化会话队列无害（挡「迟到应答错配下一请求」的设计面）。先问：这顶收侧帽覆盖服务端 worst 路径成本了吗？ → 雷册 §A76
- **估算 TCG 下「client→服务→设备」链路时长用 RPC 成本模型，别拿 syscall cycle 外推（P6 A-6）**：单次串行扇区 RPC ≈ 0.8s（两层同 prio channel 往返 × 每手至多等 slice 尾 200ms + vblk poll 1ms 睡在 tick 粒度上醒）；DEADLINE_ROOTFS=300s 按「boot 走查 ~90s + FATW commit ~15s + T-d 三 op ~20s + 余量」定。先问：新链路的时限帽是按 RPC 成本模型算的吗？ → 雷册 §A77
- **QEMU 由 SIGTERM 终止后仍存活数秒并继续刷盘（P6 A-6）**：盘取证/复用已写盘前必须等进程真退（kill 后 wait 循环 + 再 sleep 3）；smoke 相位切换用 `kill` + `wait` 串行化，全 `-drive` 线 `cache.writeback=off`（writeback 模式 kill 可丢页缓存——缓存落宿主盘的保证随 off 才成立）。先问：取盘/复用前等 QEMU 真退了吗？ → 雷册 §A78
- **注入类断言防回声歧义：用展开造内容（P6 A-6）**：注入行文本本身经 console 回显进串口日志——断言不能 grep 注入原文子串；`echo f$?t > OUT.TXT` 的文件内容 f42t 只能来自真实文件（串口只见未展开 `f$?t`）= 确定性状态谓词。先问：断言串会不会被注入回声本身满足？ → 雷册 §A79
- **QEMU virtio-net VERSION_1 路径按 12B 剥 vnet_hdr（P6 A-7）**：不协商 NET_F_MRG_RXBUF 时驱动按 legacy 10B 排头、QEMU modern 路径仍剥 12B = TX 帧斩头 2B、SLIRP 静默丢包（无诊断无回包）——rx 10s 帽假超时是唯一死状；协商 MRG_RXBUF(1<<15) 把头宽钉成 12B 规范值、收发同形。排障 = `-object filter-dump` 抓 pcap 看线上真值 + guest 收帧打印，把「驱动/解析/对端」三分。先问：线上帧长与首字节和 guest 侧对得上吗？ → 雷册 §A80
- **以太网短帧 padding：解析器长度权威在报文自身域、不在线宽（P6 A-7）**：SLIRP 把 42B ARP 应答补零到 64B 最小线宽——权威域 = ARP hlen/plen、IP total_len：parse_arp ≥28B 容忍尾随、parse_ipv4 按 total_len 截 payload（padding 不进 IP 面）；按恰长（==28/==total）执法 = 每帧必拒 → 假超时。先问：「恰长解析执法安全」这类断言拿 pcap 对过吗？ → 雷册 §A81
- **瞬态 virtio 驱动退出必须 STATUS=0 复位撤队列（P6 A-7）**：退出后仍 DRIVER_OK 的 rx avail 挂着指向 DMA VMO 页的缓冲，宿主网栈外部写者（SLIRP 周期性 IPv6 RA，无需 guest 发包）会把帧 DMA 进已复用页 = 静默腐化无关进程；vblk「退出不复位」refute 前提 = 被动设备无外部写者——**refute 前提不跨设备类转移**。修法 = 复位段抽 `reset_device`（写 0 + 回读 1s 帽）协商起点与退出终点共用。先问：这设备类有没有内核之外的宿主写者？ → 雷册 §A82
- **组帧在位就不要再搬——`copy_from_slice` 自重叠 = memcpy 契约 UB（P6 A-7）**：帧已编进 IOB[12..] 后再把同区间整段 `copy_from_slice` 进同一 IOB = src/dst 重叠——当前零行为差但 LLVM 可按不重叠假设优化出错行为、Miri 必报。修 = 发送函数改收 frame_len、帧在位即不搬；别做「组帧到暂存再搬目标位」的两步设计，一步到位组进目标位。先问：src/dst 会不会全同或部分重叠？ → 雷册 §A83
- **x86 asm 默认 Intel 语法+默认 PIC+根 config 锁 target 跨 workspace（P7-x0）**：①aarch64 线 AT&T，x86_64 `global_asm!`/`asm!` 须显式 `options(att_syntax)`；②R_X86_64_32/32S 对 local symbol 拒链，`arch/x86_64/.cargo/config.toml` 钉 `relocation-model=static`；③根 config 锁 `build.target`（aarch64）、跨 workspace 仍命中，x86 构建显式 `--target x86_64-unknown-none`。先问：目标三元组显式了吗？ → 雷册 §A84
- **x86 链接脚本 `.note*` 绝不能 DISCARD（P7-x0）**：PVH 入口声明靠 ELF note `Xen`/type 18 存活——照搬 link-user.ld `DISCARD .note*` 会让 QEMU 退 multiboot 路拒收 ELF64（`Cannot load x86-64 image`）；note desc 用 `.long _start` 非硬编码（改名断链在链接期 loud）。boot 页表体放 .rodata 预零；恒等映射把表体页映 P|RW——硬件写权限看 PTE 位不看 PT_LOAD 旗标，运行时改表不 fault（W^X 才会）。先问：DISCARD 掉的段里有没有 loader 要读的 note？ → 雷册 §A85
- **x86 ISR 桩对齐有两层，`and $-16` 只管第一层（P7-x1）**：①被打断上下文的 RSP 奇偶不可假设——锚 rbp 后 `and $-16` 归一；②**call 边界 SysV 奇偶**——call 前 RSP 须 ≡ 0 (mod 16)，`and` 后压奇数个 8B 寄存器再 call 即违约 8 字节（9 个 GP = 72B 即踩）——handler 调用树一出现 `movaps` 即 #GP → IDT 无门 → 三重故障静默退出。修法 = `and` 后 `sub $8` 凑偶；新写 ISR 桩照 timer_isr 现形态抄，别从旧版本克隆。先问：call 执行前 RSP ≡ 0 (mod 16) 吗？ → 雷册 §A86
- **x86 ISR 桩不保存 XMM 的安全前提 = builtin target 禁 SSE（P7-x1）**：xmm0-15 全 caller-saved，桩只保存 9 个 GP——builtin `x86_64-unknown-none` 全局 `-sse,-mmx,+soft-float` 故结构性安全。换 custom target JSON 自查两件：①disable-redzone=true（builtin 已禁，不可加 flag）；②保持 -sse/-mmx 软浮点或 ISR 面补 XMM 保存——重开 SSE 后 handler 一被编入 xmm 指令即静默腐化上下文。先问：新 target 的 SSE/red-zone 默认与 builtin 一致吗？ → 雷册 §A87
- **8259 PIC init 有 IMR 敞开窗（P7-x1）**：ICW 序列会清 IMR = 全部 IRQ 短暂敞开——init 必须整个包在 IF=0 内（boot `cli` 至 `sti()` 之间），且 ICW4 后**立即**写 OCW1 掩回（主片 0xFE 只放 IRQ0 / 从片 0xFF）；ICW 之间 `io_wait()`（inb 0x80）真机 settle 用，QEMU 不需要但保留。先问：中断控制器 init 全程关中断了吗？ → 雷册 §A88
- **asm 勿给「架构上真读写内存」或「正确性依赖与相邻内存写保序」的指令标 nomem（P7-x4）**：nomem ⇒ 允许跨块重排内存访问；volatile 只管 volatile 操作彼此顺序、管不到普通 store 与 asm 相对位置——`invlpg` 与页表 RMW 零寄存器依赖即被上提（TCG 不建模该缓存 = QEMU 恒绿真机才爆）；`ltr` 架构上真读 GDTR+0x28 16B 描述符。任一 yes 即不标 nomem（nostack/preserves_flags 照留）；mmu.rs 的 `tlbi` 同不标。先问：该 asm 读/写可访问内存吗（LGDT/LTR/INVLPG 类）？其正确性依赖与相邻内存写的顺序吗？ → 雷册 §A89
- **elf2pe 的 .reloc 槽位 = r_offset（最终镜像绝对 VMA），sym/addend 描述槽内容、不是槽位（V-0b）**：lld --emit-relocs 的静态 reloc 惯用「rsym=节符号（val=节基址 0x1000）+ addend=符号内偏移」形态——`collect_relocs` 把 rsym≠0 的符号值当槽位 ⇒ DIR64 全错打 RVA 0x1000，固件装载 +delta 重定位即砸穿代码。自检若用合成输入自造口径 = 钉板钉的是 buggy 输出——**合成基准必须复刻真实工具链产物的字段形态**（含 rsym≠0 节符号路径）。先问：自检基准复刻了真实工具链产物的字段形态吗？ → 雷册 §A90
- **PL011 寄存器偏移按 TRM 字节口径：FR = 0x018——「FR@0x06」是字索引误当字节偏移（V-0b）**：读 0x0900_0006 落在 RSR/ECR（只读恒 0）→ TXFE 轮询永等不到 = 静默死转。单一事实源 = `arch/aarch64/src/uart.rs` 的 `UART_FR=0x018`。取证范式：`-gdb tcp::1234` 不带 `-S` 放跑到挂死后 lldb attach 读 PC/LR，LR−装载基址对回反汇编 `bl` 返回地址即定位；LLVM 把 `(fr&0x80)!=0` 编成 `ldrsb`+`tbz #31`（测 bit7 非 bit31）。先问：这个偏移是字节口径还是字索引换算的？ → 雷册 §A91
- **C ABI 结构体布局对着目标 arch 的真 ABI 抄，不跨 arch 平移（C-2a）**：x86_64-musl 与 aarch64-musl（asm-generic：st_mode@16/st_nlink@20）字段序不同、总尺寸可同 128B = 最阴的静默错；**读侧偏移才是 ABI 事实源**、写侧自洽证明不了。布局定义处放 `const _: () = { assert!(offset_of!(…) == …) }` 编译期钉板，钉全字段。同族：桩签名对着 libc crate 真原型抄——出参桩必须参数位对齐（get*_r 真原型 5 参、result 落 x4；「忽略多余寄存器参数」仅对零参桩成立）。先问：抄的是目标 arch 真身还是平移来的？ → 雷册 §A92
- **排障打点三则：字面量先数字节、trap 现场证据分级、fat LTO 下 C 符号缺席 ≠ 调用缺席（C-2a）**：①`copy_from_slice` 用常量长度差一 = 无条件 panic——写打点前先数字节面；②trap 现场寄存器证据分级：**消息指针寄存器是铁证、数值寄存器是残影**（x2 指 rodata 字面量 = 真炸点；x9/x11 的「可疑值」多半是上一调用残影，别按残影写结论）；③fat LTO 下 `llvm-nm` 找 C 兼容符号（`stat`）扑空 ≠ 没调用——std 会直调 mangled 名，判调用有无要反汇编调用点回溯，别拿符号表缺席当缺席证据。先问：这个寄存器结论是铁证还是残影？ → 雷册 §A93
- **桩 errno 档位跟上游消费面的白名单走，不按直觉给 ENOSYS（C-2a）**：桩的「正确 errno」由调用方的错误分类代码定义（查上游 `matches!(err.raw_os_error(), …)` 面）——uucore `is_xattr_unsupported` 白名单只吞 ENOTSUP/EOPNOTSUPP(95)（musl 同值），ENOSYS(38) 即误诊。返回宽度同判别：POSIX 声明 ssize_t 的桩必须 `-> isize` 全宽写 -1——c_int 桩只写 w0（x0 高半为零）→ std 误判成功 set_len(0xffffffff) 野读。先问：调用方把哪个 errno 当「不支持」吞掉？ → 雷册 §A94
- **分配器空闲头索引不得用全窗线性扫——「某阶有无空闲头」由镜像布局掷骰（K-1）**：某阶空不空由保留前缀 k=`__kernel_end>>12` 的奇偶（镜像是否跨页）决定——让内核镜像恰好跨页的无害 commit 都在重掷骰子；线性扫对空阶付 O(num_pages) 保证失败页扫，单点基线对「布局敏感的双峰指标」无守护力。修法 = **每阶位图 + 摘要位图两级索引**（置/清位 O(1) 维护摘要，查询 = 摘要字扫 + `trailing_zeros`，最小页号优先与线性扫同一 host 对拍钉板）；**禁止**用死垫/链接脚本钉 k 奇偶或放宽容差治标。先问：失败路径的代价是否与容量线性、是否被纯布局参数翻转？ → 雷册 §A95
- **共享 target dir 跨 worktree 构建污染——指纹不区分来源、后建者胜（K-1）**：主树与 worktree 共用 `~/.cargo-target` 时，cargo 对「跨 worktree 同路径依赖包」指纹不足——worktree build 后回主树 build 可能链到旧 rlib（莫名 E0061、同 commit 重建字节抖动类灵异多半同源）。纪律：跨树构建后回主树先 touch 改动面源码再 build；worktree 构建一律独立 `CARGO_TARGET_DIR`；跨树/复算 bench 前用 `tools/size.py` 核 ram_total 指纹再下结论。先问：这个产物/基线来自哪棵树的 target dir？ → 雷册 §A96
- **低半 boot 代码的常量在 release 下可 ADRP 引高半 `.rodata.cst16` 越界断链——debug 绿不代表 release 绿（K-2）**：release（fat LTO+O3）可把 `.text.boot` 填表循环折叠成常量池（匿名 .LCP 符号，`#[link_section]` 钉不住），ADRP ±4GB 跨高半即断链；debug 保留标量循环故绿。修法 = 表写入 **volatile 标量逐写**（`(&raw mut …).write_volatile`）——禁绝向量化/常量池化，boot 常量钉低半；守卫 = `just build-release` 挂 CI。先问：低半 boot 代码 release 下会池化吗？ → 雷册 §A97
- **PSCI conduit 按机型分型：HVC→PSCI 拦截只在默认 `-machine virt`（boot EL1）成立（S-1）**：virtualization=on（boot EL2）下内核降 EL1 后 `hvc #0` → 陷阱到 EL2（QEMU 不再拦为 PSCI）→ VBAR_EL2=0 → PC=0 死循环（零 dump 静默挂死）。修法 = **入口 EL 门**：`_start` 降级**前**读 CurrentEL 经 x20 携带，经 PA 写高半 `BOOT_EL`——降级后落点再读 CurrentEL 恒得 1（此时读 = 门形同虚设）。先问：新特权调用分型过 conduit（HVC/SMC/固件 PSCI）了吗？ → 雷册 §A98
- **TCG 的 `wfe` = 立即返回的 hint、`wfi` 才是真睡——稳态 park 用 wfe 必拖垮主核吞吐（S-1/S-0）**：副核 `wfe` 等待循环实为忙转（≈拖主核 ~40% 吞吐）；`wfi` 在 TCG 下真实睡眠。短窗握手邮箱用 wfe 无妨（毫秒级 + sev 唤醒）；**长驻/稳态 park 一律 wfi**。先问：这个等是毫秒级握手还是无界驻留？ → 雷册 §A99
- **高半 Rust 直取低半符号地址 = ADRP 越 ±4GB 断链（2b-1 第三形态，S-1）**：高半代码 `&__smp_sec_entry` 式引用低半 `.text.boot`/`.bss.boot` 符号（哪怕只 cast 取值）即发 ADRP——R_AARCH64_ADR_PREL_PG_HI21 out of range 断链。两法：入口 PA 经链接脚本 `.rodata` 指针字 `QUAD(secondary_entry)` 递送（`ldr =` 取字免 ADRP）；低半待写静态量挪高半 `.bss`、boot asm 按 PA=VMA−基址 写。守卫 = `size.py --assert-flip`。先问：高半代码引用低半符号地址了吗？ → 雷册 §A100
- **QEMU 双核 MTTCG 吞吐 ≈ 单核 0.61x——`-smp 2` 线的一切时限帽 ×2 起步（S-1 定标）**：双核下金丝雀行周期同步变长（3 条线 ≈ 单核 1.6x 时长），主核全链照常但节奏整体变慢。给 `-smp` 线新加断言/帽先按单核值 ×2 给，实测后再收；反向（双核跑单核时限）= 结构性假超时（「client 收侧 RPC 帽按 worst 路径成本定值」的多核版）。先问：这个时限帽按核数缩放了吗？ → 雷册 §A101
- **内核杀（process_terminate/异常杀）不走 libc exit 族 = 收集方永远收不到零长 EOF（C-4b）**：EOF 约定只挂 libc `exit/_exit/abort`（`send_stdout_eof`），`process_terminate` 内核直收口——被杀者 stdout 无终点消息，只等 EOF 必挂。修法 = 前台收集 wait_many 挂 **TERMINATED 项**收口（数组序即命中序：EOF 已排队时 READABLE 恒先于 TERMINATED；`killed=true && eof=false` 合法终态，`!o.eof && !o.killed` 警告门）。先问：收口挂 TERMINATED 项还是赌超时？ → 雷册 §A102
- **editor 控制字节（0x03 等）落在 ESC 序列中间态必须「复位状态机后落下处理」，不得静默吞掉（C-4b）**：ESC 状态机 esc=1（已见 ESC）/esc=2（CSI 参数区）中收到 0x03——既非参数字节（0x30..=0x3f）也非 final（0x40..=0x7e），按「序列内无关字节」吞掉 = Ctrl-C 在 CSI 窗内失效（真终端语义 = 序列作废、控制键照常生效）。修法 = 两态都先复位再走普通键路径（作废当前行直达 Interrupt）。先问：这个字节落在序列中段时真终端会怎么处理？ → 雷册 §A103
- **上游 crate 纯 doc 注释也会移动 musl fat-LTO 下游产物字节——「零漂移」断言按 target 面分账（SDK-1b）**：`//!` 头注进 crate metadata，musl 链下游产物（ELF/.bap/rootfs.cpio）字节随动 = **预期非缺陷**；none 面（内核内嵌平像）恒定 = **执法面**；Cargo.toml 纯元数据键不进编译。漂移归因须二分（逐 crate 还原头注定位动因）而非整树 stash 赌运气（会踩坏 abi-check 三面一致性等守卫）。先问：该加性面进不进 crate metadata、下游走哪条 target 链？ → 雷册 §A104
