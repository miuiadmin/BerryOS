# zero_frame 专用清零臂：DC ZVA 主档 + GPR 回退

Status: implemented

## Problem

`zero_frame`（`kernel/src/vmo.rs`）是全内核唯一整页清零入口（vmo commit 页、vmar `new_root`、`next_table` 三消费点，均 fresh-from-buddy），却走通用 `core::ptr::write_bytes` → 本仓 memset（`kernel/src/mem.rs:35`）＝ 8B GPR 循环 512 迭代/页——旧 icount 基线 vmo_commit1p=1683 里清零占大头（`research/内核深度进化/01-内存管理.md` 机会 1，Top-1），一切 commit、ZFOD 缺页供给、建表、进程组装路径同付此税。

## Decision

vmo.rs 内加 `zero_page_4k` 专用臂（`zero_frame` 唯一调用点）：主档 DC ZVA（Data Cache Zero by VA，ARMv8 系统指令，按缓存行零化——64B 行下 64 指令零完 4KiB）；回退档 GPR（旧 `write_bytes` 路径原样）。启用条件两道：CTR_EL0.DZ（bit 26）＝1（0 时 DC ZVA 是 UNDEFINED）；行大小 4 << CTR_EL0.DminLine（bits[19:16]，Linux `dminline` 同式）整除 4096。探测结果惰性缓存于 `ZVA_PROBE: AtomicUsize`（0 未探测/`usize::MAX` 不可用/行字节数）——首探落 boot 单线程窗（首个 `zero_frame`），SMP 后并发首探是良性竞态（同型 CPU 写同值，Relaxed 足够）。`write_bytes` 通用路径与 memset 合同面零触碰。

正确性前提三件，`zero_frame` 调用点结构性成立：帧独占无别名；页 4KiB 对齐 + 行整除 4096（探测门）→ 行序列恰覆页内不越页；Normal WB + inner-shareable 直映（`kernel/src/vmar.rs` 匿名页属性同款）——ZVA 零化至一致性点（PoC），shareable 域内对全体核可见，与 memset 版读写语义逐字节等价。非 SIMD：不碰 FP/NEON 寄存器，内核 SIMD-free 合同（`kernel/src/mem.rs` 头注 + ktest `test_fp` ③-b canary）不破。

## Alternatives considered

- STP 16B GPR 第一档（研究文档机会 1 的起步档）：被 ZVA 档直接覆盖——QEMU virt/cortex-a72 恒 DZ=1、行 64B，GPR 增强档恒不取；双档并存只增代码面，不做。GPR 回退档保留旧 `write_bytes` 形（mem.rs 整数 memset），真硬件无 ZVA 形态可用。
- 探测值挪 boot 显式 init（读一次 CTR 存静态）：拒绝——惰性原子已把 mrs 降到每进程生命周期一次，显式 init 须动 main.rs 引入新初始化序面。
- 清零臂落 arch crate（berry-hal-aarch64）：拒绝——研究文档钉 vmo.rs 为家（消费点唯一、无跨 crate 新面），最小侵入。

## Consequences

icount 通道 vmo_commit1p 1683→396（−76.5%，超出研究文档 −25%+ 预估）；frame_alloc4k 37 逐值不动（buddy 零清零面佐证）；frame_alloc64k 82→37（在册布局骰轨道对回摆）、handle_evt +14、vmo_rw4k −3（tick 相位族，非代码因果）。prove 五跑 verdict=exact max_dev 0.000%（ICOUNT_TOL=0.0 维持），双份基线经 `--trace-dual --update` 一次刷新（on/off 六项 delta 全 +0——结构性零足迹保住），史档与归因见 `tools/bench_baseline_icount.notes.md` 2026-11-17 条。ktest 全绿（FP canary 直验新清零路径）；反汇编核证 `mrs CTR_EL0`/`tbz #0x1a`/`dc zva` 环在位、回退臂 `bl memset` 在位。陷出面（HCR_EL2.TID5 拦 CTR_EL0 读）需在席 hypervisor——本内核裸金属形态不在场；DZ=0 形态由回退档兜底。

## Related

- 机会出处与证据：[01-内存管理.md](../../../research/内核深度进化/01-内存管理.md) 机会 1（Top-1）
- 同批快赢：调度诊断退役一金丝雀与integrity-check降频10000（实施笔未随刀入库，事实承载见 commit 0ea3d11a——sched/src/sched.rs SOAK_PRINT_EVERY 100→10000 + integrity_check 消费点注；沿革[二百九十四](../../../docs/阶段沿革.md)）、[preempt-checkpoint-len门控](2026-11-17-preempt-checkpoint-len门控-SVC尾检查点免锁前置过滤.md)
