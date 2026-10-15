# IPC 设计

> 决策依据见 `架构决策.md` #5、#6、#28。本文件展开"全支持 IPC（同步 channel + 异步通知 + 共享内存）+ 路径式命名空间"。

## 1. 三条路径，按场景选

| 路径 | 用途 | 特点 |
|------|------|------|
| **Channel**（同步端点） | 请求/响应、RPC、能力传递 | 主路径；阻塞 send/recv；可传 handle |
| **Event / EventPair**（异步通知） | 唤醒、IRQ、廉价信号 | 位掩码；零分配；配 tickless 只在有事时唤醒 |
| **共享内存**（VMO 映射） | 大块数据、framebuffer、DMA | 零拷贝；经能力授权映射 |

## 2. Channel（同步端点）

- **双端**：一对端点，一端 send 一端 recv，FIFO，有界队列。
- **消息** = `inline 字节（≤ 256B）` + `handle 列表`。超长数据走共享 VMO，inline 只传小信封/控制信息。
- **阻塞语义**：满则阻塞、空则阻塞；可设超时/非阻塞标志。
- **badge**：server 端连接带 badge，server 据此区分 client（不为每个 client 建独立 channel）。
- **msg_intent: u8（内核可见元数据槽，ADR #42，4-3c 已落）**：消息头 `MsgIntent{intent, hop, acc_ns}`（内核可见、用户不传参不回写）携带发送者 `intent_effective` 快照；server recv 结算 `intent_effective = min(declared, msg_intent)`（数值越小档越高——语义 max）、**每消息重导**（弱消息自然回落 = reply 即回退的无 reply 等价物）；channel 级 opt-out = `channel_create` options bit0。三道帽：跳数 ≤8 / 每跳预算 50ms / 链累计 400ms，超顶降级为不传播。
- **快路径**：短消息缓存行级、无通用堆拷贝；handle 迁移 O(1)。全可抢占下保持临界区短、锁粒度细。

### 2.1 优先级继承（PI，QNX 发送者语义 · 战略裁决 S9 / D4 无悔项）

> 微内核服务化后，**软实时的命门在 IPC 不在调度器单点**：高优先级 client 阻塞在 send 等低优先级 server，优先级反转经 channel 链放大（Mars Pathfinder 教科书案例）。语义照抄 QNX 幸存者组合（S9 零创新预算），设计随 P4 channel 落地。

- **发送者捐赠**：client 在 channel `send` 阻塞期间，其有效优先级**捐赠给接收方**；server 处理该消息期间持有该优先级（消息携带发送者优先级，reply 阻塞同理）。
- **等待队列按优先级排序**：server 端的 send 等待队列、client 端的 recv 等待队列按优先级插入——高优先级消息先被看见、先被处理。
- **链式传播有界**：boost 沿「send 阻塞链」传播（client→server→server 正阻塞的下一个 server…），深度上限（如 8 层）防环防长链；到上限按已达 boost 处理，**不 panic**（syscall 边界纪律）。
- **释放与回落**：消息被 recv 取走 / send 超时 / channel 关闭（返 `DEAD`，决策 #28）即解除；server 回落到基础优先级。boost 是临时态，不持久污染权重/nice。
- **内核表示**：TCB 双优先级 `base` / `effective`（`effective = max(自身, 继承来源)`），boost 变化即按 `effective` 重排 runqueue——联动机制见 `调度器设计.md` §3.6。
- **intent 随消息传播（ADR #42 五件套，并入本捐赠框架不另起第二套 · 语义随 P4）**：
  1. **reply 即回退**——临时 override（XNU requested/effective 分离）：`intent_effective = max(intent_declared, 借来的)`，消息处理完回落到 declared，绝不永久改 server 声明档（QNX MsgReply 继承不回退的粘性膨胀是已出货病态）。
  2. **账记客户端**（QNX 分区记账：server 替 client 干活的时间记 client 的带），但结算全在 slowpath（dequeue/reply 时）——fastpath 只做一次 u8 cmp+sel **零记账**（QNX 砍 APS 的反面教材：它死在把记账放进热路径）。
  3. **每跳执行预算帽 + 累计时间封顶**（seL4 Johnston 2022：仅限跳数不够，恶意/失控服务链能吃光调用方预算），超顶降级为不传播并打 trace。注意 §2.1 的 8 层限的是**阻塞链**深，intent 传播限的是**消息链**的预算与时间——两条链不重合。
  4. **channel 级 opt-out** 逃生门（系统服务/看门狗类 server，QNX `_NTO_CHF_FIXED_PRIORITY` 等价物）。
  5. **共享内存 VMO 不自动传播**（需要时经显式 API）+ **值通胀计数器进遥测**（max 每跳单调不降，高频公共服务被 LC 客户端触达后整条链向顶带收敛 = 「一切皆顶带 = 没有 band」，计数器是它的可观测信号）。
- **阶段化**：v1 只预留 TCB 双优先级字段（成本≈0，随 Phase 2b 表示法字段落地）；**PI 捐赠已随 4-3b 落地（2026-10，v1 形态 = 阻塞期捐赠 + 冻结随行近似「处理期间持有」+ 持有者 handle 表反查，细节见 Phase 4 拆步表注记）**；**intent 五件套已随 4-3c 落地（2026-10）**；验收 = 三任务链（高优先级 client → 慢 server ← 低优先级干扰者）下测 client 唤醒延迟上界（G 卡用例）。

## 3. 异步通知（Event / EventPair）

- **Event**：单点信号；**EventPair**：成对，一侧 signal 另一侧观察到。
- **位掩码语义**：一个对象上可同时置多位（如 `IRQ | READABLE | WRITABLE`），wait 一次取所有就绪位。
- **IRQ 投递**：设备 Interrupt 对象 signal 一个 Event；驱动用户态服务 wait 该 Event —— 这是**用户态驱动的中断入口**。
- 与 tickless 协同：空闲核只在 Event 被 signal 时被唤醒。

## 4. 共享内存

- VMO 经 `RIGHTS_MAP` 授权后映射（map）到两个进程的 Vmar，双方按权限读写。
- 用于大缓冲、framebuffer；DMA 需连续/锁页 VMO（见内存管理设计）。

## 5. 服务发现：路径式命名空间（Plan9/Fuchsia 风）

- 全局一棵命名空间树；服务把 Channel 端点**挂载**到路径：`/svc/fs`、`/svc/net`、`/dev/console`。
- 访问服务 = `open("/svc/fs")` → 返回一个 handle（底层是 Channel 端点能力）。
- **namespace 服务**（用户态）持根，做查找与授权；你能看到哪些路径 = 你持有的 namespace handle 的权限。
- init 持 root namespace，按能力派生给各服务/进程。
- **mount**：服务把自己的 Channel 挂到路径；支持多挂载点、挂载策略。
- **分阶段**：先用纯能力传递跑通服务互联；namespace 作为其上的可用性层后加。

## 6. 能力流动 = IPC 流动

- Channel/Event/Interrupt 都是对象 → 都有 handle、权限、可经 Channel 传递。
- 传 handle = 传能力 → IPC 既是通信也是授权通道（纯能力的体现）。

## 7. 性能与低延迟约束

- 短消息快路径无堆分配；Event 信号零分配。
- 全可抢占内核下，IPC 路径短、锁细，避免长临界区拖尾延迟（呼应软实时）。
- pager 缺页事件也走 Event/共享环快路径（见内存管理设计）。
- **热路径走共享环，不走 channel（决策 #5 硬约束）**：PM governor 设频、AI 策略注入/遥测、pager 缺页等延迟敏感路径，用**共享 VMO + ring 协议**（零拷贝、无入队），**禁用 channel 消息拷贝**（否则吃延迟，见 `调度器设计.md` §5.2、`AI调度设计.md` §3）。channel/event/VMO 服务通用流量，共享环独占热路径——**任何 IPC 形态不得统管所有流量**（`架构综合纲领.md` 排除第 4 问）。
- **fastpath / slowpath 分层（D4 无悔项，设计先行——现在分层，避免后补）**：channel `send` 走 seL4 式**检查链**——「无 fault 挂起、单次 handle 查找、接收方已在 recv 等待、消息 ≤ inline 上限、同核」全过 → **直切专用短路径**（无队列、无锁竞争、零动态分配）；任一条件不满足 → slowpath（排队 / badge 分发 / 跨核唤醒 / 大消息走共享 VMO）。
  - **验收线**：x86_64 同核跨地址空间往返 **<1200 cycle**（A57 档 <500）——seL4 档（986），不是 Zircon 档（8157，Mi et al. 2019；Zircon 量级 fastpath 会让 x86 数据面整机吞吐只剩约 2/3，LionsOS 论文实测，12号 §4.2）。
  - **intent 传播不破线（ADR #42）**：`msg_intent` 头加入检查链后 fastpath 增量 = 一次 u8 cmp+sel（零记账，结算全 slowpath）——验收线保持不变；P4 实现时以实测复核。
  - **分层不是特例**：slowpath 是 fastpath 的一般化，两者语义逐条对齐（同一消息可见性/顺序/错误码），不许长出第二套语义。
  - I/O 数据面（块/网络/图形）不走 channel 逐消息，统一 **SPSC 无锁共享环 + 批量描述符 + 单次通知**（LionsOS 模式，12号 §10.5）——与 PM 环同哲学。
  - seL4 检查链仅作思路参照（GPL，不复制代码）。

## 8. 参照

- 同步端点 + badge：seL4 endpoint、Zircon channel。
- 命名空间：Plan9、Fuchsia namespace。
- 落 `参考源码/`：Zircon（BSD/MIT，channel/namespace 可细看）、seL4（endpoint 机制）。

## 阶段化目标（v1 基线，对照 `架构决策.md` #20）

- **v1**：同步 Channel（`send/recv` + handle 迁移）+ Event 异步通知；**先用纯能力传递**做服务互联（不经 namespace）；TCB 双优先级字段随 Phase 2b 预留（§2.1 PI 前置）。（G0–G2）——**非阻塞有界队列已落地**（3-4b：空 recv -EAGAIN / 满 send -EBUSY，深度 8 / inline ≤256B / 随行 handle ≤4；阻塞等待 + PI + fastpath 随 P4）。
- **v2**：共享 VMO 零拷贝；futex 用户态同步。（G2–G3）
- **v3**：路径式命名空间（namespace 服务 + mount）——作为可用性层后加；badge；**优先级继承语义随 P4 channel 落地（§2.1）+ fastpath/slowpath 分层（§7）**。（G3+）

### Phase 4 拆步（2026-10 计划）

前置设计对齐（三条，实现期不变量）：

1. **阻塞语义统一收敛到 `object_wait` 面**：`channel_send/recv` 原语本身保持非阻塞（ABI 签名冻结无 flags 位）——阻塞 = 非阻塞原语 + `object_wait` 可读/可写信号（Zircon「非阻塞 channel + wait READABLE」同形；§8.3 recv 注释已预埋「等待排队留 P4 object_wait」）。
2. **deadline 全系绝对时刻**（MONOTONIC 纳秒；`0` = poll 立即查——与 3-4c 语义无缝衔接；无「无限等」哨兵，v1 客户端传 `clock_read()+δ`）。`thread_sleep`(0x37) 同口径。
3. **睡眠切换的 PSTATE 承接沿用现有协议**：切换恒在屏蔽窗（`context_switch` #Safety 契约）、各续行点自理解蔽（首跑 trampoline `daifclr` / yield 出口 `enable_irq` / 异常帧 eret）——`block_on` 续行段屏蔽续行到 SVC eret 自然解蔽，**零新增协议**。

| 步 | 内容 | 验收 |
|----|------|------|
| **4-0** | 睡眠/唤醒原语（机制层）：`TaskState::Blocked` + 侵入式 `WaitQueue`（Task 预埋双向链接字段，零分配；按 `prio_effective` 插入——§2.1/§3.6 PI 前置，同优先级 FIFO）+ 受控睡眠 `block_on`（屏蔽窗内「验条件→入队→Blocked→切走」；唤醒方同窗「改条件→摘队→置 Ready 入 runqueue」——单核屏蔽窗原子 = 无丢唤醒/虚假唤醒，SMP(G2) 重做）+ tick 扫超时（粗粒度，tickless 精化留后）+ `thread_sleep`(0x37) 最小消费者 | 内核任务 sleeper/waker 对：超时自醒 + 被唤醒双路径（ktest 在 `sched::init` 前跑不能睡——验收走 boot spawn 对打 `BLOCK ok`，smoke 第 14 标记） |
| 4-1 | 阻塞 `object_wait_one`（deadline≠0 绝对时刻阻塞；命中回写全满足集、超时 `-ETIMEDOUT`）；`object_signal` 遍历唤醒等待者 | EL0 用户进程 wait 超时/命中双路 |
| 4-2 | wait 面 ABI v0：信号位空间（低 32 位内核对象信号 READABLE/WRITABLE + 高 32 位用户——Zircon 界；Event 用户位迁移核对）+ wait_one 扩 Channel（队列非空=READABLE / 未满=WRITABLE）+ `wait_many`(0x05) | 两进程阻塞链：send 满 → wait WRITABLE → 对端 recv 腾位唤醒 |

> **4-2a 已落地（2026-10）**：信号位空间 = `SIGNAL_READABLE`(1<<0)/`SIGNAL_WRITABLE`(1<<1) 内核派生 + `SIGNAL_USER_MASK`(0xFFFF_FFFF_0000_0000) 高半用户位（`object_signal` clear/set 越出用户半 → -EINVAL，检在 get_raw 后——坏 handle/类型仍 -EBADF 优先；Event 用户位/ktest I 段/user_waiter/waker2 全量迁高半）。wait_one 扩 Channel：`signal_state` 统一查询点（Event=用户位掩码；Channel=READABLE(本端队列非空)|WRITABLE(对端队列未满) 内核派生）；唤醒点 = send 成功 `wake_matching(对端, READABLE)` + recv 成功 `wake_matching(对端, WRITABLE)`（仅成功路径——回滚路径状态复原不唤醒）。验收 = user_pinger（endA RIGHT_SEND）send 灌到 -EBUSY → wait WRITABLE 阻塞 → user_ponger（endB RIGHT_RECV）recv 腾位唤醒 → 补发成功打 `CHAIN ok`（smoke 第 16 标记；**EBUSY 驱动**不赌精确计数）；ktest I6 低半拒面 + I7 Channel poll 面全套。wait_many(0x05) 留 4-2b。

> **4-2b 已落地（2026-10，4-2 收官）**：`object_wait_many`(0x05)——n∈[1,16] 任一命中出 `*idx`(入参序最前命中项)+`*observed`(该对象全满足集)；poll/-ETIMEDOUT 口径同 wait_one；**WAIT_ANY 哨兵**（`berry_sched::WAIT_ANY`——Task 只有单一 wait_obj 匹配键，多对象等待者挂哨兵，`wake_matching` 谓词加哨兵分支：任何对象的唤醒都命中，醒来重验兜底；v1 等待者个位数误醒开销可忽略）；两数组先整段 copy_from_user 拷入内核栈再解析（无 TOCTOU）；**钉住纪律多数组版**——全部对象各 retain 一次、唯一出口逐个 release。ktest I8 错误面+poll 面（n=0/17 -EINVAL、坏 handle 首错 -EBADF、poll 未命中 -EAGAIN 且 idx 留哨兵 u64::MAX、远古 deadline -ETIMEDOUT 不睡、命中 idx 两向活证）。验收 = ponger 稳态等待改 wait_many n=2（[endB:READABLE, boot 哑 Event:1<<32 永不 signal]）——命中须 idx==0（哑 Event 位永不置、idx==1 即断链自陷），首次打 `MANY ok`（smoke 第 17 标记；哑 Event handle 0x0001_0002 boot 断言定值）；ktest B 段未实现样本 0x05→0x33。syscall 23→**24** 号。
| 4-3 | PI 捐赠 + intent 五件套（§2.1/§3.6：wait 阻塞方捐赠给解锁方、链式 ≤8 层不 panic、解除即回落；`msg_intent` 消息头位布局随本步 ABI v0 定、slowpath 记账、每跳预算帽 + 累计时间封顶、channel opt-out；boost 即按 effective 重排 runqueue） | 三任务链（高优 client → 慢 server ← 低优干扰者）client 唤醒延迟上界（G 卡用例） |

> **4-3a 已落地（2026-10）**：`RoundRobin::pick_next` 改按 `prio_effective` 出选（两圈扫描 + 同高 FIFO + 选中后 advance_past；同 eff 层内轮转、跨层严格优先）——「boost 即按 effective 重排 runqueue」的消费点先行就位（`调度器设计.md` §3.6 v1 兑现），PI/AI 权重共用。POLICY C 用例断言升级为严格优先。

> **4-3b 已落地（2026-10，PI 捐赠——`内核/src/pi.rs`）**：**捐赠边靠 handle 表反查**（QNX「send/recv 双方同睡一个 channel」被 BerryOS 有界队列解耦——接收方持 peer 端点 handle、可阻塞在任何对象上）：donor = SLEEPQ 上睡**单一 channel** 等**内核派生位**（READABLE/WRITABLE）者；受捐 = 同在队、其 handle 表持 peer 端点且带对应**解锁 right**（等 WRITABLE 需 RECV、等 READABLE 需 SEND、多持有者全捐）的任务（`holder_has` 反查；内核任务 proc_tag=0 → 内核侧共享表 = 所有内核睡眠者都算持有的 v1 粒度，EL0 每进程表才反查到人）；WAIT_ANY 哨兵 / Event 等待 / 纯 sleep / 只等用户位均无边（诚实缺口：Event 捐赠 v2 再议）。**全图重推、零增量簿记**：SLEEPQ 变更（block 睡下后 / wake 摘队后，`set_sleepq_hook`）+ handle close 后从零推导——清零在队者 → 不动点迭代 ≤8 轮（每轮传播一跳；**d_eff 实时读 `pi_inherited`**——只读尾趟才刷的 effective 会把链截死一跳）→ 尾趟刷 effective；持有环 max 单调有界不发散。**冻结语义 = 「server 处理消息期间持有」的 v1 形态**：重推只清/算在队者，受捐方唤醒离队时冻结 `pi_inherited` 随行（boost 带进 runqueue，4-3a pick 消费），下次入睡被重推清零回落（=「reply 即回退」五件套之 1 的无 reply 等价物）；长跑不睡者 boost 挂着（QNX 语义本就如此，v1 接受）。**连带不变量**：请求位含内核派生位 ⟹ wait_obj 必须是真 channel ObjRef——4-0 合成对象 SLEEP_OBJ 曾以 `!0` 睡（含派生位）被 PI 图解引用炸对齐 panic（run 实跑抓到），改等用户位 1<<33。验收 = pi_client(32)/pi_server(0)/pi_probe(0) 三内核任务全真 SVC 对打（probe 断言 server pi==32 且 eff==32 后 signal 放行打 `PI ok`，smoke 第 18 标记）+ ktest J 段 `PI-K ok` 八用例（单跳判别/rights 门控两向/链式两跳/持有环收敛/11 节点链轮帽截断（倒序入队每轮恰一跳：L8 达、L9/L10 超帽落 0）/哨兵与 Event 跳过/冻结随行 + 重入队回落/共享表粒度含自捐无害面）。**G 卡用例的 client 唤醒延迟上界验收留正式关卡**（本步验收 = 字段断言 + 功能闭环）。intent 五件套（msg_intent/slowpath 记账/预算帽/opt-out/通胀计数器）留 4-3c。

> **4-3c 已落地（2026-10，intent 五件套——`进程调度/src/intent.rs` + channel/send-recv 全链，4-3 收官）**：**编码冻结**（berry_intent 随 ADR #42：0x00 UNSPECIFIED=不设限为顶（XNU 式「未声明 ≠ 声明低档」，未声明任务发消息 = 借出最强档的保守方向）/0x01 LC/0x02 交互/0x03 UTILITY（推断缺省）/0x04 BG/0x05 MAINTENANCE 冻结不启用；0x06..0x0F 保留拒绝——`intent_valid` 判别先冻结、入口治理随声明入口 syscall）。**数值约定 u8 越小档越高 → 「max(own, 借来的)」数值上即 min**。**传播机制**：消息头 `Message.meta = MsgIntent{intent, hop, acc_ns}`（内核可见元数据槽，用户不传参、recv ABI 不回写）；send fastpath = 一次结构体快照（未借入 → Fresh(declared,0,0)；借入且持有 ≤ 预算 → Forwarded(eff, hop+1, acc+持有段)；持有 > 每跳预算 → Capped 降级(declared,0,0)）——**零记账，结算全在 recv slowpath**（每消息重导 `eff = min(declared, msg_intent)`；借入生效记 TCB `intent_hop/since/acc` 作转发快照与时间帽数据源；**非单调累积** → QNX MsgReply 粘性膨胀机制上不可能；两消息之间保持 = 「server 处理该消息期间持有」，与 4-3b PI 冻结随行同义）。**三道帽**（超顶一律降级为不传播、绝不 panic）：消息链跳数 `INTENT_HOP_MAX=8`（与 PI 阻塞链轮帽 8 数值巧合、语义独立）；每跳预算 `INTENT_HOP_BUDGET_NS=50ms`（借入方持有超预算 → 转发快照降级为自己的 declared，**借入字段本身保持**——转发降级≠自身回落）；链累计 `INTENT_CHAIN_BUDGET_NS=400ms`（消息头 acc 超值 → 结算侧拒传播）。**opt-out**（五件套之 4）：`channel_create` 改 options 首参（Zircon/event_create 同形，ABI §8 修正），bit0 = `CHAN_OPT_NO_INTENT`（QNX `_NTO_CHF_FIXED_PRIORITY` 等价）→ 置位端 recv 不结算且回落 declared（含清掉他途借入）；对称建对两端同置；其余位 -EINVAL。**通胀计数器**（五件套之 5 的可观测半）：telemetry 三计数器 `intent_borrows/hop_capped/budget_capped`（IRQ 屏蔽静态计数零分配）+ 三条 uart trace 行；共享内存 VMO 不自动传播 = 机制上免费（msg_intent 只随消息走）。**v1 无调度消费点**：字段+传播机制落地，IntentProfile→latency-nice/EPP 第一消费者随 EEVDF/v2（「查表键先于消费者落地」= ADR #42 第一段形状）；既有矩阵零行为漂移（全员 declared 0x00 → min(0x00,0x00) = Own 不计数）。**ABI 迁移面**：`channel_create(options, *out0, *out1)`（regs[1..3]，不新增编号）；ktest H/F 段 3 处改参数序、spawner 模板 +1 条指令（adr 自适应安全）。验收 = boot 六内核任务全真 SVC 对打（driver(0x02) 三发 → mid(0x04) recv 借入 0x02/借入转发 hop+1/opt-out 回落 → sinkA(0x03) 收转发件 eff==0x02 且 hop==1（**mid 借入转发的活证**）→ slow(0x04) 借入后睡 200ms 超预算、send 降级装自己的 0x04 但借入保持 → sinkB(0x03) 吃两条 boot 伪造超帽消息（hop=8 恰顶/acc=400ms+1 恰超）双帽拒传播 → probe 睡 8 slice 遍历 SLEEPQ 断言四睡眠者字段 + 三计数器（borrows≥3/hop≥1/budget≥2）打 `INTENT ok`，smoke 第 19 标记）+ ktest K 段 `INTENT-K ok` 十用例（K0 编码域/K1 首手快照/K2 转发快照 hop+1 acc 累加/K3 send 侧预算帽/K4 借入生效记三源字段/K5 未声明借入最强档+弱消息每消息重导回落/K6 opt-out 清他途借入/K7 hop=8 拒 7 放行/K8 链封顶 >拒 =放行/K9 SVC options 面 bit0 可查 bit1 起拒）。**伪造消息绕过 send 直压收端队列** = 超帽样本确定性触发（boot 单线程队列必空不满）。
| 4-4 | fastpath/slowpath 分层（§7 检查链：队列空 + 对端已在 wait → 直切投递；两路语义逐条对齐，不长第二套）+ cycle 实测（QEMU TCG 先出量级、真机复测标注） | 验收线 <1200/<500 cycle 实测复核；intent cmp+sel 增量不破线 |
| 4-5 | 服务框架（用户态）：init/svcmgr 骨架 + 服务循环（wait READABLE → recv → 处理 → reply）；纯能力互联（3-4a bootstrap 迁移已有，namespace 留 v3）；调试槽 0xFE/0xFF 退役（user 矩阵改走真 IPC） | 里程碑：服务管理器拉起一个服务 |
| 4-6 | Phase 4 出口：两用户态服务交换消息全链演示 + AI 最小演示（**滑移出口**：IPC/调度未稳则顺延 P5 首项，不重开 P4） | smoke 全绿 + 演示串口活证 |
