# bcts park 取证自动化：串口日志签名分类器（#257 后继件）

Status: implemented

## Problem

[bcts 步](../../../.github/workflows/ci.yml)（CI 全量回放）失败时，宿主侧只见 evsclient ×10 conn timed out；guest 死因靠 berryos-smoke.* 串口日志尾 30 行人工归因（run 43461288210 / 43470515049 双证：同为 DNS 修后形态，前者 rootfs 45/45 过、后者 rootfs 相零标记纯 churn 停摆——双态 = 调度器是否放行 svcmgr）。artifact 常因配额撞顶缺席，取证据链脆弱。

## Decision

失败取证自动化：`tools/bcts-parkprobe.py` 就地分类 berryos-smoke.* 串口日志，bcts 失败路径自动调用（ci.yml），输出定谳行（dns_park / churn_starve / net_live / clean_boot / churn_after_markers / no_churn_evidence 六分支 + unreadable 负样本）。签名库钉板挂 test-host（justfile，紧随 dns-stub selftest）。退出码恒 0——取证件不参与门禁执法（bcts 主门在 smoke 侧），分类漂移由 --selftest 承担。

判据单源：switches 尾 5 连严格递增 + rootfs 标记零进展 = 调度饿死面（定谳同 [098ca8bf8 研究件](../../../docs/阶段沿革.md)：boot 演示常驻债，修窗五件 owner 域）；`bsh: epl: gai dns r=-2 res=true` = DNS 慢路 park（#257 修前形态，命中即 stub 链回退）。

## Notes

- `clean_boot` 分支防误判：裸相 32 标记齐（PWRDEG ok d=1 锚）+ 无 rootfs 锚 = 正常轮转杀（SIGTERM 换相），此日志无罪。
- 搜前查重：.agents/ 内无 bcts park/取证先例（rg 实证），本件为首刀。
