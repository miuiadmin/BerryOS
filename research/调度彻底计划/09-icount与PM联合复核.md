# 09 号：icount 基线刷新与 PM 四 crate 联合复核——两 wave 只验不修

> 复核日期：2026-11-15（同机续轮，Apple M3 Pro / 18GB / macOS 27.2.1 / QEMU 11.0.3 Homebrew，与 16 号定标同机同版本）。复核对象：两 wave——Wave1 icount 基线刷新（16 号 §4.1 四项漂移出 L2 后 `bench.py --icount --update` 立基 {0,37,82,274,1683,6948}，[基线](../../tools/bench_baseline_icount.txt) + [史档新段](../../tools/bench_baseline_icount.notes.md) :88-99）；Wave2 PM 四 crate 联合场景（`user/pm-integration/` 独立小工作区三场景 14 测 + [实施笔记](../../.agents/notes/implemented/2026-11-15-pm-four-crate-joint-scenarios.md)）。任务：只验不修——①icount 新旧对照（四项漂移是否收口）与 L2 复验实跑；②PM 三场景断言正确性（数据流/类型匹配/边界）；③门禁 + git 对拍。纪律：零修零碰（本件外零仓写）；禁 git add/commit/push；同机同模式内刷新铁律逐项核（跨机/跨模式/跨形态不可比）；推断显式标注。

---

## §0 结论速览（六条）

1. **Wave1 icount 刷新复核通过、四项漂移全部收口**：基线 diff 六项中五项换值——四漂移项（alloc64k 37→82 / handle_evt 165→274 / commit1p 1617→1683 / rw4k 6937→6948）逐项与 16 号 §4.1（[16 号](../显示与加速体系设计/16-v2定标与icount接入.md) :115-121）对上，另 alloc4k 82→37 为重建 ELF 布局重掷随迁（旧 ELF 上 16 号实测 82 逐值不动是另一棵树的事）；新值 {0,37,82,274,1683,6948} 与 prove 五跑（回执 icount-prove-20261115-015848-50319，verdict=exact max_dev=0.000%）逐值同。
2. **L2 复验本轮独立实跑 PASS**：同机同模式同款 ELF（`out/bench-target/.../berry-kernel`，sha256 `b43f12d4…` 与立基回执同指纹，本轮复核前亲验）单跑 `bench.py --icount` → 六项 {0,37,82,274,1683,6948} 与新基线**逐值相同**、BENCHWAKE 观测行与 prove 五跑**逐字节同**（n=582 min=1188 … max=107605622 sig=563 tmo=19）、`L2 对拍过（ICOUNT_TOL=0.0，6 项全在界内）` RC=0（回执 icount-20261115-020708-64394）——L2 恒红解除，基线与现役树重新挂钩。
3. **时间线澄清（树一致性成立）**：`kernel/` 最新动源提交 5187b0ce（dpg 看门狗，11-15 00:10:41）**早于**立基树 96072f55（00:54:06）——立基 ELF（01:23 构建）已含该面；其后仅 d8f2254e/87c06016 两笔 berry-ui 提交（01:32/01:52），kernel/ 与四路径依赖 crate 零改动，工作树 kernel/ 干净——基线对现役 kernel 面有效，notes :94「owner 并行提交零 kernel 改动」声称与 git 实证一致。
4. **Wave2 PM 联合复核通过、三场景断言逐项咬合四 crate 真值**：14 测全绿（5 钉板 + A/B/C 各 3）；钉板值全部与被测件单源对上（thermal 档表/JEITA 表、power 低电档、EM QEMU 三点表、slackface 六档——§3 逐项）；两处最易错数值我独立手算复验吻合（C-3 EWMA 折叠链 0→128→…→752 与 req=734375、B-1 四桶几何 window/span——§3.2/§3.4）；mA 直连无桥（类型 u32 同域、哨兵跨 crate 同值钉板）成立。
5. **零碰与实跑面全过**：四被测件 + pwproto git 零改动；四件独立测试复跑全绿（31+13+8+12，与「只读依赖零回归」声称一致）；lib 桥面 `--lib --target aarch64-unknown-none` 真编过（2 warning 为 none-target rustflags 既存面）；Cargo.lock 7 条目纯 path 零外部源。
6. **门禁 + git 对拍通过**：docs_guard PASS（0 警）；check-docs 本件落盘后复跑绿；本批仓内足迹 = 基线两文件 M（Wave1 `--update` 产出 + notes 追加）+ `user/pm-integration/` 未跟踪 + 实施笔记未跟踪，除此之外零足迹；justfile M（+99 行）经 diff 核对无本批两 wave 相关改动（owner 面）。瑕疵两条均轻（§6）：C-1 第 0 拍 NORMAL 档 min 归 JEITA 的 decide 面无直证；notes 新段标题「四漂移」未概括 alloc4k 随迁项。

## §1 Wave1 新旧对照——四漂移收口与第五项随迁

基线 diff（`tools/bench_baseline_icount.txt` :14-19，2026-11-05 值 → 2026-11-15 值）与两源对拍：

| 项 | 旧基线 | 16 号实测（旧 ELF `10ff7ef0`） | 新基线 | 本轮复跑 | 判定 |
|---|---|---|---|---|---|
| cntvct_pair | 0 | 0 | 0 | 0 | 不动 ✓ |
| frame_alloc4k | 82 | 82 | **37** | 37 | 随迁（重建重掷，§1-注） |
| frame_alloc64k | 37 | 82 | **82** | 82 | 漂移①收口 ✓ |
| handle_evt | 165 | 274 | **274** | 274 | 漂移②收口 ✓ |
| vmo_commit1p | 1617 | 1728 | **1683** | 1683 | 漂移③收口（取重建 ELF 值非旧 ELF 1728）✓ |
| vmo_rw4k | 6937 | 6948 | **6948** | 6948 | 漂移④收口 ✓ |

- **四漂移口径与 16 号 §4.1 逐项对上**：16 号 :119-122 四项漂移值（+121.6%/+66.1%/+6.9%/+0.16%）正是上表第 3 列；新基线对 handle_evt/rw4k 直接跟随、alloc64k 跟随、commit1p 取重建 ELF 值 1683——notes :92-93 双证链（同 ELF 复现 {0,82,82,274,1728,6948} 回执 icount-20261115-012536-98750；现役树重建 {0,37,82,274,1683,6948} 回执 icount-20261115-012548-8971）本轮逐回执核对数值一致，两 BENCHWAKE 行（min=1193/max=107587192 与 min=1188/max=107605622）亦与回执在档行逐字节同。
- **第五项随迁的正当性**：alloc4k 82→37 与 commit1p 1728→1683 均为「重建 ELF 增量重掷布局骰」（ktest.rs P3-5 面 +2808B + ident 串形变——notes :93），且 37 与 4-2 立档轨道域同族〔推断，notes 原标注承袭〕；16 号 §4.1「frame_alloc4k 逐值不动」描述的是旧 ELF 面（基线 82 vs 实测 82），与重建后取 37 不矛盾——两棵树两次掷骰。
- **证明链完整**：prove 五跑 verdict=exact max_dev=0.000% + BENCHWAKE 五跑逐字节同（回执尾部两行亲读）；`--update` 回执 015909-59107 argv 带 `--kernel out/bench-target/...`（与 prove 同款 ELF）；复验单跑 015933-59174 与第二 ELF 独立构建单跑 015907-58704（可重现性双证）数值全同。ICOUNT_TOL=0.0 维持（`tools/bench.py` :285 单源常量，L2 形态=精确相等）合规——prove exact 是开门前置，五跑自洽成立。

## §2 Wave1 L2 复验（本轮独立实跑）

命令 `python3 tools/bench.py --icount --kernel out/bench-target/aarch64-unknown-none/debug/berry-kernel`（与立基同机、同模式 `-icount shift=1,sleep=off` 裸跑 128M virt、同款 ELF——sha256 `b43f12d47518…` 复核前亲验与 notes :92 声称同指纹）。结果：boot→done 2.0s wall（帽 120s）；六项 cyc 逐值 = 新基线；BENCHWAKE 观测行与 prove 五跑逐字节同；`bench.py: L2 对拍过（ICOUNT_TOL=0.0，6 项全在界内）`，RC=0。对拍器语义核读：门禁面 = 六行 BENCH 行锚定收割 + `BENCH ok n=` 计数一致性（`tools/bench.py` :1146-1166），界带 = `[old×(1−tol), old×(1+tol)]` 双侧 + 0 基线 0→非零无界红（:1417-1441 `_l2_bad` 家族镜像）——tol=0.0 下即逐值精确相等，本轮六项全过 = 收口坐实。铁律面：本轮全程未跨机、未跨模式、未跨形态（未带 trace=off 臂/桌面档参数），16 号 §4.3 的「后端维度不可比」注脚未被触碰。

## §3 Wave2 PM 三场景断言正确性——逐项咬合

### 3.0 形态与契约钉板

`user/pm-integration/Cargo.toml`：空 `[workspace]` escape、四 crate + pwproto 只读 path 依赖（:36-44）；lib 面 `#![cfg_attr(not(test), no_std)]`（`src/lib.rs` :60）与四 crate 同族。契约钉板五测（`src/lib.rs` :143-222）逐项与被测件单源对上：不限哨兵 `CHARGE_NO_CAP_MA`（thermal `lib.rs` :105）== `CHG_LIMIT_UNLIMITED_MA`（power）== u32::MAX ✓；桥 1 fmax 基 == EM 表顶点（`PolicyParams::default_sim().fmax_khz`=1_000_000 == `QEMU_VIRT_DUAL` 顶频点，[berry-em](../../user/berry-em/src/lib.rs) :110-115 三点表 {250kHz,400}/{500kHz,750}/{1GHz,1024} 亲读）✓；档表 `LEVEL_FREQ_CAP_PCT=[FREQ_CAP_NONE_PCT,70,45,25]`（thermal :98-99）✓；席位号在册 ✓。

### 3.1 场景 A（热降频全链）——聚合 min 与归因

A-1（`scenario_a.rs` :41-80）五拍 ramp：迟滞链手算验（threshold 60/70/70k、clr 55/65/75k；回落 50k < 55k 全清 → level 0）✓；三源 min 手算验（min(700,600,500)=500@LOWBATT → min(450,600,500)=450@THERMAL → 250@THERMAL → 回落 500@LOWBATT）✓；mA 票 [1500,1200,1000,0] 与 `LEVEL_CHARGE_LIMIT_MA=[MAX,1200,1000,0]`（thermal :110-111）+ JEITA NORMAL 1500 的 min(1500,档) 折叠自洽 ✓；低电帽 500kHz/预充票 2400 与 power 档（`user/bins/berry-power/src/lib.rs` :85-88 `lowbatt_cap_pct=15/lowbatt_cap_khz=500_000/precharge_pct=10/precharge_limit_ma=2_400`）✓。A-2（:87-115）帽截域容量阶梯 1024→750→400 与 EM 表算术吻合；「边际 1250 < 5750」钉值出处 = berry-em 独立测试面（`lib.rs` :532 原句在档）；fit 门 `task_util <= em_max_capacity_of`（:290）使 800>750 → None、600≤750 → Some(0) 语义验 ✓。A-3（:122-166）坏拍 500°C 出可信域 [-100,200]°C（thermal :145-147/:226）整拍 Err、`OutputLatch::commit`（:474-484）保持前值、系统档 = 含停报者的锁存档 max（:374）——「Err 不触席、帽保持」失败安全链断言与被测件语义逐条对上 ✓。

### 3.2 场景 B（slack+EM 联合）——分桶几何手算复验

B-1（`scenario_b.rs` :81-124）：六档域值与 slackface 单源全同（`user/berry-slackface/src/lib.rs` :91-103：LC=0/INT=10ms/UNSPEC=UTILITY=50ms/BG=500ms/MAINT=10s）✓；**四桶几何独立复算吻合**——window=fire−最晚成员 deadline、span=fire−最早 deadline：b1 fire=100（LC t1 钉点，t0 INT 95 顺延 +5≤10ms 预算）→(0,5ms)；b2 fire=162（t2 UNSPEC 112 顺延 +50ms 恰用满）→(4ms,50ms)；b3 fire=215（t6 INT 205 顺延 +10ms 恰满）→(10ms,15ms)；b4 fire=350（t8 系统缺省 300 顺延 +50ms 恰满）→(10ms,100ms)——四桶 8 个几何值全对；三处「预算恰用满」边界恰点（L2/L3 律只禁越界）与 `laws_hold` 批全过一致 ✓。B-2（:132-159）：批需求和手算 [250,700,200,750] 逐桶验（150+100/200+200+300/50+150/400+100+250）✓；服务档 `capacity_for_util` = 最低 capacity≥util 档（berry-em :148-161）：250→400、700→750、200→400、750→750 ✓；对抗件 t7 400→800 → 1150>1024 且 overutil（阈 819 同值同律引 berry-emface）✓；d4=1150 断言隐含 b4 members 不变（工作量账不进 slack 几何）✓。B-3（:166-199）：serious 帽 450kHz 聚合胜出 → 域顶容 400 → b2(700) 放不下+overutil / b1(250) 可行——四 crate 一拍串完的翻转链，各环节值全验 ✓。

### 3.3 场景 C（充电票+关机边界）——mA 直连与哨兵

C-1（`scenario_c.rs` :38-90）：JEITA 四温区票 [1500,750,500,0] 与 `JEITA_TABLE`（thermal :137-143：NORMAL 10..45→1500/WARM 45..60→750/COOL 0..10→500/HOT ≥60→0）逐值同；60°C 恰归 HOT（左闭右开保守侧）✓；`vote_pct` pct→mA 换算（power `charge.rs` :165-177 `ma_of_pct`）80%×4800=3840、25%→1200 与 berry-power 自测钉值（50%→2400）同构 ✓；min 归因切换（1200@USER → 750@JEITA → 500@JEITA → 0@JEITA）手算全对 ✓。C-2（:98-131）：全席缺席→{MAX,NONE}、单席不限仍归因、哨兵 min 穿透、平票席号小者（JEITA）、禁充 0 压一切、`limit_pct` 还原（2400/4800=50%、750→15% 向下取整、MAX→100%、容量 0→None）——算术与语义逐条验 ✓。C-3（:139-199）：critical 三面同拍 (3,25,0) ✓；**EWMA 折叠链独立手算复验吻合**——α=1/8 整数截断 x←x+(1024−x)/8：0→128→240→338→423(423.75 截)→498→563→620→670→714→752，req=752/1024×1GHz=734_375 kHz（笔记 :25「曾误 741→723632，实跑钉板 734375」——本轮手算坐实 734375 正确）；eff=min(734375, 250000)=250000「唯一输出面吃 eff」✓；坏拍锁存保持三面、battery 帧轴与 JEITA 轴独立（50% 回升撤低电票而 JEITA 0 票不动）✓。

### 3.4 类型匹配与两桥

mA 直连面类型全同（`Verdict.charge_limit_ma: u32` → `ChgArbiter::vote(slot, ma: u32, reason)`，无宽化/ narrowing/换算）——「F1 终裁后无桥」声称成立；帽面两类型异构（thermal `freq_cap_pct: u8` vs power `FreqCapAgg::set` kHz）恰由桥 1 承接，且桥全【推断标注】+「桥非裁决、接线刀裁走后退役」边界自认（`src/lib.rs` :27-46 模块注）——04 号复核 §6-2「pct↔kHz ABI 未立法」的欠账未被私立法，合规。桥 1 拒面（0/越界/fmax=0）与向下取整只收紧（33%×999_999=329_999.67→329_999）钉板 ✓；桥 2 帽截 OPP 留 `freq_khz ≤ cap` 频点、帽低于最低频点 → 空域、`em_domain_from` 单源装配形状执法 ✓。

## §4 Wave2 实跑面（本轮独立复跑，2026-11-15）

- `CARGO_TARGET_DIR=target-cargo cargo test --manifest-path user/pm-integration/Cargo.toml --target aarch64-apple-darwin`：**14 passed / 0 failed**（contract_pins 5 + scenario_a 3 + scenario_b 3 + scenario_c 3，测试名逐条在档）——与笔记 :25 声称同数。
- 四被测件独立复跑（各自 manifest，同 host target）：berry-power **31** / berry-thermal **13** / berry-em **8** / berry-slackface **12**，全绿——与「31+13+8+12 复跑零回归」声称同数。
- lib 桥面 `--lib --target aarch64-unknown-none` 真编过（Finished dev，2 warning 与四 crate 同款既存面）；`Cargo.lock` 7 package 全 path、`source =` 零条目（零外部依赖）。
- 工作区体检：`user/pm-integration/` 仅 Cargo.toml/Cargo.lock/src 五文件，无杂物；四被测件 + pwproto `git status` 零 M——「零改其源」声称坐实。

## §5 门禁 + git 对拍

- **docs_guard**：`python3 tools/docs_guard.py` → PASS（0 警，G1-G10 全 ok）。
- **check-docs**：本件落盘后 `node .agents/tools/check-docs.mjs` 复跑绿（见 §7 时点）。
- **git 对拍**：本批两 wave 仓内足迹清单 = `M tools/bench_baseline_icount.txt` + `M tools/bench_baseline_icount.notes.md`（Wave1 `--update` 整文件重写 + 史档追加段 :88-99）+ `?? user/pm-integration/` + `?? .agents/notes/implemented/2026-11-15-pm-four-crate-joint-scenarios.md`——除此之外两 wave 零足迹。任务书「开刀前复查」项复核：bench 两文件的 M 即 Wave1 自身产出（基线空闲时点已过，diff 内容与回执数值闭环），非他人占用；pm-integration 未跟踪即 Wave2 产出。`M justfile`（+99 行）diff 核对无 pm-integration/icount 相关改动——owner 面在途，与两 wave 零交集；`M user/bins/guifbd` 等其余 M 件同为他线（vnc/berry-ui 会话簇）。本轮纪律：零 git add/commit/push，仓写仅本件。

## §6 瑕疵与残留清单

- **瑕疵①（断言覆盖，轻）**：C-1 第 0 拍（NORMAL，1500）在 vote_pct(USER,25%) 收紧后 `continue`——该拍 `decide()` 只验了 1200@USER 分支，NORMAL 温区下 min 归 JEITA（1500）的 decide 面无直证；JEITA 归因 decide 面由后三拍（750/500/0）覆盖、出票面 1500 已断言，机制面无洞，组合面少一拍。
- **瑕疵②（措辞，轻）**：notes.md :88 新段标题「16 号四漂移基线刷新」未概括第五项随迁（alloc4k 82→37 同批入基线）；正文 :92-93 有完整解释（布局三项重掷），标题概括面略窄——只读标题者会漏 alloc4k 也换了值。
- **残留①（在册挂账，非本轮义务）**：handle_evt +66.1% 精确归因仍挂 bisect（16 号 §4.1〔推断〕+ notes :93 承袭标注——两处均如实未代裁，本复核不重归因）。
- **残留②（留窗在册）**：justfile test-host 未挂 pm-integration（justfile=owner 占用零碰，笔记 :25 如实注记，04 号复核 F3 同款）；两桥随 CP-08/CP-09 立法退役。

## §7 复核方法与产物

icount：基线 git diff 全量核读；七份回执逐份读数（012536/012548/015840-846/015848-prove/015907/015909/015933 + 本轮 020708-64394，`out/bench-receipts/`）；16 号 §4.1 原文对拍；时间线 `git log --format='%h %ci %s'` 八笔 + `git status kernel/`；ELF sha256 `shasum -a 256` 亲算；L2 复验实跑一轮（QEMU 串行守卫窗口期完成）。PM：`user/pm-integration/` 七文件全文读；四 crate 常量/函数面 grep+read 直读（thermal :98-147/:374/:474-484、power lib :85-88 + charge :165-177、em :110-161/:290/:532、slackface :91-103）；三处关键数值独立手算（EWMA 链与 req、四桶 window/span、四桶批需求和）；测试三轮实跑（14 + 31+13+8+12 + none-target lib 构建）；lock 纯度 grep。门禁：docs_guard + check-docs。引用形态：本件路径:行号均为 2026-11-15 工作树时点（基线两 M 件与 pm-integration 为未提交态，行号随其落盘态；后续漂移按仓内惯例不回改）。

## 来源索引

- [16 号：v2 定标与 icount 接入](../显示与加速体系设计/16-v2定标与icount接入.md)（§4.1 四漂移定标 :113-126、§4.3 五源锚 :137-154）
- [bench_baseline_icount.txt](../../tools/bench_baseline_icount.txt)（新基线 :14-19）/[bench_baseline_icount.notes.md](../../tools/bench_baseline_icount.notes.md)（刷新史档 :88-99）
- `tools/bench.py`（ICOUNT_TOL :285、run_icount :1064-1171、_l2_bad :1417-1441）；回执 `out/bench-receipts/icount-20261115-*.log` 七份 + 本轮 `icount-20261115-020708-64394.log`
- [PM 四 crate 联合场景实施笔记](../../.agents/notes/implemented/2026-11-15-pm-four-crate-joint-scenarios.md)；`user/pm-integration/`（Cargo.toml、src/{lib,common,scenario_a,scenario_b,scenario_c}.rs）
- [04 号：PM 双服务骨架复核](04-PM双服务骨架复核.md)（§6-2 pct↔kHz ABI 未立法、F3 门禁留窗）；[berry-em](../../user/berry-em/src/lib.rs)、[berry-slackface](../../user/berry-slackface/src/lib.rs)、`user/bins/berry-power/src/{lib,charge}.rs`、`user/bins/berry-thermal/src/lib.rs`
