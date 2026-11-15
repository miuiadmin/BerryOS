# 弹簧刀验收报告（SPRING-VERIFY）

验收对象 = 提交 `16f335a26`（feat(ui/弹簧刀): 定点弹簧解算+SwipePager 初速度接入——中断恢复主干（UI-02 维三）），实施清单见 [SPRING-IMPL.md](SPRING-IMPL.md)。验收员与实施者非同一会话，全程独立复跑：测试用全新隔离 target（`.target-v`，与实施者 `.target-s` 零缓存复用），物理抽查为手算逐位对拍。结论：**五步全过、全绿收讫、零修改**（步骤⑤无红可修）。

Updated: 2026-11-15

## ①纪律核

四项逐一在案：

- **Q16.16 纯整数**：对 `git diff a2f9ff888` 全部新增行（剥注释行后）grep `[0-9]+\.[0-9]|f32|f64|float|sqrt|powf`——代码面零命中；唯一原始命中为 `dt_q` 行尾注释「Q16.16 秒」与 doc 文本（ζ=1.0 等参数描述），均非字面量。`anim.rs` 对基线**纯增量 245 行 0 删改**（numstat 实证），存量 `interp`/`Easing`/`Anim` 结构逐字保持。
- **零分配热路径**：新增 src 行 grep `Box|Vec<|vec!|format!|String|collect|HashMap|clone()` 零命中（唯一「Box」命中为节首注释里说明禁用理由）。`SpringProp` 全定长字段（i64 积分态 + u64 时戳 + bool），`spring_tick` 逐槽步进无堆面。复核 guest 面：`cargo build -p berry-ui --target aarch64-unknown-none`（no_std）在验收 target 下编译通过——std/alloc 未潜入的编译面证据。
- **AnimProp add-only**：`AnimProp` 枚举基线↔当前逐字相同（OffsetX/OffsetY/Width/Height/Gap 五属性零扩），「加 Spring」落在挂载面（`SpringProp.prop` 指认五属性 + lib.rs `springs` 第二表）。TLV golden 复跑零漂移：reconcile 12 + tlv2..tlv7 共 39 + gallery2 像素哈希 2 全绿（anim.rs 起动画零删除故 wire 面结构性无改，golden 为经验加固证据）。
- **cells.tsv 漂移独立复核**：验收全量跑后 `WAVE2-GALLERY/cells.tsv` 的 kind_input 行哈希 f6e0b83b→a9de573f 漂移。开基线临时 worktree（纯 a2f9ff888、零弹簧改动）跑同一 `gallery2_dump_cells`：产出**完全相同**的 a9de573f，与本刀改动后产出逐字节一致——漂移为本机 dump 副作用且本刀渲染零漂移的直接证据。已 `git checkout` 还原，未夹带进提交。
- **swipe 默认路径断言在案**：`snap_default_quadout_zero_change`（tests/swipe.rs）——非零速度（−600px/s）松手仍 QuadOut：起拍持 from（−30）、中点 −83 值级钉板（QuadOut t=0.5 手算 −70×49152>>16 = −53 → −83）、240ms 终点精确 −100、到期即毕再 tick 零开销。默认档行为零变化有值级断言非仅通过性断言。

## ②测试全绿（隔离复跑）

`cargo test -p berry-ui --target aarch64-apple-darwin`，`CARGO_TARGET_DIR=.target-v`（验收专用全新 target，与实施者 `.target-s` 互不相干）：

- **328 测全绿 0 红**（unittests 61 + 33 个集成测试文件 267 + doctest 0），与实施清单声称一致；基线 316 + 新 12（spring.rs 9 + swipe.rs 3）。
- golden 轨明细全绿：reconcile 12、tlv2 12、tlv3 6、tlv4 7、tlv5 7、tlv6 7、tlv7 7、gallery2 2（像素哈希）、offscreen 11、widgets 系 38。
- 编译警告 6 条全部位于存量测试文件（widgets3/widgets5/widgets6/inject4 的 unused import/variable），新代码零警告。

## ③物理正确性抽查（手算前 3 步 × 实现对拍）

**递推式（半隐式 Euler，Q16.16 全整数）**：`k_q = 144×65536 = 9_437_184`，`c_q = 24×65536 = 1_572_864`（Smooth 档，m=1）；`dt_q = (16_000_000×65536)/1_000_000_000 = 1048`（整除截断，有效子步 1048/65536 s ≈ 0.0159912 s，较名义 16ms 短 0.055%）。关键化简：k_q、c_q 均为 65536 整倍数，故 `(k_q·disp)>>16 = 144·disp`、`(c_q·v)>>16 = 24·v` **精确无舍入**——每子步仅两处 `>>16` 舍入（乘 dt_q 处），负数侧为算术右移（向 −∞ 取整）。`target_q = 100×65536 = 6_553_600`。

**例 A：Smooth，from=0，target=100，v0=0**（位移 px 与速度 px/s，括号内为 Q16.16 原值/换算）：

| 步 | disp = pos−target_q | a = −144·disp − 24·v | v′ = v+((a·dt_q)>>16) | pos′ = pos+((v′·dt_q)>>16) | value() | vel_px_s() |
|---|---|---|---|---|---|---|
| 1 | −6_553_600 | +943_718_400（=14_400 px/s²） | 15_091_200（230.2734） | 241_326（3.6839） | 4 | 230 |
| 2 | −6_312_274 | 546_778_656（8_343.18） | 23_834_852（363.69） | 622_474（9.4983） | 9 | 364 |
| 3 | −5_931_126 | 282_045_696（4_303.18） | 28_345_104（432.51） | 1_075_746（16.412） | 16 | 433 |

解析对拍（理想实数半隐式 Euler、dt 精确 0.016s）：x₁/x₂/x₃ = 3.6864/9.5086/16.43，实现 3.6839/9.4983/16.412——偏差 ≤0.11%，唯一来源即 dt_q 截断的每步 −0.055%。

**例 B：Smooth，from=0，target=100，v0=600**（600 = 精确终端速度：144×100 = 24×600 → 首步加速度**精确为零**——物理不变量，实现逐位成立）：

| 步 | disp | a | v′ | pos′ | value() | vel_px_s() |
|---|---|---|---|---|---|---|
| 1 | −6_553_600 | 0（弹簧拉力=阻尼阻力） | 39_321_600（600.0000） | 628_800（9.5947） | 10 | 600 |
| 2 | −5_924_800 | −90_547_200（−1_381.69） | 37_873_744（577.907） | 1_234_447（18.838） | 19 | 578 |
| 3 | −5_319_153 | −143_011_824（−2_182.16） | 35_586_811（543.01） | 1_803_523（27.517） | 28 | 543 |

解析对拍：x₁/x₂/x₃ = 9.6/18.846/27.53，实现 9.5947/18.838/27.517 ✓。

**例 C：Bouncy（k=256、c=20），from=0，target=100，v0=0**：同法手算得 value() = 7/17/30、vel_px_s() = 409/661/789（首步 a = 256×100 = 25_600 px/s²、v₁ = 409.375 px/s 精确）；且 dt·ω = 0.0159912×16 = 0.256 ≪ 2（半隐式 Euler 稳定域内 8 倍裕量）。

**对拍方法**：临时探针测试（`tests/spring_verify_probe.rs`，pub 面逐位断言上表全部 value()/vel_px_s() 值）三测**首跑全过**——实现与验收员独立手推逐位一致；探针用毕即删未入库（仓库无残留，`git status` 实证）。

## ④中断恢复语义验证用例

在案且全绿：`retarget_continues_from_current_state`（tests/spring.rs）断言四层——重定目标前后 `value()` 与 `vel_px_s()` **逐位不变**（位置/速度无双跳）、续步位移同向且有界（0 < Δ ≤ 20px，速度续接非重启）、新终点精确到位（200）、折返场景（目标跨到身后，速度先被吃掉再反向，终点 0 到位）；辅以 `initial_velocity_points_motion`（起手在目标位带初速，首步位移方向 = 初速度方向，带速不即时收敛）。SwipePager 面由 `snap_velocity_spring_optin_monotonic_converges`（松手速度 −600 直入弹簧、单调收敛、双阈值吸附）与 `snap_velocity_spring_zero_velocity_falls_back`（零速/Cancel 回落 QuadOut）覆盖挂接语义。

## ⑤红则修

无红：①—④全过，验收全程对实施代码**零修改**（diff 仅验收产物本报告）。

## 提交

- 实施提交：`16f335a26`（6 文件，+814/−33：anim.rs/lib.rs/swipe.rs 源三件 + tests/swipe.rs + tests/spring.rs 新 + SPRING-IMPL.md）。
- 本报告：验收产物，随后以显式路径单独提交（docs 提交，哈希见 git log）。
