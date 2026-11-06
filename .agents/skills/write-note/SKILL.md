---
name: write-note
description: 在 .agents/notes/ 新增或更新决策记录（Agent Note）。当完成非平凡变更、做出架构或流程决策、要记录「为什么这么做」时使用；用户说「记一笔 / 写笔记 / 记录决策」也触发。
---

# 写决策笔记（Agent Note）

细则与目录约定见 [.agents/notes/README.md](../../notes/README.md)；格式权威在[规范全文](../../README.md)的「笔记」节。

## 五步

1. **先搜**——含 `rejected/`（可能已被否决过）：

```bash
rg --hidden -g '!.agents/notes/archived/**' <关键词> .agents/notes/
```

2. **选家落位**：`notes/{proposed|implemented|rejected}/[<category>/]yyyy-mm-dd-topic-title.md`——`proposed` 尚未实现、`implemented` 已落地、`rejected` 已否决；日期取首次落笔日。
3. **套格式**：两行头块（标题 + `Status:`），四节默认：Problem / Decision 或 Proposal / Alternatives considered / Consequences，可加 Related；「备选方案」只记真实考虑过的，禁编造。
4. **supersession 检查**：相关旧笔记逐篇分类（无关 / 重叠互链 / 吸收合并 / 过时提案转 rejected），结论同批落盘。
5. **自检（可选）**：`node .agents/tools/check-docs.mjs`；检查器只核头块形态与链接，内容质量靠审校。

## 禁止

- 禁止把旧笔记改写成反面（决定翻转 = 新开一篇 + 互链）；禁止编造 alternatives。
- 禁止给笔记加 `Type:` / `Updated:` 头（`Status:` 紧随标题，取值与目录一致）；禁止建 `INDEX.md`。
- 禁止删除：过时提案转 `rejected`；冷历史走[归档四步](../../archive/README.md)移入 `archived/`。
