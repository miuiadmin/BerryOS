//! vprobe —— P6 A-2 virtio-mmio 探测（用户态驱动地基第一刀）。
//!
//! stdhello/fsmgr 同链（musl target + berry-libc shim）。MMIO 窗口来自
//! boot 授能：`Vmo::create_mmio(VIRTIO_MMIO0_PA, 0x4000)`（QEMU `virt`
//! 板 virtio-mmio 整窗 32 槽 × 0x200）进 svcmgr 表第 4 插 0x0001_0004
//! （R|W|MAP），svcmgr 经 `SpawnSpec::mmio_vmo` 映到 [`VIRTIO_MMIO_VA`]
//! （本 crate 经 berry-exec 单源取值）——**设备访问全在用户态**，内核只
//! 给了窗口（ADR「驱动用户态为主」的首个真实消费面）。
//!
//! MMIO 读写经 berry-hal（[`Mmio`] trait + [`VirtioMmio`] 实现——HAL
//! 落地后的**首个真消费者**，2026-11-18 接线）：volatile 与窗界检收敛
//! 在 HAL 实现内部，本文件零 MMIO unsafe；原先的逐字 `read_volatile`
//! 与本地常量镜像（VIRTIO_MAGIC/VERSION/DEV_BLOCK/DEV_NET/DEV_INPUT/
//! DEV_GPU 六件）全退役——偏移与鉴定值改引 `berry_virtio::mmio` 单源
//! （vblk/gui-inputd 消费形态同款「引常量不复制」）。
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
//!   A-7 起合法同席（`-device virtio-net-device`，vnet 自认领）；did=18
//!   （input）GUI-4 段② 起合法同席（`-device virtio-tablet-device`，
//!   gui-inputd 按类扫槽自认领——A-7 net 同款演化）；did=16（gpu）B1 起
//!   合法同席（`-device virtio-gpu-device`，guifbd vgpu 后端自认领——
//!   S1 实录 §6-1 的白名单前置项）；其它 did = 挂载面漂移，loud trap
//!   （未知设备类无消费者，静默漏检更糟）
//! - VendorID@+0x00C 只打印不断言（0x554D4551 "QEMU" 是仿真器特有值——
//!   真机/其它仿真器合法取值不同，断言它 = 把 QEMU 特例焊进标记）
//!
//! 成功打 `VIRTIO ok did=… ven=… slot=N`（**单消息原子标记**——短于
//! 192B 分片界，无撕行面；经 fd1→console）后 main 返回 = exit(0)（libc
//! exit 族 EOF 先行 + 0x33 真退出），svcmgr reap（`EXIT ok` 同款回收链）。
//! 不符 → 诊断行 + trap 只死自己（`VIRTIO ok` 缺失 = smoke FAIL 兜底）。

use berry_exec::{VIRTIO_MMIO_SLOTS, VIRTIO_MMIO_STRIDE, VIRTIO_MMIO_VA};
use berry_hal::{Mmio, VirtioMmio};
use berry_virtio::mmio;

// 链接种子（stdhello/fsmgr 同款）：rlib/LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    // 授窗几何单源（berry-exec）：32 槽 × 0x200。槽基址在 usize 域算术
    // 后转指针（aarch64 usize=u64 无损，as 转换安全）；窗口经 spawn 映射
    // （Device-nGnRnE + SH_NONE——设备访问不可缓存不可推测）。volatile
    // 读写与窗界检（4 对齐 + off+4 ≤ 窗长）全收敛在 berry-hal 的
    // VirtioMmio 实现内部——经 [`Mmio`] trait 契约面调用，纯安全。只读
    // ID 头四字（0x000–0x00F 全 RO、无副作用——不碰队列/中断类寄存器）。
    let mut block: Option<(usize, u32, u32)> = None; // (slot, did, ven)——ver 环内已验
    let mut n_block = 0usize;
    let mut n_occupied = 0usize;
    for slot in 0..VIRTIO_MMIO_SLOTS {
        let Ok(win) = VirtioMmio::new(
            (VIRTIO_MMIO_VA as usize + slot * VIRTIO_MMIO_STRIDE) as *mut u8,
            VIRTIO_MMIO_STRIDE,
        ) else {
            // null/零长窗拒（VA+slot*stride 恒非 0、stride=0x200 常量——
            // 现几何下不可达；防窗几何面未来漂移的 loud 兜底）。
            trap();
        };
        let magic = win.read32(mmio::MAGIC);
        let version = win.read32(mmio::VERSION);
        let device_id = win.read32(mmio::DEVICE_ID);
        let vendor_id = win.read32(mmio::VENDOR_ID);
        if magic != mmio::MAGIC_VAL {
            println!(
                "vprobe: slot {} bad magic={:#x}（窗口几何错？）",
                slot, magic
            );
            trap(); // 全窗 [VA, VA+0x4000) 已映射——读得到却非规范签名 = loud
        }
        if device_id != 0 {
            n_occupied += 1;
            if version != mmio::VERSION_MODERN {
                println!("vprobe: slot {} ver={}（占用槽须 2）", slot, version);
                trap(); // legacy 模式（缺 -global force-legacy=false）——占用槽环内验
            }
            if device_id == mmio::DEVICE_ID_BLOCK {
                n_block += 1;
                block = Some((slot, device_id, vendor_id));
            } else if device_id == mmio::DEVICE_ID_NETWORK {
                // A-7 起合法同席（vnet 扫描自认领）——不算 block 槽。
            } else if device_id == mmio::DEVICE_ID_INPUT {
                // GUI-4 段② 起合法同席（gui-inputd 按类扫槽自认领）——
                // net 同款：健康面由消费者自己的标记（INPUT ok）承接。
            } else if device_id == mmio::DEVICE_ID_GPU {
                // B1 起合法同席（guifbd vgpu 后端按类扫槽自认领）——健康面
                // 由 guifbd 自己的标记（`FB ok vgpu`）承接；did 数值单源
                // = berry-virtio mmio::DEVICE_ID_GPU（其身再单源
                // berry-gpuproto〔uapi 两源断言冻结〕）。S1 实录 §2.2：未
                // 白名时挂 gpu 即 `VIRTIO ok` 缺席 = smoke FAIL loud。
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
