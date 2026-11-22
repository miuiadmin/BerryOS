# 遥测槽表读口 op=3 刀与 powertop 雏形（0x84 四档齐装）

Status: implemented

一句话说明：owner 令「彻底实施」续的遥测 v3 第三件——规格轮 wf_fa33a418＋实施轮 wf_e78104bd-d3e＋主线修复三段式；0x84 op 域 add-only 扩 op=3 WakeSrc 槽表纯镜像导出（40B 轻头＋128×64B 十字段平镜像——行号即源值零 src 域；稠密形否决：谓词是策略不入内核＋first_ts≠MAX 陷阱漏 irq-only 槽＋stage 最坏定容不省 .bss）；装配序双不变式（WAKE_ACCT_N 先读 Acquire→逐槽 Relaxed→now_vct 行读毕后取→头最后落笔：Σ行wakes≥acct_n 与 now_vct≥真实行时戳同时成立）；berryctl powertop 雏形（「谁在挡住深睡」v1 判据＝区间频度×浪费率×源排行＋真睡两帧饱和差分＋--json day-one 第二件）——「人无我有」梯队 trace/powertop 双件齐装；ktest 既有 op=3→-95 负例随刀滚 op=4（add-only 自执法活演化）；D3 文档批同刀清偿 op=2 刀四笔文档实债（§9 收口注入＋笔记勘误＋shell设计 v1.2 连文件名滚＋可观测性 1.0.1→1.2.0 三 MINOR 律）。

## 裁决与判例

- **纯镜像 vs 稠密**：纯镜像胜——活动槽谓词（wakes>0||irq>0||no_waiter>0）是 powertop 的策略不入内核 ABI；first_ts≠MAX 谓词陷阱漏 merge_hal/POWER_KEY 的 irq-only 槽（恰是「谁在挡深睡」要观测的面）；稠密 stage 最坏 128 槽全活≈镜像同量级不省 .bss。
- **装配双不变式**（写序「槽先 N 末位 Release」的读者侧对偶）：N 先读使 Σ行wakes≥N 恒立（写侧槽写永远先于 N 递增）；now_vct 行读毕后取使锚≥真实行时戳（CNTVCT 单调）——哨兵域排除（审查 note 修）。
- **一致性诚实分档**：逐字段独立原子读非一致快照（单调族 fetch_add/min/max 无回退无撕裂；非单调族 last_woken_id/last_ack_ts 可旧可新不撕；跨字段错位可能）——wake_src_snapshot 单槽先例扩全表；稳定窗/generation 维持挂账。
- **trace_off 表照出**：on_wake 槽写在早退前（观测计数≠环记录面）——无空头形。

## 验证

- 宿主：telface 39 测绿（quad 扩面/row·hdr shape_pinned/round-trip/满载算术）；kernel 零 error（cross-const 十字段偏移锁步编译期执法）；berry-sys musl/none 双绿；berryctl musl 绿；canon 91 标记×57 新文案双向零碰撞；ktest K1-K4（满表 pristine＋保留段播种字节对拍＋len 阶梯＋用毕即清）随 bin 编译过——实跑挂账 e2e 窗。
- D3 清偿四笔：§9 收口注入（op=2 实债）、op2 笔记勘误、shell设计 v1.2 连名滚（trace/powertop 现役注）、可观测性 1.0.1→1.2.0（§5 op 加号=MINOR 律×3）、abi desc 四档、双文件改名全仓 33 处入链改指。

## Related

- [遥测槽表读口 op=3 刀与 powertop 雏形实施设计](../../plans/2026-11-19-遥测槽表读口op3刀与powertop雏形实施设计.md)
- [2026-11-19-遥测环读口op2刀](2026-11-19-遥测环读口op2刀.md)（op=2 面＋本刀清偿其文档债）
- [内核唤醒记账设计-v1.0](../../../docs/内核唤醒记账设计-v1.0.md) §9 收口注入
