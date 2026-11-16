# VNC 第六波「合规随包与通路补全收尾」决策记录

Status: implemented
Date: 2026-11-14
Plan: [2026-11-14-vnc-compliance-wave6.md](../../plans/2026-11-14-vnc-compliance-wave6.md)
Related: [2026-11-13-vnc-complete-wave5.md](2026-11-13-vnc-complete-wave5.md)（wave5 遗留四条的来源）

## Problem

wave5 收口留下四条真实遗留：① 第三方许可通知以 heredoc 内嵌在 `packaging/linux/build-appdir.sh`，仅进 Linux AppDir——macOS/Windows 三脚本不带（合规不一致），且违反一事实一归属（许可事实的家被埋在脚本里），`check-appdir.sh` 对通知存在性与内容均零核对；② 浏览器通路（QEMU `websocket=` 口）无 CLI 自检手段——`vnc check` 只拨 TCP；③ `vnc web --tls` 施于明文 run 的 https 页分支浏览器实测未通过，悬置为「按未验登记」；④ CLI-first 索引行落后一版（1.3 vs 文档 1.4）。

## Decision

1. **通知单源文件，脚本只拷贝**：新建 `tools/berry/packaging/THIRD-PARTY-NOTICES.txt` 为唯一事实源（heredoc 正文逐字节迁移 + noVNC 指向 `web/NOTICE.md` 的指引段——noVNC 许可事实的归属在 NOTICE.md，通知只指路不复述），三平台脚本统一 `cp`（Linux AppDir `usr/share/berry/`、macOS `.app` `Contents/Resources/`、Windows 包根 berry.exe 旁），`check-appdir.sh` 增在场 + 三关键行 grep；单源-产物同步性由打包冒烟 sha256 对拍验收（检查器面向独立产物，无仓库假设，不做 sha256 对比）。
2. **ws 层 std 手写，不引 crate**：新增 `src/ws.rs`（SHA-1/base64/Upgrade 握手/帧编解码全手写，先例 `display/des.rs` 手写 DES）；`WsStream<S: Read+Write>` 泛型内层——明文包 `Stream`、wss 包 rustls `StreamOwned`，同帧层零分叉；RFB 层把 `handshake_auth` 泛型化后直接复用（一个状态机服务 TCP 与 ws 两路），新增 `handshake_server_init` 包装暴露 ServerInit 深度。自检判据用 RFB 语义（ServerInit/尺寸），不用 `vnc.connected` 事件（QEMU 在 TCP accept 即发，probe f1 钉板）。
3. **`--tls` 明文分支修，不收窄**：probe f2 定音失败根因是 vendored noVNC 1.7.0 `app/ui.js` 对 `path=` 形态无条件强改 scheme 为 wss（绝对 `ws://` URL 同改，Chrome 随即对明文口做 TLS 握手、QEMU 对 ClientHello 零应答即死）——非混合内容拦截；`host/port/encrypt=false` 形态的 scheme 由 `encrypt` 决定不被改写，Chrome loopback 豁免实测覆盖 noVNC 拨号（3/3）。`build_page_url_https` 改 host/port/`encrypt=false&path=`（在场空值）形态，`path=` 必须在场且空（缺席默认 `websockify`，QEMU ws 只认根路径）。
4. **`check --ws` 深度 = ServerInit，不拉帧**：与 TCP check 的 auth+one frame 不同深度——noVNC 报「已连接」同为 ServerInit 深度（自检目标 = 浏览器等价通路），且免去 QEMU 8192B 分块的大流量拉帧。
5. **索引行随 v1.5 刷齐**（本刀明示放开）。

## Alternatives

- **heredoc 内嵌（原状） vs 单源文件**：heredoc 让许可事实长在构建脚本里，macOS/Windows 复制需要三处正文拷贝（内容漂移必然）；单源文件 + `cp` 让「改内容只改一处」成立，脚本只剩路径引用。
- **ws 手写 vs 引 crate（tungstenite/tokio-tungstenite 等）**：手写约 500 行（SHA-1/base64/握手/帧层），换来零新依赖红线保持（host 工具侧唯一 crypto 依赖仍是 rustls）；QEMU ws 端点是裸 RFB over ws 的极简服务端（无扩展、无子协议协商、无 ping），crate 的协商/分片/压缩面全是死重。
- **修复 vs 收窄 `--tls` 明文分支**：SPEC 预设两臂（能过则修、不能过则 rc=2 收窄）；probe f2 以 ui.js 源码 + ClientHello 零应答 + DOM/事件形态三方钉死根因并 3/3 实证修复形态，走修复臂——收窄会砍掉一个已可用的形态（https 页统一入口）。
- **ServerInit 深度 vs 拉一帧对齐 TCP check**：拉帧要多走 SetEncodings/FramebufferRequest 并承接 8192B 分块字节流重组，收益只是输出与 TCP 路径同构；浏览器自身的「已连接」判据就是 ServerInit，自检与被检对象同深度更诚实。
- **`build.ps1` sha256 门 vs 静态对照**：mac 宿主无 pwsh，静态对照（与 assemble.sh 同构逐行）+ assemble.sh 实跑对拍；真 Windows 主机实跑沿 wave5 边界不在面。

## Consequences

- 三平台包内 `THIRD-PARTY-NOTICES.txt` sha256 与单源同一（AppDir/tarball 成员/`.app` Resources/win64 包根与 zip），`check-appdir.sh` 37 ok 0 fail（含新检查三条）；改 rustls 依赖版本时只改单源一处，三平台自动随行。
- `vnc check --ws` 明文/TLS run 全 rc=0 语义（尺寸+auth+ws uri），错口令 rc=7、无 ws 口 rc=3、`--ca`+`--insecure` rc=2；TLS 配置解析与 TCP 路径共用 `check_tls_client_config`（证书纪律不会漂移）。
- `--tls` 明文分支 Chrome 3/3 命中，wave5 的「实测未通过」悬置登记在 README/设计文档改写为根因事实；非 loopback dial 的 mixed-content 一次性警告保留。
- 单测 83→96（U34-U40）；`vnc web` 明文模式文本输出多一行 hint（URL/JSON/服务行为不变）。
- 仍是边界：`check --ws` 不拉帧（README 登记深度差异）；ws 子协议协商面不发送不校验（f1 nosub 实证 101，浏览器形态一致）；noVNC 维持 1.7.0，scheme 怪癖以 URL 形态绕开不打补丁。
