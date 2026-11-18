//! berry-svcface —— SVC 入口锁类位集与号→锁类注册表纯函数面（W3-Q67）。
//!
//! **动机**：SVC 入口锁类分档（S-3c c-2c slice 2b 起）的六类位集常量与
//! 45 号注册表此前只活在两处——`kern_lock.rs` 常量 + `kernel/src/syscall.rs`
//! `install()` 的 45 行 `reg()` 字面量表。HAL/内核 crate 只 aarch64-unknown-none
//! 可编，host 测不可达：类位集与 sched `HELD_*` 的**位镜像一致性**（bit1=SQ
//! 0x02 / bit2=RQ0 0x04 / bit3=RQ1 0x08 / bit4=REAPQ 0x10 / bit5=TABLE 0x20）
//! 此前只有 kernel main 编译期断言执法（aarch64-only），「实装号 NONE 归零
//! 律」「默认槽/越界号唯一 NONE 面」零 host 钉板。本 crate 把类常量与
//! 号→类注册表按值域抽成纯函数 + 表驱动 host 测试，HAL 侧消费同一常量
//! （零第二实现——抽离即唯一事实源，非镜像；sliceface/chanface 同款纪律）。
//!
//! 抽离面（与 HAL/kernel 侧逐字对齐，改这里的语义 = 改 SVC 派发行为）：
//! - [`SVC_CLASS_NONE`]/[`SVC_CLASS_TABLE`]/[`SVC_CLASS_SQ_RQ_TABLE`]/
//!   [`SVC_CLASS_SQ_RQ`]/[`SVC_CLASS_RQ_TABLE`]/[`SVC_CLASS_ALL`]：六类
//!   位集常量单源（自 `kern_lock.rs` 迁此，HAL `pub use` re-export——
//!   kernel main 断言与 HAL 内部用点零改动）。
//! - [`HELD_SQ_BIT`]/[`HELD_RQ0_BIT`]/[`HELD_RQ1_BIT`]/[`HELD_REAPQ_BIT`]/
//!   [`HELD_TABLE_BIT`]：sched `task.rs` HELD_* 位值的**判据面镜像**（非
//!   第二实现——host 侧拿不到 berry-sched，此处钉「类位集对 sched 位值的
//!   组装律」，与 kernel main.rs 编译期断言同一语义的 host 可测形）。
//! - [`REGISTRY`]：45 号注册表（`(nr, class)` 对——与 kernel `install()`
//!   逐行对应；`0x83 policy_bind` 字面量号同款入册）。
//! - [`class_of`]：号→类查询——注册表线性扫（45 项，boot 期注册语义的
//!   纯投影；**未注册号 → [`SVC_CLASS_NONE`]**，与 HAL 派发表默认槽
//!   `(sys_unimplemented, NONE)` 全表初始化同一律）。
//! - [`NONE 号归零律`]由 [`none_registered`] 执法：注册表内不存在 NONE 类
//!   项（「NONE 实装号归零」纪律——凡 copy_out/copy_in 链或任何共享面
//!   禁落 NONE，§A113 读者面；默认槽/越界号是唯一 NONE 面）。
//!
//! 物理臂留壳（本面不钉的）：HAL `SYSCALLS` static mut 派发表本体、boot
//! 单线程 `install()` 写入、dispatch 只读快照——那是「表怎么填」的机械，
//! 本面钉「表该是什么」的数定律。

#![cfg_attr(not(test), no_std)]

/// 纯表号（0x02/0x03/0x3A/0x61/0x86/0x87/0x88/0x83 等）：仅 KL。
pub const SVC_CLASS_NONE: u8 = 0x00;
/// 纯表号（vmo/vmar 十一号 + attr/join/suspend/power/policy 等）：仅 KL。
pub const SVC_CLASS_TABLE: u8 = 0x20;
/// wait/send 族（0x01/0x04/0x05/0x06/0x52/0x53/0x21/0x23/0x24）：SQ+RQ+表。
pub const SVC_CLASS_SQ_RQ_TABLE: u8 = 0x2E;
/// sleep/yield 双号（0x37/0x3F）：SQ+RQ 双核升序、无表无 KL（0x3F 尾段 `reap_drain` 顺向取 REAPQ→TABLE，禁预持 TABLE 防 4→3 逆序）。
pub const SVC_CLASS_SQ_RQ: u8 = 0x0E;
/// start 双号（0x32/0x35）：RQ 双核升序 + 表。
pub const SVC_CLASS_RQ_TABLE: u8 = 0x2C;
/// 全谱（0x33/0x36/0x3B + create 族 0x31/0x34/0x51/0x54）。
pub const SVC_CLASS_ALL: u8 = 0x3E;

// ---- sched HELD_* 位值镜像（判据面） ----
// 位值与 `sched/src/task.rs` HELD_* 恒等（kernel main.rs 编译期断言执法
// 的 host 可测形——本 crate 不依赖 berry-sched，位值在此冻结钉板；两侧
// 漂移 = main 断言红 + 此处镜像测红双执法）。

/// SLEEPQ 持有位（sched `HELD_SQ = 1 << 1`）。
pub const HELD_SQ_BIT: u8 = 1 << 1;
/// CPU0 runqueue 持有位（sched `HELD_RQ0 = 1 << 2`）。
pub const HELD_RQ0_BIT: u8 = 1 << 2;
/// CPU1 runqueue 持有位（sched `HELD_RQ1 = 1 << 3`）。
pub const HELD_RQ1_BIT: u8 = 1 << 3;
/// REAPQ 持有位（sched `HELD_REAPQ = 1 << 4`）。
pub const HELD_REAPQ_BIT: u8 = 1 << 4;
/// 表锁持有位（sched `HELD_TABLE = 1 << 5`）。
pub const HELD_TABLE_BIT: u8 = 1 << 5;

/// 类位集按 sched HELD_* 位值组装（组装律单源——kernel main.rs 六断言
/// 的函数形）：TABLE = 仅表位；SQ_RQ = SQ+RQ0+RQ1；RQ_TABLE = RQ 双 + 表；
/// SQ_RQ_TABLE = 前者并 SQ；ALL = 全五位。REAPQ 位只在 ALL 出现（实装
/// 类无一单独取 REAPQ——kill/exit 链经全谱包络）。
pub const fn assembled_class(sq: bool, rq0: bool, rq1: bool, reapq: bool, table: bool) -> u8 {
    let mut v = 0u8;
    if sq {
        v |= HELD_SQ_BIT;
    }
    if rq0 {
        v |= HELD_RQ0_BIT;
    }
    if rq1 {
        v |= HELD_RQ1_BIT;
    }
    if reapq {
        v |= HELD_REAPQ_BIT;
    }
    if table {
        v |= HELD_TABLE_BIT;
    }
    v
}

/// 号→锁类注册表：与 kernel `syscall.rs install()` 的 45 行 `reg()` 逐行
/// 对应（改 kernel 落类 = 同步改此表——host 钉板即执法面）。号值 = ABI
/// v1/v3 真号（abi_gen NR_* / 0x83 字面量）。
pub const REGISTRY: &[(u8, u8)] = &[
    // —— SQ+RQ+TABLE（0x2E）：wait/send 族（wake→enqueue 双结构窗 + 表）——
    (0x01, SVC_CLASS_SQ_RQ_TABLE), // handle_close（close→block 潜在睡眠面）
    (0x02, SVC_CLASS_TABLE),       // handle_duplicate
    (0x03, SVC_CLASS_TABLE),       // handle_replace
    (0x04, SVC_CLASS_SQ_RQ_TABLE), // object_wait_one
    (0x05, SVC_CLASS_SQ_RQ_TABLE), // object_wait_many
    (0x06, SVC_CLASS_SQ_RQ_TABLE), // object_signal
    (0x61, SVC_CLASS_TABLE),       // clock_read（copy_out 链 §A113 读者面升 TABLE）
    // —— TABLE（0x20，仅 KL）：vmo/vmar 十一号（S-4 4-2 自 ALL 降档，§A115）——
    (0x11, SVC_CLASS_TABLE), // vmo_create
    (0x12, SVC_CLASS_TABLE), // vmo_commit
    (0x13, SVC_CLASS_TABLE), // vmo_decommit
    (0x14, SVC_CLASS_TABLE), // vmo_read
    (0x15, SVC_CLASS_TABLE), // vmo_write
    (0x1A, SVC_CLASS_TABLE), // vmo_get_size
    (0x1B, SVC_CLASS_TABLE), // vmo_get_phys
    (0x16, SVC_CLASS_TABLE), // vmar_map
    (0x17, SVC_CLASS_TABLE), // vmar_unmap
    (0x18, SVC_CLASS_TABLE), // vmar_protect
    (0x19, SVC_CLASS_TABLE), // vmar_allocate
    // —— SQ+RQ+TABLE：pager 供给三号（阻塞收货 + 唤 faulter + 双表门控）——
    (0x21, SVC_CLASS_SQ_RQ_TABLE), // pager_supply_pages
    (0x23, SVC_CLASS_SQ_RQ_TABLE), // pager_supply_mapped
    (0x24, SVC_CLASS_SQ_RQ_TABLE), // pager_supply_mapped_table
    // —— 生命周期族 ——
    (0x31, SVC_CLASS_ALL),       // process_create（+P3-3 per-parent 配额面）
    (0x32, SVC_CLASS_RQ_TABLE),  // process_start
    (0x33, SVC_CLASS_ALL),       // process_exit（-> ! 舞蹈收口）
    (0x34, SVC_CLASS_ALL),       // thread_create
    (0x35, SVC_CLASS_RQ_TABLE),  // thread_start
    (0x36, SVC_CLASS_ALL),       // thread_exit（-> !）
    (0x37, SVC_CLASS_SQ_RQ),     // thread_sleep（slice 3-1a 降档：纯睡眠面）
    (0x3A, SVC_CLASS_TABLE),     // process_get_exit_code
    (0x3B, SVC_CLASS_ALL),       // process_terminate（kill 链同 fault 面）
    (0x83, SVC_CLASS_TABLE),     // policy_bind（S6-b：copy_from_user + SNAPS 单写者）
    (0x88, SVC_CLASS_TABLE),     // power_off（桌面UX刀D：保守档）
    // —— attr/suspend/yield ——
    (0x3C, SVC_CLASS_TABLE),      // sched_setattr
    (0x3D, SVC_CLASS_TABLE),      // sched_getattr
    (0x3E, SVC_CLASS_TABLE),      // process_suspend（表门控 + freeze 扫描）
    (0x3F, SVC_CLASS_SQ_RQ),      // thread_yield（无表零指针；尾段 reap_drain 取 REAPQ(3)→TABLE(4)，严禁入口预持 TABLE(4) 逆序）
    // —— TABLE：CP-08 util 快照读（0x40——EWMA 槽 Relaxed 原子读零共享面；
    //    copy_out 链走查调用方页表 = §A113 读者面，同 0x61/0x87）。
    (0x40, SVC_CLASS_TABLE),      // sched_util_snapshot
    // —— TABLE：cbox 刀 1 排液（0x42——黑匣子快照表读，copy_out 读者面 §A113）——
    (0x42, SVC_CLASS_TABLE),      // cbox_drain
    // —— channel/event 族 ——
    (0x51, SVC_CLASS_ALL),          // channel_create
    (0x52, SVC_CLASS_SQ_RQ_TABLE),  // channel_send
    (0x53, SVC_CLASS_SQ_RQ_TABLE),  // channel_recv
    (0x54, SVC_CLASS_ALL),          // event_create
    // —— 信号面（A7-4：0x86 表查 + pending 原子；0x87 copy_out 链升 TABLE）——
    (0x86, SVC_CLASS_TABLE), // signal_send
    (0x87, SVC_CLASS_TABLE), // signal_take
    // —— TABLE：阶段五遥测环读（0x84——klog 快照首域 copy_out 读者面 §A113）——
    (0x84, SVC_CLASS_TABLE),      // telemetry_open
];

/// 号→入口锁类：注册表线性扫（45 项·boot 期注册语义纯投影）。**未注册号
/// → [`SVC_CLASS_NONE`]**——与 HAL `SYSCALLS` 默认槽全表 NONE 初始化同一
/// 律（未知/未实现/越界号零锁直达 `-ENOTSUP`，绝不 panic 绝不杀调用者）。
pub const fn class_of(nr: u8) -> u8 {
    let mut i = 0;
    while i < REGISTRY.len() {
        if REGISTRY[i].0 == nr {
            return REGISTRY[i].1;
        }
        i += 1;
    }
    SVC_CLASS_NONE
}

/// NONE 实装号归零律：注册表内不存在 NONE 类项（「NONE 实装号归零」纪律
/// ——凡 copy_out/copy_in 出参链〔walk_leaf 页表读者〕或任何共享面禁落
/// NONE，§A113 读者面；默认槽/越界号是唯一 NONE 面）。
pub const fn none_registered() -> bool {
    let mut i = 0;
    while i < REGISTRY.len() {
        if REGISTRY[i].1 == SVC_CLASS_NONE {
            return false;
        }
        i += 1;
    }
    true
}

/// 注册表号唯一律（无重复号——重复注册 = 后写覆盖前写，boot 期静默丢
/// handler 的编码错误面；install() 不查重，本面钉板执法）。
pub const fn nrs_unique() -> bool {
    let mut i = 0;
    while i < REGISTRY.len() {
        let mut j = i + 1;
        while j < REGISTRY.len() {
            if REGISTRY[i].0 == REGISTRY[j].0 {
                return false;
            }
            j += 1;
        }
        i += 1;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 类位集六常量对 sched HELD_* 位值的组装律（kernel main.rs 六断言的
    /// host 可测形——位值漂移双执法面）。
    #[test]
    fn class_bits_mirror_held_assembly() {
        assert_eq!(SVC_CLASS_NONE, 0x00);
        assert_eq!(SVC_CLASS_TABLE, assembled_class(false, false, false, false, true));
        assert_eq!(
            SVC_CLASS_SQ_RQ,
            assembled_class(true, true, true, false, false)
        );
        assert_eq!(
            SVC_CLASS_RQ_TABLE,
            assembled_class(false, true, true, false, true)
        );
        assert_eq!(
            SVC_CLASS_SQ_RQ_TABLE,
            assembled_class(true, true, true, false, true)
        );
        assert_eq!(
            SVC_CLASS_ALL,
            assembled_class(true, true, true, true, true)
        );
        // 位值冻结锚：bit1=SQ / bit2=RQ0 / bit3=RQ1 / bit4=REAPQ / bit5=TABLE。
        assert_eq!(HELD_SQ_BIT, 0x02);
        assert_eq!(HELD_RQ0_BIT, 0x04);
        assert_eq!(HELD_RQ1_BIT, 0x08);
        assert_eq!(HELD_REAPQ_BIT, 0x10);
        assert_eq!(HELD_TABLE_BIT, 0x20);
        assert_eq!(SVC_CLASS_SQ_RQ_TABLE, 0x2E);
        assert_eq!(SVC_CLASS_SQ_RQ, 0x0E);
        assert_eq!(SVC_CLASS_RQ_TABLE, 0x2C);
        assert_eq!(SVC_CLASS_ALL, 0x3E);
    }

    /// REAPQ 位只在 ALL 出现：实装类无一单独取 REAPQ（kill/exit 链经全谱
    /// 包络——svc_enter 全量形同 ALL）。
    #[test]
    fn reapq_bit_only_in_all() {
        for &(nr, class) in REGISTRY {
            if class & HELD_REAPQ_BIT != 0 {
                assert_eq!(class, SVC_CLASS_ALL, "nr={nr:#04x} REAPQ 位须 ALL 档");
            }
        }
    }

    /// 表规模与形状：45 号在册、无重复号（cbox 刀 1 0x42 + 阶段五遥测 0x84）。
    #[test]
    fn registry_shape() {
        assert_eq!(REGISTRY.len(), 45);
        assert!(nrs_unique());
    }

    /// NONE 实装号归零律：注册表零 NONE 项。
    #[test]
    fn no_installed_none_class() {
        assert!(none_registered());
    }

    /// class_of 全表往返：注册号恰返在册类、未注册号一律 NONE。
    #[test]
    fn class_of_roundtrip_and_default() {
        for &(nr, class) in REGISTRY {
            assert_eq!(class_of(nr), class, "nr={nr:#04x} 往返失配");
        }
        // 未注册面：v1 预留 0x85 attest、调试槽退役位 0xFE/0xFF、随机空号
        //（0x40 CP-08 util 通道入册后移出此列——SVC_CLASS_TABLE）。
        for nr in [0x00u8, 0x07, 0x08, 0x10, 0x20, 0x25, 0x30, 0x38, 0x39,
            0x50, 0x55, 0x60, 0x62, 0x70, 0x80, 0x82, 0x85, 0x89,
            0x8A, 0x90, 0xA0, 0xFE, 0xFF] {
            assert_eq!(class_of(nr), SVC_CLASS_NONE, "nr={nr:#04x} 须 NONE 默认");
        }
    }

    /// 敌意全域扫：0..=255 逐号 class_of 零 panic、值域 ⊆ {六类常量}。
    #[test]
    fn class_of_full_domain_bounded() {
        let legal = [
            SVC_CLASS_NONE,
            SVC_CLASS_TABLE,
            SVC_CLASS_SQ_RQ,
            SVC_CLASS_RQ_TABLE,
            SVC_CLASS_SQ_RQ_TABLE,
            SVC_CLASS_ALL,
        ];
        let mut none_cnt = 0;
        let mut hit_cnt = 0;
        for nr in 0u8..=255 {
            let c = class_of(nr);
            assert!(legal.contains(&c), "nr={nr:#04x} 越法定类域 {c:#04x}");
            if c == SVC_CLASS_NONE {
                none_cnt += 1;
            } else {
                hit_cnt += 1;
            }
        }
        // 恰 45 号命中、211 号 NONE（256-45）。
        assert_eq!(hit_cnt, REGISTRY.len() as u16);
        assert_eq!(none_cnt, 256 - REGISTRY.len() as u16);
    }

    /// 落类面锚点抽查（§A113/§A115 判例形状）：clock_read/signal_take/
    /// power_off/policy_bind = TABLE；thread_sleep = SQ_RQ（无表无 KL）；
    /// vmo 十一号 = TABLE（S-4 降档后）；create 族四号 = ALL。
    #[test]
    fn anchor_classes() {
        assert_eq!(class_of(0x61), SVC_CLASS_TABLE); // clock_read
        assert_eq!(class_of(0x87), SVC_CLASS_TABLE); // signal_take
        assert_eq!(class_of(0x88), SVC_CLASS_TABLE); // power_off
        assert_eq!(class_of(0x83), SVC_CLASS_TABLE); // policy_bind
        assert_eq!(class_of(0x37), SVC_CLASS_SQ_RQ); // thread_sleep
        assert_eq!(class_of(0x3F), SVC_CLASS_SQ_RQ); // thread_yield
        for nr in [0x11u8, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B] {
            assert_eq!(class_of(nr), SVC_CLASS_TABLE, "vmo/vmar {nr:#04x}");
        }
        for nr in [0x31u8, 0x33, 0x34, 0x36, 0x3B, 0x51, 0x54] {
            assert_eq!(class_of(nr), SVC_CLASS_ALL, "create/exit/kill {nr:#04x}");
        }
        for nr in [0x32u8, 0x35] {
            assert_eq!(class_of(nr), SVC_CLASS_RQ_TABLE, "start {nr:#04x}");
        }
        for nr in [0x01u8, 0x04, 0x05, 0x06, 0x21, 0x23, 0x24, 0x52, 0x53] {
            assert_eq!(class_of(nr), SVC_CLASS_SQ_RQ_TABLE, "wait/send {nr:#04x}");
        }
    }
}
