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
| 静态/常量 | main.rs:3479-3504 | `LAT_TRIG_IPC_OBJ`/`LAT_TRIG_SGI_OBJ`（AtomicUsize，0=未装）+ `LAT_TRIG_BIT=1<<50`（USER_SIGNAL_BITS 位表已登记 :3028——f4 F8 补）+ `LAT_TRIG_FAR_S=480`（FM-16 修正值） |
| 触发装配门 | main.rs:1580-1609 | soakmon 门块内嵌套 `if lat 探针在位` → 表插 2 Event + Release 存指针 + 自证行 `LAT trig armed`（**soak∧lat 双门**才装；bench.py --lat 单门 = 静态恒 0 → 一次性原形全保，bench 收割面零改动） |
| IPC sleeper | main.rs:3506-3585 | 批环「32 轮睡-醒 → 排序 → 打 BENCHLAT+LATIPC ok → 读静态：0=break 退（bench 形）/ 非 0=回睡续批（C 形）」 |
| IPC waker | main.rs:3588-3668 | 周期环「32 轮间隔唤醒 → 读静态：0=break / 非 0=block 触发（far=480s）→ **睡前锁窗内补位 poll**（f4 F1 修 :3630-3652——位已置即跳睡直取，恢复 poll-first 追拍语义 + 补齐 block_current 前置契约）→ 醒后锁窗内清自身位 → assert Signaled（Timeout=连漏 8 拍 = 断流死讯）」 |
| SGI waker | main.rs:3685-3924 | 周期环 `cyc` 计数 + ENTER 绝对目标 `cyc*32+i+1`（EL0 sleeper 永不复位——boot 周期耗 1..32，周期 1 对 33..64）+ 采集毕锁窗内 `LATSGI_DONE.store(0)`（安全交接窗：DONE==32 读出 = 上批全发布 + sleeper 已回睡 = BUF/DONE 至下周期首信号前无写者）+ 同形触发等（F1 补位 poll 同形 :3883 起） |
| soakmon 触发面 | main.rs:3987-4056 | 拍行打出后同 klock 窗对两触发 Event `signal(0,TRIG_BIT)` + `wake_matching`（环 :4040-4050；prio 0 < soakmon 8 → 锁窗内无抢占面） |

### EL0 sleeper kernel/src/user.rs

- `__user_latslp` 模板（user.rs:1503-1566）：撤 x8/x7 计数器与 0x90 DONE
  槽 win-store + thread_exit 尾 → **`b 3b` 永续环**；δ 60s→480s（0xC000/
  0xC23A/0x6F = 480e9，python3 验算——**首拍网格远界**：soakmon 首拍
  t0+5×60s=300s 有意后置，boot 周期末重睡点到首触发 gap 恒 <300s）；
  超时面真兜底 = 内核尾钩 L0 panic 先响（f4 F7 改口：latsgi_ret 对
  -110 / lat 侧醒因断言——svc 返回前内核已死，EL0 err++ 早退对超时
  不可达；现网两 harness 均先杀 QEMU 不触发；err 窗实际兜非超时错路
  ——event_create/clock_read/object_signal 失败类）。

### harness tools/soak-g3.sh

| 件 | 位置 | 形 |
|---|---|---|
| 恰 N+1 收割 | soak-g3.sh:208-232 | 每路 BENCHLAT/LATIPC/LATSGI ok 行数**恰 beats+1**（boot 周期 + 每拍一周期）；≠ 即红 |
| max-over-cycles | soak-g3.sh:244-251 | 全周期 max 对帽（n=32 时 p99 秩=max 秩——单 max 帽全覆盖 p99） |
| CALIBRATE 模式 | soak-g3.sh:212-243 | `SOAKG3_CALIBRATE=1` → 观测不执法 + 稳态帽建议值（×3.0）+ kill criteria 顾问 |
| kill criteria 参照 | soak-g3.sh:94-100 | `BANK_CAP_IPC=18189` / `BANK_CAP_SGI=61905000`（银行跑数字；观测 max 超其 ×3.0 = 候选① 降级线）——**执法分支同评（f4 F4 修 :252-262：执法跑机器评估降级线，LAT_CAP_* 抬高后不再盲区；pages 肢两模式对称 fail-loud）** |
| COVER 15 标记 | soak-g3.sh:321 | `LAT trig armed`（触发装配自证——双门缺一即无此行 = COVER 红）；boot 完成口径行 :334 四面自证 |
| 结算环 | 尾段（:427-447） | 末拍后 settle 轮询（≤45s 帽 < 58s 下拍裕量）等两路计数 ≥ N+1 → kill QEMU 冻结日志 → 恰等断言 + lat_face——先冻结再断言，免漏行假红/新拍移动靶双坑 |

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
   （`lat_slp: 醒因非 Signaled` main.rs:3547 + `latsgi: wait ret=-110`
   latsgi_ret 钩）——根因 = soakmon 首拍**有意**后置 t0+5×period=300s
   （留 5 拍前置让 pages 基线落稳态，任务注释明言），而触发远界误按
   稳态拍距算成 180s「3 拍裕量」：boot 周期末重睡点到首触发 gap 可达
   ~300s > 180s → 两路 sleeper 界先到期双 Timeout → 内核死 → boot 覆盖
   缺 SKT/FAT/EVS 三标记（套件被杀非卡死——银行跑标记序 NET→TCPS→
   POLL→EVS→SKT→FAT 佐证）。修 = 远界 180s→**480s**（gap 数学上界
   <300s——首拍网格固定 t0+300、重睡点 X>0 ⟹ gap<300；480=300+60%
   裕量；断言文案 3 拍→8 拍）。教训：**远界必须对「首个事件」标定，
   非稳态节拍**——首事件常带启动后置（基线窗/预热窗/握手窗）。
   〔f4 F17 出处注记（2026-11-05）：双 PANIC 文案无耐久载体——首验跑
   经 fail() KEEP 只留易失 $TMPDIR（现已被 /tmp 清洗规则扬掉），回执库
   全卷零命中；上文文案转录自首验现场勘验，机理链可由复跑再现（480→
   180 回退即红）。判例 §A129 入册依赖本段转录。〕

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
| pages 窗 | 零增量（常驻足迹 boot 吸收） | 两跑实证（f4 F18 改口）：12 拍 CALIBRATE + 执法跑 t≥5 起恒 13704（与银行跑同终态）；3 拍 sanity 无 t≥5 数据（boot 尾结算未落定窗）；t=4-5 台阶 = boot 尾结算非 per-周期泄漏（银行跑对照定谳） |
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

## 修复批复验（f4 三镜复审 19 条销账，2026-11-05）

- **修复批**：F1 = 两睡点睡前锁窗内补位 poll（task_lat_wak :3630-3652 /
  latsgi_wak :3883 起——f4 荐形，恢复 poll-first 追拍语义 + 补齐
  block_current「调用方须已验条件」前置契约）；F4 = lat_face 执法分支补
  BANK_CAP×3 kill 线断言（:252-262——执法跑同机器评估降级线，对称
  fail-loud）；F5/F6 = 判例 §A129（远界对首事件标定）+ §A45 追记（永生
  适用反转）三处一动入册；F2 = 本件落码面表整表重锚（FM-16 修后值/行
  号漂移回刷）；F7-F19 随手批（注释改口 ×6 / 位表登记 / /tmp 留档迁
  out/bench-receipts / 口径行 ×3 / 笔误）。
- **新树回执（F3）**：default 面 smoke 四相 PASS 零 FAIL（boot 27/27 +
  rootfs 全链 + RX + FAT 子目录/持久化 + B4 掉电毒环；金丝雀 42 严格
  递增；`out/bench-receipts/smoke-fixbatch-20261105.log`）+ test-host
  87 套件全绿 exit=0（`testhost-fixbatch2-20261105.log`；首跑撞 sccache
  服务坏死 EPERM——stop/start-server 修复后重跑，非测试面问题）。
- **lockdep 面 soak sanity 3 拍复跑 PASS（F1 行为回执 + F12 stdout
  留档）**：两路 BENCHLAT 恰 4 = 拍数+1（补位 poll 后触发环零丢拍、
  逐拍一一对应）+ ipc max 7187 ≤ 24561 / sgi max 20684062 ≤ 62184750
  + kill 线 18189×3/61905000×3 内（F4 执法分支首跑评估）+ lockdep
  三计数绿 + pages 13626 恒 / Δsw1min 555 ≥ 320 / churn 1343 严格递增
  + R3 boot=331s / 均拍=46s（stdout 卷
  `soakg3-cform-sanity3-stdout-20261105.log` + 串口卷
  `soakg3-cform-sanity3-20261105.log`——R3 定标行留档补 F12 缺口）。
- **flake 双证记档（§A110 复跑绿定谳 + §A118 载荷族）**：首跑 sanity2
  红于均拍 34s < 40s（S-5e ① 墙钟下界）——功能面全绿（COVER 齐 /
  VIOLATION=0 / 触发环在跑 / pages-churn 常态），同树复跑 46s 过 =
  宿主空载 TCG 墙钟压缩偶发（非 kernel 回归；真 deadline 减半 bug 在
  今日机速下显 ~17s/拍，34s 不在其带）；40s 下界保留不动，双证卷
  `soakg3-cform-sanity2-stdout-20261105.log`。
- 测试策略 G3 行已补后继指针（C 形 + 修复批复验路由本件）。
- **放行（f4 hunk 复验 19/19 闭合，2026-11-05）**：F1 poll 前置/同窗
  论证核过（event.rs `load()` 在册）、F2 新锚亲核全中、F4 活检验 PASS、
  flake 双证采信（空载压缩与 §A118 争用面同族反向）——判词放行段
  `out/bench-receipts/verdict-g35c.md`（含逐条销账表）。刀链闭环 =
  00ec23f + d3e42e0 + 64c602f + dff0640 + 7021ab5。
