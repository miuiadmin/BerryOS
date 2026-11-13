# Agent Note: boot 关键链 eager 回退止血带——hexyl/uu 家族绕过 demand paging

Status: implemented

## Problem

端到端测量（89 轮）：boot 到 bsh 271s，其中 U1 uu 批 120s + M3DEMO 段 60s + hexyl 11.4s ≈ 2/3 全部由 dpg 单页供给吞吐导致（X-5 定谳 ≈6 页/s 吞吐墙——TCG 下每圈 0x23 四发 miss + 发现延迟与扫频耦合，见 `docs/X6批供提速设计-v1.0.md` §0）。批供 0x24 楔死待裁，boot 时长税在裁决落地前每天由 smoke/bench 全额支付。

## Decision

止血带先行：对 svcmgr spawn 链中的 boot 关键短命子开 `SpawnSpec::force_eager`（X-5 常驻服务同款面，berry-exec 侧零改动）——只动 `user/bins/svcmgr/src/main.rs` 两处 spawn 点：

- hexyl walk arm（`bin/hexyl`）：84 页 demand 装载 11.4s → eager 全幅预拷瞬完；
- `uu_run`（U1 批 9 子唯一 spawn 路：mkdir/touch/cp/mv/ls/cat/rm/rmdir）：~200KiB/子。

内存安全论证（128MiB 总预算、<10MiB 约束）：hexyl 瞬态 ≈336KiB 像幅 + 128KiB 栈，终验后即 reap；uu 子 spawn→EOF→reap 严格串行 = 任一时刻至多一子存活（≈330KiB）——止血带瞬态足迹 <1MiB。且 hexyl/uu 改 eager 后不再占 pager 寄存表槽，X-3c-3 让位闸不变式（dpg 窗内无外来在册 demand 窗）单向收紧、槽位/键竞速净减。

关键澄清（分析步骤实证）：M3DEMO 段 uu_cp/df 的 spawn 走 bsh 的 berry-exec 链接副本，bsh 从不调 `set_demand_hooks` → 其 `DEMAND_REG` 恒 None → bsh 一切 spawn 结构性恒 eager（X-3 spec「bsh 面保持 eager」零漂移隔离）。M3DEMO 的 60s 是 TCG fs-RPC 串行链成本（uu_cp spawn→exit ≈17s 定标在案），非 dpg——止血带对它本就无可施力，也无需施力。

验证（QEMU TCG 单核、smoke rootfs 相全接线——18083 hostfwd + SKT client）：boot→BSH ok 271s→51.7s；U1 ok 21.7s、HEXYL ok 21.8s（X6 pg=29——hexyl 不再吃 pager 页）；M3DEMO ok n=5 163.2s；rootfs 相 45 标记全在席（HEXYL/U1/B1 ok n=4/M3DEMO 四门全过）。FAT ok n=5000 @124.3s——残余时长是 fs-RPC/TCG 面，与 dpg 正交。

## Alternatives considered

- 等 0x24 批供裁决落地后一并解决：被否——裁决时程不可控，且 0x24 楔死诊断本身未收口；止血带与 0x24 正交可共存（0x24 落地后本止血带即退场候选）。
- 全部程序 eager（回到 X-5 前）：被否——X-5 的足迹收益主体在短命测试族（dpg 自测族必须 demand：0x21/0x23/0x24 测试本体）；128MiB 下无差别 eager 有 OOM 面。
- mkcpio 预装载/内核 build 参数：force_eager 机制现成在位（X-5 常驻豁免同款），无需新面。

## Consequences

- 止血带覆盖的子不再注册 pager 槽 → PAGER_SUPPLIED/X6 观测行读数下降（hexyl 点 pg≈29 vs 旧 84+），判读 X6 行时须先扣止血带面；dpg 自测族（dpgp/dpgs/dpgn/dpgf/dpgb/dpge/dpgx/x5）保持 demand，X-5/X-6 仪器面零扰动。
- `HEXYL_EOF_CAP` 180s 与 `U1_EOF_CAP` 60s 帽保留不动（帽是上界非时延，eager 下实际 ~1s 即完；回 10s 是 0x24 裁决后随退场刀一并处理的裁量，不在本刀）。
- 退场条件：0x24 批供裁决落地且吞吐墙实证解除后，本止血带两处 `force_eager: true` 回 false（或按裁决形整体退役 demand/eager 分流）。
- 手跑 `just run-rootfs` 验证 rootfs 相尾段标记的已知陷阱（无 18083 hostfwd + SKT client → bsh 卡 SKT 无帽 accept → 尾截断）本次复踩实证：验证须按 smoke.sh rootfs 相全接线（echod 18080 + evsclient 18082 + evsclient 18083 SKT + 三 hostfwd）。

## Related

- [X6批供提速设计](../../../docs/X6批供提速设计-v1.0.md)——吞吐墙定谳与 0x24 主线（本刀是其止血前置）。
- [G4按需调页实施设计草案](../../../docs/G4按需调页实施设计草案-v1.0.md)——X-5 demand 装载与 force_eager 常驻豁免面源头。
- [zfod 内核零页直供](./2026-11-12-zfod-kernel-zero-supply.md)——同链另一刀（零窗往返面），与本刀正交共存。
