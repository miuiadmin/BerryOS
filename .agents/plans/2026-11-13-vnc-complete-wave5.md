# VNC 第五波「彻底完善」实施计划（TLS 客户端补全/https 网页桌面/P2 三修/Linux 分发链实机化）

Type: plan
Status: done
Updated: 2026-11-14

一句话说明：关掉 VNC 面最后两个功能洞（`vnc check` 过不了 VeNCrypt、TLS run 浏览器打不开网页桌面），清掉第四波 survey 的三条 P2（F16/F18/F20），把 Linux 分发链从「mac 宿主实跑可行」推到「本机全链正式产物」，并按 probe 裁决补齐 `.gitignore` 六项——引入 rustls 作为唯一新依赖，全部落在 host 工具侧。

## 背景

VNC 四波已全部落地（工作树态，未 commit）：[第一波](2026-11-13-vnc-desktop-suite.md)对外暴露与认证（`--vnc/--vnc-ws/--vnc-pass`、QEMU 第二 `-vnc`、DES 口令、`vnc status/passwd/check/open`）；[第二波](2026-11-13-vnc-maturity-wave2.md)成熟化（vendored noVNC 1.7.0 + `berry vnc web`、`--vnc-tls` + `berry vnc certs`、QMP 事件泵审计、`passwd --expire`、uri/bind）；[第三波](2026-11-13-vnc-packaging-wave3.md)分发（三平台打包带 web 资产 + NOTICE、`vnc web --max-conns`）；[第四波](../research/2026-11-13-vnc三波遗漏检查.md)遗漏检查（P0/P1 全清、单测 58→64、`just berry-vnc-smoke` 配方）。

现状两个功能洞与三条 P2 台账：

- 洞一：`vnc check` 对 TLS（VeNCrypt）端点 rc=2 `not-supported` 短路（`commands.rs` `vnc_check` 的 `listen.tls` 分支）——std 无 TLS 客户端，内置 RFB 客户端做不了 STARTTLS 升级。
- 洞二：TLS run 的浏览器路径整个不可用——http 页连不了 wss（混合内容）；且 probe 定音（见「偏差」）：Chrome 的 wss 直连 QEMU 因 QEMU gnutls 恒发 `CertificateRequest` 而以 `ERR_SSL_CLIENT_AUTH_CERT_NEEDED` 静默弃连，与页面方案无关。
- F16（`control.rs` `vnc_passwd` 臂）：轮换成功后 `let _ = q.expire_vnc_password("never")` 吞掉复位失败——到期计时器仍生效时认证整体变砖而零信号。
- F18（`web.rs` `handle_conn`/`read_head`）：仅 5s 逐读写超时无连接总预算，慢滴客户端可无限占 16 槽（仅环回绑定，风险面小）。
- F20（`vnc_cfg.rs` `write_pass_temp`）：`vncpass-*` 一次性口令文件在子进程早死时永存（F14 根因面），无按龄清扫。
- Linux 分发链：probe 已在 mac 宿主实跑全通（静态 ELF / AppDir 33 检 0 fail / tarball），本刀产正式 `out/` 产物并按 docker 可用性追加 AppImage 与 Windows 交叉产物。
- 条件项：`.gitignore` 缺六项忽略（约 2.0G 构建缓存、6120 条未跟踪污染）；probe 复核无竞争性 ACTIVE 声索（[第四波 survey](../research/2026-11-13-vnc三波遗漏检查.md) F23 明言「与并行线协调后一并于后续刀补」，`research/桌面UX审计/04-实施波一复核.md` 把其中五项归构建残渣）。`window.rs` 仍冻结（probe 确认 `window.rs`/`profile.rs` 均为并行会话活跃改动 M 态），解冻换直参顺延。

probe 事实（2026-11-13，全项 GO）要点：rustls 0.23.46 可解析可构建、与 QEMU 11.0.3 gnutls TLS 握手实测全通（含对端 `CertificateRequest` 时空证书应答被接受）；VeNCrypt 字节级序列复现钉死（版本 0.2 / 仅回 261 / 0x01 接受位 / TLS 内 261 直接 DES 挑战）；berry 侧 rustls TLS 终止后 Chrome + 真 noVNC + DES 口令认证全通（tap PoC）；docker（OrbStack）2s 可唤醒；Linux 链 mac 宿主四脚本实跑全通。

## rustls 决策

### 为什么是 rustls

四个真实考虑过的候选（完整因果在 implement 阶段落 `.agents/notes/implemented/2026-11-13-vnc-complete-wave5.md`，届时新建）：

- **rustls（入选）**：纯 Rust、无运行时进程依赖、阻塞 API（`StreamOwned<ServerConnection/ClientConnection, TcpStream>`）与 berry 现有 std 手写网络模型（每连接一线程、`Stream` 枚举）同构，不引 tokio；同一套库同时覆盖本刀三个面——VeNCrypt TLS 客户端（`vnc check`）、https 页面服务端（`vnc web --tls`）、wss 终止代理（客户端 + 服务端两侧 TLS）。probe 已实证三面可行性（含与 QEMU gnutls 的生产配对）。
- **openssl 子进程（落选）**：`vnc certs` 已用 openssl 子进程做证书生成，但那是单向一次性命令；TLS 数据面走子进程意味着双向字节管道 + 进程生命周期管理 + 输出解析（LibreSSL/OpenSSL 版本差异），且完全做不了 https 页面服务端（需要常驻 TLS 监听）。复杂度远超收益。
- **python 辅助脚本（落选）**：probe 期确用 python 完成了字节级验证，但 python3 不在 berry 的依赖面；AppDir/AppImage 交付物不能假设目标机有 python；等于把开发机专用工具变成产品依赖。
- **维持 not-supported（落选）**：两洞继续存在，第五波「彻底完善」的目标就是关掉它们；TigerVNC 之类外部客户端只是绕过，不是补全。

### 版本、feature 与依赖面

- 版本：rustls 0.23（解析到 0.23.46，当前最新稳定）；rustls-pki-types 1（解析到 1.16.0）。
- feature 最小集（probe `cargo tree` 核）：`rustls = { default-features = false, features = ["std", "ring", "tls12"] }`——不带 `aws-lc-rs`（probe 确认不在树）、不带 `logging`。`rustls-pki-types = { features = ["std"] }` 显式声明：PEM 装载（`pem_file_iter`）与 `ServerName`/`CertificateDer`/`PrivateKeyDer` 类型在本刀公共路径上直接使用，显式依赖保证 std feature 就位，不依赖 rustls 传递解析的偶然结果。
- 传递依赖（mac 编译面）：once_cell / ring（cc 构建期、cfg-if、getrandom、libc、untrusted）/ rustls-pki-types（zeroize）/ rustls-webpki / subtle / zeroize。
- Cargo.lock 预声明：除上述外还会增 `windows-sys`/`windows-targets`/`windows_*`/`wasi` 条目——全部目标门控（`cfg(windows)`），本机不编译，属 ring/getrandom 的正常目标表，不是依赖膨胀。
- MSRV：rustls 0.23.46 = 1.71、rustls-webpki = 1.71、ring = 1.66、rustls-pki-types = 1.60，全部低于仓级钉版 1.97.0（[rust-toolchain.toml](../../rust-toolchain.toml)），零工具链风险。
- 已知实现坑（probe 实证，代码须消化）：rustls 0.23 把对端无 `close_notify` 的 EOF 报为 `UnexpectedEof`——读循环按半关闭语义容忍（EOF 即连接结束，不当致命错外抛）；`ServerName::try_from(String)` 取 `'static` 生命周期可行。

### 许可

- rustls = Apache-2.0 OR ISC OR MIT；ring = Apache-2.0 AND ISC；rustls-webpki / rustls-pki-types / once_cell / zeroize / cfg-if / getrandom / libc / cc = ISC 或 MIT OR Apache-2.0——均在纪律 4 的「Apache-2.0/ISC/MIT 系」内。
- **唯一例外：subtle 2.6.1 = BSD-3-Clause**（rustls 0.23 硬依赖，无剔除路径）。BSD-3-Clause 是三句版宽松许可（OSI 认可，与 MIT 同族的再分发条件），本计划裁定接受；若用户否决，整刀 rustls 面回退（见「偏差」第 3 条）。
- 分发合规：二进制静态含上述库，Linux 打包脚本在现有许可随包机制上追加 rustls 系 license 声明（仅 Linux 侧，允许面内）；macOS/Windows 打包脚本的同类追加不在本刀允许面，README 许可段落如实登记（见「偏差」第 5 条）。

### 红线

- 只用于 host 工具：依赖加在 `tools/berry/Cargo.toml`（独立 workspace），不进客机、不进 ABI、不进 kernel 侧任何 Cargo 面。
- 不加 rustls 之外的任何新依赖；`Cargo.lock` 变更仅限本次解析结果。
- 证书校验默认严格（按证书目录 `ca-cert.pem` 验链 + SAN）；`--insecure` 是显式逃生门且必打警告；私钥与口令不进日志、不进错误信息、不进 argv（沿既有纪律）。

## 契约

### `vnc check` 的 TLS 路径（VeNCrypt 客户端）

语法：`berry vnc check [--run <id>] [--password <pw>] [--ca <dir>] [--insecure]`。

自动检测：`vnc.listen` 的 `tls=true` 时自动走 TLS 路径（`commands.rs` 现行 `not-supported` 短路删除）。证书目录解析优先级 = `--ca <dir>` 显式覆盖 > run `meta.json` 的 `vnc_tls` 字段（读法照 `control.rs` `vnc` 臂：读 `meta.json` → `vnc_tls` 字符串，过滤 `"off"`）。

`--ca` 与 `--insecure` 互斥（同时给 = rc=2 E_USAGE）；对非 TLS 端点给 `--ca`/`--insecure` 静默忽略（help 注明，沿 F17 先例）。

`--insecure` 语义：跳过证书链与 SAN 校验（rustls danger verifier 全接受），执行前 stderr 打一次性警告：`berry: warning: --insecure skips TLS certificate verification; the connection is open to interception`。

VeNCrypt 状态机（`display/vnc.rs` 新增 `connect_tls`；字节序列 probe p2 实测钉死）：明文 RFB 版本交换 → 服务端 types 含 19 则选 19 → VeNCrypt 版本协商（S→C `0x00 0x02`，C→S 回 `0x00 0x02`；服务端回 0.0 即失败）→ S→C ack `0x00` → S→C u8 count + u32[count] 子类型表 → 客户端按偏好选：**X509Vnc（261）优先、X509None（260）兜底**（纯函数 `vencrypt_choose_subtype`）→ C→S u32 子类型 → S→C u8 状态（`0x01` 接受；`0x00` 即失败断连）→ TLS 握手（rustls；CA = 证书目录 `ca-cert.pem`；SAN 校验，`ServerName` 按 dial host 取 `IpAddress`（127.0.0.1）或 `DnsName`（localhost））→ TLS 内继续子类型对应认证：261 直接 16B DES 挑战（复用 `des::vnc_challenge_response` + `SecurityResult`）；260 直接 ClientInit 无内层认证 → ServerInit 取帧尺寸（复用提取出的 ServerInit 解析段）。

子类型选择矩阵（纯函数单测钉）：offered 含 261 且有口令 → 261；offered 仅 260 → 260（报告 `auth none`）；offered 仅 261 且无口令 → E_PROTOCOL + hint（`pass --password`）；offered 无 261/260 → E_PROTOCOL（不支持的服务端子类型集）。QEMU 实测 `password=on` 时仅回 261；`verify-peer=off` 下 260-only 的实际形态以 implement 阶段实测为准（矩阵两臂都已覆盖）。

rc/hint 语义（TLS 路径）：`meta.json` 无 `vnc_tls` 或证书目录缺 `ca-cert.pem` → rc=2 + hint（`berry vnc certs`）；TLS 握手/验链/SAN 失败 → rc=7 + hint（`--ca <dir>` 指向签发该证书的 CA，或 `--insecure` 逃生）；内层 DES 认证失败 → rc=7 + 既有 passwd hint；成功 rc=0。输出：文本行加 TLS 面——`vnc check: ok WxH via vnc://… (auth vnc, tls x509-vnc)`；JSON 在既有字段上加 `"tls":{"enabled":true,"subtype":"x509-vnc"|"x509-none"}`；明文 run 输出不变（`tls.enabled=false`）。

### `berry vnc web --tls <dir>` 与页面 URL 形态（TLS 终止代理）

语法：`berry vnc web [--run <id>] [--port N] [--max-conns N] [--tls <dir>]`。`--tls` 显式给目录（经 `validate_tls_dir`）；省略时若 run 为 TLS 端点则自动用 `meta.json` `vnc_tls` 目录（同 check 的推法）；两者皆无 = 现行 http 页行为零变化（明文 run 完全不回归）。

TLS 模式下 berry 起两个监听（同进程、同 `--max-conns` 准入）：

- **https 页面口**（`--port`，rustls `ServerConfig` = 同目录 `server-cert.pem` + `server-key.pem`；不设 ALPN，浏览器回落 HTTP/1.1，probe 实证）：`handle_conn`/`write_simple`/`read_head` 泛型化后服务既有静态资产逻辑（sanitize/mime/404/405 全部不变）。
- **wss 代理口**（自动挑空闲端口）：每条连接先 rustls 服务端握手（10s 超时），完成后建立到 QEMU ws 口的 rustls 客户端连接（CA = 同目录 `ca-cert.pem`，SAN 校验，10s 超时；QEMU `verify-peer=off` 仍发 `CertificateRequest`，rustls 发空证书应答，probe 实测被接受），随后**纯字节双向管道**（两线程各一向，任一方向 EOF/错误即双向 shutdown）——不解 HTTP、不解 ws 帧，Upgrade/101/mask 全由两端点自理。此为 tap PoC 已验全通形态（含真 noVNC + DES 口令认证）。

页面 URL 形态（noVNC 1.7.0 实测修正，见「偏差」第 2 条）：TLS 模式用 `https://127.0.0.1:<page_port>/vnc.html?autoconnect=true&host=127.0.0.1&port=<proxy_port>&encrypt=true&resize=off` + 既有 `#password=` fragment 规则——**不用** `path=<wss URL>` 形态（1.7.0 不识别 wss URL 会回落明文连接；QEMU ws 端点也只认根路径）。明文 run 维持现行 `path=<ws URI>&resize=scale` 形态不动。`--tls` 用于明文 run 时页面走 https、ws 部分维持 `path=ws://…` 直连（localhost 混合内容豁免 probe 实证；非 localhost dial 时打一次性警告）。

JSON 输出加 `"proxy_port"`（TLS 模式）；文本输出第二行说明 https + 代理口。数据面例外要如实写进 `web.rs` 模块头注释与 README：第二波「berry never proxies any RFB bytes」原则在 TLS run 下让位于 TLS 终止（动机 = Chrome wss 客户端证书墙，probe 定音），明文 run 仍零代理。

### F16：passwd 轮换后 expire 复位失败的告警事件

`control.rs` `vnc_passwd` 臂的 `let _ = q.expire_vnc_password("never")` 改为：失败时 `append_event` 一条事件，不再静默。事件 JSONL 形态（纯函数 `passwd_warn_event` 构造，单测钉死，同 `t:action` 族）：

```json
{"v":1,"t":"action","action":"vnc.passwd_warn","error":"<QMP 错误摘要>","source":"control"}
```

错误摘要取 QMP 错误消息（该调用参数是 `"never"`，不含口令材料；沿「口令不进事件/日志/错误」纪律）。轮换主流程不变：复位失败不使 `vnc passwd` 命令失败（口令本身已换成功），事件是审计面信号。

### F18：web 连接总时间预算

`web.rs` 新常量（带注释）：

```rust
/// Whole-connection budget: hard cap over the entire head+body exchange.
/// Slow-drip clients (one byte every few seconds) can hold a slot forever
/// under per-read timeouts alone; 30s covers the slowest legitimate browser
/// asset load while starving that attack. Exceeded -> drop the connection,
/// the RAII slot recycles.
pub const CONN_BUDGET: Duration = Duration::from_secs(30);
```

行为：`handle_conn` 记 `Instant` 起点，`read_head` 循环每轮与响应写出前检查预算，超时直接返回（连接关闭，槽释放；不写 408——对慢滴/死客户端没有意义）。**预算只作用于页面服务的请求-响应连接**；wss 代理连接是数据面长连接（桌面会话可持续数小时），不设总预算，仅 TLS 握手阶段有 10s 超时、管道阶段读写沿用 OS 语义。

### F20：vncpass-* 孤儿清扫

`vnc_cfg.rs` 新增：常量 `PASS_ORPHAN_AGE: Duration = 3600s`（1h——活跃交接窗口远小于此，正常文件 mtime 恒新不会被误删；并行会话即使同刻在写，其文件 mtime 为当前时刻，同样安全）；纯函数 `is_stale_vncpass(file_name, mtime, now)`（文件名以 `vncpass-` 开头且龄超阈值）；`sweep_orphan_passes(dir, now)` 在 `write_pass_temp` 开头调用（写新文件前清同目录陈旧孤儿）。扫不动（目录读不了、单个删除失败如权限）一律忽略不失败——清扫是尽力而为的卫生面，不能阻断口令交接主流程。时间以注入 `now` 参数（可测性：测试传未来时刻即可让现造文件判旧，零平台 mtime hack）。

### Linux 分发链实机化

步骤（probe p5 已在 mac 宿主实跑全通，本刀产正式产物）：`tools/berry/packaging/linux/build-linux-binary.sh`（musl 静态 ELF）→ `build-appdir.sh`（含 web 资产）→ `check-appdir.sh`（0 fail 门）→ `build-tarball.sh`。产物清单（`out/`）：静态 ELF、`BerryOS.AppDir-linux`（命名按脚本现行）、`berry-linux-<arch>.tar.gz` + sha256。

docker 追加项（probe p4：`open -a OrbStack` 2s 唤醒可用）：唤醒后 `docker info` 确认，再跑 `linux/build-appimage-docker.sh` 产 AppImage、`windows/build-cross-docker.sh` + `windows/assemble.sh` 产 Windows 包；容器冒烟（`linux/smoke-docker.sh`，容器内 `apk add qemu-system-aarch64`）能跑则跑。任一步失败（docker 起不来、容器冒烟不过）如实记录进验收记录，不硬造、不装任何东西。

顺带验证项：打包产物里 `vnc web` 的 `web_roots` 解析静态推演——AppDir 的 exe 在 `usr/bin` → `parent.join("share").join("berry").join("web")` 即 `usr/share/berry/web`（`web.rs` `web_roots` 第三落点），与 `build-appdir.sh` 资产拷贝位一一对应；`check-appdir.sh` 的 web 资产检查过 = 链路成立。引入 rustls 后 musl 交叉链须复验（ring 构建期 cc；probe p5 实跑时树上尚无 rustls，此为本刀新风险点，见「偏差」第 7 条）。

### 条件项裁决

- `.gitignore` 六项：**补上**。probe p6 复核六路径全部 NOT IGNORED、合计约 2.0G、展开未跟踪 6120 条；声索面只有第四波 survey F23（「与并行线协调后一并于后续刀补」）与 `research/桌面UX审计/04-实施波一复核.md:139`（把五项归「构建残渣/卫生项」，非所有权声索），无竞争性 ACTIVE 声索。落法：根 `.gitignore` 追加 `/target-cargo/`、`/.dsh-target/`、`/.dsh-worktrees/`、`/kernel/kernel/`、`/.berry-review/`，`tools/berry/.gitignore` 追加 `/.bt/`，各带一行来源注释（probe 2026-11-13 裁决）。影响：`git status` 未跟踪污染消除，忽略前已入库文件不受影响（六路径均纯未跟踪）。
- `window.rs` 解冻直参：**顺延**。probe 确认 `window.rs`（末次提交 01028a10）与 `profile.rs`（4063e879）均为并行会话活跃改动 M 态，冻结维持；`--vnc` 走 `LAUNCH_STICKY` 进程级覆盖的现状不动，本计划与验收均不依赖解冻。

## 步骤总览

1. [x] 依赖落盘：`Cargo.toml` + rustls 两行 → `cargo fetch` 解析 → `Cargo.lock` 核对变更面与许可清单核对（无 subtle 之外的例外即继续）。
2. [x] `tls.rs` 新模块（配置装载：客户端严格/客户端 insecure/服务端；错误路径不含私钥内容）+ `display/vnc.rs` VeNCrypt 状态机与 `connect_tls`（`handshake_auth` 尾段提取共用，泛型化）——单测先行。
3. [x] `vnc check` TLS 路径接线（`commands.rs`/`main.rs`）+ rc/hint 契约。
4. [x] `web.rs` https 与 wss 代理 + F18 预算（泛型化 + 常量）——单测。
5. [x] F16（`control.rs` 纯函数 + 事件）与 F20（`vnc_cfg.rs` 清扫）——单测。
6. [x] `justfile` 增 `berry-vnc-tls-smoke` 配方（自起自收，显式 `--run`）。
7. [x] 冒烟 t1..t12 + 全量门三件套。
8. [x] Linux 链四脚本正式产物 + docker 追加项（按实际可用性）。
9. [x] `.gitignore` 六项（条件项落地）。
10. [x] docs 面：README、CLI-first 升 v1.4、沿革新节（落笔时实时重数尾部，probe 基准 = 二百五十五）、Agent Note 落稿、本计划收口 `done`、`check-docs` 零告警。

## 文件改动清单

符号级锚点（行号以落笔时工作树为准，会漂，以符号为准）：

| 文件 | 改动 |
|---|---|
| `tools/berry/Cargo.toml` | `[dependencies]` 追加 `rustls`（0.23，`default-features=false`，features `["std","ring","tls12"]`）与 `rustls-pki-types`（1，features `["std"]`）；不加其他任何项 |
| `tools/berry/Cargo.lock` | 仅本次解析结果（预声明 windows/wasi 目标门控条目） |
| `tools/berry/src/lib.rs` | 模块表追加 `pub mod tls;` |
| `tools/berry/src/tls.rs`（新建） | `load_client_config(ca: &Path)`（严格：`ca-cert.pem` 验链根）、`load_client_config_insecure()`（danger verifier 全接受）、`load_server_config(dir: &Path)`（`server-cert.pem`+`server-key.pem`）；`server_name(host) -> ServerName`（IP→`IpAddress`、名→`DnsName`）；错误路径：缺件 rc=2 + `berry vnc certs` hint、坏 PEM 报文件名不报内容；单测覆盖装载错误路径与 insecure 构造 |
| `tools/berry/src/display/vnc.rs` | `handshake_auth` 尾段（ClientInit/ServerInit 解析）提取为 `rfb_tail<S: Read+Write>` 共用，本体泛型化；新增 `connect_tls(endpoint, password, client_cfg, server_name)`（VeNCrypt 状态机 + rustls 握手 + TLS 内层认证）；纯函数 `vencrypt_choose_subtype(offered, password)` 与版本协商字节常量；`VncSource` 增 `tls: Option<&'static str>` 子类型字段与 `tls()` 访问器；既有 types 含 19 的明文臂 hint 更新（check 现在能过 TLS）；单测：子类型矩阵、版本字节、rfb_tail 回归（既有三态测试不破） |
| `tools/berry/src/commands.rs` | `vnc_check`：删 `listen.tls` not-supported 短路，签名加 `ca`/`insecure`，TLS 路径按契约（meta 推目录/覆盖/互斥/警告/rc）；`vnc_web`：签名加 `tls: Option<&Path>`，TLS 模式起 https + wss 代理（`tls.rs` 配置 + 代理线程组），URL 构造分流，JSON 加 `proxy_port` |
| `tools/berry/src/main.rs` | 仅 VNC 布线：`VncCheckCmd` 加 `--ca`/`--insecure`（help 注明仅 TLS 端点有效、二者互斥）；`VncWebCmd` 加 `--tls <dir>`；dispatch 两处传参 |
| `tools/berry/src/web.rs` | `serve`/`serve_listener` 加 `tls: Option<Arc<rustls::ServerConfig>>` 参数（None 行为零变化）；`handle_conn`/`write_simple`/`read_head` 泛型化（`impl Read`/`impl Write`）；`CONN_BUDGET` 常量与超时检查；新增 `build_page_url_tls(page_port, host, proxy_port, password)`；新增 wss 代理服务函数（`serve_wss_proxy`：准入闸同 `max_conns`、TLS 握手 10s、字节管道两线程）；模块头注释更新（TLS 终止例外如实声明）；单测：预算超时断开（短预算参数化）、TLS URL 形态、既有测试全保 |
| `tools/berry/src/control.rs` | `vnc_passwd` 臂 `expire_vnc_password("never")` 失败分支调 `append_event`；纯函数 `passwd_warn_event(err) -> Value`；单测钉事件 JSONL 形态 |
| `tools/berry/src/vnc_cfg.rs` | `PASS_ORPHAN_AGE` 常量、`is_stale_vncpass` 纯函数、`sweep_orphan_passes`；`write_pass_temp` 开头调用；单测：阈值判定矩阵 + 注入未来 `now` 的目录清扫（新文件不删、判旧全删、坏目录不 panic） |
| `justfile` | 新配方 `berry-vnc-tls-smoke`（certs → TLS run 起档 → status 断言 `tls.enabled` → check 自动/--ca/--insecure/错口令 rc=7 → trap 显式 `--run` 自收）；既有 `berry-vnc-smoke` 不动 |
| `tools/berry/packaging/linux/build-appdir.sh` | 许可随包机制追加 rustls 系 license 声明（仅当该机制存在落点；如实登记 BSD-3-Clause） |
| `tools/berry/README.md` | TLS 段重写（check 过 VeNCrypt、`--ca`/`--insecure`、`vnc web --tls` https 页 + TLS 终止代理、自签浏览器告警说明、noVNC URL 形态修正）；wss 边界段改写（第二波「留未来决策」的 wss 终止已落）；Linux 链用法段；许可登记段 |
| `docs/Berry设备平台-CLI-first设计-v1.0.md` | 升 v1.4：§7/§8（check TLS 参数、web --tls）、§12（Linux 链）、§13（TLS 终止代理与数据面例外、--insecure 逃生门）、§15 现状映射 |
| `docs/阶段沿革.md` | 新节（VNC 第五波）；节号落笔时实时重数尾部（probe 基准二百五十五，尾部已并存两个二百五十四，docs 阶段动手前必须重新数） |
| `.gitignore` + `tools/berry/.gitignore` | 六项忽略（条件项裁决落地，带来源注释） |
| `.agents/notes/implemented/2026-11-13-vnc-complete-wave5.md` | implement 阶段新建（Alternatives：rustls vs openssl 子进程 vs python 辅助 vs 维持 not-supported、https 页 vs ws 代理、连接预算取值 30s、孤儿清扫阈值 1h） |
| `out/` | Linux 链正式产物（ELF/AppDir/tar.gz/sha256；docker 可用则 AppImage + Windows 包）与冒烟截图 |

不改：`qmp.rs`（事件泵本刀无涉）、`qemu.rs`/`device.rs`（QEMU argv 与 meta 布局已满足需求）、`window.rs`/`profile.rs`/`main.rs` profile hunk（冻结面）、`display/des.rs`、web 资产树。

## 测试矩阵

### 单元测试（cargo test，64 基线全保 + 新增）

| 编号 | 面 | 断言 |
|---|---|---|
| U25 | `vencrypt_choose_subtype` | offered [261]+pw→261；[261] 无 pw→E_PROTOCOL+`--password` hint；[260]→260；[261,260]+pw→261；[]/[18]/[17]→不支持错 |
| U26 | VeNCrypt 版本协商字节 | 常量/序列钉死（`0x00 0x02` 双向、ack `0x00`、接受位 `0x01`） |
| U27 | `tls.rs` 装载错误路径 | 缺 `ca-cert.pem`→rc=2+certs hint；缺 `server-key.pem`→服务端配置错；坏 PEM→报文件名不报内容；insecure 构造不 panic |
| U28 | `rfb_tail` 提取回归 | 既有 `handshake_auth` 三态测试（none/挑战对拍/auth-fail 清洗）全绿不改断言 |
| U29 | F16 事件形态 | `passwd_warn_event` 输出逐字段钉死（v/t/action/error/source），错误文本不含口令样串 |
| U30 | F18 预算 | `handle_conn` 以 1s 短预算 + 慢滴客户端（0.2s/字节）真 TCP 测试：约 1s 后连接被服务端关闭（读到 EOF），槽回收后新连接 200 |
| U31 | F20 阈值与清扫 | `is_stale_vncpass` 边界（3599s 不删/3601s 删/非 `vncpass-` 前缀不删）；`sweep_orphan_passes(now=+2h)` 现造文件全删、`now=当前` 不删、不存在目录不 panic |
| U32 | TLS 页面 URL | `build_page_url_tls` 逐字符钉死（https + host/port/encrypt=true/resize=off + fragment 编码）；明文 `build_page_url` 输出不变 |
| U33 | 代理管道 | 字节管道纯回归：内存双端（或环回 TCP + 明文配置）双向透传逐字节一致、一方 EOF 即双方关 |

### 冒烟（t1..t12，实机 QEMU 11.0.3；run 全程显式 `--run` 自起自收）

| 编号 | 项 | 预期 |
|---|---|---|
| t1 | `berry vnc certs /tmp/vncw5/certs` | 目录四件齐、verify OK、指纹输出 |
| t2 | `device start --vnc on --vnc-ws on --vnc-tls <dir> --vnc-pass <pw> --json` + `vnc status --json` | run 起；`tls.enabled=true`、`tls.dir` 指证书目录、`ws_uri` 为 wss |
| t3 | `vnc check --run <id>`（自动推目录） | rc=0；`auth vnc`、`tls x509-vnc`；帧尺寸真值 |
| t4 | `vnc check --ca <dir>` 显式 | rc=0 同 t3 |
| t5 | `vnc check --insecure` | rc=0；stderr 有 warning 行 |
| t6 | `vnc check --password 错串` | rc=7 auth failed（TLS 内 DES 路径） |
| t7 | `vnc web`（TLS run 自动 https）后台 + Chrome headless `--ignore-certificate-errors --screenshot` 打开打印 URL | 截图非空、含真实 GUI 桌面像素（非错误页/noVNC 断连态）；进程组自收 |
| t8 | F18 实测：python 慢滴客户端占满页面口全部槽 | 30s 预算后被断开；新连接立即 200 |
| t9 | F20 实测：手工造 `~/.berry/tmp/vncpass-*` 旧 mtime 文件（touch -t）后触发一次 `write_pass_temp`（如 restart 链） | 孤儿被清、新交接文件不受影响 |
| t10 | F16 冒烟（成功路径不回归）：`vnc passwd --set` 后读 `events.jsonl` 尾行 | `t=action action=vnc.passwd` 事件在；warn 分支以 U29 覆盖（live 造失败需断 QMP，不值） |
| t11 | Linux 链：四脚本依序跑 | ELF 静态、AppDir `check-appdir.sh` 0 fail、tarball+sha256 落 `out/`；docker 醒则追加 AppImage + Windows 包 + 容器冒烟（失败如实记录） |
| t12 | `just berry-vnc-smoke` + `just berry-vnc-tls-smoke` | 两配方全绿、自收净、并行会话 run 零触碰 |

### 全量门

`cd tools/berry && PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/cargo-target" cargo test --release --target "$(rustc -vV | sed -n 's/^host: //p')` 全绿；仓库根 `just berry-build` 绿（含 docs_guard）；仓库根 `node .agents/tools/check-docs.mjs` 零告警。

## 验收标准

1. 单测 64 + 新增（U25-U33）全绿；三件套门全过。
2. `vnc check` 对 TLS run：自动/--ca/--insecure 三路 rc=0，错口令 rc=7，明文 run 输出与旧行为逐字节一致（不回归）。
3. TLS run 的 `vnc web`：https URL + Chrome headless 截图实证真实 GUI 桌面（t7）；明文 run 的 `vnc web` 行为零变化。
4. F16/F18/F20 落地且各有单测钉死；t8/t9/t10 实测通过。
5. Linux 链产物齐落 `out/`（docker 追加项按实际可用性，失败如实记录）；web_roots AppDir 解析静态推演确认写进验收记录。
6. `.gitignore` 六项落；`git status` 未跟踪条目数显著下降（对照 probe 的 6120 条基线）；已入库文件零影响。
7. README/CLI-first v1.4/沿革新节/Agent Note 齐备；本计划收口 `done`；`check-docs` 零告警。
8. 纪律面：不 git add/commit；冻结面（`window.rs`/`profile.rs`/`main.rs` profile hunk/`arch/`/`kernel/`/`sched/`/`user/`/`research/` 等）零触碰；敏感物（证书私钥）只落 `/tmp`；自起 run 全收净。

## 边界（非目标）

- SetDesktopSize 代理翻译：需 berry 终结客户端连接并翻译 RFB 消息的完整代理架构（本刀代理是字节管道，不解帧），另立项。
- 剪贴板、SASL：不在面。
- 真机（非容器）Windows/Linux GUI 实测：容器冒烟是上限（docker 可用时）；真机面维持「未验」如实文档化。
- noVNC 升版：维持 vendored 1.7.0。
- `window.rs` 解冻与 `--vnc` 直参化：并行会话属地，顺延。
- macOS/Windows 打包脚本的 rustls 许可随包追加：不在允许面，留后续刀（README 已登记）。
- QMP 事件泵、QEMU argv、meta 布局：不动。

## 偏差

写作时点已知的与 SPEC 直觉/前波记录的出入（implement 阶段如有新增在此续记）：

1. **浏览器路径架构修正（probe p3 定音）**：QEMU 11.0.3（gnutls）对 TLS 端点（RFB 与 ws 皆是）在 `verify-peer=off` 下仍发 TLS `CertificateRequest`，Chrome headless 的 wss 直连以 `ERR_SSL_CLIENT_AUTH_CERT_NEEDED` 静默弃连（`--ignore-certificate-errors` 只覆盖证书错误，不覆盖客户端证书请求）——「https 页 + wss 直连 QEMU」不可作为验收目标，浏览器路径必须 berry 侧 rustls 终止/代理（tap PoC 已验全通）；副作用 = TLS run 下 berry 代理 RFB 字节，第二波「零代理」原则的显式例外。
2. **noVNC URL 形态修正**：第二波记录的 `path=<wss URL>` 提示形态对 noVNC 1.7.0 的 wss 场景不成立（会回落明文连接）；正确形态 = `host+port+encrypt=true+path=(空)+resize=off`，且 QEMU ws 端点只认根路径。明文 run 的 `path=<ws URL>&resize=scale` 是既有验收过的路径，维持不动。CLI-first v1.4 与 README 携带此修正。
3. **subtle 2.6.1 = BSD-3-Clause**：在纪律 4「Apache-2.0/ISC/MIT 系」严格清单之外；三句版宽松许可、rustls 无剔除路径，本计划裁定接受并在此显式请用户知悉——否决即回退整刀 rustls 面。
4. **Cargo.lock 目标门控条目**：windows-sys/windows-targets/windows_*/wasi 进锁不编译，预声明避免评审误判依赖膨胀。
5. **docs/索引.md 索引行**：CLI-first 升 v1.4 后 `docs/索引.md` 该行按 F06 先例应同步注记，但该文件不在本刀允许面——留待用户/后续刀（或用户追加授权），本计划只登记。
6. **沿革节号实时态**：probe 时尾部已并存两个「二百五十四」（VNC 三波与显示加速），二百五十五只是基准；docs 阶段落笔前必须重新数尾。
7. **musl + ring 交叉链未验**：probe p5 的 Linux 链实跑时树上尚无 rustls；ring 构建期 cc 对 musl 目标的实态是 implement 阶段第 8 步的前置复验项，失败则按 fallback 记录（本地链换 target 或如实降级记录），不硬造。
8. **F16 live 失败分支不可造**：warn 分支需 QMP 调用真实失败（断连/协议错），live 构造成本高于收益——单测钉事件形态（U29）+ 成功路径冒烟（t10）为验收面。

implement 阶段新增（2026-11-13/14，全部已消化或如实登记）：

9. **TLS 页面 URL 的 `path=` 必须参数在场且值为空**：契约正文「path=(空)」初版被实现成「参数缺席」；noVNC 1.7.0 在参数缺席时默认拼 `websockify`，QEMU 根路径 404 → 页面静默不连（Chrome DOM 状态为空、QEMU clients 零）。修正为 `…&encrypt=true&path=&resize=off`（参数在场、值为空），`build_page_url_tls` 单测逐字符钉死。
10. **rustls Cursor 喂入的两个 EOF 陷阱（wss 代理实现发现）**：(a) `read_tls` 从空 Cursor 读到 `Ok(0)` 会被 latched 为传输 EOF（`has_seen_eof`），会话当场判死——空块不得进 `read_tls`；(b) 喂入循环必须按「剩余字节数」驱动，不能读到 `Ok(0)` 收尾（同理 latch）。端到端代理测试覆盖。
11. **rustls `Reader::read` 空缓冲返回 `Err(WouldBlock)`**（非普通流语义）：会话明文排水不能用 `read_to_end`（首个 WouldBlock 即外抛）；逐块 `read`，`WouldBlock` 释义为「暂无更多明文」正常返回。与 probe 已知的 `UnexpectedEof` 坑（按半关闭容忍）一并写进代理代码注释。
12. **t7 像素判据受镜像态限制**：当前 `out/berryos-portrait.img`（UITEST 构建）与 `out/berryos.img`（console 档）的 VNC 帧缓冲全黑（`berry screenshot` 地面真值 100% rgb(0,0,0)，120s 长等亦然），「真实 GUI 桌面像素」不可演示。t7 以三层证据替代：(1) python 经代理全链（101 + RFB 横幅 + DES 认证）；(2) Chrome `--dump-dom` 3/3「已连接（已加密）到QEMU」；(3) Chrome 截图 canvas 区 39.9% 纯黑与地面真值一致（帧缓冲被真实渲染并绘制到画布）。镜像像素内容属并行会话属地，不追。
13. **`vnc status` 的 clients 计数作时序判据不可靠**（查询时滞/生命周期错位）：t7 连接判据改用 noVNC DOM 状态文本；clients 计数仅在受控保持会话（python 握横幅后 hold）场景有效。
14. **ring+musl 交叉链实态（第 7 条的解决）**：本机无 musl 交叉 gcc；Apple clang（17.0.0）+ `-ffreestanding` 可编 ring 的 C 面，唯 clang 内置 `stddef.h` 的 musl 分支无条件 `include_next` 无 sysroot 可接——自足 shim `stddef.h` 经 `-isystem` 前置解决（`stdint.h` 无需 shim：其 `include_next` 受 `__STDC_HOSTED__` 保护，`-ffreestanding` 已清）。方案已打进 `build-linux-binary.sh`（环境已有 `CC_*` 时不覆盖），产物 ELF 静态 4.95MB stripped。
15. **`--tls` 施于明文 run 的 https 页分支浏览器实测未通过**：隔离实验（https 页 + `path=ws://127.0.0.1` 明文直连）单次 DOM 不连；p3 的混合内容豁免证据来自裸 WebSocket 简易页而非 noVNC 的 `path=` 形态。分支保留（实现 + `build_page_url_https` 单测 + 非 loopback 警告），文档按「实测未通过」如实登记，TLS run 主路径（代理形态）不受影响。
16. **probe tarball 被正式产物取代**：`out/BerryOS-linux-aarch64-probe.tar.gz` 删除（同名义正式件 23078348B 落位）；probe AppDir（`out/BerryOS.AppDir-linux-probe`）保留。
17. **`tools/berry/.gitignore` 追加 `/cargo-target-linux/`**：六项裁决之外，本刀 Linux 链自身构建缓存（同既有 `/cargo-target/` 口径，一行注释指明）。

## 未完成事项

implement（2026-11-13/14）与 docs（2026-11-14）两阶段全部落地，计划收口 `done`。真实遗留（均已在边界/偏差/README 已知限制登记，非本刀欠账）：

1. `docs/索引.md` 的 CLI-first 索引行按 F06 先例应随 v1.4 注记——该文件不在本刀允许面（偏差第 5 条），留用户/后续刀。
2. macOS/Windows 打包脚本的 rustls 许可随包登记（README 许可段已先行登记）。
3. `window.rs` 解冻与 `--vnc` 直参化（并行会话属地，顺延）。
4. SetDesktopSize 代理翻译、剪贴板、SASL、裸机 Linux/Windows 安装与桌面集成、`--tls` 施于明文 run 的 https 页分支（实测未通过按未验登记）——见「边界（非目标）」与 README 已知限制。

implement 阶段完成态快照（证据摘要）：

- 单测 83 绿（64 基线 + 19 新增：tls.rs 6、vnc.rs 5（含端到端 TLS loopback 3）、web.rs 4、vnc_cfg.rs 2、control.rs 2）；`just berry-build` 绿（docs_guard PASS）。
- `just berry-vnc-tls-smoke` 全绿（certs/TLS run/status/check 四路+互斥 rc=2）；`just berry-vnc-smoke` 零回归。
- t7/t8/t9/t10 实测通过（t7 证据链见偏差 12/13；t8 慢滴占 2 槽 30s 预算释放后新客 200；t9 陈旧孤儿清、新件保；t10 vnc.passwd 事件在）。
- Linux 链：musl 静态 ELF（4.95MB）→ AppDir（33 ok/0 warn/0 fail，THIRD-PARTY-NOTICES.txt 含 BSD-3-Clause subtle）→ tarball（out/BerryOS-linux-aarch64.tar.gz，sha256=82a3167f…）；docker 醒后 AppImage（out/BerryOS-aarch64.AppImage）+ Windows 双 exe（PE32+ console/GUI）+ win64 目录与 zip + 容器冒烟（doctor+phone/phone20 截图）+ AppImage FUSE 冒烟全过。
- web_roots AppDir 推演：exe 在 `usr/bin` → `../share/berry/web` 即资产拷贝位，check-appdir web 资产检查（含 NOTICE.md 等 5 件）过 = 链路成立。
- `.gitignore` 六项+2 落位：未跟踪 6120 → 17（全为真实文档/源码/产物件）。

## 验收记录（2026-11-14，verify 阶段真机实测）

v1 TLS check 端到端：`berry vnc certs /tmp/vncw5v/certs` 出四件 pem 且 `openssl verify -CAfile ca-cert.pem server-cert.pem` OK（SAN=IP:127.0.0.1+DNS:localhost）；`device start --vnc 127.0.0.1:59671 --vnc-ws on --vnc-tls <dir> --vnc-pass <pw>` 起 run 后 `vnc status --json` 报 `tls{enabled:true,dir,vencrypt:x509-vnc}`、`ws_uri=wss://…`。
`vnc check`（自动推目录）rc=0 输出 `vnc check: ok 480x1000 via vnc://127.0.0.1:59671 (auth vnc, tls x509-vnc)`，`--json` 带 `"tls":{"enabled":true,"subtype":"x509-vnc"}`；`--ca <dir>` 显式 rc=0 同文；`--insecure` rc=0 且 stderr 有 `berry: warning: --insecure skips TLS certificate verification; the connection is open to interception`；错口令 rc=7 带 passwd hint。
负例三态：`--ca` 指空目录/不存在目录 rc=2 + `missing ca-cert.pem` + `berry vnc certs` hint；`--ca` 指无关 CA rc=7 `TLS handshake failed: invalid peer certificate: UnknownIssuer` + `--ca`/`--insecure` hint；`--ca`+`--insecure` 同给 rc=2 互斥。
明文不回归：另起 `--vnc on --vnc-ws on --vnc-pass` run，status/check 文本与 JSON 输出形态与旧行为一致（JSON 无 tls 字段、文本无 `, tls x509-vnc` 尾注），错口令仍 rc=7，明文端点给 `--ca` 静默忽略仍 rc=0。
v2 https 网页桌面：TLS run 上 `berry vnc web --run <id>`（自动 TLS 模式）输出 `url=https://127.0.0.1:<page>/vnc.html?autoconnect=true&host=127.0.0.1&port=<proxy>&encrypt=true&path=&resize=off#password=…` 并带 `proxy_port` 字段。
Chrome headless（`--headless=new --ignore-certificate-errors --virtual-time-budget=15000`）`--dump-dom` 命中 `已连接（已加密）到QEMU`——noVNC 1.7.0 连接态文本，全链=https 页 → wss 代理 → TLS → DES 口令。
像素对拍：out/berryos.img（console 档）帧缓冲地面真值 unique_colors=1 全黑（偏差 12 复核成立），换 out/berryos-5key.img（真 GUI，真值黑 445605px/白 16000px/灰 14310px）重走全链后 Chrome `--screenshot out/vnc-w5-tls-browser.png`（1024x900）unique_colors=105、画布区黑 445605px/白 16011px/灰 14310px 与真值逐项吻合——帧缓冲真实流到浏览器画布。
连接证据：run `events.jsonl` 实录 `{"client":{…,"websocket":true},"server":{"auth":"vencrypt+x509+vnc",…},"t":"vnc.connected"}` 与后续 `vnc.initialized`/`vnc.disconnected`——QMP 侧 websocket 客户端经代理完成 RFB 初始化。
v3 F16/F18/F20：F18 python 慢滴实测（明文 `vnc web --max-conns 2`）：两客户端各发 1 字节后挂起占满槽，第三客户端立即 `HTTP/1.1 503 Service Unavailable`；占槽连接在 connect+30s 整被服务端关闭（两连接同刻 EOF，脚本总耗时 30.0s=CONN_BUDGET）；随后新客户端 `GET /vnc.html` 得 `HTTP/1.1 200 OK`（17936B）——槽回收可入。
F20 实测：`~/.berry/tmp` 造 `vncpass-999999-1`（`touch -t 202001010000`）+ 新鲜 `vncpass-*` + 无关旧件 `unrelated-stale.txt`，经 `device start --vnc on --vnc-pass` 触发 write_pass_temp 后陈旧孤儿消失、新鲜件与无关件保留（真实交接件被子进程即读即删）。
F16 验证层级=单测+代码走查（live 造 QMP 失败需断连不值，沿偏差 8）：control.rs vnc_passwd 臂现为 `if let Err(e) = q.expire_vnc_password("never") { append_event(run, passwd_warn_event(&e.message)); }`，单测 `passwd_warn_event_shape`（五字段+JSONL 单行往返）与 `passwd_warn_event_carries_no_password_material` 均 ok；成功路径 `vnc passwd --set` 后 events.jsonl 有 `{"action":"vnc.passwd","source":"control","t":"action","v":1}` 且 `events --filter vnc.passwd` 命中。
v4 Linux 产物（2026-11-14 复跑全链）：build-linux-binary.sh EXIT=0 产 musl 静态 ELF（`ELF 64-bit LSB executable, ARM aarch64, statically linked, stripped`，4950136B）；build-appdir.sh 打 out/BerryOS.AppDir-linux（182M，web=NOTICE.md+novnc/）；check-appdir.sh `--require-elf` 报 `33 ok, 0 warning(s), 0 failure(s)`；build-tarball.sh 产 out/BerryOS-linux-aarch64.tar.gz（23078511B，sha256=0c95982437f0f7801594506d03aa807a5eab4c9d21d0ee313cc24a8d14964606）。
THIRD-PARTY-NOTICES.txt 登记 rustls=Apache-2.0 OR ISC OR MIT 与 subtle=BSD-3-Clause（three-clause; hard rustls dep）；web_roots 推演：AppDir exe 在 usr/bin → `../share/berry/web` 与资产拷贝位一一对应，check-appdir web 资产检查过=链路成立。
docker 复验（daemon 醒）：smoke-appimage.sh 容器内 FUSE 挂载跑 `berry 0.1.0 (aarch64-linux)` + doctor（firmware/image 从挂载点读）EXIT=0；smoke-docker.sh 容器内（alpine:3.20 + qemu 9.0.2）真 Linux ELF 起 desktop 镜像两档截图 phone.png 480x1000/phone20.png 540x1200 EXIT=0；Windows 件 file 检视 berry.exe/berry-shell.exe=PE32+ console/GUI x86-64（implement 阶段容器产物复验在位）。
v5 全量回归：cargo test `--release` 83 passed/0 failed（含 vencrypt_subtype_matrix、vencrypt_version_negotiation_bytes、tls 6 项、connection_budget_drops_slow_drip_client、stale_vncpass_threshold_matrix、sweep_orphan_passes_directory_behavior、passwd_warn_event 两项）；just berry-build 绿（docs_guard PASS 0 警）；just berry-smoke 绿（marker `BerryOS boot` + berry-shot.png 800x600）；just berry-vnc-smoke 绿（uri+ws_uri/口令三态 rc=7）；just berry-vnc-tls-smoke 绿（certs/status/check 四路+互斥 rc=2）。
明文既有路径抽查（专起 run）：status/check/web（http 页 200）/passwd 轮换（轮换后新口令 rc=0 旧口令 rc=7）/events `--filter` 全过。
自清：全部自起 run 显式 `--run` stop+cleanup、`berry vnc web` 进程全杀、/tmp/vncw5v 与 ~/.berry/tmp 测试残留清空、device list 无 host-labeled run 残留、无 berry web 进程；产物保留 out/（vnc-w5-tls-browser.png、BerryOS.AppDir-linux、BerryOS-linux-aarch64.tar.gz、linux-smoke/ 三件、BerryOS-aarch64.AppImage、BerryOS-win64 双 exe+zip）。
干扰与观测：verify 中途两个自起 host run（berryos.img TLS 档与明文档）被外部（并行会话面）清删——run 目录消失而 QEMU 原在运行，本人命令面核过无 stop/cleanup 非自身 run 的调用，重起 run 续跑未影响验收；另有一次 `device start` 瞬态 `host exited before announcing run`（rc=6）原样重试即起，未复现，记录备查。

## 验收记录（2026-11-14，docs 阶段）

1. `tools/berry/README.md`：命令树补 `vnc check [--ca] [--insecure]`、`vnc web --tls` 与 `berry-vnc-smoke`/`berry-vnc-tls-smoke` 两配方行；TLS 段重写——删 not-supported 旧文，落 check 的 VeNCrypt 路径与 `--ca`/`--insecure` 语义（缺省推目录/互斥 rc=2/严格校验/失败形态三态）；新增「VNC 第五波·彻底完善」段（https 页 + wss 终止代理、URL `path=` 空值形态、自签浏览器告警与 `--ignore-certificate-errors` 指引、P2 三修）；客户端矩阵 noVNC 行、Chrome headless 指引补 TLS 形态、第一波段 `ws_uri` 表述补 TLS 档不可直连；「双击包」节落 Linux/Windows 分发链产物段（四脚本+docker 追加项+web_roots 推演+链接 packaging/linux/README.md）与 rustls 许可登记（含 BSD-3-Clause subtle 例外）；已知限制重写 Windows/Linux 实机化条目与 wss/TLS 边界条目（ berry 代理唯一场景/明文 run 零代理/`--tls` 施明文 run 未验）；测试段补两冒烟配方。
2. `tools/berry/packaging/linux/README.md`：限制段两条过时表述校准（AppDir 已在 arm64 Linux 容器实跑、AppImage/FUSE 已容器内验证；裸机与 .desktop 注册维持未验）。
3. CLI-first 升 v1.4：头块版本行 + 【v1.4】注记；§8.1 命令树 `vnc check`/`vnc web` 两行与 v1.4 波次段落；§12 补 Linux/Windows 分发链实机化条目；§13 安全表 TLS 行改 v1.2/v1.4（check 过 VeNCrypt、校验默认严格、`--insecure` 显式逃生门、TLS run 代理例外）；§14 刀序表新增「VNC 彻底完善（已落，v1.4）」行；§15 映射 TLS 行与三平台包行更新——未重构，其余节未动。
4. `docs/阶段沿革.md` 新节：落笔前实时重数尾部（最新编号节 = 二百五十五「桌面 GPU 加速深化研究」、其后为「九十五轮」，与 probe 基准一致无相撞），新节二百五十六「VNC 第五波『彻底完善』——TLS 客户端补全/https 网页桌面/P2 三修/Linux 分发链实机化」追加文末，五段式（刀面/宿主/客机/实测/未落）；`.gitignore` 六项来源与影响（probe 裁决 + 未跟踪 6120→17 条）入刀面段；实测段引 v5 证据与 Linux 产物（ELF/AppDir/tarball/AppImage/容器冒烟）。
5. Agent Note `notes/implemented/2026-11-13-vnc-complete-wave5.md` 落稿：写前 `rg` 搜 `.agents/notes/`（含 rejected/）零撞题；Problem/Decision/Alternatives/Consequences/Related 五节——Alternatives 真实记录 TLS 库四选一（rustls vs openssl 子进程 vs python 辅助 vs 维持 not-supported）、浏览器路径三选一（直连 vs 全代理 vs 终止+字节管道）、连接预算 30s 与孤儿阈值 1h 取值、rustls 0.23 四个实现陷阱（Cursor EOF latch×2/WouldBlock/UnexpectedEof）；与前四波笔记互链（旧篇属冻结面不改，单向链 + Related 注明接棒关系）。
6. 计划收口：`Status: done`、`Updated: 2026-11-14`、步骤总览十项全部 `[x]`、未完成事项改写为真实遗留四条（索引行/打包脚本许可随包/window.rs 直参/边界项）。
7. `justfile` 旧配方 `berry-vnc-smoke` 注释尾句更新：「TLS（wss/rc=2）…留手工验收」过时表述改为「本配方钉明文零回归，TLS/VeNCrypt 面由 berry-vnc-tls-smoke 覆盖」；配方体未动。
8. `node .agents/tools/check-docs.mjs`（仓库根，报告模式）：检查 213 篇、告警 12 条——与 docs 阶段动手前基线（212 篇/12 条，新增 1 篇 = 本刀 Agent Note）逐条相同，本刀零新增；12 条全部位于冻结文件 `docs/调度彻底计划-v1.0.md`（相对链接指向被并行会话改名为 `docs/调度器设计-v1.2.md` 的 `调度器设计-v1.1.md`），归属并行会话中间态，不在本刀允许面，不修仅记录。
