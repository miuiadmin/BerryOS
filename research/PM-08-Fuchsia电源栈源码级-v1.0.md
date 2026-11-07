# PM-08 Fuchsia 电源栈源码级深挖（v1.0）

> 调研日期：2026-11-07 · 方法：gitiles 直拉 main 分支 FIDL/Rust/C++ 源码逐字段精读（fuchsia.power.broker/broker.fidl 435 行、
> fuchsia.power.system/system.fidl 450 行、src/power/broker 8210 行 broker.rs、SAG 三模块、driver_manager/power、
> sdk/lib/driver/power、电池/挂起/输入/hrtimer FIDL）+ fuchsia.dev 概念文档 + F27–F30 发行说明复核（WebSearch/WebFetch）。
> 确定度图例：**高** = 一手源码/FIDL/RFC 原文（带行号或 URL）；**中** = 官方文档/发行说明转述；**低** = 推断。
> 与一期分工：PM-03 §5 已给文档级三件套（PowerBroker/SAG/RFC-0230）——本报告把它们落到 FIDL 字段与实现行号，
> 重叠结论一律「见 PM-03 §5.x」引用不重写；PM-00 §5.1 的 P1–P10 协议位在 §8 逐条对表收口。

---

## §0 一页结论（土话直答）

1. **Fuchsia 把「微内核 + 全用户态驱动」的电源问题走通了，靠的是一张图 + 三个句柄**：全局拓扑图（元素/档位/依赖边）、
   依赖授权句柄（DependencyToken=zx event）、租约句柄（LeaseToken=eventpair，最后句柄关闭即释放）、控制通道句柄
   （element_control 通道断开=元素出局）。仲裁器只算「每元素该在几档」=所有生效声明的 max（Minimum Power Level Policy），
   **一个字节策略都不做**——PM-03 §5.1 的文档级结论在源码层成立。
2. **挂起的触发不是「请求睡眠」接口**：租约图空 → broker 把 Cpu 元素 required level 降到 INACTIVE → SAG 的
   ElementRunner 回调被调 → 检查挂起阻塞器 → 调 `Suspender.Suspend(state_index)`。Linux 翻转式语义被逐字实现（PM-00 P1 裁决再添实证）。
3. **它踩的最大的坑是 API 形状换了三代还在换**：Lessor→Topology.Lease、LevelControl→ElementRunner、AcquireWakeLease→
   WithToken→WakeLease2（原子交接）、ActivityGovernorListener→SuspendBlocker。教训不是「别改」而是：**租约句柄（eventpair）
   作为唯一稳定锚点，所有换血都发生在句柄之上的协议皮**——BerryOS 应把句柄语义 day-one 冻结。
4. **归因做得比治理早**：每次唤醒回 WakeReason（中断 koid 列表）→ platform-bus InterruptAttributor 把 koid 翻成驱动名 →
   per-source 记账（signal_count/last_ack_time）→ 长租约超阈值自动报崩溃。BerryOS 刀 1（wakeup 记账面 P10）有完整可抄字段集。
5. **wake latency 没进 broker**：RFC-0230 设想的「每档唤醒延迟」最终只活在板级配置 FIDL（`Transition.latency_us`）和挂起档
   （`SuspendState.resume_latency`），仲裁图本身不消费延迟——深度策略（挂起前选档）做不了。BerryOS P3a 坚持把
   wake_latency_ns 放 manifest 申报是对的，且要**让仲裁器真的吃它**。

---

## §1 机制全景：组件与协议位地图【确定度：高（各组件源码一手）】

```
 应用/服务 ──AcquireWakeLease(名字)──▶ SAG ──Topology.Lease──▶ PowerBroker（拓扑仲裁）
                                       │                        │  AddElement(ElementSchema)
 驱动(dfv2) ──manifest: suspend_enabled / power_managed_dispatchers──▶ driver_manager
                                       │   每节点建二元元素+祖先链依赖+CPU/storage 依赖
 SAG(ExecutionState 的 owner) ◀──Cpu required=INACTIVE 即触发── broker
   └─▶ Suspender.Suspend(state_index) ──▶ 平台挂起（SuspendService 由 parent realm 提供）
        └─返回 WakeReason(koid[]) ──▶ WakeSourceObservability + InterruptAttributor 归因
 cpu-manager（独立于电源框架）：EM 配置注册进内核 + OPP 调节 + thermal 降载
```

| 组件 | 源码位置 | 职责 | 确定度 |
|---|---|---|---|
| PowerBroker | [src/power/broker](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/broker)（Rust） | 维护拓扑、算 required level、下发 SetLevel、租约生命周期 | 高 |
| SAG | [src/power/system-activity-governor](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/system-activity-governor) | ExecutionState/ApplicationActivity/Cpu 三元素 owner + wake lease 门市 + 挂起编排 + 归因 | 高 |
| driver_manager/power | [src/devices/bin/driver_manager/power](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/devices/bin/driver_manager/power)（版权 2026，现行） | 每绑定节点建二元元素；AllDrivers 元素收编全部叶子驱动；storage/CPU token 等待 | 高 |
| cpu-manager | [src/power/cpu-manager/README.md](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/cpu-manager) | RPPM 策略：OPP 经 fuchsia.hardware.cpu.ctrl 改频压；**EM 配置经 syscall 注册进内核**；thermal 降载 | 高 |
| power-manager | src/power/power-manager（thermal 枢纽，本报告不展开） | fuchsia.thermal.ClientStateConnector 等 | 中 |

**分工要点**：CPU 频压调节**不在电源框架内**——cpu-manager 直连 CPU 驱动 + 内核 syscall（README 原文："registers it with
the kernel, via syscall"）。【审校注记 2026-11-07：README 实证的只有「EM 经 syscall 注册进内核 + OPP 经 cpu-ctrl 直控」两件事；
「与 PowerBroker 分立/不在电源框架内」是架构推断而非 README 明言，确定度按中记】电源框架只管拓扑一致性与挂起次序。这正是 BerryOS「仲裁器不做策略、频率帽由策略服务单独书写」的活样本（样本强度=中，见上注）。

---

## §2 fuchsia.power.broker FIDL 逐字段【确定度：高（FIDL 一手，行号对 main）】

源：[sdk/fidl/fuchsia.power.broker/broker.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.power.broker/broker.fidl)
（全库 `@available(added=HEAD)`——**尚未进稳定 SDK**，文件头自注 "Draft Prototype ... should be expected to change" L4–7）。

### 2.1 基本类型与常量

| 名 | 定义 | 行号 | 备注 |
|---|---|---|---|
| PowerLevel | `alias uint8` | L16 | 档位就是小整数 |
| BinaryPowerLevel | OFF=0 / ON=1 | L20–23 | 官方推荐二元起步 |
| DependencyToken | `zx.Handle:EVENT` | L30 | **依赖授权 = 内核事件句柄**，先经 RegisterDependencyToken 注册 |
| LeaseToken | `zx.Handle:<EVENTPAIR, TRANSFER\|DUPLICATE\|WAIT\|SIGNAL_PEER>` | L180–182 | 句柄可复制共享；**最后一个句柄关闭 = 租约结束**（L176–179 原文） |
| LEASE_SIGNAL_SATISFIED | ZX_USER_SIGNAL_0 (0x01000000) | L184–185 | broker 在 token 上打信号通知 SATISFIED |
| 上限 | name≤64 / deps≤128 / tokens≤128 / levels≤256 | L25, L188–190 | PM-03 §8.2-1 的「name ≤64」即此 |

### 2.2 ElementSchema（AddElement 入参，L85–131）

| 字段 | 序号 | 语义 |
|---|---|---|
| element_name | 1 | 仅日志/调试用 |
| initial_current_level | 2 | 初始现行档 |
| valid_levels | 3 | **升序**列出全部合法档（L94）；未列=非法 |
| dependencies | 4 | 本元素各档**向下**的依赖边（对本元素的依赖不能在此加，L100） |
| lessor_channel | 8 | **已弃用**→用 Topology.Lease（L104） |
| element_control | 9 | **调用方自备通道**；通道关闭 = 元素从拓扑移除、全部子通道关闭、token 全注销（L108–113）——断线自动回收的白纸语义 |
| element_runner | 10 | broker 反向调 SetLevel 的客户端端 |
| initial_lease_token | 11 | 注册即持租约 + **抑制到达初始档之前的所有 SetLevel**（L118–121）——防启动期被拉闸的竞态原语 |

### 2.3 LevelDependency（依赖边，L46–66）与租约

| 字段 | 语义 |
|---|---|
| dependent_level | 依赖方哪一档有此依赖 |
| requires_token | 被依赖元素的 DependencyToken（能力授权） |
| requires_level_by_preference | **降序偏好表**：第一个合法档成为 required level（L54–57）；明说用于跨 API 级兼容（L59–61） |
| remove_with_required_element | 被依赖元素移除时自动删边（L63–65） |

`Topology.Lease(LeaseSchema)`（L81，L134–151）：`lease_token` + `lease_name` + **dependencies 非空**（空依赖直接
`LeaseError::INVALID_ARGUMENT`，L141–143）+ `should_return_pending_lease`（默认等 SATISFIED 才返回；可立即返回 PENDING，
靠信号通知）。**租约不再挂元素下（Lessor 弃用），独立入口、可跨多元素依赖**。

### 2.4 协议族与状态机

| 协议 | 要点 | 行号 |
|---|---|---|
| ElementRunner.SetLevel | **服务端阻塞到迁移完成才返回**；不能迁移且不能重试→关通道 | L261–271 |
| ElementControl | OpenStatusChannel（发只读 Status 通道给第三方）/Register·UnregisterDependencyToken/AddDependency（**阻塞到全部迁移完成**，L220–224）/RemoveDependency（**立即移除不等待**，L238–244） | L199–253 |
| Status.WatchPowerLevel | hanging get：首调立即返回，后续阻塞到变化（L372–374）——**这就是 opportunistic 观察** | L371–378 |
| Permissions bits | MODIFY_DEPENDENT=0b001 / MODIFY_DEPENDENCY=0b100（L291–294）；实现里用 token 的 **Koid 做 credential id**（[credentials.rs L15–17](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/broker/src/credentials.rs)） | 高 |
| LeaseStatus | UNKNOWN=0→PENDING=1→SATISFIED=2→POWERING_DOWN=3→VACATED=4 | L328–346 |

**注意**：现行 FIDL **没有** assertive/opportunistic 的 DependencyType 枚举，也没有 wake latency 字段——PM-03 §5.2 引的
assertive/opportunistic 二分只剩 `assertive_dependency_token` 命名残留在 SAG 侧；opportunistic 的现行表达=Status 通道观察。
（一期文档引用的 observing 页现已 404，术语在收缩。确定度：高，FIDL 缺席+404 实测。）

---

## §3 broker 实现：Minimum Power Level Policy【确定度：高（源码一手）】

源：[src/power/broker/src/broker.rs](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/broker/src/broker.rs)（8210 行）。

- **算档算法一句话**（L1623–1636 注释+实现）：`required_level = max(所有 enforced claims 的 requires.level)`，
  无 claim → 元素最低档。策略命名 **Minimum Power Level Policy**。
- **租约→声明展开**（`create_lease_and_claim_dependencies` L1653+）：租约 = 根依赖（不进图，[topology.rs L128–132](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/broker/src/topology.rs)）
  + 全部直接/传递依赖 → `filter_out_redundant_dependencies`（同元素对只留最高档，L1640–1660）→ 逐条挂 pending claim。
- **claims 状态机**：active / deactivating；元素掉级会**打断下游 claims**（`deactivate_broken_claims` L206–268，
  沿拓扑走而非沿 claims 走——注释明说防「租约被顺带砸穿」）；被砸 claims 保底不降（"Broken claims on `element_id` itself stay
  enforced" L204–205）。
- **观测**：required/current level 全进 Inspect；trace 事件 `power-broker`/`required_level`（L459）。
- 深意：**broker 不做任何排序优化**——谁先迁移由图遍历决定，深度/延迟不在输入里（呼应 §0-5）。

---

## §4 SAG：wake lease 门市与挂起编排【确定度：高（FIDL+源码一手）】

源：[sdk/fidl/fuchsia.power.system/system.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.power.system/system.fidl)（核心 added=26 已进 SDK，增量在 HEAD）、
[src/power/system-activity-governor/src](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/system-activity-governor/src)。

### 4.1 三元素与依赖图（system.fidl L19–39 图 + L80–130）

| 元素 | 档位 | 边 |
|---|---|---|
| ExecutionState | INACTIVE=0 / SUSPENDING=1 / ACTIVE=2 | SUSPENDING 与 ACTIVE 都依赖 Cpu.ACTIVE |
| ApplicationActivity | INACTIVE / ACTIVE | ACTIVE 依赖 ExecutionState.ACTIVE |
| Cpu | INACTIVE / ACTIVE | （叶子，SAG 是它的 ElementRunner） |

**wake lease 的本体**（sag.rs L388–391）：「A wake lease blocks suspension by requiring the power level of the
Execution State to be at least SUSPENDING」——即 wake lease = 对 ExecutionState.SUSPENDING 档的 broker 租约。
挂起语义=ExecutionState 落 INACTIVE，租约在场就落不下去。

### 4.2 AcquireWakeLease 四代演化（system.fidl，PM-03 §5.3 只有一代）

| 代 | 方法 | 关键字段/语义 | added |
|---|---|---|---|
| 1 | `AcquireWakeLease(name)` | 同步返回 token；**警告 L272–275：阻塞仅在收到无错响应后生效——中断必须等回复才能 ack** | 26 |
| 2 | `AcquireUnmonitoredWakeLease(name)` | 不计时长、超阈不报崩溃（L251–259）——**治理逃逸阀** | HEAD |
| 3 | `AcquireWakeLeaseWithToken(server_token)` | 调用方自带 eventpair，SAG 持 server 半边——为跨进程交接 | HEAD |
| 4 | `AcquireWakeLease2{name, server_token, previous_client_token, unmonitored}` | **原子交接**：SAG 先扣住前租约句柄直到新租约入账，`replaces_lease_id` 写进 Inspect（L293–336） | HEAD |

配一套运行时记账（sag.rs L370–386 `ActiveWakeLease`）：name / type / is_unmonitored / client·server_token_koid /
replaces_lease_id / status / error / created_at(BootInstant) / timer_task。Inspect lazy 节点 `wake_leases` 出
active_count + oldest_active 前 N 条（L459–495）。**治理**：LongLeaseDetector 阈值可配（sag_config），超时经
fuchsia.feedback 报崩溃；崩溃签名两个（sag.rs L121–122）：`fuchsia-no-suspend-in-5-application-activity-cycles`
（该睡不睡）与 `fuchsia-sag-suspend-callback-loop-detected`（挂起回调环，配 `suspend_loop_max_attempts`）。

### 4.3 挂起流（cpu_manager.rs L243–420 一手）

1. 租约图空 → broker 算出 Cpu required=INACTIVE → SAG 的 `update_current_level` 返回 true（L243–263）。
2. `trigger_suspend`：检查 `cpu_element_is_inactive` + `SuspendBlockManager.suspend_allowed()`——
   **SuspendBlockManager 专填「wake lease 已 acquire 但尚未 satisfied」的缝**（cpu_manager.rs L74–79 原文注释）。
3. 调 `fuchsia.hardware.power.suspend/Suspender.Suspend(state_index)`；**调用直到 resume 才返回**。
4. 返回 `WakeReason` → `report_wake_reason` → WakeSourceObservability 归因（§6）。
5. 全程记 SuspendStats（[fuchsia.power.suspend/suspend.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.power.suspend/suspend.fidl)：
   success/fail 计数、last_failed_error、last/total_time_in_suspend、**last_time_in_suspend_operations（进出的操作开销单独记）**，
   Stats.Watch hanging get）。
6. **BootControl.SetBootComplete**（system.fidl L418–423）：「SAG will not suspend the hardware platform until this
   method is called at least once per boot」——boot 门闩（PM-03 §5.3 文档级结论的 FIDL 原文）。
7. SuspendBlocker（system.fidl L173–210）：BeforeSuspend（ExecutionState=SUSPENDING 期间，要否决挂起必须**先**拿
   wake lease 再回复）/ AfterResume；明文死锁警告：期间禁调 TakeApplicationActivityLease（会等 ExecutionState 到
   非 INACTIVE 档，自锁），AcquireWakeLease 安全。F29 定稿：老 ActivityGovernorListener 接口已删（见 §7）。

### 4.4 CpuElementManager 的窗口期设计（system.fidl L441–449）

`AddExecutionStateDependency`：**ExecutionState 元素一旦创建，后续调用返回 BAD_STATE**——执行态的依赖边必须在
SAG 初始化前由板级配置加死。启示：系统级元素的边是「启动期一次性写死」，不开放运行时动态加。

---

## §5 挂起协议与驱动侧【确定度：高（FIDL/源码/官方文档）】

### 5.1 fuchsia.hardware.power.suspend/Suspender（[suspend.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.hardware.power.suspend/suspend.fidl) 一手）

| 方法 | 字段 | 语义 |
|---|---|---|
| GetSuspendStates | `suspend_states[]{resume_latency}` | **挂起档位表**：≥1 档（首档=suspend-to-idle）、按 resume_latency **降序**、同一 build 恒定返回 |
| Suspend | `state_index` → `{reason: WakeReason, suspend_duration, suspend_overhead}` | 挂起中被打断不算错，但 suspend_duration 必须为 None |
| WakeReason | `wake_vectors[]`（每项=**中断 koid**）、`soft_wake_vectors[]`、`wake_vectors_type=KOID`、`wake_vectors_overflow` | 内核归因到中断对象级 |
| ForceLowPowerMode | TODO 考虑移除（b/454722784） | 救火阀，正在退场 |

谁实现 Suspender：SAG 侧只连 `SuspendService`（由 parent realm 提供，`use_suspender` 配置门控，[shard.cml L64–73]
（https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/system-activity-governor/meta/system-activity-governor.shard.cml））；
RFC-0230 原文把入口机制留白（"direct syscall ... or routed through a driver or a component or HAL"，[RFC-0230]
（https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0230_suspend_to_idle））——**板级可换的实现缝**，内核四原语分工见 PM-03 §5.4 不重写。
挂起恢复的看门狗配置：`suspend_resume_stuck_warning_timeout`、`reboot_on_stalled_suspend_blocker`（卡死直接重启）。

### 5.2 dfv2 驱动的三条申报路径（[device-power 官方文档](https://fuchsia.dev/fuchsia-src/development/drivers/concepts/device_driver_model/device-power)）

| 路径 | manifest 字段 | 机制 |
|---|---|---|
| 运行时托管 dispatcher（推荐） | `power_managed_dispatchers_enabled: "true"`（或 shard） | driver host 代跑 ElementRunner：挂起=暂停 power-managed dispatcher、任务进 sleep queue、等在跑回调收尾，再在 always-on dispatcher 上回调 `SystemSuspend/SystemResume`；唤醒=先 resume 回调再放行，**wake-vector 任务插队** |
| 驱动自持挂起 | `suspend_enabled: "true"` | C++ 混入 `fdf_power::Suspendable`、Start 里 `InitializeSuspend()`、自己实现 Suspend/Resume + 提供 ElementRunner；两者互斥，前者优先 |
| 自定义元素 | 无（代码态） | 直调 Topology.AddElement / [sdk/lib/driver/power/cpp](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/lib/driver/power/cpp)；官方例程 [examples/drivers/power/cpp/power_driver.cc](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/examples/drivers/power/cpp/power_driver.cc) 即 76 行的 Suspendable 骨架 |

**driver_manager 自动拓扑**（[power/power_manager.cc](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/devices/bin/driver_manager/power/power_manager.cc)、
[all_drivers_element.h](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/devices/bin/driver_manager/power/all_drivers_element.h)，版权 2026）：
`suspend_enabled` 时每绑定节点建二元元素（0=挂起、1=活跃），依赖=祖先链绑定元素+共享元素（CPU、storage）；
AllDriversElement 收编**全部叶子驱动**——`OnNodeBound` 加依赖并持 startup lease 直到依赖入图，`OnBootupComplete` 后做
初始扫描；storage token 可等驱动上送（`wait_for_suspending_token`）。**即：设备的依赖边不是驱动申报的，是 devhost 按拓扑自动织的。**

### 5.3 板级电源配置 FIDL（[fuchsia.hardware.power/config.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.hardware.power/config.fidl) 一手）——P3a 的直接对表物

| 类型 | 字段 | 备注 |
|---|---|---|
| Transition | `target_level` + **`latency_us`** | **唤醒/迁移延迟只在此层申报**（L27–33），broker 不消费 |
| PowerLevel | level / name / transitions[] | 最多 128 档、63 字节名 |
| PowerElement | name + levels[] | |
| PowerDependency | child / parent: ParentElement / level_deps[{child_level, parent_level}] | ParentElement 三选一：`sag`(EXECUTION_STATE/APPLICATION_ACTIVITY) / `instance_name`（从 `/svc/fuchsia.hardware.power.PowerTokenProvider/{name}` 拿 token）/ `cpu_control` |
| ComponentPowerConfiguration | power_elements ≤32 | 板驱动经 metadata 下发给子设备；驱动侧用 [element-description-builder.h](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/lib/driver/power/cpp/element-description-builder.h) 把它翻成 broker ElementSchema |

并存的老电压域 API（[power.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.hardware.power/power.fidl)，2019）：
RegisterPowerDomain(min/max uV)/RequestVoltage→actual_voltage/PMIC 寄存器读写——**新旧两套硬件电源 API 同库共存**。

### 5.4 驱动 wake lease 工具库（[sdk/lib/driver/power/cpp/wake-lease.h](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/lib/driver/power/cpp/wake-lease.h) 一手）

| 类 | 语义 |
|---|---|
| ManualWakeLease | **懒取**：平时不占租约，监听系统将挂起（实现 SuspendBlocker）才去 AcquireWakeLease——省开销 |
| TimeoutWakeLease | `HandleInterrupt`/`AcquireWakeLease(timeout)`：中断驱动的限时租约，超时自动放——**BerryOS P1 delay 档租约的现成形状** |
| WakeLeaseProvider | RAII，同一时刻至多一个租约存活 |

---

## §6 电池、输入与定时器：租约接力棒的三个实例【确定度：高（FIDL 一手）】

- **电池**（[fuchsia.power.battery/battery.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.power.battery/battery.fidl)）：
  BatteryInfo 15 字段（level_percent f32、level_status 含 LOW/CRITICAL、time_remaining union、remaining_charge_uah、
  full_capacity_uah、present/average_charging_current_ua、temperature_mc、timestamp、battery_spec 设计容量）
  【勘误 2026-11-07：原写 16 字段，经 PM-12 审校对 main battery.fidl 复核实为 15 字段（ordinal 1–15），此处从正】；
  `BatteryInfoWatcher.OnChangeBatteryInfo` 携带 **optional wake_lease**——注释原文「intended for short-term 'baton passing'
  and should be released quickly」，charge_source 变化时提供（L161–173）。F29：Battery Manager 复活，**充电期间阻塞挂起**，
  batteryutil CLI（[F29 notes](https://fuchsia.dev/whats-new/release-notes/f29)）。
- **输入**（[fuchsia.input.report/report.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.input.report/report.fidl) L46–48）：
  InputReport 第 9 字段 `wake_lease zx.Handle:EVENTPAIR`（added=32）「Optional wake lease for power baton passing」——
  **每个触摸/键鼠事件可携带唤醒租约**，事件分类=描述符类型（touch/mouse/keyboard/consumer_control）。F30：input pipeline
  全接管 wake（[F30 notes](https://fuchsia.dev/whats-new/release-notes/f30)）。
- **定时器**（[fuchsia.hardware.hrtimer](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.hardware.hrtimer/hrtimer.fidl) L246–295）：
  `StartAndWait` **必填** `setup_keep_alive`（设置期租约）、到期**返回** `expiration_keep_alive`（到期时刻租约）——两段租约协议，
  注释明文交接纪律（「client must either keep this token ... or get its own lease before it drops it」）。F28 加 UTC 唤醒闹钟
  SetAndWaitUtc（Starnix 容器需求）。
- 蓝牙 F27：ConnectionReceiver2 带 wake lease 握手与流控。

**模式提炼**：Fuchsia 把「跨进程唤醒责任交接」统一成**协议字段内嵌租约句柄**（battery/input 返回值、timer 双段、
WakeLease2 previous_client_token）——不是文档纪律，是 ABI 位。

---

## §7 演化与坑：API 迁移史【确定度：高（FIDL 注记+F27–F30 发行注记）】

| 时期 | 变更 | 教训 |
|---|---|---|
| 2023 起草 | broker.fidl 全库 added=HEAD，自称 prototype | 电源协议从 prototype 起步，但**句柄语义（eventpair drop=释放）从未变过** |
| ~F26 | Lessor（元素下租约）→ **弃用**，改 Topology.Lease（broker.fidl L104/L296） | 两层 API（先拿元素再拿租约）多余；租约独立入口即可 |
| F29（2026-01） | **LevelControl 从系统与 ElementSchema 移除**，全员迁 ElementRunner；claims 顺序竞态修复；ActivityGovernorListener 删除→SuspendBlocker 定稿；`fuchsia.power.systemmode` 正式弃用；UFS 弃 wake-on-request 改标准 suspend/resume 钩子 | (a) 双控制通道并存一年才收敛；(b) **「唤醒时顺手拉起依赖」的隐式做法被明判反模式**；(c) 系统电源模式单独小库活不长，收进框架 |
| HEAD | AcquireWakeLease→WithToken→WakeLease2 三连；LeaseControl 标 TODO 换 eventpair（b/339474151） | 同步调用式租约的 ack 竞态反复打补丁，终局=调用方持句柄+原子交接 |
| 恒定 | 老电压域 power.fidl（2019）与 config.fidl（2023）与 broker（2023+）三套同库 | 用户态驱动栈的电源 API 也是分层淤积，靠「新功能只进新层」控制腐化 |

**它没踩但险些踩的坑**：RFC-0230 曾考虑照抄 Linux 回调序模型被否（PM-03 §5.4 已引）；wake latency 进图未做（§0-5）。

---

## §8 BerryOS 启示：P1–P10 对照与抄/改/不抄【确定度：高（对照 PM-00 §5.1 逐条）】

| PM-00 位 | Fuchsia 对应物 | 裁决 |
|---|---|---|
| **P1 wake lease 票据** | LeaseToken eventpair（末句柄存活即不挂起）+ AcquireWakeLease2 原子交接 + TimeoutWakeLease 限时档 + Unmonitored 逃逸阀 | **抄**：票据=能力句柄可复制、断线回收（element_control 通道同语义）；**改**：day-one 就带 `{kind: block\|delay, reason:u8, lease_ns}` 票面（Fuchsia 只有 name≤64，归因弱——我们 reason:u8 对齐 #58 词汇表）；delay 档抄 TimeoutWakeLease 形状；预留 unmonitored 位（长持豁免不计罚） |
| **P2 hint 会话** | **无对应**——broker 明言不做策略；cpu-manager 另立门户 | **不抄它的缺**：`{target_ns, actual_ns}` 会话 BerryOS 自有（#53），Fuchsia 反证「仲裁器与策略分离」可行但代价是**协调层看不到性能意图**——berry-power 合一时保留 hint 面，别学它一分为二没人管总账 |
| **P3/P3a 电源域 lease + manifest 申报** | ElementSchema（拓扑）+ config.fidl `Transition.latency_us`（板级）+ driver_manager 每节点二元元素 | **抄**：二元档起步（BinaryPowerLevel）、`requires_level_by_preference` 降序表（跨版本兼容神来之笔）、`remove_with_required_element`、元素控制通道断开=退场；**改**：latency 必须进 berry-manifest 申报位且**仲裁器消费**（Fuchsia 的 latency 死在板级层没人吃——别重蹈）；设备依赖边 v1 学 driver_manager 自动织（祖先链+CPU），不要求驱动手写 |
| **P4 网络元数据** | 文档层的下载例（PM-03 §5.2 引），无协议位 | **不抄**：无实体可抄；P4 照 PM-05 §4.2 原案 |
| **P5 定时器三档+slack** | hrtimer 双段 keep-alive 租约 + UTC 闹钟 SetAndWaitUtc | **抄形状**：定时器到期「返回」租约句柄=到期责任交接进 ABI；berry-sys 定时器三档中 window/precise 档的到期回调携带 lease 句柄位；slack 语义 Fuchsia 无对应（我们超集） |
| **P6 传感器 batching** | 无（sensor FIDL 无 batching/lease） | **不抄**：照 PM-04 §8.3 |
| **P7 交互事件分类+flag** | InputReport.wake_lease（事件级租约）+ 描述符分类 | **抄**：交互事件结构体带 `wake_lease` 句柄位 + `INDIRECT` 类 flag（触摸非直接唤醒场景）；F30 input pipeline 独占 wake 责任的结构也值得学——**输入服务是唤醒链唯一持租约者** |
| **P8 计量标注** | BatteryInfo（实测）+ SuspendStats（挂起记账）；measured/modeled 无区分位 | **改**：BatteryInfo 字段集直接抄（15 字段含 design/full 容量、温度、双电流、timestamp，勘误同 §6）；但加 `measured/modeled` 标注位（我们独有，Fuchsia 没做模型账） |
| **P9 挂起档位显式位** | `GetSuspendStates[]{resume_latency}` 降序表 + `Suspend(state_index)` + duration/overhead 双计时 | **整段抄**：档位表协议形状、按 resume_latency 排序约定、「被打断不算错但 duration=None」语义、**suspend_operations 开销与驻留时长分列**——P9 的「s2idle vs deep 显式位」就长这个样；boot-complete 门闩照抄（SetBootComplete 原文语义） |
| **P10 内核 wakeup 记账面** | WakeReason(koid[]) + WakeSourceReport 七字段 + InterruptAttributor(koid→驱动名) + 长租约崩溃报告 | **整段抄字段集**：per-source `{id, name, first_signal_ts, last_signal_ts, last_ack_ts, count, flags}`——PM-00 刀 1 的 schema 直接定这个；**加**：持有者归因（uid/服务名——Fuchsia 只有 koid→驱动名，应用侧无归因，弱于我们 P10 设计）；断言抄两个崩溃签名思路（该睡不睡 N 周期=测试失败） |

### 8.1 逐协议位可执行清单（v1 窗口就能落的三件）

1. **`PM_WAKE_LEASE` 票据 ABI**（P1）：句柄语义照 LeaseToken 四权利（TRANSFER/DUPLICATE/WAIT/SIGNAL_PEER 等价的
   BerryOS 能力位）+ 满足信号位（对齐 ZX_USER_SIGNAL_0 的「SATISFIED 打信号」用法）；name≤64 + reason:u8。
   **验证断言**：复制句柄后关一半，租约仍活；全关后租约图空触发一次挂起尝试；断线通道=元素退场+token 全注销。
2. **挂起档位表**（P9）：`pm_suspend_states[]{resume_latency_ns}` 降序、首档=浅睡、`suspend(state_idx)` 返回
   `{wake_src_ids[], resident_ns, overhead_ns}`；被打断 resident=缺省。**验证断言**：QEMU 下档表恒定返回、
   wake_src 归因守恒（Σ 记账=唤醒总数，PM-00 刀 1 断言原样接上）。
3. **SuspendBlockManager 同款缝**（挂起阻塞器）：acquire 返回≠satisfied，挂起判定必须先过阻塞器——
   **验证断言**：acquire 与 satisfied 之间注入挂起尝试，必须 NotAllowed（Fuchsia 用一整型类填的缝，BerryOS 同构实现）。

### 8.2 架构级确认（给 PM-00 四层模型添证）

- 仲裁器零策略、最小档策略一行公式（§3）→ BerryOS berry-power「拓扑仲裁」与「策略服务」**分进程不分手感**：
  仲裁面照 Fuchsia，策略面（频率帽/thermal）照 cpu-manager 的「EM 注册进内核+syscall 通知」形状——**EM 表进内核这件事
  Fuchsia 已替我们验证**（cpu-manager README 原文），直接支持 PM-03 §8.1「EM 数据=板级申报、决策=内核」的归属表。
- 系统级元素（对应 ExecutionState）的依赖边**启动期一次性写死**（§4.4 BAD_STATE 语义）——berry-power 的系统态元素
  同样不开放运行时加边，防运行时拓扑漂移。

---

## §9 引用源列表

**Fuchsia 源码（gitiles main，2026-11-07 直读）**：
[fuchsia.power.broker/broker.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.power.broker/broker.fidl) ·
[fuchsia.power.system/system.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.power.system/system.fidl) ·
[fuchsia.power.suspend/suspend.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.power.suspend/suspend.fidl) ·
[fuchsia.hardware.power.suspend/suspend.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.hardware.power.suspend/suspend.fidl) ·
[fuchsia.hardware.power/config.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.hardware.power/config.fidl) ·
[fuchsia.hardware.power/power.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.hardware.power/power.fidl) ·
[fuchsia.power.battery/battery.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.power.battery/battery.fidl) ·
[fuchsia.input.report/report.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.input.report/report.fidl) ·
[fuchsia.hardware.hrtimer/hrtimer.fidl](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/fidl/fuchsia.hardware.hrtimer/hrtimer.fidl) ·
[src/power/broker/src/broker.rs](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/broker/src/broker.rs) ·
[src/power/broker/src/topology.rs](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/broker/src/topology.rs) ·
[src/power/broker/src/credentials.rs](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/broker/src/credentials.rs) ·
[SAG src/（system_activity_governor.rs / cpu_manager.rs / power_observability.rs / main.rs）](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/system-activity-governor/src) ·
[SAG shard.cml](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/system-activity-governor/meta/system-activity-governor.shard.cml) ·
[src/power/cpu-manager/README.md](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/power/cpu-manager/README.md) ·
[driver_manager/power/（power_manager.cc / all_drivers_element.h）](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/src/devices/bin/driver_manager/power) ·
[sdk/lib/driver/power/cpp/（wake-lease.h / element-description-builder.h）](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/sdk/lib/driver/power/cpp) ·
[examples/drivers/power/cpp/power_driver.cc](https://fuchsia.googlesource.com/fuchsia/+/refs/heads/main/examples/drivers/power/cpp/power_driver.cc)

**官方文档/RFC/发行注记**：
[Power framework intro](https://fuchsia.dev/fuchsia-src/concepts/power/intro) ·
[Device power management（dfv2）](https://fuchsia.dev/fuchsia-src/development/drivers/concepts/device_driver_model/device-power) ·
[RFC-0230: Suspend-To-Idle](https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0230_suspend_to_idle) ·
[F27 notes](https://fuchsia.dev/whats-new/release-notes/f27) · [F28 notes](https://fuchsia.dev/whats-new/release-notes/f28) ·
[F29 notes](https://fuchsia.dev/whats-new/release-notes/f29) · [F30 notes](https://fuchsia.dev/whats-new/release-notes/f30)

**仓内对照**：`research/PM-00-全系统省电架构综合裁决-v1.0.md` §5.1（P1–P10）/§2（四层模型）·
`research/PM-03-Linux电源机制与微内核归属-v1.0.md` §5（文档级 Fuchsia）/§8.2（可执行建议）· `research/PM-04/05`（P4/P6/P7 票面出处）
