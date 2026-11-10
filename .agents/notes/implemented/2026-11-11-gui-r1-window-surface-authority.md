# GUI R1 Window/Surface Authority Hardening

Status: implemented — 2026-11-11

## Problem

GUI-14 二轮审计发现：P0/P1/P4 虽然建立了 Surface 合约、escape seed 和窗口焦点权威，但系统身份与绘制权威仍有高置信漏洞。`WM_CLIENT_SYSTEM` 可自报，APP 可创建 SYSTEM layer，任意 session 可提交 `surface_id=0` 全局画面，damage 只按全屏 clamp 可越窗绘制，parent 可跨 owner，SYSTEM window 在 CREATE/SHOW/refocus 路径会抢焦点。若在此之上做 safe inset、IME 或 system UI，会把权限洞产品化。

## Decision

1. 新增 per-boot SYSTEM seed：svcmgr 通过 add-only `0x69` 控制帧预共享给 wmd，并以 hex argv 只交给 berry-launcher；launcher 再铸一次性 channel 随 `WM_CONNECT(SYSTEM)` 附上。SYSTEM 身份不能由 client_type 自报。
2. `WM_CONNECT` 第 4 handle 按 client_type 分流：APP 只认 escape seed；SYSTEM 只认 system seed；二者不可互换。
3. 新增 `berry-guiproto::authority` host 纯策略模块，wmd 消费同一单源：APP layer / SYSTEM layer 权限、同 owner parent、AVOID_SYS flags、SYSTEM 不抢策略焦点、damage 本地裁剪。
4. `surface_mode_session_policy` 按 client_type 授予 role：APP 只能 App role，SYSTEM 只能 System role；System v1 仅 Retained 且无 escape；Gamepad/IME Host 继续诚实拒绝。
5. `surface_id=0` 只允许 SYSTEM session 提交；APP 全局面恒 `ST_DENIED`。
6. damage 先与本 surface 本地矩形求交；部分越界裁剪，完全越界丢弃；整批裁空 `ST_BAD`，不得降级为零 damage 提交。
7. CREATE/SHOW/refocus 只让 APP layer 接焦；SYSTEM overlay 不得抢占 Keyboard focus。
8. gfxdemo/GUI8 加真实正反例：SYSTEM seed 正例、自报 SYSTEM 拒绝、错 SYSTEM seed 拒绝、client 晚到 0x69 拒绝、APP 自报 System role 拒绝、APP SYSTEM layer 拒绝、跨 owner parent 拒绝、APP 全局 surface 拒绝、越窗 damage 拒绝。
9. seed 生成用独立时间样本、escape seed、不同 label 与 splitmix64 形 finalizer，降低同源相关性；post-seed ST_FULL 路径只回收前三柄，避免已关 seed channel 二次 close。

## Alternatives considered

- **只检查 client_type**：client_type 是请求帧自报值，任意 APP 可伪造；否决。
- **复用 P1 escape seed 作为 SYSTEM token**：Canvas escape 特权与系统身份互换；否决。
- **为 SYSTEM session 增加 env handle**：当前 env gift 已 8/8 满，需要跨 kernel/exec/libc 抬帽；本刀用 argv + connect channel 零 handle 帽漂移。
- **damage 继续按全屏 clamp**：oversized VMO 可画进邻窗；否决。
- **同刀修 AVOID_SYS buffer 合约**：需要先裁决 raw bounds+insets 与 shrink bounds 的 occlusion model；归 R2，避免把错位合约固化。

## Consequences

- SYSTEM 身份、系统层创建、全局桌面 surface 与 damage 越窗四类权威洞关闭。
- SYSTEM overlay 不再因 CREATE/SHOW/refocus 抢键盘焦点。
- `surface_mode_production_policy` 保持 APP 兼容入口，P0/P1 旧测试零漂。
- AVOID_SYS effective geometry 与 Room buffer 合约错位仍未修，归 R2。
- SYSTEM session 字形供给、System role 生产消费者、safe inset 与 IME Host 归后续刀。
- SYSTEM seed 熵源仍需真随机/失败锁定；当前优于自报但不是长期熵方案。
- SYSTEM seed 以 hex argv 留在 launcher 进程生命期；未来 argv/proc/crash dump 面开放前必须改为一次性 handle gift。

## Related

- [BerrySession-R1WindowSurfaceAuthorityHardening-v1.0.md](../../../docs/BerrySession-R1WindowSurfaceAuthorityHardening-v1.0.md)
- [GUI-14 多形态 UI 二轮对抗深研](../../../research/GUI-14-多形态UI二轮对抗深研-2026-11.md)
