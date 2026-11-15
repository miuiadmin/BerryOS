# VNC 第九波「网页桌面产品化」实施计划（BerryOS 品牌壳页/自动重连/移动端/--open）

Type: plan
Status: done
Updated: 2026-11-15

一句话说明：把 `berry vnc web` 的裸 vendored noVNC 页升级为 BerryOS 自有的产品级浏览器桌面壳页（状态栏/工具栏/自动重连/唤醒锁/深色主题/移动端自适应），完全落在 `tools/berry/web/` 新增自有 Apache-2.0 资产与 `web.rs`/`commands.rs`/`main.rs` 的 VNC 路由面内——零冻结依赖、零新 Rust 依赖、零 MPL 文件混改（只 import `novnc/core/`）。

## 背景

VNC 八波已收官（工作树态，未 commit；单测 98 绿、四门全绿、check-docs 零告警）：对外暴露与认证 → noVNC 网页桌面/TLS VeNCrypt/审计/口令 → 三平台打包与 TPN 单源 → 两轮遗漏检查（[五六波遗漏检查](../research/2026-11-14-vnc五六波遗漏检查.md)）→ rustls TLS 客户端 + https 页 + wss 终止代理 → ws 客户端自检 → 沿革节号治理。功能面已完整；本波定性是「产品化体验」——当前默认页是未改造的 vendored `vnc.html`，无品牌、无断线重连、无移动端适配，`berry vnc web` 打印 URL 后的用户体验止步于「能连上」。

本波前置 probe（2026-11-14，仓库零写入，实验产物留档 `/tmp/berry-f2`、`/tmp/berry-f3`）五面定音，构成本计划的全部外事实依据：

- **f1（core 公开面）**：`novnc/core/rfb.js` 是唯一必选入口——构造 `RFB(target, urlOrChannel, {shared, credentials, repeaterID, wsProtocols})`，事件 11 个（connect/disconnect/credentialsrequired/securityfailure/serververification/clipboard/bell/capabilities/desktopname/clippingviewport），方法含 disconnect/sendCredentials/sendKey/focus/blur，属性含 viewOnly/scaleViewport/resizeSession/clipViewport/qualityLevel/compressionLevel（无 scaleMode——缩放即 `scaleViewport` 布尔）；触摸由 core 内建 GestureHandler 合成 7 类手势（onetap 左键/twotap 右键/threetap 中键/longpress 右键/drag 拖动/twodrag 滚轮/pinch 缩放），键盘由 core 内建 Keyboard 挂 canvas；可选 import `core/util/logging.js`（initLogging，缺省 warn）与 `core/input/keysym.js`。host/port/encrypt/path 是 noVNC app 层参数、非 core 参数——本波新页自己消费。
- **f2（三形态实连）**：明文 run `path=<ws uri>`、`--tls` 施明文 `host/port/encrypt=false&path=`（空值）、TLS run `host/port/encrypt=true&path=`（空值，指 wss 代理口）三形态 Chrome headless 实起实连全过；死后端错误态 sticky（noVNC app 的 ui.js 行为）。
- **f3（headless 可测性）**：wakeLock.request('screen') 可测；requestFullscreen 权限拒（REJECT TypeError）不可测；软键盘只能测到隐藏输入框 focus 态；合成 TouchEvent 可派发但真机手势管线不可测；断流重连**终态**可断言（杀后端同口重启后 DOM 回「已连接」，25s 虚拟时间实测）而中间态单次 dump 捕不到；`--window-size=390,844` 截图恰为 390x844 但 innerWidth 被 headless=new 钳到 500——小于 500px 的媒体查询经 `--window-size` 不可测，截图是 500px 渲染的水平裁切。
- **f4（QR 裁决线索）**：自写最小 QR 约 300-450 行（v6 起多块交织，中 correctness 风险）；vendored 候选 qrcode-generator 1.4.4（MIT 单文件纯 ES5 UMD，非 ESM）；关键事实——`web.rs` serve 硬绑 127.0.0.1 且 QEMU VNC 亦 loopback，手机扫出的 QR 打开的是手机自己的 loopback，QR 价值兑现的前置是 LAN 绑定模式（不存在）。
- **f5（树与检查面）**：`web/` 现为 NOTICE.md + novnc/（全树共 128 文件约 1.5MB）；`web.rs` serve 硬接 `root/novnc` 前缀且 `/`→`vnc.html`、`find_web_root` 哨兵 `novnc/vnc.html`；自有新页放 web 根需改路由（允许面），塞进 novnc/ 则混入 vendored MPL 树（违红线）；`mime_for` 已覆盖 html/js/mjs/css/json/png/svg（`.webmanifest` 缺——本波不引 manifest 则无需补）；`check-appdir.sh` web 五件检查与 TPN 的 web/NOTICE.md 指针均以 novnc/vnc.html 为锚；NOTICE 现为单「## noVNC」节，可加自有资产节；沿革日期族现最大节号三百二十九（已复核尾部），三百三十现不存在——落笔时须实时重数。

冻结面声明：`window.rs`/`profile.rs` 仍为并行会话未提交 M 态，`main.rs` 仅许动 VNC 布线区（`VncWebCmd` 与 dispatch），`arch/`/`kernel/`/`sched/`/`user/`/`.agents/research/`（含当时的提交拆分清单——已由[落账清单](../research/2026-11-15-vnc落账清单.md)取代，本波新增件的提交归属由用户按清单自行归列，本刀不碰该文件）零触碰。

## 契约

### 1 页面文件布局、命名与路由（自有资产三件 + 双树解析）

**文件**：web 根新增三件自有资产（非 vendored、Apache-2.0、文件头各一行短注指向仓库根 LICENSE）：

| 文件 | 职责 |
|---|---|
| `tools/berry/web/index.html` | 入口页：meta viewport（width=device-width, initial-scale=1, viewport-fit=cover, 禁双击缩放）、深色主题骨架、状态栏/工具栏/画布容器/凭据弹窗的静态结构 |
| `tools/berry/web/berry.js` | 全部逻辑（自有 ESM）：参数解析、RFB 生命周期、自动重连、工具栏布线、唤醒锁、凭据弹窗、软键盘转发；仅 import `./novnc/core/rfb.js`、`./novnc/core/util/logging.js`、`./novnc/core/input/keyboard.js`（软键盘第二实例） |
| `tools/berry/web/berry.css` | 深色主题与自适应布局（系统字体栈，零外部字体/CDN/运行时） |

不放 `web/berry/` 子目录——web 根直放少一层路由与引用改写；NOTICE 与检查器点名三件即可。**不复制任何 noVNC app/ 代码**（ui.js 的 touchKeyboard「第二 Keyboard 实例挂隐藏输入框」是公开 core API 的用法模式，自有布线重写，无文本拷贝）。

**路由（[web.rs](../../tools/berry/src/web.rs)）**：从单树 `root/novnc/<rel>` 改双树解析——

| URL | 解析 | 越界防线 |
|---|---|---|
| `/` | `<root>/index.html`（新默认页） | root 基准 canonicalize 包含检查 |
| `/index.html`、`/berry.js`、`/berry.css`、`/NOTICE.md` | `<root>/<rel>`（自有资产树） | 同上 |
| `/novnc/<rel>` | `<root>/novnc/<rel>`（vendored 原样，含逃生门） | novnc 基准包含检查 |

`sanitize_path("/")` 映射 `vnc.html` → `index.html`；`..`/反斜杠/冒号/NUL 拒绝与 percent-decode 后复检语义零变化（两树共用同一 sanitize，负例矩阵扩到 `/novnc/../../x` 形）。裸 `/vnc.html` 不再映射 novnc——own 树无此文件即 404（工作树态无兼容包袱，README 记录新形态）。`Cache-Control: no-store` 与 30s 连接预算、准入闸、TLS 面全部不动。`find_web_root` 哨兵改双件：`novnc/vnc.html` **与** `index.html` 皆须在（web/ 是一个随包单元；缺新页则默认入口死，E_DEVICE hint 同步提两件）。`mime_for` 零改动（三件扩展名已覆盖；无 manifest 计划故不补 `.webmanifest`）。

### 2 URL 三形态在新页的参数消费 + raw 逃生门

三个 `build_page_url*` 仅把页路径 `/vnc.html?` 改 `/?`，query 参数、fragment 规则、percent-编码逐字节不变（三形态语义冻结，来自 [wave6 计划](2026-11-14-vnc-compliance-wave6.md)与 wave5 的实测钉死）。新页消费面（同名参数与 noVNC app 层同义，便于与逃生门互换）：

- `path` 非空 → 直接作 ws/wss URL 用（形态 1；新页自建 URL，**不复刻** ui.js「https 页无条件强改 path scheme 为 wss」的怪癖——loopback ws 直拨本就被 Chrome 豁免，f2 证实豁免基于 origin 不基于参数形态）。
- `path` 缺席或空 → 由 `host`/`port`/`encrypt` 自建 `ws://` 或 `wss://` URL（形态 2/3），路径取根 `/`（QEMU ws 端点只认根路径）；新页没有「缺席 path 默认拼 websockify」的行为， builders 里 `path=` 在场空值纯粹是既有形态保真。
- `autoconnect=true` → 载入即连；缺席或无连接参数 → 空闲态，状态栏提示「URL 缺少连接参数——请用 berry vnc web 打印的 URL 打开」（不做手动连接表单，控范围）。
- `resize` 给缩放初值：`scale`（适配窗口）| `100%`（原始尺寸可滚动）| `off`（原始尺寸裁切不滚动），缺省 scale（TLS 形态 URL 自带 `resize=off` 沿用）。
- `view_only=1` → 初始只读态（新页自有参数，产品价值=可分享只读链接）。
- fragment `#password=` → 口令缓存 + `history.replaceState` 抹掉地址栏 fragment（读后即抹，防截屏/分享泄漏；内存保留供重连复用）。

**raw 逃生门**：直访 `/novnc/vnc.html?…`（同参数，vendored 页自消费）——不做 `raw=1` 参数别名（少一条分叉逻辑）；vendored 页与其全部资产零改动、永久保留。

### 3 状态栏

顶栏三段：左「BerryOS」品牌 + `desktopname` 事件给的会话名；中连接状态文案（连接中/已连接（已加密|未加密）/重连中（N 秒后重试）/已断开/连接失败——加密态按当前 URL 形态是否 wss 判定）；右分辨率 `{w}×{h}`（连接成功后读画布容器内 canvas 的 width/height 属性，此后每秒廉价刷新一次；`clippingviewport` 等事件不依赖）。**帧率砍项**：core 无公开帧率面，rAF 自算只是页面帧率不是 VNC 帧率，不做假指标（偏差 2）。状态以 body 类镜像供测试断言：`berry-connecting`/`berry-connected`/`berry-reconnecting`/`berry-error`/`berry-disconnected`（互斥单值）；测试锚 id：`#berry-status`、`#berry-res`、`#berry-holder`（RFB target）。

### 4 工具栏（逐项行为与降级）

五键（id 即测试锚）：`#berry-btn-fullscreen`、`#berry-btn-resize`、`#berry-btn-viewonly`、`#berry-btn-kbd`、`#berry-btn-conn`。

- **全屏**：画布容器 `requestFullscreen()`；进出全屏同步 body 类。降级：headless 权限拒（f3），e2e 只断言按钮在位与 handler 注册不抛，真行为留有头验证（偏差 6）。
- **缩放三态循环** scale → 100% → off：scale = `rfb.scaleViewport = true`；100% = false + 容器 overflow auto（滚动条）；off = false + overflow hidden（裁切）。按钮 label 反映当前态；`resize` 参数给初值。不做 remote（SetDesktopSize）档——非目标。
- **view-only**：`rfb.viewOnly` 翻转 + body 类 `berry-viewonly` + 按钮 `aria-pressed`；`view_only=1` 参数给初值。
- **软键盘（移动端）**：自有隐藏 `<input>`（autocapitalize/autocorrect off）+ `core/input/keyboard.js` 第二实例（`onkeyevent → rfb.sendKey(keysym, code, down)`）——与 ui.js touchKeyboard 同型但自有布线，只 import 不拷贝。开=focus+grab、关=blur+ungrab；不被 isTouchDevice 闸（手动开关）。降级：真 OSK 弹出与 IME 组合输入 headless 不可测（f3 只能测 focus 态；组合输入不翻译，边界节）。
- **断开/重连单键双态**：已连 = `rfb.disconnect()` + 停自动重连（转 `berry-disconnected`）；未连 = 立即尝试连接 + 退避计数清零。

移动端自适应：断点 `max-width: 700px` → body 加 `berry-compact`（matchMedia 监听），工具栏收为图标键换行、状态栏两行。断点取 700 而非更窄：headless innerWidth 钳 500（f3），≤700 才能在 `--window-size=390,844`（innerWidth=500）下可断言；<500px 的像素级观感不可测（500px 渲染水平裁切，f3），如实降级（偏差 4）。触摸交互零自研：GestureHandler 在 core 内建，画布容器即得 7 类手势透传（f1）。

### 5 自动重连策略

非手动断开的 disconnect（含 ws 断流、QEMU 死亡）→ 进入 `berry-reconnecting`：第 n 次尝试前等待 `min(1000 × 2^n, 30000)` ms（n 从 0 起），状态栏显示秒级倒计时；尝试失败 n+1 递增，成功清零回 connected。手动重连键=立即尝试且清零。暂停与终止条件：`credentialsrequired` → 弹凭据弹窗暂停倒计时；`securityfailure`（口令错）→ 终止自动重连转 `berry-error` + 弹窗重输（缓存口令作废）；手动断开键 → 终止。重连始终复用载入时解析的连接参数与内存口令，不重读地址栏（fragment 已抹）。`serververification`/`bell`/`clipboard`/`capabilities` 事件不实现专用 UI（serververification 属 RSA-AES 安全型，QEMU 面不触发；边界节）。

### 6 口令 fragment 与凭据弹窗

凭据弹窗为自有深色样式（`#berry-cred-dialog` + 口令输入 `#berry-cred-pass` + 提交/取消）：`credentialsrequired` 触发时，内存有缓存口令则先 `sendCredentials({password})` 静默重试，失败或无缓存才弹窗；弹窗提交即 `sendCredentials`，取消则停自动重连转断开态。口令只在内存与 `sendCredentials` 调用中流动——不进 localStorage、不进日志、不回写地址栏。无口令 run（环回默认）全程不弹。

### 7 唤醒锁（wake lock）

connected 态调 `navigator.wakeLock.request('screen')`，成功则 body 加 `berry-wakelock` 类（e2e 可断言，f3 证实 headless 可得）；`visibilitychange` 回 visible 且仍连接则重新请求；API 缺失或被拒 → 静默降级（类缺席即降级态，不告警不阻塞）。

### 8 QR：砍项

砍。依据 f4：服务端（web.rs 硬绑 127.0.0.1）与 QEMU ws 皆 loopback，手机扫码打开的是手机自己的 loopback——功能上就是死路；QR 价值兑现的前置是 LAN 绑定模式（不在本波与既有边界内）。三案均不取：自写 300-450 行（v6 起多块交织，为一条 URL 不值）、vendored qrcode-generator 1.4.4（MIT 但非 ESM、需 NOTICE 登记）、nayuki（MIT 但要剥 TS 类型=改 vendored 文件）。重启条件（登记进 Agent Note）：LAN 绑定模式立项时随批评估，届时 QR 内容必须用剥掉 `#password=` fragment 的 URL（口令不进可扫实体）。

### 9 `--open`：落地（macOS 自动开、URL 剥 fragment）

`berry vnc web --open`：URL 构造与打印行为不变；打印后、进入阻塞 serve 前尝试开浏览器——macOS 走 `open <url>`（先例 `vnc_open`，commands.rs 1668-1694 行同型），**URL 剥掉 `#password=` fragment**（口令不进 `open` 的一次性 argv，与「口令不进 host 子进程 argv」的既有口径一致；环回默认无口令的多数场景无感，有口令时页面凭据弹窗兜底）；非 macOS 或 open 失败 → 不视为错误，文本模式多打一行 hint（平台不支持/请手动打开 URL）。JSON 输出增 `"opened": <bool>`；文本与 JSON 的 serving/hint 行零变化。

### 10 NOTICE、check-appdir 与打包随行

- [web/NOTICE.md](../../tools/berry/web/NOTICE.md) 在「## noVNC」节后增「## 自有资产」节：`index.html`/`berry.js`/`berry.css` 为本仓自有 Apache-2.0 资产（非 vendored），运行时 import vendored `novnc/core/`（MPL-2.0）且不修改任何上游文件；自有资产不入 THIRD-PARTY-NOTICES（非第三方）。
- [check-appdir.sh](../../tools/berry/packaging/linux/check-appdir.sh) web 检查段（现 319-326 行）增两检：`$WEB/index.html` 在场（check_file）+ `$WEB/NOTICE.md` 含 `## 自有资产` 行（grep，防声明漂移）；既有五件与 TPN 指针检查零变化。
- 三平台打包脚本零改动：build-appdir.sh/macos build.sh/windows assemble.sh 均整目录拷贝 `tools/berry/web/`（ Wholesale 拷贝注释在案），三件与 NOTICE 增节天然随行；Linux AppDir 重建 + check-appdir 0 fail 作实证（p1）。

## 步骤总览

1. [x] 自有资产落 web 根（实况：五件非三件——`index.html`/`berry.js`/`berry.css` 外加自产 `berry-logo.svg`/`berry-bell.wav`，后两件的由来见偏差 10/11）+ NOTICE 增「## 自有资产」节（点名五件）。
2. [x] `web.rs` 双树路由与防线、`sanitize_path` 默认页改写、`find_web_root` 双哨兵、三个 `build_page_url*` 页路径改 `/?`、`strip_fragment` 纯函数；单测改写与新增（U41-U45；实况 100 绿 = 98 基线 + U44/U45 两新，U41-U43 为既有测试改写不增数）。
3. [x] `commands.rs` `vnc_web` 增 `--open`（剥 fragment 调 open、JSON `opened` 键、hint 行）+ `main.rs` `VncWebCmd` 布线（仅 VNC 区）。
4. [x] `check-appdir.sh` 增两检；Linux AppDir 重建 + check-appdir 0 fail（p1）。
5. [x] Chrome headless e2e c1-c9（脚本 `/tmp/berry-w9probe`、产物 `out/vnc-w9-*`；终态 15/15 全绿）+ 全量门（cargo test 100 绿 / `just berry-build` rc=0 / 三冒烟绿 / check-docs 0 告警）。
6. [x] docs 面：README（命令树行/第九波段落/Chrome 段 URL 例与逃生门）、CLI-first 升 v1.6（头块/§8.1/§8.2/§14）、沿革三百三十（落笔时实数复核唯一）、Agent Note（`2026-11-14-vnc-web-polish-wave9.md`）、本计划收口 `done`、check-docs；docs 收口刀（2026-11-15）续补 README 已知限制/CLI-first §15/沿革三百三十一/笔记 Alternatives 两条（见验收记录末段）。

## 文件改动清单

| 文件 | 改动 |
|---|---|
| `tools/berry/web/index.html`（新建） | 自有入口页：viewport meta、深色骨架、状态栏/工具栏/画布容器/凭据弹窗静态结构（契约 1/3/4 的 DOM 契约） |
| `tools/berry/web/berry.js`（新建） | 自有 ESM：参数解析与 URL 自建、RFB 生命周期与事件布线、自动重连、工具栏五键、唤醒锁、凭据弹窗、软键盘第二 Keyboard 实例、compact 断点监听；仅 import `./novnc/core/rfb.js`、`./novnc/core/util/logging.js`、`./novnc/core/input/keyboard.js` |
| `tools/berry/web/berry.css`（新建） | 深色主题、三段状态栏、工具栏（含 `berry-compact` 档）、画布容器三态 overflow、弹窗样式；系统字体栈 |
| `tools/berry/web/NOTICE.md` | 增「## 自有资产」节（契约 10） |
| `tools/berry/src/web.rs` | 双树解析（`/novnc/` 前缀分叉 + 双基准 canonicalize 防线）、`sanitize_path` 默认页 `index.html`、`find_web_root` 双哨兵与 hint、三个 `build_page_url*` 页路径 `/?`（params/fragment 零变化）、`strip_fragment` 纯函数、模块头注释更新（双树与新页）；测试区改写与新增（U41-U45、合成根补写 index.html） |
| `tools/berry/src/commands.rs` | `vnc_web` 签名增 `open: bool`：URL 打印后 macOS `open` 剥 fragment URL、非 macOS/失败 hint 行、JSON `"opened"` 键；其余输出零变化 |
| `tools/berry/src/main.rs` | 仅 VNC 布线区：`VncWebCmd` 增 `--open`（help 注明 macOS 自动开/他平台打印指引）+ dispatch 传参 |
| `tools/berry/packaging/linux/check-appdir.sh` | web 检查段增 `index.html` 在场检 + NOTICE `## 自有资产` 行 grep（契约 10） |
| `tools/berry/README.md` | 命令树行（--open/新页）、wave 段落追加第九波（新默认页/逃生门/移动端指引/--open）、Chrome 验证段 URL 例改 `/?` 形态 + 逃生门例 |
| `docs/Berry设备平台-CLI-first设计-v1.0.md` | 升 v1.6：头块版本行 + 【v1.6】沿革行；§8.1 命令树 `vnc web` 行；§8.2 新增「VNC 网页桌面产品化已落（v1.6）」段；§14 表新增「VNC 网页桌面产品化（已落，v1.6）」行 |
| `docs/阶段沿革.md` | 新节三百三十（五段式：刀面/宿主/客机/实测/未落；节号落笔时实时重数尾部复核唯一） |
| `.agents/notes/implemented/2026-11-14-vnc-web-polish-wave9.md` | implement 阶段新建（Problem/Decision/Alternatives/Consequences/Related） |
| `out/` | e2e 截图与 DOM 留档（`vnc-w9-` 前缀；口令样串不落仓） |

不改：`Cargo.toml`/`Cargo.lock`（零新依赖）、`tools/berry/web/novnc/` 全树（vendored 红线）、三平台打包脚本（整目录拷贝已覆盖）、`justfile`（三冒烟配方不断言 web URL 形态，已核——只断言 status 的 uri/ws_uri 与 check 退出码）、`tools/berry/src/` 其余模块、冻结面全列（背景节声明）。

## 测试矩阵

### 单元测试（cargo test，98 基线全保 + 新增 U41-U45）

| 编号 | 面 | 断言 |
|---|---|---|
| U41 | 三 URL 构造 `/?` 形态 | `build_page_url`/`build_page_url_https`/`build_page_url_tls` 逐字符钉死：页路径 `/?autoconnect=true&…`，query 参数与顺序、percent-编码、`path=` 在场空值、`resize` 值、`#password=` fragment 规则（含奇字符编码）与改前逐字节同构仅路径段不同 |
| U42 | sanitize 双树 | `/`→`index.html`；`/index.html`、`/berry.js`、`/NOTICE.md`→原样；`/novnc/vnc.html`、`/novnc/core/rfb.js`→novnc 树；裸 `/vnc.html`→`vnc.html`（own 树字面，404 属 fs 层语义）；遍历/反斜杠/冒号/坏转义负例扩到 `/novnc/../../x`、`/novnc/%2e%2e/x`；既有负例全保 |
| U43 | http_round_trip 双树 | 合成根写 index.html+novnc/vnc.html：GET `/` 200 text/html 含 BerryOS 标记；GET `/novnc/vnc.html` 200 含 noVNC 标记（逃生门钉死）；GET `/NOTICE.md` 200 text/plain；404/400/405/HEAD/预算/准入/TLS 各既有测试适配后全绿 |
| U44 | find_web_root 双哨兵 | 哨兵目录只含 novnc/vnc.html → Err（hint 提双件）；含双件 → Ok；仓库根走查断言扩到 index.html |
| U45 | strip_fragment | 有 fragment（`…#password=x`）→ 剥至 `#` 前；无 fragment → 原样返回；空串边界 |

### Chrome headless 端到端（c1-c9；方法沿 probe f2/f3：`--headless=new --dump-dom/--screenshot --virtual-time-budget`，https 形态加 `--ignore-certificate-errors`；脚本 `/tmp/berry-w9probe`，产物 `out/vnc-w9-*`；run 全程隔离 BERRY_HOME、显式 `--run`、自起自收、Chrome 进程杀净）

| 编号 | 项 | 预期 |
|---|---|---|
| c1 | 明文 run 默认页（`/?…path=<ws>` 形态） | dump：body `berry-connected` + `#berry-status` 文案含「已连接」+ 五按钮 id + `#berry-res` 非空；`resize=off` 版 `--screenshot` 画布像素与 `berry screenshot` 调色板对拍（wave5 先例）；`berry-wakelock` 类在场 |
| c2 | `--tls` 施明文 run（https host/port/encrypt=false 形态） | 同 c1 断言集（含 `--ignore-certificate-errors`） |
| c3 | TLS run（https host/port/encrypt=true + wss 代理形态） | 同 c1 断言集 |
| c4 | raw 逃生门回归 | `/novnc/vnc.html?autoconnect=true&path=<ws>&resize=scale` dump 含 `noVNC_status_normal`「已连接」（vendored 页零回归） |
| c5 | 断流自动重连终态 | 钉 `--vnc-ws` 口起 run → 页面连上 → `device stop` 杀后端 → 同口重启 run → `--virtual-time-budget=25000` 终态 dump 回 `berry-connected`（probe f3 已证可行；「重新连接中」中间态单 dump 不可捕，如实登记验证层级） |
| c6 | 移动视口自适应 | `--window-size=390,844` dump：body `berry-compact` 在场；`--window-size=1280,800` 对照缺席；390x844 截图留档（<500px 像素级观感不可测，f3） |
| c7 | view-only 初值与死端点错误态 | `&view_only=1`：body `berry-viewonly` + 按钮 `aria-pressed="true"`；静态页 `path=ws://127.0.0.1:1/dead`：终态 dump `berry-reconnecting` 且倒计时文案在场（自动重连默认武装） |
| c8 | 口令 run 凭据链 | `--vnc-pass` run 的 URL（fragment 带口令）连上 `berry-connected`；同 URL 去 fragment 手动追加 → dump 出现 `#berry-cred-dialog`（凭据弹窗兜底） |
| c9 | `--open` JSON 契约 | 隔离 run 后台 `vnc web --open --json`：JSON 含 `"opened"` 键（macOS true——会真开一次浏览器标签，已知噪音登记；他平台 false）+ URL/serving 字段不变 |

### 打包与全量门

| 编号 | 项 | 预期 |
|---|---|---|
| p1 | Linux AppDir 重建 | `build-appdir.sh` → `check-appdir.sh` 0 fail（含 index.html 与 NOTICE 两新检，检数 +2）；AppDir `usr/share/berry/web/` 内三件 + NOTICE 增节在位；macOS/Windows 整目录拷贝机制不变（零脚本改动，低风险登记，可顺手重建 macOS 幂等对照） |
| g1 | 单测 | `cd tools/berry && PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/cargo-target" cargo test --release --target "$(rustc -vV | sed -n 's/^host: //p')"` 全绿（98 + U41-U45 家族；终值以实现期实数登记） |
| g2 | 构建/冒烟门 | 仓库根 `just berry-build`（docs_guard）绿；`just berry-smoke`/`berry-vnc-smoke`/`berry-vnc-tls-smoke` 绿（零 justfile 改动的回归证明） |
| g3 | 文档门 | `node .agents/tools/check-docs.mjs` 零告警（开工基线 226 篇 0 条；本波新增计划/Agent Note 后篇数自增仍 0） |

## 验收标准

1. 新页生效且三形态全通：c1-c3 三形态 `berry-connected` + 状态栏/工具栏 DOM 契约齐；打印 URL 仅页路径段变化（U41 逐字符钉死），`--tls`/TLS run 分支行为与 hint/警告零回归。
2. 逃生门：c4 vendored 页直访可连；novnc/ 全树（127 文件）零字节改动（`git status` 证）。
3. 自动重连：c5 终态回连 + c7 死端点重连态与倒计时在 dump；手动断开/重连键语义如契约 5（有头或代码走查佐证，点击面如实降级）。
4. 移动端：c6 compact 断点类两向断言 + 390x844 截图留档；软键盘/全屏/真机手势按 f3 降级记录（不虚报）。
5. 自有资产合规：三件为 Apache-2.0 自有资产、零 noVNC 文本拷贝（仅 import core 三模块）；NOTICE 自有资产节在位；check-appdir 0 fail（p1）。
6. `--open`：c9 JSON `opened` 键 + macOS 真开（噪音登记）；URL 剥 fragment（U45）；非 macOS 打印指引不误报。
7. 单测与门：g1-g3 全绿；check-docs 零告警。
8. 文档面齐：README（新页/逃生门/移动端/--open/Chrome 段）、CLI-first v1.6（头块/§8/§14）、沿革三百三十（节号落笔时复核唯一）、Agent Note（Alternatives 真实）；本计划收口 `done`。
9. 纪律面：不 git add/commit；冻结面零触碰；冒烟自起自收、隔离 BERRY_HOME、Chrome/QEMU 进程杀净；口令样串不落仓（截图/DOM 留档前清洗 fragment 与口令字段）；并行会话零触碰。

## 边界（非目标）

- 不改 `novnc/` 任何文件（MPL/BSD 混改红线）；只 import `core/rfb.js`、`core/util/logging.js`、`core/input/keyboard.js` 三个模块。
- 不做剪贴板、SetDesktopSize/remote 缩放档、SASL、文件传输、会话录制（沿 wave5/6 边界）。
- 不做服务端渲染或模板引擎（std 静态服务保持）；零新 Rust 依赖；页面零外部 CDN/字体/运行时（离线优先，系统字体栈）。
- 不做 LAN 绑定与 QR（契约 8 砍项）；不做 PWA manifest/service worker（本地一次性服务无意义，`.webmanifest` mime 不补）。
- 不做帧率显示（core 无公开面）；不做手动连接表单（URL 参数是唯一入口）；不做 serververification/bell/clipboard/capabilities 的专用 UI。
- IME/组合输入不翻译（软键盘按键级转发为上限）；真机触摸手势、真 OSK 弹出、真全屏、<500px 像素级渲染、重连中间态——headless 不可测面如实降级记录，不虚报验证层级。
- 沿革已提交内容不回改（新节文末追加）；`.agents/research/` 与提交拆分清单零触碰（本波新增件提交归属由用户归列）。

## 偏差

写作时点已知的与 SPEC 直直觉的出入（implement 阶段如有新增在此续记）：

1. **QR 砍项而非三选一落地**：SPEC 预设「可行=自写或 vendored 极小 MIT 生成器」；probe f4 定音 loopback-only 架构下手机扫码是死路（服务端与 QEMU ws 均 127.0.0.1），技术可行不等于价值可行——砍项记录，重启条件=LAN 绑定模式立项 + QR 内容剥 fragment。
2. **帧率砍项**：SPEC「尺寸/帧率若可得」——尺寸经 canvas 属性可得（保留），core 无公开帧率面、rAF 自算只是页面帧率非 VNC 帧率，不做假指标。
3. **`--open` 是 macOS-only 自动开且 URL 剥 fragment**：非全平台自动开；口令不进 `open` 一次性 argv（与既有口令 argv 口径一致），有口令场景由页面凭据弹窗兜底——安全优先于一步到位。
4. **移动断点 700px**：产品上偏宽（覆盖小平板竖屏），动机之一是 headless innerWidth 钳 500（f3）——断点 ≤700 才能在 390x844 窗口触发断言；<500px 观感不可测如实降级。
5. **重连中间态不可断言**：f3 实证单次 dump 捕不到过渡文案，e2e 判据=终态 + 重连态（c5/c7 两态合证），不是完整时序证明。
6. **交互点击 headless 无面**：全屏/view-only/缩放/键盘/断开五键的点击链路不可 headless 驱动——经 URL 参数初值（view_only/resize）+ 状态类 + 按钮在位断言替代，点击行为留有头验证；全屏真行为 headless 权限拒（f3）。
7. **自有资产放 web 根三件**（非 `web/berry/` 子目录、非塞 novnc/）：根直放少一层路由与 import 路径改写；NOTICE 增节点名三件。
8. **裸 `/vnc.html` 旧 URL 退役为 404**：不做 raw=1 参数别名、不做旧路径重定向——工作树态无已发布兼容包袱；逃生门=直访 `/novnc/vnc.html`（README 记录）。
9. **e2e 不进 just 配方**：Chrome headless 属外部依赖（wave5/6 同裁），c1-c9 为实现期探针脚本（/tmp）+ out/ 留档，三 just 冒烟保持纯 CLI 链。
10. **实现期新增：QEMU ws 端点只认根路径**——`ws://host:port`（空路径）升级悬挂、`ws://host:port/` 秒连（探针净对照钉死）；新页对 `path=` 形态一律过 `new URL().href` 规范化（vendored app 同型语义），form 2/3 构造自带尾斜杠。
11. **实现期新增：Chrome 虚拟时间 ws 消息悬挂与音频接地**——`--virtual-time-budget` 下 101 完成但渲染器不泵后续 ws 消息（netlog：我方 335B vs vendored 382KB）；归因 vendored 页 `<audio preload="metadata">` 的 pending 媒体请求挂住虚拟时钟。壳页以自产铃音（`berry-bell.wav` + RFB `bell` 事件发声，产品正当特性而非 UI）复得同机理，另加 8s 公有面看门狗拆真实死挂连接。先后否决：500ms 拨号延迟（假信号）、并发 fetch 接地、合成器动画、img 预载、`--timeout` 真实时间模式（载完即 dump）。
12. **实现期插曲：`just berry-build` 的 docs-guard 段曾遇 G10 失败**——`docs/索引.md` 六区标注 16 ≠ 行实数 17；该文件当时与 HEAD 逐字节一致（漂移为并行会话已提交态，守卫把六区后非编号节的表行并入六区桶），不在本刀允许面；收口前并行会话已修复（索引 6 区 123 行对齐），`just berry-build` 终态 rc=0 全绿——本刀零动作，仅记录经过。
13. **自有资产五件非三件**：`berry-logo.svg`（品牌标识，img 预载实验的正当化保留）与 `berry-bell.wav`（自产合成铃音）随主件入 NOTICE 自有资产节；check-appdir 保持计划钉死的两检（index.html + `## 自有资产` 行）不扩。

## 未完成事项

_（无——headless 不可测面与 QR/LAN 重启条件均已按偏差 6/8/11 如实降级登记；docs-guard G10 漂移归并行会话见偏差 12。）_

## 验收记录（2026-11-15，verify 补跑）

编排与环境：单路 verify 智能体独立复验 w1-w5，全程隔离 `BERRY_HOME=/tmp/berry-w9v2/home`（冒烟另用 `/tmp/berry-w9v2/gates`），二进制经仓库根 `just berry-build` 重建（docs_guard PASS 0 警），QEMU 11.0.3、Chrome 157 headless（`--headless=new --dump-dom/--screenshot --virtual-time-budget`，https 形态加 `--ignore-certificate-errors`）；口令样串只存在于 /tmp 与宿主进程 argv，不落仓。verify 脚本经四轮迭代返工，全部失败轮均归因 verify 自身 harness 缺陷（详见下「产品缺陷」条），产品面 w1-w5 判据终态全过。

w1 桌面端到端：desktop 档 `device start --profile desktop --vnc on --vnc-ws on --vnc-pass <pw>` 起 run，`vnc check --ws` 报 `ok 1088x1920 … (auth vnc)`（ServerInit 宽度为 16 对齐 stride，页面画布按实际几何 1080×1920）；`vnc web` URL 为 `http://127.0.0.1:P/?autoconnect=true&path=<ws>&resize=scale#password=…`（`/?` 形态 + fragment）；Chrome dump-dom 断言全过——body `berry-connected`、状态栏品牌 `BerryOS`、会话名 `QEMU`（desktopname 事件）、「已连接（未加密）」、`#berry-res` 文案 `1080×1920`、工具栏五键 id 全在、`berry-wakelock` 类在场；`--screenshot`（1080x2000 窗、resize=off）落档 [out/vnc-w9-browser-page.png](../../out/vnc-w9-browser-page.png)，画布区 88% 为 navy `(0,0,128)` 加黑底带——真实帧缓冲渲染（壳页深色灰阶与 Chrome 错误页白底皆不可能产出 navy）；调色板对拍两份独立 `berry screenshot` 真值的 navy 态（1,319,678 px 与 1,424,657 px navy）覆盖度均 1.0000（跨时刻同态对拍，同瞬对拍降级见下条）；逃生门 `/novnc/vnc.html?autoconnect=true&path=<ws>&resize=scale#password=…` 直访 dump 含 `noVNC_status_normal` 与「已连接」（vendored 页零回归）；路由面 curl 实测：`/`、`/index.html`、`/berry.js`、`/berry.css`、`/berry-logo.svg`、`/berry-bell.wav`、`/NOTICE.md` 全 200，裸 `/vnc.html` 404（偏差 8 语义），`/novnc/../../etc/passwd` 400。

w1 同瞬对拍降级与手机档补证：desktop 档 guest 桌面持续跑合成按键 demo（run 的 serial.log 有 `switches=N` 计数流），可见内容态为 1-3 秒瞬态、间隔不可预测；四种采样协议（紧邻夹拍、267 帧密采、tap 触发窗、内容触发快拍）约 8 分钟采样未捕获多色同瞬重合——按验证层级如实降级记录，非产品缺陷（phone 档桌面静态，implement 期 c1 与 wave5 均已同瞬对拍过）。补手机档正式同瞬对拍落档 [out/vnc-w9-browser-page-phone-paired.png](../../out/vnc-w9-browser-page-phone-paired.png)：browser 画布区对 truth 单向色覆盖 1.0000，灰阶系像素数两侧几乎一致（`(152,152,152)` 13822/14329、`(190,195,208)` 402/403、`(126,135,161)` 311/345）——画布为帧缓冲逐像素级 blit 的直接实证。

w2 三形态：形态 1 明文 `path=<ws>`（带 fragment 口令透传，同 w1 run 连接即证）；形态 2 `--tls` 施明文 run 起 https 页，URL 形态 `https://127.0.0.1:P/?autoconnect=true&host=127.0.0.1&port=<ws>&encrypt=false&path=&resize=scale#password=…`（host/port/encrypt=false&path= 在场空值，case 模式钉死）+ dump `berry-connected`「已连接（未加密）」；形态 3 TLS run（独立 certs + `--vnc-tls` + `--vnc-pass`）URL 形态 `encrypt=true&path=&resize=off` + `proxy_port` 代理 + dump `berry-connected`「已连接（已加密）」+ fragment 口令透传；错口令（fragment 替换为错串）dump：body `berry-error` + 状态文案「连接失败：口令被拒绝」+ 自有凭据弹窗 `#berry-cred-dialog` 可见（hidden 属性移除）——契约 5/6 语义全中。

w3 断流自动重连与 --open：钉 `--vnc-ws <port>` run 先证 pre-kill `berry-connected`；死端点 `path=ws://127.0.0.1:1/dead` 终态 dump `berry-reconnecting` + 倒计时文案在场（自动重连默认武装）；`device stop` 杀后端、同口重启 run 后终态 dump 回 `berry-connected`「已连接」（c5 同法）；live→dead 中间态在单次 dump 不可捕（虚拟时钟先于断流观测耗尽，按计划偏差 5 层级降级，c5+c7 合证）；`--open` 以 PATH 前置伪 `open`（记 argv）实跑——JSON `"opened": true`，伪 open 收到的 argv URL 与 JSON `url` 去 fragment 值逐字节相等且不含 `#password=`（口令不进一次性 argv 的直接证据），另跑一次真实 macOS `open`（真开一页浏览器标签，已登记噪音）JSON 同报 `opened=true`。

w4 移动视口与 view-only：`--window-size=390,844` dump——body `berry-compact` 在场 + viewport meta `width=device-width` 在位 + 五键 id 全在 + 仍 `berry-connected`；`--window-size=1280,800` 对照 `berry-compact` 缺席；`view_only=1`（参数置于 `#password=` fragment 之前拼接）dump——body `berry-viewonly` + `#berry-btn-viewonly` `aria-pressed="true"` + 仍连接（按钮态与连接参数初值两级断言，点击链路按偏差 6 留有头）。

w5 全量门：`cd tools/berry` 后 `PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/cargo-target" cargo test --release --target aarch64-apple-darwin` → 100 passed / 0 failed（98 基线 + U44/U45）；仓库根 `just berry-build` rc=0（docs_guard PASS 0 警）；`node .agents/tools/check-docs.mjs` 235 篇 0 告警；三冒烟在 `BERRY_HOME=/tmp/berry-w9v2/gates` 隔离下全绿——`just berry-smoke`（doctor `ok:true` 且 home 指向隔离目录、marker `BerryOS boot`、shot 800x600）、`just berry-vnc-smoke` 5/5（uri+ws_uri、口令三态含错串 rc=7）、`just berry-vnc-tls-smoke` 全链（certs 生成、tls.enabled+wss、auto/`--ca`/`--insecure`+警告、错口令 rc=7、错 CA rc=7、`--ca`+`--insecure` rc=2）。

自清与纪律：收尾两隔离 home `berry device list` 均 `no devices`，pgrep 无 berry-w9v2 相关 berry/qemu/Chrome 残留（扫描所见 3 个 qemu 属 `/tmp/oh-shell-gui8` 并行会话，未触碰）；共享 `~/.berry` 全程零触碰（其 run 目录最新 mtime 仍为 2026-11-14 午后，早于本 verify 会话）；不 git add/commit；out/ 仅新增两件留档（`vnc-w9-browser-page.png`、`vnc-w9-browser-page-phone-paired.png`），DOM 断言即时消费不留盘。

产品缺陷：零——verify 四轮返工皆 verify 自身脚本缺陷，逐条在案：①命令替换子壳丢失 RUNS/WEB_PIDS 变量注册致 cleanup 空转（改文件注册修复）；②逃生门测试 URL 丢 `#password=` fragment，vendored 页停「需要凭证」（补 fragment 后连）；③`view_only=1` 拼接在 fragment 之后未入 query（调整拼接收过）；④陈旧 server 输出文件与 spawn 截断的竞态致 URL 读空（父进程预截断修复）。另两观察项非缺陷：desktop 档 guest 桌面为合成按键 demo（同瞬对拍降级的根因，见上）；`vnc check --ws` 报 1088 宽为 RFB ServerInit 的 stride 对齐值，与页面画布 1080 几何各自正确。

docs 收口（2026-11-15 续刀）：implement 期 docs 面核对在位（README 命令树/第九波段落/Chrome 段 URL 例与逃生门、CLI-first 头块 v1.6+§8.1+§8.2+§14、沿革三百三十、Agent Note、[web/NOTICE.md](../../tools/berry/web/NOTICE.md) 自有资产节五件点名）后补齐残余——README 已知限制段新增 VNC 壳页 headless 硬边界一条（全屏真行为/真 OSK 与 IME/真机触摸/<500px 观感/重连中间态时序，替代断言法与有头验证归宿）；CLI-first §15「浏览器零安装直连」行追注 v1.6 产品化（默认页换自有壳页/逃生门/--open，动作列原「vendored noVNC」限定同步修正）；沿革三百三十一新节（verify 复验+docs 收口，五段式，引 w1-w4 证据与 `out/vnc-w9-browser-page.png`/`out/vnc-w9-browser-page-phone-paired.png`，节号落笔实数复核唯一）；Agent Note 补两条 Alternatives（改造 vendored noVNC app 层 MPL 不可、双页并行默认不动均否决）；步骤 6 实况同步本段；check-docs 复跑 0 告警。真实遗留维持「无」——headless 不可测面与 QR/LAN 重启条件均为既定降级登记，非未完成项。
