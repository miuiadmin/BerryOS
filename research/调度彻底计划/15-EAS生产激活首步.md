# 15 号：EAS 生产激活首步——placement hint 生产消费面（任务 B）

> 实测日期：2026-11-16。环境：Apple M3 Pro / QEMU 11.0.3 Homebrew TCG / 单核 `virt`（ktest 面）+ `-kernel` + rootfs fw_cfg 双卷（EL0 面）。任务书：FairBand 后验证 + EAS 激活双刀的 B 刀——「EAS 生产激活首步:placement hint 的生产消费者——最小面 = berry-power 读 util(syscall 0x40)→调 policy_bind(0x83, placement hint)→EAS place() 消费」。本件落三件事：①侦察定谳（placement hint 的真 ABI 形态 + 0x83 能力门的 EL0 现实）；②实施（berry-power EL0 产面接线 + ktest EASPROD 门内正向链钉板）；③验证实跑（host 33 测 / QEMU ktest EASPROD ok + 裸相 32 标记全到 / run-rootfs 产面证据行）。推断一律显式标注。

---

## §0 结论速览（四条）

1. **placement hint 不是独立 op**——任务书预设「op=placement, p1=cpu_hint」不成立：真形态是 0x83 COMMIT（op=1）**表条目字段** `PolicyEntryABI.placement: u8`（[kernel/src/syscall.rs](../../kernel/src/syscall.rs):2732），经整表提交入 policy 快照；词汇 {0=无意见, 1=偏 little(CPU0), 2=偏 big(CPU1)}（[sched/src/policy.rs](../../sched/src/policy.rs):457-464）。产面正确形态 = STATUS(op=2) 读 gen 对账 → COMMIT(op=1) 携 `{key, placement}` 条目 + 严格递增 gen。
2. **EL0 正向提交被能力门挡住（立法留窗，非本刀可开）**：0x83 全 op 面挂 Process **MANAGE** 三门控（[kernel/src/syscall.rs](../../kernel/src/syscall.rs):2822-2825，arch-r2）；berry-power 由 svcmgr 以 `stdin_feed: None, stdin_handle: None` 预拉（[user/bins/svcmgr/src/main.rs](../../user/bins/svcmgr/src/main.rs):1132-1133）→ berry-exec 的 env 块自描述对（gift[2] = self-proc MANAGE）**条件装配不触发**（[user/berry-exec/src/lib.rs](../../user/berry-exec/src/lib.rs):772）→ berry-power 表内无 Process handle → auth=0 恒 -EBADF。这正与 policydemo 立法注一致：「用户态正向链待 spawn 链授 Process MANAGE handle（S6-c）后回归」（[user/bins/policydemo/src/main.rs](../../user/bins/policydemo/src/main.rs):23-25）。
3. **产面接线完成、授权待窗形态落地**：berry-power main 尾段装上完整生产链——util(0x40) → `UtilTrack::placement_hint`（低核偏好，纯函数 + host 钉板）→ `self_proc_h()` 为 auth 的 STATUS+COMMIT 提试 → 一行证据诊断后 park。当前 auth=0 → 双拒 -EBADF = **零行为变化**（与 CP-08「拉起零行为变化」纪律连续）；handle 递达后**同一链路零改动转真提交**。
4. **门内正向全链钉板 = ktest EASPROD**（[kernel/src/ktest.rs](../../kernel/src/ktest.rs) `test_eas_prod`，标记 `EASPROD ok`）：真 SVC 过 MANAGE 门（process_create 出厂 handle）→ STATUS gen 双半区对账 → COMMIT 16B ABI 条目受理 → `policy::placement_hint` 透传 → **util 真值驱动的 hint 经 `eas_place` 消费落低核**（[350,0]+128 底景：保档 vs 跨档）→ 镜像翻转随输入翻 → overutil 门控回退 None（产面不预判、消费面内核收口）。place() 路由面（par 门）ktest 恒关，归 SMP 相——`eas_place` 即 place() 的 EAS 臂委托点（[sched/src/sched.rs](../../sched/src/sched.rs):762），决策面在 ktest 全证。

## §1 侦察定谳

### 1.1 placement hint 的 ABI 真形态（三面同律镜像）

- **内核面**：`sys_policy_bind` op=1 COMMIT，p1 = `&[PolicyEntryABI]`（16B：key@0 u64 / weight@8 i32 / **placement@12 u8** / epp_flags@13 u8 / _rsv@14 u16），p2 = 条目数，p3 = gen（陈旧 ≤ 活跃世代 → -EAGAIN），p4 恒 0（[kernel/src/syscall.rs](../../kernel/src/syscall.rs):2759-2804、解析 :2944-2992）。op=2 STATUS 出 packed（低 32 = active gen / 高 32 = shadow gen）+ self_key（[kernel/src/syscall.rs](../../kernel/src/syscall.rs):2871-2868）。op 词汇 1..6（COMMIT/STATUS/PROMOTE/DEMOTE/UNBIND/SHADOW_COMMIT），**无 placement 独立 op**。
- **用户态面**：`berry_sys::PolicyEntry` 同 16B `#[repr(C)]` 镜像 + 偏移钉板 assert（[user/berry-sys/src/lib.rs](../../user/berry-sys/src/lib.rs):551-573）；`POLICY_OP_COMMIT=1`（:588）。
- **语义面**：placement 三值词汇在**消费面**执法——`eas_pick_place` 非 {1,2} 一律按无意见（[sched/src/sched.rs](../../sched/src/sched.rs):819-822）；`policy::placement_hint` 读出口带 overutil 门控（任一核 > 域顶容量 80% → 恒 0，[sched/src/policy.rs](../../sched/src/policy.rs):471-487）。

### 1.2 消费链全景（产→表→放置）

```text
berry-power(EL0)                          内核
─────────────────                         ─────────────────────────────────────────
berry_sched_util_snapshot(0x40)  ──读──►  cpu_util(c)（per-CPU EWMA，sched/src/util.rs）
UtilTrack::placement_hint()               │（place() 面：spawn 时 spread 任务）
  低核偏好 {1,2}                           ▼
berry_policy_status(0x83,op=2)   ──门──►  MANAGE 三门控 → gen 对账
berry_policy_bind(0x83,op=1,     ──门──►  policy_tbl_parse → policy::bind_gen
  {key,placement=hint},gen+1)             │（快照发布，三槽 RCU）
                                          ▼
                              spawn → place()（sched.rs:757，par/spread 门）
                                          → eas_place(key)（sched.rs:844）
                                          → policy::placement_hint(key)（读出口+overutil 门）
                                          → eas_pick_place（词汇执法→prev 映射→berry-em
                                            eas_pick_cpu 两段选核：能量 diff 最小胜、
                                            prev 平局保护）
```

关键行号锚：place() EAS 首接 [sched/src/sched.rs](../../sched/src/sched.rs):757-772；eas_place 三源收集 :844-868（policy 表 × EM 表 × util EWMA）；能量律 `energy_diff_uw` 候选核 util = **求和** `u+task_util`、域 OPP 按 max util 选档（[user/berry-em/src/lib.rs](../../user/berry-em/src/lib.rs) `energy_diff_uw`）；QEMU 双核表 400/750/1024 三档。

### 1.3 EL0 正向通道核查（为何 auth=0）

- 出权面：`process_create` 给**创建者**表插 Process handle（MANAGE|DUPLICATE|TRANSFER，[kernel/src/syscall.rs](../../kernel/src/syscall.rs):1549）；子进程获自持 Process handle 的唯一通道 = berry-exec env 块 gift[2]（MANAGE dup，[user/berry-exec/src/lib.rs](../../user/berry-exec/src/lib.rs):773）——**条件 = stdin 在位**（:772）。
- svcmgr 预拉 berry-power 无 stdin（[user/bins/svcmgr/src/main.rs](../../user/bins/svcmgr/src/main.rs):1132-1135 `stdin_feed: None, stdin_handle: None`）→ gift[2] 不装 → `self_proc_h()==0`（[user/berry-libc/src/io.rs](../../user/berry-libc/src/io.rs):442-445）。
- 结论：berry-power 当前形态 0x83 恒 -EBADF。**这不是缺陷是立法态**——policydemo 已把「双拒恰 -EBADF = 门在位活证」立为用户态判据（[user/bins/policydemo/src/main.rs](../../user/bins/policydemo/src/main.rs):60-81）；本刀沿同判据。
- 备选通道评估（均不取，最小侵入）：①svcmgr 持各子 Process handle 可代提交——但那是 svcmgr 的策略权面（跨进程 placement 主张），归授权立法刀；②给 berry-power 配 stdin 触发自描述对——改 spawn 拓扑，牵动 berry-exec 前缀紧致律与 svcmgr 拉起面，超出本刀。

## §2 实施（三件落码）

### 2.1 berry-power lib：`UtilTrack::placement_hint`（纯函数产面）

[user/bins/berry-power/src/util.rs](../../user/bins/berry-power/src/util.rs) `impl UtilTrack` 新增方法：`est(0) <= est(1) → 1（偏 little）/ 否则 2（偏 big）`——「把 util 高的核避开」。纪律三件：①overutil 预判与词汇执法**不在产面**（内核消费面收口，方法注明）；②平局偏 little = 推断标注（省电核惯例，QEMU 对称表无实际差异）；③ktest EASPROD 侧是**独立副本**（故意不复用用户态 crate——两份独立实现互为对抗性对拍，单侧漂移即断言炸）。host 新测 `placement_hint_avoids_busy_core`（三态 + 平局 + 深空闲死区 + 快照通道同律），berry-power host 测 32→**33**（[justfile](../../justfile) test-host 行注释同步）。

### 2.2 berry-power bin：产面一-shot 接线（main 尾段、park 前）

[user/bins/berry-power/src/main.rs](../../user/bins/berry-power/src/main.rs) main 尾段新增：读 `core.util.placement_hint()` → `self_proc_h()` 为 auth → `berry_policy_status`（gen 对账 + self_key）→ `berry_policy_bind(COMMIT, {key:self_key, placement:hint, weight:0}, 1, gen+1, 0)` → 一行诊断 `berry-power: eas hint={} key={} st={} bd={} auth={}` 后照旧 park。设计裁量：

- **一-shot 非周期轮询**：park 是 10s 睡环（[user/berry-libc/src/lib.rs](../../user/berry-libc/src/lib.rs):191-199），周期循环会每 10s 刷一行诊断污染冒烟串口；窗 3「常驻循环周期轮询面」本就留窗（随窗 2 recv 循环同刀）——本刀只装最小链，周期面与 gen 重试协议（-EAGAIN 对账重发）同归后续刀。
- **gen 协议最小形**：STATUS 受理才读 packed 取 gen+1；门拒时携 gen=1 探测（门在表解析前，拒于门则 gen 不消费）——不写无法今日验证的重试环（未测代码 = 明日雷）。
- **key=self_key**：自建议最小面（「表里给别的进程加权前先对账自己」——STATUS self_key 设计语义，[kernel/src/syscall.rs](../../kernel/src/syscall.rs):2875-2780）；跨进程 placement 主张归授权立法。当前门拒 → self_key 读不到保持 0 = 哨兵键形态，同样诚实（0 键条目表侧本就丢弃）。
- **零漂移面**：既有 dry-run 行逐字节不动（新面独立第二行）；berry-power 不入 markers.sh 任何标记数组——无冒烟契约面扰动。

### 2.3 ktest EASPROD：门内正向全链钉板

[kernel/src/ktest.rs](../../kernel/src/ktest.rs) 新增 `test_eas_prod`（注册于 test_loadgen 后、ktest done 前——末位新号纪律；标记 `EASPROD ok`）。五段：

| 段 | 面内容 | 断言 |
|---|---|---|
| EP-0 | EM 幂等重注册 + policy 回哨兵 + util 全槽清零 | 前置态（EA 族同款） |
| EP-A | 真 SVC `process_create` → 真 SVC STATUS(op=2) | 返回 0；packed 双半区 = `gen()`/`shadow_gen()` 各截低 32 对账（前序 SHD 族可留 >32 位 gen 的 shadow 谱系——对账同掩码）；self_key == current_tag() == 0（ktest 期哨兵键诚实透出，产面以探测键 7 代行） |
| EP-B | 真 EWMA 折 CPU0→350 带（`util_accumulate`/`util_tick_window` = Fair::tick 生产折叠同款）→ 镜像律 hint=2 → 真 SVC COMMIT | 受理 0；gen 推进；`placement_hint(7)`==2；**`eas_place(7)`==Some(1)**（低核；[350,0]+est128：加 CPU1 保档 opp0 vs 加 CPU0 跨档 opp1——EA-6 数值锚镜像臂） |
| EP-C | 对侧清场折 [0,350] → hint=1 → 再 COMMIT（gen+1） | `eas_place(7)`==Some(0)——决策随产面输入翻转，非常量巧合 |
| EP-D | 泵 CPU0 越阈（>819） | 产面镜像律仍 hint=2（不预判）；`placement_hint(7)`==0（overutil 门控）；`eas_place(7)`==None（EAS 机会主义让位，Linux 同律） |
| EP-E | close 双 handle + unbind + util 清零 | ktest 瞬态不外泄（H8 清场同款） |

**place() 路由面留注**：ktest 先于 smp_bringup/par_enable 跑（[kernel/src/ktest.rs](../../kernel/src/ktest.rs) test_policy_v3 同注）→ place() 的 par 门恒关、place() 又是私有 fn——ktest 消费面证到 `eas_place`（pub，即 place() 的 EAS 臂委托点，[sched/src/sched.rs](../../sched/src/sched.rs):761-763）。par/spread/SGI kick 真路由归 SMP 冒烟相（推断：smoke-smp 相无产面表绑定，路由面今日结构性无消费者——见 §4 留窗②）。

## §3 验证（全实跑）

### 3.1 host 面

- `cargo test --manifest-path user/bins/berry-power/Cargo.toml --target aarch64-apple-darwin`：**33 passed / 0 failed**（32 旧 + 1 新 `util::tests::placement_hint_avoids_busy_core`）。
- `cargo check -p berry-kernel`：零 error、ktest 新码零 warning（既有 5 warning 均前序遗留）。

### 3.2 QEMU ktest 面（裸相 boot）

`cargo build -p berry-kernel && qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 128M -nographic -kernel target/aarch64-unknown-none/debug/berry-kernel`（timeout 120s）：

```text
EAS ok            ← 既有 test_eas_place 零回归
EASPROD ok        ← 本刀新标记（首轮实跑抓出一处断言 bug：STATUS 双半区对账
                     未掩码——前序 SHD 族留 >32 位 shadow gen，packed 侧截低
                     32 而断言侧全值，修正后绿；这正是对拍断言的价值面）
ktest done        ← 全链无 panic
```

裸相 32 标记对拍（source tools/markers.sh 后逐项 grep -qF）：**ALL 32 present**。

### 3.3 QEMU EL0 面（run-rootfs）

`CARGO_TARGET_DIR=<仓内> just run-rootfs`（含 docs-guard + abi-check + build-user + mkcpio/tlsscan 门禁，timeout 900s）：boot 至 svcmgr 预拉 berry-power，实跑串口（out/logs/run-rootfs-*.log，诊断行非标记——雷册 §A145 口径）：

```text
EASPROD ok                                        ← ktest 面在 rootfs 相同 boot 亦过
ktest done
eco: berry-power up
berry-power: skeleton dry-run applied=4 rejected=0 acks=4 util=127@k ...（既有行，逐字节不动）
berry-power: eas hint=2 key=0 st=-9 bd=-9 auth=0  ← 本刀新行（实跑）
```

逐字段判读：`hint=2` = 真实数据的正确产出——同 boot 的 dry-run 行自证 `util=127@k`（0x40 内核快照：CPU0 EWMA 127 > CPU1 恒 0 → 避忙核 CPU0、偏 CPU1），低核偏好律在真值上方向正确；`key=0` = STATUS 被门拒、self_key 读不到（哨兵形态诚实透传）；`st=bd=-9`（-EBADF）= MANAGE 门在位的 EL0 活证（policydemo 双拒同判据）；`auth=0` = 自描述对未装（svcmgr 无 stdin 拉起形态）。handle 递达后同链路 `st=0 bd=0` 即真提交——形态不变、只换授权。

## §4 留窗与后续刀

1. **EL0 授权立法刀（S6-c 前置）**：spawn 链授 Process MANAGE handle（policydemo 注的在册留窗）。两形态待裁：svcmgr 拉 PM 双服务时递 stdin 触发自描述对（berry-exec gift[2] 既有装配面，零内核改动），或 Job 对象落地后细粒度 SET_POLICY 权。落地日 berry-power 零改动转真提交，gen 重试协议（-EAGAIN 对账重发）随该刀补。
2. **place() 真路由证据（SMP 相）**：smoke-smp 相让 spread 任务真经 place() 路由需要①产面表在绑（被留窗①挡）②spread 用户计算线程 spawn——两条件齐前 EAS_LOG_ONCE 自证行（[sched/src/sched.rs](../../sched/src/sched.rs):858-866，par 门内才打）结构性沉默，非缺陷。
3. **周期轮询面（窗 2/3）**：berry-power recv 常驻循环 + util 周期快照 + hint 动态重提交（gen 单调递增），随 PM 通道分端执法刀。
4. **README 索引**：本 series README 索引表停在 13 号（14 号为并行 A 刀落档）——14/15 两行随并行刀合流后一并补（本刀不动 README，避双写冲突）。

## §5 引用清单（路径:行号）

| 事实 | 锚 |
|---|---|
| placement=COMMIT 表条目字段（16B ABI） | kernel/src/syscall.rs:2728-2736, 2959-2991 |
| 0x83 op 词汇（1..6）+ gen 协议 + p4 纪律 | kernel/src/syscall.rs:2759-2804 |
| MANAGE 三门控在 op 分派前 | kernel/src/syscall.rs:2816-2828 |
| STATUS packed 位布局（低 active/高 shadow） | kernel/src/syscall.rs:2904-2918 |
| placement 三值词汇 + overutil 门控读出口 | sched/src/policy.rs:457-487 |
| place() EAS 首接（par/spread 门） | sched/src/sched.rs:757-772 |
| eas_place 三源收集 + 一次性自证行 | sched/src/sched.rs:837-868 |
| eas_pick_place 词汇执法 + prev 映射 | sched/src/sched.rs:800-835 |
| 能量律（候选求和 + 域 OPP 按 max + prev 平局保护） | user/berry-em/src/lib.rs `energy_diff_uw`/`eas_pick_cpu`/`consider` |
| util 快照 0x40（cpu_util 同源） | kernel/src/syscall.rs:2615-2646 |
| self-proc gift[2] 条件装配（stdin 在位） | user/berry-exec/src/lib.rs:764-790 |
| `self_proc_h()` 访问器（0 = 未交付） | user/berry-libc/src/io.rs:442-445 |
| svcmgr 预拉 berry-power（无 stdin） | user/bins/svcmgr/src/main.rs:1123-1148 |
| policydemo 双拒判据 + S6-c 留窗注 | user/bins/policydemo/src/main.rs:18-28, 60-81 |
| ktest H8 三负例（SEC-A 门） | kernel/src/ktest.rs:1802-1862 |
| 既有 EAS ok 钉板（EA/EB/EC） | kernel/src/ktest.rs `test_eas_place` |
| 本刀 EASPROD | kernel/src/ktest.rs `test_eas_prod`（test_loadgen 后） |
| 本刀产面（lib 方法 + bin 接线） | user/bins/berry-power/src/util.rs `placement_hint`；src/main.rs main 尾段 |
