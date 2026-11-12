# BerryUI L2 Flex Ratio Allocation

> 版本：**1.0**｜2026-11-11
>
> 定位：GUI-14 L1 后的第一片 flex 基础。只做固定主轴 Row/Col 的剩余
> 空间比例分配；不重写 constraint-aware measure，不改 Node/AST/wire，
> 不做 wrap、shrink、basis、min/max 或嵌套 flex 协商。
> 执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)。

## §1 背景

L1 已有：

```text
根播种 Tight/Loose/Infinite
显式本地 AdaptiveSlot basis
```

但 UITEST 的 list/detail 仍是手写固定宽：

```text
360 + 12 + 684
```

这证明的是局部 basis，不是比例分配。

L2 补上最小差异：

```text
父容器给出固定主轴
子项声明 weight
引擎按 weight 分配剩余主轴空间
```

## §2 分层裁决

v1 measure 仍约束盲：

```text
flex 不改 child.meas_w/h
flex 不改 intrinsic
flex 只在 place_children 消费
```

因此这不是完整 flex 系统，而是：

```text
fixed-main free-space ratio allocation
```

适用前提：

```text
Row/Col 的主轴终值已确定
```

通常是：

```text
Row/Col w/h Some
```

或由父级分配给出确定宽/高。

若主轴未固定，flex-only 双 None 子的内在为 0，可用空间为 0；权重
安全折叠为 0，不 panic。

## §3 侧表

Node 保持冻结：

```text
19 fields / 144B
```

新增运行期侧表：

```rust
FlexEntry {
    node: NodeId,
    weight: u16,
}
```

容量：

```text
FLEX_MAX = 32
```

扩容前必须重算分配乘积界：

```text
flex_space ≤ 2^31
cumulative_weight ≤ FLEX_MAX × 65535
u64 product < 2^52（当前 FLEX_MAX=32）
```

满表：

```text
UiError::NodeFull loud
```

生命周期：

```text
set_flex      登记/覆写
take_flex     摘除
remove subtree 连带清
reset/apply_desc 重建后清空
```

权重域：

```text
1..=u16::MAX
0 → NodeBad
```

重复登记：

```text
同 NodeId 原槽覆写
同值零操作
```

## §4 eligible 面

v1 只接受：

```text
Rect
Circle
```

且：

```text
w/h 双 None
```

父节点必须是：

```text
Row
Col
```

拒绝：

```text
weight 0
Text/Image/Icon
固定尺寸子
父非 Row/Col
悬空 NodeId
```

原因：

```text
Rect/Circle 双 None 的 final_size 恰好消费分配 avail
```

这保证比例槽就是终值，不需要改 `final_size`。

> L3 起eligible 面已扩展到 transposed container pane；本节是 L2 时点的
> 叶子面历史快照。现行矩阵见
> [BerryUI-L3TransposedContainerFlexListDetail-v1.0.md](BerryUI-L3TransposedContainerFlexListDetail-v1.0.md)。

后置：

```text
transposed container flex
same-orientation container flex
Panel/Grid flex
Text/Image/Icon flex
```

## §5 分配算法

对 Row：

```text
W = Row placed width
G = gap × (child_count − 1)
I = Σ 非 flex 子 meas_w
F = flex 可用空间 = max(0, W − G − I)
```

对 Col 镜像使用 height/meas_h。

权重分配采用 cumulative-prefix floor：

```text
target_j = floor(F × cumulative_weight_j / total_weight)
slot_j   = target_j − already_allocated
```

性质：

```text
所有 flex slot 总和恰等于 F
最后一个 flex 子吸收整数余数
```

示例：

```text
F = 100，三子等权
slots = 33 / 33 / 34
```

无 flex 登记时：

```text
Row/Col 原 cursor 语义逐字保持
```

溢出/零空间：

```text
F = 0
flex 子主轴折 0
非 flex 子不 shrink
无 panic
```

## §6 API

新增：

```rust
Ui::set_flex(node, weight) -> Result<(), UiError>
Ui::take_flex(node) -> bool
Ui::flex_iter()
```

`set_flex` / `take_flex` 会：

```text
标脏本节点
标脏祖先链
```

这是必要面：权重变化会改变兄弟槽位，父级重摆才能 force 全部兄弟。

同值覆写：

```text
不 dirty
后续 layout 为 {0,0}
```

## §7 与既有面的交互

### measure cache

不变：

```text
flex 不写 meas
```

### damage

不变：

```text
旧/新绝对界经既有 snapshot/place 入账
clip 真实 viewport
```

### hit

不变：

```text
按新 bounds 命中
零面积 flex 子不可命中
```

### CrossAlign

flex 只管主轴。推荐：

```text
CrossAlign::Stretch
```

若 cross=Start 且子十字轴内在为 0，则会出现 0 宽/高 sliver；这是既有
内在尺寸语义，不是 flex 特例。

### AST/wire

明确非目标：

```text
flex weight 不进 AST/wire
encode_desc 不携带 flex
apply_desc 重建后侧表为空
app 需要重建后重新登记
```

这与 C1 focus 侧表同策略。host 测试钉：

```text
set_flex 前后 encode_desc 字节完全相等
```

因此没有隐形 roundtrip 漂移；非序列化是显式合同。

## §8 host 测试

新增：

```text
user/berry-ui/tests/flex.rs
```

19 项测试覆盖：

```text
Row 1:2 精确分配
gap + 固定子混合
Col 镜像
零空间 / 未固定主轴
登记序无关
权重变更触发兄弟重分配与 damage
eligible/parent/weight 拒面
容量 NodeFull
remove 生命周期
AST 前后零漂移
同值 zero-op
固定子溢出
hit 边界
三等分余数归末子
apply_desc 重建丢权重、重登记后恢复 200/400 host 样
remove 槽复用无 stale weight
CrossStart 主轴分配/十字 0 sliver
Pad/Grid 等非 Row/Col 父拒面
Circle eligible 正例
take_flex 后兄弟重排
u16::MAX 权重域
flex 不写 measured extent
```

## §9 UITEST 真链

L1 adaptive Row 保持：

```text
局部 basis = 1056
gap = 12
高 = 120
threshold = 600 → Expanded
```

两个 Pane 改为：

```text
Rect w/h None
flex weight = 1 / 2
```

分配：

```text
1056 − 12 = 1044
1044 / 3 = 348
```

终值：

```text
list   = 12,623,348,120
detail = 372,623,696,120
```

关系：

```text
detail.x = list.x + list.w + gap
detail.w = list.w × 2
```

marker：

```text
UITESTADAPT ok v=2 basis=1056
            list=12,623,348,120
            detail=372,623,696,120
```

GUI8 normal：

```text
UITESTADAPT ×2
UITESTADAPT2 ×1
UITESTAST bytes=755 a11y=8 ×2
UITESTRENDER ×11
```

像素锚：

```text
list 中心   = surface_variant
detail 中心 = surface
list 末列   = surface_variant
gap 首列   = surface
```

后两个 seam 像素把 348/696 从 marker 钉到真实屏幕。

## §10 非目标

后置：

```text
flex basis
grow/shrink 分离
min/max slot
wrap
intrinsic flex
nested flex negotiation
transposed container flex
same-orientation container flex
Grid fr
Panel flex
MainAlign/SpaceBetween
alignSelf
Spacer
ListDetail 容器
flex wire/AST 序列化
```

完整 constraint-aware measure 仍是 layout v2 独立刀。

最终回归已通过：

```text
berry-ui host（flex 19 测）
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

当前候选构建：

```text
总量       = 33,890,528B
rootfs0    = 33,554,432B
rootfs1    = 336,096B
总量 cap   = 134,217,728B
总量余量   = 100,327,200B
```

GUI BerryUI app 当前尺寸：

```text
BROWSER.bap  = 78,060B
SETTINGS.bap = 75,496B
FILES.bap    = 75,752B
PHOTOS.bap   = 95,456B
CONTACTS.bap = 79,348B
UITEST.bap   = 80,040B
```

rootfs0 仍恰在单卷帽；继续加载荷必须走拆卷/压缩/迁出。

## §12 最终镜像

最终 SHA-256：

```text
fbf754c9a30c4c733539616cfcb8c4f9637e5dd2b2b13180d1275e00d5dec213
```

全量复跑后以最终镜像为准。
