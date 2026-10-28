//! berrydump parse——固件表纯解析面（V-5a-0）。
//!
//! 纪律：**纯函数、零分配、零 panic**——一切字段读经 bounds 检查出
//! Option，畸形输入返 None/半途截断（dump 语境：宁缺勿炸，缺席行本身
//! 就是事实）。偏移钉死依据：**V-5a-1 AAVMF/QEMU 真字节裁决**
//! （GICC flags@+12/PhysBase@+32、GICD base@+8/ver@+20、GICR base@+4、
//! GAS 内 address@+4——ACPICA 结构视图，与 C 结构体习惯偏移不同）；
//! 早期 V-5a-0 手抄偏移曾与合成 fixture 同错自洽（elf2pe「合成基准须
//! 复刻真实产物形态」教训复发），真实字节才是裁决源。FDT 是唯一大端
//! 源（BE magic/size）。
//!
//! host 测试用合成 fixture 钉板（偏移即断言面，按真机裁决值造）；跨
//! 固件复验 = V-5a Fusion 实机会话对拍。

// （no_std 由 crate 根声明——模块级重复属 no-op，会被 unused_attributes 点名。）

// —— 有界小端读（RSDP/XSDT/MADT/SPCR/DBG2 全 LE；FDT 单独走 be_u32）——

pub fn le_u16(b: &[u8], off: usize) -> Option<u16> {
    let e = off.checked_add(2)?;
    if e > b.len() {
        return None;
    }
    Some(u16::from_le_bytes([b[off], b[off + 1]]))
}

pub fn le_u32(b: &[u8], off: usize) -> Option<u32> {
    let e = off.checked_add(4)?;
    if e > b.len() {
        return None;
    }
    Some(u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]))
}

pub fn le_u64(b: &[u8], off: usize) -> Option<u64> {
    let e = off.checked_add(8)?;
    if e > b.len() {
        return None;
    }
    let mut v = [0u8; 8];
    v.copy_from_slice(&b[off..off + 8]);
    Some(u64::from_le_bytes(v))
}

/// FDT 头是大端（设备树唯一例外源）。
pub fn be_u32(b: &[u8], off: usize) -> Option<u32> {
    let e = off.checked_add(4)?;
    if e > b.len() {
        return None;
    }
    Some(u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]))
}

/// 4B 签名比对（ACPI 表 sig 定长 4）。
pub fn sig4(b: &[u8], off: usize, sig: &[u8; 4]) -> bool {
    match off.checked_add(4) {
        Some(e) if e <= b.len() => &b[off..off + 4] == sig,
        _ => false,
    }
}

// —— RSDP（36B 头视图：sig "RSD PTR " 8B@0、revision u8@15、
//    XSDT 物理地址 u64@24——revision ≥ 2 才存在）——

pub const SIG_RSDP: &[u8; 8] = b"RSD PTR ";

pub struct RsdpFacts {
    pub revision: u8,
    pub xsdt_pa: Option<u64>,
}

pub fn rsdp_xsdt(b: &[u8]) -> Option<RsdpFacts> {
    if b.len() < 36 || &b[..8] != SIG_RSDP {
        return None;
    }
    let revision = b[15];
    let xsdt_pa = if revision >= 2 { le_u64(b, 24) } else { None };
    Some(RsdpFacts { revision, xsdt_pa })
}

// —— XSDT（头 36B：sig "XSDT"、length u32@4；条目 u64 指针起 @36）——

/// 事实面走查帽——dump 语境防巨型畸形表拖死（真实平台 ≤ 10 条）。
pub const MAX_XSDT_ENTRIES: usize = 16;

pub struct XsdtEntries {
    pub ptrs: [u64; MAX_XSDT_ENTRIES],
    pub n: usize,
    pub truncated: bool, // 真实条目数 > 帽（打 …more= 截断标记）
}

pub fn xsdt_entries(b: &[u8]) -> Option<XsdtEntries> {
    if !sig4(b, 0, b"XSDT") {
        return None;
    }
    let len = le_u32(b, 4)? as usize;
    if len < 36 || len > b.len() {
        return None;
    }
    let count = (len - 36) / 8;
    let mut x = XsdtEntries { ptrs: [0; MAX_XSDT_ENTRIES], n: 0, truncated: count > MAX_XSDT_ENTRIES };
    for i in 0..count.min(MAX_XSDT_ENTRIES) {
        x.ptrs[i] = le_u64(b, 36 + i * 8)?;
        x.n += 1;
    }
    Some(x)
}

// —— MADT（sig "APIC"；头 36B + local_intc_addr **u32**@36 + flags u32@40，
//    条目起 @44；条目头 = type u8@0 + length u16@1，packed 子表）——
//    V-5a-1 首验实抓（AAVMF/QEMU 真字节裁决，五锚点：GICD base@+8 =
//    0x0800_0000 / ver@+20 = 2 / GICC flags@+12 = enabled / GICC perf@+20
//    = 23(PMU PPI) / MSI base@+8 = 0x0802_0000）：① local_intc_addr 曾误
//    按 u64@36 抄 → 条目起算成 @48；② **type 编号曾错**（0→GICC 是 x86
//    Local APIC、0x0B→GICD 错位一格）——ARM 系真值：0x0B=GICC、0x0C=GICD、
//    0x0D=MSI Frame、0x0E=GICR、0x0F=ITS；③ 子域偏移按 ACPICA 结构视图
//    （子表头 2B：Type u8 + Length u8 高位折叠）——GICC flags@+12/PhysBase
//    @+32/GICRBase@+60、GICD base@+8/ver@+20、GICR base@+4，与 C 结构体
//    习惯偏移（flags@13/base@5）不同；合成 fixture 与代码曾同错自洽
//    （elf2pe「合成基准须复刻真实产物形态」教训复发），真实字节才是裁决源。

pub struct MadtFacts {
    pub cpu_total: u32,           // GICC 条目（type 0x0B）计数
    pub cpu_enabled: u32,         // flags bit0（ACPI_MADT_ENABLED）置位数
    pub gicd_version: Option<u8>, // GICD 条目（0x0C）version u8@+20
    pub gicd_base: Option<u64>,   // GICD 条目 base u64@+8
    pub gicc_base: Option<u64>,   // 首个 GICC 条目 PhysBase u64@+32（GICv2 面）
    pub gicr_base: Option<u64>,   // GICR 条目（0x0E）base u64@+4；无则 GICC GICRBase u64@+60 非零兜底
}

pub fn parse_madt(b: &[u8]) -> Option<MadtFacts> {
    if !sig4(b, 0, b"APIC") {
        return None;
    }
    let len = le_u32(b, 4)? as usize;
    if len < 44 || len > b.len() {
        return None;
    }
    let mut f = MadtFacts {
        cpu_total: 0,
        cpu_enabled: 0,
        gicd_version: None,
        gicd_base: None,
        gicc_base: None,
        gicr_base: None,
    };
    let mut gicr_in_gicc: Option<u64> = None;
    let mut off = 44usize;
    while off + 2 <= len {
        let etype = b[off];
        // 长度域读失败（尾随残字节）= 截断走查——已收事实保留（测试
        // madt_faces 的 2B 畸形尾执法点：`?` 直炸会把半程事实全扬）。
        let elen = match le_u16(b, off + 1) {
            Some(x) => x as usize,
            None => break,
        };
        if elen < 2 || off + elen > len {
            break; // 畸形条目截断走查——已收事实保留（宁缺勿炸）
        }
        match etype {
            0x0B => {
                // GICC（len 80：flags u32@+12、PhysBase u64@+32、GICRBase u64@+60）
                f.cpu_total += 1;
                if elen >= 16 && le_u32(b, off + 12).unwrap_or(0) & 1 != 0 {
                    f.cpu_enabled += 1;
                }
                if f.gicc_base.is_none() && elen >= 40 {
                    f.gicc_base = le_u64(b, off + 32);
                }
                if gicr_in_gicc.is_none() && elen >= 68 {
                    let a = le_u64(b, off + 60).unwrap_or(0);
                    if a != 0 {
                        gicr_in_gicc = Some(a); // GICv3 MADT 双源：GICC 内嵌 GICR 基址
                    }
                }
            }
            0x0C => {
                // GICD（len 24：base u64@+8 / version u8@+20）
                if elen >= 21 {
                    f.gicd_base = le_u64(b, off + 8);
                    f.gicd_version = Some(b[off + 20]);
                }
            }
            0x0E => {
                // GICR（len 16：base u64@+4，条目源优先于 GICC 内嵌）
                if elen >= 12 && f.gicr_base.is_none() {
                    f.gicr_base = le_u64(b, off + 4);
                }
            }
            _ => {} // 0x0D MSI Frame / 0x0F ITS 等——事实面不消费，走查跳过
        }
        off += elen;
    }
    if f.gicr_base.is_none() {
        f.gicr_base = gicr_in_gicc;
    }
    Some(f)
}

// —— SPCR（sig "SPCR"；iface u8@36、GAS@40（address_space u8@+0、
//    address u64@+4 → 表内 @44）、GSI u32@54、baud u8@58）——V-5a-1
//    AAVMF 真字节裁决（Linux acpi_table_spcr 同视图：IntType@52/IRQ@53/
//    GSI@54/baud@58/precision@59/stop@60/flow@61/term@62、PCI id@64..68
//    = ffff/ffff 非 PCI 全 F；首版 baud@60 读到的是 stop_bits=1）——

pub struct SpcrFacts {
    pub iface: u8,        // 0=16550 1=16550-sub 3=PL011（ACPI 6.x 口径）
    pub addr_space: u8,   // 0=MMIO 1=PIO
    pub base: u64,        // 串口基址（事实表「有无串口 + 型号」主源）
    pub gsi: Option<u32>, // 全局系统中断号（表长 ≥ 58 才有；键盘中断面）
    pub baud: Option<u8>, // 表长 ≥ 59 才有（3 = QEMU 定值；0 = 固件不设）
}

pub fn parse_spcr(b: &[u8]) -> Option<SpcrFacts> {
    if !sig4(b, 0, b"SPCR") {
        return None;
    }
    let len = le_u32(b, 4)? as usize;
    if len < 52 {
        return None; // GAS 收口（@40+12）
    }
    Some(SpcrFacts {
        iface: b[36],
        addr_space: b[40],
        base: le_u64(b, 44)?,
        gsi: if len >= 58 { le_u32(b, 54) } else { None },
        baud: if len >= 59 { Some(b[58]) } else { None },
    })
}

// —— DBG2（sig "DBG2"；info_offset u32@36 → 首设备信息：
//    port_type u16@+12、port_subtype u16@+14、base_address_offset u16@+18，
//    GAS address = 设备 + base_off + 4）——

pub struct Dbg2Facts {
    pub port_type: u16,    // 0x8000 = 串口
    pub port_subtype: u16, // 0=16550 1=16450 3=PL011 …
    pub base: Option<u64>,
}

pub fn parse_dbg2(b: &[u8]) -> Option<Dbg2Facts> {
    if !sig4(b, 0, b"DBG2") {
        return None;
    }
    let info_off = le_u32(b, 36)? as usize;
    if info_off < 22 || info_off + 22 > b.len() {
        return None; // 首设备定长头（@+20 收口）不可读
    }
    let port_type = le_u16(b, info_off + 12)?;
    let port_subtype = le_u16(b, info_off + 14)?;
    let base_off = le_u16(b, info_off + 18)? as usize;
    let gas = info_off.checked_add(base_off)?; // 相对首设备起点
    let base = if gas.checked_add(12)? <= b.len() { le_u64(b, gas + 4) } else { None };
    Some(Dbg2Facts { port_type, port_subtype, base })
}

// —— FDT（唯一大端源：magic 0xD00D_FEED BE@0、totalsize BE@4）——

pub const FDT_MAGIC: u32 = 0xD00D_FEED;

pub struct FdtFacts {
    pub magic_ok: bool,
    pub totalsize: u32,
}

pub fn parse_fdt(b: &[u8]) -> Option<FdtFacts> {
    let m = be_u32(b, 0)?;
    let s = be_u32(b, 4)?;
    Some(FdtFacts { magic_ok: m == FDT_MAGIC, totalsize: s })
}

// —— 名字表（事实表的人读面；未知值返 None → 调用方打 '?'）——

/// SPCR interface_type（ACPI 6.x 口径：PL011 = 3）。
pub fn spcr_iface_name(v: u8) -> Option<&'static str> {
    Some(match v {
        0 => "16550",
        1 => "16550-sub",
        2 => "MAX311xE",
        3 => "PL011",
        4 => "MSM8x60",
        5 => "16550-nv",
        6 => "OMAP",
        7 => "A16650",
        _ => return None,
    })
}

/// GICD version 域（MADT GICD 条目 @+21）。
pub fn gicd_ver_name(v: u8) -> Option<&'static str> {
    Some(match v {
        0 => "none",
        1 => "GICv1",
        2 => "GICv2",
        3 => "GICv3",
        4 => "GICv4",
        _ => return None,
    })
}

/// PCI class 三元（base u8 / sub u8 / prog u8——配置空间 @8 的
/// [23:16]=base [15:8]=sub [7:0]=prog）。只映射 V-5b~V-7 选型相关的
/// 事实面；其余返 None。
pub fn pci_class_name(base: u8, sub: u8, prog: u8) -> Option<&'static str> {
    Some(match (base, sub, prog) {
        (0x01, 0x08, _) => "NVMe",
        (0x01, 0x06, 0x02) => "AHCI",
        (0x01, 0x00, _) => "SCSI",
        (0x01, _, _) => "storage?",
        (0x02, 0x00, _) => "ETH",
        (0x02, _, _) => "net?",
        (0x03, 0x00, _) => "VGA",
        (0x03, 0x02, _) => "3D",
        (0x03, 0x80, _) => "DISP",
        (0x06, 0x00, _) => "HOSTBR",
        (0x06, 0x04, _) => "PCIBR",
        (0x0C, 0x03, 0x00) => "UHCI",
        (0x0C, 0x03, 0x10) => "OHCI",
        (0x0C, 0x03, 0x20) => "EHCI",
        (0x0C, 0x03, 0x30) => "XHCI",
        (0x0C, 0x03, _) => "USB-HC?",
        (0x0C, 0x05, _) => "SDHC",
        (0x09, 0x00, _) => "INPUT",
        _ => return None,
    })
}

// —— host 钉板测试：合成 fixture 按 spec 形状逐字段造（偏移即断言面）。

#[cfg(test)]
mod tests {
    use super::*;

    fn put16(v: &mut Vec<u8>, off: usize, x: u16) {
        v[off..off + 2].copy_from_slice(&x.to_le_bytes());
    }
    fn put32(v: &mut Vec<u8>, off: usize, x: u32) {
        v[off..off + 4].copy_from_slice(&x.to_le_bytes());
    }
    fn put64(v: &mut Vec<u8>, off: usize, x: u64) {
        v[off..off + 8].copy_from_slice(&x.to_le_bytes());
    }

    // —— RSDP ——

    fn rsdp_fixture(rev: u8) -> Vec<u8> {
        let mut v = vec![0u8; 36];
        v[..8].copy_from_slice(SIG_RSDP);
        v[15] = rev;
        if rev >= 2 {
            put64(&mut v, 24, 0x2030_0000);
        }
        v
    }

    #[test]
    fn rsdp_faces() {
        let f = rsdp_xsdt(&rsdp_fixture(2)).unwrap();
        assert_eq!(f.revision, 2);
        assert_eq!(f.xsdt_pa, Some(0x2030_0000));
        // rev 1：无 XSDT 域（32 位 RSDT 时代——xsdt 缺席是事实）。
        let f1 = rsdp_xsdt(&rsdp_fixture(1)).unwrap();
        assert_eq!(f1.revision, 1);
        assert_eq!(f1.xsdt_pa, None);
        // 坏签名 / 过短 → None。
        let mut bad = rsdp_fixture(2);
        bad[0] = b'X';
        assert!(rsdp_xsdt(&bad).is_none());
        assert!(rsdp_xsdt(&rsdp_fixture(2)[..20]).is_none());
    }

    // —— XSDT ——

    #[test]
    fn xsdt_faces() {
        let ptrs = [0x100u64, 0x200, 0x300];
        let mut v = vec![0u8; 36 + ptrs.len() * 8];
        v[..4].copy_from_slice(b"XSDT");
        for (i, p) in ptrs.iter().enumerate() {
            put64(&mut v, 36 + i * 8, *p);
        }
        let len = v.len() as u32;
        put32(&mut v, 4, len);
        let x = xsdt_entries(&v).unwrap();
        assert_eq!(x.n, 3);
        assert_eq!(&x.ptrs[..3], &ptrs);
        assert!(!x.truncated);
        // 声明长超缓冲 → None（宁缺勿炸）。
        let mut bad = v.clone();
        put32(&mut bad, 4, len + 8);
        assert!(xsdt_entries(&bad).is_none());
        // 超 16 条截断标记（走查帽执法）。
        let mut big = vec![0u8; 36 + 20 * 8];
        big[..4].copy_from_slice(b"XSDT");
        let big_len = big.len() as u32;
        put32(&mut big, 4, big_len);
        let x2 = xsdt_entries(&big).unwrap();
        assert_eq!(x2.n, MAX_XSDT_ENTRIES);
        assert!(x2.truncated);
    }

    // —— MADT ——

    #[test]
    fn madt_faces() {
        let mut v = vec![0u8; 44];
        v[..4].copy_from_slice(b"APIC");
        // GICC 禁用（flags@+12 = 0）+ GICC 启用（bit0；PhysBase@+32 =
        // 0x0801_0000 GICv2 CPU Interface）——V-5a-1 真字节裁决偏移执法点。
        for fl in [0u32, 1] {
            let mut e = vec![0u8; 80];
            e[0] = 0x0B;
            put16(&mut e, 1, 80);
            put32(&mut e, 12, fl);
            put64(&mut e, 32, 0x0801_0000);
            v.extend_from_slice(&e);
        }
        // GICD（0x0C）：base u64@+8 / version u8@+20。
        let mut d = vec![0u8; 24];
        d[0] = 0x0C;
        put16(&mut d, 1, 24);
        put64(&mut d, 8, 0x0800_0000);
        d[20] = 2;
        v.extend_from_slice(&d);
        // GICR（0x0E）：base u64@+4（条目源优先于 GICC 内嵌）。
        let mut r = vec![0u8; 16];
        r[0] = 0x0E;
        put16(&mut r, 1, 16);
        put64(&mut r, 4, 0x040A_0000_0000);
        v.extend_from_slice(&r);
        // 畸形尾条目（len=0）——走查截断但已收事实保留。
        v.extend_from_slice(&[0x00, 0x00]);
        let len = v.len() as u32;
        put32(&mut v, 4, len);

        let f = parse_madt(&v).unwrap();
        assert_eq!(f.cpu_total, 2);
        assert_eq!(f.cpu_enabled, 1);
        assert_eq!(f.gicd_version, Some(2));
        assert_eq!(f.gicd_base, Some(0x0800_0000));
        assert_eq!(f.gicc_base, Some(0x0801_0000));
        assert_eq!(f.gicr_base, Some(0x040A_0000_0000));
        // 坏签名 → None。
        let mut bad = v.clone();
        bad[..4].copy_from_slice(b"XPIC");
        assert!(parse_madt(&bad).is_none());
        // 头过短 → None。
        assert!(parse_madt(&v[..44]).is_none());
        // GICv3 双源：无 0x0E 条目时 GICC 内嵌 GICRBase@+60（非零）兜底。
        let mut v3 = vec![0u8; 44];
        v3[..4].copy_from_slice(b"APIC");
        let mut g = vec![0u8; 80];
        g[0] = 0x0B;
        put16(&mut g, 1, 80);
        put32(&mut g, 12, 1);
        put64(&mut g, 60, 0x040A_0000_0000);
        v3.extend_from_slice(&g);
        let l3 = v3.len() as u32;
        put32(&mut v3, 4, l3);
        let f3 = parse_madt(&v3).unwrap();
        assert_eq!(f3.gicr_base, Some(0x040A_0000_0000));
        assert_eq!(f3.gicc_base, Some(0)); // GICv3 GICC PhysBase 恒 0（真值即事实）
    }

    // —— SPCR ——

    #[test]
    fn spcr_faces() {
        let mut v = vec![0u8; 80];
        v[..4].copy_from_slice(b"SPCR");
        put32(&mut v, 4, 80);
        v[36] = 3; // PL011
        v[40] = 0; // MMIO
        put64(&mut v, 44, 0x0900_0000); // GAS address @ 表内 44（GAS+4）
        put32(&mut v, 54, 33); // GSI u32@54（V-5a-1 真字节裁决偏移执法点）
        v[58] = 3; // baud u8@58（QEMU 定值 3）
        let f = parse_spcr(&v).unwrap();
        assert_eq!(f.iface, 3);
        assert_eq!(f.addr_space, 0);
        assert_eq!(f.base, 0x0900_0000);
        assert_eq!(f.gsi, Some(33));
        assert_eq!(f.baud, Some(3));
        assert_eq!(spcr_iface_name(f.iface), Some("PL011"));
        // 短表（< 52）无 GAS → None；58 前无 GSI、59 前无 baud → 两域 None。
        let mut s = vec![0u8; 57];
        s[..4].copy_from_slice(b"SPCR");
        put32(&mut s, 4, 57);
        s[36] = 0;
        put64(&mut s, 44, 0x3F8);
        let g = parse_spcr(&s).unwrap();
        assert_eq!(g.baud, None);
        assert_eq!(g.gsi, None);
        assert_eq!(g.iface, 0);
    }

    // —— DBG2 ——

    #[test]
    fn dbg2_faces() {
        // 表头 44B（info_offset@36=44、info_count@40=1）+ 首设备 22B +
        // GAS 12B（base_address_offset=22 → GAS 紧随设备头）。
        let mut v = vec![0u8; 44 + 22 + 12];
        v[..4].copy_from_slice(b"DBG2");
        let dlen = v.len() as u32;
        put32(&mut v, 4, dlen);
        put32(&mut v, 36, 44);
        let dev = 44;
        put16(&mut v, dev + 1, 34); // 设备全长（头 + GAS）
        put16(&mut v, dev + 12, 0x8000); // port_type = 串口
        put16(&mut v, dev + 14, 3); // PL011
        put16(&mut v, dev + 18, 22); // base_address_offset（相对设备起点）
        let gas = dev + 22;
        put64(&mut v, gas + 4, 0x0900_0000); // GAS.address @ GAS+4
        let f = parse_dbg2(&v).unwrap();
        assert_eq!(f.port_type, 0x8000);
        assert_eq!(f.port_subtype, 3);
        assert_eq!(f.base, Some(0x0900_0000));
        // info_offset 越界 → None（宁缺勿炸）。
        let mut bad = v.clone();
        put32(&mut bad, 36, v.len() as u32 + 100);
        assert!(parse_dbg2(&bad).is_none());
    }

    // —— FDT ——

    #[test]
    fn fdt_faces() {
        let mut v = vec![0u8; 8];
        v[..4].copy_from_slice(&FDT_MAGIC.to_be_bytes());
        v[4..8].copy_from_slice(&3072u32.to_be_bytes());
        let f = parse_fdt(&v).unwrap();
        assert!(f.magic_ok);
        assert_eq!(f.totalsize, 3072);
        // 坏 magic：不炸、magic_ok=false（大小端搞反的活证面）。
        v[..4].copy_from_slice(&FDT_MAGIC.to_le_bytes());
        let f2 = parse_fdt(&v).unwrap();
        assert!(!f2.magic_ok);
        // 过短 → None。
        assert!(parse_fdt(&v[..4]).is_none());
    }

    // —— 名字表 ——

    #[test]
    fn name_tables() {
        assert_eq!(gicd_ver_name(3), Some("GICv3"));
        assert_eq!(gicd_ver_name(2), Some("GICv2"));
        assert_eq!(gicd_ver_name(9), None);
        assert_eq!(spcr_iface_name(0), Some("16550"));
        assert_eq!(spcr_iface_name(9), None);
        assert_eq!(pci_class_name(0x01, 0x08, 2), Some("NVMe"));
        assert_eq!(pci_class_name(0x0C, 0x03, 0x30), Some("XHCI"));
        assert_eq!(pci_class_name(0x0C, 0x03, 0x99), Some("USB-HC?"));
        assert_eq!(pci_class_name(0x02, 0x00, 0), Some("ETH"));
        assert_eq!(pci_class_name(0xFF, 0xFF, 0xFF), None);
    }
}
