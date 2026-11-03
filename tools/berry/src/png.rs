//! Minimal PNG writer + PPM reader.
//!
//! No image crate: PNG's IDAT is a zlib stream, and zlib "stored" blocks are
//! valid deflate without compression. Screenshots stay dependency-free and
//! byte-exact; a 800x600 RGB frame becomes a ~1.4 MiB PNG, acceptable for a
//! developer screenshot. (P2 can swap in a compressed encoder behind the same
//! API if size matters.)

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
    let z = zlib_store(&raw);

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

fn zlib_store(data: &[u8]) -> Vec<u8> {
    // zlib header: CMF=0x78, FLG=0x01 (no dict, fastest, valid check bits).
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut off = 0;
    while off < data.len() {
        let n = (data.len() - off).min(65535);
        let last = off + n == data.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&(!(n as u16)).to_le_bytes());
        out.extend_from_slice(&data[off..off + n]);
        off += n;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a = 1u32;
    let mut b = 0u32;
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

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
    fn zlib_stored_roundtrip_header() {
        let data = b"hello berry";
        let z = zlib_store(data);
        assert_eq!(&z[..2], &[0x78, 0x01]);
        // final stored block: 0x01, len LE, ~len LE, data
        assert_eq!(z[2], 1);
        assert_eq!(u16::from_le_bytes([z[3], z[4]]) as usize, data.len());
        assert_eq!(&z[7..7 + data.len()], data);
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
