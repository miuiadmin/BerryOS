# VNC 第六波「合规随包与通路补全收尾」实施计划（第三方通知单源化/vnc check --ws/--tls 明文分支修复）

Type: plan
Status: done
Updated: 2026-11-14

一句话说明：收掉 [wave5 计划](2026-11-13-vnc-complete-wave5.md)的四条真实遗留——第三方许可通知从 Linux heredoc 内嵌抽出为单源文件并三平台随包、`vnc check` 增 `--ws` 自检浏览器（ws/wss）通路、按 probe 定音修复 `vnc web --tls` 施于明文 run 的 https 页分支、CLI-first 索引行升 v1.5——零新增 Cargo 依赖（ws 帧层与 SHA-1/base64 全手写，先例 `display/des.rs`）。

## 背景

VNC 五波已全部落地（工作树态，未 commit；单测 83 绿、四门全绿）：对外暴露与认证 → noVNC 网页桌面/TLS VeNCrypt/审计/口令生命周期 → 三平台打包带 web 资产 → 遗漏检查 P0/P1 清零 → 彻底完善（rustls 0.23：`vnc check` 过 VeNCrypt、`vnc web --tls` https 页 + wss 终止代理、F16/F18/F20、Linux 链实机化）。wave5 收口时留下四条真实遗留，即本波主料：

1. 第三方通知现状：`THIRD-PARTY-NOTICES.txt` 内容以 heredoc 内嵌在 `packaging/linux/build-appdir.sh`（现 332-354 行），仅进 Linux AppDir；macOS/Windows 两平台三脚本不带——合规不一致，且 heredoc 内嵌违反一事实一归属（许可事实的家应在 packaging 树内单一文件，脚本只拷贝）。
2. ws 面无 CLI 自检：`vnc check` 只拨 TCP（明文/VeNCrypt-TLS），浏览器通路（QEMU `websocket=` 口）无自检手段；wave5 probe 实证该口 = 裸 RFB over ws（`Sec-WebSocket-Protocol: binary` 回显、server 帧不 mask、client 帧必须 mask），wss 口 = TLS 包住后字节序列与 ws 完全相同。
3. `vnc web --tls` 明文 run 分支悬置：https 页 + `path=ws://…` 直连形态浏览器实测未通过，分支保留并按未验登记——本波 probe 已定音根因并给出修复形态（见契约 3）。
4. 索引行落后：`docs/索引.md:102` 停在 `· 1.3`，文档本体已 v1.4（wave5 遗留第 1 条自认「不在允许面」，本刀已明示放开）。

wave6 前置 probe（2026-11-14，仓库零写入，产物留档 `/tmp/berry-w6probe/`）五项定音：

- **f1（ws 字节级清单）**：Upgrade 握手逐字节钉死——`GET /` + `Upgrade/Connection/Sec-WebSocket-Key/Version 13`，应答 `HTTP/1.1 101` + `Sec-WebSocket-Accept`（SHA1+magic 复核通过）；QEMU 的 `Date` 头在 zh 系统是本地化字节（非 RFC1123 ASCII），头解析必须字节宽容。帧形态：server 帧恒 binary（op=2）、FIN=1、从不 mask，大数据按 8192B 整切帧（字节流分块，非 RFB 消息边界）；client 帧必须 mask——未 mask 帧被 server 以 close 1002 `client frames must be masked` 硬执法。`vnc.connected` 事件在 TCP accept 即发，不能当 upgrade/RFB 完成证据，判据须用 RFB 语义（ServerInit）。不带 `Sec-WebSocket-Protocol` 也回 101 且不回显。wss 口 = TLS 1.3 包住后字节序列完全同 ws，RFB 层为明文 `003.008` + security types `[1,2]`（无 VeNCrypt 层，DES 认证直走）。
- **f2（--tls 明文分支定音，直接裁决 SPEC 第 3 条）**：现行 `path=` 形态在 https 页失败且 2/2 复现——根因是 vendored noVNC 1.7.0 `app/ui.js` `connect()` 对 `path=` 形态无条件 `url.protocol = (location.protocol === 'https:') ? 'wss:' : 'ws:'`，绝对 `ws://` URL 也被强改成 `wss://` 对明文口做 TLS 握手，QEMU 对 ClientHello 零应答即死——非混合内容拦截。`host=<dial>&port=<ws_port>&encrypt=false&path=`（在场且空值）形态 3/3 全过（DOM「已连接（未加密）到QEMU」+ events connected→initialized→disconnected）——Chrome loopback 混合内容豁免确实覆盖 noVNC 的 host/port 拨号。**裁决：修，不收窄。**
- **f3（打包面）**：heredoc 全文与拷贝点录齐（见契约 1）；`check-appdir.sh` 只查 web 面五件（现 319-326 行），通知存在性与内容均无人核对；三平台脚本幂等口径 = 整目录重建（`rm -rf` / `Remove-Item`），四脚本均已算 `ROOT`（先例 `DESKTOP_SRC`）。
- **f4（索引行）**：格式惯例 = 标签列中段 `vN.M <日期> <一句话>` 追记 + 尾缀 `· N.N`，文件名永不改名；F06 先例（wave4 survey）即此形态。
- **f5（沿革与基线）**：`docs/阶段沿革.md` 最新节号二百五十六已被占用两次（本线 VNC 第五波 + 并行线调度优化轮三），本波新节 = 二百五十七，按 wave3 撞号治理先例追加文末；`check-docs` 基线 = 检查 214 篇 / 告警 0 条（并行会话已清零存量），本波文档门 = 零基线零新增。

`window.rs`/`profile.rs` 仍为并行会话未提交 M 态（冻结），解冻换 `--vnc` 直参顺延，本波零依赖（契约 5）。

## 契约

### 1 第三方许可通知单源化

**单源文件**：新建 `tools/berry/packaging/THIRD-PARTY-NOTICES.txt` 为唯一事实源。内容 = 现 `build-appdir.sh` heredoc 正文逐字节保真（rustls 0.23 Apache-2.0 OR ISC OR MIT / rustls-pki-types 1.x MIT OR Apache-2.0 / rustls-webpki 0.103.x ISC / ring 0.17 Apache-2.0 AND ISC（cc 构建族注记）/ once_cell 1.x MIT OR Apache-2.0 / zeroize 1.x Apache-2.0 OR MIT / subtle 2.6.x BSD-3-Clause 点名 / crates.io 指路行）+ 追加一段 noVNC 指引：vendored noVNC 非静态链入二进制，其许可分层随包在 `web/NOTICE.md`（一事实一归属——noVNC 许可事实的唯一归属是 `tools/berry/web/NOTICE.md`，通知文件只指路不复述）。

**三平台拷贝落点与形态**（四脚本统一源路径 `$ROOT/tools/berry/packaging/THIRD-PARTY-NOTICES.txt`，先例 `DESKTOP_SRC=tools/berry/packaging/linux/berry.desktop`；`build.ps1` 用 `Join-Path $Root`）：

| 平台 | 脚本 | 落点 | 形态 |
|---|---|---|---|
| Linux | `linux/build-appdir.sh` | `usr/share/berry/THIRD-PARTY-NOTICES.txt`（既有位） | 删 heredoc（现 332-354 行）改 `TPN_SRC` 变量 + 存在性门 + `cp -f` + `chmod 0644`（权限口径不变）；tarball 整 AppDir 打包自动随行 |
| macOS | `macos/build.sh` | `.app` `Contents/Resources/THIRD-PARTY-NOTICES.txt` | `cp` + `chmod 0644`（先例行 20 `cp LICENSE`） |
| Windows | `windows/assemble.sh` | 包根 `THIRD-PARTY-NOTICES.txt`（berry.exe 旁） | `cp` + `chmod 0644`；zip 整目录打包自动随行 |
| Windows | `windows/build.ps1` | TargetDir 根 | `Test-Path` 门 + `Copy-Item` |

幂等口径不变：三平台各自整目录重建（`rm -rf` / `Remove-Item -Recurse -Force`），拷贝天然幂等。

**检查器**：`check-appdir.sh` 在 web 检查段（现 319-326 行）后增通知检查——在场（`check_file`）+ 三个关键内容 `grep`（`rustls 0.23` 行、`BSD-3-Clause`（subtle 点名）、`web/NOTICE.md` 指引行），缺任一 = fail，0 fail 门维持。不做与单源的 sha256 对比：检查器面向独立产物（脚本无 `ROOT`、无仓库假设），单源-产物同步性由冒烟 c1..c3 三平台 sha256 对拍验收。

**登记**：`packaging/README.md` 增 THIRD-PARTY-NOTICES 单源惯例一条（源路径、三平台落点、改内容只改单源）。

### 2 `vnc check --ws`（ws/wss 通路的 RFB 自检）

**语法**：`berry vnc check --ws [--run <id>] [--password <pw>] [--ca <dir>] [--insecure] [--json]`。`--ws` 是模式旗标：切换到 run 的 websocket 端点；省略时既有 TCP 路径（明文/VeNCrypt）零变化。

**端点与传输**：`listen.ws` 无 websocket 口 → rc=3 + hint（同 `vnc_web` 现行文案：start with --vnc on --vnc-ws on）。dial = `dial_host(listen.host)`，地址 = `addr_host(dial, ws_port)`，TCP 连接沿用 `connect_retry` 10s 口径。`listen.tls=true` 的 run，ws 口即 wss：先 rustls 客户端握手（复用 `tls.rs` `load_client_config`/`load_client_config_insecure`/`server_name`，零改动），TLS 内字节序列与 ws 完全相同（f1）；RFB 层为明文 `003.008` + types `[1,2]`——无 VeNCrypt 层，与 TCP 路径的 TLS 形态不同但复用同一明文握手机。

**ws 层**（新模块 `tools/berry/src/ws.rs`，std 手写，零新依赖）：

- Upgrade 握手：`GET / HTTP/1.1`（QEMU ws 只认根路径，wave5 偏差 9）+ `Upgrade: websocket` + `Connection: Upgrade` + `Sec-WebSocket-Key`（16B 随机的 base64）+ `Sec-WebSocket-Version: 13`；不带 `Sec-WebSocket-Protocol`（f1 nosub 变证实证 101，与浏览器形态一致）。应答校验：状态行 `HTTP/1.1 101`、`Upgrade`/`Connection`、`Sec-WebSocket-Accept` = SHA1(key + RFC6455 GUID) 的 base64 复核（`expected_accept` 纯函数）。头解析按字节（ASCII 大小写不敏感比较头名），不强解 UTF-8——zh 系统的 `Date` 头是本地化字节（f1）。随机 key 的熵源 = SystemTime 纳秒 + pid + 静态计数器过一遍 SHA-1：握手 key 只需唯一性，非安全随机（本地端点），注释如实声明。
- 帧编解码：client 帧恒 mask（4B mask-key + XOR，f1 硬执法：未 mask 被 close 1002）；server 帧解析 FIN/opcode/长度（7/16/64 位三形态按 RFC 实现，f1 实测只见 7/16 位），server 帧 mask 位出现 = E_PROTOCOL（RFC 6455 §5.1）。binary（op=2）= RFB 字节流；大数据 8192B 分块即字节流分块，`WsStream<S>` 的 `Read` 语义 = 流式（帧边界对 RFB 无意义，FIN=0 宽容续接）；`ping` → 自动回 masked `pong`；`close` → 尽力回 masked close 后按 EOF。`Write` = 整段一条 masked binary 帧（RFB 客户端消息皆小：版本 12B/类型 1B/DES 响应 16B/ClientInit 1B）。
- `WsStream<S: Read + Write>` 泛型内层：明文包 `TcpStream`，wss 包 `rustls::StreamOwned<ClientConnection, TcpStream>`——同帧层零分叉。

**RFB 层（复用结构，不复制逻辑）**：`display/vnc.rs` `handshake_auth` 签名从 `&mut Stream` 泛型化为 `S: Read + Write`（内部 `rfb_tail`/`vnc_auth_exchange`/`read_exact` 已是泛型，尾段无改动），ws 路径直接调 `handshake_auth(&mut ws_stream, password)` 走完版本/认证/ClientInit/ServerInit——与 TCP 路径同一状态机、同一错误语义、同一口令清洗。既有三态单测断言零改动（U39）。

**深度**：到 ServerInit 报尺寸为止，不发 SetEncodings/FramebufferRequest、不拉帧（偏差 2 登记：与 TCP check 的 auth+one frame 不同——noVNC 报「已连接」同为 ServerInit 深度，自检目标 = 浏览器等价通路，且避免 8192B 分块的大流量拉帧）。

**证书目录**：与 TCP TLS 路径完全一致——`--ca` > run `meta.json` `vnc_tls`；缺 → rc=2 + `berry vnc certs` hint；`--ca`+`--insecure` 互斥 rc=2；明文端点给 `--ca`/`--insecure` 静默忽略（F17 先例）。目录解析与警告逻辑从 `vnc_check` 现有 TLS 臂提取为共用函数（TCP 与 ws 两路调用，不复制）。

**输出**：rc=0 文本 `vnc check: ok {W}x{H} via {ws_uri} (auth {auth})`——`ws_uri` = `ws_uri_for(dial, ws_port, listen.tls)`，wss 时 uri scheme 自明，不加 tls 尾注（wss 无 VeNCrypt 子类型）；JSON `{"v":1,"t":"vnc.check","run":…,"ok":true,"uri":"ws://…"|"wss://…","width":W,"height":H,"auth":…}`。失败形态：升级被拒/坏帧 → rc=7 E_PROTOCOL；DES 口令错 → rc=7 + 既有 passwd hint；wss TLS 失败 → rc=7 + `--ca`/`--insecure` hint（同 TCP）。

**hint 文案同步**：`vnc web` 明文模式（无 `--tls`）文本输出在 serving 行后追加一行 `hint: self-test the browser path with berry vnc check --ws`（JSON 输出不动；TLS run 模式不加——其浏览器路径走 berry 代理，`check --ws` 测的是 QEMU ws 口直连，不等价）。

### 3 `vnc web --tls` 明文 run 分支终态：修复（f2 定音）

- **修法**：`web.rs` `build_page_url_https` 从 `path=<ws uri>` 形态改为 host/port/`encrypt=false` 形态（`build_page_url_tls` 的 `encrypt=false` 镜像），签名 `(web_port, ws_uri, password)` → `(page_port, host, ws_port, password)`；输出 `https://127.0.0.1:{page_port}/vnc.html?autoconnect=true&host={dial}&port={ws_port}&encrypt=false&path=&resize=scale` + 既有 `#password=` fragment 规则。`path=` 必须在场且值为空（缺席默认 `websockify` → QEMU 根路径 404，wave5 偏差 9）；`resize=scale` 沿明文 run 语义（f2 定音实测形态）。调用点 `commands.rs` `vnc_web` 的 `--tls` 明文分支（现 1693-1694 行）改传 dial/ws_port。
- **根因改写（文档面）**：wave5 的「实测未通过」悬置登记删除（README 已知限制行、CLI-first、沿革不重复——沿革是历史只追加；README 与设计文档按现态改写）：失败根因 = noVNC 1.7.0 `app/ui.js` 对 `path=` 形态无条件强改 scheme 为 wss（绝对 `ws://` URL 也被改），Chrome 对明文 ws 口做 TLS 握手、QEMU 对 ClientHello 零应答即死——非混合内容拦截；`host/port/encrypt=false` 形态的 scheme 由 `encrypt` 决定，不被页 scheme 重写；Chrome loopback 豁免覆盖 noVNC 的拨号（f2 3/3）。此事实进 `web.rs` 模块头注释与 README。
- **保留**：非 loopback dial 的一次性 mixed-content 警告（现行代码不动）；明文 run 默认（无 `--tls`）行为除新增 hint 行外零变化；TLS run 主路径（https 页 + wss 终止代理）零触碰。
- **实证**：c6——明文 run + `vnc web --tls <dir>` 后台 + Chrome headless `--dump-dom` 3/3 命中「已连接（未加密）到QEMU」+ `--screenshot` 留档 `out/`。

### 4 索引行升版（`docs/索引.md:102`，本刀已放开）

CLI-first 设计文档随本波升 v1.5（§8 `check --ws` 与 `--tls` 分支终态、§12 三平台通知随包、§15 现状映射——改动为 CLI 面新增旗标 + 分发合规面，够 v1.5 量级；docs 阶段落笔即定）。索引行随升，F06 先例形态：

- 标签列：中段注记刷为 `v1.5 2026-11-14 VNC 六波已落`、尾缀 `· 1.5`（现行 `v1.3 … · 1.3` 为落后一版的存量，一次刷齐，只写当前状态）。
- 摘要列追加一句：`v1.5：VNC 六波收尾——第三方通知单源随包三平台、vnc check --ws 浏览器通路自检、--tls 明文 run https 页分支修复（§8/§12/§15 同步）`。
- 文件名永不改名；终态文案以 docs 阶段落笔为准。

### 5 `window.rs` 冻结直参顺延

`window.rs`/`profile.rs` 仍为并行会话未提交 M 态，`--vnc` 走 `LAUNCH_STICKY` 进程级覆盖的现状不动，解冻换直参继续顺延——本波计划、实现与验收均不依赖解冻；沿革新节未落段记一句。

## 步骤总览

1. [x] 合规面：单源文件新建（heredoc 正文逐字节迁移 + noVNC 指引段）→ `build-appdir.sh` 删 heredoc 改 cp → `check-appdir.sh` 增检查 → macos/Windows 三脚本补 cp → `packaging/README.md` 登记。
2. [x] `ws.rs` 新模块（SHA-1/base64/`expected_accept` 纯函数、Upgrade 握手、帧编解码、`WsStream`）——单测先行（U34-U38）。
3. [x] `handshake_auth` 泛型化（U39 回归）→ `commands.rs` `vnc_check` 加 `--ws` 分支（含 TLS 配置解析共用提取）→ `main.rs` `VncCheckCmd` 布线 → `vnc web` 明文模式 hint 行。
4. [x] `build_page_url_https` 形态修复（U40 钉死）→ `commands.rs` 调用点适配 → `web.rs` 模块头注释更新。
5. [x] 冒烟 c1..c8 + 全量门三件套。
6. [x] docs 面：`tools/berry/README.md`（通知随包路径/`check --ws` 用法/`--tls` 分支终态）、CLI-first 升 v1.5、`docs/索引.md` 行升版、沿革新节二百五十七（落笔时实时重数尾部）、Agent Note 落稿（Alternatives 真实：heredoc vs 单源文件、ws 手写 vs 引 crate、修复 vs 收窄 `--tls` 明文分支、ServerInit vs 拉帧）、本计划收口 `done`、`check-docs` 零告警。

## 文件改动清单

符号级锚点（行号以落笔时工作树为准，会漂，以符号为准）：

| 文件 | 改动 |
|---|---|
| `tools/berry/packaging/THIRD-PARTY-NOTICES.txt`（新建） | 单一事实源：现 heredoc 正文逐字节 + noVNC 指向 `web/NOTICE.md` 的指引段 |
| `tools/berry/packaging/linux/build-appdir.sh` | 删 heredoc（现 332-354 行），改 `TPN_SRC="$ROOT/tools/berry/packaging/THIRD-PARTY-NOTICES.txt"` + 存在性门 + `cp -f` + `chmod 0644`；注释改指单源 |
| `tools/berry/packaging/linux/check-appdir.sh` | web 检查段（现 319-326 行）后增 `THIRD-PARTY-NOTICES.txt` 在场 + 三关键行 grep（rustls 0.23 / BSD-3-Clause / web/NOTICE.md） |
| `tools/berry/packaging/macos/build.sh` | `cp` 单源 → `$APP/Contents/Resources/THIRD-PARTY-NOTICES.txt` + `chmod 0644` |
| `tools/berry/packaging/windows/assemble.sh` | `cp` 单源 → `$OUT/THIRD-PARTY-NOTICES.txt` + `chmod 0644`（zip 自动随行） |
| `tools/berry/packaging/windows/build.ps1` | `Test-Path` 门 + `Copy-Item` 单源 → TargetDir 根 |
| `tools/berry/packaging/README.md` | THIRD-PARTY-NOTICES 单源惯例登记（源路径/三平台落点/只改单源） |
| `tools/berry/src/ws.rs`（新建） | `sha1`/`b64_encode`/`expected_accept` 纯函数（RFC 6455 §1.3 样例向量为锚）；`upgrade_request` 纯函数与 `handshake<S: Read+Write>`（字节宽容头解析、101/Accept 校验、随机 key）；`WsStream<S>`（`Read`/`Write` impl：client 帧 mask、server 帧解 7/16/64 位长度、ping→pong、close→EOF）；`encode_client_frame`（mask-key 注入可测）/`read_server_frame`；单测 U34-U38 |
| `tools/berry/src/lib.rs` | 模块表追加 `pub mod ws;`（字母序尾） |
| `tools/berry/src/display/vnc.rs` | `handshake_auth` 签名 `&mut Stream` → `S: Read + Write` 泛型化；`connect_auth`/`connect_tls`/`VncInput::connect_auth` 调用点适配；既有三态单测断言零改动 |
| `tools/berry/src/commands.rs` | `vnc_check` 签名加 `ws: bool`：ws 分支（端点 `listen.ws`/rc=3、`connect_retry`、TLS run 的 wss 包装、`handshake_auth` 复用、输出契约）；TLS 配置目录解析 + `--insecure` 警告提取为 TCP/ws 共用函数；`vnc_web` `--tls` 明文分支（现 1693-1694 行）改 `build_page_url_https` 新签名；明文模式文本输出追加 hint 行 |
| `tools/berry/src/main.rs` | 仅 VNC 布线区：`VncCheckCmd` 加 `--ws`（help：自检浏览器通路），dispatch 传参 |
| `tools/berry/src/web.rs` | `build_page_url_https` 改 host/port/`encrypt=false&path=`（空值）形态与新签名；模块头注释补 noVNC 1.7.0 `ui.js` scheme 强改怪癖与 loopback 豁免事实；既有单测适配 + 新形态逐字符钉死 |
| `tools/berry/README.md` | 命令树 `vnc check` 行补 `--ws`；`check --ws` 用法段（明文 ws/wss 三路、深度=ServerInit、判据不用 connected 事件）；`--tls` 明文分支已知限制行按终态改写；许可随包段改三平台随包 + 单源路径 |
| `docs/Berry设备平台-CLI-first设计-v1.0.md` | 升 v1.5：头块版本行 + 【v1.5】沿革行；§8（`check --ws`、`--tls` 明文分支终态）；§12（三平台通知随包单源）；§15 现状映射 |
| `docs/索引.md` | 行 102 CLI-first 行升 v1.5（标签列刷齐 + 摘要列追加，契约 4 拟文） |
| `docs/阶段沿革.md` | 新节二百五十七（五段式：刀面/宿主/客机/实测/未落——未落段含 window.rs 顺延句）；节号落笔时实时重数尾部 |
| `.agents/notes/implemented/2026-11-14-vnc-compliance-wave6.md` | implement 阶段新建（写前 `rg` 搜 notes 含 rejected/ 防撞题；Problem/Decision/Alternatives/Consequences/Related） |
| `out/` | 三平台重建产物（AppDir/tarball/.app/win64+zip）与 ws 冒烟截图 |

不改：`Cargo.toml`/`Cargo.lock`（零新依赖——ws/SHA-1/base64 全手写）、`tls.rs`/`des.rs`（纯复用零改动）、`qemu.rs`/`qmp.rs`/`device.rs`/`vnc_cfg.rs`（端点与 meta 布局已满足）、web 资产树、`window.rs`/`profile.rs`/`main.rs` profile hunk（冻结面）。

## 测试矩阵

### 单元测试（cargo test，83 基线全保 + 新增 U34-U40）

| 编号 | 面 | 断言 |
|---|---|---|
| U34 | SHA-1/base64/`expected_accept` | 已知向量：`sha1("abc")`/`sha1("")` 摘要、b64 编码边界（空/1/2/3 字节）、RFC 6455 §1.3 样例 key `dGhlIHNhbXBsZSBub25jZQ==` → `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=` |
| U35 | client 帧编码 | 固定 mask-key 逐字节钉死：小负载 7 位长度、125/126 边界切 16 位、65535/65536 边界切 64 位；mask XOR 应用正确（解码还原 = 原 payload） |
| U36 | server 帧解码 | 7/16/64 位长度；server 帧 mask 位 = 1 → E_PROTOCOL；close(8)/ping(9)/pong(A) 控制帧；FIN=0 分块帧按字节流续接（f1 恒 FIN=1，宽容实现） |
| U37 | Upgrade 握手 | loopback mock：101 + 正确 Accept 过；错 Accept → E_PROTOCOL；非 101（404）→ E_PROTOCOL 带状态行摘录；应答头值含非 UTF-8 字节（模拟 zh Date）仍过 |
| U38 | ws 端到端 RFB | mock ws server（101 + banner + types `[1,2]` + DES 挑战对拍 + SecurityResult 0 + ServerInit）→ `handshake_auth<WsStream>` 全通报尺寸与 auth；wss 臂：`tls.rs` `test_support::certs_dir` 起真 rustls 服务端 + 同流程（先例 `web.rs` `wss_proxy_end_to_end_relay`） |
| U39 | `handshake_auth` 泛型化回归 | 既有三态（none/挑战对拍/auth-fail 清洗）测试断言零改动全绿 |
| U40 | URL 形态 | `build_page_url_https` 新形态逐字符钉死（`encrypt=false`、`path=` 在场空值、`resize=scale`、fragment 编码用例沿既有）；`build_page_url`/`build_page_url_tls` 输出逐字节不变 |

### 冒烟（c1..c8，实机 QEMU 11.0.3；run 全程显式 `--run` 自起自收；探针脚本 `/tmp/berry-w6probe/` 可复用）

| 编号 | 项 | 预期 |
|---|---|---|
| c1 | Linux 链重建 | `build-appdir.sh`（无 heredoc）→ `check-appdir.sh` 0 fail（含新检查）→ `build-tarball.sh`；AppDir 与 tarball 内 `THIRD-PARTY-NOTICES.txt` sha256 与单源同一；`grep -c TPN build-appdir.sh` 证 heredoc 删净 |
| c2 | macOS 重建 | `macos/build.sh`；`.app` `Contents/Resources/THIRD-PARTY-NOTICES.txt` sha256 同一；web 面五件仍齐、`LICENSE` 仍在 |
| c3 | Windows 重建 | `windows/assemble.sh`（用 `out/` 既有双 exe + 镜像）；包根与 zip 内通知 sha256 同一；`build.ps1` 以 pwsh 在场与否定实跑或静态对照（如实登记） |
| c4 | 明文 run `check --ws` | `device start --vnc on --vnc-ws on --vnc-pass <pw> --run <id>` → `vnc check --ws` rc=0 报 `ws://` uri + 帧尺寸真值 + `auth vnc`；`--password` 错串 rc=7；`--ca` 给明文端点静默忽略仍 rc=0 |
| c5 | TLS run `check --ws`（wss） | `vnc certs` + `device start --vnc-tls <dir> --run <id>` → `vnc check --ws`（自动推目录）rc=0 报 `wss://`；`--ca` 显式 / `--insecure`（stderr warning）两路 rc=0；错口令 rc=7；`--ca`+`--insecure` 同给 rc=2 |
| c6 | `--tls` 明文分支修复实证 | 明文 run + `vnc web --tls <dir>` 后台 + Chrome headless `--dump-dom` 3/3 命中「已连接（未加密）到QEMU」；`--screenshot` 留档 `out/`；进程组自收 |
| c7 | 既有回归 | `just berry-vnc-smoke` / `just berry-vnc-tls-smoke` 全绿；明文 run `vnc web` 默认 http 页 URL 形态不变（`path=` 形态，仅多 hint 行）；TCP `vnc check` 三路（TLS run）输出不回归 |
| c8 | 全量门 | `cargo test --release` 全绿（83 + U34-U40）；仓库根 `just berry-build`（docs_guard）绿；`node .agents/tools/check-docs.mjs` 零告警（基线 214 篇 0 条，新增 Agent Note 后篇数 +1 仍 0） |

全量门命令（口径同 wave5）：`cd tools/berry && PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/cargo-target" cargo test --release --target "$(rustc -vV | sed -n 's/^host: //p')"`。

## 验收标准

1. 单源化：三平台包内 `THIRD-PARTY-NOTICES.txt` 在位且 sha256 与单源同一（c1-c3）；`check-appdir.sh` 新检查 0 fail；`build-appdir.sh` heredoc 删净；三脚本幂等重跑产物不变。
2. `vnc check --ws`：明文 run rc=0（ws uri/尺寸/auth）、TLS run wss 自动/`--ca`/`--insecure` 三路 rc=0、错口令 rc=7、无 ws 口 rc=3、互斥 rc=2；TCP 路径输出逐字节不回归；与 TCP 路径共享 `handshake_auth`（无复制状态机）。
3. `--tls` 明文分支：c6 DOM 3/3 + 截图留档；README/CLI-first 的「实测未通过」悬置清零、根因改写落档；非 loopback 警告保留。
4. 单测 83 + U34-U40 全绿；全量门三件套过；`check-docs` 零告警（零新增）。
5. 文档面齐：README / packaging README / CLI-first v1.5 / 索引行 `· 1.5` / 沿革二百五十七 / Agent Note；本计划收口 `done`。
6. 纪律面：不 git add/commit；冻结面（`window.rs`/`profile.rs`/`main.rs` profile hunk/`arch/`/`kernel/`/`sched/`/`user/`/`research/` 等）零触碰；冒烟自起自收（`stop`/`cleanup --run` 只收自己的 run，后台 web/Chrome 进程杀净）；敏感物（证书/口令）只落 `/tmp` 或 run 目录；并行会话 run/QEMU 零触碰。

## 边界（非目标）

- SetDesktopSize 代理翻译、剪贴板、SASL：沿 wave5 边界，另立项。
- noVNC 升版：维持 vendored 1.7.0；`ui.js` scheme 强改怪癖以 URL 形态绕过，不打补丁。
- 真机 Windows/Linux GUI：容器/wine 冒烟为上限；`build.ps1` 真 Windows 主机实跑不在面。
- 沿革历史重号治理：二百五十六双占为已提交内容，只追加二百五十七，不重排。
- Chrome console 日志抓取：headless=new 不出 CONSOLE 行（probe 已登记），判据维持 DOM 状态 + events 形态。
- ws 子协议协商面：不发送不校验 `Sec-WebSocket-Protocol`（f1 nosub 实证 101；浏览器形态一致）。
- Cargo 依赖面：零新增——SHA-1/base64/ws 帧层全手写。

## 偏差

写作时点已知的与 SPEC 直直觉/前波记录的出入（implement 阶段如有新增在此续记）：

1. **`--tls` 明文分支 = 修复而非收窄**：SPEC 预设「能过则修、不能过则收窄」两臂；probe f2 定音 `host/port/encrypt=false&path=`（空值）形态 3/3 全过、根因归 `ui.js` scheme 强改——走修复臂，收窄臂（rc=2 + hint）不实施。
2. **`check --ws` 深度 = ServerInit，不拉帧**：与 TCP check 的 auth+one frame 不同深度——浏览器等价判据（noVNC 同深度报「已连接」）且避免 8192B 分块大流量；README 如实登记差异。
3. **SHA-1/base64 手写**：std 无摘要/编码原语，校验 `Sec-WebSocket-Accept` 必须有 SHA-1——手写（仅握手完整性校验，非安全用途；先例 `display/des.rs` 手写 DES）；不引 crate（零新依赖红线）。
4. **`check-appdir.sh` 不做单源 sha256 对比**：检查器面向独立产物（无 `ROOT` 无仓库假设）；同步性由冒烟 c1-c3 三平台 sha256 对拍验收，检查器只做在场 + 关键行 grep。
5. **`build.ps1` 实跑条件性**：mac 宿主 pwsh 在场则实跑，不在场则与 `assemble.sh` 同构静态对照 + 如实登记（不硬造）。
6. **沿革节号 = 二百五十七**（二百五十六已双占，probe f5 基准）；docs 阶段落笔前必须实时重数尾部（wave5 偏差 6 先例）。
7. **`vnc.connected` 不作 ws 判据**：该事件在 TCP accept 即发（f1 钉板，裸 TCP 连上零字节再关也产生 connected+disconnected 对）——ws 自检判据 = RFB 语义（ServerInit/尺寸），代码注释与 README 同步。
8. **`vnc web` 明文模式文本输出新增 hint 行**：wave5 验收项「明文 run 的 `vnc web` 行为零变化」是当波回归门；本波 SPEC 明示 hint 文案同步，该行是唯一的有意变化（URL/JSON/服务行为不变）。

implement 阶段续记：

9. **c1 首跑 AppDir 用了宿主二进制**：`build-appdir.sh` 缺省取 host release berry（Mach-O），`check-appdir.sh --require-elf` 正确拦下（ checker 行为符合设计）；以 `BERRY_BIN=tools/berry/cargo-target-linux/aarch64-unknown-linux-musl/release/berry`（本波重建的 musl ELF）重跑后 37 ok 0 fail——脚本既有环境覆盖面，无代码改动。
10. **单测新增 13 个而非 7 个**：U34-U40 之外 ws.rs 另加 `ping_is_answered_and_close_is_eof`（控制帧语义钉死）与 `client_frame_small_is_7bit_length`（小负载逐字节形态），总量 83→96。
11. **沿革/check-docs 篇数漂移**：落笔时 check-docs 报 217 篇（probe 基线 214——并行会话同窗新增），本波文档面零新增告警，Agent Note 落盘后复跑仍 0 条。

## 未完成事项

_（无）_

## 验收记录（2026-11-14，verify 补跑）

编排：implement 收口后首排 verify 单体智能体两次尝试均未出结果（工作量过大超时），改拆两路并行各自隔离验收——路A「ws/wss 通路与 --tls 分支」（BERRY_HOME=/tmp/berry-w6v-a/home）与路B「三平台打包合规 + 全量回归」（BERRY_HOME=/tmp/berry-w6v-b/home），两路均 status=ok，零失败路，故无修法与大缺陷可记。两路编号 c1-c5 为 verify 编排自设，按内容归位：路A c1-c3 = 本计划测试矩阵 c4/c5/c6（通路面），路B c4-c5 = c1-c3（打包面）与 c7/c8（回归与全量门）。

路A 环境：全程 export BERRY_HOME=/tmp/berry-w6v-a/home（隔离），二进制经仓库根 `just berry-build` 重建（docs_guard PASS 0 警），QEMU 11.0.3；起始 `berry device list --json` 报 `{"runs":[]}` 证隔离生效，共享 ~/.berry 零触碰。

路A c1 明文 ws check：`berry device start --profile phone --image out/berryos.img --vnc on --vnc-ws on --vnc-pass <pw> --json` 起 run，`vnc status --json` 报 listen={host:127.0.0.1,port:57511,tls:false,ws:57512}、ws_uri=ws://127.0.0.1:57512；`berry vnc check --run <id> --ws` 缺省（读 run 的 vnc.pass）rc=0 输出 `vnc check: ok 480x1000 via ws://127.0.0.1:57512 (auth vnc)`；`--password` 对串 rc=0 同文，错串 rc=7 报 `berry: VNC: auth failed: Authentication failed` + passwd hint；TCP 路径并存（无 --ws）rc=0 输出 `ok 480x1000 via vnc://127.0.0.1:57511 (auth vnc)`；`--ws --ca /nonexistent-dir` 对明文端点静默忽略仍 rc=0。

路A c1 无 ws 口负例：另起 `--vnc on --vnc-pass <pw>`（无 --vnc-ws）run，status 报 listen.ws=null、ws_uri=null；`check --ws` rc=3 报 `berry: no websocket listener in <run 目录>` + hint `start with --vnc on --vnc-ws on (see berry vnc status)`；该 run TCP check 仍 rc=0 via vnc://127.0.0.1:57529；两 run 均 `device stop --run` + `device cleanup --run` 自收。

路A c2 wss 正路：`berry vnc certs` 造两套独立证书（ca sha256 5F:BF:28:… 与 33:F2:42:…，第二套留错 CA 负例）；`device start --vnc 127.0.0.1:57582 --vnc-ws on --vnc-tls <certs-a> --vnc-pass <pw>` 起 run，status 报 tls={enabled:true,vencrypt:x509-vnc}、ws_uri=wss://127.0.0.1:57583、auth=vencrypt；`check --ws` 自动推 run meta 的 vnc_tls 目录 rc=0 输出 `vnc check: ok 480x1000 via wss://127.0.0.1:57583 (auth vnc)`（RFB 内明文 003.008 无 VeNCrypt 层）；`--ca <certs-a>` 显式 rc=0 同文；`--insecure` rc=0 且 stderr 警告 `berry: warning: --insecure skips TLS certificate verification; the connection is open to interception`。

路A c2 三负例与 TCP 回归：`--ca` 指独立第二套 CA（错 CA）rc=7 报 `berry: VNC: TLS handshake failed: invalid peer certificate: BadSignature` + `--ca`/`--insecure` hint；`--password` 错串 rc=7 auth failed；`--ca`+`--insecure` 同给 rc=2 报 `--ca and --insecure are mutually exclusive`；TCP 面 `check`（无 --ws，VeNCrypt）rc=0 输出 `ok 480x1000 via vnc://127.0.0.1:57582 (auth vnc, tls x509-vnc)` 零回归；run stop+cleanup 自收。

路A c3 --tls 明文分支：明文 run + `berry vnc web --run <id> --tls /tmp/berry-w6v-a/certs-web --port 18701` 输出 URL=`https://127.0.0.1:18701/vnc.html?autoconnect=true&host=127.0.0.1&port=57688&encrypt=false&path=&resize=scale#password=…`（host/port/encrypt=false&path= 在场空值形态，grep 断言过）+ serving 行；Chrome headless（`--headless=new --ignore-certificate-errors --virtual-time-budget=8000`）`--dump-dom` 三跑均命中 `noVNC_status_normal">已连接（未加密）到QEMU`（3/3，dom-tls-1/2/3.html）；`--screenshot`（URL 改 resize=off）留档 out/vnc-w6v-a-tls-plain-web.png（PNG 480x1000 8-bit RGB，3954B，签名与尺寸结构校验过）。

路A c3 默认 http 页零回归：同 run `vnc web --port 18702` 输出 URL 为 `path=ws%3A%2F%2F127.0.0.1%3A57688` 编码形态（grep 断言过）且第三行 `hint: self-test the browser path with berry vnc check --ws` 在场；Chrome headless（无 --ignore-certificate-errors）dump-dom 同命中「已连接（未加密）到QEMU」。

路A 自清与观察项：终态 `berry device list --json` 报 `{"runs":[]}`，pgrep 无 berry-w6v-a 相关 qemu/berry/Chrome 残留（扫描所见其余 qemu 属 /tmp/berry-w6verify 与 /tmp/v2boot 并行会话，未触碰）；证书/Chrome profile/日志只落 /tmp/berry-w6v-a，仓库唯一新增物 out/vnc-w6v-a-tls-plain-web.png（w6v 前缀）。两观察项均非缺陷、不修：① stop 已自行退出的 run 时 stderr 打 `kill: <pid>: No such process`（stop 与 QEMU 自退的竞态噪音，stop/cleanup 语义正常完成）；② wss 错 CA 走 BerryError::protocol rc=7，与 TCP TLS 路径共用同一错误映射，plan/README 登记语义一致，设计既定。

路B c4 Linux 链：build-linux-binary.sh 重建 musl 静态 ELF（`ELF 64-bit LSB executable, ARM aarch64, statically linked, stripped`）；`BERRY_BIN=<musl berry>` 走 build-appdir.sh 打 out/BerryOS.AppDir（web 面 NOTICE.md + novnc/）；check-appdir.sh --require-elf rc=0 报 `summary: 37 ok, 0 warning(s), 0 failure(s)`，含通知三新检（在场 mode 644 共 907 字节、`register rustls 0.23` 行、`name the BSD-3-Clause crate (subtle)` 行、`point at web/NOTICE.md for noVNC` 指引行）；build-tarball.sh 产 out/BerryOS-linux-aarch64.tar.gz（23101203 字节，sha256=e42f1352119ca347b8c5dfd3cc50c542943e75673cc16d6604dcbd6876a97b69），tar 内 usr/share/berry/THIRD-PARTY-NOTICES.txt 与单源 sha256 同一。

路B c4 macOS/Windows：macos/build.sh 连跑两次，find out/BerryOS.app 全文件清单各自 sort 后 diff 报 `MANIFEST-IDENTICAL (135 files)`，含 Contents/Resources/THIRD-PARTY-NOTICES.txt（幂等过）；docker（rust:1.97-slim + mingw-w64）经 windows/build-cross-docker.sh 重建双 exe（`out/berry.exe: PE32+ executable (console) x86-64`、`out/berry-shell.exe: PE32+ executable (GUI) x86-64`），assemble.sh 打 out/BerryOS-win64 与 out/BerryOS-win64.zip，包根通知在场（907 字节）、zip 内层 sha256 同一；build.ps1 64/68 两行静态核对为从单源 `Copy-Item` 到包根；单源与三平台包内四处 `shasum -a 256` 同一 = 9c6d47871f482000f6a954576c1c00c21b74ad013275ecc91a9b9d67dbbbf4a1（consolidate 收口复核四处仍同一）。

路B c5 全量回归（BERRY_HOME=/tmp/berry-w6v-b/home）：cargo test --release 96 passed / 0 failed；`just berry-build` 绿（docs_guard PASS 0 警）；`just berry-smoke` 绿（doctor 确认 home 指向隔离目录、marker `BerryOS boot`、berry-shot.png 800x600）；`just berry-vnc-smoke` 绿（uri + ws_uri、口令三态含错串 rc=7）；`just berry-vnc-tls-smoke` 绿（certs 生成、status tls.enabled + wss uri、check 自动目录/`--ca`/`--insecure`+警告/错口令 rc=7/`--ca`+`--insecure` rc=2）；仓库根 `node .agents/tools/check-docs.mjs` 开工与收尾两跑均 218 篇 0 告警。

路B 自清：/tmp/berry-w6v-b/home/run/ 目录空，pgrep 无 berry-w6v-b 相关 qemu/berry 进程，残留 3 个 qemu 属 /tmp/berry-w6verify/ 与 /tmp/v2boot/（并行会话活 run，未触碰），共享 ~/.berry 全程未触碰，仓库文件零改动。

收口：两路合计覆盖本计划测试矩阵 c1-c8 的判据面与验收标准 1-6，零失败路；consolidate 阶段复核文档一致性——计划 Status: done 与步骤总览六项全勾、Agent Note（Status: implemented）、CLI-first 头块 v1.5 与【v1.5】注记、docs/索引.md 行 102 `v1.5 … · 1.5`、沿革节二百五十七、tools/berry/README.md（`check --ws` 用法/`--tls` 明文分支终态/许可随包段）与 packaging/README.md 单源惯例均与实际产物一致，无冲突需修。
