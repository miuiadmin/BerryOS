# berry-ui 组件七件 Phase 1：ViewKind 9→16 数据层落地

Status: implemented

## Problem

组件清单 Phase 1 要在 `user/berry-ui/src/desc.rs` 扩 ViewKind（9→16）并为 Button/Toggle/Slider/Progress/Scroll/List/Tabs 七件加 Props 结构，约束是只动数据结构层（渲染链 paint/hit/anim 不实现新 kind 渲染）、TLV tag 必须 add-only、cargo check 过、host 单测钉 TLV roundtrip。拦路三问：TLV 格式是否容得下简单追加；穷尽 match 的编译涟漪波及哪些文件（授权面只有 desc.rs + 可能 layout.rs）；清单草图里的 `&'static str` label 与 `f32` value 与 crate 的零 alloc Copy / 整数纪律冲突怎么办。

## Decision

1. **TLV tag add-only 追加（协议零变更）**：kind 字节域 9..=15 顺延钉 Button/Toggle/Slider/Progress/Scroll/List/Tabs（`KIND_BUTTON..=KIND_TABS` pub const）——旧流值域 0..=8 不受扰，9..=15 旧读端本就 `UnknownKind` 拒，无双义重编，不需要任何协议变更。props 域逐件定死：Button 变长 6..=70（len u8 + label + style u8 + on_tap u32 LE，与 Text 同款变长先例）、Toggle 5B（checked u8 ∈ {0,1} + on_change）、Slider 16B（value·min·max·on_change 四 u32 LE）、Progress 5B（value + style u8）、Scroll/List 各 8B（两 u32 LE）、Tabs 12B（三 u32 LE）。
2. **清单草图的两处类型替换**：Button `label: &'static str` → `TextBuf` 内联（≤ TEXT_INLINE=64）——零 alloc Copy 值面纪律 + TLV 流自含（与 Text 同款；清单本就注记「或 TextProps 引用」）；Slider `value: f32` → `u32` permille 0..=1000（`PERMILLE_MAX` 单源常量，清单本就注记「或 u32 permille 免 FP」）——f32 破 Props 族 Eq derive。语义值 = min + value×(max−min)/1000 由消费面折算。
3. **叶/容器分界**：Button/Toggle/Slider/Progress 四新叶入「叶带子拒面」（与 Rect/Text/Image/Icon 同列，AST build_node 与 DescBuilder::begin 两面同扩）；Scroll/List/Tabs 为容器（AST/builder 面可带子——Tabs 内容区、Scroll 滚动内容靠它）。§8.2 块×kind 合法域表不扩：七件全拒 SIZE/ALIGN 块（catch-all 天然拒，A11Y 块全 kind 合法照旧）。
4. **值域拒面新错误变体 `AstError::BadValue { kind, got: u32 }`（add-only）**：UnknownProp 的 u32 载荷版（Slider value=1001 这类越界值装不进 u8）。执法条件：Slider value>1000 或 min>max、Progress value>1000、Scroll offset_y>content_height（视口无关结构不变量）、Tabs 零页或 active≥tab_count。两面同律：parse_props 流面 + builder begin 面（`validate_phase1`，Grid 零维双面执法同型）。
5. **layout 占位不做半实现**：intrinsic 七件一律 (0,0)、final_size 一律随父 (avail,avail)——容器子树仍递归测量/走查不断轨；故意不实现 List count×item_h、Scroll content_h 等内在语义（place_children 走 `_` 臂 Root 型全幅叠放，半聚合会产出与摆放失配的中间态）。paint 走查同理：七件入「无视觉容器臂」（cull_walk / emit_walk_all 两处穷尽 match 扩列），子仍发射，组件自身零渲染。
6. **测试钉板 `tests/tlv3.rs`（tlv2 同款双轨）**：GOLDEN_V3_STREAM 字面量钉七件齐流样本树现产物（含 A11Y×Button 块、Tabs 带子）+ arena 往返 roundtrip 字节相等 + BadValue/UnknownProp/截断/新叶带子拒面矩阵（流面与 builder 面同律）+ add() 命令面烟囱（新 kind 可建可编码、两场对拍同流）。`tests/reconcile.rs` 未知 kind 绊线 9→16 让位新域界（GUI-6 扩 6..=8 时同款挪法）。

## Alternatives considered

- Button 作组合件（Rect 背景 + Text 标签 + hit 区）不单列 kind：否——清单裁决独立 kind（a11y role 与 golden 对拍锚），本刀照办。
- 在 desc.rs 内拦新 kind 不入 arena 以避免动 paint.rs：否——roundtrip 必须走 apply_desc/encode_desc 全环才作数，拦了数据层就不通；穷尽 match 是类型级义务， paint 两处 match 不扩列则 crate 编不过。
- layout 给 List/Scroll 半内在（count×item_h / content_height）：否——见 Decision 5，避免测量与摆放失配的中间态；Phase 2 布局刀一并做。
- 复用 UnknownProp 承载值域拒：否——got 是 u8，permille 越界值（1001+）装不下；BadValue u32 载荷版 add-only 更诚实。
- 顺带收编 lib.rs 命令面（add 容器白名单加三容器 / set_props kind 门加七对）：超本刀授权面（lib.rs 不在「desc.rs + 可能 layout.rs」内）且属 Phase 2 接线面——不取，钉为已知边界（见 Consequences）。

## Consequences

- **paint.rs 两处穷尽 match 扩列 = 授权外的必要偏差**（本刀唯一）：超出「只改 desc.rs + layout.rs」的字面授权，但不动则 cargo check 不过；改动是纯穷尽性收口（七件入无视觉容器臂，零渲染实现、子仍走查），与「暂不实现新 kind 渲染」的意图一致。
- **命令面 Phase 1 边界（lib.rs 未动，钉为现行为）**：add() 可建新 kind 节点（spec.kind 只拒 Root）但新 kind 作父位全拒 BadParent（容器白名单 catch-all）——即 Scroll/List/Tabs 只能经 AST/builder 带子，命令面不能；set_props 对新 kind 一律 NodeBad（kind_ok 门 matches! 不含七对）——Toggle.checked 翻面等交互挂 Phase 2 收编（届时扩 lib.rs 白名单/kind 门 + mask.rs props_diff 七臂）。
- 事件 ID（on_tap/on_change/on_switch）纯数据面，hit 层派发未接线；anim/swipe 复用面同挂 Phase 2。
- 值域三面执法只落两面（流面 + builder 面）：add() 命令面产物可能带越界值（如 Slider value=5000），编码可过、对端 apply_desc 拒——loud 在解码侧，无静默腐化；builder 产规范流不受扰。
- wire 域界自此 0..=15，未知 kind 绊线在 16；后续再扩 kind 继续顺延 add-only（旧流逐字节不变由既有 GOLDEN_V1/V2 守卫 + 新 GOLDEN_V3 共保）。
- 验证：`cargo check -p berry-ui`（默认与 `--features s7` 两口径）绿；`cargo test -p berry-ui` host 177 测全绿（tlv3 新钉 6 测，reconcile 绊线改钉后 12 测过）；下游 berry-launcher check 绿（仅构造 Props，无穷尽 match 涟漪）。

## Related

- 实施文件：[desc.rs](../../../user/berry-ui/src/desc.rs)（枚举/Props/wire 常量/编解码/叶卫/validate_phase1）、[layout.rs](../../../user/berry-ui/src/layout.rs)（intrinsic/final_size 占位臂）、[paint.rs](../../../user/berry-ui/src/paint.rs)（两处穷尽 match 扩列）、[tlv3.rs](../../../user/berry-ui/tests/tlv3.rs)（新钉板）、[reconcile.rs](../../../user/berry-ui/tests/reconcile.rs)（绊线挪位）
- 前例笔记：[HD 字体比例 advance](./2026-11-13-hd-font-proportional-advance.md)（无关主题——其 Alternatives 曾否决「新 ViewKind 当开关位」，与本刀的正向扩 kind 互不干扰）
