# .claude 配置面登记（MANIFEST）

本文件是 `.claude/` 跟踪面的唯一登记处；`python3 tools/claude-audit.py` 五向对拍：
①本清单行集 ②`git ls-files .claude` ③磁盘实存（除运行时面）④settings.json 引用的
命令文件存在 ⑤settings.json hook 事件覆盖面与本清单描述一致。配方源：2026-11-22
深研（[survey](../.agents/research/2026-11-22-survey-impossible-os自动化开发深研.md)
差距表 #8——配置面漂移是静默的，hook 改了没人知道）。

立法（default-conservative）：**新跟踪文件未登记=红**；登记悬空（列了但 git 不跟踪
/磁盘不在）=红；hook 事件与描述不一致=红。运行时面（`state/`、`commands/`、
`goals/`、`worktrees/`、`scheduled_tasks.lock`）是 `.gitignore` 排除的非登记面，
不入对拍。改 `settings.json` 的同批提交必须过 `python3 tools/claude-audit.py`
（经 [control-plane 清单](../tools/control-plane.txt) 前缀级测试接线）。

登记行格式（机器解析面，审计件依赖）：`- `路径` — 用途 [hooks: 事件,事件]`
（`[hooks: ...]` 标记仅 settings.json 行需要——审计对拍 ⑤ 用它对照实际事件集）。

- `.claude/settings.json` — E lane hooks 接线：Stop 扣停守卫（e-guard stop-hook：无人会话无 fixpoint 哨兵即 exit 2）+ PreToolUse FROZEN 封条（e-guard frozen-guard：无人会话禁写宪法/闸面）[hooks: PreToolUse,Stop]
- `.claude/skills` — 符号链接 → `../.agents/skills`（技能桥；本体与规则在 `.agents/skills/`，桥缺失的重建命令见 AGENTS.md「桥」节）
- `.claude/MANIFEST.md` — 本登记清单（自登记：清单自己是跟踪面的一员，防「登记面自身漂移无主」）
