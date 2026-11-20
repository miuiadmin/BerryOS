# BerryUI SEM-A1 Semantic Tree Sidecar

> 版本：**1.0**｜2026-11-12｜状态：**superseded**｜取代：[BerryUI-SEMA1SemanticTreeSidecar-v1.1.md](BerryUI-SEMA1SemanticTreeSidecar-v1.1.md)
>
> 定位：SEM-A1 现行实施规格。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)；上游词表 = [BerrySession-P0SurfaceMode协议-v1.0.md](BerrySession-P0SurfaceMode协议-v1.0.md) §6；焦点权威 = [BerryUI-C1ComponentFocusNavigation-v1.0.md](BerryUI-C1ComponentFocusNavigation-v1.0.md)；文本来源 = [BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md](BerryUI-GUIT1TextFieldFoundationBeforeIME-v1.0.md)；研究依据 = [../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md](../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md)。
>
> 核心裁决：**语义树是保留树的导出旁车，不是第二真相源**。它不造第二焦点、第二命中或第二几何权威；不进 AST/wire；不启动读屏服务。

---

## §1 范围与非目标

### 做

```text
runtime-only SemEntry overlay
legacy a11y role/label + value/state/actions/text range
P0 A11yRegion 52B 投影
树前序快照
最近已发射语义祖先 parent
bounds ∩ Clip 链可见性过滤
merge-descendants
C1 focusable/focused 派生
TextField value/selection 同步
Button Disabled
Switch Checked
ListItem Selected
semantic hit 的最近语义锚解析
```

### 不做

```text
screen reader 服务
TTS / 读屏导航
批量语义传输
订阅 / push channel
跨进程 a11y 服务
语义事件外发
action dispatch
P0 wire 变更
value/text range wire 序列化
语义命中成为输入路由权威
密码明文长期语义
SEM-A2 记录扩展
```

---

## §2 分层裁决

三层权威不变：

```text
wmd        window focus / seat / keyboard routing
BerryUI C1 component focus / tree navigation
hit/wmd    visual input routing
```

SEM-A1 只消费这些权威：

```text
focus table -> FOCUS action + FOCUSED state
focus_node  -> FOCUSED state
bounds/clip -> visible bounds
a11y table  -> legacy thin face
text state  -> value / selection
```

`sem_at(x,y)` 只是“视觉命中后向上找最近语义锚”的辅助查询；不参与输入路由，不产生第二 hit test。

---

## §3 数据模型

新增小容量 runtime overlay，必须挂在既有 a11y 薄条目上：

```rust
SemEntry {
    node: NodeId,
    actions: u8,
    state: u8,
    value: TextBuf,
    text_range: TextSelection,
    merge_descendants: bool,
}
```

容量与预算：

```text
SEM_MAX = 8
SEM_LABEL_MAX = P0 A11Y_LABEL_MAX = 32
value ≤ TEXT_INLINE = 64B
```

`SEM_MAX=8` 是客机栈预算裁决：语义 overlay 不复制 role/label/几何，也不为全部 a11y 文本节点常驻 value 缓冲；普通 BerryUI 页面交互控件超过 8 时 NodeFull loud，要求页面拆分或后续容量刀显式重裁。

存储 / 派生分界：

```text
存储：声明 actions、静态 state、value、range、merge
单源：role/label = legacy a11y；bounds = layout；focus = C1
派生：bounds、visible、focused、focusable、parent
```

FOCUS action 位禁止存储，只能由 C1 focus 表派生；`VISIBLE/FOCUSED` state 位禁止存储，只能由快照派生。

---

## §4 生命周期

```text
set_sem(entry)       先有 a11y + 悬空/词表/文本/range 先验拒；同节点覆写；满 8 NodeFull
take_sem(node)       幂等摘除
sem_iter()           活性过滤
sem_of(node)         runtime value/range 读面
semantic_count()     活语义数
remove(subtree)      连带清
reset/apply_desc     整清
encode_desc          永不序列化
```

apply_desc 重建后，widget/app 必须重新同步语义；与 C1 focus、GUI-T1 文本状态同一 runtime-only 合同。

---

## §5 快照与 merge

`Ui::sem_snapshot_into(&self, out)`：

```text
输入：legacy a11y 薄表 + 同节点 SEM overlay
顺序：视觉树前序
合并：role/label 取 a11y；actions/state/value/range 取 SEM overlay
bounds：绝对 bounds ∩ Clip 链
可见：空界直接跳过
node_id：slot + 1
parent_id：最近已发射语义祖先 slot + 1；根语义为 0
actions：SEM actions | C1 enabled FOCUS
state：静态 state | VISIBLE | C1 focused FOCUSED
输出：P0 A11yRegion 52B 记录
```

`slot + 1` 的原因：

```text
raw NodeId 可能等于 0
P0 node_id=0 非法
parent_id=0 是 root sentinel
```

快照 id 不承诺跨帧稳定；增量 diff / 稳定 id 归 SEM-A2。

### merge-descendants

```text
entry.merge_descendants = true
```

效果：

```text
该节点自身保留
后代语义节点全部跳过
视觉树 / hit / focus / damage 不变
```

这是官方 accessibilityGroup 的最小落面；不实现 Compose `clearAndSetSemantics` 的完整替换语义。

---

## §6 Widget 同步

```text
Button      role=BUTTON, action=ACTIVATE, state=DISABLED
Switch      role=TOGGLE, action=ACTIVATE, state=CHECKED/DISABLED, value=on/off
ListItem    role=LIST_ITEM, action=ACTIVATE, state=CHECKED
TextField   role=TEXT, action=TEXT_EDIT, value, ordered text_range
```

TextField Password：

```text
value 恒空
text_range 恒 0,0
真实明文只留在 app 的 TextInputState
```

---

## §7 P0 导出边界

P0 记录：

```text
A11Y_REGION_LEN = 52B
role 0..=7
actions Focus/Activate/Adjust/TextEdit
state Visible/Focused/Disabled/Checked
label ≤32B UTF-8
x/y/w/h u16，w/h != 0
```

BerryUI 普通保留树只产生 role `0..=5`：

```text
NONE / BUTTON / TEXT / IMAGE / TOGGLE / LIST_ITEM
```

`DOCUMENT=6`、`GAME_REGION=7` 留给专业 canvas / immersive 声明，不由普通 BerryUI widget 自动生成。

BerryUI legacy a11y label 上限是 64B；投影到 P0 时超过 32B loud 拒，不截断、不静默丢。

---

## §8 AST / wire 合同

```text
ViewKind 零新增
Props 零新增
Msg 零新增
Node 19 字段 / 144B 零漂
AST a11y 块仍只含 role/label
encode_desc 前后字节相等
apply_desc 重建清 SEM
P0 52B codec 只消费不修改
```

---

## §9 Host 证据

新增：

```text
user/berry-ui/tests/sem.rs
```

8 组测试（含全部快照记录 P0 round-trip与反向 selection 排序）：

```text
set/take/lifecycle/capacity loud
先有 a11y / 词表 / UTF-8 / value / range 拒面
remove 子树连带清
apply_desc 重建清且 AST 不序列化
树前序与 parent id
P0 codec round-trip
SEM wins legacy
focus/state/action 派生
clip 过滤
merge-descendants
out too small loud
Button/ListItem/Switch/TextField 同步
Password 语义 value 恒空
sem_at 最近语义祖先
AST 零漂
Disabled 语义状态
```

---

## §10 UITEST / GUI8 真链

UITEST 增加 semantic dump：

```text
UITESTSEM
UITESTSEMSTATE
UITESTSEMPAGE
UITESTSEMTEXT
```

覆盖：

```text
page0 快照计数 / 树前序 / P0 codec
Switch CHECKED 变化
page swap 后旧语义不残留
TextField value/range/focused 派生
clip 过滤几何
AST 仍 bytes=860 / a11y=9
```

既有 GUI-T1 marker 与计数全部保持不变。

---

## §11 预算

基线：

```text
UITEST.bap 98,328B（+9,128B）
rootfs.cpio 33,918,464B
rootfs0     33,554,432B（恰 32MiB 单卷帽）
rootfs1     364,032B
总量余量   100,299,264B
```

SEM overlay 为 runtime-only，无资产、无服务、无新 channel；增长来自 BerryUI 代码链接与 UITEST dump。rootfs0 仍无单卷余量，后续 guest 增长继续走拆卷/压缩/迁出前置裁决。

---

## §12 最终镜像

```text
SHA-256 = 27693390dc49e49ecc66709be9ea099e9e257bcc3490a8a879f354b5b7022135
```

---

## §13 回归与验收

必须全绿：

```text
berry-ui host suite
just test-host
just check
berry-ui no_std
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
berry-ui host suite               PASS
just test-host                    PASS
just check                        PASS
berry-ui no_std / guest-font-vmo  PASS
uitest no_std                     PASS
GUI8 self-test                    PASS
GUI8 normal-boot                  PASS
GUI8 dual-boot canonical persistence PASS
smoke-image 138/138               PASS
cargo deny licenses               PASS
docs_guard / Curia strict         PASS
writable_globals                  PASS
```

---

## §14 SEM-A2 边界

```text
语义批量帧 / 订阅 / tree update policy
稳定 id 与增量 diff
语义 action dispatch
value/text range wire v2
heading / paneTitle / hidden / accessibilityLevel
minimum touch target
screen reader 服务与跨进程边界
password a11y 深化
scroll-into-view
```
