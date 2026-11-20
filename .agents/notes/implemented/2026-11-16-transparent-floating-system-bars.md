# 透明悬浮系统栏与Safe区色延伸

Status: implemented

## Problem

用户指出第一版 edge-to-edge 系统栏仍把三大金刚放进深色全宽 scrim：这不是 Android 的观感。Android 的导航图标悬浮在透明背景上，背景跟随桌面或应用底栏；应用必须为摄像头/状态栏和导航键留出 safe area。iOS 从第一天把安全区作为布局契约，Android 因历史遗留依赖适配。用户同时要求未来液态玻璃悬浮底栏也遵守同一安全区，不能因系统导航键可见而压住应用交互面。

第一版实现的另一个视觉问题是 Scaffold 的 safe_top/safe_bottom 只是空白布局节点。窗口虽已 edge-to-edge，但应用帧在 safe 带保留初始黑色，系统 overlay 再叠 scrim，最终仍呈现黑块。

## Decision

### 系统栏背景完全透明

status/navigation 两个 SYSTEM overlay 的背景 alpha 改为 0：

```text
status background alpha = 0
navigation background alpha = 0
```

wmd 只重混合 glyph alpha。桌面时背景是壁纸；应用时背景是应用自己的 chrome 色。系统不再绘制全宽黑块、渐变或半透明底带。

AOSP 三键与状态资产保留原始 alpha 形状，统一 tint 为高对比 `0x1B2540`，在浅色 app chrome 与彩色壁纸上均可见。AOSP VectorDrawable 的 `singleToneColor` 本来就是消费方 tint，不改变原始几何。

### Scaffold safe区颜色延伸

Scaffold 的 safe_top/safe_bottom 不再是透明洞：

```text
top safe reserve    -> AppBar surface 色
bottom safe reserve -> 应用 bottom chrome surface 色
```

几何不变：

```text
body_h = viewport_h
         - safe_top
         - app_bar_h
         - bottom_h
         - safe_bottom
```

因此：

- AppBar 一体色延伸到摄像头/挖孔下方。
- AppBar 文字、返回键和 action 仍从 safe_top 之下开始。
- 应用底栏/Tabs 色延伸到屏幕底。
- Tabs、按钮、列表和未来悬浮玻璃面板的可交互面仍被抬高到 safe_bottom 之上。
- 液态玻璃底栏只能在 app bottom slot 内画圆角、margin 和玻璃面；safe_bottom 只允许背景色延伸，不允许放交互控件。

## Alternatives considered

- 保留 alpha=0xD8 的导航 scrim：仍是用户明确反对的“大黑块里放图标”，拒绝。
- 只让窗口 raw 全屏、Scaffold safe区留空：窗口初始黑色会露出，视觉上仍像系统黑边，拒绝。
- 把 `safe_bottom` 并入 bottom slot 高度：会丢失系统权威 inset 与 app 底栏高度两个语义，导致液态玻璃无法区分“悬浮留白”和“系统导航避让”，拒绝。
- 让系统 overlay 自绘玻璃底栏：应用底栏属于 app，系统只拥有三键/状态 glyph 与 safe area 真值，拒绝。
- 为可读性恢复局部 pill 背景：第一版先保持纯透明悬浮；若真实壁纸对比度不足，后续玻璃/对比策略必须作为显式控件或系统主题规格，不悄悄回到全宽黑块。

## Consequences

BerryUI host 钉板新增离屏渲染断言：

```text
y=0    == AppBar surface
y=180  == AppBar surface
y=1815 == bottom slot surface
y=1919 == bottom safe extension surface
```

产品 e2e 从“白色 glyph + 深色 scrim”改为：

- 桌面/应用状态区均出现 6000..8000 个系统 ink 像素。
- 桌面/应用导航区均出现 15000..22000 个 AOSP ink 像素。
- 桌面导航区黑色像素少于 1000，证明无系统黑块。
- 应用导航区白色 surface 背景超过 90000 像素，证明底栏应用色连续延伸。

Photos explicit immersive、Recents/HOME、frame-cache 与 LTPO 链保持不变。真实挖孔形状、动态 status foreground 深浅色切换和液态玻璃材质控件仍未完成。

## Related

- [透明悬浮系统栏与安全区规格](../../../docs/Berry设备平台-TransparentFloatingShell-v1.4.md)
- [Safe区色延伸控件规格](../../../docs/BerryUI-TriPlatformControls-v1.15.md)
- [Android 沉浸系统栏与软件 LTPO](2026-11-16-android-immersive-bars-ltpo.md)
