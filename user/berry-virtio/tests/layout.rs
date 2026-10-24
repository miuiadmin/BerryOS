//! berry-virtio host 单测（P6 A-3）：split ring 布局/编解码 + virtio-mmio
//! 偏移表——「解析器即产品」纪律，全部纯函数面，QEMU 之前先在 host 盖住。

use berry_virtio::*;

#[test]
fn layout_qsize16_geometry() {
    // vblk 消费档：desc@0(256B) avail@256 used@512，全幅 646B ≤ 单页。
    let lay = RingLayout::new(16).unwrap();
    assert_eq!(lay.desc_off, 0);
    assert_eq!(lay.avail_off, 256);
    assert_eq!(lay.used_off, 512);
    assert_eq!(lay.span(), 512 + 4 + 8 * 16 + 2);
}

#[test]
fn layout_minimal() {
    // qsize=1：desc@0，avail 原始尾 16+4+2+2=24 → used 对齐 256。
    let lay = RingLayout::new(1).unwrap();
    assert_eq!(lay.avail_off, 16);
    assert_eq!(lay.used_off, 256);
    assert_eq!(lay.span(), 256 + 4 + 8 + 2);
}

#[test]
fn layout_single_page_boundary() {
    // 128 恰入单页（span 3590）；256 出页（desc 表独占 4096）——承诺面执法。
    assert!(RingLayout::new(128).is_some());
    assert!(RingLayout::new(256).is_none());
}

#[test]
fn layout_rejects() {
    assert!(RingLayout::new(0).is_none());
    assert!(RingLayout::new(3).is_none()); // 非 2 的幂
    assert!(RingLayout::new(24).is_none());
}

#[test]
fn desc_roundtrip() {
    let lay = RingLayout::new(16).unwrap();
    let mut ring = [0u8; 4096];
    put_desc(&mut ring, &lay, 5, 0x4001_2345, 512, DESC_F_NEXT | DESC_F_WRITE, 6).unwrap();
    assert_eq!(get_u64(&ring, lay.desc_off + 5 * 16).unwrap(), 0x4001_2345);
    assert_eq!(get_u32(&ring, lay.desc_off + 5 * 16 + 8).unwrap(), 512);
    assert_eq!(get_u16(&ring, lay.desc_off + 5 * 16 + 12).unwrap(), 3);
    assert_eq!(get_u16(&ring, lay.desc_off + 5 * 16 + 14).unwrap(), 6);
}

#[test]
fn desc_bounds() {
    let lay = RingLayout::new(16).unwrap();
    let mut ring = [0u8; 4096];
    assert!(put_desc(&mut ring, &lay, 16, 0, 0, 0, 0).is_none()); // idx == qsize
    // 缓冲截断（desc 15 需要到 240+16=256；只给 200；desc 12 到 208 也出界，
    // desc 11 到 192 恰在）。
    let mut short = [0u8; 200];
    assert!(put_desc(&mut short, &lay, 15, 0, 0, 0, 0).is_none());
    assert!(put_desc(&mut short, &lay, 12, 1, 2, 0, 0).is_none());
    assert!(put_desc(&mut short, &lay, 11, 1, 2, 0, 0).is_some()); // 界内不受牵连
}

#[test]
fn publish_first_and_wrap() {
    let lay = RingLayout::new(16).unwrap();
    let mut ring = [0u8; 4096];
    publish(&mut ring, &lay, 3).unwrap();
    assert_eq!(get_u16(&ring, lay.avail_off + 2).unwrap(), 1); // idx 0→1
    assert_eq!(get_u16(&ring, lay.avail_off + 4).unwrap(), 3); // ring[0]=head

    // 自由运行 idx：预置 20（> qsize，长期跑的真实形态）→ 槽 4 收 head。
    put_u16pub(&mut ring, lay.avail_off + 2, 20);
    publish(&mut ring, &lay, 7).unwrap();
    assert_eq!(get_u16(&ring, lay.avail_off + 4 + 2 * 4).unwrap(), 7);
    assert_eq!(get_u16(&ring, lay.avail_off + 2).unwrap(), 21);

    // u16 环绕：65535 → 0（wrapping，不 panic）；65535 % 16 = 15 → 槽 15 收。
    put_u16pub(&mut ring, lay.avail_off + 2, 65535);
    publish(&mut ring, &lay, 9).unwrap();
    assert_eq!(get_u16(&ring, lay.avail_off + 2).unwrap(), 0);
    assert_eq!(get_u16(&ring, lay.avail_off + 4 + 2 * 15).unwrap(), 9);
}

#[test]
fn publish_truncated() {
    let lay = RingLayout::new(16).unwrap();
    // avail idx 字段在 256+2——缓冲只给 256B（差 2 字节）必拒。
    let mut short = [0u8; 256];
    assert!(publish(&mut short, &lay, 0).is_none());
}

#[test]
fn used_face() {
    let lay = RingLayout::new(16).unwrap();
    let mut ring = [0u8; 4096];
    // 设备侧完成形态：used.idx=1、ring[0]=(id=3,len=513)（blk IN 完成长 = 512+1）。
    put_u16pub(&mut ring, lay.used_off + 2, 1);
    ring[lay.used_off + 4..lay.used_off + 8].copy_from_slice(&3u32.to_le_bytes());
    ring[lay.used_off + 8..lay.used_off + 12].copy_from_slice(&513u32.to_le_bytes());
    assert_eq!(used_idx(&ring, &lay).unwrap(), 1);
    assert_eq!(used_ent(&ring, &lay, 0).unwrap(), (3, 513));
    assert!(used_ent(&ring, &lay, 16).is_none()); // i == qsize
}

#[test]
fn blk_hdr_encoding() {
    let mut buf = [0xffu8; 32]; // 预置脏值验 reserved 显式清零
    put_blk_hdr(&mut buf, 8, BLK_T_OUT, 0x1234).unwrap();
    assert_eq!(get_u32(&buf, 8).unwrap(), BLK_T_OUT);
    assert_eq!(&buf[12..16], &[0, 0, 0, 0]); // reserved 恒 0
    assert_eq!(get_u64(&buf, 16).unwrap(), 0x1234);
    assert_eq!(&buf[24..32], &[0xff; 8]); // 幅外不扰

    let mut tiny = [0u8; 8];
    assert!(put_blk_hdr(&mut tiny, 0, BLK_T_IN, 0).is_none()); // 放不下 16B
    assert!(put_blk_hdr(&mut buf, 20, BLK_T_IN, 0).is_none()); // 尾越界（20+16>32）
}

#[test]
fn le_read_bounds() {
    let buf = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    assert_eq!(get_u16(&buf, 0).unwrap(), 0x0201);
    assert_eq!(get_u32(&buf, 0).unwrap(), 0x0403_0201);
    assert_eq!(get_u64(&buf, 0).unwrap(), 0x0807_0605_0403_0201);
    assert!(get_u16(&buf, 7).is_none());
    assert!(get_u32(&buf, 5).is_none());
    assert!(get_u64(&buf, 1).is_none());
}

#[test]
fn capacity_config_read() {
    // blk config capacity @CONFIG+0 u64：1MiB/512B = 2048 扇区（smoke 盘容断言源）。
    let mut cfg = [0u8; 8];
    cfg.copy_from_slice(&2048u64.to_le_bytes());
    assert_eq!(get_u64(&cfg, 0).unwrap(), 2048);
}

#[test]
fn mmio_table_invariants() {
    use berry_virtio::mmio::*;
    assert_eq!(MAGIC_VAL, u32::from_le_bytes(*b"virt"));
    assert_eq!(VERSION_MODERN, 2);
    assert_eq!(DEVICE_ID_BLOCK, 2);
    // 表驱动不变量：全部偏移 4 对齐、互异、且落在规范寄存器空间 [0, 0x100)。
    let offs = [
        MAGIC, VERSION, DEVICE_ID, VENDOR_ID, DEV_FEAT, DEV_FEAT_SEL, DRV_FEAT,
        DRV_FEAT_SEL, Q_SEL, Q_NUM_MAX, Q_NUM, Q_READY, Q_NOTIFY, INT_STATUS,
        INT_ACK, STATUS, Q_DESC_LOW, Q_DESC_HIGH, Q_AVAIL_LOW, Q_AVAIL_HIGH,
        Q_USED_LOW, Q_USED_HIGH, CONFIG_GEN,
    ];
    for (i, &a) in offs.iter().enumerate() {
        assert_eq!(a % 4, 0, "offset 0x{a:x} 未 4 对齐");
        assert!(a < CONFIG);
        for &b in &offs[i + 1..] {
            assert_ne!(a, b, "偏移重复 0x{a:x}");
        }
    }
    // 三对 LOW/HIGH 恒差 4（驱动按 +4 拼高低半）。
    assert_eq!(Q_DESC_HIGH - Q_DESC_LOW, 4);
    assert_eq!(Q_AVAIL_HIGH - Q_AVAIL_LOW, 4);
    assert_eq!(Q_USED_HIGH - Q_USED_LOW, 4);
}

/// 测试挂具：写 LE u16（复用被测的私有 put 不诚实——独立拼字节）。
fn put_u16pub(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
