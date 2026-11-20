# GUI C1 BerryUI 嵌套视口裁剪

Status: implemented — 2026-11-08

## Problem

F4/F5/F6/F7/F8/F9 连续记录同一地基债：BerryUI 只有 damage↔viewport、paint culling 和 raster dirty clip，没有 retained 子树 clip。ScrollView 溢出内容可画穿 Panel，Swipe 越界页可压相邻区域，滚出视口的子节点仍可命中。仅靠在 app 里交矩形或整字形丢弃，无法精确裁剪 Text/RoundRect/Image。

## Decision

1. BerryGFX DisplayList 新增 add-only `PushClip{rect}` / `PopClip` 命令；Queue 维护 16 层定容栈，先验失衡/超深并保证零半执行，所有光栅命令按当前交集裁剪。
2. BerryUI 新增结构性 `ViewKind::Clip`（AST kind=11）与 `ClipProps{w,h}`。基 props 0B，尺寸走既有 HAS_SIZE 块；Node 144B、旧 AST kind/flags 流和 UiError/Msg 均不变。
3. BerryUI paint 走查线程化传递有效 clip，用 order[] 哨兵零分配发射 `PushClip → subtree → PopClip`；overflow 回退路同样裁剪。hit 使用同一 clip 链，点在有效 port 外整子树不命中，Clip 容器本身可命中以承载拖拽。
4. `Ui::bounds` 继续返回未裁剪视觉界；新增 `clip_of` 返回有效 clip。`set_offset` 的 before/after 子树包络与旧/新 clip 链求交，滚动 damage 不再放大到 port 外；其它 layout damage 保持安全超集。
5. ScrollView 结构改为 Panel → Clip → content；SwipePager 在 track 外加 Clip port。组件不直接写像素，六个普通 GUI app 无需手写裁剪。
6. F9 gui-boundary-guard 扩展：普通 app 禁止直接调用 berry-gfx `push_clip/pop_clip`，裁剪必须来自 BerryUI 结构性 Clip。
7. BerryUI producer 维持命令水位不变量：每个打开 Clip 预留 Pop 槽；预算不足时跳过整棵 Clip 子树或截尾叶子，绝不出 dangling Push / orphan Pop。

## Alternatives considered

- Ui 侧表 + `set_clip(node)`：clip 不进 AST/DescBuilder，生命周期与重建树脱节，需要复制 a11y 式三重对拍；否决。
- 在 BerryUI emit 时对 FillRect 做几何交集：Text 字形、圆角、图像缩放边缘无法正确部分裁剪；否决。
- 只做 paint 不做 hit：用户会按到不可见内容；必须同刀面世，否决单独实施。
- 把 ScrollView/Swipe 逻辑塞进引擎：违反 app 侧组件状态边界；Clip 只提供结构性 port，滚动状态仍归组件/app。
- 把 damage 立即全面改到 clip 链：容易为残影引入欠 damage；先保证超集，仅对 `set_offset` 公共口收紧。

## Consequences

- 嵌套与兄弟溢出像素可被精确裁剪；滚出视口子树不可命中。
- `ViewKind::Clip` 与 `ClipProps` 进入 BerryUI SDK；`PushClip/PopClip` 进入 BerryGFX DisplayList。
- AST kind=11 为 add-only；未知 kind 拒面顺延到 12，旧流 golden 零漂。
- BerryGFX/BerryUI/六个 GUI app 包体增长；当前 rootfs 总量 33,802,144B，rootfs0 恰单卷帽，rootfs1 247,712B，仍在 A1 总量帽内。
- UITEST 滚动样本改为真实 48px 溢出，并在 offset 24 时输出 `UITESTCLIP` port/item/probe 几何；GUI8 精确断言两次 Clip proof 和十次 render。
- 复杂嵌套滚动、双轴滚动、Transform 动画全面 damage 收紧、轴向/圆角/stencil clip 仍归后续 BerryUI 刀。
- 规格见 [BerryUIClipping-C1嵌套视口-v1.0](../../../docs/BerryUIClipping-C1嵌套视口-v1.0.md)。
