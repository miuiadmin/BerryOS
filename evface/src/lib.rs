//! berry-evface —— Event 信号位空间纯函数面（W3-Q7）。
//!
//! **动机**：Event 信号位语义（4-2 位空间：低 32 内核派生 + 高 32 用户）
//! 此前散在 `kernel/src/abi_gen.rs`（生成文件，SIGNAL_* 常量）与
//! `kernel/src/syscall.rs`（掩码执法判、wait 资格判）的裸表达式里——host
//! 测不可达（kernel 依赖 HAL global_asm，只 aarch64-unknown-none 可编），
//! ktest 只覆盖 boot 相。位空间的回归面（用户位门、派生位只读、别位拒、
//! wait 命中律）需要一张 host 可测的**钉板**：本 crate 把位域常量与判定
//! 按值域抽成纯函数 + 表驱动测试，`kernel` 侧消费同一函数（零第二实现
//! ——抽离即唯一事实源，非镜像）。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 kernel 行为）：
//! - [`SIGNAL_READABLE`]/[`SIGNAL_WRITABLE`]/[`SIGNAL_TERMINATED`]/
//!   [`SIGNAL_WAKE_PENDING`]/[`SIGNAL_USER_MASK`]：位空间常量单源
//!   （ABI §8.2 冻结值镜像——[`test_abi_mirror_frozen_values`] 钉死字面
//!   值，改值先改 `tools/abi.toml`）。
//! - [`apply_signal`]:`(cur, clear, set)` → 新掩码——Event
//!   `object_signal` 的置/清位律（`signals = (cur & !clear) | set`）。
//! - [`signal_masks_legal`]:object_signal 参数门——两掩码只收高 32 位
//!   用户位，低 32 位内核派生保留（越界 → -EINVAL）。
//! - [`wait_hit`]:wait 命中判——`current & wanted != 0`，命中回写
//!   **全满足集**（Ziron `zx_object_wait_one` 同形）。
//! - [`wake_eligible`]:signal 置位后唤醒资格——请求位与**新置位集**相交
//!   才该醒（清位不唤醒——位没新增；纯清位 set=0 无命中）。
//!
//! 与冻结面的关系：syscall ABI 零触碰（位值本身即 ABI §8.2 冻结常量，
//! 本 crate 只钉其判定律）；Event 本体的原子 RMW（fetch_update CAS 环）
//! 与写者纪律（SVC/IRQ 上下文）不在此 crate——本面只钉「位怎么判」，
//! 不钉「原子怎么打」。

#![cfg_attr(not(test), no_std)]

// ---- 信号位（4-2 位空间：低 32 内核派生 + 高 32 用户；ABI §8.2 冻结值镜像）----

/// Channel 端点可读：**本端**队列非空（内核派生位——用户 signal 不出此位）。
pub const SIGNAL_READABLE: u64 = 1 << 0;
/// Channel 端点可写：**对端**队列未满（内核派生位）。
pub const SIGNAL_WRITABLE: u64 = 1 << 1;
/// Process 已退出（Plan B v0.5 内核派生位；Exited 单调置位永不清）。
pub const SIGNAL_TERMINATED: u64 = 1 << 2;
/// 冻结进程收讯挂起位（AM-1.5 内核派生位——SuspendToken wait 面唯一可
/// 命中位）：目标进程冻结期间任一 send 成功入队即置位，全解冻清零。
pub const SIGNAL_WAKE_PENDING: u64 = 1 << 3;
/// 用户信号位掩码（Event `object_signal` 只收高 32 位用户位；低 32 位
/// 保留给内核派生信号——位面冲突由内核执法拒绝）。
pub const SIGNAL_USER_MASK: u64 = 0xFFFF_FFFF_0000_0000;

/// 内核派生位全集（低 32 位在册四位的并集——READABLE|WRITABLE|
/// TERMINATED|WAKE_PENDING）。
pub const SIGNAL_DERIVED_MASK: u64 =
    SIGNAL_READABLE | SIGNAL_WRITABLE | SIGNAL_TERMINATED | SIGNAL_WAKE_PENDING;

/// `object_signal` 置/清位律：`signals = (signals & !clear) | set`
/// （置/清一条原子——本函数为其纯值域投影；kernel Event::signal 的
/// fetch_update CAS 环与此处逐字同式）。
#[inline]
pub fn apply_signal(current: u64, clear_mask: u64, set_mask: u64) -> u64 {
    (current & !clear_mask) | set_mask
}

/// `object_signal` 参数门（syscall.rs 4-2 位空间执法的纯投影）：
/// clear/set 两掩码**只收高 32 位用户位**——任一掩码触及低 32 位内核
/// 派生保留区即拒（false → 调用方折 -EINVAL）。检在 get_raw 门控之后
/// （坏 handle /类型不符仍优先 -EBADF——错误面优先级不在本函数）。
#[inline]
pub fn signal_masks_legal(clear_mask: u64, set_mask: u64) -> bool {
    (clear_mask | set_mask) & !SIGNAL_USER_MASK == 0
}

/// wait 命中判（wait_one/wait_many 共用）：`current & wanted != 0`。
/// 命中时观察值 = **全满足集** current（非仅命中位——Zircon 同形；
/// wait_many 取「入参序最前命中」是调用方循环纪律，不在本函数）。
#[inline]
pub fn wait_hit(current: u64, wanted: u64) -> bool {
    current & wanted != 0
}

/// signal 置位后唤醒资格判（`wake_matching` 匹配律的纯投影）：
/// 睡眠者请求位 `wanted` 与**新置位集** `set` 相交才可能因本 signal 醒
/// （清位不唤醒——位没新增；`set == 0` 纯清位恒 false）。被唤醒者重验
/// 条件兜底（重置已置位等边角 → 假唤醒无害，kernel wait 循环纪律）。
#[inline]
pub fn wake_eligible(wanted: u64, set: u64) -> bool {
    set != 0 && wanted & set != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- ABI 冻结值镜像钉板（tools/abi.toml [[signal]] 表逐字对齐）----

    #[test]
    fn test_abi_mirror_frozen_values() {
        // 改这些值先改 tools/abi.toml（gen_abi.py 再生 abi_gen.rs）——
        // 本测试钉死 crate 常量与冻结 ABI 值不漂移。
        assert_eq!(SIGNAL_READABLE, 1 << 0);
        assert_eq!(SIGNAL_WRITABLE, 1 << 1);
        assert_eq!(SIGNAL_TERMINATED, 1 << 2);
        assert_eq!(SIGNAL_WAKE_PENDING, 1 << 3);
        assert_eq!(SIGNAL_USER_MASK, 0xFFFF_FFFF_0000_0000);
    }

    // ---- 位空间自洽：低 32 派生 / 高 32 用户互斥 ----

    #[test]
    fn test_bit_space_partition() {
        // 在册四位全落低 32；USER_MASK 恰为高 32 全集。
        assert_eq!(SIGNAL_DERIVED_MASK, 0xF);
        assert_eq!(SIGNAL_DERIVED_MASK & SIGNAL_USER_MASK, 0);
        assert_eq!(SIGNAL_DERIVED_MASK | SIGNAL_USER_MASK, 0xFFFF_FFFF_0000_000F);
        // 每个派生位互不重叠（位域自洽）。
        let bits = [SIGNAL_READABLE, SIGNAL_WRITABLE, SIGNAL_TERMINATED, SIGNAL_WAKE_PENDING];
        for i in 0..bits.len() {
            for j in (i + 1)..bits.len() {
                assert_eq!(bits[i] & bits[j], 0, "derived bits {} {} overlap", i, j);
            }
        }
    }

    // ---- apply_signal 置/清律 ----

    #[test]
    fn test_apply_signal_set_and_clear() {
        // 纯置位：全零起位置两位（并集叠加不丢）。
        assert_eq!(apply_signal(0, 0, 1 << 32 | 1 << 40), 1 << 32 | 1 << 40);
        // 纯清位。
        assert_eq!(apply_signal(1 << 32 | 1 << 33, 1 << 32, 0), 1 << 33);
        // 同位置盖：set 优先（(cur & !clear) | set —— set 赢）。
        assert_eq!(apply_signal(1 << 32, 1 << 32, 1 << 32), 1 << 32);
        // 同一掩码既清又置 = 位不变（clear 与 set 同位——net 效果 set 赢，
        // 与 kernel Event::signal 闭包逐字同式：0b1010 & !0b0110 = 0b1000，
        // | 0b0110 = 0b1110——先清后置的顺序律钉板）。
        assert_eq!(apply_signal(0b1010, 0b0110, 0b0110), 0b1110);
        // 不相交位：clear/set 各管各的。
        assert_eq!(apply_signal(0b1010, 0b1000, 0b0001), 0b0011);
        // 全清（clear=USER_MASK，set=0 → 归零）。
        assert_eq!(apply_signal(u64::MAX, SIGNAL_USER_MASK, 0), !SIGNAL_USER_MASK);
        // 恒等三形：clear=set=0 / clear=0 set=cur / clear=cur set=0。
        let cur = 1 << 33 | 1 << 51;
        assert_eq!(apply_signal(cur, 0, 0), cur);
        assert_eq!(apply_signal(cur, 0, cur), cur);
        assert_eq!(apply_signal(cur, cur, 0), 0);
    }

    #[test]
    fn test_apply_signal_zero_masks() {
        // 双零掩码 = 无操作（v1 无此调用形，但位律完备性钉板）。
        assert_eq!(apply_signal(0, 0, 0), 0);
        assert_eq!(apply_signal(SIGNAL_DERIVED_MASK, 0, 0), SIGNAL_DERIVED_MASK);
    }

    // ---- signal_masks_legal 用户位门（表驱动）----

    #[test]
    fn test_signal_masks_legal_table() {
        // (clear, set, expect) —— 合法 = 双掩码只触及高 32 用户位。
        let cases: &[(u64, u64, bool)] = &[
            // 双零：合法（无操作调用面存在——poll 探针不置不清）。
            (0, 0, true),
            // 纯用户位（单/多/全）。
            (1 << 32, 0, true),
            (0, 1 << 63, true),
            (1 << 32 | 1 << 40, 1 << 41, true),
            (SIGNAL_USER_MASK, SIGNAL_USER_MASK, true),
            // 内核派生位（任一在册位 / 全集 / 低位任意）→ 拒。
            (SIGNAL_READABLE, 0, false),
            (0, SIGNAL_WRITABLE, false),
            (SIGNAL_TERMINATED | SIGNAL_WAKE_PENDING, 0, false),
            (SIGNAL_DERIVED_MASK, 0, false),
            (0, SIGNAL_DERIVED_MASK, false),
            // 低位任意非在册值（bit4=1 未派生仍在保留区）→ 拒。
            (1 << 4, 0, false),
            (0, 1 << 31, false),
            // 混形：用户位 + 派生位并集 → 拒（一票否决）。
            (1 << 32 | SIGNAL_READABLE, 0, false),
            (0, 1 << 40 | SIGNAL_WRITABLE, false),
            (1 << 33, 1 << 3, false),
            // 跨界单值：USER_MASK 顶位 + 低一位。
            (0, 1 << 63 | 1, false),
        ];
        for &(c, s, want) in cases {
            assert_eq!(signal_masks_legal(c, s), want, "clear={:#x} set={:#x}", c, s);
        }
    }

    #[test]
    fn test_signal_masks_legal_is_all_low_bits_reject() {
        // 穷举低 32 位每一位：单位置位任一掩码即拒（位空间执法完备）。
        for b in 0..32u32 {
            assert!(!signal_masks_legal(1 << b, 0), "clear bit {} must reject", b);
            assert!(!signal_masks_legal(0, 1 << b), "set bit {} must reject", b);
        }
        // 穷举高 32 位每一位：单位置位即合法。
        for b in 32..64u32 {
            assert!(signal_masks_legal(1 << b, 0), "clear bit {} must pass", b);
            assert!(signal_masks_legal(0, 1 << b), "set bit {} must pass", b);
        }
    }

    // ---- wait_hit 命中律 ----

    #[test]
    fn test_wait_hit_table() {
        let cases: &[(u64, u64, bool)] = &[
            (0, 0, false),                        // 双零永不命中
            (0, 1 << 32, false),                  // 无信号等位
            (1 << 32, 0, false),                  // 有信号等零（poll 探针无效形）
            (1 << 32, 1 << 32, true),             // 单位命中
            (1 << 32 | 1 << 40, 1 << 40, true),   // 多信号命中其一
            (1 << 32, 1 << 33, false),            // 信号位不在请求集
            (SIGNAL_READABLE, SIGNAL_READABLE, true), // 派生位同律
            (SIGNAL_READABLE, SIGNAL_READABLE | SIGNAL_WRITABLE, true),
            (SIGNAL_USER_MASK, 1, false),         // 高位信号等低位
            (1, SIGNAL_USER_MASK, false),
            (u64::MAX, 1 << 63, true),            // 全集必命中任一非零请求
            (u64::MAX, u64::MAX, true),
        ];
        for &(cur, want, hit) in cases {
            assert_eq!(wait_hit(cur, want), hit, "cur={:#x} want={:#x}", cur, want);
        }
    }

    #[test]
    fn test_wait_hit_observed_is_full_set() {
        // 命中回写「全满足集」——观察值 = current 原值（Zircon 同形）。
        // kernel sys_object_wait_one: copy_out_u64(observed, current)。
        let cur = 1 << 32 | 1 << 41 | SIGNAL_READABLE;
        let want = 1 << 41;
        assert!(wait_hit(cur, want));
        // 「回写全满足集」由调用方以 current 落笔——本测试钉语义面：
        // 命中不改变 current（纯判，无副作用）。
        assert_eq!(cur, 1 << 32 | 1 << 41 | SIGNAL_READABLE);
    }

    // ---- wake_eligible 唤醒资格律 ----

    #[test]
    fn test_wake_eligible_table() {
        let cases: &[(u64, u64, bool)] = &[
            // 纯清位（set=0）恒不唤醒——位没新增。
            (1 << 32, 0, false),
            (u64::MAX, 0, false),
            (0, 0, false),
            // 请求位与新置位集相交才醒。
            (1 << 32, 1 << 32, true),
            (1 << 32 | 1 << 40, 1 << 40, true),
            (1 << 32, 1 << 33, false),            // 不相交
            (SIGNAL_READABLE, SIGNAL_READABLE, true), // 派生位同律
            (SIGNAL_READABLE | SIGNAL_WRITABLE, SIGNAL_WRITABLE, true),
            (SIGNAL_TERMINATED, SIGNAL_READABLE, false),
            // 请求零（哨兵 WAIT_ANY 在 kernel 侧特判，不经本函数——
            // 纯值域 wanted=0 恒 false）。
            (0, 1 << 32, false),
            // 大并集。
            (SIGNAL_USER_MASK, 1 << 63, true),
            (1 << 63, SIGNAL_USER_MASK, true),
        ];
        for &(want, set, e) in cases {
            assert_eq!(wake_eligible(want, set), e, "want={:#x} set={:#x}", want, set);
        }
    }

    #[test]
    fn test_wake_eligible_clear_only_never_wakes() {
        // 清位不唤醒：set=0 无论请求多大。
        assert!(!wake_eligible(u64::MAX, 0));
        // 即使清位后掩码巨变（apply_signal 大改），唤醒资格只看 set。
        let _new = apply_signal(u64::MAX, SIGNAL_USER_MASK, 0);
        assert!(!wake_eligible(1 << 32, 0));
    }

    // ---- 组合律：signal → wake → wait 重验闭环（纯值域全链）----

    #[test]
    fn test_signal_wake_wait_roundtrip() {
        // 场景：睡眠者等 1<<40；signal(clear=0, set=1<<40|1<<41)。
        let wanted = 1 << 40u64;
        let cur = 0;
        // 参数门先过（纯用户位）。
        assert!(signal_masks_legal(0, 1 << 40 | 1 << 41));
        // 应用置位。
        let new = apply_signal(cur, 0, 1 << 40 | 1 << 41);
        assert_eq!(new, 1 << 40 | 1 << 41);
        // 唤醒资格：请求与置位集相交 → 该醒。
        assert!(wake_eligible(wanted, 1 << 40 | 1 << 41));
        // 醒来重验：wait_hit 命中，观察 = 全满足集。
        assert!(wait_hit(new, wanted));
        assert_eq!(new, 1 << 40 | 1 << 41);
    }

    #[test]
    fn test_clear_race_recheck_loop() {
        // 清位竞态：唤醒后位刚被清 → 重验未命中 → 再睡（kernel wait 循环
        // 纯值域投影：醒因 ≠ 结果）。
        let wanted = 1 << 33;
        // t0: signal 置位 → 唤醒资格成立。
        let s1 = apply_signal(0, 0, 1 << 33);
        assert!(wake_eligible(wanted, 1 << 33));
        assert!(wait_hit(s1, wanted));
        // t1: 另一写者清位（竞态窗内）。
        let s2 = apply_signal(s1, 1 << 33, 0);
        // t2: 被唤醒者重验——未命中，未到点则再睡。
        assert!(!wait_hit(s2, wanted));
    }

    #[test]
    fn test_derived_bits_never_from_user_signal() {
        // 派生位只读律（值域面）：用户 signal 的合法掩码空间与派生位
        // 不相交 ⇒ 经 object_signal 永远写不到派生位。
        for m in [1u64 << 32, 1 << 40, 1 << 63, SIGNAL_USER_MASK] {
            assert!(signal_masks_legal(0, m));
            let after = apply_signal(0, 0, m);
            assert_eq!(after & SIGNAL_DERIVED_MASK, 0, "user signal must not set derived bits");
        }
        // 反向：任何触及派生位的掩码过不了门。
        for b in [SIGNAL_READABLE, SIGNAL_WRITABLE, SIGNAL_TERMINATED, SIGNAL_WAKE_PENDING] {
            assert!(!signal_masks_legal(0, b));
            assert!(!signal_masks_legal(b, 0));
        }
    }

    // ---- 与 kernel 消费点的语义对齐（文档性钉板）----

    #[test]
    fn test_kernel_align_syscall_paths() {
        // sys_object_signal（syscall.rs 3346）逐字投影：
        //   if (clear | set) & !SIGNAL_USER_MASK != 0 { return -EINVAL }
        //   ev.signal(clear, set);  wake_matching(key, set)
        // —— 本 crate 三函数覆盖同一判定链，零第二实现。
        let (clear, set) = (0u64, 1 << 42);
        if !signal_masks_legal(clear, set) {
            panic!("legal masks rejected");
        }
        let cur = apply_signal(0, clear, set);
        assert_eq!(cur, 1 << 42);
        assert!(wake_eligible(1 << 42, set));
        // sys_object_wait_one 命中判（syscall.rs 3474）：
        //   if current & signals != 0 { copy_out(observed, current) }
        assert!(wait_hit(cur, 1 << 42));
        // wait_many 命中判（syscall.rs 3609）：同一 wait_hit（逐项循环取
        // 入参序最前命中——循环纪律在调用方）。
        let objs = [1u64 << 42, (1 << 42) | (1 << 43), 0];
        let wants = [1u64 << 43, 1 << 43, 1 << 43];
        let mut first = None;
        for (i, (&c, &w)) in objs.iter().zip(wants.iter()).enumerate() {
            if wait_hit(c, w) {
                first = Some(i);
                break;
            }
        }
        assert_eq!(first, Some(1)); // 入参序最前命中（idx0 不相交 idx2 双零）
    }
}
