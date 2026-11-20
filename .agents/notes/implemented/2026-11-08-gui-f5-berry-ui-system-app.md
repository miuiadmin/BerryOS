# GUI F5 BerryUI 系统验收落地

Status: implemented — 2026-11-08

## Problem

F4 组件只有 host 与离屏证据，`uitest` 仍是 F1 零像素 Berry Room probe。GUI 路线要求 F5 让第一个 guest 真实消费 BerryUI 组件，并同时证明主题、滚动、状态、返回栈、布局/命中 dump、真实 frame、截图像素与 owner-local 输入。直接把 `berry-ui` 链进 guest 后，静态 `FontAtlas32x48` 及其 CJK 分页闭包把 `UITEST.bap` 推到 722,664B、rootfs 推到 34,212,112B（超帽 657,680B）；这不是可接受的“先落再优化”。

## Decision

按分层落三条缝：

1. BerryUI 增加 `FontSource` 供给槽与 `guest-font-vmo` feature。host 默认面继续用静态图集；guest feature 编译掉 `FontAtlas32x48` fallback，app 必须按字号供给图集。
2. Berry Room 增加 `SystemFontAtlas` 与 `AppRoom::render_display_list`。前者适配 wmd per-app 32×48 ASCII A8 字形 VMO，后者在 Room 内创建 TextureDesc/soft Queue 并光栅化 DisplayList；app 不知道 stride 或 frame 指针。
3. `uitest` 启用 guest feature，S32 文本走 `SYSTEM_FONT_ATLAS`，EmptyState 说明文本改用 Body 档，避免拉入 S24 静态表。最终 `UITEST.bap = 64,416B`，rootfs 余量 952B。

`uitest` page0 消费 F4 全部 13 组件；真实滚动样本在小 viewport 上断言 offset=24，boot-probe 程序化验证 Switch、ScrollView、push/pop 换页与 5 次真实 submit；runtime 由 GUI8 normal 注入两次真实 QMP 点击，验证 Details/Back 返回栈、owner-local PhaseUP、截图像素锚、AST/layout/hit marker 与销毁清理。

## Alternatives considered

- 让 `uitest` 直接链接静态 HD 图集：首包 722,664B 且 rootfs 超帽，且每个迁移 app 都会复制一份昂贵字表；否决。
- 让 BerryUI 依赖 Berry Room 或在组件层映射 VMO：反向依赖破坏 GUI 分层；否决。
- 保留零像素 probe，仅用 F4 host golden 代替 F5：不满足路线“真实 guest 消费者 + 帧截图 + 输入命中”出口门；否决。

## Consequences

- `berry-room` 合法依赖 `berry-gfx` 作为渲染宿主；`berry-ui` 仍不知道 WM/VMO/stride。
- F6 迁移 Settings 时也必须启用 `guest-font-vmo` 或提供等价 FontSource，不得把 HD/CJK 静态表复制回 app。
- rootfs 余量仅 952B，任何 GUI app 扩张都必须先重出包体账；超帽时裁特性或泄漏面，不降视觉资产质量。
- ScrollView 仍无 clipping；返回栈仍是 app-local 状态。二者不因 F5 通过而提前立法，后续归 P0 Surface Mode 与 Berry Home 任务语义。
- 证据与现行边界见 [BerryUI-F5系统验收-v1.0](../../../docs/BerryUI-F5系统验收-v1.0.md)。
