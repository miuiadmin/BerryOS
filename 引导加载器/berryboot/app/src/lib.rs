//! berryboot——BerryOS 引导应用（V-0a 刀 1：EFI 骨架 + 单行横幅）。
//!
//! 形态：`crate-type=staticlib` → rust-lld 静态全链接（berryboot.ld 单 .img
//! @0x1000 + --image-base 0 + --emit-relocs）→ 工具链/elf2pe.py 转 PE32+
//! （AArch64 EFI application）→ mkfat --put 造 \EFI\BOOT\BOOTAA64.EFI 引导盘
//! → QEMU AAVMF 可移介质默认路径自动拉起。
//!
//! 刀 1 面：efi_main 解 SystemTable → ConOut 横幅一行 → EFI_SUCCESS 返回
//! （固件续行——串口可见横幅即「镜像被 DXE 接受且执行」的阳证）。刀 2 加
//! 固件版本行 + GOP 信息行；刀 3 加 SFS 开卷读 HELLO.TXT 打 BERRYBOOT ok。
//!
//! 纪律：引导链零 panic（panic handler = 死循环 + 横幅已打则可见）；一切
//! 协议调用失败只影响对应信息行，不炸链。
#![no_std]

use berry_efi::{utf16_encode, EfiStatus, EfiSystemTable, EFI_SUCCESS};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// 刀 1 横幅：ASCII、含 \r\n（EFI ConOut 惯例——部分终端只认 CRLF）。
const BANNER: &str = "BerryBoot EFI skeleton (V-0a)\r\n";

#[no_mangle]
pub extern "C" fn efi_main(
    _image_handle: *const core::ffi::c_void,
    system_table: *mut EfiSystemTable,
) -> EfiStatus {
    // 引导语境的防御式：表指针空/签名不符 → 静默成功返回（刀 1 无断言面，
    // 横幅本身即验收标记——打出 = 全链通）。
    if !system_table.is_null() {
        unsafe {
            let st = &*system_table;
            if st.hdr.signature == berry_efi::EFI_SYSTEM_TABLE_SIGNATURE
                && !st.con_out.is_null()
            {
                let mut wbuf = [0u16; 64];
                let s = utf16_encode(BANNER, &mut wbuf);
                ((*st.con_out).output_string)(st.con_out, s.as_ptr());
            }
        }
    }
    EFI_SUCCESS
}
