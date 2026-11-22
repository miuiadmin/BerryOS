# 机会一②双圈合一：pick_next_preempt 单圈门控出选与 has_eff_above_filtered 退役

Status: implemented

## Problem

[04 号调度热路径审计](../../../research/内核深度进化/04-调度热路径-Top5优化机会-v1.0.md) 机会一②：`try_preempt` 每次执行 = `has_eff_above_filtered` 只读探峰一圈 O(n) + 命中后 `pick_next_filtered` 出选第二圈 O(n)——同一环同窗两圈走查；命中形态扫描翻倍。三路勘察（wf_2c9da814-53f：design/counter/regression）定谳三前提修正：①机会一③计数移窗**已随 0ea3d11ad len 门刀连带落地**（fetch_add 现居 len<=1 门与 count==0 双门之后，空探零全局行触达——本刀勿重复记账，残余仅 API 参考 preempt_checkpoint 行陈账）；②双圈合一曾被 len 门刀 Alternatives 以「lazy_enforce/lazy_unthrottle 只在 pick 侧的副作用不对称」显式推迟——S5-c 刀（095d41d13）把解冻接进出选面后该算术改变；③`has_eff_above_filtered` 全仓唯一调用者=try_preempt、ktest 零直调——退役自由。

## Decision

- **Plan A 定案**（勘察设计路，PURE_EEVDF 陷阱绕开）：`class.rs` 抽取私有共享体 `pick_inner(gate: Option<i32>)`——头部 S5-c 解冻（照 pick_next_filtered 现形）→ `scan_filtered` → gate=Some 时**扫尾门控**行读胜者 pre-apply `prio_effective`（`<= th` 零副作用返 None：advance/apply/note_pick 全在门后不触）→ 过门 `advance_past` + `policy::apply`；**不含 note_pick**（两调用方各自补）。两口：`pick_next_filtered` = pick_inner(None)+note_pick（签名与行为逐位冻结——七生产位+ktest 三族零漂移）；新增 `pub Fair::pick_next_preempt` = 阈值自读 `(*my_cur).prio_effective` 一次 + pick_inner(Some)+note_pick（契约 my_cur 非空，try_preempt null 早退护栏保证）。
- **门控读点纪律（立法进 pick_inner doc）**：读点必须在 `advance_past` 与 `policy::apply` **之前**（pre-apply 值）——读 post-apply 值会在负权重策略形使门控与 try_preempt 守卫两道判定错位、破幻影防线。Plan B（ScanAcc 导出 best_eff）弃用：pure 下 best_eff 恒 i32::MIN 门恒死。
- **try_preempt 单圈化**（sched.rs）：删探峰+出选两段改单次 `RR.pick_next_preempt`；null cur 早退保留；**守卫 `next_ptr != cur && post-apply eff 严格更高` 逐字保留**（apply 降 eff 边界的真防线）。S5-c 接受的「探序 ≤1 片延迟」挂账随刀闭合——解冻进探峰面（pick_inner 头部先于扫描），刚到期 DL 任务对 try_preempt 立即可见。
- **has_eff_above_filtered 退役**（runqueue.rs 整删，PickFilter 退役改形先例）：dpgface 纯值域钉板（`has_eff_above` 纯函数）独立保留——存在律在门控形下由构造性成立（出选后不切走不可能发生）。
- **裁决口径（文档化接受，非无条件零语义变化）**：零语义变化主张只限 v1 默认形态（门闭=无带武装+dl_on_ring==0——等价证书：!PURE_EEVDF 下 chosen 必在最高带 ⟹ 门控行读 ⟺ 旧探峰存在性判定，同一快照）；armed/dl 形态两处方向性偏差为 S5-c lazy 面设计意图方向——① miss 路幂等跑 lazy 面（旧探峰纯读）；② 幻影窗构造性消灭（旧形探峰命中后扫描期 lazy_enforce 可降出选者资格 → 守卫拒切 → 游标白耗一格+note_pick 虚计；门控 pre-apply 读把这些 miss 全变零副作用）。
- **钉板三件**：ktest `test_pick_preempt`（PNPK ok n=3，末位新号）三态——(a) cur+同带他者 → None 且 head 零位移（等键探针读 head：同带同键下 filtered 出选者=head 侧首见律，`Runqueue::head` 是 pub(crate) 不可直读的替代法）；(b) 严格更高 eligible → ptr::eq 身份+head 推进；(c) ==threshold 恰界 → None（严格更高律）。ringface host `scan_tests` adversarial_matrix 补 chosen-in-max-band 等价律断言（!PURE_EEVDF 守卫，2000 例逐例执法——数学证书的 host 钉板）。dpgface fastpath doc 两句对齐指针改指门控形。
- **注释与文档随刀**：class.rs PURE_EEVDF 判例注（门控读胜者自身 eff；合并形消除 pure 形幻影 head 消耗——行为差异仅实验轨）；sched.rs try_preempt 头注幻影防线段改单圈门控形+W3-Q59 注+null 护栏注+偷取远端窗注三处清创；ktest BS-3 注「唯一路径」改两面；API 参考 v1.12→v1.13 升版随迁（git mv 保史+新行+计数 181 不变〔class 16→17/runqueue 7→6——影抄本就缺 has_eff_above_filtered 行，退役后行集对齐〕+preempt_checkpoint 行补 0ea3d11ad 双门陈账=机会一③文档收口）；雷册 §A43 与规则/04 §A43 机制表述两处滚新（瘦行+全文齐动）；六处入链 v1.12→v1.13；索引 API 参考行滚新。

## Alternatives considered

- **Plan B（ScanAcc 导出 best_eff 布尔）**：pure 形门恒死（best_eff 恒 i32::MIN）须 !PURE_EEVDF 门或退化成读胜者 eff=Plan A——弃。
- **纯扫描合并（探峰侧零副作用、命中路跳过 lazy 面）**：命中路径丢 S5-c 解冻面=行为回退——弃。
- **继续推迟②（len 门刀原裁决）只做③收尾**：S5-c 已改变副作用不对称的前提（解冻本就应在一切出选面发生），且探序 ≤1 片延迟挂账待闭——推进。
- **复合返回 Option<(bool, &mut Task)>**：泄漏探峰布尔给调用方重复判定、BelowThreshold 三态无消费者——弃。

## Consequences

- 热路径收益（结构论证）：try_preempt 命中路省整段探峰圈（k 步廉价节点访问）；miss 路两形同为一圈；DL_ON_RING>0 形态旧形可达三圈（probe+unthrottle+scan）收敛为两圈（unthrottle+scan）。v1 全员 fair 常态：len 门（0ea3d11ad）先行，单圈形不额外触发。
- note_pick 记账面变化：旧形 probe-true+守卫拒切的幻影计消失（门控 miss 零计）；gate 过+守卫拒切（负权重策略残余形态）仍计——与旧形同，守卫兜底不变。
- PURE_EEVDF 实验轨差异：幻影 head 消耗被合并形消除（v1 恒 false，判例已注 class.rs）。
- 验证实跑（隔离 CARGO_TARGET_DIR=$PWD/target-s5c）：cargo build 绿；berry-ringface host 全绿（28+8+15 含新等价律）；berry-dpgface 全绿（23+11）；boot 相 smoke 32/32 标记齐（FAST 直切硬网=ping-pong 直切断言、PNPK 三态、全套 ktest 过）。test-host 链上 berry-appshell 两败定谳为并行 lane 已提交态测试竞速（共享 diag 静态+毒锁连坐，left/right 为两条不同测试的 diag 行——构造性竞速证据；berry-appshell 工作树净、与本刀零交集）。
- `just build` 的 docs-guard 在工作树被并行 lane 对其自己已提交内容的陈旧缓冲覆写挡红（索引 :33/:134+沿革 547 节自抹+沉浸头块回退）——主线改 cargo 直构+smoke.sh 直跑完成验证；该红非本刀债（AGENTS 规则 8）。
- 对抗审查（wf_5d4ccbe5-ad2，25 agent 四维 finder+逐 finding 对抗核实）：21 发现（19 CONFIRMED/1 PLAUSIBLE/1 REJECTED）全数处置——①**v1.13 计数勘正 181→182**（最硬一条，四维度重复命中：退役项 `has_eff_above_filtered` 系 `pub(crate)` 从不在机提口径，runqueue 仍 7、class 16→17、合计 182——版本头/分文件表/合计行/索引行四处同步+「事实源对拍时点」行补 v1.13 注）；②规则 README 入链漏一（反引号路径提及，check-docs 不报警的盲区）；③API 参考 pick_next_filtered 行「生产出选唯一路径」陈账；④雷册/规则 §A43 两处符号名漏 `_filtered` 勘正（dpgface 纯函数 `has_eff_above` 独立存续，勿误伤）；⑤class.rs trait 面头注「八个出选点」清创（七 filtered+一 preempt）+性能主张「miss 路同为一圈」补 DL_ON_RING>0 反例限定（该形态 miss=解冻+扫描两圈，但命中路旧形三圈收敛两圈——净口径按形态记）；⑥dpgface 两处孪生注+dpg.rs W3-Q59 消费锚注滚新；⑦ktest (c) 注勘正（>= 误用形返回 cur 自身——等键 head 侧首见）+miss 路 note_pick 零计补钉（intent_picks 快照对拍）+(b) 等键前提自钉（键漂移两方向显形——PLAUSIBLE 补强）；⑧MAJOR「提交闸风险」（工作树兄弟陈旧覆写与本刀必交文件相交）=外科提交处置；⑨笔记/沿革缺口=本笔收口。修后复跑：cargo build 绿+boot smoke 32/32+berry-dpgface/berry-ringface host 全绿。
- 沿革五百四十九；API 参考 v1.13（**182** pub fn——审查勘正后口径）。

## 形替换附记（2026-11-18 合并 1e7872196）

本刀落地面在合并 1e7872196（2026-11-18 02:29）中被 origin 侧同题刀形胜替换——终形 = `Fair::pick_next_if_above`（W3-Q93，见[探峰出选双圈合一笔记](2026-11-17-探峰出选双圈合一.md)）：class.rs 解到 origin 侧 `scan_with_eff` 单圈累加器带门形，本刀 `pick_next_preempt`/`pick_inner` 载体与 ktest `test_pick_preempt`（PNPK）已随合并裁退（全树零定义）。语义等价（解冻→扫描→带门→出选，miss 零副作用）经 DL-B4 四态 + ringface host 3000 例对拍钉板。按规则 9 读本笔记：Problem/Decision/Consequences 各段的载体与机制名均系本刀落笔时形，现行代码以 W3-Q93 形为准（API 参考 v1.18 条目已改登终形）。
