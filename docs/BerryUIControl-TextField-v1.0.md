# BerryUI Control Spec: TextField

> 版本：**1.0**｜2026-11-11
>
> 定位：SDK-S0 首个控件规格。实现与真链证据见 [BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md](BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md)。

---

## 1 Background

表单、搜索、设置项与命令行都依赖文本输入。C1 已有组件焦点与键盘路由，但 BerryUI 尚无文本值、selection/caret 或编辑事件。GUI-T1 在 IME Host 之前先落硬件键盘基础。

## 2 Problem / non-goals

问题：

```text
无法输入/编辑单行文本
无 selection/caret 状态
无 text focus 与 C1 激活键的优先级裁决
```

非目标：

```text
软键盘
IME Host
FormMetrics
Route/Back
富文本
clipboard
autofill
TextArea/multiline
拖拽选择与选择柄
caret 闪烁
```

## 3 Is this the right control?

用 TextField 当需要可编辑单行值。只读文本用 `text()`；开关用 `Switch`；按钮触发动作用 `Button`；多行编辑、自动补全与密码管理均不在本控件。

## 4 Geometry / layout contract

组合既有节点，不新增 ViewKind/Props/wire：

```text
Panel(surface_variant, fixed width, fixed height, radius Small)
└── Clip(line height)
    ├── Rect(selection highlight)
    ├── Text(display value)
    └── Rect(caret)
```

几何：

```text
line height = Body font cell_h
field height = line_h + 2 * Spacing::Xs
selection x/w = codepoint range * Body cell_w
caret x = caret_cp * Body cell_w
caret width = max(Stroke::Thin, 1)
```

值溢出时水平滚动，保持 caret 可见；不换行、不垂直滚动。

## 5 State ownership

app 持有：

```rust
TextInputState {
    value: TextBuf,
    selection: TextSelection { anchor_cp, caret_cp },
}
```

TextField 持有 NodeId 句柄与视觉应用状态，不持业务值、不回调、不自动保存。

## 6 Focus contract

```text
panel = C1 focusable/a11y/hit 语义锚
FocusIn -> Focused 视觉 + caret visible
FocusOut -> Normal 视觉 + caret hidden
窗口焦点仍归 gui-wmd
```

TextField 不因获得焦点请求软键盘或 IME Host。

## 7 Keyboard contract

焦点在 TextField：

```text
printable -> replace selection / insert
Backspace -> delete backward
Delete    -> delete forward
Left/Right/Home/End -> caret move；Shift extend selection
Enter     -> submit
Space     -> literal space
Tab/Shift+Tab -> C1 focus navigation
Ctrl/Alt/Super 的非 Tab 组合 -> reserved，不进文本；Tab 组合仍按 C1 既有行为交还导航
```

非文本焦点保持 C1：

```text
Enter/Space -> Activate -> app 合成 Msg::Click
```

总路由单源：`berry_ui::sdk::key_route`。焦点在 TextField 时，Enter/Space 不可能再产生组件 Activate。

## 8 Pointer/touch contract

v1 只保证组件焦点可通过显式 `request_focus` 请求。点击坐标到 caret 的映射、拖拽选择、长按菜单后置。

## 9 Text/IME contract

输入档：

```text
Text     -> P0 IME_PURPOSE_TEXT
Number   -> P0 IME_PURPOSE_NUMBER，仅 ASCII digit
Password -> P0 IME_PURPOSE_PASSWORD，显示每码点一个 *
```

`Multiline` 是 P0 保留 purpose，不是 TextField 构造档。email/phone/URL 只能作为 app 侧过滤层映射到 Text purpose，不得新增 wire 值。

键盘避让数据面：

```text
R2 view_insets[3] = insets::Ime
ime_overlap_px(field_bottom, viewport_h, ime_bottom_inset)
```

当前 IME producer 恒 0，因此 overlap 恒 0；不实现 KeyboardAvoider 容器。

## 10 Accessibility semantics

```text
role = ROLE_TEXT
label = "input"（静态；不把 password 明文放入 a11y 长期树）
state = VISIBLE，持焦时 OR FOCUSED
```

文本变化、selection 变化与 TextEdit action 归 SEM-A1。

## 11 Theming/token contract

```text
panel Normal/Focused = StateColors
selection = Selected.background
value text = text_primary
caret = primary
field height/padding = ThemeMetrics token
```

不新增散装颜色或尺寸字面量。

## 12 API Notes / API Details

```rust
TextFieldSpec { kind: TextInputKind, width: u32 }
TextField::new(ui, parent, ctx, spec)
TextField::request_focus(ui, ctx)
TextField::sync(ui, ctx, state)
TextField::on_msg(ui, ctx, msg)
TextInputState::new(bytes)
TextInputState::apply_edit(edit)
TextInputState::submit()
TextInputState::display_value(kind)
```

事件面：

```rust
TextInputOutcome {
    changed,
    selection_changed,
    submitted,
    rejected,
}
```

## 13 Examples

Host：

```rust
let mut state = TextInputState::new(b"").unwrap();
state.apply_edit(TextInputEdit::Insert('a'));
field.sync(&mut ui, &ctx, &state)?;
```

Guest：UITEST page0 追加 TextField，真实 QMP 键盘完成 focus/edit/select/submit。

## 14 Tests and evidence

- Host：`user/berry-ui/tests/text_field.rs`
- GUI-T1 规格：`docs/BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md`
- AST：组合节点 round-trip 字节相等；无新 kind/props/wire
- GUI8：`UITESTTEXTFOCUS/EDIT/SELECT/SUBMIT + UITESTIMEINSET + UITESTKEY2`
- 预算：UITEST `.bap` 与 rootfs 两帽随 GUI-T1 规格记录

## 15 Compatibility / @since

账本条目：

```text
since = GUI-T1
channel = in-tree
stability = experimental
capability = none
```

## 16 Open Questions

```text
点击定位 caret 的 monospace 坐标映射
selection 绘制的抗锯齿/透明度
a11y text changed 事件粒度
IME composition commit 的直接入口
TextArea 与 multiline newline
```
