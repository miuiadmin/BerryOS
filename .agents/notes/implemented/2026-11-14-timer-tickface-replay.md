# W3-Q31 重放：timer 滴答账纯面外迁 berry-tickface

Status: implemented

## 背景与动机

#216（37d899000）曾合入 base 后被重写剥落（f55f02e0e 防回流剥除族），W3-Q29「九犯干净重放」同款纪律重放。base 在 #216 合入后又长出 G5 `SLICE_HZ_OVR`/`slice_cycles_for_hz` 臂——重放保留 base 增量、不回退 G5 面。

## 交付面

- `berry-tickface` 新 crate（455 行）：`ns_reciprocal_for` tick→ns 倒数乘-移对预计算 const fn + `ns_fast_path`/`ns_slow_path`/`ticks_to_ns` 快慢双路资格判与合流 + `arm_slice_clamp` S-6 deadline 单调夹取律 + `slice_budget` 预算再装数学 + `vct_delta` LAST_VCT 回绕窗增量 + 常量族（SLICE_HZ/NS_FAST_DOMAIN_TICKS/NS_MUL_CAP）。
- `arch/aarch64/src/timer.rs` 机械壳 re-export：寄存器臂 mrs/msr、原子 static、`static mut` 留壳；编译期 const 钉板随迁单源侧；G5 覆盖臂保留（init 换算走 `slice_cycles_for_hz`，缺省 `SLICE_HZ` 单源自 tickface）。
- `sched/src/class.rs` tick 记账 delta 换单源（`hal::timer::vct_delta`）；`sched/Cargo.toml` 接线。

## 测试面（host +24）

ns_reciprocal_for 三频锚（QEMU 62.5MHz (2048,7) 二进精准/24MHz (2667,6) 半 ulp/异常频率 (0,0) 域卫/<244_171Hz 快路弃/244_171 恰过 mul=4095）+ 快慢双路全域逐位一致（密集 0..4096 + 对数抽样至域界 2^52−1 + 域界恰失格）+ S-6 夹取三点几何 + slice_budget 截尾律 + vct_delta 回绕窗（含 u64::MAX→0 回绕点）。

## 验证

隔离 CARGO_TARGET_DIR 实跑 `just test-host` 163 组 cargo **1532 全绿 EXIT=0**（= base 098ca8bf8 系 1508 + tickface 24）；涉动 crate no_std 构建绿（berry-hal-aarch64/berry-sched）；docs-guard PASS / abi-check PASS / unsafe 台账随源重产 994 对拍绿（timer.rs 12 条 MMIO 留壳行号随迁）/ wg --selftest 0 面红 / check-docs 204 篇 0 警。

## Related

- 沿革 [阶段沿革.md](../../../docs/阶段沿革.md)（二百五十九）
- [2026-11-13-scheduler-95-rounds-knowledge-transfer.md](2026-11-13-scheduler-95-rounds-knowledge-transfer.md)（重放纪律判例）
