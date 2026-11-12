# BerryUI GUI-T1 TextField Foundation Before IME

> 版本：**1.0**｜2026-11-11
>
> 定位：GUI-T1 现行实施规格。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)，控件规格 = [BerryUIControl-TextField-v1.0.md](BerryUIControl-TextField-v1.0.md)，研究依据 = [../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md](../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md)。
>
> 核心裁决：**先落硬件文本编辑基础，不做软键盘与 IME Host**。焦点在 TextField 时，Enter/Space 先由文本状态机消费；Tab/Shift+Tab 永远交还 C1 导航。

---

## §1 范围

### 做

```text
TextInputState Plain Data
UTF-8 value（≤64B）
codepoint selection/caret
hardware key edit commands
change/selection/submit/reject 布尔事件面
text focus 与 C1 keymap 总路由
TextField 组合控件
selection/caret 视觉与水平 caret-visible 滚动
P0 IME purpose add-only 映射
R2 insets::Ime 数据消费函数
host 测试 + UITEST/GUI8 真键链
```

### 不做

```text
软键盘
IME Host
IME composition 消费
FormMetrics
Route/Back
富文本
clipboard
autofill
TextArea/multiline
点击定位 caret
拖拽选择
选择柄/上下文菜单
caret 闪烁
KeyboardAvoider 容器
```

---

## §2 数据模型

### 状态

```rust
pub struct TextInputState {
    value: TextBuf,
    selection: TextSelection,
}

pub struct TextSelection {
    pub anchor_cp: u16,
    pub caret_cp: u16,
}
```

口径：

```text
value 必须是合法 UTF-8
selection anchor/caret 是 codepoint offset
构造/写入口拒绝非法 UTF-8
caret_cp 与 P0 IME cursor_cp 同口径
值上限 = TEXT_INLINE = 64B = P0 IME_TEXT_MAX
```

### 事件

```rust
pub struct TextInputOutcome {
    pub changed: bool,
    pub selection_changed: bool,
    pub submitted: bool,
    pub rejected: bool,
}
```

映射：

```text
changed           ~ onChange
selection_changed ~ onTextSelectionChange
submitted         ~ onSubmit
rejected          ~ capacity/filter refusal
```

无回调、无 FormMetrics；事件结果由 app 消费。

---

## §3 编辑命令

```rust
pub enum TextInputEdit {
    Insert(char),
    Backspace,
    Delete,
    Left { extend: bool },
    Right { extend: bool },
    Home { extend: bool },
    End { extend: bool },
    SelectAll,
}
```

不变量：

```text
Insert 替换当前 selection
Backspace/Delete 删除 selection 或相邻 codepoint
非 extend 移动折叠 selection
extend 移动保持 anchor
空值/边界操作 total，不 panic
容量拒绝不产生半写
非法 UTF-8不入状态
```

---

## §4 键盘与 C1 优先级

### 硬件 keymap

Linux evdev code：

```text
14  Backspace
15  Tab
28  Enter
30  A
57  Space
102 Home
105 Left
106 Right
107 End
111 Delete
```

规则：

```text
UP 恒 None
Tab 只认 DOWN，且任何修饰形态都交还 C1
Backspace/Delete/arrows/printables 允许 REPEAT
Ctrl/Alt/Super 的非 Tab 组合保留给快捷键，不进文本
Number 档只接受 ASCII digits
```

可打印键 v1 钉板：

```text
A..Z / a..z
0..9 与 !@#$%^&*()
-_=  []{}  ;:'"  `~  \|  ,.<>/?
Space
```

不含 numpad；numpad 归后续硬件键盘矩阵刀。

### 总路由

```rust
pub enum KeyRoute {
    Text(TextInputKeyOp),
    Focus(FocusOp),
    None,
}

pub fn key_route(
    code: u8, action: u8, modifiers: u8,
    kind: TextInputKind, text_focused: bool,
) -> KeyRoute
```

裁决：

```text
text_focused=true:
    Enter -> Submit
    Space -> literal space
    Tab/Shift+Tab -> C1 Next/Prev

text_focused=false:
    保持既有 focus_key_intent 零漂移
    Enter/Space -> Activate
```

结构上保证 TextField 焦点下不会合成 `Msg::Click`，无双激活。

---

## §5 TextField 组合控件

```text
Panel
└── Clip
    ├── Rect(selection)
    ├── Text(value)
    └── Rect(caret)
```

约束：

```text
无新 ViewKind/Props/AST wire
AST kind 值域仍 0..=11
selection/caret 全走既有 set_props/set_offset/damage
TextField 状态不进 AST
focusable/a11y 走既有侧表
```

显示：

```text
Text/Number 原样显示
Password 每个码点显示 *
真实值仍保留在 TextInputState
```

水平滚动：

```text
caret_x < scroll_x -> scroll_x = caret_x
caret_end > scroll_x + port_w -> scroll_x = caret_end - port_w
scroll_x >= 0
```

---

## §6 P0 / R2 映射

### P0 IME purpose

```text
Text     -> IME_PURPOSE_TEXT
Number   -> IME_PURPOSE_NUMBER
Password -> IME_PURPOSE_PASSWORD
Multiline -> 保留值，不作为 TextField 档
```

不新增 wire purpose，不消费 composition frame，不请求 IME Host。

### R2 insets::Ime

```rust
pub fn ime_overlap_px(
    field_bottom: i32,
    viewport_h: i32,
    ime_bottom_inset: i32,
) -> u32
```

数据源：

```text
AppRoom::occlusion().view_insets[3]
```

当前 producer 恒 0，函数恒返 0；未来 IME Host 只替换 inset 生产者，不改 TextField 状态模型。

---

## §7 Host 证据

新增：

```text
user/berry-ui/tests/text_field.rs
```

15 项测试：

```text
空值操作 total
UTF-8 selection/insert 边界
delete backward/forward/capacity loud
Number 过滤与 Password 掩码
key route 文本优先级
P0 purpose 常量对拍
R2 IME overlap
焦点/damage/caret 可见
monospace selection 几何
password mask
水平溢出滚动
AST round-trip 字节相等
C1 离焦不丢文本
零宽先验拒且树零漂
keymap repeat/shortcut 边沿
```

既有 berry-ui host suite 全部保持绿。

---

## §8 UITEST / GUI8 真链

### 页面位置

TextField 追加在 page0 ListDetail 之后：

```text
既有 Button/Switch 两 Tab C1 证明零漂
既有 Button/Scroll/ListDetail 像素锚零漂
TextField 是树前序第 5 个 focusable，index=4
```

### 键序

既有 C1：

```text
Tab×2 -> Button -> Switch
Enter×2 -> Switch 0->1->0
```

GUI-T1：

```text
Tab×3 -> ListItem -> clip item -> TextField
A, Space, A, Backspace -> value "a "
Left, Right -> caret 1 -> 2
Enter -> submit，Switch 不翻转
```

### markers

```text
UITESTTEXTFOCUS ok node=4
UITESTTEXTEDIT ok edits=6 bytes=2 caret=2
UITESTTEXTSELECT ok anchor=2 caret=2
UITESTTEXTSUBMIT ok len=2 toggles=0
UITESTIMEINSET ok bottom=<field_bottom> overlap=0
UITESTKEY2 ok codes=14,15,28,30,57,105,106
```

### 协同漂移

```text
UITESTAST: bytes 779 -> 860, a11y 8 -> 9，×2
UITESTRENDER: ×11 -> ×12（文本编辑后追加真实 render/submit 帧）
WMKEYROUTE: down/up 4/4 -> 14/14
```

layout/hit/ListDetail/clip/surface/原有 focus markers保持既有精确值。

---

## §9 AST / 协议合同

```text
ViewKind/Props/Msg 零新增
Node 19 字段/144B 零漂
encode_desc/apply_desc 字节相等
TextField/focus/selection 不序列化
apply_desc 重建后 focus 与文本状态归 app 重建
P0 IME composition wire 零漂
```

---

## §10 预算

最终构建：

```text
UITEST.bap 89,200B（+8,768B）
rootfs.cpio 33,901,080B
rootfs0     33,554,432B（恰 32MiB 单卷帽）
rootfs1     346,648B
rootfs 总帽余量 100,316,648B
```

其余 GUI app `.bap`：

```text
BROWSER  78,292B
SETTINGS 75,712B
FILES    75,976B
PHOTOS   95,696B
CONTACTS 79,556B
```

rootfs0 仍无余量；后续任何 guest 增长必须继续给拆卷/压缩/迁出方案。

---

## §11 最终镜像

```text
out/berryos.img
SHA-256 = 66aa68fe21e930a338080bd2dc1d71776b27cff2dd5e669b6cd439f934f9bb41
```

镜像由当前树重建，非 L3 旧镜像复用。

---

## §12 回归与验收

必须全绿：

```text
just test-host
just check
berry-ui no_std
uitest no_std
python3 tools/gui8-smoke.py --self-test
just gui8-smoke-normal
just gui8-smoke
just smoke-image
cargo deny licenses
docs_guard
Curia strict
writable_globals
git diff --check
```

最终复跑结果：

```text
just test-host                  PASS
just check                      PASS
berry-ui/uitest no_std           PASS
GUI8 self-test                  PASS
GUI8 normal-boot                PASS
GUI8 dual-boot layout self-heal PASS
smoke-image 138/138             PASS
cargo deny licenses             PASS
docs_guard / Curia strict       PASS
writable_globals                PASS
git diff --check                PASS
```

---

## §13 后置

```text
IME composition commit
软键盘与 IME Host
点击定位 caret
拖拽选择与选择柄
password a11y 安全策略
TextArea/multiline
clipboard/autofill
FormMetrics
KeyboardAvoider 容器
a11y TextEdit action/event（SEM-A1）
```
