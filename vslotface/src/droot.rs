//! Vmar asid→root 反查登记槽（DEMAND_ROOTS）判序纯面（W3-Q60）。
//!
//! 自 `kernel/src/vmar.rs` 外迁的登记/反查/注销三函数判定律（零 unsafe、
//! 零 HAL 依赖——槽表经 [`RootSlots`] 抽象注入，物理原子臂留壳）：
//!
//! - [`register_step`]：登记判序——既有同 asid 槽**先于**空槽扫描（同
//!   (asid, root) 幂等 Ok；异 root `OwnerConflict` 防御拒——单 root 不变量），
//!   首空槽 first-wins；满表 `Full`（静默丢登记 = 探针永不命中 = demand
//!   页死循环杀链，宁拒可响）。
//! - [`lookup_step`]：反查判序——asid 0 哨兵直达 miss；扫描命中 + ptr!=0
//!   才是 Some；**X+0 注销半态按 miss 处理**（deregister 先清 asid 后清
//!   ptr 的镜像序——锁外读者永不见「新 asid + 旧 ptr」）。
//! - [`deregister_step`]：注销判序——**asid+ptr 双匹配**才清（asid 复用后
//!   旧 root 的注销不得误清新登记）；清序恒 asid 先 ptr 后（卫生序）。
//!
//! 纪律与 W3-Q17/Q56 同款（vslotface 同源扩面）：判定律单源迁此，kernel
//! vmar.rs 消费面换调用，行为逐字等价（含判序与幂等/防御面）。

use core::result::Result;

/// 登记槽数（kernel `DEMAND_ROOT_SLOTS` 单源镜像；16 槽定长线性扫——探针
/// 未命中 16 次原子载入 ≈ 数十周期，fault 快路径可受）。
pub const DEMAND_ROOT_SLOTS: usize = 16;

/// 登记失败形（kernel `DemandRootError` 纯投影）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemandRootError {
    /// 16 槽满——无空位（拒登记 fail loud，不静默丢）。
    Full,
    /// asid 已登记且 root 指针不同——单 root 不变量破（异 Vmar 同 asid =
    /// 装配面 bug，防御拒）。
    OwnerConflict,
}

/// 单槽纯快照（asid=0 = 空槽哨兵；ptr=0 = 无 root——注销半态 X+0 同形）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootSlot {
    pub asid: u16,
    pub ptr: usize,
}

/// 空槽哨兵（asid 分配器从 1 起——0 永不合法登记键）。
pub const EMPTY: RootSlot = RootSlot { asid: 0, ptr: 0 };

/// 登记槽表抽象（消费方实现读写两个原语；kernel 侧 = 原子对数组薄壳，
/// host 测试 = `[RootSlot]` 直驱）。
pub trait RootSlots {
    /// 读第 `i` 槽快照。
    fn slot(&self, i: usize) -> RootSlot;
    /// 写第 `i` 槽（登记 = ptr 先 asid 后；注销 = asid 先 ptr 后——由
    /// [`register_step`]/[`deregister_step`] 的调用序钉死，实现只管落格）。
    fn set(&mut self, i: usize, s: RootSlot);
}

/// `[RootSlot; N]` 直驱实现（host 测试面）。
impl<const N: usize> RootSlots for [RootSlot; N] {
    fn slot(&self, i: usize) -> RootSlot {
        self[i]
    }
    fn set(&mut self, i: usize, s: RootSlot) {
        self[i] = s;
    }
}

/// 登记判序（kernel `register_demand_root` 判定段单源）：
///
/// 1. 全表扫既有槽：命中同 asid → 同 ptr 幂等 `Ok(false)`（已登记，零写）/
///    异 ptr `OwnerConflict`；**此查先于空槽扫描**（幂等/冲突判不占空槽）。
/// 2. 无同 asid：首个空槽 first-wins 写入 `Ok(true)`。
/// 3. 无空槽：`Full`。
///
/// 返回 `(true = 本次实写新登记)`；写序 = ptr 先 asid 后（发布序——读者
/// 经 asid 命中即见完整对）。
pub fn register_step<S: RootSlots + ?Sized>(
    slots: &mut S,
    asid: u16,
    root: usize,
) -> Result<bool, DemandRootError> {
    let mut free: Option<usize> = None;
    for i in 0..DEMAND_ROOT_SLOTS {
        let cur = slots.slot(i);
        if cur.asid == asid {
            // 同 asid：同 root 幂等 Ok；异 root = OwnerConflict。
            return if cur.ptr == root {
                Ok(false)
            } else {
                Err(DemandRootError::OwnerConflict)
            };
        }
        if cur.asid == 0 && free.is_none() {
            free = Some(i);
        }
    }
    let Some(i) = free else {
        return Err(DemandRootError::Full);
    };
    // 发布序：ptr 先 asid 后（读者按 asid 命中即见完整对）。
    slots.set(i, RootSlot { asid, ptr: root });
    Ok(true)
}

/// 反查判序（kernel `demand_root` 判定段单源）：asid 0 哨兵直达 `None`；
/// 扫描命中 asid 后 ptr!=0 才 `Some`——**X+0 注销半态按 miss 处理**
/// （hint 面零解引用，锁窗内重查给真值）。
pub fn lookup_step<S: RootSlots + ?Sized>(slots: &S, asid: u16) -> Option<usize> {
    if asid == 0 {
        return None;
    }
    for i in 0..DEMAND_ROOT_SLOTS {
        let cur = slots.slot(i);
        if cur.asid == asid {
            if cur.ptr != 0 {
                return Some(cur.ptr);
            }
            // 锁外读者可见的注销半态（deregister 先清 asid 后清 ptr 的
            // 镜像序）：X+0 按 miss 处理——hint 面零解引用，窗内重查给真值。
        }
    }
    None
}

/// 注销判序（kernel `deregister_demand_root` 判定段单源）：**asid+ptr 双
/// 匹配**才清（asid 复用后旧 root 的注销不得误清新登记）；清序 = asid 先
/// （=EMPTY 态）ptr 后——由单次 `set(EMPTY)` 原子对落格承载。返回
/// `true` = 清了一槽。
pub fn deregister_step<S: RootSlots + ?Sized>(slots: &mut S, asid: u16, root: usize) -> bool {
    for i in 0..DEMAND_ROOT_SLOTS {
        let cur = slots.slot(i);
        if cur.asid == asid && cur.ptr == root {
            // 卫生序：asid 先清后 ptr（读者要么见 asid 消失即止步，要么见
            // X+0 半态按 miss 处理，永不见「新 asid + 旧 ptr」）。
            slots.set(i, EMPTY);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
