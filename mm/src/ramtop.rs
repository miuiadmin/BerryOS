//! EFI RAM 顶修正与帧源选择纯面（W3-Q35）。
//!
//! **动机**：`kernel/src/mm.rs` 的 `efi_ram_top` 折叠——handoff `dram_top`
//! 可能仍是 128MiB 模板，用 EFI 图里 Conventional 段最大 end 修正（cap 到
//! HAL `RAM_TOP`）——此前只活在 kernel crate 里（只 aarch64-unknown-none
//! 可编），其判定律 host 测不可达：
//! - Conventional 最大 end 的 max 折叠（非 Conventional 段不参与——
//!   firmware 拥有的段不能推高可分配顶）；
//! - 每段 `checked_mul/checked_add` 双回卷拒 + `min(RAM_TOP)` cap；
//! - 终值 `clamp(RAM_BASE, RAM_TOP)` 窗口卫。
//! 本模块把折叠律按值域抽成**纯投影**：BerryHandoff 不进本 crate（kernel
//! 结构），入参 = 纯 `RamTopInputs`（dram_top + 内存图描述符切片——W3-Q24
//! dpgface「Task 判据字段以纯结构投影入参」同款）；kernel 侧走图循环
//! （raw 指针 `desc_at` unsafe 留壳）把描述符投给本函数折叠。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 mm.rs 行为）：
//! - [`fold_conventional_end`]：单描述符 → Conventional 段 end 候选
//!   （`checked_mul(4096)` + `checked_add` 双回卷 None；非 Conventional
//!   None 不参与折叠）；
//! - [`efi_ram_top`]：dram_top 基线 + 描述符集 max 折叠 + `min(ram_top)`
//!   cap + 终值 `clamp(ram_base, ram_top)` 窗口卫；
//! - [`frame_window`]：帧源选择——`Some(handoff)` 路取修正后 ram_top、
//!   `None`（QEMU `-kernel` 直加载路）取 `ram_top_default`
//!   （`RAM_TOP_DEFAULT` 128MiB 常量的参数化镜像）。
//!
//! 与冻结面的关系：`map_ok` 整图形状门在 `berry_mm::memmap`（在册，本
//! 模块复用其 `EFI_CONVENTIONAL` 常量单源）；raw 指针走图（`desc_at`
//! unsafe 臂）留 kernel 壳——本面只钉「给定图形状，顶取多少」的数学。

use crate::memmap::EFI_CONVENTIONAL;

/// 单描述符的页大小视图（与 `memmap::PAGE_U64` 同值；不 re-export 私有常量，
/// 本模块自持镜像并 const assert 对拍——两处 4096 漂移编译期炸响）。
const PAGE_U64: u64 = 4096;
const _: () = assert!(PAGE_U64 == 4096);

/// 纯投影入参：一个内存图描述符的折叠相关字段（kind/physical_start/
/// number_of_pages——`EfiMemdesc` 的判据子集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescProj {
    pub kind: u32,
    pub physical_start: u64,
    pub number_of_pages: u64,
}

impl DescProj {
    /// 从 `berry_mm::memmap::EfiMemdesc`（镜像 40B 布局）投影。
    #[inline]
    pub fn from_memdesc(d: &crate::memmap::EfiMemdesc) -> Self {
        Self {
            kind: d.kind,
            physical_start: d.physical_start,
            number_of_pages: d.number_of_pages,
        }
    }

    /// kernel 走图臂的逐描述符迭代投影（`(0..n).map(...)` 直喂
    /// [`efi_ram_top`]——raw `desc_at` 解引用留在 kernel 闭包里）。
    #[inline]
    pub fn project_raw(
        map_ok: Option<usize>,
        mut desc_at: impl FnMut(usize) -> crate::memmap::EfiMemdesc,
    ) -> impl Iterator<Item = DescProj> {
        let n = map_ok.unwrap_or(0);
        (0..n).map(move |i| DescProj::from_memdesc(&desc_at(i)))
    }
}

/// 单描述符 → Conventional end 候选。
///
/// - Conventional 段：`start + npages*4096` 的 checked 链——任一回卷
///   `None`（敌意/损坏值不得折叠成巨顶）；
/// - 非 Conventional 段：`None`（firmware/保留段不推高可分配顶——只有
///   真空闲段的最大 end 才是「物理 RAM 实际到这」的证据）。
#[inline]
pub fn fold_conventional_end(d: &DescProj) -> Option<u64> {
    if d.kind != EFI_CONVENTIONAL {
        return None;
    }
    let len = d.number_of_pages.checked_mul(PAGE_U64)?;
    d.physical_start.checked_add(len)
}

/// EFI RAM 顶修正折叠（kernel `efi_ram_top` 单源）。
///
/// 判序（与 kernel 侧逐字对齐）：
/// 1. 基线 = `dram_top`（handoff 模板值）；
/// 2. 逐描述符：Conventional end 候选（checked 双拒）→ `min(ram_top)`
///    cap 后 max 折叠；
/// 3. 终值 `clamp(ram_base, ram_top)`——防御 handoff 越窗值（几何哨兵
///    之外的第二道卫；assert 在 kernel init 前置，这里 clamp 保返回值
///    恒在窗内）。
///
/// 入参取迭代器（kernel 侧 no_std 无堆，`(0..n).map(projection)` 直喂——
/// 走图 raw 指针臂留 kernel，折叠数学全在本函数）。
pub fn efi_ram_top<I: IntoIterator<Item = DescProj>>(
    dram_top: u64,
    descs: I,
    ram_base: u64,
    ram_top: u64,
) -> u64 {
    let mut top = dram_top;
    for d in descs {
        if let Some(end) = fold_conventional_end(&d) {
            let end = end.min(ram_top);
            top = top.max(end);
        }
    }
    top.clamp(ram_base, ram_top)
}


/// 帧源选择（kernel `mm.rs::init` 的 `ram_top` 装配序单源）。
///
/// - `Some(top)`：EFI 路（handoff 命中且 `efi_ram_top` 已折叠）——用修正顶；
/// - `None`：QEMU `-kernel` 直加载路（无 handoff）——退 `ram_top_default`。
#[inline]
pub fn frame_window(top: Option<u64>, ram_top_default: u64) -> u64 {
    top.unwrap_or(ram_top_default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memmap::EfiMemdesc;

    // HAL 真值锚（QEMU virt 256MiB 窗 + 128MiB 模板顶——与
    // berry_hal_aarch64 常量同值；host 面不链 HAL，值域镜像钉板）。
    const RAM_BASE: u64 = 0x4000_0000;
    const RAM_TOP: u64 = 0x5000_0000;
    const RAM_TOP_DEFAULT: u64 = 0x4800_0000;

    fn proj(kind: u32, start: u64, npages: u64) -> DescProj {
        DescProj {
            kind,
            physical_start: start,
            number_of_pages: npages,
        }
    }

    fn memdesc(kind: u32, start: u64, npages: u64) -> EfiMemdesc {
        EfiMemdesc {
            kind,
            _pad: 0,
            physical_start: start,
            virtual_start: 0,
            number_of_pages: npages,
            attribute: 0,
        }
    }

    // ---- T1. fold_conventional_end：类型分流 + checked 双拒 ----

    #[test]
    fn t1_fold_by_kind_and_checked() {
        // Conventional 段：end = start + npages*4096。
        assert_eq!(fold_conventional_end(&proj(7, 0x4000_0000, 16)), Some(0x4001_0000));
        // 非 Conventional（firmware/loader/保留）不参与——None。
        for kind in [0u32, 2, 4, 5, 6, 10, 11] {
            assert_eq!(
                fold_conventional_end(&proj(kind, 0x4700_0000, 1024)),
                None,
                "kind={kind} 非 Conventional 不折叠"
            );
        }
        // checked 双拒：npages*4096 回卷 / start+len 回卷 → None（不折叠巨顶）。
        assert_eq!(fold_conventional_end(&proj(7, 0, u64::MAX / 4096 + 1)), None);
        assert_eq!(fold_conventional_end(&proj(7, u64::MAX - 4096, 2)), None);
        // 零页 Conventional：end = start（合法零长段，折叠值即 start）。
        assert_eq!(fold_conventional_end(&proj(7, 0x4200_0000, 0)), Some(0x4200_0000));
    }

    // ---- T2. efi_ram_top：max 折叠 + cap + clamp ----

    #[test]
    fn t2_template_top_upgraded_by_conventional_max() {
        // 128MiB 模板 + 图里 Conventional 到 0x4F80_0000 → 顶修正到 248MiB。
        let descs = [
            proj(4, 0x4000_0000, 0x600), // LoaderData 6MiB——不参与
            proj(7, 0x4060_0000, 0xF200), // Conventional 到 0x4F80_0000
            proj(7, 0x4100_0000, 0x100),  // 小 Conventional——max 不退
        ];
        let top = efi_ram_top(RAM_TOP_DEFAULT, descs, RAM_BASE, RAM_TOP);
        assert_eq!(top, 0x4F80_0000);
        // 模板已高于图（少见但合法）：max 保模板不退。
        let low = [proj(7, RAM_BASE, 0x1000)];
        assert_eq!(efi_ram_top(RAM_TOP_DEFAULT, low, RAM_BASE, RAM_TOP), RAM_TOP_DEFAULT);
    }

    #[test]
    fn t2_non_conventional_never_raises_top() {
        // firmware 段 end 越模板——非 Conventional 不推高（只认真空闲证据）。
        let descs = [proj(5, 0x4C00_0000, 0x3FFF)];
        assert_eq!(efi_ram_top(RAM_TOP_DEFAULT, descs, RAM_BASE, RAM_TOP), RAM_TOP_DEFAULT);
        // 空图：模板原样。
        assert_eq!(efi_ram_top(RAM_TOP_DEFAULT, [], RAM_BASE, RAM_TOP), RAM_TOP_DEFAULT);
    }

    #[test]
    fn t2_cap_and_clamp() {
        // Conventional end 越窗顶（敌意图）：min(ram_top) cap 后折叠。
        let descs = [proj(7, 0x4F00_0000, 0x1_0000)];
        assert_eq!(efi_ram_top(RAM_TOP_DEFAULT, descs, RAM_BASE, RAM_TOP), RAM_TOP);
        // handoff dram_top 越窗（几何哨兵前的防御面）：clamp 回窗内。
        assert_eq!(efi_ram_top(u64::MAX, [], RAM_BASE, RAM_TOP), RAM_TOP);
        assert_eq!(efi_ram_top(0, [], RAM_BASE, RAM_TOP), RAM_BASE);
        // 回卷候选被 checked 拒：不折叠、模板保持。
        let evil = [proj(7, 0, u64::MAX / 4096 + 1)];
        assert_eq!(efi_ram_top(RAM_TOP_DEFAULT, evil, RAM_BASE, RAM_TOP), RAM_TOP_DEFAULT);
    }

    // ---- T3. 全图形状：map_ok 门 + 折叠的组合合同 ----

    #[test]
    fn t3_map_gate_then_fold() {
        // 组合合同：整图先过 map_ok 形状门（memmap 在册面），过的图才折叠。
        // 合成 3 描述符图（3×40B，窗内 8 对齐）。
        let descs = [
            memdesc(4, 0x4000_0000, 0x600),
            memdesc(7, 0x4060_0000, 0xF200),
            memdesc(7, 0x4100_0000, 0x100),
        ];
        let n = crate::memmap::map_ok(RAM_BASE + 0x1000, 120, 40, 1, RAM_BASE, RAM_TOP);
        assert_eq!(n, Some(3));
        let projs: Vec<DescProj> = descs.iter().map(DescProj::from_memdesc).collect();
        // 投影往返：from_memdesc 字段逐位对齐。
        assert_eq!(projs[1], proj(7, 0x4060_0000, 0xF200));
        assert_eq!(
            efi_ram_top(RAM_TOP_DEFAULT, projs, RAM_BASE, RAM_TOP),
            0x4F80_0000
        );
    }

    // ---- T4. frame_window：帧源两路 ----

    #[test]
    fn t4_frame_window_two_paths() {
        // EFI 路：修正顶直达。
        assert_eq!(frame_window(Some(0x4F80_0000), RAM_TOP_DEFAULT), 0x4F80_0000);
        // QEMU -kernel 直加载路：无 handoff → 模板顶。
        assert_eq!(frame_window(None, RAM_TOP_DEFAULT), RAM_TOP_DEFAULT);
        // 修正顶低于模板（不退化合同：取 Some 本身，不与模板再 max）。
        assert_eq!(frame_window(Some(0x4200_0000), RAM_TOP_DEFAULT), 0x4200_0000);
    }

    // ---- T5. HAL 锚钉板（漂移即红——改 HAL 常量须同步本测试锚）----

    #[test]
    fn t5_hal_anchor_values() {
        // QEMU virt 256MiB 窗 + 128MiB 模板（arch/aarch64/src/lib.rs 同值）。
        assert_eq!(RAM_BASE, 0x4000_0000);
        assert_eq!(RAM_TOP, 0x5000_0000);
        assert_eq!(RAM_TOP_DEFAULT, 0x4800_0000);
        // EFI_CONVENTIONAL 单源对拍（本模块经 memmap 引用，值域 7）。
        assert_eq!(EFI_CONVENTIONAL, 7);
    }

    // ---- T6. project_raw：kernel 走图臂的迭代投影合同 ----

    #[test]
    fn t6_project_raw_iterator_contract() {
        // 合成图经 memdesc 数组承载（模拟 kernel desc_at 的按索引取件）。
        let table = [
            memdesc(4, 0x4000_0000, 0x600),
            memdesc(7, 0x4060_0000, 0xF200),
            memdesc(7, 0x4100_0000, 0x100),
        ];
        let projs: Vec<DescProj> = DescProj::project_raw(Some(3), |i| {
            let d = &table[i];
            EfiMemdesc { kind: d.kind, _pad: 0, physical_start: d.physical_start, virtual_start: 0, number_of_pages: d.number_of_pages, attribute: 0 }
        }).collect();
        // 逐位对齐 from_memdesc。
        assert_eq!(projs.len(), 3);
        assert_eq!(projs[0], proj(4, 0x4000_0000, 0x600));
        assert_eq!(projs[2], proj(7, 0x4100_0000, 0x100));
        // None（坏图降级路）→ 空迭代：折叠退模板。
        assert_eq!(DescProj::project_raw(None, |i| {
            let d = &table[i];
            EfiMemdesc { kind: d.kind, _pad: 0, physical_start: d.physical_start, virtual_start: 0, number_of_pages: d.number_of_pages, attribute: 0 }
        }).count(), 0);
        // 端到端：kernel 同款装配（map_ok 门 → project_raw → efi_ram_top）。
        let top = efi_ram_top(
            RAM_TOP_DEFAULT,
            DescProj::project_raw(Some(3), |i| {
                let d = &table[i];
                EfiMemdesc { kind: d.kind, _pad: 0, physical_start: d.physical_start, virtual_start: 0, number_of_pages: d.number_of_pages, attribute: 0 }
            }),
            RAM_BASE,
            RAM_TOP,
        );
        assert_eq!(top, 0x4F80_0000);
        let degraded = efi_ram_top(
            RAM_TOP_DEFAULT,
            DescProj::project_raw(None, |i| {
                let d = &table[i];
                EfiMemdesc { kind: d.kind, _pad: 0, physical_start: d.physical_start, virtual_start: 0, number_of_pages: d.number_of_pages, attribute: 0 }
            }),
            RAM_BASE,
            RAM_TOP,
        );
        assert_eq!(degraded, RAM_TOP_DEFAULT, "坏图降级退模板");
    }
}
