# AGENTS.md

> 跨工具通用的项目说明，任何 AI 编码代理（Cursor / Copilot / Codex / Aider / Claude Code …）在读到本仓库时应先读此文件。
> Claude Code 专用的会话工作规范见 `CLAUDE.md`，两者内容保持一致，本文为单一事实来源（source of truth）。

## 这是什么

**BerryOS** —— 自研的微内核（microkernel）+ 跨平台操作系统。目标是极简内核 + 用户态服务化，首批跑在 **ARM64 (AArch64)** 上。端定位（ADR #39–#41）：v1 主战场 = 手表/穿戴 rich 档（MB 级内存）单线走到底；MCU 档（KB 级）排除；v2 攻工业 RT；x86_64 提前启动 **QEMU 最小集**移植（范围盒子 + kill criteria），长期为兼容验收目标（XNU 同源分端模式）。

**身份支柱**：微内核 + 省电（手表/手机）+ **AI 原生调度**（内核留第一类接口让用户态 AI 策略服务干涉调度，见 `docs/AI调度设计.md`）+ PC 高性能。

## 设计总原则（三条，钦定）

1. **人无我有，人有我强**：博采众长的落地口诀——别人验证过的优点学过来，别人踩过的坑绕开。证据库 = `research/`（各家好在哪/不好在哪/怎么做的）；借鉴任何参考前先过 `docs/架构综合纲领.md` 排除 8 问，防杂糅。软件行业积累到今天是做 OS 最好的时代：成熟参考实现全部开源可得，站在巨人肩上是常态而非例外。
2. **AI 友好化**：把 AI 当系统的一等公民——不止 AI 原生调度（`docs/AI调度设计.md`），一切接口对 AI 可读、可推断：ABI 单一事实源生成（`docs/系统调用ABI设计.md` §8）、结构化 tracing（`docs/可观测性设计.md`）、语义化错误码、机器可读文档。
3. **AI 测试友好化**：为"以后都是 AI 测软件"的时代预埋基建——测试基建是产品的一部分，不是附属品：确定性可复现（同输入同输出）、可编程故障注入、机器可读遥测/断言、QEMU 自动化关卡（`docs/测试策略.md` G0–G5）；目标是 AI agent 能独立跑测试、读结果、报 bug。

## 技术栈

- **全 Rust**：内核核心是 `#![no_std]` Rust（只含调度器、IPC、地址空间、能力派发）；用户态服务与工具也是 Rust。
- **syscall 边界用 C-ABI 约定**（Rust ABI 不稳定）：入口 `extern "C"` + `#[repr(C)]`，两端皆 Rust，无跨语言 FFI 风险。
- **`unsafe` 收敛**到页表 / 上下文切换 / MMIO / 分配器，外层包安全 API。
- **首批架构：ARM64**。内核 EL1，用户态 EL0，**启动入口 EL 随 QEMU 配置而变**（默认 `-machine virt`+`-kernel` ELF 起始 EL1；`virtualization=on` 则 EL2），EL 检测/降级在 `arch/aarch64/` 处理；调试用 `qemu-system-aarch64 -machine virt`。

## 架构决策（已定，详见 `docs/架构决策.md`）

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
| **AI 原生调度** | 内核留接口让用户态 AI 策略服务干涉放置/频率/优先级；慢/快循环切分、RT 屏蔽、护栏、平台分层（见 `AI调度设计.md`）。**2026-10-16 修订**：干涉面限带内、AI 降级为机制预留+实验轨道+kill criteria |
| **调度 intent** | intent=第一类调度输入（u8）·两段式：v1/v2 查表键，M4 ×4 带+预算兜底，凭遥测入场券（ADR #42） |

## 不可妥协的设计原则

1. **内核极简**：FS / 网络 / 驱动都不进内核，做成用户态服务。写代码前先问"这个能不能放到用户态？"
2. **IPC 为中枢**：服务间只通过显式消息通信，内核内不搞全局可变共享。
3. **能力即权限（capability）**：资源访问靠句柄，不靠全局权限位。
4. **硬件抽象隔离**：架构相关代码只活在 `arch/<arch>/`（规划中 `arch/aarch64/`），核心逻辑保持架构无关。
5. **失败隔离**：服务崩溃由内核监督重启，不波及系统。

## 调度器与电源（设计已定，详见 `docs/调度器设计.md`）

改动任何调度 / 电源相关代码前先读设计文档。硬约束：
- **机制在内核（Rust），策略可插拔**：内核只做上下文切换 / 每核 runqueue / 派发框架；策略做成可注册 trait（`class_rt` / `class_fair` / `class_deadline` / `class_idle`）。
- **capacity(0..1023) 抽象**：每个 CPU 一个算力值，统一手表 → 手机大小核 → PC 全对称；拓扑由 `arch/<arch>/` 填表，核心逻辑架构无关。
- **Tickless（NO_HZ）从 day-one 落地**：空闲核不收周期定时中断，续航第一杠杆。
- **每核 runqueue + 负载均衡**：多核吞吐主力，单/少核近零开销。
- **电源管理是用户态 Rust 服务**：governor / 热管理 / 休眠推用户态；内核↔PM 走**共享环形缓冲**（禁通用 IPC 拷贝）；PM 可崩，内核降级保活。
- **每任务 EPP hint**（`perf`/`efficiency`）指引大小核放置与改频。
- **双线并行**：省电与多核性能同为 v1 一等公民。
- **intent 两段式（ADR #42，2026-10-16）**：**公平是机制默认，intent 是策略输入，叠加永不替代**。v1/v2 intent = u8 查表键（IntentProfile→latency-nice/EPP/uclamp/timer slack；TCB `intent_declared/effective` P3 预埋、`berry_intent` 编码冻结 0x00 UNSPECIFIED(XNU 式不设限)–0x05 MAINTENANCE(冻结不启用)）；M4 起 class_fair ×4 带实例 + 预算包络 + deadline-server 兜底，**凭 v1 遥测入场券**（不满足则单实例即终态）；三层拓扑 class 链 > band 序 > 带内 EEVDF；`SchedClass` trait 签名零改动；band/intent **不入 AI 动作空间**。

## 仓库布局

```
berryos/
├── AGENTS.md          ← 你正在读
├── CLAUDE.md          Claude Code 会话规范
├── kernel/  mm/  sched/  ipc/      Rust no_std（内核本体）
├── arch/  bootloader/            Rust（HAL + 内联汇编 / EFI 引导）
├── drivers/  Rust：用户态驱动独立工作区（2026-10-24 启用：berry-virtio + vprobe/vblk/vnet）
├── fs/  net/  user/   Rust（用户态服务）
├── tools/  docs/                  构建 / 文档
├── research/                       调研报告（仅私密仓）
└── reference/                    第三方参考实现，独立存放（仅私密仓）
```

> 全 Rust：内核类目录 `#![no_std]` freestanding，服务类目录普通 Rust；**syscall 边界用 C-ABI 约定**，两端皆 Rust。`arch/` 内按平台分子目录（如 `arch/aarch64/`）。`reference/` 与自研代码严格分离，只放别人的源码。**目录已英文化（2026-10-24，映射见 `docs/阶段沿革.md` 追记）**。

## 设计文档

唯一详表 = `docs/索引.md`（37 份文档分类/状态标签/双阅读路径/维护待办）；本节只留分类速览。动相关子系统前先读对应设计文档：

- **总纲与路线**：`docs/架构决策.md`（49 条 ADR 单一事实源）· `docs/架构综合纲领.md`（取舍纪律 + 排除 8 问——借鉴任何参考前先读）· `docs/路线图.md`（Phase 0–8，现行 M-VM）· `docs/优化议程.md`（五弱题动作表）。
- **内核核心**：`docs/能力模型设计.md`（纯能力 handle）· `docs/IPC设计.md`（channel/Event/intent）· `docs/内存管理设计.md`（VMO/pager）· `docs/系统调用ABI设计.md`（v1 冻结，abi.toml 单源）· `docs/调度器设计.md`（机制 + M 里程碑权威）· `docs/AI调度设计.md`（ADR #21 修订版）· `docs/可观测性设计.md` · `docs/安全与硬件隔离设计.md`（挂 v2）。
- **服务与生态**：`docs/文件系统设计.md` · `docs/驱动模型设计.md`（驱动=持证应用）· `docs/shell设计.md`（bsh + 命令梯队）· `docs/应用模型设计.md`（包 + 清单 + 能力）· 挂 v2 四份：`docs/存储布局与沙箱设计.md` / `docs/兼容子系统设计.md` / `docs/更新与OTA设计.md` / `docs/AI工程面设计.md`。
- **验证与流程**：`docs/测试策略.md`（G0–G5 关卡）· `docs/开发体验.md`（四级阶梯 + 日常循环）。
- **在途拆解**：`docs/VM里程碑设计草案.md`（现行主里程碑 M-G1..G3）· `docs/SMP设计草案.md`（G2 拆步，S-0/S-1 已落）。
- **演进与归档**（时点记录，勿当现状引用）：`docs/阶段沿革.md`（详史归档 + 追记区——新进展追加其末尾，勿回写本文件）· `docs/Phase4收官盘点.md` / `docs/P6收官盘点.md` · `docs/战役复盘-2026-10.md`（同型地雷册）· 审查报告 ×2 · `docs/战略裁决-2026-10-15.md`（裁决已吸收进 ADR）。
- **已落地草案**（冻结候选）：`docs/TCP栈设计草案.md` · `docs/P6-A8网络服务化设计草案.md` · `docs/G-eco编译侧设计草案.md` · `docs/4-6出口演示设计草案.md` · `docs/app-0实施设计草案.md`。

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
- 跨 syscall 结构体加 `#[repr(C)]`，与 `berry-sys` 镜像同步。

## 改动前清单（任何代理执行任务前过一遍）

- [ ] 是否查过 `reference/` 里对应参考项目的成熟实现？
- [ ] **借鉴是否过了「架构综合纲领.md」的排除 8 问**？（中心=可注入机制/策略分离+硬件信任边界；冲突中心→拒；找不到 G0–G5 关卡=范围泄漏）
- [ ] 这个功能属于内核还是用户态服务？（默认用户态）
- [ ] 是否把架构相关代码收敛到了 `arch/`？
- [ ] 是否避开了 GPL 传染？（参照 GPL 代码时只看思路，不复制实质代码）
- [ ] 改动是否能在 QEMU 上独立验证、小步提交？
- [ ] 是否需要回写 `CLAUDE.md` / `AGENTS.md`（新增架构 / 改语言边界 / 确定构建命令）？**阶段进展/标记沿革只写 `docs/阶段沿革.md`，勿回写这两个文件**（它们每轮全文注入 AI 上下文，2026-10-22 已因沿革膨胀归档瘦身一次）。

## 构建与测试

**启动输出速览**（完整标记沿革逐字归档 → `docs/阶段沿革.md`；标记清单单一事实源 = `tools/smoke.sh`）：

- `just run` / `just run-el2`：`BerryOS boot` → `KVA ok` → `ROOTFS ok/absent` + `ROOTFS vmo=` 双行 → ktest 各段 `… ok` 至 `ktest done` → BENCH 六行微基准 + `BENCHCHURN ok` 泄漏金丝雀 + `BENCHXH ok frame=1 vmo=1 handle=1 chan=1`（S-2 资源耗尽/恢复六面）+ `BENCH ok n=6` → 稳态矩阵（A/B 忙循环 + EL0 用户进程轮转，`switches=N` 金丝雀）→ watchdog `G1.5 ok` → `BENCHWAKE` 唤醒延迟直方图（R-0——G1.5 断言后 boot 尾恰一次）。
- `just run-smp`（S-1）：默认机型 + `-smp 2`（**全仓唯一 -smp 入口**）——boot 尾 DAIF 屏蔽窗多三行 `SMP: cpu_on entry=<hex>` → `sSMP: sec up el=1 mpidr=0x80000001 …`（行首 `s` = 副核哨兵字符）→ `SMP ok ncores=2`（hb1+hb2 心跳状态谓词）；单核（CPU_ON 返 -2）/ EL2 机型（conduit 分型）/ EFI 三降级路径 boot 零扰动。
- `just run-rootfs`：rootfs 相 35 标记（`ECO ok` … `APP ok n=2` + `DIR ok n=` + `CD ok` + `U1 ok`（C-2a uutils 九步自验——盖 8 命令）+ `U2 ok`（C-2b echo/true/false 执行面六步）+ `U3 ok`（C-2c text 批五步——pwd 收编/cat/wc -c/cut|tr/三段链；head·sort 深坑挂账裁出）+ `B1 ok n=4`（B-1 EL0 四项微基准格式断言收口，数值对拍走 bench.py --el0）+ `DF ok`（C-2 刀 2 df+statfs 真值面三步——几何定值/RW.TXT 重定尺寸差分/df 表格对拍；du 步 uu_du 无条件 thread::spawn 踩 std 线程面平台缺口挂账裁出）+ `U4 ok`（C-2c 挂账刀深坑六件八步——tail/head fd 克隆/env/date 钉板/grep/which/find）+ `TAB ok`（C-4 Tab 补全九步——editor.rs 状态机 + FakeSrc 喂键：唯一补全/多候选列示/历史召回/退格/Ctrl-U/ESC 非 final 落下/路径补全/真源双面；断言走 `(^|[^G])TAB ok` 前邻特例防 TAGTAB ok 子串假阳性）+ `INT ok`（C-4b Ctrl-C 杀链四步——editor 0x03 作废行/ESC 两态共存/续用 + 前台杀链真机面：spawn bin/hello → process_terminate(130) → collect_fg TERMINATED 收口（EOF 缺口活证——内核杀不走 libc exit 族）→ reap 130；链序排 fat 族前 EFI 路亦打）+ `PS ok n=`/`BC ok`（C-3 段②——ps 表查询 + berryctl ver/ps/mem 三子命令）+ `M3DEMO ok`（C-5 里程碑演示五步——cpio 七锚/`ls /`/uu_cp 读写对拍（collect 帽 30s 定标）/`cat|grep` 管道/真码落盘 T3.TXT 读回 x1y；排 fat 族后 EFI 路不打），逐标记语义见归档与 smoke.sh）；裸跑双行降级（`ROOTFS absent`/`vmo=none`）零扰动。
- `just smoke`：五跑——裸相 26 零漂移（B-0 起 +`BENCH ok n=6`；R-0 起 +`BENCHWAKE n=` 唤醒延迟直方图形状行；S-2 起 +`BENCHXH ok` 资源耗尽/恢复六面行）/ rootfs 相 35（C-3 起 +`PS ok n=`/`BC ok`；C-4 起 +`TAB ok`；C-4b 起 +`INT ok`；C-5 起 +`M3DEMO ok n=5`）/ RX 注入相（61 合相 + 编辑/`run fail`/落盘/RAM 目录链/cd-pwd/Ctrl-C 杀注入段）/ FAT 子目录相（fixture 盘 ls/cat DATA）/ FAT 持久化相（脏盘跨 boot 读回 f42t + 注入 `cat T3.TXT` 断言 x1y）+ 金丝雀严格递增；`SMOKE_KEEP_LOG=1` 留日志。`just smoke-x86` 独立关卡（7 标记恰一次）。`just smoke-smp` S-1 独立关卡（`SMP ok ncores=2` 恰一次 + 裸相 26 + 金丝雀严格递增；时限 = 单核 ×2——双核 MTTCG 0.61x 吞吐；不并入主 smoke 不进 CI）。
- `just run-efi`：引导链（V-1c 起十段 + 内核接续）——AAVMF 固件 + 引导盘拉起 BerryBoot：骨架横幅 → `fw:` → `gop:`（ramfb 供 GOP）→ `fs: HELLO.TXT 28B match` → `kfile: <n>B ok`（KERNEL.ELF 整档读入，n = 内核 ELF 字节数）→ `init: <n>B ok`（V-1c：INITRD.BIN 整档读入 + AllocatePages 页钉递送；缺席 init: absent 降级）→ `elf: segs=5 entry=0x40000000 ok`（V-0b 刀 2：berry-loader `boot_parse` 引导视图——p_paddr 口径 + DRAM 窗口拒面）→ `load: segs=5 pages=133 ok`（V-0b 刀 3：段装载——整窗 AllocateAddress 认领 + 清零 + 逐段拷贝回读对拍 + handoff v0 112B 双镜像）→ `mmap: n=31 ok`（V-0b 刀 4：watchdog 归零 + GetMemoryMap 终图——map key 有效期 = 取图到 EBS 零 BS 调用，被拒重取帽 3 轮）→ `BERRYBOOT ok`（EBS 成功后 PL011 直写（ConOut 已死，**FR@0x018 字节口径——0x06 是字索引误用，读 RSR/ECR 恒 0 即静默死转**）+ quiesce（DAIF 屏蔽 + dcache 清刷四面（内核窗/handoff 页/memmap 缓冲/init blob 窗）+ SCTLR_EL1 清 M|A|C|I + ic/tlbi）→ `br entry`（x0=handoff PA））——EBS 后内核接续（`HANDOFF ok ver=0` → `MMAP ok segs=31 pages=17426` → `ROOTFS ok src=handoff` → 裸相 26 + rootfs 生态标记 ECO/SH/APP/PIPE，无 virtio 面）；V-3 起盘 = `out/berryos.img`（`just image` 单口：MBR + 单 FAT16 分区 48MiB（C-2c 起，V-3 时 24MiB）——mkimage.py 薄 MBR 层 + mkfat `build_volume_puts` 单编码器，hidden=分区 LBA 自洽；KERNEL.ELF/INITRD.BIN 直进盘，缺失即 loud）。独立工作区，内核链与主 smoke 零交集。

**前置（macOS，一次性）**：`brew install qemu just` + `rustup target add aarch64-unknown-none`（stable，无需 nightly/build-std）；P7-x0 起 x86_64 最小集另需 `rustup target add x86_64-unknown-none`（同 stable 预编译 core）。

**日常命令**（仓库根）：

```bash
just build          # = cargo build（内核 ELF）；先自动跑 build-user 补模板产物
just build-user     # G-eco：用户程序编译 → flat binary（kernel/src/userbins/*.bin，gitignored）+ mkapp.py 打包 demo 双 app（app/*.bap）+ mkcpio.py 产 rootfs.cpio
just run            # QEMU virt → 1BerryOS boot（默认起始 EL1），halt
just run-el2        # virt,virtualization=on → 21BerryOS boot（EL2→EL1 eret 降级路径）
just smoke          # 无人值守冒烟（CI 同款）：boot 标记断言 + switches 金丝雀严格递增；五跑 C-0 刀 4 起（裸 26 + rootfs 30——P6 A-8 加 `DNS ok addr=` + TCP T-c 加 `TCP ok echo=` + T-d 加 `TCPS ok echo=` + app-0 加 `APP ok n=` + C-0 加 `DIR ok n=` + C-1 加 `FILE ok`/`CD ok` + C-2a 加 `U1 ok` + C-2b 加 `U2 ok` + C-2c 加 `U3 ok` + B-1 加 `B1 ok n=4` + C-2 刀 2 加 `DF ok` + C-2c 挂账刀加 `U4 ok` + S-2 加 `BENCHXH ok` + RX 交互 56（f 段 cd/pwd 防回声断言）+ FAT 子目录相（fixture 盘 ls/cat DATA）+ FAT 持久化相）
just abi-check      # ABI 单源守卫：gen_abi.py --check 逐字节比对 abi.toml 三面产物（忘再生成/手改产物即 FAIL）
just abi-gen        # ABI 再生：abi.toml → kernel/berry-sys abi_gen.rs + 文档 marker 块（改 ABI = 改源 → abi-gen → diff 入册）
just size           # 内核体积基线报告（文件量 / PT_LOAD 镜像与静态内存 / 段明细；CI 同步打印）
just bench         # B-0/B-1 性能基准单口（docs/基准与压力测试设计.md §5-6/§8）：默认 B-0 QEMU 裸跑收割 BENCH 六行 + 基线对拍（宽松因子 3x）；`python3 tools/bench.py --el0` = B-1 EL0 四项（run-rootfs 形态收割 BENCH1，rootfs 缺席先 just build-user）；基线 tools/bench_baseline.txt 双段（B-0 裸 CSV / B-1 `BENCH1,` 前缀行），`[--el0] --update` 只刷本段另一段保留（diff 随源入册）；churn 泄漏金丝雀已挂每次 boot（smoke 裸相 `BENCH ok n=6`）
just bench-rt       # R-1 实时基准单口（docs/基准与压力测试设计.md §10）：`python3 tools/bench.py --rt`——fw_cfg 具名项 opt/berry/rt 门控裸跑（内核 rt_probe 仅该具名项在位才 spawn：三档 δ × 32 样本 oversleep，cyclictest 漏整周期再锚形）收割 BENCHRT 三行 + `RT ok n=96`，只出表无基线（--rt 独立通道，--update/--el0/--factor 组合一律拒绝）；门控缺席 = run/smoke/EFI 零扰动（BENCHRT/RT ok 恒缺）
just bench-icount   # B-2 -icount 确定性通道单口（docs/基准与压力测试设计.md §13）：`python3 tools/bench.py --icount`——QEMU 追加定值 `-icount shift=1,sleep=off`，周期数变指令数代理（≈0.125 cyc/instr）、与实时基线不可比分文件 tools/bench_baseline_icount.txt，只跑 bench 收割不用于功能面（六行 BENCH 门禁面 + BENCHWAKE 观测行）；L2 紧门禁（精确相等/±5% 双侧带）由 `--icount --prove` 五跑证明烘焙为 bench.py ICOUNT_TOL 常量（证明可重验）；跨机/跨 QEMU 版本不可比，CI 自立首跑；--el0/--rt/--factor 组合拒绝
just soak [分钟]    # S-1 soak 长跑单口（docs/基准与压力测试设计.md §11，默认 60）：`bash tools/soak.sh <分钟>`——fw_cfg 具名项 opt/berry/soak 门控（内核 soakmon 仅该具名项在位才 spawn 并打 `SOAK gate on` 自证行：常驻心跳任务每 60s vct（本机实测 ≈60s wall）打 `SOAK t=/sw=/pages=/lost=`，首拍潜伏 5 周期（≈300-600s wall）让 boot 尾部帧回收落在 pages 基线之前）+ run-rootfs 全量形态（echod 必抄——缺席则 DNS/TCP 常驻族静默缩水）+ boot 覆盖守卫（SOAK gate on + ECO/VIRTIO/FAT/NET/DNS/TCP/TCPS 七标记——vnet 链尾三标记必收，整族死亡必须 loud）；soak.sh 轮询四断言（t 严格 +1 / sw 严格递增 / pages∈[首拍,首拍+64] / lost 恒 0；非锚定收割——SOAK 行嵌矩阵巨行中段）+ 单拍帽 150s + 总帽 300+(N+5)×150+120s + SIGTERM 纪律 kill+wait+sleep 3；不进 CI（小时级，周跑/手跑，短验 `just soak 2` ≈6-7min）；门控缺席 = run/smoke/EFI 零扰动（SOAK 恒缺）；勿与 smoke/run-rootfs 并行（echod 18080 端口冲突；盘名 disk.soak.img 专属零交集）
just run-rootfs     # G-eco：带 rootfs 启动（-fw_cfg name=opt/berry/rootfs,file=rootfs.cpio + virtio-blk/net）——svcmgr 消费链 `rootfs hello` + `ECO ok` … + `NET ok ping=32` + `DNS ok addr=127.0.0.1`（P6 A-8）+ `TCP ok echo=64`（TCP T-c：guest 主动 connect 10.0.2.2:18080 → SLIRP 转发宿主 echod.py，echod 由本 recipe 起/收）+ `TCPS ok echo=64`（TCP T-d：netdproto OPEN/ROUNDTRIP/CLOSE 单会话独占 ST_BUSY——svcmgr 作 client 三 op 一轮全 ST_OK）+ `APP ok n=2`（app-0：svcmgr appd 雏形——`app/*.bap` 收集 → manifest/ELF 双守卫 → spawn → EOF/reap 真码 0 全槽过才自打）（裸跑降级 ROOTFS absent/vmo=none）
just build-x86      # P7-x0：x86_64 最小集构建（独立小工作区 arch/x86_64/，**必须显式 --target**——根 .cargo/config.toml 锁 build.target=aarch64 且 config 发现跨 workspace 边界仍命中）
just run-x86        # QEMU pc 机 + PVH ELF（-no-reboot）——期望 BerryOS boot (x86_64) → PVH ok → TIMER ok → EXC dump vec=6 + EXC ok → CTX ok n=128 → SYS ok n=128 → X86 ok 七标记（P7-x1 定时器链；P7-x2 #UD dump 后 rip+2 恢复；P7-x3 A/B 任务乒乓 128 轮互验；P7-x4 CPL3 用户程序 SYSCALL/SYSRET 128 轮对拍）
just smoke-x86      # x86 独立冒烟（7 标记恰一次；不并入主 smoke——ARM64 五跑零扰动；P7-x4 收官起已挂 CI：ci.yml 装 just 复用单一入口 + 两条 x86 步）
just run-smp        # S-1 副核 bring-up 手跑（默认机型 + -smp 2——**全仓唯一 -smp 入口**）：PSCI CPU_ON（HVC）拉副核走 §3.2 十项 → 心跳握手 → `SMP ok ncores=2`；单核/EL2 机型/EFI 三降级路径 boot 零扰动
just smoke-smp      # S-1 副核冒烟（独立形态——不并入主 smoke 不进 CI）：`SMP ok ncores=2` 恰一次 + 裸相 26 + 金丝雀严格递增；时限 = 单核 ×2（双核 MTTCG 0.61x）；`SMP: FAIL`/`SMP: skip` 出现即 FAIL
just smoke-image    # V-4 img-boot 独立冒烟（M-G1 关卡；独立形态不并入主 smoke）：AAVMF 挂 berryos.img 自举——EFI 十段恰一次 + 接续 + 裸相 26 + 生态 20（C-3 起 +PS/BC；C-4 起 +TAB；C-4b 起 +INT）+ 字节对拍 + RX 注入（rx → RX ok——内核直打面；C-4b 裁定 EFI 路无 Ctrl-C 杀段：bsh 停 fat park 交互循环不可达，交互杀面主 smoke RX 相独占）+ 注入后活性复验；已挂 CI（Ubuntu 侧 AAVMF 路径环境变量覆盖）；UTM 手动点验四条见 docs/开发体验.md
just build-boot     # BerryBoot EFI 应用构建链（bootloader/berryboot 独立工作区：staticlib → rust-lld 静态全链接 → tools/elf2pe.py 转 PE32+；链入 berry-loader（用户态 crate 首次进 EFI 引导链）；V-3 起纯 EFI 产链——造盘段已吸收进 just image，主构建链不感知、本链消费内核产物）
just image          # V-3 单口整盘镜像：MBR（LBA 假几何 255H/63S，protective CHS 自洽；type 0x0E，分区 @LBA2048 1MiB 对齐）+ 单 FAT16 分区 48MiB（C-2c：rootfs ≈21.5MB 后 24MiB 余量见底；mkfat build_volume_puts 单编码器 + total32 域 + hidden=分区 LBA——BPB 与分区表自洽）+ 四件套 BOOTAA64.EFI/HELLO.TXT/KERNEL.ELF/INITRD.BIN → out/berryos.img（gitignored；依赖 build + build-boot；同输入同字节）；修 C-2a 后 rootfs 14.4MB 超 8MiB 盘存量断裂
just run-efi        # V-1c：QEMU AAVMF（-pflash 固件对）挂 out/berryos.img（V-3 形制 MBR+FAT16，C-2c 起盘容 48MiB，just image 单口产出）+ -device ramfb——十段横幅至 BERRYBOOT ok（… + init: INITRD.BIN 整档读入 + load: 段装载/handoff v0 + mmap: n=31 ok 终图/EBS + BERRYBOOT ok 后 quiesce + br entry），EBS 后内核接续（HANDOFF ok ver=0 → MMAP ok segs=31 pages=15534 → ROOTFS ok src=handoff → 裸相 26 标记 + rootfs 相生态标记 ECO/SH/APP/PIPE）；退出 Ctrl-A X
just build-release  # K-2 release 链接守卫：abi-check + build-user + `cargo build --release`——只构建不跑 QEMU（挂 CI）；release 优化（fat LTO + O3）才生成的引用形态（.text.boot ADRP 引 .rodata.cst16 越 ±4GB）可断链而 debug 全绿，此前 CI 对 ARM64 内核 release 面零覆盖 = 断链结构性不可见；K-3 起尾挂 `size.py --assert-flip`（release 面 .text.boot 全部 ADRP 目标必低半断言，debug 面经 smoke 既有调用零改动生效）
just run-release / just check / just clean
```

要点：产物在 `~/.cargo-target`（本机全局 target-dir 覆盖，justfile 已动态解析）；QEMU ≥11.x 不用 `-bios none`；halt 后用 `Ctrl-A X` 退出 QEMU。详见 `CLAUDE.md`「构建与测试」与 `docs/路线图.md` Phase 0。

## 参考项目与协议注意

- 优先对照 **Rust OS（MIT，语言匹配）**：Redox、Theseus、zCore（zCore=MIT 可对照；rCore-Tutorial=GPL 只看思路）。
- hubris = **MPL-2.0**：监督重启等语义可借、文件不搬。
- seL4 / Fuchsia Zircon 作**思路参考**（C/C++，机制通用，不复制代码；seL4 内核 GPL-2.0-only，代码一字不进）。
- **GPL/AGPL 项目（Linux、xv6、Genode、rCore-Tutorial）只作思路参考，不复制代码**，以免协议传染。本仓库许可证 Apache-2.0（ADR #37）。
- 详细对照建议见 `reference/README.md`。

## Do / Don't

**Do**
- 把新功能先写成架构无关的 Rust。
- 每个可启动里程碑单独成一个提交。
- 在 `reference/` 里沉淀对照实现，写清出处与协议。

**Don't**
- 不要为了方便把 FS / 驱动塞进内核。
- 不要在内核里散落 `unsafe`（收敛到页表/上下文切换/MMIO/分配器）。
- 不要跨 syscall 边界 panic。
- 不要让 libc 桩在 std 启动关键路径上返 ENOSYS：`sanitize_standard_fds` 对 poll 非 EINTR 失败直接 `libc::abort()` 静默死（G-eco-d 实踩——bare `abort`、零文案）；能谎报成功就谎报（fd 恒有效语义），或改真实现。
- 不要在 syscall handler 里解蔽 IRQ 或忙等跨任务：SVC 入口硬件置 DAIF → syscall 主体不可抢占是 v1 免锁根基（3-4a 已拆 per-process 表，跨表访问仍全靠它串行）。
- 不要把「映射中 VMO 拒 close」写成一律拒：只拒**销毁性 close**（refs==1 且 mapcnt>0 → -EBUSY），别名 close（refs≥2）放行——对象不死，存活句柄钉住它。
- 不要在内核对象间存裸指针跨界引用（Thread→Process、Process→root Vmar 等）：必须 `ObjRef::retain/release` 钉住（3-3 起模式，见 `kernel/src/process.rs`）——否则持有者关光 handle 即 use-after-free。
- 不要假设 QEMU 启动在固定 EL：默认 `-machine virt`+`-kernel` ELF 起始 EL1，`virtualization=on` 起始 EL2——boot 第一行 `mrs CurrentEL` 检测并按需 eret 降级到 EL1。
- 长命睡眠内核任务不睡共享表（4-5 纪律；4-6 新形态：被错捐 eff 32 的睡眠者自己抢片，把断言对象推过自身重试预算——验收司机/probe 也挂私有表，重试预算按「最坏轮转+自己偷片」放宽；halt 后「全系统冻结」先查 panic 尾巴）。
- `for_each_sleeper` 须 IRQ 屏蔽窗内调用（tick 摘队动睡眠链接——任务体直调须手动 disable/enable 包络）。
- 不要复制 GPL/AGPL/MPL 文件进自研内核（主参考用 Rust OS：Redox/Theseus/zCore；hubris 只借语义）。
- 用户程序编译链走 builtin `aarch64-unknown-none` + build.rs 注入链接脚本（stable 免 nightly，G-eco-a 决策勿回退 custom target；`-zmax-page-size=4096` 必带否则 0x1000 基址表达不了）；`kernel/src/userbins/*.bin` 是 gitignored 编译产物——缺产物 = 裸 `cargo build` 编译期炸响（设计如此），`just build/check/build-release` 已挂 build-user 依赖自动补产。
- asm 模板换轨编译产物时，重试形态（自旋 vs 阻塞）是契约的一部分（G-eco-b 实踩）：逐发送点核对 asm 标号 EBUSY 分支去向（`b Nb` 回环自旋 vs clock+wait 阻塞），不能按家族归纳——console 饱和窗口内阻塞等待者会被自旋友商抢槽饿到超时；多页平像 spawn 的 sp 读 `UserProcess::stack_top` 字段；svcmgr build.rs rerun-if-changed 须盖全 staged 平像（漏列 = 改了子程序、内核还嵌旧字节静默上线）。
- console 通道纯自旋 send = 长跑饱和下结构性饿死（雷 2 残留实证；与上条对账——自旋赢 boot 竞速、输长跑公平）：自旋者从不阻塞 → 环序冻结 → 排水窗口被占优者垄断（五任务 30s 零成功发送 + pinger -110 下游晚杀）。新用户程序打印一律阻塞形态（EBUSY → wait_one(console, WRITABLE, 10s) → 重发，超时重臂）；svcmgr 豁免亦于 P5 A-2 撤销（send_spin 全量→send_retry 阻塞——全场生产者皆阻塞，无一幸免）。
- **wait_many 恒满足项使常驻服务永不入睡 = drain 相位钉死 RR 环（P5 A-2 实踩，上条姊妹坑）**：等待数组里「对端永不满 → WRITABLE 恒真」的项让 wait_many poll-first 每次即返、服务从不进 SLEEPQ → drain 相位冻结 RR 环固定节点 → 环序固化后填槽序末位者结构性饿死。修法 = 恒满足项条件化入列（仅 plen>0 才入列该项，常态真睡）。给常驻服务加 wait 项先问：稳态下会不会恒满足？
- 不要用 8 字节写 handle 出参（G-eco-c 实踩）：ABI §8 冻结 handle=u32，内核一律 `copy_out_u32`——8 字节写会砸编译 Rust 紧排的邻接局部（asm 模板 8 间距手排槽 + ktest `as u32` 截读曾掩盖）；非 handle 出参（ns/size/vaddr/idx 等 usize）仍 8 字节，宽度跟 ABI 类型走。真实链接器 PT_LOAD 不页对齐（RO 段与 text 共享页）——解析器不收页对齐，消费端页铺平单 VMO 消化。
- std 程序栈按程序面给 + 映像/栈之间必设保护页（G-eco-d d-3c hexyl 实踩）：默认 8 页栈被 ≈64KB 用量下行扎进映像 RW 页腐化 .bss（首击 `__aarch64_have_lse_atomics` → LSE 误判 `casa` UNDEFINED，死状与死因南辕北辙）——栈页数参数化（hexyl 32 页）+ 保护页（栈穿 = 干净 D-abort）。多分片用户输出不可直接当 smoke 标记（>192B 经 console 分片会被稳态矩阵插花撕行）——私有 stdout 通道收齐 + 零长 EOF（`park` 写侧约定）+ 宿主基准逐字节比对后才打单消息原子标记（`HEXYL ok` 模式）。
- 模板握手信号用专用位、清位豁免——模板不烤调度时序假设（P5-0 实踩，waiter 位拆分 a27eb9c）：一次性无确认 signal 与对端多段前置窗口天然竞速（死状双相位：早到被清位抹掉→10s 空等 / 意外命中→严格断言炸）；修法 = 握手专用位 + 全部清位掩码豁免 + observed 断言前掩位——任意调度序成立。现役用户位 1<<32～1<<39（U4=握手专用 P5-0 起、1<<39=RX Event P5 A-1 起），新增前先查碰撞。
- GIC SPI 段 ITARGETSR 复位 0 = 静默不投任何核（P5 A-1）：PPI 只读惯例不可照搬——不真写 target 则 pending 挂着无人收（完全静默）；新 SPI 源走 `gic::enable_spi` 一次齐。
- 电平触发源去活电平必须在 EOIR 前 + `exception_irq` 已按 INTID 分流（P5 A-1）：PL011 RX = drain 到 RXFE（不排空 → EOIR 后线仍高立即重投活锁；CNTV rearm 同铁序）；tick 槽只对 CNTV（新源进 tick 路径 = 授片与 CVAL 脱钩）；新源加 match 臂。PL011 FEN=0 有效 FIFO 深 1 字节——QEMU chardev 流控押住输入不 overrun（INT_OE 从不置位，真机才丢字；QEMU 唯一丢字向量 = 环满 lost），消费面按打字速率（smoke 逐字节慢注入 = 标记确定性）。rx_irq/rx_init 尾 ICR 勿清 RXIC（0x7ef 非 0x7ff，对抗审查实抓）：QEMU INT_RX 仅入队 count==1 严格判等置位——drain 末次 FR 读与 ICR 写间的竞速字节锁存被抹 = 滞留 + 线永低 + 流控押住 = RX 永久静默挂起；掩 RXIC 靠电平重投兜底。FEN=1+RT 留后续实测（QEMU 对 RT 建模未验，别盲开；A-2 为 bsh 未做此面）。console spawn arg 已满 → 新 handle 走静态槽（RX_EV_H/RX_EV_REF，ObjRef retain 永不 release）。
- x86 asm 三坑（P7-x0 实踩）：①rustc 的 x86 内联 asm 默认 **Intel 语法**（本仓 aarch64 全线 AT&T）——每处 global_asm!/asm! 都要 `options(att_syntax)`；②x86_64-unknown-none 默认 **PIC**，`.long _start` 类 R_X86_64_32/32S 对 local symbol 拒链——`.cargo/config.toml` 钉 `relocation-model=static`；③根 .cargo/config.toml 锁 build.target=aarch64 且 **config 发现跨 workspace 边界照样命中**——x86 构建必须显式 `--target x86_64-unknown-none`（justfile build-x86）。
- x86 链接脚本 `.note*` 绝不能 DISCARD（P7-x0）：PVH 靠 ELF note（`Xen`/type 18）被 QEMU 识别——丢了 note = 静默落 multiboot 路拒收 ELF64。note desc 用 `.long _start` 符号引用非硬编码地址；boot 页表放 .rodata 文件面预零（PT_LOAD 装载即零，弹床免清——硬件写权限看 PTE 位不看 PT_LOAD 旗标）。
- x86 ISR 桩对齐两层（P7-x1 对抗审查实抓）：①锚 rbp 后 `and $-16` 只归一被打断上下文奇偶；②call 边界 SysV 还要求 call 前 RSP ≡ 0 (mod 16)——`and` 后压**奇数个** 8B 寄存器再 call 即违约（须 `sub $8` 凑偶）；纯整数 handler 今日零可观测，克隆进含 movaps 的调用树即 #GP 三重故障静默退出。新桩照 timer_isr 现形态抄。
- 不要只等 EOF 收集被杀/异常退出的子进程 stdout（C-4b 实踩）：零长 EOF 只挂 libc exit 族钩子，`process_terminate`/内核杀无终点消息——收集方 wait_many 必挂 TERMINATED 项收口（数组序即命中序：EOF 已排队时 READABLE 恒先于 TERMINATED，正常退出零漂移；`killed=true && eof=false` 为合法终态）；v1 channel 无对端关闭检测，EOF 语义补全挂 v2。
- x86 ISR 桩不保存 XMM 的前提 = builtin target 禁 SSE（P7-x1 挂账）：换 custom target JSON 自查两件——disable-redzone=true（builtin 已禁，三重实证；**不需要也不可加 flag**）+ 保持 -sse/-mmx 软浮点（或 ISR 面补 XMM 保存），见 `arch/x86_64/.cargo/config.toml` 注。
- 8259 PIC init 的 IMR 敞开窗（P7-x1）：ICW 序列清 IMR = 全部 IRQ 短暂敞开——init 整个包在 IF=0 内，ICW4 后立即 OCW1 掩回（主片 0xFE / 从片 0xFF）；ICW 间 io_wait()（inb 0x80）。
- asm 勿给「架构上真读写内存」或「正确性依赖与相邻内存写保序」的指令标 nomem（P7-x4 对抗审查实抓）：nomem = 声明无内存副作用 ⇒ 编译器可跨块重排内存访问；volatile 只约束 volatile 操作彼此顺序。实例：invlpg 与页表 U/S RMW 零寄存器依赖即被上提（SDM §4.10 分页结构缓存——store 不刷缓存，失效上提过 RMW 即落空；TCG 不建模该缓存 = 冒烟恒绿真机才爆）；ltr 架构上真读 GDT 描述符。判别：读/写可访问内存（LGDT/LTR/INVLPG 类）或依赖与相邻内存写的顺序——任一 yes 不标 nomem（nostack/preserves_flags 照留）；aarch64 mmu.rs tlbi 同不标，本仓既有先例。
- 分配器空闲头索引不得用全窗线性扫——「某阶有无空闲头」由镜像布局掷骰（K-1 实抓，2026-10-27）：buddy `find_free_head` 旧版对空阶付 O(num_pages) 保证失败页扫（32768 页 ×7 指令 ≈ 28k icount），而某阶空不空由保留前缀 k=`__kernel_end>>12` 的**奇偶**（boot 期拆分史 × 镜像是否跨页）决定——任何让内核镜像恰好跨页的无害 commit 都在重掷这颗骰子（94c1fd5 .rodata +0x1000 → k 181→182 奇→偶 → ktest 后窗内零个 order-0 空闲头 → frame_alloc4k ×59 回归；frame_alloc64k 自 B-0 起潜伏 ×58——单点基线对「布局敏感的双峰指标」无守护力，icount L2 精确相等门禁才替它显形）。修法 = **每阶位图 + 摘要位图两级索引**（置/清位 O(1) 维护摘要，查询 = 摘要字扫 + `trailing_zeros`，返回与旧线性扫同为最小页号优先——host 对拍 `bitmap_index_matches_linear_scan` 4000 步钉板）；**禁止**用死垫/链接脚本钉 k 奇偶或放宽容差治标——前者仍是骰子（下次跨页又翻面），后者销毁 icount 通道的确定性价值。新写分配器/查找结构先问：失败路径的代价是否与容量线性、是否被纯布局参数翻转。
- 共享 target dir 跨 worktree 构建污染——指纹不区分来源、后建者胜（K-1 审查实抓，2026-10-27）：本机全局 `~/.cargo-target` 被主树与 probe/实验 worktree 共用，cargo 对「跨 worktree 同路径依赖包」的指纹不足以区分来源——worktree 里 build 过后回主树再 build，rustc 可能链到 worktree 侧的 rlib（实抓形态：主树 `just build` 报 E0061「FrameAllocator::new 取 4 参但给了 6 参」——链的是 worktree 的旧 berry-mm；touch mm/kernel 源强制重编后恢复，反向（主树→worktree）亦复现）。「同 commit 重建 ELF 有 32B 抖动」类灵异多半同源（共享 target 残留）。纪律：跨树构建后回主树必先 touch 改动面源码再 build（既有纪律不变）；worktree 构建一律独立 `CARGO_TARGET_DIR`（`.k2-target` 先例，K-2 probe 实操）——从源头免疫共享污染；跨树/复算 bench 前用 `python3 tools/size.py $(bash tools/paths.sh kernel)` 核 ram_total 指纹（K-1 主树 = 802,816B/k=196）——确认测的是哪个内核再下结论。
- 低半 boot 代码的常量在 release 优化下可 ADRP 引高半 .rodata.cst16 越界断链——debug 绿不代表 release 绿（K-2 实抓，2026-10-27）：release（fat LTO + codegen-units=1 + O3）把 `.text.boot` 内 `build_boot_pt`（arch/aarch64/src/mmu.rs）的 RAM L2 填表循环全展开+常量折叠——64 个块描述符被合成 512B `.rodata.cst16` 常量池（**匿名 .LCP 符号，`#[link_section]` 钉不住段位**），代码生成 32 对 adrp+ldr q 从池装载 + stp q,q 向量化直写；链接脚本把 `.rodata.cst16` 收高半而 build_boot_pt 在低半——ADRP ±4GB 跨 ~512GiB 越界断链（2b-1「ADRP 双向坑」的实例）；debug（opt=1）保留标量循环、常量走 movz/movk 立即数故全绿——「debug 绿 release 炸」形态由此而来。修法 = 表写入 **volatile 标量逐写**（`(&raw mut …).write_volatile`）——从语义上禁绝向量化/常量池化（LLVM AArch64 标量整数立即数只走 movz/movk 不走 literal pool），boot 面常量钉低半。守卫 = `just build-release` 挂 CI（release 链接守卫，只构建不跑 QEMU）——CI 此前对 ARM64 内核 release 面零覆盖，断链结构性不可见。首破点未定案：无 last-good 锚点（run-release 无绿色实录 + 仓库无 rust-toolchain 钉版，rustc 代码生成漂移是 commit 空间外变量），结构性下界 = 3f2017c（2b-1 高半翻转）。
