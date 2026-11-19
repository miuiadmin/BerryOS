# BerryOS -icount 基线史档（bench_baseline_icount.txt 头注迁出件）

> 2026-11-04 m1 迁移（g3-1d errata-lite，f4 三镜 follow-up）：`--icount --update`
> 按 bench.py 内嵌模板**整文件重写**，会抹掉 txt 里手工追记的史档/勘误头注
> （g3-1d 已实证一次，沿革五十四续）——史档迁本文件，txt 只留指针行
> （load 侧跳 `#` 行，同形契约不变）。本文件不受 `--update` 影响，新条目按日期追记。

## g3-1c-2 定态位移（2026-11-04，prove 五跑 max_dev 0.000%；errata F1 勘正版，头注原 tick 相位移归因系虚构作废——沿革四十九续勘正段 + 五十续）

位移机制 = §A95/§A109 **布局骰轨道回摆**：frame_alloc4k 171→37 / vmo_commit1p
1708→1574 是 ±134 同签名轨道态（沿革 :1385/:1416 在册 {37,1574}→{171,1708}→回摆），
区间代码增量重掷 boot 布局骰（基线 171/1708 烘于 6b7a745）。双 ELF 段足迹
diff（c49de69 vs 8cbf832 同机同链，隔离 worktree 冷构；**对照区间含 3323b87
phone20 buddy 面——+2048 B 为区间合量非 8cbf832 独量**）：PT_LOAD 五段中唯
高半可执行段 246880→248928（+2048 B），rodata/data/bss 三段零变。更硬论据
= 轨道签名本身：两项精确落历史轨道对 {37,1574} 即零算法增量签名——若
3323b87 真改 order-0 热径，frame_alloc4k 不会精确落回 37（顺带洗清其
order-0 面）。tick 归因四重封死（f4 三镜）：测量位（bench::run，boot 窗）在
timer::init/arm_slice/enable_irq 前且 boot DAIF 屏蔽 = 物理无 tick 落点；两漂移
项恰同 −134/op 而迭代数 1024 vs 256（固定 tick 成本不可同摊）；最长窗项
vmo_rw4k 888k cyc 6937 分文不动。prove exact 定态观测与基线数字（真实测量）
本身有效。

## g3-1d 烘焙（2026-11-04，prove 五跑 max_dev 0.000%）

vmo_commit1p 1574→1572（−2）唯一移项，余五项分毫不动——**非**轨道对翻转
（{37,1574}↔{171,1708} 未换轨），系区间合量微布局骰（6b7a745 烘基后
3323b87/8cbf832/b495fb0/53a381e/68fea8b + g3-1d 增量叠掷；68fea8b RAM_TOP
256MiB 使 mm::FRAME 三静态数组 ×2 = bss 段 248688→372464（+123776，f4
复验对账），text 段 248928 不变——布局重掷而热径零算法增量）。

## g3-1d errata-lite 批（2026-11-04，prove 五跑 max_dev 0.000%）

vmo_commit1p 1572→**1574**（+2 回摆），余五项分毫不动——p1 断言消息串变更
（fa_ping ckpt 断言语义正名 = rodata 常量变更）重掷布局骰，幅度与 g3-1d
（−2）同量级反向，归因唯一干净。本条为 notes.md 迁移后首例 --update 烘基
（指针行存活实证 = m1 契约自验过）。

## G3-4a 烘基（2026-11-05，prove 五跑 max_dev 0.000%；f4 P-2 补档——本条
当时漏记，erratum 见沿革六十二续勘误行）

基线 {37,1574}→{172,1706}（frame_alloc4k/vmo_commit1p 两行，余四行分毫
不动）。归因 = §A95/§A109 **轨道对翻转**：{172,1706} = 在册轨道对
{171,1708} 加微位移（+1/−2——G3-4a 新码〔Task.wake_vct 增域 + note_wake
面 + bench-lat IPC 路静态〕叠掷布局骰）。**勘误**：当时三处（evidence
-g3-4a.md 验证矩阵行 / 沿革六十二续 / 提交 98332e9 消息）曾归因「note_wake
戳+Task 增域的 icount 足迹」——机制上不可能（f4 P-1 实证：bench::run()
前置 berry_sched::init()，note_wake 调用面在 bench 窗内不可达；alloc/commit
环无切换无唤醒）。原始记录不回写（append-only 纪律），本条 + evidence
勘误行承修正。判据复盘：两漂移项落历史轨道邻域 + 反向可达（1708→1706 变
快）即零算法增量签名——「新戳有足迹」的机制叙事是后验合理化（P-1 教训：
归因先验机制可达性）。

## G3-4b 批（2026-11-05，prove 五跑 max_dev 0.000%；SGI 路 + wait_one 三钩子
+ trace 双跑门）

基线 {172,37,166,1706,6937}→{82,37,165,1617,6937}：frame_alloc4k −90 /
vmo_commit1p −89 **反向漂移**（排除 envelope 假设——加码只会变慢；G3-4b
新镜像内容〔SGI 静态/EL0 载荷 blob/waker 任务/三钩子/trace 门〕重掷布局
骰与 tick 相位，buddy 两相位敏感行换轨）；handle_evt 166→165（−1 微位移，
§A95 族）；cntvct_pair/vmo_rw4k/frame_alloc64k 逐值不动（tick 无关窗）。
**wait_one 三钩子未在任一测量窗显形**——与 bench 前置调度器纪元一致（P-1
机制面反向承重），轨迹与 G3-4a 勘误同源。BENCHWAKE 五跑逐字节同
（n=209 max=46257624）。

### G3-4b trace 双跑 off 臂立档（同日，五跑逐值相同）

off 臂（opt/berry/trace=0）五跑 {0,82,37,165,1617,6937} 逐值相同 = off 家族
bench_baseline_icount_traceoff.txt L2 资格。关-开六行全零差 = **结构性零**
（bench 六窗前置调度器纪元，遥测钩子窗口内不可能发射——TRACE_CAP=0.0
首标定即此定谳，单一事实源在 bench.py 常量注）。

## f4 修复批复验（2026-11-05，prove 五跑 max_dev=0.000%，对在库基线零漂移）

**勘误〔f4 F1〕**：G3-4a 烘基 / G3-4b 批 / off 臂五跑三组 icount 数值主张
当时**无档可引**——六份存档日志不含 icount 跑（run_icount tempfile 即跑即
删；「off 臂五跑」无工具通道）。修复 = bench.py 回执面（`write_receipt`
落 `out/bench-receipts/`，gitignored 机内持久；argv 头 + summary + 全
serial），本批起一切 bench 数值主张有档。复验五跑
{0,82,37,165,1617,6937} **与在库双基线逐值同**——修复批内核增量（F3
LATSGI_CPU 静态+断言 / F4 界断言 / F11 Release-Acquire / F14 total()==0
断言 / 三条注释）**零布局漂移**（断言与注释全冷径；静态并入既有段对齐），
双基线不重烘直接续役。BENCHWAKE 五跑逐字节同（n=209 max=46257626——
观测面 max 与 G3-4b 批的 46257624 差 2 cyc：wake 分布统计行非 L2 面，
五跑内自洽即定态）。回执：icount 五份 + icount-prove + icount/off +
trace-dual + lat + reject-trio + testhost（同目录 20261105-0930~0931 批）。

## 16 号四漂移基线刷新（2026-11-15，prove 五跑 max_dev 0.000%）

背景 = 16 号 §4.1 定标（2026-11-14）：基线（2026-11-05 f4 复验值 {0,82,37,165,1617,6937}）与现役树脱钩四项出 L2（alloc64k 37→82 / handle_evt 165→274 / commit1p 1617→1728 / rw4k 6937→6948）——L2 恒红，基线空闲可刷新（本条 = 刷新烘焙）。

复现双证：①同 ELF 复现——11-14 02:01 构建内核（sha256 `10ff7ef0…`，banner `v0.2.0-197-g429675903-dirty`，即 16 号实测件）单跑 {0,82,82,274,1728,6948} 与 16 号双跑逐值同、BENCHWAKE 整行逐字节同（n=582 min=1193 … max=107587192 sig=563 tmo=19）——四漂移稳定复现坐实（回执 icount-20261115-012536-98750）；②现役树重建——kernel/ @ 96072f55（ktest.rs P3-5 面为唯一晚于旧 ELF 的源文件，11-14 14:30）重建 ELF（sha256 `b43f12d4…`，banner `96072f55-dirty`——本分支 git describe 回退 rev-parse 短哈希形）单跑 {0,37,82,274,1683,6948}（回执 icount-20261115-012548-8971）。

新值归因：稳定三项（cntvct_pair 0 / handle_evt 274 / vmo_rw4k 6948）跨双 ELF 逐值同——五源锚（16 号 §4.3）再添两源；handle_evt +66.1% 归因承 16 号〔推断——sched 系提交（P2-1 sleepq per-key/P2-2 退出异步化等）候选，精确归因须 bisect 挂账〕，rw4k +0.16% 微量拷贝路径稳定项同判。布局三项重掷：alloc64k 37→82 = 4-2/4-3 轨道振荡族；alloc4k 82→37 与 commit1p 1728→1683（对旧 ELF 的差）= 重建 ELF 增量（ktest.rs 面 +2808B + ident 串形变）重掷布局骰，轨道域与 4-2 立档值 {37,…} 同族〔推断〕。

烘焙与验证：prove 五跑 verdict=exact max_dev=0.000%（BENCHWAKE 五跑逐字节同 n=582 min=1188 … max=107605622；回执 icount-prove-20261115-015848-50319）→ ICOUNT_TOL=0.0 维持；`--icount --update` 立基 {0,37,82,274,1683,6948}（头注模板整文件重写，指针行存活 = m1 契约再验；回执 icount-20261115-015909-59107）；复验单跑 L2 对拍过（ICOUNT_TOL=0.0 六项全在界内，回执 icount-20261115-015933-59174）。构建可重现性双证：双 target 独立构建 PT_LOAD 五段 offset/va/filesz 逐段同、加载域内唯一差异 = git ident 数据串（同长度非执行面），第二 ELF 单跑逐值同含 BENCHWAKE 整行（icount-20261115-015907-58704）——同树重建落同值，基线对后续 cargo build 产物有效。

环境注记：宿主多会话并发（在场 QEMU 2-5 pids 波动，串行守卫两轮 900s 等空超时后于窗口期完成全部轮次）；会话中 owner 并行提交 d8f2254e/87c06016（berry-ui 面——kernel/ 与全部路径依赖 crate 零改动，基线仍对现役 kernel 面有效；后续重建仅 ident 串随之变，同长度数据面零布局效应）。


## 快赢③ DC ZVA 清零臂基线刷新（2026-11-17，prove 五跑 max_dev 0.000%）

背景：快赢③（04 号+01 号审计三快赢之一）——`zero_frame`（`kernel/src/vmo.rs`）4KiB 清零从 `write_bytes`→8B GPR memset 循环（512 迭代/页）改 DC ZVA 专用臂（CTR_EL0.DZ 探测 + DminLine 行步进 + GPR 回退，非 SIMD 不破 mem.rs 合同）——一切 commit/缺页供给/建表路径同沾。

新值归因：vmo_commit1p 1683→396（−76.5%）＝设计内算法增量（64 条 dc zva + 环骨架替代 512 迭代 GPR 环，反汇编核证 zero_frame 内 `mrs CTR_EL0`/`tbz #0x1a`/`dc zva` 环在位）；frame_alloc4k 37→37 逐值不动＝buddy 路径零清零面佐证；frame_alloc64k 82→37＝4-2/4-3 在册布局骰轨道对 {37,82} 回摆（zero_frame 代码量变化重掷镜像布局）；handle_evt 274→288（+14）＝tick 相位族（S-2a 同族：boot 指令流变短 → 测量窗内 tick 边沿落位重掷；handle_evt 查表面不进 zero_frame，非代码因果）；vmo_rw4k 6948→6945（−3）＝同相位族微量；cntvct_pair 0→0 观测锚不动。

烘焙与验证：prove 五跑 verdict=exact max_dev=0.000%（值 {0,37,37,288,396,6945}；BENCHWAKE 五跑逐字节同 n=551 max=86009764；回执 icount-prove-20261117-115759-37998）→ ICOUNT_TOL=0.0 维持；`--trace-dual --update` 双份一次刷新（on=off 六项 delta 全 +0——结构性零足迹保住；回执 icount-20261117-115919-48375 / icount-off-20261117-120033-48375 / trace-dual-20261117-120033-48375）；复验单跑 L2 对拍过（ICOUNT_TOL=0.0 六项全在界内，回执 icount-20261117-120204-49314）。ktest 全绿随 boot 结构性背书（FRAME/FP/VMO/HEAP/ASPACE ok 在席，FP canary 直验新清零路径 SIMD-free）。首跑对拍（旧基线四项出界红）回执 icount-20261117-115045-6019。

## [2026-11-18 icount 基线刀] 六旧项不动 + pick_scan/switch_pair 两项入册（08→125 归因 + 覆写保全修 + 帽校准）

- 04 号审计「基线缺口与建议」两项落地：pick_scan（bench 前置窗合成环 20 节点×1024 次 pick_next_filtered）=125、switch_pair（调度器纪元 block/wake 对×128）=301——worktree 换基树（icount-baseline@1e7872196）五跑 prove verdict=exact 逐值同。
- **108→125 归因**：换基前主树（我方五刀 sched 形）测 108，换基后（origin W3-Q93 探峰出选双圈合一形）125——两线同名刀的扫描形态指令差，确定性通道如实记两值；旧六项跨换基逐位同（0/37/37/288/396/6945——pick 路径外零漂移）。
- **trace-dual 覆写保全修（审查轮实抓）**：--trace-dual --update 曾以 sbench=False 臂剥掉 on 家族基线的 switch_pair 行（L2 永不设防）——落盘改「实测 ∪ 现册门控行」并立法定形序「--icount --update 终形」。
- **BOOT_TIMEOUT_ICOUNT 120→1800 帽校准**：静场实测 43s/118.6s/136.7s（三回执在档，骑旧帽线随机截杀）+ 争用态单靴可至 10 分钟级（诊断靴在案）——执法语义不变（真挂死仍帽内 loud），体量与争用态校准。
- B-0 实时通道同批收 pick_scan=39（switch_pair 门控缺席不进 B-0 面）。

## [2026-11-18 深 park 醒源刀] 布局骰四项 + pick_scan 内联位移重烘

- 醒源刀（nearest_dl_throttle_wake+T5）改镜像布局：buddy 族两项换轨（frame_alloc4k 37→127 / frame_alloc64k 37→233）+vmo_commit1p 396→485（内含 4k 分配近邻继承：+89 vs alloc4k +90 差 1——同族非严格 1:1，S-4 4-3 同族判据）；pick_scan 125→131（+4.8%=共享代码内联位移，pick 路径本刀零改动——对照项 switch_pair 301 逐位不动佐证切换舞蹈无染）；cntvct_pair/handle_evt/vmo_rw4k 不动。五跑 prove verdict=exact（回执 icount-prove-20261118-082131）。

## [2026-11-18 深 park 醒源刀·风暴修轮] 布局骰二项再滚

- 审查轮修（两形分离+T5 补强+兜底臂）再改镜像：alloc4k 127→412 / vmo_commit1p 485→771（+359 vs alloc4k +285——近邻继承族再滚，非 1:1）；alloc64k/pick_scan/switch_pair/handle_evt/vmo_rw4k 逐位不动（上轮值稳）。五跑 prove verdict=exact（回执 icount-prove-20261118-091942）。buddy 轨道对族第三次换轨（S-4 4-2/4-3 同族惯例）。

## 十项基线自立 + G1.5 配对悬账收口（2026-11-18，prove 五跑 max_dev 0.000%）

背景：bench.rs 十项在册而基线文件仅六项（pick_scan/switch_pair/send_recv_pair/svc_roundtrip 四项缺席，门禁对缺席只提示不红）——QUEUE 尾「svc_roundtrip icount 基线 --update 自立」梯度件。首跑即炸出 **base 债实锤**：-icount boot 尾 KERNEL PANIC `user.rs:1892 G1.5 遥测环未跟上（total=990 < switches_tt=1249）`——根因 = 04 号 sbench 纪元刀自记的「配对悬账」（bench.rs run_sched_epoch 头注明文）：SWITCHES_TT 在钩子缺席期也计，而 icount 通道恒递 sbench 键 → 纪元窗 ≈259 切换计数照涨环零写 → BENCHWAKE 结构性缺席、基线无法自立。修复（本件携带）：SWITCHES_TT 计数移入 SWITCH_HOOK 在位分支（分母与环写同生同灭，配平律在所有断言窗恒成立）。

新值归因：frame_alloc4k 37→82 / frame_alloc64k 37→82 = 在册布局骰轨道族（{37,82} 对，11-15 notes 先例——zero_frame 后代码量变化重掷镜像布局）；vmo_commit1p 396→441（+11.4%）= 11-17 基线笔后树前进的指令流漂移（berry error/frame 六刀等 boot 面变化）；handle_evt 288 / vmo_rw4k 6945 / cntvct_pair 0 观测锚不动；四新项首录：pick_scan=125 / send_recv_pair=1168 / svc_roundtrip=50 / switch_pair=304（修复后 310→304 = SWITCHES_TT 移分支的小指令流差）。

烘焙与验证：prove 五跑 verdict=exact max_dev=0.000%（十项全 0 漂移；BENCHWAKE 五跑逐字节同 n=694 max=89131406；回执 icount-prove-20261118-200021-37298）→ ICOUNT_TOL=0.0 维持；`--icount --update` 立基十项；复验单跑 L2 对拍过（回执 icount-20261118-200705 系列）。boot→done 57.2s/43.2s wall（修复前 1800s 帽超时）。

## [2026-11-19 三刀重落地] switch_pair 真指令增量 +5 与 buddy 族滚回旧轨

- 机会三/四/五①②重落地（K3→K4→K5 一批）：**switch_pair 301→306（+5）=K3 分槽的真实指令增量**（cpu_id 读+槽变址入 note_switch/bump_switches 计数路径——确定性通道首次量化单刀真成本，勘察预告兑现）；buddy 族三项滚回旧轨（alloc4k 412→37 / alloc64k 233→37 / vmo_commit1p 771→396——布局骰第四掷恰回 v1.14 前值，轨道对族往返形态）；pick_scan 131 逐位稳。五跑 prove verdict=exact（回执 icount-prove-20261119-032315）。

## [2026-11-19 merge-icount 合并树] 十项终档

- origin/wip/sched-1.1@1008dcdc7 × icount-baseline 合并树终档：我方八项逐位保持（pick_scan 131/switch_pair 306——合并指令流对测径零污染的实证）；origin 侧 W3-Q98/Q102 两新项入册（send_recv_pair 1168/svc_roundtrip 50）；alloc64k 37→127（布局骰第五掷，孤项）。五跑 prove verdict=exact（回执 icount-prove-20261119-070911）。

## [2026-11-19 W3-Q108 dispatch 尾合并闩] svc_roundtrip/send_recv_pair 双降——双闩单读的真实指令削减

- Q108（SmpCpu 尾插 exit_latch AtomicU8 + exit_checks() 单点分诊）替换 dispatch 尾两连调（sched_exit_check/roll_quiesce_check 各自 cpu_id() MRS tpidr 追链 + 各读一格全局闩数组）：**svc_roundtrip 50→46（-8）= 每号 syscall 省 1 次 MRS tpidr + 1 次闩数组 Acquire 读**（单核常态分诊读全零直返——置位展开零发生）；send_recv_pair 1168→1160（-8）= 往返内多号 svc 同因累积（send+recv 各省同量）。其余八项逐位不动（pick_scan/switch_pair 佐证调度器面零外溢）。五跑 prove verdict=exact max_dev=0.000%（回执 icount-prove-20261119-115305）→ ICOUNT_TOL=0.0 维持；`--icount --update` 两行立法。
