# Agent Note: 跨越早期平面磨砂，深度融合 iOS 18/26/27 Liquid Glass 与 OpenHarmony 6.0 光场及一镜到底架构

Status: proposed

## Problem

前一轮立定的 v1.0 视觉规范虽终结了安卓三键与多端散装混搭，但对标基线仅停留在 2024 年的 iOS 18（二维平面高斯毛玻璃、贴底全宽静态 `TabBar`、高悬顶部的搜索框与纯手工转场动画）。而如 [iOS 18/26/27 液态玻璃与 OpenHarmony 5.1/6.0 光场及渲染架构对标深研](../../research/2026-11-18-iOS26至27液态玻璃与OpenHarmony6光场架构对标深研.md) 所示，2025–2026 年 Apple（iOS 26 `Liquid Glass`、iOS 27 流光与意图驱动 UI）与开源鸿蒙（OpenHarmony 5.1/6.0 API 18–20+、HarmonyOS NEXT / 6 智慧光感、Rosen 渲染服务、一镜到底与实况窗）均已完成代际跃迁。若不把这两大现代标杆的最新光学材质、单手交互重构、声明式共享转场与服务端保留树「一多」架构一并吸收，BerryOS 做出时仍将落后最新一代主流系统整整一个身位。

## Proposal

立定 [iOS 与鸿蒙融合液态玻璃及光场交互标准（v2.0）](../../../docs/iOS与鸿蒙融合液态玻璃及光场交互标准-v2.0.md) 升版取代 v1.0，并按「视觉光学与交互取 iOS 26/27 Liquid Glass × HarmonyOS 6 智慧光感最高交集，渲染架构与跨端开发人体工学深度吸收 OpenHarmony 6.0」的正交分工同步升级全套模块规格与二十四刀方案：

1. **材质从「二维高斯磨砂」升格为「六阶液态玻璃与智慧光场（Liquid Glass & Immersive Light）」**：在 Dual-Kawase 模糊与 `1.8x` 饱和度提拉之上，引入 iOS 26 超椭圆边缘凸透镜折射位移（Edge Lensing）、环境色漫溢吸收（Harmony）、随手指/指针实时游走的动态镜面高光与 HarmonyOS 6 触控水面光场涟漪；应用图标支持 `Light / Dark / Clear / Tinted` 四态分层液态玻璃渲染；强制内置 `Reduce Transparency` 无障碍实心高对比降级契约。
2. **导航与控件升格为 iOS 26/27 新一代流体形态**：以离底悬浮、随向下滚动自动流态收缩的**液态玻璃胶囊底栏（Floating Shrinking TabBar / 悬浮页签）**取代传统全宽贴底横条；将高频搜索栏全面下沉至底部单手拇指热区（**Bottom-Reach Search**）；确认框与上下文菜单一律从触发按钮几何轮廓中原位液态膨胀长出（**In-Place Morphing Alert/Menu**）；文本选区配备边缘折射液态放大镜（**Liquid Glass Loupe**）。
3. **吸收 OpenHarmony `geometryTransition`「一镜到底」与 OpenHarmony 6.0 沉浸式布局精髓**：在 `berry-ui` 与 `compositor` 原生打通声明式 `geometry_transition("id")` 共享元素转场，由服务端节点树自动插值跨页面/跨层级的几何与曲率；引入 OpenHarmony 6.0（API 20+）**组件背景安全区默认延伸**与 `LayoutPolicy::{MatchParent, WrapContent, FitAtIdealSize}`，让开发者零手工计算安全区垫块即可写出顶底通透的沉浸页面。
4. **融合「iOS 27 流光灵动岛 × HarmonyOS 6 实况窗」为统一 `Live Capsule`（实况胶囊）**：同一套 `LiveActivity` 状态同时驱动顶部双舱胶囊、全屏观影/终端专注态下的**免打扰弹幕式轻提示**、下拉多级快捷交互实况卡与锁屏叠卡；唤起系统智能体时点亮 iOS 27 同款彩色流光边框并内联展开 `"搜索或交办意图"` 输入栏（直连零截屏语义场景总线）。
5. **底层同构吸收 OpenHarmony Rosen `RSRenderNode` + ArkUI 单节点双脏分离 + 一多双栏占位架构**：客户端通过单节点函数式绑定精准区分 `LayoutDirty` 与 `PaintDirty`，向 `compositor` 提交保留式 `SceneNode` 增量，同时提供等价于 OpenHarmony 6.0 `XComponent` / `lockCanvas` 的交换链自绘直通槽；依托横向/纵向栅格断点与七大自适应原语、`Navigation` 双栏默认占位页及窗口级精准拖拽投送，实现手机竖屏至平板/PC 自由多窗的零割裂演进。

## Alternatives considered

- **方案 A（只参考 iOS 18 传统形态，忽略 iOS 26/27 与 OpenHarmony 6.0）**：否决。全宽静态底栏、顶部遥不可及的搜索框与无折射平面磨砂在 2026 年已显陈旧，且缺失 OpenHarmony 6.0 的安全区默认延伸、`geometryTransition` 与 Rosen 保留树机制会让客户端开发复杂度居高不下。
- **方案 B（机械拼贴两套不同视觉控件，一部分页面画 iOS 风格、一部分页面画鸿蒙风格）**：否决。那会重蹈早期「多端缝合怪」覆辙；正确做法是在视觉物理上统一为同源的「超椭圆 + 液态玻璃折射光场」，在框架人体工学与场景流转上吸收 OpenHarmony 6.0 的声明式底座原语。

## Consequences

- **正面收益**：BerryOS 直接站在 2026 年全球两大最先进终端操作系统（iOS 26/27 与 OpenHarmony 6.0 / HarmonyOS 6）的肩膀上——既有纯正顶级的液态玻璃光学质感与单手拇指流体交互，又具备开源鸿蒙经过大规模验证的高效保留式合成树、一行代码「一镜到底」与极简沉浸式「一多开发」体验。
- **工程增量**：`gfx` 与 `compositor` 着色管线增加超椭圆边缘法线折射偏移与点光源高光算子（纯定点/SIMD 轻量可算，零大内存开销）；`ui-sdk` 增加 `LayoutPolicy`、背景安全区默认延伸、`geometry_transition` 与悬浮收缩胶囊组件。

## Related

- [iOS 18/26/27 液态玻璃与 OpenHarmony 5.1/6.0 光场及渲染架构对标深研](../../research/2026-11-18-iOS26至27液态玻璃与OpenHarmony6光场架构对标深研.md)
- [iOS 与鸿蒙融合液态玻璃及光场交互标准（v2.0）](../../../docs/iOS与鸿蒙融合液态玻璃及光场交互标准-v2.0.md)
- [全面复刻 iOS 18 视觉与流体交互范式裁决](2026-11-18-全面复刻iOS18视觉与流体交互范式裁决.md)
- [gfx 总体设计](../../plans/gfx/design.md)
- [compositor 总体设计](../../plans/compositor/design.md)
- [ui-sdk 总体设计](../../plans/ui-sdk/design.md)
- [desktop-boot 总体设计](../../plans/desktop-boot/design.md)
- [desktop 总体设计](../../plans/desktop/design.md)
- [GUI 与 SDK 及模拟器彻底重构计划](../../plans/2026-11-18-GUI与SDK及模拟器彻底重构计划.md)
