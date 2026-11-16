# VNC 成熟化第二波实施计划（noVNC 网页桌面/TLS VeNCrypt/事件审计/口令生命周期）

Type: plan
Status: done
Updated: 2026-11-13

一句话说明：把第一波「能用」的对外 VNC 推到「跟成熟系统一样」——浏览器零安装直连（vendored noVNC 网页桌面）、TLS/VeNCrypt 加密通道、客户端连接审计（QMP 事件泵落 events.jsonl）、口令生命周期（轮换/到期）、健壮性（端口竞态重试、可拨地址），并以真实客户端矩阵（Chrome headless/RealVNC/macOS 屏幕共享/TLS 探针）实证。

## 背景

第一波（[2026-11-13-vnc-desktop-suite.md](2026-11-13-vnc-desktop-suite.md)，Status: done）落了对外 VNC 的「能用」面：QEMU 第二个 `-vnc` TCP 端点（对外在前 = id=default，QMP `set_password` 唯一作用面）+ `--vnc-ws` websocket + 口令策略矩阵（环回默认无口令、非环回强制自动生成）+ `berry vnc status/passwd/check/open` 四动词 + 控制面 `vnc`/`vnc_passwd` op + 重启链同端口同口令；内部面（vnc.sock/RamSource/VncInput/QMP broker/control.sock）零改动，决策记录见[第一波笔记](../notes/implemented/2026-11-13-vnc-desktop-suite.md)。

这一波补成熟度，对标三类成熟系统：服务端能力面对齐 TigerVNC/RealVNC（TLS/VeNCrypt 加密与认证组合、口令轮换与到期、客户端连接可观测）；网页面对齐 noVNC 官方发行（零安装浏览器直连）；开发者可观测性对齐 Android Emulator（连接事件流可审计、`berry events` 可过滤）。第一波留下的缺口正好被覆盖：其一，noVNC 需用户自带且无页面服务（第一波笔记曾裁「选 websocket 透传」，本刀按成熟化规格反转，理由与代价见偏差 8）；其二，通道无加密（DES 口令非 TLS 级安全）；其三，probe p5 实测 `qmp.rs` 的 `execute` 读循环（第 63–79 行）对异步事件静默跳过——无事件队列、无回调、无监听线程，VNC_CONNECTED/VNC_INITIALIZED/VNC_DISCONNECTED 全部丢弃，客户端来去无审计；其四，自动端口 TOCTOU 竞态只被检出（qemu.log 尾 hint）不自愈，`--vnc 0.0.0.0` 时 status 的 uri 回显绑定地址本身（第一波遗留清单原话）。

probe 阶段（2026-11-13，p1–p6，脚本与产物在 `/tmp/vncprobe2/`，会话临时不入仓）已实测关键事实：noVNC 1.7.0 可纯静态服务（最小子集约 1.5 MB，MPL-2.0/BSD-2-Clause/OFL-1.1/CC-BY-SA-3.0 分层许可，ESM 无构建步骤，`path=` 直连 QEMU 原生 ws；Chrome headless 截图链路全通，`--dump-dom` 状态栏报「已连接（未加密）到QEMU」，VNC auth 时 noVNC 弹口令框且纯 JS 实现 DES 应答）；QEMU `tls-creds-x509` 是 VeNCrypt STARTTLS 型（明文 RFB banner → 选 19 → VeNCrypt 0.2 → subtype 260/261 → TLS 1.3，链验证 CN=localhost ← berryos-probe-ca），`websocket=` 端口随之变 wss（明文 Upgrade 被 RST）；`query-vnc-servers` 报 `auth:"vencrypt"` 与 `vencrypt:"x509-none"/"x509-vnc"`，但该端点 ws 面的 auth 误报 `none`；RealVNC Viewer 7.15.1 有完整 CLI 可直连但不支持 VeNCrypt（strings 仅 VncAuth/RA2）；macOS 屏幕共享可 `open vnc://` 驱动且 QMP clients 可见；`set_password` 的 `connected` 枚举对 protocol=vnc 强制 keep（disconnect 报 GenericError）；`expire_password` 收 `never`/秒数字符串。SPEC 与 probe 的冲突裁决见「偏差」节。

## 契约

### 旗标与新动词总览

| 命令/旗标 | 语法 | 默认与说明 |
|---|---|---|
| `berry vnc web` | `[--run <id>] [--port N]` | 前台阻塞；默认自动挑空闲口，绑定 127.0.0.1 |
| `berry vnc certs` | `<dir>` | 用 `/usr/bin/openssl` 子进程生成自签 x509 证书集到 dir |
| `--vnc-tls <dir>` | `device start`/`host`/`shell` 三入口旗标 | 默认 off；env `BERRY_VNC_TLS` 兜底（先例 `BERRY_VNC`）；要求 `--vnc` ≠ off，否则 E_USAGE |
| `berry vnc passwd` | `[--set <pw>|--generate|--expire <seconds|never>]` | 三者互斥，默认 `--generate`；`--expire` 值域 `never` 或正整数秒 |
| `berry events` | `[--filter <substr>]` | 按子串过滤事件行（客户端侧，tail 与 follow 两态都过滤） |

`--vnc-tls <dir>` 的 dir 是 QEMU x509 证书目录（布局见下节）；目录路径不是秘密，可进 QEMU argv 与子进程 argv。解析优先级沿用第一波：显式旗标 > env > off。

### TLS/VeNCrypt：证书目录、生成命令与 QEMU argv

目录布局（probe p3 实测 QEMU 接受的最小集，`berry vnc certs <dir>` 产出）：

```text
<dir>/ca-cert.pem      # 自签 CA（basicConstraints=critical,CA:TRUE 必须，否则 QEMU 拒载）
<dir>/ca-key.pem       # 留作续签，0600；QEMU server 端点不读它
<dir>/server-cert.pem  # CN=localhost，SAN=IP:127.0.0.1,DNS:localhost
<dir>/server-key.pem   # 0600
```

`berry vnc certs <dir>` 生成命令（LibreSSL 3.3.6 实测通过；失败时打印同款手动命令并退 E_DEVICE）：

```bash
/usr/bin/openssl req -nodes -new -x509 -newkey rsa:2048 -keyout ca-key.pem -out ca-cert.pem -days 3650 -subj /CN=berryos-vnc-ca -addext basicConstraints=critical,CA:TRUE -addext keyUsage=critical,keyCertSign,cRLSign
/usr/bin/openssl req -nodes -new -newkey rsa:2048 -keyout server-key.pem -out server.csr -subj /CN=localhost
/usr/bin/openssl x509 -req -in server.csr -CA ca-cert.pem -CAkey ca-key.pem -CAcreateserial -out server-cert.pem -days 825 -extfile server.ext
#   server.ext 内容一行：subjectAltName=IP:127.0.0.1,DNS:localhost（临时文件，用后删）
/usr/bin/openssl verify -CAfile ca-cert.pem server-cert.pem   # 必须输出 OK
```

行为细则：openssl 定位顺序 `/usr/bin/openssl` > PATH 上的 `openssl`；dir 已存在且含 `server-cert.pem` 时拒跑（E_USAGE，hint 删除后重生成，避免覆写用户证书）；产物保留 ca-key/server-key（0600），删除 server.csr/server.ext；输出 CA 与 server 证书的 SHA256 指纹（`openssl x509 -noout -fingerprint -sha256`），human 两行 + JSON `{"v":1,"t":"vnc.certs","dir":…,"ca_fingerprint":…,"server_fingerprint":…}`。

QEMU argv 形态（对外端点沿用第一波「对外在前」，追加式扩展）：

```text
-object tls-creds-x509,id=vnc-tls,dir=<dir>,endpoint=server,verify-peer=off
-vnc <host>:<display>[,password=on][,websocket=<port>][,tls-creds=vnc-tls]   ← 对外（id=default）
-vnc unix:<run>/vnc.sock                                                     ← 内部不变
```

与口令的组合语义（probe p3）：tls-creds 无口令 = `vencrypt x509-none`（加密不认证）；tls-creds + password=on = `vencrypt x509-vnc`（TLS 内再走 DES 挑战，subtype 261）。口令策略矩阵不变——环回默认仍无口令（加密即开发者收益，认证照旧可 `--vnc-pass` 显式开），非环回强制口令。TLS 生效时 `websocket=` 端口自动变 wss（QEMU 行为，实测明文 Upgrade 被 RST），`vnc status` 的 `ws_uri` 随之回显 `wss://`。

`vnc status` 增 `tls` 显示：control 面 result 增 `"tls":{"enabled":true,"dir":"…","vencrypt":"x509-vnc"}`（vencrypt 取 `query-vnc-servers` default 条目的 `vencrypt` 子字段）；human 输出一行 `tls: on (<dir>, vencrypt x509-vnc)`；control 不可达时降级由 `vnc.listen` 的 `tls` 布尔回显 `tls: on`。注意 ws 面 auth 报 `none` 是 QEMU 报告口径缺陷（probe issues），端点安全属性一律按本端点 spec 推导，不单信该字段。

`vnc check` 对 TLS 端点：短路报 not-supported（退出码 2 E_USAGE），信息 `vnc check: not supported on TLS (VeNCrypt) endpoints`，hint 指向 `berry vnc web`（wss 场景见边界 5）与 TigerVNC；同时 `display/vnc.rs` 的握手层在 offered types 只含 19 时也报同语义错误（防止旁路直连时得到含混的「requires authentication」）。裁决依据：std 无 TLS 客户端、`openssl s_client` 无法直验 STARTTLS 型 VeNCrypt（偏差 4），不引新依赖是既有纪律。

### 内建 noVNC 网页桌面（vendored）

裁决：**vendored noVNC 1.7.0 最小子集**，不自写 JS 客户端、不砍项——probe p1/p2/p6 已端到端证明纯静态可服务、Chrome 可连 QEMU 原生 ws、口令交互与 DES 应答全在 noVNC core 内闭环；自写最小 RFB-over-ws 客户端是用更差的重复实现替换已验证的成熟件，无收益。

仓库内目录布局（`tools/berry/web/` 本刀新建）：

```text
tools/berry/web/
├── NOTICE.md                     # 本刀新建：来源/版本/sha256/许可分层/子集清单/获取方式
└── novnc/                        # noVNC v1.7.0 最小子集（约 1.5 MB，58 个 JS）
    ├── vnc.html                  # 入口页（BSD-2-Clause）
    ├── app/                      # *.js MPL-2.0；styles/*.css BSD-2-Clause；images CC-BY-SA-3.0
    ├── core/                     # *.js MPL-2.0（ESM，无构建）
    ├── vendor/pako/              # 上游自带 LICENSE 原样保留（core/inflator/deflator 硬依赖）
    ├── defaults.json / mandatory.json
    ├── LICENSE.txt               # 上游原样，不改一字
    └── AUTHORS                   # 上游原样
```

剔除项：`po/`、`tests/`、`docs/`、`snap/`、`utils/`、`package.json`、`eslint/karma` 配置、`vnc_lite.html`（probe 最小子集实测可用，页面不依赖被剔文件）。

资产查找顺序（`web.rs::web_roots()`，命中判据 = 目录下 `novnc/vnc.html` 存在；先例 `qemu.rs::resource_dirs()`，但不共用函数——语义多了 cwd 上溯）：exe 目录 `web` → exe 目录 `../Resources/web`（.app）→ `$HOME/Library/Application Support/BerryOS/web` → `%LOCALAPPDATA%/BerryOS/web` → cwd 逐级上溯找 `tools/berry/web`（仓库开发态）。全部未命中报 E_DEVICE，hint 给仓库路径与打包拷贝说明。`.app` 打包脚本拷贝 web 资产一步不在本刀允许改动面（packaging/ 未开放），留打包刀；`web_roots` 的 Resources 查找已就位。

`berry vnc web` 行为：

- 前置校验：`find_run` 后读 `vnc.listen`——无 listen（对外 VNC 未暴露）退出码 3，hint `start with --vnc on --vnc-ws on (see berry vnc status)`；listen 有但 `ws` 为 null 同码同 hint 并点名 `--vnc-ws on`。
- URL 构造（noVNC 1.7.0 语义，`host/port/encrypt` 已弃用故用 `path=`）：`http://127.0.0.1:<port>/vnc.html?autoconnect=true&path=<percent-encoded ws_uri 或 wss_uri>&resize=scale`；run 目录有 `vnc.pass` 时追加 `#password=<pw>`——fragment 不发往任何服务器（EMBEDDING.md 推荐位），省略时 noVNC 自弹口令框（probe p6），两者都支持；headless 自动化需要 fragment。
- TLS 端点：`path` 用 `wss://…` 并在 stderr 打一次性 mixed-content 警告（http 页连 wss 被浏览器拦，见边界 5），仍打印 URL。
- HTTP 服务：std `TcpListener` 手写 HTTP/1.1，零新依赖——GET/HEAD；路径消毒（拒绝任何 `..` 段，percent 解码后再判）；MIME 表（html/js/css/json/png/svg/ico，未知回落 application/octet-stream）；`Connection: close`、`Content-Length`、`Cache-Control: no-store`；请求头上限 8KB，超限或坏请求行回 400；读超时 5s；每连接一线程；`/` 直接回 vnc.html 内容（其相对 ESM 引用解析到同根）。
- 生命周期：前台阻塞（同 `berry events --follow` 形态）；human 输出两行（URL + `serving <root> on 127.0.0.1:<port> (Ctrl-C to stop)`）；`--json` 先出一行 `{"v":1,"t":"vnc.web","run":…,"url":…,"port":…,"root":…}` 再阻塞。
- 口令卫生：口令仅出现在 berry 自己 stdout 的 URL fragment（用户面，与 `vnc passwd --generate` 回显同级）；web 服务不打印请求行（无日志面）；berry 不代起浏览器（URL 不进任何外部进程 argv）。

NOTICE.md 内容契约：上游名 noVNC、版本 v1.7.0（2026-04-28 发布）、tarball `https://github.com/novnc/noVNC/archive/refs/tags/v1.7.0.tar.gz`（726728 字节）、sha256 `b1003a11b6e6e8d8f7f5e5586daae7f8ca651d8aee0aa155ff9ac841c48f52c6`、许可分层清单（core/app JS = MPL-2.0；vnc.html 与 styles CSS = BSD-2-Clause；Orbitron 字体条目 = OFL-1.1；app/images = CC-BY-SA-3.0；vendor/pako 自带 MIT LICENSE）、子集与剔除清单、获取与校验命令（curl + `shasum -a 256`）、「上游许可文本原样保留、未修改」声明。implement 取材：优先重新下载并校验；网络失败则用 probe 留档 `/tmp/vncprobe2/novnc/` 副本并重算文件级核对，实际来源与复核结果记入 NOTICE 与验收记录（不臆造下载成功）。

### 客户端事件审计（QMP 事件泵）

现状（probe p5）：`qmp.rs` 无事件读取线。改造为内置读线程 + 命令应答路由，`execute` 对调用方的契约（命令名进错误文本、错误形状、10s 超时语义）不变：

- `QmpClient::connect` 保持签名与同步序（greeting + `qmp_capabilities`），完成后 `try_clone` 出 writer，`BufReader` 移交专职读线程。
- 读线程逐行解析：含 `event` 键的行交给事件 sink；带 `id` 的应答按 pending 表路由给等待中的 `execute`（`mpsc::Receiver::recv_timeout` 10s，超时映射为现行的 `QMP: read timeout` 错误形状）；无 `id` 的 `return` 沿用现行兜底（完成最早 pending）。EOF（QEMU 退出）时读线程退出，pending 等待者收到断线错误。
- `quit_now` 改走 writer 直写（现经由 `stream.get_mut()`，重构后 stream 在读线程侧）。
- 新增 `set_event_sink(Arc<dyn Fn(&str, &Value) + Send + Sync>)`；`device.rs` 在 QMP 连上后（`boot_runtime` 第 373–383 行一带）装 sink，只处理三个 VNC 事件，其余静默（维持现状语义）。

events.jsonl 行形态（与既有事件同构，`v`/`t`/`source` 字段对齐，probe p5 全文为据）：

| QMP 事件 | t 值 | 行字段 |
|---|---|---|
| VNC_CONNECTED | `vnc.connected` | `v:1`，`t`，`source:"qmp"`，`client`，`server` |
| VNC_INITIALIZED | `vnc.initialized` | 同上 |
| VNC_DISCONNECTED | `vnc.disconnected` | 同上 |

`client` 对象 = QMP `data.client` 原样（`host`/`service`/`family`/`websocket`，断开事件同样携带客端信息——probe 实测三事件字段形态一致，客端端口可区分客户端）；`server` 对象 = `data.server` 原样（`host`/`service`/`family`/`websocket`/`auth`）。示例行：

```json
{"v":1,"t":"vnc.connected","source":"qmp","client":{"host":"127.0.0.1","service":"61086","family":"ipv4","websocket":false},"server":{"host":"127.0.0.1","service":"11801","family":"ipv4","websocket":false,"auth":"none"}}
```

落盘复用 `control.rs::append_event`（改 `pub(crate)`，避免第三份写者）。`vnc status` 的 clients 维持 `query-vnc-servers` 实时值；`berry events --filter vnc.` 直接过滤。追加写并发安全性与现状同基（O_APPEND 单行 writeln）。

### 口令生命周期

- `berry vnc passwd --expire <seconds|never>`：控制面 `vnc_passwd` args 扩展 `{"expire":"60"}`（与 `password`/`generate` 互斥）→ QMP `expire_password {"protocol":"vnc","time":…}` → 事件 `{"v":1,"t":"action","action":"vnc.passwd_expire","expire":"<原串>","source":"control"}` → 应答 `{"expire":"<原串>"}`。到期后新连接认证拒绝（w6 实证），已连客户端是否断开归 QEMU 行为不承诺。
- 无 `--disconnect`：QEMU 11 对 protocol=vnc 只接受 `connected=keep`（偏差 1），不提供做不到的旗标；「轮换不踢已连客户端」写进 README 与设计文档。
- 到期策略不进 `vnc status`：QMP 无可查字段（偏差 2），expire 动作以事件落档替代。
- 既有轮换路径（`--set`/`--generate`）与三不出现纪律（argv/日志/错误信息）零变化。

### uri/bind 可拨地址修正

- 新函数 `vnc_cfg::dial_host(bind) -> &str`：`0.0.0.0` → `127.0.0.1`；`::`/`[::]` → `[::1]`；其余（127.0.0.1/localhost/外部地址/hostname）原样。
- `vnc status`（control `vnc` op 与 CLI 降级两路）：`uri`/`ws_uri` 用 dial host 构造；result 增 `"bind":"<bindhost>:<port>"`（`addr_host` 形态）回显真实绑定；`listen` 维持绑定事实不变。human 输出 `vnc: 127.0.0.1:15944 (bind 0.0.0.0:15944) auth=… clients=…`。
- `vnc open`、`vnc check`、`device status` 顺带端点全部改用 dial host（现 `vnc check` 对 0.0.0.0 端点直连绑定地址本身，属碰巧能拨的错用法，一并修正）。
- control `status` op 的 `vnc` 字段同步：`{"host":<dial>,"port":…,"uri":…,"bind":…}`。

### 端口竞态重试

- 判定（纯函数，单测钉板）：boot 失败且 qemu.log 尾部命中 bind 竞态关键词（初判 `Failed to bind`/`Address already in use`/`Failed to start VNC`，实施时以真实复现日志校准）且 `Exposed::port_auto` 或 `ws_auto` 为真。
- 动作：`Exposed::rebuild_auto_ports()` 只对 auto 侧换新空闲口（host、固定口、口令、tls 目录全部不动），重写 `vnc.listen` 与 meta `vnc_listen` 后重走一次 `boot_runtime`；再失败按现状报错（qemu.log 尾 hint 保留）。
- 固定端口（字面 TCP 或字面 ws）：不重试，响亮报错（现状 qemu.log 尾 hint 保留）。
- 覆盖点：`Device::start` 与 `restart_in_place` 共用该包装（重生路径同样会撞竞态）。确定性说明：auto 口竞态窗口是毫秒级、不可编排复现，验收以纯函数单测 + 固定端口错误路径实录 + 代码审查为证（w7）。

### run 目录产物与 meta.json

- `vnc.listen` 形状扩展：`{"host":…,"port":…,"ws":…,"tls":<bool>}`；旧文件无 `tls` 字段读作 false（向后兼容，第一波 run 仍可 status）。
- meta.json 增 `vnc_tls`（目录串或 `"off"`）；`restart_with` 继承、`device_start_run` 透传 `--vnc-tls <dir>` 给 host 子进程。
- `Exposed` 增 `tls_dir: Option<PathBuf>` 与 `port_auto`/`ws_auto: bool`；`validate_flags` 扩为 `(spec, ws, tls)`：tls 无 `--vnc` → E_USAGE；tls 目录缺三 pem（ca-cert/server-cert/server-key）→ E_USAGE + `berry vnc certs <dir>` hint（在 CLI 侧预检命中，重演第一波「后台 host stderr 为 null 掩盖真实错误」的教训）。
- 无新增 secret 产物；证书目录属用户域（`vnc certs` 只生成，不登记进 run）。
- 控制面无新 op：`vnc`/`vnc_passwd` 扩字段，web/certs 是 CLI 本地动词（web 服务生命周期在 CLI 进程，certs 纯本地生成），不进会话控制面；hello capabilities 维持 15 项。

## 文件改动清单

实施顺序即下列次序（依赖从底向上）：vnc_cfg → qmp → qemu → device → control → web.rs → commands → main → vendored 资产 → 文档。

1. `tools/berry/src/vnc_cfg.rs`——`Launch`（第 409 行）增 `tls: Option<String>`；`resolve_launch`（第 447 行）env `BERRY_VNC_TLS` 兜底（spec 已给时仍可补 tls）；`resolve_from`（第 488 行）透传并校验 tls 目录（`validate_tls_dir`：三 pem 存在）；`Exposed`（第 244 行）增 `tls_dir`/`port_auto`/`ws_auto`，`build`（第 252 行）填充，新增 `rebuild_auto_ports`；`qemu_arg`（第 280 行）追加 `,tls-creds=vnc-tls`；新增 `dial_host`；`write_listen`/`read_listen`/`ListenInfo`/`listen_value`（第 318–345 行）增 tls 布尔；`ws_uri_for`（第 310 行）改带 tls 参产 `ws://`/`wss://`；`validate_flags`（第 469 行）扩 tls 三态校验。单测主场。
2. `tools/berry/src/qmp.rs`——结构重构（读线程 + pending 路由 + 事件 sink，见契约节）：`connect`（第 17 行）保持签名与同步握手；`execute`（第 53 行）改走 channel，错误形状与 10s 超时语义不变（第 63–79 行的丢弃式读循环被读线程取代）；`read_line` 移入读线程；新增 `set_event_sink` 与 `expire_vnc_password(&mut self, time: &str)`（`expire_password` + probe p5 schema）；`quit_now`（第 159 行）改 writer 直写；`set_vnc_password`/`query_vnc_servers` 不动。
3. `tools/berry/src/qemu.rs`——只追加：`spawn_image`（第 179 行）签名末尾追加 `tls_creds: Option<&std::path::Path>`，在 ext_vnc 注入点（第 207–209 行）之前插入 `-object tls-creds-x509,id=vnc-tls,dir=…,endpoint=server,verify-peer=off`；两个调用点同步（commands.rs shot 路径传 `None`、device.rs boot_runtime 传解析结果）。rtc fw_cfg 段（第 258–273 行）与既有 ext_vnc 段零回退。
4. `tools/berry/src/device.rs`——`Device::start`（第 49 行）：resolve 产物带 tls（spawn 前已由 validate_flags 预检），写 listen 带 tls，meta 构造（第 89–107 行）增 `vnc_tls`；`boot_runtime`（第 319 行）增 `tls_creds` 透传 spawn_image，QMP 连上后（第 373–383 行）装事件 sink（VNC 三事件 → `control::append_event` 落 events.jsonl，映射纯函数可单测）；新增 `boot_with_retry` 包装（bind 竞态判定 + `rebuild_auto_ports` + 重写 listen/meta + 重试一次），`Device::start` 与 `restart_in_place`（第 166 行）共用，`restart_in_place` 的 `self.vnc_ext` 在重试成功时同步更新。
5. `tools/berry/src/control.rs`——dispatch `"vnc"`（第 508 行）result 增 `bind`/`tls`，uri/ws_uri 按 dial_host 与 tls scheme；`"status"`（第 270 行）vnc 字段同步；`"vnc_passwd"`（第 552 行）增 `expire` 形态（互斥校验 + QMP + 事件）；`append_event`（第 593 行）改 `pub(crate)`；hello（第 249 行）不动。
6. 新增 `tools/berry/src/web.rs`——`web_roots()`（查找顺序见契约）、`find_web_root()`、`build_page_url(ws_uri, password)`、`serve(run, port, json)`（TcpListener 循环 + 每连接线程）、`handle_conn`（HTTP/1.1 解析/消毒/MIME/响应）、`mime_for(path)`。单测主场之二（路径消毒、MIME、URL 构造、请求解析）。
7. `tools/berry/src/commands.rs`——`HostArgs`（第 53 行）/`DeviceStartArgs`（第 63 行）/`ShellArgs`（第 75 行）增 `vnc_tls: Option<String>`；`shell`（第 341 行起 validate/set_launch 段）、`host`（第 446 行段）、`device_start`（第 830 行 validate_flags）透传；`device_start_run`（第 859 行）argv 增 `--vnc-tls`；`restart_with`（第 933 行起，meta 读取段第 956–1003 行）继承 `vnc_tls` 并重传；新增 `vnc_web(run, port, json)`（前置校验 + `web::serve`）与 `vnc_certs(dir, json)`（openssl 子进程序列 + 指纹 + verify）；`vnc_status`（第 1260 行）输出 bind/tls；`vnc_passwd`（第 1332 行）增 expire 参；`vnc_check`（第 1360 行）dial_host + tls not-supported 短路；`vnc_open`（第 1396 行）dial_host；`events`（第 780 行）增 filter 参（tail 与 follow 两态）。
8. `tools/berry/src/main.rs`——`StartCmd`（第 276 行）/`HostCmd`（第 330 行）/`ShellCmd`（第 365 行）各增 `--vnc-tls <dir>`；`VncAction`（第 383 行）增 `Web(VncWebCmd{run, port})` 与 `Certs(VncCertCmd{dir})`；`EventsCmd`（第 205 行）增 `--filter`；dispatch（第 668 行 Vnc 臂、第 550 行 Events 臂）接线。只追加 VNC 相关定义，不触碰并行会话的 profile/accel 默认值 hunk（ShellCmd 第 351–356 行一带）。
9. `tools/berry/src/display/vnc.rs`——`handshake_auth`（第 132 行）的「无 1/2」分支（第 210–213 行）细化：offered types 含 19 时报 `VNC: TLS (VeNCrypt) endpoint — not supported by the built-in client` + hint（`use berry vnc web or a VeNCrypt-capable client (TigerVNC), or drop --vnc-tls`）；`AuthKind` 不扩（VeNCrypt 会话不建立）。`connect_auth` 既有路径零回归。
10. `tools/berry/src/lib.rs`——增 `pub mod web;`。
11. 新增 `tools/berry/web/`——`novnc/` 子集与 `NOTICE.md`（内容契约见上）；上游 LICENSE.txt/AUTHORS/vendor pako LICENSE 原样。`.gitignore` 预计无需改（纯 vendored 静态文件，无生成物；若 implement 产生中间物再单列）。
12. `tools/berry/Cargo.toml`——不动（零新增依赖；openssl 只作子进程）。
13. 文档（收口阶段）——`tools/berry/README.md`（web/certs 动词、`--vnc-tls`、`BERRY_VNC_TLS`、`events --filter`、客户端兼容矩阵：RealVNC/屏幕共享不走 VeNCrypt、TLS 端点用 TigerVNC/探针）；`docs/Berry设备平台-CLI-first设计-v1.0.md` 升 v1.2（§5 命令树、§7.3 op 表扩字段、§13 安全表 TLS 行、§14 刀序表、§15 决策记录）；`docs/阶段沿革.md` 新节（二百四十九续）；`.agents/notes/implemented/2026-11-13-vnc-maturity-wave2.md`（implement 阶段同批，备选真实记录：vendored noVNC vs 自写 JS vs 代理转发、openssl 子进程 vs 纯 Rust 证书（不可行原因：std 无 x509/rsa）、QMP 事件线 vs 轮询、TLS 透传 vs 终止）。

## 测试矩阵

单测（`cd tools/berry && PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/cargo-target" cargo test --release --target "$(rustc -vV | sed -n 's/^host: //p')"`，新增落 vnc_cfg.rs/web.rs/device.rs）：

| # | 项 | 预期 |
|---|---|---|
| U12 | `dial_host` 映射 | `0.0.0.0`→`127.0.0.1`；`::`/`[::]`→`[::1]`；`127.0.0.1`/`localhost`/`192.168.1.4` 原样 |
| U13 | `vnc.listen` 往返含 `tls:true`；旧格式（无 tls 字段）读为 false | round-trip 相等；旧 run 兼容 |
| U14 | `Exposed::qemu_arg` 全组合与 auto 标志 | `127.0.0.1:71,password=on,websocket=6001,tls-creds=vnc-tls`；`port_auto`/`ws_auto` 正确；`rebuild_auto_ports` 只换 auto 侧 |
| U15 | `validate_flags` 扩 tls | tls 无 `--vnc` → E_USAGE；tls 目录缺三 pem → E_USAGE + `berry vnc certs` hint（tmpdir 造空/齐两态） |
| U16 | expire 参数校验 | `never`/`60` 通过；`abc`/`0`/`-1` 拒（E_USAGE） |
| U17 | ws scheme | tls=false → `ws://`；true → `wss://` |
| U18 | `build_page_url` | path 百分号编码正确；有 `vnc.pass` 出 `#password=`，无则无 fragment |
| U19 | web 路径消毒 | `/`、`/vnc.html`、`/core/rfb.js` 通过；含 `..` 段拒；`%2e%2e` 解码后拒；空路径拒 |
| U20 | MIME 表 | `.js`→`text/javascript`、`.css`/`.json`/`.html`/`.png`/`.svg`/`.ico` 正确；未知→`application/octet-stream` |
| U21 | VNC 事件映射纯函数 | 三事件 → t 名与字段形状（v/t/source/client/server，client/server 原样透传）；非 VNC 事件 → None |
| U22 | bind 竞态判定纯函数 | 样本日志（含 `Failed to bind`/`Address already in use`）+ auto=true → 重试；固定端口/无关错误 → 不重试 |
| U23 | HTTP 请求解析 | GET/HEAD 解析正确；坏请求行/超长头（>8KB）→ 400 |
| U24 | 既有 39 项 | 全绿零回归 |

真机冒烟（macOS + `/opt/homebrew/bin/qemu-system-aarch64` 11.0.3；desktop 档 `out/berryos-portrait.img` + `BERRY_DATA_DISK=out/gui-data.img`；只用 `berry device stop/cleanup --run <id>` 收自己的 run，严禁 `--all`；Chrome/RealVNC/屏幕共享起后收净）：

| # | 场景 | 预期与证据 |
|---|---|---|
| w1 | 网页桌面端到端：`device start --profile desktop --vnc on --vnc-ws on`；`berry vnc web --port N` 后台；Chrome `--headless=new --screenshot=out/vnc-web-desktop.png --window-size=480x1000 --virtual-time-budget=8000 <url>`；`--dump-dom` | PNG 落 out/ 非空白（尺寸/颜色数采样，真实 GUI 画面——ramfb 已被客机驱动）；dom 含 noVNC 状态「已连接」；杀 web 进程与 Chrome、stop+cleanup run 收净 |
| w2 | RealVNC 真客户端：`/Applications/VNC Viewer.app/Contents/MacOS/vncviewer 127.0.0.1:<port>` 后台；sleep 后查 | `vnc status --json` clients 含该会话（websocket:false）；events.jsonl 出 `vnc.connected`/`vnc.initialized`；kill pid 后 `vnc.disconnected` 落档（裁决：CLI 可直连，真连，不走 open 旁路） |
| w3 | macOS 屏幕共享：`open vnc://127.0.0.1:<port>`；sleep | clients 增量 + 三事件证据同 w2；`osascript -e 'tell application "Screen Sharing" to quit'` 收净 |
| w4 | TLS：`berry vnc certs <tmp>/certs`；`device start --vnc 127.0.0.1:<port> --vnc-tls <dir> --vnc-pass pw --vnc-ws on`；`vnc status`；python STARTTLS 探针（p3 序列：明文 RFB → 19 → VeNCrypt 0.2 → 260/261 → ssl 包裹）；`vnc check` | certs 出指纹且 verify OK；status 报 `auth=vencrypt`、`vencrypt=x509-vnc`、`ws_uri=wss://`、`tls` 字段；探针 TLSv1.3 握手成功 + CN 链验证 + TLS 内 SecurityResult=0（x509-vnc 时含 DES 挑战应答）；`vnc check` 退出码 2 报 not-supported；收净 |
| w5 | 事件审计：python RFB 探针连/断对外端点；`berry events --filter vnc.` | events.jsonl 三事件行与字段表逐一相符；`--filter` 只出 `vnc.*` 行；`--filter` 空串等价无过滤 |
| w6 | 口令到期：`vnc passwd --expire 2` → 立即 `vnc check` ok → sleep 3 → check 失败 → `passwd --set newpw` → check ok；`--expire never` | 到期后认证拒绝（AUTH FAILED，退出码 7）；`vnc.passwd_expire` 事件落档；轮换后旧口令失效；never 被接受 |
| w7 | 回归与竞态：无 `--vnc/--vnc-tls` 起档 argv 等价；固定端口被 python 预占后起档；`cargo test` 全绿；`just berry doctor` | argv 仅一条 `-vnc unix:`、无 `tls-creds` 字样；固定端口失败响亮（qemu.log 尾 hint 在错误里）；auto 重试以 U22 + 实施期复现尝试记录佐证（确定性说明见契约节）；39+新增全绿 |

## 验收标准

1. 单测全绿：既有 39 项零回归 + 新增 ≥12 项（U12–U23）。
2. 冒烟 w1–w7 全过，每项留证据（命令 + 输出摘录/PNG/事件行/dom 文本）。
3. 口令卫生延续：QEMU argv、host 子进程 argv、qemu.log、events.jsonl、CLI 错误信息与 hint 零口令；web URL 的 `#password=` 仅出现在 berry 自己的 stdout（用户面，与 `passwd --generate` 回显同级），fragment 不出浏览器、不进任何服务器日志。
4. 冻结面零改动：`git diff` 范围核对不含 window.rs/profile.rs、main.rs 并行 hunk、kernel/sched/user/arch/research/tools 冻结件；qemu.rs 只追加不回退。
5. NOTICE 齐备：来源/版本/sha256/许可分层/子集清单/获取校验命令；上游 LICENSE.txt/AUTHORS/pako LICENSE 逐字保留。
6. 零新增依赖：Cargo.toml 不动，HTTP 服务纯 std，证书经 openssl 子进程。
7. 文档收口：README、设计文档 v1.2、沿革新节、新 implemented 笔记、计划翻 done；`node .agents/tools/check-docs.mjs` 报告模式零告警；`just berry-build`（含 docs-guard）通过。

## 边界

- 不做 berry 帧代理/websockify 转发：浏览器直连 QEMU 的 ws 端口，`vnc web` 只发静态页与带参 URL，不代理任何 RFB 字节（数据面单一真相在 QEMU）。
- 不做 SetDesktopSize 翻译：ramfb 下客机控制几何（第一波边界沿用），分辨率变更仍走 `berry display` profile 重启。
- 不做剪贴板：客机无剪贴板栈，noVNC 剪贴板面板无后端。
- 不做 SASL：QEMU 需构建期 cyrus-sasl 且开发者场景 x509-vnc 已覆盖「加密 + 认证」。
- 不做 wss 终止与 berry 自持 TLS 服务端面：std 无 TLS，`vnc web` 只发 http 静态页；http 页连 wss 的混合内容限制如实警告、不解决；berry 自持 TLS（rustls 等）留未来决策。
- 不做 VncSource 客户端编码升级：维持 Raw；内部面帧主路 RamSource 不吃这条。
- vnc check 不内建 TLS/VeNCrypt 客户端：TLS 端点报 not-supported（退出码 2 + hint），真验走 TigerVNC 或 python 探针（probe 实测路径）。
- passwd 不做「踢已连客户端」（QEMU vnc 强制 connected=keep，偏差 1）；到期策略 QMP 不可查，status 不显示（偏差 2）。

## 偏差

probe 实测与功能规格假设的冲突，逐条以 probe 为准（QEMU 11.0.3 与 noVNC 1.7.0 事实优先）：

1. SPEC「`passwd --disconnect`（set_password 的 connected=disconnect 语义：轮换时断开已连客户端）」 vs p5（query-qmp-schema 与实测：枚举 keep|fail|disconnect 仅 spice 可用，vnc 强制 keep，在线 RFB 会话实测未被踢）——不提供 `--disconnect`；文档写明轮换不断开已连客户端，踢客端只能停会话/停端点。
2. SPEC「status 显示口令到期策略（若 QMP 可查）」 vs p5（query-vnc-servers/query-vnc 均无到期字段）——不可查则不显示；expire 动作以 events.jsonl 事件落档替代。
3. SPEC「vnc status 显示 tls/vencrypt」 vs p3（tls-creds 实例的 ws 面在 query-vnc-servers 中 auth 误报 `none`，明文 Upgrade 实际被 RST）——tls 状态以端点 spec/`vnc.listen.tls` 推导为准，auth/vencrypt 字段取 QMP 但不据此断 ws 面安全属性；status 同时回显 wss scheme。
4. SPEC「TLS 通道 openssl s_client 握手成功」 vs p3（s_client 直连报 tlsv1 alert protocol version——VeNCrypt 是 STARTTLS 型，TLS 不在 TCP 层开始）——TLS 握手证据改用 python 探针（明文 RFB 前导 → VeNCrypt 协商 → ssl 包裹 TLS 1.3），`openssl s_client` 不可用一事写入文档。
5. SPEC「若 RealVNC 支持 VeNCrypt 则真客户端过 TLS」 vs p4（7.15.1 无 VeNCrypt，仅 VncAuth/RA2，连 VeNCrypt-only 端点无输出直接退出；本机无 TigerVNC，noVNC 的加密路径是 wss 而非 VeNCrypt）——TLS 真客户端实证以 python 探针为限；客户端兼容矩阵（RealVNC/macOS 屏幕共享不走 VeNCrypt）写入 README 与设计文档，屏幕共享对 VeNCrypt 端点未实证不承诺。
6. SPEC「页面 URL 带 host/port/加密参数」 vs p1（noVNC 1.7.0 的 host/port/encrypt 已弃用，EMBEDDING.md 推荐 `path=` 绝对或相对 URL；口令推荐 fragment）——URL 用 `path=<ws|wss uri>` + `autoconnect=true&resize=scale`，口令走 `#password=`。
7. SPEC「VeNCrypt subtype 选择后 server 发 accept 字节」 vs p3（实测 0x01，与 spec 文本 0 不符，TLS 照常升级）——仅记录；本刀不做 TLS 客户端，无代码依赖该字节，未来实现者勿以它判成败。
8. 第一波笔记「内建 noVNC 页面 vs websocket 透传——选透传」 vs 本刀 SPEC（成熟化要求浏览器零安装直连）——按 SPEC 反转：vendored noVNC 由 berry 直接服务（成熟件已验证、自写无收益）；代价（约 1.5 MB 资产、NOTICE 维护、resource 查找面）在契约节显式化，新笔记记演进因果，不改第一波笔记。
9. implement 实测补充（2026-11-13，全部 live 校准于本刀 run）：
   - `expire_password` 裸数字 N 是绝对 epoch 秒（立即失效），相对语义必须发 `+N`——`vnc_cfg::expire_qmp_time` 做 CLI `<seconds|never>` → QMP `+N/never` 翻译；w6 全链（expire 2 → 立即 check ok → 3s 后 rc=7）已实证。
   - QEMU 11 的已触发到期计时器能活过 `set_password`（轮换后新口令仍被拒）——轮换路径追加 `expire_password never` 复位，w6「到期后轮换即可用」已实证。
   - 严格 TLS 校验方（python ssl = OpenSSL 3）拒绝无 AKID 的 CA/叶子证书——`berry vnc certs` 在 CA（SKID+AKID）与 server ext（AKID）补扩展；QEMU 接受扩展后的集，python STARTTLS 探针 TLS1.3 + CN 链验证 + TLS 内 DES 全通。
   - 第一波的 10s socket 读超时若留给事件泵读线程，空闲 10s 即被误判「connection closed」杀线（首场 run 实测复现：宿主 QMP 静默失效、外部端点事件丢失）——`connect` 握手完成后清读超时（阻塞读 + execute 自带 10s channel 超时），修后 30s 空闲事件照常落档。
   - VNC 事件 data.server.auth 在 TLS 端点报细粒度 `vencrypt+x509+vnc`（与 query-vnc-servers 的 `vencrypt`/`x509-vnc` 口径不同）——按「原样透传」契约保留；ws 客户端行 client.websocket=true 而 server.websocket 恒 false（QEMU 报告形状），同保留。
   - 固定端口 bind 失败在后台 `device start` 下仍表现为「host exited before announcing run」（后台 host stderr 丢弃是第一波结构性现状）；响亮的 qemu.log 尾证据经 `--foreground` 实录：`-vnc 127.0.0.1:10071: Failed to find an available port: Address already in use`（display 10071 = 15971），不重试（固定口）。

## 未完成事项

implement 阶段（2026-11-13）已完成：全部代码与单测落地（U12–U23 钉板，`cargo test` 55 项全绿 = 既有 39 + 新增 16）、`just berry-build` 绿、vendored noVNC 1.7.0 最小子集入仓（重新下载 + sha256 校验通过，非 probe 留档回退）并附 NOTICE；本机 live 冒烟已过：certs（指纹 + verify OK + 拒覆写）、无 VNC/无 ws 的 `vnc web` 错误路径（rc=3 + hint）、`vnc status` bind/tls/wss 显示、`vnc check` TLS 短路（rc=2）、外部端点 python DES 探针三事件落档、noVNC 页面 + Chrome headless 连通（dom「已连接（未加密）到QEMU」+ ws 客户端事件 websocket:true）、w6 口令到期全链、TLS x509-vnc 全链（TLS1.3 + 链验证 + TLS 内 DES + ServerInit 1088x1920）、无 VNC 起档 argv 等价（单条 `-vnc unix:`、零 tls-creds）、固定端口 bind 失败响亮报错实录。auto 口竞态重试的确定性复现未做成（毫秒级 TOCTOU 窗口不可编排，按契约节确定性说明以 U22 单测 + 固定口实录 + 代码审查为证）。

verify 阶段遗留（2026-11-13 implement 批移交，后由 verify 批全部完成）：w1 真实 GUI 画面截图（本次桌面近黑，需等 launcher 绘制后再截）、w2 RealVNC Viewer 真客户端、w3 macOS 屏幕共享、w7 `just berry doctor`。docs 阶段遗留（同批移交，后由 docs 批全部完成）：README/设计文档 v1.2/沿革新节/新 implemented 笔记/计划翻 done/check-docs 零告警。

verify 阶段（2026-11-13 同日）已完成：w1–w7 全矩阵真机验收通过，证据逐项见「验收记录」；`cargo test` 55 项全绿复跑、`just berry doctor` 绿、`just berry-smoke` 绿（doctor `ok:true` + marker `BerryOS boot` + `out/berry-shot.png`）；全部冒烟 run/证书与探针临时文件/GUI 进程自清，并行会话进程未受影响。

docs 阶段（2026-11-13 同日）已完成：`tools/berry/README.md`（web/certs 动词示例、`--vnc-tls`/`BERRY_VNC_TLS`、事件审计、`--expire`、Chrome 截图指引、客户端兼容矩阵、已知限制补剪贴板与 wss 边界）；设计文档升 v1.2（注记区/§7.3/§8.1/§13/§14/§15）；沿革二百五十节；新 implemented 笔记；NOTICE 复核齐备（来源/版本/sha256/许可分层/上游许可文件原样）；本计划翻 done；check-docs 报告模式零告警。

剩余真实遗留（开放项，均非本刀缺陷）：其一，RealVNC 7.15.1 与 macOS 屏幕共享无 VeNCrypt——TLS 端点真客户端实证以 python 探针为限（偏差 5），装 TigerVNC 后可补真客户端一档；其二，wss 终止与 berry 自持 TLS 服务端面留未来决策（std 无 TLS，引 rustls 级依赖需另立计划）；其三，auto 口 TOCTOU 竞态重试的确定性复现不可编排（毫秒级窗口），按契约以 U22 单测 + 固定口响亮报错实录 + 代码审查为证；其四，`.app` 打包脚本拷贝 web 资产一步留打包刀（`packaging/` 不在本刀改动面，`web_roots` 的 Resources 查找已就位）；其五，Windows 实机未验（对外端点与 web 服务走同一 std 代码路径）。

## 验收记录

以下为 verify 阶段（2026-11-13）真机矩阵实录，一段一物证；run id 均为本批自起自收（stop+cleanup 完成，残留 0）。

w1 网页桌面端到端（run `1794543470-560521000-40350-host`，desktop 档 `out/berryos-portrait.img` 256MiB，数据盘 `mkfat` + `BERRYOSVBLK!` 签名）：`just berry device start --profile desktop --image out/berryos-portrait.img --memory 256 --vnc on --vnc-ws on --keep` 起机，`berry vnc status` 报 `vnc: 127.0.0.1:54004 (bind 127.0.0.1:54004) auth=none clients=0` + `ws: ws://127.0.0.1:54005`；`berry vnc web --run <id> --port 18700` 输出 `http://127.0.0.1:18700/vnc.html?autoconnect=true&path=ws%3A%2F%2F127.0.0.1%3A54005&resize=scale`（环回无口令，无 fragment）。

w1 截图与像素实证：Chrome `--headless=new --screenshot=out/vnc-w2-novnc-browser.png --window-size=480x1000 --virtual-time-budget=8000 <url>` 产 8968B PNG（480x1000，486 色，主色白 410534 + 蓝 (0,116,224) 13250 + (40,40,40) 53760）；决定性对照=同期 `berry screenshot --run <id>`（内部面真值）1080x1920、408 色、白 2004075 + 同款 (0,116,224) 64049 + 同款 (27,37,64)，两画面调色板同构，证明浏览器页渲染的是真实 GUI 桌面而非空白/加载面板；`--dump-dom` 状态栏 `noVNC_status` 文本 = 「已连接（未加密）到QEMU」；events.jsonl 含 9 行 `websocket:true` 客户端事件（connected/initialized/disconnected 全形态）；Chrome headless 进程自退，零残留。

w2 RealVNC 真客户端（run `1794543826-013356000-42049-host`，`--vnc on --vnc-ws on --vnc-pass w2vncsec`，端口 54903，`auth=vnc`）：`berry vnc check` 先证对口令 ok（1080x1920）；错口令 python RFB 探针（位反 key + `/usr/bin/openssl enc -des-ecb`）回 `SecurityResult: 1` + reason `Authentication failed`（兼作第一波快检项）；RealVNC 口令文件用经典 VNC 固定混淆 key {23,82,107,6,35,78,88,7} 位反后经 openssl DES 生成（8 字节 `4a246fbd28b7edd1`），`vncviewer 127.0.0.1:54903 PasswordFile=… WarnUnencrypted=0 Encryption=PreferOff` 完成真实接入。

w2 证据与收净：client `127.0.0.1:55385` 在 events.jsonl 留完整生命周期 `vnc.connected → vnc.initialized → vnc.disconnected`（server auth=vnc），接入期间 `vnc status` 报 `clients=1`；`vncviewer -screenshot <pid> out/vnc-w2-realvnc-frame.png` 落 939185B PNG（1080x1920、81782 色、白 + 蓝系照片内容，非空帧）；kill 后进程消亡、断开事件落档。实测踩坑（已按「可达层级」记录并建议 docs 阶段写进 README 兼容矩阵）：默认参数下 RealVNC 停在 VNC_CONNECTED 不发 ClientInit——是其「未加密连接」确认弹窗阻塞（本机无 GUI 会话无法点掉），加 `WarnUnencrypted=0 Encryption=PreferOff` 后立即全握手；两把 key 变体对照证明停滞与口令文件无关。

w3 macOS 屏幕共享（run A 同档无口令 54004）：`open vnc://127.0.0.1:54004` 后 8s，`vnc status --json` clients 含 `127.0.0.1:54753`（websocket:false），events.jsonl 该 client 有 `vnc.connected`，`osascript -e 'tell application "Screen Sharing" to quit'` 后 `vnc.disconnected` 落档；第二次重开（client 54793）同样形态。可达层级说明：Screen Sharing 每次先做一条完整 RFB 探测会话（首捕获 client 54732 全三事件 connected/initialized/disconnected），其持久显示会话停在 connected 态不发 initialized（与 w2 弹窗停滞同类的客户端侧行为，probe p4 的证据层级即 clients[] + 连接事件，未承诺 initialized）；进程收净（ps 零残留）。

w4 TLS/VeNCrypt（run `1794544194-700305000-44133-host`，`--vnc 127.0.0.1:55100 --vnc-ws on --vnc-pass w2vncsec --vnc-tls /tmp/w2-certs`）：`berry vnc certs /tmp/w2-certs` 六条 openssl 子进程命令全录（CA 带 SKID/AKID 扩展），`openssl verify -CAfile ca-cert.pem server-cert.pem` = OK，指纹 ca `AF:3F:FD:26:…:93` / server `47:D5:E6:FA:…:6A`，私钥 0600，server.csr/ext 自删，二次运行拒覆写（rc=2 + hint）；status 报 `auth=vencrypt`、`tls: on (/tmp/w2-certs, vencrypt x509-vnc)`、`ws: wss://127.0.0.1:55527`（即 query-vnc-servers 的 auth/vencrypt 口径透出）。

w4 通道实证：python STARTTLS 探针（probe p3 序列）对 55100 全通——security types `[19]`、VeNCrypt 0.2、subtype `261`（x509-vnc）、post-subtype 字节 0x01（已知 spec 偏差，以 TLS 结果判）、`TLSv1.3 / TLS_AES_256_GCM_SHA384`、链验证 `commonName=localhost ← commonName=berryos-vnc-ca`、SAN `IP:127.0.0.1,DNS:localhost`、TLS 内 16B DES challenge（`ee1c4815…`）应答后 `SecurityResult: 0`、ClientInit 后 `ServerInit: 1088x1920`；`vnc check` 对 TLS 端点短路 rc=2「not supported on TLS (VeNCrypt) endpoints」+ TigerVNC/`vnc web` hint；wss 口 55527 明文 Upgrade 被 RST（ConnectionResetError 54），python ssl 包住后 `101 Switching Protocols` + `Sec-WebSocket-Protocol: binary`；该 run 的 events.jsonl 三事件 server.auth = `vencrypt+x509+vnc`（QEMU 事件面细粒度口径，按契约原样透传）；`berry vnc web` 对 TLS run 打 mixed-content 警告并给 `path=wss://…#password=…` URL；RealVNC 过 TLS 未试（probe p4 裁定 7.15.1 无 VeNCrypt，偏差 5）。

w5 事件审计：三个 run 的 events.jsonl 共 46 行 vnc 事件全量程序化核对——top 字段恰为 `{v,t,source,client,server}` 且 `v=1`、`source="qmp"`，client 字段恰为 `{family,host,service,websocket}`，server 字段恰为 `{auth,family,host,service,websocket}`，三种 t 值形态一致，与契约表逐项相符（含断开事件携带完整 client 信息）；`berry events --filter vnc.` 过滤正确（run A total=24、过滤后 23、排除的 1 行为 screenshot 事件；过滤结果非 vnc 行数 0；空过滤等价无过滤 24 行；无匹配子串 0 行）。

w6 口令生命周期（run B，口令档）：轮换不踢客端实证——python 全握手 client `55994` 在线时 `berry vnc passwd --set w2rot8pw`，轮换后 disconnected 计数不变（5→5）、clients 列表仍含 55994（QEMU vnc 强制 connected=keep，SPEC `--disconnect` 不提供的偏差 1 以 live 会话坐实），该 client 自然关闭后全三事件落档；`passwd --expire 1` → 3s 后 `vnc check` rc=7 `VNC: auth failed: Authentication failed`（对口令也被拒=到期语义）；随后 `passwd --set w2rot9pw` 立即 check ok（轮换路径追加的 `expire_password never` 复位生效，QEMU 11 到期计时器不跨界）；`passwd --expire never` 接受且 check ok；`--expire abc` rc=2 + 值域 hint；events.jsonl 落 `{"action":"vnc.passwd"…}` 与 `{"action":"vnc.passwd_expire","expire":"1"/"never"…}` 四行，无口令明文。

w7 回归与竞态：`cargo test --release` 55 项全绿（既有 39 + 新增 16，含 `is_vnc_bind_race`/`should_retry_bind_race`/`boot_with_retry` 纯函数钉板）；固定端口竞态实录——python 预占 TCP 55199 后 `--vnc 127.0.0.1:55199` 起档，`--foreground` 响亮报错 `qemu-system-aarch64: -vnc 127.0.0.1:49299: Failed to find an available port: Address already in use`（display 49299 = TCP 55199，net.rs 换算符合第一波口径）、退出码 7、失败 run cleanup 收净，不重试（固定口）；auto 口重试的确定性复现仍不可编排（毫秒级 TOCTOU 窗口），按契约以 U22 单测 + 固定口实录 + 代码审查为证。

w7 可拨地址与收尾：`--vnc 0.0.0.0:56000` 档 status 报 `vnc: 127.0.0.1:56000 (bind 0.0.0.0:56000) auth=vnc`（非环回自动生成口令），uri/ws_uri 用 127.0.0.1 可拨地址、bind 回显真实绑定，`vnc check` 经 dial host ok（rc=0），`device status --json` 的 status.vnc 字段 `{"bind":"0.0.0.0:56000","host":"127.0.0.1","port":56000,"uri":"vnc://127.0.0.1:56000"}`，`vnc open` 打开 `vnc://127.0.0.1:56000` 且 QMP 侧出现真实 client（127.0.0.1:56713）；口令卫生终扫——run B 的 qemu.log/meta.json/events.jsonl/serial.log/state.json/vnc.listen 及全部 QEMU argv、host 子进程 argv 中三个口令串命中数全为 0，vnc.pass 权限 600；`just berry doctor` 绿（qemu 11.0.3/hvf/固件/镜像/内核/profiles 齐）；本批 4 个 run 全部 stop+cleanup、`vnc web` 服务进程 killed、/tmp 证书与探针文件删除、out/ 冒烟数据盘（gui-data-w2/w2b.img）删除、vncviewer/Chrome/屏幕共享进程零残留，证据产物留 `out/vnc-w2-novnc-browser.png` 与 `out/vnc-w2-realvnc-frame.png`。

verify 阶段发现（无需改码，移交 docs 阶段）：其一，RealVNC 默认参数停在 VNC_CONNECTED（未加密确认弹窗），非 TLS 端点自动化/无 GUI 场景需 `WarnUnencrypted=0 Encryption=PreferOff`，建议写入 README 客户端兼容矩阵；其二，macOS 屏幕共享持久会话不发 ClientInit（停 connected 态），审计面以 clients[] + connected/disconnected 为准，探测会话可见全三事件；其三，`--vnc 0.0.0.0`（无端口）按语法拒收并给值域 hint，属设计行为非缺陷。
