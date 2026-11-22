# am §二.2 首刀：唤醒风暴熔断核 berry-wakeflood

Status: implemented

一句话说明：E lane R10（DAG `am §2` 有界片段）——契约二.2「唤醒即解冻与风暴熔断」的判定面落地：新 crate `user/berry-wakeflood`，每客户端分钟桶环形计数（定容零分配，换桶归 prev 再旧清零——「每分钟超 12 次」按单分钟桶计，跨分钟不累计）+ 12 次/分钟钉值熔断 + 冷却窗释放（纯时间驱动——时钟是释放唯一驱动，释放后速率仍异常由 on_wake 立即再熔断环守，抖动环防护）+ tick 单调回拨忽略；host 7 测。退避投递、计时器驱动与 power-thermal 耗电台账登记归 appd 接线刀。

## 裁决与判例

- **释放纯时间驱动（死条件清理）**：初版释放判据带「新速率回健康」条件——换桶后 cur 恒 0，条件恒真（死代码）；且把速率健康性绑在释放门上会在「冷却期满但洪水持续」态产生释放-再熔断抖动。定版：冷却期满即释，速率异常由再熔断环即时兜住——两态机各自单驱动（熔断=计数驱动，释放=时钟驱动）。`A: (run decision, 2026-11-22) 释放纯时间驱动 + 再熔断环（更简且无抖动门）. Reversal: on_tick 单分支改。`
- **冷却窗 5 分钟是工程默认**：表文只钉 12 次/分钟——`COOLDOWN_MIN` 注记归 appd 配置刀（同 amgrad JUST_BG_WINDOW_MS 先例：表未钉值以常量占位、注记真源）。
- **单分钟桶语义**：「每分钟超 N 次」解为当前分钟桶计数（非滑动 60s 窗）——实现简、判读清；两分钟各 12 次不熔断（测试钉板）。

## 验证

`cargo test --manifest-path user/Cargo.toml -p berry-wakeflood --target aarch64-apple-darwin` 7/7 绿（阈下健康/同分钟超阈熔断/换桶清零/跨分钟不累计/冷却释放与再熔断〔trip_minute 随迁〕/洪水持续再熔断/tick 回拨忽略）；全量门禁随批。

## Related

- [am 总体设计](../../plans/am/design.md)（契约二.2 判定面兑现；租约四源接线/WAKE_PENDING 执行链/ANR 取证挂清单）
- [amgrad 首刀](2026-11-22-amgrad首刀五级梯度.md)（§一 决策核——租约集同源联动）
