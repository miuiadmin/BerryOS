# berry — BerryOS 开发者设备壳（host tool）

一个宿主二进制，三种角色：

- **CLI client**：`doctor / run / screenshot / input / ...` —— 可编程、可 CI/AI；
- **GUI client**：双击/无参数 —— 手机比例窗口 + 快捷栏，调同一套控制面；
- **session host**：同一二进制持有 QEMU/QMP/帧源，CLI/GUI attach（C1 收口）。

CLI 是主接口；窗口按钮只是命令的按钮化，任何 UI 动作都应有等价 CLI。

设计依据：`docs/Berry设备平台-CLI-first设计-v1.8.md`（平台/CLI/控制面/快捷栏单一事实源）+
`docs/berry设备壳设计草案-v1.0.md`（显示/帧面/QEMU 子系统）+ ADR #62 追记。**QEMU 只是引擎子进程**：
帧面走 QEMU `memory-backend-file,share=on` 的客机 RAM 镜（`RamSource`），输入走
VNC 多客户端（`VncInput`），生命周期/复位走 QMP。窗口、截图、缩放都在宿主侧。

## 构建

```bash
just berry-build          # host 独立 workspace，显式 --target <host>
just berry doctor         # QEMU / 固件 / 镜像 / accel / fw_cfg 键注入报告（--json 含 fwcfg 数组）
```

fw_cfg 具名键（`opt/berry/*`）的单源键表在 `tools/fwcfg-keys.toml`（20 键：形态/注入方/消费方/极性），改键先改表再动代码，`bash tools/fwcfg-check.sh` 对拍两侧零漂移。

`tools/berry` 是独立 workspace；根 `.cargo/config.toml` 把 `build.target` 锁成
`aarch64-unknown-none`，所以必须走 `just berry-build`（或显式 `--target`）。宿主依赖
（winit/softbuffer/clap/serde_json）只在 `tools/berry/Cargo.lock`，不进客机/ABI。

## 双击包

```bash
tools/berry/packaging/macos/build.sh      # → out/BerryOS.app（自包含镜像/固件，ad-hoc 签名）
BERRY_IMAGE=out/berryos-desktop.img BERRY_BUNDLE_ID=dev.berryos.shell.desktop \
  tools/berry/packaging/macos/build.sh out/BerryOS-Desktop.app
```

macOS `.app` 会从 `Contents/Resources` 找 `berryos.img`/`AAVMF_CODE.fd`；web 资产（vendored noVNC，`berry vnc web` 的页面源）同在 `Contents/Resources/web/`——Linux AppDir 落 `usr/share/berry/web/`（tarball/AppImage 随 AppDir 整体打包继承），Windows 目录包落 `<包根>/web/`（与 berry.exe 平铺）。三种形态里 `berry vnc web` 都零仓库依赖：双击态以包内二进制 `device start --profile desktop --vnc on --vnc-ws on`（desktop 档配 `BERRY_DATA_DISK` 数据盘）后 `berry vnc web`，资产按 `web_roots()` 顺序解析（exe 同目录 `web`、exe 父目录 `Resources/web` 与 `share/berry/web`、用户数据目录、cwd 逐级向上的 `tools/berry/web`——最后一路是仓库开发态）。QEMU 仍是外部依赖（GPLv2，进程边界，不打包）；Windows 交叉产物见下，真机 Windows 的 GUI 子系统 + `AttachConsole` 验证仍待补。

Linux/Windows 分发链（wave5 实机化，产物落 `out/`）：Linux 侧 `tools/berry/packaging/linux/` 四脚本依序产 musl 静态 ELF（`build-linux-binary.sh`，aarch64，ring 的 C 面经 Apple clang + `-ffreestanding` + 自足 `stddef.h` shim 交叉编译）→ `BerryOS.AppDir-linux`（`build-appdir.sh`，含 web 资产与 THIRD-PARTY-NOTICES.txt）→ 结构检查（`check-appdir.sh`，0 fail 门）→ `BerryOS-linux-aarch64.tar.gz` + sha256（`build-tarball.sh`）；用法与变量详见 [packaging/linux/README.md](packaging/linux/README.md)。docker（OrbStack 唤醒）可用时追加 `BerryOS-aarch64.AppImage`（`build-appimage-docker.sh`）与 Windows 目录包 `BerryOS-win64`/zip（`windows/build-cross-docker.sh` + `assemble.sh`，双 exe 为 PE32+ console/GUI x86-64）；容器冒烟（`linux/smoke-appimage.sh` AppImage FUSE 挂载跑 doctor、`linux/smoke-docker.sh` alpine 容器内真 ELF 起机双档截图）实测通过。AppDir 内 exe 在 `usr/bin`，`web_roots()` 第三落点解析到 `usr/share/berry/web` 与资产拷贝位一致——打包态 `vnc web` 零仓库依赖同明文档。

许可随包：`NOTICE.md`（来源/版本/sha256/许可分层）与上游许可文件 `novnc/LICENSE.txt`、`novnc/AUTHORS`、`novnc/vendor/pako/LICENSE` 随 novnc 全目录完整进入每种分发形态——macOS `Contents/Resources/web/`、Linux `usr/share/berry/web/`、Windows `<包根>/web/`（包内路径亦见各打包脚本拷贝步骤注释）。berry 二进制自 wave5 起静态链 rustls（Apache-2.0 OR ISC OR MIT；host 工具侧唯一新 crypto 依赖，不进客机/ABI），其传递依赖含 subtle（BSD-3-Clause，rustls 0.23 硬依赖，无剔除路径——三句版宽松许可，本仓裁定接受并登记）；静态链组件的许可通知单源在 `tools/berry/packaging/THIRD-PARTY-NOTICES.txt`（rustls 系 6 项 + subtle 点名 + noVNC 指向 `web/NOTICE.md` 的指引），wave6 起三平台随包——Linux AppDir `usr/share/berry/THIRD-PARTY-NOTICES.txt`、macOS `.app` `Contents/Resources/THIRD-PARTY-NOTICES.txt`、Windows 包根（berry.exe 旁），tarball/zip 整目录打包自动随行，改内容只改单源（详见 [packaging/README.md](packaging/README.md)）。

## 命令

```bash
just berry doctor [--json]
just berry run --kernel <elf> --marker "BerryOS boot" --timeout 30 --follow
just berry shot --image out/berryos.img --out out/shot.png --timeout 120
just berry shot --image out/berryos.img --profile phone --out out/phone.png
just berry screenshot --out out/shot.png --settle-ms 500   # 抓正在跑的 shell，不重启
just shell                                  # GUI 优先（out/berryos.img）/ console 回退
just desktop                                # GUI 桌面：out/berryos-portrait.img + ~/.berry/data/desktop.img（一次造跨启动复用） + tablet + 1080×1920@60
just berry shell --profile phone --timeout 30
just berry device list
just berry device status --run <id>
just berry logs --run <id> --tail 50
just berry power reboot --run <id>
just berry power off --run <id>
just berry input key ret
just berry input tap 400 300
just berry input swipe 400 500 400 200 --ms 300
just berry input text hello
just berry tap 240 500
just berry key ret
just berry text hello
just berry device start --profile desktop --vnc on        # 对外 VNC（127.0.0.1:自动端口）
just berry device start --vnc 0.0.0.0:15944               # 非环回：自动生成随机口令（run 目录 vnc.pass，0600）
just berry device start --vnc on --vnc-ws on              # noVNC websocket 端点（自动挑 ws 端口）
just berry device start --vnc on --vnc-ws on --vnc-tls <dir>  # TLS/VeNCrypt 加密端点（dir 用 vnc certs 生成）
just berry vnc status [--json]                            # listen/uri/bind/tls/auth/clients
just berry vnc check [--ws] [--ca <dir>] [--insecure]     # 完整 RFB 自检 + 取一帧；TLS 端点过 VeNCrypt（rustls，缺省从 run 推证书目录）；--ws 自检浏览器（ws/wss）通路
just berry vnc passwd [--set <pw>|--generate|--expire <s|never>]  # 轮换/到期口令（需启动即认证）
just berry vnc open                                       # macOS 屏幕共享直连；其他平台打印 URI
just berry vnc web [--run <id>] [--port N] [--max-conns N] [--tls <dir>] [--open]   # 浏览器桌面：默认 BerryOS 自有壳页（状态栏/工具栏/自动重连/移动端；前台阻塞；并发上限默认 16；TLS run 自动 https+wss 代理；--open 在 macOS 自动开浏览器——URL 剥口令 fragment）
just berry vnc web --lan [--host <addr>] [--qr|--no-qr] [--ca <dir>|--insecure] [--port N]   # LAN 单口远程桌面：页面与 /ws ws 代理同口绑 0.0.0.0，手机同 Wi-Fi 扫 QR 直开桌面（明文/TLS run 均可且必须有口令；TLS run 的上游腿为 rustls wss，--ca 覆盖信任源/--insecure 跳过校验带警告；--host 覆盖自动解析；--qr/--no-qr/--host/--ca/--insecure 无 --lan 时 rc=2）
just berry vnc certs <dir>                                # 生成自签 x509 证书集（openssl 子进程，出指纹）
just berry events --filter vnc.                           # VNC 审计事件（connected/initialized/disconnected/passwd 族）
just berry-vnc-smoke                         # 明文 VNC 面 CLI 冒烟（对外端点+ws+口令三态）
just berry-vnc-tls-smoke                     # TLS/VeNCrypt 面 CLI 冒烟（certs/status/check 四路+互斥）
just berry-smoke                            # doctor + kernel marker + EFI shot
```

`berry shot` 起一台**临时**设备：QEMU（`-display none -vnc`）→ 等串口标记 → 从 VNC 抓一帧 →
写 PNG → 停设备。`berry screenshot` **不启动**新设备，直接 attach 正在跑的 shell（VNC 多客户端）
抓一帧；这是 CLI/AI 对「当前界面」截图的主路径。`berry shell` 用 `-display none` + `memory-backend-file` 起同一套设备：
串口 `FB ok 480x1000 … fb=0x…` 给出几何/PA，`RamSource` 直接读 `ram.bin`，窗口按
19:9 等比缩放显示；VNC 只作输入通道（不请求帧），`berry input` 可与 shell 并存。

对外 VNC 桌面（2026-11-13 落）：任何标准 VNC 客户端（macOS 屏幕共享、TigerVNC、RealVNC、noVNC）可直连 BerryOS 桌面看画面、发输入。`--vnc <spec>` 布到 `device start`、`host`（隐藏）、`shell` 三入口（env `BERRY_VNC` 兜底），取值 `off`（默认，argv 与现状等价）| `on`（= `127.0.0.1:自动端口`）| `<port>` | `<host>:<port>`（端口 5900..65535，`0`/`auto` 自动挑）；对外端点是 QEMU **第二个** `-vnc` TCP 服务，与内部 `vnc.sock` 并存，内部面（RamSource/VncInput/QMP broker/control.sock）零改动。`--vnc-ws <on|off|port>` 开 websocket（noVNC 通路，QEMU 侧显式 `websocket=<port>`）；口令策略：环回绑定默认无口令（`--vnc-pass <pw>` 可显式开认证），非环回必须口令——默认自动生成随机 8 字符写 run 目录 `vnc.pass`（0600），无「非环回免口令」逃生门；口令运行期经 QMP `set_password` 注入，不进日志、不进错误信息，也不进 host 子进程 argv（`device start`→host 交接与重启链继承都走一次性 0600 `--vnc-pass-file`，子进程读后即删）；注意 `berry shell --vnc-pass` / `berry host --vnc-pass` 的口令仍留在该长驻进程自身 argv（`ps` 整场会话可读）。重启链（`device restart`、`--restart-on-crash`、`display set --restart`）保持同端口同口令；`vnc check` 缺省读 `vnc.pass`；`vnc status`/`device status` 顺带显示端点；noVNC 用 `vnc status` 报的 `ws_uri` 接入（TLS 档的 `wss://` 浏览器直连不可用——QEMU 恒发客户端证书请求，请走 `berry vnc web` 的 https 页 + 代理）。

VNC 成熟化第二波（2026-11-13 落）：浏览器零安装直连与通道加密。`berry vnc web [--run <id>] [--port N]` 在 127.0.0.1 起本地静态 HTTP 服务（std 手写 HTTP/1.1，零新依赖），served 页面是 vendored noVNC 1.7.0 最小子集（资产在 `tools/berry/web/`，来源/版本/sha256/许可分层见 [web/NOTICE.md](web/NOTICE.md)）；打印的 URL 以 `path=<ws_uri>` 参数让浏览器**直连 QEMU 的 ws 端口**——berry 只发页面，不代理任何 RFB 字节；run 有 `vnc.pass` 时 URL 以 `#password=` fragment 带口令（fragment 不发往任何服务器），无口令则页面自弹口令框。对外 VNC 未暴露或无 ws 端点时 `vnc web` 报错并提示 `--vnc on --vnc-ws on`。

TLS/VeNCrypt：`--vnc-tls <dir>`（`device start`/`host`/`shell` 三入口，env `BERRY_VNC_TLS` 兜底，要求 `--vnc` 非 off）给对外端点加 x509 加密（QEMU `-object tls-creds-x509` + `tls-creds=`，VeNCrypt STARTTLS 型）；与口令组合为 `x509-vnc`（TLS 内再走 DES 挑战），`vnc status` 显示 `auth=vencrypt` 与 `tls: on (<dir>, vencrypt x509-vnc)`，ws 端口随之变 `wss://`。`berry vnc certs <dir>` 用 `/usr/bin/openssl` 子进程生成自签证书集（ca-cert/ca-key/server-cert/server-key，私钥 0600，CN=localhost + SAN `IP:127.0.0.1,DNS:localhost`，输出 SHA256 指纹；目录已有证书时拒覆写）。
`vnc check` 的 TLS 路径（wave5 落，内置 rustls 客户端）：明文 RFB → security type 19 → VeNCrypt 0.2 版本协商 → 子类型按偏好选 X509Vnc（261）优先、X509None（260）兜底 → TLS 握手 → TLS 内继续既有认证（261 走 DES 挑战、260 无内层认证）。证书目录解析：缺省从 run meta 的 `--vnc-tls` 目录推得，`--ca <dir>` 显式覆盖，二者与 `--insecure` 互斥（同给 rc=2）；对明文端点 `--ca`/`--insecure` 静默忽略。校验默认严格——按证书目录 `ca-cert.pem` 验链并核对 SAN；`--insecure` 是显式逃生门，执行前 stderr 打一次性警告（连接对中间人开放）。失败形态：目录缺 `ca-cert.pem` rc=2 并提示 `berry vnc certs`；CA 不对 rc=7 `invalid peer certificate: UnknownIssuer`（hint 指向 `--ca`/`--insecure`）；TLS 内口令错 rc=7 带 passwd hint；成功输出带 `tls x509-vnc` 尾注（JSON 加 `"tls":{"enabled":true,"subtype":…}`）。GUI 客户端面不变：RealVNC Viewer 与 macOS 屏幕共享无 VeNCrypt 能力，TLS 端点人工验证用 TigerVNC，CLI 自检用 `vnc check`。

VNC 第三波·分发与打包收尾（2026-11-13 落）：`vnc web --max-conns N` 设页面服务并发连接上限（默认 16，值域 1..=1024，非法值退出码 2；超限连接收 `HTTP/1.1 503` + 一条 stderr 警告，服务不崩，连接释放后名额回收；绑定保持仅 127.0.0.1）；三平台打包脚本把 web 资产连同许可文件拷进包内资源位（见「双击包」节），双击态 `vnc web` 零仓库依赖。

VNC 第五波·彻底完善（2026-11-14 落）：`vnc web` 对 TLS run 自动起 https 页面（rustls 服务端，证书目录同 `--vnc-tls` 的 server-cert/server-key；`--tls <dir>` 可显式指定）+ 本地 wss 终止代理（自动挑口，JSON 带 `proxy_port`）——浏览器 wss 到 berry，berry 以 rustls 终止两侧 TLS 后纯字节管道连 QEMU ws 口。这是 berry 代理 RFB 字节的唯一场景（明文 run 仍零代理直连 QEMU ws）；动机是 Chrome 对 QEMU gnutls 恒发客户端证书请求的 wss 直连会静默弃连，berry 侧终止是实测唯一可行浏览器通路。打印 URL 形如 `https://127.0.0.1:<page>/?autoconnect=true&host=127.0.0.1&port=<proxy>&encrypt=true&path=&resize=off#password=…`（wave9 起打根路径——默认页已是自有壳页，形态与现行样例同构）——`path=` 参数在场且值为空（noVNC 1.7.0 参数缺席会回落拼 `websockify` 路径而连不上，QEMU ws 端点也只认根路径）。浏览器打开自签 https 页会告警（Chrome「您的连接不是私密连接」/Safari「此网站证书无效」）——这是自签证书的预期行为，点「高级 → 继续前往」即可；自动化场景给 Chrome headless 加 `--ignore-certificate-errors`。安全卫生三修：页面服务每连接 30s 总预算（慢滴客户端超时即断、槽位回收；wss 代理是数据面长连接不受此限）；`vncpass-*` 一次性口令文件写前清扫同目录 mtime 超 1h 的孤儿（扫不动不失败）；`vnc passwd` 轮换后的 expire 复位失败不再静默——落 `vnc.passwd_warn` 审计事件（错误摘要不含口令材料）。

VNC 第六波·合规随包与通路补全（2026-11-14 落）：`vnc check --ws` 自检浏览器通路——对 run 的 websocket 端点（`listen.ws`，无则 rc=3 提示 `--vnc-ws on`）完成 std 手写的 RFC 6455 Upgrade 握手（含 `Sec-WebSocket-Accept` SHA-1 复核）与 masked 帧编解码（client 帧必 mask，QEMU 对未 mask 帧以 close 1002 硬拒；server 帧按字节流读，8192B 分块非消息边界），复用 TCP 路径同一 RFB 状态机（`handshake_auth`）走到 ServerInit 报尺寸与 auth——与 TCP check 的 auth+one frame 深度不同：noVNC 报「已连接」同为 ServerInit 深度，且免去大流量拉帧；TLS run 的 ws 口自动按 wss 拨（rustls 包住后帧层同一），RFB 内为明文 `003.008` 无 VeNCrypt 层，证书目录解析/`--ca`/`--insecure`/互斥语义与 TCP TLS 路径完全一致（共用同一配置函数）。注意判据：`vnc.connected` 事件在 QEMU 侧 TCP accept 即发（裸 TCP 连上零字节再关也产生 connected+disconnected 对），不能当 upgrade/RFB 完成证据——CLI 自检判据是 RFB 语义（ServerInit/尺寸）。`vnc web` 明文模式的文本输出新增一行 hint 指向该自检。`--tls` 施于明文 run 的 https 页分支同波修复：原 `path=ws://…` 形态在 https 页失败，根因是 vendored noVNC 1.7.0 `app/ui.js` 对 `path=` 形态无条件强改 scheme 为 wss（绝对 `ws://` URL 也被改，Chrome 随即对明文 ws 口做 TLS 握手、QEMU 对 ClientHello 零应答即死）——非混合内容拦截；改用 host/port/`encrypt=false&path=`（在场空值）形态后 Chrome loopback 混合内容豁免实测覆盖 noVNC 拨号，DOM「已连接（未加密）到QEMU」3/3 命中。

VNC 第九波·网页桌面产品化（2026-11-14 落）：`vnc web` 默认页从裸 vendored noVNC 换成本仓自有 Apache-2.0 壳页（`web/index.html` + `berry.js` + `berry.css` + 自产 `berry-logo.svg`/`berry-bell.wav`，仅 import vendored `novnc/core/` 三模块、零 CDN/零新依赖，声明见 [web/NOTICE.md](web/NOTICE.md) 自有资产节）——顶栏状态（品牌/会话名/连接态含加密标注/分辨率）+ 工具栏五键（全屏/缩放三态 scale-100%-off/只读/软键盘开关/断开-重连）+ 断线自动重连（指数退避 1s 起、帽 30s，状态栏秒级倒计时；口令错终止转错误态并弹重输；8s 看门狗拆死挂连接）+ 屏幕唤醒锁（connected 态申请、可见性恢复重申请，缺 API 静默降级）+ RFB bell 事件自产铃音 + 深色主题与移动端自适应（≤700px 紧凑档，matchMedia 驱动 body 类）。URL 三形态（明文 `path=<ws uri>`、`--tls` 明文 run `host/port/encrypt=false`、TLS run `host/port/encrypt=true`）由新页自消费；`#password=` fragment 读后即抹（地址栏不留、仅内存供重连与静默重试，不进 localStorage/日志）；`resize`/`view_only=1` 参数给初值；无连接参数时空闲态提示用 `berry vnc web` 的 URL 打开。原 noVNC 页保留为逃生门：直访 `/novnc/vnc.html?…`（同参数、vendored 零改动），裸 `/vnc.html` 旧形态退役为 404。`--open`：macOS 打印 URL 后自动 `open`（URL 剥口令 fragment——口令不进一次性 argv，有口令时页面凭据弹窗兜底），非 macOS 或失败打一行指引不视为错；JSON 输出增 `"opened"` 键。QR 码砍项：服务端与 QEMU ws 均 loopback 绑定，手机扫码打开的是手机自己的 loopback——功能死路；重启条件是 LAN 绑定模式立项（届时 QR 内容须剥 fragment）。

连接审计与口令生命周期：宿主 QMP 事件泵把 VNC_CONNECTED/VNC_INITIALIZED/VNC_DISCONNECTED 落 `events.jsonl`（t 值 `vnc.connected`/`vnc.initialized`/`vnc.disconnected`，含 client 的 host/service/websocket 与 server 的 auth），口令动作落 `t=action` 族（`vnc.passwd`/失败告警 `vnc.passwd_warn`），`berry events --filter vnc.` 直接过滤；`vnc status` 的 clients 维持 QMP 实时值。`vnc passwd --expire <seconds|never>` 设口令到期（到期后新连接认证拒绝；`--set`/`--generate` 轮换自动把到期复位为 never，复位失败落 warn 事件）；口令轮换**不断开**已连客户端（QEMU 11 对 vnc 强制 keep，要踢客户端只能停会话/停端点）。健壮性：`--vnc 0.0.0.0`/`::` 绑定时 `vnc status`/`open`/`check` 用可拨地址（127.0.0.1/`[::1]`）并加 `bind` 字段回显真实绑定；自动端口撞竞态时换新空闲口重试一次，固定端口被占响亮报错（错误里带 qemu.log 尾部原因，不重试）。

VNC 第十波·LAN 远程桌面（2026-11-15 落）：`vnc web --lan` 把网页桌面扩成 LAN 直开形态——berry 作唯一 LAN 入口，页面与 `/ws` ws 代理**同口**绑 `0.0.0.0`；`/ws` 升级请求只把请求行 target 改写为 `/`（QEMU ws 端点只认根路径）后按明文字节管道转发到 QEMU 的环回 ws 口（QEMU 不绑 LAN，VNC DES 认证端到端透传，代理连接豁免 30s 连接预算）。配对流程：终端打印剥 `#password=` fragment 的 URL 二维码（qrcodegen 1.8，MIT，零传递依赖），手机连同一 Wi-Fi 扫码打开页面，口令从终端 URL 行读取后在页面凭据弹窗手动输入（口令不进任何可扫实体/argv/日志）。准入：`--tls` 组合 rc=2 拒绝（互斥），**无口令 run 一律拒绝**（`--vnc-pass` 重启），启动打一次性 DES 弱防线提醒；LAN 地址解析 = `--host` 显式 > 平台接口 IPv4（macOS `ipconfig` 实测、Linux/Windows 尽力解析未实机验证）> 127.0.0.1 回退警告。同波壳页长出**底栏设备面板**（拇指位）：返回/多任务/主页/旋转四导航键——按客机 launcher 五键导航条的真实几何（紧凑 44px/大屏 160px 两档）经合成指针事件**代点客机自己的导航条**（真客机语义，实测坐标命中；`#password=` fragment/sessionStorage 双通道口令，刷新免重输）；截图（canvas 当前帧 PNG 直接存进手机）+ 重启/关机（`POST /api/power/<reboot|off>`，berry 转发控制面 `power` op；run 有口令时必须 `X-Berry-Pass` 匹配——LAN 下与 VNC 认证等强；Host 头须为 IP/localhost 形态防 DNS 重绑定；关机 3s 双击确认）+ 全屏键。环回模式（无 `--lan`）输出与行为逐字节照旧。

GUI 家族 noktest 注入（2026-11-15 落，六波 owner 裁决提前实现）：`berry shell`/`device start`/`host` 的 QEMU 参数默认注入 `opt/berry/noktest=1`——GUI 启动形态按生产 boot 走（内核跳过 ktest/bench 开发自测，省分钟级开机）；`berry run`/`berry shot` 冒烟路径**不注**（ktest 逐行输出仍是冒烟断言面，语义零漂移）。诊断要真跑内核自测时用 `berry run` 或 `BERRY_QEMU` 直跑。

VNC 第十一波·解冻收尾 + TLS×LAN（2026-11-15 落）：两件。①TLS run 的 `--lan` 形态兑现（wave10 重启条件销账）——页面维持明文 http 0.0.0.0 单口（URL/QR/文本输出与明文 `--lan` 逐字节同构，手机侧零新墙），只有 `/ws` 代理的**上游腿**以严格校验的 rustls 客户端连 QEMU 环回 wss（安全语义与 `vnc check` 一致：证书目录 `--ca <dir>` 显式 > run meta `vnc_tls` 自动推 > rc=2 带 `vnc certs` hint；`--insecure` 逃生门跳过校验并打一次性警告，与 `--ca` 互斥 rc=2；坏 CA 时服务正常起、上游握手失败 stderr 有声、页面转错误态）；JSON `lan` 对象增 `upstream: ws|wss` 单键。LAN 腿暴露面与明文 `--lan` 同构——明文 http 页 + VNC DES 端到端，TLS 只加在 berry→QEMU 环回腿，不宣称「LAN 加密」。②内部机制退役：wave1 冻结期的 LAUNCH/LAUNCH_STICKY 进程级覆盖删除，`--vnc` 五旗标改 `Device::start` 直参（shell→`window::run`→`Device::start`，RES 菜单/ROTATE 同进程重启显式重传）——CLI 用户面零变化（旗标/env/输出全照旧）。附窗口工具栏新增第六键 WEB：点击后台起 `berry vnc web --open` 并自动开浏览器（明文 run 环回 URL、TLS run 自动落 https 双口环回形态；再点幂等拒，换分辨率/旋转后需重点、退出窗口即收）；check-appdir.sh 的 TPN 检查补钉 `qrcodegen 1.8` 行。

真实客户端矩阵（QEMU 11.0.3 + macOS 实测）：

| 客户端 | 接入 | 备注 |
| --- | --- | --- |
| 浏览器 + noVNC | `berry vnc web` 打印的 URL | 零安装；明文/口令/TLS 档均可（口令走 URL fragment 或页面输入框；TLS 档经 berry 的 https 页 + wss 代理） |
| macOS 屏幕共享 | `berry vnc open` 或 `open vnc://host:port` | 持久显示会话不发 ClientInit——审计只见 connected/disconnected 属客户端行为，非故障 |
| RealVNC Viewer | `vncviewer <host>:<port> PasswordFile=…` | headless/脚本场景加 `WarnUnencrypted=0 Encryption=PreferOff`：默认「未加密连接」确认弹窗会阻塞在 VNC_CONNECTED |
| TigerVNC | 直连 | TLS/VeNCrypt 端点的推荐客户端（RealVNC 7.15.1 与屏幕共享无 VeNCrypt 能力） |

Chrome headless 截图（CI/AI 验证网页桌面）：

```bash
berry vnc web --run <id> --port 18700 &    # 前台阻塞服务，后台起或另开终端
# 复制其打印的 URL（形如 http://127.0.0.1:18700/?autoconnect=true&path=ws%3A%2F%2F127.0.0.1%3A54005&resize=scale——默认页是 BerryOS 自有壳页）
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --screenshot=out/vnc-web.png --window-size=480x1000 \
  --virtual-time-budget=8000 "$URL"
```

`--virtual-time-budget` 给页面完成握手与首帧的虚拟时间（数秒量级）；验画面真实性可对照 `berry screenshot`（内部面真值）的调色板。已知：Chrome headless 虚拟时间下 WebSocket 消息泵送是坑——壳页的 `<audio preload="metadata">` 以 pending 媒体请求挂住虚拟时钟让 RFB 握手完成（wave9 实测定型，真浏览器无感）；裸 vendored 页同条件天然可连。自动化截图把 URL 里的 `&resize=scale` 去掉可避免画布缩放不提交（截图空白）的问题。

逃生门与对照：直访 `/novnc/vnc.html?autoconnect=true&path=<ws>&resize=scale` 用原生 vendored noVNC 页（与壳页同参数语义、零改动保留）；裸 `/vnc.html` 旧 URL 已退役（404）。

TLS run 同一命令链直接可用：`berry vnc web --run <id>` 打印的是 https URL（页面口 + `proxy_port` 代理口），Chrome headless 加 `--ignore-certificate-errors`（自签证书告警会阻塞 headless 渲染）即可 `--screenshot`；TLS URL 已内建 `resize=off`，无需再去 resize 参数（实测：DOM 连接态「已连接（已加密）」，截图画布像素与 `berry screenshot` 地面真值逐项吻合，产物 `out/vnc-w5-tls-browser.png`；壳页对应态为状态栏「已连接（已加密）」）。

## CLI-first 控制面（现状）

目标：会话（session host）持有一台设备，CLI/GUI 都 attach 到同一 `control.sock`；窗口按钮
与 CLI verb 一一对应，AI/CI 不依赖窗口。

| 动作 | CLI（当前/规划） | 窗口快捷栏 |
| --- | --- | --- |
| 截图 | **`berry screenshot`（已落）** / `shot`（临时设备） | 截图按钮 / `Cmd+S` |
| 输入 | `berry input key/tap/swipe/text`（VNC attach，已落） | 客机触摸/键盘 |
| 重启/关机 | **`berry power reboot/off`（C1a）** / `berry shortcut`（C2b） | BOOT/OFF 按钮（C2a） |
| 分辨率 | **`berry display set <profile> --restart`（C2c）** | **RES 菜单（C2d host / C2e attach）** |
| 外部 VNC | **`berry vnc status/passwd/check/open/web/certs`（已落）**；起档 `--vnc`/`--vnc-ws`/`--vnc-pass`/`--vnc-tls` | —（标准 VNC 客户端/浏览器直连） |
| 全屏 | — | `F` / `FULL` 按钮 |
| 审计 | `berry events/logs`（C1/C2） | 同一事件流，`source=ui/cli` |

C1a/C1b/C1c 已落：`berry shell` 进程内或 headless `berry device start` 都会起
`control.sock` + QMP broker；CLI `device start/list/status/restart/stop`、`power off/reboot`、
`logs`、`screenshot`（优先走 host）已可用，CLI 不直连 QMP；`berry shell --attach <id>` 可给
headless host 开窗，关窗只 detach。C1d/C1e/C1f 已落（state/cleanup/TCP）；
C1g 的 host 自动重启已落：`device start --restart-on-crash`（QEMU 异常退出后同 run
原地重启；attach 窗口在 control 恢复后自动重连 VNC）；还剩 Windows 实机验证/打包。

## 环境变量

| 变量 | 作用 |
| --- | --- |
| `BERRY_HOME` | run dir 根（默认 `~/.berry`，Windows `%LOCALAPPDATA%\BerryOS`） |
| `BERRY_QEMU` | 指定 `qemu-system-aarch64` |
| `BERRY_KERNEL` / `BERRY_IMAGE` | 默认内核/镜像 |
| `BERRY_DATA_DISK` | GUI 桌面数据盘（FAT12 + `BERRYOSVBLK!`；launcher 需它拿 fs/app/input gift；`just desktop` 默认设为 `~/.berry/data/desktop.img` 复用制） |
| `BERRY_VARS_PERSIST` | vars.fd 可选持久位（设了且文件在即复用，不在则造 blank 到该位；未设 = one-per-run blank——确定性干净启动默认，勿放 run 目录内） |
| `BERRY_VNC` | 对外 VNC 暴露 spec 兜底（`off`/`on`/`<port>`/`<host>:<port>`；显式 `--vnc` 优先，先例 `BERRY_DATA_DISK`） |
| `BERRY_VNC_TLS` | `--vnc-tls` 证书目录兜底（spec 已给时仍可补 tls；要求 `--vnc` 非 off） |
| `AAVMF_CODE` | AAVMF code pflash（默认 brew / 发行版路径） |

## 窗口快捷键

| 键 | 动作 |
| --- | --- |
| `Cmd/Ctrl+S` | 截图 `berry-shot-<ts>.png` |
| `Cmd/Ctrl+R` | 复位设备 |
| `Cmd/Ctrl+Q` | 退出 |
| `F` | 全屏切换 |
| `I` | 显示/隐藏传输信息 |
| 鼠标/触摸 | 点击、拖动 → 客机触摸 |
| 键盘 | 映射到 X11 keysym |

## Profile

| id | 面板 | 客机渲染 | 备注 |
| --- | --- | --- | --- |
| `default` | 800×600 | 800×600 | P1 兼容档 / CI |
| `desktop` | 540×960 | 1080×1920 | GUI 9:16 竖屏桌面档（`just desktop`）；fb 8.3MB 走同一 defer+buddy 路 |
| `phone` | 1080×2250 | 480×1000 | 默认；恰好用满 800×600 GOP 缓冲 |
| `phone16` | 1080×1920 | 480×854 | 16:9 |
| `phone20` | 1080×2400 | 540×1200 | 已验证：defer+buddy 连续 fb + ramfb 重编程 |
| `tablet` | 1600×2560 | 800×1280 | 已验证：defer+buddy 连续 fb + ramfb 重编程 |

> 表内「已验证」是**验证登记**，不是分配能力上限——所有超 GOP 缓冲（1.92MB）的档都走同一条 defer+buddy 路（按需取 2^n 连续页，上限 `MAX_ORDER=13` = 32MiB）。2026-11-15 曾把「desktop 档未标已验证」误读成「4MB 分配帽」，实际是当日 a11y 容量回归（`A11Y_MAX` 34>32，大屏档建 dock 才触发）——教训与证据见 [.agents/research/2026-11-15-1080p恒黑根因会审.md](../../.agents/research/2026-11-15-1080p恒黑根因会审.md)。

`phone*`/`tablet` 默认内存 **256MiB**（`MMAP ok` + ROOTFS/GUI）；`default` 128MiB，
`--memory 128` 仍零回归。gui8 540 预览镜像也已换 256 内核。

## 已知限制（P2）

- QEMU 是外部依赖（GPLv2，进程边界）；主仓 Apache-2.0 不打包 QEMU。
- 帧面要求 QEMU `memory-backend-file,share=on`；不支持该后端的环境回退 VNC（P1 路径）。
- `berry input`/`berry screenshot` 走 VNC 多客户端 attach；QMP 由 host broker 独占，
  `power reboot/off`、`display set` 已落（C1a/C2c）；host 自动重启已落（--restart-on-crash）；
只剩 Windows 实机/打包。
- 窗口快捷栏 C2d 已落 6 键：`SHOT`/`RES`/`BOOT`/`OFF`/`FULL`/`WEB`（鼠标/触摸）；
  `OFF` 需 3s 内二次点击；`RES` 打开 profile 菜单，host 窗口内停旧/起新 profile；
  `WEB`（wave11）点击后台起 `berry vnc web --open` 并自动开浏览器——明文 run 落环回 URL、
  TLS run 自动落 https 双口环回形态，服务存活期再点幂等拒，换分辨率/旋转后需重点
  （旧 run 端点已死），窗口退出即收；真点按在无显示环境未验（单测 + 代码走查降级登记）。
- C2b 已落：`berry shortcut screenshot|reboot|power`、`berry display list/set`（记录
  next-profile）、`berry events`（JSONL）。
- C2c 已落 CLI：`berry display set phone16 --restart` 停旧 host、起新 profile host
  （新 run id，实测 480×854）。
- attach 窗口的 `RES` 已自动重附着：`restart_profile` 停旧 host/起新 profile host，
  窗口重连新 run VNC，无需手动 re-attach。按钮 hover 高亮 + tooltip 已落（C2f）；旧 run 清理/新 host 先起（失败不伤旧）= C2g。
`state.json` 已落（C1d）：正常 stop 显示 stopped；host 被杀后 list/status 显示 stale。
C1e：`berry device cleanup [--run|--all]` 对 stale/stopped run 杀 orphan QEMU 并删 run；
running 拒绝/跳过。示例：`berry device cleanup --all`。`power off` 走 ACPI 5s 优雅关机，
`--force` 立即 quit。
C1f：Unix 默认 `control.sock`；非 Unix 用 loopback TCP + `control.port`/`control.token`。
`BERRY_CONTROL_TCP=1` 可在 macOS 强制走 TCP 路径（已实测 status/logs/power）。
C2h：attach RES 新 host 失败保留旧 host；VNC 重连失败按旧 profile 回滚重连。
- 默认 `berry shot` 等 `BERRYBOOT ok`，当前测试镜像此时可能还在固件/测试面；要看 GUI
  请传 `--wait-marker "FBW ok"` 或 launcher 标记。`--profile phone` 时 guifbd 进入
  GUI 模式（console 只走串口，不盖桌面）；默认 800×600 仍保留 console 上屏。
- Windows/Linux 代码路径与打包链已实机化到容器级（wave5）：Linux musl 静态 ELF / AppDir（wave6 通知三条后口径 37 检 0 fail；wave9 起检查器再加自有壳页 `index.html` 检与 web NOTICE 自有资产节检、wave11 再加 TPN `qrcodegen` 钉行——2026-11-15 时点现行检查器已非 37 项，当日重建的 `out/BerryOS.AppDir-linux`/`out/BerryOS.AppDir` 档实跑 40 ok / 0 fail 与 39 ok / 1 warn / 0 fail——AppDir 与 AppImage 内档 TPN 均与单源 4cf7879f… 同一） / tarball / AppImage 全链产物 + alpine 容器真 ELF 起机双档截图冒烟；Windows 交叉产物（PE32+ console/GUI 双 exe + 目录包/zip）容器构建并过 file 检视。上限=容器冒烟——裸机 Linux/Windows 的安装、桌面集成（.desktop 注册）与真机 Windows GUI 子系统 + `AttachConsole` 未验。
- 多会话同机共享 `~/.berry`：不带 `--run` 的 device stop/cleanup 命中「最新 running」run——并行会话可能顺带停掉彼此的设备；多会话请显式 `--run <id>`，或以 `BERRY_HOME` 隔离各会话的 run 树。
- P1 用 QEMU `snapshot=on` 临时覆盖层，基础镜像只读；跨 boot 持久化（GUI-8 布局）留后续。
- PNG 用 `miniz_oxide` deflate（zlib）；480×1000 干净桌面约 3.8KB，含图标/照片内容仍远小于 stored。
- 对外 VNC：客户端发起的 SetDesktopSize 桌面调整在 ramfb 后端下不被支持——分辨率变更走 `berry display` profile 重启；客机发起的几何变更会经 DesktopSize 广播给 VNC 客户端。
- VNC 经典 DES 口令强度有限（有效口令 8 字节、可离线穷举），仅适合开发/内网，不是 TLS 级安全；需要通道加密用 `--vnc-tls`（VeNCrypt x509，可与口令组合）；非环回绑定强制口令（自动生成或 `--vnc-pass`），启动时打一次性安全提醒。
- 非 Unix 平台（Windows 打包脚本已发、实机未验）自动生成的非环回口令随机源弱：无 `/dev/urandom` 时回落到时钟^pid^计数器种子的 xorshift，材料可预测——Windows 会话建议显式 `--vnc-pass`，代码臂接系统 CSPRNG（BCryptGenRandom/ProcessPrng）留后续刀。
- 剪贴板不可用属预期：客机无剪贴板栈，noVNC 剪贴板面板无后端。
- wss/TLS 边界（wave5 后，wave6 补全）：TLS run 的网页桌面走 berry 自持 https 页 + wss 终止代理（rustls）——这是 berry 代理 RFB 字节的唯一场景，动机是 Chrome 对 QEMU gnutls 恒发客户端证书请求的 wss 直连会静默弃连（`ERR_SSL_CLIENT_CERT_NEEDED`，`--ignore-certificate-errors` 不覆盖）；明文 run 仍 http 页 + 浏览器直连 QEMU ws，berry 零代理。`--tls` 施于明文 run 的 https 页分支 wave6 修复（host/port/`encrypt=false` 形态，Chrome loopback 豁免实测 3/3，详见上「VNC 第六波」段）——其浏览器通路与明文默认页同走 QEMU ws 口，可用 `vnc check --ws` 自检。
- 控制面（`control.sock`/`control.token`）与 QMP 不随 VNC 暴露：对外只有 RFB 端点（TCP/ws），控制面仍本机 0600。
- VNC 网页壳页（wave9）的交互验证有 headless 硬边界：全屏真行为（headless 权限拒）、真 OSK 弹出与 IME 组合输入（软键盘按键级转发为上限，组合输入不翻译）、真机触摸手势、<500px 像素级观感、断线重连中间态时序——自动化以 URL 参数初值（`view_only=1`/`resize`）+ body 状态类 + 按钮在位断言替代，点击链路留有头验证。
- LAN 远程桌面（wave10 建/wave11 扩）安全与 TLS 边界：明文 http 页 + 经典 DES 端到端是弱防线（LAN 假定可信，仅同 Wi-Fi 场景用）；TLS run 自 wave11 起 `--lan` 可用（http 页 + 上游 rustls wss 腿，`--ca`/`--insecure` 语义见第十一波段），`--lan --tls` 显式组合仍 rc=2 互斥拒（https 页绑 0.0.0.0 撞手机自签证书墙，架构性死路维持）；无口令 run 一律拒。
- LAN 平台与验证边界（wave10）：Linux/Windows 的 LAN 地址解析分支未实机验证（纯函数面已单测，解析不出回退 127.0.0.1 + 一次性警告，`--host` 显式兜底）；QR 在深底终端的极性反转依赖解码器容错（macOS Vision 实证可读）；真机扫码与触摸手感属人验面（headless 通道等价性已证）；底栏导航键的客机语义以代点客机自身导航条为上限（wave10 晚修订：客机无键盘设备，标准 keysym 注入实测零效果），真客机级导航语义待客机 GUI 波次生长。

## 测试

```bash
cd tools/berry
cargo test --target "$(rustc -vV | sed -n 's/^host: //p')"
just berry-smoke          # 仓库根；需要 out/berryos.img
just berry-vnc-smoke      # 明文 VNC 面 CLI 冒烟
just berry-vnc-tls-smoke  # TLS/VeNCrypt 面 CLI 冒烟（certs→TLS run→status→check 四路+互斥）
```
