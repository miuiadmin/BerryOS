//! S-1 副核 bring-up 基建（`SMP设计草案.md` §3.2 十项 + §4 S-1 行）。
//!
//! 主核侧 [`smp_bringup`]：PSCI `CPU_ON`（S-0 钉板：HVC conduit、
//! fnid `0xC400_0003`、target MPIDR Aff0=1、entry PA、context_id）拉起副核
//! → CNTVCT 截止帽轮询心跳（状态谓词非竞速，4-5a 纪律）→ dump 副核自报
//! → release（store → dsb sy → sev 门铃序）→ 等 hb2 → `SMP ok ncores=2`。
//!
//! 副核路径 `secondary_entry`（global_asm 落 `.text.boot` 低半 VMA==PA——
//! entry PA 直填符号裸值），§3.2 十项：
//! ①入口 EL 分流：EL2 → 复刻 boot.rs 降级段（elr 落回本函数 EL1 续行）；
//!   EL1 直达（默认机型 PSCI 入口）；EL3 不预期 → 直入 park；
//! ②不清 .bss（主核 boot 已清一次——副核再清 = 扬主核状态）；
//! ③`bl enable_mmu`（表 = 主核所建只读消费；TTBR0/1/SCTLR/TCR/MAIR 皆
//!   per-CPU 寄存器，值全核同构）；
//! ④VBAR_EL1 ← 同一向量表（`__vectors` 高半 VMA − 基址 = PA；CPU1 身份
//!   TTBR0 常活，PA 向量开 MMU 前后皆有效）；
//! ⑤CPACR_EL1.FPEN=0b11（同 boot.rs——消 reset 依赖）；
//! ⑥GIC per-CPU：只动本核 GICC（banked）PMR=0xff + CTLR=1——**绝不碰
//!   GICD**（全局；副核重跑 gic::init 的 GICD_CTLR=0 会清光 CPU0 中断）；
//! ⑦SP = 低半 32KiB 早期栈（主核换高半主栈后已弃用——单写者复用）；
//! ⑧稳态 park = `wfi` 真睡（TCG 实证 wfe=立即返回的 hint 忙转——副核
//!   忙转会拖主核 ~40% 吞吐，S-0 B 相对照）；
//! ⑨CNTKCTL_EL1.EL0VCTEN 自开（per-CPU 复位 0——主核开过不影响副核）；
//! ⑩TPIDR_EL1 = 副核 per-CPU块地址（`.bss` 高半；S-3 per-CPU 区的种子）。
//!
//! 打印纪律（bring-up 窗口 UART 单写者）：副核只写内存 + 至多 1 个哨兵
//! 字符 `'s'` 进 UART（PA 直写，入口执行的第一证据）；主核从 CPU_ON 到
//! 心跳观测之间静默，之后才打印——probe 相曾见双写者字节交错撕行
//! （`'S1:s cpuS_1SEC:low'`），本纪律即其修复。
//!
//! 纪律：asm 只用 movz/movk/ldr=（无 ADRP——2b-1 双向坑）；高半取低半
//! 符号地址越 ±4GB，入口 PA 经链接脚本 `.rodata` 指针字 `__smp_sec_entry`
//! 间接递送（唯一已验证解法，S-0 probe A/B/C 三相实证）；报告区/心跳区
//! 全在高半 `.bss`（主核 boot 清一次；副核经 PA = VMA − KERNEL_VA_BASE 写）。
//!
//! S-1 盒子边界：无 KERN_LOCK（S-2）、无 per-CPU runqueue/IPI（S-3）、
//! console/RX 钉 CPU0（SPI_ROUTES target=0x01）、副核稳态零 UART 写。

use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicU64, Ordering};

use crate::{uart_put_dec, uart_put_hex64, uart_puts};

/// 副核自报字段（asm 偏移 ×8 与此结构逐字段对应；编译期钉板见下方 const 断言）。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SmpReport {
    pub entry_el: u64,  // 入口 CurrentEL>>2（默认机型 1 / virtualization=on 2——实测面）
    pub mpidr: u64,     // 入口 MPIDR_EL1 原始值（期望 0x8000_0001：U 位 + Aff0=1）
    pub x0: u64,        // PSCI context_id 回显（S-0 钉板：x0 递送、x1..x3=0）
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub cntkctl0: u64,  // CNTKCTL 复位读（per-core 证据：主核已开 bit1，副核应为 0）
    pub cntkctl1: u64,  // 自开 EL0VCTEN 后读回（期望 bit1=1）
    pub cntv_ctl: u64,  // CNTV_CTL（期望 0=屏蔽——副核无定时器源）
    pub cntvct: u64,    // CNTVCT 直读（全局计数器）
    pub cntfrq: u64,    // CNTFRQ（期望 62.5MHz=0x3B9ACA0）
    pub gicc_ctlr: u64, // 本核 GICC CTLR 使能前复位读（期望 0）
    pub gicc_pmr: u64,  // PMR 写 0xff 后读回
    pub gicc_iar: u64,  // IAR（期望 0x3ff spurious——无 routed 源）
    pub itgt33: u64,    // GICD ITARGETSR[33]（期望 0x01=主核 SPI_ROUTES 所写路由证据）
    pub tpidr: u64,     // TPIDR_EL1 ← per-CPU 块地址写读
}

const fn zero_report() -> SmpReport {
    SmpReport {
        entry_el: 0, mpidr: 0, x0: 0, x1: 0, x2: 0, x3: 0,
        cntkctl0: 0, cntkctl1: 0, cntv_ctl: 0, cntvct: 0, cntfrq: 0,
        gicc_ctlr: 0, gicc_pmr: 0, gicc_iar: 0, itgt33: 0, tpidr: 0,
    }
}

// 编译期钉板：asm 字段偏移 ×8 与结构布局一致（C-2a Stat 布局案立法——同尺寸
// 不同字段序是最阴的静默错，钉全不钉零头）。
const _: () = assert!(core::mem::size_of::<SmpReport>() == 16 * 8);
const _: () = assert!(core::mem::offset_of!(SmpReport, entry_el) == 0);
const _: () = assert!(core::mem::offset_of!(SmpReport, mpidr) == 8);
const _: () = assert!(core::mem::offset_of!(SmpReport, x0) == 16);
const _: () = assert!(core::mem::offset_of!(SmpReport, x1) == 24);
const _: () = assert!(core::mem::offset_of!(SmpReport, x2) == 32);
const _: () = assert!(core::mem::offset_of!(SmpReport, x3) == 40);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntkctl0) == 48);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntkctl1) == 56);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntv_ctl) == 64);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntvct) == 72);
const _: () = assert!(core::mem::offset_of!(SmpReport, cntfrq) == 80);
const _: () = assert!(core::mem::offset_of!(SmpReport, gicc_ctlr) == 88);
const _: () = assert!(core::mem::offset_of!(SmpReport, gicc_pmr) == 96);
const _: () = assert!(core::mem::offset_of!(SmpReport, gicc_iar) == 104);
const _: () = assert!(core::mem::offset_of!(SmpReport, itgt33) == 112);
const _: () = assert!(core::mem::offset_of!(SmpReport, tpidr) == 120);

/// 副核报告区（高半 `.bss`，主核 boot 清零一次；副核经 PA 写——超时也能收尸）。
#[no_mangle]
pub static mut SMP_SEC_REPORT: SmpReport = zero_report();

/// 心跳 1：副核十项初始化全走完、入 park 前（主核见此才解除静默开始打印）。
#[no_mangle]
pub static SMP_HB1: AtomicU64 = AtomicU64::new(0);
/// 心跳 2：副核收到 release、离开 wfe 邮箱后（握手闭环）。
#[no_mangle]
pub static SMP_HB2: AtomicU64 = AtomicU64::new(0);
/// release 邮箱（主核 store→dsb sy→sev 门铃序；副核查条件→wfe→醒来重查
/// 与之配套才构成丢唤醒免疫——store 全局可见性先于事件投递由 DSB 保证，
/// 见 `smp_bringup` release 段注）。
#[no_mangle]
pub static SMP_RELEASE: AtomicU64 = AtomicU64::new(0);
/// wfe 返回计数（诊断面：TCG wfe=hint 立返 → 此值高；真机应为小值）。
#[no_mangle]
pub static SMP_SEC_SPINS: AtomicU64 = AtomicU64::new(0);

/// per-CPU 块（副核 §3.2 ⑩ / 主核 S-2 前置件 2）：TPIDR_EL1 钉本核块地址
/// ——S-3 起扩为真 per-CPU 区（runqueue/current 指针等）。64 对齐（cache 行
/// 口径预留）。S-2 起首个真字段 = `last_vct`（A11：RR.tick 周期数记账基准
/// per-CPU 化——双 tick 结构性必须：两核共享全局时每核只对「自对方核上次
/// tick 以来的增量」扣减 = slice 约半速不确定燃烧，§3.4 红线；本核只写本核块）。
#[repr(C, align(64))]
pub struct SmpCpu {
    pub cpu: u64,        // 逻辑核号（主核=0 / 副核=1）
    pub mpidr: u64,      // 入口 MPIDR 原始值
    pub present: u64,    // 主核恒 1；副核 = bring-up 心跳闭环（hb2 后置）
    pub last_vct: u64,   // RR.tick 记账基准（A11 per-CPU——本核独占写）
    pub current: u64,    // 本核 CURRENT 任务指针（S-2b——null = 副核 park 态）
    pub el0_ticks: u64,  // PAR 金丝雀：tick 采样时 CURRENT 为 EL0 用户任务的计数
    pub last_task: u64,  // PAR 见证：tick 采样时本核当前任务（0 = 非 EL0 清钉）
    pub par_witness: u64, // PAR 粘性 witness（仅 CPU0 块有意义，见 par_sample）
    /// 本核 on_irq_tick 总计数（诊断面：PAR absent 时区分「tick 没响」vs
    /// 「tick 响了但派发不出」——与 el0_ticks 不同，park/内核态 tick 也计）。
    pub ticks: u64,
    /// 本核 tick **入口**计数（klock_acquire 之前——诊断面：ticks_pre 涨而
    /// ticks 不涨 = 卡锁；两者同冻 = CNTV/IRQ 链死。S-2b 排障期）。
    pub ticks_pre: u64,
    /// 本核 park 派发计数（on_irq_tick cur==null 出选真任务的次数——
    /// c1 恒 0 时证「派发发生过但任务没活到下个 tick」。S-2b 排障期）。
    pub disp_cnt: u64,
    /// 本核上下文切换计数（S-5a per-CPU 统计——`SWITCHES` 全局原子的分核
    /// 面；sched 切换位点调 [`note_switch_cpu`]，本核独占写）。soak-smp 心跳
    /// 侧 sw0/sw1 各自严格递增 = 双核各自调度活（全局计数盖不住对核冻死）。
    pub sw_cnt: u64,
    /// 本核 park 入口计数（S-5a——idle_loop None 臂/secondary_main park 尾
    /// 调 [`note_park_cpu`]）。soak 双核满载证明：满载核 park 停增（深 park
    /// 窗无醒源）；心跳侧 idle 增量有界帽 = 满载谓词（`测试策略.md` idle
    /// residency 常设义务的最低限度兑现）。
    pub park_cnt: u64,
    /// 本核抢占禁用计数（G3-1a——P-A 抢占点加密基座）：>0 = 禁切窗（第三窗，
    /// SVC 主体无锁段/长循环段；语义等价今日 IRQ 屏蔽窗的「非零即禁」）。本核
    /// 独占写（禁切窗只在本核开合）；读点 = 切换原语入口配平断言 + 检查点探峰
    /// （g3-1b 起）。零行为变更步：g3-1a 落位后恒 0（无人 inc），g3-1c 域解蔽
    /// 才有真消费者。AtomicU64（f4 m3 修复批）：g3-1c 起窗口在 IRQ 解蔽态开合，
    /// 非原子 RMW 会被 tick 撕裂（读/写之间打断 = tick 见旧值判可切）——原子
    /// 类型同尺寸 8B，偏移 104/尺寸断言不动。
    pub preempt_count: AtomicU64,
    /// G3-3 lockdep per-CPU 持有链（尾插第四组）：类号栈（物理取序记录，
    /// [`crate::lockdep`] 独用——feature `lockdep` 关时无人读写，字段常零）。
    /// 本核独占：一切取放点恒在 IRQ 屏蔽窗（雷 2 纪律）⇒ 无原子需求。
    pub lockdep_chain: [u8; crate::lockdep::DEPTH_MAX],
    /// 链深（栈顶位）。
    pub lockdep_depth: u8,
    /// IRQ 地板（`exception_irq` 入口深度快照——跨流分层，lockdep 模块头）。
    pub lockdep_floor: u8,
}

const fn zero_cpu() -> SmpCpu {
    SmpCpu {
        cpu: 0,
        mpidr: 0,
        present: 0,
        last_vct: 0,
        current: 0,
        el0_ticks: 0,
        last_task: 0,
        par_witness: 0,
        ticks: 0,
        ticks_pre: 0,
        disp_cnt: 0,
        sw_cnt: 0,
        park_cnt: 0,
        preempt_count: AtomicU64::new(0),
        lockdep_chain: [0; crate::lockdep::DEPTH_MAX],
        lockdep_depth: 0,
        lockdep_floor: 0,
    }
}

// 编译期钉板：secondary_entry asm 硬编码偏移（cpu@[x24]、mpidr@[x24,#8]、
// present@[x24,#16]）与结构布局一致——SmpReport 同款纪律（钉全不钉零头）。
// 本块是 S-3 per-CPU 区的种子，重排/插字段要到 S-3 才有消费者显形，
// 编译期先拦（C-2a Stat 布局案：同尺寸不同字段序 = 最阴的静默错）。
const _: () = assert!(core::mem::size_of::<SmpCpu>() == 128); // ticks@64 起第二 cache 行
const _: () = assert!(core::mem::offset_of!(SmpCpu, cpu) == 0);
const _: () = assert!(core::mem::offset_of!(SmpCpu, mpidr) == 8);
const _: () = assert!(core::mem::offset_of!(SmpCpu, present) == 16);
const _: () = assert!(core::mem::offset_of!(SmpCpu, last_vct) == 24);
const _: () = assert!(core::mem::offset_of!(SmpCpu, current) == 32);
const _: () = assert!(core::mem::offset_of!(SmpCpu, el0_ticks) == 40);
const _: () = assert!(core::mem::offset_of!(SmpCpu, last_task) == 48);
const _: () = assert!(core::mem::offset_of!(SmpCpu, par_witness) == 56);
const _: () = assert!(core::mem::offset_of!(SmpCpu, ticks) == 64);
const _: () = assert!(core::mem::offset_of!(SmpCpu, ticks_pre) == 72);
const _: () = assert!(core::mem::offset_of!(SmpCpu, disp_cnt) == 80);
// S-5a 尾插两字段（sw_cnt@88/park_cnt@96）：逻辑尺寸 104 仍向上取整 128
// （align 64）——size 断言不变；asm 只钉 0/8/16 三偏移，不受尾插影响。
const _: () = assert!(core::mem::offset_of!(SmpCpu, sw_cnt) == 88);
const _: () = assert!(core::mem::offset_of!(SmpCpu, park_cnt) == 96);
// G3-1a 尾插第三字段（preempt_count@104）：同 S-5a 尾插纪律——逻辑尺寸 112
// 仍 ≤ 128 帽，size 断言不变、asm 不受影响。
const _: () = assert!(core::mem::offset_of!(SmpCpu, preempt_count) == 104);
// G3-3 尾插第四组（lockdep_chain@112 / depth@124 / floor@125）：g3-3c 抬
// DEPTH_MAX 8→12（PT/ASID 入检测域留裕），逻辑尺寸 126 仍 ≤ 128 帽
// （align 64）——size 断言不变、asm 只钉 0/8/16 不受影响。
const _: () = assert!(core::mem::offset_of!(SmpCpu, lockdep_chain) == 112);
const _: () = assert!(core::mem::offset_of!(SmpCpu, lockdep_depth) == 124);
const _: () = assert!(core::mem::offset_of!(SmpCpu, lockdep_floor) == 125);

/// 副核 per-CPU 块（TPIDR_EL1 由 secondary_entry asm ⑩ 自钉）。
#[no_mangle]
pub static mut SMP_CPU1: SmpCpu = zero_cpu();

/// 主核 per-CPU 块（前置件 2，S-2）：TPIDR_EL1 由 [`cpu0_init`] 自钉——
/// 单核机型同走（LAST_VCT per-CPU 化不分核数）。
#[no_mangle]
pub static mut SMP_CPU0: SmpCpu = zero_cpu();

/// 主核自钉 per-CPU 块：boot DAIF 屏蔽尾窗、`arm_slice`/`enable_irq` 前调用
///（首 tick 前必达）。副核同款动作在 secondary_entry asm ⑩——两核各自
/// TPIDR_EL1 钉各自块。
pub fn cpu0_init() {
    let mpidr: u64;
    // SAFETY: 只读系统寄存器；boot 单线程窗。
    unsafe {
        asm!("mrs {}, mpidr_el1", out(reg) mpidr);
        let p: *mut SmpCpu = &raw mut SMP_CPU0;
        (*p).cpu = 0;
        (*p).mpidr = mpidr;
        (*p).present = 1; // 主核恒在场（无 bring-up 握手）
        asm!("msr tpidr_el1, {0}", in(reg) p as u64);
        asm!("isb");
    }
}

/// 本核 LAST_VCT 槽（A11/前置件 2）：TPIDR_EL1 → `SmpCpu.last_vct`。返回
/// 裸指针由调用方 volatile 读写（IRQ 热路径零包装）；None = TPIDR_EL1 未装
///（cpu0_init 前防御窗——RR.tick 实际不可达此态）——调用方回落自有全局。
pub fn last_vct_slot() -> Option<*mut u64> {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return None;
    }
    // SAFETY: 非零即本核钉入的 SmpCpu 地址（cpu0_init / secondary_entry ⑩）；
    // last_vct 字段本核独占（tick 锁窗内单写者）。
    Some(unsafe { core::ptr::addr_of_mut!((*(base as *mut SmpCpu)).last_vct) })
}

/// 本核 CURRENT 槽（S-2b）：TPIDR_EL1 → `SmpCpu.current`——CURRENT per-CPU
/// 访问器的存储面（sched 侧 current_task/set_current 经此读写；null = 副核
/// park 态空态契约）。None = TPIDR_EL1 未装（cpu0_init/ktest 前防御窗）——
/// 调用方回落自有全局（与 last_vct_slot 同款契约）。
pub fn current_slot() -> Option<*mut u64> {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return None;
    }
    // SAFETY: 非零即本核钉入的 SmpCpu 地址；current 字段本核独占写
    //（切换原语锁窗内单写者）。
    Some(unsafe { core::ptr::addr_of_mut!((*(base as *mut SmpCpu)).current) })
}

/// 本核 PREEMPT_COUNT 槽（G3-1a）：TPIDR_EL1 → `SmpCpu.preempt_count`——
/// 抢占禁用计数的 per-CPU 存储面（sched 侧 preempt_disable/enable 经此读写；
/// m3 修复批起原子面——返回共享引用，sched 侧消费全安全代码）。
/// None = TPIDR_EL1 未装（cpu0_init/ktest 前防御窗）——调用方回落自有全局
///（与 [`current_slot`] 同款契约）。
pub fn preempt_count_slot() -> Option<&'static AtomicU64> {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return None;
    }
    // SAFETY: 非零即本核钉入的 SmpCpu 地址（boot 期钉、此后不变）；
    // preempt_count 字段本核独占写（禁切窗只在本核开合——跨核不触达）。
    Some(unsafe { &*(core::ptr::addr_of_mut!((*(base as *mut SmpCpu)).preempt_count)) })
}

/// 本核 LOCKDEP 槽（G3-3）：TPIDR_EL1 → `SmpCpu` 尾插链三元组（类栈首址 /
/// 深度 / 地板裸指针）。仅 [`crate::lockdep`] 消费——模块常驻编译（feature
/// 只门调用点），feature 关时链零读写零成本。本核独占写窗 = 取放点 IRQ 屏蔽
/// 窗，无原子需求。None = TPIDR_EL1 未装（ktest/极早 boot 窗）——lockdep
/// 全检跳过（覆盖缺口注记见模块头）。
pub(crate) fn lockdep_slot() -> Option<(*mut u8, *mut u8, *mut u8)> {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return None;
    }
    // SAFETY: 非零即本核钉入的 SmpCpu 地址（boot 期钉、此后不变）；三字段
    // 本核独占（IRQ 屏蔽窗内单写者）。
    let c = unsafe { &mut *(base as *mut SmpCpu) };
    Some((
        c.lockdep_chain.as_mut_ptr(),
        &mut c.lockdep_depth,
        &mut c.lockdep_floor,
    ))
}

/// 本核逻辑核号（S-2b）：TPIDR_EL1 未装（ktest/boot 早期窗）→ 0（CPU0）。
/// 副核 pick 过滤（pinned!=0）与 on_irq_tick 副核早退按此分诊。
pub fn cpu_id() -> usize {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return 0;
    }
    // SAFETY: 非零即本核钉入的 SmpCpu 地址；cpu 字段块建成后只读。
    unsafe { (&raw const (*(base as *mut SmpCpu)).cpu).read_volatile() as usize }
}

/// per-CPU park 上下文槽（S-2b §2.3「首派发上下文」；S-3b b-2 雷② 收口——
/// 子案 §9.5 末）：某核 park 态 wfi 被打断 → park 派发把该核栈上下文惰性
/// 存进**本核槽位**；任务侧 block/preempt 回 park = `context_switch(cur.ctx,
/// park_ctx)` 恢复同一帧 → eret 回该核统一环。非调度语义 Task：不进
/// RUNQUEUE、无 TCB、仅 TaskContext 存档（不立第二个 IDLE_TASK——A7 口径
/// 延续）。首次派发前 = ZERO 纯 sink（switch asm 写 prev 读 next，ZERO 起步
/// 安全）；「切回 park 前必有 park→task 派发」构造性保证。
///
/// S-3b b-2 起单槽真化 per-CPU `[ctx; 2]`：全局单槽形态下，CPU0 idle 若经
/// 本块派发会**覆写 CPU1 park 帧**（跨核帧灾难——雷②，从未触发：CPU0 idle
/// 旧形态结构性不达本块，wedge 比静默跨核腐坏更安全）。分诊按 cpu_id 后
/// CPU0 槽防御性在位（现调用面 block/exit 两 park 切点均 `cpu_id != 0` 臂
/// 独占；S-3c 拆锁若开 CPU0 park 面即天然就位，不再改本块）。
#[no_mangle]
pub static mut SMP_SEC_PARK_CTX: [crate::task::TaskContext; 2] =
    [crate::task::TaskContext::ZERO, crate::task::TaskContext::ZERO];

/// 本核 park ctx 槽指针（切换原语消费；block/exit 两 park 切点调用零改）。
pub fn sec_park_ctx() -> *mut crate::task::TaskContext {
    // SAFETY: static mut 槽位寻址（无并发窗——本核槽本核独占读写，跨核槽
    // 只经 KERN_LOCK 串行的切换原语触达）。
    unsafe { &raw mut SMP_SEC_PARK_CTX[cpu_id()] }
}

/// 副核高半栈（S-2b 修复：低半 `__boot_stack_top` 只供 asm 早期）。副核
/// Rust 入口立即换到本栈——此后探测环帧/tick IRQ 帧/park 存档帧全在高半：
/// 主表与任意用户表（内核高半共享）都映，park 帧恢复后 `load_frame` 读栈
/// 不再依赖低半映射。实测雷（rootfs 形态 c1=0 排障）：exit 链 park 切点
/// `activate_aspace(0,0)` 切主表（2b-2 起无低半映射）→ 恢复的 park 帧 SP=
/// 低半栈 → 读 IRQ 栈帧 fault → 向量风暴死循环（SP 每轮 `-0x120` 下坠，
/// tick 永不进——`pre1==t1` 同冻即此死状指纹）。16KiB 同早期栈量级；换栈
/// 点在 `-> !` 入口（永不 ret），旧低半帧弃置无害。
#[repr(C, align(16))]
struct SecStackBuf([u8; 16384]);
static SMP_SEC_STACK: SecStackBuf = SecStackBuf([0; 16384]);

/// 副核高半栈顶（满减栈 = 数组尾）。
pub fn sec_stack_top() -> usize {
    (&raw const SMP_SEC_STACK).addr() + 16384
}

/// 副核参与门（gate，S-2a/S-2b 拆步天然缝）：置位前副核 secondary_main
/// 探测环每 slice 空转（on_irq_tick 早退、零共享触达）；置位后入正式参与
///（tick 进锁窗 pick）。主核 smp_bringup 成功后置位（Release 序，副核
/// 下次 tick 醒来生效——无 IPI 的 E0 代价，轮询免疫丢唤醒）。
static SMP_PAR: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// 置位参与门（主核 boot 尾、DAIF 屏蔽窗内单写点）。
pub fn par_enable() {
    SMP_PAR.store(1, core::sync::atomic::Ordering::Release);
}

/// 副核参与门状态（on_irq_tick 副核早退判据）。
pub fn par_enabled() -> bool {
    SMP_PAR.load(core::sync::atomic::Ordering::Acquire) >= 1
}

/// 每核算力（0..=1023 同构尺度，Linux `scale_capacity` 口径）——S-3b
/// `Topo::capacity` 填表源（sched::init 调）。QEMU virt 双核同构：恒 1024
/// 占位；真差异容量（大小核）= EAS 面 v2+（届时按 DTT/ACPI 定形，不在此
/// 硬编码拓扑）。`cpu` 越界核号返 0（保守——拓扑表外核无算力）。
pub fn cpu_capacity(_cpu: u32) -> u32 {
    1024
}

/// SMP 活性（hb2 心跳闭环 = bring-up 成功）：PAR 金丝雀 spawn 门 =
/// 单核相零漂移自动成立（ncores==2 才 spawn）。
pub fn smp_active() -> bool {
    SMP_HB2.load(core::sync::atomic::Ordering::Acquire) >= 1
}

/// PAR 金丝雀采样（S-2b §5.2 D1——on_irq_tick 锁窗内每 tick 一次；
/// per-CPU 块本核独占写，跨核读 volatile，BKL 锁窗串行无撕裂）：
/// - EL0 tick：本核 el0_ticks+1、last_task=cur 钉板；
/// - 非 EL0：last_task 清钉（0 = 非 EL0 见证缺席，witness 不误置）；
/// - 粘性 witness（仅 CPU0）：自身 EL0 且读到 CPU1 last_task 为**另一**
///   EL0 任务 → 双核重叠执行真并行见证，单向置位后不清。
pub fn par_sample(cur: usize, is_el0: bool) {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return; // ktest/早期防御窗：无本核块（PAR 面不可达）
    }
    // SAFETY: 本核块；el0_ticks/last_task/par_witness 本核独占写（tick
    // 锁窗内），SMP_CPU1.last_task 跨核 volatile 读（对齐 u64 原子无撕裂）。
    unsafe {
        let my = base as *mut SmpCpu;
        if is_el0 {
            (*my).el0_ticks = (*my).el0_ticks.wrapping_add(1);
            (*my).last_task = cur as u64;
            if (*my).cpu == 0 {
                let c1_last = (&raw const SMP_CPU1.last_task).read_volatile();
                if c1_last != 0 && c1_last != cur as u64 {
                    (*my).par_witness = 1;
                }
            }
        } else {
            (*my).last_task = 0;
        }
    }
}

/// PAR 读数（打印任务消费：volatile 单字读，下界断言无需一致快照）：
/// 返回 (el0_ticks0, el0_ticks1, witness, ticks0, ticks1)——后两位是本核
/// on_irq_tick 总计数（诊断：`t1==0` = 副核 tick 链路死（探测环没被 CNTV
/// 打醒），`t1>0 && c1==0` = tick 活但派发/出选面问题）。
pub fn par_readout() -> (u64, u64, u64, u64, u64) {
    // SAFETY: 跨核 volatile 读（对齐 u64 原子无撕裂；谓词为下界断言）。
    unsafe {
        let c0 = (&raw const SMP_CPU0.el0_ticks).read_volatile();
        let c1 = (&raw const SMP_CPU1.el0_ticks).read_volatile();
        let w = (&raw const SMP_CPU0.par_witness).read_volatile();
        let t0 = (&raw const SMP_CPU0.ticks).read_volatile();
        let t1 = (&raw const SMP_CPU1.ticks).read_volatile();
        (c0, c1, w, t0, t1)
    }
}

/// 排障期扩展读数（S-2b 诊断面）：(pre0, pre1, disp1)——pre = klock 前 tick
/// 入口计数（pre 涨/ticks 冻 = 卡锁；同冻 = CNTV/IRQ 链死），disp1 = 副核
/// park 派发计数（disp1>0 && c1==0 = 派发出的任务没活到下个 tick）。
pub fn diag_readout() -> (u64, u64, u64) {
    // SAFETY: 跨核 volatile 读（诊断谓词，无需一致快照）。
    unsafe {
        let p0 = (&raw const SMP_CPU0.ticks_pre).read_volatile();
        let p1 = (&raw const SMP_CPU1.ticks_pre).read_volatile();
        let d1 = (&raw const SMP_CPU1.disp_cnt).read_volatile();
        (p0, p1, d1)
    }
}

/// S-2b：副核 park 派发**限频门**——每 4 tick（800ms 周期）放行一次摘取。
/// pinned 满速循环（child 类）占空比砍到 25%：console 单通道的生产率降回
/// CPU0 轮转可消化量级（fix4/fix5 双向实锤——低优消费者 → 生产者深饿；
/// 高优消费者 → 0 层验收任务全饿；无界生产下下游怎么调都错，正解在源头
/// 限频）。
///
/// **已退役（S-3a 六件义务⑥，子案 §9.5 步 B）**：tickless park + SGI 事件
/// 驱动唤醒落地后，副核参与节奏由 CPU0 入队事件定步（往返背压——console
/// 消费一轮才唤醒生产一轮），周期轮询限频失去存在前提，两处调用点摘除。
/// 保留 fn 与注释供诊断史对照（复发盯面 = smoke-smp 相 2 现存断言：
/// rootfs 帽 6.5× 余量 + echod/-ETIMEDOUT 标记链——帽断言 FAIL 即复发信号）。
#[allow(dead_code)]
pub fn park_due() -> bool {
    // SAFETY: 本核块（调用点在 CPU1 tick 锁窗内）；对齐 u64 volatile 无撕裂。
    unsafe { ((&raw const SMP_CPU1.ticks).read_volatile() & 3) == 0 }
}

/// 本核 on_irq_tick 计数（on_irq_tick gate 检查后调用——两核都计，per-CPU
/// 块本核独占写）。诊断面：见 [`par_readout`]。
pub fn note_tick() {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return; // ktest/早期防御窗
    }
    // SAFETY: 本核块；ticks 本核独占写（tick 锁窗内）。
    unsafe {
        let my = base as *mut SmpCpu;
        (*my).ticks = (*my).ticks.wrapping_add(1);
    }
}

/// 本核 tick **入口**计数（klock_acquire 之前调用——含 gate 早退轮）。
/// 诊断面：见 [`diag_readout`]。S-2b 排障期。
pub fn note_tick_entry() {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return; // ktest/早期防御窗
    }
    // SAFETY: 本核块；ticks_pre 本核独占写（IRQ 入口，锁外）。
    unsafe {
        let my = base as *mut SmpCpu;
        (*my).ticks_pre = (*my).ticks_pre.wrapping_add(1);
    }
}

/// 本核上下文切换计数（S-5a——sched 六切换位点全局 `SWITCHES` 自增后调用；
/// 与全局计数器同窗、本核独占写）。读面：[`sw_park_readout`]。
pub fn note_switch_cpu() {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return; // ktest/早期防御窗（切换面实际不可达，防御对齐 note_tick 族）
    }
    // SAFETY: 本核块；sw_cnt 本核独占写（切换位点 IRQ 屏蔽/锁窗内）。
    unsafe {
        let my = base as *mut SmpCpu;
        (*my).sw_cnt = (*my).sw_cnt.wrapping_add(1);
    }
}

/// 本核 park 入口计数（S-5a——idle_loop None 臂 / secondary_main park 尾
/// wfi 前调用；本核独占写）。读面：[`sw_park_readout`]。
pub fn note_park_cpu() {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return; // ktest/早期防御窗
    }
    // SAFETY: 本核块；park_cnt 本核独占写（park 尾屏蔽窗内单写者）。
    unsafe {
        let my = base as *mut SmpCpu;
        (*my).park_cnt = (*my).park_cnt.wrapping_add(1);
    }
}

/// S-5a per-CPU 统计读数（跨核 volatile 单字读，对齐 u64 原子无撕裂；谓词
/// 为严格递增/有界增量断言，无需一致快照）：返回 (sw0, sw1, park0, park1)。
/// soak-smp 心跳消费：sw0/sw1 各自严格递增 = 双核各自调度活；park 增量
/// 有界 = 满载谓词（满载核 park 停增）。
pub fn sw_park_readout() -> (u64, u64, u64, u64) {
    // SAFETY: 跨核 volatile 读（诊断谓词）。
    unsafe {
        let s0 = (&raw const SMP_CPU0.sw_cnt).read_volatile();
        let s1 = (&raw const SMP_CPU1.sw_cnt).read_volatile();
        let p0 = (&raw const SMP_CPU0.park_cnt).read_volatile();
        let p1 = (&raw const SMP_CPU1.park_cnt).read_volatile();
        (s0, s1, p0, p1)
    }
}

/// 本核 park 派发计数（on_irq_tick cur==null 出选真任务时调用——锁窗内）。
/// 诊断面：见 [`diag_readout`]。S-2b 排障期。
pub fn note_dispatch() {
    let base: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) };
    if base == 0 {
        return; // ktest/早期防御窗
    }
    // SAFETY: 本核块；disp_cnt 本核独占写（tick 锁窗内）。
    unsafe {
        let my = base as *mut SmpCpu;
        (*my).disp_cnt = (*my).disp_cnt.wrapping_add(1);
    }
}

/// boot asm 钉存的**入口** EL（1/2/3）：`_start` 处检测、经 x20（callee-saved，
/// eret 不动 GPR）携带过 EL2→EL1 降级路径，`.Lbss_done`（`.bss` 清零之后）
/// 经 PA 写入——在 EL1 落点再读 CurrentEL 只会得到降级后的 1，必须入口即钉。
/// S-1 conduit 分型门：PSCI HVC 只在默认机型（boot EL1）发放——
/// `virtualization=on` 下 HVC 从 EL1 陷阱到 EL2 且 QEMU 不再拦截为 PSCI
/// （VBAR_EL2=0 → PC=0 死循环，probe C 相 + S-1 主树首跑双实据挂死），
/// SMC 变体留档不实现（S-1 盒子）。高半普通 `.bss`（boot asm 写
/// PA = VMA − 基址——与 .bss 清零循环同款口径）。
#[no_mangle]
pub static mut BOOT_EL: u64 = 0;

extern "C" {
    /// 链接脚本高半 `.rodata` 指针字 = 低半 `secondary_entry` 绝对地址
    /// （VMA==PA）。高半 Rust 直取低半符号地址越 ADRP ±4GB（2b-1 双向坑），
    /// 经指针字间接取——S-0 probe 三相验证的唯一解法。
    static __smp_sec_entry: u64;
}

global_asm!(
    ".section .text.boot",
    ".globl secondary_entry",
    ".type secondary_entry,%function",
    "secondary_entry:",
    "    ldr x27, =0xFFFFFF8000000000",      // x27 = KERNEL_VA_BASE（全程钉住）
    "    ldr x26, =SMP_SEC_REPORT",
    "    sub x26, x26, x27",                 // x26 = 报告区 PA（MMU 前经 PA 写）
    "    stp x0, x1, [x26, #16]",            // 第一时间保存入口 x0..x3
    "    stp x2, x3, [x26, #32]",
    "    mrs x0, CurrentEL",
    "    lsr x0, x0, #2",
    "    str x0, [x26]",                     // entry_el
    "    mrs x1, mpidr_el1",
    "    str x1, [x26, #8]",                 // mpidr
    "    ldr x1, =0x09000000",               // 哨兵字符 's'（副核唯一 UART 写；
    "    mov x0, #0x73",                     //  PA 直写——入口执行的第一证据）
    "    str w0, [x1]",
    "    ldr x0, =__vectors",                // ④VBAR_EL1 ← 向量表 PA（同 CPU0
    "    sub x0, x0, x27",                   //  一张表；CPU1 身份 TTBR0 常活）
    "    msr vbar_el1, x0",
    "    isb",
    "    ldr x0, [x26]",                     // entry_el 重载（x0 刚被哨兵占用）
    "    cmp x0, #2",
    "    b.eq .Lsmp_from_el2",
    "    cmp x0, #3",
    "    b.eq .Lsmp_el3_park",              // EL3 不预期（virt 无）；MMU 未开
    "    b .Lsmp_el1",                       //   → 独立 PA wfi 环（读不得高半指针字）

    // ---- ① EL2 → EL1 降级（复刻 boot.rs .Lfrom_el2；elr 落回本函数）----
    ".Lsmp_from_el2:",
    "    mrs x0, sctlr_el2",
    "    bic x0, x0, #1",                    // 清 M 位：关 EL2 MMU
    "    msr sctlr_el2, x0",
    "    isb",
    "    tlbi alle2",
    "    dsb sy",
    "    isb",
    "    movz x0, #0x8000, lsl #16",         // HCR_EL2.RW=1 → EL1 为 AArch64
    "    msr hcr_el2, x0",
    "    msr sctlr_el1, xzr",                // EL1 MMU 干净状态
    "    isb",
    "    ldr x0, =.Lsmp_el1",
    "    msr elr_el2, x0",
    "    mov x0, #0x3c5",                    // EL1h + DAIF 屏蔽
    "    msr spsr_el2, x0",
    "    eret",

    // ---- EL1 续行（直达 或 eret 后）----
    ".Lsmp_el1:",
    "    ldr x0, =__boot_stack_top",         // ⑦复用主核已弃用的低半 32KiB 早期栈
    "    mov sp, x0",
    "    movz x0, #0x30, lsl #16",           // ⑤CPACR_EL1.FPEN=0b11（同 boot.rs）
    "    msr cpacr_el1, x0",
    "    isb",
    "    mrs x0, cntkctl_el1",               // ⑨复位读（per-core 证据面）
    "    str x0, [x26, #48]",
    "    orr x0, x0, #2",                    // EL0VCTEN=1（B-1 同款门，自开）
    "    msr cntkctl_el1, x0",
    "    isb",
    "    mrs x0, cntkctl_el1",
    "    str x0, [x26, #56]",
    "    mrs x0, cntv_ctl_el0",
    "    str x0, [x26, #64]",
    "    mrs x0, cntvct_el0",
    "    str x0, [x26, #72]",
    "    mrs x0, cntfrq_el0",
    "    str x0, [x26, #80]",
    // ---- ⑥GIC per-CPU：只动本核 GICC（banked），绝不碰 GICD ----
    "    ldr x1, =0x08010000",               // GICC PA
    "    ldr w0, [x1]",                      // CTLR 复位读（期望 0）
    "    str x0, [x26, #88]",
    "    mov w0, #0xff",
    "    str w0, [x1, #4]",                  // PMR=0xff（全优先级放行）
    "    ldr w0, [x1, #4]",
    "    str x0, [x26, #96]",
    "    ldr w0, [x1, #0xC]",                // IAR spurious 读（无副作用）
    "    str x0, [x26, #104]",
    "    mov w0, #1",
    "    str w0, [x1]",                      // GICC CTLR=1（无 routed 源 → 无中断）
    "    ldr x1, =0x08000000",               // GICD PA（只读面）
    "    ldrb w0, [x1, #0x821]",             // ITARGETSR[33]（0x800+33）
    "    str x0, [x26, #112]",
    // ---- ③开 MMU（表 = 主核 boot 所建、只读消费；寄存器全 per-CPU）----
    "    bl enable_mmu",
    // ---- 高半续行：⑩TPIDR_EL1 = per-CPU 块 ----
    "    ldr x26, =SMP_SEC_REPORT",          // 换 VMA（TTBR1 生效）
    "    ldr x24, =SMP_CPU1",
    "    mov x0, #1",
    "    str x0, [x24]",                     // cpu = 1
    "    mrs x1, mpidr_el1",
    "    str x1, [x24, #8]",                 // mpidr
    "    msr tpidr_el1, x24",                // TPIDR_EL1 ← per-CPU 块地址
    "    mrs x1, tpidr_el1",
    "    str x1, [x26, #120]",               // 写读证据
    // ---- 心跳 1 → wfe 邮箱 park（先置后查再 wfe：丢失唤醒免疫序）----
    "    dmb ish",
    "    ldr x9, =SMP_HB1",
    "    mov x0, #1",
    "    str x0, [x9]",
    "    dmb ish",
    ".Lsmp_park:",
    "    ldr x9, =SMP_RELEASE",
    "    ldr x0, [x9]",
    "    cbnz x0, .Lsmp_released",
    "    wfe",
    "    ldr x9, =SMP_SEC_SPINS",
    "    ldr x0, [x9]",
    "    add x0, x0, #1",
    "    str x0, [x9]",
    "    b .Lsmp_park",
    ".Lsmp_released:",
    "    dmb ish",
    "    ldr x9, =SMP_HB2",
    "    mov x0, #1",
    "    str x0, [x9]",
    "    str x0, [x24, #16]",                // per-CPU 块 present=1
    "    dmb ish",
    // ---- ⑧转高半 secondary_main（S-2b）：指针字 + blr——直接 `bl` 低半→高半
    //      CALL26 ±128MiB 越界断链，经 `.rodata` 指针字寄存器间接跳无此限
    //      （__smp_sec_entry 同款先例）；MMU 已开（bl enable_mmu 之后）高半
    //      字面量可读。gate 检查在 on_irq_tick 早退（统一探测/参与环合一，
    //      secondary_main 自身无 gate 分支）——不归（-> !）。
    ".Lsmp_final:",
    "    ldr x0, =__secondary_main_ptr",
    "    ldr x1, [x0]",
    "    blr x1",
    ".Lsmp_el3_park:",                       // EL3 兜底：MMU 未开，PA wfi 永停
    "    wfi",
    "    b .Lsmp_el3_park",
    ".size secondary_entry, . - secondary_entry",
    ".ltorg",
);

/// 读 boot asm 钉存的入口 EL（高半 `.bss` 普通静态量，主核单写点后只读）。
fn boot_el() -> u64 {
    // SAFETY: 读 boot asm 单写点钉存的静态量（此后只读）。
    unsafe { (&raw const BOOT_EL).read_volatile() }
}

fn cntvct() -> u64 {
    let v: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, cntvct_el0", out(reg) v) };
    v
}

fn cntfrq() -> u64 {
    let v: u64;
    // SAFETY: 只读系统寄存器。
    unsafe { asm!("mrs {}, cntfrq_el0", out(reg) v) };
    v
}

/// 状态谓词轮询（CNTVCT 截止帽；非竞速——谓词满足即真，到点再验一次即假）。
fn wait_secs(secs: u64, mut pred: impl FnMut() -> bool) -> bool {
    let freq = cntfrq();
    let deadline = cntvct().saturating_add(secs.saturating_mul(freq));
    loop {
        if pred() {
            return true;
        }
        if cntvct() >= deadline {
            return pred();
        }
        for _ in 0..256 {
            core::hint::spin_loop();
        }
    }
}

/// 主核侧 bring-up：CPU_ON → 心跳 → dump → release → hb2 → `SMP ok ncores=2`。
///
/// 调用窗口契约：boot DAIF 屏蔽尾窗（全 spawn/回调注册后、`arm_slice`/
/// `enable_irq` 前）——主核轮询期间静默、无中断、无调度，与副核窗口
/// 结构性无重叠。
///
/// 门（S-1 盒子）：`efi_boot`（EFI/handoff 路的 PSCI conduit 归固件管，
/// 未实测——跳过）+ 入口 EL==1（默认机型；EL2 机型 HVC 挂死风险见
/// [`BOOT_EL`] 注）。单核（无 -smp）时 CPU_ON 返回 -2 → 一行降级，boot 继续。
pub fn smp_bringup(efi_boot: bool) {
    if efi_boot {
        uart_puts(b"SMP: skip (EFI handoff boot)\n");
        return;
    }
    let el = boot_el();
    if el != 1 {
        uart_puts(b"SMP: skip (boot el=");
        uart_put_dec(el);
        uart_puts(b"; HVC conduit = default machine only, S-1 box)\n");
        return;
    }
    // SAFETY: 读链接期定值指针字（高半 .rodata，无副作用）。
    let entry_pa = unsafe { (&raw const __smp_sec_entry).read_volatile() } as usize;
    let ctx_id: u64 = 0x5EC0_DEAD_BEEF_0001; // S-0 钉板口径（x0 回显实测面）
    let ret: u64;
    // CPU_ON 前打点（挂死可定位）；调用后静默直到心跳（UART 单写者纪律）。
    uart_puts(b"\nSMP: cpu_on entry=");
    uart_put_hex64(entry_pa as u64);
    uart_puts(b"\n");
    unsafe {
        asm!(
            "hvc #0",
            inout("x0") 0xC400_0003u64 => ret,   // PSCI CPU_ON（S-0 钉板 fnid）
            inout("x1") 0x1u64 => _,             // target MPIDR Aff0=1（S-0 钉板）
            inout("x2") entry_pa as u64 => _,    // entry PA（.text.boot VMA==PA）
            inout("x3") ctx_id => _,             // context_id
            // SMCCC：x4-x17 调用者保存 + x18 平台寄存器（规范允许固件
            // 不保留——真机 ATF 可改写；不赌实现保留，寄存器套路纪律）
            out("x4") _, out("x5") _, out("x6") _, out("x7") _,
            out("x8") _, out("x9") _, out("x10") _, out("x11") _,
            out("x12") _, out("x13") _, out("x14") _, out("x15") _,
            out("x16") _, out("x17") _, out("x18") _,
        );
    }
    if ret != 0 {
        // 单核（默认 -smp 1）常态：PSCI 返回 -2（INVALID_PARAMS——目标核不在）。
        uart_puts(b"SMP: cpu_on ret=");
        uart_put_hex64(ret);
        uart_puts(b" -> single-core, skip\n");
        return;
    }
    if !wait_secs(5, || SMP_HB1.load(Ordering::Acquire) >= 1) {
        // 超时也 dump：入口块（entry_el/mpidr/x0..x3）在 EL 分流前已写入——
        // 副核死在半路也能收尸定位死点。
        // SAFETY: 只读副核报告区（副核可能仍在写——值仅作诊断）。
        let r = unsafe { (&raw const SMP_SEC_REPORT).read_volatile() };
        uart_puts(b"SMP: FAIL hb timeout el=");
        uart_put_hex64(r.entry_el);
        uart_puts(b" mpidr=");
        uart_put_hex64(r.mpidr);
        uart_puts(b" x0=");
        uart_put_hex64(r.x0);
        uart_puts(b" spins=");
        uart_put_hex64(SMP_SEC_SPINS.load(Ordering::Relaxed));
        uart_puts(b"\n");
        return;
    }
    // 副核已入 wfe park（静默）——dump 报告 + 主控打印窗口。
    // SAFETY: 副核此后只写 SMP_RELEASE 观察位与 spins 计数，报告区冻结。
    let r = unsafe { (&raw const SMP_SEC_REPORT).read_volatile() };
    uart_puts(b"SMP: sec up el=");
    uart_put_hex64(r.entry_el);
    uart_puts(b" mpidr=");
    uart_put_hex64(r.mpidr);
    uart_puts(b" x0=");
    uart_put_hex64(r.x0);
    uart_puts(b" ck0=");
    uart_put_hex64(r.cntkctl0);
    uart_puts(b" ck1=");
    uart_put_hex64(r.cntkctl1);
    uart_puts(b" gctl=");
    uart_put_hex64(r.gicc_ctlr);
    uart_puts(b" gpmr=");
    uart_put_hex64(r.gicc_pmr);
    uart_puts(b" itgt33=");
    uart_put_hex64(r.itgt33);
    uart_puts(b" tpidr=");
    uart_put_hex64(r.tpidr);
    uart_puts(b"\n");
    // release：置邮箱 → dsb sy → sev（Arm ARM 门铃序——store 的全局可见性
    // 先于事件投递由 DSB 保证；副核侧查条件→wfe→醒来重查，与之配套才构成
    // 丢唤醒免疫）。缺 DSB 时真机存在按次随机的丢唤醒窗口：副核被 SEV 兑换
    // 的一次重查若仍读 0，回 wfe 后事件寄存器已清、全系统再无唤醒源 → 永久
    // 睡死（5s 帽收口 `SMP: FAIL hb2 timeout`）。TCG 的 wfe=hint 立返
    // （副核实际忙轮询 RELEASE）结构性掩蔽此窗——恰因此真机序必须在代码里
    // 修掉，不能靠 CI。
    SMP_RELEASE.store(1, Ordering::Release);
    // SAFETY: 特权指令；EL1。无 nomem/pure 选项 = 有内存副作用，编译器
    // 不将其与上方 store 重排；dsb sy 保证 store 先于 sev 对全系统可见。
    unsafe { asm!("dsb sy", "sev") };
    if !wait_secs(5, || SMP_HB2.load(Ordering::Acquire) >= 1) {
        uart_puts(b"SMP: FAIL hb2 timeout\n");
        return;
    }
    // 状态谓词全过才打标记（4-5a 纪律）：非竞速——两心跳都是内存状态断言。
    uart_puts(b"SMP ok ncores=2\n");
    // S-3a probe ①（已实测入册 2026-11-01——母案执行结果区 S-3a 段）：CPU0 →
    // CPU1 单发 SGI_RESCHEDULE(1)，filter=0b00 List=0b10（bit1=CPU1）。probe ①
    // 时收端未接分流臂 → 未知源面 loud `irq? id=1` 零代码观测；probe ② 起
    // trap.rs SGI 消费臂接管（收讫计数，`irq?` 通道退役还给真未知源）。本点
    // 早于副核 enable_sgi_this_core 也不丢：pending 在 CPU1 bank 锁存、使能即
    // 投递（实测兑现）。
    // SAFETY: MMIO 写；boot DAIF 屏蔽窗内（发不等收）。
    unsafe {
        crate::gic::send_sgi(crate::gic::SGI_RESCHEDULE, crate::gic::SgiTarget::List(0b10))
    };
    // S-3a probe ②：收讫状态谓词等待（HB2 同款 wait_secs——非竞速，命中才打
    // 标记；smoke-smp 按相断言 `SGI ok` 恰一次）。超时 = 链断 loud（`SMP: FAIL`
    // 前缀进 smoke 反向快败）；计数装载 Acquire 与 HB2 同款。
    if wait_secs(3, || crate::trap::SGI_SEEN.load(Ordering::Acquire) >= 1) {
        uart_puts(b"SGI ok n=");
        uart_put_dec(crate::trap::SGI_SEEN.load(Ordering::Acquire) as u64);
        uart_puts(b"\n");
    } else {
        uart_puts(b"SMP: FAIL sgi timeout\n");
    }
    // S-3a probe ③：形态 + pending 合并语义观测（观察轮——实测入母案后 SGI
    // 观测面收官；`SGI3` 前缀与 `SGI ok`/`SMP: FAIL` 断言面零碰撞，观察行
    // 不进 smoke 断言）。bringup 全程 boot DAIF 屏蔽（main.rs 装载序：全
    // spawn/回调注册后、arm_slice/enable_irq 前）。
    //
    // 观测 1——Others(0b01)：除己外全体 = CPU1（双核上应与 List(0b10)
    // 同效）。SAFETY: MMIO 写；DAIF 屏蔽窗内（发不等收）。
    unsafe {
        crate::gic::send_sgi(crate::gic::SGI_RESCHEDULE, crate::gic::SgiTarget::Others)
    };
    if wait_secs(3, || crate::trap::SGI_SEEN.load(Ordering::Acquire) >= 2) {
        uart_puts(b"SGI3 others ok n=");
        uart_put_dec(crate::trap::SGI_SEEN.load(Ordering::Acquire) as u64);
        uart_puts(b"\n");
    } else {
        uart_puts(b"SGI3 others timeout\n");
    }
    // 观测 2——同源双发合并：背靠背两发不等首投递。GICv2 同源 SGI 共享一个
    // pending 位 → 次发合并（计数 +1）；实测可能 n=3（合并）/n=4（逐发竞速
    // 胜出）——观察不定论，实测入册（S-0 纪律）。相邻 MMIO 写间隔远小于
    // CPU1 唤醒+ack 延迟 → 合并应结构性成立；2s 稳定窗兜底区分合并/逐发。
    // SAFETY: MMIO 写 ×2；DAIF 屏蔽窗内。
    unsafe {
        crate::gic::send_sgi(crate::gic::SGI_RESCHEDULE, crate::gic::SgiTarget::List(0b10));
        crate::gic::send_sgi(crate::gic::SGI_RESCHEDULE, crate::gic::SgiTarget::List(0b10));
    }
    if wait_secs(3, || crate::trap::SGI_SEEN.load(Ordering::Acquire) >= 3) {
        let merged = !wait_secs(2, || crate::trap::SGI_SEEN.load(Ordering::Acquire) >= 4);
        uart_puts(b"SGI3 merge ok n=");
        uart_put_dec(crate::trap::SGI_SEEN.load(Ordering::Acquire) as u64);
        if merged {
            uart_puts(b" merged");
        }
        uart_puts(b"\n");
    } else {
        uart_puts(b"SGI3 merge timeout\n");
    }
    // 观测 3——Self(0b10)：仅发送核自身。CPU0 屏蔽窗内发送 → 不投递、
    // pending 锁存本核 bank（ISPENDR0 直读自证）；boot 尾 enable_irq 后才
    // 投递。「先发后解蔽不丢」发端版，与 probe ① 收端版互补。首跑实测
    // pending=0（≠预期 1）——补 seen/daif_i 观测位定谳：seen=4 → 未投递
    // 且未锁存（Self filter 路径疑点）；seen=5/daif_i=1 → 窗内被解蔽、
    // 即发即消费。
    // SAFETY: MMIO 写；DAIF 屏蔽窗内。
    unsafe {
        crate::gic::send_sgi(crate::gic::SGI_RESCHEDULE, crate::gic::SgiTarget::Self_)
    };
    uart_puts(b"SGI3 self ok pending=");
    uart_put_dec(
        crate::gic::sgi_pending_this_core(crate::gic::SGI_RESCHEDULE as usize) as u64,
    );
    uart_puts(b" seen=");
    uart_put_dec(crate::trap::SGI_SEEN.load(Ordering::Acquire) as u64);
    uart_puts(b" daif_i=");
    uart_put_dec(crate::kern_lock::irq_enabled() as u64);
    uart_puts(b"\n");
    // S-4 4-1 probe ④：TLB shootdown 回执链——裸发 SGI #2 Others（probe 专用
    // 无闩形态；生产面 = broadcast() 闩 + dispatch 尾等待，4-2 接线）→ CPU1
    // handler 全量本地失效 + 回执计数。等待 = wait_secs 内存状态谓词（4-5a
    // 非竞速纪律）——boot DAIF 屏蔽窗内不依赖本核 IRQ 开，CPU1 handler 侧
    // 置位。`SGI2 ok n=` 恰一次 = smoke-smp 裸相断言源（S-4 §2 4-1 验收）。
    {
        let base = crate::tlbshoot::ack_count(1);
        crate::tlbshoot::send_others();
        if wait_secs(3, || crate::tlbshoot::ack_count(1) > base) {
            uart_puts(b"SGI2 ok n=");
            uart_put_dec((crate::tlbshoot::ack_count(1) - base) as u64);
            uart_puts(b"\n");
        } else {
            uart_puts(b"SMP: FAIL sgi2 timeout\n");
        }
    }
}
