# BerrySettings F6 迁移

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI F6 的第一个 Native Shell 迁移规格。执行序权威 = [GUI框架地基路线-v1.4.md](GUI框架地基路线-v1.4.md)；组件与主题见 [BerryUI-F4组件第一版-v1.1.md](BerryUI-F4组件第一版-v1.1.md)，真链宿主见 [BerryUI-F5系统验收-v1.0.md](BerryUI-F5系统验收-v1.0.md)，Surface 词表见 [BerrySession-P0SurfaceMode协议-v1.0.md](BerrySession-P0SurfaceMode协议-v1.0.md)。容量口径同时遵循 [容量工程前置-A1设计草案-v1.0.md](容量工程前置-A1设计草案-v1.0.md)：单卷 ≤32MiB、多卷总量 ≤128MiB。

## §1 迁移结果

Settings 从迁移前手写 Native Shell 迁到：

```text
Berry Room：APP env / WM session / 窗口 / frame VMO / 输入 / 销毁
BerryUI：Scaffold / AppBar / ScrollView / Card / Button / Switch / ListItem / Divider / Text / Row / Column
Berry Design：Theme + ThemeMetrics + InteractionState
系统字形：per-app HD A8 VMO → FontSource → SYSTEM_FONT_ATLAS
```

迁移后 Settings 源码不再出现：

```text
berry_channel_*
berry_vmar_*
berry_vmo_*
WM RPC encode/parse
frame stride 计算
frame 指针
put_px / fill / round_rect / circle / glyph 像素循环
berry-nshfont
berry-sys 直接依赖
```

app 只保留设置业务状态与页面语义；DisplayList 由 BerryUI 发射、Berry Room 光栅化并提交。

## §2 页面与业务状态

页面结构：

```text
Scaffold + AppBar “Settings”
ScrollView(surface)
Card(surface)
Back Button
WLAN row + Text + Switch
Divider
Bluetooth row + Text + Switch
Display / Sounds / Battery / About phone ListItem
```

业务状态归 app：

```text
wlan: bool
bluetooth: bool
toggles: u32
frame_seq: u32
```

Switch/ Button 不携带业务回调：

```text
BerryUI Msg → app handler
Switch on_msg 返回 true → app 写回 wlan/bluetooth
Button Back on_msg Click → app 发出返回语义
状态变更后 app 调 render → BerryUI damage → DisplayList → Room submit
```

v1 仍不接驱动、系统服务或持久化；WLAN/Bluetooth 是进程内真实状态与真实重绘，不宣称系统级开关已生效。

## §3 系统字形与渲染

Settings 启用：

```toml
berry-ui = { path = "../../berry-ui", features = ["guest-font-vmo"] }
```

并注入：

```text
SYSTEM_FONT_ATLAS
```

S32 文本（含 ListItem detail）走 wmd per-app 32×48 ASCII A8 VMO；app 不复制静态 HD/CJK 表，也不留下 S24 空绘面。

渲染链：

```text
ui.wake
ui.render_frame
CommandEncoder::finish
AppRoom::render_display_list
AppRoom::submit
```

每帧输出：

```text
SETTINGSRENDER ok seq=<n> cmds=<n>
```

## §4 GUI8 真链验收

`gui8-smoke --normal-boot` 的 Settings 段改为 BerryUI layout-driven：

```text
launcher 真实 APP_SPAWN
等待 SETTINGSWIN + SETTINGSLAYOUT + SETTINGSRENDER + SETTINGSFRAME
解析 SETTINGSLAYOUT 中 Back/WLAN/Bluetooth bounds
QMP 点击 WLAN Switch 中心
等待 SETTINGSSTATE n=1 + TOGGLE1 + 新帧
QMP 点击 Bluetooth Switch 中心
等待 SETTINGSSTATE n=2 + TOGGLE2 + 新帧
QEMU 截图断言 Berry Theme 像素与系统 A8 色数
QMP 点击 Back Button 中心
等待 SETTINGSHIT + BACK + DESTROY + EXIT
```

2026-11-08 实测布局：

```text
Back       = (12,72,1056,60)，点击 (540,102)
WLAN Switch = (146,141,78,42)，点击 (185,162)
Bluetooth Switch = (306,203,78,42)，点击 (345,224)
PhaseUP Back = (539,101)
```

精确业务状态：

```text
SETTINGSSTATE ok n=1 wlan=1 bluetooth=0
SETTINGSSTATE ok n=2 wlan=1 bluetooth=1
```

截图像素锚：

```text
(540,20)  = AppBar primary (0,116,224)
(540,600) = surface (255,255,255)
WLAN Switch 中心       = Selected background (214,228,255)
Bluetooth Switch 中心  = Selected background (214,228,255)
```

并保留系统 A8 反马赛克色数锚：

```text
box = (20,90,700,330)
minimum colors = 128
```

GUI8 self-test 的 Settings 假串同步包含 layout/render/state/hit/toggle/back/destroy/exit，缺状态帧、缺重绘、顺序倒置均 fail closed。

## §5 预算

此前 GUI 文档把 `33,554,432B` 写成 rootfs 总量帽；容量工程 A1 的真实口径是：

```text
单卷 cap   = 33,554,432B
总量 cap   = 134,217,728B
```

F6 最终 rootfs：

```text
总量       = 33,594,856B
rootfs0    = 33,554,432B（恰单卷帽）
rootfs1    = 40,424B
总量余量   = 100,622,872B
```

包体：

```text
旧 SETTINGS.bap = 27,520B
新 SETTINGS.bap = 68,896B
增量            = 41,376B
stack_pages     = 48
UITEST.bap      = 64,416B（不变）
```

四个未迁移 Native Shell 包体：
```text
BROWSER.bap  = 31,804B
FILES.bap    = 27,912B
PHOTOS.bap   = 27,448B
CONTACTS.bap = 31,308B
```

最终 F6 镜像 SHA-256：

```text
a03b1f6559cd3ef5247253ba36793843df6d64cad61d12df124b8ce7b107723f
```

迁移增加的是首个真实应用级 BerryUI 静态消费者代码；系统 A8 字表未复制进 app。后续应用迁移必须继续逐 app 记账，不能把单卷帽误当总量帽，也不能以降低视觉资产质量换空间。

## §6 回归证据

已通过：

```text
Settings no_std build
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal
smoke-image 138/138
rootfs0/rootfs1/total 字节对拍
EFI 十段 + 裸相 28 + 生态 97
金丝雀严格递增
RX 注入后活性复验
```

最终全量回归清单见本文提交记录与 [阶段沿革.md](阶段沿革.md) 对应条目。

## §7 未完成边界

- Settings 仍无系统驱动/持久化；WLAN/Bluetooth 只改变进程内状态。
- F6 时点嵌套 clipping 归后续 BerryUI 刀；C1 已落地结构性 Clip，Settings 继续经 ScrollView 自动获得裁剪。
- CanvasSurface / immersive buffer 仍未开放；Settings 走 P0 Retained 路径。
- F6 时点 Browser/Files/Photos/Contacts 仍是基线；F7 已迁移 Files/Contacts，当前仅 Browser/Photos 待 F8。
- `berry-nshfont` 在 F6 时点仍被迁移前基线引用；F8 后无 GUI app 消费者，F9 已删除。
