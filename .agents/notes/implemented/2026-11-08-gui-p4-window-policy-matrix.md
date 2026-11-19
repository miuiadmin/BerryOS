# GUI P4 Window Policy Matrix 生产接线

Status: implemented — 2026-11-08

## Problem

P3 已把 pointer DOWN 接焦和 Keyboard 路由统一到 gui-wmd `WmTable`，但窗口策略还缺少可回归矩阵：desktop miss 是否清焦、HIDE/SHOW 后焦点是否残留、重叠 z-order 是否按命中者接焦、pointer lock 目标窗消失后 UP 如何收尾，均没有真实 QMP 证明。若为了测试在 app/inputd 侧记录焦点或层序，会形成第二真相。

## Decision

1. P4 零 wire 变更、零 gui-wmd 核心变更；全部通过既有 `WM_CREATE / WM_SET / WM_DESTROY / WM_TABLE_GET / INPUT_POLL / INPUT_KEY_POLL` 驱动和观察。
2. gfxdemo 两个半屏窗先完成 P3 focus proof；只有该交互证明成功才进入 P4 matrix。无输入 Boot B/C/normal 沿 `WMFOCUSSKIP` 直接继续 legacy flow。
3. desktop miss：HIDE 右窗后在原右窗中心点击；guest 断言 TABLE version 与 HIDE 版本相等、两侧 pointer queue 均静默，KEY_A down/up 仍只进左窗 KeyQueue。
4. HIDE/SHOW：HIDE 持焦右窗后断言 focused bit 归左窗；SHOW 后断言右窗接焦，版本严格推进。
5. z-order：右窗移入重叠带。z 平局按表序由左窗命中；右窗 z=1 后同一点由右窗命中。焦点只跟随 hit-test winner，Z SET 本身不改焦点。
6. stale input lock：创建第三个 z=1 临时窗，QMP raw DOWN 后 guest 收到 DOWN 并销毁该窗，随后注入 raw UP。wmd 将 UP 合成 `PHASE_CANCEL + PE_FL_SYNTHESIZED` 投给原锁定 session 并清锁。断言 SYNTHESIZED 位，不要求 flags 精确等于 2。
7. GUI8 对 marker、ready gate、键盘路由数、版本链做精确计数和 backstop；Boot B/C/normal 断言 P4 marker 全零。

## Alternatives considered

- **guest 维护焦点/层序表**：制造第二真相，无法证明 wmd 执法；否决。
- **desktop miss 清焦**：与 P3 裁决冲突，且 desktop surface_id=0 不是 WM 窗；否决。
- **P4 在所有 boot 强行等待输入**：Boot B/C/normal/smoke-image 会被无输入超时拖死；改为挂 P3 success 门。
- **把锁定窗销毁并入 gfxdemo 最终销毁**：会扰动 WMDTY/WMDEAD 既有语义；改用自恢复的第三临时窗。
- **断言 CANCEL flags==2**：RESAMPLED 位可与 SYNTHESIZED OR，正确行为可能 flags=3；只断 bit。

## Consequences

- P3 的四个遗留策略场景全部有真实 QMP 证据。
- hidden window 不参与 hit-test、不残留 Keyboard focus、z 变更不改焦点、stale lock 合成 CANCEL 均不可静默回退。
- HIDE 变体 CANCEL、CANCEL 后 re-lock 正向探针、SYSTEM 层窗口和 MRU fallback 仍留后续窗口策略刀。
- gui-wmd host harness 仍缺；P4 由真实链和 GUI8 反 mock 断言覆盖。

## Related

- [BerrySession-P4WindowPolicyMatrix生产接线-v1.0.md](../../../docs/BerrySession-P4WindowPolicyMatrix生产接线-v1.0.md)
- [GUI P3 Click-to-Focus](2026-11-08-gui-p3-click-to-focus.md)
