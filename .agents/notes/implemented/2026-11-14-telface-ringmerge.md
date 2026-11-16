# Agent Note: telface ringmerge——遥测双环 vct 归并律单源（W3-Q81）

Status: implemented

## Problem

kernel/src/telemetry.rs 的 per-CPU 双环（切换环/唤醒环）「按新→旧序合并取第 i 条」vct 归并循环在 `last()` 与 `wake_last()` 逐字重复两份（各 ~30 行游标机械），合并律——vct 大者胜、平局取环 0、游标推进 `k[winner]+=1`、命中判 `k0+k1-1==i`、流尽 None——至今无 host 钉板（ktest 只覆盖 boot 相单调样本，平局/流尽/越界分支不可达）。归并序错 = `check()` 单调断言假红/遥测时序面撕裂，且两份重复漂移即两环行为分叉。

## Decision

telface 内分模块 `ringmerge`（域界：lib.rs 钉「值怎么编码」，本模块钉「多环值怎么归并」；环写机械——seqlock 协议、per-CPU 静态、unsafe 解引用——留 kernel 有状态并发面不外迁）：

- `span_cap(total, len)`：环形缓冲有效条数帽律 `min(total, len)`（wrap 丢头几何）单源，kernel `ring_span`/`wake_ring_span` 两点消费。
- `merged_at(i, peek0, peek1, vct_of)`：双流 vct 归并取第 i 条判定律单源（泛型 T + vct 访问器闭包注入——SwitchRec/WakeRec 共用一具），kernel `last`/`wake_last` 委托消费（零第二实现）。
- `monotonic_nondecr`：合并流全量单调校验律，host 侧执法锚（kernel `check`/`wake_check` 断言循环是对合并读的机械控制流，非第二实现——doc 注明不复用）。

host 7 测：帽律全域（敌意巨值/零容量）、交错序、平局取环 0（同刻戳/交叉平局）、单流存活/双空、环内重复 vct 稳定倒序、50 例随机双流 vs 降序排序参考真值对拍、单调律边角。

## Consequences

- 自审抓住 WIP 缺陷两处：①`monotonic_law_edges` 空数组类型推断红（turbofish 钉 T=u64）；②交叉平局测试数据把写入序当 peek 序（`[7,6]` 流内 vct 倒退非法——实跑红抓住，改写入序 `[6,7]` 并注释钉死两序区别）。
- `merged_at` 泛型 Clone 起拷贝（kernel 侧 Rec 均 Copy 值语义，开销一次 memcpy——热路径读面可忽略）。
- 相关：telface lib.rs（W3-Q10 值编码面）；hdlface lifecycle（同款纯投影纪律）。
