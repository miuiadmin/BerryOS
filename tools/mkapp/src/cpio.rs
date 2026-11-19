//! W1-Q27 四刀：cpio(newc) 走查器 + SHA256SUMS 复验（Python `mkapp.py`
//! `_cpio_entries`/`_verify_sums` 镜像——彻底 Rust 化轨道 b 层 #4 刀序
//! 第四位第四刀）。
//!
//! 范围 = `check_bap` 的读侧纯函数两件：
//! - [`cpio_entries`]：newc 走查（magic `070701` + 13 × hex8、filesize =
//!   域[6]、namesize = 域[11]、名起 off+110 剥尾 NUL、头+名/数据双垫页
//!   4096（X-3c：与 mkcpio `entry()` 编码、berry-loader `cpio_walk`、
//!   kernel bootparams 五点一动的同口径）、`TRAILER!!!` 终止）。越界/
//!   坏魔数 = `Reject`（敌意包 loud，不静默截断）。
//! - [`verify_sums`]：LF 收尾、行数 = 条目数、`<64hex>␣␣<名>` 行形、
//!   行序 = 条目名字节序、逐文件 SHA-256 重算比对（§4.4 步 4）。
//!
//! SHA-256 为零依赖自包含实现（FIPS 180-4；tools/mkimage `sha256_hex`
//! 同款——各独立小工作区不共 crate，代码镜像非依赖引用）。
//!
//! 与 Python 单源对齐义务：改任一侧必须同步另一侧并重钉对拍向量。
//! 产线零触碰——tools/mkapp.py 现役不动，五刀产链换轨时接线。


use crate::gate::Reject;

/// 包体帽（Python `BAP_MAX` 镜像）。
pub const BAP_MAX: usize = 4 * 1024 * 1024;
/// 头+名垫、数据垫的页对齐粒度（X-3c；Python 侧字面 4095/~4095 位掩码）。
pub const CPIO_PAD: usize = 4096;

/// 拒因构造（byi.rs `rej` 同款：`where_` 运行期串 per 拒一次 leak——
/// 敌意包诊断的运行期 where 与静态串纪律的折中，与三刀先例一致）。
fn rej(msg: String, code: &'static str, where_: &str) -> Reject {
    Reject {
        message: msg,
        code,
        where_: Box::leak(where_.to_string().into_boxed_str()),
        line: None, jx_where: None, jx_expected: None, jx_got: None, jx_fix: None,
    }
}

/// cpio(newc) 走查（Python `_cpio_entries` 逐分支镜像）。
///
/// 返回 `(name, data)` 序列（TRAILER 前全部条目，保序）。任一拒因 =
/// `Reject`（code 族 `bap.cpio.*`——header/range 与 Python 同名）。
pub fn cpio_entries(blob: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Reject> {
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut off = 0usize;
    loop {
        if off.checked_add(110).map(|e| e > blob.len()).unwrap_or(true) {
            return Err(rej(
                format!("bap: cpio 头越界 @ {off}"),
                "bap.cpio.header",
                &format!("cpio@{off}"),
            ));
        }
        if &blob[off..off + 6] != b"070701" {
            return Err(rej(
                format!("bap: cpio 坏魔数 @ {off}"),
                "bap.cpio.header",
                &format!("cpio@{off}"),
            ));
        }
        let mut fields = [0u32; 13];
        for (i, f) in fields.iter_mut().enumerate() {
            let s = off + 6 + i * 8;
            let e = s + 8;
            if e > blob.len() {
                return Err(rej(
                    format!("bap: cpio 坏字段 @ {off}"),
                    "bap.cpio.header",
                    &format!("cpio@{off}"),
                ));
            }
            let Ok(txt) = std::str::from_utf8(&blob[s..e]) else {
                return Err(rej(
                    format!("bap: cpio 坏字段 @ {off}"),
                    "bap.cpio.header",
                    &format!("cpio@{off}"),
                ));
            };
            match u32::from_str_radix(txt, 16) {
                Ok(v) => *f = v,
                Err(_) => {
                    return Err(rej(
                        format!("bap: cpio 坏字段 @ {off}"),
                        "bap.cpio.header",
                        &format!("cpio@{off}"),
                    ))
                }
            }
        }
        let (filesize, namesize) = (fields[6] as usize, fields[11] as usize);
        if namesize < 1 {
            return Err(rej(
                format!("bap: cpio 名长非法 @ {off}"),
                "bap.cpio.header",
                &format!("cpio@{off}"),
            ));
        }
        let name_off = off + 110;
        if name_off.checked_add(namesize).map(|e| e > blob.len()).unwrap_or(true) {
            return Err(rej(
                format!("bap: cpio 名越界 @ {off}"),
                "bap.cpio.range",
                &format!("cpio@{off}"),
            ));
        }
        let name = blob[name_off..name_off + namesize - 1].to_vec(); // 剥尾 NUL
        let data_off = (name_off + namesize + CPIO_PAD - 1) & !(CPIO_PAD - 1); // 头+名垫页
        if data_off.checked_add(filesize).map(|e| e > blob.len()).unwrap_or(true) {
            return Err(rej(
                format!("bap: cpio 数据越界 @ {off}"),
                "bap.cpio.range",
                &format!("cpio@{off}"),
            ));
        }
        if name == b"TRAILER!!!" {
            break;
        }
        out.push((name, blob[data_off..data_off + filesize].to_vec()));
        off = (data_off + filesize + CPIO_PAD - 1) & !(CPIO_PAD - 1); // 数据垫页
    }
    Ok(out)
}

/// SHA256SUMS 行解析：`<64 小写 hex>␣␣<名>`（`SUMS_LINE_RE.fullmatch` 镜像）。
fn parse_sums_line(line: &[u8]) -> Option<(&[u8], &[u8])> {
    if line.len() < 66 || &line[64..66] != b"  " {
        return None;
    }
    let digest = &line[..64];
    if !digest.iter().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return None;
    }
    let name = &line[66..];
    if name.is_empty() || name.contains(&b'\n') || name.contains(&b'\r') {
        return None;
    }
    Some((digest, name))
}

/// SHA256SUMS 复验（Python `_verify_sums` 逐分支镜像；§4.4 步 4）。
///
/// `payload` = 除 SHA256SUMS 外的全部条目（行序基准 = 名字节序排序，
/// 与 Python `sorted(payload)` 同——元组序即首键名字节序）。
pub fn verify_sums(sums: &[u8], payload: &[(Vec<u8>, Vec<u8>)]) -> Result<(), Reject> {
    if !sums.ends_with(b"\n") {
        return Err(rej("bap: SHA256SUMS 应 LF 收尾".to_string(), "bap.sums", "SHA256SUMS"));
    }
    let lines: Vec<&[u8]> = sums[..sums.len() - 1].split(|b| *b == b'\n').collect();
    let mut expect: Vec<&(Vec<u8>, Vec<u8>)> = payload.iter().collect();
    expect.sort_by(|a, b| a.0.cmp(&b.0));
    if lines.len() != expect.len() {
        return Err(rej(
            format!("bap: SHA256SUMS 行数 {} != 条目数 {}", lines.len(), expect.len()),
            "bap.sums",
            "SHA256SUMS",
        ));
    }
    for (line, (name, data)) in lines.iter().zip(expect.iter()) {
        let Some((digest, lname)) = parse_sums_line(line) else {
            return Err(rej(
                format!(
                    "bap: SHA256SUMS 行形/行序不符 {}",
                    String::from_utf8_lossy(line)
                ),
                "bap.sums",
                "SHA256SUMS",
            ));
        };
        if lname != &name[..] {
            return Err(rej(
                format!(
                    "bap: SHA256SUMS 行形/行序不符 {}",
                    String::from_utf8_lossy(line)
                ),
                "bap.sums",
                "SHA256SUMS",
            ));
        }
        if digest != sha256_hex(data).as_bytes() {
            return Err(rej(
                format!(
                    "bap: SHA256SUMS 摘要不符 {}",
                    String::from_utf8_lossy(name)
                ),
                "bap.sums",
                "SHA256SUMS",
            ));
        }
    }
    Ok(())
}

// ---- SHA-256（FIPS 180-4 自包含实现，tools/mkimage sha256_hex 镜像）----

/// SHA-256 小写 hex（64 位定宽）。
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bitlen = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 编码器垫片（mkcpio `entry()` 同口径：头+名垫页、数据垫页 4096）----
    // 测试侧自建最小 newc 编码，与 Python selftest 的合成件面等价；产线
    // 对拍腿（mkcpio 真流）见 cpio_walk_python_stream_cross_check。

    fn hex8(v: u32) -> [u8; 8] {
        format!("{v:08x}").into_bytes().try_into().unwrap()
    }

    fn entry(name: &[u8], data: &[u8], ino: u32) -> Vec<u8> {
        let namesize = name.len() + 1;
        let mut out = b"070701".to_vec();
        let vals = [
            ino, 0o100755, 0, 0, 1, 0, data.len() as u32, 0, 0, 0, 0, namesize as u32, 0,
        ];
        for v in vals {
            out.extend_from_slice(&hex8(v));
        }
        out.extend_from_slice(name);
        out.push(0);
        while out.len() % CPIO_PAD != 0 {
            out.push(0);
        }
        out.extend_from_slice(data);
        while out.len() % CPIO_PAD != 0 {
            out.push(0);
        }
        out
    }

    fn build_archive(items: &[(&[u8], &[u8])]) -> Vec<u8> {
        let mut blob = Vec::new();
        for (i, (name, data)) in items.iter().enumerate() {
            blob.extend_from_slice(&entry(name, data, i as u32 + 1));
        }
        blob.extend_from_slice(&entry(b"TRAILER!!!", b"", 0));
        while blob.len() % CPIO_PAD != 0 {
            blob.push(0);
        }
        blob
    }

    #[test]
    fn walk_roundtrip_three_entries() {
        // 编码→走查往返恒等：名/数据保序、TRAILER 剥除、垫页透明。
        let blob = build_archive(&[
            (b"manifest.toml", b"[app]\n"),
            (b"bin/demo", b"\x7fELFpayload"),
            (b"SHA256SUMS", b"abc\n"),
        ]);
        let got = cpio_entries(&blob).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], (b"manifest.toml".to_vec(), b"[app]\n".to_vec()));
        assert_eq!(got[1], (b"bin/demo".to_vec(), b"\x7fELFpayload".to_vec()));
        assert_eq!(got[2], (b"SHA256SUMS".to_vec(), b"abc\n".to_vec()));
    }

    #[test]
    fn walk_empty_file_and_trailer_only() {
        // filesize=0 条目 + 仅 TRAILER 两端：空包走查 = 空（Python 同）。
        let blob = build_archive(&[]);
        assert!(cpio_entries(&blob).unwrap().is_empty());
        let blob2 = build_archive(&[(b"empty", b"")]);
        let got = cpio_entries(&blob2).unwrap();
        assert_eq!(got, vec![(b"empty".to_vec(), Vec::new())]);
    }

    #[test]
    fn walk_header_reject_families() {
        // 拒面五族（Python 同 code）：头越界 / 坏魔数 / 坏字段 / 名长 0 /
        // 名越界。逐族合成 + code/message 钉板。
        let good = entry(b"a", b"x", 1);
        // 坏魔数：首 6 字节改 070702。
        let mut bad = good.clone();
        bad[5] = b'2';
        let r = cpio_entries(&bad).unwrap_err();
        assert_eq!(r.code, "bap.cpio.header");
        assert!(r.message.contains("坏魔数"), "{}", r.message);
        // 头越界：截到 110 内。
        let r = cpio_entries(&good[..80]).unwrap_err();
        assert_eq!(r.code, "bap.cpio.header");
        assert!(r.message.contains("头越界"));
        // 坏字段：namesize 域（域[11]，off+6+11*8=off+94）写非 hex。
        let mut bad2 = good.clone();
        bad2[94..102].copy_from_slice(b"zzzzzzzz");
        let r = cpio_entries(&bad2).unwrap_err();
        assert_eq!(r.code, "bap.cpio.header");
        assert!(r.message.contains("坏字段"));
        // 名长 0：namesize=0（合法 hex 00000000）。
        let mut bad3 = good.clone();
        bad3[94..102].copy_from_slice(b"00000000");
        let r = cpio_entries(&bad3).unwrap_err();
        assert_eq!(r.code, "bap.cpio.header");
        assert!(r.message.contains("名长非法"));
        // 名越界：namesize 声明超包长。
        let mut bad4 = good.clone();
        bad4[94..102].copy_from_slice(b"fffffff0");
        let r = cpio_entries(&bad4).unwrap_err();
        assert_eq!(r.code, "bap.cpio.range");
        assert!(r.message.contains("名越界"));
        assert_eq!(r.where_, "cpio@0");
    }

    #[test]
    fn walk_data_range_reject_and_field_overflow() {
        // 数据越界：filesize 声明超包长（域[6] = off+6+6*8 = off+54）。
        // 0x1000 恰好贴边（4096+4096=8192=包长）不拒——用 4097 越一拒。
        let good = entry(b"a", b"x", 1);
        let mut bad = good.clone();
        bad[54..62].copy_from_slice(b"00001001");
        let r = cpio_entries(&bad).unwrap_err();
        assert_eq!(r.code, "bap.cpio.range");
        assert!(r.message.contains("数据越界"));
        // 域值 > u32（filesize = 0x10000000——8 位 hex 恰 u32 域内但不
        // 越包；再大 9 位非 8 字段宽——Python 侧 int 收后越界拒，本侧
        // 字段宽恒 8 不会遇；用包内越界 0x01000001 同为 range 拒）。
        let mut bad2 = good.clone();
        bad2[54..62].copy_from_slice(b"01000001");
        let r = cpio_entries(&bad2).unwrap_err();
        assert_eq!(r.code, "bap.cpio.range");
        assert!(r.message.contains("数据越界"), "{}", r.message);
    }

    #[test]
    fn walk_second_entry_offset_continuity() {
        // 多条目偏移连续性：第二头位点 = 首条 data_off+filesize 垫页后。
        // 首条头+名(1+1+pad)+数据(3+pad) 恰 2 页 → 第二头 @ 8192。
        let blob = build_archive(&[(b"a", b"xyz"), (b"b", b"q")]);
        assert_eq!(&blob[8192..8198], b"070701");
        let got = cpio_entries(&blob).unwrap();
        assert_eq!(got.len(), 2);
        // 数据越界族在第二头位点同样拒（where_ 带 cpio@8192）。全包长
        // 8192+4096（b 条目页）+4096（TRAILER 页）=16384——filesize 取
        // 0x00003001 越第二条目页拒。
        let mut bad = blob.clone();
        let f2 = 8192 + 54;
        bad[f2..f2 + 8].copy_from_slice(b"00003001");
        let r = cpio_entries(&bad).unwrap_err();
        assert_eq!(r.code, "bap.cpio.range");
        assert_eq!(r.where_, "cpio@8192");
    }

    #[test]
    fn walk_uppercase_hex_rejected_as_field_garbage() {
        // mkcpio 产线恒小写 hex8；大写形在 Python int(x,16) 会收（大小写
        // 均收——berry-loader field 同注），本侧 from_str_radix 亦收。
        // 钉住「大写收」与产线/加载器口径一致（非拒面）。注意单条目包
        // 走查到 TRAILER 后停止——合成件须含 TRAILER 尾（build_archive）。
        let mut blob = build_archive(&[(b"a", b"x")]);
        blob[6..14].copy_from_slice(b"0000000A"); // ino=10 大写 A
        let got = cpio_entries(&blob).unwrap();
        assert_eq!(got.len(), 1);
    }

    // ---- SHA256SUMS 复验 ----

    fn payload() -> Vec<(Vec<u8>, Vec<u8>)> {
        vec![
            (b"bin/demo".to_vec(), b"\x7fELFpayload".to_vec()),
            (b"manifest.toml".to_vec(), b"[app]\n".to_vec()),
        ]
    }

    fn good_sums() -> Vec<u8> {
        let mut p = payload();
        p.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out = Vec::new();
        for (name, data) in p {
            out.extend_from_slice(sha256_hex(&data).as_bytes());
            out.extend_from_slice(b"  ");
            out.extend_from_slice(&name);
            out.push(b'\n');
        }
        out
    }

    #[test]
    fn sums_accept_roundtrip() {
        assert!(verify_sums(&good_sums(), &payload()).is_ok());
    }

    #[test]
    fn sums_reject_no_lf_tail() {
        let mut s = good_sums();
        s.pop();
        let r = verify_sums(&s, &payload()).unwrap_err();
        assert_eq!(r.code, "bap.sums");
        assert!(r.message.contains("LF 收尾"));
    }

    #[test]
    fn sums_reject_count_mismatch() {
        let mut s = good_sums();
        s.extend_from_slice(b"0000000000000000000000000000000000000000000000000000000000000000  extra\n");
        let r = verify_sums(&s, &payload()).unwrap_err();
        assert!(r.message.contains("行数 3 != 条目数 2"), "{}", r.message);
    }

    #[test]
    fn sums_reject_bad_shape_single_space() {
        // 单空格行形拒（SUMS_LINE_RE 双空格钉板）。行数先对齐（2 行），
        // 第二行单空格 → 行形拒。
        let d0 = sha256_hex(b"\x7fELFpayload");
        let d1 = sha256_hex(b"[app]\n");
        let mut s = Vec::new();
        s.extend_from_slice(format!("{d0}  bin/demo\n").as_bytes());
        s.extend_from_slice(format!("{d1} manifest.toml\n").as_bytes());
        let r = verify_sums(&s, &payload()).unwrap_err();
        assert!(r.message.contains("行形/行序不符"), "{}", r.message);
    }

    #[test]
    fn sums_reject_uppercase_digest() {
        let mut s = good_sums();
        for b in s.iter_mut() {
            if b.is_ascii_lowercase() && b.is_ascii_hexdigit() {
                *b = b.to_ascii_uppercase();
            }
        }
        let r = verify_sums(&s, &payload()).unwrap_err();
        assert!(r.message.contains("行形/行序不符") || r.message.contains("摘要不符"), "{}", r.message);
    }

    #[test]
    fn sums_reject_order_mismatch() {
        // 行序 = 条目名字节序：两行整行互换（LF 收尾保持）→ 行形/行序拒
        // （bin/demo < manifest.toml 字节序）。
        let d0 = sha256_hex(b"\x7fELFpayload");
        let d1 = sha256_hex(b"[app]\n");
        let mut s = Vec::new();
        s.extend_from_slice(format!("{d1}  manifest.toml\n").as_bytes());
        s.extend_from_slice(format!("{d0}  bin/demo\n").as_bytes());
        let r = verify_sums(&s, &payload()).unwrap_err();
        assert!(r.message.contains("行形/行序不符"), "{}", r.message);
    }

    #[test]
    fn sums_reject_digest_mismatch() {
        // 行形/行序全对、摘要错一位 → 摘要不符拒（非行形拒）。
        let mut s = good_sums();
        let last = s.len() - 1;
        // 翻转最后一行的首 hex 字符（行首 = 摘要 [0]）。
        let flip_at = s.len() - 1 - (b"manifest.toml\n".len()) - 65;
        let _ = last;
        s[flip_at] = if s[flip_at] == b'0' { b'1' } else { b'0' };
        let r = verify_sums(&s, &payload()).unwrap_err();
        assert!(r.message.contains("摘要不符"), "{}", r.message);
    }

    #[test]
    fn sums_accept_empty_payload() {
        // 空清单 = 恰一行空？不——LF 收尾 + 0 行 = 空 sums（sums=b"\n" 拆
        // 出 1 空行 ≠ 0）。Python split(b"\n") 同形：b"\n"[:-1].split →
        // [b""]。钉住空清单的边界：b"\n" 与空 payload 行数 1≠0 拒。
        let r = verify_sums(b"\n", &[]).unwrap_err();
        assert!(r.message.contains("行数 1 != 条目数 0"));
        // 真空 sums（b""）无 LF 收尾拒。
        assert!(verify_sums(b"", &[]).is_err());
    }

    // ---- SHA-256 标准向量（FIPS 180-4 / NIST）----

    #[test]
    fn sha256_standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // 跨块边界（<64 / 恰 55/56 填充位 / >64）。
        assert_eq!(sha256_hex(&vec![b'a'; 55]).len(), 64);
        assert_eq!(sha256_hex(&vec![b'a'; 56]).len(), 64);
        assert_eq!(sha256_hex(&vec![b'a'; 64]).len(), 64);
        assert_eq!(sha256_hex(&vec![b'a'; 1000]).len(), 64);
        // 与 hashlib 对拍锚（Python hashlib.sha256(b'x'*1000)）。
        assert_eq!(
            sha256_hex(&vec![b'x'; 1000]),
            "44f8354494a5ba03ba1792a8d3e9c534c47a9181980fde7a3f44b06ef2ae7c7f"
        );
    }
}

#[cfg(test)]
mod python_cross {
    use super::*;

    /// Python 现役件对拍（2026-11-19 实产）：
    /// - mkcpio.build_archive 三组真流（20KB×2 + TRAILER-only 4096B）经
    ///   Python `_cpio_entries` 走查恒等回原条目——本测同几何合成件在
    ///   Rust 侧走查同结果（编码几何 = mkcpio `entry()` 五点一动口径）。
    /// - `_verify_sums` accept 真流：hex 向量逐字节钉板（Python
    ///   hashlib 实产摘要）。
    #[test]
    fn cpio_walk_python_stream_cross_check() {
        // case 0：manifest.toml(22B) + bin/demo(36B)——Python 侧 20480B。
        let blob = build_arch(&[
            (b"manifest.toml", b"[app]\nid = \"demo.app\"\n"),
            (b"bin/demo", b"\x7fELFpayload!\x7fELFpayload!\x7fELFpayload!"),
        ]);
        assert_eq!(blob.len(), 20480);
        let got = cpio_entries(&blob).unwrap();
        assert_eq!(got[0].0, b"manifest.toml".to_vec());
        assert_eq!(got[0].1.len(), 22);
        assert_eq!(got[1].0, b"bin/demo".to_vec());
        assert_eq!(got[1].1.len(), 36);
        // case 1：SHA256SUMS(13B) + icons/a.byi(0..=255)——Python 侧 20480B。
        let blob1 = build_arch(&[
            (b"SHA256SUMS", b"digest-lines\n"),
            (b"icons/a.byi", &(0u8..=255).collect::<Vec<u8>>()),
        ]);
        assert_eq!(blob1.len(), 20480);
        let got1 = cpio_entries(&blob1).unwrap();
        assert_eq!(got1[0].0, b"SHA256SUMS".to_vec());
        assert_eq!(got1[0].1.len(), 13);
        assert_eq!(got1[1].1, (0u8..=255).collect::<Vec<u8>>());
        // case 2：仅 TRAILER——Python 侧 4096B、条目零。
        let blob2 = build_arch(&[]);
        assert_eq!(blob2.len(), 4096);
        assert!(cpio_entries(&blob2).unwrap().is_empty());
    }

    #[test]
    fn sums_python_golden_accept() {
        // Python _verify_sums accept 真流 hex（2026-11-19 hashlib 实产）：
        // 行序 = sorted(payload) = bin/demo → manifest.toml。
        const SUMS_HEX: &str = "38643539656633306638613738336334373332326162313363393433643962643363623565346439313737626330653465313136373362636431626234383364202062696e2f64656d6f0a3030336564633137333164336433303964326466316662326663393263636166313533613731613939386631336562353939316561326433323730313830363520206d616e69666573742e746f6d6c0a";
        fn unhex(s: &str) -> Vec<u8> {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect()
        }
        let sums = unhex(SUMS_HEX);
        let payload = vec![
            (b"bin/demo".to_vec(), b"\x7fELFx".to_vec()),
            (b"manifest.toml".to_vec(), b"[app]\n".to_vec()),
        ];
        // 摘要钉板（Python hashlib 双锚 + 本侧 sha256_hex 自证）。
        assert_eq!(
            sha256_hex(b"\x7fELFx"),
            "8d59ef30f8a783c47322ab13c943d9bd3cb5e4d9177bc0e4e11673bcd1bb483d"
        );
        assert_eq!(
            sha256_hex(b"[app]\n"),
            "003edc1731d3d309d2df1fb2fc92ccaf153a71a998f13eb5991ea2d327018065"
        );
        verify_sums(&sums, &payload).unwrap();
    }

    /// mkcpio `entry()`/`build_archive` 同口径编码垫片（tests 模块私有
    /// 重造——与 tests::build_archive 相同几何，供本模组复用）。
    fn build_arch(items: &[(&[u8], &[u8])]) -> Vec<u8> {
        let mut blob = Vec::new();
        for (i, (name, data)) in items.iter().enumerate() {
            blob.extend_from_slice(&enc_entry(name, data, i as u32 + 1));
        }
        blob.extend_from_slice(&enc_entry(b"TRAILER!!!", b"", 0));
        while blob.len() % CPIO_PAD != 0 {
            blob.push(0);
        }
        blob
    }

    fn enc_entry(name: &[u8], data: &[u8], ino: u32) -> Vec<u8> {
        let namesize = name.len() + 1;
        let mut out = b"070701".to_vec();
        let vals = [
            ino, 0o100755, 0, 0, 1, 0, data.len() as u32, 0, 0, 0, 0, namesize as u32, 0,
        ];
        for v in vals {
            out.extend_from_slice(format!("{v:08x}").as_bytes());
        }
        out.extend_from_slice(name);
        out.push(0);
        while out.len() % CPIO_PAD != 0 {
            out.push(0);
        }
        out.extend_from_slice(data);
        while out.len() % CPIO_PAD != 0 {
            out.push(0);
        }
        out
    }
}
