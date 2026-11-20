//! compat-wayland-bridge——G2 刀 9：Linux GUI Wayland 协议→BerryOS 桥接。
//!
//! 架构：
//! - **wl_surface → SurfaceId**：每个 Linux wl_surface 映射到 BerryOS
//!   compositor 的 SurfaceId（u32 命名空间，独立计数不与原生面冲突）。
//! - **wl_shm → 共享 VMO**：wl_shm_pool 的 mmap 基址映射为 BerryOS
//!   共享 VMO（guest 写→host 读零拷贝——wl_shm 本身就是 SHM 协议）。
//! - **wl_buffer attach/commit → BerryOS 帧提交**：attach 将 buffer 关联
//!   到 surface；commit 触发 BerryOS compositor 重绘通知。
//! - **输入桥**：BerryOS input 事件→wl_pointer/wl_keyboard 协议帧。
//!
//! 纯面纪律：对象模型/ID 映射/几何计算 = 零 I/O 纯函数（host 可测）；
//! 实际 VMO 创建/surface 注册由 berry-libc 运行时承接。
#![cfg_attr(not(test), no_std)]

/// Wayland 接口名→接口号的静态注册表（wl_registry bind 用）。
pub mod iface {
    pub const WL_DISPLAY: u32 = 1;
    pub const WL_REGISTRY: u32 = 2;
    pub const WL_CALLBACK: u32 = 3;
    pub const WL_COMPOSITOR: u32 = 4;
    pub const WL_SHM: u32 = 5;
    pub const WL_BUFFER: u32 = 6;
    pub const WL_SURFACE: u32 = 7;
    pub const WL_SEAT: u32 = 8;
    pub const WL_POINTER: u32 = 9;
    pub const WL_KEYBOARD: u32 = 10;
    pub const WL_OUTPUT: u32 = 11;

    /// 接口名→接口号（字符串查找）。
    pub fn lookup(name: &str) -> Option<u32> {
        match name {
            "wl_display" => Some(WL_DISPLAY),
            "wl_registry" => Some(WL_REGISTRY),
            "wl_callback" => Some(WL_CALLBACK),
            "wl_compositor" => Some(WL_COMPOSITOR),
            "wl_shm" => Some(WL_SHM),
            "wl_buffer" => Some(WL_BUFFER),
            "wl_surface" => Some(WL_SURFACE),
            "wl_seat" => Some(WL_SEAT),
            "wl_pointer" => Some(WL_POINTER),
            "wl_keyboard" => Some(WL_KEYBOARD),
            "wl_output" => Some(WL_OUTPUT),
            _ => None,
        }
    }

    /// 全局广播列表（wl_registry.global 事件的 payloads）。
    pub const GLOBALS: &[(&str, u32, &str)] = &[
        ("wl_compositor", 5, "wl_compositor"),
        ("wl_shm", 1, "wl_shm"),
        ("wl_seat", 7, "wl_seat"),
        ("wl_output", 4, "wl_output"),
    ];
}

/// wl_shm 格式常量（wl_shm.format 枚举——drm_fourcc 子集）。
pub mod shm_format {
    pub const ARGB8888: u32 = 0; // 32bpp ARGB
    pub const XRGB8888: u32 = 1; // 32bpp XRGB（alpha 忽略）
    pub const ABGR8888: u32 = 0x34324241; // little-endian ABGR
    pub const XBGR8888: u32 = 0x34324258; // little-endian XBGR
}

/// Surface 映射表——wl_surface 对象 ID → BerryOS SurfaceId。
pub struct SurfaceMap {
    /// wl 新对象 ID（客户端分配，从 1 起）。
    next_wl_id: u32,
    /// 下一 BerryOS SurfaceId（桥接分配，从 0x10000 起避开原生面）。
    next_surface_id: u32,
    /// 活跃映射：(wl_id, surface_id)。
    active: [(u32, u32); 32], // 定容——v1 域面
    count: usize,
}

impl SurfaceMap {
    pub const BRIDGE_SURFACE_BASE: u32 = 0x0001_0000;
    pub const MAX_SURFACES: usize = 32;

    pub fn new() -> SurfaceMap {
        SurfaceMap {
            next_wl_id: 1,
            next_surface_id: Self::BRIDGE_SURFACE_BASE,
            active: [(0, 0); Self::MAX_SURFACES],
            count: 0,
        }
    }

    /// 创建新 wl_surface→BerryOS SurfaceId 映射。
    /// 返回 (wl_id, surface_id)；满则 None。
    pub fn create(&mut self) -> Option<(u32, u32)> {
        if self.count >= Self::MAX_SURFACES {
            return None;
        }
        let wl = self.next_wl_id;
        let sid = self.next_surface_id;
        self.next_wl_id += 1;
        self.next_surface_id += 1;
        self.active[self.count] = (wl, sid);
        self.count += 1;
        Some((wl, sid))
    }

    /// wl_id → SurfaceId 查找。
    pub fn lookup(&self, wl_id: u32) -> Option<u32> {
        self.active[..self.count]
            .iter()
            .find(|(w, _)| *w == wl_id)
            .map(|(_, s)| *s)
    }

    /// 当前活跃 surface 数。
    pub fn count(&self) -> usize {
        self.count
    }
}

/// SHM buffer 描述——wl_buffer 与 wl_shm_pool 的桥接面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShmBuffer {
    /// wl_shm_pool 中的字节偏移。
    pub offset: u32,
    /// 宽度（像素）。
    pub width: u32,
    /// 高度（像素）。
    pub height: u32,
    /// 步长（字节/行）。
    pub stride: u32,
    /// wl_shm.format。
    pub format: u32,
}

impl ShmBuffer {
    /// 验证 buffer 几何——越界/零尺寸/不支持格式 → None。
    pub fn validate(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.stride >= self.width * 4 // 至少 4bpp
            && matches!(self.format, shm_format::ARGB8888 | shm_format::XRGB8888)
    }

    /// 总字节长（offset 之后到 buffer 尾）。
    pub fn byte_len(&self) -> usize {
        self.offset as usize + (self.stride as usize) * (self.height as usize)
    }
}

/// 输入桥——BerryOS 事件→Wayland 协议帧的分类型。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputBridge {
    /// pointer motion——(surface_id, x, y)。
    PointerMotion { sid: u32, x: f32, y: f32 },
    /// pointer button——(surface_id, button, state)。
    PointerButton { sid: u32, button: u32, down: bool },
    /// keyboard key——(keycode, down).
    KeyboardKey { keycode: u32, down: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iface_lookup() {
        assert_eq!(iface::lookup("wl_display"), Some(iface::WL_DISPLAY));
        assert_eq!(iface::lookup("wl_compositor"), Some(iface::WL_COMPOSITOR));
        assert_eq!(iface::lookup("wl_nonexistent"), None);
    }

    #[test]
    fn globals_nonempty() {
        assert!(iface::GLOBALS.len() >= 3, "至少 compositor/shm/seat");
    }

    #[test]
    fn surface_map_create_lookup() {
        let mut m = SurfaceMap::new();
        let (wl1, s1) = m.create().unwrap();
        let (wl2, s2) = m.create().unwrap();
        assert_eq!(m.lookup(wl1), Some(s1));
        assert_eq!(m.lookup(wl2), Some(s2));
        assert_ne!(s1, s2);
        assert_eq!(m.count(), 2);
        assert!(s1 >= SurfaceMap::BRIDGE_SURFACE_BASE);
    }

    #[test]
    fn surface_map_cap() {
        let mut m = SurfaceMap::new();
        for _ in 0..SurfaceMap::MAX_SURFACES {
            assert!(m.create().is_some());
        }
        assert!(m.create().is_none(), "满 32 后拒");
    }

    #[test]
    fn shm_buffer_validate() {
        let ok = ShmBuffer { offset: 0, width: 100, height: 50, stride: 400, format: shm_format::ARGB8888 };
        assert!(ok.validate());
        assert_eq!(ok.byte_len(), 20000);

        let bad_fmt = ShmBuffer { format: 0xDEAD, ..ok };
        assert!(!bad_fmt.validate());

        let zero_w = ShmBuffer { width: 0, ..ok };
        assert!(!zero_w.validate());

        let short_stride = ShmBuffer { stride: 100, ..ok };
        assert!(!short_stride.validate());
    }
}
