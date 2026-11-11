# G3-5 证据档：激进配置常驻 soak 模板 + 首银行跑（草案 §5）

> 刀：G3-5 激进 soak 常驻——模板（tools/soak-g3.sh）+ 首银行跑 + 转正流程
> 启动。日期：2026-11-05。

## 激进镜像定谳（侦察面，2026-11-05）

- **全树 cargo feature 门唯 lockdep**：kernel/Cargo.toml features 仅
  `lockdep = ["berry-sched/lockdep"]`；sched/Cargo.toml 仅
  `lockdep = ["berry-hal-aarch64/lockdep"]`；cfg(feature) 全扫
  （kernel/src sched/src arch/src）命中 = aspace.rs:31/66/89 + vmar.rs:66/85
  + main.rs:651 + sched.rs:532-582——全 lockdep 桶。
- **P-A 抢占点 / P-B preempt-on-wake 已随 G3-1/G3-2 落 default 面**（非
  feature 化、无可开关面）；细粒度锁 = 基线设计非开关件；跨核 SGI len≥2
  轮转门 = G3-2 段② 收窄定谳后保留 default（swap ruled 非生产）。
- **定谳：草案 §5「preempt-full + 细锁全开 + lockdep」激进镜像 delta =
  lockdep feature 单项**——前两件已转正语义，激进镜像 = default 面 +
  lockdep。回写草案 §8 G3-5 段（本刀合同件）。

## 模板设计（tools/soak-g3.sh）

- **骨架复用合同**：soak-smp.sh 八断言逐字同款（validate_beats 不改测量
  面语义）+ 对拍三件套（echod/evsclient/sktclient 懒启动）+ G1.5 kill
  基线扣除 + 均拍下界 40s + R3 定标行 + 盘名专属 disk.soakg3.img
  （与 soak/soak-smp/smoke 四方零交集）+ 端口纪律同族（18080-18083
  勿与 smoke/run-rootfs/soak 并行）。
- **lockdep 断言面（新①）**：镜像自证（marker 串 strings 验——default
  面直拒不烧银行跑）+ boot COVER 加 `LOCKDEP selftest ok`（face 缺席
  即非绿）+ 终验三计数（ok 恰 1 / selftest FAIL 0 / VIOLATION 0——
  smoke-lockdep.sh 执法面搬入长跑收口；selftest FAIL 非停机必须计数
  执法，VIOLATION fail-loud 停机由心跳帽兜）。
- **尾延迟帽断言面（新②，§4 纪律「尾延迟帽后续进激进 soak 断言面」落地）**：
  fw_cfg 三键齐发（soak + soakload + lat）——G3-4 lat 载荷（一次性 ×32
  样本/路，boot 窗 spawn、测完 exit_current，rt_probe 同形）在 soakload
  舰队共窗下采样。spawn 序实测：IPC 路 main.rs:1523（soakload 舰队前 =
  净态 + lockdep 开销面）→ soakmon:1577 → soakload master:1791 → SGI 路
  waker:1818（舰队后 = 满载受扰采样——两路对照 = lockdep 增量与满载
  增量的分解面）。
- **执法面**：BENCHLAT 双路行非锚定收割（bench.py BENCHLAT_RE 同款口径，
  每路恰一行）+ LATIPC/LATSGI ok n=32 完成标记恰一次 + max 帽（n=32 时
  p99 秩 = max 秩、p99==max 恒成立——单 max 帽全覆盖，p99 随定标行报告）。
- **帽值首跑标定**（S-5d SW1_FLOOR 先例同形）：SOAKG3_CALIBRATE=1 首跑
  → 定标行观测 max → 观测 ×3.0 向上取整烘焙默认（B-0 界同款裕量），
  LAT_CAP_IPC/LAT_CAP_SGI 环境变量可覆盖（TCG 载荷 §A118 跨机不可比
  ——B-2 纪律）。
- **限界诚实记档**：lat 载荷一次性（一次性采样任务非常驻——kernel 侧
  设计合同），尾延迟帽覆盖 = boot 窗至首拍前；稳态时延面由既有活性断言
  族承载（均拍下界 40s / 单拍帽 240s / park 窗 / Δsw1 地板全时延敏感）。
  常驻再装形（周期重启采样）= 转正流程后继刀候选——本刀不开（舰队
  respawn 的僵尸累积面与 pages 稳态窗断言冲突，须先侦察任务回收面）。
  〔f4 F10 标注（2026-11-05）：本段「一次性 = 设计合同」口径已废除——
  G3-5recon 裁决 C 形常驻再触发可行、G3-5c 已落地执法
  （`tools/evidence-g3-5c.md`）；一次性仅历史原形（bench.py --lat 单门）。〕

## 标定首跑（CALIBRATE 模式，8 拍，2026-11-05）

- 三面自证全在席：`LOCKDEP selftest ok`×1 + FAIL 0 + VIOLATION 0；三门控
  gate on 行齐；零 PANIC——**lat 载荷在 soakload 舰队共窗下完整走完**
  （§A128 序号握手在满载竞速面成立）。
- BENCHLAT 双路观测（boot 窗，lockdep 面 + 满载受扰）：

| lane | min | p50 | p90 | p99 | max | §4 净态基线 max | 增量 |
|------|-----|-----|-----|-----|-----|----------------|------|
| ipc | 1750 | 3250 | 4875 | 6063 | 6063 | 5000 | ×1.21 |
| sgi | 13753938 | 19895062 | 20558938 | 20635000 | 20635000 | 15234750 | ×1.35 |

- 增量解读：ipc 路（soakload 舰队 spawn 前）= lockdep 取放 hook 进
  wake 路径的净开销面；sgi 路（舰队后）= lockdep + CPU1 满载受扰合成
  面。两路对照 = §5 激进镜像退化度的分解测量。
- **烘焙帽 = 观测 ×3.0**（B-0 界同款裕量）：LAT_CAP_IPC=18189 /
  LAT_CAP_SGI=61905000（脚本默认值 + 推导注；环境变量可覆盖）。

- **PASS 8/8 拍八断言全绿 + lockdep 三计数绿 + lat 双路完整**（回执
  `out/bench-receipts/soakg3-pilot-20261105.log` + stdout 同目录
  soakg3-pilot-stdout-20261105.log——f4 F1 纪律：$TMPDIR 易失路径不入证据）。
  〔f4 F16 出处注记（2026-11-05）：stdout 卷 PASS 行 LAT_OBS 仍默认值
  ——已提交 lat_face 必先于 PASS echo 置 LAT_OBS，故该卷出自未提交早期
  修订；定标产物 18189/61905000 经 G3-5c 链独立复核沿用，现以
  BANK_CAP_* 身份活在 kill 参照线（soak-g3.sh 头注同款注记）。〕
  R3：boot=344s（帽 600s）/ 首拍=0s / 均拍=54s（下界 40）/ Δsw1min=555
  （地板 320）/ pages=13591..13704 / churn=2214 严格递增 / parkD0=D1=0。

## 首银行跑（60 拍，2026-11-05）

- **PASS 60/60 拍八断言全绿 + lockdep 三计数绿 + lat 帽执法内**（回执
  `out/bench-receipts/soakg3-bank-20261105.log` + stdout 同目录）：全程
  3498s（58.3min），总帽 15720s。
- lockdep 面：selftest ok 恰 1 / FAIL 0 / **VIOLATION 0——1h 激进面
  （lockdep feature）+ 双核满载零锁序违约**。
- 尾延迟帽执法：ipc p99=max=5750 ≤ 18189（帽占 32%）/ sgi
  p99=max=20678750 ≤ 61905000（帽占 33%）。与标定跑对照（ipc 6063 /
  sgi 20635000）= §A118 族内浮动，非漂移判据。
- R3：boot=346s（帽 600s）/ 首拍=0s / 均拍=59s（下界 40）/ n=60
  last_sw=68795（sw0=33075 / sw1=35720 双核都在切）/ Δsw1min=553
  （地板 320）/ pages=13579..14563（pmin..pmax 含首 5 拍豁免窗观测，
  稳态执法窗 pbase..pbase+64 内）/ churn=11067 严格递增 / parkD0=D1=0。

## 验证矩阵（G3-5 收口态）

| 项 | 结果 |
|----|------|
| cargo build --features lockdep | ok（增量 2.09s；镜像 marker 自证 strings 命中恰 1） |
| SOAKG3_CALIBRATE=1 标定首跑（8 拍） | PASS：八断言全绿 + lockdep 三计数绿 + lat 双路完整（观测 ipc max=6063 / sgi max=20635000；回执 soakg3-pilot-20261105.log） |
| 首银行跑（60 拍，帽执法） | **PASS**：60/60 八断言全绿 + lockdep 三计数绿（VIOLATION 0）+ lat 帽内（5750/20678750 ≤ 18189/61905000；回执 soakg3-bank-20261105.log） |
| 镜像自证门（default 面直拒） | 在位：marker 串 strings 验前置（smoke-lockdep 同款） |
| 帽值推导 | 观测 ×3.0（B-0 界同款裕量），环境变量 LAT_CAP_IPC/LAT_CAP_SGI 可覆盖（跨机 B-2 纪律） |
| 转正流程启动 | §5 后继刀候选立案：①常驻再装形 lat 载荷（周期重启采样——须先侦察舰队 respawn 僵尸累积 vs pages 稳态窗冲突面）②lockdep 面稳定 N 跑后转正评估（N 参照 S-5d 银行跑先例首标定） |
