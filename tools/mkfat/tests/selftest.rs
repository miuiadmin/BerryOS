//! 钉板五步第 3/4 步：字节级对拍 + selftest 全绿（Python 版面逐条对齐）。
//!
//! 对拍纪律：同输入 → 产物逐字节恒等（现役「同输入同字节」确定性纪律）。
//! 双路：①Rust 纯面钉板（Python selftest_puts 逐腿对齐——确定性/链回读/
//! `.`/`..`/负样本/几何域）；②与 Python 现役件真跑对拍（CLI 子进程：合成
//! puts → 双方整盘 sha256 恒等——`tools/mkfat.py` 在席时执行；缺席打 skip
//! 〔CI 环境净仓面〕，纯面钉板兜底）。

use mkfat::*;

fn synth_puts() -> Vec<(Vec<u8>, Vec<u8>)> {
    vec![
        (
            b"/EFI/BOOT/BOOTAA64.EFI".to_vec(),
            (0..3000).map(|i| ((i * 7 + 3) % 256) as u8).collect(),
        ),
        (b"/HELLO.TXT".to_vec(), b"berryboot efi hello\n".to_vec()),
    ]
}

fn entries_of(blob: &[u8]) -> Vec<([u8; 11], u8, u16, u32)> {
    let mut out = Vec::new();
    for i in 0..blob.len() / 32 {
        let e = &blob[i * 32..(i + 1) * 32];
        if e[0] == 0 {
            break;
        }
        let mut n11 = [0u8; 11];
        n11.copy_from_slice(&e[0..11]);
        out.push((
            n11,
            e[11],
            u16::from_le_bytes([e[26], e[27]]),
            u32::from_le_bytes([e[28], e[29], e[30], e[31]]),
        ));
    }
    out
}

fn chain_read(vol: &[u8], geo: &Geo, fat: &[u8], first: u16, size: u32, fat16: bool) -> Vec<u8> {
    let mut data = Vec::new();
    let mut c = first as u32;
    let eoc_min = if fat16 { 0xFFF8u32 } else { 0xFF8 };
    while (2..eoc_min).contains(&c) {
        let off = (geo.data_start + (c as usize - 2) * geo.spc) * SECTOR;
        data.extend_from_slice(&vol[off..off + geo.spc * SECTOR]);
        c = if fat16 {
            fat16_get(fat, c as usize) as u32
        } else {
            fat12_get(fat, c as usize) as u32
        };
    }
    data.truncate(size as usize);
    data
}

#[test]
fn puts_determinism_and_layout() {
    // 确定性 + 根布局 + `.`/`..` 嵌套 + 链回读对拍源（Python selftest_puts
    // 主腿对齐）。
    let puts = synth_puts();
    let (vol, geo) = build_volume_puts(&puts, DISK_SECTORS, false, 0).unwrap();
    let (vol2, _) = build_volume_puts(&puts, DISK_SECTORS, false, 0).unwrap();
    assert_eq!(vol, vol2, "--put 不确定");
    assert_eq!(vol.len(), DISK_SECTORS * SECTOR);
    let fat = vol[SECTOR..(1 + geo.fat_sz) * SECTOR].to_vec();
    let root = &vol[(geo.data_start - 1) * SECTOR..geo.data_start * SECTOR];
    let ents = entries_of(root);
    let find = |n: &[u8; 11]| ents.iter().find(|e| &e.0 == n).copied().unwrap();
    assert!(find(b"BERRYBOOT  ").1 == 0x08, "卷标项缺失");
    let efi = find(b"EFI        ");
    assert_eq!(efi.1, 0x10, "EFI 非 attr 0x10");
    let efi_dir_blob = chain_read(&vol, &geo, &fat, efi.2, 2048, false);
    let efi_dir = entries_of(&efi_dir_blob);
    let f = |n: &[u8; 11]| efi_dir.iter().find(|e| &e.0 == n).copied().unwrap();
    assert!(f(b".          ").0 == *b".          " && f(b"..         ").0 == *b"..         ");
    assert_eq!(f(b".          ").2, efi.2, "`.` 簇不自指");
    assert_eq!(f(b"..         ").2, 0, "`..` 应指根(0)");
    let boot = f(b"BOOT       ");
    let boot_dir = entries_of(&chain_read(&vol, &geo, &fat, boot.2, 2048, false));
    let bf = |n: &[u8; 11]| boot_dir.iter().find(|e| &e.0 == n).copied().unwrap();
    assert_eq!(bf(b"..         ").2, efi.2, "BOOT 的 `..` 应指 EFI 簇");
    let app = bf(b"BOOTAA64EFI");
    assert_eq!(
        chain_read(&vol, &geo, &fat, app.2, app.3, false),
        puts[0].1,
        "BOOTAA64.EFI 链回读不等于源"
    );
    let hello = find(b"HELLO   TXT");
    assert_eq!(
        chain_read(&vol, &geo, &fat, hello.2, hello.3, false),
        puts[1].1,
        "HELLO.TXT 链回读不等于源"
    );
    // FAT 保留项：FAT[0] 低字节 = media 0xF0、FAT[1]=EOC。
    assert_eq!(fat12_get(&fat, 0), 0xFF0, "FAT[0]");
    assert_eq!(fat12_get(&fat, 1), 0xFFF, "FAT[1]");
}

#[test]
fn fat12_fat16_codec_roundtrip() {
    // 编码叶往返：set 后 get 恒等（0x000..0xFFF 全域抽走查）。
    let mut fat = vec![0u8; 4096];
    for v in [0u16, 1, 2, 0xFF8, 0xFFF, 0xABC, 0x123] {
        for n in [0usize, 1, 2, 3, 100, 999] {
            fat12_set(&mut fat, n, v);
            assert_eq!(fat12_get(&fat, n), v, "fat12 往返破 @{n}={v:03X}");
        }
    }
    let mut f16 = vec![0u8; 4096];
    for v in [0u16, 1, 0xFFF7, 0xFFF8, 0xFFFF] {
        fat16_set(&mut f16, 42, v);
        assert_eq!(fat16_get(&f16, 42), v);
    }
    // fat12 紧包错位守卫：相邻项互不串扰（偶/奇交错写后全读回）。
    let mut fat = vec![0u8; 8];
    fat12_set(&mut fat, 0, 0xABC);
    fat12_set(&mut fat, 1, 0x123);
    assert_eq!((fat12_get(&fat, 0), fat12_get(&fat, 1)), (0xABC, 0x123));
}

#[test]
fn path_comps_negative_faces() {
    // 拒面全谱（Python selftest_puts 负样本逐条对齐 + 域内样本）。
    for bad in [
        &b"/TOOLONGNAME.EFI"[..],
        b"/EFI//BOOT/X",
        b"/a.b.c",
        b"EFI/BOOT/X",
        b"/EFI/",
        b"/x/y\xE9.EFI",
        b"/BO OT.EFI",
    ] {
        assert!(fat_path_comps(bad).is_err(), "坏路径 {bad:?} 未被拒绝");
    }
    let ok = fat_path_comps(b"/EFI/BOOT/BOOTAA64.EFI").unwrap();
    assert_eq!(ok, vec![b"EFI".to_vec(), b"BOOT".to_vec(), b"BOOTAA64.EFI".to_vec()]);
    // 小写归一（Python seg.upper() 面）。
    let ok2 = fat_path_comps(b"/efi/boot/bootaa64.efi").unwrap();
    assert_eq!(ok2, ok);
    // 重复路径拒。
    let dup = vec![
        (b"/A.TXT".to_vec(), b"x".to_vec()),
        (b"/A.TXT".to_vec(), b"y".to_vec()),
    ];
    assert!(build_volume_puts(&dup, DISK_SECTORS, false, 0).is_err(), "重复路径未被拒绝");
}

#[test]
fn size_parametrization_and_spc_adapt() {
    // 尺寸参数化面（V-0b 裁决 C）：4MiB 卷多簇大档——fat_sz 收敛 >2、
    // 链回读仍等于源；SPC 自适应（V-1c）：8MiB 盘翻倍 spc=8 入 FAT12 域。
    let puts = synth_puts();
    let big: Vec<u8> = (0..1_100_000).map(|i| ((i * 11 + 5) % 256) as u8).collect();
    let puts2: Vec<_> = puts.iter().cloned().chain(std::iter::once((b"/KERNEL.ELF".to_vec(), big.clone()))).collect();
    let (vol2, geo2) = build_volume_puts(&puts2, 8192, false, 0).unwrap();
    assert_eq!(vol2.len(), 8192 * SECTOR);
    assert!(geo2.fat_sz > 2, "4MiB 卷 fat_sz 应收敛 >2（得 {}）", geo2.fat_sz);
    assert_eq!(geo2.spc, SPC, "4MiB 卷 SPC 漂移");
    let fat2 = vol2[SECTOR..(1 + geo2.fat_sz) * SECTOR].to_vec();
    let root2 = &vol2[(geo2.data_start - 1) * SECTOR..geo2.data_start * SECTOR];
    let kern = entries_of(root2).into_iter().find(|e| e.0 == *b"KERNEL  ELF").unwrap();
    assert_eq!(chain_read(&vol2, &geo2, &fat2, kern.2, kern.3, false), big, "KERNEL.ELF 大档链回读不等于源");
    // 8MiB 面：INITRD.BIN 3.5MB → spc 翻倍 8。
    let init: Vec<u8> = (0..3_500_000).map(|i| ((i * 13 + 9) % 256) as u8).collect();
    let puts3: Vec<_> = puts2.into_iter().chain(std::iter::once((b"/INITRD.BIN".to_vec(), init.clone()))).collect();
    let (vol3, geo3) = build_volume_puts(&puts3, 16384, false, 0).unwrap();
    assert_eq!((geo3.spc, geo3.clusters < 4085), (8, true), "8MiB 卷几何异常 spc={} clusters={}", geo3.spc, geo3.clusters);
    let fat3 = vol3[SECTOR..(1 + geo3.fat_sz) * SECTOR].to_vec();
    let root3 = &vol3[(geo3.data_start - 1) * SECTOR..geo3.data_start * SECTOR];
    let icpio = entries_of(root3).into_iter().find(|e| e.0 == *b"INITRD  BIN").unwrap();
    assert_eq!(chain_read(&vol3, &geo3, &fat3, icpio.2, icpio.3, false), init, "INITRD.BIN 链回读不等于源");
}

#[test]
fn fat16_faces() {
    // FAT16 面（V-2）：16384 扇区卷（4087 簇恰贴分界、fat_sz=16 收敛）+
    // fstype 串/FAT[0]/FAT[1] + 链回读 + FAT12 域小卷拒 + total 域。
    let puts16 = vec![
        (b"/HELLO.TXT".to_vec(), b"berryos fat read-only hello\n".to_vec()),
        (b"/PATTERN.BIN".to_vec(), pattern_bin(5000)),
        (b"/DATA/NOTE.TXT".to_vec(), b"berryos fat subdir note\n".to_vec()),
    ];
    let (vol16, geo16) = build_volume_puts(&puts16, FIXTURE16_VOL_SECTORS, true, 0).unwrap();
    let (vol16b, _) = build_volume_puts(&puts16, FIXTURE16_VOL_SECTORS, true, 0).unwrap();
    assert_eq!(vol16, vol16b, "FAT16 --put 不确定");
    assert_eq!((geo16.spc, geo16.fat_sz), (4, 16), "FAT16 卷几何异常");
    assert!((FAT16_MIN_CLUSTERS..=FAT16_MAX_CLUSTERS).contains(&geo16.clusters));
    assert_eq!(&vol16[54..62], b"FAT16   ", "fstype 串缺失");
    let fat16raw = vol16[SECTOR..(1 + geo16.fat_sz) * SECTOR].to_vec();
    assert_eq!((fat16_get(&fat16raw, 0), fat16_get(&fat16raw, 1)), (0xFFF0, 0xFFFF), "FAT16 保留项值错");
    let root16 = &vol16[(geo16.data_start - 1) * SECTOR..geo16.data_start * SECTOR];
    let ents16 = entries_of(root16);
    let find16 = |n: &[u8; 11]| ents16.iter().find(|e| &e.0 == n).copied().unwrap();
    let hello16 = find16(b"HELLO   TXT");
    assert_eq!(chain_read(&vol16, &geo16, &fat16raw, hello16.2, hello16.3, true), puts16[0].1);
    let pat16 = find16(b"PATTERN BIN");
    assert_eq!(chain_read(&vol16, &geo16, &fat16raw, pat16.2, pat16.3, true), puts16[1].1);
    assert_eq!((fat16_get(&fat16raw, 2), fat16_get(&fat16raw, 5)), (FAT16_EOC, FAT16_EOC), "链尾 EOC≠0xFFFF");
    // FAT12 域小卷声明 fat16 = 宽域不符拒。
    assert!(
        build_volume_puts(&[(b"/A.TXT".to_vec(), b"x".to_vec())], 2048, true, 0).is_err(),
        "FAT12 域小卷 --fat16 未被拒绝"
    );
    // total 域（C-2c）：域顶 65535 total16=0xFFFF；≥0x10000 走 total32。
    let (vol_top, geo_top) = build_volume_puts(&[(b"/A.TXT".to_vec(), b"x".to_vec())], 65535, true, 0).unwrap();
    assert_eq!(&vol_top[19..21], &0xFFFFu16.to_le_bytes(), "total16 域顶编码漂移");
    assert_eq!(geo_top.spc, 4);
    let (vol32, geo32) = build_volume_puts(&[(b"/A.TXT".to_vec(), b"x".to_vec())], 96256, true, 0).unwrap();
    assert_eq!(&vol32[19..21], &[0, 0], "total16 应=0");
    assert_eq!(&vol32[32..36], &(96256u32).to_le_bytes(), "total32 应=卷扇区数");
    assert_eq!(geo32.spc, 4);
    let (_, geo_dbl) = build_volume_puts(&[(b"/A.TXT".to_vec(), b"x".to_vec())], 300000, true, 0).unwrap();
    assert_eq!(geo_dbl.spc, 8, "FAT16 翻倍分支未触发（300000 扇区应 spc 4→8）");
    // 域外拒面：total32 上限 / 过小卷。
    assert!(build_volume_puts(&[(b"/A.TXT".to_vec(), b"x".to_vec())], 0x1_0000_0000, true, 0).is_err(), "total32 域外卷未被拒绝");
    assert!(build_volume_puts(&[(b"/A.TXT".to_vec(), b"x".to_vec())], 8, false, 0).is_err(), "过小卷未被拒绝");
}

#[test]
fn default_volume_bake() {
    // 默认盘（Python build_volume 裸调面）：烘焙 ROOT_LISTING 簇锚——
    // HELLO@2 单簇 / PATTERN@3..5 三簇链 / fixture 面子目录簇 6/7。
    let (vol, geo) = build_volume(None, &[]).unwrap();
    assert_eq!(vol.len(), VOL_SECTORS * SECTOR);
    assert_eq!((geo.clusters, geo.fat_sz, geo.data_start, geo.spc), (506, 2, 6, 4));
    let fat = vol[SECTOR..(1 + geo.fat_sz) * SECTOR].to_vec();
    let root = &vol[(geo.data_start - 1) * SECTOR..geo.data_start * SECTOR];
    let ents = entries_of(root);
    let find = |n: &[u8; 11]| ents.iter().find(|e| &e.0 == n).copied().unwrap();
    let hello = find(b"HELLO   TXT");
    assert_eq!((hello.2, hello.3), (2, b"berryos fat read-only hello\n".len() as u32), "HELLO 簇锚漂移");
    assert_eq!(fat12_get(&fat, 2), 0xFFF, "HELLO 链尾");
    let pat = find(b"PATTERN BIN");
    assert_eq!((pat.2, pat.3), (3, 5000), "PATTERN 簇锚漂移");
    // 链走活证：3→4→5→EOC。
    assert_eq!((fat12_get(&fat, 3), fat12_get(&fat, 4), fat12_get(&fat, 5)), (4, 5, 0xFFF));
    assert_eq!(chain_read(&vol, &geo, &fat, pat.2, pat.3, false), pattern_bin(5000));
    // fixture 面（DATA 目录簇 6 + NOTE.TXT 簇 7）。
    let note = b"berryos fat subdir note\n".to_vec();
    let note_children: Vec<(Vec<u8>, Vec<u8>)> = vec![(b"NOTE.TXT".to_vec(), note.clone())];
    let dirs: Vec<(&[u8], &[(Vec<u8>, Vec<u8>)])> = vec![(b"DATA", note_children.as_slice())];
    let (volf, geof) = build_volume(None, &dirs).unwrap();
    assert_eq!((geof.next_clus, geof.data_start), (8, 6), "fixture 簇分配漂移");
    let fatf = volf[SECTOR..(1 + geof.fat_sz) * SECTOR].to_vec();
    let rootf = &volf[(geof.data_start - 1) * SECTOR..geof.data_start * SECTOR];
    let entsf = entries_of(rootf);
    let data = entsf.iter().find(|e| e.0 == *b"DATA       ").copied().unwrap();
    assert_eq!((data.1, data.2, data.3), (0x10, 6, 0), "DATA 目录项漂移");
    let dird = chain_read(&volf, &geof, &fatf, 6, 2048, false);
    let dent = entries_of(&dird);
    let noteent = dent.iter().find(|e| e.0 == *b"NOTE    TXT").copied().unwrap();
    assert_eq!(noteent.2, 7, "NOTE.TXT 簇锚漂移");
    assert_eq!(chain_read(&volf, &geof, &fatf, 7, noteent.3, false), note);
    // 整盘面：保留区前缀全零 + 卷 @LBA16。
    let (disk, _) = build_disk().unwrap();
    assert_eq!(disk.len(), DISK_SECTORS * SECTOR);
    assert!(disk[..RESERVED_ZONE * SECTOR].iter().all(|&b| b == 0), "保留区前缀非零");
    assert_eq!(&disk[RESERVED_ZONE * SECTOR..], &vol[..], "卷前缀漂移");
}

// ─── 与 Python 现役件字节级对拍（钉板五步第 3 步） ─────────────────────
//
// Python `tools/mkfat.py` 在席时：合成 puts → 双方整盘（--put 面）与默认盘
// （裸调面）逐字节恒等。缺席打 skip（CI 净仓面）——纯面钉板兜底。
// 构建纪律同 mkcpio：--target-dir 显式钉 tools/mkfat/cargo-target（旗标
// 优先于任何 CARGO_TARGET_DIR 环境与默认值，三环境产物路径恒定可寻）。

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn python_available() -> bool {
    repo_root().join("tools/mkfat.py").exists()
}

fn sha256_hex(data: &[u8]) -> String {
    // 零依赖 sha256（FIPS 180-4）——mkcpio/elf2pe oracle 对拍同款自包含面。
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0x831c66d5, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70808, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
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
    h.iter().map(|x| format!("{x:08x}")).collect()
}

fn run_cmd(cmd: &[&str], cwd: &std::path::Path) -> (bool, String, String) {
    match std::process::Command::new(cmd[0]).args(&cmd[1..]).current_dir(cwd).output() {
        Ok(o) => (
            o.status.success(),
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        ),
        Err(_) => (false, String::new(), String::new()),
    }
}

#[test]
fn oracle_byte_parity() {
    let _ = run_oracle_byte_parity();
}

fn run_oracle_byte_parity() -> Option<()> {
    if !python_available() {
        eprintln!("mkfat: tools/mkfat.py 缺席——CLI 对拍 skip（纯面钉板兜底）");
        return None;
    }
    let td = std::env::temp_dir().join(format!("mkfat-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&td);
    std::fs::create_dir_all(&td).unwrap();
    // 布景：--put 面（嵌套目录 + 多簇文件）。
    let efi = td.join("BOOTAA64.EFI");
    std::fs::write(&efi, (0..3000).map(|i| ((i * 7 + 3) % 256) as u8).collect::<Vec<_>>()).unwrap();
    let hello = td.join("HELLO.TXT");
    std::fs::write(&hello, b"berryboot efi hello\n").unwrap();
    // Rust CLI：先 build 再跑（--target-dir 钉位纪律同 mkcpio）。
    let repo = repo_root();
    let host = {
        let (_, o, _) = run_cmd(&["rustc", "-vV"], &repo);
        o.lines().find_map(|l| l.strip_prefix("host: ").map(str::to_string)).unwrap_or_default()
    };
    let cargo_target = repo.join("tools/mkfat/cargo-target");
    let (ok, _, err) = run_cmd(
        &[
            "cargo",
            "build",
            "--release",
            "--manifest-path",
            "tools/mkfat/Cargo.toml",
            "--target",
            &host,
            "--target-dir",
            cargo_target.to_str().unwrap(),
        ],
        &repo,
    );
    if !ok {
        panic!("Rust mkfat build 失败：{err}");
    }
    let rust_bin = cargo_target.join(&host).join("release").join("mkfat");
    assert!(rust_bin.exists(), "Rust mkfat 产物缺席：{}", rust_bin.display());
    // 面 1：--put 整盘对拍。
    let py_put = td.join("py.img");
    let (ok, _, err) = run_cmd(
        &[
            "python3",
            "tools/mkfat.py",
            py_put.to_str().unwrap(),
            "--put",
            &format!("{}:/EFI/BOOT/BOOTAA64.EFI", efi.display()),
            "--put",
            &format!("{}:/HELLO.TXT", hello.display()),
        ],
        &repo,
    );
    assert!(ok, "Python --put 跑挂：{err}");
    let rs_put = td.join("rs.img");
    let (ok, _, err) = run_cmd(
        &[
            rust_bin.to_str().unwrap(),
            rs_put.to_str().unwrap(),
            "--put",
            &format!("{}:/EFI/BOOT/BOOTAA64.EFI", efi.display()),
            "--put",
            &format!("{}:/HELLO.TXT", hello.display()),
        ],
        &repo,
    );
    assert!(ok, "Rust --put 跑挂：{err}");
    let py = std::fs::read(&py_put).unwrap();
    let rs = std::fs::read(&rs_put).unwrap();
    assert_eq!(py.len(), rs.len(), "--put 整盘长度不等");
    assert_eq!(sha256_hex(&py), sha256_hex(&rs), "--put 整盘字节级对拍破（sha256 不等）");
    eprintln!("mkfat: oracle --put 对拍 PASS（{}B sha256={}）", py.len(), sha256_hex(&py));
    // 面 2：默认盘（裸调）对拍。
    let py_disk = td.join("py-disk.img");
    let (ok, _, err) = run_cmd(&["python3", "tools/mkfat.py", py_disk.to_str().unwrap()], &repo);
    assert!(ok, "Python 裸调跑挂：{err}");
    let rs_disk = td.join("rs-disk.img");
    let (ok, _, err) = run_cmd(&[rust_bin.to_str().unwrap(), rs_disk.to_str().unwrap()], &repo);
    assert!(ok, "Rust 裸调跑挂：{err}");
    let pyd = std::fs::read(&py_disk).unwrap();
    let rsd = std::fs::read(&rs_disk).unwrap();
    assert_eq!(pyd.len(), rsd.len(), "默认盘长度不等");
    assert_eq!(sha256_hex(&pyd), sha256_hex(&rsd), "默认盘字节级对拍破");
    eprintln!("mkfat: oracle 默认盘对拍 PASS（{}B sha256={}）", pyd.len(), sha256_hex(&pyd));
    // 面 3：FAT16 8MiB 卷对拍（16384 扇区 = FIXTURE16_VOL_SECTORS 同域，
    // 4087 簇恰贴 FAT12/16 分界、fat_sz 收敛 16）。
    let py16 = td.join("py16.img");
    let (ok, _, err) = run_cmd(
        &[
            "python3",
            "tools/mkfat.py",
            py16.to_str().unwrap(),
            "--put",
            &format!("{}:/EFI/BOOT/BOOTAA64.EFI", efi.display()),
            "--size-mib",
            "8",
            "--fat16",
        ],
        &repo,
    );
    assert!(ok, "Python fat16 跑挂：{err}");
    let rs16 = td.join("rs16.img");
    let (ok, _, err) = run_cmd(
        &[
            rust_bin.to_str().unwrap(),
            rs16.to_str().unwrap(),
            "--put",
            &format!("{}:/EFI/BOOT/BOOTAA64.EFI", efi.display()),
            "--size-mib",
            "8",
            "--fat16",
        ],
        &repo,
    );
    assert!(ok, "Rust fat16 跑挂：{err}");
    let p16 = std::fs::read(&py16).unwrap();
    let r16 = std::fs::read(&rs16).unwrap();
    assert_eq!(p16.len(), 8 * 1024 * 1024);
    assert_eq!(sha256_hex(&p16), sha256_hex(&r16), "FAT16 8MiB 卷字节级对拍破");
    eprintln!("mkfat: oracle FAT16 对拍 PASS（{}B sha256={}）", p16.len(), sha256_hex(&p16));
    let _ = std::fs::remove_dir_all(&td);
    Some(())
}
