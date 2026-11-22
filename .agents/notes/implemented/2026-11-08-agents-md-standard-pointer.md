# Agent Note: AGENTS.md 版本化条款补 standard 指针（翻 2.4 笔记「不动」后果）

Status: implemented

## Problem

[2.4 升级笔记](2026-11-08-upgrade-curia-24.md)把「AGENTS.md 维护合同的版本化条款不动」记为已接受残留：裁决三处落位（docs 裁决表 + 规范特记 + write-doc 特记）+ 预算 2994/3400 距警线（3000）仅 6 字符。仓主令「彻底处理掉」——只读 AGENTS.md 的 agent 写新跨模块标准时仍可能错落 `名-v主.次.md`，入口文件缺这条规则就是缺口。

## Decision

条款行内自消化，全文件恰好 3000/3400（警线判据 `>`，不触、零 WARN）：删示例 `xxx-v1.1.md` 与冗字（「并」「下次」「。细则」改「，细则」）腾 21 字符，补「新跨模块标准走 `Type: standard`」九字规则。不带指针——发现性归 write-doc 特记与 `docs/README.md` 裁决表（写文档必经），入口只记规则不记路径。

## Alternatives considered

- **接受超警线（>3000 警非拒，硬帽 3400 仍有余量）**：被拒——执法输出留常驻 WARN，密度回归信号被永久污染，警线形同虚设。
- **压缩他行腾字符（如「QEMU 独立验证」泛化为「可独立验证」）**：被拒——动非相关语义换预算，探针（16 串）/锚点（119 行）风险不对称；条款行自有赘肉（示例括号 19 字符）足够。

## Consequences

- AGENTS.md 3000/3400 字符零余量——后续任何入文须先在同条款或邻行腾字符，G6 一步触警。
- standard 发现性三处不变（裁决表/特记/write-doc）；第一份 `Type: standard` 落地时按维护合同登记 `docs/索引.md`，文件名带版本须为头块 `Version:` 前缀。
- `docs_guard` PASS（0 警）、`check-docs --strict` 绿；探针/锚点/迁出目标零触碰。

## Related

- [2.4 升级笔记（被翻转后果的原文）](2026-11-08-upgrade-curia-24.md) · [Curia 2.4.1 同步（沿革一四二）](../../../docs/阶段沿革.md)
