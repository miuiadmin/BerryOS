# 预算授片分派刀：dl_grant 律与 grant_cycles_of per-task 面

Status: implemented

## Problem

S5-c 刀 Consequences 挂账（最久语义债）+机会四笔记 CBS 口径限定：`dl_refill` 解冻置 slice_left=budget，但生产八个授片点派发尾 `slice_left = class_of_task(next).grant_cycles()` 整片覆盖——授片面（TaskClass::grant_cycles(&self) 无 Task 参）对 deadline 轴（prio_effective>=PRIO_DEADLINE）结构性不可见：Unit deadline 任务拿全局 100ms 片（10Hz）、Band deadline 任务拿带 quantum，CBS「每期至多 budget」只在解冻后首片成立，此后每次再派发重授帽=100ms。两路勘察（wf_014160ce-a59）设计定案+关键边界定谳：budget<=0 **必须回落非断言**（ktest 无预算 deadline 手造任务大量实存+生产 budget_ns=0 是 sched_setattr 文档明言合法态+replenish/throttle_admissible 回落律钉板先例——断言会在合法配置上炸；0 授片活锁分析：slice_left=0+永不置 THROTTLE=语义僵尸，回落形直接消解）。

## Decision

- **三件套最小落位**（「经 class_of_task 分派面勿散写」的现结构解）：①数学律 `dl_grant(prio_effective, preempt_threshold) -> Option<u64>` 进 berry-sliceface（W3-Q11 host 钉板纪律——三态+边界四例：deadline+budget>0→Some / budget<=0→None / 非 deadline→None / i32::MAX 不溢出）；协调律入注：dl_grant 前置两合取 == throttle_admissible 前两合取——**受预算执法任务集恰是受预算授片任务集**（免执法者免预算片，一句律）。②class.rs per-task 面 `TaskClass::grant_cycles_of(&self, t: &Task)`（dl_grant 命中返 budget，否则回落 self.grant_cycles()）——class 面 grant_cycles 原样（ktest 既有断言零动）；deadline 轴正交于 intent 带轴（Band+deadline 实存组合〔ktest SMPLZ sldl/生产 INTENT 块与 policy=3 块独立可并置〕预算压过带 quantum——设计意图内）。③sched.rs 八授片点+idle 点**统一换臂**单习语（idle 双零免疫由 dl_grant 双门结构性保证）。
- **ktest**：七处镜像点（BANDM4-G 四+BAND-SMP 三）同批换 grant_cycles_of 镜形（fair 带任务双门不中回落——断言值恒等全绿）；DL 族尾新增 **B5 段**三态断言（deadline+budget=77→77 / budget=0→class 面回落 / fair→class 原值）+Band+deadline 复合臂（预算压过带 quantum，assert_ne 前提守卫防同值假绿）。
- **诚实口径**（s5c 探针注记同批翻新）：本刀把再授片帽 100ms→budget（占空比相位均值 ≈25%→≈12.5%），**不闭合**两条既存语义：执法检测延迟仍=到下 tick 边界（arm_slice 臂全局片不读 slice_left——后续刀候选=arm_slice 收紧 min(slice_left, SLICE)）；抢占窗再授片宽恕仍在（闭合需 switch-out 记账——后续刀候选）。验收文案禁写「每期≤budget」硬口径与「0.5%」。

## Alternatives considered

- **新 TaskClass::Deadline 变体**：class 轴只看 intent_effective，加变体须把 prio 读进 class 分派并触全部 match 臂（enqueue/tick/pick/yield）——错误轴，弃。
- **grant_cycles 签名扩 t: &Task**：ktest 六点连带 churn 无额外收益——弃（新 per-task 面）。
- **断言 budget>0**：合法 API 态上炸——回落定谳。
- **idle 点保持旧形**：最小 diff vs 单习语——统一换（免疫性从路径例外变结构性）。

## Consequences

- 生产 budget>0 deadline 任务每次重授=budget（帽 100ms→budget）；Band deadline 预算压过带 quantum；idle/fair/无预算 deadline 逐位零漂移。
- 验证：**绿基独立验证法**——origin 线带 VMO 记账断言在途红（hermes PR#532 lane 债，纯 tip 实测同红定谳），故刀补丁落于已知绿基（376399d0e）独立验证：just build 绿+boot smoke **32/32（B5 随套过）**+sliceface host 8 测绿+**真预算探针实机通过**（S5C-DLP ok stalls=2 iters=255465——新授片语义下燃尽→限流→解冻→复活链成立）+prove 五跑（绿基留档；正式基线重烘待 origin 修红后——推送树 CI 面红纯系其 VMO 债，与本刀无关）。
- 残留超卖面（本刀缩小非闭合）两条已入 s5c 注记+本档：tick 采样粒度/arm_slice 全局片（候选刀：收紧 min）；抢占窗再授片宽恕（候选刀：switch-out 记账）。
- S5-c 笔记挂账行/机会四笔记 CBS 限定段翻真+API 参考 v1.16→v1.17（grant_cycles 行+新行）——主线文档批随提交收。
