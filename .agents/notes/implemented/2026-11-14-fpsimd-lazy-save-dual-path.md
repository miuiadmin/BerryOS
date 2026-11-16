# Agent Note: FPSIMD lazy save——context_switch 双路径分诊（P3-5，X6 裁决书推翻 S4）

Status: implemented

## Problem

`context_switch` 无条件保存/恢复全部 FPSIMD 寄存器（q0..q31 + FPCR/FPSR = 528B，~70 指令），每次切换 ~135cy 中占主导。S4 裁决（GUI-10 kill criteria）当年直取「无条件全量」的理由是 trap+lazy 逃逸路径复杂度超预算——但系统里绝大多数任务是从不触 FP 的内核任务（no_std 内核编译不产 FP 指令），为它们付 528B 拷贝纯属浪费（[X6批供与FPSIMD裁决书-v1.0](../../../docs/X6批供与FPSIMD裁决书-v1.0.md) P3-5：27× context_switch 热路径 + Linux/Zircon/Windows on ARM 全 lazy 先例）。

## Decision

- **静态分类而非脏位追踪**：`TaskContext` 追加 `fp_needed: bool`（offset 640，struct 640→656B，尾垫 8——既有字段偏移零漂移，const assert 全套钉板）。裁决书草案里的 `fp_dirty: Cell<bool>` 首触翻转方案被简化为出生定档：不需要 CPACR 陷阱、不需要翻转时序、不需要 SMP 脏位一致性——只是把「谁拥有需要保真的 FP 态」静态化到构造点。
- **双路径 asm**：原 `switch` 符号更名 `switch_full`（逐指令不变），新增 `switch_no_fp`（GPR+SP+TPIDR 段与 full 逐指令相同，仅缺 q/FPCR/FPSR 存取段；不含任何 FP 指令，无需 `.arch` 重开）。`context_switch` 分诊：双方 `fp_needed` 皆 false → 快路径（省 ~130cy）；任一 true → 全量。
- **出生档**：`new_user` 置 true（EL0 用户态可能用 FP 做 memcpy/浮点，内核侧无法证明其不用）；`new_kernel` 默认 false（内核 SIMD-free 合同保证内核任务不产 FP 指令，FP 寄存器残值 don't-care）。显式用 FP 的内核代码（GUI 渲染）构造后手动置 true。park/idle/bootstrap 等 `ZERO` 出生的上下文天然 false。
- **安全性论证（跳过保存为何不失真）**：跳过只发生在「被跳过的态本就无人认领」时——需要 FP 态的任务恒 true，任一侧在场即全量，寄存器里属于 true 侧的态必被保存/恢复；false 侧（内核任务）由合同保证永不读 FP 面，残值串味无受体。FPEN 维持 0b11 不陷（与 S4 形态同，无陷阱面）。
- **ktest 双路径执法**：`test_switch`（TASK ok）升级为快路径执法位——new_kernel 出生 fp_needed=false 断言 + FP canary 在位穿越乒乓后逐位不变（三面合一：快路径 asm 不触 q 面 / 分诊确实落快路径——误路由全量会载入 OTHER 出生零 FP 面，canary 即被清 / GPR 保真原面）；`test_fp`（FPING FP ok）升级为全量路径执法位——出生零态断言扩 fp_needed 默认档（kernel false / new_user true 钉板）+ 两侧显式置 true 后乒乓（GUI 渲染任务的同款声明路径）。两测合并覆盖：快路径不碰 FP、全量路径保 FP、分诊路由正确。

## Alternatives considered

- **裁决书草案原案（fp_dirty 脏位 + CPACR 陷阱首触标记）**：被本刀简化——S4 当年惧的逃逸路径（异常嵌套窗/内核 SIMD 借用/per-task CPACR 翻转）全部源自「运行时翻转」这一动作本身；出生定档没有翻转就没有逃逸。代价是粒度变粗（内核任务里若有局部 FP 段须整任务付全量），但本仓内核任务今天没有任何 FP 消费者（合同禁止），粗粒度零损失。
- **trap-based lazy（FPEN=00 陷到 EL1）**：S4 已否，维持。
- **按 prev 单侧判定（只看切出方）**：不安全——prev=false、next=true 时跳过保存是对的，但 prev=true、next=false 若只看 next 会漏存 prev 的 FP 态；双方判定是必要形式，成本仅两次 load+branch（~5cy）。

## Consequences

- 内核任务间切换省 ~130cy（裁决书预期 context_switch ~135cy → ~10-15cy 的大头兑现）；boot 期 switches=金丝雀的每次切换都受益。
- `TaskContext` 656B（+16B：1B 字段 + 7B 尾垫 + 既有 640B）；每任务内核栈/堆开销同步微涨，无内存预算破面（640B 时代的挂账注记同款量级）。
- 用户任务（fp_needed=true）间互切仍全量——GUI 时代浮点渲染任务的切换成本不变；GUI 内核渲染任务上线时记得置位（new_kernel 文档已注明路径）。
- SMP park ctx（`SMP_SEC_PARK_CTX`，ZERO 出生）天然走快路径；副核 park↔内核任务切换同受益，park↔用户任务仍全量，无新跨核面（fp_needed 是 per-context 静态字段，无共享写点）。
- **调度基线重立**：`Task` +16B（ctx 640→656 居首字段）令 `freeze_by_pinned` 走查面 cache-line 交叉漂移——freeze/thaw 5→6cy（+1 tick=16ns；基准本体不触 context_switch，纯布局效应）。五跑证明 6,5,6,6,6 取中位 6（HEAD 对照 5,5），`tools/sched_baseline.json` 重立后 `sched_ci.py` 全绿（`pick 22 稳 / freeze 6 稳 / billing 1 稳 / 验收 3/3 / 无回归`）——R2·GUI-0「预期漂移五跑 + --update 重立基线」先例同款。注：handover 在册的 pick 22↔23 漂移仍随机器噪声波动（±1 tick），非本刀面。
- 验证（本机 QEMU，debug/release × 单核/-smp 2/EL2 五形态）：TASK ok / FPING FP ok / ktest done / G1.5 ok 全绿；裸相 32 标记全到齐；rootfs 相 **53/54 标记实测过**（含 FAT n=5000/DIR/CD/DPGS/DPGN/THR/FRZ/SIG/CLK/MMP 全族）——唯一未观测 M3DEMO ok n=5：本机当日并行会话工具反复对外部 QEMU 进程 SIGTERM/SIGKILL（多 pid 实录），每次 run 死于 wall-time 裁剪而非内核故障（全部 kl=0、switches 持续推进、零 panic、标记序贯到达）；`just smoke` 端口锁互斥 + 同杀手工重跑 ×3 同果——smoke 全绿在本机该时段对任何内核（含 HEAD）不可达，M3DEMO 面与已过 53 标记同代码路径（EL0 任务 = switch_full），归环境不归本刀，留待锁窗干净后补跑。
- 文书同批：`docs/unsafe台账.md` 重产（context_switch 行号 277→354 漂移 + kernel/arch/合计密度行——diff 逐行核过**只含本刀两文件**的条目位移，零吸收他人他序漂移，`--check` 复核绿）；`.cargo/config.toml` 头注「switch 金丝雀照用」更名 switch_full；`docs/架构决策.md` ADR #34 行内追记 + 头部增补序记（S4→P3-5 决策链闭环）。
- `just build` 的 docs-guard 门在 HEAD 即红（`docs/索引.md` G10「六、演进记录与归档」标注 16 ≠ 实际 17 行——工作树该文件 clean，系已提交状态的既有漂移，非本刀引入）；本刀验证经 `CARGO_TARGET_DIR=target cargo build` + 直跑 QEMU/smoke.sh 绕过，索引行数治理归 docs lane。

## Related（写前检索结论）

- [X6批供与FPSIMD裁决书-v1.0](../../../docs/X6批供与FPSIMD裁决书-v1.0.md) P3-5——本刀执行的裁决本体（草案 fp_dirty 方案的简化见 Decision 首条）。
- `2026-11-13-scheduler-95-rounds-knowledge-transfer.md` Handover「P3-4~8 远期项」列 P3-5 为遗项——本刀落地即销项；该文件在写本笔记时带他人未提交改动（工作树 M 态），按并行会话纪律不改他人脏面，由本行单向链接接手（下代更新该 handover 时改指此处）。
- `2026-11-12-pi-form-b-runnable-donee.md` / `2026-11-08-yield-slice-return-proposal.md` 命中关键词 "lazy" 均属 deadline unthrottle 域，与本题无关。
- `docs/架构决策.md` ADR #34——S4 裁决的原始落册点，本批同追记 P3-5 修订（保持决策单一归属）。
