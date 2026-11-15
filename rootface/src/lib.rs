//! berry-rootface —— rootfs 装载门纯面（W3-Q85）。
//!
//! **动机**：`kernel/src/main.rs` 的 rootfs 装载链有四组**纯判定门**此前只
//! 活在 kernel crate（host 测不可达），而它们是 boot 期「无 rootfs 降级
//! 存活」纪律（§A136 族 no-panic）的语义单源——错一门 = 要么放过违约
//! 产链形态（下游 cpio walk 才炸，语义错层），要么误拒合法镜像（显示面
//! 静默降级同族）：
//!
//! 1. **fwcfg 多卷发现门**（`fwcfg_rootfs_multivol` 发现段）：单卷域
//!    `(0, MAX_VOL]`（空卷拒 = 产链契约）、Σ `checked_add` 溢出守卫 +
//!    `TOTAL_MAX` 帽、连续前缀止（`rootfs0` 缺席 = 非多卷形态直接回落）；
//! 2. **v0 init_blob 域门**：`pa/len` 零拒、越 `MAX_VOL` 帽拒、
//!    `[RAM_BASE, RAM_TOP)` 窗检含 `checked_add` 回卷拒；
//! 3. **isegs Σ 帽**：逐段 `checked_add` 溢出守卫 + `TOTAL_MAX` 帽；
//! 4. **拼装对账门**：实拼字节 `off != total` 拒（拼装错位不裸奔半截
//!    blob）。
//!
//! 纪律（dmface/bootpface 先例同族）：判定律按**纯投影**（size/pa/len 值
//! 入参，Vmo/fw_cfg/handoff 物理臂留壳）抽成纯函数 + 表驱动测试钉板；
//! kernel 侧读臂/装配臂不动。
//!
//! **留壳**（物理臂，不外迁）：`fwcfg::rootfs_volume` 目录行走、
//! `SeqRead` DMA 读、`Vmo::create/commit/write`、`handoff::init_segs`
//! 探测、`phys_to_virt` 直映借用。

#![cfg_attr(not(test), no_std)]

// ---- 常量（kernel/main.rs 单源迁此，kernel 侧改 re-export 消费）----

/// fw_cfg 多卷单卷帽（产链契约：mkcpio `--split 32`——32MiB/卷）。
pub const MAX_VOL: usize = 32 << 20;

/// fw_cfg 多卷总量帽（A1-2 帽：Σ ≤ 128MiB；isegs 路同口径）。
pub const TOTAL_MAX: usize = 128 << 20;

// ---- 1. fwcfg 多卷发现门 ----

/// 单卷发现门：`Some(size)` 收卷 / `None` 止（缺席或违约拒）。
///
/// 判序（与 kernel `fwcfg_rootfs_multivol` 发现段逐字同序）：
/// 1. 空卷拒（`0` = 产链违约——mkcpio 产卷恒非空）；
/// 2. 单卷帽（`> MAX_VOL` 拒）；
/// 3. 累加守卫（Σ 溢出 = 敌意巨值，整源降级）；
/// 4. 总量帽（Σ > `TOTAL_MAX` 拒）。
pub fn vol_admit(sz: usize, total_so_far: usize) -> VolOutcome {
    if sz == 0 {
        return VolOutcome::Reject;
    }
    if sz > MAX_VOL {
        return VolOutcome::Reject;
    }
    let Some(t) = total_so_far.checked_add(sz) else {
        return VolOutcome::Reject;
    };
    if t > TOTAL_MAX {
        return VolOutcome::Reject;
    }
    VolOutcome::Admit(t)
}

/// fwcfg 多卷发现门三态（kernel 侧 `break`〔前缀止〕/`return None`〔降级〕
/// /`push`〔收卷〕三控制流的纯面镜像）。
#[derive(Debug, PartialEq, Eq)]
pub enum VolOutcome {
    /// 收卷（回传新 Σ——kernel `total = …` 同值）。
    Admit(usize),
    /// 整源降级 `None`（违约：空卷/越帽/溢出）。
    Reject,
}

// ---- 2. v0 init_blob 域门 ----

/// v0 单 blob 域门：`pa/len` 值合法且整段落 `[ram_base, ram_top)` 窗。
///
/// 判序（与 kernel v0 段逐字同序）：零拒（`pa==0 || len==0`）→ 越帽拒
/// （`len > MAX_VOL`）→ 窗下界（`pa < ram_base` 拒）→ 回卷拒
/// （`pa + len` `checked_add` 溢出或越 `ram_top` 拒）。
pub fn blob_span_ok(pa: u64, len: u64, ram_base: u64, ram_top: u64) -> bool {
    if pa == 0 || len == 0 {
        return false;
    }
    if len > MAX_VOL as u64 {
        return false;
    }
    if pa < ram_base {
        return false;
    }
    match pa.checked_add(len) {
        Some(end) if end <= ram_top => true,
        _ => false,
    }
}

// ---- 3. isegs Σ 帽 ----

/// isegs 逐段累加帽：`Some(new_total)` 续 / `None` 降级（溢出或越
/// `TOTAL_MAX`——与 fwcfg 多卷路同口径，A1-1 帽分家执法点）。
pub fn segs_total_step(total: usize, seg_len: usize) -> Option<usize> {
    let t = total.checked_add(seg_len)?;
    if t > TOTAL_MAX {
        return None;
    }
    Some(t)
}

// ---- 4. 拼装对账门 ----

/// 拼装对账：实拼 `off` == Σ 卷 `total`（SeqRead 项尾守卫的调用侧执法
//——差值即拼装错位，整源降级不裸奔半截 blob）。
pub fn assembly_matches(off: usize, total: usize) -> bool {
    off == total
}

#[cfg(test)]
mod tests;
