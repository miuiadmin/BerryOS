# 内核深度进化 16 号：try_alloc_asid 两形语义对拍——ASID 恒等区零手术与 destroy 路径语义差定谳（Q110 游标面消亡清单件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：13 号件施工序第 1 步「aspace.rs 整取 overhaul」前，把 kernel/src/aspace.rs 的 ASID 分配面两形（merge-base `fceb03ff3` vs overhaul `492d5e00c`）逐 token 对拍定谳：**四函数（try_alloc_asid/alloc_asid/free_asid/asid_force_near_roll）代码本体零差异**——剥注释剥空行 diff 仅剩 4 处纯空白行残差，含锁内 `roll_gen_bump()`（代数序=取号序契约）与锁外 `roll_quiesce_arm()` 武装序两线同形；syscall.rs ENOMEM 消费段（try_alloc_asid 耗尽折返 + 建表失败 free_asid 归还臂）亦逐字一致。即 ASID 分配面属**自动合并恒等区，回流零手术**——15 号件第 0 步「F0-2 复活随刀」复活的是 base 侧被静默回退的**缺席**（61c279132 上 try_alloc_asid/free_asid 函数体全失、mm/asid.rs AsidState=0 回 v1 轮转形），而非两形分歧。真语义差全部集中在 **destroy 路径架构**（fceb 纯字段 + Drop 几何 is_root 判定 vs overhaul AtomicU16 + 幂等 teardown + 现役 TTBR0 切空臂），且该差随 13 号件第 1 步 aspace.rs/vmar.rs 整取 overhaul **自动承接**，无需独立施工步。Q110 游标面消亡清单补完：ASID 面两线均无游标残留（overhaul 分配器走 asidface 位图 cursor，非 Q110 页表游标）。

## 审计基线（2026-11-20 01:4x 直核）

- base 线 = `origin/wip/sched-1.1@61c279132`（PR #562 合并笔）；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`；merge-base = `fceb03ff3`。
- 对拍法：`git show <c>:kernel/src/aspace.rs` 取段 → `sed 's/\/\/.*$//'` 剥行注释 → 去空行 → 去行首缩进 → `diff`。fceb 段 `:18-116`、overhaul 段 `:874-962`（行号漂移因 F1-1 架构重排，段本体连续完整）。
- 复现残差：仅 4/5/37/74 行序处的「纯空格行」差（fceb 侧多 4 行空壳），代码 token 全等。

## 恒等面实锤（零手术区）

| 触点 | fceb03ff3 | overhaul 492d5e00c | 判 |
|---|---|---|---|
| try_alloc_asid | `:45-73`：active_cpu_asids 钉死探测 → ASID_LOCK 临界区 → 惰性初始化 → try_alloc(&pinned) → `if let Some((_, true))` 锁内 roll_gen_bump → 锁外 rolled 即 roll_quiesce_arm | `:896-924` 同语句序 | **token 全等**（仅注释详略：fceb 侧多锁序第六层位/SAFETY 详注/S-4 4-3 回卷语义段） |
| alloc_asid | `:75-77`：try_alloc_asid().expect（boot/ktest 可信语境便捷入口） | `:925-927` 同 | **token 全等** |
| free_asid | `:80-95`：0 短路 → 锁窗 → alloc.free(asid) | `:930-945` 同 | **token 全等** |
| asid_force_near_roll | `:99-112`：锁窗快进（smoke-smp asidroll 门专用） | `:948-961` 同 | **token 全等** |
| asid_lock/ASIDS/ASID_LOCK | TTAS 自旋（CAS 外环 + Relaxed 内环）+ lockdep 取放点两枚 | 同 | **token 全等** |
| syscall.rs ENOMEM 段 | `:1615-1640`：活进程帽 → try_alloc_asid else 返 ERR_NO_MEMORY → new_root_global 失败 free_asid(new_asid) 归还 → 表满/gen 尽 vmar 正常 drop 无泄漏 | `:1640-1665` | **逐字一致** |

回流判：merge-base 即 fceb（F0-2 全家俱在）且两线 fork 后**均未触本段**（`git log fceb03ff3..492d5e00c -- kernel/src/aspace.rs` 仅 2 笔 = F1-1/F1-2 架构重排，ASID 四函数段移位不改体）——三方合并本段无冲突、无单侧改动，**自动合并恒等区**。

## base tip 缺席实况（61c279132——15 号件已定谳，本件补强）

- kernel/src/aspace.rs 仅剩 v1 形 `alloc_asid`（`:50` 计数器轮转）与 `asid_force_near_roll`（`:78`）；try_alloc_asid/free_asid 函数体全失（grep 零命中，仅注释残引）。
- mm/src/asid.rs AsidState 零命中（v1 裸轮转计数器形）。
- v1 形保留 S-4 4-3 契约骨架：锁内 roll_gen_bump + 锁外 roll_quiesce_arm 武装序仍在——F0-2 复活（15 号件第 0 步④）换分代位图分配器后此契约面**不变**（恒等段实锤），复活施工可预期零回归。

## destroy 路径语义差定谳（随第 1 步自动承接）

- **fceb 形**：Vmar 纯字段 `asid: u16`（vmar.rs `:63` PT_LOCKS 键 / `:240` 结构字段）；Drop（`:1005-1018`）走 `is_root() && asid != 0` → tlbi_aside1 + free_asid。`is_root` 是**几何判定**（`base == PAGE_SIZE && len == USER_VA_TOP - PAGE_SIZE`，`:288-290`）。
- **overhaul 形**：AddressSpace `asid: AtomicU16`（aspace.rs `:151`）；`teardown()`（`:715-750`）= `torn_down` 幂等门（swap true 二进即 O(1) 返回）→ **现役 TTBR0 切空臂**（`current_ttbr0_pa() == self.l1_pa` → `activate_aspace(0, 0)`）→ `asid.swap(0)` 单次语义 → deregister_demand_aspace → drain_all/clear_leaves 清叶 → `dsb ish` + tlbi_aside1 → 页表帧全回收（F1-2 两阶段退出）→ free_asid；Drop → teardown() 接线（`:864-870`）。
- **切空臂非 fceb 缺陷，是语境差**：fceb 单阶段退出（sched 切走后 drop，Drop 语境必不在本空间上运行）该臂不可达；overhaul F1-2 允许进程**末线程先行显式 teardown**，此时现役 TTBR0 可能正指本空间——切空臂是两阶段退出的结构性新增。回流整取 aspace.rs/vmar.rs（13 号件第 1 步）即自动带回，无需独立施工步。
- **幂等门 + swap(0) 单次语义**防双释放（teardown 后 Drop 再入 O(1) 短路）——fceb Drop 无重入风险（drop 即死），语境差同源。

## asidface 双代数并存澄清

AsidState::alloc 内部自带 `generation`（asidface/src/lib.rs `:171-198`：cursor 回卷即 wrapping_add(1)），与 tlbshoot `ROLL_GEN` 全局代数是**两套账**：前者是分配器私有语义（回卷跳过存活号的轮次标记），后者是 quiesce 协议代数（代数序=取号序，锁内 bump）。F0-2 形两者并存且职责正交，恒等段实锤两线同接——回流无歧义面。

## Q110 游标面消亡清单收口

Q110（aspace 页表游标）两线均未承接（12 号件已定谳 overhaul 走区间树批量形）。本件补完 ASID 面：两形分配器均无 Q110 游标残留——overhaul 的 `cursor` 是 asidface 位图游标（ASID 号空间线性扫），与页表游标（VA 空间表树走查）无涉。消亡清单至此 ASID 面闭合，与 12 号件区间树定谳拼成完整消亡账。

## 与其他文档的关系

- 施工序单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)（第 1 步 aspace.rs 整取 overhaul——本件实证其 ASID 段为恒等区自动合并，destroy 语义差随整取自动承接）。
- 复活随刀单源：[15-F0-2分代ASID面base线静默回退定谳-回流义务5补遗二.md](15-F0-2分代ASID面base线静默回退定谳-回流义务5补遗二.md)（第 0 步五接线——本件补强：复活后 S-4 4-3 武装序契约面不变，恒等段可预期零回归）。
- 游标消亡单源：[12-Q110E2跨段用例重钉形态预备-回流义务4深化-区间树批量形对拍审计.md](12-Q110E2跨段用例重钉形态预备-回流义务4深化-区间树批量形对拍审计.md)（Q110 消亡主定谳——本件补 ASID 面闭合）。
- 证据单源：token 对拍命令链（本文件「审计基线」节可复现）；`git log fceb03ff3..492d5e00c -- kernel/src/aspace.rs` = 2 笔（9f113a65d F1-2 两阶段退出 + 2a9abb05d F1-1 区间树解耦）。
- 沿革：`docs/阶段沿革.md` 六百一十八节（随本批）。
