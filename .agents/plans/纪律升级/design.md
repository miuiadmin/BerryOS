# 纪律升级（工程纪律六面）总体设计

Type: spec
Status: active
Updated: 2026-11-22
Version: 1.0.0

一句话说明：本模块把 [impossible-os 工程纪律调研](../../research/2026-11-21-survey-impossible-os工程纪律.md)的采纳裁决立为 BerryOS 六面契约——门禁防绕过、判据单源与新鲜度、数字自证、规划双戳、E 面与无人值守 runner——使「做完」成为机器可盖戳的状态、「数字」成为脚本重算的派生制品、「门禁」成为带防绕过的机器执法；域铁律正文唯一家是[规则/16-工程纪律](../../../docs/规则/16-工程纪律.md)，本文立契约与验收口径。

## 背景

调研结论（详见上述 survey）：impossible-os 八个月到真机桌面，支柱不是写码速度而是三条纪律——状态机判（节级双戳，调度只读结构不读散文）、数字自证（计数由脚本重算、无时间戳回写 git 跟踪制品）、门禁防绕过（判暂存区不判工作树、门路径清跳过变量、印记拒 `--no-verify`）。owner 裁决四面全上；check-docs 同批升格提交硬门槛（AGENTS.md 规则 8 改写、1:1 终裁同批改裁——见[立项笔记](../../notes/implemented/2026-11-21-工程纪律升级立项与AGENTS回写和解.md)）；E 面 runner 底座收编 Hermes 既有四 lane 舰队（w1/w2/w3/review，独立克隆 + `hermes-auto` PR 队列，W1 已 691 轮）。BerryOS 现状底子：markers.sh 已是 smoke 判据单源但有三个漂移面；bench icount 基线已是数字自证先例；`.githooks/` 三件实装但判工作树且无 install 脚本；bcts 已是冻结集回放。

## 范围

- **覆盖**：`.githooks/` 全套（pre-commit 暂存快照重写、prepare-commit-msg 印记闸、install.sh、pre-push 身份闸段）；`tools/` 判据与计数面（markers.sh 收编 ECO 与 banner 正则、image-fresh.sh、floor-check.sh、facts.py 与 facts.txt、gate-selftest.sh）；`.agents/tools/check-docs.mjs` 三刀（`--verify-facts`、`--selftest`、双戳检查）；design.md 节级双戳语法与反黑洞机检；`.agents/skills/berryos-runner/`（SKILL.md 宪法 + RUNBOOK.md 接入手册）；`docs/规则/16-工程纪律.md`（域铁律正文）；AGENTS.md 规则 8 与相关 规则/README、规则/13 的改裁。
- **不覆盖**：跨模型评审 pin、docs 覆盖率棘轮、站点漂移检查、OS Comparison 表、loopx（owner 排除）；`.agents/README.md` 上游零改动；40 存量 design.md 不强制回填；ci.yml 全局串行结构与 required-check 聚合不动。

## 契约

### 一、门禁与防绕过契约

1. pre-commit 只判暂存区快照：`git checkout-index -a --prefix=<tmp>/` 全索引物化后 `node .agents/tools/check-docs.mjs --strict <tmp>/.agents <tmp>/docs`（显式路径模式）；check-docs 保持零依赖、不读 git 的 Curia 契约，git 物化归钩子。工作树修好/索引留毒必须被拒；兄弟会话未跟踪半成品不得误拦——两条都是 gate-selftest 固化腿。
2. 门路径强制 unset 一切 SKIP 类旁路变量；唯二 owner 阀 `BERRYOS_ALLOW_CROSSROOT` 与 `BERRYOS_ALLOW_NOVERIFY`。
3. prepare-commit-msg 印记闸拒 `--no-verify`（merge/cherry-pick/revert/squash 免检；印记 900 秒新鲜、消费即删）；与 reference-transaction（ref 层跨根兜底）互补分层。
4. 明示不做：amend `--only` 机器强制（pre-commit 层不可靠机判）——残风险三层兜底（staged 清单打印 + pre-push + CI）并写进 规则/16。

### 二、判据单源与新鲜度契约

1. `tools/markers.sh` 四族（MARKERS/ROOTFS/ECO/RX）+ `BANNER_FORM_RE` 唯一源；smoke-image.sh 两处硬拷贝改 sourced；smoke.sh/smoke-smp.sh banner grep 换用单源；直跑自检腿做全表两两互非子串与哨兵↔消费者 1:1 映射机检。
2. `tools/image-fresh.sh` 对拍 `out/manifest.txt` sha256 与现场重算值；挂 smoke-image 开跑前与 gui8-smoke 默认路径（陈旧即重建，`GUI8_REBUILD=1` 降级为强制重建）。
3. `tools/floor-check.sh --verify`：rustc 地板解析 `rust-toolchain.toml` 单源；qemu/just/node/python 地板常量与消费者注释同文件；证不出的地板即失败；CI 首步同脚本。
4. gui8 判读词不搬家（负向前瞻正则塞不进 shell 数组）：单源留 `tools/gui8-smoke.py`，计数经 facts 派生对拍。

### 三、数字自证契约

1. `tools/facts.txt` git 跟踪、`key=value` 排序行、零日期；键面：tests_rust、smoke_bare/rootfs/eco/rx、smoke_gui8、bcts_floor_live、docs_md、design_md、stamped_designs、banner_consumers。`python3 tools/facts.py` 对拍、`--update` 刷新、`--selftest` 合成树夹具。
2. check-docs 加 `--verify-facts`（对拍文档面两键，文档计数权威身份不变）与 `--selftest`（每检查器一组必触发坏样例）。
3. `tools/selftest_guard.py` 扫描面扩到 `.agents/tools/*.mjs`；刷新义务：动计数输入面的同一提交带 `--update`。

### 四、规划双戳契约

1. 适用面：design.md 已含至少一条戳行者（戳在场即受检）；存量不回填；棘轮 = `stamped_designs` 入 facts，撤戳即数字降即 diff 可见。
2. 戳语法与反黑洞三红（裸 `- [ ]`、计数不符、缺 Quality reviewed）的机读语义在 check-docs；人读语法在 规则/16——同批改纪律承袭 Curia 类型表既有取舍。
3. Deferred 带 XREF 相对链接，闭合留 RESOLVED 回执。

### 五、E 面与 runner 契约

1. 宪法单源 `.agents/skills/berryos-runner/SKILL.md`（每轮循环、decide-then-report、五类硬停、park 语义、一单元一推推到绿、路径白/黑名单、git 身份与分支纪律）；Hermes cron prompt 一句指针。
2. punch-list = 活跃计划「未完成事项」节（Curia 原生机制复用）；parked 项带 `[PARKED→人审: 理由]` 标记。
3. 独立克隆结构性隔离；bot 身份经 `git config berryos.bot-email` 入 pre-push 身份闸；只推 `wip/auto-*` 与 `hermes/*`，永不碰 main 与集成分支，绝不自 merge。
4. RUNBOOK.md 含一次 Hermes 侧探索产出（四 lane LOOP.md 盘点、cron jobs 字段、CLI 用法、LOG 轮换）。

## 验收标准

- [x] 阶段 1 立法：`node .agents/tools/check-docs.mjs --strict` 绿；`python3 tools/docs_guard.py` 绿（G1/G6 在帽内）；AGENTS.md 规则 8 与 规则/16 互指成立
- [x] 阶段 2 判据单源：`bash tools/markers.sh` 自检绿；`just smoke` 与 `just smoke-image` 判读词逐字不变（裸相 32/ROOTFS 56/ECO 52/bcts 91 稳）；伪造 manifest 哈希 image-fresh 退出 1；内核重编后 gui8 自动重建；floor-check 本地与 CI 双绿
- [x] 阶段 3 门禁：gate-selftest 六腿全绿（含工作树修好/索引留毒攻击腿、兄弟半成品不误拦腿）；临时克隆装钩子后坏文档与 `--no-verify` 两绕过面均被拦；真实仓常规提交零扰动
- [x] 阶段 4 数字自证：facts 对拍绿且幂等（`--update` 后 `git diff` 为空）；删一个 `#[test]` 红并指认键名；check-docs `--selftest` 全夹具触发；selftest_guard 零漏接线（含 .mjs）
- [x] 阶段 5 双戳：本文件试点盖双戳；`--selftest` 三腿坏样例（裸 checkbox/计数不符/缺 Quality reviewed）全触发；facts 显示 `stamped_designs >= 1`
- [x] 阶段 6 E 面（仓内侧）：SKILL.md/RUNBOOK 过 `--strict`；身份闸腿绿（bot 推集成分支拒、推 wip/auto-* 过）；punch-list 载体落位（[E 面运营计划](../2026-11-22-e面运营.md)未完成事项节）——E lane 首跑（owner 机配置 + 监督轮 + 验收三件）挂 punch-list 前三条

> Verified: 2026-11-22 · 147005ed1 + 收口笔 · 6/6 · tests 6
> Quality reviewed: 主会话对抗自查——六检测器自检首跑共抓 8 处真伤（ver_le 实参反/macOS awk -v 拒 '='/快照根相对路径 82 假红/腿 f 无差异/facts 好坏夹具对调/RUNBOOK 无类型头/链接层级错/4 篇未同批刷 facts），快照门活体两咬立制者本人；E lane 首跑（owner 机配置）与 CI 首跑观察挂 punch-list 在案 · 2026-11-22

## 适用边界

- 本设计立法不改 Curia 上游（`.agents/README.md` 零改动）；BerryOS 本地纪律正文唯一家 = `docs/规则/16-工程纪律.md`。
- 双戳人读语法（规则/16）与机读语义（check-docs）同批演进，属有意双份表示——与 Curia 类型枚举两份表示同取舍，改一面必须同批改另一面。
- facts 计数口径是静态字面量口径（`#[test]`、标记数组长度、文件数），与运行期结果口径（cargo result、smoke verdict）并立互不可比，勿混引作同一数。
