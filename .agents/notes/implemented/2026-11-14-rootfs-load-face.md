# Agent Note: rootfs 装载门纯面（berry-rootface 落件）

Status: implemented

## Problem

kernel/src/main.rs 的 rootfs 装载链有四组纯判定门此前内联于 kernel crate（host 测不可达），而它们是 boot 期「无 rootfs 降级存活」纪律（§A136 族 no-panic）的语义单源：错一门 = 要么放过违约产链形态（下游 cpio walk 才炸——语义错层），要么误拒合法镜像（显示面静默降级同族）。四门 = ①fwcfg 多卷发现门（空卷拒/单卷帽/Σ 溢出守卫/总量帽）；②v0 init_blob 域门（零拒/帽拒/RAM 窗含回卷）；③isegs Σ 帽；④拼装对账门。

## Decision

新建 berry-rootface（零依赖 face crate）单源 `MAX_VOL`/`TOTAL_MAX` 常量 + `vol_admit`/`blob_span_ok`/`segs_total_step`/`assembly_matches` 四判定门纯函数；kernel main.rs 五消费点换单源（多卷发现段、旧名单单卷域门 `vol_admit(size,0)`、v0 域门、isegs 累加、两路对账），物理臂（fwcfg 目录行走/SeqRead DMA/Vmo 装配/handoff 探测/直映借用）全部留壳。host 9 测钉板：四门全表 + 敌意值域（usize::MAX 域）+ 连续前缀/端到端模型对拍 + 常量冻结锚。

## Alternatives considered

卷数上限 `VOLS` 同迁 rootface（否：kernel 循环消费 `fwcfg::ROOTFS_VOLS` 单源，四判定门零消费——迁移即双写第三源）；`ROOTFS_TOTAL_MAX` 保留 kernel re-export（否：常量已零代码消费，doc 链直指 `berry_rootface::TOTAL_MAX` 即真链）；旧名单单卷域门保持内联（否：`vol_admit(size, 0)` 与 `size==0 || size>MAX_VOL` 严格等价——Σ=0 无溢出/越帽面，同律即同源）。

## Related

[timer slack face](./2026-11-13-timer-slack-face.md)（同 face-crate 纪律先例）；消费面 = kernel main.rs `fwcfg_rootfs_multivol` 头注（A1-3b/A1-2 帽分家口径）与 `rust_main` rootfs 装载段。
