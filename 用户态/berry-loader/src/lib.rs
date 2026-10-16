//! `berry-loader`——rootfs 解析纯函数库（G-eco-c，草案 D5「解析器即产品」）。
//!
//! 两个解析器，零分配、零 unsafe、零依赖，输入只借用：
//! - [`cpio_walk`]：cpio newc 流式迭代（`TRAILER!!!` 自终止；`vmo_get_size`
//!   给的是**页对齐容量**，尾零页天然被 trailer 截住，不需 blob 实长）；
//! - [`elf_parse`]：ELF64 little-endian / ET_EXEC / AArch64 静态镜像解析，
//!   逐 PT_LOAD 出段（v1 段帽 [`MAX_SEGMENTS`]，越过诚实拒——不猜语义）。
//!
//! 消费端 = svcmgr（rootfs VMO → 本库解析 → process_create/vmar_map/
//! process_start 拉起）。错误面（截断/坏魔数/野值）全部 host 单测盖——
//! 「AI 测试友好化」在生态关的落点：解析正确性不依赖 QEMU 就能验证。
//!
//! v1 明确不收（返回 [`LoadError::NotSupported`]，不 panic 不猜）：ET_DYN、
//! 32 位 / 大端、非 AArch64、`e_phentsize != 56`、PT_LOAD 数超帽、非 UTF-8
//! 文件名。注意 **p_vaddr 不要求页对齐**——真实 ld（rust-lld）会把 RO 段
//! 放在 0x10c0 这类段内偏移（off ≡ vaddr mod p_align 即合法，且与 text 段
//! 共享页）；页对齐是**消费端**约束，svcmgr 以「页铺平单 VMO」消化（同
//! 工具链/elf2flat.py 语义）。filesz ≤ memsz 纪律保留（.bss = memsz 尾部
//! 由 VMO 零初始化页免费覆盖）。

#![cfg_attr(not(test), no_std)]
#![deny(static_mut_refs)]

/// 页大小（vmar_map 映射基址/长度粒度，与内核 PAGE_SIZE 同值）。
pub const PAGE_SIZE: u64 = 4096;
/// 用户地址空间上界（copy_from/to_user 界检 `[0x1000, 2^39)` 同源）。
pub const USER_TOP: u64 = 1 << 39;
/// v1 单镜像 PT_LOAD 段帽（固定数组零分配；越过 = NotSupported 诚实拒）。
pub const MAX_SEGMENTS: usize = 8;

/// 解析错误。语义分层：长度问题（Truncated）/ 魔数问题（BadMagic）/
/// 字段野值（BadField）/ v1 不支持的面（NotSupported）/ 段几何非法（BadSegment）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LoadError {
    /// 声称的长度越出输入（截断的 cpio 头 / ELF 段越过文件尾）。
    Truncated,
    /// 魔数不对（非 `070701`/`070702`；非 `\x7fELF`）。
    BadMagic,
    /// 字段野值（hex 位非十六进制 / 名字尾非 NUL / 名字非 UTF-8）。
    BadField,
    /// v1 明确不收：见模块头清单。
    NotSupported,
    /// PT_LOAD 几何非法：filesz > memsz / vaddr+memsz 越 USER_TOP。
    BadSegment,
}

// ==================== cpio newc ====================

/// cpio 条目（名字不含 NUL；data 为字节切片借用）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct CpioEntry<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
}

/// `TRAILER!!!` 条目名（归档终止符；walk 到此返回 `None`）。
pub const CPIO_TRAILER: &str = "TRAILER!!!";

/// newc 头长：6 字节魔数 + 13 × 8 hex 字段。
const CPIO_HDR: usize = 110;

/// cpio newc 迭代器。空输入 / 全零尾（页对齐容量垫的零）= 干净结束；
/// trailer 后的内容（含归档垫零）不再看。
pub struct CpioIter<'a> {
    buf: &'a [u8],
    pos: usize,
    done: bool,
}

/// 流式遍历 cpio newc 归档。逐条 `Ok(entry)`；`Ok(None)` 终止；`Err` 即停
/// （首个坏条目上抛——消费端整体降级，不半途装载）。
pub fn cpio_walk(buf: &[u8]) -> CpioIter<'_> {
    CpioIter { buf, pos: 0, done: false }
}

impl<'a> Iterator for CpioIter<'a> {
    type Item = Result<CpioEntry<'a>, LoadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let buf = self.buf;
        // 尾部不足一个头：全零 = 页垫尾（干净结束），非零 = 截断。
        if self.pos + CPIO_HDR > buf.len() {
            self.done = true;
            return if buf[self.pos..].iter().all(|&b| b == 0) {
                None
            } else {
                Some(Err(LoadError::Truncated))
            };
        }
        let h = &buf[self.pos..self.pos + CPIO_HDR];
        if &h[..4] != b"0707" || h[4] != b'0' || (h[5] != b'1' && h[5] != b'2') {
            self.done = true;
            return Some(Err(LoadError::BadMagic));
        }
        // 13 字段：ino mode uid gid nlink mtime filesize devmaj devmin
        //          rdevmaj rdevmin namesize check（check v1 不校验）。
        let field = |i: usize| -> Result<u64, LoadError> {
            let s = &h[6 + i * 8..6 + i * 8 + 8];
            let mut v: u64 = 0;
            for &c in s {
                let d = match c {
                    b'0'..=b'9' => c - b'0',
                    b'a'..=b'f' => c - b'a' + 10,
                    b'A'..=b'F' => c - b'A' + 10,
                    _ => return Err(LoadError::BadField),
                };
                v = v * 16 + d as u64;
            }
            Ok(v)
        };
        let (filesize, namesize) = match (field(6), field(11)) {
            (Ok(f), Ok(n)) => (f, n), // u64 全程算，恶意巨值 checked 兜住
            (Err(e), _) | (_, Err(e)) => {
                self.done = true;
                return Some(Err(e));
            }
        };
        if namesize == 0 {
            self.done = true;
            return Some(Err(LoadError::BadField));
        }
        // 全 u64 checked 几何：header → 名（垫 4）→ 数据（垫 4）→ 下条。
        let name_off = (self.pos + CPIO_HDR) as u64;
        let data_off = name_off
            .checked_add(namesize)
            .and_then(|v| v.checked_add(3))
            .map(|v| v & !3);
        let data_end = data_off.and_then(|d| d.checked_add(filesize));
        let next = data_end.map(|e| (e + 3) & !3);
        let (data_off, next) = match (data_off, next) {
            (Some(d), Some(n)) => (d, n),
            _ => {
                self.done = true;
                return Some(Err(LoadError::Truncated)); // 声称长度溢出 = 越界
            }
        };
        if next > buf.len() as u64 {
            self.done = true;
            return Some(Err(LoadError::Truncated));
        }
        let (name_off, namesize, filesize, data_off) =
            (name_off as usize, namesize as usize, filesize as usize, data_off as usize);
        let name_bytes = &buf[name_off..name_off + namesize];
        if name_bytes[namesize - 1] != 0 {
            self.done = true;
            return Some(Err(LoadError::BadField));
        }
        let name = match core::str::from_utf8(&name_bytes[..namesize - 1]) {
            Ok(s) => s,
            Err(_) => {
                self.done = true;
                return Some(Err(LoadError::BadField));
            }
        };
        let data = &buf[data_off..data_off + filesize];
        self.pos = next as usize;
        if name == CPIO_TRAILER {
            self.done = true;
            return None;
        }
        Some(Ok(CpioEntry { name, data }))
    }
}

// ==================== ELF64 ====================

/// 一个 PT_LOAD 段：映射基址 `vaddr`、映射长 `memsz`（页粒度由消费端取整）、
/// `data` = 文件字节（len == filesz；filesz < memsz 的尾部 = .bss，VMO 零页覆盖）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Segment<'a> {
    pub vaddr: u64,
    pub memsz: u64,
    pub data: &'a [u8],
}

/// 解析出的可加载镜像：入口 + PT_LOAD 段（固定帽数组，[`ElfImage::segments`]
/// 切片视图）。
#[derive(Debug, PartialEq)]
pub struct ElfImage<'a> {
    pub entry: u64,
    segs: [Segment<'a>; MAX_SEGMENTS],
    count: usize,
}

impl<'a> ElfImage<'a> {
    pub fn segments(&self) -> &[Segment<'a>] {
        &self.segs[..self.count]
    }
}

fn rd_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn rd_u64(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}

/// 解析 ELF64 LE / ET_EXEC / AArch64 静态镜像。校验面见模块头；只挑
/// PT_LOAD（其余 phdr 类型跳过——PT_NOTE 等对装载无语义）。
pub fn elf_parse(elf: &[u8]) -> Result<ElfImage<'_>, LoadError> {
    if elf.len() < 64 {
        return Err(LoadError::Truncated);
    }
    if &elf[..4] != b"\x7fELF" {
        return Err(LoadError::BadMagic);
    }
    if elf[4] != 2 || elf[5] != 1 || elf[6] != 1 {
        return Err(LoadError::NotSupported); // 非 64 位 / 非小端 / 版本
    }
    let e_type = rd_u16(elf, 16);
    let e_machine = rd_u16(elf, 18);
    if e_type != 2 {
        return Err(LoadError::NotSupported); // v1 只收 ET_EXEC（固定基址）
    }
    if e_machine != 183 {
        return Err(LoadError::NotSupported); // EM_AARCH64
    }
    let entry = rd_u64(elf, 24);
    let phoff = rd_u64(elf, 32) as usize;
    let phentsize = rd_u16(elf, 54) as usize;
    let phnum = rd_u16(elf, 56) as usize;
    if entry == 0 || entry >= USER_TOP || phoff == 0 || phentsize != 56 {
        return Err(LoadError::NotSupported);
    }
    let phend = phoff.checked_add(phnum.checked_mul(56).ok_or(LoadError::NotSupported)?)
        .ok_or(LoadError::NotSupported)?;
    if phend > elf.len() {
        return Err(LoadError::Truncated);
    }
    let mut segs = [Segment { vaddr: 0, memsz: 0, data: &[] }; MAX_SEGMENTS];
    let mut count = 0usize;
    for i in 0..phnum {
        let p = &elf[phoff + i * 56..phoff + (i + 1) * 56];
        if rd_u32(p, 0) != 1 {
            continue; // 非 PT_LOAD
        }
        let offset = rd_u64(p, 8);
        let vaddr = rd_u64(p, 16);
        let filesz = rd_u64(p, 32);
        let memsz = rd_u64(p, 40);
        if memsz == 0 {
            continue; // 空段（padding LOAD）容忍跳过
        }
        if count == MAX_SEGMENTS {
            return Err(LoadError::NotSupported); // 段帽：诚实拒
        }
        if filesz > memsz
            || vaddr.checked_add(memsz).map_or(true, |e| e > USER_TOP)
        {
            return Err(LoadError::BadSegment);
        }
        let off = offset as usize;
        let fsz = filesz as usize;
        if off.checked_add(fsz).map_or(true, |e| e > elf.len()) {
            return Err(LoadError::Truncated);
        }
        segs[count] = Segment { vaddr, memsz, data: &elf[off..off + fsz] };
        count += 1;
    }
    if count == 0 {
        return Err(LoadError::NotSupported); // 无 PT_LOAD = 无可装载内容
    }
    Ok(ElfImage { entry, segs, count })
}

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

// ==================== host 单元测试（AI 测试友好化：解析面不依赖 QEMU） ====================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 测试侧构造器：手搭 newc / ELF 字节 ----

    fn hex8(mut v: u64) -> [u8; 8] {
        let mut s = [0u8; 8];
        for i in (0..8).rev() {
            s[i] = match v & 0xF {
                d @ 0..=9 => b'0' + d as u8,
                d => b'A' + d as u8 - 10,
            };
            v >>= 4;
        }
        s
    }

    fn cpio_entry(name: &str, data: &[u8], ino: u64) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"070701");
        for v in [ino, 0o100755, 0, 0, 1, 0, data.len() as u64, 0, 0, 0, 0,
                  name.len() as u64 + 1, 0] {
            out.extend_from_slice(&hex8(v));
        }
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out
    }

    fn cpio_archive(items: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, (n, d)) in items.iter().enumerate() {
            out.extend_from_slice(&cpio_entry(n, d, i as u64 + 1));
        }
        out.extend_from_slice(&cpio_entry(CPIO_TRAILER, b"", 0));
        out
    }

    fn phdr(ptype: u32, offset: u64, vaddr: u64, filesz: u64, memsz: u64) -> Vec<u8> {
        let mut p = vec![0u8; 56];
        p[0..4].copy_from_slice(&ptype.to_le_bytes());
        p[4..8].copy_from_slice(&5u32.to_le_bytes()); // p_flags RX
        p[8..16].copy_from_slice(&offset.to_le_bytes());
        p[16..24].copy_from_slice(&vaddr.to_le_bytes());
        p[32..40].copy_from_slice(&filesz.to_le_bytes());
        p[40..48].copy_from_slice(&memsz.to_le_bytes());
        p
    }

    fn ehdr(phoff: u64, phnum: u16, phdrs: &[u8], e_type: u16, machine: u16) -> Vec<u8> {
        let mut e = vec![0u8; 64];
        e[..4].copy_from_slice(b"\x7fELF");
        e[4] = 2; e[5] = 1; e[6] = 1; // 64 / LE / version
        e[16..18].copy_from_slice(&e_type.to_le_bytes());
        e[18..20].copy_from_slice(&machine.to_le_bytes());
        e[24..32].copy_from_slice(&0x1000u64.to_le_bytes()); // e_entry
        e[32..40].copy_from_slice(&phoff.to_le_bytes());
        e[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
        e[56..58].copy_from_slice(&phnum.to_le_bytes());
        let mut out = e;
        out.extend_from_slice(phdrs);
        out
    }

    fn elf_one_load(content: &[u8], memsz: u64) -> Vec<u8> {
        // 单 PT_LOAD @0x1000 内嵌整个文件（标准形态：首段含 ehdr/phdr +
        // 内容，p_offset=0）。filesz = 文件全长。
        let total = 64 + 56 + content.len();
        let p = phdr(1, 0, 0x1000, total as u64, memsz);
        let mut f = ehdr(64, 1, &p, 2, 183);
        f.extend_from_slice(content);
        f
    }

    // ---- cpio 面 ----

    #[test]
    fn cpio_empty_and_zero_tail_end_cleanly() {
        assert_eq!(cpio_walk(b"").count(), 0);
        assert_eq!(cpio_walk(&[0u8; 64]).count(), 0); // 页垫零
    }

    #[test]
    fn cpio_single_entry_roundtrip() {
        let arc = cpio_archive(&[("bin/hello", b"ABC")]);
        let got: Vec<_> = cpio_walk(&arc).collect();
        assert_eq!(got.len(), 1);
        let e = got[0].as_ref().unwrap();
        assert_eq!((e.name, e.data), ("bin/hello", &b"ABC"[..]));
    }

    #[test]
    fn cpio_trailer_terminates_and_ignores_tail() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc.extend_from_slice(&cpio_entry("after", b"?", 9)); // trailer 后的垃圾不看
        arc.extend_from_slice(&[0u8; 8]);
        assert_eq!(cpio_walk(&arc).count(), 1);
    }

    #[test]
    fn cpio_odd_name_and_data_alignment() {
        // 名 2B（奇）+ 数据 5B（非 4 倍）→ 两 entry 均可读回。
        let arc = cpio_archive(&[("a", b"12345"), ("b", b"67")]);
        let got: Vec<_> = cpio_walk(&arc).map(|e| e.unwrap()).collect();
        assert_eq!(got[0].data, b"12345");
        assert_eq!(got[1].data, b"67");
    }

    #[test]
    fn cpio_bad_magic() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[0] = b'9';
        assert_eq!(cpio_walk(&arc).next(), Some(Err(LoadError::BadMagic)));
    }

    #[test]
    fn cpio_magic_crc_variant_accepted() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[5] = b'2'; // 070702（CRC 变体）收
        assert!(cpio_walk(&arc).next().unwrap().is_ok());
    }

    #[test]
    fn cpio_namesize_lie_truncated() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        // namesize 字段 @6+11*8=94，改成 0xFF0 撒谎。
        arc[94..102].copy_from_slice(&hex8(0xFF0));
        assert_eq!(cpio_walk(&arc).next(), Some(Err(LoadError::Truncated)));
    }

    #[test]
    fn cpio_filesize_lie_truncated() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[54..62].copy_from_slice(&hex8(1 << 20));
        assert_eq!(cpio_walk(&arc).next(), Some(Err(LoadError::Truncated)));
    }

    #[test]
    fn cpio_nonzero_garbage_tail_truncated() {
        // 无 trailer、非零残尾（手搭单 entry 不加 trailer——cpio_archive 会补）。
        let mut arc = cpio_entry("a", b"x", 1);
        arc.extend_from_slice(b"garbage!!");
        assert_eq!(cpio_walk(&arc).last(), Some(Err(LoadError::Truncated)));
    }

    #[test]
    fn cpio_name_missing_nul_bad_field() {
        let mut a = cpio_archive(&[("a", b"x")]);
        a[111] = b'z'; // 名 = "a\0"（namesize 2），NUL 位改非零
        assert_eq!(cpio_walk(&a).next(), Some(Err(LoadError::BadField)));
    }

    #[test]
    fn cpio_hostile_huge_filesize_no_panic() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[54..62].copy_from_slice(&hex8(u64::MAX)); // 溢出面：不许 panic
        assert_eq!(cpio_walk(&arc).next(), Some(Err(LoadError::Truncated)));
    }

    #[test]
    fn cpio_hostile_huge_namesize_no_panic() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[94..102].copy_from_slice(&hex8(u64::MAX));
        assert_eq!(cpio_walk(&arc).next(), Some(Err(LoadError::Truncated)));
    }

    #[test]
    fn cpio_hex_garbage_bad_field() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[61] = b'x'; // filesize 字段（54..62）末位非 hex
        assert_eq!(cpio_walk(&arc).next(), Some(Err(LoadError::BadField)));
    }

    #[test]
    fn cpio_empty_file_entry() {
        let arc = cpio_archive(&[("dir/", b"")]);
        let e = cpio_walk(&arc).next().unwrap().unwrap();
        assert_eq!((e.name, e.data), ("dir/", &b""[..]));
    }

    // ---- ELF 面 ----

    #[test]
    fn elf_minimal_single_load() {
        let f = elf_one_load(b"CODE", 0x1000);
        let img = elf_parse(&f).unwrap();
        assert_eq!(img.entry, 0x1000);
        assert_eq!(img.segments().len(), 1);
        let s = &img.segments()[0];
        assert_eq!((s.vaddr, s.memsz), (0x1000, 0x1000));
        assert_eq!(s.data.len(), f.len()); // filesz = 文件全长（首段含头）
        assert_eq!(&s.data[..4], b"\x7fELF"); // 头在段首（标准形态）
        assert_eq!(&s.data[120..], b"CODE"); // 内容紧随 phdr
    }

    #[test]
    fn elf_bss_memsz_tail_beyond_filesz_ok() {
        let f = elf_one_load(b"AB", 0x3000); // memsz 3 页，filesz 68B
        let img = elf_parse(&f).unwrap();
        assert_eq!(img.segments()[0].memsz, 0x3000);
    }

    #[test]
    fn elf_truncated_under_ehdr() {
        assert_eq!(elf_parse(&[0u8; 63]), Err(LoadError::Truncated));
    }

    #[test]
    fn elf_bad_magic() {
        let mut f = elf_one_load(b"C", 0x1000);
        f[0] = 0x7e;
        assert_eq!(elf_parse(&f), Err(LoadError::BadMagic));
    }

    #[test]
    fn elf_32bit_class_not_supported() {
        let mut f = elf_one_load(b"C", 0x1000);
        f[4] = 1;
        assert_eq!(elf_parse(&f), Err(LoadError::NotSupported));
    }

    #[test]
    fn elf_dyn_not_supported() {
        let p = phdr(1, 0, 0x1000, 8, 0x1000);
        let f = ehdr(64, 1, &p, 3, 183); // ET_DYN
        assert_eq!(elf_parse(&f), Err(LoadError::NotSupported));
    }

    #[test]
    fn elf_wrong_machine_not_supported() {
        let p = phdr(1, 0, 0x1000, 8, 0x1000);
        let f = ehdr(64, 1, &p, 2, 62); // EM_X86_64
        assert_eq!(elf_parse(&f), Err(LoadError::NotSupported));
    }

    #[test]
    fn elf_unaligned_vaddr_accepted_parser() {
        // 真实 ld 形态（rust-lld RO 段 vaddr=0x10c0）：off ≡ vaddr (mod align)
        // 即合法——解析器收，页对齐由消费端页铺平消化（svcmgr spawn_elf）。
        let p = phdr(1, 0x80, 0x1080, 8, 0x1000);
        let mut f = ehdr(64, 1, &p, 2, 183);
        f.extend_from_slice(&[0u8; 0x88]);
        let img = elf_parse(&f).unwrap();
        assert_eq!(img.segments()[0].vaddr, 0x1080);
    }

    #[test]
    fn elf_filesz_over_memsz_bad_segment() {
        let p = phdr(1, 0, 0x1000, 0x2000, 0x1000);
        let f = ehdr(64, 1, &p, 2, 183);
        assert_eq!(elf_parse(&f), Err(LoadError::BadSegment));
    }

    #[test]
    fn elf_vaddr_over_user_top_bad_segment() {
        let p = phdr(1, 0, 1 << 39, 8, 0x1000);
        let f = ehdr(64, 1, &p, 2, 183);
        assert_eq!(elf_parse(&f), Err(LoadError::BadSegment));
    }

    #[test]
    fn elf_segment_beyond_file_truncated() {
        let p = phdr(1, 0x100, 0x1000, 0x1000, 0x1000); // offset 越文件尾
        let f = ehdr(64, 1, &p, 2, 183);
        assert_eq!(elf_parse(&f), Err(LoadError::Truncated));
    }

    #[test]
    fn elf_non_load_phdrs_skipped() {
        // PT_NOTE(4) + PT_LOAD：只出 1 段。
        let mut ph = phdr(4, 0, 0, 0x20, 0x20);
        ph.extend_from_slice(&phdr(1, 0, 0x1000, 8, 0x1000));
        let f = ehdr(64, 2, &ph, 2, 183);
        let img = elf_parse(&f).unwrap();
        assert_eq!(img.segments().len(), 1);
    }

    #[test]
    fn elf_multi_load_ordered() {
        let mut ph = phdr(1, 0, 0x1000, 0x10, 0x1000);
        ph.extend_from_slice(&phdr(1, 0x10, 0x2000, 0x10, 0x2000));
        let mut f = ehdr(64, 2, &ph, 2, 183);
        f.extend_from_slice(&[0u8; 0x20]); // 补段文件字节
        let img = elf_parse(&f).unwrap();
        assert_eq!(img.segments().len(), 2);
        assert_eq!(img.segments()[1].vaddr, 0x2000);
    }

    #[test]
    fn elf_too_many_loads_not_supported() {
        let mut ph = Vec::new();
        for i in 0..=MAX_SEGMENTS {
            ph.extend_from_slice(&phdr(1, 0, 0x1000 + (i as u64) * 0x1000, 8, 0x1000));
        }
        let f = ehdr(64, (MAX_SEGMENTS + 1) as u16, &ph, 2, 183);
        assert_eq!(elf_parse(&f), Err(LoadError::NotSupported));
    }

    #[test]
    fn elf_no_load_not_supported() {
        let p = phdr(4, 0, 0, 0x20, 0x20); // 只有 PT_NOTE
        let f = ehdr(64, 1, &p, 2, 183);
        assert_eq!(elf_parse(&f), Err(LoadError::NotSupported));
    }
}
