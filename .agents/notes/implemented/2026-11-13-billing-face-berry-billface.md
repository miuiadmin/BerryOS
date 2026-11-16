# telemetry billing 域账户与压力窗折卷纯面外迁 berry-billface（W3-Q76）

Status: implemented
Type: note
Related: 判例——manifest 人工组标签须与 auto_group 映射同组（2026-11-12 SLICE_HZ_OVR、2026-11-13 HT_DRAINQ 两案同律）

## 背景

kernel/telemetry.rs 有两段内联判定（S4-b 区间计费 + AM-2b 压力窗口）无 host 钉板：`billing_charge` 的槽分派三态（既有域累加/空槽 CAS 抢占/表满覆写最小账户）与 `dpg_stall_fold` 的折卷判定（回绕窗/θ_p 占比判据）均为原子表旁的内联纯逻辑，判定律改动零自动执法面。

## 落刀

- 新 crate `billface`（W3-Q10 telface / W3-Q6 itally 同款遥测账本族纪律）：`charge_step`（快照入参 → ChargeStep 三态决策）、`domain_total`、`fold_step`（WinState → FoldStep）、`win_len_of`——原子读改写/CAS/回调注册物理臂留 telemetry 壳。
- `billing_charge` 消费侧改 CAS 竞态收敛循环：Claim 败者以新快照重走分派（收敛界 = 表容量有限域）；行为与旧内联逐语义对齐（判序/覆写语义/并列取最前均不变）。
- 常量单源：`BILLING_SLOTS`/`THETA_P_PERCENT` 迁 crate，telemetry 侧 const 镜像消费（ktest BL-OVF 仍钉 8 值执法）。

## 随刀 base 债

writable_globals HT_DRAINQ×3 组标签失配：base 9791a99ce 登记 `kernel/src/process.rs::HT_DRAINQ{,_LEN,_WARNED}` 为 `scheduler`，但 `auto_group(process.rs)` = `other`——`--selftest` live 对拍（含组值）在 base tip 必红，堵全部 PR 的 test-host 门。修法 = SLICE_HZ_OVR 判例同款：manifest 三行组标签归位 `other`。

## 验证

- billface 16 host 测全绿（ktest test_billing·overflow 全周期模型对拍/θ_p 恰界/回绕窗/饱和病态/常量冻结锚）。
- test-host 163 组 1524 全绿 EXIT=0（隔离 CARGO_TARGET_DIR 前台实跑）；kernel no_std check 0 error；docs-guard/abi-check PASS；wg --selftest 0 面红、--check 82 entries；unsafe 台账随源重产 994 对拍绿。

## 判例

billing_charge 的 CAS 收敛循环模式：判定律外迁后消费侧以「快照 → 纯函数决策 → 原子执行 → 败者重走」闭环，胜者独占判阈执行——与 dpg_stall_fold 的 swap(0) 胜者独占同构，可作为后续原子账本判定外迁的参考实现。
