# g3-1d 收官验证证据摘要（耐久档，2026-11-04）

> f4 三镜 p3 回写：PAR/LB 记账原仅存易失 /tmp（f4 档案 /tmp/k3-review/g31d/，
> 随 /tmp 清洗扬失）——本文件为耐久证据摘要（硬门行原文摘录）。原始整日志
> 16 份（KEEP_LOG 全量，各含恰 1 行 `ckpt=1469`，md5 全异——含瞬态段）存
> f4 档案，扬失后以本摘要为准。smoke-smp 钩子面：`SMOKE_KEEP_LOG=1` 八 boot
> 各留档（tools/smoke-smp.sh :109-129 在册；g3-1d 跑未用——smoke-smp 证据走
> 手工重定向归档 f4 档案，后续 smoke-smp 跑建议带之。f4 复审 e1 勘正）。
> 计数豁免注记（2026-11-06 G4-3 收官补）：下文「裸相 27 + rootfs 相 47」
> 为 g3-1d 时点（2026-11-04）真值；G4-1 起 +`DPG ok`（裸相 28）、G4-2/
> C-lite/B-2 起 dpg 三标记（rootfs 50、主 smoke 78）落地后，现役计数以
> `tools/markers.sh` 单源为准——冻结回执原文不追改，本注为唯一勘正面。

## 树态与产物

- 合态树 = 68fea8b + e0ef223 + 900d140 + g3-1d（668da1d + f4a3c14 沿革勘正）。
- 主 smoke PASS @ /tmp/smoke-g31d-final.log（金丝雀 43 严格递增 + 全八相 + exit=0）。
- icount prove 5/5 exact max_dev 0.000%（vmo_commit1p 1574→1572 微布局骰，
  史档 → bench_baseline_icount.notes.md）。
- abi-check PASS ×2（NR v1 34 四面单源）+ docs_guard PASS 0 警。

## smoke-smp 八相硬门行摘录（/tmp/smoke-smp-g31d.log，2026-11-04 20:56 起）

    smoke-smp: 相 1 SMP 握手 + boot 标记 27/27 到齐（墙钟 5s）
    smoke-smp: 相 1 金丝雀行周期 ≈ 0s/行（值 100→300）
    smoke-smp: 相 2 rootfs 相 …（rootfs 相 EVS/SKT client 对拍完成 ×2）
    smoke-smp: PAR ok 提前命中（witness）——PAR ok c0=11 c1=9
    smoke-smp: 相 2 金丝雀行周期 ≈ 0s/行（值 100→2300）
    smoke-smp: 相 3 LB 相：…LB ok c0=0 c1=77 st1=0 n=4
    smoke-smp: 相 4 ASID 回卷相：ASIDR ok n=2
    smoke-smp: 相 5 TLB canary 相：TLBS ok n=10
    smoke-smp: 相 6 ALLOC churn 相：ALLOC ok n=320
    smoke-smp: 相 7 HT 风暴相：HT ok n=320
    smoke-smp: 相 8 PP ping-pong 相：PP ok a=320 b=320
    smoke-smp: PASS（八相：相 1/2 boot 执法面 = SMP ok ncores=2 各恰一次 + SGI ok
      各恰一次（S-3a IPI 收讫链）+ SGI2 ok 各恰一次（S-4 TLB shootdown 回执链）
      + 裸相 27 + rootfs 相 47 标记 + PAR d1 活证 + 金丝雀严格递增；相 3 专项门
      = LB c0=0/c1≥24/n=4 三硬门（S-3b 副核得货活证）；相 4 专项门 = ASIDR gate
      on + n≥1（S-4 4-3 回卷 quiesce 回执链）；相 5 专项门 = TLBS gate on + n=10
      （S-4 4-4 canary 压测双轮 decommit/Drop——stale TLB 被 SGI #2 广播打掉的
      十命中活证）；相 6 专项门 = ALLOC gate on + n=320（S-5b 双核 EL0 分配器
      churn——水位恒等 + 双核 sw 参与证）；相 7 专项门 = HT gate on + n=320
      （S-5b 双核 handle 表风暴——静默点双走查 Ok(0)）；相 8 专项门 = PP gate
      on + a=320/b=320（S-5c 双核 channel ping-pong——单进程双线程对打 + 序号
      零丢失对账）——恰一次/金丝雀执法不跨相 3-8 boot）
    exit=0

## PAR/LB 基线重定口径（g3-1d ①，行为门无数值基线文件）

- PAR gate = 行为门 ok/absent + d1≥1：8cbf832 态活证 c0=12 c1=8；合态复验
  c0=11 c1=9（witness 提前命中，相 2）；d1 活证在八相 PASS 行内。
- LB 三硬门 c0=0 / c1≥24（实跑 77）/ n=4（相 3）。
