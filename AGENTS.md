# AGENTS.md

> 跨工具通用的项目说明，任何 AI 编码代理（Cursor / Copilot / Codex / Aider / Claude Code …）在读到本仓库时应先读此文件。
> Claude Code 专用的会话工作规范见 `CLAUDE.md`——**规则层同步维护**（约定/铁律/命令口径两文件一致；本文为规则层单一事实来源）；判例与构建标记全文只在 `docs/雷册-判例库.md`，两文件以瘦行 + §A/§B/§C 锚点索引。

## 这是什么

**BerryOS** —— 自研的微内核（microkernel）+ 跨平台操作系统。目标是极简内核 + 用户态服务化，首批跑在 **ARM64 (AArch64)** 上。端定位（ADR #39–#41，#39 2026-10-28 修订）：v1 主战场 = **移动设备（手机）** rich 档（MB 级内存）单线走到底——主战场 = 手机 → PC；手表/穿戴 = 同族低功耗端（省电设计直接受益、又低功耗又优秀，随时可落，勿以手表锚死 v1 范围）；MCU 档（KB 级）排除；v2 攻工业 RT；x86_64 提前启动 **QEMU 最小集**移植（范围盒子 + kill criteria），长期为兼容验收目标（XNU 同源分端模式）。

**身份支柱**：微内核 + 省电（移动设备/手表）+ **AI 原生调度**（内核留第一类接口让用户态 AI 策略服务干涉调度，见 `docs/AI调度设计.md`）+ PC 高性能——主战场 = 移动设备（手机）与 PC，手表为同族低功耗端。

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
| **端定位** | v1 = **移动设备（手机）** rich 档 ARM64 单线（主战场 = 手机 → PC）；手表/穿戴 = 同族低功耗端随时可落；MCU 档排除；v2 攻工业 RT；x86 = 兼容验收 + QEMU 最小集提前（ADR #39–#41） |
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
- **capacity(0..1023) 抽象**：每个 CPU 一个算力值，统一手机大小核 → PC 全对称 → 手表单核小算力（同一路径覆盖，端无主次）；拓扑由 `arch/<arch>/` 填表，核心逻辑架构无关。
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

唯一详表 = `docs/索引.md`（59 份文档分类/状态标签/双阅读路径/维护待办）；本节只留分类速览。动相关子系统前先读对应设计文档：

- **总纲与路线**：`docs/架构决策.md`（57 条 ADR 单一事实源）· `docs/架构综合纲领.md`（取舍纪律 + 排除 8 问——借鉴任何参考前先读）· `docs/路线图.md`（Phase 0–8，现行 M-VM）· `docs/优化议程.md`（五弱题动作表）。
- **内核核心**：`docs/能力模型设计.md`（纯能力 handle）· `docs/IPC设计.md`（channel/Event/intent）· `docs/内存管理设计.md`（VMO/pager）· `docs/系统调用ABI设计.md`（v1 冻结，abi.toml 单源）· `docs/调度器设计.md`（机制 + M 里程碑权威）· `docs/AI调度设计.md`（ADR #21 修订版）· `docs/可观测性设计.md` · `docs/安全与硬件隔离设计.md`（挂 v2）。
- **服务与生态**：`docs/文件系统设计.md` · `docs/驱动模型设计.md`（驱动=持证应用）· `docs/shell设计.md`（bsh + 命令梯队）· `docs/应用模型设计.md`（包 + 清单 + 能力）· `docs/GUI里程碑规划-2026-10.md`（M-GUI 刀序 GUI-0..9 + 裁决 A–I——ADR #50–#54：B 形态协议 / pass 形态 API / berryui 声明式 / 低功耗缝 / SDK 序）· 挂 v2 四份：`docs/存储布局与沙箱设计.md` / `docs/兼容子系统设计.md` / `docs/更新与OTA设计.md` / `docs/AI工程面设计.md`。
- **验证与流程**：`docs/测试策略.md`（G0–G5 关卡）· `docs/开发体验.md`（四级阶梯 + 日常循环）。
- **在途拆解**：`docs/VM里程碑设计草案.md`（现行主里程碑 M-G1..G3）· `docs/SMP设计草案.md`（G2 拆步，S-0/S-1 已落）· M-ECO 事件循环线 A 系草案 ×7（`容量工程前置-A1设计草案.md` + `事件循环A2..A7设计草案.md`——A1-A6 已落地 2026-10-30 收官（六标记 POLL/LSN/EVS/SKT/EPL/NETS 入 rootfs 相 37-42，A6-0 盘点在册）、A7 POSIX 原语批已收官 2026-10-31（A7-1 匿名 mmap/A7-2 timer/A7-3 文件映射+mprotect dormant 已落地 2026-10-30——MMP n=9/CLK n=6；信号面门 C 已裁定 A 2026-10-30（add-only 加号放行）；A7-4 信号面已落地 2026-10-31——方案 B 协同式用户态投递（0x86/0x87 内核两号 + libc 投递循环/sigaction 表/掩码/held 再投 + bsh 六步 `SIG ok n=6` 入 rootfs 相 45，`docs/事件循环A7-4信号面实施设计草案.md`）；FPU 出局归 M-GUI 线））· `事件循环A7-{2,3}实施设计草案.md`、`事件循环A7-4信号面实施设计草案.md`。
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

**启动输出速览**（标记清单单一事实源 = `tools/smoke.sh`；标记沿革逐字归档 → `docs/阶段沿革.md`；**标记语义注解与命令注释全文快照 → `docs/雷册-判例库.md` §C**）：

- `just run` / `just run-el2`：裸相 27 标记（行首 1/21 = 起始 EL）——`BerryOS boot` → `KVA ok` → `ROOTFS ok/absent` + `ROOTFS vmo=` 双行 → ktest 各段 `… ok` → BENCH 六行 + `BENCHCHURN ok`/`BENCHXH ok` 金丝雀 + `BENCH ok n=6` → 稳态矩阵（`switches=N` 金丝雀严格递增）→ watchdog `G1.5 ok` → `BENCHWAKE` 直方图。
- `just run-smp`（S-1，**全仓唯一 -smp 入口**）：`SMP: cpu_on entry=<hex>` → `sSMP: sec up el=1 …`（行首 `s` = 副核哨兵）→ `SMP ok ncores=2`（心跳状态谓词）；单核/EL2 机型/EFI 三降级路径 boot 全标记零扰动。
- `just run-rootfs`：rootfs 相 45 标记——svcmgr 消费链全谱：`ECO ok`…`APP ok n=2` + `FS`/`VIRTIO`/`VBLK`/`BLKD`/`FAT`/`FATW`/`NET`/`DNS`/`TCP`/`TCPS ok` + C 系 `DIR`/`CD`/`U1`–`U4`/`B1`/`DF`/`TAB`/`INT`/`PS`/`BC`/`M3DEMO` + T-1 `THR` + A 系 `POLL`/`LSN`/`EVS`/`SKT`/`EPL`/`NETS`/`MMP`/`CLK`/`SIG`；A 系排 fat 族前 + netd 拓扑降级双门（netd_h()==0/ENODEV 静默跳过）；裸跑双行降级（`ROOTFS absent`/`vmo=none`）零扰动。逐标记语义 → `tools/smoke.sh` + 雷册 §C。
- `just smoke`：八跑 CI 冒烟——裸相 27 + rootfs 相 45 + RX 注入相（rootfs+FIFO 合相 72，含 Ctrl-C 杀段）+ FAT 子目录相 + FAT 持久化相（脏盘跨 boot 读回 f42t + `cat T3.TXT` 断言 x1y）+ B4 掉电相（独立盘三 boot 采样 + `pwrcls` 毒环注入，guest 标记与宿主盘面双面不变量）+ 金丝雀严格递增；`SMOKE_KEEP_LOG=1` 留日志。`smoke-x86`（7 标记）/`smoke-smp`（SMP ok 恰一次）独立关卡不并入主 smoke。
- `just run-efi`：AAVMF 挂 `out/berryos.img` 自举——BerryBoot 十段（`fw:`/`gop:`/`fs:`/`kfile:`/`init:`/`elf:`/`load:`/`mmap:` → `BERRYBOOT ok`）→ 内核接续（`HANDOFF ok` → `MMAP ok` → `ROOTFS ok src=handoff`）→ 裸相 27 + EFI 路生态 27（无 virtio 面；LSN/EVS/SKT/EPL/NETS 拓扑门静默不打 = 拓扑真值；GUI-1 起 +FBW/FBSERVE 双路上屏）；盘形制 = MBR + FAT16 96MiB + 0xDA 预留槽 16MiB（`just image` 单口 + `manifest.txt` 侧产物）。十段全文 → 雷册 §C。

**前置（macOS，一次性）**：`brew install qemu just` + `rustup target add aarch64-unknown-none`（stable，无需 nightly/build-std）；P7-x0 起 x86_64 最小集另需 `rustup target add x86_64-unknown-none`（同 stable 预编译 core）。

**日常命令**（仓库根）：

```bash
just build          # = cargo build（内核 ELF）；先自动跑 build-user 补产物
just build-user     # 用户程序编译 → 平像 + mkapp 打包 + rootfs.cpio
just run            # QEMU virt → 1BerryOS boot（默认起始 EL1），halt
just run-el2        # virt,virtualization=on → 21BerryOS boot（EL2→EL1 eret 降级）
just smoke          # 无人值守冒烟（CI 同款）八跑：相构成见上方速览；标记清单 = tools/smoke.sh
just abi-check      # ABI 单源守卫：gen_abi.py --check 逐字节比对 abi.toml 三面产物
just abi-gen        # ABI 再生（改 ABI = 改源 → abi-gen → diff 入册）
just size           # 内核体积基线报告（CI 同步打印）
just bench         # B-0/B-1 基准单口：默认裸跑 BENCH 六行 + 基线对拍；--el0 四项；--update 分段刷新
just bench-rt       # R-1 实时基准：fw_cfg opt/berry/rt 门控 → BENCHRT + RT ok n=96；门控缺席零扰动
just bench-icount   # B-2 -icount 确定性通道：独立基线；L2 紧门禁 --prove 五跑烘焙；跨机不可比
just soak [分钟]    # S-1 soak 长跑单口（默认 60）：fw_cfg 门控心跳四断言；不进 CI；勿与 smoke/run-rootfs 并行（echod 18080）
just run-rootfs     # G-eco：fw_cfg 挂 rootfs + virtio 盘网——svcmgr 消费链全谱（见上方速览）；SKT/EPL 主战场 = just smoke
just build-x86      # x86_64 最小集构建（**必须显式 --target**——根 config 锁 aarch64 跨 workspace 命中）
just run-x86        # QEMU pc + PVH ELF——七标记 PVH/TIMER/EXC/CTX/SYS/X86 ok
just smoke-x86      # x86 独立冒烟（7 标记恰一次；已挂 CI）
just run-smp        # S-1 副核 bring-up 手跑（全仓唯一 -smp 入口）
just smoke-smp      # S-1 副核冒烟（独立关卡不进 CI）：SMP ok 恰一次 + 裸相 27 + 金丝雀递增；时限 = 单核 ×2
just smoke-image    # V-4 img-boot 独立冒烟（已挂 CI）：EFI 十段 + 接续 + 生态 27 + 字节对拍 + RX 注入（EFI 路无 Ctrl-C 段——C-4b 裁定）
just build-boot     # BerryBoot EFI 应用构建链（staticlib → rust-lld 全链接 → elf2pe.py PE32+）
just image          # V-3 单口整盘镜像 → out/berryos.img（MBR+FAT16+0xDA 槽+四件套，同输入同字节）+ manifest.txt；--key-hash 只落 manifest
just run-efi        # AAVMF 挂 berryos.img + ramfb——十段引导 + 内核接续（见上方速览）
just build-release  # K-2 release 链接守卫（挂 CI）：release 独有 ADRP 断链形态的结构补面 + assert-flip
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

- 不要为了方便把 FS / 驱动塞进内核。 → 雷册 §B1
- 不要在内核里散落 `unsafe`（收敛到页表/上下文切换/MMIO/分配器）。 → 雷册 §B2
- 不要跨 syscall 边界 panic。 → 雷册 §B3
- **libc 桩勿在 std 启动关键路径返 ENOSYS（G-eco-d）**：`sanitize_standard_fds` 对 poll 非 EINTR 失败直接 `libc::abort()` 静默死（bare abort、零文案）——能谎报成功就谎报（fd 恒有效语义），或改真实现。先问：这个桩会被 std 启动路径引用吗、std 对它的失败是 abort 吗？ → 雷册 §B4
- 不要在 syscall handler 里解蔽 IRQ 或忙等跨任务：SVC 入口硬件置 DAIF → syscall 主体不可抢占是 v1 免锁根基（3-4a 已拆 per-process 表，跨表访问仍全靠它串行）。 → 雷册 §B5
- 不要把「映射中 VMO 拒 close」写成一律拒：只拒销毁性 close（refs==1 且 mapcnt>0 → -EBUSY），别名 close（refs≥2）放行——对象不死，存活句柄钉住它。 → 雷册 §B6
- **内核对象间跨界引用必须 `ObjRef::retain/release` 钉住（3-3）**：Thread→Process、Process→root Vmar 等只存裸指针 = 持有者关光 handle 即 use-after-free（模式见 `kernel/src/process.rs`）。先问：这个跨对象引用的生命周期被谁钉住？ → 雷册 §B7
- **勿假设 QEMU 启动固定 EL**：默认 `-machine virt`+`-kernel` ELF 起始 EL1，`virtualization=on` 起始 EL2——boot 第一行 `mrs CurrentEL` 检测、按需 eret 降级到 EL1。先问：这段 boot 代码假设的起始 EL 在两种机型下都成立吗？ → 雷册 §B8
- **长命睡眠内核任务不睡共享表（4-5/4-6）**：被错捐 eff 32 的睡眠者自己每轮抢片，把断言对象推过自身重试预算——验收司机/probe 也挂私有表，重试预算按「最坏轮转+自己偷片」放宽；halt 后「全系统冻结」先查 panic 尾巴。先问：这个长命睡眠任务稳态会不会错捐/偷片？ → 雷册 §B9
- `for_each_sleeper` 须 IRQ 屏蔽窗内调用（tick 摘队动睡眠链接——任务体直调须手动 disable/enable 包络）。 → 雷册 §B10
- 不要复制 GPL/AGPL/MPL 文件进自研内核（主参考用 Rust OS：Redox/Theseus/zCore；hubris 只借语义）。 → 雷册 §B11
- **用户程序编译链走 builtin `aarch64-unknown-none` + build.rs 注入链接脚本（G-eco-a）**：stable 免 nightly、勿回退 custom target；`-zmax-page-size=4096` 必带（否则 0x1000 基址表达不了）；`kernel/src/userbins/*.bin` 是 gitignored 编译产物——缺产物 = 裸 `cargo build` 编译期炸响（设计如此），`just build/check/build-release` 已挂 build-user 依赖自动补产。先问：裸 cargo build 前编译产物在位吗？ → 雷册 §B12
- **asm 模板换轨编译产物时重试形态（自旋 vs 阻塞）是契约的一部分（G-eco-b）**：逐发送点核对 asm 标号 EBUSY 分支去向（`b Nb` 回环自旋 vs clock+wait 阻塞），不能按家族归纳——console 饱和窗口内阻塞者会被自旋友商抢槽饿到超时；多页平像 spawn 的 sp 读 `UserProcess::stack_top` 字段；svcmgr build.rs rerun-if-changed 须盖全 staged 平像（漏列 = 改了子程序、内核还嵌旧字节静默上线）。先问：换轨前端口的 EBUSY 分支走向核对了吗、rerun-if 列全了吗？ → 雷册 §B13
- **console 通道纯自旋 send = 长跑饱和下结构性饿死（雷 2）**：自旋赢 boot 竞速、输长跑公平——自旋者从不阻塞 → RR 环序冻结 → 排水窗口被占优者垄断；新用户程序打印一律阻塞形态（EBUSY → wait_one(console, WRITABLE, 10s) → 重发，超时重臂；P5 A-2 svcmgr 豁免已撤销，send_spin 全量→send_retry）。先问：这个发送点在饱和长跑下退出过 runqueue 吗？ → 雷册 §B14
- **wait_many 恒满足项使常驻服务永不入睡 = drain 相位钉死 RR 环（P5 A-2）**：等待数组里「对端永不满 → WRITABLE 恒真」的项让 poll-first 每次即返、服务从不进 SLEEPQ → drain 相位冻结 RR 环 → 填槽序末位者结构性饿死；修法 = 恒满足项条件化入列（仅 plen>0 才入列该项，常态真睡）。先问：给常驻服务加 wait 项，稳态下会不会恒满足？ → 雷册 §B15
- **handle 出参宽度 = 4 字节（`copy_out_u32`，G-eco-c）**：ABI §8 冻结 handle=u32——8 字节写会砸编译 Rust 紧排的邻接局部；非 handle 出参（ns/size/vaddr/idx 等 usize）仍 8 字节，宽度跟 ABI 类型走、不跟「凑巧能跑」走；真实链接器 PT_LOAD 不页对齐（RO 段与 text 共享页）——解析器不收页对齐，消费端页铺平单 VMO 消化。先问：这个出参的宽度跟 ABI 类型对齐了吗？ → 雷册 §B16
- **std 程序栈按程序面给 + 映像/栈之间必设保护页（G-eco-d）**：默认 8 页栈会被大用量下行栈扎进映像 RW 页腐化 .bss（死状与死因南辕北辙）——栈页数参数化 + 保护页（栈穿 = 干净 D-abort）；多分片用户输出不可直接当 smoke 标记（>192B 经 console 分片会被稳态矩阵插花撕行）——私有 stdout 通道收齐 + 零长 EOF + 宿主基准逐字节比对后才打单消息原子标记（`HEXYL ok` 模式）。先问：栈够这个程序用吗、标记断言的是收齐后的整段吗？ → 雷册 §B17
- **模板握手信号用专用位、清位豁免——不烤调度时序假设（P5-0）**：一次性无确认 signal 与对端前置窗口天然竞速（双相位都炸：早到被清位抹掉→空等超时 / 意外命中→严格断言炸）；修法 = 握手专用位 + 全部清位掩码豁免 + observed 断言前掩位——任意调度序成立。现役用户位 1<<32～1<<39（U4=握手专用、1<<39=RX Event），新增前先查碰撞。先问：这个握手的正确性依赖调度时序吗？ → 雷册 §B18
- **GIC SPI 段 ITARGETSR 复位 0 = 静默不投任何核（P5 A-1）**：PPI 只读惯例不可照搬——不真写 target 则 pending 挂着无人收（完全静默）；新 SPI 源走 `gic::enable_spi` 一次齐。先问：新 SPI 源的 target 位真写了吗？ → 雷册 §B19
- **中断源纪律**：①电平去活在 EOIR 前（PL011 RX drain 到 RXFE；CNTV rearm 同）；②`exception_irq` 按 INTID 分流、新源加 match 臂，tick 槽只对 CNTV（授片/CVAL 脱钩）；③FEN=0 深 1B：QEMU 唯一丢字向量=环满 lost（真机才丢字），按打字速率消费；④rx_irq/rx_init 尾 ICR 勿清 RXIC（0x7ef 非 0x7ff）：INT_RX 仅 count==1 置位，锁存被抹即挂死；⑤FEN=1+RT 未验勿开；⑥console spawn arg 满→新 handle 走静态槽（RX_EV_H/RX_EV_REF，retain 永不 release）。先问：EOIR 序、INTID 分流核对了吗？ → 雷册 §B20
- **x86 asm 三坑（P7-x0）**：①x86 asm 默认 Intel 语法（aarch64 侧 AT&T）——`global_asm!`/`asm!` 须 `options(att_syntax)`；②`x86_64-unknown-none` 默认 PIC，`.long _start` 类 R_X86_64_32/32S 对 local symbol 拒链——`.cargo/config.toml` 钉 `relocation-model=static`；③根 config 锁 build.target=aarch64、跨 workspace 仍命中——x86 构建须显式 `--target x86_64-unknown-none`。先问：asm 带 att_syntax、构建带 --target？ → 雷册 §B21
- **x86 链接脚本 `.note*` 绝不能 DISCARD（P7-x0）**：PVH 入口声明靠 ELF note（`Xen`/type 18）——丢了 note = 静默落 multiboot 路拒收 ELF64；note desc 用 `.long _start` 符号引用非硬编码地址；boot 页表放 .rodata 文件面预零（PT_LOAD 装载即零——硬件写权限看 PTE 位不看 PT_LOAD 旗标）。先问：链接脚本 DISCARD 列表里有 .note* 吗？ → 雷册 §B22
- **x86 ISR 桩对齐两层（P7-x1）**：①锚 rbp 后 `and $-16` 只归一被打断上下文奇偶；②call 边界 SysV 还要求 call 前 RSP ≡ 0 (mod 16)——`and` 后压奇数个 8B 寄存器再 call 即违约（须 `sub $8` 凑偶）；纯整数叶子 handler 今日零可观测，调用树一出现 movaps 即 #GP → 三重故障静默退出；新桩照 timer_isr 现形态抄。先问：ISR 桩 call 前的 RSP 按 SysV 凑偶了吗？ → 雷册 §B23
- **收集被杀/异常退出的子进程 stdout 勿只等 EOF（C-4b）**：零长 EOF 只挂 libc exit 族钩子，`process_terminate`/内核杀无终点消息——收集方 wait_many 必挂 TERMINATED 项收口（数组序即命中序：EOF 已排队时 READABLE 恒先于 TERMINATED，正常退出零漂移；`killed=true && eof=false` 为合法终态）；v1 channel 无对端关闭检测，EOF 语义补全挂 v2。先问：收集方等的终态里含 TERMINATED 吗？ → 雷册 §B24
- **x86 ISR 桩不保存 XMM 的前提 = builtin target 禁 SSE（P7-x1）**：换 custom target JSON 须自查两件——disable-redzone=true（builtin 已禁，不需要也不可加 flag）+ 保持 -sse/-mmx 软浮点（或 ISR 面补 XMM 保存），见 `arch/x86_64/.cargo/config.toml` 注。先问：换 target JSON 后 SSE/red-zone 两前提还成立吗？ → 雷册 §B25
- **8259 PIC init 的 IMR 敞开窗（P7-x1）**：ICW 序列清 IMR = 全部 IRQ 短暂敞开——init 整个包在 IF=0 内，ICW4 后立即 OCW1 掩回（主片 0xFE / 从片 0xFF）；ICW 间 io_wait()（inb 0x80）。先问：PIC init 全程 IF=0、ICW4 后立即掩回了吗？ → 雷册 §B26
- **asm 勿给「架构上真读写内存」或「正确性依赖与相邻内存写保序」的指令标 nomem（P7-x4）**：nomem = 声明无内存副作用 ⇒ 允许跨块重排内存访问；volatile 只约束 volatile 操作彼此顺序、管不到普通 store 与 asm 相对位置（invlpg 与页表 U/S RMW 被上提即落空——TCG 不建模分页结构缓存 = 冒烟恒绿真机才爆；ltr 真读 GDT 描述符）。判别：读写可访问内存（LGDT/LTR/INVLPG 类）或依赖与相邻内存写的顺序——任一 yes 不标 nomem（nostack/preserves_flags 照留）；aarch64 mmu.rs tlbi 同不标。先问：这条 asm 有真实内存副作用或顺序依赖吗？ → 雷册 §B27
- **分配器空闲头索引不得全窗线性扫——「某阶有无空闲头」由镜像布局掷骰（K-1）**：buddy `find_free_head` 空阶付 O(num_pages) 失败扫，某阶空不空由保留前缀 k=`__kernel_end>>12` 的奇偶决定——让镜像跨页的无害 commit 都重掷骰子（单点基线守不住布局敏感双峰指标，icount L2 精确相等门禁才显形）。修法 = 每阶位图 + 摘要位图两级索引（置/清位 O(1)，查询 = 摘要字扫 + `trailing_zeros`，最小页号优先，`bitmap_index_matches_linear_scan` 对拍钉板）；禁止死垫/链接脚本钉 k 奇偶或放宽容差治标。先问：失败路径的代价是否与容量线性、是否被纯布局参数翻转？ → 雷册 §B28
- **共享 target dir 跨 worktree 构建污染——后建者胜（K-1）**：`~/.cargo-target` 被主树/worktree 共用，rustc 可能链到别树旧 rlib（如 E0061 签名数不符）；同 commit 重建 ELF 字节抖动多半同源。纪律：跨树回来先 touch 改动面源码再 build；worktree 构建独立 `CARGO_TARGET_DIR`（`.k2-target` 先例）；跨树/复算 bench 前用 `tools/size.py $(bash tools/paths.sh kernel)` 核 ram_total 指纹（测的是哪个内核）。先问：这个 worktree 的 CARGO_TARGET_DIR 独立吗？ → 雷册 §B29
- **低半 boot 常量 release 下可 ADRP 引高半 `.rodata.cst16` 越界断链——debug 绿不代表 release 绿**：fat LTO+O3 把 `build_boot_pt` 类填表循环折叠成常量池（匿名 .LCP，`#[link_section]` 钉不住）——池收高半而 `.text.boot` 在低半 = ADRP ±4GB 断链；debug（opt=1）走 movz/movk 故绿。修法 = volatile 标量逐写（`(&raw mut …).write_volatile`）禁绝向量化/常量池化；守卫 = `just build-release` 挂 CI。先问：boot 常量钉低半、release 有 CI 守卫吗？ → 雷册 §B30
