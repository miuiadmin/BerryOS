# 机会五③密度账本化：Runqueue 双计数器与 settle 锁包重登

Status: implemented

## Problem

04 号审计机会五③收官件：`try_steal_one` 密度快照段（机会五①空判门+门开时本核 RQ 锁+整圈走查）——门开态每次偷取探测白付锁对+O(n) 走查；带感知偷取偏好序（STEAL_RANK 三档）消费的密度真值可账本化。两路勘察（wf_4808cb98-f26）：前勘察账本路线在现树全部复成立且**更顺**——settle→write_back 是生产唯一在环 intent_effective 写点（cur-in-ring），其调用窗（recv slowpath 全屏蔽+PreemptSpan）天然满足 RQ 锁契约=零额外前置；ktest 现存零直改回填点（resync 义务纯前瞻）；跨核原子性照 len 纪律（单写者 load+store 免 RMW——一切写点在本队列 RQ 锁窗内，原子性只为跨核读者防撕裂）。

## Decision

- **A 账本**（runqueue.rs）：Runqueue len 后加 `int_count`/`util_count: AtomicUsize`（纪律同 len；账点挂 link_back/unlink 结构原语层——出环三路含 block 直调/steal 摘对核环，挂类层必漏账）；link 尾按 intent_effective 当下值入账/unlink 尾镜像出账；`density_reclass(from,to)`（settle 带变重登，调用方持本队列锁）；`dens_on_ring_walk`（§A136 手写形+帽，integrity 真值侧）；`resync_density`（测试洗账面照 resync_vruntime_sum 纪律）。单源等价律钉死：band_of_task(t)==INTENT_x ⟺ intent_effective==INTENT_x（FairBand::new 使 band_id==intent 恒成立）。
- **B settle 锁包**（sched.rs intent_settle_current_ext）：before→rq_lock→settle→after→带变则 density_reclass→rq_unlock；**write_back 与计数修正同窗**（窗外=走查见新带账旧值假火）；契约扩张 doc（新增 RQ 锁触达，调用方 IRQ 屏蔽——现役唯一调用者已满足）。
- **C 快照换载**：try_steal_one 密度段整体换两次 Relaxed load——①空判门被吞没（超集定谳：读旧零=(0,0) 定义值/读旧非零≡门不开形已消/漏登态空环读陈非零=已裁买办——诚实注记）；偷取三触发点各从「len 读+门开锁+O(n)」降两 load。
- **D integrity 双边等式**：双 RQ 循环窗内双读（walk 真值 vs 两计数）放锁后断言**等式**（per-RQ 环私有无 ktest 本地环污染——DL 单边理由不适用；在途偷取两环两账同缺席免疫；两向响亮=漏登漂移即捕获，含下溢回绕形）。
- **E ktest DENS 族**（末位新号）：D-1 混合计账/D-2 四向转移/D-3 resync 修复面（直改→漂移→归零病理演示）/D-4 settle 正形+走查==账本+手数钉死。既有 ktest 零改。
- **F 不动面**：DL_ON_RING 及单边断言、nearest_dl_throttle_wake 醒源走查、place()/pull_due、steal 回插链（自然经 link_back 入账）。
- 顺手清偿：探针刀漏账两行（writable_globals NAME_HOOK/SPIN_BARK STALE——合并复活清单版本所致，89 entries 复绿）。

## Alternatives considered

- **扩 ringface::ilist 泛形**：双谓词计数改泛形扰动大收益低——手写 §A136 环（仓内两例先例）。
- **integrity 并单趟三计数**：省一趟 O(n) 但 dl_on_ring_walk 签名零动爆炸半径最小——不并。
- **原子 RMW 计数**：单写者纪律下不需要（len 先例）。

## Consequences

- 偷取探测密度成本：门开态「锁对+O(n) 走查」→两 load（严格减）；link/unlink 热路径 +2 比较分支（非 int/util 零访存）——switch_pair 基线预期漂移（绿基 prove 留档）。
- 漏登防线：integrity 双边等式每 100 切换响亮（未来新在环 intent 写点不经 reclass 即捕获）；task.rs 字段注释挂指针防线归修复回写相。
- 验证：绿基独立验证法（origin VMO 断言在途红仍在——hermes lane 债）：绿基（376399d0e+刀，唯一冲突块取节流刀并集形）build 绿+boot smoke **32/32（DENS 族随套过）**+prove 五跑 verdict=exact——**switch_pair 306→310（+4=link/unlink 计账分支真成本）+send_recv +1（settle 锁包）+pick_scan 131 逐位稳（pick 路径零改预测兑现）**（确定性通道第五批单刀量化，回执 icount-prove-20261119-180940）；正式基线重烘待 origin 修红。
