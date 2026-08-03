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
