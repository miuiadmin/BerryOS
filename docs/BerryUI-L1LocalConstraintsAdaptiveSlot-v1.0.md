# BerryUI L1 Local Constraints & Adaptive Slot

> 版本：**1.0**｜2026-11-11
>
> 定位：GUI-14 L1 第一刀。落地根播种 Tight/Loose/Infinite 约束与显式
> 本地 AdaptiveSlot 判定。只解决“嵌套组件不读全局屏幕尺寸”的最小面；
> 不做 FormMetrics、SafeArea、完整 flex/wrap、ListDetail 或 size-class
> 阶梯。执行序权威 = [GUI框架地基路线-v1.16.md](GUI框架地基路线-v1.16.md)。

## §1 背景

C1 前，`Constraints` 只有词表：

```text
Viewport
Tight { w, h }
Loose { w, h }
Infinite
```

实际执行路径：

```text
非 Viewport → debug_assert panic
release/guest → 静默按 Viewport
```

这使 GUI-14 B10 的 local adaptive pane、ListDetail、wrap/flex 都没有
真实约束基础。

## §2 分层裁决

v1 布局仍是单 intrinsic、约束盲 measure：

```text
每个节点先算一个内在尺寸
place 阶段按父分配终值
```

因此 L1 最小诚实实现是：

```text
Constraints 只播种本次 relayout 的 Root
```

不影响：

```text
Node 字段
AST/wire
measure 缓存
paint
hit
damage 合并规则
wmd/Room
```

## §3 四档语义

### Viewport

```text
Root intrinsic = viewport
Root place avail = viewport
Root final = viewport
```

这是既有语义，必须 byte-for-byte 保持。

### Tight

```text
Root intrinsic = (w, h)
Root place avail = (w, h)
Root final = (w, h)
```

等价于把本次布局的 viewport 换成精确盒，但 damage clip 仍用真实
viewport。

### Loose

Root 先取子的 stack 包络：

```text
extent_w = max(child.meas_w)
extent_h = max(child.meas_h)
```

再 clamp：

```text
Root = min(payload.w, extent_w) × min(payload.h, extent_h)
```

空 Root：

```text
(0, 0)
```

已知 wart：

```text
Stretch 型子（Rect None/None）内在为 0
```

因为 v1 measure 约束盲。shrink-wrap 目标需要 HAS_SIZE 或内在叶；
host 测试钉板该行为。

### Infinite

```text
Root = child extent
不设 payload 上限
```

可大于 viewport；越界部分只按真实 viewport 裁 damage/paint。

## §4 API

新增门面：

```rust
Ui::layout_with(Constraints) -> LayoutStat
```

语义：

```text
pull-based 一次性布局
不保存上次 Constraints
净树调用 = documented no-op { measured: 0, placed: 0 }
约束切换需调用方先制造几何 dirty
render_frame 发起的 relayout 请求仍为 Viewport；
净树不会自动恢复上次 layout_with 的约束，调用方需显式制造 dirty 并回 Viewport
```

既有：

```rust
Ui::layout()
```

等价：

```rust
Ui::layout_with(Constraints::Viewport)
```

SDK 导出：

```rust
Constraints
```

## §5 damage / clipping

不变量：

```text
damage clip 永远 = 真实 viewport
不用 Tight/Loose/Infinite 盒裁剪
```

原因：

```text
paint 的既有裁剪域是真实 viewport
hit 的根域是 Root 摆放界；Viewport 约束下二者相等
内容合法溢出约束盒
局部裁剪归 Clip 节点
ScrollView 已用 Clip 承担该职责
```

溢出、零尺寸、u32::MAX 均走既有饱和路径，零 panic。

## §6 AdaptiveSlot

新增纯函数：

```rust
slot_variant(basis_w: u32, threshold: u32) -> SlotVariant
```

词表：

```rust
SlotVariant::Compact
SlotVariant::Expanded
```

判定：

```text
basis_w >= threshold → Expanded
否则 Compact
```

纪律：

```text
basis 由调用方明示
可为 raw width 或 usable width
函数无 Ui/Room/Context 参数
编译期不可能隐式读全局屏幕或设备档
```

v1 不做：

```text
WindowSizeClass 阶梯
dp 换算
设备枚举
断点回调改 padding
双子树常驻 AdaptiveSlot 容器
```

分支翻转应走既有 app rebuild：

```text
remove old subtree
build selected variant
relayout
render
```

C1 焦点语义随之保持：

```text
旧子树 focusable 连带清除
当前 component focus 若在被删子树 → None
FocusRestorer 仍后置
```

## §7 host 测试

新增：

```text
user/berry-ui/tests/constraints.rs
```

覆盖：

```text
Viewport 与 layout() 完全等价
Tight 等价 resized viewport
Tight 0 / u32::MAX 饱和
Loose shrink-wrap
Loose clamp
Loose Stretch 子 collapse wart
Infinite 子包络
damage 恒真实 viewport 裁剪
净树 no-op
显式 dirty 约束切换与 FULL union
Infinite 下 hit 域 = Root 摆放界
Clip 局部性
slot_variant 半开阈值与极值
```

## §8 UITEST 真链

page0 在 EmptyState 后加入一个本地自适应样例：

```text
局部 basis = card 内容宽 1056
阈值 = 600
variant = Expanded
```

布局：

```text
Row 宽 1056、高 120
list   = 360×120
gap    = 12
detail = 684×120
```

注意：

```text
basis 是 1056，不是窗口全局 1080
```

marker：

```text
UITESTADAPT ok v=1 basis=1056
            list=12,623,360,120
            detail=384,623,684,120
```

> L2 起该生产样已升级为 flex 1:2：`v=2`、348/696。上面是 L1 时点的
> 固定分栏历史形态；现行值以
> [BerryUI-L2FlexRatioAllocation-v1.0.md](BerryUI-L2FlexRatioAllocation-v1.0.md)
> 为准。

boot-probe 纯判定：

```text
UITESTADAPT2 ok v=0 basis=320
```

GUI8 normal：

```text
UITESTADAPT ×2（boot probe + runtime）
UITESTADAPT2 ×1
AST bytes 712 → 755
a11y 仍 8
```

像素锚：

```text
list 中心 = surface_variant
detail 中心 = surface
```

既有：

```text
UITESTRENDER ×11
UITESTLAYOUT 精确字符串
UITESTHIT ×3
UITESTCLIP ×2
UITESTSURFACE frames=8,9
C1 focus marker
```

全部不变。

## §9 非目标

后置：

```text
constraint-aware measure
flex/wrap/最小比例分配
ListDetail
Grid breakpoint/span
WindowSizeClass ladder
SafeArea/4-sided padding
Room 自动 config pump
occluded-basis 翻转真链
dp 阈值
subtree-scoped constraints
render_frame 约束源
```

尤其：

```text
wrap/flex 需要内在尺寸随可用宽变化
```

这是 layout v2 独立刀，不在本刀偷做。

最终回归已通过：

```text
berry-ui host（constraints 10 测）
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

## §10 预算

当前候选构建：

```text
总量       = 33,880,912B
rootfs0    = 33,554,432B
rootfs1    = 326,480B
总量 cap   = 134,217,728B
总量余量   = 100,336,816B
```

UITEST：

```text
UITEST.bap = 78,312B
```

rootfs0 仍恰在单卷帽；后续增长必须继续走拆卷/压缩/迁出计划。

## §11 最终镜像

当前候选 SHA-256：

```text
97625e895ce3108c16dd77bf73178766bb2469166e335e91c83ca97cc6dca133
```

复跑镜像 SHA-256 即上值。
