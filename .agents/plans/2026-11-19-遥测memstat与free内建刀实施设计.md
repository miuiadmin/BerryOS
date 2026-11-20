# 遥测 memstat op 与 free 内建刀实施设计（0x84 op=1＋MemstatRec＋bsh free 第 24 内建）

Type: plan
Status: draft
Updated: 2026-11-19
落库家：.agents/plans/2026-11-19-遥测memstat与free内建刀实施设计.md（主线评审后翻 active 交实施轮）

## 一句话说明

owner 令（2026-11-19「彻底实施」续）兑现完善刀挂账 #3 遥测 v3 首件——内核 0x84 telemetry_open op 域 add-only 扩 op=1 memstat 定长快照（MemstatRec 定谳五字段 5×u64=40B：total/reserved/alloc/free/committed——free 读点派生使 total==alloc+free+reserved 按构造闭合，reserved/committed 两聚合水位补账），形状与 op 常量单源落 berry-telface（kernel 已依赖、face crate 纪律——消内核/用户双写），内核补 free 真源（buddy reserved 对称水位）与 committed 全局聚合原子（vmo.rs 单文件写点，账面口径=ledger Some(pa) 项数），berry-sys 薄包装（结构体出参 berry_clock_read 同层先例），bsh free 第 24 内建（单行紧凑带键值 KiB 无旗标形——uname/uptime/free 三件套 RAM 面板收口）；NR 表零新号（0x84 复用、字面量挂派发维持）、七面标记零滚新（新步⑫吸收进 CLI ok n=12→13 前缀式、内核 ktest 臂静默盖折入 ktest done）、op=0 klog 语义零漂移（match 化保序保码＋既有四负例全保）、数值断言分治（ktest 静止窗强式恒等式＋因果差分、bsh 运行窗弱式域断言——committed 与 alloc 分窗读的结构性反例禁强式）、全恒等式域零精确值断言。

## 背景

（锚点 2026-11-19 现场工作树复核成立——合成轮对三视角稿〔abi/user/risk〕逐锚复验；本刀域文件 porcelain 全空：kernel/src/{syscall,telemetry,mm,ktest,vmo,abi_gen}.rs、mm/src、vmoledger、telface、user/berry-sys、user/bins/bsh、tools/abi.toml；git 现脏 14 M＋未跟踪 8 项全属 icount/bench、berry CLI/模拟器、GUI 三车道在途件——实施禁碰清单，tools/bench.py 在途 8 行尤烈〔本刀结构性零 bench 通道〕）

1. 挂账与令：挂账 #3 原文（.agents/plans/2026-11-17-命令行终端与shell完善刀实施设计.md:77）「free 内建：RAM 真源须内核统计口径先定 ABI（0x84 op 域 stage=v3 现仅 klog 一档，SwitchRec/WakeRec 排队在前）——随遥测 v3 刀（MemstatRec 形状草案随账附送 { total_pages, alloc_pages, free_pages, committed_pages } repr(C)）」；实施笔记硬化面 .agents/notes/implemented/2026-11-17-命令行终端与shell完善刀.md:26；owner 令明文 memstat＝v3 首件——「排队在前」由令重裁为 memstat 单独首发（op 号 add-only 从未在册无重排违约），「形状定谳」即本谱裁权（四域是随账附送草案非钦定）。
2. 0x84 现形（实证 kernel/src/syscall.rs:491-546）：op 裸 u64 取 tf.regs[1]，唯一常量 TELEMETRY_OP_KLOG=0（:495）；判序＝op 判先行（:521-523 单臂 if op!=0 → ERR_NOT_SUPPORTED）后 len 帽（:524-526 len==0 || >16KiB → -EINVAL，klog 专属语义按 op 无关形前置）；KLOG_SNAP_STAGE 单写者=KL（:501-504）；install() 字面量 reg(0x84, ..., SVC_CLASS_TABLE)（:4264-4270）；零 handle 零权利位（:515 sched_util_snapshot 同族口径）。
3. RAM 真源：buddy.allocated AtomicUsize O(1) 水位（mm/src/buddy.rs:84-89——alloc_order 成功 +2^order、free 成功 −2^order、reserve/release 不动 RESERVED 恒不计，#[cfg(test)] 对拍钉板 :441-450）→ FrameAllocator 透传（mm/src/frame.rs:148-158）→ kernel mm::allocated_pages() with_frames 单口（kernel/src/mm.rs:253-257）；total＝构造窗 [RAM_BASE, ram_top) 定容 npages()（mm.rs:145-164，ram_top 三级定源）；reserve_range 逐页标 RESERVED（buddy.rs:279-315）、release_range 返实还页数（:328-344）但调用方只进 MMAP ok segs= pages= 打印（mm.rs:202-219）——保留页无聚合计数，free 无现成单源。
4. committed 现状：vmoledger committed_count()/committed_pages() #[cfg(test)] pub(crate)（vmoledger/src/lib.rs:424-439）；kernel/src/vmo.rs 只持 per-object ledger（:33-35）；写点实证＝mark_committed×2（:202/:214）、mark_decommitted×2（:232/:249）、Drop 三臂（MMIO 零动作 :362-365／contig 整阶归还 :367-372／匿名 drain 逐页 :376-379）、create_contiguous_global 建账臂（:106-139）；new_contiguous 实证 pages=Some(pa)×npages（vmoledger lib.rs:175-205）——账面口径=项数非 2^order 块。
5. 形状冲突与单源件：挂账四域 vs ipc/design.md:91-97 七字段（0x07 INFO_KMEM_STATS(6)：{total, free_buddy, pcp, slab, vmo, discardable, wired}_pages——pcp/slab/discardable/wired 四源零实现，heap.rs 零计数器实证、per-CPU 页缓存不存在）vs cli-suite/design.md:61 六面；可观测性设计-v1.2.md:35 立 MemstatRec 家＋禁重复记账源、§5 :47-48 明文「在 0x84 下新增操作码」=MINOR；另有内核唤醒记账设计-v1.0.md:166-177 §9 冻结未实现的另一套 0x84 签名（kind,options→Handle——属环读口刀面）。telface 单源件实证：telface/src/lib.rs 在册（no_std 纯函数面、含 ringmerge W3-Q81 先例）、kernel/Cargo.toml:33 已依赖 berry-telface、根 Cargo.toml:31 member、user/bins/* 数十件跨 workspace path 依赖（path = "../.."）先例；user/berry-sys/Cargo.toml [dependencies] 现空表——telface 为首件依赖零迁移面。
6. bsh 扩展面（实证）：BUILTINS 23 单源（user/bins/bsh/src/main.rs:3865-3870）；HELP_LINES 三行 192B 帽实测 147/181/96B（:3876-3880）；CLI ok n=12 单源 println（:5839）＋步数注释两处（:396-405/:5425）；④ 探针表十名（:5541）＋help↔BUILTINS 互钉（:5549-5553）；uptime 数值域自验模板（:5490-5503——内核活值禁精确断言先例）；dmesg_output 4KiB 栈缓冲＋诊断码 1（:4652-4667）；markers.sh:35 前缀串 "CLI ok n="＋:54 canon 互非子串纪律＋:62 RX 族（SIMBAT/BATT/PWRKEY）。
7. ktest 形态（实证 kernel/src/ktest.rs）：test_telemetry_open :6046-6080 静默盖（fail=panic 停机、零输出行保 boot 逐行零漂移）＋四负例（op=99→-95 :6072、len=0→-22 :6074、len=16KiB+1→-22 :6075-6079）；调用位点 :128-130 末位新号纪律、ktest 主链 sched::init 前单核静止窗（差分断言确定性依据）；WSRC-K 布局钉 :6794-6806；WakeRec 定义处 const assert telemetry.rs:298 惯例。
8. 门禁与基线：gen_abi --check 现场红（文档 v1 计数 40≠abi.toml nr_total 41，F0-4 lane 欠账）堵 just build/smoke 入口——cargo 直跑＋隔离 CARGO_TARGET_DIR 绕行判例、客机面被拦挂账不连坐；本地 wip 与 origin/wip 两线互缺（kernel 面 origin 领先三刀含 vdso/BENCH n=10、bsh 面本地领先作业控制 v2 整刀，域内差 24 件；telemetry.rs 两线同 blob 锚稳）；沿革权威尾号五百八十（origin/wip），本刀取五百八十一；40 模块总纲表 §2 新增号表无 0x84 行——已上线在册号不归其辖，零改动。

## 契约

### 全局契约（六域共守）

G1 op=0 klog 零漂移：match 化对 op=0 逐字节保形——现状判序实证 op 先行（:521-523）后 len 帽（:524-526），重构保此序；klog 臂 len 帽原序原码随臂下放；KLOG_SNAP_STAGE 与拼接契约不动；既有 test_telemetry_open 四负例零改自执法；dmesg e2e（rootfs 相实跑）承担运行时验证。
G2 NR 零新号＋ABI 单源律：0x84 字面量挂派发维持；gen_abi 常量层只产 v1/1.1 现状不动（gen_abi.py:218-231）；abi.toml 仅 0x84 desc 滚新（nr_total/nr_ceiling/implemented 计数面零动）；改后 python3 tools/gen_abi.py 再产五面（kernel/src/abi_gen.rs、user/berry-sys/src/abi_gen.rs、ABI 文档 §4 生成块、tools/abi.syz、arch/aarch64/src/wake_gen.rs——后两件应零 diff，diff 须归因）并以 --check 为门；先存红（40≠41，F0-4 lane 欠账）不连坐不代拆——并流后复跑定基线、红面非本刀 desc 则如实挂账；禁手改产物、禁替跑 just abi-gen。
G3 七面零滚新含注释：markers.sh/smoke.sh/smoke-smp.sh/smoke-image.sh/bcts 冻结件/bcts-run.sh/gui8-smoke.py 全零触；新步⑫吸收进 CLI ok n=12→13（步数单源 main.rs:5839＋两处步数注释同批滚——bsh 不在七面）；内核 ktest 臂静默盖折入既有 ktest done；boot 逐行零漂移（fa.reserve/fa.release 返回值与 MMAP 打印零改）；新文案（mem: 行/usage: free/诊断行）与 RX 断言族（SIMBAT/BATT/PWRKEY/hellq/f42t/d127x/i130t//d1）及 91 件标记全表两两互非子串——实施毕 canon 机查记档（markers.sh:54 先例）。
G4 数值断言分治＋恒等式域：一切面禁精确值断言（RAM 随配置/拓扑/负载漂移）。ktest 静止窗（sched::init 前单核、写点全静止）强式：双源对拍＋因果差分（末态==初态全等）＋reserved>0＋committed≤alloc；bsh 运行窗弱式：total>0、alloc/reserved/committed 各自≤total、free==total−alloc−reserved 重演算（struct 内定义式恒真——free 由单 with_frames 一致窗三元组派生）；committed≤alloc 在 bsh 侧禁用（committed 与 alloc 分窗读——反例序列「读 committed 后他窗 decommit+free 再读 allocated」可造假态）；各自≤total 恒安全（total boot 定容常量）。
G5 脏件禁碰与并行纪律：git 脏 14 M＋未跟踪三车道件全禁碰；实施轮文件域互斥、禁 git 提交（主线统一显式路径暂存，禁 add -A）；构建隔离 CARGO_TARGET_DIR（~/.cargo-target-<名> 惯例）；每域开工 git status 复核；docs/阶段沿革.md 实施轮全周期禁碰（主线 HEAD 重同步后追加，取五百八十一，落笔前 git log 复核撞号顺延）；user/Cargo.toml 与根 Cargo.toml 零触碰（telface 已是 member、berry-sys 依赖走自身 Cargo.toml）。
G6 基线前置硬门：两线在本刀两半面互相领先——不并流即动手＝syscall/ktest 对旧 kernel 写、bsh 对 21 内建态写（第 24 号位与 CLI n 步数算错）；主线先并流（真源重建＋增量补丁＋两向审计判例）或明裁改基（步骤 0 前置）。

### 契约 F：形状与常量单源（telface/src/lib.rs）

F1 #[repr(C)] pub struct MemstatRec { pub total_pages: u64, pub reserved_pages: u64, pub alloc_pages: u64, pub free_pages: u64, pub committed_pages: u64 }＋定义处 const _: () = assert!(size_of::<MemstatRec>() == 40)（WakeRec telemetry.rs:298 惯例）；day-one 定型注＋add-only 扩路注（加字段=尾追新版本域或新 op 值，禁改旧结构——对外层四十年合同的 op 域版）；字段全定宽对齐 8、LE 直拷。
F2 op 常量单源：pub const TELEMETRY_OP_KLOG: u64 = 0; pub const TELEMETRY_OP_MEMSTAT: u64 = 1;（演进注：SwitchRec/WakeRec 环读口自 op=2 起按落地序取号，不预留）。
F3 host 钉板测试：size==40＋字段偏移字面钉板（0/8/16/24/32）＋两 op 常量值钉板。

### 契约 M：内核 op 域（kernel/src/syscall.rs＋kernel/src/telemetry.rs）

M1 syscall.rs：本地 TELEMETRY_OP_KLOG（:495）撤、改 use berry_telface 消费；分发自单臂 if 改 match op——臂 0（klog）逐字节保形（G1）、臂 1（memstat）新、_ => ERR_NOT_SUPPORTED；memstat 臂 len 门＝冻结下限常量 MEMSTAT_REC_V1=40（len<40 → -EINVAL；len≥40 实写 40B 返 40——尾追追加律：未来扩形旧调用方 len=40 收 40B v1 前缀、返回值自证所得，跨大改走新 op 值；两臂 len 门各自在臂内，拒序 op 先行现状保持）；快照装配栈上 40B raw＋copy_to_user 单发整段（sched_util_snapshot raw 组装同款，零 static stage）；:493-494 注与模块头注滚（memstat 已落 v3 首件、环读口 op=2 起随后续波次）。
M2 telemetry.rs：pub(crate) fn memstat_snapshot() -> MemstatRec——mm::memstat_frame_counts() 单闭包一致窗取 (total, reserved, allocated)（free=total−reserved−alloc，u64 saturating 防御——结构不变量下不触发）＋vmo::committed_pages()；头注 :13-15 滚（klog 首域与 memstat 第二档在役、环读口留后续波次）。

### 契约 R：mm 记账（mm/src/buddy.rs＋mm/src/frame.rs＋kernel/src/mm.rs）

R1 buddy.rs 加 reserved: AtomicUsize 水位（allocated 同款形态 :84-89 邻位）：reserve_range 每实标一 RESERVED 单元 +1（幂等跳过已占用块零动）、release_range 每实还页 −1（released 计数已在函数内——同步累计）；reserved_pages() O(1) Relaxed；allocated/npages 语义零改；两函数签名与返回值零改（调用方零滚）。
R2 host 对拍钉板：#[cfg(test)] reserved_pages_scan 对拍（扫 order==RESERVED vs 原子——allocated_pages_scan :441-450 同款纪律）＋交叠幂等不重计＋release 归零＋reserve/release 往返守恒。
R3 透传与单口：frame.rs reserved_pages() 透传（:153-158 邻位）；kernel/src/mm.rs pub(crate) fn memstat_frame_counts() -> (usize, usize, usize)（with_frames 单闭包一致窗取 npages/reserved/allocated——:253-257 同族邻位）；fa.reserve/fa.release 返回值与 MMAP 打印零改（G3）。

### 契约 V：vmo committed 聚合（kernel/src/vmo.rs 单文件）

V1 static VMO_COMMITTED_PAGES: AtomicUsize（Relaxed）＋pub(crate) fn committed_pages() -> usize 读口；口径＝PageLedger Some(pa) 项数（MMIO ledger pages 恒空天然不计；contig 按 new_contiguous 的 npages 记账非 2^order 块——实证 vmoledger lib.rs:175-205，阶块尾浪费页留 alloc−committed 差额可见=诚实账）。
V2 写点逐对（实施轮以 rg mark_committed|mark_decommitted|new_contiguous|contig_drop_block|droppable_pages 全量复核记档为准）：commit 逐页 +1（:202）、commit_global 逐页 +1（:214）、decommit 逐页 −1（:232）、decommit_global 逐页 −1（:249）、create_contiguous_global 成功建账臂 +npages（:125；Err 回滚臂零净计——建账失败账面未立）、Drop 匿名 drain 臂逐页 −1（:376-379）、Drop contig 臂 −建账 npages（:367-372）、Drop MMIO 臂零动作（:362-365）。
V3 「禁重复记账源」（可观测性设计:35）论证：PageLedger 仍是 per-page 唯一事实源（资格判定与簿记），聚合原子是其物理执行臂的全局投影——与 buddy.allocated 对位图的关系同构（B-0 先例）；论证全文入实施笔记；drift 护栏＝ktest 因果差分臂（末态==初态）＋committed≤alloc 恒等式静止窗双钉。

### 契约 K：ktest 臂（kernel/src/ktest.rs）

K1 扩进 test_telemetry_open 尾部（:6080 前）零新函数零新调用点（同号同域聚合、调用表零变更）：op=1 正例 svc4(0x84, 1, ptr, 40) 返 40（static mut REC 静态手法同 SNAP）＋解码五域恒等式（total>0、reserved>0〔内核镜像必占保留页——防水位忘接/恒零〕、reserved≤total、alloc≤total、free==total−alloc−reserved、committed≤alloc〔静止窗成立〕）＋双源对拍（total==EL1 直调 frames().npages()、alloc==mm::allocated_pages()——svc 读回 vs 单口两独立路径同源对拍）＋布局钉 size_of::<MemstatRec>()==40（WSRC-K :6794-6806 同款）。
K2 因果差分臂（V2 写点主钉板）：瞬态 VMO 三相（test_sysver 先例现建现验不留柄）——建+commit → decommit → drop，每相后 svc4 快照对拍，末态与初态五域全等（写点净零检测——语义无关强执法，不依赖 eager/demand 语义假定）；中间态 committed 单调方向断言（实施轮按所用 VMO 路径实测语义钉 ±N）；reserved 全程不动断言（VMO 不碰保留面）。
K3 负例：op=1 len=0 → -22、len=8（<40）→ -22；既有负例 op=99→-95 与 op=0 len 边界三断言零改零漂移（:6072-6079）。全臂静默盖（零输出行保 boot 相逐行零漂移）。

### 契约 W：berry-sys 薄包装（user/berry-sys/src/lib.rs＋Cargo.toml）

W1 Cargo.toml [dependencies] 加 berry-telface = { path = "../../telface" }（首件依赖；user/bins/* 数十件跨 workspace path 先例）；lib.rs pub use berry_telface::{MemstatRec, TELEMETRY_OP_MEMSTAT}＋本地 TELEMETRY_OP_KLOG 常量撤改消费（TELEMETRY_OPEN_NR 私有字面量 :548 保持——v3 常量层落地后换名注在册）。
W2 #[no_mangle] pub extern "C" fn berry_telemetry_memstat_snapshot(out: *mut MemstatRec) -> Status（svc 0x84 op=1，内部 len=size_of；berry_clock_read 结构体出参同层先例；返回 40 或负 errno 透传）；:549-552 注释滚。

### 契约 B：bsh free 第 24 内建（user/bins/bsh/src/main.rs 单文件）

B1 BUILTINS 23→24（kill 后加 "free" :3865-3870＋表头注滚）；HELP_LINES 第三行尾加「| free（RAM 面板）」（:3879——现约 96B＋约 22B≈118B≤192B 帽，实施毕字节数复核记档）；Tab/type/help 互钉自动随表（RealSrc::commands 直读 BUILTINS）。
B2 render_cmd 双臂（uname/uptime 同形）：cmd == "free" → free_output(out)；strip_prefix("free ") → 「usage: free」拒 2（纯查询无参语义——旗标族 additive 留门归演进面）；free_output＝dmesg_output 同位（:4658 邻）——栈 MemstatRec 调 W2，r<0 打诊断行「bsh: free: telemetry memstat snapshot failed (r)」码 1（三拒面透传 dmesg 同形），Ok 返单行数据「mem: total=<n>K reserved=<n>K alloc=<n>K free=<n>K committed=<n>K」＋换行尾（值=pages×4 KiB 换算——4KiB 页整除精确、K 后缀自述单位、POSIX 默认单位习惯；字段序与 struct 同序；数据面走 render_cmd 返回值——free > f 天然重定向，诊断打 out=stderr 语义）。
B3 cli ④ 探针表 :5541 十名加 "free"（进表——free 数值域可确定性断言，强于 dmesg 字节流先例）；步⑫（CLI ok n=12→13 :5839＋两处步数注释 :396-405/:5425 同批滚）：行形断言（mem: 前缀＋单行＋换行尾）＋五 token 键序齐（键=值K 形）＋值域 ASCII 数字非空＋恒等式域（total>0、alloc/reserved/committed≤total、free==total−alloc−reserved KiB 域整数重演算——内核派生式契约钉板，派生式变即红；committed≤alloc 禁用〔G4 分窗反例〕）＋「free x」带参拒 2（uptime :5490-5503 模板直改，纯 render 直调零外部 bin、静默不打印数据面）。

### 契约 D：ABI 与文档批（终值定形后）

D1 tools/abi.toml 0x84 desc 滚：op=0 句零改＋op=1 句追加（MemstatRec 40B 五域、len≥40 冻结下限与尾追追加律、恒等式口径、域外 -ENOTSUP、SwitchRec/WakeRec 环读口随后续波次 op=2 起）→ gen_abi 五面再产＋--check（G2 规则）。
D2 docs/系统调用ABI设计-v1.0.md：§4 0x84 行（:107）随生成滚；§8.4 :469 邻位手写补 berry_status_t berry_telemetry_memstat_snapshot(berry_memstat_t *out);（0x84 op=1）＋berry_memstat_t 五字段定义（形状相对链可观测性设计——一事实一归属）；Version 1.0.2→1.0.3＋修订行＋Updated 2026-11-19（op 域枚举扩展=§8.2 七枚举 1.0.2 PATCH 同判例——文件名 v1.0 不滚）。
D3 docs/可观测性设计-v1.2.md：§3 :35 MemstatRec 具名处落五域定谳＋0x84 op=1 在册＋两水位口径（reserved=buddy 对称水位、committed=vmo 聚合原子——聚合账 vs 明细账镜像论证）＋非一致快照取舍（字段各自原子、alloc/reserved/total 单锁窗一致、committed 独立窗、组合强式仅静止窗）；Version 1.0.1→1.1.0（§5 MINOR 明文授权）＋修订行＋Updated。
D4 docs/shell设计-v1.2.md：Version 1.1.0→1.2.0（§4 MINOR 新增内建）连文件名滚 v1.1→v1.2＋全仓入链改指（rg 全仓复核——两稿 13/25 处计数分歧以实施轮 rg 为准；jobs 刀连名滚判例 14bf83943）＋§2.3 名单 23→24＋系统与网络行 free 注记（bsh 内建先行·0x84 op=1 单源；外部 free [-h|--json] 仍演进同源）。
D5 .agents 批：挂账 #3 兑现注记（plans/2026-11-17-命令行终端与shell完善刀实施设计.md:77 行尾——#1/#2 同款尾注法；note :26 历史记录不改）；.agents/plans/ipc/design.md:91-97 INFO_KMEM_STATS(6) 行补单源指向注（形状以 0x84 op=1 落地版 MemstatRec 为准、七字段前瞻形不另立、届时作投影消费）；新笔记 .agents/notes/implemented/2026-11-19-遥测memstat与free内建刀.md（裁决与砍单全文＋恒等式账＋两水位论证＋验证账）＋node .agents/tools/check-docs.mjs 绿＋本刀谱主线落库后步骤勾账；docs/内核唤醒记账设计-v1.0.md §9 零触碰（裁决 13）；docs/阶段沿革.md 主线 HEAD 重同步后追加（五百八十一，git log 复核撞号顺延）。

## 步骤

0. [ ] P0 基线并流（主线会话职责，非实施轮）：并流或明裁改基＋gen_abi --check 定基线＋域文件干净复核。
1. [ ] D1 mm 记账域（dependsOn 0；与 2/3 并行）：R1-R3；cargo test -p berry-mm 绿（隔离 CARGO_TARGET_DIR）。
2. [ ] D2 vmo committed 域（dependsOn 0；与 1/3 并行）：V1-V3；开工 rg 写点全量复核记档；kernel musl 构建绿。
3. [ ] D3 telface 形状单源域（dependsOn 0；与 1/2 并行）：F1-F3；cargo test -p berry-telface 绿。
4. [ ] D4 内核 op 域域（dependsOn 1＋2＋3）：M1/M2；kernel musl 批件构建绿。
5. [ ] D5 berry-sys 域（dependsOn 3——telface 落地即可启，可与 1/2/4 并行）：W1/W2；musl 构建绿。
6. [ ] D6 ktest 域（dependsOn 4）：K1-K3。
7. [ ] D7 bsh 域（dependsOn 5）：B1-B3；musl 单件构建绿＋步⑫逻辑自核（实跑留客机/主线）。
8. [ ] D8 文档/ABI 批（dependsOn 4-7 终值）：D1-D5；gen_abi 五面再产＋--check 规则＋check-docs 绿。
9. [ ] D9 主线统一验证提交（dependsOn 全部）：分层验证链（见 verification）；git diff --stat 自证七面零触与脏件零碰；沿革五百八十一；smoke 按 ABI 门规则。

## 挂账移交（本刀显式不做）

1. SwitchRec/WakeRec 环读口（op=2 起按落地序取号）；内核唤醒记账 §9 kind/options→Handle 形与现行 (op,buf,len) 形的收口裁决随其刀（本刀不动 §9 不引入 handle 语义——裁决 13）；SwitchRec 24B size 断言补齐随其刀。（兑现注记：op=2 已落——[环读口刀谱](2026-11-19-遥测环读口op2刀实施设计.md)。）
2. MemstatRec 细域扩展（slab_pages/pcp_pages/discardable_pages/wired_pages——现内核零实现源；补真源后走新 op 值或尾追域）。
3. 0x07 INFO_KMEM_STATS(6) 投影导出（ipc 刀——届时引用本案 MemstatRec 单源形状）。
4. free 旗标族（-b/-k/-m/-g/-h）与 --json（cli-suite 外部 free [-h|--json] 面——shell设计 §3 梯队二）；vmstat/btop。
5. [[telemetry_op]] 机器可读 op 表（abi.toml 新段＋gen_abi 新产物）——形状与常量已 telface 单源，机器表随遥测 v3 整刀再议。
6. ProcStatRec/CapInfoRec（可观测性设计:35 同族——独立刀）。
7. IDL 单源生成原型层（v2 挂账维持——berry-sys 既有手写镜像欠债不还不扩；本刀新形状走 telface 单源不新增欠债）。
8. 快照全字段一致窗（generation/seqlock）——字段级原子已满足 free 消费，一致性升级随 ProcStatRec/CapInfoRec 统一裁决。

## 裁决记录

1. 导出通道＝0x84 op=1（三稿共识；否决 0x07 INFO_KMEM_STATS(6) 案）：owner 令明文；0x84 在册 implemented＋klog 先例与 dmesg 消费链同域＋零 handle 统计读面与 memstat 同族；0x07 是 v2 未实现号且需 root_job/RIGHT_INSPECT 权利面对「读五计数」过权；一事实一归属以形状单源收口——通道可多、形状唯一（D5 指向注防双形状并立）。
2. MemstatRec＝五字段 40B（挂账四域案＋reserved；user/abi 稿同判，否决 risk 稿四域 32B 案与 ipc 七字段案）：四域案 free 语义含保留页坑——buddy RESERVED 恒不计 allocated（实证 :84-89），天真 total−alloc 把内核镜像/dtb 保留页误计空闲（free 虚大）；＋reserved 闭合恒等式且消费者可自验（AI 测试友好化）；owner 令「形状定谳」即本谱裁权——挂账四域是随账附送草案非钦定（risk 稿解读过强）；七字段案四源零实现——冻结恒 0 字段=伪 ABI（SDK 历史教训：前瞻形一次定足只对有真源域成立）；字段序 total→reserved→alloc→free→committed（阅读序：总量→不可动→已用→可用→VMO 账）；owner 翻案回四域的降级路径见待裁项 2。
3. free_pages＝读点派生 total−reserved−alloc（三稿共识；否决 buddy 独立空闲计数与 boot 静态快照两案）：派生＝恒等式按构造闭合零新账；静态快照被 A1-3b handoff 段归还破坏（release_range 在档）；独立计数＝第二记账源违可观测性设计:35。
4. reserved＝buddy 对称水位（三稿共识）：reserve_range 实标页 ±、幂等跳过零动；allocated 对拍同法 host 钉板；fa.reserve/fa.release 返回值与 MMAP 打印零改（G3——release 已返回实还页数但语义不动，水位在函数内同步累计）。
5. committed＝vmo.rs 全局聚合原子（abi/risk 共识，否决 user 稿 KL 窗 handle 表全表走查案）：走查案结构性缺口——无全局对象表，per-process handle 表枚举漏 handle 表外内核内部 VMO 即低估＋共享 ObjRef 去重机器重且触 handle/process 冻结邻面＋持 KL 随对象数增长；原子案写点单文件封闭（rg 全量复核）、buddy.allocated 同款聚合账先例；contig 账面口径＝new_contiguous 的 Some(pa)×npages（实证）非 2^order 块——与 ledger 投影语义一致，阶块尾浪费页留 alloc−committed 差额可见=诚实账；禁重复记账源以聚合账 vs 明细账论证消解（V3 入笔记）。
6. 形状与 op 常量单源＝berry-telface（risk 稿发现、本谱现场实证采纳；否决 abi/user 稿 telemetry.rs 定义＋berry-sys 手写镜像双写案）：telface 在册（no_std 纯函数面、kernel/Cargo.toml:33 已依赖、根 workspace member、ringmerge W3-Q81 先例）＋face crate 纪律（hdlface/wsrcface 同款「抽离即唯一事实源」）＋跨 workspace path 依赖先例数十件＋host 可测钉板；双写增殖随 op 域扩展必然，一次收口；既有手写镜像欠债（SchedAttr 等）不还不扩。
7. op=1＝memstat 首号（三稿共识；否决预留 1/2 给 SwitchRec/WakeRec 案）：owner 令 memstat＝v3 首件即首号；「排队在前」（telemetry.rs:13-15）是排期非号位法——add-only 纪律下预留号无执法值（域外恒 -ENOTSUP 与未定义同形）；后续波次按落地序取 2/3+。
8. 分发 match 化＋len 门下放各臂＋尾追追加律（abi 稿为骨）：现状判序实证 op 先行（:521-523→:524-526），match 化保此序使 klog 臂 len 帽下放对 op=0 行为面恒等；klog 16KiB 帽是 op=0 专属语义，memstat 定长 40B 需自定门——len≥40 冻结下限＋实写 40 返 40（否决 len==size_of 精确形：尾追追加律使未来扩字段旧调用方收 40B v1 前缀、返回值自证所得——对外层四十年合同「加字段不改旧结构」的 op 域版立法，入 ABI desc 与可观测性设计）。
9. ktest 臂＝扩进 test_telemetry_open（risk 稿；否决 abi/user 稿新函数新调用点案）：同号同域测试聚合、调用表零变更（末位新号纪律面零动）、静默盖零新行风险面最小；因果差分臂以「末态==初态全等」为主执法（写点净零检测不依赖 VMO eager/demand 语义假定）＋双源对拍＋reserved>0 防忘接。
10. 断言分治＝ktest 静止窗强式/bsh 运行窗弱式（risk 稿为骨＋abi 稿重演算嫁接）：total==alloc+free+reserved 在 struct 内是定义式恒真——free 由单 with_frames 一致窗三元组派生，bsh ⑫ 重演算安全且钉内核派生式契约（内核改派生式不滚文档即红）；committed 与 alloc 分窗读（committed 在 ALLOC_LOCK 外）——反例序列「t1 读 committed=100 → 他窗 decommit 50＋free 50 → t2 读 allocated=50」可造 committed>alloc 假态，故 committed≤alloc 仅静止窗（ktest）断言、bsh 侧禁用；各自≤total 恒安全（total boot 定容常量）。
11. 输出形＝单行紧凑带键值 KiB（abi/risk 稿收敛；否决 procps 两行表、旗标族、user 稿裸 pages 形）：bsh 惯用法＝单行紧凑无列填充机器（uname/uptime 先例）；五数值无标签不可读故带键＋K 后缀（POSIX 默认 KiB 习惯＋后缀自述单位双得）；pages×4 整除精确无损；带参照拒 2（uname/uptime 同形——纯查询无参语义，dmesg 幂等忽略是 clear 参照特例不适用）。
12. 两消费面定谳：bsh 内建＝交互速查简形即终形；cli-suite 外部 free [-h|--json]＝/svc 面板演进件——同吃 0x84 op=1 MemstatRec 单源零第二数据面；shell设计 §3 --json 铁律辖外部查询 CLI 面不辖 bsh 内建（§2.3 梯队表 free 原在演进列——D4 注记收口两形关系，先绑错面=迁移时 MAJOR 断兼容之防）。
13. 内核唤醒记账 §9 零触碰（abi/user 共识；否决 risk 稿随刀翻状态案）：§9 kind/options→Handle 形属 SwitchRec/WakeRec 环读口刀面——memstat 第二档按现行 (op,buf,len) 形延展不结构性取代 §9（环读口是否需要 handle/generation 面仍开放）；提前翻状态=替未来刀预裁＋翻错成本 MAJOR 级；挂账 1 移交。
14. ktest 折入＋零 bench 通道（三稿共识）：bench.py 在途（icount 兄弟 8 行未提交）＋bench 臂须五文件同批滚＋bcts 冻结滚新（@4/@5/@6 判例）——结构性避撞；ktest 静默盖＋CLI 前缀式双臂全覆盖。
15. 版本律三梯各自律（三稿收敛、修 user 稿可观测性 1.0.2 笔误）：可观测性 1.0.1→1.1.0（§5 MINOR 明文——实证头块 Version 1.0.1）；ABI 文档 1.0.2→1.0.3（op 域枚举扩展=§8.2 七枚举 1.0.2 PATCH 判例，文件名不滚——实证头块 Version 1.0.2）；shell设计 1.1.0→1.2.0 连文件名滚（14bf83943 判例；入链计数以实施轮 rg 全仓为准——jobs 刀实滚 25 处在案）。
16. 40 模块总纲表零改动：0x84 是已上线在册号不属其 §2 新增号表（0x07..0x8A）管辖——实证该表无 0x84 行；0x84 op 域在册家＝ABI 文档 §4 0x84 行＋abi.toml desc＋可观测性设计 §3（形状）。

## 风险

最大单点＝committed 写点漏挂（contig 建账/Err 回滚/Drop 三臂/匿名 drain）→ 账实漂移：护栏＝写点单文件封闭（开工 rg 全量复核记档）＋ktest 因果差分臂末态==初态全等（净零检测）＋双源对拍＋committed≤alloc 恒等式静止窗双钉。
match 化动已上线 dmesg 路径：护栏＝G1 逐字节保形（判序/帽/码——现状 op 先行实证）＋既有四负例零改自执法＋dmesg e2e rootfs 相回归。
基线分叉（两线互缺 24 件）与 ABI 红欠账（40≠41 属 F0-4 lane）：硬前提＝步骤 0 主线先并流并核欠账归属；并流前动手＝锚错号位/步数/ABI 产物世代三连；先存红不连坐不代拆。
bsh 步⑫误写强式断言（committed≤alloc→TCG 并行偶发红 loud park）：G4 分治立法＋uptime 数值域模板钉死弱式；⑫纯 render 直调静默零串口新行——boot 相 CLI ok n=13 单行收口零漂移；HELP_LINES 118B 复核义务；canon 新文案三族机查。
telface 接线面：berry-sys 首件依赖（[dependencies] 现空表）＋跨 workspace path（数十先例）——构建隔离 target 首跑验证；kernel 侧换 use 消本地常量零行为面。
shell设计滚名入链漏改：check-docs 相对链接存在性执法＋rg 全仓（含隐藏目录二段清扫判例）复核记档。
体积账：内核＋约 200B（match 臂＋两原子＋装配）、telface＋约 0.5KiB、berry-sys＋约 0.5KiB、bsh＋约 1KiB——九牛一毛；零新 bin 零 manifest 改动。

## 砍单汇总（killList）

落选封存（设计裁决，复苏需翻本刀谱）：0x07 INFO_KMEM_STATS(6) 导出通道案（裁决 1——ipc/design.md 补注消双轨）；ipc 七字段前瞻形 now 定型（裁决 2——四源零真源，冻结恒 0=伪 ABI；补源后走新 op 值）；挂账四域无 reserved 形（裁决 2——free 虚大坑；owner 翻案通道见待裁项 2）；free 落 buddy 第三空闲计数器（裁决 3——第二记账源）；reserved 走 boot 静态快照/读时位图扫描（裁决 3——A1-3b release 破坏/O(n) 不合 TABLE handler）；改 fa.reserve/fa.release 返回值口径与 MMAP 打印（裁决 4——boot 行零漂移破）；committed 走 KL 窗 handle 表全表走查（裁决 5——枚举不全低估＋持锁随对象数增长＋触冻结邻面）；contig committed 按 2^order 块计（裁决 5——账面口径=npages 与 ledger 投影一致）；MemstatRec 定义 kernel 内＋op 常量内核/用户双写（裁决 6——telface 单源收口）；[[telemetry_op]] 机器表/新单源 crate/gen_abi 第六产物（挂账 5——telface 已单源，机器表随 v3 整刀）；ktest 新函数新调用点（裁决 9——扩进 test_telemetry_open）；运行时窗 committed≤alloc 强式断言（裁决 10——分窗反例可造假态）；一切精确值数值断言（G4——恒等式域＋因果差分＋双源对拍）；procps 两行表/列填充/available 列//proc/meminfo 数据源（裁决 11——无 procfs、bsh 零列填充机器）；free 旗标族 -b/-k/-m/-g/-h/--json（裁决 11/12——归 cli-suite 外部演进面）；裸 pages 单位输出（裁决 11——KiB K 后缀定谳）；free 带参照幂等忽略（裁决 11——clear 参照特例不适用纯查询）；唤醒记账 §9 随刀翻状态（裁决 13——环读口刀面）；len==size_of 精确门（裁决 8——尾追追加律下限形）；op=1/2 预留 SwitchRec/WakeRec（裁决 7——add-only 预留号无执法值）；NR 表新号/常量层产 v3 行/nr 计数面滚动（G2）；标记七面任何滚新含注释/新独立字面标记/boot 行漂移（G3）；bench 通道验证臂与 bench.py 滚新（裁决 14/G5——在途脏件）；SwitchRec/WakeRec 环读口与 SwitchRec 24B 断言顺手做（挂账 1）；slab/pcp/discardable 观测面建设（挂账 2——恒 0 域不导出不建设）；IDL 单源生成替换（挂账 7——v2 欠债维持不还不扩）；手改 gen_abi 产物不经再产/替跑 just abi-gen 拆先存红（G2——F0-4 lane 归属）；git 脏件 14 M＋未跟踪三车道全禁碰（G5）；实施轮 git 提交与沿革落笔（G5——主线统一显式路径）；本地 wip 基直接动手（G6 硬前提）；阈值告警/水位监控策略面（free 是快照查询非监控器）；ProcStatRec/CapInfoRec 顺手做（挂账 6）。
砍刀重议触发（实发即停刀重议）：① ktest 因果差分臂末态≠初态或中间态方向断言红（committed 写点漏挂实证）→停刀归因禁带红推进；② 双源对拍任一失合（svc4 读回≠EL1 直调单口——装配路径错位）→停刀归因；③ op=0 klog 既有四负例任一红或 dmesg e2e 红且归因到本刀（match 化漂移）→停刀归因；④ gen_abi --check 再产后红面涉本刀 desc/计数面→停刀与主线重核基线与欠账归属；⑤ 并行 lane 在提交轮前触碰本刀域文件或 gen_abi 五面产物→暂停提交轮重划文件面；⑥ 主线裁改基（origin/wip 重放路）→第 24 内建号位与 CLI n=13 步数按新基复核重算。

## 待裁项（owner）

1. 基线并流路线：先并流再开刀（推荐——两线 24 文件差走「真源重建＋增量补丁＋两向审计」判例）vs 本刀改基 origin/wip 重放 bsh 作业控制刀——主线会话裁决（步骤 0 前置门）。
2. MemstatRec 五域终认：本稿裁五域 40B（＋reserved 闭强恒等式）；owner 若坚持挂账四域原案则 free 退内核派生域不出 ABI、恒等式降弱式、bsh ⑫ 撤重演算断言——形状受对外层四十年冻结合同约束，建议五域。
3. abi.toml desc 滚新时窗：本刀同批滚（推荐——五面再产＋先存红如实挂账）vs 推迟 F0-4 拆红后补滚（desc 与 ABI 文档行短暂失同步记面）。
4. 输出行终形（mem: 前缀/键序/K 后缀）与 HELP_LINES 补名落行（第三行尾——本稿默认）：低风险可直接实施，翻案随 B1/B2 同批。
5. 客机 QEMU 冒验与 smoke 收口时点：ABI 门绿则入链、红则挂账不连坐（jobs 刀判例）；若定硬门槛需先催 F0-4 lane 拆红。
6. 沿革取号五百八十一撞号顺延（流程项非设计项）。