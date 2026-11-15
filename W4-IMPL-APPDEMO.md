# W4-IMPL-APPDEMO —— 波 4 APPDEMO 控件画廊实施报告

基线：worktree `wip/ui-wave4` HEAD 769515a9a（共同依据 = 本目录 [W4-RECON.md](W4-RECON.md) + `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.8，行号以 RECON 现场直数为准）。任务书：appdemo 42 行秒退自验件升格为控件画廊常驻演示——pattern 全件 + 核心 kind 分区展览页（每件一格：控件实体 + 名称 Text）、Scroll 翻览、格子清单口径对齐 `user/berry-ui/tests/gallery2.rs`。**全部落地**：`cargo check --manifest-path user/bins/appdemo/Cargo.toml --target aarch64-unknown-linux-musl`（含 `--tests`、`--release`）零 error 零 warning；宿主对拍检查器（建树/节点预算/六路手势交互/三滚动位离屏渲染）全过（2026-11-15 现场，见 §四）。未触 launcher/guifbd/gui-wmd/svcmgr/berry-* 基座/justfile 与他批实施件（`user/bins/{files,contacts,photos,camera,messages,browser}/`、`W4-IMPL-{FILES,CONTACTS,PHOTOS,CAMERA,MMS,BROWSER}.md`、`W4-RECON.md` 在册未碰——本批足迹仅 `user/bins/appdemo/` 五件 + 本报告，`git status` 佐证）。

## 一、改动清单

| 件 | 行数/尺寸 | 说明 |
|---|---|---|
| `user/bins/appdemo/src/main.rs` | 755 行（原 42 行重写） | 帧循环 + 输入接线（settings 食谱 + 画廊四增量）；`app APPDEMO up` / `app APPDEMO fs ok 23` 两自证行原序保留（argv0 断言同面） |
| `user/bins/appdemo/src/gallery.rs` | 1028 行（新增） | 建树面纯模块（berry-ui/berry-gfx 零 libc/sys 依赖——宿主对拍可跑，见 §四）；glyph 供件 + 两分区 49 格 |
| `user/bins/appdemo/Cargo.toml` | 修改 | deps 从 berry-libc 单件扩为 settings 五件套（libc/sys/guiproto/ui/gfx——任务书「按 Settings 先例建 crate」裁定） |
| `user/bins/appdemo/manifest.toml` | 修改 | 增 `[runtime] stack_pages = 64`（真 GUI 链 8 页穿先例 ×8）；`[ui] label` APPDEMO→**Gallery**（设计稿 §2.8 明令；gui8 LAYOUT.JSN cells 断言按 app id 不受扰）；console=true / fs / [compat] 三值现状不动（设计稿 §2.8「现状不动」） |
| `user/bins/appdemo/Cargo.lock` | 自动 | cargo check 产物随 deps 扩容刷新 |

资产面零增量：`icons/icon.png`（108×108 PNG，mkapp 编码 .byi 源）与 `views/main.bui` 占位件原样在席——desktop 图标与打包面不动。justfile :301-312 APPDEMO 段本就以本 crate manifest/ELF/icons/views staging（与七 stub 段不同）——接线批零改码即可产新 .bap（§四）。

## 二、v1 实现形态

### 2.1 页面树与格子清单（口径对齐 gallery2）

```text
窗（全幅单面呈现——settings/launcher 同形，surface_id=0）
└─ Stack[滚动层, 铬层]（铬层后画 = 滚动溢出遮盖双保险）
   ├─ Scroll（kind 自带 clip——挂路 1 已合；内容 Col cross=Center）
   │  ├─ 顶让位 spacer（TOPBAR_H）
   │  ├─ [分区头] pattern.rs recipes
   │  │  └─ 26 格：spacer/divider/divider_v/stack/app_bar/top_bar/
   │  │     scaffold/NavHost/tab_bar/seg/step/bottom_tabs/card/
   │  │     list_item/Collapse/SwipeItem/DragReorder/chip/badge/
   │  │     qr_show/fab/icon_button/gauge/Sheet/Drawer/Refresh
   │  ├─ [分区头] ViewKind widgets
   │  │  └─ 23 格：Button/Button states/Toggle/Slider/Progress/Tabs/
   │  │     Seg(kind 直载)/Sbar/List/Check/Radio/Select/Input/Edit/
   │  │     Canvas/Tip/DatePick/TimePick/Image/Icon/Grid/Text/Rect
   │  └─ 尾留白锚（content_h 实测基准）
   └─ 铬层：pattern::app_bar(title=Gallery, 无返回)
```

- **pattern 节 = pattern.rs 全部可渲染面位 26 格**：刀 3 资产 23 配方经主树移植（dd53bc039）= 20 pub fn 配方 + 7 attach 型器件 = 27 公开面位（W4-RECON §二），其中 NavStack（纯数据页栈）无独立视觉面（由 NavHost 格承载其消费形态）、NavItem（入参件）由 bottom_tabs/scaffold 格入参承载——26 格全覆盖，**波 4 首个 pattern 真实消费场**（此前唯一消费 = pattern.rs 自带单测，W4-RECON §1.4）。
- **kind 节 = gallery2 CELLS 39 格 kind_\* 全集映射**：14 件被 pattern 配方收编（divider/spacer/stack/step/badge/collapse/chip/fab/card/gauge/qr/dragreorder/refresh/swipeitem）、Scroll = 翻览容器本体、layout（Root/Col/Row/Pad）= 全页结构锚、余 23 件独立成格（含 Seg kind 直载格——pattern::seg 配方走 Grid 组合、引擎 Seg kind 另有 widget_react 切段面，两形分立各占一格）。Pad/Spacer 两结构 kind 无 v1 视觉面（Spacer flex 挂布局刀；gallery2 kind_spacer 格同为 0×0 示意），如实注记不入格。gallery2 的 4 件 pattern 近似格（彼时配方未移植只能手拼值面）由真配方格取代。
- **每件一格 = 控件实体 + 名称 Text**（S24x48 主墨名牌 + 关键格 S16x32 次墨说明字——v1 诚实边界注记 5 处）；格间 pattern::divider 分隔（Divider kind 直载——零手拼）；Scroll 拖滚翻览（app 自管 offset——contacts 同形）。

### 2.2 交互（画廊 = 波 2 件的活体验收场）

- **引擎面零接线**（widget_react/press_react 同拍）：Toggle/Check/Radio/Select/Collapse/Chip 点击翻转、Slider 拖动跟值、Tabs/Seg(kind)/Step 击点切段步进、Input/Edit 点击置焦、Button/Chip/Fab on_tap；按压视觉 = press_map 登记 10 枚 Button + paint::PressedVis 查表（**按压单源**）。
- **app 侧四件接线**（main.rs Msg/事件双面）：①页拖滚（Y 拖未被器件认领域 → scroll_by，clamp 非负不越尾）；②Seg/TabBar 配方换档（Click node → `hit()` → `set_active`——pattern Cell hit 面首个真实消费）；③Sheet/Drawer 按钮开合（on_tap 事件 → `set_open` 起吸附 + 帧循环 `tick` 推进）；④SwipeItem 横拖（`on_msg` 自判行界跟手 + 吸附开）/ DragReorder 纵拖（`on_msg` 自判容器界换槽——宿主对拍实测 [1,2,0] 换位）/ Refresh 纵拖（`on_msg` 越阈触发 + 1.5s 驻留 `settle` 回弹——Toast 2s 驻留同拍量级）。
- **常驻不退出**（对秒退自验件的根本变更）：wm 缺席 = `appdemo: wm absent (no gui gift)` + park；fail = eprintln + park（崩溃监督归 svcmgr app_supervise 域）；首帧回执 `appdemo: first frame`（console=true 串口可见，非标记形态）。

### 2.3 零手拼执法与配色单源

卡片/条目 = `pattern::card` / `pattern::list_item`（Card kind 自绘卡面/阴影/8vp 圆角）；骨架 = pattern 配方（app_bar/stack/spacer/divider/seg/step/tab_bar/bottom_tabs/scaffold）；全页配色 = `theme::Tokens::pick(Light)` 组引用 + `theme::BG_PAGE` 清屏单源（零清屏色值字面量）；glyph = 程序化 const 位图（pattern::icons 约定域 7 件 + Image 演示缩略图 1 件，ink = theme token 打包——设计稿 §3.3「不新增位图资产」；contacts crate 同款先例）；文案全 ASCII。

### 2.4 v1 无 clip 诚实设计（gallery2 现态注记的对策）

Scroll/List kind 已内嵌 clip（挂路 1 已合），但 Stack/Col 无——Sheet 闭合面板下移 / Drawer 闭合面板左移 / Refresh 指示器上藏的「逃逸区」以**格内预留空带（spacer 巷道）**承载：面板态位在相邻空白带内滑动不越格（宿主对拍断言闭合态几何：sheet oy=300 恰落巷道、drawer ox=−336 恰落巷道、swipe 动作层 ≥视口右缘）；SwipeItem 行宽 = 格宽 + 右页留白通抵视口右缘（配方 width 语义「v1 取 vp_w」的格内对齐版——闭合态动作层零露缝，edge-to-edge 行 = OH 列表习语）。

## 三、v1 形态 vs 终态差异（3b 依赖降解清单）

未落底座（W4-RECON §四 全景）与本应用逐项降解：

| # | 3b/appshell 依赖（未落） | 本应用 v1 降解 | 终态形态（落齐后） |
|---|---|---|---|
| 1 | **berry-appshell crate**（rpc/pump/scene/icons/conf 五模块） | settings 同款手拼 RPC 半程 + 自管帧循环（§2.2） | appshell::rpc/pump 收编，帧循环退化为例行骨架 |
| 2 | **wm 礼物槽位无专用访问器**（本 app 带 fs 词 → gift 布局 [rootfs, fs, wm, input]，wm 落 extras[2]） | 同槽访问器 `berry_libc::io::wm_present_h()`（io.rs「同一 gift 位对不同程序承载不同对象」多产面惯例；W4-IMPL-FILES.md §三.2 先例） | 裁决 A（`ui=["wm","input"]` 能力词 + appshell crt0 位序立法）落地换轨专用访问器，本行一处替换 |
| 3 | **appshell Toast/Dialog/Menu Overlay**（设计稿 §2.8 Demo 页三演示位） | 三演示位降级为应用内器件近似：Toast → Refresh 驻留行/settle 节拍、Dialog → Sheet、Menu → Drawer（Stack 叠层语 + 逃逸巷道） | appshell v1 窗内 Overlay 原生演示位 |
| 4 | **wm back 路由**（encode_wm_back_req 零消费） | 画廊单页滚动形态无页栈——back 消费面天然缺席（NavHost 格静态级联展示，推拉动画归真应用消费面如 contacts 详情页） | 设计稿 §2.8 全形：每件点击推控件详情页（单 kind 全态陈列）+ Nav pop back 链 |
| 5 | **softime/软键盘**（OP_INPUT_INJECT 无消费者） | Input/Edit 格只展值面（已填文本 + 焦点回显——engine 点击置焦在席） | softimed 敲键回显，画廊成为输入件的活演示场 |
| 6 | theme 样式族（card_style/oh_press_bg/OH_PRESS_SCALE——主树未落，W4-RECON §二末段裁定「用 ItemState 面」） | 等价面：Card kind 自绘 + press_map/PressedVis 查表 + `pattern::ItemState` 五态预混表（配方内建） | 样式族落地后按压/缩放随迁单源 |
| 7 | IconSource 真供件 = D28 接线刀（pattern::icons 约定域字形无系统级注册面） | crate 内程序化 const 位图 8 件（§2.3）——域外 id 空绘不炸 | appshell::icons 共享字形注册（含 TopBar 尾随动作位 icon_button 承载面） |
| 8 | **set_disabled 挂路 1**（paint PressedVis disabled 档数据面先行、无引擎写面） | Button states 格 D 档渲染同 N——gallery2 kind_button_states 同一诚实注记（说明字 `D (path-1 pending)`） | 路径 1 合入后 D 档灰面点亮，画廊零改码 |
| 9 | 常驻 park 形与 boot 批调和（裁决 B 未落——W4-RECON §1.7 铁律） | **本批只落 crate 不入镜像**（justfile APPDEMO 段产 .bap 的现状未动；§四交接） | 裁决 B（boot 批 ui deferred）先行后入包：`app <id> gui deferred` 诊断行 + APP_SPAWN_LIVE_MAX=4 预算内占 1 活槽 |

另：Scroll 拖滚无惯性/fling（跟手步进即停——v1 引擎无速度外推面）；画廊无 NavStack 消费（单页形态——NavHost 器件在席可直升，contacts 先例）。

## 四、验证与交接

- **编译门**：`cargo check --manifest-path user/bins/appdemo/Cargo.toml --target aarch64-unknown-linux-musl` → Finished 零 error 零 warning；`--tests` / `--release` 同过；berry-ld RUSTFLAGS 形 `cargo build --release` 过（ELF **504,592 B** ≈ 0.48 MiB——.bap 估 ≈ 0.51 MiB（settings 比 ELF→.bap +56KB 同律）< 稿估 0.90 MiB 帽、BAP_MAX 4MiB 远未触；现 APPDEMO.bap 385,024 B → Δ ≈ +0.14 MiB，rootfs 总量帽 128MiB 余量宽裕（W4-RECON §5.3））。
- **宿主对拍检查器**（一次性验证件 /tmp/w4check——`#[path]` 挂载 gallery.rs 整建树，bin 本体链 berry-libc aarch64 asm 宿主不可跑故分文件）：①build_all 全树零 Err（NodeFull/文本域/值域全过）+ 节点余量探针实测 **475/512 用量（余 37）**；②content_h=12036px（1080×1920 视口 ×6.3 翻览量）；③六路真指针手势（referee 真走）：页拖滚判轴/seg 点击换档（hit→set_active active=1）/SwipeItem 跟手+吸附开/DragReorder 换槽 [1,2,0]/Sheet 开合 300ms 滑入滑回/Refresh 越阈 fire+settle；④三滚动位（顶/中/尾）离屏渲染——色数 7-12、非底色 5.9-24.9%、divider 可见行 4-9（分隔线宽度 ≥900px 断言——修掉 cross=Center 下 Divider 塌宽坑）；⑤器件闭合态几何断言（逃逸巷道语义，§2.4）。检查器与 PPM 落 /tmp 不入仓。
- **接线批（整合批）待办**：justfile :301-312 APPDEMO 段本就消费本 crate——`just build-user` 重跑即产新 .bap（manifest label=Gallery 随 staging 自然入包；rootfs-manifest.toml 零改——条目名 `app/APPDEMO.bap` 不变）；**裁决 B 先行**——常驻 park 形入镜像即挂死 boot（W4-RECON §1.7 铁律：`APP ok` 全线 FAIL）；gui8-smoke 判读随动：`app APPDEMO up` 恰 1 行断言保留（本 crate 启动序两自证行先于 wm 门，boot 批 deferred 形态下时点移位但无断言面扰动）、`app APPDEMO fs ok 23` 无断言面照打。
- **运行时诊断面**（console=true 串口 + eprintln 族）：`app APPDEMO up` / `app APPDEMO fs ok 23`（既定断言面，原序保留）/ `appdemo: wm v=… input=…`（起握手）/ `appdemo: first frame`（首帧回执）/ `appdemo: <what>`（fail+park 收口）。
