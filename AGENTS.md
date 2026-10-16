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
- `文档/开发体验.md` —— Mac 上的运行/安装四级阶梯（QEMU 直加载 → initrd → fat:rw → 自举镜像）+ 交叉编译日常循环 + hvf/TCG 双模式。
- `文档/shell设计.md` —— P5 交互 shell（bsh）：命令三梯队、channel 即 pipe、与 UNIX 生态的源码级兼容边界。
- `文档/调度器设计.md` —— 调度器与电源详细设计。
- `文档/架构决策.md` —— 全部决策的单一事实来源（ADR）。
- `文档/能力模型设计.md` / `IPC设计.md` / `内存管理设计.md` / `系统调用ABI设计.md` —— 各子系统详细设计（均含「阶段化目标」节）。
- `文档/系统调用ABI设计.md` 另含具体 syscall 编号表 + POSIX 子集边界（v1 无 fork）。
- `文档/测试策略.md` —— 关卡式验证 G0–G5：单核正确 → 抢占 → SMP → 全抢占 → demand paging。
- `文档/AI调度设计.md` —— AI 原生调度：策略注入/遥测 ABI、安全护栏、平台分层。
- `文档/安全与硬件隔离设计.md` —— IOMMU/SMMU + PAC/BTI/MTE + 度量启动。
- `文档/可观测性设计.md` —— day-one 内置 tracing。
- `文档/文件系统设计.md` —— 现代 FS 目标（FAT 仅 MVP）。
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

Phase 2b 已收官（2b-0~2b-4 全落地，出口关卡 G1.5 过关）、**Phase 3-0（内核对象壳 + handle 表）/3-1（syscall ABI 骨架）/3-2（VMO/Vmar syscall 全量门控 + copy_from/to_user + 悬空 PTE 防线）/3-3（process/thread 最小对象）/3-4a（per-process handle 表 + bootstrap 迁移）/3-4b（channel 三号 + handle 随消息迁移）/**3-4c（event 三号 + 遥测环 + 空策略表等价测试——Phase 3 收官）/4-0（睡眠/唤醒原语 + thread_sleep 首个阻塞 syscall）/4-1（阻塞 object_wait_one + signal 遍历唤醒）/4-2（wait 面 ABI v0：信号位空间 + wait_one 扩 Channel + wait_many，4-2a/4-2b 双步）/4-3a（调度决策看 effective——pick 按 `prio_effective` 出选，同 eff 层内轮转跨层严格优先）/4-3b（PI 捐赠——`内核/src/pi.rs` 全图重推：donor = 睡 channel 等派生位者，受捐 = handle 表持 peer 端点带解锁 right 的睡眠者；≤8 轮不动点传播 + 冻结随行 + 零分配；ktest J 段 `PI-K ok` + boot 三任务 `PI ok` 第 18 标记）/4-3c（intent 五件套——`进程调度/src/intent.rs`：msg_intent 随消息传播 + 每消息重导 min + 三道帽（跳数 8/每跳 50ms/链累计 400ms）+ channel_create options bit0 opt-out + 遥测三计数器；ktest K 段 `INTENT-K ok` + boot 六任务 `INTENT ok` 第 19 标记——4-3 收官）/4-4（fastpath/slowpath 分层：find_single_waiter 检查链（全队无哨兵 + 恰一 READABLE 匹配者）+ fast_wake 定点唤醒（与 wake_matching 共用 wake_one_inner + PI 重推——唤醒集恒等）+ try_preempt 直切（严格更高 eff 才切，seL4 direct-switch 同款）+ ipc_fast_hits/misses/direct 三计数器；boot fa_ping(16)/fa_pong(32) bench `FAST ok` 第 20 标记 + ktest L 段 `FAST-K ok`）/4-5a（console 服务循环 + 调试槽 0xFE/0xFF 退役 + 用户打印真 IPC）/4-5b（svcmgr 拉起 echo 服务——4-5 收官）/4-6 B 半（AI 最小演示管线：tele: dump → advisor.py → bind 注入表 + clamp 执法 + rt2≤2×rt1 开销断言，AIDEMO ok 第 22 标记）+ 4-6 A 半（try_preempt 幻影出选饿死修复 + talker/echo2 两服务全链演示——DEMO ok 第 23 标记，4-6 收官）+ G-eco 入机侧第一刀（rootfs 递送通道：boot x19 抢救 + `dtb.rs` /chosen 解析 + fw_cfg 具名文件装载，`just run-rootfs`；QEMU 非 Linux ELF 直加载不走 Linux boot 协议——x0=0/RAM 无 DTB/-initrd 不加载，见 CLAUDE.md Gotcha）已落地**：stable Rust + `aarch64-unknown-none`，`just run` 在 QEMU `virt` 串口依次打印 `BerryOS boot` → `KVA ok`（TTBR1 高半别名自校验，2b-2 起只走高半）→ `ROOTFS ok src=… / ROOTFS absent`（G-eco 入机侧：fw_cfg/DTB 双源装载，无 rootfs 正常降级）→ `GIC ok / FRAME ok / HEAP ok / SYSCALL ok`（3-2 起含 VMO/Vmar 九号全链；3-3 起加 process/thread F 段：双出参/参数面/门控/归属面/钉住释放链；3-4a 起加 G 段：两表隔离/take 迁移作废/insert_ref 落新表/子进程出生空表；3-4b 起加 H 段：channel 双端/空 -EAGAIN/往返/参数上限/满 -EBUSY/RO 拒/handle 随消息迁移/send 原子性；3-4c 起加 I 段：event create options 拒/poll 未命中 -EAGAIN/deadline=1 远古时刻 -ETIMEDOUT 不睡（4-1 改板）/signal 置清位/全满足集回写/dup 无 SIGNAL -EACCES 但 wait 照常/类型野值 -EBADF；4-3c 起加 K 段：intent 纯函数面十用例（编码域/快照三分支/结算五路/options 面））→ `PING TASK ok`（switch asm 乒乓）→ 低半读期望故障恢复 → `ASPACE ok`（两张用户页表同 VA 隔离 + asid=16b）→ `VMO ok`（VMO/Vmar 六操作 + 两次期望故障）→ `HANDLE ok`（3-0 handle 表五操作 + 槽位重用代号 +1）→ `POLICY ok`（3-4c 空策略表等价 + 4-3a 真消费：未绑/绑空表逐项全等 + 对照表 eff=32 者独占出选——严格优先，旧「RR 不看优先级」护栏随 4-3a 撤）→ `ktest done` + brk 可恢复 dump → **两忙循环任务 `A`/`B` 与 EL0 用户进程 `user_good`（打 'u'）/`user_abuser`（打 'S'，坏 handle 滥用 → -EBADF 存活）/`user_spawner`（打 's'，**3-3 验收：经真 syscall 创建第二个进程；3-4a 起 bootstrap 迁移后 close 原值必 -EBADF**）及其子进程 `user_child`（打 'C'，**3-4b 验收：凭自己表里的 channel 端点 recv 出父进程 send 的 PING+礼物 handle、vmo_read 魔数比对——handle 随消息跨表迁移闭环；稳态收 PING 回 REPLY 与 spawner 交换消息**）在 CNTV timer slice 下交替（A/B 每次打印先读私有页断言隔离）**，周期打 `switches=N` 金丝雀；user_good 入口 x0 = root Vmar handle（首能力）先跑一次性验收链 `vmo create→commit→write→map→经 VA 读写→别名回读→close 拒(-EBUSY)/RO 写拒(-EACCES)→别名 close→unmap→decommit→close` 全绿才进稳态循环；spawner 链 `process_create`(双出参)→`vmo_create/commit/write`(内嵌子代码)→`vmar_map` 进子空间→建礼物 VMO→`channel_create` 建对→`send(PING+礼物 handle)`（**3-4b：handle 随消息迁出本表**）→`thread_create`→`process_start(bootstrap=end1)`（端点迁子表，child 凭它收发）；EL0 syscall 经 `Task::proc_tag` 派发到所属进程自己的 handle 表（take/insert_ref 迁移原语，refs 不变）；恶意用户进程 rogue_ptr（野指针）dump + 被杀（**只死自己**，watchdog 断言杀数==1 且死者是 rogue_ptr）+ 3-4c 遥测环存活断言（`telemetry::total() ≥ switches()` + 环内 vct 单调）+ 唤醒环断言（`BLOCK_BOOT_DONE` 门槛后：wake_total≥2 / 延迟非负 / vct 单调 / 环未淘汰期双醒因覆盖）打 `G1.5 ok`；**4-0 验收：sleeper/waker 对打**——sleeper 超时自醒（'T'，断言 Timeout+时长）+ 条件唤醒（'K'，挂 SLEEP_OBJ 被 waker 轮询 `wake_matching` 摘醒断言 Signaled）双绿打 `BLOCK ok`，此后两者 exit 退出、稳态矩阵无扰；**4-1 验收：user_waiter（打 'W'）三路 + waker2 对打**——waiter 超时（-ETIMEDOUT + observed==0 + 真睡到点）/poll 命中（恰置 bit5 → 请求超集 → observed==0x20 全满足集）/真睡（10s deadline）被 waker2 真 SVC signal 唤醒（observed==0x8）三路全绿才进稳态打 'W'；waker2 内核任务经真 SVC 链（clock_read→thread_sleep 3 slice→signal 内核侧表 handle→再睡 1 slice）打 `WAIT ok`，此后 exit；同一 Event 双表挂同一 ObjRef（bootstrap 表 + waiter 表）——signal/wait 各持己表 handle 的跨表互通活证；**4-2a 验收：user_pinger（打 'p'）/user_ponger（打 'P'）对打**——boot 手搭 channel 对（两表各 insert 一端 + pair 互钉，pinger 持 endA SEND / ponger 持 endB RECV）；pinger send 灌到 -EBUSY → `wait_one(WRITABLE, now+10s)` 阻塞 → ponger recv 腾位唤醒 → 补发成功 → 首圈打 `CHAIN ok`（smoke 第 16 标记）后稳态每圈 'p'；ponger recv/-EAGAIN → `wait_one(READABLE)` 阻塞 → pinger send 唤醒 → 验 PING 载荷打 'P'——「send 满 → 对端 recv 腾位唤醒」两进程阻塞链活证（ktest 同步加 I6 低半拒面 + I7 Channel poll 面全套；Event 位面/I 段/waiter/waker2 全量迁高 32 位用户半）；**4-2b 验收：ponger 稳态等待改 wait_many n=2**——[endB:READABLE, boot 直插哑 Event:1<<32 永不 signal]，命中须 `*idx==0`（哑 Event 位永不置、idx==1 即断链自陷），首次打 `MANY ok`（smoke 第 17 标记；哑 Event handle 0x0001_0002 boot 断言定值——ponger 模板硬编码依赖）；ktest I8 盖错误面+poll 面（n 界/坏 handle/poll -EAGAIN 且 idx 留哨兵/远古 deadline -ETIMEDOUT 不睡/命中 idx 两向），B 段未实现样本 0x05→0x33；**4-3b 验收：pi_client(32)/pi_server(0)/pi_probe(0) 三内核任务对打（全真 SVC）**——client 灌满 send 后 wait WRITABLE 睡（donor）、server 睡 Event 时 block 钩子重推成边受捐 32、probe 睡 5 slice 醒来断言捐赠字段（server pi==32 且 eff==32）后 signal 放行打 `PI ok`（smoke 第 18 标记），server 醒（冻结 32 随行）recv×8 腾位唤醒 client、client 补发自证后双双 exit；ktest J 段 `PI-K ok` 八用例（单跳/rights 门控/链式两跳/环收敛/轮帽截断/哨兵跳过/冻结回落/共享表粒度）；4-0 sleeper 信号位连带收窄（合成对象只等用户位 1<<33——带派生位睡非对象会被 PI 图解引用炸对齐）；**4-3c 验收：it_driver/mid/sinkA/sinkB/slow/probe 六内核任务对打（全真 SVC）**——借入生效（sinkA eff==0x02 且 hop==1 = mid 借入转发活证）/opt-out 回落/每跳预算帽（slow 借入后睡 200ms → send 降级装自己的 declared、借入字段保持）/双帽拒传播（boot 伪造消息直压收端队列：hop=8 恰顶、acc=400ms+1 恰超），probe 遍历 SLEEPQ 断言四睡眠者字段 + 三计数器打 `INTENT ok`；三条 `intent: … cap` trace 行；`channel_create` 改 options 首参（ABI §8 修正）；**4-4（fastpath/slowpath 分层——`find_single_waiter` 检查链 + `fast_wake` 定点唤醒 + `try_preempt` 直切 + telemetry 三计数器）**：fa_ping(16)/fa_pong(32) bench 对打（专用通道双端各 SEND|RECV）——稳态每轮 ping send 检查链全过 → fastpath + 直切（32>16）、pong 回件时 ping Ready 未睡 → 慢路 miss；ping 收尾断言 hits/direct ≥128、miss≥1 + CNTVCT 计量 RT 打 `FAST ok`（TCG 量级 ≈28.4k 参考 cycle，真机 <1200/<500 线留 G 卡复测）；ktest L 段（`FAST-K ok`）盖 find_single_waiter 纯函数面六用例 + SVC 慢路回归；**4-5a 验收：console 内核服务常驻**——稳态矩阵 A/B/u/s/S/C/p/P 全员打印经 channel_send(endU) → console 排水落串口，watchdog 用户活证改 CONSOLE_DRAINED 服务侧计数（console 挂 spawn_with_tag 私有表 + 稳态等待 wait_many——PI 双保险），ktest A 段 0xFE/0xFF 断言 -95 退役面；MANY ok 标记改 ponger 入口先发 wait_many 定确定性（稳态 -EAGAIN 是排水/补灌竞速——打印走 console 后实测 120s 队列恒非空的 run 存在，标记不赌竞速）；`just run-el2` 同（EL2→EL1 eret 降级路径）；**4-5 前置补丁已落地**：`spawn_with_tag` + `caller_table` EL1 分支 tag≠0 走私有 handle 表（修 4-3b 共享表粒度的错捐级联——常驻内核服务须挂 `Box::leak` 私有表，tag==0 的 ktest/boot/短命 bench 零扰动；自证 svc_tag 双断言打 `TAGTAB ok`，非 smoke 标记）；**4-5b 验收：user_svcmgr（打 'v'）经真 syscall 全链拉起 echo 服务**——process_create（双出参）→ vmo_create/commit/write（内嵌 echo 模板 268B）→ vmar_map@0x1000 → channel_create → thread_create → process_start(entry, sp, bootstrap=end1)（close 原值必 -EBADF 迁移自证）；svcmgr send ECHO_REQ → wait_one(end0, READABLE) → recv 验低 32 位回显打 `SVC ok`（smoke 第 21 标记）后稳态节流循环打 'v'；echo 子进程静默跑最小服务循环（recv → 原字节回发，-EAGAIN→wait READABLE 重收）；svcmgr/echo 均 EL0 每进程私有表 + prio 0——PI 面零新增。**4-6 A 半验收：talker/echo2 两用户态服务互联**——svcmgr `SVC ok` 后直线拉起 talker（936B 模板）+ echo2（复用 echo 模板），交接消息一次迁移双 handle（console 别名 + t2 对端），talker 凭收到的能力 echo2 round-trip DEMO_REQ 验回显打 `DEMO ok`（smoke 第 23 标记——B 半 AIDEMO ok 已占 22，尾插随合并序）后稳态 'd'；幻影出选修复后全员出选:切入 1:1（修复前 echo1 29:1 冻结 + svcmgr 超时死）；MMU 开（高半 TTBR1 内核表 `KERNEL_VA_BASE`，**2b-1 起内核主体高半执行**、异常 dump PC 应为 `0xffffff80…`；**2b-2 起低半身份表退役**——低半访问必 fault；GIC/UART MMIO 全走高半别名；**2b-3 起 VMO/Vmar 为正式映射路径**（map 即填用户页表）；smoke 固化程序头断言 `size.py --assert-flip`）、buddy/slab 堆、异常向量（current-EL + lower-EL 双路）+ panic dump、GICv2、SVC syscall（EL1/EL0 双入口 + 语义层注册 + copy_from/to_user 界检，24 号实现——0x04 阻塞化 wait（4-2a 扩 Event|Channel）/0x05 wait_many 随 4-2b/0x06 signal+唤醒+位空间检、0x37 thread_sleep 随 4-0；0xFE/0xFF 调试槽 4-5a 退役（-ENOTSUP）；未知号 -ENOTSUP 永不杀）、内核线程 + 上下文切换 + tickless idle + IRQ 抢占（含 EL0 运行中抢占）+ 切换遥测环（四切换位 yield/preempt/exit/block 写静态预储环，零分配）+ 唤醒遥测环（4-0 配套：双唤醒位 Signaled/Timeout 写 `WAKE_RING`，block→wake 延迟数据源，零分配）+ 睡眠/唤醒原语（4-0：SLEEPQ 单队列按 deadline 升序 + block_current/wake_matching + tick 扫超时 + thread_sleep 0x37；4-1：wait_one 阻塞化（poll-first + 醒来重验 + -ETIMEDOUT）+ signal 遍历唤醒 + 睡眠钉住 ObjRef::retain/release；4-2a：Channel 唤醒点——send 成功唤对端 READABLE / recv 成功唤对端 WRITABLE；4-2b：WAIT_ANY 哨兵——多对象等待者任何唤醒都命中、醒来重验）全通。smoke 23 标记（`POLICY ok` 第 11 / `BLOCK ok` 第 14 / `WAIT ok` 第 15 / `CHAIN ok` 第 16 / `MANY ok` 第 17 / `PI ok` 第 18 / `INTENT ok` 第 19 / `FAST ok` 第 20 / `SVC ok` 第 21 / `AIDEMO ok` 第 22 / `DEMO ok` 第 23）+ switches 金丝雀严格递增。

**前置（macOS，一次性）**：`brew install qemu just` + `rustup target add aarch64-unknown-none`（stable，无需 nightly/build-std）。

**日常命令**（仓库根）：

```bash
just build          # = cargo build（内核 ELF）
just run            # QEMU virt → 1BerryOS boot（默认起始 EL1），halt
just run-el2        # virt,virtualization=on → 21BerryOS boot（EL2→EL1 eret 降级路径）
just smoke          # 无人值守冒烟（CI 同款）：boot 标记断言 + switches 金丝雀严格递增
just size           # 内核体积基线报告（文件量 / PT_LOAD 镜像与静态内存 / 段明细；CI 同步打印）
just run-rootfs     # G-eco 入机侧：带 rootfs 启动（-fw_cfg name=opt/berry/rootfs,file=rootfs.cpio）
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
- 不要在 syscall handler 里解蔽 IRQ 或忙等跨任务：SVC 入口硬件置 DAIF → syscall 主体不可抢占是 v1 免锁根基（3-4a 已拆 per-process 表，跨表访问仍全靠它串行）。
- 不要把「映射中 VMO 拒 close」写成一律拒：只拒**销毁性 close**（refs==1 且 mapcnt>0 → -EBUSY），别名 close（refs≥2）放行——对象不死，存活句柄钉住它。
- 不要在内核对象间存裸指针跨界引用（Thread→Process、Process→root Vmar 等）：必须 `ObjRef::retain/release` 钉住（3-3 起模式，见 `内核/src/process.rs`）——否则持有者关光 handle 即 use-after-free。
- 不要假设 QEMU 启动在固定 EL：默认 `-machine virt`+`-kernel` ELF 起始 EL1，`virtualization=on` 起始 EL2——boot 第一行 `mrs CurrentEL` 检测并按需 eret 降级到 EL1。
- 长命睡眠内核任务不睡共享表（4-5 纪律；4-6 新形态：被错捐 eff 32 的睡眠者自己抢片，把断言对象推过自身重试预算——验收司机/probe 也挂私有表，重试预算按「最坏轮转+自己偷片」放宽；halt 后「全系统冻结」先查 panic 尾巴）。
- `for_each_sleeper` 须 IRQ 屏蔽窗内调用（tick 摘队动睡眠链接——任务体直调须手动 disable/enable 包络）。
- 不要复制 GPL/AGPL/MPL 文件进自研内核（主参考用 Rust OS：Redox/Theseus/zCore；hubris 只借语义）。
