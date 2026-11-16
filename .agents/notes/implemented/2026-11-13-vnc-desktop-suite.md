# berry 对外 VNC 桌面全套（第二 -vnc 端点/口令/websocket）

Status: implemented

## Problem

BerryOS 桌面此前只有 berry 自己能看、能点：帧面主路是共享客机 RAM（`ram.bin` 的 `RamSource`），输入走 run 目录 `vnc.sock` 上的 `VncInput`——外部标准 VNC 客户端（macOS 屏幕共享、TigerVNC、RealVNC、noVNC）没有任何接入面，`display/vnc.rs` 的 RFB 客户端也只会 None 认证（遇到带口令服务端直接报错）。需要把 QEMU 已有的 VNC server 能力面向外部打开（对外 TCP/websocket 端点 + 口令认证 + `berry vnc` 管理与自检），同时内部面（unix socket、RamSource、QMP broker、control.sock）零改动。

## Decision

- 端点形态用 QEMU 同进程第二个 `-vnc` TCP 服务，且对外端点必须写在第一个 `-vnc`（id=default）：probe 实测 QEMU 11.0.3 的 QMP `set_password` 只作用于第一个声明的 server，`password=on` 是 per-endpoint——对外认证与内部 unix socket 的 None 认证天然分离，内部面零改动成立。argv 布局因此成为契约：后续改 `qemu.rs` 构造时必须保持「对外在前」。
- spec 与口令策略：`--vnc <off|on|<port>|<host>:<port>>`（端口 5900..65535——单冒号数字是 display 编号语义，实际端口 = 数字 + 5900，`tcp:` 前缀被拒，故 `net.rs` Tcp 臂产 `host:{port-5900}`）布到 `device start`/`host`/`shell` 三入口，env `BERRY_VNC` 兜底；环回绑定默认无口令，非环回强制口令（默认自动生成 8 字符写 run 目录 `vnc.pass`，0600），不提供「非环回免口令」逃生门（安全姿态对齐设计文档 §13）；口令运行期经 QMP `set_password` 注入，不进 argv、不进日志、不进错误信息。轮换（`vnc passwd`）仅对启动即认证的端点可用——QEMU 不能运行期从无到有开认证，如实回 E_USAGE + 重启 hint。
- DES 自含实现：`display/des.rs` 纯 Rust 自含 DES-ECB（标准向量 key `133457799BBCDFF1` 钉板 + VNC key 每字节位反 quirk + 口令截断/补零到 8 字节），不引第三方 crypto crate，workspace 依赖面零新增。
- 重启链：meta 记 `vnc_spec`/`vnc_ws`/`vnc_listen`，口令副本 `vnc.pass` 随 run 继承；同 run 重生（`restart_in_place`、`--restart-on-crash`）硬保同端口同口令（QMP 重连后重新 `set_password`）；跨 run 重启保同 spec 同口令——显式端口改「先停旧再起新」（两 QEMU 并存窗口内同端口物理不可绑定），auto/on 端口新 run 重挑口。
- websocket 走 QEMU 原生 `websocket=<port>` 且一律显式字面端口：`websocket=on` 的默认口是 5700+display，与 display 编号联动、易撞口；裸 ws 无 TLS 可完成全 RFB 握手（实测 HTTP 101 + RFB 3.8 + ServerInit），noVNC 直接可用。clients 查询用 `query-vnc-servers`（数组，可辨 websocket），不用 `query-vnc`（只返回 default 单对象）。
- 布线：`window.rs` 冻结（并行会话未提交改动）期间，shell 入口经 `commands::shell` 设进程级 launch 覆盖（`vnc_cfg` 全局 + `LAUNCH_STICKY` 粘性副本），`Device::start` 签名不动；该文件解冻后可换直参。

## Alternatives considered

- 单 TCP 端点全家共用（内部 unix socket 面直接对外）vs 第二个 `-vnc` 服务：共用可少一个端点，但对外必须 TCP 且认证语义会绑死内部面（ berry 自己的 `VncSource`/`VncInput` 都要走认证）；`password=on` 的 per-endpoint 语义实测允许分离，双 `-vnc` 让对外认证与内部 None 各自独立、内部面零改动，选双服务。
- 内建 noVNC HTTP 页面 vs websocket 透传：内建页面要带静态资产与宿主侧 HTTP 监听（多一条维护面与攻击面）；QEMU 原生 websocket 已实测可达 noVNC（101 + RFB 3.8 全握手），选透传——noVNC 由用户自带，berry 只给 `ws_uri`。
- 引第三方 DES crate vs 自含实现：DES 只为 RFB security type 2 一处，VNC quirk（密钥每字节位反）还要在外再包一层；自含实现约百行、标准向量可钉板、workspace 保持零新增依赖，选自含。
- 口令经控制面自管（berry 做 RFB 认证代理）vs QMP `set_password` 单一真相：自管要多一套认证状态与协议面，且与 QEMU server 原生行为双轨易漂；QEMU 原生认证 + 运行期 `set_password` 注入是单一真相，`vnc.pass` 只是 0600 副本，选 QMP。
- `--vnc-ws on` 用 QEMU `websocket=on` vs 自动挑口后显式 `websocket=<port>`：联动口 5700+display 随 display 编号漂移、易撞口，选显式字面端口。

## Consequences

- 任何标准 VNC 客户端可直连 BerryOS 桌面看画面、发输入；外部客户端在场不干扰内部面（与 `berry screenshot`/`input` 并存实测）。外部 PointerEvent 真实到达客机输入守护（串口 gui-inputd hit 计数恰增）。
- VNC 经典 DES 口令强度有限（有效口令 8 字节、56 位密钥、挑战明文可离线穷举），仅适合开发/内网，不是 TLS 级安全；非环回暴露启动时打一次性安全提醒。
- 客户端发起的 SetDesktopSize 在 ramfb 后端下不被支持（QEMU 11.0.3 静默忽略）：分辨率变更仍走 `berry display` profile 重启；客机发起的几何变更会经 DesktopSize 广播给 VNC 客户端。
- 控制面（control.sock/control.token）与 QMP 不随 VNC 暴露：对外只有 RFB 端点（TCP/ws）。
- 环回无口令 run 上 `vnc passwd` 是 E_USAGE（QEMU 限制，非 berry 拒绝），hint 引导重启开认证；`device restart` 后立即 `vnc check` 可能短暂报固件 GOP 几何（800×600），客机重设 ramfb 后恢复——ramfb 几何由客机侧控制的既定边界。
- 冒烟证据与完整偏差记录见计划文档「验收记录」节（`out/vnc-smoke-desktop.png` 等产物）。

## Related

- 计划（契约/偏差/验收记录）：[2026-11-13-vnc-desktop-suite](../../plans/2026-11-13-vnc-desktop-suite.md)
- 设计（§7/§8/§13/§14/§15，v1.1）：[Berry设备平台-CLI-first设计-v1.0](../../../docs/Berry设备平台-CLI-first设计-v1.9.md)
- 代码：`tools/berry/src/vnc_cfg.rs`、`tools/berry/src/display/des.rs`、`tools/berry/src/display/vnc.rs`（`connect_auth`）、`tools/berry/src/qemu.rs`（双 `-vnc`）、`tools/berry/src/control.rs`（`vnc`/`vnc_passwd` op）
- 冒烟产物：`out/vnc-smoke-desktop.png`、`out/vnc-smoke-regress-default.png`
