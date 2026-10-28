//! berrydump——Fusion/AAVMF 设备面事实 dump 应用（V-5a-0，VM 草案 §4）。
//!
//! 定位：**纯取证工具**（S-0/fw_cfg 方法论——先拿事实再写驱动码）。经
//! ConOut 打机器可 grep 的行式事实（`fw:`/`gop:`/`cfg:`/`acpi:`/`xsdt:`/
//! `madt:`/`spcr:`/`dbg2:`/`fdt:`/`pci:`/`hcs:` 前缀 + `madtraw:`/`spcrraw:`
//! 原始 hex 锚——跨固件表形异时的裁决源），消费端 = 人/脚本
//! 对拍回填 `docs/VM里程碑设计草案.md` 附录 B。**缺席显式打印**（`xxx:
//! absent`——机器可读缺席报告纪律）；**不 EBS**（dump 完返回
//! EFI_SUCCESS，ConOut 全程活——与 BerryBoot 的 EBS 后直写复杂度无涉）。
//!
//! 产链 = build.sh（berryboot 镜像复刻：staticlib → rust-lld 单 .img →
//! elf2pe.py → out/BERRYDUMP.EFI）；部署 = FAT 卷 `\EFI\BOOT\BOOTAA64.EFI`
//! （可移动介质默认路径，与 BerryBoot 同形）。独立 workspace（不进
//! berryboot members——dump 工具编译错不得耦合进 `just build-boot`/
//! `just image` 主链；berry-efi 跨 workspace path dep 合法——纯数据 crate
//! 无构建副作用）。

#![cfg_attr(not(test), no_std)]

pub mod parse;

use berry_efi::{
    fmt_u32_dec, fmt_u64_hex, EfiBootServices, EfiConfigurationTable,
    EfiGraphicsOutputModeInfo, EfiGraphicsOutputProtocol, EfiGuid, EfiPciIoProtocol,
    EfiSimpleTextOutputProtocol, EfiStatus, EfiSystemTable, EFI_ACPI20_TABLE_GUID,
    EFI_BLOCK_IO_PROTOCOL_GUID, EFI_BOOT_SERVICES_SIGNATURE, EFI_DTB_TABLE_GUID,
    EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID, EFI_LOCATE_BY_PROTOCOL, EFI_NOT_FOUND,
    EFI_PCI_IO_PROTOCOL_GUID, EFI_PCI_IO_WIDTH_UINT32, EFI_SIMPLE_NETWORK_PROTOCOL_GUID,
    EFI_SUCCESS, EFI_SYSTEM_TABLE_SIGNATURE,
};
use core::ffi::c_void;

#[cfg(not(test))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

const BANNER: &str = "BerryDump V-5a device facts (V-5a-0)";
/// 配置表走查帽（真实平台 ≤ 10 条；防畸形巨型表拖死 dump）。
const CFG_WALK_CAP: usize = 32;
/// GOP 逐模式枚举帽（V-6 fbcon 选分辨率的事实集；8 档足够选型）。
const GOP_MODE_CAP: u32 = 8;
/// PCI 设备逐打帽 + 溢出计数行（事实表关心型号集，不关心全拓扑）。
const PCI_PRINT_CAP: usize = 16;

// —— 行组装（berryboot/app Line 克隆 + 定宽 hex 扩展；UTF-16 桥最后一步）——

struct Line {
    buf: [u8; 128],
    n: usize,
}

impl Line {
    fn new() -> Self {
        Line { buf: [0; 128], n: 0 }
    }
    fn push(&mut self, s: &[u8]) {
        for &c in s {
            if self.n == self.buf.len() {
                break;
            }
            self.buf[self.n] = c;
            self.n += 1;
        }
    }
    fn push_str(&mut self, s: &str) {
        self.push(s.as_bytes());
    }
    fn push_u32(&mut self, v: u32) {
        let mut t = [0u8; 10];
        self.push(fmt_u32_dec(v, &mut t));
    }
    fn push_hex(&mut self, v: u64) {
        let mut t = [0u8; 16];
        self.push(b"0x");
        self.push(fmt_u64_hex(v, &mut t));
    }
    /// 定宽 n 位小写 hex（class 码 6 位 / ven·dev 4 位——定宽对齐人读）。
    fn push_hexn(&mut self, v: u64, digits: usize) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for i in 0..digits {
            let shift = (digits - 1 - i) * 4;
            let mut b = [0u8; 1];
            b[0] = HEX[((v >> shift) & 0xF) as usize];
            self.push(&b);
        }
    }
}

/// ConOut 行打印：Line → UTF-16（ASCII 拓宽）+ CRLF + NUL 终止（wbuf 尾
/// 余零即终止，berryboot 同款）。
fn print_line(con_out: &EfiSimpleTextOutputProtocol, f: impl FnOnce(&mut Line)) {
    let mut l = Line::new();
    f(&mut l);
    let mut wbuf = [0u16; 144];
    let mut n = 0usize;
    while n < l.n {
        wbuf[n] = l.buf[n] as u16;
        n += 1;
    }
    wbuf[n] = 0x000D; // CR
    wbuf[n + 1] = 0x000A; // LF
    unsafe { (con_out.output_string)(con_out as *const _ as *mut _, wbuf.as_ptr()) };
}

// —— 固件表物理读（有界裸读——ConfigurationTable 报告的表指针按 UEFI
//    契约有效；长度帽全在调用方 sanity 层）——

unsafe fn fw_bytes(pa: u64, len: usize) -> Option<&'static [u8]> {
    if pa == 0 || len == 0 {
        return None;
    }
    Some(core::slice::from_raw_parts(pa as *const u8, len))
}

/// 表头 36B 预读 + length 域 sanity（36..=65536）+ 全表切片。None = 畸形
/// （调用方打 `bad` 行——与 `absent` 区分：在场但读不得也是事实）。
unsafe fn read_table(pa: u64) -> Option<&'static [u8]> {
    let hdr = fw_bytes(pa, 36)?;
    let len = parse::le_u32(hdr, 4)? as usize;
    if !(36..=0x1_0000).contains(&len) {
        return None;
    }
    fw_bytes(pa, len)
}

/// 固件厂商 UCS-2 → ASCII 有损解码（帽 40 字符——事实行装得下即可）。
unsafe fn vendor_ascii(p: *const u16, out: &mut [u8]) -> usize {
    let mut n = 0usize;
    while n < out.len() {
        let c = *p.add(n);
        if c == 0 {
            break;
        }
        out[n] = if c < 0x80 { c as u8 } else { b'?' };
        n += 1;
    }
    n
}

/// LocateHandleBuffer 计数（NOT_FOUND → 0；其余错原样上抛——事实面不吞错）。
unsafe fn lhb_count(bs: &EfiBootServices, guid: &EfiGuid) -> Result<usize, EfiStatus> {
    let mut no_h = 0usize;
    let mut buf: *mut *mut c_void = core::ptr::null_mut();
    let r = (bs.locate_handle_buffer)(
        EFI_LOCATE_BY_PROTOCOL,
        guid as *const EfiGuid as *mut EfiGuid,
        core::ptr::null_mut(),
        &mut no_h,
        &mut buf,
    );
    if r != EFI_SUCCESS {
        return Err(r);
    }
    if !buf.is_null() {
        (bs.free_pool)(buf as *mut c_void); // 数组即刻回收——只要计数
    }
    Ok(no_h)
}

// —— 应用入口 ——

#[no_mangle]
pub extern "C" fn efi_main(
    _image_handle: *mut c_void,
    system_table: *mut EfiSystemTable,
) -> EfiStatus {
    if system_table.is_null() {
        return EFI_SUCCESS;
    }
    let st = unsafe { &*system_table };
    if st.hdr.signature != EFI_SYSTEM_TABLE_SIGNATURE || st.con_out.is_null() {
        return EFI_SUCCESS;
    }
    let con_out = unsafe { &*st.con_out };
    let bs: Option<&EfiBootServices> = if !st.boot_services.is_null()
        && unsafe { (*st.boot_services).hdr.signature } == EFI_BOOT_SERVICES_SIGNATURE
    {
        Some(unsafe { &*st.boot_services })
    } else {
        None
    };

    // 行 1：横幅（镜像被执行的阳证）。
    print_line(con_out, |l| l.push_str(BANNER));

    // 行 2：固件厂商 + 修订（hdr.revision 高 16 位 = UEFI 主版本）。
    print_line(con_out, |l| {
        l.push_str("fw: ");
        if !st.firmware_vendor.is_null() {
            let mut v = [0u8; 40];
            let n = unsafe { vendor_ascii(st.firmware_vendor, &mut v) };
            l.push(&v[..n]);
        } else {
            l.push_str("-");
        }
        l.push_str(" fw=");
        l.push_hex(st.firmware_revision as u64);
        l.push_str(" uefi=");
        l.push_hex(st.hdr.revision as u64);
    });

    // 行 3..：GOP——当前模式 + 逐模式枚举（V-6 fbcon 选分辨率事实集）。
    let mut gop: *mut EfiGraphicsOutputProtocol = core::ptr::null_mut();
    if let Some(bs) = bs {
        let r = unsafe {
            (bs.locate_protocol)(
                &EFI_GRAPHICS_OUTPUT_PROTOCOL_GUID as *const EfiGuid as *mut EfiGuid,
                core::ptr::null_mut(),
                &mut gop as *mut *mut EfiGraphicsOutputProtocol as *mut *mut c_void,
            )
        };
        if r == EFI_SUCCESS && !gop.is_null() {
            let mode = unsafe { &*(*gop).mode };
            let info = unsafe { &*mode.info };
            print_line(con_out, |l| {
                l.push_str("gop: ");
                l.push_u32(info.horizontal_resolution);
                l.push(b"x");
                l.push_u32(info.vertical_resolution);
                l.push_str(" fmt=");
                l.push_u32(info.pixel_format);
                l.push_str(" m=");
                l.push_u32(mode.mode);
                l.push(b"/");
                l.push_u32(mode.max_mode);
                l.push_str(" fb=");
                l.push_hex(mode.frame_buffer_base);
                l.push_str(" st=");
                l.push_u32(info.pixels_per_scan_line);
            });
            for m in 0..mode.max_mode.min(GOP_MODE_CAP) {
                let mut sz = 0usize;
                let mut mi: *mut EfiGraphicsOutputModeInfo = core::ptr::null_mut();
                let r = unsafe {
                    ((*gop).query_mode)(
                        gop,
                        m as usize,
                        &mut sz,
                        &mut mi,
                    )
                };
                if r == EFI_SUCCESS && !mi.is_null() {
                    let i = unsafe { &*mi };
                    print_line(con_out, |l| {
                        l.push_str("gop[");
                        l.push_u32(m);
                        l.push_str("]: ");
                        l.push_u32(i.horizontal_resolution);
                        l.push(b"x");
                        l.push_u32(i.vertical_resolution);
                        l.push_str(" fmt=");
                        l.push_u32(i.pixel_format);
                        l.push_str(" st=");
                        l.push_u32(i.pixels_per_scan_line);
                    });
                } else {
                    print_line(con_out, |l| {
                        l.push_str("gop[");
                        l.push_u32(m);
                        l.push_str("]: q=");
                        l.push_hex(r as u64);
                    });
                }
            }
            if mode.max_mode > GOP_MODE_CAP {
                print_line(con_out, |l| {
                    l.push_str("gop: more=");
                    l.push_u32(mode.max_mode - GOP_MODE_CAP);
                });
            }
        } else {
            print_line(con_out, |l| l.push_str("gop: absent"));
        }
    } else {
        print_line(con_out, |l| l.push_str("gop: bs absent"));
    }

    // 行 N：配置表扫描——ACPI 2.0 / DTB 入口分类（发现机制判别第一层）。
    let mut n_acpi20 = 0u32;
    let mut n_dtb = 0u32;
    let mut n_other = 0u32;
    let mut acpi_pa: u64 = 0;
    let mut fdt_pa: u64 = 0;
    let cfg_n = st.number_of_table_entries.min(CFG_WALK_CAP);
    if !st.configuration_table.is_null() {
        let ct = st.configuration_table as *const EfiConfigurationTable;
        for i in 0..cfg_n {
            let ent = unsafe { &*ct.add(i) };
            if ent.vendor_guid == EFI_ACPI20_TABLE_GUID {
                n_acpi20 += 1;
                acpi_pa = ent.vendor_table as u64;
            } else if ent.vendor_guid == EFI_DTB_TABLE_GUID {
                n_dtb += 1;
                fdt_pa = ent.vendor_table as u64;
            } else {
                n_other += 1;
            }
        }
    }
    print_line(con_out, |l| {
        l.push_str("cfg: n=");
        l.push_u32(cfg_n as u32);
        l.push_str(" acpi20=");
        l.push_u32(n_acpi20);
        l.push_str(" dtb=");
        l.push_u32(n_dtb);
        l.push_str(" other=");
        l.push_u32(n_other);
    });

    // ACPI 路：RSDP → XSDT → 签名清单 + APIC/SPCR/DBG2 定位 → 全表读 → parse。
    let mut madt_pa: u64 = 0;
    let mut spcr_pa: u64 = 0;
    let mut dbg2_pa: u64 = 0;
    if acpi_pa != 0 {
        let rsdp = unsafe { fw_bytes(acpi_pa, 36) }.and_then(parse::rsdp_xsdt);
        match rsdp.as_ref().and_then(|r| r.xsdt_pa) {
            Some(xsdt_pa) => {
                let entries = unsafe { read_table(xsdt_pa) }.and_then(parse::xsdt_entries);
                match entries {
                    Some(x) => {
                        print_line(con_out, |l| {
                            l.push_str("xsdt: n=");
                            l.push_u32(x.n as u32);
                            if x.truncated {
                                l.push_str("+");
                            }
                            l.push(b" [");
                            for i in 0..x.n {
                                if i > 0 {
                                    l.push(b" ");
                                }
                                if let Some(hdr) = unsafe { fw_bytes(x.ptrs[i], 4) } {
                                    l.push(hdr);
                                } else {
                                    l.push_str("????");
                                }
                            }
                            l.push(b"]");
                        });
                        for i in 0..x.n {
                            let sig = match unsafe { fw_bytes(x.ptrs[i], 4) } {
                                Some(s) => [s[0], s[1], s[2], s[3]],
                                None => continue,
                            };
                            if &sig == b"APIC" {
                                madt_pa = x.ptrs[i];
                            } else if &sig == b"SPCR" {
                                spcr_pa = x.ptrs[i];
                            } else if &sig == b"DBG2" {
                                dbg2_pa = x.ptrs[i];
                            }
                        }
                    }
                    None => print_line(con_out, |l| l.push_str("xsdt: bad")),
                }
            }
            None => print_line(con_out, |l| {
                if rsdp.is_some() {
                    l.push_str("acpi: no xsdt (rev<2?)");
                } else {
                    l.push_str("acpi: rsdp bad");
                }
            }),
        }
    } else {
        print_line(con_out, |l| l.push_str("acpi: absent"));
    }

    // MADT：CPU 数 + GIC 拓扑（附录 B 首行事实）。
    if madt_pa != 0 {
        // MADT 原始 hex 锚（madtraw 每行 40B——Line 帽 128：前缀 9 + 80
        // hex = 89 恒安全；首版 60B/行 = 129 越帽每行丢 1 字符、拼接后半
        // 字节错位）。**常驻行**（非临时调试）：V-5a-1 靠真实字节裁决了
        // packed 偏移（合成 fixture 与代码同错自洽的 elf2pe 教训）；Fusion
        // 会话遇固件表形异（行式值可疑）时，raw 是唯一裁决源——ACPI 三
        // 表（MADT/SPCR/XSDT）中仅 MADT 体量值得锚（172B/5 行）。
        if let Some(t) = unsafe { read_table(madt_pa) } {
            for c in 0..5 {
                let s = c * 40;
                if s >= t.len() {
                    break;
                }
                let e = (s + 40).min(t.len());
                print_line(con_out, |l| {
                    l.push_str("madtraw: ");
                    for i in s..e {
                        l.push_hexn(t[i] as u64, 2);
                    }
                });
            }
        }
        match unsafe { read_table(madt_pa) }.and_then(parse::parse_madt) {
            Some(m) => print_line(con_out, |l| {
                l.push_str("madt: cpu=");
                l.push_u32(m.cpu_enabled);
                l.push(b"/");
                l.push_u32(m.cpu_total);
                l.push_str(" gicd=");
                // gicd 版本（名字）+ 基址 + gicr 基址；缺项打 '-'。
                match m.gicd_version {
                    Some(v) => {
                        l.push_u32(v as u32);
                        if let Some(name) = parse::gicd_ver_name(v) {
                            l.push_str("(");
                            l.push_str(name);
                            l.push(b")");
                        }
                    }
                    None => l.push(b"-"),
                }
                l.push_str(" gicd@");
                match m.gicd_base {
                    Some(a) => l.push_hex(a),
                    None => l.push(b"-"),
                }
                l.push_str(" gicc@");
                match m.gicc_base {
                    Some(a) => l.push_hex(a),
                    None => l.push(b"-"),
                }
                l.push_str(" gicr@");
                match m.gicr_base {
                    Some(a) => l.push_hex(a),
                    None => l.push(b"-"),
                }
            }),
            None => print_line(con_out, |l| l.push_str("madt: bad")),
        }
    } else {
        print_line(con_out, |l| l.push_str("madt: absent"));
    }

    // SPCR：串口有无 + 型号 + 基址（键盘降级线成立与否的主判据）。
    if spcr_pa != 0 {
        // SPCR @36 起尾部 hex 锚（常驻行同 madtraw——baud@58/GSI@54 即靠
        // 本行真字节裁决：Linux acpi_table_spcr 视图，首版 baud@60 读到
        // stop_bits=1）。
        if let Some(t) = unsafe { read_table(spcr_pa) } {
            let e = t.len().min(88);
            if e > 36 {
                print_line(con_out, |l| {
                    l.push_str("spcrraw: ");
                    for i in 36..e {
                        l.push_hexn(t[i] as u64, 2);
                    }
                });
            }
        }
        match unsafe { read_table(spcr_pa) }.and_then(parse::parse_spcr) {
            Some(s) => print_line(con_out, |l| {
                l.push_str("spcr: iface=");
                l.push_u32(s.iface as u32);
                if let Some(name) = parse::spcr_iface_name(s.iface) {
                    l.push_str("(");
                    l.push_str(name);
                    l.push(b")");
                }
                l.push_str(" base=");
                l.push_hex(s.base);
                l.push_str(" sp=");
                l.push_u32(s.addr_space as u32);
                l.push_str(" gsi=");
                match s.gsi {
                    Some(g) => l.push_u32(g),
                    None => l.push(b"-"),
                }
                l.push_str(" baud=");
                match s.baud {
                    Some(b) => l.push_u32(b as u32),
                    None => l.push(b"-"),
                }
            }),
            None => print_line(con_out, |l| l.push_str("spcr: bad")),
        }
    } else {
        print_line(con_out, |l| l.push_str("spcr: absent"));
    }

    // DBG2：调试口（SPCR 的旁证——两者交叉验证串口事实）。
    if dbg2_pa != 0 {
        match unsafe { read_table(dbg2_pa) }.and_then(parse::parse_dbg2) {
            Some(d) => print_line(con_out, |l| {
                l.push_str("dbg2: type=");
                l.push_hexn(d.port_type as u64, 4);
                l.push_str(" sub=");
                l.push_hexn(d.port_subtype as u64, 4);
                l.push_str(" base=");
                match d.base {
                    Some(a) => l.push_hex(a),
                    None => l.push(b"-"),
                }
            }),
            None => print_line(con_out, |l| l.push_str("dbg2: bad")),
        }
    } else {
        print_line(con_out, |l| l.push_str("dbg2: absent"));
    }

    // FDT：DT 发现机制的直接证据（magic 校验——大小端搞错的活证面）。
    if fdt_pa != 0 {
        match unsafe { fw_bytes(fdt_pa, 8) }.and_then(parse::parse_fdt) {
            Some(f) => print_line(con_out, |l| {
                l.push_str("fdt: magic=");
                if f.magic_ok {
                    l.push_str("ok");
                } else {
                    l.push_str("bad");
                }
                l.push_str(" size=");
                l.push_u32(f.totalsize);
            }),
            None => print_line(con_out, |l| l.push_str("fdt: short")),
        }
    } else {
        print_line(con_out, |l| l.push_str("fdt: absent"));
    }

    // PCI：PciIo 句柄枚举 + Uint32 配置空间读（ven/dev/class 三元）——
    // 附录 B 盘控制器/USB HC/网卡三行的主源。
    if let Some(bs) = bs {
        let mut no_h = 0usize;
        let mut hbuf: *mut *mut c_void = core::ptr::null_mut();
        let r = unsafe {
            (bs.locate_handle_buffer)(
                EFI_LOCATE_BY_PROTOCOL,
                &EFI_PCI_IO_PROTOCOL_GUID as *const EfiGuid as *mut EfiGuid,
                core::ptr::null_mut(),
                &mut no_h,
                &mut hbuf,
            )
        };
        if r == EFI_SUCCESS && !hbuf.is_null() {
            print_line(con_out, |l| {
                l.push_str("pci: n=");
                l.push_u32(no_h as u32);
            });
            for i in 0..no_h.min(PCI_PRINT_CAP) {
                let h = unsafe { *hbuf.add(i) };
                let mut iface: *mut c_void = core::ptr::null_mut();
                let hp = unsafe {
                    (bs.handle_protocol)(
                        h,
                        &EFI_PCI_IO_PROTOCOL_GUID as *const EfiGuid as *mut EfiGuid,
                        &mut iface,
                    )
                };
                if hp != EFI_SUCCESS || iface.is_null() {
                    // 失败 loud（假缺席比噪声糟——handle 协议查不到本身是事实）。
                    print_line(con_out, |l| {
                        l.push_str("pcih[");
                        l.push_u32(i as u32);
                        l.push_str("]: st=");
                        l.push_hex(hp as u64);
                    });
                    continue;
                }
                let pio = unsafe { &*(iface as *const EfiPciIoProtocol) };
                let mut r0 = 0u32;
                let mut r8 = 0u32;
                let st0 = unsafe {
                    (pio.pci_read)(
                        iface as *mut EfiPciIoProtocol,
                        EFI_PCI_IO_WIDTH_UINT32,
                        0,
                        1,
                        &mut r0 as *mut u32 as *mut c_void,
                    )
                };
                let st8 = unsafe {
                    (pio.pci_read)(
                        iface as *mut EfiPciIoProtocol,
                        EFI_PCI_IO_WIDTH_UINT32,
                        8,
                        1,
                        &mut r8 as *mut u32 as *mut c_void,
                    )
                };
                if st0 == EFI_SUCCESS && st8 == EFI_SUCCESS {
                    // BDF 定位（lspci 对拍形状——段③ 交叉验证面）。
                    let (mut seg, mut bus, mut dev_, mut fun) = (0usize, 0usize, 0usize, 0usize);
                    let _ = unsafe {
                        (pio.get_location)(
                            iface as *mut EfiPciIoProtocol,
                            &mut seg,
                            &mut bus,
                            &mut dev_,
                            &mut fun,
                        )
                    };
                    let ven = (r0 & 0xFFFF) as u64;
                    let dev = ((r0 >> 16) & 0xFFFF) as u64;
                    let base = ((r8 >> 24) & 0xFF) as u8;
                    let sub = ((r8 >> 16) & 0xFF) as u8;
                    let prog = ((r8 >> 8) & 0xFF) as u8;
                    let class6 = (base as u64) << 16 | (sub as u64) << 8 | prog as u64;
                    print_line(con_out, |l| {
                        l.push_str("pci[");
                        l.push_u32(i as u32);
                        l.push_str("]: loc=");
                        l.push_u32(bus as u32);
                        l.push(b"/");
                        l.push_u32(dev_ as u32);
                        l.push(b"/");
                        l.push_u32(fun as u32);
                        l.push_str(" ven=");
                        l.push_hexn(ven, 4);
                        l.push_str(" dev=");
                        l.push_hexn(dev, 4);
                        l.push_str(" c=");
                        l.push_hexn(class6, 6);
                        l.push(b" ");
                        match parse::pci_class_name(base, sub, prog) {
                            Some(n) => l.push_str(n),
                            None => l.push(b"?"),
                        }
                    });
                } else {
                    // 配置读失败 loud——静默跳过 = 假缺席（事实表漏行）。
                    print_line(con_out, |l| {
                        l.push_str("pcir[");
                        l.push_u32(i as u32);
                        l.push_str("]: s0=");
                        l.push_hex(st0 as u64);
                        l.push_str(" s8=");
                        l.push_hex(st8 as u64);
                    });
                }
            }
            if no_h > PCI_PRINT_CAP {
                print_line(con_out, |l| {
                    l.push_str("pci: more=");
                    l.push_u32((no_h - PCI_PRINT_CAP) as u32);
                });
            }
            unsafe { (bs.free_pool)(hbuf as *mut c_void) };
        } else if r == EFI_NOT_FOUND {
            print_line(con_out, |l| l.push_str("pci: n=0"));
        } else {
            print_line(con_out, |l| {
                l.push_str("pci: lhb=");
                l.push_hex(r as u64);
            });
        }

        // 协议族计数（旁证——USB 事实主源是 PCI class 0C03，此处不含
        // Usb2Hc：GUID 单源置信不足，宁缺勿假）。
        let blk = unsafe { lhb_count(bs, &EFI_BLOCK_IO_PROTOCOL_GUID) };
        let net = unsafe { lhb_count(bs, &EFI_SIMPLE_NETWORK_PROTOCOL_GUID) };
        print_line(con_out, |l| {
            l.push_str("hcs: blk=");
            match blk {
                Ok(n) => l.push_u32(n as u32),
                Err(e) => l.push_hex(e as u64),
            }
            l.push_str(" net=");
            match net {
                Ok(n) => l.push_u32(n as u32),
                Err(e) => l.push_hex(e as u64),
            }
        });
    } else {
        print_line(con_out, |l| l.push_str("pci: bs absent"));
    }

    print_line(con_out, |l| l.push_str("BerryDump done"));
    EFI_SUCCESS
}

// —— host 钉板测试：Line 组装面（fmt 助手本体已在 berry-efi 钉板）。

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_push_and_hexn() {
        let mut l = Line::new();
        l.push_str("pci[0]: loc=");
        l.push_u32(2);
        l.push_str(" ven=");
        l.push_hexn(0x106B, 4);
        l.push_str(" c=");
        l.push_hexn(0x0C0330, 6);
        l.push(b" ");
        l.push_str("XHCI");
        assert_eq!(&l.buf[..l.n], b"pci[0]: loc=2 ven=106b c=0c0330 XHCI");
        // 定宽零填充：ven=0x12 → "0012"。
        let mut l2 = Line::new();
        l2.push_hexn(0x12, 4);
        assert_eq!(&l2.buf[..l2.n], b"0012");
        // 溢出截断：128B 帽保头不炸。
        let mut l3 = Line::new();
        for _ in 0..40 {
            l3.push_str("abcd"); // 160B 意图
        }
        assert_eq!(l3.n, 128);
    }
}
