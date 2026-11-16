//! berry-gpuproto —— virtio-gpu 2D 控制队列协议纯函数库（刀 3 B1-a）。
//!
//! 归属：显示与加速体系（`research/显示与加速体系设计/02-virtio-gpu-2D档-
//! GPU加速路径兑现设计-v1.0.md` §3.1 协议规格/§3.2 落法/§6 刀序；02 号 §3.2
//! 原名 berry-virtio-gpu，本轮定名 berry-gpuproto 随 user/ proto crate 族
//! 命名例）。S1 spike GO 依据 = 05 号实录（`05-S1spike-三验实录.md` §0：设备
//! 在〔vprobe 枚举 did=16〕/ 传输活〔VNC 三应答〕/ HVF 双 boot 过）。ADR
//! 预#86 = 显示与加速体系立法预留号（现役最高 #85，`docs/架构决策.md`）——
//! 本 crate 是预留号下的协议面首件，正式落册随 B1 序列收口。
//!
//! **范围与边界**：只做纯类型与编码面——2D 九命令（GET_DISPLAY_INFO /
//! RESOURCE_CREATE_2D / RESOURCE_UNREF / SET_SCANOUT / RESOURCE_FLUSH /
//! TRANSFER_TO_HOST_2D / RESOURCE_ATTACH_BACKING / RESOURCE_DETACH_BACKING /
//! GET_EDID）+ 控制头（type/flags/fence/ctx）+ 响应面（NODATA /
//! DISPLAY_INFO / EDID / 错误族）+ config 事件面（events_read /
//! events_clear / num_scanouts——CONFIG_GEN 翻转检测在 mmio 传输层
//! `drivers/berry-virtio/src/lib.rs` :223 CONFIG_GEN=0xfc，归 B1-c）+ BGRX
//! 格式常量与 ramfb fmt=1 映射 + scanout 矩形。**传输接线归 B1-c**：virtqueue
//! 布局（berry-virtio 单页承诺 :14-17）、mmio 扫描/DEVICE_ID_GPU 白名单行
//! （:230-237 设备 ID 族）、INDIRECT 描述符落地（:34 留 v2 位）都不在本 crate。
//! 纪律照 berry-blkproto/berry-fsproto 族例：零分配、零 unsafe、零依赖、
//! `cfg_attr(not(test), no_std)`；编码器 None 拒绝不截断；解析器恰长执法
//! （截断/尾随/未知 type 全 None）；host 单测全盖。
//!
//! **数值两源纪律**：全部线常量逐一对照 Linux uapi（BSD 许可头——协议洁净，
//! 02 号 §3.1；本刀取回原文逐值核）：
//! https://github.com/torvalds/linux/blob/master/include/uapi/linux/virtio_gpu.h
//! https://github.com/torvalds/linux/blob/master/include/uapi/linux/virtio_ids.h
//! host 测 `uapi_constants_pinned` 以 uapi 头手抄字面值逐个断言（crate 常量
//! vs uapi 头——改任一侧即炸响）。
//!
//! 完成语义对 BerryOS 友好（02 号 §3.1 引 QEMU `virtio-gpu.c` 源码）：2D
//! 简单路径全命令即答、无 fence 挂起——used ring 收割即背压；guest v1 不设
//! fence 位（fence 字段面仍建模——uapi 在册，cursor/3D 才真用）。
//!
//! S1 三陷阱（05 号 §5/§6，写给 B1-c 接线者，此处存证）：① 设备名只用
//! `virtio-gpu-device`（§5.1——`virtio-gpu` 是 PCI 别名，落 PCI 总线则 mmio
//! 32 槽窗全盲）；② `-global virtio-mmio.force-legacy=false` 必带（§5.4——
//! 默认 legacy 下槽头 version 报 1）；③ QEMU 设备侧 `indirect_desc` 默认 on
//! （§2.1/§5.7）——ATTACH_BACKING 的 mem_entry 表可达 8KB 级（1080P ≈ 507
//! 页 × 16B ≈ 8.1KB，02 号 §3.2 坑①），超 berry-virtio 单页 virtqueue，传输
//! 侧走 INDIRECT 是正路。

#![cfg_attr(not(test), no_std)]

// ==========================================================================
// 常量面（全部两源断言——tests::uapi_constants_pinned）
// ==========================================================================

/// virtio-gpu 设备 ID（uapi `virtio_ids.h` VIRTIO_ID_GPU=16；05 号 §2.2 实测
/// vprobe 枚举 `slot 28 did=16` 互证）。B1-c 消费：berry-virtio mmio 表设备
/// ID 族（lib.rs :230-237）增行 / vprobe 白名单——本 crate 单源，接线侧引
/// 常量不复制。
pub const DEVICE_ID_GPU: u32 = 16;

/// 设备特性位页 0（uapi `VIRTIO_GPU_F_*` 给**位序号** 0-5，本 crate 存**掩码
/// 值**——特性协商寄存器 DEV_FEAT 读出的就是掩码，berry-virtio
/// `FEAT_VERSION_1` 同例）。guest v1 只协商 VERSION_1（页 1 bit0，传输层
/// `berry-virtio::FEAT_VERSION_1` 单源）+ 可选 EDID；virgl/blob 全族不取
/// （02 号 §2 案 γ 出局、§3.1）。
pub const FEAT_VIRGL: u32 = 1 << 0;
/// EDID 特性位——协商后 GET_EDID 可用（02 号 §3.1；QEMU `edid=on` 默认开，
/// 05 号 §2.1）。
pub const FEAT_EDID: u32 = 1 << 1;
pub const FEAT_RESOURCE_UUID: u32 = 1 << 2;
pub const FEAT_RESOURCE_BLOB: u32 = 1 << 3;
pub const FEAT_CONTEXT_INIT: u32 = 1 << 4;
pub const FEAT_BLOB_ALIGNMENT: u32 = 1 << 5;

// —— 2D 控制命令 type（uapi enum virtio_gpu_ctrl_type，0x0100 族连续递增）——

/// 查显示几何（pmodes 表——响应 `RESP_OK_DISPLAY_INFO`）。
pub const CMD_GET_DISPLAY_INFO: u32 = 0x0100;
/// 建 2D 资源（format/width/height——scanout 几何随资源，02 号 §5）。
pub const CMD_RESOURCE_CREATE_2D: u32 = 0x0101;
/// 销资源。
pub const CMD_RESOURCE_UNREF: u32 = 0x0102;
/// 绑 scanout（scanout 边界 ≥16×16，QEMU 源码执法——见 [`SCANOUT_MIN_EDGE`]）。
pub const CMD_SET_SCANOUT: u32 = 0x0103;
/// 冲刷矩形（与 scanout 求交后 `qemu_console_update`——damage 语义端到端，
/// 02 号 §4.2）。
pub const CMD_RESOURCE_FLUSH: u32 = 0x0104;
/// guest RAM → 设备资源按矩形上传（per-rect 流与 Linux `virtgpu_plane.c`
/// dumb bo 路径同构，02 号 §3.2）。
pub const CMD_TRANSFER_TO_HOST_2D: u32 = 0x0105;
/// 资源挂 backing（mem_entry 表随行——8KB 级表超单页 virtqueue，INDIRECT
/// 面，见头注三陷阱③）。
pub const CMD_RESOURCE_ATTACH_BACKING: u32 = 0x0106;
/// 资源解 backing。
pub const CMD_RESOURCE_DETACH_BACKING: u32 = 0x0107;
/// 查 EDID 原始块（VIRTIO_GPU_F_EDID 协商后；BerryOS 不解析完整 EDID——只信
/// GET_DISPLAY_INFO pmodes 几何，02 号 §7）。
pub const CMD_GET_EDID: u32 = 0x010a;

/// 九命令在册判定（0x0108 GET_CAPSET_INFO / 0x0109 GET_CAPSET 是 virgl 面
/// gap 位，不取；cursor 0x0300 族 v1 砍——鼠标在壳侧绘制，02 号 §3.1）。
pub fn is_cmd(kind: u32) -> bool {
    matches!(
        kind,
        CMD_GET_DISPLAY_INFO
            | CMD_RESOURCE_CREATE_2D
            | CMD_RESOURCE_UNREF
            | CMD_SET_SCANOUT
            | CMD_RESOURCE_FLUSH
            | CMD_TRANSFER_TO_HOST_2D
            | CMD_RESOURCE_ATTACH_BACKING
            | CMD_RESOURCE_DETACH_BACKING
            | CMD_GET_EDID
    )
}

// —— 响应 type ——

/// 无载荷成功（2D 九命令的默认完成形态——QEMU 即答）。
pub const RESP_OK_NODATA: u32 = 0x1100;
/// GET_DISPLAY_INFO 的载荷响应（pmodes 表）。
pub const RESP_OK_DISPLAY_INFO: u32 = 0x1101;
/// GET_EDID 的载荷响应（1024B 容量 EDID 块）。
pub const RESP_OK_EDID: u32 = 0x1104;
/// 错误族首（uapi 0x1200 起六连）。
pub const RESP_ERR_UNSPEC: u32 = 0x1200;
pub const RESP_ERR_OUT_OF_MEMORY: u32 = 0x1201;
pub const RESP_ERR_INVALID_SCANOUT_ID: u32 = 0x1202;
pub const RESP_ERR_INVALID_RESOURCE_ID: u32 = 0x1203;
pub const RESP_ERR_INVALID_CONTEXT_ID: u32 = 0x1204;
pub const RESP_ERR_INVALID_PARAMETER: u32 = 0x1205;

/// 错误响应族判定（uapi 0x1200..=0x1205 恰六连；错误响应与 NODATA 同形——
/// 恰 24B 头，无载荷）。
pub fn is_resp_err(kind: u32) -> bool {
    matches!(
        kind,
        RESP_ERR_UNSPEC
            | RESP_ERR_OUT_OF_MEMORY
            | RESP_ERR_INVALID_SCANOUT_ID
            | RESP_ERR_INVALID_RESOURCE_ID
            | RESP_ERR_INVALID_CONTEXT_ID
            | RESP_ERR_INVALID_PARAMETER
    )
}

// —— 控制头 flags（uapi VIRTIO_GPU_FLAG_*）——

/// fence 位：置位时 fence_id 字段有效。QEMU 2D 简单路径不挂起（02 号 §3.1）
/// ——guest v1 恒不置。
pub const FLAG_FENCE: u32 = 1 << 0;
/// ring_idx 位：置位时 ring_idx 字段有效（多 timeline ring 选路——cursor/3D
/// 面，v1 不用）。
pub const FLAG_INFO_RING_IDX: u32 = 1 << 1;

// —— 像素格式（uapi enum virtio_gpu_formats「simple formats for fbcon/X use」）——

pub const FORMAT_B8G8R8A8_UNORM: u32 = 1;
/// **现役帧格式**：与 ramfb fmt=1 BGRX 同字节序直配（guest 半零像素转换——
/// `tools/berry/src/display/ram.rs` :173 `1 => (b2<<16)|(b1<<8)|b0 // BGRX`；
/// 02 号 §3.1「格式直配」）。RESOURCE_CREATE_2D 的 format 域用本值。
pub const FORMAT_B8G8R8X8_UNORM: u32 = 2;
pub const FORMAT_A8R8G8B8_UNORM: u32 = 3;
pub const FORMAT_X8R8G8B8_UNORM: u32 = 4;
pub const FORMAT_R8G8B8A8_UNORM: u32 = 67;
pub const FORMAT_X8B8G8R8_UNORM: u32 = 68;
pub const FORMAT_A8B8G8R8_UNORM: u32 = 121;
pub const FORMAT_R8G8B8X8_UNORM: u32 = 134;

/// ramfb 侧 BGRX 格式码（`tools/berry/src/display/ram.rs` :168-173 的
/// `info.format` 取值域 0=RGBX/1=BGRX 两档；BGRX 档与
/// [`FORMAT_B8G8R8X8_UNORM`] 同字节序——X 通道字节两栖，alpha 路径互不踩）。
pub const RAMFB_FMT_BGRX: u32 = 1;

/// ramfb fmt → virtio-gpu format 映射（v1 恰一档：BGRX 直配；RGBX=0 档
/// uapi 无同字节序 simple format〔X8R8G8B8=4 是反序〕，不映——guifbd 后端
/// 起资源恒 BGRX，ram.rs :173 的转换面退位）。
pub const fn ramfb_fmt_to_gpu(fmt: u32) -> Option<u32> {
    if fmt == RAMFB_FMT_BGRX {
        Some(FORMAT_B8G8R8X8_UNORM)
    } else {
        None
    }
}

/// GET_DISPLAY_INFO 的 pmodes 表容量（uapi VIRTIO_GPU_MAX_SCANOUTS）。
pub const MAX_SCANOUTS: usize = 16;
/// GET_EDID 响应的 EDID 块容量（uapi `virtio_gpu_resp_edid.edid[1024]`）。
pub const EDID_CAP: usize = 1024;

/// scanout 矩形最小边（QEMU `virtio_gpu_check_scanout_bounds` 源码执法
/// ≥16×16——02 号 §5 引；「scanout 几何随资源、guest 起资源即换分辨率」的
/// 下限依据）。
pub const SCANOUT_MIN_EDGE: u32 = 16;

/// scanout 尺寸合法判定（width/height 皆 ≥ [`SCANOUT_MIN_EDGE`]——只验尺寸
/// 边；scanout_id 越界是设备侧 RESP_ERR_INVALID_SCANOUT_ID 的职责，线面不
/// 预知 max_outputs）。
pub const fn scanout_size_ok(r: &Rect) -> bool {
    r.width >= SCANOUT_MIN_EDGE && r.height >= SCANOUT_MIN_EDGE
}

// ==========================================================================
// 线尺寸常量（wire 真相——#[repr(C)] 尺寸断言 + tests::sizes_pinned 双钉）
// ==========================================================================

/// 控制头（uapi `virtio_gpu_ctrl_hdr`：type u32 + flags u32 + fence_id u64 +
/// ctx_id u32 + ring_idx u8 + padding[3]）= 24B。
pub const CTRL_HDR_SZ: usize = 24;
/// 矩形（uapi `virtio_gpu_rect`：x/y/width/height 各 u32）= 16B。
pub const RECT_SZ: usize = 16;
/// backing 表项（uapi `virtio_gpu_mem_entry`：addr u64 + length u32 +
/// padding u32）= 16B。
pub const MEM_ENTRY_SZ: usize = 16;
/// 单输出几何项（uapi `virtio_gpu_display_one`：rect + enabled u32 +
/// flags u32）= 24B。
pub const PMODE_SZ: usize = RECT_SZ + 8;

pub const CMD_GET_DISPLAY_INFO_SZ: usize = CTRL_HDR_SZ;
pub const CMD_RESOURCE_CREATE_2D_SZ: usize = CTRL_HDR_SZ + 16;
pub const CMD_RESOURCE_UNREF_SZ: usize = CTRL_HDR_SZ + 8;
pub const CMD_SET_SCANOUT_SZ: usize = CTRL_HDR_SZ + RECT_SZ + 8;
pub const CMD_RESOURCE_FLUSH_SZ: usize = CTRL_HDR_SZ + RECT_SZ + 8;
pub const CMD_TRANSFER_TO_HOST_2D_SZ: usize = CTRL_HDR_SZ + RECT_SZ + 16;
/// ATTACH_BACKING 命令定长头（mem_entry 表另计——总长见
/// [`attach_backing_total_sz`]）。
pub const CMD_RESOURCE_ATTACH_BACKING_SZ: usize = CTRL_HDR_SZ + 8;
pub const CMD_RESOURCE_DETACH_BACKING_SZ: usize = CTRL_HDR_SZ + 8;
pub const CMD_GET_EDID_SZ: usize = CTRL_HDR_SZ + 8;

pub const RESP_OK_NODATA_SZ: usize = CTRL_HDR_SZ;
pub const RESP_DISPLAY_INFO_SZ: usize = CTRL_HDR_SZ + MAX_SCANOUTS * PMODE_SZ;
pub const RESP_EDID_SZ: usize = CTRL_HDR_SZ + 8 + EDID_CAP;

// ==========================================================================
// 类型面（#[repr(C)]——字段序/自然对齐与 uapi C 头逐一相合；语义结构体不带
// 线上 padding 域〔恒 0，编码写 0、解析不采信〕，线真相在 SZ 常量）
// ==========================================================================

/// 矩形（SET_SCANOUT / RESOURCE_FLUSH / TRANSFER_TO_HOST_2D / pmodes 共用
/// ——scanout 矩形类型）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
}

/// 控制头（命令/响应共用——uapi `virtio_gpu_ctrl_hdr` 的 `type` 域 Rust 保留
/// 字，此处名 `kind`；padding[3] 恒 0 不入结构，线尾 21..24 字节编码写 0）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CtrlHdr {
    pub kind: u32,
    pub flags: u32,
    pub fence_id: u64,
    pub ctx_id: u32,
    pub ring_idx: u8,
}

impl CtrlHdr {
    /// 最小头（kind 外全 0——QEMU 2D 即答路径无需 fence/ctx，02 号 §3.1）。
    pub const fn new(kind: u32) -> Self {
        Self { kind, flags: 0, fence_id: 0, ctx_id: 0, ring_idx: 0 }
    }

    /// 置 fence 位并带 fence_id（v1 不用——建模存证；FLAG_INFO_RING_IDX 同理
    /// 由调用方自行置位）。
    pub const fn with_fence(mut self, fence_id: u64) -> Self {
        self.flags = self.flags | FLAG_FENCE;
        self.fence_id = fence_id;
        self
    }
}

/// RESOURCE_CREATE_2D 体（resource_id + format + width + height，恰 16B 无
/// padding）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Create2d {
    pub resource_id: u32,
    pub format: u32,
    pub width: u32,
    pub height: u32,
}

/// RESOURCE_UNREF 体（+ 线上 padding u32 恒 0）。resource_id=0 设备侧
/// 拒（RESP_ERR_INVALID_RESOURCE_ID）——线面全域可表达，语义门在设备。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unref {
    pub resource_id: u32,
}

/// SET_SCANOUT 体（rect + scanout_id + resource_id，恰 24B 无 padding）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetScanout {
    pub r: Rect,
    pub scanout_id: u32,
    pub resource_id: u32,
}

/// RESOURCE_FLUSH 体（rect + resource_id + 线上 padding u32 恒 0）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceFlush {
    pub r: Rect,
    pub resource_id: u32,
}

/// TRANSFER_TO_HOST_2D 体（rect + offset u64 + resource_id + 线上 padding
/// u32 恒 0）。offset = 资源 backing 起点的字节偏移（矩形行首定位——
/// `offset + y*stride` 由驱动算好压进矩形语义，本域只给块基）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferToHost2d {
    pub r: Rect,
    pub offset: u64,
    pub resource_id: u32,
}

/// ATTACH_BACKING 定长头体（resource_id + nr_entries；mem_entry 表紧随命令
/// 之后随行——`nr_entries × 16B`，总长恰长执法见
/// [`parse_resource_attach_backing`]）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachBacking {
    pub resource_id: u32,
    pub nr_entries: u32,
}

/// RESOURCE_DETACH_BACKING 体（+ 线上 padding u32 恒 0）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetachBacking {
    pub resource_id: u32,
}

/// GET_EDID 体（scanout + 线上 padding u32 恒 0）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetEdid {
    pub scanout: u32,
}

/// backing 表项（addr=guest 物理址、length=字节长；线上另有 padding u32 恒 0
/// 不入结构）。1080P 帧缓冲 4K 页 ≈ 507 项 × 16B ≈ 8.1KB——超单页
/// virtqueue，传输侧 INDIRECT 面（头注三陷阱③）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemEntry {
    pub addr: u64,
    pub length: u32,
}

/// 单输出几何项（GET_DISPLAY_INFO pmodes 表项：rect + enabled + flags）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pmode {
    pub r: Rect,
    pub enabled: u32,
    pub flags: u32,
}

/// GET_DISPLAY_INFO 载荷（16 槽 pmodes——`enabled=0` 槽无效，BerryOS v1 单屏
/// 只采 pmode[0]，余槽保留态透传）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayInfo {
    pub pmodes: [Pmode; MAX_SCANOUTS],
}

/// GET_EDID 载荷（size=有效字节数 ≤ [`EDID_CAP`]，bytes=1024B 容量块——
/// BerryOS 不解析 EDID 本体〔02 号 §7〕，size 之外只透传）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edid {
    pub size: u32,
    pub bytes: [u8; EDID_CAP],
}

/// 响应分派产物（guest 驱动对 controlq used ring 收割后的按 kind 分流——
/// [`parse_resp`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resp {
    /// 无载荷成功（九命令默认完成形态）。
    OkNodata(CtrlHdr),
    /// GET_DISPLAY_INFO 载荷。
    OkDisplayInfo(CtrlHdr, DisplayInfo),
    /// GET_EDID 载荷。
    OkEdid(CtrlHdr, Edid),
    /// 错误族（kind 保真——`is_resp_err` 全域）。
    Err(CtrlHdr),
}

// 尺寸断言（#[repr(C)] 字段序/自然对齐与 uapi C 头相合的编译期钉板——改
// 字段类型/序即炸响；padding 语义体除外〔Unref/Detach/GetEdid/Flush/
// Transfer 的线上 padding 恒 0〕，其线真相由 SZ 常量 + tests 钉）。
const _: () = assert!(core::mem::size_of::<CtrlHdr>() == CTRL_HDR_SZ);
const _: () = assert!(core::mem::size_of::<Rect>() == RECT_SZ);
const _: () = assert!(core::mem::size_of::<MemEntry>() == MEM_ENTRY_SZ);
const _: () = assert!(core::mem::size_of::<Pmode>() == PMODE_SZ);
const _: () = assert!(core::mem::size_of::<DisplayInfo>() == RESP_DISPLAY_INFO_SZ - CTRL_HDR_SZ);
const _: () = assert!(core::mem::size_of::<Create2d>() == CMD_RESOURCE_CREATE_2D_SZ - CTRL_HDR_SZ);
const _: () = assert!(core::mem::size_of::<SetScanout>() == CMD_SET_SCANOUT_SZ - CTRL_HDR_SZ);
const _: () = assert!(core::mem::size_of::<AttachBacking>() == CMD_RESOURCE_ATTACH_BACKING_SZ - CTRL_HDR_SZ);

// ==========================================================================
// 编解码底座（私有小件——界检 Option 读 / 容量预检后直写）
// ==========================================================================

fn get_u32(buf: &[u8], off: usize) -> Option<u32> {
    let b: [u8; 4] = buf.get(off..off + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(b))
}

fn get_u64(buf: &[u8], off: usize) -> Option<u64> {
    let b: [u8; 8] = buf.get(off..off + 8)?.try_into().ok()?;
    Some(u64::from_le_bytes(b))
}

fn put_u32(out: &mut [u8], off: usize, v: u32) {
    out[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut [u8], off: usize, v: u64) {
    out[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn put_rect(out: &mut [u8], off: usize, r: Rect) {
    put_u32(out, off, r.x);
    put_u32(out, off + 4, r.y);
    put_u32(out, off + 8, r.width);
    put_u32(out, off + 12, r.height);
}

fn read_rect(buf: &[u8], off: usize) -> Option<Rect> {
    Some(Rect {
        x: get_u32(buf, off)?,
        y: get_u32(buf, off + 4)?,
        width: get_u32(buf, off + 8)?,
        height: get_u32(buf, off + 12)?,
    })
}

/// 写头（容量已预检——恒 24B，padding 三字节写 0）。
fn write_hdr(out: &mut [u8], h: &CtrlHdr) {
    put_u32(out, 0, h.kind);
    put_u32(out, 4, h.flags);
    put_u64(out, 8, h.fence_id);
    put_u32(out, 16, h.ctx_id);
    out[20] = h.ring_idx;
    out[21] = 0;
    out[22] = 0;
    out[23] = 0;
}

/// 读头（前 24B，不足 None——调用方续做恰长/kind 门）。
fn read_hdr(buf: &[u8]) -> Option<CtrlHdr> {
    Some(CtrlHdr {
        kind: get_u32(buf, 0)?,
        flags: get_u32(buf, 4)?,
        fence_id: get_u64(buf, 8)?,
        ctx_id: get_u32(buf, 16)?,
        ring_idx: *buf.get(20)?,
    })
}

// ==========================================================================
// 控制头编解码
// ==========================================================================

/// 编头（恰 [`CTRL_HDR_SZ`] 字节；out 太短 → None 不截断）。
pub fn encode_hdr(out: &mut [u8], h: &CtrlHdr) -> Option<usize> {
    if out.len() < CTRL_HDR_SZ {
        return None;
    }
    write_hdr(out, h);
    Some(CTRL_HDR_SZ)
}

/// 解头（恰 [`CTRL_HDR_SZ`] 字节；截断/尾随 → None。kind 在册性由各命令/
/// 响应面自判——本函数只管 24B 几何）。
pub fn parse_hdr(buf: &[u8]) -> Option<CtrlHdr> {
    if buf.len() != CTRL_HDR_SZ {
        return None;
    }
    read_hdr(buf)
}

// ==========================================================================
// 2D 九命令编码/解码（每对：kind 门 + 恰长门；坏输入全 None）
// ==========================================================================

/// GET_DISPLAY_INFO（无体——恰头 24B）。
pub fn encode_get_display_info(out: &mut [u8], hdr: &CtrlHdr) -> Option<usize> {
    if hdr.kind != CMD_GET_DISPLAY_INFO {
        return None;
    }
    if out.len() < CMD_GET_DISPLAY_INFO_SZ {
        return None;
    }
    write_hdr(out, hdr);
    Some(CMD_GET_DISPLAY_INFO_SZ)
}

/// GET_DISPLAY_INFO 解析（恰 24B + kind 门）。
pub fn parse_get_display_info(buf: &[u8]) -> Option<CtrlHdr> {
    if buf.len() != CMD_GET_DISPLAY_INFO_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != CMD_GET_DISPLAY_INFO {
        return None;
    }
    Some(hdr)
}

/// RESOURCE_CREATE_2D 编码（头 + resource_id/format/width/height = 40B）。
pub fn encode_resource_create_2d(
    out: &mut [u8],
    hdr: &CtrlHdr,
    b: &Create2d,
) -> Option<usize> {
    if hdr.kind != CMD_RESOURCE_CREATE_2D {
        return None;
    }
    if out.len() < CMD_RESOURCE_CREATE_2D_SZ {
        return None;
    }
    write_hdr(out, hdr);
    put_u32(out, 24, b.resource_id);
    put_u32(out, 28, b.format);
    put_u32(out, 32, b.width);
    put_u32(out, 36, b.height);
    Some(CMD_RESOURCE_CREATE_2D_SZ)
}

/// RESOURCE_CREATE_2D 解析（恰 40B + kind 门）。
pub fn parse_resource_create_2d(buf: &[u8]) -> Option<(CtrlHdr, Create2d)> {
    if buf.len() != CMD_RESOURCE_CREATE_2D_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != CMD_RESOURCE_CREATE_2D {
        return None;
    }
    let b = Create2d {
        resource_id: get_u32(buf, 24)?,
        format: get_u32(buf, 28)?,
        width: get_u32(buf, 32)?,
        height: get_u32(buf, 36)?,
    };
    Some((hdr, b))
}

/// RESOURCE_UNREF 编码（头 + resource_id + padding0 = 32B）。
pub fn encode_resource_unref(out: &mut [u8], hdr: &CtrlHdr, resource_id: u32) -> Option<usize> {
    if hdr.kind != CMD_RESOURCE_UNREF {
        return None;
    }
    if out.len() < CMD_RESOURCE_UNREF_SZ {
        return None;
    }
    write_hdr(out, hdr);
    put_u32(out, 24, resource_id);
    put_u32(out, 28, 0);
    Some(CMD_RESOURCE_UNREF_SZ)
}

/// RESOURCE_UNREF 解析（恰 32B + kind 门）。
pub fn parse_resource_unref(buf: &[u8]) -> Option<(CtrlHdr, Unref)> {
    if buf.len() != CMD_RESOURCE_UNREF_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != CMD_RESOURCE_UNREF {
        return None;
    }
    Some((hdr, Unref { resource_id: get_u32(buf, 24)? }))
}

/// SET_SCANOUT 编码（头 + rect + scanout_id + resource_id = 48B）。
pub fn encode_set_scanout(out: &mut [u8], hdr: &CtrlHdr, b: &SetScanout) -> Option<usize> {
    if hdr.kind != CMD_SET_SCANOUT {
        return None;
    }
    if out.len() < CMD_SET_SCANOUT_SZ {
        return None;
    }
    write_hdr(out, hdr);
    put_rect(out, 24, b.r);
    put_u32(out, 40, b.scanout_id);
    put_u32(out, 44, b.resource_id);
    Some(CMD_SET_SCANOUT_SZ)
}

/// SET_SCANOUT 解析（恰 48B + kind 门）。
pub fn parse_set_scanout(buf: &[u8]) -> Option<(CtrlHdr, SetScanout)> {
    if buf.len() != CMD_SET_SCANOUT_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != CMD_SET_SCANOUT {
        return None;
    }
    let b = SetScanout {
        r: read_rect(buf, 24)?,
        scanout_id: get_u32(buf, 40)?,
        resource_id: get_u32(buf, 44)?,
    };
    Some((hdr, b))
}

/// RESOURCE_FLUSH 编码（头 + rect + resource_id + padding0 = 48B）。
pub fn encode_resource_flush(out: &mut [u8], hdr: &CtrlHdr, b: &ResourceFlush) -> Option<usize> {
    if hdr.kind != CMD_RESOURCE_FLUSH {
        return None;
    }
    if out.len() < CMD_RESOURCE_FLUSH_SZ {
        return None;
    }
    write_hdr(out, hdr);
    put_rect(out, 24, b.r);
    put_u32(out, 40, b.resource_id);
    put_u32(out, 44, 0);
    Some(CMD_RESOURCE_FLUSH_SZ)
}

/// RESOURCE_FLUSH 解析（恰 48B + kind 门）。
pub fn parse_resource_flush(buf: &[u8]) -> Option<(CtrlHdr, ResourceFlush)> {
    if buf.len() != CMD_RESOURCE_FLUSH_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != CMD_RESOURCE_FLUSH {
        return None;
    }
    let b = ResourceFlush { r: read_rect(buf, 24)?, resource_id: get_u32(buf, 40)? };
    Some((hdr, b))
}

/// TRANSFER_TO_HOST_2D 编码（头 + rect + offset + resource_id + padding0
/// = 56B）。
pub fn encode_transfer_to_host_2d(
    out: &mut [u8],
    hdr: &CtrlHdr,
    b: &TransferToHost2d,
) -> Option<usize> {
    if hdr.kind != CMD_TRANSFER_TO_HOST_2D {
        return None;
    }
    if out.len() < CMD_TRANSFER_TO_HOST_2D_SZ {
        return None;
    }
    write_hdr(out, hdr);
    put_rect(out, 24, b.r);
    put_u64(out, 40, b.offset);
    put_u32(out, 48, b.resource_id);
    put_u32(out, 52, 0);
    Some(CMD_TRANSFER_TO_HOST_2D_SZ)
}

/// TRANSFER_TO_HOST_2D 解析（恰 56B + kind 门）。
pub fn parse_transfer_to_host_2d(buf: &[u8]) -> Option<(CtrlHdr, TransferToHost2d)> {
    if buf.len() != CMD_TRANSFER_TO_HOST_2D_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != CMD_TRANSFER_TO_HOST_2D {
        return None;
    }
    let b = TransferToHost2d {
        r: read_rect(buf, 24)?,
        offset: get_u64(buf, 40)?,
        resource_id: get_u32(buf, 48)?,
    };
    Some((hdr, b))
}

/// ATTACH_BACKING 总长（定长头 32B + nr_entries × 16B——乘加溢出〔32 位
/// usize 场景〕→ None；64 位现场恒可表）。
pub const fn attach_backing_total_sz(nr_entries: u32) -> Option<usize> {
    if let Some(n) = (nr_entries as usize).checked_mul(MEM_ENTRY_SZ) {
        CMD_RESOURCE_ATTACH_BACKING_SZ.checked_add(n)
    } else {
        None
    }
}

/// ATTACH_BACKING 编码（头 + resource_id + nr_entries + 表项随行——nr_entries
/// = entries 实长；表长可达 8KB 级〔头注三陷阱③〕，desc 布阵归 B1-c）。
pub fn encode_resource_attach_backing(
    out: &mut [u8],
    hdr: &CtrlHdr,
    resource_id: u32,
    entries: &[MemEntry],
) -> Option<usize> {
    if hdr.kind != CMD_RESOURCE_ATTACH_BACKING {
        return None;
    }
    if entries.len() > u32::MAX as usize {
        return None;
    }
    let total = attach_backing_total_sz(entries.len() as u32)?;
    if out.len() < total {
        return None;
    }
    write_hdr(out, hdr);
    put_u32(out, 24, resource_id);
    put_u32(out, 28, entries.len() as u32);
    for (i, e) in entries.iter().enumerate() {
        let off = CMD_RESOURCE_ATTACH_BACKING_SZ + i * MEM_ENTRY_SZ;
        put_u64(out, off, e.addr);
        put_u32(out, off + 8, e.length);
        put_u32(out, off + 12, 0);
    }
    Some(total)
}

/// ATTACH_BACKING 解析（恰长含表执法：buf 恰 = 32B + nr_entries × 16B——
/// 表截断/表尾随皆 None；nr_entries 溢出乘加亦 None。返回定长头，表项经
/// [`parse_mem_entry`] 逐项取——零分配纪律下不整表返回）。
pub fn parse_resource_attach_backing(buf: &[u8]) -> Option<(CtrlHdr, AttachBacking)> {
    let hdr = read_hdr(buf)?;
    if hdr.kind != CMD_RESOURCE_ATTACH_BACKING {
        return None;
    }
    let resource_id = get_u32(buf, 24)?;
    let nr_entries = get_u32(buf, 28)?;
    let total = attach_backing_total_sz(nr_entries)?;
    if buf.len() != total {
        return None;
    }
    Some((hdr, AttachBacking { resource_id, nr_entries }))
}

/// ATTACH_BACKING 表项逐项取（`buf` = 整条命令缓冲；`i` 从 0 起——越表
/// 〔32 + (i+1)×16 超长〕→ None；表级恰长由
/// [`parse_resource_attach_backing`] 执法）。
pub fn parse_mem_entry(buf: &[u8], i: usize) -> Option<MemEntry> {
    let off = CMD_RESOURCE_ATTACH_BACKING_SZ.checked_add(i.checked_mul(MEM_ENTRY_SZ)?)?;
    Some(MemEntry { addr: get_u64(buf, off)?, length: get_u32(buf, off + 8)? })
}

/// RESOURCE_DETACH_BACKING 编码（头 + resource_id + padding0 = 32B）。
pub fn encode_resource_detach_backing(
    out: &mut [u8],
    hdr: &CtrlHdr,
    resource_id: u32,
) -> Option<usize> {
    if hdr.kind != CMD_RESOURCE_DETACH_BACKING {
        return None;
    }
    if out.len() < CMD_RESOURCE_DETACH_BACKING_SZ {
        return None;
    }
    write_hdr(out, hdr);
    put_u32(out, 24, resource_id);
    put_u32(out, 28, 0);
    Some(CMD_RESOURCE_DETACH_BACKING_SZ)
}

/// RESOURCE_DETACH_BACKING 解析（恰 32B + kind 门）。
pub fn parse_resource_detach_backing(buf: &[u8]) -> Option<(CtrlHdr, DetachBacking)> {
    if buf.len() != CMD_RESOURCE_DETACH_BACKING_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != CMD_RESOURCE_DETACH_BACKING {
        return None;
    }
    Some((hdr, DetachBacking { resource_id: get_u32(buf, 24)? }))
}

/// GET_EDID 编码（头 + scanout + padding0 = 32B）。
pub fn encode_get_edid(out: &mut [u8], hdr: &CtrlHdr, scanout: u32) -> Option<usize> {
    if hdr.kind != CMD_GET_EDID {
        return None;
    }
    if out.len() < CMD_GET_EDID_SZ {
        return None;
    }
    write_hdr(out, hdr);
    put_u32(out, 24, scanout);
    put_u32(out, 28, 0);
    Some(CMD_GET_EDID_SZ)
}

/// GET_EDID 解析（恰 32B + kind 门）。
pub fn parse_get_edid(buf: &[u8]) -> Option<(CtrlHdr, GetEdid)> {
    if buf.len() != CMD_GET_EDID_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != CMD_GET_EDID {
        return None;
    }
    Some((hdr, GetEdid { scanout: get_u32(buf, 24)? }))
}

// ==========================================================================
// 响应面（guest 主消费向 = 解析；编码面给 host mock/对拍用）
// ==========================================================================

/// OK_NODATA 编码（恰头 24B；kind 恰 [`RESP_OK_NODATA`] 门——错误族走
/// [`encode_resp_err`]）。
pub fn encode_resp_nodata(out: &mut [u8], hdr: &CtrlHdr) -> Option<usize> {
    if hdr.kind != RESP_OK_NODATA {
        return None;
    }
    if out.len() < RESP_OK_NODATA_SZ {
        return None;
    }
    write_hdr(out, hdr);
    Some(RESP_OK_NODATA_SZ)
}

/// OK_NODATA 解析（恰 24B + kind 门）。
pub fn parse_resp_nodata(buf: &[u8]) -> Option<CtrlHdr> {
    if buf.len() != RESP_OK_NODATA_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != RESP_OK_NODATA {
        return None;
    }
    Some(hdr)
}

/// 错误响应编码（与 NODATA 同形——恰头 24B；kind 须在
/// [`is_resp_err`] 六连域）。
pub fn encode_resp_err(out: &mut [u8], hdr: &CtrlHdr) -> Option<usize> {
    if !is_resp_err(hdr.kind) {
        return None;
    }
    if out.len() < RESP_OK_NODATA_SZ {
        return None;
    }
    write_hdr(out, hdr);
    Some(RESP_OK_NODATA_SZ)
}

/// OK_DISPLAY_INFO 编码（头 + 16 槽 pmodes = 408B）。
pub fn encode_resp_display_info(
    out: &mut [u8],
    hdr: &CtrlHdr,
    info: &DisplayInfo,
) -> Option<usize> {
    if hdr.kind != RESP_OK_DISPLAY_INFO {
        return None;
    }
    if out.len() < RESP_DISPLAY_INFO_SZ {
        return None;
    }
    write_hdr(out, hdr);
    for i in 0..MAX_SCANOUTS {
        let off = CTRL_HDR_SZ + i * PMODE_SZ;
        let p = info.pmodes[i];
        put_rect(out, off, p.r);
        put_u32(out, off + RECT_SZ, p.enabled);
        put_u32(out, off + RECT_SZ + 4, p.flags);
    }
    Some(RESP_DISPLAY_INFO_SZ)
}

/// OK_DISPLAY_INFO 解析（恰 408B + kind 门）。
pub fn parse_resp_display_info(buf: &[u8]) -> Option<(CtrlHdr, DisplayInfo)> {
    if buf.len() != RESP_DISPLAY_INFO_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != RESP_OK_DISPLAY_INFO {
        return None;
    }
    let mut pmodes = [Pmode { r: Rect::new(0, 0, 0, 0), enabled: 0, flags: 0 }; MAX_SCANOUTS];
    for i in 0..MAX_SCANOUTS {
        let off = CTRL_HDR_SZ + i * PMODE_SZ;
        pmodes[i] = Pmode {
            r: read_rect(buf, off)?,
            enabled: get_u32(buf, off + RECT_SZ)?,
            flags: get_u32(buf, off + RECT_SZ + 4)?,
        };
    }
    Some((hdr, DisplayInfo { pmodes }))
}

/// OK_EDID 编码（头 + size + padding0 + 1024B 块 = 1056B；size ≤
/// [`EDID_CAP`] 执法——超容量 = 协议违规拒编）。
pub fn encode_resp_edid(out: &mut [u8], hdr: &CtrlHdr, edid: &Edid) -> Option<usize> {
    if hdr.kind != RESP_OK_EDID {
        return None;
    }
    if edid.size as usize > EDID_CAP {
        return None;
    }
    if out.len() < RESP_EDID_SZ {
        return None;
    }
    write_hdr(out, hdr);
    put_u32(out, 24, edid.size);
    put_u32(out, 28, 0);
    out[32..32 + EDID_CAP].copy_from_slice(&edid.bytes);
    Some(RESP_EDID_SZ)
}

/// OK_EDID 解析（恰 1056B + kind 门 + size ≤ [`EDID_CAP`] 执法——设备报
/// size 超容量 = 坏输入拒）。
pub fn parse_resp_edid(buf: &[u8]) -> Option<(CtrlHdr, Edid)> {
    if buf.len() != RESP_EDID_SZ {
        return None;
    }
    let hdr = read_hdr(buf)?;
    if hdr.kind != RESP_OK_EDID {
        return None;
    }
    let size = get_u32(buf, 24)?;
    if size as usize > EDID_CAP {
        return None;
    }
    let mut bytes = [0u8; EDID_CAP];
    bytes.copy_from_slice(buf.get(32..32 + EDID_CAP)?);
    Some((hdr, Edid { size, bytes }))
}

/// 响应总分派（按头 kind 分流；各变体自带恰长门——NODATA/错误族 24B、
/// DISPLAY_INFO 408B、EDID 1056B，错配长度/未建模 kind 全 None）。
pub fn parse_resp(buf: &[u8]) -> Option<Resp> {
    match read_hdr(buf)?.kind {
        RESP_OK_NODATA => parse_resp_nodata(buf).map(Resp::OkNodata),
        RESP_OK_DISPLAY_INFO => parse_resp_display_info(buf).map(|(h, i)| Resp::OkDisplayInfo(h, i)),
        RESP_OK_EDID => parse_resp_edid(buf).map(|(h, e)| Resp::OkEdid(h, e)),
        k if is_resp_err(k) && buf.len() == RESP_OK_NODATA_SZ => {
            // 错误族与 NODATA 同形——恰 24B 头，kind 保真进 Err。
            Some(Resp::Err(read_hdr(buf)?))
        }
        _ => None,
    }
}

// ==========================================================================
// config 事件面（mmio config 区——CONFIG_GEN 翻转检测归 berry-virtio :223，
// 本面只给字段解码与 ACK 编码）
// ==========================================================================

/// uapi `virtio_gpu_config` 全长（events_read + events_clear + num_scanouts +
/// num_capsets + blob_alignment 各 u32 = 20B；后两域随特性位在场——
/// blob_alignment 需 F_BLOB_ALIGNMENT〔uapi 注〕，B1 消费面只保前 12B）。
pub const CONFIG_SZ_UAPI: usize = 20;
/// events_read 偏移（RO——设备置位通知 display 变更）。
pub const CONFIG_OFF_EVENTS_READ: usize = 0;
/// events_clear 偏移（WO——guest ACK 写）。
pub const CONFIG_OFF_EVENTS_CLEAR: usize = 4;
/// num_scanouts 偏移（RO）。
pub const CONFIG_OFF_NUM_SCANOUTS: usize = 8;
/// num_capsets 偏移（RO——virgl 面，BerryOS 不采）。
pub const CONFIG_OFF_NUM_CAPSETS: usize = 12;
/// blob_alignment 偏移（F_BLOB_ALIGNMENT 才有效——BerryOS 不采）。
pub const CONFIG_OFF_BLOB_ALIGNMENT: usize = 16;

/// config events 事件位（uapi VIRTIO_GPU_EVENT_DISPLAY——display 配置变更
/// 〔热插拔/EDID 变化〕；通知链 = 设备置 events_read 位 + mmio CONFIG_GEN
/// 翻转〔传输层〕→ guest 读 → 写 events_clear ACK〔QEMU
/// `virtio_gpu_set_config` 语义，02 号 §3.1〕）。
pub const EVENT_DISPLAY: u32 = 1 << 0;

/// config 事件面三字段（前 12B 恒在域——不依赖任何特性位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigEvents {
    pub events_read: u32,
    pub events_clear: u32,
    pub num_scanouts: u32,
}

/// 解 config 事件面（`buf` = mmio CONFIG 区起读的 ≥12B——设备 config 区
/// 实长设备侧定，本面只取前 12B 恒在域；不足 12B → None。后段 num_capsets/
/// blob_alignment 随特性位，不在本面——防「未协商读域」）。
pub fn parse_config_events(buf: &[u8]) -> Option<ConfigEvents> {
    Some(ConfigEvents {
        events_read: get_u32(buf, CONFIG_OFF_EVENTS_READ)?,
        events_clear: get_u32(buf, CONFIG_OFF_EVENTS_CLEAR)?,
        num_scanouts: get_u32(buf, CONFIG_OFF_NUM_SCANOUTS)?,
    })
}

/// 编 events_clear ACK 写值（恰 4B——Linux 同款单字写〔写
/// `offsetof(virtio_gpu_config, events_clear)` 处 u32〕；置位 = 清
/// events_read 对应位。out 太短 → None）。
pub fn encode_config_events_clear(out: &mut [u8], events: u32) -> Option<usize> {
    if out.len() < 4 {
        return None;
    }
    put_u32(out, 0, events);
    Some(4)
}

// ==========================================================================
// host 测试（AI 测试友好化：协议正确性不依赖 QEMU——blkproto/fsproto 同款；
// 覆盖 = 尺寸钉板 / uapi 两源断言 / 每命令 round-trip + 边界值 / 坏输入
// 拒面 / 线格式手搓钉板）
// ==========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 坏输入拒面三连：截断 None / 原长 Some（sanity）/ 尾随 None。
    fn rej<T>(buf: &[u8], f: impl Fn(&[u8]) -> Option<T>) {
        assert!(f(&buf[..buf.len() - 1]).is_none(), "截断须拒");
        assert!(f(buf).is_some(), "原长须过（sanity）");
        let mut t = buf.to_vec();
        t.push(0xAA);
        assert!(f(&t).is_none(), "尾随须拒");
    }

    #[test]
    fn sizes_pinned() {
        // 线尺寸钉板（uapi 结构自然对齐算术——ctrl_hdr 24 起）。
        assert_eq!(CTRL_HDR_SZ, 24);
        assert_eq!(RECT_SZ, 16);
        assert_eq!(MEM_ENTRY_SZ, 16);
        assert_eq!(PMODE_SZ, 24);
        assert_eq!(CMD_GET_DISPLAY_INFO_SZ, 24);
        assert_eq!(CMD_RESOURCE_CREATE_2D_SZ, 40);
        assert_eq!(CMD_RESOURCE_UNREF_SZ, 32);
        assert_eq!(CMD_SET_SCANOUT_SZ, 48);
        assert_eq!(CMD_RESOURCE_FLUSH_SZ, 48);
        assert_eq!(CMD_TRANSFER_TO_HOST_2D_SZ, 56);
        assert_eq!(CMD_RESOURCE_ATTACH_BACKING_SZ, 32);
        assert_eq!(CMD_RESOURCE_DETACH_BACKING_SZ, 32);
        assert_eq!(CMD_GET_EDID_SZ, 32);
        assert_eq!(RESP_OK_NODATA_SZ, 24);
        assert_eq!(RESP_DISPLAY_INFO_SZ, 408); // 24 + 16×24
        assert_eq!(RESP_EDID_SZ, 1056); // 24 + 8 + 1024
        assert_eq!(CONFIG_SZ_UAPI, 20);
        assert_eq!(MAX_SCANOUTS, 16);
        assert_eq!(EDID_CAP, 1024);
    }

    #[test]
    fn uapi_constants_pinned() {
        // 两源断言：左 = crate 常量，右 = uapi 头手抄字面值
        // （virtio_gpu.h / virtio_ids.h，URL 见 crate 头注）。
        assert_eq!(DEVICE_ID_GPU, 16); // virtio_ids.h VIRTIO_ID_GPU
        // 特性位（uapi VIRTIO_GPU_F_* 给位序号 0-5——此处断掩码值 1<<n，
        // 与 DEV_FEAT 寄存器读数同构）。
        assert_eq!(FEAT_VIRGL, 1); // 1<<0
        assert_eq!(FEAT_EDID, 2); // 1<<1
        assert_eq!(FEAT_RESOURCE_UUID, 4); // 1<<2
        assert_eq!(FEAT_RESOURCE_BLOB, 8); // 1<<3
        assert_eq!(FEAT_CONTEXT_INIT, 16); // 1<<4
        assert_eq!(FEAT_BLOB_ALIGNMENT, 32); // 1<<5
        // 2D 命令族（enum virtio_gpu_ctrl_type 0x0100 起递增）。
        assert_eq!(CMD_GET_DISPLAY_INFO, 0x0100);
        assert_eq!(CMD_RESOURCE_CREATE_2D, 0x0101);
        assert_eq!(CMD_RESOURCE_UNREF, 0x0102);
        assert_eq!(CMD_SET_SCANOUT, 0x0103);
        assert_eq!(CMD_RESOURCE_FLUSH, 0x0104);
        assert_eq!(CMD_TRANSFER_TO_HOST_2D, 0x0105);
        assert_eq!(CMD_RESOURCE_ATTACH_BACKING, 0x0106);
        assert_eq!(CMD_RESOURCE_DETACH_BACKING, 0x0107);
        assert_eq!(CMD_GET_EDID, 0x010a);
        // 响应族。
        assert_eq!(RESP_OK_NODATA, 0x1100);
        assert_eq!(RESP_OK_DISPLAY_INFO, 0x1101);
        assert_eq!(RESP_OK_EDID, 0x1104);
        assert_eq!(RESP_ERR_UNSPEC, 0x1200);
        assert_eq!(RESP_ERR_OUT_OF_MEMORY, 0x1201);
        assert_eq!(RESP_ERR_INVALID_SCANOUT_ID, 0x1202);
        assert_eq!(RESP_ERR_INVALID_RESOURCE_ID, 0x1203);
        assert_eq!(RESP_ERR_INVALID_CONTEXT_ID, 0x1204);
        assert_eq!(RESP_ERR_INVALID_PARAMETER, 0x1205);
        // 头 flags（VIRTIO_GPU_FLAG_FENCE=(1<<0)/INFO_RING_IDX=(1<<1)）。
        assert_eq!(FLAG_FENCE, 1);
        assert_eq!(FLAG_INFO_RING_IDX, 2);
        // 事件位（VIRTIO_GPU_EVENT_DISPLAY=(1<<0)）。
        assert_eq!(EVENT_DISPLAY, 1);
        // 格式族（enum virtio_gpu_formats）。
        assert_eq!(FORMAT_B8G8R8A8_UNORM, 1);
        assert_eq!(FORMAT_B8G8R8X8_UNORM, 2);
        assert_eq!(FORMAT_A8R8G8B8_UNORM, 3);
        assert_eq!(FORMAT_X8R8G8B8_UNORM, 4);
        assert_eq!(FORMAT_R8G8B8A8_UNORM, 67);
        assert_eq!(FORMAT_X8B8G8R8_UNORM, 68);
        assert_eq!(FORMAT_A8B8G8R8_UNORM, 121);
        assert_eq!(FORMAT_R8G8B8X8_UNORM, 134);
        // ramfb 侧（ram.rs :168-173：0=RGBX/1=BGRX）。
        assert_eq!(RAMFB_FMT_BGRX, 1);
        // scanout 最小边（QEMU virtio_gpu_check_scanout_bounds）。
        assert_eq!(SCANOUT_MIN_EDGE, 16);
    }

    #[test]
    fn uapi_cmd_id_family() {
        // 0x0100..=0x0107 八连递增（uapi enum 连续位），GET_EDID=0x010a
        // （0x0108 GET_CAPSET_INFO/0x0109 GET_CAPSET 是 virgl gap 位——不取）。
        for (i, k) in [
            CMD_GET_DISPLAY_INFO,
            CMD_RESOURCE_CREATE_2D,
            CMD_RESOURCE_UNREF,
            CMD_SET_SCANOUT,
            CMD_RESOURCE_FLUSH,
            CMD_TRANSFER_TO_HOST_2D,
            CMD_RESOURCE_ATTACH_BACKING,
            CMD_RESOURCE_DETACH_BACKING,
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(*k, 0x0100 + i as u32);
        }
        assert_eq!(CMD_GET_EDID, 0x0100 + 10);
        // 在册判定：九命令真；gap 位/cursor/3D/未知全假。
        assert!(is_cmd(CMD_GET_DISPLAY_INFO) && is_cmd(CMD_GET_EDID));
        assert!(!is_cmd(0x0108)); // GET_CAPSET_INFO（不取）
        assert!(!is_cmd(0x0109)); // GET_CAPSET（不取）
        assert!(!is_cmd(0x0300)); // UPDATE_CURSOR（v1 砍）
        assert!(!is_cmd(0x0200)); // CTX_CREATE（3D）
        assert!(!is_cmd(0x9999));
        // 错误族判定：六连真；邻值假。
        for k in [
            RESP_ERR_UNSPEC,
            RESP_ERR_OUT_OF_MEMORY,
            RESP_ERR_INVALID_SCANOUT_ID,
            RESP_ERR_INVALID_RESOURCE_ID,
            RESP_ERR_INVALID_CONTEXT_ID,
            RESP_ERR_INVALID_PARAMETER,
        ] {
            assert!(is_resp_err(k));
        }
        assert!(!is_resp_err(0x11FF));
        assert!(!is_resp_err(0x1206));
        assert!(!is_resp_err(RESP_OK_NODATA));
    }

    #[test]
    fn hdr_roundtrip() {
        // 边界三档全字段：全 0 / 现役形态 / 全极值。
        let hs = [
            CtrlHdr::new(CMD_GET_DISPLAY_INFO),
            CtrlHdr::new(CMD_SET_SCANOUT),
            CtrlHdr {
                kind: RESP_ERR_INVALID_PARAMETER,
                flags: FLAG_FENCE | FLAG_INFO_RING_IDX,
                fence_id: u64::MAX,
                ctx_id: u32::MAX,
                ring_idx: u8::MAX,
            },
        ];
        for h in hs {
            let mut b = [0u8; CTRL_HDR_SZ];
            assert_eq!(encode_hdr(&mut b, &h), Some(CTRL_HDR_SZ));
            assert_eq!(parse_hdr(&b), Some(h));
        }
    }

    #[test]
    fn hdr_wire_pinned() {
        // 手搓 24B 直断（blkproto wire_format_pinned 同款——不经对称往返
        // 自证端序/字段序）：kind=0x0103 LE / flags=0x3 / fence=
        // 0x1122334455667788 LE / ctx=7 / ring=9 / padding=0。
        let m = [
            0x03, 0x01, 0x00, 0x00, // kind
            0x03, 0x00, 0x00, 0x00, // flags = FENCE|INFO_RING_IDX
            0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, // fence LE
            0x07, 0x00, 0x00, 0x00, // ctx
            0x09, 0x00, 0x00, 0x00, // ring + padding0
        ];
        let h = parse_hdr(&m).unwrap();
        assert_eq!(h.kind, 0x0103);
        assert_eq!(h.flags, FLAG_FENCE | FLAG_INFO_RING_IDX);
        assert_eq!(h.fence_id, 0x1122_3344_5566_7788);
        assert_eq!(h.ctx_id, 7);
        assert_eq!(h.ring_idx, 9);
        // 编码镜像：round-trip 后字节级一致（padding 归 0 执法）。
        let mut b = [0u8; 24];
        encode_hdr(&mut b, &h).unwrap();
        assert_eq!(b, m);
    }

    #[test]
    fn hdr_len_rejected() {
        assert_eq!(parse_hdr(&[]), None);
        assert_eq!(parse_hdr(&[0u8; 23]), None);
        assert_eq!(parse_hdr(&[0u8; 25]), None);
        assert_eq!(encode_hdr(&mut [], &CtrlHdr::new(0)), None);
        assert_eq!(encode_hdr(&mut [0u8; 23], &CtrlHdr::new(0)), None);
    }

    #[test]
    fn hdr_ctor() {
        let h = CtrlHdr::new(CMD_RESOURCE_FLUSH).with_fence(0xdead_beef);
        assert_eq!(h.kind, CMD_RESOURCE_FLUSH);
        assert_eq!(h.flags, FLAG_FENCE);
        assert_eq!(h.fence_id, 0xdead_beef);
        assert_eq!(h.ctx_id, 0);
        assert_eq!(CtrlHdr::new(RESP_ERR_UNSPEC).flags, 0);
    }

    #[test]
    fn get_display_info_roundtrip() {
        let h = CtrlHdr::new(CMD_GET_DISPLAY_INFO);
        let mut b = [0u8; 32];
        let n = encode_get_display_info(&mut b, &h).unwrap();
        assert_eq!(n, CMD_GET_DISPLAY_INFO_SZ);
        assert_eq!(parse_get_display_info(&b[..n]), Some(h));
        rej(&b[..n], parse_get_display_info);
    }

    #[test]
    fn resource_create_2d_roundtrip() {
        // 边界三档：现役 1080P BGRX / 首资源最小形 / 全极值（线面全域可表达
        // ——resource_id=0/format=0 的语义门在设备侧，协议面不预拒）。
        let bs = [
            Create2d { resource_id: 1, format: FORMAT_B8G8R8X8_UNORM, width: 1920, height: 1080 },
            Create2d { resource_id: 1, format: FORMAT_B8G8R8X8_UNORM, width: 16, height: 16 },
            Create2d { resource_id: u32::MAX, format: u32::MAX, width: u32::MAX, height: u32::MAX },
        ];
        for b in bs {
            let h = CtrlHdr::new(CMD_RESOURCE_CREATE_2D);
            let mut buf = [0u8; 64];
            let n = encode_resource_create_2d(&mut buf, &h, &b).unwrap();
            assert_eq!(n, CMD_RESOURCE_CREATE_2D_SZ);
            assert_eq!(parse_resource_create_2d(&buf[..n]), Some((h, b)));
            rej(&buf[..n], parse_resource_create_2d);
        }
        // 手搓钉板：resource_id=0x0102_0304 / format=2(BGRX) / 1920×1080。
        let mut m = [0u8; 40];
        let h = CtrlHdr::new(CMD_RESOURCE_CREATE_2D);
        encode_resource_create_2d(
            &mut m,
            &h,
            &Create2d { resource_id: 0x0102_0304, format: 2, width: 1920, height: 1080 },
        )
        .unwrap();
        assert_eq!(&m[24..28], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(&m[28..32], &[0x02, 0x00, 0x00, 0x00]);
        assert_eq!(&m[32..36], 1920u32.to_le_bytes());
        assert_eq!(&m[36..40], 1080u32.to_le_bytes());
    }

    #[test]
    fn resource_unref_roundtrip() {
        for rid in [1, u32::MAX] {
            let h = CtrlHdr::new(CMD_RESOURCE_UNREF);
            let mut b = [0u8; 64];
            let n = encode_resource_unref(&mut b, &h, rid).unwrap();
            assert_eq!(n, CMD_RESOURCE_UNREF_SZ);
            assert_eq!(parse_resource_unref(&b[..n]), Some((h, Unref { resource_id: rid })));
            // padding 恒 0 执法。
            assert_eq!(&b[28..32], &[0, 0, 0, 0]);
            rej(&b[..n], parse_resource_unref);
        }
    }

    #[test]
    fn set_scanout_roundtrip_and_min_edge() {
        let h = CtrlHdr::new(CMD_SET_SCANOUT);
        for b in [
            SetScanout { r: Rect::new(0, 0, 1920, 1080), scanout_id: 0, resource_id: 1 },
            SetScanout {
                r: Rect::new(u32::MAX, u32::MAX, 16, 16),
                scanout_id: u32::MAX,
                resource_id: u32::MAX,
            },
        ] {
            let mut buf = [0u8; 64];
            let n = encode_set_scanout(&mut buf, &h, &b).unwrap();
            assert_eq!(n, CMD_SET_SCANOUT_SZ);
            assert_eq!(parse_set_scanout(&buf[..n]), Some((h, b)));
            rej(&buf[..n], parse_set_scanout);
        }
        // scanout 最小边（QEMU ≥16×16 执法——02 号 §5）：恰 16 过、15/0 拒。
        assert!(scanout_size_ok(&Rect::new(0, 0, 16, 16)));
        assert!(scanout_size_ok(&Rect::new(100, 200, 1920, 1080)));
        assert!(!scanout_size_ok(&Rect::new(0, 0, 15, 16)));
        assert!(!scanout_size_ok(&Rect::new(0, 0, 16, 15)));
        assert!(!scanout_size_ok(&Rect::new(0, 0, 0, 0)));
    }

    #[test]
    fn resource_flush_roundtrip() {
        let h = CtrlHdr::new(CMD_RESOURCE_FLUSH);
        let b = ResourceFlush { r: Rect::new(192, 0, 384, 108), resource_id: 7 };
        let mut buf = [0u8; 64];
        let n = encode_resource_flush(&mut buf, &h, &b).unwrap();
        assert_eq!(n, CMD_RESOURCE_FLUSH_SZ);
        assert_eq!(parse_resource_flush(&buf[..n]), Some((h, b)));
        assert_eq!(&buf[44..48], &[0, 0, 0, 0]); // padding 恒 0
        rej(&buf[..n], parse_resource_flush);
    }

    #[test]
    fn transfer_to_host_2d_roundtrip() {
        let h = CtrlHdr::new(CMD_TRANSFER_TO_HOST_2D);
        for b in [
            TransferToHost2d { r: Rect::new(0, 0, 1920, 108), offset: 0, resource_id: 1 },
            TransferToHost2d { r: Rect::new(1, 2, 3, 4), offset: u64::MAX, resource_id: u32::MAX },
        ] {
            let mut buf = [0u8; 80];
            let n = encode_transfer_to_host_2d(&mut buf, &h, &b).unwrap();
            assert_eq!(n, CMD_TRANSFER_TO_HOST_2D_SZ);
            assert_eq!(parse_transfer_to_host_2d(&buf[..n]), Some((h, b)));
            // offset LE 钉板（u64 域在 40..48）。
            assert_eq!(&buf[40..48], b.offset.to_le_bytes());
            assert_eq!(&buf[52..56], &[0, 0, 0, 0]); // padding 恒 0
            rej(&buf[..n], parse_transfer_to_host_2d);
        }
    }

    #[test]
    fn resource_detach_backing_roundtrip() {
        let h = CtrlHdr::new(CMD_RESOURCE_DETACH_BACKING);
        let mut b = [0u8; 64];
        let n = encode_resource_detach_backing(&mut b, &h, 9).unwrap();
        assert_eq!(n, CMD_RESOURCE_DETACH_BACKING_SZ);
        assert_eq!(parse_resource_detach_backing(&b[..n]), Some((h, DetachBacking { resource_id: 9 })));
        rej(&b[..n], parse_resource_detach_backing);
    }

    #[test]
    fn get_edid_roundtrip() {
        for scanout in [0, 1, u32::MAX] {
            let h = CtrlHdr::new(CMD_GET_EDID);
            let mut b = [0u8; 64];
            let n = encode_get_edid(&mut b, &h, scanout).unwrap();
            assert_eq!(n, CMD_GET_EDID_SZ);
            assert_eq!(parse_get_edid(&b[..n]), Some((h, GetEdid { scanout })));
            rej(&b[..n], parse_get_edid);
        }
    }

    #[test]
    fn attach_backing_roundtrip() {
        // 三项表：首零项 / 极值项 / 现役几何项（1080P 首行 backing 形态）。
        let entries = [
            MemEntry { addr: 0, length: 4096 },
            MemEntry { addr: u64::MAX, length: u32::MAX },
            MemEntry { addr: 0x7000_0000, length: 16_384 },
        ];
        let h = CtrlHdr::new(CMD_RESOURCE_ATTACH_BACKING);
        let mut buf = [0u8; 256];
        let n = encode_resource_attach_backing(&mut buf, &h, 1, &entries).unwrap();
        assert_eq!(n, CMD_RESOURCE_ATTACH_BACKING_SZ + 3 * MEM_ENTRY_SZ); // 80
        assert_eq!(
            parse_resource_attach_backing(&buf[..n]),
            Some((h, AttachBacking { resource_id: 1, nr_entries: 3 }))
        );
        for (i, e) in entries.iter().enumerate() {
            assert_eq!(parse_mem_entry(&buf[..n], i), Some(*e));
        }
        // 表项字节钉板：项 1 addr u64::MAX LE + length u32::MAX + padding0。
        assert_eq!(&buf[32 + 16..32 + 24], &[0xFF; 8]);
        assert_eq!(&buf[32 + 24..32 + 28], &[0xFF; 4]);
        assert_eq!(&buf[32 + 28..32 + 32], &[0, 0, 0, 0]);
        // 表尾随/截断拒（rej 的截断臂 = 掐末项 1B）。
        rej(&buf[..n], parse_resource_attach_backing);
    }

    #[test]
    fn attach_backing_zero_entries() {
        // 空表：恰 32B 头（nr_entries=0——线面合法；设备侧语义〔等价 DETACH
        // 与否〕BerryOS 消费面不发送，02 号 §3.2 流程恒实表）。
        let h = CtrlHdr::new(CMD_RESOURCE_ATTACH_BACKING);
        let mut buf = [0u8; 64];
        let n = encode_resource_attach_backing(&mut buf, &h, 1, &[]).unwrap();
        assert_eq!(n, CMD_RESOURCE_ATTACH_BACKING_SZ);
        assert_eq!(
            parse_resource_attach_backing(&buf[..n]),
            Some((h, AttachBacking { resource_id: 1, nr_entries: 0 }))
        );
        // 尾随 1B 拒。
        let mut t = buf[..n].to_vec();
        t.push(0);
        assert_eq!(parse_resource_attach_backing(&t), None);
    }

    #[test]
    fn attach_backing_table_mismatch_rejected() {
        // 声明 3 项只给 2 项（手搓 nr_entries=3 + 32B 表）→ None。
        let entries = [
            MemEntry { addr: 0x1000, length: 4096 },
            MemEntry { addr: 0x2000, length: 4096 },
        ];
        let h = CtrlHdr::new(CMD_RESOURCE_ATTACH_BACKING);
        let mut buf = [0u8; 128];
        let n = encode_resource_attach_backing(&mut buf, &h, 1, &entries).unwrap();
        buf[28..32].copy_from_slice(&3u32.to_le_bytes()); // 篡改 nr_entries
        assert_eq!(parse_resource_attach_backing(&buf[..n]), None);
        // 表内越界取项 → None；负 i（i=0 表前）不存在——usize 无负，越表即 None。
        assert_eq!(parse_mem_entry(&buf[..n], 2), None);
        assert_eq!(parse_mem_entry(&buf[..n], usize::MAX), None);
        // 溢出乘加：nr_entries=u32::MAX 时 total = 32+16×u32::MAX——64 位可
        // 表（≠ None），恰长门拒之；32 位平台乘法溢出 → None（checked 路径）。
        let mut big = [0u8; 64];
        big[0..4].copy_from_slice(&CMD_RESOURCE_ATTACH_BACKING.to_le_bytes());
        big[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(attach_backing_total_sz(u32::MAX).is_some()); // 64 位 host
        assert_eq!(parse_resource_attach_backing(&big), None); // 恰长门
    }

    #[test]
    fn cmd_wrong_kind_rejected() {
        // 编码面：kind 与命令不符 → None（全九对抽查三对）。
        let mut b = [0u8; 128];
        assert_eq!(
            encode_resource_create_2d(&mut b, &CtrlHdr::new(CMD_SET_SCANOUT), &Create2d {
                resource_id: 1,
                format: 2,
                width: 16,
                height: 16
            }),
            None
        );
        assert_eq!(encode_get_display_info(&mut b, &CtrlHdr::new(CMD_GET_EDID)), None);
        assert_eq!(encode_resp_err(&mut b, &CtrlHdr::new(RESP_OK_NODATA)), None);
        // 解析面：线字节 kind 被换 → None。
        let h = CtrlHdr::new(CMD_SET_SCANOUT);
        let s = SetScanout { r: Rect::new(0, 0, 64, 64), scanout_id: 0, resource_id: 1 };
        let n = encode_set_scanout(&mut b, &h, &s).unwrap();
        let mut m = b[..n].to_vec();
        m[0..4].copy_from_slice(&CMD_GET_DISPLAY_INFO.to_le_bytes());
        assert_eq!(parse_set_scanout(&m), None);
        // 未建模 kind → None（is_cmd 假 + 恰长过也不分派）。
        m[0..4].copy_from_slice(&0x0108u32.to_le_bytes());
        assert_eq!(parse_set_scanout(&m), None);
    }

    #[test]
    fn cmd_all_nine_truncated_trailing_rejected() {
        // 九命令全走 rej 三连（截断/原长/尾随）——每命令独立缓冲。
        let mut b = [0u8; 2048];
        let n = encode_get_display_info(&mut b, &CtrlHdr::new(CMD_GET_DISPLAY_INFO)).unwrap();
        rej(&b[..n].to_vec(), parse_get_display_info);

        let n = encode_resource_create_2d(
            &mut b,
            &CtrlHdr::new(CMD_RESOURCE_CREATE_2D),
            &Create2d { resource_id: 1, format: 2, width: 64, height: 64 },
        )
        .unwrap();
        rej(&b[..n].to_vec(), parse_resource_create_2d);

        let n = encode_resource_unref(&mut b, &CtrlHdr::new(CMD_RESOURCE_UNREF), 1).unwrap();
        rej(&b[..n].to_vec(), parse_resource_unref);

        let n = encode_set_scanout(
            &mut b,
            &CtrlHdr::new(CMD_SET_SCANOUT),
            &SetScanout { r: Rect::new(0, 0, 64, 64), scanout_id: 0, resource_id: 1 },
        )
        .unwrap();
        rej(&b[..n].to_vec(), parse_set_scanout);

        let n = encode_resource_flush(
            &mut b,
            &CtrlHdr::new(CMD_RESOURCE_FLUSH),
            &ResourceFlush { r: Rect::new(0, 0, 8, 8), resource_id: 1 },
        )
        .unwrap();
        rej(&b[..n].to_vec(), parse_resource_flush);

        let n = encode_transfer_to_host_2d(
            &mut b,
            &CtrlHdr::new(CMD_TRANSFER_TO_HOST_2D),
            &TransferToHost2d { r: Rect::new(0, 0, 8, 8), offset: 0, resource_id: 1 },
        )
        .unwrap();
        rej(&b[..n].to_vec(), parse_transfer_to_host_2d);

        let n = encode_resource_attach_backing(
            &mut b,
            &CtrlHdr::new(CMD_RESOURCE_ATTACH_BACKING),
            1,
            &[MemEntry { addr: 0x1000, length: 4096 }],
        )
        .unwrap();
        rej(&b[..n].to_vec(), parse_resource_attach_backing);

        let n = encode_resource_detach_backing(&mut b, &CtrlHdr::new(CMD_RESOURCE_DETACH_BACKING), 1)
            .unwrap();
        rej(&b[..n].to_vec(), parse_resource_detach_backing);

        let n = encode_get_edid(&mut b, &CtrlHdr::new(CMD_GET_EDID), 0).unwrap();
        rej(&b[..n].to_vec(), parse_get_edid);
    }

    #[test]
    fn resp_nodata_roundtrip() {
        let h = CtrlHdr::new(RESP_OK_NODATA);
        let mut b = [0u8; 64];
        let n = encode_resp_nodata(&mut b, &h).unwrap();
        assert_eq!(n, RESP_OK_NODATA_SZ);
        assert_eq!(parse_resp_nodata(&b[..n]), Some(h));
        assert_eq!(parse_resp(&b[..n]), Some(Resp::OkNodata(h)));
        rej(&b[..n], parse_resp_nodata);
    }

    #[test]
    fn resp_err_family_roundtrip() {
        // 六连错误 kind 全域：编码/解析/分派三面。
        for k in [
            RESP_ERR_UNSPEC,
            RESP_ERR_OUT_OF_MEMORY,
            RESP_ERR_INVALID_SCANOUT_ID,
            RESP_ERR_INVALID_RESOURCE_ID,
            RESP_ERR_INVALID_CONTEXT_ID,
            RESP_ERR_INVALID_PARAMETER,
        ] {
            let h = CtrlHdr::new(k);
            let mut b = [0u8; 64];
            assert_eq!(encode_resp_err(&mut b, &h), Some(RESP_OK_NODATA_SZ));
            assert_eq!(parse_resp(&b[..RESP_OK_NODATA_SZ]), Some(Resp::Err(h)));
        }
        // OK kind 不走错误编码面。
        let mut b = [0u8; 64];
        assert_eq!(encode_resp_err(&mut b, &CtrlHdr::new(RESP_OK_NODATA)), None);
    }

    #[test]
    fn resp_display_info_roundtrip() {
        let mut info = DisplayInfo {
            pmodes: [Pmode { r: Rect::new(0, 0, 0, 0), enabled: 0, flags: 0 }; MAX_SCANOUTS],
        };
        info.pmodes[0] = Pmode { r: Rect::new(0, 0, 1280, 800), enabled: 1, flags: 0 };
        info.pmodes[1] = Pmode { r: Rect::new(0, 0, 1920, 1080), enabled: 1, flags: u32::MAX };
        info.pmodes[15] = Pmode { r: Rect::new(u32::MAX, u32::MAX, 16, 16), enabled: u32::MAX, flags: 7 };
        let h = CtrlHdr::new(RESP_OK_DISPLAY_INFO);
        let mut b = [0u8; RESP_DISPLAY_INFO_SZ + 8];
        let n = encode_resp_display_info(&mut b, &h, &info).unwrap();
        assert_eq!(n, RESP_DISPLAY_INFO_SZ);
        assert_eq!(parse_resp_display_info(&b[..n]), Some((h, info)));
        assert_eq!(parse_resp(&b[..n]), Some(Resp::OkDisplayInfo(h, info)));
        // 槽 0 字节钉板：pmodes[0].enabled 在 24+16=40 处 LE。
        assert_eq!(&b[40..44], &1u32.to_le_bytes());
        rej(&b[..n], parse_resp_display_info);
    }

    #[test]
    fn resp_edid_roundtrip_and_size_gate() {
        let mut edid = Edid { size: 128, bytes: [0u8; EDID_CAP] };
        edid.bytes[0] = 0x00;
        edid.bytes[1] = 0xFF;
        edid.bytes[127] = 0x5A;
        let h = CtrlHdr::new(RESP_OK_EDID);
        let mut b = [0u8; RESP_EDID_SZ + 8];
        let n = encode_resp_edid(&mut b, &h, &edid).unwrap();
        assert_eq!(n, RESP_EDID_SZ);
        assert_eq!(parse_resp_edid(&b[..n]), Some((h, edid)));
        assert_eq!(parse_resp(&b[..n]), Some(Resp::OkEdid(h, edid)));
        // size 域与首字节钉板。
        assert_eq!(&b[24..28], &128u32.to_le_bytes());
        assert_eq!(&b[28..32], &[0, 0, 0, 0]); // padding 恒 0
        assert_eq!(b[32], 0x00);
        assert_eq!(b[33], 0xFF);
        rej(&b[..n], parse_resp_edid);

        // 边界：size=1024 恰容量过；size=1025 双面拒（编码 None / 解析 None）。
        let mut full = Edid { size: EDID_CAP as u32, bytes: [0xEE; EDID_CAP] };
        full.bytes[0] = 0x00;
        let n = encode_resp_edid(&mut b, &h, &full).unwrap();
        assert_eq!(parse_resp_edid(&b[..n]), Some((h, full)));
        let mut bad = full;
        bad.size = EDID_CAP as u32 + 1;
        assert_eq!(encode_resp_edid(&mut b, &h, &bad), None);
        let mut m = b[..n].to_vec();
        m[24..28].copy_from_slice(&(EDID_CAP as u32 + 1).to_le_bytes());
        assert_eq!(parse_resp_edid(&m), None);
        assert_eq!(parse_resp(&m), None);
    }

    #[test]
    fn resp_dispatch_rejects() {
        // 未建模 kind → None（OK_CAPSET_INFO=0x1102 不取）。
        let mut m = vec![0u8; RESP_OK_NODATA_SZ];
        m[0..4].copy_from_slice(&0x1102u32.to_le_bytes());
        assert_eq!(parse_resp(&m), None);
        // kind/长度错配：DISPLAY_INFO kind + 24B → None。
        m[0..4].copy_from_slice(&RESP_OK_DISPLAY_INFO.to_le_bytes());
        assert_eq!(parse_resp(&m), None);
        // 错误族 kind + 408B（同形错配）→ None。
        let mut big = vec![0u8; RESP_DISPLAY_INFO_SZ];
        big[0..4].copy_from_slice(&RESP_ERR_UNSPEC.to_le_bytes());
        assert_eq!(parse_resp(&big), None);
        // 空缓冲/截断头 → None。
        assert_eq!(parse_resp(&[]), None);
        assert_eq!(parse_resp(&[0u8; 23]), None);
    }

    #[test]
    fn config_events_roundtrip() {
        // 20B uapi 全长手搓：events_read=EVENT_DISPLAY / events_clear=0 /
        // num_scanouts=1 / num_capsets=0 / blob_alignment=0。
        let mut cfg = [0u8; CONFIG_SZ_UAPI];
        cfg[0..4].copy_from_slice(&EVENT_DISPLAY.to_le_bytes());
        cfg[8..12].copy_from_slice(&1u32.to_le_bytes());
        let ev = parse_config_events(&cfg).unwrap();
        assert_eq!(
            ev,
            ConfigEvents { events_read: EVENT_DISPLAY, events_clear: 0, num_scanouts: 1 }
        );
        // 偏移常量与手搓互证。
        assert_eq!(CONFIG_OFF_EVENTS_READ, 0);
        assert_eq!(CONFIG_OFF_EVENTS_CLEAR, 4);
        assert_eq!(CONFIG_OFF_NUM_SCANOUTS, 8);
        // 恰 12B 前缀（B1 消费面读法）过；11B 拒。
        assert!(parse_config_events(&cfg[..12]).is_some());
        assert_eq!(parse_config_events(&cfg[..11]), None);
        assert_eq!(parse_config_events(&[]), None);
        // ACK 编码：4B LE 单字写（Linux 同款写 events_clear 偏移处）。
        let mut ack = [0u8; 4];
        assert_eq!(encode_config_events_clear(&mut ack, EVENT_DISPLAY), Some(4));
        assert_eq!(ack, EVENT_DISPLAY.to_le_bytes());
        assert_eq!(encode_config_events_clear(&mut [], EVENT_DISPLAY), None);
    }

    #[test]
    fn ramfb_bgrx_mapping() {
        // 格式直配（02 号 §3.1「格式直配」）：ramfb fmt=1(BGRX) ↔ uapi
        // B8G8R8X8_UNORM=2——guest 半零像素转换的协议根基。
        assert_eq!(ramfb_fmt_to_gpu(RAMFB_FMT_BGRX), Some(FORMAT_B8G8R8X8_UNORM));
        assert_eq!(ramfb_fmt_to_gpu(RAMFB_FMT_BGRX), Some(2));
        // RGBX=0 档 uapi 无同字节序 simple format——v1 不映。
        assert_eq!(ramfb_fmt_to_gpu(0), None);
        assert_eq!(ramfb_fmt_to_gpu(u32::MAX), None);
    }

    #[test]
    fn encode_empty_out_rejected() {
        // None 拒绝契约 day-one 面（fsproto 对抗审查同族坑——空 out 全编码器
        // None，不截断不 panic）。
        let h0 = CtrlHdr::new(0);
        assert_eq!(encode_hdr(&mut [], &h0), None);
        assert_eq!(encode_get_display_info(&mut [], &CtrlHdr::new(CMD_GET_DISPLAY_INFO)), None);
        assert_eq!(
            encode_resource_create_2d(
                &mut [],
                &CtrlHdr::new(CMD_RESOURCE_CREATE_2D),
                &Create2d { resource_id: 1, format: 2, width: 16, height: 16 }
            ),
            None
        );
        assert_eq!(encode_resource_unref(&mut [], &CtrlHdr::new(CMD_RESOURCE_UNREF), 1), None);
        assert_eq!(
            encode_set_scanout(
                &mut [],
                &CtrlHdr::new(CMD_SET_SCANOUT),
                &SetScanout { r: Rect::new(0, 0, 16, 16), scanout_id: 0, resource_id: 1 }
            ),
            None
        );
        assert_eq!(
            encode_resource_flush(
                &mut [],
                &CtrlHdr::new(CMD_RESOURCE_FLUSH),
                &ResourceFlush { r: Rect::new(0, 0, 16, 16), resource_id: 1 }
            ),
            None
        );
        assert_eq!(
            encode_transfer_to_host_2d(
                &mut [],
                &CtrlHdr::new(CMD_TRANSFER_TO_HOST_2D),
                &TransferToHost2d { r: Rect::new(0, 0, 16, 16), offset: 0, resource_id: 1 }
            ),
            None
        );
        assert_eq!(
            encode_resource_attach_backing(
                &mut [],
                &CtrlHdr::new(CMD_RESOURCE_ATTACH_BACKING),
                1,
                &[MemEntry { addr: 0, length: 16 }]
            ),
            None
        );
        assert_eq!(
            encode_resource_detach_backing(&mut [], &CtrlHdr::new(CMD_RESOURCE_DETACH_BACKING), 1),
            None
        );
        assert_eq!(encode_get_edid(&mut [], &CtrlHdr::new(CMD_GET_EDID), 0), None);
        assert_eq!(encode_resp_nodata(&mut [], &CtrlHdr::new(RESP_OK_NODATA)), None);
        assert_eq!(encode_resp_err(&mut [], &CtrlHdr::new(RESP_ERR_UNSPEC)), None);
        let info = DisplayInfo {
            pmodes: [Pmode { r: Rect::new(0, 0, 0, 0), enabled: 0, flags: 0 }; MAX_SCANOUTS],
        };
        assert_eq!(
            encode_resp_display_info(&mut [], &CtrlHdr::new(RESP_OK_DISPLAY_INFO), &info),
            None
        );
        assert_eq!(
            encode_resp_edid(
                &mut [],
                &CtrlHdr::new(RESP_OK_EDID),
                &Edid { size: 0, bytes: [0; EDID_CAP] }
            ),
            None
        );
    }

    #[test]
    fn encode_out_exact_fit() {
        // out 恰等于所需容量：容量门误写 < 时本测显形（blkproto
        // out_exact_fit 同款盲区盖）。
        let mut b24 = [0u8; CMD_GET_DISPLAY_INFO_SZ];
        assert_eq!(
            encode_get_display_info(&mut b24, &CtrlHdr::new(CMD_GET_DISPLAY_INFO)),
            Some(CMD_GET_DISPLAY_INFO_SZ)
        );
        let mut b48 = [0u8; CMD_SET_SCANOUT_SZ];
        assert_eq!(
            encode_set_scanout(
                &mut b48,
                &CtrlHdr::new(CMD_SET_SCANOUT),
                &SetScanout { r: Rect::new(0, 0, 1920, 1080), scanout_id: 0, resource_id: 1 }
            ),
            Some(CMD_SET_SCANOUT_SZ)
        );
        let mut b56 = [0u8; CMD_TRANSFER_TO_HOST_2D_SZ];
        assert_eq!(
            encode_transfer_to_host_2d(
                &mut b56,
                &CtrlHdr::new(CMD_TRANSFER_TO_HOST_2D),
                &TransferToHost2d { r: Rect::new(0, 0, 8, 8), offset: 0, resource_id: 1 }
            ),
            Some(CMD_TRANSFER_TO_HOST_2D_SZ)
        );
        let mut b80 = [0u8; 80]; // attach 恰 32+3×16
        assert_eq!(
            encode_resource_attach_backing(
                &mut b80,
                &CtrlHdr::new(CMD_RESOURCE_ATTACH_BACKING),
                1,
                &[MemEntry { addr: 0, length: 1 }, MemEntry { addr: 0, length: 2 }, MemEntry {
                    addr: 0,
                    length: 3
                }]
            ),
            Some(80)
        );
        let mut b1056 = [0u8; RESP_EDID_SZ];
        assert_eq!(
            encode_resp_edid(
                &mut b1056,
                &CtrlHdr::new(RESP_OK_EDID),
                &Edid { size: 0, bytes: [0; EDID_CAP] }
            ),
            Some(RESP_EDID_SZ)
        );
    }
}
