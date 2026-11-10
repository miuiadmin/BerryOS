# BerrySession P1 Surface Mode 生产接线

> 版本：**1.0**｜2026-11-08
>
> 定位：P0 Surface Mode 协议的生产消费刀。P0 冻结 `0x67`、IME composition
> 与 a11y region 的 wire 词表；P1 只把 Canvas / Immersive 的授权、协商、
> buffer 借用与呈现生命周期接入 `svcmgr → berry-room → gui-wmd`，不扩
> `VER_WM`，不改 `0x60..=0x66` 帧型，也不宣称 IME / a11y 批量服务已落地。
> 执行序权威 = [GUI框架地基路线-v1.9.md](GUI框架地基路线-v1.9.md)。

## §1 结论

P1 后三条 UI 路径的 production 状态为：

```text
Retained   已开放：BerryUI retained tree / DisplayList
Canvas     已开放：授权 app 经 Berry Room 借写窗口 buffer
Immersive  已开放：授权 app 经 Berry Room 借写窗口 buffer
```

开放面有意收窄：

```text
授权源       manifest [capabilities] surface_escape=true
授权件       svcmgr 预载的 per-boot escape seed
授权消费者   Berry Room WM_CONNECT 随行迁移给 gui-wmd
buffer      app 私有窗口 VMO，Berry Room 持映射与 handle
呈现         既有 wmd 合成 VMO + guifbd credits/token 链
```

普通 GUI app 的禁令不变：不得直连 WM RPC、不得映射 VMO、不得计算 stride、
不得绕过 Berry Room 写屏。P1 没有为 Canvas / Immersive 新开第二 UI 框架。

## §2 授权链

### manifest

`berry-manifest` 与 `tools/mkapp.py` 新增 add-only 能力键：

```toml
[capabilities]
wm = true
surface_escape = true
```

规则：

```text
缺省 surface_escape = false，旧包逐字节零漂
surface_escape=true 必须 wm=true
非法 bool / 未知能力键 loud 拒绝
mkapp canonical 重发射同步该键
```

当前唯一授权验收车：

```text
UITEST
```

allowlist 三层同步执法：

```text
tools/mkapp.py        打包期拒绝未授权 surface_escape
tools/gui_guard.py    CI/源码树 fail closed
user/bins/svcmgr      运行期不铸 seed channel
```

其他 app 声明 `surface_escape=true` 直接 fail closed；缺 `wm` 同样拒绝。

### per-boot seed

gui-wmd 启动后、任何 app broker 端被 dup/赠出前，svcmgr 先在原始
`w_cli` 上发送内部 add-only 控制帧：

```text
OP_WM_SURFACE_ESCAPE_SEED = 0x68
帧长 = 9B = op + seed[8]
```

seed 由 svcmgr 按启动时序生成：

```text
非公开常量
不写入 app 包
不进入 manifest
每次启动不同
```

gui-wmd：

```text
只在任何 WM_CONNECT 前接受一次
重复 / 迟到 seed → 拒绝
保存 per-boot seed
回 1B ST_OK
```

svcmgr 阻塞等到该 ack 后，才建立可赠给 app 的 `WMD_CLI_DUP`。

授权 app launch 时，svcmgr 创建 channel pair：

```text
sender 预载 8B per-boot seed
立即 close sender
receiver 作为 app env 最后一个 extra gift
```

Berry Room 读取该 gift 后，在 `WM_CONNECT` 请求的随行 handle 中作为第 4
枚迁给 gui-wmd：

```text
[wm endpoint, input endpoint, self process, escape seed channel]
```

gui-wmd 恰读一次：

```text
消息长度 = 8
随行 handle = 0
字节 == per-boot seed
读取后立即 close token handle
```

任一项失败则整个 WM_CONNECT 以 `ST_DENIED` fail closed。app 自造 channel
并预载历史公开常量 `BSESCAPE` 无法通过；gfxdemo 在真实 QEMU 链中完整复刻
该攻击并断言精确 `ST_DENIED`。

### 为什么不用 client_type 自报

`WM_CLIENT_APP / WM_CLIENT_SYSTEM` 是既有自报形态，不适合作为 escape
权限。P1 不新增 `client_type=2`，也不新增 wmd 第二 broker 端；seed channel 由
svcmgr 按 manifest 授权铸出，避免以下三重扩张：

```text
CHAN_HND_MAX 8→9
wmd 服务端 extras 4→5
wait_many 16 对象帽重构
```

当前 `fs+wm+token` 的 app gift 上界为 4，仍在 channel 随行帽内。

## §3 gui-wmd 0x67 服务语义

`on_session` 在既有 WM 域分发表前新增专用臂：

```text
OP_WM_SURFACE_MODE = 0x67
```

处理序：

```text
1. 帧解析失败                 → ST_BAD
2. app 合约失败 / 自报 System  → ST_DENIED
3. surface_id 非本 owner 可见窗 → ST_NOENT
4. Canvas/Immersive 且无有效 seed → ST_DENIED
5. 无生产者/服务的能力          → ST_UNSUPPORTED
6. 生成 20B 授予帧并记录逐窗状态
```

P1 的 fail-closed 能力策略：

```text
Keyboard 位    → P1 时点 ST_UNSUPPORTED；P2 起真实生产链开放
Gamepad 位     → ST_UNSUPPORTED
IME Host       → ST_UNSUPPORTED
```

原因：gui-inputd 当前只有 pointer 生产者，IME composition 也没有服务通道。
Touch / Stylus / Mouse 的 pointer 族语义可以进入合约词表，但不能伪造键盘、
手柄或输入法事件。

授予帧：

```text
Retained   flags = Focus|Rate|Cleanup|A11y
Canvas     flags = Focus|Rate|Cleanup|A11y|Escape
Immersive  flags = Focus|Rate|Cleanup|A11y|Escape
scale      = 1000
safe area  = 0,0,0,0
```

safe area 为 0 是当前 wmd 表的真值：系统状态栏 / 壁纸仍是 launcher 窗口
内容层，不存在 wmd SYSTEM layer 窗口。未来系统层窗口落地时由 wmd 从真实
几何推导 inset，不允许 app 自报。

### 重协商

同一 surface 的状态规则：

```text
同请求                 幂等成功
Canvas ↔ Immersive    授权 session 内允许重协商
escape → Retained     ST_DENIED
普通 APP escape 请求   ST_DENIED
```

重协商只改变 0x67 合约记录，不额外复制 buffer、不重开窗口、不改 present
credits。

### 清理

surface 合约状态挂在 `WmSession`，不是全局第二张窗口表：

```text
WM_DESTROY   清对应 surface_id
Process 死亡  整个 WmSession 与全部 surface 状态一起释放
窗口表        沿用 destroy_owned / refocus / recompute / bump
```

## §4 Berry Room SDK

`RoomConfig` 新增：

```rust
pub struct RoomConfig {
    pub argv0: &'static [u8],
    pub uses_fs: bool,
    pub surface_escape: bool,
}
```

普通六 app 均为 `surface_escape:false`；UITEST 为 `true`。

app 侧安全面：

```rust
SurfaceModeKind::{Retained, Canvas, Immersive}
SurfaceRequest { mode, input_classes, ime_mode, a11y_mode }
SurfaceGrant { mode, input_classes, ime_mode, a11y_mode,
               contract_flags, scale_permille, safe_* }
AppRoom::configure_surface(request)
AppRoom::surface_grant()
AppRoom::surface_frame_mut()
```

`configure_surface()`：

```text
role 恒 App
surface_id 恒当前 Room 窗口
safe area / scale 只接受 host 授予
grant 必须与请求逐字段匹配且 fit 当前窗口
错误映射为 RoomError::SurfaceDenied / SurfaceUnsupported / SurfaceBad
```

`surface_frame_mut()`：

```text
仅 Canvas / Immersive 授予后可用
返回窗口 XRGB buffer 借用
VMO handle、映射、stride、清理仍归 Berry Room
Retained 调用恒 SurfaceDenied
```

呈现仍复用：

```text
AppRoom::submit()
wmd relay::submit_composite
guifbd SUBMIT / FRAME_CALLBACK / RELEASE_WAIT
```

P1 不新增 zero-copy slot，不修改 guifbd，不改变 credits/token 环。

## §5 旁车边界

P1 记录并授予 input class / IME mode / a11y mode，但不伪造服务：

```text
IME Host           拒绝，直到 IME 服务与推送通道落地
a11y Tree/Hybrid   只作为合约状态记录，批量传输未开放
a11y Regions       只作为合约状态记录，区域表提交 API 未开放
P1 时点 Keyboard/Gamepad 拒绝；P2 起Keyboard 由真实生产链承载，Gamepad 继续拒绝
```

因此 P1 的 Canvas 验收请求为：

```text
Canvas     Touch|Mouse + IME Disabled + Hybrid
Immersive  Touch + IME Disabled + Regions
```

这不是协议词表缩水：`berry-guiproto` 仍保留 P0 全部 codec 与值域；只是
production wmd 对无服务能力诚实返回 `ST_UNSUPPORTED`。

## §6 真链验收

### 普通APP 拒面

`gfxdemo` 在自己拥有的可见窗口上请求 Canvas：

```text
无有效 seed
input = Touch|Mouse
ime   = Disabled
a11y  = Hybrid
```

必须得到：

```text
1B ST_DENIED
```

20B Retained 降级、ST_BAD、静默成功都为失败。

### UITEST 授权链

UITEST manifest 声明：

```text
wm=true
surface_escape=true
```

boot-probe 在 C1 Clip 自证后执行：

```text
1. P1 时点 Canvas + Keyboard 负例 → SurfaceUnsupported；
   P2 起该负例改为 Gamepad，Keyboard 走真实生产链

2. Canvas + Touch|Mouse + IME Disabled + Hybrid
   → 20B 授予，flags=31，scale=1000，safe=0,0,0,0
   → surface_frame_mut() 填充整窗 buffer
   → frame 8 真实 SUBMIT / callback / release

3. Immersive + Touch + IME Disabled + Regions
   → 20B 授予，flags=31，scale=1000，safe=0,0,0,0
   → surface_frame_mut() 填充整窗 buffer
   → frame 9 真实 SUBMIT / callback / release
```

guest 标记：

```text
UITESTSURFACE ok modes=1,2 frames=8,9 flags=31 scale=1000 safe=0,0,0,0
```

GUI8 normal 精确断言该标记恰一次；缺 Surface 真链的 fake corpus 必须 FAIL。

### 既有链零回退

P1 后仍需保持：

```text
APP ok n=9
六个 BerryUI app 真实 APP_SPAWN / WM / frame / input / destroy
Files READDIR/STAT/GET 真链
Photos 5000B 图像真链
Contacts 4 记录真链
UITEST BerryUI 返回栈与 C1 Clip 几何
```

## §7 测试与守卫

已落地测试面：

```text
berry-guiproto per-boot seed 控制帧严格解析
berry-guiproto production policy（escape 拒绝/Unsupported/降级拒/合约位）
berry-manifest surface_escape 缺省、wm 必备、重复键/非 bool 拒绝
gfxdemo 自铸历史公开常量 token 的完整攻击负例
gfxdemo foreign window / hidden window / 0x67 附带 handle 负例
gui-boundary-guard --self-test：21 类负样本（含未授权 frame_mut）
gui-boundary-guard：非授权 surface_escape fail closed
GUI8 self-test：缺 UITESTSURFACE / flags 造假 / marker 重复假串 FAIL
GUI8 normal：UITESTSURFACE 恰一次 + 六 app 真链
```

`surface_mode_production_policy` 下沉在 `berry-guiproto`，由 `just
test-host` 直接执行；gui-wmd 只保留服务装配面，不再携带不可运行的
cfg(test) 文本。

静态守卫新增：

```text
surface_escape=true 需要 wm=true
surface_escape app 必须在显式 allowlist
重复 / 冒用 app id 直接拒绝
```

当前负样本数：

```text
21
```

最终回归已通过：

```text
berry-guiproto / berry-manifest 全量 host 测试
berry-guiproto / berry-gfx / berry-ui no_std
berry-ui guest-font-vmo no_std
berry-room no_std
UITEST no_std
just test-host
just check
cargo deny licenses
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal
smoke-image 138/138
docs_guard / Curia strict / writable_globals
```

## §8 预算

P1 当前 rootfs：

```text
总量       = 33,811,480B
单卷 cap   = 33,554,432B
rootfs0    = 33,554,432B
rootfs1    = 257,048B
总量 cap   = 134,217,728B
总量余量   = 100,406,248B
```

GUI 包体：

```text
BROWSER.bap  = 74,188B
SETTINGS.bap = 72,040B
FILES.bap    = 72,424B
PHOTOS.bap   = 92,000B
CONTACTS.bap = 75,444B
UITEST.bap   = 70,920B
```

增长主要来自：

```text
Berry Room Surface API
gui-wmd 0x67 状态与策略
svcmgr per-boot seed 预共享与授权 app token channel
UITEST 授权验收段
```

## §9 最终镜像

```text
SHA-256 = ecd7af0007bb8e87e0fefd37f69d6f274170cc9b8727807932d785ea1235f3bd
```

## §10 未完成边界

- P1 时点 Keyboard/Gamepad 输入事件帧未落地；P2 已开放 Keyboard，Gamepad 仍未落地。
- IME composition 服务、软键盘与推送通道未落地；`IME_HOST` 恒拒绝。
- a11y Regions / Hybrid 的批量提交、订阅与更新协议未落地。
- safe area 仍为全 0；wmd SYSTEM layer 窗口尚未成为真实来源。
- Canvas damage 仍走 `SUBMIT_F_DAMAGE_FULL`；局部 damage API 随后续性能刀。
- HDI zero-copy / async buffer slot 未开放。
- Berry Room 仍为单窗口单 surface 形态；多 surface 需先重构 Room 状态。
- 授权 allowlist 当前只有 UITEST；第一个真实专业 / 游戏 renderer 必须逐 app 显式扩表。
- seed 目前由 CLOCK_MONOTONIC 与启动 handle 混合生成，wmd 未做错误 token 猜测锁定；真实特权（scanout/键盘/IME）接入前需换内核熵源并加失败计数。
- 授权 app 在 connect 前物理持有 seed channel；v1 信任 manifest allowlist 内 app。若未来要求 per-launch 单次使用，应改为按 Process handle 注册的一次性 token 表。
