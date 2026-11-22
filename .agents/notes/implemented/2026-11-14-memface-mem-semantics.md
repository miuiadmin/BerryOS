# Agent Note: memface——mem* 整数版语义模型外迁（W3-Q80）

Status: implemented

## Problem

`kernel/src/mem.rs` 的 mem* 整数版覆写（GUI-0「内核 SIMD-free 合同」builtins 半边）全模块 `target_os = "none"` 门控——host 测试面不链接（macOS 下与 libSystem 符号冲突），8 字节块 unaligned 读写、memmove 方向分治、memcmp LE 首异字节定位的语义正确性只有 ktest `test_fp` ③ 与每次 boot 的事实覆盖（间接），无独立 host 钉板。

## Decision

新 crate `berry-memface`（W3 lane face-crate 纪律）：用安全原语（`to_le_bytes`/`from_le_bytes`/切片）重建同构算法模型，四面单源——`block_model`（8B 块切分几何）、`fill_word`（memset 字节广播）、`cmp_first_diff`（memcmp 块内首异定位——LE 装载低地址在低位，升序逐字节比较即 memcmp 符号语义）、`move_split`（memmove 方向分治三态）。kernel mem.rs 三消费点换单源（memset 广播字/memmove 分治判/memcmp 块内定位）；`#[no_mangle] extern "C"` 符号壳与链接期强符号防线、host 面门控全部留 kernel 侧。host 9 测：切分几何全域段和恰等不变量、广播律 256 值全域、首异定位恰界、方向分治边界、memset/memcpy/memmove/memcmp 块算法 vs 逐字节参考模型全量对拍（重叠全窗扫描 vs `copy_within` 真值）、冻结锚。

## Consequences

- 语义锚互指：crate 头注 ↔ mem.rs 头注（GUI-0 合同 builtins 半边）；bcmp 不独立建模（kernel 侧直接复用 memcmp 同款，测试注释钉板）。
- `memmove_model` 签名带显式 `dst_off`（首版 WIP 硬编码 0 致分治判据误判同址——对拍全窗扫描实测抓住，修复后钉板）。
- 相关：[GUI-0 设计](../../../docs/GUI-6实施设计草案-v1.0.md)（SIMD-free 合同上层）；tickface/dtbface 同款单源纪律。
