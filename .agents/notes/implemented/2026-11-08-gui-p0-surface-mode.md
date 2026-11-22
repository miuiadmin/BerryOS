# GUI P0 Surface Mode 协议落地

Status: implemented — 2026-11-08

## Problem

F5 已让 `uitest` 成为真实 BerryUI guest。若直接进入 F6 应用迁移，普通 BerryUI、专业 CanvasSurface 与游戏 immersive surface 会在窗口、输入、IME、无障碍、safe area、scale 和帧率语义上各写一套私有约定；BerryUI 也可能被误升为特权渲染器，或反过来迫使专业/游戏路径绕开系统语义旁车。

## Decision

按 ADR #67 落一个 Berry Session Surface Mode 协议，而不是三条 UI 全栈：

1. `berry-guiproto::surface` 新增 WM 域 add-only op `0x67`。请求显式携带 `surface_id`，一个多窗 session 必须逐 surface 协商，不隐式绑定“当前窗”。
2. Role 词表为 `App/System`，Mode 词表为 `Retained/Canvas/Immersive`。三条路径都处于 `App`；`System` v1 仅允许 Retained，app 不得自报 System。
3. host 独占授予 safe area、scale 与合约位。`surface_mode_grant_matches` 要求授予逐字段回显请求的 surface/role/mode/input/IME/a11y；未授权 escape hatch 必须回 1B 错误，不得静默降级为 Retained。
4. 合约位对所有形态强制 `Focus|Rate|Cleanup|A11y`；Canvas/Immersive 额外且仅额外携带 `Escape`。帧率意图继续走 present VOTE，不在 Surface Mode 复制第二份。
5. IME composition 冻结 76B UTF-8 旁车帧，a11y semantic sidecar 冻结 52B 扁平记录帧。a11y 无 `None` 值：Retained 用 Tree，Canvas 用 Regions/Hybrid，Immersive 用 Regions。BerryUI 既有 `ROLE_*` 常量改为转发 BerrySession P0 词表，避免两套角色值漂移。

## Alternatives considered

- 让 BerryUI 成为唯一特权渲染器：会迫使专业文档与游戏同帧 UI 服从 retained tree，违背 GUI-12 与 ADR #67；否决。
- 为普通/专业/游戏各建一套协议：焦点、IME、a11y、safe area、帧率与清理会付三倍税；否决。
- 塞进 present GETINFO 保留位：GETINFO 是能力协商，Surface Mode 是逐 surface 请求/授予，生命周期不同；否决。
- 本刀直接实现 gui-wmd 0x67 分发和 Canvas/Immersive buffer：P0 出口是词表与 host codec，生产 escape buffer 未开；提前接线会宣称未验证的授权面，否决。

## Consequences

- 既有 WM op `0x60..0x66`、`VER_WM=1` 与 44 项旧协议测试零漂移；`0x67` 是 optional add-only 面，当前生产 gui-wmd 未接线。
- P0 新增 8 项 host 测试，覆盖三路径 round-trip、字节 golden、safe area/scale、UTF-8 全类拒面、a11y sidecar、逐 cut 截断与 parse 侧 wild patch。
- CanvasSurface / immersive buffer 分配、gui-wmd 0x67 分发、IME/a11y 旁车传输通道和键盘事件仍属后续刀；任何 escape-hatch 消费者出现前必须先接线并复跑真链。
- F6 Settings 走 Retained，不消费 escape hatch；本协议不扩大 F6 的 UI 范围。
- 规格见 [BerrySession-P0SurfaceMode协议-v1.0](../../../docs/BerrySession-P0SurfaceMode协议-v1.0.md)。
