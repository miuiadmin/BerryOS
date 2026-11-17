//! mkfat——确定性 FAT12/FAT16 造盘器 Rust 件（彻底 Rust 化轨道 b 层 #8
//! 刀序第二位首刀，钉板五步 2/3/4：独立小工作区 + 字节级对拍 + selftest；
//! 产链换轨留二刀）。
//!
//! Python 现役件 `tools/mkfat.py` 逐面对齐（docs/彻底Rust化计划-v1.0.md §5）：
//! - 默认盘构造面 `build_volume`（FAT12 冒烟盘：vblk 保留区前缀 + 卷 @LBA16、
//!   ROOT_LISTING 烘焙文件、可选 FIXTURE_DIRS 扁平子目录）；
//! - EFI 引导盘构造面 `build_volume_puts`（FAT12/FAT16 双宽、任意层嵌套、
//!   `.`/`..` 目录项、SPC 自适应、fat_sz 迭代收敛、total16/total32 域、
//!   hidden 参数化）；
//! - 路径校验面 `fat_path_comps`（8.3 短名 loud 拒面全谱）；
//! - FAT 编码叶 `fat12_set`/`fat16_set`/`fat12_get`/`fat16_get`、
//!   `dir_entry`/`name_to_11`/`clusters_pad`。
//!
//! 确定性纪律同 Python：无时刻输入（DATE/TIME/VOLUME_ID 定值）、字段全定宽
//! ——同输入同字节。fixture 面（--fixture/--check-fixture16 等文件 IO 模式）
//! 归 CLI 层；首刀只钉**纯编码面**，对拍以 build_volume/build_volume_puts
//! 的卷字节为锚。

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

pub const SECTOR: usize = 512;
pub const DISK_SECTORS: usize = 2048; // 1MiB（vblk EXPECT_SECTORS 对账）
pub const RESERVED_ZONE: usize = 16; // vblk 测试保留区（LBA 0..15）
pub const VOL_SECTORS: usize = DISK_SECTORS - RESERVED_ZONE; // 2032
pub const SPC: usize = 4; // sectors per cluster
pub const NUM_FATS: usize = 2;
pub const ROOT_ENTRIES: usize = 16;
pub const VOLUME_ID: u32 = 0x42525953; // "BRS" 定值——确定性
pub const DATE: u16 = 0x5021; // 2020-01-01（(40<<9)|(1<<5)|1）
pub const TIME: u16 = 0x0000;

// FAT16 域值常量（V-2 能力层）：与 berry-fatfs 双端同值钉死（MS FAT spec）。
pub const FAT16_MIN_CLUSTERS: u32 = 4085;
pub const FAT16_MAX_CLUSTERS: u32 = 65524;
pub const FAT16_EOC: u16 = 0xFFFF;
pub const FAT16_BAD: u16 = 0xFFF7;

/// 默认盘烘焙根文件（Python ROOT_LISTING 同源）。
pub const ROOT_LISTING: &[(&[u8], &[u8])] = &[
    (b"HELLO.TXT", b"berryos fat read-only hello\n"),
    // PATTERN.BIN = (i*31+7)%256 × 5000——链走活证（3 簇）。
];

/// fixture 专列子目录（C-0 刀 3）——默认卷不含（QEMU 冒烟盘零漂移硬线）。
pub const FIXTURE_DIRS: &[(&[u8], &[(&[u8], &[u8])])] =
    &[(b"DATA", &[(b"NOTE.TXT", b"berryos fat subdir note\n")])];

/// FAT16 共识基准专列 puts（V-2）——内容与 FAT12 fixture 同串，几何翻宽。
pub const FIXTURE16_VOL_SECTORS: usize = 16384;

fn u16b(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

fn u32b(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

/// PATTERN.BIN 内容律：(i*31+7)%256——Python bytes((i*31+7)%256 for i in range(5000))。
pub fn pattern_bin(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 31 + 7) % 256) as u8).collect()
}

/// FAT12 表项写入（12 位紧包：偶项低 12 位 / 奇项高 12 位）。
pub fn fat12_set(fat: &mut [u8], n: usize, val: u16) {
    let off = n * 3 / 2;
    if n % 2 == 0 {
        fat[off] = (val & 0xFF) as u8;
        fat[off + 1] = (fat[off + 1] & 0xF0) | ((val >> 8) & 0x0F) as u8;
    } else {
        fat[off] = (fat[off] & 0x0F) | ((val & 0x0F) << 4) as u8;
        fat[off + 1] = ((val >> 4) & 0xFF) as u8;
    }
}

/// FAT12 表项读取（fat12_set 的逆——链回读对拍用）。
pub fn fat12_get(fat: &[u8], n: usize) -> u16 {
    let off = n * 3 / 2;
    if n % 2 == 0 {
        fat[off] as u16 | ((fat[off + 1] as u16 & 0x0F) << 8)
    } else {
        (fat[off] as u16 >> 4) | ((fat[off + 1] as u16) << 4)
    }
}

/// FAT16 表项写入（u16le@2n 定宽）。
pub fn fat16_set(fat: &mut [u8], n: usize, val: u16) {
    let off = n * 2;
    fat[off..off + 2].copy_from_slice(&u16b(val));
}

/// FAT16 表项读取（fat16_set 的逆）。
pub fn fat16_get(fat: &[u8], n: usize) -> u16 {
    let off = n * 2;
    u16::from_le_bytes([fat[off], fat[off + 1]])
}

/// 32B 目录项（Python dir_entry 同形：attr/wtime/wdate/首簇/size 定宽落位）。
pub fn dir_entry(name11: &[u8], attr: u8, clus: u16, size: u32) -> [u8; 32] {
    let mut e = [0u8; 32];
    e[0..11].copy_from_slice(&name11[..11]);
    e[11] = attr;
    e[22..24].copy_from_slice(&u16b(TIME));
    e[24..26].copy_from_slice(&u16b(DATE));
    e[26..28].copy_from_slice(&u16b(clus));
    e[28..32].copy_from_slice(&u32b(size));
    e
}

/// 8.3 名 → 11B 填空名（Python name_to_11：base 左填 8 + ext 左填 3）。
pub fn name_to_11(n: &[u8]) -> [u8; 11] {
    let base_end = n.iter().position(|&c| c == b'.').unwrap_or(n.len());
    let (base, ext) = (&n[..base_end], &n[(base_end + 1).min(n.len())..]);
    let mut out = [b' '; 11];
    out[..8.min(base.len())].copy_from_slice(&base[..8.min(base.len())]);
    let eoff = 8 + 3.min(ext.len());
    out[8..eoff].copy_from_slice(&ext[..3.min(ext.len())]);
    out
}

/// 文件占用的整簇字节数（向上取整，至少 1 簇）。
pub fn clusters_pad(size: usize, clus_sz: usize) -> usize {
    if size <= clus_sz {
        clus_sz
    } else {
        (size + clus_sz - 1) / clus_sz * clus_sz
    }
}

/// 卷几何摘要（Python geo dict 面）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Geo {
    pub clusters: u32,
    pub fat_sz: usize,
    pub data_start: usize,
    pub next_clus: u32,
    pub spc: usize,
}

/// mkfat 错误面（Python SystemExit/loud FAIL 的 Rust 化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MkfatError {
    /// 路径拒面（fat_path_comps 全谱——非绝对/空段/`.`/`..`/非 8.3/非 ASCII）。
    Path(String),
    /// puts 面拒面（中途撞文件/重复路径/目录超单簇/根项超帽/簇超帽/域外）。
    Puts(String),
    /// 卷几何域外（FAT16 声明与簇数域不符等 assert 面）。
    Geometry(String),
}

impl std::fmt::Display for MkfatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MkfatError::Path(m) | MkfatError::Puts(m) | MkfatError::Geometry(m) => {
                write!(f, "mkfat: --put FAIL: {m}")
            }
        }
    }
}

impl std::error::Error for MkfatError {}

/// 引导盘 FAT 路径 → 8.3 分量列表（bytes、已大写）。loud 拒面与 Python
/// fat_path_comps 逐条对齐：非绝对 / 根路径 / 空段 / `.`/`..` / 非 ASCII /
/// 非 [A-Z0-9] / 主名超 8 / 扩展超 3 / 多点。
pub fn fat_path_comps(path: &[u8]) -> Result<Vec<Vec<u8>>, MkfatError> {
    let path_str = String::from_utf8_lossy(path).into_owned();
    if !path.starts_with(b"/") {
        return Err(MkfatError::Path(format!("路径须绝对（/{path_str}）")));
    }
    let raw: Vec<&[u8]> = path[1..].split(|&c| c == b'/').collect();
    if raw == vec![&b""[..]] {
        return Err(MkfatError::Path("路径为根（无文件名）".into()));
    }
    let mut comps = Vec::new();
    for seg in raw {
        if seg.is_empty() {
            return Err(MkfatError::Path(format!("空路径段（{path_str}）")));
        }
        if seg == b"." || seg == b".." {
            return Err(MkfatError::Path(format!("`.`/`..` 段（{path_str}）")));
        }
        if !seg.is_ascii() {
            return Err(MkfatError::Path(format!(
                "非 ASCII 段（{}）",
                String::from_utf8_lossy(seg)
            )));
        }
        let up: Vec<u8> = seg.to_ascii_uppercase();
        let base_end = up.iter().position(|&c| c == b'.').unwrap_or(up.len());
        let (base, ext) = (&up[..base_end], &up[(base_end + 1).min(up.len())..]);
        let body = {
            let mut b = base.to_vec();
            b.extend_from_slice(ext);
            b
        };
        let ok = !base.is_empty()
            && base.len() <= 8
            && ext.len() <= 3
            && !ext.contains(&b'.')
            && body.iter().all(|&c| c.is_ascii_uppercase() || c.is_ascii_digit());
        if !ok {
            return Err(MkfatError::Path(format!(
                "`{}` 非 8.3 短名（[A-Z0-9]、主名≤8、扩展≤3、单点——LFN 留后续）",
                String::from_utf8_lossy(seg)
            )));
        }
        comps.push(up);
    }
    Ok(comps)
}

/// 烘焙文件面：listing（根文件）与 dirs（扁平子目录，fixture 专列形态）。
/// 签名对齐 build_volume_puts 的 puts 域以便共享构造器。
type Listing = Vec<(Vec<u8>, Vec<u8>)>;

fn root_listing_default() -> Listing {
    let mut v: Listing = ROOT_LISTING.iter().map(|(n, d)| (n.to_vec(), d.to_vec())).collect();
    v.push((b"PATTERN.BIN".to_vec(), pattern_bin(5000)));
    v
}

/// 构建默认 FAT12 卷（卷相对 LBA 0 起）——Python build_volume 逐字节对齐。
/// listing=None 走 ROOT_LISTING 烘焙面；dirs = fixture 扁平子目录（默认盘空）。
/// 簇分配序 = 根文件名序 → 逐目录名序（目录簇先于其子文件簇）——确定性。
pub fn build_volume(
    listing: Option<&[(Vec<u8>, Vec<u8>)]>,
    dirs: &[(&[u8], &[(Vec<u8>, Vec<u8>)])],
) -> Result<(Vec<u8>, Geo), MkfatError> {
    let owned: Listing = match listing {
        Some(l) => l.to_vec(),
        None => root_listing_default(),
    };
    let fat_sz = 2usize; // (506+2)*1.5=762B → 2 扇区
    let data_start = 1 + NUM_FATS * fat_sz + 1; // 6（根目录恰 1 扇区）
    let clusters = ((VOL_SECTORS - data_start) / SPC) as u32; // 506
    if clusters >= 4085 {
        return Err(MkfatError::Geometry("FAT12 域外".into()));
    }

    let mut vol = vec![0u8; VOL_SECTORS * SECTOR];
    // —— boot sector（BPB）。
    let mut b = [0u8; SECTOR];
    b[0..3].copy_from_slice(b"\xEB\x3C\x90"); // jmp short + nop
    b[3..11].copy_from_slice(b"BERRYOS "); // OEM 名（8B）
    b[11..13].copy_from_slice(&u16b(SECTOR as u16));
    b[13] = SPC as u8;
    b[14..16].copy_from_slice(&u16b(1)); // 保留扇区
    b[16] = NUM_FATS as u8;
    b[17..19].copy_from_slice(&u16b(ROOT_ENTRIES as u16));
    b[19..21].copy_from_slice(&u16b(VOL_SECTORS as u16)); // total16
    b[21] = 0xF8; // media（硬盘）
    b[22..24].copy_from_slice(&u16b(fat_sz as u16));
    b[24..26].copy_from_slice(&u16b(63)); // 每磁道扇区
    b[26..28].copy_from_slice(&u16b(255)); // 磁头数
    b[28..32].copy_from_slice(&u32b(RESERVED_ZONE as u32)); // hidden
    b[36] = 0x80; // drive number
    b[38] = 0x29; // boot sig
    b[39..43].copy_from_slice(&u32b(VOLUME_ID));
    b[43..54].copy_from_slice(b"BERRYOS    "); // 卷标（11B）
    b[54..62].copy_from_slice(b"FAT12   ");
    b[510..512].copy_from_slice(b"\x55\xAA");
    vol[0..SECTOR].copy_from_slice(&b);

    let mut fat = vec![0u8; fat_sz * SECTOR];
    fat12_set(&mut fat, 0, 0xFF8);
    fat12_set(&mut fat, 1, 0xFFF);
    let mut next_clus: u32 = 2;

    let mut root_entries: Vec<[u8; 32]> = Vec::new();
    let mut placements: Vec<(u32, Vec<u8>)> = Vec::new();

    // 根文件名序（Python sorted(listing)——bytes 序）。
    let mut sorted_listing = owned;
    sorted_listing.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, data) in &sorted_listing {
        let first = alloc_chain12(&mut fat, &mut next_clus, data.len());
        root_entries.push(dir_entry(&name_to_11(name), 0x20, first as u16, data.len() as u32));
        place(&mut placements, first, data, SPC);
    }
    // 逐目录名序（目录簇先于其子文件簇）。
    let mut sorted_dirs: Vec<(&[u8], &[(Vec<u8>, Vec<u8>)])> = dirs.to_vec();
    sorted_dirs.sort_by(|a, b| a.0.cmp(b.0));
    for (dname, children) in &sorted_dirs {
        let dfirst = next_clus; // 目录单簇：先占簇，再排子文件簇
        fat12_set(&mut fat, dfirst as usize, 0xFFF);
        next_clus += 1;
        let mut child_entries: Vec<u8> = Vec::new();
        let mut sorted_children: Vec<&(Vec<u8>, Vec<u8>)> = children.iter().collect();
        sorted_children.sort_by(|a, b| a.0.cmp(&b.0));
        for (cname, cdata) in sorted_children {
            let cfirst = alloc_chain12(&mut fat, &mut next_clus, cdata.len());
            child_entries.extend_from_slice(&dir_entry(
                &name_to_11(cname),
                0x20,
                cfirst as u16,
                cdata.len() as u32,
            ));
            place(&mut placements, cfirst, cdata, SPC);
        }
        root_entries.push(dir_entry(&name_to_11(dname), 0x10, dfirst as u16, 0));
        place(&mut placements, dfirst, &child_entries, SPC); // 尾随 0x00 终结
    }
    for i in 0..NUM_FATS {
        let s = (1 + i * fat_sz) * SECTOR;
        vol[s..s + fat_sz * SECTOR].copy_from_slice(&fat);
    }

    // —— 根目录（16 项 × 32B）：卷标 + 项，余 0x00。
    let mut root = [0u8; SECTOR];
    root[0..32].copy_from_slice(&dir_entry(b"BERRYOS    ", 0x08, 0, 0));
    for (i, e) in root_entries.iter().enumerate() {
        root[(i + 1) * 32..(i + 2) * 32].copy_from_slice(e);
    }
    let rs = data_start * SECTOR - SECTOR;
    vol[rs..rs + SECTOR].copy_from_slice(&root);

    // —— 数据区。
    for (clus, pad) in &placements {
        let off = (data_start + (*clus as usize - 2) * SPC) * SECTOR;
        vol[off..off + pad.len()].copy_from_slice(pad);
    }

    Ok((
        vol,
        Geo {
            clusters,
            fat_sz,
            data_start,
            next_clus,
            spc: SPC,
        },
    ))
}

fn alloc_chain12(fat: &mut [u8], next_clus: &mut u32, nbytes: usize) -> u32 {
    let nclus = max(1, (nbytes + SPC * SECTOR - 1) / (SPC * SECTOR));
    let first = *next_clus;
    for i in 0..nclus {
        fat12_set(fat, (first + i as u32) as usize, (first + i as u32 + 1) as u16);
    }
    fat12_set(fat, (first + nclus as u32 - 1) as usize, 0xFFF); // 链尾 EOC
    *next_clus += nclus as u32;
    first
}

fn place(placements: &mut Vec<(u32, Vec<u8>)>, clus: u32, data: &[u8], spc: usize) {
    let padded_len = clusters_pad(data.len(), spc * SECTOR);
    let mut pad = data.to_vec();
    pad.resize(padded_len, 0);
    placements.push((clus, pad));
}

fn max(a: usize, b: usize) -> usize {
    if a > b { a } else { b }
}

/// FAT 域宽度（build_volume_puts 的按宽分派）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatWidth {
    Fat12,
    Fat16,
}

/// 构建引导卷（--put 面）——Python build_volume_puts 逐字节对齐。
/// puts = [(fat_path, 内容)]；vol_sectors = 卷扇区数；fat16 = 显式宽旗；
/// hidden = BPB 卷前隐藏扇区（默认 0；V-3 mkimage MBR 分区盘 = 分区起始）。
///
/// 与 build_volume 的差异面：任意层嵌套；子目录产 `.`/`..` 首两项；media
/// 0xF0；内容即 puts。簇序 = 深度优先：每层先文件（名序）后子目录（名序），
/// 目录簇先于其子内容簇。单目录单簇。
#[allow(clippy::too_many_lines)]
pub fn build_volume_puts(
    puts: &[(Vec<u8>, Vec<u8>)],
    vol_sectors: usize,
    fat16: bool,
    hidden: u32,
) -> Result<(Vec<u8>, Geo), MkfatError> {
    // puts → 嵌套树（BTreeMap 保序确定性——Python dict 插入序 + sorted 面
    // 由排放期显式排序承担）。
    #[derive(Default)]
    struct Node {
        files: BTreeMap<Vec<u8>, Vec<u8>>,
        dirs: BTreeMap<Vec<u8>, Node>,
    }
    let mut root = Node::default();
    for (path, data) in puts {
        let comps = fat_path_comps(path)?;
        let mut node = &mut root;
        for c in &comps[..comps.len() - 1] {
            if node.files.contains_key(c) {
                return Err(MkfatError::Puts(format!(
                    "{} 中途撞文件 {:?}",
                    String::from_utf8_lossy(path),
                    String::from_utf8_lossy(c)
                )));
            }
            node = node.dirs.entry(c.clone()).or_default();
        }
        let leaf = comps.last().unwrap();
        if node.files.contains_key(leaf) || node.dirs.contains_key(leaf) {
            return Err(MkfatError::Puts(format!(
                "重复路径 {}",
                String::from_utf8_lossy(path)
            )));
        }
        node.files.insert(leaf.clone(), data.clone());
    }

    if vol_sectors == 0 || vol_sectors >= 0x1_0000_0000 {
        return Err(MkfatError::Geometry("total 域外（≥1 扇区、<2^32）".into()));
    }
    // SPC 自适应 + fat_sz 迭代收敛（Python 同款双循环）。
    let mut spc = SPC;
    let (mut fat_sz, mut data_start, mut clusters): (usize, usize, u32);
    loop {
        fat_sz = 2;
        loop {
            data_start = 1 + NUM_FATS * fat_sz + 1;
            clusters = ((vol_sectors - data_start) / spc) as u32;
            let need_sectors = if fat16 {
                ((clusters as usize + 2) * 2 + SECTOR - 1) / SECTOR
            } else {
                ((((clusters as usize + 2) * 3 + 1) / 2) + SECTOR - 1) / SECTOR
            };
            if need_sectors <= fat_sz {
                break;
            }
            fat_sz = need_sectors;
        }
        if fat16 {
            if clusters <= FAT16_MAX_CLUSTERS {
                break;
            }
        } else if clusters < FAT16_MIN_CLUSTERS {
            break;
        }
        spc *= 2;
        if spc > 64 {
            return Err(MkfatError::Puts(format!(
                "{vol_sectors} 扇区超 FAT{} 可表达域（SPC>64）",
                if fat16 { 16 } else { 12 }
            )));
        }
    }
    if fat16 && clusters < FAT16_MIN_CLUSTERS {
        return Err(MkfatError::Geometry(
            "FAT16 声明与簇数域不符（<4085 属 FAT12 域——换 FAT12 或加大卷）".into(),
        ));
    }
    if clusters < 1 {
        return Err(MkfatError::Puts("卷太小：装不下数据区".into()));
    }

    let mut vol = vec![0u8; vol_sectors * SECTOR];
    // —— boot sector（BPB）——与 build_volume 同形，差异：total=全盘 /
    // hidden 参数化 / media 0xF0 / drive 0x00 / 卷标 BERRYBOOT。
    let mut b = [0u8; SECTOR];
    b[0..3].copy_from_slice(b"\xEB\x3C\x90");
    b[3..11].copy_from_slice(b"BERRYOS ");
    b[11..13].copy_from_slice(&u16b(SECTOR as u16));
    b[13] = spc as u8;
    b[14..16].copy_from_slice(&u16b(1));
    b[16] = NUM_FATS as u8;
    b[17..19].copy_from_slice(&u16b(ROOT_ENTRIES as u16));
    if vol_sectors < 0x10000 {
        b[19..21].copy_from_slice(&u16b(vol_sectors as u16)); // total16
    } else {
        b[19..21].copy_from_slice(&u16b(0)); // total16=0 + total32 合成
        b[32..36].copy_from_slice(&u32b(vol_sectors as u32));
    }
    b[21] = 0xF0; // media：可移介质
    b[22..24].copy_from_slice(&u16b(fat_sz as u16));
    b[24..26].copy_from_slice(&u16b(63));
    b[26..28].copy_from_slice(&u16b(255));
    b[28..32].copy_from_slice(&u32b(hidden));
    b[36] = 0x00; // drive number（可移介质惯例）
    b[38] = 0x29;
    b[39..43].copy_from_slice(&u32b(VOLUME_ID));
    b[43..54].copy_from_slice(b"BERRYBOOT  "); // 卷标（11B）
    b[54..62].copy_from_slice(if fat16 { b"FAT16   " } else { b"FAT12   " });
    b[510..512].copy_from_slice(b"\x55\xAA");
    vol[0..SECTOR].copy_from_slice(&b);

    let (eoc, fat_set): (u16, fn(&mut [u8], usize, u16)) =
        if fat16 { (FAT16_EOC, fat16_set) } else { (0xFFF, fat12_set) };
    let mut fat = vec![0u8; fat_sz * SECTOR];
    fat_set(&mut fat, 0, if fat16 { 0xFF00 } else { 0xF00 } | 0xF0);
    fat_set(&mut fat, 1, if fat16 { 0xFFFF } else { 0xFFF });
    let mut next_clus: u32 = 2;
    let mut placements: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut root_entries: Vec<[u8; 32]> = Vec::new();

    fn alloc_chain_wide(
        fat: &mut [u8],
        next_clus: &mut u32,
        nbytes: usize,
        spc: usize,
        fat16: bool,
        eoc: u16,
    ) -> u32 {
        let nclus = max(1, (nbytes + spc * SECTOR - 1) / (spc * SECTOR));
        let first = *next_clus;
        for i in 0..nclus {
            let v = (first + i as u32 + 1) as u16;
            if fat16 {
                fat16_set(fat, (first + i as u32) as usize, v);
            } else {
                fat12_set(fat, (first + i as u32) as usize, v);
            }
        }
        if fat16 {
            fat16_set(fat, (first + nclus as u32 - 1) as usize, eoc);
        } else {
            fat12_set(fat, (first + nclus as u32 - 1) as usize, eoc);
        }
        *next_clus += nclus as u32;
        first
    }

    // emit_dir：分配目录簇并排放内容（` `.`/`..` 首两项 + 文件名序 + 子目录
    // 递归名序——BTreeMap 迭代序即名序，Python sorted 等价）。
    fn emit_dir(
        node: &Node,
        parent_clus: u32,
        fat: &mut [u8],
        next_clus: &mut u32,
        placements: &mut Vec<(u32, Vec<u8>)>,
        spc: usize,
        fat16: bool,
        eoc: u16,
    ) -> Result<u32, MkfatError> {
        let first = *next_clus;
        if fat16 {
            fat16_set(fat, first as usize, eoc);
        } else {
            fat12_set(fat, first as usize, eoc);
        }
        *next_clus += 1;
        let mut entries: Vec<u8> = Vec::new();
        entries.extend_from_slice(&dir_entry(b".          ", 0x10, first as u16, 0));
        entries.extend_from_slice(&dir_entry(b"..         ", 0x10, parent_clus as u16, 0));
        for (name, data) in &node.files {
            let c = alloc_chain_wide(fat, next_clus, data.len(), spc, fat16, eoc);
            entries.extend_from_slice(&dir_entry(&name_to_11(name), 0x20, c as u16, data.len() as u32));
            place(placements, c, data, spc);
        }
        for (name, sub) in &node.dirs {
            let sub_first = emit_dir(sub, first, fat, next_clus, placements, spc, fat16, eoc)?;
            entries.extend_from_slice(&dir_entry(&name_to_11(name), 0x10, sub_first as u16, 0));
        }
        if entries.len() > spc * SECTOR {
            return Err(MkfatError::Puts(format!(
                "目录内容 {}B 超单簇 {}B（v1 单簇目录——多簇目录留后续）",
                entries.len(),
                spc * SECTOR
            )));
        }
        place(placements, first, &entries, spc);
        Ok(first)
    }

    for (name, data) in &root.files {
        let c = alloc_chain_wide(&mut fat, &mut next_clus, data.len(), spc, fat16, eoc);
        root_entries.push(dir_entry(&name_to_11(name), 0x20, c as u16, data.len() as u32));
        place(&mut placements, c, data, spc);
    }
    for (name, sub) in &root.dirs {
        let sub_first = emit_dir(
            sub,
            0,
            &mut fat,
            &mut next_clus,
            &mut placements,
            spc,
            fat16,
            eoc,
        )?;
        root_entries.push(dir_entry(&name_to_11(name), 0x10, sub_first as u16, 0));
    }
    if 1 + root_entries.len() > ROOT_ENTRIES {
        return Err(MkfatError::Puts(format!(
            "根项 {} 超帽 {}",
            1 + root_entries.len(),
            ROOT_ENTRIES
        )));
    }
    if next_clus - 2 > clusters {
        return Err(MkfatError::Puts(format!(
            "簇 {} 超帽 {}",
            next_clus - 2,
            clusters
        )));
    }

    for i in 0..NUM_FATS {
        let s = (1 + i * fat_sz) * SECTOR;
        vol[s..s + fat_sz * SECTOR].copy_from_slice(&fat);
    }
    let mut rootdir = [0u8; SECTOR];
    rootdir[0..32].copy_from_slice(&dir_entry(b"BERRYBOOT  ", 0x08, 0, 0));
    for (i, e) in root_entries.iter().enumerate() {
        rootdir[(i + 1) * 32..(i + 2) * 32].copy_from_slice(e);
    }
    let rs = (data_start - 1) * SECTOR;
    vol[rs..rs + SECTOR].copy_from_slice(&rootdir);
    for (clus, pad) in &placements {
        let off = (data_start + (*clus as usize - 2) * spc) * SECTOR;
        vol[off..off + pad.len()].copy_from_slice(pad);
    }

    Ok((
        vol,
        Geo {
            clusters,
            fat_sz,
            data_start,
            next_clus,
            spc,
        },
    ))
}

/// 默认整盘（保留区前缀 + 卷）——Python main 裸调路径（mkfat <out.img>）。
pub fn build_disk() -> Result<(Vec<u8>, Geo), MkfatError> {
    let (vol, geo) = build_volume(None, &[])?;
    let mut disk = vec![0u8; DISK_SECTORS * SECTOR];
    disk[RESERVED_ZONE * SECTOR..].copy_from_slice(&vol);
    Ok((disk, geo))
}
