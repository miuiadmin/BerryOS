# Berry Design F3 主题与基元

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI F3 的主题 token、密度换算、交互状态与 BerryGFX 表面基元契约。执行序权威 = [GUI框架地基路线-v1.11.md](GUI框架地基路线-v1.11.md)；BerryUI core 现状 = [BerryUI-F2核心盘点-v1.0.md](BerryUI-F2核心盘点-v1.0.md)。本文只定义 F3 地基，不提前实现 F4 组件。

## §1 分层

```text
BerryUI theme
├── Theme / ThemeColors / InteractionState
├── Density / ThemeMetrics
└── Spacing / Radius / Typography token

BerryGFX surface
├── blend_xrgb
├── fill_round_rect_xrgb
├── fill_circle_xrgb
└── draw_shadow_xrgb

berry-nshfont（迁移期）
├── HD A8 字形 VMO 映射与文本定位保留
└── text/blend/round_rect/circle/shadow 的像素混合复用 BerryGFX
```

BerryUI 只定义视觉语义，不写像素；BerryGFX 只做几何与混合，不认识主题。F3 时点五个迁移前 Native Shell 仍经 `berry-nshfont` 兼容 API 运行；F8 后该临时件已无消费者，F9 删除。

## §2 密度与 token

`Density::for_viewport`：

```text
800×600                    → Compact @2x
1080×1920                  → Product @3x
宽 <1000 或高 <1800       → Compact
```

间距与圆角 token 以 quarter-vp 存储并四舍五入：

| Token | 基准 | Compact px | Product px |
|---|---:|---:|---:|
| Spacing::Xs | 1vp | 2 | 3 |
| Spacing::S | 2vp | 4 | 6 |
| Spacing::M | 4vp | 8 | 12 |
| Spacing::L | 8vp | 16 | 24 |
| Spacing::Xl | 12vp | 24 | 36 |
| Radius::Small | 6vp | 12 | 18 |
| Radius::Medium | 12vp | 24 | 36 |
| Radius::Large | 16vp | 32 | 48 |

排版映射到既有 BerryGFX 字号档：

| Token | Compact | Product |
|---|---|---|
| Display | `S24x48` | `S32x48` |
| Title | `S24x48` | `S32x48` |
| Body | `S16x32` | `S32x48` |
| Caption | `S8x16` | `S24x48` |

`ThemeMetrics` 是密度换算单源；`Theme::spacing/radius/typography` 只是便捷转发。

## §3 颜色与状态

`Theme::light()` 语义色：

| Token | RGB |
|---|---|
| background | `#F5F7FA` |
| surface | `#FFFFFF` |
| surface_variant | `#E9EDF3` |
| outline | `#DCE1E8` |
| primary | `#0074E0` |
| on_primary | `#FFFFFF` |
| text_primary | `#1B2540` |
| text_secondary | `#4F5D75` |
| success | `#008B6F` |
| warning | `#FB8C00` |
| danger | `#D02F3D` |

`Theme::state` 返回背景、内容、边框三件：

| 状态 | background | content | border |
|---|---|---|---|
| Normal | `#FFFFFF` | `#1B2540` | `#DCE1E8` |
| Pressed | `#E1EAF8` | `#172B49` | `#9FC0F0` |
| Disabled | `#F0F2F5` | `#A6ADBA` | `#E3E7EC` |
| Selected | `#D6E4FF` | `#102A66` | `#9DBBFF` |

F4 组件不得复制这些色值；只能经 `Theme::light()`、`ThemeColors` 或 `Theme::state()` 消费。暗色主题后续以同级构造口加入，不改变组件形状。

## §4 BerryGFX 表面基元

`berry_gfx::surface` 面向窗口局部 XRGB 帧：

```text
内存：[B, G, R, X] little-endian
stride：字节
行尾 padding：永不写
混合：8-bit straight alpha
a=0：不写
a=255：直写
1..=254：逐通道四舍五入混合
```

公开 API：

```rust
blend_xrgb
fill_round_rect_xrgb
fill_circle_xrgb
draw_shadow_xrgb
```

不变量：

```text
圆角矩形与圆为 3×3 超采样抗锯齿；
圆的几何中心按像素中心采样；
半径自动夹到宽高半值；
零宽/高/半径为 no-op；
越界与缓冲不足 no-op 或逐像素拒绝，不 panic；
阴影为双层低覆盖圆角近似，不产生不透明写。
```

`berry-nshfont` 的 `text/blend_px/round_rect/circle/shadow` 兼容签名保留，但像素混合与形状算法转发 BerryGFX。F9 删除该 crate 时，调用方可直接迁移到 BerryUI 组件与 BerryGFX API，不再搬第三套算法。

## §5 测试与出口证据

| 出口门 | 证据 |
|---|---|
| Host token | `berry-ui/src/theme.rs` 单测：密度选择、双几何 token 换算、四种交互状态互异 |
| 离屏 golden | `berry-ui/tests/theme_golden.rs`：同一 StateCard 描述在 800×600 与 1080×1920 输出钉板几何与像素锚 |
| 基元单测 | `berry-gfx/src/surface.rs`：XRGB 字节序、alpha 混合、圆角中心/边缘、圆心/边界、双层阴影、usize::MAX 级敌意几何不 panic |
| 既有渲染 | berry-gfx 全套与 berry-ui 默认/S7/no_std 全绿 |
| 迁移回归 | 五个 Native Shell 经 berry-nshfont 复用 BerryGFX 后 GUI8 normal 与 smoke-image 全绿 |

F3 golden 钉板：

```text
800×600：card=(16,16,768,96)，radius=24，Title=S24x48
1080×1920：card=(24,24,1032,144)，radius=36，Title=S32x48
```

状态变化只改变颜色，不改变布局几何。

## §6 预算

F3 后 rootfs 与五个迁移前 Native Shell 包体：

```text
rootfs 33,501,392B；单卷 cap 33,554,432B，单卷余 53,040B；总量 cap 134,217,728B
BROWSER.bap   31,868B
SETTINGS.bap  27,744B
FILES.bap     28,120B
PHOTOS.bap    27,656B
CONTACTS.bap  31,548B
```

`berry-nshfont` 复用 BerryGFX 后不新增每 app 字形/基元拷贝；敌意几何防御使五包合计增 768B、rootfs 较 F2 基线增 1,072B，仍余 53,040B。

## §7 边界

- 不新增 Button/Card/Switch 组件；归 F4。
- 不把 UITEST 扩成组件 showcase；归 F5。
- 不迁移 Settings/Files/Contacts/Photos/Browser 的页面代码；归 F6..F8。五个迁移前 Native Shell 中既有视觉字面量仍是兼容基线，不作为新代码模板。
- Berry Home 的 OpenHarmony 对齐几何（状态栏、网格、Dock）仍由 launcher 自身布局消费；F3 只提供后续组件统一 token，不回写桌面布局。
- 阴影仍是轻量双层近似；复杂模糊与 elevation 体系不提前实现。
