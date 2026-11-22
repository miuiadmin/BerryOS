# ai-runtime §二 Tier 1 客机侧首刀：virtio-compute wire 编解码 berry-npuproto

Status: implemented

一句话说明：E lane R11（DAG `ai-runtime §2` 续片，按 Tier 1 侦查件定向）——客机侧 wire 契约首钉：新 crate `user/berry-npuproto`，设备 config（magic "VNPU"/版本/caps 位/queue_pow2 域检）+ 命令描述符（virtio-gpu 控制头同形 24B + MATVEC/RMSNORM 两 op 载荷，op 值镜像 berry-tensor 语义单源）+ blob 页帧表（off 4096 对齐执法——与 `.bmod` 张量段同律的零拷贝前提）+ 完成环三态（`Unsupported` = 宿主不识 op → 客机降级 Tier 0 的机器依据；`Watchdog` = 契约二.3 看门狗复位）；family 纪律：畸形即 None 不截断、保留位显式为零执法；host 7 测。宿主 Metal 桥与共享页 DMA 归 tools/berry（边界外）。

## 裁决与判例

- **wire 契约首钉即单向门（侦查件裁决落地）**：只钉传输层不钉算子语义——op 枚举值与 berry-tensor 对齐但语义单源在彼；`Unsupported` 完成态把「宿主能力协商」收进 wire 面（客机据此降级 Tier 0，契约降级链的机器形态）。`A: (run decision, 2026-11-22) 首刀两 op（MATVEC/RMSNORM）窄开——后加 op 只增 tag 不破形. Reversal: 枚举单点扩。`
- **控制头 24B 与 virtio-gpu 同形**：设备发现/环协商可复用 berry-virtio 家族的既有形态——自定义设备少一类例外。
- **blob off 页对齐在编解码双层执法**（encode 拒 + parse 拒）：与 berry-aiproto 张量段同一物理事实（4096），跨 crate 不双写数值——两处常量各钉各面（wire 面 vs 容器面），对齐性由注释互指。

## 验证

`cargo test --manifest-path user/Cargo.toml -p berry-npuproto --target aarch64-apple-darwin` 7/7 绿（config roundtrip+四拒/两描述符 roundtrip+截断拒/描述符三拒〔flags/保留段/op 域外〕/blob 对齐与零长执法/完成环三态 roundtrip+域外与 pad 拒/op tag 域）；全量门禁随批。

## Related

- [ai-runtime 总体设计](../../plans/ai-runtime/design.md)（契约二.2 客机面兑现；宿主侧挂账入 owner 边界队列）
- [Tier 1 宿主直通可行性侦查](../../research/2026-11-22-ai-runtime-tier1宿主直通可行性侦查.md)（本刀即其定向件）
