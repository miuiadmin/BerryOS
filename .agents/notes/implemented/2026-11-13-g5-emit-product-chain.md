# Agent Note: G5 工具半收口——berry_assemble --emit 最小产件面与产品链端到端闭合

Status: implemented

## Problem

G5 投放面长期只出计划不出件：`[platform.boot.params]` 非空只在 --dry-run 计划里给 `etc/boot-params.toml` 条目行，42 号复核内核消费端（七靴 QEMU）用的是 mkcpio 位置参数手工产链的验证件——产品链「最后一毫米」（产品清单 → 实件 → rootfs → 镜像 → 内核读出）没有一次实件贯通，42 号 risks 挂账「产品链接线后需一次端到端复验」。

## Decision

`berry_assemble.py --emit <product.toml> --out-dir <dir>` = 最小产件面：复走全量复验（装配反回滚门 ⊕ 结构/产物面）后实写 `<dir>/etc/boot-params.toml`，序列化单点 `serialize_boot_params()`——`schema = 1` 头 + 键值对、键序字典序、u32 裸数字 / intent 带引号字面量，与内核消费端 `kernel/src/bootparams.rs` `parse` 的 TOML 子集合同对读；空 params 零产件零目录（缺省零扰动合同的工具面配套）。mkcpio 消费形态由产件行明示：`etc/boot-params.toml=<abs>` 整条 name=path 并入条目集——与 override/trust 条目同形（mkcpio CLI 现役语义，零第二映射）；locale 等余投放件不预产（读者未在册，归全产线步⑥）。

## Alternatives considered

- **等 V13 镜像全产线步⑥一起做实件**：被拒——42 号 risks 要求链接后即复验，且步⑥ 无排期；最小出口先把贯通验掉，全产线到时只换编排不换形。
- **--emit 直接产整份 rootfs 条目表或 cpio**：被拒——越权（镜像全产线归 V13，just 配方归主会话），最小刀只补 boot-params 一件。
- **只文档明示 mkcpio 消费形态不产件**：被拒——复验仍要手工拼件，序列化逻辑活在文档里等于第二实现漂移面。

## Consequences

- 产品链实件贯通（/tmp/drill3 双靴，AAVMF 挂盘自举、零 -kernel / 零 fw_cfg）：处置组（`sched_slice_hz = 250`）`ROOTFS ok src=handoff bytes=34074624`（= mkcpio 档字节恒等）紧随 `bootparams slice_hz=250 slice=250000`；对照组（同链去 params）全量日志 `bootparams` 行数 = 0——默认零扰动在整盘链上再证。
- 顺带补上 42 号 §5 观察项：INITRD/Borrowed 借片形 rootfs 的 QEMU 首靴活证（此前只有 Owned/Assembled 两路）。
- CPIO_PAD 消费点「四点 → 五点」字样三处陈旧（42 号 F5）由本窗并行轮对齐（`tools/mkcpio.py` / `tools/mkapp.py` / `kernel/src/bootparams.rs`）。

## Related

- 内核半决策：[2026-11-12-g5-bootparams-kernel-consumer.md](./2026-11-12-g5-bootparams-kernel-consumer.md)
- 实施面：`tools/berry_assemble.py`（BOOT_PARAMS_ENTRY / serialize_boot_params / do_emit + selftest G5 腿）；基线 `research/厂商定制化深研/38-OEM演练实弹报告.md` §4 G5 行、`42-实施轮十六复核.md` §1/§5。
