# 内核深度进化 27 号：ktest.rs 段 1 并集解与 F0-3/F0-4 复活面 diff 草案——第十一连 deletion-wins 供体链整链静默丢弃定谳（27 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：13 号件把 ktest.rs 列为「唯一冲突段 = run() 尾调用序列并集解，F0-3/F0-4 定义体自动并区带回」——本件以三方 merge-file 实跑（ours 13484 / fceb 12438 / ovh 13170 行，冲突恰 1 段 @:177-194 与 13 号零漂）升为逐段 diff 草案，并带出事故族**第十一连定谳：F0-3/F0-4 两定义体在合并树被 deletion-wins 静默丢弃**（fceb↔ovh 逐字节同 blob + base 线单方删 → git 静默取删侧，无冲突无编译红），13 号「自动并区带回」主张两面皆破——既不在 ktest.rs 自动并区（merged `fn test_f0_3/4` 计数 0），也不在 vmo.rs/mm.rs「自动并区保留」（两文件非冲突文件但合并树取 ours blob，contig/global 面 0 命中）；连带 ABI 常量面（`NR_VMO_CACHE_OP`/`VMO_OPT_CONTIGUOUS` 族在 abi_gen.rs 合并树 0 命中）与四供体文件（kern_lock 双 guard/tlbshoot `test_inline_consume_once`/mm `allocated_pages`/vmo `commit_global` 族）全链同笔静默丢弃，解冲突后即使手工贴回 f0_3/f0_4 定义体仍 E0425/E0432 ×N——复活义务从「ktest 单文件」升为「五文件贴回链」，施工序第 1 步「随带 vmo.rs/ktest.rs 的 F0-3/F0-4 定义体（自动并区）」须整体改写。

## 审计基线（2026-11-20 09:0x 直核）

- base 线 = `origin/wip/sched-1.1@bc15265c7`（#591 合流 + NET 十轮 sweep + 三轮遗漏清偿 tip）；fceb = `fceb03ff3`；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）。
- merge-tree 实跑：`git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul` → 树 `a44daaa0f`（exit=1，冲突 10 文件与 13/22/24/25/26 号基线零漂）。
- ktest.rs 段位 = 三方 `git merge-file -p`（ours 13484 / base 12438 / theirs 13170 行，exit=1）——冲突恰 1 段 `@:177-194`（run() 尾调用序列），与 13 号「唯一冲突段」基线零漂；13 号基线时点（ours=4f76d8414）复现同结论（f0_3/4 定义 merged=0），非 base 前进新触点。
- syscall.rs 六段段位复核（合并树实测）：:58/:698/:1205/:1220/:3654/:3983——08 号施工图零漂。

## 主发现一：段 1 并集解解剖（13 号裁定的实施化）

冲突段（merged :177-194）两侧内容与处置：

| 侧 | 内容 | 处置 |
|---|---|---|
| ours（base） | `test_density_ledger()` 调用 + 机会五③注释四行（per-RQ 密度账本挂 link_back/unlink + settle 在环改档） | 并集保留（base 线新资产，fceb 无此面） |
| theirs（overhaul） | `test_f0_3()/test_f0_4()/test_f1_1()/test_f1_2()/test_f1_3()` 五调用 + F0/F1 注释五行 | 并集保留（末位新号纪律——排 `ktest done` 前；F1-1/2/3 定义体已自动存活见主发现三） |

- 并集解零语义冲突：两侧调用互不排斥，density 是 ours 独有面（fceb 0）、F0/F1 族是 theirs 侧全带面；排序按「末位新号纪律」排入 `ktest done` 前（13 号口径）。
- 13 号「回归义务」段的五处 committed 水位写点（commit_global 逐页 +1 / decommit_global −1 / contig 建账臂 +npages / Drop contig −npages / MMIO 零动作）随 vmo.rs 取 overhaul 自动带回——**本件修正：vmo.rs 合并树取 ours blob（349a7e5f），五写点不在合并树**，回归义务与定义体复活同批落（见主发现二）。
- f5490947a（三轮遗漏清偿）对 ktest 的两口正身（pick_next_filtered+pick_next_if_above）与 f0_3/f0_4 面零触碰（diff 2 行注释），无新调和面。

## 主发现二：第十一连——deletion-wins 静默丢弃整链定谳（本件主发现）

机制（三层递进）：

1. **fceb↔ovh blob 逐字节同**：`fn test_f0_3`/`fn test_f0_4` 定义体 61 行/177 行在 fceb 与 overhaul 双线逐字节同（diff -q 空）；vmo.rs（cc312589）/mm.rs（b6860a5a）/kern_lock.rs（8f44a182）/tlbshoot.rs（80e14c94）四供体文件 blob 同值。
2. **base 线单方删**：13 号已定谳的活线合并对单文件取旧 blob 静默回退——f0_3/f0_4 定义+调用双零、vmo.rs contig/global 面（`VMO_OPT_CONTIGUOUS` 4→0）、syscall 0x1C 号位（`NR_VMO_CACHE_OP` 2→0）、kern_lock 双 guard（`klock_enter_irqsave`/`kern_acquire_irqsave_reentrant` 1→0）、tlbshoot `test_inline_consume_once` 1→0。
3. **git 三方合并 deletion-wins**：base 线删 + overhaul 未改（blob 同）→ 非冲突、静默取删侧。合并树 a44daaa0f 实测：ktest `fn test_f0_3/4` = 0、vmo.rs blob = ours（349a7e5f）、mm.rs blob = ours（3a900fe0）、kern_lock.rs blob = ours（f06d34d3）、tlbshoot.rs blob = ours（0761f536）；abi_gen.rs（Auto-merging 非冲突）`NR_VMO_CACHE_OP`/`VMO_OPT_CONTIGUOUS` 族 = 0。

与第十连（26 号「新增件自动活、回退件自动死」）对照补完判据：**fork 后新增件（f1_1/2/3、density）自动活；「fceb 有→ovh 未改→base 删」链的回退件自动死且无红**——第十一连把「回退件自动死」从「冲突段手动取侧」扩展到「非冲突文件自动并区」，静默性最档（无冲突标记、无编译红，五文件链全零）。

## 主发现三：供体核活四态表（解冲突后 build 前的复活清单）

f0_3/f0_4 定义体消费的全部符号在合并树的活性（git grep 合并树 a44daaa0f 实测）：

| 供体 | 定义位 | fceb | ovh | base tip | 合并树 | 判 |
|---|---|---|---|---|---|---|
| `klock_enter_irqsave`/`KlockIrqSaveGuard` | arch kern_lock.rs | 1 | 1 | 0 | **0（blob 取 ours）** | 贴回刀（20 号 fceb 形已验） |
| `kern_acquire_irqsave_reentrant`/`TableIrqSaveGuard` | arch kern_lock.rs | 1 | 1 | 0 | **0** | 贴回刀（同上） |
| `test_inline_consume_once` | arch tlbshoot.rs | 1 | 1 | 0 | **0（blob 取 ours）** | 贴回刀 |
| `allocated_pages` | kernel mm.rs | 1 | 1 | 0 | **0（blob 取 ours）** | 贴回刀（20 号三锁助手同批） |
| `commit_global`/`decommit_global`/`create_mmio` | kernel vmo.rs | 1 | 1 | 0 | **0（blob 取 ours）** | 贴回刀（contig/global 面整段） |
| `VMO_OPT_CONTIGUOUS`/`VMO_OPT_PINNED`/`VMO_CACHE_OP_*` | kernel abi_gen.rs | 1 | 1 | 0 | **0（Auto-merge 静默丢）** | 贴回刀（常量族整段） |
| `NR_VMO_CACHE_OP`（0x1C） | kernel abi_gen.rs + syscall reg 位 | 2 | 2 | 0 | **0** | 贴回刀（常量 + reg 位两处） |
| `test_f1_1`/`test_f1_2`/`test_f1_3` 定义 | ktest.rs 尾 | 0 | 1 | 0 | 1（自动活） | 零刀（fork 后新增件） |
| `test_density_ledger` 定义 | ktest.rs | 0 | 0 | 1 | 1（自动活） | 零刀（base 线新资产） |
| `kl_held` | arch kern_lock.rs | 1 | 1 | 4 | 4（活） | 零刀 |

- 复活义务链：解冲突后 build 前须贴回**五文件**（kern_lock 双 guard + tlbshoot 探针 + mm `allocated_pages` + vmo contig/global 面 + abi_gen 常量族含 syscall reg 位）+ ktest f0_3/f0_4 定义体 + 段 1 五调用——共七贴回刀。20 号/21 号已验隔离树贴回形（kern_lock +55 行、mm 三锁助手 +65 行、七刀增量补丁 EXIT=0），本链是其 ktest 面补完。
- E0425/E0432 预警：只贴 ktest 定义体不贴供体 = E0425（kern_lock 守卫/探针/vmo 方法组缺失）；只贴供体不贴 abi_gen 常量 = E0432（常量族缺失）——七刀必须同批，分批即红。
- 13 号「施工序第 1 步」修正：原表述「整文件取 overhaul：vmar.rs（10 段）→ 随带 aspace.rs/dpg.rs（各 2 横）+ vmo.rs/ktest.rs 的 F0-3/F0-4 定义体（自动并区）」——「自动并区」两处皆不成立，改写为「vmar.rs 十段整取 → aspace/dpg 各两段 → **五文件贴回链**（kern_lock/tlbshoot/mm/vmo/abi_gen）+ ktest 定义体与调用并集」；第 4 步 ktest 并集解不变。

## 主发现四：12 号 E2 重钉同批窗与 F0-4 消费面扩张

- 12 号（Q110E2 跨段用例重钉）的 ktest E2 重钉三笔与 f0_3/f0_4 复活同批窗：F0-4 定义体消费 `svc4/svc6` 直调 `NR_VMO_CREATE/COMMIT/DECOMMIT/GET_PHYS/CACHE_OP/HANDLE_*` 族——abi_gen 常量族贴回时 E2 重钉笔随批（12 号口径「ktest 因果差分扩 contig 相位」）。
- F0-4 消费面扩张登记：`svc6(NR_VMO_CACHE_OP, ...)` 六参形依赖 syscall reg 位贴回；`NR_VMO_GET_PHYS` 权利门（need READ，#210 判例）在 ours syscall 已收紧——f0_4 定义体调用 `svc4(NR_VMO_GET_PHYS, hv, 0, ...)` 用**完整权利句柄**（非 READ-only），#210 收紧面零冲突；READ-only 拒 INVALIDATE 断言（EACCES）与 #210 同向，无回归面。
- f0_3 消费的 `Vmo::create(PAGE_SIZE*2)`/`commit_global`/`decommit_global` 与 f0_4 的 `create_mmio`（VIRTIO_MMIO0_PA/VIRTIO_MMIO_LEN）均在 vmo.rs 贴回刀范围内；`PAGE_SIZE`（berry_mm::va）base 已活零刀。

## 施工卡（ktest.rs 面实施化，七刀一批）

- **刀① 段 1 并集解**：ours density 调用 + theirs 五调用并集，末位新号纪律排 `ktest done` 前；注释并集（机会五③ + F0/F1 注释各保）。
- **刀② ktest 定义体贴回**：`fn test_f0_3`（61 行）/`fn test_f0_4`（177 行）从 overhaul ktest.rs :12210/:12274 贴回（fceb≡ovh 逐字节同，20 号取形等价判据）。
- **刀③ 四供体文件贴回**：kern_lock.rs 双 guard（20 号 fceb 形 +55 行）；tlbshoot.rs `test_inline_consume_once` 探针；mm.rs `allocated_pages`（20 号三锁助手同批 +65 行）；vmo.rs contig/global 面（`VMO_OPT_CONTIGUOUS` 建账臂 + commit/decommit_global + create_mmio）。
- **刀④ abi_gen 常量族贴回**：`NR_VMO_CACHE_OP`=0x1C + `VMO_OPT_CONTIGUOUS/PINNED/DEMAND` + `VMO_CACHE_OP_CLEAN/INVALIDATE/CLEAN_INVALIDATE` + syscall.rs :90 use 位 + reg 位（SVC_CLASS_TABLE）。
- **刀⑤ 12 号 E2 重钉三笔随批**：contig 相位因果差分（12 号口径）。
- **刀⑥ 台账随刀行号重产**：ktest/vmo/mm/kern_lock/tlbshoot/abi_gen 六文件行号漂移。
- **刀⑦ 沿革让号**：Q7 契约让号律。
- 验证口径：`cargo build -p berry-kernel` 0 error（七刀同批后）；对照红 = 仪贴刀①+②（E0425 kern_lock/vmo 面）→ +③（E0432 abi_gen 面）逐级兑现；QEMU smoke ktest F0-3/F0-4 自检面随全量回归。

## 与其他文档的关系

- 施工单源：[13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md](13-ktest调用序列与九文件回流对拍总账-回流义务5补遗.md)——「ktest.rs 段 1 并集解」裁定的实施化；本件修正其「自动并区带回」两处表述并补 deletion-wins 整链定谳，裁定本体（并集解）不动。
- 供体贴回形单源：[20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md](20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md)（kern_lock +55/mm +65 隔离树已验形）；[21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md](21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红定谳.md)（七刀增量补丁 EXIT=0 形）。
- 判据链：[24-aspace.rs两段冲突处置与消费改接面diff草案-第九连供体链生死分叉定谳.md](24-aspace.rs两段冲突处置与消费改接面diff草案-第九连供体链生死分叉定谳.md)（第九连「回退件自动死」）→ [26-channel.rs五段冲突处置与死链消费面diff草案-第十连channel面供体全活定谳.md](26-channel.rs五段冲突处置与死链消费面diff草案-第十连channel面供体全活定谳.md)（第十连「全活零刀」+ 消费侧对偶）→ 本件（第十一连：deletion-wins 非冲突文件静默丢弃，静默性最档）。
- E2 重钉：[12-Q110E2跨段用例重钉形态预备-回流义务4深化-区间树批量形对拍审计.md](12-Q110E2跨段用例重钉形态预备-回流义务4深化-区间树批量形对拍审计.md)（同批窗口径单源）。
- 证据可复现：merge-tree = `git merge-tree --write-tree --merge-base=fceb03ff3 origin/wip/sched-1.1 origin/wip/base-overhaul`（树 `a44daaa0f`）；段位 = 三方 `git show ...:kernel/src/ktest.rs` + `git merge-file -p`（`<<<<<<<` ×1 @:177-194）；blob 同值 = `git rev-parse <ref>:<file>` 四文件对照；deletion-wins 机制 = fceb↔ovh 定义体 `diff -q` 空 + base/合并树双零。
- 沿革：[docs/阶段沿革.md](../../docs/阶段沿革.md) 六百三十六节（随本批）。
