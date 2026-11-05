# G3-5 转正候选① 实施档：C 形常驻再触发 lat——稳态 BENCHLAT 覆盖落地

> 刀：G3-5 转正候选①实施刀（todo_fbb5e3c1efba；依据 evidence-g3-5recon.md
> 后继刀形 1/2/4）。产出 = C 形全链落码 + soak-g3.sh 收割改写 + CALIBRATE
> 首跑定稳态帽 + 相互面复验。日期：2026-11-05。
> 前置：侦察档三形定谳（A 判死 A45 泄漏 / B 备选 / **C 最优零 respawn**）。

## 形状定稿（C 形，零 respawn）

- **fleet/waker 恰 boot 一次 spawn**（现链零改动）→ 一次性足迹
  （~6-9 页 + 2 触发 Event + 常驻栈）全落首拍基线（侦察事 9 机理反用）。
- **每路一个触发 Event**（soakmon 私有表内，`RIGHT_SIGNAL`）：共享单
  Event 有清位竞速（先醒者清位后另一路 re-verify 见空 → 漏拍），故分路。
- **触发源 = soakmon 拍后**：每拍行打出后、同 klock 窗内
  `signal(0, LAT_TRIG_BIT)` + `wake_matching`（不 assert 唤醒数——waker
  周期中段信号 = 位锁存，poll-first 追平自愈；多拍合并 → 行数短少由
  harness 恰 N+1 断言网住）。
- **位段**：LAT_EV_BIT=1<<48 / LAT_SGI_BIT=1<<49 延续 → `LAT_TRIG_BIT=1<<50`。
  匹配键 = (obj, bit) 对——跨 Event 复用位安全。

## 落码面（file:line 锚点，主树本刀）

### 内核 kernel/src/main.rs

| 件 | 位置 | 形 |
|---|---|---|
| 静态/常量 | main.rs:3488-3498 | `LAT_TRIG_IPC_OBJ`/`LAT_TRIG_SGI_OBJ`（AtomicUsize，0=未装）+ `LAT_TRIG_BIT=1<<50` + `LAT_TRIG_FAR_S=180` |
| 触发装配门 | main.rs:1580-1608 | soakmon 门块内嵌套 `if lat 探针在位` → 表插 2 Event + Release 存指针 + 自证行 `LAT trig armed`（**soak∧lat 双门**才装；bench.py --lat 单门 = 静态恒 0 → 一次性原形全保，bench 收割面零改动） |
| IPC sleeper | main.rs:3500-3577 | 批环「32 轮睡-醒 → 排序 → 打 BENCHLAT+LATIPC ok → 读静态：0=break 退（bench 形）/ 非 0=回睡续批（C 形）」 |
| IPC waker | main.rs:3579-3640 | 周期环「32 轮间隔唤醒 → 读静态：0=break / 非 0=block 触发（far=480s）→ 醒后锁窗内清自身位 → assert Signaled（Timeout=连漏 8 拍 = 断流死讯）」 |
| SGI waker | main.rs:3725-3880 | 周期环 `cyc` 计数 + ENTER 绝对目标 `cyc*32+i+1`（EL0 sleeper 永不复位——boot 周期耗 1..32，周期 1 对 33..64）+ 采集毕锁窗内 `LATSGI_DONE.store(0)`（安全交接窗：DONE==32 读出 = 上批全发布 + sleeper 已回睡 = BUF/DONE 至下周期首信号前无写者）+ 同形触发等 |
| soakmon 触发面 | main.rs:3935-3998 | 拍行打出后同 klock 窗对两触发 Event `signal(0,TRIG_BIT)` + `wake_matching`（prio 0 < soakmon 8 → 锁窗内无抢占面） |

### EL0 sleeper kernel/src/user.rs

- `__user_latslp` 模板（user.rs:1503-1565）：撤 x8/x7 计数器与 0x90 DONE
  槽 win-store + thread_exit 尾 → **`b 3b` 永续环**；δ 60s→480s（0xC000/
  0xC23A/0x6F = 480e9，python3 验算——**首拍网格远界**：soakmon 首拍
  t0+5×60s=300s 有意后置，boot 周期末重睡点到首触发 gap 恒 <300s）；
  超时 = err++ → thread_exit（waker 退场/断流 8 拍 → 舰队自回收——
  bench 单门形退出后 480s 收口，无僵尸）。

### harness tools/soak-g3.sh

| 件 | 位置 | 形 |
|---|---|---|
| 恰 N+1 收割 | soak-g3.sh:204+ | 每路 BENCHLAT/LATIPC/LATSGI ok 行数**恰 beats+1**（boot 周期 + 每拍一周期）；≠ 即红 |
| max-over-cycles | soak-g3.sh:237-244 | 全周期 max 对帽（n=32 时 p99 秩=max 秩——单 max 帽全覆盖 p99） |
| CALIBRATE 模式 | soak-g3.sh:225-238 | `SOAKG3_CALIBRATE=1` → 观测不执法 + 稳态帽建议值（×3.0）+ kill criteria 顾问 |
| kill criteria 参照 | soak-g3.sh:93-94 | `BANK_CAP_IPC=18189` / `BANK_CAP_SGI=61905000`（银行跑数字；观测 max 超其 ×3.0 = 候选① 降级线） |
| COVER 15 标记 | soak-g3.sh:303 | `LAT trig armed`（触发装配自证——双门缺一即无此行 = COVER 红） |
| 结算环 | 尾段 | 末拍后 settle 轮询（≤45s 帽 < 58s 下拍裕量）等两路计数 ≥ N+1 → kill QEMU 冻结日志 → 恰等断言 + lat_face——先冻结再断言，免漏行假红/新拍移动靶双坑 |

## 竞态/失效面清单（实施前定谳 FM-1..15 摘要）

1. **共享触发位清位竞速** → 每路独立 Event。
2. **DONE/BUF 跨周期安全** → 无写者交接窗复位（SGI waker 采集毕复位）。
3. **漏拍合并** → 位锁存 + poll-first 追平 + 恰 N+1 断言网形。
4. **触发源竞速（soakmon 信号时 waker 周期中）** → 不 assert 唤醒数，
   醒因靠 waker 自身 Signaled 断言（Timeout 才红）。
5. **fleet 亡后悬空 LATSGI_OBJ 解引用** → 不可达：waker ENTER 轮询先失败
   → L0 panic 先至。
6. **park1 下界窗风险** → CPU1 被 soakload 写者饱和，park 冻结无扰。
7. **REAPQ 流量降**（不再周期退任务）→ 无断言依赖 REAPQ 量，无害。
8. **〔FM-16 首验红案·已修〕首拍网格 vs 触发远界错配**：首验跑双 PANIC
   （`lat_slp: 醒因非 Signaled` main.rs:3538 + `latsgi: wait ret=-110`
   latsgi_ret 钩）——根因 = soakmon 首拍**有意**后置 t0+5×period=300s
   （留 5 拍前置让 pages 基线落稳态，任务注释明言），而触发远界误按
   稳态拍距算成 180s「3 拍裕量」：boot 周期末重睡点到首触发 gap 可达
   ~300s > 180s → 两路 sleeper 界先到期双 Timeout → 内核死 → boot 覆盖
   缺 SKT/FAT/EVS 三标记（套件被杀非卡死——银行跑标记序 NET→TCPS→
   POLL→EVS→SKT→FAT 佐证）。修 = 远界 180s→**480s**（gap 数学上界
   <300s——首拍网格固定 t0+300、重睡点 X>0 ⟹ gap<300；480=300+60%
   裕量；断言文案 3 拍→8 拍）。教训：**远界必须对「首个事件」标定，
   非稳态节拍**——首事件常带启动后置（基线窗/预热窗/握手窗）。

## 跑档（验形 → 定标 → 执法三段）

### ① 3 拍验形跑（CALIBRATE 观测）——PASS 全绿

- 命令：`SOAKG3_CALIBRATE=1 SOAKG3_KEEP_LOG=1 bash tools/soak-g3.sh $(bash tools/paths.sh kernel) 3`
- 判据：COVER 15/15（含 `LAT trig armed`）+ 每路恰 4 行 + 触发节拍 + lockdep/park/churn 全绿 + PASS。
- **结果：PASS 3/3 拍七断言全绿**（首验红案 FM-16 修复后复跑；日志
  `out/bench-receipts/soakg3-cform-sanity-20261105.log`）：
  - 恰 N+1：两路各恰 4 周期行（boot+3 拍一一对应，收割面零漂移）。
  - 逐周期 IPC p99/max（tick）：boot 6125 → 8188 → 7188 → 6125——**稳态
    ≈ boot 同量级**，soakload 干扰未抬尾（对照银行跑帽 18189 远低）。
  - 逐周期 SGI p99/max（ns）：boot 20697062 → 20688875 → 20665000 →
    20666750——稳态 ≈ boot（对照银行跑帽 61.9ms 远低）。
  - **pages=13603 三拍恒定零增量**（C 形核心主张「常驻足迹恒定」实证）。
  - park0=0 / park1=5 恒（sleeper 睡 SLEEPQ 无扰）；churn 1007→1176→
    1346 严格递增；sw 5252→6291→7331 递增；Δsw1min=556 ≥ 地板 320/拍。
  - R3 定标：boot=338s（帽 600s）/ 均拍=42s / lockdep 三计数绿（VIOLATION=0）。

### ② 12 拍 CALIBRATE 定标跑（稳态帽）——PASS 八断言全绿

- 命令：`SOAKG3_CALIBRATE=1 SOAKG3_KEEP_LOG=1 bash tools/soak-g3.sh $(bash tools/paths.sh kernel) 12`
- **结果：PASS 12/12 八断言全绿**（日志
  `out/bench-receipts/soakg3-cform-calib12-20261105.log`）：
  - 观测 max-over-cycles：ipc p99max=max=**8187**（帽建议 ×3.0 → 24561）/
    sgi p99max=max=**20728250**（→ 62184750）——13 周期分布稳定（3 拍跑
    8188/20697062 同量级复现）。
  - **pages 稳态窗真比较激活（≥7 拍）过**：轨迹 13603(t=1-3) → 13652(t=4)
    → 13704(t=5-12 落定，8 拍恒定)。+49/+52 台阶 = boot 尾结算（银行跑
    对照同款：13579→13603→[t=4 瞬峰 14563]→13704 落定→尾 56 拍恒定
    **同一终态 13704**）——非 per-周期泄漏，恰被「第 7 拍起真比较」基线
    设计吸收。**C 形零足迹主张 12 拍窗复证成立**。
  - R3：boot=335s / 均拍=57s / Δsw1min=555 ≥ 地板 320 / lockdep
    VIOLATION=0 / churn 严格递增至 2885。
- kill criteria 判读：稳态观测远低银行跑帽（ipc 8187 ≪ 18189×3、sgi
  20.7ms ≪ 61.9ms×3）——**无降级触发**，候选① 健康。

### ③ 稳态帽烘入 + 执法复跑——PASS 帽真执法生效

- 烘入：soak-g3.sh `LAT_CAP_IPC` 18189→**24561** / `LAT_CAP_SGI`
  61905000→**62184750**（12 拍观测 ×3.0；银行跑 boot 窗数字降为
  `BANK_CAP_*` kill-criteria 参照线，不随环境变量覆盖漂移）。
- 命令：`SOAKG3_KEEP_LOG=1 bash tools/soak-g3.sh $(bash tools/paths.sh kernel) 12`
- **结果：PASS 12/12 八断言全绿**（日志
  `out/bench-receipts/soakg3-cform-enforce12-20261105.log`）：
  - **lat 帽执法生效**：ipc p99max=max=14875 ≤ 24561（×1.65 余量）/
    sgi p99max=max=26388125 ≤ 62184750（×2.36 余量）；12+1 周期恰形。
  - 本跑观测 max 高于定标跑（14875 vs 8187 / 26.4ms vs 20.7ms）= 宿主
    TCG 载荷波动（§A118 同族）——仍在帽内，恰证 ×3.0 帽必要性（×1.5
    帽在此跑即红）。
  - pages=13579..13704 同终态落定窗绿；Δsw1min=553 ≥ 地板；lockdep
    VIOLATION=0；churn 严格递增至 2883；boot=358s / 均拍=55s / 全程 607s。

## 相互面复验（全绿预期）

| 面 | 预期 | 结果 |
|---|---|---|
| pages 窗 | 零增量（常驻足迹 boot 吸收） | 三跑实证：t≥5 起恒 13704（与银行跑同终态）；t=4-5 台阶 = boot 尾结算非 per-周期泄漏（银行跑对照定谳） |
| Δsw1 地板 | 只增不减（周期唤醒纯加项） | 3 拍实证：Δsw1min=556 ≥ 地板 320/拍 |
| park 帽 | 无扰（sleeper 睡 SLEEPQ 非 park） | 3 拍实证：park0=0 / park1=5 恒 |
| churn | 无扰（soakload 自家计数） | 3 拍实证：1007→1176→1346 严格递增 |
| lockdep | 同锁路径 N 次重复（银行跑 VIOLATION 0 先证） | 3 拍实证：VIOLATION=0 |

## kill criteria 执法

- pages 窗破（常驻足迹超预期）或 稳态 p99max > 银行跑帽 ×3.0
  （IPC 54567 / SGI 185715000）→ 候选① 降级，候选②（lockdep N 跑转正）顶上。
- **裁决：未触发——候选① 全链转正。** 三跑观测 max 最高 ipc 14875 /
  sgi 26388125（执法跑，TCG 载荷高位）均 ≪ 降级线（ipc 54567 / sgi
  185715000）且 ≤ 烘入稳态帽；pages 窗三跑绿（零 per-周期增长）。
  **稳态时延面自此有真 BENCHLAT 执法覆盖**（boot 窗 → 全 run 稳态窗）。
