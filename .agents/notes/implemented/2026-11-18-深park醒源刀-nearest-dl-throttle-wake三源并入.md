# S5-c 深 park 醒源刀：nearest_dl_throttle_wake 三源并入 nearest_deadline

Status: implemented

## Problem

S5-c 刀挂账（遗漏检查轮勘正升级为正确性权重）：deadline 任务预算燃尽置 HELD_THROTTLE 后残留停泊核环上（不可偷/不可出选/不摘环），解冻唯一通道=家核出选面 `lazy_unthrottle`；而 `nearest_deadline` 只并 SLEEPQ 队头与 `band::nearest_throttle_wake`（带级）两源——SLEEPQ 空且无武装带时副核 `mask()` 深 park（CNTV IMASK=1），vdeadline 到期零中断=复活迟至任意外源醒=**无界迟延**（同 F5 修前形态；无界严格条件=副核+par_enabled+SLEEPQ 空+无带限流+无 GIC IRQ）。三路勘察（wf_2021e954-7fc，基点 icount-baseline@合并后 HEAD）证伪成立：origin 69e1715ca 形（合并裁留的 S5-c 实现）与我方 095d41d13 形解冻语义零差，均无醒源。

## Decision

- **环走查案**（否决全局 deadline-min 账本形——账本需置位/重臂/kill 三账点+min 跟踪〔精确 min 要堆或容忍陈旧下界空醒〕，而逐任务 vdeadline 是环局域惰性态：停泊期环上 throttled 集不变，读真值零账零漂零执法面）：sched.rs 新 `pub fn nearest_dl_throttle_wake() -> Option<u64>`——头部 `dl_on_ring()==0` Relaxed 早退门（门常闭=v1 全员 fair 零行为差）；门开 `try_rq_lock`（F3 非阻塞判例）**只走本核环**（定谳依据：限流任务不可偷+复活唯一途径=家核 pick，对核走查只添冗余 arm 功耗负向）；过滤律逐位镜像 lazy_unthrottle 四与门**前三**（prio>=PRIO_DEADLINE ∧ THROTTLE 位 ∧ duration_hint_ns>0；**刻意不判 now>=vdeadline**——醒源取未来最近到期，过期者由 pick 面补）；返被限流 DL 的 min vdeadline；try 失败返 None（「锁竞争=环上活动=家核醒着有 pick 面」论证入头注——阻塞 rq_lock 形违 F3 红线）。
- **nearest_deadline 第三源并入**：三元组 min 合并；SQ 窗外调取（分窗不嵌 SQ——integrity_check 三窗先例）；三域同绝对 CNTVCT cycles 零换算（duration_hint_ns 名带 _ns 实存 cycles 命名漂移注）。消费面自动继承：park 三武装点（idle_loop 双臂/secondary_main）+tick tighten 臂（band 同待遇统一形——DL 回场延迟从片级收紧到精确 vdeadline）。
- **THROTTLED_ON_RING 第二账本不立**（主线裁）：先 dl_on_ring 门+THROTTLE 过滤落地，对核正常跑 DL 期间本核每次 park 空走查的成本量化后再议——头注挂账句。
- **ktest THRYLD T5**（n=4→n=5，头注五段化）：`rq_insert_kt` 入真 PER_CPU_RQ（J8/PILAT 判例——手搭本地环对走查不可见）；三断言——(a) 两 throttled DL 取 min vdeadline+nearest_deadline 三源合（SLEEPQ 真空基线对照臂）；(b) duration_hint_ns=0 负控不臂（vdeadline=1 深过去，漏滤=min 塌到 1=arm_at 立即醒自旋活锁的钉板）；(c) 环上 DL 无 THROTTLE 位→None；测毕 rq_unlink_kt ×3 出账 DL_ON_RING 配平+摘后 None 复位。
- 注释滚新四处（on_irq_tick tighten 三源臂/block_current 忙核臂刻意不对称/F5 强制让点两处「护栏由带级补齐到逐任务级」）+class.rs:160 doc 勘正（解冻清 HELD_THROTTLE 非 FROZEN——行为与行内注释一致化）+lib.rs 再导出一行（ktest 扁平消费）。

## Alternatives considered

- **全局 deadline-min 原子镜像**（band::DEADLINES 同款）：三账点+min 维护复杂度，环局域惰性态不值得——弃（勘察设计路定案）。
- **走双核环**：对核冗余 arm 功耗负向——只走本核。
- **THROTTLED_ON_RING 门**：更状态更竞态面、漏计危险向——先落地后量化。
- **tick tighten 臂不并入**：正确性只要求 park 臂——并入取 band 同待遇统一形（零额外分支）。

## Consequences

- 深 park 无界迟延闭合：throttled DL 的 vdeadline 成为停泊家核的定时醒源（每期至多一醒/parked 核——band F5 同阶；门常闭时零行为差=深 park 功耗语义保持）。
- 已知理论窗（审查轮核验）：门读旧零（link 先 head 后 len 的切片）漏臂有界（下一次 park 重查）；try 锁竞争瞬态漏醒（对核 place 跨核入环窗口）——锁竞争本身=环上活动论证兜底，残余窗归外源醒（同 band 侧既有形态）。
- 验证（/tmp/icount-wt 隔离 target）：cargo build 绿（首轮一处 T5 类型错主线修——Some 包裹）；boot smoke 32/32（THRYLD T5 随套过）；icount prove 五跑 verdict=exact——基线四项布局骰漂移（alloc4k 37→127/alloc64k 37→233/vmo_commit1p 396→485〔+89≡alloc4k 增量 1:1 纯继承——S-4 4-3 同族判据〕）+pick_scan 125→131（+4.8% 内联位移，switch_pair 301 逐位不动佐证切换舞蹈无染）；归因条目入 bench_baseline_icount.notes.md、双基线重烘。
- 对抗审查（wf_be55303c-424，19 agent 四维+逐 finding）：15 发现全 CONFIRMED 零误报，全处置——①**MAJOR 定时器 IRQ 风暴**（主线原裁「tick 收紧臂并入」被推翻：过期 vd 经 tighten 无 d>now 门写过去 CVAL→CNTV 电平重挂自旋至片尽、每 DL 周期复发、忙核零收益〔≤1 片片尽 pick 本兜底〕）——修=两形分离：`nearest_deadline` 回两源（tick 臂/block 臂），新 `nearest_deadline_park` 三源（park 三武装点专用）+block_current 臂同款裁决对齐；②T5 三补强（换插序使 min 持有者非环首〔截断敏感〕+**t5d 过期正控**〔本刀核心理由钉板——区分臂剔除正修与走查滤过期值错修〕+三源合断言切 park 形〔风暴修连带必炸点〕）；③try 失败 None→有界兜底臂 now+一片（两已核漏醒窗〔对核偷取扫描持锁/SVC 窗放锁〕的严格更优收口）；④HELD_FROZEN 陈名三处复修（合并丢我方四修后的余账：class.rs:321+ktest B1 两注）；⑤API 参考 **v1.15 正身**（机提重数勘误链：183/184 均低估→**186**；v1.14a 字母后缀无先例弃用；落位修正迁 sched 节；nearest_deadline 行回两源+park 形新行；band.rs:296 与 API band 行两源注滚新）；⑥归因算术勘正（+89 vs +90 差 1=近邻继承非严格 1:1）。
- API 参考 v1.14a 补登（nearest_dl_throttle_wake 新行+nearest_deadline 三源滚新+计数 184）；S5-c 笔记两挂账行注记已修；F5 笔记/SMP-01/F3 收口笔记三处 THRYLD n=4 历史引用加注（注记式不改写）。
- 沿革随 icount-baseline 分支提交时定号；THROTTLED_ON_RING 量化挂账。验证补遗：风暴修+T5 补强后终树 build 绿+boot smoke 32/32（T5a-d 全随套）+icount prove 复跑（值若漂移按纪律重烘归因——见 bench notes）。
