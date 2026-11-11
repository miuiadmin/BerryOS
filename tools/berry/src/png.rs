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
}
