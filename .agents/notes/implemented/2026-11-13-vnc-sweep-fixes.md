# berry VNC 三波 sweep 修复（env 感知预检/重启链校验/安全提醒/测试沉淀）

Status: implemented

## Problem

三波落地（[2026-11-13-vnc-desktop-suite](2026-11-13-vnc-desktop-suite.md)、[2026-11-13-vnc-maturity-wave2](2026-11-13-vnc-maturity-wave2.md)、[2026-11-13-vnc-packaging-wave3](2026-11-13-vnc-packaging-wave3.md)）后的四路只读 sweep（[../../research/2026-11-13-vnc三波遗漏检查.md](../../research/2026-11-13-vnc三波遗漏检查.md)）裁出 14 条本刀修条目，承重缺陷同根：`validate_flags` 只吃 CLI 旗标、不感知 `BERRY_VNC`/`BERRY_VNC_TLS`（env 兜底只在 `resolve_launch`），造成两向缺陷——env 提供合法端点时 `--vnc-ws`/`--vnc-tls` 被误拒（rc=2）；env 值非法时绕过 CLI 预检，死在 stderr 已 null 的后台子进程，真实 E_USAGE 丢失只剩 opaque 的「host exited before announcing run」。同族还有三处：`restart_with` 对继承的 vnc 三元组无父侧校验（固定端口路径先毁旧会话才失败且真因不可见）、env 驱动的非环回暴露安全提醒全失（CLI 侧只读旗标、子进程的提醒进 null stderr）、TLS 端点 rc=2 的 hint 首选 `berry vnc web` 与自家 README 的混合内容限制自相矛盾。另有测试沉淀缺口：serve 准入闸/QMP 事件泵/handshake_auth 三个网络路径零自动化、justfile 零 VNC 配方，以及 README 口令 argv 表述绝对化、非 Unix 弱随机未落边界节两处文档漂移。

## Decision

- F01/F03 同修法：`validate_flags` 与 `maybe_security_note` 内部先做 flag‖env 合并（`merge_flag_env`：旗标优先、env 只补缺，与 `resolve_launch` 同序），spec 解析与 ws/tls 的 requires-端点判定、tls 目录校验、安全提醒全基于 effective 值；纯核拆 `validate_effective` 供单测（避免测试改进程 env 与其他用例竞态）。
- F02：`restart_with` 在 stop_old/start_new 之前对继承三元组调一次 `validate_flags`（吃 effective 值，与子进程实际行为一致——子进程继承父 env，env spec 会在其 `resolve_launch` 兜底生效）。
- F12：TLS hint 以 VeNCrypt 客户端（TigerVNC）/外部 TLS 探针为先，`vnc web` 降为说明性后置（http 页面连不了 wss），commands.rs 与 display/vnc.rs 两处同族 hint 一并对齐。
- F08-F10 测试沉淀：`serve()` 拆 `serve_listener(listener, root, max_conns)`（绑定后委托，行为零变化），单测绑 127.0.0.1:0 起真 accept 循环，空连接打满 max_conns 断言 503、断开释放后断言重新放行（ConnSlot 递减回归网）；qmp.rs 起假 QMP 服务器（脚本化 greeting→capabilities→异步 VNC_CONNECTED→id 应答）覆盖握手协商/事件泵 sink 派发/execute 路由；display/vnc.rs 起假 RFB 服务器钉 handshake_auth 三态（type 1 无认证/`--password` 对开放端点报 auth none 真相、type 2 挑战响应用 des 模块对拍、错口令路径钉「尾随空格+NUL 清洗」的 wave1 live 修复与 rc=7）。
- F11：justfile 新增 `berry-vnc-smoke`（依赖 berry-build）：`device start --vnc on --vnc-ws on --vnc-pass <pw> --json` 取 run id → `vnc status --json` 断言 uri/ws_uri → `vnc check` 三态（缺省读 vnc.pass / 对口令 / 错口令期望 rc=7）→ trap 自收（显式 `--run`，绝不命中并行会话的「最新 running」）；TLS/wss 与 ws 握手引外部依赖不进配方。
- F04/F05 文档：README 口令表述收窄为「不进日志/错误信息/host 子进程 argv」并如实写 shell/host 长驻进程 argv 可读；边界节新增非 Unix 弱随机条目（xorshift 回落材料可预测，建议显式 `--vnc-pass`，CSPRNG 代码臂留后续）。
- F13/F14 卫生：wave3 遗留 failed run（`1794554323-…-35554-host`，含明文 vnc.pass + 320M 盘件）以显式 `--run` stop+cleanup 补清，wave3 计划 v5 段落补偏差句；`~/.berry/tmp/vncpass-68812-…` 一次性交接残留删除。
- 顺手带两条 P2（均一行、同属 VNC 面）：qmp.rs greeting `recv_timeout` 超时路径补 `writer.shutdown()`（否则 QmpClient 未构造、Drop 不跑，reader 线程 + fd 每次超时泄漏一份）；main.rs 三处 `--vnc-pass` help 注明「无端点时忽略」（与 ws/tls 硬 E_USAGE 的一致性 nit，取 help 注记分支）。

## Alternatives considered

- F01 修法三选一：改三处调用点各自传 effective 值 vs 在 `resolve_launch` 后校验 vs 在 `validate_flags` 内合并——调用点改法重复三遍且 `shell`/`host` 路径要重排时序；resolve_launch 后校验救不了「起子进程前失败」的设计意图（后台模式下 CLI 进程才拿得到错误面）；函数内合并最小且三调用点同批受益。选函数内合并 + 纯核拆分。
- F08 测试用 mock 计数器 vs 真 TCP 空连接——准入/递减已经各有一个纯函数单测，回归风险恰在「线程 + 锁 + accept 循环」的组合面，mock 计数器测不出 ConnSlot 的 Drop 语义。选真 listener（绑 127.0.0.1:0，CI 可跑、零 QEMU）。
- F04 修法：shell/host 也走暂存文件 vs 改文档——shell 入口经进程级 LAUNCH_STICKY 且 window.rs 冻结中（已知边界），动它跨会话属地。选先改文档如实收窄。
- F17 修法：补硬 E_USAGE vs help 注明 ignored——`--vnc-pass` 无端点时静默忽略不构成安全或行为缺陷（没有端点就没有口令面），硬拒会把 `BERRY_VNC` 兜底场景下合法的冗余旗标变成使用错误。选 help 注记。

## Consequences

- env 驱动暴露与旗标暴露在预检、安全提醒、错误可见性三面完全同权：`BERRY_VNC=zzz` 与 `BERRY_VNC_TLS` 坏目录现在都在 CLI 侧以真实 E_USAGE + hint 失败（live 验证 rc=2），`BERRY_VNC=on --vnc-ws on` 不再被误拒，`BERRY_VNC=0.0.0.0:5971` 后台模式打出非环回安全提醒。
- 重启链（`display set --restart` / attach RES）在继承的 TLS 目录等失效时先于「毁旧起新」失败，真因（missing ca-cert.pem + certs hint）直达调用方。
- 单测 58→64（web 准入/qmp 泵/RFB 三态/值合并六件），`just berry-vnc-smoke` 实跑全绿并自收；VNC 面从此有 CI 可跑的网络级回归网与一条可重复的冒烟链。
- 未修面维持 survey 记录：F16（passwd 轮换后 expire 复位失败被吞）、F18（web 连接总预算）、F20（vncpass 孤儿清扫机制）仍为 P2 记录项；F15/F17 已顺手带上。
