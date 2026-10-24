//! berryboot——BerryOS 引导应用（V-0b 刀 4：终段——`mmap: n=<N> ok` 第 8
//! 段 + GetMemoryMap/ExitBootServices + EBS 后直写 PL011 `BERRYBOOT ok` +
//! DAIF/dcache/SCTLR 整备跳 entry；刀 3 段装载、刀 2 引导视图解析、刀 1
//! KERNEL.ELF 整档读入、V-0a 的 SFS 开卷读原样保留）。
//!
//! 形态：`crate-type=staticlib` → rust-lld 静态全链接（berryboot.ld 单 .img
//! @0x1000 + --image-base 0 + --emit-relocs）→ tools/elf2pe.py 转 PE32+
//! （AArch64 EFI application）→ mkfat --put 造 \EFI\BOOT\BOOTAA64.EFI 引导盘
//! → QEMU AAVMF 可移介质默认路径自动拉起。
//!
//! 刀 2 面：横幅三行——骨架行（刀 1）+ `fw:` 固件厂商/修订行（SystemTable
//! 直读，UCS-2 → ASCII 有损）+ `gop:` 模式信息行（BootServices
//! LocateProtocol(GOP) → QueryMode 当前模式——裁决 C：只信息行不画像素）。
//! 刀 3 面：行 4 `fs:` HELLO.TXT 对拍（LoadedImage(image handle) →
//! DeviceHandle → SFS → OpenVolume → 根 Open(READ) → Read → 与内嵌基准
//! 逐字节对拍）。
//! V-0b 刀 1 面：行 5 `kfile:` KERNEL.ELF 整档读入——根句柄 Open →
//! GetInfo(EFI_FILE_INFO) 探 file_size → AllocatePool(EfiLoaderData) 按长
//! 整档缓冲 → 循环 Read 收齐（got == file_size）→ 打段行；缓冲不 FreePool
//! （刀 2/刀 3 就地消费）。
//! V-0b 刀 2 面：行 6 `elf:` 引导视图解析——kfile 缓冲就地借用（零拷贝），
//! berry-loader `boot_parse`（p_paddr 口径——引导链按 paddr 落内存，VMA
//! 高半只是链接视图）出段数 + 入口；DRAM 窗 = QEMU virt `-m 128M`。
//! V-0b 刀 3 面：行 7 `load:` 段装载——`span()` 整窗一次
//! AllocatePages(AllocateAddress) 认领（缝隙一并拿下，防段共享页撞）→
//! 全窗清零（.bss + 缝隙确定性，对齐 QEMU `-kernel` 直加载零 RAM 起步）→
//! 逐段 copy filesz → 回读逐字节对拍（标记 = 状态谓词）→ handoff 页填
//! v0（裁决 A/B：memmap 刀 4 填，gop/init_blob/cmdline 留位零值）。
//! V-0b 刀 4 面：watchdog 归零 + 内存图取回（GetMemoryMap + 扩容重试帽
//! 两轮）→ handoff memmap_* 四字段落值 → EBS 重试协议（**key 有效期 =
//! 取图到 EBS 之间零 boot service 调用**；被拒即 key 过期 → 重取图再
//! 试，帽三轮）→ **EBS 过后固件服务全死**：`mmap: n=<N> ok` 与
//! `BERRYBOOT ok` 均直写 PL011 MMIO（ConOut 已死——标记语义 = 「含 EBS
//! 的全链真过」状态谓词）→ DAIF 全屏蔽 + dcache 清刷（真机面；TCG 无
//! 实缓存 = no-op）+ SCTLR_EL1 清 M/A/C/I（对齐内核 boot asm 为 QEMU
//! `-kernel` 复位态写的入场假设）+
//! ic/tlbi 全失效 → `br entry`，x0 = handoff PA、x1..x3 = 0——内核零改动
//! 收编（x19 抢救 x0 → BRYH 魔数不中 DTB 探测 → fw_cfg 降级路 → 裸相
//! 23 标记）。CurrentEL ≠ EL1 拒跳（AAVMF virt 默认 EL1；EL2 入场语境的
//! sctlr 整备不成立——诚实拒绝优于错态跳转）。
//! V-1c 面：行 5b `init:` INITRD.BIN 整档读入 → AllocatePages
//! (EfiLoaderData) 页钉——**先于终段 GetMemoryMap**（页进图成非
//! Conventional，内核 V-1b pass2 reserve 自动保）→ pa/len 透传
//! load_kernel 填 handoff init_blob_*（@80/@88，V-1c 起消费）+ 终段
//! dcache 刷集加 init blob 窗（CPU 拷贝脏线防线）。盘上缺席打
//! `init: absent` 续行——内核降级 ROOTFS absent，非致命。
//!
//! 纪律：引导链零 panic（panic handler = 死循环）；一切协议调用失败打
//! **状态码诊断行**（机器可读缺席报告——AI 测试友好：`locate fail r=0x…e`
//! = EFI_NOT_FOUND 即「此固件配置无 GOP 实例」），不炸链；横幅首行本身
//! 即「镜像被执行」的阳证。
#![cfg_attr(not(test), no_std)]

use berry_efi::{
    fmt_u32_dec, fmt_u64_hex, utf16_encode, utf16_to_ascii, EfiFileInfo,
    EfiGraphicsOutputModeInfo, EfiGraphicsOutputProtocol, EfiLoadedImageProtocol,
    EfiMemoryDescriptor, EfiSimpleFileSystemProtocol, EfiStatus, EfiSystemTable,
    EFI_ALLOCATE_ADDRESS, EFI_ALLOCATE_ANY_PAGES, EFI_BOOT_SERVICES_SIGNATURE,
    EFI_BUFFER_TOO_SMALL, EFI_FILE_INFO_GUID, EFI_FILE_MODE_READ,
    EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID, EFI_LOADED_IMAGE_PROTOCOL_GUID, EFI_LOADER_DATA,
    EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID, EFI_SUCCESS, EFI_SYSTEM_TABLE_SIGNATURE,
};
use core::ffi::c_void;
use core::mem::{offset_of, size_of};

#[cfg(not(test))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// 刀 3 基准：HELLO.TXT 的期望字节（与仓库 fixture `../hello.txt` 单源
/// 对拍——host 测试 hello_fixture_consensus 钉同步）。
const EXPECTED_HELLO: &[u8] = b"BERRYBOOT HELLO FIXTURE v0a\n";

/// V-0b 刀 2：DRAM 装载窗 `[DRAM_BASE, DRAM_TOP)`——QEMU virt `-m 128M`
/// （run-efi recipe 同源；改 -m 容量须同步此处）。
const DRAM_BASE: u64 = 0x4000_0000;
const DRAM_TOP: u64 = 0x4800_0000;

/// 引导链诊断行用的错误短名（`berry_loader::LoadError` → ASCII，机器可读
/// 缺席报告；无 Display——库本体 no_std，短名面由消费端给）。
fn boot_err_name(e: berry_loader::LoadError) -> &'static str {
    use berry_loader::LoadError;
    match e {
        LoadError::Truncated => "trunc",
        LoadError::BadMagic => "magic",
        LoadError::BadField => "field",
        LoadError::NotSupported => "nsup",
        LoadError::BadSegment => "seg",
    }
}

// ==================== BerryHandoff v0（V-0b 刀 3 冻结） ====================
//
// **双镜像纪律**（同 kernel/src/handoff.rs——两侧逐字段同步 + 同一张 const
// assert 偏移表，任一侧漂移 = 编译期炸响；abi_gen.rs 双面镜像同精神）。
// 本侧 = 生产端（装载器填写），内核侧 = 消费端（刀 4 起 x19 抢救 → 判
// magic）。字段语义见内核侧头注（单一详注，此处不重复）。

/// 魔数：LE u32 读出 = ASCII "BRYH"。
pub const HANDOFF_MAGIC: u32 = 0x4252_5948;
/// 协议版本：v0 = 0（字段集冻结于 V-0b 刀 3，扩字段 = bump version）。
pub const HANDOFF_VERSION: u32 = 0;

/// 交接结构（112B，全 8B 字段 8 对齐；无指针——PA 直递）。
#[repr(C)]
pub struct BerryHandoff {
    pub magic: u32,               // @0
    pub version: u32,             // @4
    pub entry: u64,               // @8
    pub dram_base: u64,           // @16
    pub dram_top: u64,            // @24
    pub memmap_pa: u64,           // @32（刀 4 填）
    pub memmap_size: u32,         // @40（刀 4 填）
    pub memmap_desc_size: u32,    // @44（刀 4 填）
    pub memmap_desc_version: u32, // @48（刀 4 填）
    pub _r0: u32,                 // @52 保留零
    pub gop_fb_pa: u64,           // @56 留位零（V-1 fbcon）
    pub gop_width: u32,           // @64 留位零
    pub gop_height: u32,          // @68 留位零
    pub gop_stride: u32,          // @72 留位零
    pub gop_format: u32,          // @76 留位零
    pub init_blob_pa: u64,        // @80 留位零（裁决 B）
    pub init_blob_len: u64,       // @88 留位零（裁决 B）
    pub cmdline_pa: u64,          // @96 留位零（裁决 B）
    pub cmdline_len: u64,         // @104 留位零（裁决 B）
}

/// 布局执法表（与 kernel/src/handoff.rs 逐行相同——两侧同表执法）。
const _: () = {
    assert!(size_of::<BerryHandoff>() == 112);
    assert!(offset_of!(BerryHandoff, magic) == 0);
    assert!(offset_of!(BerryHandoff, version) == 4);
    assert!(offset_of!(BerryHandoff, entry) == 8);
    assert!(offset_of!(BerryHandoff, dram_base) == 16);
    assert!(offset_of!(BerryHandoff, dram_top) == 24);
    assert!(offset_of!(BerryHandoff, memmap_pa) == 32);
    assert!(offset_of!(BerryHandoff, memmap_size) == 40);
    assert!(offset_of!(BerryHandoff, memmap_desc_size) == 44);
    assert!(offset_of!(BerryHandoff, memmap_desc_version) == 48);
    assert!(offset_of!(BerryHandoff, _r0) == 52);
    assert!(offset_of!(BerryHandoff, gop_fb_pa) == 56);
    assert!(offset_of!(BerryHandoff, gop_width) == 64);
    assert!(offset_of!(BerryHandoff, gop_height) == 68);
    assert!(offset_of!(BerryHandoff, gop_stride) == 72);
    assert!(offset_of!(BerryHandoff, gop_format) == 76);
    assert!(offset_of!(BerryHandoff, init_blob_pa) == 80);
    assert!(offset_of!(BerryHandoff, init_blob_len) == 88);
    assert!(offset_of!(BerryHandoff, cmdline_pa) == 96);
    assert!(offset_of!(BerryHandoff, cmdline_len) == 104);
};

/// 刀 1 骨架行：ASCII + CRLF（EFI ConOut 惯例——部分终端只认 CRLF）。
const BANNER: &str = "BerryBoot EFI skeleton (V-0a)\r\n";

/// 行 7（V-0b 刀 3）：段装载——`span()` 整窗一次
/// AllocatePages(AllocateAddress) 认领（段间缝隙一并拿下：段可共享页/
/// 有缝，逐段认领撞页冲突）→ 全窗清零（.bss + 缝隙确定性归零，对齐
/// QEMU `-kernel` 直加载零 RAM 起步）→ 逐段 copy filesz → 回读逐字节
/// 对拍（标记 = 状态谓词，不建立在「拷了就算」上）→ handoff 页填 v0
/// （裁决 A：结构 V-0b 冻结实例化；裁决 B：gop/cmdline 留位零值；
/// memmap 刀 4 填；init_blob_* V-1c 填——`init_pa/init_len` 由 kfile 段
/// INITRD.BIN 读入产出，缺省 0/0 = 无 blob）。全过打 `load: segs=<n>
/// pages=<n> ok` 并返 `Some(hpa)`（handoff 物理地址——刀 4 终段消费）；
/// 任一步失败打状态码诊断行返 `None`（终标缺席即失败面——机器可读缺席
/// 报告）。零 panic：一切失败走诊断行返回。
fn load_kernel(
    bs: &berry_efi::EfiBootServices,
    con_out: &berry_efi::EfiSimpleTextOutputProtocol,
    kbuf: *mut c_void,
    bo: &berry_loader::BootElf,
    init_pa: u64,
    init_len: u64,
) -> Option<u64> {
    let segs = bo.segments();
    // 整窗认领（AllocateAddress 对已占用页返错不覆盖——kbuf/固件数据若
    // 压进 span，此处干净报 EFI 状态码而非静默腐化）。
    let Some((span_base, span_end)) = bo.span() else {
        print_line(con_out, |l| l.push_str("load: span none"));
        return None;
    };
    let pages = ((span_end - span_base) / 4096) as usize;
    let mut got: u64 = span_base;
    let r_ap =
        unsafe { (bs.allocate_pages)(EFI_ALLOCATE_ADDRESS, EFI_LOADER_DATA, pages, &mut got) };
    if r_ap != EFI_SUCCESS || got != span_base {
        print_line(con_out, |l| {
            l.push_str("load: claim r=0x");
            let mut t = [0u8; 16];
            l.push(fmt_u64_hex(r_ap as u64, &mut t));
            l.push_str(" addr=");
            l.push_u64_hex(got);
        });
        return None;
    }
    // 全窗清零（span_base 页对齐 → u64 字写）。
    let zp = span_base as *mut u64;
    for i in 0..((span_end - span_base) / 8) as usize {
        unsafe { zp.add(i).write(0) };
    }
    // 逐段拷 filesz 字节（file_off/filesz 已过 boot_parse 界检）。
    for s in segs {
        unsafe {
            core::ptr::copy_nonoverlapping(
                (kbuf as *const u8).add(s.file_off as usize),
                s.paddr as *mut u8,
                s.filesz as usize,
            );
        }
    }
    // 回读对拍：任一段字节差 = 诊断行（段号定位）+ 失败。
    for (i, s) in segs.iter().enumerate() {
        let src = unsafe {
            core::slice::from_raw_parts((kbuf as *const u8).add(s.file_off as usize), s.filesz as usize)
        };
        let dst =
            unsafe { core::slice::from_raw_parts(s.paddr as *const u8, s.filesz as usize) };
        if src != dst {
            print_line(con_out, |l| {
                l.push_str("load: verify i=");
                l.push_u32_dec(i as u32);
            });
            return None;
        }
    }
    // handoff 页：任意页一页 EfiLoaderData（不 FreePages——刀 4 跳 entry
    // 时 x0 指向它，内核接管后经 memmap 纳管）。
    let mut hpa: u64 = 0;
    let r_h = unsafe { (bs.allocate_pages)(EFI_ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, 1, &mut hpa) };
    if r_h != EFI_SUCCESS || hpa == 0 {
        print_line(con_out, |l| {
            l.push_str("load: hoff r=0x");
            let mut t = [0u8; 16];
            l.push(fmt_u64_hex(r_h as u64, &mut t));
        });
        return None;
    }
    unsafe {
        (hpa as *mut BerryHandoff).write(BerryHandoff {
            magic: HANDOFF_MAGIC,
            version: HANDOFF_VERSION,
            entry: bo.entry,
            dram_base: DRAM_BASE,
            dram_top: DRAM_TOP,
            memmap_pa: 0,
            memmap_size: 0,
            memmap_desc_size: 0,
            memmap_desc_version: 0,
            _r0: 0,
            gop_fb_pa: 0,
            gop_width: 0,
            gop_height: 0,
            gop_stride: 0,
            gop_format: 0,
            init_blob_pa: init_pa,
            init_blob_len: init_len,
            cmdline_pa: 0,
            cmdline_len: 0,
        });
    }
    print_line(con_out, |l| {
        l.push_str("load: segs=");
        l.push_u32_dec(segs.len() as u32);
        l.push_str(" pages=");
        l.push_u32_dec(pages as u32);
        l.push_str(" ok");
    });
    Some(hpa)
}

/// EBS 后直写 PL011：QEMU virt UART0 @0x0900_0000（与 AAVMF ConOut 同
/// 设备——串口即同一个）——DR@0x00 逐字节、**FR@0x018 TXFE(0x80) 轮询**
/// （PL011 寄存器偏移按 TRM §3.2 **字节**口径：FR = 字索引 6 = 字节 0x018
/// ——写成字节 0x06 落进 RSR/ECR 恒 0，TXFE 永等不到即静默死转（刀 4 四轮
/// 实踩，gdbstub 断点取证）；同 内核 arch/aarch64/src/uart.rs 的
/// UART_FR=0x018）。EBS 过后 ConOut 已死，终标 `BERRYBOOT ok`
/// 由引导链自持打出。
fn uart_line(msg: &[u8]) {
    let uart = 0x0900_0000usize as *mut u8;
    for &b in msg {
        unsafe {
            while (uart.add(0x18).read_volatile() & 0x80) == 0 {}
            uart.write_volatile(b);
        }
    }
}

/// PL011 直写的行打印（同 print_line 的组装器形态，发射端换直访硬件）。
/// EBS 尝试环内/后的诊断一律走此面：ConOut 调用既是「动内存图让 map key
/// 过期」的源，又可能在 EBS 半拆状态下已死——观测面不依赖被测对象。
fn uart_print(f: impl FnOnce(&mut Line)) {
    let mut l = Line::new();
    f(&mut l);
    l.push_str("\r\n");
    uart_line(&l.buf[..l.n]);
}

/// dcache 按地址清刷到 PoC（`dc cvac`，64B 步长 = cortex-a72 cacheline）。
/// 真机面：MMU 关前把引导链写入（内核映像/handoff/memmap）压进 DRAM；
/// QEMU TCG 无实缓存 = no-op 零成本。收尾 `dsb sy` 保证清刷点之前完成。
/// 按 P7-x4 纪律不标 nomem：`dc cvac` 架构上真读缓存行。
fn dcache_clean(base: u64, len: u64) {
    let mut a = base & !0x3fu64;
    let end = base + len;
    while a < end {
        unsafe { core::arch::asm!("dc cvac, {0}", in(reg) a) };
        a += 64;
    }
    unsafe { core::arch::asm!("dsb sy") };
}

/// 行 8 + 终段（V-0b 刀 4）：内存图取回 → handoff memmap_* 落值 →
/// EBS → 整备跳 entry。成功路径**永不返回**（EBS 后固件全死，`br entry`
/// 把控制权交内核）；任一步失败打状态码诊断行返回（此时必在 EBS
/// 前——ConOut 仍活，诊断行走正常打印面；终标缺席即失败面）。
///
/// - GetMemoryMap：首探 4096B 缓冲，`EFI_BUFFER_TOO_SMALL` 按出参所需
///   长扩容重试（帽两轮）；非 SUCCESS 非扩容错 = 诊断行拒。
/// - EBS 重试协议（spec 惯例）：**key 有效期 = 取图到 EBS 之间零 boot
///   service 调用**（环内零打印零协议——ConOut 也算动图源）；被拒即
///   key 过期 → 重取图（handoff memmap_* 随之更新）再试，帽三轮；环内
///   外诊断一律 PL011 直写（不依赖可能已死的 ConOut）。watchdog 归零
///   在取图前完成（EFI 默认 5 分钟看门狗会重启整机，内核长跑必关）。
/// - CurrentEL 预检 ≠ EL1 拒跳（AAVMF virt 默认 EL1；本函数的 sctlr_el1
///   整备只对 EL1 语境成立——内核自带的 EL2 降级路预期的是 QEMU 复位
///   态而非 EFI 态，错态跳转两头皆坑）。
/// - EBS 后序列：直写 PL011 `BERRYBOOT ok`（标记 = 含 EBS 全链真过的
///   状态谓词）→ DAIF 全屏蔽 → dcache 清刷（span + handoff 页 + memmap
///   缓冲）→ SCTLR_EL1 清 M/A/C/I + dsb/isb → `ic iallu` + `tlbi
///   vmalle1`（全失效——MMU 已关后的收尾防线）→ `br entry`（x0 =
///   handoff PA、x1..x3 = 0，对齐 QEMU `-kernel` 入场寄存器形态——内核
///   x19 抢救后按 DTB 探测分流，BRYH 魔数与 0xd00dfeed 互不碰撞）。
fn final_boot(
    bs: &berry_efi::EfiBootServices,
    con_out: &berry_efi::EfiSimpleTextOutputProtocol,
    image_handle: *mut c_void,
    hpa: u64,
    bo: &berry_loader::BootElf,
    init_pa: u64,
    init_len: u64,
) {
    // CurrentEL 预检（EL 编码在位 [3:2]：EL1 → 0x4）。
    let el: u64;
    unsafe { core::arch::asm!("mrs {}, CurrentEL", out(reg) el) };
    if (el >> 2) & 3 != 1 {
        print_line(con_out, |l| {
            l.push_str("boot: el=");
            l.push_u32_dec(((el >> 2) & 3) as u32);
        });
        return;
    }
    // watchdog 归零：EFI 默认 5 分钟看门狗会在内核长跑（smoke 类分钟级）
    // 中途重启整机——EBS 不豁免它，GRUB 同款显式关闭。
    let r_wd = unsafe { (bs.set_watchdog_timer)(0, 0, 0, core::ptr::null()) };
    if r_wd != EFI_SUCCESS {
        print_line(con_out, |l| {
            l.push_str("boot: wd r=0x");
            let mut t = [0u8; 16];
            l.push(fmt_u64_hex(r_wd as u64, &mut t));
        });
        return;
    }
    // 内存图缓冲：AllocatePool(EfiLoaderData)（EBS 后保活，内核经
    // handoff 纳管——不 FreePool）。
    let mut mm_cap: usize = 4096;
    let mut mm_buf: *mut c_void = core::ptr::null_mut();
    let r_ap =
        unsafe { (bs.allocate_pool)(EFI_LOADER_DATA, mm_cap, &mut mm_buf) };
    if r_ap != EFI_SUCCESS || mm_buf.is_null() {
        print_line(con_out, |l| {
            l.push_str("mmap: alloc r=0x");
            let mut t = [0u8; 16];
            l.push(fmt_u64_hex(r_ap as u64, &mut t));
        });
        return;
    }
    // GetMemoryMap：出参 mm_size 回带所需长（含表尾换页余量）。
    let mut mm_size: usize = mm_cap;
    let mut map_key: usize = 0;
    let mut dsize: usize = 0;
    let mut dver: u32 = 0;
    let mut r_mm = EFI_BUFFER_TOO_SMALL;
    for _ in 0..2 {
        mm_size = mm_cap;
        r_mm = unsafe {
            (bs.get_memory_map)(
                &mut mm_size,
                mm_buf as *mut EfiMemoryDescriptor,
                &mut map_key,
                &mut dsize,
                &mut dver,
            )
        };
        if r_mm == EFI_SUCCESS {
            break;
        }
        if r_mm != EFI_BUFFER_TOO_SMALL {
            break;
        }
        mm_cap = mm_size + 64;
        let r_grow =
            unsafe { (bs.allocate_pool)(EFI_LOADER_DATA, mm_cap, &mut mm_buf) };
        if r_grow != EFI_SUCCESS || mm_buf.is_null() {
            r_mm = r_grow; // 落到下方统一诊断行
            break;
        }
    }
    if r_mm != EFI_SUCCESS || dsize == 0 {
        print_line(con_out, |l| {
            l.push_str("mmap: get r=0x");
            let mut t = [0u8; 16];
            l.push(fmt_u64_hex(r_mm as u64, &mut t));
        });
        return;
    }
    // handoff memmap_* 四字段落值 + EBS 重试协议。**map key 的有效期 =
    // 取图到 EBS 之间零 boot service 调用**（spec 惯例——任何服务调用
    // 都可能动内存图让 key 过期；ConOut 打印也算）：取图 → 直写 handoff
    // → 立即 EBS，环内零协议调用零打印；被拒（EFI_INVALID_PARAMETER =
    // key 过期）→ 重取图（复用同缓冲）再试，帽三轮。环内外诊断一律
    // PL011 直写（uart_print）：不依赖可能已死的 ConOut，也不会自己
    // 弄过期 key。
    let h = hpa as *mut BerryHandoff;
    unsafe {
        (*h).memmap_pa = mm_buf as u64;
        (*h).memmap_size = mm_size as u32;
        (*h).memmap_desc_size = dsize as u32;
        (*h).memmap_desc_version = dver;
    }
    let mut ebs_done = false;
    for _att in 0..3u32 {
        // 环内常态路径零打印（PL011 MMIO 直写不动内存图、技术上不碍
        // key——但首试即成是常态，不留调试噪声）；失败诊断走环内 EBS
        // 前后的 uart_print 两侧（ebs: r= / mmap: reget=）。
        let r_ebs = unsafe { (bs.exit_boot_services)(image_handle, map_key) };
        if r_ebs == EFI_SUCCESS {
            ebs_done = true;
            break;
        }
        uart_print(|l| {
            l.push_str("ebs: r=0x");
            let mut t = [0u8; 16];
            l.push(fmt_u64_hex(r_ebs as u64, &mut t));
        });
        let mut rsize: usize = mm_cap;
        let r_re = unsafe {
            (bs.get_memory_map)(
                &mut rsize,
                mm_buf as *mut EfiMemoryDescriptor,
                &mut map_key,
                &mut dsize,
                &mut dver,
            )
        };
        if r_re != EFI_SUCCESS || rsize > mm_cap || dsize == 0 {
            uart_print(|l| {
                l.push_str("mmap: reget r=0x");
                let mut t = [0u8; 16];
                l.push(fmt_u64_hex(r_re as u64, &mut t));
            });
            break;
        }
        mm_size = rsize;
        unsafe {
            (*h).memmap_size = rsize as u32;
            (*h).memmap_desc_size = dsize as u32;
            (*h).memmap_desc_version = dver;
        }
    }
    if !ebs_done {
        uart_line(b"ebs: retry fail\r\n");
        return;
    }
    // ===== EBS 已过：固件服务全死——以下只许直访硬件，零协议调用 =====
    // 行 8 标记在 EBS 后经 PL011 打出：n = EBS 接受的那张终图的描述符数
    // （重试路径下与首图不同——报终值才是状态谓词）。
    let ndesc = (mm_size / dsize) as u32;
    uart_print(|l| {
        l.push_str("mmap: n=");
        l.push_u32_dec(ndesc);
        l.push_str(" ok");
    });
    uart_line(b"BERRYBOOT ok\r\n");
    // DAIF 全屏蔽（D/A/I/F 四位——immediate = 直写该四位组）。
    unsafe { core::arch::asm!("msr daifset, #0xf") };
    // dcache 清刷：内核映像整窗 + handoff 页 + memmap 缓冲（全部
    // identity 映射 VA=PA，cvac 按 VA 有效）。
    if let Some((sb, se)) = bo.span() {
        dcache_clean(sb, se - sb);
    }
    dcache_clean(hpa, 4096);
    dcache_clean(mm_buf as u64, mm_size as u64);
    // V-1c：init blob 页入刷集——读入经固件 CPU 拷贝、脏线随 SCTLR.C 清零
    // 而丢 = 内核读到旧内存（与内核映像/handoff/memmap 同一条纪律）。
    if init_len > 0 {
        dcache_clean(init_pa, (init_len + 4095) & !4095u64);
    }
    // SCTLR_EL1 清 M(0)/A(1)/C(2)/I(12)：MMU 关（内核自带页表从零建）、
    // 对齐检查关、icache/dcache 关——对齐内核 boot asm 为 QEMU `-kernel`
    // 复位态写的入场假设。
    let sctlr: u64;
    unsafe { core::arch::asm!("mrs {}, sctlr_el1", out(reg) sctlr) };
    let cleared = sctlr & !0x1007u64;
    unsafe {
        core::arch::asm!("msr sctlr_el1, {0}", in(reg) cleared);
        core::arch::asm!("dsb sy");
        core::arch::asm!("isb");
        // icache 全失效 + TLB 全失效（MMU 已关后的缓存残影防线；同 P7-x4
        // 纪律不标 nomem——二者皆真影响内存/取指一致性）。
        core::arch::asm!("ic iallu");
        core::arch::asm!("tlbi vmalle1");
        core::arch::asm!("dsb sy");
        core::arch::asm!("isb");
        // 跳：x0 = handoff PA、x1..x3 = 0（对齐 QEMU `-kernel` 入场寄存器
        // 形态）；entry 永不返回（内核自设 SP——boot asm 链接栈）。
        core::arch::asm!(
            "br {target}",
            in("x0") hpa,
            in("x1") 0u64,
            in("x2") 0u64,
            in("x3") 0u64,
            target = in(reg) bo.entry,
        );
    }
    // br 不归；防御兜底（不可达）——EBS 后无路可退，挂死优于错跑。
    loop {}
}

/// 单行组装器：定长栈缓冲，零分配；越界即截断（引导语境纪律）。
struct Line {
    buf: [u8; 128],
    n: usize,
}

impl Line {
    fn new() -> Line {
        Line { buf: [0; 128], n: 0 }
    }
    fn push(&mut self, s: &[u8]) {
        let m = s.len().min(self.buf.len() - self.n);
        self.buf[self.n..self.n + m].copy_from_slice(&s[..m]);
        self.n += m;
    }
    fn push_str(&mut self, s: &str) {
        self.push(s.as_bytes());
    }
    fn push_u32_dec(&mut self, v: u32) {
        let mut tmp = [0u8; 10];
        self.push(fmt_u32_dec(v, &mut tmp));
    }
    fn push_u64_hex(&mut self, v: u64) {
        self.push(b"0x");
        let mut tmp = [0u8; 16];
        self.push(fmt_u64_hex(v, &mut tmp));
    }
}

/// ConOut 打一行：Line 组装（ASCII）→ UTF-16 编码 → OutputString。
fn print_line(
    con_out: &berry_efi::EfiSimpleTextOutputProtocol,
    parts: impl FnOnce(&mut Line),
) {
    let mut line = Line::new();
    parts(&mut line);
    line.push_str("\r\n");
    let mut wbuf = [0u16; 96];
    let s = utf16_encode_bytes(&line.buf[..line.n], &mut wbuf);
    unsafe { (con_out.output_string)(con_out as *const _ as *mut _, s.as_ptr()) };
}

/// ASCII 字节面 → UTF-16（横幅全 ASCII，逐字节拓宽即合法 UCS-2）。
fn utf16_encode_bytes<'a>(src: &[u8], out: &'a mut [u16]) -> &'a mut [u16] {
    let mut n = 0;
    for &b in src {
        if n >= out.len() {
            break;
        }
        out[n] = b as u16;
        n += 1;
    }
    &mut out[..n]
}

/// NUL 结尾 UCS-2 厂商串 → 安全 &[u16]（长帽 64——防御式，越界截断）。
/// 固件表数据在应用生命周期内常驻，出参钉 'static（读后即用，不跨阶段存）。
unsafe fn ucs2_cstr_slice(ptr: *const u16) -> &'static [u16] {
    const CAP: usize = 64;
    let mut n = 0;
    while n < CAP && *ptr.add(n) != 0 {
        n += 1;
    }
    core::slice::from_raw_parts(ptr, n)
}

#[no_mangle]
pub extern "C" fn efi_main(
    image_handle: *mut c_void,
    system_table: *mut EfiSystemTable,
) -> EfiStatus {
    // 引导语境的防御式：表指针空/签名不符 → 静默成功返回（横幅首行本身
    // 即验收标记——打出 = 全链通）。
    if system_table.is_null() {
        return EFI_SUCCESS;
    }
    let st = unsafe { &*system_table };
    if st.hdr.signature != EFI_SYSTEM_TABLE_SIGNATURE || st.con_out.is_null() {
        return EFI_SUCCESS;
    }
    let con_out = unsafe { &*st.con_out };

    // 行 1：骨架横幅（刀 1 面不变）。
    let mut wbuf = [0u16; 64];
    let s = utf16_encode(BANNER, &mut wbuf);
    unsafe { (con_out.output_string)(con_out as *const _ as *mut _, s.as_ptr()) };

    // 行 2：固件厂商/修订（SystemTable 直读；厂商 UCS-2 → ASCII 有损）。
    print_line(con_out, |l| {
        l.push_str("fw: ");
        if !st.firmware_vendor.is_null() {
            let mut vbuf = [0u8; 48];
            let vendor =
                utf16_to_ascii(unsafe { ucs2_cstr_slice(st.firmware_vendor) }, &mut vbuf);
            l.push(vendor);
        }
        l.push_str(" rev=0x");
        let mut htmp = [0u8; 8];
        l.push(fmt_u64_hex(st.firmware_revision as u64, &mut htmp));
    });

    // 行 3：GOP 模式信息（LocateProtocol → QueryMode 当前模式——只读面，
    // 裁决 C：只信息行不画像素）。失败打状态码诊断行：实测 AAVMF
    // `virt -nographic` 无显示设备 → GOP 实例缺席 → EFI_NOT_FOUND
    // （0x800000000000000E）——run-efi 挂 -device ramfb 供 RamfbDxe。
    let bs = st.boot_services;
    if !bs.is_null() && unsafe { (*bs).hdr.signature } == EFI_BOOT_SERVICES_SIGNATURE {
        let mut gop: *mut c_void = core::ptr::null_mut();
        let r = unsafe {
            ((*bs).locate_protocol)(
                &EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID as *const _ as *mut _,
                core::ptr::null_mut(),
                &mut gop,
            )
        };
        if r == EFI_SUCCESS && !gop.is_null() {
            let g = gop as *mut EfiGraphicsOutputProtocol;
            let gop_ref = unsafe { &*g };
            if !gop_ref.mode.is_null() {
                let m = unsafe { &*gop_ref.mode };
                let mut size = 0usize;
                let mut info: *mut EfiGraphicsOutputModeInfo = core::ptr::null_mut();
                let rq = unsafe {
                    (gop_ref.query_mode)(g, m.mode as usize, &mut size, &mut info)
                };
                if rq == EFI_SUCCESS && !info.is_null() {
                    let i = unsafe { &*info };
                    let cur = m.mode;
                    let max = m.max_mode;
                    print_line(con_out, |l| {
                        l.push_str("gop: ");
                        l.push_u32_dec(i.horizontal_resolution);
                        l.push(b"x");
                        l.push_u32_dec(i.vertical_resolution);
                        l.push_str(" fmt=");
                        l.push_u32_dec(i.pixel_format);
                        l.push_str(" mode=");
                        l.push_u32_dec(cur);
                        l.push(b"/");
                        l.push_u32_dec(max);
                        l.push_str(" fb=");
                        l.push_u64_hex(m.frame_buffer_base);
                    });
                } else {
                    print_line(con_out, |l| {
                        l.push_str("gop: query fail rq=0x");
                        let mut t = [0u8; 16];
                        l.push(fmt_u64_hex(rq as u64, &mut t));
                        l.push_str(" info?=");
                        l.push_u32_dec(info.is_null() as u32);
                    });
                }
            } else {
                print_line(con_out, |l| l.push_str("gop: mode ptr null"));
            }
        } else {
            print_line(con_out, |l| {
                l.push_str("gop: locate fail r=0x");
                let mut t = [0u8; 16];
                l.push(fmt_u64_hex(r as u64, &mut t));
                l.push_str(" gop?=");
                l.push_u32_dec(gop.is_null() as u32);
            });
        }
    } else {
        print_line(con_out, |l| {
            l.push_str("gop: bs null?=");
            l.push_u32_dec(bs.is_null() as u32);
        });
    }

    // 行 4–7：SFS 开卷四段——LoadedImage(image handle) → DeviceHandle
    // → SFS → OpenVolume → 根句柄（各段共用）→ 行 4 HELLO.TXT 对拍（刀 3
    // 原样）+ 行 5 KERNEL.ELF 整档读入（V-0b 刀 1）+ 行 6 引导视图解析
    // （V-0b 刀 2，kfile 缓冲就地消费）+ 行 7 段装载（V-0b 刀 3）→ Close。
    // 全段过才打 `BERRYBOOT ok`（验收标记 = 状态谓词）；任一步失败打
    // `fs:`/`kfile:`/`elf:`/`load: <step> r=0x…/e=…` 诊断行。
    if !bs.is_null() && unsafe { (*bs).hdr.signature } == EFI_BOOT_SERVICES_SIGNATURE {
        let hp = unsafe { (*bs).handle_protocol };
        let mut li: *mut c_void = core::ptr::null_mut();
        let r_li = unsafe {
            hp(
                image_handle,
                &EFI_LOADED_IMAGE_PROTOCOL_GUID as *const _ as *mut _,
                &mut li,
            )
        };
        let device = if r_li == EFI_SUCCESS && !li.is_null() {
            unsafe { (*(li as *mut EfiLoadedImageProtocol)).device_handle }
        } else {
            core::ptr::null_mut()
        };
        if !device.is_null() {
            let mut sfs: *mut c_void = core::ptr::null_mut();
            let r_sfs = unsafe {
                hp(
                    device,
                    &EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID as *const _ as *mut _,
                    &mut sfs,
                )
            };
            if r_sfs == EFI_SUCCESS && !sfs.is_null() {
                let sfs_ref = unsafe { &*(sfs as *mut EfiSimpleFileSystemProtocol) };
                let mut root = core::ptr::null_mut();
                let r_vol = unsafe { (sfs_ref.open_volume)(sfs as *mut _, &mut root) };
                if r_vol == EFI_SUCCESS && !root.is_null() {
                    let root_ref = unsafe { &*root };

                    // 行 4：HELLO.TXT 逐字节对拍（V-0a 刀 3 验收面原样）。
                    let mut name16 = [0u16; 16];
                    let name = utf16_encode_bytes(b"HELLO.TXT", &mut name16);
                    let mut file = core::ptr::null_mut();
                    let r_open = unsafe {
                        (root_ref.open)(root, &mut file, name.as_ptr(), EFI_FILE_MODE_READ, 0)
                    };
                    if r_open == EFI_SUCCESS && !file.is_null() {
                        let file_ref = unsafe { &*file };
                        let mut buf = [0u8; 256];
                        let mut n = buf.len();
                        let r_read = unsafe {
                            (file_ref.read)(file, &mut n, buf.as_mut_ptr() as *mut c_void)
                        };
                        // 判读前先收句柄（错误路径也不漏 close——句柄生命周期
                        // 与判定解耦）。
                        unsafe { (file_ref.close)(file) };
                        if r_read == EFI_SUCCESS
                            && n == EXPECTED_HELLO.len()
                            && &buf[..n] == EXPECTED_HELLO
                        {
                            print_line(con_out, |l| {
                                l.push_str("fs: HELLO.TXT ");
                                l.push_u32_dec(n as u32);
                                l.push_str("B match");
                            });
                        } else {
                            print_line(con_out, |l| {
                                l.push_str("fs: mismatch read=");
                                l.push_u32_dec(n as u32);
                                l.push_str(" expect=");
                                l.push_u32_dec(EXPECTED_HELLO.len() as u32);
                                l.push_str(" r=0x");
                                let mut t = [0u8; 16];
                                l.push(fmt_u64_hex(r_read as u64, &mut t));
                            });
                        }
                    } else {
                        print_line(con_out, |l| {
                            l.push_str("fs: open r=0x");
                            let mut t = [0u8; 16];
                            l.push(fmt_u64_hex(r_open as u64, &mut t));
                        });
                    }

                    // 行 5（V-0b 刀 1）：KERNEL.ELF 整档读入——GetInfo 探
                    // file_size → AllocatePool(EfiLoaderData) 按长整档缓冲 →
                    // 循环 Read 收齐（got == file_size 才算过）。缓冲**不
                    // FreePool**：刀 2 ELF 解析 / 刀 3 段拷贝就地消费，应用
                    // 返回后由固件回收。帽 64MiB：荒谬长先拒（VMO_MAX_SIZE
                    // 同精神——防无界分配把引导面炸成 panic）。
                    let mut name16k = [0u16; 16];
                    let kname = utf16_encode_bytes(b"KERNEL.ELF", &mut name16k);
                    let mut kfile = core::ptr::null_mut();
                    let r_kopen = unsafe {
                        (root_ref.open)(root, &mut kfile, kname.as_ptr(), EFI_FILE_MODE_READ, 0)
                    };
                    if r_kopen == EFI_SUCCESS && !kfile.is_null() {
                        let kfile_ref = unsafe { &*kfile };
                        let mut info_buf = [0u8; 256];
                        let mut info_len = info_buf.len();
                        let r_gi = unsafe {
                            (kfile_ref.get_info)(
                                kfile,
                                &EFI_FILE_INFO_GUID as *const _,
                                &mut info_len,
                                info_buf.as_mut_ptr() as *mut c_void,
                            )
                        };
                        // 定长头 80B（到 attribute；name 变长不入面）。
                        if r_gi == EFI_SUCCESS && info_len >= 80 {
                            let info = info_buf.as_ptr() as *const EfiFileInfo;
                            let fsize = unsafe { (*info).file_size } as usize;
                            if fsize > 0 && fsize <= 64 * 1024 * 1024 {
                                let mut kbuf: *mut c_void = core::ptr::null_mut();
                                let r_ap = unsafe {
                                    ((*bs).allocate_pool)(EFI_LOADER_DATA, fsize, &mut kbuf)
                                };
                                if r_ap == EFI_SUCCESS && !kbuf.is_null() {
                                    let mut got = 0usize;
                                    let mut r_rd = EFI_SUCCESS;
                                    while got < fsize {
                                        let mut chunk = fsize - got;
                                        r_rd = unsafe {
                                            (kfile_ref.read)(
                                                kfile,
                                                &mut chunk,
                                                (kbuf as *mut u8).add(got) as *mut c_void,
                                            )
                                        };
                                        if r_rd != EFI_SUCCESS || chunk == 0 {
                                            break;
                                        }
                                        got += chunk;
                                    }
                                    if r_rd == EFI_SUCCESS && got == fsize {
                                        print_line(con_out, |l| {
                                            l.push_str("kfile: ");
                                            l.push_u32_dec(fsize as u32);
                                            l.push_str("B ok");
                                        });
                                        // 行 5b（V-1c）：INITRD.BIN 整档读入
                                        // → AllocatePages(EfiLoaderData) 页
                                        // 钉——**必须发生在终段 GetMemoryMap
                                        // 之前**（页进图成非 Conventional，
                                        // 内核 V-1b pass2 reserve 自动保）。
                                        // pa/len 透传 load_kernel 填 handoff
                                        // init_blob_*（@80/@88）。缺席（旧盘
                                        // 不带 INITRD.BIN）打 `init: absent`
                                        // 续行——内核降级 ROOTFS absent，
                                        // 非致命（裁决 B 留位零值同义）。
                                        // 帽 16MiB：与内核 ROOTFS_MAX 同口径。
                                        let mut name16i = [0u16; 16];
                                        let iname =
                                            utf16_encode_bytes(b"INITRD.BIN", &mut name16i);
                                        let mut ifile = core::ptr::null_mut();
                                        let r_iopen = unsafe {
                                            (root_ref.open)(root, &mut ifile, iname.as_ptr(), EFI_FILE_MODE_READ, 0)
                                        };
                                        let mut init_pa: u64 = 0;
                                        let mut init_len: u64 = 0;
                                        if r_iopen == EFI_SUCCESS && !ifile.is_null() {
                                            let ifile_ref = unsafe { &*ifile };
                                            let mut ibuf = [0u8; 256];
                                            let mut ilen = ibuf.len();
                                            let r_igi = unsafe {
                                                (ifile_ref.get_info)(
                                                    ifile,
                                                    &EFI_FILE_INFO_GUID as *const _,
                                                    &mut ilen,
                                                    ibuf.as_mut_ptr() as *mut c_void,
                                                )
                                            };
                                            if r_igi == EFI_SUCCESS && ilen >= 80 {
                                                let isize_ = unsafe {
                                                    (*(ibuf.as_ptr() as *const EfiFileInfo)).file_size
                                                        as usize
                                                };
                                                if isize_ > 0 && isize_ <= 16 * 1024 * 1024 {
                                                    let ipages = (isize_ + 4095) / 4096;
                                                    let mut ipa: u64 = 0;
                                                    let r_iap = unsafe {
                                                        ((*bs).allocate_pages)(
                                                            EFI_ALLOCATE_ANY_PAGES,
                                                            EFI_LOADER_DATA,
                                                            ipages,
                                                            &mut ipa,
                                                        )
                                                    };
                                                    if r_iap == EFI_SUCCESS && ipa != 0 {
                                                        let mut igot = 0usize;
                                                        let mut r_ird = EFI_SUCCESS;
                                                        while igot < isize_ {
                                                            let mut ichunk = isize_ - igot;
                                                            r_ird = unsafe {
                                                                (ifile_ref.read)(
                                                                    ifile,
                                                                    &mut ichunk,
                                                                    (ipa as *mut u8).add(igot)
                                                                        as *mut c_void,
                                                                )
                                                            };
                                                            if r_ird != EFI_SUCCESS || ichunk == 0 {
                                                                break;
                                                            }
                                                            igot += ichunk;
                                                        }
                                                        if r_ird == EFI_SUCCESS && igot == isize_ {
                                                            print_line(con_out, |l| {
                                                                l.push_str("init: ");
                                                                l.push_u32_dec(isize_ as u32);
                                                                l.push_str("B ok");
                                                            });
                                                            init_pa = ipa;
                                                            init_len = isize_ as u64;
                                                        } else {
                                                            print_line(con_out, |l| {
                                                                l.push_str("init: read got=");
                                                                l.push_u32_dec(igot as u32);
                                                                l.push_str(" want=");
                                                                l.push_u32_dec(isize_ as u32);
                                                            });
                                                            // 不 FreePages：读残页留图
                                                            // 无害（终图非 Conventional，
                                                            // 内核 reserve 保住不再用）。
                                                        }
                                                    } else {
                                                        print_line(con_out, |l| {
                                                            l.push_str("init: alloc r=0x");
                                                            let mut t = [0u8; 16];
                                                            l.push(fmt_u64_hex(r_iap as u64, &mut t));
                                                        });
                                                    }
                                                } else {
                                                    print_line(con_out, |l| {
                                                        l.push_str("init: size 帽 size=");
                                                        l.push_u32_dec(isize_ as u32);
                                                    });
                                                }
                                            } else {
                                                print_line(con_out, |l| {
                                                    l.push_str("init: get_info r=0x");
                                                    let mut t = [0u8; 16];
                                                    l.push(fmt_u64_hex(r_igi as u64, &mut t));
                                                });
                                            }
                                            // 判读边界外收句柄（同 kfile 面纪律）。
                                            unsafe { (ifile_ref.close)(ifile) };
                                        } else {
                                            print_line(con_out, |l| l.push_str("init: absent"));
                                        }
                                        // 行 6（V-0b 刀 2）：引导视图解析——
                                        // kfile 缓冲就地借用（零拷贝），
                                        // p_paddr 口径出段 + 入口（VMA 高半
                                        // 是链接视图，引导链按 paddr 落内存）。
                                        let kelf = unsafe {
                                            core::slice::from_raw_parts(kbuf as *const u8, fsize)
                                        };
                                        match berry_loader::boot_parse(kelf, DRAM_BASE, DRAM_TOP)
                                        {
                                            Ok(bo) => {
                                                print_line(con_out, |l| {
                                                    l.push_str("elf: segs=");
                                                    l.push_u32_dec(bo.segments().len() as u32);
                                                    l.push_str(" entry=");
                                                    l.push_u64_hex(bo.entry);
                                                    l.push_str(" ok");
                                                });
                                                // 行 7（V-0b 刀 3）：段装载 +
                                                // handoff v0（bs 签名已在行 3
                                                // 前验过）；刀 4 起成功返
                                                // handoff PA 直进终段——
                                                // mmap + EBS + 整备跳 entry，
                                                // 成功即接管（不归此臂）。
                                                // V-1c：init_blob_*（kfile 段
                                                // INITRD.BIN 产物）随行入
                                                // handoff。
                                                if let Some(hpa) = load_kernel(
                                                    unsafe { &*bs },
                                                    con_out,
                                                    kbuf,
                                                    &bo,
                                                    init_pa,
                                                    init_len,
                                                ) {
                                                    final_boot(
                                                        unsafe { &*bs },
                                                        con_out,
                                                        image_handle,
                                                        hpa,
                                                        &bo,
                                                        init_pa,
                                                        init_len,
                                                    );
                                                }
                                            }
                                            Err(e) => {
                                                print_line(con_out, |l| {
                                                    l.push_str("elf: parse e=");
                                                    l.push_str(boot_err_name(e));
                                                });
                                            }
                                        }
                                    } else {
                                        print_line(con_out, |l| {
                                            l.push_str("kfile: read got=");
                                            l.push_u32_dec(got as u32);
                                            l.push_str(" want=");
                                            l.push_u32_dec(fsize as u32);
                                            l.push_str(" r=0x");
                                            let mut t = [0u8; 16];
                                            l.push(fmt_u64_hex(r_rd as u64, &mut t));
                                        });
                                    }
                                } else {
                                    print_line(con_out, |l| {
                                        l.push_str("kfile: alloc r=0x");
                                        let mut t = [0u8; 16];
                                        l.push(fmt_u64_hex(r_ap as u64, &mut t));
                                        l.push_str(" buf?=");
                                        l.push_u32_dec(kbuf.is_null() as u32);
                                    });
                                }
                            } else {
                                print_line(con_out, |l| {
                                    l.push_str("kfile: size 帽 size=");
                                    l.push_u32_dec(fsize as u32);
                                });
                            }
                        } else {
                            print_line(con_out, |l| {
                                l.push_str("kfile: get_info r=0x");
                                let mut t = [0u8; 16];
                                l.push(fmt_u64_hex(r_gi as u64, &mut t));
                                l.push_str(" len=");
                                l.push_u32_dec(info_len as u32);
                            });
                        }
                        // 判读边界外收句柄（同 HELLO 面纪律）。
                        unsafe { (kfile_ref.close)(kfile) };
                    } else {
                        print_line(con_out, |l| {
                            l.push_str("kfile: open r=0x");
                            let mut t = [0u8; 16];
                            l.push(fmt_u64_hex(r_kopen as u64, &mut t));
                        });
                    }

                    // 根句柄收口（两档读毕）。终段成功路径已在 final_boot
                    // 内 EBS + 跳 entry 永不归此；走到这里的失败路径必在
                    // EBS 前（final_boot 一切返回路径都置 ok=false）——
                    // ConOut/root 仍活，收口合法。`BERRYBOOT ok` 由 EBS 后
                    // 直写 PL011 打出（刀 4：标记 = 含 EBS 全链真过）。
                    unsafe { (root_ref.close)(root) };
                } else {
                    print_line(con_out, |l| {
                        l.push_str("fs: open_volume r=0x");
                        let mut t = [0u8; 16];
                        l.push(fmt_u64_hex(r_vol as u64, &mut t));
                    });
                }
            } else {
                print_line(con_out, |l| {
                    l.push_str("fs: sfs r=0x");
                    let mut t = [0u8; 16];
                    l.push(fmt_u64_hex(r_sfs as u64, &mut t));
                });
            }
        } else {
            print_line(con_out, |l| {
                l.push_str("fs: device null li r=0x");
                let mut t = [0u8; 16];
                l.push(fmt_u64_hex(r_li as u64, &mut t));
            });
        }
    } else {
        print_line(con_out, |l| l.push_str("fs: bs absent"));
    }
    EFI_SUCCESS
}

#[cfg(test)]
mod tests {
    /// 单源纪律：仓库 fixture hello.txt 与内嵌 EXPECTED_HELLO 逐字节一致
    /// （改任一侧忘同步 = 此处炸——mkfat fixture 共识基准同族，A-5 纪律；
    /// 挂 just test-host 前置）。
    #[test]
    fn hello_fixture_consensus() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../hello.txt");
        let disk = std::fs::read(path).expect("读 fixture hello.txt 失败");
        assert_eq!(
            disk.as_slice(),
            super::EXPECTED_HELLO,
            "hello.txt 与 EXPECTED_HELLO 漂移——两侧须同步改"
        );
    }
}
