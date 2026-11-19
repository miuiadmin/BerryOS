# BSHRC 宿主写者与 PATH 端到端刀实施设计（berry bshrc 子命令族＋FAT12 就地写面＋两轮共盘 e2e）

Type: plan
Status: draft
Updated: 2026-11-19
落库家：.agents/plans/2026-11-19-BSHRC宿主写者与PATH端到端刀实施设计.md（主线评审后翻 active 交实施轮）

## 一句话说明

owner 令（2026-11-19「彻底实施」续）兑现 export 面刀挂账 #3——宿主 berry CLI 新增 `bshrc` 子命令族（set/get/list/rm 独立族——shell 环境变量与系统设置语义分家；写者＝能力非策略，不造盘不播种），写面＝新模块 tools/berry/src/bshrc.rs 经 berry-fatfs path 依赖（客机 fsmgr 消费同 crate——「写读两端 FAT 实现对拍」按构造成立）对既有 FAT12 数据盘就地写卷根 8.3 名 BSHRC.CFG，提交形＝整盘读入内存编辑（新链分配→数据写簇尾零填→FAT 全副本→目录项 32B 同槽覆写为像内提交点→旧链释放→crashlog read_root_file 独立读面自回验）→同目录 temp 件 fsync＋rename 原子换名（宿主中断窗撕裂结构性不出现——FatErr::Torn 前车在册）；写端帽 fail-closed（总量 ≤2048B 单簇/Entry 行 ≤63/行内容 ≤126B/键 1..=32B/键值禁 CR·LF·NUL——读端 rc_load 截断面不可达）＋env 通道帽（16 项/256B）投影软警不拒写；客机端零改（rc_load 现语义＝写端适配对象）、内核零改零 ABI、settings 五键链零漂移（盘保真断言执法——非 BSHRC 域字节恒等）、七面标记零滚新（e2e 走两轮共盘手跑收档：轮一 PATH=/apps 反证式 miss 文本印 env 原值＋轮二 PATH=/etc 正证式命中 fsmgr 每 boot 预建 /etc/mtab——两轮各自独立可判成对闭环）；写前 run 注册表在用护栏拒活会话（fsmgr eager 整读 FAT0＋根进 RAM——活跑宿主写必被客机下次 commit 覆写）；berry CLI 测试三层滚新（src 单测以 crashlog 独立读面为 oracle＋mkfat dev-dep 字节恒等对拍＝写方言≡造盘方言同源证＋新 tests/bshrc.rs 避 hermes 活跃 lane）；基线以 rg '#[test]' 机械复算钉口径（345@b35c7526b 实测）；挂账 #3 与待裁项 5 收口、刀 11 v2「迁树不迁语义」代际边界成文。

## 背景

（锚点 2026-11-19 逐条实证于 origin/main=b35c7526b——本地检出 65281139d 落后〔tools/berry/src 27 文件差、bsh main.rs 109 行差〕，本文一切锚点取 origin/main 树，实施轮 rebase 后复核；git 现脏恰 bench 三件〔tools/bench.py＋bench_baseline_icount 两件——icount lane c86f662ea 同 lane 活体〕＋未跟踪三车道件全禁碰；wip/sched-1.1 现差十笔零 tools/berry 交集）

1. 挂账与令：挂账 #3 原文（.agents/plans/2026-11-17-export内建与spawn-env面刀实施设计.md:96）「BSHRC.CFG 写者与宿主生成面（树内无写者，PATH 变量唯一来源悬空——which 真值端到端待此）」＋排期条（同文 :143 待裁项 5）；帽差记 #4（:97「ENV 容量帽 256B/16 项扩容路……>256B 走 VMO 路挂 v3」）；env 转发链挂账 #2（:95，27 点恒 &[]）；硬化面 .agents/notes/implemented/2026-11-17-export内建与spawn-env面刀.md:24「挂账不做」行（历史记录不改——本刀只在 plan 侧加尾注）；owner 令明文宿主 berry CLI 写者面（子命令形态自定）＋客机 rc_load 装载→全导出→`run which` PATH 端到端闭环。
2. 客机消费面三段全就绪只缺供数（实证 origin/main）：rc_load（user/bins/bsh/src/main.rs:442-491）——RC_PATH=b"BSHRC.CFG\0"（:446）、读帽 8192B 总量（:460-462）/64 数据行（:467-469）/行 >128B skip（:470-472）/CRLF 剥除在行帽后（:473-480）/键 1..=32B（:481-484，eq==0 或 eq>32 skip）/无 '=' 行不计行不入表（:481）；静默纪律（:438-441 头注——fs 礼物缺席/文件缺席/坏行/读失败零输出零致命）；装载点＝interactive 入口（:501）environ 播种先（:507-512）、rc_load 后（:513-515）同名尾置覆盖胜出；常规链尾与 --no-selftest 两路径同机生效；EFI 无盘路 fat_selftest park 挡断 interactive、裸相 fs_h()==0 零 RPC 早退（:443-445）。全导出集＝build_export_set（:3477-3492，rev 扫最新值胜出）经 bsh 自产 spawn 三点喂 SpawnSpec.env（run 面 :3815-3832）；which＝全树唯一 $PATH 消费者（user/bins/which/src/main.rs:23 未设默认 /bin、:24-25 拆 ':' 滤空段全空塌回 ["/bin"]、:30-37 逐目录 stat 存在即中、:39 miss 打 stderr "which: no {name} in ({path_env})"——印 env 原值＝PATH 字面穿透证介质；exit 全中 0/部分 miss 1/usage 2）。
3. env 双通道与帽（export 刀已落）：svcmgr/berry-term 全 spawn 点恒 env:&[]（svcmgr main.rs:2604 等）；bsh interactive 先播种自己的 environ（crt0 第二消息——产侧 berry-exec env_blob KEY=VALUE\0 续装串帽 256B＝CHAN_MSG_MAX、非空才发〔user/berry-exec/src/lib.rs:400-405/:827-833〕；子侧 16 项/256B 单调截断〔user/berry-libc/src/io.rs:166-210〕）再 rc_load——BSHRC 是 console/GUI 终端 bsh 里 PATH 的唯一活来源；BSHRC 64 行帽 vs env 16 项帽量级不匹配＝挂账 #4（:97 已文档化）；`run`/`mkdir` 均在 BUILTINS 单源表——`run which` 即闭环末段。
4. 落点契约（钦定）：docs/存储布局与沙箱设计-v1.0.md:168——跨 boot 持久层 v1 落点＝FAT 数据盘卷根 8.3 文件族（SETTINGS.CFG〔五键，含 legacy SETTINGS.JSN 迁移〕/BSHRC.CFG/LAYOUT.JSN），宿主侧盘复用制 profile 路径 ~/.berry/data/<profile>.img 一次造跨启动复用，「berry 只复用不造盘、首造归 just desktop」，smoke/CI 路径每次重造零残留；v1 写面只认单段 8.3 大写根名；「届时文件族迁树不迁语义」。
5. 宿主 FAT 现状：berry CLI 自身 FAT 面只读——crashlog.rs 手搓直读（FatErr :23-42 含 Torn/Loop、bpb_ok 私有 :55-63、pub fn read_root_file(d: &[u8], name: &str) :66 双探 {LBA0,LBA16} 内含、单测 mkfat_disk 助手 :435-466）；Cargo.toml 零 FAT 库依赖、berry-cbox path 依赖先例在册（跨 workspace path 可行）；树内唯一宿主侧盘内原地写器＝tools/gui8-smoke.py Fat12Volume（add-only 拒已存在名、七面冻结件禁触）；mkfat（tools/mkfat 独立小工作区零依赖纯 std）是造盘器不写已有盘——build_volume(listing, dirs)（lib.rs:229，listing 可指定全根文件族）、fat12_set :68/dir_entry :102/name_to_11 :114/clusters_pad :125、几何常量 :24-37（RESERVED_ZONE 16/SPC 4/NUM_FATS 2/ROOT_ENTRIES 16/DATE 0x5021/TIME 0）、506 簇 FAT12 域（:237-239）；mkfat.py --put 可重复但走 build_volume_puts EFI 形（vol@LBA0 无保留区）且「默认盘字节零漂移硬线不破」头注明文——测试预置面恰好够用（crashlog/cli.rs 16 扇区零前缀先例）。
6. 写基元单源在册：user/berry-fatfs（cfg_attr(not(test), no_std) 纯函数零依赖）——parse_bpb :163（簇数域判宽：FAT12 <4085/FAT16/None 三档；卷相对扇区＋hidden 折算头注在册）、fat12_entry :238/fat12_set :263/find_free_cluster :321/count_free_clusters :347/encode_file_entry :401/name_to_fat11 :447（只收 [A-Z0-9]）/FAT12_EOC :51；客机 fsmgr 消费同 crate（user/bins/fsmgr/src/fat.rs:1-13 头注「消费 berry-fatfs 纯编解码」＋bps==512 断言＋域检双拒面）；mkfat.py↔berry-fatfs↔fsmgr 三方 fixture 共识执法在册（「改 mkfat 须重产 fixture」）；接线注：berry-fatfs Cargo.toml 为 version.workspace = true（berry-cbox 是字面量版本先例）——workspace 继承经目录上溯解析，首跑验证（见风险）。
7. 客机写路与 e2e 先例：fsmgr staging＋TRUNC 即 commit（fat.rs:22-25 头注——近原子：commit 前读者只见旧版/无文件；「FAT 文件删除留 v2、`>` 只落根」诚实缺口在册）；bsh `>` 重定向写 8.3 大写名即此路（客机内潜在手工写者）；2026-11-15 计划 :111 两轮共盘 e2e 先例（客机 `echo GREETING=... > BSHRC.CFG` 落盘→新会话展开恰值）已证客机写路径——宿主写→rc_load 消费即本刀要闭的未测链；smoke/CI 每次重造盘结构性不可验证持久化（justfile:557-559 判例明文）——持久化验证须走复用盘。
8. 盘位与生命周期（实证）：default_data_disk（tools/berry/src/qemu.rs:401-403）＝home()/data/<profile>.img；data_disk_plan 单源三级（:412-423——BERRY_DATA_DISK 显式路径 > "off" 显式关 > 缺省位存在即挂/缺席无盘，canonicalize 防相对路径）；home() 单源（run_dir.rs:10-23）；just desktop 首造（justfile:1866-1876——Rust mkfat 造盘＋dd LBA0 BERRYOSVBLK! 印章，此后存在即复用）；doctor DataDiskReport（doctor.rs:32）逐 profile data_disk_plan 同源推演（:81）；run 注册表基座（meta.json 含 profile/qemu_pid、state.json 活态、find_run 探活）＝在用护栏可实现面；ENV_LOCK 测试串行锁（run_dir.rs:227-230——改 BERRY_HOME/BERRY_DATA_DISK 的测试互斥）。
9. 测试形态与基线（实证）：三层惯例＝src 模块内单测用真 mkfat.py 产物（crashlog.rs :435-466——mkfat 缺席即失败不静默跳过）＋tests/ 真进程集成（tests/cli.rs:1-51——CARGO_BIN_EXE_berry、钉退出码、--json stdout 纯 JSON/诊断走 stderr、mkfat_disk 助手 :29-51）＋cli_flags.rs rc=2 usage 矩阵；跑法 EPERM 坑（README.md:20-23——--target host＋隔离 CARGO_TARGET_DIR；测试节 :257-265）；新子命令先例＝crash-log 三件套（模块单测＋集成测＋README 命令节 :48-54）；基线实测 rg '#\[test\]' 合计 345@origin/main（src 340＋tests 5）；口径漂移史 137（SKILL.md:43）/160/169/339 无单一真相源——本刀立机械复算口径；berry CLI 真实并发面＝hermes W1 测试刀系持续经 PR 落 main（ddb23def7 dispatch 族已立 cli_flags.rs）。
10. 图纸与纪律面：全 plans 零 BSHRC 写者设计（十八刀计划与 .agents/plans/{cli-suite,bsh}/design.md 零命中——无需避让）；刀 11（十八刀计划 :33/:60）＝v2 配置体系（/etc/profile＋/userdata/home/.bshrc＋set -euo pipefail）未启动——本刀 v1 增量；markers.sh ROOTFS_MARKERS 表（:36-37，"CLI ok n=" 前缀式）＋canon 互非子串纪律（:51-58）；沿革 origin/main 尾节五百八十三（docs/阶段沿革.md:7702）、wip 在途占 584-588——落笔实时重数顺延。

## 契约

### 全局契约（六域共守）

G1 客机端零改＋内核零改零 ABI：user/ 与 kernel/ 全域零 diff（bsh rc_load/build_export_set/which/fsmgr/settings/svcmgr/berry-term/berry-libc/berry-exec/kernel/ABI）；rc_load 现语义（帽/静默/CRLF/装载点 interactive 入口）是写端适配对象非改动对象——「写端适配读端而非反之」字面执法；CLI ok n=13 单源（bsh main.rs:5932）与 cli_selftest 链序契约（:398-405）零改。
G2 settings 五键链零漂移：写面只动 BSHRC.CFG 名下数据簇、其 FAT 表项、其根目录槽三面；SETTINGS.CFG（含 legacy SETTINGS.JSN）/LAYOUT.JSN/CBOX.LOG/HELLO.TXT/PATTERN.BIN 等全盘其余字节恒等——测试执法＝盘保真断言（写前后全盘 diff 恰限定三面）；user/bins/settings 零 diff；宿主写者永不造 settings 面。
G3 七面零滚新含注释＋零新标记：markers.sh/smoke.sh/smoke-smp.sh/smoke-image.sh/bcts 冻结件/bcts-run.sh/gui8-smoke.py 全零触；零新 smoke 标记、boot 串口逐行零漂移（e2e typed 行在链后交互态，不入 smoke）；新 CLI 固定文案（usage/诊断/警告行）与现役标记全表（ROOTFS_MARKERS＋RX 族）两两互非子串——实施毕 canon 机查记档（markers.sh:51-58 先例）。
G4 造盘律保持：berry 仍只复用不造盘（存储布局 :168 措辞）——缺盘报错附 home()/data 盘清单与「先 just desktop 造盘或 --disk 指路」；绝不调 mkfat 造盘、绝不新建盘件；写已有盘是升格能力，README 与存储布局 :168 注记同批同步为「复用＋可写已有盘、仍不造」。
G5 脏件禁碰与并行纪律：bench 三脏件＋未跟踪他 lane 件全禁碰；实施轮文件域互斥、禁 git 提交（主线统一显式路径暂存，禁 add -A）；构建/测试隔离 CARGO_TARGET_DIR（README :20-23 EPERM 坑）；每域开工 git status 复核＋rebase 最新 origin/main（hermes lane 活跃）；docs/阶段沿革.md 实施轮全周期禁碰（主线 HEAD 重同步后追加，取号实时重数——origin/main 尾五百八十三、wip 在途 584-588，撞号顺延不预占）。
G6 基线机械复算口径：berry CLI 测试基线以 rg '#\[test\]'（或等价 git grep -c）合计数为唯一口径（345@b35c7526b 实测）；实施笔记记前后两数与命令；cargo test 实数为后值；.agents/skills/berryos-dev/SKILL.md:43 旧值 137 同批滚新并注口径；137/160/169/339 旧口径不追认只记差。
G7 停机写与在用护栏：写者面向停机盘态——写前扫 home()/run/* 注册表（meta.json profile＋qemu_pid＋state 活态＋探活），活会话的缺省盘与目标 canonical 路径相等即拒写并附 run id 与「先 berry device stop」指路；berry 体系外手起 QEMU 无 meta＝残余风险 README 文档化（不筑锁）。
G8 挂账对齐义务：同批关闭 export 计划 :96 挂账 #3（行尾兑现尾注——#1/#2 同款尾注法）＋:143 待裁项 5；:95 挂账 #2 与 :97 挂账 #4 维持另刀不连坐；note :24 历史记录不改；十八刀计划与 cli-suite/bsh design 零触；不造与 .bshrc 竞争的第二配置格式（刀 11 v2 边界）。

### 契约 W：FAT12 就地写面（src/bshrc.rs 写面＋依赖接线）

W1 模块与依赖：新模块 tools/berry/src/bshrc.rs（crashlog.rs 先例形——实现＋单测同模块）；Cargo.toml [dependencies] 加 berry-fatfs = { path = "../../user/berry-fatfs" }（berry-cbox path 先例）＋[dev-dependencies] 加 mkfat = { path = "../../tools/mkfat" }（仅测试字节恒等对拍消费，零外部 crate）；version.workspace 继承接线首跑验证——失败即停刀上报主线重裁，禁改 user/ 侧任何文件。
W2 几何与卷定位：berry-fatfs parse_bpb 动态几何（卷相对扇区＋hidden 折算——fatfs 头注纪律，不硬编 mkfat 常量）；卷探 {LBA0,LBA16}＋hidden==候选位对账（fsmgr 形，严于 crashlog 读面 bpb_ok 形）＋bps==512 与 fat_bits==Fat12 双断言（fsmgr 双拒面镜像——畸形 BPB 防御）；簇数 ≥4085 域外拒形带诊断；crashlog.rs 零改（read_root_file :66 已 pub，独立读面 oracle 专用）。
W3 文档模型：文件＝有序行序列 Line∈{Comment(原字节)|Blank|Entry(key,value)}；解析 rc_load 同款（split '\n'/剥尾 '\r'/首 '=' 切键/键 1..=32B/无 '=' 行不入键面不计行）；重复键折叠 last-wins（get/list 投影取末值；set 改末次出现清更早重复、缺席文件尾追）；注释/空行/坏行原样保留原位（客机 `>` 手写件不破坏——静默丢注记违诚实面）；序列化 LF-only、行尾换行收尾；多簇旧件（客机 `>` 手写形）list/get 只读容忍（≤8KiB/64 行同 rc_load 帽），set 收拢单簇新链旧链释放。
W4 写端帽 fail-closed＋env 软警：硬帽——序列化总量 ≤2048B（单簇——settings_io 单簇覆写先例同款）、Entry 行 ≤63、单行内容 ≤126B（读端 128B 行帽剥 CR 余量）、键 1..=32B、键与值禁 \r\n\0，超帽拒绝报错指名违例帽（读端 rc_load 截断面结构性不可达）；软警——last-wins 唯一键集按 build_export_set 语义投影（KEY=VALUE\0 续装串），项数 ≥16 或 blob ≥256B 时 stderr 警告「子进程 env 通道帽（16 项/256B）将截断」（挂账 #4 v3 扩容前的诚实防线，不拒写）。
W5 提交序（撕裂不可达形）：整盘读入内存副本→①旧链 collect 暂记（fat12_entry 走查含环防护；缺席则记无旧链）②新链分配（find_free_cluster 自 2 升序——mkfat/fsmgr 确定性惯例；空内容特判零簇）③数据逐簇写（簇尾 0x00 填）④FAT 全副本（nfats 逐份）写新链（末簇 FAT12_EOC）⑤目录项 32B 一次写（encode_file_entry——同槽覆写 first_clus/size；缺席取首个 0x00/0xE5 空槽；attr 0x20/date 定值/time 0——与 mkfat 及客机 fsmgr 写字节同形；根 16 槽满＝拒写附在场名清单）⑥旧链 FAT 全副本释放（改 0）⑦内存副本经 crashlog::read_root_file 自回验（内容逐字节等值）→落盘＝同目录 temp 件 write_all＋fsync＋rename 原子换名（失败清 temp；原盘模式位保随）。撕裂分析：rename 前任意宿主中断＝旧值完整（整件原子换名）；像内序目录项末位＋旧链释放后置＝并读窗亦无 Torn 可读形（严格优于 fsmgr v1「FAT 已写根未写」文档窗）；①-④ 中断的孤儿簇零落盘（内存态丢弃）。
W6 失败面 loud：无卷（裸件/非 mkfat 形）/FAT16 域/坏 BPB/根满附占用清单/无空闲簇附 free 计数（count_free_clusters）/盘缺席附指路/BERRY_DATA_DISK=off/写后回验失败——全走 BerryError 既有家族（error.rs 附录 E 构造器：device(6)/env(3) 形实施轮对齐，不发明新码）非零退出禁静默。
W7 rm 语义：删该键全部 Entry 行（注释保留）；可致空文件形——目录项留驻 first=0/size=0（fsmgr「空文件两端都解析」惯例同款）；FAT 文件删除面（0xE5/槽回收）挂账 v2（fsmgr「FAT 文件删除留 v2」fat.rs:21 同界）。

### 契约 C：命令面（main.rs＋lib.rs＋doctor 薄面）

C1 族形：Cmd 枚举追加 Bshrc(BshrcCmd)（main.rs :40-94 邻位，crash-log wiring 先例）；子动作 set KEY=VALUE...（一或多对同批，模型改毕一次落盘）/get KEY/list/rm KEY；flags --disk PATH 与 --profile ID 互斥（--profile 缺省 desktop、取值校验沿 profile 单源）＋全局 --json 照用。
C2 盘解析：--disk 显式最优先（canonicalize）> BERRY_DATA_DISK（"off"＝显式关→写语境报错说明；路径直用）> default_data_disk(--profile)——与 data_disk_plan（qemu.rs:412-423）三级同序但独立薄函数（写语境须拆「off 显式关」与「缺盘」两类可行动报错——挂载语义 None 是合法态不可复用本体）；缺盘＝G4 报错＋home()/data 盘清单＋just desktop 指路。
C3 在用护栏：写前扫 run 注册表（G7 规则）；仅 berry 自有会话可检，残余文档化。
C4 退出码与输出：clap usage 恒 2（usage 拒形钉进新 tests/bshrc.rs）；语义失败非零（W6 族＋get/rm 键缺席 rc≠0＋stderr 诊断）；--json 形 stdout 纯 JSON（tests/cli.rs :1-51 契约——get 值对象/list entries 数组/set·rm 结果对象含 path/key/bytes/clusters；错误对象沿 error.rs 既有 t=error print 契约零改）；human 形 list 打 KEY=VALUE 行（与文件形一致脚本友好）、get 打裸值、set/rm 成功零 stdout；诊断全走 stderr。
C5 doctor 薄面（可裁——待裁项 1）：DataDiskReport 逐 profile 增 bshrc_present 布尔（经 crashlog 读面缺席探测——静态只读零副作用纪律保持，fwcfg/data_disk 推演先例延伸）＋--json 面＋README 一行。

### 契约 T：测试三层

T1 src/bshrc.rs 单测（真 mkfat.py 产物——crashlog :435-466 mkfat_disk 助手同款泛化：--put 可重复预置多件＋16 扇区零前缀对齐主路盘形态；mkfat 缺席即失败不静默跳过）：模型族——parse/serialize round-trip、注释坏行保留、last-wins 折叠、帽四拒绝、空值、CRLF 读容、软警投影；写面族——新建→read_root_file 回读字节恒等、同键更新/新键尾追/rm 至空文件形、多簇旧件（--put 预置 >2048B BSHRC.CFG 模拟客机 `>` 手写）list 解析＋set 收拢单簇、盘保真（预置 SETTINGS.CFG＋LAYOUT.JSN 写前后全盘 diff 恰限定 BSHRC 三面）、根 16 槽预满→报错附清单、裸件无卷→报错、写后空闲簇守恒差分；字节恒等对拍（mkfat dev-dep：build_volume(Some(自定义双件＋BSHRC.CFG))＋16 扇区前缀 vs berry 写者在 build_volume(Some(自定义双件)) 前缀盘上写同内容——整盘逐字节等值＝写方言≡造盘方言同源证；不等即停刀归因）。
T2 tests/bshrc.rs 新文件集成测（真进程 CARGO_BIN_EXE_berry；独立新文件避 hermes 活跃 lane cli.rs/cli_flags.rs 撞面）：退出码族（usage 2/键缺席/帽拒/盘缺/off）、--json stdout 纯 JSON 可解析、human 诊断走 stderr、--disk 显式（零 ENV_LOCK 触碰——须 env 的缺省解析用例才走 ENV_LOCK 串行锁 run_dir.rs :227-230）、序贯 set→get→list→rm→get-miss 流。
T3 基线：rg '#\[test\]' 合计前后记档（G6）；README 命令节 berry bshrc 条目同批。

### 契约 E：e2e 两轮共盘收档（零标记面）

E1 rig 形态：手跑存证＋判据留档逐字执行＋日志存证（2026-11-15 计划 :111 客机写两轮先例的宿主写镜像形）；载具＝berry device start --profile desktop（缺省盘）＋串口喂命令（console/logs 面按当日起态选）；击杀纪律按 QEMU 字段协议（钉脚本 PID 整体退出、击杀哨只钉自己私有路径特征）；轮间 device stop 后再写（G7）。
E2 轮一（真值递达反证式）：宿主 `berry bshrc set BSHRC_E2E=host-writer-pass`＋`set PATH=/apps` → boot → 串口断言三值：`echo $BSHRC_E2E` 恰印 host-writer-pass（宿主写→FAT→rc_load→vars 链）；`echo $PATH` 恰印 /apps（装载真值——which :23 未设亦默认 /bin，只看 which 输出不可分辨装载与回退）；`run which mtab` miss 行恰 "which: no mtab in (/apps)"（which :39 印 env 原值＝PATH 字面穿透证——回落值无以伪造 /apps 形；不依赖 /apps 目录存在）。
E3 轮二（命中闭环正证式）：宿主 `set PATH=/etc` → 同盘再 boot → 断言：`echo $PATH` 恰印 /etc；`run which mtab` 恰印 /etc/mtab（fsmgr 每 boot 预建件 main.rs:70-85 为命中真源——免改客机免依赖自建件；缺省 /bin 无 mtab 必 miss，命中即 BSHRC 真值递达）；轮二本身即跨 boot 持久证（轮一写入轮二复用同盘）。两轮各自独立可判、成对闭环。
E4 负控相（可裁——待裁项 6）：无 BSHRC 的 fresh 盘 boot → `echo $PATH` 空串＋`run which nosuchx` 断言 in (/bin)（which 回落不变量）。侦察排雷录：u4 ⑥ 自建 /bin/wc 用毕即清（bsh 自验 unlink 收尾）不可作命中真源；PATH=/ 全空段塌回缺省 ["/bin"]（which :24-25）不可作自指形；PATH=/bin 单值与回退不可判（不作轮值）。
E5 G3 执法：两轮 boot 链标记行与无盘基线逐行恒等对照（typed 行在链后交互态）；e2e 不入 smoke/CI、不造标记、不触七面；TCG 载荷竞速适用状态裁定判读（同码异果先查载荷）＋失败重跑条款入笔记。

### 契约 D：文档与挂账批

D1 tools/berry/README.md：`berry bshrc` 命令节（四子命令＋--disk/--profile＋帽与软警语义＋停机写纪律与在用护栏＋PATH/which 端到端例＋刀 10 后 PATH 自然值预告）；测试节零改（配方现成）。
D2 挂账关闭：export 计划 :96 挂账 #3 行尾兑现注记＋:143 待裁项 5 行尾注记（#1/#2 同款尾注法）；:95/:97 挂账 #2/#4 维持另刀不连坐；note :24 历史记录不改。
D3 docs/存储布局与沙箱设计-v1.0.md :168 文件族行补「宿主写者＝berry bshrc（BSHRC.CFG 专写，仍不造盘）」一句＋Version 修订行随最小 PATCH 惯例。
D4 .agents/skills/berryos-dev/SKILL.md:43 基线行滚新（rg 口径注记）。
D5 新笔记 .agents/notes/implemented/2026-11-19-BSHRC宿主写者与PATH端到端刀.md（裁决与砍单全文＋验证账＋基线前后数＋e2e 存证路径＋QEMU 互踩运维规则＋刀 11 v2 迁树预告）＋node .agents/tools/check-docs.mjs 绿。
D6 沿革：主线 HEAD 重同步后追加，取号实时重数（origin/main 尾五百八十三、wip 在途 584-588——撞号顺延不预占）。

## 步骤

0. [ ] P0 前置（主线职责）：重同步最新 origin/main（hermes lane 在途）；本刀域 porcelain 复核；基线 rg 计数实测记档（现值 345）；沿革尾号实数；客机锚点复核表（rc_load 帽/装载点/mtab 预建/fsmgr commit 序——漂则按新基改谱再动手）。
1. [ ] D1 写者模块域（dependsOn 0）：W1-W7＋T1；berry-fatfs path 依赖首跑验证；cargo test 隔离 CARGO_TARGET_DIR 绿。
2. [ ] D2 命令面域（dependsOn 1）：C1-C4＋C5（可裁）；lib.rs 挂模；构建绿＋usage 拒形自核。
3. [ ] D3 集成测域（dependsOn 2）：T2 绿＋T3 基线复核。
4. [ ] D4 e2e 收档域（dependsOn 3）：E1-E5 两轮共盘手跑＋判据逐字执行＋日志存证；boot 链逐行对照。
5. [ ] D5 文档与挂账批（dependsOn 4）：D1-D5 文档面＋check-docs 绿。
6. [ ] D6 主线统一验证提交（dependsOn 全部）：分层验证链全绿；git diff --stat 自证七面零触/客机零 diff/bench 零碰；沿革实时取号；显式路径提交。

## 挂账移交（本刀显式不做）

1. svcmgr/launcher/berry-term env 转发链 27 点（export 计划 :95 维持——BSHRC 只进 bsh vars/export 集不回灌任何 spawn 产点 env 字段；本刀 e2e 数据即其收益定案输入）。
2. ENV 容量帽扩容（>256B 走 VMO 挂 v3——export 计划 :97 维持；本刀仅写者侧投影软警）。
3. 刀 11 v2 配置体系（/etc/profile＋/userdata/home/.bshrc＋set 内建——十八刀计划归属不动；本刀 v1 KEY=VALUE 单格式＋迁树预告，不造第二格式不碰 set 内建不碰 .bshrc 名）。
4. FAT 文件删除面（0xE5/槽回收——W7 rm 留空文件形；fsmgr「FAT 文件删除留 v2」同界）。
5. mkfat 首造播种 PATH/BSHRC 缺省值（裁决 6——能力非策略；PATH 字面值由用户/验收例显式写）。
6. 多簇 BSHRC 写面（>2048B——W4 帽拒绝；读帽 8KiB 与 env 256B 之间无现实需求层）。
7. 活跑检测全语义/盘锁执法（G7 仅 berry 自有会话——run index 探测扩展随需求另刀）。
8. /apps 目录天然来源与 mkdir 演示面（判据已解耦——miss 文本不依赖目录存在、命中走 /etc/mtab 预建件）。
9. settings 运行时验证移交项（2026-11-15 计划 :113——rig 形态同源可复用；归属 settings 面，笔记互链即可）。
10. 根目录 16 项扩帽（mkfat 几何与 fixture 双端重产归 mkfat 面——W6 满根报错附清单即护栏）。
11. LAYOUT.JSN/通用配置写者命令面泛化（写面按名参数化天然支持——命令面只开 BSHRC.CFG，需求出现再扩）。
12. crashlog/bshrc 读面公共几何层抽取（本刀 crashlog 零改、双读面各自钉板——整洁刀另议）。

## 裁决记录

1. 写者归宿＝宿主 berry CLI 子命令族（三稿共识；owner 令字面）——否决客机 bsh 内建写者（违 G1 客机零改，且 rc_load 消费者兼任写者＝写读同体自举纠缠）与 settings 式客机应用扩面（第二常驻服务面；五键 canonical JSON 冻结 schema 对开放键空间平文本不称）。
2. 子命令形＝`berry bshrc` set/get/list/rm（writer 稿名为骨）——独立族否决并入 settings 键族（settings 五键是客机应用自治面、宿主并族＝双写者并立犯 G2）与 `berry config` 泛名（语义过宽与 v2 刀 11 配置体系撞义）；名贴 v1 契约文件 BSHRC.CFG，刀 11 迁树时命令面不动落点随迁；unset/del 为同义别名翻案通道（随 C1）；clear 整件删除砍——牵 0xE5 删除面（挂账 4），rm 留空文件形已覆盖语义（loop/risk 的 del/clear 形收敛）。
3. 写基元单源＝berry-fatfs path 依赖（writer/risk 共识裁 loop 稿 mkfat 主依赖案）——客机 fsmgr 消费同 crate（fat.rs:1 头注在册）＝「写读两端 FAT 实现对拍」按构造成立；parse_bpb 动态几何＋写基元完备（fat12_set/find_free_cluster/count_free_clusters/encode_file_entry/name_to_fat11）严于 mkfat 常量几何面；mkfat 降级 [dev-dependencies] 只做字节恒等对拍 oracle（loop 稿最强测试嫁接保留）；手搓第四套 FAT12 编码违一事实一归属（三方 fixture 共识执法在册）；移植 gui8 Fat12Volume 出局（七面冻结件＋跨语言双写）。
4. 落盘形＝整像内存编辑＋temp+fsync+rename 原子换名（loop/risk 共识裁 writer 稿原位提交序案）——宿主中断撕裂面前车＝crashlog FatErr::Torn 实测形态（crashlog.rs:26-27 注释在册）；rename 使宿主侧任意中断窗外＝旧值完整；writer 裁决所拒「整盘重建形」＝mkfat 重造灭客机写态——内存编辑保全盘其余字节＋同路径换名，非重建形与 G2 兼容；writer 的像内提交序（目录项 32B 末位＋旧链释放后置）保留为内存编辑纪律——并读窗亦无 Torn 可读形，严格优于 fsmgr v1「FAT 已写根未写」文档窗（fat.rs:24 近原子边界）。
5. 帽＝单簇 ≤2048B fail-closed（writer 稿裁 loop/risk 读帽满配 8KiB 案）——链长恒 1 使提交序最简＋近原子面最大化；settings_io 单簇覆写先例同款；读端 8KiB/64 行帽与写端 2048B 帽之间 fail-closed 收口；多簇旧件 list/get 只读容忍＋set 收拢单簇（loop/risk 读容忍嫁接）；软警阈值＝env 通道帽本体（16 项/256B——berry-exec :400-405 可机械复算），不取 writer 稿 14 键/200B 预留魔法数。
6. 盘解析＝data_disk_plan 三级同序独立薄函数＋不造盘律（三稿共识；risk 稿 off/缺盘拆分可行动报错采纳）——「berry 只复用不造盘、首造归 just desktop」（存储布局 :168＋qemu.rs:411 注释）延展到写面；BSHRC 缺席是合法形态（rc_load 静默纪律 :449-451）不代造不播种；PATH 缺省值不内置（which 回落 /bin 已在 which :23；播种造第二真相源违一事实一归属）。
7. 在用护栏＝run 注册表扫描拒写（risk 稿嫁接；writer/loop 纯文档纪律案否决）——fsmgr eager init 整读 FAT0＋根目录进 RAM（fat.rs:12-13）：活跑会话中宿主写盘，客机下次 commit（settings 覆写/bsh `>`）必以 RAM 旧态覆写宿主改＝双向毁写态事故，正对 owner「现用户数据不破坏」硬约束；berry 自有 run 注册表基座现成（meta profile/qemu_pid/state 探活）；残余（体系外手起 QEMU）README 文档化不筑锁。
8. e2e＝两轮共盘手跑存证（writer/risk 稿为骨；loop 稿脚本化 rig 列待裁项 3）——判据三件套：`echo $PATH` 逐字（risk 稿——装载真值唯一判据，破 which 回退 /bin 遮蔽）＋轮一 /apps miss 文本反证（writer 稿——which :39 印 env 原值＝PATH 字面穿透证）＋轮二 /etc/mtab 命中正证（writer 稿——fsmgr 每 boot 预建件 :78-85 唯一免改客机稳定命中真源）；负控相（risk 稿）列可裁；排雷录三条入谱（自建件用毕即清/PATH=/ 塌回/PATH=/bin 单值与回退不可判——risk 稿轮值弃用）。
9. 客机 e2e 不走标记吸收（三稿共识）——cli_selftest 链序契约「排 fat 族之前（EFI 路无盘亦打 CLI）」（bsh :398-405 注释区）使 PATH 真值步结构性依赖盘上 BSHRC＝破无盘不变量；FAT ok n=/FATW ok n= 语义已占位非 PATH 面；零新标记约束下手跑存证＝2026-11-15 :111 客机写两轮先例的宿主写镜像形。
10. 集成测家＝新 tests/bshrc.rs（writer 稿裁 loop/risk 稿 cli.rs·cli_flags.rs 追加案）——hermes W1 测试刀系活跃 lane（ddb23def7 dispatch 族直测已立 cli_flags.rs、持续经 PR 落 main），新文件零撞面；cli.rs 惯例（mkfat_disk 助手/--json 契约/退出码钉）在新文件复刻。
11. 基线口径＝rg 机械复算前值（345@b35c7526b 实测）＋cargo 实数后值＋SKILL.md:43 同批滚（risk 稿裁 writer 稿「不滚」案）——技能文档是现行态载体，留 137 旧值违「只写当前状态」；CI 无 berry 计数门禁现状如实记不本刀建。
12. doctor bshrc_present 薄面默认做可裁（三稿两含一推，取含）——静态只读零副作用纪律保持。
13. crashlog.rs 零改（writer 稿目标裁 risk 稿 pub(crate) 微调案）——read_root_file 已 pub（:66）足为独立读面 oracle（与 berry-fatfs 不同码路径的真独立对拍）；卷发现走 berry-fatfs parse_bpb 不需 bpb_ok 外扩；两读面双立暂受（挂账 12）。
14. mkfat 零改（三稿共识）——否决 --put 播种/ROOT_LISTING 增项/首造播种（mkfat.py 头注「默认盘字节零漂移硬线不破」：--put 走 build_volume_puts 独立构造体；mkfat 是造盘器与写者职责分立）；mkfat.py --put 可重复＋EFI 形恰好构成测试预置面（crashlog/cli.rs 16 扇区零前缀先例）。
15. 图纸对齐＝无避让＋代际边界成文（三稿共识）——全 plans 零 BSHRC 写者设计（十八刀计划与 cli-suite/bsh design 零命中实证）；刀 11 v2 归属不动；本刀 v1 KEY=VALUE 单格式＋「迁树不迁语义」（存储布局 :168 钦定句）预告，零代码冲突。
16. 模块形＝单模块 bshrc.rs（crashlog.rs 先例形；loop 稿 fat12.rs/risk 稿 fatvol.rs 分模块＋commands.rs 增载案否决）——commands.rs 3465 行巨文件零增触（hermes 撞面最小化）；实现＋单测同模块（crashlog :435-466 先例）。

## 风险

最大单点＝方言失配（写者产盘客机 rc_load 读不出）且 rc_load 静默纪律掩盖失败：护栏四层＝berry-fatfs 同源 codec（客机 fsmgr 同 crate）＋W5 写后独立读面自回验＋mkfat dev-dep 字节恒等对拍（方言同源机查）＋e2e 两轮真机判据（miss 文本/echo $PATH 值敏感）——静默不可能四层全绿而错；红则停刀归因禁带红推进。
毁客机写态（SETTINGS.CFG 等被误写/覆写）＝持久化契约事故：护栏＝整像内存编辑（其余字节按构造零变）＋T1 盘保真断言＋rename 原子（提交点前旧件完整）＋在用护栏拒活会话（fsmgr RAM 态覆写面封堵）；残余（体系外 QEMU）README 文档化。
hermes lane 持续落 main（本刀真实并发面）：护栏＝新文件为主＋main.rs/lib.rs/Cargo.toml 小触面＋步骤 0 重同步后开工＋提交轮前 git log 复核撞面。
berry-fatfs version.workspace 接线（跨 workspace path 依赖的继承解析——berry-cbox 先例是字面量版本，不完全覆盖）：D1 首跑验证；失败即停刀上报主线重裁，禁改 user/ 侧 Cargo.toml（G1）。
客机锚点漂移（重同步后 rc_load 帽/装载点/mtab 预建/commit 序变形）：P0 复核表执法，漂则按新基改谱再动手。
根 16 槽满（烘制 2＋客机族累计）：W6 报错附在场清单转 loud；扩帽归 mkfat 几何面另刀（挂账 10）。
TCG 载荷竞速（e2e 偶发）：状态裁定判读＋重跑条款（QEMU TCG 条目惯例）。
沿革撞号（wip 在途 584-588 密集）：主线实时重数顺延。
体积账：berry CLI 增一模（模型＋写面＋cmd 约 600-800 行含测试）＋berry-fatfs/mkfat path 依赖零客机面——宿主工具面，零镜像/manifest 改动。

## 砍单汇总（killList）

落选封存（设计裁决，复苏需翻本刀谱）：客机侧写者形态（bsh 内建写者/settings 式客机应用——裁决 1）；并入 settings 键族/宿主 settings 子命令造族（裁决 2——双写者并立犯 G2）；`berry config` 泛名族（裁决 2）；clear/unset/del 别名臂（裁决 2——四动词定形，翻案随 C1）；mkfat 主依赖写面（裁决 3——降级 dev-dep oracle）；手搓第四套 FAT12 编码/移植 gui8 Fat12Volume（裁决 3——一事实一归属＋七面冻结件）；原位逐扇区就地写（裁决 4——宿主撕裂面前车 FatErr::Torn）；整盘重建形（裁决 4——灭客机写态犯 G2）；同簇就地覆写锚定形/临时簇＋rename 双目录项写形（裁决 4——像内撕裂窗/无原子 rename）；读帽满配 8KiB 多簇写面（裁决 5——单簇 2048B fail-closed 收口）；软警预留魔法数阈值（裁决 5——帽本体可机械复算）；mkfat --put 增 BSHRC 项/just desktop 首造播种/ROOT_LISTING 增项（裁决 14——默认盘字节零漂移硬线＋只解首造不解改值）；PATH 缺省值写死或自动播种（裁决 6——能力非策略；which 回落 /bin 已在）；berry 内嵌造盘/缺席自动造盘/缺席静默（裁决 6——只复用不造律＋缺盘报错指路）；纯文档停机写纪律（裁决 7——fsmgr RAM 态覆写面须硬护栏）；盘锁全语义/体系外 QEMU 执法（裁决 7——残余文档化）；cli 步吸收 CLI ok n=13→14/盘族标记吸收 PATH 面/任何新独立标记（裁决 9——无盘不变量＋零新标记）；e2e 脚本化 rig/入 CI（裁决 8——列待裁项 3，默认手跑存证）；cli.rs/cli_flags.rs 加行（裁决 10——hermes 活跃 lane）；commands.rs 增载/分模块 fat12.rs·fatvol.rs（裁决 16——巨文件零增触）；crashlog.rs 任何改动含 pub(crate) 微调（裁决 13——read_root_file 已 pub 足用）；客机任何改 bsh/which/fsmgr/settings/svcmgr/berry-term/berry-libc/berry-exec/kernel/ABI（G1）；SETTINGS.CFG/SETTINGS.JSN/LAYOUT.JSN/CBOX.LOG 任何写触碰（G2/W 契约——盘保真断言执法）；七面任何滚新含注释（G3）；bench 三脏件与 icount lane 件（G5）；实施轮 git 提交/沿革落笔（G5——主线统一显式路径）；SKILL.md 基线旧值保留（裁决 11——现行态文档同批滚）；旧口径 137/160/169/339 追认（G6）；ENV 帽扩容 VMO v3/svcmgr·launcher·berry-term env 转发链 27 点（挂账 1/2）；刀 11 v2 配置体系顺手做（set 内建/.bshrc 名/控制流——挂账 3）；FAT 文件删除面 0xE5（挂账 4——rm 留空文件形）；多簇 BSHRC 写面（挂账 6）；孤儿簇回收/journaling（裁决 4——rename 原子下宿主侧无此面，客机侧维持 fsmgr v1 文档边界）；/apps 目录造册与 mkdir 演示面（挂账 8——判据已解耦）；settings 运行时验证顺手收（挂账 9——归属 settings 面笔记互链）；根目录扩帽 mkfat ROOT_ENTRIES 16→N（挂账 10——几何与 fixture 双端重产归 mkfat 面）；LAYOUT.JSN 通用写者命令面（挂账 11——写面泛型天然支持，命令面只开 BSHRC）；doctor 深报告（C5 薄面只增 bshrc_present 推演位）；crashlog/bshrc 读面公共层抽取（挂账 12——整洁刀另议）；FAT16 写面/子目录多段写（W2——FAT12 域内单段根名平面）；裸 cargo test 不带 --target host/不隔离 CARGO_TARGET_DIR（README :20-23 EPERM 坑）；旧基（rebase 前）直接动手（G5）。
砍刀重议触发（实发即停刀重议）：① 字节恒等对拍红且归因方言失配（berry-fatfs encode 与 mkfat dir_entry 字节分歧或分配序不重合）→停刀对齐禁带红推进；② 盘保真断言红（写前后非 BSHRC 域字节漂移）→停刀（G2 持久化契约事故形）；③ e2e 任一轮判据失合（miss 文本不含 (/apps)/或 /etc/mtab 未命中/或 echo $PATH 非 BSHRC 值/或 $BSHRC_E2E 空）→停刀归因——rc_load 静默面掩盖失败，宿主回读三层（W5 自回验/单测 oracle/集成）先红定位；④ berry cargo 既有测任一红且归因本刀（berry-fatfs 依赖面/枚举面/ENV_LOCK 串行面）→停刀归因；⑤ hermes lane 在提交轮前触碰本刀域文件（main.rs/lib.rs/Cargo.toml 撞面）→暂停提交轮 rebase 重划；⑥ 主线重同步后 bsh/fsmgr 域锚点漂移（rc_load 帽/装载点/commit 序/mtab 预建变形）→按新基复核设计锚点再动手；⑦ berry-fatfs version.workspace 接线解析失败→停刀上报主线重裁（禁改 user/ 侧）；⑧ bench 脏件被任何步骤触碰→即停。

## 待裁项（owner）

1. doctor bshrc_present 薄面去留（本稿默认做——约 30 行含测试；砍则 C5 与 doctor.rs 触面全删零牵连）。
2. rm/get 缺席语义与 list 缺席形：本稿裁 get/rm 缺席 rc≠0＋诊断（which miss 退出 1 对偶）、list 缺席空输出 rc 0（rc_load 缺席常态镜像）——翻案改幂等 rc 0 随 C1/C4 同批。
3. e2e 载具终形：手跑存证（本稿——2026-11-15 :111 客机写两轮先例的宿主写镜像形）vs 脚本化 rig（loop 稿形态——新 python 驱动机件零标记、判据同 E2-E4、可复跑）。
4. 在用护栏强度：本稿裁 run 注册表扫描拒写（risk 稿）——翻案降为纯 README 停机写纪律（writer/loop 稿）随 C3 同批。
5. mkfat dev-dep 字节恒等对拍去留（本稿默认留——写方言≡造盘方言的最强机查；砍则 berry-fatfs 同源＋crashlog 回读＋e2e 真机三层护栏兜底）。
6. e2e 负控相去留（E4——默认做，一分钟成本钉 which 回退 /bin 遮蔽判据；砍不碍 E2/E3 主判据）。
7. 沿革取号（流程项非设计项——实时重数顺延）。