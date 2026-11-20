# GUI P3 Click-to-Focus 生产接线

Status: implemented — 2026-11-08

## Problem

P2 Keyboard Input 已按 `table.focus` 投递 KeyQueue，但焦点仍只由 CREATE/SHOW/DESTROY/HIDE 推导。用户点击另一个窗口后，键盘仍会投给旧焦点窗；若让 app 或 inputd 自行记录“当前窗口”，又会出现第二焦点权威。

## Decision

1. 焦点唯一权威保持为 gui-wmd `WmTable`。pointer DOWN 的 `hit_route` 命中真实可见 APP 层窗口时调用 `focus_from_pointer` 接焦并 `bump()` 快照。
2. desktop miss 不清焦：launcher 桌面是 surface_id=0，不是 WM 窗口；清焦会制造 window=0 假焦点。SYSTEM 层窗口也不抢焦。
3. 点击已焦点窗为 no-op：不改 focus、不 bump、不打 `WMFOCUS`，防快照/标记风暴。
4. input lock 与 focus 正交：lock 只保证 DOWN→MOVE→UP 指针粘滞，焦点只作为 Keyboard/窗口策略权威。
5. `WMFOCUS ok sid=... window=... v=...` 携带递增表版本，GUI8 必须等该 marker 后才注入 KEY_A，杀掉 key-before-click 收割竞态。
6. svcmgr 把 gui-inputd 提前到 gfxdemo 前。gfxdemo 两个真实半屏窗口声明 Retained+Keyboard；GUI8 点击左窗+KEY_A、右窗+KEY_A，guest 只在同侧 pointer DOWN + 同侧 key pair 均到达后打印证明。
7. 无 QMP 交互的 Boot B/C 显式 `WMFOCUSSKIP reason=no-pointer`，30s 有界等待后跳过，不伪造焦点证明。

## Alternatives considered

- **app/inputd 维护点击焦点**：制造第二焦点权威，破坏 wmd 窗口表和 Keyboard 路由；否决。
- **desktop miss 清焦**：桌面是 surface_id=0 非 WM 窗口，清焦会让键盘无稳定落点且引入 window=0 假语义；否决。
- **每次 pointer DOWN 都 bump**：同窗重复点击造成 TABLE_WAIT 推送风暴；否决。
- **同刀 click-to-raise**：焦点与层序是两个策略面，先不扩张；否决。
- **让 gfxdemo 常驻等输入**：svcmgr 阻塞 reap 会拖死 launcher 装配；改为有界等待 + 无交互显式 skip；否决常驻。

## Consequences

- 点击窗口后 P2 KeyQueue 立即跟随新焦点；Keyboard 不再投给旧窗口。
- CREATE/SHOW/HIDE/DESTROY/owner-death 的既有焦点策略不变。
- z 序仍只决定 hit-test winner；raise/MRU focus 栈归后续窗口策略刀。
- gui-wmd host harness 仍缺，P3 由 GUI8 双窗真实 QMP 链覆盖。

## Related

- [BerrySession-P3ClickToFocus生产接线-v1.0.md](../../../docs/BerrySession-P3ClickToFocus生产接线-v1.0.md)
- [GUI P2 Keyboard Input 生产接线](2026-11-08-gui-p2-keyboard-input.md)
