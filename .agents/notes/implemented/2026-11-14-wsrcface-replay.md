# 唤醒源分类面单源 wsrcface（W3-Q28 净增量重放）

Status: implemented

- Type: note
- Date: 2026-11-14
- Related: [2026-11-12-sleepq-per-key-wake-chain.md](2026-11-12-sleepq-per-key-wake-chain.md)

## 一、为什么净增量重放

#277 旧件为 59 文件大杂烩重放（reviewer 2026-11-15 判例：携带 bootpface/schedface/sysvface 等全 lane 面），按指引自现行 base 848a0fb35 单面重做——只含 wsrcface crate + 单源接线 + 台账/沿革/07 口径原位更新。判定律本体与旧件同文直接复用（#394 dmface 同款纪律）。

## 二、单源面

berry-wsrcface 294 行：SLOTS=128/SRC_MAX=0x7F/WSRC_F_* 五分类位常量 + M_* 九源冻结值镜像 + 三谓词（class_of 分类映射/valid 环白名单/slot_index 钳位）+ 8 host 测。kernel telemetry.rs 薄壳 re-export：WAKE_SRC_SLOTS、WSRC_F_*（静态数组与 ktest 引用面承重）、wake_src_class（ktest 引用面保持）三面换单源，五钳位点换单源。HAL WAKE_SRC_* 九值定义不迁（HAL ABI 面——值域参数化钉板，HAL 改值测试镜像锚与 ktest 同红双面执法）。

## 三、host 钉板要点

9 源两列全表（class×valid 交叉）；值域互异 + HAL 冻结值端点锚；256 值全域扫描（分类位域闭合 + 白名单恰 0x01–0x06 连续段）；钳位律 ≤0x7F 恒等 · ≥0x80 钳 127；POWER_KEY 到达-only 三角（有分类位 ∧ 不进环 ∧ 槽可落）；预留段 0x08–0x7E 恒 false + 零分类；位面冻结低 5 位闭合；到达置位门判序镜像。

## 四、判例

POWER_KEY 到达-only：有 IRQ_LINE 分类位但 valid 白名单不收（到达-only 源不进唤醒环）——分类面与环白名单是两个正交判据，勿合并。

census 节号扫描器（.hermes-loop/census.py）字符类漏「零」——「五百零一」截成「五百」，501 实占被报成空号。任何带「零」的中文节数字（501/502/601…）都会被漏扫；用前先补「零」入字符类或手验目标号实占。
