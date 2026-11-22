# PORT-XB 波 2 pattern 扩容改动清单（19 配方族 ui-wave2 → 主树）

> pattern 扩容者 XB 产出 ｜ 2026-11-15 ｜ 落地面 = 本 worktree（`wip/ui-wave2-port`）`user/berry-ui/src/pattern.rs` 单文件（197 → 3022 行，52 单测全绿）；对账依据 = [RECON-PORT.md](./RECON-PORT.md) 第 4 项；移植源 = `../ui-wave2/user/berry-ui/src/pattern.rs`（3649 行，只读）。
> 自查：`cargo test --manifest-path user/Cargo.toml -p berry-ui --lib --target aarch64-apple-darwin` 两环境全绿——①干净 HEAD 临时 worktree（3272bbdeb + 仅本文件，55/55，隔离并行在途改动）②共享 worktree 现势（含路 1/路 3 在途，61/61）；`cargo clippy` 本机工具链未装（1.97.0 无 clippy 组件），挂后续。

## 一、改动面

- `user/berry-ui/src/pattern.rs`：唯一改动文件（测试内联同文件——`--lib` 面自含）；既有 4 件 API 原样保留——几何常量族（`DENSITY`/`TOPBAR_H`/`BOTTOMTABS_H`/`LISTITEM_H`/`LISTITEM_RADIUS`/`LISTITEM_PAD_X`/`NAV_DEPTH_MAX`）、`ItemState` 五态表、`NavStack` 纯数据页栈（含三存量测试）一字未动。
- 19 配方族（RECON 第 4 项清单逐件落齐）＋随件结构工具（spacer/divider/divider_v/stack）＋ mount 小工具（vspec/text_spec/icon_spec/a11y，pub(crate)）＋ `icons` id 约定域＋尺寸常数族。

## 二、19 配方清单（源行号 → 主树形态）

| 配方 | 源行 | 主树形态与适配 |
|---|---|---|
| AppBar | :362 | 照移植；`theme::OH_APPBAR_MINI_H_PX`→`TOPBAR_H`、标题起始 48px 新 const `APPBAR_TITLE_INSET` |
| TopBar | :416 | 照移植；动作位帽/方寸/缝 const 族随件 |
| Scaffold | :468 | 照移植（title/bottom_items 双 Option 槽） |
| NavHost | :508 | 源 `NavStack` 状态机改名（主树纯数据 `NavStack` 名占用）；深帽 4→`NAV_DEPTH_MAX`=8（主树页深帽单源） |
| TabBar | :715 | 照移植（Grid 等分轨 + Stack 格 + restyle_cell） |
| BottomTabs | :1011 | 照移植；`OH_TAB_BAR_H_PX`→`BOTTOMTABS_H` |
| Seg | :874 | 照移植；`SEG_H`→`SEG_BAR_H`=96px（避与 lib `SEG_H`=36px 撞名） |
| Step | :933 | 照移植（勾 Icon/accent Rect/连线段拼） |
| Card | :1194 | 主树 Card kind 容器直用（面底+阴影=kind 自绘）；h=`LISTITEM_H`、radius=`LISTITEM_RADIUS` |
| ListItem | :1234 | Card 薄别名（elevation 0 无图标）；hpad 源 24px(8vp)→`LISTITEM_PAD_X` 48px(16vp)（主树既有律，偏差见 §四） |
| Collapse | :1299 | 主树 Collapse kind 容器直用：自带 header 带+箭头、收起三面守卫——源「title+chevron 组合头 + 宿主手动挂卸 body」退役，`set_open` 走 set_props（见 §三-3） |
| SwipeItem | :1364 | 状态机照移植；frame=Stack + kind 占位锚（见 §三-4）；reveal 同步走 set_props |
| DragReorder | :1595 | 状态机照移植；每行「Stack 行格（行 spec + 24×36 手柄叶）」——手柄 meas 由行格 max 吸收不扰行高；totals 收口走 set_props |
| Chip | :1802 | kind 直载（主树自绘胶囊+选中色变、内在 = 标签×8+16 ×32）；选中翻转 = 宿主 set_props |
| Badge | :1834 | 主树 Badge kind 直载（count=0 隐藏、>99 封顶、红底白字）——源「Pad+Text 衬底」组合退役，返回 `NodeId`（非 Option） |
| Fab | :1884 | kind 直载（八角圆 ACCENT 底+'+'+扩展标签；空标签 56 圆 / 扩展 56+8+标签×8） |
| IconButton | :1902 | Fab kind 小档同源（空扩展标签）；源五态小档档位随 ItemState 机制改写（§三-2） |
| Gauge | :1929 | kind 直载（内在 64×64；Ring/Bar/Arc 三档） |
| Qr | :1860 | kind 直载（点阵 = module×4 px 方阵） |
| Sheet | :2344 | 照移植；面板底面经内层 Stack 满幅叠放（主树 Col 子高=内在，Rect 直挂折 0——见 §三-5） |
| Drawer | :2497 | 照移植；同 Sheet 底面形态；内容锚 + 面板内容锚双 exposed |
| Refresh | :2197 | 状态机照移植；frame=Stack + kind 占位锚（§三-4）；触发/完结走 set_props（源重挂退役）；配方指示 Icon 保留（跟手层）+ kind 带（驻留层）双层各司其职 |

不移植（不在 19 清单）：Tabs 联动控制器（:768，SwipePager 联动面）、Sbar（:1112）、Check/Radio/progress_circular、DatePick/TimePick、Palette/FiveState/七张 style 表（机制退役，§三-2）。

## 三、适配四律（主树 API 面）

1. **主题面**：入参 `&Palette` → `&crate::theme::Tokens`（`Tokens::pick` 主树单点取值）；字段映射 bg→`bg_page`、accent→`brand`、text_primary→`font_primary`、text_secondary→`font_secondary`、bg_secondary→`bg_comp`；`accent_dim` = 本面 `const fn dim_accent`（brand 上页面底 12% 预混、整数千分比——派生随入参 token，light/dark 各得其值；premix 原语归属 theme 面，路 1 落地后收编）。`ON_ACCENT` 本地 const（theme OnPrimary 档挂收编）。
2. **五态默认 = 主树查表 `ItemState`**：源 `FiveState`/`StyleParams`/`Ui::set_style` 落表机制主树未立——交互件产物（TabBar/Seg/BottomTabs/Card/Collapse/SwipeItem/DragReorder/Chip/Fab/IconButton/Gauge）携 `pub state: ItemState`（默认 `Normal`，消费面 `state.bg()` 直取换底）；`ItemState` 已含 `Disabled` 档 = 扩展后查表形态，**编译绿、无需 cfg 门**。XA paint 侧消费面（set_disabled/DisabledVis、theme `BG_DISABLED`/`FG_DISABLED` token——本 worktree 在途已见 add-only 落地）与 `ItemState::Disabled.bg()`=0xF2F2F2 同值，pattern 面无引擎调用、数据面先行。
3. **set_props 全 kind 开口 → 源「重挂同步」退役**：Collapse.set_open / Refresh 触发与完结 / SwipeItem.reveal 同步 / DragReorder.sync_totals / Chip 选中翻转均走 `set_props` 原位更新——节点 id 稳定、子树保持（测试断言 id 不变；源同名测试断言「重挂换 id」就此反转）。
4. **`add` 父位白名单约束**：主树白名单（Root/Col/Row/Pad/Grid/Stack/Tabs/Scroll/List/Collapse/Card）不含 SwipeItem/Refresh 等组件 kind（`_` 兜底 BadParent）——源「Stack 帧 + kind 占位锚」技巧**保留**（RECON §四预判「可简化」经实测不成立，占位锚满幅铺底经 Stack `_` 臂随父填充，kind 自绘面照常全幅）；Card/Collapse 已在白名单，直用为容器。
5. **布局语义差**：主树 Col/Row 子主轴尺寸 = 内在（非父分配）——Sheet/Drawer 面板底面、SwipeItem 动作层底面 Rect 直挂会折 0，经「定尺寸 wrapper + 内层 Stack 满幅叠放」承载（面板 Col(w,h) > inner Stack > [底面 Rect(定高 meas), 内容锚]）；Stack 内在 = 子 max（源 Pad Σ）——Refresh 帧内在 = 内容高（指示藏上缘外不抬内容高，源 Σ 行为就此收正）。

## 四、与源的偏差记录（逐条）

- ListItem/Card 水平内边：源 `OH_CARD_HPAD_PX`=24px(8vp) → 主树 `LISTITEM_PAD_X`=48px(16vp)（主树 ListItem 既有常量单源；内容锚起点断言 48/120 随之）。
- Collapse API：源 `collapse(ui, parent, palette, title, open, on_toggle)` 五参 → 主树 `collapse(ui, parent, expanded, on_toggle)` 四参（title/chevron 视觉 = kind header 带自绘；title 面挂 kind props 递进）。
- Badge API：源 `badge(ui, anchor, palette, n) -> Option<NodeId>`（n=0 不建树）→ 主树恒建 `badge(ui, parent, count) -> NodeId`（count=0 = kind 0×0 空绘隐藏）；叠放定位归宿主（源 Pad 内衬 12px 语义不随件）。
- 值域执法去重：源配方内联先验（Card elevation/Qr 版本域/Gauge permille 等）全部移除——主树 add 面 `widget_value_ok` 单源 loud 拒（同 `NodeBad` 族；测试断言 loud 不变）。
- 深帽/命名：NavHost 深帽 4→8（`NAV_DEPTH_MAX` 单源）；`SEG_H`→`SEG_BAR_H`；icons 域删 CHEVRON_DOWN/UP、CHECK_BLANK（Collapse chevron 语义随 kind 化消失）。
- swipe/anim 依赖：`FLIP_VEL_PX_S`/`SNAP_DUR_NS`/`anim::interp`/`Easing` 主树 swipe.rs/anim.rs 直用（RECON 预判成立）；SwipePager 联动（Tabs 控制器）不在清单未引入。

## 五、并行会话交互记录

- 本 worktree 同时承载路 1（paint/圆角/disabled——XA）与路 3（gallery2/画廊）在途改动；XB 面自查期间路 1 的 paint.rs 测试有一次瞬时编译红（`render` 元组不匹配，非 XB 面），故增补干净 HEAD 临时 worktree 隔离自查（55/55）——随后路 1 修复，共享 worktree 现 61/61 全绿（含 XA 的 `xa_*`/`disabled_tokens_*`/`radius_token_px_pinned` 新测试与本文件 52 测试共存）。
- 值交集核验：XA theme 在途新增 `OH_RADIUS_PX`=24 与本文件 `LISTITEM_RADIUS`=8×`DENSITY` 同值（其 `radius_token_px_pinned` 测试经 `crate::pattern::DENSITY` 对拍）；`BG_DISABLED`=0xF2F2F2 与 `ItemState::Disabled.bg()` 同值——两面同值不耦合（XB 不引用在途未提交 API，合流先后皆绿）。
- 文件面纪律：XB 仅改 `user/berry-ui/src/pattern.rs`（路 2 单文件面），未碰 paint/lib/theme/gfx/tests 任何在途文件；提交时按显式路径 `git commit -- user/berry-ui/src/pattern.rs PORT-XB.md`。
