# 内核深度进化 07 号：vslotface→vmarface 归并裁定预备——三查判序两形恒等性对拍审计

Type: research
Status: done
Updated: 2026-11-19

一句话说明：Q112（05 号件）回流义务 2 预判 vslotface 的 mapgate/Q17 三查面「预期退役，判定律语义由 vmarface 承接」——本件以两线实测把该预判落为可执行裁定：vslotface 与 vmarface 在两线零 diff（lib.rs/droot.rs/slot_tests.rs 逐字节同文），判序三查（`Unaligned → OutOfBounds → Overlap`）在表驱动形（`check_slot_of` O(N) 全扫）与区间树形（`VmarTree::check_slot` O(log N)）**200,000 样本全域扫描零分歧**，`aligned_nonzero` 两线逐字节同文；唯一语义增量 = vmarface `bounds_end` 增加 `base.checked_add(span)` 回卷防御——**裁定：回流时 vslotface lib.rs 判定律面退役、droot 面保留、退役面 host 钉板测试随语义宿主迁移 vmarface**。

## 审计基线（2026-11-19 20:1x 直核）

- base 线 = `origin/wip/sched-1.1@92ce8a123`：vslotface = 221 行 lib.rs + 140 行 droot.rs + 233 行 slot_tests.rs；kernel/src/vmar.rs 消费 18 处 `berry_vslotface` 引用（三查 inline 装配 :609-619 + `find_exact_at`/`find_mapped_at`/`demand_hit_at` 三查找 :632/:639/:512 + droot 全套）。
- overhaul 线 = `origin/wip/base-overhaul@492d5e00c`：vslotface 三文件与 base 零 diff（`git diff` 逐字节实证）；kernel/src/vmar.rs 仅存 8 处引用且**全部为 droot 面**（`use berry_vslotface::droot` + re-export 两处 + `RootSlots` impl）；vmarface 747 行新增（F1-1），自带 `aligned_nonzero`/`bounds_end`/`check_slot` O(log N) 三查。
- 零消费实锤：`git grep vslotface:: -- '*.rs'`（排除 droot 与 crate 自身）在 overhaul 全树零命中——lib.rs 12 导出（`aligned_nonzero`/`bounds_end`/`overlaps`/`check_slot_of`/`check_slot_const`/`find_exact_at`/`find_mapped_at`/`demand_hit_at`/`snapshot_kind_of` + `SlotKind`/`SlotRegion`/`SlotError`）在区间树形态下无宿主。

## 判序恒等性对拍（本件核心证据）

对拍工程（host 侧独立 bin，vslotface path 依赖 + vmarface lib 全文引入）：空表与灌入五区间（互不交叠种子 [b, b+0x2_0000) 等）两轮，对 `[b−1 MiB, b+span+1 MiB)` 全域按 4 KiB 步进 × 长度四档（0/4 KiB/128 KiB/16 MiB）× 偏移五档（0/1/0xfff/0x1000/0x8000）× 尾长三档（0/1/0x2000）扫描，共 200,000 样本，两形结论（Ok/Err 变体）全同——`total=200000 agree=200000 disagree=0`。

| 谓词 | vslotface（表驱动形） | vmarface（区间树形） | 定谳 |
|---|---|---|---|
| 对齐律 `aligned_nonzero` | `vaddr % PAGE_SIZE == 0 && len % PAGE_SIZE == 0 && len != 0` | 同文逐字节 | 恒等，可退役 |
| 管辖界 `bounds_end` | `checked_add(len)` 防回卷；`base + span` 裸加 | 同形 + `base.checked_add(span)` 回卷防御 | 语义增强（vmarface 严格域更宽），退役安全 |
| 重叠律 `overlaps` | `vaddr < r_start + r_len && r_start < end`（全表扫） | `spans.range(..end).next_back()` 邻域探（严格不交序） | 判定恒等（200k 样本），复杂度 O(N)→O(log N) |
| 三查判序 | `check_slot_of`：Unaligned → OutOfBounds → Overlap | `check_slot`：同判序（InvalidVmar 前置） | 判序恒等，`SlotError` 三变体一一对应 |

## 归并裁定（回流落地时执行）

1. **退役面**：vslotface lib.rs 判定律 12 导出（三查/查找/投影族）——区间树形态下零宿主，回流时整面退役不重放。
2. **保留面**：vslotface droot.rs（DEMAND_ROOTS 判序）——overhaul kernel/src/vmar.rs 8 处消费全部为 droot 面，零改动随并流保留。
3. **测试迁移义务**：slot_tests.rs 233 行 host 钉板（三查判序/边界/错误码优先级）语义宿主变 vmarface——按 06 号件判例迁移为 `VmarTree::check_slot` 对拍形（表形种子灌树 + 逐断言改树形调用），迁移后 vslotface 仅存 droot 测试。
4. **错误码契约**：`SlotError::{Unaligned,OutOfBounds,Overlap}` 与 `VmarTreeError` 同名三变体一一对应；`VmarTreeError::{NotMapped,InvalidVmar}` 为区间树新增态（find/destroy 面宿主在 vmarface），表形无此二态不迁移。
5. **入册义务**：vmarface 入册按 05 号义务 6 全量对账（members/白名单/justfile/unsafe 台账，#171 判例族）。
6. **执行时点**：回流合并时随刀执行（与 05 号义务 5 syscall.rs 逐段重放同批）；本件不预先改 base 线代码——预先退役会破坏 base 线 18 处消费。

## 与其他文档的关系

- 义务来源（回流总图）：[05-base-overhaul回流接缝预研-W3面资产对撞审计.md](05-base-overhaul回流接缝预研-W3面资产对撞审计.md)——本件把其义务清单第 2 条的「预期退役」落为实测裁定。
- 姊妹件（义务 3 深化）：[06-recv_n重放形态预研-ChannelShared接缝审计.md](06-recv_n重放形态预研-ChannelShared接缝审计.md)。
- 判定律单源：`vslotface/src/lib.rs`（base 与 overhaul 零 diff 的实证对象）；`vmarface/src/lib.rs`（overhaul 线 `git show origin/wip/base-overhaul:vmarface/src/lib.rs` 747 行）。
- 对拍工程证据：host 对拍 bin（vslotface path 依赖 + vmarface lib 引入）200,000 样本 `agree=200000 disagree=0`（2026-11-19 20:1x 实跑，脚本随 W3 工作区 scratch 存档）。
- 沿革：`docs/阶段沿革.md` 五百九十七节（随本批）。
