# berry VNC 第五波「彻底完善」（TLS 客户端补全/https 网页桌面/P2 三修/Linux 分发链实机化）

Status: implemented

## Problem

四波落地后 VNC 面剩两个功能洞：`vnc check` 对 TLS（VeNCrypt）端点 rc=2 not-supported 短路——std 无 TLS 客户端，内置 RFB 客户端做不了 STARTTLS 升级；TLS run 的浏览器路径整个不可用——http 页连不了 wss（混合内容），且 probe 定音 Chrome 的 wss 直连 QEMU 因 gnutls 恒发 `CertificateRequest` 以 `ERR_SSL_CLIENT_CERT_NEEDED` 静默弃连（`--ignore-certificate-errors` 不覆盖客户端证书请求），与页面方案无关。另有第四波 survey 三条 P2：F16 passwd 轮换后 expire 复位失败被 `let _` 吞（认证可能变砖零信号）、F18 web 无连接总预算（慢滴客户端可占满 16 槽）、F20 `vncpass-*` 孤儿无清扫；Linux 分发链停在「mac 宿主实跑可行」，无正式产物。

## Decision

- TLS 库选 rustls 0.23（`default-features=false`，features `["std","ring","tls12"]`，另显式 `rustls-pki-types/std`——PEM 装载与 `ServerName` 等类型在公共路径直接使用，不赌传递解析的偶然结果）：纯 Rust、无运行时进程依赖，阻塞 `StreamOwned` 与 berry 的 std 手写网络模型（每连接一线程、`Stream` 枚举）同构，不引 tokio；同库三面复用——VeNCrypt 客户端（`vnc check`）、https 页服务端（`vnc web`）、wss 终止代理（客户端 + 服务端两侧）；probe 已实证与 QEMU 11.0.3 gnutls 生产配对（含对端 `CertificateRequest` 时空证书应答被接受）。只进 `tools/berry` 独立 workspace，不进客机/ABI/kernel 侧任何 Cargo 面；许可 Apache-2.0 OR ISC OR MIT，传递依赖唯一例外 subtle = BSD-3-Clause（rustls 0.23 硬依赖、无剔除路径，三句版宽松许可裁定接受，Linux 包 `THIRD-PARTY-NOTICES.txt` 登记）。
- VeNCrypt 客户端走字节级状态机（明文 RFB → security type 19 → VeNCrypt 0.2 版本协商 → 子类型纯函数选择：X509Vnc 261 优先、X509None 260 兜底 → rustls TLS 握手 → TLS 内继续既有认证：261 走 DES 挑战、260 无内层认证）；证书校验默认严格（按证书目录 `ca-cert.pem` 验链 + SAN），`--insecure` 是显式逃生门且必打一次性警告，与 `--ca` 互斥（同给 rc=2），明文端点静默忽略二者；`handshake_auth` 尾段提取 `rfb_tail` 明密共用，既有三态测试零改动。
- TLS run 的浏览器路径用 berry 侧 TLS 终止代理而非页面直连：Chrome wss 直连 QEMU 不可行（客户端证书墙，berry 控制不了 QEMU argv 之外的对端行为），故 berry 起 https 页（rustls 服务端，同证书目录）+ wss 代理口（两侧 rustls 终止后纯字节双向管道，不解 HTTP 不解 ws 帧，Upgrade/101/mask 由两端点自理）。这是第二波「berry never proxies any RFB bytes」原则的显式例外，动机与边界写进 `web.rs` 模块头注释与 README；明文 run 完全零代理不回归。页面 URL 用 `host+port+encrypt=true+path=(空)+resize=off` 形态——noVNC 1.7.0 的 `path=<wss URL>` 形态会回落明文连接，且 `path` 参数缺席时 noVNC 默认拼 `websockify` 路径而 QEMU ws 端点只认根路径（参数必须在场且值为空）。
- F18 连接总预算取 30s：覆盖最慢合法浏览器资产加载，同时饿死一字节/数秒的慢滴占用；只作用于页面请求-响应连接——wss 代理是可持续数小时的桌面数据面长连接，不设总预算（仅 TLS 握手 10s 超时、管道阶段沿用 OS 语义）。F20 孤儿清扫阈值取 1h：活跃交接窗口（秒级）远小于此，正常文件 mtime 恒新不会被误删，并行会话同刻在写的文件同样安全；清扫尽力而为，扫不动（权限/坏目录）不阻断口令交接主流程。F16 复位失败告警走 `append_event` 而非命令失败：口令本身已换成功，让 `vnc passwd` 整体失败会误导操作者；事件形态由纯函数 `passwd_warn_event` 构造、单测钉死，错误摘要取 QMP 错误消息（该调用参数是 `"never"`，不含口令材料）。

## Alternatives considered

- TLS 库四选一：openssl 子进程落选——`vnc certs` 已用 openssl 子进程做证书生成，但那是单向一次性命令；TLS 数据面子进程化 = 双向字节管道 + 进程生命周期管理 + LibreSSL/OpenSSL 版本差异解析，且完全做不了常驻 https 页面监听。python 辅助脚本落选——probe 期确用 python 完成字节级验证，但 python3 不在 berry 依赖面，AppDir/AppImage 交付物不能假设目标机有 python，等于把开发机专用工具变成产品依赖。维持 not-supported 落选——第五波「彻底完善」的目标就是关掉两个洞，TigerVNC 之类外部客户端只是绕过不是补全。rustls 入选（理由见 Decision 首条）。
- 浏览器 TLS 路径三选一：https 页 + wss 直连 QEMU 落选（probe 实测 Chrome 对恒发 `CertificateRequest` 的端点静默弃连）；berry 全代理（终结客户端连接并解 RFB 帧做翻译）落选——那是 SetDesktopSize 翻译等完整代理架构的体量，另立项；选中 = 最小面 TLS 终止 + 字节管道（两端点协议自理）。
- rustls 0.23 实现陷阱（代差记录，与代理代码注释同源）：Cursor 空块不得进 `read_tls`（`Ok(0)` 被 latch 为传输 EOF、会话当场判死）；喂入循环按剩余字节数驱动、不能读到 `Ok(0)` 收尾（同理 latch）；`Reader::read` 空缓冲返回 `WouldBlock` 非普通流语义（明文排水不能 `read_to_end`，逐块读并把 `WouldBlock` 释义为暂无更多明文）；对端无 `close_notify` 的 EOF 报 `UnexpectedEof` 按半关闭容忍。

## Consequences

- `vnc check` 对 TLS run 自动走 VeNCrypt（证书目录从 run meta 推得，`--ca` 显式覆盖），通道自检不再依赖 TigerVNC/外部探针；实测三路 rc=0（自动/`--ca`/`--insecure`+警告）、错 CA rc=7 `invalid peer certificate: UnknownIssuer`、缺 `ca-cert.pem` rc=2 + `berry vnc certs` hint、`--ca`+`--insecure` 互斥 rc=2、明文档输出形态逐字节不回归。
- TLS run 的 `vnc web` 出 https URL + `proxy_port`，Chrome headless（`--ignore-certificate-errors`）实测 DOM「已连接（已加密）到QEMU」、截图画布像素与 `berry screenshot` 地面真值逐项吻合（`out/vnc-w5-tls-browser.png`，真 GUI 镜像对拍）；run `events.jsonl` 实录 websocket 客户端 `vencrypt+x509+vnc` 完整 RFB 生命周期——QMP 侧可见代理路径与直连等价。
- berry 在 TLS run 下代理 RFB 字节（唯一场景）——审计上 berry 成为数据面一环；明文 run 行为零变化（http 页 + 浏览器直连 QEMU ws，berry 零代理）。
- Linux 分发链有正式产物：`out/` 下 musl 静态 ELF（4.95MB stripped，ring 的 C 面经 Apple clang + `-ffreestanding` + 自足 `stddef.h` shim 交叉）、`BerryOS.AppDir-linux`（`check-appdir.sh --require-elf` 33 ok 0 fail）、`BerryOS-linux-aarch64.tar.gz` + sha256；docker 醒时追加 `BerryOS-aarch64.AppImage` 与 Windows 交叉包（PE32+ 双 exe）；容器冒烟实测过（AppImage FUSE 挂载跑 doctor、alpine + qemu 真机架起机双档截图）。
- 单测 64→83；justfile 增 `berry-vnc-tls-smoke` 配方（certs → TLS run → status 断言 → check 四路 + 互斥，自起自收）；工作树 `.gitignore` 补六项忽略（probe 2026-11-13 裁决无竞争性 ACTIVE 声索后落地，未跟踪污染 6120 条降到 17 条，已入库文件零影响）。
- 遗留：SetDesktopSize 代理翻译（另立项）、剪贴板、SASL、裸机 Linux/Windows 安装与桌面集成（容器冒烟为上限）、`--tls` 施于明文 run 的 https 页分支（实现保留、浏览器实测未通过，按未验登记）、`window.rs` 解冻直参（并行会话属地顺延）、macOS/Windows 打包脚本的 rustls 许可随包登记（README 先行登记）。

## Related

- 前波：[2026-11-13-vnc-desktop-suite](2026-11-13-vnc-desktop-suite.md)（对外暴露与认证）、[2026-11-13-vnc-maturity-wave2](2026-11-13-vnc-maturity-wave2.md)（web/TLS/审计——其「wss 终止与 berry 自持 TLS 服务端面留未来决策」由本篇接棒，旧篇冻结不改、单向链）、[2026-11-13-vnc-packaging-wave3](2026-11-13-vnc-packaging-wave3.md)（分发与连接上限）、[2026-11-13-vnc-sweep-fixes](2026-11-13-vnc-sweep-fixes.md)（P0/P1 清理与测试沉淀，F16/F18/F20 台账出自其 survey）。
- 计划（契约/偏差/验收记录）：[../../plans/2026-11-13-vnc-complete-wave5.md](../../plans/2026-11-13-vnc-complete-wave5.md)；第四波 survey：[../../research/2026-11-13-vnc三波遗漏检查.md](../../research/2026-11-13-vnc三波遗漏检查.md)。
