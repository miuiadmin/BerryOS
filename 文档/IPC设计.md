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
- **msg_intent: u8（内核可见元数据槽，ADR #42）**：消息头携带发送者 `intent_effective` 快照；server dequeue 时 `intent_effective = max(own, msg_intent)`、reply 即回落（传播五件套见 §2.1）；channel 级 opt-out 标志可豁免。位布局随 P4 ABI 草案 v0 定（当前占位）。
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
- **阶段化**：v1 只预留 TCB 双优先级字段（成本≈0，随 Phase 2b 表示法字段落地）；语义实现随 P4；验收 = 三任务链（高优先级 client → 慢 server ← 低优先级干扰者）下测 client 唤醒延迟上界（G 卡用例）。

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
