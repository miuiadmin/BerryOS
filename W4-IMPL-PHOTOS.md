# W4-IMPL-PHOTOS —— 波 4 PHOTOS 应用实施报告

基线：worktree `wip/ui-wave4`（共同依据 = [W4-RECON.md](W4-RECON.md) + `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.4）。实施件 = `user/bins/photos/` 独立 crate（settings 垂直切片先例形态）；未触 launcher/guifbd/wmd/svcmgr/justfile/berry-* 基座 crate 与他批实施件（`user/bins/files/`、`user/bins/contacts/`、`W4-IMPL-FILES.md`、`W4-IMPL-CONTACTS.md` 在册未碰——`git status` 佐证）。验收门：`RUSTFLAGS="-C linker=tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" cargo check --release --manifest-path user/bins/photos/Cargo.toml --target aarch64-unknown-linux-musl` 过、零 warning；同旗 `cargo build` 过（ELF 457,376 B）。

## 一、改动清单（全部新增，无既有文件修改）

| 件 | 行数/尺寸 | 说明 |
|---|---|---|
| `user/bins/photos/Cargo.toml` | 48 行 | settings 镜像（空 [workspace] escape、字面 version=0.2.0、deps 五件 = berry-libc/sys/guiproto/ui/gfx、release z/lto/cu1/abort/strip） |
| `user/bins/photos/manifest.toml` | 23 行 | id=PHOTOS / entry=bin/photos / console=false / **fs = ["rootfs"]**（PHOTOS.JSN 持久化授权词——APPDEMO fs 词先例形态）/ stack_pages=64（settings 穿页先例同值）/ [security] 三键 / [ui] 三键 |
| `user/bins/photos/src/main.rs` | 1512 行 | 应用本体（下节形态）：程序化样张供件 + PHOTOS.JSN 持久化 + 网格页/看片页 + settings 镜像帧循环 |
| `user/bins/photos/icons/icon.png` | 546 B | 自 `bins/appstub/apps/PHOTOS/icons/icon.png` 拷入（mkapp 编码 .byi 的 PNG 源——「源不入包」管道） |
| `user/bins/photos/views/main.bui` | 52 B | 占位 view（同 stub 形） |
| `user/bins/photos/Cargo.lock` | 自动 | cargo check 产物（settings 同款在册） |

crate 内资产 = PHOTOS 资产新单源（任务书「manifest.toml + src/main.rs + icons」口径）；`bins/appstub/apps/PHOTOS/` 旧副本留待接线批随 appstub 七应用循环整体退役（justfile :328-337 现仍 cp appstub——本批不碰 justfile，接线序归整合批 + 裁决 B 前置铁律 W4-RECON §1.7/§5.4）。

## 二、v1 实现形态（对照设计稿 §2.4 页面树）

- **网格页**：`pattern::top_bar`(Photos) + `pattern::seg`(All/Favorites，pattern 配方) + 三列缩略网格（Grid kind 行主序、gap 8vp、cell = 宽向 (vp_w−2·16vp−2·8vp)/3 与纵向可用高取小——恒一屏纳，见 §三 #10）+ 诚实标注行（`N photos - sample art` / `N favorites - sample art` / `No favorites yet` / `No photos left`，S16x32 次级色）。网格整树 = 过滤/删除后 remove + 重建 + press_map 整表重登记。
- **缩略格**：Card kind 直载（elevation 0、corner_radius = `pattern::LISTITEM_RADIUS` 8vp@3x 常量单源——`pattern::card` 配方是 48vp 行卡不适配方形缩略格，诚实偏差见 §三 #7）> Stack > [按压暗框层 Rect（常态 = `theme::Tokens::bg_comp` 与卡面同色无缝）, 内容 Col(cross=Center) > [spacer, Image(SmoothArgb 缩放), spacer]]。Image `res` = 0x2100+序 的样张 id。
- **按压反馈**：引擎 `set_press_map`（trigger = 格 Image + 内容 Col 双锚 → visual = 暗框 Rect；≤2×12=24 条 ≤ PRESS_MAP_MAX=28）——Press 经引擎 `press_react` 换 `PRESSED_BG`（0x404040 换底），呈现为缩略图四周 4vp 边带暗框（v1 无 alpha 通道整面遮罩不可达，见 §三 #4）；app 源零 press/遮罩/圆角字面量（§五执法门过）。拖拽误触（slop 外抬手走 DragEnd、引擎无还原臂）的暗框卡死 = app 侧复位常态 token 色 + press_map 重登记（§三 #12）。
- **看片页**（点缩略懒建、Back 退出整树 remove、再点重建——「首点懒建、退出 DESTROY、复用」的单窗近似）：根 Stack > [整幅底 Rect(暗档 bg_page), 内容 Col] 叠于网格页上（后子盖前）+ 顶行 icon_button(Back/Star/Delete)（`pattern::icon_button` 配方；任务书口径三键全在顶行——设计稿 §2.4 原文为顶行 Back + 底行 Star/Delete，从任务书）+ 页位 "n / N" + 收藏态 "Starred"/空 文案 + `SwipePager` 翻片区（attach 建 track+圆点行；全幅 Image = big = min(vp_w, vp_h−顶行−圆点−边距) 居中）。>8 张 = 8 页滑窗惰性换绑（§三 #9）：落末页前推窗右移一回钉倒数第二页、落首页回溯窗左移一钉第二页——照片视觉连续、圆点示窗内位。
- **样张（12 张程序化图案）**：启动期一次生成 240×240 ARGB 像素板（12×240²×4B ≈ 2.7 MiB 堆置）经 `IconSource` 真供件注册（launcher `LauncherIcons` 先例；`smooth_scale` 置位 → SmoothArgb 双线性缩放——壁纸同律）。确定性纯整数生成：3 色板 × 4 母题（同心环/斜纹/棋盘/落日）= 12 组合恰各不相同；`i` 唯一决定全像素。宿主侧对拍：往返确定性、12 张互异、全不透明、对比度 sd 68–78、落日母题底部暗化带与设计一致。如实标注无相册真源（状态行 + Camera 联动挂 v2）。
- **PHOTOS.JSN 持久化**：`{"schema":1,"del":"..12..","fav":"..12.."}` 位串形恰 56B 单簇（launcher layout_io 食谱自实现——appshell::conf 缺，§三 #5）：三约束纪律（缺席回默认 + 坏读自愈回写 + 写失败非致命 eprintln 常驻不死）+ 严格串行单 RPC 会话 + close 即 commit（TRUNC 游标）+ `fs_h()==0` 结构性早退零 RPC。Star 切收藏位/Delete 置删除位即存。
- **GUI 通道形态**（svcmgr GUI 授权链 5452649af）：本 app 带 fs 词 + [ui] 节 → gift 槽位 `[rootfs, fs, wm, input]`——wm 落 extras[2]（berry-libc io.rs GIFT_H[4]；`wm_cli_h()` 的 [0] 位对 wants_fs 程序承载 rootfs VMO，不可用），经同槽访问器 `berry_libc::io::wm_present_h` 取用（「同一 gift 位对不同程序承载不同对象」多产面惯例——extras[2] 对 gui-wmd 承载 present 客户端端、对本 app 承载 wm 客户端端；与 FILES 批同口径）。input 固定位 [3] = `input_h()` 不受影响。settings（无 fs 词）wm 落 [0] 走 `wm_cli_h`——两形分野单源 = svcmgr app_launch。
- **帧循环/退出照 settings 先例**：wm 门控（0 = `photos: wm absent (no gui gift)` + park 常驻）→ WM_GETINFO → present GETINFO 轻断言 → 帧 VMO 三连@FRAME_VA=0x5100_0000 → 主循环（INPUT_POLL 批拉→push_pointer→`pump_input`（referee + press_react/widget_react 同拍）→MsgBuf 消费（SwipePager on_msg/Seg 路由/格点击/DragEnd 复位）→take_events（三 icon_button on_tap）→pager.tick→render_frame→SUBMIT/FRAME_CALLBACK/RELEASE_WAIT 串行（在途 ≤1）→vote 变化才发→wait_many n≤2 + 10s IDLE 帽）；damage 空集零提交零 RPC；无退出路径（fail = eprintln + park，崩溃监督归 svcmgr）；首帧回执 `photos: first frame`。RPC 半程族/`damage_envelope`/`input_arm` = settings 同形拷贝（300s READABLE 帽、EBUSY 重臂、send 即 take）。
- **节点/登记预算**：网格页 ~110 + 看片页 ~45 < MAX_NODES=512；press_map ≤24 < 28；a11y 尽力面（表满跳过不阻断——引擎 32 帽）。

## 三、v1 形态 vs 终态差异（3b 依赖降解清单）

未落底座全景见 W4-RECON §四；本应用逐项：

| # | 3b/波 2 依赖（未落） | 本应用 v1 降解 | 终态形态（落齐后） |
|---|---|---|---|
| 1 | **berry-appshell crate**（rpc/pump/scene/icons/conf 五模块） | settings 同款手拼 RPC 半程 + 自管帧循环（§二末条） | appshell::rpc/pump 收编帧循环骨架；conf 收编 JSN 食谱 |
| 2 | **wm 礼物槽位无专用访问器**（svcmgr :6154-6158 自认「wm 落 [2] 暂无 app 侧访问器 = 裁决 A 位」） | 经同槽访问器 `wm_present_h` 取 extras[2] 的 wm 客户端端（多产面惯例——gui-wmd 同槽承载 present 客户端端；与 FILES 批同口径，零基座改动） | 裁决 A（`ui=["wm","input"]` 能力词 + appshell crt0 位序立法）落地换轨专用访问器，本行一处替换 |
| 3 | **WM_CREATE_F_IMMERSIVE**（flags 仅 AVOID_SYS=1<<0） | 看片窗 = 单窗全幅近似：v1 单 surface 全幅模型下看片页整幅自绘底叠于网格页上（Stack 后子盖前），无状态栏隐藏/避让/回显语义 | 独立看片窗 WM_CREATE_F_IMMERSIVE 沉浸态 + 失焦/退场状态栏回显链（设计稿验收句「状态栏隐/回显」挂此） |
| 4 | **theme 样式族**（card_style/oh_press_bg 20% 黑遮罩预混/OH_PRESS_SCALE_MIDDLE_PERMIL=950——ui-wave2 独有未移植，W4-RECON §二末段） | 按压 = 引擎 press_map `PRESSED_BG`（0x404040 不透明换底）：呈现为缩略图四周 4vp 暗框（无 alpha 通道 → 整面半透遮罩不可达）；**0.95 缩放档无等价面（缺失）**；卡片面/圆角 = Card kind 自绘 + LISTITEM_RADIUS 常量单源 | oh_press_bg 20% 预混遮罩 + 0.95 中点缩放档（theme.rs 单源）——press_map 登记行原地换轨，格结构不变 |
| 5 | **appshell::conf 键族**（LAYOUT/BOOT 外 <NAME>.JSN 泛化） | PHOTOS.JSN = launcher layout_io 食谱自实现（位串形 ≤64B 单簇；三约束 + 串行单会话 + close 即 commit） | appshell::conf 键族读写（格式随 conf canonical 单源换轨） |
| 6 | **wm back 路由**（encode_wm_back_req 零消费）+ **根页优雅退出**（R3） | Back = 看片页内 icon_button（关看片页回网格）；主窗无退出路径（settings park 先例同形） | 系统 back 全链 pop（看片窗 DESTROY + BACK_ACK 焦点回主窗）+ 主窗根页 back 优雅退出（exit 0 + 窗 DESTROY） |
| 7 | **pattern::card 配方形态**（48vp 行卡 = list_item 同源，非方形） | 缩略格 = Card kind 直载 + `LISTITEM_RADIUS` 常量（与 list_item 同源同值——圆角单源保持；非手写 Rect 拼卡） | card_style 样式族落地后方形格配方入 pattern 面（格结构原地换轨） |
| 8 | **Fab paint 恒绘 '+'**（paint.rs 不消费 FabProps.icon id——icon_button 三键同形 '+' 圆钮、无按压视觉（Fab 不在 press_map visual 白名单与 PressedVis 表）） | 三键语义经 a11y 锚（viewer back/star/delete）+ "Starred" 态文案补偿；icon id 值域已立法（0x0140/0x0141）待 paint 消费 | paint 按 icon id 绘字形（返回箭头/星/垃圾桶）+ Fab 入 PressedVis 按压查表 |
| 9 | **SwipePager SWIPE_PAGES_MAX=8**（swipe.rs 帽） | 12 张全量档 = 8 页滑窗惰性换绑（越界侧移一页 + 整组 Image remove/add + snap_to 回钉；照片视觉连续，圆点示窗内位） | 帽内直载（相册真源接入后分页模型随 appshell::scene 重裁） |
| 10 | **Scroll kind 无手势控制器**（无自动滚面——offset 纯 props 域） | 网格 cell = 宽向/纵向可用高取小（下限 8vp 微缩格）恒一屏纳；OH phone 1080×1920 档 cell=312px 方格、四行 1688px < 1920 | Scroll/List 手势滚动面（网格超一屏纳长列表） |
| 11 | **pattern::seg 点击锚域**（Cell::hit 只认 bg/label 锚——col 垫域/spacer 域点击 miss） | Seg 路由 = Click 走 `Seg::hit` 锚 + Release 坐标域兜底（seg frame bounds 二等分）双路 | pattern::seg hit 锚域扩全格（或 Seg kind 引擎 on_change 直载） |
| 12 | **press_react 无 DragEnd 还原臂**（Release/Cancel 才还原——拖拽误触暗框卡死） | app 侧 DragEnd 复位：命中格锚 → set_props(mask, `Tokens::bg_comp` 常态 token 色) + press_map 整表重登记（active/saved 归零） | 引擎 press_react 补 DragEnd 还原臂（app 侧复位退役） |
| 13 | **相机卷真源**（无照片档案面；v2 挂账） | 12 张程序化样张 + 状态行如实标注「sample art」；收藏/删除位真存 PHOTOS.JSN | Camera 产物入卷（BYI 编码器 + 卷容量预算，v2）+ 真相册枚举 |

## 四、验证记录

- `cargo check --release --target aarch64-unknown-linux-musl`（berry-ld RUSTFLAGS 形）：**过、零 warning**（多轮迭代后终态）。
- `cargo build --release`（同旗）：过——ELF **457,376 B ≈ 0.44 MiB** 静态链（< 稿估 ELF 0.70 MiB / .bap 0.80 MiB；对照 settings v0 435KB——pattern/swipe/seg/press_map/样张生成增量约 +22KB，LTO 死码消除下可信）。未做 mkapp 打包/入镜像（接线批归口，裁决 B 前置铁律）。
- 宿主侧纯逻辑对拍（拷出 JSN 编解码/过滤/文案/样张生成四纯函数族独立编跑）：JSN 编码恰 56B、往返全等、坏形拒（schema 变体/垃圾串/短位串/长读）、默认形逐字节钉板；过滤（全量 12/收藏 2 删位交错/全删空）全对；文案字节钉板（`12 / 12`、`1 / 8`、`12 photos - sample art`、`No favorites yet`）；样张往返确定性、12 张互异、全不透明。RPC/帧循环面不可宿主机执行（berry-libc AArch64 专用同 settings 注记）。
- 样张数值目检：每张亮度 sd 68–78（非平涂）、落日母题（i%4==3）底部 50 行亮度较顶带显著压暗（暗化地平带生效）、渐变底自左上至右下单调增亮。
- 铁律执法门（§五）+ `git status` 足迹核验（仅 `user/bins/photos/` + 本报告）。

## 五、铁律合规自检

- **零手拼**：卡片本体 = Card kind（grep `ViewKind::Card` 恰 1 处 = 网格格）；条目/分段 = `pattern::seg`；骨架 = `pattern::top_bar`/`stack`/`spacer`/`icon_button`；按压 = 引擎 `set_press_map`/`press_react`（PRESSED_BG 引擎常量）。grep 门：`scrim`/`0.95`/`950`/`0x404040`/`0xCCE3FF` 预混字面量**零命中**；`corner_radius` 唯一取 `LISTITEM_RADIUS` 常量（无数值）。
- **文案 ASCII**：全部 `b"..."` 字面量经 LC_ALL=C 过滤非可印字符**零命中**（词汇表内词：Photos/All/Favorites/Starred/photo/sample art/No favorites yet/No photos left/n / N）。
- **帧循环/退出形态照先例**：settings 同形拷贝（RPC 半程族/VMO 三连/SUBMIT 链/vote/wait_many/IDLE 帽/无退出路径 park）；首帧回执 `photos: first frame` 诊断面（console=false 走私有对）。
- **边界**：未触 launcher/guifbd/wmd/svcmgr/berry-* 基座/他批 crate 与报告；appstub/apps/PHOTOS 旧资产未动（资产迁 crate 为任务书口径，旧副本退役归接线批）。

## 六、交接注意点（接线批）

1. **入镜像序**：裁决 B（boot 批 ui deferred）先行——本 crate 是常驻 park 形 GUI app，未经裁决 B 直入镜像 = boot 批 app_reap_tail 永等 → `APP ok` 缺失全线 smoke FAIL（W4-RECON §1.7 铁律）。
2. **打包**：justfile :328-337 七应用 appstub 循环退役后按 APPTOMB :315-327 食谱逐 crate 段（musl + berry-ld RUSTFLAGS → staging bin/ + manifest + icons/views → mkapp）；资产源二选一钉单源（crate 内已在席——建议 appstub/apps/PHOTOS 随循环整体退役）。`tools/rootfs-manifest.toml` 零改（条目名 app/PHOTOS.bap 不变）；单包实测入账（估 0.80 MiB，ELF 实测 0.44 MiB 余量足）。
3. **manifest 合规**：fs 词 + [ui] 节 + [security] 三键齐备——mkapp 五道门禁/sign 面零改可过；svcmgr GUI 授权链现认 [ui] 节（裁决 A 换轨时 manifest 词面随批全族统一改）。
4. **两个 v1 交互语义待 3b 后回归**：看片页 Back 无系统 back 链（#6）、按压无 0.95 缩放（#4）——终态落齐后按 §三表逐项换轨，格结构与登记面已按原地换轨设计。
