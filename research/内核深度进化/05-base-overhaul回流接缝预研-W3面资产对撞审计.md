# 内核深度进化 05 号：base-overhaul 回流接缝预研——W3 face crate 群资产对撞审计

Type: research
Status: done
Updated: 2026-11-19

一句话说明：owner 的 `wip/base-overhaul` 线（F1-1/F1-2/F1-3 已落）回流 base（wip/sched-1.1）时，W3 lane 三周积累的 face crate 群资产将面对一次结构性对撞——本件只读审计两线分叉面，逐 face crate 定级「正和／原样／消亡／重放」四态，给出回流时的 W3 侧重放与对拍义务清单；结论：**对撞重心在三处**（vmar.rs −1114 区间树重构消亡 Q110 游标、channel.rs F1-3 重构剥除 recv_n 批量收消费点、syscall.rs 217 行与 base 线后续件的组合冲突），其余 face crate 全部正和或原样保留。

## 审计基线（2026-11-19 19:0x 直核）

- base 线 = `origin/wip/sched-1.1@e006c830b`；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`（2026-11-18 20:01 后未再前进）。
- 两线 merge-base = `fceb03ff3`（F0-4）——**overhaul 未吸收 base 线近三周的任何 W3 件**（recv_n 批量收、Q99 边缓存、Q100 类型无关折叠、Q101 诊断账面门控、Q104 SYSCALLS 表、Q105 icount、Q106 vDSO、Q108 exit_latch、Q109 PageLedger、Q110 L3 游标等均在 merge-base 之后落 base）。
- overhaul 对 kernel/sched 的净改动：+2734/−1287（aspace.rs +934 新 AddressSpace 对象、vmar.rs −1114 降级为句柄投影、channel.rs +626 ChannelShared 重构、handle.rs 218、syscall.rs 217、ktest.rs +794、process.rs 79、sched 侧仅 +7——`sq_is_held` 锁序判别新原语）。

## face crate 消费对撞四态表（base→overhaul 引用实测）

| face crate | base 引用 | overhaul 引用 | 态 | 说明 |
|---|---|---|---|---|
| berry-vmoledger | 9 | 11 | **正和** | overhaul 多消费 2 处（F1-2 map_refs 记账面 release_map/untrack 接线）——Q109 打包槽（8B 单 u64）与 F1-2 组合计数正交，回流零改 |
| berry-hdlface | 6 | 6 | 原样 | fork 时已带（Q8 位域+Q25 lifecycle），ObjRef 64 位组合原子态改的是 kernel 侧宿主结构，face 判定律单源不动 |
| berry-polface／piface／dpgface／safeck | 14/5/8/6 | 14/5/8/6 | 原样 | 同上——判定律面与 overhaul 重构面无交集 |
| berry-wqmatch | 2（pi.rs） | 2 | 原样 | PI 面两线同源 |
| berry-vslotface | 17 | 7 | **部分消亡** | droot 面（Q60）overhaul 继续消费（DEMAND_ROOT_SLOTS 收口）；mapgate（Q56）与槽位三查（Q17）随 vmar.rs 区间树化整面失去宿主——overhaul 自建 `berry-vmarface` 区间树 crate（VmarTree/destroy_vmar/RemovedSpan）接管判定 |
| berry-chanface | 13 | 4 | **消费收窄** | F1-3 保留 push/pop/frozen/容量不变量 re-export（Q49/Q95 判定律存活）；**recv_n 族（批量收）在 overhaul 零命中**——fork 早于落地，非否定裁定 |
| berry-vmarface（overhaul 新建） | 0 | 2 | 新增 | W3 无对应资产；回流时为 base 引入新 crate，与 vslotface 判定面有语义重叠（区间合法性 vs 槽位三查）需归并裁定 |

## 三处对撞重心展开

### ① vmar.rs −1114：Q110 游标结构性消亡（接受消亡）

overhaul 的 `AddressSpace::map/clear_leaves/protect` 走区间树 span 批量形（`l3_slot_create/l3_slot_peek` 单点查 + `RemovedSpan<VmoRegionMeta>` 批量清叶 + `release_removed_span` 统一释放），Q110 的 `l3_span_walk/l3_span_peek` 段游标在区间树形态下无宿主——**判「接受消亡」**：Q110 的动机（每页一走查的冗余）被区间树批量形更彻底地解决。Q110 随刀的 ktest E2 跨段界用例与 ledger 单遍合一不变量应在回流后于新形态下重钉（对拍义务，非重放）。

### ② channel.rs F1-3：recv_n 批量收消费点剥除（重放件）

F1-3 的 `ChannelShared`（双端 Arc 共享、`Message::Drop` 在途句柄析构、`SIGNAL_PEER_CLOSED`）与 recv_n 批量收（base 线 syscall.rs 14 处消费、chanface `recv_n_*` 判定律族）正交且互补——fork 时序导致缺席，非形态否定。回流时 W3 义务 = **在 ChannelShared 形态上重放 recv_n**：chanface 侧 `recv_n_k_legal/recv_n_slot_legal/recv_n_wake_needed` 判定律零改动可直接复用，kernel 侧消费点需从旧 `ChannelEnd::pair` 双端形改接 `Arc<ChannelShared>` 单体形。

### ③ syscall.rs 217 行：组合冲突面最大（合并时逐段对拍）

overhaul 保留 fork 前的 W3 件（Q22 safeck、Q91 rights_of 探针——overhaul 树 :2623-2655/:3368-3376 注释自署可证），但 fork 后 base 线的 syscall.rs 新增面（recv_n 十四处、Q99 边缓存、Q100 wait_get 五链折叠、Q101 诊断门控、Q104 SYSCALLS 表固化）overhaul 零吸收；且 F1-3 给 syscall 退出链带 `<1ms` PEER_CLOSED 同步关闭窗（:2052）。回流合并时 syscall.rs 将是冲突文件数之首——**逐段对拍义务**：W3 后落件按 Q 号注释锚点逐段重放，overhaul 侧 F1-3 语义（PEER_CLOSED/safeck）按段保留。

## 回流时 W3 义务清单（单源）

1. 正和/原样件（vmoledger/hdlface/polface/piface/dpgface/safeck/wqmatch）：零动作，随并流自动保留。
2. vslotface：droot 面保留；mapgate/Q17 三查面随 vmarface 区间树归并裁定（预期退役，判定律语义由 vmarface 承接）。
3. chanface recv_n 族：判定律复用 + kernel 消费点改接 ChannelShared 形（重放件）。
4. Q110 游标：接受消亡 + E2 用例/ledger 不变量在新形态重钉（对拍件）。
5. syscall.rs：逐段 Q 号锚点对拍重放（组合件）。
6. 台账/白名单/wg 面：berry-vmarface 新 crate 入册（members/白名单/justfile/unsafe 台账），按 #171 判例族全量对账。

## 与其他文档的关系

- 机会 3 承接裁定（本 lane 对 overhaul 线的前一份研究）：[01-机会3立项前置研究-进程退出回收双线对撞审计.md](01-机会3立项前置研究-进程退出回收双线对撞审计.md)。
- 线体证据：`git log origin/wip/base-overhaul`（fceb03ff3→2a9abb05d→9f113a65d→492d5e00c）；引用计数 `git grep -c berry_<face>` 两线实测（本文件表格单源）。
- 沿革：`docs/阶段沿革.md` 五百九十五节（随本批）。
