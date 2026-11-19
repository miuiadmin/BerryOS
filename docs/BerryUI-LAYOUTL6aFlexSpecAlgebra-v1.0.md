# BerryUI LAYOUT-L6a Flex Spec Algebra

> 版本：**1.0**｜2026-11-12
>
> 定位：LAYOUT-L6 第一刀现行实施规格。执行序权威 = [GUI框架地基路线-v1.20.md](GUI框架地基路线-v1.20.md)；前置 = [BerryUI-LAYOUTL5PerNodeBoxTextWrap-v1.0.md](BerryUI-LAYOUTL5PerNodeBoxTextWrap-v1.0.md)。L6 完成面拆为 L6a / L6b：本文只拥有 runtime-only flex basis/grow/shrink 代数；嵌套协商、Grid fr/auto、MainAlign/alignSelf、intrinsic query 与持久 constraint cache 归 L6b。
>
> 核心裁决：**flex spec 仍是 runtime-only 侧表，不进 Node/Props/AST/wire**。L2 `set_flex(weight)` 语义冻结为 `FlexSpec { grow: weight, shrink: 0, basis: FlexBasis::Px(0) }` 的语法糖；L2 1:2 与 L3 transposed 语义逐字保持。

## §1 问题

L2/L3 只有 grow weight：

```text
flex 子 basis 恒 0
无 shrink
溢出时 flex 子塌缩，inflexible 子不收缩
basis Auto / Px 无法表达
```

这足以支撑 ListDetail 固定剩余空间 1:2，但不能支撑表单、工具栏和动态内容的主轴协商。

## §2 数据模型

```rust
pub enum FlexBasis {
    Auto,
    Px(u32),
}

pub struct FlexSpec {
    pub grow: u16,
    pub shrink: u16,
    pub basis: FlexBasis,
}

pub struct FlexEntry {
    pub node: NodeId,
    pub weight: u16, // 等价 grow；保留 L2 字段名
    pub shrink: u16,
    pub basis: FlexBasis,
}
```

容量仍为：

```text
FLEX_MAX = 32
```

`weight` 是 `grow` 的存储兼容名。`FlexEntry` 字段扩展记录为 SDK breaking history；仓库内只经 API 创建 entry，不外部构造字面量。

## §3 API

```rust
Ui::set_flex_spec(node, spec) -> Result<(), UiError>
Ui::flex_spec(node) -> Option<FlexSpec>
Ui::set_flex(node, weight) -> Result<(), UiError>
Ui::take_flex(node) -> bool
Ui::flex_iter()
```

冻结糖：

```text
set_flex(weight)
≈ set_flex_spec(FlexSpec {
    grow: weight,
    shrink: 0,
    basis: FlexBasis::Px(0),
})
```

拒面顺序：

```text
悬空/死 id                → NodeBad
grow == 0 && shrink == 0  → NodeBad
kind/props/父容器不 eligible → NodeBad
第 33 个唯一节点           → NodeFull
```

合法特殊形：

```text
grow == 0 && shrink > 0：shrink-only
grow > 0 && shrink == 0：L2 grow-only
basis Px 全 u32 域；布局侧 i32 饱和，不 panic
```

同值覆写 no-op；任一字段变化 dirty 本节点与祖先链。

## §4 分配代数

对固定主轴 Row/Col：

```text
base_j =
    registered:
        Px(v) -> v
        Auto   -> child meas main
    unregistered:
        child meas main

free = container_main - gaps - Σ base_j
```

### grow

```text
free >= 0 且 total_grow >= 1:
    share_j = floor(free × cumGrow_j / totalGrow) - prevTarget
    slot_j = base_j + share_j
```

cumulative-prefix floor 保证总增量恰为 `free`，余数归最后一个 grow 子。

### shrink

```text
free < 0:
    deficit = -free
    pool = Σ shrink_j × base_j

pool == 0:
    不收缩，保持 L2/L3 溢出语义

pool > 0:
    CSS 7.2 型 freeze loop：
        按 shrink × base 比例分配 deficit
        单子 take 超过 base 时冻结为 take = base
        冻结轮回滚未冻结子的本轮预分配，避免 capped 子 base + 未 capped 预分配超过 deficit
        冻结后对剩余 active 子重复
        每轮至少冻结一子，最多 FLEX_MAX 轮
        Σ take = min(deficit, Σ shrinkable base)
```

不变量：

```text
slot >= 0
unregistered 子永不收缩
shrink == 0 子永不收缩
固定子保持 base
无除零
零 heap / 零 panic
```

u128 中间量用于 shrink 乘积；现有 grow 乘积仍留在 u64。

## §5 L2/L3 兼容定理

所有既有 `set_flex(weight)` 登记：

```text
basis = Px(0)
shrink = 0
```

因此：

```text
Σ base = inflexible intrinsic
free = L2 flex_space
slot = 0 + grow share
溢出 pool = 0，走旧不收缩路径
```

L2 1:2、gap、fixed、零空间、余数、生命周期与 L3 transposed pane 的几何、damage、hit、AST 均逐字保持。

## §6 生命周期与缓存

```text
apply_desc full rebuild    清 flex 表
apply_desc_keyed matched   保 slot+generation，flex spec 存活
remove subtree             清子树 entry
slot reuse                 gen 对拍，不继承旧 spec
set_props 漂移出 eligible  自动摘除
```

本刀不新增 measure constraint cache。分配仍在 place 阶段一次完成，不写 `meas_w/h`；L5 wrap 子仍消费 per-node incoming box，wrap-to-flex-slot 归 L6b。

## §7 Host 证据

新增：

```text
user/berry-ui/tests/flex_spec.rs
```

`user/berry-ui/tests/flex_spec.rs` 当前 16 组测试覆盖：

```text
set_flex 糖等价
Px basis grow
Auto basis grow
Col 纵向 Auto basis
ListDetail Px basis 与 L2 恢复
shrink 简单比例
shrink freeze loop
deficit >= 可收缩 base 时不产负 slot
64 轮确定性随机 spec：grow/shrink 精确填满与非负不变量
pool 零回退
固定子不收缩
shrink-only 合法
dead spec / ineligible 拒面
同值 no-op / 变更 dirty
AST bytes 不变
full rebuild 清表 / keyed 保表
measured purity
```

## §8 Guest 证据

L6a 是 runtime-only SDK 基础面，刻意不接入 guest app：

```text
不新增 UITEST marker
不新增 AST/wire 字段
不提交额外 frame
不改变生产 ListDetail 1:2 登记
```

GUI8 证明面因此是零漂而不是新 marker：

```text
GUI8 normal-boot
GUI8 dual-boot canonical persistence
smoke-image 138/138
UITESTAST bytes=860 / a11y=9
UITESTADAPT 348/696
UITESTLISTDETAIL 348/696
L1..L5 既有 marker 与像素锚
```

首个真实 flex-spec guest 消费者与 `UITESTL6` marker 归 L6b；届时必须证明恢复 L2 生产形态后再提交帧。

---

## §9 SDK

新增 facade：

```text
FlexBasis
FlexSpec
```

`Ui::set_flex_spec` / `Ui::flex_spec` 是既有 `Ui` 方法，不另立顶层 facade 名。`FlexEntry` breaking history 记录字段扩展。

## §10 非目标（L6b 拥有）

```text
嵌套 flex measure negotiation
wrap-to-flex-slot
Grid fr/auto
MainAlign / alignSelf / SpaceBetween
min-content / max-content floor
intrinsic query 公开 API
持久 constraint cache
Props/AST/wire 表达 flex
```

## §11 预算

LAYOUT-L5 基线：

```text
UITEST.bap 105,776B
rootfs.cpio 33,945,368B
rootfs0 33,554,432B
rootfs1 390,936B
总量余量 100,272,360B
```

LAYOUT-L6a 最终：

```text
BROWSER.bap  = 83,956B   (+1,200B)
SETTINGS.bap = 82,336B   (+1,216B)
FILES.bap    = 81,704B   (+1,008B)
PHOTOS.bap   = 101,536B  (+1,296B)
CONTACTS.bap = 85,348B   (+1,152B)
UITEST.bap   = 106,944B  (+1,168B)

rootfs.cpio 33,953,712B  (+8,344B)
rootfs0     33,554,432B  (0B，仍恰 32MiB 单卷帽)
rootfs1       399,280B  (+8,344B)
总量余量   100,264,016B
```

全部增长进入 rootfs1；rootfs0 零增长。

最终镜像：

```text
SHA-256 = 47a9fe5b7c7e66ee283e77e2e39dc74dce6d4eb594eaacd5c8e6de49706e9712
```

---

## §12 验收

```text
cargo test -p berry-ui
just test-host
just check
berry-ui no_std ×2
uitest no_std
launcher / gui-wmd guest check
GUI8 self-test
GUI8 normal-boot
GUI8 dual-boot canonical persistence
smoke-image 138/138
cargo deny licenses
gui_guard + docs_guard + Curia strict
writable_globals
git diff --check
```

最终复跑结果：

```text
cargo test -p berry-ui                  PASS
flex_spec 16/16                        PASS
just test-host                          PASS
just check                              PASS
berry-ui no_std                         PASS
berry-ui guest-font-vmo no_std          PASS
uitest no_std                           PASS
launcher musl check                    PASS
gui-wmd musl check                      PASS
GUI8 self-test                         PASS
GUI8 normal-boot                        PASS
GUI8 dual-boot canonical persistence    PASS
smoke-image 138/138                     PASS
cargo deny licenses                     PASS
gui_guard / docs_guard / Curia strict   PASS
writable_globals                        PASS
git diff --check                        PASS
```

smoke-image 字节对拍：

```text
kfile = 4,772,040B
init  = 33,953,712B
与宿主精确同数
```
