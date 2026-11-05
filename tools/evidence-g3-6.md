# G3-6 证据档：关卡收官——全要件复验 + 测试策略 G3 行转「已过」（草案 §6）

> 刀：G3-6 关卡收官——延迟基准 + lockdep + trace 双过 + soak 全要件复验，
> `docs/测试策略.md` G3 行转「已过」+ 话术落档。日期：2026-11-05。

## 树一致性定谳（复验口径的基础）

- `git log e74c7f4..HEAD -- kernel/ sched/ arch/ mm/ ipc/ bootloader/
  drivers/ fs/ net/ user/` = **零提交**——f4 修复批（e74c7f4，最后一次内核
  源增量）之后的 93606c7 / 00ec23f 全为 docs/tools 增量。
- **推论**：本日全部回执（09:30 f4 复验批 + 12:13 soak-g3 银行跑）运行的
  内核工件 = 最终树内核源所出同二进制——**G3-6 复验 = 回执面全引 +
  树一致性论证**，不重跑同工件的重复跑（上午 icount prove 五跑 exact
  零漂移已证同源确定性；重跑 2h 电池对同 ELF 零信息增量——S-5d
  「环境不可复现、段内为准」纪律同族口径）。收官刀自有新鲜回执两卷：
  testhost-g36 + bench-g36（本日最终 docs 态下 docs_guard/abi-check 复验
  ——docs 在上午回执后确有增量，此面重验非冗余）。

## G3 判据裁定矩阵（测试策略 G3 行三要件）

| 判据要件 | 裁定 | 证据 |
|---------|------|------|
| **低尾延迟**（延迟基准） | 毕 | bench-lat 双路在册：ipc p50 ≈13-30µs（同核直切亚百微秒，P-B 直切面收益实证）/ sgi min ≈291µs 量级（跨核机件下界，p50 被 TCG 宿主 vCPU 调度主导——诚实分解测量）；尾延迟帽进激进 soak 断言面执法（soak-g3 双路 max 帽内，帽占 32%/33%） |
| **无优先级反转死锁**（lockdep） | 毕 | lockdep 面：selftest ok 恰 1（六阳性三负对照）+ **1h 激进 soak VIOLATION 0**（60 拍满载双核零锁序违约）+ 误报率 = 0%（全 lockdep 面跑零 VIOLATION 零 selftest FAIL——§3 kill criteria 误报率 <10% 大裕量通过）；饿死面 = soak 活性断言族（Δsw1 地板/park 越窗/均拍下界——满载下无饿死实证） |
| **trace 开/关两套基准都过** | 毕 | trace-dual：TRACE_CAP=0.0 首标定（六行关/开逐值恰等 = 结构性零——bench 窗全在调度器纪元前的串行 boot 段，遥测钩子窗口内结构性不可能发射）+ 双基线入库（bench_baseline_icount{,_traceoff}.txt）+ off 臂 F14 total()==0 真空断言活检验 |
| smoke 族全绿（复验基线） | 毕 | smoke 八跑 PASS（金丝雀 43 严格递增 + rootfs 47 + RX 相 + FAT 子目录/持久化 + B4 掉电三相）+ smoke-smp 八相 PASS（LB/ASIDR/TLBS/ALLOC/HT/PP 全门）——回执 smoke/smoke-smp-20261105.log |
| 收官新鲜回执 | 毕 | test-host 78 suite 895 passed 0 failed exit=0（testhost-g36-20261105.log）+ just bench B-0 六项界内 + docs_guard 0 警 + abi-check PASS（bench-g36-20261105.log——最终 docs 态复验） |

## 刀链闭环总览（G3-1~G3-6）

| 刀 | 面 | 提交 |
|----|----|------|
| G3-1 抢占点加密（P-A） | preempt_disable + 检查点族 + SVC 首域解蔽 | （沿革 G3-1 段） |
| G3-2 preempt-on-wake（P-B） | 通用唤醒直切 + 段②收窄定谳（rx 尾检查点） | （沿革 G3-2 段） |
| G3-3 lockdep | feature + 环检测 + selftest + smoke-lockdep 执法面 | （沿革 G3-3 段） |
| G3-4 bench-lat + trace 双跑 | IPC 路 + SGI 路（§A127/§A128 判例）+ TRACE_CAP=0.0 + 双基线 | 98332e9 + b22d040 |
| G3-4c f4 修复批 | F1-F15 全回写 + 回执面立档 | e74c7f4 + 93606c7 |
| G3-5 激进 soak | soak-g3 模板 + 60 拍银行跑 PASS + 转正流程启动 | 00ec23f |
| G3-6 关卡收官 | 本档：判据裁定矩阵 + 树一致性 + 收官回执 + 测试策略转已过 | （本刀） |

## 验证矩阵（G3-6 收口态）

| 项 | 结果 |
|----|------|
| 树一致性（`git log e74c7f4..HEAD` 内核十路径） | 零提交——全日回执 = 最终树同二进制（复验口径成立） |
| just test-host（收官新鲜） | PASS：78 suite / 895 passed / 0 failed / exit 0（testhost-g36-20261105.log） |
| just bench（收官新鲜，docs_guard + abi-check 前置同卷） | PASS：B-0 ×3.0 六项在界（3/28/18/26/261/759）+ docs_guard 0 警 + abi-check NR v1 34 号（bench-g36-20261105.log） |
| bench-lat 双路（引 0930 批回执） | PASS：双路齐发零 panic + LATSGI 落核断言活检（lat-20261105-093156-41389.log） |
| trace-dual（引 0930 批回执） | PASS：TRACE_CAP=0.0 六行 δ=0 + 双家族基线 12 项 + off 臂 total()==0 真空断言（trace-dual-20261105-093122-41343.log） |
| icount prove（引 0930 批回执） | exact（五跑零漂移，双基线对拍过门——同源确定性 = 树一致性论证的一翼） |
| lockdep 面（引回执） | selftest ok 恰 1 / FAIL 0 + 误报率 0%（smoke-lockdep 快启面 + soakg3 银行跑零 VIOLATION） |
| soak-g3 银行跑（引回执） | PASS 60/60：VIOLATION 0 + lat 帽内（帽占 32%/33%）（soakg3-bank-20261105.log） |
| smoke 族（引 0930 批回执） | 主八跑 PASS（金丝雀 43 严格递增，smoke-20261105.log）+ smoke-smp 八相 PASS（smoke-smp-20261105.log） |

## 限界与移交（G4 前）

- 转正流程两候选在册（草案 §8 G3-5 段 E）：①常驻再装形 lat ②lockdep
  N 跑转正评估——G3 关卡已过不阻塞两件，按 §5 转正节奏推进。
- TCG 口径限界承 B-2/§A118：绝对值跨机不可比，段内对照为准；真机
  复测归后续真机面。
