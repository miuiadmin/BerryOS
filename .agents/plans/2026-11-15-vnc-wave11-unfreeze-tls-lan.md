# VNC 第十一波「解冻收尾 + TLS×LAN」实施计划（LAUNCH_STICKY 直参化/TLS run 的 --lan/TPN 钉行/工具栏 WEB 键）

Type: plan
Status: done
Updated: 2026-11-15

一句话说明：window.rs 解冻后把 shell/host 入口的 VNC 旗标从进程级 `LAUNCH_STICKY` 隐式传递换成 `Device::start` 直参（sticky 机制整体退役），并兑现 [wave10 笔记](../notes/implemented/2026-11-15-vnc-lan-wave10.md)登记的 TLS×LAN 重启条件——TLS run 上 `vnc web --lan` 不再 rc=2，形态 = http 页 + `/ws` 代理上游以 rustls 客户端连 QEMU 环回 wss——附带 [check-appdir.sh](../../tools/berry/packaging/linux/check-appdir.sh) 补钉 qrcodegen TPN 行与按 cut-line 裁决的窗口工具栏 WEB 键。

## 背景

VNC 十波已收官（owner 以 821d3f152 落账基线：单测 113 绿、构建/三冒烟/check-docs 四门全绿）。window.rs 与 profile.rs 已解冻、工作树干净（全史仅 2 提交：821d3f152 创建 + afd68392a docs，基线后零活跃）——[wave1 笔记](../notes/implemented/2026-11-13-vnc-desktop-suite.md) L16 与 [wave1 计划](2026-11-13-vnc-desktop-suite.md)偏差 7 登记的「shell 入口 `--vnc` 直参布线，解冻后换直参」首次可做。

现状机制（本波要退役的）：shell/host/device start 的 `--vnc` 五旗标经 `vnc_cfg` 的进程级覆盖传递——`set_launch` 写 `LAUNCH`（一次性）与 `LAUNCH_STICKY`（同进程粘性副本，供 RES 菜单/ROTATE 的同进程二次 `Device::start` 继承），`Device::start` 内 `resolve_launch()` 消费并用 `BERRY_VNC`/`BERRY_VNC_TLS` env 补位。设计初衷就是绕开冻结期 `Device::start` 签名改动。

TLS×LAN 的欠账：[wave10 计划](2026-11-15-vnc-lan-wave10.md)契约 6 对 TLS run × `--lan` 硬拒 rc=2（QEMU ws 端点环回 wss-only，明文管道无上游；rustls 客户端上游是新面；手机侧自签证书墙打死 https 形态），并在笔记 L28 登记重启条件「rustls 客户端终止上游 + http 页」——本波兑现。ws.rs 的 `WsStream` 已有 TLS 面（`vnc check --ws` 的 wss 路径用 rustls `StreamOwned` 包 `TcpStream`），但 web 代理是纯字节管道、不经 `WsStream`。

本波前置 probe（2026-11-15，仓库零写入，仅 /tmp 试验与 gitignored 产物）四面定音，关键事实已逐项对代码复核：

- **f1（直参面）**：`Device::start` 全仓恰 4 个调用点——[window.rs](../../tools/berry/src/window.rs) L270（shell Host 首起，display_mode 全参）、L588（RES 菜单 Host 分支重启）、L838（ROTATE 触发重启）与 [commands.rs](../../tools/berry/src/commands.rs) L459（隐藏 `berry host`）；`berry device start` 不直接调用——后台路径 spawn `berry host` 子进程传 argv 旗标（`device_start_run` L877+），前台路径转 `host()` 同进程。`set_launch` 恰 2 处（shell L349/host L452），消费点唯一（[device.rs](../../tools/berry/src/device.rs) L66）；LAUNCH_STICKY 测试钉面 = 零（全仓 rg 无任何测试引用）。
- **f2（TLS×LAN 面）**：上游链双重活体实证——TLS run 起档（/tmp 隔离）上 `berry vnc check --ws` 严格 rustls 校验过 RFB（480x1000 auth vnc），python 首性探针 TLSv1.3 + ws upgrade 101 + RFB 003.008 banner；`--lan` 于 TLS run rc=2 原文「--lan needs a plaintext run」确认；http 静态面对 TLS 无结构性依赖。wss 客户端拼装点 = commands.rs `vnc_check_ws` L1603-1627（rustls `ClientConnection` + `StreamOwned` → 握手环），证书链 `check_tls_client_config`（L1543）与 TCP VeNCrypt 路共用。
- **f3（窗口面）**：工具栏五键机械全泛型（`TOOLBAR_LABELS`/`TOOLBAR_TIPS` 数组、`toolbar_buttons` 循环、`toolbar_click` match 臂 0-4），W/E/B 字模已在；`toolbar_tests` 4 测试循环动态迭代，六键化零测试改写。
- **f4（门与沿革）**：cargo test 基线 113 passed 0 failed；三冒烟全绿（隔离 BERRY_HOME=/tmp 自起自收）；check-docs 0 告警；[THIRD-PARTY-NOTICES.txt](../../tools/berry/packaging/THIRD-PARTY-NOTICES.txt) L13 已有 qrcodegen 1.8 行而 check-appdir.sh 未钉（wave10 audit 观察）；沿革 probe 时点文末实节 = 二百六十二（L6175），并行会话高频续写，落笔以实时重读为准。

design 时点工作树观察（2026-11-15）：commands.rs/web.rs/main.rs/qemu.rs/README/CLI-first 等处于并行会话未提交 M 态（跨线演练线活跃），window.rs/profile.rs 干净——实现期动手前必复探 `git status`，以工作树现状为基线、不得回退他人改动；window.rs 若 M 态重现即停窗口面并降级记录（纪律 2）。

冻结面声明：arch/、kernel/、sched/、user/、research/、docs/调度*与显示*、沿革已提交内容零触碰；main.rs 只动 VNC 布线（`VncWebCmd` 增旗标与 dispatch 传参）；berry-ui 组件库等并行新件零触碰；不 git add/commit，提交归用户。

## 契约

### 1 直参化形态与调用点同步表

**形态裁决**：`Device::start` 追加第 8 个尾参 `vnc: vnc_cfg::Launch`（[vnc_cfg.rs](../../tools/berry/src/vnc_cfg.rs) L554-561，已 derive `Clone/Debug/Default`，零改动）。依据：与参数结构体重构的调用点同步数相同（4 处），追加参数 diff 最小且不动既有 7 位置参语义；不取 `Option<Launch>`——`Launch` 自带 `Default`（全 None = off），语义即「无 VNC」，无需再包一层。

调用点同步表（全景，唯一清单）：

| 调用点 | 现状（隐式经 sticky） | 直参化后 |
|---|---|---|
| commands.rs `shell`（L343-358） | `set_launch(Launch)` 于 `window::run` 前（L349-355） | 构造 `Launch` 局部值，`window::run(args, vnc)` 直传；`validate_flags` 前置快败与 `maybe_security_note` 原位保留 |
| window.rs `run`（L245-292） | 签名只收 `ShellArgs`，L270 首起吃 sticky | 签名增 `vnc: vnc_cfg::Launch`；L270 `Device::start(…, vnc.clone())`；同值存入 App |
| window.rs L588（RES 菜单 Host 分支重启） | 继承靠 `LAUNCH_STICKY` 残留 | `Device::start(…, self.vnc.clone())`——App 新字段（下行） |
| window.rs L838（ROTATE Host 分支重启） | 同上 | 同上 |
| commands.rs `host`（L448-477） | `set_launch`（L452-458）后立即 `Device::start`（L459） | 构造 `Launch` 直传；`berry device start` 前台路径（转 `host()` 同进程）与后台路径（spawn `berry host` 传 argv）经此一点自动覆盖，零额外调用点 |
| device.rs L66 | `vnc_cfg::resolve_launch()` 无参 | `vnc_cfg::resolve(&vnc)`（env 合并逻辑原样收编，见契约 2） |

window.rs 内的结构改动（均属 VNC 面，纪律 2 边界内）：`App`（L419-446）增字段 `vnc: vnc_cfg::Launch`；`App::new`（L450-458，已有 `too_many_arguments` allow）增对应尾参；`window::run` 签名增参。main.rs 直参化部分零改——`ShellCmd`/`HostCmd`/`DeviceCmd` 已带 `vnc_*` 五字段，dispatch 组装照旧。

**行为等价与零回归面**（验收判据）：`--vnc/--vnc-ws/--vnc-pass/--vnc-tls` 经 shell 起档、RES 菜单 Host 分支重启继承、ROTATE 重启继承、`restart_on_crash`（经 Device 持有的 `vnc_ext` 与 `restart_in_place` 重注口令，本就不依赖 sticky）、attach 路径（RES 菜单 Attach 分支 → `restart_profile`/`restart_with` L961+，从 meta.json 继承 `vnc_spec/vnc_ws/vnc_tls` + 一次性 0600 口令文件跨进程传递）——全部行为零变化，由单测 + 三冒烟 + berry-smoke 钉。CLI 用户面无感（无旗标/输出变化）。

### 2 sticky 退役口径

- **整体删除，不留 `#[cfg(test)]` 兼容面**：`static LAUNCH`（L563）、`static LAUNCH_STICKY`（L567）、`set_launch`/`take_launch`/`sticky_launch`（L569-580）五个符号全删——全仓 rg 证零测试引用，唯二生产消费点（shell/host 的 `set_launch`）与唯一消费点（`resolve_launch`）全部随契约 1 改写，保留即死代码。
- `resolve_launch()`（L595-619）改为 `resolve(vnc: Launch)`：显式旗标直入、`BERRY_VNC`/`BERRY_VNC_TLS` env 仅补缺省字段的合并逻辑（L597-610）逐字保留；分区头注释（L546-553「window.rs is frozen」段）改写为直参现实。
- wave1 笔记 L16 与 wave1 计划偏差 7 的文字登记是历史记录，不改写；本波沿革新节与 wave11 笔记点名销账。
- 语义差异登记：旧 sticky 对「同进程多次 `Device::start`」隐式继承；新形态经 App 字段显式传递。唯一行为差异场景 = 有代码绕过 App 直接二次调用 `Device::start` 且指望 sticky——全仓无此调用点（同步表全景已列），故等价。

### 3 TLS×LAN（语法/证书语义/输出形态）

**语法**（`--ca`/`--insecure` 为本波新增旗标，落 `VncWebCmd`）：

```
berry vnc web [--run <id>] [--port N] [--max-conns N]
              [--lan [--host <addr>] [--qr | --no-qr] [--ca <dir> | --insecure]]
              [--open] [--json] [--tls <dir>]
```

- `--ca <dir>`：覆盖上游证书目录（默认 = run meta `vnc_tls` 自动推）。`--insecure`：跳过上游校验，stderr 一次性警告。两者互斥，同给 rc=2（commands.rs L1473 `vnc check` 先例同式）。
- 依赖门：无 `--lan` 给 `--ca`/`--insecure` → rc=2（`--host`/`--qr` 同门先例——环回 TLS 模式的上游与页面同目录自洽，`--ca` 无用武之地，前置拦误导）；明文 run 的 `--lan` 上游不是 TLS，二者静默忽略（F17 先例）。

**形态**：TLS run 上 `--lan` 不再 rc=2——页面 = http（0.0.0.0 单口，与明文 `--lan` 同构），`/ws` 代理上游 = rustls 客户端连 QEMU 环回 wss（wave10 笔记 L28 重启条件原文「rustls 客户端终止上游 + http 页」兑现）。`--lan` × 显式 `--tls` 互斥拒维持不变（https 页绑 0.0.0.0 撞手机自签证书墙，架构性死路，wave10 契约 6 依据仍立）；serve 的页面 TLS × lan 互斥断言（debug_assert）不变——TLS×LAN 的页面仍是 http，TLS 只加在上游腿。

**证书语义**（安全红线）：上游校验默认严格——证书目录解析 = `--ca` 显式 > run meta `vnc_tls` 自动推 > rc=2（certs hint），`--insecure` 换 AcceptAll 带警告，全部复用 `check_tls_client_config`（commands.rs L1543，原函数零改）；环回 dial 走 `tls::server_name(dial)` 的 SAN-IP 验证。错误路径与既有 TLS 面一致：证书缺件 rc=2；坏 CA → 上游握手失败 stderr 有声（F30 先例）+ 断开浏览器侧连接（页面转重连/错误态），不静默死。

**输出/QR/安全矩阵**：与明文 `--lan` 同构——URL 与 QR 逐字节同 `build_page_url_lan`（零改，单测回归钉）；文本面 serving 行/note 行同构（LAN 腿暴露面不变：明文 http 页 + VNC DES 端到端，TLS 只加在 berry→QEMU 环回腿，note 行如实）；JSON `lan` 对象增 `"upstream": "ws" | "wss"`（tls 细节入 JSON）；DES 口令端到端透传不变（wss 内 RFB 为明文 003.008 无 VeNCrypt 层，wave6/probe f2 既证，`vnc check --ws` 同构先例）。`lan_admission`（[lan.rs](../../tools/berry/src/lan.rs) L199-224）矩阵更新：`lan=true 且 listen.tls 且 has_pass` 行由 `Reject` 改 `Allow`（上游形态由命令层按 `listen.tls` 选），`--tls` 互斥拒与无口令拒两行逐字不变。

**实现面**：[web.rs](../../tools/berry/src/web.rs) `LanUpstream`（L636-638）增 `tls: Option<LanTlsUpstream>`（新结构 `LanTlsUpstream { cfg: Arc<rustls::ClientConfig>, name: ServerName<'static> }`）；`handle_conn_plain` 的 `/ws` 分支（L783-790）按上游形态分派 `proxy_ws_plain`（L1058，零改）或新 `proxy_ws_tls`（拨号 + rustls 客户端握手环——`vnc_check_ws` L1611-1621 同式 + 改写头经 TLS 写 + 浏览器明文↔上游 TLS 双线程对泵，OS 超时摘除与双侧 shutdown 拆链语义同 `proxy_ws_plain`）；commands.rs `vnc_web` 的 `--lan` 分支（L1894+）按 `listen.tls` 构造上游（`check_tls_client_config` + `tls::server_name`），环回双分支（tls_dir match 两臂）逐字节不动——结构上 `--lan` 分支先于 tls_dir 计算分岔。

### 4 TPN 钉行

check-appdir.sh TPN 段（L341-358）在 web/NOTICE.md 检查对（L353-357）与外层 `fi`（L358）之间加第四对 grep：`grep -q '^qrcodegen 1.8' "$TPN"` → pass/fail 行，与 rustls 行（L343）同型；依据 = TPN 单源 L13 已有实物行（wave10 落）。实跑核对 = [build-appdir.sh](../../tools/berry/packaging/linux/build-appdir.sh) 重建 AppDir 后 check-appdir.sh 全 pass + 负例（AppDir 内 TPN 删该行 → fail 行出现，验后就地还原）。

### 5 工具栏 WEB 键裁决

- **做，但为尾项**：排序在直参化/TLS×LAN/TPN 三项全绿之后；cut-line = window.rs M 态重现（并行会话再改）或前三项门未全绿或时间超限——命中即砍，降级记录留后续刀（纪律 2/7）。
- **机械**：`TOOLBAR_LABELS`/`TOOLBAR_TIPS`（L27-28）`[&str;5]`→`[&str;6]`（label `WEB`、tip `BROWSER`）；`toolbar_buttons`（L30-41）total 式改 `6*bw + 5*gap`；`toolbar_click`（L684-709）match 增臂 5；W/E/B 字模已在（glyph L61-81），零字模新增。
- **语义**：点击 = spawn 后台子进程 `berry vnc web --open`（macOS `open` 剥 fragment 先例 = vnc_web 的 `--open` 闭包 L1781-1790）——明文 run 落环回 URL，TLS run 由 vnc web 自动落既有 https 双口环回形态；不绑 `--lan`（窗口所在桌面即本机，环回是自然受众；LAN 形态的口令/QR 语义不进窗口按钮）。生命周期：App 持子进程句柄，应用退出时 kill；服务已活时幂等（拒绝二连点重复起）。
- **验证降级**：无显示环境真点按不可验——单测（六键几何/命中）+ 代码走查 + 降级如实记录（RULES 7），不虚报验证层级。

## 步骤

1. [x] 直参化（契约 1/2）：vnc_cfg.rs `resolve` 收参 + 删 sticky 五符号 + 注释更新；device.rs 签名与 L66 一行；commands.rs shell/host 两处；window.rs `run`/`App`/`App::new` 与 L270/L588/L838 三调用点；cargo test 全绿（113 基线全保）；rg 证 sticky 五符号全仓零残。
2. [x] TLS×LAN（契约 3）：web.rs `LanTlsUpstream` + `proxy_ws_tls` + 进程内回环单测；commands.rs `vnc_web` 增 `ca`/`insecure` 参与 TLS 上游分支、互斥/依赖校验；main.rs `VncWebCmd` 两旗标 + dispatch；lan.rs 矩阵改行与单测重钉。
3. [x] TPN 钉行（契约 4）：check-appdir.sh 第四对 grep + build-appdir 实跑正负例。
4. [x] Chrome e2e t1-t4（测试矩阵）：TLS run `--lan` 已连接 + 真桌面截图 + QR 样张；明文 `--lan` 零回归；错误路径 rc 语义。全程隔离 BERRY_HOME、LAN 端口用毕即关、进程杀净。
5. [x] 窗口 WEB 键（契约 5，条件项）：cut-line 复探（window.rs 仍干净 + 前四步全绿）后实施；单测 + 走查 + 降级记录。
6. [x] 全量门四路 + docs 面：g1-g4；README（TLS run 的 LAN 用法/`--ca`/`--insecure`/直参化无感变更说明）、CLI-first 升 v1.8（§8 命令树与参数/§13 LAN 行「架构性拒绝」表述改写/§14 刀序行）、沿革新节（节号落笔时实时重读文末取唯一；销账 wave10 TLS×LAN 重启条件）、Agent Note（Alternatives 至少记直参形态选型/sticky 退役 vs 保留/TLS×LAN 三案/WEB 键裁决四案）；本计划收口 `done`。

## 文件改动清单（符号锚点）

| 文件 | 符号锚点 | 改动 |
|---|---|---|
| [tools/berry/src/vnc_cfg.rs](../../tools/berry/src/vnc_cfg.rs) | `Launch`（L554-561）；`LAUNCH`/`LAUNCH_STICKY`（L563/567）；`set_launch`/`take_launch`/`sticky_launch`（L569-580）；`resolve_launch`（L595-619）；分区头注释（L546-553） | `Launch` 零改；sticky 五符号删除；`resolve_launch` → `resolve(vnc: Launch)`（env 合并段 L597-610 逐字收编）；头注释改直参现实；相关单测随迁 |
| [tools/berry/src/device.rs](../../tools/berry/src/device.rs) | `Device::start`（L49-57）；L66 | 签名增尾参 `vnc: vnc_cfg::Launch`（必要时补 `too_many_arguments` allow）；L66 改 `resolve(&vnc)` |
| [tools/berry/src/commands.rs](../../tools/berry/src/commands.rs) | `shell`（L343-358）；`host`（L448-477）；`vnc_web`（L1713-1912+）；`check_tls_client_config`（L1543-1566）；`vnc_check_ws` wss 拼装（L1603-1627）；`--ca`×`--insecure` 先例（L1472-1474） | shell/host 删 `set_launch` 段改直传（契约 1 表）；`vnc_web` 增 `ca`/`insecure` 参、`--lan` 分支按 `listen.tls` 构造 TLS 上游、互斥/依赖校验、JSON `lan.upstream` 键；`check_tls_client_config` 与 wss 拼装式零改复用 |
| [tools/berry/src/window.rs](../../tools/berry/src/window.rs) | `run`（L245-292）；`App`（L419-446）；`App::new`（L450-458）；调用点 L270/L588/L838；（条件项）`TOOLBAR_LABELS`/`TOOLBAR_TIPS`（L27-28）、`toolbar_buttons`（L30-41）、`toolbar_click`（L662-709）、App 增 web 子进程句柄字段 | `run` 增参；`App` 增字段 `vnc`；`App::new` 增尾参；三调用点传 `vnc.clone()`；WEB 键六键化（契约 5，cut-line 内） |
| [tools/berry/src/web.rs](../../tools/berry/src/web.rs) | `LanUpstream`（L636-638）；`handle_conn_plain` `/ws` 分支（L783-790）；`proxy_ws_plain`（L1058-1114）；`rewrite_ws_request_head`；tests `wss_proxy_end_to_end_relay` 同型位 | `LanUpstream` 增 `tls` 脸（新 `LanTlsUpstream`）；`/ws` 分支按上游形态分派；新 `proxy_ws_tls`（rustls 客户端握手环 + 明文↔TLS 双线程泵）；新上游 wss 回环单测；`proxy_ws_plain` 与头改写零改 |
| [tools/berry/src/lan.rs](../../tools/berry/src/lan.rs) | `lan_admission`（L199-224）及其文档注释（L189-198）；tests 矩阵（U48） | `listen.tls` 臂（L203-208）由 `Reject` 改 `Allow`；注释同步；矩阵单测重钉 |
| [tools/berry/src/main.rs](../../tools/berry/src/main.rs) | `VncWebCmd`（L464-503）；dispatch `vnc_web` 调用（L~770-790） | 增 `--ca`/`--insecure`（help 注依赖与互斥）；dispatch 传参与校验（rc=2 路径）；其余零改 |
| [tools/berry/packaging/linux/check-appdir.sh](../../tools/berry/packaging/linux/check-appdir.sh) | TPN 段 L341-358 | L357 与 L358 之间第四对 grep（`^qrcodegen 1.8`，rustls 行同型） |
| [tools/berry/README.md](../../tools/berry/README.md) | `vnc web` 段 | TLS run 的 LAN 用法（`--ca`/`--insecure`/上游语义/安全姿态）；直参化无感变更说明（内部机制退役，CLI 零变化） |
| [docs/Berry设备平台-CLI-first设计-v1.0.md](../../docs/Berry设备平台-CLI-first设计-v1.9.md) | 头块（现 v1.7）；§8 命令树行（L361）与参数说明；§13 LAN 行（L579）；§14 刀序表 | 升 v1.8：版本行 + 【v1.8】沿革行；§8 补 TLS×LAN 语法；§13「TLS run × `--lan` 架构性拒绝」表述改写为 TLS×LAN 形态与证书语义；§14 增第十一波行 |
| [docs/阶段沿革.md](../../docs/阶段沿革.md) | 文末新节 | 五段式（刀面/宿主/客机/实测/未落）；节号落笔时实时重读文末取唯一（probe 时点二百六十二已过时不作依据）；销账 wave10 TLS×LAN 重启条件 |
| `.agents/notes/implemented/2026-11-15-vnc-wave11-unfreeze-tls-lan.md` | implement 阶段新建 | Problem/Decision/Alternatives（直参形态选型/sticky 退役 vs `#[cfg(test)]` 保留/TLS×LAN 三案（硬拒维持 vs rustls 上游 vs https 页）/WEB 键裁决）/Consequences/Related（与 wave1/wave10 笔记互链） |
| out/ | 冒烟/e2e 产物 | `vnc-w11-` 前缀截图与 QR 样张；口令样串不落仓 |

不改：[ws.rs](../../tools/berry/src/ws.rs)（web 代理是字节管道不经 `WsStream`；仅 `vnc check --ws` 用，若实现期发现需动记偏差）、[tls.rs](../../tools/berry/src/tls.rs)（`load_client_config`/`load_client_config_insecure`/`server_name` 原样复用）、qr.rs、THIRD-PARTY-NOTICES.txt（qrcodegen 行已在）、Cargo.toml/Cargo.lock（零新依赖）、justfile、tools/berry/web/ 全树、冻结面全列（背景节声明）。

## 测试矩阵

### 单元测试（cargo test，113 基线全保 + 新增 U52-U55（U56 条件项）；终值以实现期实数登记）

| 编号 | 面 | 断言 |
|---|---|---|
| U52 | 直参 resolve 纯函数 | `resolve(Launch::default())` → off 三元组；显式 spec 胜 `BERRY_VNC`；env 仅补缺省（spec/tls 两方向，F01 同构保留）；Launch 字段逐项入 `Resolved`（含 pass/pass_file 路径） |
| U53 | 准入矩阵重钉 | `lan_admission` 行为：TLS run + 有口令 → `Allow`（warn_extra 按 bind）；其余五行（Loopback/`--tls` 互斥拒/无口令拒/明文环回 Allow/明文非环回 Allow+warn_extra）断言逐字零变化 |
| U54 | TLS×LAN 上游判定与 URL | 上游形态判定（`lan` × `listen.tls` → ws/wss）；`build_page_url_lan` 在 TLS run 输出与明文逐字节同构（回归钉）；JSON `lan.upstream` 取值 |
| U55 | proxy_ws_tls 端到端 | 进程内 rustls 服务端（accept TLS → 收改写头断言 target=`/` 且其余头保真 → 101 → echo）经 `/ws` 分支拨入 TLS 上游：双向逐字节往返、一侧 EOF 双侧拆链、CONN_BUDGET 豁免；坏 CA 客户端 → 握手失败路径有声；仿 `wss_proxy_end_to_end_relay` |
| U56 | （条件项）工具栏六键 | `toolbar_buttons` 布局 = `6*bw+5*gap` 右锚；`toolbar_hit` 六臂命中且旧臂语义不变；label/tip 数组长度一致——既有 4 测试泛型迭代，预期零改写 |

### Chrome headless 端到端（t1-t4；方法沿 wave10：`--headless=new --dump-dom/--screenshot --virtual-time-budget`；脚本 /tmp、产物 `out/vnc-w11-*`；全程隔离 BERRY_HOME、真实 LAN 端口用毕即关、Chrome/QEMU 进程杀净）

| 编号 | 项 | 预期 |
|---|---|---|
| t1 | TLS run `--lan` 已连接 + 真桌面 | `--vnc-pass` + `vnc certs` 目录起 TLS run → `vnc web --lan`：URL/QR 与明文同构（host=LAN IP、`path=ws://…/ws`）；Chrome 经 LAN IP dump：body `berry-connected` + 「已连接（未加密）」（LAN 腿明文，如实）+ 分辨率行；`--screenshot` 留档对拍（调色板法，wave10 偏差 12 同款） |
| t2 | QR 样张 | 终端 QR 块留档；macOS Vision 实扫 payload 与剥 fragment URL 逐字节相等、不含 `#password=` |
| t3 | 明文 `--lan` 零回归 | 复跑 wave10 l1 核心断言：`berry-connected` + URL/QR 形态 + serving 行 |
| t4 | 错误路径与旗标门 | TLS run 无证书目录（meta `vnc_tls` 空）且无 `--ca` → rc=2 + certs hint；`--ca` 指坏 CA 目录 → 服务起、上游握手失败 stderr 有声、页面转错误态；`--ca` × `--insecure` → rc=2；无 `--lan` 给 `--ca`/`--insecure` → rc=2；全部错误输出不含口令 |

窗口路径（WEB 键若做）：无显示环境按 RULES 7 降级——单测（U56）+ 代码走查 + 降级记录，不做 Chrome 面、不虚报验证层级。

### 打包与全量门

| 编号 | 项 | 预期 |
|---|---|---|
| p1 | TPN 钉行实跑 | `build-appdir.sh` 重建 AppDir → `check-appdir.sh` 全 pass（含新 qrcodegen 行）；负例 = AppDir 内 TPN 删该行 → 对应 fail 行出现（验后就地还原） |
| g1 | 单测 | `cd tools/berry && PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/cargo-target" cargo test --release --target "$(rustc -vV \| sed -n 's/^host: //p')"` 全绿（113 + 新增；终值实数登记） |
| g2 | 构建与冒烟 | 仓库根 `just berry-build`（docs_guard）绿；`just berry-smoke`/`berry-vnc-smoke`/`berry-vnc-tls-smoke` 绿（后两支沿 wave10 verify 备注，手动加隔离 BERRY_HOME=/tmp 自起自收） |
| g3 | 文档门 | `node .agents/tools/check-docs.mjs` 零告警（本计划入列后基线自增仍 0） |
| g4 | sticky 残检 | 全仓 rg `set_launch\|take_launch\|sticky_launch\|LAUNCH_STICKY` 零命中（代码与测试） |

## 验收标准

1. 直参化成立：`Device::start` 签名带 `vnc: vnc_cfg::Launch`，sticky 五符号删除且 g4 零残；契约 1 表六个同步点全部落地；既有 VNC 起档/重启/继承/attach/crash-restart 路径行为零回归（g1/g2 证）。
2. TLS×LAN 兑现：TLS run 上 `vnc web --lan` 起服务（不再 rc=2），http 页 + rustls wss 上游，Chrome 真引擎 `berry-connected` + 真桌面截图实证（t1）；wave10 笔记 L28 重启条件在沿革新节与 wave11 笔记销账（旧笔记不改写，Curia 纪律）。
3. 证书语义全中（t4 + U55）：默认严格（run meta 目录）；`--ca` 覆盖；`--insecure` 警告；`--ca`×`--insecure` rc=2；错误路径与既有 TLS 面一致（缺件 rc=2、坏 CA 有声）。
4. QR/口令纪律：QR 内容 = 剥 fragment URL（Vision 实扫逐字节，t2）；口令不进 argv/日志/错误信息/QR 实体。
5. 明文 `--lan` 与环回模式零回归（t3 + g2 三冒烟）；`--tls` 互斥拒维持。
6. TPN 钉行：check-appdir.sh qrcodegen 对在位，正负例实跑（p1）。
7. WEB 键按契约 5 cut-line 裁决：做则 U56 绿 + 降级验证如实记录；砍则偏差节登记留后续刀。
8. 门全绿与 docs 面齐：g1-g4；README/CLI-first v1.8（§8/§13/§14）/沿革新节（节号实时文尾唯一）/Agent Note（Alternatives 四案）；本计划收口 `done`。
9. 纪律面：不 git add/commit；冻结面与并行会话文件零触碰（M 态文件以现状为基线）；window.rs 改动严格限 VNC 面，M 态重现即停该面并降级记录；冒烟自起自收隔离 BERRY_HOME；Chrome/QEMU 进程杀净；口令样串不落仓。

## 边界（非目标）

- 不做 https 页 on LAN（自签证书墙打死手机形态，wave10 契约 6 依据维持）；TLS×LAN 只加密 berry→QEMU 环回上游腿，LAN 腿仍是明文 http + VNC DES 端到端（与明文 `--lan` 同构的暴露面，note 行如实），不宣称「LAN 加密」。
- 不做公网暴露/UPnP/端口映射/mDNS/SetDesktopSize 翻译/剪贴板/SASL（wave5/6/9/10 沿）。
- 不动 window.rs 的 ROTATE/串口/渲染/菜单既有逻辑（仅 VNC 布线 + WEB 键两处 VNC 面）；berry-ui 组件库等并行新件零触碰。
- `vnc check --ws` 不经 LAN 代理，语义零变化。
- 沿革已提交内容不回改；沿革新节节号以落笔时实时重读文末为准（并行会话高频续写）。
- 真窗口（GUI）路径验证降级为单测 + 代码走查 + 降级记录（RULES 7），不虚报验证层级。
- Linux/Windows 的 LAN 解析分支本机不可测面沿 wave10 登记，本波不扩。

## 偏差

写作时点已知的与任务功能规格直觉的出入（implement 阶段如有新增在此续记）：

1. **`--ca`/`--insecure` 落在 `vnc web` 且设依赖门**：规格只说「证书目录语义沿 run meta vnc_tls 自动推/`--ca` 覆盖/`--insecure` 逃生门」未定旗标归属与门——裁决：两旗标为 `vnc web` 新增、无 `--lan` 时 rc=2（`--host`/`--qr` 同门先例），明文上游静默忽略（F17 先例），互斥 rc=2（`vnc check` L1473 先例）。
2. **WEB 键裁为「尾项 + cut-line」而非无条件做**：规格「可选（按 probe 裁决）」——probe 裁决 = 技术上可留（window.rs 基线后零活跃、六键化零测试改写）但它是本刀 window.rs 最大单体改动且无显示环境只能降级验证，故排序尾项、cut-line 三条件（M 态重现/前三项门未全绿/时间超限）命中即砍。
3. **WEB 键语义定为 spawn `berry vnc web --open`（环回）而非 `--lan`**：规格原文「明文 run 用环回 URL，TLS/口令语义按裁」——桌面窗口的自然受众是本机；TLS run 由 vnc web 自动落既有 https 双口环回形态，口令/QR 的 LAN 语义不进窗口按钮。
4. **sticky 退役裁「整体删除」而非 `#[cfg(test)]` 保留**：零测试钉 + 零残余消费点（契约 2），保留即死代码。
5. **直参形态裁「追加单参 `vnc: Launch`」而非 `Option<Launch>` 或参数结构体**：`Launch` 自带 `Default`（全 None = off）；结构体重构调用点同步数相同（4 处）但 diff 更大、动既有 7 位置参语义。
6. **CLI-first v1.8 触及 §13**：规格只列「§8 vnc web TLS×LAN/§14 刀序行」，但 §13 LAN 行（L579）现行契约含「TLS run × `--lan` 架构性拒绝」表述，不改则文档自相矛盾——增改该行，越出规格措辞之差在此登记。
7. **TLS×LAN 的 JSON 细节落为 `lan.upstream` 单键**（值 `ws`/`wss`）：规格「tls 细节入 JSON」未定键形，取最小键（页面仍 http，无 `tls.enabled` 语义空间）。

implement 阶段续记（2026-11-15）：

8. **单测实数 = 新增 5（U52/U53 重钉/U54/U55×2）而非计划的 4（U52-U55）**：U55 拆成端到端与坏 CA 拆链两测（进程内 rustls 服务端断言改写头 + echo 往返 vs wrong-CA 拆链有声），终值 118（113 基线零回归 + 5）。
9. **`LanTlsUpstream` 增第三字段 `dir: PathBuf`**（计划只列 cfg/name）：F30 有声失败路径的诊断需要证书目录名（proxy_ws_tls 的 stderr 行带 cert dir），无它则坏 CA 只有无名错误。
10. **t4f 初跑误用 `--ca`+`--insecure` 组合**（rc=2 互斥门正确拦截，服务未起属预期），改纯 `--insecure` 重验通过——过程瑕疵非缺陷，互斥门由此得一次活体复证。
11. **QR 样张归档做口令脱敏**（`out/vnc-w11-t2-qr-sample.txt` 的 URL 行 `#password=` 段替换为「样串已脱敏」）：QR 实体本就不含口令（Vision 证），脱敏只及归档文件的 URL echo 行——纪律 9 口令不落仓的执行细则。
12. **沿革节号 = 三百四十一**：probe 时点文尾二百六十二已过时（并行会话续写至三百四十），按计划口径实时重读取三百四十一（全文唯一）。
13. **WEB 键做成**（cut-line 三条件均未命中：window.rs M 态未现、前三项门全绿、时间未超限），U56 六键几何单测绿 + 降级记录（无显示环境真点按不可验）。

## 验收记录（verify，2026-11-15，隔离 BERRY_HOME=/tmp/berry-w11-verify 与 /tmp/berry-w11-smoke，全程不 git add/commit）

- v1 TLS×LAN 端到端全过：TLS run（--vnc on --vnc-ws on --vnc-pass + --vnc-tls certs，wss://127.0.0.1:59119、auth vencrypt）上 `vnc web --lan --qr` 正常起服务——URL 为 http://192.168.1.105:18971/?autoconnect=true&path=ws%3A%2F%2F…%2Fws&resize=scale（LAN IP + http 页 + ws 代理同口），serving/note/hint 行与明文 `--lan` 同构；Chrome headless dump-dom 命中 berry-connected + 「已连接（未加密）」+ 480×1000 分辨率行（DES 经代理 + rustls 上游全链连通）；真桌面截图落 out/vnc-w11-tls-lan.png（520x1040、742 唯色、30.2% 非背景像素，调色板与 implement 参照 vnc-w11-t1-tls-lan-desktop.png 同族六主色——非空白实证）；QR 样张刷新 out/vnc-w11-t2-qr-sample.txt（URL 行 #password= 段脱敏），QR 块经 Chrome 渲染 PNG 后 macOS Vision（VNDetectBarcodesRequest，QR 码型）实扫 payload 与剥 fragment URL 逐字节相等且不含 #password=。
- v1 证书语义与错误路径矩阵满格：缺省严格（run meta vnc_tls 自动推，主连接即证）；`--ca <正目录>` 显式→Chrome berry-connected=1（独立 run3 复测）；`--ca <空目录>`→rc=2「missing ca-cert.pem」+ certs hint（缺件）；`--ca <错 CA 目录>`→服务正常起、Chrome 页面非连接态（berry-connected 计 0）、stderr 两行有声「lan /ws proxy upstream 127.0.0.1:59119 failed (cert dir …): invalid peer certificate: BadSignature」（F30）；`--insecure`→stderr 一次性警告 + Chrome berry-connected=1；`--ca`×`--insecure`→rc=2；无 `--lan` 给 `--ca`/`--insecure`→各 rc=2；`--lan --tls` 显式→rc=2（互斥维持）；JSON `lan.upstream`=wss（TLS run）/ws（明文 run），`bind`=0.0.0.0；全部错误输出零口令。
- v1 明文 `--lan` 零回归过：独立明文 run（`device start` 后台 spawn 路径）上 URL/QR/serving/note 形态与 wave10 同构、Chrome berry-connected=1、JSON upstream=ws。
- v2 直参化回归过（降级面 + 本机可起真窗口的加验）：走查——`Device::start` 全仓恰 4 调用点全部带 `vnc: Launch` 直参（window.rs L272/L606/L898 + commands.rs L460），sticky 五符号全仓 rg 零残（唯一命中为 window.rs L447 历史注释文字，g4 过）；`restart_with`（L963+）meta 继承 vnc_spec/vnc_ws/vnc_tls + 一次性 0600 口令文件逻辑零改动（wave11 未触及该面）。单测——118 passed 0 failed，新增五测全绿（U52 resolve_direct_parameter_matrix/U53 admission_matrix 行 2 重钉/U54 lan_upstream_kind_and_url_shape_invariance/U55 lan_ws_tls_upstream_end_to_end + wrong_ca_tears_down/U56 six_button_layout_and_arrays）。CLI 等价面——TLS run（host 前台路径）与明文 run（device start 后台 argv 路径）的 status/check/--ws/check --ws --password 全过；`berry device restart`（客机侧 reboot，同 QEMU）后 status 同端口 59118/59119、tls/auth 保持、check 带口令 ok（分辨率瞬时 800x600 为客机重启初期 VGA 态，与 VNC 旗标无关）。真窗口活体——`berry shell --vnc on --vnc-ws on --vnc-pass --timeout 12` 真窗口开合 rc=0，窗口存活期 status 见 run-*-shell + uri/ws/auth=vnc 布线，`vnc check --ws` 显式口令与缺省读 vnc.pass 双路 ok（首跑 5s 一击 rc=7 为 set_password 注入竞态，加宽窗口复跑双绿澄清，非缺陷）。
- v3 TPN 钉行过：独立路径重建 AppDir（out/BerryOS.AppDir-w11v，验后已删）→ check-appdir.sh 四对 TPN 检查全 ok（含新「register qrcodegen 1.8」行）；负例=AppDir 内 TPN 删 qrcodegen 行 → 「FAIL third-party notices miss the qrcodegen 1.8 entry」+ 整体 rc=1。
- v4 全量门四路过：g1 cargo test --release（host triple）118 passed 0 failed；g2 `just berry-build`（docs_guard PASS 0 警）+ 三冒烟全绿——berry-smoke（marker ok + shot 800x600）/berry-vnc-smoke（status uri+ws_uri、check 三态 rc7、--ws 共 5 断言）/berry-vnc-tls-smoke（含 wss 自动目录 ok、错 CA rc=7 严格拒、--ca×--insecure rc=2）；g3 check-docs 253 篇 0 告警（含本计划与本记录）；docs 面齐——CLI-first 头块 v1.8 +【v1.8】沿革行 + §8 L362 命令树 + §13 TLS×LAN 段、沿革三百四十一节（L6199，全文唯一）、Agent Note Alternatives 五案（计划四案 + WEB 键语义第五案）。
- verify 过程观察（非缺陷）：①verify 期间发现同线（wave11）遗留进程 `berry vnc web --lan --port 18700 --no-qr`（08:58:46 起，非本会话端口段，疑 implement 阶段末次探针漏收或收尾探活）已按自清纪律杀净——该进程无状态，若属仍在活跃的 implement 会话其命令侧可见非零退出，无数据损失；②JSON `url` 字段携 #password= fragment 系 wave10 以来既定的口令交付面（stdout 归用户），非本波回归；③implement 偏差 8-13 逐条与实况相符（118 实数、LanTlsUpstream 第三字段 dir、t4f 互斥门、QR 脱敏、三百四十一、WEB 键做成）。
- 自清与纪律：隔离 BERRY_HOME 两处及 /tmp 全部含口令临时文件（w11-pw.txt/各 log）已删净；本会话 QEMU/host/web/Chrome 进程全收（device list 空、pgrep 零残留）；并行会话进程（berry-kernel 调试 QEMU、knife6、berry-live）零触碰；仓库写入仅本计划验收记录一节与 gitignored out/ 三件（vnc-w11-tls-lan.png/vnc-w11-t2-qr-sample.txt 刷新 + implement 留存 vnc-w11-t1-tls-lan-desktop.png）；临时 AppDir 已删。
- v5 docs 收口（2026-11-15，docs 会话，工作树面、不 git add/commit）：README 窗口快捷栏行刷六键（`WEB` 语义/幂等拒/换档需重点/退出即收/真点按无显示环境未验的降级注记）——原「5 键」行在六键化后陈旧，属任务 1「窗口按钮 + 已知限制刷新」的收口；CLI-first §9.1 布局图六键 + C2f tooltip 补 BROWSER + §9 增「WEB 键已落（v1.8）」bullet、§15「看桌面要先装 VNC 客户端」行补 v1.8 子句（TLS run 亦可 `--lan`，http 页 + 上游 rustls wss）——§9/§15 两处越出计划改动清单原文（同偏差 6 先例：六键化后 5 键图与映射行不自改即文档自相矛盾）；Agent Note 头块对齐 notes README 两行式（去 `Date:`/`Type: note` 行——`Type:` 系全树唯一违例，rg 防重复检索同批过：TLS×LAN/直参面全树唯此一篇，wave1/wave5 命中为历史 sticky 机制旧篇、Related 已互链）；沿革三百四十一节核实已随 71bb9f358 入库（工作树与 HEAD 一致，已提交内容零触碰，本会话未改沿革）；v1.8/§8/§13/§14 与 README 第十一波段/已知限制行经 implement 阶段落位、本会话逐段复读核实在案；check-docs 复跑零告警（docs 会话终值 253 篇 0 条）。

## 未完成事项

- DEFERRED（WEB 键真点按验证）: 原因 = 本机无显示环境，真窗口点按不可起；归属 = 后续有真显示环境的走查刀；触发条件 = 真显示器/桌面会话下人工点按走查——当前验证层级 = 单测 U56 六键几何 + 代码走查（沿革三百四十一「未落」已登记，不虚报）。
- DEFERRED（Linux/Windows 的 LAN 地址解析实机验证）: 原因/归属/触发条件沿 wave10 登记未扩——本波未触及该面（纯函数面已单测，`--host` 显式兜底）。
- DEFERRED（壳页对 `lan.upstream: wss` 的 UI 标示）: 原因 = JSON 细节已给而页面文案未动（明文 LAN 腿「已连接（未加密）」提示已如实覆盖 TLS run 场景）；归属 = 后续壳页刀（`web/` 全树冻结纪律沿 wave10）；触发条件 = 壳页解冻批次。
