# AGENTS.md

> 跨工具通用的项目说明，任何 AI 编码代理（Cursor / Copilot / Codex / Aider / Claude Code …）在读到本仓库时应先读此文件。
> Claude Code 专用的会话工作规范见 `CLAUDE.md`，两者内容保持一致，本文为单一事实来源（source of truth）。

## 这是什么

**BerryOS** —— 自研的微内核（microkernel）+ 跨平台操作系统。目标是极简内核 + 用户态服务化，首批跑在 **ARM64 (AArch64)** 上。端定位（ADR #39–#41）：v1 主战场 = 手表/穿戴 rich 档（MB 级内存）单线走到底；MCU 档（KB 级）排除；v2 攻工业 RT；x86_64 提前启动 **QEMU 最小集**移植（范围盒子 + kill criteria），长期为兼容验收目标（XNU 同源分端模式）。

**身份支柱**：微内核 + 省电（手表/手机）+ **AI 原生调度**（内核留第一类接口让用户态 AI 策略服务干涉调度，见 `文档/AI调度设计.md`）+ PC 高性能。

## 设计总原则（三条，钦定）

1. **人无我有，人有我强**：博采众长的落地口诀——别人验证过的优点学过来，别人踩过的坑绕开。证据库 = `调研报告/`（各家好在哪/不好在哪/怎么做的）；借鉴任何参考前先过 `文档/架构综合纲领.md` 排除 8 问，防杂糅。软件行业积累到今天是做 OS 最好的时代：成熟参考实现全部开源可得，站在巨人肩上是常态而非例外。
2. **AI 友好化**：把 AI 当系统的一等公民——不止 AI 原生调度（`文档/AI调度设计.md`），一切接口对 AI 可读、可推断：ABI 单一事实源生成（`文档/系统调用ABI设计.md` §8）、结构化 tracing（`文档/可观测性设计.md`）、语义化错误码、机器可读文档。
3. **AI 测试友好化**：为"以后都是 AI 测软件"的时代预埋基建——测试基建是产品的一部分，不是附属品：确定性可复现（同输入同输出）、可编程故障注入、机器可读遥测/断言、QEMU 自动化关卡（`文档/测试策略.md` G0–G5）；目标是 AI agent 能独立跑测试、读结果、报 bug。

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
| **AI 原生调度** | 内核留接口让用户态 AI 策略服务干涉放置/频率/优先级；慢/快循环切分、RT 屏蔽、护栏、平台分层（见 `AI调度设计.md`）。**2026-10-16 修订**：干涉面限带内、AI 降级为机制预留+实验轨道+kill criteria |
| **调度 intent** | intent=第一类调度输入（u8）·两段式：v1/v2 查表键，M4 ×4 带+预算兜底，凭遥测入场券（ADR #42） |

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
- **intent 两段式（ADR #42，2026-10-16）**：**公平是机制默认，intent 是策略输入，叠加永不替代**。v1/v2 intent = u8 查表键（IntentProfile→latency-nice/EPP/uclamp/timer slack；TCB `intent_declared/effective` P3 预埋、`berry_intent` 编码冻结 0x00 UNSPECIFIED(XNU 式不设限)–0x05 MAINTENANCE(冻结不启用)）；M4 起 class_fair ×4 带实例 + 预算包络 + deadline-server 兜底，**凭 v1 遥测入场券**（不满足则单实例即终态）；三层拓扑 class 链 > band 序 > 带内 EEVDF；`SchedClass` trait 签名零改动；band/intent **不入 AI 动作空间**。

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
- `文档/优化议程.md` —— 五道弱题的「及格线 → 拉开差距」动作表（IPC 拳头 / 生态伺候 / 驱动热换 / 性能 scoreboard / AI 攒数据），13 号报告导出，按 Phase 挂钩。
- `文档/开发体验.md` —— Mac 上的运行/安装四级阶梯（QEMU 直加载 → initrd → fat:rw → 自举镜像）+ 交叉编译日常循环 + hvf/TCG 双模式。
- `文档/shell设计.md` —— P5 交互 shell（bsh）：命令三梯队、channel 即 pipe、与 UNIX 生态的源码级兼容边界。
- `文档/应用模型设计.md` —— 应用模型：包格式（cpio + manifest）/ 层级（层 = IPC 边界，硬边界唯一 + 执法三规则）/ 生命周期分级（active→freeze→墓碑，墓碑 v1 纯用户态）/ intent 单一消费轴 / GUI 可拔插（能力决定形态；ADR #43/#44）。
- `文档/兼容子系统设计.md` —— 扩展生态槽位：三层模型（T1 原生重编译 / T2 翻译子系统 / T3 虚机兜底）+ 四预 ADR 槽位（S1 syscall 人格转发 / S2 异常通道 / S3 路径命名空间 / S4 EL2 虚机化）；内核零 Linux 语义（面 = S1–S4 槽位）跑 Linux/Wine/APK（扩展包 = 应用包同源 manifest）；v2+ 议程。
- `文档/调度器设计.md` —— 调度器与电源详细设计。
- `文档/架构决策.md` —— 全部决策的单一事实来源（ADR）。
- `文档/能力模型设计.md` / `IPC设计.md` / `内存管理设计.md` / `系统调用ABI设计.md` —— 各子系统详细设计（均含「阶段化目标」节）。
- `文档/系统调用ABI设计.md` 另含具体 syscall 编号表 + POSIX 子集边界（v1 无 fork）。
- `文档/测试策略.md` —— 关卡式验证 G0–G5：单核正确 → 抢占 → SMP → 全抢占 → demand paging。
- `文档/AI调度设计.md` —— AI 原生调度：策略注入/遥测 ABI、安全护栏、平台分层。
- `文档/安全与硬件隔离设计.md` —— IOMMU/SMMU + PAC/BTI/MTE + 度量启动。
- `文档/可观测性设计.md` —— day-one 内置 tracing。
- `文档/文件系统设计.md` —— 现代 FS 目标（FAT 仅 MVP）。
- `文档/存储布局与沙箱设计.md` —— 零分区 + 四棵树 + app 树沙箱 + 树即登记簿（ADR #45）。
- `文档/驱动模型设计.md` —— 驱动=持证应用 + devmgr + 硬件资源能力化（ADR #46）。
- `文档/更新与OTA设计.md` —— 更新四层刀法 + 固件随驱动包（ADR #47）。
- `文档/架构综合纲领.md` —— **综合视角（博采众长不杂糅）**：六个参考的取/留、矛盾矩阵 10 轮的唯一 PICK、防杂糅的排除 8 问；开篇 §0 系统四层速览图（层/目录/时点 + 两条微内核铁律）。借鉴任何参考前先读。

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

- [ ] 是否查过 `参考源码/` 里对应参考项目的成熟实现？
- [ ] **借鉴是否过了「架构综合纲领.md」的排除 8 问**？（中心=可注入机制/策略分离+硬件信任边界；冲突中心→拒；找不到 G0–G5 关卡=范围泄漏）
- [ ] 这个功能属于内核还是用户态服务？（默认用户态）
- [ ] 是否把架构相关代码收敛到了 `架构相关/`？
- [ ] 是否避开了 GPL 传染？（参照 GPL 代码时只看思路，不复制实质代码）
- [ ] 改动是否能在 QEMU 上独立验证、小步提交？
- [ ] 是否需要回写 `CLAUDE.md` / `AGENTS.md`（新增架构 / 改语言边界 / 确定构建命令）？

## 构建与测试

Phase 2b 已收官（2b-0~2b-4 全落地，出口关卡 G1.5 过关）、**Phase 3-0（内核对象壳 + handle 表）/3-1（syscall ABI 骨架）/3-2（VMO/Vmar syscall 全量门控 + copy_from/to_user + 悬空 PTE 防线）/3-3（process/thread 最小对象）/3-4a（per-process handle 表 + bootstrap 迁移）/3-4b（channel 三号 + handle 随消息迁移）/**3-4c（event 三号 + 遥测环 + 空策略表等价测试——Phase 3 收官）/4-0（睡眠/唤醒原语 + thread_sleep 首个阻塞 syscall）/4-1（阻塞 object_wait_one + signal 遍历唤醒）/4-2（wait 面 ABI v0：信号位空间 + wait_one 扩 Channel + wait_many，4-2a/4-2b 双步）/4-3a（调度决策看 effective——pick 按 `prio_effective` 出选，同 eff 层内轮转跨层严格优先）/4-3b（PI 捐赠——`内核/src/pi.rs` 全图重推：donor = 睡 channel 等派生位者，受捐 = handle 表持 peer 端点带解锁 right 的睡眠者；≤8 轮不动点传播 + 冻结随行 + 零分配；ktest J 段 `PI-K ok` + boot 三任务 `PI ok` 第 18 标记）/4-3c（intent 五件套——`进程调度/src/intent.rs`：msg_intent 随消息传播 + 每消息重导 min + 三道帽（跳数 8/每跳 50ms/链累计 400ms）+ channel_create options bit0 opt-out + 遥测三计数器；ktest K 段 `INTENT-K ok` + boot 六任务 `INTENT ok` 第 19 标记——4-3 收官）/4-4（fastpath/slowpath 分层：find_single_waiter 检查链（全队无哨兵 + 恰一 READABLE 匹配者）+ fast_wake 定点唤醒（与 wake_matching 共用 wake_one_inner + PI 重推——唤醒集恒等）+ try_preempt 直切（严格更高 eff 才切，seL4 direct-switch 同款）+ ipc_fast_hits/misses/direct 三计数器；boot fa_ping(16)/fa_pong(32) bench `FAST ok` 第 20 标记 + ktest L 段 `FAST-K ok`）/4-5a（console 服务循环 + 调试槽 0xFE/0xFF 退役 + 用户打印真 IPC）/4-5b（svcmgr 拉起 echo 服务——4-5 收官）/4-6 B 半（AI 最小演示管线：tele: dump → advisor.py → bind 注入表 + clamp 执法 + rt2≤2×rt1 开销断言，AIDEMO ok 第 22 标记）+ 4-6 A 半（try_preempt 幻影出选饿死修复 + talker/echo2 两服务全链演示——DEMO ok 第 23 标记，4-6 收官）+ G-eco 入机侧第一刀（rootfs 递送通道：boot x19 抢救 + `dtb.rs` /chosen 解析 + fw_cfg 具名文件装载，`just run-rootfs`；QEMU 非 Linux ELF 直加载不走 Linux boot 协议——x0=0/RAM 无 DTB/-initrd 不加载，见 CLAUDE.md Gotcha）+ 第二刀（rootfs VMO 交付：标准 VMO 三连进 svcmgr 表第 2 插 0x0001_0002 只读（x0 已被 console 占用不走 bootstrap）、缺席 = 查表 -EBADF 降级 + `vmo_get_size`(0x1A) need=0 门控返回页对齐容量；ktest E5 段五路）+ 编译侧 G-eco-0/a/b/c（`用户态/berry-sys` raw 绑定 27 号与 ABI §8.3 对账 + abuser 模板退役 asm 换编译 flat binary——**stable 免 nightly**：builtin target + 各 bin build.rs 注入 `link-user.ld`（0x1000 基址 + `-zmax-page-size=4096`）+ `工具链/elf2flat.py` 平像（补零到页盖 .bss + `--max 4096` 单页墙）；`just build-user` 产 `内核/src/userbins/*.bin` gitignored，缺产物 = include_bytes 编译期炸响；**G-eco-b**：svcmgr asm 模板（user.rs 911–1663 行）整块退役换 `用户态/bins/svcmgr` 编译平像，echo/talker 同批换轨（**两遍构建**：pass1 产子程序平像落 `用户态/userbins/` → pass2 svcmgr include_bytes! 内嵌，`--max 65536`）+ **D3 多页镜像**——`UserProcess::build` 按长度动态布局（code ceil(len/页)页 @0x1000、data/stack 上移、新 `stack_top` 字段，帽 `USER_CODE_MAX`=64KiB；单页程序布局逐位不变零扰动）；console 发送形态实踩：asm svcmgr 本体 console 发送 = **自旋重发**非阻塞（阻塞版启动期被自旋友商抢槽，醒来须等一整圈轮转、三场竞速耗尽 10s 帽饿死 → `send_spin` asm 同形）；SVC 寄存器约定 x0=号/x1..x6=参/只回写 x0）；**G-eco-c**：`用户态/berry-loader` 纯函数 crate（cpio newc 流式迭代 + ELF64 解析，零分配/零 unsafe/host 29 测——解析器即产品 D5）+ svcmgr 消费 rootfs VMO（定值 0x0001_0002：get_size 定界 → `vmo_read` 入 `.bss` 16KiB 帽缓冲 → cpio 找 `bin/hello` → **页铺平单 VMO 装载**（真实 rust-lld PT_LOAD 不页对齐，逐段独立映射撞页；`[floor页(min), ceil页(max))` 一枚 VMO + 段字节相对偏移 write + 一次 vmar_map + 栈页其上）→ process_start(bootstrap=console 别名)——hello 凭自己表里的 handle 打 `rootfs hello`，svcmgr 打 `ECO ok`）；产物链 `工具链/mkcpio.py`（确定性 cpio，`build-user` 尾接）+ smoke rootfs 相；**连带修复内核 ABI 违约**：handle 出参 copy_out 一律 4 字节（新 `copy_out_u32` 18 处——ABI §8 冻结 handle=u32；原 `copy_out_u64` 写 8 字节砸编译 Rust 紧排邻接局部，曾致 hello sp0=0x1000 入 EL0 即自陷；asm 模板 8 间距手排槽 + ktest `as u32` 截读碰巧免疫——asm 时代的宽度违约在编译时代第一天显形）已落地**；**G-eco-d（std 面）d-0～d-3c 已落地（D4 路线①收官）**：d-0 musl 符号审计 → d-1a `用户态/berry-libc` shim（fd 0/1/2→console、静态堆、桩面三宏）+ `工具链/berry-ld`（剥 musl crt，`-lc` 惰性保险网）+ crt0 自举（自设 TPIDR_EL0，TP_ADJ=−0x10 反汇编实证）+ `用户态/bins/stdhello`（musl target + panic=abort 独立小工作区）→ d-1b berry-loader 流式几何（`cpio_step`/`elf_head`，host 51 测）→ d-1c svcmgr 三窗流式拉起 stdhello——`STD ok`/`STD err ok` 双标记；**poll 桩实踩**：std 启动 `sanitize_standard_fds` 对 poll 非 EINTR 失败直接 `libc::abort()` 静默死（bare `abort` 零 panic 文案）——poll 改真实现（revents 清零返 0）后全链通 → d-3a env 块 bootstrap 协议（argv 真值 + console/stdin 端随消息交付，`process_start` 前压队无竞速）→ d-3b stdin 面（fd 0 channel recv + 零长消息 = EOF 约定，残段 PEND 缓冲）→ **d-3c hexyl 0.17.0 上游逐字未动全链终验（D4 路线① 收官）**：stdout 走 svcmgr 私有通道对，libc `park` 先补发零长 EOF（写侧约定），svcmgr 收齐与内嵌宿主基准**逐字节比对全等**才打 `HEXYL ok`（rootfs 相 7/7；直接 grep 表行会被稳态矩阵在 192B 分片边界插花撕行——多分片输出不可为标记）；hexyl 栈溢出实踩：默认 8 页栈被 ≈64KB 用量下行扎进映像 RW 页（首击 `__aarch64_have_lse_atomics` → LSE 误判 `casa` 在 cortex-a72 UNDEFINED）——栈页数按程序面给（32 页 = musl 128KiB 默认）+ 映像/栈间保护页（栈穿 = 干净 D-abort）。构建基线：stable Rust + `aarch64-unknown-none`，`just run` 在 QEMU `virt` 串口依次打印 `BerryOS boot` → `KVA ok`（TTBR1 高半别名自校验，2b-2 起只走高半）→ `ROOTFS ok src=… / ROOTFS absent`（G-eco 入机侧：fw_cfg 具名文件/DTB `/chosen` 双源装载，无 rootfs 正常降级）→ `ROOTFS vmo=… / ROOTFS vmo=none`（G-eco 第二刀：VMO 只读交付 svcmgr 表第 2 插 0x0001_0002）→ `GIC ok / FRAME ok / HEAP ok / SYSCALL ok`（3-2 起含 VMO/Vmar 九号全链；3-3 起加 process/thread F 段：双出参/参数面/门控/归属面/钉住释放链；3-4a 起加 G 段：两表隔离/take 迁移作废/insert_ref 落新表/子进程出生空表；3-4b 起加 H 段：channel 双端/空 -EAGAIN/往返/参数上限/满 -EBUSY/RO 拒/handle 随消息迁移/send 原子性；3-4c 起加 I 段：event create options 拒/poll 未命中 -EAGAIN/deadline=1 远古时刻 -ETIMEDOUT 不睡（4-1 改板）/signal 置清位/全满足集回写/dup 无 SIGNAL -EACCES 但 wait 照常/类型野值 -EBADF；4-3c 起加 K 段：intent 纯函数面十用例（编码域/快照三分支/结算五路/options 面））→ `PING TASK ok`（switch asm 乒乓）→ 低半读期望故障恢复 → `ASPACE ok`（两张用户页表同 VA 隔离 + asid=16b）→ `VMO ok`（VMO/Vmar 六操作 + 两次期望故障）→ `HANDLE ok`（3-0 handle 表五操作 + 槽位重用代号 +1）→ `POLICY ok`（3-4c 空策略表等价 + 4-3a 真消费：未绑/绑空表逐项全等 + 对照表 eff=32 者独占出选——严格优先，旧「RR 不看优先级」护栏随 4-3a 撤）→ `ktest done` + brk 可恢复 dump → **两忙循环任务 `A`/`B` 与 EL0 用户进程 `user_good`（打 'u'）/`user_abuser`（打 'S'，坏 handle 滥用 → -EBADF 存活）/`user_spawner`（打 's'，**3-3 验收：经真 syscall 创建第二个进程；3-4a 起 bootstrap 迁移后 close 原值必 -EBADF**）及其子进程 `user_child`（打 'C'，**3-4b 验收：凭自己表里的 channel 端点 recv 出父进程 send 的 PING+礼物 handle、vmo_read 魔数比对——handle 随消息跨表迁移闭环；稳态收 PING 回 REPLY 与 spawner 交换消息**）在 CNTV timer slice 下交替（A/B 每次打印先读私有页断言隔离）**，周期打 `switches=N` 金丝雀；user_good 入口 x0 = root Vmar handle（首能力）先跑一次性验收链 `vmo create→commit→write→map→经 VA 读写→别名回读→close 拒(-EBUSY)/RO 写拒(-EACCES)→别名 close→unmap→decommit→close` 全绿才进稳态循环；spawner 链 `process_create`(双出参)→`vmo_create/commit/write`(内嵌子代码)→`vmar_map` 进子空间→建礼物 VMO→`channel_create` 建对→`send(PING+礼物 handle)`（**3-4b：handle 随消息迁出本表**）→`thread_create`→`process_start(bootstrap=end1)`（端点迁子表，child 凭它收发）；EL0 syscall 经 `Task::proc_tag` 派发到所属进程自己的 handle 表（take/insert_ref 迁移原语，refs 不变）；恶意用户进程 rogue_ptr（野指针）dump + 被杀（**只死自己**，watchdog 断言杀数==1 且死者是 rogue_ptr）+ 3-4c 遥测环存活断言（`telemetry::total() ≥ switches()` + 环内 vct 单调）+ 唤醒环断言（`BLOCK_BOOT_DONE` 门槛后：wake_total≥2 / 延迟非负 / vct 单调 / 环未淘汰期双醒因覆盖）打 `G1.5 ok`；**4-0 验收：sleeper/waker 对打**——sleeper 超时自醒（'T'，断言 Timeout+时长）+ 条件唤醒（'K'，挂 SLEEP_OBJ 被 waker 轮询 `wake_matching` 摘醒断言 Signaled）双绿打 `BLOCK ok`，此后两者 exit 退出、稳态矩阵无扰；**4-1 验收：user_waiter（打 'W'）三路 + waker2 对打**——waiter 超时（-ETIMEDOUT + observed==0 + 真睡到点）/poll 命中（恰置 bit5 → 请求超集 → observed==0x20 全满足集）/真睡（10s deadline）被 waker2 真 SVC signal 唤醒（observed==0x8）三路全绿才进稳态打 'W'；waker2 内核任务经真 SVC 链（clock_read→thread_sleep 3 slice→signal 内核侧表 handle→再睡 1 slice）打 `WAIT ok`，此后 exit；同一 Event 双表挂同一 ObjRef（bootstrap 表 + waiter 表）——signal/wait 各持己表 handle 的跨表互通活证；**4-2a 验收：user_pinger（打 'p'）/user_ponger（打 'P'）对打**——boot 手搭 channel 对（两表各 insert 一端 + pair 互钉，pinger 持 endA SEND / ponger 持 endB RECV）；pinger send 灌到 -EBUSY → `wait_one(WRITABLE, now+10s)` 阻塞 → ponger recv 腾位唤醒 → 补发成功 → 首圈打 `CHAIN ok`（smoke 第 16 标记）后稳态每圈 'p'；ponger recv/-EAGAIN → `wait_one(READABLE)` 阻塞 → pinger send 唤醒 → 验 PING 载荷打 'P'——「send 满 → 对端 recv 腾位唤醒」两进程阻塞链活证（ktest 同步加 I6 低半拒面 + I7 Channel poll 面全套；Event 位面/I 段/waiter/waker2 全量迁高 32 位用户半）；**4-2b 验收：ponger 稳态等待改 wait_many n=2**——[endB:READABLE, boot 直插哑 Event:1<<32 永不 signal]，命中须 `*idx==0`（哑 Event 位永不置、idx==1 即断链自陷），首次打 `MANY ok`（smoke 第 17 标记；哑 Event handle 0x0001_0002 boot 断言定值——ponger 模板硬编码依赖）；ktest I8 盖错误面+poll 面（n 界/坏 handle/poll -EAGAIN 且 idx 留哨兵/远古 deadline -ETIMEDOUT 不睡/命中 idx 两向），B 段未实现样本 0x05→0x33；**4-3b 验收：pi_client(32)/pi_server(0)/pi_probe(0) 三内核任务对打（全真 SVC）**——client 灌满 send 后 wait WRITABLE 睡（donor）、server 睡 Event 时 block 钩子重推成边受捐 32、probe 睡 5 slice 醒来断言捐赠字段（server pi==32 且 eff==32）后 signal 放行打 `PI ok`（smoke 第 18 标记），server 醒（冻结 32 随行）recv×8 腾位唤醒 client、client 补发自证后双双 exit；ktest J 段 `PI-K ok` 八用例（单跳/rights 门控/链式两跳/环收敛/轮帽截断/哨兵跳过/冻结回落/共享表粒度）；4-0 sleeper 信号位连带收窄（合成对象只等用户位 1<<33——带派生位睡非对象会被 PI 图解引用炸对齐）；**4-3c 验收：it_driver/mid/sinkA/sinkB/slow/probe 六内核任务对打（全真 SVC）**——借入生效（sinkA eff==0x02 且 hop==1 = mid 借入转发活证）/opt-out 回落/每跳预算帽（slow 借入后睡 200ms → send 降级装自己的 declared、借入字段保持）/双帽拒传播（boot 伪造消息直压收端队列：hop=8 恰顶、acc=400ms+1 恰超），probe 遍历 SLEEPQ 断言四睡眠者字段 + 三计数器打 `INTENT ok`；三条 `intent: … cap` trace 行；`channel_create` 改 options 首参（ABI §8 修正）；**4-4（fastpath/slowpath 分层——`find_single_waiter` 检查链 + `fast_wake` 定点唤醒 + `try_preempt` 直切 + telemetry 三计数器）**：fa_ping(16)/fa_pong(32) bench 对打（专用通道双端各 SEND|RECV）——稳态每轮 ping send 检查链全过 → fastpath + 直切（32>16）、pong 回件时 ping Ready 未睡 → 慢路 miss；ping 收尾断言 hits/direct ≥128、miss≥1 + CNTVCT 计量 RT 打 `FAST ok`（TCG 量级 ≈28.4k 参考 cycle，真机 <1200/<500 线留 G 卡复测）；ktest L 段（`FAST-K ok`）盖 find_single_waiter 纯函数面六用例 + SVC 慢路回归；**4-5a 验收：console 内核服务常驻**——稳态矩阵 A/B/u/s/S/C/p/P 全员打印经 channel_send(endU) → console 排水落串口，watchdog 用户活证改 CONSOLE_DRAINED 服务侧计数（console 挂 spawn_with_tag 私有表 + 稳态等待 wait_many——PI 双保险），ktest A 段 0xFE/0xFF 断言 -95 退役面；MANY ok 标记改 ponger 入口先发 wait_many 定确定性（稳态 -EAGAIN 是排水/补灌竞速——打印走 console 后实测 120s 队列恒非空的 run 存在，标记不赌竞速）；`just run-el2` 同（EL2→EL1 eret 降级路径）；**4-5 前置补丁已落地**：`spawn_with_tag` + `caller_table` EL1 分支 tag≠0 走私有 handle 表（修 4-3b 共享表粒度的错捐级联——常驻内核服务须挂 `Box::leak` 私有表，tag==0 的 ktest/boot/短命 bench 零扰动；自证 svc_tag 双断言打 `TAGTAB ok`，非 smoke 标记）；**4-5b 验收：user_svcmgr（打 'v'）经真 syscall 全链拉起 echo 服务**——process_create（双出参）→ vmo_create/commit/write（内嵌 echo 镜像：4-5b 时为 asm 模板 268B，G-eco-b 起换编译平像 `用户态/bins/echo`）→ vmar_map@0x1000 → channel_create → thread_create → process_start(entry, sp, bootstrap=end1)（close 原值必 -EBADF 迁移自证）；svcmgr send ECHO_REQ → wait_one(end0, READABLE) → recv 验低 32 位回显打 `SVC ok`（smoke 第 21 标记）后稳态节流循环打 'v'；echo 子进程静默跑最小服务循环（recv → 原字节回发，-EAGAIN→wait READABLE 重收）；svcmgr/echo 均 EL0 每进程私有表 + prio 0——PI 面零新增。**4-6 A 半验收：talker/echo2 两用户态服务互联**——svcmgr `SVC ok` 后直线拉起 talker（4-6 时为 936B asm 模板，G-eco-b 起编译平像）+ echo2（复用 echo 镜像），交接消息一次迁移双 handle（console 别名 + t2 对端），talker 凭收到的能力 echo2 round-trip DEMO_REQ 验回显打 `DEMO ok`（smoke 第 23 标记——B 半 AIDEMO ok 已占 22，尾插随合并序）后稳态 'd'；幻影出选修复后全员出选:切入 1:1（修复前 echo1 29:1 冻结 + svcmgr 超时死）；**G-eco-b 起 svcmgr 本体亦为编译平像（首个多页用户镜像，D3）**；MMU 开（高半 TTBR1 内核表 `KERNEL_VA_BASE`，**2b-1 起内核主体高半执行**、异常 dump PC 应为 `0xffffff80…`；**2b-2 起低半身份表退役**——低半访问必 fault；GIC/UART MMIO 全走高半别名；**2b-3 起 VMO/Vmar 为正式映射路径**（map 即填用户页表）；smoke 固化程序头断言 `size.py --assert-flip`）、buddy/slab 堆、异常向量（current-EL + lower-EL 双路）+ panic dump、GICv2、SVC syscall（EL1/EL0 双入口 + 语义层注册 + copy_from/to_user 界检，27 号实现——0x04 阻塞化 wait（4-2a 扩 Event|Channel、Plan B 扩 Process）/0x05 wait_many 随 4-2b/0x06 signal+唤醒+位空间检、0x37 thread_sleep 随 4-0、0x1A vmo_get_size 随 G-eco 第二刀、0x33/0x36 真退出随 Plan B v0.5（统一退出钩子 + TERMINATED wait 位 + REAPQ 回收）；0xFE/0xFF 调试槽 4-5a 退役（-ENOTSUP）；未知号 -ENOTSUP 永不杀）、内核线程 + 上下文切换 + tickless idle + IRQ 抢占（含 EL0 运行中抢占）+ 切换遥测环（四切换位 yield/preempt/exit/block 写静态预储环，零分配）+ 唤醒遥测环（4-0 配套：双唤醒位 Signaled/Timeout 写 `WAKE_RING`，block→wake 延迟数据源，零分配）+ 睡眠/唤醒原语（4-0：SLEEPQ 单队列按 deadline 升序 + block_current/wake_matching + tick 扫超时 + thread_sleep 0x37；4-1：wait_one 阻塞化（poll-first + 醒来重验 + -ETIMEDOUT）+ signal 遍历唤醒 + 睡眠钉住 ObjRef::retain/release；4-2a：Channel 唤醒点——send 成功唤对端 READABLE / recv 成功唤对端 WRITABLE；4-2b：WAIT_ANY 哨兵——多对象等待者任何唤醒都命中、醒来重验）全通。smoke 25 标记（`POLICY ok` 第 11 / `BLOCK ok` 第 14 / `WAIT ok` 第 15 / `CHAIN ok` 第 16 / `MANY ok` 第 17 / `PI ok` 第 18 / `INTENT ok` 第 19 / `FAST ok` 第 20 / `SVC ok` 第 21 / `AIDEMO ok` 第 22 / `DEMO ok` 第 23 / `ECO ok` 第 24——rootfs 相尾插 / `RX ok` 第 25——P5 A-1 RX 交互相）+ switches 金丝雀严格递增；**`just smoke` 双相（G-eco-c 起）、三相（P5 A-1 起）、四跑（P6 A-6 起）**——裸相 23 标记零漂移 + rootfs 相二跑 `-fw_cfg` 断言 `ROOTFS ok src=` / `ROOTFS vmo=` / `rootfs hello` / `ECO ok`（G-eco-d d-1c 起加 `STD ok`/`STD err ok`、d-3c 起加 `HEXYL ok`、P5 A-2 起加 `BSH ok`、Plan B v0.5 起加 `EXIT ok`、P5 v0.5 起加 `PIPE ok`、P6 A-1 起加 `FS ok n=`、P6 A-2 起加 `VIRTIO ok`、P6 A-3 起加 `VBLK ok`、P6 A-4 起加 `BLKD ok n=`、P6 A-5 起加 `FAT ok n=`、P6 A-6 起加 `FATW ok n=`、P6 A-7 起加 `NET ok ping=` 共 17 标记）+ RX 相三拉（FIFO stdin + holder 防 EOF，40 标记（裸 23 + rootfs 17）到齐后逐字节 0.5s 注入 `rx\n`——PL011 FEN=0 深 1 字节消竞速 → 断言 console 收齐行 token 才打的 `RX ok` + 注入后金丝雀复验严格递增——RX 破坏 IRQ 契约死状 = 冻而不死）。**P6 A-6 起四跑**：注入序列扩四段（+`run fail` → `echo f$?t > OUT.TXT` → `cat OUT.TXT` 断言 f42t——`$?` 展开造内容 = 消回声歧义）+ **FAT 持久化相**（第四跑不重造盘：40 标记脏盘照跑[FATW 覆写幂等] + cat OUT.TXT 跨 boot 读回——fat:rw 关卡；全 -drive 线 cache.writeback=off，DEADLINE_ROOTFS=240s[TCG 单次串行扇区 RPC ~0.8s，FATW commit 13 次 ≈ +10-40s]）。**Plan B v0.5（退出路径）已落地（2026-10）**：0x33 `process_exit`/0x36 `thread_exit` 真实现（`-> !` 不返）+ **统一退出钩子** `task_exit_hook`（fault 杀同钩码 0；序 = mark_exited → wake TERMINATED → drain 子表 → release 钉住——顺序即正确性；pinned=0 直通绝不 drain 共享表）+ Process 派生 `SIGNAL_TERMINATED`(1<<2) 可等待（单调永不清）+ **REAPQ**（Task Box/内核栈静态环，reap_drain 挂 idle/yield）+ berry-exec `ChildHandles::reap`（wait TERMINATED → unmap×2 → close×5，映像/栈帧真还；root Vmar L1/PT+ASID v1 恒泄漏）+ berry-libc exit 族 **EOF 先行**再 0x33（std lang_start 真死路径；abort=134）+ bsh `exit` 内建/`run` 后 reap + svcmgr stdhello arm reap 成功打 `EXIT ok`（rootfs 相第 9 标记，确定性状态谓词）+ ktest B 段样本 0x33→0x35/F4b Process wait 面 + `工具链/abi_check.py` 三方镜像断言挂 smoke（IDL 单源生成前临时关卡）。**P5 A-1（内核 RX 中断）已落地（2026-10）**：`架构相关/aarch64/src/uart.rs`（TX 迁入 + RX 面：FR.RXFE/DR + 静态环 256B 零分配 + `set_rx_notify` 通知槽）+ gic `IRQ_PL011_RX=33`（zCore 同板 + QEMU DTB LEVEL_HIGH 双活证、首试实测点亮）+ `enable_spi`（SPI 段 ITARGETSR 复位 0 须真写 target）+ `exception_irq` 按 INTID 分流（CNTV→rearm+EOIR+tick 槽，tick 槽只对 CNTV；PL011 RX→drain 到 RXFE 在 EOIR 前（电平去活铁序）+EOIR；未知源首 4 loud `irq? id=` 后静默）+ console wait_many 扩第三项 RX Event（私有表第 3 插 0x0001_0003、位 1<<39；IRQ 侧置位 + `wake_matching` 不抢占）+ console 每醒无条件消费（disable_irq 包络内 `rx_pop_all` 搬空 + 清位——「位 0 ⟺ 环空」不变量闭环）+ 直写 UART echo + 行 token `rx` → `RX ok n=… lost=…`。**P5 A-2（bsh 交互 shell）已落地（2026-10）**：`用户态/bins/bsh`（musl + berry-libc shim，stdhello/hexyl 同链经 rootfs 流式装载）自驱自验——启动即内部 `run_capture("hexyl")`（256B + `--color=never` + 私有 stdout + 零长 EOF 收齐）字节数==2156 才自打 `BSH ok`（rootfs 相第 8 标记，不赌交互输入）；prompt/mini-readline/内建 help/echo/run 为化妆面（fd0 = console 键盘行转发端 endB2，经 gift take 中转进 bsh 表）。`用户态/berry-exec` 抽取（spawn_rootfs_elf/channel_pair 迁出 svcmgr + SpawnSpec 化——bsh `run` 与 svcmgr 拉起同链，「不长第二套」）。**恒满足项回归实踩（A-2 最大收获）**：console wait_many 第 4 项 [endK2:WRITABLE] 无键盘输入时恒满足 → console 永不入睡 → drain 相位钉死 RR 环 → 填槽序末位的最后残存自旋者 svcmgr send_spin 结构性饿死（SVC ok 永缺 + echo 连带死，裸跑即复现）；修法两刀 = 恒满足项条件化入列（plen>0 才 4 项）+ svcmgr 自旋豁免撤销（send_spin 全量→send_retry 阻塞，G-eco-b boot 竞速前提已随 82fe305 消失）。smoke 三相全绿 + EL2 同绿 + host 11+51。 对抗审查加固（4 confirmed）：berry-exec 栈顶前置（贴顶映像+栈越 USER_TOP 前置 Err 降级，防 vmar_map 栈页自陷杀 svcmgr）+ bsh arm 一次性标志（cpio 重复名防已 take handle 死值自陷）+ Err 路径 rootfs_dup 回收。 **P5 v0.5 管道 `|`（PIPE ok = rootfs 相第 10 标记）**：bsh `A | B` 相邻段 channel_pair 当 pipe + EOF 零长消息级联 + 先 collect 后 reap 死锁序；feeder 192B 分片 send_retry（EBUSY 阻塞重臂）；PIPE ok = 三跑对拍逐字节全等（64B 输入）。对抗审查（20 agents，7 confirmed/9 refuted）两修：process_create -ENOMEM 降级 Err（PROCESS_MAX 出帽不再 trap 反杀 shell——僵尸累积链断根，不退出段冻结+僵尸文档化随 v2 kill）+ 自验输入 256B→64B（参照腿 659B 结构性不进 feeder 互锁分支——门限 L≥2689B 文档化于 berry-exec feeder 注）。 **P5 v0（shell v0 收官）已落地（2026-10）**：console mini-readline（HIST 8×96B newest-first 环 + ESC 状态机 A/B 翻历史/Ctrl-U/退格；任何 Enter（含空行）回 live 游标；Tab 补全 defer v2）+ syscall 0x3A `process_get_exit_code`（**ABI 单源管线首次真实扩号 28→29**：need=0 仅类型门控、未退 -EAGAIN（先 wait TERMINATED 再查）、i32 出参 4B；`ChildHandles::reap` 改 `Result<i32>`——bsh `&&`/`$?` 真码源）+ bsh 环境变量（vars + `$NAME`/`$?` 展开 + `env` 内建，子进程 env 传递 defer v2）+ `&&` 短路 + 状态码 0/2/126/127/子真码（rootfs bin `fail` exit 42 = 确定性非零源；**操作符识别（&&/|/赋值形）一律在展开前原始文本上**——POSIX 分词先于展开，展开结果只是数据）+ svcmgr `EXIT ok` 强化（reap Ok(0) 才打）+ smoke RX 相升 rootfs+FIFO 合相 33 标记 + 编辑注入三段（`rx\n` → RX ok；`helll`+DEL+`o` → 退格编辑验；ESC[A 召回+`!` → 历史召回验）。对抗审查（14 agents 四维，9 confirmed/1 refuted）修：hist 空行 Enter 回 live、ESC unclosed CSI 吞键（三分域非 final 复位落下）、bsh 展开后重解析操作符（`P=a|b` 后 `echo $P` 误切管道/`$P` 变量值当语法）+ `runfoo` 词边界、DEMO ok ⊂ AIDEMO ok 子串混叠（smoke `assert_marker` 负向断言）、ABI 文档 §8.3/§8.4 镜像补 vmo_get_size/get_exit_code 两原型（原型层无机器守卫的落后账清偿）。 **P6 A-1（RAM-fs 最小面）已落地**：`berry-fsproto` 协议纯函数 crate（单对双向 channel 严格串行 RPC：PUT 分片/TRUNC 定长/GET more 链/LIST 名列，host 18 测）+ `bins/fsmgr` RAM-fs 服务（svcmgr 预拉起，fs_b 经 env 块 gift[2]、bsh 收 fs_a dup gift[3]——svcmgr 关 fs_a 后 bsh 唯一读者）+ bsh `>` 重定向/cat/ls 内建（操作符识别在展开前；诊断打真 stdout 不随 > 进文件）+ `FS ok n=` rootfs 相第 11 标记（PUT/GET 2156B 全等自验）。对抗审查（15 agents，5 confirmed 全修/6 refuted）：标记子串混叠（FS ok ⊂ ROOTFS ok src=——改带参形态 `FS ok n=`）/编码器空 out panic（Option 化）/fs_rpc 收侧超时会话毒化（无请求序号防迟到应答错配 = 静默错数据——FS_SESSION_OK 收侧超时作废、发送侧不毒化）/测试盲区四补一强（14→18 测）/`ls ` 带参 usage-2。 **P6 A-2（用户态驱动地基第一刀）已落地**：`Vmo::create_mmio(pa,size)` boot-only 构造（无新 syscall；pages 恒空 Drop 零动作、内容面四拒 NotSupported、map attr=Device-nGnRnE；size 页对齐执法——静默取整 = 漏授相邻设备 PA）包 virtio-mmio 整窗 `[0x0A00_0000,+0x4000)`（32 槽×0x200）授 svcmgr 表第 4 插 0x0001_0004（`VIRTIO vmo=` 行）+ `SpawnSpec.mmio_vmo` 映固定 VA 0x7000_0000 + `bins/vprobe` 扫全 32 槽（空槽也应答规范 ID 头）：全槽验 magic + 恰一占用槽 ver==2/did==2 才打 `VIRTIO ok`（rootfs 相第 12 标记）。QEMU 需 `-global virtio-mmio.force-legacy=false` + virtio-blk-device（justfile/smoke 已挂、disk.img 现场造 gitignored）。实踩两条：QEMU virt 板倒序建 32 条总线、首 `-device` 落最高槽 31（绑 transport[0] = 空槽 ver=1/did=0——绑死槽号 = 把总线排布焊进标记，扫窗定位）；build-user 无 `set -e` 静默吞 bsh E0063、mkcpio 打陈旧产物「全绿」（D6 内核侧 include_bytes 守卫护不到用户态中转链——已补 set -e）。对抗审查（9 agents，3 confirmed 全修/2 refuted）：medium MMIO 固定 VA Overlap 是 rootfs 输入面可达 trap（前置 [base,stack_end)×窗口相交即 Err 降级，栈顶前置同族）；size 页对齐执法 + ktest 断言；smoke.sh「四跑」失实措辞。refuted：bsh `run vprobe` 无窗口「静默死」实为三通道 loud（dump+kill 行+no EOF 诊断）。 **P6 A-3（virtio-blk 真块设备——真设备 IO 第一刀）已落地**：syscall 0x1B `vmo_get_phys`（ABI 单源第二次扩号 30 在册/29 实现；need=0 仅类型门控、匿名页须已 commit（-EBADSTATE）、MMIO 窗口恒可查、字节精确；ktest E7 六路）+ `用户态/berry-virtio` 纯函数 crate（RingLayout/desc 编解码/blk 常量，host 13 测）+ `bins/vblk` 驱动全链（32 槽定位 → modern v2 协商（只收 VERSION_1 + FEATURES_OK 回读验）→ qsize=16 建队（三址 LOW/HIGH + Q_READY 回读）→ **DMA 双页 VMO 从不进任何地址空间**（构造/收割全走 vmo_write/vmo_read + get_phys 查后备 PA——纯 VMO 机制零新对象变体）→ DRIVER_OK → LBA0 印章 `BERRYOSVBLK!` 对拍 + LBA1 写/读回对拍 → `VBLK ok`（rootfs 相第 13 标记，RX 相 36）→ exit + svcmgr reap；poll used idx 10s 帽（used_off+2——flags 位恒 0 是排障实踩）；盘容断言 2048 扇区）。联调抓三 bug：**TPIDR_EL0 每任务保存**（TaskContext +tpidr@104 + switch asm 存/取对——抢占后拿别人 TLS 裸奔，vblk 首次 TLS 访问夹在数秒设备 IO 后才显形）/ **echo 常驻服务空闲超时重臂**（-ETIMEDOUT 重臂续等、链断判定交发问方 roundtrip）/ **used ring poll 偏移**（flags≠idx——诊断同源原则：诊断与被测必须同 API，差一偏移的两次「同一读」自造假象）。对抗审查（workflow 四维 × 逐 finding 对抗核实，5 confirmed 去重 2 项全修/1 refuted——ABI §8.3 注记 errno 对齐实现 -EOVERANGE/-EBADSTATE + vblk 门铃前 `dmb ish` 发布屏障与 STATUS 复位回读等待；refuted = 退出不复位设备无触发路径——见 CLAUDE.md）。验收 = smoke 三相全绿（裸 23 + rootfs 13 + RX 36）+ EL2 同绿 + host 93。 **P6 A-4（vblk 服务化）已落地**：`berry-blkproto` 纯函数协议 crate（严格串行 RPC：请求 9B op+sector、应答 1B st，扇区 512 定值，host 13 测）+ vblk 自验毕转常驻服务（channel 端随 env 块 gift[2] 迁入，fsmgr 同槽；服务纪律同款：空队重臂/client 错 ST_BAD 不死/设备面 trap loud）+ **数据面 = VMO 随行迁移**（512B > inline 帽 256——dup 别名随请求 send、driver bounce-buffer 后 close，本体留 client 表）+ svcmgr 消费自验（写 LBA5 pattern → OP_READ 前刷 VMO 本体反值 0xA5 → 读回对拍 → `BLKD ok n=512` rootfs 相第 14 标记；常驻子不 reap（reap 首步 wait TERMINATED 帽必超时返 Err——不 reap = 免 30s 白等）；recv 收口容量 4 收协议值 1——容量不足 -EINVAL 会误入 trap 杀服务；随行 handle 回收撞 -EBUSY（client 送已映射唯一 handle）分流 ST_BAD 勿 trap；应答帽 30s 超时不重发；单设备单主人 v1 文档化——negotiate STATUS=0 复位掀翻既有队列，第二个持窗实例杀服务（v1 唯一持窗路径 = svcmgr 单遍拉起，bsh run vblk 无窗死在 find_device）。对抗审查 workflow 12 agents：8 confirmed 全修（EBUSY 分流/0xA5 防线错位两 medium + 恰满容量与线钉板两测试/SECTOR_SZ const assert/注释语义两修/文档锚）。验收 = smoke 三相全绿（裸 23 + rootfs 14 + RX 37）+ EL2 同绿 + host 106。 **P6 A-5（FAT 只读）已落地**：`工具链/mkfat.py` 确定性 FAT12 造盘器（2048 扇区 = 16 扇区 vblk 保留区 + 卷 @LBA16；BPB hidden=卷前缀折算源；HELLO.TXT + PATTERN.BIN 5000B 簇链；`--fixture` 产 host 共识基准 + `--check-fixture` 守卫挂 test-host 前置）+ `用户态/berry-fatfs` 纯函数 crate（「解析器即产品」第 4 例，host 11 测含四共识：BPB 合法域/FAT12 紧包/目录项/8.3 名，扇区号卷相对）+ fsmgr `fat.rs` 覆盖层（eager init 三验定位卷 + 簇界分片 GET + 单簇缓存；RPC 三态——收侧超时 = 会话毒化 ST_BAD 降级；overlay：RAM=写通道/FAT=只读底座，PUT/TRUNC 命中 FAT 名先物化，LIST 双层并置；名字空间按构造隔离——name_to_fat11 只收 [A-Z0-9]）+ svcmgr 预拉序：vblk 先于 fsmgr（BLKD 门后 blk_a 经 gift[3] 交付，fsmgr = vblk 唯一 client）+ bsh 自驱自验（GET 双文件对拍 → `FAT ok n=5000` rootfs 相第 15 标记，确定性状态谓词）。对抗审查 workflow 10 agents：4 confirmed 全修（fixture 重产守卫三件套/justfile A-2 注释过时/smoke mkfat `|| fail` 守卫/文档计数锚）、2 refuted 留档（mkfat 8.3 域校验 v1 不可达/name_to_fat11 尾点别名激活前提不成立）。验收 = smoke 三相全绿（裸 23 + rootfs 15 + RX 38）+ EL2 同绿 + host 117。 **P6 A-6（FAT 读写 fat:rw——开发体验第三档）已落地**：berry-fatfs 写基元（fat12_set 12 位紧包双项读改写/find_free_cluster 升序/encode_file_entry 8.3，host 11→14 测）+ fsmgr staging+commit 两段写（PUT stage_put 不碰盘、TRUNC 触发 commit 八步序：旧链 collect → alloc-before-free 新链 → 数据扇区簇尾零填 → 旧链释放 → 双 FAT 镜像 → 根目录两遍 → **根扇区写盘+读回对拍** → install；任一步失败内存态不变）+ rpc_write（vmo_write 载荷先行 → dup R|W 别名随行 → ST_OK；收侧超时 Poison → sess_ok=false 永久降级）+ bsh `>` 8.3 大写名真落盘（RAM 物化退役，小写名仍 RAM）+ FATW 自验 3000B 跨 2 簇写读对拍（rootfs 相第 16 标记）+ **FS_COMMIT_CAP_NS=90s**（fs 写请求分片 PUT+TRUNC 收侧帽——TCG 串行扇区 RPC ~0.8s×13 次，10s 帽结构性误杀：bsh 放弃打 fs unreachable 时 fsmgr 仍在走完整 commit，**服务慢 ≠ 服务死**）+ smoke 四跑（RX 段 d：run fail → echo f$?t > OUT.TXT → cat 断言 f42t[$? 展开造内容消回声歧义]；FAT 持久化相第四跑不重造盘 + cat OUT.TXT 跨 boot 读回；cache.writeback=off 全线）。**对抗审查（workflow 8 agents 四维，3 confirmed 全修/1 refuted）**：stage_trunc_fresh 补盘上 size 帽（A-5 materialize 原有、换轨丢失的回归——野值 abort 杀常驻服务）+ fs_put 分片 PUT 同挂 commit 帽（跨名 PUT 隐式重跑旧 staging 槽整链 commit 面）+ run-rootfs 注释改正（mkfat 每次重造 = 手动跨 boot 验持久化结构性不可达）。验收 = smoke 四跑全绿（裸 23 + rootfs 16 + RX 39 含 f42t + 持久化相跨 boot）+ EL2 同绿 + host 120。 **P6 A-7（virtio-net 网络面第一刀）已落地**：`用户态/berry-netproto` 纯函数crate（「解析器即产品」第 5 例，host 11 测：eth/ARP/IPv4/ICMP-echo 编解码 + RFC1071 校验和 + **padding 容忍**——长度权威在报文域不在线宽）+ `用户态/bins/vnet` 用户态网络驱动（musl+shim、MMIO 窗口同 vprobe/vblk；按类扫槽认领唯一 net → v2 协商（MAC+MRG_RXBUF——**QEMU VERSION_1 按 12B 剥 vnet_hdr，不协商 = TX 斩头 SLIRP 静默丢**）→ 双队列 + 16 rx 单描述符整收 → ARP 解网关 10.0.2.2 MAC → ICMP echo 32B 往返对拍 → `NET ok ping=32 slot=30` → **exit 前 STATUS=0 复位撤队列** + reap）；QEMU 三处 + run-rootfs 挂 `-netdev user`；vprobe/vblk find_device 同改按设备类计数（block 恰一 + net 合法同席）。对抗审查（workflow 12 agents 四维，4 confirmed 全修/2 refuted 留档）：medium = **退出不复位则 SLIRP 周期 IPv6 RA（实证 ~200-600s 不请自来）令仍 DRIVER_OK 的设备 DMA 写已还帧页 = 静默腐化**（vblk A-3 refute 前提=被动设备无外部写者，不跨设备类转移）；low = tx_send copy_from_slice 自重叠 UB（帧在位不搬）、头注 padding 陈旧口径、文档计数锚；refuted = ttl=0 不对称（消费方硬编 64）、组合 padded 测试缺位（已有钉板 + 校验和数学免疫）。验收 = smoke 四跑全绿（裸 23 + rootfs 17 含 NET ok + RX 40 + 持久化相）+ EL2 同绿 + host …+11=131。

**前置（macOS，一次性）**：`brew install qemu just` + `rustup target add aarch64-unknown-none`（stable，无需 nightly/build-std）。

**日常命令**（仓库根）：

```bash
just build          # = cargo build（内核 ELF）；先自动跑 build-user 补模板产物
just build-user     # G-eco：用户程序编译 → flat binary（内核/src/userbins/*.bin，gitignored）+ mkcpio.py 产 rootfs.cpio
just run            # QEMU virt → 1BerryOS boot（默认起始 EL1），halt
just run-el2        # virt,virtualization=on → 21BerryOS boot（EL2→EL1 eret 降级路径）
just smoke          # 无人值守冒烟（CI 同款）：boot 标记断言 + switches 金丝雀严格递增；三相（裸 + rootfs 15 标记 + RX 交互——P5 A-1 起）
just abi-check      # ABI 单源守卫：gen_abi.py --check 逐字节比对 abi.toml 三面产物（忘再生成/手改产物即 FAIL）
just abi-gen        # ABI 再生：abi.toml → 内核/berry-sys abi_gen.rs + 文档 marker 块（改 ABI = 改源 → abi-gen → diff 入册）
just size           # 内核体积基线报告（文件量 / PT_LOAD 镜像与静态内存 / 段明细；CI 同步打印）
just run-rootfs     # G-eco：带 rootfs 启动（-fw_cfg name=opt/berry/rootfs,file=rootfs.cpio）——svcmgr 消费链 `rootfs hello` + `ECO ok`（裸跑降级 ROOTFS absent/vmo=none）
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
- 不要让 libc 桩在 std 启动关键路径上返 ENOSYS：`sanitize_standard_fds` 对 poll 非 EINTR 失败直接 `libc::abort()` 静默死（G-eco-d 实踩——bare `abort`、零文案）；能谎报成功就谎报（fd 恒有效语义），或改真实现。
- 不要在 syscall handler 里解蔽 IRQ 或忙等跨任务：SVC 入口硬件置 DAIF → syscall 主体不可抢占是 v1 免锁根基（3-4a 已拆 per-process 表，跨表访问仍全靠它串行）。
- 不要把「映射中 VMO 拒 close」写成一律拒：只拒**销毁性 close**（refs==1 且 mapcnt>0 → -EBUSY），别名 close（refs≥2）放行——对象不死，存活句柄钉住它。
- 不要在内核对象间存裸指针跨界引用（Thread→Process、Process→root Vmar 等）：必须 `ObjRef::retain/release` 钉住（3-3 起模式，见 `内核/src/process.rs`）——否则持有者关光 handle 即 use-after-free。
- 不要假设 QEMU 启动在固定 EL：默认 `-machine virt`+`-kernel` ELF 起始 EL1，`virtualization=on` 起始 EL2——boot 第一行 `mrs CurrentEL` 检测并按需 eret 降级到 EL1。
- 长命睡眠内核任务不睡共享表（4-5 纪律；4-6 新形态：被错捐 eff 32 的睡眠者自己抢片，把断言对象推过自身重试预算——验收司机/probe 也挂私有表，重试预算按「最坏轮转+自己偷片」放宽；halt 后「全系统冻结」先查 panic 尾巴）。
- `for_each_sleeper` 须 IRQ 屏蔽窗内调用（tick 摘队动睡眠链接——任务体直调须手动 disable/enable 包络）。
- 不要复制 GPL/AGPL/MPL 文件进自研内核（主参考用 Rust OS：Redox/Theseus/zCore；hubris 只借语义）。
- 用户程序编译链走 builtin `aarch64-unknown-none` + build.rs 注入链接脚本（stable 免 nightly，G-eco-a 决策勿回退 custom target；`-zmax-page-size=4096` 必带否则 0x1000 基址表达不了）；`内核/src/userbins/*.bin` 是 gitignored 编译产物——缺产物 = 裸 `cargo build` 编译期炸响（设计如此），`just build/check` 已挂 build-user 依赖自动补产。
- asm 模板换轨编译产物时，重试形态（自旋 vs 阻塞）是契约的一部分（G-eco-b 实踩）：逐发送点核对 asm 标号 EBUSY 分支去向（`b Nb` 回环自旋 vs clock+wait 阻塞），不能按家族归纳——console 饱和窗口内阻塞等待者会被自旋友商抢槽饿到超时；多页平像 spawn 的 sp 读 `UserProcess::stack_top` 字段；svcmgr build.rs rerun-if-changed 须盖全 staged 平像（漏列 = 改了子程序、内核还嵌旧字节静默上线）。
- console 通道纯自旋 send = 长跑饱和下结构性饿死（雷 2 残留实证；与上条对账——自旋赢 boot 竞速、输长跑公平）：自旋者从不阻塞 → 环序冻结 → 排水窗口被占优者垄断（五任务 30s 零成功发送 + pinger -110 下游晚杀）。新用户程序打印一律阻塞形态（EBUSY → wait_one(console, WRITABLE, 10s) → 重发，超时重臂）；svcmgr 豁免亦于 P5 A-2 撤销（send_spin 全量→send_retry 阻塞——全场生产者皆阻塞，无一幸免）。
- **wait_many 恒满足项使常驻服务永不入睡 = drain 相位钉死 RR 环（P5 A-2 实踩，上条姊妹坑）**：等待数组里「对端永不满 → WRITABLE 恒真」的项让 wait_many poll-first 每次即返、服务从不进 SLEEPQ → drain 相位冻结 RR 环固定节点 → 环序固化后填槽序末位者结构性饿死。修法 = 恒满足项条件化入列（仅 plen>0 才入列该项，常态真睡）。给常驻服务加 wait 项先问：稳态下会不会恒满足？
- 不要用 8 字节写 handle 出参（G-eco-c 实踩）：ABI §8 冻结 handle=u32，内核一律 `copy_out_u32`——8 字节写会砸编译 Rust 紧排的邻接局部（asm 模板 8 间距手排槽 + ktest `as u32` 截读曾掩盖）；非 handle 出参（ns/size/vaddr/idx 等 usize）仍 8 字节，宽度跟 ABI 类型走。真实链接器 PT_LOAD 不页对齐（RO 段与 text 共享页）——解析器不收页对齐，消费端页铺平单 VMO 消化。
- std 程序栈按程序面给 + 映像/栈之间必设保护页（G-eco-d d-3c hexyl 实踩）：默认 8 页栈被 ≈64KB 用量下行扎进映像 RW 页腐化 .bss（首击 `__aarch64_have_lse_atomics` → LSE 误判 `casa` UNDEFINED，死状与死因南辕北辙）——栈页数参数化（hexyl 32 页）+ 保护页（栈穿 = 干净 D-abort）。多分片用户输出不可直接当 smoke 标记（>192B 经 console 分片会被稳态矩阵插花撕行）——私有 stdout 通道收齐 + 零长 EOF（`park` 写侧约定）+ 宿主基准逐字节比对后才打单消息原子标记（`HEXYL ok` 模式）。
- 模板握手信号用专用位、清位豁免——模板不烤调度时序假设（P5-0 实踩，waiter 位拆分 a27eb9c）：一次性无确认 signal 与对端多段前置窗口天然竞速（死状双相位：早到被清位抹掉→10s 空等 / 意外命中→严格断言炸）；修法 = 握手专用位 + 全部清位掩码豁免 + observed 断言前掩位——任意调度序成立。现役用户位 1<<32～1<<39（U4=握手专用 P5-0 起、1<<39=RX Event P5 A-1 起），新增前先查碰撞。
- GIC SPI 段 ITARGETSR 复位 0 = 静默不投任何核（P5 A-1）：PPI 只读惯例不可照搬——不真写 target 则 pending 挂着无人收（完全静默）；新 SPI 源走 `gic::enable_spi` 一次齐。
- 电平触发源去活电平必须在 EOIR 前 + `exception_irq` 已按 INTID 分流（P5 A-1）：PL011 RX = drain 到 RXFE（不排空 → EOIR 后线仍高立即重投活锁；CNTV rearm 同铁序）；tick 槽只对 CNTV（新源进 tick 路径 = 授片与 CVAL 脱钩）；新源加 match 臂。PL011 FEN=0 有效 FIFO 深 1 字节——QEMU chardev 流控押住输入不 overrun（INT_OE 从不置位，真机才丢字；QEMU 唯一丢字向量 = 环满 lost），消费面按打字速率（smoke 逐字节慢注入 = 标记确定性）。rx_irq/rx_init 尾 ICR 勿清 RXIC（0x7ef 非 0x7ff，对抗审查实抓）：QEMU INT_RX 仅入队 count==1 严格判等置位——drain 末次 FR 读与 ICR 写间的竞速字节锁存被抹 = 滞留 + 线永低 + 流控押住 = RX 永久静默挂起；掩 RXIC 靠电平重投兜底。FEN=1+RT 留后续实测（QEMU 对 RT 建模未验，别盲开；A-2 为 bsh 未做此面）。console spawn arg 已满 → 新 handle 走静态槽（RX_EV_H/RX_EV_REF，ObjRef retain 永不 release）。
