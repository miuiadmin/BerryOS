# astrid EventBus 公平实现深读(per-principal FIFO + deficit round-robin)

> 调研日期 2026-11-08 · 方法:git clone --depth 50(HEAD f519aaa,2026-11-07)实读 astrid-events 源码——bus.rs(889 行)、bus/dispatch.rs、route.rs 及 route/{entry,receiver,matcher,gate,state}.rs、ordered_delivery.rs,辅证 astrid-types/src/ipc.rs 与 entry_tests.rs 测试名;对照基准 = 本仓 `docs/IPC设计-v1.0.md` 背压与调度器口径(只读)
>
> 本文为 19 号专题;结论均注出处(astrid-events/ 相对路径:行号,均指 /tmp/os-src/astrid 快照),不自造事实。上游口径 = 24 号（原 12）§2.1「tokio broadcast + 发布侧 per-principal 路由表 + per-principal FIFO 子队列 + deficit round-robin 公平」(经 book 转引),本篇为源码下探验证。

## 1. 实现结构:四个问题的源码答案

### 1.1 per-principal FIFO 子队列的结构与背压——满则丢最老,永不阻塞

- 结构是两级 fan-out:总线持 `routes: RwLock<HashMap<RouteKey, Arc<Mutex<RouteEntry>>>>`,键 = `(capsule_uuid, topic_pattern, subscription_rep)` 三元组,subscription_rep 是跨总线单调编号,同 pattern 的两个订阅者各拿独立条目、各收一份拷贝(bus.rs:105;entry.rs:70-80;state.rs:23-29);`RouteEntry` 内部才是 `fanout: HashMap<PrincipalKey, PrincipalQueue>`,PrincipalKey = `Option<String>`(None = 内核 system principal)(entry.rs:84,109-116)。每个 `PrincipalQueue` = `VecDeque<Arc<AstridEvent>>` FIFO + `bytes`(队内事件字节数和)+ `head_enqueued_at`(头龄,驱逐用)+ `deficit`(DRR 欠账)(entry.rs:95-105);另有 `principal_order: VecDeque<PrincipalKey>` 记录去重后的 principal 入环序供轮转(entry.rs:116)。子队列按需分配,闲 principal 零条目——5000 principal 不必然 5000 条目(route.rs:40-42;bus.rs:99-101)。
- 背压是丢弃不是阻塞,且分三级:`push_with_eviction` 是同步函数,发布侧永不等待。①单消息字节数超全路由预算直接拒收(entry.rs:286-306);②全路由字节预算 `MAX_SUBSCRIPTION_BUDGET_BYTES = 1 MiB` 超限时循环「最老头驱逐」——线性扫描找 `head_enqueued_at` 最小的桶弹队头,直到新消息放得下(entry.rs:22;309-317;371-429);③每桶消息数兜底 `PENDING_PER_PRINCIPAL_FALLBACK = 16_384`,专防近 0 字节小消息洪泛绕过字节预算(entry.rs:52;333-360)。字节计量 `ipc_size_of` = payload JSON 长度 + topic 长度,非 IPC 事件按 64 计(matcher.rs:97-106)。
- 丢弃只打 `tracing::error!`(security_event)+ `METRIC_ROUTE_BYTE_EVICTIONS_TOTAL` 计数,无带内通知——完整性关键消费方被明确告知要么及时排空、要么以持久审计日志为准(entry.rs:140-147)。流式响应的终结符按构造免疫头驱逐:它总在所属 principal 队列的尾,头驱逐只删前缀(route.rs:49-51)。消费侧空队列时 park 在 `tokio::sync::Notify` 上,发布侧 enqueue 后 `notify_one` 唤醒(receiver.rs:56-88;dispatch.rs:50-52);`RoutedEventReceiver` 的 `drain_lagged` 恒返回 0——丢弃全部发生在发布侧(receiver.rs:99-106)。

### 1.2 DRR 配额与结算——量子 = max(4 KiB, budget/N),deficit 跨轮携带

- 量子自适应:`quantum = max(DRR_QUANTUM_MIN_BYTES, budget / principal_order.len())`,地板 4 KiB;注释给出算术:5000 活跃 principal × 4 KiB = 20 MiB 理论每轮吞吐,远超 1 MiB 总预算,保证每 principal 每轮都有进展、长尾不饿(entry.rs:24-28;441-444)。budget 由消费方排空时传入(`try_drain(budget)`),不是常量(receiver.rs:92-97)。
- 结算循环(`drr_drain`,entry.rs:434-506):外层 loop,每轮访问环上全部 principal——弹出键、`deficit += quantum`,内层 while 只要队头消息 size ≤ deficit 且本轮 served + size ≤ budget 就弹出一个、`deficit -= size`、同步扣 bytes/total_bytes/served;一桶服务不完(队头超 deficit)记 `METRIC_ROUTE_QUANTUM_STARVED_TOTAL` 诊断计数后把桶放回环尾,服务空了则连桶带条目删除;外层直到一整轮无进展或 served ≥ budget 退出。deficit 用 saturating 加减不会下溢,跨轮携带但只在被访问时累加——空桶即删,不存在离场攒账(entry.rs:104;456-502)。无经典 DRR 的显式 deficit 上限,上限事实由「量子固定 + 空桶删除」隐式约束。
- 接收快路径 `drr_pop_one` 是纯轮转不看 deficit:弹一个消息、该 principal 转回环尾、空桶删除(entry.rs:518-549)。刻意单弹的动机是 loss-free:批量 drain 只返回首个会静默丢其余(entry.rs:508-517;receiver.rs:58-71 注释)。因此公平是双层的——低速率消费(单弹)得到纯轮转的机会公平,高速率批量排空(`drr_drain`)才有字节配额公平;测试背书 `drr_two_principals_yield_equal_counts` / `drr_interleaves_when_quantum_caps_per_round` / `fairness_under_5000_principals_makes_progress`(entry_tests.rs:106;144;279)。
- 准入与公平显式解耦:scope(`Exact(principal)` / `PrincipalOrSystem`)只决定谁进桶,外键 principal 事件在 enqueue 前即拒、永不占预算、永不驱逐属主条目;DRR 只决定已接纳者的顺序(entry.rs:130-134;233-263;dispatch.rs:42-44 的二次 fence)。运行代次门 `RouteAdmissionGate`(staged/published/retired 三态原子)让暂存 runtime 不收事件、开闸只放未来流量防重放(gate.rs:20-74;bus.rs:552-560)。

### 1.3 发布侧路由表与广播扇出——双路径共存,routed 刻意排在 broadcast 之后

- 总线同时持三条通道:全量 broadcast(`sender`)、best-effort lane(`unreserved_sender`,排除已预留投递)——各容量默认 1024(bus.rs:42;84-86;120-122),以及独占有序 dispatcher inbox(mpsc,容量 = 总线容量;普通发布 `try_send` 满即丢计 lagged,预留发布 `reserve_owned` 等容量不丢)(bus.rs:210-234;316-345;ordered_delivery.rs:30-48)。routed 面不走 tokio broadcast,是上述 routes 表 + Notify。
- `publish_reserved` 的总序(publish_order 可重入锁内):盖 seq → 有序 lane 投递 → broadcast `send` → 同步 registry 通知;放锁后才 `dispatch_to_routes`(bus.rs:292-388)。关键注释:fan-out 排在 `broadcast::send` 之后,慢的 routed enqueue 永不拖累无定向消费者——kernel_router、admin_router、bus_monitor 等宿主消费者全在 broadcast 上(bus.rs:380-385)。`dispatch_to_routes` 只处理 IPC 事件,读锁内做准入 + topic 匹配快照、放表锁再逐条目 fence + 入队 + 唤醒——慢接收者不阻塞并发订阅或兄弟发布者(dispatch.rs:12-54)。
- broadcast 侧自身背压是 tokio 环形语义:慢接收者 Lagged(n) 丢旧、计 lagged 计数并 yield 后再追(bus.rs:832-851);消费侧 topic 过滤排空积压时每跳过 32 个不匹配事件 yield 一次防独占 worker(`YIELD_AFTER_SKIPPED = 32`,bus.rs:44-52;812-830)。双路径的动机是 #813:此前 guest 订阅全走全局 broadcast + 消费侧过滤,100 宽跨 principal burst 打满 1024 容量丢事件,混主体批次被截断,出现「并发悬崖」;发布侧 demux 给每路由独立字节预算、每 principal 独立 FIFO,扇出发生在 broadcast 背压之前(route.rs:10-21)。

### 1.4 单调序列号与公平性——发布序单一事实源,与公平决策彻底解耦

- `ipc_seq: Arc<AtomicU64>` 初值 1,总线全 clone 共享;publish 时对 IPC 事件 `message.seq = fetch_add(1, Relaxed)`(bus.rs:93-94;130;306-312;740)。字段定义:「由事件总线在发布时赋值的单调序列号,dispatcher 用它保证 per-capsule 有序投递」(astrid-types/src/ipc.rs:129-132);有序 inbox 的契约即「按总线序列序接收」(ordered_delivery.rs:39-42)。
- seq 赋值在 publish_order 临界区内,与 broadcast 投递、同步观察构成同一全序——后发的 disconnect 不可能越过先发的 connect(bus.rs:295-304)。这个锁也保证 RoutedEventReceiver 看到的跨 principal 交织序确定。
- 与公平性的关系是分层而非耦合:seq 只标发布序,DRR/轮转不看 seq——公平单位是 principal 身份,字节配额重排跨 principal 顺序,所以 routed 消费者看到的跨 principal 序必然偏离全局发布序;seq 保证的是每个 principal 子队列内部 FIFO 即发布序(入队即发布序)以及 ordered lane 的全序。消费者可拿 seq 检测丢失/乱序,路由层自身不用它做任何公平决策(entry.rs 全文无 seq;盖号仅在 bus.rs:306-312)。

## 2. 与规范书口径(24 号 §2.1)的差异

- 四要素全部源码证实,无翻案;需要细化的四点:①「per-principal 路由表」实为两级——表键是 (capsule, topic_pattern, subscription_rep),per-principal 队列是条目内部的第二级 fan-out,「per-principal」不在表键上(bus.rs:105;entry.rs:70-80;114)。②公平参数具体化:1 MiB 字节预算 + 4 KiB 量子地板 + 16_384 条目兜底 + max(量子, budget/N) 自适应,24 号未记。③双路径排序: routed fan-out 刻意排在 `broadcast::send` 之后,慢路由不拖累广播消费者(bus.rs:380-385)。④「EventBus = tokio broadcast」只覆盖广播面;routed 面是 `RwLock<HashMap>` + `Notify` + `parking_lot::Mutex`,与 tokio broadcast 无关(bus.rs:105;receiver.rs:23-30)。
- 24 号自注「经 book 转引」,本篇为直读源码;astrid book 未逐章核对,若 book 有更细承诺以源码为准。

## 3. 对 BerryOS IPC/遥测环的启示

### 3.1 适用面(语义层)

- 准入与公平分离:scope 决定谁进桶、DRR 只决定已接纳者顺序,外键流量在入队前即拒、不占预算(entry.rs:130-134;274-282)。BerryOS channel 的 badge 是同位问题(server 凭 badge 区分多 client,队列纯 FIFO,`IPC设计-v1.0.md` §2);若未来多 client 需公平,语义应是 badge 维度子队列 + 配额轮转,而不是动调度器优先级。
- 字节为主、条目为兜底:主准入记字节(1 MiB),条目数只堵近 0 字节洪泛,且源码自己承认条目数是内存的粗代理、真堆挂在 Arc 上(#1159 留账)(entry.rs:44-51)。内核队列若做配额应记字节或槽位×上限,记条目数会被小消息洪泛绕过。
- 量子地板防长尾饿:max(4 KiB, budget/N) 保证主体数大时每轮仍有进展(entry.rs:24-28)。此语义可平移到内核定长结构:deficit 是 usize、量子是常量,不需要分配。
- 结构同构可互认:per-queue FIFO 保发布序 + 轮转游标只决定跨队列取序,与 BerryOS 调度器「单游标环、同 eff 层内 FIFO 轮转、跨层严格优先」(4-3a:两圈扫描 + 同高 FIFO + advance_past,`IPC设计-v1.0.md` §2.1)是同一形状——差别只在公平单位(astrid = principal 身份平等,BerryOS = 优先级层间不平等)与「公平只叠加不改变层间序」的既有冻结。
- 丢终结符挂死的教训:每桶 256 太小曾静默丢流式响应的 Done 终结符,消费方永久挂起,才加到 16_384(entry.rs:36-44)。对共享环协议直接可用:边界/终结帧不可设为可丢,或用代次计数让丢帧可检测——BerryOS 热路径环「无入队、丢旧可接受」(`IPC设计-v1.0.md` §7)的前提恰是不承载需要终结语义的流。
- #813 并发悬崖的结构教训:消费侧过滤 + 共享有界环,主体数超过缓冲余量即崩塌;结构修法是发布侧分桶,每桶独立预算(route.rs:10-21)。BerryOS 遥测环当前 SPSC 单生产者无此面;若未来多生产者化,分桶必须在入环侧而非消费侧。
- 单弹与批排空分离的 loss-free 论证:批量操作只回一部分会静默丢其余,所以快路径恰好取一、余量留队(entry.rs:508-517;receiver.rs:58-71)。内核服务循环「wait READABLE → drain recv 排空」的写法同理:排空要么到空,要么逐条全收。
- 同一系统内背压按路径分档:astrid 正确性关键路径走 reserve 等容量不丢、best-effort 路径丢旧 + 审计计数(bus.rs:163-208;entry.rs:140-147);BerryOS「channel 满则 -EBUSY 发送方处置 + 热路径环丢旧」(`IPC设计-v1.0.md` §2、§7)已是同构分档,只需守住「内核不替正确性路径做丢弃决策」这条线。

### 3.2 不适用面(直接移植不可行)

- 数据结构层全不可移植:HashMap/VecDeque/Arc/Instant/Notify/parking_lot 都是用户态分配热路径;BerryOS 内核无分配热路径(决策 #5:热路径走共享环零拷贝无入队;channel inline ≤256B、深度 8),公平状态若进内核只能侵入式链表 + 定长槽位 + usize deficit,Oldest-head 线性扫描(entry.rs:418-429 自认 5000 principal 热点)在内核更不可接受。
- 丢弃语义不可平移到内核正确性路径:满则丢最老 + 仅审计计数 + 无带内信号,前提是消费方可回退到持久事实源;内核 IPC 无此回退,满则 -EBUSY 把处置权交还发送方是 BerryOS 已冻结的口径。
- 量级与驱动方式不同:astid DRR 的预算由消费方在排空时传入(receiver.rs:92-97),面向「一个订阅者对 5000 生产 principal」的扇入;BerryOS v1 channel 深度 8、服务循环一次排空,该规模问题尚不存在。
- 公平目标不同:astrid 是无优先级平等轮转,BerryOS 是优先级调度 + 意图叠加(公平默认、只叠加,AGENTS.md 冻结)——可借的是「子队列 + 配额 + 地板」的结构语义,不是调度策略本身。
