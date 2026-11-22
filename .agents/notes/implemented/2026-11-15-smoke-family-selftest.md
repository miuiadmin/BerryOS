# 独立冒烟族 selftest 挂载：smoke-lockdep / smoke-dpgsoak / soak-g3（W1）

Status: implemented

## Problem

三件独立冒烟关卡脚本（[smoke-lockdep.sh](../../../tools/smoke-lockdep.sh) / [smoke-dpgsoak.sh](../../../tools/smoke-dpgsoak.sh) / [soak-g3.sh](../../../tools/soak-g3.sh)）此前零 `--selftest` 且未挂 test-host——前置拒面（用法/缺件/marker 串直拒/分钟数校验/QEMU 缺件）与相 1 判定引擎（计数恰 1/行序断言）改动坏无机器咬合，属「有守卫零自检」判例族（mkfat/genhdfont 同族，a-3 U4 纪律）。selftest_guard（#214）只扫 `--selftest` 声明与接线，脚本无 selftest 时无从执法。

## Decision

三件各加 `--selftest`（sync-public/bcts-run 合成腿风格：脚本副本入 mktemp 沙箱，ROOT 恒沙箱绝不触真仓）：

- 拒面腿（确定性零 QEMU）：用法红/缺文件红/marker 串缺席红（default 面直拒防烧帽）/dpgsoak rootfs 卷缺席红/soak-g3 分钟数非数字与 1 拒红（平凡无鉴别律）/QEMU 缺件红。
- smoke-lockdep 独有假 QEMU 相 1 判定引擎 5 腿：`QEMU` 环境变量覆盖钩子（soak-g3 先例）+ 假 QEMU 打印 marker 流即退——绿引擎（相 1 PASS）/相 2 前置有序红（沙箱缺 rootfs.cpio loud 快败）/FAIL 标记红/ok 重复红（期望恰 1）/DPG 行序红。
- dpgsoak/soak-g3 不做深腿：dpgsoak 深腿会走到变体卷手术 + 端口让场 + 对端三件套（真绑 18080-18083 与并行 CI 撞口）；拒面腿已盖脚本最外层防线。

接线 test-host（justfile shvarcheck 后，`{{BASH}}` 前缀保 brew/bash3.2 一致）；07 口径行补描述。实跑（b3e21f2b7 谱系）173 组 1586 passed 0 failed；第 15 次 base 重写后净增量重放至新基线 71bb9f358（第 9/10 次 OWNER 裁定净增量自救口径），新基线实跑见下——selftest 非 cargo test 不计入 test result，口径行数字以新基线实测为准。

## 风险与豁免

零 kernel/ABI/依赖面。selftest 腿全在 QEMU 启动前出口，零端口零盘面零真仓副作用。smoke-lockdep `QEMU` env 覆盖默认值不变（真跑仍 qemu-system-aarch64）。
