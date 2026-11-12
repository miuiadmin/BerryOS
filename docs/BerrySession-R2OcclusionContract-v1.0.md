# BerrySession R2 Occlusion Contract 生产接线

> 版本：**1.0**｜2026-11-11
>
> 定位：GUI-14 D5 / B6 的第一收口刀。R2 不做 FormMetrics、SafeArea 容器、
> IME KeyboardAvoider 或 resize/config event；只钉死窗口 raw bounds、
> persistent occlusion、buffer stride 与 damage 坐标空间的单一合约。
> 执行序权威 = [GUI框架地基路线-v1.18.md](GUI框架地基路线-v1.18.md)。

## §1 背景

R1 关闭了系统身份与绘制权威洞，但 AVOID_SYS 仍有三套几何真值互不交汇：

```text
WmTable.req   = 客户端申请矩形，Room/gfxdemo 以此分配 buffer
WmTable.eff   = 可见 SYSTEM 窗遮挡后的可呈现/可命中矩形
wmd compositor= 旧实现按 eff 推导 src_stride/src_h
```

典型 top bar：

```text
APP req  = (0,0,800,600)
SYSTEM   = (0,0,800,30)
APP eff  = (0,30,800,570)
Room VMO = 800×600, stride=3200
旧 wmd   = src_stride=3200, src_h=570, 从 row 0 读
```

结果：

```text
内容整体下移 30 row
底部 30 row 丢失
水平遮挡时每行 shear
VMO size gate 仍通过，无错误无事件
```

## §2 工业语义对齐

R2 采用的主流语义：

```text
Android WindowMetrics.getBounds() = raw bounds
insets = 独立 typed channel，不缩小 Surface buffer
edge-to-edge 是默认形态

Flutter:
  viewPadding = persistent occlusion
  viewInsets  = transient IME occlusion
  padding     = max(0, viewPadding - viewInsets) 派生值

Fuchsia:
  logical_size 变化触发 re-layout
  device_pixel_ratio 变化可只触发 buffer reallocation
  buffer 几何不因 occlusion 静默缩小

Wayland:
  geometry 变化是 configure + serial + ack 事务
  归 R3，不在本刀做 push event
```

因此 R2 选择：

```text
buffer / layout 基准 = raw window bounds
hit-test / composite 可见域 = visible bounds
persistent occlusion = 0x67 safe_* 授予值
transient IME occlusion = 词表保留，无生产者
```

## §3 双几何模型

新增 host 纯策略类型：

```rust
berry_guiproto::authority::SurfaceGeometry {
    raw_x, raw_y, raw_w, raw_h,
    visible_x, visible_y, visible_w, visible_h,
}
```

不变量：

```text
visible ⊆ raw
raw/visible w/h 均非零
全遮挡不构造 w/h=0 假几何，SUBMIT 拒绝
```

映射：

```text
visible_local =
  (visible.x-raw.x, visible.y-raw.y, visible.w, visible.h)

src_stride = raw_w * 4

screen(visible.x+x, visible.y+y)
  := buffer[(visible.y-raw.y+y) * src_stride
            + (visible.x-raw.x+x) * 4]
```

top bar 例子：

```text
screen y=30 读 raw row 30
screen y=599 读 raw row 599
不再从 raw row 0 重排
```

## §4 SUBMIT 合约

### FULL

`SUBMIT_F_DAMAGE_FULL` 仍表示 raw buffer 整窗语义，但实际拷贝前先收缩到：

```text
visible_local
```

因此：

```text
不读持久遮挡区
不写持久遮挡区
不丢底部 raw row
不产生水平 shear
```

### partial damage

damage 坐标空间立法为：

```text
raw surface-local buffer 坐标
```

处理链：

```text
rect ∩ raw bounds
∩ visible_local
→ 以 raw origin 平移到屏幕
→ present damage rect
```

完全越出 raw 或完全落在持久遮挡区的 rect 丢弃；整批裁空仍：

```text
ST_BAD
```

VMO size gate：

```text
size ≥ raw_w*4*raw_h
```

oversized VMO 合法，但窗外字节不可进入屏幕。

## §5 persistent occlusion / safe insets

0x67 继续使用既有字段：

```text
safe_left
safe_top
safe_right
safe_bottom
```

R2 起 APP surface 的授予值由 wmd 从双几何推导：

```text
left   = visible.x - raw.x
top    = visible.y - raw.y
right  = raw_right - visible_right
bottom = raw_bottom - visible_bottom
```

语义：

```text
坐标系 = raw surface-local
来源   = wmd host 授予，APP 不得自报
用途   = app 布局避让
不改变 buffer 几何
不与 AVOID_SYS 双重收缩
```

`surface_mode_info_fits` 以 raw bounds 检查，保证 safe insets 不吞满
surface。

### typed inset 词表（不上 wire）

新增 host-side `berry-guiproto::insets`：

```text
SystemBars
DisplayCutout
Ime
SystemGestures
MandatorySystemGestures
TappableElement
SystemOverlays
```

producer 裁决：

```text
SystemBars ← 可见 SYSTEM 窗（R2 已生产）
Ime        ← P6b IME Host（未生产）
其余       ← 无真值源，恒不授予
```

`OcclusionSnapshot` 提供 Flutter 形派生：

```text
effective_padding = max(0, view_padding - view_insets)
```

派生值不落第二存储真值。

## §6 Room bootstrap

`AppRoom::open_window()` 继续以 raw display bounds 分配 VMO，并在
CREATE 后做一次：

```text
WM_TABLE_GET
```

Room `open_window` 现在携带 `WM_CREATE_F_AVOID_SYS`；raw 申请仍恒为
`(0,0,disp_w,disp_h)`，因此可从 snapshot 的 effective 矩形推导
create-time persistent insets。

新增：

```rust
AppRoom::occlusion() -> OcclusionSnapshot
```

显式 0x67 成功后，Room 用最新 grant safe_* 刷新该快照。推导函数有
host 单测（无遮挡、top、left+bottom）；当前 BerryUI 容器尚未消费该快照。

边界：

```text
TABLE_GET 是 v1 stopgap，不是动态事件
SYSTEM 窗在其后出现时快照可陈旧
push/config/serial/ack 归 R3
```

R3 追记：`AppRoom::open_window` 的初始 TABLE_GET bootstrap 已被
`CREATED config POLL/ACK` 替换；本节保留为 R2 时点的几何推导依据与迁移
记录。

## §7 输入坐标

hit-test 仍用 visible bounds；坐标本地化改为：

```text
screen - raw origin
```

因此 app 收到的 pointer 坐标与 raw buffer row/column 同源：

```text
top bar 32px 下，visible 顶部点击 y≈32
而不是从 0 重排
```

这与 edge-to-edge + insets 语义一致，避免输入 hit box 与 buffer 内容错位。

## §8 GUI8 真链证据

每个 GUI8 哨兵 boot 精确断言：

```text
WMSYSOCC       ×1   SYSTEM session 创建真实 SYSTEM-layer 遮挡窗
WMAVOIDWIN     ×1   APP raw/visible/safe 几何
WMAVOIDHIDE    ×1   隐藏遮挡后 visible 恢复 raw
WMAVOIDSHOW    ×1   再显示后 visible 重新让出 top 32
WMAVOIDGRANT   ×1   0x67 safe_top=32
WMAVOIDFRAME        ×1   raw row pattern 真实提交
WMOCCDAMAGEBAD      ×1   damage 完全落在持久遮挡带 → ST_BAD
WMDAMAGECLIP        ×1   partial damage 横跨 raw 边界
WMRAWBUFBAD         ×1   eff-size VMO 小于 raw bounds → ST_BAD
```

Boot A 额外像素锚：

```text
跨界 damage：
  窗内 x=w-4 = (255,0,255)
  右邻 x=w+4 = 右窗原底色，不被 oversized VMO 污染

top occlusion：
  raw buffer 每行 R=y、G=64、B=128
  screen y=40 必须 R=40
  screen y=132 必须 R=132
  旧 eff-stride 合约会分别读成 R=8 / R=100
```

输入锚：

```text
QMP 点 APP raw 窗中心
WMROUTEWIN local x/y 必须等于 screen - raw origin（QMP/取整容差 ±2）
```

常态 normal boot：

```text
不创建哨兵 SYSTEM 遮挡窗，七个 occluder marker 机器断言为 0
WMDAMAGECLIP / WMRAWBUFBAD 仍在席且恰一次
P2/P3/P4 与六个 BerryUI app 全链零回退
```

相邻硬化：session RPC 通道若 parked 在 TABLE_WAIT，进程死亡回收时先丢弃
parking 位，避免后续 bump 对已关 handle 发送导致 wmd trap。该问题为既有
面，非 R2 引入。
## §9 host 测试

Berry Room 新增 host 单测：

```text
无遮挡 snapshot = 0
top occlusion snapshot = top inset
left+bottom occlusion snapshot
```

新增 `berry-guiproto/tests/occlusion.rs`：

```text
top occlusion row mapping
left occlusion horizontal skew
raw/visible subset 与零尺寸拒绝
inset kind 域与 producer 门
Flutter effective_padding 代数
damage straddle / under-bar / outside-raw
```

既有 wire golden 与 P0/P1 测试零漂。上述 Room 测试已纳入 `just test-host`。

## §10 未完成边界

- configure/resize/inset 变化仍无 per-session push event；归 R3。
- transient IME occlusion 只有词表，无生产者；归 P6b。
- Display cutout / gesture / tappable / overlay 无真值源，恒不授予。
- compositor 仍无通用 z/occlusion 树；R2 只定义 SYSTEM 遮挡与 raw buffer
  采样，不宣称多窗 z-order 完成。
- `surface_id=0` 全局桌面 surface 仍是 SYSTEM 聚合路径；它本身不是
  AVOID_SYS occluder，只有可见 SYSTEM 窗参与推导。
- raw client 若 `WM_SET F_GEOM` 改变几何而不同步 buffer，仍属协议使用
  错误；buffer 显式声明 op 归后续加固刀。
- guifbd/GOP padded stride 仍是未来 add-only GETINFO 扩展，不在本刀。
- BerryUI SafeArea/KeyboardAvoider 容器未实现；app 目前只获得 insets
  快照/授予值。

最终回归已通过：

```text
berry-guiproto host：46 unit + 20 integration
berry-room host snapshot：1
just test-host（含 berry-room）
just check
gui-wmd / launcher / gfxdemo target check
berry-guiproto / berry-gfx / berry-ui no_std
berry-ui guest-font-vmo no_std
berry-room no_std
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal（A/B/C 九 marker 全 ×1）
smoke-image 138/138
cargo deny licenses
docs_guard / Curia strict / writable_globals
```

## §11 预算

当前构建：

```text
总量       = 33,853,944B
单卷 cap   = 33,554,432B
rootfs0    = 33,554,432B
rootfs1    = 299,512B
总量 cap   = 134,217,728B
总量余量   = 100,363,784B
```

rootfs0 恰在单卷帽；继续增加 rootfs payload 前必须先给拆卷/压缩/迁出
方案。

## §12 最终镜像

当前候选镜像 SHA-256：

```text
062cab37d76a4e9b479d2ab22e1e02d1ce5bf09837a52193745cd0099dc9fee8
```

复跑镜像 SHA-256 即上值。
