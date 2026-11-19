# GUI P2 Keyboard Input 生产接线

Status: implemented — 2026-11-08

## Problem

P0 把 Keyboard 定义为 Surface input class，P1 又因无生产者诚实返回 `ST_UNSUPPORTED`。若下一步直接让 app 解析键盘事件或在 inputd 里按窗口转发，会形成第二套输入协议与焦点权威，破坏 Berry Session 的 input class 合约。

## Decision

1. QEMU GUI 拓扑在 tablet 后追加 `virtio-keyboard-device`，保留 tablet 的倒序 virtio-mmio 槽位；QMP key 注入继续省略 `device` 键。
2. gui-inputd 扫描全部 did=18 设备，按 config `EV_BITS` 能力位识别：ABS_X/Y 双在场为 tablet；ABS_X/Y 均缺席且 KEY_A 在场为 keyboard；partial ABS / BTN-only / 未知设备 Unknown 并 park。pointer 原路径零漂。
3. 新增 16B `KeyEvent` wire：timestamp、Linux KEY code、Up/Down/Repeat、修饰位、device id。新增 add-only `0x23` inputd→wmd route 与 `0x24` app→wmd poll。
4. EV_KEY 经零分配 KeyPump 单源转换；BTN_* 仍归 pointer，修饰键状态由 pump 维护，队列满丢新并计数。
5. wmd 新增 per-session KeyQueue。键盘路由只使用权威 `table.focus()` 和该窗 0x67 合约 `INPUT_CLASS_KEYBOARD`；默认 Retained 无显式合约 fail closed。Gamepad/IME Host 继续无生产者或服务，恒 `ST_UNSUPPORTED`。
6. Berry Room 只公开 `poll_keys`，app 不接触 inputd/wmd 内部通道或 virtio wire。
7. GUI8 normal 四段验收：启动无 focus KEY_A 两次 drop；focused Browser 无 Keyboard 合约两次 class drop；UITEST 返回 page0 后协商 Canvas+Keyboard，等待 QMP KEY_A down/up，断言 WMKEYROUTE down/up 各一次与 `UITESTKEY ok code=30 downs=1 ups=1`；UITEST 销毁后再注入两次 drop 且零新增路由。

## Alternatives considered

- **把 KeyEvent 塞进 PointerEvent 判别位**：破坏既有 wire 与 PointerEvent 值域零漂；否决。
- **gui-inputd 直接按窗口路由**：制造第二焦点权威，绕过 wmd table focus 与 Surface input_classes；否决。
- **授予 Gamepad/IME Host**：仍无设备生产者与服务通道，空授能力比拒绝更危险；否决。
- **让 app 直连 inputd**：绕过 Berry Room 与 wmd 生命周期；否决。

## Consequences

- Keyboard 从 P1 的诚实 unsupported 升级为真实生产链，可用于 Retained、Canvas、Immersive，只要对应窗口 0x67 合约声明该 input class。
- 默认未声明 Keyboard 的窗口按键 fail closed，不会误投。
- P2 时点焦点仍是 create/show/destroy 策略焦点；P3 已落地 pointer DOWN 权威接焦。
- Gamepad、IME Host、键盘布局/热插拔、a11y 批量旁车仍未开放。

## Related

- [BerrySession-P2KeyboardInput生产接线-v1.0.md](../../../docs/BerrySession-P2KeyboardInput生产接线-v1.0.md)
- [GUI P1 Surface Mode 生产接线](2026-11-08-gui-p1-surface-mode.md)
