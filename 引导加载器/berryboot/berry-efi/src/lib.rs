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
    pub boot_services: *mut c_void,    // BOOT_SERVICES*（刀 3 描）
    pub number_of_table_entries: usize,
    pub configuration_table: *const c_void, // CONFIGURATION_TABLE*
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
