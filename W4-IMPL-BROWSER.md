# W4-IMPL-BROWSER —— 波 4 BROWSER 应用实施报告（第四件）

基线：worktree `wip/ui-wave4` HEAD 769515a9a（共同依据 = 本目录 [W4-RECON.md](W4-RECON.md) + `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.7，行号以 RECON 现场直数为准）。任务书四件：书签卡片网格（Card+icon）→ 页面视图（TopBar 含 Back+刷新、地址行、程序化模拟内容+Progress 模拟加载）+ 返回书签；历史 v1 单级；BOOKS.JSN（稿 §2.7）。**全部落地**：`cargo check --release --tests --target aarch64-unknown-linux-musl`（berry-ld RUSTFLAGS 形）过、零警告零错误；`cargo build --release` 过（ELF 464,280 B）；宿主纯逻辑对拍 + glyph ASCII 目检 ALL OK（2026-11-15 现场）。未触 launcher/guifbd/gui-wmd/svcmgr/berry-* 基座/justfile 与他批实施件（`user/bins/{files,contacts,photos,camera,messages}/`、`W4-IMPL-{FILES,CONTACTS,PHOTOS,MMS}.md` 在册未碰——`git status` 佐证，本批足迹仅 `user/bins/browser/` + 本报告）。

## 一、改动清单（全部新增，无既有文件修改）

| 件 | 行数/尺寸 | 说明 |
|---|---|---|
| `user/bins/browser/Cargo.toml` | 45 行 | settings/photos 镜像（空 [workspace] escape user 根、字面 version=0.2.0、deps 五件 = berry-libc/sys/guiproto/ui/gfx、release z/lto/cgu1/abort/strip；三偏差裁定注记在册） |
| `user/bins/browser/manifest.toml` | 23 行 | id=BROWSER / entry=bin/browser / label=Browser / console=false / **fs = ["rootfs"]**（BOOKS.JSN 持久化授权词——photos/fsclient 批同形）/ stack_pages=64（settings 8 页穿先例 ×8）/ [security] 三键 / [ui] 三键 |
| `user/bins/browser/src/main.rs` | 1909 行 | 应用本体（下节形态）；内联纯函数单测 6 件（JSN 编解码/域门、站点表自洽/形状、文案装配、glyph 表） |
| `user/bins/browser/icons/icon.png` | 9,045 B | 自 `bins/appstub/apps/BROWSER/icons/icon.png` 拷入（mkapp 编码 .byi 的 PNG 源——「源不入包」管道，files/photos 同款） |
| `user/bins/browser/views/main.bui` | 52 B | 占位 view（stub 原样拷入） |
| `user/bins/browser/Cargo.lock` | 99 行 | cargo check 生成入册（settings/photos 同款先例） |

crate 内资产 = BROWSER 资产新单源（任务书「manifest.toml + src/main.rs + icons」口径）；`bins/appstub/apps/BROWSER/` 旧副本留待接线批随 appstub 七应用循环整体退役（justfile :328-337 现仍 cp appstub——本批不碰 justfile，W4-RECON §5.4）。

## 二、v1 实现形态（对照设计稿 §2.7 页面树 + 任务书）

- **书签页（根，NavHost depth 0 常驻底页）**：`pattern::top_bar`(Browser) + 上次地址行（`Last: <addr>`，BOOKS.JSN 回显，空到访 = 空文隐藏）+ **两列书签卡片网格**（Grid kind 等分轨，track = (vp_w−2·16vp−8vp)/2；六站恰三行一屏纳）。卡 = `pattern::card` 配方（elevation 0、Card kind 自绘面 + LISTITEM_RADIUS 圆角单源）+ **globe 前导图标**（Icon kind 24vp，`IconSource` 程序化线框 glyph，brand 色）+ 站名（S24x48）/地址（S16x32）双行——「Card+icon」两件全在席。窄视口防御：track < 336px 藏图标防溢出。
- **卡按压反馈**：files 批行装配同形叠层习语——卡 Stack[card 本体, 按压面层 Rect, 内容行（置顶层）]；按压色单源 = `pattern::ItemState` 五态预混表（Normal = 卡面白常态隐形 / Pressed = brand 20% 预混白 0xCCE3FF——pattern 单源值，app 零字面量）；Press 置 / Release·Cancel·DragStart 还原（拖路径无 Release 的补还原面）；命中 = Release 坐标对卡 Stack `ui.bounds` 几何包含判定（Click 无坐标面；slop 外拖拽自然豁免误触）。
- **页页（push，NavHost 300ms 右滑入；z 顶覆盖）**：Stack[不透明页底 Rect(bg_page), 内容 Col] → ① **顶栏 = `pattern::app_bar`(站名, back=true)**（返回 glyph = IconSource 程序化「<」chevron，Click 锚 → NavHost pop 回书签）**+ 尾随刷新 glyph**（REFRESH 程序化圆箭头，bar 行尾补挂——「TopBar 含 Back+刷新」的 pattern 组合承载：top_bar 配方无返回位、app_bar 无动作位，两键经 app_bar+补挂 Icon 拼齐，Click 锚 → 重载当前页）；② **地址行** = Input(kind Text) + Go/Stop Button——Input 值面经 placeholder 承载（内容不进 props——输入焦点协议 Phase 2 未接线，弱化灰显 = 诚实只读观感），随导航 set_props 换址；Go/Stop = 单按钮 label 换档（加载中 = Stop 即点定格揭页 / 已载 = Go 重载，contacts Star 按钮同形 set_props 换文；引擎 press_map 1 枚 = Button 按压视觉）；③ **进度行** = Progress kind（Linear，`value` permille 0..=1000 set_props 步进）+ 百分文案（"Loading n%" → "Done"）——稿 §2.7 写「Sbar 进度条」、任务书写「Progress」：两 kind 均在册，取 Progress（Linear 语义即加载进度；Sbar 是滚动条 thumb 语义面）；④ **页体**（程序化模拟内容）= 页标题（S32x48）+ 段落行（S16x32 次级色）+ **链接行**（brand 色 + "> " 前缀，Click 锚）+ 底部诚实标注 `simulated page - no net`。六站 × 两页（home + 一子页）const 表种子，行文全 ASCII ≤ 32B。
- **模拟加载**：Loading→Loaded 两态——时间基 permille（elapsed/1.2ms，1.2s 到满，帧率无关）；Loading 相帧循环逐拍 `ui.wake` 保 Run（vsync 拍点醒，每拍 set_props/set_text 前进）、满格 → `finish_load`（页体子树整 retire 重挂 + Go 档换文 + "Done"）；刷新 glyph / Go 均可 0 重爬；Stop 立即定格揭页（任务书「Stop 立即定格揭页」）。
- **历史 v1 单级**：页内链接点按**就地换页**（同栈位换 site/page + 地址行换文 + 重启加载，不叠 NavStack）——back 恒回书签；多级历史链/Forward 挂终态。同页加载中点按幂等忽略。
- **BOOKS.JSN 持久化**：`{"schema":1,"last":N}` 恰 21B 单簇（N = 上次到访站序 0..=5，9 = 无）——launcher layout_io 食谱自实现（appshell::conf 缺）：三约束纪律（缺席回默认 + 坏读自愈回写 + 写失败非致命 eprintln 常驻不死）+ 严格串行单 RPC 会话 + close 即 commit（TRUNC 游标两段式）+ `fs_h()==0` 结构性早退零 RPC。开页/链接导航即存；书签集 v1 = 内置静态种子（无编辑面，「书签集真存」降解为集序回显，见 §三 #10）。
- **GUI 通道形态**（svcmgr GUI 授权链 5452649af）：本 app 带 fs 词 + [ui] 节 → gift 槽位 `[rootfs, fs, wm, input]`——wm 落 extras[2]，经同槽访问器 `berry_libc::io::wm_present_h` 取用（「同一 gift 位对不同程序承载不同对象」多产面惯例；files/photos 批同口径）；input 固定位 [3] = `input_h()`。settings（无 fs 词）wm 落 [0] 走 `wm_cli_h`——两形分野单源 = svcmgr app_launch。
- **帧循环/退出照 settings 先例**：wm 门控（0 = `browser: wm absent (no gui gift)` + park 常驻）→ WM_GETINFO → present GETINFO 轻断言 → 帧 VMO 三连@FRAME_VA=0x5100_0000 → 主循环（INPUT_POLL 批拉 → push_pointer → `pump_input`（referee + press_react/widget_react 同拍）→ Msg 消费 → take_events（EV_GO）→ NavHost tick/take_popped → 进度步进 → render_frame damage 驱动零空转 → SUBMIT/FRAME_CALLBACK/RELEASE_WAIT 串行（在途 ≤1）→ vote 变化才发 → wait_many n≤2 + 10s IDLE 帽）；无退出路径（fail = eprintln + park，崩溃监督归 svcmgr）；首帧回执 `browser: first frame`。RPC 半程族/`damage_envelope`/`input_arm` = settings 同形拷贝。
- **预算**：节点 ~160/512（书签页 ~100 + 页页 ~60）；a11y ~17/32 帽；press_map 1/28；glyph scratch 384 ≥ 全树绘制文案峰值 ~290（v1 无 clip——页页叠书签页期两子树同帧绘制，photos 256 档不够用故抬 384）。

## 三、v1 形态 vs 终态差异（3b 依赖降解清单）

未落底座全景见 W4-RECON §四；本应用逐项：

| # | 3b/波 2 依赖（未落） | 本应用 v1 降解 | 终态形态（落齐后） |
|---|---|---|---|
| 1 | **berry-appshell crate**（rpc/pump/scene/icons/conf 五模块） | settings 同款手拼 RPC 半程 + 自管帧循环（§二末条）；BOOKS.JSN = layout_io 食谱自实现 | appshell::rpc/pump 收编帧循环；conf 收编 JSN 键族（格式随 conf canonical 换轨） |
| 2 | **wm 礼物槽位无专用访问器**（svcmgr :6154-6158 自认「wm 落 [2] 暂无 app 侧访问器 = 裁决 A 位」） | 经同槽访问器 `wm_present_h` 取 extras[2] 的 wm 客户端端（多产面惯例；files/photos 批同口径，零基座改动） | 裁决 A（`ui=["wm","input"]` 能力词 + appshell crt0 位序立法）落地换轨专用访问器，本行一处替换 |
| 3 | **wm back 路由**（encode_wm_back_req 零消费）+ 根页优雅退出（R3） | 返回 = 页页内 app_bar 返回 glyph（app 内手势位）；根页无退出路径（settings park 先例同形） | 系统 back 全链 pop + 根页 back 优雅退出（exit 0 + 窗 DESTROY + BACK_ACK） |
| 4 | **softime/软键盘**（OP_INPUT_INJECT 无消费者） | 地址行只读展示（Input 值面 = placeholder 承载，弱化灰显）；换址走链接点按 | softimed 敲键回显 + 地址行可编辑 + Go 直达任意 URL |
| 5 | **theme 样式族**（card_style/oh_press_bg/OH_PRESS_SCALE_MIDDLE_PERMIL=950——主树未落，W4-RECON §二末段） | 卡按压 = ItemState 五态表 app 侧换底（files 同款 Stack 面层 Rect）；卡面/圆角 = Card kind 自绘 + LISTITEM_RADIUS 常量单源；无 0.95 缩放档 | oh_press_bg 20% 预混遮罩 + 0.95 中点缩放档引擎单源——按压面原地换轨，卡结构不变 |
| 6 | **top_bar 无返回位 / app_bar 无动作位**（pattern v1 诚实标注） | 「TopBar 含 Back+刷新」= app_bar(back) + bar 行尾补挂 REFRESH Icon（两键在席；Icon Click 锚路由） | pattern 顶栏配方补动作位/返回位后迁回配方单源 |
| 7 | **Fab paint 恒绘 '+'**（paint 不消费 FabProps.icon id——photos §三 #8 同面） | Go/Stop = Button kind 文字档（引擎 PressedVis 按压视觉在席）；刷新/返回 = Icon kind（程序化 glyph 可见可点，无按压视觉——Icon 不在 press_map visual 白名单，contacts 返回键同状） | Fab/icon_button 按 icon id 绘字形 + 入 PressedVis 表——按钮结构原地换轨 |
| 8 | **net 面**（无网络栈/词；net 词 loud 拒与终态同——本 v1 不声明 net 词） | 页体 = 本地程序化模拟页（六站 × 两页 const 表）；状态行如实 `simulated page - no net`；Loading/Page 词汇如实 | 真抓页（URL→页体解析管线）+ net 词授权——模拟表降级为离线档 |
| 9 | **多级历史/Forward**（任务书「历史 v1 单级」自裁） | 链接点按就地换页（同栈位重启加载）；back 恒回书签 | 多级历史栈（NavStack 深用）+ Forward 位 + 历史列表页 |
| 10 | **书签编辑面**（稿 §2.7 无增删书签交互；CONTACTS.JSN 同批降解先例） | 书签集 = 六站静态种子；JSN 只存上次到访（`last`）——「书签集真存」降解为集序回显 | 书签增删/改序真存 BOOKS.JSN（星标/位串/条目形随 appshell::conf 定稿） |
| 11 | **Scroll kind 无手势控制器**（offset 纯 props 域） | 书签网格/页体均按内容恰一屏纳设计（photos §三 #10 同裁定）；无滚动面 | 超一屏长页（Scroll + 自管 offset 滚动，contacts 同款跟手步进可迁入） |
| 12 | **Card elevation 阴影带 vs 不透明按压面层**（v1 无 alpha 通道） | elevation 0 平卡（elevation > 0 的阴影带会被按压面层白整面盖没——files/photos 同裁定） | oh_press_bg 半透遮罩落地后 elevation 档随迁（阴影带可见） |
| 13 | **Progress/文案刷新 = 整帧提交链**（无局部动画面） | 加载相逐拍 set_props/set_text（damage 驱动增量帧，vsync 拍点 60fps） | 动画面引擎收编（进度条过渡插值）；语义不变 |

另（应用内小裁，非 3b 依赖）：Input 内在 200×48 定尺（SIZE 块拒扩）——地址 13B 域内显示无截断；站名 ≤ 10B/地址 ≤ 13B 表域钉板（单测执法）；「Loading 0%→100%」满格三位档（宿主对拍捕得 `b'0'+10` 字形塌 bug 已修——见 §四）。

## 四、验证记录

- `cargo check --release --target aarch64-unknown-linux-musl`（berry-ld RUSTFLAGS 形）：**过、零 warning**；同旗 `--tests` 同过（单测纯函数面——RPC/帧循环 berry-libc AArch64 专属 asm 不入 host 执行面，files/photos 同口径注记）。
- `cargo build --release`（同旗，CARGO_TARGET_DIR=cargo-target——justfile :97 同形）：过——ELF **464,280 B ≈ 0.44 MiB** 静态链（< 稿估 .bap 0.90 MiB；对照 settings 435KB / photos 457KB——NavHost/双页/glyph 增量 +7KB，LTO 死码消除下可信）。未做 mkapp/入镜像（归接线批，裁决 B 前置铁律）。
- **宿主侧纯逻辑对拍**（photos 先例：从 main.rs 原文提取纯函数面 + stub Color/theme 常量独立编跑，零转写）：JSN 编码恰 21B 逐字节钉板、往返全等（None/0/5）、坏形五连拒（空/断头/schema 变体/尾形/非数字）、域外站序自愈门（7 拒 5 收 None 收）；站点表 6×2 页全 ASCII、域帽（名 ≤10B/址 ≤13B/题 ≤16B/行 ≤32B/链接 "> " 前缀 + 目标域内）、每页恰 2 链接（全表 24）、站名/home 地址互异；文案装配钉板（`Loading 0%`/`50%`/`100%`、`Last: news.berry`/`forum.berry`、None 空文）；glyph 三表互异、全像素 ARGB 域内、非空密度 > 40、BACK 折点/GLOBE 十字心着墨。**对拍捕得一真 bug**：`pct_bytes` 满格档 pct=100 走 `b'0'+(pct/10)` 产 `':'` 字形塌（"Loading :0%"）——已修（三位档直录 "100"）后全绿。
- **glyph ASCII 面目检**（contacts 先例）：BACK 双臂 chevron 成形；REFRESH 环 + 东北缺口 + 缺口内「>」箭尖（环流入尖，圆箭头成形）；GLOBE 外环 + 赤道 + 经线线框（十字心着墨）。
- 铁律执法门（§五）+ `git status` 足迹核验（仅 `user/bins/browser/` + 本报告；他批 `user/bins/{camera,messages}/`、`W4-IMPL-MMS.md` 在册未碰）。

## 五、铁律合规自检

- **零手拼**：卡片 = `pattern::card` 配方（grep `ViewKind::Card` 直载 **0** 处——卡片全走配方面；`ViewKind::Rect` 恰 3 处 = 双页不透明底（pattern app_bar 面层语）+ 卡按压面层（files 叠层习语），非拼卡）；骨架 = `pattern::top_bar`/`app_bar`/`stack`/`spacer`；网格 = Grid kind 等分轨；进度 = Progress kind；按压色 = `pattern::ItemState` 五态表 + 引擎 press_map（Button）——grep 门：`0xCCE3FF`/`PRESSED_BG`/`0x404040`/`0.95`/`corner_radius` **代码面零命中**（唯一 1 处 = 模块头注对未落常量名 `=950` 的引述，非字面量消费）；Color 构造恰 1 处（state_color 解包 ItemState 单源）。
- **文案 ASCII**：全部 `b"..."` 字面量经宿主过滤非可印字符零命中（词汇表内词：Browser/Last:/news…berry/Loading n%/Done/Go/Stop/页体行文/模拟页标注）。
- **帧循环/退出形态照先例**：settings 同形拷贝（RPC 半程族/VMO 三连/SUBMIT 链/vote/wait_many/IDLE 帽/无退出路径 park）；诊断面 = `browser: wm v=… input=…`（起握手）/ `browser: first frame`（首帧回执，console=false 走私有对）/ `browser: <what>`（fail+park 收口）。
- **边界**：未触 launcher/guifbd/wmd/svcmgr/berry-* 基座/justfile/tools/rootfs-manifest；他批 crate 与报告未碰；appstub/apps/BROWSER 旧资产未动（资产迁 crate 为任务书口径，旧副本退役归接线批）。

## 六、交接注意点（接线批）

1. **入镜像序**：裁决 B（boot 批 ui deferred）先行——本 crate 是常驻 park 形 GUI app，未经裁决 B 直入镜像 = boot 批 app_reap_tail 永等 → `APP ok` 缺失全线 smoke FAIL（W4-RECON §1.7 铁律）。
2. **打包**：justfile :328-337 七应用 appstub 循环退役后按 APPTOMB :315-327 食谱逐 crate 段（musl + berry-ld RUSTFLAGS → staging bin/ + manifest + icons/views → mkapp）；资产源二选一钉单源（crate 内已在席——建议 appstub/apps/BROWSER 随循环整体退役）。`tools/rootfs-manifest.toml` 零改（条目名 `app/BROWSER.bap` 不变）；单包实测入账（估 0.90 MiB 档，ELF 实测 0.44 MiB 余量足；>0.9 MiB 查因门）。
3. **manifest 合规**：fs 词 + [ui] 节 + [security] 三键齐备——mkapp 五道门禁/sign 面零改可过；svcmgr GUI 授权链现认 [ui] 节（裁决 A 换轨时 manifest 词面随批全族统一改）。
4. **v1 交互语义待 3b 后回归**：系统 back pop（#3）/地址行敲键（#4）/0.95 按压缩放（#5）——终态落齐后按 §三表逐项换轨，卡结构与锚面已按原地换轨设计。
