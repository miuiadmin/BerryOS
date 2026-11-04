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

