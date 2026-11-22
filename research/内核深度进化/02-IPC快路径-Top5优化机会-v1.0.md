# 内核深度进化 02 号：IPC 快路径审计——send/recv 锁粒度·signal→wake 链·handle 表查找·批量 → Top-5 优化机会（影响×可行性）

> 版本：**1.0**｜2026-11-17｜文件名 `02-IPC快路径-Top5优化机会-v1.0.md`
> 定位：内核深度进化四路审计之 02 路（IPC 快路径）。审计对象 = `kernel/src/channel.rs`（258 行直读）+ `kernel/src/event.rs`（56 行直读）+ `kernel/src/handle.rs`（685 行直读）；快路径主体横跨调用方与调度侧，辅核 `kernel/src/syscall.rs` send/recv/wait/signal/copy 链段（131-370、3062-3630、3658-4047 直读）、`sched/src/sched.rs` 唤醒链（985-1007、2140-2260、2640-3180 直读）、`kernel/src/pi.rs`（全 262 行直读）、`chanface/src/lib.rs`、`intface/src/lib.rs`。
> 量化基线：icount 在册六项（`tools/bench_baseline_icount.txt` :14-19，2026-11-15 值，handle_evt=274）+ 4-4 fastpath bench TCG RT ≈ 28.4k 参考 cycle、真机 <1200/<500 验收线（`docs/阶段沿革.md` 4-4 段，单源）。
> 证据纪律：引用 = 仓内路径:行号（2026-11-17 工作树直核，行号随时点漂移，漂移按符号定位）；推断标「推断」；TCG/icount 数字均为相对口径（绝对值属真机阶段）。
> 只读审计：未改任何仓内文件；本件为唯一产出。禁 git 已遵守。

## 结论速览

一句话判词：BerryOS 的 IPC 快路径**分层与门控设计已经过硬**（fastpath/slowpath 共用唤醒序列、P1 无等待者 O(1) 门、P2-1 同键链扫描、PI 双计数门、send/recv 全链零分配），剩余的 Top-5 机会不在「缺优化」而在「**每次操作的恒定冗余**」——recv 一发搬 10 次页表走查 + 6 次 PT 锁 + 6 次解蔽窗、队列 pop 是 440B×O(n) 的 memmove、随行 handle 预检走最多 56 次类型探测查表、稳态下 PI 重推门因任意派生位睡眠者常开、批量语义完全缺席。五项按影响×可行性排序，前四项均不动 ABI 语义面。

## 基线与测量口径

icount 在册六项（`tools/bench_baseline_icount.txt` :14-19）：cntvct_pair=0 / frame_alloc4k=37 / frame_alloc64k=82 / handle_evt=274 / vmo_commit1p=1683 / vmo_rw4k=6948（指令数代理，0.125 cyc/instr 折算口径同文件头注）。其中 handle_evt=274 覆盖 Event 的 insert+get+close 全生命周期——**查表门本身很便宜**（单次 lookup ≈ 界检+代比+槽读），这坐实了热点的重心不在单次查表而在「查多少次」与「拷贝/走查多少回」。**缺口记录**：基线无 send/recv 项——IPC 主路径只有 4-4 的 FAST bench（TCG 墙钟 RT，`kernel/src/bench.rs` 外的 boot 任务形态），icount 确定性通道未覆盖；本审计所有 IPC 侧量化均为结构推算（标「推断」），建议随 Top-5 任一刀补 send/recv_pair 入基线（落选记录三）。

## Top-5 总表

| 排名 | 机会 | 审计维度 | 影响量级（推断） | 可行性 | 动面 |
|---|---|---|---|---|---|
| 1 | recv/send 用户参数 I/O 合并单窗 | 锁粒度 | 每 recv 10 次页表走查+6 次 PT 锁+6 次解蔽窗 → 约 2/1/1 | 高（基建已备） | 仅 syscall.rs |
| 2 | PI 重推常态门收紧 + 传播相静态边缓存 | signal→wake 链 | 多任务稳态每次唤醒 O(H·S·(S+R)·槽) 主力削减 | 中 | pi.rs + sched 计数面 |
| 3 | Channel 队列环形化 + 消息就地构造 | 队列机械 | 每 recv 最高 ~3.1KB memmove 归零；send 拷链 3→1 | 中高 | channel.rs + chanface 钉板 |
| 4 | 类型探测链折叠为类型无关单查 | handle 表查找 | 随行 send 最多 56 查、wait 最多 5 查 → 各 1 查 | 极高（rights_of 已在册） | syscall.rs + handle.rs |
| 5 | 排空式批量 recv ABI（add-only） | 批量 | 排空型服务每消息省一整圈 SVC 入口+参数窗 | 中 | 新号 + libc |

## 机会一（锁粒度）：recv/send 用户参数 I/O 合并单窗

**现状证据**。recv 的用户内存触点（`kernel/src/syscall.rs`）：copy_in_u64 读 nbytes 容量（3501-3508）与 nhandles 容量（3509-3516）各经一次 `irq_window` + 一次 `pt_lock_current` + 一次 `access_ok` 走查；出参预扫 access_ok×4（3532-3540，每次各自 `walk_leaf` 三级走查）；载荷拷出 copy_to_user（3541-3545）、随行值拷出（3563-3567）、nbytes/nhandles 回写 copy_out_u64×2（3568-3577）又各带一次 `irq_window` + PT 锁 + 走查。计 **10 次 walk_leaf、6 次 PT 锁取放、6 次 irq_window 开包**（copy_in_u64 内持锁见 `kernel/src/syscall.rs`:322-330，copy_to_user 见 311-319，access_ok 逐页走查见 177-205）。send 侧同形两窗（3235-3238、3281-3284）。

**热点机理**。每次 walk_leaf = 三级表依赖载入 + AP 位判（`kernel/src/syscall.rs`:195-203）；每次 PT 锁取放 = 一对原子/临界区。这 10+6 全部发生在**每一条消息**的收发上，且 nbytes/nhandles/bytes/handles 四段在典型调用里同页或邻页——10 次走查里 8 次是重复验同一两页。对照真机验收线 <1200 cycle/往返（单源，阶段沿革 4-4 段），仅走查项即数百 cycle 量级（推断：按每走查 3 次依赖载入 ×8 冗余估）。EL1 内核服务（console 任务）走高半快判免走查（184-187），**全成本由 EL0 用户服务（svcmgr/echo/talker 及后续生态）承担**——恰是微内核形态的主力路径。

**方案**。vmo 族已有完整判例：单 PT 锁窗 + `last_page` 同页跳过 + 分块块流（`access_ok_window`:221-244、`copy_from_user_locked`:250-271，S-4 4-2 落地）。把 recv 的「两读预扫 + 四写」收进**一个** PT 锁窗与**一个** `last_page` 缓存（preempt 段守卫不动——`PreemptSpan`:3130-3145 仍在，仅解蔽窗从 6 个并成 1-2 个）；send 同理并窗。语义零变化：预扫本就是「fail-fast 纯走查、非安全边界」（3522-3531 注），并窗后边界仍由锁定拷贝承担。

**风险与守卫**。窗口变长 = IRQ 解蔽时长略增（仍受 256B+双 8B 出参界 bound）；ktest H 段与 RX 相回归面现成。判分：影响大（恒定、每发、确定性）×可行性高（基建现成、纯调用方重构）→ **Top-1**。

## 机会二（signal→wake 链）：PI 全图重推的常态门收紧与传播相静态边缓存

**现状证据**。每次 SLEEPQ 变更（block 睡下、wake 摘队、fast_wake、handle_close）都发射 PI 全图重推（`sched/src/sched.rs`:2152-2157 → `kernel/src/pi.rs`:70-72）。重推入口有 P2 双计数门：等派生位睡眠者与 pi≠0 睡眠者**双零**才整体跳过（`kernel/src/pi.rs`:100-102）。门开则：清零趟两遍（SLEEPQ+RQ，113-114）+ 不动点至多 8 跳（131-176），每跳每 donor 对**每候选者**做 `holder_table` 反查 + `holder_has` 全槽扫（140-171；`kernel/src/handle.rs`:446-454 的 O(槽) 线性扫）+ 尾趟刷两遍（190-196）。

**热点机理**。门控条件是「**存在任一**派生位睡眠者」——而系统常驻服务恰好常态睡在派生位上：console 稳态等 endK 的 READABLE（阶段沿革 4-5a 段）、wait_many 哨兵不豁免同键真等者。只要 console 睡着，**全系统每次 send/recv 唤醒都进门开分支跑全图重推**，即便全员 prio 0、永无捐赠可传播（`kernel/src/pi.rs`:92-95 自注「常态 IPC 无捐赠边即此路」在常驻派生位睡眠者在场时不成立——推断：由门条件语义直推）。多客户端服务（C 个 client 阻塞 WRITABLE + server 阻塞 READABLE）是最坏形：每跳成本 O(donor×(S+R)×槽)，8 跳封顶（推断：按 S=R=10、槽 64 估单次重推 10⁴ 级槽比较——量级推断，未实测）。

**方案两刀**。刀 a（门收紧）：计数①只数「等派生位**且 prio_effective>0**」的睡眠者——prio 0 的 donor 恒无捐赠可传（`pi.rs`:142 的 `d_eff > cand.pi_inherited` 在 d_eff=0 时恒假），全零稳态（v1 常态）门即常闭。计数语义需随 eff 变更重登（refresh_effective/policy.apply 面），是本刀的全部复杂度。刀 b（静态边缓存）：一次重推内 SQ/RQ/KL 三窗全持（89-109），睡眠者 wait 字段、表内容、proc_tag 在窗内冻结——**边集（donor→候选持有关系）跨 8 跳静态**（推断：由三窗冻结序直推，建议实跑验证），可首跳建边入静态有界数组（零分配纪律下用固定容量 + 溢出回退逐跳原路径），余跳只在边上传播——传播相 8×→1×。

**风险与守卫**。ktest J 段（J0-J7）是语义钉板；FAST bench 的 hits/misses/direct 三计数器与 ai_ping poll-0 直切确定性测试是行为守卫。判分：影响最大（随任务数超线性、常态常开）×可行性中（门语义重登 + 边缓存各需一次设计审查）→ **Top-2**。

## 机会三（队列机械）：Channel 队列环形化 + 消息就地构造

**现状证据**。队列 = `Vec<Message>`（`kernel/src/channel.rs`:107），pop 用 `remove(0)`（228-231，注释「深度 ≤8：O(n) 可忽略」）、回滚 push_front 用 `insert(0)`（237-240）、容量由 create 时 `try_reserve_exact(CHAN_DEPTH)` 预储（132-136）。Message 是定长内联结构（52-65）：bytes[256] + handles 8×16B=128 + MsgIntent 16（`intface/src/lib.rs`:75-82）+ MsgBilling 24 + 两计数字段 → **约 440B/条**（推断：按 repr 布局推算）。

**热点机理**。「O(n) 可忽略」的注记写在 3b3 H2 定长内联化**之前**的形状上——内联后 remove(0) 是 (len-1)×440B 的 memmove，满队 recv 一次搬 **~3.1KB**（7×440，推断）；recv 失败回滚路径再搬一次。send 侧载荷拷链：user→bbuf（copy_from_user）→ msg.bytes（copy_from_slice，3301）→ push 进队槽（440B struct move，3379）= **3 次拷贝**，其中两次纯中转。真机 <1200 cycle 预算下，3KB memmove 是百分之十几的直接占比（推断）。

**方案**。定容环形（`[Message; CHAN_DEPTH]` 或 `Box<[Message]>` + head/tail/len 三字段——单核窗内访问，无需原子）替换 Vec：pop/push/push_front 全 O(1)，Drop 消费区间保既有「未收割令牌随队泄漏」政策（channel.rs:243-257 注）。chanface 四判据（push_ok/pop_ok/push_front_ok/queue_full_of 均吃 len）以 `len()` 供给，钉板零改动（`chanface/src/lib.rs`:55-63 界常量不动）。进一步：send 满检过后取槽位可变引用，user→槽位 bytes **单拷贝直达**（bbuf 与 msg 中转全消）；随行 handle 预检/take 序天然兼容（拷贝仍后于满检与快照，原子序不变——`kernel/src/syscall.rs`:3206-3232 论证链不动）。

**风险与守卫**。chanface host 钉板 + ktest H 段八组（含深度满 -EBUSY 与回滚保序）是双守卫；冻结属主标在端点不在队列（channel.rs:122 注），零牵连。判分：影响大（每发恒付、真机占比高）×可行性中高（局部替换、判据面不动）→ **Top-3**。

## 机会四（handle 表查找）：类型探测链折叠为类型无关单查

**现状证据**。send 随行 handle 预检对**每枚** handle 逐一试 7 个 ObjType 的 `get_raw`（`kernel/src/syscall.rs`:3253-3276）——命中前的每个类型都是一次完整 lookup（界检+代比）+ ObjRef 解引用 + 类型比对；CHAN_HND_MAX 现为 8（`chanface/src/lib.rs`:61，A2-3 从 6 抬 8），最坏 **7×8=56 次查表探测**，且 3251 行注释「7 次定点查表 × nhandles ≤ 4」的「≤4」是 HND_MAX=6 时代的**陈旧口径**。wait_get 同形五类型链（Event|Channel|Process|Thread|SuspendToken，3755-3774）——Channel 等待者（server 循环主力形）付 2 查，SuspendToken 付 5 查。

**热点机理**。probe 链的本意是「拿 rights + 探活」，但 handle.rs **已有类型无关查询面** `rights_of`（`kernel/src/handle.rs`:586-588，P3-1 起在册、handle_duplicate 执法消费）——查表+代校后直接回 rights 全集，不验类型。KernelObj 恰好只有 7 个变体（80-89）且探测表全列（syscall.rs:3255-3263），两路径语义**逐位等价**（死值同 -EBADF、活值同 rights）。注释自辩「HandleTable 无类型无关查询面」（3248-3250）与 rights_of 的存在直接矛盾——这是审计意义上的**即时可收**项。

**方案**。send 预检环体换 `t.rights_of(hvals[i])` 一查（死值/TRANSFER 判面不变，3269-3275 不动）；wait_get 换底层 `lookup` + 一次 `obj_type()` 分发（需把 lookup 升 pub(crate) 或加 `get_typed` 薄面，错误码保「首错=Event 的」既有契约，3753-3754 注）。顺带修正 3251 行陈旧注释。

**风险与守卫**。ktest H 段迁移闭环面 + I 段 wait 面 + AM-1.5 token 面全在册；改动是纯查表次数削减、零语义位移。判分：影响中（随行 send 与每圈 wait 各省数查至数十查）×可行性极高（函数已在册、当天可落）→ **Top-4**（排位让位于前三的每发恒定大项，但「性价比/工程小时」全场第一）。

## 机会五（批量）：排空式批量 recv ABI（add-only）

**现状证据**。recv 恒单消息语义（`kernel/src/syscall.rs`:3460-3470 ABI 注），排空队列 = 逐发 syscall。console 服务循环已是「wait_many 唤醒 → drain 循环 recv」（阶段沿革 4-5a 段）、C-4 console 逐字节转发实测「单唤醒只发 1 字节、一次唤醒收 ≥2 字节」的并聚形态（`docs/SMP设计草案-v1.0.md`:316 f42t 实录）——**唤醒粒度与收割粒度天然失配**，排空型消费者每消息重复付：SVC 入口结构锁窗 + caller_table + get_raw 门控 + 参数 I/O 全套窗（机会一合并后仍有残量）+ WRITABLE 唤醒链（3584-3586）。

**方案**。add-only 新号（仓内先例：0x21/0x23/0x24/0x83/0x84 均后补号，syscall.rs:4126-4188）`channel_recv_n(ch, msgs[], k, …)`：内部循环既有 recv 原子序，**每消息语义逐条保持**（intent 结算 3606-3622、billing 3593-3603、insert_ref 入表均 per-message），仅 SVC 入口/门控/WRITABLE 唤醒摊到 k 条；返回实收数，队空即 0（poll 语义同形）。libc 面收 `berry_channel_recv_n` 薄封装。

**风险与守卫**。新号零回归面（未列号 -ENOTSUP 默认）；同步往返型服务（echo/talker）无收益、不强制迁移——收益专属排空型（console/netd/后续 fs 读前）。判分：影响中（场景明确但非全形态）×可行性中（ABI + ktest + libc 三面）→ **Top-5**。

## 落选记录（有据不列，留档防重复审计）

一、**send/recv 拷贝段 KL 降档**（锁粒度·SMP 面）：send/recv 挂 SVC_CLASS_SQ_RQ_TABLE=0x2E（syscall.rs:4092-4093、4162-4163），SVC 入口结构锁窗把 KL 全持过整个 handler 含拷贝段——单核零争用、SMP 实验态则与一切 TABLE 类跨核互斥。落选理由：单核现役零收益，且属 S-3c 拆锁既定轨迹内的通用工程（vmo 族 S-4 4-2 判例 §A115 四问已立），非 IPC 专属机会；机会一并窗后此窗的拷贝段占比同时缩小，届时报益更大。二、**Event::signal 单指令臂**：`kernel/src/event.rs`:35-50 恒走 fetch_update CAS 环，纯置位（clear=0）可 fetch_or、纯清位可 fetch_and——单核锁窗下 CAS 环零重试、收益亚周期级（对照 bench_am_pair 的 fetch_add 口径，bench.rs:206-229），仅值得作为顺手刀搭任意一刀。三、**icount 基线补 send/recv_pair 项**：本审计全部 IPC 量化只能结构推算的根因；建议随 Top-1/3 任一刀同步入 `tools/bench_baseline_icount.txt`（形态仿 handle_evt 的内核侧直调对，1024 次 send+recv 往返）。

## 验证与验收建议

任一刀落地按 berryos-dev 分层验证链：quick 相（fw_cfg 键表对拍 + 文档自检 + ABI 对拍）保 ABI 面；host 相加 chanface/hdlface host 钉板与 berry CLI 测试；QEMU e2e 手动跑主 smoke 五相（ktest H/I/J 段 + FAST bench 三计数器 + RX 相 f42t 是 IPC 面的行为金丝雀）。量化验收：刀一/刀三以 FAST bench TCG RT 与（补入后的）icount send/recv_pair 双通道对拍，刀二以 PI 段计数（recompute 进门次数可经 telemetry 临时探针）证常态门闭合。
