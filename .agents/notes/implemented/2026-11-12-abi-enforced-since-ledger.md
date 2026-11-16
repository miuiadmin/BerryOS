# Agent Note: abi.toml enforced_since 执法台账——权利门控执法起始日单源化（路线图 P1-4）

Status: implemented

## Problem

R2 安全审计（沿革 §第二轮架构研究）发现 9 个能力缺口（1 High + 4 Medium + 4 Low），其中 policy_bind 零门控（High）、RIGHT_TRANSFER 零执法 / vmo_get_phys 零权利（Medium）已修，但执法事实只活在 git blame 与沿革散文里——abi.toml 作为 ABI 单一事实源无法回答「 syscall X 的权利门控从哪天开始执法」。新 syscall 入册时没有任何机制提醒补能力门控，裸面可能再产（审计建议原文：「abi.toml 冻结位表加 enforced_since 列防再产裸面」）。

## Decision

- **字段语义 = 现行权利门控集执法起始日**（非 handler 首落日）：取该 syscall 最近一次权利门控变更的落地日。channel_send 记 arch-r2 补 TRANSFER（2026-11-11）而非 3-4b 首门 RIGHT_SEND（2026-10-16）——台账回答的是「现门控集自何时完整执法」，审计价值最高；语义在 abi.toml §4 头注与 ABI 文档 §4 图例双落。
- **值形 = TOML 裸日期**（`enforced_since = 2026-11-12`）：tomllib 直接产 `datetime.date`，格式错在 TOML 解析层即炸；gen_abi.py validate 对引号字符串显式拒（「留给消费面猜就是漂移面」）。
- **日期取 git 提交实锤**（author=commit 双日期一致），逐门控 `git log -S` 追首落提交核对引入 diff 含门控行；本刀登记 24 行——任务书给的五笔日期有四笔与 git 差 1–2 天（S1/S3-a 实为 2026-11-06 非标注的 2026-11-08；policy_bind/channel_send 实为 2026-11-11 非标注的 2026-11-12），台账从 git 不从记忆。
- **生成面**：文档 §4 表新增「执法起始」列（`—` = 无门或未标注）；enforced_since 不进 kernel/berry-sys 常量层与 syz（执法日是审计面不是 ABI 常量——四产物零漂移，仅文档面 diff）。
- **绊网 = WARN 不 FAIL**：abi.toml 无逐 syscall 参数表（§8.3 原型层 v2 挂账），「涉 handle」由 gen_abi.py 侧 `NO_GATE_SYSCALLS` 白名单（17 行，逐行注记豁免理由）单源裁决；live 行缺 enforced_since 且不豁免 → WARN（stderr，abi-check 照 PASS）。selftest 另钉「在册台账零 WARN」——新 syscall 裸面 = selftest 红，评审判走 enforced_since 或豁免单二门之一。
- **live 判定**（row_live）：显式 `implemented` 覆盖一切；缺省 = v1/1.1 在册即实现。v2/v3 行不缺省实现（0x83 policy_bind 类「v3 但字面量挂派发」的例外行自带 enforced_since，不依赖判定入账）。

## Alternatives considered

- **每权利位一列**（roadmap 原文「逐 syscall/权利位」）：被拒为过度工程——单 syscall 多门位（vmar_map 双对象三门）会膨胀成嵌套表；单日期 + abi.toml 行内注释（门位名 + 首落提交锚）已可审计，位级精度留给 git blame。
- **enforced_since 进 kernel/berry-sys 常量层**：被拒——内核不需要执法日期（运行时零消费），进 abi_gen.rs 徒增两端镜像面。
- **WARN 升 FAIL（挂 abi-check 硬门）**：被拒——台账是登记面不是执法面；硬门会把「豁免单漏更」变成 build 断裂。selftest 零 WARN 钉板已是足够的强制力（test-host 前置）。
- **白名单放 abi.toml（如 `gate_exempt = true` 字段）**：被拒—— abi.toml 行字段该描述 ABI 事实而非生成器执法策略；且新号忘加任何字段都会静默，白名单在生成器侧缺席即 WARN，绊网方向正确（deny-by-default）。

## Consequences

- 24 个带门控 syscall 全量登记（vmo/vmar 族 3-2 起、channel 3-4b 起、sched 双号 S1 起、五笔安全修复 2026-11-11/12 止）；17 个无门 live syscall 显式豁免（close/dup/replace/wait 族/创建面/几何惯例 need=0/零 handle 面）。
- P3-1（RIGHT_DUPLICATE 执法）/P3-2（process_create 设防）落地时：handle_duplicate 出豁免单 + 进登记，process_create 同理——「每项关闭即 enforced_since 登记」有处可落。
- 后续给 live syscall 加/改权利门控时，enforced_since 应随刀前移到新门控落地日（语义是「现行门控集」，不是历史最早日）。
- abi-check 与 build/smoke 链零扰动（PASS 口径不变）；gen_abi selftest 61→69 通过。
- 验证：`just abi-check` PASS / `just docs-guard` PASS / `gen_abi.py --selftest` 69 passed / check-docs 0 警 / 绊网功能实测（合成 0x99 裸号恰产一 WARN 指名）。

## Related

- [系统调用ABI设计-v1.0.md](../../../docs/系统调用ABI设计-v1.0.md) §3 能力检查 / §4 表执法起始列 · [系统优化路线图-v1.0.md](../../../docs/系统优化路线图-v1.0.md) P1-4（本刀落地）· [阶段沿革.md](../../../docs/阶段沿革.md) §第二轮架构研究（缺口清单）与 §安全修复 #1–#4
