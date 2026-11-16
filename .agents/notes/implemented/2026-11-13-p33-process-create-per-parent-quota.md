# P3-3 process_create 设防：per-parent spawn 配额

Status: implemented

## Problem

`sys_process_create` 此前只受全局 `PROCESS_MAX=64` 一道帽约束（[process.rs](../../../kernel/src/process.rs) 头注「活进程帽」）：单 EL0 进程循环 create 即可占满全局进程表，把其余一切服务的拉起面挤成恒 -ENOMEM——进程炸弹的单点垄断面。路线图在册：[系统优化路线图-v1.0 §P3-3](../../../docs/系统优化路线图-v1.0.md)（「per-Process 配额三帽（已落）向创建入口的自然延伸」）。

## Decision

- **记账面 = 调用方 handle 表在册 Process handle 数（`count_type` 扫描）**，新帽常量 `MAX_CHILDREN_PER_PROC` 落 `process.rs`（与 `MAX_VMO_PER_PROC`/`MAX_CHANNEL_PER_PROC`/`MAX_THREADS_PER_PROC` 同族同位），执法点在 `sys_process_create` 全局帽**前置**（per-parent 分账先于系统总账 fail-fast），错误面 -ENOMEM 与全局帽一致。不新增任何记账状态、无退出递减面——扫描派生随 close/take/drain/channel 迁移天然配平。
- **帽值 24**（任务案给 16）：诊断实测（TEMP 打印表内计数，量毕移除）svcmgr 稳态在册 Process handle 仅 ≈5-7（自柄 1〔berry-exec env 块 `gift[2]`，`pthread_create` 终身消费〕+ echo×2/talker 泄漏柄 3〔`spawn_child` 局部量不关——v1 常驻泄漏政策〕+ 瞬态 1-2；app 批 9 子**串行 spawn→reap 即关柄**、GUI 链各 arm 多为瞬态——[svcmgr 并行预拉笔记](./2026-11-13-svcmgr-parallel-prepull.md)的 spawn 序佐证），dpgf 满配波 15 子并发叠加后峰值 ≈20-22——**16 会在波中途拒造**（dpgsoak 变体面，`DPGF_N=15`），24 放行全合法面且仍小于全局 64 帽 2.6 倍——反垄断语义保留（单父至多吃系统进程表的 3/8）。
- 语义口径 = 「本表在册 Process handle」而非「活亲子数」：含自柄与经 channel 迁入的他人进程柄，是既有三帽同款的在册语义（换取零 sidecar 状态）；亲子数口径的漏洞（父 create→start→close 循环可绕在册帽）由全局 64 帽兜底——该循环的每个子都要走全 bootstrap 链（vmo/map/start），是全局帽合法预算的真实进程，非廉价空壳堆积。

## Alternatives considered

先搜分类（notes/AGENTS.md 纪律）：命中两篇旧笔记——[svcmgr 并行预拉](./2026-11-13-svcmgr-parallel-prepull.md)（重叠互链：其 spawn 序/驻留语义是本帽 sizing 的输入）、abi enforced_since 台账（无关：仅 syscall 名面命中，不涉 spawn 配额）；无过时提案转 rejected。

- **选项 A（`Process` 加 `children: AtomicU32`，create +1 / exit -1）——否决**：三重结构性障碍。① `Process` 无 `parent_id`/`children` 字段（实施前核实），create 面拿不到父对象挂账；② EL1 常驻服务表（`spawn_with_tag` 挂裸 `HandleTable`）与 boot 期手搭进程根本没有 Process 对象——计数器无宿主；③ 递减面（子退出 → 父计数 -1）在父先 drop 时是对已释放对象 fetch_sub——这正是 `process.rs` 头注（per-Process 配额帽块）为 VMO/Channel 帽已文档否决 sidecar 的同型 UAF：handle 跨表迁移下增减结构性失配。本帽跟随既有裁决，不翻案。
- **选项 B（全局进程表扫 `parent_id` 匹配）——否决**：系统无全局进程表（Process 对象只活在各 handle 表里），落地需新增 parent 追踪 + 全表注册面，且与选项 A 共担 EL1 无宿主问题——为 O(1) vs O(n) 优化引入两套新状态，方向反了。
- **「handle 表 65535 槽帽已间接限制 → not-applicable」——评估后不成立**：`PROCESS_MAX=64` 全局帽远先于 65535 槽帽绑定——单进程在册配额缺位正是 64 帽被单点垄断的成因，P3-3 适用于此、非 not-applicable。
- **ktest 灌满负向测试（Channel 帽 1952 行同款）——否决**：灌到 24 需真造 24 个 Process（L1 表树帧 + ASID）且永驻 `bootstrap_table`——`PROCESS_LIVE` 全程 +24，直接吃掉全局 64 帽近四成、污染一切下游 smoke 面。执法语句与 Channel/SuspendToken 帽逐字同款（`count_type` ≥ 帽 → -ENOMEM），该模式已有 ktest 钉板；本帽的实证面交给 smoke（默认 + dpgsoak 变体）。

## Consequences

- `just check` 全绿（4 告警均为 channel.rs 既有无关项）。
- 默认 smoke（`just smoke`，三跑 86/86 标记 + FAT/掉电相）**PASS**——`SVC ok`/`DEMO ok`/`THR ok`/`APP ok n=` 在链内全到，配额零误伤默认面。
- dpgsoak 变体（dpgf 满配 15 波，本帽最紧合法面）：共享宿主上 QEMU 场被兄弟会话长占（脚本让行协议放弃）+ 手动复现跑被宿主侧周期性 SIGKILL，**契约形态（-smp 2 全设备）未能完整跑毕**——留 CI/空闲窗口补跑。已完成的归因链：手动 -smp 1 变体卷跑中 `G1.5 ok` + 四任务标记到齐；诊断内核两轮全程零 `P33Q` 打印（计数门槛 18）= svcmgr 全程 <18、波峰 ≈20-22 < 24——配额在变体面**不开火**；该 -smp 1 跑的 `eco: dpgf1 sp`（波 1 相位 1）与配额无关（phase 1 含 vmar_map/thread_start——dpgf 的 demand 反查 16 槽「boot 1 + 15 恰满」设计绑 -smp 2 时序，单核属非契约形态）；同点 G1.5 双核 kill 窗口竞速两次复现于 -smp 2、-smp 1 消失，亦与本帽无因果（帽只拒创建、不杀进程）。
- 攻击面变化：廉价炸弹（create 不 start、纯持柄堆积）被压到单表 24；v2 Job 落地后（`job` 参数收口）本帽自然演进为 Job 分账（`process.rs` 既有挂账注释）。
