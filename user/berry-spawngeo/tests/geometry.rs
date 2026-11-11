//! berry-spawngeo host 单测：三段几何门的边界全景。语义源 = berry-exec
//! `spawn_rootfs_elf` process_create 前置段（外迁时逐字等价——先在原处
//! 确认语义再落测；门序 = 帽 → 栈 → MMIO，原处三段同序）。

use berry_loader::USER_TOP;
use berry_spawngeo::*;

const PG: u64 = 4096;

#[test]
fn gate_order_span_cap_first() {
    // 门序钉板：幅超帽 + 栈 0 页同时坏 → SpanTooBig 先裁（原处 len 检先于
    // 栈检，消费端三类同走 Err 降级——枚举面只忠实记录裁序）。
    assert_eq!(
        spawn_geometry(0x1000, ELF_MAP_MAX + 1, 0, None),
        Err(SpawnGeoErr::SpanTooBig)
    );
    // 恰帽 = 过（真实 std 镜像 ~283KB，16MiB 帽余量面）。
    assert!(spawn_geometry(0x1000, ELF_MAP_MAX, 8, None).is_ok());
}

#[test]
fn stack_end_arithmetic_and_guard_page() {
    // 常态几何：base 0x1000 + len 0x1000 + 保护页 0x1000 + 8 页栈 = 0xB000。
    let g = spawn_geometry(0x1000, 0x1000, 8, None).unwrap();
    assert_eq!(g.stack_end, 0xB000);
    // 保护页在带内：1 页栈的带 = [base, base+len+两页)——映像顶与栈底之间
    // 恒隔一页（d-3c 栈穿保护律的几何面）。
    let g1 = spawn_geometry(0x1000, 0x1000, 1, None).unwrap();
    assert_eq!(g1.stack_end, 0x4000);
}

#[test]
fn stack_zero_pages_rejected() {
    // 0 页 = 拒（护全族调用方：manifest 域检 1..=64 之外的面，app-0 U3）。
    assert_eq!(
        spawn_geometry(0x1000, 0x1000, 0, None),
        Err(SpawnGeoErr::StackOverflowTop)
    );
}

#[test]
fn stack_top_boundary_user_top() {
    // 恰顶 = 过（e == USER_TOP 合法：elf_head 只严格拒 vend > USER_TOP——
    // 贴顶映像本身合法，本门管的只是加栈后越顶）。
    let len = 0x1000;
    let base = 0x1000;
    // stack_end = base+len+PG + n*PG == USER_TOP 的 n：
    let n = (USER_TOP - (base + len + PG)) / PG;
    let g = spawn_geometry(base, len, n, None).unwrap();
    assert_eq!(g.stack_end, USER_TOP);
    // +1 页 → 越顶拒。
    assert_eq!(
        spawn_geometry(base, len, n + 1, None),
        Err(SpawnGeoErr::StackOverflowTop)
    );
}

#[test]
fn stack_checked_chain_overflow() {
    // checked_mul 溢出（巨 stack_pages）：拒——防调用方巨值溢出绕检后走到
    // vmar_map 才 OutOfBounds 自陷。
    assert_eq!(
        spawn_geometry(0x1000, 0x1000, u64::MAX, None),
        Err(SpawnGeoErr::StackOverflowTop)
    );
    // checked_add 溢出：stack_pages 使 (base+len+PG)+s 进位。
    let n = u64::MAX / PG; // n*PG = 0xFFFF...F000，+ 任意正量必进位
    assert_eq!(
        spawn_geometry(0x1000, 0x1000, n, None),
        Err(SpawnGeoErr::StackOverflowTop)
    );
}

#[test]
fn mmio_none_skips_third_gate() {
    // 无窗口（mmio_vmo == None 面）：第三段整跳——平像/std 无驱动窗的
    // 大多数 spawn 形态。
    assert!(spawn_geometry(0x1000, 0x1000, 8, None).is_ok());
}

#[test]
fn mmio_overlap_image_band() {
    // 窗口横跨映像带：PT_LOAD vaddr 落 0x7000_0000 完全合法过 loader 既有
    // 界检——本门在几何层拦（原处会走到 vmar_map 才 Overlap 拒 → 自陷）。
    let base = 0x7000_0000;
    let len = 0x1000;
    assert_eq!(
        spawn_geometry(base, len, 8, Some((0x7000_0000, 0x200))),
        Err(SpawnGeoErr::MmioOverlap)
    );
    // 窗口尾恰触映像基（me == base）：相邻非重叠 = 过。
    assert!(spawn_geometry(0x1000, 0x1000, 8, Some((0x0, 0x1000))).is_ok());
}

#[test]
fn mmio_overlap_guard_and_stack_band() {
    // 保护页/栈带同在 [base, stack_end) 判定域：窗落保护页 = 重叠。
    let base = 0x1000;
    let len = 0x1000;
    let g = spawn_geometry(base, len, 8, None).unwrap(); // stack_end=0xB000
    let _ = g;
    assert_eq!(
        spawn_geometry(base, len, 8, Some((0x2000, 0x1000))), // [0x2000,0x3000) = 保护页带
        Err(SpawnGeoErr::MmioOverlap)
    );
    // 窗落栈带中段 = 重叠。
    assert_eq!(
        spawn_geometry(base, len, 8, Some((0x9000, 0x1000))),
        Err(SpawnGeoErr::MmioOverlap)
    );
    // 窗首恰贴 stack_end（mmio_va == stack_end）：相邻非重叠 = 过。
    assert!(spawn_geometry(base, len, 8, Some((0xB000, 0x1000))).is_ok());
}

#[test]
fn mmio_end_overflow_conservative_reject() {
    // mmio_va+len 进位（checked_add None）：保守拒——幅越界窗口与映像带
    // 的关系不可判，按可重叠处理。
    assert_eq!(
        spawn_geometry(0x1000, 0x1000, 8, Some((u64::MAX, 1))),
        Err(SpawnGeoErr::MmioOverlap)
    );
}

#[test]
fn real_world_forms_pass() {
    // 现役 std 形态：stdhello 基 0x10000、~283KB 映幅、8 页栈 + virtio 窗
    // 0x7000_0000/+32KiB——远隔，过。
    let len = (283 * 1024 + PG - 1) & !(PG - 1); // page_ceil
    assert!(spawn_geometry(0x10000, len, 8, Some((0x7000_0000, 32 * 1024))).is_ok());
    // svcmgr 平像形态：小映像 1 页栈 + fb 窗 0x7100_0000。
    assert!(spawn_geometry(0x1000, 0x2000, 1, Some((0x7100_0000, 0x8000))).is_ok());
    // bsh 大栈形态：32 页 + 无窗。
    assert!(spawn_geometry(0x1000, 0x47000, 32, None).is_ok());
}
