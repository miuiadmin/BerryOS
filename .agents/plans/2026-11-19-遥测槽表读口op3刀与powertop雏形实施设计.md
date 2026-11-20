# 遥测槽表读口 op3 刀与 powertop 雏形实施设计（0x84 op=3 WakeSrc 槽表纯镜像导出＋WsrcHdrV1/WakeSrcRecV1 telface 单源＋berryctl powertop 子命令雏形）

Type: plan
Status: draft
Updated: 2026-11-19
落库家：.agents/plans/2026-11-19-遥测槽表读口op3刀与powertop雏形实施设计.md（主线评审后翻 active 交实施轮；锚点 2026-11-19 晚逐条实证于 origin/wip/sched-1.1＝origin/main＝50871f37a——本地检出严重落后禁作锚，一律 git show origin/wip/sched-1.1:&lt;path&gt; 取 blob，实施轮 rebase 重钉后复核）

## 一句话说明

owner 令（2026-11-19「彻底实施」续）兑现 op=2 谱挂账 1（plans/2026-11-19-遥测环读口op2刀实施设计.md:102「WakeSrcRec 槽表导出——op=3 起按落地序；wake_src_snapshot 纯镜像形先例已备迁移路径；trace/powertop 消费刀按需取号」）——遥测 v3 第三件：0x84 op 域 add-only 扩 op=3 全槽表纯镜像导出（40B 轻头 WsrcHdrV1{version|hdr_bytes|slots|rec_size|cntfrq|now_vct|wake_acct_n}＋128×64B 定长平数据镜像 WakeSrcRecV1——行号即源值零 src 域，十字段偏移与 kernel 原子壳 cross-const assert 钉死，满载 8232B 字面冻结；len 门照 op=2 裁决 4 判例＝冻结下限 40 无上帽＋整行粒度截尾＋返回值自证；稠密活跃槽形否决——谓词是策略不入内核、first_ts≠MAX 陷阱漏 irq-only 槽恰伤「谁在挡住深睡」观测面、stage 最坏定容不省 .bss）＋装配序双不变式（WAKE_ACCT_N 先读 Acquire→逐槽逐字段 Relaxed 读→now_vct 行读毕后取→头最后落笔：Σ行wakes≥acct_n 与 now_vct≥任一行时戳同时成立——写序「槽先 N 末位 Release」的读者侧对偶＋CNTVCT 单调律）＋一致性契约＝逐字段独立原子读非一致快照诚实文档化（telemetry.rs:552-553 原文先例扩全表——单调族无回退无撕裂、非单调族可旧可新不撕、跨字段错位可能；稳定窗/一致窗/generation 维持挂账不引入）＋8232B static stage KL 单写者新立单发整段（RING_SNAP_STAGE 最新判例直移）＋trace_off 表照出无空头形（on_wake 槽写在早退前——计数器族不随关）＋ktest＝满表 pristine 钉板＋保留段不可达槽 0x40/0x41 播种字节级对拍（boot DAIF 屏蔽＋钩子未注册双证静止窗；种子不触 8 可达槽对噪声结构性免疫）＋ktest_wake_acct_set/reset 两助手（WAKE_ACCT_N 私有）＋既有 op=3→-95 负例滚 op=4＋用毕即清保 G1.5；berry-sys 第四件薄包装＋berryctl powertop 子命令雏形（早分支直调 power/trace 先例、零 gift、不进 smoke boot 序列零标记面；「谁在挡住深睡」v1 判据＝频度〔区间 Δwakes/Δt——头 now_vct 差自证区间锚〕×浪费率〔no_waiter/irq_count 到达锚，真数面仅 UART_RX/POWER_KEY 两槽；TIMER 类走环侧 TIMEOUT 代理不复制〕×源排行〔Δwakes 降序并列 src 升序〕＋安静时长〔now_vct−last_ts〕＋ack 延迟单点代理〔last_ts−last_ack_ts 诚实降档〕；差分＝berry_thread_sleep 真睡两帧逐槽饱和差分——测量自身 TIMER+1 如实注记；--json day-one 第二件 mode 条件域两层、raw 单调计数＋因子出口、率/排名/合成指数归人读形〔公约 5〕）；文档批同批叠清 op=2 四笔未落库欠账（内核唤醒记账 §9 收口改写〔origin 实证仍旧文〕＋op2 笔记勘误尾注、shell设计 v1.2 滚名、可观测性 1.0.1→1.1.0、ABI 文档 §8.4 三原型含 rings 欠账）＋abi desc 三档滚四档＋gen_abi 五面再产 --check；NR 零新号、op=0/1/2 语义零漂移（match 只加臂、唯 K3 负例号滚例外）、G1.5/user.rs/wsrcface/sched 零改、标记七面零滚新、git 脏件禁碰、沿革实施轮禁碰（名义五百九十七临笔 census）；「人无我有」梯队 trace/powertop 双件齐装收口（原文锚 shell设计 v1.0:74＋完善刀挂账 14 双锚引据）。

## 背景

（git 现脏 11 M＋未跟踪件（bootloader/slot、GUI 监督链 gui-inputd/gui-wmd/svcmgr、bench/icount 车道、disk-qw1.img、7 个 target 目录）与本刀域 porcelain 逐项核对零交集全禁碰；origin tip 带在途红：VMO 记账断言红＝hermes PR#532 lane 债——e53806254..50871f37a 全 26 commit 零修红、vmo.rs 390 行零 contig 面，树证据判未拆，P0 前置裁基线〔待裁项 1〕）

1. 挂账与令：op2 谱挂账 1 原文（:102）＋挂账 10 与待裁 5（:111/:154——per-CPU 选择器 regs[4..7] 滤波需求出现再议）；「powertop（谁在挡住深睡）」原文锚＝shell设计 v1.0 §3 :74（commit 260b63aa5 树实证）——现行 standard v1.1 §3（:70-78）三梯队表无 powertop 字面，引据须钉 v1.0:74＋完善刀挂账 14（plans/2026-11-17-命令行终端与shell完善刀实施设计.md:88）双锚，禁单引「shell设计 §3」；owner 令明文 op=3 槽表导出（导出形裁决授权）＋berryctl powertop 雏形（v1 代理判据＋差分模式取舍）＋「人无我有」梯队双件齐装收口。
2. 槽表本体（kernel/src/telemetry.rs:429-500）：WAKE_SRC_SLOTS＝berry_wsrcface::SLOTS re-export（128＝值域上界 0x7F+1 值直索引 table[src]，钳位 slot_index 单源 wsrcface:121-123，kernel 五消费点同引）；WakeSrcRec repr(C) 64B（:445-471 const assert==64）字段 wakes@0/irq_count@8/no_waiter@16/first_ts@24（哨兵 u64::MAX fetch_min）/last_ts@32（fetch_max）/last_ack_ts@40/last_woken_id@48（u32）/flags@52（fetch_or）七原子＋ts_domain@54/_pad@55 普通字节（v1 恒 0 冻结非原子面——如实注记）；WAKE_SRC_TABLE pub static（:492，inline const 初始化，.bss 8KB boot 后永不增长）；WAKE_ACCT_N 私有 static（:495）＋wake_acct_n() 公读（:498）＋wake_acct_conserved 64 次双读稳定窗（:506-518——N 先读后 Σ，写序槽先 N 末位 Release，瞬态只保证 Σ≥N）；wake_src_snapshot 单槽逐字段 Relaxed 读（:569-583）＋WakeSrcSnapshot 十字段平量（:554-566）＋诚实注原文（:552-553「各字段独立原子读，非一致快照——计数族只增不减，撕裂只在读窗内自差，断言面不承重」）＝本刀一致性契约在册原文先例。
3. 写点可达集恰 8 槽（0x01-0x07＋0x7F）：on_wake 六真源 0x01-0x06（NONE/UNKNOWN/POWER_KEY 写点 panic 在前）＋WAKE_ACCT_N 递增皆在 trace_off 早退之前（:364-381「计数器族先例——观测计数≠环记录面」）；到达点 TIMER（main.rs post-EOIR 派发戳）/UART_RX（pre-EOI rx_notify＋n==0 臂 no_waiter 良性竞态）/POWER_KEY（结构性恒无主）；no_waiter 真实写点仅 UART_RX/POWER_KEY 两源；wake_src_merge_hal（:608-627）G1.5 boot 尾单点并 SGI_RESCHED/SGI_TLB/UNKNOWN 的 irq_count（不触 first_ts——三槽 irq-only 哨兵恒 MAX）；预留段 0x08-0x7E 与 0x00 结构性不可达＝120 槽恒 pristine；KERNEL_BOOT.wakes 经验恒 0（spawn 不经 note_wake）→ 典型非零槽 7-8；活动槽谓词先例＝wakes>0||irq>0||no_waiter>0（user.rs:2030）；环白名单 6 真源语义与导出视角 8 槽口径差须分清（混用会把 POWER_KEY/UNKNOWN 槽误当可裁剪）。
4. ktest 静止窗双证（树实证 main.rs）：ktest::run（:1007）先于 berry_sched::init（:1128）与 telemetry::init（:1140）——on_wake 钩子未注册、wakes 族全槽恒 0；:1127 注「IRQ 此刻仍屏蔽（boot DAIF）」——到达族写点结构性不可达；merge_hal 在 G1.5 尾（后于 ktest）⇒ ktest 窗内表结构性近 pristine——满表钉板确定性根基（防御纵深：可达槽 irq 不断言绝对值，不押纯度前提）。
5. 派发与 stage 先例（kernel/src/syscall.rs:488-688，树实证）：match 首键 op、len 门各自在臂内（:605）；op=0 16KiB 帽专属＋逐环分段拷诚实部分读；op=1 冻结下限 40＋栈上 40B 装配单发（定长小件栈先例）；op=2 冻结下限 32 无上帽＋RING_SNAP_STAGE 7296B KL 单写者新立不共用（命名诚实）＋单遍装配头最后落笔＋单发整段（无半写面）＋返回 cursor 自证；域外 -ENOTSUP 臂注「后续波次自 op=3 起按落地序取号」（:686，本刀滚 op=4）。
6. telface 与消费链（树实证）：op 常量单源（TELEMETRY_OP_KLOG/MEMSTAT/RINGS :178/:184/:223）；RingsHdrV1 32B 头样式模板（version/hdr_bytes 尾追锚＋cntfrq 换算唯一源）＋RingSegV1 rec_size 自述判例＋LE 编解码纯函数对（:356-379）＋host 钉板 34 测（shape_pinned/trio 互异/round-trip）；WakesrcRow（:133）已立「Wsrc」缩写与字段序律先例；TELEMETRY_RINGS_MAX=7296 字面冻结（:349）；berry-sys 三档薄包装＋re-export（:526-591）＋berry_thread_sleep（:486——deadline 绝对 MONOTONIC ns 真睡包装现成）；berryctl power 早分支（:324）＋trace 早分支（:329-337）＋trace_cmd（:105）＋--json day-one 首件 schema（:170-244）＋wsrc= 改名避「ROOTFS ok src=」子串判例（:299-301）＋零 #[test]（门＝justfile build-user musl 链＋smoke bc_selftest 仅 ver/ps/mem——trace 不进 boot 序列零标记面，powertop 同构）。
7. ktest 面现状：op=2 臂 K1-K4 全族在册（:6319-6650 静默盖——种子正例/因果差分/负例/用毕即清）；op=3→-95 域外负例（:6632-6637）翻真必滚下一值 op=4；WAKE_SRC_TABLE pub static 字段全 pub＝槽播种可直原子 store 零新口，唯 WAKE_ACCT_N 私有需助手；G1.5 WAKESRC 断言面（user.rs:1995-2034）与 WSRC-K（ktest.rs:7366-7378）均经内核读口——user.rs/wsrcface 零改硬约束沿用（种子污染由用毕即清封堵）。
8. powertop 判据底座：TCG 实测 PSCI SYSTEM_SUSPEND NOT_SUPPORTED（psface 降级/ktest:6336/仅 susp=1 走 HVC）＋无 PMU/无 C-state 面——v1「深睡」语义锚＝idle_loop 尽空深 park masked-wfi 长驻留（sched/src/sched.rs:3526、main.rs:6333-6384）；「谁在挡住深睡」＝谁的唤醒频度打断空闲驻留——Linux powertop wakeups/s 判据同构（PM-03 research:40「powertop 十几年只优化一个指标 wakeups——每次唤醒强制 CPU 退出深 idle」）；差分时钟锚缺失（槽表只有 first/last 单点戳）由头 now_vct 解——安静源不虚长区间、无宿主-客机钟混源；TIMER 类浪费 v1 无槽级面，环侧 WakeRec.reason==TIMEOUT 占比代理归 trace 面不复制（受 64 条环帽如实注记）。
9. 文档债勘定（origin 树实证）：①docs/内核唤醒记账设计-v1.0.md §9（:166-182）仍「（冻结，不实现）」Handle 形旧文（magic|version|cntfrq|generation|slept_ns|suspend_count 九字段头＋烘焙一致像）——op2 实施笔记（notes/implemented/2026-11-19-遥测环读口op2刀.md:5/:24）声称已改写而树无此改（e53806254 触面 12 件不含该 doc），本刀同批真落＋笔记勘误尾注；②docs/可观测性设计-v1.2.md 仍 Version 1.0.1（§5 MINOR 明文授权「0x84 新增操作码＋--json 导出字段」）；③docs/系统调用ABI设计-v1.0.md §8.4 零 berry_telemetry_* 原型（头块仍 1.0｜2026-11-06）；④docs/shell设计仍 v1.1 未滚 v1.2——四笔 op2 D2-D5 未落，本刀 desc 再滚即三代失同步，同批叠清（待裁项 2）。
10. 沿革与并行：docs/阶段沿革.md 尾节五百九十三（:7785）；lane 已占 594（hermes/w3）/595-596（hermes/w2）——本刀名义五百九十七临笔 census 复核撞号顺延；op=2/memstat-free/BSHRC/预算授片四刀沿革节仍欠（主线统补窗另裁）；2026-11-19 活跃 origin lane 对本刀域文件零碰撞（碰撞 lane 全陈线）。

## 契约

### 全局契约（六域共守）

G1 op=0/1/2 语义零漂移：match 只加臂（TELEMETRY_OP_WSRC => 插 RINGS 臂后），klog/memstat/rings 三臂原码零字符改；KLOG/RING 两 stage 与既有装配契约不动；既有 test_telemetry_open 断言零改自执法——唯 K3 的 op=3 域外负例滚 op=4（add-only 自执法迁移；op=99 与 op=0/1/2 族负例零改）；trace/dmesg/free e2e 运行时面零扰。
G2 NR 零新号＋ABI 单源律：0x84 复用、TELEMETRY_OPEN_NR 私有字面量维持、nr 计数面零动；abi.toml 仅 0x84 desc 滚（三档在册→四档在册＋op=3 句）＋abi.syz 同句锁步；改后 python3 tools/gen_abi.py 再产五面（kernel/src/abi_gen.rs、user/berry-sys/src/abi_gen.rs、ABI 文档 §4 生成块、tools/abi.syz、arch/aarch64/src/wake_gen.rs——第五件预期零 diff：本刀零触 wsrcface 值域与 [[wakesrc]] 生成器源，非零 diff 即停刀归因）＋--check 逐字节门；禁手改产物；先存红（VMO＝PR#532 lane 债）不连坐不代拆。
G3 七面零滚新＋现行消费面零改：markers.sh/smoke.sh/smoke-smp.sh/smoke-image.sh/bcts 冻结件/bcts-run.sh/gui8-smoke.py 全零触；kernel/src/user.rs 零 diff（G1.5 WAKESRC 断言面/wake_src_snapshot 消费零改——种子污染由 K4 用毕即清封堵）；sched/ 零触；wsrcface/ 零改（三律纯面与 8 host 测零牵连、值常量不迁）；ktest 新臂静默盖（零新输出行保 boot 逐行零漂移，ktest done 裸相第 13 位不动）；berryctl powertop 不进 smoke boot 序列（零标记面）、bc_selftest 三子命令面（ver/ps/mem）零滚；新固定文案（usage 行、powertop 行前缀与列名）与 91 件标记两两互非子串——实施毕 canon 机查记档（wsrc= token 族复用仍必全量机查；特别规避「src=」「ok 」子串面）。
G4 脏件禁碰与并行纪律：git 脏 11 M＋未跟踪件全禁碰；实施轮文件域互斥、禁 git 提交（主线统一显式路径暂存，禁 add -A）；构建/测试隔离 CARGO_TARGET_DIR（~/.cargo-target-<名> 惯例）；每域开工 git status 复核＋rebase 重钉 origin tip；docs/阶段沿革.md 实施轮全周期禁碰（主线 HEAD 重同步后追加，取号临笔 census）；结构性零 bench 通道。
G5 基线前置硬门：VMO 记账断言在途红（树证据未拆）——P0 三选一（催 lane 清偿/绿基独立验证法〔预算授片 9fe7f3889 判例〕/主线明裁改基）后动手；本刀 ktest 臂零 VMO 相位（种子是槽表非 VMO）不直撞红面，smoke 全链按基线路线跑（待裁项 1）。
G6 op=3 一致性语义诚实文档化（本刀核心承诺）：「逐槽逐字段独立原子读、非一致快照」全文进 abi.toml desc＋可观测性 §3 注＋berry-sys doc 三处——单调族（wakes/irq_count/no_waiter fetch_add、first_ts fetch_min、last_ts fetch_max、flags fetch_or、merge_hal 单调 store）中途读恒得某个真实持过的值无回退无撕裂；非单调族（last_woken_id/last_ack_ts 定宽 store）可旧可新不撕；跨字段错位可能（wakes 新于 last_ts 等）；装配序双不变式＝N 先读（Acquire）后扫槽使 Σ行wakes≥acct_n 恒立（瞬态语义照 wake_acct_conserved 原文，一次广播可醒任意多任务不做固定差值帽）＋行读毕后取 now_vct 使头锚≥任一行内时戳（CNTVCT 单调律——消费方区间分母可安全夹上界）；first_ts==u64::MAX 哨兵＝未触；差分只许用单调族（区间差精确自愈、不丢不双计）；戳字段单点语义（跨字段戳差是可旧可新单点代理非分布）；全记录一致窗不承诺（挂账维持）；trace_off 表照出（计数器族不随关）；SGI×2/UNKNOWN 的 irq_count＝G1.5 boot 尾单点并账快照非连续并账——差分对该三槽恒 0 的 v1 已知边界如实注记（连续并账挂 v2）；禁默示「全表烘焙一致像」。

### 契约 F：telface 形状与常量单源域（telface/src/lib.rs）

F1 pub const TELEMETRY_OP_WSRC: u64 = 3＋演进注（后续波次自 op=4 起按落地序取号不预留）；telemetry_op_trio_distinct 扩 quad（{0,1,2,3} 两两互异＋==3 钉值）。
F2 行形状：pub struct WakeSrcRecV1 repr(C) 64B 平数据镜像——wakes:u64@0/irq_count:u64@8/no_waiter:u64@16/first_ts:u64@24/last_ts:u64@32/last_ack_ts:u64@40/last_woken_id:u32@48/flags:u16@52/ts_domain:u8@54/_pad:[u8;5]@55——const assert size_of==64＋align_of==8；不含 src 域（行号即源值——值直索引 0..=0x7F 单源 wsrcface，头 slots/rec_size 自述几何）；doc 注：内核原子壳 WakeSrcRec（telemetry.rs:445-471）同偏移镜像（kernel 侧 cross-const assert 执法）、first_ts 哨兵语义随形、ts_domain/_pad 保留冻结域（v1 恒 0 普通字节非原子——如实标注）、add-only 扩路注（条目形变走新 op 值纪律）。
F3 头形状：pub const WSRC_SNAP_VER: u32 = 1；pub struct WsrcHdrV1 repr(C) 40B——version:u32@0/hdr_bytes:u32@4（=40 头尾追扩形锚）/slots:u32@8（=128 消费方自适应行数）/rec_size:u32@12（=64 条目步进自述——跨版本走读锚）/cntfrq:u64@16（vct→ns 换算唯一源——rings 头同款）/now_vct:u64@24（快照时刻 CNTVCT——差分区间时钟锚＋安静时长分母）/wake_acct_n:u64@32（独立总账——守恒自验右侧，瞬态 Σ≥N 方向随注）＋const assert==40；砍 magic 注（memstat/rings 无 magic 双判例）。
F4 几何与容量常量：WSRC_TABLE_V1_SLOTS=128/WSRC_TABLE_V1_REC=64（kernel cross-const assert 同值钉死——槽域双改同批执法）＋TELEMETRY_WSRC_MAX=8232 字面冻结（40+128×64，不随 size_of 动——TELEMETRY_RINGS_MAX 判例）。
F5 LE 编解码纯函数对：wsrc_hdr_write(&WsrcHdrV1,&mut [u8;40])/wsrc_hdr_read(&[u8;40])＋wsrc_row_write(&WakeSrcRecV1,&mut [u8;64])/wsrc_row_read(&[u8;64])（垫区写零、读方垫区不验——host 可测 round-trip，rings_hdr_write/read 判例）。
F6 host 钉板测试族（既有 34 测邻位追加）：wsrc_row_shape_pinned（size 64＋align 8＋十字段偏移字面）；wsrc_hdr_shape_pinned（40B＋七字段偏移）；编解码 round-trip×2；哨兵保真测（未触行 round-trip 后 first_ts==u64::MAX）；TELEMETRY_WSRC_MAX 算术恒等（40+slots×64 对拍）；rec_size 域==size_of::<WakeSrcRecV1> 恒等；quad 互异（F1）。

### 契约 M：内核遥测装配域（kernel/src/telemetry.rs）

M1 cross-const assert：offset_of!(WakeSrcRec,&lt;f&gt;)==offset_of!(berry_telface::WakeSrcRecV1,&lt;f&gt;) 十字段逐偏移＋size_of==64＋WAKE_SRC_SLOTS==WSRC_TABLE_V1_SLOTS——插字段/错序即编译红（op=2 M2 判例同构）。
M2 全表导出装配：pub(crate) fn wsrc_table_snap_into(stage:&mut[u8], limit:usize)->usize——装配序①N＝WAKE_ACCT_N.load(Acquire) 最先（wake_acct_conserved 读序同款——瞬态恒 Σ≥N）；②逐槽 0..128 字段独立 Relaxed 读（wake_src_snapshot :569-583 同款原子→平值转换）→wsrc_row_write LE 落 stage[40+src*64..]（行数 rows=min(128,(limit−40)/64)——整行粒度，半行结构性写不出）；③now_vct＝cntvct() 行读毕后取（头锚≥任一行时戳）；④头最后装配落笔 offset 0（version/hdr_bytes/slots/rec_size/cntfrq=timer::cntfrq()/now_vct/wake_acct_n=N——op=2 头最后落笔判例）；⑤返回 40+rows×64；不并账不重试不设窗（读口零写——merge_hal 不进装配）；trace_off 无特判表照出。
M3 ktest 助手两件（pub(crate)，doc 注「仅 ktest 静止窗合法；生产路径禁调」）：ktest_wake_acct_set(n:u64)（WAKE_ACCT_N.store——ktest 自算 Σ 播种对拍）＋ktest_reset_wsrc()（128 槽逐字段恢复 WakeSrcRec::new() 初值＋N 归 0）；槽播种零新口（WAKE_SRC_TABLE pub static 字段全 pub 直原子 store——与 op=2 环需开种子口的差异：表已 pub、唯 N 私有）。
M4 模块头注 :13-18 滚：op 域四档在役（op=3 WakeSrc 槽表纯镜像——本文件 wsrc_table_snap_into）、后续波次自 op=4 起。
M5 既有面零改：on_wake/note_irq_arrival/note_irq_no_waiter/wake_src_merge_hal/wake_src_snapshot/WakeSrcSnapshot/wake_acct_* 全零触；WakeSrcRec 原子壳与 WAKE_SRC_TABLE 定义零改（本刀只加读口）。

### 契约 S：syscall 派发域（kernel/src/syscall.rs）

S1 op=3 臂（插 TELEMETRY_OP_RINGS 臂后）：use 增引 TELEMETRY_OP_WSRC；const WSRC_SNAP_HDR_V1: usize = 40（字面冻结下限——MEMSTAT_REC_V1/RINGS_SNAP_HDR_V1 同款注族）；len<40 → -EINVAL；无上帽（变长填充返回值自证——16KiB 帽 op=0 专属不移植）；len=40 可单读头探几何（op=2 探针先例同精神）。
S2 stage 与拷出：static mut WSRC_SNAP_STAGE: [u8; berry_telface::TELEMETRY_WSRC_MAX] 新立（单写者＝KL 注——sys_telemetry_open 恒 TABLE 类 KL 全局互斥跨核串行＋ktest EL1 同经 dispatch 取锁；RING_SNAP_STAGE 裁决 6 判例：新立不共用、命名诚实）；n＝wsrc_table_snap_into(stage, len)（槽对齐截尾在装配内）→copy_to_user(el1, ptr, &stage[..n]) 单发整段（无半写面；-EFAULT 透传）→返回 n（≥40 头自证）。
S3 注释滚：模块头注（四档在役）、域外臂注「后续波次自 op=3 起」→「自 op=4 起」、op=3 臂注含 G6 一致性契约全文（分族语义＋Σ≥N 方向＋now_vct≥行时戳＋哨兵＋截尾＋trace_off 表照出＋并账点边界）；既有 op=0/1/2 臂逐字节保形。

### 契约 K：ktest 臂（kernel/src/ktest.rs test_telemetry_open 尾部扩展，:6652 闭括前）

K1 满表 pristine 钉板（静默盖零新输出行；static mut WSNAP: [u8;8232]）：svc4(0x84,3,WSNAP,8232) 返恰 8232；头钉板（version==1/hdr_bytes==40/slots==128/rec_size==64/cntfrq>0 且==EL1 直调 timer::cntfrq() 双源/wake_acct_n==EL1 直调 telemetry::wake_acct_n() 双源且==0/now_vct 单调 sane）；128 行结构性钉板——120 不可达槽（0x00、0x08-0x7E）全字段断言 pristine 谱（wakes/irq/no_waiter==0、first_ts==u64::MAX、last/ack==0、id==0、flags==0、ts_domain==0、垫 0——写点全枚举执法的运行期版）＋全 128 槽 wakes==0（钩子未注册确定性面）＋Σ行wakes==acct_n==0（守恒镜像）；注：TIMER/UART/PWRKEY 三可达槽 irq_count 不断言绝对值（噪声免疫层——boot DAIF 屏蔽 :1127 使到达点结构性不可达为据，但断言面不押该前提）。
K2 播种对拍＋len 因果：直原子 store 播两保留段不可达槽——0x40 全谱非零（wakes=5/irq=9/nw=3/first=1000/last=2000/ack=1900/id=0xDEADBEEF/flags=分类位）＋0x41 irq-only 形（irq=4、first_ts 不动==u64::MAX——irq-only 哨兵语义钉板）＋ktest_wake_acct_set(5)；满取→两行 LE 解码逐字段恒等＋126 行仍 pristine＋头 wake_acct_n==5＋Σ==5（守恒镜像）＋now_vct>2000（真 CNTVCT 对种子值结构性优势）；重播 0x40 wakes fetch_add(5)＋N set(10)→再满取 row wakes==10、其余行逐字节恒等（单调差分面钉板）；len 阶梯：len=0/8/39→-22、len=40→返 40（单读头探几何）、len=104→返 104 且恰 row[0] 全字段落位、len=40+64×5→返 360 且为满载结果前缀逐字节恒等、len=8231→返 8168（(8231−40)/64=127 整行截尾钉板）。
K3 负例滚面：既有 op=3→-95 断言（:6632-6637）滚为 op=4→-95（域外负例迁下一值——add-only 自执法；op=99 与 op=0/1/2 族负例零改零漂移）。
K4 用毕即清：ktest_reset_wsrc()→再满取返 8232 且 128 行全 pristine 谱＋头 wake_acct_n==0＋telemetry::wake_acct_conserved() 真（Σ==N==0 复原）——封堵种子对 G1.5 双形态断言面（user.rs:1995-2034）与 WSRC-K（ktest.rs:7366-7378）的污染；静默盖延续（fail=panic 停机零输出行）。

### 契约 W：berry-sys 薄包装域（user/berry-sys/src/lib.rs）

W1 pub use berry_telface::{WakeSrcRecV1, WsrcHdrV1, TELEMETRY_OP_WSRC, TELEMETRY_WSRC_MAX, wsrc_hdr_read, wsrc_row_read}＋区注滚（四档在役、后续自 op=4）；Cargo.toml 零改（berry-telface path 依赖已在）。
W2 #[no_mangle] pub extern "C" fn berry_telemetry_wsrc_snapshot(buf: *mut u8, len: usize) -> Status——sys(TELEMETRY_OPEN_NR, TELEMETRY_OP_WSRC, buf, len)（klog/rings 裸缓冲透传同形第四件）；doc 注：返回实写字节数（≥40 头自证）或负 errno 透传；len<40→-EINVAL 内核执法；G6 一致性契约全文＋条目步进按头 rec_size 自述（消费方跨版本走读锚）。

### 契约 U：berryctl powertop 子命令雏形（user/bins/berryctl/src/main.rs）

U1 早分支（trace 先例位 :329-337 邻接、psproto match 前——直调 berry_sys 非 RPC 零 gift 依赖）：if sub=="powertop" { powertop_cmd(&args[2..]); exit(0); }；args 扫描 --json 与 -d &lt;ms&gt;（u64 毫秒、帽 30000ms——RPC_CAP_NS 30s 判例；缺值/非数 stderr＋exit 1）；无 -d＝单快照累计形；usage 行滚 ver|ps|mem|power|trace|powertop。
U2 快照取数：vec![0u8;8232]＋berry_telemetry_wsrc_snapshot；st<0/n<40 → stderr 一行＋exit 1（trace 先例同形）；wsrc_hdr_read 头解析＋128 行按头 rec_size 步进 wsrc_row_read 解码（telface 单源——条目偏移手解不在本面）；活动槽过滤＝wakes>0||irq>0||no_waiter>0（user.rs:2030 计数族谓词判例——first_ts≠MAX 禁用：漏 merge_hal 三槽与 PWRKEY 恰是要观测的无主到达面）。
U3 人读形（默认单快照）：首行「powertop acct=N cntfrq=N active=M/128 now=NNNms」（now＝now_vct×1000/cntfrq ms；不出现「src=」「ok 」子串面——canon 机查门）；活动槽行按 wakes 降序、并列 irq 降序、再并列 src 升序（确定性——src_top 并列取首见判例同精神），行形复用 wsrc= token 族（canon 已过前缀）：「wsrc=0x02(TIMER) w=5 i=7 nw=0 first=-|NNNms last=NNNms ack=NNNms idle=NNNms」——src→短名＝本文件本地 8 槽表（KBOOT/TIMER/UART/RESCHED/TLB/IPC/PWRKEY/UNK）＋hex 兜底（名字映射归消费方公约）；浪费面 nw 列直出＋仅 irq>0 槽行内括注 waste=nw/irq%（v1 真数仅 UART_RX 竞态＋POWER_KEY 结构两槽——TIMER 类浪费走环侧 TIMEOUT 代理归 trace 面注记一行）；idle＝now_vct−last_ts（仅 last_ts>0 出）；ack＝last_ts−last_ack_ts 单点代理（仅 last_ack_ts>0∧last_ts≥last_ack_ts 出，跨字段错位诚实降档「-」）；first_ts==u64::MAX→「-」。
U4 差分形（-d &lt;ms&gt;）：snap1→berry_thread_sleep(now()+ms×1e6) 真睡（berry-sys :486 现成包装——测量任务真睡让核 park、窗内醒源直接归因＝powertop 本义；禁 clock 忙等——自抬 TIMER 到达计数污染被测面）→snap2→逐槽饱和差分（单调族 Δ 恒非负）；区间＝h2.now_vct−h1.now_vct（头时钟锚自证——命令行标称值仅作睡眠 pacing 不作分母）；人读行同列形出 dw=/dirq=/dnw=＋区间率 wake/s（cntfrq 换算，消费方派生）；差分行集＝两帧活动槽并集；测量自身贡献如实注记（本任务超时醒＝TIMER wake+1 诚实可解释）。
U5 --json day-one 第二件：纯净 stdout 单对象、诊断 stderr（shell设计 §3 :78 铁律）、schema add-only；顶层公共因子平铺＋mode 条件域两层——{version:1, mode:"cum"|"diff", cntfrq, slots:128, rec_size:64；cum 形带 now_vct/wake_acct_n/sources:[{src,wakes,irq_count,no_waiter,first_ts,last_ts,last_ack_ts,last_woken_id,flags}]；diff 形带 t0:{now_vct,wake_acct_n}/t1:{同}/interval_vct/sources:[{src,dwakes,dirq_count,dno_waiter,last_ts}]}——活动槽 src 升序、src 出数值不出名；字段单义：cum 行恒 raw 累计、diff 行恒 Δ（mode 条件域——op=2 族内条件域按 family 可缺席判例同构）；raw 单调计数＋cntfrq＋now_vct 锚出口，率/比/排行/合成指数不进 JSON（公约 5 判例——防测量窗口语义双义进 ABI）；first_ts 哨兵 u64::MAX 原样导出（schema 注明）；ts_domain 不进（v1 冻结 0——add-only 后补通道在）；实施毕 canon 机查记档。
U6 测试惯例：零 #[test]（berryctl 全仓惯例——构建即门）；门＝justfile build-user musl 链构建绿＋手跑双形存证（e2e 观测归主线窗口——表导出无 seqlock 重试面，与 op=2 环降级风险不同构更低，如实记档）；不进 smoke boot 序列零标记面。

### 契约 D：文档与 ABI 批（终值定形后；主线职责；四笔 op=2 欠账叠清范围按待裁项 2 裁）

D1 tools/abi.toml 0x84 desc 滚：「三档在册」→「四档在册」＋追加 op=3 句（WsrcHdrV1 40B 七域＋128×64B 纯镜像值直索引＋逐字段独立原子读非一致快照＋单调/非单调分族＋Σ≥N 方向＋now_vct≥行时戳＋哨兵＋len≥40 冻结下限无上帽＋整行截尾返回值自证＋trace_off 表照出＋SGI×2/UNKNOWN 并账点边界＋域外 -ENOTSUP 后续自 op=4＋berryctl powertop 消费）＋abi.syz 同句锁步→gen_abi 五面再产＋--check（G2 规则；wake_gen.rs 预期零 diff 复核记档）。
D2 docs/系统调用ABI设计-v1.0.md：§4 0x84 行随生成滚；§8.4 补三原型（berry_telemetry_memstat_snapshot＋berry_telemetry_rings_snapshot〔op=2 D2 欠账〕＋berry_telemetry_wsrc_snapshot 新）＋WsrcHdrV1/WakeSrcRecV1 字段定义（形状链指可观测性——一事实一归属）；版本行按实态滚修订（现记 1.0｜2026-11-06）。
D3 docs/可观测性设计-v1.2.md：Version 1.0.1→1.1.0（§5 MINOR 明文授权「0x84 新增操作码＋--json 导出字段」——修订行分记 op=2 欠补与 op=3 两笔）＋Updated；§3 telemetry 通道句滚 op 域四档＋powertop 消费注＋非一致快照口径。
D4 docs/内核唤醒记账设计-v1.0.md §9 收口改写真落（op=2 D4 欠账——origin 实证 :166-182 仍「（冻结，不实现）」Handle 形旧文，op2 笔记 :5/:24 声称已归档与树不符）：标题翻「（已收口——2026-11-19 环读口刀起分档落地）」＋原案保留历史引述块（永不删除）＋收口裁决段（零 handle (op,buf,len) 延续形已由 op=0/1/2 连续落地＋op=3 兑现 WakeSrcRec 面＝40B 头＋64B 行平镜像非九字段重头＋generation/烘焙像维持挂账）；版本 1.0→1.1＋修订行＋Updated（文件名不滚）；§9 以外章节零改；.agents/notes/implemented/2026-11-19-遥测环读口op2刀.md Related 行加勘误尾注（超前标注本刀补落——加注非改写）。
D5 docs/shell设计-v1.2.md 滚名 v1.2（op=2 D5 欠账同批）：梯队二 berryctl 行记 trace＋powertop 子命令、梯队三演进注 powertop 雏形已落 berryctl（引据 v1.0:74 原文锚入修订行——现行 v1.1 §3 无 powertop 字面禁单引）；文件滚名＋全仓入链 rg 复核改指＋check-docs 执法。
D6 .agents 批：op2 谱 :102 挂账 1 行尾兑现尾注＋:111 挂账 10 维持注；本谱主线评审后落库翻 active；新笔记 .agents/notes/implemented/2026-11-19-遥测槽表读口op3刀与powertop雏形.md（导出形裁决论据＋装配序双不变式论证＋稀疏度/写点可达集执法账＋ktest 静止窗双证与播种槽论证＋powertop 判据 v1 定谳＋文档欠账清偿账分刀记欠）；node .agents/tools/check-docs.mjs 绿；docs/阶段沿革.md 主线 HEAD 重同步后追加（取号临笔 census——名义五百九十七、lane 已占 594-596、四刀欠节清点记档）。

## 步骤

0. [ ] P0 前置（主线职责）：重钉 origin tip＋域文件 blob 对照；本刀域 porcelain 复核；VMO 红基三选一路线裁决（待裁项 1）；gen_abi --check 定基线记档；锚点复核表（派发臂 :605/槽表布局 :445-500/静止窗时序 :1007&lt;:1128&lt;:1140＋DAIF :1127/berryctl 早分支 :324/:329/四文档欠账实态——漂则按新基改谱再动手）。
1. [ ] D1 内核＋telface 域（dependsOn 0）：契约 F/M/S/K 四文件；cargo test -p berry-telface 绿（隔离 CARGO_TARGET_DIR）＋kernel 构建绿（cross-const assert 编译期执法）＋裸相 QEMU boot ktest done 存证（K1-K4 静默盖）。
2. [ ] D2 消费端域（dependsOn 1 的 telface 落地即可启、可与 D1 尾段并行）：契约 W/U 两文件；justfile build-user musl 链绿＋canon 机查记档。
3. [ ] D3 文档/ABI 批（主线职责，dependsOn 1,2 终值；叠清范围按待裁项 2 裁）：契约 D1-D6；gen_abi 五面再产＋--check＋check-docs 绿。
4. [ ] D4 主线统一验证提交（dependsOn 全部）：分层验证链全绿；git diff --stat 自证 user.rs/wsrcface/sched 零 diff/七面零滚新/脏件零碰；e2e 手跑 powertop 双形＋差分存证；沿革实时取号（census 撞号顺延）；显式路径提交。

## 挂账移交（本刀显式不做）

1. 连续 HAL 并账（SGI×2/UNKNOWN 运行期 re-merge 进表——差分恒 0 边界的 v2 解；现役 G1.5 boot 尾单点并账语义零漂移）。
2. 槽级 TIMER 浪费判据字段与 post-EOI ack 精化（last_ack_ts v2 同款挂账模式——add-only 尾追或新 op；v1 用环侧 WakeRec.reason==TIMEOUT 占比代理归 trace 面）。
3. 全记录一致窗/generation/烘焙一致像（op2 挂账 2/8 维持——随 ProcStatRec/CapInfoRec 统一裁决）。
4. per-CPU 选择器（regs[4..7] cpu 过滤——槽表全局无 per-CPU 维度，维持 op2 挂账 10 滤波需求出现再议）。
5. idle 驻留窗（op=2 idle_next 派生）进 powertop 面板与 op=2 交叉面板（64 条环帽下区间归因不可靠——随 e2e 观测账另议）。
6. powertop 阈值定谱与「挡深睡」判定线＋循环刷新模式（依赖 op2 e2e 降级观测挂账先闭——v1 只出排行与 raw；循环刷新归 btop/traced 领土）。
7. bsh 内建/btop/traced/PMU 面消费件与 CLI ok n 滚动（cli-suite 与完善刀挂账 14 归属——本刀 berryctl powertop 雏形即梯队第二块）；AI 策略服务本体（berry-sys 面已备）。
8. [[telemetry_op]] 机器可读 op 表与 gen_abi 第六产物（free 挂账 5 维持）。
9. 稠密活跃槽导出形与 ~56B 打包条目新 ABI 域（落选封存——8KB 拷出在真实消费节奏下成成本时 add-only 新 op 复活）。
10. SLEPT 域/ts_domain≠0 双钟域（挂起刀；本刀 ts_domain 照镜像出恒 0）＋名字哈希 id→name 反查表内核导出（FNV-1a 消费方可自建）。

## 裁决记录

1. 导出形＝纯镜像全表＋40B 自述头（否决稠密活跃槽形、裸无头镜像、48B 头）：四论据——①挂账 1 原文明文「8KB 纯镜像」且 wake_src_snapshot（:569-583）逐槽纯镜像先例在册，定长值直索引使 ABI 面最简（行号即源值、消费方零解包零 count 依赖、字节前缀性免费获得——小 len 结果恒为满载前缀）；②稠密省的只是典型返回字节（~0.6KB vs 8232B），stage 须按 128 槽全活最坏定容不省 .bss——诊断节奏秒级手跑下 8232B 拷出非成本（op=0 16KiB 单发在役先例）；③稠密把活动槽谓词烧进 ABI——first_ts≠MAX 陷阱漏 irq-only 槽（merge_hal 三槽＋POWER_KEY 的 first_ts 恒哨兵）恰是「谁在挡住深睡」要观测的无主到达面（user.rs:2030 计数族谓词判例归消费方，镜像导出＋消费端过滤两全）；④~56B 打包窄行另立 ABI 域不值（同偏移 64B 平镜保 cross-const 钉几何与 ktest 字节钉板）。
2. 头形＝40B 七域 version|hdr_bytes|slots|rec_size|cntfrq|now_vct|wake_acct_n（三稿分歧终裁）：export 稿 48B 全 u64 否决——slots/rec_size 用 u32 尽够（128/64 远离上限）省 8B 无语义损失；judge 稿 40B 缺 rec_size 否决——条目步进自述是 add-only 跨版本走读锚（op=2 RingSegV1 rec_size 判例），砍它断自适应消费方；终形取 risk 稿布局（u32×4 前紧凑＋u64×3 尾对齐，40B 恰 8 对齐）。now_vct 立字段（差分区间时钟锚——无它则区间绑「最后一笔事件」〔安静源虚长〕或宿主钟〔异源〕）；wake_acct_n 立字段（守恒自验右侧进导出面）；cntfrq 立字段（vct→ns 换算唯一源——rings 头判例）；砍 magic（memstat/rings 双判例）；ts_domain 不入头（v1 恒 0——镜像行字节照拷即尽）。
3. 拷出机械＝8232B static stage KL 单写者＋单发整段（否决 risk 稿逐槽栈装 129 次 copy_out）：RING_SNAP_STAGE 是最新在册判例（单发整段无半写面——半写面即用户缓冲出现不完整行＋无头的中间态）；.bss 8232B 与既有 stage 同域同量级（RING 7296B/KLOG 16KiB），对照 WAKE_SRC_TABLE 本身 8KB；129 次用户页检查在诊断频度下非成本但引入 128 个中途 -EFAULT 部分读分支——面更多非更少；memstat 40B 栈先例不适用（彼是单件 40B，本刀满载 8232B 超栈舒适区）。
4. 装配序双不变式（三稿各执一序的合流）：N 先读（Acquire）→逐槽行读→now_vct 后取→头最后落笔——同时成立 Σ行wakes≥acct_n（export/judge 方向不变式：写序槽先 N 末位 Release 的读者侧对偶，照 wake_acct_conserved 读序）与 now_vct≥任一行时戳（risk 时钟锚序：CNTVCT 单调律——消费方区间分母安全夹上界）；两头锚都进 desc/berry-sys doc 文档化。
5. 一致性契约＝逐槽逐字段诚实非一致（否决稳定窗复用与一致窗新机器）：telemetry.rs:552-553 原文先例延伸全表——单调族中途读恒真值无回退、非单调族定宽不撕、跨字段错位可能；wake_acct_conserved 式稳定窗只冻结 wake 侧写者——irq 侧（last_ack_ts/irq_count/no_waiter）窗内可前进，伪一致像比诚实非一致更危险；差分模式靠单调族自愈（相邻区间自差归位不丢不双计）；全记录一致窗维持 op2 挂账 2/8 与裁决 1（砍 generation）不重开；表无 seqlock 无重试面——op=2 最大风险（TCG 降级）在本刀结构性不存在，风险定级低于 op=2。
6. 形状单源＝telface 新名 WakeSrcRecV1/WsrcHdrV1＋kernel 原子壳留壳（否决同名迁移 re-export）：kernel WakeSrcRec 是 AtomicU64 壳（表本体不可替换为平数据——wsrcface:33-36「布局留 kernel 壳」明文），SwitchRec 同名迁移判例对原子壳结构性不可行；行名取 WakeSrcRecV1（镜像 kernel 同名血统、cross-const assert 对位直观——否决 WsrcRowV1：与既有 WakesrcRow 一字之差易混）；头名取 WsrcHdrV1（照 RingsHdrV1 缩写纪律——Wsrc 缩写在 WakesrcRow/WSRC_F_* 双先例）；WAKE_SRC_* 值常量不迁（HAL ABI 面留 wake_gen——wsrcface 值域参数化纪律）；wsrcface 零触。
7. ktest＝满表 pristine 钉板＋保留段不可达槽播种（三稿播种槽分歧终裁）：静止窗双证在树（ktest::run main.rs:1007 先于 sched :1128/telemetry :1140 钩子未注册＋:1127「IRQ 此刻仍屏蔽（boot DAIF）」到达点结构性不可达）——但断言面不押纯度前提：①120 结构性不可达槽全字段 pristine 钉板（export——写点全枚举的运行期执法版，对任何前提鲁棒）；②全 128 槽 wakes==0＋Σ==N==0（钩子未注册确定性面）；③TIMER/UART/PWRKEY 可达槽的 irq_count 不断言绝对值（防御纵深）；④播种槽取保留段 0x40/0x41（risk——任何运行期路径结构性不可达，对静止窗纯度与 IRQ 噪声双免疫；否决 export 的 0x01/0x04/0x7F 与 judge 的 0x02/0x03/0x7F——均触可达 8 槽集，播种正确性依赖静止窗纯度主张）；⑤0x41 播 irq-only 形钉 first_ts 哨兵语义（judge 的 UNKNOWN irq-only 灵感移植到不可达槽——语义钉板不依赖真写点）；⑥ktest_wake_acct_set 使 Σ==N 守恒镜像可断言（judge——WAKE_ACCT_N 私有实证 :495，slot 直 store 不需要助手）；⑦K4 清零＋conserved 复核封堵 G1.5/WSRC-K 污染。
8. 差分睡眠原语＝berry_thread_sleep（否决 event wait_one 帽睡）：berry-sys :486 现成真睡包装（deadline 绝对 MONOTONIC ns）——与 event 形同为真睡、同有测量任务超时醒 TIMER+1 自污染面，event 形多造一件事件对象与 SIGNAL 语义零收益；「clock 忙等」无人提案、禁令保留（自抬 TIMER 到达计数污染被测面）；差分区间＝两帧头 now_vct 差自证（标称值仅 pacing）；测量自身贡献人读行如实注记。
9. powertop v1 判据＝频度×浪费率×源排行（三方收敛、judge 论据全文采纳）：「深睡」v1 语义＝idle masked-wfi 深 park 长驻留的打断（TCG 无 PSCI suspend/无 C-state/无 PMU——非 Linux 同名面）；频度主判据＝区间 Δwakes/Δt（PM-03 :40 wakeups 单指标论；单快照降级 avg=wakes/((now_vct−first_ts)/cntfrq) 自首触平均——first_ts 哨兵入 ABI 使派生零歧义）；浪费率＝no_waiter/irq_count 到达锚 [0,1]（分母裁 irq_count 非 wakes——wakes 是广播乘数〔一次广播醒任意多任务、Σ≥N 判例〕nw/wakes 无界不可比；POWER_KEY wakes==0∧nw>0 自然 100%＝结构性纯浪费正确语义；真数面 v1 仅 UART_RX 竞态＋POWER_KEY 结构两槽——TIMER 类走环侧 TIMEOUT 占比代理归 trace 面不复制）；排行＝Δwakes 降序（单发 wakes 降序）并列 src 升序；安静时长与 ack 延迟为展示辅助（单点代理诚实降档）；--json 只出 raw＋因子（字段单义——率/排名/合成指数归人读形/消费方，公约 5 判例；mode 条件域保 cum/diff 两形字段不换义）。
10. len 门＝冻结下限 40 无上帽＋整行截尾（三方一致，op=2 裁决 4 判例直移）：len=40 单读头探几何；余量整 64B 分、不足一行即止、返回值 40+行数×64 自证；否决 len==精确 8232 门（free 裁决 8 否决判例）与 16KiB 帽移植（op=0 专属语义）；尾追演化路＝头尾追（hdr_bytes 锚）＋条目步进按 rec_size 自述＋条目形变跨大改走新 op 值纪律。
11. 文档批＝op=2 四笔欠账同批叠清（默认；owner 翻案通道待裁项 2）：四件同为 0x84/berryctl 登记面，本刀 desc 再滚即三代失同步（38bbb7017 两代失同步判例）——§9 收口改写真落（op2 笔记声称与 origin 树实态不符已实证 :166-182 仍旧文——不落则两代笔记连谎且后续刀按 Handle 旧文设计＝MAJOR 返工）＋op2 笔记勘误尾注（永不删除纪律——加注非改写）＋可观测性 1.0.1→1.1.0（树实证 Version: 1.0.1——judge 稿 1.1.0→1.2.0 前提错否决）＋ABI §8.4 三原型（含 rings 欠账）＋shell设计 v1.2 滚名（powertop 引据钉 v1.0:74 原文＋完善刀挂账 14 双锚——现行 v1.1 §3 无 powertop 字面禁单引，docs 审查按陈旧引据打回）。
12. NR 零新号＋op=0/1/2 逐字节保形＋G1.5/user.rs/wsrcface/sched 零改＋七面零滚新（G1-G3 全文）：match 只插臂；既有断言唯一改动＝K3 域外负例 op=3→op=4 迁址（add-only 自执法判例在册）；种子污染由 K4 用毕即清封堵；powertop 不进 boot 序列使标记面结构性为零；wsrc= token 族复用（canon 已过）仍全量机查。
13. trace_off 表照出零特判（与 op=2 环空头形对照文档化）：on_wake 槽写与 N 递增都在 trace_off 早退之前（:364-381「计数器族先例——观测计数≠环记录面」）＋G1.5/merge_hal 无条件——表导出双 trace 态皆真数据、无空头形、零特判；两语义不同源，desc 分别注明禁混称。

## 风险

最大单点＝ktest 满表 pristine 钉板假设破（未枚举的 boot 早期表写点）：护栏＝断言红即停刀归因（写点全枚举走查＋boot DAIF 屏蔽注 :1127 为据）；降级谱＝「120 不可达槽 pristine＋可达 8 槽仅 wakes==0 结构断言、irq 不断言」——已内建于 K1。
装配序漂移破坏 Σ≥N 或 now_vct≥行时戳（未来改装配代码）：护栏＝装配序注释执法＋K2 守恒镜像钉板＋ktest now_vct>种子 last_ts 断言；头锚语义进 desc 三处。
powertop 差分测量自身污染被测面（超时醒 TIMER+1）：护栏＝人读行如实注记＋差分行以 Δ 呈现使贡献可解释＋-d 帽 30000ms。
canon 新固定文案撞 91 标记子串（usage 滚动＋powertop 行前缀/列名）：护栏＝实施毕 canon 机查记档＋行首 wsrc=/hex 短名规避「src=」「ok 」子串面（wsrc= 判例）。
--json schema 第二件锚定（add-only 首扩）：护栏＝字段保守最小（raw＋因子＋mode 条件域）＋派生量禁入；翻案通道待裁项。
形状漂移（kernel 壳改字段不抄 telface）：cross-const assert 逐偏移编译期执法＋telface shape_pinned＋ktest 字节级三层。
K3 漏滚（op=3 翻真返数据不再 -95）与 K4 漏清（污染 G1.5/WSRC-K）：两断言列入验收单；红则停刀归因禁带红推进。
gen_abi 再产 wake_gen.rs 非 zero-diff：本刀零触 wsrcface 值域与 [[wakesrc]] 生成器源——diff 即归因（lane 撞域或基线漂），停刀复核。
VMO 红基归因混淆（PR#532 lane 债）：P0 裁基线后动手；本刀 ktest 臂零 VMO 相位不直撞；smoke 全链按基线路线跑。
文档批触面广（shell设计滚名入链/§9 改写）：全仓入链 rg 复核＋check-docs 绿门＋待裁项 2 翻案通道。
沿革撞号（lane 已占 594-596＋四刀欠节）：主线临笔 census 顺延（流程项）。
并行 lane 活跃：提交轮前重钉 tip＋域文件 blob 对照。
体积账：telface 约增 1KiB（两形状＋常量＋编解码＋host 测试）、kernel telemetry 约增 0.8KiB＋syscall 约增 0.4KiB（op=3 臂＋注）＋stage 8232B .bss（RING_SNAP_STAGE 7296B 同域同量级、KLOG 16KiB 之下）、ktest 约增 3KiB 源、berry-sys 约增 0.3KiB、berryctl 约增 150-180 行——零镜像/manifest 改动。

## 砍单汇总（killList）

落选封存（复苏需翻本刀谱）：稠密活跃槽导出形（头+count+活跃条目变长——裁决 1）；~56B 打包条目新 ABI 域（裁决 1）；裸 8KB 无头镜像（裁决 2）；48B 全 u64 头与无 rec_size 头（裁决 2）；magic 头域（裁决 2）；len==精确 8232 门与 16KiB 上帽移植（裁决 10）；稳定窗复用/conserved 旗导出/generation 头域/全记录一致窗（裁决 5——挂账 3 维持）；读口内 HAL 二次并账（裁决 13——G1.5 语义零漂移，连续并账挂 v2）；同名 WakeSrcRec 迁 telface＋kernel re-export（裁决 6——原子壳结构性不可行）；WAKE_SRC_* 值常量迁 telface/wsrcface 与 wake_gen.rs 任何改动（裁决 6/G2）；ktest 可达槽播种（export/judge 槽选——裁决 7 否决）；event wait_one 差分睡眠原语（裁决 8——berry_thread_sleep 定谳）；clock 忙等差分（裁决 8 禁令）；槽级 TIMER 浪费字段与 post-EOI ack 精化（挂账 2——v2 add-only）；per-CPU 选择器 regs[4..7]（挂账 4——表全局）；idle 驻留窗面板与 op=2 交叉面板（挂账 5）；浪费率 no_waiter/wakes 分母形（裁决 9）；ack 延迟分布面复制（裁决 9——单点代理，分布归 trace 三分位）；powertop 循环刷新/阈值/色彩面板（挂账 6——btop/traced 领土＋e2e 先闭）；内核侧测量窗/率/排行/浪费率合成语义（裁决 9——公约 5）；--json 派生率/合成指数与 ts_domain 入 JSON（裁决 9——字段单义/add-only 后补通道在）；ts_domain 入头（裁决 2）；bsh 内建/btop/traced/PMU 件与 CLI ok n 滚动（挂账 7——cli-suite 归属）；powertop 进 smoke boot 序列/bc_selftest（G3——零标记面）；[[telemetry_op]] 机器表与 gen_abi 第六产物（挂账 8）；SLEPT/双钟域（挂账 10）；id→name 反查内核导出（挂账 10）；AI 策略服务本体（挂账 7）。
硬边界（零改执法）：op=0/1/2 三臂任何字符改动与 KLOG/RING stage 契约改动（G1——唯 K3 负例号滚例外）；kernel/src/user.rs、sched/、wsrcface/ 任何改动（G3——G1.5/WSRC-K 现行消费面零改，种子污染由 K4 封堵）；标记七面任何滚新含注释/boot 行漂移/new canon 面（G3）；NR 表新号与 gen_abi 产物手改不经再产（G2）；git 脏件 11 M＋未跟踪车道任何触碰与 bench 通道（G4）；实施轮 git 提交与沿革落笔（G4——主线统一显式路径＋census 取号）；内核唤醒记账 §9 以外章节改动与全文档重写（D4——只加收口注＋版本滚＋勘误尾注）；旧基（rebase 重钉 tip 前）直接动手（G4）。

## 待裁项（owner）

1. 基线路线：origin tip VMO 记账断言在途红（PR#532 lane 债）——催 lane 清偿 vs 绿基独立验证法（9fe7f3889 判例）vs 主线明裁改基；P0 后置门。
2. op=2 四笔文档欠账是否随本刀同批叠清（本稿默认同批——四件同为 0x84/berryctl 登记面，本刀 desc 再滚即三代失同步；翻案改记欠则 D3 拆批、归因更净——含 shell设计 v1.2 滚名与 §9 收口改写的落库归属）。
3. 沿革节：四刀欠节（op2/memstat-free/BSHRC/预算授片）与本刀节的主线统补窗口、节序与取号（名义五百九十七——尾 593、lane 已占 594-596，临笔 census 撞号顺延）。
4. powertop e2e 观测窗归属：双形手跑存证与 op2 挂账 e2e 窗（TCG 降级谱观测）合并还是分窗；-d 默认区间值与 30000ms 帽终值随窗定谱；差分是否后续进 smoke 常驻观测（入列即动标记面须另刀）。