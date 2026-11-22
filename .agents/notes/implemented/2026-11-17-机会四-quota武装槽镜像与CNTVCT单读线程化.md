# 机会四：band 授片 quota 武装槽镜像与 FairBand tick 的 CNTVCT 单读线程化

Status: implemented

## Problem

[04 号调度热路径审计](../../../research/内核深度进化/04-调度热路径-Top5优化机会-v1.0.md) 机会四：`TaskClass::grant_cycles` Band 臂每调用一次 `quantum_cycles() → ns_to_cycles()`（cntfrq MRS 直读 + u128 乘除软例程，数十指令量级）；消费面 = sched.rs 八 Band 可达授片点 + `FairBand::rearm_vd`（每片耗尽 tick）+ `rearm_members_vd`（限流边沿环扫）。CNTVCT 同族冗余：一次 `FairBand::tick` 内 peek_tick_delta + Fair.tick 委托读 + period_check 实参读 + consume 内部读 = 每 tick 四次计数器读。三路勘察（wf_d897b86b-c13）：武装槽里已躺着答案（`BANDS[band].quota` 冷配置字段）只是 grant 路径不读它；签名爆炸半径比审计预估更小（period_check 本就 now 参数形）。

## Decision

- **刀①镜像**：band.rs 新增 `pub fn quota_cycles_of(band) -> Option<u64>`——读法镜像 band_on 先例（slot + 单趟读定门：quota/period 各读一次、以同一次读到的值定门与返回——审查轮收口，杜绝 armed 门过、返回值二次读撞 configure(0) 清零窗的 Some(0) 撕裂形）；已武装（quota∧period 均正）→ Some(quota)、未武装/越界 → None。不进 lib.rs re-export（全树零外部直连）。class.rs 私有 `quantum_cached`（命中槽镜像、回落现算）；三换点 = `TaskClass::grant_cycles` Band 臂 + `FairBand::rearm_vd` + `rearm_members_vd`（授片与 VD 重臂对称跟槽）；**arm 不动**=写点保持现算（「现算 → configure → 槽镜像」数据流单向）。`quantum_cycles`/`period_cycles` 公有签名不动。
- **镜像一致性依据**：quota 唯一写点 configure + 重配面「先 unthrottle_all 清残留再 configure」收口契约（band.rs doc 原文在库）+ 表行 ns 与 cntfrq 运行期恒定 ⇒ ns_to_cycles 纯函数 ⇒ 懒武装路径槽值==现算值逐位。**显式 configure≠表行时授片跟随槽 quota 是有意语义变化**（与 arm「显式 configure 优先」同向、授片与 consume CBS 执法同源；THRYLD 50µs 合成 quota 场景已在树——两形皆绿因其无 grant/VD 断言）；未来 M4 启动配置覆盖路径接入者须读 quota_cycles_of doc（记名义务在档）。
- **刀②now 线程化（四读归二）**：`FairBand::tick` 入口单读 `let now = cntvct()`，三消费点换参——`peek_tick_delta` → `peek_tick_delta_at(now)`（私有改名加参、删内部读）；`period_check(now)`（签名本就 now 形零改）；`consume(delta, now)`（pub 签名扩参、删内部读）。Fair.tick 委托的 last_vct 基准读保持原位（它是**写点**）。
- **语义差文档化（禁写零语义变化）**：入口读与原各点自读间的指令窗（亚 µs 级 vs 最小期 1ms）使恰边界翻期/首越线可迟一拍——方向单边保守（限流多持、记账只少不多、双兜底在席）；例外：懒武装首臂 deadline 锚随入口读前移一指令窗、首次解限可早一拍（一次性、每带仅首臂、同量级——审查轮补记）。反向收益 = consume 剪辑与 period_check 同参，「剪辑界 > period」理论窗消灭（CBS 期内律不变式收紧）。
- ktest 零改动核证：M4G 预武装断言恰钉回落臂（旧值）、轮内钉镜像臂（恒等）；BAND-SMP BS-2 双锚同款；period_check ktest 七处显式传参零扰动；恰等边界测试（BD3/INTEG）余量周期级；LOADGEN/THRYLD/INTEG/BAND/SMPLZ 显式 configure 面与 grant/FairBand 面 ktest 零组合。

## Alternatives considered

- **arm 也换镜像形**（审计原文「rearm_vd/arm 同改」）：可达路径上镜像恒 None（band_on 门先返）——两形行为逐位同；保留现算使数据流单向清晰，弃。
- **quota_cycles_of 进 re-export**：全树零外部直连需求（M4G 经 grant_cycles 端到端钉板已足）——不进；后续 ktest 想直钉镜像值再补。
- **两刀分批**：代码面正交（勘察核证），同批落使 class.rs 注释一次滚新到位。

## Consequences

- 带任务每授片/VD 重臂省 cntfrq MRS + u128 软除法（数十指令）→ 一次 load + 分支；带 tick 路径 CNTVCT 四读归二。Unit 臂零漂移（slice_cycles 缓存 static 本就零除法）；未武装 Band = 回落现算逐位同旧。
- SMP 重配竞态（configure(0) mid-flight 对核 grant 读旧值一发）：u64 对齐无撕裂、有界、方向安全——与 consume/period_check 冷字段同族读法同窗竞态，非新增暴露等级（注释记档）。
- 验证实跑（隔离 target-s5c）：cargo build 绿×2；boot 相 smoke 32/32 ×2（BANDM4/BAND-SMP 带族 ktest 数值面随套过——镜像恒等论证的运行时面）。
- 对抗审查（wf_2cf96b04-c13，10 agent 四维）：6 发现（5 CONFIRMED/1 PLAUSIBLE/0 rejected）全处置——grant_cycles doc「两值恒等」加限定（显式 configure 场景不成立）；tick 注补懒武装首臂锚点反向微差；API 参考合计行粗体 181→183（v1.13 遗传两版落后，本版正身）；class.rs:169 与 main.rs:5352「八授片点」随正典加 Band 可达限定；PLAUSIBLE 单趟读定门收口；笔记/沿革本笔。
- API 参考 v1.13→v1.14 升版随迁（git mv 保史）：`quota_cycles_of` 新行（band.rs 10→11、合计 **183** 粗体正身）、`FairBand::consume` 行签名+now 参与语义差注、八处入链滚新（含沿革被并行 lane 覆写后重打一次）。
- 沿革五百五十三（551/552 被并行 lane 同窗占用，落笔前重 census 取 553）。

## 重落地附记（2026-11-19）

本刀提交（本笔记「验证」节所指）在合并 1e7872196 中被裁为僵尸态（代码面缺席、无冲突文件存活）。机会四 已按勘察落性表在 icount-baseline 分支重落地（见 2026-11-19 重落地刀笔记）——本笔记现状陈述自重落地起重新为真；旧提交号指向被裁形。