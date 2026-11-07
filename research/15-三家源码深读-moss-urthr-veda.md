# 三家源码级深读：moss EEVDF/锁纪律 · urthr 板级驱动 · Veda BKL-MCS

> 调研日期 2026-11-07 · 方法：三家完整 clone（moss-kernel 12M / urthr 8.2M / Veda 44M，depth 50），源码逐文件实读；对照基准 = BerryOS 当日 HEAD（`sched/src/class.rs` S2a 带内 EEVDF、`arch/aarch64/src/gic.rs`）· 姊妹篇：12/13/14 号
>
> 本文为 15 号专题。**许可修正（重要）**：Veda 经源码 SPDX 头实证为 **GPL-3.0-only**（`bkl.rs` 头部）——此前按 README 判断为宽松系是误判；BerryOS（Apache-2.0）对 Veda 执行 GPL 纪律：**机制语义可参照、代码一字不进**（规则 13 同口径）。

## 1. moss EEVDF vs BerryOS S2a——两种实现的逐点对照

moss 调度器（`src/sched/`：runqueue 254 行 + vclock 47 行 + sched_task）与 BerryOS（`sched/src/class.rs` 349 行）同代同算法、结构选择全面分叉：

| 维度 | BerryOS S2a | moss | 判读 |
|---|---|---|---|
| lag 时钟 | `avg_vruntime` 每次入队 O(n) 重算（`vruntime <= avg` 近似 eligible） | **集中式 VClock**：`v += (Δt << 65.63定点)/Σw` O(1) 增量累加 | moss 的增量时钟值得借鉴——BerryOS 可维护运行和 Σvruntime/Σw 增量，把入队的 O(n) 摊成 O(1)（S2b+ 优化议题） |
| 数据结构 | 单游标有序环（O(n) 扫、零分配、同键 head FIFO） | **双 BinaryHeap**（Ineligible 堆 by v_eligible + Eligible 堆 by deadline，O(log n)） | 嵌入式/移动任务数（几十级）下环的 O(n) + 零分配更优；大规模任务 moss 优——两者各自正确 |
| 定点 | u64 + SCALE | u128 + 65.63 格式 | 同思路不同位宽 |
| 防饿死 | 带内无 eligible 退化最小 `(vdeadline, vruntime)` | **VCLOCK_EPSILON 容差**（`v_eligible - clk <= ε` 即 eligible） | 互证：两种工程化都成立 |
| 入队对齐 | 抬到 avg_vruntime（防睡眠 lag 攒积） | `v_eligible = vclock` + 按 weight 定初 v_deadline | 等价物互证 |
| 归还片 | yield 走既有路径 | **`yield_slice`：`v_eligible = max(v_eligible, v_deadline)`** 显式归还未用片 | moss 的显式归还语义干净，BerryOS 可对照自查 yield 路径是否有等价物 |
| deadline 类 | S5 手动绝对 vdeadline + 周期重臂 + lazy unthrottle | 无（Linux 单树语义） | BerryOS 的 deadline 面是超出 moss 的自有增量 |

## 2. moss 编译期锁纪律的实现真身

`libkernel/src/sync/spinlock.rs`：`SpinLockIrqGuard` 含 **`PhantomData<*const ()>`（注释明示 `// !Send`）**——guard 自动 `!Send`；async fn 的 future 跨 `.await` 持有 guard 即 `!Send`，在要求 `Send` 的执行器上**编译失败**。这是 std::sync::MutexGuard 的经典手法被升格为内核纪律基石。**代价结构**：整套内核 async 化（waker/condvar-future/mpsc 异步原语全套）换一类死锁编译期消灭。

与 BerryOS 的对照（执法哲学表）：

| | moss | BerryOS |
|---|---|---|
| 违规发现时机 | 编译期（拒绝编译） | 运行期（lockdep VIOLATION 计数 + smoke 断言） |
| 代价 | 全内核 async 化（执行器 + 原语栈） | lockdep 基建 + 违规可能漏到运行时 |
| 适用前提 | syscall 天然 async 化 | 全同步 no_std 内核 |

BerryOS 不采纳 async 内核（同步内核 + 细粒度锁是既定路线），但「类型系统消灭违规类别」的思路已在吸收路线图 P2 挂议题（14 号 §3）——源码实证其实现成本后，该议题的可信度更高。

## 3. urthr 板级驱动——两件体量实测

- **`gicv2.zig` 极薄**：setBase（GICD/GICC 偏移 0x1000/0x2000）+ initGlobal（验 implementer 0x43B → 全禁 → 逐 SPI 电平 + target CPU0）——与 BerryOS `gic.rs` 同型同量级。**证实 RPi 适配的 GIC 面成本 = 改 base + 中断拓扑**（14 号判断再获源码级确认）。
- **`sdhc.zig` 2,358 行 + 板级 `sdio.zig` 238 行**：完整 SD 主机控制器（Capabilities 探测/CardInfo/块设备 vtable/DMA/分块读写）——**P0-a 台阶三"SD rootfs 通道"工作量的实证锚点 ≈2,600 行**（此前 proposal 只有定性描述，现在有量级）。

## 4. Veda BKL-MCS——122 行与三个精妙细节

`kernel/source/synchronization/bkl.rs`（GPL-3.0-only，只读）：

- 经典 MCS：每 CPU 一个 `#[repr(align(64))]` 的 `Waiter{next, waiting}`（各旋各的 cache line，只被前一 CPU 写一次）；TAIL/OWNER 各占独立行。
- 细节一：**等待 BKL 期间继续服务 TLB shootdown 请求**——否则持锁者等 shootdown 应答、等待者等锁，死锁。
- 细节二：递归获取直接 panic（debug_assert 关中断持有）。
- 细节三（注释里的教训）：**为什么不是 ticket 锁**——ticket 全员旋同一计数器，每次交接全员缓存失效，CPU 越多交接越慢直到交接时间超过持锁时间；MCS 交接代价与等待数无关。
- 「为什么不是 BKL」ADR（吸收路线图 P2）的证据面补全：BKL 的真实代价 = 内核单线程化（窗口系统每帧、驱动 VM 每次 exit 都排队）；收益 = 并发正确性证明面极小（122 行 vs BerryOS 细粒度锁全树 + lockdep）。

## 5. 源码级启示汇总（喂吸收路线图）

1. **EEVDF 时钟增量化**（moss VClock）：BerryOS `avg_vruntime` O(n) 重算 → 维护 (Σvruntime·w, Σw) 增量和，入队 O(1)——挂 S2b+ 优化议题（P1 池候选）。
2. **yield 片归还语义**（moss `yield_slice`）：对照自查 BerryOS yield 路径（随调度域下一刀顺手）。
3. **SDHC ≈2,600 行实测**：更新 P0-a 台阶三工作量预估（proposal 前置雷 4 的量化补强）。
4. **「为什么不是 async 内核」ADR 候选**：与「为什么不是 BKL」并列——BerryOS 同步内核路线的两条主要岔路的否决证据都在库了。
5. **Veda 许可修正传导**：吸收清单中 Veda 两项（consent-by-identity、三动词）降为**纯语义参照**，永不落代码；12 号无错误表述（许可误判仅存在于对话），无需回改。

## 参考

- moss：`src/sched/runqueue/{mod,vclock}.rs`、`libkernel/src/sync/spinlock.rs`（clone /tmp/os-src/moss-kernel）
- urthr：`urthr/arch/aarch64/gicv2.zig`、`urthr/dd/sdhc.zig`、`urthr/board/rpi5/dd/sdio.zig`
- Veda：`implementation/kernel/source/synchronization/bkl.rs`（SPDX: GPL-3.0-only）
- BerryOS 对照基准：`sched/src/class.rs`（S2a）、`arch/aarch64/src/gic.rs`
