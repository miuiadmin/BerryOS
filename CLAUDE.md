# CLAUDE.md

> 本文件由 Claude Code 每次会话自动加载，是本项目的工作上下文。
> 跨工具通用版（Cursor/Copilot/Codex 等）见 `AGENTS.md`，两者保持一致。

## 项目身份

**BerryOS** —— 自研的微内核 + 跨平台操作系统。
- 内核核心：**Rust**（`#![no_std]`，最小特权，只做调度 / IPC / 地址空间 / 能力）
- 用户态服务与工具：**Rust**
- 首批主线架构：**ARM64 (AArch64)**（v1 主战场 = 手表/穿戴 rich 档单线走到底，ADR #39）；**x86_64 提前启动 QEMU 最小集移植**（串口+定时器+异常向量+上下文切换+SVC 入口，范围盒子 + kill criteria，ADR #41）；RISC-V 留位。MCU 档（KB 级内存）v1 排除（ADR #40，覆盖面 = MMU 设备 MB 级起）
- 阶段沿革（**详史逐字归档 → `docs/阶段沿革.md`**；新记录追加到该文件末尾，勿回写本文件）：
  - Phase 0–3 已收官：G0/G1（调度与任务）/G1.5（地址空间 + 首个 EL0）/Phase 3（能力与 syscall——handle 表、VMO/Vmar、process/thread、channel、event、遥测环）。
  - Phase 4（IPC 与服务框架）已收官 2026-10：睡眠/唤醒、阻塞 wait + wait_many、调度看 effective + PI 捐赠、intent 五件套、fastpath/slowpath、console/svcmgr 服务化、AI 演示管线、G-eco rootfs 通道与编译侧（14 项出口对表见 `docs/Phase4收官盘点.md`）。
  - Phase 5（shell）已收官：RX 中断、bsh + `|` 管道 + mini-readline/env/`&&`/`$?`、process_exit/REAPQ 退出链。
  - Phase 6（生态）已收官 2026-10-22：std 面（stdhello/hexyl）、RAM-fs、virtio MMIO/blk/net 用户态驱动、FAT12 读写、UDP/DNS、TCP 四刀（T-d 收官刀）。
  - P7 x86_64 最小集五原语收官 2026-10：PVH 引导 / PIT+PIC / 异常向量 / 上下文切换 / SYSCALL-SYSRET，smoke-x86 7 标记已挂 CI。
  - app-0（应用模型 .bap v0）已落地 2026-10：berry-manifest + mkapp + svcmgr appd 雏形（`APP ok n=2`）。
  - C-2a（uutils coreutils 批）已落地 2026-10-25：12 命令（ls/cat/cp/mv/rm/mkdir/rmdir/touch/pwd/echo/true/false）经 crates.io `uu_*` =0.12.0 锁步薄 harness 接入 rootfs（`user/bins/uu/` 家族工作区，rustix use-libc 翻转；`U1 ok` 九步自验，rootfs 相 24→25）；随刀定案：buddy MAX_ORDER 11→12（13.8MiB rootfs 单块）、berry-exec `SpawnSpec::args` 多参面（cp/mv 双操作数）、berry-libc `Stat` 改 musl aarch64 asm-generic 布局 + `offset_of!` 全字段编译期钉板。
  - V-2（盘格式 v0·FAT16 能力层）已落地 2026-10-25：mkfat `--fat16`/`--fixture16`/`--check-fixture16` 双宽造盘 + berry-fatfs `FatBits` 分派（簇数=版本域单一事实源，BPB fstype 串不读）+ fixture16 双端共识 9 新测（host 316→325）+ fsmgr 域检拒面（`fat_bits≠Fat12 → diag("geom")` RAM-only——对抗审查 confirmed 畸形 BPB 角落后补的执法臂）；FAT12 路径零漂移（cmp 三对复产对拍 + 五跑 smoke 全绿）；FatDxe 实验（24MiB FAT16 盘 AAVMF 十段 + 内核接续全绿）消 V-3 分区盘 FAT16 接受性之雷——顺带实录：C-2a 后 rootfs 14.4MB 已超 build-boot 8MiB 盘约定（存量断裂、run-efi 不在 CI 故未踩，V-3 首刀修盘尺寸）；MBR/分区偏移全留 V-3。
  - V-3（`just image` 确定性打包）已落地 2026-10-25：`tools/mkimage.py`——MBR（LBA 假几何 255H/63S、protective CHS 自洽；type 0x0E，分区 @LBA2048 = 1MiB 对齐）+ 单 FAT16 分区 24MiB（`build_volume_puts` 单编码器复用 + `hidden`=分区 LBA——BPB 与分区表自洽，参数默认 0 保 FAT12/FAT16 全盘卷字节零漂移）+ 四件套 BOOTAA64.EFI/HELLO.TXT/KERNEL.ELF/INITRD.BIN → `out/berryos.img`（同输入同字节）；build-boot 纯 EFI 产链化（造盘段吸收进 image 单口）、run-efi 切挂 berryos.img——EDK2 PartitionDxe+FatDxe 消化 MBR/分区偏移、**BerryBoot SFS 面零改动**；修 C-2a 后 rootfs 14.4MB 超 8MiB 盘的存量断裂；验收 = 十段 + 内核接续 + 生态标记零漂移（kfile/init 字节与宿主精确同数，host 325 恒定 + 五跑 smoke 零扰动）。
  - C-2b（uutils U2 管道面）已落地 2026-10-25：echo/true/false 三命令**执行验收**（接入面 C-2a 已全落——纯验收刀）：bsh `u2_selftest` 六步（argv 多参/`-n` 无尾换行/true·false 真码/`&&` 链传播/`echo|cat` 管道对拍，全在私有通道收齐逐字节比对）→ `U2 ok`（rootfs 相 25→26）；**链序契约**：u2 排 fat 族前——EFI 路无盘 fat park（`-> !` 挡断其后自验）原本结构性打不出 U2，对抗审查实抓后前移（EFI 路亦打 U2，smoke-image ECO 14 canon 同面）；12 命令族 11/12 验收（uu_pwd 派发被 bsh 内建优先所掩，归 text 批裁）；记号裁决：U2 = echo/true/false，并行 VM 草案 text 批顺延 C-2c/U3。
  - C-2c（uutils U3 text 批 + 容量一体）已落地 2026-10-25：uu 中批六命令 head/wc/cut/tr/sort/uniq（=0.12.0 锁步，uu 家族 12→18 命令）+ bsh `u3_selftest` **五步**（⓪pwd=uu_pwd 直 spawn 收编/①cat 回显/②wc -c/⑤cut|tr 双段管道/⑥'cat|uniq|wc -l 三段链，期望值全字面常量，spawn 合计 8）→ `U3 ok`（rootfs 相 26→27）；**原案七步门审两处裁形**（首跑定标实录，不硬凑假绿）：步③ head 裁出（uu_head unix stdin `try_clone_to_owned`→fcntl(F_DUPFD) 桩 -EINVAL——fd 克隆面 libc 挂账）、步④ sort|uniq 与步⑥ sort 段裁出（uu_sort 启动期 ctrlc/signal-hook 后台线程踩 std 线程面悬垂——pthread/TLS 平台缺口挂账，⑥ 授权裁三段）；链序契约同 U2 排 fat 族前（EFI 路无盘亦打 U3，smoke-image ECO 14 canon 同面）；**容量一体**（55c6f3e，rootfs ≈21.5MiB 触发）：帽 16→32MiB（`VMO_MAX_SIZE`/`ROOTFS_MAX`/BerryBoot init 帽同口径）、buddy MAX_ORDER 12→13、盘 24→48MiB（mkfat total32 域随入——≥0x10000 扇区走 total16=0+total32@BPB+32，FAT 规范 32 位合成路径）；**EFI 路 rootfs 消费改零拷贝借片**（`RootfsBuf::Borrowed`——内核堆大对象 = 单块连续 buddy 块，21.3MiB blob 在固件碎块 + blob 自占 20.4MiB 地形下 order-13 对齐块结构性不可得，smoke-image 首跑实踩 boot panic；handoff/dtb 借片 + VMO 逐页 commit 两路碎片免疫，fw_cfg 整读保持——冷启 RAM 完整侥幸可达）；深坑六件 tail/env/date/grep/find/which 挂账另刀（`docs/挂账-C2c深坑件.md`——grep/find/which 供源 uu_grep/findutils 硬依赖 onig C 正则库无 feature 门可关，互斥实证）。
  - 当前基线：`just smoke` 五跑全绿（裸相 26 + rootfs 相 28 + RX/FAT 子目录/持久化 54）+ EL2 同绿；host 326 测；syscall 30 号在册/29 实现；BerryBoot EFI 链 **V-1 收官内核 handoff 消费面**（`just run-efi` AAVMF 挂 `out/berryos.img`（V-3）下 `kfile: 3020248B ok` → `init: 21345480B ok` → `elf: segs=5 entry=0x40000000 ok` → `load: segs=5 pages=105 ok` → `mmap: n=31 ok` → `BERRYBOOT ok` 十段，EBS 后内核 `HANDOFF ok ver=0` → `MMAP ok segs=31 pages=15534` → `ROOTFS ok src=handoff` → 裸相 26 标记 + rootfs 相生态标记（ECO/SH/APP/PIPE，无 virtio 面）接续）。下一步见 `docs/路线图.md`。
- **身份支柱**：微内核 + 省电（手表/手机）+ **AI 原生调度**（内核留第一类接口让用户态 AI 干涉放置/频率/优先级，见 `docs/AI调度设计.md`）+ PC 高性能。

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
| **端定位** | v1 = 手表 rich 档（MB 级内存）ARM64 单线；MCU 档排除；v2 攻工业 RT（对齐 6.12 PREEMPT_RT 量级）；x86 = 兼容验收 + QEMU 最小集提前（XNU 同源分端；ADR #39–#41） |
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
- **capacity(0..1023) 抽象**：每个 CPU 一个算力值，统一手表(单核小算力)→手机(大小核)→PC(全对称)；拓扑由 `arch/<arch>/` 填表，核心逻辑架构无关。一条放置路径三端通用。
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

唯一详表 = `docs/索引.md`（37 份文档分类/状态标签/双阅读路径/维护待办）；本节只留分类速览。动相关子系统前先读对应设计文档：

- **总纲与路线**：`docs/架构决策.md`（49 条 ADR 单一事实源）· `docs/架构综合纲领.md`（取舍纪律 + 排除 8 问——借鉴任何参考前先读）· `docs/路线图.md`（Phase 0–8，现行 M-VM）· `docs/优化议程.md`（五弱题动作表）。
- **内核核心**：`docs/能力模型设计.md`（纯能力 handle）· `docs/IPC设计.md`（channel/Event/intent）· `docs/内存管理设计.md`（VMO/pager）· `docs/系统调用ABI设计.md`（v1 冻结，abi.toml 单源）· `docs/调度器设计.md`（机制 + M 里程碑权威）· `docs/AI调度设计.md`（ADR #21 修订版）· `docs/可观测性设计.md` · `docs/安全与硬件隔离设计.md`（挂 v2）。
- **服务与生态**：`docs/文件系统设计.md` · `docs/驱动模型设计.md`（驱动=持证应用）· `docs/shell设计.md`（bsh + 命令梯队）· `docs/应用模型设计.md`（包 + 清单 + 能力）· 挂 v2 四份：`docs/存储布局与沙箱设计.md` / `docs/兼容子系统设计.md` / `docs/更新与OTA设计.md` / `docs/AI工程面设计.md`。
- **验证与流程**：`docs/测试策略.md`（G0–G5 关卡）· `docs/开发体验.md`（四级阶梯 + 日常循环）。
- **在途拆解**：`docs/VM里程碑设计草案.md`（现行主里程碑 M-G1..G3）· `docs/SMP设计草案.md`（G2 拆步，S-0 已落）。
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

**启动输出速览**（完整标记沿革逐字归档 → `docs/阶段沿革.md`；标记清单单一事实源 = `tools/smoke.sh`）：

- `just run` / `just run-el2`：`BerryOS boot` → `KVA ok` → `ROOTFS ok/absent` + `ROOTFS vmo=` 双行 → ktest 各段 `… ok` 至 `ktest done` → BENCH 六行微基准 + `BENCHCHURN ok` 泄漏金丝雀 + `BENCHXH ok frame=1 vmo=1 handle=1 chan=1`（S-2 资源耗尽/恢复六面——boot 窗口确定性边界断言，BENCHCHURN 后恰一次）+ `BENCH ok n=6` → 稳态矩阵（A/B 忙循环 + EL0 用户进程轮转，`switches=N` 金丝雀）→ watchdog `G1.5 ok` → `BENCHWAKE` 唤醒延迟直方图（R-0——G1.5 断言后 boot 尾恰一次，n/min/p50/p90/p99/max/sig/tmo 一行）。
- `just run-rootfs`：rootfs 相 28 标记（`ECO ok` … `APP ok n=2` + `DIR ok n=` + `CD ok` + `U1 ok`（C-2a uutils 九步自验——盖 mkdir/touch/cp/mv/ls/cat/rm/rmdir 8 命令）+ `U2 ok`（C-2b echo/true/false 执行面六步——argv 多参/`-n`/真码/`&&` 链/管道对拍）+ `U3 ok`（C-2c text 批五步——pwd 收编/cat/wc -c/cut|tr/三段链；head·sort 深坑挂账裁出）+ `B1 ok n=4`（B-1 EL0 四项微基准——sys_null/sys_vmo_rw4k/ipc_enqueue8/evt_signal 格式断言收口，数值对拍走 bench.py --el0），逐标记语义见归档与 smoke.sh）；裸跑双行降级（`ROOTFS absent`/`vmo=none`）零扰动。
- `just smoke`：五跑——裸相 26 零漂移（B-0 起 +`BENCH ok n=6`——六项微基准 + churn 泄漏金丝雀每次 boot 执法；R-0 起 +`BENCHWAKE n=`——唤醒延迟直方图形状行；S-2 起 +`BENCHXH ok`——资源耗尽/恢复六面确定性边界行，docs/基准与压力测试设计.md）/ rootfs 相 28 / RX 注入相（54 合相 + 编辑/`run fail`/落盘/RAM 目录链/cd-pwd 注入段）/ FAT 子目录相（fixture 盘 ls/cat DATA）/ FAT 持久化相（脏盘跨 boot 读回 f42t）+ 金丝雀严格递增；`SMOKE_KEEP_LOG=1` 留日志。`just smoke-x86` 独立关卡（7 标记恰一次）。
- `just run-efi`：引导链（V-1c 起十段 + 内核接续）——AAVMF 固件 + 引导盘拉起 BerryBoot：骨架横幅 → `fw: EDK II rev=…` → `gop: 800x600 …`（ramfb 供 GOP）→ `fs: HELLO.TXT 28B match` → `kfile: <n>B ok`（V-0b 刀 1：KERNEL.ELF GetInfo 探长 + AllocatePool + 循环 Read 整档收齐，n = 内核 ELF 字节数）→ `init: <n>B ok`（V-1c：INITRD.BIN 整档读入——SFS Open → GetInfo 探长 → 帽 32MiB（C-2c：16→32，与内核 ROOTFS_MAX 同口径）→ AllocatePages(EfiLoaderData)（先于 GetMemoryMap → 图内非 Conventional → V-1b pass2 自动保，无需内核新逻辑）→ 循环 Read 收齐，n = rootfs.cpio 字节数；缺席打 `init: absent` 续行——内核降级 ROOTFS absent 非致命）→ `elf: segs=<n> entry=0x… ok`（V-0b 刀 2：berry-loader `boot_parse` 引导视图——p_paddr 口径 + DRAM 窗口越界/敌意位拒面）→ `load: segs=<n> pages=<n> ok`（V-0b 刀 3：段装载——`boot.span()` 整窗 AllocateAddress 认领（缝隙页一并拿下防同窗冲突）→ 整窗 u64 字清零（BSS/缝隙确定性，与 QEMU `-kernel` 零 RAM 起步对齐）→ 逐段 copy + 回读逐字节对拍 → handoff 页（AnyPages）写 `BerryHandoff` v0 112B 双镜像）→ `mmap: n=<n> ok`（V-0b 刀 4：watchdog 归零（EFI 默认 5 分钟看门狗会在分钟级内核长跑中重启整机，GRUB 同款显式关）→ AllocatePool 取缓冲 → GetMemoryMap 重试帽 2 → handoff memmap_* 四字段落值——**map key 有效期 = 取图到 EBS 之间零 boot service 调用**（ConOut 打印也算，EBS 环内全静默；被拒 = key 过期 → 重取图再试帽 3 轮））→ `BERRYBOOT ok`（刀 4 起语义升级：EBS 成功后 **PL011 直写**（ConOut 已死——FR@**0x018** 字节口径，字索引 6 误当字节 0x06 读 RSR/ECR 恒 0 即静默死转，刀 4 四轮实踩）报终图描述符数，其后 DAIF 全屏蔽 + dcache 清刷（内核窗/handoff 页/memmap 缓冲/init blob 窗四面）+ SCTLR_EL1 清 M|A|C|I + ic iallu/tlbi vmalle1 → `br entry`（x0=handoff PA、x1..x3=0）交接；EL≠EL1 拒跳打 `boot: el=N`）——**EBS 后内核接续**（`1BerryOS boot` → `HANDOFF ok ver=0`（V-1a：x0 分流 + BRYH/ver 校验）→ `MMAP ok segs=31 pages=15534`（V-1b：memmap 模块 + buddy reserve + mm::init 两遍走图）→ `ROOTFS ok src=handoff`（V-1c：三源归一——handoff init_blob 界检 → DTB → fw_cfg）→ 裸相 26 标记 → rootfs 相生态标记（rootfs hello/ECO/SH/APP/PIPE——EFI 路首次带 rootfs 生态，无 virtio 面））；V-3 起盘 = `out/berryos.img`（`just image` 单口：MBR + 单 FAT16 分区 48MiB（C-2c 起，V-3 时 24MiB）——mkimage.py 薄 MBR 层 + mkfat `build_volume_puts` 单编码器，hidden=分区 LBA 自洽；KERNEL.ELF/INITRD.BIN 直进盘，缺失即 loud；EDK2 PartitionDxe+FatDxe 消化分区偏移，BerryBoot SFS 面零改动）。独立工作区，内核链与主 smoke 零交集。

**前置（macOS，一次性）**：

```bash
brew install qemu just                          # qemu-system-aarch64 + just（brew 的 qemu 一包含全 arch 的 qemu-system-*）
rustup target add aarch64-unknown-none          # stable，拉预编译 core（无需 nightly/build-std）
rustup target add x86_64-unknown-none           # P7-x0 起 x86_64 最小集（stable 预编译 core，免 nightly）
```

**日常命令**（仓库根；justfile 已固化 `~/.cargo/bin` 到 PATH）：

```bash
just build          # = cargo build（内核 ELF，target=aarch64-unknown-none）；先跑 build-user
just build-user     # G-eco：用户程序编译 → flat binary（kernel/src/userbins/*.bin，gitignored）+ mkapp.py 打包 demo 双 app（app/*.bap）+ mkcpio.py 产 rootfs.cpio（run-rootfs/smoke rootfs 相输入）
just run            # QEMU virt 冷启动 → 串口打印 1BerryOS boot（默认起始 EL1），halt
just run-el2        # -machine virt,virtualization=on → 21BerryOS boot（强制 EL2→EL1 eret 降级路径）
just run-release    # release 构建 + 同样 QEMU
just run-rootfs     # G-eco：带 rootfs 启动（-fw_cfg name=opt/berry/rootfs,file=rootfs.cpio）+ P6 A-2 virtio-blk（force-legacy=false）——期望 ROOTFS ok/vmo= 双行 + svcmgr 消费链 `rootfs hello` + `ECO ok` + `VIRTIO ok` + `FAT ok n=5000` + `FATW ok n=3000` + `NET ok ping=32` + `DNS ok addr=127.0.0.1`（P6 A-8 网络服务化：vnet 自验毕转常驻 DNS 服务 + svcmgr 消费自验——SLIRP 虚拟 DNS 离线确定）+ `TCP ok echo=64`（TCP T-c 最小客户端：guest 主动 connect 10.0.2.2:18080 → SLIRP 转发宿主 echod.py 回显——握手/64B 对拍/主动挥手全链；echod 由本 recipe 起（QEMU 前）/收（trap EXIT））+ `TCPS ok echo=64`（T-d TCP 服务化：netdproto OPEN/ROUNDTRIP/CLOSE 单会话独占 ST_BUSY——svcmgr 作 client 三 op 一轮全 ST_OK 才自打——「栈本身能跑」+「栈经服务化 RPC 面能被消费」两半话术闭环）+ `APP ok n=2`（app-0：svcmgr appd 雏形——rootfs `app/*.bap` 收集 → manifest/ELF 双守卫 → spawn → EOF/reap 真码 0 全槽过才自打）（P6 A-5/A-6：盘 = mkfat 确定性 FAT12 卷，FATW = RW.TXT 落盘写对拍；盘线 `cache.writeback=off`——SIGTERM/退出前扇区同步落宿主盘；**跨 boot 持久化验证走 `just smoke` 持久化相**——run-rootfs 每次 mkfat 重造盘，手动跨 boot 不可经它，对抗审查实抓注释误导；P6 A-7 网络 = `-netdev user` SLIRP 网关 ARP/ICMP 往返对拍）（裸跑 `just run` 双行降级 ROOTFS absent/vmo=none、无 virtio 面）
just abi-check     # ABI 单源守卫：gen_abi.py --check 逐字节比对 abi.toml 三面产物（kernel/berry-sys 的 abi_gen.rs + 文档 marker 块）；忘再生成/手改产物即 FAIL（挂 build/check/smoke 前置）
just abi-gen       # ABI 再生：abi.toml → 内核 abi_gen.rs + berry-sys abi_gen.rs + 文档 marker 块（改 ABI 的流程 = 改源 → abi-gen → 产物 diff 随源入册）
just smoke          # 无人值守冒烟（CI 同款）：boot 标记断言 + switches 金丝雀严格递增；G-eco-c 起双相（裸相 26——B-0 加 BENCH ok n=6、R-0 加 BENCHWAKE n= 唤醒延迟直方图形状行、S-2 加 BENCHXH ok 资源耗尽/恢复六面行 + rootfs 21→22——P5 A-2 起加 BSH ok、Plan B v0.5 加 EXIT ok、P5 v0.5 加 PIPE ok、P6 A-1 加 FS ok n=、P6 A-2 加 VIRTIO ok、P6 A-3 加 VBLK ok、P6 A-4 加 BLKD ok n=、P6 A-5 加 FAT ok n=、P6 A-6 加 FATW ok n=、P6 A-7 加 NET ok ping=、P6 A-8 加 DNS ok addr=、TCP T-c 加 TCP ok echo=、T-d 加 TCPS ok echo=、app-0 加 APP ok n=、C-0 刀 4 加 DIR ok n=、C-1 刀 5 加 CD ok、C-2a 加 U1 ok、C-2b 加 U2 ok、C-2c 加 U3 ok、B-1 加 B1 ok n=4）、P5 A-1 起三相、P6 A-6 起四跑、C-0 刀 4 起五跑（RX 交互相：FIFO stdin 逐字节注入六段——rx\n 断言 RX ok + 退格/ESC[A 编辑验 + `run fail`/`echo f$?t > OUT.TXT`/cat OUT.TXT 断言 f42t + 段 e RAM 目录链（mkdir d1 + 多段 > + cat d1/Z.TXT 断言 d127x + cat d1 目录类型面 + rm）+ 段 f cd/pwd 防回声（pwd 应答 `'/d1'`，C-1 刀 5）+ 注入后金丝雀复验；**FAT 子目录相**：mkfat --fixture 专列盘 disk_dirs.img（默认盘字节零漂移硬线不破——子目录只在 fixture 模式）——ls DATA 断言 NOTE.TXT + cat DATA/NOTE.TXT 断言 subdir note + cat DATA 目录类型面；**FAT 持久化相**：不重造盘复用已写盘——54 标记重到齐[FATW 覆写幂等] + cat OUT.TXT 跨 boot 读回 f42t；盘线全 `cache.writeback=off` + DEADLINE_ROOTFS=300s（T-d 起：三 op 逐 op 握手/挥手叠 FATW commit 后 240s 余量见底））；SMOKE_KEEP_LOG=1 留日志
just build-x86      # P7-x0：x86_64 最小集构建（独立小工作区 arch/x86_64/，**必须显式 --target**——根 .cargo/config.toml 锁 build.target=aarch64-unknown-none 且 config 发现跨 workspace 边界仍命中，见 Gotchas）
just run-x86        # QEMU pc 机 + PVH ELF（-no-reboot）——期望 BerryOS boot (x86_64) → PVH ok → TIMER ok → EXC dump vec=6 + EXC ok → CTX ok n=128 → SYS ok n=128 → X86 ok 七标记（P7-x1 定时器链 100Hz 真中断 100 拍；P7-x2 ud2 触发 #UD dump 后 rip+2 恢复续行；P7-x3 A/B 内核任务独立 .bss 栈乒乓 128 轮互验计数；P7-x4 CPL3 用户程序 SYSCALL/SYSRET 128 轮对拍 + DONE 哨兵经内核恢复路径归位）；退出 Ctrl-A X
just smoke-x86      # x86 独立冒烟（7 标记恰一次；不并入主 smoke——ARM64 四跑零扰动；P7-x4 收官起已挂 CI：ci.yml 装 just 复用 build/size/smoke 单一入口 + 两条 x86 步（cargo 直调构建 + 本脚本））
just smoke-image    # V-4 img-boot 独立冒烟（M-G1 关卡；smoke-x86 同款独立形态——不并入主 smoke，M-G1 收官另裁）：AAVMF 挂 out/berryos.img 自举（零 -kernel/零 fw_cfg）→ EFI 十段恰一次 + 接续（ROOTFS ok src=handoff）+ 裸相 26 + EFI 路生态 15（rootfs hello/ECO/STD 双/FILE/HEXYL/BSH/EXIT/APP/PIPE/FS/U1/U2/U3/B1，与主 smoke rootfs 相 canon 同面，无 virtio 面）+ kfile/init 字节与宿主精确对拍 + 金丝雀严格递增 + FIFO 注入 rx → RX ok + 注入后金丝雀活性复验（bsh 可交互的自动代理；UTM 手动四条点验落 docs/开发体验.md §1）；已挂 CI——Ubuntu qemu-efi-aarch64 包布局 /usr/share/AAVMF/，AAVMF_CODE/AAVMF_VARS 环境变量覆盖 justfile brew 默认路径
just build-boot     # BerryBoot EFI 应用构建链（bootloader/berryboot 独立工作区：staticlib → rust-lld 静态全链接（berryboot.ld 单 .img @0x1000 + --image-base 0，链入 berry-loader——用户态 crate 首次进 EFI 引导链）→ tools/elf2pe.py 转 PE32+；V-3 起纯 EFI 产链——造盘段已吸收进 just image，主构建链不感知、本链消费内核产物）
just image          # V-3 单口整盘镜像：MBR（LBA 假几何 255H/63S，protective CHS 自洽；type 0x0E，分区 @LBA2048 1MiB 对齐）+ 单 FAT16 分区 48MiB（C-2c：rootfs ≈21.5MB 后 24MiB 余量见底，mkfat build_volume_puts 单编码器 + total32 域 + hidden=分区 LBA——BPB 与分区表自洽）+ 四件套 BOOTAA64.EFI/HELLO.TXT/KERNEL.ELF/INITRD.BIN → out/berryos.img（gitignored；依赖 build + build-boot；同输入同字节无时刻输入）；修 C-2a 后 rootfs 14.4MB 超 8MiB 盘存量断裂
just run-efi        # V-1c：QEMU AAVMF（-pflash 固件对）挂 out/berryos.img（V-3 形制 MBR+FAT16，C-2c 起盘容 48MiB，just image 单口产出）+ -device ramfb——期望十段：BerryBoot EFI skeleton (V-0a) / fw: EDK II rev=… / gop: 800x600 fmt=1 mode=1/3 fb=… / fs: HELLO.TXT 28B match / kfile: <n>B ok（n=内核 ELF 字节数）/ init: <n>B ok（V-1c：INITRD.BIN 整档读入 + AllocatePages 页钉递送；缺席 init: absent 降级）/ elf: segs=5 entry=0x40000000 ok（boot_parse 引导视图）/ load: segs=5 pages=105 ok（段装载 + handoff v0）/ mmap: n=31 ok（GetMemoryMap 终图 + EBS 重试协议——key 有效期窗内零 BS 调用 + watchdog 归零）/ BERRYBOOT ok（EBS 后 PL011 直写 + quiesce（DAIF/dcache/SCTLR/ic/tlbi）+ br entry 交接；协议失败打状态码诊断行，机器可读缺席报告）——EBS 后内核接续：HANDOFF ok ver=0 → MMAP ok segs=31 pages=15534 → ROOTFS ok src=handoff → 裸相 26 标记 + rootfs 相生态标记（ECO/SH/APP/PIPE，无 virtio 面）；退出 Ctrl-A X
just size           # 内核体积基线报告（文件量 / PT_LOAD 镜像与静态内存 / 段明细；CI 同步打印）
just bench         # B-0/B-1 性能基准单口（docs/基准与压力测试设计.md §5-6/§8）：默认 B-0 QEMU 裸跑收割 BENCH 六行 + 基线对拍（宽松因子 3x）；`python3 tools/bench.py --el0` = B-1 EL0 四项（run-rootfs 形态收割 BENCH1，rootfs 缺席先 just build-user）；基线 tools/bench_baseline.txt 双段（B-0 裸 CSV / B-1 `BENCH1,` 前缀行），`[--el0] --update` 只刷本段另一段保留（diff 随源入册）；churn 泄漏金丝雀已挂每次 boot（smoke 裸相 `BENCH ok n=6`）
just check / just clean
```

要点 / 已踩坑：

- **产物不在 `./target`**：本机全局 `~/.cargo/config.toml` 设 `target-dir=~/.cargo-target`（sccache + 共享 target）。路径解析收敛在 `tools/paths.sh`（单一事实源，justfile/smoke.sh/CI 同源，内部 `cargo metadata` 动态取真实路径），无需关心。
- **QEMU ≥11.x 不接受 `-bios none`**（报 `Could not find ROM image 'none'`）；`virt -kernel <ELF>` 直加载即可，故 justfile / `.cargo/config.toml` 都不带 `-bios`。
- **QEMU halt 后不退出**：内核 `wfe` 死循环，串口打印完即停在那。退出 QEMU 用 `Ctrl-A` 然后 `X`（macOS 无 GNU `timeout`，自动化里用后台 + `kill`）。
- 调试：`-s -S` + lldb 单步；`-d cpu_reset -D qemu.log` 看启动 CurrentEL。
- 测试入口：`just smoke`（QEMU 无人值守冒烟，CI 同款，`tools/smoke.sh`——G-eco-c 起裸相 + rootfs 相双跑、P5 A-1 起加 RX 交互相三跑；P5 v0 起 RX 相 = rootfs+FIFO 合相 + 三段编辑注入自验（标记数随相递增，T-d 起 43、C-0 刀 4 起 45 五段、C-1 刀 5 起 47 六段、C-2a 起 48、C-2b 起 49、C-2c 起 50、B-0 起 51、R-0 起 52、S-2 起 53、B-1 起 54——f 段 cd/pwd 防回声断言 `'/d1'`））+ `just test-host`（host 单元，不依赖 QEMU；G-eco-c 起含 `user/berry-loader` cpio/ELF 解析 29 测（V-0b 刀 2 起扩引导视图面 `boot_parse`——p_paddr 口径 + DRAM 窗口越界拒 + 敌意位不 panic + elf_parse 对偶，51→59；刀 3 再扩 `boot_span` 整窗几何面——缝隙/BSS 窗含入 + 乱序无关 + 真身五段 [0x4000_0000,0x4006_6000) 钉板，59→62）、P6 A-1 起含 `user/berry-fsproto` 协议 32 测（C-0 刀 1 起 v2 路径面：NUL 分段 + LOOKUP/MKDIR/UNLINK/READDIR/STAT 五 op，18→31；C-1 刀 5 空路径串（nlen=0）= 根寻址立法——读面三 op 专属放行 + parse 白名单对偶 + 写面拒空钉死，31→32）、C-1 刀 1 起含 `user/berry-fsclient` 23 测（libc 文件面纯函数：fd 表客户端游标/resolve_path 相对挂 CWD/分片读写/严格串行 RPC 会话机）、C-0 刀 2 起含 `user/berry-fstree` 目录树 18 测（RAM overlay 平面表升树——多段路径/五 op 语义/容量帽；C-1 刀 4 list_root_nul 根名 NUL 连接形状面，17→18）、P6 A-3 起含 `drivers/berry-virtio` 13 测（2026-10-24 随驱动一族迁入 drivers/ 独立工作区）、P6 A-4 起含 `user/berry-blkproto` 协议 13 测、P6 A-5 起含 `user/berry-fatfs` 11 测、P6 A-6 起含写基元共 14 测（fat12_set/find_free_cluster/encode_file_entry roundtrip——fixture 共识基准钉读写两面编码一致）、C-0 刀 3 起扩子目录链读面 14→16 测（DirKind::Dir/簇链走查，fixture 专列同源基准）、V-2 起双宽 FAT16 面 16→25 测（FatBits 四界分派钉板 4084/4085/65524/65525 + fat16_entry/fat16_set 值域与恰合切片 + encode 按宽域拒（Fat12 拒 0xFFF6 不静默截断）+ fixture16 共识四连（BPB/链/目录/字节）——`--check-fixture16` 守卫同位挂 test-host）、P6 A-7 起含 `user/berry-netproto` 以太网/ARP/IPv4/ICMP/UDP/DNS 编解码 17 测（校验和向量 + 逐字节交叉实现对拍 + wire 钉板 + padding 容忍/恰长拒面；A-8a 扩 UDP/DNS 面 11→17）、P6 A-8a 起含 `user/berry-netdproto` DNS RPC 协议 14 测（严格串行 RPC + 编码 None 拒绝契约 + 「非 OK 恒全零 addr」烘焙线双侧 + 应答黄金字节 wire 钉板——A-8d 补；T-d 起扩 TCP 三 op 面 OPEN/ROUNDTRIP/CLOSE——「非 OK 恒不携 data」烘焙线 + ST_BUSY 单会话语义，14→26 测）+ TCP T-c 起含 `user/berry-tcp` 的最小 TCP 客户端状态机纯函数面（握手/序号推进/RTO/TIME_WAIT 状态转移 + close 双路，IO 驱动在 vnet tcp.rs，30 测）、app-0 起含 `user/berry-manifest` 的 .bap manifest v0 解析纯函数面（TOML 子集冻结到字节级 + 严格拒面 + 双守卫，host 23 测）、V-0a 起含 `bootloader/berryboot` 工作区 14 测（berry-efi：EFI 协议表布局/偏移/GUID LE 钉板 13——表形状 host 钉板防漂移（刀 4 起扩 GetMemoryMap/ExitBootServices/SetWatchdogTimer 三原型入表 + `EfiMemoryDescriptor` 40B 布局钉板 memory_descriptor_layout，12→13）；berryboot：HELLO fixture 单源共识 1——EXPECTED_HELLO ↔ hello.txt 逐字节，A-5 纪律族；`#![cfg_attr(not(test), no_std)]` 让 staticlib 宿主 std harness 双跑）、V-1b 起含 `内存管理` crate 11→22 测（memmap 模块 3 测——map_ok/desc_at/desc_range 40B 描述符步进遍历；buddy reserve 面 8→13；frame 面 +2；C-2a 起 +1——rootfs 帽单块 13.8MiB=order 12 入 MAX_ORDER 界钉板；B-0 起 +1——`allocated_pages` 对账口径钉板（churn 泄漏金丝雀判据））——总计 23+62+32+23+18+13+13+25+24+26+30+23+14=326）+ mkapp 门禁自检（`tools/mkapp.py --selftest`，9 负样本 + 2 正对照——挂 just test-host 前置，mkfat --check-fixture 同位）+ QEMU 集成（`ktest` 框架，见 `docs/测试策略.md`）。

## Claude 在本仓库的工作方式

1. **先查参考**：涉及某子系统（IPC、调度、页表…）时，先看 `reference/` 是否已有对应对照项目，参照成熟实现再写。
2. **小步提交**：OS 调试极难，每个改动尽量小且可独立验证；能在 QEMU 跑通的里程碑单独成一个提交。
3. **不污染内核态**：内核 `#![no_std]`，禁用 `std`/`alloc`（在自有分配器就绪前）；用内核自有的分配器/日志；新增 crate 前确认 `no_std` 兼容。
4. **协议洁净**：主参考是 **Rust OS**（Redox / Theseus / zCore，MIT 可对照）；hubris = **MPL-2.0 语义可借、文件不搬**；rCore-Tutorial = GPL 只看思路；参照 seL4/Zircon/Linux（C/C++/GPL）时只看思路、不复制实质代码。本仓库许可证 Apache-2.0（ADR #37）。
5. **架构无关优先**：新功能先写成架构无关 Rust，确实需要特权指令时再落到 `arch/aarch64/` 并留好接口。
6. **博采众长不杂糅**：中心 = **可注入的「机制/策略分离」+ 硬件信任边界**；借鉴任何参考前先过 `架构综合纲领.md` 的**排除 8 问**（轴 PICK？冲突中心？Theseus 仅进程内？IPC 不统管？seL4 只锁不变量？hubris 静态只借 boot 层？GPL 不进？哪关 G0–G5 验证？）。
7. **改了约定就更新本文件**：新增目标架构、改变语言边界、确定构建命令时，同步回写 `CLAUDE.md` 与 `AGENTS.md`。**但历史/沿革类内容（阶段进展、标记流水账、战役复盘）只写 `docs/阶段沿革.md` 等文档，不回写本文件**——本文件每轮会话全文注入 AI 上下文，只承载「当前状态 + 铁律」（2026-10-22 实测：沿革回写曾把两文件灌到 328KB ≈ 数万 token，已归档瘦身）。

## 约束与陷阱（Gotchas）

- 内核 `#![no_std]`：禁 `std`/`alloc`（分配器就绪前）；`panic=halt`（自实现 panic handler + 栈回溯）；**浮点寄存器默认不保存**，用 FPU 的路径需显式保存恢复。
- **`unsafe` 纪律**：只在页表/上下文切换/MMIO/分配器/硬件原子用 `unsafe`，包薄安全层；绝不散落。
- **syscall 边界无 panic**：跨 syscall 的 Rust 函数返回 `Result`/状态码，绝不 panic（内核态 panic = 系统停）。
- ARM64 下注意 Cache 一致性、屏障指令（`dmb`/`dsb`/`isb`）、大端小端（默认小端）。
- **QEMU aarch64 `virt` 起始 EL 随配置而变**：默认 `-machine virt`+`-kernel` ELF 起始 **EL1**（实测 `just run`→`1`）；`-machine virt,virtualization=on` 起始 **EL2**（`just run-el2`→`21`，走 EL2→EL1 eret）。boot 第一行（`global_asm!`）`mrs CurrentEL` 检测并按需降到 EL1（关 MMU/cache、写 `sctlr`、`eret` 到 EL1）——**别假设固定 EL**。
- **QEMU 非 Linux ELF `-kernel` 直加载不走 Linux boot 协议（G-eco 四层实证，2026-10）**：入口 x0=0（DTB 指针传递是 Linux 协议专属）、RAM 内无 DTB（扫 0xD00D_FEED 魔数零命中——LE u32 读出应为 0xD000_EDFE，别按字节序想当然）、`-initrd` **整体不加载**（pattern 文件 RAM 零命中——initrd/dtb/cmdline 机制全在 hw/arm/boot.c 的 is_linux 分支）、fw_cfg INITRD_ADDR/SIZE 键为空。**blob 递送走 fw_cfg 具名文件**：`-fw_cfg name=opt/berry/rootfs,file=…`（FILE_DIR 目录自描述名字+大小，无魔法地址；selector 收 BE u16 须整字 `write_u16`——MMIO 总线宽度敏感；data 口 u32 低字节 = 项内首字节）。DTB x0 路径只对真机/Linux 协议生效（boot asm x19 抢救已预埋，`dtb::boot_probe`）。切 Linux Image boot 协议 = ADR 级 boot 重构，留 v2。
- **PAC/BTI/MTE 目标 day-one 开**，但 Phase 0/1 推迟——`cortex-a72`（ARMv8.0）无此扩展；**v2** 切 `-cpu max` 时开 target feature（源码零改动，见 `安全与硬件隔离设计.md` §3/§6）。
- **QEMU TCG 下 CNTV 实测 tick 率 ≈ SLICE_HZ/2**：`CNTFRQ_EL0`=62.5MHz、`SLICE=FREQ/SLICE_HZ`=6.25M（配置正确，`SLICE_HZ=10` 应得 10Hz），但 `just run` 实测抢占 ~5Hz（200s 稳定 1000 切换、60s 稳定 300）——`CNTVCT` 在 TCG `virt`/`cortex-a72` 下推进速率 ≈ `CNTFRQ/2`。**非内核 bug**：真机或加 `-icount` 应为 10Hz。看 tick 率别被这 2x 误导。
- **EL0 读 CNTVCT 需 CNTKCTL_EL1.EL0VCTEN=1（复位 0 = 陷阱，B-1 首跑实踩）**：`mrs cntvct_el0` 在 EL0 直读需内核显式开位（timer `init()` 已开位 1，Linux 同款）——没开就是 EC=0x18 陷阱 → `user_fault_exit` 静默杀，且 **fault 杀退出码 = 0 + 无 EOF**（「code=0 eof=false len=0 跑完零输出」死状，静态比对排不掉、靠串口 EXCEPTION dump 破案）；EL2 场另需 CNTHCTL_EL2.EL0VCTEN。新用户态代码读系统计数器先核这扇门。
- **2b-1 起内核主体链在高半（VMA = `KERNEL_VA_BASE` + LMA 镜像）**：符号/PC 显示 `0xffffff80…` 而镜像实加载在低半（QEMU `-kernel` ELF 按 **p_paddr** 落内存）——看布局跑 `python3 tools/size.py --assert-flip`（smoke 已固化断言）。**ADRP 只有 ±4GB、双向坑**：低半早期代码（`.text.boot`）引用不了高半静态量（故 4 张 boot 表放 `.bss.boot` 低半）；高半代码取低半绝对符号地址同样越界（故 mm.rs 导入高半 `__kernel_end` 运行时减基址）。
- **2b-2 起低半已退役**：MMIO（GIC/UART）一律走高半别名（gic.rs/uart 内部常量 `KERNEL_VA_BASE + PA`），**PA 直访必 fault**——只有 boot asm 的早期打印例外（开 MMU 前直写 PA）。内核空表 `NULL_L1` 放普通 `.bss`（高半 VMA），TTBR0 写入按 `VMA - KERNEL_VA_BASE` 折算 PA。
- **用户页表叶必须 nG=1，全量失效用 `tlbi vmalle1is`**：`tlbi aside1*` 按号失效**够不着 global 项**（boot 块描述符全 nG=0）——用户叶不标 nG 则同 VA 映射跨 ASID 经 TLB 泄漏；退役/回卷要清 global 项只能全量 `vmalle1is`（`aside1all` 不是真实指令，设计文档 §5.2 原写法已更正）。
- **2b-4 起 EL0 异常走 lower-EL 向量 0x400（sync）/0x480（IRQ）**，帧宏 `save/load_frame_el0` 与 current-EL 版**仅 `sp` 槽差异**（存取 **SP_EL0** = 用户 SP；异常自 EL0 入 EL1 硬件强制 SPsel=1，帧仍落内核栈）。不变量：**EL0 运行期间 SP_EL1 留着本任务内核栈位置**（trampoline/上下文切换装入），lower-EL 异常入口才能精确落回该栈。
- **3-1 起 syscall 错误一律错误码自理、永不杀调用者**：未知/越界/未实现编号 → `-ENOTSUP`，坏 handle → `-EBADF`，EL0/EL1 同权（ABI §8.0「永不 panic」承诺的内核执法；2b-4「畸形号杀」临时政策已撤——杀只留给 CPU 故障：D-abort/I-abort/BRK 走 `user_fault_exit`）。用户页 AP=0b01 且未设 UXN = **用户页当前可执行**（NX/只读档留 v2）。
- **内核字节串 `b"..."` 只能 ASCII**：塞中文直接编译错，且下游 crate 可能耗旧 rlib 报出**误导性错误**（如 "cannot find xxx in crate berry_sched"）——见到莫名符号缺失先查上游 crate 真实编译输出；中文文案用 `println!`/`uart_puts(str.as_bytes())` 路径。
- **2b-4 起时间片按周期数记账**（`slice_left` 存 CNTVCT 周期、tick 按增量扣减）：TCG 的 2× tick 率效应原样继承（观测仍 ~5Hz/slice），行为与 tick 计数版一致。
- **`FrameAllocator::alloc()` 返回 PFN 不是 PA**（PA = pfn << PAGE_SHIFT），且帧内容不保证干净（buddy 只管元数据）——新页先清零再用。
- **3-2 起映射中 VMO 的「销毁性 close」拒 -EBUSY、别名 close 放行**：只有 refs==1（本次 close 会 drop 对象）且 mapcnt>0 才拒；dup 出的别名 close 不死对象、正常放行——写成「映射中一律拒」会把合法别名链（RO dup→close RO）误伤（3-2 调试期实踩）。decommit 同理拒映射中页；`Vmo::Drop` 对映射中页只泄漏。
- **syscall 主体不可抢占 = v1 免锁根基**：SVC 异常入口硬件置 DAIF 屏蔽 IRQ 且 handler 全程不解蔽 → 引导表/对象访问单核串行；**任何新 syscall handler 不得解蔽 IRQ、不得忙等跨任务**（3-3 拆 per-process 表前都靠这条）。
- **`vmo_create` v1 上限 32MiB（`VMO_MAX_SIZE`，C-2c 起 16→32——rootfs cpio 单 VMO 装载面）**：巨 size 的 `Vec` 分配 abort 是 EL0 可触发 panic（违 ABI §8.0），故超限先拒 -ENOMEM——新 syscall 凡按用户 size 做分配的都要照此设上限。
- **内核堆大对象 = 单块连续 buddy 块（heap.rs 整页路径，`2^order` 对齐）**：>16MiB 量级的一次性分配在碎片化地形结构性失败——EFI 路固件保留区 + init blob 自占 20.4MiB 下 21.3MiB rootfs 的 order-13 块不可得（C-2c smoke-image 实踩 boot panic），fw_cfg 冷启 RAM 完整属侥幸可达。boot 期大缓冲优先**借用已在 DRAM 的源**（handoff/dtb 零拷贝 `RootfsBuf::Borrowed`）或逐页 VMO commit（物理不连续无碍）；给任何路径写大 `Vec` 前先问内存地形。
- **user_good 的 3-2 验收链是一次性的**（进稳态循环前跑一遍）：14 位代号经不起循环 create/close 耗 gen；改链时记住 `movz x3,#0x2018` 槽基会被 commit 步的 len 参数借走，须重载（曾因此把 pattern 写进自己代码页 0x1008——恰好自洽没炸，纯运气）。
- **spawner 内嵌子代码的 `movz x4, #<len>` 必须精确等于子模板字节数**（3-4b 起 324 = 81 指令 → 4-5a 起 376 = 94 指令 → 4-5b 起 svcmgr 内嵌 echo 268 = 67 指令）：改子模板后重数指令（每行字符串 = 1 指令 ×4B）再改常量——拷多带尾随零垃圾、拷少截断尾部，两种错都静默（vmo_write 只管拷不管语义）。ktest 的 `svc6` 是 nr+6 参共 7 形参——5 参 syscall（channel_send/recv 等）末尾要补 0 占位（3-4b 实踩 22 处）。
- **用户模板 svc 后复用参数寄存器，先想清值是什么**（3-4a 实踩）：派发只回写 x0、x1..x6 保留的是**你传进去的参数值**——vmo_read 后拿缓冲地址要用 x2（buf 参数），拿 x3 得到的是 offset（=0，野读被杀，watchdog `杀数 2 > 1` panic 拦下）。套路口诀：哪个寄存器装的什么，svc 前后一个样。
- **3-3 起内核对象间跨界引用必须 `ObjRef::retain/release` 钉住**（Thread 钉 Process、Process 钉 root Vmar、运行中任务钉 Process）：只存裸指针 = 父 handle 关光后 use-after-free。Drop 链（close→drop Thread→release Process→drop Process→release Vmar）纯指针操作不回表，无重入。另一边界：**ktest 在 `berry_sched::init` 之前跑 → 任何会 spawn 的成功路径（如 process_start）不能在 ktest 验**，只能盖参数/归属/状态错误面，happy path 交 EL0 用户进程 + watchdog 错杀断言兜底。
- **用户模板可用 `adr` 取同页内嵌数据**（3-3 spawner 的子代码字节）：自相对寻址整块搬迁后偏移不变，与「仅局部数字标号 + movz 绝对地址」的拷贝安全约定同源——但仅限**同页**内（adr ±1MiB 且不得跨模板边界引用）。
- **ktest B 段「未实现真号」样本是 0x35（thread_start）**：0x04/0x06/0x54 随 3-4c、0x05 随 4-2b、**0x33/0x36 随 Plan B v0.5、0x3A 随 P5 v0 已实现**——再拿旧号当样本断言 -ENOTSUP 会真 FAIL（0x33 更会直接杀掉 ktest）；后续实现 0x35 时（v2 多线程批）在册 29 号将全部实现——「未实现真号」样本届时**退役**（B 段仅剩未知号 -ENOTSUP 断言），勿拿不在册的 0x38 冒充（其 -ENOTSUP 成因与未知号相同，「在册未实现」覆盖点丢失）；新 NR 常量入册时再立。
- **`object_wait_one` 4-1 起阻塞化**：poll-first（命中即返不睡）→ 未命中且 deadline≠0 挂 SLEEPQ 睡到绝对时刻；醒来**重验**（醒因≠结果，清位竞态再睡循环兜底）；到点未命中 -ETIMEDOUT、`deadline==0` 仍是 poll（-EAGAIN）。命中回写**全满足集**（current，非请求子集）；等待不需权利位（12 位 rights 表无 WAIT 位）。`object_signal` 仅 Event、置位后 `wake_matching`（匹配键 = ObjRef 地址——同一对象跨表 handle 互通）。**睡眠钉住纪律**：wait 全程 `ObjRef::retain` 一次、唯一出口 release——睡眠期间全表 handle 被关光也不销毁；逐轮 retain/release 会在归零后再循环时 UAF（跨表 waker 场景真实可达）。
- **4-2 起信号位空间分半**：低 32 位 = 内核派生（`SIGNAL_READABLE`/`SIGNAL_WRITABLE`，Channel wait 面）、高 32 位 = 用户（Event `object_signal` 专用）。`object_signal` 的 clear/set 掩码**只收用户半**——越出 `SIGNAL_USER_MASK` 一律 -EINVAL（检在 get_raw 之后：坏 handle/类型仍 -EBADF 优先）。**清零掩码用用户半全集 `0xFFFF_FFFF_0000_0000`**（旧 `!0` 清零 4-2 起非法）；模板/ktest 用户位一律 `1<<32` 起编（U1/U2/U3=1<<33/34/35）。wait 低 32 位请求 Channel 上 READABLE/WRITABLE 之外的位 = 永不命中（同 Zircon 不设门）。
- **用户模板尾部内嵌数据（`.ascii` banner 等）后必须 `.balign 4`**：`UserProcess::build` 断言模板长度 4 字节倍数——banner 9 字节直接 KERNEL PANIC（boot 停在 ktest done 后，4-2a 实踩；banner 长度恰为 4 的倍数也保留该行——纪律一致性）。
- **多对象等待挂 `WAIT_ANY` 哨兵、不挂真实对象地址**（4-2b）：Task 只有单一 `wait_obj` 匹配键，`wait_many` 睡眠传 `berry_sched::WAIT_ANY`（usize::MAX）+ 全位掩码——`wake_matching` 谓词对哨兵等待者「任何对象、任何新置位」都命中，醒来由 syscall 层重验循环定夺（虚假唤醒无害）；**纯清位（set_signals==0）不误醒哨兵者**（交集为空）。数组出参 `*idx` 预清 u64::MAX = 未命中哨兵值。
- **smoke 标记不得建立在时序竞速上（4-5a 实踩）**：`MANY ok` 原只在 ponger 首次撞 -EAGAIN 时打——队列空不空是排水/补灌速率的竞速（用户打印改走 console 往返后排水率降，120s 队列恒非空的 run 真实出现过）；修法 = 标记改入口先发一次 wait_many（队空睡到首发/非空 poll 即中，两路必然命中）。给标记定条件先问：这是状态谓词还是竞速结果？
- **boot 直插 handle 的定值可被用户模板硬编，但 boot 必须断言**（4-2b ponger 哑 Event；4-5a console 发端同款）：ponger 表第 2 插 = 槽 2/代号 1 = 0x0001_0002 硬编进模板，main.rs `assert_eq!` 拦表序漂移——错值 → wait_many -EBADF → 自陷被杀 → MANY 缺失 = smoke FAIL（断言让漂移在 boot 期炸响而非静默错 handle）。4-5a console endU 定值：good/waiter/pinger = 0x0001_0002（槽 2）、ponger = 0x0001_0003（槽 3，哑 Event 先占了槽 2）、abuser/spawner = 0x0001_0001（首插，经 spawn arg x0 传入）。
- **PI 共享表粒度级联饥饿（4-5a 实证，内核服务稳态等待的铁律）**：内核任务 proc_tag=0 全查共享内核侧表 → 任何睡 channel 派生位（READABLE/WRITABLE）的内核任务都是 donor，受捐面覆盖所有持 peer 端点的睡眠者（含全体 EL0 用户）；叠 4-3a 跨层严格优先——高优用户冻结 boost 后永不入睡，可把 prio-0 常驻任务（waker）**永久饿死**（BLOCK/G1.5 缺失 + 稳态全员静默是症状）。**两道防线（新常驻内核服务都要上）**：①`spawn_with_tag` 挂 leak 私有表（4-4 随附前置补丁——受捐只看自己的表，错捐喷洒断根）；②稳态等待勿用 wait_one 直睡 channel 端点——用 wait_many（WAIT_ANY 哨兵 → `pi::donor_edge` 返回 None）+ 永不置位的哑 Event 对照项，主动退出 v1 donor 面（console 双管齐下，勿改回 wait_one）；高优 client 阻塞服务满的真实受捐不受影响（受捐方向不经 wait 对象）。
- **wake_matching 返回计数勿当收工判据**（4-2b 哨兵起）：WAIT_ANY 等待者匹配**任何**对象的唤醒——返回值混入误醒票不再可信；waker 类任务收工判据用被唤方置的旗标（sleeper 侧 `BLOCK_BOOT_DONE` 模式，4-5a 修正）。
- **调试槽 0xFE/0xFF 已随 4-5a 退役**（回落 -ENOTSUP，ktest A 段断言 -95）：用户打印一律 `channel_send(endU)` 经 console 服务落串口（发送侧 -EBUSY/-EAGAIN 全参重建立重试）；新内核侧调试需求直接 uart 打（EL1 语境），勿复活调试槽。
- **遥测环/切换钩子写入零分配**（`telemetry.rs` 静态预储环 + `sched.rs` `note_switch`）：三切换位是 IRQ 屏蔽热路径——try 化批堵住的 OOM-panic 面不得从这里重开；新钩子须「零分配、零切换、短平快」。`duplicate` 到**零权利**会被表拒（-EACCES，空 rights 拒面）——要「只读观察」handle 得 dup 到某个非零权利位（如 DUPLICATE）再靠「wait 不查权利」观察对象。
- **`block_current` 续行时 IRQ 仍屏蔽**（4-0）：切回点在唤醒方屏蔽窗/IRQ 窗口内，之后无自动解蔽点——**SVC 语境勿解蔽**（eret 自然恢复 SPSR，syscall 主体不可抢占免锁根基）；内核任务语境续行后自行 `enable_irq`（与 `yield_now` 的 disable→switch→enable 对称包络同款纪律）。
- **`SleepQueue::unlink` 不幂等**（4-0）：非环形双向链表无法廉价判「在队上」（单元素节点 prev/next 均 None 与空节点无异）——对不在队者调用会误清 head/tail 扬掉整队；三个调用点（block/wake/expire）都由屏蔽窗契约保证在队。
- **带内核派生位（READABLE/WRITABLE）的等待 ⟹ wait_obj 必须是真 channel ObjRef**（4-3b）：`pi::donor_edge` 靠这条不变量直接解引用 wait_obj——4-0 合成对象 SLEEP_OBJ（u8 静态、不透明 token）曾以 `!0` 位睡它（含派生位），PI 重推解引用 1 mod 8 地址即对齐 panic（run 实跑抓到）。合成/非对象等待一律只等**用户位**（内核永不置位；现役位**集中登记在 `kernel/src/main.rs` 的 `USER_SIGNAL_BITS` 表**——1<<32 哑 Event / 1<<33 SLEEP_OBJ / 1<<34 AI_EV_DONE / 1<<35 waiter U3 / 1<<36 AI_EV_REL / 1<<37 waiter U2 / 1<<38 waiter U4，新增用户位**必先登记该表**（编译期去重断言执法，2026-10 复盘雷 7）。
- **intent 传播的数值序（4-3c）**：u8 越小档越高（0x00 UNSPECIFIED=不设限为顶）——「借入取 max」实现为数值 `min`，文档公式与代码对读时先想这层反转。每消息重导 `eff = min(declared, msg_intent)` 非单调累积：改结算逻辑时别引入「按历史 max 累积」；三道帽超顶一律降级为不传播（非 kill、非 panic）。**转发降级 ≠ 自身回落**：slow 超预算后 send 降级装自己的 declared，但自身 `intent_effective` 借入保持——只有 recv 结算（弱消息/opt-out/超帽）才改自身档。`channel_create` 4-3c 起 options 首参（`channel_create(options, *out0, *out1)`）——旧签名调用会静默错位（out0 拿到 0 当 handle 用才 -EBADF，编译期不拦）。ktest 里测超帽样本用**伪造消息直压收端队列**（boot 单线程队列必空不满）；`Task::new_kernel` 返回 `Box<Task>`（ktest 纯函数面 `*t` 解箱再用）。
- **fastpath 检查链含「全队无哨兵」链项**（4-4）：`find_single_waiter` 遍历中撞到任一 WAIT_ANY 等待者立即 None——哨兵者被任何非零唤醒命中，单点唤醒集合 ≠ `wake_matching` 全匹配集，快路漏醒哨兵 = missed wakeup；多匹配者同理退 slowpath（唤醒集恒等 = 「不长第二套」的执法点）。
- **`try_preempt` 严格更高才切**（4-4）：直切条件 = pick 出选者 `prio_effective` **严格高于**当前任务（seL4 direct-switch 同款）——等/低不切，4-3a 同层 FIFO 不因 IPC 插队；协议同 `block_current`（SVC 屏蔽窗内可中途切走、切回点续行、IRQ 仍屏蔽勿解蔽），授片即 `arm_slice` 重编。**幻影出选防线（4-6 实踩）**：`pick_next` 非纯查询（选中即 `advance_past` 消耗 head 槽位）——「先 pick 再判切不切」在全员同层矩阵里每条 fastpath send 白耗一格 head，重复 IPC 节奏把真实出选相位锁死 → 固定节点集之外**结构性饿死**（实测出选:切入 29:1、ctx 永冻）；故 `try_preempt` 以 `Runqueue::has_eff_above` 只读探峰前置——不可能切走就根本不 pick。任何「pick 了又不用出选结果」的新调用点同罪（tick 路径 `next==cur` 自耗合法——cur 真被服务过）。
- **ktest 绝不可在手搭睡眠者在队时走 SVC send**（4-4）：fastpath 分支会 `fast_wake` 把睡眠者塞进 `sched::init` 前未启用的全局 RUNQUEUE + `try_preempt` 解引用 null CURRENT = 挂死；SVC 错误面只在 SLEEPQ 清场后测（慢路分支无等待者，安全）。量 IPC cycle 用**专用点对点 bench 对**（boot 手搭通道 + spawn_with 高优任务对，如 fa_ping/fa_pong）——别拿稳态矩阵当基线（console 排队是噪声源），且 bench 高于矩阵 prio 独占 CPU 至完成 = 与时序敏感任务（10s deadline 帽、intent probe）零纠缠。
- **常驻内核服务必须挂私有 handle 表**（4-5 前置补丁）：PI 受捐按持有表反查（4-3b），共享 BOOTSTRAP_HT 上的睡眠者被视作**持有表内一切 handle**（v1 粒度，短命 bench 任务无害）——常驻服务（console 类）睡共享表 = 每轮捐赠周期全体内核睡眠者错捐 + 级联到用户端 + 冻结随行 → 4-3a 严格优先下 prio-0 任务永久饿死（4-5 实踩）。挂法 = boot 侧 `Box::leak` 私有表 + `spawn_with_tag(entry, arg, name, prio, 表指针)`；`caller_table` EL1 分支 tag≠0 走私有表（SVC 派发与 `pi::holder_table` 同源），tag==0（ktest/boot 期/短命 bench）仍共享表零扰动。自证样板 = main.rs `task_svc_tag` 双断言（自己表 handle poll 空队 → -EAGAIN；共享表真 handle → -EBADF——路由判别铁证，打 `TAGTAB ok` 非 smoke 标记、断言炸 = panic = 后续标记缺失兜底）。
- **长命睡眠内核任务不睡共享表——4-6 新形态**：不止 4-5 的「错捐级联饿死别人」——被错捐 eff 32 的睡眠者自己每轮醒来以严格优先抢片，**把自己的断言对象推过自身重试预算**（ai_probe 8×1 slice ≈816ms 先尽 panic「时序未达」、ai_demo 第二轮 ~830ms 才到——probe 自己就是延迟源）。修法 = 步进重试的验收司机也挂私有表（表内仅 E_rel 一个 Event → 零受捐面）+ 预算按「最坏轮转 + 自己也在偷片」放宽（64；条件驱动重试、窗口由被等方 deadline 帽保持敞开——不赌竞速）。halt 之后的「全系统冻结」别当调度器 bug 先查 panic 尾巴：排队中的标记全缺 = 单点 halt 的连带面。
- **`for_each_sleeper` 契约 = IRQ 屏蔽窗内调用**（sched.rs 文档注释）：tick 超时摘队会动睡眠链接、遍历中接线即扬队。任务体经 SVC eret 后 IRQ 已解蔽——it_probe 直调未包络属侥幸（单次扫描窗口小）；新消费者照 ai_probe 手动 disable/enable 包络扫描窗。
- **用户程序编译链 stable 免 nightly（G-eco-a 决策，勿回退 custom target）**：builtin `aarch64-unknown-none`（预编译 core）+ bin crate 自带 build.rs 绝对路径注入 `link-user.ld`（0x1000 基址 = USER_CODE_VA；`-C link-arg` 相对路径随 CWD 漂移，绝对路径才稳）+ `-zmax-page-size=4096`（rust-lld AArch64 默认 64K max-page 把 PT_LOAD 对齐抬到 0x10000——0x1000 基址直接表达不了，症状 = elf2flat 报「链接基址 ≠ 约定」或段被抬到 0x10000）。custom target JSON 推迟到确需 target 级差异再上（届时才引 nightly；G-eco-d std 面走 musl 同样免 nightly）。平像抽取用 `tools/elf2flat.py`（本机无 objcopy）：**取整按相对长度**（绝对地址取整把 200B 代码吹成 2 页——实踩）；`--max` 是单页墙执法点（放宽随 G-eco-b/D3）。
- **`kernel/src/userbins/*.bin` 是 gitignored 编译产物（G-eco-a 起）**：内核 `include_bytes!` 直嵌——缺产物 = `cargo build` 编译期炸响（D6 loud-not-silent 设计，**不是静默回退**）；`just build`/`check`/`run-release`/`size-release` 已挂 build-user 依赖自动补产，但裸 `cargo build` 不行。改用户程序源码在 `user/bins/*`（abuser/svcmgr/echo/talker 已编译化；good/spawner/waiter/pinger/ponger 仍是 `kernel/src/user.rs` 的 asm 模板——两套产源并存，逐个换轨中）。
- **asm 模板换轨编译产物时，重试形态（自旋 vs 阻塞）是契约的一部分（G-eco-b 实踩）**：asm svcmgr 本体 console 发送 = `b 6b` 纯自旋重发，asm talker console 发送 = EBUSY→wait WRITABLE 阻塞——同一家族两种形态，按家族归纳会混记。换轨前逐发送点核对 asm 标号的 EBUSY 分支去向（`b Nb` 回环自旋 vs clock+wait 阻塞）。TCG 启动期 console 队列饱和窗口：阻塞等待者醒来须等一整圈轮转才拿片（~16 任务 × 200ms），自旋友商持续抢槽 → 三场竞速即耗尽 10s 帽 → -ETIMEDOUT 自陷死（SVC ok 缺失 + echo 连带饿死是症状）；自旋者由轮转兜底必达（EL0 可抢占 = 自旋有界）。
- **console 通道纯自旋 send = 长跑饱和下结构性饿死（雷 2 残留实证，与上条对账：自旋赢 boot 竞速、输长跑公平）**：自旋者从不阻塞 → 永不出 runqueue → RR 环序冻结，console 排水窗口（每轮 drain 8 槽）被环相位占优者垄断 → 自旋者每片只见满队（S/s/C/W/P 五任务 30s 零成功发送、pinger -110 晚杀 = 下游：ponger 自旋卡死不排空 endA）。**新用户程序打印一律阻塞形态**（EBUSY → clock_read + wait_one(console, WRITABLE, now+10s) → 重发；-ETIMEDOUT 重臂——与自旋同耐心只换阻塞；其它错照旧自陷）；阻塞者经 recv 侧 `wake_matching(WRITABLE)` 公平分槽。svcmgr 本体的豁免亦于 **P5 A-2 撤销**（send_spin 全量→send_retry 阻塞）——G-eco-b「boot 竞速」前提（自旋友商抢槽）已随 82fe305 全场阻塞化消失，且 A-2 恒满足项回归下它就是饿死者本人（见下条）。
- **wait_many 恒满足项使常驻服务永不入睡 = drain 相位钉死 RR 环（P5 A-2 实踩，雷 2 姊妹坑）**：等待数组里放一项「对端队列永不满 → WRITABLE 恒真」的通道项（无接收者的转发端），wait_many poll-first 每次立即命中 → 服务每醒即返、**从不进 SLEEPQ** → drain 相位冻结在 RR 环固定节点；其余生产者的唤醒旋转失效、环序固化 → **填槽序末位者结构性饿死**（与雷 2 同死状，且此回连自旋都不是必要条件——但自旋者最先死）。修法 = **恒满足项条件化入列**：仅当确有待处理状态（plen>0）才入列该项，常态少一项真睡（相位随发送方唤醒旋转）。给常驻服务加 wait 项先问：这项在稳态下会不会恒满足？
- **两遍构建 + rerun-if-changed 全列纪律（G-eco-b）**：svcmgr `include_bytes!` 内嵌 echo/talker 平像——平像由 pass 1 产出（`user/userbins/` 中转区），单遍 workspace 构建无编排保证顺序；svcmgr build.rs 须显式 rerun-if-changed 盖 link-user.ld + 自身源 + 两 staged 平像（**任何 rerun-if 指令都替换 cargo 默认全量重编行为，漏列 = 改了子程序、内核还嵌旧字节静默上线**）。
- **elf2flat 平像页补零——磁盘 .bin 是页倍数（G-eco-b/D3）**：铺平 PT_LOAD 后补零盖 .bss 并取整到页（svcmgr 9975B → 12288B 磁盘 3 页）；`UserProcess::build` 的 4 字节倍数断言天然过；`vmo_write(len)` 传平像全长（含补零，零字节无害）。`--max` 是墙执法点：svcmgr `--max 65536`（= D3 `USER_CODE_MAX`），其余单页 bin `--max 4096`；多页 spawn 的 sp 读 `UserProcess::stack_top` 字段（data/stack 随 code 页数上移），别再读单页常量。
- **SVC 寄存器映射：x0 = 号、x1..x6 = 参、返回只回写 x0（G-eco-b dump 对位确认）**：异常 dump 里 x1..x3 = wait_one 的 (handle, signals, deadline)；berry-sys 已按此镜像（G-eco-0）。诊断用户态死亡先按寄存器反推卡在哪个 syscall 的哪个参数，再按 dump 顺序对时间线。
- **std 程序 bare `abort` 静默死 = libc 桩被启动关键路径判死（G-eco-d d-1c 实踩）**：std `sys::init`→`sanitize_standard_fds` 先 `poll(3)` 验 fd 0/1/2，**非 EINTR 失败直接 `libc::abort()`——不走 panic 机制、零文案**（console 上只见 berry-libc 自家 `abort()` 桩打的 `abort\n` 诊断行；ENOSYS 桩即触发）。`poll` 已改真实现（revents 清零返 0：console 模型 fd 恒有效无 POLLNVAL，后续 /dev/null 开补循环天然跳过）——**新桩若可能被 std 启动路径引用，先查 std 对失败是否 abort，能谎报成功就谎报**。排障方法论：bare `abort` 先 grep berry-libc 自家打印认领；audit 构建（strip=false）+ llvm-objdump 找 `bl abort` 调用点、llvm-nm 定相邻 static 符号（errno=38 内联写入 = 自家 ENOSYS 桩被 std 判死的铁证）——LTO 混排下照样可读。
- **syscall handle 出参宽度 = 4 字节（`copy_out_u32`，G-eco-c 实踩）**：ABI §8 冻结基础类型 handle=u32——内核写 handle 出参一律 `copy_out_u32`（`kernel/src/syscall.rs`），曾一律 `copy_out_u64` 写 8 字节砸穿调用方 4 字节槽的**邻接局部**（编译 Rust 紧排局部下 svcmgr 的栈映射基址槽被 thread_create 出参低半清零 → hello 以 sp0=0x1000（应 0x3000）入 EL0 首条 stp 写 0xff0 自陷）。asm 模板 8 字节间距手排槽 + ktest 静态出参 `as u32` 截读碰巧免疫——**asm 时代的宽度违约在编译时代第一天即显形，编译用户程序 = ABI 宽度类 bug 的检测器**。非 handle 出参（ns/size/vaddr/nbytes/idx/observed = usize）仍 8 字节：宽度跟 ABI 类型走，不跟「凑巧能跑」走。
- **真实链接器的 PT_LOAD 不页对齐（G-eco-c/D5，装载器纪律）**：rust-lld 实测 hello RO 段 vaddr=0x10c0 与 text 段共享页——`off ≡ vaddr (mod align)` 即合法，解析器（`berry-loader`）**不收页对齐**、消费端（svcmgr `spawn_elf`）用**页铺平单 VMO** 消化（逐段独立 vmar_map 会撞已映页）。cpio 容量 = `vmo_get_size` 页对齐**含尾零页**——walk 靠 `TRAILER!!!` 自终止，别假设缓冲尾巴是干净数据。
- **std 程序栈按程序面给 + 映像/栈之间必设保护页（G-eco-d d-3c hexyl 实踩）**：默认 8 页栈被 hexyl ≈64KB 用量的下行栈静默扎进映像 RW 页腐化 .bss——首击 0x30a000 恰是 `__aarch64_have_lse_atomics` → outline-atomics 误判 → `casa`（LSE）在非 LSE cortex-a72 上 UNDEFINED、EC=0 被杀，**死状（启动即死）与死因（栈溢出）南辕北辙**。修法：`spawn_rootfs_elf` 栈页数参数化（hexyl 32 页 = musl 128KiB 默认）+ `stack_va = base+len+PAGE_SIZE` 隔一页保护页——栈穿 = 干净 D-abort，dump SP/FAR 直指祸首。
- **多分片用户输出不可直接当 smoke 标记（G-eco-d d-3c，4-5a 教训的 192B 版）**：>192B 输出经 console 通道分片，稳态矩阵打印（A/B 忙循环等）会在分片边界插花撕行（fixrun 实测 0xa0 行被撕）——grep 表行 = 时序竞速。确定性验收面 = svcmgr 私有 stdout 通道收齐（libc `park` 先补发**零长 EOF** = d-3b 约定的写侧对称面）→ 与内嵌宿主基准逐字节比对 → 才打**单消息原子**标记（`HEXYL ok` 模式）；全表 relay 到 console 只是化妆面。
- **模板握手信号用专用位、清位豁免——模板不烤调度时序假设（P5-0 实踩，waiter 位拆分 a27eb9c）**：一次性无确认 signal 与对端多段前置窗口天然竞速——模板注释「waker2 最早 3 slice 后才置位」是把调度时序当契约（boot 期 PI 共享表错捐把 waker2 抬 eff 32 更使相位确定性漂移——放大器非本因）；死状双相位可判（早到信号被 UMASK 清位抹掉 → 路 3 十秒空等 -110；意外命中 → 路 1 严格断言炸——CLAUDE.md 曾记「~10s 超时自陷」实为竞速另一相位）。修法 = 握手专用位 U4=1<<38 + 全部清位掩码豁免该位（`movz x2,#0xFFFF,lsl #32` → `#0xFFBF`，movk 高半行不动）+ observed 断言前 `bfc x8,#38,#1` 掩掉——任意相位（早到穿越清位 → poll-first 即中 / 晚到睡中被唤）皆成立。现役用户位：1<<32 哑Event(ponger) / 1<<33 SLEEP_OBJ / 1<<34 ktest·AI_EV_DONE / 1<<35 U3(waiter 路1 请求·pi E) / 1<<36 E_rel / 1<<37 U2(waiter 路2) / 1<<38 U4（P5-0 起握手专用） / 1<<39 RX Event（P5 A-1 console 输入）——新增用户位前先查碰撞。
- **GIC SPI 段 ITARGETSR 复位 0 = 静默不投任何核（P5 A-1 实踩前置规避）**：PPI 段只读、复位即投 cpu0 的惯例**不可照搬**——SPI 段可写且复位 0，不真写 target 则中断 pending 挂着永远无人收（死状 = 完全静默，不炸不 dump）。新 SPI 源一律走 `gic::enable_spi(irq, prio, target, cfg)`（prio/target/config + 清 pending + 使能一次齐）。
- **电平触发源 handler 去活电平必须在 EOIR 前 + `exception_irq` 已按 INTID 分流（P5 A-1）**：PL011 RX = drain FIFO 到 FR.RXFE（不排空 → EOIR 后线仍高立即重投 = 同优先级活锁、timer 饿死；与 CNTV「rearm CVAL 在 EOIR 前」同一条铁序）。分流纪律：**tick 槽（`on_irq_tick` 授片/slice 记账）只对 CNTV 调**——新源进 tick 路径会把授片与 CVAL 绝对编程脱钩；新中断源加 match 臂，未知源首 4 个 loud `irq? id=N` 后静默（防电平未清源重投的打印风暴）。
- **PL011 FEN=0（复位默认）有效 FIFO 深 1 字节（P5 A-1）**：v1 不写 LCR_H——真机突发第二字节在 IRQ 延迟窗内即 overrun 丢字（**QEMU 无此面**：INT_OE 从不置位、chardev 流控把 host 输入押在设备外——QEMU 唯一丢字向量 = 内核环满 `lost`）；消费面按打字速率设计（console 16B 行缓冲、smoke 逐字节 0.5s 注入 = 单字节-per-handler 标记确定性），粘贴突发/`FEN=1`+RT 中断留 A-2（QEMU 对 RT 建模未实测，别盲开）。**console 任务的 spawn arg 已满**（x0 = console 端对打包 hi32/lo32）——新 handle 交它走静态槽（`RX_EV_H`/`RX_EV_REF`，INTENT_H 惯例；ObjRef `retain` 后永不 release = 常驻钉住，4-1 睡眠钉住纪律的静态版）。
- **PL011 `rx_irq`/`rx_init` 尾 ICR 勿清 RXIC（写 `UART_ICR_STICKY`=0x7ef 非 0x7ff；P5 A-1 对抗审查实抓）**：QEMU（11.0.3 源码实证）的 INT_RX 只在入队时「严格 count==read_trigger(=1)」置位、DR 排空读清——drain 循环末次 FR 读与 ICR 写之间到达的竞速字节若锁存被 ICR 抹掉：字节滞留 FIFO、INT_RX 永不再置（严格判等 + count=1 流控拒新入队）、线永低 GIC 不投 → **交互输入面永久静默挂起**（调度照跑、金丝雀绿；smoke 0.5s 慢注入打不中几条指令宽的窗口故全绿——「标记不得建立在竞速上」的反面残留，burst 注入才复现）。修法 = ICR 掩 RXIC：竞速字节锁存保住 → EOIR 时电平仍高 → GIC 电平重投 → 再进 handler 排空；正常路径排空 DR 读已清 INT_RX、线低无风暴。教训：**仿真器外设的「写 1 清粘滞」寄存器，先核对该位的置位条件再决定是否全清**。
- **退出钩子顺序即正确性（Plan B v0.5）**：`task_exit_hook` 四步 **mark_exited → wake_matching(TERMINATED) → drain 子表 → release 运行钉住** 不可换序——release 先于 drain = Process 引用归零 drop 后还触其 `ht` = UAF；drain 先于 wake = 等待者醒来查对象时表已被清（wait 不需权利位无碍，但语义序是审计基线）。**drain 只 drain pinned Process 的表**：pinned=0（内核任务/boot 手搭裸表用户进程/共享 tag 表——ai bench ping/pong 同表）一律直通，退出 drain 会扬活任务句柄（4-6 共享表粒度坑的退出版）。drain 不做销毁性拒检（-EBUSY 拒面保护活进程），但**映射中 VMO 的 Box 不可放**（对抗审查实抓，D1）：`Region.vmo` 裸指针的有效性依托「对象不销毁」——drain 无拒面则用**整对象泄漏**兜（refs 将归零且 any_mapped → 跳过递减、令牌随死表泄漏；「Drop 只泄漏帧」对帧成立、对 Box/Region 指针不成立——放 Box 则后续 vmar_unmap 的 untrack_map 经悬空指针写堆）。**process_start 每进程至多一次**（`Process.started` + `state==Alive` 双检 → -EBADSTATE，Zircon 同形；对抗审查实抓 K1——只查 per-Thread `started` 时 thread_create×2 + start×2 可造共享同一 ht 的双任务，首任务退出即 drain 活兄弟的表 + 提前 TERMINATED、父侧 reap 把地址空间从运行中任务脚下抽走）。
- **libc exit 族 EOF 先行（Plan B v0.5）**：`exit/_exit/abort/__assert_fail` 先 `send_stdout_eof()`（零长消息 = 收集方终点，d-3b 写侧对称面）再 `berry_process_exit`——顺序颠倒 = 进程死了 EOF 永不到达，收集方 10s/30s 帽误判断链。对真 console 端 0 字节消息无扰（drain 打 0 字节 = 空操作）。std 真死路径 lang_start→`libc::exit` 全走此面——crt0 `berry_libc_park_after_main` 只是 bypass 兜底（x0 = main 返回码透传），别在它后面排逻辑。
- **`-> !` 的 syscall 永不返回（Plan B v0.5）**：0x33/0x36 经 `exit_current_with` 内 `context_switch` 切走不归——HAL 派发「回写 x0」路径天然不可达；新加 `-> !` 号勿在 handler 尾补清理逻辑（不可达 = 死码）。
- **REAPQ 消费点契约（Plan B v0.5）**：任务退出后 Task Box/内核栈入 REAPQ，`reap_drain` 只在 idle_loop 顶 + yield_now 尾消费——新增「永不 yield 也不进 idle」的自旋任务会推迟回收（不丢：cap 32 满则泄漏 + 首 4 loud）；ktest 期（sched::init 前）不产生 REAPQ 流量，勿在 ktest 断言回收数。
- **TPIDR_EL0 用户 TLS 基址必须每任务保存（P6 A-3 实踩）**：crt0 在 EL0 自设 TPIDR_EL0，但寄存器每 CPU 一份——TaskContext 不随行 = 抢占后拿**别人**的 TLS 指针裸奔（musl errno/栈保护探针全经它）。修法 = TaskContext +tpidr@104（112B）+ switch asm 存/取对（const assert 偏移），新任务出生 0。死状特征：**首次 TLS 访问距自设微秒级**的程序（先 println 再被抢占）永不触发——只有 TLS 访问前夹长 IO 的程序（vblk 数秒设备 IO 后首个 println 惰性初始化）踩中；dump TP 读落未映射区 D-abort。新加 per-CPU 用户态寄存器照此模式（字段 + asm 对 + const assert）。
- **常驻服务的 wait 超时是正常空闲、不是链断（P6 A-3 实踩，echo）**：svcmgr rootfs 走查 ~15s+ 对某服务零往来完全合法——服务端 wait 帽到点 -ETIMEDOUT 必须**重臂续等**，只有非 -EAGAIN 错码才自陷（真链断 loud）。链断判定交给发问方的 roundtrip 超时（svcmgr send→wait→recv 验），服务端超时自杀 = 早夭 + 下游连坐（echo 一死 svcmgr 稳态 'v' 循环跟着死）。给常驻服务写 wait 循环先问：这顶帽子等的是「数据」还是「活性」？等数据的超时是心跳，不是死线。
- **virtqueue used ring 的 idx 在 used_off+2——used_off 本身是 flags 位恒 0（P6 A-3 实踩，排障三节的最大教训）**：`get_u16(ring, used_off)` 读的是 flags（无 EVENT_IDX 协商时恒 0）→ poll 永不前进恒假超时；berry-virtio 的 `used_idx()`（=used_off+2）才是 idx。**诊断同源原则**：诊断打印用了 `used_idx()` 而被测 poll 手写 `used_off`——差一个偏移的两次「同一缓冲读」制造「poll=0 / dump=1 同 syscall 两结果」的「内存视图分裂」假象，误导到 QEMU monitor 物理取证才破案（xp 双半字 flags=0,idx=1 在 t0 就摆着答案：设备完成从头就在，guest 读的根本不是 idx 位）。写 poll/诊断前先对齐同一 API；纯函数 crate 的正确 API 在册就别手写偏移。
- 微内核里"把东西塞进内核"是反模式 —— 先问：这能不能做成用户态服务？
- **常驻服务子进程勿 reap + 服务端 recv 收口容量给富余 + 随行 handle 回收 EBUSY 分流（P6 A-4）**：常驻子（vblk/fsmgr 类）的 `ChildHandles` 不进 reap——reap 首步 wait TERMINATED 30s 帽对常驻子必超时返 Err（回收链前置门，未确认退出**绝不** unmap/close——不会抽走运行中地址空间），不 reap = 免 30s 白等（表槽泄漏 = 有界 = 服务生命周期，文档化取舍）；服务端 `channel_recv` 的 handle 收口容量按「协议定值 + 富余」给（vblk = 容量 4 收协议值 1）——恰按 1 收时多带垃圾消息的容量不足 -EINVAL 会误入「链断 trap」杀服务，富余容量 + `nh != 协议值 → ST_BAD` 才是协议面拒绝；**回收随行 handle 撞 -EBUSY（client 送「已映射进其地址空间的唯一 handle」，内核销毁性 close 拒映射中 VMO）分流降级 ST_BAD 勿 trap**——常驻服务被单个坏 client 杀死是最差死法（对抗审查实抓；令牌留表有界泄漏，协议头注已明示 client 契约「随行必须是 dup 别名」）；严格串行 RPC **应答帽超时不重发**（请求可能仍在途，重发 = 重复写——v1 无请求序号防重，序号重同步随 v2）。
- **跨实现共识测试的基准漂移面（P6 A-5）**：berry-fatfs 的 host 测试对 `tests/volume_fixture.bin`（mkfat.py `--fixture` 产物，进 git）做 python 造盘 ↔ Rust 解析双实现共识——改了 mkfat 忘重产 fixture，共识测试照跑照绿（对陈旧基准自证，「双绿即独立收敛」退化成单实现自洽）。守卫 = mkfat `--check-fixture <path>` 内存重产逐字节比对挂 `just test-host` 前置（漂移即 FAIL 退 1）；义务注记在 berry-fatfs 头注（改 mkfat → `mkfat.py x.img --fixture user/berry-fatfs/tests/volume_fixture.bin` 重产）。同类面：hexyl 的 `hexyl-plain.bin` 宿主基准（版本锁死 bump 须重产，d-3c 同族——那边无机器守卫，靠注释纪律）。新增「双实现互证」型测试先问：基准的重产义务有没有机器守卫？
- **client 收侧 RPC 帽按服务端 worst 路径成本定值（服务慢 ≠ 服务死，P6 A-6 实踩）**：TRUNC 在 fsmgr 侧触发整链 commit = 一串**串行扇区 RPC**（3000B/2 簇 ≈ 13 次：8 数据 + 4 双 FAT + 1 根 + 1 读回），TCG 下单次往返 ~0.8s——bsh 原 10s 帽**结构性误杀**：超时打 fs unreachable 时 fsmgr 仍在走完整 commit（服务没死只是慢）。修法 = fs 写请求收侧帽 90s（16KiB worst ≈ 22 次仍有 4x 余量）——**分片 PUT 与 TRUNC 同帽**：跨名 PUT 会先对保留的旧 staging 槽隐式重跑整链 commit 才应答（对抗审查实抓首版只扩 TRUNC 腿）；真挂死交外层 smoke 标记帽兜底；晚到应答滞留已毒化会话队列**无害**——收侧超时毒化会话正是挡「迟到应答错配下一请求」的设计面。
- **估算 TCG 下「client→服务→设备」链路时长用 RPC 成本模型，别拿 syscall cycle 外推（P6 A-6）**：单次串行扇区 RPC ≈ 0.8s（两层同 prio channel 往返 × 每手至多等 slice 尾 200ms + vblk poll 1ms 睡在 tick 粒度上醒）；DEADLINE_ROOTFS=300s 按「boot 走查 ~90s + FATW commit ~15s + T-d 三 op ~20s + 余量」定（T-d 起 240→300）。
- **QEMU 由 SIGTERM 终止后仍存活数秒并继续刷盘（P6 A-6 持久化相）**：盘取证/复用已写盘前必须等进程真退（kill 后 wait 循环 + 再 sleep 3）；smoke 相位切换用 `kill` + `wait` 串行化，全 -drive 线 `cache.writeback=off`（writeback 模式 kill 可丢页缓存——缓存落宿主盘的保证随 off 才成立）。
- **注入类断言防回声歧义：用展开造内容（P6 A-6 段 d）**：注入行文本本身经 console 回显进串口日志——断言不能 grep 注入原文子串；`echo f$?t > OUT.TXT` 的文件内容 f42t 只能来自真实文件（串口只见未展开 `f$?t`）= 确定性状态谓词（「标记不得建立在竞速上」的注入版）。
- **QEMU virtio-net VERSION_1 路径按 12B 剥 vnet_hdr（P6 A-7 实踩）**：不协商 NET_F_MRG_RXBUF 时驱动按 legacy 10B 排头、QEMU modern 路径仍剥 12B = **TX 帧被斩头 2B、SLIRP 静默丢包**（无诊断无回包）——rx 10s 帽假超时是唯一死状。协商 MRG_RXBUF(1<<15) 把头宽钉成 12B 规范值、收发同形。排障方法论：`-object filter-dump` 抓 pcap 看线上真值（帧长 + 首字节对不上 = 发送侧问题），guest 侧收帧打印区分「设备投递了但 parse 拒」vs「设备没投递」——两端取证把「驱动问题/解析问题/对端问题」三分。
- **以太网短帧 padding vs 解析恰长执法（P6 A-7 实踩）**：SLIRP 把 42B ARP 应答补零到 64B 最小线宽（50B eth payload = 28 报文 + 22 零）——**解析器长度权威在报文自身域**（ARP hlen/plen、IP total_len），不在线宽：parse_arp ≥28B 容忍尾随、parse_ipv4 按 total 截 payload（padding 不进 IP 面，下游 ICMP 整报文校验和/payload 对拍不受扰）。按恰长（==28 / ==total）执法 = 每帧必拒 → 假超时；「恰长解析执法安全」这类断言要拿 pcap 对过再写。
- **瞬态 virtio 驱动退出必须 STATUS=0 复位撤队列（P6 A-7 对抗审查实抓 medium）**：vnet 退出后 rx avail 仍挂指向 DMA VMO 页的缓冲，退出钩子 drain 还帧回 buddy——此后 SLIRP 周期性 IPv6 RA（filter-dump 实证 ~200-600s 一发、无需 guest 发任何 IPv6 包）令仍 DRIVER_OK 的设备把帧 DMA 写进已复用页 = **静默腐化无关进程**（死状与祸首完全脱钩；交互 run-rootfs >6 分钟必踩）。vblk A-3「退出不复位」的 refute 前提 = 被动设备无外部写者——**refute 前提不跨设备类转移**（blk 只响门铃，net 的 rx 有宿主网栈写者）。修法 = 复位段抽 `reset_device`（写 0 + 回读 1s 帽）协商起点与退出终点共用。
- **组帧在位就不要再搬——copy_from_slice 自重叠 = memcpy 契约 UB（P6 A-7 对抗审查实抓）**：把帧编进 IOB[12..] 后再把同区间整段 `copy_from_slice` 进同一 IOB = src/dst 全同重叠——当前零行为差（恒等拷贝字节不变）但 LLVM 可按不重叠假设优化出错行为、Miri 必报。修 = 发送函数改收 frame_len、帧在位即不搬；「组帧到暂存再搬目标位」的两步设计天然埋这雷，一步到位组进目标位。
- **rustc 的 x86 asm 默认 Intel 语法 + x86_64-unknown-none 默认 PIC + 根 config 锁 target 跨 workspace 边界（P7-x0 三连实踩）**：①本仓 aarch64 全线 AT&T——x86_64 侧所有 `global_asm!`/`asm!` 必须显式 `options(att_syntax)`，漏一条 = 该块每条指令解析失败（首版 41 个编译错）；②`.long _start`（PVH note）/`mov $sym,%esp`（弹床）等 R_X86_64_32/32S 对 local symbol 直接拒链报 "recompile with -fPIC"——`arch/x86_64/.cargo/config.toml` 钉 `relocation-model=static`；③根 `.cargo/config.toml` 锁了 `build.target=aarch64-unknown-none`，config 发现按 CWD 向上合并、**跨独立 workspace 边界照样命中**——x86 构建一律显式 `--target x86_64-unknown-none`（CLI 才能盖 config）。
- **x86 链接脚本 `.note*` 绝不能 DISCARD（P7-x0）**：PVH 入口声明靠 ELF note `Xen`/type 18 存活——照搬 link-user.ld 的 `DISCARD .note*`（减体积习惯）会让 QEMU 退 multiboot 路拒收 ELF64（`Cannot load x86-64 image`）；note desc 用 `.long _start` 符号非硬编码，入口改名断链在链接期 loud。boot 页表体放 .rodata 文件面预零（PT_LOAD 装载即零，弹床免清页表）；恒等映射把表体自身页映 P|RW——硬件写权限看 PTE 位不看 PT_LOAD 旗标，运行时改表不 fault（W^X 才会且不在 ADR #41 盒内，对抗审查 refuted 留档）。
- **x86 ISR 桩对齐有两层，`and $-16` 只管第一层（P7-x1 对抗审查实抓，三 finding 同根）**：①被打断上下文的 RSP 奇偶不可假设——锚 rbp 后 `and $-16` 归一；②**call 边界 SysV 奇偶**——call 执行前 RSP 须 ≡ 0 (mod 16)，`and` 后压**奇数个** 8B 寄存器再 call 即违约 8 字节（本刀恰 9 个 caller-saved GP = 72B）。今日 handler 纯整数叶子（lock incq + outb + retq）+ target 无 SSE 零可观测，但 P7-x2/x4 克隆此桩后 handler 调用树一出现 `movaps`（对齐 SSE 栈槽）即 #GP → IDT 无门 → 三重故障静默退出、无 dump 面可查。修法 = `and` 后 `sub $8` 凑偶（尾部 rbp 恢复兜底无需对称 add）。新写 ISR 桩照 timer_isr 现形态抄，别从旧版本克隆。
- **x86 ISR 桩不保存 XMM 的安全前提 = builtin target 禁 SSE（P7-x1 挂账）**：xmm0-15 全部 caller-saved，桩只保存 9 个 GP——builtin `x86_64-unknown-none` 全局 `-sse,-mmx,+soft-float`（二进制零 xmm 指令）使其结构性安全。与 red-zone 同触发前提：**换 custom target JSON 须自查两件**（`arch/x86_64/.cargo/config.toml` 注）：①disable-redzone=true（三重实证 builtin 已禁：rustc 拒收 `-red-zone` flag + `+red-zone` 透传码型不变 + 18 值 black_box 探针零负偏移写——**不需要也不可加 flag**，加了只是每构建 3 条告警噪声）；②保持 -sse/-mmx 软浮点，或 ISR 面补 XMM 保存——重开 SSE（memcpy/格式化加速常见动机）后 handler 一旦被编入 xmm 指令即静默腐化被打断上下文。
- **8259 PIC init 的 IMR 敞开窗（P7-x1）**：ICW 序列会清 IMR = 全部 IRQ 短暂敞开——init 必须整个包在 IF=0 内（boot `cli` 至 `sti()` 之间），且 ICW4 后**立即**写 OCW1 掩回（主片 0xFE 只放 IRQ0 / 从片 0xFF）；ICW 之间 `io_wait()`（inb 0x80）真机 settle 用，QEMU 不需要但保留。
- **asm 勿给「架构上真读写内存」或「正确性依赖与相邻内存写保序」的指令标 nomem（P7-x4 对抗审查实抓）**：nomem = 向编译器声明该块无内存副作用 ⇒ 允许跨块重排内存访问；volatile 只约束 volatile 操作彼此的顺序、管不到普通 store 与 asm 的相对位置。实例：`invlpg` 与页表 U/S 的 RMW 零寄存器依赖即被上提（SDM §4.10 分页结构缓存——store 只改内存不刷缓存，失效若被上提过 RMW 即落空；**TCG 不建模该缓存 = QEMU 冒烟恒绿、真机才爆**）；`ltr` 架构上真读 GDTR+0x28 的 16B 描述符（nomem 字面失实）。判别两问：该 asm 是否读/写可访问内存（LGDT/LTR/INVLPG 类）？其正确性是否依赖与相邻内存写的顺序？任一 yes 即不标 nomem（nostack/preserves_flags 照留）。aarch64 侧 mmu.rs 的 tlbi 同不标 nomem——本仓既有执法先例。
- **elf2pe 的 .reloc 槽位 = r_offset（最终镜像绝对 VMA），sym/addend 描述槽内容、不是槽位（V-0b 刀 2 实抓）**：lld --emit-relocs 的静态 reloc 惯用「rsym=节符号（val=节基址 0x1000）+ addend=符号内偏移」形态——`collect_relocs` 旧版「rsym≠0 取符号值当槽位」⇒ 五条 DIR64 全错打 RVA 0x1000，固件装载时 +delta×5 砸穿首函数头两条指令（症状 = 骨架横幅打出后 undefined instruction 死；QEMU monitor dump guest vs 文件字节，u64 差值恰 = 条数×delta 即定位口诀）。自检双绿为何没拦：合成输入自造 r_off=节相对口径、钉板钉的是 buggy 输出——**合成基准必须复刻真实工具链产物的字段形态**（含 rsym≠0 节符号路径，现钉板有该回归项），P6 A-5 基准漂移雷的工具链版。
- **PL011 寄存器偏移按 TRM 字节口径：FR = 0x018（字索引 6）——「FR@0x06」是字索引误当字节偏移的笔误（V-0b 刀 4 四轮实抓）**：读 0x0900_0006 落在 RSR/ECR（字 1，只读恒 0）→ TXFE 轮询永等不到 → **静默死转**（无 dump、无固件报错——「标记后静默」死状与 EBS 挂死同形，连环误诊两轮）。单一事实源 = 内核 `arch/aarch64/src/uart.rs` 的 `UART_FR=0x018`（TRM §3.2，与 zCore 对照）。取证法（无 printf 窗口时的定位范式）：QEMU 带 `-gdb tcp::1234` **不带 `-S`** 放跑到挂死后 lldb attach（连接即自动停机，无需 timer/interrupt 花活——lldb batch 里 threading.Timer 异步 interrupt 因 `lldb.debugger` 在子线程为 None 而哑火）读 PC/LR：LR−装载基址直接对回 ELF 反汇编的 `bl` 返回地址即精确定位。反汇编判读：LLVM 会把 `(fr & 0x80) != 0` 编成 `ldrsb`+`tbz #31`（符号位测试 bit7），见 bit31 别误判成测错位。
- **C ABI 结构体布局对着目标 arch 的真 ABI 抄，不跨 arch 平移（C-2a 实抓，Stat 布局误抄案）**：berry-libc `Stat` 曾按 x86_64-musl 字段序抄（st_nlink@16 u64 / st_mode@24），而 libc crate aarch64-musl（std 消费面）是 asm-generic 布局（st_mode@16 u32 / st_nlink@20 u32）——两版**同为 128B 且 st_size@48 以降偏移恰好同位** → size 类消费者（hexyl/bsh）一直绿，mode 类首个真消费者（uu_ls 的 is_dir 分类）才显形：std 在 +16 读 mode 读到的是我们写的 nlink=1，真目录全判非目录（ls 只打目录名）。取证法 = audit 构建（strip=false）反汇编**消费者取数点**：`ldr w8,[sp,#0x88]`（+16=mode）+ `and #0xf000; cmp #0x4000`（is_dir）——读侧偏移才是 ABI 事实源，写侧自洽证明不了布局。立法：布局定义处直接放 `const _: () = { assert!(offset_of!(…) == …) }` 编译期钉板（fs.rs 全 20 字段 + size；首版只钉 10 项被对抗审查抓——同型相邻对（st_dev↔st_ino、三个 atime/mtime/ctime 与三个 nsec）互换仍静默过，钉板要钉全）；同尺寸不同字段序 = 最阴的一类静默错。**同族：桩函数签名对着 libc crate 真原型抄**——get*_r 四桩曾写成单参 `result` 绑 x0，真原型 5 参、result 是第 5 参落 x4（x0 是 uid/name）：调用即野写（std home_dir fallback 真实可达；对抗审查实抓）；「被调方忽略多余寄存器参数」只对**零参桩**成立，出参桩必须参数位对齐。
- **排障打点三则：字面量先数字节、trap 现场证据分级、fat LTO 下 C 符号缺席 ≠ 调用缺席（C-2a 排障沉淀）**：①`copy_from_slice` 用常量长度差一 = 无条件 panic（`"u1: try "`=8B 非 9——写打点前先数字节面）；②trap 现场寄存器证据分级：**消息指针寄存器是铁证、数值寄存器是残影**（x2 指 rodata 字面量 = 真炸点；x9/x11 的「可疑值」多半是上一调用残影，别按残影写结论）；③fat LTO 下 `llvm-nm` 找 C 兼容符号（`stat`）扑空 ≠ 没调用——std 会直调 mangled 名（`bl _$LTberry_libc…`），DCE 后 C 名缺席；判调用有无要反汇编调用点回溯，别拿符号表缺席当缺席证据。
- **桩 errno 档位跟上游消费面的白名单走，不按直觉给 ENOSYS（C-2a 对抗审查实抓）**：xattr 族桩曾全给 ENOSYS(38)，但 uucore fsxattr 的 `is_xattr_unsupported` 白名单只吞 ENOTSUP/EOPNOTSUPP(95)——38 会经 uu_cp `-a`（stderr 伪诊断）与 `--preserve=xattr`（整单失败）传播。桩的「正确 errno」由**调用方的错误分类代码**定义（查上游 `matches!(err.raw_os_error(), …)` 面），「v1 无此语义」≠「ENOSYS 万金油」；musl 上 ENOTSUP==EOPNOTSUPP==95。同判别适用于返回宽度：POSIX 声明 ssize_t 的（readlink/xattr 读六族）桩必须 `-> isize` 全宽写 -1（c_int 桩只写 w0 = x0 高 32 位为零 → std 判「成功」set_len(0xffffffff) 野读，G-eco 时代 readlink 案同型）。
