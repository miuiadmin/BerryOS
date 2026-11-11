# 调度器 API 参考

> 版本：v1.3（2026-11-11 升版：W3-Q16 HDR 形状与分位数学外迁随迁——`bucket_of`/`bucket_upper`/`Hdr`/`CpuHdr` 等 12 项转 re-export 单源 `berry_hdrface`，hdr.rs 20→8 机械壳，计数 146→134；v1.2：W3-Q13 `intent_valid` 外迁随迁 147→146）｜ 对象：`sched/` crate（`berry-sched`，12 文件）｜ 审计口径：**134 个 `pub fn`**（含 `pub const fn` / `pub unsafe fn`，逐文件机提 + 人工核对 + sched/ 12 文件全量复核；初估 132 为低估）
> 事实源 = 源码 doc 注释首行（提取时点 2026-11-08，2026-11-11 增量对拍至 cf9e253/d1b25ca）；本文是影抄不是立法——签名冲突时以 `sched/src/` 为准并回改本文。

## 使用方法

- **查 API**：按下方八个模块分组浏览；每条目格式为签名 + `> 模块 | 一句话描述`（描述提自该函数 doc 注释首行，`[`链接`]` 已清洗）。
- **调用路径**：所有条目均可经 crate 根 `berry_sched::` 直达（`sched/src/lib.rs` 的 `pub use` 扁平再导出）；未导出者（如 `Runqueue::link_back`）标注 `pub(crate)`。
- **unsafe 条目**：签名标 `unsafe` 者携带 Safety 契约（IRQ 上下文/锁窗/C-ABI 槽），调用前读源码对应 `# Safety` 节。
- **trait 面**：`SchedClass` 的 `enqueue`/`dequeue`/`pick_next`/`tick`/`priority` 是 trait 方法（签名冻结，一字不改），非自由函数——归入「一、调度核心」节末表。
- **维护合同**：改 `sched/src/` 任何 `pub fn` 签名或删除/新增 → 同步本文对应条目并升版本号（文件名 + 抬头双标），`docs/索引.md` 标签列同步。

---

## 一、调度核心（class.rs + sched.rs）

EEVDF+EDF 混合调度核：spawn 家族、freeze/thaw、抢占控制、SLEEPQ 睡眠/唤醒、tickless idle。

### `preempt_disable()`
> sched | 开本核禁切窗（可嵌套；须与 `preempt_enable` 严格配平）。

### `preempt_enable()`
> sched | 关本核禁切窗一层（嵌套弹一层；归零点 = 可切点）。

### `preempt_count() -> u64`
> sched | 读本核禁切计数（诊断/检查点探峰自查面）。

### `preempt_checkpoint()`
> sched | 抢占检查点（无 doc——实义：IRQ 保存 + 计数为零时 `try_preempt`，g3-1d 真探形）。

### `preempt_checkpoint_hits() -> u64`
> sched | g3-1d：检查点真探形命中数（boot 起累计；fa ktest 尾 `ckpt=` 字段打印）。

### `print_window_enter()`
> sched | G3-1c-1：PRINT_LOCK 窗 preempt 包络对（enter/exit 槽）。

### `print_window_exit()`
> sched | 退出半（文档见 `print_window_enter`——DAIF 分诊论证在彼）。

### `set_klock_ops(ops: KlockOps)`
> sched | 安装锁原语槽（kernel main，ktest 前恰一次）。

### `svc_window(class: u8, enter: bool)` `unsafe extern "C"`
> sched | SVC 持锁窗 enter/exit 槽（arch 侧经槽调；按位集升降物理锁）。

### `init()`
> sched | 初始化调度器：`current = &IDLE_TASK`（须在 `spawn`/`yield_now` 前）。

### `topo_smp_online()`
> sched | S-3b bring-up 成功后真化拓扑：`nr_cpus=2` + 双核容量。

### `spawn(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8])`
> sched | 生成一个内核线程：分配 16KiB 栈 + 接线 + 入 runqueue（无地址空间）。

### `spawn_as_spread(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8], ttbr0: usize, asid: u16)`
> sched | 同 `spawn`，但挂地址空间（2b-2）：`ttbr0` 页表 PA + `asid`（0/0 = 无 AS）。

### `spawn_as(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8], ttbr0: usize, asid: u16)`
> sched | 同 `spawn_as_spread`（无 doc；落 `new_kernel_with_as` + place 入环）。

### `spawn_with(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8], prio: i32)`
> sched | 同 `spawn`，但带基础优先级（4-3b PI 验收：高优 client 的捐赠源）。

### `spawn_with_tag(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8], prio: i32, tag: usize)`
> sched | 同 `spawn_with`，但挂内核侧私有 handle 表 tag（4-5 前置：常驻内核服务）。

### `spawn_intent(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8], intent: u8)`
> sched | 同 `spawn`，但带声明档（4-3c intent 验收：借入/拒借的档位差源）。

### `spawn_user(entry_va: usize, arg: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize)`
> sched | 生成一个用户线程（2b-4 首个 EL0）：内核栈 + `new_user` 上下文。

### `try_prepare_user(entry_va: usize, arg: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize, pinned: usize, thread_key: usize, spread: bool) -> Option<usize>`
> sched | 同 `spawn_user` 但 try 化（3b3 审查 H2）：栈/TCB 任一 OOM 返 None。

### `enqueue_prepared_user(task_ptr: usize)` `unsafe`
> sched | 把 `try_prepare_user` 产出的已初始化任务入环（调用侧须完成一切登记）。

### `try_spawn_user(entry_va: usize, arg: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize, pinned: usize, thread_key: usize, spread: bool) -> Option<usize>`
> sched | try 面 spawn_user 全流程（无 doc；prepare + enqueue 合一）。

### `current_tag() -> usize`
> sched | 当前任务的进程标签（3-4a per-process handle 表派发用）。

### `current_pinned() -> usize`
> sched | 当前任务钉住的 Process 对象键（C-4b `process_terminate` 自杀判别用）。

### `current_thread_key() -> usize`
> sched | 当前任务的 thread_key（S3-b 线程级 freeze 自冻判别用）。

### `current_deadline_state() -> (u64, u64)`
> sched | 1.1/v1.2：当前任务的运行时 deadline 状态（sched_getattr 自查面消费）。

### `freeze_precheck_by_pinned(pinned: usize) -> bool`
> sched | 1.1/S3-a：冻结前置谓词——按钉住键扫描，任一任务命中即返 false。

### `freeze_by_pinned(pinned: usize) -> usize`
> sched | 1.1/S3-a：按钉住键冻结该进程全部任务（SLEEPQ + 双 RQ 扫描）。

### `thaw_by_pinned(pinned: usize)`
> sched | 1.1/S3-a：按钉住键解冻（SuspendToken Drop 路径调用）。

### `freeze_precheck_by_thread(thread_key: usize) -> bool`
> sched | 1.1/S3-b：按 thread_key 冻结前置谓词（单线程版）。

### `freeze_by_thread(thread_key: usize) -> usize`
> sched | 1.1/S3-b：按 thread_key 冻结单任务（返回冻结数 0/1）。

### `thaw_by_thread(thread_key: usize)`
> sched | 1.1/S3-b：按 thread_key 解冻。

### `kill_all_tasks_by_pinned(pinned: usize, code: i32) -> Option<(&'static [u8], usize)>` `unsafe`
> sched | C-4b `process_terminate`/T-1 `process_exit`：按钉住键杀全任务（返回名 + 数）。

### `intent_snapshot_current(now_ns: u64) -> Option<(MsgIntent, SnapOut)>`
> sched | 4-3c：send 侧 intent 快照（syscall 层调用；语义在 `crate::intent`）。

### `intent_settle_current(m: MsgIntent, opt_out: bool, now_ns: u64) -> Option<SettleOut>`
> sched | 4-3c：recv 侧 intent 结算（旧面——W3-Q6 前 9 个调用点零改动；实现在 `intent_settle_current_ext`）。

### `intent_settle_current_ext(m: MsgIntent, opt_out: bool, now_ns: u64) -> Option<(SettleOut, u8, u8)>`
> sched | 4-3c：recv 侧 intent 结算 slowpath 本体（语义在 `crate::intent`）；CURRENT 无效 → `None`（消息照常投递，不结算不记遥测）；W3-Q6 起结算后附 `(declared, effective)` 双档读出——`intent_settle_note` 的档位直方/通胀计数源（单源 `berry_itally`）。

### `yield_now()`
> sched | 协作让出：切到 `pick_next` 选出的下一个任务（若有且非自己）。

### `yield_now_keep_irq()`
> sched | SVC 0x3F 专用协作让出：同舞蹈但出口保持 IRQ 屏蔽。

### `exit_current() -> !`
> sched | 当前任务退出：出 runqueue + 切到下一个（或 idle）。永不返回。

### `exit_current_with(code: i32) -> !`
> sched | 当前任务带退出码退出（Plan B v0.5：`process_exit`/`thread_exit` 的底座）。

### `user_fault_exit() -> !` `unsafe extern "C"`
> sched | 当前用户任务因致命故障被杀（HAL `exception_sync_el0` 经槽调；只死自己）。

### `set_exit_hook(f: unsafe extern "C" fn(pinned: usize, thread_key: usize, code: i32))`
> sched | 注册任务退出钩子（`内核` crate 语义层 boot 期调用）。

### `reap_drain()`
> sched | 回收退出任务的 Task Box + 内核栈（16KiB/个；idle_loop 稳态与 ktest 尾调）。

### `switches() -> u64`
> sched | 总切换数（含 park 事件；金丝雀 `switches=` 行口径）。

### `switches_tt() -> u64`
> sched | 任务↔任务切换数（遥测环分母——G1.5 断言比较面）。

### `set_switch_hook(hook: Option<SwitchHook>)`
> sched | 注册/注销切换钩子。仅 boot 期（IRQ 屏蔽、首个切换发生前）调用。

### `set_wake_hook(hook: Option<WakeHook>)`
> sched | 注册/注销唤醒钩子。仅 boot 期（IRQ 屏蔽、首个切换发生前）调用。

### `wake_stamp_take() -> u64`
> sched | 取走当前任务的唤醒时刻戳（G3-4 bench-lat run 侧读点；无戳 → 0）。

### `set_sleepq_hook(hook: Option<SleepqHook>)`
> sched | 注册/注销 SLEEPQ 变更钩子。仅 boot 期（IRQ 屏蔽、首个切换前）调用。

### `set_lb_match(f: Option<fn(key: usize) -> bool>)`
> sched | 注册 LB 子任务判别（仅 boot 期 lb gate 块调用；键 = `thread_key`）。

### `lb_stats() -> (u64, u64, u64)`
> sched | LB 统计快照（`(c0, c1, st1)`）——lb_master 在 klock 包络内调。

### `sq_lock() -> bool`
> sched | 取 SLEEPQ 锁（含持有位记账）；pub 供 kernel 侧 fastpath 义务④跨窗。

### `sq_unlock(phys: bool)`
> sched | 放 SLEEPQ 锁（先清位后放——位与锁恒同窗）。

### `block_current(obj: usize, signals: u64, deadline_vct: u64) -> WakeReason`
> sched | 受控睡眠（4-0）：当前任务挂 SLEEPQ 等「`obj` 上与 `signals` 相交的信号/超时」。

### `wake_matching(obj: usize, set_signals: u64) -> usize`
> sched | 条件方唤醒（4-0）：摘除所有「`wait_obj == obj` 且请求位相交」的睡眠者。

### `wake_matching_locked(obj: usize, set_signals: u64) -> usize`
> sched | `wake_matching` 的持窗版（IRQ 生产者消费；须已持 SLEEPQ 锁）。

### `find_single_waiter(obj: usize, signals: u64) -> Option<NonNull<Task>>`
> sched | 4-4 fastpath 检查链的 SLEEPQ 侧：找睡单一对象 `obj` 等相交信号的唯一睡眠者。

### `find_single_waiter_locked(obj: usize, signals: u64) -> Option<NonNull<Task>>`
> sched | `find_single_waiter` 的持窗版：须已持 SLEEPQ 锁且整段窗内调用。

### `fast_wake_locked(t: NonNull<Task>)` `unsafe`
> sched | 4-4 fastpath 定点唤醒：唤醒同窗 `find_single_waiter_locked` 找到的唯一睡眠者。

### `try_preempt() -> bool`
> sched | 4-4 直切点（§7「直切投递」，seL4 direct switch 的 v1 形态）。

### `for_each_sleeper(mut f: impl FnMut(&mut Task))`
> sched | 遍历全局 SLEEPQ 上所有睡眠者（head → tail）；契约：IRQ 屏蔽窗内。

### `refresh_effective(t: &mut Task)`
> sched | 重算 `t` 的有效优先级（4-3b 统一合成公式）。

### `sleepq_insertable(t: &Task) -> bool`
> sched | 入队前置判据（ktest RING3 直测用；生产执法 = `SleepQueue::insert_sorted`）。

### `sleepq_insert(t: &mut Task)`
> sched | 把 `t` 按 deadline 升序压入全局 SLEEPQ（ktest 手搭睡眠者专用）。

### `sleepq_unlink(t: &mut Task)`
> sched | 把 `t` 摘出全局 SLEEPQ（ktest 清理专用；不幂等）。

### `user_kills() -> u64`
> sched | 用户任务致死计数只读访问（G1.5 watchdog 断言用；A12 原子化）。

### `last_killed() -> &'static [u8]`
> sched | 最近被杀用户任务名只读访问（watchdog 断言「杀对了人」用）。

### `idle_loop() -> !`
> sched | idle 主循环（tickless）：runqueue 空且无睡眠者时屏蔽 CNTV + `wfi`。

### `on_irq_tick(_tf: &mut hal::trap::TrapFrame)` `unsafe extern "C"`
> sched | IRQ tick 槽（HAL `exception_irq` 经函数指针调）：expire 超时/片记账/pick 重调度。

### `on_sgi_resched(_tf: &mut hal::trap::TrapFrame)` `unsafe extern "C"`
> sched | SGI 重调度槽（副核经 eret 回 park 环承接；不进 tick 语义面）。

### `park_recheck()` `unsafe`
> sched | S-3a 副核 park 重查——自 `on_irq_tick` 的 cur==null 形态承接派发。

### `nearest_deadline() -> Option<u64>`
> sched | SLEEPQ 最近截止时刻（S-3a tickless park 武装面——子案 §9.5 步 A）。

### `yield_slice_return(&self, t: &mut Task, rq: &Runqueue)`（`Fair` 固有方法）
> class | yield 片归还（moss `yield_slice` 语义的 BerryOS 形态）——让出者不得带片优势。

### `pick_next_filtered(&self, rq: &Runqueue, _topo: &Topo, my_cur: *mut Task) -> Option<&mut Task>`（`Fair` 固有方法）
> class | 过滤版出选（S-2b inherent 方法——trait 冻结不动）；带内无 eligible 时取最小键。

**`SchedClass` trait 方法面**（class.rs，签名冻结——一字不改）：

| 方法 | 签名 | 语义 |
|------|------|------|
| `enqueue` | `fn enqueue(&self, t: &mut Task, rq: &mut Runqueue)` | 入环（EEVDF：vruntime 抬到均值的 Lag 补偿） |
| `dequeue` | `fn dequeue(&self, t: &mut Task, rq: &mut Runqueue)` | 出环 |
| `pick_next` | `fn pick_next(&self, rq: &Runqueue, topo: &Topo) -> Option<&mut Task>` | 出选（虚拟截止期最小者；`&Runqueue` 下经裸指针返回 `&mut Task`） |
| `tick` | `fn tick(&self, t: &mut Task, rq: &mut Runqueue)` | 片记账（CNTVCT 周期递减；耗尽置重调度） |
| `priority` | `fn priority(&self) -> i32` | 类优先级（EDF = `PRIO_DEADLINE`） |

---

## 二、策略服务（policy.rs）

0x83 `berry_policy_bind` 六 op 的写侧底座：active/shadow 双轨道 + gen 换表 + divergence 对账。

### `bind(tbl: &'static [Entry])`
> policy | 绑定表（v1 仅内核侧 ktest/boot 期调用；v3 由 `policy_bind` 经共享环换表）。

### `bind_gen(tbl: &[Entry], gen: u64) -> bool`
> policy | 带世代的换表 + 陈旧拒收（v3 `policy_bind` 换表协议的写侧底座）。

### `bind_gen_shadow(tbl: &[Entry], gen: u64) -> bool`
> policy | 绑 shadow 表（COMMIT + SHADOW flag 语义——双轨道写侧）：校验 + 发布。

### `gen() -> u64`
> policy | 当前活跃快照的世代数（读点观察用）：未绑 → 0。

### `shadow_gen() -> u64`
> policy | shadow 侧 gen 观察（双轨道遥测/对账）：从未绑定 → 0。

### `unbind()`
> policy | 解绑：翻转 `ACTIVE` 回 `SENTINEL_EMPTY` 哨兵（读路径回落）。

### `bound() -> bool`
> policy | 接口是否在位（测试观察用）：已发布快照（含空表）→ true。

### `apply(task: &mut Task)`
> policy | 决策路径挂钩：pick_next 出选后调用；查到指令 → 写 TCB 表示法字段。

### `placement_hint(task_key: u64) -> u8`
> policy | 该 key 当前绑定的放置偏置（动作空间①：`调点调度-v1.3` §6 placement）。

### `promote() -> bool`
> policy | shadow → enforced 原子转正（PROMOTE）：读当前 shadow 活槽 → 一次 ~4KiB 拷贝换轨。

### `demote()`
> policy | enforced → shadow 降级（DEMOTE）：单条 Release store 把 `ACTIVE` 打回 shadow 槽。

### `reject_stats() -> RejectStats`
> policy | 读拒收入账四计数（Relaxed 快照——纯统计，无重置面）。

### `divergence_count() -> u64`
> policy | 对账期内决策不一致次数（shadow 会做的 ≠ active 做了的：weight 或 EPP）。

### `shadow_eval_count() -> u64`
> policy | 对账期内 shadow 决策评估次数（divergence 率的分母）。

### `divergence_reset()`
> policy | 清零对账计数（两个计数器同清——新对账期从零起算）。

---

## 三、层级预算（band.rs）

intent 带级 CPU 包络（quota/period）：限流只叠不加、翻期事件驱动。

### `configure(band: u8, quota: u64, period: u64, now: u64) -> bool`
> band | 配置某带包络（启动配置面——§7「数字进启动配置」；quota/period 均正才受理）。

### `band_on(band: u8) -> bool`
> band | 该带包络是否在执法（quota/period 均正且带在表内）。

### `state(band: u8) -> Option<BandBudget>`
> band | 槽快照拷出（ktest / telemetry 观测面；越界 → None）。

### `consume(band: u8, cycles: u64) -> bool`
> band | 微记账：把 `cycles` 周期数记到 `band` 本期账上（包络关/带越界 → false）。

### `period_check(band: u8, now: u64) -> bool`
> band | 事件驱动周期翻转：`now ≥ deadline` 即翻期（tick / pick_next 顺带调）。

### `throttle_all(band: u8, rq: &Runqueue) -> usize`
> band | 带级限流：扫环上带成员（intent_effective == band 的 fair 任务）置 `HELD_THROTTLE`。

### `unthrottle_all(band: u8, rq: &Runqueue) -> usize`
> band | 带级解限流：扫环上带成员清 `HELD_THROTTLE`（只清本位——FROZEN 不动）。

---

## 四、EAS 能效（em.rs + util.rs）

EM 频点表（形状不落算法）+ per-CPU util EWMA + overutilized 回退判据。

### `em_register(domain: usize, cpu_mask: u32, points: &[EmPoint]) -> bool`
> em | 注册/重注册某域 EM 表（boot/ktest 期单写者；真机 DT 解析后调用）。

### `em_valid(domain: usize) -> bool`
> em | 域是否已注册有效 EM 表（Acquire 载入——true 保证表内容已同步可见）。

### `em_domain(domain: usize) -> Option<EmDomain>`
> em | 拷出某域 EM 表快照（ktest/消费者观测面；未注册/越界 → None）。

### `em_max_capacity(domain: usize) -> u32`
> em | 最高频点容量（overutilized 阈值/选核的容量基准；未注册/越界 → 0）。

### `em_min_power(domain: usize) -> u32`
> em | 最低频点功耗（域最低能耗/空闲基准——首点即最低频点；未注册/越界 → 0）。

### `em_register_qemu_virt() -> bool`
> em | QEMU virt 默认表：单域 CPU0-1、三点 250M/500M/1G（TCG 对称核建模）。

### `util_window_vct() -> u64`
> util | 窗长（VCT cycles）= `CNTFRQ / UTIL_WINDOW_HZ`。

### `util_accumulate(cpu: usize, delta_cycles: u64)`
> util | tick 面调用：累计本核运行时间（每 tick 的 CNTVCT delta cycles）。

### `util_tick_window(cpu: usize, now: u64)`
> util | 窗口到期检查（tick 内调用）：到期则折叠一窗 EWMA。

### `cpu_util(cpu: usize) -> u32`
> util | PM 消费面：读当前 EWMA（0..=1024——与 `Topo::capacity`/EM capacity 同标）。

### `overutilized(cpu: usize) -> bool`
> util | 指定核是否 overutilized（EAS 回退判据）。

### `any_overutilized() -> bool`
> util | 全域 overutilized（任一核超 → true——Linux EAS 全域回退口径）。

### `util_reset(cpu: usize)`
> util | 清零重开窗（ktest/boot 期清场专用——生产路径无重置消费面）。

---

## 五、测量（hdr.rs + `berry_hdrface` re-export）

HDR 直方图（897 桶对数尺）+ per-CPU 切换/唤醒延迟双路 + 跨核合并分位。W3-Q16（cf9e253）起形状常量/桶映射律/分位秩/`Hdr`·`CpuHdr` 原子直方图单源外迁 `berry-hdrface`（host 可测钉板），hdr.rs 留 8 条裸并发机械（per-CPU 存储/槽化/写入口/合并读面），其余 12 项 `pub use` re-export——`berry_sched::` 调用路径不变（re-export 族条目语义描述仍以 `berry_hdrface` 源 doc 为准）。

### `bucket_of(value: u64) -> usize`（re-export）
> hdr | 值 → 桶下标（纯函数；`record` 的 O(1) 映射独立成函数供 ktest 全域钉板）。判定律单源 `berry_hdrface::bucket_of`（W3-Q16 外迁族，`intent_valid` 同款）。

### `bucket_upper(idx: usize) -> u64` `const`（re-export）
> hdr | 桶下标 → 桶上界（含）。分位返回值 = 命中桶的保守上界。单源 `berry_hdrface::bucket_upper`。

### `Hdr::new() -> Self` `const`（re-export）
> hdr | const 构造（无 doc；零桶 + 哨兵 min/max + count 0）。单源 `berry_hdrface::Hdr`。

### `Hdr::record(&self, value: u64)`（re-export）
> hdr | 记录一个值（热路径：一次 clz + 一次移位 + 四次本核原子 RMW——零分配零 panic）。

### `Hdr::percentile(&self, pm: u64) -> u64`（re-export）
> hdr | 分位查询（千分比入参：p50 → 500、p99 → 990、p99.9 → 999；零样本 → 0）。

### `Hdr::percentile_rank(&self, rank: u64) -> u64`（re-export）
> hdr | 第 `rank` 小样本所在桶的保守上界（rank ∈ 1..=N，越界帽到界内；零样本 → 0）。

### `Hdr::total(&self) -> u64`（re-export）
> hdr | 总样本数（读者 N 的 Acquire 读最先——模块头读序纪律）。

### `Hdr::min_value(&self) -> u64`（re-export）
> hdr | 精确最小值（u64::MAX ⇔ 零样本——打印端 n>0 门控后必真值）。

### `Hdr::max_value(&self) -> u64`（re-export）
> hdr | 精确最大值（0 ⇔ 零样本；原值口径——超 `HDR_MAX_VALUE` = 钳位失真信号）。

### `Hdr::bucket(&self, idx: usize) -> u64`（re-export）
> hdr | 单桶计数读（dump/导出面原料；越界 → 0）。

### `Hdr::reset(&self)`（re-export）
> hdr | 清零回出生态（ktest 清场/基准多轮跑专用——单写者语境调用）。

### `CpuHdr::new() -> Self` `const`（re-export）
> hdr | const 构造（无 doc；静态预储 .bss——boot 后永不增长）。单源 `berry_hdrface::CpuHdr`。

### `record_switch_lat(v: u64)`
> hdr | 热路径写：记一笔本核切换延迟（cpu_id 内读 + 拓扑槽外防御收敛尾槽）。

### `record_wake_lat(v: u64)`
> hdr | 热路径写：记一笔本核唤醒延迟。

### `cpu_hdr(cpu: usize) -> Option<&'static CpuHdr>`
> hdr | 取 `cpu` 槽只读引用（越界 None）；生产热路径写走 record_* 两写面。

### `cpu_reset(cpu: usize)`
> hdr | 清 `cpu` 槽双路（ktest 清场；单写者语境——`Hdr::reset` 同政策）。

### `merged_percentile(k: LatKind, pm: u64) -> u64`
> hdr | 合并分位（千分比入参，语义同 `Hdr::percentile`；各核 N 先 Acquire 读、桶后读）。

### `merged_total(k: LatKind) -> u64`
> hdr | 合并总样本数（Σ各核 N）。

### `merged_min(k: LatKind) -> u64`
> hdr | 合并最小值（跳过空核的 u64::MAX 哨兵；全空 → u64::MAX）。

### `merged_max(k: LatKind) -> u64`
> hdr | 合并最大值（原值口径；全空 → 0）。

---

## 六、任务（task.rs）

`Task` TCB + 构造器族 + 持有集位图常量。

### `Task::new_kernel(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8]) -> Box<Task>`
> task | 分配一个新内核线程：16KiB 栈 + 由 `new_kernel` 接线的初始上下文。

### `Task::new_kernel_with_as(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8], ttbr0: usize, asid: u16) -> Box<Task>`
> task | 同 `Task::new_kernel`，但挂地址空间（2b-2：`ttbr0`/`asid`，0/0 = 无 AS）。

### `Task::try_new_kernel_with_as(entry: extern "C" fn(usize) -> !, arg: usize, name: &'static [u8], ttbr0: usize, asid: u16) -> Option<Box<Task>>`
> task | `Task::new_kernel_with_as` 的 try 版（3b3 H2）：栈/TCB 任一分配失败返 None。

### `Task::new_user_with_as(entry_va: usize, arg: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize) -> Box<Task>`
> task | 用户线程版（2b-4 首个 EL0）：`entry_va`/`sp0_va` 为用户 VA（须已映射）。

### `Task::try_new_user_with_as(entry_va: usize, arg: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize) -> Option<Box<Task>>`
> task | `Task::new_user_with_as` 的 try 版（3b3 H2）：16KiB 栈 + TCB 装箱任一 OOM 返 None。

**`Task` 公有字段速览**（详注 → `sched/src/task.rs` L103 起）：

| 字段 | 类型 | 一句话 |
|------|------|--------|
| `ctx` / `kstack` / `name` / `state` | `TaskContext` / `Option<Box<[u8]>>` / `&'static [u8]` / `TaskState` | 切换上下文 / 私有内核栈 / 调试名 / 任务态 |
| `slice_left` | `u64` | 剩余时间片（周期数口径，DVFS 不漂移） |
| `vruntime` / `vdeadline` | `u64` / `u64` | EEVDF 虚拟运行时间 / 虚拟截止期（`Fair` 消费） |
| `ttbr0` / `asid` | `usize` / `u16` | 地址空间（0/0 = 无 AS） |
| `proc_tag` / `pinned` / `thread_key` | `usize` × 3 | 进程标签 / Process ObjRef 钉住 / Thread ObjRef 钉住（语义归 `内核`） |
| `home_cpu` / `spread` / `placement_hint` | `u8` / `bool` / `u8` | 归属核 / 可铺核资格位 / 放置偏置（EAS-1 首刀） |
| `held_locks` | `u8` | 持有集位图（切换舞蹈按位集逆序放/正序重取） |
| `svc_mask` / `preempt_threshold` | `u8` / `i32` | SVC 持锁类位集 / 抢占阈值 |
| `prio_base` / `prio_effective` / `pi_inherited` | `i32` × 3 | 基础 / 有效 / PI 捐赠优先级 |
| `duration_hint_ns` / `epp` | `u64` / `EppHint` | 时长提示 / EPP 偏好 |
| `intent_declared` / `intent_effective` / `intent_hop` / `intent_since` / `intent_acc` | `u8`/`u8`/`u8`/`u64`/`u64` | intent 声明档 / 生效档 / 跳数 / 时戳 / 记账 |
| `wait_obj` / `wait_signals` / `wait_deadline` / `wait_since` | `usize`/`u64`/`u64`/`u64` | SLEEPQ 等待四元组 |
| `wakeup` / `wake_vct` | `Cell<WakeReason>` / `Cell<u64>` | 唤醒原因 / 唤醒时刻戳（interior mutability） |

**常量**：

| 常量 | 值 | 语义 |
|------|-----|------|
| `STACK_SIZE` | `16 * 1024` | 内核栈 16KiB |
| `HELD_SQ` / `HELD_RQ0` / `HELD_RQ1` / `HELD_REAPQ` / `HELD_TABLE` | `1<<1` / `1<<2` / `1<<3` / `1<<4` / `1<<5` | 结构锁五位（SQ/双 RQ/REAPQ/表锁） |
| `HELD_FROZEN` | `1<<6` | freeze 冻结位（W3-Q12 起位值单源 `berry_ringface::HELD_FROZEN`，task.rs re-export 维持 `berry_sched::` 引用路径零改动） |
| `HELD_THROTTLE` | `1<<7` | band 限流位（W3-Q12 起位值单源 `berry_ringface::HELD_THROTTLE`，同上 re-export 面） |
| `PRIO_DEADLINE`（class.rs） | `1000` | deadline 类优先级（EDF 准入门） |

---

## 七、环形队列（runqueue.rs）

侵入式双向环（`NonNull<Task>` 裸指针 + `Cell` interior mutability）+ `Topo` 拓扑。

### `linkable(t: &Task) -> bool`
> runqueue | 入环前置判据：`t` 的链接字段须均 `None`（不在任何环上）。

### `Runqueue::new() -> Self` `const`
> runqueue | const 构造（无 doc；空环哨兵——head 指自身）。

### `Runqueue::len(&self) -> usize`
> runqueue | 环长（走查对账用；Relaxed 免锁读——c-2b slice 3 原子化）。

### `Runqueue::resync_vruntime_sum(&self) -> u64`
> runqueue | 全环重算 Σvruntime 并回写账本（S2b+ 洗账面——仅测试/诊断用）。

### `Runqueue::ring_check(&self) -> usize`
> runqueue | 环完整性走查（复盘雷 2 残留诊断）：head 沿 `rq_next` 走一圈须回 head。

### `Runqueue::is_empty(&self) -> bool`
> runqueue | 队列是否为空。

### `Topo::single() -> Self` `const`
> runqueue | 单核拓扑（G0/G1）。

**未导出面**（`pub(crate)`，调度核内部执法——外部勿调）：

- `Runqueue::link_back(&self, task: NonNull<Task>)` — 尾插入环（unsafe；`enqueue` 的底座）
- `Runqueue::unlink(&self, task: NonNull<Task>)` — 摘出环（unsafe；`dequeue` 的底座）

---

## 八、intent（intent.rs）

intent（u8）第一类输入：六档词汇表（ADR #58）+ send/recv 两侧结算。

### `intent_valid(v: u8) -> bool`
> intent | intent 值合法域（0x00–0x05；入口治理随声明入口 syscall 落地，判别先冻结）。W3-Q13 起判定律单源 `berry_intface::intent_valid`，intent.rs 转为 `pub use` re-export（`berry_sched::intent_valid` 调用路径不变）——本地 pub fn 计数 3→2。

### `snapshot(t: &Task, now_ns: u64) -> (MsgIntent, SnapOut)`
> intent | send 侧快照：发送者 `intent_effective` 打包进消息头（fastpath 增量 = 一次读 + 一次原子加）。

### `settle(t: &mut Task, m: MsgIntent, opt_out: bool, now_ns: u64) -> SettleOut`
> intent | recv 侧结算（slowpath，dequeue 即结算）：每消息重导——借入（六档之 4）与超帽一律回落 declared。

**六档常量**：

| 常量 | 值 | 档位 |
|------|-----|------|
| `INTENT_UNSPECIFIED` | `0x00` | 未声明（默认） |
| `INTENT_LATENCY_CRITICAL` | `0x01` | 延迟临界 |
| `INTENT_INTERACTIVE` | `0x02` | 交互 |
| `INTENT_UTILITY` | `0x03` | 效用 |
| `INTENT_BACKGROUND` | `0x04` | 后台 |
| `INTENT_MAINTENANCE` | `0x05` | 维护 |

**预算常量**：`INTENT_HOP_MAX = 8` ｜ `INTENT_HOP_BUDGET_NS = 50ms` ｜ `INTENT_CHAIN_BUDGET_NS = HOP_BUDGET × HOP_MAX`。

---

## 附：自旋锁（spin.rs）

`SpinLock`（TTAS：CAS 入主，失败转廉读自旋让出总线）——lib.rs 未列 `pub use`，经 `berry_sched::spin::` 路径可达。

### `set_spin_bark(f: fn(usize))`
> spin | 安装自旋看门狗 bark（sched::init 自装；boot 单线程恰一次，此后只读）。

### `SpinLock::new() -> Self` `const`
> spin | const 构造（static / `Runqueue::new` 组合用）。

### `SpinLock::acquire(&self)`
> spin | 取锁（TTAS：CAS 入主，失败转廉读自旋让出总线）。

### `SpinLock::release(&self)`
> spin | 放锁。

---

## 统计

| 文件 | pub fn | 归组 |
|------|--------|------|
| sched.rs | 71 | 一、调度核心 |
| policy.rs | 15 | 二、策略服务 |
| band.rs | 7 | 三、层级预算 |
| em.rs + util.rs | 6 + 7 | 四、EAS 能效 |
| hdr.rs | 8（+`pub use` re-export 族：`bucket_of`/`bucket_upper`/`rank_of`/`Hdr`·`CpuHdr` 全方法单源 `berry_hdrface`） | 五、测量 |
| task.rs | 5 | 六、任务 |
| runqueue.rs | 7 | 七、环形队列 |
| intent.rs | 2（+`pub use` re-export 族：`intent_valid` 单源 `berry_intface`） | 八、intent |
| spin.rs | 4 | 附 |
| class.rs | 2（+trait 5 法） | 一、调度核心 |
| lib.rs | 0（`pub use` 根） | — |
| **合计** | **134** | ｜ 含 `unsafe` 8、`const` 3、`extern "C"` 4（v1.3：W3-Q16 hdrface 外迁后 134——v1.2 的 146 为外迁前时点，const 6→3 = `bucket_upper`/`Hdr::new`/`CpuHdr::new` 三 const 随迁；v1.2：W3-Q13 `intent_valid` 外迁 re-export 后 146，v1.1 的 147 为外迁前时点；const 6 系 v1.1 勘误，v1.0 笔误 5 不回传；W3-Q6 起含 `intent_settle_current_ext`） |
