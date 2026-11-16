# berry-ui Select 数字带存量红修复（波2批C 生而红对齐钉板）

Status: implemented
Type: note
Date: 2026-11-15

为什么：波3a 验收（W3A-VERIFY.md 步骤②）全量跑 berry-ui host 测试发现 `tests/widgets4.rs` 两测红（`select_layout_and_paint_golden`/`select_click_cycles_and_set_props`），隔离 target dir 复跑同红（排除 ~/.cargo-target 共享投毒形态），在出生提交 b9788e248 的独立 worktree 上同红——**生而即红**，非波3a 三路（SA/SB/SC 零碰 berry-ui，`git diff b1646469d HEAD -- user/berry-ui/` 为空）的回归。

根因：波2批C 的 Select 渲染把选中索引数字传整框区带给 `emit_widget_text`（区带内**水平居中**共核——Button 标签同款），1 位数字落 x = 4+(180−8)/2 = 90 悬于框心；而同批落下的 widgets4 钉板断言 (b'2', 6, 16)——测试与实现出生即矛盾，批C 提交时未真跑集成测试面（后续批D/E/件⑤a 的「N 测零回归」口径亦未覆盖此红）。

裁决：**测试是契约方**——钉板几何（左端 12px 数字带：4+(12−8)/2 = 6）符合下拉选择器惯例（值左置、v 箭头右端），实现传整框区带是误用居中共核的笔误。修法 = paint.rs Select 臂区带参数 `b.w - 20` → `12`（一行，emit_widget_text/钉板零改动）；多位数字区带溢出时左锚 x=4（`(zone−text).max(0)/2` 语义天然），「99+」至 28px 仍在值区。修后 berry-ui 全量 243 绿零失败。

顺带记录：波2批C/D/E 提交信息所引「沿革三百四十三/四十四/四十五」在 docs/阶段沿革.md 无对应条目（号只进提交信息未进文档）——文档缺口挂账，不在本刀补写。（更正 2026-11-17：该挂账已被其后的沿革大重写取代——343/344/345 号现由 W3-Q39 heap 分配（:5633）与 W2 布局树两件（:6451/:6465）持有，号位有主，回补旧文口径不再成立。）
