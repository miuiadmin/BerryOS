# W3A-IMPL-SC —— 波3 件⑤ TabBar 三页实施清单（SC）

Date: 2026-11-15
Owner: SC（TabBar 三页 + 设置页数据面）
依据: 波3 设计稿《2026-11-15-波3桌面三栏与系统窗实施设计》（plans/ 在册）§1「TabBar 三系统页」条 + §7 验收面「TabBar 三页切换/设置页真改 boot 模式默认」

> 注一：任务书指名的 `W3A-RECON.md` 在本工作树不存在（开工时全树检索无果）——按 SA 同境先例（W3A-IMPL-SA.md 注一）以设计稿 §1/§7 + 现场直核替代。
> 注二（并行会话事故）：本批实施两度遭并行会话现场抹除（SA 件⑦ 抽取的 desktop.rs 重写覆写 + 波4 会话「工作树净」清树删除未提交新文件）；第三轮按 SB/SA 先例打法——detached 工作树（/tmp/sc-verify @ b1646469d）先行编译与冒烟验证，回主树重套后**即时显式路径提交**（5211d5df3）。后续会话请勿 checkout/清理 `user/bins/berry-launcher` 路径。

## 交付物（commit 5211d5df3，全在 user/bins/berry-launcher）

### ① 结构：content 四段化 + 「覆盖即隐藏」（desktop.rs）

- content 列自「状态栏 + 桌面」改四段：状态栏（scene 整条，SA 件⑦ 面，零改动）→ **pages Stack 区**（host_h = vp_h − 状态栏 − grid_top − 条高 − 五键高（紧凑再扣 8px 底垫））→ **BottomTabs 条**（宿主 Stack 恒定流槽）→ 五键导航条。
- 桌面页 = 原 SwipePager 四页 + filler + Dock **原样内嵌**（只换父位 content→pages Stack 内 desktop_page；pager 状态机/网格/dock 建树面零改动）。
- sys 页（应用列表/设置）按需建入 pages Stack 顶层 = Stack（**不透明底 Rect 整面盖住桌面** + 内容列）；离树 = `remove` 整页。裁决依据：berry-ui v1 无 clip/隐藏原语（paint 对子树无条件发射），前稿件⑤b「几何归零即不绘」前提不成立——详见笔记 [2026-11-15-波3TabBar三页装配面](.agents/notes/implemented/2026-11-15-波3TabBar三页装配面.md)。
- 几何让位：big 档 filler 149→5（dock 上移 144px——OH 45vp 底保留区与条区共享）+ 五键条高 190→135（= OH sysUI 45vp×3 原 owner 加高回调；键面 160px 不变）→ big 整列恰收口 1920（原溢出 55px 一并修复）；紧凑档网格 −48px（ff0395eb2 已落，本批条体真正入列——原稿条挂列尾在 600px 高溢出屏外不可见不可点）。
- `switch_sys_page`：离树（remove 旧 sys 页）→ 入树（build_apps/build_settings）→ 换条（rebuild_tab_bar）→ 重挂（rehang_view）。Err 非致命 eprintln（桌面常驻义务优先）。

### ② BottomTabs（syspages.rs `build_tab_bar`）

- big 档：`berry_ui::pattern::bottom_tabs` 冻结名直用（48vp@3x=144px 条 + 24vp 图标 + brand 染色 + 2vp 顶指示条）。
- 紧凑档（800×600 @1x）：同构镜像（48px 条 + 24px 图标 + S8x16 标签 + 2px 指示条）——pattern 常量全 @3x 单源，紧凑镜像按 desktop.rs 既有双档纪律本地立值（诚实边界：pattern 无密度参数）。
- 换选 = 条实例整条重建（pattern 选中态建树期烘焙、无运行期改笔面；宿主 Stack 流槽恒定 → 几何零漂）。三页位 a11y `ROLE_BUTTON`（"home tab"/"apps tab"/"settings tab"）随重建重挂。

### ③ 应用列表页（List+ListItem+Search）

- 数据 = `icons::registry()`（boot 期 APP_LIST 拉取的 svcmgr 注册表真流，零第二数据源）。
- 结构：Stack 页 > [不透明底, 内容列[标题 / Search 行（Input 件 placeholder "search apps" + "fixed filters" 注记） / 固定过滤词表行 / List 容器（`ViewKind::List` item_count×item_height 声明面）> Col > N×Card 行]]。行 = Card（白底 ListItem normal 态，LISTITEM_H 144/48 双档）> Row > Pad(16vp) > label Text。
- **Search 诚实降级（3b 暂缓面）**：输入链（softimed 升起 + TextEvent 回显 + 返回键收链）挂 3b；v1 = 固定过滤词表 ALL / A-F / G-M / N-Z（ASCII id 首字母四分桶，纯函数 `filter_matches` 带测试）——点词换底换行（`set_filter`），不装帽位留白。
- 行点击 → 与桌面格同一 `spawn_by_id_inner` 路（含 recents 记账；Card/Row/Pad/Text 四节点命中面皆归行 `row_index_of`）。
- 行数按页高预算裁（紧凑 5 行 / big 7 行，帽 8）——List 无滚动 v1（Scroll 手势接线挂账），容量外条目走过滤词收窄。

### ④ 设置页（分组 Toggle/Radio/Seg/Slider）

- Boot 组：Toggle "Verbose boot"——**真落 BOOT.JSN**（事件回写 `save_boot`，canonical `{"boot":"splash"}` 17B / `{"boot":"verbose"}` 18B；初稿笔误 16B/17B，波3a 验收修正——字节字面与 Fat12Volume 对拍为准；canonical 子串精确匹配解析 fail-closed）。
- Display 组：Radio×2 theme light/dark（引擎 widget_react 置位 + 本面 `enforce_theme_pair` 清位互斥）+ Seg 2 段 anim（Release 击点分段引擎已接）。
- Sound 组：Slider volume 0..=1000 permille（DragStart/DragMove 行程跟手引擎已接；纯点击 v1 = +100 wrap 步进 `step_slider`，如实注记）。
- About 组静态行。真值消费面（换肤/帧率档/混音）挂后续波——本波立「真存真读」。
- 事件面：frameloop `ui.take_events()` 消费 `WidgetEvent`（on_change id EV_BOOT/EV_THEME_*/EV_ANIM/EV_VOL）→ `SysFrame` 真值镜像 + 落盘。

### ⑤ 设置页数据面（settings_io.rs，新增）

- **I/O 走 `appshell::conf`**（`ConfKey`/`read_key`/`write_key`——SA 件⑦ f016aa99a 落地后即时切换，mandate 首选项兑现；本波先内联后切换的双份现场已清，仅 schema 面留 launcher）。
- SETTINGS.JSN canonical 单形：`{"anim":1,"theme":0,"volume":500}`（键序字典序、无空白）；解析容忍键序/未知键/缺键/坏值（独立扫描 `scan_u32`，缺键保默认 500/1/0）。
- 读语义：缺席 = 默认零打印；坏件 = eprintln + 默认（**不重写自愈**——下次 save 全量覆写即愈，不造额外写流量）；冻结相 `fs_h()==0` 零 RPC 直返。
- 宿主消费面（波1 合入方）：berry CLI `--boot-mode` 缺省读数据盘 BOOT.JSN 默认——FAT12 读回复用 tools/gui8-smoke.py `Fat12Volume.read_file` 先例（审查建议 c）；canonical 字节以本清单与 settings_io 测试为对拍锚点。
- 单测 5 件（musl 目标编译验证；宿主无 qemu-user 不可执行——layout_io 同注）：boot canonical 字节/boot 解析 fail-closed/settings canonical 字节/回读容忍面/近似键不误吞。

### ⑥ press_map 翻页重登记 + a11y 预算重立法（desktop.rs `rehang_view`/`rehang_press`）

- **press_map（帽 28，侦察 :31 挂账清偿）**：`set_press_map` 整表替换，三拍共用单源（桌面翻页 / sys 切页 / 建树尾）。桌面页 = 当前页 20 格 (cell→label)（波 2 批 A 建树尾登记形原样——trigger = 格容器 Pad，Icon/Text 直击不换底 = 波 2 原面）；应用列表页 = 有主行 (pad/text→label) 双触发面 + 过滤词自映（≤20 条）；设置页 = 控件→行标签 5 条（Toggle/Slider/Seg 不在 visual 白名单——反馈落标签 Text 换底）。
- **a11y（帽 32）**：底座 16 = 状态栏 4 + dock 4 + 五键 5 + tab 三页位 3；桌面页集 = 页容器 1 + **有主格 ≤15**（`A11Y_CELL_CAP`——占位格 label 空无 spawn 面不占槽；满注册表 20 格超帽行主序确定性截断，诚实边界：截断格无 a11y 条目）；sys 页集 = List 容器 + 有主行（ROLE_LIST_ITEM）/控件五件，随可见真相挂/摘。**修潜在 NodeFull**：五键化后旧账面实为 34>32（boot 期 `rehang_a11y` 满 20 格页必炸——此前未暴露因冒烟死于更早故障）。

### ⑦ 返回键/五键路由（frameloop.rs）

- back：应用在焦先收应用（件④ 原路）→ **sys 页在树先回桌面 tab（R1）** → 桌面翻页（同拍一义）。
- home：sys 页先回桌面 tab → 再落 page 0。
- tab 命中路由在五键之先（件⑤b 原位）；sys 页在树时 `pager.on_msg` 不喂（隐藏 pager 不吃手势）。

## 验证

- **编译**：launcher musl release + `cargo test --no-run` 全绿（本批零新告警；appshell 3 告警 = SA 件⑦ 既有）。
- **gui8-smoke（既有断言零回归）**：基线（b1646469d 干净）与改动版（detached 工作树同基 + 本批）各跑一轮对比——改动版标记 = `LAUNCH v=1`×2 + `LAYOUTW ok`×1 + `LAYOUT ok n=2`×1（基线仅 `LAUNCH v=1`×1 即死于 svcmgr 竞态 panic），**基线产出的标记改动版一个不丢、且进度更远（严格超集）**；`LAUNCH ok`/`PAGE ok n=2`/`CLICK ok n=1` 两版同缺——被**先在故障**阻断（见下），非本批回归。本批代码在两版中均未及执行（崩溃点在 `UiMain::new` 堆分配，先于 desktop::build），标记等价性由崩溃点先于全部新代码结构性保证。
- **先在故障（非本批范围，证据在案）**：① launcher 堆耗尽——`memory allocation of 87432 bytes failed`（UiMain 分配）于 icons APP_LIST 全量解码后 abort，波4 七真应用图标集入册后 `icons` 像素常驻帽（单条 ≤225k px ≈ 879KB）击穿 launcher 静态堆预算（该面现归 appshell::icons，SA 件⑦ 域）；② svcmgr APP_LIST 流期竞态 panic（`G1.5: kill user task 'user_svcmgr'`，全日间歇复现）。两者任一均阻断 `LAUNCH ok` 链——**建议 icons 常驻帽/堆预算复测先行归账**（波4 批量真图标后无人复测），本批 TabBar 交互链（切页/过滤/设置落盘）待其解除后经 VNC 三连截图验收（设计 §7 原句）。

## 诚实边界与挂账

- BottomTabs 紧凑档为 pattern @1x 镜像（pattern 无密度参数——双档纪律本地立值）；换选走整条重建（pattern 无运行期改笔面）。
- a11y 满注册表截断（>15 有主格的页，第 16+ 格无 a11y 条目）；press_map Icon/Text 直击不换底（波 2 原面）。
- Search 输入链（softimed/焦点环键入/返回键收链）= 3b 暂缓面，v1 固定过滤词表如实降级；List 无滚动（Scroll 手势挂账），行数按页高预算裁。
- Slider 纯点击 = 步进（拖动已接引擎行程映射）；设置项真值消费面（换肤/帧率/混音）挂后续波。
- BOOT.JSN 只影响下次 reboot 的宿主默认（splash/verbose 为 boot 早期链，本 boot 相不可逆）；宿主 berry CLI 消费面归波1 合入方，canonical 字节已钉板。
- SA 合入后切换已兑现一档：持久键 I/O 已走 `appshell::conf`；sys 页建树糖（scene 组合面）待 appshell 扩运行期 API 后迁（现 launcher 内联——mandate 裁决）。
