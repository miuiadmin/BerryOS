//! `berry-loader`——rootfs 解析纯函数库（G-eco-c，草案 D5「解析器即产品」）。
//!
//! 解析器两对，零分配、零 unsafe、零依赖，输入只借用：
//! - 整档面（G-eco-c）：[`cpio_walk`]（cpio newc 流式迭代，`TRAILER!!!`
//!   自终止）+ [`elf_parse`]（ELF64 LE / ET_EXEC / AArch64 静态镜像解析，
//!   逐 PT_LOAD 出段，v1 段帽 [`MAX_SEGMENTS`] 越过诚实拒）；
//! - 流式面（G-eco-d d-1b）：[`cpio_step`]（单条目步进——头+名进窗、
//!   数据按偏移算术跳过，std 镜像 ~283KB 归档永不整档驻留）+
//!   [`elf_head`]（首窗解析装载几何——**跨度按 p_memsz**（.bss 计入，
//!   stdhello RW 段 memsz=0x41241 实测）+ [`ElfHead::file_len_min`] 文件
//!   侧下界，段文件字节由消费端分块泵送）。
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

// ==================== cpio 步进（流式消费面，G-eco-d d-1b） ====================
//
// std 程序 ELF ~283KB，rootfs VMO 远超 svcmgr 的 16KiB 静态窗——但 cpio
// 遍历只需要每条目的「头 + 名」（数据按偏移算术跳过，永不进窗）。本节
// 把 [`cpio_walk`] 的几何拆成单步纯函数：消费端（svcmgr）自管
// `vmo_read` 开窗，逐步推进。

/// 一步遍历的结果：条目（数据**不在窗内**，只给归档内偏移）或干净终止。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum StepOutcome<'a> {
    /// 命中一个条目（trailer 折叠进 [`StepOutcome::End`]——消费端本来就要停）。
    Entry(CpioStep<'a>),
    /// 干净终止：`TRAILER!!!` 或全零尾（页垫零）。
    End,
}

/// cpio 单条目几何：`data_off`/`data_len` 是**归档内**的文件数据窗口，
/// `next_off` 是下一条目头位置。消费端按 data_off 分块泵送，永不需要
/// 整档驻留。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct CpioStep<'a> {
    pub name: &'a str,
    pub data_off: u64,
    pub data_len: u64,
    pub next_off: u64,
}

/// 解析 `window`（= 归档内 `pos` 处开窗读到的字节，约定
/// `window.len() == min(消费端窗宽, archive_len - pos)`）里的一条 cpio 头。
///
/// 契约：窗宽须盖住「头 110B + 名字」——mkcpio 名字 ≤ 56B，512B 窗绰绰
/// 有余；盖不住按 [`LoadError::Truncated`] 诚实拒（名字撒谎同理）。
/// 文件数据范围只对 `archive_len` 校验（数据无需在窗内）。
pub fn cpio_step(window: &[u8], pos: u64, archive_len: u64) -> Result<StepOutcome<'_>, LoadError> {
    let tail = match archive_len.checked_sub(pos) {
        Some(t) if t > 0 => t, // 契约：pos < archive_len（调用方循环条件）
        _ => return Err(LoadError::Truncated),
    };
    if window.is_empty() || window.len() as u64 > tail {
        return Err(LoadError::Truncated); // 开窗契约违约（读越过归档尾）
    }
    if tail < CPIO_HDR as u64 {
        // 尾部不足一个头：全零 = 页垫尾（干净结束），非零 = 截断。
        return if window.iter().all(|&b| b == 0) {
            Ok(StepOutcome::End)
        } else {
            Err(LoadError::Truncated)
        };
    }
    if window.len() < CPIO_HDR {
        return Err(LoadError::Truncated); // 窗盖不住头（消费端窗太窄）
    }
    let h = &window[..CPIO_HDR];
    if &h[..4] != b"0707" || h[4] != b'0' || (h[5] != b'1' && h[5] != b'2') {
        return Err(LoadError::BadMagic);
    }
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
        (Ok(f), Ok(n)) => (f, n),
        (Err(e), _) | (_, Err(e)) => return Err(e),
    };
    if namesize == 0 {
        return Err(LoadError::BadField);
    }
    let name_off = pos.checked_add(CPIO_HDR as u64).ok_or(LoadError::Truncated)?;
    let data_off = name_off
        .checked_add(namesize)
        .and_then(|v| v.checked_add(3))
        .map(|v| v & !3);
    let data_end = data_off.and_then(|d| d.checked_add(filesize));
    let next = data_end.and_then(|e| e.checked_add(3)).map(|v| v & !3);
    let (data_off, next) = match (data_off, next) {
        (Some(d), Some(n)) => (d, n),
        _ => return Err(LoadError::Truncated), // 恶意巨值溢出
    };
    if next > archive_len {
        return Err(LoadError::Truncated);
    }
    // 名字须在窗内（数据不必）。
    let name_rel_end = CPIO_HDR as u64 + namesize;
    if name_rel_end > window.len() as u64 {
        return Err(LoadError::Truncated);
    }
    let name_bytes = &window[CPIO_HDR..CPIO_HDR + namesize as usize];
    if name_bytes[namesize as usize - 1] != 0 {
        return Err(LoadError::BadField);
    }
    let name = match core::str::from_utf8(&name_bytes[..namesize as usize - 1]) {
        Ok(s) => s,
        Err(_) => return Err(LoadError::BadField),
    };
    if name == CPIO_TRAILER {
        return Ok(StepOutcome::End);
    }
    Ok(StepOutcome::Entry(CpioStep {
        name,
        data_off,
        data_len: filesize,
        next_off: next,
    }))
}

// ==================== ELF64 头面（流式装载几何，G-eco-d d-1b） ====================
//
// std 镜像的装载跨度由 **p_memsz** 决定（RW 段 memsz = 文件字节 + 256KiB
// .bss 堆——实测 stdhello 0x41241），文件字节只占其中前端。本节从
// 「首页字节 + 声称的文件长」解析装载几何，段文件字节由消费端分块泵送。

/// 一个 PT_LOAD 段的装载几何（文件字节位置 + 映射位置/长度）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct SegHead {
    pub file_off: u64,
    pub vaddr: u64,
    pub filesz: u64,
    pub memsz: u64,
}

/// 流式装载几何：入口 + 段表 + 三个推导量（全 checked，host 单测盖）。
#[derive(Debug, PartialEq)]
pub struct ElfHead {
    pub entry: u64,
    segs: [SegHead; MAX_SEGMENTS],
    count: usize,
    /// 映像最低 vaddr（vmar_map 基址 = `page_floor(image_min)`）。
    pub image_min: u64,
    /// 装载跨度 = max(vaddr + memsz) − image_min（**memsz 口径**：.bss
    /// 计入；页数 = `page_ceil(image_span)`）。
    pub image_span: u64,
    /// 文件侧下界 = max(file_off + filesz)：cpio 条目 data_len 须 ≥ 此值
    /// （泵送前一次性验明，不给截断留到半程）。
    pub file_len_min: u64,
}

impl ElfHead {
    pub fn segments(&self) -> &[SegHead] {
        &self.segs[..self.count]
    }
}

/// 页底对齐（vmar_map 基址粒度）。
pub fn page_floor(v: u64) -> u64 {
    v & !(PAGE_SIZE - 1)
}

/// 页顶对齐（映射长度粒度）。
pub fn page_ceil(v: u64) -> u64 {
    v.checked_add(PAGE_SIZE - 1).map(|x| x & !(PAGE_SIZE - 1)).unwrap_or(u64::MAX)
}

/// 从镜像**首窗字节**（ehdr + 全部 phdr；首页 4KB 对真实 lld 产物绰绰
/// 有余——stdhello phnum=9）+ 声称的文件总长解析装载几何。
///
/// 与 [`elf_parse`] 的校验差异：段文件范围对 `file_len`（声称值）而非
/// 缓冲长度校验；另收两条装载端诚实面——`image_min ≥ 0x1000`（EL0 界，
/// [`USER_TOP`] 同源）与入口落在映像跨度内（跳飞 = 子进程自陷死，
/// 装载端拦下比运行期死诚实）。
pub fn elf_head(head: &[u8], file_len: u64) -> Result<ElfHead, LoadError> {
    if head.len() < 64 {
        return Err(LoadError::Truncated);
    }
    if &head[..4] != b"\x7fELF" {
        return Err(LoadError::BadMagic);
    }
    if head[4] != 2 || head[5] != 1 || head[6] != 1 {
        return Err(LoadError::NotSupported);
    }
    if rd_u16(head, 16) != 2 {
        return Err(LoadError::NotSupported); // v1 只收 ET_EXEC
    }
    if rd_u16(head, 18) != 183 {
        return Err(LoadError::NotSupported); // EM_AARCH64
    }
    let entry = rd_u64(head, 24);
    let phoff = rd_u64(head, 32) as usize;
    let phentsize = rd_u16(head, 54) as usize;
    let phnum = rd_u16(head, 56) as usize;
    if entry == 0 || phoff == 0 || phentsize != 56 {
        return Err(LoadError::NotSupported);
    }
    let phend = phoff.checked_add(phnum.checked_mul(56).ok_or(LoadError::NotSupported)?)
        .ok_or(LoadError::NotSupported)?;
    if phend > head.len() {
        return Err(LoadError::Truncated); // phdr 越出首窗
    }
    let mut segs = [SegHead { file_off: 0, vaddr: 0, filesz: 0, memsz: 0 }; MAX_SEGMENTS];
    let mut count = 0usize;
    let mut image_min = u64::MAX;
    let mut image_max_end = 0u64;
    let mut file_len_min = 0u64;
    for i in 0..phnum {
        let p = &head[phoff + i * 56..phoff + (i + 1) * 56];
        if rd_u32(p, 0) != 1 {
            continue; // 非 PT_LOAD
        }
        let file_off = rd_u64(p, 8);
        let vaddr = rd_u64(p, 16);
        let filesz = rd_u64(p, 32);
        let memsz = rd_u64(p, 40);
        if memsz == 0 {
            continue; // 空 padding 段容忍
        }
        if count == MAX_SEGMENTS {
            return Err(LoadError::NotSupported);
        }
        let vend = vaddr.checked_add(memsz).ok_or(LoadError::BadSegment)?;
        let fend = file_off.checked_add(filesz).ok_or(LoadError::BadSegment)?;
        if filesz > memsz || vend > USER_TOP || vaddr < PAGE_SIZE {
            return Err(LoadError::BadSegment); // 含 EL0 下界检
        }
        if fend > file_len {
            return Err(LoadError::Truncated); // 声称的文件长盖不住段
        }
        segs[count] = SegHead { file_off, vaddr, filesz, memsz };
        count += 1;
        image_min = image_min.min(vaddr);
        image_max_end = image_max_end.max(vend);
        file_len_min = file_len_min.max(fend);
    }
    if count == 0 {
        return Err(LoadError::NotSupported);
    }
    let image_span = image_max_end - image_min; // count>0 且各段已过界检，不溢出
    if entry < image_min || entry >= image_max_end {
        return Err(LoadError::BadField); // 入口在映像外 = 跳飞死，装载端拦
    }
    Ok(ElfHead { entry, segs, count, image_min, image_span, file_len_min })
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

    // ---- cpio 步进面（d-1b：流式消费几何） ----

    /// 以固定窗宽驱动 `cpio_step` 走完整个归档（模拟 svcmgr 的 vmo_read 开窗）。
    fn step_drive(arc: &[u8], win: usize) -> Vec<(String, Vec<u8>)> {
        let mut pos = 0u64;
        let mut out = Vec::new();
        while pos < arc.len() as u64 {
            let end = (pos + win as u64).min(arc.len() as u64) as usize;
            let w = &arc[pos as usize..end];
            match cpio_step(w, pos, arc.len() as u64).unwrap() {
                StepOutcome::End => break,
                StepOutcome::Entry(s) => {
                    out.push((
                        s.name.to_string(),
                        arc[s.data_off as usize..(s.data_off + s.data_len) as usize].to_vec(),
                    ));
                    pos = s.next_off;
                }
            }
        }
        out
    }

    #[test]
    fn cpio_step_equivalent_to_walk() {
        // 窗宽取最小可靠值（头 110 + 名 ≤ 56 有余）；奇名/奇数据逼出对齐路径。
        let mut arc = cpio_archive(&[("bin/hello", b"12345"), ("b", b"67"), ("dir/", b"")]);
        arc.extend_from_slice(&[0u8; 16]); // 页垫零尾（trailer 后不看，但驱动器会撞到）
        let via_step = step_drive(&arc, 160);
        let via_walk: Vec<_> = cpio_walk(&arc)
            .map(|e| (e.unwrap().name.to_string(), e.unwrap().data.to_vec()))
            .collect();
        assert_eq!(via_step, via_walk);
        assert_eq!(via_step.len(), 3);
    }

    #[test]
    fn cpio_step_geometry_arithmetic() {
        // "bin/hello"（9B+NUL=10）：name_off=110 → data_off=align4(120)=120，
        // 数据 4B → next=align4(124)=124。
        let arc = cpio_archive(&[("bin/hello", b"ABCD")]);
        match cpio_step(&arc, 0, arc.len() as u64).unwrap() {
            StepOutcome::Entry(s) => {
                assert_eq!(s.name, "bin/hello");
                assert_eq!((s.data_off, s.data_len, s.next_off), (120, 4, 124));
            }
            _ => panic!("expected entry"),
        }
    }

    #[test]
    fn cpio_step_data_need_not_be_in_window() {
        // 流式本义：4KiB 数据、窗只开 128B——几何照常出，数据偏移可信。
        let arc = cpio_archive(&[("big", &[7u8; 4096])]);
        let w = &arc[..128];
        match cpio_step(w, 0, arc.len() as u64).unwrap() {
            StepOutcome::Entry(s) => {
                assert_eq!(s.name, "big");
                assert_eq!(s.data_len, 4096);
                assert_eq!(&arc[s.data_off as usize..][..8], &[7u8; 8]);
            }
            _ => panic!("expected entry"),
        }
    }

    #[test]
    fn cpio_step_trailer_is_end() {
        let arc = cpio_archive(&[("a", b"x")]);
        // 手进到 trailer 头位置（单条目 next）。
        let first = match cpio_step(&arc, 0, arc.len() as u64).unwrap() {
            StepOutcome::Entry(s) => s.next_off,
            _ => panic!("expected entry"),
        };
        let w = &arc[first as usize..];
        assert_eq!(cpio_step(w, first, arc.len() as u64).unwrap(), StepOutcome::End);
    }

    #[test]
    fn cpio_step_zero_tail_end_nonzero_truncated() {
        // 无 trailer：数据后直接页垫零 → 零尾干净结束；非零残尾 = 截断。
        let mut arc = cpio_entry("a", b"x", 1);
        let entry_end = arc.len() as u64;
        arc.extend_from_slice(&[0u8; 32]);
        let w = &arc[entry_end as usize..];
        assert_eq!(cpio_step(w, entry_end, arc.len() as u64).unwrap(), StepOutcome::End);
        let nz: Vec<u8> = [1u8; 40].to_vec();
        assert_eq!(
            cpio_step(&nz, 0, nz.len() as u64),
            Err(LoadError::Truncated)
        );
    }

    #[test]
    fn cpio_step_window_too_small_for_header_truncated() {
        let arc = cpio_archive(&[("a", b"x")]);
        // 尾 ≥ 110 但窗只 64B：消费端窗太窄（区别于归档截断，同错误面诚实拒）。
        assert_eq!(cpio_step(&arc[..64], 0, arc.len() as u64), Err(LoadError::Truncated));
    }

    #[test]
    fn cpio_step_name_beyond_window_truncated() {
        let arc = cpio_archive(&[("bin/hello", b"x")]);
        // 窗到 111B：头 110 全在，名（10B）只盖 1B。
        assert_eq!(cpio_step(&arc[..111], 0, arc.len() as u64), Err(LoadError::Truncated));
    }

    #[test]
    fn cpio_step_pos_at_or_beyond_end_truncated() {
        let arc = cpio_archive(&[("a", b"x")]);
        assert_eq!(cpio_step(&arc, arc.len() as u64, arc.len() as u64), Err(LoadError::Truncated));
        assert_eq!(cpio_step(&arc, arc.len() as u64 + 8, arc.len() as u64), Err(LoadError::Truncated));
    }

    #[test]
    fn cpio_step_next_beyond_archive_truncated() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[54..62].copy_from_slice(&hex8(1 << 20)); // filesize 撒谎：next 越归档
        assert_eq!(cpio_step(&arc, 0, arc.len() as u64), Err(LoadError::Truncated));
    }

    #[test]
    fn cpio_step_hostile_huge_filesize_no_panic() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[54..62].copy_from_slice(&hex8(u64::MAX)); // 溢出面：不许 panic
        assert_eq!(cpio_step(&arc, 0, arc.len() as u64), Err(LoadError::Truncated));
    }

    #[test]
    fn cpio_step_hostile_huge_namesize_no_panic() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[94..102].copy_from_slice(&hex8(u64::MAX));
        assert_eq!(cpio_step(&arc, 0, arc.len() as u64), Err(LoadError::Truncated));
    }

    #[test]
    fn cpio_step_bad_magic_and_bad_field_parity() {
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[0] = b'9';
        assert_eq!(cpio_step(&arc, 0, arc.len() as u64), Err(LoadError::BadMagic));
        let mut arc = cpio_archive(&[("a", b"x")]);
        arc[111] = b'z'; // 名尾 NUL 位破坏
        assert_eq!(cpio_step(&arc, 0, arc.len() as u64), Err(LoadError::BadField));
    }

    // ---- ELF 头面（d-1b：流式装载几何） ----

    #[test]
    fn elf_head_stdhello_shape_geometry() {
        // stdhello 形态浓缩：RX/R/RW 三段，RW 段 memsz 远超 filesz（.bss 堆）。
        let mut ph = phdr(1, 0, 0x200000, 0x8000, 0x8000);
        ph.extend_from_slice(&phdr(1, 0x8000, 0x208000, 0x1000, 0x1000));
        ph.extend_from_slice(&phdr(1, 0x9000, 0x2098c0, 0x40, 0x41241));
        let mut head = ehdr(64, 3, &ph, 2, 183);
        head[24..32].copy_from_slice(&0x200100u64.to_le_bytes()); // entry 入映像
        // 首窗只有 ehdr+phdr（232B），声称文件长 0x9040——数据不进窗。
        let h = elf_head(&head, 0x9040).unwrap();
        assert_eq!(h.entry, 0x200100);
        assert_eq!(h.segments().len(), 3);
        assert_eq!((h.image_min, h.image_span), (0x200000, 0x24AB01 - 0x200000));
        assert_eq!(h.file_len_min, 0x9040);
        assert_eq!(
            h.segments()[2],
            SegHead { file_off: 0x9000, vaddr: 0x2098c0, filesz: 0x40, memsz: 0x41241 }
        );
        // 装载端推导：基址 = 页底(min)，页数 = 页顶(span)。
        assert_eq!(page_floor(h.image_min), 0x200000);
        assert_eq!(page_ceil(h.image_span), 0x4B000);
    }

    #[test]
    fn elf_head_agrees_with_elf_parse() {
        let f = elf_one_load(b"CODE", 0x3000);
        let h = elf_head(&f, f.len() as u64).unwrap();
        let p = elf_parse(&f).unwrap();
        assert_eq!(h.entry, p.entry);
        assert_eq!(h.segments().len(), p.segments().len());
        assert_eq!(h.segments()[0].vaddr, p.segments()[0].vaddr);
        assert_eq!(h.segments()[0].memsz, p.segments()[0].memsz);
        assert_eq!(h.segments()[0].filesz as usize, p.segments()[0].data.len());
        assert_eq!(h.file_len_min as usize, f.len());
    }

    #[test]
    fn elf_head_bss_span_uses_memsz() {
        // filesz 0x80 / memsz 0x3000：跨度按 memsz（.bss 计入）。
        let p = phdr(1, 0, 0x1000, 0x80, 0x3000);
        let head = ehdr(64, 1, &p, 2, 183);
        let h = elf_head(&head, 0x80).unwrap();
        assert_eq!((h.image_min, h.image_span), (0x1000, 0x3000));
    }

    #[test]
    fn elf_head_unaligned_min_vaddr_accepted() {
        let p = phdr(1, 0x80, 0x1080, 8, 0x1000); // 真实 lld RO 段形态
        let mut head = ehdr(64, 1, &p, 2, 183);
        head[24..32].copy_from_slice(&0x1084u64.to_le_bytes()); // entry 须入映像
        let h = elf_head(&head, 0x88).unwrap();
        assert_eq!(h.image_min, 0x1080);
        assert_eq!(page_floor(h.image_min), 0x1000); // 消费端页铺平基址
    }

    #[test]
    fn elf_head_entry_outside_span_bad_field() {
        // entry 0x1000 在映像 [0x1080, 0x2080) 之外 = 跳飞死，装载端拦。
        let p = phdr(1, 0x80, 0x1080, 8, 0x1000);
        let head = ehdr(64, 1, &p, 2, 183); // 默认 entry 0x1000
        assert_eq!(elf_head(&head, 0x88), Err(LoadError::BadField));
    }

    #[test]
    fn elf_head_vaddr_below_page_bad_segment() {
        // vaddr 0x800：页底 0 = EL0 界下（copy_from/to_user 下界 0x1000 同源）。
        let p = phdr(1, 0, 0x800, 8, 0x1000);
        let head = ehdr(64, 1, &p, 2, 183);
        assert_eq!(elf_head(&head, 0x1000), Err(LoadError::BadSegment));
    }

    #[test]
    fn elf_head_claimed_file_len_too_small_truncated() {
        // 段文件范围对**声称的 file_len**校验（不是首窗长度）：file_len 0x40
        // 盖不住 filesz 0x80——即使首窗本身装得下 phdr。
        let p = phdr(1, 0, 0x1000, 0x80, 0x1000);
        let head = ehdr(64, 1, &p, 2, 183);
        assert_eq!(elf_head(&head, 0x40), Err(LoadError::Truncated));
    }

    #[test]
    fn elf_head_head_window_missing_phdrs_truncated() {
        let p = phdr(1, 0, 0x1000, 0x80, 0x1000);
        let head = ehdr(64, 1, &p, 2, 183);
        assert_eq!(elf_head(&head[..100], 0x1000), Err(LoadError::Truncated)); // phdr 越首窗
        assert_eq!(elf_head(&head[..32], 0x1000), Err(LoadError::Truncated)); // ehdr 都不全
        assert_eq!(elf_head(&head, 0x1000).is_ok(), true);
    }

    #[test]
    fn elf_head_dyn_and_no_load_not_supported() {
        let p = phdr(3, 0, 0x1000, 8, 0x1000);
        let head = ehdr(64, 1, &p, 2, 183); // ET_DYN 由 e_type=3
        assert_eq!(elf_head(&head, 0x1000), Err(LoadError::NotSupported));
        let p2 = phdr(4, 0, 0, 0x20, 0x20); // 只有 PT_NOTE
        let head2 = ehdr(64, 1, &p2, 2, 183);
        assert_eq!(elf_head(&head2, 0x1000), Err(LoadError::NotSupported));
    }

    #[test]
    fn page_floor_ceil_edges() {
        assert_eq!(page_floor(0x1080), 0x1000);
        assert_eq!(page_floor(0x1000), 0x1000);
        assert_eq!(page_ceil(0x1000), 0x1000); // 已对齐不动
        assert_eq!(page_ceil(0x1001), 0x2000);
        assert_eq!(page_ceil(u64::MAX), u64::MAX); // 溢出饱和不 panic
    }
}
