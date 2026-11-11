//! S-4 TLB shootdown 回执协议（4-1 基建）——`SMP-S4实施设计草案.md` §2 4-1
//! + §3.2/§3.3。三件：收端 handler（SGI #2 → 本地全量失效 + 回执计数）、
//! 发端广播原语（广播即返、置 per-CPU 待等闩）、包络外等待点（dispatch 尾
//! `sched_exit_check`——`svc_exit_class` 之后零锁自旋）。
//!
//! ## 等待窗纪律（§3.2 立法）
//!
//! 发端等待时持锁集必须与他核一切 IRQ 屏蔽自旋段的交集为空——本模块等待
//! 恒在 ①PT 锁外 ②整个 SVC 类窗外（`svc_exit_class` 已放 KL+结构锁）③任何
//! 包络锁外。等待 = 自旋读**对核回执计数**（他核 IRQ handler 置位的内存字），
//! 不依赖本核 IRQ 开（SVC 语境 PSTATE.I=1 不构成阻塞）。他核 SVC 窗有限长
//! → 出窗即投递 pending SGI → 必然推进 → 无环（§A112 反例 = 等待落在包络
//! 锁内，此处结构性规避）。
//!
//! ## 回执形态（§3.3）：per-核单调计数
//!
//! 回执 = 每核 `ACKS[core]` 单调计数（收端 handler 本地失效完成后自增），
//! 发端广播前快照对核计数、等待「严格大于快照」。等价于位图清位，但兼作
//! 观测面（probe/ktest 可读）。双核阻塞式发送形态下的恰一归因（§3.3 裁决）：
//! 双核 Others 单发送者单在飞——对核计数只会因**本核发出**的 SGI #2 投递
//! 而增（他人不向我们发 Others 之外的形态），快照后首个增量 ⇔ 本轮失效完成。
//!
//! ## v1 粒度裁决
//!
//! 收端 = **本地全量失效**（`vmalle1` 非 IS，无请求槽）——任意并发/合并
//! 广播下恒正确（每次投递 ⇔ 一次全量清）。按号精确（请求槽 + 分代）= v2
//! 优化；发端 PT 锁窗内自己的 `tlbi aside1is` 已按号失效本核视角。
//!
//! ## S-4 4-3：ASID 回卷 quiesce（代数 + 锁外驱动）
//!
//! ASID 计数器回卷（`AsidAllocator::alloc` 返 `rolled`）时，上一轮同号映射
//! 可能仍驻他核 TLB——回卷方须**全核失效并等回执**（quiesce）后新轮才算
//! 可用。协议（aspace ASID 锁只护计数器推进，quiesce 恒在锁外）：
//! ①锁内 [`roll_gen_bump`]（代数 +1，与取号同临界区——代数序 = 取号序）；
//! ②锁外 [`roll_quiesce_arm`]：本核 `vmalle1is` + [`broadcast`]（置回执闩 +
//! SGI #2，广播即返）+ 置 per-CPU 待驱动闩；③驱动点 [`roll_quiesce_check`]
//! ——SVC 路由 dispatch 尾自动消费，内核任务路显式调用——循环
//! 「快照代数 → broadcast → 等回执 → `QUIESCED` 追平」至追上 `ROLL`
//!（等待期间他核又回卷则代数再进，循环再驱动一轮，全局单调收敛）。
//! 日常防线 = 激活路径按号失效（`activate_aspace` 无条件 `tlbi aside1`）；
//! 本 quiesce 是回卷兜底（`内存管理设计.md` §5.2 / 草案 §2 4-3）。
//!
//! **已知对飞面（4-4 压测相法定探针，本刀只记录不修）**：两核同时广播+
//! 尾等待（并发 unmap / 并发回卷）时各自 SVC 窗 PSTATE.I=1 自旋、互收不到
//! 对方 SGI #2 → 互等。当前测试面单发送者形态不可达；草案 §3.3「对飞不
//! 成立」论断按阻塞式发送立论，实装闩式（广播即返+尾等）的形态漂移在此
//! 记录，修面归 4-4（对核满载荷 SVC 风暴 = 该面探针）。
//!
//! ## 单核短路 / 自核排除（§2 4-1③④）
//!
//! `par_enabled()==false` → `broadcast()` 零发送零闩直返（调用方 PT 窗内
//! 本地 `tlbi_aside1` 已覆盖——单核无他核 stale 面）；发送形态恒
//! `SgiTarget::Others`（天然排自核，双核即对核）。发送计数 [`TX`] 单核
//! 恒 0（等价镜实证项）。

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// 双核形态（QEMU virt 主验；S-3b per-CPU 基建同界）。v2 扩核 = 目标集
/// 位图 + 逐核快照数组（§3.3 位图原案）。
const MAX_CORES: usize = 2;

/// per-核回执计数（收端 handler 自增；发端等待「> 快照」）。
static ACKS: [AtomicU32; MAX_CORES] = [AtomicU32::new(0), AtomicU32::new(0)];

/// SGI #2 实发送计数（等价镜：单核相恒 0——短路路径不增）。
static TX: AtomicU32 = AtomicU32::new(0);

/// per-CPU 待等闩（broadcast 置位；dispatch 尾等待毕清零）。
static PENDING: [AtomicBool; MAX_CORES] = [AtomicBool::new(false), AtomicBool::new(false)];

/// per-CPU 快照（broadcast 时点对核 ACKS 值）。
static PEND_SNAP: [AtomicU32; MAX_CORES] = [AtomicU32::new(0), AtomicU32::new(0)];

/// 回卷代数（aspace ASID 锁窗内 bump——每回卷 +1；代数序 = 取号序）。
static ROLL_GEN: AtomicU32 = AtomicU32::new(0);

/// 已 quiesce（全核回执等毕）的最高代数（单调只升——fetch_max）。
static QUIESCED_GEN: AtomicU32 = AtomicU32::new(0);

/// per-CPU 回卷待驱动闩（arm 置位；quiesce_check 等毕清零——单核恒不置）。
static ROLL_PENDING: [AtomicBool; MAX_CORES] = [AtomicBool::new(false), AtomicBool::new(false)];

/// 收端半边（trap.rs SGI #2 臂调）：本地全量失效 + `dsb ish`（失效完成
/// 先于回执可观的排序边）+ 回执计数自增。**取锁面恒零、零打印零分配**
/// （IRQ 上下文纪律，SGI_SEEN 同款；§3.2 无环判据的收端半边）。
pub(crate) fn irq_ack() {
    // SAFETY: 本核 TLB 失效——IRQ 上下文，无共享结构触达。
    unsafe { crate::mmu::flush_local_all() };
    let me = crate::smp::cpu_id();
    ACKS[me].fetch_add(1, Ordering::Release);
}

/// 裸发送（SGI #2 Others + 计数）——broadcast 的无闩内核件，bringup
/// probe ④ 专用（生产面走 [`broadcast`]：闩 + dispatch 尾等待）。
pub(crate) fn send_others() {
    // SAFETY: MMIO 写（SGI 生成寄存器，整字写钉板）；调用语境 IRQ 屏蔽
    //（SVC 窗 / boot DAIF 窗）或可重入 IRQ——发送不等收。
    unsafe {
        crate::gic::send_sgi(crate::gic::SGI_TLB_SHOOTDOWN, crate::gic::SgiTarget::Others)
    };
    TX.fetch_add(1, Ordering::Relaxed);
}

/// 发端广播原语（4-2 起 vmar unmap 在 PT 锁窗内调，**广播即返不等待**——
/// 等待由 dispatch 尾 [`sched_exit_check`] 在包络外完成）。调用方合同：
/// ①页表改毕 + `dsb ish` 已做（本核视角失效 + 全核可见序）；②经 dispatch
/// 返回的 SVC 语境（闩有唯一消费点）。单核短路：`par_enabled()==false`
/// 零发送零闩直返。
pub fn broadcast() {
    if !crate::smp::par_enabled() {
        return;
    }
    let me = crate::smp::cpu_id();
    let t = 1 - me; // 双核 Others 目标 = 对核（自核排除 ④）
    let snap = ACKS[t].load(Ordering::Acquire);
    PEND_SNAP[me].store(snap, Ordering::Relaxed);
    PENDING[me].store(true, Ordering::Release);
    // 快照→发送不可重排（SeqCst 栅）：若发送先于快照可观，快照可能已含
    // 本轮回执 → 等待谓词（> 快照）永不满足 = 悬死。
    core::sync::atomic::fence(Ordering::SeqCst);
    send_others();
}

/// 闩等待（per-CPU）：自旋至对核回执计数严格大于快照，清闩。**调用点必须
/// 零锁持有**（§3.2——dispatch 尾 = `svc_exit_class` 后，KL+结构锁已全放）。
///
/// 有界自旋 fail-loud 探针（400M 圈 ≈ TCG 下秒级，远超正常回执 µs-ms 级）：
/// 本等待点真楔死 = 静默僵尸核（无 panic 无串口），探针把静默翻成 panic
/// 自报名全状态。诊断史（2026-11-03 尸检修订版）：三连复现的「单核 B 洪流 +
/// ai 家族饿死」病灶**非本协议楔面**——真根 = ai_ping wait deadline 单位
/// 错配（CNTVCT 周期数喂 ns 语义参数，有界等待退化为瞬时超时 poll；雷册
/// §A118 修订版），本探针全程零触发（阴性证）。保留作回执链真楔面守卫；
/// 对飞面 = 4-4 压测相法定探针（模块头「已知对飞面」节）。
fn wait_pending(me: usize) {
    let t = 1 - me;
    let snap = PEND_SNAP[me].load(Ordering::Relaxed);
    let mut spins: u32 = 0;
    while ACKS[t].load(Ordering::Acquire) <= snap {
        spins = spins.wrapping_add(1);
        if spins >= 400_000_000 {
            panic!(
                "tlbshoot: 核{me} 回执等待楔死 snap={snap} acks[t]={} tx={} pend=({},{})",
                ack_count(t),
                tx_count(),
                PENDING[0].load(Ordering::Acquire),
                PENDING[1].load(Ordering::Acquire)
            );
        }
        core::hint::spin_loop();
    }
    PENDING[me].store(false, Ordering::Release);
}

/// dispatch 尾等待点（arch syscall.rs `dispatch` 在 `svc_exit_class` 后调）：
/// 本核有待等闩才自旋（NONE 类号多付一次 Acquire 原子读；单核闩恒 0）。
pub(crate) fn sched_exit_check() {
    let me = crate::smp::cpu_id();
    if PENDING[me].load(Ordering::Acquire) {
        wait_pending(me);
    }
}

// ---- S-4 4-3：ASID 回卷 quiesce（aspace.rs 计数器锁外的驱动协议）----

/// 回卷代数 +1（**aspace ASID 锁窗内**调用——与取号同临界区，代数序 =
/// 取号序；锁序第六层位最深处，持锁窗内无任何等待）。
pub fn roll_gen_bump() {
    ROLL_GEN.fetch_add(1, Ordering::Release);
}

/// 回卷 quiesce 武装（aspace ASID 锁**外**调用，`rolled==true` 时恰一次）：
/// 本核全量失效（`vmalle1is`——本核旧代翻译即刻清）+ [`broadcast`]（SGI #2
/// 对核全量失效，广播即返）+ 置本核待驱动闩。**本函数绝不等待**——等待归
/// [`roll_quiesce_check`]（SVC 路 dispatch 尾自动；内核任务路显式）。单核
/// 短路：仅本核失效（无他核 stale 面），代数直接追平（闩不置——单核零
/// 漂移，与 4-3 前行为等价）。
pub fn roll_quiesce_arm() {
    // 本核全量 TLB 失效（安全封装——非 unsafe 面）；调用语境 = alloc 返回路
    //（零锁）或 boot 屏蔽窗——无共享结构触达。
    crate::flush_all_asid();
    if !crate::smp::par_enabled() {
        QUIESCED_GEN.fetch_max(ROLL_GEN.load(Ordering::Acquire), Ordering::Release);
        return;
    }
    ROLL_PENDING[crate::smp::cpu_id()].store(true, Ordering::Release);
    broadcast();
}

/// 回卷 quiesce 驱动（闩驱动——无闩一次 Acquire 读直返；**调用点必须零锁**，
/// §3.2 同款纪律）：循环「快照代数 → broadcast → 等回执 → `QUIESCED`
/// 追平」至追上 `ROLL`。SVC 路由 dispatch 尾（`sched_exit_check` 后）与
/// 内核任务显式调用共用；双核并发驱动同格收敛——每轮 broadcast 各自快照
/// 各自等（恰一归因不变），`QUIESCED` 单调只升。等待期间他核又回卷则
/// `ROLL` 再进，循环条件重检再驱动一轮（有界：回卷间隔 ≫ 回执往返）。
pub fn roll_quiesce_check() {
    let me = crate::smp::cpu_id();
    if !ROLL_PENDING[me].load(Ordering::Acquire) {
        return;
    }
    while QUIESCED_GEN.load(Ordering::Acquire) < ROLL_GEN.load(Ordering::Acquire) {
        let g = ROLL_GEN.load(Ordering::Acquire);
        broadcast();
        sched_exit_check();
        QUIESCED_GEN.fetch_max(g, Ordering::Release);
    }
    ROLL_PENDING[me].store(false, Ordering::Release);
}

// ---- 观测面（probe ④ / ktest / 等价镜）----

/// 指定核回执计数（probe ④ 的 wait_secs 轮询谓词源）。
pub fn ack_count(core: usize) -> u32 {
    ACKS[core].load(Ordering::Acquire)
}

/// SGI #2 实发送计数（等价镜实证项：单核相恒 0）。
pub fn tx_count() -> u32 {
    TX.load(Ordering::Relaxed)
}

/// 回卷代数（4-3 相 4 断言面：force_near_roll 后须 ≥1）。
pub fn roll_gen() -> u32 {
    ROLL_GEN.load(Ordering::Acquire)
}

/// 已 quiesce 最高代数（4-3 相 4 断言面：驱动毕须追平 roll_gen）。
pub fn quiesced_gen() -> u32 {
    QUIESCED_GEN.load(Ordering::Acquire)
}
