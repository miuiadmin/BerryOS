# AGENTS.md — .agents/notes 决策记录树

Agent Note 是 Agent 写的决策记录：沉淀代码与文档装不下的「为什么」与「放弃了什么」。操作细则见 [README.md](README.md)，格式权威在[规范全文](../README.md)的「笔记」节。

- **动手重构或选型前先搜笔记**——含 `rejected/`，可能是被否决过的方案：

```bash
rg --hidden -g '!.agents/notes/archived/**' <关键词> .agents/notes/
```

- **每篇新笔记先搜再写**：相关旧笔记逐篇分类（无关 / 重叠互链 / 吸收合并 / 过时提案转 rejected），结论同批落盘。
- **只写决策级内容**：行为、架构、契约、流程、格式的取舍与后果；机械改动与执行流水账不写。
- **禁建全局 `INDEX.md`**；检索用上面的 rg 命令。
- 改完可跑 `node .agents/tools/check-docs.mjs` 自检（可选）。
- `archived/` 是冻结历史快照：永不修改，不作为当前权威。
