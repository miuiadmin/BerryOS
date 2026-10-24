//! 最小 MMIO 助手（unsafe 核，外层包安全 API）。

/// 向 `addr` 写一个 32 位字。
#[inline]
pub unsafe fn write(addr: usize, val: u32) {
    (addr as *mut u32).write_volatile(val);
}

/// 从 `addr` 读一个 32 位字。
#[allow(dead_code)]
#[inline]
pub unsafe fn read(addr: usize) -> u32 {
    (addr as *const u32).read_volatile()
}

/// 向 `addr` 写一个字节（GICD IPRIORITYR/ITARGETSR 每 IRQ 一字节）。
#[inline]
pub unsafe fn write_u8(addr: usize, val: u8) {
    (addr as *mut u8).write_volatile(val);
}

/// 从 `addr` 读一个字节。
#[inline]
pub unsafe fn read_u8(addr: usize) -> u8 {
    (addr as *const u8).read_volatile()
}

/// 向 `addr` 写一个 16 位字（fw_cfg selector 等 u16 MMIO 口）。
#[inline]
pub unsafe fn write_u16(addr: usize, val: u16) {
    (addr as *mut u16).write_volatile(val);
}
