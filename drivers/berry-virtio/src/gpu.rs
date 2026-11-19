//! virtio-gpu 2D 控制命令编解码（GPU Phase 1，virtio 规范 §5.10；
//! wire 形态与 Linux UAPI `include/uapi/linux/virtio_gpu.h` 逐字段对拍）。
//!
//! 消费者规划 = `vgpud`（drivers/bins，后续刀）：ctrlq 上的 2D 命令与
//! `RESP_OK_DISPLAY_INFO` 应答经本模块单源编码/解码。volatile MMIO、队列
//! 协商与 DMA 仍归驱动本体；本模块保持 crate 纪律——零分配、零 unsafe、
//! 零依赖，编不出就拒绝 None，不截断。
//!
//! wire 事实：
//! - 控制头恰 24B 全 LE：`type u32 + flags u32 + fence_id u64 + ctx_id u32 +
//!   ring_idx u8 + padding[3]`。v1 2D 驱动 flags/fence/ctx/ring 全零。
//! - `rect` 恰 16B：x/y/width/height 四 u32。
//! - `GET_DISPLAY_INFO` 请求 = 头；应答 = 头 + 16 个 display mode，每 mode
//!   24B（rect16 + enabled u32 + flags u32），总幅 408B。
//! - `RESOURCE_CREATE_2D` = 40B；`SET_SCANOUT` / `RESOURCE_FLUSH` = 40B；
//!   `TRANSFER_TO_HOST_2D` = 56B；ATTACH/DETACH_BACKING 命令 = 32B，
//!   backing `mem_entry` 独立恰 16B。
//! - config 恰 20B：events_read/events_clear/num_scanouts/num_capsets/
//!   blob_alignment 五 u32（v1 消费前三项，后两项只解码不解释）。

use crate::{get_u32, get_u64, put_u32, put_u64};

/// 控制头幅（type/flags/fence/ctx/ring+padding）。
pub const GPU_CTRL_HDR_SZ: usize = 24;
/// 矩形幅（x/y/w/h 四 u32）。
pub const GPU_RECT_SZ: usize = 16;
/// 单 scanout mode 幅（rect + enabled + flags）。
pub const GPU_DISPLAY_MODE_SZ: usize = 24;
/// 规范 scanout 数帽。
pub const GPU_MAX_SCANOUTS: usize = 16;
/// GET_DISPLAY_INFO 应答总幅。
pub const GPU_DISPLAY_INFO_SZ: usize = GPU_CTRL_HDR_SZ + GPU_MAX_SCANOUTS * GPU_DISPLAY_MODE_SZ;
/// 2D resource create 命令幅。
pub const GPU_RESOURCE_CREATE_2D_SZ: usize = 40;
/// set-scanout 命令幅（头 24 + rect 16 + scanout/resource 8）。
pub const GPU_SET_SCANOUT_SZ: usize = 48;
/// resource-flush 命令幅（头 24 + rect 16 + resource/padding 8）。
pub const GPU_RESOURCE_FLUSH_SZ: usize = 48;
/// guest→host 2D transfer 命令幅。
pub const GPU_TRANSFER_2D_SZ: usize = 56;
/// attach/detach backing 命令幅。
pub const GPU_BACKING_CMD_SZ: usize = 32;
/// backing memory entry 幅。
pub const GPU_MEM_ENTRY_SZ: usize = 16;
/// MMIO config 区幅。
pub const GPU_CONFIG_SZ: usize = 20;

// ---- 2D 命令 type（规范 §5.10.4）----

pub const CMD_GET_DISPLAY_INFO: u32 = 0x0100;
pub const CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
pub const CMD_RESOURCE_UNREF: u32 = 0x0102;
pub const CMD_SET_SCANOUT: u32 = 0x0103;
pub const CMD_RESOURCE_FLUSH: u32 = 0x0104;
pub const CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
pub const CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
pub const CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;

// ---- 应答 type ----

pub const RESP_OK_NODATA: u32 = 0x1100;
pub const RESP_OK_DISPLAY_INFO: u32 = 0x1101;
pub const RESP_ERR_UNSPEC: u32 = 0x1200;
pub const RESP_ERR_OUT_OF_MEMORY: u32 = 0x1201;
pub const RESP_ERR_INVALID_SCANOUT_ID: u32 = 0x1202;
pub const RESP_ERR_INVALID_RESOURCE_ID: u32 = 0x1203;

// ---- 简单像素格式（规范 §5.10.6）----

/// BGRX8888：本仓 XRGB8888 帧字节的自然 virtio-gpu 格式。
pub const FORMAT_B8G8R8X8_UNORM: u32 = 2;

/// 控制头（语义字段；padding 不携带语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuCtrlHdr {
    pub typ: u32,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub ring_idx: u8,
}

/// 全零非语义字段的标准 2D 头。
impl GpuCtrlHdr {
    pub fn command(typ: u32) -> Self {
        Self { typ, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0 }
    }
}

/// 矩形。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// GET_DISPLAY_INFO 中的一个 scanout mode。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuDisplayMode {
    pub rect: GpuRect,
    pub enabled: u32,
    pub flags: u32,
}

/// 完整 display-info 应答。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuDisplayInfo {
    pub hdr: GpuCtrlHdr,
    pub modes: [GpuDisplayMode; GPU_MAX_SCANOUTS],
}

/// MMIO config 区。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuConfig {
    pub events_read: u32,
    pub events_clear: u32,
    pub num_scanouts: u32,
    pub num_capsets: u32,
    pub blob_alignment: u32,
}

fn put_hdr(buf: &mut [u8], off: usize, hdr: &GpuCtrlHdr) -> Option<()> {
    {
        let b = buf.get_mut(off..off + GPU_CTRL_HDR_SZ)?;
        b.fill(0);
    }
    put_u32(buf, off, hdr.typ)?;
    put_u32(buf, off + 4, hdr.flags)?;
    put_u64(buf, off + 8, hdr.fence_id)?;
    put_u32(buf, off + 16, hdr.ctx_id)?;
    *buf.get_mut(off + 23)? = hdr.ring_idx;
    Some(())
}

fn put_rect(buf: &mut [u8], off: usize, r: GpuRect) -> Option<()> {
    put_u32(buf, off, r.x)?;
    put_u32(buf, off + 4, r.y)?;
    put_u32(buf, off + 8, r.width)?;
    put_u32(buf, off + 12, r.height)
}

/// 编控制头（`GET_DISPLAY_INFO` 请求可直接用本面）。
pub fn put_ctrl_hdr(buf: &mut [u8], off: usize, hdr: &GpuCtrlHdr) -> Option<()> {
    put_hdr(buf, off, hdr)
}

/// RESOURCE_CREATE_2D（40B）。
pub fn put_resource_create_2d(
    buf: &mut [u8],
    off: usize,
    resource_id: u32,
    format: u32,
    width: u32,
    height: u32,
) -> Option<()> {
    buf.get_mut(off..off + GPU_RESOURCE_CREATE_2D_SZ)?;
    put_hdr(buf, off, &GpuCtrlHdr::command(CMD_RESOURCE_CREATE_2D))?;
    put_u32(buf, off + 24, resource_id)?;
    put_u32(buf, off + 28, format)?;
    put_u32(buf, off + 32, width)?;
    put_u32(buf, off + 36, height)
}

/// SET_SCANOUT（48B；wire 序 = rect 后 scanout/resource id）。
pub fn put_set_scanout(
    buf: &mut [u8],
    off: usize,
    r: GpuRect,
    scanout_id: u32,
    resource_id: u32,
) -> Option<()> {
    buf.get_mut(off..off + GPU_SET_SCANOUT_SZ)?;
    put_hdr(buf, off, &GpuCtrlHdr::command(CMD_SET_SCANOUT))?;
    put_rect(buf, off + 24, r)?;
    put_u32(buf, off + 40, scanout_id)?;
    put_u32(buf, off + 44, resource_id)
}

/// RESOURCE_FLUSH（48B；wire 序 = rect 后 resource id + padding）。
pub fn put_resource_flush(
    buf: &mut [u8],
    off: usize,
    resource_id: u32,
    r: GpuRect,
) -> Option<()> {
    buf.get_mut(off..off + GPU_RESOURCE_FLUSH_SZ)?;
    put_hdr(buf, off, &GpuCtrlHdr::command(CMD_RESOURCE_FLUSH))?;
    put_rect(buf, off + 24, r)?;
    put_u32(buf, off + 40, resource_id)?;
    put_u32(buf, off + 44, 0)
}

/// TRANSFER_TO_HOST_2D（56B）。
pub fn put_transfer_to_host_2d(
    buf: &mut [u8],
    off: usize,
    resource_id: u32,
    r: GpuRect,
    offset: u64,
) -> Option<()> {
    buf.get_mut(off..off + GPU_TRANSFER_2D_SZ)?;
    put_hdr(buf, off, &GpuCtrlHdr::command(CMD_TRANSFER_TO_HOST_2D))?;
    put_rect(buf, off + 24, r)?;
    put_u64(buf, off + 40, offset)?;
    put_u32(buf, off + 48, resource_id)
}

/// RESOURCE_ATTACH_BACKING 命令头（32B；mem entries 由调用方另链）。
pub fn put_attach_backing(
    buf: &mut [u8],
    off: usize,
    resource_id: u32,
    nr_entries: u32,
) -> Option<()> {
    buf.get_mut(off..off + GPU_BACKING_CMD_SZ)?;
    put_hdr(buf, off, &GpuCtrlHdr::command(CMD_RESOURCE_ATTACH_BACKING))?;
    put_u32(buf, off + 24, resource_id)?;
    put_u32(buf, off + 28, nr_entries)
}

/// RESOURCE_DETACH_BACKING（32B）。
pub fn put_detach_backing(buf: &mut [u8], off: usize, resource_id: u32) -> Option<()> {
    buf.get_mut(off..off + GPU_BACKING_CMD_SZ)?;
    put_hdr(buf, off, &GpuCtrlHdr::command(CMD_RESOURCE_DETACH_BACKING))?;
    put_u32(buf, off + 24, resource_id)?;
    put_u32(buf, off + 28, 0)
}

/// backing mem entry（16B）。
pub fn put_mem_entry(buf: &mut [u8], off: usize, addr: u64, length: u32) -> Option<()> {
    buf.get_mut(off..off + GPU_MEM_ENTRY_SZ)?.fill(0);
    put_u64(buf, off, addr)?;
    put_u32(buf, off + 8, length)
}

/// 解控制头。
pub fn parse_ctrl_hdr(buf: &[u8], off: usize) -> Option<GpuCtrlHdr> {
    let b = buf.get(off..off + GPU_CTRL_HDR_SZ)?;
    Some(GpuCtrlHdr {
        typ: get_u32(buf, off)?,
        flags: get_u32(buf, off + 4)?,
        fence_id: get_u64(buf, off + 8)?,
        ctx_id: get_u32(buf, off + 16)?,
        ring_idx: b[23],
    })
}

/// 解 MMIO config（恰 20B；截断/尾随全拒）。
pub fn parse_config(data: &[u8]) -> Option<GpuConfig> {
    if data.len() != GPU_CONFIG_SZ {
        return None;
    }
    Some(GpuConfig {
        events_read: get_u32(data, 0)?,
        events_clear: get_u32(data, 4)?,
        num_scanouts: get_u32(data, 8)?,
        num_capsets: get_u32(data, 12)?,
        blob_alignment: get_u32(data, 16)?,
    })
}

/// 解完整 display-info 应答（恰 408B）。
pub fn parse_display_info(data: &[u8]) -> Option<GpuDisplayInfo> {
    if data.len() != GPU_DISPLAY_INFO_SZ {
        return None;
    }
    let hdr = parse_ctrl_hdr(data, 0)?;
    let mut modes = [GpuDisplayMode {
        rect: GpuRect { x: 0, y: 0, width: 0, height: 0 },
        enabled: 0,
        flags: 0,
    }; GPU_MAX_SCANOUTS];
    for (i, mode) in modes.iter_mut().enumerate() {
        let off = GPU_CTRL_HDR_SZ + i * GPU_DISPLAY_MODE_SZ;
        mode.rect = GpuRect {
            x: get_u32(data, off)?,
            y: get_u32(data, off + 4)?,
            width: get_u32(data, off + 8)?,
            height: get_u32(data, off + 12)?,
        };
        mode.enabled = get_u32(data, off + 16)?;
        mode.flags = get_u32(data, off + 20)?;
    }
    Some(GpuDisplayInfo { hdr, modes })
}

const _: () = assert!(GPU_CTRL_HDR_SZ == 24);
const _: () = assert!(GPU_DISPLAY_INFO_SZ == 408);
const _: () = assert!(GPU_MEM_ENTRY_SZ == 16);

#[cfg(test)]
mod gpu_tests {
    use super::*;

    #[test]
    fn ctrl_header_wire_layout_is_little_endian() {
        let mut buf = [0u8; GPU_CTRL_HDR_SZ];
        let hdr = GpuCtrlHdr {
            typ: CMD_GET_DISPLAY_INFO,
            flags: 0x0102_0304,
            fence_id: 0x1122_3344_5566_7788,
            ctx_id: 0x1929_3949,
            ring_idx: 7,
        };
        put_ctrl_hdr(&mut buf, 0, &hdr).unwrap();
        assert_eq!(buf[0..4], CMD_GET_DISPLAY_INFO.to_le_bytes());
        assert_eq!(buf[4..8], hdr.flags.to_le_bytes());
        assert_eq!(buf[8..16], hdr.fence_id.to_le_bytes());
        assert_eq!(buf[16..20], hdr.ctx_id.to_le_bytes());
        assert_eq!(buf[20..23], [0; 3]);
        assert_eq!(buf[23], 7);
        assert_eq!(parse_ctrl_hdr(&buf, 0).unwrap(), hdr);
    }

    #[test]
    fn create_flush_and_transfer_golden_offsets() {
        let mut buf = [0u8; 128];
        put_resource_create_2d(
            &mut buf,
            0,
            9,
            FORMAT_B8G8R8X8_UNORM,
            1080,
            1920,
        )
        .unwrap();
        assert_eq!(buf.len(), 128);
        assert_eq!(&buf[0..4], &CMD_RESOURCE_CREATE_2D.to_le_bytes());
        assert_eq!(&buf[24..28], &9u32.to_le_bytes());
        assert_eq!(&buf[28..32], &FORMAT_B8G8R8X8_UNORM.to_le_bytes());
        assert_eq!(&buf[32..36], &1080u32.to_le_bytes());
        assert_eq!(&buf[36..40], &1920u32.to_le_bytes());
        assert_eq!(buf[40..], [0; 88]);

        let r = GpuRect { x: 12, y: 34, width: 56, height: 78 };
        put_resource_flush(&mut buf, 0, 9, r).unwrap();
        assert_eq!(&buf[0..4], &CMD_RESOURCE_FLUSH.to_le_bytes());
        assert_eq!(&buf[24..40], [12, 0, 0, 0, 34, 0, 0, 0, 56, 0, 0, 0, 78, 0, 0, 0]);
        assert_eq!(&buf[40..44], &9u32.to_le_bytes());
        assert_eq!(&buf[44..48], &[0; 4]);

        put_transfer_to_host_2d(&mut buf, 16, 9, r, 0x1234).unwrap();
        assert_eq!(&buf[16..20], &CMD_TRANSFER_TO_HOST_2D.to_le_bytes());
        assert_eq!(&buf[56..64], &0x1234u64.to_le_bytes());
        assert_eq!(&buf[64..68], &9u32.to_le_bytes());
        assert_eq!(&buf[68..72], &[0; 4]);
    }

    #[test]
    fn scanout_backing_and_mem_entry_layouts() {
        let mut buf = [0u8; 96];
        let r = GpuRect { x: 1, y: 2, width: 3, height: 4 };
        put_set_scanout(&mut buf, 0, r, 5, 6).unwrap();
        assert_eq!(&buf[0..4], &CMD_SET_SCANOUT.to_le_bytes());
        assert_eq!(&buf[24..28], &1u32.to_le_bytes());
        assert_eq!(&buf[40..44], &5u32.to_le_bytes());
        assert_eq!(&buf[44..48], &6u32.to_le_bytes());

        put_attach_backing(&mut buf, 32, 9, 2).unwrap();
        assert_eq!(&buf[32..36], &CMD_RESOURCE_ATTACH_BACKING.to_le_bytes());
        assert_eq!(&buf[56..60], &9u32.to_le_bytes());
        assert_eq!(&buf[60..64], &2u32.to_le_bytes());

        put_detach_backing(&mut buf, 64, 9).unwrap();
        assert_eq!(&buf[64..68], &CMD_RESOURCE_DETACH_BACKING.to_le_bytes());
        assert_eq!(&buf[88..92], &9u32.to_le_bytes());

        put_mem_entry(&mut buf, 0, 0x7000_1000, 4096).unwrap();
        assert_eq!(&buf[0..8], &0x7000_1000u64.to_le_bytes());
        assert_eq!(&buf[8..12], &4096u32.to_le_bytes());
        assert_eq!(&buf[12..16], &[0; 4]);
    }

    #[test]
    fn display_info_and_config_exact_decode() {
        let mut info = [0u8; GPU_DISPLAY_INFO_SZ];
        put_ctrl_hdr(&mut info, 0, &GpuCtrlHdr::command(RESP_OK_DISPLAY_INFO)).unwrap();
        let r = GpuRect { x: 0, y: 0, width: 1080, height: 1920 };
        put_rect(&mut info, 24, r).unwrap();
        put_u32(&mut info, 40, 1).unwrap();
        assert_eq!(parse_display_info(&info).unwrap().hdr.typ, RESP_OK_DISPLAY_INFO);
        assert_eq!(parse_display_info(&info).unwrap().modes[0], GpuDisplayMode {
            rect: r,
            enabled: 1,
            flags: 0,
        });
        assert_eq!(parse_display_info(&info[..407]), None);
        assert_eq!(parse_display_info(&[0; 409]), None);

        let cfg = [1u8, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 4, 0, 0, 0, 5, 0, 0, 0];
        assert_eq!(
            parse_config(&cfg).unwrap(),
            GpuConfig {
                events_read: 1,
                events_clear: 2,
                num_scanouts: 3,
                num_capsets: 4,
                blob_alignment: 5,
            }
        );
        assert_eq!(parse_config(&cfg[..19]), None);
    }

    #[test]
    fn short_command_buffers_reject_without_panic() {
        let mut buf = [0u8; 55];
        assert!(put_resource_create_2d(&mut buf, 16, 1, 2, 3, 4).is_none());
        assert!(put_set_scanout(&mut buf, 16, GpuRect { x: 0, y: 0, width: 1, height: 1 }, 0, 1).is_none());
        assert!(put_resource_flush(&mut buf, 16, 1, GpuRect { x: 0, y: 0, width: 1, height: 1 }).is_none());
        assert!(put_transfer_to_host_2d(&mut buf, 0, 1, GpuRect { x: 0, y: 0, width: 1, height: 1 }, 0).is_none());
        assert!(put_attach_backing(&mut buf, 32, 1, 1).is_none());
        assert!(put_detach_backing(&mut buf, 32, 1).is_none());
        assert!(put_mem_entry(&mut buf, 48, 1, 2).is_none());
    }
}
