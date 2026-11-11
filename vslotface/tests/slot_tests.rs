//! berry-vslotface 表驱动测试（W3-Q17）——判序/边界/错误码优先级钉板。

#[cfg(test)]
mod tests {
    use berry_vslotface::*;

    const B: usize = 0x1000_0000; // base
    const S: usize = 0x1000_000; // span 16 MiB

    fn r(start: usize, len: usize, kind: SlotKind) -> SlotRegion {
        SlotRegion { start, len, kind }
    }

    // ---- check_slot_of：三查判序 ----

    #[test]
    fn check_ok_empty() {
        // 空区间表 + 全对齐辖内 → 过
        assert_eq!(check_slot_of(B, 0x1000, B, S, &[]), Ok(()));
        assert_eq!(check_slot_of(B, S, B, S, &[]), Ok(())); // 恰满 span
    }

    #[test]
    fn check_unaligned_first() {
        // 判序钉板：非对齐优先于一切（含界外/重叠同在时也先报 Unaligned）
        assert_eq!(
            check_slot_of(B + 1, 0x1000, B, S, &[]),
            Err(SlotError::Unaligned)
        );
        assert_eq!(
            check_slot_of(B, 0x800, B, S, &[]),
            Err(SlotError::Unaligned)
        );
        assert_eq!(check_slot_of(B, 0, B, S, &[]), Err(SlotError::Unaligned));
        // 非对齐 + 界外 → Unaligned 优先
        assert_eq!(
            check_slot_of(B - 0x10, 0x1000, B, S, &[]),
            Err(SlotError::Unaligned)
        );
    }

    #[test]
    fn check_bounds_domain() {
        // 下界：base 前一页 → OutOfBounds
        assert_eq!(
            check_slot_of(B - 0x1000, 0x1000, B, S, &[]),
            Err(SlotError::OutOfBounds)
        );
        // 上界：恰触 base+span 过、超一页拒
        assert_eq!(check_slot_of(B + S - 0x1000, 0x1000, B, S, &[]), Ok(()));
        assert_eq!(
            check_slot_of(B + S, 0x1000, B, S, &[]),
            Err(SlotError::OutOfBounds)
        );
        // 回卷：vaddr 接近 usize::MAX 时 end 回卷 → OutOfBounds（非 panic）
        assert_eq!(
            check_slot_of(usize::MAX - 0x1FFF, 0x3000, 0, usize::MAX, &[]),
            Err(SlotError::OutOfBounds)
        );
        // 零 base 全域：usize::MAX 侧回卷仍拒
        assert_eq!(
            check_slot_of(usize::MAX - 0xFFF, 0x2000, 0, usize::MAX, &[]),
            Err(SlotError::OutOfBounds)
        );
    }

    #[test]
    fn check_overlap_half_open() {
        // 既有 Mapped 区间 [B, B+0x10000)
        let rs = [r(B, 0x10000, SlotKind::Mapped)];
        // 前贴：end == r.start → 不重叠（半开）
        assert_eq!(check_slot_of(B - 0x1000, 0x1000, B - 0x1000_000, 0x2000_000, &[r(B, 0x10000, SlotKind::Mapped)]), Ok(()));
        // 前交一页（页粒度下最小重叠面）→ 拒；且判序在界检之后（辖内合法）
        assert_eq!(
            check_slot_of(B - 0x1000, 0x2000, B - 0x1000_000, 0x2000_000, &rs),
            Err(SlotError::Overlap)
        );
        // 内含 → 拒
        assert_eq!(
            check_slot_of(B + 0x1000, 0x1000, B, S, &rs),
            Err(SlotError::Overlap)
        );
        // 外包 → 拒
        assert_eq!(
            check_slot_of(B - 0x1000, 0x12000, B - 0x1000_000, 0x2000_000, &rs),
            Err(SlotError::Overlap)
        );
        // 后贴：start == r.start+r.len → 不重叠
        assert_eq!(
            check_slot_of(B + 0x10000, 0x1000, B, S, &rs),
            Ok(())
        );
        // Reserved / Demand 区间同拒重叠（三 kind 一视）
        for k in [SlotKind::Reserved, SlotKind::Demand] {
            assert_eq!(
                check_slot_of(B + 0x1000, 0x1000, B, S, &[r(B, 0x10000, k)]),
                Err(SlotError::Overlap)
            );
        }
    }

    #[test]
    fn check_multi_regions_scan_order() {
        // 多区间表：撞在第二个区间上也能扫到（position 语义 = 首个命中）
        let rs = [
            r(B, 0x1000, SlotKind::Reserved),
            r(B + 0x10000, 0x1000, SlotKind::Mapped),
        ];
        assert_eq!(
            check_slot_of(B + 0x10000, 0x1000, B, S, &rs),
            Err(SlotError::Overlap)
        );
        // 撞第一个 → 同样拒
        assert_eq!(
            check_slot_of(B, 0x1000, B, S, &rs),
            Err(SlotError::Overlap)
        );
        // 都不撞 → 过
        assert_eq!(
            check_slot_of(B + 0x2000, 0x1000, B, S, &rs),
            Ok(())
        );
    }

    // ---- find_exact_of / find_mapped_of ----

    #[test]
    fn find_exact_takes_demand_mapped() {
        let rs = [
            r(B, 0x1000, SlotKind::Reserved),
            r(B + 0x1000, 0x2000, SlotKind::Mapped),
            r(B + 0x3000, 0x1000, SlotKind::Demand),
        ];
        // Reserved 拒（exact 不收）
        assert_eq!(find_exact_at(&rs, B, 0x1000), Err(SlotError::NotMapped));
        // Mapped 收 → 下标 1
        assert_eq!(find_exact_at(&rs, B + 0x1000, 0x2000), Ok(1));
        // Demand 收 → 下标 2
        assert_eq!(find_exact_at(&rs, B + 0x3000, 0x1000), Ok(2));
    }

    #[test]
    fn find_mapped_only_mapped() {
        let rs = [
            r(B + 0x1000, 0x2000, SlotKind::Mapped),
            r(B + 0x3000, 0x1000, SlotKind::Demand),
        ];
        assert_eq!(find_mapped_at(&rs, B + 0x1000, 0x2000), Ok(0));
        // Demand 拒（protect 面）
        assert_eq!(find_mapped_at(&rs, B + 0x3000, 0x1000), Err(SlotError::NotMapped));
        assert_eq!(find_mapped_at(&rs, B, 0x1000), Err(SlotError::NotMapped));
    }

    #[test]
    fn find_no_subrange_or_superrange() {
        // 精确匹配律：start+len 双等才收，子区间/超区间/半边都 NotMapped
        let rs = [r(B, 0x3000, SlotKind::Mapped)];
        assert_eq!(find_exact_at(&rs, B, 0x1000), Err(SlotError::NotMapped)); // 子
        assert_eq!(find_exact_at(&rs, B - 0x1000, 0x3000), Err(SlotError::NotMapped)); // 平移
        assert_eq!(find_exact_at(&rs, B + 0x1000, 0x1000), Err(SlotError::NotMapped)); // 内偏
        assert_eq!(find_mapped_at(&rs, B, 0x4000), Err(SlotError::NotMapped)); // 超
        assert_eq!(find_exact_at(&rs, B, 0x3000), Ok(0)); // 唯一精确形
    }

    #[test]
    fn find_first_hit_wins() {
        // 同 start+len 多区间（理论不共存于真实 Vmar——check_slot 拒重叠，
        // 但 kind 迁移窗口 Reserved→Demand 可能瞬时并存下标序）：position 首
        // 个命中返回。表驱动钉 position 语义。
        let rs = [
            r(B, 0x1000, SlotKind::Reserved),
            r(B + 0x1000, 0x1000, SlotKind::Demand),
            r(B + 0x1000, 0x1000, SlotKind::Mapped),
        ];
        // B+0x1000 有 Demand(idx1) 与 Mapped(idx2) 同形：position → idx1
        assert_eq!(find_exact_at(&rs, B + 0x1000, 0x1000), Ok(1));
        assert_eq!(find_mapped_at(&rs, B + 0x1000, 0x1000), Ok(2));
    }

    // ---- demand_index_of ----

    #[test]
    fn demand_index_hit_miss() {
        let rs = [
            r(B, 0x1000, SlotKind::Mapped),
            r(B + 0x1000, 0x2000, SlotKind::Demand),
        ];
        // 命中域：闭开 [B+0x1000, B+0x3000)
        assert_eq!(demand_hit_at(&rs, B + 0x1000), Some(1)); // 首字节
        assert_eq!(demand_hit_at(&rs, B + 0x2FFF), Some(1)); // 末字节
        // Miss：区间末页外一字节
        assert_eq!(demand_hit_at(&rs, B + 0x3000), None);
        // Miss：Mapped 区间内不收
        assert_eq!(demand_hit_at(&rs, B + 0xFFF), None);
        // Miss：首区间前
        assert_eq!(demand_hit_at(&rs, B - 1), None);
    }

    #[test]
    fn demand_index_multi_demand_first() {
        let rs = [
            r(B, 0x1000, SlotKind::Demand),
            r(B + 0x2000, 0x1000, SlotKind::Demand),
        ];
        assert_eq!(demand_hit_at(&rs, B), Some(0));
        assert_eq!(demand_hit_at(&rs, B + 0x2000), Some(1));
        assert_eq!(demand_hit_at(&rs, B + 0x1000), None); // 两区间夹缝
    }

    // ---- const 上下文可评钉板 ----

    #[test]
    fn const_evaluable() {
        // 两查谓词 const fn：编译期即评钉板
        const ALIGN_OK: bool = aligned_nonzero(0x1000_0000, 0x1000);
        const ALIGN_REJ: bool = aligned_nonzero(0x1000_0001, 0x1000);
        const ZERO_REJ: bool = aligned_nonzero(0x1000_0000, 0);
        assert!(ALIGN_OK);
        assert!(!ALIGN_REJ);
        assert!(!ZERO_REJ);
        const IN: Option<usize> = bounds_end(0x1000_0000, 0x1000, 0x1000_0000, 0x1000_000);
        const OUT: Option<usize> = bounds_end(0x1000_0000, 0x1000_000, 0x1000_0000, 0x1000);
        assert_eq!(IN, Some(0x1000_1000));
        assert_eq!(OUT, None);
        const WRAP: Option<usize> = bounds_end(usize::MAX - 0xFFF, 0x2000, 0, usize::MAX);
        assert_eq!(WRAP, None);
        // 完整三查 const 形（无表）
        const VERDICT: Result<(), SlotError> = check_slot_const(0x1000_0000, 0x1000, 0x1000_0000, 0x1000_000);
        const REJECT: Result<(), SlotError> = check_slot_const(0x1000_0001, 0x1000, 0x1000_0000, 0x1000_000);
        assert_eq!(VERDICT, Ok(()));
        assert_eq!(REJECT, Err(SlotError::Unaligned));
    }
}
