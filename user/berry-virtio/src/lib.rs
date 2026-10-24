//! virtio 1.x split virtqueue 布局/编解码（P6 A-3，virtio 规范 §2.6/§5.2）。
//!
//! 纯函数纪律（`berry-loader`/`berry-fsproto` 同款）：零分配、零 unsafe、
//! 零依赖；输入只借用，输出写调用方切片——**编不出就拒绝（None），不截断**。
//! host 单测全盖（`tests/layout.rs`）；消费者 vblk（用户态块设备驱动）经
//! path 依赖双端共用。
//!
//! 范围：split queue（packed ring 留 v2）+ virtio-mmio v2 寄存器偏移表
//! （规范冻结值，表驱动单源）+ virtio-blk 请求头。volatile MMIO 读写留在
//! 驱动本体（副作用面不进纯函数 crate）——本 crate 只管「内存里的字节
//! 长什么样」。
//!
//! 几何前提（vblk 的消费决策，写在这防后来者踩）：qsize=16 时 desc(256B)
//! + avail(38B) + used(134B) 共 428B——**整 virtqueue 落一页**，描述符
//! 地址域只指向本页与缓冲页 ⇒ 单页天然物理连续，无需多页连续分配
//! （帧分配器无连续性保证，跨页 DMA 结构 = 逐 desc 指各自页基 + 偏移）。

#![cfg_attr(not(test), no_std)]

/// split 描述符表项：addr u64 + len u32 + flags u16 + next u16 = 16B（规范 §2.6.1）。
pub const DESC_SZ: usize = 16;
/// avail ring 头（flags u16 + idx u16）+ ring[qsize] u16 + used_event u16（无
/// EVENT_IDX 协商时设备不读 used_event——占位保偏移算术显式）。
pub const AVAIL_HDR: usize = 4;
/// used ring 头（flags u16 + idx u16）+ ring[qsize]（id u32 + len u32）+
/// avail_event u16。
pub const USED_HDR: usize = 4;
pub const USED_ENT: usize = 8;

/// 描述符 flags：NEXT(1) = 链有下家；WRITE(2) = device-write（读方向缓冲/
/// 状态字节）；INDIRECT(4) 留 v2。
pub const DESC_F_NEXT: u16 = 1;
pub const DESC_F_WRITE: u16 = 2;

/// 传输层特性位（规范 §6）：页 1（bit 32 起）——VERSION_1 必收（现代 v2
/// 传输层的协商下限；驱动不收 = 设备可不置 FEATURES_OK）。
pub const FEAT_VERSION_1: u32 = 1 << 0; // 页 1 的 bit0 = 全局 bit 32

/// 设备状态位（规范 §2.1）：复位恒 0 起，ACK → DRIVER → FEATURES_OK（回读
/// 验设备未撤）→ 建队列 → DRIVER_OK（此后设备可用）。
pub const ST_ACKNOWLEDGE: u32 = 1;
pub const ST_DRIVER: u32 = 2;
pub const ST_DRIVER_OK: u32 = 4;
pub const ST_FEATURES_OK: u32 = 8;

/// virtio-blk 请求头（规范 §5.2.6）：type u32（0 IN 读 / 1 OUT 写 / …）+
/// reserved u32 + sector u64 = 16B。响应 = 单字节状态（0 OK，其余错）。
pub const BLK_HDR_SZ: usize = 16;
pub const BLK_T_IN: u32 = 0;
pub const BLK_T_OUT: u32 = 1;
pub const BLK_SECTOR_SZ: usize = 512;

/// virtio-net 特性位（规范 §6.5）：页 0 bit5 = VIRTIO_NET_F_MAC（config
/// 区的 MAC 地址有效）——vnet 协商收它 + MRG_RXBUF + VERSION_1（页 1）。
pub const NET_F_MAC: u32 = 1 << 5;
/// VIRTIO_NET_F_MRG_RXBUF（规范 §6.5 页 0 bit15）：协商后 net 头恒为 mrg
/// 变体 12B（num_buffers 字段在场——TX 侧驱动置 0、RX 侧设备写实际合并
/// 缓冲数，小帧恒 1）。**必须协商**：QEMU virtio-net 在 VERSION_1 路径下
/// `vnet_hdr_len` 即按 12 剥头——只按 10B 组 TX 包 = 帧被斩头 2B、SLIRP
/// 静默丢弃、rx 永假超时（A-7 filter-dump 实证：捕获帧恰为发帧 [2..]）。
/// 协商它把 12B 从「QEMU 实现行为」升为规范保证，跨版本稳。
pub const NET_F_MRG_RXBUF: u32 = 1 << 15;
/// virtio-net 请求头（规范 §5.1.6 mrg 变体，协商 MRG_RXBUF 后收发同形）：
/// flags/gso_type/hdr_len/gso_size/csum_start/csum_offset 六 u16 + num_buffers
/// u16 = 12B——非 checksum/gso 驱动（v1 vnet）恒写全零，帧本体紧随其后同
/// 缓冲（rx 同形：设备写 12B 头再写帧，num_buffers = 实际合并缓冲数）。
pub const NET_HDR_SZ: usize = 12;

/// split ring 布局（单页内偏移）：desc @0，avail @256（desc[16] 恰 256B，
/// 天然分界），used @512（对 avail 尾取整到 512——纯为算术显式，规范对
/// modern 环无对齐要求）。全链 = 512 + 6 + 8*16 + 2 = 646B ≤ 4KiB。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingLayout {
    /// qsize（2 的幂，≤ QueueNumMax——16 为 vblk 档）。
    pub qsize: usize,
    /// desc 表偏移（恒 0，字段化保调用方不硬编）。
    pub desc_off: usize,
    /// avail ring 偏移。
    pub avail_off: usize,
    /// used ring 偏移。
    pub used_off: usize,
}

impl RingLayout {
    /// qsize 建布局。拒绝：0 / 非 2 的幂 / 超单页（页内布局是本 crate 的
    /// 承诺面——超页布局留 v2 连续分配刀）。
    pub fn new(qsize: usize) -> Option<Self> {
        if qsize == 0 || !qsize.is_power_of_two() {
            return None;
        }
        let avail_off = DESC_SZ * qsize;
        let used_off = avail_off + AVAIL_HDR + 2 * qsize + 2;
        // used 尾对齐到 256 的倍数（desc/avail 段本就 ≤256 对齐界——单 qsize
        // 档下等价于「used @512」，写通用式防 qsize 变档时漂移）。
        let used_off = (used_off + 0xff) & !0xff;
        let end = used_off + USED_HDR + USED_ENT * qsize + 2;
        if end > 4096 {
            return None; // 单页承诺（见模块头几何前提）
        }
        Some(Self { qsize, desc_off: 0, avail_off, used_off })
    }

    /// 布局总幅（字节，页内）。
    pub fn span(&self) -> usize {
        self.used_off + USED_HDR + USED_ENT * self.qsize + 2
    }
}

/// LE 定宽读写（virtio 全域 LE，规范 §2.6.1；本 crate 不用 usize 拼字节——
/// 宽度即语义）。越界拒绝 None（「编不出就拒绝」）。
fn put_u16(buf: &mut [u8], off: usize, v: u16) -> Option<()> {
    let b = buf.get_mut(off..off + 2)?;
    b.copy_from_slice(&v.to_le_bytes());
    Some(())
}

fn put_u32(buf: &mut [u8], off: usize, v: u32) -> Option<()> {
    let b = buf.get_mut(off..off + 4)?;
    b.copy_from_slice(&v.to_le_bytes());
    Some(())
}

fn put_u64(buf: &mut [u8], off: usize, v: u64) -> Option<()> {
    let b = buf.get_mut(off..off + 8)?;
    b.copy_from_slice(&v.to_le_bytes());
    Some(())
}

/// avail/used idx 等半字读：`buf[off..off+2]` LE u16。
pub fn get_u16(buf: &[u8], off: usize) -> Option<u16> {
    let b = buf.get(off..off + 2)?;
    Some(u16::from_le_bytes([b[0], b[1]]))
}

pub fn get_u32(buf: &[u8], off: usize) -> Option<u32> {
    let b = buf.get(off..off + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// 读 LE u64（blk config 的 capacity 字段等）。
pub fn get_u64(buf: &[u8], off: usize) -> Option<u64> {
    let b = buf.get(off..off + 8)?;
    let mut v = [0u8; 8];
    v.copy_from_slice(b);
    Some(u64::from_le_bytes(v))
}

/// 写 split 描述符 `idx`（addr/len/flags/next；next 仅 NEXT 位置时有效——
/// 调用方纪律，此处不解释字段）。
pub fn put_desc(
    ring: &mut [u8],
    lay: &RingLayout,
    idx: usize,
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
) -> Option<()> {
    if idx >= lay.qsize {
        return None;
    }
    let off = lay.desc_off + idx * DESC_SZ;
    put_u64(ring, off, addr)?;
    put_u32(ring, off + 8, len)?;
    put_u16(ring, off + 12, flags)?;
    put_u16(ring, off + 14, next)
}

/// 发布请求：avail.ring[idx % qsize] = head 描述符号 + avail.idx 前进一
/// （单请求发布面——批量发布留 v2）。**idx 是自由运行 u16**（规范 §2.6.7：
/// 环绕在 65536，不在 qsize——环下标恒 `% qsize`；「单在途同步等」纪律由
/// 调用方保证，本函数不记账在途数）。调用方随后 `dmb ish` 再 QueueNotify。
pub fn publish(ring: &mut [u8], lay: &RingLayout, head: u16) -> Option<()> {
    let old = get_u16(ring, lay.avail_off + 2)? as usize;
    let slot = old % lay.qsize;
    put_u16(ring, lay.avail_off + 4 + 2 * slot, head)?;
    put_u16(ring, lay.avail_off + 2, (old.wrapping_add(1)) as u16)
}

/// 收割面：used.idx（已完成的环下标——驱动侧与 avail.idx 对齐数比）。
pub fn used_idx(ring: &[u8], lay: &RingLayout) -> Option<u16> {
    get_u16(ring, lay.used_off + 2)
}

/// used ring 第 `i` 项（id u32 + len u32）——id = 完成请求的头描述符号。
pub fn used_ent(ring: &[u8], lay: &RingLayout, i: usize) -> Option<(u32, u32)> {
    if i >= lay.qsize {
        return None;
    }
    let off = lay.used_off + USED_HDR + USED_ENT * i;
    Some((get_u32(ring, off)?, get_u32(ring, off + 4)?))
}

/// virtio-mmio 传输层寄存器偏移（规范 §4.2.2，冻结值；LE 32 位访问，
/// 队列三址各拆 LOW/HIGH 两寄存器——v2 专属，legacy 走 PFN@0x40 不在册）。
/// volatile 读写在驱动本体——这里只单源偏移与鉴定值（magic/version/deviceID）。
pub mod mmio {
    pub const MAGIC: usize = 0x00;
    pub const VERSION: usize = 0x04;
    pub const DEVICE_ID: usize = 0x08;
    pub const VENDOR_ID: usize = 0x0c;
    pub const DEV_FEAT: usize = 0x10;
    pub const DEV_FEAT_SEL: usize = 0x14;
    pub const DRV_FEAT: usize = 0x20;
    pub const DRV_FEAT_SEL: usize = 0x24;
    pub const Q_SEL: usize = 0x30;
    pub const Q_NUM_MAX: usize = 0x34;
    pub const Q_NUM: usize = 0x38;
    pub const Q_READY: usize = 0x44;
    pub const Q_NOTIFY: usize = 0x50;
    pub const INT_STATUS: usize = 0x60;
    pub const INT_ACK: usize = 0x64;
    pub const STATUS: usize = 0x70;
    pub const Q_DESC_LOW: usize = 0x80;
    pub const Q_DESC_HIGH: usize = 0x84;
    pub const Q_AVAIL_LOW: usize = 0x90;
    pub const Q_AVAIL_HIGH: usize = 0x94;
    pub const Q_USED_LOW: usize = 0xa0;
    pub const Q_USED_HIGH: usize = 0xa4;
    pub const CONFIG_GEN: usize = 0xfc;
    pub const CONFIG: usize = 0x100;

    /// MAGIC 值 = `"virt"` LE（0x74726976）——探测判据（vprobe 扫描面）。
    pub const MAGIC_VAL: u32 = 0x7472_6976;
    /// 传输层版本：2 = modern（v1 legacy 已弃；0 = 非virtio设备）。
    pub const VERSION_MODERN: u32 = 2;
    /// virtio-blk 设备 ID（规范 §5.2）。
    pub const DEVICE_ID_BLOCK: u32 = 2;
    /// virtio-net 设备 ID（规范 §5.1；A-7 起同窗挂 virtio-net-device——
    /// 占用槽按设备类判读，见 vprobe/vnet/vblk 扫描面）。
    pub const DEVICE_ID_NETWORK: u32 = 1;
}

/// 编 virtio-blk 请求头（type/reserved/sector → 16B）。
pub fn put_blk_hdr(buf: &mut [u8], off: usize, typ: u32, sector: u64) -> Option<()> {
    let b = buf.get_mut(off..off + BLK_HDR_SZ)?;
    b.copy_from_slice(&[0u8; BLK_HDR_SZ]); // reserved 恒 0（显式清，不赌调用方）
    let mut o = off;
    put_u32(buf, o, typ)?;
    o += 8;
    put_u64(buf, o, sector)
}
