# W4-IMPL-CONTACTS —— Contacts 应用 v1 实施报告（波 4 第三件）

基线：worktree `wip/ui-wave4` HEAD 769515a9a；共同依据 = 本目录 [W4-RECON.md](W4-RECON.md) + `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.3（行号以 RECON 现场直数为准）。任务书四件：列表（ListItem 头像位+姓名+号码）→ 详情 NavHost push/pop；拨号 →「Calling…」瞬态行（v1 降解 Toast）；Star 收藏过滤（Seg）；数据 v1 内置静态示例（fs 持久化挂终态）。**全部落地，`cargo check --target aarch64-unknown-linux-musl` 零警告零错误（2026-11-15 现场）**。

## 一、已落面

### 1.1 crate 形态（Settings 垂直切片镜像）

- `user/bins/contacts/Cargo.toml`：独立小工作区（空 [workspace] escape user 根）、字面 version=0.2.0、五 deps（berry-libc/sys/guiproto/ui/gfx）、release z/lto/cgu=1/abort/strip——settings/Cargo.toml 逐字段镜像，含三偏差裁定注记（字面 version / +berry-libc / +berry-guiproto）。
- `user/bins/contacts/manifest.toml`：id=CONTACTS / entry=bin/contacts / label=Contacts / console=false / stack_pages=64（真 GUI 链 8 页穿先例 ×8）/ security 三键 / [ui] 三键。**无 fs 词**（v1 静态数据零持久化——与任务书一致；fs 词随 CONTACTS.JSN 终态同批增）。
- `user/bins/contacts/src/main.rs`（1434 行）：settings/main.rs（941 行）为底模 + 四增量（glyph 供件 / NavHost / Seg / 瞬态行节拍）。
- 资产面：icons/views 仍由 `user/bins/appstub/apps/CONTACTS/`（icon.png + main.bui）供给——settings 先例同形（crate 内无 icons 目录；[ui] icon/views 键在打包期由 staging 满足）。桌面图标零增量（设计稿 §3.3）。
- `user/bins/contacts/Cargo.lock`：cargo check 生成入册（settings 同款先例）。

### 1.2 页面树与交互（四件全落地）

```text
窗（全幅单面呈现——settings/launcher 同形，surface_id=0）
└─ NavHost（pattern.rs 页栈状态机；根页不 push = depth 0 根语义）
   ├─ [根] 列表页 Stack（pattern::stack）
   │  ├─ 滚动层 Scroll（app 自管 offset：DragStart/DragMove 纵拖 → set_props 换 offset_y，
   │  │   clamp [0, content_h − vp_h] 直钳无橡皮筋）
   │  │  └─ 内容 Col[顶让位 spacer 264px, list_item × n（首字母头像位 + Name/Phone
   │  │      双行 + 收藏星标 glyph 右对齐）, 空态提示（Starred 档空集）]
   │  └─ 铬层 Col（后画盖上 = Scroll v1 无 clip 的滚动溢出遮盖面）
   │     ├─ app_bar(title=Contacts, back=false)
   │     └─ Seg(All | Starred) —— 点击切档重建行集（回顶）
   └─ [推] 详情页 Stack（NavHost.push 300ms 右滑入；z 顶覆盖）
      ├─ 不透明页底 Rect(bg_page)（滑入期列表页不可透——app_bar 面层语同款）
      └─ Col[app_bar(title=联系人名, back=true → pop), 居中名片（大首字母头像 +
         Name/Phone）, 按钮行（Call=Button Filled / Star|Unstar=Button Outlined）,
         底部瞬态行（"Calling <name>..." 2s 自撤，空文隐藏）]
```

- **行命中**：`Msg::Release` 携坐标 → `ui.bounds` 行卡界包含判定（Card 子树全幅——pattern 内部节点 id 不外露的补偿面；`Msg::Click` 无坐标面故走 Release；slop 内非拖 up 才有 Release，拖拽路径自然豁免误触）。
- **行按压反馈**：`pattern::ItemState::Pressed.bg()`（brand 20% 预混白）app 侧 set_props 换头像衬底——引擎 `press_react` 同形（Press 换底 / Release·Cancel·DragStart 还原，拖路径无 Release 的补还原面）；色值零字面量（§3.2）。Button 按压 = 引擎 PressedVis 查表（press_map 两枚登记）。
- **Star 闭环**：详情页 Button 翻 `stars[]` + label 换文（Star↔Unstar）→ pop 返回重建行集 → 行内星标 glyph（STAR_ON/STAR_OFF 两态）与 Seg 过滤集同步。
- **Calling 瞬态行**：EV_CALL → set_text 装配 "Calling <name>..."，deadline = now+2s 进帧循环三源 wait（min(next_wakeup, IDLE 帽, 自撤期限)）——到期 set_text 空文隐藏；重复 Call 重起节拍。无 modem 仅 UI 反馈（诚实边界注记在头注）。
- **滚动**：DragY 累进（y ≥ chrome_h 门控 = 铬区不滚）；Seg 切档回顶 / pop 返回保留偏移（钳新域）。

### 1.3 应用内 glyph（程序化——设计稿 §3.3 自绘路）

`IconSource` 真供件三面，const fn 整数光栅（零浮点零堆，rodata 落位）：BACK=pattern::icons::BACK（0x0101，「<」双臂线段 r=3px，24px 源 → 72px 三倍整数最近邻硬边）；STAR_ON/STAR_OFF（0x0210/0x0211，五角星十顶点偶奇填充，色 = theme::BRAND/FONT_SECONDARY 打包——两态同构异色）。**宿主独立编译视觉自检过**（ASCII 面打印：箭头双臂成形、五星双腿显形——首版截断除法令亚像素腿宽塌 0，已改四舍五入除（符号归一）修复，坑注记在 in_star 头注）。

### 1.4 引擎消费面（pattern 27 公开面位的首批真实消费者之一）

list_item / app_bar / seg / stack / spacer / NavHost（attach/push/pop/tick/take_popped/depth）/ ItemState 五态表 / pattern::icons::BACK / LISTITEM_H/LISTITEM_PAD_X/TOPBAR_H/SEG_BAR_H 几何常量；desc 面 Button(kind)/Scroll(kind)/ButtonStyle；theme Tokens::pick(Light) 全页配色单源。节点预算 ≈170/512；a11y ≈19/32 帽。

## 二、v1 形态 vs 终态差异（3b 依赖降解清单）

未落底座五件（RECON §四开列，全部适用本应用）：

| # | v1 形态（本实现） | 终态差异（3b/波 2 落齐后） | 迁移面 |
|---|---|---|---|
| 1 | **appshell 缺席**：RPC 半程族（rpc_send/recv/rpc）+ 帧循环 + input_arm 全套手拼（settings v0 逐字镜像，~450 行） | appshell::rpc/pump/scene 收编——应用侧零直连 RPC 手拼 | 整段删除换 appshell 调用；帧循环骨架归 pump |
| 2 | **wm back 链无**（encode_wm_back_req 零消费）：详情返回 = app 内 AppBar 返回 glyph 手势位；根页无返回键（无退出路径——park 常驻照先例） | 系统 back 全链 pop（R3）；根页 back = 优雅退出（exit 0 + DESTROY + BACK_ACK） | Click(back) 分支改 appshell back 消费；退出面挂裁决 B 后的回槽语义 |
| 3 | **Toast 降解**：应用内底部 Text 瞬态行（帧循环节拍 2s 自撤，三源 wait deadline 补源） | appshell::Toast 窗内 Overlay（底部 2s 同语义，跨页常驻） | EV_CALL 分支换 appshell Toast 调用；删 calling_until/三源 wait |
| 4 | **fs 持久化缺席**：数据内置静态 10 条（SEEDS const），Star 翻转 = 会话态（重启回种子）；manifest 无 fs 词 | CONTACTS.JSN 经 appshell::conf 单簇原子写（launcher layout_io 食谱：缺席回默认/坏读自愈/写失败非致命三纪律）；manifest 增 `fs = ["rootfs"]` | stars[] 挂 conf 键读写；种子降级为首次生成默认值 |
| 5 | **theme 样式族未落**：行按压 = ItemState 五态表 app 侧换底（press_react 同形）；无 0.95 缩放档 | card_style/oh_press_bg（20% 遮罩预混）/OH_PRESS_SCALE_MIDDLE_PERMIL=950 引擎单源 | tint_row 删；行卡视觉随 pattern 内建（RECON §二末段「补移植 or 用 ItemState 面」——本批裁定用 ItemState 面） |

应用内裁决级差异（非 3b 依赖，本批自裁——评审可推翻）：

- **头像位方形降解**：首字母 Text bg 衬底 = 字格方形块（32×48px），非「首字母圆标」——主树无圆/椭圆原语（fill_rounded 是引擎内部面），圆标挂 theme 样式族或 Canvas 档后续刀。
- **Scroll 无 clip 补偿**：主树 Scroll v1 无 paint clip（子溢出可见）——列表页用 Stack 铬层后画盖上遮盖滚动溢出（pattern app_bar 自带不透明面层/Seg 格自带底）；overscroll 直钳无橡皮筋、无惯性滚动（速度环数据在 DragEnd 已有，未消费）。
- **Search 砍**（设计稿 §2.3 有）：softimed 未落无敲键面，过滤职能由 Seg 承担（任务书原文即 Seg）；终态 Search Input + softimed 升起回显。
- **新建/编辑表单 + Fab(+) 砍**（设计稿 §2.3 有）：静态数据无写源，表单无持久化语义（CONTACTS.JSN 同批落）；终态随 fs 词一起落。
- **Delete/Dialog 砍**（设计稿 §2.3 有）：appshell Dialog Overlay 未落（静态数据删除亦无写源）；终态 Dialog 确认 + 删条目回根页。
- **Message 跳 MMS**：设计稿原句挂 v2（无应用间跳转面）——本批未涉及，无漂移。
- **返回键无按压视觉**：Icon kind 不在 press_map visual 白名单（Rect/Text/Button 族）——appshell Overlay 化后随迁。

## 三、改动清单

新增（本批独占，均为新文件）：

- `user/bins/contacts/Cargo.toml`（46 行——settings 镜像 + 波 4 注记）
- `user/bins/contacts/manifest.toml`（22 行——appstub CONTACTS 形 + stack_pages 64）
- `user/bins/contacts/src/main.rs`（1434 行）
- `user/bins/contacts/Cargo.lock`（check 生成）

零改动（铁律面核实）：launcher / guifbd / gui-wmd / svcmgr / berry-ui / berry-gfx / justfile / tools / rootfs-manifest——`git status` 仅 `W4-RECON.md`（先行批）+ `user/bins/{contacts,files,photos}/`（contacts=本批；files/photos=他批实施文件，未碰未读）。appstub/apps/CONTACTS 资产不动。

## 四、验证面

- `cargo check --target aarch64-unknown-linux-musl`（worktree cargo-target）：**过，零警告**（命令口径 = settings 同检；berry-ld 链接面归打包批 build 行）。
- glyph 视觉自检：宿主 rustc 独立编译同源 const fn，ASCII 面打印验证（BACK 双臂 chevron 成形 / STAR 五星双腿成形 / 两态形状同构断言）；发现并修复截断除腿塌 bug（§1.3）。
- 铁律 grep 门：`scrim|PRESSED_BG|0xCCE3` 零命中；`ViewKind::Rect` 全文件恰 1 处（详情页底，pattern 叠层语 + theme bg 单源）；字符串字面量全 ASCII；Color 构造恰 2 处（ON_ACCENT 按值镜像带单源注记 + state_color 解包 ItemState 单源）。
- 交互逻辑走查（源级）：拖拽误触豁免（slop 内 Release-only 门）/ 双指连击门（detail_page Some 门控）/ pop 动画期行命中门（同门）/ 空态提示回收（防残树占位）/ press_map 2 枚 ≤ 16 帽 / a11y 19 ≤ 32 帽 / 节点 170 ≤ 512 帽。

## 五、接线待办（归波 4 打包/横切批——本批不动 justfile）

1. justfile :328-337 七应用 appstub 循环 → 逐 crate 段（APPTOMB :315-327 食谱：musl 构建 RUSTFLAGS berry-ld → staging（manifest + ELF + appstub/apps/CONTACTS 资产）→ mkapp）。
2. **裁决 B（boot 批 ui deferred）先行**——常驻 park 形 crate 未入册直入镜像 = boot 批 app_reap_tail 永等 = 全线 smoke FAIL（RECON §1.7 铁律）。
3. rootfs-manifest 零改（条目名 app/CONTACTS.bap 不变）；包体门 = 构建后实测入账（估 0.80 MiB 档，>0.9 MiB 查因，BAP_MAX 4 MiB 单包帽远未触）。
4. smoke 随动判读：`APP ok n=9` / `APPUI ok n=` 零漂（boot 批不拉起本应用——gui deferred 形）；VNC 截图验收（列表/按压换底/详情 push 滑入/Calling 瞬态/Seg 过滤五景）。
