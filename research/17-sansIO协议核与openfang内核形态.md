# swift-os sans-IO 协议核与 openfang 内核形态（第六层）

> 调研日期 2026-11-08 · 方法：swift-os `kernel/net/`（3,572 行 Swift）逐文件头注与结构实读；openfang（18,242★ Agent OS 头部）`crates/` 14 crate 结构实读 · 姊妹篇：12–16 号
>
> 本文为 17 号专题。

## 1. swift-os sans-IO 协议核——教科书级分层（Apache-2.0，语义可借）

### 1.1 三层分法（`stack.swift`/`socket.swift` 头注实读）

| 层 | 文件 | 职责 | 测试面 |
|---|---|---|---|
| **纯逻辑引擎** | `stack.swift`（NetStack） | 一次消费一帧 → 回复写调用者缓冲，**零 I/O、每包零分配**（唯一堆用 = 固定 8 项 ARP 缓存，控制面） | ✓ 构造字节直喂 |
| **值语义状态机** | `tcp.swift`（TCPConnection） | 消费解析后段字段 + **单调 now tick** → 出站段**描述符**进固定小队列由调用者排空；无 I/O 无内核态 | ✓ 同上 |
| **IO glue** | `socket.swift`（1,323 行） | own NetStack、泵 NIC、demux、syscall 边界拷一次 | ✗ **明示不在 host 测试面** |

### 1.2 四个可借手法（对 BerryOS 用户态网络栈 P6-A8 的输入）

1. **时间外置**：TCPConnection 的 RTO/重传全部由注入的 `now` tick 驱动——时间依赖逻辑的确定性测试关键（AI 测试友好化「同输入同输出」在网络面的实现形态）。
2. **零拷贝边界**：`onFrame` 输入直指 RX DMA、输出直指 TX DMA；UDP 接收环持 RX 缓冲**引用**（不拷贝），syscall 边界才拷一次。
3. **每包路径零分配纪律**：数据面无堆；堆只在控制面（ARP 缓存）。
4. **scope 诚实清单**：TCP 头注明列 in-scope（被动/主动开、in-order+累计 ACK、单定时器 RTO、固定窗口、完整关闭、RST）与 **out-of-scope**（乱序重组=丢弃+重 ACK、delayed ACK、Nagle、拥塞控制、SACK、时间戳）——「最小但真」的边界写法值得 BerryOS 设计文档效法。
5. **量级锚**：最小真 TCP/UDP/IP(v4+v6)/ICMP×2/ARP/DHCP/DNS + socket 层 ≈ **3,572 行**。

## 2. openfang 内核真身——应用服务器内核（生态位确认）

`crates/openfang-kernel`（模块实读）：agent 生命周期/内存/权限/调度/agent 间通信——具体模块 = approval（审批）/auth/**capabilities**/cron/heartbeat/**metering**（计量）/**supervisor**（监督）/triggers/event_bus/workflow/**whatsapp_gateway**。14 crates 总形态 = **聊天 agent 应用服务器**（含桌面/CLI/extensions/migrate），非操作系统内核。

**对 BerryOS 的生态位结论（24 号（原 12）判断的源码级确认）**：

- 2026 Agent OS 头部的真实内核 = **进程监督 + 审批 + 计量 + 触发器**的用户态模拟——它们在通用 OS 上用应用层重建「操作系统本该提供的服务」。
- 对照映射：openfang 的 scheduler/cron/heartbeat ↔ BerryOS **intent 表达 + 调度带内接口**；metering ↔ BerryOS **遥测环 + per-process stats 账**；approval/capabilities ↔ BerryOS **SET_POLICY 能力委托 + 同意面（P1-c）**——BerryOS 是这些需求的内核级原生供给者，openfang 们是它的（未来的）用户态客户形态。
- 三层收敛再证：openfang approval / Veda consent / astrid ACL 同型——「agent 权限面」是 2025–26 全行业的收敛点，BerryOS 把它做进内核是稀缺位置。

## 3. 汇总输入

| 输入 | 去处 |
|---|---|
| sans-IO 三层分法 + 时间外置 + 零拷贝 + scope 清单 | 用户态网络栈（P6-A8 线）设计输入——协议核纯函数化与 AI 测试友好化对齐 |
| 最小网络栈 ≈3,572 行量级锚 | P6-A8 工作量预估参照 |
| openfang 生态位确认（应用服务器内核） | AI 调度带内接口的下游客户画像（v3+ 叙事：BerryOS 原生供调度/计量/审批面） |

## 参考

- swift-os: `kernel/net/{stack,tcp,socket}.swift`（Apache-2.0）
- openfang: `crates/openfang-kernel/src/*`（MIT/Apache 双许可）——模块清单实读
- BerryOS 对照：`P6-A8网络服务化设计草案-v1.0.md`、AI 调度设计 §3/§4、调研 24 号 §1
