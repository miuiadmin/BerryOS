# BerryUI L3 Transposed Container Flex & ListDetail

> 版本：**1.0**｜2026-11-11
>
> 定位：在 L2 runtime-only flex ratio 上开放最小 transposed container
> pane 面，并提供无状态 ListDetail 布局原语。不引入 Route/Back/选择
> 状态栈/FormMetrics，不做完整 constraint-aware measure。
> 执行序权威 = [GUI框架地基路线-v1.18.md](GUI框架地基路线-v1.18.md)。

## §1 背景

L2 只允许：

```text
Rect/Circle 且 w/h 双 None
```

这能证明比例分配，但 Rect/Circle 是叶子，不能承载列表与详情内容。
真正 ListDetail 需要：

```text
pane 本身吃掉 flex 主轴槽
pane 内部继续挂内容子树
```

L3 只扩大 eligible 面，不改：

```text
allocation 数学
measure
final_size
damage
hit
Node
AST/wire
```

## §2 eligible 矩阵

判定依据是现有 `final_size` 是否消费父主轴 avail。

### 父 Row，主轴 = 宽

允许：

```text
Rect/Circle：w/h 双 None（L2 原面）
Col：p.w None
Grid：p.w None
Panel：p.w None
```

拒绝：

```text
Row（同向，宽=内在）
Clip/Pad（本刀后置）
Text/Image/Icon（内在叶）
```

### 父 Col，主轴 = 高

允许：

```text
Rect/Circle：w/h 双 None（L2 原面）
Row：p.h None
```

拒绝：

```text
Col/Grid/Panel（同向/高度内在）
Clip/Pad
Text/Image/Icon
```

同向容器拒绝的原因：

```text
slot 会被 final_size 的内在主轴覆盖，形成静默死权重
```

## §3 单源 predicate

新增：

```rust
layout::flex_eligible_props(nodes, idx, props)
```

三个消费点同源：

```text
Ui::set_flex 写侧 loud 拒面
Ui::set_props props 漂移摘除
layout::flex_weight 读侧残项过滤
```

父 kind 与槽位关系不可变：

```text
set_props 拒 kind 漂移
无 reparent API
```

因此 orientation 判定不会在运行中漂移。

## §4 pane 行为

flex 只影响父主轴：

```text
Row pane 宽 = slot
Col pane 高 = slot
```

非 share 轴仍走既有规则：

```text
HAS_SIZE 覆盖 avail
None 则取内在或父分配
```

例如 Row 中的 Col pane：

```text
宽吃 flex slot
高由 p.h 或内容内在决定
```

ListDetail 的 Panel pane 用：

```text
p.w = None
p.h = Some(height)
```

因此主轴比例分配，十字轴稳定。

### shrink/overflow

slot 小于 pane 内在需求时不 shrink：

```text
pane 保持内在子树布局
可能溢出 pane slot
不自动 Clip
```

需要裁剪时仍显式使用 Clip/ScrollView。

### collapsed pane hit 语义

普通父节点不是 Clip。pane 主轴为 0 时：

```text
pane 自身不可命中
非零子树仍按既有 hit_walk 可命中
```

这是既有容器语义，不是 L3 新裁剪规则。

### nested flex

外层 slot 给 pane 提供确定主轴后，pane 内部可继续使用固定主轴
Row/Col flex：

```text
outer Row flex -> Col pane 宽确定
inner Col flex -> 内层 Row 高按权重分配
```

这是语法组合，不是 measure-phase nested negotiation。

## §5 ListDetail 原语

新增 Plain Data 规格：

```rust
ListDetailSpec {
    width: u32,
    height: u32,
    gap: u32,
    list_weight: u16,
    detail_weight: u16,
}
```

新增无状态句柄：

```rust
ListDetail {
    row: NodeId,
    list: NodeId,
    detail: NodeId,
}
```

构造：

```rust
ListDetail::new(ui, parent, ctx, spec)
```

生成：

```text
Row(w=width,h=height,gap,Stretch)
list   Panel(bg=surface_variant,w=None,h=height,
            padding=Spacing::S,gap=Spacing::S,radius=0)
detail Panel(bg=surface,w=None,h=height,
             padding=Spacing::S,gap=Spacing::S,radius=0)
set_flex(list,list_weight)
set_flex(detail,detail_weight)
```

weight 0 在建树前拒绝：

```text
NodeBad
```

业务责任仍归 app：

```text
选择状态
内容节点
Compact/Expanded 分支
返回/路由
```

BerryUI 不新增：

```text
on_msg
回调
Route/Back 栈
selection controller
FormMetrics
```

## §6 focus / a11y

ListDetail pane 本身不可焦、不登记 a11y。

内容子树按 C1 树前序：

```text
list subtree -> detail subtree
```

权重变化：

```text
不产生 FocusIn/FocusOut
focus node 保持
```

移除 ListDetail 子树：

```text
flex side table 连带清
focus 若在子树内 -> None
```

## §7 AST / runtime-only

L3 不改变 L2 runtime-only 合同：

```text
flex weight 不进 AST/wire
encode_desc 不携带权重
apply_desc 重建后侧表为空
重建后必须重新 ListDetail::new 或 set_flex
```

host 钉：

```text
AST roundtrip 字节相等
重建后 pane 回内在/塌缩形态
重登记后恢复 196/392 或 348/696
```

## §8 host 测试

新增：

```text
user/berry-ui/tests/list_detail.rs
```

16 项测试覆盖：

```text
Row 中 Col pane 1:2
Row 中 Panel/Grid pane
Col 中 Row pane 镜像
同向/固定主轴拒面
props 离开 eligible 摘除
ListDetail 几何/damage/hit
权重重分配
AST runtime-only
apply_desc 重建重登记
C1 focus 顺序与 remove 生命周期
collapsed pane 子树 hit 语义
nested transposed flex
CrossStart 非零内容界
pane measured intrinsic 与 slot 分离
权重变化零 focus 消息
pane 注册前后 AST 零漂移
ListDetail weight 0 建树前拒绝
Clip/Pad/Image/Icon 与 kind/props 误配拒面
无 flex 重建的内在 8/8 几何
```

既有 L2 19 测不改，全部保持通过。

## §9 UITEST 真链

page0 的 adaptive 样例改为：

```rust
ListDetailSpec {
    width: 1056,
    height: 120,
    gap: 12,
    list_weight: 1,
    detail_weight: 2,
}
```

分配不变：

```text
1056 − 12 = 1044
1:2 -> 348 / 696
```

markers：

```text
UITESTADAPT ok v=2 basis=1056
            list=12,623,348,120
            detail=372,623,696,120

UITESTLISTDETAIL ok weights=1,2
                  row=12,623,1056,120
                  list=12,623,348,120
                  detail=372,623,696,120
```

AST：

```text
bytes 755 -> 779
a11y 仍 8
```

原因：

```text
两个 Rect pane 变为两个可承载内容的 Panel pane
flex weight 仍 runtime-only
```

GUI8 normal：

```text
UITESTADAPT ×2
UITESTLISTDETAIL ×2
UITESTADAPT2 ×1
UITESTAST bytes=779 a11y=8 ×2
UITESTRENDER ×11
```

像素锚不变：

```text
list/detail 中心
348/696 seam
```

反 mock 新增：

```text
ListDetail marker 缺失
ListDetail 几何回旧固定 360/684
weights 1,2 -> 1,3
marker 重复
AST 779 -> 712
```

## §10 非目标

后置：

```text
same-orientation container flex
Clip/Pad flex
Grid fr
flex basis
grow/shrink
min/max slot
wrap
intrinsic flex
measure-phase nested negotiation
MainAlign/SpaceBetween
alignSelf
Spacer
状态化 ListDetail controller
Route/Back/selection stack
flex wire 序列化
scroll-into-view
```

最终回归已通过：

```text
berry-ui host（ListDetail 16 测）
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

## §11 预算

最终构建：

```text
总量       = 33,892,312B
rootfs0    = 33,554,432B
rootfs1    = 337,880B
总量 cap   = 134,217,728B
总量余量   = 100,325,416B
```

GUI BerryUI app：

```text
BROWSER.bap  = 78,292B
SETTINGS.bap = 75,712B
FILES.bap    = 75,976B
PHOTOS.bap   = 95,696B
CONTACTS.bap = 79,556B
UITEST.bap   = 80,432B
```

rootfs0 恰在单卷帽；后续增长继续走拆卷/压缩/迁出。

## §12 最终镜像

最终 SHA-256：

```text
7850a3d7629dcf0db90bc30cd8b83af03fd7b5de5337f1316a42910fa65b7aae
```

复跑镜像 SHA-256 即上值。
