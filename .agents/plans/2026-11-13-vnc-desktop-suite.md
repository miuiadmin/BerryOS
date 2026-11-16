# VNC 桌面全套支持实施计划（berry 对外 VNC 暴露）

Type: plan
Status: done
Updated: 2026-11-13

一句话说明：给 berry 增加对外 VNC 端点（QEMU 第二个 `-vnc` TCP 服务 + 可选 websocket + 口令认证 + `berry vnc` 四动词），让任何标准 VNC 客户端（macOS 屏幕共享、TigerVNC、RealVNC、noVNC）直连 BerryOS 桌面看画面、发输入；内部面（unix socket、RamSource、QMP broker、control.sock）零改动。

## 背景

BerryOS 桌面目前只有 berry 自己能看、能点：帧面主路是共享客机 RAM（`ram.bin` 的 `RamSource`），输入走 run 目录 `vnc.sock` 上的 `VncInput`，这套内部面只服务于 berry 的 CLI/GUI/host 三角色。外部标准 VNC 客户端无法接入——没有对外 TCP 监听，`display/vnc.rs` 的 RFB 客户端也只会 None 认证（遇到带口令的服务端直接报错）。本刀把 QEMU 已有的 VNC server 能力面向外部打开：同一个 QEMU 进程声明第二个 `-vnc`（TCP，可选 websocket、可选口令），berry 负责端点解析、口令策略、run 目录产物、控制面 op、CLI 动词与自检（`vnc check` 以完整 RFB 客户端身份走对外端点 + 认证握手并取一帧）。产品依据是 [../../docs/Berry设备平台-CLI-first设计-v1.0.md](../../docs/Berry设备平台-CLI-first设计-v1.9.md)（§5 CLI verbs、§7 控制面、§13 安全、§14 刀序）；QEMU 侧语法与限制以 probe 阶段实测（f1–f6，脚本与日志在 `/tmp/vncprobe/`，会话临时不入仓）为准，冲突处见「偏差」节。前置事实：QEMU 11.0.3 同进程双 `-vnc` 可用；`-vnc` 的 TCP 地址单冒号数字是 display 编号（实际端口 = 数字 + 5900），`tcp:` 前缀与 `host::port` 均被拒；`password=on` 是 per-endpoint，QMP `set_password` 只作用于第一个声明的 server（id=default）；`query-vnc-servers` 返回全部端点数组；ramfb 静默忽略客户端 SetDesktopSize。

## 契约

### 旗标与环境变量

`--vnc <spec>` 布到 `device start`、`host`（隐藏命令）、`shell` 三个入口；env `BERRY_VNC` 兜底，解析优先级为「显式旗标 > `BERRY_VNC` > off」（先例 `BERRY_DATA_DISK`，env 深层读取）。

| spec 取值 | 绑定 host | 端口 | 说明 |
|---|---|---|---|
| `off`（默认） | — | — | 不暴露对外 VNC；argv 与现状等价（仅内部 `-vnc unix:…`） |
| `on` | `127.0.0.1` | 自动挑空闲口（≥5900） | 开发者最短路径 |
| `<port>` | `127.0.0.1` | 字面端口 5900..65535 | 如 `--vnc 5971` |
| `<host>:<port>` | 指定 host | 字面端口 5900..65535，或 `0`/`auto`（自动挑） | host 形态：IPv4（`127.0.0.1`、`0.0.0.0`）、hostname（`localhost`）、方括号 IPv6（`[::1]`、`[::]`）、裸 `::`（写成 `:::5971` 形态） |

解析规则：按最后一个冒号切分 host 与 port；空 host（串以 `:` 开头）、无端口的裸 host、非数字端口、端口小于 5900 或大于 65535，一律 `E_USAGE`（退出码 2）并给 hint（说明 QEMU `-vnc` 是 display 编号语义，字面端口下限 5900）。环回判定集合：空（默认 `127.0.0.1`）、`127.0.0.1`、`localhost`、`::1`、`[::1]`；其余（含 `0.0.0.0`、`::`、`[::]`、任意外部地址）为非环回。

`--vnc-ws <on|off|port>`（默认 `off`）：`on` = 自动挑空闲 ws 端口；`<port>` = 字面端口 1..65535。前提 `--vnc` ≠ off，否则 `E_USAGE`。无论哪种取值，QEMU 侧一律显式 `websocket=<port>`（不用 `websocket=on`——它的默认端口是 5700+display，与 display 编号联动、易撞口，见偏差 5）。

`--vnc-pass <pw>`：显式口令；前提 `--vnc` ≠ off。口令经 QMP `set_password` 运行期注入，不进 QEMU argv、不进日志、不进错误信息（QEMU argv 里只出现 `password=on` 开关）。

### 口令策略矩阵

不提供「非环回免口令」逃生门（安全姿态对齐设计文档 §13）。

| 绑定 | `--vnc-pass` | 结果 | `vnc.pass` | QEMU `password=` |
|---|---|---|---|---|
| 环回 | 未给（默认） | 无认证，任意客户端可连 | 不写 | off |
| 环回 | 给定 | 认证，口令 = 所给 | 写（0600） | on |
| 非环回 | 未给 | 认证，口令 = 自动生成随机 8 字符 | 写（0600） | on |
| 非环回 | 给定 | 认证，口令 = 所给 | 写（0600） | on |

随机口令：8 字符，字符集 `[A-Za-z0-9]`，熵源 Unix 读 `/dev/urandom`、非 Unix 用时间纳秒 xor pid xor 计数器的回退混合（DES 有效口令只有 8 字节，熵上限本就受限，见「边界」）。

### run 目录产物与 meta.json

仅列新增项；既有产物（`vnc.sock`、`qmp.sock`、`control.sock`、`control.token`、`ram.bin`、`serial.log`、`qemu.log`、`events.jsonl`、`vars.fd`、`state.json`）不变：

- `vnc.listen`——JSON 对象 `{"host":"127.0.0.1","port":5971,"ws":6001}`（ws 未开时为 `null`）；`--vnc` ≠ off 时写。
- `vnc.pass`——口令原文（无换行），权限 0600（先例 `control.token`）；认证开启时写，环回默认无口令时不写。

`meta.json` 新增字段：`"vnc_spec"`（原始 spec 串，如 `"on"`、`"127.0.0.1:5971"`、`"off"`）与 `"vnc_listen"`（同 `vnc.listen` 的对象，off 时为 `null`）。重启链依赖这两个字段继承配置。

### QEMU argv 形态

对外端点必须写在第一个 `-vnc`（id=default，QMP `set_password` 的唯一作用面，见偏差 2），内部 unix socket 写第二个：

```text
-vnc 127.0.0.1:71,password=on,websocket=6001   ← 对外（display 编号 = 字面端口 - 5900）
-vnc unix:<run>/vnc.sock                        ← 内部，零改动
```

口令在 QMP 连接建立后经 `set_password`（`{"protocol":"vnc","password":"…"}`）注入；`restart_in_place` 与 `--restart-on-crash` 的 QEMU 重生后必须重新注入。

### 控制面 op

`vnc`（状态查询）请求 `{"op":"vnc","args":{}}`；应答（已暴露）：

```json
{"ok":true,"result":{"enabled":true,
  "listen":{"host":"127.0.0.1","port":5971,"ws":6001},
  "uri":"vnc://127.0.0.1:5971","ws_uri":"ws://127.0.0.1:6001",
  "auth":"vnc","has_password":true,
  "clients":[{"host":"127.0.0.1","service":"53128","family":"ipv4","websocket":false}]}}
```

应答（未暴露）：`{"ok":true,"result":{"enabled":false}}`。`listen`/`uri` 取 `vnc.listen` 文件；`auth`/`clients` 取 QMP `query-vnc-servers` 中 id=default 条目（见偏差 4）；`has_password` = `vnc.pass` 存在。

`vnc_passwd`（轮换）请求 `{"op":"vnc_passwd","args":{"password":"newpw"}}` 或 `{"op":"vnc_passwd","args":{"generate":true}}`；应答 `{"ok":true,"result":{"generated":true,"password":"aB3xK9mQ"}}`。执行序：QMP `set_password` → 重写 `vnc.pass`（0600）→ 追加事件 `{"v":1,"t":"action","action":"vnc.passwd","source":"control"}`（事件不记口令值）。端点启动即无认证（环回默认）时 QMP 拒绝，返回 `E_USAGE` 并 hint「重启时加 `--vnc-pass` 开认证」（QEMU 限制，见偏差 3）。错误信息与日志均不含口令。

`hello` 的 capabilities 列表更新为：`["hello","status","stats","power","stop","logs","display","shortcut","events","screenshot","key","tap","swipe","text","vnc","vnc_passwd"]`。`status` 的 result 增补 `"vnc"` 字段：已暴露时 `{"host":"127.0.0.1","port":5971,"uri":"vnc://127.0.0.1:5971"}`，未暴露时 `null`（供 `device status` 顺带显示；control 不可达时 CLI 回退读 run 目录 `vnc.listen`）。

### CLI 动词

`berry vnc status [--run] [--json]`：

```text
vnc: 127.0.0.1:5971 auth=vnc clients=1
uri: vnc://127.0.0.1:5971
ws:  ws://127.0.0.1:6001
run: /Users/dev/.berry/run/1763000000-1234-host
```

```json
{"v":1,"t":"vnc.status","run":"…","enabled":true,"listen":{"host":"127.0.0.1","port":5971,"ws":6001},"uri":"vnc://127.0.0.1:5971","ws_uri":"ws://127.0.0.1:6001","auth":"vnc","has_password":true,"clients":[{"host":"127.0.0.1","service":"53128","family":"ipv4","websocket":false}]}
```

未暴露时人类可读输出 `vnc: not exposed (run …)`，JSON 输出 `"enabled":false`；control 不可达时降级读 `vnc.listen`（无 clients）。

`berry vnc passwd [--run] [--set <pw>|--generate]`（默认 `--generate`）：

```text
vnc: password rotated (generated): aB3xK9mQ
```

```json
{"v":1,"t":"vnc.passwd","run":"…","generated":true,"password":"aB3xK9mQ"}
```

`--set` 时 `generated:false`，人类可读输出不回显口令（用户自己给的）。

`berry vnc check [--run] [--password <pw>]`（自检）：以完整 RFB 客户端身份连对外 TCP 端点，走认证握手（口令优先级：`--password` > run 目录 `vnc.pass`；端点带认证而无口令可用时报错并 hint），取一帧，报告尺寸与来源：

```text
vnc check: ok 480x1000 via vnc://127.0.0.1:5971 (auth vnc)
```

```json
{"v":1,"t":"vnc.check","run":"…","ok":true,"uri":"vnc://127.0.0.1:5971","width":480,"height":1000,"auth":"vnc"}
```

`berry vnc open [--run]`：macOS 执行 `open vnc://host:port`（口令由客户端侧弹窗输入，不拼进 URI）；其他平台打印 URI 与指引（TigerVNC/RealVNC/noVNC）；失败不致命（退出码 0）：

```text
vnc: opened vnc://127.0.0.1:5971
```

```json
{"v":1,"t":"vnc.open","run":"…","uri":"vnc://127.0.0.1:5971","opened":true}
```

`berry device status` 人类可读输出在暴露时顺带端点：`…: running (profile phone, 256MiB, pid 123, vnc 127.0.0.1:5971)`。

## 文件改动清单

实施顺序即下列次序（依赖从底向上）：net → des → vnc_cfg → vnc 认证 → qmp → qemu → device → control → commands → main → 文档。

1. 新增 `tools/berry/src/vnc_cfg.rs`——本刀的解析与策略中枢。`enum VncSpec { Off, On, Addr { host: String, port: PortSpec } }` 与 `enum WsSpec { Off, On, Port(u16) }`；`parse_spec(&str)`/`parse_ws(&str)`（含上表全部合法/非法样例的判定）；`enum PortSpec { Auto, Fixed(u16) }` 与 `port_to_display(u16) -> Result<u16>`（5971→71、5900→0、5899→Err）；`is_loopback(&host) -> bool`；`resolve_auth(spec, explicit_pass) -> AuthDecision { None, Password(String) }`（策略矩阵）；`generate_password() -> String`；`uri/ws_uri` 构造；`struct Exposed { qemu_arg: String, host: String, port: u16, ws_port: Option<u16>, password: Option<String> }`（QEMU `-vnc` 值的完整构造：`host:{display}` + 可选 `,password=on` + 可选 `,websocket=<port>`）；进程级 launch 覆盖 `static LAUNCH: Mutex<Option<…>>` 及 `set_launch`/`take_launch`（供 shell 入口绕过冻结的 window.rs，见偏差 7）；`resolve_from_env()` 读 `BERRY_VNC`。单测主场。
2. `tools/berry/src/net.rs`——`Endpoint::qemu_vnc()`（第 14–20 行）Tcp 臂从 `format!("tcp:{a}")` 改为拆 `host:port` 后产 `format!("{host}:{}", port - 5900)`（端口 <5900 返回错误由调用方兜底改为本函数返回 `Result`，或在调用前由 vnc_cfg 保证；实施时取 `Result<String>` 并更新唯一调用点 qemu.rs 第 202 行）。`qemu_qmp()` 不动（`tcp:` 前缀对 `-qmp` chardev 解析器合法，实测并行 run 正常）。`Endpoint::Unix` 臂不动。
3. 新增 `tools/berry/src/display/des.rs`——自含 DES-ECB（初始/最终置换、S 盒、Feistel 轮，纯位运算与查表，零第三方 crate；workspace 现依赖仅 clap/serde_json/winit/softbuffer/miniz_oxide，保持零新增）。接口：`des_ecb_encrypt_block(key: &[u8;8], block: &[u8;8]) -> [u8;8]` 与 `vnc_challenge_response(password: &str, challenge: &[u8;16]) -> [u8;16]`（口令截断/补零到 8 字节，key 每字节位反转，16 字节挑战按两个 ECB 块加密）。单测：标准向量（key `133457799BBCDFF1`、明文 `0123456789ABCDEF`、密文 `85E813540F0AB405`）；位反性质 `rev(DES(k,p)) == DES(rev(k),rev(p))`；口令截断/补零。`display/mod.rs`（第 1–4 行）增 `pub mod des;`。
4. `tools/berry/src/display/vnc.rs`——`handshake()`（第 111 行）扩展为 `handshake_auth(stream, password: Option<&str>)`：服务端 offered types 含 1 时维持现状；含 2 且带口令时走 VNC auth（收 16 字节 challenge → `vnc_challenge_response` 应答 → 读 `SecurityResult` u32，非 0 时尽力读 reason 并报错，错误信息不含口令）；带口令但服务端只 offered 1 时按 None 认证继续（`vnc check` 报告实际 auth）。返回值增第四元 `auth_used`。`VncSource::connect`（第 181 行）与 `VncInput::connect`（第 265 行）各拆出 `connect_auth(endpoint, Option<&str>)`，原 `connect` 委托 `None`——既有 None 认证路径与 unix socket 用法零回归。安全类型列表与 SecurityResult 语义以 RFB 3.8 为准（probe f3 实测 u32）。
5. `tools/berry/src/qmp.rs`——`QmpClient` 增两个薄封装：`set_vnc_password(&mut self, pw: &str)`（`execute("set_password", {"protocol":"vnc","password":pw})`）与 `query_vnc_servers(&mut self)`（`execute("query-vnc-servers", {})`）。既有 `execute`（第 53 行）不改；QEMU 错误 desc 不含口令，透传安全。
6. `tools/berry/src/qemu.rs`——`spawn_image`（第 179 行）签名末尾追加 `ext_vnc: Option<&str>`；在第 202 行既有 `cmd.arg("-vnc").arg(vnc.qemu_vnc())` 之前插入 `if let Some(v) = ext_vnc { cmd.arg("-vnc").arg(v); }`（对外端点先声明 = id=default，保 QMP `set_password` 作用面）。第 258–265 行 rtc fw_cfg 段（前会话未提交改动）不动，新代码不触碰该段。两个调用点同步：`commands.rs` 第 204 行（shot，传 `None`）与 `device.rs` 第 310 行（boot_runtime，传解析结果）。
7. `tools/berry/src/device.rs`——`Device` 结构体（第 20 行）增字段 `vnc_ext: Option<vnc_cfg::Exposed>`；`Device::start`（第 44 行）签名不变（window.rs 第 270/582 行两个调用点冻结，见偏差 7），函数体开头解析：`vnc_cfg::take_launch()`（shell/host/device start 入口已设）> `BERRY_VNC` env > off；解析产物 `Some(exposed)` 时写 `vnc.listen`、按策略写 `vnc.pass`（0600，先例 control.token 的 `PermissionsExt` 用法）、meta.json 增 `vnc_spec`/`vnc_listen`（meta 构造在第 74–88 行）。`boot_runtime`（第 288 行）增参 `ext: Option<&vnc_cfg::Exposed>` 并透传给 `spawn_image`；QMP 连接（第 337 行）成功后、ControlServer 启动前，若 `ext.password` 存在则 `qmp.set_vnc_password`——这使 `restart_in_place`（第 139 行，复用同一 `self.vnc_ext` 与同一已解析端口）与 `--restart-on-crash` 自动获得「同端口同口令 + 重生后重注」。`endpoints()`（第 374 行）与 `free_port()`（第 397 行）：free_port 改为循环挑直到 ≥5900（Windows 内部面 `-vnc` display 编号语义的正确性，偏差 1 顺带修复）。
8. `tools/berry/src/control.rs`——`dispatch`（第 239 行）增 `"vnc"` 与 `"vnc_passwd"` 两个 match 臂（行为见契约节；`vnc` 需 `qmp` 句柄读 `query-vnc-servers`，run 目录读 `vnc.listen`/`vnc.pass`）；`"hello"` 臂（第 248 行）capabilities 列表追加 `"vnc","vnc_passwd"`；`"status"` 臂（第 269 行）result 增 `vnc` 字段；`vnc_passwd` 成功后重写 `vnc.pass`（0600）并 `append_event`（第 502 行，事件无口令值）。
9. `tools/berry/src/commands.rs`——`HostArgs`（第 53 行）、`DeviceStartArgs`（第 63 行）、`ShellArgs`（第 75 行）各增 `vnc_spec: Option<String>`、`vnc_ws: Option<String>`、`vnc_pass: Option<String>`；`host()`（第 418 行）与 `device_start` 前台路径在 `Device::start` 前调 `vnc_cfg::set_launch`；`commands::shell`（第 326 行）同样在 `window::run` 前设全局（window.rs 零改动）。`device_start_run`（第 813 行）把旗标透传给 host 子进程 argv（`--vnc`/`--vnc-ws`/`--vnc-pass`，另加隐藏 `--vnc-pass-file <path>` 供跨 run 继承，口令不进子进程 argv）。`restart_with`（第 869 行）继承旧 run 的 `meta.json` `vnc_spec` 与 `vnc.pass`：有 `vnc.pass` 时以 `--vnc-pass-file` 传旧口令；spec 为显式端口时改为先停旧再起新（同端口，偏差 6），auto/on 端口维持先起新后停旧、新 run 重挑口。新增 `vnc_status`/`vnc_passwd`/`vnc_check`/`vnc_open` 四个命令函数（`vnc_check` 用 `VncSource::connect_auth` 连对外端点取帧）。`device_status`（第 1056 行）输出顺带 vnc 端点（control result 的 `vnc` 字段，降级读 `vnc.listen`）。`endpoints()`（第 1244 行）与 `free_port()`（第 1267 行）同 device.rs 的 ≥5900 修复。
10. `tools/berry/src/main.rs`——`Cmd` 增 `Vnc(VncCmd)`，`VncCmd` 子命令 `Status/Passwd/Check/Open`（`--run` 全带；`passwd` 带 `--set`/`--generate`；`check` 带 `--password`）；`StartCmd`（第 276 行）、`HostCmd`（第 303 行）、`ShellCmd`（第 322 行）各增 `--vnc`/`--vnc-ws`/`--vnc-pass`（Host 另有隐藏 `--vnc-pass-file`）；`dispatch`（第 428 行）接线到 commands 新函数；无参默认 shell 路径（第 578 行起）不动（`BERRY_VNC` env 在 Device 层生效）。
11. `tools/berry/README.md`——「命令」节增 `vnc` 四动词与 `--vnc` 三入口示例；「环境变量」表增 `BERRY_VNC`；「已知限制」增 DES 强度与 SetDesktopSize 边界。
12. `docs/Berry设备平台-CLI-first设计-v1.0.md`——§5 命令树（第 317 行起）增 `vnc status|passwd|check|open` 与三入口旗标；§7.3 op 表（第 280 行起）增 `vnc`/`vnc_passwd` 两行、§7.1 传输说明补对外 VNC 端点不属控制面；§13 安全表（第 538 行起）增「对外 VNC 暴露」行（口令策略矩阵要点 + DES 强度有限）；§14 刀序表（第 549 行起）增本刀行。
13. `docs/阶段沿革.md` 与 `docs/Berry平台-剩余缺口.md`——仅在实施时确认有相关行才补记（本阶段不预写）。
14. `.agents/notes/implemented/` 新笔记一篇——implement 阶段与非平凡变更同批落（Curia 规则 3），记关键裁决（端点顺序、口令存放、DES 自含、全局覆盖布线）。

## 测试矩阵

单测（`cd tools/berry && PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/cargo-target" cargo test --release --target "$(rustc -vV | sed -n 's/^host: //p')"`，新增用例全部落 vnc_cfg.rs / des.rs / net.rs）：

| # | 项 | 预期 |
|---|---|---|
| U1 | spec 合法样例：`off`/`on`/`5971`/`127.0.0.1:5971`/`localhost:0`/`0.0.0.0:5902`/`[::1]:6000`/`::5902` | 各自解析为对应 VncSpec；`0` 归一为 Auto |
| U2 | spec 非法样例：`tcp:127.0.0.1:5971`、`5899`、`70000`、`abc`、`:5901`、`::`、`127.0.0.1`（无端口） | 全部 Err，错误串含 hint |
| U3 | 口令策略矩阵四格（环回/非环回 × 有/无 `--vnc-pass`） | 与契约表逐一相符 |
| U4 | URI 构造：IPv4/IPv6/ws | `vnc://127.0.0.1:5971` 等正确形态 |
| U5 | 口令生成器 | 长度 8、字符集 `[A-Za-z0-9]`、连调两次不同 |
| U6 | DES 标准向量 | key `133457799BBCDFF1` + 明文 `0123456789ABCDEF` → `85E813540F0AB405` |
| U7 | DES 位反性质与 VNC key 派生 | `rev(DES(k,p)) == DES(rev(k),rev(p))`；口令 >8 截断、<8 补零 |
| U8 | port↔display 映射 | 5971→71、5900→0、5899→Err |
| U9 | net.rs `qemu_vnc` Tcp 臂 | `127.0.0.1:5971` → `127.0.0.1:71`；Unix 臂输出不变 |
| U10 | `vnc.listen` 读写往返 + 权限 | round-trip 相等；unix 下 mode 0600 |
| U11 | 既有测试 | 全绿（pixel_bgrx/pixel_565/qmp_error_shape 等零回归） |

真机冒烟（macOS + `/opt/homebrew/bin/qemu-system-aarch64` 11.0.3 + `out/berryos.img`；只用 `berry device stop/cleanup --run <id>` 收自己的 run，严禁 `--all`）：

| # | 场景 | 预期 |
|---|---|---|
| v1 | `device start --profile phone --vnc on --json`；`vnc status`；`device status`；查 run 目录与 `ps` | status 报 uri/`has_password:false`/`auth:none`；device status 顺带端点；`vnc.listen` 在、无 `vnc.pass`；meta 有 `vnc_spec:"on"`；QEMU argv 含双 `-vnc`（对外在前）且无口令 |
| v2 | `vnc check`（无认证环回）+ 并发内部面 `screenshot` + `input tap` | check 经对外 TCP 完整 RFB 取帧，尺寸 = profile 渲染尺寸（480x1000）；check 进行中 `vnc status` 的 clients ≥1；内部面动作全部成功（双端点共存，f1） |
| v3 | 口令全流程：环回 `--vnc-pass pw1`；非环回 `0.0.0.0:<port>` 默认；`passwd --set/--generate` | check `--password pw1` 过、错口令报 AUTH FAILED；非环回自动生成 `vnc.pass`（0600）且 check 默认读它通过；轮换后旧口令败、新口令过（f3）；环回无口令 run 上 `passwd` 回 E_USAGE + 重启 hint |
| v4 | `--vnc on --vnc-ws on` 与 `--vnc-ws <port>` 两形态 | status 报 ws 端口；python ws 探针（照 `/tmp/vncprobe` 脚本形态）完成 HTTP 101 + RFB 3.8 握手 + ServerInit（f5）；`query-vnc-servers` 见 `websocket:true` 条目 |
| v5 | 输入双路：python RFB 客户端经对外端点发 PointerEvent；同时 `berry input tap` 走内部 unix | 客机侧均有反应（串口/画面差分佐证）；两路互不干扰 |
| v6 | 重启保持：`--vnc 127.0.0.1:5971 --vnc-pass pw --restart-on-crash` → kill QEMU pid；`display set phone16 --restart` | 同 run 复活后 `lsof` 仍 5971、`vnc check --password pw` 直接过（重生重注口令）；`device restart`（guest 重启）端点口令不变；跨 run 重启后新 run 同 spec 同口令、显式端口仍 5971 |
| v7 | 回归：无 `--vnc` 起会话 | argv 与改动前等价（仅一个 `-vnc unix:`）；无 `vnc.listen`/`vnc.pass`；`just berry-smoke`、screenshot、record、play 全绿；U11 全绿 |

## 验收标准

1. U1–U11 单测全绿（新增 ≥11 项），既有测试零回归。
2. 冒烟 v1–v7 全过，每项留证据（命令 + 输出摘录）。
3. 口令三不出现：QEMU argv（`ps` 验证）、`qemu.log`/`events.jsonl`、CLI 错误信息与 hint。
4. 内部面零回归：默认路径（`--vnc off`）QEMU argv 与改动前逐参等价；screenshot/input/record/play 冒烟通过。
5. 人工验收一次标准客户端：macOS 屏幕共享（`berry vnc open`）或 TigerVNC 直连看画面、发输入成功（带口令与不带口令各一次）。
6. `node .agents/tools/check-docs.mjs` 报告模式零告警；`just berry-build`（含 docs-guard）通过。

## 边界

- 客户端发起的 SetDesktopSize 桌面调整在 ramfb 后端下不被支持（QEMU 11.0.3 静默忽略，无 252 回包、重连尺寸不变，probe f6）：分辨率变更走 `berry display` profile 重启；客机发起的几何变更会经 DesktopSize 广播给 VNC 客户端（`VncSource` 已处理 -223 伪编码）。
- VNC 经典 DES 口令强度有限（有效口令 8 字节、56 位密钥、挑战明文可离线穷举），仅适合开发/内网，不是 TLS 级安全；安全姿态对齐设计文档 §13，公网暴露不在支持范围。
- 控制面（`control.sock`/`control.token`）与 QMP（`qmp.sock`）不随 VNC 暴露：对外只有 RFB 端点（TCP/ws）；控制面仍本机 0600。
- 非环回绑定强制口令（无免口令逃生门）；即便有口令，DES 强度决定它只是入门闸，须配合受信内网使用——CLI 在非环回暴露启动时打印一次性安全提醒。

## 偏差

probe 实测与功能规格假设的冲突，逐条记录以 probe 为准（QEMU 11.0.3 事实优先，规格按可表达子集收窄）：

1. SPEC「port 1..65535」 vs f2（`-vnc` 单冒号数字是 display 编号，实际端口 = 数字 + 5900；`tcp:` 前缀与 `host::port` 双冒号均被拒）——以 probe 为准：字面端口收窄为 5900..65535，`net.rs` 的 `qemu_vnc()` Tcp 臂必须从 `tcp:{a}` 改为 `host:{port-5900}`；Windows 内部面 `free_port()` 可能挑出 <5900 端口的潜在错一并修复（同语义）。
2. SPEC「口令经 QMP set_password 注入」未涉端点归属 vs f3（双 server 时 `set_password`/`change-vnc-password` 只作用于第一个声明的 server，id=default 无 `password=on` 时直接报 `VNC password authentication is disabled`）——以 probe 为准：对外 TCP 端点写成第一个 `-vnc`（id=default），内部 unix 写第二个；内部面零改动依赖 `password=on` 的 per-endpoint 语义（f3 D3 实测另一端点保持 None）。
3. SPEC「vnc_passwd 轮换」无前置条件 vs f3（default 未开 `password=on` 则 set_password 必败）——以 probe 为准：轮换仅对「启动即认证」的端点可用；环回默认无口令的 run 上 `vnc passwd` 返回 E_USAGE 并提示重启开认证，不做运行期「从无到有」开认证。
4. SPEC「clients 走 query-vnc 或 query-vnc-servers」两可 vs f4（`query-vnc` 只返回 default 单对象；`query-vnc-servers` 返回数组、可辨 id/server[]/clients[]/websocket）——定为 `query-vnc-servers`，取 id=default 条目的 clients。
5. SPEC「`--vnc-ws on`」未定端口 vs f5（`websocket=on` 的端口是 5700+display，与 display 编号联动、易撞口）——定为 `--vnc-ws on` 一律自动挑空闲口并显式传 `websocket=<port>`（f5 实测字面端口可用，裸 ws 无 TLS 可完成全 RFB 握手）。
6. SPEC「重启链（含 `display set --restart`、RES 菜单 restart_profile）保持同端口同口令」 vs 既有 C2g 先起新后停旧（两 QEMU 并存窗口内同端口物理不可绑定）——分层落地：同 run 重生（`restart_in_place`、`--restart-on-crash`）硬保同端口同口令；跨 run 重启保同 spec 同口令——显式端口改先停旧再起新（该子场景放弃「新起失败旧还在」回滚，失败时如实报错），auto/on 端口新 run 重挑并在 CLI 输出新端点。
7. SPEC「`--vnc` 布到 shell 入口」 vs `tools/berry/src/window.rs` 冻结（并行会话未提交改动，且其第 270/582 行以位置参数调 `Device::start`，签名改动会破坏编译）——shell 入口经 `commands::shell`（可改）在 `window::run` 前设进程级 launch 覆盖（vnc_cfg 全局）+ `BERRY_VNC` env 兜底，`Device::start` 签名不动；window.rs 解冻后可换直参。

## 未完成事项

（design 定稿保存时为空；implement/verify/docs 三阶段收尾后，本节只剩真实遗留。）

implement 阶段（2026-11-13）代码与单测全部落地：文件改动清单 1–10 完成，U1–U11 全绿（cargo test 39 通过 = 既有 21 零回归 + 新增 18），冒烟 v1–v7 全过（双 `-vnc` 对外在前、口令全流程、ws 两形态、kill QEMU 重生同端口同口令、跨 run `display set --restart` 同端口同口令、无 `--vnc` 回归 argv 等价），口令三不出现经 `ps`/`qemu.log`/`events.jsonl`/错误信息逐项验证（证据见「验收记录」）。

docs 阶段（2026-11-13 同批）收尾：文件改动清单 11–14 完成——`tools/berry/README.md`（`vnc` 动词与三旗标示例、`BERRY_VNC` 环境变量、控制面表 VNC 行、已知限制三条）、`docs/Berry设备平台-CLI-first设计-v1.0.md` 升版 v1.1（§7.1/§7.3/§8.1/§13/§14/§15）、`docs/阶段沿革.md` 追加二百四十八节、`docs/Berry平台-剩余缺口.md` 核对（无远程访问/attach 相关行，未改）、[implemented 笔记一篇](../notes/implemented/2026-11-13-vnc-desktop-suite.md)；`node .agents/tools/check-docs.mjs` 报告模式零告警。

实施偏差（均已在代码注释或本节说明理由）：其一，U7 计划命题 `rev(DES(k,p)) == DES(rev(k),rev(p))` 在标准 MSB-first DES 下数学不成立（实测证伪），落为两条真命题测试替代：IP/FP 置换互逆（钉置换表）与「VNC 应答 = 位反 key 的标准 DES」（钉 quirk 本体，probe f3 的 openssl 通道同构）。其二，`resolve_launch` 初版让空 spec 的显式 launch 短路了 `BERRY_VNC` 兜底，已改为 spec 未给时由 env 填充；另加 `validate_flags` 三个入口预检，让非法 spec 在 host 子进程 spawn 之前以 E_USAGE 与真实 hint 露出（后台 host 的 stderr 是 null，不预检会被「host exited before announcing run」掩盖）。其三，控制面错误透传补强：`serve` 的错误应答补 `hint` 字段、客户端 `decode` 保留服务端退出码与 hint（此前一律压成 code 6），`vnc passwd` 在无认证端点上的 E_USAGE + 重启 hint 依赖此修复；该改动使所有控制面错误的 CLI 退出码与服务端一致（行为收紧、加法字段，旧客户端不受影响）。其四，meta.json 增记 `vnc_ws` 原始串且 `restart_with` 继承（计划字段清单只列 `vnc_spec`/`vnc_listen`，功能规格第 5 条的「保持 vnc 暴露」覆盖 ws，故补记）。其五，`LAUNCH_STICKY` 进程粘性副本：同进程内首次 `Device::start` 消费 launch 后，后续同进程启动（shell 窗口 RES 菜单的 host 内切档重启）仍继承最初 `--vnc` 配置，弥补 window.rs 冻结期间该路径的暴露保持。其六，v5 的画面差分佐证在 console 档无意义（该档输入不驱动画面），双路验证落在传输层：外部 RFB PointerEvent 与内部 `berry tap` 并发无干扰、帧面可取。

真实遗留（移交后续）：

- 验收标准第 5 条「人工验收一次标准客户端」未留 GUI 客户端实录——协议面已由纯 python RFB/ws 探针与 `vnc check` 全握手代验（v2/v4/v5），macOS 屏幕共享/TigerVNC/noVNC 浏览器实录留用户侧随手补。
- Windows 实机未验（对外端点走同一 TCP 代码路径，本刀只在 macOS 实测）。
- `tools/berry/src/window.rs` 冻结期间 shell 入口经进程级 launch 覆盖（`LAUNCH_STICKY` 粘性副本）布线；该文件解冻后可换 `Device::start` 直参。
- `--vnc 0.0.0.0` 时 `vnc status` 的 uri 回显绑定 host 本身（可连但不宜直接粘贴），如需可拨地址再议。

## 验收记录

2026-11-13 verify 阶段端到端验收（macOS arm64，QEMU 11.0.3 `/opt/homebrew/bin/qemu-system-aarch64`，desktop 档镜像 `out/berryos-portrait.img` + FAT12 数据盘，`just berry-build` 含 docs-guard PASS 后起步）：v1–v7 全过，冒烟 run 全部以 `berry device stop --run <id>` + `berry device cleanup --run <id>` 自清，并行会话的 run（8295-shell、72034-shell）与其进程全程未动。

v1 暴露与状态——通过。数据盘两条命令输出 `mkfat: out/gui-data.img 1048576B vol@LBA16 clusters=506 fat=2x2` 与 1MiB 镜像；`BERRY_DATA_DISK=out/gui-data.img just berry device start --profile desktop --image out/berryos-portrait.img --memory 256 --vnc on --keep` 输出 `device: /Users/murongcheng/.berry/run/1794515326-959738000-52037-host`；`just berry vnc status --json` 报 `{"auth":"none","clients":[],"enabled":true,"has_password":false,"listen":{"host":"127.0.0.1","port":50144,"ws":null},"uri":"vnc://127.0.0.1:50144","ws_uri":null}`；run 目录 `vnc.listen` 内容 `{"host":"127.0.0.1","port":50144,"ws":null}`，meta 报 `vnc_spec:"on"`、`vnc_ws:"off"`、`vnc_listen:{host/port/ws 同前}`，无 `vnc.pass`（环回默认无口令）；QEMU（pid 52051）argv 为 `-vnc 127.0.0.1:44244`（对外在前，display 44244 → 实际端口 50144）再 `-vnc unix:…/vnc.sock`（内部在后），无 `password=`/`websocket=` 字样；`lsof -nP -i TCP:50144` 见 `qemu-syst 52051 … TCP 127.0.0.1:50144 (LISTEN)`；`just berry device status` 输出 `…: running (profile desktop, 256MiB, pid 52051, vnc 127.0.0.1:50144)`。

v2 外部取帧与共存——通过。`just berry vnc check` 输出 `vnc check: ok 1080x1920 via vnc://127.0.0.1:50144 (auth none)`（JSON `"width":1080,"height":1920,"ok":true`），与串口 `WMFOCUSREADY ok left=1 right=2 left_rect=0,0,540,1920 right_rect=540,0,540,1920` 的双面板 1080x1920 几何一致；外部 RFB 客户端（python 挂对外 TCP 14 秒）在场时 `vnc status --json` 的 clients 报 `[{"family":"ipv4","host":"127.0.0.1","service":"50356","websocket":false}]`，同一窗口内内部面 `just berry screenshot --settle-ms 2000 --out out/vnc-smoke-desktop.png` 成功输出 `screenshot: …/out/vnc-smoke-desktop.png (1080x1920)`（PNG 头校验 1080x1920 8-bit RGB）——双端点共存，外部客户端在场不干扰内部面。

v3 口令全流程——通过。`--vnc 0.0.0.0:15944` 起档时 stderr 先打一次性提醒 `berry: note: VNC exposed on non-loopback 0.0.0.0 — classic VNC auth (DES) is weak; keep it on trusted networks`；`vnc status --json` 报 `auth:"vnc"`、`has_password:true`；run 目录 `vnc.pass` 权限 `-rw-------`（0600）、8 字符全字母数字（自动生成）；QEMU argv 为 `-vnc 0.0.0.0:10044,password=on`（字面端口 15944，口令不在 argv、不在日志），`lsof` 见 `TCP *:15944 (LISTEN)`；`vnc check --password wrongpass` 失败，错误行 `berry: VNC: auth failed: Authentication failed` + hint `check the VNC password (berry vnc passwd or the run's vnc.pass)`、退出码 7，且输出全文不含真实口令（grep 比对无命中）；`vnc check`（缺省读 run 目录 `vnc.pass`）输出 `vnc check: ok 1080x1920 via vnc://0.0.0.0:15944 (auth vnc)`；`vnc passwd --set test1234` 后旧口令 check 报同一 AUTH FAILED、`--password test1234` 与缺省读文件均 ok，`vnc.pass` 更新为 test1234 且保持 0600，`events.jsonl` 记 `{"action":"vnc.passwd","source":"control","t":"action","v":1}`（无口令值）；另验环回无口令 run 上 `vnc passwd --generate` 返回 E_USAGE（退出码 2）与 hint `QEMU cannot enable VNC auth at runtime; restart with --vnc-pass to enable it`。

v4 websocket——通过。`--vnc on --vnc-ws on` 起档后 `vnc status --json` 报 `listen:{host:"127.0.0.1",port:50531,ws:50532}` 与 `ws_uri:"ws://127.0.0.1:50532"`，QEMU argv 为 `-vnc 127.0.0.1:44631,websocket=50532`（显式字面 ws 端口，非 websocket=on 的联动口）；纯 python ws 探针对 50532 完成 `HTTP/1.1 101 Switching Protocols`（`Server: QEMU VNC`、`Sec-WebSocket-Protocol: binary`）后 ws 二进制帧内 RFB 全握手：frame1 `b'RFB 003.008\n'`、frame2 security types `[1]`、frame3 SecurityResult u32=0、frame4 ServerInit `1088x1920 name=b'QEMU'`（1088 为 ramfb 64 字节对齐步长，内部截图仍 1080 宽）——noVNC 面可达；挂一条 ws 客户端时 `vnc status --json` 的 clients 报 `[{"family":"ipv4","host":"127.0.0.1","service":"50594","websocket":true}]`。

v5 输入双路——通过。内部路：对外 TCP 挂外部 RFB 客户端期间 `just berry input --run … key ret` 输出 `input: ok (…run…, guest 1088x1920)`、`tap 540 960` 同样 ok，多客户端共存无干扰；外部路：纯 python RFB 客户端经对外端点完成握手（types `[1]`、SecurityResult 0、ServerInit `1088x1920 name=b'QEMU'`）后发 PointerEvent 按下/抬起各一条，客机串口 `gui-inputd: wm … hit=2` 计数变为 `hit=4`（恰增 2 = 两条指针消息均被客机输入守护处理）——外部注入真实到达客机。

v6 重启保持——通过。v1 会话 `just berry device restart --run 1794515326-959738000-52037-host` 后同端口 `vnc check` 仍 ok（`vnc://127.0.0.1:50144`，auth none；`lsof` 同监听，QEMU 同 pid——该动词为 QMP 复位同进程）；v3 口令会话 restart 后同端口 15944、`vnc check --password test1234` 仍 ok（auth vnc）；再以 `--vnc 127.0.0.1:15945 --vnc-pass pw6 --restart-on-crash` 起档，杀自起 QEMU pid 22191 后 5 秒内重生为 pid 29771、`lsof` 仍 `TCP 127.0.0.1:15945 (LISTEN)`，`vnc check --password pw6` 直接成功（重生后 set_password 重注入）、错口令仍被拒——同端口同口令硬保持。

v7 回归——通过。`just berry doctor --json` 报 `"ok":true`（QEMU 11.0.3、hvf、6 profiles）；`just berry-smoke` 三步全过：doctor ok、`marker ok: BerryOS boot`、`shot: out/berry-shot.png (800x600)`，退出码 0；`cd tools/berry && cargo test --release` 报 `39 passed; 0 failed`（既有 21 零回归 + 新增 18）；无 `--vnc` 起档（`--image out/berryos.img --memory 128 --keep`）argv 仅一条 `-vnc unix:…/vnc.sock`、run 目录无 `vnc.listen`/`vnc.pass`、meta 报 `vnc_spec:"off"`、内部面 screenshot（480x1000）与 `vnc status`（`vnc: not exposed`）均正常；五个冒烟 run 全部 stop + cleanup 自清，`~/.berry/run/` 仅余并行会话两档。

verify 阶段小修（tools/berry/src/display/vnc.rs）：认证失败 reason 透传原样带出 QEMU 的 `"Authentication failed \0"`（尾空格 + NUL 填充），CLI 错误行以乱码字节收尾；改为 `trim_matches`（剥空白与 `\0`）后错误行为干净的 `berry: VNC: auth failed: Authentication failed`（od 校验无尾随 \0/空格），正确口令路径与 39 项单测重跑全绿。佐证观察两条（不修）：其一，`cargo build` 有 2 条 dead-code 警告，均在冻结的 window.rs（并行会话未提交改动），非本刀面；其二，`device restart` 后立即 `vnc check` 报固件 GOP 几何（800x600），客机重设 ramfb 后恢复 1080x1920——ramfb 几何由客机侧控制的既定边界（见「边界」节），非缺陷。

docs 阶段（2026-11-13）：全套文档以本节与落码为唯一事实源收尾——`tools/berry/README.md`、`docs/Berry设备平台-CLI-first设计-v1.0.md`（升版 v1.1）、`docs/阶段沿革.md`（二百四十八节）、本计划 `Status: done` 与 implemented 笔记一篇；`docs/Berry平台-剩余缺口.md` 逐行核对无远程访问/attach 相关行，未改；`node .agents/tools/check-docs.mjs` 报「检查 191 篇 / 告警 0 条」。

audit 阶段（2026-11-13）：独立复审裁 partial，四项检查整体过硬，唯一实质发现为低危口令卫生缺口——后台 `device start --vnc-pass <pw>` 曾把口令放进 host 子进程 argv（`ps` 实测可见，QEMU argv 本身干净）；当日已修（`commands.rs` device_start_run）：显式 `--vnc-pass` 改经 `vnc_cfg::write_pass_temp` 暂存为一次性 0600 文件并以隐藏 `--vnc-pass-file` 传递（与 restart_with 同机制），spawn 失败时删除暂存文件；复验：cargo test 39/39 绿，真机起档 `--vnc 127.0.0.1:15966 --vnc-pass smoketestpw` 后 host 子进程 argv 仅见 `--vnc-pass-file ~/.berry/tmp/vncpass-…`、全进程表零口令命中，`vnc check --password smoketestpw` 过（`ok 480x1000 … auth vnc`），`~/.berry/tmp` 无残留，run 已 stop+cleanup 自清；「口令三不出现」自此对后台路径亦字面成立。audit 其余发现（profile.rs/main.rs 桌面档默认改动属并行会话配套、「横屏 1080P」注释笔误同属该会话 hunk）按归属不动，留归属会话处置；docs-guard G10 的未登文档 `docs/显示与加速体系设计-v1.0.md` 亦系并行会话半成品，本刀未动。
