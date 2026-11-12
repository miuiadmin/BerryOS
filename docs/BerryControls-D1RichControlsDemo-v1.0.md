# BerryControls D1 Rich Controls Demo

> 版本：**1.0**｜2026-11-12
>
> 状态：**superseded**（2026-11-12 用户追裁改名并扩展范围）。
> Superseded-by: [BerryDemo-D1-v1.0.md](BerryDemo-D1-v1.0.md)。
>
> 定位：BerryUI 真实产品面 demo 规格。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)；前置 = [BerryUI-LAYOUTL6aFlexSpecAlgebra-v1.0.md](BerryUI-LAYOUTL6aFlexSpecAlgebra-v1.0.md)。本刀响应产品方向修正：暂停继续向下扩协议，先把一个视觉丰富、可运行、可点击的 demo 放到 BerryOS 桌面上。
>
> 核心裁决：新增独立 `CONTROLS` app，不改造 UITEST。UITEST 是系统验收仪器；CONTROLS 是产品面控件橱窗。

## §1 问题

此前 F6..F8 已有 Browser/Settings/Files/Photos/Contacts，但它们都是功能壳；UITEST 面向协议和性质验收，视觉上不像一个“控件都有”的 demo。继续只做协议会脱离用户可见价值。

本刀要求：

```text
真实 BerryOS 桌面入口
真实 APP_SPAWN / WM window / frame / input / destroy
BerryUI 现有公开控件全覆盖
多页面、多状态、多主题、可滚动、可交互
不新增 AST/wire/opcode
不新增 surface escape
```

## §2 App 身份

```text
id       = CONTROLS
entry    = bin/controls
label    = Controls
中文标签 = 控件
type     = app
intent   = utility
stack    = 48 pages
```

crate：

```text
user/bins/controls/
```

依赖只允许：

```text
berry-gfx
berry-room
berry-ui(guest-font-vmo)
```

`gui_guard` 将 CONTROLS 纳入七个普通 BerryUI app 的强制集合。

## §3 页面模型

### P0 Berry Gallery

首屏是视觉入口而非空列表：

```text
Scaffold + AppBar + ScrollView + Card
primary hero Panel
Display/Title/Body 层级文本
主题色 chip row
四个 ListItem 导航
Start tour Button
Exit Button
```

导航项：

```text
Buttons & states
Text inputs
Lists & panes
Theme & density
```

### P1 Buttons & States

覆盖 Button 的实际状态面：

```text
Press me 计数 Button
Switch on/off
Disabled Button
Selected ListItem
进度条形 Rect 组合
三个 demo IconSource 图形
```

点击计数会更新 Button 文本并重绘；Switch 输出 checked 状态。

### P2 Text Inputs

三种 TextField：

```text
Text
Number
Password
```

点击字段请求焦点并显示 caret；TextField 宽度、selection、caret、Clip port 全部走 GUI-T1 既有路径。

### P3 Lists & Panes

真实 ListDetail：

```text
1:2 双 pane
四个可单选 ListItem
detail pane 图标 + 标题 + 说明
可切换 EmptyState + Restore
```

本刀先保证选择与恢复状态；键盘编辑不在本 app 验收面。

### P4 Theme & Density

主题 token 视觉板：

```text
10 个 ThemeColors chip
Spacing / Radius token 样例
demo icon
Light / Dark / HighContrastLight 循环
```

主题切换重建页面，所有颜色仍来自 Berry Design token。

## §4 图形与资产

app 自带常量 `DemoIconSource`：

```text
id 100/101/102
48×48 ARGB
checker / dot / ring 三种图形
smooth_scale = true
```

普通 app 仍不得触碰 stride、framebuffer、WM RPC 或像素循环；位图只经 `IconSource` 供给。

桌面图标：

```text
108×108 RGBA PNG
mkapp 编码 BYI1
```

## §5 APP 容量

`APP_SLOTS`：

```text
9 -> 10
```

svcmgr 标记从单位数字改为两位零填充：

```text
APP ok n=10
APPUI ok n=08
```

boot 批 app 顺序变为：

```text
BROWSER  id=3
CONTACTS id=4
CONTROLS id=5
FILES    id=6
PHOTOS   id=7
SETTINGS id=8
UITEST   id=9
```

`WMCONN id=1/2` 与 GUI8 既有窗口策略链不变；boot 批多出一个 GUI app 后，
GUI8 owner-route APP session 从 sid=10 顺延为 sid=11。

launcher 首启 RPC READABLE 帽从 300s 抬到 600s：svcmgr 必须先跑完
10 个 .bap 的 boot 批才进入稳态 app serve；TCG 下该批已可超过旧 300s，
旧帽会把正常排队误判为超时。

## §6 GUI8 真链

boot 批必须完整出现：

```text
CONTROLS up
CONTROLSWIN ok
CONTROLSFRAME ok
CONTROLSPROBE ok
CONTROLSDESTROY ok
CONTROLSEXIT ok
```

normal boot 在桌面首行第四格点击 CONTROLS：

```text
真实 APP_SPAWN
CONTROLSLAYOUT page=0
点击 Buttons ListItem
CONTROLPAGE page=1
点击 Switch
CONTROLSSWITCH checked=1
AppBar/surface/Selected 像素锚
A8 文本色数锚
返回 Home
CONTROLPAGE page=0
点击 Exit
CONTROLSBACK ok
CONTROLSEXIT ok
```

GUI8 精确计数：

```text
runtime layout = 3
runtime render/frame = 4
CONTROLPAGE = 2
CONTROLSSWITCH = 1
CONTROLSHIT = 4
CONTROLSBACK = 1
```

## §7 不变式

```text
CONTROLS 不解析 WM RPC
不 map VMO
不计算 stride
不写像素
不用 surface escape
不新增 AST/wire/opcode
不新增 berry-ui facade
不改 UITEST 既有锚
```

页面重建使用：

```text
Ui::remove(page_root)
build_page(...)
ScrollView::sync_bounds
render_frame
```

业务状态由 app 持有，控件不承载回调。

已知边界：hit dispatch 会把 DragStart/DragMove 锚到最深层节点，而
ScrollView 只认 panel/port/content；CONTROLS 的整页 Card 会让拖拽滚动
在更小窗口中不可达。当前 Product 页内容均不超过 1440px viewport，本刀
不扩 SDK；结构性拖拽命中归后续 SDK / ScrollView 刀。

## §8 预算

L6a 后基线：

```text
rootfs.cpio 33,953,712B
rootfs0     33,554,432B
rootfs1       399,280B
```

D1 初值：

```text
CONTROLS.bap 154,180B
rootfs.cpio 34,108,100B
rootfs0     33,554,432B
rootfs1       553,668B
总量余量   100,109,628B
```

rootfs0 仍钉在 32MiB 帽；CONTROLS.bap 实体位于 vol0，挤出后的增量落在 rootfs1。

最终镜像：

```text
SHA-256 = 60722de0815cba51c29b2902347403f1980cbd816986360f5d7d0270274b9b1b
```

图标常驻像素预算：

```text
217,104 / 225,000
```

余量 7,896px，已不足再装一个 108×108 icon；后续新图标必须先重算该预算。

## §9 验收

```text
gui8-smoke --self-test
GUI8 normal-boot
GUI8 dual-boot canonical persistence
smoke-image 全标记
controls no_std
berry-manifest tests
svcmgr none check
launcher musl check
gui_guard
just build-user
just image
just test-host
just check
cargo deny licenses
docs_guard / Curia strict
git diff --check
```

最终复跑结果：

```text
controls no_std                         PASS
berry-manifest tests / no_std           PASS
svcmgr none check                      PASS
launcher musl check                    PASS
mkapp --check CONTROLS.bap              PASS
GUI8 self-test                         PASS
GUI8 normal-boot                        PASS
GUI8 dual-boot canonical persistence    PASS
smoke-image 145/145                     PASS
just test-host                          PASS
just check                              PASS
cargo deny licenses                     PASS
gui_guard / docs_guard / Curia strict   PASS
git diff --check                        PASS
```

smoke-image 字节对拍：

```text
kfile = 4,772,024B
init  = 34,108,100B
与宿主精确同数
```
