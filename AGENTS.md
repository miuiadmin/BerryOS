# AGENTS.md

> 跨工具通用（Cursor/Copilot/Codex/Claude Code…）。规则单一事实源 = 本文件。
> Claude Code 的 CLAUDE.md 以 `@AGENTS.md` 导入语法引用本文件——两文件同步合同作废。
> 判例全文 → `docs/雷册-判例库.md`（§A 内核侧 / §B 通用侧 / §C 构建标记）；文档详表 → `docs/索引.md`。
> 沿革/里程碑流水 → `docs/阶段沿革.md` 末尾追记，勿回写本文件；本文件只载「当前状态 + 铁律路由」，预算硬帽 200 行。

## 项目身份

**BerryOS** —— 自研微内核 + 跨平台操作系统，全 Rust（内核 `#![no_std]` + 用户态服务），ARM64 首发，QEMU virt 主验。
v1 主战场 = **移动设备（手机）** rich 档 → PC；手表/穿戴 = 同族低功耗端随时可落；MCU 档排除；v2 攻工业 RT；
x86_64 = QEMU 最小集提前 + 兼容验收（ADR #39–#41）。身份支柱：微内核 + 省电 + **AI 原生调度** + PC 高性能。
当前在哪一站 → `docs/路线图.md`；57 条 ADR 单一事实源 → `docs/架构决策.md`。

## 三原则（钦定）

1. **人无我有，人有我强**——借鉴任何参考前先过 `docs/架构综合纲领.md` 的排除 8 问；证据库 = `research/`。
2. **AI 友好化**——一切接口对 AI 可读、可推断：ABI 单源生成、结构化 tracing、语义化错误码、机器可读文档。
3. **AI 测试友好化**——确定性可复现、可编程故障注入、机器可读遥测/断言、QEMU 自动化关卡（G0–G5）。

## 微内核五原则

1. 内核极简：FS/网络/驱动是用户态服务——先问「这能不能放用户态？」
2. IPC 是中枢：显式消息传递，内核内禁直接共享可变状态。
3. 能力即权限：capability handle 派发，不做全局权限位检查。
4. 硬件抽象隔离：架构相关代码只活在 `arch/<arch>/`，核心逻辑架构无关。
5. 失败隔离：服务崩溃由内核监督重启，不波及系统。

## 架构决策速览（冻结常量；全表 → `docs/架构决策.md`）

- 哲学：Zircon 务实派（内核有对象/分配器）；权限：纯能力（内核态 handle）。
- 内存：VMO + 用户态 pager + 每进程独立 AS；实时/FS：软实时 + FAT MVP。
- IPC：同步 channel + 异步通知 + 共享内存；路径式命名空间；FS/网络全走 IPC。
- syscall：~30–45 号（v1 冻结 34 在册）；边界 C-ABI（`extern "C"` + `#[repr(C)]`，两端皆 Rust）。
- 软件：自有 ABI + POSIX 兼容库，不跑 Linux 二进制，**v1 无 fork**；启动：QEMU `-kernel` 直加载 + EFI。
- 并发：细粒度锁 + 全可抢占（分阶段）；驱动：用户态为主，启动关键（GIC/timer/UART）暂留内核。
- 调度：机制在内核、策略可插拔（class_rt/fair/deadline/idle）；tickless day-one；双线并行（省电+多核）。
- intent = 第一类调度输入（u8，#42 两段式）：公平是机制默认，intent 是策略叠加永不替代；#58 单一词汇表（SCENE/VOTE 折算进六档）。
- AI 原生调度：内核留带内接口给用户态策略（ADR #21 修订：机制预留 + 实验轨道 + kill criteria）。
- v1 基线：单核 + 抢占点 + 无 demand paging + 最小能力/IPC，逐版推向满配（关卡 G0–G5）。

## 总闸：动代码前先扫域铁律

**任何改动动手前，按下表读对应 `docs/规则/<域>.md`**（域铁律 = 判例瘦行 + 判别问句；全文再深一层 → 雷册 §A/§B）。
unsafe 纪律全域适用：只进页表/上下文切换（寄存器保存）/MMIO/分配器/硬件原子，**外层包安全 API**——`tools/unsafe_audit.py` 同口径。

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

## 仓库布局

```
kernel/ mm/ sched/ ipc/            Rust no_std 内核本体（调度/IPC/地址空间/能力）
arch/                              HAL + 内联汇编（aarch64/ 现役 + x86_64/ 最小集）
bootloader/ drivers/ fs/ net/      EFI 引导 / 用户态驱动工作区 / 用户态服务
user/ tools/                       应用生态与 berry-sys/libc / 构建脚本（just + smoke）
docs/ research/ reference/         文档 / 调研报告（仅私密仓）/ 参考实现（仅私密仓，协议隔离）
```

## 命令速览（仓库根；产物在 `~/.cargo-target`，路径解析单源 = `tools/paths.sh`）

前置（macOS 一次性；stable 预编译 core 免 nightly；brew 的 qemu 含全 arch）：

- `brew install qemu just`
- `rustup target add aarch64-unknown-none x86_64-unknown-none aarch64-unknown-linux-musl`

```bash
just build          # 内核 ELF（自动先跑 build-user 补内嵌产物）
just build-user     # 用户程序 → 平像 + mkapp 打包 .bap + mkcpio 产 rootfs.cpio
just run            # QEMU virt 冷启动，行首 1 = 起始 EL1；halt 后 Ctrl-A X 退出
just run-el2        # virtualization=on，行首 21 = EL2→EL1 eret 降级
just run-rootfs     # fw_cfg 挂 rootfs + virtio 盘网 → svcmgr 消费链全谱（rootfs 相 47 标记）
just smoke          # 无人值守八跑冒烟（CI 同款）；标记清单 = tools/smoke.sh，语义注解 → 雷册 §C
just test-host      # host 单元测试（不依赖 QEMU）；host 面总口径以本命令输出为准
just abi-check      # ABI 单源守卫：abi.toml 三面产物逐字节比对（挂 build/smoke 前置）
just abi-gen        # ABI 再生（改 ABI = 改源 → abi-gen → diff 随源入册）
just size           # 内核体积基线报告（CI 同步打印）
just build-x86      # x86_64 构建（必须显式 --target——根 config 锁 aarch64 跨 workspace 命中）
just run-x86        # QEMU pc + PVH ELF，七标记（PVH/TIMER/EXC/CTX/SYS/X86 ok）
just smoke-x86      # x86 独立冒烟（7 标记恰一次；已挂 CI，不并入主 smoke）
just run-smp        # 副核 bring-up + 参与态手跑（全仓唯一 -smp 入口）
just smoke-smp      # 副核五相冒烟（独立关卡不进 CI）：相1/2 SMP/SGI/SGI2 恰一次 + 金丝雀递增门；相3 LB 三硬门（c0=0/c1≥TH/n=4）+ 相4 ASIDR 门（n≥1）+ 相5 TLBS 门（n=10——4-4 canary 压测双轮）
just smoke-image    # EFI 整盘镜像独立冒烟（已挂 CI）：十段 + 字节对拍 + RX 注入
just build-boot     # BerryBoot EFI 构建链（staticlib → rust-lld 全链接 → elf2pe PE32+）
just image          # 整盘镜像 → out/berryos.img + manifest.txt（同输入同字节）
just run-efi        # AAVMF 挂 berryos.img + ramfb——十段引导 + 内核接续
just build-release  # release 链接守卫（挂 CI）：fat LTO ADRP 断链形态补面 + assert-flip
just run-release / just check / just clean
just bench          # B-0/B-1 基准：BENCH 六行 + 基线对拍；--el0 四项；--update 分段刷新
just bench-rt       # R-1 实时基准（fw_cfg 门控，缺席零扰动）
just bench-icount   # B-2 -icount 确定性通道（独立基线；L2 紧门禁 --prove 五跑烘焙；跨机不可比）
just soak [分钟]    # soak 长跑心跳四断言（不进 CI；勿与 smoke/run-rootfs 并行——echod 18080 冲突）
```

TCG 下 CNTV tick 实测 ≈ **SLICE_HZ/2**（非内核 bug；详 `规则/08-QEMU与仿真.md` / 雷册 §A8）——读切换率别被 2× 误导。

## 维护合同

- 改约定（新架构/语言边界/新命令）→ 回写本文件对应速览节；阶段进展/标记沿革 → 只写 `docs/阶段沿革.md`。
- 新判例 = 域文件加瘦行 + 雷册对应节追加全文，两处一一对应（拆分契约 → `docs/规则/README.md`）。
- 行数帽/探针/锚点由 `tools/docs_guard.py` 执法（AGENTS ≤200 行、行长 >100 警 >120 拒）。
- 新文档落库/标签更新 → `docs/索引.md` 维护规则四条。
- 小步提交：能在 QEMU 独立验证的里程碑单独成一个提交；写前先查 `reference/` 对照实现。
