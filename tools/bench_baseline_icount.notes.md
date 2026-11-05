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


