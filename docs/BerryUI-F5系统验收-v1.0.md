# BerryUI F5 系统验收

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI F5 的真实消费者验收记录与现行边界。执行序权威 = [GUI框架地基路线-v1.13.md](GUI框架地基路线-v1.13.md)；组件与主题契约见 [BerryUI-F4组件第一版-v1.1.md](BerryUI-F4组件第一版-v1.1.md) 与 [BerryDesign-F3主题与基元-v1.0.md](BerryDesign-F3主题与基元-v1.0.md)。本文只描述已落地的 `uitest` 验收面；Surface Mode、IME、a11y sidecar 与安全区的协议词表已由 P0 冻结。

## §1 验收目标

F5 把 `uitest` 从 F1 的零像素 Berry Room probe 升级为第一个真实 BerryUI 消费者。它必须同时证明：

```text
F4 13 组件能在 guest 进程内组合
ThemeMetrics 在 1080×1920 得到 Product 几何
Button/Switch/ListItem 状态变化走保留树 set_props
ScrollView offset clamp 真实生效
返回栈语义由 app 状态驱动并能真实换页
AST encode、layout dump、hit-test dump 可观测
BerryUI DisplayList 能被 Berry Room 光栅化并提交
QEMU 截图像素来自主题 token，而不是零页或手写像素
owner-local 输入经 BerryUI hit/gesture 后驱动业务状态
销毁清理与 WM 空表语义不回退
```

F5 不新增第二套 GUI 框架，也不迁移 Browser/Settings/Files/Photos/Contacts。

## §2 验收 app 结构

`user/bins/uitest` 的依赖面固定为：

```text
berry-room（PointerEvent / phase / 输入容量词表由 Room re-export）
berry-ui（features = ["guest-font-vmo"]）
berry-gfx CommandEncoder/GlyphPos（组合 DisplayList，不写像素）
```

页面 0 覆盖 F4 全组件：

```text
Scaffold + AppBar + ScrollView
Card + Column + Row
Text Title/Body/Caption
Icon id=7（2×2 ARGB 真供件）
Button Details
ListItem Owner-local input / WM routed
Switch
Divider
EmptyState
```

页面 1 覆盖返回栈：

```text
Scaffold + AppBar Details
Card + Title + selected ListItem
Button Back
EmptyState
```

页面 0 内含一个真实滚动样本：单行 ListItem 比小 viewport 高 24px，Product 与 Compact 密度下 `sync_bounds` 后 `scroll_by(24)` 的实际 offset 均为 24；app 断言该值后才打印 scroll marker。C1 起该样本升级为真实结构性 Clip 溢出/命中/几何验收，不再以“内容刚好放得下”替代 clipping。

业务状态归 app：

```text
page_index: 0 | 1
switch_on: bool
frame_seq: u32
```

组件只把 `Msg` 映射为视觉态；`Click` 后由 app 换页或翻转业务状态。

## §3 Berry Room 渲染与系统字形

Berry Room 新增：

```rust
AppRoom::render_display_list(&DisplayList) -> RoomResult<usize>
```

职责边界：

```text
BerryUI 发射 DisplayList
Berry Room 取得 frame VMO 借用
Berry Room 创建 TextureDesc / soft Queue
Berry Room 光栅化并返回命令数
app 不知道 stride、frame 指针或像素循环
```

`SystemFontAtlas` 把 wmd per-app 32×48 ASCII A8 字形 VMO 适配为 `GlyphAtlas`：

```text
cell = 32×48
ASCII 32..=126
coverage 直接读 FONT_VA
glyph 1-bit 面缺席，raster coverage_cp 优先路径消费 A8
```

BerryUI 新增 `FontSource` 供给槽：

```rust
pub trait FontSource {
    fn atlas(&self, scale: FontSize) -> Option<&'static dyn GlyphAtlas>;
}
```

默认 host 面继续使用 berry-gfx 静态图集。`guest-font-vmo` feature 编译掉 `FontAtlas32x48` fallback，客机必须注入 `FontSource`。`uitest` 的 S32 文本全部走 `SYSTEM_FONT_ATLAS`，避免把约 145KB HD A8 表和 CJK 分页数据复制进 app 包。

EmptyState 说明文本使用 `TextVariant::Body`，保证 guest 页面不拉入 S24 静态表；F4 host widget 测试仍单独覆盖 Caption 排版档。

`uitest` runtime stack 为 48 页，容纳 `Ui<128>`、192 glyph scratch、256 command DisplayList 与 Berry Room 光栅化调用栈。Present 失败拆出 `SubmitAck` / `FrameDone` / `ReleaseReply` 错误面，宿主诊断不再折叠成单一 Decode。

## §4 boot-probe 验收

`RoomConfig.boot_probe` 支路无输入、无 QMP 假点击，执行真实保留树与帧链：

```text
初始 page0 layout
hit-test Button
AST encode
render + submit
Switch set_checked(true)
ScrollView sync_bounds
ScrollView scroll_by(24)
offset == 24 断言
render + submit + UITESTSCROLL marker
Button Click 推入 page1
重建树 + render + submit
Button Click 弹回 page0
重建树 + render + submit
重置 runtime 初态 page0/switch=false
render + submit
destroy + WM 空表回读
```

标记面：

```text
UITESTLAYOUT ok page=...
UITESTHIT ok node=button x=... y=...
UITESTAST ok bytes=... a11y=...
UITESTRENDER ok seq=... cmds=...
UITESTFRAME ok
UITESTSCROLL ok offset=24 switch=1
UITESTSTATE ok page=1 switch=1
UITESTBACK ok page=0 switch=1
UITESTPROBE ok
UITESTDESTROY ok
UITESTEXIT ok
```

2026-11-08 镜像实测：AST 690B，a11y 8 项；boot-probe 提交 5 帧。

## §5 runtime 验收

`tools/gui8-smoke.py --normal-boot` 的 F5 段执行：

```text
launcher 网格真实 APP_SPAWN 启动 UITEST
等待初始 UITESTFRAME + layout/hit dump
QEMU 截图并断言主题像素
QMP 点击 Details Button 中心
等待 page1 layout/state/render
QEMU 截图并断言 page1 像素
QMP 点击 Back Button 中心
等待 owner-local INPUT + page0 BACK + DESTROY/EXIT
```

2026-11-08 实测几何与输入：

```text
AppBar = (0,0,1080,60)
body   = (0,60,1080,1860)
Details Button = (12,126,1056,60)，点击 (540,156)
Back Button    = (12,255,1056,60)，点击 (540,285)
PhaseUP owner-local = (539,284)
```

初始页像素锚：

```text
(540,20)  = primary       (0,116,224)
(540,60)  = surface       (255,255,255)
(540,100) = on_surface 文本 (27,37,64)
(540,600) = surface_variant (245,247,250)
```

page1 继续断言 AppBar primary 与 Title 文本像素。runtime 提交 3 帧：初始、Details 换页、Back 换页。

`gui8-smoke --self-test` 使用同 marker 语料做负样本：缺输入、缺真实帧、缺真实滚动 marker、缺 hit dump、输入早于快照均 fail closed。

## §6 回归与预算

已通过：

```text
berry-ui 默认面
berry-ui --features s7
berry-ui no_std build
berry-ui guest-font-vmo check
berry-room no_std check
uitest no_std build
just test-host
just abi-check
cargo deny licenses
gui8-smoke --self-test
gui8-smoke --layout-selfheal
gui8-smoke --normal-boot
smoke-image 138/138
docs_guard
Curia strict
writable_globals
```

F5 预算：

```text
rootfs   = 33,553,480B
单卷 cap = 33,554,432B
单卷余量 = 952B
总量 cap = 134,217,728B
UITEST.bap = 64,416B
stack_pages = 48
```

F4 rootfs 为 33,502,608B；F5 增量 50,872B。系统字形 VMO 档使 `UITEST.bap` 保持在 64KB 内；若后续组件面扩张导致超帽，先裁特性或泄漏面，不得把静态 HD/CJK 表复制回 app。

最终 F5 镜像 SHA-256：

```text
96cf19ef13cf5bab1f907f7bd563711d5079242ea848bc107d1c5890c8a192c1
```

## §7 未完成边界

- C1 起ScrollView/嵌套 Clip 已有结构性裁剪；复杂嵌套滚动交互与动画裁剪优化仍随后续 BerryUI 刀演进。
- 返回栈是 `uitest` 的 app-local 两页状态，不是 OS 导航服务或 Berry Home 任务栈。
- a11y 已进入 BerryUI 保留树与 AST 语义表；P0 冻结 semantic sidecar 记录 codec，系统级 sidecar 服务与批量传输仍未接线。
- safe area、scale factor、input class、IME composition 与 Surface role/mode 已由 P0 立法；F5 时点 buffer 接线未开放，P1 已完成授权 Canvas/Immersive 生产链。
- `.bui` 仍不解析；F5 继续使用 Rust typed `berry_ui::sdk`。
- Browser/Settings/Files/Photos/Contacts 仍是迁移前基线，归 F6..F8。
