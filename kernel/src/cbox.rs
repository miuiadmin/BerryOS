//! cbox —— 黑匣子崩溃记录表（`.agents/plans/cbox/design.md` v0.1.0 刀 1）。
//!
//! 内核异常 fatal 路径（EL0 kill 链 / EL1 halt 前，经 HAL 槽 [`on_fault`]）把
//! 故障帧镜像进本表（RAM 环形 8 槽）；用户态经 `berry_cbox_drain`（ABI 0x42）
//! 取全量快照落盘 FAT 账本。与串口 dump **双写并存**：串口全寄存器深度取证，
//! 本表是指纹聚合与跨监督面存活的证据源（串口 canary 交错实测撕毁约四成——
//! 账本存在理由，`.agents/notes/implemented/2026-11-15-svcmgr-wtdie-selftrap-forensics.md`
//! 可复用取证节）。
//!
//! 写入纪律（异常上下文，PSTATE.I=1）：**零分配、零锁、零 uart**——槽位经
//! `CBOX_NEXT.fetch_add` 分配（NCPUS=2 双核可同时 fault，各自拿槽）；撕裂防护
//! 用每槽 `stamp` 轻量 seqlock（奇 = 写入中：写者先抬奇、写域、再落偶 Release；
//! 读者见奇或复读变值 = 以 magic=0 哨兵记录告知排液方跳过）。
//!
//! 排液语义 = **快照非消费**：记录永驻至槽位轮转；排液方自持 last_seq 去重。

use berry_hal_aarch64::timer::cntvct;
use berry_hal_aarch64::trap::TrapFrame;
use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::Ordering;

/// 记录 magic（"CBOX" 小端 = 0x58424F43）。
pub const REC_MAGIC: u32 = 0x5842_4F43;
/// 记录格式版本（尾部追加字段 = ver 递增，禁中插）。
pub const REC_VER: u8 = 1;

/// 单条崩溃记录（`#[repr(C)]` 定宽；排液原样拷贝 = 用户态 ABI 形状 day-one
/// 定型，偏移由下方 const assert 钉板）。
#[derive(Clone, Copy)]
#[repr(C)]
pub struct CboxRec {
    pub magic: u32,
    /// 1 = EL0 sync（用户任务 fault）；2 = EL1 sync（内核 fault）。
    pub kind: u8,
    pub ver: u8,
    pub rsvd: u16,
    /// 全局单调写序（1 起；0 保留）。
    pub seq: u32,
    /// 槽代序（seqlock 稳态偶 / 写入中奇；见模块头注）。
    pub stamp: u32,
    pub vct: u64,
    /// 任务名 NUL 填充（CURRENT 空态 = `(boot)`）。
    pub name: [u8; 16],
    /// ELR。
    pub pc: u64,
    pub far: u64,
    pub esr: u64,
    /// x30——回溯真实调用点（WTDIE 定谳实证的主字段）。
    pub lr: u64,
    pub sp: u64,
    /// callee-saved 三元组（WTDIE 案 x19=sig / x20=h / x21=e 取证先例）。
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
}

// 编译期钉布局（TrapFrame 同款纪律——防 #[repr(C)] 错位漂移；spec 表同源）。
const _: () = assert!(core::mem::size_of::<CboxRec>() == 104);
const _: () = assert!(core::mem::offset_of!(CboxRec, magic) == 0);
const _: () = assert!(core::mem::offset_of!(CboxRec, seq) == 8);
const _: () = assert!(core::mem::offset_of!(CboxRec, stamp) == 12);
const _: () = assert!(core::mem::offset_of!(CboxRec, vct) == 16);
const _: () = assert!(core::mem::offset_of!(CboxRec, name) == 24);
const _: () = assert!(core::mem::offset_of!(CboxRec, pc) == 40);
const _: () = assert!(core::mem::offset_of!(CboxRec, x21) == 96);

/// 排液快照头（12B，设计稿 ABI 节：`ver / total_ever / stored`）。
#[derive(Clone, Copy)]
#[repr(C)]
pub struct CboxHdr {
    pub ver: u32,
    pub total: u32,
    pub stored: u32,
}
const _: () = assert!(core::mem::size_of::<CboxHdr>() == 12);

/// 环形槽容量（静态预储 .bss，boot 后永不增长——telemetry 环同款纪律）。
const SLOTS: usize = 8;

const REC_ZERO: CboxRec = CboxRec {
    magic: 0,
    kind: 0,
    ver: REC_VER,
    rsvd: 0,
    seq: 0,
    stamp: 0,
    vct: 0,
    name: [0; 16],
    pc: 0,
    far: 0,
    esr: 0,
    lr: 0,
    sp: 0,
    x19: 0,
    x20: 0,
    x21: 0,
};

// SAFETY 契约：写侧仅异常上下文（PSTATE.I=1；seq 原子分槽 + stamp seqlock）；
// 读侧 syscall 上下文（无锁快照——stamp 复读兜底）。
static mut CBOX_TBL: [CboxRec; SLOTS] = [REC_ZERO; SLOTS];
/// 全局单调写序（既是 seq 源也是槽分配器；0 = 未写过）。
static CBOX_NEXT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// HAL 槽入口（`set_cbox_hook` 注册；trap.rs fatal 路调）。kind 语义见
/// `berry_hal_aarch64::trap::{KIND_EL0, KIND_EL1}`。
///
/// # Safety
/// 同步异常上下文（PSTATE.I=1）；本函数零分配零锁零 uart（异常纪律）。
pub unsafe extern "C" fn on_fault(kind: u8, tf: *const TrapFrame) {
    let seq = CBOX_NEXT.fetch_add(1, Ordering::Relaxed) + 1;
    let slot = ((seq as usize - 1) % SLOTS) as usize;
    let tf = &*tf;
    let mut name = [0u8; 16];
    match berry_sched::current_task_name() {
        Some(n) => {
            let n = &n[..n.len().min(16)];
            name[..n.len()].copy_from_slice(n);
        }
        None => name[..6].copy_from_slice(b"(boot)"),
    }
    // SAFETY: 写侧异常上下文分槽独占（seq 原子——双核同时 fault 各自拿槽；
    // 本槽上一读者的排液撕裂由 stamp seqlock 兜底）。
    let tbl = addr_of_mut!(CBOX_TBL);
    let r: &mut CboxRec = unsafe { &mut (*tbl)[slot] };
    // seqlock 开写：先抬奇（排液方见奇即跳）；fence 防域 store 先于奇标可见。
    let odd = (r.stamp.wrapping_add(1)) | 1;
    // SAFETY: 定宽对齐 u32 单标 store。
    unsafe { core::ptr::write_volatile(&mut r.stamp, odd) };
    core::sync::atomic::fence(Ordering::Release);
    // SAFETY: 域 store 原子性由定宽对齐保证；整记录一次写（stamp 同落奇值）。
    unsafe {
        core::ptr::write_volatile(
            r as *mut CboxRec,
            CboxRec {
                magic: REC_MAGIC,
                kind,
                ver: REC_VER,
                rsvd: 0,
                seq,
                stamp: odd,
                vct: cntvct(),
                name,
                pc: tf.elr,
                far: tf.far,
                esr: tf.esr,
                lr: tf.regs[30],
                sp: tf.sp,
                x19: tf.regs[19],
                x20: tf.regs[20],
                x21: tf.regs[21],
            },
        );
    }
    // seqlock 收口：落偶（Release fence——域 store 对读者先于偶标可见；读者
    // 侧 Acquire 复读配对）。
    core::sync::atomic::fence(Ordering::Release);
    // SAFETY: 同上单标 store。
    unsafe { core::ptr::write_volatile(&mut r.stamp, odd.wrapping_add(1)) };
}

/// 排液：`emit` 依序收字节片（头 12B + 存量记录按 seq 升序；每片独立完整，
/// 拷贝语义由 emit 方定）。两遍调用（先尺寸后拷贝）结果一致性由 total 快照
/// 保证——第二遍多出的新记录不进本快照（seq 窗口以首读 total 锚定）。
pub fn drain_with(emit: &mut dyn FnMut(&[u8])) {
    let total = CBOX_NEXT.load(Ordering::Acquire);
    let stored = total.min(SLOTS as u32);
    let hdr = CboxHdr {
        ver: REC_VER as u32,
        total,
        stored,
    };
    // SAFETY: repr(C) 定宽 POD 转字节片。
    emit(unsafe { core::slice::from_raw_parts(addr_of!(hdr) as *const u8, 12) });
    if total == 0 {
        return;
    }
    let first = total - stored + 1;
    // SAFETY: 读侧 syscall 上下文；写侧异常上下文（stamp seqlock——见模块头）。
    let tbl: &[CboxRec] = unsafe { &*addr_of!(CBOX_TBL) };
    for s in first..=total {
        let slot = ((s - 1) as usize) % SLOTS;
        // 规范 seqlock 读：stamp 读 → Acquire → 域读 → Acquire → stamp 复读；
        // 两读一致且偶 = 稳态快照。任一不满足（写入中/撕裂/槽被覆盖）以
        // magic=0 哨兵记录占位（排液方跳过——seq 序不破）。
        // SAFETY: 定宽对齐 u32 单标读。
        let s0 = unsafe { core::ptr::read_volatile(&tbl[slot].stamp) };
        core::sync::atomic::fence(Ordering::Acquire);
        let mut rec = tbl[slot];
        core::sync::atomic::fence(Ordering::Acquire);
        // SAFETY: 同上单标读。
        let s1 = unsafe { core::ptr::read_volatile(&tbl[slot].stamp) };
        let stable = s0 == s1 && s0 & 1 == 0 && rec.seq == s;
        if !stable {
            rec = REC_ZERO;
            rec.seq = s; // 序占位（排液方按 seq 去重仍一致）
        }
        // SAFETY: 同 hdr。
        emit(unsafe {
            core::slice::from_raw_parts(
                addr_of!(rec) as *const u8,
                core::mem::size_of::<CboxRec>(),
            )
        });
    }
}
