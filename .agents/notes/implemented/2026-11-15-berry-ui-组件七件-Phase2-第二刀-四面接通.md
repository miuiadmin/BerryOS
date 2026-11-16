# berry-ui 组件七件 Phase 2 第二刀：Check/Radio/Step/Seg + Stack/Spacer/Divider 四面接通

Status: implemented

## Problem

继首批（Button/Toggle/Slider/Progress）后的第二刀七组件通电：P1 批九件中的七件（交互四件 Check/Radio/Step/Seg + 布局三件 Stack/Spacer/Divider）在布局是 (0,0) 占位（永不命中、永不显示）、paint 无视觉分支、set_props kind 门全拒、事件 ID 纯数据不接线——七个数据壳。P1 数据层笔记钉的命令面边界（add 白名单无 Stack / set_props 拒九件）本刀收编。约束同首批：只改 paint.rs / layout.rs / lib.rs + 新测试（widgets2.rs），不动 desc.rs（Msg 枚举与字段冻结）、不动 mask.rs；渲染只用已有原语组合、固定色值起步；现有 210+213 测全绿（默认与 s7 两面）。

## Decision

1. **布局（layout.rs）——交互四件内在型叶 + 布局三件分臂**：intrinsic 新七臂 = Check/Radio 24×24、Step 120×36、Seg = 段数×80 宽（u64 累计防溢出）、**Stack = 子聚合 max×max**（z 序叠层——与 Col/Row 的 Σ 聚合不同源：子全幅叠放时容器内在取最大子尺寸）、Spacer 0×0、Divider (0, thickness)。final_size：交互四件入首批「内在型叶」臂（不吃 avail）；Divider 独臂 = (avail_w, meas_h) 半随父型（高恒 thickness、宽随父拉通）；Stack/Spacer 保持 `_` 臂全随父（Stack 分配面即叠层面；Spacer 吃满 avail——flex 权重分配挂后续布局刀）。place_children `_` 臂对 Stack **天然合义**（本刀确认并注释钉板）：全子 (0,0) 同位叠放 = z 序语义本义，内在型子保内在尺寸、随父型子吃满——零新摆位分支。
2. **渲染（paint.rs）——固定色板扩义 + 原语组合五分支**：五件（Check/Radio/Step/Seg/Divider）入 cull_walk/emit_walk_all 可见叶列（Stack/Spacer 留容器列——无视觉面）；恒不计遮挡源（首批同列保守）。Check = 方框描边四 fill_rect（Outlined 按钮同形，未选 OUTLINE / 选中 ACCENT）+ 选中态双矩形勾（短臂下角块 4×4 + 长臂上行条 3×9，交叠成勾角）；Radio = 八片环（四直段 + 四角块——八角近似同族新助手 ring8，比例随 d 推导）+ 选中内圆（oct3 半径减半复用）；Step = TRACK_DIM 底 + 双分隔线 + 三组字形（'-'/'+'/十进制值——S8x16 同档区带居中，emit_button_label 升格 emit_widget_text 共核 + fmt_u32 手写十进制）；Seg = 等分底 + active 段 ACCENT 高亮 + 段间分隔线（active 先发、隔线后发——高亮段两界恒可见）；Divider = 单 fill_rect 带 inset（thickness 已入节点高）。
3. **交互（lib.rs widget_react）——Click 臂两件 + Release 臂两件**：Click 落 Check → 翻转 checked + on_change（Toggle 同形）；Click 落 Radio → 置 selected + on_change（value 恒 1 置位语义、已选再击仍派发——组互斥裁决在 app 面）。**Step/Seg 挂 Release 臂而非 Click**：分段命中需要击点横坐标，而 `Msg::Click` 无坐标面（desc.rs 冻结不加字段）——Release 与 Click 在非拖 up 路径同拍成对双发且携 x/y，坐标消费面挂此臂零新面。Step 半区判据 = 击点与节点水平中线，左半 saturating_sub(step).max(min) / 右半 saturating_add(step).min(max)，**clamp 贴边零派发**（Slider「值变才派发」同形）；Seg 段索引 = 相对横坐标 ÷ (w/段数) + 越界钳尾段，值变（≠ active）才写才派发。
4. **命令面（lib.rs）——白名单/kind 门/两表扩**：add() 容器白名单收编 Stack（P1 数据层笔记钉的边界收口）、六新叶入显式「叶带子拒面」列（AST 面同律）；set_props kind 门加 Check/Radio/Step/Seg/Divider 五对——**Stack/Spacer 仍拒**（align/flex 未消费，防半落状态）；props_geom_equal 五臂（Check/Radio/Step 恒真——纯绘面；Seg segment_count 入几何；Divider thickness 入几何、inset 两端纯绘）；widget_value_ok 加 Step（step>0 且 min≤max 且 value∈[min,max]）与 Seg（count>0 且 active<count）——AST 面 BadValue 域同律，add/set_props 两口执法。几何常量单源 lib.rs「组件七件 v1 几何常量」块（首批同款）。
5. **测试（tests/widgets2.rs，7 测）**：① 布局值级 + FULL 帧逐 Cmd golden（19 填充序列 + Step 三段字形区带居中 + cmds=23——「渲染非空 op 集」DoD 门）；② Check 点击翻转两来回（事件 value 1→0 + 勾现勾消 + 框色换）；③ Radio 置位（内圆八角三片 + 已选再击仍派发 + 界外击不派发）；④ Step 半区加减（5→8→10→贴边零派发→7→4→1→0→贴边零派发 + 值字形随变 + 布局零扰动）；⑤ Seg 击点折段（active 高亮随击移段 + 同段再击零派发 + 界外击）；⑥ 布局三件（Stack max×max + 全幅叠放 + z 序命中 topmost + 画家序 2 cmd；Spacer 吃满 Pad 余幅 + Col 主轴零占；Divider 高 thickness 宽随父 + inset 单线值级）；⑦ set_props 五 kind 生效 + 拒面矩阵（值域两口 / kind 失配 / Stack·Spacer 半落拒 / 叶带子拒）。既有 tlv4.rs 的「Stack 命令面父位拒」占位钉板随接线翻正（原断言文本即标注「Phase 2 接线面」）。双面分录同首批：同值 set_props measured==0 断言 A 面 / ≥1 B 面（mask::props_diff 未扩表——保守失配臂）。

## Alternatives considered

- Step/Seg 命中坐标改由 Click 携带（扩 Msg::Click 加 x/y 字段）：否——desc.rs 冻结是本刀硬约束，且 Msg 是手势 referee 产物面；Release 与 Click 同拍成对（hit.rs 非拖 up 双发序）且本就携坐标，挂 Release 臂语义等价零新面。
- Step/Seg 在 Press（DOWN 拍）即反应：否——Press 与 Click 语义分家（Click = 按下抬起完整拍）；抬拍反应与首批 Button/Toggle/Check 同拍，app 侧消费时序一致。
- Seg active 段字形/标签渲染：否——SegProps 无标签面（纯选择器，数据层 P1 已定）；v1 高亮即语义，标签供给挂后续刀。
- Stack 实现 StackAlign 摆位（Start/Center/End 对齐内在小于分配的子）：否——本刀 Stack 语义 = 全幅叠放（`_` 臂天然合义），align 消费需要 per-子对齐分支且现有两处消费面（Root 直子恒全幅）无对齐词汇；半实现必与摆放失配（「不半实现」纪律），align 留数据携载挂后续布局刀。
- Spacer 实现 flex 权重分配（Col/Row 剩余空间按 flex 等分）：否——分配语义在父容器游标面（Col/Row place_children），牵动面远超七件刀授权；本刀落「0×0 内在 + final 吃满 avail」两义，flex 分配挂后续布局刀。
- Step 的加减号用 fill_rect 画横/纵横线（不依赖字体）：否——任务钦定文字渲染（'-'/'+'),且与值字形同档同色单源；字体表 ASCII 可印域全覆盖（fbcon FIRST..=LAST）。
- Seg 分隔线在 active 高亮前发（高亮盖线）：否——高亮段两界分隔线是「等分 N 段」的可读性锚，线后发恒可见。
- 顺带扩 mask.rs props_diff 五臂：同首批否决——超授权面（不动 mask.rs），s7 面保守臂行为已双面分录钉板。

## Consequences

- 七件从数据壳升至完整可用；可用 ViewKind 13 → 20（41 kind 余 21 件待接）。
- 测面 210+213 → 217+220（widgets2.rs 新 7 测）两面全绿；none 目标（aarch64-unknown-none）编译净。
- 刻意债务三处（文档化迁出锚）：StackAlign 消费面（叠层对齐刀）、Spacer flex 权重分配（容器游标刀）、Divider 纵线走向（随容器主轴定——v1 恒横线）。
- Radio 恒派发语义：已选再击仍产 on_change——app 组互斥逻辑须幂等（置位语义与「值变才派发」的 Check/Step/Seg 分家，钉板于测试③）。
- widget_react 的 Release 臂成为坐标消费面先例：后续需击点坐标的组件（如 P0 尾批 Select 展开定位）同法挂 Release，不扩 Msg。
- 既有钉板翻转一处：tlv4.rs `p1_add_face_smoke` 的 Stack 父位拒断言翻为收编正断言（占位钉板的预期演进，非行为回归）。
- 后续批次模板不变：intrinsic 臂 + 可见叶列 + emit_node 分支 + widget_react 扩臂 + set_props kind 门/props_geom_equal/props_diff 三表同批扩（props_diff 仍欠两批组件臂，挂 S7 后续刀）。

## Related

- 首批模板：[2026-11-15-berry-ui-组件四件-Phase2-四面接通.md](2026-11-15-berry-ui-组件四件-Phase2-四面接通.md)——五处同批扩的模板源；其 Consequences 钉的「余组件件接通模板」本刀为第二例。
- 数据层前篇：[2026-11-15-berry-ui-组件P1批九件-数据层.md](2026-11-15-berry-ui-组件P1批九件-数据层.md)——其 Consequences 钉的「命令面 P1 批边界」（add 白名单无 Stack / set_props 拒九件）由本刀收编七件（Badge/Sbar 仍待）。
