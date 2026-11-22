# 遥测 memstat 与 free 内建刀（0x84 op=1＋MemstatRec＋bsh free 第 24 内建）

Status: implemented

一句话说明：owner 令「彻底实施」续的 v3 遥测首件——规格轮 wf_c13f7443 + 实施轮 wf_ee2a9d63 + 主线愈合修复三段式；内核 0x84 op 域 add-only 扩 op=1 memstat（MemstatRec 五域 40B：total/reserved/alloc/free/committed——free 派生使 total==alloc+free+reserved 构造闭合）、buddy reserved 对称水位补真源、vmo committed 全局聚合原子（三写点全量）、telface 形状单源（消内核/用户双写）、bsh free 第 24 内建（CLI ok n=12→13）；**愈合批**：活线合并曾丢作业控制刀 bsh 面（−453 行＋三根接线，14bf83943 在祖先链但内容被单边取材回退——与 vmo.rs 丢 F0-3/F0-4 同型丢失病），本刀同批再嫁接复通（bsh/Cargo 依赖/user members/justfile 行/bsh 主体四件）。

## 裁决与判例

- **活线丢失病判例（本刀最大发现）**：合并对单文件取旧 blob 可在「提交在祖先链」的前提下静默回退特性——`git log A..HEAD -- file` 不显 merge 内 diff（无 -m），判据=blob 直比（14bf83943 vs HEAD bsh 差 −453/+23 且 +23 全为旧时代措辞回写零真增量）。bsh jobs 面与 berry-jobs 三接线全丢、crate 成孤儿；四刀其余面（哑终端/termio/箭头键/env 链/DUP 位）全审计存活。愈合=bsh 两文件 checkout 本刀版本＋三接线复通＋free 面按恢复版移植（⑪→⑫、n=12→13）。**D2 并行发现同病**：origin/wip vmo.rs 丢 F0-3/F0-4 contig/global 面（该 lane 的债——并流回时五处 committed 水位写点必须同批接上：commit_global 逐页 +1/decommit_global −1/contig 建账臂 +npages〔Some(pa)×npages 口径〕/Drop contig −npages/MMIO 零动作，ktest 因果差分届时扩 contig 相位）。
- MemstatRec 五域定谳（挂账四域案＋reserved 闭强恒等式）；op 常量与形状单源 telface；断言分治（ktest 静止窗强式含因果差分 / bsh 运行窗弱式——committed 与 alloc 分窗读禁强式）；op=0 klog 四负例逐字节保全（match 化保序保码）。
- 禁重复记账源论证（可观测性 §3）：PageLedger 仍是逐页唯一事实源，聚合原子=物理执行臂上的全局水位投影（buddy.allocated 对位图同构先例）。

## 验证

- 宿主：berry-mm 56（含 reserved 对称双钉）·berry-telface 25（形状 40B 断言+常量钉板）·berry-jobs 14（愈合复通过）·kernel cargo build 零 error·bsh musl 零本刀警（6 条全基线存量）；D6 ktest 静默盖（恒等式强式+因果差分+负例保全）。
- 客机：boot smoke 与 free 端到端随 D9 主线链（低载窗）；klog op=0 回归由 boot 链 dmesg 面随套。

## Related

- [遥测 memstat 与 free 内建刀实施设计](../../plans/2026-11-19-遥测memstat与free内建刀实施设计.md)
- [2026-11-19-bsh作业控制v2刀](2026-11-19-bsh作业控制v2刀.md)（愈合源刀）
- [可观测性设计-v1.0](../../../docs/可观测性设计-v1.2.md) §3/§5（禁重复记账源律与 op 域 MINOR 版本律——本刀 op=1 即 MINOR 滚版点，abi.toml desc 与 ABI 文档行随 D8 滚）
