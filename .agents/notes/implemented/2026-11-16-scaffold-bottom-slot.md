# Scaffold bottom 槽与 BerryDemo 六页迁移

Status: implemented

## Problem

BerryDemo 六页虽然视觉上有持久底栏，但高度在 app 侧手写：Scaffold 只提供 AppBar/body，`page_body` 再建一层带间距的 column，手动做 `viewport - safe_top - app_bar - tab_h - safe_bottom` 减法。该层 column 在产品密度下带来 6px gap，使底栏落到 y=1710..1818，越过 SYSTEM navigation reserve 顶界 1812。Scaffold bottom 槽也是三方控件矩阵中未闭合的骨架缺口。

## Decision

Scaffold 新增固定高 bottom 槽：

```text
new_light_bottom_safe
with_back_bottom_safe
body_height()
bottom: Option<NodeId>
```

树序固定为：

```text
safe_top reserve
→ AppBar
→ body
→ bottom slot
→ safe_bottom reserve
```

`bottom_h` 叠加在 Berry Room `safe_bottom` 预留带之上，禁止把栏高并入 `safe_bottom` 双扣。Scaffold 只拥有几何槽，不建 Tabs/ToolBar、不持栏选中态；栏内容与 safe_bottom floor 策略仍归 app。

BerryDemo 六页全部迁移到该槽。`page_body` 删除手抄高度链与内层 column，ScrollView 使用 `scaffold.body_height()`，Tabs 直接挂 `scaffold.bottom`。产品几何变为 body 285..1704、bottom/Tabs 1704..1812，修正旧实现 6px 越界。

## Alternatives considered

- 继续由 app 手写减法：六页重复且已产生 6px 系统带越界，拒绝。
- 把底栏高度并入 `safe_bottom`：会把 Berry Room 权威系统预留与 app 栏高混成两个语义，拒绝。
- 让 Scaffold 直接创建 BottomNav/ToolBar：栏内容、选中页和业务归 app，Scaffold 代持会破坏状态所有权，拒绝。
- 在 bottom 槽中保留 6px 间距以冻结旧像素：等于保留越界 bug，拒绝。

## Consequences

Host 钉板覆盖 1080×1920 产品几何、Tabs 与槽同位、ScrollView 与 body 同位、light/back 两种形态、`bottom_h=0` loud 拒绝和夸张 inset 饱和。BerryDemo 产品 e2e 复跑验证六页生命周期与底栏点击。既有 safe Scaffold 测试不改一行，作为旧构造器零漂证据。

Tab badge、滚动隐藏底栏、SafeInsets 构造签名收敛与 PC ToolBar 组合件仍未完成；本刀只落几何槽。

## Related

- [三方共性控件体系](2026-11-15-tri-platform-controls.md)
- [产品 e2e rig 收编](2026-11-16-product-e2e-rig.md)
- [Card、Dock 与 Recents 按压反馈](2026-11-16-card-dock-recents-press-feedback.md)
