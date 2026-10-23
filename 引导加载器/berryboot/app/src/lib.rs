//! berryboot——BerryOS 引导应用（V-0a 刀 2：横幅增强——固件版本行 +
//! GOP QueryMode 信息行）。
//!
//! 形态：`crate-type=staticlib` → rust-lld 静态全链接（berryboot.ld 单 .img
//! @0x1000 + --image-base 0 + --emit-relocs）→ 工具链/elf2pe.py 转 PE32+
//! （AArch64 EFI application）→ mkfat --put 造 \EFI\BOOT\BOOTAA64.EFI 引导盘
//! → QEMU AAVMF 可移介质默认路径自动拉起。
//!
//! 刀 2 面：横幅三行——骨架行（刀 1）+ `fw:` 固件厂商/修订行（SystemTable
//! 直读，UCS-2 → ASCII 有损）+ `gop:` 模式信息行（BootServices
//! LocateProtocol(GOP) → QueryMode 当前模式——裁决 C：只信息行不画像素）。
//! 刀 3 加 SFS 开卷读 HELLO.TXT 打 BERRYBOOT ok。
//!
//! 纪律：引导链零 panic（panic handler = 死循环）；一切协议调用失败打
//! **状态码诊断行**（机器可读缺席报告——AI 测试友好：`locate fail r=0x…e`
//! = EFI_NOT_FOUND 即「此固件配置无 GOP 实例」），不炸链；横幅首行本身
//! 即「镜像被执行」的阳证。
#![no_std]

use berry_efi::{
    fmt_u32_dec, fmt_u64_hex, utf16_encode, utf16_to_ascii, EfiGraphicsOutputModeInfo,
    EfiGraphicsOutputProtocol, EfiStatus, EfiSystemTable, EFI_BOOT_SERVICES_SIGNATURE,
    EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID, EFI_SUCCESS, EFI_SYSTEM_TABLE_SIGNATURE,
};
use core::ffi::c_void;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// 刀 1 骨架行：ASCII + CRLF（EFI ConOut 惯例——部分终端只认 CRLF）。
const BANNER: &str = "BerryBoot EFI skeleton (V-0a)\r\n";

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
    _image_handle: *const c_void,
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
    EFI_SUCCESS
}
