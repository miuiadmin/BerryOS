//! Minimal PNG writer + PPM reader.
//!
//! No image crate: PNG's IDAT is a zlib stream; `miniz_oxide` supplies the
//! deflate encoder (host-only dependency). A 480×1000 frame drops from
//! ~1.4 MiB stored to a few tens of KiB; `write_png` API is unchanged.

use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::error::{BerryError, Result};

pub fn write_png(path: &Path, width: u32, height: u32, rgb: &[u8]) -> Result<()> {
    let need = width as usize * height as usize * 3;
    if rgb.len() != need {
        return Err(BerryError::internal(format!(
            "png buffer {} != {}x{}x3",
            rgb.len(),
            width,
            height
        )));
    }
    let mut raw = Vec::with_capacity(need + height as usize);
    let row = width as usize * 3;
    for y in 0..height as usize {
        raw.push(0); // filter: None
        raw.extend_from_slice(&rgb[y * row..(y + 1) * row]);
    }
    let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(2); // color type: truecolor RGB
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace

    let f = File::create(path)?;
    let mut w = BufWriter::new(f);
    w.write_all(b"\x89PNG\r\n\x1a\n")?;
    write_chunk(&mut w, b"IHDR", &ihdr)?;
    write_chunk(&mut w, b"IDAT", &z)?;
    write_chunk(&mut w, b"IEND", &[])?;
    w.flush()?;
    Ok(())
}

/// Animated PNG (APNG): truecolor RGB, one fcTL per frame, fdAT after the
/// first IDAT. delay_ms is split as delay_num/delay_den=1000.
pub fn write_apng(
    path: &Path,
    width: u32,
    height: u32,
    frames: &[Vec<u8>],
    delay_ms: u16,
) -> Result<()> {
    let need = width as usize * height as usize * 3;
    if frames.is_empty() || frames.iter().any(|f| f.len() != need) {
        return Err(BerryError::internal("apng frame size mismatch"));
    }
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8);
    ihdr.push(2);
    ihdr.push(0);
    ihdr.push(0);
    ihdr.push(0);
    let mut actl = Vec::with_capacity(8);
    actl.extend_from_slice(&(frames.len() as u32).to_be_bytes());
    actl.extend_from_slice(&0u32.to_be_bytes()); // infinite plays

    let f = File::create(path)?;
    let mut w = BufWriter::new(f);
    w.write_all(b"\x89PNG\r\n\x1a\n")?;
    write_chunk(&mut w, b"IHDR", &ihdr)?;
    write_chunk(&mut w, b"acTL", &actl)?;
    let row = width as usize * 3;
    let mut seq = 0u32;
    for (i, frame) in frames.iter().enumerate() {
        let mut raw = Vec::with_capacity(need + height as usize);
        for y in 0..height as usize {
            raw.push(0);
            raw.extend_from_slice(&frame[y * row..(y + 1) * row]);
        }
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        let mut fctl = Vec::with_capacity(26);
        fctl.extend_from_slice(&seq.to_be_bytes());
        seq += 1;
        fctl.extend_from_slice(&width.to_be_bytes());
        fctl.extend_from_slice(&height.to_be_bytes());
        fctl.extend_from_slice(&0u32.to_be_bytes());
        fctl.extend_from_slice(&0u32.to_be_bytes());
        fctl.extend_from_slice(&delay_ms.to_be_bytes());
        fctl.extend_from_slice(&1000u16.to_be_bytes());
        fctl.push(0); // dispose: none
        fctl.push(0); // blend: source
        write_chunk(&mut w, b"fcTL", &fctl)?;
        if i == 0 {
            write_chunk(&mut w, b"IDAT", &z)?;
        } else {
            let mut fdat = Vec::with_capacity(4 + z.len());
            fdat.extend_from_slice(&seq.to_be_bytes());
            seq += 1;
            fdat.extend_from_slice(&z);
            write_chunk(&mut w, b"fdAT", &fdat)?;
        }
    }
    write_chunk(&mut w, b"IEND", &[])?;
    w.flush()?;
    Ok(())
}

fn write_chunk(w: &mut impl Write, kind: &[u8; 4], data: &[u8]) -> Result<()> {
    w.write_all(&(data.len() as u32).to_be_bytes())?;
    w.write_all(kind)?;
    w.write_all(data)?;
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    w.write_all(&crc.finish().to_be_bytes())?;
    Ok(())
}

// CRC32 bitwise——镜像方 = `user/berry-lifecycle/src/lib.rs` `crc32`
// （poly `0xEDB88320` 同款 bitwise：两侧互指单源纪律，改任一侧须同步；
// 向量钉板 "123456789" → `0xCBF43926` 在 berry-lifecycle host 测）。
struct Crc32 {
    value: u32,
}

impl Crc32 {
    fn new() -> Self {
        Self { value: 0xffff_ffff }
    }

    fn update(&mut self, data: &[u8]) {
        let mut crc = self.value;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        self.value = crc;
    }

    fn finish(self) -> u32 {
        self.value ^ 0xffff_ffff
    }
}

/// Read a binary P6 PPM (QEMU `screendump` output).
pub fn read_ppm(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    let data = fs::read(path)?;
    let mut pos = 0usize;
    let magic = next_token(&data, &mut pos)
        .ok_or_else(|| BerryError::protocol("ppm: missing magic"))?;
    if magic != b"P6" {
        return Err(BerryError::protocol("ppm: only P6 is supported"));
    }
    let w = parse_u32(&data, &mut pos, "width")?;
    let h = parse_u32(&data, &mut pos, "height")?;
    let max = parse_u32(&data, &mut pos, "maxval")?;
    if max != 255 {
        return Err(BerryError::protocol("ppm: only maxval=255 is supported"));
    }
    // Exactly one whitespace delimiter after maxval; accept CRLF too.
    if pos >= data.len() || !data[pos].is_ascii_whitespace() {
        return Err(BerryError::protocol("ppm: missing data delimiter"));
    }
    if data[pos] == b'\r' && data.get(pos + 1) == Some(&b'\n') {
        pos += 2;
    } else {
        pos += 1;
    }
    let need = w as usize * h as usize * 3;
    if data.len() < pos + need {
        return Err(BerryError::protocol("ppm: truncated pixel data"));
    }
    Ok((w, h, data[pos..pos + need].to_vec()))
}

fn parse_u32(data: &[u8], pos: &mut usize, what: &str) -> Result<u32> {
    let tok = next_token(data, pos)
        .ok_or_else(|| BerryError::protocol(format!("ppm: missing {what}")))?;
    std::str::from_utf8(tok)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| BerryError::protocol(format!("ppm: bad {what}")))
}

fn next_token<'a>(data: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    loop {
        while *pos < data.len() && data[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
        if *pos < data.len() && data[*pos] == b'#' {
            while *pos < data.len() && data[*pos] != b'\n' {
                *pos += 1;
            }
            continue;
        }
        break;
    }
    if *pos >= data.len() {
        return None;
    }
    let start = *pos;
    while *pos < data.len()
        && !data[*pos].is_ascii_whitespace()
        && data[*pos] != b'#'
    {
        *pos += 1;
    }
    Some(&data[start..*pos])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zlib_roundtrip() {
        let data = b"hello berry hello berry hello berry";
        let z = miniz_oxide::deflate::compress_to_vec_zlib(data, 6);
        assert_eq!(&z[..1], &[0x78]);
        let back = miniz_oxide::inflate::decompress_to_vec_zlib(&z).unwrap();
        assert_eq!(back, data);
    }

    #[test]
    fn ppm_parse() {
        let mut v = b"P6\n2 1\n255\n".to_vec();
        v.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        let tmp = std::env::temp_dir().join("berry-test.ppm");
        std::fs::write(&tmp, &v).unwrap();
        let (w, h, rgb) = read_ppm(&tmp).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgb, vec![1, 2, 3, 4, 5, 6]);
        let _ = std::fs::remove_file(tmp);
    }

    // —— png 深测面（W1-Q25 四刀）：write_png/write_apng 产物结构钉板 +
    // Crc32 单源镜像钉板 + read_ppm 边界全家桶。全部纯函数面，无 I/O 依赖
    // （tmp 文件走 std::env::temp_dir，同 ppm_parse 先例）。 ——

    /// 拆 chunk：len(4BE) + kind(4) + data + crc(4BE)。返回 (kind, data)。
    fn chunks(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let mut out = Vec::new();
        let mut p = 8usize;
        while p < bytes.len() {
            let len = u32::from_be_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
            let kind = String::from_utf8_lossy(&bytes[p + 4..p + 8]).into_owned();
            let data = bytes[p + 8..p + 8 + len].to_vec();
            let crc = u32::from_be_bytes(
                bytes[p + 8 + len..p + 12 + len].try_into().unwrap(),
            );
            let mut c = Crc32::new();
            c.update(kind.as_bytes());
            c.update(&data);
            assert_eq!(c.finish(), crc, "chunk {kind} CRC mismatch");
            out.push((kind, data));
            p += 12 + len;
        }
        out
    }

    #[test]
    fn png_structure_and_roundtrip() {
        // 3×2 RGB 六像素：产物 = 签名 + IHDR + 单 IDAT + IEND；IDAT 解压
        // 回滤波行（每行前置 filter=0 字节）。
        let rgb: Vec<u8> = (1..=18).collect();
        let tmp = std::env::temp_dir().join(format!("berry-png-{}.png", std::process::id()));
        write_png(&tmp, 3, 2, &rgb).unwrap();
        let bytes = std::fs::read(&tmp).unwrap();
        let _ = std::fs::remove_file(&tmp);
        let cs = chunks(&bytes);
        assert_eq!(
            cs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            vec!["IHDR", "IDAT", "IEND"]
        );
        let ihdr = &cs[0].1;
        assert_eq!(ihdr.len(), 13);
        assert_eq!(&ihdr[..4], &3u32.to_be_bytes());
        assert_eq!(&ihdr[4..8], &2u32.to_be_bytes());
        assert_eq!(&ihdr[8..], &[8, 2, 0, 0, 0]);
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&cs[1].1).unwrap();
        assert_eq!(raw.len(), 20);
        assert_eq!(&raw[..10], &[0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9]); // filter+行一
        assert_eq!(&raw[10..], &[0u8, 10, 11, 12, 13, 14, 15, 16, 17, 18]); // filter+行二
        // IEND 恒零长。
        assert!(cs[2].1.is_empty());
    }

    #[test]
    fn png_size_mismatch_rejected() {
        let tmp = std::env::temp_dir().join(format!("berry-png-bad-{}.png", std::process::id()));
        // 短 buffer：拒绝，不落盘。
        assert!(write_png(&tmp, 2, 2, &[1, 2, 3]).is_err());
        assert!(!tmp.exists());
        // 长_buffer 同拒。
        let long = vec![0u8; 2 * 2 * 3 + 1];
        assert!(write_png(&tmp, 2, 2, &long).is_err());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn apng_structure_and_sequence() {
        // 两帧 2×1：acTL(num_frames, 0) + 帧一 fcTL(seq=0)+IDAT + 帧二
        // fcTL(seq=1)+fdAT(seq=2)。fdAT 载荷 = seq(4BE) + zlib 流，解压
        // 回第二帧滤波行。
        let f1 = vec![10, 11, 12, 13, 14, 15];
        let f2 = vec![20, 21, 22, 23, 24, 25];
        let tmp = std::env::temp_dir().join(format!("berry-apng-{}.png", std::process::id()));
        write_apng(&tmp, 2, 1, &[f1.clone(), f2.clone()], 250).unwrap();
        let bytes = std::fs::read(&tmp).unwrap();
        let _ = std::fs::remove_file(&tmp);
        let cs = chunks(&bytes);
        assert_eq!(
            cs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            vec!["IHDR", "acTL", "fcTL", "IDAT", "fcTL", "fdAT", "IEND"]
        );
        let actl = &cs[1].1;
        assert_eq!(&actl[..4], &2u32.to_be_bytes());
        assert_eq!(&actl[4..], &0u32.to_be_bytes()); // 无限循环
        let fc1 = &cs[2].1;
        assert_eq!(fc1.len(), 26);
        assert_eq!(&fc1[..4], &0u32.to_be_bytes()); // seq 0
        assert_eq!(&fc1[20..22], &250u16.to_be_bytes()); // delay_num
        assert_eq!(&fc1[22..24], &1000u16.to_be_bytes()); // delay_den
        assert_eq!(&fc1[24..], &[0, 0]); // dispose/blend
        let idat = miniz_oxide::inflate::decompress_to_vec_zlib(&cs[3].1).unwrap();
        assert_eq!(&idat[..7], &[0, 10, 11, 12, 13, 14, 15]);
        let fc2 = &cs[4].1;
        assert_eq!(&fc2[..4], &1u32.to_be_bytes()); // seq 1
        let fdat = &cs[5].1;
        assert_eq!(&fdat[..4], &2u32.to_be_bytes()); // fdAT seq 2
        let z2 = miniz_oxide::inflate::decompress_to_vec_zlib(&fdat[4..]).unwrap();
        assert_eq!(&z2[..7], &[0, 20, 21, 22, 23, 24, 25]);
        assert!(cs[6].1.is_empty());
    }

    #[test]
    fn apng_rejects_empty_and_mismatched_frames() {
        let tmp = std::env::temp_dir().join(format!("berry-apng-bad-{}.png", std::process::id()));
        // 零帧。
        assert!(write_apng(&tmp, 2, 1, &[], 100).is_err());
        // 尺寸不符帧。
        let bad = vec![vec![1u8; 7], vec![1u8; 6]];
        assert!(write_apng(&tmp, 2, 1, &bad, 100).is_err());
        assert!(!tmp.exists());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn crc32_pinning() {
        // 单源镜像钉板：poly 0xEDB88320 bitwise 与 user/berry-lifecycle
        // crc32 同款——标准向量 "123456789" → 0xCBF43926 两侧互指单源。
        let mut c = Crc32::new();
        c.update(b"123456789");
        assert_eq!(c.finish(), 0xCBF4_3926);
        // 空输入 = 初值/终值翻转 = 0。
        assert_eq!(Crc32::new().finish(), 0);
        // 分段 update 与一次 update 等价（write_chunk 两段路径的数学依据）。
        let mut split = Crc32::new();
        split.update(b"IDAT");
        split.update(&[1, 2, 3]);
        let mut whole = Crc32::new();
        whole.update(b"IDAT\x01\x02\x03");
        assert_eq!(split.finish(), whole.finish());
        // 单字节向量（bitwise 实现逐位对拍）。
        let mut one = Crc32::new();
        one.update(&[0x00]);
        assert_eq!(one.finish(), 0xD202EF8D);
    }

    #[test]
    fn ppm_edge_cases() {
        let t = |name: &str, v: &[u8]| {
            let p = std::env::temp_dir().join(format!("berry-ppm-{}-{name}", std::process::id()));
            std::fs::write(&p, v).unwrap();
            let r = read_ppm(&p);
            let _ = std::fs::remove_file(&p);
            r
        };
        // 注释行跳过（# 后整行吞）。
        let mut commented = b"P6\n# comment\n2 1\n255\n".to_vec();
        commented.extend_from_slice(&[9, 8, 7, 6, 5, 4]);
        let (w, h, rgb) = t("c", &commented).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgb, vec![9, 8, 7, 6, 5, 4]);
        // CRLF 分隔 + 数据首字节恰为 \r（不误吞像素）。
        let mut crlf = b"P6\r\n2 1\r\n255\r\n".to_vec();
        crlf.extend_from_slice(&[0x0d, 2, 3, 4, 5, 6]);
        let (_, _, rgb) = t("crlf", &crlf).unwrap();
        assert_eq!(rgb, vec![0x0d, 2, 3, 4, 5, 6]);
        // 头部内注释（next_token 对 # 截断）。
        let mut incomment = b"P6\n2 # width done\n1\n255\n".to_vec();
        incomment.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        let (w, h, rgb) = t("ic", &incomment).unwrap();
        assert_eq!((w, h), (2, 1));
        assert_eq!(rgb, vec![1, 2, 3, 4, 5, 6]);
        // 拒面：非 P6 magic。
        assert!(t("p5", b"P5\n2 1\n255\n").is_err());
        // 拒面：maxval != 255。
        assert!(t("max65535", b"P6\n2 1\n65535\n").is_err());
        // 拒面：像素数据截断。
        let short = b"P6\n2 1\n255\n\x01\x02\x03";
        assert!(t("trunc", short).is_err());
        // 拒面：头部截断（无 maxval）。
        assert!(t("nohdr", b"P6\n2 1\n").is_err());
        // 拒面：maxval 后无分隔符（数据顶贴）。
        assert!(t("nodelim", b"P6\n2 1\n255").is_err());
        // 拒面：坏 token（非数字宽）。
        assert!(t("badw", b"P6\nxx 1\n255\n").is_err());
        // 拒面：空文件。
        assert!(t("empty", b"").is_err());
        // 拒面：宽=0 合法但零像素（need=0 通过——钉住契约：不误拒）。
        let z = t("zw", b"P6\n0 1\n255\n").unwrap();
        assert_eq!(z, (0, 1, Vec::new()));
    }
}
