# Berry Design D2 Token Pair Contract

> 版本：**1.0**｜2026-11-12
>
> 定位：DESIGN-D2 现行实施规格。执行序权威 = [GUI框架地基路线-v1.19.md](GUI框架地基路线-v1.19.md)；F3 兼容基线 = [BerryDesign-F3主题与基元-v1.0.md](BerryDesign-F3主题与基元-v1.0.md)；研究依据 = [../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md](../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md) §6.5/§9/E5；API 治理 = [BerryUI-SDKS0ControlSpecAPIGovernance-v1.0.md](BerryUI-SDKS0ControlSpecAPIGovernance-v1.0.md)。
>
> 核心裁决：**视觉背景 token 必须成对携带前景，并进入机器对比度夹具**。F3 light 色值与 state 表字节冻结；D2 以 additive API 增加 scheme 与 pair 代数，不迁移五 app 暗色皮肤。

---

## §1 范围与非目标

### 做

```text
ColorPair{background, foreground}
ColorPairToken
ColorScheme{Light,Dark,HighContrastLight}
ColorSchemePreference
WCAG contrast ratio / level
light + dark + high-contrast palette
background/surface/surface_variant/primary/status pairs
normal/pressed/disabled/selected/focused state pairs
quarter-vp effective-pixel grid
离散 FontSize text-scale ladder
Theme 切换保持几何不变
design token ledger + just check 机器门
```

### 不做

```text
五 app dark/high-contrast 皮肤迁移
Material HCT 复制
动态取色 / OEM 品牌主题
连续字号乘法
运行时系统主题设置服务
R3 新 theme event
elevation/shadow token
连续 scale_permille 密度
AST/wire/ViewKind/Props/Msg 变更
PRESSED_BG 与 swipe 圆点遗留常量字节迁移
```

Disabled pair 进入 matrix，但按 WCAG inactive-component 豁免 4.5:1；这不是隐式跳过，账本必须显式 `level="inactive"`。

---

## §2 单源与兼容

单源关系：

```text
theme.rs          Rust const/type 单源
design_tokens.toml    pair/scheme/对比度等级账本
design_ledger.py    双向/对比度/字面量治理
本文件               人读合同 + generated matrix
gui8-smoke.py       像素锚只消费 UITESTTOKEN dump，不再手抄第二份色值
```

F3 兼容承诺：

```text
Theme::light() 11 个既有色值字节不变
Theme::state() light 5 组 state 色字节不变
ThemeMetrics spacing/radius/typography/stroke 输出不变
现有 WidgetContext / widget 调用形状不变
现有 golden 与 AST bytes=860 / a11y=9 不变
```

`Theme` 增加 `scheme` 字段用于 additive scheme dispatch；仓库内所有调用均经 `Theme::light()`/`Theme::for_scheme()`，无手写 `Theme {}` 字面量。

---

## §3 Pair 代数

```rust
pub struct ColorPair {
    pub background: Color,
    pub foreground: Color,
}

pub enum ColorPairToken {
    Background,
    Surface,
    SurfaceVariant,
    Primary,
    Success,
    Warning,
    Danger,
}
```

规则：

```text
新增背景/status token 必须 bg+fg 同时入账
F3 legacy ThemeColors 保留为兼容薄面
新代码优先 Theme::pair(token)
状态 pair = StateColors.background + StateColors.content
border 是非文字描边，不强制 4.5:1
```

Status token 的 light 前景采用可读深墨/白色；light status 背景色本身不改，保证既有像素零漂。

---

## §4 Scheme 与对比度

```rust
pub enum ColorScheme {
    Light,
    Dark,
    HighContrastLight,
}

pub enum ColorSchemePreference {
    System,
    Light,
    Dark,
    HighContrastLight,
}

pub fn resolve_color_scheme(preference, system) -> ColorScheme
```

解析是纯函数；v1 不新增系统配置通道。系统偏好生产者出现后，由宿主启动快照传入。

对比度：

```rust
pub enum ContrastLevel {
    NonText,   // >= 3.0:1
    Text,      // >= 4.5:1
    Inactive,  // explicit WCAG inactive exemption
}

pub const fn contrast_ratio_centi(pair: ColorPair) -> u32
pub const fn contrast_meets(pair: ColorPair, level: ContrastLevel) -> bool
```

实现为 256-entry sRGB linear LUT + Q16 整数 WCAG 数学，零浮点、零分配、no_std。黑白参考值恰为 `2100`（21.00:1）。

### Generated pair matrix

<!-- BEGIN generated:design-token-pairs -->

| scheme | token | background | foreground | contrast | level |
|---|---|---|---|---:|---|
| dark | background | `#0f141a` | `#edf2fa` | 16.46:1 | text |
| dark | danger | `#ff8a95` | `#2a0608` | 8.26:1 | text |
| dark | disabled | `#1e252f` | `#8e9aab` | 5.41:1 | inactive |
| dark | focused | `#263447` | `#e5f0ff` | 10.96:1 | text |
| dark | normal | `#222b38` | `#e7eef9` | 12.24:1 | text |
| dark | pressed | `#2d3a4c` | `#dce8fa` | 9.32:1 | text |
| dark | primary | `#8abeff` | `#0f1724` | 9.32:1 | text |
| dark | selected | `#31415c` | `#dceafb` | 8.43:1 | text |
| dark | success | `#62d8b5` | `#071b16` | 10.20:1 | text |
| dark | surface | `#1a212b` | `#edf2fa` | 14.41:1 | text |
| dark | surface_variant | `#242d3a` | `#edf2fa` | 12.36:1 | text |
| dark | warning | `#ffc66d` | `#221200` | 11.74:1 | text |
| high_contrast_light | background | `#ffffff` | `#000000` | 21.00:1 | text |
| high_contrast_light | danger | `#9e1b27` | `#ffffff` | 7.95:1 | text |
| high_contrast_light | disabled | `#f3f4f7` | `#5a626e` | 5.60:1 | inactive |
| high_contrast_light | focused | `#e1ebff` | `#001040` | 15.27:1 | text |
| high_contrast_light | normal | `#ffffff` | `#000000` | 21.00:1 | text |
| high_contrast_light | pressed | `#e7eefa` | `#001a4d` | 14.35:1 | text |
| high_contrast_light | primary | `#003e9c` | `#ffffff` | 9.68:1 | text |
| high_contrast_light | selected | `#dce6ff` | `#001444` | 14.20:1 | text |
| high_contrast_light | success | `#00694f` | `#ffffff` | 6.70:1 | text |
| high_contrast_light | surface | `#ffffff` | `#000000` | 21.00:1 | text |
| high_contrast_light | surface_variant | `#f2f5fa` | `#000000` | 19.22:1 | text |
| high_contrast_light | warning | `#8a4b00` | `#ffffff` | 6.80:1 | text |
| light | background | `#f5f7fa` | `#1b2540` | 14.13:1 | text |
| light | danger | `#d02f3d` | `#ffffff` | 5.05:1 | text |
| light | disabled | `#f0f2f5` | `#a6adba` | 2.01:1 | inactive |
| light | focused | `#e9f2ff` | `#0b2554` | 13.23:1 | text |
| light | normal | `#ffffff` | `#1b2540` | 15.16:1 | text |
| light | pressed | `#e1eaf8` | `#172b49` | 11.72:1 | text |
| light | primary | `#0074e0` | `#ffffff` | 4.59:1 | text |
| light | selected | `#d6e4ff` | `#102a66` | 10.61:1 | text |
| light | success | `#008b6f` | `#000000` | 4.92:1 | text |
| light | surface | `#ffffff` | `#1b2540` | 15.16:1 | text |
| light | surface_variant | `#e9edf3` | `#1b2540` | 12.90:1 | text |
| light | warning | `#fb8c00` | `#1b2540` | 6.39:1 | text |

<!-- END generated:design-token-pairs -->

---

## §5 Text scale 与 effective pixel

```rust
pub enum TextScale {
    Standard,
    Large,
    Maximum,
}
```

规则：

```text
只映射既有离散 FontSize 档
不做连续百分比乘法
到达当前字形资产上限时饱和
消费者必须触发 measure/layout/damage
```

Product Body 已在 S32x48 资产档，Large/Maximum 饱和为 S32x48；这不是降级，而是防止伪造不存在的线性字号导致 guest 空绘。

Effective pixel：

```rust
ThemeMetrics::effective_px(quarter_vp)
```

规则：

```text
布局恒物理像素
Density 只负责 @2x/@3x 换算
spacing/radius 继续 quarter-vp 存储
四舍五入单源：(quarter * density + 2) / 4
组件禁止私算线性像素
```

---

## §6 Theme 切换与 R3 兼容

D2 不新增 WM/config wire。主题变化仍属于 app 内 UI 状态：

```text
1. app 取新 Theme/WidgetContext
2. widget setter 走既有 set_props
3. set_props 产生既有 paint damage
4. relayout 仅在几何 token 变化时发生
```

Host 钉板：

```text
light -> dark
Button bounds 不变
Normal pair 改变
dark Normal pair >= 4.5:1
```

---

## §7 AST / wire 合同

```text
ViewKind 零新增
Props 零新增
Msg 零新增
Node 19 字段 / 144B 零漂
AST bytes=860 / a11y=9 零漂
P0 / R3 wire 零修改
不序列化 Theme / scheme / preference
```

颜色本身仍是 Text fg / Panel bg 的 3B props 值；D2 只改生产来源，不改编码形状。

---

## §8 SDK 面与治理

新增 facade：

```text
ColorPair
ColorPairToken
ColorScheme
ColorSchemePreference
ContrastLevel
TextScale
contrast_ratio_centi
contrast_meets
resolve_color_scheme
```

全部进入 `tools/sdk_api.toml`，since=`DESIGN-D2`，stability=`experimental`，spec 指向本文。

机器账本：

```bash
python3 tools/design_ledger.py --check
```

执法：

```text
36 pair 全量唯一
active text pair >= 4.5:1
Disabled 显式 inactive
ColorPairToken enum 与 TOML 双向一致
TOML RGB 必须物理存在于 theme.rs
theme.rs 之外禁止 Color { r: 0x.. } 字面量
规格 generated matrix byte 对拍
```

`just check` 调用本门。

---

## §9 Host 证据

新增：

```text
user/berry-ui/tests/design_tokens.rs
```

覆盖：

```text
light/dark/high-contrast 全 pair 对比度
normal/pressed/disabled/selected/focused 全 state pair
Disabled 显式 inactive 豁免
黑白 / primary 参考比率
ColorSchemePreference 纯解析
三 scheme 互异
离散 FontSize text-scale ladder
quarter-vp effective-pixel 换算
light->dark geometry 不变、视觉 pair 改变
```

---

## §10 UITEST / GUI8 真链

UITEST 输出单一 token dump：

```text
UITESTTOKEN ok scheme=light n=12
background=f5f7fa
surface=ffffff
surface_variant=e9edf3
outline=dce1e8
primary=0074e0
on_primary=ffffff
text_primary=1b2540
text_secondary=4f5d75
success=008b6f
warning=fb8c00
danger=d02f3d
selected=d6e4ff
```

GUI8 normal 恰出现两次（boot-probe + runtime），并以该 dump 驱动六个 BerryUI app 的主题像素锚，消除 `gui8-smoke.py` 内第二份手抄调色板。

既有必须零漂：

```text
AST bytes=860 / a11y=9
GUI-T1 全 marker
SEM-A1 全 marker
layout/render/clip/surface/key route 全计数
```

---

## §11 预算

基线：

```text
UITEST.bap 100,184B
rootfs.cpio 33,921,888B
rootfs0     33,554,432B（恰 32MiB 单卷帽）
rootfs1     367,456B
总量余量   100,295,840B
```

D2 的 dark/high-contrast scheme 保持 additive；六个 BerryUI app 均因链接 berry-ui 有 224..400B 小增量，UITEST 因 token dump +1,856B。rootfs0 无单卷余量，全部增长进入 rootfs1。

---

## §12 最终镜像

```text
SHA-256 = 0dba6d08168cf496190276afc9c6a93cddb7cd4443f2a70c33bc62fe98a13226
```

---

## §13 回归与验收

```text
cargo test -p berry-ui
just test-host
just check
design ledger --check
sdk ledger --check
berry-ui no_std
berry-ui guest-font-vmo no_std
uitest no_std
GUI8 self-test
GUI8 normal-boot
GUI8 dual-boot canonical persistence
smoke-image
cargo deny licenses
docs_guard
Curia strict
writable_globals
git diff --check
```

最终复跑结果：

```text
just test-host                     PASS
just check                         PASS
berry-ui no_std / guest-font-vmo   PASS
uitest no_std                      PASS
GUI8 self-test                     PASS
GUI8 normal-boot                   PASS
GUI8 dual-boot canonical persistence PASS
smoke-image 138/138                PASS
cargo deny licenses                PASS
docs_guard / Curia strict          PASS
writable_globals                   PASS
```

---

## §14 后置

```text
运行时系统主题设置与 R3 event
五 app dark/high-contrast 迁移
多档字形 VMO / 真正连续 text scale
elevation token
连续 scale_permille 密度
HCT / 动态取色
OEM 品牌主题
PRESSED_BG 遗产常量迁移
swipe 圆点视觉常量迁移
完整 motion token 族
```
