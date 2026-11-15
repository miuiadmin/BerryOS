# berry VNC 成熟化第二波（noVNC 网页桌面/TLS VeNCrypt/连接审计/口令生命周期）

Status: implemented

## Problem

第一波对外 VNC（[2026-11-13-vnc-desktop-suite](2026-11-13-vnc-desktop-suite.md)）落到「能用」：标准客户端可直连、口令策略齐、websocket 可达——但距成熟系统还有四个缺口：看桌面得先装 VNC 客户端（noVNC 要用户自带，berry 只给 `ws_uri`）；通道明文（DES 只认证不加密，非 TLS 级安全）；客户端来去无审计（`qmp.rs` 的 `execute` 读循环对异步事件静默跳过，VNC_CONNECTED/VNC_INITIALIZED/VNC_DISCONNECTED 全部丢弃）；口令只能轮换不能到期，且自动端口的 TOCTOU 竞态只被检出不自愈、`0.0.0.0` 绑定时 uri 回显绑定地址本身（不可拨的错用法）。

## Decision

- 网页桌面选 vendored noVNC 1.7.0 最小子集（约 1.5MB，`tools/berry/web/`，NOTICE 记来源/sha256/许可分层），`berry vnc web` 用 std `TcpListener` 手写 HTTP/1.1 只发静态页；打印的 URL 以 `path=<ws_uri>` 让浏览器直连 QEMU 的 ws 端口，berry 不代理任何 RFB 字节——数据面单一真相保持在 QEMU。URL 参数用 `path=`（noVNC 1.7.0 的 `host`/`port`/`encrypt` 已弃用），口令走 `#password=` fragment（不发往任何服务器），无口令则页面自弹输入框。
- TLS 选 QEMU `tls-creds-x509` 透传（`--vnc-tls <dir>`，对外端点 argv 追加 `tls-creds=`）：VeNCrypt STARTTLS 型端到端加密，与口令组合为 x509-vnc（TLS 内再走 DES 挑战）；证书集由 `berry vnc certs <dir>` 经 `/usr/bin/openssl` 子进程生成（CA 与 server 证书带 SKID/AKID 扩展——OpenSSL 3 类严格校验方拒收无 AKID 证书，live 发现），私钥 0600、指纹回显、已有证书拒覆写。内置客户端无 TLS：`vnc check` 对 TLS 端点短路报 not-supported，`display/vnc.rs` 握手层对 offered types 只含 19 时报同语义错误。
- 事件审计选 QMP 专职读线程 + pending-id 路由 + 事件 sink（重构 `qmp.rs`）：`execute` 对调用方的契约（错误形状、10s 超时）不变；`connect` 握手完成后清读超时——第一波留下的 10s socket 读超时若留给事件泵，空闲 10s 即误判断线杀线（首场 run live 复现）。VNC 三事件原样透传落 `events.jsonl`（t 值 `vnc.connected`/`vnc.initialized`/`vnc.disconnected`，client/server 对象字段不动），`berry events --filter <substr>` 客户端侧过滤。
- 口令生命周期：`vnc passwd --expire <seconds|never>` 翻译成 QMP `expire_password` 的 `+N`/`never`（QMP 裸数字 N 是绝对 epoch 秒＝立即失效，live 发现）；`--set`/`--generate` 轮换路径追加 `expire_password never` 复位（QEMU 11 的已触发到期计时器能活过 `set_password`，live 发现）。不提供 `--disconnect`：QEMU 11 对 protocol=vnc 强制 `connected=keep`，轮换不踢已连客户端（在线会话 live 坐实）。
- 健壮性：`vnc_cfg::dial_host` 把 `0.0.0.0`/`::` 映射为可拨地址（127.0.0.1/`[::1]`），`vnc status`/`open`/`check`/`device status` 的 uri 用 dial host 另加 `bind` 字段回显真实绑定；bind 竞态判定纯函数 + `rebuild_auto_ports` 只对 auto 侧换新空闲口重试一次（固定端口不重试、响亮报错带 qemu.log 尾）。
- 端点安全属性以端点 spec/`vnc.listen.tls` 推导，不单信 QMP 报告：`query-vnc-servers` 对 TLS 端点 ws 面 auth 误报 `none`（明文 Upgrade 实际被 RST），status 的 `tls` 字段 vencrypt 口径取 QMP 但 ws 安全面按 spec 断。

## Alternatives considered

- vendored noVNC vs 自写最小 RFB-over-ws HTML 客户端 vs 砍项 vs berry 代理转发（websockify 形态）：probe 已端到端证明 noVNC 纯静态可服务、Chrome 可直连 QEMU 原生 ws、口令 DES 应答全在 noVNC core 内闭环——自写客户端是用更差的重复实现替换已验证成熟件；代理转发给数据面加一跳 berry 进程，破坏「单一真相在 QEMU」并新增维护面；砍项放弃浏览器零安装收益。选 vendored。代价是约 1.5MB 资产与 NOTICE 维护义务，第一波「选 websocket 透传、页面用户自带」的裁决（彼时顾虑静态资产维护面）由本波按成熟化规格反转，因果记于此、第一波笔记不改。
- openssl 子进程生成证书 vs 纯 Rust 自签：std 无 x509/RSA/ASN.1，纯 Rust 意味着引第三方 crypto crate（违反本 workspace「不引第三方 crypto/networking」既有纪律）或手写数百行证书编码（第一波 DES 自含可行是因它只有百行且可标准向量钉板；x509+RSA 不是同量级）；macOS 自带 `/usr/bin/openssl`，子进程边界干净、失败时还能打印同款手动命令。选子进程。
- QMP 事件读线程 vs 轮询 `query-vnc-servers` 对比 clients：轮询抓不住短命客户端（disconnect 后无从对账）、固定间隔浪费 QMP 往返、且事件自带 server.auth 细粒度口径；读线程一次到位且 `execute` 契约可保不变。选读线程（重构风险以 55 项单测 + 既有命令路径回归兜住）。
- TLS 透传（QEMU `tls-creds-x509`）vs berry 终止 TLS（自持 TLS 服务端面）：终止要 berry 引 TLS 栈（rustls 级新依赖）并接管证书/会话生命周期，且对外端点仍需 QEMU 侧配合；透传零新依赖、端到端到客户端、证书目录不是秘密可进 argv。选透传；berry 自持 TLS（如未来 `vnc web` 出 https/wss 服务）留未来决策。
- `passwd --expire` 的 CLI 值域 `<seconds|never>` vs 直传 QMP `time` 串：QMP 裸数字是绝对 epoch（用户给 `60` 会立即失效而非 60 秒后），语义陷阱直接吃掉用户；CLI 侧翻译成 `+60` 把相对语义钉死在 berry 面。选翻译。

## Consequences

- 浏览器零安装直连可用：Chrome headless 打开 `vnc web` URL 截图与同刻 `berry screenshot` 内部帧调色板同构（真实 GUI 画面非加载面板），DOM 状态栏报「已连接（未加密）到QEMU」，ws 客户端在事件流 `websocket:true`。
- 客户端兼容矩阵实测入档：RealVNC Viewer 默认参数停在 VNC_CONNECTED——「未加密连接」确认弹窗阻塞（无 GUI 会话点不掉），headless/脚本场景需 `WarnUnencrypted=0 Encryption=PreferOff`；macOS 屏幕共享持久显示会话不发 ClientInit（审计停 connected/disconnected，其探测会话有全三事件）；RealVNC 7.15.1 与屏幕共享均无 VeNCrypt——TLS 端点真客户端实证以 python STARTTLS 探针为限（本机无 TigerVNC）。
- wss 边界如实不解决：`vnc web` 只发 http 静态页（std 无 TLS），TLS 端点的 `wss://` 受浏览器混合内容限制，berry 打一次性警告仍给 URL；wss 终止/自持 TLS 留未来。
- TLS 端点失去 `vnc check` 内建自检（not-supported 短路），验证路径换成 TigerVNC/外部探针——文档与 hint 都指过去。
- 口令卫生延续：argv/日志/事件零口令；web URL 的 `#password=` fragment 只出现在 berry 自己 stdout（用户面，与 `passwd --generate` 回显同级），不进任何服务器日志与外部进程 argv。
- `tools/berry/web/` 成为 vendored 资产面：升级 noVNC 需按 NOTICE 重下校验、重建子集、更新版本/sha256；`.app` 打包拷贝该目录留打包刀（`web_roots` 的 Resources 查找已就位）。
- 单测 39→55（dial_host/listen 往返/qemu_arg 组合/validate_tls/expire 校验/ws scheme/URL 构造/路径消毒/MIME/事件映射/bind 竞态判定/HTTP 解析等），既有 39 项零回归。

## Related

- 计划（契约/偏差/验收记录）：[2026-11-13-vnc-maturity-wave2](../../plans/2026-11-13-vnc-maturity-wave2.md)
- 第一波笔记（本波反转其「内建页面 vs 透传选透传」一条，其余互链）：[2026-11-13-vnc-desktop-suite](2026-11-13-vnc-desktop-suite.md)
- 设计（§7/§8/§13/§14/§15，v1.2）：[Berry设备平台-CLI-first设计-v1.0](../../../docs/Berry设备平台-CLI-first设计-v1.8.md)
- vendored 资产声明：[tools/berry/web/NOTICE.md](../../../tools/berry/web/NOTICE.md)
- 代码：`tools/berry/src/web.rs`（HTTP/1.1 静态服务）、`vnc_cfg.rs`（tls/dial_host/rebuild_auto_ports/到期翻译）、`qmp.rs`（读线程 + 事件 sink）、`qemu.rs`（`tls-creds-x509` 追加）、`device.rs`（sink 装配 + `boot_with_retry`）、`commands.rs`/`control.rs`/`main.rs`（布线）
- 实证产物：`out/vnc-w2-novnc-browser.png`（Chrome headless 网页桌面）、`out/vnc-w2-realvnc-frame.png`（RealVNC 真客户端帧）
- 沿革：[阶段沿革](../../../docs/阶段沿革.md)二百五十三节（VNC 第三波撞号治理自「二百五十」重排）
