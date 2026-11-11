# BerryUI F2 核心盘点

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI F2 的现状盘点、补齐记录与 SDK 最小冻结面。执行序权威是 [GUI框架地基路线-v1.15.md](GUI框架地基路线-v1.15.md)；本文不改 F0..F9 顺序，只说明 F2 的核心事实与出口证据。F3 主题 token、F4 组件、F5 uitest 与 F6..F8 应用迁移不在本文扩面。

## §1 盘点结论

`berry-ui` 已有完整引擎内核雏形，F2 不重写。现役核心面如下。

| 模块 | 现状 | F2 判断 |
|---|---|---|
| `lib.rs` | 定容保留树 arena、代数 NodeId、树增删/props/text/offset/a11y/press-map、布局与渲染门面 | 保留；不新增第三套状态入口 |
| `desc.rs` | Plain Data ViewSpec/Props、TLV AST 双入口、Msg-only 事件、DescBuilder | 继续「声明即数据」；SDK 只 re-export 冻结子集 |
| `layout.rs` | 后序内在测量 + 前序摆放、Measure 缓存、Col/Row/Pad/Grid/Image/Icon/Text、CrossAlign | 已满足 F2 布局面；新增诊断读取，不改算法 |
| `damage.rs` | RectI、viewport 裁剪、K≤8 合并、Σ×1.25 退化、RectSet | 保留；dirty/damage 语义继续单源 |
| `paint.rs` | damage 驱动、遮挡剔除、DisplayList、overdraw 预算、Present 尾命令 | 离屏 golden 继续作为渲染真值 |
| `hit.rs` | topmost 命中、press/click/drag/cancel referee、速度环与锁轴 | 命中 dump 必须委托同一 `hit::node_at` |
| `metrics.rs` | TextMetrics 与 IconSource 供给槽、Monospace/Empty 默认件 | F2 冻结供给缝，不引入主题系统 |
| `frame.rs` | RUN/SETTLE/IDLE、vsync 协商倒数、credits 与提交/呈现簿记 | 保留；F5 已由 Berry Room `render_display_list` 接宿主 |
| `diagnostic.rs` | **F2 新增**：前序布局 dump 与逐点命中 dump | 补齐布局/命中反 mock 证据面 |
| `sdk.rs` | **F2 新增**：最小 SDK facade | 冻结 F4/F5 消费入口 |

## §2 F2 补齐面

### 布局 dump

`Ui::layout_dump(&mut [LayoutDumpEntry]) -> Result<usize, UiError>` 输出：

```text
前序树序
NodeId / parent / first_child / next_sibling
ViewKind / key / depth
父相对 x,y,w,h
视觉绝对 bounds
measured_w/h
layout_dirty / paint_dirty
```

不变量：

```text
输出顺序 = 前序树遍历；
bounds 与 Ui::bounds 同一保留树结论；
dirty 位是引擎内部状态原样投影；
输出容量不足 = UiError::NodeFull，绝不静默截断；
Root 也可 dump，但不参与命中。
```

### 命中测试 dump

`Ui::hit_test_dump(points, &mut [HitDumpEntry]) -> Result<usize, UiError>` 输出：

```text
x/y
Option<NodeId>
Option<ViewKind>
Option<RectI>
```

不变量：

```text
每个点结论与 Ui::node_at 完全一致；
后兄弟优先 = topmost；
Root 恒不命中；
points 数超过输出容量 = UiError::NodeFull。
```

### SDK 最小冻结面

F4 组件与 F5 uitest 统一从 `berry_ui::sdk` 引用：

```rust
Ui / UiMain / UiError / NodeId
ViewSpec / ViewKind / Props
RectProps / CircleProps / TextProps / TextStyle
ColProps / RowProps / PadProps / GridProps
ImageProps / IconProps
TextBuf / Msg / MsgBuf / Axis
CrossAlign / LayoutStat / RectI
TextMetrics / MonospaceTextMetrics
IconSource / EmptyIconSource
FontSource（F5 guest 字形 VMO 档）
LayoutDumpEntry / HitDumpEntry
```

进入或移出该 facade 的规则：

```text
先有值级 host 测试；
保持 Plain Data 或 &mut self 引擎门面；
no_std 零分配；
破坏名字/签名先更新本文接口表。
```

## §3 F2 出口证据

| 路线出口门 | 证据 |
|---|---|
| Host 单测 | `user/berry-ui/tests/diagnostic.rs` 钉布局前序、绝对界、dirty 传播、纯色零重测、topmost、Root 排除、容量满 loud |
| 离屏 golden | `tests/offscreen.rs::golden_cmd_stream_col_scene` 在既有 Col/Rect/Text golden 场景中同时验证 layout dump 与 hit dump，防第二几何真源 |
| 布局 dump | `LayoutDumpEntry` 值级断言；dump bounds 与 `Ui::bounds` 逐项对拍 |
| hit-test dump | `HitDumpEntry` 与 `Ui::node_at` 逐点对拍；重叠后兄弟 topmost |
| SDK 最小接口 | `sdk.rs` 冻结 re-export；diagnostic 测试仅经 facade 构造核心场景 |
| S7 dirty A/B | `--features s7` 全套通过；`set_text` 的 scale 从 `TextProps.style` 单源捕获，不再误读 `TextBuf` |
| 全量回归 | F2 收口命令见 §4 |

## §4 验收命令

```sh
cd user
CARGO_TARGET_DIR=<target> cargo test --manifest-path Cargo.toml \
  -p berry-ui --target aarch64-apple-darwin

# S7 属性级失效 A/B 面同套全测（dirty 语义不能只过默认面）。
CARGO_TARGET_DIR=<target> cargo test --manifest-path Cargo.toml \
  -p berry-ui --target aarch64-apple-darwin --features s7
```

非 Rust 回归仍按仓库主门执行：

```sh
just test-host
just abi-check
python3 tools/gui8-smoke.py --self-test
python3 tools/gui8-smoke.py --layout-selfheal
python3 tools/gui8-smoke.py --normal-boot
bash tools/smoke-image.sh out/berryos.img <AAVMF_CODE> <AAVMF_VARS>
```

## §5 边界

- F2 不引入 Berry Design token；颜色/字号/圆角/间距的收敛面见 [BerryDesign-F3主题与基元-v1.0.md](BerryDesign-F3主题与基元-v1.0.md)。
- F2 不新增 Button/Card/Switch 组件；组件面见 [BerryUI-F4组件第一版-v1.1.md](BerryUI-F4组件第一版-v1.1.md)。
- F2 不把 UITEST 扩成组件 showcase；归 F5。
- F2 不迁移 Settings/Files/Contacts/Photos/Browser；归 F6..F8。
- F2 不把 `berry-nshfont` 删除；该件仍只服务五个迁移前 Native Shell，F9 拆归 gfx/ui 后退役。
