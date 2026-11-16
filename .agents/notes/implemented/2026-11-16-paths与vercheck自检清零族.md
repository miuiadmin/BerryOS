# Agent Note: paths.sh/ver-check selftest 清零族——tools 纯函数零自检件收口（W1）

Status: implemented

## Problem

[paths.sh](../../../tools/paths.sh)（内核产物路径单一解析点）与 [ver-check.sh](../../../tools/ver-check.sh)（版本一致性验收）是 tools/ 下仅剩的两件纯函数零自检件（smoke-smp/smoke-x86 为 QEMU 执行件、docs_guard 为 W2 域避让）。paths.sh 被 justfile KERNEL/KERNEL_RELEASE/RUST_LLD 属性背引号与 smoke.sh `source` 双面消费——路径解析律（target_directory 提取/缺席拒/PATH 自足修补）改坏即全链构建/smoke 断，却零机器咬合。ver-check 是发版门（版本管理标准 §3 G-ver-3/4），此前同样零自检。

## Decision

- 两件各补 `--selftest`（fwcfg-check 判例族形）：合成沙箱钉板，零真仓零网络零真工具链。
- paths.sh 10 腿：合成 cargo/rustc 垫片沙箱（`PATH`/`HOME` 双沙箱化——缺席腿须把 HOME 也指进沙箱，否则 PATH 自足修补 `export PATH="$HOME/.cargo/bin:$PATH"` 会从真 ~/.cargo 拾到真件，拒腿假红）。腿面：target-dir 解析/kernel 两档组装/cargo 垫片哑拒/cargo 缺席拒/rust-lld 解析/rust-lld 缺席拒/rustc 缺席拒/usage 拒/source 形态三函数计数。
- ver-check 8 腿：合成 git 沙箱（脚本拷入沙箱仓 tools/ 再跑——fwcfg-check 同款，脚本头自 `cd $(dirname $0)/..` 到合成仓根）。腿面：G-ver-3 对拍绿/不匹配红 + 点名/无 version 红/git_ident 缺席红/段作用域防串号（version 行在 [workspace.dependencies] 段不得串号——awk 段卫兵的对面纪律）/零 tag 豁免/非 git 豁免。
- 接线 just test-host 双行（fwcfg-check 块后）。selftest_guard 接线门即扫到即执法（本刀元自证）。

## 验证

paths.sh selftest 10/10 绿 + 真环境三解析（kernel/target-dir/rust-lld）回归绿；ver-check selftest 8/8 绿 + 真仓 v0.2.0 对拍回归绿。test-host 全链见 PR body。

## 风险

零 kernel/ABI/依赖面。paths.sh 垫片沙箱不触真 cargo/rustc（PATH 首位垫片）；ver-check 合成仓 git tag 全在 mktemp 沙箱，零真仓写。
