# 调度器 API 参考

> 版本：v1.20（2026-11-23 升版：w38 F1 保命批 sq_is_held 随迁〔01a194159，2026-11-22 17:05 落库经 b977264d8 17:32 入轨——晚于 v1.19 升版窗 04:42 同日，影抄零随迁〕——sched.rs 新增 pub fn `sq_is_held() -> bool`〔查询当前任务/执行流是否已持有 SLEEPQ 锁：只读 held_locks 位 HELD_SQ，CURRENT 空窗恒 false——kernel 侧跨层析构路径锁序判别面，channel.rs:196 与 process.rs:390 两消费点 `!kl_held() || sq_is_held()` 形；`sq_lock` 的姊妹查询形——取锁/放锁/查询三面族齐〕，sched.rs 93→94、合计 192→193，全 safe 非 const）；v1.19（2026-11-22 升版：遥测进程账读口 op=4 刀随迁〔2310da74c，2026-11-19 经 dd5660dbf 合并窗入 base〕——sched.rs 新增 pub fn `for_each_inflight_task(mut f: impl FnMut(&Task))`〔遍历全体在役任务：SLEEPQ 头→尾 ＋ 双 RQ 环走查三并集，0x84 op=4 三并集任务快照的遍历基座；`for_each_runnable` 的姊妹形——本核环 → 全体在役面〕，sched.rs 92→93、合计 191→192）；v1.18（2026-11-20 勘正：正文 `pick_next_preempt` 条目与 `pick_inner` 引用系合并前幻影——末笔合并 1e7872196 形胜替换我方 pick_next_preempt 形（语义等价：解冻→扫描→带门→出选，DL-B4 钉板），条目行/`pick_next_filtered` 行/class.rs 计数行已改登 W3-Q93 `Fair::pick_next_if_above` 实形〔五参；v1.13 版史叙述保留原名〕；2026-11-19 升版：机会五③密度账本化〔e547df7da，04 号审计收官件——04 号全部行动项+五件挂账债清偿〕——sched.rs +2 pub fn〔`density_reclass_kt`/`dens_on_ring_walk_kt`，ktest 跨 crate 出口对〕+ runqueue.rs +3 pub fn〔`Runqueue::int_count`/`util_count` 密度账本读对 + `resync_density` 洗账面〕，186→191）；v1.17（2026-11-19 升版：预算授片分派刀——TaskClass 新 pub `grant_cycles_of(&self, t)` per-task 面〔+1，186；dl_grant 前置 deadline×budget>0 双门命中返 budget、回落 class 面——八授片点+idle 统一换臂单习语〕；sliceface 新律 dl_grant 系 balface 域外不计）；v1.16（2026-11-19 升版：探针族退役刀——spin.rs `set_spin_bark` 退役〔186→185，spin_bark 族随 hb/STUCK/PULL_CNT/EAS 日志/sS2b 打印探针族退役〕；balface `canary_due`/`CANARY_PERIOD` 为 bal 模块面〔计数归 balface 域不在本口径〕）；v1.15（2026-11-18 升版：深 park 醒源刀——sched.rs `nearest_dl_throttle_wake` + `nearest_deadline_park` 两 pub fn〔风暴修两形分离：park 三源形/park 武装点专用，忙核 nearest_deadline 刻意不含第三源〕；**计数机提正身 186**〔v1.14 的 183 与 v1.14a 补登的 184 均系低估，逐文件重数勘误——sched.rs 88→90〕）；v1.14（2026-11-17 升版：04 号审计机会四①②——band.rs 新增 pub `quota_cycles_of`〔+1，182→183；武装槽 quota 镜像读——band_on 同款读法，grant/rearm 三消费点经 class.rs 私有 quantum_cached 免每授片 cntfrq MRS+u128 软除法〕+ `FairBand::consume` 签名扩 now 参〔now 线程化——tick 入口单读传三消费点，四读归二；语义差=单边保守微差文档化〕；arm 保持现算=数据流「现算→configure→槽镜像」单向）；v1.13（2026-11-17 升版：04 号审计机会一②双圈合一——sched/src pub fn 面一动一退役：class.rs `Fair::pick_next_preempt` 新增〔16→17——抢占单圈门控形出选：scan_filtered 出选 + 扫尾门控行读胜者 pre-apply eff，miss 零副作用；try_preempt 双圈（探峰+出选）合一的消费面，`has_eff_above_filtered` 唯一消费者迁移〕+ runqueue.rs `has_eff_above_filtered` 退役〔`pub(crate)` 不在机提口径、计数不动——PickFilter 退役改形先例；影抄本就无行（历史缺登），退役后行集对齐〕，合计 181→182（+1 pub fn——退役项 `has_eff_above_filtered` 系 `pub(crate)` 从不在机提口径，runqueue.rs 计数不变仍 7；新 fn safe 非 const）；连带滚新：preempt_checkpoint 行补 0ea3d11ad len<=1 门+count==0 双门陈账〔机会一①/③——③计数移窗已随该刀落地〕）；v1.12（2026-11-17 升版：SMP-01 F3/F4 + CP-09 EAS M2 随迁——v1.11 对拍时点 b046aeb2 后三笔改 sched/src pub fn 面：0d02a3db〔F3 锁面收口〕spin.rs `try_acquire`〔4→5〕+ sched.rs `try_rq_lock`/`try_rq_unlock`〔84→86——band tick 环扫撕裂窗非阻塞 RQ 锁，可用→扫/不可用→跳过永不 wedge〕、c1f695be〔F4 wake 重放+band 偷取〕sched.rs `steal_band_rank`〔86→87——F4 band 感知偷取排序，STEAL_RANK_COMPAT/DEPRIOR/PROTECT 三常量锚〕+ policy.rs `placement_hint_ungated`〔16→17——F4 ungated 直读出口，消费面接线在途〕、dcf359c5〔CP-09 EAS M2+四带全开〕em.rs `em_max_capacity_of_cpu`〔6→7——overutil 域修：本核相对本域容量，修域 0 硬编码〕，合计 175→181（六 fn 全 safe 非 const，const 4/unsafe 8/extern "C" 4 不变）；2026-11-17 系统审计 S2 补录五条〔steal_band_rank/try_rq_lock/try_rq_unlock/placement_hint_ungated/SpinLock::try_acquire——34cbd255b 后漏同步〕已并入本版口径，其头注滚新 180 系漏计 CP-09 `em_max_capacity_of_cpu` 同窗竞速，本版勘正 181；v1.11：2026-11-16 升版：跨根收口随迁〔c3717d95c——origin/main 显示线并回 wip/sched-1.1，2026-11-16〕——v1.10 对拍时点 85434cb6d 后四笔改 sched/src pub fn 面：e648147a〔第五轮终验 FairBand 四 API 行为差异化〕class.rs +8 fn〔8→16——`FairBand::quantum_cycles`/`period_cycles`/`arm`/`consume`/`period_check`/`throttle_all`/`unthrottle_all` + `TaskClass::grant_cycles`〕、5131ebcd〔SMP 全路径+可观测性〕band.rs +3 fn〔7→10——`any_armed`/`nearest_throttle_wake` 于 02634905 补、`lazy_enforce`〕+ stats.rs 新档 3 fn〔OBS-01 观测面，第 14 文件〕+ util.rs +1 fn〔7→8——`util_idle_fold`〕+ sched.rs +1 fn〔`current_task_name`，cbox 读面 037ea2e1〕、02634905〔F5+F6〕sched.rs +1 fn〔82→84——`throttle_yield_due`〕，合计 158→175（十七 fn 全 safe 非 const，const 4/unsafe 8/extern "C" 4 不变；对象口径 13→14 文件）；v1.10：水通管道随迁〔10297ac22，63fbca1c9 合流入 base〕——06 号 §5「水通管道」双管道首接：class.rs 新增 `class_of_task`〔class 赋值面单源——`intent_effective` 经 `class_fair_intent` 工厂查表动态派生 `TaskClass`，零缓存〕+ `FairBand::yield_slice_return`/`TaskClass::yield_slice_return`/`TaskClass::band` 三 fn〔4→8〕；sched.rs 新增 `eas_pick_place`〔纯决策面，ktest 钉板对象〕/`eas_place`〔现役面收集+决策——place() 的 SMP 消费面〕双 fn〔80→82〕，合计 152→158（六 fn 全 safe 非 const，const 4/unsafe 8/extern "C" 4 不变）；连带收口 `class_fair_intent` 条目——集成面已接（水通管道任务①），现役集口径改首批 0x02/0x03）；v1.9：2026-11-16 升版：调度深水区收官 timer slack 随迁〔988944e09〕——`intent.rs` 新增 `set_timer_slack_ns`/`timer_slack_ns`/`timer_slack_defer` 三 fn〔CP-11 §3.8 系统缺省 timer slack：boot-params 第三键存取双 fn + LATE-only 顺延判定律消费面，4→7〕，合计 149→152（三 fn 全 safe 非 const，const 4/unsafe 8/extern "C" 4 不变）；v1.8：2026-11-15 升版：A 线审计 §4 漏登收口——补登 `sched/src/oneshot.rs`（第 13 文件，`tighten_deadline` 1 fn〔c3022903b 忙核 arm-to-nearest-deadline 收紧律，lib.rs:53 已 `pub use`；v1.7 升版时该件在途未并故缺席〕），对象口径 12→13 文件、合计 148→149（sched.rs 80 逐文件口径不变——「12 文件 148」系 oneshot 缺席非虚计，非逐文件重算差）；v1.7：2026-11-15 升版：调度终窗 idle 闭合+M4 骨架〔01cd881b〕——`spawn_user_env` 新 fn〔sched.rs 79→80，boot env 旗标面 x1 递 EL0 服务〕+ `try_prepare_user`/`try_spawn_user` 签名扩 `arg1` 参 + `band_of`〔policy.rs 15→16，band 选择位读出口——v1 恒 `BAND_SEL_NONE` 纯预留〕+ `FairBand::new`/`class_fair_intent`〔class.rs 2→4，Intent M4 带实例工厂〕，合计 144→148、const 3→4；v1.6：P2-2 退出异步化〔9791a99〕新增 `set_reap_post_hook` 单 fn（sched.rs 78→79）——`reap_drain` 尾钩子槽，kernel 侧 pending ht-drain 队列的异步排水入口，合计 143→144；v1.5：PI 形 B 修复〔42a794e0〕新增 `pi_rq_lock`/`pi_rq_unlock`/`for_each_runnable`/`rq_insert_kt`/`rq_unlink_kt` 五 fn（sched.rs 73→78）+ G5 bootparams〔345739e1〕新增 `set_intent_default`/`intent_default` 双 fn（intent.rs 2→4），合计 136→143；v1.4：daa5ef7e perf(pi) 全图重推门控新增 `pi_recompute_needed`/`pi_boost_recount` 双计数闸门 fn，sched.rs 71→73、合计 134→136；v1.3：W3-Q16 HDR 形状与分位数学外迁随迁——`bucket_of`/`bucket_upper`/`Hdr`/`CpuHdr` 等 12 项转 re-export 单源 `berry_hdrface`，hdr.rs 20→8 机械壳，计数 146→134；v1.2：W3-Q13 `intent_valid` 外迁随迁 147→146）｜ 对象：`sched/` crate（`berry-sched`，14 文件）｜ 审计口径：**193 个 `pub fn`**（v1.20：w38 F1 保命批 01a194159 sched.rs +1 `sq_is_held`〔全 safe 非 const——SLEEPQ 持有查询面，kernel channel/process 两消费点〕；v1.19：遥测 op=4 刀 2310da74c sched.rs +1〔`for_each_inflight_task`——SLEEPQ＋双 RQ 三并集遍历，0x84 op=4 快照基座〕；v1.18：机会五③密度账本化 e547df7da runqueue +3〔int_count/util_count/resync_density〕+ sched +2〔density_reclass_kt/dens_on_ring_walk_kt〕；v1.17：预算授片 class.rs +1 grant_cycles_of；v1.16：探针族退役 spin.rs −1〔set_spin_bark〕；v1.15 机提正身：醒源刀 sched.rs +2〔nearest_dl_throttle_wake/nearest_deadline_park〕且逐文件重数勘误——v1.14/v1.14a 期 183/184 均低估 1-2；v1.14：机会四 band.rs `quota_cycles_of` +1〔182→183〕；v1.13 复算勘正〔181→182 正身随本版一并落〕：机会一② class.rs +1〔`pick_next_preempt`〕→ 182——退役项 `has_eff_above_filtered` 系 `pub(crate)` 不在本口径、runqueue 7 不动〔初版「181 不变」为误〕；v1.12 勘正：2026-11-17 系统审计 S2 滚新 180 漏计 CP-09 `em_max_capacity_of_cpu`〔dcf359c5 同窗竞速〕，本版机提复算 181；含 `pub const fn` / `pub unsafe fn` / `pub unsafe extern "C" fn`，逐文件机提 + 人工核对 + sched/ 14 文件全量复核〔v1.12 重数：`grep -oE 'pub (const |unsafe )*(extern "C" )?fn'` 逐文件=10/16/7/8/7/0/1/17/7/87/5/3/5/8 合计 181；初估 132 为低估）
> 事实源 = 源码 doc 注释首行（提取时点 2026-11-08，2026-11-11 增量对拍至 daa5ef7e，2026-11-13 增量对拍至 f44c2ba8/HEAD，2026-11-15 增量对拍至 01cd881b/HEAD，2026-11-15 v1.8 补登对拍至 4f673f206/HEAD——oneshot.rs 全量复核，2026-11-16 v1.9 增量对拍至 988944e09/HEAD——intent.rs timer slack 三 fn 全量复核，2026-11-16 v1.10 增量对拍至 85434cb6d/HEAD——水通管道六 fn + class_fair_intent 集成面收口全量复核，2026-11-16 v1.11 增量对拍至 HEAD〔b046aeb2 基线〕——跨根收口四笔〔e648147a/5131ebcd/02634905/037ea2e1〕十七 fn 全量复核，2026-11-17 v1.14 增量对拍至 HEAD〔630597a72 基线〕——机会四①②一动一扩参全量复核（计数 183），2026-11-17 v1.13 增量对拍至 HEAD〔095d41d13 基线〕——机会一②一动一退役全量复核（计数勘正 182），2026-11-17 v1.12 增量对拍至 HEAD〔a11977b8 基线〕——F3/F4/CP-09 三笔六 fn 全量复核，**补录块四签名凭空拼形 2026-11-17 车队窗遗漏审查〔wf_c99667d9-89a〕捕获按源回改**——详见补录节勘正注；2026-11-19 v1.18 增量对拍至 HEAD〔5b0aeffa1 基线〕——机会五③密度账本化〔e547df7da〕五 fn 全量复核（计数 191），base 前进窗 W2 代偿随迁；2026-11-22 v1.19 增量对拍至 HEAD〔2e6274d279 基线〕——遥测 op=4 刀〔2310da74c〕一 fn 全量复核（计数 192），W2 影抄随迁，2026-11-23 v1.20 增量对拍至 HEAD〔b977264d8 后谱系——01a194159 w38 F1 保命批〕——sq_is_held 一 fn 全量复核（计数 193），W2 影抄随迁）；本文是影抄不是立法——签名冲突时以 `sched/src/` 为准并回改本文。

## 使用方法

- **查 API**：按下方十个模块分组浏览（附：自旋锁不计）；每条目格式为签名 + `> 模块 | 一句话描述`（描述提自该函数 doc 注释首行，`[`链接`]` 已清洗）。
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
> sched | 抢占检查点（无 doc——实义：len<=1 门 + count==0 门双前置〔0ea3d11ad 机会一①/③——空探零锁零扫描零全局行触达〕+ IRQ 保存 + 计数为零时 `try_preempt`，g3-1d 真探形）。

### `preempt_checkpoint_hits() -> u64`
> sched | g3-1d：检查点真探形命中数（boot 起累计；fa ktest 尾 `ckpt=` 字段打印；机会三〔2026-11-17〕起 per-CPU 两槽求和快照——读者须 ≥/增长型，禁等值比较）。

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

### `spawn_user_env(entry_va: usize, arg: usize, env: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize)`
> sched | 同 `spawn_user` 但入口带第二参（x1 = boot env 旗标面——06 号修窗件 1 后半：内核 boot 期消费的 fw_cfg 键值随 spawn 递 EL0 服务，零新 ABI/零句柄槽位重排；现役消费者 = svcmgr demo-face 门）。旗标位定义归内核 crate（语义层 `USER_ENV_*` 常量镜像）。

### `try_prepare_user(entry_va: usize, arg: usize, arg1: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize, pinned: usize, thread_key: usize, spread: bool) -> Option<usize>`
> sched | 同 `spawn_user` 但 try 化（3b3 审查 H2）：栈/TCB 任一 OOM 返 None（v1.7：签名扩 `arg1` 第二参——`spawn_user_env` 同批）。

### `enqueue_prepared_user(task_ptr: usize)` `unsafe`
> sched | 把 `try_prepare_user` 产出的已初始化任务入环（调用侧须完成一切登记）。

### `try_spawn_user(entry_va: usize, arg: usize, arg1: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize, pinned: usize, thread_key: usize, spread: bool) -> Option<usize>`
> sched | try 面 spawn_user 全流程（无 doc；prepare + enqueue 合一；v1.7：签名扩 `arg1` 第二参）。

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

### `set_reap_post_hook(f: fn())`
> sched | 注册 REAP 后处理钩子（kernel 语义层 boot 期调用，EXIT_HOOK 同模式；P2-2 退出异步化——pending ht-drain 队列排水入口）。

### `reap_drain()`
> sched | 回收退出任务的 Task Box + 内核栈（16KiB/个；idle_loop 稳态与 yield 切回尾调）。P2-2 起尾部（REAPQ 锁窗外）追调 [`REAP 后处理钩子`](#set_reap_post_hookf-fn)（未注册空过）。

### `switches() -> u64`
> sched | 总切换数（含 park 事件；金丝雀 `switches=` 行口径；机会三〔2026-11-17〕起 per-CPU 两槽求和快照——读者须 ≥/增长型，禁等值比较）。

### `switches_tt() -> u64`
> sched | 任务↔任务切换数（遥测环分母——G1.5 断言比较面；机会三〔2026-11-17〕起 per-CPU 两槽求和快照，与 telemetry 两环求和对称）。

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

### `pi_recompute_needed() -> bool`
> sched | PI 全图重推门控：双零（`PI_DERIVED_WAITERS`/`PI_BOOSTED_SLEEPERS` 均 0）⇒ 队上既无 donor 可能也无待清 boost，重推整体跳过（daa5ef7e，常态零捐赠 O(n²)→2 原子读；消费者 = `kernel/src/pi.rs::recompute_locked`）。

### `pi_boost_recount(n: u32)`
> sched | 重推尾部重数回写（`PI_BOOSTED_SLEEPERS` 第三收口点）：全图重推写完在队者 `pi_inherited` 后重数非零个数回写；仅 SQ 窗内调用（daa5ef7e）。

### `pi_derived_recount(n: u32)`
> sched | 重推尾趟重登计数 ①（[`PI_DERIVED_WAITERS`] 第三收口点扩展——W3-Q94 刀 a，267a793d2）：清零趟 + 传播趟跑完后逐睡眠者按**入队同款判据**（keyroute 单源：真键 ∧ 派生位 ∧ d_eff>0，d_eff = max(prio_effective, pi_inherited)——此刻 pi_inherited 已是本轮终值）重数回写。价值 = 配对安全：入队 d_eff=0 未入账、睡眠中被传播抬 pi 的睡眠者在此补登——「欠账必补」消灭出队误减 ①的下溢面（计数只可能短暂虚高至下一趟重推，门常开方向劣化 = 性能损失非正确性损失——与 ② recount 同款）。仅 SQ 窗内调用（重推本体所在窗）。v1.15 随迁〔267a793d2〕。

### `sq_lock() -> bool`
> sched | 取 SLEEPQ 锁（含持有位记账）；pub 供 kernel 侧 fastpath 义务④跨窗。

### `sq_is_held() -> bool`
> sched | 查询当前任务/执行流是否已持有 SLEEPQ 锁（只读 held_locks 位 `HELD_SQ`；CURRENT 空窗恒 false）——kernel 侧跨层析构路径锁序判别面（channel.rs/process.rs `!kl_held() || sq_is_held()` 两消费点）。v1.20 随迁〔01a194159 w38 F1 保命批〕。

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

### `pi_rq_lock() -> bool`
> sched | PI 重推 RQ 窗（4-3b PI 形 B——runnable 受捐盲区收口，42a794e0）：位感知取本核 runqueue 锁，返回 `phys` 配对 `pi_rq_unlock`（契约：SQ 锁已持 + IRQ 屏蔽窗内；KL 子窗须晚于本窗开启、早于本窗闭合——升序锁序唯一保证）。

### `pi_rq_unlock(phys: bool)`
> sched | 配对 `pi_rq_lock`（`phys=false` 退化窗不触——物理持有归外层，同 `sq_unlock` 的退化跳过语义）。

### `for_each_runnable(mut f: impl FnMut(&mut Task))`
> sched | 遍历本核 runqueue 环上全部 runnable 任务（head → 一圈）；契约：`pi_rq_lock` 窗内 + 回调不动 `rq_prev`/`rq_next`、不出入环不 block/wake（形 B 三趟重推并进本面；SMP 残留：只罩本核环）。

### `for_each_inflight_task(mut f: impl FnMut(&Task))`
> sched | 遍历**全体在役任务**（遥测进程账读口 op=4 刀，2310da74c）：SLEEPQ 头→尾 ＋ for cpu in 0..2 双 RQ 环走查三并集（freeze_by_pinned 三段同款形；遍历序确定性 SLEEPQ→RQ0→RQ1——截尾前缀语义的自述基础）。面 = Blocked+Ready+Running 全体在役（cur-in-ring 不变式）；Exited（REAPQ 瞬态）与 idle/park 语境结构性不在面。契约：SQ+RQ0+RQ1 三并集持锁窗内（0x84 op=4 SVC 入口 SQ_RQ_TABLE 预取形）＋ IRQ 屏蔽；回调只读（禁改链接字段、禁出/入队/入环、禁 block/wake）。`for_each_runnable` 的姊妹形——本核环 → 全体在役面。

### `rq_insert_kt(t: &mut Task)`
> sched | 把 `t` 链到本核 runqueue 环尾（ktest 手搭 runnable 专用；生产路径走 class enqueue/`enqueue_with_kick`——本函数不置 Ready、不路由；len/Σvruntime/DL_ON_RING 三账本经结构原语层入账）。

### `rq_unlink_kt(t: &mut Task)`
> sched | 把 `t` 摘出本核 runqueue 环（ktest 清理专用；不在环上幂等返回；账本出账同经结构原语层，与插入配对）。

### `density_reclass_kt(rq: &Runqueue, from: u8, to: u8)`
> sched | 机会五③密度账本化（2026-11-19）ktest 面：对给定（本地）Runqueue 做带档重登——[`Runqueue::density_reclass`] 的跨 crate 出口（pub(crate) 面 kernel/ktest 不可达；生产唯一调用者 = [`intent_settle_current_ext`] 锁包臂）。ktest 本地环单线程窗（timer/IRQ 未启）——RQ 锁契约的串行等价。与 TCB 档位写成对使用（settle 锁包的本地环镜像：只调其一即账本与走查漂移——DENS 族 D-3 钉的病理面）。

### `dens_on_ring_walk_kt(rq: &Runqueue) -> (usize, usize)`
> sched | 机会五③密度账本化（2026-11-19）ktest 面：对给定（本地）Runqueue 走查密度真值——[`Runqueue::dens_on_ring_walk`] 的跨 crate 出口（生产消费面 = `integrity_check` 金丝雀与 `Runqueue::resync_density` 重数）。

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

### `current_task_name() -> Option<&'static [u8]>`
> sched | cbox（黑匣子）读面：当前任务调试名。空态（boot 早期 CURRENT null/副核 park）= None——调用方落 `(boot)` 类占位名。读侧契约同 `current_task`（屏蔽窗/锁窗内读；cbox 钩子在异常入口 PSTATE.I=1 下调用，满足）。v1.11 随迁〔037ea2e1，经 c3717d95c 跨根收口入 base〕。

### `last_killed() -> &'static [u8]`
> sched | 最近被杀用户任务名只读访问（watchdog 断言「杀对了人」用）。

### `idle_loop() -> !`
> sched | idle 主循环（tickless）：runqueue 空且无睡眠者时屏蔽 CNTV + `wfi`。

### `throttle_yield_due(cur: *const Task) -> bool`
> sched | F5 限流垄断者强制让点判据（02634905，2026-11-16）：`cur` 持 `HELD_THROTTLE` 位即应让点（限流 cur 不再续跑、核回 idle/park）。消费点 = `sched.rs` 让点路径（`go.is_none()` 臂）。v1.11 随迁〔02634905，经 c3717d95c 跨根收口入 base〕。

### `on_irq_tick(_tf: &mut hal::trap::TrapFrame)` `unsafe extern "C"`
> sched | IRQ tick 槽（HAL `exception_irq` 经函数指针调）：expire 超时/片记账/pick 重调度。

### `on_sgi_resched(_tf: &mut hal::trap::TrapFrame)` `unsafe extern "C"`
> sched | SGI 重调度槽（副核经 eret 回 park 环承接；不进 tick 语义面）。

### `park_recheck()` `unsafe`
> sched | S-3a 副核 park 重查——自 `on_irq_tick` 的 cur==null 形态承接派发。

### `nearest_dl_throttle_wake() -> Option<u64>`
> sched | S5-c 逐任务限流的停泊醒源（深 park 醒源刀 2026-11-18）：`dl_on_ring()==0` 门常闭早退；门开 try_rq_lock（F3 非阻塞）只走本核环，过滤律镜像 lazy_unthrottle 四与门前三（**刻意不判到期**——过期 vd 必须入 min：停泊核要在已过期 vdeadline 醒），返被限流 DL 的 min vdeadline；try 失败返有界兜底臂 now+一片（锁竞争漏醒窗两形态〔对核偷取扫描持锁/SVC 窗放锁〕的严格更优收口——审查轮勘正）。阻塞 rq_lock 形违 F3 红线。

### `nearest_deadline_park() -> Option<u64>`
> sched | [`nearest_deadline`] 的停泊三源形（深 park 醒源刀·审查轮风暴修定形）：追加 `nearest_dl_throttle_wake` 第三源——只供 park 武装点（idle_loop 双臂/secondary_main）；忙核面（tick 收紧臂/block 忙核臂）刻意不含：过期 vd 经 tighten 无 d>now 门写过去 CVAL=IRQ 风暴（每 DL 周期复发至多一片——忙核本有 ≤1 片片尽 pick 兜底零收益）。

### `nearest_deadline() -> Option<u64>`
> sched | 最近截止时刻两源 min（S-3a tickless park 武装面——子案 §9.5 步 A）：SLEEPQ 队头 wait_deadline + `band::nearest_throttle_wake`（F5 带级护栏）。park 武装点应走三源形 [`nearest_deadline_park`]（含 S5-c 逐任务第三源——深 park 醒源刀 2026-11-18；忙核面刻意不含，见彼注）。

### `yield_slice_return(&self, t: &mut Task, rq: &Runqueue)`（`Fair` 固有方法）
> class | yield 片归还（moss `yield_slice` 语义的 BerryOS 形态）——让出者不得带片优势。

### `pick_next_filtered(&self, rq: &Runqueue, _topo: &Topo, my_cur: *mut Task) -> Option<&mut Task>`（`Fair` 固有方法）
> class | 过滤版出选（S-2b inherent 方法——trait 冻结不动）；头部先 S5-c lazy unthrottle（04 号审计附记一：生产出选两口之一——与 `pick_next_if_above` 头部同源，deadline 限流解冻在两函数头部发生——dl_on_ring 门常闭时零扫描）；带内无 eligible 时取最小键。

### `pick_next_if_above(&self, rq: &Runqueue, _topo: &Topo, my_cur: *mut Task, v: i32) -> Option<&mut Task>`（`Fair` 固有方法）
> class | try_preempt 专用单圈门控出选（W3-Q93 双圈合一终形——2026-11-18 末笔合并 1e7872196 形胜替换 v1.13 所册 `pick_next_preempt` 形，语义等价：解冻→扫描→带门→出选，DL-B4 钉板）：`scan_with_eff` 单圈扫描三件套——头部 S5-c lazy unthrottle 与 `pick_next_filtered` 同源 → 单圈追踪最高带与带内最优 → 带门 `best_eff > v`（`ScanAcc::best_eff` 单源累加器；阈值 `v` 由调用方 `(*my_cur).prio_effective` 自读一次传入；契约 my_cur 非空）；门假（`best_eff <= v`）不出选、不消耗 head 槽位——miss 零副作用（advance/apply/note_pick 全在门后），幻影出选防线的单圈形态，替代退役的 `Runqueue::has_eff_above_filtered` 探峰前置；门真出选者 = `pick_next_filtered` 出选者（同扫描单源 + `advance_past` + `policy::apply` + `note_pick`）；解冻时序前移（探峰假也解冻）与 PURE_EEVDF 形退化（仅 v1 生产形消费）文档化接受见源注。

### `FairBand::new(band_id: u8, band: IntentBand) -> FairBand` `const`
> class | 显式带参数构造（M4 启动配置覆盖路径入口形状——数字进启动配置，不写死；band_id 域执法归调用方）。

### `FairBand::yield_slice_return(&self, t: &mut Task, rq: &Runqueue)`
> class | yield 片归还委托（trait 外 inherent 方法，`TaskClass` 分派臂消费；行为与 unit `Fair` 逐位一致，M4 入场券纪律）。

### `class_fair_intent(intent: u8) -> Option<FairBand>`
> class | **class_fair_intent 工厂**：按 intent 档建 `FairBand` 带实例——查 `berry_iprofile::band_of_intent`（首批现役集 0x02/0x03 → 该档带参数行；首批外档/保留域 → `None` = 无带，调用方回落现役单带 `Fair`，缺省行为零漂移）。集成面（06 号 §5「水通管道」任务①，2026-11-16 收口）：task→band 赋值已接——`class_of_task` 按 `intent_effective` 查本工厂，`sched.rs` per-task 原语六点消费（enqueue_with_kick/偷取重入环/tick/dequeue×2/yield）；intent 双落点 = `spawn_intent`（内核 spawn 面）与 EL0 attr 面（spawn 后落 intent，动态派生下一次原语调用即改带）；策略覆盖通道 `policy::band_of` 仍归 v3——本工厂只认 intent 档。

### `class_of_task(t: &Task) -> TaskClass`
> class | **class 赋值面单源**（M4 集成面收口）：task → class 实例——`intent_effective`（§11 带键）经 `class_fair_intent` 工厂查表：现役集 → `TaskClass::Band`（表行带参数），其余 → `TaskClass::Unit`（现役单带，缺省行为零漂移）。动态派生、零缓存：intent 结算借入改档或 EL0 attr spawn 后落档，下一次调度原语调用自然改带——「spawn 时 band 赋值」与「运行期 band 跟随」同一实现。消费面 = `sched.rs` per-task 调度原语六点；`pick_next_filtered` 是队列级扫描（混合带同环出选），不经 per-task 分派。

### `TaskClass::yield_slice_return(&self, t: &mut Task, rq: &Runqueue)`
> class | yield 片归还的分派形（per-task 消费点——两臂同委托 unit `Fair`，行为零漂移）。

### `TaskClass::band(&self) -> Option<IntentBand>`
> class | 带参数投影（`Unit` = None）——ktest spawn 面断言与诊断面消费。

### `FairBand::quantum_cycles(&self) -> u64`
> class | 本带 quantum 的周期数（片授予〔`TaskClass::grant_cycles`〕与带配额〔`Self::consume` 懒武装〕的共用换算源）。v1.11 随迁〔e648147a，经 c3717d95c 跨根收口入 base〕。

### `FairBand::period_cycles(&self) -> u64`
> class | 本带 latency_target 的周期数（带预算期长——`Self::period_check` 判到期的参数源）。v1.11 随迁〔e648147a，经 c3717d95c 入 base〕。

### `FairBand::arm(&self, now: u64) -> bool`
> class | 带预算槽懒武装：未武装才 configure（quota = 本带 quantum、period = 本带 latency_target）。已武装槽不覆写——启动配置/ktest 显式 configure 优先（重配即清账翻期语义归 `band::configure` 单源）。v1.11 随迁〔e648147a〕。

### `FairBand::consume(&self, delta: u64, now: u64) -> bool`
> class | 带配额微记账（四 API·consume）：`delta` 周期数记入本带本期账——**quota = 本带 `quantum_ns`**（band 特有配额，替代 unit Fair 的全局 slice 锚）。**CBS 期内律**：只记本期内的周期数——`delta` 先剪辑到 `now − period_start`，跨期（旧期）部分不记入新期账（缺此律则翻期面粒度 > period 的带被误越线，ktest M4G 实测抓到 6 次假越线）。返 true = 本期**首**越线（调用方随后 [`Self::throttle_all`]——记账与环执法分两步，同 `band::consume` 契约；判定律/原子账单单源 `band::consume`，此处只携参 + 剪辑）。v1.11 随迁〔e648147a〕；**v1.14 签名扩 now 参**〔机会四②——now 自 FairBand::tick 入口单读线程化下发，剪辑界与 period_check 同参（「剪辑界 > period」理论窗消灭）；语义差=单边保守微差（恰边界翻期可迟一拍，亚 µs 级 vs 最小期 1ms，双兜底在席）〕。

### `FairBand::period_check(&self, now: u64) -> bool`
> class | 带翻期判（四 API·period_check）：到期判 = **本带 `latency_target_ns`**（band 特有延迟目标作预算期长）。返 true = 已翻期（调用方随后 [`Self::unthrottle_all`]）。翻期数学/CAS 防双翻单源 `band::period_check`。v1.11 随迁〔e648147a〕。

### `FairBand::throttle_all(&self, rq: &Runqueue) -> usize`
> class | 带级限流（四 API·throttle）：环扫置 HELD_THROTTLE + 带成员 VD 按 **`band.low_weight`** 重臂（被限流者垫后——带权重替代全局 prio_effective 的消费点）。deadline 任务豁免（`band_member`）。v1.11 随迁〔e648147a〕。

### `FairBand::unthrottle_all(&self, rq: &Runqueue) -> usize`
> class | 带级解限流（四 API·unthrottle）：环扫清 HELD_THROTTLE + 带成员 VD 按 **`band.high_weight`** 重臂（回场竞争给早截止——高权带回场不排尾）。v1.11 随迁〔e648147a〕。

### `TaskClass::grant_cycles(&self) -> u64`
> class | 授片量（per-task 分派——M4 并线第五刀）：`Unit` = 全局 `slice_cycles`（**零漂移**——现役所有非 band 任务不受影响）；`Band` = 武装槽 quota 镜像优先〔v1.14 机会四①——`quantum_cached` 经 `quota_cycles_of`，免每授片 cntfrq MRS+u128 软除法；未武装回落现算=本带 quantum_ns 的周期数，懒武装路径两值恒等；显式 configure 下授片跟随槽 quota〕（band 特有配额替代全局片——`sched.rs` 八 Band 可达授片点 + idle Unit 授片消费，interactive 短片/utility 长片的差异化源头）。v1.11 随迁〔e648147a〕。
### `TaskClass::grant_cycles(&self) -> u64`
> class | 授片量（per-task 分派——M4 并线第五刀）：`Unit` = 全局 `slice_cycles`（**零漂移**——现役所有非 band 任务不受影响）；`Band` = 武装槽 quota 镜像优先〔v1.14 机会四①——`quantum_cached` 经 `quota_cycles_of`，免每授片 cntfrq MRS+u128 软除法；未武装回落现算=本带 quantum_ns 的周期数，懒武装路径两值恒等；显式 configure 下授片跟随槽 quota〕（band 特有配额替代全局片——`sched.rs` 八 Band 可达授片点 + idle Unit 授片消费，interactive 短片/utility 长片的差异化源头）。v1.11 随迁〔e648147a〕。
### `TaskClass::grant_cycles_of(&self, t: &Task) -> u64`
> class | per-task 授片面（预算授片分派刀 2026-11-19）：`dl_grant`（sliceface 律——deadline ∧ preempt_threshold>0 → Some(budget)）命中返 budget，否则回落 [`Self::grant_cycles`] class 面。受预算执法任务集==受预算授片任务集（与 throttle_admissible 前两合取协调）；budget<=0 回落非断言（ktest 手造/生产 budget_ns=0 合法态——0 授片=语义僵尸，回落消解）。八授片点+idle 统一单习语消费。


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

### `band_of(task_key: u64) -> u8`
> policy | band 选择位读出口（v1 纯预留：写路 `snap_insert` 恒 `BAND_SEL_NONE`，与接口缺席等价；v3 `policy_bind` 开 band 位真值写入后即 task→band 赋值的策略覆盖通道，消费面 `class.rs::class_fair_intent` 同批。ADR #21 带内纪律：band/intent 不入 AI 动作空间）。读纪律同 `placement_hint`。

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

### `quota_cycles_of(band: u8) -> Option<u64>`
> band | 武装槽 quota 镜像读（04 号审计机会四①，2026-11-17）：已武装（quota∧period 均正——`armed` 单源判）→ Some(quota)=configure 写入的冷配置镜像；未武装/越界 → None（调用方回落现算）。读法=band_on 同款（屏蔽窗契约）；不进 lib.rs re-export——消费面在 class.rs quantum_cached 私有形（grant/rearm 三点）。显式 configure≠表行时授片跟随槽 quota（有意语义——与 arm「显式 configure 优先」同向，M4 启动配置覆盖路径须记名）。

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

### `any_armed() -> bool`
> band | 是否存在任一武装槽（pick 扫描的 lazy 执法门——零武装时逐节点零成本；Relaxed 纯统计门，同 `DEADLINES` 口径）。v1.11 随迁〔5131ebcd，经 c3717d95c 跨根收口入 base〕。

### `nearest_throttle_wake() -> Option<u64>`
> band | 最近限流带截止（SMP-01 F5 强制让点的醒源面，2026-11-16）：`armed` 且 `throttled` 镜像真的槽中 `DEADLINES` 最小值——即最早「翻期解限」时刻。无（未武装/无一在限流）→ None。消费点 = `sched.rs::nearest_deadline`/`nearest_deadline_park`（与 SLEEPQ 队头 min 合并；醒源刀起 park 形另并逐任务第三源）：F5 强制让点后停泊核在带翻期时刻自醒（tick → `band_period_sweep` 翻期解限 → pick 回场）——缺此面则深 park（mask CNTV 无 tick）把「让点」变「永久饿死」。读序：`throttled` 镜像先行、`DEADLINES` Acquire 后随（与翻期「清镜像先行 + CAS(Release) 发布」配对）。v1.11 随迁〔02634905，经 c3717d95c 入 base〕。

### `lazy_enforce(t: *mut Task)`
> band | 单成员 lazy 执法（pick 扫描逐节点调；调用契约 = 本核 RQ 锁/屏蔽窗内、`t` 为本环在环任务）。未武装槽/deadline 豁免（`band_member` 单源）→ 零动作。双窗门 `any_armed` 在扫描外层。v1.11 随迁〔5131ebcd〕——SMP-01 F1 修（CAS 输家核残留限流位深 park 无 tick 结构性饿死）。

---

## 四、EAS 能效（em.rs + util.rs + sched.rs EAS 放置面）

EM 频点表（形状不落算法）+ per-CPU util EWMA + overutilized 回退判据 + EAS 放置决策/现役面（v1.10 起含 `eas_pick_place`/`eas_place`——06 号 §5 水通管道②首接，2026-11-16）。

### `em_register(domain: usize, cpu_mask: u32, points: &[EmPoint]) -> bool`
> em | 注册/重注册某域 EM 表（boot/ktest 期单写者；真机 DT 解析后调用）。

### `em_valid(domain: usize) -> bool`
> em | 域是否已注册有效 EM 表（Acquire 载入——true 保证表内容已同步可见）。

### `em_domain(domain: usize) -> Option<EmDomain>`
> em | 拷出某域 EM 表快照（ktest/消费者观测面；未注册/越界 → None）。

### `em_max_capacity(domain: usize) -> u32`
> em | 最高频点容量（overutilized 阈值/选核的容量基准；未注册/越界 → 0）。

### `em_max_capacity_of_cpu(cpu: usize) -> u32`
> em | 本核所属域的最高频点容量（CP-09 EAS M2 域修，dcf359c5）：多域（大小核）拓扑下各域容量各异，「本核相对本域容量」才是 overutil 语义——修域 0 硬编码（此前 `em_max_capacity(0)`，单域 QEMU 零漂移、多域语义错）。域归属扫描与 `eas_pick_cpu`/`eas_pick_place` 候选域成员判同口径（首含位域）；cap 0 语义归 `overutil_judgment` 钳 1（无据保守回退）。消费点 = `util::overutilized`。

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

### `util_idle_fold(cpu: usize)`
> util | park 入口折一窗（SMP-01 修正：parked 核 util 冻结喂跨核读者，5131ebcd）。缺口：深 park（mask CNTV）后本核无 tick = `util_tick_window` 的折叠永不发生——EWMA 冻结致 `any_overutilized`（EAS 全域回退门）恒判 overutil、对核 `eas_place` utils 快照失真，EAS 选核整域死亡。修法 = park 入口无条件折一窗（accum 取走真值、elapsed = 距上次翻窗实差；数学面与 `util_tick_window` Fold 分支同源〔`ewma_fold` 单源〕，仅去「窗口到期」门）。调用契约：本核 IRQ 屏蔽窗内、只触本核槽。v1.11 随迁〔5131ebcd，经 c3717d95c 跨根收口入 base〕。

### `eas_pick_place(domains: &[EmDomain], utils: &[u32], hint: u8, task_util: u32) -> Option<usize>`
> sched | EAS 放置决策（**纯决策面**——输入全注入，ktest 钉板对象）：placement hint → `berry_em` `eas_pick_cpu` 的组合律。判序：①hint 词汇执法（0 无意见/越域值 → None 不进 EAS 面；{1=偏 little, 2=偏 big} 在此消费面收口）；②overutil 系统级门控（任一候选域任一成员核 util 超域顶容量 80% → None，判定律单源 `berry_emface::overutil_judgment`）；③hint → prev_cpu 映射（1→CPU0/2→CPU1）享平局保护（「只在严格更省时迁移」= 偏好非钉死）；④`eas_pick_cpu` 两段选核（EM 无据/放不下 → None 同回退）。

### `eas_place(task_key: u64) -> Option<usize>`
> sched | EAS 放置（**现役面收集** + 决策——`place` 的 SMP 消费面，ktest 直调面；par 门在 `place` 不在此）。三源真值：EM 域表（`em_domain`——boot `em_register_qemu_virt` 注册）+ per-CPU util EWMA（`cpu_util`——PM 频率信号同源读面）+ policy placement hint（overutil 门控在彼，本面快照再判）。hint 无意见/overutil/EM 无据/放不下 → None（`place` 回退现役 K=2 启发式）。

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

### `Task::new_user_with_as(entry_va: usize, arg: usize, arg1: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize) -> Box<Task>`
> task | 用户线程版（2b-4 首个 EL0）：`entry_va`/`sp0_va` 为用户 VA（须已映射）。

### `Task::try_new_user_with_as(entry_va: usize, arg: usize, arg1: usize, sp0_va: usize, name: &'static [u8], ttbr0: usize, asid: u16, proc_tag: usize) -> Option<Box<Task>>`
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

### `Runqueue::int_count(&self) -> usize`
> runqueue | 机会五③密度账本读：interactive(0x02) 在环数（消费面 = 偷取快照两 load 之一）。Relaxed 免锁读同 [`Self::len`] 口径：跨核读者不撕裂，本核锁窗内读者值稳定。

### `Runqueue::util_count(&self) -> usize`
> runqueue | 机会五③密度账本读：utility(0x03) 在环数。口径同 [`Self::int_count`]（命名消歧见字段注——与 utilization EWMA 族无涉）。

### `Runqueue::resync_density(&self) -> (usize, usize)`
> runqueue | 机会五③（2026-11-19）全环重数两带在环数并回写账本（洗账面——仅测试/诊断用，照 [`Self::resync_vruntime_sum`] 纪律；生产路径 intent 变更全经 link/unlink/`density_reclass` 入账；调用契约同 [`Self::ring_check`]——环变更屏蔽窗内或其串行等价）。

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

intent（u8）第一类输入：六档词汇表（ADR #58）+ send/recv 两侧结算 + 新任务缺省档产品级覆盖（G5 boot-params）。

### `intent_valid(v: u8) -> bool`
> intent | intent 值合法域（0x00–0x05；入口治理随声明入口 syscall 落地，判别先冻结）。W3-Q13 起判定律单源 `berry_intface::intent_valid`，intent.rs 转为 `pub use` re-export（`berry_sched::intent_valid` 调用路径不变）——本地 pub fn 计数 3→2。

### `set_intent_default(code: u8)`
> intent | 设置新任务缺省 intent（boot 早期、首个 spawn 前；编码域执法归调用方——词表映射在 bootparams::parse）。G5（§1.6-补，345739e1）`intent_default` boot-param 消费注入面。

### `intent_default() -> u8`
> intent | 读新任务缺省 intent（`Task` 装配点消费——task.rs `try_assemble`）；缺省 `INTENT_UNSPECIFIED` = 现役行为零漂移（产品显式声明才发生变更）。

### `set_timer_slack_ns(ns: u64)`
> intent | 设置系统缺省 timer slack 纳秒值（boot 早期；值域 [0, 999_999_999] 执法归 bootparams 词表——本面信任调用方）。CP-11（§3.8 机制面·2026-11-15，988944e09）——boot-params 第三键 `timer_slack` 的内核全局存（slackface `PolicyTriple.timer_slack_ns`）。缺省 0 = 无松弛 = 现役行为零漂移（空 params 镜像逐位不变——`INTENT_DEFAULT` 同款合同）；原子 Relaxed：boot 单写者 + 之后 `block_current` 多核只读。

### `timer_slack_ns() -> u64`
> intent | 读系统缺省 timer slack（`block_current` 消费点）。

### `timer_slack_defer(intent: u8, deadline_vct: u64, slack_ns: u64, freq: u64) -> u64`
> intent | timer slack 的 LATE-only 顺延判定律消费面（纯函数；判定律单源 `user/berry-slackface`——L1 late-only / L2 最紧不越两律）。语义：`intent` 档 **>= UTILITY**（UTILITY/BACKGROUND/MAINTENANCE 三档）才顺延；**交互零牺牲**（INTERACTIVE/LATENCY_CRITICAL 不加）+ 未声明零漂移（UNSPECIFIED 不加）；保留域（0x06..）保守回 exact。数学面：零 slack / 异常频率 / 无限等哨兵（`u64::MAX`）恒等；否则 ns→tick **截尾**换算（实际顺延恒 ≤ 声明 slack，LATE-only 永不提前），顶域夹 `u64::MAX - 1`（不回绕、不撞无限等哨兵）。

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

## 九、oneshot 收紧律（oneshot.rs）

忙核 arm-to-nearest-deadline（CP-17 前置子件，c3022903b）——tick/授片两臂共用的纯决策律单源，host 钉板 `sched/host/oneshot_law_test.rs`（`just test-oneshot`，rustc 直编同源编译）；无 HAL/asm 依赖，纯 core 面。v1.8 补漏登（A 线审计 §4——v1.7 升版时该文件在途未并）。

### `tighten_deadline(nearest: Option<u64>, armed: u64) -> Option<u64>`
> oneshot | 收紧律：已知最近到期超时 `nearest` 与现编程截止 `armed`，**仅当 `nearest` 严格早于 `armed` 才收紧**（`Some(d)`），否则 `None` 保持现臂（无到期/不更早 → 周期 tick 兜底已在场）——方向性与 `arm_slice` 的 S-6 单调夹取同向，只在「更早」方向移动、永不推迟；`nearest ≤ now` 过期值合法（compare 立即成立即醒，回环 `expire_timeouts` 承接）；`nearest == armed` 同值幂等不重臂。消费臂两处（sched.rs）：`on_irq_tick` 的 `expire_timeouts` 之后（tick 处理器 oneshot 化）+ `block_current` 忙核授片臂——两臂合璧后 CVAL 恒 ≤ 最近已知截止，睡眠唤醒粒度从 1 slice 收紧到真 deadline（R-1 δ 塌缩 16.5-33.1× 实测在册）。

---

## 十、观测统计（stats.rs）

per-band picks/runtime + per-intent picks 计数面（OBS-01，5131ebcd——SMP 双核验证批观测面，2026-11-16 经 c3717d95c 跨根收口入 base；v1.11 补登——第 14 文件，该文件 v1.10 时点尚不存在）。读出口纯计数（Relaxed 累加），telemetry/DUMP 消费面预留。

### `band_picks(band: u8) -> u64`
> stats | per-band pick 计数读出口（telemetry/DUMP 消费面预留）。

### `band_runtime(band: u8) -> u64`
> stats | per-band 累积 runtime 读出口（CNTVCT 周期数）。

### `intent_picks(intent: u8) -> u64`
> stats | per-intent pick 计数读出口。

---

## 附：自旋锁（spin.rs）

`SpinLock`（TTAS：CAS 入主，失败转廉读自旋让出总线）——lib.rs 未列 `pub use`，经 `berry_sched::spin::` 路径可达。


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
| sched.rs | 94（v1.20：w38 F1 保命批 +1——`sq_is_held`〔01a194159 SLEEPQ 持有查询面——kernel 跨层析构锁序判别，`sq_lock` 姊妹查询形〕；v1.19：遥测 op=4 刀 +1——`for_each_inflight_task`〔2310da74c 全体在役三并集遍历，0x84 op=4 快照基座，`for_each_runnable` 姊妹形〕；v1.18：机会五③密度账本化 +2——`density_reclass_kt`/`dens_on_ring_walk_kt`〔e547df7da ktest 跨 crate 出口对，生产唯一调用者 settle 锁包臂/integrity 金丝雀〕；v1.15：醒源刀 `nearest_dl_throttle_wake`+`nearest_deadline_park` 两 fn，含重数勘正；v1.10：水通管道〔10297ac22〕+2——`eas_pick_place`/`eas_place`〔EAS 放置面，归四〕；v1.11：跨根收口 +2——`current_task_name`〔cbox 读面，037ea2e1〕/`throttle_yield_due`〔F5 限流垄断者强制让点判据，02634905〕；v1.12：F3/F4 +3——`try_rq_lock`/`try_rq_unlock`〔0d02a3db 非阻塞 RQ 锁〕/`steal_band_rank`〔c1f695be F4 band 感知偷取排序〕） | 一、调度核心（EAS 放置双 fn 归四） |
| policy.rs | 17（v1.12：F4 +1——`placement_hint_ungated`〔c1f695be ungated 直读出口，消费面接线在途〕） | 二、策略服务 |
| band.rs | 11（v1.14：机会四 `quota_cycles_of` +1〔武装槽 quota 镜像读〕；v1.11：跨根收口 +3——`any_armed`/`lazy_enforce`〔5131ebcd SMP-01 F1 lazy 执法门〕/`nearest_throttle_wake`〔02634905 F5 醒源面〕） | 三、层级预算 |
| em.rs + util.rs | 7 + 8（+sched.rs EAS 放置 2 fn v1.10 起归本组；v1.11：util.rs +1——`util_idle_fold`〔5131ebcd SMP-01 F2 park 核 util 冻结修〕；v1.12：em.rs +1——`em_max_capacity_of_cpu`〔dcf359c5 CP-09 overutil 域修〕） | 四、EAS 能效 |
| hdr.rs | 8（+`pub use` re-export 族：`bucket_of`/`bucket_upper`/`rank_of`/`Hdr`·`CpuHdr` 全方法单源 `berry_hdrface`） | 五、测量 |
| task.rs | 5 | 六、任务 |
| runqueue.rs | 10（v1.18：机会五③密度账本化 +3——`int_count`/`util_count`〔e547df7da 密度账本读对，偷取快照两 load 消费〕/`resync_density`〔洗账面——仅测试/诊断用〕；v1.13：退役的 `has_eff_above_filtered` 系 `pub(crate)` 不在机提口径——计数不动） | 七、环形队列 |
| intent.rs | 7（+`pub use` re-export 族：`intent_valid` 单源 `berry_intface`；G5 前 2——timer slack 三 fn 前为 4，v1.9 补 CP-11 族） | 八、intent |
| spin.rs | 4（v1.16：探针族退役 −1 set_spin_bark；v1.12：F3 +1——`SpinLock::try_acquire`〔0d02a3db 非阻塞尝试获取〕） | 附 |
| oneshot.rs | 1 | 九、oneshot 收紧律（v1.8 补登——c3022903b） |
| stats.rs | 3（v1.11 补登第 14 文件——`band_picks`/`band_runtime`/`intent_picks`〔5131ebcd OBS-01 观测面〕） | 十、观测统计 |
| class.rs | 18（v1.17：grant_cycles_of +1（+trait 5 法；v1.13：机会一② +1〔`pick_next_preempt`——末笔合并 1e7872196 形胜替换为 W3-Q93 `pick_next_if_above`，现册即该 fn〕；v1.10：水通管道〔10297ac22〕+4——`class_of_task`/`FairBand::yield_slice_return`/`TaskClass::yield_slice_return`/`TaskClass::band`；v1.11：跨根收口〔e648147a 第五轮终验〕+8——`FairBand::quantum_cycles`/`period_cycles`/`arm`/`consume`/`period_check`/`throttle_all`/`unthrottle_all` + `TaskClass::grant_cycles`） | 一、调度核心 |
| lib.rs | 0（`pub use` 根） | — |
| **合计** | **193** | ｜ 含 `unsafe` 8、`const` 4、`extern "C"` 4（v1.20：w38 F1 保命批 01a194159 sched.rs +1 `sq_is_held`〔全 safe 非 const——192→193〕；v1.19：遥测 op=4 刀 2310da74c sched.rs +1 `for_each_inflight_task`〔全 safe 非 const〕；v1.18：机会五③密度账本化 e547df7da +5——runqueue 密度账本读对/洗账 + sched ktest 出口对，全 safe 非 const〔→191〕；v1.17：预算授片 class +1〔→186〕；v1.16：探针族退役 spin −1〔→185〕；v1.15：醒源刀 sched +2〔185→186，含机提重数勘误〕；v1.14：机会四 band +1 `quota_cycles_of`〔182→183〕，全 safe 非 const；v1.13：机会一② class +1 `pick_next_preempt`〔181→182〕——退役的 `has_eff_above_filtered` 系 `pub(crate)` 不计，runqueue 7 不动，新 fn safe 非 const；v1.12：SMP-01 F3/F4 + CP-09〔0d02a3db/c1f695be/dcf359c5〕+6 fn 后 181——`spin::try_acquire`/`sched::try_rq_lock`/`try_rq_unlock`〔F3 撕裂窗〕+ `sched::steal_band_rank`/`policy::placement_hint_ungated`〔F4 偷取/wake〕+ `em::em_max_capacity_of_cpu`〔CP-09 域修〕，六 fn 全 safe 非 const；系统审计 S2 补录五条已并入，180 系漏计 CP-09 一 fn；v1.11 的 175 为 F3/F4/CP-09 前时点；v1.11：跨根收口〔c3717d95c，2026-11-16〕+17 fn 后 175——`class_of_task` 前十五笔见版本行；e648147a〔class.rs 8→16〕+ 5131ebcd〔band.rs 7→10 + stats.rs 新档 3 + util.rs 7→8〕+ 02634905〔sched.rs 82→84〕+ 037ea2e1〔sched.rs `current_task_name`〕，十七 fn 全 safe 非 const；v1.10 的 158 为跨根收口前时点；v1.10：水通管道〔10297ac22，2026-11-16 经 63fbca1c9 合流入 base〕+6 fn 后 158——`class_of_task`/`FairBand::yield_slice_return`/`TaskClass::yield_slice_return`/`TaskClass::band`〔class.rs 4→8〕+ `eas_pick_place`/`eas_place`〔sched.rs 80→82，归四〕，全 safe 非 const；v1.9 的 152 为水通管道前时点；v1.9：调度深水区收官〔988944e09〕`intent.rs` +3 fn——`set_timer_slack_ns`/`timer_slack_ns`/`timer_slack_defer`〔CP-11 §3.8 系统缺省 timer slack 族，全 safe 非 const〕后 152——v1.8 的 149 为 timer slack 前时点；v1.8：补登 oneshot.rs `tighten_deadline`〔第 13 文件，safe〕后 149——v1.7 的 148 为该件在途未并时点，A 线审计 §4 收口；v1.7：调度终窗〔01cd881b〕+4 fn 后 148——`spawn_user_env`〔sched.rs，safe〕/`band_of`〔policy.rs，safe〕/`FairBand::new`〔class.rs，const——const 3→4〕/`class_fair_intent`〔class.rs，safe〕+ `try_prepare_user`/`try_spawn_user` 签名扩 `arg1` 参净计数不变；v1.6 的 144 为终窗前时点；v1.6：P2-2 退出异步化 +1 fn〔`set_reap_post_hook`，safe 非 const〕后 144——v1.5 的 143 为退出异步化前时点；v1.5：PI 形 B〔42a794e0〕+5 fn + G5 intent 缺省档〔345739e1〕+2 fn 后 143——v1.4 的 136 为形 B/G5 前时点，新增 7 fn 全 safe 非 const；v1.4：daa5ef7e perf(pi) 全图重推门控 +2 fn 后 136——v1.3 的 134 为门控前时点；v1.3：W3-Q16 hdrface 外迁后 134——v1.2 的 146 为外迁前时点，const 6→3 = `bucket_upper`/`Hdr::new`/`CpuHdr::new` 三 const 随迁；v1.2：W3-Q13 `intent_valid` 外迁 re-export 后 146，v1.1 的 147 为外迁前时点；const 6 系 v1.1 勘误，v1.0 笔误 5 不回传；W3-Q6 起含 `intent_settle_current_ext`） |

### 补录（2026-11-17 系统审计 S2——34cbd255b 后漏同步五条；v1.12 已并入统计口径，em 域修条目归 §四）

> **签名勘正（2026-11-17 车队窗遗漏审查 wf_c99667d9-89a 捕获）**：本补录块原四条签名（`steal_band_rank`/`try_rq_lock`/`try_rq_unlock`/`placement_hint_ungated`）系按名凭空拼形——各 fn 自引入笔起源码从未长成所抄形状（`git show` 逐笔核对坐实），已按 `sched/src/` 现行签名回改；v1.12 头注「六 fn 全量复核」对补录块失实，同批勘正。两条正确者（`SpinLock::try_acquire`、§四 `em_max_capacity_of_cpu`）复核维持。

### `steal_band_rank(band: u8, tgt_int: usize, tgt_util: usize) -> u8`
> sched | F4 band 感知偷取的 band 级排序（STEAL_RANK_COMPAT/DEPRIOR/PROTECT 三常量锚）——偷取候选环排序单源。

### `try_rq_lock() -> Option<bool>`
> sched | F3 非阻塞 RQ 锁（band tick 环扫撕裂窗治理）：可用→扫 / 不可用→跳过，永不 wedge。

### `try_rq_unlock(phys: bool)`
> sched | `try_rq_lock` 的配对解锁。

### `placement_hint_ungated(task_key: u64) -> u8`
> policy | F4 ungated 直读出口。〔审计注：写面在库、声称的消费点 eas_wake_pick 实未读 hint——读面接线不存在，现状如实；接线与否归调度 owner 裁量〕

### `SpinLock::try_acquire(&self) -> bool`
> spin | 自旋锁非阻塞尝试获取（F3 家族）。
