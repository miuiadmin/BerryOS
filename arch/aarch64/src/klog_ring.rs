//! klog 字节环（启动产物计划第 16 步）：内核串口流（UART0 TX）的 per-CPU
//! 镜像环——0x84 `telemetry_open` klog 快照 op 域的数据源，客机自读面
//! （bsh `dmesg`）的底层。
//!
//! 设计裁决（四轮审计钉下的硬前提）：
//! - **每核独立环 + 本核写 + 快照读**——`kernel/src/telemetry.rs` 的
//!   per-CPU 环先例（SWITCH_RINGS），但本环是**字节流**（串口镜像）：
//!   每环静态定长 16KiB 字节 buf + AtomicUsize 单调写游标
//!   （`fetch_add` 分配槽位、追尾覆盖）+ seqlock（快照读一致性）。
//! - **写点挂 [`crate::uart::uart_putc`]**（HAL 发射面单源——与串口同序
//!   全量覆盖，`uart_puts`/`uart_write_txff` 逐字节经此）。fetch_add 无锁
//!   分配天然覆盖 panic 绕锁路径（panic 直写 UART 也入环）。boot asm 的
//!   三字符早期标记在环外（开 MMU 前直写 PA，不走 uart_putc——已知口径）。
//! - **同核嵌套写者安全**（任务态打印被 IRQ 打断再打印）：fetch_add 各得
//!   各槽（指标不撕）；seqlock 嵌套退化为「内层完整完成后外层收口」——
//!   IRQ handler 在外层续行前完整返回，末位 even store 晚于两者数据写，
//!   读者要么看到两者、要么重试，无撕窗（telemetry 单写者前提的本环放宽
//!   论证）。
//! - UART1（sim 设备注入通道）不入环——klog 环镜像 console 串口
//!   （serial0），设备通道归其自有观测面。

use core::sync::atomic::{AtomicUsize, Ordering};

/// 单环容量（静态定长 .bss——boot 后永不增长；审计裁决的数 KiB 级行/字节
/// 帽量级，取 16KiB：裸相 boot 全程日志可整装）。
pub const KLOG_RING_SIZE: usize = 16 * 1024;

/// 环数 = NCPUS（2 核，telemetry.rs NCPUS 同款；拓扑真化后随扩）。
pub const KLOG_NCPUS: usize = 2;

/// 单核字节环：写方 = 本核 uart_putc（fetch_add 分配）；读者任意核按
/// 热尾丢弃容错窗读（best-effort 快照——见 snapshot_into 注）。
struct KlogRing {
    /// 单调累计写字节数（fetch_add 分配；`pos % SIZE` 为槽位，追尾覆盖）。
    pos: AtomicUsize,
    /// 环槽——槽位由 fetch_add 独占分配，写方互不覆盖。
    buf: [u8; KLOG_RING_SIZE],
}

impl KlogRing {
    const fn new() -> Self {
        Self { pos: AtomicUsize::new(0), buf: [0; KLOG_RING_SIZE] }
    }
}

/// per-CPU 环（静态预储；`static mut` + 本核写 = 全仓内核静态惯式）。
static mut KLOG_RINGS: [KlogRing; KLOG_NCPUS] =
    [KlogRing::new(), KlogRing::new()];

/// 单字节入本核环（uart_putc 挂点；任何上下文可调——任务/IRQ/panic 绕锁
/// 面，全程无锁）。槽位 fetch_add 独占分配、数据普通 store——无 per-char
/// seqlock（首版 seqlock 包络在稳态矩阵高写压下读者 64 次重试恒撞写窗
/// 饿死返 0，客机 dmesg 实跑实抓；分配-落字间的指令级嵌套窗由读者的
/// 热尾丢弃吸收——dmesg best-effort 语义）。
#[inline]
pub fn put(c: u8) {
    let cpu = crate::smp::cpu_id().min(KLOG_NCPUS - 1); // 拓扑槽外防御：收敛尾槽
    // SAFETY: 写方限本核——本核槽无他核写者；槽位独占分配互不覆盖。
    let r = unsafe { &mut (*core::ptr::addr_of_mut!(KLOG_RINGS))[cpu] };
    let slot = r.pos.fetch_add(1, Ordering::Relaxed) % KLOG_RING_SIZE;
    // SAFETY: 槽位由 fetch_add 独占分配（同核嵌套写者也各得各槽）。
    unsafe { (*core::ptr::addr_of_mut!(r.buf))[slot] = c };
}

/// 快照读：本核环内**最新** `out.len()` 字节按旧→新序拷入 `out`，返回实拷
/// 数。热尾丢弃容错窗：pos 快照减去 `HOT_TAIL`（分配了但字节可能未落的
/// 指令级嵌套窗上界）后整窗拷贝、无版本校验——首版 per-char seqlock 读者
/// 在高频写流下 64 次重试恒撞写窗饿死返 0（客机 dmesg 实跑实抓），dmesg
/// 语义本就 best-effort，撕裂最坏是读到旧环残值。
pub fn snapshot_into(cpu: usize, out: &mut [u8]) -> usize {
    /// 热尾丢弃量：远大于任何现实同核嵌套写者深度（IRQ 在 fetch_add 与
    /// 数据 store 间被打断的窗口是几条指令）。
    const HOT_TAIL: usize = 128;
    // SAFETY: 只读；写方限本核、槽位独占分配。
    let r = unsafe { &(*core::ptr::addr_of!(KLOG_RINGS))[cpu.min(KLOG_NCPUS - 1)] };
    let pos = r.pos.load(Ordering::Acquire);
    let avail = pos.min(KLOG_RING_SIZE);
    let take = avail.saturating_sub(HOT_TAIL).min(out.len());
    if take == 0 {
        return 0;
    }
    let start = avail - take; // 最旧待拷位（热尾丢弃后的有效沿）
    // SAFETY: 只读 buf；窗口内槽位在 pos 快照前已分配且大概率已落字。
    let buf = unsafe { &(*core::ptr::addr_of!(r.buf)) };
    for i in 0..take {
        out[i] = buf[(start + i) % KLOG_RING_SIZE];
    }
    take
}
