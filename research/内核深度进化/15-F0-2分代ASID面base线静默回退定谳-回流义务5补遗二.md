# 内核深度进化 15 号：F0-2 分代 ASID 面 base 线静默回退定谳——谱系实锤、合并树隐藏编译红与施工序补遗（回流义务 5 补遗二）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：13 号件抓到 F0-3/F0-4 自检面在 base 线静默丢失（同病三处），本件把同一事故的 **F0-2 全家面**（berry-asidface 接线 + mm 分代位图分配器 + kernel try_alloc_asid/free_asid 耗尽防御 + arch active_cpu_asids/TCR.AS 面）补齐定谳：base 第一父链上 AsidState 3→0 的唯一切换点 = `7f05be12e`（2026-11-19 19:26:38「merge: origin/wip 并流（G0 刀 2 件随流）」），v1 回退形源头 = w1 侧枝谱系（PR #511 merge 节点 `1008dcdc7` 起 v1 形，经「二级合龙」`e022cf3c8`〔提交信息自述「asidface 目录孤儿化对方线已无」——把事故当收口自述〕随 PR #538 回灌 wip 主线）；`-S` 全历史零显式删除提交，与 13 号件 F0-3/F0-4 完全同病（活线合并单边取旧 blob 静默回退）。**增量后果**：11 号件「不复活 asidface 死行」裁定方向性错误须修正（overhaul 线 F0-2 全活，回流取 overhaul aspace.rs 即要求 asidface 复活）；merge-tree 合并树 `acbcc8d2` 自带隐藏编译红三连 + 挂测行孤儿红（`cargo test -p berry-asidface` 时 crate 不在 members）；13 号件施工序第 1 步（aspace.rs 整取 overhaul）若无 F0-2 复活随刀直接编译红——施工序须插前置步。

## 审计基线（2026-11-20 00:3x 直核）

- base 线 = `origin/wip/sched-1.1@c8c58f4cc`；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`（未前进）；merge-base = `fceb03ff3`（F0-4 笔，**F0-2 全家俱在**：`95e934f27`（2026-11-18 12:10，F0-2 落地笔）是其祖先）。
- merge-tree 实跑：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul` → 合并树 `acbcc8d2`（10 文件 38 段冲突，与 13 号件基线同构）。
- 谱系扫描法：第一父链逐树 `git show <c>:mm/src/asid.rs | grep -c AsidState`（F0-2 形恒 3、v1 轮转形恒 0）。

## 谱系实锤：回退链全程（谁、何时、经哪条线洗掉）

- **wip 主线从未主动拆 F0-2**：第一父链自 `95e934f27`（11-18 12:10 落地）至 `94e302566`（11-19 19:26:34，G0 刀 2 VBT 槽位化）连续 35+ 笔 AsidState=3（时间轴 11-18 12:10 → 11-19 19:26 全程含 fceb03ff3/c28917c45/e87f079b8/87243fdd1/2af914845/64d9396b9/efde379ea/27b9a7f27/d202ad408/c3f1bbba4）。
- **唯一切换点 = `7f05be12e`**（11-19 19:26:38，仅晚于 94e302566 四秒）：「merge: origin/wip 并流（G0 刀 2 件随流）」，第二父 = `3ed0a1d11`（Merge PR #538 from hermes/w1-202611191607）。该 merge 后 F0-2 全家八文件同时归零（mm/src/asid.rs 84 行回 v1、mm/Cargo.toml asidface dep 摘、根 members 摘 asidface、justfile 挂测行摘、arch/aarch64/Cargo.toml dep 行摘、arch mmu.rs F0-2 面摘〔active_cpu_asids/TCR_AS/tcr_as_config 由 4 命中归 0〕、kernel syscall.rs ENOMEM 消费面摘、ktest F0-2 用例摘）——net -1956/+1026。
- **v1 形源头在 w1 侧枝**：`3ed0a1d11` 谱系内 `e022cf3c8`（11-19 07:50，「二级合龙吸收 origin 双分支新进——代码域整取 origin/wip 最新态（55 目录 wholesale）……asidface 目录孤儿化对方线已无」）树已 v1 形；其 v1 来自第二母 `e2ff00045`（merge-icount 线），后者谱系内 `1008dcdc7`（11-19 06:20，Merge PR #511 from hermes/w1-202611190400）树已 v1 形——w1 分支 base 点早于 F0-2 落地，merge 解算整文件取了 w1 侧老态。**即 w1 线 merge 解失误 → 「二级合龙」wholesale 整取固化 → 随 PR #538 回灌 wip 主线**，三跳接力完成静默回退。
- **同病互证**：`git log --full-history -S 'try_alloc_asid' fceb03ff3..origin/wip/sched-1.1 -- kernel/src/aspace.rs` 与 `-S 'AsidState' -- mm/src/asid.rs` 双零命中（无任何显式删除提交）——与 13 号件 F0-3/F0-4（vmo contig/0x1C/IrqSaveGuard）完全同签名：活线合并单边取旧 blob 静默回退，`3e323b8c9` 提交注记「D2 并行发现 vmo.rs 丢 F0-3/F0-4 contig 面」是同批事故的已点名部分，F0-2 是未被点名部分。base 树残留的 `asidface/` 孤儿目录（与 fceb 版逐字节一致）与 arch Cargo.toml 注释里的两处「asidface 迁 mmu.rs 面同款纪律」引文，即回退事故的化石证据。

## 两线 F0-2 面对照（overhaul 全活实证）

| 触点 | fceb03ff3（merge-base） | base tip（c8c58f4cc） | overhaul tip（492d5e00c） |
|---|---|---|---|
| asidface/ 目录 | 在（F0-2 改造版） | 在（孤儿，树内零消费） | 在（与 base 孤儿逐字节一致） |
| 根 Cargo.toml members | 含 asidface | 摘 | 含 |
| mm/Cargo.toml dep | 含 | 摘 | 含 |
| arch/aarch64/Cargo.toml dep 行 | 含 | 摘（仅剩注释引文） | 含 |
| mm/src/asid.rs | AsidState 分代位图（pinned 钉死/free/耗尽 None） | v1 裸轮转计数器 | 分代位图（未触 fceb 形） |
| kernel aspace.rs | try_alloc_asid/free_asid | 无 | 有（含 asid_lock 防御形） |
| kernel syscall.rs | 耗尽返 -ENOMEM + 失败归还 | 无 | 有（:1651/:1622 同 fceb 形） |
| arch mmu.rs | active_cpu_asids/TCR_AS/tcr_as_config | 零命中 | 有（:314 等） |
| ktest | F0-2 用例 | 无（grep test_f0_2=0，注意 fceb 亦 0——F0-2 验收在 smoke/单测非 ktest run() 序列） | 无 ktest 面 |

要点：overhaul fork 于 fceb 后**从未触** mm/src/asid.rs 与 asidface/（`git log fceb03ff3..origin/wip/base-overhaul -- mm/src/asid.rs` 空）——F0-2 是双方共同资产，base 侧单方丢失后，回流合并的三方解会把 base 的「删除」当单侧改动自动接受，**不会自动复活**。

## 合并树 `acbcc8d2` 隐藏编译红（四连）

1. **AsidAllocator::try_alloc 方法缺失**：合并树 kernel/aspace.rs 冲突段若按 13 号件裁定取 overhaul 侧（`try_alloc_asid` 带 `pinned` 探测形），其调用 `.try_alloc(&pinned)` 在合并树 mm/src/asid.rs（base v1 形）无此方法 → E0599。
2. **free_asid 调用链断裂**：overhaul aspace.rs `:747 free_asid(asid)` 与 syscall.rs `:1651` 归还臂依赖 `AsidAllocator::free`——v1 形无此方法 → E0599；且 base v1 分配器只发不还，ASID 泄漏面回归。
3. **hal::active_cpu_asids 缺失**：overhaul aspace.rs `:898 berry_hal_aarch64::active_cpu_asids()` 在合并树 arch mmu.rs（base 形）零命中 → E0425。
4. **挂测行孤儿红**：合并树 justfile `:711 cargo test -p berry-asidface --target {{host}}` 已自动复活（冲突段外自动并区），而根 members 无 asidface → `cargo test -p berry-asidface` unknown package，test-host 门即红。

第 1-3 项在 13 号件施工序第 1 步（「整文件取 overhaul：vmar.rs → 随带 aspace.rs/dpg.rs」）落地瞬间触发——施工序缺前置步，本件补遗。

## 11 号件裁定修正（「asidface 死行不复活」→「复活」）

11 号件触点 4 把 base 侧 asidface 挂测行已拆定性为「F0-2 二级合龙收口」，施工案写「不复活 asidface 死行」。本件谱系实锤推翻该前提：这不是收口，是二级合龙事故的静默回退（见上「回退链」）——overhaul 线 F0-2 全活且回流施工强依赖它（aspace.rs/ syscall.rs/ arch mmu.rs 三面消费）。修正：回流时 asidface 五接线随 overhaul 侧复活——members 行、mm/Cargo.toml dep、arch/aarch64/Cargo.toml dep 行、justfile 挂测行（合并树已自动带回）、mm/src/asid.rs 取 overhaul 分代位图形；复活后 11 号件触点 4 的 vmarface 插行锚点（「berry-mm 行后直插」）不受影响，但「不复活 asidface 死行」句须随回流施工案一并改写。asidface/ 目录本体无需搬运：base 孤儿目录与 overhaul 版逐字节一致（`git diff origin/wip/base-overhaul -- asidface/` 空），纯接线复活。

## 13 号件施工序补遗（第 0 步插入）

13 号件「回流施工序（10 文件总序）」第 1 步前插 **第 0 步：F0-2 复活随刀**——①根 members 补 asidface 行；②mm/Cargo.toml 补 dep；③arch/aarch64/Cargo.toml 补 dep 行；④mm/src/asid.rs 整取 overhaul 分代位图形（AsidState/try_alloc/free/active_count/generation）；⑤arch mmu.rs F0-2 面取 overhaul（active_cpu_asids/TCR_AS/tcr_as_config，W^X/PXN 隔离面同批）；⑥kernel syscall.rs ENOMEM 消费段取 overhaul（try_alloc_asid 折返 + free_asid 归还臂——该段不在六冲突段单内，属 base 单侧删除自动接受区，须手工恢复）。完成后再进原第 1 步（vmar.rs/aspace.rs/dpg.rs 整取），四连编译红消解。回归门补一项：overhaul asidface 单测（width8 轮次复用不撞存活号用例）随挂测行复活自动进套。

## 与其他文档的关系

- 事故族单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)（F0-3/F0-4 静默丢失主发现——本件补 F0-2 面，同一事故的未被点名部分；其施工序六步由本件插第 0 步补遗）。
- 修正对象：[11-vmarface入册清单预备-回流义务6深化-171判例族全量对账.md](11-vmarface入册清单预备-回流义务6深化-171判例族全量对账.md)（触点 4「不复活 asidface 死行」前提错误，修正方向见本件）。
- 义务来源：[05-base-overhaul回流接缝预研-W3面资产对撞审计.md](05-base-overhaul回流接缝预研-W3面资产对撞审计.md)（回流义务清单单源；本件 = 义务 5 补遗二，F0-2 复活随刀列入回流义务增补项）。
- 证据单源：合并树 `acbcc8d20280efabf2cd12929d4bb8e8fd7c8dc5`（merge-tree 实跑可复现）；谱系切换点 `7f05be12e`/`3ed0a1d11`/`e022cf3c8`/`1008dcdc7`（`git log --full-history -S` + 第一父链逐树扫描可复现）；overhaul 未触 mm/asid.rs 实证 = `git log fceb03ff3..origin/wip/base-overhaul -- mm/src/asid.rs` 零命中。
- 沿革：`docs/阶段沿革.md` 六百一十六节（随本批）。
