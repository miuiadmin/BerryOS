# CP-08 闭合: svcmgr D3 帽扩容 + PM 双服务预拉段

Status: implemented

## Problem

svcmgr 平像几何满载阻断 PM 消费端：.bss 终点 0x1dff0 距内核钉位 .pager_entry@0x1E000 仅 0x10（16B），berry-power/berry-thermal 预拉 arm 最小面 ~0x1a0 装不进（链接期位置计数器回退 loud 炸响——fail loud 二道几何钉设计使然）。CP-08 的装配半边（rootfs 条目 + build-user musl 段）两轮前已落，消费半边悬置。

## Decision

扩容走 kernel 面四点同笔：①D3 帽 `USER_CODE_MAX` 128→160KiB（kernel/src/user.rs，elf2flat `--max` 131072→163840 同值执法）；②svcmgr link-user.ld `.pager_entry` 钉位 0x1E000→0x28000、`.pager_stack` 顶 0x20000→0x29000；③kernel/src/main.rs X-3c-1 spawn_user 入口/栈顶同值双改（0x28_000/0x29_000）；④svcmgr 加 PM 双服务预拉段（vnet 后预拉位，spawn 形态照 simbattery 先例：musl std 档 8 页栈 + demand 路 force_eager=false + extras 空/stdin None）。

关键几何取舍：钉位=帽值（0x28000=160KiB）⇒ 平像 total 恰满帽零余量 by design——pager 段再长先撞链接钉（loud 先于帽炸）；代价 = pager 栈自 0x1D10 缩至 0xD10（3344B，pager_main 最大栈对象 512B 表 ×6.5 余量，smoke DPG 族回归实证）。pager 栈顶 0x29000 == 主任务 data_va——与旧几何（栈顶==data_va）同形，非新风险面。

预拉段三裁：失败隔离 = eco_skip 可见降级不阻 boot（vblk 同款）；拉起成功打 `eco: berry-power up`/`eco: berry-thermal up` 观测行（`eco: VER ok` 同族非标记，非 smoke 门）；**不入 ps 账本**——psproto NAME_MAX=8 装不下全名（berry-power=11B）且 PS_MAX=18 GUI 形恰容零余量，账本/通道面归分端执法刀（thr 先例 PS ok n= 零漂移）。

## Alternatives considered

svcmgr 瘦身挤 arm 面（否：上轮已裁——工作量大且脆）；钉位 0x26000 留帽内余量（否：任务书钉 0x28000=160KiB，且零余量=未来 pager 增长先撞 loud 钉符合 fail-loud 设计）；force_eager: true（否：vblk 等 no_std 服务先例虽用 eager，musl 常驻先例 simbattery 走 demand，334KB×2 全幅预拷无谓）；ps_reg 短名入账（否：名缩写失真 + 帽零余量，账本归下一刀）。

## Consequences

实测（扩容后）：.bss 终点 0x1e150（arm 净增 0x160），距新钉位余量 0x9eb0=40624B；平像 163840B 恰满帽；QEMU rootfs 靴 `eco: berry-power up`/`eco: berry-thermal up`/berry-power dry-run 行三证齐。留窗未动：通道端点 gift/ps 账本/真实输入输出面 = 分端执法刀；berry-power/thermal main.rs 窗 1 注与 tools/rootfs-manifest.toml:47-56 留窗注已陈旧（本刀纪律限定文件面未改，归 owner 顺手翻新）。

## Related

[CP-08 PM 接线: test-host 挂行+rootfs 装配](2026-11-15-cp08-pm-wiring.md)（装配半边+阻断首证）；调度彻底计划 CP-08/10 号 §2.5（几何实证单源）；沿革随 owner 提交窗。
