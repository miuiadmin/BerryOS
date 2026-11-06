# GUI-6 实施设计草案——berryui 布局 v1 精修 + 交互三件 + S7 度量

> 版本：**1.0**（v1.0）｜2026-11-06

> 【状态】草案 v2（2026-11-03 三镜对抗审 29 findings 全量合折回写——处置表见 §15；
> 实施序冻结；§6 接口契约逐字冻结——改契约先改本文）。
> 【钦定源】`docs/GUI里程碑规划-2026-10-v1.0.md` :92（GUI-6 行）+ :99-105（立项注记）+
> `docs/架构决策.md` ADR #60（2026-11-02 已立法——本刀 = 兑现 #60，非立法刀）+
> research/SDK-02-声明式UI与SDK拆解报告.md §四 终案 × §6.1②（S7 spike 口径 §6.2）。
> 【读者】本刀 impl agents（§12 编排）+ 实施三镜审查（§3 定案表逐行对表）。
> 【基线】gui6 分支 @ e59c077；berry-ui host 测 ~96（anim 9 / damage 15 / input 10 /
> frame 15 / layout 7 / offscreen 6 / reconcile 10 / tree 8 / dep 8 / resample 8）。

## 1. 刀面定位与范围

GUI-6 = berryui 视图内核的**布局 v1 精修 + 交互三件**：

1. **布局 v1** = 案 3 修正版（现状两趟直落精修：后序内在测量 Σ/max + 前序游标摆放），
   零协议破坏。案 1（layout.rs v2 约束单遍协商重写，~300 行）= v1 收官后**独立刀**，
   ADR #60 已直接排刀（不设升级条件，两次独立立项非两档完备度）。案 2 Taffy 出局。
2. **交互三件** = 手势 referee v2（速度/锁轴）+ 缓动扩档（QuadOut/CubicOut）+
   声明式 pressed-state 短路径。
3. **S7 属性级生死门 spike**（feature 门 A/B + kill criteria 入档）。

### 1.1 v1 范围盒七件（ADR #60 冻结面）

| # | 件 | 内容 |
|---|---|---|
| ① | Grid | 固定行列 + 等分轨（auto/fr 禁入）+ 十字轴；gap = 刀内补件（AnimProp::Gap 先例 + Tailwind gap-* 语义，launcher 图标网格刚需——非钦定明文，此处注明出处） |
| ② | Col/Row 增 w/h | Props 增 `Option<u32>` 定尺寸（TLV HAS_SIZE 门控） |
| ③ | Image/Icon 叶 | BYI1 资源挂点（kind 6/7 + IconSource 缝） |
| ④ | 十字轴对齐 | CrossAlign {Start, Center, Stretch}，默认 Stretch 保 golden（豁免面见 §4.1——「保 golden」只对 Rect/容器子成立，内在型叶恒内在尺寸） |
| ⑤ | TextMetrics 缝 | 抽象档位缝（u8 scale id，不焊 FontSize——先于 GUI-2 增件⑧） |
| ⑥ | 容量帽 | MAX_NODES 256→512 / CAP 512→1024 / DESC_MAX 4096→8192 + 栈审计（含递归面，§9） |
| ⑦ | 无障碍 | label/role TLV flags 门控 add-only（role 只立值域不实现） |

### 1.2 out-of-scope（显式出局，防 scope creep）

- **u16 样式词表闭集 / mkapp 编译期执法 / DSL**——SDK-2 前置刀；本刀只落 Rust 侧
  原子语义面（CrossAlign 枚举、role 值域常量）。
- **clip 语义**（子内容越界裁剪）——挂账；v1 SwipePager 页框 = viewport 全宽免 clip。
- **paint_dirty 消费面**——v1 保持纯 damage 驱动（paint_dirty 现状 = write-only 簿记位，
  全仓零读者，本刀只保位不消费）。
- **hover/flick/swipe 手势词、多指语义**——referee v2 只加速度环与锁轴。
- **layout.rs v2（案 1）**——独立刀，本刀零预埋（Constraints 枚举立全签名但 v1 只接
  Viewport 档）。

## 2. 现状基线（侦察锚点浓缩，2026-11-03 五路侦察 + 三镜抽验修正）

### 2.1 布局两趟（user/berry-ui/src/layout.rs）

- `measure` :141-185（后序内在测量）/ `intrinsic` :188-218 / `child_agg` :222-246 /
  `snapshot` :252-282 / `final_size` :289-301 / `place` :304-359 / `place_children`
  :364-452（前序游标摆放）。
- `MeasCtx` :132-136——实测 1028B @CAP=512，CAP 翻倍后 2052B（§9 栈审计）。
- **截断缝 :184**：measure 尾恒 `true`，`:181` 写入 mctx.changed 后全库 write-only；
  measured 计数（测 6 断言 measured==3）语义不动。
- `relayout` 签名（:60-66）：`(nodes, vp_w, vp_h, damage, freed) -> LayoutStat`
  （`let _ = freed;` 先例）；`dmg_add`（:46-53）= viewport 交集后 add。现调用点两处：
  `Ui::layout`（lib.rs:656-666）与 `render_frame`（lib.rs:823-829，anim tick 后直调）。
- 容量帽耦合（注释同步清单五处）：offscreen.rs:343 测试 `Ui<600>`（翻倍后重钉
  `Ui<1100>` 且**测试内 513 叶/513 宽常量同步翻至 1025**）；paint.rs:37-39
  （ORDER_CAP 注释）；paint.rs:115（「N ≤ MAX_NODES=256」注）；layout.rs:33-34
  （MAX_NODES 契约注）；tests/reconcile.rs:355（「3573 ≤ DESC_MAX=4096」注）。

### 2.2 冻结面（Node / Props / 三漏斗）

- Node 19 字段 144B（lib.rs:95-115，冻结注 :90-94）——本刀**零字段增删**（a11y 走
  Ui 侧表，§3-D12）。
- Props 六变体 + `props_geom_equal`（lib.rs:172-184）——唯一调用方 :578（set_props）。
- 三漏斗 = set_props（lib.rs:557-599）/ set_text（lib.rs:601-627，**无旧值比较**——
  同文换文也全量 relayout）/ anim::apply（anim.rs:206-254，第三几何入口）。
- `mark_ancestors_dirty`（lib.rs:331-340）4 调用方（:498/552/592/618）。
- Text 十字轴现状 = 恒内在宽（final_size :296 恒返 meas_w/meas_h；铁证
  tests/layout.rs:69 `bounds(c)==(0,18,16,16)`——Text "hi" 在 Col(100) 内宽 16 非
  拉满 100）——§4.1 豁免行的依据。

### 2.3 TLV 流（desc.rs）

- TLV 常量区 :234-257（KIND_ROOT..KIND_PAD :237-242 / FLAG_HAS_KEY :245 /
  OPT_NONE :248 / DESC_MAX :251 / DESC_DEPTH_MAX=**16** :254 / CHILD_MAX :257）；
  parse_node :445-471；encode_props :328-369；encode_tree/emit_node :478-542。
  版本域**隐式三槽**：kind 字节 0..=5 / flags bit0 / props 子值域（ADR #60 冻结项⑥）。
- **新 kind 静默缝 10 处**：desc.rs:588/:676、layout.rs:379、paint.rs:228/:279/:319/
  :237、lib.rs:563/:465/:172——每处显式臂或显式拒（§8.3）。
- 编译强制面 8 处（Props enum match 族）+ oracle 函数 2 个
  （rich_tree_oracle_bytes tests/reconcile.rs:72-101——用 DescBuilder 即编码器本身
  推导，**拦不住编码器漂移**——D26 字面量守卫的动机）。
- reconcile.rs 绊线：:200 `{0x99}`（未知 kind 拒）、:202 `UnknownFlags{0x02}`
  （bit1 现非法——本刀 bit1 变合法，绊线改钉 0x10；:203 的 0x80 拒 bit1 合法化后
  仍成立可保）。

### 2.4 交互与动画（hit.rs / anim.rs / damage.rs）

- hit.rs：PtrEvent :38-46 / DRAG_SLOP=8px :283（**欧氏判定** `dx²+dy² > slop²`）/
  node_at :142-151 / dispatch :236 纯 referee（ADR #52 Msg-only，零副作用）；
  PTR_MAX=4；Msg 词表 7 条（desc.rs:152-160）。
- anim.rs：AnimProp{OffsetX,OffsetY,Width,Height,Gap}（:28-36）；Easing{Linear,StepEnd}
  （:40-44）；缓动纯整数 i128 中间量 :170-180；ox/oy 仅 Transform 路可写
  （anim.rs:191-205，Ui 无公开 offset 面——D31 动机）；**damage_add :151-155 未 clip
  viewport**（与 lib.rs:369-382 / layout.rs:46-53 口径分歧——正确性 wart，本刀收敛
  D17；连坐重钉 tests/anim.rs:268-304 footprint 测 w 205→100，单列验收）。
- damage.rs 四流程 :234-298：K≤8 合并 / Σ×1.25 面积门 / 第 9 条不可并 → FULL；
  DAMAGE_MAX=8（berry-guiproto lib.rs:214）。
- FrameRec（telemetry.rs:32-33）已有 damage_rects/damage_full，无 pre-merge 计数。
- 递归面（R2 实锤，§9 审计输入）：measure / place↔place_children 相互递归 /
  snapshot / hit_walk（hit.rs:157）/ cull_walk（paint.rs:171-248）/ build_node /
  emit_node（desc.rs:569-638/:491-542）——深度上界均为 nodes.len()，**未迭代化**。

## 3. 设计定案表（刀内裁决全量——D1–D31；三镜逐行对表）

| # | 裁决 | 定案 |
|---|---|---|
| D1 | anim 几何入口统一（**B 面**） | anim::apply 改产 `(idx, ChangeMask)` staging 定容数组；render_frame 持 `&mut Ui` 统一走同一分派；Transform 型（Offset*）保持直写 offset + damage 现路径。落点 = cfg(feature="s7")（§10） |
| D2 | ChangeMask 位面（**B 面**） | `ChangeMask(u8)`：bit0=W / bit1=H / bit2=gap / bit3=bg / bit4=fg / bit5=text / bit6=scale / bit7=cross；GEOM=W\|H\|GAP\|SCALE\|CROSS，PAINT=BG\|FG\|TEXT；GEOM 位→layout_dirty + mark_ancestors + 旧界/父界 damage + paint_dirty；PAINT 位→paint_dirty + 同界 damage；mask=0→零操作。**PadProps.all → GAP 位兼任**（§5 映射表） |
| D3 | set_text 三漏斗收敛（**B 面**） | **长度感知三态**：同文（字节相等）→ mask=0 no-op；等长换文（len 与 scale 均不变——内在尺寸不变）→ TEXT\|BG（纯 PAINT）；变长/scale 变 → TEXT\|BG\|W\|H（GEOM 追加——Text 内在尺寸 = len×cell_w，变长即几何变更，layout 测 6 保绿的生命线）。**公理前提 = 度量等宽**（MonospaceTextMetrics 等长必等宽——R2-8 立法；GUI-2 真字体落地时三态判据须重审，§6 TextMetrics 注已钉） |
| D4 | measure 截断缝（**B 面**） | measure :184 改返 changed（消费 mctx.changed）；oversize 路恒 true（文档化）；measured 计数语义不动。消费点 cfg(feature="s7") 门控——A 面忽略 changed 恒重摆（现状逐字保持） |
| D5 | TLV 版本域纪律 | kind 字节 KIND_IMAGE=6 / KIND_ICON=7 / KIND_GRID=8；flags bit1=HAS_SIZE / bit2=HAS_A11Y / bit3=HAS_ALIGN；块序恒 SIZE→A11Y→ALIGN；reconcile:202 绊线 0x02→0x10（bit4+ 拒）；禁改既有变体定长；合法域表见 §8.2 |
| D6 | Grid wire 与语义 | wire 7B：cols u8 + rows u8 + gap u32(LE) + cross u8；子行主序摆格（i→row=i/cols, col=i%cols）；轨 = (可用 - gap×(轨数-1)) 等分向下取整；拒面错误承载 = **AstError 新变体**（D30） |
| D7 | Image/Icon wire 与资源缝 | Icon 13B：id u16 + fg RGB 3B + w/h 哨兵对 8B；Image 10B：res u16 + w/h 哨兵对 8B（均不含 kind 字节）；哨兵 = u32::MAX 表 None；`trait IconSource`（dims + pixels 借用）——LRU 编排留消费侧（镜像 TextMetrics 缝）；id/res 共享 u16 资源表；Image/Icon 恒不计遮挡源（只产自身 damage）；供给通道 = D28 槽制 |
| D8 | TextMetrics 缝 | `trait TextMetrics { fn measure(&self, text: &[u8], scale: u8) -> (u32, u32) }` + `MonospaceTextMetrics`（FontSize 单源，scale id = 档位序号）；relayout 进参收 `&dyn TextMetrics`（ADR #60 ②钦定）；paint 侧暂不动（GUI-2 增件⑧ 时迁）；供给通道 = D28 槽制 |
| D9 | Constraints 立缝 | `enum Constraints { Viewport, Tight{w,h}, Loose{w,h}, Infinite }` **四变体**（忠于钦定三档 Tight/Loose/Infinite + Viewport = v1 根调用形态）；v1 只接 Viewport（其余三档 debug_assert + 按 Viewport 处理），Infinite 档案 1 v2 接实现 |
| D10 | 容量帽一次到位 | MAX_NODES=512；CAP/ORDER_CAP = `2*MAX_NODES` 常量表达式（单源）；DESC_MAX=8192；offscreen 重钉 Ui<1100> **含测试常量 513→1025**；注释耦合五处同步（§2.1 清单）；crate 内 `const _: () = assert!(size_of::<Node>() == 144)` 等 const 断言族 |
| D11 | Builder 深帽 | DESC_DEPTH_MAX 16→**64** loud（desc 流路径）；与 D29 arena 树深帽分工：builder 帽护构建期递归，arena 帽护运行期递归，64 < 96 自洽 |
| D12 | a11y = Ui 侧表 | `[Option<A11yEntry>; 32]`（node + role u8 + label TextBuf，≈72B/条）——Node 19 字段零改动；role 闭集 {None=0, Button=1, Text=2, Image=3, Toggle=4, ListItem=5} 只立值域不实现消费；TLV HAS_A11Y 块 = role u8 + len u8 + bytes；**生命周期四条**：remove() 子树连带清条目 / reset_arena 与 apply_desc 整树重建清表 / a11y_iter 按活性校验过滤（防御） / 帽满 → NodeFull（D30 立法） |
| D13 | 缓动扩档 | Easing 增 QuadOut / CubicOut；Q16.16 定点（i128 中间量，同 :170-180 纯整数纪律——禁浮点） |
| D14 | 动画显式分型 | `enum AnimKind { Transform, Layout }` + `kind_of(prop) -> AnimKind` 单源 exhaustive match（加 prop 漏分类 = 编译错）；Anim 结构零改动；apply 按 kind 走 D1 双路 |
| D15 | referee v2 | 速度环 `[(ts,x,y); 8]` 96ms 窗：Δt==0（同拍双样本）→ 速度 0；样本 <2 → 速度 0；DOWN/UP/CANCEL 清环；**入环点 = resample 替换前原始 (ts,x,y)**（lib.rs:733-734 替换点之前捕获——resampled 样本不入环防速度低估）。DragEnd 增 vx/vy（px/s，i32 钳 ±32768）。**slop 判定保持欧氏**（零回归——input 10 测零漂）；锁轴 = 触发时位移主轴（\|dx\|≥\|dy\| → X，tie 取 X，文档化）；DragStart 增 `axis: Axis{X,Y}`；DragMove 只透传锁定轴分量 |
| D16 | SwipePager | 控制器组件（**非新 kind**，pub 模块 swipe.rs）：`new(page_count)`（有效域 1..=8 = SWIPE_PAGES_MAX，超 = loud）+ `attach(ui, parent) -> NodeId`（显式挂点，首次建页行 + 圆点行；page 宽 = ui.viewport()）+ `on_msg(&mut self, ui, msg) -> bool`（app update 内调——Msg-only 纯度保住，引擎零闭包）；Row 页 + OffsetX Transform 动画（跟手 = D31 set_offset）；橡皮筋 ×0.5；翻页判据 = 速度 ≥500px/s 或位移 >40% 页宽；QuadOut 240ms 吸附；圆点指示器自管节点组（8 页 × 页树 + 9 圆点 ≤ MAX_NODES 预算——§11 近帽测试）；v1 页框 = viewport 全宽（免 clip 挂账）；实施增补三件 = `tick(ui, now_ns)` 惰性起拍 / `page_node(i)` 填充口 / on_msg 消费 Cancel（取消回弹不翻页）——§6 已同步 |
| D17 | anim damage clip（主线） | anim damage_add 改经 viewport 交集；三处 clip 收敛单源 helper `pub(crate) fn dmg_clip_add`（layout.rs:46-53 / lib.rs:369-382 / anim.rs 三调用点同语义）；**连坐重钉** tests/anim.rs:268-304 footprint 测（w 205→100）——正确性 wart 修复，单列验收不混语义变更 |
| D18 | pressed 短路径（主线） | 声明式映射表 `set_press_map(&[(trigger, visual)])`（Ui 侧表 16 帽，超 = NodeFull loud——D30 立法）；v1 视觉 = bg 色换（记原值，Release/Cancel 还原）；pump_input 期执行走**现状 set_props 语义**（非 geom 变 → damage+paint_dirty 同帧生效——主线无需 B 面 mask）；引擎内零闭包（ADR #52 纯度）；pressed 色 = 常量 PRESSED_BG（bg 亮度减半，无样式系统硬编码文档化）；原 bg=None → 还原 = 移除 bg；签名返 `Result<(), UiError>`（实施裁定：Unit 无法承载 NodeFull loud——§6 已同步）；清空挂 reset_arena 单汇点（remove 子树/apply_desc 重建两路共清） |
| D19 | S7 生死门（feature 门） | feature `"s7"`（host-only）A/B 双实现并存：**A 面 = 现状三漏斗逐字保持（含同文换文全量 relayout 的现状浪费）；B 面 = mask 三漏斗（D1/D2/D3）+ D4 截断**——忠于 SDK-02 §6.2「属性级 = mask 三漏斗 + mctx.changed 消费」全口径；FrameRec 增 `damage_adds: u16`（pre-merge 计数）+ relayout 耗时 ns 计；kill criteria「属性级收益 <20% 帧耗时 → 维持视图级至 v2」入档（§10） |
| D20 | 规划注记回填 | 随刀回填立项注记 5 条漏抄（§14 清单） |
| D21 | 错误面 | UiError 不加变体（NodeBad 兼任 kind 失配；侧表满 → NodeFull——容量族单源立法）；**AstError 加两变体（add-only 合规）**：`GridZeroDims`（cols/rows=0）+ `GridNotFit{cols,rows,children}`（格数<子数——恰载拒因，TooSmall/BadParent 语义均不正） |
| D22 | 范围裁定 | u16 词表/mkapp 执法/DSL = SDK-2 前置刀出局（§1.2）；GUI-6 只落 Rust 侧原子语义面 |
| D23 | golden 纪律 | 十字轴默认 Stretch 保 golden（**Rect/容器子**——内在型叶豁免 §4.1，Text 恒内在宽零漂）；现有 layout 七测零漂移；重钉单列验收两类：容量面致变（Ui<1100>）与 D17 footprint（w 205→100）——单独提交不混语义变更 |
| D24 | reconcile 归类 | Grid 进 add() 容器白名单；Image/Icon 进叶带子拒面（child_count>0 → NodeBad）；props_diff 归类：id/res 变 → W\|H 保守（内在尺寸可能变）、fg → FG——逐字段映射表见 §5 |
| D25 | 窗口期裁 Msg 扩域 | Msg 变体加域（DragStart.axis / DragEnd.vx,vy）趁零消费者窗口（gfxdemo/launcher 尚未消费 Drag*）——ADR #52 形态不动（引擎产/app 消费） |
| D26 | golden 守卫 | rich_tree_oracle_bytes 产物钉字面量常量 GOLDEN_V1_STREAM（旧流字节稳定守卫——现 oracle 用编码器自推，拦不住漂移）；**v2 golden 同法**：三新 kind + 新 flags 样例树产流一次后钉字面量常量，encode→parse→encode 环测试仅辅助（环拦不住对称漂移） |
| D27 | 钉板先行 | 先写 host 钉板测试再动签名（ADR #60 纪律）——清单见 §11（分栏：runtime-red vs compile-red） |
| D28 | 供给槽制（主线） | Ui 持 `tm: &'static dyn TextMetrics` + `icons: &'static dyn IconSource` 两槽 + `set_text_metrics`/`set_icon_source` setter（默认件 = MonospaceTextMetrics / EmptyIconSource——dims 返 None/pixels 返 None 的空实现）；**relayout/paint 内部签名收 `&dyn` 进参（钦定 ADR #60 ②），pub 门面 `Ui::layout()`/`render_frame()` 签名不变**（从槽取传下）——~96 host 测调用面零级联；guest embedder（GUI-8）挂 static 供件 |
| D29 | arena 树深帽（主线） | `TREE_DEPTH_MAX = 96`，add() 时执法（超深 → NodeBad loud）——R2 实锤递归面（§2.4）未迭代化，512 深链 × ~200B/帧 ≈ 数十 KB 穿 32KB guest 栈；96×~200B ≈ 19.2KB + MeasCtx 2KB < 32KB 安全；与 D11 builder 帽 64 分工（64<96 自洽）；**三镜 R2-1 收编**：apply_desc 的 wire 直递归面（build_node）同帽执法——Grid 1×1 嵌套链绕 add() 帽的路面封死（深度参数 + TooSmall 承载，单源引用 lib.rs 常量） |
| D30 | 错误承载立法 | press_map 超 16 / a11y 满 32 → NodeFull（容量族单源兼任侧表满——语义 = 「Ui 容量域满」，文档注释立法理由）；Grid 拒面 → AstError 两新变体（D21） |
| D31 | offset 公开面（主线） | `Ui::set_offset(node, ox, oy)`（内部走 anim Transform 同路径：直写 ox/oy + 子树包络 damage 经 D17 helper）——SwipePager 跟手与 app 直控共用；subtree_abs_rect（anim.rs:121）升 pub(crate) |

## 4. 布局 v1 精修详设

### 4.1 十字轴对齐（CrossAlign）

```rust
pub enum CrossAlign { Start, Center, Stretch }   // wire 0/1/2；缺省 Stretch（升 pub 同 §6——E0446）
```

- 主轴/十字轴：Col 主 = 纵、十字 = 横；Row 主 = 横、十字 = 纵；Grid v1 = cell 内
  双轴共用一档（per-axis 不做）。
- 语义矩阵（十字轴尺寸决策，以 Col 的 w 为例）：

| w (HAS_SIZE) | cross | 子宽 |
|---|---|---|
| Some(x) | 任意 | x（定尺寸覆盖一切） |
| None + **随父型子**（Rect/Col/Row/Grid 子） | Stretch | 父可用内宽（**现状黄金行为，零漂移**） |
| None + 随父型子 | Start | 子内在宽（intrinsic） |
| None + 随父型子 | Center | 子内在宽，摆放居中 |
| None + **内在型叶**（Text/Image/Icon） | 任意 | **恒内在尺寸，不参与十字轴拉伸**（final_size :296 现状——保 golden；tests/layout.rs:69 铁证） |

- 主轴定尺寸（Col 的 h=Some(y)）：该子主轴占位恒 y（内在测量仍跑——damage 需要
  旧界，但占位不随内在变）。
- 摆放：Start = 游标原点对齐；Center = (可用 - 内在)/2；Stretch = 尺寸拉满 +
  原点对齐（= 现状，仅随父型子）。

### 4.2 Grid（kind 8）

- **measure**（后序）：子内在测量 → Grid 内在尺寸 = 行堆叠（rows_needed =
  ceil(n/cols)，行高 = 该行子内在 max，行间 gap）× 列方向内在宽 = 各列内在 max 和
  （+gap）。十字轴 Stretch 子在 measure 期内在值不变（拉伸只发生在 place 的可用域）。
- **place**（前序）：轨尺寸 = (可用 - gap×(轨数-1)) / 轨数，向下取整（**等分轨，
  auto/fr 禁入**）；cell 原点 = 游标步进；子摆放按 cross 档（同 §4.1 矩阵，
  可用域 = cell 两轴；内在型叶同豁免）。
- **拒面**：cols=0 或 rows=0 → `AstError::GridZeroDims`；rows×cols < child_count →
  `AstError::GridNotFit`（loud，超格不容忍静默截断）。
- Grid 自身 w/h（HAS_SIZE）语义同容器通则（§4.1 矩阵作用于 Grid 在父内的占位——
  Grid 属随父型子）。

### 4.3 Image / Icon 叶（kind 6/7）

- 尺寸决策：w/h Some → 定值；None → `IconSource::dims(id)` 内在尺寸；dims 缺
  （资源未注册）→ (0,0) + debug_assert（host 测试恒注册；guest 未注册 = 空绘不炸）。
- paint：经既有 berry-gfx `CommandEncoder::draw_image`（berry-gfx src/cmd.rs:326 +
  ImageSrc :130——段② 已落）发射，paint 侧**新增 Image/Icon 叶臂**（现 emit_node 仅
  Rect/Text 两臂）；icons 供给经 D28 槽（paint 内部签名增 `icons: &dyn IconSource`）；
  fg（Icon 独有）= 调色前景（v1 = alpha 检测 tint，文档化）。
- hit：叶可命中（有界即可 Press/Click）；**恒不计遮挡源**——不参与兄弟遮挡裁剪
  计算，只产自身 damage。

## 5. 三漏斗 ChangeMask 统一（B 面——cfg(feature="s7")）

单源分派函数（lib.rs，三漏斗共用，仅 B 面编译）：

```
fn apply_change(内部, idx: usize, mask: ChangeMask)
  if mask.is_empty() { return }                       // D2 零操作
  if mask ∩ GEOM:                                     // W|H|GAP|SCALE|CROSS
      damage += 旧界（本节点）
      // 实施裁定（段E）：不调 mark_ancestors_dirty——与 §11-6「顶层 measured==0」
      // 互斥（预标脏即全测）；几何上行由 D4 changed 返值承担（measure 尾截断缝）
      damage += 父链各界（父链只读收集）→ 交 viewport（D17 helper）
      layout_dirty = true; paint_dirty = true
  if mask ∩ PAINT:                                    // BG|FG|TEXT
      paint_dirty 位保位（不消费——§1.2）
      damage += 本界 → 交 viewport
```

**props_diff 逐字段映射表**（props_geom_equal 的位面化替身——十静默缝之 lib.rs:172
处置；A 面保 bool 现状）：

| Props 变体 | 字段 → 位 |
|---|---|
| Rect | w→W，h→H，bg→BG |
| Text | scale→SCALE，text→TEXT（len 变另加 W\|H——D3 三态），fg→FG，bg→BG |
| Col / Row | gap→GAP，cross→CROSS，w→W，h→H（实施补——漏列则显式定尺寸变产空 mask，布局永不更新） |
| Pad | all→**GAP（兼任**——pad 内缩改子摆放几何，GEOM 同族；位面不重排） |
| Root | 恒 NONE（根无 props 变更面） |
| Grid（新） | cols/rows→W\|H\|GAP（保守），gap→GAP，cross→CROSS，w→W，h→H（实施补，同上因） |
| Image（新） | res→W\|H（保守——D24），w/h→W\|H |
| Icon（新） | id→W\|H（保守），fg→FG，w/h→W\|H |

- **set_props**（B 面）：props_diff 产 mask → apply_change。
- **set_text**（B 面）：D3 三态——同文 mask=0 直返；等长换文 TEXT|BG；变长/scale 变
  追加 W|H。
- **anim::apply**（B 面）：staging `[(u32, ChangeMask); ANIM_STAGING_MAX]`（帽 = 现
  动画槽帽同源，满 = 既有 loud 形态）；Layout 型 prop → 对应 GEOM 位（Width→W、
  Height→H、Gap→GAP）入 staging；Transform 型（Offset*）→ 直写 offset + 旧/新界
  damage（clip 经 D17 helper）——两条路同函数体，无第三套 dirty 语义。
  **AnimProp::Gap 作用域 = Col/Row**（三镜 R2-6 立法）：Grid 落 no-op 完成语义
  （动画照常结束移除、gap 不动、两面同口径）——扩臂支持 Grid（与 set_props 的
  GAP 位路径对齐）= 后续刀，不在本刀面。
- **render_frame** 期（B 面）：staging 数组统一 apply_change 逐条执行（持 &mut Ui，
  时序 = anim apply 后、relayout 前）。
- **A 面（默认构建）**：三入口保持现状 dirty 语义逐字不变（§10）。

## 6. 接口契约逐字冻结（改契约先改本文）

```rust
// mask.rs（新模块——B 面 cfg(feature="s7") 编译进）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangeMask(pub u8);
impl ChangeMask {
    pub const NONE: Self = Self(0);
    pub const W: Self = Self(1 << 0);
    pub const H: Self = Self(1 << 1);
    pub const GAP: Self = Self(1 << 2);
    pub const BG: Self = Self(1 << 3);
    pub const FG: Self = Self(1 << 4);
    pub const TEXT: Self = Self(1 << 5);
    pub const SCALE: Self = Self(1 << 6);
    pub const CROSS: Self = Self(1 << 7);
    pub const GEOM: Self = Self(0b1100_0111);   // W|H|GAP|SCALE|CROSS（三镜 R1-M1/R2-5/R3-2 勘误：原字面量 0b1110_0111=231 含 bit5=TEXT 系笔误，与「分组不叠」矛盾——以本行 199 为准）
    pub const PAINT: Self = Self(0b0011_1000);  // BG|FG|TEXT
    pub fn is_empty(self) -> bool;
    pub fn intersects(self, other: Self) -> bool;
    pub fn union(self, other: Self) -> Self;
}

// metrics.rs（新模块——主线）
pub trait TextMetrics {
    /// text = 有效字节切片；scale = 档位 id（u8，不焊 FontSize——GUI-2 增件⑧ 迁移缝）
    /// v1 契约不变量（R2-8）：等长字节串必等宽（等宽字体）——B 面 set_text
    /// 「等长换文 = 纯 PAINT 零重测」三态判据的前提；真字体落地时判据改
    /// TEXT|BG|W|H（或经度量缓存比对内在宽）
    fn measure(&self, text: &[u8], scale: u8) -> (u32, u32);
}
pub struct MonospaceTextMetrics;   // FontSize 单源；host 测试默认供件

pub trait IconSource {
    /// 内在尺寸（w/h None 时用）；未注册 → None
    fn dims(&self, id: u16) -> Option<(u32, u32)>;
    /// 解码后 ARGB 借用（0xAARRGGBB row-major）；LRU/解码编排留消费侧
    fn pixels(&self, id: u16) -> Option<&[u32]>;
    /// 实施增补（段C）：provided 默认 false，空供件覆写 true——§4.3 字面与 §11-14
    /// 钦定测互斥的收窄解：默认空供件未注册 = 静默 (0,0)/空绘不炸（guest 面），
    /// 真供件缺注册 / dims-pixels 恰长不符 = debug_assert loud
    fn is_empty(&self) -> bool { false }
}
pub struct EmptyIconSource;        // 默认供件：dims/pixels 恒 None

// layout.rs（签名变更——钉板先行；门面经 D28 槽取，pub 面零级联）
pub enum CrossAlign { Start, Center, Stretch }   // wire 0/1/2（实施裁定升 pub：pub props 携 pub cross 字段，私有入公 = E0446）
pub enum Constraints { Viewport, Tight { w: u32, h: u32 }, Loose { w: u32, h: u32 }, Infinite }
pub(crate) fn relayout(
    nodes: &mut [Option<Node>],
    vp_w: u32, vp_h: u32,
    damage: &mut DamageSet,
    freed: &[u32],
    tm: &dyn TextMetrics,         // 新进参（D8，ADR #60 ②钦定）
    icons: &dyn IconSource,       // 新进参（D7——intrinsic 期 dims）
    cs: Constraints,              // 新进参（D9，v1 只接 Viewport）
) -> LayoutStat;

// paint 内部签名（门面 Ui::paint 不变——槽取 icons）
pub(crate) fn paint(nodes: &[Option<Node>], /* …既有参数… */, icons: &dyn IconSource, /* … */);

// desc.rs（Msg 扩域——零消费者窗口，D25）
pub enum Axis { X, Y }
pub enum Msg {
    Press { node: NodeId, x: u32, y: u32 },
    Release { node: NodeId, x: u32, y: u32 },
    Click { node: NodeId },
    DragStart { node: NodeId, axis: Axis, x: u32, y: u32 },
    DragMove { node: NodeId, x: u32, y: u32, resampled: bool },
    DragEnd { node: NodeId, vx: i32, vy: i32 },   // px/s，钳 ±32768
    Cancel { node: NodeId },
}

// desc.rs（TLV 常量 + 错误扩面）
pub const KIND_IMAGE: u8 = 6;
pub const KIND_ICON: u8 = 7;
pub const KIND_GRID: u8 = 8;
pub const FLAG_HAS_SIZE: u8 = 1 << 1;   // w/h 哨兵对 8B 块
pub const FLAG_HAS_A11Y: u8 = 1 << 2;   // role u8 + len u8 + bytes 块
pub const FLAG_HAS_ALIGN: u8 = 1 << 3;  // cross u8 块
pub const SIZE_NONE: u32 = u32::MAX;    // w/h None 哨兵
pub enum AstError { /* …既有… */
    GridZeroDims,                                  // cols=0 或 rows=0（D21 add-only）
    GridNotFit { cols: u8, rows: u8, children: u32 },  // 格数 < 子数（D21）
}

// desc.rs（DescBuilder 新块 API——实施裁定形：&mut self + Result<(), AstError> 承载
// loud 拒（by-value 链式无法拒——§0 纪律）；块序 SIZE→A11Y→ALIGN 由阶段位图执法（dup/乱序拒））
impl DescBuilder<'_> {
    pub fn size(&mut self, w: Option<u32>, h: Option<u32>) -> Result<(), AstError>;  // HAS_SIZE
    pub fn a11y(&mut self, role: u8, label: &[u8]) -> Result<(), AstError>;           // HAS_A11Y
    pub fn align(&mut self, cross: CrossAlign) -> Result<(), AstError>;               // HAS_ALIGN
    // 三新 begin 变体已落（begin_grid/begin_image/begin_icon，props 一次性传）；
    // props 只入 encode_props 基域（Col/Row = gap；Grid = cols/rows/gap/cross），
    // w/h（及 Col/Row cross）须 .size()/.align() 块 API 另载（R2-7 基域收窄注记）
}

// anim.rs
pub enum Easing { Linear, StepEnd, QuadOut, CubicOut }
pub(crate) enum AnimKind { Transform, Layout }
pub(crate) fn kind_of(prop: AnimProp) -> AnimKind;   // exhaustive match 编译强制

// a11y（lib.rs）
pub struct A11yEntry { pub node: NodeId, pub role: u8, pub label: TextBuf }
pub const ROLE_NONE: u8 = 0;  pub const ROLE_BUTTON: u8 = 1;
pub const ROLE_TEXT: u8 = 2;  pub const ROLE_IMAGE: u8 = 3;
pub const ROLE_TOGGLE: u8 = 4;  pub const ROLE_LIST_ITEM: u8 = 5;
impl Ui {
    pub fn set_a11y(&mut self, node: NodeId, role: u8, label: &[u8]) -> Result<(), UiError>;
    pub fn take_a11y(&mut self, node: NodeId) -> bool;
    pub fn a11y_iter(&self) -> impl Iterator<Item = &A11yEntry>;   // 活性过滤
    pub fn set_press_map(&mut self, entries: &[(NodeId, NodeId)]) -> Result<(), UiError>;  // 帽 16 超满 NodeFull loud（实施裁定：Unit 无法 loud）；清空挂 reset_arena 单汇点
    pub fn set_text_metrics(&mut self, tm: &'static dyn TextMetrics);   // D28 槽
    pub fn set_icon_source(&mut self, src: &'static dyn IconSource);   // D28 槽
    pub fn set_offset(&mut self, node: NodeId, ox: i32, oy: i32) -> Result<(), UiError>;  // D31
}

// swipe.rs（新模块，pub——app 侧控制器）
pub const SWIPE_PAGES_MAX: u8 = 8;
pub struct SwipePager { /* page 状态 + 动画簿记 */ }
impl SwipePager {
    pub fn new(page_count: u8) -> Self;            // 1..=8，超 = panic loud（冻结签名无 Result——实施裁定，#[should_panic] 钉板）
    pub fn attach(&mut self, ui: &mut Ui, parent: NodeId) -> Result<NodeId, UiError>;
    pub fn page(&self) -> u8;
    pub fn page_node(&self, i: u8) -> Option<NodeId>;  // 实施增补：页为空容器，launcher（GUI-8）填内容口
    /// Msg 消费（app update 内调用）；true = 状态已变需重摆；未 attach → false；
    /// 消费 DragStart/Move/End + Cancel（取消 = 回弹当前页不翻页——实施增补）
    pub fn on_msg(&mut self, ui: &mut Ui, msg: &Msg) -> bool;
    /// 吸附推进（实施增补）：动画表需 start_ns 而 Msg 不携时钟——tick 惰性起拍（首 tick 钉 start_ns）
    pub fn tick(&mut self, ui: &mut Ui, now_ns: u64);
}
```

**容量常量**（lib.rs / desc.rs / layout.rs / paint.rs 同源）：

```rust
pub const MAX_NODES: usize = 512;
pub(crate) const CAP: usize = 2 * MAX_NODES;        // layout.rs
pub(crate) const ORDER_CAP: usize = 2 * MAX_NODES;  // paint.rs（注释五处同步）
pub const DESC_MAX: usize = 8192;                   // desc.rs
pub const DESC_DEPTH_MAX: u8 = 64;                  // desc.rs（D11，16→64）
pub const TREE_DEPTH_MAX: u32 = 96;                 // lib.rs（D29，add() 执法）
pub(crate) const ANIM_STAGING_MAX: usize = /* 现动画槽帽同源 */;
const _: () = assert!(size_of::<Node>() == 144);    // const 断言族（D10）
// offscreen 测试帽用顶层 const（非泛型表达式——stable 无 generic_const_exprs）：
const OVERFLOW_NODES: usize = MAX_NODES + 1;        // Ui<OVERFLOW_NODES> 满场右移随源
```

## 7. 交互三件详设

### 7.1 referee v2（hit.rs）

- **速度环**：每指针位 8 样本环形 `[(ts_ns, x, y); 8]`；DragEnd 速度 = 环内 ≥96ms 窗
  的首末样本差分；Δt==0（同拍双样本）→ 速度 0；样本 <2 → 速度 0；DOWN/UP/CANCEL
  清环；**入环点 = resample 替换前原始 (ts,x,y)**（lib.rs:733-734 前——resampled
  样本不入环防速度低估）；vx/vy = i32 钳 ±32768（px/s）。
- **slop 判定保持欧氏**（hit.rs:283 现状零漂——input 10 测零回归）；**锁轴** = 欧氏
  触发时取位移主轴（|dx|≥|dy| → X，tie 取 X，文档化）；DragStart 携 axis；DragMove
  只透传锁定轴分量（跨轴分量忽略——防对角抖动）。
- PTR_MAX=4 不变；resample 路径照旧。

### 7.2 缓动（anim.rs）

- QuadOut = `1-(1-t)²`、CubicOut = `1-(1-t)³`，Q16.16 定点：t = elapsed/dur 先
  i128 除法产 Q16.16，多项式全程 i128（t 最大 1.0<<16 = 65536，立方 ≈ 2.8e14 «
  i128 上限），末尾 >>16 回 i32（同 :170-180 纪律）。
- StepEnd/Linear 零漂移（既有钉板不许动）。

### 7.3 press_map 短路径（lib.rs，主线）

- 表 `[(trigger, visual, saved_bg: Option<u32>); 16]`；pump_input 内 Msg 产出后：
  Press 命中 trigger → visual set_props(bg=pressed 色，saved_bg 记原值)；
  Release/Cancel → 还原 saved_bg。走现状 set_props 语义（非 geom → damage +
  paint_dirty）**同帧生效**，主线零依赖 B 面。
- pressed 色 = 常量 `PRESSED_BG`；原 bg=None → 还原 = 移除 bg。

### 7.4 SwipePager（swipe.rs）

- 状态：page_count、当前 page、拖拽跟手 OffsetX（DragMove 期 `Ui::set_offset`
  直写——Transform 路径免 relayout）、动画队列（QuadOut 240ms 吸附）。
- 拖拽：|dx| 越界（首页左拖/末页右拖）→ 橡皮筋 ×0.5（offset = dx/2）。
- 松手：|v| ≥500px/s → 按速度向翻页；否则位移 >40% 页宽 → 翻页；否则回弹吸附。
- 圆点指示器：pager attach 时自建（page_count 个 Rect 小节点 + gap 行），当前页
  fg 高亮——on_msg 返 true 时 pager 直摆（MAX_NODES 帽内自管，§11 近帽测试）。
- **Msg-only 纯度**：pager 是普通库代码持 &mut Ui，引擎内零闭包零特判。

## 8. TLV 扩面（块序与拒面）

### 8.1 块序（kind 字节后）

```
节点头: kind u8 | flags u8 | props 变体定长块 | [HAS_SIZE: w u32 + h u32] |
        [HAS_A11Y: role u8 + len u8 + bytes] | [HAS_ALIGN: cross u8] | child_count u32
```

- props 块定长（既有六变体 + 三新 kind 变体）**禁改**；延长块恒按 SIZE→A11Y→ALIGN
  序（老流零 HAS_* flags → 零延长块，逐字节兼容）。
- 新变体定长：Grid = cols u8 + rows u8 + gap u32 + cross u8（7B，cross 在 props 内，
  不产 HAS_ALIGN 块）；Icon = id u16 + fg 3B + w/h 8B（13B）；Image = res u16 +
  w/h 8B（10B）。

### 8.2 拒面与合法域表（完备——R2 镜补面）

**HAS_SIZE × kind**：

| kind | HAS_SIZE | 处置 |
|---|---|---|
| Col / Row / Grid | 合法 | 容器通则（§4.1/§4.2） |
| Text | **拒（UnknownProp）** | 三镜 R2-2 回写：TextProps 满载、Node 144B 冻结无落位——加字段即破 Node 帽（实现 apply_size_block 自始即拒，本表原「合法」为设计侧笔误） |
| Rect | 合法（**容读折入**） | 块胜折入 RectProps、再编码归一无块形（字节级 roundtrip 仅对规范形承诺——非规范流 encode→apply→encode 不守恒；builder 面拒 Rect+size 保双入口收敛） |
| Image / Icon | **拒（NodeBad）** | w/h 已在 props 块（双源禁） |
| Pad / Root | **拒（NodeBad）** | 无定尺寸语义 |

**HAS_ALIGN × kind**：

| kind | HAS_ALIGN | 处置 |
|---|---|---|
| Col / Row | 合法 | 十字轴档 |
| Grid | **拒（NodeBad）** | cross 在 props 内，冗余块 = loud（防双源歧义） |
| Text / Rect / Image / Icon | **拒（NodeBad）** | 内在型叶不参与十字轴（§4.1 豁免） |
| Pad / Root | **拒（NodeBad）** | 无对齐语义 |

- flags bit4+（0x10 起）→ UnknownFlags（绊线从 0x02 改钉 0x10；:203 的 0x80 拒保）。
- kind 6/7 带 child_count>0 → NodeBad（D24 叶拒子）。
- Grid 拒面 → AstError 两新变体（D21/D6）。
- HAS_A11Y × kind：全 kind 合法（a11y 是正交标注面，add-only）。
- 尾随字节拒 / 截断拒——既有纪律外推至新块。

### 8.3 静默缝处置（10 处逐点）

desc.rs:588（builder kind 分派）/ :676（流 walk）、layout.rs:379（容器排版 match）、
paint.rs:228/:279/:319/:237（绘制四段）、lib.rs:563（set_props match）/ :465（reconcile
归类）/ :172（props diff）——每处 = 显式臂或显式拒，禁 `_ => {}` 吞。

### 8.4 golden 双轨

- **GOLDEN_V1_STREAM**（字面量常量，D26）：rich_tree_oracle_bytes 现产物钉死——
  守卫「旧流字节稳定」（编码器重构致旧包不可读 = 测试炸；现 oracle 用编码器自推
  拦不住漂移）。
- **v2 golden 同法**：三新 kind + 三新 flags 样例树产流**一次后钉字面量常量**；
  encode→parse→encode 字节相等环测试仅辅助（环拦不住对称漂移）。

## 9. 容量与栈审计（D10/D11/D29 验收面）

| 项 | 现值（CAP=512） | 新值（CAP=1024） | 处置 |
|---|---|---|---|
| MeasCtx | 1028B | 2052B | relayout 栈帧；其中 `changed` 位图 2KB **A 面写不读**（D4 消费仅 B 面——三镜 R2-9 补记；cfg 门控省 2KB 挂账：分面复杂度 > 2KB 收益，暂不加 cfg 噪声进 measure 热路径） |
| old_abs 快照 | 10240B | 20480B | 同上 |
| relayout 数组面 | 11268B | 22532B | host 断言 ≤24KB |
| paint order/bnds | **10240B**（order 2048 + bnds 8192，paint.rs:101-102） | **20480B** | R2 修正（原草案虚报一倍） |
| **递归面** | 深度上界 = nodes.len()（256 链 ≈ 12.8KB 勉强） | 512 链 ≈ 数十 KB **超 32KB** | **TREE_DEPTH_MAX=96**（D29）：96×~200B ≈ 19.2KB + 数组面 2KB < 32KB |
| Ui 结构 | Ui<512>=82520B | Ui<1024>≈165KB（纯倍增）+ **新增侧成员 ≈3.4KB**（a11y 表 32×72B≈2.3KB + press_map 16×20B≈0.3KB + 速度环 4×8×16B=0.5KB + FrameRec.damage_adds） | 静态分配归 embedder（GUI-8）；offscreen 测试重钉 Ui<1100> + 513→1025 常量 |
| builder 深帽 | DESC_DEPTH_MAX=16 | 64 loud | 64×~100B 调用链 <8KB（32KB 栈 25% 线） |

- const 断言族：Node==144B / A11yEntry 尺寸 / Msg size_of——入 lib.rs 常量区。
- **launcher 树规模**（ADR #60 ④立项义务，本表现兑）：SDK-02 §4.7 推算 = 5 页 ×
  24 格 × 2-3 节点 ≈ **240–360 < 512** ✓（含圆点行余量充裕；GUI-8 接线时以实树
  复核入档）。

## 10. S7 属性级生死门（feature "s7"）

- **A 面（默认构建，主线现状逐字保持）**：三漏斗现状 dirty 语义不变——同文换文也
  全量 relayout（现状浪费保留——A/B 对照的诚实基线）。
- **B 面（feature s7，属性级全口径）**：mask 三漏斗（D1/D2/D3——set_props 位面化 /
  set_text 三态 / anim staging）+ D4 截断（measure 返 changed，未变子树截断上行）。
- **度量**：FrameRec 增 `damage_adds: u16`（damage.rs add 入口 pre-merge 计数——
  合并率可观测）+ LayoutStat 已有 measured/placed 计数 + relayout_ns 环形计数。
- **判定**：host bench（tests/bench_s7.rs，标准样例树 ×N 帧，A/B 同树双跑）
  B 面帧耗时收益 ≥20% → 属性级转正（下刀拆 feature 门进主线）；<20% → kill
  （入档：维持视图级至 v2，feature 整体出 CI——主线回到现状三漏斗，零残留）。
  **三镜 R3-3 修订**：bench 内比值/判定文案读数 = **参考级**（单编译面无法
  自测对侧——env 基线 `BERRY_UI_S7_A_PAINT_US`/`BERRY_UI_S7_A_LARGE_US` 供
  B 面算比值打判定文案；大树轴 103 节点 × 1024 帧已补）；kill criteria **终判
  venue 正式迁移**到 GUI-8 embedder 双打点 + 更大量级样例树——本 bench 不作
  终判依据（确定性计数 3→0/帧 / adds 13→2/帧 仍为强方向性证据）。
- berry-ui 未进 guest 镜像——S7 全 host 面，smoke 零扰动。

## 11. 测试计划（钉板先行——先测后码，ADR #60 纪律；分栏防「等不可达的红」）

**术语定义**（三镜合折——防搅混）：**同文** = 新旧字节串相等；**等长换文** =
len 与 scale 均同、内容不同（等宽字体下内在尺寸不变，如 "hi"→"oy"）；**异文** =
len 或 scale 变（内在尺寸变，如 "hi"→"hey"）。

### 11.1 runtime-red/green 组（现状树上可跑，段 A/B 前先写）

1. **A 面保持锚**：set_text 同文 → 现状语义 measured≥1（A 面此测恒绿——守护现状
   不被段 A 基座误伤）。
2. **变长正确性锚**（= layout 测 6 现状绿）：异文 "hi"→"hey" → measured==3 +
   bounds 宽 24——B 面 GEOM 三态的生命线（D3 修法直接源于此）。
3. TLV 绊线现值：flags 0x02 现拒（绿）→ 段 B 后改钉 0x10 拒 + 0x02 过（双态对拍）；
   GOLDEN_V1_STREAM 今天即可钉绿（现产物逐字节）。
4. offscreen 现值：Ui<600> 满场路径绿 → 段 A 后重钉 Ui<1100> + 1025 常量。

### 11.2 compile-red 组（新类型/新字段不存在——随签名同刀落，勿等「先红后绿」）

5. 等长换文双态："hi"→"oy" → B 面 measured==0（PAINT 短路）+ damage 含本界；
   A 面 measured≥1。
6. **钦定钉板**（SDK-02 §4.4）：深 3 层树、最深孙 set_props（GEOM 位）但内在尺寸
   不变 → B 面中间聚合 measured≥1 且顶层 measured==0（真截断缝）；A 面顶层
   measured≥1。
7. 同文 B 面：measured==0（全缓存早退）。
8. 容量满场：`Ui<OVERFLOW_NODES>`（const 路径形态，随 MAX_NODES 自动右移）add 满
   → NodeFull loud（守护对象 = arena 通用行为 + 帽值联动）。
9. Grid 算术：3×2 + gap=4 + viewport 100×∞ → 轨宽 (100-8)/3=30 + 行主序坐标逐子；
   GridZeroDims / GridNotFit 双拒。
10. QuadOut/CubicOut 中点：t=0.5 → 0.75/0.875（Q16.16 定点值逐位钉）。
11. DragEnd 速度：8 样本 96ms 窗差分精确值；Δt==0 → 0；样本 <2 → 0；
    resample 激活时速度取原始流（resampled 样本不入环）。
12. 锁轴：欧氏触发（(6,6) 径 8.49>8 触发）+ 主轴锁定（tie 取 X）+ DragMove 跨轴
    分量忽略；slop 判定欧氏零漂（input 现测全保）。
13. press_map：Press 后 damage 含 visual 界 + bg 已换（同帧断言）；Release 还原；
    bg=None 还原 = 移除。
14. TextMetrics 自定义（cell ×2）→ text 布局随之变；icon dims 未注册 → (0,0) +
    空绘不炸。
15. SwipePager：attach 建树 + 翻页三判据（速度/位移/回弹）+ 橡皮筋 + 圆点高亮 +
    page_count 域 1..=8 loud + 与宿主树近帽（MAX_NODES 余量断言）。
16. a11y 三钉：remove 后无悬挂 / reset_arena 清表 / 33 条 NodeFull loud。
17. S7 双态组织：= 5/6/7 的 cfg(feature="s7") 双面编写形态。

**回归零漂移面**：layout 七测（含测 6 measured==3）/ damage 15 / tree 8 /
reconcile 10（:200/:203 绊线保，:202 改钉）/ input 10（slop 欧氏零漂）/
frame 15（FrameRec 加字段仅访问无整构相等——编译安全）/ offscreen 6（重钉）/
anim 9 中 **footprint 测 w 205→100 重钉单列**（D17 连坐——正确性 wart 修复）/
resample 8 / dep 8。
**审计口径注记（三镜 R3-4）**：重钉恰三处之外，D25 Msg 扩域**编译强制连坐** =
input.rs `slop_boundary_eight_stays_nine_drags`/`cancel_terminates_without_up`
两条 DragEnd 断言补 vx/vy 载荷字段——结构变体字面量必须携全字段（旧断言文本
物理不可保留）、补值为加强性（顺带钉速度环钳位/清环语义）、slop 判别值 8/9
原样未动——**不计入重钉**，后续刀零漂审计对表单列。

**新增预估**：mask 10-12 / metrics 3-5 / icon 6-8 / grid 8-10 / axis 8-12 /
swipe 10-15 / a11y 5-8 / press 6-8 / s7 2-4 ≈ **+60~80 测**（berry-ui ~96 →
~160~175）。

## 12. 实施编排（workflow 模式，~9 agents）

```
段A 冻结面基座（主线，1 agent）：容量常量 + 注释五处 + const 断言族 +
     TREE_DEPTH_MAX/DESC_DEPTH_MAX 帽 + relayout/paint 签名扩展（tm/icons/cs）+
     D28 供给槽 + CrossAlign/Constraints + D31 set_offset + D17 clip helper 三点
     收敛（footprint 测重钉单列提交）+ 钉板 11.1 组
段B TLV/Props 扩面（主线，串行于 A）：kind 6/7/8 + flags 三位 + 合法域表 +
     wire 块 + 10 静默缝 + 绊线改钉 + AstError 两变体 + DescBuilder 块 API +
     GOLDEN 双轨（V1 钉字面量 + v2 产流钉）+ 钉板 3/9 的 TLV 半
段C 布局语义（主线，串行于 B）：十字轴（含内在型叶豁免）+ HAS_SIZE 定尺寸 +
     Grid measure/place + Image/Icon 叶尺寸与 paint 叶臂 + offscreen 重钉 +
     钉板 9/14 + golden 零漂核（A 面全量）
段D 交互三件（主线，串行于 C）：referee v2（速度环/锁轴/DragEnd 扩域）+
     Easing/AnimKind + press_map（走现状 set_props）+ SwipePager +
     钉板 10/11/12/13/15/16
段E S7 spike（feature 门，串行于 D）：mask 三漏斗（B 面分派 + D3 三态 + anim
     staging）+ D4 截断消费 + FrameRec.damage_adds + bench_s7 + 钉板 5/6/7/17
     双态 + host 全量双 feature 绿自证
── 实施三镜对抗审（3 agents 并行，段 E 毕）：R1 规划对表（§3 逐行 + §1 范围 +
     ADR #60 六项兑现面 + 本草案 v2 合折承诺兑现）/ R2 正确性（位面映射完整性、
     TLV 块序与合法域、Grid 算术、定点溢出、栈帽、no_std 零 alloc、A 面逐字保持
     核证）/ R3 测试充分性（钉板真伪、mutation 针、golden 双轨独立性）
── 修复回写（1 agent）：findings 修/驳逐条有据
主会话管：test-host 全量（默认 + --features s7 两态）+ no_std build 核 +
     smoke 零扰动确认 + 提交 + docs 回写
```

同文件面（lib.rs/desc.rs/layout.rs）五段**串行**不可并行；测试随段内自携。
（草案轮三镜审已先行完成——§15；段 E 后的实施三镜审对象 = 代码非草案，非双重。）

## 13. 验证与提交

1. `just test-host` 全量绿 ×2 态：默认（A 面）+ `cargo test -p berry-ui --features
   s7`（B 面）——写 `~/.cargo-target` 前按三方钉板协议通告 dd/6c 兄弟会话并避让
   QEMU/构建窗。
2. `cargo build -p berry-ui --target aarch64-unknown-none`——no_std 零漂（feature
   s7 是 host-only cfg，不进 target 面）。
3. smoke 零扰动：berry-ui 未进 guest 镜像——`git status` 不见 kernel/ 兄弟 WIP；
   smoke 标记清单零变化（抽 `just build-user` 链核零触碰）。
4. 原子提交（pathspec = `user/berry-ui` + 本草案 + docs 回写四件）：

```
feat(gui): GUI-6——布局 v1 精修（Grid/定尺寸/十字轴/Image·Icon 叶/TextMetrics 缝/
容量帽×2/树深帽）+ 交互三件（referee v2 速度锁轴/QuadOut·CubicOut/press_map/
SwipePager）+ S7 属性级生死门 spike（ADR #60 兑现）

via [HAPI](https://hapi.run)

Co-Authored-By: HAPI <noreply@hapi.run>
Co-Authored-By: Claude Code <noreply@anthropic.com>
```

5. docs 回写：`docs/索引.md` 五节加行 + `docs/GUI里程碑规划-2026-10-v1.0.md` 执行结果
   + 立项注记 5 条回填（D20）+ `docs/阶段沿革.md` 末尾追记。

## 14. 挂账与分界

- **注记 5 条漏抄回填**（D20，随刀入规划——**已回填 2026-11-03**，落规划「GUI-6 落地注记」节）：① S7 kill 线 <20%；
  ② 动画帧处置二选一已写死 = anim 改产 mask 走同一分派（D1——B 面）；③ mctx.changed
  截断与 mask 三漏斗同随 feature s7 门（段 E，不进段 A 基座）；④ GUI-6 与 GUI-2
  段③ 互斥（段③ 已收官 2026-11-02，本刀开工无冲突——回填时注记已解除）；⑤ Node
  侧语义位预留（a11y/press 走 Ui 侧表，Node 19 字段冻结不动——未来若入 Node 须新
  ADR）。
- **clip 语义挂账**：SwipePager 页框 = viewport 全宽（v1 免 clip）；子内容越界
  裁剪 = v2 候选（layout.rs v2 刀内议）。
- **paint_dirty 消费**：v1 保位不消费；S7 转正后若属性级 paint 截断需要它，
  v2 再议（零读者现状 = 无行为依赖）。
- **案 1 layout.rs v2**：v1 收官后独立立项（ADR #60 已排）——Constraints 非
  Viewport 档届时才接实现（Infinite 档已立位）。
- **ADR 面**：本刀零新 ADR（#60 已立法全覆盖）；刀内裁决以本文 §3 为准（v2 已过
  草案三镜审）；实施三镜审若出架构级变更 → 升 ADR #62 再动。
- **GUI-2 增件⑧ 依赖**：TextMetrics 缝已留（D8），GUI-2 段④（真字体面）届时迁
  paint 侧——两刀互斥不并行（多会话纪律）。

## 15. 三镜审查合折表（2026-11-03，29 findings → 22 裁决组）

| 裁决 | findings（镜#） | 处置 |
|---|---|---|
| A | D3 变长换文丢 relayout（R2 blocker / R3 blocker / R1 minor#5） | **采纳**：D3 长度感知三态（§3/§5/§11 术语定义） |
| B | S7 A/B 与段A 矛盾、kill 后处置未定义、钦定钉板缺席（R1 major#1 / R3 major#5 / R1 major#2 / R2 note / R3 note） | **采纳扩展**：mask 三漏斗 + D4 截断**整体** cfg(feature="s7")（忠于 SDK-02 §6.2 全口径）；A 面 = 现状逐字；kill → feature 出 CI 零残留；钦定钉板入 §11-6 |
| C | IconSource/TextMetrics 供给链、门面级联、DescBuilder API（R2 major#2/#3 / R3 major#3） | **采纳**：D28 供给槽制（pub 门面零级联）+ relayout/paint 内部进参 + DescBuilder 块 API 入 §6 |
| D | Text 十字轴豁免（R2 major#4） | **采纳**：§4.1 矩阵加内在型叶行；D23 注明保 golden 限定面 |
| E | TLV 合法域表 + DescBad 不存在（R2 major#5 / R1 minor#6） | **采纳**：§8.2 两表 + D21/D30 AstError 两新变体 + 侧表满 NodeFull 立法 |
| F | 递归未迭代化、512 深链穿栈（R2 major#6） | **采纳**：D29 TREE_DEPTH_MAX=96 + §9 递归行；删「已迭代化」错误断言 |
| G | PadProps.all 无位（R2 major#7） | **采纳**：GAP 位兼任 + §5 全映射表 |
| H | D17 连坐 anim footprint 测（R3 major#2） | **采纳**：§11 回归面点名重钉单列；D23 同步 |
| I | a11y 生命周期（R3 major#4） | **采纳**：D12 四条 + §11-16 三钉 |
| J | 速度环 Δt=0/清环/入环点 + resample 钉板（R2 minor#10 / R3 note#9） | **采纳**：D15 三条 + §11-11 |
| K | 锁轴改 slop 语义（R2 minor#11） | **改判优于原案**：slop 保持欧氏（零回归），锁轴 = 触发时主轴——input 10 测零漂 |
| L | 钉板 compile-red/runtime-red 不分（R3 minor#6） | **采纳**：§11 分栏 + 头部术语定义 |
| M | golden v2 字面量（R3 minor#7） | **采纳**：§8.4 写死同 V1 法 |
| N | SwipePager attach/页域/set_offset（R3 minor#8 / R2 note#13） | **采纳**：D16 attach+SWIPE_PAGES_MAX=8 + D31 set_offset 入 §6 |
| O | Constraints Infinite 档（R1 minor#3） | **采纳**：四变体（Viewport = v1 根形态） |
| P | launcher 树规模立项义务（R1 minor#4） | **采纳**：§9 落 SDK-02 §4.7 推算 240–360 |
| Q | 栈数字虚报 + 锚点互换 + 注释清单（R2 minor#8/#9 / R3 note#10） | **采纳**：§9 表改 10240/20480；§2.3 改 TLV 常量/parse_node/encode_props；注释五处入 §2.1 |
| R | Ui 侧成员尺寸账 + offscreen 常量翻倍（R2 note#15 / R3 note#11） | **采纳**：§9 补行 + D10 注记 |
| S | 钉板 2 守护对象（R3 note#12） | **采纳**：const 路径形态 Ui<OVERFLOW_NODES> + 明示守护对象 |
| T | Grid gap 出处 + DrawImage 锚点措辞（R1 note#7/#8） | **采纳**：§1.1① 注明刀内补件；§4.3 改 berry-gfx cmd.rs:326 |
| U | R3 正向核实底账 | 入档：96 计数/十静默缝/DESC_DEPTH_MAX=16/五段依赖/九族预估全对表成立——草案 v2 的旁证基座 |
| V | 实施三镜 vs 草案三镜双审（R3 正向核实内问） | 维持：对象不同（代码 vs 草案），非双重浪费 |

## 16. 实施合折（2026-11-03 收官——24 裁定 + 18 findings + 两事故 + 测试账）

### 16.0 终验五门（主会话，隔离 target dir 零共享写零通告）

- berry-ui 默认面 `cargo test -p berry-ui`：**163 passed / 0 failed**。
- berry-ui s7 面 `--features s7`：**166 passed / 0 failed**（差 3 = bench_s7 纯 cfg(s7) 单测）。
- no_std 双面构建过：`cargo build -p berry-ui --target aarch64-unknown-none` 默认 + s7（仅 2 条既有重复警告，非本刀引入）。
- `just test-host` 全量：**961 passed / 0 failed**。
- docs_guard 复验过（追加本节后复跑）。

### 16.1 实施裁定 24 条（主会话全数预接受——本文即事实源，逐条落码形态）

| # | 裁定 | 落点/依据 |
|---|---|---|
| 1 | Text×HAS_SIZE 改拒（不接受定尺寸——保 Node 144B 冻结） | §8.2 拒面表 |
| 2 | Rect×HAS_SIZE 容读折入：块胜 + 归一（w/h 归一进 props），builder 拒 | desc.rs |
| 3 | 规范形收紧：SIZE 双 None 拒、ALIGN 载 Stretch 拒 | reconcile 规范形 |
| 4 | kind×块失配 → AstError::UnknownProp | parse |
| 5 | CrossAlign pub(crate)→pub（pub props 携 pub cross 字段，私有入公 = E0446） | §4/§6 双处 |
| 6 | DescBuilder 块 API `&mut self` + `Result<(), AstError>` 三签名 | §6 |
| 7 | a11y 侧表最小集提前落段B（原排段E） | 段B |
| 8 | IconSource provided `is_empty()` 缺省 false + 两档分流 | §6 |
| 9 | set_press_map 返 `Result<(), UiError>`（帽 16 超满 NodeFull loud） | §6 |
| 10 | SwipePager 四增补：new panic 承载 1..=8 / tick(ui,now_ns) 惰性起拍 / page_node(i) 填充口 / on_msg 消费 Cancel | §6/D16 |
| 11 | apply_change GEOM 路不调 mark_ancestors_dirty（与 §11-6 顶层 measured==0 互斥；几何上行由 D4 changed 返值承担） | §5 伪码 |
| 12 | props_diff 补 Col/Row/Grid w/h 行（漏列则显式定尺寸变产空 mask，布局永不更新） | §5 表 |
| 13 | FrameRec.relayout_ns 恒 0（v1 无 relayout 计时面） | telemetry |
| 14 | DESC_DEPTH_MAX 保持 usize（不硬缩 u8） | desc.rs |
| 15 | wire 路深帽收编（R2-1）：build_node 加 depth 参数执法 TREE_DEPTH_MAX=96，AstError::TooSmall 承载——96 合法/97 拒/300 深攻击钉板 | §3 D29 |
| 16 | AnimProp::Gap×Grid = no-op 完成语义（立法非扩臂，扩臂归 v2） | §5 anim 契约 |
| 17 | anim staging 满帽 debug_assert（结构性不可达——STAGING_MAX 由 CAP 帽先行拒） | anim.rs |
| 18 | props_diff 落 src/mask.rs pub fn（三漏斗单源） | mask.rs |
| 19 | props_geom_equal cfg_attr(s7, allow(dead_code))（A 面唯一消费者消失） | mask.rs |
| 20 | press_map/a11y 清空挂 reset_arena 单汇点（remove 子树/apply_desc 重建两路共清） | ui 侧 |
| 21 | §11-9 Grid 钉板取 Pad(all=0) 包定值 rect 载体 | tests/layout |
| 22 | offscreen Grid 轨高 48→16 笔误修正 | tests/offscreen |
| 23 | D25 扩域连坐：input.rs DragEnd 补 vx/vy 不计重钉 | tests/input |
| 24 | R2-9 MeasCtx.changed [bool;2048]：A 面 +2KB 记账不门控（cfg 分叉面减半换不动基座） | §9 |

### 16.2 实施三镜 18 findings 处置（修复段两趟合并账：16 修 2 驳）

- 2 blocker 定案 = **R3 mutation 针污染伪影**（非代码缺陷，取证见 16.3①——终态基线复验全绿 + D1–D31 静态干净）。
- R2-1 wire 深帽 major：实修先红后绿 → 裁定 #15。
- R3-1 CAP_PROBE 绝对值钉 `assert_eq!(CAP_PROBE, 512)`；R3-3 kill_verdict_print 改 env 基线判定文案 + bench_churn_large_tree 103 节点×1024 帧大树 churn（脱离冒烟量级）。
- 余 15 条注释/文档/钉板级——修/驳逐条有据（台账随修复段报告归档，不赘列）；doc 侧 6 条归主会话（本节 + §3/§5/§6 各注）。

### 16.3 两编排事故存档（取证定案，未损树——编排方法论依据）

1. **mutation 针 × 只读镜并行互污**：三镜 prompt 只读总则与 R3 mutation 针自相矛盾 → R3 改共享工作区三文件恰落 R1 审查窗，R1 两 blocker 实为针污染伪影（08:06:26 后内容再基线全绿）。教训：**mutation 针与只读镜并行必互污——须工作副本或串行**。
2. **修复段上下文压缩自撞**：单 transcript 压缩前趟已落 16 条修复、压缩后趟失忆误判「外部写入者」（E0428 自重复块，删自块自解）；journal 实证无第三方无重派，两趟成果归并一份台账。教训：压缩续趟先读 journal/台账再下「外部写入」结论。

### 16.4 测试账

- berry-ui：基线 96 → 段A 103 → 段B 115 → 段C 129 → 段D 154 → 段E 160 → 修复段 +3 → **终态 163（默认面）/ 166（s7 面）**——+67/+70，预估带 +60~80 命中。
- 全仓：`just test-host` 961/0（GUI-5 收官时 762 → 本刀后 961，含兄弟线并轨面）。
- S7 方向性读数（非判定面）：PAINT churn A 面 3 重测/帧 → B 面 0/帧；布局轴墙钟 311µs→35.9µs（/64 帧单机参考）；kill criteria ≥20% 正式判定迁 GUI-8 embedder 双打点（§10）。
