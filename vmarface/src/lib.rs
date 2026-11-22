//! berry-vmarface —— 层级 `Vmar` 有序区间树、子区间切分与子空间回收纯状态机（F1-1）。
//!
//! 将整个 `AddressSpace` 的虚拟区间拓扑（根 `Vmar`、多层嵌套子 `Vmar`、`Mapped` 叶区间、
//! `Demand` 按需区间）收敛为 `no_std + alloc`、**零 `unsafe`** 的有序 `BTreeMap` 状态机：
//! 1. **O(log N) 重叠检测与缺页定位**：替代旧线性 `Vec<Region>` O(N) 扫描；`demand_hit(far)`
//!    跨根与任意深度子 `Vmar` 单次树探即中，彻底根除子 `Vmar` 按需映射 `OwnerConflict`。
//! 2. **精确与任意子区间 `unmap` / `protect` 自动切分**：支持对已映射段的前缀、中段、后缀或跨相邻
//!    多段执行 `unmap_range` / `protect_range`，经 [`SliceMeta::slice_at`] 自动推进右半段 VMO 字节偏移。
//! 3. **子 `Vmar` 确定性回收**：`destroy_vmar(id)` 递归摘除全部后代映射并从父 `Vmar` 移除 `SubVmar`
//!    预留占位，使父区间立即可重新映射；`drain_all()` 为 `F1-2` 进程退出整空间拆解提供一趟排水原语。

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// 页粒度（4 KiB）。
pub const PAGE_SIZE: usize = 0x1000;

/// 逻辑 `Vmar` 节点标识（同一 `AddressSpace` 内单调唯一）。
pub type VmarId = u64;

/// 根 `Vmar` 恒定 ID。
pub const ROOT_VMAR_ID: VmarId = 1;

/// 关联元数据切分契约：当区间 `[start, start + len)` 在 `start + byte_delta` 处被切分时，
/// 右半段元数据（如 `(vmo_ptr, vmo_off)`）据此前推偏移。
pub trait SliceMeta: Copy + Eq {
    fn slice_at(&self, byte_delta: usize) -> Self;
}

impl SliceMeta for () {
    #[inline]
    fn slice_at(&self, _byte_delta: usize) -> Self {}
}

impl SliceMeta for (usize, usize) {
    #[inline]
    fn slice_at(&self, byte_delta: usize) -> Self {
        (self.0, self.1 + byte_delta)
    }
}

/// 叶映射种类（`Mapped` 实页映射 vs `Demand` 按需缺页映射）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafKind {
    Mapped,
    Demand,
}

/// 区间树错误码（严格保持 `Unaligned -> OutOfBounds -> Overlap` 判序优先级）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmarTreeError {
    Unaligned,
    OutOfBounds,
    Overlap,
    NotMapped,
    InvalidVmar,
}

/// 单个 `Vmar` 作用域内的直接子区间项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeSpan<M: SliceMeta> {
    Leaf {
        len: usize,
        kind: LeafKind,
        perm: u8,
        meta: M,
    },
    SubVmar {
        len: usize,
        child_id: VmarId,
    },
}

impl<M: SliceMeta> ScopeSpan<M> {
    #[inline]
    fn len(&self) -> usize {
        match *self {
            ScopeSpan::Leaf { len, .. } | ScopeSpan::SubVmar { len, .. } => len,
        }
    }
}

/// 全地址空间扁平叶索引项（仅收录 `Mapped` 与 `Demand` 叶区间，两两严格不交）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveLeaf<M: SliceMeta> {
    pub owner: VmarId,
    pub start: usize,
    pub len: usize,
    pub kind: LeafKind,
    pub perm: u8,
    pub meta: M,
}

/// `unmap_range` / `destroy_vmar` / `drain_all` 返回的待清叶与退钉片段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedSpan<M: SliceMeta> {
    pub start: usize,
    pub len: usize,
    pub kind: LeafKind,
    pub perm: u8,
    pub meta: M,
}

/// `protect_range` 返回的已改权片段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtectedSpan<M: SliceMeta> {
    pub start: usize,
    pub len: usize,
    pub old_perm: u8,
    pub new_perm: u8,
    pub meta: M,
}

struct VmarScope<M: SliceMeta> {
    parent: Option<VmarId>,
    base: usize,
    len: usize,
    spans: BTreeMap<usize, ScopeSpan<M>>,
}

/// 单一 `AddressSpace` 内的完整层级 `Vmar` 区间树。
pub struct VmarTree<M: SliceMeta> {
    next_id: VmarId,
    scopes: BTreeMap<VmarId, VmarScope<M>>,
    active_leaves: BTreeMap<usize, ActiveLeaf<M>>,
}

#[inline]
pub const fn aligned_nonzero(vaddr: usize, len: usize) -> bool {
    vaddr % PAGE_SIZE == 0 && len % PAGE_SIZE == 0 && len != 0
}

#[inline]
pub const fn bounds_end(vaddr: usize, len: usize, base: usize, span: usize) -> Option<usize> {
    if !aligned_nonzero(vaddr, len) {
        return None;
    }
    let Some(end) = vaddr.checked_add(len) else {
        return None;
    };
    let Some(limit) = base.checked_add(span) else {
        return None;
    };
    if vaddr >= base && end <= limit {
        Some(end)
    } else {
        None
    }
}

impl<M: SliceMeta> VmarTree<M> {
    /// 创建以 `[base, base + len)` 为根作用域的区间树（根 ID = [`ROOT_VMAR_ID`]）。
    pub fn new_root(base: usize, len: usize) -> Self {
        let mut scopes = BTreeMap::new();
        scopes.insert(
            ROOT_VMAR_ID,
            VmarScope {
                parent: None,
                base,
                len,
                spans: BTreeMap::new(),
            },
        );
        Self {
            next_id: ROOT_VMAR_ID + 1,
            scopes,
            active_leaves: BTreeMap::new(),
        }
    }

    /// 查询指定 `vmar_id` 的管辖区间 `(base, len)`。
    pub fn scope_bounds(&self, vmar_id: VmarId) -> Option<(usize, usize)> {
        self.scopes.get(&vmar_id).map(|s| (s.base, s.len))
    }

    /// O(log N) 槽位三查：`Unaligned -> OutOfBounds -> Overlap`。
    pub fn check_slot(&self, vmar_id: VmarId, vaddr: usize, len: usize) -> Result<usize, VmarTreeError> {
        let scope = self.scopes.get(&vmar_id).ok_or(VmarTreeError::InvalidVmar)?;
        if !aligned_nonzero(vaddr, len) {
            return Err(VmarTreeError::Unaligned);
        }
        let end = bounds_end(vaddr, len, scope.base, scope.len).ok_or(VmarTreeError::OutOfBounds)?;
        // 因同作用域内区间严格不交，[vaddr, end) 与既有集相交当且仅当 start < end 的最后一个区间右端 > vaddr。
        if let Some((&s_start, s_span)) = scope.spans.range(..end).next_back() {
            if s_start + s_span.len() > vaddr {
                return Err(VmarTreeError::Overlap);
            }
        }
        Ok(end)
    }

    /// 在父 `Vmar` 下切分 `[vaddr, vaddr + len)` 子 `Vmar` 并登记 `SubVmar` 占位。
    pub fn allocate_child(
        &mut self,
        parent_id: VmarId,
        vaddr: usize,
        len: usize,
    ) -> Result<VmarId, VmarTreeError> {
        let _end = self.check_slot(parent_id, vaddr, len)?;
        let child_id = self.next_id;
        self.next_id += 1;
        self.scopes
            .get_mut(&parent_id)
            .expect("check_slot 已校验 parent")
            .spans
            .insert(vaddr, ScopeSpan::SubVmar { len, child_id });
        self.scopes.insert(
            child_id,
            VmarScope {
                parent: Some(parent_id),
                base: vaddr,
                len,
                spans: BTreeMap::new(),
            },
        );
        Ok(child_id)
    }

    /// 登记 `Mapped` 或 `Demand` 叶区间（调用前已完成物理帧/页表前置校验）。
    pub fn insert_leaf(
        &mut self,
        vmar_id: VmarId,
        vaddr: usize,
        len: usize,
        kind: LeafKind,
        perm: u8,
        meta: M,
    ) -> Result<(), VmarTreeError> {
        let _end = self.check_slot(vmar_id, vaddr, len)?;
        self.scopes
            .get_mut(&vmar_id)
            .expect("check_slot 已校验 vmar_id")
            .spans
            .insert(
                vaddr,
                ScopeSpan::Leaf {
                    len,
                    kind,
                    perm,
                    meta,
                },
            );
        self.active_leaves.insert(
            vaddr,
            ActiveLeaf {
                owner: vmar_id,
                start: vaddr,
                len,
                kind,
                perm,
                meta,
            },
        );
        Ok(())
    }

    /// O(log N) 全空间查找包含 `va` 的活跃叶区间（跨根与所有子 `Vmar`）。
    pub fn find_leaf_at(&self, va: usize) -> Option<ActiveLeaf<M>> {
        let (&start, leaf) = self.active_leaves.range(..=va).next_back()?;
        if va < start + leaf.len {
            Some(*leaf)
        } else {
            None
        }
    }

    /// O(log N) 缺页探针命中判定：`far` 落入任意 `Demand` 区间即返回其完整描述。
    pub fn demand_hit(&self, far: usize) -> Option<ActiveLeaf<M>> {
        let leaf = self.find_leaf_at(far)?;
        (leaf.kind == LeafKind::Demand).then_some(leaf)
    }

    /// 解除 `[vaddr, vaddr + len)` 映射（支持精确匹配与任意连续子区间/跨相邻多段切分）。
    ///
    /// 若请求区间未页对齐返 `Unaligned`；若越出 `vmar_id` 管辖界、含未映射空洞或触碰 `SubVmar`
    /// 返 `NotMapped`（先全段无副作用校验，通过后再原子切分更新，绝不留半截状态）。
    pub fn unmap_range(
        &mut self,
        vmar_id: VmarId,
        vaddr: usize,
        len: usize,
    ) -> Result<Vec<RemovedSpan<M>>, VmarTreeError> {
        let scope = self.scopes.get(&vmar_id).ok_or(VmarTreeError::InvalidVmar)?;
        if !aligned_nonzero(vaddr, len) {
            return Err(VmarTreeError::Unaligned);
        }
        let Some(end) = bounds_end(vaddr, len, scope.base, scope.len) else {
            return Err(VmarTreeError::NotMapped);
        };

        // 第一遍只读校验：确认 [vaddr, end) 被本 Vmar 的 Leaf 区间无缝全覆盖。
        let mut cursor = vaddr;
        let mut touched: Vec<(usize, LeafKind, usize, u8, M)> = Vec::new();
        if let Some((&p_start, p_span)) = scope.spans.range(..=vaddr).next_back() {
            if p_start + p_span.len() > vaddr {
                match *p_span {
                    ScopeSpan::Leaf {
                        len: s_len,
                        kind,
                        perm,
                        meta,
                    } => {
                        touched.push((p_start, kind, s_len, perm, meta));
                        cursor = (p_start + s_len).min(end);
                    }
                    ScopeSpan::SubVmar { .. } => return Err(VmarTreeError::NotMapped),
                }
            }
        }
        for (&s_start, s_span) in scope.spans.range(vaddr..end) {
            if s_start <= vaddr && !touched.is_empty() {
                continue;
            }
            if s_start != cursor {
                return Err(VmarTreeError::NotMapped);
            }
            match *s_span {
                ScopeSpan::Leaf {
                    len: s_len,
                    kind,
                    perm,
                    meta,
                } => {
                    touched.push((s_start, kind, s_len, perm, meta));
                    cursor = (s_start + s_len).min(end);
                }
                ScopeSpan::SubVmar { .. } => return Err(VmarTreeError::NotMapped),
            }
        }
        if cursor != end || touched.is_empty() {
            return Err(VmarTreeError::NotMapped);
        }

        // 第二遍执行切分更新（校验已全过，以下纯内存写入必成）。
        let scope_mut = self.scopes.get_mut(&vmar_id).unwrap();
        let mut removed = Vec::with_capacity(touched.len());
        for (s_start, kind, s_len, perm, meta) in touched {
            let s_end = s_start + s_len;
            scope_mut.spans.remove(&s_start);
            self.active_leaves.remove(&s_start);

            let cut_start = vaddr.max(s_start);
            let cut_end = end.min(s_end);
            let cut_meta = meta.slice_at(cut_start - s_start);
            removed.push(RemovedSpan {
                start: cut_start,
                len: cut_end - cut_start,
                kind,
                perm,
                meta: cut_meta,
            });

            // 保留左侧残余前缀 [s_start, vaddr)
            if s_start < vaddr {
                let left_len = vaddr - s_start;
                scope_mut.spans.insert(
                    s_start,
                    ScopeSpan::Leaf {
                        len: left_len,
                        kind,
                        perm,
                        meta,
                    },
                );
                self.active_leaves.insert(
                    s_start,
                    ActiveLeaf {
                        owner: vmar_id,
                        start: s_start,
                        len: left_len,
                        kind,
                        perm,
                        meta,
                    },
                );
            }
            // 保留右侧残余后缀 [end, s_end)
            if end < s_end {
                let right_len = s_end - end;
                let right_meta = meta.slice_at(end - s_start);
                scope_mut.spans.insert(
                    end,
                    ScopeSpan::Leaf {
                        len: right_len,
                        kind,
                        perm,
                        meta: right_meta,
                    },
                );
                self.active_leaves.insert(
                    end,
                    ActiveLeaf {
                        owner: vmar_id,
                        start: end,
                        len: right_len,
                        kind,
                        perm,
                        meta: right_meta,
                    },
                );
            }
        }
        Ok(removed)
    }

    /// 修改 `[vaddr, vaddr + len)` 权限（支持精确匹配与连续 `Mapped` 子区间切分；`Demand` / 空洞拒 `NotMapped`）。
    pub fn protect_range(
        &mut self,
        vmar_id: VmarId,
        vaddr: usize,
        len: usize,
        new_perm: u8,
    ) -> Result<Vec<ProtectedSpan<M>>, VmarTreeError> {
        let scope = self.scopes.get(&vmar_id).ok_or(VmarTreeError::InvalidVmar)?;
        if !aligned_nonzero(vaddr, len) {
            return Err(VmarTreeError::Unaligned);
        }
        let Some(end) = bounds_end(vaddr, len, scope.base, scope.len) else {
            return Err(VmarTreeError::NotMapped);
        };

        let mut cursor = vaddr;
        let mut touched: Vec<(usize, usize, u8, M)> = Vec::new();
        if let Some((&p_start, p_span)) = scope.spans.range(..=vaddr).next_back() {
            if p_start + p_span.len() > vaddr {
                match *p_span {
                    ScopeSpan::Leaf {
                        len: s_len,
                        kind: LeafKind::Mapped,
                        perm,
                        meta,
                    } => {
                        touched.push((p_start, s_len, perm, meta));
                        cursor = (p_start + s_len).min(end);
                    }
                    _ => return Err(VmarTreeError::NotMapped),
                }
            }
        }
        for (&s_start, s_span) in scope.spans.range(vaddr..end) {
            if s_start <= vaddr && !touched.is_empty() {
                continue;
            }
            if s_start != cursor {
                return Err(VmarTreeError::NotMapped);
            }
            match *s_span {
                ScopeSpan::Leaf {
                    len: s_len,
                    kind: LeafKind::Mapped,
                    perm,
                    meta,
                } => {
                    touched.push((s_start, s_len, perm, meta));
                    cursor = (s_start + s_len).min(end);
                }
                _ => return Err(VmarTreeError::NotMapped),
            }
        }
        if cursor != end || touched.is_empty() {
            return Err(VmarTreeError::NotMapped);
        }

        let scope_mut = self.scopes.get_mut(&vmar_id).unwrap();
        let mut out = Vec::with_capacity(touched.len());
        for (s_start, s_len, old_perm, meta) in touched {
            let s_end = s_start + s_len;
            scope_mut.spans.remove(&s_start);
            self.active_leaves.remove(&s_start);

            if s_start < vaddr {
                let left_len = vaddr - s_start;
                scope_mut.spans.insert(
                    s_start,
                    ScopeSpan::Leaf {
                        len: left_len,
                        kind: LeafKind::Mapped,
                        perm: old_perm,
                        meta,
                    },
                );
                self.active_leaves.insert(
                    s_start,
                    ActiveLeaf {
                        owner: vmar_id,
                        start: s_start,
                        len: left_len,
                        kind: LeafKind::Mapped,
                        perm: old_perm,
                        meta,
                    },
                );
            }

            let mid_start = vaddr.max(s_start);
            let mid_end = end.min(s_end);
            let mid_len = mid_end - mid_start;
            let mid_meta = meta.slice_at(mid_start - s_start);
            scope_mut.spans.insert(
                mid_start,
                ScopeSpan::Leaf {
                    len: mid_len,
                    kind: LeafKind::Mapped,
                    perm: new_perm,
                    meta: mid_meta,
                },
            );
            self.active_leaves.insert(
                mid_start,
                ActiveLeaf {
                    owner: vmar_id,
                    start: mid_start,
                    len: mid_len,
                    kind: LeafKind::Mapped,
                    perm: new_perm,
                    meta: mid_meta,
                },
            );
            out.push(ProtectedSpan {
                start: mid_start,
                len: mid_len,
                old_perm,
                new_perm,
                meta: mid_meta,
            });

            if end < s_end {
                let right_len = s_end - end;
                let right_meta = meta.slice_at(end - s_start);
                scope_mut.spans.insert(
                    end,
                    ScopeSpan::Leaf {
                        len: right_len,
                        kind: LeafKind::Mapped,
                        perm: old_perm,
                        meta: right_meta,
                    },
                );
                self.active_leaves.insert(
                    end,
                    ActiveLeaf {
                        owner: vmar_id,
                        start: end,
                        len: right_len,
                        kind: LeafKind::Mapped,
                        perm: old_perm,
                        meta: right_meta,
                    },
                );
            }
        }
        Ok(out)
    }

    /// 销毁指定 `vmar_id`（递归回收其后代子 `Vmar` 与全部叶映射，并从父 `Vmar` 移除 `SubVmar` 占位）。
    /// 对已销毁 ID 幂等返回空 `Vec`。
    pub fn destroy_vmar(&mut self, vmar_id: VmarId) -> Vec<RemovedSpan<M>> {
        let mut out = Vec::new();
        self.destroy_vmar_into(vmar_id, true, &mut out);
        out
    }

    fn destroy_vmar_into(&mut self, vmar_id: VmarId, unlink_parent: bool, out: &mut Vec<RemovedSpan<M>>) {
        let Some(scope) = self.scopes.remove(&vmar_id) else {
            return;
        };
        if unlink_parent {
            if let Some(pid) = scope.parent {
                if let Some(p_scope) = self.scopes.get_mut(&pid) {
                    if let Some(ScopeSpan::SubVmar { child_id, .. }) = p_scope.spans.get(&scope.base)
                    {
                        if *child_id == vmar_id {
                            p_scope.spans.remove(&scope.base);
                        }
                    }
                }
            }
        }
        for (start, span) in scope.spans {
            match span {
                ScopeSpan::Leaf {
                    len,
                    kind,
                    perm,
                    meta,
                } => {
                    self.active_leaves.remove(&start);
                    out.push(RemovedSpan {
                        start,
                        len,
                        kind,
                        perm,
                        meta,
                    });
                }
                ScopeSpan::SubVmar { child_id, .. } => {
                    self.destroy_vmar_into(child_id, false, out);
                }
            }
        }
    }

    /// 整空间一次性排空全部活跃映射（供 `F1-2` `AddressSpace::teardown()` 使用）。
    pub fn drain_all(&mut self) -> Vec<RemovedSpan<M>> {
        self.scopes.clear();
        let leaves = core::mem::take(&mut self.active_leaves);
        leaves
            .into_values()
            .map(|l| RemovedSpan {
                start: l.start,
                len: l.len,
                kind: l.kind,
                perm: l.perm,
                meta: l.meta,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: usize = 0x1000;
    const SPAN: usize = (1usize << 39) - BASE;

    #[test]
    fn precedence_unaligned_before_oob_before_overlap() {
        let mut t = VmarTree::<(usize, usize)>::new_root(BASE, SPAN);
        t.insert_leaf(ROOT_VMAR_ID, 0x10000, 0x2000, LeafKind::Mapped, 3, (1, 0))
            .unwrap();
        // 未对齐优先于界外与重叠
        assert_eq!(
            t.check_slot(ROOT_VMAR_ID, 0x10001, usize::MAX),
            Err(VmarTreeError::Unaligned)
        );
        assert_eq!(
            t.check_slot(ROOT_VMAR_ID, 0, 0x1000),
            Err(VmarTreeError::OutOfBounds)
        );
        assert_eq!(
            t.check_slot(ROOT_VMAR_ID, 0x11000, 0x1000),
            Err(VmarTreeError::Overlap)
        );
        assert_eq!(
            t.check_slot(ROOT_VMAR_ID, 0x0F000, 0x2000),
            Err(VmarTreeError::Overlap)
        );
        assert!(t.check_slot(ROOT_VMAR_ID, 0x12000, 0x1000).is_ok());
    }

    #[test]
    fn child_vmar_demand_hit_and_parent_reclaim_on_destroy() {
        let mut t = VmarTree::<(usize, usize)>::new_root(BASE, SPAN);
        let child = t.allocate_child(ROOT_VMAR_ID, 0x20_0000, 0x4000).unwrap();
        // 父撞子预留须拒 Overlap
        assert_eq!(
            t.insert_leaf(ROOT_VMAR_ID, 0x20_1000, 0x1000, LeafKind::Mapped, 3, (10, 0)),
            Err(VmarTreeError::Overlap)
        );
        // 子越界须拒 OutOfBounds
        assert_eq!(
            t.insert_leaf(child, 0x20_4000, 0x1000, LeafKind::Mapped, 3, (10, 0)),
            Err(VmarTreeError::OutOfBounds)
        );
        // 根与子同时登记 Demand 区间：零冲突且 O(log N) demand_hit 双命中
        t.insert_leaf(ROOT_VMAR_ID, 0x10_0000, 0x2000, LeafKind::Demand, 1, (100, 0))
            .unwrap();
        t.insert_leaf(child, 0x20_1000, 0x2000, LeafKind::Demand, 3, (200, 0x5000))
            .unwrap();
        let hit_root = t.demand_hit(0x10_1800).unwrap();
        assert_eq!((hit_root.start, hit_root.meta), (0x10_0000, (100, 0)));
        let hit_child = t.demand_hit(0x20_2FFF).unwrap();
        assert_eq!((hit_child.start, hit_child.meta), (0x20_1000, (200, 0x5000)));

        // 销毁子 Vmar：返回子内 Demand 区间，且父区间立即可重新映射！
        let removed = t.destroy_vmar(child);
        assert_eq!(removed.len(), 1);
        assert_eq!((removed[0].start, removed[0].len, removed[0].meta), (0x20_1000, 0x2000, (200, 0x5000)));
        assert!(t.demand_hit(0x20_1000).is_none());
        t.insert_leaf(ROOT_VMAR_ID, 0x20_0000, 0x4000, LeafKind::Mapped, 3, (300, 0))
            .expect("子 Vmar 销毁后父区间必须 100% 可重新映射");
    }

    #[test]
    fn subrange_unmap_and_protect_split_offsets_cleanly() {
        let mut t = VmarTree::<(usize, usize)>::new_root(BASE, SPAN);
        // 映射 4 页 [0x40000, 0x44000)，vmo=7, vmo_off=0x10000, perm=RW(3)
        t.insert_leaf(ROOT_VMAR_ID, 0x40000, 0x4000, LeafKind::Mapped, 3, (7, 0x10000))
            .unwrap();

        // 对中间两页 [0x41000, 0x43000) protect 为 RO(1) → 自动裂解为三段且偏移递增
        let prot = t.protect_range(ROOT_VMAR_ID, 0x41000, 0x2000, 1).unwrap();
        assert_eq!(prot.len(), 1);
        assert_eq!((prot[0].start, prot[0].len, prot[0].meta), (0x41000, 0x2000, (7, 0x11000)));

        assert_eq!(t.find_leaf_at(0x40000).unwrap().perm, 3);
        assert_eq!(t.find_leaf_at(0x41000).unwrap().perm, 1);
        assert_eq!(t.find_leaf_at(0x41000).unwrap().meta, (7, 0x11000));
        assert_eq!(t.find_leaf_at(0x43000).unwrap().perm, 3);
        assert_eq!(t.find_leaf_at(0x43000).unwrap().meta, (7, 0x13000));

        // 跨相邻切分段解除 [0x40000, 0x42000)（覆盖第一段整段 + 第二段前半）
        let unm = t.unmap_range(ROOT_VMAR_ID, 0x40000, 0x2000).unwrap();
        assert_eq!(unm.len(), 2);
        assert_eq!((unm[0].start, unm[0].len, unm[0].meta), (0x40000, 0x1000, (7, 0x10000)));
        assert_eq!((unm[1].start, unm[1].len, unm[1].meta), (0x41000, 0x1000, (7, 0x11000)));

        assert!(t.find_leaf_at(0x40000).is_none());
        assert!(t.find_leaf_at(0x41000).is_none());
        let rem_mid = t.find_leaf_at(0x42000).unwrap();
        assert_eq!((rem_mid.start, rem_mid.len, rem_mid.perm, rem_mid.meta), (0x42000, 0x1000, 1, (7, 0x12000)));
        let rem_tail = t.find_leaf_at(0x43000).unwrap();
        assert_eq!((rem_tail.start, rem_tail.len, rem_tail.perm, rem_tail.meta), (0x43000, 0x1000, 3, (7, 0x13000)));

        // 含空洞的 unmap 必须原子拒绝且不破坏剩余两页
        assert_eq!(
            t.unmap_range(ROOT_VMAR_ID, 0x41000, 0x2000),
            Err(VmarTreeError::NotMapped)
        );
        assert!(t.find_leaf_at(0x42000).is_some());
    }

    #[test]
    fn nested_grandchild_destroy_and_drain_all() {
        let mut t = VmarTree::<(usize, usize)>::new_root(BASE, SPAN);
        let c1 = t.allocate_child(ROOT_VMAR_ID, 0x100_0000, 0x10_0000).unwrap();
        let c2 = t.allocate_child(c1, 0x102_0000, 0x4000).unwrap();
        t.insert_leaf(c1, 0x100_0000, 0x2000, LeafKind::Mapped, 3, (1, 0)).unwrap();
        t.insert_leaf(c2, 0x102_0000, 0x2000, LeafKind::Demand, 1, (2, 0x1000)).unwrap();

        let rem = t.destroy_vmar(c1);
        assert_eq!(rem.len(), 2);
        assert!(t.find_leaf_at(0x100_0000).is_none());
        assert!(t.find_leaf_at(0x102_0000).is_none());

        // drain_all 一趟清空测试
        t.insert_leaf(ROOT_VMAR_ID, 0x50_0000, 0x3000, LeafKind::Mapped, 5, (9, 0)).unwrap();
        let all = t.drain_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].start, 0x50_0000);
    }
}
