# SCHED-01 调度之王：业界调度机制深研与综合裁决（2026-11）

> 版本：**1.0**（v1.0）｜2026-11-06｜文件名 `SCHED-01-调度之王业界深研与综合裁决-v1.0.md`
> **定位**：v1 冻结前的调度机制裁决稿——回答「调度还能做到多深」，把 8 维业界证据转成 BerryOS 可落地增量、C 项冲突与 S0–S6 路线；不修改任何 ADR 条文，冲突交主会话拍板。
> **方法**：workflow `sched-king-deep-research-v2`——8 维调研（Linux 6.x / Zircon / 强实时与分区 / 消费 OS QoS / 用户态与 AI 调度 / 微内核资源管理 / freeze 谱系 / 调度 ABI 与度量），按 3+3+2 分批规避并发上限 → 40 条 URL 逐条核验 → 1 席红队（17 项攻击）→ 综合员。共 11 个 agent。
> **失败与修复（如实记录）**：首轮 9 个研究 agent 同时并发，全部返回 null（provider 并发上限）；改为 3+3+2 分批后 8/8 成功。结论：workflow 并发不是越大越好，研究型任务 3–4 路一批更稳。
> **证据分级**：【证】= 一手文档/源码/commit 直接支撑；【推】= 由证据推出或本仓目标值；【未证实】= 可达信源无一手支撑，不得写进对外承诺。
> **已知边界（先读）**：① 8 维原始稿未单独落盘，本文是综合稿；② 来源索引标「调研引用」者未逐页核；③ QNX 8.0 是否移除 APS、proxy execution 的合入范围，本次二次核验后已降级/改写（见附录 A）；④ Meta/Google 的 p99 与能耗数字无可核验一手来源；⑤ 所有验收阈值均为【推】的自定门，不是实测。
> **与 docs/ 的关系**：#42/#58/#20/#21 只读不改；冲突见 C1–C6，以新 ADR 或修订案处理。

---

## 0. 总判断与边界

**总判断**：BerryOS 不缺新调度器，缺的是一条可证明的全序链和一条干净的策略边界。v1 增量应压到三件事：fair 类换 EEVDF 双键、加一对 attr、加 token 化 freeze；预算、捐赠、band、AI 全部后置。三份独立证据同向：Zircon 用 class 链 + 用户态 admission 跑通量产；seL4 证明上界只能做成对象而非属性；ghOSt/APS 证明策略进快路径必死。

## ① 执行摘要

1. **EEVDF 是 v1 fair 类唯一需要的底座**：`lag_i=w_i(V−v_i)`，lag≥0 才 eligible，eligible 内选最早 `VD=vruntime+slice/weight`；单核 rbtree O(log n) 足够，不做跨核均衡。[S1][S3]
2. **base slice 不是固定 3ms**：v6.12 为 `750µs×(1+ilog(ncpus))`，8 核才约 3ms；自定义 slice 经 `sched_runtime` 写入并 clamp 到 0.1–100ms。[S5][S6]
3. **sched_ext 只能借契约、不能借实现**：no_std 无 BPF/verifier，watchdog 会引入第二条调度路径；可借的是「有界动作集 + stall 回退 + 热切换」。[S9][S10]
4. **Zircon 给的是最合身的形状**：class 链 `critical-DL > DL > fair > idle`、每类唯一 pick 键、profile 一等对象、attr 用 flags 位图选字段。[S25][S29][S65]
5. **Zircon deadline 不是 CBS**：内核只持久化 capacity+relative_deadline，period 仅校验，无补充、无 admission，超限只告警。[S27][S29]
6. **seL4 MCS 是唯一 µs 级硬上界**：SC(budget,period) + sporadic refill + reply object/SC donation，跨 IPC 仍记客户预算；但无下界、无 admission，且 MCS 证明仍 in progress。[S30][S31][S32]
7. **消费 OS 的共同答案是「应用 hint、系统仲裁」**：Windows QoS 不影响优先级、EcoQoS 可被电源策略否决；Apple/Android 同构。[S38][S44][S45]
8. **Windows 三条教训要写进反面清单**：静默空闲降 QoS 污染基准、HardCap 到限即停造成 burst-stall、线程级 SuspendThread 会冻死死锁持有者。[S38][S39][S41]
9. **ghOSt 死于「定制内核 + 每决策跨态 + agent 专用核」**；sched_ext 赢在上游、安全网、运行即切换。[S49][S50][S9]
10. **APS 是第一 kill criteria 判例**：策略焊进热路径、每次决策扫全部分区、分区硬上限 8；**有资料称 QNX 8.0 已整体移除 APS，但本次未核到 8.0 一手页面（S54 是 7.1 文档），该点标【未证实】**；可借的只是窗口/借用/偿还语义。
11. **微内核侧没有 per-handle 持久 QoS 原语**：Genode 是 RAM/CPU 配额 + client-pays，seL4 是 SC 能力，Fuchsia 是 per-thread profile；可辩护的新颖性只在组合与账务语义。[S56][S67][S58]
12. **freeze 的正确形状是 token，不是布尔**：Zircon `task_suspend` 返回 token，close 全部才恢复；冻结者死亡必须自动解冻。[S74][S75]
13. **单核 v1 的 freeze 必须三重正交**：不在 PI 捐赠边两端、`in_flight==0`、不是带内核派生位的 channel 端点；否则一次合法 freeze 可静默停摆整域。[S74]
14. **饥饿上界不能由 per-handle attr 导出**：weight 只在当前可运行集分份额，class 链上层饱和可把 fair 压到 0；上界只能配置期离线算、内核单调收窄 + 计数。[S1][S62]
15. **syscall 预算已紧**：实测 `abi_gen.rs` 已发 35 号（34 在册），七动词各占一号到 42，M4+ 只剩 3；v1 只加 3 号到 38。[S62][S74]

## ② 业界全景

| 机制 | 系统 | 公平 | 延迟 | 隔离 | 吞吐 | 成本 | 成熟度 |
|---|---|---|---|---|---|---|---|
| EEVDF | Linux 6.6+ | 强 | 中 | 中 | 强 | 低 | 量产 |
| sched_ext | Linux 6.12+ | 取决于策略 | 中 | 弱 | 高 | 中 | 量产 |
| fair WAVL | Zircon | 强 | 强 | 中 | 强 | 中 | 量产 |
| DL/EDF | Zircon | 弱 | 强 | 中 | 中 | 低 | 量产 |
| MCS SC | seL4 | 弱 | 强 | 强 | 中 | 中 | 量产/证明中 |
| APS | QNX | 中 | 中 | 强 | 中 | 高 | 已弃用（8.0 移除待核） |
| ARINC 653 | 航电 | 无 | 强 | 最强 | 弱 | 高 | 认证量产 |
| Job rate | Windows | 中 | 弱 | 强 | 中 | 低 | 量产 |
| EcoQoS | Windows | 无 | 弱 | 弱 | 中 | 低 | 量产 |
| Clutch | XNU | 强 | 强 | 中 | 高 | 中 | 量产 |
| cgroup v2 | Linux | 强 | 中 | 强 | 强 | 低 | 量产 |
| ADPF | Android | 无 | 中 | 无 | 中 | 低 | 量产 |
| ghOSt | Google | 任意 | 中 | 弱 | 中 | 高 | 已停更 |
| freezer | Linux | — | — | — | — | 低 | 量产 |

**读表三点**：其一，公平与硬上界不可由同一机制给出：EEVDF/weight 给比例，SC/CBS 给上界，混写只会得到「看起来都有、实际都无」的承诺。其二，延迟与吞吐在表内系统性对立：ARINC 隔离最强、吞吐最弱；EEVDF/Clutch 反之，选型必须先定主目标。其三，成熟度与「是否进主线」高度相关：sched_ext、EEVDF、cgroup 因上游获得寿命，ghOSt/PDS/BFS 因外挂而死；应把「可长期维护」置于单次收益之前。

## ③ 七问七答

**1. 任何东西可调度？** 最佳 = 统一 thread 对象 + 严格 class 链。硬边界 = IRQ/内核线程/idle 不受用户策略，process/job 只是容器。落点 = v1 调度单位是 thread，process 仅承载地址空间与配额。

**2. 执行进度？** 最佳 = lag/VD。硬边界 = lag 只定义在 fair 类，class 链上层可把 fair 压到 0。落点 = 只对 fair 建 lag；freeze 入冻点 lag 归零，不做补偿。

**3. 资源消耗？** 最佳 = 域级 (budget,period) + 句柄级 advisory。硬边界 = per-handle 硬预算的记账/撤销成本在快路径不可承受。落点 = v1 句柄只带 intent；域契约 v2。

**4. 不公平？** 最佳 = strict 类 + 有界 cap + 显式 freeze。硬边界 = strict 天然饿死、cap 有 burst-stall、weight 无下界。落点 = 承诺改写为「只有显式 freeze 产生无界饥饿，其余必须有配置期可算的界」。

**5. 冻结？** 最佳 = token + 安全点 + 自动解冻。硬边界 = 单核上冻住 PI donor 或 pager 等于全系统暂停。落点 = 三重正交检查 + 义务完成位 + 超时回滚。

**6. 最细粒度？** 最佳 = 线程调度、消息计费。硬边界 = syscall/指令级调度不可验证且开销爆炸。落点 = thread 是调度单位，message header 的 billing_token 是计费单位。

**7. 最强 API？** 最佳 = seL4 SC（硬上界）+ Zircon profile（可组合形状）+ Linux sched_attr（版本化）。硬边界 = 没有单一 API 同时给硬实时、公平、可组合、低开销。落点 = v1 一对 attr，v2 升 SC 对象；实现上宁少勿多，一个字段的兼容成本远低于一个动词。

**七问归一**：调度 API 只能承诺两件事——「按什么键排序」与「最多花多少」。前者要求每类唯一比较器，后者要求预算对象；凡不能归入这两句的承诺（绝对延迟、跨域公平、AI 收益）都应移出内核或移出 v1。

## ④ 设计增量

**4.0 五条原则**：①一个 class 一个比较器，禁止跨类混合键；②快路径只读一次策略快照，判定放慢点；③任何「停住别人」的原语必须先证明不破坏 PI、pager、唤醒三条不变量；④预算与准入分离，内核 enforce、用户态 admission；⑤所有对外数字必须能被离线检查器复现。

**4.1 域树**：root → system → app → job → thread。capacity contract 挂域节点，intent 挂线程，billing 挂消息。三条不变式：子树配额求和 ≤ 父；域内线程共享契约；跨域只经 channel/notification。三者都必须写成 ktest，而不是文档承诺。

**4.2 七动词 = 三个 syscall + 一个只读面**：

| 动词 | 落地 |
|---|---|
| declare/query | `sched_setattr` / `sched_getattr`（attr：size + flags + 3×u64 ns） |
| freeze/thaw | `process_suspend(h,&token)` / `HANDLE_CLOSE(token)` |
| borrow/lend | v2 SC 对象转移，v1 不做 |
| reclaim | 内核自动 + telemetry 计数，无动词 |

**4.3 freeze 六条硬规则**：① token + suspend_depth，close 一层解一层，depth 上限 8；② 只在返回用户态/object_wait 前落冻，内核态记 pending；③ 三重正交检查任一命中 `-EBUSY`；④ 入冻 lag 归零、预算清零、refill 清空、deadline 重锚 `now+period`，禁止结转；⑤ 向 frozen 对端同步 send 有界等待后 `-EDEADLK`，异步 notification 保持 pending；⑥ 冻结者死亡由句柄表 teardown 自动 close 全部 token（auto-thaw）；tombstone 撤销能力但保留记录。

第③条是硬门：本仓 `pi.rs` 的捐赠边可覆盖任意睡眠者，单核上冻住 donor 会让等待者的 `prio_effective` 被高位值钉死，唤醒无法出选。

**4.4 句柄 QoS**：v1 只放 8bit `policy_id` + epoch 快照；状态挂对象（Channel/VMO）不挂句柄；快路径只「读一次 + 无锁累加」，判定放 tick/block 前。禁止 per-handle 令牌桶。

**4.5 预算/准入**：v2 预算对象落 (budget,period,deadline)，`budget==period` 退化 RR，`period==0→period=deadline`；错误三档 EINVAL/EBUSY/EPERM；准入只在域挂载时做，ΣU ≤ 1−system_reserve，热路径只 enforce + 记账。

准入只做一次的理由：在线可行性判定需要全局 ΣU 与 class 链状态，放进热路径即 APS 式扫描；离线算、挂载时验、运行期只记账，才能同时保住机制极简与可解释。

**4.6 受益者付费**：billing_token 随消息头传播（与 intent 的 min 重导同构）；异步/worker 必须携带，否则记「无归属」受限类；服务底薪来自全局单池、work-conserving，不按域累加——按域累加会让 Σr_i 在空载时也抽干带宽，admission 永远失败。

**4.7 借/不借/自研**：借 EEVDF lag/VD、Zircon class 链+profile 形状、Linux attr size/flags/错误分档、seL4 SC+donation 概念、Genode reference account/client-pays、Zircon suspend token、cgroup 四件套、HDR/runqlat 度量。不借 sched_ext BPF/verifier、ghOSt agent、Clutch 代码、SuspendThread、Job HardCap 当保证、cgroup freezer 布尔 API、APS 扫描式热路径、per-handle 硬预算、学习型进快路径、BFS/MuQSS/PDS。自研单比较器 class 链 + 编译期断言、freeze 三重正交谓词 + auto-thaw、billing_token 传播、离线饥饿界检查器 + 文档一致性 CI、freeze lag barrier。许可纪律：Linux/scx 为 GPL、XNU 为 APSL，只取语义与接口形状，不复制代码。

**4.8 失败语义**：统一四类——参数非法 EINVAL、当前不可准入 EBUSY、权限不足 EPERM、对象状态冲突 EBADSTATE。任何「部分生效」都必须回滚：attr 失败不得留下半套参数；freeze 超时必须把已 parked 线程全部 resume；thaw 失败保持 token 有效。日志进 telemetry 环，带 generation/actor/reason，便于事后追责。

## ⑤ ADR 相容性与 C 项

**C1（#42）**：三拓扑「class 链 > band > 带内 EEVDF」与「公平默认、intent 叠加」相容；但若 handle weight/reserve 可自设，intent 就从唯一第一类输入变成第二调度语义域。裁决：v1 不做 handle weight，band 仍只在 fair 内。

**C2（#42 事实）**：ADR 引用的「Meta 软分区 p99 −28%·3.28MW」在可达一手信源中无支撑；可核验的只有 ~1000 台、+3.5% 吞吐、p99 定容的场景描述。应降级为【未证实】，不得再作论据。[S51] 这是本报告最重要的修正：不是措辞问题，而是论据链断裂；继续引用会在评审中被迫用二手转述自证。

**C3（#42 二段）**：「每带预算包络 + deadline-server 兜底」在 v1 无 CBS 准入时不可兑现；推迟到 S5，以「离线检查器 + 挂载期准入」为入场券。

**C4（#58）**：六档词汇表是唯一意图域；reserve/cap/deadline 属域契约，不得暴露成第七档或新 intent 值。

**C5（#20）**：#20 是「单核 + 抢占点 + 无 demand paging」的分阶段基线，而当前已启 demand paging 机制面；freeze 与 pager/PI 相交，属基线变更，须新 ADR 或 #20 修订案，不能静默加入，否则 freeze 会成为「合法但可复现」的停摆入口。

**C6（#21）**：AI 只能带内、慢环、单调收窄；borrow/lend 扩动作空间，与「band/intent 不入动作空间」冲突，v1 不做。

**总体相容性**：#42 的三层拓扑与 #58 的单词汇表是本报告全部增量的边界条件，二者不冲突；真正要修的是事实引用（C2）与二段兑现节奏（C3）。#20/#21 的冲突属「基线已前进但条文未追认」，应以修订案补齐而非绕开；修订案不改变 v1 目标，只改变实现顺序与对外措辞。

## ⑥ 路线图 S0–S6

| 阶段 | 内容 | 可测验收（阈值为自定验收门【推】） |
|---|---|---|
| S0 规格冻结 | freeze 谓词/单比较器/离线界检查器规格 | ktest 骨架 + 文档-检查器一致性 CI 红 |
| S1 ABI | attr 对 + 三档错误 + size 协议 | abi-check；38 号；小/大/E2BIG/截断 fuzz |
| S2 EEVDF | fair 类 lag+VD，750µs 基准 | pick 全序 property；「pick 后未切换」=0；真机切换 p99 ≤ 基线 +5% |
| S3 freeze | token + 三重正交 + auto-thaw | 1000 次无残留 depth；PI/pager/in-flight fuzz 全 -EBUSY；thaw 100 任务首周期超预算=0 |
| S4 记账 | per-CPU 计数 + billing_token + telemetry VMO | 开/关记账 bench-lat p99 差 ≤2%；无归属告警；零新 syscall |
| S5 域契约 | 离线准入 + 挂载期 (budget,period) | ΣU>1−δ 返回 BUSY；界可复现（容差 1%）；Σr=0.9 空载 idle>0 |
| S6 策略/AI | 慢环策略服务 + shadow + 回滚 | shadow 一致率 ≥99%；动作 p99 与回滚 <100ms；有/无 AI 的 pick p99 相等 |

**排期逻辑**：S0–S5 是调点调度 1.1 的范围，S6 是 1.2。S2 前不做 band，S3 与 S2 的先后不可交换：没有 lag barrier，freeze 就是带宽信用攻击面；S3 前不暴露 freeze ABI，S5 前不对外说「预算」。每阶段以 kill criteria 为门，未过门不进下一阶段；QEMU 只作功能与顺序回归，所有分位数字留真机。

## ⑦ 反面清单 + kill criteria

以下十条不是风格偏好，而是从 ghOSt/APS/BFS 三个死亡案例反推的结构性禁令；违反任一条，系统仍可能「跑得起来」，但会在负载迁移或故障注入时退化为不可诊断的静默停摆；每条都应对应一个可执行断言，而不是一句原则。

**反面**：①策略/记账进 pick_next；②每决策跨态调用；③学习型进快路径；④per-handle 硬预算；⑤布尔 freeze；⑥冻结 PI donor/pager；⑦thaw 结转预算；⑧唤醒合并改「谁被唤醒」；⑨reserve 按域累加；⑩用 QEMU 数字报 p99.9。

**Kill（任一命中即回滚）**：freeze 对捐赠边/在飞义务不返回 `-EBUSY`；自冻 10s 后 thaw 的 100ms CPU 超同 weight 公平份额 1.1×；token 持有者死亡后 1 tick 未 auto-thaw；thaw 100 任务首周期 Σruntime>Σbudget；开 QoS 的 bench-lat p99 差 >2%；lend 链 depth≥2 未拒；文档上界与检查器差 >1%；pick 返回不满足 class 谓词的任务。

## ⑧ 度量纪律与来源索引

**度量纪律**：QEMU/TCG 只报功能、顺序、饥饿/死锁与相对趋势，不报绝对 µs、max、p99.9、切换开销；分位数字必须来自真机、≥1e4–1e5 事件、HDR + 溢出计数，并从「应激活时刻」计时以校正 coordinated omission。cyclictest 测唤醒延迟，runqlat 测 runqueue 等待，schbench 测负载，perf sched 无 p99——四者不可互相替代。[S69][S71][S72][S73]

完整 URL 见下方 source_index：带 ✓ 者为本次核验，标「调研引用」者未独立核验。核心 18 条：S1–S6、S9、S10、S14、S15、S17、S19、S25、S27、S29、S30、S32、S35。

**未证实/降级（不得进承诺）**：

- U1 Meta/Google「生产收益 X%」、p99 −28%、3.28MW——无一手数字。
- U2 PDS 现状——一手仓库 404。
- U3 Apple QoS→Clutch bucket 精确映射——未公开。
- U4 PikeOS 的 ARINC 653 具体配置——推断。
- U5 seL4 MCS 形式化时间隔离——proofs in progress（已验证属实）。
- U6 Shenango/Caladan 具体加速比——页面未返回摘要。
- U7 QNX 8.0 移除 APS——未核到 8.0 一手页面（S54 仅 7.1），见附录 A。
- U8 proxy execution 完整合入——6.13 只有 preparatory changes，完整功能仍是 RFC，见附录 A。

## 附录 A：核验纠错（16 条，含本轮二次核验追加）

1. EEVDF 默认时间片不是固定 3ms：v6.12 源码为 750µs×(1+ilog(ncpus))，8 核约 3ms；引用必须带缩放条件。[S5]
2. 提交 3f020399 不是 sched_ext 进 v6.12 的合入点：该 sched-core-2024-11-18 merge 属 v6.13；v6.12 已有 `kernel/sched/ext.c`，v6.11 无。[S8][S14]
3. `docs.kernel.org` EEVDF 页称 2024 年「新选项」，与 Linux 6.6 于 2023-10-29 默认启用 EEVDF 矛盾；版本事实以 kernelnewbies 6.6 为准。[S1][S7]
4. **proxy execution 的合入范围（本轮二次核验改写）**：6.13 只合入「Preparatory changes for Proxy Execution v13」（kernelnewbies 6.13），**完整功能仍是 RFC，不能据此说 proxy execution 已量产**；上一稿「6.13 已含 proxy execution」过度声明。[S14]
5. 「Meta/Google 生产收益 X%」「p99 −28%」「3.28MW」无可核验一手数字；可核验的只有 ~1000 台机器 A/B、+3.5% 吞吐、该负载以 p99 定容的场景描述。ADR #42 中对应引用应降级为【未证实】。[S51]
6. 「Google 转向 sched_ext」是作者重叠（Vernet/Don/Rhoden）+ 双方公开承诺，不是 Google 官方转向声明。[S49][S50][S10]
7. PDS「仍活跃/可评估」不成立：一手仓库已 404，现状未证实，只能标 research。
8. scx_bpfland 不是 6.12 可用：README 要求 Linux ≥6.16，6.13 上 verifier 失败。[S13]
9. Zircon deadline 不是 CBS、也无内核 admission：只持久化 capacity+relative_deadline，period 仅参与校验，critical 超 100% 只告警；不能据此宣称硬实时。[S27][S29]
10. 「seL4 MCS 是已验证硬实时」属过度声明：verified configs 列出 MCS 但官方写明 proofs in progress，且 seL4 验证不含时序/WCET。[S32][S60]（本轮复核：verified-configurations 页面原文确认 MCS proofs in progress）
11. 「cgroup freezer 是布尔开关」错误：层级、异步，写 `cgroup.freeze=1` 只是请求，靠 `cgroup.events` 的 `frozen=1` 确认；D 状态任务可永不完成。[S79]
12. 「freeze 可随时局部停住线程」错误：单核必须避开 PI 捐赠边两端、pager 在飞义务、channel 内核派生位；否则一次合法 freeze 可静默停摆整域。
13. 「v1 已发 34 个 syscall」不准确：实测 `user/berry-sys/src/abi_gen.rs` 已发 35 个号（0x01–0x3B 等），34 是在册预算。
14. 「QNX APS 是可借用的现代方案」错误：APS 于 QNX 8.0 整体移除的说法**本次未核到 8.0 一手页面**（S54 是 7.1 文档）；只能确认 7.1 有 APS，且其扫描式热路径是反面教材。可借窗口/借用/偿还语义，不能借实现。[S54]（降为【未证实】，见 U7）
15. 「per-handle 持久 QoS 是微内核已有原语」不成立：只有 Genode 配额/client-pays、seL4 SC、Fuchsia profile 等分散组件；可辩护新颖性在组合与账务语义，不在单点原语。[S56][S67][S58]
16. **APS 移除（本轮追加）**：QNX 8.0 URL 均 404，无法确认「8.0 整体移除」；报告正文与全景表已从「已移除」改为「已弃用（8.0 移除待核）」，需 QNX 8.0 文档/发行说明补证后才能进对外材料。

## 附录 B：8 维调研范围

1. Linux 6.x：EEVDF / sched_ext / PI & core-sched / cgroup v2 / 非主线调度器。
2. Zircon/Fuchsia：fair（WFQ/WAVL）、deadline（EDF/capacity）、profile 能力门控、affinity/热插拔/大小核（main @82604da，2026-11）。
3. 强实时与分区：seL4 MCS / QNX APS / ARINC 653 及「不公平但有上界」。
4. 消费 OS QoS/限额/冻结：Windows、Apple XNU、Android 对比。
5. 用户态与 AI 调度：ghOSt → sched_ext 的边界证据、生产数字、死因与 kill criteria。
6. 微内核/能力制资源管理：配额、受益者付费、调度权委托与 per-handle QoS。
7. freeze/suspend/tombstone 语义、实现谱系与 BerryOS 边界。
8. 调度 ABI（Linux/Zircon/seL4/QNX/Windows）、CBS/GRUB 形式化与度量方法学。

## 附录 C：来源索引

- S1 · Linux EEVDF 调度文档 · https://docs.kernel.org/scheduler/sched-eevdf.html · ✓
- S2 · LWN: An EEVDF CPU scheduler for Linux · https://lwn.net/Articles/925371/ · ✓
- S3 · LWN: Completing the EEVDF scheduler · https://lwn.net/Articles/969062/ · ✓
- S4 · Linux 6.6 sched-core merge（EEVDF） · https://github.com/torvalds/linux/commit/3ca9a836ff53db8eb76d559764c07fb3b015886a · ✓
- S5 · v6.12 fair.c 基础时间片 · https://github.com/torvalds/linux/blob/v6.12/kernel/sched/fair.c#L76 · ✓
- S6 · v6.12 syscalls.c 自定义 slice · https://github.com/torvalds/linux/blob/v6.12/kernel/sched/syscalls.c#L307-L314 · ✓
- S7 · kernelnewbies Linux 6.6 · https://www.kernelnewbies.org/Linux_6.6 · ✓
- S8 · kernelnewbies Linux 6.12 · https://www.kernelnewbies.org/Linux_6.12 · ✓
- S9 · sched_ext 内核文档 · https://docs.kernel.org/scheduler/sched-ext.html · ✓
- S10 · scx 仓库 README · https://github.com/sched-ext/scx/blob/main/README.md · ✓
- S11 · scx_lavd README · https://github.com/sched-ext/scx/blob/main/scheds/rust/scx_lavd/README.md · ✓
- S12 · scx_rusty README · https://github.com/sched-ext/scx/blob/main/scheds/rust/scx_rusty/README.md · ✓
- S13 · scx_bpfland README · https://github.com/sched-ext/scx/blob/main/scheds/rust/scx_bpfland/README.md · ✓
- S14 · sched-core-2024-11-18 merge（v6.13，proxy execution 仅 preparatory） · https://github.com/torvalds/linux/commit/3f020399e4f1c690ce87b4c472f75b1fc89e07d5 · ✓
- S15 · RT-mutex with PI · https://docs.kernel.org/locking/rt-mutex.html · ✓
- S16 · Core Scheduling · https://docs.kernel.org/admin-guide/hw-vuln/core-scheduling.html · ✓
- S17 · cgroup v2 · https://docs.kernel.org/admin-guide/cgroup-v2.html · ✓
- S18 · PSI · https://docs.kernel.org/accounting/psi.html · ✓
- S19 · SCHED_DEADLINE（CBS+EDF+GRUB） · https://docs.kernel.org/scheduler/sched-deadline.html · ✓
- S20 · Utilization Clamping · https://docs.kernel.org/scheduler/sched-util-clamp.html · ✓
- S21 · schedutil · https://docs.kernel.org/scheduler/schedutil.html · ✓
- S22 · BFS 词条 · https://en.wikipedia.org/wiki/Brain_Fuck_Scheduler · ✓
- S23 · Zircon fair scheduler 文档 · https://fuchsia.googlesource.com/fuchsia/+/82604dadaad3311d8779a5573a6c74aa9de92774/docs/concepts/kernel/fair_scheduler.md · ✓
- S24 · Zircon kernel scheduling 文档 · https://fuchsia.googlesource.com/fuchsia/+/82604dadaad3311d8779a5573a6c74aa9de92774/docs/concepts/kernel/kernel_scheduling.md · ✓
- S25 · Zircon scheduler.rs · https://fuchsia.googlesource.com/fuchsia/+/82604dadaad3311d8779a5573a6c74aa9de92774/zircon/kernel/kernel/scheduler.rs · ✓
- S26 · Zircon scheduler.h（10ms/1ms） · https://fuchsia.googlesource.com/fuchsia/+/82604dadaad3311d8779a5573a6c74aa9de92774/zircon/kernel/include/kernel/scheduler.h · ✓
- S27 · Zircon profile_dispatcher.rs · https://fuchsia.googlesource.com/fuchsia/+/82604dadaad3311d8779a5573a6c74aa9de92774/zircon/kernel/object/profile_dispatcher.rs · ✓
- S28 · Zircon profile syscall 实现 · https://fuchsia.googlesource.com/fuchsia/+/82604dadaad3311d8779a5573a6c74aa9de92774/zircon/kernel/lib/syscalls/profile.rs · ✓
- S29 · Zircon deadline ABI 头 · https://fuchsia.googlesource.com/fuchsia/+/82604dadaad3311d8779a5573a6c74aa9de92774/zircon/system/public/zircon/syscalls/scheduler.h · ✓
- S30 · seL4 手册（MCS/SC） · https://sel4.systems/Info/Docs/seL4-manual-latest.pdf · ✓（仅 HTTP 200/Content-Type，未逐页核对）
- S31 · seL4 MCS tutorial · https://docs.sel4.systems/Tutorials/mcs.html · ✓
- S32 · seL4 verified configurations · https://docs.sel4.systems/projects/sel4/verified-configurations.html · ✓
- S33 · QNX sched_setscheduler · https://www.qnx.com/developers/docs/6.5.0SP1.update/com.qnx.doc.neutrino_lib_ref/s/sched_setscheduler.html · ✓
- S34 · QNX pthread_mutexattr_setprotocol · https://www.qnx.com/developers/docs/6.5.0SP1.update/com.qnx.doc.neutrino_lib_ref/p/pthread_mutexattr_setprotocol.html · ✓
- S35 · QNX APS details（7.1） · https://www.qnx.com/developers/docs/6.5.0SP1.update/com.qnx.doc.adaptive_partitioning_en_user_guide/aps_details.html · ✓
- S36 · QNX SchedCtl 6.5 · https://www.qnx.com/developers/docs/6.5.0SP1.update/com.qnx.doc.neutrino_lib_ref/s/schedctl.html · 调研引用
- S37 · ARINC 653 词条 · https://en.wikipedia.org/wiki/ARINC_653 · 调研引用
- S38 · Windows QoS · https://learn.microsoft.com/en-us/windows/win32/procthread/quality-of-service · 调研引用
- S39 · Windows Job CPU rate control · https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_cpu_rate_control_information · 调研引用
- S40 · Windows CPU Sets · https://learn.microsoft.com/en-us/windows/win32/procthread/cpu-sets · 调研引用
- S41 · Windows SuspendThread · https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-suspendthread · 调研引用
- S42 · Windows Job Objects · https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects · 调研引用
- S43 · XNU thread_policy_private.h · https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/mach/thread_policy_private.h · 调研引用
- S44 · XNU Clutch/Edge scheduler 文档 · https://github.com/apple-oss-distributions/xnu/blob/main/doc/scheduler/sched_clutch_edge.md · 调研引用
- S45 · Android PerformanceHintManager（ADPF） · https://developer.android.com/reference/android/os/PerformanceHintManager · 调研引用
- S46 · Android cached apps freezer · https://source.android.com/docs/core/perf/cached-apps-freezer · 调研引用
- S47 · Android task_profiles.json · https://android.googlesource.com/platform/system/core/+/refs/heads/main/libprocessgroup/profiles/task_profiles.json · 调研引用
- S48 · Android Process.setThreadPriority · https://developer.android.com/reference/android/os/Process#setThreadPriority(int) · 调研引用
- S49 · Google ghOSt userspace · https://github.com/google/ghost-userspace · 调研引用
- S50 · Google ghost-kernel · https://github.com/google/ghost-kernel · 调研引用
- S51 · scx_layered 案例研究 · https://github.com/sched-ext/scx/blob/case-studies/case-studies/scx_layered.md · 调研引用
- S52 · Shenango（NSDI'19） · https://www.usenix.org/conference/nsdi19/presentation/ousterhout · 调研引用
- S53 · Caladan（OSDI'20） · https://www.usenix.org/conference/osdi20/presentation/ousterhout · 调研引用
- S54 · QNX 7.1 APS（8.0 已移除待核） · https://www.qnx.com/developers/docs/7.1/#com.qnx.doc.neutrino.sys_arch/topic/adaptive.html · 调研引用
- S55 · Decima（arXiv） · https://arxiv.org/abs/1810.01963 · 调研引用
- S56 · Genode Resource trading · https://genode.org/documentation/genode-foundations/24.05/architecture/Resource_trading.html · 调研引用
- S57 · Genode base-hw execution · https://genode.org/documentation/genode-foundations/24.05/under_the_hood/Execution_on_bare_hardware_(base-hw).html · 调研引用
- S58 · Fuchsia Profile 对象参考 · https://fuchsia.dev/fuchsia-src/reference/kernel_objects/profile · 调研引用
- S59 · Barrelfish HotPar'10 · https://www.barrelfish.org/publications/barrelfish_hotpar10.pdf · 调研引用
- S60 · seL4 Verification · https://sel4.systems/Verification/ · 调研引用
- S61 · Prosa · https://prosa.mpi-sws.org/ · 调研引用
- S62 · Linux sched_setattr(2) · https://man7.org/linux/man-pages/man2/sched_setattr.2.html · 调研引用
- S63 · Linux sched(7) · https://man7.org/linux/man-pages/man7/sched.7.html · 调研引用
- S64 · Linux uapi sched/types.h · https://raw.githubusercontent.com/torvalds/linux/master/include/uapi/linux/sched/types.h · 调研引用
- S65 · Fuchsia profile_create · https://fuchsia.dev/reference/syscalls/profile_create · 调研引用
- S66 · Fuchsia object_set_profile · https://fuchsia.dev/reference/syscalls/object_set_profile · 调研引用
- S67 · seL4 API 文档 · https://docs.sel4.systems/projects/sel4/api-doc.html · 调研引用
- S68 · QNX 7.1 SchedCtl · https://www.qnx.com/developers/docs/7.1/com.qnx.doc.neutrino.lib_ref/topic/s/schedctl.html · 调研引用
- S69 · cyclictest(8) · https://manpages.debian.org/unstable/rt-tests/cyclictest.8.en.html · 调研引用
- S70 · perf-sched(1) · https://man7.org/linux/man-pages/man1/perf-sched.1.html · 调研引用
- S71 · runqlat · https://raw.githubusercontent.com/iovisor/bcc/master/tools/runqlat.py · 调研引用
- S72 · schbench README · https://raw.githubusercontent.com/masoncl/schbench/master/README.md · 调研引用
- S73 · QEMU TCG 文档 · https://www.qemu.org/docs/master/devel/tcg.html · 调研引用
- S74 · Zircon task_suspend · https://fuchsia.dev/reference/syscalls/task_suspend · 调研引用
- S75 · Zircon task_kill · https://fuchsia.dev/reference/syscalls/task_kill · 调研引用
- S76 · CRIU Checkpoint/Restore · https://criu.org/Checkpoint/Restore · 调研引用
- S77 · ptrace(2) · https://man7.org/linux/man-pages/man2/ptrace.2.html · 调研引用
- S78 · signal(7) · https://man7.org/linux/man-pages/man7/signal.7.html · 调研引用
- S79 · cgroup-v2.rst · https://github.com/torvalds/linux/blob/master/Documentation/admin-guide/cgroup-v2.rst · 调研引用
- S80 · freezing-of-tasks.rst · https://github.com/torvalds/linux/blob/master/Documentation/power/freezing-of-tasks.rst · 调研引用
- S81 · AOSP CachedAppOptimizer · https://github.com/aosp-mirror/platform_frameworks_base/blob/main/services/core/java/com/android/server/am/CachedAppOptimizer.java · 调研引用
- S82 · Linux sched-nice-design · https://docs.kernel.org/scheduler/sched-nice-design.html · 调研引用
- S83 · LWN: sched_ext v2 patch series · https://lwn.net/Articles/922405/ · 调研引用
- S84 · scx_layered README · https://github.com/sched-ext/scx/blob/main/scheds/rust/scx_layered/README.md · 调研引用
- S85 · fuchsia.scheduler role.fidl · https://fuchsia.googlesource.com/fuchsia/+/82604dadaad3311d8779a5573a6c74aa9de92774/sdk/fidl/fuchsia.scheduler/role.fidl · 调研引用

---

**维护注**：本文是调研稿，不是 ADR。**版本化落点 = `docs/调点调度-v1.1.md`**（调点调度 1.1 = S0–S5；1.2 = S6）。C 项拍板后，结论回写 `docs/调度器设计-v1.0.md` / `docs/架构决策.md`；本文只加「追记」防误导。8 维原始稿未单独落盘，若需逐维细节需重跑 workflow 或补采。
