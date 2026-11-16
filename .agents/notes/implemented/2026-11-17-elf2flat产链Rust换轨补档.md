# Agent Note: elf2flat 产链 Rust 换轨补档（彻底 Rust 化 b 层首件出列——迟到笔记）

Status: implemented

## Problem

彻底 Rust 化计划（docs/彻底Rust化计划-v1.0.md §4）把 elf2flat/elf2pe 列为「独立，随时」件。跨根收口窗三刀已落：b598e5bbf（PR #393）elf2flat Rust 件首刀（钉板五步 2/3/4）、8d8a2f3ed 产链换轨二刀（build-user 六处 python3→Rust bin，Python 降格 oracle 改名 elf2flat_oracle.py——五步 5/5 收官）、2a342d813 elf2pe Rust 件首刀（同轨第三刀，产链换轨明示挂二刀——justfile Python 件现役不动）。该刀族的「为什么」（钉板五步首走验证、oracle 退役法、独立件先于主刀序）只活在提交信息里，.agents/notes/ 零承载，且计划文档 §2 清单未回填——2026-11-17 第二轮遗漏审计抓获，本笔记补档 + 计划同批翻注。

## Decision

五步对照（计划 §5 钉板法首走）：①oracle 降格改名（elf2flat_oracle.py，入豁免区）②Rust 件落 tools/（同名 CLI 形态）③字节级对拍（同输入同字节纪律）④selftest 全绿 ⑤just 产线换轨（build-user 六处）——elf2flat 五步全闭；elf2pe 走到②-④，⑤挂二刀。主刀序 §4（mkcpio 起的单编码器 import 链）未动——独立件先出列不破坏保序约束。

## Consequences

- b 层现役 .py 计 11→10（elf2flat 出列）；§6 验收线（计数 = 0）随末件生效。
- 计划文档 §2 表两行回填 + 状态行翻「实施中」；雷册 §A48 平像抽取指称同步（elf2flat.py → Rust 件 + oracle 改名注）。

## Related

- [彻底 Rust 化计划](../../../docs/彻底Rust化计划-v1.0.md) §2/§4-§6
- [雷册-判例库](../../../docs/雷册-判例库.md) §A48
