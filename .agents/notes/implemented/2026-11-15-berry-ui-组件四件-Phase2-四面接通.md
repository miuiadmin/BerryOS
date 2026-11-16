# berry-ui 组件四件 Phase 2 首批：Button/Toggle/Slider/Progress 四面接通

Status: implemented

## Problem

诊断确认「41 ViewKind 只有 9 个可用」是最大问题：数据层（desc.rs Props/TLV）完备，但 Button/Toggle/Slider/Progress 四件在布局是 (0,0) 占位（永不命中、永不显示）、paint 无视觉分支、set_props kind 门全拒、事件 ID（on_tap/on_change）纯数据不接线——四个数据壳。要求：只改 paint.rs / layout.rs / lib.rs / hit.rs + 新测试，不动 desc.rs（Msg 枚举冻结、不加变体）、不动 mask.rs；渲染只用已有原语（fill_rect / draw_glyph_run）组合、固定色值起步；现有 205 测全绿（默认与 s7 两面）。

## Decision

1. **布局（layout.rs）——固定内在 + 内在型叶**：intrinsic 四臂 = Button 宽 `max(len×8+32, 64)` 恒高 48、Toggle 52×32、Slider 200×44、Progress 200×8；final_size 四件从「随父填充」臂移入「内在型叶」臂（Text/Image/Icon 同列——不吃 avail、十字轴拉伸不及）。选内在型叶而非保持随父填充：随父填充下 Root 直子会变 viewport 尺寸巨物（place_children `_` 臂 Root 型全幅分配），内在型叶语义与 Text 一致、天然可摆。几何常量单源放 lib.rs「组件四件 v1 几何常量」块（pub(crate)，DRAG_SLOP_PX 供 hit.rs 同款先例）——layout 测量、paint 渲染、lib 命中映射三面共用。Button 标签字宽 8 = S8x16 档 cell_w（paint 用同档图集推进——测量/渲染不分叉）。
2. **渲染（paint.rs）——固定色板 + 原语组合**：四件入 cull_walk/emit_walk_all 可见叶列；不透明度不可证（Outlined 有洞、Text 式无底）——与 Text 同列恒不计遮挡源（保守正确）。emit_node 四分支：Button Filled = ACCENT 底 + 居中标签（glyph run 走 Text 同款 scratch 切分）、Outlined = 四 fill_rect 描边（无 stroke 原语；底不填——Color 无 alpha，实心 surface 会盖应用底）、Text 式 = 纯标签；Toggle = 胶囊轨道（三片）+ 滑块（八角三片）；Slider = 轨道线 + 填充至圆心 + 拖动圆；Progress = 轨道 + permille 填充条（Circular/Indeterminate v1 统一线性——数据随 props 携载，圆/动画面挂后续刀）。圆/胶囊 = 八角三片近似（「不发明新绘制指令」纪律的原语组合）。色板 pub 常量（ACCENT/TRACK_OFF/TRACK_DIM/KNOB/OUTLINE/ON_ACCENT/ON_SURFACE——golden 对拍锚，主题系统落地后迁出，PRESSED_BG 同款文档化豁免）。
3. **交互（lib.rs widget_react）——hit 事件派发，D18 press_react 同形**：referee 本就 kind 无关（hit.rs 零改动）——四件有了真实尺寸后 node_at 自然命中、Click/Drag 自然产出；接线全在 pump_input 的逐 Msg 消费环（press_react 旁加 widget_react）：Click 落 Button → 派 on_tap；Click 落 Toggle → 经 set_props 翻 checked + 派 on_change（value=0/1）；DragStart/DragMove 落 Slider → 行程映射跳跟/跟手（圆心行程 [x+r, x+w−r]，与渲染同式单源），值变才写才派发。**事件经新 EventBuf（WidgetEvent{node,event,value} 值面）携出，`Ui::take_events()` take 语义消费**——不加 Msg 变体（desc.rs 冻结）、ADR #52 零闭包不动、Msg 面不劫持（Click 原样交 app，双消费合法）；满帽 8 计 lost_widget_events loud（Gesture::lost_msgs 同形）。reset_arena 随树清（旧 NodeId 悬空无义）。
4. **命令面（lib.rs）——set_props 开口 + 值域先验**：kind 门加 Button/Toggle/Slider/Progress 四对（余组件件仍拒——防半落状态）；props_geom_equal 四臂（Button label/style 入几何、on_tap 不比；Toggle/Slider/Progress 内在恒定 → 几何等价恒真——纯绘面零重排）；`widget_value_ok` 单源先验（Slider value≤1000 且 min≤max、Progress value≤1000）add/set_props 两口同律（AST/builder 面 BadValue 已拒，UiError 族冻结 NodeBad 兼任——Grid 零维先例）。
5. **测试（tests/widgets.rs，5 测）**：① 布局值级 + FULL 帧逐 Cmd golden（14 填充序列 + 标签字形双轴居中 + cmds=16——「渲染非空 op 集」DoD 门）；② Toggle 点击翻转两来回（事件 value 1→0 + 轨色 ACCENT↔TRACK_OFF 对拍 + Msg 不劫持）；③ Button on_tap（含界外击不派发）；④ Slider 拖跟（permille 611→1000 值级 + 填充宽随值 + 布局零扰动）；⑤ set_props 四件生效 + 拒面矩阵。双面分录一处：同值 set_props 的 measured==0 断言 A 面精确（props_geom_equal 四臂）、B 面（s7）≥1——mask::props_diff 未随本批扩表（约束不动 mask.rs），组件对落保守失配臂 GEOM∪PAINT 全置（正确但保守，扩表面挂 S7 后续刀）。

## Alternatives considered

- Toggle/Slider 交互改由 app 收 Click/Drag 自管（引擎零参与）：否——Toggle 的 checked 是引擎 props 状态、翻转走 set_props 单漏斗才与 damage/dirty 同帧生效；纯 app 自管会让「引擎视觉状态」与「app 影子状态」双源分叉。
- 事件 ID 塞进 Msg 新变体（如 Msg::Widget{..}）：否——desc.rs 冻结（本刀约束），且 Msg 是手势 referee 产物面；独立 EventBuf 值面更贴近「引擎产、app 消费」的 ADR #52 分层。
- Button 宽用 TextMetrics 供给缝度量（tm.measure）：否——ButtonProps 无字号档字段，固定 8px/字（= S8x16 cell_w）与渲染档单源；换供件（比例 HD）会造成测量/渲染分叉，固定值起步更诚实。
- Outlined 按钮填实心 surface 底：否——Color 无 alpha 通道，实心底会盖住应用背景（Outlined 语义 = 透底）；只描边。
- 四件保持随父填充（final_size 原臂）、只给 intrinsic：否——Root 直子全幅叠放（place_children `_` 臂）= 320×240 巨型按钮，不可用；内在型叶是 Text 同列的既有词汇。
- 顺带扩 mask.rs props_diff 四臂：超本刀授权面（约束四文件 + 测试）——不取，s7 面保守臂行为已双面分录钉板。

## Consequences

- 四件从 0 分数据壳升至 4 分完整可用（数据/渲染/布局/交互/命令面）；41 kind 可用数 9→13。
- 固定值起步的三处刻意债务（文档化迁出锚）：几何常量块（主题/样式系统）、色板块（同）、Circular/Indeterminate 线性渲染 + 八角近似圆（圆/动画原语刀）。
- slider 拖跟受 referee 锁轴约束：垂直主轴拖动时 DragMove 只透传冻结 lock_x——水平滑块不跟手纵拖（合义行为，非缺陷）。
- s7 实验面：组件 props 走保守失配臂（GEOM∪PAINT）——Toggle 翻转触发保守重排，正确不精确；mask.rs 扩表挂 S7 后续刀。
- 事件面新公共 API：`WidgetEvent` / `EventBuf` / `WIDGET_EVENT_MAX` / `Ui::take_events` / `Ui::lost_widget_events`——app update 消费面；guest 消费者（launcher 等）零强制迁移（不用不编译进产物面）。
- 后续同类刀的模板已立：余组件件（Scroll/List/Tabs/…）接通时按「intrinsic 臂 + 可见叶列 + emit_node 分支 + widget_react 扩臂 + set_props kind 门/props_geom_equal/props_diff 三表」五处同批扩。

## Related

- 前篇（数据层）：[2026-11-15-berry-ui-组件七件-Phase1-数据层.md](2026-11-15-berry-ui-组件七件-Phase1-数据层.md)——其 Consequences 钉的「命令面 Phase 1 边界」由本刀收编一半（四件；Scroll/List/Tabs 仍待）。
- 后续批次数据层：[2026-11-15-berry-ui-组件P1批九件-数据层.md](2026-11-15-berry-ui-组件P1批九件-数据层.md)、[2026-11-15-berry-ui-组件P1尾五件-数据层.md](2026-11-15-berry-ui-组件P1尾五件-数据层.md)
