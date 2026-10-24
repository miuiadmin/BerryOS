//! vprobe —— P6 A-2 virtio-mmio 探测（用户态驱动地基第一刀）。
//!
//! stdhello/fsmgr 同链（musl target + berry-libc shim）。MMIO 窗口来自
//! boot 授能：`Vmo::create_mmio(VIRTIO_MMIO0_PA, 0x4000)`（QEMU `virt`
//! 板 virtio-mmio 整窗 32 槽 × 0x200）进 svcmgr 表第 4 插 0x0001_0004
//! （R|W|MAP），svcmgr 经 `SpawnSpec::mmio_vmo` 映到 [`VIRTIO_MMIO_VA`]
//! （本 crate 经 berry-exec 单源取值）——**设备访问全在用户态**，内核只
//! 给了窗口（ADR「驱动用户态为主」的首个真实消费面）。
//!
//! 探测面 = **扫 32 槽**定位设备（QEMU 倒序建总线、首个 `-device` 落最高
//! 槽且落位随 `-device` 顺序漂移——绑死槽号 = 把 QEMU 总线排布焊进标记，
//! 首版绑 transport[0] 即实踩空槽 ver=1/did=0）：每槽读 virtio 1.x 规范
//! 寄存器头四字（LE u32）：
//! - MagicValue@+0x000 须 0x74726976（"virt" LE——空槽也在席应答规范签名，
//!   32 槽全验 = 窗口几何的总检查）
//! - Version@+0x004：占用槽须 2（virtio 1.x 现代版；QEMU 默认 force-legacy
//!   =true 报 1，启动参数 `-global virtio-mmio.force-legacy=false` 显式要
//!   现代版——v1 不收 legacy）
//! - DeviceID@+0x008 **按设备类判读**（A-7 起同窗可挂多 virtio 设备——
//!   占用槽不再「须恰一」而是分类计数）：did=2（block）须**恰一**
//!   （smoke/run-rootfs 挂 `-device virtio-blk-device`）；did=1（net）
//!   A-7 起合法同席（`-device virtio-net-device`，vnet 自认领）；其它
//!   did = 挂载面漂移，loud trap（未知设备类无消费者，静默漏检更糟）
//! - VendorID@+0x00C 只打印不断言（0x554D4551 "QEMU" 是仿真器特有值——
//!   真机/其它仿真器合法取值不同，断言它 = 把 QEMU 特例焊进标记）
//!
//! 成功打 `VIRTIO ok did=… ven=… slot=N`（**单消息原子标记**——短于
//! 192B 分片界，无撕行面；经 fd1→console）后 main 返回 = exit(0)（libc
//! exit 族 EOF 先行 + 0x33 真退出），svcmgr reap（`EXIT ok` 同款回收链）。
//! 不符 → 诊断行 + trap 只死自己（`VIRTIO ok` 缺失 = smoke FAIL 兜底）。

use berry_exec::{VIRTIO_MMIO_SLOTS, VIRTIO_MMIO_STRIDE, VIRTIO_MMIO_VA};

// 链接种子（stdhello/fsmgr 同款）：rlib/LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// virtio-mmio 规范签名（"virt" LE）。
const VIRTIO_MAGIC: u32 = 0x7472_6976;
/// virtio 1.x 版本字（现代版）。
const VIRTIO_VERSION: u32 = 2;
/// DeviceID = 2（block 设备）。
const VIRTIO_DEV_BLOCK: u32 = 2;
/// DeviceID = 1（net 设备——A-7 起合法同席，vnet 自认领）。
const VIRTIO_DEV_NET: u32 = 1;

fn main() {
    // SAFETY: 窗口经 spawn 映射（Device-nGnRnE + SH_NONE——设备访问不可
    // 缓存不可推测）；逐字 read_volatile（MMIO 读不可合并/重排）。u32 对齐
    // （VA 页对齐、槽内偏移 4 的倍数）。只读 ID 头四字（0x000–0x00F 全
    // RO、无副作用——不碰队列/中断类寄存器）。
    let base = VIRTIO_MMIO_VA as *const u32;
    let mut block: Option<(usize, u32, u32)> = None; // (slot, did, ven)——ver 环内已验
    let mut n_block = 0usize;
    let mut n_occupied = 0usize;
    for slot in 0..VIRTIO_MMIO_SLOTS {
        let regs = unsafe { base.add(slot * (VIRTIO_MMIO_STRIDE / 4)) };
        let magic = unsafe { core::ptr::read_volatile(regs) };
        let version = unsafe { core::ptr::read_volatile(regs.add(1)) };
        let device_id = unsafe { core::ptr::read_volatile(regs.add(2)) };
        let vendor_id = unsafe { core::ptr::read_volatile(regs.add(3)) };
        if magic != VIRTIO_MAGIC {
            println!(
                "vprobe: slot {} bad magic={:#x}（窗口几何错？）",
                slot, magic
            );
            trap(); // 全窗 [VA, VA+0x4000) 已映射——读得到却非规范签名 = loud
        }
        if device_id != 0 {
            n_occupied += 1;
            if version != VIRTIO_VERSION {
                println!("vprobe: slot {} ver={}（占用槽须 2）", slot, version);
                trap(); // legacy 模式（缺 -global force-legacy=false）——占用槽环内验
            }
            if device_id == VIRTIO_DEV_BLOCK {
                n_block += 1;
                block = Some((slot, device_id, vendor_id));
            } else if device_id == VIRTIO_DEV_NET {
                // A-7 起合法同席（vnet 扫描自认领）——不算 block 槽。
            } else {
                println!("vprobe: slot {} did={}（未知设备类）", slot, device_id);
                trap();
            }
        }
    }
    let (slot, device_id, vendor_id) = match (n_block, block) {
        // block 恰一：smoke/run-rootfs 挂一个 virtio-blk——多 block = 命令
        // 行面漂移，零 = 设备/盘缺席（传输层在而设备没接上）；net 槽不扰
        // 本判据（合法同席，占用总数只进诊断行）。
        (1, Some(found)) => found,
        _ => {
            println!(
                "vprobe: block slots = {}（占用共 {}，block 须恰 1）",
                n_block, n_occupied
            );
            trap();
        }
    };
    println!(
        "VIRTIO ok did={} ven={:#x} slot={}",
        device_id, vendor_id, slot
    );
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （fsmgr/bsh 同款——链断 loud，`VIRTIO ok` 缺失 = smoke FAIL 兜底）。
#[cold]
fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（0x4000_0000 与用户布局无交集——家族惯例值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}
