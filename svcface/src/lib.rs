//! berry-svcface —— SVC 入口锁类位集与号→锁类注册表纯函数面（W3-Q67）。
//!
//! **动机**：SVC 入口锁类分档（S-3c c-2c slice 2b 起）的六类位集常量与
//! 45 号注册表此前只活在两处——`kern_lock.rs` 常量 + `kernel/src/syscall.rs`
//! `install()` 的 52 行 `reg()` 字面量表。HAL/内核 crate 只 aarch64-unknown-none
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
//! - [`REGISTRY`]：52 号注册表（`(nr, class)` 对——与 kernel `install()`
//!   逐行对应；`0x83 policy_bind` 字面量号同款入册）。
//! - [`class_of`]：号→类查询——注册表线性扫（52 项，boot 期注册语义的
//!   纯投影；**未注册号 → [`SVC_CLASS_NONE`]**，与 HAL 派发表默认槽
//!   `(sys_unimplemented, NONE)` 全表初始化同一律）。
//! - [`NONE 号后继审计律`]由 [`none_class_audited`] 执法：在册 NONE 实装
//!   号集恰等 [`NONE_DOWNGRADED_NRS`] 白名单（W3-Q100 起 0x61/0x40/0x87
//!   过 §A115 四问 PT 锁后继审计降档——copy 链 §A113 读者义务已由
//!   per-AS PT 锁同窗接管；白名单外 NONE 实装号即违规）。
//!
//! 物理臂留壳（本面不钉的）：HAL `SYSCALLS` static mut 派发表本体、boot
//! 单线程 `install()` 写入、dispatch 只读快照——那是「表怎么填」的机械，
//! 本面钉「表该是什么」的数定律。

#![cfg_attr(not(test), no_std)]

/// NONE（0x00）= 零锁零槽直达 handler。未注册/未实现/越界号默认；实装
/// 号须过 §A115 四问 PT 锁后继审计才可落此档（W3-Q100 首批三平凡号
/// 0x61/0x87/0x40——copy 链 §A113 读者义务已由 per-AS PT 锁同窗接管〔S-4
/// 4-2〕，KL 再无剩余义务；归零律退役为后继审计律，见 [`none_downgraded_nrs`]）。
pub const SVC_CLASS_NONE: u8 = 0x00;
/// 纯表号（vmo/vmar 十一号 + attr/join/suspend/power/policy 等）：仅 KL。
pub const SVC_CLASS_TABLE: u8 = 0x20;
/// wait/send 族（0x01/0x04/0x05/0x06/0x52/0x53/0x21/0x23/0x24）：SQ+RQ+表。
pub const SVC_CLASS_SQ_RQ_TABLE: u8 = 0x2E;
/// thread_sleep 单号（0x37）：SQ+RQ 双核升序、无表无 KL（slice 3-1a 降档）。
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

/// 号→锁类注册表：与 kernel `syscall.rs install()` 的 52 行 `reg()` 逐行
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
    // —— NONE：clock_read（0x61，W3-Q100 自 TABLE 降档）——CNTVCT/CNTFRQ
    //    硬件读 + rtc 注入面（EPOCH_S/BOOT_MONO_NS Relaxed 原子，无锁需
    //    求）零共享串行面；出参 copy_out_u64 链 = §A113 读者面，S-4 4-2 起
    //    走查与 store 同一 per-AS PT 锁窗（跨核 unmap/protect 写者同窗互
    //    斥）——KL 后继义务已偿清，§A115 四问复审落 NONE（03 号审计机会二）。
    (0x61, SVC_CLASS_NONE),       // clock_read（PT 锁后继审计——W3-Q100 降档）
    // —— TABLE（0x20，仅 KL）：vmo/vmar 十一号（S-4 4-2 自 ALL 降档，§A115）——
    (0x11, SVC_CLASS_TABLE), // vmo_create
    (0x12, SVC_CLASS_TABLE), // vmo_commit
    (0x13, SVC_CLASS_TABLE), // vmo_decommit
    (0x14, SVC_CLASS_TABLE), // vmo_read
    (0x15, SVC_CLASS_TABLE), // vmo_write
    (0x1A, SVC_CLASS_TABLE), // vmo_get_size
    (0x1B, SVC_CLASS_TABLE), // vmo_get_phys
    (0x1C, SVC_CLASS_TABLE), // vmo_cache_op（35 号件 F0-4 非一致 DMA 缓存维护）
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
    (0x3F, SVC_CLASS_RQ_TABLE),   // thread_yield（pick/requeue + 切换舞蹈）
    // —— NONE：CP-08 util 快照读（0x40，W3-Q100 自 TABLE 降档）——EWMA 槽
    //    Relaxed 原子读零共享串行面（统计信号无序角色，同 LB_TICKS 先例）；
    //    copy_to_user 链 = §A113 读者面，S-4 4-2 起 per-AS PT 锁同窗——
    //    §A115 四问复审落 NONE（同 0x61/0x87）。
    (0x40, SVC_CLASS_NONE),      // sched_util_snapshot（W3-Q100 降档）
    // —— TABLE：cbox 刀 1 排液（0x42——黑匣子快照表读，copy_out 读者面 §A113）——
    (0x42, SVC_CLASS_TABLE),      // cbox_drain
    // —— channel/event 族 ——
    (0x51, SVC_CLASS_ALL),          // channel_create
    (0x52, SVC_CLASS_SQ_RQ_TABLE),  // channel_send
    (0x53, SVC_CLASS_SQ_RQ_TABLE),  // channel_recv
    // W3-Q97 批量 recv（机会五）：锁类与单发 recv 完全同面（同
    // caller_table/get_raw/PT 窗/wake_matching），同类挂法。
    (0x57, SVC_CLASS_SQ_RQ_TABLE),  // channel_recv_n
    (0x54, SVC_CLASS_ALL),          // event_create
    // —— 信号面（A7-4：0x86 表查 + pending 原子；0x87 W3-Q100 降 NONE）——
    (0x86, SVC_CLASS_TABLE), // signal_send
    // —— NONE：signal_take（0x87，W3-Q100 自 TABLE 降档）——pending 取走
    //    swap(0) Relaxed 原子零锁需求；出参 copy_out_u64 链 = §A113 读者面，
    //    S-4 4-2 起 per-AS PT 锁同窗——§A115 四问复审落 NONE（同 0x61）。
    (0x87, SVC_CLASS_NONE), // signal_take（W3-Q100 降档）
    // —— TABLE：G2 刀 7 受限态原语双号（0x4A/0x4B）——v1 桩（TABLE：
    //    abi_gen 常量面走查；v2 号位——abi_gen 常量层只产 v1/1.1 行，
    //    字面量挂同 0x83 先例）。
    (0x4A, SVC_CLASS_TABLE), // restricted_enter（G2 刀 7 桩）
    (0x4B, SVC_CLASS_TABLE), // restricted_kick（G2 刀 7 桩）
    // —— TABLE：G2 刀 10 Hypervisor MicroVM 三号（0x77/0x78/0x79）——
    //    v1 桩 -ENOSYS（QEMU TCG 无 EL2；真机 EL2 面归后续刀）。
    (0x77, SVC_CLASS_TABLE), // guest_create（G2 刀 10 桩）
    (0x78, SVC_CLASS_TABLE), // vcpu_create（G2 刀 10 桩）
    (0x79, SVC_CLASS_TABLE), // vcpu_run（G2 刀 10 桩）
    // —— SQ+RQ+TABLE（0x2E）：阶段五遥测环读（0x84——copy_out 读者面
    // §A113 保留叠加；进程账读口 op4 刀整号滚类 TABLE→SQ_RQ_TABLE：op=4
    // 任务集三并集遍历需 SQ+RQ0+RQ1 终态正序预取，thread_sleep 0x37 无 KL
    // 改 SQ 在案 KL 窗裸走有撕链面、持 KL 取结构锁违七层冻结序——入口
    // 整号改类 = 唯一合法构式，op=0..3 连坐 = 入口多三对无争用 TTAS）——
    (0x84, SVC_CLASS_SQ_RQ_TABLE), // telemetry_open
];

/// 号→入口锁类：注册表线性扫（52 项·boot 期注册语义纯投影）。**未注册号
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

/// NONE 实装号后继审计律（W3-Q100 修订——原「归零律」为 S-4 4-2 PT 锁
/// 落地前的保守面）：实装号落 NONE 须过 §A115 四问复审——①读者面数到
/// 底（copy 链 §A113 页表读者义务由 per-AS PT 锁同窗接管 = S-4 4-2 判
/// 例）；②共享面清点（KL 无剩余义务）；③锁序零新增边；④位感知判据
/// 不依赖被撤锁。在册 NONE 实装号 = [`NONE_DOWNGRADED_NRS`] 白名单
/// （逐号带判例注释），白名单外出现 NONE 项即违规。
pub const NONE_DOWNGRADED_NRS: &[u8] = &[0x61, 0x40, 0x87];

/// NONE 实装号恰等白名单律：注册表内 NONE 类号集 == [`NONE_DOWNGRADED_NRS`]
/// （多一号少一号都红——降档须带判例入册，防静默扩散）。
pub const fn none_class_audited() -> bool {
    let mut i = 0;
    let mut seen = [false; NONE_DOWNGRADED_NRS.len()];
    let mut none_cnt = 0;
    while i < REGISTRY.len() {
        if REGISTRY[i].1 == SVC_CLASS_NONE {
            none_cnt += 1;
            let mut j = 0;
            let mut hit = false;
            while j < NONE_DOWNGRADED_NRS.len() {
                if REGISTRY[i].0 == NONE_DOWNGRADED_NRS[j] {
                    if seen[j] {
                        return false; // 白名单内重复
                    }
                    seen[j] = true;
                    hit = true;
                }
                j += 1;
            }
            if !hit {
                return false; // 白名单外 NONE 实装号
            }
        }
        i += 1;
    }
    // 白名单内每一号都必须在册（防「白名单写了但注册表丢了」的漂移）。
    let mut j = 0;
    while j < seen.len() {
        if !seen[j] {
            return false;
        }
        j += 1;
    }
    none_cnt == NONE_DOWNGRADED_NRS.len() as u8
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

    /// 表规模与形状：52 号在册、无重复号（W3-Q97 0x57／35 号件 0x1C／G2 刀 7+10 五号入册）。
    #[test]
    fn registry_shape() {
        assert_eq!(REGISTRY.len(), 52);
        assert!(nrs_unique());
    }

    /// NONE 实装号后继审计律（W3-Q100）：在册 NONE 号集恰等白名单
    /// 0x61/0x40/0x87（多一号少一号都红——§A115 四问复审是降档前置）。
    #[test]
    fn none_installed_class_audited() {
        assert!(none_class_audited());
    }

    /// 白名单外敌意注入对拍（负例）：白名单 ±一号（0x61 换 0x42）或表内
    /// 多一号 NONE，恰等律必红——执法面的执法面。
    #[test]
    fn none_audit_catches_drift() {
        // 结构性对拍：class_of 对三白名单号返 NONE、对非白名单实装号不返 NONE。
        for nr in NONE_DOWNGRADED_NRS {
            assert_eq!(class_of(*nr), SVC_CLASS_NONE, "nr={nr:#04x} 须 NONE");
        }
        for nr in [0x42u8, 0x84, 0x86, 0x88] {
            assert_ne!(class_of(nr), SVC_CLASS_NONE, "nr={nr:#04x} 不得 NONE");
        }
    }

    /// class_of 全表往返：注册号恰返在册类、未注册号一律 NONE。
    #[test]
    fn class_of_roundtrip_and_default() {
        for &(nr, class) in REGISTRY {
            assert_eq!(class_of(nr), class, "nr={nr:#04x} 往返失配");
        }
        // 未注册面：v1 预留 0x85 attest、调试槽退役位 0xFE/0xFF、随机空号
        //（0x61/0x40/0x87 W3-Q100 降档入册后移出此列——NONE 白名单）。
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
        // 恰 49 号非 NONE 命中、207 号 NONE（256-52+3 白名单降档——W3-Q100
        // 后 NONE 面 = 未注册 204 号 + 白名单实装 3 号）。
        let none_installed = (REGISTRY.len() - NONE_DOWNGRADED_NRS.len()) as u16;
        assert_eq!(hit_cnt, none_installed);
        assert_eq!(none_cnt, 256 - none_installed);
    }

    /// 落类面锚点抽查（§A113/§A115 判例形状）：W3-Q100 降档三平凡号
    /// 0x61/0x87/0x40 = NONE（PT 锁后继审计）；signal_send/power_off/
    /// policy_bind = TABLE；thread_sleep = SQ_RQ（无表无 KL）；
    /// vmo 十一号 = TABLE（S-4 降档后）；create 族四号 = ALL。
    #[test]
    fn anchor_classes() {
        assert_eq!(class_of(0x61), SVC_CLASS_NONE); // clock_read（W3-Q100）
        assert_eq!(class_of(0x87), SVC_CLASS_NONE); // signal_take（W3-Q100）
        assert_eq!(class_of(0x40), SVC_CLASS_NONE); // util_snapshot（W3-Q100）
        assert_eq!(class_of(0x88), SVC_CLASS_TABLE); // power_off
        assert_eq!(class_of(0x83), SVC_CLASS_TABLE); // policy_bind
        assert_eq!(class_of(0x37), SVC_CLASS_SQ_RQ); // thread_sleep
        for nr in [0x11u8, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C] {
            assert_eq!(class_of(nr), SVC_CLASS_TABLE, "vmo/vmar {nr:#04x}");
        }
        for nr in [0x31u8, 0x33, 0x34, 0x36, 0x3B, 0x51, 0x54] {
            assert_eq!(class_of(nr), SVC_CLASS_ALL, "create/exit/kill {nr:#04x}");
        }
        for nr in [0x32u8, 0x35, 0x3F] {
            assert_eq!(class_of(nr), SVC_CLASS_RQ_TABLE, "start/yield {nr:#04x}");
        }
        for nr in [0x01u8, 0x04, 0x05, 0x06, 0x21, 0x23, 0x24, 0x52, 0x53] {
            assert_eq!(class_of(nr), SVC_CLASS_SQ_RQ_TABLE, "wait/send {nr:#04x}");
        }
    }
}
