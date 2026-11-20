# Agent Note: RUN-K1 key 身份与生命周期性质

Status: implemented — 2026-11-12

## Problem

`Ui::apply_desc` 一直是整树重建：NodeId 与 generation 不跨 apply 存活，focus、SEM、flex、animation、dependency、press 与 a11y 侧表被整清。`ViewSpec.key` 虽已在 AST 透传，但没有调和语义。后续 IME、semantic action、theme event、navigation 与 layout v2 都会频繁改树；若 apply/reset/remove/focus 的生命周期性质没有机器夹具，每刀都会重复引入同类回归。

## Decision

落地 `Ui::apply_desc_keyed(src) -> Result<ReapplyStat, AstError>`：

```text
Pass 0 临时 arena 解析 source，非法源不修改当前树
Pass 1 只读计划 key 匹配 / 移动 / 更新 / 增删与容量
Pass 2 先经既有 remove 清理删除子树
Pass 3 matched 保 slot+generation，added 走 Ui::add
Pass 4 source a11y 按 matched NodeId upsert，不清运行期 a11y
```

key 身份域固定为 `(parent, key)`；同父非零 key 唯一，同 key 改 ViewKind loud reject，key=0 永不匹配。`SETTLE_DERIVE_MAX=3`，当前实现单轮调和，第二次应用同一 AST 为 clean no-op。大 churn 自动走 `apply_desc` full-rebuild oracle。

同时固化 `WidgetContext::for_ui(theme, &ui)`，六个 BerryUI app 不再手抄 viewport；gui-wmd 拆为 host lib 与 Linux guest bin，`WmIo` 注入时钟/发送/关闭/诊断/trap，host 可重放窗口、R3 config、AVOID_SYS 与 P4 HIDE/SHOW 矩阵。

## Alternatives considered

- **直接继续 LAYOUT-L5 / IME / SEM 协议**：这些刀都消费树身份与焦点生命周期，跳过性质夹具会把同一回归分散到后续每刀；被否。
- **所有后置项合成一次 GUI 大重构**：违反小步可验证与路线出口门纪律；被否。
- **同父重复 key last-wins**：调和结果依赖兄弟序，身份歧义不可测；被否。
- **kind 改变时静默 remove/add**：Node 身份表面保留语义却被破坏；被否。
- **keyed 路径整清 a11y**：会抹掉 widget/app 在树建立后的运行期登记；改为 source upsert + removed subtree 清理；被采纳。
- **为 gui-wmd 造假 channel 或第二策略副本**：host harness 会变成第二权威；改为 lib/bin 双面 + WmIo 供给缝；被采纳。

## Consequences

BerryUI 仍保持 no_std、零分配、零 panic 与 AST/wire 零漂。`apply_desc` 保留为 full-rebuild oracle；guest 本刀不调用 keyed 路径，不新增 UITEST 标记或提交帧。运行期 focus/sem/flex/anim/dep/a11y 随 matched NodeId 存活；removed subtree 仍由既有 remove 链清空。gui-wmd guest 逻辑仅迁移 I/O 供给面，策略表与所有 guest 标记保持零漂。

Final gates on the exact shipped image:

```text
cargo test -p berry-ui             PASS
just test-host                     PASS
just check                         PASS
berry-ui no_std / guest-font-vmo   PASS
uitest no_std                      PASS
gui-wmd host tests 6/6             PASS
gui-wmd musl guest check           PASS
GUI8 self-test                    PASS
GUI8 normal-boot                  PASS
GUI8 dual-boot canonical persistence PASS
smoke-image 138/138                PASS
cargo deny licenses                PASS
docs_guard / Curia strict         PASS
writable_globals                  PASS
```

Final budget:

```text
UITEST.bap 101,648B
rootfs.cpio 33,923,776B
rootfs0 33,554,432B
rootfs1 369,344B
total headroom 100,293,952B
```

Final image SHA-256:

```text
7881a3114272d789ee01d952d785e61d2017c4854f5fd25a18097544d45608d4
```

## Related

- [BerryUI-RUNK1KeyedReApplyLifecycle-v1.0.md](../../../docs/BerryUI-RUNK1KeyedReApplyLifecycle-v1.0.md)
- [GUI架构协议完善与Rust实施计划-v1.0.md](../../../docs/GUI架构协议完善与Rust实施计划-v1.0.md)
- [GUI框架地基路线-v1.20.md](../../../docs/GUI框架地基路线-v1.20.md)
- [架构决策.md](../../../docs/架构决策.md) ADR #85
