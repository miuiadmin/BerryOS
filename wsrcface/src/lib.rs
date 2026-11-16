//! berry-wsrcface —— 唤醒源分类/白名单/槽位钳位纯函数面（W3-Q28）。
//!
//! **动机**：刀1（PM-17 §5）per-source 唤醒记账的判定律——源值→分类位
//! 映射（[`class_of`]/现 kernel `wake_src_class`）、「可进环」白名单
//! （[`valid`]/现 `wake_src_valid`）、槽表值直索引钳位（[`slot_index`]）
//! ——此前内联在 `kernel/src/telemetry.rs`，host 测不可达（kernel crate
//! no_std + HAL 依赖），ktest WSRC-K 只覆盖 boot 相 + 静态断言。源值域
//! 的精细边界（POWER_KEY 到达-only 排除律 / 0x08–0x7E 预留段恒 false /
//! NONE·UNKNOWN 不进环）需要一张 host 可测的**钉板**：本 crate 把三律
//! 按值域抽成纯函数 + 表驱动测试，kernel telemetry.rs 消费同一函数
//! （零第二实现——抽离即唯一事实源，非镜像）。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 kernel 行为）：
//! - [`WSRC_F_*`]：分类/sticky 位常量（u16 位面——add-only 语义：未定义
//!   源恒 0，位只置不清；HAD_NO_WAITER sticky / SLEPT v2 预留恒不置）。
//! - [`SLOTS`]：槽表槽数（= 值域上界 0x7F + 1——值直索引 `table[src]`；
//!   128 槽 × 64B = 8KB .bss，`WakeSrcRec` 布局执法留 kernel 侧 const
//!   assert，本面只钉槽数与值域的闭合关系）。
//! - [`class_of`]：源值 → 分类位。TIMER=定时器类 / UART·POWER_KEY=外设
//!   线位 / SGI×2=内部位 / IPC·KERNEL_BOOT·NONE·UNKNOWN·未定义=0。
//!   注意 POWER_KEY 有分类位（IRQ 线到达计数点会置位——note_irq_arrival
//!   消费）但不在环白名单（到达-only，无唤醒路径）。
//! - [`valid`]：「可进环」白名单——6 真源（KERNEL_BOOT/TIMER/UART_RX/
//!   SGI_RESCHED/SGI_TLB/IPC）过；NONE（哨兵）/UNKNOWN/POWER_KEY
//!   （到达-only）/未定义段（0x08–0x7E）恒 false。与 ktest WSRC-K B、
//!   `[[wakesrc]]` 生成器三方同源；写点 panic 在前、本谓词是环条目
//!   事后审计面（wake_check ⑤ 收口）。
//! - [`slot_index`]：槽位钳位读——`(src as usize).min(SLOTS - 1)`。
//!   kernel 四消费点（wakes 读/快照/到达记账/无等待者记账）+ on_wake
//!   写点的同款钳位，单源化防漂移。0x80–0xFF 值（若 HAL cell 被外物
//!   写坏）钳到 127 号槽（UNKNOWN 端点槽），不越界不 panic。
//!
//! 不在此 crate（留 kernel 壳）：`WAKE_SRC_TABLE` 原子槽表与
//! `WakeSrcRec` 布局（`#[repr(C)]` 64B ABI 面）、seqlock 环、直方图
//! 计数器、HAL 合并（`wake_src_merge_hal`）——本面只钉「源值怎么分
//! 类/谁可进环/槽怎么钳」，不钉「原子怎么打」。
//!
//! **源值常量不迁**：`WAKE_SRC_*` 九值定义在 HAL（`arch/aarch64/src/
//! wake_gen.rs`）——HAL 语境 cell 的值域是 HAL ABI 面，wsrcface 依赖
//! HAL 会把纯面拖进 arch 依赖（host 测不可达）。本面以**值域参数化**
//! 形态钉板：测试用 HAL 冻结值镜像（0x00/0x01/…/0x07/0x7F——与 ktest
//! WSRC-K A 端点锚同源），消费侧 kernel 传 HAL 常量。若 HAL 改值，
//! 本 crate 测试的镜像锚与 ktest 同时红——双面执法。

#![cfg_attr(not(test), no_std)]

/// 槽表槽数 = 值域上界 + 1（源值 ≤ 0x7F；值直索引——`table[src]`）。
/// kernel 侧 `WAKE_SRC_SLOTS` re-export 本值（128 槽 × 64B = 8KB .bss）。
pub const SLOTS: usize = 128;

/// 值域上界（UNKNOWN 端点锚——HAL 冻结值 0x7F；测试镜像同源）。
pub const SRC_MAX: u8 = 0x7F;

// ---- 分类/sticky 位（u16 位面；kernel telemetry.rs re-export 单源）----

/// 定时器类（TIMER）。
pub const WSRC_F_TIMER_CLASS: u16 = 1 << 0;
/// 外设线（UART_RX / POWER_KEY 到达计数）。
pub const WSRC_F_IRQ_LINE: u16 = 1 << 1;
/// SGI 内部（RESCHED/TLB）。
pub const WSRC_F_SGI_INTERNAL: u16 = 1 << 2;
/// 曾零唤醒 sticky（UART 到达无等待者——良性竞态记账，只置不清）。
pub const WSRC_F_HAD_NO_WAITER: u16 = 1 << 3;
/// SLEPT 域事件见过（v2 挂起双钟预留，v1 恒不置）。
pub const WSRC_F_SLEPT: u16 = 1 << 4;

// ---- HAL 冻结值镜像（仅测试用——消费侧传 HAL 常量；值漂移双面红）----
// 与 ktest WSRC-K A 端点锚同源：NONE=0x00 哨兵 / POWER_KEY=0x07 到达-only
// 占用（sim-S2 起 0x08–0x7E 是预留段而非自由段）/ UNKNOWN=0x7F 端点。

/// 测试镜像：NONE 哨兵（HAL `WAKE_SRC_NONE`）。
#[cfg(test)]
pub const M_NONE: u8 = 0x00;
/// 测试镜像：KERNEL_BOOT（HAL `WAKE_SRC_KERNEL_BOOT`）。
#[cfg(test)]
pub const M_KERNEL_BOOT: u8 = 0x01;
/// 测试镜像：TIMER（HAL `WAKE_SRC_TIMER`）。
#[cfg(test)]
pub const M_TIMER: u8 = 0x02;
/// 测试镜像：UART_RX（HAL `WAKE_SRC_UART_RX`）。
#[cfg(test)]
pub const M_UART_RX: u8 = 0x03;
/// 测试镜像：SGI_RESCHED（HAL `WAKE_SRC_SGI_RESCHED`）。
#[cfg(test)]
pub const M_SGI_RESCHED: u8 = 0x04;
/// 测试镜像：SGI_TLB（HAL `WAKE_SRC_SGI_TLB`）。
#[cfg(test)]
pub const M_SGI_TLB: u8 = 0x05;
/// 测试镜像：IPC（HAL `WAKE_SRC_IPC`）。
#[cfg(test)]
pub const M_IPC: u8 = 0x06;
/// 测试镜像：POWER_KEY 到达-only（HAL `WAKE_SRC_POWER_KEY`）。
#[cfg(test)]
pub const M_POWER_KEY: u8 = 0x07;
/// 测试镜像：UNKNOWN 端点（HAL `WAKE_SRC_UNKNOWN`）。
#[cfg(test)]
pub const M_UNKNOWN: u8 = 0x7F;

/// 源值 → 分类位（纯函数；现 kernel `wake_src_class`——语义逐字对齐）。
/// 未定义源 = 0。POWER_KEY 有 IRQ 线分类位（到达计数点置位——它进
/// 槽表进 irq_count，只是不进环、不计 wakes）。
pub fn class_of(src: u8) -> u16 {
    match src {
        0x02 => WSRC_F_TIMER_CLASS,
        0x03 | 0x07 => WSRC_F_IRQ_LINE,
        0x04 | 0x05 => WSRC_F_SGI_INTERNAL,
        _ => 0,
    }
}

/// 「可进环」白名单（现 kernel `wake_src_valid`——语义逐字对齐）。
/// 6 真源过；NONE 哨兵 / UNKNOWN / POWER_KEY 到达-only / 0x08–0x7E
/// 预留段恒 false。写点 panic 在前，本谓词是环条目事后审计面。
pub fn valid(src: u8) -> bool {
    matches!(src, 0x01..=0x06)
}

/// 槽位钳位读：`(src as usize).min(SLOTS - 1)`。kernel 四读点 +
/// on_wake 写点共五处同款钳位单源化；0x80–0xFF 坏值钳 127 号
/// （UNKNOWN 端点槽），不越界不 panic。
pub fn slot_index(src: u8) -> usize {
    (src as usize).min(SLOTS - 1)
}

// ---------------------------------------------------------------------------
// 表驱动测试（host 面——ktest WSRC-K 的全域扩展：256 值全扫描 + 表锚）
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// 表驱动锚：9 定义源 ×（分类位，环白名单）两列全表。
    /// 与 ktest WSRC-K B/C 逐行同源（kernel 侧消费同函数后两测共用本表
    /// 语义）。
    #[test]
    fn test_defined_sources_table() {
        // (源, 分类位, 可进环)
        let table = [
            (M_NONE, 0u16, false),
            (M_KERNEL_BOOT, 0, true),
            (M_TIMER, WSRC_F_TIMER_CLASS, true),
            (M_UART_RX, WSRC_F_IRQ_LINE, true),
            (M_SGI_RESCHED, WSRC_F_SGI_INTERNAL, true),
            (M_SGI_TLB, WSRC_F_SGI_INTERNAL, true),
            (M_IPC, 0, true),
            (M_POWER_KEY, WSRC_F_IRQ_LINE, false),
            (M_UNKNOWN, 0, false),
        ];
        for (src, cls, in_ring) in table {
            assert_eq!(class_of(src), cls, "class_of({:#04x})", src);
            assert_eq!(valid(src), in_ring, "valid({:#04x})", src);
        }
    }

    /// 值域互异 + 端点锚（ktest WSRC-K A 的 host 镜像——HAL 冻结值漂移
    /// 双面红）。
    #[test]
    fn test_mirror_anchor_values() {
        let all = [
            M_NONE, M_KERNEL_BOOT, M_TIMER, M_UART_RX, M_SGI_RESCHED, M_SGI_TLB, M_IPC,
            M_POWER_KEY, M_UNKNOWN,
        ];
        for (i, &v) in all.iter().enumerate() {
            assert!(v <= SRC_MAX, "src[{}]={:#04x} 超 0x7F", i, v);
            for &w in &all[..i] {
                assert_ne!(v, w, "源值撞号 {:#04x}", v);
            }
        }
        assert_eq!(M_NONE, 0x00);
        assert_eq!(M_POWER_KEY, 0x07, "POWER_KEY 到达-only 锚（sim-S2 A4②）");
        assert_eq!(M_UNKNOWN, 0x7F, "UNKNOWN 端点锚");
        // 槽数与值域闭合：SLOTS = SRC_MAX + 1（值直索引容量界）。
        assert_eq!(SLOTS, SRC_MAX as usize + 1);
    }

    /// 全域 256 值扫描：分类位只有三值（0/单类位）；白名单只放行
    /// 0x01–0x06 连续段；预留段 0x08–0x7E 全拒。
    #[test]
    fn test_full_domain_scan() {
        for v in 0u16..=255 {
            let b = v as u8;
            let cls = class_of(b);
            // 分类位域闭合：只有 0 / 三类位之一，无组合位。
            assert!(
                cls == 0
                    || cls == WSRC_F_TIMER_CLASS
                    || cls == WSRC_F_IRQ_LINE
                    || cls == WSRC_F_SGI_INTERNAL,
                "class_of({:#04x}) = {:#06x} 非法组合位",
                b,
                cls
            );
            // 白名单 = 恰 0x01–0x06（连续段——预留段与哨兵全拒）。
            let expect_valid = (0x01..=0x06).contains(&v);
            assert_eq!(valid(b), expect_valid, "valid({:#04x})", b);
        }
    }

    /// 钳位律：0x00–0x7F 恒等映射；0x80–0xFF 钳 127（UNKNOWN 端点槽）。
    /// kernel 五消费点（wakes 读/快照/到达/无等待者/on_wake 写）同款。
    #[test]
    fn test_slot_index_clamp() {
        assert_eq!(SLOTS, 128);
        for v in 0u16..=255 {
            let b = v as u8;
            let idx = slot_index(b);
            assert!(idx < SLOTS, "slot_index({:#04x})={} 越界", b, idx);
            if v <= 0x7F {
                assert_eq!(idx, v as usize, "≤0x7F 须恒等映射");
            } else {
                assert_eq!(idx, 127, "≥0x80 须钳 127");
            }
        }
        // 恰界三点：127 恒等 / 128 钳 / 255 钳。
        assert_eq!(slot_index(0x7F), 127);
        assert_eq!(slot_index(0x80), 127);
        assert_eq!(slot_index(0xFF), 127);
    }

    /// 位面冻结值（ktest WSRC-K C 的 host 镜像）：五分类/sticky 位
    /// 互异、低 5 位闭合、add-only 语义的位面根基。
    #[test]
    fn test_flag_bits_frozen() {
        assert_eq!(WSRC_F_TIMER_CLASS, 1 << 0);
        assert_eq!(WSRC_F_IRQ_LINE, 1 << 1);
        assert_eq!(WSRC_F_SGI_INTERNAL, 1 << 2);
        assert_eq!(WSRC_F_HAD_NO_WAITER, 1 << 3);
        assert_eq!(WSRC_F_SLEPT, 1 << 4);
        // 位面闭合：已知位族 = 低 5 位。
        let all = WSRC_F_TIMER_CLASS
            | WSRC_F_IRQ_LINE
            | WSRC_F_SGI_INTERNAL
            | WSRC_F_HAD_NO_WAITER
            | WSRC_F_SLEPT;
        assert_eq!(all, 0b11111);
        // class_of 产出 ⊆ 三分类位（sticky 位不来自分类面）。
        for v in 0u16..=255 {
            let cls = class_of(v as u8);
            assert_eq!(cls & !all, 0, "class_of 产出溢出已知位族");
            assert_eq!(cls & (WSRC_F_HAD_NO_WAITER | WSRC_F_SLEPT), 0);
        }
    }

    /// POWER_KEY 排除律单钉（到达-only 语义的完整三角）：有分类位（IRQ
    /// 计数点会置 flags）∧ 不在环白名单 ∧ 槽位可落（值直索引 7 号槽）。
    /// ——「进槽表不进环」三面一致才不违约。
    #[test]
    fn test_power_key_arrival_only_triangle() {
        assert_ne!(class_of(M_POWER_KEY), 0, "到达计数点须有分类位");
        assert!(!valid(M_POWER_KEY), "v1 电源键恒无唤醒路径");
        assert_eq!(slot_index(M_POWER_KEY), 7);
        // 反三角对照：UART_RX 同有 IRQ 线位但可进环（有等待者的外设线）。
        assert_eq!(class_of(M_UART_RX), WSRC_F_IRQ_LINE);
        assert!(valid(M_UART_RX));
    }

    /// 预留段律：0x08–0x7E 恒 false + 分类 0（69 值连续抽查边界）。
    /// sim-S2 起 POWER_KEY 占 0x07 后预留段起点从 0x08 起算。
    #[test]
    fn test_reserved_segment() {
        for v in 0x08u16..=0x7E {
            assert!(!valid(v as u8), "预留段 {:#04x} 竟过白名单", v);
            assert_eq!(class_of(v as u8), 0, "预留段 {:#04x} 有分类位", v);
            // 预留段槽位可落（值直索引——到达坏值不 panic）。
            assert_eq!(slot_index(v as u8), v as usize);
        }
    }

    /// 消费面契约（kernel 侧五消费点同款判序）：note_irq_arrival 的
    /// flags 置位门——`cls != 0` 才 fetch_or（0 位 fetch_or 无害但语义
    /// 噪声，kernel 判序钉板）。未定义源到达计数仍走（irq_count 无
    /// 白名单门——unknown_arrivals 合并进 0x7F 槽先例）。
    #[test]
    fn test_irq_arrival_flag_gate() {
        for v in 0u16..=255 {
            let b = v as u8;
            let cls = class_of(b);
            // 到达记账门（kernel note_irq_arrival 判序镜像）。
            if cls != 0 {
                // 会置位的源：TIMER/UART/POWER_KEY/SGI×2 五源。
                assert!(
                    b == M_TIMER
                        || b == M_UART_RX
                        || b == M_POWER_KEY
                        || b == M_SGI_RESCHED
                        || b == M_SGI_TLB,
                    "有分类位的意外源 {:#04x}",
                    b
                );
            }
        }
    }
}
