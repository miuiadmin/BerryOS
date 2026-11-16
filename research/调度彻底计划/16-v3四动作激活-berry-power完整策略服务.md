# 16 号：v3 四动作激活——berry-power 完整策略服务（POLICY4 刀）

> 实测日期：2026-11-16。环境：Apple M3 Pro 宿主（host 测试 + musl/none 产线形实编；**QEMU 零跑**——本刀会话纪律 kernel/qemu = owner 零碰，boot/smoke 证据归 owner 下一轮 build-user+run-rootfs，§4 留窗②③）。任务书：v3 全四动作激活——berry-power 扩展为完整策略服务，①placement（15 号已通）②frequency hint（EWMA→schedutil→policy_bind）③weight adjustment（±64 包络）④EPP（intent+thermal→档）四动作面。本件落三件事：①侦察定谳（四动作的真实 ABI 参数面——任务书「四动作各一次 policy_bind 不同 op 值」预设 vs 真 op/表形态；Directive 消费面 FREQ→? WEIGHT→? EPP→?）；②实施（hint 纯面 + main 壳四动作链 + svcmgr 授权落位）；③验证实跑（host 36 测 / 产线形双 bin 实编）。推断一律显式标注。

---

## §0 结论速览（五条）

1. **四动作的真实参数面（侦察定谳）**：`policy_bind` 0x83 的 op 词汇 = 1..6（COMMIT/STATUS/PROMOTE/DEMOTE/UNBIND/SHADOW_COMMIT，[kernel/src/syscall.rs](../../kernel/src/syscall.rs):2830-2901）——**没有动作对应 op**。①③④动作 = COMMIT（op=1）**表条目字段**：`PolicyEntry` 16B（key@0 u64 / weight@8 i32 / placement@12 u8 / epp_flags@13 u8 / _rsv@14 u16——:2728-2736、解析 :2944-2992）；②频率**无 0x83 载体**——动作空间立法原文「频率/热限 hint（**走 PM 服务**、不经调度路径——不在表里）」（[sched/src/policy.rs](../../sched/src/policy.rs):16-18），表槽（`Slot` 16B）物理上没有频率位。任务书「FREQ=p1 kHz?」的答案：p1 在 COMMIT 是**表指针**、频率不进表。
2. **四动作消费面（Directive 怎么被调度器吃）**：③WEIGHT → `policy::apply`/`weight_of` 写 `Task::prio_effective`（带序键——[sched/src/class.rs](../../sched/src/class.rs):43,53「pick 按 prio_effective 出选」，高 = 优带；+Δ = 提带早完 race-to-idle）；④EPP → `apply` 写 `Task::epp`（TCB 落点，[sched/src/policy.rs](../../sched/src/policy.rs):423-424；下游大小核放置/PM 消费随 M1b/M3——iprofile EppHint 单源注）；①PLACEMENT → `policy::placement_hint` 读出口 → `eas_place`（15 号已接）；②FREQ → 不经调度路径——**PM 服务自己就是消费者**（本服务 `FreqVerdict` req/eff），内核聚合点写面 = 窗 4 留窗（「调度路径零频率写点」验收线）。
3. **实施形态**：lib 新 `hint.rs` 纯面（`weight_delta`/`epp_flags`/`action_hints` 四动作一次读全；词汇/包络单源 berry-polface `WEIGHT_MAX`/`EPP_HAS`——kernel `sched/policy.rs` 同源消费零第二实现）+ `PowerCore::action_hints` 组装 + main 壳四动作链（STATUS gen 对账 → COMMIT 单条目三字段 → gen echo 回读对账 → 一行 `policy4` 诊断后 park）。host 测 33→**36**（[justfile](../../justfile) test-host 注同步）。
4. **授权同刀落位（15 号留窗①前半）**：svcmgr 预拉 berry-power 改 `stdin_feed: Some(&[])`（vblk/vnet 同款空喂+EOF）→ berry-exec 自描述对条件装配（stdin 在位即装，[user/berry-exec/src/lib.rs](../../user/berry-exec/src/lib.rs):764-790）→ gift[2] = 自身 Process handle（MANAGE|DUPLICATE）→ `self_proc_h()` 真值 → MANAGE 三门控过 → STATUS/COMMIT **真受理**（st=0/bd=0）。零内核改动、bsh 子进程同款权利形态（自持 handle 非新权类）。
5. **key = 自建议最小面**：COMMIT 条目 key = STATUS 读回的自身 `proc_tag`（v1 载体；EL0 进程 = 所属进程表指针非零，[kernel/src/syscall.rs](../../kernel/src/syscall.rs):1762 spawn_user tag 注入）——四动作条目真落表、真消费（apply 对同 key 任务写 effective/epp）；跨进程 key 主张（给别的进程加权/放置）仍归授权立法刀（15 号留窗①后半）。

## §1 侦察定谳

### 1.1 0x83 参数面：op 词汇与表条目（任务书四问的逐答）

- **op 面**：1 COMMIT（p1 = `&[PolicyEntry]` 表指针、p2 = 条目数、p3 = gen 陈旧拒收、p4 恒 0）/ 2 STATUS（p1 = *packed 双谱系 gen、p2 = *self_key 可选、p3 = *divergence 可选）/ 3 PROMOTE / 4 DEMOTE / 5 UNBIND / 6 SHADOW_COMMIT——[kernel/src/syscall.rs](../../kernel/src/syscall.rs):2759-2804。无 FREQ/WEIGHT/EPP 独立 op；「四动作各调一次 policy_bind（不同 op 值）」不成立。
- **FREQ**：不在表、不在 op——`sched/policy.rs` 动作空间四类封顶原文把②划给 PM 服务（:16-18）；0x40 `sched_util_snapshot` doc 同律（「频率裁决单点归 PM，本号只读不写、调度路径零频率写点」，:2615-2627）。berry-power 的频率面 = 自家 `FreqVerdict`（schedutil req → 聚合帽 min → eff，`decide()`）——「policy_bind(frequency 值) → 内核频率面」的内核写通道 = 窗 4 留窗（对内核 DVFS/聚合点的写请求 op 随内核聚合点 ABI 立法挂 CP-08/CP-10 刀）。
- **WEIGHT**：`PolicyEntry.weight: i32`（@8）——内核三点夹紧 ±`WEIGHT_MAX`=64（handler :2977-2978 / build `snap_insert` / 消费点 `apply`），用户态产面自律是第一道（本刀 `WEIGHT_STEP`=16 构造性在包络内）。
- **EPP**：`PolicyEntry.epp_flags: u8`（@13）——bit0 `POLICY_EPP_HAS`（带覆盖）、bit1 `POLICY_EPP_VAL`（0 = Efficiency / 1 = Perf），bit2.. 非 0 → -EINVAL（:2970）；二值词汇单源 `berry-polface`（`EPP_HAS`/`EPP_VAL`——sched/policy.rs:55-57 re-export 同源）。
- **PLACEMENT**：`PolicyEntry.placement: u8`（@12）——三值词汇 {0 无意见, 1 偏 little, 2 偏 big}，词汇执法在消费面收口（`eas_pick_place` 非 {1,2} 按无意见；overutil 门控恒 0）——15 号 §1.1 同律。

### 1.2 消费链（四动作各归其面）

```text
berry-power(EL0)                                    内核
─────────────────────                               ─────────────────────────────────
0x40 util 快照 → UtilTrack EWMA（既有）
thermal 帧 → thermal_level 遥测（既有，sim 合成源）
PowerCore::action_hints()（本刀）
  ├─① placement_hint（15 号既有）
  ├─② schedutil f≈util×fmax/capacity → FreqVerdict   （PM 域自家裁决——不经 0x83；
  │                                                   内核聚合点写面 = 窗 4 留窗）
  ├─③ weight_delta（本刀：±16 包络内）
  └─④ epp_flags（本刀：fair 起 Efficiency）
self_proc_h() = gift[2]（svcmgr 递 stdin 触发装配，本刀）
berry_policy_status(op=2) ─── MANAGE 门 ──► gen 对账 + self_key
berry_policy_bind(op=1, {key,placement,weight,      ► policy_tbl_parse → bind_gen
  epp_flags}, gen+1)                                  （三槽 RCU 快照发布）
                                                     ├─③ apply/weight_of → prio_effective
                                                     │   （带序键——Fair 带内 EEVDF）
                                                     ├─④ apply → task.epp（TCB 落点）
                                                     └─① placement_hint(key) → eas_place
 berry_policy_status 回读 gen echo（受理对账）
```

### 1.3 授权链核查（本刀为何能真提交而 15 号不能）

- 出权面：berry-exec 自描述对 gift[2] = 自身 Process handle（`RIGHT_MANAGE|DUPLICATE|TRANSFER` dup），**条件 = stdin 在位**（`stdin_pair.is_some() || stdin_handle.is_some()`，[user/berry-exec/src/lib.rs](../../user/berry-exec/src/lib.rs):772）——15 号 §1.3 已定谳，阻断点 = svcmgr 预拉 `stdin_feed: None`。
- 本刀修法：svcmgr 预拉 berry-power 改 `stdin_feed: Some(&[])`（[user/bins/svcmgr/src/main.rs](../../user/bins/svcmgr/src/main.rs):1139）——空喂+EOF（feeder 只发零长 EOF 标记即关端，berry-exec lib.rs:823-835；服务不读 stdin，EOF 滞队无害），vblk（:1014）/vnet（:1077）同款先例形态。
- 权利面自查：自持 handle 过 `get_raw(auth, Process, RIGHT_MANAGE)` 三门控（syscall.rs:2822-2825）；bsh 拉起的所有子进程（hexyl/uu 家族）本就持同款自描述对——非新权类、零内核改动。
- key 面：`spawn_user` 注入 `proc_tag = 子进程表指针`（syscall.rs:1762）→ EL0 进程非零 → STATUS self_key 真值 → COMMIT 条目不被 0 键丢弃面吃掉（policy build `KEY_EMPTY` 语义丢弃）。

## §2 实施（四件落码）

### 2.1 lib `hint.rs`（新文件——四动作 hint 纯面）

[user/bins/berry-power/src/hint.rs](../../user/bins/berry-power/src/hint.rs)：`weight_delta(util)`（≥896 → +16 / ≤128 → -16 / 中带 0）、`epp_flags(Option<u8>)`（None/0 nominal → 0 无意见；≥1 fair → `EPP_HAS` Efficiency）、`action_hints(&UtilTrack, thermal, fmax, capacity) -> ActionHints` 四字段一次读全（①③④消费同一 EWMA 态、②`util` 面单源公式直用）。设计裁量：

- **weight 方向**：+Δ = 提带（`prio_effective` 高 = 优带）——忙段提带早完 = race-to-idle（早进深空闲，电池净赢）；低 util → -Δ 礼貌降带。**推断标注**：步长 16 = 包络 ±64 的 1/4（首刀保守档）、阈 896/128 = 满格 7/8 与 1/8（对称分数档）——策略数值面 = 不承诺面（ADR #68 ⑥ 口径）。
- **EPP 档**：thermal 四档词汇单源 berry-thermal（0 nominal/1 fair/2 serious/3 critical，其 lib.rs:34-35）；fair 起偏效率对齐 berry-thermal 频率帽阶梯（fair 档已限频 70%——热动作同档起效）；nominal → 无意见不覆盖——**二值词汇（Efficiency/Perf）无 balance 档**，内核默认 Efficiency（省电身份支柱，iprofile 注）已在位，武断 Perf 不如沉默（无意见 = balance 的诚实形）。**推断标注**。
- **单源纪律**：EPP 打包位/权重包络复用 berry-polface（`EPP_HAS`/`WEIGHT_MAX`）——kernel `sched/policy.rs` 同源消费，零第二实现（berry-power Cargo.toml 新 path 依赖 + Cargo.lock 两包）。
- host 3 测：`weight_delta_three_bands`（三带/阈端/包络自律）、`epp_flags_thermal_bands`（None/nominal/fair+/野值/位自律——内核 -EINVAL 拒面的产面免疫）、`action_hints_end_to_end`（四字段与分面重算逐位一致 + 忙轨高档正向 + EM 缺席 capacity=0 → ②freq 0 ①③④零污染）。

### 2.2 lib 组装面（`PowerCore::action_hints`）

[user/bins/berry-power/src/lib.rs](../../user/bins/berry-power/src/lib.rs):193——`hint::action_hints(&self.util, self.thermal_level, self.policy.fmax_khz, self.policy.capacity)`：thermal_level = sim 热帧遥测折叠（`apply_snapshot` 落位，None = 尚无热帧 → ④ 无意见）；纯读零副作用。模块注/re-export 同批（`ActionHints`/三函数/四常量）。

### 2.3 bin 壳四动作链（main 尾段，park 前）

[user/bins/berry-power/src/main.rs](../../user/bins/berry-power/src/main.rs):224-299：`core.action_hints()` → `self_proc_h()` 为 auth → STATUS（gen 对账 + key）→ COMMIT 单条目（`{key, placement, weight, epp_flags}` 三动作字段——**一条不是三条**：整表提交语义下逐字段分提只留末表，联合条目是唯一正确形）→ 再 STATUS 回读 gen echo（受理对账活证）→ 一行诊断：

```text
berry-power: policy4 place=2 freq=124khz(pm) wd=-16 epp=eff key=… st=0 bd=0 gn=1 auth=…
```

（字段值 = 授权在位时的预期形态——boot 实跑归 owner，§4 留窗②；`freq…(pm)` 标记 ② 动作 PM 域无 0x83 载体。）设计裁量：①dry-run 行逐字节不动（15 号纪律延续）；②单发非周期（park 10s 睡环不刷串口——周期面随窗 2/3 立法刀）；③门拒退化诚实（授权缺席形态 st=bd=-9、key/gn=0 哨兵透传——旧 svcmgr 像未重建的裸跑档即此形态）。

### 2.4 svcmgr 授权落位（一行 + 段注）

[user/bins/svcmgr/src/main.rs](../../user/bins/svcmgr/src/main.rs):1108-1151——berry-power 预拉 `stdin_feed: None → Some(&[])`（berry-thermal 维持 None：无策略注入面）；段注明示「不为喂数据、为触发 gift[2] 装配」。**平像尺寸实测**：elf2flat `--max 163840` 过墙（file=0x272f0 / mem=0x28000 与改前逐值相同——增量恰页内吸收）；**零余量警示**：mem 顶 = 0x28000 恰触 D3 帽/pager 钉位，svcmgr 后续任何增长须先升帽（kernel 面，justfile `--max 163840` + kernel D3 同值双改）。

## §3 验证（host 全实跑；QEMU 归 owner）

### 3.1 host 面（实跑）

- `CARGO_TARGET_DIR=<隔离> cargo test --manifest-path user/bins/berry-power/Cargo.toml --target aarch64-apple-darwin`：**36 passed / 0 failed**（33 旧 + 3 新——`hint` 面三测）。
- 产线形实编①：`cargo build --release --manifest-path bins/berry-power/Cargo.toml --features wired --target aarch64-unknown-linux-musl`（berry-ld 链）——零 error 零新 warning。
- 产线形实编②：`cargo build --release -p svcmgr`（aarch64-unknown-none）+ `tools/elf2flat.py --max 163840`——过墙（§2.4 尺寸账）；既有 3 warning 均 `unused_assignments` 前序遗留（DPGB p50 段，与本刀无关）。

### 3.2 QEMU 面（owner 待跑——本刀会话 kernel/qemu 零碰）

owner 下一轮 `just run-rootfs`（build-user 会重建 rootfs 内 berry-power 与 `kernel/src/userbins/svcmgr.bin`——后者当前是 12:15 旧像，**不重建则授权不生效**、诊断行退 §2.3 门拒形态）预期判读：

```text
eco: berry-power up
berry-power: skeleton dry-run …（既有行逐字节不动）
berry-power: policy4 place=… freq=…khz(pm) wd=… epp=… key=… st=0 bd=0 gn=… auth=<非0>
```

- `st=0 bd=0`：MANAGE 门过（授权链 §1.3 全通）；`gn == 提交 gen`（= STATUS 首读 active gen+1）：四动作条目真落表（gen echo 对账）；`key != 0`：自身 proc_tag 真值（0 键丢弃面未触发）。
- `wd`/`place` 判读示例：15 号同 boot 自证 util=127@k → max_est=127 → wd=-16（≤128 低带礼貌降带）、place=2（CPU0 忙避开）；`epp=eff`：sim 合成热帧 level_hint=1（fair）→ Efficiency。
- smoke 回归：berry-power 不入 markers.sh 任何数组（非标记诊断行——雷册 §A145 口径），32 标记零扰动预期；`docs_guard` 见 §3.3。

### 3.3 docs 面

`node .agents/tools/check-docs.mjs`（docs_guard 同源工具）——本刀文档批（本件 + notes 笔记 + README 索引三行）过检；实跑记录见 §5 引用清单尾。

## §4 留窗与后续刀

1. **ktest POLICY4 断言（owner 落地件——kernel 面零碰纪律）**：EASPROD（[kernel/src/ktest.rs](../../kernel/src/ktest.rs) `test_eas_prod`）姊妹扩展或新号 `test_policy_four`，建议断言集：①真 SVC STATUS（真 Process handle）gen 双半区对账；②COMMIT 单条目三字段（placement=2 / weight=9999 越包络 / epp_flags=`EPP_HAS`）受理 0 → `policy::placement_hint(k)`==2、`apply(task_k)` 后 `prio_effective == prio_base+64`（夹紧第三道）且 `task.epp == Efficiency`、`reject_stats().clamped ≥ 1`；③epp_flags=0 条目 → `task.epp` 不被覆盖（None 语义）；④清场 unbind + close（H8/EASPROD EP-E 同款）。BANDM4 侧无新断言面（band 位恒 `BAND_SEL_NONE`——v3 写路未开，ADR #21 带内纪律）。
2. **QEMU boot 证据行**（owner）：§3.2 判读面；`kernel/src/userbins/svcmgr.bin` 须随 build-user 重建（当前旧像 = 授权不生效的门拒退化形态——非缺陷，§2.3 诚实退化设计）。
3. **周期轮询面**（窗 2/3，15 号留窗③同款）：recv 常驻循环 + util 周期快照 + gen 重试协议（-EAGAIN 对账重发）+ hint 动态重提交（thermal 档翻新 → EPP 重提交）——随 PM 通道分端执法刀。
4. **跨进程 key 授权立法**（15 号留窗①后半）：svcmgr 持各子 Process handle 的代提交/Job 对象 SET_POLICY 细粒度权——本刀只落自建议最小面。
5. **svcmgr D3 帽零余量**：平像 mem 顶恰 0x28000——后续 svcmgr 增长先升帽（kernel/src D3 + justfile `--max` + pager 钉位同值三改，owner 面）。
6. **频率动作的内核写面**（窗 4）：`FreqVerdict.eff_khz` → 内核聚合点/DVFS 写请求 op——随内核聚合点 ABI 立法（CP-08/CP-10）；到位前 ② 动作的「激活」= 产面计算 + 诊断行自证（PM 域内部闭环）。

## §5 引用清单（路径:行号）

| 事实 | 锚 |
|---|---|
| 0x83 op 词汇 1..6 + p1-p4 语义 + MANAGE 门 | kernel/src/syscall.rs:2759-2804, 2816-2828 |
| PolicyEntryABI 16B 布局（placement@12/weight@8/epp_flags@13） | kernel/src/syscall.rs:2728-2736, 2959-2991 |
| weight 三点夹紧 ±WEIGHT_MAX / epp_flags 位检 -EINVAL | kernel/src/syscall.rs:2970-2987 |
| 动作空间②「频率走 PM 不在表里」 | sched/src/policy.rs:16-18 |
| Directive 三字段 + apply 消费（effective/epp 写点） | sched/src/policy.rs:65-74, 419-424 |
| weight_of / placement_hint 读出口 | sched/src/policy.rs:441-455, 471-487 |
| prio_effective 带序（pick 按 effective 出选） | sched/src/class.rs:43, 53 |
| task.epp 字段 + 默认 Efficiency | sched/src/task.rs:215, 403; iprofile/src/lib.rs:30-42 |
| EPP/WEIGHT 词汇单源（polface re-export） | sched/src/policy.rs:54-57; polface/src/lib.rs:74-83 |
| proc_tag = 子进程表指针（EL0 非零） | kernel/src/syscall.rs:1762; kernel/src/pi.rs:248-258 |
| 自描述对 gift[2] 条件装配（stdin 在位） | user/berry-exec/src/lib.rs:739-748, 764-790 |
| feeder 空喂 EOF + 关端 | user/berry-exec/src/lib.rs:823-835 |
| libc boot gift 解析 / self_proc_h | user/berry-libc/src/io.rs:90-118, 439-445 |
| svcmgr 预拉改 stdin（vblk/vnet 先例 :1014/:1077） | user/bins/svcmgr/src/main.rs:1108-1151（:1139） |
| thermal 四档词汇（0 nominal/1 fair/2 serious/3 critical） | user/bins/berry-thermal/src/lib.rs:34-35 |
| hint 纯面（三函数/四常量/3 测） | user/bins/berry-power/src/hint.rs:36-104, 106-198 |
| PowerCore::action_hints 组装 | user/bins/berry-power/src/lib.rs:184-199 |
| bin 壳四动作链 + policy4 诊断行 | user/bins/berry-power/src/main.rs:224-299 |
| host 36 测行注（33→36） | justfile:855-859 |
| 决策笔记（本刀） | .agents/notes/implemented/2026-11-16-v3四动作激活-berry-power完整策略服务.md |
