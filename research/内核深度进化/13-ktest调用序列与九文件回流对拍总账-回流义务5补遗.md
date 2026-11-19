# 内核深度进化 13 号：ktest.rs 回流调用序列与 F0/F1 自检面施工图——全仓冲突文件 10 文件对拍总账（回流义务 5 补遗）

Type: research
Status: done
Updated: 2026-11-19

一句话说明：08 号件把 syscall.rs 落为六段冲突施工图，但 merge-tree 全仓冲突面共 10 文件——本件以最新 tip（base = `origin/wip/sched-1.1@4f76d8414`、overhaul = `492d5e00c`、merge-base `fceb03ff3`）实跑补齐另九文件的对拍定谳：**ktest.rs 的唯一冲突段是 run() 尾部调用序列（并集解，但 base 线 F0-3/F0-4 自检面已随 vmo.rs 单边取材回退丢失——回流时须以 overhaul 侧为准复活并按 08 号段 5/6 施工图对齐）**；vmar.rs 10 段全取 overhaul（vmarface 区间树形）；channel.rs 5 段 = 07/06 号件已裁定的归并/重放面实弹化；aspace.rs 2 段取 overhaul（try_alloc_asid 活锁防御形）；dpg.rs 2 段取 overhaul（demand_aspace 改接 + IrqSaveGuard——base 侧 klock_enter 手动配对是旧形）；user.rs/justfile/Cargo.lock/台账四文件零手工（vmarface/asidface 入册随刀 + 台账工具重产即解）。

## 审计基线（2026-11-19 23:4x 直核）

- base 线 = `origin/wip/sched-1.1@4f76d8414`（Q119 修复重推后 tip）；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`（未前进）；merge-base = `fceb03ff3`（F0-4 笔，含 `test_f0_3`/`test_f0_4` 定义与调用）。
- merge-tree 实跑：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul`（合并树 `9c0f779dbf`，exit=1）——全仓冲突 10 文件、共 38 段：Cargo.lock 2 / unsafe台账 8 / justfile 1 / aspace.rs 2 / channel.rs 5 / dpg.rs 2 / ktest.rs 1 / syscall.rs 6 / user.rs 1 / vmar.rs 10。
- syscall.rs 六段与 08 号施工图逐段复核一致（:58/:698/:1205/:1220/:3654/:3983 段位零漂——base 前进未触该六窗），施工图不重复，本件只补另九文件。

## ktest.rs：run() 调用序列并集解＋F0/F1 自检面复活义务（本件主发现）

唯一冲突段（合并树 :177-194）= `run()` 尾部 `ktest done` 前的调用序列：base 侧新增 `test_density_ledger();`（机会五③）；overhaul 侧新增 `test_f0_3/f0_4/f1_1/f1_2/f1_3` 五调用。**解 = 并集**（两侧调用互不排斥，密度账本与 F0/F1 族按「末位新号纪律」排 ktest done 前）。但并集有隐藏前提，是本件主发现：

- **base 线已丢 F0-3/F0-4 自检面的全部代码**：`fceb03ff3` 里 `test_f0_3`/`test_f0_4` 定义与调用俱在（:12198/:12262 定义、:178/:180 调用）；base tip（4f76d8414）里定义与调用**双零命中**（`git log --full-history -S` 在 fceb..tip 区间零提交命中——非显式删除，是活线合并对单文件取旧 blob 的静默回退，`3e323b8c9` 提交注记「D2 并行发现 vmo.rs 丢 F0-3/F0-4 contig 面」已预警同病，ktest.rs 面是同一事故的未被点名部分）。
- 丢失面清单（base tip vs fceb 实测）：`kernel/src/vmo.rs` contig/global 面（`VMO_OPT_CONTIGUOUS` 4 命中→0）、`kernel/src/syscall.rs` `NR_VMO_CACHE_OP`（0x1C 号位）4 命中→0、ktest 定义+调用→0；F0-3 的 `IrqSaveGuard` 在 trap.rs/main.rs 亦零命中（tlbshoot 对飞面 3 命中存活——F0-3 只丢了一部分）。
- **回流解**：ktest.rs 冲突段并集时 `test_f0_3()`/`test_f0_4()` 的**定义体取 overhaul 侧**（overhaul ktest :12210/:12274 定义俱在且吸收了 fceb 后的自身修正），base 侧无竞争版本（已丢）；`test_density_ledger` 定义已在 base 侧自动并区保留（合并树实测 1 命中）。F1-1/2/3 三定义 overhaul 侧全带（:12454/:12600/:12840）。五调用 + density 一调用按注释时序排入 `ktest done` 前。
- **回归义务**：`3e323b8c9` 注记的「五处 committed 水位写点须同批接」（commit_global 逐页 +1 / decommit_global −1 / contig 建账臂 +npages / Drop contig −npages / MMIO 零动作）随 vmo.rs 取 overhaul 侧自动带回，ktest 因果差分扩 contig 相位（12 号件增量钉板同批窗）。

## vmar.rs：10 段全取 overhaul（07 号裁定实施面）

10 段全落在 Q110 游标面与 v1 Vmar 结构面：use 位域（`zero_frame, Vec` → `Arc`）、IDX_MASK/TABLE_BIT/ADDR_MASK/AP_MASK 常量区（换 per-AddressSpace PT 锁守卫区）、Drop 臂（base 逐 region untrack → overhaul `destroy_vmar` 单源）、map/unmap/protect 主体、`demand_root` → `demand_aspace` 改接、walk_leaf 常量消费。**定谳：整文件取 overhaul**——07 号件已裁定 vslotface mapgate/Q17 三查面退役、判定律由 vmarface 区间树承接（200k 样本恒等对拍），本文件是其实施面；base 侧 Q110 游标（394580987）接受消亡（05 号义务 4 原判）。唯一留意点：overhaul `walk_leaf` 保留 base 同源版本（合并树 :877+ 自动并区），PAGE_VALID 局部常量形零变化。

## channel.rs：5 段 = 06/07 号施工图实弹（recv_n-on-ChannelShared + 环形队列并存裁定）

- 段 1（:15）use 并集：base `ObjRef/Rights/Box` + overhaul `Arc/Vec/UnsafeCell` 并存（F1-3 ChannelShared 用）。
- 段 2（:26）chanface re-export 并集：base 12 导出（含 Q95 ring 族 `ring_next_head/ring_slot_of/ring_tail_of`）+ overhaul `wait_signal_peer_state`；**裁定项**：Q95 定容环形 `Box<[Message; CHAN_DEPTH]>` 与 F1-3 `ChannelInner.queues: [Vec; 2]` 是两套队列存储——**取 overhaul Vec 形则 Q95 环形化接受消亡**（chanface ring 模块判定律保留、kernel 存储臂回退），或按 06 号接缝图在 ChannelShared 内重放环形存储（成本高、收益待 profile）——**列 owner 裁定项，与 RIGHT_EXECUTE 权利域（08 号段 4）同批**。
- 段 3（:95）结构定义二选一：`ChannelEnd { peer, queue, head, qlen, ... }`（base）vs `Message::Drop` + `ChannelShared`（overhaul）——取 overhaul（F1-3 骨架）。
- 段 4（:249）`try_unpaired` vs `try_pair`——取 overhaul `try_pair`（Arc 共享体一次建对；base 回滚语义由 open_ends 计数承接）。
- 段 5（:486）base 侧 `pair/peer/queue_full/has_message/qlen/push` 方法组 vs overhaul 方法组——取 overhaul；base Q98 尾槽直拷按 08 号段 5 施工图在 push_to_peer 窗内组合重放（06 号接缝⑤）。

## aspace.rs / dpg.rs / user.rs：三文件全取 overhaul

- aspace.rs 2 段：`alloc_asid` 双形（base 锁外 roll_quiesce_arm 单形 vs overhaul `try_alloc_asid` Option 形 + pinned 避让）——**取 overhaul**（`try_alloc_asid` 避开「硬件可用 ASID 全部钉死」时的 expect panic，是 base 形的防御升级；base 的锁外 arm 语义 overhaul 同款保留）。
- dpg.rs 2 段（:149/:287）：`klock_enter/klock_exit` 手动配对 + `demand_root` → `klock_enter_irqsave`（RAII）+ `demand_aspace`——**取 overhaul**（F0-3 IrqSaveGuard 形 + F1-1 改接一次到位）；base 侧 P1-3 ZFOD 快路径等新增注释/逻辑在两段之间的自动并区已保留（合并树实测）。
- user.rs 1 段（:1797）：Q106 vDSO map_into vs `register_boot_aspace`——**并集**（两句先后排：先 boot aspace 登记（overhaul，F1-1 退出链依赖）后 vDSO 映射（base Q106 资产保留）；vDSO 面在 overhaul 树零冲突（05 号正和件判定的文件级实证））。

## 零手工四文件

- justfile 1 段（:709）：overhaul 增 `berry-asidface`/`berry-vmarface` 两挂测行——11 号件已列手工点（base 侧 asidface 行已拆致冲突，解 = berry-mm 行后直插不复活死行）；本冲突实测为 base 侧无此二行、overhaul 有——按 11 号件方案随刀。
- Cargo.lock 2 段：机械冲突，`cargo update --workspace`（或任意 cargo 命令触发重锁）即平——05615c1e7 判例。
- unsafe台账 8 段：overhaul 侧台账是 stale 面（base 1214 条 vs overhaul 5469 unsafe 非空行 9.52% 旧账）——合并后 `python3 tools/unsafe_audit.py` 重产即解（工具唯一真值源，两线同在）；05 号义务 6 的 vmarface 零 unsafe 不入台账已实测。
- ktest.rs 段内 F0 调用注释里的 vmo.rs 面丢失复活：随「ktest.rs 主发现」段 vmo.rs 取 overhaul 自动解决。

## 回流施工序（10 文件总序）

1. 整文件取 overhaul：vmar.rs（10 段）→ 随带 aspace.rs/dpg.rs（各 2 段）+ vmo.rs/ktest.rs 的 F0-3/F0-4 定义体（自动并区）。
2. channel.rs 五段按 06 号六接缝 + 08 号段 5/6 组合重放（F1-3 骨架 + Q98 直拷窗 + Q97 消费单源 + disarm 义务）。
3. syscall.rs 六段按 08 号施工图（3 overhaul 权威 + 2 base 权威 + 1 组合重放）。
4. ktest.rs 调用序列并集（density + f0/f1 六调用，末位新号纪律）＋ 12 号件 E2 重钉三笔增量钉板同批。
5. user.rs 并集两句、justfile 两行随刀、Cargo.lock 重锁、台账重产、vmarface/asidface 入册（11 号件清单）。
6. 回归门：test-host 全量 + smoke（F1-2 2000 轮进程压测随 ktest 并集自动进套）+ wg/abi-check/docs-guard。

## 与其他文档的关系

- 义务来源：[05-base-overhaul回流接缝预研-W3面资产对撞审计.md](05-base-overhaul回流接缝预研-W3面资产对撞审计.md)（义务清单单源）；本件 = 义务 5 的「syscall.rs 之外九文件」补遗 + ktest F0 面复活义务新增登记。
- 姊妹件：[06-recv_n重放形态预研-ChannelShared接缝审计.md](06-recv_n重放形态预研-ChannelShared接缝审计.md)（channel.rs 五段细化单源）；[07-vslotface归并裁定预备-三查判序两形恒等性对拍审计.md](07-vslotface归并裁定预备-三查判序两形恒等性对拍审计.md)（vmar.rs 十段裁定依据）；[08-syscall逐段对拍清单预备-回流六冲突段施工图.md](08-syscall逐段对拍清单预备-回流六冲突段施工图.md)（syscall.rs 六段单源，本件复核段位零漂）；[11-vmarface入册清单预备-回流义务6深化-171判例族全量对账.md](11-vmarface入册清单预备-回流义务6深化-171判例族全量对账.md)（justfile/入册手工点）；[12-Q110E2跨段用例重钉形态预备-回流义务4深化-区间树批量形对拍审计.md](12-Q110E2跨段用例重钉形态预备-回流义务4深化-区间树批量形对拍审计.md)（ktest E2 重钉同批窗）。
- 证据单源：合并树 `9c0f779dbf0f9cc2627635547605673fbaceca38`（merge-tree 实跑可复现，base/overhaul 头与本件基线同）；F0 丢失面判据 = `git log --full-history -S 'fn test_f0_3' fceb03ff3..4f76d8414 -- kernel/src/ktest.rs` 零命中 + tip 定义/调用双零 + `3e323b8c9` D2 注记互证。
- 沿革：`docs/阶段沿革.md` 六百一十四节（随本批）。
