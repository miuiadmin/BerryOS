# Berry Demo D1

> 版本：**1.0**｜2026-11-12
>
> 定位：BerryOS 系统级演示应用现行规格。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)；取代早期“Rich Controls Demo”命名规格 = [BerryControls-D1RichControlsDemo-v1.0.md](BerryControls-D1RichControlsDemo-v1.0.md)。
>
> 核心裁决：应用直接命名为 **Berry Demo**。它不是 UI 专属 Gallery，而是同一个真实桌面应用同时展示 BerryUI 控件、Berry Room 系统特性与应用开发标准。

## §1 问题与方向修正

第一版演示应用以控件橱窗为工作名落地后，用户追裁：

```text
直接叫 berry demo，不要叫其他奇奇怪怪的名字。
它不仅是 UI 的，还有系统特性、应用开发标准的。
```

因此本规格完成两件事：

```text
1. 命名统一为 Berry Demo / BERRYDEMO，不再使用 Controls/BerryControls 产品名。
2. 范围从 UI 控件扩展为 UI + 系统特性 + 应用开发标准。
```

## §2 App 身份

```text
id       = BERRYDEMO
entry    = bin/berrydemo
label    = Berry Demo
type     = app
intent   = utility
stack    = 48 pages
```

crate 与包：

```text
user/bins/berrydemo/
user/userbins/app/BERRYDEMO.bap
```

依赖只允许：

```text
berry-gfx
berry-room
berry-ui(guest-font-vmo)
```

`gui_guard` 将 BERRYDEMO 纳入七个普通 BerryUI app 的强制集合。

## §3 页面模型

### P0 Berry Gallery

首屏展示 demo 入口与系统定位：

```text
Scaffold + AppBar + ScrollView + Card
primary hero Panel
Display/Title/Body 层级文本
主题色 chip row
五个 ListItem 导航
Start tour Button
Exit Button
```

导航项：

```text
Buttons & states
Text inputs
Lists & panes
Theme & density
System & standards
```

### P1 Buttons & States

覆盖 Button 状态面、计数状态、Switch、禁用态、选中 ListItem、进度面板与 IconSource 图形。

### P2 Text Inputs

覆盖 Text / Number / Password 三种 `TextInputKind`、caption、焦点 caret 与 app-owned `TextInputState`。

### P3 Lists & Panes

覆盖 ListDetail 双栏、四项选择、detail pane、EmptyState、Clear list 与 Restore list 的同页重建。

### P4 Theme & Density

覆盖主题 token、色板、spacing/radius 密度派生、IconSource 平滑缩放与 Light / Dark / High contrast scheme 轮换。

### P5 System & Standards

新增系统与应用开发标准页，直接展示本 app 正在使用的真实链：

```text
Desktop click → real APP_SPAWN
BERRYDEMO is a packaged .bap process
Berry Room owns WM window and frame VMO
DisplayList submit is the only render path
Pointer events arrive owner-local
Destroy and exit clean the WM session
```

应用开发标准：

```text
manifest.toml declares id, entry, wm
mkapp emits a checked .bap archive
BerryUI widgets own layout and state
IconSource supplies all demo pixels
Guest font VMO supplies HD text
No WM RPC, VMO mapping, or stride
No surface escape in a normal app
```

页面还显示当前 frame sequence，把运行时状态作为可见 demo 内容。

## §4 系统特性与应用开发标准

Berry Demo 的演示面分三层：

```text
UI：BerryUI 控件、主题、密度、焦点、输入、列表、空态与页面替换。
系统：真实 APP_SPAWN、独立进程、WM 窗口、DisplayList frame、指针输入、
      owner-local 坐标、销毁清理与桌面入口。
标准：manifest/.bap/mkapp、Berry Room、BerryUI SDK、IconSource、
      guest font VMO、gui_guard 禁令与 no surface escape。
```

应用本体不新增协议面：

```text
不解析 WM RPC
不映射 VMO
不计算 stride
不写 framebuffer
不用 surface escape
不新增 AST/wire/opcode
```

## §5 容量与会话

```text
APP_SLOTS = 10
APP ok n=10
APPUI ok n=08
普通 BerryUI app = 7
```

rootfs 按 `.bap` 字节序启动，BERRYDEMO 排在 BROWSER 前：

```text
WMCONN id=3   GUI8 SYSTEM 探针会话
WMCONN id=4   BERRYDEMO
WMCONN id=5   BROWSER
WMCONN id=6   CONTACTS
WMCONN id=7   FILES
WMCONN id=8   PHOTOS
WMCONN id=9   SETTINGS
WMCONN id=10  UITEST
```

GUI8 owner-route APP session 保持顺延后的 `sid=11`。

launcher 首启 RPC READABLE 帽为 600s：svcmgr 必须先完成 10 个 `.bap` 的 boot 批再进入稳态 app serve，TCG 下该批可超过旧 300s 帽；真机远低于该值。

## §6 GUI8 真链

normal boot 把 BERRYDEMO 放在桌面首行第四格，执行：

```text
真实桌面点击
→ APP_SPAWN
→ WM window
→ Home layout/frame
→ ListItem 进入 Buttons & States
→ Switch checked=1
→ AppBar / surface / Switch / A8 像素锚
→ Back 返回 Home
→ System & Standards tour
→ System AppBar / A8 像素锚
→ Back 返回 Home
→ Exit / destroy / exit
```

精确计数：

```text
runtime BERRYDEMOLAYOUT = berrydemo_before + 5
runtime BERRYDEMORENDER = berrydemo_before + 6
runtime BERRYDEMOFRAME  = berrydemo_before + 6
BERRYDEMOPAGE           = 4
BERRYDEMOSWITCH         = 1
BERRYDEMOHIT            = 6
BERRYDEMOSYSTEM         = 1
BERRYDEMOBACK           = 1
```

boot-probe 标记族：

```text
BERRYDEMO up
BERRYDEMOWIN ok
BERRYDEMOFRAME ok
BERRYDEMOPROBE ok
BERRYDEMODESTROY ok
BERRYDEMOEXIT ok
```

既有 UITEST 零漂锚点不变。

## §7 不变量与已知边界

业务状态由 app 持有，widget 不承载回调。页面替换使用 `Ui::remove`、重建、`ScrollView::sync_bounds` 与 `render_frame`。

已知边界：hit dispatch 会把 DragStart/DragMove 锚到最深层节点，而 ScrollView 只认 panel/port/content；整页 Card 会让拖拽滚动在更小窗口中不可达。当前 Berry Demo 各页内容均在 1440px viewport 内，本刀不扩 SDK；结构性拖拽命中归后续 SDK / ScrollView 刀。

## §8 预算

```text
BERRYDEMO.bap 155,976B
rootfs.cpio 34,109,896B
rootfs0     33,554,432B
rootfs1       555,464B
总量余量   100,107,832B
```

rootfs0 仍钉在 32MiB 帽；BERRYDEMO.bap 实体位于 vol0，挤出后的增量落在 rootfs1。

icon residency 当前为 217,104 / 225,000 px，余量 7,896px，不足再装一个 108×108 icon；后续新图标必须先重算该预算。

## §9 验收

必须全部通过：

```text
berrydemo no_std
berry-manifest tests / no_std
svcmgr none check
launcher musl check
mkapp --check BERRYDEMO.bap
GUI8 self-test
GUI8 normal-boot
GUI8 dual-boot canonical persistence
smoke-image
just test-host
just check
cargo deny licenses
gui_guard / docs_guard / Curia strict
git diff --check
```

最终镜像：`out/berryos.img` SHA-256 = `aac6e4ffd81987dd8466b6c11df04e5e338d017b31e96f67582433f7a7a6581c`。

最终复跑结果：

```text
berrydemo no_std                         PASS
berry-manifest tests / no_std            PASS
svcmgr none check                        PASS
launcher musl check                      PASS
mkapp --check BERRYDEMO.bap              PASS
GUI8 self-test                           PASS
GUI8 normal-boot                         PASS
GUI8 dual-boot canonical persistence     PASS
smoke-image 146/146                      PASS
just test-host                           PASS
just check                               PASS
cargo deny licenses                      PASS
gui_guard / docs_guard / Curia strict    PASS
git diff --check                         PASS
```

smoke-image 字节对拍：

```text
kfile = 4,772,024B
init  = 34,109,896B
与宿主精确同数
```
