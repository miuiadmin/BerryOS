# S5-c deadline 解冻生产接线（04 号审计附记一）——双写修复与 DL-B3 钉板与 s5cprobe 端到端探针

Status: implemented

## Problem

[04 号调度热路径审计](../../../research/内核深度进化/04-调度热路径-Top5优化机会-v1.0.md) 附记一（优先级高于全部性能项）：S5-c 逐任务 deadline 预算的唯一解冻点 `lazy_unthrottle_deadlines`（sched/src/class.rs）只被 trait `Fair::pick_next` 调用，而生产全部八个出选点（yield_now_impl / switch_away_forever_with / block_current / try_preempt / idle_loop / on_irq_tick / on_sgi_resched / park_recheck）全走 `Fair::pick_next_filtered` 且不含该调用——deadline 任务（生产入口 `sys_sched_setattr` policy=3 spawn 期可达）预算燃尽被 `Fair::tick` 置 HELD_THROTTLE 后在生产永久不可出选。开工前四路勘察 workflow（fix-surface / pinboard / evidence / regression，wf_620a40bb-7b7）对抗核验全部 CONFIRMED：band 解限流（deadline 豁免）、freeze-thaw（单 bit6）、结构锁位（bit1-5）、setattr（只存不配）四条潜在替代解冻路全被结构性排除；且现行生产载荷零 deadline 任务（user/ 与 kernel/bins/ 无 SA_FLAG_POLICY 消费者）、ktest 在 sched::init 前跑无真调度循环——修复对 smoke 不可观测，ktest 是唯一活执法面。

## Decision

- **修法=双写**（方案 B）：trait 面 `Fair::pick_next` 调用保留（ktest 直调面，DL-B2 钉板原样绿），`pick_next_filtered` 头部、`scan_filtered` 之前新增 `unsafe { lazy_unthrottle_deadlines(rq) };`——两面结构不交、无双重调用路径；`dl_on_ring()==0` 早退门在函数自身头部，v1 常态全员 fair 零扫描。SAFETY 判据：八生产调用点全在本核 RQ 锁+IRQ 屏蔽窗内，与紧随的 `scan_filtered` 同窗同节点集，访问形态严格弱于既有扫描。
- **ktest DL-B3**（test_deadline 内、DL-K marker 前）：独立环 rq3+tb3（B1 制造法、enqueue 正路入环使 DL_ON_RING 记账）——负控：远截止（now+1e8 cycles，「远截止不触面」范式）限流者 filtered 直调断言 None（修前生产缺陷形态钉死）；正钉：vdeadline=1 过期后 filtered 调用四断言（ptr::eq 身份 / THROTTLE 清 / slice_left==100 预算重授 / vdeadline 重臂=1+period——第四断言为审查轮补，dl_refill 回写接线此前零自动化覆盖）。
- **段尾出账**（审查轮补，wf_12815dd0-cfd CONFIRMED 处置）：本族 DL 任务（td1/td2/tb/tb3）段尾全部 `rr.dequeue` 摘环配对 DL_ON_RING 出账（td1/td2 在 D3 前已降 fair，先复带 PRIO_DEADLINE 再摘使 unlink 按带位出账）——否则默认 boot（ktest 后继续生产调度）账本恒 ≥4、解冻快门常开、每次生产出选白付一圈 O(n) 走查，「零成本」主张被同一刀证伪。RING2 族 dequeue 收口同款惯用法。
- **s5cprobe 端到端探针**（kernel/src/main.rs，pilat 同款机械形态）：fw_cfg 键 `opt/berry/s5cprobe` 在位才武装（早读置 S5C_ARMED + sched::init 后 spawn 门控块 + 自证行），缺席 = run/smoke 零扰动。task_s5c_dl（受害者）：宽限睡后 klock_enter 全谱窗内经 `for_each_runnable` 自我补齐预算三件（vdeadline/duration_hint_ns/preempt_threshold+slice_left，口径照 `apply_sched_attr_to_task` 生产写面）→ 自旋测量环（计数+CNTVCT 比隙，隙>period/2 判确认 stall——prio 1000 之下长隙唯一来源=预算限流；stall 后续跑即复活）→ ≥2 次确认 stall → `S5C-DLP ok stalls=N iters=M`。task_s5c_fair（fair 陪跑+看门狗）：dl 冻死（附记一缺陷形态，dl 自己无法自报）时代报 `S5C-DLP FAIL dl-frozen`；帽到链未证打 `S5C-DLP FAIL cap`。判读法注两条：TCG 宿主停顿 ≥3×period 可假报 dl-frozen（guest 钟=墙钟判例，复跑分辨）；勿与 pilat/LOADGEN 同 boot 共开（顶带垄断污染彼探针）。
- **探针首版判例（本刀最贵一课）**：首版以 fair prio 8 spawn、宽限后在途改档 deadline——`DL_ON_RING` 账本在 link_back 时刻按 `is_deadline` 计数，在途改档不入环则账本恒 0，`dl_on_ring()==0` 早退门常闭，解冻永不发生：修复树上跑出 `S5C-DLP FAIL dl-frozen iters=71088`（探针精确复现了附记一缺陷形态，反向自证探针判读力）。修正=spawn 即 `PRIO_DEADLINE` 入环（`spawn_with` 的 prio 落双字段在 enqueue 前，账本 +1）；宽限窗内预算三字段缺省故不受执法（threshold=0 不限流、duration_hint_ns=0 不触解冻面）。生产无此形态——唯一 deadline 入口在 spawn 期 enqueue 前落字段；「在途改档不重入环」是账本不变量，非生产缺陷。
- **注释与登记随刀翻新**：class.rs 四处+ sched.rs 一处（「三调用点」陈账改八点枚举）路径归属注翻新防下次静态审计误读；顺手纠两处 HELD_FROZEN→HELD_THROTTLE 陈账错名（tick 执法注与 lazy_unthrottle 头文档）；lazy_unthrottle 头文档 CBS 口径限定（审查轮：「每期至多 budget CPU 时长」只在解冻/首片成立——八个生产授片点派发尾 `slice_left = grant_cycles()` 整片覆盖，每期实跑≈一授片）；ktest B1 两处同族位名陈账一并清；`docs/调度器API参考-v1.12.md` pick_next_filtered 行滚新；`tools/fwcfg-keys.toml` 补登 s5cprobe（27 键对拍绿）。

## Alternatives considered

- **搬而非双写**（trait 面 :251 删调用、仅 filtered 保留）：生产等价，但 DL-B2（trait 面钉板）须同批改写、trait 面成死码——双写零 ktest 改写量、两面各自被钉板执法，取双写。
- **静态论证收口（不出 boot 探针）**：ktest 在 sched::init 前跑、无真调度循环，ktest 直调只证函数体；现行生产零 deadline 任务，端到端复活链（真 spawn→真 tick 限流→真出选解冻→复活）无探针则无证据——pilat fw_cfg 门控判例在席，出探针。探针首版即抓出账本不变量问题，证明这个决定正确。
- **nearest_deadline 醒源扩展随刀**（停泊核残留窗：throttled DL 任务停在深 park 副核环上，`nearest_deadline` 不含逐任务 vdeadline，复活迟至该核任意醒）：周期 tick 再臂下迟至一片、有界非饿死（THRYLD T3 给 band 限流解了同题，S5-c 逐任务位可同法扩）——今日根本无解冻故非恶化，不扩本刀审查面，挂账（见 Consequences）。

## Consequences

- 生产复活链实证：探针两跑 `S5C-DLP ok stalls=2 iters=1228314`（首修树）/ `ok stalls=2 iters=647634`（终树）——每次确认 stall 的检测都发生在复活后（冻死任务读不了钟），stalls=2 即两轮「燃尽→限流→周期到→filtered 面解冻→复活」端到端实证。
- **预算/授片交互挂账**（审查 NOTE 定谳，非本刀引入）：`dl_refill` 置 slice_left=budget 后，八个生产授片点派发尾 `slice_left = grant_cycles()` 整片覆盖——生产每期实跑 ≈ 一授片（10Hz 下 100ms）而非 budget，B3(iii) 钉的预算语义仅 ktest 直调面可观察（探针 S5C_BUDGET_MS 注同款事实已在案）。真预算帽待「grant 面按 class 分派」预算刀（`is_deadline(next)` 授 preempt_threshold，经 class_of_task 分派面勿散写）。
- **停泊核醒源残留窗挂账**：见 Alternatives 第三条——后续刀可照 THRYLD T3 手法把逐任务 vdeadline 并入 nearest_deadline。
- try_preempt 探序（has_eff_above_filtered 先于 filtered 调用）：刚到期解冻任务对探峰不可见 ≤1 片——接受有界延迟，避免 SVC 尾热路径重加扫描（违 0ea3d11ad len 门快赢）。
- 默认 boot ktest 后 DL_ON_RING 回零：解冻快门恢复常闭、v1 零成本主张在默认 boot 形态成立（审查 MINOR 处置后）。
- 验证实跑（全部隔离 CARGO_TARGET_DIR=$PWD/target-s5c）：`just build` 全链绿 ×3（docs-guard/abi-check/build-user/mkapp 随链）；`bash tools/fwcfg-check.sh` 27 键两侧一致；`SMOKE_PHASES=boot just smoke` boot 相 32/32 标记 ×3（impl 后/探针修正后/终树——ktest done 在列 = DL 族含 B3 与段尾出账全套绿）；探针如上两跑 ok。
- 对抗审查（wf_12815dd0-cfd，14 agent：四维 finder × 逐 finding 对抗核实）：10 条发现全数处置——9 CONFIRMED（ktest 账本泄漏 MINOR/B3 缺 vdeadline 断言 MINOR/CBS 口径 MINOR 修在批；B1 位名 NOTE/API 参考 NOTE 修在批；索引陈旧覆写 MAJOR 见下；TCG 假报与共开两 NOTE 修在批判读法；泄漏零破坏 NOTE 随账本修复消解）+ 1 PLAUSIBLE（TCG 假报面，判读法钉注）。
- **并行 lane 交互两笔**：①`docs/索引.md` 桌面沉浸式规范 G10 漏登——我方曾按最小闭窗补登（三区），审查轮发现兄弟 lane W2-Q100 已在 HEAD 正规代偿（f15e69609，五区更完整行 + 头块先例形修面），我方三编辑全数撤回、本刀提交零索引面；审查同时抓出工作树有兄弟 lane 陈旧缓冲对其自己提交的覆写（索引 :33 回退 + :134 删行 + 沿革五百四十七节自抹）——原样保留不碰，其 lane 自会随下次 docs_guard 察觉。②ktest.rs 兄弟 lane 有在途改动，我方三 hunk 全在 test_deadline 域内无混装；提交走显式路径。
- 沿革五百四十八；fwcfg 键表 27 键。
