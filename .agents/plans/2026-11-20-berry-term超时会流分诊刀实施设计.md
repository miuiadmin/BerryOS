# berry-term 超时会流分诊刀实施设计（Major-3 清偿：键路满队异步背压＋真死双信号分诊＋terminate 先行收口＋respawn 收口门）

Type: plan
Status: draft
Updated: 2026-11-20
落库家：.agents/plans/2026-11-20-berry-term超时会流分诊刀实施设计.md（主线评审后翻 active 交实施轮）

## 一句话说明

owner 令（2026-11-20「彻底实施…review…合入」续）清偿跨刀 review wf_bb3bfdb2 Major-3 移交项——berry-term 键路把「子忙不读 stdin 的满队 10s 帽超时」与「链断」折叠成同一 false 喂进 mark_child_dead（GUI 终端跑 bsh 内建 sleep 期间敲满 9 键即误杀健康子，后果链＝帧循环同步冻结 10s＋30s＋活孤儿占 PROCESS_MAX=64 槽＋respawn 双活 bsh 共享同一 fs 通道〔第二消费者互截禁例形〕）；本刀四面根除：①键路改异步保序背压——child_send（main.rs:696-715）拆非阻塞三态探测（Sent/Full/LinkErr），Full 转本地有界 FIFO（KEYPEND_CAP=256B、溢出丢新键加计数与提示行）、帧循环顶无条件排空步＋稳态 wait_many 增 in_s WRITABLE 条件背压臂（子腾位即醒续发——帧循环键路零同步阻塞零超时，EBUSY 从死判面彻底除名）；②真死双信号分诊——L1 stdout 零长 EOF（配合性退出权威）＞L2 hp SIGNAL_TERMINATED（非配合死亡唯一天窗——稳态第 5 条件臂子活期恒挂＋deadline=0 poll 备用，EOF 缺口面全时覆盖）＞L3 stdout 非 EAGAIN 错与键路非 BUSY 错（死询打标后一律收口——v1 近不可达面终止重启是唯一有界恢复）；③收口漏斗——mark_child_dead（:1179-1213）重构：先排空 out_s 残留（保退出末段输出）→ exited 行（文案零滚）→ pending 键陪葬计数行 → 关双端 → 无条件 berry_process_terminate(hp, 130)（pipe_selftest :571-578 同码先例；已退 -37 幂等）→ reap（TERMINATED 前置保证下瞬回，30s 帽降为防御兜底）→ reap Err 降级熔断停自动 respawn → T5 记账零改；④respawn 收口门——自动路与熔断 Enter 手动路前置 child.proc.is_none() 结构断言——双活 bsh 共享 fs 通道形结构性不可达；哑终端契约（shell设计-v1.2 §2.4 透传句零改）与 TermCore 解释器零改（berry-termio 只增 triage 纯面模块＋host 测试族，:6-9/:50-51 边界条款同批滚）；kernel/berry-exec/bsh/berry-libc/berry-sys 全零 diff（SIGNAL_TERMINATED 走既有导出仅加 use）；标记七面零滚新（TERM ok n=2 步数单源在 bsh 不动）；COLLECT_TIMEOUT_NS 键路死判用途退役归义自检活动帽＋input_arm :1252 字面量常量化（零行为漂）；文档批＝本谱＋新实施笔记＋清偿批 :18 翻态尾注＋11-17 谱 T5 勾账＋shell设计监督语义句与降解面＋沿革六百四十五。

## 背景

（锚点 2026-11-20 深夜逐条实证于 origin/main=bc092714a——三域底册按 e26ee4fee 出具、本侦察对 bc092714a 复核全部锚点行号零漂：child_send :696-715（帽 :707、注释 :711）、键路调用点 :937-938、mark_child_dead :1179-1213、稳态等待 :1157-1167、input_arm :1238-1261（字面量 :1252）、pipe_selftest terminate :571-578、fs_gifts :401-420、Child 结构 :590-599、CHAN_DEPTH=8（chanface :74）、berry-exec reap 帽 30s（lib.rs:150-152/:233-241）、kernel terminate 幂等面（syscall.rs:2331 起）、bsh sleep :4619-4648、libc io.rs :778-790、WAIT_MANY_MAX=16（syscall.rs:4347）皆 git show 逐条复核；本地 wip/sched-1.1@97a0edeb 落后 origin/main，实施轮以 rebase 后 origin 树为基；git 现脏 10 M 全属 NET P4 留树批（vnet×2/netdproto/tcp×2/bench×3/berryboot Cargo.lock）——pump.rs 已随 97a0edeb 落库不再脏——与本刀域（user/bins/berry-term/、user/berry-termio/、docs 件）porcelain 零交集全禁碰）

1. 令与移交：清偿批笔记（[2026-11-20-跨刀整体review清偿批](../notes/implemented/2026-11-20-跨刀整体review清偿批.md) 移交项 :18）Major-3 原文「berry-term『子忙 stdin 队列满 10s 超时』与『链断』合流（sleep 期间敲满 9 键误杀健康子）＋帧循环同步阻塞 30s＋respawn 双 bsh 共享 fs 通道禁例形——需超时/链断分诊＋reap terminate 先行改造，独立小刀量级」（wf_bb3bfdb2 review 全文未落库——一句话概括与代码实锚全部对榫）；owner 令三面：队列满不等于链断（sleep 期间敲 9 键不得误杀健康子）、真死分诊（权威死判信号）、误杀后果链根除（respawn 前旧子 terminate 加 reap——双消费者禁例结构性不可达）；硬约束＝TermCore 解释器行为零改、真死判据用现役原语零内核改、标记七面零滚新、git 脏件禁碰、实施轮文件域互斥禁提交隔离构建；队列满策略裁决自定（须与 bsh 子 stdin 读环语义对拍）＋分诊逻辑尽量纯面化（berry-termio host 测试面扩展先例）。
2. 病灶一（超时/链断合流点）：child_send（main.rs:696-715）＝loop { berry_channel_send；s==0 过；非 BUSY 错返 false（链断）；EBUSY → berry_object_wait_one(in_s, SIGNAL_WRITABLE, now+COLLECT_TIMEOUT_NS)（:707，值 10s :152「收集环活动帽——term_collect 同值：收包刷新、帽尽即收场」——原义仅自检收集环活动帽）超时返 false，:711 注释原文「满队超时（子不读——视同链断）」}——同一 bool 折叠两语义；键路唯一调用点 :937-938 直呼 mark_child_dead。CHAN_DEPTH=8（chanface :74「每端收件队列深度（有界性 = send -EBUSY 的来源）」）＋每键 1 字节 1 条消息（:923/:937）→ 子忙期第 1..8 键积压、第 9 键 EBUSY 进 10s 同步等待；等待对象是本端 in_s，对端 recv 腾位经 wake_matching(对端, WRITABLE) 唤醒、匹配键＝ObjRef 地址跨 handle 别名互通（berry-exec lib.rs:310-312 契约注）。第三处同数字复用＝input_arm :1252 硬编码 10_000_000_000 字面量（未引常量）。
3. 病灶二（reap 无 terminate 先行）：mark_child_dead（:1179-1213）三路共用——stdout 零长 EOF（:996-1000，libc send_stdout_eof 收口约定）／stdout 非 EAGAIN 错（:1005-1008）／键路 false（:937-938）；收口序＝exited 行（:1185）→ 关 out_s/in_s 置 0（:1188-1191）→ proc.take().reap()（:1194-1199）——reap＝reap_with_timeout(30s)（berry-exec :150-152/:227-229），第一步即 wait_one(hp, SIGNAL_TERMINATED, 30s 绝对帽) 无 terminate 先行（:217 注释自认假设「EOF 先行约定下常态瞬回」——误判路径子活、TERMINATED 永不置位 → 帧循环第二段同步冻结 ≤30s，sleep 60 场景合计 10+30=40s GUI 全冻）；30s 帽尽返 Err「不冒险回收」（:239-241）→ 读码/unmap/close 链（:242-295）全不走＋ChildHandles 无 Drop impl（仅 EagerGuard 有，:118）→ hp/hv/ht/svmo 句柄裸漏＋活孤儿占 PROCESS_MAX=64 槽（kernel/process.rs:39）；孤儿永活闭环＝v1 channel 两端互钉 retain 成环对象不释放（kernel/channel.rs:13-16）＋对端关闭检测明文不做（:18-19「send/recv 对死端表现如常」）→ 旧 bsh sleep 醒后回交互循环 stdin 空队 recv 10s READABLE 帽超时被 libc EOF 化返 0（berry-libc io.rs:778-790）→ bsh 当空读噪声静默续读（bsh :539-540）＝每 10s 醒一次的永活 wedged 孤儿。对照正确先例＝pipe_selftest 收口（:571-578 先 berry_process_terminate(hp,130) 再 reap Ok(130) 瞬回——boot 即测即过即 terminate→reap 快速性的实证）。
4. 病灶三（双 bsh 禁例形）：fs_gifts（:401-420）每次 spawn 对 fs_h() dup RIGHT_SEND|RIGHT_RECV|RIGHT_TRANSFER 授子——dup 是同一 ChannelEnd 对象别名不复制收件队列（单 FIFO）；头注 :405-406 自宣「子 bsh 是 fs 通道唯一活动客户端」在双 bsh 并存窗内破产；误杀后 2s 冷却自动 respawn（RESPAWN_COOLDOWN_NS :158-160；帧循环顶检查 :883-891）起新 bsh 又 dup 同一 fs 会话端 → 两活消费者 fs RPC 应答互截——与 wm 共享队列第二消费者禁例（MEMORY 判例）同形；误杀形死亡子已活 >2s → strikes 清零（:1201-1204）——每次误杀漏一个活孤儿且熔断永不设防（review 移交文案未列、本刀随根除自然收口并记档）。
5. 误杀窗口判别：sleep 系 bsh 内建（分发 :4036-4040；sleep_output :4619-4648 usleep 100ms 步进环 0..=60s、头注明文 v1 不可打断、期间单线程零读 stdin）——当前词汇表内唯一长时件；外部命令前台走 collect_fg 的 wait_many 三项 [out:READABLE, proc:TERMINATED, stdin:READABLE]（bsh :7162-7164）持续排空 stdin（非 0x03 键进 TYPEAHEAD）不触发——误杀面专属「同步阻塞不读 stdin 的执行段」。
6. 真死信号面（现役原语、零内核改）：①stdout 零长 EOF——libc exit/_exit/abort 在 berry_process_exit 前主动补发（io.rs:919-945）＝配合性真死权威（已知缺口＝内核 terminate 杀不走 libc exit 族无 EOF——bsh :7122-7127 明文「EOF 缺口面（v1 边界）」）；②hp SIGNAL_TERMINATED——非配合死亡唯一天窗：wait_one deadline==0 即非阻塞 poll（syscall.rs:4295-4296「poll 语义」——返 ERR_AGAIN 即未命中）；TERMINATED 唤醒在 mark_exited 落账之后（berry-exec :242-243 注）；terminate 语义（syscall.rs:2331 起）：活进程 kill_all_tasks_by_pinned 带码杀且末任务钩子内完成 Process 收口后返 0、已退返 -37 ERR_BAD_STATE 幂等（abi_gen.rs:187）；berry-sys 已导出 SIGNAL_TERMINATED（berry-exec :40 use 实证——berry-term 仅加 use）；③通道对端关闭信号 v1 根本不存在（channel.rs:18-19 明文裁剪）——键路非 BUSY 错近不可达、恒为满队超时。仓内先例：bsh collect_fg 三臂 wait_many＋pipe_selftest terminate→reap；稳态现 3 路（:1157-1167）＋WAIT_MANY_MAX=16——扩 5 路结构性零障碍。
7. 纯面与测试先例：berry-termio 系 no_std 纯 crate，头注 :6-9 职责边界「行环/尾段/feed 解释器/软换行/视图计算全纯面；ui.wake/child_send/wait_many 胶水留在 berry-term main.rs 不进本 crate」——范围声明非硬规（11-17 谱 T7 立法）；host 测试面＝lib.rs 17 单测＋tests/pipe_sim.rs 2 集成（客机流形态 host 仿真钉板先例）；监督逻辑现状零 host 测——本刀分诊决策入该 crate 须同批滚边界条款。
8. 契约与标记面：哑终端契约＝bsh 交互循环仅依赖 fd0 逐字节读＋fd1 字节流写、宿主与客机 berry-term 均纯字节透传（docs/shell设计-v1.2.md §2.4 :68）——本刀全部改动在发送/监督面、透传契约句零改；标记七面（markers.sh/smoke.sh/smoke-image.sh/bcts@0-@6/gui8-smoke.py）全前缀式「TERM ok n=」且步数单源在 bsh term_selftest——本刀不动自检步即零滚新（bcts@0:548 冻结值原样）；berry-term 全部诊断走 svcmgr 私有 drain 不上串口（:82-84）——新 push_line 文案零标记面（canon 机查兜底）；T5 现行语义（mark_child_dead「best-effort reap」）由 [2026-11-17 完善刀谱](2026-11-17-命令行终端与shell完善刀实施设计.md) :48 立法——本刀改的正是这条，须回谱勾账并滚 shell设计 降解面（契约底册明示「而非只改代码」）。
9. 沿革与定性：docs/阶段沿革.md 尾节＝六百四十四（:8180 实测，无 645+/七百占位）——本刀取六百四十五；伞形「五波十八刀」计划（2026-11-18，active）刀 1-18 无一覆盖 v1 通道世界监督超时面（刀 8 是未来 pty 世界全双工）——独立小刀定性成立，按最小修兑现避日落重叠投资。

## 契约

### 全局契约（三域共守）

G1 哑终端契约与 TermCore 零改：透传架构零动（shell设计 :68 契约句零 diff 自证记档）；键字节仍一视同仁透传（0x03/0x08/0x0A/0x09/0x1B/可打印零本地语义、空行 Enter 照发 \n）；显示单源仍是子回显字节、本地不生成键回显；TermCore 解释器函数族（feed/行环/尾段/软换行/视图）零字符改；berry-termio 既有 19 host 测（17 单测＋pipe_sim 2 集成）零改零漂移自执法。
G2 真死判据现役原语零内核改：分诊只用 stdout 零长 EOF、wait TERMINATED（deadline=0 poll 或等待臂）、berry_process_terminate 现有导出（号 130——POSIX 128+SIGINT 惯例、pipe_selftest 同码；已退 -37 幂等容忍）；kernel/、user/berry-exec/、user/bins/bsh/、user/berry-libc/、user/berry-sys/ 全零 diff（SIGNAL_TERMINATED 走既有导出仅加 use——验收断言）；v1 channel 对端关闭检测不立（挂账）。
G3 标记七面零滚新＋boot 行零漂移：markers.sh/smoke.sh/smoke-image.sh/bcts@0-@6/gui8-smoke.py 全零触；pipe_selftest 五断言与「berry-term: pipe ok」「berry-term: first frame」文案零漂移；「berry-term: bsh exited」「berry-term: respawn」「berry-term: respawn circuit broken (enter to restart)」既有文案零滚；新 push_line 文案（忙段提示/dropped N keys）走行环不上串口（:82-84 先例）——实施毕 canon 机查与 91 件标记两两互非子串记档。
G4 脏件禁碰与并行纪律＋基线硬门：git 脏 10 M（NET P4 留树批）全禁碰；实施轮文件域互斥（域＝user/bins/berry-term/**＋user/berry-termio/**＋docs 件）；禁 git 提交（主线统一显式路径暂存，禁 add -A）；构建/测试隔离 CARGO_TARGET_DIR（~/.cargo-target-<名> 惯例）；开工前 git status 复核＋先并 origin/main（本地 97a0edeb 落后 bc092714a——旧基直接施工重演活线丢失病）＋域文件 blob 对照（漂则改谱再动手）；docs/阶段沿革.md 实施轮全周期禁碰（主线 HEAD 重同步后追加，取号六百四十五临笔复核撞号顺延）。
G5 分诊不变式（本刀核心承诺）：EBUSY（满队）永不构成 mark_child_dead 依据——键路与排空路不得出现任何带非零 deadline 的 wait_one（input_arm 既有等待不连坐、豁免清单写明）；mark_child_dead 完成即旧子死证在先（terminate 返 0 已杀／-37 已退皆死证；reap Ok 更强）——respawn（2s 冷却自动／熔断 Enter 手动）全部后置于该同步收口且前置 child.proc.is_none() 结构断言，双活 bsh 共享 fs 通道形结构性不可达；reap Err → broken 置位停自动 respawn（Enter 手动路保留——接受有界泄漏不冒第二消费者险）。
G6 键序与 bsh stdin 读环对拍承诺（owner 令硬校验）：键字节全程 FIFO（通道＋本地缓冲跨段保序——pending 非空时新键不探测直接入缓冲、排空步为满队期唯一发送者，结构性无竞窗）；缓发不经 bsh 读路径反杀——队列满 ⇔ 子不在 read() 等待（在队消息即 READABLE 源），libc read 的 10s 帽超时 EOF 化仅在空队时参与，子醒回读先收割在队键即命中不落超时支 → 背压键路与假 EOF 面结构性不相交；即使伪 0 出现，bsh 交互循环把空读当噪声静默续读（bsh :539-540）、collect_fg 前台路径本就监 stdin——双层保险；TYPEAHEAD「不丢键」契约管 bsh 已收字节，term 侧溢出丢键不入约（v1 降解面记档）；显式损失面仅两处且计数可见：溢出丢新键（dropped 行）＋子死弃 pending（dropped 行）。

### 契约 P：berry-termio triage 分诊纯面域（src/triage.rs 新＋tests/triage_sim.rs 新＋src/lib.rs）

P1 模块定位：pub mod triage——分诊决策与键路仲裁纯面（no_std 延续、定长零 alloc、零 syscall 依赖）；模块头注立法死信号层级表：L1 stdout 零长 EOF（配合性真死权威——libc exit 族收口约定；terminate 杀缺口记面）＞L2 hp SIGNAL_TERMINATED（非配合死唯一权威窗——deadline=0 poll／等待臂）＞L3 stdout 非 EAGAIN 错与键路非 BUSY 错（通道坏——近不可达）；EBUSY 非死信号（本刀除名）；通道对端关闭信号 v1 不存在（kernel/channel.rs:18-19 裁剪）不作判据；IO 原语调用（send/recv/wait/terminate/reap）留 main.rs 薄壳。
P2 镜像枚举与死判收敛：pub enum SendFeed { Sent, Full, LinkErr }（berry_channel_send 单发非阻塞三态镜像——满队与非 BUSY 错分立，编译期除合流）；pub enum Liveness { Terminated, Alive }（死询结果镜像）；pub enum DeathEntry { StdoutEof, StdoutErr, SendLinkErr(Liveness) }＋收敛规则——四入口全收敛 TerminateThenReap 单路（层级与例外全文进 doc 注，host 表测执法）；Liveness 标仅影响诊断面文本不影响收敛。
P3 Backlog 状态机：pub struct Backlog——定长环形缓冲（pub const KEYPEND_CAP: usize = 256）＋dropped 计数＋忙段追踪（busy_since/hinted 一次性，pub const CHILD_BUSY_HINT_NS: u64 = 5s）；API 语义：on_key(feed, b, now) -> KeyAct { Delivered | Buffered | DroppedNew }（pending 空＋Sent → Delivered；pending 非空一律入缓冲不探测——保序不变式结构性成立；缓冲满 → DroppedNew 丢新键计数不丢已收键；LinkErr 不进 Backlog 由 main 先分诊）；排空面 next_pending/on_sent 逐个出队＋忙段排空毕回报 dropped 总数；hint_due(now) -> Option<usize>（阈值到、每忙段恰一次、附 pending 数，排空复位）＋hint_at() -> Option<u64>（deadline min 用）；on_child_dead() -> usize（pending 数回报＋全态清零——respawn 整体替换自动清零语义）。
P4 host 测试族（lib.rs mod tests 17 件邻位追加＋新集成 tests/triage_sim.rs——pipe_sim 先例）：①九键回归（8×Sent＋1×Full → 8 Delivered＋1 Buffered、零 Dropped、busy_since 置位）；②溢出（CAP+5×Full → 5 DroppedNew、FIFO 序断言）＋保序不变量负证（pending 非空时 Sent 不旁路）；③忙段提示恰一次＋排空复位后再忙段再提示；④排空全链（episode 回报 dropped>0）；⑤on_child_dead 回报与清零；⑥DeathEntry 表测（四入口全 TerminateThenReap）；⑦triage_sim 事件流集成（8 键 Sent→12 键 Full→Writable→drain→续键——零死判、键全达或 dropped 对账、提示恰一次）。既有 19 测零扰动。
P5 边界条款同批滚：termio lib.rs:6-9 职责边界句补「子监督分诊决策面（triage——只产决策不触 IO）」＋「通道/进程 IO 胶水（send/recv/wait/terminate/reap 调用）留 berry-term main.rs」；两 crate 头注互指。

### 契约 M：berry-term 监督面域（user/bins/berry-term/src/main.rs）

M1 child_send → child_probe：fn child_probe(h: Handle, bytes: &[u8]) -> SendFeed——单次 berry_channel_send 非阻塞（0→Sent；BUSY→Full；其余→LinkErr），零等待零重臂；:694-695 doc 注与 :920 键路注释（「遇 EBUSY 等可写再发——child_send 契约」）同批滚为异步背压注；COLLECT_TIMEOUT_NS 键路死判用途退役（:707/:711 消失）——常量归义 term_collect 自检活动帽专属（:151-152 注释滚）；child_send 唯一键路调用点 :937 实证（pipe_selftest 零依赖——自检链字节契约零漂）。
M2 键路与排空接线：Backlog 入 Child 结构（增 backlog 字段，try_spawn_child 构造全零——respawn 整体替换自动清零、新会话不继承旧键）；帧循环顶（respawn 检查 :883-891 后、INPUT_POLL 收割前）增排空步——!child.dead && pending 非空：连发 next_pending 至 Full/空/LinkErr（LinkErr 走 M4 死分诊后 break）；键路 :937-938 改：child_probe → LinkErr 先死分诊，Sent/Full 交 backlog.on_key；排空正确性不依赖 WRITABLE 唤醒粒度（臂仅优化延迟）。
M3 稳态等待 3→5 路＋deadline 滚：hs/ss 数组 3→5、逐臂装填——hw READABLE＋（hi READABLE 条件）＋（out_s READABLE 条件）＋（in_s WRITABLE 条件＝pending 非空且 in_s≠0——背压臂）＋（hp TERMINATED 条件＝!child.dead 且 proc 在席——死讯臂恒挂，bsh collect_fg 三臂先例同构）；:77-80 头注「稳态 wait_many 至多 3 路」滚「至多 5 路（条件背压臂＋条件死讯臂）」；deadline min 增忙段提示点（hint_at 未出行时并入——不赌 IDLE 帽）；wait_many 命中死讯臂（idx 判别）→ M4 收口漏斗；幂等门防重入。
M4 mark_child_dead 收口漏斗重构（:1174-1213）：幂等门后序＝①若 out_s≠0：先排空残留队列（recv 至 EAGAIN、字节喂 term.feed——保退出末段输出，EOF 消息自然抵达；有界＝队列深 8）②dead 置位＋push「berry-term: bsh exited」（文案零滚）③backlog.on_child_dead 回报 N>0 追行「berry-term: dropped N keys」④关 out_s/in_s 置 0 ⑤proc.take() 后无条件 berry_process_terminate(hp, 130)（返码仅 eprintln 诊断——0 已杀/-37 已退皆死证；EOF 路子配合退出中 terminate 幂等安全）⑥reap()（terminate 后 TERMINATED 已落/将落瞬回；:1192-1193「best-effort reap」表述滚「terminate 先行收口」）——Ok 走 ⑦、Err → eprintln＋broken 置位＋pending=None＋push「berry-term: respawn circuit broken (enter to restart)」（既有文案复用）停自动 respawn ⑦T5 记账（strikes/pending 逻辑零改）；「child reap failed (bounded leak)」eprintln 保留。
M5 respawn 收口门：帧循环自动路（:883-891）与熔断 Enter 手动路（:928-936）前置 child.proc.is_none() 条件（同步漏斗下恒真的结构断言——双活 bsh 禁例立法点）；try_respawn 成功 → Child 整体替换（backlog/sup 随新会话复位）。
M6 忙段提示接线：帧循环内 hint_due 到点 → push_line「berry-term: child busy, N keys buffered」（每忙段恰一次；格式化走 main.rs——纯 crate 不触 fmt 依赖面）。
M7 头注与常量滚：:48-51 边界条款（分诊决策与状态机在 berry-termio triage；通道 IO 原语调用留本文件）；:52-67 监督段补 terminate 先行与异步背压句；:86-92 v1 降解面段补「满队溢出丢新键＋子死弃 pending 键（两处计数可见）」记面；use 增 SIGNAL_TERMINATED；input_arm :1252 字面量命名 INPUT_ARM_CAP_NS（常量区邻位新立、值同零行为漂、doc 注与自检活动帽显式分立）；pipe_selftest/term_collect/term_ready 全零字符改。

### 契约 D：文档批（终值定形后；主线职责）

D1 本谱落库翻 active＋新笔记 .agents/notes/implemented/2026-11-20-berry-term超时会流分诊刀.md（三病灶机理＋裁决全文＋死信号层级表＋bsh 读环对拍论证＋EOF/TERMINATED 竞窗与 terminate 幂等安全论证＋后果链根除证明＋host 测与 e2e 存证账＋shell设计 :68 零改自证记档）。
D2 翻态两链：清偿批笔记 :16-19 移交段 Major-3 行加兑现尾注＋指向新刀笔记相对链接（翻态＝取代动作——笔记者 Status: implemented 不动）；2026-11-17 谱 :48 T5 行「best-effort reap」句尾加兑现尾注（T5 其余立法零改）。
D3 docs/shell设计-v1.2.md §2.4（:67-68 邻位）补监督语义句（键路满队＝本地保序缓存帽 256、满队不判死；死判权威信号；respawn 前置收口门）与丢键降解面——:68 透传契约句零改；版本行按该文档自身修订律临笔滚（默认 PATCH——待裁项 4）。
D4 docs/阶段沿革.md：主线 HEAD 重同步后追加六百四十五节（取号临笔复核撞号顺延）；node .agents/tools/check-docs.mjs 0 告警为 D 批门；相对链接目标全部存在。

## 步骤

0. [ ] P0 前置（主线职责）：本谱落库翻 active；并 origin/main（bc092714a 或更新）重钉基线＋域文件 blob 对照（漂则改谱再动手）；porcelain 复核脏件零交集；锚点复核表（child_send/mark_child_dead/三入口/等待面/respawn 门/input_arm/清偿批 :18——本谱锚点即复核清单）。
1. [ ] D1 berry-termio triage 纯面域（dependsOn 0）：P1-P5；cargo test -p berry-termio 绿（隔离 CARGO_TARGET_DIR）——既有 19 测零扰动自证＋新族全绿。
2. [ ] D2 berry-term 监督面域（dependsOn 1）：M1-M7；构建绿（隔离 target）；rg 自查＝child_probe/排空步/Backlog 路径零 berry_object_wait_one 调用（input_arm 与 RPC 面既有等待豁免清单写明）。
3. [ ] D3 验收域（dependsOn 2）：host 全测重跑；smoke boot 标记 32/32 零滚新比对（TERM ok n=2 前缀值原样）＋pipe ok 零漂；gui8-smoke 常规 PASS；QEMU 手跑 e2e 双场景存证（TCG 标注、断言用状态裁定不用墙钟）——场景甲 sleep 30＋连敲 ≥9 键 → 无「bsh exited」行、忙段提示行恰一次、帧仍渲（排空后键回显到席）、无冻结、单 bsh 在席；场景乙 exit 命令 → exited 行＋respawn 行＋进程表单 bsh 无孤儿；对照例 echo 短忙段零提示行（阈值下）。
4. [ ] D4 文档批（dependsOn 3）：契约 D1-D4；check-docs 0 告警。
5. [ ] D5 主线统一验证提交（dependsOn 全部）：分层验证链全绿；git diff 自证（kernel/berry-exec/bsh/berry-libc/berry-sys 零 diff、标记七面零触、termio 解释器零 diff、脏件零碰）；canon 机查新文案与标记互非子串；沿革实时取号六百四十五；显式路径提交。

## 挂账移交（本刀显式不做）

1. v1 channel 对端关闭检测/DEAD 信号（v3 域——channel.rs:18-19 诚实缺口维持；键路非 BUSY 错近不可达的根因；死讯臂已盖非配合死可观测面，通道级检测随 v3）。
2. berry-exec ChildHandles Drop impl/reap 改造（域外消费者 svcmgr/bsh 连坐面；本刀以 terminate 先行＋收口门在调用侧根除孤儿产源）。
3. bsh 内建 sleep collect_fg 化前台等待（上游消「子忙不读」形态的 bsh 侧解——本刀 term 侧兜住即够，bsh 零改硬约束）。
4. 已进通道 ≤8 键的子死陪葬可观测化（通道队列 berry-term 侧不可读——dropped 行只记本地 pending 数，诚实边界记档）。
5. gui8 --aged-minutes 式「子忙不读」注入回归用例（动 gui8/smoke 面须另刀授权；本刀回归面＝host 纯面测＋手跑 e2e 存证——薄面记档）。
6. fs_gifts 通道共享的拓扑级防御（fsmgr 多会话化另刀——本刀以「无双活 bsh」结构性根除替代）。
7. 冻结窗内 wm/inputd 服务端会话容忍面（帧循环零长停摆后该窗结构性消失——服务端超时面另刀勘察）。
8. input_arm EBUSY 内联 10s 阻塞形为面（:1242-1258——本刀仅常量命名零行为改；如再现冻结归输入面刀）。
9. pty 世界全双工面与常驻 hp 臂的进一步消费（十八刀伞形计划刀 8——本刀 v1 通道世界最小修，不预投资；triage 纯面决策机可迁移）。
10. svcmgr→term env 继承挂账 2 与输入焦点门控（独立刀不混批）。

## 裁决记录

三稿冲突逐条裁决（sem 语义稿／mech 机制稿／risk 深挖稿；其余三稿共识项直收）：
1. 收口形态＝同步 terminate 先行＋reap 瞬回（sem/mech 共识为骨）——否决 risk 全异步形（证据帧只 terminate 不等、hp 臂命中帧才 reap、35s 看门狗 Stuck 相位机）：terminate 对活子同步杀且末任务钩子内完成 Process 收口（syscall.rs:2349-2360 kill_all_tasks_by_pinned 路径实测）→ TERMINATED 落账先于返 0，reap wait 瞬回——pipe_selftest :571-578 terminate(130)+reap Ok(130) boot 即测即过即实证；全异步防的是「terminate 返 0 而 TERMINATED 不落账」的内核 bug 面，该面以砍刀重议触发④替代常驻相位机（Collecting/CollectedOk/Stuck 三相＋看门狗＝状态机超量级，违独立小刀定性）；「帧循环零同步阻塞绝对律」以「键路/排空路零带帽等待＋收口路径 terminate 前置保证快速性」实质兑现。
2. hp TERMINATED 死讯臂＝稳态子活期恒挂（risk 独有，嫁接采纳）——否决 sem 的 LinkErr/忙段提示点零星死询与 mech 的不挂：非配合死讯（内核 terminate 杀/G1.5 击杀——no_std panic 静默击杀判例同面）唯一权威窗，臂恒挂使 EOF 缺口面从「键忙期可探测半边」升全时覆盖且零每帧 syscall；成本＝稳态等待 3→5 路两项（WAIT_MANY_MAX=16 容量实证）＋每帧装配分支；sem 的忙段提示点搭车死询随之退役（臂全时盖）；e2e 实测帧率受扰则回退窄挂档（待裁项 3）。
3. 队列满策略＝异步保序缓存背压（三稿共识）——否决纯丢键案（首选项用户损失、sleep 常态场景不必丢）、无限阻塞案（libc console_write io.rs:549-555 公平排队形内联帧循环＝本缺陷放大版）、带帽重试案（帽尽仍留同一分诊题）；对拍结论（G6）：队列满 ⇔ 子不在 read() 等待——libc read 10s 帽 EOF 化面结构性不相交，背压键路不产假 EOF。
4. 缓冲帽 256B＋溢出丢新键计数（sem/mech 共识）——否决 risk 64：256 锚 bsh LINE_CAP 256 同量级（一条满行命令的缓冲纵深）；丢新不丢旧保 FIFO（丢旧保新＝bsh 收乱序半行）；无界缓冲否决（长睡连敲内存面）。
5. 键路/stdout 非 BUSY 错分诊＝死询打标后一律 terminate 收口（sem/risk 共识）——否决 mech「未确证 → 面退役不杀不记账」：v1 通道无对端关闭检测使该错近不可达，一旦出现＝监督契约破产且无恢复路（该面上 EOF 永不可达、子不可观测）；终止重启是唯一有界恢复；「不杀健康子」令的边界是「不因满队/超时杀」，链破产面不在此列；Liveness 标仅进 eprintln 诊断（DeathEntry::SendLinkErr(Liveness) 表测执法）。
6. respawn 门＝proc.is_none() 前置断言＋reap Err 降级熔断（mech 形）——否决 sem 无新门（「terminate 返码即死证」依赖内核落账序假设——该假设以砍刀重议触发④执法更稳，断言零本钱）与 risk CollectedOk 相位门（随裁决 1 同裁不取）；同步漏斗下断言恒真＝文档化不变量；reap Err 面宁可停自动 respawn（Enter 可恢复）不冒第二消费者险；双活 bsh 禁例不依赖 reap 成功——死证在 terminate 返码（0/-37），reap Err 仅句柄有界泄漏。
7. 忙段提示 5s 一次性（sem 档）——否决 risk 10s 与 mech 无提示：哑终端无本地回显，忙期屏静默需解释（用户可见性）；5s 使外部命令 1-2s 短忙段零噪声、sleep 级忙段有反馈；纯常量面实跑可调零结构牵连。
8. input_arm 字面量本刀常量化 INPUT_ARM_CAP_NS（mech/risk）——否决 sem 挂账另刀：一行命名零行为漂、同文件独占域、且是 COLLECT_TIMEOUT_NS 三处复用漂移故事的收口半边；sem 的「输入面另刀」顾虑指向 GUI lane 活跃件（pump.rs——已随 97a0edeb 落库），input_arm 在本刀域内零冲突。
9. shell设计-v1.2.md 补监督语义句＋丢键降解面（mech/risk＋契约底册明示）——否决 sem 零触：契约底册原文「须回该刀谱勾账/加注并滚 docs/shell设计-v1.2.md berry-term 降解面，而非只改代码」；:68 透传契约句零改（G1 自证记档于笔记）；版本档位按该文档自身修订律临笔定（默认 PATCH——待裁项 4）。
10. 纯面落点＝berry-termio 新 triage 模块最小集（sem 形为骨）——否决 mech 全事件/指令枚举状态机（同步形裁决后状态面收缩，全枚举超量级）与 risk KeyPump＋Supervisor 双相位机件：SendFeed/Liveness/DeathEntry 三枚举＋Backlog 状态机＋收敛表即盖 host 测试面；新 crate 否决（workspace 滚面超量级）、留 main.rs 否决（监督逻辑零 host 回归网现状延续——「尽量纯面化」owner 令落空）；边界条款 :6-9/:50-51 两处同批滚（T7 范围声明非硬规——IO 胶水界定照旧）。
11. 滞留键随旧子陪葬＋「dropped N keys」行（sem/risk）——否决 mech 静默与重投新子案（半行命令重投＝新会话误执行面）；N 只记本地 pending（通道内 ≤8 键不可数——诚实边界记档）；Backlog 入 Child 结构使 respawn 整体替换自动清零。
12. COLLECT_TIMEOUT_NS 拆而不增（sem 裁决吸收）：键路死判引用退役后名义收窄回自检收集环活动帽（注释滚新）；input_arm 收编命名；三处复用语义漂移一次性收口零重构——死判帽形态（「帽尽即杀」任何保留形）整体入砍单。
13. 排空步不依赖唤醒语义（mech 论点采纳）：帧循环顶每轮无条件试排空——WRITABLE 臂仅优化响应延迟，正确性不依赖内核唤醒粒度（每 recv 一唤 vs 状态迁移一唤不明面）。
14. 死亡检测权威检查点＝wait_many 死讯臂命中（idx 判别）＋EOF 收割路＋LinkErr 死询三处——不做每帧周期性 hp 常询（sem 裁决 9 维持：臂命中即查零常驻 syscall）；臂安装前 TERMINATED 已置的竞窗由 wait_many 立即返回覆盖。

## 风险

最大单点＝帧循环手术序错误（排空步/双新臂装填/死讯臂接线/漏斗重构的次序与条件错位——host 不可测面）：护栏＝M2/M3/M4 逐条代码序钉死＋triage_sim 先行序（步骤 1→2）＋e2e 双场景存证＋rg 自查零带帽等待。
保序不变式破窗（新键 Sent 绕过 pending 乱序——排空与键收割间隙子腾位竞窗）：护栏＝结构性规则「pending 非空时新键不探测直接入缓冲」（P3 立法）＋triage_sim FIFO 断言——不变式不靠时序假设。
terminate→TERMINATED 落账序假设（terminate 返 0 后 reap wait 是否恒瞬回）：护栏＝pipe_selftest :571-578 先例（boot 即测）＋砍刀重议触发④——e2e 实测 reap 竟 Err 即停刀升 berry-exec 面重议。
hp 臂恒挂性能面：稳态等待 3→5 路（内核栈上定长数组 n∈[1,16] 零成本面）＋每帧装配分支一项——预期可忽略；e2e 实测帧率变化则回退窄挂档（子活期仅 pending 非空/busy 段挂）记档。
EOF→TERMINATED 落账竞窗（EOF 收口时子尚在 process_exit 中）：漏斗 terminate 支幂等（mid-exit 子返 0 或 -37 均安全）→ reap wait 短暂即回；论证入笔记。
terminate 返码异常面（非 0 非 -37——理论不可达）：reap 仍走（30s 帽防御兜底）＋Err 即 broken 停自动 respawn——极端面以熔断降级替代双活险。
新文案撞标记面：G3 canon 机查（行环文案不上串口结构性成立——:82-84 私有 drain 先例）。
WRITABLE 唤醒粒度不明：排空步每轮无条件试——正确性不依赖（裁决 13）。
旧基施工（本地 97a0edeb 落后 origin/main）：G4 P0 rebase 硬门（活线丢失病判例）。
e2e 环境竞速（TCG 载荷 deadline 竞速判例）：断言用行为裁定（行在场/帧活性/键序）不用墙钟；smoke 全链不因本刀重跑（标记零滚新自证——gui8 常规回归除外）。
沿革撞号（lane 密集）：主线实时重数顺延（流程项）。
体积账：termio 约增 250 行（状态机＋测试）、berry-term 净约 ±120 行（child_send 帽环删/排空步＋漏斗＋双臂增）、docs 件——零镜像/manifest/依赖改动。

## 砍单汇总（killList）

击杀形态：child_send 满队 10s 帽超时→false 的键路死判形态（:696-715 等待臂＋:711「视同链断」注释＋:937-939 调用点——队列满≠链断合流整段废除）；mark_child_dead 内联直呼 reap 无 terminate 先行（:1194-1198——30s 同步冻结＋Err 后 ChildHandles 裸值泄漏＋活孤儿占 PROCESS_MAX 槽）；respawn 门不问收口（:883-891 died_at+2s 即重生＋死亡即记账 :1201-1212——双活 bsh 共享 fs_h() dup 单 FIFO 互截禁例形）；COLLECT_TIMEOUT_NS 三处复用中的两处漂移（键路死判帽 :707 整体退役、input_arm 硬编码字面量 :1252 收编命名）；键路 EBUSY 内联同步等待（帧循环冻结第一段）与误杀形 strikes 清零记账（存活>2s 误杀既漏活孤儿又清熔断账——随键路死判退役自然收口）；真死非 EOF 面零覆盖（hp SIGNAL_TERMINATED 原语现成未用——死讯臂＋死询接入）。
落选封存（复苏需翻本刀谱）：满队纯丢键案、无限阻塞等 drain 案（console_write io.rs:549-555 公平排队形内联帧循环＝缺陷放大版）、纯提示案、带帽重试＋帽尽分诊案、「帽尽即杀」任何保留形（EBUSY 从死判面除名——G5）；全异步收口相位机（Collecting/CollectedOk/Stuck＋35s 看门狗——裁决 1）；周期性每帧 hp 常询死探（裁决 14）；mech「LinkErr 未确证面退役不杀」形（裁决 5）；忙段提示点搭车死询（死讯臂全时盖——裁决 2）；缓冲帽 64 档（裁决 4）；滞留键重投新子（裁决 11）；新 crate 承载分诊与 mech 全事件/指令枚举状态机（裁决 10）；mark_child_dead 条件 terminate（先 poll 活才杀——无条件幂等收口已盖）；respawn 新门「仅 reap 成功才重生」（相位门随裁决 1 不取——proc.is_none() 断言＋Err 熔断已盖）。
纪律禁碰：TermCore 解释器任何改动与 berry-termio 既有 19 测任何扰动（G1）；kernel/berry-exec/bsh/berry-libc/berry-sys 任何 diff（G2 验收断言）；标记七面（markers.sh/smoke.sh/smoke-image.sh/bcts@0-@6/gui8-smoke.py）任何滚新含注释与 boot 行漂移、TERM ok n=2 冻结值、pipe_selftest 断言与文案（G3）；「berry-term: bsh exited」等既有用户可见文案翻新与死因上行（新可见文案仅 busy/dropped 两行——死因 reason 只进 eprintln）；docs/shell设计-v1.2.md :68 透传契约句（G1 自证）；哑终端透传架构（本地键语义/回显生成）；v1 channel 对端关闭检测/DEAD 信号（v3 域——挂账 1）；berry-exec Drop impl/reap 改造（挂账 2）；bsh sleep collect_fg 化与 bsh 任何改动（挂账 3）；gui8 注入回归用例（挂账 5）；fs_gifts 拓扑级防御（挂账 6）；已进通道键可观测化（挂账 4）；pty 世界预投资（挂账 9）；git 脏件 10 M 任何触碰、实施轮 git 提交与 docs/阶段沿革.md 落笔（G4——主线统一显式路径＋取号临笔复核）；旧基（并 origin/main 前）直接施工（G4 硬门）；退出码新设（130 SIGINT 惯例沿用 pipe_selftest :571-578 先例）；strikes 击数规则改动（T5 口径不变——<2s 记击/≥2s 清零/三击熔断）。
砍刀重议触发（实发即停刀重议）：① e2e sleep 场景仍现「bsh exited」行或帧冻结 → 停刀归因（分诊链有漏）；② e2e 排空后键序错乱（回显序与键入序不符）→ 停刀重议保序面；③ 既有 termio 19 测或 TERM ok n=2 标记任何漂移 → 停刀归因（G1/G3 执法）；④ terminate 先行路径 e2e 实测 reap 竟 Err → 停刀升 berry-exec 面重议（terminate→TERMINATED 落账序假设破产）；⑤ 提交轮前域文件与重钉 tip 间 blob 漂移（lane 撞域）→ 暂停提交轮 rebase 重划。

## 待裁项（owner）

1. e2e 回归深度：本稿默认 host 纯面测＋一次性手跑 QEMU 存证（挂账 5 记薄面）；若要 gui8 常驻注入用例须另刀授权（动 gui8/smoke 面）。
2. 档位确认：CHILD_BUSY_HINT_NS=5s 与 KEYPEND_CAP=256B（本稿自定——纯常量面，实跑手感可调零结构牵连）。
3. hp 死讯臂恒挂 vs 窄挂（本稿默认子活期恒挂——非 EOF 死讯即醒即收；e2e 实测帧率受扰则回退窄挂档）。
4. shell设计-v1.2.md 版本档位（本稿默认 PATCH 纯补记；若主线判监督语义句属 MINOR 则按该文档自身修订律临笔定——文件名不滚）。
5. 新固定文案定稿（本稿默认「berry-term: child busy, N keys buffered」「berry-term: dropped N keys」——客机 UI 面，主线可改文案不改语义）。