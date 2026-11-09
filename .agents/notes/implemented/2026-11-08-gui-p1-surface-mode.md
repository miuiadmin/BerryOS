# GUI P1 Surface Mode 生产接线

Status: implemented — 2026-11-08

## Problem

P0 已冻结 Surface Mode wire，但生产 `gui-wmd` 对 `0x67` 仍落入 `ST_BAD`；授权真值源、Canvas/Immersive buffer 借用与 escape hatch 消费链均不存在。若专业或游戏 renderer 在此状态下出现，只能自报形态、私接 WM/VMO/stride，或把 BerryUI 误升为唯一特权渲染器。

## Decision

1. 授权真值源进 manifest：`[capabilities] surface_escape=true`，必须伴随 `wm=true`；`berry-manifest` 与 `mkapp` 双侧执法，缺省旧包零漂。
2. svcmgr 生成 per-boot 随机 seed，在任何 app broker 端暴露前用内部 add-only `0x68` 帧预共享给 gui-wmd 并等待 ST_OK；授权 app 收到预载该 seed 的 channel，Berry Room 在 `WM_CONNECT` 随行第 4 handle 迁给 gui-wmd，gui-wmd 恰读一次并关闭。公开常量 token 被删除；gfxdemo 用历史 `BSESCAPE` 自铸 channel 的攻击负例必须得到 `ST_DENIED`。不用自报 `client_type`，不新增第二 broker，不抬 `CHAN_HND_MAX` / wait_many 帽。
3. gui-wmd 在 session 专用臂处理 `OP_WM_SURFACE_MODE=0x67`：先验 owner 可见窗口，再执行纯策略；无有效 seed 的 Canvas/Immersive 恒 `ST_DENIED`，P1 时点 Keyboard/Gamepad/IME Host 因无生产者或服务恒 `ST_UNSUPPORTED`（P2 起 Keyboard 真链开放），授予合约位/safe/scale 按 P0 精确生成。
4. Surface 合约状态挂 `WmSession` 并按窗口记录；`WM_DESTROY` 清单窗，Process 死亡随 session 全清，不建第二全局窗口表。
5. Berry Room 暴露安全子集 `SurfaceModeKind/SurfaceRequest/SurfaceGrant/configure_surface/surface_frame_mut`。VMO、映射、stride 与生命周期留在 Room；只有 Canvas/Immersive 授予后才能借窗口 buffer。
6. gui-boundary-guard 引入显式 allowlist；当前仅 UITEST 可声明 `surface_escape`，其他 app fail closed，负样本扩到 21 类。
7. UITEST 作为真链验收车：普通 APP 拒面由 gfxdemo 证明；UITEST 先测 Keyboard 不支持，再完成 Canvas 与 Immersive 授权、buffer 借写、真实 submit/callback/release，并输出唯一 `UITESTSURFACE` 标记给 GUI8 精确断言。

## Alternatives considered

- **新增 `WM_CLIENT_SURFACE_APP` 自报类型**：任何恶意/误用 app 都能自报，escape 不是能力；否决。
- **gui-wmd 增第二 escape broker 面**：需要服务端第 5 gift 并穿 `CHAN_HND_MAX=8` 与 wait_many 16 帽，授权刀被迫扩张内核/执行链；预载 token 达成同一能力语义且零帽漂移，否决。
- **本轮开放 Keyboard/Gamepad/IME Host 词表**：inputd 无键盘/手柄生产者，IME 无推送服务；授予假能力比拒绝对抗更危险，否决。
- **为 Canvas 新建共享 composition buffer**：既有 app 私有窗口 VMO + wmd 服务端读别名已满足安全与呈现，zero-copy 留后续性能刀，否决。

## Consequences

- P0 三路径中的 Canvas 与 Immersive 从“协议词表”升级为“授权真链”；普通 BerryUI app 仍不得自绘或私接系统机制。
- P1 后边界：`IME Host`、Gamepad、a11y 批量传输、safe area 真实系统层、多 surface Room 与局部 damage 仍显式未开放。
- 第一个真实专业/游戏 renderer 必须逐 app 扩 manifest 与 gui-guard allowlist，并用同样的 per-boot seed / 0x67 / Room buffer 链验收。

## Related

- [GUI P0 Surface Mode 协议落地](2026-11-08-gui-p0-surface-mode.md)
- [BerrySession-P1SurfaceMode生产接线-v1.0.md](../../../docs/BerrySession-P1SurfaceMode生产接线-v1.0.md)
