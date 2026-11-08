# BerryUI F4 组件第一版

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI F4 的组件 API、组合基元、状态语义与测试证据。执行序权威 = [GUI框架地基路线-v1.4.md](GUI框架地基路线-v1.4.md)；core/诊断与 SDK 最小面见 [BerryUI-F2核心盘点-v1.0.md](BerryUI-F2核心盘点-v1.0.md)，主题与像素 token 见 [BerryDesign-F3主题与基元-v1.0.md](BerryDesign-F3主题与基元-v1.0.md)。本文只定义普通 BerryUI 应用路径；CanvasSurface / immersive surface 归 P0 后续协议。

## §1 组合基元

F4 新增 `ViewKind::Panel` 与 `PanelProps`：

```rust
PanelProps {
    bg: Color,
    w: Option<u32>,
    h: Option<u32>,
    padding: u32,
    gap: u32,
    radius: u32,
}
```

Panel 是可含子的纯色背景容器：

```text
纵向聚合子内容
四向 padding
子间 gap
w/h 可定值；w=None 随父，h=None 随内容
radius=0 发射 FillRect；radius>0 发射 FillRoundRect
radius=0 才作为完整遮挡源参与 cull（圆角透明边保守不计）
hit test 按完整 bounds 参与
```

AST add-only 扩展：

```text
kind = 10
props = 23B
bg 3B + w/h 哨兵对 + padding/gap/radius
HAS_SIZE 扩展块拒（双源歧义）
```

既有 kind 0..=9 的旧流不变；未知 kind 拒面顺延为 11。Panel 不吃 `HAS_SIZE` 块，尺寸只在 props 定长域，避免双源歧义。

## §2 SDK 面

组件统一从 `berry_ui::sdk` 消费：

```text
WidgetContext
Scaffold
AppBar
Card
Button
ListItem
Switch
ScrollView
EmptyState
TextVariant
column
row
text
icon
divider
```

`WidgetContext` 持有 `Theme + ThemeMetrics`。组件构造只做：

```text
组合既有 ViewSpec/Props
应用 Theme token
登记 a11y role/label
返回稳定 NodeId
```

组件不做：

```text
WM RPC
VMO / stride / framebuffer
业务回调
BUI 解析
动画调度
自有布局算法
```

## §3 组件语义

| 组件 | 结构 | 状态 / 输入 | 测试面 |
|---|---|---|---|
| Scaffold | Col + AppBar + body Col | body 高度 = viewport − AppBar | 800×600 钉板 `AppBar=(0,0,800,56)`、`body=(0,56,800,544)` |
| AppBar | Panel(primary) + Title | 无独立状态；Title 使用 on_primary | 布局、a11y、离屏主题色锚 |
| Card | Panel(surface) + Radius::Medium + padding/gap | 无独立状态；内容由调用方添加 | 布局与离屏 surface 像素锚 |
| Column | Col + Spacing::S | 无独立状态 | 子节点纵向摆位 |
| Row | Row + Spacing::S + CrossAlign::Center | 无独立状态 | Icon 间距与 topmost hit |
| Text | Text + TextVariant | Title/Body/Caption | 排版档、a11y、标题像素锚 |
| Icon | Icon + primary + Spacing::L | 像素经 IconSource | 16px Compact 尺寸、四角 SmoothArgb 像素锚 |
| Button | Panel + Radius::Small + Text + a11y button | Normal/Pressed/Disabled/Selected；Disabled 忽略 Press/Click；Click 返回 true，业务动作仍归 app | a11y、Disabled 拒绝、Click、离屏状态色 |
| ListItem | Panel + title + detail + a11y list item | Normal/Selected | 标题/说明布局、选中态、离屏像素 |
| Switch | pill-radius Panel + Row + spacer + Circle + a11y toggle | checked/disabled；Checked 用 Selected token；Click 可翻转视觉态；Disabled 忽略输入 | Compact 52×28、travel 28、a11y on/off、选中色 |
| Divider | Rect(outline) + Thin stroke | 无独立状态 | Compact 1px / Product 2px |
| Scroll | Panel + content Col + offset | DragStart/DragMove/DragEnd；offset clamp 到 content−viewport | 150−80 内容 clamp 20→70、bounds 随 offset |
| EmptyState | Panel + centered Col + Icon/Title/Caption | 无独立状态 | 图标、标题、说明均非零并参与离屏 |

## §4 状态与输入边界

Button 与 Switch 不携带业务回调。`on_msg` 只识别自身 Panel、文本或 thumb 子节点：

```text
Press → 应用 Pressed 视觉
Release/Cancel → 恢复基础视觉
Click → 返回 true，由 app 更新业务状态
Disabled → 直接 false
```

业务状态由 app 持有；组件方法只把状态映射到 Berry Design token 与保留树。这样保留树、damage、hit test 与 a11y 仍由 core 单源执法。

ScrollView v1 使用 `Ui::set_offset` 移动内容，并按 `content.h − viewport.h` clamp。当前 core 无 clipping：溢出内容在 Panel 外仍可见。F5 验收滚动必须在视口内布局；跨视口裁剪与嵌套 scroll 语义归 P0 Surface Mode 后续协议，不在组件内私造 clip。

## §5 测试证据

| 门 | 文件 / 用例 |
|---|---|
| Panel core + AST | `tests/widgets.rs::panel_is_drawn_container_and_roundtrips_ast` |
| RoundRect 命令 | `berry-gfx/tests/round_rect.rs`：命令回放与 direct surface 逐字节一致；dirty clip 外零写 |
| Scaffold/核心组件/状态/a11y | `tests/widgets.rs::scaffold_core_widgets_layout_state_and_a11y` |
| Row/Icon/EmptyState/hit | `tests/widgets.rs::row_icon_empty_state_and_hit_truth` |
| Scroll clamp/drag | `tests/widgets.rs::scrollview_clamps_drag_and_updates_bounds` |
| 文本容量拒面 | `tests/widgets.rs::widget_text_over_cap_is_loud` |
| @2x/@3x 组件几何 | `tests/widgets.rs::widget_geometry_scales_from_compact_to_product` |
| 13 组件同页离屏 golden | `tests/widgets_golden.rs::all_f4_widgets_share_one_offscreen_golden_page` |

离屏 golden 使用真实：

```text
Ui 保留树
ThemeMetrics
Panel paint
Text glyph run
IconSource SmoothArgb
berry-gfx soft Queue
```

并断言：

```text
AppBar / ScrollView / Card / Button / Divider / Switch / ListItem / EmptyState 像素锚
FillRoundRect 命令真实在场
Icon 四角像素
Title 背景与 on_primary 字形像素
overdraw ≤ OVERDRAW_BUDGET_X256
```

BerryUI 默认面、S7 面、no_std 构建均通过。

## §6 预算

F4 后 guest app 尚未消费 Widget 模块，LTO 裁剪生效：

```text
rootfs = 33,502,608B
cap    = 33,554,432B
余量   = 51,824B
```

五个迁移前 Native Shell 包体：

```text
BROWSER.bap  = 31,804B
SETTINGS.bap = 27,520B
FILES.bap    = 27,912B
PHOTOS.bap   = 27,448B
CONTACTS.bap = 31,308B
```

F5 uitest 首次真实消费组件后再输出新的包体与内存账；若超帽，先裁组件特性或泄漏面，不得降低视觉资产质量。

## §7 边界

- F4 不实现桌面 Dock、树表、虚拟化列表或命令面板；归后续桌面专业件刀。
- F4 不实现 CanvasSurface / immersive surface；归 P0。
- F4 不解析 `.bui`；Rust typed API 是唯一入口。
- F4 不迁移 Settings / Files / Contacts / Photos / Browser；归 F6..F8。
- F4 不新增 guest app；F5 uitest 承接首个真实消费者。
- ScrollView 不私造 clip；裁剪语义归 P0 Surface Mode。
