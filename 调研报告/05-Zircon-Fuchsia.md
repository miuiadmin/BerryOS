# Zircon / Fuchsia 调研报告

> 调研日期 2026-10-15 · 基于公开资料与源码快照 · 许可证：Fuchsia 逐文件 BSD 3-Clause / MIT / Apache-2.0；zCore（本地 `参考源码/zcore/`）MIT（rCore-os 2019-2020）

## 0. 一句话定位

Zircon 是 Google Fuchsia 的 C++ 微内核（内核态对象/句柄/权限 + 每进程地址空间 + channel IPC），BerryOS 设计谱系的直系参照；zCore 是 rCore-os 社区对 Zircon 的纯 Rust 复刻，是本次源码走读的主对象。

## 1. 谁在做（2025–2026 现状）

- **Google 官方**：开发在 `fuchsia.googlesource.com`（非 GitHub 主仓）。落地设备仍只有 Nest Hub 智能音箱/屏一系，2025 年随 Fuchsia 16 更新加入 Bluetooth/Matter/Thread 支持（来源见 §8）。2021 年大裁员后团队规模收缩（公开报道称约百人量级，确切数字待核实）；2025 年末出现 Google 内部 "Aluminium OS（ALOS）" 基于 Fuchsia 做 PC 形态的传闻（待核实，无官方确认）。Gemini for Home（2025-10）后 AI 语音管线跑在 Fuchsia 设备上的部分细节待核实。
- **zCore**：rcore-os 社区（清华 rCore 系）维护，教学/研究定位，MIT。本地快照含 `zircon-object` / `zircon-syscall` / `kernel-hal` 等 crate，syscall 层实现度约 150/167。

## 2. 总体架构

内核目录（gitiles 实测 main 分支）：`zircon/kernel/{arch, lib, object, phys, platform, top, vm}`——`arch` 平台层、`lib` 通用库、`object` 内核对象、`phys` 物理地址模式的早期引导、`vm` 内存管理；构建用内核专用 GN 工具链按架构/编译目标分 switchset（`zircon/kernel/README.md`）。内核外的世界全是组件（component manager 管理）+ 用户态服务：driver_manager（DFv2）、文件系统、网络栈、power 框架（§3.6）。**"微内核"但内核不小**：对象/调度/VM 都在内核态，哲学是 Zircon 务实派——与 BerryOS 架构决策表的「内核有对象/分配器」一致。

## 3. 核心机制深挖

### 3.1 能力 = handle + rights

一切资源经 `Handle{object: Arc<dyn KernelObject>, rights: Rights}` 引用；`Rights` 是 bitflags（DUPLICATE..MANAGE_THREAD = 1<<18，SAME_RIGHTS = 1<<31）。复制句柄**只允许收窄**：`sys_handle_duplicate` 校验 DUPLICATE 权限后，若 `(handle_rights & rights) != rights` 即 INVALID_ARGS（`参考源码/zcore/zircon-syscall/src/handle.rs` L19-31）——权限单向递减是整个安全模型的根基。

### 3.2 对象 + 信号 + Port 事件模型

每个内核对象带信号位（内核低位 + USER_SIGNAL_0..7 = 0xff<<24），等待即轮询信号位。异步路径 `object_wait_async` 把信号变化打包成 packet 投给 Port；Port 有全局 16×1024 / 每端口 2048 的 packet 上限，超限返回 SHOULD_WAIT 背压（`zircon-object/src/signal/port.rs`）。同步 channel 带 txid 的 call/reply 走拦截逻辑（`ipc/channel.rs`，next_txid 从 0x8000_0000 起）。

### 3.3 调度器（重点核实，两份一手材料）

**a. 官方概念文档**（fuchsia.dev `concepts/kernel/kernel_scheduling`，2026-04 更新，webReader 全文抓取）：混合 Fair（加权公平队列 WFQ，摊还 O(1)）+ Deadline（EDF）；每逻辑 CPU 三个运行队列——critical deadline / deadline / fair，各为一棵 augmented WAVL 树；选择顺序：最早完成的 critical deadline → 最早 deadline → fair 队首。放置判据 = 亲和性 + 上次所在 CPU + 利用率 + 性能/功耗标度（映射 DVFS 频点，能耗感知选核）；负载均衡用**簇感知的延迟 work stealing**（本核队列低于簇均值-阈值才触发）。Deadline 是带宽契约（音频/VR），不是重要性等级；critical 档专供 timer 线程/IPI 类内核工作。
**b. 源码 README**（`zircon/kernel/lib/sched/README.md`，gitiles 实测抓取）：新调度库把两类统一为**带宽模型**——线程工作按固定宽度"激活周期"分帧，配额拆为 firm capacity（周期内必须完成 = deadline）+ flexible weight（剩余带宽按权重分 = fair），effective capacity = firm + flexible；选下一个线程按**最早完成时间**（平局比开始时间、再比地址），由 `sched::RunQueue` 承载；**阻塞线程仍为所在周期贡献 firm demand**，快速解除阻塞不被惩罚。抢占时刻 = min(周期到期、达容量、更早开始线程就绪、被阻塞持有 firm demand 者的最早完成时间)。
机制/策略分离点在参数层：策略由用户态经 `zx_object_set_profile`（zCore consts.rs 中 OBJECT_SET_PROFILE=76、PROFILE_CREATE=105）注入，调度算法本体在内核——与 BerryOS「机制在内核 + 策略 trait 可插拔」同构但更内核化。

### 3.4 内存：VMO / Vmar / 用户态 pager

`VMObjectTrait`（read/write/commit_page/commit/decommit/create_child/pin/…）三种后端 paged/physical/slice；`VmObject` 持父子链（Weak），COW 子 VMO、`VMO_ZERO_CHILDREN` 信号在 0↔1 边沿翻转，Drop 时把孙辈重挂到祖父（`参考源码/zcore/zircon-object/src/vm/vmo/mod.rs` L379-408）。Vmar 以 `CAN_MAP_READ/WRITE/EXECUTE` 等 flag 做映射粒度权限。缺页供应走用户态 pager（PAGER_CREATE=77 等系统调用组）。

### 3.5 任务树与 Job 策略

Job→Process→Thread 三层；`JobPolicy` 15 个 PolicyCondition × PolicyAction（Kill/AllowException/DenyException…），子 Job 合并时 parent-wins；`set_critical` 可把进程标记为 critical_to_job，进程死则 Job 亡——这是 Zircon 的失败监督原语（`task/job_policy.rs`、`task/job.rs`）。线程状态机含类型化阻塞子状态（BlockedFutex=0x303、BlockedPort=0x403、BlockedChannel=0x503…，`task/thread.rs`）。

### 3.6 电源与驱动框架

电源框架全在用户态：RFC-0250 Power Topology 定义 power element DAG，Power Broker（fuchsia.power.broker FIDL，Lessor/DependencyToken）做仲裁，SAG 管系统活动，RFC-0230 定义 Suspend-To-Idle；内核侧只在 `zircon/kernel/lib/power-management/`（gitiles 目录实测存在）参与 CPU idle/挂起协调。驱动 DFv2 由 driver_manager 启停驱动并路由 FIDL 能力；**devfs 正在迁移**：设备访问从 `dev-class` 目录能力改为逐设备 service 能力路由（官方迁移指南，见 §8）——目录式设备命名空间被实践否定中。组件框架 CFv2 用 manifest 的 expose/offer/use 声明做能力路由，component manager 惰性拉起提供方——内核 handle 能力的用户态回声。

## 4. 源码走读（zcore 本地快照）

- `zircon-syscall/src/lib.rs`：核心派发 `pub async fn syscall(&mut self, num: u32, args: [usize; 8]) -> isize` 巨型 match；crate 头 `#![deny(warnings, unsafe_code, unreachable_patterns)]`——**syscall 层零 unsafe**，证明该模型可全程安全 Rust。两个合并快路径值得注意：

```rust
Sys::VMAR_UNMAP_HANDLE_CLOSE_THREAD_EXIT => self
    .sys_vmar_unmap(a0 as _, a1 as _, a2 as _)
    .and_then(|_| {
        let _ = self.sys_handle_close(a3 as _);
        self.sys_thread_exit()
    }),
```
（线程退出路径免三次陷入；FUTEX_WAKE 版本 200 还带 `Ordering::Release` 用户内存写。）
- `zircon-syscall/src/consts.rs`：build.rs 从 zx-syscall-numbers.h 生成，**COUNT=167 + 2 个合并调用 = 169**，对照 BerryOS v1 的 ~26（object_* 合并）可见"中等偏上"口径的真实上限。
- `zircon-object/src/object/mod.rs`：`KObjectBase{KoID(全局 AtomicU64 自 1024), Mutex<Inner{name, signal, signal_callbacks}>}` + `impl_kobject!` 宏 + 手写 `SignalFuture`（poll + signal_callback + Waker 注册），`wait_signal_many` 支持多对象等待。
- `zircon-object/src/vm/vmo/mod.rs`：见 §3.4；`VmoInfo #[repr(C)]` 与官方 ABI 对齐。

## 5. 亮点

1. **能力模型自洽**：narrowing-only 复制 + 信号/等待/Port 三层事件模型 + CFv2 声明式路由，内核态与用户态用同一套能力语义贯穿。
2. **调度器先进且已文档化到可实现程度**：统一带宽模型（firm/flexible）、augmented WAVL、簇感知延迟 stealing、DVFS 能耗感知选核，软实时契约（deadline=带宽合同）与 best-effort 同队列管理。
3. **syscall 合并快路径**（200/201）与 `deny(unsafe_code)` 的 syscall 层：工程上兼顾性能与安全。
4. **电源全用户态 + 内核最小协调**：power element DAG/Broker/SAG 分层清晰，对应 RFC-0230/0250 治理流程。
5. **zCore 证明该内核对象模型可纯 Rust 安全复刻**——对 BerryOS 是可行性背书。

## 6. 不足与争议

1. 内核 C++，无内存安全保证，数十万行 object/vm 代码靠 review 与模糊测试兜底。
2. 生态封闭：自有 ABI、不跑 Linux/Android 二进制，应用生态始终未立；设备面七年只覆盖 Nest Hub 一系。
3. 组件机制重：manifest/resolver/runner/capability routing 全链路学习与调试成本高，devfs→service 迁移本身说明初版设备命名空间设计走了弯路。
4. 治理单点：完全 Google 主导，社区贡献面窄；团队 2021 缩编后演进速度放缓（幅度待核实）。
5. 167 个 syscall 的宽接口面（含 PCI/hypervisor/debuglog 等杂项）增大攻击面与稳定性负担。

## 7. 对 BerryOS 的借鉴与警示

**已经拿了什么**（CLAUDE.md 决策表已对齐）：Zircon 务实派对象内核、handle+rights 纯能力、VMO/Vmar/用户态 pager、channel+port IPC、tickless、用户态驱动与 PM。zCore 层面还隐性拿了 KObjectBase/信号位/类型化阻塞态的建模方式。

**还值得再挖（机制级）**：
1. **libsched 带宽模型**：firm capacity（周期内必保）+ flexible weight（余量按权重）统一建模，单棵 augmented WAVL RunQueue 按"最早完成时间"选线程——BerryOS class_deadline/class_fair 各自队列之外的可选统一底座；尤其"阻塞线程在所在激活周期保留 firm demand"的快唤醒不惩罚语义，直接可用于 AI 调度的 RT 护栏设计。
2. **critical deadline 档**：专供 timer/IPI 类内核工作的最高档，BerryOS 的 RT 屏蔽可借鉴此分层而非单一 RT 优先级。
3. **Job 策略树 + critical_to_job**：15 条 PolicyCondition×Kill/Exception 动作、parent-wins 合并——BerryOS 失败隔离/监督重启（设计原则 5）可直接化用为 Job 树原语。
4. **Port 背压常数**：全局 16K/每端口 2048 packet→SHOULD_WAIT，给 BerryOS AI 遥测共享环的容量与背压策略提供现成量级参照。
5. **合并退出快路径**：`*_HANDLE_CLOSE_THREAD_EXIT` 的单陷入三动作模式，适合 BerryOS 无 fork 设计下的线程退出热路径。
6. **CFv2 expose/offer/use 路由语法**：BerryOS 路径式命名空间（IPC 设计文档）可借其声明式清单做服务发现与权限传递的用户态配置面。
7. **性能/功耗标度**（performance/power scales 映射 DVFS 频点入调度判据）：与 BerryOS capacity(0..1023) 同向，可补每核"当前等效算力"维度。

**警示**：① Zircon 策略在内核（profile 只传参），BerryOS 是策略 trait 可插拔——借鉴算法别连"策略驻内核"一起搬；② devfs 目录命名空间被官方迁移否定，BerryOS 设备/服务路径设计要一步到位 service 化；③ 生态孤岛的代价（自有 ABI 无兼容层）Fuchsia 已演示七年，BerryOS 的 POSIX 兼容库必须早于生态预期落地；④ 组件机制全套（resolver/runner/manifest）v1 不要上，先留能力传递内核语义即可；⑤ 官方 C++ 细节（spinlock 抢占关断语义、timer 内部结构、SMP 启动流程）本轮未能直读源码验证，相关处均待核实，勿从本报告转引为实现依据。

## 8. 参考资料链接

- 调度概念文档（2026-04 更新）：https://fuchsia.dev/fuchsia-src/concepts/kernel/kernel_scheduling
- libsched README（gitiles main，实测抓取）：https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/lib/sched/README.md
- Zircon 内核源码树：https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/
- power-management（内核侧目录）：https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/zircon/kernel/lib/power-management/
- 电源框架总览：https://fuchsia.dev/fuchsia-src/concepts/power/intro/in_depth_overview ；RFC-0230：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0230_suspend_to_idle ；RFC-0250：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0250_power_topology
- component manager（2025-02 更新）：https://fuchsia.dev/fuchsia-src/concepts/components/v2/component_manager
- devfs→service 迁移：https://fuchsia.dev/fuchsia-src/development/hardware/frameworking-migration/devfs-to-service-based-access
- Fuchsia 安全论文：Pagano et al., "Understanding Fuchsia Security"（JOWUA v12n3）
- FOSDEM 2025 组件 vs 容器：https://fosdem.org/2025/schedule/event/fosdem-2025-5650-comparing-fuchsia-components-and-linux-containers/
- zCore：https://github.com/rcore-os/zCore （本地：`/Users/murongcheng/Documents/code/berryos/参考源码/zcore/`）
