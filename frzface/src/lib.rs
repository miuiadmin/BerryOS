//! berry-frzface —— sched 冻结面（freeze/thaw 资格判 + 扫描序）纯函数面（W3-Q27）。
//!
//! **动机**：冻结机制（1.1/S3-a 进程冻结 + S3-b 单线程冻结）的判定律此前
//! 内联在 `sched/src/sched.rs` 的 `freeze_precheck_by_pinned` /
//! `freeze_by_pinned` / `thaw_by_pinned` / `freeze_by_thread` /
//! `thaw_by_thread` 五函数的裸 unsafe 指针扫描里——host 测不可达（sched
//! 依赖内核队列结构），ktest 只覆盖 boot 相。app-1 段 a-1 双残影定谳
//! （睡眠者 svc_mask/held_locks 是记账残影、真在飞 = RQ 在席）这类精细
//! 判序需要一张 host 可测的**钉板**：本 crate 把三谓词与扫描序步进按
//! 值域抽成纯函数 + 表驱动测试，`sched` 侧消费同一函数（零第二实现
//! ——抽离即唯一事实源，非镜像）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`SIG_READABLE`]/[`SIG_WRITABLE`]：内核派生位本地镜像（ABI 冻结值
//!   1/2——sched crate 不依赖 kernel handle 模块的既有同款；值域钉死见
//!   [`test_abi_mirror_frozen_values`]）。
//! - [`sleep_ok`]/[`rq_ok`]：**by_pinned 族谓词**（freeze_precheck_by_pinned
//!   消费）——睡眠者 `pi_inherited == 0`（PI 捐赠边上冻结 = 捐赠链断）
//!   ∧ `wait_signals` 无内核派生位（READABLE/WRITABLE——donor 面）。
//!   **不含** svc_mask/held_locks：睡眠者位是残影（app-1 段 a-1 双残影
//!   定谳，16 试全拒实证其中持锁残影 ×15）。RQ 在席者另加
//!   `svc_mask == 0`（SVC 窗在飞）∧ `held_locks` 剥 [`HELD_FROZEN`]/
//!   [`HELD_THROTTLE`] 后零（bit6 嵌套冻结 / bit7 预算限流是调度态非
//!   持锁；只有结构锁位 bit1–5 才拒）。
//! - [`thread_sleep_ok`]/[`thread_rq_ok`]：**by_thread 族谓词现状钉**
//!   （freeze_precheck_by_thread 消费）——sched 侧两张脸**忠实镜像**
//!   （行为零变）：睡眠者查 `pi_inherited == 0 ∧ held_locks == 0`
//!   （**不查 wait_signals 派生位、不剥 bit6/7**——held 残影照拒，与
//!   §A144「双级同修」宣示相抵，疑似修复只落 by_pinned 单级，本 crate
//!   钉现状 + 差异表面单列，不动行为——judgement 在 PR body 详陈）。
//! - [`freeze_ok_for`]/[`thread_freeze_ok_for`]：两族统一入口——按队列
//!   成员资格分流两谓词（[`Where`]::SleepQ → sleep 系；Rq → rq 系）。
//! - [`zone_order`]/[`next_step`]：扫描序纯投影——SLEEPQ 先、双 RQ 后
//!   （cpu 0→1）；SLEEPQ 步进（先访当前再推进——单链无环）、RQ 环步进
//!   （§A136：**先访当前节点，再推进并判回 head**——先 break 会漏尾/
//!   单元素）、freeze_by_thread 单命中早退位置（RQ 双区先扫完才轮
//!   SLEEPQ——与 precheck 相反的区序，钉住两族扫描不是同一张脸）。
//! - [`freeze_bit_set`]/[`freeze_bit_clear`]：幂等置位/清位律
//!   （`|= HELD_FROZEN` / `&= !HELD_FROZEN`——重复冻结不叠加、重复
//!   解冻不掏空）。
//!
//! 不在此 crate（留 sched 壳）：指针解引用与队列访问（unsafe 物理臂）、
//! wake_pending 清面（AM-1.5）、freeze_depth 记账、遥测计数。本面只钉
//! 「谁可冻/怎么扫」，不钉「原子怎么打」。

#![cfg_attr(not(test), no_std)]

use berry_ringface::{HELD_FROZEN, HELD_THROTTLE};

/// 内核派生位 READABLE（ABI 冻结值 1——sched.rs 本地镜像同值，改值先改
/// `tools/abi.toml`）。
pub const SIG_READABLE: u64 = 1;
/// 内核派生位 WRITABLE（ABI 冻结值 2）。
pub const SIG_WRITABLE: u64 = 2;

/// 队列成员资格（谓词分流轴——SLEEPQ 与 RQ 的判据差异是本面的核心钉）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Where {
    /// 睡在 syscall 内（block 舞蹈放锁入队）——svc_mask/held_locks 是
    /// 残影不拒（真在飞 = RQ 在席）。
    SleepQ,
    /// RQ 在席（SVC 窗内可抢占形）——svc_mask + held 结构位都拒。
    Rq,
}

/// 冻结前置谓词的**纯投影入参**（Task 判据字段镜像——dpgface SlotSnapshot
/// 同款；sched 侧从 `&Task` 逐字段装包，零 trait 依赖）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreezeProbe {
    /// PI 捐赠边计数（≠0 = 捐赠链在——冻结即断链）。
    pub pi_inherited: i32,
    /// 等待信号位（含内核派生位 READABLE/WRITABLE——donor 面）。
    pub wait_signals: u64,
    /// SVC 窗记账 mask（SLEEPQ 残影 / RQ 真在飞）。
    pub svc_mask: u8,
    /// 持锁位图（bit1–5 结构锁 / bit6 HELD_FROZEN / bit7 HELD_THROTTLE）。
    pub held_locks: u8,
}

impl FreezeProbe {
    /// 全零探针（干净任务——两谓词皆过）。
    pub const CLEAN: FreezeProbe = FreezeProbe {
        pi_inherited: 0,
        wait_signals: 0,
        svc_mask: 0,
        held_locks: 0,
    };
}

/// 睡眠者谓词：PI 边 ∧ 内核派生等待位两拒。svc_mask/held_locks 有意
/// 不查（app-1 段 a-1：随「睡在 syscall 内」带进 SLEEPQ 的记账残影，
/// 物理锁已放——冻结只暂停执行不动锁，无停摆面）。
pub fn sleep_ok(p: &FreezeProbe) -> bool {
    p.pi_inherited == 0 && p.wait_signals & (SIG_READABLE | SIG_WRITABLE) == 0
}

/// RQ 在席者谓词：sleep_ok ∧ SVC mask 零 ∧ 结构锁位零（剥
/// HELD_FROZEN/HELD_THROTTLE——调度态非持锁；限流中的 deadline 任务仍可
/// 被进程冻结）。
pub fn rq_ok(p: &FreezeProbe) -> bool {
    sleep_ok(p)
        && p.svc_mask == 0
        && p.held_locks & !(HELD_FROZEN | HELD_THROTTLE) == 0
}

/// 统一入口：按队列成员资格分流（sched 扫描循环的消费面——zone 语义
/// 单源，避免两处裸调用点判据漂移）。
pub fn freeze_ok_for(where_: Where, p: &FreezeProbe) -> bool {
    match where_ {
        Where::SleepQ => sleep_ok(p),
        Where::Rq => rq_ok(p),
    }
}

// ---------------------------------------------------------------------------
// by_thread 族谓词（现状忠实镜像——sched.rs:2813-2814 两行裸闭包的纯面）
// ---------------------------------------------------------------------------

/// by_thread 族睡眠者谓词（现状钉）：`pi_inherited == 0 ∧ held_locks == 0`。
///
/// 与 by_pinned 族 [`sleep_ok`] 的两张脸（**行为零变镜像**，非笔误）：
/// - 查 `held_locks == 0` 全位（含 bit6/7 与结构位）——睡眠者持锁残影
///   照拒（by_pinned 族定谳「残影不拒」未落此级）；
/// - **不查 `wait_signals` 内核派生位**——睡 channel 的 donor 面单线程
///   冻结不拒（by_pinned 族拒）。
///
/// §A144 判例宣称「freeze_precheck_by_pinned/by_thread 双级同修」，实树
/// by_thread 侧谓词仍是旧脸——修复疑似只落 by_pinned 单级。本 crate 钉
/// 现状（改这里的语义 = 改 sched 行为的同一条纪律），差异面归 PR 裁。
pub fn thread_sleep_ok(p: &FreezeProbe) -> bool {
    p.pi_inherited == 0 && p.held_locks == 0
}

/// by_thread 族 RQ 在席者谓词（现状钉）：`thread_sleep_ok ∧ svc_mask == 0`
/// （**不剥 HELD_FROZEN/HELD_THROTTLE**——与 by_pinned 族 [`rq_ok`]
/// 的差异面之二：此处 bit6/7 照拒）。
pub fn thread_rq_ok(p: &FreezeProbe) -> bool {
    thread_sleep_ok(p) && p.svc_mask == 0
}

/// by_thread 族统一入口（freeze_precheck_by_thread 的消费面）。
pub fn thread_freeze_ok_for(where_: Where, p: &FreezeProbe) -> bool {
    match where_ {
        Where::SleepQ => thread_sleep_ok(p),
        Where::Rq => thread_rq_ok(p),
    }
}

/// 扫描区（三区全序钉板：precheck/freeze_by_pinned 族 = SleepQ→Rq0→Rq1；
/// freeze_by_thread 族 = Rq0→Rq1→SleepQ——两族区序不同，钉住差异）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// SLEEPQ（单链无环——步进 = 先访当前再推进 next）。
    SleepQ,
    /// CPU0 RQ（环形——§A136 先访后判）。
    Rq0,
    /// CPU1 RQ（环形）。
    Rq1,
}

/// 区全序（两族扫描序的单源描述）。
pub fn zone_order(thread_scan: bool) -> [Zone; 3] {
    if thread_scan {
        [Zone::Rq0, Zone::Rq1, Zone::SleepQ]
    } else {
        [Zone::SleepQ, Zone::Rq0, Zone::Rq1]
    }
}

/// 环形队列访问序（§A136 纯投影）：**先访当前节点，再推进并判回 head**
/// ——先 break 会漏尾/单元素。`next_of` = 节点→后继映射（sched 侧
/// `(*t).rq_next.get()`），`head` = 入口锚。产出完整访问序（含尾），
/// 到 head 即闭合停。sched 三处 RQ 循环的步进律单源：本迭代器与
/// `while let Some(c) = cur { 访; cur = next_of(c); if cur == head
/// { break; } }` 语义等价（防呆帽 = 环长上界，病态 next_of 不死循环）。
pub fn ring_visit_order(
    next_of: impl Fn(usize) -> usize,
    head: usize,
    cap: usize,
) -> impl Iterator<Item = usize> {
    let mut cur = Some(head);
    let mut left = cap;
    core::iter::from_fn(move || {
        let c = cur?;
        if left == 0 {
            cur = None;
            return Some(c);
        }
        left -= 1;
        let n = next_of(c);
        if n == head {
            cur = None; // 闭合：本节点已访，产出后停（§A136 先访后判）
        } else {
            cur = Some(n);
        }
        Some(c)
    })
}

/// 幂等置位（freeze_by_pinned/freeze_by_thread 共律——重复冻结不叠加，
/// 位域单 bit 语义）。
pub fn freeze_bit_set(held_locks: u8) -> u8 {
    held_locks | HELD_FROZEN
}

/// 幂等清位（thaw_by_pinned/thaw_by_thread 共律——重复解冻不掏空他人
/// 位）。
pub fn freeze_bit_clear(held_locks: u8) -> u8 {
    held_locks & !HELD_FROZEN
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ABI 冻结值镜像钉死（改值先改 tools/abi.toml——evface 同款执法）。
    #[test]
    fn test_abi_mirror_frozen_values() {
        assert_eq!(SIG_READABLE, 1);
        assert_eq!(SIG_WRITABLE, 2);
        assert_eq!(HELD_FROZEN, 1 << 6);
        assert_eq!(HELD_THROTTLE, 1 << 7);
    }

    /// 干净任务两谓词皆过（含已带 HELD_FROZEN/HELD_THROTTLE 位的
    /// RQ 任务——调度态非持锁不拒）。
    #[test]
    fn test_clean_passes_both() {
        assert!(sleep_ok(&FreezeProbe::CLEAN));
        assert!(rq_ok(&FreezeProbe::CLEAN));
        let sched_state = FreezeProbe {
            held_locks: HELD_FROZEN | HELD_THROTTLE,
            ..FreezeProbe::CLEAN
        };
        assert!(rq_ok(&sched_state)); // 嵌套冻结+限流中仍可冻（幂等置位面）
        assert!(sleep_ok(&sched_state));
    }

    /// PI 捐赠边两谓词全拒（冻结 = 捐赠链断——防停摆的正是这条）。
    #[test]
    fn test_pi_edge_rejects_everywhere() {
        let p = FreezeProbe {
            pi_inherited: 1,
            ..FreezeProbe::CLEAN
        };
        assert!(!sleep_ok(&p));
        assert!(!rq_ok(&p));
        assert!(!freeze_ok_for(Where::SleepQ, &p));
        assert!(!freeze_ok_for(Where::Rq, &p));
        // 深捐赠（多跳）同样拒。
        let deep = FreezeProbe {
            pi_inherited: 3,
            ..FreezeProbe::CLEAN
        };
        assert!(!freeze_ok_for(Where::Rq, &deep));
    }

    /// 内核派生等待位拒（READABLE/WRITABLE——donor 面）；用户位不拒。
    #[test]
    fn test_kernel_wait_bits_reject() {
        for bits in [SIG_READABLE, SIG_WRITABLE, SIG_READABLE | SIG_WRITABLE] {
            let p = FreezeProbe {
                wait_signals: bits,
                ..FreezeProbe::CLEAN
            };
            assert!(!sleep_ok(&p), "wait_signals={bits} 须拒睡眠者");
            assert!(!rq_ok(&p));
        }
        // 高 32 位用户位（1<<32、1<<63）不拒——冻结不碰用户等待语义。
        for ub in [1u64 << 32, 1u64 << 63, (1u64 << 40) | (1u64 << 33)] {
            let p = FreezeProbe {
                wait_signals: ub,
                ..FreezeProbe::CLEAN
            };
            assert!(sleep_ok(&p), "用户位 {ub:#x} 不拒");
            assert!(rq_ok(&p));
        }
    }

    /// svc_mask：SLEEPQ 残影不拒 / RQ 真在飞拒（app-1 段 a-1 双残影定谳
    /// 的钉板——16×50ms 采样全拒实证的反面：睡眠者带非零 mask 挂 SLEEPQ
    /// 是结构性的，若也查 mask 则睡着的进程结构性不可冻）。
    #[test]
    fn test_svc_mask_residual_vs_live() {
        let p = FreezeProbe {
            svc_mask: 0b0011_1111,
            ..FreezeProbe::CLEAN
        };
        assert!(sleep_ok(&p), "SLEEPQ 残影不拒（定谳核心）");
        assert!(!rq_ok(&p), "RQ 在飞拒（窗内可抢占形）");
        assert!(freeze_ok_for(Where::SleepQ, &p));
        assert!(!freeze_ok_for(Where::Rq, &p));
    }

    /// held_locks 结构锁位拒 RQ（bit1–5）；bit6/7 剥离后不拒。
    #[test]
    fn test_held_struct_bits_rq_only() {
        for bit in 1..=5u8 {
            let p = FreezeProbe {
                held_locks: 1 << bit,
                ..FreezeProbe::CLEAN
            };
            assert!(sleep_ok(&p), "SLEEPQ 残影不拒（物理锁已放）");
            assert!(!rq_ok(&p), "RQ 结构锁位 bit{bit} 拒");
        }
        // bit6/7 与结构位合体：剥后仍有结构位 → 拒。
        let mixed = FreezeProbe {
            held_locks: HELD_FROZEN | HELD_THROTTLE | (1 << 3),
            ..FreezeProbe::CLEAN
        };
        assert!(!rq_ok(&mixed));
        // 全 8 位减 bit6/7 = bit0..5 全结构位 → 拒。
        let all_struct = FreezeProbe {
            held_locks: 0b0011_1111,
            ..FreezeProbe::CLEAN
        };
        assert!(!rq_ok(&all_struct));
        assert!(sleep_ok(&all_struct));
    }

    /// 谓词分流全表（4 轴 × 2 队列——表驱动主钉）。
    #[test]
    fn test_predicate_table() {
        let cases: &[(FreezeProbe, bool, bool, &str)] = &[
            // (probe, sleep_ok, rq_ok, label)
            (FreezeProbe::CLEAN, true, true, "干净"),
            (
                FreezeProbe { pi_inherited: 1, ..FreezeProbe::CLEAN },
                false, false, "PI 边",
            ),
            (
                FreezeProbe { wait_signals: SIG_READABLE, ..FreezeProbe::CLEAN },
                false, false, "内核等待位",
            ),
            (
                FreezeProbe { wait_signals: 1 << 32, ..FreezeProbe::CLEAN },
                true, true, "用户等待位",
            ),
            (
                FreezeProbe { svc_mask: 1, ..FreezeProbe::CLEAN },
                true, false, "svc_mask 残影/在飞",
            ),
            (
                FreezeProbe { held_locks: 1 << 2, ..FreezeProbe::CLEAN },
                true, false, "结构锁位",
            ),
            (
                FreezeProbe { held_locks: HELD_FROZEN, ..FreezeProbe::CLEAN },
                true, true, "已冻位（幂等）",
            ),
            (
                FreezeProbe { held_locks: HELD_THROTTLE, ..FreezeProbe::CLEAN },
                true, true, "限流位",
            ),
            (
                FreezeProbe {
                    pi_inherited: 2,
                    wait_signals: SIG_WRITABLE,
                    svc_mask: 0x10,
                    held_locks: 1 << 5,
                },
                false, false, "全红组合",
            ),
        ];
        for (p, s, r, label) in cases {
            assert_eq!(sleep_ok(p), *s, "sleep_ok: {label}");
            assert_eq!(rq_ok(p), *r, "rq_ok: {label}");
            assert_eq!(freeze_ok_for(Where::SleepQ, p), *s, "分流 SleepQ: {label}");
            assert_eq!(freeze_ok_for(Where::Rq, p), *r, "分流 Rq: {label}");
        }
    }

    /// by_thread 族谓词两张脸差异钉板（现状忠实镜像——改 sched 前先改
    /// 判例 §A144 的「双级同修」宣示）。
    #[test]
    fn test_thread_family_predicate_divergence() {
        // 差异一：睡眠者 held_locks 残影——by_pinned 不拒 / by_thread 拒。
        let held_residual = FreezeProbe {
            held_locks: 1 << 2,
            ..FreezeProbe::CLEAN
        };
        assert!(sleep_ok(&held_residual), "by_pinned 族：残影不拒");
        assert!(!thread_sleep_ok(&held_residual), "by_thread 族：残影照拒（现状）");

        // 差异二：wait_signals 内核派生位——by_pinned 拒 / by_thread 不查。
        let donor = FreezeProbe {
            wait_signals: SIG_READABLE,
            ..FreezeProbe::CLEAN
        };
        assert!(!sleep_ok(&donor), "by_pinned 族：donor 面拒");
        assert!(thread_sleep_ok(&donor), "by_thread 族：不查派生位（现状）");

        // 差异三：RQ 在席带 HELD_FROZEN/THROTTLE——by_pinned 剥后不拒 /
        // by_thread 不剥照拒。
        let sched_bits = FreezeProbe {
            held_locks: HELD_FROZEN | HELD_THROTTLE,
            ..FreezeProbe::CLEAN
        };
        assert!(rq_ok(&sched_bits), "by_pinned 族：调度态剥后不拒");
        assert!(!thread_rq_ok(&sched_bits), "by_thread 族：bit6/7 照拒（现状）");

        // 两族共同面：PI 边全拒、干净任务全过。
        let pi = FreezeProbe { pi_inherited: 1, ..FreezeProbe::CLEAN };
        assert!(!thread_sleep_ok(&pi) && !thread_rq_ok(&pi));
        assert!(thread_sleep_ok(&FreezeProbe::CLEAN));
        assert!(thread_rq_ok(&FreezeProbe::CLEAN));

        // by_thread 族 svc_mask：SLEEPQ 残影不拒 / RQ 拒（与 by_pinned 同向）。
        let svc = FreezeProbe { svc_mask: 1, ..FreezeProbe::CLEAN };
        assert!(thread_sleep_ok(&svc));
        assert!(!thread_rq_ok(&svc));
        // 分流入口一致性。
        assert_eq!(thread_freeze_ok_for(Where::SleepQ, &svc), true);
        assert_eq!(thread_freeze_ok_for(Where::Rq, &svc), false);
    }

    /// by_thread 族谓词全表（表驱动——sched.rs:2813-2814 裸闭包的钉板）。
    #[test]
    fn test_thread_predicate_table() {
        let cases: &[(FreezeProbe, bool, bool, &str)] = &[
            (FreezeProbe::CLEAN, true, true, "干净"),
            (
                FreezeProbe { pi_inherited: 1, ..FreezeProbe::CLEAN },
                false, false, "PI 边",
            ),
            (
                FreezeProbe { held_locks: 1 << 1, ..FreezeProbe::CLEAN },
                false, false, "结构锁位（睡眠者也拒——现状）",
            ),
            (
                FreezeProbe { held_locks: HELD_FROZEN, ..FreezeProbe::CLEAN },
                false, false, "已冻位（不剥——现状）",
            ),
            (
                FreezeProbe { held_locks: HELD_THROTTLE, ..FreezeProbe::CLEAN },
                false, false, "限流位（不剥——现状）",
            ),
            (
                FreezeProbe { wait_signals: SIG_WRITABLE, ..FreezeProbe::CLEAN },
                true, true, "内核派生等待位（不查——现状）",
            ),
            (
                FreezeProbe { svc_mask: 0x20, ..FreezeProbe::CLEAN },
                true, false, "svc_mask 残影/在飞",
            ),
            (
                FreezeProbe {
                    pi_inherited: 2,
                    wait_signals: SIG_READABLE,
                    svc_mask: 0x10,
                    held_locks: 1 << 5,
                },
                false, false, "全红组合",
            ),
        ];
        for (p, s, r, label) in cases {
            assert_eq!(thread_sleep_ok(p), *s, "thread_sleep_ok: {label}");
            assert_eq!(thread_rq_ok(p), *r, "thread_rq_ok: {label}");
            assert_eq!(
                thread_freeze_ok_for(Where::SleepQ, p), *s,
                "thread 分流 SleepQ: {label}"
            );
            assert_eq!(
                thread_freeze_ok_for(Where::Rq, p), *r,
                "thread 分流 Rq: {label}"
            );
        }
    }

    /// 区序两族钉板：precheck/pinned 族 SLEEPQ 先；thread 族 RQ 先
    /// （freeze_by_thread 单命中早退在 RQ 双区——与 precheck 相反）。
    #[test]
    fn test_zone_order_two_families() {
        assert_eq!(
            zone_order(false),
            [Zone::SleepQ, Zone::Rq0, Zone::Rq1],
            "precheck/freeze_by_pinned/thaw_by_pinned 族"
        );
        assert_eq!(
            zone_order(true),
            [Zone::Rq0, Zone::Rq1, Zone::SleepQ],
            "freeze_by_thread/thaw_by_thread 族"
        );
    }

    /// 幂等置位/清位律（重复冻结不叠加、重复解冻不掏空、往返闭合）。
    #[test]
    fn test_freeze_bit_idempotent() {
        assert_eq!(freeze_bit_set(0), HELD_FROZEN);
        assert_eq!(freeze_bit_set(HELD_FROZEN), HELD_FROZEN, "幂等");
        assert_eq!(freeze_bit_set(HELD_THROTTLE), HELD_FROZEN | HELD_THROTTLE, "不吞邻位");
        // 结构位共存：置冻结位不动结构位。
        assert_eq!(freeze_bit_set(1 << 2), HELD_FROZEN | (1 << 2));
        assert_eq!(freeze_bit_clear(HELD_FROZEN), 0);
        assert_eq!(freeze_bit_clear(0), 0, "幂等");
        assert_eq!(freeze_bit_clear(HELD_FROZEN | (1 << 2)), 1 << 2, "只掏冻结位");
        assert_eq!(
            freeze_bit_clear(freeze_bit_set(HELD_THROTTLE | (1 << 1))),
            HELD_THROTTLE | (1 << 1),
            "往返闭合不动他人位"
        );
        // 冻→解→冻 往返稳定。
        let x = 1 << 4;
        assert_eq!(freeze_bit_set(freeze_bit_clear(freeze_bit_set(x))), x | HELD_FROZEN);
    }

    /// §A136 环终止先访后判的纯投影钉板（sched 三处 RQ 循环同律）。
    #[test]
    fn test_ring_step_law() {
        // 三元环 head→a→b→head：完整访问序 = [head, a, b]（尾必访——
        // 先访后判，b 的 next 是 head 才 break，b 不漏）。
        let succ = |n: usize| match n {
            0 => 1, // head → a
            1 => 2, // a → b
            _ => 0, // b → head（闭合）
        };
        let v: Vec<usize> = ring_visit_order(succ, 0, 16).collect();
        assert_eq!(v, vec![0, 1, 2], "三元环全访含尾");

        // 单元素环（head 的 next 即 head）：只访 head 一次，不漏不重。
        let v1: Vec<usize> = ring_visit_order(|n| n, 7, 16).collect();
        assert_eq!(v1, vec![7], "单元素环恰访一次");

        // 二元环：head→x→head。
        let v2: Vec<usize> = ring_visit_order(|n| 1 - n, 0, 16).collect();
        assert_eq!(v2, vec![0, 1]);

        // 防呆帽：病态 next_of（永指新节点不闭合）在帽处停。
        let counter = std::cell::Cell::new(0usize);
        let vc: Vec<usize> = ring_visit_order(
            |_n| {
                100 + counter.replace(counter.get() + 1)
            },
            0,
            5,
        )
        .collect();
        assert_eq!(vc.len(), 6, "帽 5 → 恰访 6 节点后停（帽+1=首个被帽的访）");
        assert_eq!(vc[0], 0);
    }
}
