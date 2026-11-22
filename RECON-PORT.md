# RECON-PORT 波 2 UI 移植对账（ui-wave2 → 主树）

> 移植对账员产出 ｜ 2026-11-15 ｜ 主树现状 = 本 worktree（`wip/ui-wave2-port` @ 3272bbdeb「41 ViewKind 全部通电」）；移植源 = `../ui-wave2` @ 37c02abbc（只读）。路径约定：主树 `user/...`、源 `../ui-wave2/user/...`，行号为当前工作树实读行号。
> 结论先行：三路实施文件面两两不相交；主树 34 脏文件与移植面零交集；TLV 与存量 golden 面 add-only 成立；依赖序 = 路 1（paint 面）→ 路 2（pattern.rs）→ 路 3（画廊）。

## 一、圆角原语（Cmd::FillRectRounded + raster 行 LUT）

主树现状：全直角近似——Chip 直角胶囊底（`user/berry-ui/src/paint.rs:909-914` 注释自认「v1 无圆角原语——fill_rect 近似纪律」，臂实现在 :943）；Card 直角 + corner_radius 数据携载未消费（`user/berry-ui/src/paint.rs:912-914` 注释、臂 :978）；Toggle 胶囊近似 `capsule_h`（`user/berry-ui/src/paint.rs:1290-1307`，端帽内缩 h/4 宽 h/3，调用点 :691）；ListItem 半径常量已携载（`user/berry-ui/src/pattern.rs:38` `LISTITEM_RADIUS = 24px`）无消费面；gfx 面只有直角 `Cmd::FillRect`（`user/berry-gfx/src/cmd.rs:216`）。

源实现：`Cmd::FillRectRounded` 枚举变体（`../ui-wave2/user/berry-gfx/src/cmd.rs:254-260`，追加于 Present 之后）+ encoder `fill_rect_rounded`（同文件 :393-402，radius==0 与 FillRect 逐像素等价）；raster 执行臂 = 行内缩 LUT（`../ui-wave2/user/berry-gfx/src/raster.rs:28` `RADIUS_LUT_MAX=255`、:34-44 `radius_row_inset` 单源几何、:122-173 `fill_rect_rounded`——半径三重钳 min(w,h)/2、一次预计算两角复用、直角快速路不另起 LUT）；回放测试 `../ui-wave2/user/berry-gfx/src/queue.rs:546-573`；ui 消费面 = 填充发射共核（`../ui-wave2/user/berry-ui/src/paint.rs:353-364`，radius>0 走 rounded）与 Rect/状态底（同文件 :835-866）。

add-only 核实（两面均成立）：存量 `_cmd` golden 零改动——两侧 `user/berry-gfx/tests/golden.rs` 224 行 `diff -q` 逐字节同文；TLV 流零变更——desc TLV 面 Card corner_radius u32 LE 主树已有（`user/berry-ui/src/desc.rs:111`、:702 字段、:1795-1798 编码、:2368-2372 解码），圆角是 gfx 命令内部变体，不新增 wire 字段；Cmd 枚举新变体追加尾部不重排。

移植形态建议：整件照搬 gfx 三文件增量（cmd 变体 + raster LUT + queue 回放与测试），主树 paint 升级点按序——Toggle `capsule_h` → `FillRectRounded(r=h/2)` 真胶囊（近似最失真件，最先升）；Chip 直角 → r=24px（8vp）；Card 直角 → 消费 `p.corner_radius`（数据已在 TLV 流里跑）；ListItem r=24px（pattern 配方落地后随之）；Button 可选 r=24px（源 button_style 半径 8vp）。不升级 = 弧族（Progress Circular、Check 勾选符、Radio 环）——那是弧/环原语面（波 3+），非本原语射程。

## 二、disabled 态（五态三档）

主树现状：零实现——`user/berry-ui/src/lib.rs` 无 set_style/set_disabled/NodeState（grep 计 0）；已有资产 = `PressedVis` 查表（`user/berry-ui/src/paint.rs:131-147`，slots [u32;32] 快照 press_map 活动槽），唯一消费点 Button（`user/berry-ui/src/paint.rs:669` `pressed.contains(slot)` 换 `ACCENT_PRESSED` 暗色底，:164 const 先例）；pattern.rs 有纯数据 `ItemState` 五态枚举含 Disabled 灰底 0xF2F2F2（`user/berry-ui/src/pattern.rs:51-70`），无任何 paint 消费。

源实现：`NodeState` 六值（`../ui-wave2/user/berry-ui/src/lib.rs:296-319`——Normal/Pressed/Disabled 三态验收面 + Hover/Focused/Selected 占位）；`StyleParams` 四字段（同文件 :355-364——press_bg/disabled_bg 预混色 + PressScale 档 + radius_vp；色彩预计算铁律 = 宿主预混落槽、引擎零运行时混合）；per-slot `StyleSlot` 侧表（:371）；API 面 set_style（:1922）/set_disabled（:1942）/node_state（:1976）；消息机 Disabled 锁死（:2021-2027）；hit 侧拒（`../ui-wave2/user/berry-ui/src/hit.rs:309`）；paint 查表 `state_color`/`style_of`（`../ui-wave2/user/berry-ui/src/paint.rs:276-289`）；预混单源 `premix`/`oh_press_bg`（`../ui-wave2/user/berry-ui/src/theme.rs:32`、:45）。

移植形态建议：不整套换源 API（StyleParams 四字段 + 六值状态机对主树是过度面），走「扩展主树查表加 disabled 档」——`PressedVis` 扩为三态槽表（或并列 `DisabledVis`），lib.rs 加最小 `set_disabled`（写侧表 + Msg 机锁死 + hit 拒，三处照源逻辑搬），色值按主树纪律硬预混 const（`ACCENT_PRESSED` 先例，不必引 premix 族）。先吃 disabled 的 kind：Button/Toggle/Check/Radio/Seg（离散交互件，查表换一格底色即成，五件同批）；Slider 次之（轨道 + 滑块两处换色）；输入族 Input/Edit/Select 挂后批。

## 三、clip 双面（溢出裁剪）

主树现状：paint 面零 clip——`user/berry-ui/src/paint.rs` 的 clip 命中全为 `#[allow(clippy::too_many_arguments)]` 误报（:346、:483）；damage.rs 的 clip 是脏区折 viewport 语义（`user/berry-ui/src/damage.rs:16`、:97、:236）非绘制裁剪；Scroll 现状 = layout 子纵向上移 offset_y（`user/berry-ui/src/layout.rs:931-936`，注释自认「paint 期 clip 由双面 clip [补]」）+ paint 纯容器无窗（`user/berry-ui/src/paint.rs:455-456`、:561-562）；List 行数 = 子节点数，无溢出控制。即溢出处理不是行数控制——是「不处理」，滚动内容直接画出视口侵入后续区。

源实现：三面结构——paint 面 per-slot 开关 + 走查有效窗（`../ui-wave2/user/berry-ui/src/paint.rs:73` `Sides.clips`、:214-221 `enc.push_clip/pop_clip`、:474-477 `eff = damage ∩ 祖先 clip 界`、:512-541 段标记与子树整剪、:593-598 段闭标记）；结构面侧表（`../ui-wave2/user/berry-ui/src/lib.rs:490` `clip_on`、:1226-1233 Scroll/List 登记即开、:2099-2116 `set_clip`/`is_clip`、:2254/:2473 Sides 传递）；回放面裁剪栈（`../ui-wave2/user/berry-gfx/src/queue.rs:20-22` `CLIP_STACK_MAX`、:117-138——超帽 push 折 no-op 保守，多画不少画）+ `Cmd::PushClip` 变体（cmd.rs 枚举尾部）。关键佐证：源 damage.rs 与主树 522 行逐字节同文（`diff -q` 无差异）——clip 全落在 paint/lib/gfx 三处，damage.rs 零改。

移植形态建议：移植价值高（现状溢出即视觉缺陷非缺省设计）；最小面 = gfx {cmd.rs 加 PushClip/PopClip + queue.rs 裁剪栈} + ui {paint.rs eff 窗与段标记 + lib.rs clip_on 侧表与 set_clip}，Scroll/List 登记即开一处照搬；damage.rs 不动；`damage ∩ 祖先 clip` 折窗逻辑随 paint 走查单点实现。

## 四、pattern 扩容（197 行 → 23 配方族）

主树现状：`user/berry-ui/src/pattern.rs` 197 行 = 几何常量（:26-44，DENSITY/TOPBAR_H/BOTTOMTABS_H/LISTITEM_H/LISTITEM_RADIUS/PAD_X/NAV_DEPTH_MAX）+ `ItemState` 五态色表（:51-70）+ `NavStack` 纯数据页栈（:82-143）+ 单测；零组合配方函数——TopBar/BottomTabs/ListItem 只有常量、Nav 只有数据面。

源实现：`../ui-wave2/user/berry-ui/src/pattern.rs` 3649 行、90 个 pub fn；头注（:1-30）自述移植映射 = 刀 3 资产 23 配方，其中七件（Collapse/Chip/Fab/Card/Gauge/Qr/DragReorder，tag 34..=40）改薄包装直建 kind 树，其余 16 照移植，外加 Sheet/Drawer/Refresh/DatePick/TimePick 增量件；结构 = `Palette`（:63）+ `FiveState` 五态默认值（:105）+ 七张 style 表（:148-192）+ 结构三件 spacer/divider/divider_v（:299-334）+ 各配方（结构体 + 建树 fn + 状态机方法）。

主树缺的配方清单（19 件，按源画廊 pat 格族对齐）：TopBar（源 :416）、AppBar（:362）、Scaffold（:468）、NavHost 页栈状态机（NavStack 升级，:508 起）、BottomTabs（:1011）、TabBar（:715）、Seg（:874）、Step（:933）、ListItem+Card（:1234/:1194）、Collapse（:1299）、SwipeItem（:1364）、DragReorder（:1595）、Chip+Badge（:1802/:1834）、Fab+IconButton（:1884/:1902）、Gauge（:1929）、Qr（:1860 qr_show）、Sheet（:2344）、Drawer（:2497）、Refresh（:2197）。

kind/props 覆盖核实：源 pattern 的 desc use 面（CardProps..TimePickProps 全族）在主树 41 kind desc 全在——主树 desc @3272bbdeb 为源 @cfcf65284 的超集（41 kind 全通电）；swipe/anim 符号（SwipePager/FLIP_VEL_PX_S/SNAP_DUR_NS/Easing）主树 swipe.rs/anim.rs 均有（grep 9/7 命中）。

适配点两条：①theme API——源 `Palette::from_tokens`/`FiveState::interactive` 调 `theme::{token,premix,oh_press_bg}`（源 theme.rs:116/:32/:45），主树 theme.rs 128 行只有 const 色 + `Tokens::pick`，改用主树 API（const 直取 + 硬预混值，`ACCENT_PRESSED` 先例）或顺带在路 1 给 theme.rs 补最小 premix；②add 容器白名单——主树白名单（`user/berry-ui/src/lib.rs:925-939`）已含 Stack/Tabs/Scroll/List/Collapse/Card，比源移植时（只认 Root/Col/Row/Pad/Grid，源头注「落地约束」）更宽，源配方「0×0 占位锚」技巧可简化但照搬亦兼容。

## 五、画廊（WAVE2-GALLERY）

主树现状：无——`user/berry-ui/tests/` 27 文件无 gallery；worktree 根无 WAVE2-GALLERY。

源实现：测试面 `../ui-wave2/user/berry-ui/tests/gallery2.rs` 979 行（tests/ 内、src 外——照源先例）；资产面 `../ui-wave2/WAVE2-GALLERY/` 五个 tracked 文件：README.md（联络表）、cells.tsv（46 格 FNV-1a 64 位像素哈希单源）、gallery.png、tools/make_gallery.py（拼图）、tools/probe_cells.py（像素级色证据探针 46/46）；ppm/ 为未跟踪中间产物；golden 双轨同文件内联（`gallery2_dump_cells` dump + `gallery2_golden_hashes` 哈希钉板，渲染回归即红，视觉变更首跑回填）；通道 = `Ui::paint` → `CommandEncoder` → `Queue::submit` → RGBx → PPM，guest 面零改动。

移植形态建议：移植为 `user/berry-ui/tests/gallery2.rs` + 本 worktree 根 `WAVE2-GALLERY/`（tools 两脚本 + README + cells.tsv 照搬）；格子集适配主树现实——主树 41 kind 全通电（3272bbdeb），源画廊「裸 kind 锚无 paint 臂」七件格（Chip/Fab/Gauge/Qr/Card/Collapse/SwipeItem/DragReorder）在主树可显真视觉、格子可更全；哈希钉板必须首跑回填（主树 v1 直角 vs 源圆角渲染必然不同，GOLDEN 表不可照抄）；依赖序 = 路 1（clip + 状态档，kind_scroll/kind_button_states 格需要）与路 2（19 件 pat 格需要）先落，画廊殿后。

## 六、冲突面预判（三路文件面 + 主树脏文件核验）

三路实施文件面（两两不相交，已核验）：

- 路 1 paint 面：`user/berry-gfx/src/{cmd,queue,raster}.rs` + `user/berry-ui/src/{paint,lib}.rs`（+ 可选 `user/berry-ui/src/theme.rs` 加预混 const、`user/berry-gfx/tests/` 增量新测试文件）。不碰 pattern.rs；damage.rs 与源同文故零改。
- 路 2 pattern 面：`user/berry-ui/src/pattern.rs` 单文件。
- 路 3 画廊面：`user/berry-ui/tests/gallery2.rs` + `WAVE2-GALLERY/**`。

交集结验：pattern.rs 仅属路 2；paint/lib/damage/gfx 属路 1；tests 与画廊目录属路 3——两两交集为空。编译依赖序（非文件冲突）：路 2 适配后用主树现状 theme API 即无依赖可先行；路 3 依赖路 1 + 路 2 落地。

主树 34 脏文件（他会话，`git status --porcelain` 于主 worktree，33 改文件 + user/target/ 未跟踪目录）逐个核验：全部分布于 `.agents/{notes,plans,research}/`（14 件）、`docs/`（1）、`justfile`（1）、`kernel/src/{main,user}.rs`（2）、`tools/berry/{src,web,packaging,README}`（13 件）、`user/bins/svcmgr/`（2 件）、`user/target/`（1）——与三路移植面（`user/berry-ui/`、`user/berry-gfx/`、`user/berry-ui/tests/`、`WAVE2-GALLERY/`）交集为空，无一在移植面。另：移植工作落本 worktree（分支 `wip/ui-wave2-port`），与主树脏面天然隔离；合回时按显式路径提交（并行会话纪律）。

## 附：两侧体量差速览

| 文件 | 主树 | 源 | 差额归属 |
|---|---|---|---|
| berry-ui/src/pattern.rs | 197 | 3649 | 路 2 主缺口 |
| berry-ui/src/paint.rs | 1459 | 1951 | 路 1（圆角/状态/clip 消费面） |
| berry-ui/src/lib.rs | 2525 | 2710 | 路 1（StyleSlot/clip_on/状态机） |
| berry-ui/src/layout.rs | 989 | 1099 | 源含按压仿射等（按需另核） |
| berry-ui/src/hit.rs | 456 | 518 | 路 1（Disabled 拒） |
| berry-ui/src/theme.rs | 128 | 130 | premix/oh_press_bg/token 三 fn |
| berry-ui/src/damage.rs | 522 | 522 | 逐字节同文——零改 |
| berry-gfx/src/cmd.rs | 493 | 570 | 路 1（FillRectRounded/PushClip） |
| berry-gfx/src/queue.rs | 531 | 670 | 路 1（裁剪栈） |
| berry-gfx/src/raster.rs | 491 | 587 | 路 1（行 LUT） |
| berry-gfx/tests/golden.rs | 224 | 224 | 逐字节同文——add-only 佐证 |
| berry-ui/tests/ | 27 文件 | +10（clip/gallery2/multiline/pressmap/render2a-e/style/theme）−7（widgets1-7） | 路 3 取 gallery2.rs；render2a-e/style/clip 属路 1 测试面可选 |
