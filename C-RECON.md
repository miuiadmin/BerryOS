# C 刀侦察报告（land-c）

基线：主树 HEAD = `bff81e891`（land-c worktree 同基干净面）；侦察日期 2026-11-15 后。主树 `user/berry-guiproto/src/lib.rs` 有未提交脏面（他人波3 批A 在途稿），本报告一律以**已提交面**为基，脏差异单列。

## 一、guiproto 焦点/注入协议终态（已提交面）

均出自 `user/berry-guiproto/src/lib.rs`（494c3c0c0「波2 输入焦点协议 add-only」已并主）：

- `OP_WM_FOCUS: u8 = 0x66`（lib.rs:106）。帧恰 8B `[op][win u16 LE][r16]`（`WM_FOCUS_REQ_LEN = 8` lib.rs:1897）；win=0 = 让出焦点；codec 在 lib.rs:1909（encode）/1919（parse）；应答走 1B status 形。
- `OP_WM_BACK: u8 = 0x67`（lib.rs:108）。无参帧恰 8B `[op][r24]`（`WM_BACK_REQ_LEN = 8` lib.rs:1901）；导航键触发、wmd 查表路由到焦点窗；codec 在 lib.rs:1927/1935。
- `OP_INPUT_INJECT: u8 = 0x22`（lib.rs:112）。帧 `[op][dev_kind][payload ≤ 24B][尾 r16]`，`INPUT_INJECT_REQ_MAX = 28`（lib.rs:1905）；dev_kind 三值域 `DEV_POINTER=0 / DEV_KEYBOARD=1 / DEV_TEXT=2`（lib.rs:119-123）；键盘 payload = 1B keycode、文本 = ASCII 串（lib.rs:1943 注）；encode None 契约（野 dev_kind/载荷越帽），parse 在 lib.rs:1966。
- `WM_LAYER_SERVICE: u8 = 2`（lib.rs:115）。**注意：已提交面 `wm_layer_known` 仍 `l <= WM_LAYER_SYSTEM(1)`（lib.rs:438-440）**——layer=2 建 CREATE 请求在 wire 层即拒；SERVICE 收宽在 wip/ui-wave3 已提交（彼处 lib.rs:464-465 `l <= WM_LAYER_SERVICE`）。C 刀若接 SERVICE 层窗（softime/系统窗）需先收宽该谓词，属协议面变更、非 add-only 常量。

### 主树 guiproto 脏差异（未提交，C 刀不碰、仅注明）

主树工作树 lib.rs 相对 HEAD 增：波3 批A 旗位 `WM_CREATE_F_IMMERSIVE(1<<1)`/`WM_CREATE_F_IME_WANTED(1<<2)`/`WM_SET_F_IME_WANTED(1<<3)`/`WM_WIN_F_IMMERSIVE(1<<2)`/`WM_WIN_F_IME_WANTED(1<<3)` + 属主事件通道帧型 `WM_EVENT_BACK=0x01`（8B）/`WM_EVENT_TABLE=0x02`（8B）/`BACK_ACK=0x03`（1B）及对应 codec/值域收宽/测试。与 wave3 分支已提交 guiproto 同源但**缺 layer 收宽**。C 刀落码须避开此脏面（guiproto 若需改动，先等脏面落库或只走 SERVICE 收宽单点）。

### gui-inputd 消费形态

`bf0203258`（gui-inputd OP_INPUT_INJECT 消费面：DEV_KEYBOARD 1B keycode / DEV_TEXT ASCII 串 → EV_KEY press+release+SYN_REPORT 批 feed 泵，与 virtio-tablet 同队 drain；未知 dev_kind/空 payload ST_BAD）**未并主**——`git merge-base --is-ancestor` 判 NOT in HEAD，仅在 `wip/ui-wave3`（HEAD..wip/ui-wave3 含之，另有 b2b865a37 gui-wmd 焦点表/BACK 路由/属主事件通道同分支未并主）。主树基线 `drivers/bins/gui-inputd/src/main.rs`（988 行）零 INJECT 消费。C 刀若需注入消费面须自带或等波3 并主。

## 二、simbattery 通道现状

- 真值链：宿主 feeder → UART1 → 内核 task_simcon → 帧通道 fd0 → simbattery（`user/bins/simbattery/src/main.rs`，svcmgr 表 0x0001_0005 门控预拉）。契约单源 `berry-pwproto`：battery 载荷 state(4 态)/health(8 态)/capacity_pct(≤100)/voltage_mV/temp_mC（`user/berry-pwproto/src/lib.rs:29`，`BatterySnapshot.capacity_pct` lib.rs:113）。
- simbattery 现状：只持 seq 状态机 + ACK 回程（GIFT_H[0] `sim_ack_h()`）+ `SIMBAT ok seq=N` 标记（main.rs `Step::Frame` 分支，约 :62-77）；**不向任何面发布真值**（docs/sim设备通道与契约设计-v1.1.md §6 服务面只定义 ACK/标记）。berry-power 的 sim 份额 = DEV_MODEM only（F2 终裁，`user/bins/berry-power/src/main.rs:24-25`——DEV_BATTERY 归 simbattery）。
- 真值可接点：真值字节已在 simbattery 手中（Step::Frame 处可 decode BatterySnapshot）——C 刀接法 = simbattery 增发布面（channel 转发或查询 op）+ launcher 消费；或按波3 计划（`.agents/plans/2026-11-15-体验大改造六波实施方案.md:26`「battery 接 simbattery 真值」）对齐。
- launcher 当前电池显示（模拟满格钉死）：`user/bins/berry-launcher/src/desktop.rs:482` 建电池图标（STATUS_BATTERY）、**:483 a11y `b"battery simulated 100"`**、:292 低电换 DIM 图；`icons.rs:50` STATUS_BATTERY=2、`status_icons.rs:435-437` BATTERY_W/H/PX（OH 官方几何复刻，PX 数组 2880 项）。刷新可接点 = desktop.rs 右组构建处 + frameloop 周期面。

## 三、launcher 状态栏/导航条主树现状（不含波3）

- **单窗形态**：launcher 一个 APP 层窗，状态栏非独立 SYSTEM 窗——desktop.rs:380 注「状态栏：左时间、右 signal/wifi/battery」、:395-415 右组布局、:482 电池。波3 计划里的「双窗化（状态栏+五键导航条迁 layer=SYSTEM 窗）」主树未落。
- **五键导航条已在主树**：desktop.rs:146-152 常量（NAV_KEY_PX 44 / BIG 160 / NAV_BAR_H_BIG 190 / GAP 12/48）、:724-733 nav_bar 容器（5 键+4 gap 宽度式）、:755-759 五键 `back/recents/home/rotate/power`（nav_icon 闭包）。点击处理 `frameloop.rs on_nav_click`（约 :772 起）：back = 桌面 pager 翻页回退（:786-791，p>0 则 snap p-1）、recents = 重拉最近 spawn 应用、home = 回 0 页、rotate = `ROTATE` 串口标记、power = 二击 PSCI SYSTEM_OFF。
- **SYSBAR/BACK 标记零存在**：user/drivers/tools 全树 grep 零命中；wip/ui-wave3 的 gui8-smoke.py 标记表与主树 identical（也无）——SYSBAR/BACK 标记是全新面，C 刀（或波3 批B）首造。OP_WM_BACK 常量在（guiproto:108）但 launcher 未接（back 键现走 pager）、gui-wmd 路由在 wave3 未并主。

## 四、gui8-smoke 标记面扩法（tools/gui8-smoke.py）

- 标记常量段 :80-92（`MARK_BERRYBOOT`…`MARK_INPUT_ARMED`），字面族两种：guest 证据族 `UPPER ok` / `UPPER ok n=`（BERRYBOOT ok、LAUNCH ok、PAGE ok n=2、CLICK ok n=、LAYOUTW ok、LAYOUT ok n=）与 launcher 诊断族 `launcher: click ready` / `launcher: input poll armed`（小写冒号形）。现役表内**无 kebab-case 先例**——若按小写 kebab 落新标记（如 `sysbar up` / `back routed` 形），须对全表互查子串。
- 扩点四件：① :80-92 常量段加 `MARK_SYSBAR`/`MARK_BACK` 字面；② `_boot_base_markers()`（:1095-1096）或专批 wait 挂新标记；③ `launcher_marker_status()`（:1040-1053）探针表加 `(src 文件, needle)` 行（现查 frameloop.rs/layout_io.rs 源码字面）；④ 计数/时序断言走 `_require_exact_count`（:744）与 `_require_pos_after`（:759）。
- 契约：与现役全表**两两互非子串**（grep -F 互查；先例 docs/app-0实施设计草案-v1.0.md:134「全部 44 枚两两互查」、docs/app-1实施设计草案-v1.2.md:101 备胎表——`APPTOMB ok n=1` 因含 `TOMB ok n=1` 被废的判例）。SYSBAR/BACK 首词现役零命中，词根安全；带参形态（`SYSBAR ok n=` 类）维持鉴别力。

## 五、appshell 裁决

- wip/ui-wave3 的 berry-appshell = 波3-件⑦ `f016aa99a`「launcher 五模块纯抽取」：rpc（严格串行半程）/pump（帧循环骨架+HostHooks）/scene（状态栏底座）/icons（资产供给桥）/conf（单簇持久键）五模块，库本体 no_std+host 测（27 测）。deps = berry-sys/berry-guiproto/berry-appproto/berry-layout/berry-ui/berry-gfx——全部主树在席 crate；appshell 源零用 OP_WM_*/IMMERSIVE/WM_EVENT/BACK_ACK 面（grep 零命中），库依赖面本身轻。
- 但落地形态重：f016aa99a 单笔 19 文件 +2319/−1109——新 crate 8 文件之外**重写 launcher 四件**（desktop.rs 260 处/frameloop.rs 667 处/layout_io.rs 177 处/main.rs 247 处变更）+ justfile + user Cargo 面；其父提交与主树 HEAD 已漂移（launcher 3 文件 224+/17−；berry-ui 27 文件 807+/8629−——主树 berry-ui 反超 8.6k 行，wave3 基更老）。cherry-pick 到主树 = launcher 全量重构高冲突 + 拖波3 双窗/TabBar 前置语义。
- 且 C 刀要的 Dialog/Menu/Toast 在 wave3 **零实现**（W3A-REVIEW `425e77dee` 复核「3b 暂缓面独立复核零实现」；wave3 appshell/berry-ui 源 grep Dialog/Toast/Menu 零命中）——抽取版里没有这三件，cherry-pick 拿不到目标物。
- **裁决建议：不 cherry-pick。** 依赖重（launcher 四件全重写 + 基漂移 + berry-ui 代差反向）且抽取版不含 Dialog/Menu/Toast。C 刀新建最小 berry-appshell（只 Dialog/Menu/Toast widget 面；deps 收窄 berry-ui[+berry-gfx]；host 纯函数测走 justfile test-host 先例；no_std+cfg(not(test)) 门照 f016aa99a Cargo 注的 fsclient 先例）。抽取版留 wave3，并主时合并：两版零重叠（最小版=widget 模块、抽取版=五模块骨架），同名 crate 归并以抽取版骨架吸收 widget 模块，纯增量无语义冲突面。
