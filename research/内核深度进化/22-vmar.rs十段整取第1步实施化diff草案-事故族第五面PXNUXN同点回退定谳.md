# 内核深度进化 22 号：vmar.rs 十段整取第 1 步实施化 diff 草案——事故族第五面 PXN/UXN 同点回退定谳（22 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：13 号件把 vmar.rs 裁定为「10 段全取 overhaul」但止于段落清单、19/20/21 号件把随刀序①②③实装验证——本件把**④步 vmar.rs 十段整取从裁定升为逐段实施化 diff 草案**，并带出事故族**第五面**：`arch/aarch64/src/paging.rs` 的 `PXN`/`UXN` 两常量（95e934f27 F0-2 笔定义，fceb/overhaul 双线全活）base tip 全零——7f05be12e 并流静默剥除 + d25c75876 owner merge 蔓延笔定型，四问核全中（base=0/fceb=7/overhaul=7/-S 零删除笔），与 F0-1/F0-2/F0-3/F0-4 四面同源同判；overhaul `Perm::pte_bits()` 六档矩阵硬依赖 PXN/UXN——十段整取（段 3）不随刀复活 paging.rs 供体即第八连 E0425（vmar.rs 消费供体面首例，前七连均消费者先活供体后死）。

## 审计基线（2026-11-20 06:3x 直核）

- base 线 = `origin/wip/sched-1.1@44459a2d7`（21 号件本体笔即 base tip）；fceb = `fceb03ff3`；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）。
- merge-tree 现行实跑：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul` → 树 `25440ace1`，exit=1，冲突十文件与 13 号基线（`9c0f779dbf`）零漂；vmar.rs ours/theirs blob = `7129e3f6f`/`5aabe2145`。
- 十段冲突段位 = git merge-file 三方合并实跑（ours=base 918 行 / base=fceb / theirs=overhaul 508 行），段位 @:13/:32/:131/:190/:239/:263/:323/:504/:606/:846——与 13 号「10 段全取 overhaul」裁定文件面吻合，base 前进（Q106 vDSO/15 号 F0-2 笔/Q110 游标）未新增段。
- fceb 供体链核验：`git grep 'pub const PXN' fceb03ff3 -- arch/aarch64/src/paging.rs` = 7 命中；overhaul 同 7 命中（fceb≡overhaul 同形）；base tip 零命中（`pub const` 面全零，仅注释残迹 2 处 :38/:80）。

## 主发现一：事故族第五面——PXN/UXN 常量对 base 线静默回退

- **供体链**：`PXN: u64 = 1 << 53`、`UXN: u64 = 1 << 54`（paging.rs :37/:39）由 95e934f27（F0-2「全栈硬件 W^X/PXN 隔离」笔，2026-11-18）定义，fceb 与 overhaul 双线继承全活；`arch/aarch64/src/lib.rs` :209/:212 re-export（`PXN, SH_INNER, SH_NONE, UXN`）同步在 fceb/overhaul、base tip 零。
- **回退链定谳**（四问核）：base tip `pub const` 零命中 = 问一；fceb 7 命中 = 问二；overhaul 7 命中 = 问三；`git log --full-history -S 'pub const PXN' fceb03ff3..base` 零命中（无显式删除笔）= 问四——与 F0-1/F0-2/F0-3/F0-4 四面 18 号四问核同法同源，事故族第五面成立。
- **跳变点双笔**：7f05be12e（并流笔，2026-11-19 19:26）树里 PXN=0（fceb 内 95e934f27 是其祖先、无显式删除笔 = 静默剥除）；d25c75876（owner merge，b3a200533×27b35c111）b3a200533 侧 PXN=1、27b35c111 侧 0——owner 本地分支持有活形、origin 侧取回退态，蔓延定型 base tip 现状（18 号 owner merge 蔓延笔判例同款）。
- **与前四面的分档**：前四面（F0-1/F0-2/F0-3/F0-4 及 18 号 18 symbol）均为「消费者与供体同笔配套丢 → 回退静默无红」；第五面 = **供体丢而消费者活**——base tip vmar.rs `Perm::ap()` 硬编码 AP 位合成（Rw=00/Ro=10/UserRw=01/UserRo=11 四档），不触 PXN/UXN 故 base 无红；但第 1 步十段整取取 overhaul `pte_bits()` 形即引入消费（六档 `AP | PXN | UXN` 矩阵）——**第八连 E0425 预警：vmar.rs 段 3 整取与 paging.rs PXN/UXN + lib.rs re-export 复活必须同批随刀**。
- **第八连的特别之处（与前七连反向）**：前七连 = 消费者面自动并区全活、供体面全零（解冲突后 build 红）；第八连 = vmar.rs 本身是**冲突文件**（段 3 手工取 overhaul 形 = 人为引入消费），paging.rs 供体**不在冲突清单**（base 侧无演进、git 自动取 base 删态）——供体复活不随刀即红，且红源在冲突段手工解时即埋下，比前七连更隐蔽。
- **18 号件补遗**：42 候选里 PXN/UXN 在列（本轮 sym42 复跑实证），但被「13/15/17 号已登记面（vmar/aspace/asid/maru/vmo/ktest）」排除规则漏排——本轮补登，第五面并入 18 号事故族清单。

## 主发现二：十段逐段处置表（实施化 diff 草案核心）

段位实测（git merge-file，base 918 行×overhaul 508 行）——每段一句处置 + 手工调和面：

| 段 | 段位 | base 侧（918 行） | overhaul 侧（508 行） | 处置 |
|---|---|---|---|---|
| 1 | @:13 use 区 | `use crate::vmo::{zero_frame, Vmo};` + `Vec` | `use alloc::sync::Arc;` | 取 ovh，`zero_frame` 消费随段 8 supply 改接自动消亡 |
| 2 | @:32 常量区 | IDX_MASK/TABLE_BIT/ADDR_MASK/AP_MASK 四常量 | per-AS PT 锁守卫区（PtGuard/BOOT_ASPACES/register_boot_aspace） | 取 ovh；**调和面 A**：base `walk_leaf`（自动并区保留）引用 IDX_MASK/TABLE_BIT/ADDR_MASK——取 ovh 后须把 walk_leaf 内联字面量化（ovh 形 ：485-:502 已是字面量形）或保留常量区，两者取一，字面量形零残留 |
| 3 | @:131 Perm 区 | Q106 四档（Rw/Ro/UserRw/UserRo）+ ap() 硬编码 | F0-2 六档（+UserRx/UserRwxCompat）+ pte_bits() PXN/UXN 矩阵 + TryFrom<u8> + allows_exec | 取 ovh；**调和面 B（第八连）**：pte_bits 硬依赖 `hal::PXN/UXN`——paging.rs 供体复活同批随刀；**调和面 C（判别值换位）**：base `Perm::UserRw` 自动判别值 2、`UserRo`=3 vs ovh 显式 `UserRo=2`/`UserRw=3`——base 无 u8 序列化消费面（syscall 只走 rights 位掩码到 Perm 映射，无 `as u8`/`TryFrom` 消费，abi_gen 零命中实证）→ 判别值换位零 ABI 面，直接取 ovh 形 |
| 4 | @:190 | （base 无对应） | allows_exec + From/impl 块 | 取 ovh 纯新增 |
| 5 | @:239 | （base 无对应） | new_root_global + AddressSpace::new_global | 取 ovh 纯新增（F1-2 全局地址空间族） |
| 6 | @:263 | activate doc 注释 | base()/len()/vmar_id()/is_root()/aspace()/aspace_ptr() 存取器族 | 取 ovh（F1-1 存取器族）；base activate 语义由 aspace.activate() 承接 |
| 7 | @:323 map 主体 89 行 | pt_lock + check_slot + 两遍填叶 | aspace.map 委托（40 行） | 取 ovh；Q110 游标（l3_span_peek 段内推进）随 map 主体整取**接受消亡**（13 号裁定原文）——overhaul aspace.map 是区间树单遍形，游标优化由 aspace.rs 后续演进刀承接 |
| /unmap/protect 主体（段 9 内） | @:606 178 行 | pt_lock + find_mapped + Q110 l3_span_peek 游标形 protect | aspace.protect 单行委托 | 取 ovh；Q110 游标接受消亡（同段 7） |
| 8 | @:504 supply_page 58 行 | pt_lock + demand_index + 幂等门 + l3_slot_peek | aspace.supply_page 委托（29 行） | 取 ovh；supply_page 消费者 dpg.rs（:142/:150/:276/:282/:419/:495/:603）与 syscall.rs 同批改接（第 1 步 aspace/dpg 整取面，13 号已裁） |
| 10 | @:846 Drop 臂 | kl_held 门 + deregister_demand_root + regions 走查 untrack | is_root 分支 + destroy_vmar 单源 | 取 ovh；**调和面 D（21 号刀⑦衔接）**：base Vmar::drop 尾 free_asid 归还（21 号第 0 步刀⑦）随整取消亡，改由 AddressSpace::Drop → teardown → free_asid（aspace.rs :747/:864/:930）承接——ASID 归还闭环在 overhaul 形下移至 Arc<AddressSpace> 末引用 Drop，语义不变路径变 |
| — | syscall demand_root 消费 | dpg/ktest/syscall demand_root/register/deregister 消费活 | register_demand_aspace 族 | 第 1 步 aspace.rs 整取面同批（13 号已裁 vmar10+aspace2+dpg2 段） |
| — | vdso.rs `vmar.map(fa, vmo, VDSO_VUserRo)` | map 签名两形同（fa/vmo/vaddr/vmo_offset/len/perm） | 同签名 | 签名兼容零调和——vdso 调用面零改动（map 签名两线恒等实测） |
| — | berry-sys/abi_gen | map_demand/pt_lock 注释面 | — | 注释面零冲突，随刀 |

- **净面预估**：vmar.rs 918→508 行（-410 净）；随刀供体 paging.rs +2 常量行 + lib.rs re-export 1 行（fceb 形）；aspace/dpg/syscall/ktest 消费改接面属 13 号已裁随行件。
- **随刀序修正**：19 号随刀序④（冲突段解）实施化为 = 段 1/2/4/5/6 机械取 ovh → 段 3 Perm（连 paging.rs 供体 + PXN/UXN + lib.rs re-export 同批）→ 段 7/8/9/10 主体委托形 → walk_leaf 字面量化收尾（调和面 A）→ 消费改接（dpg/syscall/ktest，13 号随行面）→ 台账随源重产（预计 +4~6：pte_bits const fn 无 unsafe；destroy_vmar 链的 unsafe 面并入 aspace.rs 面）。

## 主发现三：第五面与 18 号清单的关系修正

- 18 号排除规则「13/15/17 号已登记面（vmar/aspace/asid/mmu/vmo/ktest）」漏排 PXN/UXN：42 候选 → 排除已登记面后 24 → 18 symbol 定谳——P[XY]N 因「vmar 已登记」被误并入排除（18 号扫描按文件归组，paging.rs 归入 vmar 面时未逐 symbol 复核 base 侧零命中）。本件补登，第五面并入事故族总清单。
- 消费链登记：overhaul 侧 PXN/UXN 消费 = vmar.rs pte_bits 6 命中 + aspace.rs（pte 合成面）+ mmu.rs F0-4 面（cache_poc 清理链，17 号已登记）——vmar 整取引入前两消费面。
- 施工判据（第八连补则）：**整取任一「引入新供体消费」的冲突段前，先核该段引用的 hal symbol 在 base tip 的存活性**——前七连判据「先复活供体」只覆盖「消费者文件整取带回既有消费」，第八连新增「冲突段手工解引入的消费面」同样适用供体先行原则。

## 与其他文档的关系

- 裁定单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)——「vmar.rs 10 段全取 overhaul」裁定的实施化，本件逐段处置表为施工单源；十段段位零漂实证（现行 base 前进未新增段）。
- 事故族判例谱：[15-…分代ASID…](15-F0-2分代ASID面base线静默回退定谳-回流义务5补遗二.md)（第二面）→ [17-…F0-1arch同点回退](17-回流第0步实施化-F0-1与F0-2复活随刀diff草案-事故族第三面F0-1arch同点回退.md)（第三面）→ [18-…18symbol](18-事故族第四面排查定谳-7f05be12e供体复活随刀序实施化与17号vmar六调用点表述修正.md)（第四面）→ 本件（第五面）；18 号清单补登 PXN/UXN 笔。
- 实装链：[19-…第六连](19-第六连E0425合并树实证-供体复活随刀序实施化与vmar六调用点表述修正.md)（随刀序）→ [20-…①②](20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md)（①②实装）→ [21-…③](21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md)（③实装）→ 本件（④实施化草案：十段逐段处置表 + 第五面 + 第八连预警）。
- 证据可复现：merge-tree = `git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul`（树 `25440ace1`，exit=1，vmar ours/theirs `7129e3f6f`/`5aabe2145`）；十段段位 = `git merge-file -p <ours> <fceb> <theirs>`（10 个 `<<<<<<<` 标记实测）；PXN 四问 = `git grep -c 'pub const PXN' {base,fceb,overhaul} -- arch/aarch64/src/paging.rs` = 0/7/7 + `git log --full-history -S 'pub const PXN' fceb03ff3..origin/wip/sched-1.1` 零命中；蔓延笔 = `git show d25c75876^1:arch/aarch64/src/paging.rs | grep -c PXN`（b3a200533 侧=1）vs `git show d25c75876^2:…`（27b35c111 侧=0）。
- 沿革：`docs/阶段沿革.md` 六百二十九节（随本批）。
