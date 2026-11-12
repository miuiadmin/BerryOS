# BerryUI C1 Component Focus & Navigation 生产接线

> 版本：**1.0**｜2026-11-11
>
> 定位：GUI-14 C1/D7 的第一实现刀。窗口激活/键盘路由仍归 gui-wmd；
> 本刀只在 BerryUI 树内建立 component focus、1D Tab 顺序与激活语义。
> 不做 FormMetrics、SafeArea、2D D-pad、FocusGroup/Restorer 或 Route 栈。
> 执行序权威 = [GUI框架地基路线-v1.18.md](GUI框架地基路线-v1.18.md)。

## §1 背景

R3 后配置事件已经一致，但键盘链在 app 内停止：

```text
inputd → wmd window focus → Room::poll_keys → app
```

BerryUI 原来只有 pointer Msg：

```text
Press / Release / Click / DragStart / DragMove / DragEnd / Cancel
```

没有树内焦点、焦点顺序、焦点视觉或激活语义。

## §2 分层裁决

```text
window activation / seat focus = gui-wmd WmTable.focus
component focus                = BerryUI Ui.focused
exclusive/top arbitration      = future layer 3
```

硬边界：

```text
BerryUI 不改 wmd
wmd 不知道 component focus
Room 不持树焦点
component focus 不抢窗口键盘授权
```

窗口失焦时，component focus 保持为 app 内记忆状态；窗口重新获得键盘
路由后可继续消费。v1 不发窗口 focus-loss 通知，这是显式非声明。

## §3 focusable 侧表

Node 冻结为 19 字段，不新增 Node 字段。BerryUI 新增第三张侧表：

```rust
FocusEntry {
    node: NodeId,
    visual: Option<NodeId>,
    enabled: bool,
}
```

容量：

```text
FOCUS_MAX = 32
```

满表：

```text
UiError::NodeFull loud
```

生命周期：

```text
remove(subtree) 连带清 focusable
当前焦点在被删子树 → component focus = None
reset/apply_desc 重建 → component focus = 空
```

禁用项可保留在表中但不参与顺序；恢复 enabled 后无需重建表。禁用/摘除
当前焦点会直接清空 component focus，不合成 FocusOut——视觉属主必须在同
一禁用路径还原 ring。

## §4 组件注册

v1 自动注册：

```text
Button
Switch
ListItem
```

锚点均为组件 panel：

```text
a11y anchor = focus anchor = hit container
```

禁用联动：

```text
Button base=Disabled → focusable.enabled=false
Switch disabled     → focusable.enabled=false
```

不可焦：

```text
Scaffold
AppBar
Card
ScrollView 容器
EmptyState
Text
Icon
Divider
```

ScrollView 内容子树正常参序。

## §5 Msg 扩展

新增：

```rust
Msg::FocusIn { node }
Msg::FocusOut { node }
```

不变量：

```text
一次 move 至多 FocusOut + FocusIn
FocusOut 先于 FocusIn
no-op 不产消息
clear 只产 FocusOut
```

激活不新增 Msg：

```text
Enter / Space → app 合成 Msg::Click { focused_node }
```

Button/Switch 复用既有 `on_msg(Click)`，避免第二激活语义。

## §6 顺序：树前序 1D

v1 顺序：

```text
活 focusable 的树前序
```

原因：

```text
add() 尾插兄弟 → 前序 = 作者序/视觉声明序
布局无关 → 首帧 layout 前可用
relayout/viewport 变化不扰动焦点顺序
```

导航：

```text
Next     下一项
Prev     上一项
边界环绕（对齐 Compose 1D Tab）
```

无焦点时：

```text
Next/Prev 都聚焦首项
```

零可焦项或唯一项自指：

```text
不产生 FocusOut/FocusIn 抖动
```

后置：

```text
2D geometric nearest
FocusGroup
FocusRestorer
explicit order key
virtual list anchor
```

## §7 键映射

新增 SDK 纯函数：

```rust
focus_key_intent(code, action, modifiers) -> Option<FocusOp>
```

词表：

```rust
FocusOp::{Next, Prev, Activate}
```

键：

```text
Tab          → Next（只认 DOWN，拒绝 REPEAT 连发）
Shift+Tab    → Prev
Enter        → Activate
Space        → Activate
其它字母     → None
UP           → None
```

app 消费流程：

```text
Room::poll_keys
→ focus_key_intent
→ Next/Prev: ui.focus_move
→ FocusIn/Out: widget on_msg / set_focused
→ Activate: 合成 Msg::Click
→ render + submit
```

## §8 焦点视觉与 a11y

新增主题状态：

```text
InteractionState::Focused
```

Button/Switch/ListItem 消费：

```text
FocusIn  → Focused 色
FocusOut → base/selected 色
```

视觉路径仍是：

```text
set_props → same-rect damage → paint
```

引擎不直绘 focus ring，不新增第二 paint 权威。

本地 a11y 查询：

```rust
Ui::a11y_state(node) -> Option<u8>
```

返回：

```text
VISIBLE
VISIBLE | FOCUSED
```

不改 AST/wire，也不宣称 a11y sidecar 批量传输已落地。

## §9 damage 不变量

焦点变化只 dirty：

```text
旧 focus visual
新 focus visual
```

同一矩形的 FocusIn/FocusOut 会合并为同界 damage。相邻等宽 ring 可按
既有 `Σ×1.25` 规则合并成包络；host 测试用：

```text
Button 路径的 panel/text recolor attempts 全部入账
rect = old bounds ∪ new bounds
```

证明旧/新组件视觉均更新且未夹带父容器；引擎 invalidate 与 widget
set_props 的重复 add 会按既有 damage 合并规则收敛。

## §10 GUI8 真链

normal UITEST 在真实 Keyboard surface 合约下注入：

```text
Tab down/up ×2
Enter down/up ×2
```

期望：

```text
Tab 1: component focus None → Button(index 0)
Tab 2: Button → Switch(index 1)
Enter 1: Switch false → true
Enter 2: Switch true → false
```

markers：

```text
UITESTFOCUSMOVE ok step=1 from=255 to=0
UITESTFOCUSMOVE ok step=2 from=0 to=1
UITESTFOCUSACT ok node=1 toggles=2 states=0,1,0 final=0
UITESTKEY ok code=28 tab=2 downs=2 ups=2
```

键盘路由仍是 wmd 窗口焦点面：

```text
WMKEYROUTE code=15/28 ×8
组件焦点移动不产生 WMFOCUS 表版本变化
```

`255` 是“无初始焦点”的 marker 哨兵，不是 NodeId。

## §11 host 测试

新增：

```text
user/berry-ui/tests/focus.rs
```

覆盖：

```text
树前序 ≠ 登记序
Next/Prev 环绕与冷启动
Widget Disabled 跳过且重启用不复活旧焦点
focus_set no-op 空消息
remove/reset 清焦点与侧表
悬空 NodeId NodeBad
表满 NodeFull loud
Button FocusIn/Out set_props 视觉路径
pointer Click 不移动 component focus
a11y VISIBLE/FOCUSED 位
Tab/Shift+Tab/Enter/Space keymap
UP/REPEAT/字母拒面
Focused 主题色钉板
```

## §12 未完成边界

- 无 2D geometric focus / D-pad。
- 无 FocusGroup、FocusRestorer、explicit tabindex/order key。
- 无 Route/Back 栈；Back 不是 traversal key。
- 无虚拟列表焦点锚。
- 无 focus capture/modal isolation。
- 无跨窗口/global FocusManager。
- 无 a11y 批量 sidecar 输出。
- 无 scroll-into-view；焦点项滚出 Clip port 时 ring 可不可见。
- 文本编辑焦点行为等待 TextField。
- 窗口失焦通知与 component focus dormancy 归后续 wmd/BerrySession 刀。

最终回归已通过：

```text
berry-ui host（focus 9 测）
just test-host
just check
uitest no_std
berry-ui no_std
berry-ui guest-font-vmo no_std
GUI8 self-test
GUI8 normal-boot
GUI8 dual boot + layout self-heal
smoke-image 138/138
cargo deny licenses
docs_guard / Curia strict / writable_globals
```

## §13 预算

当前构建：

```text
总量       = 33,877,368B
单卷 cap   = 33,554,432B
rootfs0    = 33,554,432B
rootfs1    = 322,936B
总量 cap   = 134,217,728B
总量余量   = 100,340,360B
```

UITEST.bap 当前：

```text
76,992B
```

rootfs0 恰在单卷帽；继续加载荷需拆卷/压缩/迁出方案。

## §14 最终镜像

当前候选 SHA-256：

```text
b56dba8dde1f26f810c2c5e015b3d7caed79d43d78e9fb88acf2e3ef10b5b76f
```

复跑镜像 SHA-256 即上值。
