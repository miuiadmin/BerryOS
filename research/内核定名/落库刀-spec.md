# 内核定名落库刀 spec（Meihe/莓核）——f4 拟，13 落

## 终裁

2026-11-06 用户终裁：内核定名 **Meihe（中文主名：莓核）**。两轮同规格深研（Goji `wf_02714ab7-79c` / Meihe `wf_54fb54d7-af4`，各五面并行扫描 + blocker/caution 逐条对抗复核）均零 blocker，两强对照后用户择 Meihe。**取名不动代码**：crate 仍 `kernel/`、syscall/ABI 零变化——本刀纯文档面。

## 官方口径（建议稿，用户可改字）

- 中文主名：**莓核**；读 hé（同「内核」之核）——钉死，防儿化 méi húr 漂移（语言面判词）
- 对外：**Meihe kernel of BerryOS**；README 首行注解建议 `Meihe (MAY-huh) — the microkernel of BerryOS（莓核，莓果之核）`
- **同音梗官方认领**（语言面判词的前提条件：「没核」完全同音若不主动认领则视同 blocker）：tagline 建议直接接梗——**「莓核——小到近乎没有的核」**（微内核极简人设自证）
- 「内核」仍是通名，**不做全库批量替换**——Meihe 只进身份定义位

## 改动清单（预估 ≤6 档）

1. `AGENTS.md` 身份段：首行定义中内核冠名（「内核 **Meihe（莓核）** `#![no_std]` + 用户态服务」）。⚠️ 预算 ≤200 行 / ≤3400 字符（docs_guard.py 执法），净增 ~10 字符可控
2. `README.zh-CN.md`：项目简介首段加内核名一行
3. `docs/阶段沿革.md`：末尾追记里程碑条目（定名 + 两轮深研 + 终裁 + 口径）
4. 定名决策档：按库规落 `docs/`（或 `research/`）新建 `内核定名-v1.0.md`（版本规则 名-v主.次.md）：终裁、候选对照结论、软雷三条与对策、同音梗认领口径、占位清单。**/tmp/k3-review/{goji,meihe}/wf-result-full.json 全量数据在 /tmp 易失——关键结论以此档入库为准**
5. `docs/索引.md`：补新档一行
6. **零触碰**：CLAUDE.md（仅 @AGENTS.md 指针）、雷册（非判例）、全部设计文档正文（不做「内核→莓核」批量替换）

## 工程纪律

- 纯文档刀：零 QEMU、零构建，与 S-6 run10 让场零冲突
- 主树有未提交 sched 改动在飞：**原子点名暂存（git add 具名档）勿裹挟**；单独成提交（混线教训 §A133 / DOC-DE5）
- 分支落位归 13 裁（wip/sched-1.1 有在飞工作；独立小分支或 main 直落按多线现场定）
- 完工回执 → f4 hunk 复验（diff 只对本刀 commit）

## 占位清单（用户账号动作，非本刀范围）

- GitHub org **`meihe-os`**（RDAP/API 实证空置）——免费，建议即刻
- 域名 **`meihe.dev`**（Google Registry RDAP 实证未注册）——最值的一个；meihe-os.org / meiheos.org 可选
- crates.io `meihe`：注意无「占位」机制，须首个版本发布才锁名——不急，开源发布时落
- 开源/对外传播前：**中国商标网人工查第 9/42 类**（网检不能，两案共同挂账）
