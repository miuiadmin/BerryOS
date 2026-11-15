# W4-RECON —— 波 4 先行批侦察（只读解剖）

基线：worktree `wip/ui-wave4` 自主树 HEAD 769515a9a 开出（含 9035af061 Settings 垂直切片 + 5452649af svcmgr GUI 通道授权两先例）。全部行号为 2026-11-15 工作树现场直数；包体/rootfs 尺寸取主树构建产物实测（user/userbins/app/、rootfs*.cpio，Nov 15 14:22 批）。

## 〇、结论速览

1. **Settings 先例已落但未接线**：crate（941 行真 GUI）+ svcmgr 授权两件在册，但 justfile :328-337 打包循环仍对 SETTINGS 七应用一律 cp appstub——镜像内 SETTINGS.bap（61,440 B）仍是秒退壳；ELF 实测 435KB / .bap 491KB（commit msg 口径）未入档。
2. **pattern.rs 主树版 27 公开面位零真实消费**：v0 settings 直用 desc Props 手拼（组件内在几何常量按值镜像），pattern 唯一消费面 = 自带单测（:2152-3022）。波 4 七应用 = 首批真实消费者。
3. **波 3（3b）全线未落**：berry-appshell crate 不存在、WM_CREATE_F_IMMERSIVE 无（flags 仅 AVOID_SYS=1<<0）、softime 驱动无（OP_INPUT_INJECT=0x22 在 guiproto :112 定义但 drivers/bins 无消费者）、wm back 链无（encode_wm_back_req :1927 无人用）、launcher 无 BOOT.JSN/TabBar 设置页/​apps 预建 mkdir。
4. **设计稿行号锚点普遍漂移 +33..+141**（svcmgr 面最重）；pattern.rs :1234 → 实际 :1045（移植后 −189）；theme.rs 样式族（card_style/oh_press_bg/OH_PRESS_SCALE_MIDDLE_PERMIL）主树未落（ui-wave2 工作树独有），主树等价面 = pattern::ItemState 五态预混表 + paint PressedVis 查表。
5. **预算宽裕**：八改造包现 843,776 B（0.80 MiB）→ 估 6.70 MiB（表列逐项和；稿内文 5.90 与表不自洽，差 0.8 MiB）；rootfs 现 34.0 MiB → 估 ≈40 MiB；--split 32 自动分卷消化，实质硬界 128MiB 总量帽余 ≈88 MiB。

## 一、Settings 垂直切片先例（9035af061 + 5452649af）

### 1.1 manifest.toml 全形态（user/bins/settings/manifest.toml，22 行）

- `[app]` id=SETTINGS / version=0.1.0 / entry=bin/settings / type=app / intent=utility。
- `[capabilities] console = false`（stdout 走 svcmgr 私有对，drain 归 app_supervise；无 fs 词——v0 不做持久化）。
- `[runtime] stack_pages = 64`（appstub 壳为 8——真 GUI 链 UiMain 构造中间体 LTO 后仍部分落栈 ~29KB 的 8 页穿页先例，故 x8）。
- `[security]` profile/key_lineage/integrator 三键同全家桶。
- `[ui]` icon=icons/icon.byi / label=Settings / views=views/main.bui——资产节三键。
- Cargo.toml 形态 = berry-launcher 镜像：独立小工作区（空 [workspace] escape user 根）、字面 version=0.2.0（L-SYS 规则/14，无 workspace 继承）、deps = berry-libc+berry-sys+berry-guiproto+berry-ui+berry-gfx（+berry-guiproto 是任务书草签三偏差之一：berry-ui 只引类型不 re-export 收发面）、release profile opt-level=z/lto/codegen-units=1/panic=abort/strip。

### 1.2 [ui] 通道授权链（5452649af，svcmgr main.rs :6136-6260）

- 判别门：`wants_gui = man.ui.is_some() ∧ WMD_CLI_DUP≠0`（两值 Relaxed load）——manifest type 无 gui 分类（app/driver 两值域 ADR #82），[ui] 资产节兼判别位是过渡形；头注明示「裁决 A（capabilities `ui = ["wm","input"]` + appshell crt0 位序立法）落地时本面整体换轨退役」。
- dup 源与权：WMD_CLI_DUP / INPUT_A 常驻源现 dup，RIGHT_SEND|RECV|TRANSFER（TRANSFER = env 块迁移 send 预检保位；wmd arm w_dup 同口径）。
- gift 槽位布局（前缀紧致律，berry-libc io.rs GIFT_H 按位落种对偶）：wants_fs → `[rootfs, fs, wm, input]`（wm 落 [2]、input 固定 [3]=input_h() 位）；!wants_fs → `[wm, wm 占位×2(gui_ph), input]`（wm 落 [0]=wm_cli_h() launcher 镜像位、input 到 [3] 需满前缀——洞 = send 预检 0 值 -EBADF 自陷）。
- inputd 缺席（INPUT_A==0）= 尾截断：wm 照授，app 侧 input_h()==0 静态页降级（launcher 同款）；半途失败反序全量回收 + `eco: app gui dup` loud 行；APPQUIET 无 [ui] 不入此面（lifecycle absent 探针断 GIFT_H[0]==0 零漂）；console app（APPDEMO 带 [ui]）照授 = app 侧无访问器惰性无害。
- stdin_feed 判据从 wants_fs 改 gifts 非空（SDK-1a：extras 非空须 stdin 槽在席，零长 feed 纯占位）。

### 1.3 main.rs 形态（user/bins/settings/src/main.rs，941 行）

- **建树**：main() 降级门 wm_cli_h()==0 → `settings: wm absent (no gui gift)` + park 常驻（不秒退——窗口义务语义）→ WM_GETINFO 几何真值 → `Box::leak(Box::new(UiMain::new(w,h)))` 堆置 + ProportionalTextMetrics → build()：根 Col(Center) → 顶留白 → 内容 Col(Stretch, content_w) → 标题 Text + Divider + 四行 `Row[定宽标签列, flex 推位, 尾件]`（Toggle×2 / 值 Text+Slider / Check）；props on_change 携 EV_* id；a11y set_a11y 登记。
- **帧循环 run()**：present GETINFO 轻断言（版本/FMT_XRGB8888/credits + 与 wm 几何失配 fail）→ RELEASE_WAIT 跨代清账 → 帧 VMO 三连 create/commit/map@FRAME_VA=0x5100_0000 → glyph scratch 堆置 256 → set_vsync(16_666_666)+wake → 主循环：INPUT_POLL 批拉（≤3 事件）逐条 push_pointer → pump_input（referee+widget_react 同拍）→ take_events 消费（EV_BRIGHT → set_text 百分值，值同跳过）→ render_frame（damage 空集零 SUBMIT 零 RPC）→ drew 则 Queue::submit 光栅 + SUBMIT(surface_id=0 全幅, alias dup READ|TRANSFER) → FRAME_CALLBACK 断 PRESENTED → RELEASE_WAIT 归还别名（在途 ≤1 结构性背压）→ vote 票面变化才发 → wait_many n≤2 [wm READABLE, input READABLE] + deadline=min(next_wakeup, now+10s IDLE 帽)。
- **退出形态**：无退出路径——fail()=eprintln+park；崩溃监督归 svcmgr app_supervise 域。首帧回执 `settings: first frame`（presented==1 一次性 println，诊断面）。
- **组件几何镜像**：TOGGLE_W=52/SLIDER_W=200/CHECK_W=24 按 lib.rs「组件 v1 几何常量」pub(crate) 面按值镜像并注明单源——pattern 消费后此面随迁。
- RPC 半程族 rpc_send/rpc_recv/rpc（300s READABLE 帽、EBUSY 阻塞等 WRITABLE 重臂、send 即 take）与 damage_envelope K 矩形包络、input_arm（EBUSY 等 10s、失败置 hi=0 静态页续活）均 launcher 同形拷贝。

### 1.4 pattern 消费清单（现状 = 零）

v0 settings 直用 `berry_ui::desc::{CheckProps,ColProps,DividerProps,RowProps,SliderProps,TextProps,TextStyle,ToggleProps}` + layout::CrossAlign + lib::{NodeId,Props,TextBuf,UiMain,ViewKind,ViewSpec,ROLE_*}——未 import pattern/theme 任一面。appdemo 42 行秒退自验件、launcher 建树在 desktop.rs 手拼，同样零 pattern。pattern.rs 唯一消费 = 模块内单测（:2152-3022，含组件画廊 golden 钉板，commit 463482569/dd53bc039）。**波 4 七应用 + appdemo 画廊 = pattern 首批真实消费场。**

### 1.5 FAT 持久化用法（v0 settings 无 fs；真先例 = launcher layout_io.rs）

- 单簇 ≤2048B 近原子（LAYOUT_BYTES_MAX 单源 berry-layout 双侧执法）；三约束纪律：**缺席回默认 + 坏读自愈（回默认重写）+ 写失败非致命**（eprintln 后照用内存值，常驻不死）。
- 严格串行单 RPC 会话（fsclient 单读者协议：一请求一应答无序号，一次只持一个 fd，open→…→close 不交叠）；**close 即 commit**（TRUNC 游标两段式，C-1 裁决 2，bsh put_file 同款）。
- `fs_h()==0` 结构性早退零 RPC（冻结相零扰动）；API = berry-libc fs::{open(O_CREAT/O_RDONLY/O_WRONLY), read, write} + io::close。
- 冒烟面：数据盘根哨兵 GUI8.SMK 全字节比对 → LAYOUTW ok（写后读回自检）/ LAYOUT ok n=2（跨 boot 回读）；gui8-smoke.py 钉 LAYOUT_CANONICAL 109B 确定性字节。
- 波 4 应用侧（SETTINGS.JSN 等六键族）规划走 appshell::conf——**crate 未落**，v1 降解见 §四。

### 1.6 打包入镜像链（mkapp/userbins/rootfs 装配）

- 现状（justfile build-user）：APPDEMO(:301-312 musl + icons/views staging)/APPQUIET/APPTOMB(:315-327 musl 显式段) 三件各自产 .bap；**:328-337 GUI-8 循环对 SETTINGS/FILES/PHOTOS/CAMERA/CONTACTS/MMS/BROWSER 七件一律 cp `cargo-target/aarch64-unknown-none/release/appstub` + `bins/appstub/apps/<ID>/{manifest.toml,icons/,views/}` → mkapp**——settings crate 未进链。
- crate 入包先例形态 = APPTOMB 段：`RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" cargo build --release --manifest-path bins/<c>/Cargo.toml --target aarch64-unknown-linux-musl` → mkdir staging/bin → cp manifest + ELF + assets → `python3 ../tools/mkapp.py userbins/app/<ID>.bap <stage>`。
- rootfs：`python3 ../tools/mkcpio.py ../rootfs.cpio --manifest ../tools/rootfs-manifest.toml --split 32`——条目名 `app/<ID>.bap` 不变则 rootfs-manifest.toml **零改**；.bap.sig 伴生自动收编（sign-user 产 .sig 共置即随档，缺席零漂移）；mkapp 门禁五道（manifest 唯一/bin 唯一/ELF 魔数等）不动。
- mkcpio 垫料 X-3c：头+名与数据均垫 4096（CPIO_PAD），条目长恒页倍数，data_off 页对齐。

### 1.7 smoke 的 APP 计数断言随动（判读）

- `APP ok n=9` = rootfs 相第 23 标记（markers.sh:24）：9 槽 = APPDEMO+APPQUIET+七 stub（APPTOMB selftest/ 前缀不入册）；谓词 = 全槽 bap_scan→manifest→双守卫→拉起→EOF/reap 真码 0，`APP_SLOTS<10` 编译期钉单数字位（:2198）。
- `APPUI ok n=` = 第 24 标记：[ui] 节 ui_decl>0 且 ui_ok==ui_decl 才打（与 APP ok 独立判）；现 n=8（APPQUIET 无 [ui]）；smoke.sh/smoke-image.sh 只断前缀 `APPUI ok n=`，gui8-smoke 断 MARK_APP_N9 全串（boot 相等待锚列表 :1096/:1421/:1444/:1459）。
- **随动铁律：任一常驻 GUI crate（park 形）未经裁决 B（ui 词 → 入册不拉起 + `app <id> gui deferred` 诊断行）先入镜像 = boot 批 app_reap_tail 永等 → `APP ok` 缺失 = 全线 smoke FAIL。**接线序 = 裁决 B 先于 settings crate 接线；裁决 B 落后 n=9 语义变「拉起子集收口」仍打 9，gui8 两处重裁（boot 锚改桌面就绪 + `app APPDEMO up` 恰 1 行）同批落。
- appdemo 画廊化后常驻占 1 活槽：APP_SPAWN_LIVE_MAX=4（:817）预算内（画廊+三应用）；`app APPDEMO fs ok 23` 行时点随启动期移位但无断言面。

## 二、pattern 23 件 API 速查表（主树版 user/berry-ui/src/pattern.rs，3022 行）

口径：波 2 刀 3 资产 23 配方经主树移植（dd53bc039）后 = **20 个 `pub fn` 配方 + 7 个 attach 型 struct 器件 = 27 公开面位**（Check/Radio/Sbar/DatePick/TimePick 等回归 kind 直载/引擎面，不在本表）。全部 `<const N: usize>(ui: &mut Ui<N>, parent: NodeId, …)`；tokens 参数 `t: &theme::Tokens`。纪律：零 alloc、零 panic（定容满 → loud Err）、文案 ASCII。

| 件 | 行 | 签名要点（省略 ui/parent） | 产物/备注 |
|---|---|---|---|
| spacer | :299 | `(w, h)` | NodeId；定宽高 Col 占位 |
| divider | :314 | `()` | NodeId；Divider kind 直载，Row 内主轴塌（Row 用显式 w Rect） |
| divider_v | :324 | `(color)` | NodeId；Rect w=1 竖线 |
| stack | :336 | `()` | NodeId；子 max×max 叠放，z=子序 |
| app_bar | :356 | `(t, title, back: bool)` | AppBar{frame,bar,title,back:Option}；56vp 高 |
| top_bar | :407 | `(t, title, actions: &[u16]≤3)` | TopBar{frame,bar,title,actions:[Option;3]}；v1 无 flex 顺排（诚实标注） |
| scaffold | :456 | `(t, title: Option<(&[u8],bool)>, bottom: Option<&[NavItem]>)` | Scaffold{frame,top,content,bottom} |
| NavStack | :119 | `new()/push(NavPage)/pop()` | 值面页栈（NAV_DEPTH_MAX=8）；NavPage{key,title:Option} |
| NavHost | :497 | `attach/push(ui,page)/pop(ui)->bool/tick(ui,now)/take_popped/top/depth` | Stack 叠放容器 + 300ms 推拉动画（NAV_ANIM_NS） |
| NavItem | :620 | `{icon: u16, label: &[u8]}` | tab/bottom 入参 |
| tab_bar | :707 | `(t, labels: &[&[u8]], active)` + `set_active(ui,i)->bool` | TabBar{frame,n,active,state} |
| seg | :794 | `(t, options, active)` + `set_active` | Seg；32vp 高 Grid 等分 |
| step | :853 | `(t, steps, current)` | Step{frame,marks:[Option;8]}；静态，进度变=重挂 |
| bottom_tabs | :931 | `(t, items: &[NavItem], active)` + `hit(node)->Option<usize>` | BottomTabs；48vp Grid |
| card | :1011 | `(t, elevation, leading_icon: Option<u16>)` | Card{node,content,…}；corner_radius=LISTITEM_RADIUS(8vp@3x=24px) |
| list_item | :1045 | `(t)` | Card（= card(0,None) 别名）；48vp 五态行 |
| Collapse | :1094 | `collapse(expanded, on_toggle)` + `set_open(ui,open)->bool/is_open` | Collapse{node,body,state}；kind 直载 expanded 翻转三面守卫 |
| SwipeItem | :1125 | `attach(t, width, action_count, on_action)` + on_msg/tick/is_open/set_open | {frame,node,content,actions}；跟手横拖 + QuadOut 吸附 |
| DragReorder | :1359 | attach/add_item/sync_totals/on_msg/len/order/reset_offsets | 拖拽重排 ≤8 |
| chip | :1565 | `(label, selected, on_tap)` | Chip{node,state} |
| badge | :1583 | `(count)` | NodeId；count=0 隐藏、>99 封「99+」 |
| qr_show | :1597 | `(module_count, dark, light)` | NodeId；版本域 {21,25,…,177} 域外 loud 拒 |
| fab | :1619 | `(icon, extended_label, on_tap)` | Fab{node,state}；空标签=56 圆形 |
| icon_button | :1636 | `(icon, on_tap)` | Fab（Fab kind 空扩展标签小档） |
| gauge | :1663 | `(value_permille 0..=1000, style: GaugeStyle)` | Gauge{node,state}；Ring/Bar/Arc 三档 |
| Sheet | :1686 | `attach(t, width, height)` + on_msg/tick/is_open/set_open | 底部抽屉；{frame,panel,content} |
| Drawer | :1842 | attach + on_msg/tick/is_open/set_open | 侧滑抽屉（Sheet 换轴） |
| Refresh | :2012 | `attach(t, on_refresh)` + on_msg/settle/is_refreshing/on_refresh | 下拉刷新（REFRESH_THRESH_PX=32vp） |

常量速记：DENSITY=3、TOPBAR_H=56vp、BOTTOMTABS_H/LISTITEM_H=48vp、LISTITEM_RADIUS=8vp、NAV_DEPTH_MAX=8、TABS_MAX=8、SWIPE_REVEAL_W=96vp、NAV_ANIM_NS=300ms。五态：`ItemState::{Normal,Pressed,Selected,Disabled,Focus}` `.bg()` 预混表（Pressed=brand 20% 预混白 0xCCE3FF，:87-110）。**主题样式族（card_style/chip_style/oh_press_bg/OH_PRESS_SCALE_MIDDLE_PERMIL=950/FiveState/StyleParams）主树未落**（ui-wave2 theme.rs 独有）——主树按压反馈等价面 = paint::PressedVis 查表（ACCENT_PRESSED 换底，disabled 优先）+ ItemState 表；波 4 引用前须先裁「补移植 or 用 ItemState 面」。

## 三、设计稿锚点核对（.agents/plans/2026-11-15-波4预制应用翻新实施设计.md → 本工作树）

| 稿内锚点 | 现状 | 判 |
|---|---|---|
| §1 launcher 五模块（main/frameloop/desktop/icons 含 status_icons+wallpaper/layout_io） | 全在 user/bins/berry-launcher/src/（icons 的两件为独立文件非子模块，无碍） | OK |
| §1 pattern.rs :1234 list_item | 实际 :1045（主树移植后 −189） | 漂移 |
| §1 theme.rs card_style/OH_RADIUS_VP/oh_press_bg/OH_PRESS_SCALE_MIDDLE_PERMIL=950 | OH_RADIUS_VP=8 在（theme.rs:70）；样式族/预混/缩放档全不在主树（ui-wave2 独有） | **未落** |
| §1 appdemo main.rs:14-20 argv[0] 断言 | 注记 :11-16、assert :27-31（42 行文件） | 近似 OK |
| §1/§2.2 justfile :324-334 七应用块 | 实际 :328-337（+4） | 漂移 |
| §2.1 launcher TabBar 设置页 + BOOT.JSN（波 3 §7） | launcher 无 TabBar/BOOT.JSN（layout_io 仅 LAYOUT.JSN） | 未落（3b） |
| §2.2 /apps 预建 mkdir→rpc_mkdir→OP_MKDIR | launcher 零 mkdir；fsproto OP_MKDIR=6 在（fsproto lib.rs:95）链可用 | 半落（链在，消费无） |
| §3.1 裁决 A `ui=["wm","input"]` 能力词 | 未按稿；5452649af 以 [ui] 资产节兼判（:6139 注记自认过渡形） | **形态分叉** |
| §3.1 裁决 B boot 批 ui deferred | 未落（boot 批仍全槽拉起 + app_reap_tail :2170-2177） | 未落 |
| §3.1 launcher_assemble :6935-6991 | fn 实际 :7075（+140） | 漂移 |
| §3.1 boot 批 main.rs:2107-2148 | 实际 :2140-2215（+33） | 漂移 |
| §3.1 APP_SPAWN_LIVE_MAX=4 预检 :7281-7299 | const :817、预检 :7407-7421（+126） | 漂移 |
| §3.1 app_supervise :7446-7456 | fn 实际 :7587（+141） | 漂移 |
| §3.1 frameloop.rs:856 encode_spawn_request | :856 精确命中 | OK |
| §3.1 berry-manifest lib.rs:1108 unknown key | :1108 精确命中 | OK |
| §2.8 appdemo label=Gallery | manifest 仍 label="APPDEMO"；`app APPDEMO up`/`fs ok 23` 行在（main.rs :31/:40） | 待改 |
| §6 SwipePager = swipe.rs | SwipePager :53 在册（new/attach/snap_to/on_msg/tick/page/page_node） | OK |
| §1/§11 依赖 berry-appshell 五模块 | crate 不存在 | 未落（3b） |

判读：稿内「以波 3 落地为准」的引用面全部悬空——波 4 工作树若照稿直写七 crate，appshell/back/softimed/IMMERSIVE/Toast 五面必须先走 §四降解或先落 3b。svcmgr 行号簇整体 +126..+141（boot 批后 app 批/UI 面/裁决 D 系列插入所致），改稿引用时按本表现场行号重锚。

## 四、v1 降解面清单（3b 未落——逐应用「v1 形态 vs 终态差异」）

未落底座（影响全部应用）：①berry-appshell（rpc/pump/scene/icons/conf 五模块 + Nav back 消费 + ime_wanted + Dialog/Menu/Toast Overlay）全缺 → 应用侧沿用 settings v0 手拼 RPC 半程 + 自管帧循环；②wm 仅 WM_CREATE_F_AVOID_SYS（guiproto lib.rs:132）无 IMMERSIVE 位；③softime 驱动无（OP_INPUT_INJECT=0x22 定义在册 :112、gui-inputd 无消费分支）；④wm back 路由无（encode_wm_back_req :1927 零消费）；⑤theme 样式族未落（§二末段）。

| 应用 | v1 形态（3b 缺席降解） | 终态差异（3b/波 2 落齐后） |
|---|---|---|
| Settings | 纯 pattern 建树 + widget_react；SETTINGS.JSN 经 launcher layout_io 食谱自实现（libc fs 直开直写单簇）；无 NavStack back——单页 + 根页即退出位；Boot mode 项无 BOOT.JSN 真源 → 砍或只读展示 | appshell::conf 键族 + Nav pop + BOOT.JSN 与 launcher TabBar 设置页同键同源；Radio/Seg 详情页推拉 |
| Files | /apps 枚举可行（OP_MKDIR 链在 + launcher 预建落则真树）；Search 无 softimed → 预置过滤 chips（chip 件）前缀过滤替代敲键 | softimed 升起敲键回显；面包屑 chip 跳级同形可保 |
| Contacts | 列表→详情用 NavHost 值面页序自管（pop 走 app 内手势位/自绘返回 glyph 的 icon_button，无系统 back）；Dialing Toast → 应用内底部 Text 瞬态行（帧循环节拍 2s 自撤）；CONTACTS.JSN 同 Settings 食谱 | 系统 back 全链 pop + appshell Toast 窗内 Overlay |
| Photos | 看片窗无 IMMERSIVE → 全幅窗近似（AVOID_SYS 或 surface 全幅 + 状态栏区域自绘遮盖的诚实标注）；SwipePager 在册可用（swipe.rs 波 2 已落主树） | WM_CREATE_F_IMMERSIVE 沉浸态 + 状态栏隐藏/回显链 |
| Camera | 全屏取景同上全幅窗近似；白闪 = 全屏白 Rect 单帧（非三禁区）；Saved Toast → 应用内角标/瞬态行；快门计数 CAMERA.JSN 可落（fs 词 + layout_io 食谱） | IMMERSIVE + appshell Toast |
| MMS | 无 softimed → 固定快捷短语 chips（点选即发）+ Input 草稿展示（Edit/Input kind 值面可显不可敲）+ Me 侧节拍模拟回复；会话 NavHost 同 Contacts 自管 | softimed 真敲键 + ime_wanted 焦点环 + R3 先收键盘 back 链 |
| Browser | 书签/页页 NavHost 自管；进度 Sbar（kind 在册）帧驱动；本地模拟页体不变（net 词 loud 拒与终态同）；BOOKS.JSN 可落 | 系统 back pop + 地址行 softimed（同 MMS 降解） |
| APPDEMO | 画廊化可行（pattern 27 面位 + 引擎 41 kind 全在）；Toast/Dialog/Menu 三演示位降级 = 自绘 Overlay 近似（Stack 叠放 + 瞬态行）；常驻 park 形 = 裁决 B 前置（§1.7 铁律） | appshell 三 Overlay 原生演示 |

## 五、打包与预算

### 5.1 现尺寸（实测，userbins/app/ 十包 + 不动两包）

| 包 | B | KiB | | 包 | B | KiB |
|---|---|---|---|---|---|---|
| APPDEMO | 385,024 | 376.0 | | MMS | 77,824 | 76.0 |
| APPTOMB(不动) | 352,256 | 344.0 | | CONTACTS | 61,440 | 60.0 |
| APPQUIET(不动) | 348,160 | 340.0 | | SETTINGS | 61,440 | 60.0 |
| BROWSER | 94,208 | 92.0 | | CAMERA/PHOTOS | 57,344 | 56.0 |
| FILES | 49,152 | 48.0 | | | | |

八改造包合计 = **843,776 B = 0.80 MiB**（稿 §6 修正口径吻合）；十包合计 1,544,192 B = 1.47 MiB。settings v0 实测（commit msg）ELF 435KB / .bap 491KB ≈ 0.42/0.48 MiB < 稿估 0.85——上界锚成立（v0 不含 pattern/画廊面，真形态会涨）。

### 5.2 新增六包估算（稿 §3.2 表 .bap 估）

FILES 0.85 + CONTACTS 0.80 + PHOTOS 0.80 + CAMERA 0.75 + MMS 0.85 + BROWSER 0.90 = **4.95 MiB**；加 SETTINGS 0.85 + APPDEMO 0.90 → 八包估和 **6.70 MiB**（稿内文写 5.90 MiB 与自家表逐项和不自洽，差 0.8 MiB——以表为准落账复核）。Δ ≈ +5.9 MiB。实施门：单包构建后实测入账，>0.9 MiB 查因；BAP_MAX 4MiB 单包帽远未触。

### 5.3 rootfs 帽与余量

- 现 rootfs.cpio 35,676,160 B = **34.0 MiB**（62 条目，payload 和 33.7 MiB；大头 = uu coreutils 族 bin/ls 2.40 + bin/sort 2.33 + bin/date 1.91 MiB 等，app 族仅 1.47 MiB）。
- 已分卷：rootfs0 = 33,554,432 B（**恰满 32.0 MiB 单卷帽**）+ rootfs1 = 2,121,728 B（2.0 MiB）——mkcpio `--split 32` 只切字节流不做条目对齐，消费端内核 VMO 逐页拼装零改动。
- 波 4 后估 ≈ 40 MiB → rootfs0 仍 32 MiB + rootfs1 ≈ 8 MiB（十卷上限内）；实质硬界 = 总量帽 128 MiB → **余 ≈ 88 MiB 宽裕**。32MiB 帽对波 4 无实质约束（分卷已消化）。

### 5.4 rootfs 装配改动点（清单）

1. justfile :328-337 七应用 appstub 循环整段退役 → 逐 crate 段（APPTOMB :315-327 食谱：musl 构建（RUSTFLAGS berry-ld）→ staging → mkapp）；资产 icons/views 源保持 bins/appstub/apps/<ID>/ 或随 crate 迁（二选一，定稿时钉）。
2. crates manifest 增量：`fs = ["rootfs"]`（持久化应用）+ ui 授权词（按 §三分叉裁决：沿用 [ui] 节兼判 or 落裁决 A 能力词——svcmgr 现实现认 [ui] 节，改能力词需同批换轨 svcmgr :6136-6260 + berry-manifest schema）。
3. tools/rootfs-manifest.toml 零改（条目名不变）；.sig 伴生自动跟车。
4. 裁决 B（boot 批 deferred）先行——否则首件常驻 crate 入包即挂死 boot（§1.7）。
5. mkapp 五道门禁/mkcpio 配方零动；gui8-smoke 两处重裁 + 差异清单入册。

## 六、七路 crate 文件面清单（波 4 落地件）

每 crate = user/bins/<名>/ 独立小工作区（settings/Cargo.toml 形镜像）：`Cargo.toml`（字面 version + 空 workspace + 五 deps + release z/lto）+ `manifest.toml`（§1.1 形 + fs/ui 增量）+ `src/main.rs`（降级门→GETINFO→UiMain 堆置→build→run 帧循环骨架，settings 941 行为底模）。落位：settings（已建，翻新 pattern 化+JSN）、files、contacts、photos、camera、messages（bin 名与 manifest entry 一致：messages→bin/messages）、browser 六新 + appdemo（42 行升画廊）。资产仍由 bins/appstub/apps/<ID>/{icons,views} 供给（或随迁 crate 内，定稿钉单源）。
