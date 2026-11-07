# 2026 新 OS 四家吸收路线图（调研 12 号 → 行动）

Type: plan
Status: draft
Updated: 2026-11-06

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
8. **P2（期权，不排期）**：presence 语义（移动档语音 idle 交互）、describe/state/invoke 三动词（berryui typed action 参照）、云真机门禁（Hetzner 形态）、「为什么不是 BKL」ADR。
9. **同批笔记**：每项落地时各写一篇 note（决策因果），本计划翻 Status。

## 未完成事项

- [x] P0-a proposal 起草（→ [2026-11-06-baremetal-bringup-proposal.md](2026-11-06-baremetal-bringup-proposal.md)，三台阶：云 VM→RPi4→rootfs 通道，Status: draft 待裁）
- [x] P0-b 存储设计节追加（→ `docs/存储布局与沙箱设计-v1.0.md` §8.5 吸收议程）
- [ ] P1-a 拒收入账实施（含遥测词汇表扩号）
- [x] P1-b 纲领充实（→ `docs/架构综合纲领.md` §1「机制极简的三条结构性论证」+ churn 反证）
- [ ] P1-c 同意面草案
- [ ] P1-d errno 值域探针（随 berry-sys 下一刀）
- [ ] P1-e 对抗性分配器 ktest（随 mm 域下一刀）
- [ ] P2 四项挂账（Presence/三动词/云真机/BKL-ADR）
