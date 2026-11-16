# W4-IMPL-MMS —— 波 4 MMS（messages）应用实施报告

基线：worktree `wip/ui-wave4`（共同依据 = 本目录 [W4-RECON.md](W4-RECON.md) + `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.6；行号以 RECON 现场直数为准）。实施件 = `user/bins/messages/` 独立 crate（settings/contacts 垂直切片先例形态）；未触 launcher/guifbd/wmd/svcmgr/justfile/berry-* 基座 crate 与他批实施文件（`user/bins/{contacts,files,photos,browser,camera}/` 在册未碰——browser/camera 为并行批新增）。验收门：`cargo check --manifest-path user/bins/messages/Cargo.toml --target aarch64-unknown-linux-musl`（含 `--tests`）双过，**零 error 零 warning（2026-11-15 现场）**。

任务书四件（MMS）：会话 List（气泡行 Card 变体）+ 输入区；v1 输入 = 固定快捷短语 Chip 组选择发送（真 typing 挂 softimed）+ 草稿展示；发送入列 + 瞬态行反馈。**全部落地**（含 RECON §四 MMS 行的 Input 值面草稿展示、对端本地节拍模拟回复、会话 NavHost 同 Contacts 自管三项降解面）。

## 一、改动清单（全部新增，无既有文件修改）

| 件 | 尺寸 | 说明 |
|---|---|---|
| `user/bins/messages/Cargo.toml` | 37 行 | settings/contacts 镜像（空 [workspace] escape、字面 version=0.2.0、五 deps = berry-libc/sys/guiproto/ui/gfx、release z/lto/cu1/abort/strip） |
| `user/bins/messages/manifest.toml` | 22 行 | id=MMS / entry=bin/messages（bin 名与 entry 一致——RECON §六）/ label=Messages / console=false / stack_pages=64（真 GUI 链 8 页穿先例 ×8）/ security 三键 / [ui] 三键。**无 fs 词**（v1 内存态零持久化——MMS.JSN 挂终态） |
| `user/bins/messages/src/main.rs` | 1916 行 | 应用本体（§二形态）；内联纯函数单测 5 件（状态文案映射/事件 ID 解码/气泡宽估值/摘要装配/返回 glyph 形状） |
| `user/bins/messages/icons/icon.png` | 6442 B | 自 `bins/appstub/apps/MMS/icons/icon.png` 拷入（mkapp 编码 .byi 的 PNG 源——「源不入包」） |
| `user/bins/messages/views/main.bui` | 52 B | 占位 view（同 stub 形） |
| `user/bins/messages/Cargo.lock` | 1387 B | cargo check 产物 |

crate 内资产 = MMS 资产新单源（任务书「manifest.toml + src/main.rs + icons」口径，files/photos 同款）；`bins/appstub/apps/MMS/` 旧副本留待接线批随 appstub 循环整体退役（justfile :328-337 现仍 cp appstub——本批不碰 justfile，接线序归整合批，W4-RECON §5.4）。

## 二、v1 实现形态（对照设计稿 §2.6 页面树）

```text
窗（全幅单面呈现——settings/launcher 同形，surface_id=0）
└─ NavHost（pattern.rs 页栈状态机；根页不 push = depth 0 根语义；会话 NavHost 自管——contacts 同形）
   ├─ [根] 列表页 Stack（pattern::stack）
   │  ├─ 滚动层 Scroll（app 自管 offset：DragStart/DragMove 纵拖 → set_props 换 offset_y，
   │  │   clamp [0, content_h − vp_h] 直钳无橡皮筋）
   │  │  └─ 内容 Col[顶让位 168px, list_item × 3（首字母头像位 + Name/末条摘要（Me: 前缀
   │  │      诚实标注侧别）双行 + Draft 角标）, 尾留白]
   │  └─ 铬层 Col（后画盖上 = Scroll v1 无 clip 的滚动溢出遮盖面）
   │     └─ app_bar(title=Messages, back=false)
   └─ [推] 会话页 Stack（NavHost.push 300ms 右滑入；z 顶覆盖；不透明页底 Rect(bg_page)）
      ├─ 滚动层 Scroll（气泡列 + 顶/尾让位）
      │  └─ 内容 Col[顶让位, 气泡行 × n（Row[侧别推位, 定宽 Col{card}]:
      │      对方 = 左贴 Card(elev 1)/font_primary 单行；Me = 右贴 Card(elev 2)/brand 正文
      │      + Sending→Sent→Delivered 状态次行), 尾让位 = 输入区高 + 呼吸位]
      └─ 铬层 Col（后画盖上）
         ├─ app_bar(title=对端名, back=true → pop)
         ├─ 推位（vp_h − TOPBAR_H − 输入区高）
         └─ 输入区 Stack[bg_page 面层, Col[divider, hint 瞬态行(空文隐藏),
            chips 行(pattern::chip × 7——点选即草稿/再击取消),
            草稿行[Input kind 200×48 值面（set_input_text 可显不可敲）, 推位,
            Send Button(Filled)]]]
```

- **行命中**：`Msg::Release` 携坐标 → `ui.bounds` 行卡界包含判定（Card 子树全幅——pattern 内部节点 id 不外露的补偿面；Click 无坐标面故走 Release；slop 内非拖 up 才有 Release，拖拽路径自然豁免误触）。
- **行按压反馈**：`pattern::ItemState::Pressed.bg()`（brand 20% 预混白）app 侧 set_props 换头像衬底（引擎 press_react 同形：Press 换底 / Release·Cancel·DragStart 还原）；色值零字面量。Send 按压 = 引擎 PressedVis 查表（press_map 1 枚——Button 在 visual 白名单）；Chip 点选反馈 = kind selected 色变（ACCENT 底白字——引擎自绘，不入 press_map）。
- **草稿闭环**：chip 点选（引擎 widget_react 翻转 selected + on_tap value=位）→ 前一枚还原 + `set_input_text` 值面换文（Input kind 值面——RECON §四「值面可显不可敲」裁定形态）→ Send 或再击取消；退出会话草稿暂存 → 列表行 Draft 角标（brand 衬底白字 mini 标）回显 → 重进会话 chip 选中态 + 值面双回显。
- **发送入列 + 瞬态行反馈**：Send → Me 气泡入列（滚底）+ 状态次行 Sending →（帧循环节拍 1.2s）Sent →（再 1.8s）Delivered；新发送将在途条目直落终态（单在途简化，数据/节点双面同步）。无草稿 Send = hint 瞬态行引导（2s 自撤）不空转。
- **对端本地模拟回复**：发送后 4.5s 节拍入列轮换回复（REPLIES ×4 轮换，Me/对方侧别诚实标注；无 modem 仅 UI 反馈）。节拍跨 pop 存活（数据面入列——重进会话可见）。
- **会话返回**：AppBar 返回 glyph（app 内手势位——程序化 24px「<」双臂位图 ×3 最近邻 = 72px，IconSource 真供件，theme token 打包单源；contacts BACK 同技法独立实现）→ nav.pop 300ms 左滑出 → take_popped 移交 remove + 列表行集重建（Draft 角标/末条摘要刷新，滚动位保留）。
- **滚动**：双页各自 DragY 累进（列表页 y ≥ TOPBAR_H 门控；会话页 TOPBAR_H < y < vp_h − 输入区高门控 = 铬区不滚）；开页/新消息即滚底（最新在底）。
- **零手拼执法**：条目/气泡一律 `pattern::list_item`/`pattern::card`（Card kind 自绘卡面/阴影/8vp 圆角，各 3 处消费）；骨架 = `pattern::app_bar/stack/spacer/divider`（2/3/12/1 处）；文案全 ASCII（python 全字面量扫描过）；`ViewKind::Rect` 全文件恰 2 处（会话页不透明页底 + 输入区面层——pattern app_bar「面层 w/h None」叠层习语，bg = theme token 单源，非卡片拼装）；`Color` 构造恰 2 处（ON_ACCENT 按值镜像带单源注记 + state_color 解包 ItemState 预混表）；`PRESSED_BG`/`0xCCE3`/scrim 零命中。
- **帧循环/退出照 settings 先例**：wm 门控（0 = `mms: wm absent (no gui gift)` + park 常驻）→ WM_GETINFO → present GETINFO 轻断言 → 帧 VMO 三连@FRAME_VA → 主循环（INPUT_POLL 批拉→push_pointer→pump_input→Msg 消费→NavHost tick/take_popped→take_events→三节拍→render_frame→SUBMIT/FRAME_CALLBACK/RELEASE_WAIT 串行→vote 变化才发→wait_many n≤2 + IDLE 帽）；无退出路径（fail = eprintln + park，崩溃监督归 svcmgr）；首帧回执 `mms: first frame`。
- **容量账**：节点 ≈ 列表 55 + 会话 230 < MAX_NODES=512；glyph scratch 1024（**气泡列全帧共享游标**——16 气泡 ×26 字 + 16 状态 ×9 字 + 铬层/nav 过渡 ≈ 750，settings/contacts 256 档对本页不够，头注明账）；a11y 唯一节点 ≈ 30 ≤ 32 帽（尽力面）；press_map 1 ≤ 28 帽。

## 三、v1 形态 vs 终态差异（3b 依赖降解清单）

未落底座（W4-RECON §四 全景）与本应用逐项降解：

| # | 3b/appshell 依赖（未落） | 本应用 v1 降解 | 终态形态（落齐后） |
|---|---|---|---|
| 1 | **berry-appshell crate**（rpc/pump/scene/icons/conf 五模块） | settings 同款手拼 RPC 半程 + 自管帧循环（§二末条） | appshell::rpc/pump 收编，帧循环食谱退化为例行骨架 |
| 2 | **softimed/软键盘**（OP_INPUT_INJECT 无消费者） | 输入 = 固定快捷短语 chip 组（7 词）点选 + Input kind 值面草稿展示（`set_input_text` 可显不可敲）；点按 Input = 引擎焦点环/caret 即显（近似 ime_wanted 焦点环）+ hint 瞬态行「Typing hangs on softime - pick a chip」诚实标注 | softimed 升起敲键回显 + ime_wanted 焦点环接线（设计稿 §2.6 原句）；chips 保留为快捷短语层 |
| 3 | **wm back 路由**（encode_wm_back_req 零消费） | 会话返回 = app 内 AppBar 返回 glyph（NavHost 自管——contacts 同形） | R3 三段 back 链：先收键盘（清焦点环 + 撤 ime_wanted + BACK_ACK 不退出）再 back pop 回列表再 back 根页优雅退出 |
| 4 | **appshell::conf / MMS.JSN** | 会话/消息/草稿/回复计数全内存态（种子 3 会话内置；退出即弃）；容量 16 条/会话，满则丢最旧（数据槽 + 气泡节点整子树 remove，rows/msgs 同序不变式） | appshell::conf 键族 + FAT 单簇近原子持久化（launcher layout_io 食谱三约束纪律） |
| 5 | **theme 样式族**（card_style/oh_press_bg/OH_PRESS_SCALE——主树未落，W4-RECON §二末段） | 等价面：ItemState 五态预混表（pattern 单源）+ Card kind 自绘面 + Button/Chip 引擎 PressedVis/selected 色变 | 样式族落地后按压/缩放随迁单源 |
| 6 | **appshell Toast/Dialog Overlay** | 发送反馈/输入引导 = 应用内瞬态行（hint 行 + 状态次行，帧循环节拍 2s/3s 自撤，deadline 并入稳态等待五源取早） | appshell Toast 窗内 Overlay |
| 7 | **对端真回复面**（无 modem/net） | 本地节拍模拟（4.5s 轮换 4 条——诚实标注 Me/对方侧别） | 真消息面接入（net 词 loud 拒与终态同——v2） |
| 8 | **launcher /apps 预建 + 应用间跳转** | Message 跳 MMS 联动（contacts 侧）与 MMS 新会话通讯录选人挂 v2（无应用间跳转面） | 跨应用跳转面落齐后接线 |
| 9 | wm 礼物 input 槽位洞（无 fs 词 GUI app 的 input 落 [3] 需满前缀——send 预检 0 值 -EBADF 自陷可能，W4-RECON §1.2 已在册） | 照 settings/contacts 形用 input_h()（礼物在则亮，缺席/失效 = hi=0 静态页续活不死） | svcmgr/裁决 A 批收口（本批零基座改动） |

另（非 3b 依赖的 v1 形态取舍，设计稿 §2.6 对照）：

- **Fab(+) 新会话砍**：新会话组建依赖真敲键选人（softimed 挂 3b）——v1 无有意义写源，Fab 占位不做（诚实展示优先）；终态随敲键盘回归。
- **气泡行固定 48vp 高**：Card kind 内在 Row h=LISTITEM_H 定值（pattern 契约）——单行正文 + 状态次行内恰容（48+32+8 = 88 ≤ 144）；多行折行气泡挂布局后续刀。
- **Me/对方分侧视觉**：左/右贴 + elevation 1/2 + Me 正文 brand 色/状态次行（Card kind 面底色无 props 域——白底胶囊为 kind 单源；终态随 theme 样式族扩着色域随迁）。
- **Draft 角标**：plan 原 Badge 角标 → brand 衬底白字 Text mini 标（pattern::badge 为 count 值域件，非文本位）；终态 Badge kind 扩文本域后随迁。
- **滚动无惯性/fling**（跟手步进即停——contacts/files 同款）；头像 = 方形 brand 衬底首字母（「圆标」v1 方形降解，contacts 同款）。

## 四、验证与交接

- `cargo check --manifest-path user/bins/messages/Cargo.toml --target aarch64-unknown-linux-musl` → Finished 零 error 零 warning；`--tests` 同过（单测 = 纯函数面——状态文案/事件解码/气泡宽估值（含窄窗 clamp 倒挂防 panic max/min 形）/摘要装配/BITMAP glyph 形状抽查；host 运行不可行：bin 链 berry-libc 的 aarch64 ELF 专属 asm，执行面归 QEMU/镜像冒烟——files 同款结论）。
- 铁律 grep 门：`PRESSED_BG|0xCCE3|scrim` 零命中；`ViewKind::Rect` 恰 2 处（叠层语 + theme bg 单源）；`Color` 构造恰 2 处（ON_ACCENT 镜像注记 + ItemState 解包）；字符串字面量全 ASCII（python 扫描）。
- 交互逻辑走查（源级）：拖拽误触豁免（slop 内 Release-only 门）/ 双击连开门（cur Some 门控）/ pop 动画期行命中门（同门）/ chip 互斥与再击取消 / 在途状态跨发送/跨 pop 收口（数据面直落 Delivered）/ 容量满丢最旧在途索引随移 / bubble_w 窄窗防倒挂 / press_map 1 枚（Button 白名单）。
- 接线批（整合批）待办：justfile :328-337 七应用循环对 MMS 换 APPTOMB 食谱（musl 构建 + staging cp 本 crate manifest/ELF/icons/views + mkapp；循环 spec 已含 `MMS:messages` 映射与 entry=bin/messages 一致）；**裁决 B（boot 批 ui deferred）先行**——常驻 park 形入镜像即挂死 boot（W4-RECON §1.7 铁律）；rootfs-manifest.toml 零改（条目名 `app/MMS.bap` 不变）；包体门 = 构建后实测入账（稿估 0.85 MiB 档，>0.9 MiB 查因，BAP_MAX 4 MiB 远未触）；smoke APP/APPUI 计数断言随动判读 + VNC 截图验收（列表/Draft 角标/进会话/气泡分侧/chip 点选值面换文/Send 三拍瞬态/模拟回复/返回 pop 八景）。
- 运行时诊断面（console=false 私有对）：`mms: wm v=… input=…`（起握手）/ `mms: first frame`（首帧呈现回执）/ `mms: <what>`（fail+park 收口）。
