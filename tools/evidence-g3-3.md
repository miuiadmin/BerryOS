# G3-3 构建面证据件（f4 F7 修复——durable 证据载体）

> 义务源：G3-3 立项 kill criteria「构建面慢 >3× 降级取放 trace 面」此前只有
> 单发增量口径读数（g3-3b：feature 1.99s vs default 2.48s——增量口径受上次
> 构建脏度摆布、且单发无噪声自持），f4 F7 判「无耐久证据 + 噪声自我反驳」。
> 本件 = clean 口径多跑中位数，落库为耐久证据。

## 方法（2026-11-05 实测）

- 口径：**clean build**——每跑前 `rm -rf` 隔离 target dir（`~/.cargo-target-f7-g33`
  ，不碰共享 `~/.cargo-target`），`cargo build [--features lockdep]` 全量重建。
- 每面 3 跑取中位数（run 1 双面均现首触慢化——见下「噪声注」）。
- 环境同构：双面同 shell/同 sccache 全局 rustc-wrapper（缓存热）、同宿主
  （Darwin 27.2）、构建窗独占（无并发 cargo/QEMU——f4 会话确认零构建）。

## 结果

| 面 | real（s）三跑 | real 中位 | user（s）三跑 | user 中位 |
|---|---|---|---|---|
| default | 2.140 / 1.384 / 1.405 | **1.405** | 3.09 / 3.07 / 3.10 | **3.10** |
| feature `lockdep` | 1.970 / 1.389 / 1.412 | **1.412** | 3.15 / 3.11 / 3.13 | **3.13** |

- **比值**：user 中位 3.13/3.10 = **1.010×**；real 中位 1.412/1.405 = 1.005×
  ——均 ≪ 3× kill 帽（G3-3 构建面判据：慢 >3× 降级取放 trace 面）。**不触发**。
- **噪声注**：双面 run 1 real 均显著慢（2.140s@152%cpu / 1.970s@167%cpu，
  对比后续跑 1.38–1.41s@231–235%cpu）——target dir 首触/页缓存暖机所致，
  与 feature 开关无关；这正是 F7 指认「单发读数自我反驳」的噪声源，中位数吸收。
- **增量口径读数（g3-3b）归档说明**：1.99s vs 2.48s 为单发增量（脏度不定），
  保留为历史记录，判据以本件 clean 中位为准。

## 附带量

- 二进制尺寸（debug profile，debuginfo 主导）：default 4367864B vs feature
  4372024B = **Δ4160B**（g3-3b 时点曾记 Δ8608B——随两面各自演进波动，非单调
  码量指标；default 面 icount 零漂移由 prove/gate 面证，见沿革 G3-3 行）。

## 复现

```sh
export PATH="$HOME/.cargo/bin:$PATH"
TD="$HOME/.cargo-target-f7-g33"
for i in 1 2 3; do rm -rf "$TD"; { time cargo build --target-dir "$TD"; } 2>&1; done
for i in 1 2 3; do rm -rf "$TD"; { time cargo build --features lockdep --target-dir "$TD"; } 2>&1; done
rm -rf "$TD"
```
