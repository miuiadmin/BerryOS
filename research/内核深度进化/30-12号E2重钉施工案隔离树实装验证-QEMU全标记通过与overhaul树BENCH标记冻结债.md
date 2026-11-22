# 内核深度进化 30 号：12 号 E2 重钉施工案隔离树实装验证——QEMU 真机全标记通过 + overhaul 树 BENCH 标记冻结债新发现（30 号件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：12 号件把 Q110 E2 跨段用例重钉落为五点施工案（基础三面钉逐字同文 + A/B/C 三增量钉板）但只到纸面——本件在 overhaul 隔离树（492d5e00c）实装验证全链闭合：**贴 94 行补丁（插入锚 = test_vmo 子 Vmar 别名断言 :3310 后、A1-2 容量冒烟段前，与 12 号锚点裁定零漂）→ cargo build 0 error → QEMU smoke 真机跑通 E2 全断言面（VMO ok / ktest done 到齐，expected D-abort 恢复链正常）**；并带出两个新定谳：①**overhaul 树自带 BENCH 标记冻结债**——bench 本体 n=10（比 base 多 send_recv_pair/svc_roundtrip 两项）而 markers.sh 断言 `BENCH ok n=8`（base 线冻结值），overhaul 树 smoke 必 FAIL（基线无补丁对照跑同 FAIL 实证非本补丁回归）——回流施工单补登「markers.sh 滚新 n=8→n=10」义务刀；②**增量钉板 A 的断言形态修正**——12 号案文写「三页 VMO 仅 commit 页 0/1」但实施取「只 commit 页 0」（两段 commit 语义更纯：预检面在第 3 页才命中，钉的是「先全量预检后填叶」不变量本身而非部分覆盖），补齐 commit 走 `commit(PAGE_SIZE, PAGE_SIZE*2)` 连续区间臂。

## 审计基线（2026-11-20 11:0x 直核）

- base 线 = `origin/wip/sched-1.1@ffb6f0af1`（#599 合流后）；overhaul = `origin/wip/base-overhaul@492d5e00c`（未前进）；merge-base = `fceb03ff3`。
- 隔离 worktree = `scratch/w3q30-ovh`（detached @492d5e00c）；基线构建链：`just build-user`（userbins 产物前置）→ `cargo build -p berry-kernel --target aarch64-unknown-none` EXIT=0（0 error，9 警告全既有）。
- 27 号 deletion-wins 定谳随 base 前进复核：五文件四问核（ktest `fn test_f0_3/4`、mm `allocated_pages`、vmo contig/global、abi_gen `NR_VMO_CACHE_OP`）在当前 merge-tree 330291ef9 仍全零——十一连结论零漂。

## 实装验证链（20/21 号先例同款）

1. **锚点核**：overhaul ktest.rs :3310 = 子 Vmar 别名断言（`VMO：子映射经共享表根可读`）；:3313 起 = A1-2 容量冒烟注释头——12 号施工锚点裁定（base :3307 同位）零漂。
2. **作用域核**：插入点前 `buf`/`MAGIC2`/`root`/`fa`/`PAGE_SIZE`/`Vmo`/`VmarError`/`Perm` 全在作用域（test_vmo 头部 `use crate::vmar::{Perm, Vmar, VmarError}` + 局部绑定实测）；`hal::trap::arm_expected_fault`/`fault_armed` 在 overhaul trap.rs 存活（pub fn ×2，ktest 消费 8 处）。
3. **贴补丁**：python 单锚插入 94 行（唯一锚断言 `src.count(anchor)==1`）——E2 基础三面钉（map 读写×3 / protect→RO 写 fault×3 + 提回 Rw 写通 / unmap 后读 fault×3，断言文案与 base E2 逐字同文仅注释改署新形态）+ 增量钉板 C（protect 仅中页降 Ro：中页写 fault、首尾写通、整区间复原）+ 增量钉板 B（unmap 后同区间重 map 必 Ok = 零树残留反向钉板，再 unmap 收净）+ 增量钉板 A（只 commit 页 0 → 整区间 map 必 `Err(VmarError::VmoNotCommitted)`；补 commit 页 1/2 后重 map 必成；unmap 收净 + drop）。
4. **构建门**：`cargo build -p berry-kernel --target aarch64-unknown-none` EXIT=0，0 error；E2 段零新警告（ktest 警告全既有行号 :7085/:7646/:7662/:10895/:11189/:12936，无 :3310-3451 段）。
5. **QEMU 真机门**：`bash tools/smoke.sh`——E2 全断言面通过：`VMO ok`（:23）、`ktest done`（:120）到齐；expected D-abort 恢复链正常（VMO ok 前 10 连 expected D-abort，含 E2 的 RO 写 fault×3 + unmap 读 fault×3 + 切分中页 fault）；boot 32 标记唯一缺 `BENCH ok n=8`——**见主发现一，系 overhaul 树冻结债非本补丁回归**。
6. **对照基线**：stash 补丁后无补丁重跑 smoke 同样 FAIL 于 `BENCH ok n=8`，且基线打出 `BENCH ok n=8`、E2 版打出 `BENCH ok n=10`——两跑对照定谳：overhaul bench 本体 10 项（BENCH 行序：cntvct_pair/frame_alloc4k/frame_alloc64k/vmo_commit1p/vmo_rw4k/handle_evt/pick_scan/**send_recv_pair/svc_roundtrip** 两多出项/BENCHFA 族），markers.sh 断言 n=8 是 base 线冻结值。

## 主发现一：overhaul 树 BENCH 标记冻结债（回流施工单补刀）

- markers.sh MARKERS 断言 `"BENCH ok n=8"`；overhaul 树 bench 实发 `BENCH ok n=10`。smoke 脚本 grep 差集必挂 `缺：BENCH ok n=8`。
- 定性：**回流合并时的 markers.sh 滚新义务**（n=8→n=10），与 26 号「死链消费面须全文件对账」同族——回流施工单收尾面（29 号刀④ 07 口径滚新）旁补一刀：markers.sh BENCH 标记同步。不属编译面（无红），属标记冻结面（CI smoke 红）。
- 本件处置：隔离树验证只在树内跑通（不推 base——markers.sh 属 base 线冻结资产，回流时随 bench 本体一起滚新）；登记进施工单义务清单。

## 主发现二：增量钉板 A 断言形态修正（12 号案文微调）

- 12 号案文：「三页 VMO 仅 commit 页 0/1，map 必 Err」——实施取「只 commit 页 0」再补「commit(PAGE_SIZE, PAGE_SIZE*2)」两段式。理由：预检循环 `for i in 0..npages` 在第 3 页（i=2）命中 None 才拒——只 commit 页 0 时拒收点最深（穿透两页检查），钉的是「先全量预检后填叶」不变量本身；且补齐臂走连续区间 commit（`PAGE_SIZE..3*PAGE_SIZE`）恰好覆盖跨段 commit 的连续区间臂语义。断言强度不降反升。
- 补丁已按实施形态落（见证据可复现），12 号案文与实施的差异在本件定谳：以实施形为准。

## 与其他文档的关系

- 施工案单源：[12-Q110E2跨段用例重钉形态预备-回流义务4深化-区间树批量形对拍审计.md](12-Q110E2跨段用例重钉形态预备-回流义务4深化-区间树批量形对拍审计.md)——本件 = 其五点施工案的实施验证闭合（锚点/作用域/断言面/构建/真机五环全过）；A 钉板形态微调以本件实施形为准。
- 先例链：[20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md](20-kernLock双guard与mm三锁助手复活实装验证-第1步随刀序12实跑定谳.md)、[21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md](21-F0-1F0-2供体复活第0步实装验证-隔离树编译四连红全消定谳.md)——隔离树实装验证纪律（干净树 + 唯一锚插入 + 构建/真机双门）同源。
- 收尾面总账：[29-user.rs段1并集解与回流收尾面总账diff草案-十文件逐段落刀链收官.md](29-user.rs段1并集解与回流收尾面总账diff草案-十文件逐段落刀链收官.md)——其施工卡刀④（07 口径滚新）旁的 markers.sh 滚新义务由本件主发现一补登。
- 证据可复现：隔离树 = `git worktree add --detach <dir> origin/wip/base-overhaul`；补丁 = 锚 `assert_eq!(v, MAGIC, "VMO：子映射经共享表根可读（同 PA 双 VA 别名）");` 后插 94 行（本件 diff 草案随 PR 附 scratch 导出 `e2-repin.patch` 105 行）；构建 = `just build-user && cargo build -p berry-kernel --target aarch64-unknown-none`；真机 = `bash tools/smoke.sh`（串口日志两份对照：补丁版 berryos-smoke.i8wt9q / 基线 berryos-smoke.FCB74u）；overhaul bench 10 项清单 = 串口 BENCH 行序逐行。
- 沿革：[docs/阶段沿革.md](../../docs/阶段沿革.md) 六百四十二节（随本批——base 现占至六百四十一，W2-Q253 台账代偿重产刀实占）。
