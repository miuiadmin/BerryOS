# Agent Note: 品牌前缀裁决——berry 单一家族，否决 Be

Status: implemented

## Problem

仓主提议品牌前缀计划（对照 HarmonyOS `Hi` 族），候选 `Be`——需裁决全系统标识符前缀用哪个家族。

## Decision

**berry 单一家族（四形：`berry-` crate / `berry_` ABI / `BERRY_` 宏与常量 / `Berry` 专名），否决 `Be`**。规则正文 → `docs/命名与前缀规范-v1.0.md`（Type: standard v1.0，本笔记只载决策因果）。

## Alternatives considered

- **`Be`（仓主候选）**：被否——①撞 BeOS（Be Inc. 1991–2001，BeFS/BeAPI）历史商标与认知混淆；②ABI 冻结面已是 `berry_`，引入 Be = 新旧两套前缀割裂（HarmonyOS `Hi` 成立的前提是第一天单一家族）；③Be 是 Berry 截断、无独立识别度。
- **`bri` / `br_` 等其他短形**：同 ③——短前缀的收益（少敲几字符）小于两套并存的心智与检索成本。
- **内核 Rust 内部也加前缀**：被否——模块系统天然命名空间（Linux 先例），前缀只在跨边界导出面有意义。

## Consequences

- 全仓 38 个 `berry-` 前缀 crate 与冻结 ABI **零迁移即合规**——立法成本低（成文即生效）。
- 白名单（`bsh`/freestanding mem 族/上游 crate 内部）永不清算；新面由评审拦。
- SDK 五件套（ADR #75–#80 预留）落册时引用本标准；`BERRY_` 宏面从此有了唯一合法形态。

## Related

- [命名与前缀规范-v1.0](../../../docs/命名与前缀规范-v1.0.md)（规则正文，Type: standard）
