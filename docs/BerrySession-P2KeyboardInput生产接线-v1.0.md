# BerrySession P2 Keyboard Input 生产接线

> 版本：**1.0**｜2026-11-08
>
> 定位：P1 Surface Mode 后的第一条真实输入能力刀。P2 把
> `INPUT_CLASS_KEYBOARD` 从协议词表升级为
> `QEMU virtio-keyboard → gui-inputd KeyEvent → gui-wmd focus/input_classes →
> Berry Room poll_keys` 的生产链；Gamepad 与 IME Host 仍无生产者或服务，
> 继续诚实 `ST_UNSUPPORTED`。执行序权威 =
> [GUI框架地基路线-v1.6.md](GUI框架地基路线-v1.6.md)。

## §1 结论

P2 后输入能力状态：

```text
Touch/Stylus/Mouse  既有 PointerEvent 路径
Keyboard             真实 virtio-keyboard 生产链已开放
Gamepad              无生产者，ST_UNSUPPORTED
IME Host             无服务/推送通道，ST_UNSUPPORTED
```

Keyboard 不等于 Canvas escape：

```text
Retained   可声明 Keyboard
Canvas     可声明 Keyboard
Immersive  可声明 Keyboard
```

真正约束在 wmd 路由期：

```text
focused window 存在
该 window 的 0x67 合约 input_classes 含 Keyboard
事件才进入该 session 的 KeyQueue
```

未显式协商 Surface 的默认 Retained 无 Keyboard 合约，按键 fail closed。

## §2 QEMU 设备与注入

GUI 拓扑在 tablet 后追加：

```text
-device virtio-tablet-device,id=tab0
-device virtio-keyboard-device,id=kbd0
```

virtio-mmio 总线倒序建槽：tablet 先建设备槽位保持稳定，keyboard 使用更
低槽位。实测：

```text
INPUTDEV ok tab=30 kb=29 q=64x8B cal=x[0,32767) y[0,32767)
```

QMP 注入使用：

```json
{
  "execute": "input-send-event",
  "arguments": {
    "events": [
      {
        "type": "key",
        "data": {
          "down": true,
          "key": {"type": "number", "data": 30}
        }
      }
    ]
  }
}
```

纪律：

```text
必须省略 device 键
number 30 = Linux evdev KEY_A
down/up 分两批注入
```

`device` 键在 QEMU 中是 console 查找键，不是输入设备选择器；在
`-display none` 拓扑会触发 QEMU QOM abort，继续禁止。

## §3 gui-inputd 多设备认领

P1 前 inputd 要求 input 设备槽恰一个；追加 keyboard 会立即 park。P2 改为：

```text
扫描全部 32 个 virtio-mmio 槽
did=18 的槽逐个读取 config EV_BITS
ABS_X + ABS_Y 同时在场 → Pointer
ABS_X/Y 均缺席且 KEY_A 在场 → Keyboard
partial ABS / 只带 BTN_* / 未识别 → Unknown，不猜测
```

拓扑约束：

```text
tablet 恰 1
keyboard 0..=1
任何 Unknown input 设备 → park
```

识别按设备自述能力位，不绑定 QEMU ID_NAME 字符串。

启动标记：

```text
INPUTDEV ok tab=<slot> kb=<slot|usize::MAX> ...
```

Pointer 原路径零漂：

```text
PointerPump / normalize / PointerEvent / INPUT_ROUTE 0x22
INPUTDROPS ok|bad
WMROUTE / WMRTWIN
```

Keyboard 独立溢出证据：

```text
INPUTKEYDROPS ok n=0
INPUTKEYDROPS bad n=<dropped>
```

## §4 KeyEvent wire

新增 add-only input 域 op：

```text
OP_INPUT_KEY_ROUTE = 0x23   inputd → wmd 通知帧
OP_INPUT_KEY_POLL  = 0x24   app → wmd 拉取帧
```

KeyEvent 定长 16B：

```text
off 0   timestamp_ns u64
off 8   code u16       Linux evdev KEY_*，非 0
off 10  action u8      0=Up 1=Down 2=Repeat
off 11  modifiers u8   Shift|Ctrl|Alt|Super 已知位
off 12  device_id u16  非 0
off 14  reserved u16   恒 0
```

帧帽：

```text
INPUT_KEY_EVENTS_MAX = 15
INPUT_KEY_EVENT_LEN  = 16
INPUT_KEY_ROUTE_MAX  = 244
```

路由帧：

```text
[op][count u8][reserved u16=0][count × KeyEvent]
```

拉取帧：

```text
request  [op][max_events u8][reserved u16=0]
reply    [ST_OK][count u8][reserved u16=0][count × KeyEvent]
empty    1B ST_EAGAIN
```

所有截断、尾随、空批、count 越帽、未知 action/modifier、保留位非零、
device 0、code 0 均拒绝；0x23 路由帧逐事件预验，任一内事件畸形则整帧
拒绝，wmd 不静默跳过。

## §5 KeyPump

gui-inputd 纯逻辑层新增 `KeyPump`：

```text
输入：RawInputEvent(EV_KEY)
输出：berry_guiproto::KeyEvent
```

语义：

```text
KEY_* code < 0x100 进入键盘泵
BTN_* code >= 0x100 仍归 PointerPump，KeyPump 忽略计数
value 0/1/2 → Up/Down/Repeat，其它忽略计数
修饰键状态由泵单源维护
队列帽 64，满则丢新并计数
```

修饰位：

```text
Shift / Ctrl / Alt / Super
左/右同位
Down/Repeat 置位，Up 清位
事件携带更新后的修饰状态
```

host 测试覆盖 down/repeat/up、修饰转换、BTN 忽略、坏 value、队列帽与
drop counter。

## §6 gui-wmd 路由

新增 per-session KeyQueue：

```text
KeyEvent × 16
head/tail/len/dropped
与 PointerQueue 分域，互不挤占
```

路由规则：

```text
1. table.focused() 取当前 focus window 的 owner
2. owner session 必须存在
3. 该 window 的 SurfaceModeInfo.input_classes
   必须含 INPUT_CLASS_KEYBOARD
4. 通过后进入 owner session KeyQueue
```

无 focus：

```text
WMKEYDROP bad sid=0 window=0 reason=focus code=<code>
```

无授权合约：

```text
WMKEYDROP bad sid=<sid> window=<win> reason=class code=<code>
```

成功：

```text
WMKEYROUTE ok sid=<sid> window=<win> code=<code> action=<0|1|2>
```

队列满：

```text
WMKEYDROP bad sid=... dropped=<n> code=...
```

输入通道支持：

```text
OP_INPUT_KEY_POLL
```

空批按 1B `ST_EAGAIN`，成功批返回 KeyEvent 定长流。

### focus 语义

P2 使用现行 WM 策略焦点：

```text
CREATE / SHOW / DESTROY / HIDE 产生的 table.focus
```

不是 click-to-focus。点击其它窗口不会即时转移键盘焦点；该交互策略归后续
窗口管理刀。P2 先把键盘路由建立在单一权威 focus 真值上，不在 inputd 或
app 复制第二套焦点。

### 生命周期

```text
WM_DESTROY 清该 window SurfaceMode → Keyboard 授权随之消失
session 死亡 → KeyQueue 随 WmSession 一起释放
focus window 摘除 → table.refocus_after_removal
```

## §7 Berry Room API

公开安全面：

```rust
pub use berry_guiproto::{KeyEvent, INPUT_KEY_EVENTS_MAX};

pub const SURFACE_INPUT_KEYBOARD: u8 = 1 << 3;
pub const SURFACE_A11Y_TREE: u8 = 0;

AppRoom::poll_keys(&mut self, out: &mut [KeyEvent])
    -> RoomResult<usize>
```

行为：

```text
复用 per-session input channel
非阻塞
空批 Ok(0)
count 超出 out.len() → RoomError::Input
app 不解析 virtio wire，不接触 inputd/wmd 内部通道
```

普通 app 如不声明 Keyboard 或不调用 poll_keys，行为零漂。

## §8 Surface policy

P1 production policy 原拒绝：

```text
Keyboard | Gamepad | IME Host → ST_UNSUPPORTED
```

P2 改为：

```text
Gamepad | IME Host → ST_UNSUPPORTED
Keyboard 可授予
```

Keyboard 不要求 P1 escape seed：

```text
Retained + Keyboard 可授予
Canvas/Immersive + Keyboard 可授予
无有效 seed 的 Canvas/Immersive 仍 ST_DENIED
```

P1 负例从 Keyboard 改为 Gamepad，继续钉板“无生产者能力不得空授”。

## §9 真链验收

### 启动态 fail-closed

GUI8 normal 在点击 Browser 前注入：

```text
KEY_A down
KEY_A up
```

此时无 app 持 Keyboard 合约，必须得到两次：

```text
WMKEYDROP bad ... code=30
```

且不得出现：

```text
WMKEYROUTE ok
```

### UITEST 授权链

UITEST runtime 完成既有 BerryUI 返回栈后：

```text
Canvas + Touch|Keyboard + IME Disabled + a11y Hybrid
→ 0x67 授予
→ UITESTKEYWAIT ok input=9
→ GUI8 QMP 注入 KEY_A down/up
→ WMKEYROUTE down/up 各一次
→ poll_keys 收到同 code down/up
→ UITESTKEY ok code=30 downs=1 ups=1
```

真实日志三段：

```text
启动态无 focus：
WMKEYDROP bad sid=0 window=0 reason=focus code=30   ×2

Browser 为 focused window 但 Retained 未声明 Keyboard：
WMKEYDROP bad ... reason=class code=30             ×2

UITEST 授权链：
WMKEYROUTE ok sid=15 window=14 code=30 action=1
WMKEYROUTE ok sid=15 window=14 code=30 action=0
UITESTKEY ok code=30 downs=1 ups=1

UITEST WM_DESTROY / WMDEAD 后再注入：
WMKEYDROP bad ... code=30                          ×2
WMKEYROUTE 新增                                    0
```

GUI8 对以上均为精确计数，不用“包含即过”。

## §10 测试与守卫

已落地：

```text
berry-guiproto KeyEvent byte golden
KeyEvent action/modifier/device/code 拒面
0x23 route / 0x24 poll round-trip
count 0/16、截断、尾随、保留位拒面
244B 恰帽 const assert

gui-inputd InputDeviceKind 能力位分类
Pointer / Keyboard / mouse-like Unknown
KeyPump down/repeat/up
KeyPump 修饰位
KeyPump BTN_* 忽略
KeyPump 队列帽与 drop counter

gui-wmd musl release build
WmSessions 结构帽 ≤16KiB
gui-wmd spawn stack 8→16 pages

GUI8 self-test：
缺 INPUTDEV / INPUTKEYDROPS / KEYWAIT / KEY / 键码造假 /
marker 重复 / 拒收数不足 FAIL
fake corpus 覆盖 focus drop、class drop、销毁后 drop 与授权 route

GUI8 normal：
无 focus KEY_A 拒收
focused Browser 无 Keyboard 合约拒收
授权 UITEST KEY_A down/up 真链
UITEST 销毁后 KEY_A 拒收且零新增路由
六 BerryUI app / pointer / touch 回归

keyboard 缺席拓扑（手动 EFI 相）：
INPUTDEV ok tab=30 kb=<usize::MAX>
INPUTDROPS ok n=0
gui-inputd 继续获得 wmd 快照
pointer 服务不 park
```

P2 实测还暴露两项启动稳定性问题并修复：

```text
Surface seed ack 等待 10s → 120s
gui-wmd stack 8 pages → 16 pages
```

原因：P1 seed 是 GUI 服务装配硬前置；P2 KeyQueue 使 `WmSessions` 增至
约 15KiB，叠加 serve 帧局部后 8 页主栈无余量。TCG 冷启动下二者叠加会
造成 gui-wmd 未入 serve、svcmgr 死等。

最终回归已通过：

```text
berry-guiproto 全量 host 测试
berry-virtio / gui-inputd 全量 host 测试
berry-guiproto / berry-gfx / berry-ui no_std
berry-ui guest-font-vmo no_std
berry-room no_std
UITEST no_std
gui-wmd musl build/test profile check
tools/berry host check
just test-host
just check
GUI8 self-test
GUI8 normal-boot（四段键盘证据）
GUI8 dual boot + layout self-heal
smoke-image 138/138
cargo deny licenses
docs_guard / Curia strict / writable_globals
```

## §11 A1 预算

当前 rootfs：

```text
总量       = 33,818,824B
单卷 cap   = 33,554,432B
rootfs0    = 33,554,432B
rootfs1    = 264,392B
总量 cap   = 134,217,728B
总量余量   = 100,398,904B
```

GUI 包体：

```text
BROWSER.bap  = 74,188B
SETTINGS.bap = 72,040B
FILES.bap    = 72,424B
PHOTOS.bap   = 92,000B
CONTACTS.bap = 75,444B
UITEST.bap   = 71,784B
```

增长主要来自：

```text
KeyEvent codec
inputd 双设备认领与 KeyPump
wmd KeyQueue/route/poll
Berry Room poll_keys
UITEST 键盘等待段
```

## §12 最终镜像

当前 P2 镜像：

```text
SHA-256 = 734335063cd77a1ba5c7420edbe1da54da42153c3c429c13771d540c063497cd
```

## §13 未完成边界

- Gamepad 仍无真实设备/事件生产者，`ST_UNSUPPORTED`。
- gui-wmd 仍无本机 host 测试 harness；P2 依靠 guiproto/inputd host 纯函数与 GUI8 四段真链覆盖，KeyQueue 环回/溢出矩阵归后续服务测试刀。
- IME Host 仍无 composition 服务与推送通道，`ST_UNSUPPORTED`。
- 键盘焦点仍是 WM create/show/destroy 策略焦点，非 click-to-focus。
- 键盘 LED / statusq 未消费。
- Key repeat 由设备透传，系统层尚无自动 repeat 策略。
- a11y Keyboard 语义旁车仍只有 P0 region/tree 词表，无批量传输。
- 多键盘 / 热插拔 / 键盘布局映射未开放。
- 未做 key release-on-destroy 合成；队列与授权随窗口/session 清理，短按
  down/up 形态由真实 QMP 批次验证。
