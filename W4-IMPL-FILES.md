# W4-IMPL-FILES —— 波 4 FILES 应用实施报告

基线：worktree `wip/ui-wave4`（共同依据 = [W4-RECON.md](W4-RECON.md) + `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.2）。实施件 = `user/bins/files/` 独立 crate（settings 垂直切片先例形态）；未触 launcher/guifbd/wmd/svcmgr/其他应用 crate/justfile/berry-* 基座 crate（他批 `user/bins/contacts/`、`user/bins/photos/` 在册未碰）。验收门：`cargo check --manifest-path user/bins/files/Cargo.toml --target aarch64-unknown-linux-musl`（含 `--tests`）双过，零 warning。

## 一、改动清单（全部新增，无既有文件修改）

| 件 | 行数/尺寸 | 说明 |
|---|---|---|
| `user/bins/files/Cargo.toml` | 58 行 | settings 镜像（空 [workspace] escape、字面 version=0.2.0、release z/lto/cu1/abort/strip）；deps = settings 五件 + **berry-fsclient**（READDIR/STAT/STATFS 直面 + 帽常量/TYPE_DIR 单源再出口） |
| `user/bins/files/manifest.toml` | 23 行 | id=FILES / entry=bin/files / console=false / **fs = ["rootfs"]**（data 根 fsclient 真读的授权词——APPDEMO fs 词先例形态）/ stack_pages=64 / security 三键 / [ui] 三键 |
| `user/bins/files/src/main.rs` | 1749 行 | 应用本体（下节形态）；内联纯函数单测 4 件（wire 路径/排序/词表/十进制） |
| `user/bins/files/icons/icon.png` | 546 B | 自 `bins/appstub/apps/FILES/icons/icon.png` 拷入（mkapp 编码 .byi 的 PNG 源——「源不入包」） |
| `user/bins/files/views/main.bui` | 52 B | 占位 view（同 stub 形） |
| `user/bins/files/Cargo.lock` | 自动 | cargo check 产物 |

crate 内资产 = FILES 资产新单源（任务书「manifest.toml + src/main.rs + icons」口径）；`bins/appstub/apps/FILES/` 旧副本留待接线批随 appstub 循环整体退役（justfile :328-337 现仍 cp appstub——本批不碰 justfile，接线序归整合批，W4-RECON §5.4）。

## 二、v1 实现形态（对照设计稿 §2.2 页面树）

- **双根页**：TopBar（`pattern::top_bar` 配方，title=Files）+ 两行 `pattern::list_item`（apps / data，DIR 角标）。首帧零 fs RPC（fs 首触在进目录时）。
- **面包屑**：`pattern::chip` 逐级（Files > apps/data > seg…，当前层 selected；标签截 8B）；chip 点按跳级（`widget_react` on_tap 事件 → 层截断 + 重建）；Up 一级 = 面包屑行首 Button kind「Up」（on_tap + engine press_map/PressedVis 按压视觉）。「TopBar 右 icon_button=Up」的承载位偏差：top_bar 动作位 = Icon kind，IconSource 未注册 = 空绘不可见（pattern icons::BACK 同因），故 Up 承载于面包屑行首（可见 + 可按）。
- **Search（v1 固定过滤词表降解）**：过滤行 = `pattern::chip`（all + ≤6 字母）；词表 = 当前目录条目首字母去重升序确定性派生（大小写归一），点选即前缀过滤，过滤零命中显示「no match」空态。零敲键（softimed 缺席），词表随目录重建。
- **Scroll[List]**：Scroll kind（offset_y/viewport clip = 引擎面）+ `pattern::list_item` 行；**目录在前、名 bytewise 升序**（类型 = 逐条 STAT 真值，非 8.3 猜测）；行 = 名（截 20B）+ 类型角标（DIR/FILE）。拖滚 = app 面 DragStart/DragMove 跟手步进（引擎无自动滚面——offset 纯 props 域），clamp 到 content_h−view_h，导航/过滤即归零。
- **/apps 树**：真 READDIR("apps")；v1 launcher 预建 mkdir 未落（波 3 §2.2 / W4-RECON §三）→ Noent 如实显示「apps tree absent」空态；预建落则九目录真枚举自然点亮，零改码。
- **data 根 = fsclient 真读 FAT12**：`berry_fsclient::readdir("")`（空 wire = fsmgr 根 = RAM+FAT overlay 全景，C-1 刀 5 立法）+ 逐名 `stat` 定型 + `statfs` 空闲真值（首次进 data 根一次缓存，状态行 `items n / free m KiB`）。严格串行单 RPC 会话（fsclient 会话机内建：txid 匹配/毒化位/90s·10s 双帽）；失败非致命（layout_io 三约束纪律）——fs 礼物缺席「fs not available」/会话毒化「fs error」空态如实，页面常驻不死。子目录链 = 层进 READDIR(segs)，深帽 PATH_DEPTH_MAX−1 + PATH_MAX 客户端预拒。
- **文件条目只读详情**：v1 应用内瞬态行（Dialog 降解）——详情文本 `NAME - file - N B`（stat 尺寸真值），驻留 2.5s 帧循环节拍自撤（deadline 并入稳态等待）。
- **零手拼执法**：卡片/条目 = `pattern::list_item`（Card kind）；chip = `pattern::chip`；骨架 = `pattern::top_bar`/`stack`/`spacer`；行按压反馈色单源 = `pattern::ItemState` 五态预混表（Pressed=0xCCE3FF）落在 Stack 面层 Rect（pattern app_bar/top_bar 同款「面层 w/h None」叠层习语——非手拼卡片，卡片本体仍是 Card kind）；Up 按压 = engine press_map + paint::PressedVis 查表。文案全 ASCII。
- **帧循环/退出照 settings 先例**：wm 门控（0 = `files: wm absent (no gui gift)` + park 常驻）→ WM_GETINFO → present GETINFO 轻断言 → 帧 VMO 三连@FRAME_VA → 主循环（INPUT_POLL 批拉→push_pointer→pump_input→Msg 消费→take_events→render_frame→SUBMIT/FRAME_CALLBACK/RELEASE_WAIT 串行→vote 变化才发→wait_many n≤2 + IDLE 帽）；无退出路径（fail = eprintln + park，崩溃监督归 svcmgr）；首帧回执 `files: first frame`。
- **节点预算**：骨架 ~45 + 行 ×11 ×24 帽 ≈ 327 < MAX_NODES=512；a11y 登记行名（ROLE_LIST_ITEM）+ 稳定锚，子树 remove 连带清理（引擎 D12 面）零泄漏。

## 三、v1 形态 vs 终态差异（3b 依赖降解清单）

未落底座（W4-RECON §四 全景）与本应用逐项降解：

| # | 3b/appshell 依赖（未落） | 本应用 v1 降解 | 终态形态（落齐后） |
|---|---|---|---|
| 1 | **berry-appshell crate**（rpc/pump/scene/icons/conf 五模块） | settings 同款手拼 RPC 半程 + 自管帧循环（本文件 §二末条） | appshell::rpc/pump 收编，帧循环食谱退化为例行骨架 |
| 2 | **wm 礼物槽位无专用访问器**（wants_fs GUI app 的 wm 落 extras[2]=GIFT_H[4]；svcmgr :6154-6158 自认「暂无 app 侧访问器 = 波 4 appshell 裁决 A 位」） | 经同槽访问器 `berry_libc::io::wm_present_h()` 取 wm 客户端端（io.rs「同一 gift 位对不同程序承载不同对象」多产面惯例——extras[2] 对 gui-wmd 承载 present 客户端端、对本 app 承载 wm 客户端端；零基座改动） | 裁决 A（`ui=["wm","input"]` 能力词 + appshell crt0 位序立法）落地换轨专用访问器，本行一处替换 |
| 3 | **softime/软键盘**（OP_INPUT_INJECT 无消费者） | Search = 固定过滤词表 chip 前缀过滤（§二） | softimed 敲键回显 + 前缀过滤保 chip 快捷 |
| 4 | **appshell Dialog/Overlay** | 文件详情 = 应用内瞬态行 2.5s 自撤 | appshell Dialog 窗内 Overlay（大小/Confirm 只读展示原设计形） |
| 5 | **wm back 路由**（encode_wm_back_req 零消费） | 回退导航 = 面包屑 chip 跳级 + Up 按钮（无系统 back 消费面） | 系统 back 全链 pop（根页 back 优雅退出 = exit 0 + DESTROY + BACK_ACK，波 3 R3） |
| 6 | **launcher /apps 预建 mkdir**（波 3 §2.2） | /apps READDIR Noent → 「apps tree absent」空态如实 | APP_LIST 真流预建九目录，真枚举零改码点亮；v2 = .bap 增删推送真文件树 |
| 7 | **异步 fs 面**（appshell 无异步 IO） | 目录装载同步阻塞帧循环（fsclient 90s 帽 worst = TCG FAT 链走查；同簇缓存后续 STAT 快） | 异步 fs + 骨架屏/渐进填充 |
| 8 | **NavHost 推拉动画消费** | 页面切换 = 即时重建（无转场动画；单页自管状态机） | NavHost 值面页序 + 300ms 推拉（件在册——`pattern::NavHost` 可直接升装） |
| 9 | theme 样式族（card_style/oh_press_bg/OH_PRESS_SCALE——主树未落，W4-RECON §二末段） | 等价面：ItemState 五态预混表（pattern 单源）+ Card kind 自绘面 | 样式族落地后按压/缩放随迁单源 |
| 10 | TopBar 尾随动作位 icon_button（IconSource 注册面 = D28 接线刀未落） | Up 承载于面包屑行首 Button kind（可见可按）；top_bar 动作位空置 | 图标字形注册后 Up 迁回 TopBar 右位（icon_button 配方） |

另：滚动无惯性/fling（跟手步进即停）；STAT 定型失败条目诚实降级按 FILE 角标（列名已证在场，不丢行）；行帽 24（超帽状态行 `items n of total` 如实）。

## 四、验证与交接

- `cargo check --manifest-path user/bins/files/Cargo.toml --target aarch64-unknown-linux-musl` → Finished 零 error 零 warning；`--tests` 同过（单测为纯函数面——wire 路径拼段/目录排序/过滤词表/十进制；host 运行不可行：bin 链 berry-libc 的 aarch64 ELF 专属 asm（`:lo12:` fixup 在 Mach-O 宿主炸），执行面归 QEMU/镜像冒烟）。
- 接线批（整合批）待办：justfile :328-337 七应用循环对 FILES 换 APPTOMB 食谱（musl 构建 + staging cp 本 crate manifest/ELF/icons/views + mkapp；循环 spec 已含 `FILES:files` 映射）；**裁决 B（boot 批 ui deferred）先行**——否则常驻 park 形入镜像即挂死 boot（W4-RECON §1.7 铁律）；rootfs-manifest.toml 零改（条目名 `app/FILES.bap` 不变）；smoke APP/APPUI 计数断言随动判读。
- 运行时诊断面（console=false 私有对）：`files: wm v=… input=… fs=…`（起握手）/ `files: first frame`（首帧呈现回执）/ `files: <what>`（fail+park 收口）。
