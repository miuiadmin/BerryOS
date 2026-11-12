# docs/ — 知识、契约与外部事实

本目录收录三类内容：仓外系统的登记与定义、面向所有读者的现行标准、以及面向人的说明与附件。**自由知识当前版即唯一真相**——改了直接改，历史在 git；**现行标准**用 `Type: standard`，带 `Status:` 与 `Version:`，过时按取代链退役。

## 两种文档

标题后的头块里没有 `Type:` 的是自由知识：外部事实、接入说明、架构导览、FAQ、演示手册。它没有状态机，当前文件就是现行版。

标题后的头块里有 `Type: standard` 的是现行标准：设计规范、UI 设计系统、API/数据契约、编码约定、写作与排版约定——需要回答「现行第几版」的文档。格式与生命周期见[规范全文](../.agents/README.md)的「docs 标准」节。

```markdown
# <主题>（v<MAJOR.MINOR>）

Type: standard
Status: current
Updated: yyyy-mm-dd
Version: 1.2.0
```

`Status` 取 `current`（现行）、`historical`（仅保留给存量或参考）、`superseded`（被取代，必须带 `Superseded-by:` 相对链接）。`Version` 是这份标准自己的契约版本：MAJOR 不兼容、MINOR 兼容新增、PATCH 澄清改字。

## 现有标准升格

已有自由知识要升成标准时，先跑只读候选清单：

```bash
node .agents/tools/check-docs.mjs --suggest-standards
```

它按文件名与头块线索列出候选并给出建议头块，**不写文件**。是否升格由人裁决：确认它需要回答「现行第几版」后，手工补 `Type: standard`、`Status:`、`Updated:`、`Version:` 四行；同主题只留一个 `current` 载体，旧的翻 `historical` 或 `superseded`。不批量转换。

工作区体检用 `--health`：只读报告载荷、桥、`AGENTS.md` 合并态与 docs 标准/候选计数，正常退出 0，不改文件。

## 放什么，不放什么

放这里：外部系统登记、字段映射、端点、协议；现行设计标准与契约（含写作与排版约定）；面向人的使用说明、接入指南、FAQ；图片、图表、HTML 原型、PDF、JSON schema 等附件。

不放这里：计划、提案、路线图 → [`.agents/plans/`](../.agents/plans/)；调研、证据、复盘 → [`.agents/research/`](../.agents/research/)；决策因果、ADR → [`.agents/notes/`](../.agents/notes/)；可复用流程、runbook、skill → [`.agents/skills/`](../.agents/skills/)；模块总体设计 → `.agents/plans/<模块>/design.md`。

## 命名

自由知识与标准都用主题名，不加日期前缀——身份是主题，不是落笔时点；主题段用 kebab-case 或纯中文名。

标准可以带版本后缀，如 `设计规范v1.2.md`、`design-standard-v2.md`；文件名里的版本是人读标签，头块 `Version:` 是权威；文件名版本必须是头块版本的前缀（`v1.2` 对 `1.2.0`）。升 MINOR/PATCH 时不必改文件名，只改头块。

`design.md` 是模块设计文档的固定名，不要用来命名标准。

## 生命周期与归档

标准默认原地改、`Version` 递增、`Updated:` 改今天，历史在 git；只有大版本线必须共存、主题被拆分/改名/整体重做时，才新建一份并让旧的翻 `superseded` 或 `historical`。

冷历史可移入可选的 `docs/archive/`（如果创建）：带类型头块的标准加 `Archived: yyyy-mm-dd` 行，自由知识原样移入。永不删除。只有顶层的 `docs/archive/` 享受归档机器豁免；其他名字或嵌套的 `archive/`（如 `docs/<主题>/archive/`）按普通主题目录处理，不强制迁移。

## 检索与附件

目录即索引，不建 `INDEX.md`；检索配方见[规范全文](../.agents/README.md)的「链接与检索」：

```bash
# 列出标准的状态与版本
rg -l '^Type: standard$' docs/ | while read -r f; do
  printf '%s\t' "$f"; head -12 "$f" | awk '/^(Status|Version|Updated):/{printf "%s ", $0} END{print ""}'
done
```

附件与被机器消费的文件不参与生命周期，不要求 Markdown 头块；检查器只扫 `.md`。

引用一律相对 Markdown 链接；指向 `.agents/` 时写 `../.agents/…`。
