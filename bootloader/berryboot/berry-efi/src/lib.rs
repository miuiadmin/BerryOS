//! berry-efi——EFI（UEFI）结构最小面（V-0a 刀 1）。
//!
//! 定位：**纯数据 + 纯函数**（no_std、core-only、零静态、零分配）——host
//! 可测（`cargo test`，cfg(not(test)) 让 harness 用 std），与 berry-fsproto
//! 家族同纪律：跨边界结构全 `#[repr(C)]`，偏移/尺寸用 host 测试钉板（C-ABI
//! 对齐坑的第一道闸），消费端 = berryboot 引导应用。
//!
//! v0 面 = 刀 1 所需：SystemTable 头部 + ConOut 协议 + 状态码 + UTF-16 编码
//! 助手。刀 2/3 再扩（BootServices/HandleProtocol/SimpleFileSystem/GOP）。
//! 只覆盖**用到的**——UEFI 全量 API 面不是本 crate 的目标（人无我有：不做
//! 第二个 uefi-rs，引导链需要什么长什么）。
#![cfg_attr(not(test), no_std)]

use core::ffi::c_void;

/// EFI 状态码（usize——AArch64 调用约定的返回寄存器宽度）。只收用到的。
pub type EfiStatus = usize;

pub const EFI_SUCCESS: EfiStatus = 0;
pub const EFI_LOAD_ERROR: EfiStatus = 1;
pub const EFI_INVALID_PARAMETER: EfiStatus = 2;
pub const EFI_UNSUPPORTED: EfiStatus = 3;
/// GetMemoryMap 缓冲不足——出参 MemoryMapSize 回填所需字节长（V-0b 刀 4：
/// 首探即按此扩容重试，帽两轮）。
pub const EFI_BUFFER_TOO_SMALL: EfiStatus = 5;
pub const EFI_NOT_FOUND: EfiStatus = 14;

/// System Table 签名（"IBI SYST" 的小端 u64 读出——官方定值）。
pub const EFI_SYSTEM_TABLE_SIGNATURE: u64 = 0x5453_5953_2049_4249;

/// 所有 EFI 表共用的 24B 头。
#[repr(C)]
pub struct EfiTableHeader {
    pub signature: u64,
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}

/// SIMPLE_TEXT_OUTPUT_PROTOCOL——ConOut 指向的协议体（10 槽函数表 + mode）。
#[repr(C)]
pub struct EfiSimpleTextOutputProtocol {
    pub reset: unsafe extern "C" fn(this: *mut EfiSimpleTextOutputProtocol,
                                    extended_verification: bool) -> EfiStatus,
    pub output_string: unsafe extern "C" fn(this: *mut EfiSimpleTextOutputProtocol,
                                            string: *const u16) -> EfiStatus,
    pub test_string: unsafe extern "C" fn(this: *mut EfiSimpleTextOutputProtocol,
                                          string: *const u16) -> EfiStatus,
    pub query_mode: unsafe extern "C" fn(this: *mut EfiSimpleTextOutputProtocol,
                                         mode_number: usize,
                                         columns: *mut usize,
                                         rows: *mut usize) -> EfiStatus,
    pub set_mode: unsafe extern "C" fn(this: *mut EfiSimpleTextOutputProtocol,
                                       mode_number: usize) -> EfiStatus,
    pub set_attribute: unsafe extern "C" fn(this: *mut EfiSimpleTextOutputProtocol,
                                            attribute: usize) -> EfiStatus,
    pub clear_screen: unsafe extern "C" fn(this: *mut EfiSimpleTextOutputProtocol) -> EfiStatus,
    pub set_cursor_position: unsafe extern "C" fn(this: *mut EfiSimpleTextOutputProtocol,
                                                  column: usize,
                                                  row: usize) -> EfiStatus,
    pub enable_cursor: unsafe extern "C" fn(this: *mut EfiSimpleTextOutputProtocol,
                                            visible: bool) -> EfiStatus,
    pub mode: *const c_void, // SIMPLE_TEXT_OUTPUT_MODE*（v0 不透明）
}

/// EFI System Table——efi_main 的第二入参。v0 只描到 boot_services；
/// number_of_table_entries/configuration_table 保偏移（刀 2 GOP 用得到）。
#[repr(C)]
pub struct EfiSystemTable {
    pub hdr: EfiTableHeader,
    pub firmware_vendor: *const u16,
    pub firmware_revision: u32,
    _pad0: u32, // 对齐垫（firmware_revision 后自然对齐 usize）——host 测试钉板
    pub console_in_handle: *mut c_void,
    pub con_in: *const c_void, // SIMPLE_TEXT_INPUT_PROTOCOL*（刀 2+ 再描）
    pub console_out_handle: *mut c_void,
    pub con_out: *mut EfiSimpleTextOutputProtocol,
    pub standard_error_handle: *mut c_void,
    pub std_err: *const c_void, // SIMPLE_TEXT_OUTPUT_PROTOCOL*
    pub runtime_services: *mut c_void, // RUNTIME_SERVICES*（v2）
    pub boot_services: *const EfiBootServices,
    pub number_of_table_entries: usize,
    pub configuration_table: *const c_void, // CONFIGURATION_TABLE*
}

/// Boot Services 签名（小端 u64 读出 "BOOTSERV"——官方定值）。
pub const EFI_BOOT_SERVICES_SIGNATURE: u64 = 0x5652_4553_544F_4F42;

/// EFI GUID（16B，C 布局 = u32 + u16 + u16 + u8[8]）。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EfiGuid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

/// GRAPHICS_OUTPUT_PROTOCOL GUID（9042A9DE-23DC-4A38-96FB-7ADED080516A）。
pub const EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x9042_A9DE,
    data2: 0x23DC,
    data3: 0x4A38,
    data4: [0x96, 0xFB, 0x7A, 0xDE, 0xD0, 0x80, 0x51, 0x6A],
};

/// SIMPLE_FILE_SYSTEM_PROTOCOL GUID（964E5B22-6459-11D2-8E39-00A0C969723B）
/// ——刀 3 SFS 定位（挂在引导卷的 DeviceHandle 上）。
pub const EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x964E_5B22,
    data2: 0x6459,
    data3: 0x11D2,
    data4: [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

/// LOADED_IMAGE_PROTOCOL GUID（5B1B31A1-9562-11D2-8E3F-00A0C969723B）——
/// 经 image handle 反查本镜像装载信息（DeviceHandle = SFS 路由起点）。
pub const EFI_LOADED_IMAGE_PROTOCOL_GUID: EfiGuid = EfiGuid {
    data1: 0x5B1B_31A1,
    data2: 0x9562,
    data3: 0x11D2,
    data4: [0x8E, 0x3F, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

/// EFI_FILE_PROTOCOL 的 open_mode 位（只收用到的）。
pub const EFI_FILE_MODE_READ: u64 = 0x0000_0000_0000_0001;

/// AllocatePool 内存类型（EfiLoaderData = 引导应用数据——EBS 后仍有效；
/// V-0b 刀 1 KERNEL.ELF 整档缓冲用）。
pub const EFI_LOADER_DATA: usize = 2;

/// AllocatePages 的分配类型：任意空闲页（V-0b 刀 3 handoff 页用）。
pub const EFI_ALLOCATE_ANY_PAGES: u32 = 0;
/// AllocatePages 的分配类型：**指定地址**认领（V-0b 刀 3 内核 span 用——
/// AllocateAddress 对已占用页返错不覆盖，天然防撞）。
pub const EFI_ALLOCATE_ADDRESS: u32 = 2;

/// EFI_MEMORY_DESCRIPTOR——GetMemoryMap 单条目（V-0b 刀 4）。40B；遍历步长
/// 用出参 DescriptorSize（固件可在尾随加私有垫，**不得按 sizeof 前进**——
/// handoff 把 size/version 原样透传，内核 V-1 消费时同纪律）。
#[repr(C)]
pub struct EfiMemoryDescriptor {
    pub kind: u32,             // EFI_MEMORY_TYPE（7=Conventional 等）
    pub _pad: u32,             // 自然对齐垫（spec 布局，非填充可查）
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

/// BOOT_SERVICES——只类型化用到的入口（allocate_pool（V-0b 刀 1）/
/// handle_protocol（V-0a 刀 3）/ locate_protocol（V-0b 刀 2）/
/// allocate_pages（V-0b 刀 3）/ get_memory_map + exit_boot_services
/// （V-0b 刀 4——EBS 时序是引导器最难段，签名必须描对）），其余槽以具名
/// `*const c_void` 占位（同 SystemTable 的 con_in 纪律：保偏移可审计，
/// 不描无用签名）。376B 全表，host 测试钉板。
#[repr(C)]
pub struct EfiBootServices {
    pub hdr: EfiTableHeader,                                  // @0
    pub raise_tpl: *mut c_void,                               // @24
    pub restore_tpl: *mut c_void,                             // @32
    pub allocate_pages: unsafe extern "C" fn(
        allocate_type: u32,
        memory_type: usize,
        pages: usize,
        memory: *mut u64, // EFI_PHYSICAL_ADDRESS*（出参：分配/认领到的基址）
    ) -> EfiStatus,                                           // @40（V-0b 刀 3）
    pub free_pages: *mut c_void,                              // @48
    pub get_memory_map: unsafe extern "C" fn(
        memory_map_size: *mut usize, // 入参=缓冲字节长，出参=所需/实收字节长
        memory_map: *mut EfiMemoryDescriptor,
        map_key: *mut usize,        // 出参：EBS 一致性键（map 变则键变）
        descriptor_size: *mut usize, // 出参：单描述符步长（≥sizeof，走路用它）
        descriptor_version: *mut u32, // 出参：描述符结构版本（handoff 透传）
    ) -> EfiStatus,                                             // @56（V-0b 刀 4）
    pub allocate_pool: unsafe extern "C" fn(
        pool_type: usize,
        size: usize,
        buffer: *mut *mut c_void,
    ) -> EfiStatus,                                           // @64（V-0b 刀 1）
    pub free_pool: *mut c_void,                               // @72
    pub create_event: *mut c_void,                            // @80
    pub set_timer: *mut c_void,                               // @88
    pub wait_for_event: *mut c_void,                          // @96
    pub signal_event: *mut c_void,                            // @104
    pub close_event: *mut c_void,                             // @112
    pub check_event: *mut c_void,                             // @120
    pub install_protocol_interface: *mut c_void,              // @128
    pub reinstall_protocol_interface: *mut c_void,            // @136
    pub uninstall_protocol_interface: *mut c_void,            // @144
    pub handle_protocol: unsafe extern "C" fn(
        handle: *mut c_void,
        protocol: *mut EfiGuid,
        interface: *mut *mut c_void,
    ) -> EfiStatus,                                           // @152（刀 3）
    pub reserved: *mut c_void,                                // @160
    pub register_protocol_notify: *mut c_void,                // @168
    pub locate_handle: *mut c_void,                           // @176
    pub locate_device_path: *mut c_void,                      // @184
    pub install_configuration_table: *mut c_void,             // @192
    pub load_image: *mut c_void,                              // @200
    pub start_image: *mut c_void,                             // @208
    pub exit: *mut c_void,                                    // @216
    pub unload_image: *mut c_void,                            // @224
    pub exit_boot_services: unsafe extern "C" fn(
        image_handle: *mut c_void, // efi_main 首参原样回传
        map_key: usize,            // GetMemoryMap 出参原样回传——变则拒
    ) -> EfiStatus,                                             // @232（V-0b 刀 4）
    pub get_next_monotonic_count: *mut c_void,                // @240
    pub stall: *mut c_void,                                   // @248
    pub set_watchdog_timer: unsafe extern "C" fn(
        timeout: usize,          // 0 = 关（默认 5 分钟会重启整机——内核长跑必关）
        watchdog_code: u64,
        data_size: usize,
        watchdog_data: *const c_void,
    ) -> EfiStatus,                                              // @256（V-0b 刀 4）
    pub connect_controller: *mut c_void,                      // @264
    pub disconnect_controller: *mut c_void,                   // @272
    pub open_protocol: *mut c_void,                           // @280
    pub close_protocol: *mut c_void,                          // @288
    pub open_protocol_information: *mut c_void,               // @296
    pub protocols_per_handle: *mut c_void,                    // @304
    pub locate_handle_buffer: *mut c_void,                    // @312
    pub locate_protocol: unsafe extern "C" fn(
        protocol: *mut EfiGuid,
        registration: *mut c_void,
        interface: *mut *mut c_void,
    ) -> EfiStatus,                                           // @320（刀 2）
    pub install_multiple_protocol_interfaces: *mut c_void,    // @328
    pub uninstall_multiple_protocol_interfaces: *mut c_void,  // @336
    pub calculate_crc32: *mut c_void,                         // @344
    pub copy_mem: *mut c_void,                                // @352
    pub set_mem: *mut c_void,                                 // @360
    pub create_event_ex: *mut c_void,                         // @368
}

/// GRAPHICS_OUTPUT_PROTOCOL（裁决 C：只 QueryMode 信息行，不画像素）。
#[repr(C)]
pub struct EfiGraphicsOutputProtocol {
    pub query_mode: unsafe extern "C" fn(
        this: *mut EfiGraphicsOutputProtocol,
        mode_number: usize,
        size_of_info: *mut usize,
        info: *mut *mut EfiGraphicsOutputModeInfo,
    ) -> EfiStatus,                                           // @0
    pub set_mode: *mut c_void,                                // @8
    pub blt: *mut c_void,                                     // @16
    pub mode: *const EfiGraphicsOutputMode,                   // @24
}

/// GOP 当前模式快照（mode 指针指向的 40B）。
#[repr(C)]
pub struct EfiGraphicsOutputMode {
    pub max_mode: u32,                          // @0
    pub mode: u32,                              // @4（当前模式号）
    pub info: *const EfiGraphicsOutputModeInfo, // @8
    pub size_of_info: usize,                    // @16
    pub frame_buffer_base: u64,                 // @24
    pub frame_buffer_size: usize,               // @32
}

/// QueryMode 出的模式信息（36B，全 u32——像素格式 0=RGBr/1=BGRr/2=位掩码/
/// 3=仅 Blt）。
#[repr(C)]
pub struct EfiGraphicsOutputModeInfo {
    pub version: u32,               // @0
    pub horizontal_resolution: u32, // @4
    pub vertical_resolution: u32,   // @8
    pub pixel_format: u32,          // @12
    pub pixel_information: [u32; 4], // @16
    pub pixels_per_scan_line: u32,  // @32
}

/// SIMPLE_FILE_SYSTEM_PROTOCOL（16B）——OpenVolume 出根目录文件句柄。
#[repr(C)]
pub struct EfiSimpleFileSystemProtocol {
    pub revision: u64, // @0
    pub open_volume: unsafe extern "C" fn(
        this: *mut EfiSimpleFileSystemProtocol,
        root: *mut *mut EfiFileProtocol,
    ) -> EfiStatus, // @8
}

/// EFI_FILE_PROTOCOL（88B 全表）——open/close/read 类型化（刀 3 用面），
/// 其余槽具名 `*mut c_void` 占位（同 EfiBootServices 纪律：保偏移可审计）。
#[repr(C)]
pub struct EfiFileProtocol {
    pub revision: u64, // @0
    pub open: unsafe extern "C" fn(
        this: *mut EfiFileProtocol,
        new_handle: *mut *mut EfiFileProtocol,
        filename: *const u16,
        open_mode: u64,
        attributes: u64,
    ) -> EfiStatus, // @8
    pub close: unsafe extern "C" fn(this: *mut EfiFileProtocol) -> EfiStatus, // @16
    pub delete: *mut c_void,        // @24
    pub read: unsafe extern "C" fn(
        this: *mut EfiFileProtocol,
        buffer_size: *mut usize,
        buffer: *mut c_void,
    ) -> EfiStatus, // @32
    pub write: *mut c_void,        // @40
    pub get_position: *mut c_void, // @48
    pub set_position: *mut c_void, // @56
    pub get_info: unsafe extern "C" fn(
        this: *mut EfiFileProtocol,
        information_type: *const EfiGuid,
        buffer_size: *mut usize,
        buffer: *mut c_void,
    ) -> EfiStatus, // @64（V-0b 刀 1）
    pub set_info: *mut c_void,     // @72
    pub flush: *mut c_void,        // @80
}

/// LOADED_IMAGE_PROTOCOL——v0 只描到 device_handle @24（32B 截断，后续
/// 阶段按需再描；保偏移可审计纪律同 EfiSystemTable 的 con_in 注）。
#[repr(C)]
pub struct EfiLoadedImageProtocol {
    pub revision: u32,              // @0
    _pad0: u32,                     // @4（usize 对齐垫）
    pub parent_handle: *mut c_void, // @8
    pub system_table: *mut c_void,  // @16（*mut EfiSystemTable）
    pub device_handle: *mut c_void, // @24——SFS 定位路由的目标
}

/// EFI_FILE_INFO GUID（09576E92-6D3F-11D2-8E39-00A0C969723B）——GetInfo
/// 查档长（V-0b 刀 1：KERNEL.ELF 整档读入先探 file_size）。
pub const EFI_FILE_INFO_GUID: EfiGuid = EfiGuid {
    data1: 0x0957_6E92,
    data2: 0x6D3F,
    data3: 0x11D2,
    data4: [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B],
};

/// EFI_TIME（16B 定长——FILE_INFO 内三枚时间戳的排面）。
#[repr(C)]
pub struct EfiTime {
    pub year: u16,       // @0
    pub month: u8,       // @2
    pub day: u8,         // @3
    pub hour: u8,        // @4
    pub minute: u8,      // @5
    pub second: u8,      // @6
    _pad1: u8,           // @7
    pub nanosecond: u32, // @8
    pub timezone: u16,   // @12
    pub daylight: u8,    // @14
    _pad2: u8,           // @15
}

/// FILE_INFO——GetInfo(EFI_FILE_INFO) 出参（@80 起变长 UCS-2 名；定长面
/// 88B 含 name[1]，host 测试钉板）。size = 整结构含名的字节数。
#[repr(C)]
pub struct EfiFileInfo {
    pub size: u64,                 // @0
    pub file_size: u64,            // @8——刀 1 消费面（整档 Read 的长度权威）
    pub physical_size: u64,        // @16
    pub create_time: EfiTime,      // @24
    pub last_access_time: EfiTime, // @40
    pub modification_time: EfiTime, // @56
    pub attribute: u64,            // @72
    pub name: [u16; 1],            // @80（变长首槽）
}

/// UTF-16 → ASCII 有损解码：`src` 逐 u16 映射进 `out`（≥0x80 → '?'，
/// 与 utf16_encode 的 BMP 外策略对称）。返回写满的子切片。纯函数。
pub fn utf16_to_ascii<'a>(src: &[u16], out: &'a mut [u8]) -> &'a [u8] {
    let mut n = 0;
    for &u in src {
        if n >= out.len() {
            break;
        }
        out[n] = if u < 0x80 { u as u8 } else { b'?' };
        n += 1;
    }
    &out[..n]
}

/// u32 十进制（无前导零，0 → "0"）。容量不足即截断（引导语境纪律同
/// utf16_encode：宁可截断不可炸）。
pub fn fmt_u32_dec<'a>(v: u32, out: &'a mut [u8]) -> &'a [u8] {
    let mut digits = [0u8; 10]; // u32::MAX = 10 位
    let mut n = 0;
    let mut v = v;
    loop {
        digits[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let m = n.min(out.len());
    for i in 0..m {
        out[i] = digits[n - 1 - i];
    }
    &out[..m]
}

/// u64 十六进制小写（无前导零，0 → "0"；"0x" 前缀由调用方拼）。
pub fn fmt_u64_hex<'a>(v: u64, out: &'a mut [u8]) -> &'a [u8] {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut digits = [0u8; 16]; // u64::MAX = 16 位
    let mut n = 0;
    let mut v = v;
    loop {
        digits[n] = HEX[(v & 0xF) as usize];
        n += 1;
        v >>= 4;
        if v == 0 {
            break;
        }
    }
    let m = n.min(out.len());
    for i in 0..m {
        out[i] = digits[n - 1 - i];
    }
    &out[..m]
}

/// UTF-16（UCS-2 子集）编码：`s` 逐字符编进 `out`，返回写满的子切片。
/// BMP 外字符替换 `'?'`（v0 引导横幅全 ASCII；不 panic——边界行为钉板在
/// host 测试）。越界即截断（引导语境宁可截断不可炸）。
pub fn utf16_encode<'a>(s: &str, out: &'a mut [u16]) -> &'a mut [u16] {
    let mut n = 0;
    for ch in s.chars() {
        if n >= out.len() {
            break;
        }
        let cp = ch as u32;
        out[n] = if cp <= 0xFFFF { cp as u16 } else { b'?' as u16 };
        n += 1;
    }
    &mut out[..n]
}

// —— host 钉板测试：偏移/尺寸/签名/编码行为（C-ABI 对齐坑的第一道闸）。
#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    macro_rules! off {
        ($ptr:expr, $field:ident) => {
            unsafe { (core::ptr::addr_of!((*$ptr).$field) as usize) - ($ptr as usize) }
        };
    }

    #[test]
    fn table_sizes() {
        assert_eq!(size_of::<EfiTableHeader>(), 24);
        assert_eq!(size_of::<EfiSimpleTextOutputProtocol>(), 80);
        assert_eq!(size_of::<EfiSystemTable>(), 120);
        assert_eq!(size_of::<EfiGuid>(), 16);
        assert_eq!(size_of::<EfiBootServices>(), 376);
        assert_eq!(size_of::<EfiGraphicsOutputProtocol>(), 32);
        assert_eq!(size_of::<EfiGraphicsOutputMode>(), 40);
        assert_eq!(size_of::<EfiGraphicsOutputModeInfo>(), 36);
        assert_eq!(size_of::<EfiSimpleFileSystemProtocol>(), 16);
        assert_eq!(size_of::<EfiFileProtocol>(), 88);
        assert_eq!(size_of::<EfiLoadedImageProtocol>(), 32);
        assert_eq!(size_of::<EfiMemoryDescriptor>(), 40);
    }

    #[test]
    fn system_table_offsets() {
        let p = 0x1000usize as *const EfiSystemTable;
        assert_eq!(off!(p, firmware_vendor), 24);
        assert_eq!(off!(p, firmware_revision), 32);
        assert_eq!(off!(p, console_in_handle), 40);
        assert_eq!(off!(p, con_in), 48);
        assert_eq!(off!(p, con_out), 64);
        assert_eq!(off!(p, std_err), 80);
        assert_eq!(off!(p, runtime_services), 88);
        assert_eq!(off!(p, boot_services), 96);
        assert_eq!(off!(p, number_of_table_entries), 104);
        assert_eq!(off!(p, configuration_table), 112);
    }

    #[test]
    fn conout_offsets() {
        let p = 0x1000usize as *const EfiSimpleTextOutputProtocol;
        assert_eq!(off!(p, reset), 0);
        assert_eq!(off!(p, output_string), 8);
        assert_eq!(off!(p, query_mode), 24);
        assert_eq!(off!(p, mode), 72);
    }

    #[test]
    fn signature_value() {
        // 小端逐字节应读出 "IBI SYST"。
        let b = EFI_SYSTEM_TABLE_SIGNATURE.to_le_bytes();
        assert_eq!(&b, b"IBI SYST");
        // Boot Services 同款：小端读出 "BOOTSERV"。
        assert_eq!(&EFI_BOOT_SERVICES_SIGNATURE.to_le_bytes(), b"BOOTSERV");
    }

    #[test]
    fn boot_services_offsets() {
        let p = 0x1000usize as *const EfiBootServices;
        assert_eq!(off!(p, allocate_pages), 40);
        assert_eq!(off!(p, get_memory_map), 56);
        assert_eq!(off!(p, allocate_pool), 64);
        assert_eq!(off!(p, handle_protocol), 152);
        assert_eq!(off!(p, exit_boot_services), 232);
        assert_eq!(off!(p, set_watchdog_timer), 256);
        assert_eq!(off!(p, locate_protocol), 320);
        assert_eq!(off!(p, create_event_ex), 368);
    }

    #[test]
    fn memory_descriptor_layout() {
        // 40B + 字段偏移钉板（kind@0 … attribute@32）——handoff memmap_* 透传
        // 面 + 内核 V-1 消费面共同的语言；漂移编译期/测试期炸响。
        let d = 0x1000usize as *const EfiMemoryDescriptor;
        assert_eq!(size_of::<EfiMemoryDescriptor>(), 40);
        assert_eq!(off!(d, kind), 0);
        assert_eq!(off!(d, physical_start), 8);
        assert_eq!(off!(d, virtual_start), 16);
        assert_eq!(off!(d, number_of_pages), 24);
        assert_eq!(off!(d, attribute), 32);
    }

    #[test]
    fn gop_offsets() {
        let g = 0x1000usize as *const EfiGraphicsOutputProtocol;
        assert_eq!(off!(g, query_mode), 0);
        assert_eq!(off!(g, set_mode), 8);
        assert_eq!(off!(g, blt), 16);
        assert_eq!(off!(g, mode), 24);
        let m = 0x1000usize as *const EfiGraphicsOutputMode;
        assert_eq!(off!(m, max_mode), 0);
        assert_eq!(off!(m, mode), 4);
        assert_eq!(off!(m, info), 8);
        assert_eq!(off!(m, size_of_info), 16);
        assert_eq!(off!(m, frame_buffer_base), 24);
        assert_eq!(off!(m, frame_buffer_size), 32);
        let i = 0x1000usize as *const EfiGraphicsOutputModeInfo;
        assert_eq!(off!(i, version), 0);
        assert_eq!(off!(i, horizontal_resolution), 4);
        assert_eq!(off!(i, vertical_resolution), 8);
        assert_eq!(off!(i, pixel_format), 12);
        assert_eq!(off!(i, pixel_information), 16);
        assert_eq!(off!(i, pixels_per_scan_line), 32);
    }

    #[test]
    fn gop_guid_bytes() {
        let g = EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID;
        assert_eq!(g.data1, 0x9042A9DE);
        assert_eq!(g.data2, 0x23DC);
        assert_eq!(g.data3, 0x4A38);
        assert_eq!(g.data4, [0x96, 0xFB, 0x7A, 0xDE, 0xD0, 0x80, 0x51, 0x6A]);
        // 小端整读：data1 在线上的首 4B（协议面 LocateProtocol 对拍的基准）。
        assert_eq!(g.data1.to_le_bytes(), [0xDE, 0xA9, 0x42, 0x90]);
    }

    #[test]
    fn sfs_and_file_offsets() {
        let s = 0x1000usize as *const EfiSimpleFileSystemProtocol;
        assert_eq!(off!(s, revision), 0);
        assert_eq!(off!(s, open_volume), 8);
        let f = 0x1000usize as *const EfiFileProtocol;
        assert_eq!(off!(f, revision), 0);
        assert_eq!(off!(f, open), 8);
        assert_eq!(off!(f, close), 16);
        assert_eq!(off!(f, delete), 24);
        assert_eq!(off!(f, read), 32);
        assert_eq!(off!(f, write), 40);
        assert_eq!(off!(f, get_position), 48);
        assert_eq!(off!(f, set_position), 56);
        assert_eq!(off!(f, get_info), 64);
        assert_eq!(off!(f, set_info), 72);
        assert_eq!(off!(f, flush), 80);
        let li = 0x1000usize as *const EfiLoadedImageProtocol;
        assert_eq!(off!(li, revision), 0);
        assert_eq!(off!(li, parent_handle), 8);
        assert_eq!(off!(li, system_table), 16);
        assert_eq!(off!(li, device_handle), 24);
    }

    #[test]
    fn sfs_and_loaded_image_guid_bytes() {
        let s = EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID;
        assert_eq!(s.data1, 0x964E5B22);
        assert_eq!(s.data4, [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B]);
        assert_eq!(s.data1.to_le_bytes(), [0x22, 0x5B, 0x4E, 0x96]);
        let li = EFI_LOADED_IMAGE_PROTOCOL_GUID;
        assert_eq!(li.data1, 0x5B1B31A1);
        assert_eq!(li.data4, [0x8E, 0x3F, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B]);
        assert_eq!(li.data1.to_le_bytes(), [0xA1, 0x31, 0x1B, 0x5B]);
    }

    #[test]
    fn file_info_guid_and_layout() {
        let g = EFI_FILE_INFO_GUID;
        assert_eq!(g.data1, 0x09576E92);
        assert_eq!(g.data2, 0x6D3F);
        assert_eq!(g.data3, 0x11D2);
        assert_eq!(g.data4, [0x8E, 0x39, 0x00, 0xA0, 0xC9, 0x69, 0x72, 0x3B]);
        assert_eq!(g.data1.to_le_bytes(), [0x92, 0x6E, 0x57, 0x09]);
        assert_eq!(core::mem::size_of::<EfiTime>(), 16);
        assert_eq!(core::mem::size_of::<EfiFileInfo>(), 88);
        let i = 0x1000usize as *const EfiFileInfo;
        assert_eq!(off!(i, size), 0);
        assert_eq!(off!(i, file_size), 8);
        assert_eq!(off!(i, physical_size), 16);
        assert_eq!(off!(i, create_time), 24);
        assert_eq!(off!(i, last_access_time), 40);
        assert_eq!(off!(i, modification_time), 56);
        assert_eq!(off!(i, attribute), 72);
        assert_eq!(off!(i, name), 80);
    }

    #[test]
    fn ascii_and_number_fmts() {
        let mut b = [0u8; 16];
        assert_eq!(utf16_to_ascii(&[0x41, 0x4E2D, 0x42], &mut b), b"A?B"); // CJK → '?'
        assert_eq!(utf16_to_ascii(&[], &mut b), b"");
        // 截断：3 槽收 5 字。
        let mut small = [0u8; 3];
        assert_eq!(utf16_to_ascii(&[0x41, 0x42, 0x43, 0x44, 0x45], &mut small), b"ABC");

        let mut d = [0u8; 12];
        assert_eq!(fmt_u32_dec(0, &mut d), b"0");
        assert_eq!(fmt_u32_dec(42, &mut d), b"42");
        assert_eq!(fmt_u32_dec(u32::MAX, &mut d), b"4294967295");
        let mut s = [0u8; 2];
        assert_eq!(fmt_u32_dec(12345, &mut s), b"12"); // 截断保头

        let mut h = [0u8; 20];
        assert_eq!(fmt_u64_hex(0, &mut h), b"0");
        assert_eq!(fmt_u64_hex(0xDEAD_BEEF, &mut h), b"deadbeef");
        assert_eq!(fmt_u64_hex(u64::MAX, &mut h), b"ffffffffffffffff");
        let mut hs = [0u8; 3];
        assert_eq!(fmt_u64_hex(0xAB_CDEF, &mut hs), b"abc"); // 截断保头
    }

    #[test]
    fn utf16_faces() {
        let mut buf = [0u16; 8];
        assert_eq!(utf16_encode("ABC", &mut buf), &[65, 66, 67]);
        assert_eq!(utf16_encode("", &mut buf), &[] as &[u16]);
        // BMP 内非 ASCII（CJK）原码点。
        assert_eq!(utf16_encode("\u{4E2D}", &mut buf), &[0x4E2D]);
        // BMP 外（SMP 螃蟹）→ '?' 替换（v0 不做代理对）。
        assert_eq!(utf16_encode("\u{1F980}", &mut buf), &['?' as u16]);
        // 越界截断：4 槽收 6 字符。
        let mut small = [0u16; 4];
        assert_eq!(utf16_encode("ABCDEF", &mut small), &[65, 66, 67, 68]);
    }
}
