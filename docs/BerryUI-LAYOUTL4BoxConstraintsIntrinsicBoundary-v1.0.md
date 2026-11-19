# BerryUI LAYOUT-L4 Box Constraints & Intrinsic Boundary

> 版本：**1.0**｜2026-11-12
>
> 定位：LAYOUT-L4 现行实施规格。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)；L1 基线 = [BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md](BerryUI-L1LocalConstraintsAdaptiveSlot-v1.0.md)；L2/L3 = [BerryUI-L2FlexRatioAllocation-v1.0.md](BerryUI-L2FlexRatioAllocation-v1.0.md) / [BerryUI-L3TransposedContainerFlexListDetail-v1.0.md](BerryUI-L3TransposedContainerFlexListDetail-v1.0.md)；研究依据 = [../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md](../research/GUI-15-官方SDK与UI架构一手源深研-2026-11.md) E6/§8.1/§8.3。
>
> 核心裁决：**约束先成为可测代数，不在本刀重写布局内核**。四元组只播种 Root intrinsic boundary；既有单遍 measure/place 结构保持不变。

---

## §1 背景

L1 已有四档根约束：

```text
Viewport
Tight { w, h }
Loose { w, h }
Infinite
```

限制：

```text
无 min/max 四元组
无 fit/intersect/deflate 纯代数
L2 flex share 与 L3 pane slot 只散在 place 代码
constraints down / sizes up 尚未形成机器钉板
```

L4 不解决：

```text
完整单遍协商内核
每个节点接收 incoming box
Text wrap
flex grow/shrink/basis/wrap
Grid fr
```

---

## §2 数据模型

新增：

```rust
pub struct BoxConstraints {
    min_w: u32,
    max_w: u32,
    min_h: u32,
    max_h: u32,
}
```

结构不变量：

```text
min_w <= max_w
min_h <= max_h
u32::MAX = unbounded
字段私有，只经代数方法消费
```

构造：

```rust
BoxConstraints::new(min_w, max_w, min_h, max_h) -> Result<Self, UiError>
BoxConstraints::tight(w, h)
BoxConstraints::loose(max_w, max_h)
BoxConstraints::infinite()
```

代数：

```rust
min() -> (u32, u32)
max() -> (u32, u32)
is_tight() -> bool
is_infinite() -> bool
fit(w, h) -> (u32, u32)
deflate(w, h) -> BoxConstraints
intersect(other) -> Option<BoxConstraints>
```

语义：

```text
fit        sizes up：每轴独立夹到 [min,max]
deflate    constraints down：扣 padding/gap/固定占位；min 下折 0，max 饱和减
intersect  两盒相交；空盒返 None
```

非法 `min > max` loud 拒，绝不静默 normalize。

---

## §3 L1 兼容糖

L1 API 保持不变：

```rust
Ui::layout_with(Constraints)
```

内部转换：

```text
Viewport(w,h) -> tight(w,h)
Tight(w,h)    -> tight(w,h)
Loose(w,h)    -> loose(w,h)
Infinite      -> infinite()
```

新入口：

```rust
Ui::layout_box(BoxConstraints) -> Result<LayoutStat, UiError>
```

两者都是 pull-based 一次性布局：

```text
不保存上次约束
净树调用 { measured:0, placed:0 }
切换约束需显式制造 dirty
render_frame 仍用 Viewport
```

---

## §4 Root intrinsic boundary

L4 的约束感知只落在 Root intrinsic：

```text
child_extent = max(child.meas_w), max(child.meas_h)
root intrinsic = box.fit(child_extent)
root final = root intrinsic
```

性质：

```text
Tight/min=max：子包络被覆盖，等价旧 L1 Tight/Viewport
Loose：child extent clamp 到 max
Infinite：不设上限
任意 min：可显式抬高 shrink-wrap 下界
任意 max：可显式压低包络上界
```

子内容仍可合法溢出 Root 约束盒；视觉/hit 裁剪规则不变：

```text
damage/paint clip = 真实 viewport
局部裁剪 = 结构性 Clip
hit 根域 = Root 摆放界
```

---

## §5 单遍与短路边界

不引入 Android 式多遍 measure，也不做 flex 权重重分配。

```text
一次 relayout = 一后序 measure + 一前序 place
每个 slot 至多进入 measure 一次
净树相同约束 = {0,0} 短路
flex cumulative-prefix 分配保持 L2/L3 原样
```

L4 以 `LayoutStat.measured/placed` 与 host 测试钉板：

```text
同树 Tight/Loose/Infinite 三次显式 dirty 布局 measured 相等
净树重复 layout_box = 0/0
默认 Viewport 路径 bounds/damage 与旧 layout() byte-equivalent
```

---

## §6 L2/L3 纯函数表达

L2/L3 的 slot 语义可映射为 Box 约束：

```text
UITEST 1056 宽、12px pane gap：
flex 可分域 = loose(1056,120).deflate(12,0) = loose(1044,120)

1:2 分配：
pane A = loose(348,120)
pane B = loose(696,120)
348 + 696 = 1044
```

`deflate` 表达 padding/gap/固定占位，`intersect` 表达父子可用域相交。L4 不把这些函数接入 flex 内核，避免 L2/L3 字节漂；先建立代数与测试面。

---

## §7 AST / wire / runtime 合同

```text
ViewKind 零新增
Props 零新增
Msg 零新增
Node 19 字段 / 144B 零漂
encode_desc / apply_desc 零漂
BoxConstraints 不序列化
P0/R3/wmd wire 零变更
```

默认路径：

```text
六个 BerryUI app 现有布局不变
GUI8 AST bytes=860 / a11y=9 不变
L1/L2/L3 全部既有测试不改
```

---

## §8 SDK 面

新增 facade：

```text
BoxConstraints
```

既有：

```text
Constraints
LayoutStat
Ui
```

`Ui::layout_box` 是既有 `Ui` 类型上的方法，不新增独立 facade 名。`BoxConstraints` 进入 SDK ledger，since=`LAYOUT-L4`，stability=`experimental`。

---

## §9 Host 证据

新增：

```text
user/berry-ui/tests/box_constraints.rs
```

7 组测试覆盖：

```text
min>max loud
tight/loose/infinite 构造糖
fit min/max 独立夹取
deflate min 下折 0 / max 饱和减
intersect 空盒返 None
Root intrinsic 任意 min/max
Root 子内容合法溢出
Box tight == L1 Tight
默认 Viewport byte-equivalent
L2/L3 slot 纯函数表达
净树 L4 调用 no-op
显式 dirty 后 measured/placed 计数
二次净树调用 0/0
TextMetrics 计数探针：每次显式布局每 Text 恰 measure 一次
```

既有 L1 suite 10/10 全绿且测试文件零修改。

---

## §10 UITEST / GUI8 真链

UITEST 增加探针：

```text
UITESTL4
```

在既有 page0 树上执行：

```text
set dirty
layout_box(tight 800x1200)
set dirty
layout_box(loose 700x900)
set dirty
layout_box(infinite)
净树重复 infinite -> noop 0,0
set dirty
layout() 恢复真实 viewport
```

marker 输出：

```text
UITESTL4 ok tight=0,0,800,1200
  loose=0,0,700,900
  inf=0,0,1080,1920
  single=41,41,41
  noop=0,0
```

`single=41,41,41` 表示 Tight/Loose/Infinite 三次显式 dirty 布局的 measured 计数完全一致；`noop=0,0` 表示净树相同约束短路。

不新增节点、不提交 frame、不改变 AST。探针恢复后才进入后续 Clip/SEM/pixel 断言。

GUI8 normal boot 恰两次：

```text
boot-probe 实例 ×1
runtime 实例 ×1
```

---

## §11 预算

DESIGN-D2 后基线：

```text
UITEST.bap 101,656B
rootfs.cpio 33,922,872B
rootfs0     33,554,432B（恰 32MiB 单卷帽）
rootfs1     368,440B
总量余量   100,294,856B
```

L4 增长来自：

```text
BoxConstraints 类型与 relayout Root fit
UITESTL4 探针代码
六个 BerryUI app 共享 berry-ui 链接增量
```

rootfs0 零余量，rootfs 总量 +984B 的增长全部进入 rootfs1；五个生产 BerryUI app 因 berry-ui LTO 代码面净缩 88..128B，UITEST 因探针 +1,472B。

---

## §12 最终镜像

```text
SHA-256 = 6499b90692732fe015ba97dac4a49f2c2b2cb679707aad5d1fd7043f8006ccaf
```

---

## §13 回归与验收

```text
cargo test -p berry-ui
just test-host
just check
berry-ui no_std
berry-ui guest-font-vmo no_std
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
cargo test -p berry-ui             PASS
just test-host                     PASS
just check                         PASS
berry-ui no_std                    PASS
berry-ui guest-font-vmo no_std     PASS
uitest no_std                      PASS
GUI8 self-test                    PASS
GUI8 normal-boot                  PASS
GUI8 dual-boot canonical persistence PASS
smoke-image 138/138                PASS
cargo deny licenses                PASS
docs_guard / Curia strict         PASS
writable_globals                  PASS
```

---

## §14 后置

```text
per-node incoming box
box-sensitive measure
Text wrap under max width
TextField shrink-to-width
flex basis/grow/shrink/redistribution
nested flex measure negotiation
Grid fr
intrinsic width/height query pass
MainAlign/SpaceBetween/alignSelf
persistent per-node constraint cache
```
