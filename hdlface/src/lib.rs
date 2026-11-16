//! berry-hdlface —— handle 槽位编码纯函数面（W3-Q8）。
//!
//! **动机**：handle 位域语义（决策 #28：槽位[15:0] | 代号[29:16] | bit30/31
//! 预留）此前只活在 `kernel/src/handle.rs` 的私有常量与 `#[cfg(test)]` 外
//! 不可达的三个自由函数里——host 测不可达（kernel 依赖 HAL global_asm，
//! 只 aarch64-unknown-none 可编），ktest 只覆盖 boot 相。位域编码的回归面
//! （别名拒、gen 单调、耗尽跳过、roundtrip）需要一张 host 可测的**钉板**：
//! 本 crate 把位域常量与 encode/decode 按值域抽成纯函数 + 表驱动测试，
//! `kernel` 侧消费同一函数（零第二实现——抽离即唯一事实源，非镜像）。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 kernel 行为）：
//! - [`SLOT_BITS`]/[`SLOT_MASK`]/[`GEN_BITS`]/[`GEN_MASK`]/
//!   [`RESERVED_MASK`]：位域常量单源（决策 #28 + ABI §8.1 预留位）。
//! - [`encode`]：`(slot, gen)` → handle 值；[`decode_slot`]/
//!   [`decode_gen`]：handle 值 → 位域分量（roundtrip 三律）。
//! - [`has_reserved_bits`]：预留位探测（v1 从不发行非零值；lookup 对
//!   非零一律拒——X 与 X|0x8000_0000 别名防线，3b3 审查 M5）。
//! - [`slot_index_valid`]：槽位下标可入表判（≤ SLOT_MASK——alloc_slot
//!   尾插门与表满门共用判据）。
//! - [`gen_in_issue_domain`]：代号可发行判（[1, GEN_MASK]——take_gen
//!   发行域；出生 1、close/take/drain 只 +1、耗尽 > GEN_MASK 拒——防
//!   回卷复活旧 handle）。
//!
//! 与冻结面的关系：syscall ABI 零触碰（handle 值域内部语义，ABI §8.1
//! 位布局本身即冻结常量，本 crate 只钉其编码律）；`HandleTable` 的
//! 槽状态机/锁窗/raw 指针机械不在此 crate（Vec/NonNull 留在 kernel
//! ——本面只钉「值怎么编」的判定律，不钉「表怎么管」的机械）。

#![cfg_attr(not(test), no_std)]

/// handle（`berry_handle_t`，u32；位域见模块注释）——值域别名（kernel
/// 侧 `type Handle = u32` 同源，此处独立声明保持本 crate 零依赖）。
pub type Handle = u32;

// ---- handle 位域（决策 #28：低位槽位、中位代号、高位可选类型）----

/// 槽位号位宽（低 16 位 = 每表最多 65536 槽）。
pub const SLOT_BITS: u32 = 16;
/// 槽位号掩码（低 16 位）。
pub const SLOT_MASK: u32 = (1 << SLOT_BITS) - 1;
/// 代号位宽（bits [16, 30)；bit31 预留类型编码，v1 不写）。
pub const GEN_BITS: u32 = 14;
/// 代号掩码（bits [16, 30) 内 14 位）。
pub const GEN_MASK: u32 = (1 << GEN_BITS) - 1;

/// bit30/31 = ABI §8.1 预留位（v1 内核从不发行非零值）。lookup 对非零
/// 一律拒（3b3 审查 M5）——否则 X 与 X|0x8000_0000 被解码成同一 handle，
/// replace 的裸值 `h == repl` 挡不住这种别名，「任一无效则表不动」的
/// 原子承诺即被打破（调用者收错误码却已永久丢失合法能力）。
pub const RESERVED_MASK: Handle = 0xC000_0000;

/// `(slot, gen)` → handle 值。
///
/// debug 契约（与 kernel 侧 `encode` 的 debug_assert 同款）：slot ≤
/// SLOT_MASK 且 gen ∈ [1, GEN_MASK]——发行侧（insert/duplicate/replace/
/// insert_ref 经 alloc_slot+take_gen）已保证，违者=发行器 bug。
#[inline]
pub fn encode(slot: u32, gen: u32) -> Handle {
    debug_assert!(slot <= SLOT_MASK && gen >= 1 && gen <= GEN_MASK);
    (gen << SLOT_BITS) | slot
}

/// handle 值 → 槽位号分量。
#[inline]
pub fn decode_slot(h: Handle) -> u32 {
    h & SLOT_MASK
}

/// handle 值 → 代号分量。
#[inline]
pub fn decode_gen(h: Handle) -> u32 {
    (h >> SLOT_BITS) & GEN_MASK
}

/// 预留位探测：任一预留位非零 → true（lookup/lookup_at 入口拒判据）。
#[inline]
pub fn has_reserved_bits(h: Handle) -> bool {
    h & RESERVED_MASK != 0
}

/// 槽位下标可入表判：≤ SLOT_MASK（alloc_slot 的「表满 65536 槽」门与
/// 尾插门共用；usize 下标先经本判再收窄 u32）。
#[inline]
pub fn slot_index_valid(slot_idx: usize) -> bool {
    slot_idx as u64 <= SLOT_MASK as u64
}

/// 代号可发行判：[1, GEN_MASK]（take_gen 发行域闭区间；0 = 出生前/
/// 写坏，> GEN_MASK = 耗尽——gen 耗尽槽由 alloc_slot 跳过驻留，拒发行
/// 防回卷复活旧 handle，3b3 审查 H3）。
#[inline]
pub fn gen_in_issue_domain(gen: u32) -> bool {
    (1..=GEN_MASK).contains(&gen)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 表驱动行形状：一次编码律快照（slot/gen 分量 → handle 值）。
    struct Row {
        slot: u32,
        gen: u32,
        h: Handle,
    }

    /// 基线表：覆盖最小/典型/最大三档 + 跨位域进位边界。
    fn rows() -> Vec<Row> {
        vec![
            // 最小发行对（槽 1 起步——槽 0 恒 Free 保留 INVALID；gen 1 起步）。
            Row { slot: 1, gen: 1, h: 0x0001_0001 },
            // 典型对（第 42 槽第 7 代）。
            Row { slot: 42, gen: 7, h: 0x0007_002A },
            // slot 顶格（65535 = SLOT_MASK；gen 最小）。
            Row { slot: SLOT_MASK, gen: 1, h: 0x0001_FFFF },
            // gen 顶格（16383 = GEN_MASK；slot 最小发行位）。
            Row { slot: 1, gen: GEN_MASK, h: 0x3FFF_0001 },
            // 双顶格（发行域内最大 handle 值）。
            Row { slot: SLOT_MASK, gen: GEN_MASK, h: 0x3FFF_FFFF },
            // 跨位域进位边界：slot 顶格 + gen 低档（bit15→bit16 无进位
            // 串扰的见证行——slot 分量不溢出到 gen 位）。
            Row { slot: 0xFFFF, gen: 2, h: 0x0002_FFFF },
        ]
    }

    /// 律 1（roundtrip）：encode(slot,gen) → decode 两分量复原。
    #[test]
    fn t_roundtrip_all_rows() {
        for r in rows() {
            assert_eq!(encode(r.slot, r.gen), r.h, "row ({},{})", r.slot, r.gen);
            assert_eq!(decode_slot(r.h), r.slot);
            assert_eq!(decode_gen(r.h), r.gen);
        }
    }

    /// 律 2（发行域外的 encode = debug 契约违例）：debug_assert 面 host
    /// 钉板（cfg(debug_assertions)——release 构建无此契约）。gen=0（出生
    /// 前/写坏）与 gen=GEN_MASK+1（耗尽后）均应触发；发行域内全段无 panic。
    #[test]
    #[cfg(debug_assertions)]
    fn t_encode_domain_contract() {
        let r0 = std::panic::catch_unwind(|| encode(1, 0));
        assert!(r0.is_err(), "gen=0 must trip debug_assert");
        let rx = std::panic::catch_unwind(|| encode(1, GEN_MASK + 1));
        assert!(rx.is_err(), "gen exhausted must trip debug_assert");
        let rs = std::panic::catch_unwind(|| encode(SLOT_MASK + 1, 1));
        assert!(rs.is_err(), "slot overflow must trip debug_assert");
        // 正向钉发行域内全段无 panic。
        for gen in 1..=GEN_MASK {
            let _ = encode(1, gen);
        }
    }

    /// 律 3（预留位别名拒）：任意合法 handle 值 | 预留位 → 探测必真；
    /// 合法值域内探测恒假。X 与 X|0x8000_0000 / X|0x4000_0000 /
    /// X|0xC000_0000 三形别名全钉（3b3 审查 M5 的值域面）。
    #[test]
    fn t_reserved_alias_reject() {
        for r in rows() {
            assert!(!has_reserved_bits(r.h), "legal h=0x{:08X}", r.h);
            assert!(has_reserved_bits(r.h | 0x8000_0000));
            assert!(has_reserved_bits(r.h | 0x4000_0000));
            assert!(has_reserved_bits(r.h | 0xC000_0000));
            // 别名值 decode 分量与原值相同——正是别名危险所在，探测是
            // 唯一防线（decode 自身不拒）。
            assert_eq!(decode_slot(r.h | 0x8000_0000), r.slot);
            assert_eq!(decode_gen(r.h | 0x8000_0000), r.gen);
        }
        // 全保留值本身（slot=0/gen=0/预留位全 1）。
        assert!(has_reserved_bits(0xC000_0000));
        assert!(has_reserved_bits(u32::MAX));
        // handle 0（INVALID）：非预留位污染（其非法性来自槽 0 恒空，
        // 不来自位域）。
        assert!(!has_reserved_bits(0));
    }

    /// 律 4（slot_index_valid 门）：0..=SLOT_MASK 全过、SLOT_MASK+1 起
    /// 全拒——alloc_slot「表满 65536 槽」门的值域面（usize 收窄前判）。
    #[test]
    fn t_slot_index_valid_gate() {
        assert!(slot_index_valid(0));
        assert!(slot_index_valid(SLOT_MASK as usize));
        assert!(!slot_index_valid(SLOT_MASK as usize + 1));
        assert!(!slot_index_valid(usize::MAX));
        // 全域扫描（65536+2 点——微秒级）。
        for i in 0..=(SLOT_MASK as usize + 1) {
            assert_eq!(slot_index_valid(i), i as u64 <= SLOT_MASK as u64);
        }
    }

    /// 律 5（gen 发行域）：[1, GEN_MASK] 闭区间真、0 与 GEN_MASK+1 假、
    /// 耗尽驻留值（GEN_MASK+1 .. 2*GEN_MASK 采样）恒假——take_gen 拒发行
    /// / alloc_slot 跳过驻留的共判据（3b3 H3 防回卷复活）。
    #[test]
    fn t_gen_issue_domain() {
        assert!(!gen_in_issue_domain(0));
        assert!(gen_in_issue_domain(1));
        assert!(gen_in_issue_domain(GEN_MASK));
        assert!(!gen_in_issue_domain(GEN_MASK + 1));
        // 全域扫描（16384+2 点）。
        for g in 0..=(GEN_MASK + 1) {
            assert_eq!(gen_in_issue_domain(g), (1..=GEN_MASK).contains(&g));
        }
        // 耗尽后驻留值永不回卷复活的值域见证。
        assert!(!gen_in_issue_domain(u32::MAX));
    }

    /// 律 6（位域自洽）：SLOT_BITS+GEN_BITS = 30（bit30/31 恰全预留）；
    /// 掩码互斥无重叠位；SLOT_MASK 顶格恰 65535 槽容量。
    #[test]
    fn t_bitfield_layout_selfconsistent() {
        assert_eq!(SLOT_BITS + GEN_BITS, 30);
        // GEN_MASK 是 14 位裸值、SLOT_MASK 是 16 位裸值——两者按位与不为零
        // 是平凡的；正交性钉在「gen 先左移到自己的窗口再与 slot 域并」：
        // encode(s,g) 的两分量位窗互不相交。
        for r in rows() {
            let h = encode(r.slot, r.gen);
            let slot_bits = h & SLOT_MASK;
            let gen_bits = h >> SLOT_BITS;
            assert_eq!(slot_bits & (gen_bits << SLOT_BITS), 0);
            assert_eq!(slot_bits, r.slot);
            assert_eq!(gen_bits & !GEN_MASK, 0, "gen bits inside window");
        }
        assert_eq!(RESERVED_MASK, 0xC000_0000);
        assert_eq!(RESERVED_MASK & !(SLOT_MASK | (GEN_MASK << SLOT_BITS)), RESERVED_MASK);
        // 发行值域全域 = [0x0001_0001, 0x3FFF_FFFF]（gen≥1、slot≥1 的
        // 实际发行子域顶界；纯值域顶界 0x3FFF_FFFF 由双顶格行见证）。
        assert_eq!(SLOT_MASK, 0xFFFF);
        assert_eq!(GEN_MASK, 0x3FFF);
    }

    /// 律 7（encode 零串扰）：slot 分量变化不影响 gen 分量位、反之亦然
    /// ——位域正交性的操作性钉板（任意对 (s,g) 与 (s',g) 的差恰在低位）。
    #[test]
    fn t_encode_no_crosstalk() {
        let base = encode(0x1234, 0x0BCD);
        for s in [0u32, 1, 0xFFFF, 0x1233, 0x1235] {
            let v = encode(s, 0x0BCD);
            assert_eq!(decode_gen(v), 0x0BCD);
            assert_eq!(decode_slot(v), s);
            assert_eq!(v & !(SLOT_MASK), base & !SLOT_MASK);
        }
        for g in [1u32, 2, 0x3FFF, 0x0BCC, 0x0BCE] {
            let v = encode(0x1234, g);
            assert_eq!(decode_slot(v), 0x1234);
            assert_eq!(decode_gen(v), g);
            assert_eq!(v & SLOT_MASK, base & SLOT_MASK);
        }
    }

    /// 律 8（handle 0 语义钉板）：0 解码得 slot=0/gen=0——两者均非发行
    /// 域（gen_in_issue_domain(0)=假、槽 0 恒 Free），INVALID 的非法性
    /// 来自表侧而非位域侧；探测面（预留位）对 0 恒假。
    #[test]
    fn t_handle_zero_invalid_semantics() {
        assert_eq!(decode_slot(0), 0);
        assert_eq!(decode_gen(0), 0);
        assert!(!gen_in_issue_domain(decode_gen(0)));
        assert!(!has_reserved_bits(0));
    }
}
