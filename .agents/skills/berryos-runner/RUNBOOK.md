# berryos-runner 接入手册（Hermes 侧）

Type: skill
Updated: 2026-11-22

SKILL.md 是 runner 宪法（给机器读）；本手册是 owner 接入/排查面（给人读）。宪法细节别在这重复——一事实一归属。

## 现有 fleet 盘点（2026-11-22 实测，`~/.hermes/cron/jobs.json`）

| 任务 | 节拍 | 工作区 | 角色 |
|---|---|---|---|
| `berryos-hermes-w1` | 10m | `/Users/murongcheng/berryos-hermes` | tools/test/CI lane（693+ 轮；宪法 = 该克隆 `.hermes-loop/LOOP.md`） |
| `berryos-hermes-w2` | 10m | `/Users/murongcheng/berryos-w2` | 文档/沿革 lane |
| `berryos-hermes-w3` | 10m | `/Users/murongcheng/berryos-w3` | 第三工作 lane |
| `berryos-hermes-review` | 3m | `/Users/murongcheng/berryos-review` | 评审 lane |

四 lane 全部：model glm-5.3、provider newapi、PR label `hermes-auto`、base `wip/sched-1.1`、绝不 merge。W1 的 prompt 是「单指针 + 摘要」形态：`先完整读取 <克隆>/.hermes-loop/LOOP.md，严格照做` + 分支/PR 规则摘要——宪法单源模式已在产验证。

## cron 任务字段速查（jobs.json 实测形态）

`id`（hex）、`name`、`prompt`（单指针 + 摘要）、`schedule{kind:interval, minutes:N}`、`model`、`provider`、`repeat{completed:N}`（轮计数）、`enabled`、`state`、`next_run_at`/`last_run_at`。改任务优先走 `hermes cron` 子命令（CLI 自带 `.bak` 轮换——`~/.hermes/config.yaml.bak-*` 多份在案）；直接手编 jobs.json 是最后手段，改前备份。

## E lane 新建（Claude-Code-native，2026-11-22 二期 P0 形态；照此逐步，一次配完）

1. 克隆：`git clone <BerryOS-private 远端> /Users/murongcheng/berryos-auto`。
2. 身份与闸（该克隆内）：
   `git config user.name berryos-hermes-bot`
   `git config user.email <bot 邮箱>`
   `git config berryos.bot-email <bot 邮箱>`（pre-push 身份闸——推非 `wip/auto-*` 即拒）
   `bash .githooks/install.sh`
3. 发射：`bash tools/run-e.sh start`——e-doctor 先行（工具/身份/钩子/上游任一缺席即拒发射），marker 落盘，tmux 起 run + 心跳两会话。观察：`bash tools/run-e.sh watch`（只读附着不干扰判活）；状态：`status`。
4. 停机：宿主 shell `bash tools/run-e.sh stop`（无人会话环境内拒执）。会话自己跑不动了会走 `e-guard.py fixpoint` 四门写哨兵后自然停。
5. 监督首跑：验收三件（`wip/auto-*` 分支 + 带 `A:` 行 PR + punch-list 首批条目）；心跳/监督行为看 `.claude/state/run-health.json`。

备选形态（Hermes cron）：沿用旧五步（克隆+身份+install+cron 单指针 prompt）仍可工作——但 Stop 守卫/FROZEN/心跳/监督是 Claude Code harness 面，Hermes 会话不经过它们；需要全套机器治理时用 run-e 形态。

## punch-list 与 LOG 的家

- punch-list（人机共享任务队列）：`/Users/murongcheng/Documents/code/berryos/.agents/plans/2026-11-22-e面运营.md` 的「未完成事项」节——runner 经 git 同步读写，owner 直接编辑同一文件；parked 条目格式见 SKILL.md。
- LOG（运行流水）：各 lane 克隆的 `.hermes-loop/LOG.md`（W1 先例形态）——过程真相归 runner 工作区，不进主仓。
- 决策账本：`A:` 行住 PR body 固定节；触及仓规的升格主仓 notes/（SKILL.md §decide-then-report 第 3 条）。

## 排查速查

- lane 停摆：`jobs.json` 看该任务 `enabled`/`state`/`last_run_at`；`hermes cron` 重新启用。
- push 被拒（身份闸文案）：确认分支名 `wip/auto-*` 或 `hermes/*`；确属 owner 代跑用 owner 身份。
- 冲突/基线重写：照 W1 LOOP.md 第 9 步的 BASE REWRITTEN 护栏——LOG 大写并停轮，等 owner。
- 决策争议：owner 在 PR 评论否决 = 下轮第一优先回滚（SKILL.md §decide-then-report 第 4 条）。
