# Curia — 智能体工作区规则

本项目由 Curia 管理：过程与过程产物文档收在 `.agents/`，外部事实、现行标准与知识收在 `docs/`。

**放东西之前先读总规格：[`.agents/README.md`](./.agents/README.md)。**

## 通用规则

1. 一事实一归属：同一条事实只有一个家；别处引用用相对链接，不复制、不双写。
2. 先定家再落笔（四句口诀）：没做的进 `plans/`、做完的进 `research/`、为什么进 `notes/`、过时的翻状态或归档；可复用的干法进 `skills/`，外部事实进 `docs/`；现行标准进 `docs/`（`Type: standard`，带 `Version:` 与 `Status:`）。模块的总体设计固定为 `plans/<模块>/design.md`（`Type: spec`，带 `Version:`），要改模块先看那份。
3. 非平凡变更同批写笔记：改变行为、结构、契约、流程、工具或格式的改动，同一批变更里新增或更新一篇 `notes/` 笔记；写前先搜（含 `rejected/`）。
4. 永不删除：过时文档优先翻状态；翻状态就是取代动作——同批补 `Superseded-by:` 把链指到现行版本，并把引用旧文的现行文档一并改指；冷历史走归档四步——移动、`Archived:` 行、重写自身相对链接、修入链。
5. 引用一律相对 Markdown 链接，目标必须存在；禁全局 `INDEX.md`，检索用全文检索工具（`rg` 或等价）。
6. 写作格式：一段一物理行；中英之间加一个半角空格；中文全角标点；禁 emoji；只写当前状态。
7. 并行会话纪律：动手前 `git status`；只精确暂存自己的路径（禁 `git add -A` / `git add .`）；提交带显式路径（`git commit -- <自己的路径…>`）；推送前 `git log origin/main..HEAD` 确认推送范围；别人的未跟踪与已改文件一律不碰。
8. 改动文档后跑检查：`node .agents/tools/check-docs.mjs`（读工作树，无需先 `git add`）；工作区体检与标准候选分别用 `--health`、`--suggest-standards`——两者只读诊断、不改文件、正常退出 0；提交把关用 `--strict`，可自挂钩子；装不装钩子归项目自定，本条不是提交硬门槛。
9. 用前先解析状态：检索命中不等于现行——把一份文档当依据前，先看标题后有没有 `Type:`；有 `Type:` 的读头块（`docs/` 标准还有 `Version:`），`Status: superseded` 的按 `Superseded-by:` 追到终点再读，`historical` 只作存量/历史参考，`Archived:` 的按冷历史处理；没有 `Type:` 的 `docs/` 自由知识当前文件即真相。旧文可以读，旧文的内容不能当现行规则执行。

## 桥

`CLAUDE.md`、`.claude/skills`、`.codex/skills` 是指向 `AGENTS.md` / `.agents/skills/` 的符号链接（不是副本）；Claude Code、Codex 等 CLI 从这些入口读取本规则与技能。桥缺失时（被占用的路径先备份移开再重建），命令是：

```bash
mkdir -p .claude .codex
ln -s AGENTS.md CLAUDE.md
ln -s ../.agents/skills .claude/skills
ln -s ../.agents/skills .codex/skills
```

## 结构速览

```text
<你的项目>/
├── AGENTS.md           # 本文件；CLAUDE.md 是其符号链接
├── .agents/            # 过程与过程产物（规范全文见其 README）
│   ├── plans/          # 没做的
│   ├── research/       # 做完的
│   ├── notes/          # 为什么
│   ├── skills/         # 可复用的干法
│   ├── archive/        # 过时的（封存，永不删除）
│   └── tools/          # check-docs.mjs（轻量自检）
└── docs/               # 外部事实、现行标准与知识（standard 带 Version）
```

子目录如需更细规则，放该目录的 `AGENTS.md`（只能追加约束，不得放宽本文件）。
