//! W1-Q27 三刀：BYI1 图标编码面（Python `tools/mkapp.py` `_png_chunks`/
//! `_paeth`/`png_to_byi`/`_byi_validate` 镜像——彻底 Rust 化轨道 b 层 #4
//! 刀序第四位第三刀）。
//!
//! 收面 = 产面帽收紧的合规 PNG：depth 8 / 彩色类型 RGB(2)·RGBA(6) / 非
//! 隔行 / 无 tRNS / 单边 ≤1024 且 w*h ≤65536 / chunk CRC 全复验 / IDAT
//! 解压长恰合 / filter 0-4。行主序 RLE（u32 ARGB8888，RGB 源 alpha 恒
//! 255；run >65535 编码期拆段——count 是 u16）。
//!
//! inflate 为**本文件自持最小实现**（RFC 1951 stored/fixed/dynamic 全
//! 三形态 + zlib 包装层 FDICT 拒）——零新外部依赖纪律（W1 lane），与
//! tools/mkcpio/mkfat/mkimage 同款纯 std。与 Python 单源对齐义务：改任
//! 一侧必须同步另一侧并重钉黄金向量（tests 内 gold22 等字节级对拍）。

use crate::gate::Reject;

/// 产面帽（对账锚点：tools/mkapp.py :194-197——ICON_DIM_MAX/
/// ICON_PIXELS_MAX 对齐 berry-gfx decode 面，mkapp ↔ appd 双侧镜像义务）。
pub const ICON_DIM_MAX: u32 = 1024;
pub const ICON_PIXELS_MAX: u32 = 65536;

/// RLE count 帽（u16——超 65535 的 run 编码期拆段，非拒面）。
pub const RUN_MAX: u32 = 65535;

/// PNG 签名（RFC 2083）。
const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// `Reject.where_` 是 `&'static str` 而走查面的 where 是运行期源名——拒绝
/// 路径终结一次，小泄漏（per 拒一次）换静态串纪律；诊断 message 完整。
fn rej(msg: String, code: &'static str, where_: &str) -> Reject {
    Reject {
        message: msg,
        code,
        where_: Box::leak(where_.to_string().into_boxed_str()),
        line: None,
    }
}

// ---------------------------------------------------------------------------
// PNG 走查
// ---------------------------------------------------------------------------

/// PNG 逐 chunk 走查：签名 + 头界 + CRC 复验（敌意/损坏文件 loud 拒）。
/// 返回 (tag, body) 序列——首 chunk 非 IHDR 由调用面拒（Python 同序）。
fn png_chunks<'a>(png: &'a [u8], name: &str) -> Result<Vec<(&'a [u8], &'a [u8])>, Reject> {
    if png.len() < 8 || png[..8] != PNG_SIG {
        return Err(rej(format!("icon: 非 PNG（签名不符）"), "icon.png.signature", name));
    }
    let mut out = Vec::new();
    let mut off = 8usize;
    while off < png.len() {
        if off + 8 > png.len() {
            return Err(rej("icon: PNG chunk 头越界".into(), "icon.png.chunk", name));
        }
        let length = u32::from_be_bytes([png[off], png[off + 1], png[off + 2], png[off + 3]]) as usize;
        let tag = &png[off + 4..off + 8];
        if let Some(end) = off.checked_add(12).and_then(|e| e.checked_add(length)) {
            if end > png.len() {
                return Err(rej(
                    format!("icon: PNG chunk {:?} 越界", tag),
                    "icon.png.chunk",
                    name,
                ));
            }
        } else {
            // length + 12 在 usize 域回绕（敌意长度）——按越界拒。
            return Err(rej(
                format!("icon: PNG chunk {:?} 越界", tag),
                "icon.png.chunk",
                name,
            ));
        }
        let body = &png[off + 8..off + 8 + length];
        let crc = u32::from_be_bytes([
            png[off + 8 + length],
            png[off + 9 + length],
            png[off + 10 + length],
            png[off + 11 + length],
        ]);
        if crc32(tag, body) != crc {
            return Err(rej(
                format!("icon: PNG chunk {:?} CRC 不符", tag),
                "icon.png.crc",
                name,
            ));
        }
        out.push((tag, body));
        off += 12 + length;
        if tag == b"IEND" {
            break;
        }
    }
    if off != png.len() {
        return Err(rej("icon: PNG IEND 后拖尾字节".into(), "icon.png.trailing", name));
    }
    if out.is_empty() || out[0].0 != b"IHDR" {
        return Err(rej("icon: PNG 首 chunk 非 IHDR".into(), "icon.png.ihdr", name));
    }
    Ok(out)
}

/// CRC-32（IEEE 802.3 / PNG 规范）——查表法，表由标准多项式 0xEDB88320
/// 反射生成（运行时 lazy 静态等价：std::sync::OnceLock）。
fn crc32(tag: &[u8], body: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (n, e) in t.iter_mut().enumerate() {
            let mut c = n as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
            }
            *e = c;
        }
        t
    });
    let mut c: u32 = 0xFFFF_FFFF;
    for &b in tag.iter().chain(body) {
        c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

// ---------------------------------------------------------------------------
// inflate（RFC 1951）+ zlib 包装（RFC 1950）——自持最小实现
// ---------------------------------------------------------------------------

/// 自持 inflate：返回解压字节。拒面 = 块界/码距/Huffman 表构造/存储块
/// 长度补码/距离越界/尾 Adler-32 不符（敌意压缩流 loud 拒，不 panic）。
fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 6 {
        return Err("zlib 流截断".into());
    }
    let cmf = data[0];
    let flg = data[1];
    if cmf & 0x0F != 8 {
        return Err(format!("zlib 压缩方法 {} 非 deflate", cmf & 0x0F));
    }
    if ((cmf as u32) * 256 + flg as u32) % 31 != 0 {
        return Err("zlib 头校验（%31）不符".into());
    }
    if flg & 0x20 != 0 {
        return Err("zlib FDICT 预设字典不支持".into());
    }
    let mut out = inflate(&data[2..data.len() - 4])?;
    // 尾 4 字节 Adler-32 大端。
    let want = u32::from_be_bytes([
        data[data.len() - 4],
        data[data.len() - 4 + 1],
        data[data.len() - 4 + 2],
        data[data.len() - 4 + 3],
    ]);
    if adler32(&out) != want {
        return Err("zlib Adler-32 校验不符".into());
    }
    out.shrink_to_fit();
    Ok(out)
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize, // 字节位
    bit: u32,   // 位相位（0-7）
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, bit: 0 }
    }

    fn read_bit(&mut self) -> Result<u32, String> {
        if self.pos >= self.data.len() {
            return Err("deflate 流截断（读位）".into());
        }
        let v = (self.data[self.pos] >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.pos += 1;
        }
        Ok(v as u32)
    }

    fn read_bits(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0u32;
        for i in 0..n {
            v |= self.read_bit()? << i;
        }
        Ok(v)
    }

    fn align_byte(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.pos += 1;
        }
    }
}

/// RFC 1951 deflate 解压（stored / fixed Huffman / dynamic Huffman 全形态）。
fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut r = BitReader::new(data);
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = r.read_bit()?;
        let btype = r.read_bits(2)?;
        match btype {
            0 => {
                // 存储块：字节对齐 + LEN + NLEN + 原始字节。
                r.align_byte();
                if r.pos + 4 > r.data.len() {
                    return Err("stored 块头截断".into());
                }
                let len = u16::from_le_bytes([r.data[r.pos], r.data[r.pos + 1]]) as usize;
                let nlen = u16::from_le_bytes([r.data[r.pos + 2], r.data[r.pos + 3]]) as usize;
                if len ^ 0xFFFF != nlen {
                    return Err(format!("stored 块 NLEN 补码不符（{len}^{:#x}）", nlen));
                }
                r.pos += 4;
                if r.pos + len > r.data.len() {
                    return Err("stored 块数据截断".into());
                }
                out.extend_from_slice(&r.data[r.pos..r.pos + len]);
                r.pos += len;
            }
            1 => {
                // fixed Huffman。
                let (lit, dist) = fixed_tables();
                inflate_block(&mut r, &lit, &dist, &mut out)?;
            }
            2 => {
                // dynamic Huffman。
                let (lit, dist) = dynamic_tables(&mut r)?;
                inflate_block(&mut r, &lit, &dist, &mut out)?;
            }
            _ => return Err("deflate 块类型 3 保留".into()),
        }
        if last == 1 {
            break;
        }
    }
    Ok(out)
}

/// Huffman 解码表：canonical code 长度数组 → (first_code, first_symbol)
/// 索引（RFC 1951 §3.2.2 解码律——按长度递增填充符号表）。
struct HuffTable {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl HuffTable {
    fn new(lengths: &[u8]) -> Result<Self, String> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            if l > 15 {
                return Err("Huffman 码长 >15".into());
            }
            counts[l as usize] += 1;
        }
        counts[0] = 0; // 未用符号
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, r: &mut BitReader) -> Result<u16, String> {
        let mut code = 0i32;
        let mut first = 0i32;
        let mut index = 0i32;
        for len in 1..16 {
            code |= r.read_bit()? as i32;
            let count = self.counts[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("Huffman 码距越界".into())
    }
}

fn fixed_tables() -> (HuffTable, HuffTable) {
    let mut lengths = [0u8; 288];
    for (i, e) in lengths.iter_mut().enumerate() {
        *e = if i < 144 {
            8
        } else if i < 256 {
            9
        } else if i < 280 {
            7
        } else {
            8
        };
    }
    let lit = HuffTable::new(&lengths).unwrap();
    let dist = HuffTable::new(&[5u8; 30]).unwrap();
    (lit, dist)
}

fn dynamic_tables(r: &mut BitReader) -> Result<(HuffTable, HuffTable), String> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = r.read_bits(5)? as usize + 257;
    let hdist = r.read_bits(5)? as usize + 1;
    let hclen = r.read_bits(4)? as usize + 4;
    let mut cl_lengths = [0u8; 19];
    for &idx in ORDER.iter().take(hclen) {
        cl_lengths[idx] = r.read_bits(3)? as u8;
    }
    let cl_table = HuffTable::new(&cl_lengths)?;
    let mut lengths = vec![0u8; hlit + hdist];
    let mut i = 0usize;
    while i < lengths.len() {
        let sym = cl_table.decode(r)?;
        match sym {
            0..=15 => {
                lengths[i] = sym as u8;
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err("长度重复 16 无前值".into());
                }
                let prev = lengths[i - 1];
                let rep = 3 + r.read_bits(2)?;
                if i + rep as usize > lengths.len() {
                    return Err("长度重复越表".into());
                }
                for _ in 0..rep {
                    lengths[i] = prev;
                    i += 1;
                }
            }
            17 => {
                let rep = 3 + r.read_bits(3)?;
                if i + rep as usize > lengths.len() {
                    return Err("长度重复越表".into());
                }
                i += rep as usize;
            }
            18 => {
                let rep = 11 + r.read_bits(7)?;
                if i + rep as usize > lengths.len() {
                    return Err("长度重复越表".into());
                }
                i += rep as usize;
            }
            _ => return Err("码长符号越界".into()),
        }
    }
    let lit = HuffTable::new(&lengths[..hlit])?;
    let dist = HuffTable::new(&lengths[hlit..])?;
    Ok((lit, dist))
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4,  5, 7,    9,   13,  17,  25,  33,  49,  65, 97, 129, 193, 257, 385, 513,
    769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn inflate_block(
    r: &mut BitReader,
    lit: &HuffTable,
    dist: &HuffTable,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    loop {
        let sym = lit.decode(r)?;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => return Ok(()),
            257..=285 => {
                let li = (sym - 257) as usize;
                let len = LEN_BASE[li] as usize + r.read_bits(LEN_EXTRA[li] as u32)? as usize;
                let dsym = dist.decode(r)?;
                if dsym > 29 {
                    return Err(format!("距离符号 {dsym} 越界"));
                }
                let d = DIST_BASE[dsym as usize] as usize
                    + r.read_bits(DIST_EXTRA[dsym as usize] as u32)? as usize;
                if d > out.len() {
                    return Err(format!("距离 {d} 越界（输出仅 {}）", out.len()));
                }
                let start = out.len() - d;
                for k in 0..len {
                    let b = out[start + k];
                    out.push(b);
                }
            }
            _ => return Err(format!("字面量符号 {sym} 越界")),
        }
    }
}

// ---------------------------------------------------------------------------
// Paeth 预测子 + 编码主面
// ---------------------------------------------------------------------------

/// PNG 规范 Paeth 预测子（RFC 2083）：|p-a| 最小取 a，次 |p-b|，次 |p-c|。
pub fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let (pa, pb, pc) = ((p - a as i32).abs(), (p - b as i32).abs(), (p - c as i32).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// PNG（depth 8、RGB/RGBA、非隔行、≤产面帽）→ BYI1 字节。拒面与 Python
/// `png_to_byi`（mkapp.py :730）单源对齐——改任一侧必须同步另一侧。
pub fn png_to_byi(png: &[u8], src_name: &str) -> Result<Vec<u8>, Reject> {
    let chunks = png_chunks(png, src_name)?;
    let ihdr = chunks[0].1;
    if ihdr.len() != 13 {
        return Err(rej(
            format!("icon: {src_name} IHDR 长 {} ≠ 13", ihdr.len()),
            "icon.encode.ihdr",
            src_name,
        ));
    }
    let w = u32::from_be_bytes([ihdr[0], ihdr[1], ihdr[2], ihdr[3]]);
    let h = u32::from_be_bytes([ihdr[4], ihdr[5], ihdr[6], ihdr[7]]);
    let depth = ihdr[8];
    let color = ihdr[9];
    let (comp, filt, interlace) = (ihdr[10], ihdr[11], ihdr[12]);
    if depth != 8 || !(color == 2 || color == 6) {
        return Err(rej(
            format!(
                "icon: {src_name} 应为 8-bit RGB/RGBA（得 depth={depth} color={color}；调色板/灰度/16-bit 请重导出）"
            ),
            "icon.encode.color",
            src_name,
        ));
    }
    if comp != 0 || filt != 0 || interlace != 0 {
        return Err(rej(
            format!("icon: {src_name} 非基本压缩/过滤/隔行形态（interlace={interlace}）"),
            "icon.encode.form",
            src_name,
        ));
    }
    if !(w >= 1 && w <= ICON_DIM_MAX && h >= 1 && h <= ICON_DIM_MAX && w * h <= ICON_PIXELS_MAX) {
        return Err(rej(
            format!(
                "icon: {src_name} 尺寸 {w}x{h} 越产面帽（单边≤{ICON_DIM_MAX} 且 w*h≤{ICON_PIXELS_MAX}）"
            ),
            "icon.encode.dim",
            src_name,
        ));
    }
    let mut idat: Vec<u8> = Vec::new();
    for (tag, body) in &chunks[1..] {
        match *tag {
            b"IDAT" => idat.extend_from_slice(body), // 多 IDAT 拼接后一次解压（规范允许分段）
            b"IEND" => break,
            b"tRNS" => {
                return Err(rej(
                    format!("icon: {src_name} 带 tRNS（透明色键）——请导出 RGBA"),
                    "icon.encode.trns",
                    src_name,
                ))
            }
            t if t[0].is_ascii_uppercase() => {
                // 大写首字母 = 关键 chunk——不认识就是拒。
                return Err(rej(
                    format!("icon: {src_name} 未知关键 chunk {:?}", t),
                    "icon.encode.chunk",
                    src_name,
                ));
            }
            _ => {} // 小写首字母 ancillary chunk 跳过（不改像素语义）。
        }
    }
    let bpp = if color == 2 { 3 } else { 4 };
    let raw = inflate_zlib(&idat).map_err(|e| {
        rej(format!("icon: {src_name} IDAT 解压失败： {e}"), "icon.encode.idat", src_name)
    })?;
    let expect = (h as usize) * (1 + (w as usize) * bpp);
    if raw.len() != expect {
        return Err(rej(
            format!("icon: {src_name} 解压长 {} ≠ 期望 {expect}", raw.len()),
            "icon.encode.idat.len",
            src_name,
        ));
    }
    // 逐行 unfilter（filters 0-4；首行 b/c 退化为 0）。
    let stride = (w as usize) * bpp;
    let mut pos = 0usize;
    let mut prev: Vec<u8> = vec![0; stride];
    let mut pixels: Vec<u32> = Vec::with_capacity((w * h) as usize);
    for _y in 0..h {
        let f = raw[pos];
        pos += 1;
        if f > 4 {
            return Err(rej(
                format!("icon: {src_name} 未知 filter 类型 {f}"),
                "icon.encode.filter",
                src_name,
            ));
        }
        let mut line = raw[pos..pos + stride].to_vec();
        pos += stride;
        for i in 0..stride {
            let a = if i >= bpp { line[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            let pred = match f {
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                4 => paeth(a, b, c),
                _ => 0,
            };
            line[i] = line[i].wrapping_add(pred);
        }
        for chunk_px in line.chunks_exact(bpp) {
            let (r, g, b) = (chunk_px[0], chunk_px[1], chunk_px[2]);
            let a = if bpp == 4 { chunk_px[3] } else { 255 };
            pixels.push(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32);
        }
        prev = line;
    }
    // 行主序 RLE（run > 65535 拆段——count 是 u16）。
    let mut out = Vec::with_capacity(10 + pixels.len() * 6);
    out.extend_from_slice(b"BYI1");
    out.extend_from_slice(&[1, 0]);
    out.extend_from_slice(&(w as u16).to_le_bytes());
    out.extend_from_slice(&(h as u16).to_le_bytes());
    let mut run_px = pixels[0];
    let mut run_n = 1u32;
    for &px in &pixels[1..] {
        if px == run_px && run_n < RUN_MAX {
            run_n += 1;
            continue;
        }
        out.extend_from_slice(&(run_n as u16).to_le_bytes());
        out.extend_from_slice(&run_px.to_le_bytes());
        run_px = px;
        run_n = 1;
    }
    out.extend_from_slice(&(run_n as u16).to_le_bytes());
    out.extend_from_slice(&run_px.to_le_bytes());
    Ok(out)
}

/// `--check` 复验：包内 .byi 全量重走（header + RLE 段逐段 + Σcount ==
/// w*h + 字节恰耗尽；约束面对齐 berry-gfx decode + 产面帽）。
pub fn byi_validate(data: &[u8], name: &[u8]) -> Result<(), Reject> {
    let label = String::from_utf8_lossy(name).into_owned();
    if data.len() < 10 || &data[..4] != b"BYI1" {
        return Err(rej(
            format!("bap: {label} 非 BYI1（魔数不符）"),
            "bap.byi.magic",
            label.as_str(),
        ));
    }
    if data[4] != 1 || data[5] != 0 {
        return Err(rej(
            format!("bap: {label} version/flags 非 1/0"),
            "bap.byi.version",
            label.as_str(),
        ));
    }
    let w = u16::from_le_bytes([data[6], data[7]]) as u32;
    let h = u16::from_le_bytes([data[8], data[9]]) as u32;
    if !(w >= 1 && w <= ICON_DIM_MAX && h >= 1 && h <= ICON_DIM_MAX && w * h <= ICON_PIXELS_MAX) {
        return Err(rej(
            format!(
                "bap: {label} 尺寸 {w}x{h} 越产面帽（单边≤{ICON_DIM_MAX} 且 w*h≤{ICON_PIXELS_MAX}）"
            ),
            "bap.byi.dim",
            label.as_str(),
        ));
    }
    let mut off = 10usize;
    let mut total = 0u64;
    while off < data.len() {
        if off + 6 > data.len() {
            return Err(rej(
                format!("bap: {label} RLE 段截断"),
                "bap.byi.trunc",
                label.as_str(),
            ));
        }
        let count = u16::from_le_bytes([data[off], data[off + 1]]) as u64;
        off += 2;
        // px 4 字节读后即弃（Python 同款 `_px`——值域不执法，结构执法）。
        off += 4;
        if count < 1 {
            return Err(rej(
                format!("bap: {label} RLE 零长段"),
                "bap.byi.zero",
                label.as_str(),
            ));
        }
        total += count;
        if total > (w * h) as u64 {
            return Err(rej(
                format!("bap: {label} RLE 像素溢出（{total} > {}）", w * h),
                "bap.byi.overflow",
                label.as_str(),
            ));
        }
    }
    if total != (w * h) as u64 {
        return Err(rej(
            format!("bap: {label} RLE Σcount {total} != w*h {}", w * h),
            "bap.byi.sum",
            label.as_str(),
        ));
    }
    Ok(())
}


#[cfg(test)]
mod byi_tests {
    use super::*;

    /// selftest 专用 PNG 工厂（Python `_mk_png` 镜像——最小合法件 + 可调
    /// 形态面；raw 缺省 = 全零像素行；extra_chunks 插在 IHDR 后）。
    fn mk_png(
        w: u32,
        h: u32,
        color: u8,
        depth: u8,
        interlace: u8,
        raw: Option<&[u8]>,
        extra_chunks: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        let bpp = if color == 6 { 4 } else { 3 };
        let raw_owned: Vec<u8>;
        let raw = match raw {
            Some(r) => r,
            None => {
                raw_owned = (0..h)
                    .flat_map(|_| {
                        let mut v = vec![0u8; 1 + w as usize * bpp];
                        v[0] = 0;
                        v
                    })
                    .collect();
                &raw_owned
            }
        };
        let chunk = |tag: &[u8; 4], data: &[u8]| -> Vec<u8> {
            let mut v = Vec::with_capacity(12 + data.len());
            v.extend_from_slice(&(data.len() as u32).to_be_bytes());
            v.extend_from_slice(tag);
            v.extend_from_slice(data);
            v.extend_from_slice(&crc32(tag, data).to_be_bytes());
            v
        };
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[depth, color, 0, 0, interlace]);
        let mut out = Vec::new();
        out.extend_from_slice(&PNG_SIG);
        out.extend_from_slice(&chunk(b"IHDR", &ihdr));
        for (tag, data) in extra_chunks {
            out.extend_from_slice(&chunk(tag, data));
        }
        // Python zlib.compress = RFC 1950 deflate；本 crate 无压缩面——用
        // stored 块包装（解压侧全形态收，黄金字节不受压缩形态影响）。
        out.extend_from_slice(&chunk(b"IDAT", &zlib_stored(raw)));
        out.extend_from_slice(&chunk(b"IEND", b""));
        out
    }

    /// zlib 包装的最小 stored-deflate（RFC 1950 + 1951 stored 块）——测试
    /// 侧压缩件（生产方向是 png_to_byi 收任意合规 PNG，含 dynamic
    /// Huffman 的真压缩流由 Python 产线对拍腿覆盖）。
    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01];
        let mut i = 0usize;
        loop {
            let end = (i + 65535).min(data.len());
            let last = end == data.len();
            out.push(if last { 1 } else { 0 });
            let len = (end - i) as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(&data[i..end]);
            i = end;
            if last {
                break;
            }
        }
        out.extend_from_slice(&adler32(data).to_be_bytes());
        out
    }

    #[test]
    fn enc_golden_2x2_rgba() {
        // Python selftest gold22 逐字节镜像：四异像素 → 四段 count=1。
        // 像素值 = (a<<24)|(r<<16)|(g<<8)|b；raw22 字节序 = PNG 行序 (r,g,b,a)。
        let raw22: &[u8] = &[
            0x00, 255, 0, 0, 255, 0, 255, 0, 255, //
            0x00, 0, 0, 255, 255, 128, 128, 128, 128,
        ];
        let png = mk_png(2, 2, 6, 8, 0, Some(raw22), &[]);
        let got = png_to_byi(&png, "icon.png").unwrap();
        let mut want = Vec::new();
        want.extend_from_slice(b"BYI1");
        want.extend_from_slice(&[1, 0]);
        want.extend_from_slice(&2u16.to_le_bytes());
        want.extend_from_slice(&2u16.to_le_bytes());
        for px in [0xFFFF0000u32, 0xFF00FF00, 0xFF0000FF, 0x80808080] {
            want.extend_from_slice(&1u16.to_le_bytes());
            want.extend_from_slice(&px.to_le_bytes());
        }
        assert_eq!(got, want, "字节不等：{:02x?} != {:02x?}", got, want);
    }

    #[test]
    fn enc_rgb_alpha_forced_255() {
        let png = mk_png(1, 1, 2, 8, 0, Some(&[0x00, 0xff, 0x00, 0x00]), &[]);
        let got = png_to_byi(&png, "icon.png").unwrap();
        let mut want = Vec::new();
        want.extend_from_slice(b"BYI1");
        want.extend_from_slice(&[1, 0]);
        want.extend_from_slice(&1u16.to_le_bytes());
        want.extend_from_slice(&1u16.to_le_bytes());
        want.extend_from_slice(&1u16.to_le_bytes());
        want.extend_from_slice(&0xFFFF0000u32.to_le_bytes());
        assert_eq!(got, want);
    }

    #[test]
    fn enc_run_split_65535() {
        // 256×256 纯色：单 run 65536 > u16 帽 → 拆 [65535, 1] 两段（22B）。
        let png = mk_png(256, 256, 6, 8, 0, None, &[]);
        let got = png_to_byi(&png, "icon.png").unwrap();
        assert_eq!(got.len(), 22);
        assert_eq!(u16::from_le_bytes([got[6], got[7]]), 256);
        assert_eq!(u16::from_le_bytes([got[8], got[9]]), 256);
        assert_eq!(u16::from_le_bytes([got[10], got[11]]), 65535);
        assert_eq!(u32::from_le_bytes([got[12], got[13], got[14], got[15]]), 0);
        assert_eq!(u16::from_le_bytes([got[16], got[17]]), 1);
        assert_eq!(u32::from_le_bytes([got[18], got[19], got[20], got[21]]), 0);
    }

    #[test]
    fn enc_reject_spectrum() {
        let trns: Vec<u8> = vec![0, 255, 0, 0, 0, 0]; // >HHH 六字节（值面不执法）
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("not-png", b"BYI1-definitely-not-a-png".to_vec()),
            ("truncated", mk_png(2, 2, 6, 8, 0, None, &[])[..4.min(usize::MAX)].to_vec()),
            ("bad-crc", {
                let mut p = mk_png(2, 2, 6, 8, 0, None, &[]);
                let n = p.len();
                p[n - 4..n].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
                p
            }),
            ("palette", mk_png(1, 1, 3, 8, 0, Some(&[0x00, 0x00]), &[])),
            ("gray", mk_png(1, 1, 0, 8, 0, Some(&[0x00, 0x00]), &[])),
            ("16bit", mk_png(1, 1, 6, 16, 0, Some(&[0x00, 0, 0, 0, 0, 0, 0, 0, 0]), &[])),
            ("interlaced", mk_png(1, 1, 6, 8, 1, None, &[])),
            ("dim-1025", mk_png(1025, 1, 6, 8, 0, None, &[])),
            ("pixels-over", mk_png(1024, 65, 6, 8, 0, None, &[])),
            ("raw-len", mk_png(2, 2, 6, 8, 0, Some(&[0x00, 0, 0, 0]), &[])),
            (
                "filter-5",
                mk_png(1, 1, 6, 8, 0, Some(&[0x05, 0xff, 0x00, 0x00, 0xff]), &[]),
            ),
            (
                "trns",
                mk_png(1, 1, 2, 8, 0, Some(&[0x00, 0xff, 0x00, 0x00]), &[(b"tRNS", &trns)]),
            ),
        ];
        for (label, png) in cases {
            let r = png_to_byi(&png, "icon.png");
            assert!(r.is_err(), "{label} 被编码器放行：{r:?}");
        }
    }

    #[test]
    fn enc_filters_all_five() {
        // 3×2 RGBA，五行 filter 各取一形（0-4）——unfilter 数学（含 Paeth）
        // 与 gold 向量交叉：像素 [0..6) 行重复两行。
        let px = |i: u8| vec![i, (i * 3).wrapping_add(1), (i * 7).wrapping_add(2), 255];
        let mut rows: Vec<Vec<u8>> = Vec::new();
        let filters = [0u8, 1, 2, 3, 4, 0];
        for y in 0..6u8 {
            let mut line: Vec<u8> = Vec::new();
            for x in 0..3u8 {
                line.extend_from_slice(&px(y * 3 + x));
            }
            rows.push(line);
        }
        // 手工 filter（编码方向——测试件）：对每行按 filters[y] 重滤。
        let mut filtered: Vec<u8> = Vec::new();
        let bpp = 4usize;
        let mut prev: Vec<u8> = vec![0; 12];
        for (y, line) in rows.iter().enumerate() {
            let f = filters[y];
            filtered.push(f);
            let mut out = vec![0u8; 12];
            for i in 0..12 {
                let a = if i >= bpp { line[i - bpp] } else { 0 };
                let b = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };
                let pred = match f {
                    1 => a,
                    2 => b,
                    3 => ((a as u16 + b as u16) / 2) as u8,
                    4 => paeth(a, b, c),
                    _ => 0,
                };
                out[i] = line[i].wrapping_sub(pred);
            }
            filtered.extend_from_slice(&out);
            prev = line.clone();
        }
        let png = mk_png(3, 6, 6, 8, 0, Some(&filtered), &[]);
        let got = png_to_byi(&png, "icon.png").unwrap();
        // 期望 = 行主序像素直拼（每行三像素、两行相同行 → RLE 段）。
        let mut want = Vec::new();
        want.extend_from_slice(b"BYI1");
        want.extend_from_slice(&[1, 0]);
        want.extend_from_slice(&3u16.to_le_bytes());
        want.extend_from_slice(&6u16.to_le_bytes());
        for line in &rows {
            for c in line.chunks_exact(4) {
                let pxv = ((c[3] as u32) << 24)
                    | ((c[0] as u32) << 16)
                    | ((c[1] as u32) << 8)
                    | c[2] as u32;
                want.extend_from_slice(&1u16.to_le_bytes());
                want.extend_from_slice(&pxv.to_le_bytes());
            }
        }
        assert_eq!(got, want, "全 filter 形 unfilter 往返不等");
    }

    #[test]
    fn inflate_matches_expected_shapes() {
        // stored 块往返 + 多 stored 块（>65535 触发分块）+ 拒面族。
        let data = b"hello world hello world hello".to_vec();
        assert_eq!(inflate_zlib(&zlib_stored(&data)).unwrap(), data);
        let big = (0..200_000u32).map(|i| (i % 251) as u8).collect::<Vec<u8>>();
        assert_eq!(inflate_zlib(&zlib_stored(&big)).unwrap(), big);
        // 短流 / 坏方法 / 坏 %31 / FDICT / 坏 Adler。
        assert!(inflate_zlib(&[0x78]).is_err());
        assert!(inflate_zlib(&zlib_stored(&data)[..2]).is_err());
        let mut bad = zlib_stored(&data);
        bad[0] = 0x77; // method 7
        assert!(inflate_zlib(&bad).is_err());
        let mut bad = zlib_stored(&data);
        bad[1] ^= 0x01; // %31 破坏
        assert!(inflate_zlib(&bad).is_err());
        let mut bad = zlib_stored(&data);
        bad[1] |= 0x20; // FDICT 位
        if ((bad[0] as u32) * 256 + bad[1] as u32) % 31 != 0 {
            bad[1] += 1; // 保持 %31 合法且 FDICT 立
        }
        assert!(inflate_zlib(&bad).is_err());
        let mut bad = zlib_stored(&data);
        let n = bad.len();
        bad[n - 1] ^= 0xFF; // Adler 破坏
        assert!(inflate_zlib(&bad).is_err());
    }

    #[test]
    fn python_oracle_cross_check() {
        // Python 现役件对拍（缺席 skip——CI 无 python 面时兜底不红）。
        let out = std::process::Command::new("python3")
            .arg("-c")
            .arg(
                r#"
import sys, struct, zlib
sys.path.insert(0, "tools")
import mkapp
def mk_png(w, h, color=6, depth=8, interlace=0, raw=None, extra=None):
    bpp = 4 if color == 6 else 3
    if raw is None:
        raw = b"".join(b"\x00" + bytes(w * bpp) for _ in range(h))
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", w, h, depth, color, 0, 0, interlace)
    mid = b"".join(chunk(t, d) for t, d in (extra or []))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + mid + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")
cases = []
raw22 = (b"\x00" + bytes((255,0,0,255)) + bytes((0,255,0,255)) + b"\x00" + bytes((0,0,255,255)) + bytes((128,128,128,128)))
cases.append(("2x2", mk_png(2,2,raw=raw22)))
cases.append(("1x1-rgb", mk_png(1,1,color=2,raw=b"\x00\xff\x00\x00")))
cases.append(("256", mk_png(256,256)))
raw75 = b"".join(bytes([0]) + bytes(c for x in range(7) for c in ((x*37+y*11)%256, (x*11+y*3)%256, (x*7+y*13)%256, 255)) for y in range(5))
cases.append(("7x5-grad", mk_png(7,5,raw=raw75)))
for name, png in cases:
    byi = mkapp.png_to_byi(png, "icon.png")
    mkapp._byi_validate(byi, b"icon.png")
    print(name, byi.hex(), png.hex())
"#,
            )
            .current_dir(std::path::Path::new("../../"))
            .output();
        let out = match out {
            Ok(o) => o,
            Err(_) => {
                eprintln!("skip：python3 缺席");
                return;
            }
        };
        if !out.status.success() {
            panic!("Python oracle 失败：{}", String::from_utf8_lossy(&out.stderr));
        }
        // stdout 可能带 _HUMAN 忠告行（w<108 warn）——只收「名 hex」形腿。
        let rows: Vec<Vec<&str>> = std::str::from_utf8(&out.stdout)
            .unwrap()
            .lines()
            .filter(|l| l.split_whitespace().count() == 3)
            .map(|l| l.split_whitespace().collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 4, "四腿对拍（忠告行滤除后）：{:?}", rows);
        let hexes: Vec<&str> = rows.iter().map(|r| r[1]).collect();
        let pngs_hex: Vec<&str> = rows.iter().map(|r| r[2]).collect();
        // Rust 侧直喂 Python 产的 PNG 字节（zlib.compress = dynamic Huffman
        // 真压缩流——覆盖 stored 之外的 inflate 形态面），BYI 输出对拍。
        let unhex = |h: &str| -> Vec<u8> {
            (0..h.len() / 2)
                .map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap())
                .collect()
        };
        for (i, name) in ["2x2", "1x1-rgb", "256", "7x5-grad"].iter().enumerate() {
            let png = unhex(pngs_hex[i]);
            let got = png_to_byi(&png, "icon.png")
                .unwrap_or_else(|e| panic!("{name} Rust 侧误拒：{e}"));
            assert_eq!(got, unhex(hexes[i]), "{name} 对拍不等（dynamic 流）");
            byi_validate(&got, b"icon.png").unwrap();
        }
    }

    #[test]
    fn byi_validate_spectrum() {
        // roundtrip 过 + 六拒面（magic/version/dim/trunc/zero/overflow/sum）。
        let ok = png_to_byi(&mk_png(2, 2, 6, 8, 0, None, &[]), "i.png").unwrap();
        assert!(byi_validate(&ok, b"i.png").is_ok());

        let mut bad = ok.clone();
        bad[0] = b'X';
        assert_eq!(byi_validate(&bad, b"i.png").unwrap_err().code, "bap.byi.magic");
        let mut bad = ok.clone();
        bad[4] = 2;
        assert_eq!(byi_validate(&bad, b"i.png").unwrap_err().code, "bap.byi.version");
        let mut bad = ok.clone();
        bad[6..8].copy_from_slice(&1200u16.to_le_bytes()); // w=1200 越单边帽（h 保 2）
        assert_eq!(byi_validate(&bad, b"i.png").unwrap_err().code, "bap.byi.dim");
        let mut bad = ok.clone();
        bad.truncate(bad.len() - 3); // 段截断
        assert_eq!(byi_validate(&bad, b"i.png").unwrap_err().code, "bap.byi.trunc");
        let mut bad = ok.clone();
        bad[10..12].copy_from_slice(&0u16.to_le_bytes()); // 零长段
        assert_eq!(byi_validate(&bad, b"i.png").unwrap_err().code, "bap.byi.zero");
        // 溢出：Σcount > w*h（首段 count=5 > 4）。
        let mut bad = ok.clone();
        bad[10..12].copy_from_slice(&5u16.to_le_bytes());
        assert_eq!(byi_validate(&bad, b"i.png").unwrap_err().code, "bap.byi.overflow");
        // Σcount < w*h：截尾一段后 sum=3 ≠ 4。
        let mut bad = ok.clone();
        bad.truncate(bad.len() - 6);
        assert_eq!(byi_validate(&bad, b"i.png").unwrap_err().code, "bap.byi.sum");
        // 短于 10 字节首拒 magic（len<10 面）。
        assert_eq!(byi_validate(b"BYI", b"i.png").unwrap_err().code, "bap.byi.magic");
    }

    #[test]
    fn paeth_spec_vectors() {
        // RFC 2083 判序：|p-a| 最小取 a，次 |p-b|，次 |p-c|；平局序 a>b>c。
        assert_eq!(paeth(0, 0, 0), 0);
        assert_eq!(paeth(255, 255, 255), 255);
        assert_eq!(paeth(10, 20, 30), 10); // p=0：|p-a|=10 最小 → a
        assert_eq!(paeth(1, 2, 3), 1); // p=0：|p-a|=1 最小 → a
        assert_eq!(paeth(5, 5, 0), 5);
        // a=0,b=0,c=255: p=-255 → pa=255,pb=255,pc=510 → c? |p-c|=|-255-255|=510;pa=255;pb=255 → pc 最大 → a or b：pa<=pb → a
        assert_eq!(paeth(0, 0, 255), 0);
    }
}
