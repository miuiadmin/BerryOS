# notes/ — 决策因果（Agent Note）

本目录存 Agent Note（决策笔记）：代码与文档装不下的「为什么」与「放弃了什么」。格式权威在[规范全文](../README.md)的「笔记」节；本页是操作细则。

## 何时写

非平凡变更同批写：改变行为、结构、契约、流程、工具或格式的改动，同一批变更里新增或更新一篇笔记（规则见根 `AGENTS.md` 的 Curia 通用规则集第 3 条；采用时若目标仓已有自己的 `AGENTS.md`，`init` 会保留它而不覆盖，那份规则集需自行并入）。纯机械改动、执行流水账不写。

## 头块与节

两行头块，不带 `Type:` / `Updated:`：

```markdown
# Agent Note: <标题>

Status: <proposed | implemented | rejected — 一行原因>
```

- `proposed`：尚未实现；正文节 Problem / Proposal / Alternatives considered / Consequences。
- `implemented`：已落地；Proposal 换 Decision，用现在时描述现实，与落地保持同步。
- `rejected`：考虑过并否决；`—` 后的一行原因非空；只在否决理由仍能阻止错误时保留。

四节是默认（节名中英均可），可加 Related；「备选方案」只记真实考虑过的选项与落选原因，禁编造。

## 落位与命名

`notes/{proposed|implemented|rejected}/[<category>/]yyyy-mm-dd-topic-title.md`——日期取首次落笔日；允许品类子目录（如 `architecture/`）。归档笔记扁平放 `archived/`。

## 纪律

- **先搜再写**——含 `rejected/`（可能已被否决过）：

```bash
rg --hidden -g '!.agents/notes/archived/**' <关键词> .agents/notes/
```

- **决定翻转不改写旧篇**：新开一篇、两篇互链；旧篇保持原样。
- **永不删除**：过时优先翻状态（未实施的提案过时转 `rejected`）；冷历史走[归档四步](../archive/README.md)移入 `archived/`。
- 引用一律相对 Markdown 链接；不建 `INDEX.md`。
- 改完可跑 `node .agents/tools/check-docs.mjs` 自检（可选）。
