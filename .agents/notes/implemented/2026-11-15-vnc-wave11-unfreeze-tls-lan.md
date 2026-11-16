# VNC 第十一波：解冻收尾 + TLS×LAN——LAUNCH_STICKY 直参化/TLS run 的 --lan/TPN 钉行/工具栏 WEB 键

Status: implemented
Related: [VNC 第十波：LAN 远程桌面](2026-11-15-vnc-lan-wave10.md) ｜ [VNC 桌面全套（第一波）](2026-11-13-vnc-desktop-suite.md) ｜ [wave11 计划](../../plans/2026-11-15-vnc-wave11-unfreeze-tls-lan.md)

## Problem

wave1 冻结期给 `window.rs` 绕道的进程级 `LAUNCH`/`LAUNCH_STICKY` 覆盖（`set_launch` 写一次性 LAUNCH + 同进程粘性副本，`Device::start` 内 `resolve_launch()` 消费）在 window.rs 解冻后成了纯历史债——隐式全局状态传递 VNC 旗标，同进程二次 `Device::start`（RES 菜单/ROTATE）靠粘性副本继承。同时 [wave10 笔记](2026-11-15-vnc-lan-wave10.md) L28 登记的 TLS×LAN 重启条件欠账：TLS run 上 `vnc web --lan` 硬拒 rc=2（QEMU ws 端点环回 wss-only，明文管道无上游；手机侧自签证书墙打死 https 页形态），重启条件 =「rustls 客户端终止上游 + http 页」。另有 wave10 audit 观察的 check-appdir.sh TPN 检查漏钉 qrcodegen 行（单源 L13 已有实物）。

## Decision

1. **直参化**：`Device::start` 追加第八尾参 `vnc: vnc_cfg::Launch`（自带 `Default`，全 None = off，不包 `Option`）；shell 构造 Launch 经 `window::run(args, vnc)` 直传，window.rs 三调用点（首起传 clone、RES/ROTATE 传 `self.vnc.clone()`），`App` 增 `vnc` 字段显式重传；sticky 五符号（两 static + set/take/sticky_launch）整体删除（全仓零测试钉、零残余消费点），`resolve_launch()` 改 `resolve(vnc: Launch)` 收参，BERRY_VNC/BERRY_VNC_TLS env 合并逻辑逐字保留。
2. **TLS×LAN**：`--lan` 分支前置于 tls_dir 平面；TLS run 上页面维持明文 http 0.0.0.0 单口（URL/QR/输出与明文 `--lan` 逐字节同构），仅 `/ws` 代理上游腿换 rustls 客户端连 QEMU 环回 wss——`web.rs` 新 `LanTlsUpstream { cfg, name, dir }` 挂 `LanUpstream.tls`，`ProxyConn` 加 `Plain` 腿（deframe/send 直通泛化），新 `proxy_ws_tls` 复用 `pump_direction` 双线程泵与 OS 拆链语义；证书语义零新面（复用 `check_tls_client_config`：`--ca` > run meta `vnc_tls` > rc=2 certs hint；`--insecure` 警告后 AcceptAll，与 `--ca` 互斥 rc=2）；`lan_admission` 矩阵 TLS run × 有口令行 Reject→Allow（上游形态由命令层选），`--tls` 互斥拒与无口令拒逐字不变；JSON `lan` 对象增 `upstream: ws|wss` 单键。
3. **TPN 钉行**：check-appdir.sh TPN 段第四对 grep `^qrcodegen 1.8`（rustls 行同型），正例 pass/负例 fail 实跑后就地还原。
4. **WEB 键**：工具栏六键化（label WEB/tip BROWSER，W/E/B 字模已在零新增），点击 spawn `berry vnc web --open` 后台子进程（明文 run 打印环回 URL、TLS run 自动落 https 双口环回形态）；App 持 `web_child` 句柄——存活二连点幂等拒、RES/ROTATE 会话重启与退出时 kill（旧 run 端点随 run 死亡，新 run 需重点）；口令不进子进程 argv（`--open` 剥 fragment 先例）。

## Alternatives

- **直参形态**：参数结构体（StartOpts）重构 vs 追加单参 `vnc: Launch`——两者调用点同步数相同（4 处），追加参数 diff 最小且不动既有 7 位置参语义；`Option<Launch>` 亦弃（`Launch::default()` 即 off，再包一层是空语义）。
- **sticky 退役口径**：整体删除 vs `#[cfg(test)]` 保留——零测试钉 + 消费点全部随直参化改写，保留即死代码；语义差异场景（绕过 App 直接二次 `Device::start` 指望 sticky）全仓无调用点，删除等价。
- **TLS×LAN 三案**：硬拒维持（手机侧需求悬置）vs https 页绑 0.0.0.0（手机自签证书墙打死，wave10 契约 6 依据仍立）vs http 页 + rustls 上游（入选——LAN 腿暴露面与明文 `--lan` 同构、手机侧零新墙、上游链双重活体实证）。
- **WEB 键裁决**：无条件做 vs 砍 vs 尾项 + cut-line（入选）——window.rs 基线后零活跃、六键化零测试改写故技术上可留，但它是本刀 window.rs 最大单体改动且无显示环境只能降级验证，故排序直参化/TLS×LAN/TPN 之后，cut-line = M 态重现/前三项门未全绿/时间超限（实测三项全绿且 M 未现，做成）。
- **WEB 键语义**：绑 `--lan` vs 环回（入选环回）——桌面窗口的自然受众是本机；LAN 形态的口令/QR 语义不进窗口按钮。

## Consequences

- CLI 用户面零变化（旗标/env/输出全照旧）；既有 VNC 起档/重启/继承/attach/crash-restart 路径行为零回归（118 单测 + 三冒烟证）。
- TLS run 的 LAN 暴露面如实 = 明文 http 页 + VNC DES 端到端，TLS 只加在 berry→QEMU 环回腿——note 行与文档措辞不宣称「LAN 加密」。
- `ProxyConn::Plain` 腿令 wss 代理的泵机械与 LAN 明文/TLS 上游三形态共享同一 `pump_direction`（单一拆链语义）。
- WEB 键真点按在无显示环境不可验（单测六键几何 + 代码走查降级登记，不虚报验证层级）；换分辨率/旋转后需重点 WEB 取新 run 端点（服务随旧 run 死亡，幂等门随子进程消亡）。

## Related

实现面：[tools/berry/src/vnc_cfg.rs](../../../tools/berry/src/vnc_cfg.rs)、[device.rs](../../../tools/berry/src/device.rs)、[commands.rs](../../../tools/berry/src/commands.rs)、[window.rs](../../../tools/berry/src/window.rs)、[web.rs](../../../tools/berry/src/web.rs)、[lan.rs](../../../tools/berry/src/lan.rs)、[main.rs](../../../tools/berry/src/main.rs)、[check-appdir.sh](../../../tools/berry/packaging/linux/check-appdir.sh)；文档面：[CLI-first v1.8 §8/§13/§14](../../../docs/Berry设备平台-CLI-first设计-v1.9.md)、[berry README](../../../tools/berry/README.md)、沿革三百四十一。wave10 笔记 L28 重启条件本文销账（旧笔记不改写，Curia 纪律）。
