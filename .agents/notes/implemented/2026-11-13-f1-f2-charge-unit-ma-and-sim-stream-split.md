# F1/F2 终裁落码：充电票单位统一 mA 与 sim 流按设备分端

Status: implemented

## Problem

[04 号复核](../../../research/调度彻底计划/04-PM双服务骨架复核.md)点名两件「立法两文自带的缝」（§7-F1/F2），骨架期各 crate 各忠一文、无人认领，接线刀（CP-08）前必须闭合否则双消费者分帧错乱：F1 充电限流票面单位双轨——berry-thermal 出票面 `charge_limit_ma`（mA，忠 ADR #66 ⑤「电流版」）vs berry-power `ChgArbiter` 仲裁面 `limit_pct`（%，忠 ADR #68 冻结形 `CHG_OP_VOTE_LIMIT{limit_pct}`），mA↔pct 换算与终裁无主；F2 同一 UART1 sim 字节流（task_simcon 转发）的消费切分三方主张未合流——simbattery 只收 `DEV_BATTERY`（余拒 BAD）、berry-power 会话面三设备全收、berry-thermal 从 `DEV_THERMAL` 组装 Tick。

## Decision

- **F1（单位统一 mA，主会话终裁）**：#66 ⑤「JEITA 充电温区五档电流版」为终态形态，mA 为票面正统；#68 的 pct 面 = 协议冻结时快照形态，只作兼容读写。落码：[charge.rs](../../../user/bins/berry-power/src/charge.rs) 仲裁域整体并轨 mA（`vote(slot, limit_ma, reason)`、`ChgDecision{limit_ma}`、不限哨兵 `CHG_LIMIT_UNLIMITED_MA = u32::MAX` 与 berry-thermal `CHARGE_NO_CAP_MA` 同值同语义）；桥 = 设计容量单源字段（`BatterySnapshot.charge_full_uah`，µAh）的 C 率换算（1C = 设计容量数值 mA——JEITA 电流档惯例域），`pct_of_ma`/`ma_of_pct` 双向向下取整 = 只收紧不放宽，`vote_pct`/`ChgDecision::limit_pct` 为 #68 线面兼容口（**升版携 mA 直传时退役改口**，F1 兼容锚）；低电预充策略参数并轨 `precharge_limit_ma = 2400`（0.5C @ sim 4800 mAh 基准，推断标注）。
- **F1 实证面（桥表达力边界）**：host 判定律（性质测试）证明 pct→mA→pct 与 mA→pct→mA 在 pct 表达域（<1C）内只收紧不放宽（允收严 ≤1 个百分点、禁放宽）；≥1C 电流在 pct 面无表达（100 = 不限），超域往返放宽到哨兵——**#68 冻结形表达不了的域，正是 F1 裁 mA 的实证**，显式钉板不藏。
- **F2（sim 流按设备分端）**：`DEV_BATTERY` 归 simbattery（现状即裁决，零改）、`DEV_THERMAL` 归 berry-thermal（其订阅面）、`DEV_MODEM` 归 berry-power（遥测）。berry-power 侧按代码现实取最小改：`PwSession` 会话机保持三设备可解（pwproto 通用消费面——真机相 battery 策略输入 = 电池驱动自报经同一会话机；host 测试/合成流 = loopback 面），仅接受面注释收窄记录裁决（[session.rs](../../../user/bins/berry-power/src/session.rs)：sim 流份额 = modem 遥测、不主张全流消费权）；分端执法（通道拓扑）归接线刀，两服务 main.rs 窗注（[berry-power 窗 2](../../../user/bins/berry-power/src/main.rs)、[berry-thermal 留窗 2](../../../user/bins/berry-thermal/src/main.rs)）同步锚。

## Alternatives considered

先搜笔记：命中[PM 双服务骨架](./2026-11-13-pm-dual-service-skeleton.md)（重叠互链——其 Related 已预言 F1/F2 须裁，本篇即裁决落地，不吸收合并）；无 rejected 旧案。

- **ChgDecision 双字段（limit_ma + limit_pct 并存）——否决**：pct 无容量不可算，双字段须在投票时携带容量并在两值间保持一致，等于把桥藏进状态机；mA 单真相 + 派生兼容读（`limit_pct(cap)`）才是单源纪律。
- **berry-power 收窄会话机代码面（仅解 modem）——否决**：会话机是通用 pwproto 消费面，收窄即砍掉真机相 battery 策略输入与 host loopback 全链测试（lib 8 件测试喂 battery 帧走策略面），改动面大且与「PM 中枢遥测角色」相悖；裁决落在**流消费权**（谁从 UART1 收什么），不在传输解码头。
- **改 berry-pwproto / 架构决策.md（把 mA 写进协议或 ADR 表）——否决**：#68 明文「同窗冻结」，pwproto 与 ADR 均非本刀指定文件；兼容桥形态本就对冻结线零改（pct 线值照收照回），升版立法归 chargerd/协议刀。
- **哨兵常量收进 berry-pwproto 单源——留窗**：两 crate 各自常量 + 同值同语义注释锚（值钉测试防漂移）；终态随 chargerd 契约立法收协议 crate。

## Consequences

- 验证实跑（host = aarch64-apple-darwin，`CARGO_TARGET_DIR=target-cargo` 覆写全局配置）：berry-power 31 测全绿（原 28，+4：桥边界/桥性质律/兼容投递链/不限哨兵穿透；1 件满票档测试由 mA 域不限哨兵版取代）、berry-thermal 13 测全绿（原 12，+1：F1 出票面 mA+哨兵穿透钉板）；双 lib `aarch64-unknown-none` 真编通过（2 条 warning 为仓 rustflags 既存面）；berry-thermal `--features wired` bin check 过（main.rs 窗注编译面）；dry-run 复算全对——`chg=2400mA(r2;compat 50pct@4800000uah)` = min(低电 2400, 用户 vote_pct(80)→3840)，兼容读恰还原 0.5C。
- `ChgErr` 增 `BadCap`（兼容面容量 0 无据拒）；`CHG_LIMIT_UNLIMITED`（u8=100）拆为 `CHG_LIMIT_UNLIMITED_MA`（u32::MAX）与 `CHG_LIMIT_UNLIMITED_PCT`（100，兼容面专用）——公共 API 破式更名，消费面全在 berry-power crate 内（grep 实证零外部引用）。
- 04 号复核 §6 清单第 3/4 件（裁决类）闭合；第 5/6 件（装配）与 chargerd 本体（第 9 件）仍留窗。F3（test-host 未挂两 crate）、F4（bin shim 缺注）不属本刀。
- 阶段沿革行未补：沿革件为并行线占用 M 态（VNC 改号），零碰纪律让行；本笔记即本刀过程记录。

## Related

[04 号复核](../../../research/调度彻底计划/04-PM双服务骨架复核.md) §6-3/§6-4/§7；[PM 双服务骨架笔记](./2026-11-13-pm-dual-service-skeleton.md)；ADR #66 ⑤/#68（[架构决策](../../../docs/架构决策.md) 决策总表 ：74/:76）。
