# W4-IMPL-CAMERA —— 波 4 CAMERA 应用实施报告

基线：worktree `wip/ui-wave4`（共同依据 = [W4-RECON.md](W4-RECON.md) + `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.5）。实施件 = `user/bins/camera/` 独立 crate（settings 垂直切片先例形态）；未触 launcher/guifbd/wmd/svcmgr/justfile/berry-* 基座 crate 与他批实施件（`user/bins/files/`、`user/bins/contacts/`、`user/bins/photos/`、`user/bins/messages/`、`user/bins/browser/` 与对应 W4-IMPL-* 报告在册未碰——`git status` 佐证）。验收门：`CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$PWD/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" cargo check --release --manifest-path user/bins/camera/Cargo.toml --target aarch64-unknown-linux-musl` 过、零 warning；同旗 `cargo build` 过（ELF 450,176 B ≈ 0.43 MiB；注意 linker 须绝对路径——justfile `$ROOT` 形，相对路径 rustc 按 manifest 工作目录解析报 linker not found）。

## 一、改动清单（全部新增，无既有文件修改）

| 件 | 行数/尺寸 | 说明 |
|---|---|---|
| `user/bins/camera/Cargo.toml` | 46 行 | settings/photos 镜像（空 [workspace] escape、字面 version=0.2.0、deps 五件 = berry-libc/sys/guiproto/ui/gfx、release z/lto/cu1/abort/strip） |
| `user/bins/camera/manifest.toml` | 23 行 | id=CAMERA / entry=bin/camera / console=false / **fs = ["rootfs"]**（CAMERA.JSN 持久化授权词——photos 先例形态）/ stack_pages=64（settings 穿页先例同值）/ [security] 三键 / [ui] 三键——除 fs 词与 stack_pages 外与 appstub CAMERA manifest 逐键同值（intent=utility/label=Camera 原样） |
| `user/bins/camera/src/main.rs` | 1489 行 | 应用本体（下节形态）：程序化模拟景/快门白环供件 + CAMERA.JSN 持久化 + 取景/网格线/双控制带建树 + settings 镜像帧循环 |
| `user/bins/camera/icons/icon.png` | 2,681 B | 自 `bins/appstub/apps/CAMERA/icons/icon.png` 拷入（mkapp 编码 .byi 的 PNG 源——「源不入包」管道） |
| `user/bins/camera/views/main.bui` | 52 B | 占位 view（同 stub 形） |
| `user/bins/camera/Cargo.lock` | 自动 | cargo build 产物（settings/photos 同款在册） |

crate 内资产 = CAMERA 资产新单源（任务书「manifest.toml + src/main.rs + icons」口径）；`bins/appstub/apps/CAMERA/` 旧副本留待接线批随 appstub 七应用循环整体退役（justfile :328-337 现仍 cp appstub——本批不碰 justfile，接线序归整合批 + 裁决 B 前置铁律 W4-RECON §1.7/§5.4）。

## 二、v1 实现形态（对照设计稿 §2.5 页面树）

- **全幅取景**：根 Stack（`pattern::stack`）> 全幅 Image（360×640 内在分辨率、SmoothArgb 双线性上采样至视口——壁纸/样张同律）> 三分网格线 > 顶带 > 底带（后子盖前）。v1 全幅窗近似 = 单 surface 全幅取景常驻（无 IMMERSIVE 位，W4-RECON §四），带体 = 控制带自绘整幅底叠于取景上（photos 看片页同形）。
- **程序化模拟景**（`gen_scene`，确定性纯整数——photos `gen_photo` 同律，`(front, flash_auto, tick)` 唯一决定全像素）：后置 = 渐变天 + 落日漂移（三角波 ±w/8）+ 双山剪影（整数半平面族）+ 三流云（整数椭圆胶囊横漂回绕）+ 地面渐变；前置 = 室内渐变 + 面盘浮动（顶带暗化发影）+ 八枚波克光斑升腾。微动 = 帧节拍 ~10fps slab 重生成（230,400 px ≈ 0.9 MiB 堆置）+ `ui.invalidate(vf)`（paint_dirty + damage 双标记）——活取景器常驻 redraw 是取景器语义本义，每拍一次全幅 SUBMIT RPC 的功耗/带宽面如实记账（MOTION_STEP_NS=100ms 单源可调）。
- **三分网格线**（任务书口径，设计稿 §2.5 未列——取景辅助线）：2 竖 + 2 横 2px Rect，比例布局定位（Row/Col + `pattern::spacer` 三分拼接——零手写绝对坐标）；色取 theme `font_secondary` token（中灰双底可判）。纯装饰辅助线非卡片拼装，不涉 press/遮罩/圆角三禁区。
- **顶行控制带**（TOPBAR_H=168px 带）：左 `pattern::icon_button`(Flash) + 标题定宽列 160px（"Flash Auto"/"Flash Off" 循环切换）；右 `pattern::icon_button`(Front/Back) + 标题定宽列 80px（"Back"/"Front"）。切换即时生效于模拟景：Flash Auto = 补光提升（lerp 白 28/256 全帧预升——无闪光硬件的可判等效面）；Front/Back = 母题整体换景。切换路径 = on_tap 事件 → set_text 换标题 → slab 重生成 → invalidate → wake（同拍生效）。
- **底行控制带**（264px 带）：中置大圆快门 + 左侧瞬态行 "Saved n"（定宽列 288px——数字进位不推快门位）。快门 = 168px 程序化白环 Image（外环 r84..60 + 中点 r≤36，其余像素 = 底带色无缝；偶数边长圆心半像素 → 2× 坐标轴系四向精确对称）+ 200px 外扩按压暗框 chip。
- **快门三连**（Click 路由 = MsgBuf Click 三锚：环 Image / 内容 Col / 外扩框——photos 缩略格同形，Image 非 widget 无 on_tap 面）：①按压反馈 = 引擎 `set_press_map`（7 条 < PRESS_MAP_MAX=28；trigger = Fab/标题/快门三族锚 → visual = chip Rect；快门 168px Image 满盖中心 → 按压可见域 = 外扩 16px 暗框带——无 alpha 通道整面半透遮罩不可达，photos #4 同形）②全屏白 Rect 单帧特效 = 快门拍下 → 白 Rect 入树（根 Stack 末位盖全幅）→ 本帧呈现（SUBMIT→FRAME_CALLBACK 过链恰一帧）→ RELEASE 后即撤节点 + wake 恢复取景——设计稿 §2.5 明载的非拼卡特效、不走 press 三禁区 ③瞬态行 "Saved n" = set_text + 2s 帧循环节拍自撤（SAVED_NS=2s，app 侧 deadline 面驱动）。
- **CAMERA.JSN 持久化**：`{"schema":1,"count":<u32>,"last":<u64>}` canonical 形（默认 31B / 满档 59B 单簇；last = CLOCK_MONOTONIC ns——boot 期归零语义如实，跨 boot 比较无意义）——launcher layout_io 食谱自实现（appshell::conf 缺，§三 #5）：三约束纪律（缺席回默认 + 坏读自愈回写 + 写失败非致命 eprintln 常驻不死）+ 严格串行单 RPC 会话 + close 即 commit + `fs_h()==0` 结构性早退零 RPC。快门按下即存（计数 + 最近时戳）。
- **GUI 通道形态**（svcmgr GUI 授权链 5452649af）：本 app 带 fs 词 + [ui] 节 → gift 槽位 `[rootfs, fs, wm, input]`——wm 落 extras[2]（berry-libc io.rs GIFT_H[4]；`wm_cli_h()` 的 [0] 位对 wants_fs 程序承载 rootfs VMO，不可用），经同槽访问器 `berry_libc::io::wm_present_h` 取用（「同一 gift 位对不同程序承载不同对象」多产面惯例；与 FILES/PHOTOS 批同口径）。input 固定位 [3] = `input_h()` 不受影响。
- **帧循环/退出照 settings 先例**：wm 门控（0 = `camera: wm absent (no gui gift)` + park 常驻）→ WM_GETINFO → present GETINFO 轻断言 → 帧 VMO 三连@FRAME_VA=0x5100_0000 → 主循环（INPUT_POLL 批拉→push_pointer→`pump_input`（referee + press_react/widget_react 同拍）→MsgBuf 消费（快门 Click/DragEnd 暗框复位）→take_events（Flash/Flip 切换）→微动节拍/瞬态行自撤→render_frame→SUBMIT/FRAME_CALLBACK/RELEASE_WAIT 串行（在途 ≤1）→白闪撤收→vote 变化才发→wait_many n≤2 + deadline = min(next_wakeup, 微动下拍, 瞬态行到点, 10s IDLE 帽)）；damage 空集零提交零 RPC；无退出路径（fail = eprintln + park，崩溃监督归 svcmgr）；首帧回执 `camera: first frame`。RPC 半程族/`damage_envelope`/`input_arm` = settings/photos 同形拷贝（300s READABLE 帽、EBUSY 重臂、send 即 take）。
- **节点/登记预算**：树 ~40 节点 < MAX_NODES=512；press_map 7 条 < 28；a11y 7 条（vf/shutter/两标题行/瞬态行 + 两 Fab 配方自登记）< 32。

## 三、v1 形态 vs 终态差异（3b 依赖降解清单）

未落底座全景见 W4-RECON §四；本应用逐项：

| # | 3b/波 2 依赖（未落） | 本应用 v1 降解 | 终态形态（落齐后） |
|---|---|---|---|
| 1 | **berry-appshell crate**（rpc/pump/scene/icons/conf 五模块） | settings/photos 同款手拼 RPC 半程 + 自管帧循环（§二末条） | appshell::rpc/pump 收编帧循环骨架；conf 收编 JSN 食谱；scene 收编取景节拍面 |
| 2 | **WM_CREATE_F_IMMERSIVE**（flags 仅 AVOID_SYS=1<<0） | 单窗全幅近似：surface 全幅取景常驻 + 控制带自绘叠盖，无状态栏隐藏/避让/回显语义 | IMMERSIVE 沉浸态（状态栏隐/失焦回显链——设计稿验收句「沉浸态状态栏隐」挂此） |
| 3 | **appshell Toast**（窗内 Overlay） | 瞬态行 "Saved n"：底带定宽 Text + 2s 帧循环节拍自撤（app 侧 deadline 驱动）；设计稿「Saved 计数角标」持久位按任务书口径并入瞬态行（拍照时刻显示 Saved n 即计数） | appshell Toast 底部 2s 短驻留（节拍面换轨，文案与触发点不变） |
| 4 | **theme 样式族**（card_style/oh_press_bg 20% 黑遮罩预混/OH_PRESS_SCALE_MIDDLE_PERMIL=950——ui-wave2 独有未移植，W4-RECON §二末段） | 按压 = 引擎 press_map `PRESSED_BG`（0x404040 不透明换底）：快门呈现为外扩 16px 暗框带、控制钮呈现为钮栈域换底（无 alpha 通道 → 整面半透遮罩不可达）；**0.95 缩放档无等价面（缺失）** | oh_press_bg 20% 预混遮罩 + 0.95 中点缩放档（theme.rs 单源）——press_map 登记行原地换轨，chip 结构不变 |
| 5 | **appshell::conf 键族**（LAYOUT/BOOT 外 <NAME>.JSN 泛化） | CAMERA.JSN = launcher layout_io 食谱自实现（≤59B 单簇；三约束 + 串行单会话 + close 即 commit） | appshell::conf 键族读写（格式随 conf canonical 单源换轨） |
| 6 | **wm back 路由**（encode_wm_back_req 零消费）+ **根页优雅退出**（R3） | 单页应用无退出路径（settings park 先例同形）；无 back 手势消费面 | 系统 back = 根页语义直接优雅退出（exit 0 + 窗 DESTROY + BACK_ACK + 状态栏回显） |
| 7 | **Fab paint 恒绘 '+'**（paint.rs 不消费 FabProps.icon id；FAB_D=56px 固定档） | ①Flash/Front-Back 钮 = `pattern::icon_button`（56px accent 圆钮同形 '+'，无按压视觉——Fab 不在 press_map visual 白名单）——语义经 a11y 锚 + 标题行文案补偿；icon id 值域已立法（0x0142/0x0143）待 paint 消费 ②**大圆快门不适配 Fab 档**（56px 上限 + '+' 字形）→ 168px 程序化白环 Image 直载（photos #7 方形格不适配 card 配方的同款诚实偏差；快门艺术面非按压特效，按压反馈仍走引擎 press_map） | paint 按 icon id 绘字形（闪光/翻面 glyph）+ Fab 入 PressedVis 按压查表；快门大圆配方入 pattern 面（结构原地换轨） |
| 8 | **press_react 无 DragEnd 还原臂**（Release/Cancel 才还原——拖拽误触暗框卡死） | app 侧 DragEnd 复位：命中锚 → set_props(mask, `Tokens::bg_comp` 常态 token 色) + press_map 整表重登记（active/saved 归零）——photos #12 同款 | 引擎 press_react 补 DragEnd 还原臂（app 侧复位退役） |
| 9 | **引擎无通用 timer/调度 API**（next_wakeup 只反映引擎内动画） | 微动节拍/瞬态行自撤的 deadline = app 侧并入 wait_many deadline（min(引擎 wakeup, next_motion, saved_until, IDLE 帽)——TIMEDOUT 醒后重生成/自撤） | appshell::pump 节拍面收编（app 侧 deadline 拼接退役） |
| 10 | **无相机传感器**（无取景流/无成像面；v2 挂账） | 取景器 = 程序化确定性模拟景（状态行级诚实标注于报告与源码头注）；快门不产图档（BYI 编码器 + 卷容量预算挂 v2）；Flash Auto 无闪光硬件 → 补光提升等效面 | 真传感器取景流 + 成像入卷（PHOTOS 联动）+ 闪光硬件面 |
| 11 | **常驻 ~10fps 全幅 redraw**（活取景器语义本义；每拍一次 SUBMIT RPC） | MOTION_STEP_NS=100ms 单源常量（TCG 档观感/带宽折衷值）；静态降级路径在席（input 失效/无礼物 → 静态取景续活） | 终态真传感器帧率驱动 + present Vote 面协商（vote_hint 已在席透传） |

## 四、验证记录

- `cargo check --release --target aarch64-unknown-linux-musl`（berry-ld RUSTFLAGS 形，linker 绝对路径）：**过、零 warning**（多轮迭代后终态）。
- `cargo build --release`（同旗）：过——ELF **450,176 B ≈ 0.43 MiB** 静态链（< 稿估 ELF 0.65 MiB / .bap 0.75 MiB；对照 photos 457,376 B / settings v0 435KB——pattern/取景供件增量约 −7KB 对 photos，LTO 死码消除下可信）。未做 mkapp 打包/入镜像（接线批归口，裁决 B 前置铁律）。
- 宿主侧纯逻辑对拍（拷出 JSN 编解码/瞬态行文案/场景生成/快门环生成四纯函数族独立编跑）：JSN 默认形恰 31B 逐字节钉板（`{"schema":1,"count":0,"last":0}`）、满档 59B ≤ 64 帽、四组值往返全等、13 种坏形全拒（垃圾串/截断/尾随/schema 变体/前导零/负号/u32 域外/u64 域外/空域/多余键/超帽长读）；瞬态行文案钉板（`Saved 1`/`Saved 42`/`Saved 4294967295` ≤16B）；场景往返确定性（前后置双档）、微动可判（相邻 tick 差 304 px）、补光可判（on/off 全帧 230,400 px 差）、前后置互异、全不透明、非平涂（back sd=45.3 / front sd=33.0）、前置面盘中心像素 FACE 系、后置天顶冷色序；快门环四角/中心 = 底带色、环带四向中点 = 白、环隙 = 底带色、中心点 = 白、四向精确对称（2× 轴系修后）、白质量 14,928/28,224 px。RPC/帧循环面不可宿主机执行（berry-libc AArch64 专用同 settings/photos 注记）。
- 布局算术目检：OH phone 1080×1920 档——顶带行 48+160+744+80+48=1080 恰满（右钮贴右缘）；底带行 296+288+48+200+296=1080 恰满（快门组居中）；网格线 x=360/722、y=640/1282 三分位。800×600 调试档——顶带 flex=464、底带 flex=132、无溢出。
- 铁律执法门（§五）+ `git status` 足迹核验（仅 `user/bins/camera/` + 本报告）。

## 五、铁律合规自检

- **零手拼**：本应用零卡片零列表（相机 HUD 无卡面——grep `ViewKind::Card` 恰 0 处、`corner_radius` 零命中）；控制钮 = `pattern::icon_button`；骨架 = `pattern::stack`/`spacer`；按压 = 引擎 `set_press_map`/`press_react`（PRESSED_BG 引擎常量，应用源零引用）。grep 门：`scrim`/`0x404040`/`0xCCE3FF` 等按压字面量**零命中**（`0.95`/`950` 仅存于「未落降解」文档注释）；全屏白 Rect = 设计稿 §2.5 明载快门白闪特效（非拼卡、不走三禁区）；网格线 = 取景辅助装饰 Rect（非卡片）。
- **文案 ASCII**：全部 `b"..."` 字面量（20 件）与诊断行（`camera: ...` 21 件）经 LC_ALL=C 过滤非可印字符**零命中**（词汇表 Camera 域：Flash/Auto/Off/Front/Back/Saved + 通用 Back；a11y 标签 viewfinder/shutter/flash mode/camera face/saved count）。
- **帧循环/退出形态照先例**：settings/photos 同形拷贝（RPC 半程族/VMO 三连/SUBMIT 链/vote/wait_many/无退出路径 park）；首帧回执 `camera: first frame` 诊断面（console=false 走私有对）。
- **边界**：未触 launcher/guifbd/wmd/svcmgr/berry-* 基座/justfile/他批 crate 与报告；appstub/apps/CAMERA 旧资产未动（资产迁 crate 为任务书口径，旧副本退役归接线批）。

## 六、交接注意点（接线批）

1. **入镜像序**：裁决 B（boot 批 ui deferred）先行——本 crate 是常驻 park 形 GUI app，未经裁决 B 直入镜像 = boot 批 app_reap_tail 永等 → `APP ok` 缺失全线 smoke FAIL（W4-RECON §1.7 铁律）。
2. **打包**：justfile :328-337 七应用 appstub 循环退役后按 APPTOMB :315-327 食谱逐 crate 段（musl + berry-ld RUSTFLAGS **绝对路径** → staging bin/ + manifest + icons/views → mkapp）；资产源二选一钉单源（crate 内已在席——建议 appstub/apps/CAMERA 随循环整体退役）。`tools/rootfs-manifest.toml` 零改（条目名 app/CAMERA.bap 不变）；单包实测入账（估 0.75 MiB，ELF 实测 0.43 MiB 余量足）。
3. **manifest 合规**：fs 词 + [ui] 节 + [security] 三键齐备——mkapp 五道门禁/sign 面零改可过；svcmgr GUI 授权链现认 [ui] 节（裁决 A 换轨时 manifest 词面随批全族统一改）。
4. **运行面观察点**：①常驻 ~10fps 全幅 SUBMIT（活取景语义）——TCG 档首帧后持续 redraw 属预期，冒烟判读勿当死循环；②白闪单帧 = SUBMIT→FRAME_CALLBACK→撤节点序列，VNC 序列帧断言取拍照时刻连续两帧（白帧 + 恢复帧）；③CAMERA.JSN 的 last 为 CLOCK_MONOTONIC ns（boot 期归零——跨 boot 时戳不可比，值域合法即可）。
5. **3b 落齐后回归位**：§三表逐项换轨——IMMERSIVE 沉浸态（#2）、Toast 承载（#3）、oh_press_bg/0.95 档（#4）、back 优雅退出（#6）、Fab 字形（#7）五处为可见行为差异面，格结构与登记面已按原地换轨设计。
