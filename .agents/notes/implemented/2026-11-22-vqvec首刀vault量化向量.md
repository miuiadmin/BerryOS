# ai-runtime §四.1 首刀：vault 定点量化向量基元 berry-vqvec

Status: implemented

一句话说明：E lane R6（DAG `ai-runtime §4` 有界片段）——契约四.1「HNSW 定点量化向量索引」的算术底座落地：新 crate `user/berry-vqvec`，每 vec 单 f32 scale 的 i8 标量量化（非对称——库侧省 4× 内存走整数域、查询侧保 f32）+ 非对称距离（内积 = scale×Σ 顺序累积、L2² 重建域）+ 全零 vec scale=0 无除零退化；round-half-away-from-zero 与 floor 均手写位实现（core 无 round/trunc），量化与距离全顺序累积确定性（HNSW 建图可复现的底座）；host 8 测。HNSW 图结构、FBE 加密与 `VaultQueryCap` 门控归 berry-vault 服务刀。

## 裁决与判例

- **每 vec 单 scale（对标量量化 SQ8 的收窄首片）**：一行 scale 元数据换满域 i8 覆盖，乘法在距离入口一次性展开——比 per-dim scale（SQ8 常形）少一趟元数据与回查；精度损失在 vault 语义检索的容差内（rank 序保真测试钉板）。`A: (run decision, 2026-11-22) per-vec 单 scale 先行（更简、元数据省 4×；per-dim/PQ 归 Berry 需要时升格）. Reversal: quantize 单函数改。`
- **浮点半点域教训钉板**：round-half-away 断言只在半点可精确表示时成立（0.5×127=63.5 ✓、0.635×inv 落 63.4999… → 63）——测试夹具选可表示域，注释写明。
- **round/trunc 手写位实现**：core 无 f32::round/trunc（std 面）——`trunc_bits` 尾数抹零式 floor（非负域）+ 对称臂 round_afz；量化幅度域（≤127）远小于 2^23 精确界，域内逐位确定。

## 验证

`cargo test --manifest-path user/Cargo.toml -p berry-vqvec --target aarch64-apple-darwin` 8/8 绿（round 半点离零正负对称、可表示域量化精确钉板、全零退化两距离面、重建逼近 |v|² 容差、显式精确域 dot/L2²、长度/scale 错误三态、quantize 长度失配 loud、近邻序保真）；全量门禁随批（test-host/docs-guard/facts/check-docs）。

## Related

- [ai-runtime 总体设计](../../plans/ai-runtime/design.md)（契约四.1 本刀兑现；HNSW 图/FBE/门控建造清单挂账——后者涉 security/svcmgr 面入 owner 边界队列）
- 姐妹件：berry-aiproto（§一.1）/berry-tensor（§二）/berry-kvpool（§三.1）——airtd 四件纯语义基座齐（格式/算子/缓存/向量），运行时与 vault 服务接线归后续刀
