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
- **快路径**：短消息缓存行级、无通用堆拷贝；handle 迁移 O(1)。全可抢占下保持临界区短、锁粒度细。

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

## 8. 参照

- 同步端点 + badge：seL4 endpoint、Zircon channel。
- 命名空间：Plan9、Fuchsia namespace。
- 落 `参考源码/`：Zircon（BSD/MIT，channel/namespace 可细看）、seL4（endpoint 机制）。

## 阶段化目标（v1 基线，对照 `架构决策.md` #20）

- **v1**：同步 Channel（`send/recv` + handle 迁移）+ Event 异步通知；**先用纯能力传递**做服务互联（不经 namespace）。（G0–G2）
- **v2**：共享 VMO 零拷贝；futex 用户态同步。（G2–G3）
- **v3**：路径式命名空间（namespace 服务 + mount）——作为可用性层后加；badge。（G3+）
