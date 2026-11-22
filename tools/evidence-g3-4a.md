# G3-4a 证据档：bench-lat 基座 + IPC 路（wake→run 段补盲之一）

> 刀：G3-4 延迟基准+双跑（草案 §4）第一段——测量基座（Task.wake_vct 戳 +
> wake_stamp_take）+ IPC 路（内核任务对）+ bench.py --lat 通道。
> SGI 路（EL0 用户对）与 trace 开/关双跑归 G3-4b/c。日期：2026-11-05。

## 侦察定谳（草案 §7 侦察项「唤醒源构造」收口）

- **IPC send 路 = 内核任务对**：rt_probe 确定性同款（boot 窗门控 spawn、
  直调 block_current、零 SVC/EL0 噪声）；唤醒面 = `wake_matching` 直驱
  （IPC send 尾的唤醒面本体——send 在 wake 前的拷贝/入队机件不在被测的
  wake→run 段内，直驱 = 段的隔离测量）。
- **SGI 路 = EL0 用户对（PP/storm_spawn_fleet 先例形），内核任务对架构堵死**：
  `place()` 内核任务恒 CPU0（内核串行纪元不变量）+ CPU1 出选 = user_only 臂
  （pinned!=0）——内核 sleeper 入 CPU1 队恒不被选出（挂死面）。跨核对只有
  EL0 用户线程（pinned+spread）可落 CPU1。
- 落点：G3-4b 以 storm_spawn_fleet writer_n=1 形（root 家核 + 写者
  spread→CPU1）承载 SGI 路；测量载体 = wake_vct 戳 + wait 返程尾读点
  （本段已落的 wake_stamp_take 基座直接复用）。

## 测量口径

- **wake 侧锚**：`note_wake` 无条件戳 `Task.wake_vct`（cntvct；置因窗内，
  与 wait_since 同窗纪律；不随遥测钩子在否漂移）。
- **run 侧读点**：醒来续行锁窗内 `wake_stamp_take()`（取走即消费）+ 立即
  cntvct——读点 = 醒后首段内核执行（rt_probe wake_vct 读序同族，F33/F42
  分工：读点在重取锁后、零 tick/切换噪声混入）。
- 样本 = now − 戳；负值 panic（L0，R-0 on_wake 负延迟同族）。N=32 全排序
  精确分位（rt_probe 形；R-0 直方图形态的精确化同族先例），p99==max =
  32 样本最近秩的诚实结果。
- **行形**：`BENCHLAT,lane=ipc,n=32,min=,p50=,p90=,p99=,max=`（BENCHRT
  六字段族）+ 完成标记 `LATIPC ok n=32`。行嵌矩阵流中段（prio 8 抢占
  打印）——BENCHRT 同款非锚定收割（首跑实录坐实）。
- 门控：fw_cfg `opt/berry/lat`（rt 同形）；缺席 = run/smoke 零扰动。
- L0 执法面（全在内核侧）：醒因恒 Signaled（Timeout = waker 漏醒）、
  唤醒数恒 1（sleeper 未在睡 = 握手竞速破损）、戳非 0（note_wake 戳面
  破损）、负 wake→run panic。bench.py 只出表不对拍（R-1 公约 2 同族）。

## 实跑数据（本机 mac，QEMU 11.x TCG，-smp 2；绝对值跨机不可比——B-2 纪律）

> **环境声明〔f4 F7〕**：段① 首锚 = **单路 boot**（lat 门下仅 IPC 对在树；
> SGI 对未在树）——段②起 lat=1 同 boot 起双对，单路环境不再复现，
> **跨段数值不可直接互拍**（对拍须同段内跑）。

| 跑 | min | p50 | p90 | p99 | max |
|----|-----|-----|-----|-----|-----|
| 1（手跑 90s 窗） | 875 | 1937 | 3000 | 4500 | 4500 |
| 2（bench.py --lat 通道首跑） | 750 | 2125 | 2500 | 3813 | 3813 |

62.5MHz 口径：p50 ≈ 30-34µs，max < 80µs——P-B 直切面（G3-2 段①）下
wake→run 同核段亚百微秒量级。G3 阶段对照：本表 = G3-4 段首锚（环境 =
单路 boot、段②起不可复现——跨段互拍无效，段内复跑为准〔f4 F7〕）。

## 踩坑实录（判例 §A127 已立）

- **CURRENT 静态 vs current_task() 正统读法（首跑 D-abort）**：wake_stamp_take
  首版读裸 `CURRENT` 静态——SMP 形态下 `set_current` 恒走 per-CPU 槽路径、
  静态从未被写恒 null → 空解引用 D-abort（FAR=0x320 = wake_vct 字段偏移；
  崩点 wake_stamp_take+0x8 = `ldr x0,[x8,#0x320]`，x8=[CURRENT]=0）。
  修复 = `current_task()`（槽优先 + 静态回落）。教训：sched 内新增读
  「当前任务」的 API 必须走 current_task() 正统读法，裸 CURRENT 静态在
  SMP 形态是弃用回落位。

## 验证矩阵（G3-4a 收口态）

| 项 | 结果 |
|----|------|
| cargo build（default 面） | ok（既有 2 警告 = hal target-feature 噪声，非本刀） |
| QEMU -smp 2 + lat 门（手跑 ×2） | BENCHLAT,lane=ipc + LATIPC ok n=32 双跑齐 |
| bench.py --lat 通道（含 qemu_preflight/收割/一致性） | PASS（表见上） |
| bench.py --lat/--rt/--update 组合拒绝集 | 拒绝文案三形齐（f4 F15 复验回执：`out/bench-receipts/reject-trio-20261105-093026.log` 三形全拒 exit=2） |
| just test-host | 全 suite ok 0 failed |
| just smoke（八跑） | PASS（SMOKE_EXIT=0）：金丝雀 42 严格递增 + rootfs 相全链 + RX 交互相 + FAT 子目录/持久化相 + B4 掉电三相（采样 A/B + 毒环注入 + Boot C 恢复终验）——零漂移 |
| B-2 icount prove 五跑 + 重立 | 五跑自洽 exact（max_dev 0.000%；BENCHWAKE 行五跑逐字节同）；**对旧基线两行漂**：frame_alloc4k 37→172（+135）、vmo_commit1p 1574→1706（+132）——**轨道对翻转 + 微位移**（{172,1706} = 在册轨道对 {171,1708} 加 +1/−2；G3-4a 新码叠掷布局骰，notes.md「G3-4a 烘基」条 + §A95/§A109 先例。**勘误〔f4 P-1，2026-11-05〕**：本行原归因「note_wake 戳+Task 增域的 icount 足迹」机制上不可能——bench::run() 前置 berry_sched::init()，note_wake 调用面在 bench 窗内不可达）；`--update` 重立后过门禁（回执见下） |
| just bench（B-0 对拍） | PASS（6 项全在 ×3.0 界内：4/48/17/28/293/784；顺带 docs_guard 0 警 + abi-check PASS） |
| just smoke-smp（八相） | PASS（SMP_EXIT=0）：相 1/2 boot 执法（SMP/SGI/SGI2 恰一次 + 裸 27 + rootfs 47 + PAR d1）+ 相 3 LB 三硬门 + 相 4 ASIDR + 相 5 TLBS n=10 + 相 6 ALLOC n=320 + 相 7 HT n=320 + 相 8 PP 320/320 |
