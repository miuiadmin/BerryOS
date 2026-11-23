# gui-inputd `normalize_axis` 乘法溢出——中间量 i64 改 u64 并补等价性测试

Status: implemented

## Problem

`drivers/bins/gui-inputd/src/lib.rs` 的 `normalize_axis(value, cal_min, cal_max, screen_len) -> Option<u32>` 把设备自述 ABS 域 `[cal_min, cal_max]` 归一到屏幕像素 `[0, screen_len-1]`。修前中间量走 i64：`(v - cal_min) as i64 * (screen_len as i64 - 1)`。钳位后 `v - cal_min ≤ cal_max - cal_min ≤ 2^32-1`、`screen_len - 1 ≤ 2^32-2`，三因子全在 u32 域，但乘积上限 `(2^32-1)(2^32-2) ≈ 1.8e19` 超过 `i64::MAX ≈ 9.2e18`——极端三元组（`i32::MIN`/`i32::MAX`/`u32::MAX`）下 debug 构建在 `lib.rs:164` 溢出 panic，release 构建静默回绕：实测 `value=i32::MAX-1, cal_min=i32::MIN, cal_max=i32::MAX, screen_len=u32::MAX` 回绕出 `4294967293`（正确值 `4294967294`，差 1）。真实路径到不了：生产面 `screen_len` 来自 `u32::from(snap.disp_w)`（`main.rs:944-946`，u16）或常量 800/600，可达域乘积上限 `(2^32-1)·65535 ≈ 2.8e14`——所以这是防御面缺陷而非现网故障；但函数签名收任意 `u32`，纯函数面就应当对全域正确。模块头注与函数文档原写「i64 定点」，与缺陷同源。

## Decision

生产面两行改动（`lib.rs:164-165`）：`num = (v as i64 - cal_min as i64) as u64 * (screen_len as u64 - 1)`、`den = (cal_max as i64 - cal_min as i64) as u64`——减法仍在 i64 做（钳位后结果非负且 ≤ 2^32-1，不溢出），随即转 u64 再乘。u64 乘积上限 `(2^32-1)(2^32-2) < 2^64` 恒不溢出；三因子非负 → u64 截断除 = floor = 旧 i64 除，所以**在旧式不溢出的入参域内结果逐位相同**（含 None 面：`cal_max <= cal_min || screen_len == 0` 守卫、`clamp` 三行、返回式 `Some((num / den) as u32)` 逐字未动；`screen_len ≥ 1` 保证 `len-1` 不下溢，除数 `den ≥ 1`）。模块头注 `lib.rs:9` 与函数文档 `lib.rs:149-158` 的「i64 定点」改「u64 定点」并补溢出论证——一词之改只是对本函数描述的同步。

回归测试三条（`lib.rs:1185-1290`，零外部 crate：i128 参考实现 `normalize_axis_ref` 是旧 i64 公式原形放宽到 i128，与被测函数只共享守卫与钳位；随机源自手写 LCG，Numerical Recipes 64 位常数取高 32 位）：

| 测试 | 行 | 规模 | 钉板 |
|---|---|---|---|
| `normalize_axis_extreme_triple_no_wrap` | 1186 | 端点 `MIN→0`、`MAX→4294967294`、`MAX-1→4294967293`，中点 `-1→2147483646`、`0→2147483647` | 手算期望值；并用 `wrapping_mul` 反证旧 i64 式在该入参下确实回绕出 `…293` |
| `normalize_axis_equivalent_to_i128_reference` | 1218 | 20 万组随机（i32 边界值 1/4 概率、屏宽 1/4 边界 / 1/4 可达域 / 1/2 全随机） | 与 i128 参考逐位相同（含 None 面，自证 None 样本 > 1 万） |
| `normalize_axis_reachable_domain_spot_check` | 1252 | `screen_len ≤ 65536` 的 10 档 × 5 个校准域 | 对拍 i128 参考、恒在 `[0, len-1]`、端点钉板、越域钳位、`{0,32767}` 全域单调扫描 |

## Alternatives considered

i128 中间量（弃——aarch64 无原生 128 位乘除，i128 会拉入 compiler-builtins 软例程，musl no_std 面多一层依赖；u64 在 aarch64 是原生 `mul`/`udiv`，与旧 i64 同级，只在测试侧用 i128 做参考）；提前窄化为 u32 乘（弃——乘积上限 1.8e19 本身就超 u32，是比旧式更糟的回绕面）；四舍五入（弃——改变全部旧式结果，不是等价修法）；去掉钳位靠入参自律（弃——`clamp` 是 API 规约的一部分，越域入参必须钉到端点）；把 `screen_len` 收窄为 u16 从结构上消灭溢出面（弃——签名是 `docs/GUI-4实施设计草案-v1.0.md` 冻结面，改签名波及 `main.rs` 两处调用与文档 §6-I3，与本刀「等价修法」口径不符）。

## Consequences

测试（`drivers/` 下，`RUSTFLAGS=""`，隔离 target，`--target aarch64-apple-darwin`）：release `48 passed; 0 failed`（45 现有 + 3 新增）；debug（`just test-host` 形态，溢出检查开）`48 passed; 0 failed`。旧码 + 新测反证非空洞：`46 passed; 2 failed`——`extreme_triple` 得 `left: Some(4294967293)  right: Some(4294967294)`，`equivalent_to_i128_reference` 随机命中 `value=2147483647 cal_min=-1459355975 cal_max=1546539774 screen_len=3312758349 → left: Some(1470871404) right: Some(3312758348)`；第三条在旧码下通过——印证可达域本就不溢出、改前改后真实路径行为逐位不变。复审变异矩阵：提前窄化 / 去钳位 → 3 条新测试全红；四舍五入 → 14 败。20 万组中落在旧式乘积溢出区的 1251 组（复审以 Python 复刻 LCG 独立复算，首处失配样本与 Rust 实跑逐字段相同）。

musl `cargo check --release --target aarch64-unknown-linux-musl -p gui-inputd` exit 0，gui-inputd 自身 0 warning（仅 berry-libc 既有 3 条 `user/berry-libc/src/net.rs:248/335/368`，与基线逐条相同）。`tools/facts.txt` 的 `tests_rust` 随本刀 3600 → 3603。

未覆盖面：rustfmt/clippy 不在钉版工具链 1.97.0 上，未跑；端到端不需要（纯函数且 host 测试覆盖，desktop 机 `INPUT ok` 按设计缺席，串口无可观测面）。

## Related

- [驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)——§3.4 健壮性审计后续已登记本刀。
- [GUI-4 实施设计草案](../../../docs/GUI-4实施设计草案-v1.0.md)——`normalize_axis` 签名与端点/单调/None 规约的冻结面（§6-I3）。
- [inputd 在途悬挂强制重发 I3](2026-11-19-inputd在途悬挂强制重发I3.md)——同一文件的上一刀，改的是泵面而非归一化面。
