# G3-4b 证据档：bench-lat SGI 路 + trace 开/关双跑（wake→run 段补盲之二）

> 刀：G3-4 延迟基准+双跑（草案 §4）第二段——SGI 路（EL0 用户对跨核唤醒）+
> trace 开/关双跑（可观测性 :52 过关要件载体）。日期：2026-11-05。

## SGI 路（lane=sgi）

- **载体**：storm_spawn_fleet 形 EL0 舰队（root x0=0 占位即刻退 + writer x0=1
  sleeper spread→CPU1）+ 内核 waker `task_latsgi_wak`（CPU0，prio 0）。v1 无
  跨表 handle 转递——载荷自建 Event（event_create 落舰队私有表），**内核钩子
  在 wait 进入点钉 ObjRef = 唯一桥**（`LATSGI_OBJ`）。
- **载荷环**（×32）：clock_read 锚 → deadline = now+60s（§A118：deadline 是
  MONOTONIC ns 绝对时刻，非 CNTVCT 周期）→ `wait_one(h, 1<<49, deadline, obs)`
  跨核睡 → 醒后 `object_signal(h, clear=1<<49, 0)` 消费闩位（§A61 纪律）→ 再睡。
  窗槽：0x0 时钟 / 0x40 handle / 0x48 observed / 0x80 err / 0x90 DONE。
- **唤醒面 = sys_object_signal 尾同链**（`Event::signal(0, bit)` +
  `wake_matching(addr, bit)`）：跨核睡的醒来重验（重读 Event `signal_state`）
  须真置位才命中——纯 `wake_matching` 不写 Event 态即漏醒（与 IPC 路裸
  `wake_matching` 直驱的实质差异：IPC sleeper 经 `block_current` 直睡不经
  重验面）。位签名 `1<<49` 在册 USER_SIGNAL_BITS（LAT_EV_BIT 1<<48 邻位）。
- **测量口径**（与 IPC 路同族）：wake 侧锚 = note_wake 戳（CPU0 唤醒侧）；
  run 侧读点 = 醒后首段内核执行（wait 返程钩子取戳+cntvct）。**被测段
  勘正〔f4 F2〕：远端 RQ 入队与 SGI 发送（`enqueue_with_kick` 内含
  send_sgi MMIO）在 `wake_one_inner` 实序中先于 note_wake 戳完成、不在
  被测段内**——段内 = SGI 到达尾延 + CPU1 park 出场 + 派发切回 + 取戳
  （IPC 路 = 同核直切段；两 lane 同点打戳，对照差 = 跨核增量）。L0 执法：
  醒因恒 Signaled（Timeout = waker 漏醒）、唤醒数恒 1、戳非 0、负延迟
  panic、**sleeper 落核断言 ==1〔f4 F3 补：place() 失衡启发不满足时原先
  静默退化同核，量级签名只有人眼察觉——现 LATSGI_CPU 结构背书〕**。

### 踩坑实录（判例 §A128 已立）

- **首跑双 panic 同根**：①`latsgi_wak: 唤醒数 0 ≠ 1`（boot 早期）②60s 后
  `wait 返程 ret=Timeout`。根因 = **TCG 跨 vCPU 握手竞速**：waker 用 ¼ slice
  guest 钟 spacer 盲 pacing，但 guest 钟=墙钟——CPU0 睡过 ≠ CPU1 vCPU 已被
  宿主调度（boot 高载副核滞后任意量）→ sleeper 未回睡而信号已到 → n==0；
  首发漏掉的信号闩在 Event 上，sleeper 首睡即 Signaled 快进一轮、后续某轮
  撞漏醒 → 60s 后 Timeout。逐项排除 walkthrough（匹配谓词/per-CPU SLEEPQ/
  ObjRef 地址/轮内清位/deadline 量纲）全立——竞速在 pacing 假设自身。
- **修法 = 等待序号握手**：`LATSGI_ENTER` 在每次 wait 进入点递增（钩子跑在
  sleeper 的 SVC 屏蔽窗内，insert_sorted 先于放锁）；waker 每轮有界轮询
  `ENTER ≥ i+1` 后再取 KL 发 signal+wake——**KL 串行保证 waker 拿锁时
  sleeper 必已在 SLEEPQ（无 TOCTOU）**；n==1 断言留作终背书。次跑两路齐发
  零 panic。

### 实跑数据（本机 mac，QEMU 11.x TCG，-smp 2；绝对值跨机不可比——B-2 纪律）

> **环境声明〔f4 F7〕**：本段（段②起）= **双路同 boot**（lat=1 门下一次
> 起 IPC+SGI 两对）；段①（evidence-g3-4a）= 单路 boot（SGI 对未在树）
> ——跨段数值不可直接互拍，段内对照为准。

| 跑 | lane | min | p50 | p90 | p99 | max |
|----|------|-----|-----|-----|-----|-----|
| 手跑 90s 窗 | ipc | 500 | 812 | 2000 | 4312 | 4312 |
| 手跑 90s 窗 | sgi | 19562 | 9690187 | 13751437 | 14685750 | 14685750 |
| bench.py --lat | ipc | 563 | 1875 | 2562 | 4312 | 4312 |
| bench.py --lat | sgi | 18187 | 10494312 | 13517063 | 15135750 | 15135750 |
| bench.py --lat〔f4 F1/F6 回执复跑〕 | ipc | 562 | 875 | 2250 | 5000 | 5000 |
| bench.py --lat〔f4 F1/F6 回执复跑〕 | sgi | 16500 | 8755500 | 13780125 | 15234750 | 15234750 |

> 上四行 = 修复批前史档行（当时无回执通道——f4 F1/F6 缺档面；行值跨跑
> 浮动属 TCG 载荷纪律 §A118 族，非漂移判据）。**修复批起 write_receipt
> 落 `out/bench-receipts/`（gitignored 机内持久），复跑行有档可引**：
> `lat-20261105-093156-41389.log`（argv 头 + lane 表 + 全 serial）。本跑
> 同时 = **F3 落核断言（LATSGI_CPU==1）首活检验 PASS**。

62.5MHz 口径：ipc p50 ≈ 13-30µs（同核直切）；sgi min ≈ 291µs 但 p50 ≈ 168ms
——**跨核段被 TCG 宿主 vCPU 调度主导**（MTTCG 下副核 vCPU 线的宿主唤醒+
调度延迟，非内核机件成本；min 才接近机件下界）。两路对照即段分解的原始
意图成立：同核/跨核量级差 = 平台调度面的诚实测量。

## trace 开/关双跑（--trace-dual 通道）

- **门**：fw_cfg `opt/berry/trace`（值 "0" 恰 1 字节 = 记录面关；缺席 = 开，
  默认面零漂移——run/smoke 不递送该键）。关 = `on_switch`/`on_wake` 整体
  让位（rec 构造 + 环写 seqlock + 直方图）；**计数器族（intent/IPC fastpath）
  与 note_wake 戳不随关**（boot 断言面/bench-lat 基座，观测计数 ≠ 环记录面）。
- **watchdog 适配**：环/直方图断言与 BENCHWAKE 打印随记录面让位（断言对象
  不在场还执法 = 错杀），替代标记 `TRACE off ok`（off 臂 done 锚）；金丝雀/
  杀数断言不动。
- **通道**：`bench.py --trace-dual` = --icount 同形态两臂背靠背 + 关-开行差
  帽 TRACE_CAP + 两臂各对拍自家家族基线（on = 既有 bench_baseline_icount.txt
  / off = 新 bench_baseline_icount_traceoff.txt，基线双份入库——草案 §4）。
  `--update` 刷双份且**帽不过不许刷**。
- **首标定定谳：TRACE_CAP = 0.0（结构性零）**——六行关/开逐值恰等，非量不
  到：bench::run() 前置 berry_sched::init()（P-1 机制面反向承重），六窗全在
  调度器存在之前的串行 boot 纪元，遥测钩子窗口内结构性不可能发射。帽 =
  ④' 控制列断言的全行推广：任一非零差 = 门泄漏（报警）或 bench 面迁纪元
  （合法演进须重标定）。off 臂五跑逐值相同 = off 家族 L2 资格。

| 面 | off | on | δ |
|----|-----|----|---|
| cntvct_pair | 0 | 0 | 0 |
| frame_alloc4k | 82 | 82 | 0 |
| frame_alloc64k | 37 | 37 | 0 |
| handle_evt | 165 | 165 | 0 |
| vmo_commit1p | 1617 | 1617 | 0 |
| vmo_rw4k | 6937 | 6937 | 0 |

## 验证矩阵（G3-4b 收口态）

| 项 | 结果 |
|----|------|
| cargo build（default 面） | ok（既有 2 警告 = hal target-feature 噪声，非本刀） |
| QEMU -smp 2 + lat 门（手跑） | BENCHLAT,lane=ipc + lane=sgi 双路齐 + LATIPC/LATSGI ok n=32，零 PANIC |
| bench.py --lat 通道（双路收割/一致性） | PASS（表见上） |
| QEMU trace=0 手跑 | TRACE off ok 标记在位 + G1.5 ok + tele total=0 + BENCHWAKE 让位，零 PANIC〔f4 F12 注：该跑实为**单核 boot**——cpu_on 回落（traceoff.log `SMP: cpu_on ret=… -> single-core, skip`），故只作单核 off 臂定性面；双核 off 臂数值面由 --trace-dual off 臂覆盖〕 |
| bench.py --trace-dual（帽 + 双臂 L2） | PASS：六行 δ=0 过帽（TRACE_CAP=0.0）+ on/off 两家族 12 项 L2 全界内 |
| off 臂五跑证明 | 逐值相同 {0,82,37,165,1617,6937}（L2 资格，notes.md 台账） |
| B-2 icount prove 五跑 | verdict=exact max_dev=0.000%；对旧基线漂移 = 172→82 / 1706→1617（反向 = 轨道骰换轨非 envelope）+ handle_evt 166→165（−1 微位移）；BENCHWAKE 五跑逐字节同；`--trace-dual --update` 双份重立 |
| P-1/P-2 折叠（f4 对抗审查） | ①notes.md 补 G3-4a 烘基史档条（轨道对翻转归因 + 勘误）②evidence-g3-4a.md 验证矩阵行改「轨道对翻转+微位移」引 notes.md ③沿革本段追记勘误行（原段不重写）④trace 帽方法论 = TRACE_CAP 常量注（④' 全行推广） |
| just test-host | 全 suite ok 0 failed |
| just smoke（八跑） | PASS exit=0（金丝雀 43 条严格递增〔=switches 行数随时钟节奏浮动，门=≥3 且严格递增，G3-3c 跑同 43〕+ rootfs 47 + RX 交互相 + FAT 子目录 74 + 持久化 + B4 掉电三 boot 恢复；全卷 PANIC/VIOLATION 零命中；本机宿主争用下全链 ~40min） |
| just smoke-smp（八相） | PASS exit=0（相 1/2 boot 执法 + 裸 27 + rootfs 47 + PAR d1 + 金丝雀递增；LB c0=0/c1≥24/n=4、ASIDR n≥1、TLBS n=10、ALLOC n=320、HT n=320、PP 320/320 全门；VIOLATION/PANIC 零命中） |
| just bench（B-0 对拍 + docs_guard + abi-check） | PASS exit=0（B-0 ×3.0 六项全在界：4/28/18/29/270/760 + docs_guard 0 警 + abi-check v1 34 号四面单源一致） |

## f4 修复批复验矩阵（F1-F15 回写后，2026-11-05）

| 项 | 结果 |
|----|------|
| B-2 icount prove 五跑（回执化） | verdict=exact max_dev=0.000%；{0,82,37,165,1617,6937} 对在库双基线**零漂移**——修复批内核增量全冷径零布局漂移，双基线不重烘续役（回执 icount-20261105-09305{0,1,2,3,4}-41274 + icount-prove-20261105-093054） |
| bench.py --trace-dual（回执化） | TRACE_CAP=0.0 六行 δ=0 过帽 + 双家族 12 项 L2 全界内；**off 臂 F14 记录面真空断言（total()==0）活检验 PASS**（回执 trace-dual-20261105-093122-41343） |
| bench.py --lat（回执化） | 双路齐；**F3 落核断言（LATSGI_CPU==1）首活检验 PASS**——sleeper 结构性落 CPU1（回执 lat-20261105-093156-41389；表见上节复跑行） |
| 拒绝三形取证（F15） | --lat --rt / --lat --update / --trace-dual --icount 全拒 exit=2（回执 reject-trio-20261105-093026） |
| just test-host 取证（F15） | 78 suite 全 ok 0 failed exit=0（回执 testhost-20261105-093131） |
| F2/F7/F12/F13 文档面 | 测量口径勘正（被测段不含 RQ 入队+SGI 发送）+ 段①②环境声明×4（单路→双路 boot 不可跨段互拍）+ trace=0 手跑单核注记 + 命令速览措辞耐久化 |
| F5/F8/F9/F10/F11 内核面 | BOOT_TIMEOUT_LAT 240（规则/08 ×2 律）+ 注释四条（wait 位纪律前提 / SMP halt 语义 / wake_stamp_take SQ 序边 / TRACE_OFF 显式边 Release-Acquire） |
| F1 结构根治 | write_receipt 回执面落 out/bench-receipts/（argv 头 + summary + 全 serial）；史档缺档勘误见 notes.md f4 修复批复验条 |
| just smoke（八跑） | PASS exit=0（金丝雀 43 条严格递增〔G3-4b 批同值，门=≥3 且严格递增〕+ rootfs 相全链 + RX 交互相 + FAT 子目录/持久化相 + B4 掉电三相恢复终验；全卷 PANIC/VIOLATION 零命中；回执 `out/bench-receipts/smoke-20261105.log`〔f4 复验抢救归档，/tmp 易失路径不入 committed 证据〕） |
| just smoke-smp（八相） | PASS exit=0（相 1/2 boot 执法 SMP/SGI/SGI2 恰一次 + 裸 27 + rootfs 47 + PAR d1；LB c0=0/c1≥24/n=4、ASIDR n≥1、TLBS n=10、ALLOC n=320、HT n=320、PP 320/320 全门；VIOLATION/PANIC 零命中；回执 `out/bench-receipts/smoke-smp-20261105.log`） |
| just bench（B-0 对拍 + docs_guard + abi-check） | PASS exit=0（B-0 ×3.0 六项全在界：4/29/17/26/273/750 + docs_guard 0 警 + abi-check PASS；回执 `out/bench-receipts/bench-20261105.log`） |
