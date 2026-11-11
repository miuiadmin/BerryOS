//! berry-winplan —— X-5 demand 装载窗规划纯函数 crate。
//!
//! [`plan_wins`] 自 berry-exec 的 `demand_plan_wins` 外迁（行为逐字等价，
//! 唯一差异：根幅 `cap` 参数化——原处内嵌 `berry_vmo_get_size` FFI，是
//! berry-exec host 不可测的根因）。三色窗几何与 congruent 门 (a')-(e')
//! 的语义全文见 [`WinKind`]；消费方 berry-exec 的 spawn demand 分支经
//! path 依赖单源引回，「不长第二套」。
//!
//! 纪律：零 FFI / 零 unsafe / 零分配（输入只借用，输出写调用方切片）；
//! host 测试全盖（tests/plan.rs）。

#![no_std]

use berry_loader::{ElfHead, PAGE_SIZE};

/// X-5 映像窗数帽（berry-exec 侧规划/回收表容量；怪形 ELF 超 → None →
/// eager 兜底不硬塞）。现役 musl 族 4 PT_LOAD 实测 8 窗（4 file + 3
/// hybrid + 1 零尾）——16 留一倍余量。
pub const WIN_MAX: usize = 16;

/// X-5 窗计划条目（[`plan_wins`] 产物）：映像幅内一段连续页域的装载法。
/// 三色窗升序铺满 [base, base+len)——eager 全幅单 VMO 语义的 demand
/// 分解（file 字节按窗恒等供给、其余页恒零）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WinPlan {
    /// 窗基（绝对 VA；页对齐）。
    pub start: u64,
    /// 窗幅（页倍数；hybrid 恒 1 页）。
    pub len: u64,
    pub kind: WinKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WinKind {
    /// file 域 demand 窗：0x23(dv, rootfs, delta) 恒等供给——窗内页偏 p
    /// 的源偏 = p + delta（**本窗自己的**常数：delta_i = data_off +
    /// k_i + wstart，k_i = 该段 file_off − vaddr——X-3c 单 k 全局门在此
    /// 逐段化，musl 族 {−0x20{0,1,2,3}000} 多 k 布局由此过门）。
    File { delta: i64 },
    /// 零域 demand 窗：0x23(dv, 0, 0) 内核零页臂——段间 gap / 纯 bss
    /// 段 / 末尾域（eager 全幅 VMO 零语义的 demand 等价面）。
    Zero,
    /// hybrid 尾页（eager 预填 1 页）：段尾 filesz 非页对齐时的混合页
    /// ——前 `flen` 字节 = 根档 `soff` 起的段尾文件字节、余零。该页的
    /// 文件侧背后（根档坐标）已是**下一段首字节**，demand 整页供给会
    /// 把邻段字节灌进本段 bss 尾（X-3c 检查 (f) 在 musl 族恒拒的根因
    /// ——bss 尾页与邻段文件字节同页共存）——eager 预填是零污染解
    /// （内核 0x23 无部分页供给面，v1 ABI 冻结不扩；X-5 起 (f) 整条
    /// 退役：混合页不再走 demand 供给面）。
    Hybrid { soff: u64, flen: u32 },
}

/// X-5 多窗 congruent 门（X-3c 单 k 门对现役 musl 多段族的推广；纯几何
/// + 根幅验，**零根档内容读**——(f) live-bss 尾零验退役，见
/// [`WinKind::Hybrid`]）。各门任一不满足 → None → eager 回退（非
/// congruent ELF 永有 eager 兜底）：
/// (a') 段序 = vaddr 升序且窗页不相交（含「本段文件窗扎进前段 hybrid
///      页」形；berry-ld musl 族 3-4 段恒升序页邻接，X-4 宿主清点 24
///      bins 全过；纯 bss 段 filesz==0 不参与——其页域由零窗覆盖）；
/// (b)  data_off 页对齐（mkcpio X-3c 垫页几何产链兑现，此处只验不垫）；
/// (c') 每段 k_i = file_off − vaddr 页倍数（0x23 `delta % PAGE` EINVAL
///      门的窗级化：k_i 页倍 + data_off/wstart 页对齐 ⇒ delta_i 自动
///      页对齐，只需再拦 delta_i < 0 下溢面）；
/// (d') 每窗顶 ≤ base+len（X-3c (d) 的逐窗版——filesz 谎报在几何层拦）；
/// (e') 每窗源域在 rootfs 幅内：delta_i + wlen ≤ cap / hybrid 的
///      soff+flen ≤ cap（越界供给 = faulter 永久重试 livelock，资格层
///      拦死——ELF 头谎报 filesz 的输入面）。
/// 窗数 > [`WIN_MAX`] 或全段 filesz==0（无 file 面，X-3c `k?` 同款保守
/// 拒）→ None。通过 = `Some(nw)`：out[..nw] 升序**完备**铺满
/// [base, base+len)（walk 保证——reap 逐窗 unmap 即全幅回收）。
pub fn plan_wins(
    h: &ElfHead,
    base: u64,
    len: u64,
    data_off: u64,
    cap: u64,
    out: &mut [WinPlan],
) -> Option<usize> {
    // (b)：data_off 页对齐。
    if data_off % PAGE_SIZE as u64 != 0 {
        return None;
    }
    let top = base.checked_add(len)?;
    let mut nw = 0usize;
    let mut nfile = 0usize;
    let mut cursor = base; // 已规划上沿（零覆盖游标）
    for seg in h.segments() {
        if seg.filesz == 0 {
            continue; // 纯 bss 段：页域由零窗/gap 收口
        }
        let wlo = berry_loader::page_floor(seg.vaddr);
        let fend = seg.vaddr.checked_add(seg.filesz)?;
        let ftop = berry_loader::page_floor(fend);
        let whi = berry_loader::page_ceil(fend);
        // (a')+(d')：升序不重叠 + 窗顶在映像幅内。
        if wlo < cursor || whi > top {
            return None;
        }
        // (c')：k_i 页倍数（i64 域安全：file_off/vaddr 均 < USER_TOP=2^39）。
        let k = (seg.file_off as i64) - (seg.vaddr as i64);
        if k % PAGE_SIZE as i64 != 0 {
            return None;
        }
        // gap 零窗 [cursor, wlo)（段前空档——eager 零语义等价）。
        if wlo > cursor {
            if nw == out.len() {
                return None;
            }
            out[nw] = WinPlan {
                start: cursor,
                len: wlo - cursor,
                kind: WinKind::Zero,
            };
            nw += 1;
        }
        // file 窗 [wlo, ftop)：delta_i = data_off + k_i + wlo（推导：窗内
        // va 的绝对源偏 = data_off + file_off + (va − vaddr) = data_off +
        // k_i + va = delta_i + (va − wlo)，toff = va − wlo）。
        if ftop > wlo {
            let delta = data_off as i64 + k + wlo as i64;
            // (e')：下溢 + 根幅（checked 链防谎报 filesz 溢出）。
            if delta < 0 || (delta as u64).checked_add(ftop - wlo)? > cap {
                return None;
            }
            if nw == out.len() {
                return None;
            }
            out[nw] = WinPlan {
                start: wlo,
                len: ftop - wlo,
                kind: WinKind::File { delta },
            };
            nw += 1;
            nfile += 1;
        }
        // hybrid 尾页 [ftop, whi)：段尾非页对齐时整页 eager 预填。
        if whi > ftop {
            let soff = (data_off as i64 + k + ftop as i64) as u64;
            let flen = (fend - ftop) as u32;
            if soff.checked_add(flen as u64)? > cap {
                return None; // (e') hybrid 面
            }
            if nw == out.len() {
                return None;
            }
            out[nw] = WinPlan {
                start: ftop,
                len: whi - ftop,
                kind: WinKind::Hybrid { soff, flen },
            };
            nw += 1;
        }
        cursor = whi;
    }
    if nfile == 0 {
        return None; // 无 file 面（X-3c `k?` 同款保守拒——全零映像无 demand 意义）
    }
    // 末零窗 [cursor, top)：末段 bss/尾域收口（walk 完备性保证）。
    if cursor < top {
        if nw == out.len() {
            return None;
        }
        out[nw] = WinPlan {
            start: cursor,
            len: top - cursor,
            kind: WinKind::Zero,
        };
        nw += 1;
    }
    Some(nw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win_max_pinned() {
        assert_eq!(WIN_MAX, 16);
    }
}
