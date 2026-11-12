# BerryUI LAYOUT-L5 Per-node Box & Text Wrap

> 版本：**1.0**｜2026-11-12
>
> 定位：LAYOUT-L5 现行实施规格。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)；前置 = [BerryUI-LAYOUTL4BoxConstraintsIntrinsicBoundary-v1.0.md](BerryUI-LAYOUTL4BoxConstraintsIntrinsicBoundary-v1.0.md)；RUN-K1 生命周期地基 = [BerryUI-RUNK1KeyedReApplyLifecycle-v1.0.md](BerryUI-RUNK1KeyedReApplyLifecycle-v1.0.md)。
>
> 核心裁决：**incoming BoxConstraints 在一次后序 measure walk 中按父容器结构向下投影**；本刀只有显式登记 wrap 的 Text 消费 `max_w`。wrap 是 runtime-only 状态，不进 Node/Props/AST/wire；wrap 未登记的树必须与 L4 逐字节等价。

---

## §1 问题

L4 的四元组只作用于 Root intrinsic boundary。子树仍是约束盲测量：

```text
Text intrinsic = tm.measure(text)
长文本在窄容器中仍按单行宽返回
TextField 只能声明固定 width，无法向父级可用宽收敛
theme 与 launcher 各持一份视口密度阈值
```

L5 不重写布局内核、不引入第二遍 measure、不把 place 阶段结果回灌 measure。

---

## §2 constraints down

新增内部单源：

```rust
child_measure_box(parent: &Node, incoming: BoxConstraints)
    -> BoxConstraints
```

投影规则：

```text
Root:
    loose(incoming.max_w, incoming.max_h)

Pad:
    incoming.deflate(pad×2, pad×2)——与 place 面左右两侧 port 同口径

Panel:
    若 w/h Some：定尺寸覆盖一切，再扣 padding
    否则 incoming 对应轴扣 padding

Col:
    cross width = p.w 或 incoming.max_w
    main height = unbounded

Row:
    main width = unbounded
    cross height = p.h 或 incoming.max_h

Grid:
    子盒 = 单 track，而非整格容器宽
    track = (fixed/incoming 尺寸 - gap×(轨数-1)) / 轨数
    行数按子实占 ceil(k/cols)，与 place 面同源

Clip:
    fixed w/h 优先；否则 incoming
```

flex slot 仍是 place 阶段分配，不伪装成 measure box；嵌套 flex 协商归 LAYOUT-L6b。

Row 的主轴宽恒 unbounded，直接放在 Row 下的 wrap Text 不会主动断行；flex slot / 嵌套协商中的 wrap 边界归 LAYOUT-L6b。

---

## §3 Text wrap 状态模型

新增 runtime-only 侧表：

```rust
pub const TEXT_WRAP_MAX: usize = 32;

pub struct TextWrapEntry {
    pub node: NodeId,
    pub enabled: bool,
    pub last_max_w: u32,
}
```

API：

```rust
Ui::set_text_wrap(node: NodeId, enabled: bool)
Ui::take_text_wrap(node: NodeId)
Ui::text_wrap_iter()
```

语义：

```text
只允许活 Text 节点
悬空/非 Text → NodeBad
表满 → NodeFull
同值覆写 no-op
变更 → 节点 + 祖先 dirty
remove 子树 → 连带清理
reset/apply full rebuild → 整清
```

调用方只声明 `enabled`；`last_max_w` 由 measure walk 写入，paint 复用同一断行计划。侧表不序列化。

---

## §4 box-sensitive TextMetrics

`TextMetrics` 冻结签名保持，新增默认方法：

```rust
fn measure_with_box(
    &self,
    text: &[u8],
    scale: u8,
    max_w: u32,
) -> (u32, u32) {
    self.measure(text, scale)
}
```

Monospace 供件覆写为 wrap-aware；旧供件零级联。 `set_text_metrics` 换件会全树 layout/paint dirty 并记 FULL damage，避免旧 intrinsic 与新断行计划分叉。测量调用合同：

```text
每个显式 dirty layout pass
每个 wrapped Text 恰调用一次 measure_with_box
```

内部断行可多次调用底层 `measure`；供件若要计数，应覆写 `measure_with_box` 作为一次逻辑测量。

---

## §5 贪心断行

新增 `text_wrap::plan`：

```text
U+0020 后可断
CJK 区间码点后可断
无软断点且下一码点溢出时，断在前一个可容纳码点
首码点自身超宽时独立成行，不丢字
断点后的 ASCII 空格不进入下一行可视宽
空文本零行
```

固定容量：

```text
WrapPlan starts/ends = [u8; 65]
不复制文本
零 heap
```

测量：

```text
width  = max(line width)
height = lines × line height
```

绘制仍用一个 GlyphRun；每个 `GlyphPos` 自带 x/y，多行只是 y 递增、x 归零。测量与绘制共用同一个 `WrapPlan`。

---

## §6 TextField shrink-to-width

`TextField` 新增：

```rust
shrink_to_width(ui, ctx, avail_w) -> Result<u32, UiError>
```

下限：

```text
body cell width
+ 左右 Xs padding
+ caret Thin 宽
```

上限：

```text
构造时的 spec.width
```

实现走既有：

```text
PanelProps.w + Ui::set_props + geometry damage + relayout
```

选择/光标滚动仍在单行 `Clip` 内，语义不变。多行 TextArea、光标跨行映射、IME 换行行为全部后置。

---

## §7 密度阈值单源

唯一权威：

```rust
Density::for_viewport(w, h)
```

旧 launcher 私有：

```text
w >= 600 && h >= 1800
```

删除。600..1000 宽、高 ≥1800 的旧分叉区统一 fail-closed 为 Compact。

`tools/gui_guard.py` 新增静态门禁：

```text
600/1000 宽阈值 + 1800 高阈值
不得在 theme.rs/tests 之外共现
```

负样本进 `--self-test`。

---

## §8 零漂合同

```text
wrap 未登记：
    Text 仍走 measure() 单行语义
    L1/L2/L3/L4 全部既有测试不改
    LayoutStat measured/placed 语义不变
    damage 仍裁真实 viewport
    hit 仍按 Root 摆放界

AST/wire：
    ViewKind 零新增
    Props 零新增
    Msg 零新增
    Node 19 字段 / 144B 零漂
    encode_desc / apply_desc 零漂
    GUI8 AST bytes=860 / a11y=9 零漂
```

---

## §9 SDK 面

新增 facade：

```text
TextWrapEntry
TEXT_WRAP_MAX
```

`Ui::set_text_wrap` / `take_text_wrap` / `text_wrap_iter` 是既有 `Ui` 方法；`measure_with_box` 是既有 `TextMetrics` trait 扩展。二者不另立顶层 facade 名。

SDK ledger：

```text
since     = LAYOUT-L5
stability = experimental
spec      = 本文
```

---

## §10 Host 证据

新增：

```text
user/berry-ui/tests/layout_l5.rs
```

10 组测试：

```text
贪心断行：space / CJK / emergency / zero-width / 行尾空清理
incoming parent box：Col 宽变化驱动 wrap 重测
paint 与 measure 共用同一 WrapPlan
wrap 登记 reject / 幂等 / remove 生命周期
TextField shrink floor / 上限 / damage 路径
wrap-off 单行零漂
wrapped Text 每显式布局 pass 恰一次 measure_with_box
Pad 双侧投影与 place port 对拍
Grid 单 track 投影与 place 行主序轨道对拍
父容器几何变化不全树 dirty 时 wrap 叶子重测
```

---

## §11 UITEST / GUI8 证据

UITEST 在既有 Button label 上执行 runtime-only 探针。label 先临时改为长样本：

```text
FRAME LAYOUT HIT STATE
```

再执行：

```text
set_text_wrap(label, true)
dirty
layout_box(tight 300×1200)
读取 label bounds / incoming max / LayoutStat
take_text_wrap
set_text(label, "Details")
dirty + layout() 恢复 1080×1920
```

真实标记：

```text
UITESTL5 ok bounds=18,132,192,192 max=264 measured=41,41
```

`bounds=192×192` 是 wrapped label 的真实 4 行界（行尾断行空格不计宽），证明 guest 走过 greedy break 与多行 GlyphPos；`max=264` 是 per-node incoming 宽上界；`measured=41,41` 表示该次布局仍只重测 41 个节点且全部完成摆放。

normal boot 恰两次；不新增节点，不改变 AST，不额外提交 frame。GUI8 对 marker 做 exact count 与 payload 反 mock 断言。

---

## §12 预算

RUN-K1 基线：

```text
UITEST.bap 101,648B
rootfs.cpio 33,923,776B
rootfs0 33,554,432B
rootfs1 369,344B
总量余量 100,293,952B
```

LAYOUT-L5 最终：

```text
BROWSER.bap  = 82,756B   (+2,936B)
SETTINGS.bap = 81,120B   (+2,752B)
FILES.bap    = 80,696B   (+2,768B)
PHOTOS.bap   = 100,240B  (+2,864B)
CONTACTS.bap = 84,196B   (+2,864B)
UITEST.bap   = 105,776B  (+4,128B)

rootfs.cpio 33,945,368B  (+21,592B)
rootfs0     33,554,432B  (0B，仍恰 32MiB 单卷帽)
rootfs1       390,936B  (+21,592B)
总量余量   100,272,360B
```

全部增长进入 rootfs1；rootfs0 零增长。

最终镜像：

```text
SHA-256 = 2c5efced5b102d802153e5387556b562f83105f58d5a2df1d45084868f518fa2
```

---

## §13 验收

```text
cargo test -p berry-ui
just test-host
just check
berry-ui no_std
berry-ui guest-font-vmo no_std
uitest no_std
gui-wmd host/musl 既有面
GUI8 self-test
GUI8 normal-boot
GUI8 dual-boot canonical persistence
smoke-image 138/138
cargo deny licenses
docs_guard
Curia strict
writable_globals
git diff --check
```

最终复跑结果：

```text
cargo test -p berry-ui                  PASS
just test-host                          PASS
just check                              PASS
berry-ui no_std                         PASS
berry-ui guest-font-vmo no_std          PASS
uitest no_std                           PASS
gui-wmd host/musl                       PASS
GUI8 self-test                         PASS
GUI8 normal-boot                        PASS
GUI8 dual-boot canonical persistence    PASS
smoke-image 138/138                     PASS
cargo deny licenses                     PASS
docs_guard / Curia strict              PASS
writable_globals                        PASS
git diff --check                        PASS
```

smoke-image 字节对拍：

```text
kfile = 4,772,040B
init  = 33,945,368B
与宿主精确同数
```

---

## §14 后置

```text
flex basis/grow/shrink
嵌套 flex measure negotiation
Grid fr/auto track
MainAlign / alignSelf / SpaceBetween
wrap-to-flex-slot
TextArea 多行光标
比例字体高级断行 / kinsoku / hyphenation
continuous text scale
per-node incoming box for非 Text kind
```
