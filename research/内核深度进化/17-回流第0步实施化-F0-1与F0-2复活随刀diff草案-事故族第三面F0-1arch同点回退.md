# 内核深度进化 17 号：回流施工序第 0 步实施化——F0-1/F0-2 复活随刀逐步 diff 草案＋F0-1 arch 面同点回退新发现（事故族第三面）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：13 号件施工序六步是总账，15 号件给它插了第 0 步（F0-2 复活随刀）但只给触点清单未给实施 diff——本件把第 0 步落成逐步 diff 草案（六触点逐 hunk 可直接施工），并在实施化扫描中抓到**事故族第三面：F0-1 的 arch 面（CTR_EL0 PoU 缓存一致性）在 base 线同点 `7f05be12e` 静默回退**（mmu.rs `sync_icache_pou`/`CTR_CACHE` 5 命中→0、arch Cargo.toml memface dep 1→0、lib.rs re-export 摘除），与 F0-2 同笔同签名；15 号第 0 步③⑤须扩为 F0-1+F0-2 复合面——否则 13 号第 1 步 vmar.rs 整取 overhaul 后其 6 个 `hal::sync_icache_pou` 调用点 E0425 第五连编译红。

## 审计基线（2026-11-20 02:4x 直核）

- base 线 = `origin/wip/sched-1.1@370532d24`；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`（未前进）；merge-base = `fceb03ff3`（15 号件基线后零笔触第 0 步六触点文件，新鲜度全持）。
- merge-tree 实跑（`--merge-base=fceb03ff3`）→ 合并树 `c001704bc5`（10 文件 38 段，与 13/15 号件同构、段位零漂）。
- 合并树关键自动并区取证：justfile asidface/vmarface 挂测行自动复活（:711/:712）而根 members/mm dep/mmu.rs/asid.rs 全取 base 删态——15 号预判逐项实证。

## 主发现：F0-1 arch 面同点回退（15 号件 F0-2 清单外）

- **回退实锤**：`7f05be12e~1`（`94e302566`）mmu.rs F0-1 面 5 命中、arch Cargo.toml memface dep 1 行；`7f05be12e` 本树双零——与 F0-2 全家（AsidState 3→0 等）同笔同签名，`-S` 全历史零显式删除，活线合并单边取旧 blob 静默回退事故族第三面（13 号 F0-3/F0-4、15 号 F0-2 之后）。
- **丢面清单（base tip 实测全零）**：`arch/aarch64/src/mmu.rs` 的 `sync_icache_pou`（CTR_EL0 惰性探测 `CTR_CACHE` + IDC/DIC 分治 `dc cvau`/`ic ivau`/`ic ialluis` + dsb/isb 收口）；`arch/aarch64/src/lib.rs:183` re-export；`arch/aarch64/Cargo.toml` `berry-memface` dep 行。注：kernel 侧 memface 消费（`kernel/src/mem.rs` W3-Q80 块尾算法族）未丢——那是 W3 自己的资产；丢的仅 F0-1 在 arch 的接线。
- **overhaul 侧全活且未触**：`git log fceb03ff3..origin/wip/base-overhaul -- arch/aarch64/src/mmu.rs arch/aarch64/src/lib.rs` 零命中，`git diff origin/wip/base-overhaul fceb03ff3 -- arch/aarch64/src/mmu.rs` 零行——overhaul mmu.rs ≡ fceb 形。因此合并树 mmu.rs **无冲突自动取 base 删态**（合并树实测 F0-1/F0-2 面零命中），回流不复活即丢。
- **消费链依赖（第五连编译红预警）**：fceb vmar.rs 六调用点（map/protect/unmap 的可执行叶 PTE 前刷 PoU，:397/:461/:588/:646/:737 等）+ ktest test_vmo G 段——13 号第 1 步 vmar.rs 整取 overhaul 自动带回六调用点，若 mmu.rs/lib.rs 不随刀复活则 `hal::sync_icache_pou` E0425。
- **ktest G/H 验收面静默丢失（非编译红、更隐蔽）**：test_vmo 的 G 段（F0-1：DCZID_EL0 探测 + supply_page_init「先填帧刷 PoU、后置有效叶 PTE」时序断言）与 H 段（F0-2：.rodata 拒写/PXN 不可执行/ASID 位图归还）在 base tip 全失（`zva_block_size_probed`/`sync_icache_pou`/`free_asid(t_asid` 零命中）、overhaul 全活；ktest.rs 冲突段仅 run() 尾部一段，G/H 段处自动并区被静默接受 base 删态——**无编译红、纯测试面丢失**，13 号件 ktest 并集解只覆盖了调用序列，G/H 函数体复活义务本件登记。

## 第 0 步逐步 diff 草案（F0-1+F0-2 复合面，可直接施工）

按依赖序七点（15 号六点扩为七点；操作均为「取 overhaul 侧」，overhaul 形 ≡ fceb 形无三方分歧）：

1. **根 `Cargo.toml` members 两行**：`"vmarface"`（vslotface 前）与 `"asidface"`（aokface 后 wsrcface 前）直插——合并树实测 members 零命中（base 删态自动接受），diff 即 `git diff origin/wip/sched-1.1 origin/wip/base-overhaul -- Cargo.toml` 的两个 `+` 行。
2. **`mm/Cargo.toml`**：`[dependencies]` 节 + `berry-asidface = { path = "../asidface" }` 一行（base 现「无外部依赖」注释行随之替换）。
3. **`arch/aarch64/Cargo.toml` 两行**：`berry-memface`（F0-1，含注释行「F0-1：ARMv8 CTR_EL0 PoU 指令/数据一致性解码」）+ `berry-asidface`（F0-2，含注释行）——15 号件③只列了 asidface，本件修正补 memface。
4. **`mm/src/asid.rs` 整取 overhaul**：v1 轮转计数器形（`width/next` 字段 + `alloc()` 回卷信号）→ AsidState 分代位图形（`inner: AsidState` + `try_alloc(&pinned)`/`alloc()`（expect 便捷形）/`free()`/`active_count()`/`generation()`/`force_near_roll()`）；单测同步换（`free(1)` 后循环补号、`(255, true)` 回卷、新轮只复用已释放号 `assert_eq!(a.alloc(), (1, false))`、`try_alloc(&[])` 耗尽 `None`）。diff = 42 行替换 42 行。
5. **`arch/aarch64/src/mmu.rs` 整取 overhaul**（≡fceb 形，一段操作双面复活）：F0-1 面（`use berry_memface::{cache_line_span, ctr_dmin_line, ctr_pou_plan, IcachePolicy}` + `static CTR_CACHE` + `sync_icache_pou` 体）+ F0-2 面（`use berry_asidface::{tcr_as_config, ASID_SHIFT}` + `static ACTIVE_CPU_ASID: [AtomicU16; 2]` + `active_cpu_asids()` + `asid_shift` const 化 + `activate_ttbr0_regs` 内 Acquire/Release 记账 + `tcr_as_config` 消费）。**留意点**：base 侧 fceb 后五笔 R490 系（SYSCALLS 表固化只读/邻页盲冻撤除等）全落在 mmu.rs，整取 overhaul 会洗掉——R490 五笔须按「真源重建 + 增量补丁」在整取后重放（五笔清单：`763327bec`/`e12c61d75`/`705ebb604`/`f409015c2`/`f575553c7`，涉及 SPLIT_L3_SYSCALLS/table_addr/SYSCALLS_RO 置序）；这是第 0 步唯一的非纯取件点。
6. **`arch/aarch64/src/lib.rs` re-export 复活**：`pub fn sync_icache_pou(kva: usize, len: usize) { mmu::sync_icache_pou(kva, len) }` 两行（overhaul ≡fceb :183 形）——15 号件未列，本件新增触点。
7. **`kernel/src/syscall.rs` ENOMEM 消费段手工恢复**（base 单侧删除自动接受区）：`sys_process_create` 取号段 base 形 `Vmar::new_root(crate::mm::frames(), crate::aspace::alloc_asid())` → overhaul 形 `let Some(new_asid) = crate::aspace::try_alloc_asid() else { return ERR_NO_MEMORY }` + `Vmar::new_root_global(new_asid)` + 失败臂 `crate::aspace::free_asid(new_asid)` 归还（:1645/:1651 形）；注意 overhaul 形 P3-3 配额臂与 F1-2 drain-assist 臂在合并树自动并区已并存（实测 2 命中），本点只动取号窗。ktest G/H 段复活随 13 号第 4 步 ktest 并集同批（G/H 函数体按 overhaul 侧恢复，义务登记见上）。

完成判据：`cargo build`（aarch64 target）零 E0599/E0425；`just test-host` 的 asidface/vmarface 挂测行（合并树 :711/:712 已自动带回）不再 unknown package；15 号四连 + 本件第五连全消。

## 与其他文档的关系

- 实施化对象：[15-F0-2分代ASID面base线静默回退定谳-回流义务5补遗二.md](15-F0-2分代ASID面base线静默回退定谳-回流义务5补遗二.md)（第 0 步触点清单单源——本件③⑤⑥扩为 F0-1+F0-2 复合面并新增第 6 点 lib.rs re-export；其「五接线」表述随本件升为七点）。
- 总序单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)（六步总序；第 0 步实施化在本件、第 4 步 ktest 并集增补 G/H 函数体复活义务）。
- 事故族谱：13 号（F0-3/F0-4）→ 15 号（F0-2）→ 本件（F0-1 arch 面）——同点 `7f05be12e` 三面一族，`3e323b8c9` D2 注记是族名来源。
- 恒等区佐证：[16-try_alloc_asid两形语义对拍-ASID恒等区与destroy路径语义差定谳-Q110游标面消亡清单件.md](16-try_alloc_asid两形语义对拍-ASID恒等区与destroy路径语义差定谳-Q110游标面消亡清单件.md)（ASID 分配面两形恒等=第 0 步 4/7 点零语义分歧的旁证）。
- R490 重放纪律：AGENTS.md 分支拓扑纪律「真源重建 + 增量补patch + 两向审计」；五笔清单见本件第 5 点。
- 证据单源：合并树 `c001704bc5e4aa29b4d7eeced896c65994655170`（merge-tree 实跑可复现）；F0-1 回退判据 = `git show 94e302566:arch/aarch64/src/mmu.rs | grep -c 'ctr_el0\|berry_memface'` = 5 vs `7f05be12e` 同口径 = 0，及 arch Cargo.toml 同笔 1→0。
- 沿革：`docs/阶段沿革.md` 六百二十节（随本批）。
