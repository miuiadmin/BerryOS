# 2026 新 OS 四家吸收路线图（调研 12 号 → 行动）

Type: plan
Status: done
Updated: 2026-11-08

一句话说明：把 research/12 号调研的吸收清单排成可执行路线——每项标归属文档与验证关，不夹带代码（排除 8 问：全部只借语义/论证，四家许可虽宽松但 BerryOS Apache-2.0 不搬实质代码）。

## 背景

2026 全场扫描确认内核级 AI 原生位空缺（调研 12 号 §1）；四家深读给出 9 项可吸收机制（§5）。本计划是那 9 项的落地路线，P0 两项对冲最大风险（QEMU→真机鸿沟、OTA 存储安全）。

## 步骤

1. **P0-a 真机预研立项**：新侦察文档（树莓派 4/5 bring-up 三件套 UART→GIC→timer + UDP netconsole 预埋，参照 TrustOS 三通道与 swift-os 真机先例）；产出 = 一份 proposal（目标/成功判据/候选路线），挂 G5 后里程碑。
2. **P0-b 存储分层补签名 base 层**：`docs/存储布局与沙箱设计-v1.0.md` 议程追加"签名只读 base + 持久 data"节（swift-os 形态，OTA 回滚安全），先设计后实施。
3. **P1-a 拒收入账**：AI 调度护栏拒收事件进 trace 环（astrid「审计链含 deny」启示）；改动面 = 遥测环词汇表 + 护栏拒收路径各一处。
4. **P1-b 哑内核三理由充实纲领**：`docs/架构综合纲领.md` §1 补「业务逻辑不进内核」三论证（schema 耦合/沙箱逃逸/共享状态污染），注明出处调研 12 号。
5. **P1-c AI 同意面设计**：新设计文档草案（consent-by-identity：SET_POLICY 委托时用户可见审批——Veda 机制 + BerryOS 能力委托链的接合）；依赖 G 线 GUI 收口。
6. **P1-d errno 值域探针**（13 号 §2 坑二）：berry-sys/libc 兼容层把目标 libc 宏实际值纳入对拍（swift-os 断层教训：内核 98 vs newlib 112 害 nginx 拒合法配置）；翻译只住 shim 层，内核 errno 编号不动。
7. **P1-e 对抗性分配器 ktest**（13 号 §2 坑一）：mm 域连续分配低区边界测试——预留全部低帧复现越界形态、验证有界失败与精确边界成功（swift-os trap_return fault 实录）。
8. **P2（期权，不排期）**：presence 语义（移动档语音 idle 交互）、describe/state/invoke 三动词（berryui typed action 参照）、云真机门禁（Hetzner 形态）、「为什么不是 BKL」ADR、**编译期执法议题**（moss「编译器禁持自旋锁跨睡眠点」——类型系统消灭违规类别的思路，评估 handle 权限位类型化/lockdep 热路径子集编译期化，调研 14 号 §3）。
9. **同批笔记**：每项落地时各写一篇 note（决策因果），本计划翻 Status。
10. **2025 段增量（调研 14 号）**：P0-a proposal 已按 urthr 实证修订（目标板 RPi4→RPi5 翻案候选 + 真机 CI 形态参照）；EEVDF 选型获 moss/Linux 双点主流确认（S2a 无回归风险）。
11. **源码级增量（调研 15 号）**：① EEVDF 时钟增量化（moss VClock——`avg_vruntime` O(n) 重算改增量维护 Σ/Σw，入队 O(1)，挂 S2b+ 议题）；② yield 片归还语义自查（moss `yield_slice`，随调度域下一刀）；③ SDHC ≈2,600 行实测（P0-a 台阶三量化锚点）；④「为什么不是 async 内核」ADR 候选（与 BKL-ADR 并列）；⑤ **Veda 许可修正 GPL-3.0-only**——其两项吸收（consent/三动词）降为纯语义参照、代码永不进。
12. **sans-IO 四手法（调研 17 号，2026-11-08 入账）**：P6-A8 网络线设计输入——① 时间外置（RTO 由注入 tick 驱动 = 确定性可测）；② 零拷贝边界（RX/TX DMA 直指 + syscall 边界单次拷贝）；③ 每包零分配（堆只在控制面）；④ scope 诚实清单（in/out-of-scope 明列）。量级锚：最小真网络栈 ≈3,572 行。

## 未完成事项

- [x] P0-a proposal 起草（→ [2026-11-06-baremetal-bringup-proposal.md](2026-11-06-baremetal-bringup-proposal.md)，三台阶 + 延后条款：暂不购板云 VM 先行，2026-11-07 仓主裁）
- [x] P0-b 存储设计节追加（→ 存储布局 §8.5 + 16 号签名双槽形态精确化）
- [x] P1-a 拒收入账（POLS-R ktest QEMU 实证）
- [x] P1-b 纲领三论证（架构综合纲领 §1）
- [x] P1-c 同意面（**设计就绪**：docs/AI同意面设计-v1.0.md + v1 先落两件已落地——consent 五事件词汇 + [agent-actions] 风险档 42 测；v2 审批 UI 随 GUI 线，非本路线图义务）
- [x] P1-d errno 值域探针（发现 EADDRINUSE=98 断差 → 仓主裁 98→112 落刀，52 测全绿）
- [x] P1-e 对抗性分配器测试（mm 域 34 测全绿 + buddy 语义边界文档化）
- [x] yield 双刀（发现缺口 → 抬均 → ring max 真让出 + 账本调和，YIELD-K 全绿）
- [x] sans-IO 四手法入账（P6-A8 网络线设计输入）

## 终局注记（2026-11-08 收口）

- P2 期权全部转记归宿：presence/三动词 → AI 同意面设计 §3；云真机门禁 → 真机 proposal 台阶一；「为什么不是 BKL/async」→ ADR #69/#70 已落；编译期执法 → ADR #70 长期议题；EEVDF 增量化 → 兄弟 S2b+ 账本刀已覆盖。
- 方法资产：六波 workflow 编排经验沉淀为技能 .agents/skills/workflow-implement/SKILL.md。
- 证据链：research/12–23 号 13 份 ↔ 提交链 ↔ 笔记 ×4 ↔ ktest 标记（POLS-R/YIELD-K）全可追溯。
- 步骤 9 笔记承诺收窄说明：机械实施项（P0×2/P1-a/b/d/e 等）不另立笔记（.agents/notes/AGENTS.md「只写决策级内容」），决策级四篇（yield×2/品牌/ktest 否决）+ 18 号裁决已覆盖。
