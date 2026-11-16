# berry VNC 五六波 sweep 修复（错 CA 负例/冒烟 --ws 覆盖/代理诊断/json 对称/握手总期限）

Status: implemented

## Problem

五六波落地后的第二轮四路 sweep（[../../research/2026-11-14-vnc五六波遗漏检查.md](../../research/2026-11-14-vnc五六波遗漏检查.md)，F25-F41）裁出九条本刀修（P1 五 + P2 四），同根缺口有四：其一，严格证书校验的「真拒绝」无常跑钉子——96 单测全部共用 `tls::test_support` 的 OnceLock 单一证书目录，若 `load_client_config` 退化为全信任测试照全绿，唯一判据是 wave6 的一次性验收记录；其二，六波主功能 `vnc check --ws`（浏览器通路自检）在常跑冒烟门零覆盖，且 `vnc web` 明文模式输出的 hint 行正指向此命令、justfile 注释仍写「浏览器 ws 握手引外部依赖留手工验收」（--ws 已变纯 CLI，陈述失实）；其三，wss 终止代理的上游建立失败被 `let _ =` 静默吞掉——`--tls` 传错目录时页面正常加载、每条 wss 连接静默死、操作者零诊断；其四，`vnc check --ws --json` 不输出 tls 字段（TCP TLS 路径有 `tls:{enabled,subtype}`），JSON 消费者只能靠 uri 前缀区分 ws/wss。另有四处文档计数漂移与一句过时 help（TPN「rustls 系 8 项」实为 6+subtle、AppDir「33 检」现行 37 检、`vnc web` about 绝对化 no proxies、剩余缺口基线行半失真）与 ws 握手无整体 deadline 的理论上界（慢滴服务器可把头阶段拉长至 8192×读超时）。

## Decision

- F28（严格校验钉子）：`tls.rs` test_support 把 openssl 生成拆成 `gen(cell, tag)` 参数化内核，`certs_dir()` 与新增 `other_ca_dir()` 各持一个 OnceLock——第二套独立密钥对、同布局同 CN（名字相同正强调只有密钥对说了算）；`ws.rs` 新增 `wss_wrong_ca_is_rejected`：与既有 `rfb_handshake_over_wss_end_to_end` 同脚本同端点，仅客户端信任另一套 CA，断言 rustls 握手 Err 且错误含 certificate。正负两测成对，任何一侧退化（全信任 / 全拒绝）都会被抓住。
- F29（冒烟覆盖）：berry-vnc-smoke 追加一行 `vnc check --ws --run`（ServerInit 深度不拉帧，约 1 秒）；berry-vnc-tls-smoke 追加 wss 正路（auto cert dir）与错 CA 负例（`vnc certs` 生成第二套目录作 `--ca`，期望 rc=7——TLS 握手失败走 protocol 错误类）；失实注释改写为「--ws 纯 CLI 自检覆盖，页面/浏览器行为留手工验收」。两配方均 append-only，隔离 BERRY_HOME 实跑自证。
- F30（代理诊断）：`serve_wss_proxy`/`proxy_conn` 增 `tls_dir` 参数（仅为诊断携带），上游 connect / 超时设置 / TLS 客户端建立各失败点经 `upstream_fail` 闭包 eprintln 一行（含 upstream 地址与 `--tls` 目录名）再返回 Err；中继期错误维持静默（对端断开是会话正常终点），浏览器侧握手失败不在本条裁决面维持原样。
- F31（json 对称）：`vnc_check_ws` 输出补 `"tls":{"enabled":<listen.tls>}`；wss 内层 RFB 恒明文 003.008（无 VeNCrypt 层），故不设 subtype，注释言明对称性依据。
- F32（握手总期限）：`ws.rs` 增 `UPGRADE_BUDGET=30s` 常量——与 web.rs CONN_BUDGET（F18）同构的 Instant 总期限，在逐字节读循环顶部检查；`handshake` 委托私有 `handshake_with_budget`（测试缝），新测试用 60ms 滴灌 vs 150ms 预算钉「每次读都成功而总期限先到」的语义（且构造上不可能 flake 成成功：预算远小于滴灌总时长）。
- F25/F26/F27/F33（文档四处）：TPN 计数三处 8→6（以 TPN 实文 7 条为准）；AppDir 33→37 检（补 wave6 口径注，CLI-first 的 v1.4 当刻验收记录按裁决不动）；`vnc web` about 改「plaintext runs proxy nothing; --tls on a TLS run adds a local wss proxy」与旗标 help 同语；剩余缺口基线行收敛为 BerryOS.app 半句加时点限定（Desktop.app 未随 img 重建重打包，注明不在基线内）。
- F39（卫生）：`/tmp/berry-w6-a` 空骨架核身（仅两个空目录、0 文件）后删除。

## Alternatives considered

- 错 CA 负例放法：给既有 wss 端到端测试加臂 vs 独立测试——加臂省几行但失败时无法定位是正路还是负路退化；独立测试与正路测试形成显式的一对，选独立。
- 错 CA 断言粒度：断言 rustls 具体变体（UnknownIssuer/BadSignature）vs 只断言 Err 加错误含 certificate——变体随 rustls 版本与链构建路径可漂移（wave6 live 记录是 BadSignature、本地回环脚本下是 unknown issuer 族），安全属性的本体是「严格握手失败而全信任下会成功」，选粗粒度稳定断言。
- F30 打印位置：spawner 线程对 `proxy_conn` 任何 Err 打印 vs 在上游建立点内联打印——`proxy_conn` 的 Err 还涵盖浏览器侧 TLS 失败（随机探针/健康检查打到代理口会刷屏），内联可精确圈定「上游 connect/TLS 建立」语义，选内联（代价是 `tls_dir` 参数穿透两层签名）。
- F32 期限数值：复用调用方 10s vs 独立 30s 常量——10s 与单次读超时相同会让总期限在正常慢网络下先于读超时误伤；F18 CONN_BUDGET 取 30s 的先例（页头阶段预算）语义同构，选 30s。
- F33 修法：整行补时点限定 vs 收敛为 BerryOS.app 半句——Desktop.app 的旧档事实属 F34（重打包择机）台账，基线行带旧档哈希会随重建再次漂移，收敛半句加「不在本基线内」注记最抗漂移，选收敛。

## Consequences

- 严格证书校验的拒绝语义与浏览器通路（ws/wss）从此都在常跑门里：单测 96→98 全绿，两冒烟配方实跑含 `vnc check --ws` 明文/wss/错 CA 三新断言全绿（隔离 BERRY_HOME，run 自收零残留）。
- TLS run 上 `--tls` 目录配错不再静默：每条 wss 连接的上游失败在 stderr 留一行含目录名的诊断，页面加载正常与连接死寂的组合不再是零线索状态。
- `vnc check --ws --json` 消费者可用 `tls.enabled` 布尔区分 ws/wss，无需解析 uri 前缀；TCP 明文路径维持无 tls 字段（裁决只对称 ws 面）。
- ws 客户端握手的最坏时长从「8192×读超时」收敛到「30s+一次读超时」；单次阻塞读仍由调用方超时管辖（与 F18 同构的检查位置语义）。
- 三处 TPN 计数、AppDir 检数、`vnc web` help、剩余缺口基线句与实物/现行口径一致；F34（AppImage/Desktop.app 重打包）、F35-F38、F40/F41 维持记录未动，台账见 survey 的 Fix 线兑现节。

## Related

- 裁决与逐条台账：[../../research/2026-11-14-vnc五六波遗漏检查.md](../../research/2026-11-14-vnc五六波遗漏检查.md)（Fix 线兑现节）
- 第一轮 sweep 修复（F01-F15）：[2026-11-13-vnc-sweep-fixes.md](2026-11-13-vnc-sweep-fixes.md)
- 六波本体：[2026-11-14-vnc-compliance-wave6.md](2026-11-14-vnc-compliance-wave6.md)、[2026-11-13-vnc-complete-wave5.md](2026-11-13-vnc-complete-wave5.md)
