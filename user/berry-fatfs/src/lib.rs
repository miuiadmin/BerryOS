//! berry-fatfs —— FAT12/FAT16 只读解析纯函数库（P6 A-5，`berry-loader`/
//! `berry-fsproto`/`berry-blkproto` 先例「解析器即产品」第 4 例：零分配、
//! 零 unsafe、零依赖，输入只借用）。
//!
//! 职责边界：本库只做**字节 ↔ 结构**的无状态编解码（BPB 几何 / FAT12
//! 紧包与 FAT16 定宽表项 / 根目录项 / 8.3 名转换 + A-6 写基元
//! `fat12_set`/`fat16_set`/`find_free_cluster`/`encode_file_entry` + C-2
//! 刀 2 `count_free_clusters`（df/STATFS 空闲真值））；扇区
//! 读写（vblk 经 `berry-blkproto` RPC）、簇链缓存、staging/commit 编排全
//! 在消费方 fsmgr（`user/bins/fsmgr/src/fat.rs`）。
//!
//! 卷几何约定：本库一切扇区号**卷相对**（boot 扇区 = 0）；盘绝对 LBA 由
//! 消费方加 BPB 的 `hidden`（卷前缀扇区数）折算——mkfat 盘 = 16 扇区
//! vblk 测试保留区 + 卷 @ LBA16，解析器不硬编码。
//!
//! 合法域执法（V-2 起双域 FAT12+FAT16，原文纪律保留）：
//! - bps ∈ {512, 1024, 2048, 4096}（规范幂二档；**消费方 fsmgr 另断言
//!   == 512**——vblk 服务单扇区粒度，非 512 盘的簇读取会跨扇区错位）；
//! - spc 幂二且 ≥ 1、reserved ≥ 1、nfats ≥ 1、根目录 32B 项填整扇区；
//! - **簇数 = 版本域单一事实源**（BPB :54 fstype 串不读——串是建议性
//!   提示，判宽只看簇数）：簇数 < 4085 → Fat12、4085..=65524 → Fat16、
//!   ≥ 65525 → None（FAT32 地界拒——不静默按错宽解）。
//!
//! 8.3 名命名空间：[`name_to_fat11`] 只收 **[A-Z0-9]**（小写/连字符/空格
//! 全拒）——RAM 层名字（t64.bin、h.hex 等小写）按构造永不落进 FAT 名空间
//! = 双层名字无碰撞面；FAT 层名字（mkfat 造的 HELLO.TXT 等）恒大写。这是
//! P6 A-5 覆盖层（overlay）语义的地基：同一名字按写法天然只命中一层。
//!
//! host 测试对 `tests/volume_fixture.bin`（`tools/mkfat.py --fixture`
//! 产物，确定性字节进 git）做跨实现共识——python 造盘与 Rust 解析互不借
//! 码、双绿即独立收敛；几何/链/内容三面全钉。V-2 起双宽双基准：
//! `volume_fixture_fat16.bin`（`--fixture16` 产物，4087 簇 FAT16 域）仿
//! 同名模式钉 FAT16 面。**改 mkfat 须重产 fixture**（`mkfat.py x.img
//! --fixture tests/volume_fixture.bin` / `--fixture16 …
//! volume_fixture_fat16.bin`）——`just test-host` 前置 `--check-fixture`
//! 双行重产比对守卫，基准漂移即 FAIL（防共识测试对陈旧基准假绿，A-5
//! 对抗审查实抓）。

#![cfg_attr(not(test), no_std)]

/// boot 扇区长（bps 最小档；本库输入的最小切片长度）。
pub const SECTOR: usize = 512;
/// FAT12/FAT16 簇数上限（≥ 此值为 FAT16 地界）。
pub const FAT12_MAX_CLUSTERS: u32 = 4085;
/// EOC 起始值（0xFF8..=0xFFF 皆链尾）。
pub const FAT12_EOC_MIN: u32 = 0xFF8;
/// 坏簇标记值。
pub const FAT12_BAD: u32 = 0xFF7;
/// 本库写侧恒用的链尾标记（0xFF8..=0xFFF 皆合法 EOC；写读跨实现共识——
/// mkfat `fat12_set(fat, …, 0xFFF)` 同值钉死）。
pub const FAT12_EOC: u32 = 0xFFF;
/// FAT16 簇数上限（≥ 此值为 FAT32 地界 → parse_bpb 拒）。
pub const FAT16_MAX_CLUSTERS: u32 = 65524;
/// FAT16 EOC 起始值（0xFFF8..=0xFFFF 皆链尾）。
pub const FAT16_EOC_MIN: u32 = 0xFFF8;
/// FAT16 坏簇标记值。
pub const FAT16_BAD: u32 = 0xFFF7;
/// FAT16 写侧恒用的链尾标记（0xFFF8..=0xFFFF 皆合法 EOC；写读跨实现
/// 共识——mkfat `fat16_set(fat, …, 0xFFFF)` 同值钉死）。
pub const FAT16_EOC: u32 = 0xFFFF;
/// 文件项日期字段定值（2020-01-01 = (40<<9)|(1<<5)|1，mkfat DATE 同值——
/// 确定性纪律：无时刻输入，归档字节只随内容变）。
const FAT_ENTRY_DATE: u16 = 0x5021;

/// 卷几何（[`parse_bpb`] 产物；一切扇区号卷相对）。
#[derive(Debug, PartialEq, Eq)]
pub struct Bpb {
    /// 每扇区字节。
    pub bps: u16,
    /// 每簇扇区。
    pub spc: u8,
    /// 保留扇区数（首个 FAT 前）。
    pub reserved: u16,
    /// FAT 份数。
    pub nfats: u8,
    /// 根目录项数（32B 每项）。
    pub root_entries: u16,
    /// 卷总扇区（16/32 位字段合成）。
    pub total: u32,
    /// 每 FAT 扇区数。
    pub fat_sz: u32,
    /// 卷前缀扇区数（盘绝对 LBA = 卷相对 + 此值；mkfat = 16）。
    pub hidden: u32,
    /// 首个 FAT 起始扇区（= reserved）。
    pub fat_sector: u32,
    /// 根目录起始扇区。
    pub root_sector: u32,
    /// 根目录占用扇区数。
    pub root_secs: u32,
    /// 数据区起始扇区（簇 2 起点）。
    pub data_sector: u32,
    /// 数据区簇数。
    pub cluster_count: u32,
    /// FAT 位宽（簇数分派：< 4085 = Fat12 / 4085..=65524 = Fat16——版本
    /// 域单一事实源，BPB :54 fstype 串不读）。
    pub fat_bits: FatBits,
}

/// 目录文件项（32B 项的可消费面；名字保持 8.3 原始 11B 形态）。
#[derive(Debug, PartialEq, Eq)]
pub struct DirFile {
    /// 8.3 名（"NAME    EXT" 填空格形态——比较用 [`name_to_fat11`] 折算）。
    pub name: [u8; 11],
    /// 首簇号。
    pub first_clus: u32,
    /// 文件字节数（簇尾零填不在此数内）。
    pub size: u32,
}

/// 根目录项分类（读面：目录是真项——簇链可走查（C-0 刀 3）；LFN/卷标/
/// 删除项 Skip）。
#[derive(Debug, PartialEq, Eq)]
pub enum DirKind {
    /// 0x00 起始——目录终结（其后全无效，遍历即停）。
    End,
    /// 跳过项（0xE5 删除 / LFN 0x0F / 卷标 0x08）。
    Skip,
    /// 子目录（attr 0x10）：`first_clus` = 子项表簇链头（走查消费方 fsmgr）；
    /// `size` 恒 0（规范面——非 0 解码即拒）；first_clus 0 = 空目录。
    Dir(DirFile),
    /// 普通文件（attr 0x20 且非删除）。
    File(DirFile),
}

/// FAT 位宽（V-2）：由 [`parse_bpb`] 按簇数分派（< 4085 = Fat12、
/// 4085..=65524 = Fat16），一切按宽分派的基元（`find_free_cluster` /
/// `encode_file_entry`）以它选臂——消费方不经手拼。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatBits {
    /// 12 位紧包表项（FAT12 域）。
    Fat12,
    /// 16 位定宽表项（FAT16 域）。
    Fat16,
}

/// FAT 表项分类（12/16 位同构——Free/Next/Eoc/Bad 四态值域各宽自带）。
#[derive(Debug, PartialEq, Eq)]
pub enum FatEntry {
    /// 0x000——空簇。
    Free,
    /// 链下一簇（Fat12: 2..=0xFF6 / Fat16: 2..=0xFFF6）。
    Next(u32),
    /// 链尾（Fat12: 0xFF8..=0xFFF / Fat16: 0xFFF8..=0xFFFF）。
    Eoc,
    /// 坏簇（Fat12: 0xFF7 / Fat16: 0xFFF7）。
    Bad,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn is_pow2(v: u32) -> bool {
    v != 0 && v & (v - 1) == 0
}

/// 解析 boot 扇区 → 卷几何（坏签名 / 非法 BPB / FAT32 地界（簇数 ≥
/// 65525）→ None；消费方另断言 bps==512——见模块头注）。
pub fn parse_bpb(boot: &[u8]) -> Option<Bpb> {
    if boot.len() < SECTOR {
        return None;
    }
    if u16le(boot, 510) != 0xAA55 {
        return None; // 签名字节 55 AA——LE u16 读出即 0xAA55（勿按字面写 0x55AA）
    }
    let bps = u16le(boot, 11);
    if !matches!(bps, 512 | 1024 | 2048 | 4096) {
        return None; // 规范外（非幂二档拒——静默按其它值解会全盘错位）
    }
    let spc = boot[13] as u32;
    if !is_pow2(spc) {
        return None;
    }
    let reserved = u16le(boot, 14) as u32;
    if reserved == 0 {
        return None;
    }
    let nfats = boot[16] as u32;
    if nfats == 0 {
        return None;
    }
    let root_entries = u16le(boot, 17) as u32;
    let root_bytes = root_entries * 32;
    if root_bytes % bps as u32 != 0 {
        return None; // 根目录不填整扇区（规范外；跨扇区遍历会错位）
    }
    let t16 = u16le(boot, 19) as u32;
    let t32 = u32le(boot, 32);
    let total = match (t16, t32) {
        (0, 0) => return None,
        (0, t) => t,
        (t, _) => t, // 16 位在场优先（32 位为 0 或冗余——规范约定）
    };
    let fat_sz = u16le(boot, 22) as u32;
    if fat_sz == 0 {
        return None; // FAT12/16 无 32 位 FAT 尺寸字段，0 = 畸形
    }
    let fat_sector = reserved;
    let root_sector = fat_sector + nfats * fat_sz;
    let root_secs = root_bytes / bps as u32;
    let data_sector = root_sector + root_secs;
    if total <= data_sector {
        return None; // 无数据区
    }
    let cluster_count = (total - data_sector) / spc;
    // 版本域三分派（V-2）：簇数是 FAT 宽度的单一事实源（fstype 串不读）；
    // ≥ 65525 = FAT32 地界 → None（不静默按错宽解）。
    let fat_bits = if cluster_count < FAT12_MAX_CLUSTERS {
        FatBits::Fat12
    } else if cluster_count <= FAT16_MAX_CLUSTERS {
        FatBits::Fat16
    } else {
        return None; // FAT32 地界
    };
    Some(Bpb {
        bps,
        spc: spc as u8,
        reserved: reserved as u16,
        nfats: nfats as u8,
        root_entries: root_entries as u16,
        total,
        fat_sz,
        hidden: u32le(boot, 28),
        fat_sector,
        root_sector,
        root_secs,
        data_sector,
        cluster_count,
        fat_bits,
    })
}

/// 读 FAT12 表项（`fat` = 单份 FAT 切片；`clus` ≥ 2；切片不够长 → None）。
pub fn fat12_entry(fat: &[u8], clus: u32) -> Option<FatEntry> {
    if clus < 2 {
        return None; // 0/1 保留项——不是文件簇
    }
    let off = (clus as usize * 3) / 2;
    if fat.len() < off + 2 {
        return None;
    }
    let v = if clus % 2 == 0 {
        fat[off] as u32 | ((fat[off + 1] as u32 & 0x0F) << 8)
    } else {
        (fat[off] as u32 >> 4) | ((fat[off + 1] as u32) << 4)
    };
    match v {
        0 => Some(FatEntry::Free),
        FAT12_BAD => Some(FatEntry::Bad),
        FAT12_EOC_MIN..=0xFFF => Some(FatEntry::Eoc),
        2..=0xFF6 => Some(FatEntry::Next(v)),
        _ => None, // 0/1 出现在文件簇位 = 畸形（保留值非链指针）
    }
}

/// 写 FAT12 表项（[`fat12_entry`] 的写侧镜像——12 位紧包逆运算；簇 < 2 /
/// 值 > 0xFFF / 切片不够长 → None，None 拒绝契约：写不出就拒绝）。mkfat
/// `fat12_set` 同式——python/Rust 双实现写读共识。
pub fn fat12_set(fat: &mut [u8], clus: u32, v: u32) -> Option<()> {
    if clus < 2 || v > 0xFFF {
        return None;
    }
    let off = (clus as usize * 3) / 2;
    if fat.len() < off + 2 {
        return None;
    }
    if clus % 2 == 0 {
        fat[off] = (v & 0xFF) as u8;
        fat[off + 1] = (fat[off + 1] & 0xF0) | ((v >> 8) as u8 & 0x0F);
    } else {
        fat[off] = (fat[off] & 0x0F) | (((v & 0x0F) as u8) << 4);
        fat[off + 1] = (v >> 4) as u8;
    }
    Some(())
}

/// 读 FAT16 表项（`fat12_entry` 的宽域对偶——u16le@2n 定宽，无奇偶紧包；
/// `clus` ≥ 2；切片不够长 → None）。
pub fn fat16_entry(fat: &[u8], clus: u32) -> Option<FatEntry> {
    if clus < 2 {
        return None; // 0/1 保留项——不是文件簇
    }
    let off = clus as usize * 2;
    if fat.len() < off + 2 {
        return None;
    }
    let v = u16le(fat, off) as u32;
    match v {
        0 => Some(FatEntry::Free),
        FAT16_BAD => Some(FatEntry::Bad),
        FAT16_EOC_MIN..=0xFFFF => Some(FatEntry::Eoc),
        2..=0xFFF6 => Some(FatEntry::Next(v)),
        _ => None, // 1 出现在文件簇位 = 畸形（保留值非链指针）
    }
}

/// 写 FAT16 表项（`fat16_entry` 的写侧镜像 / [`fat12_set`] 的宽域对偶——
/// 定宽全字节覆盖无邻位半 nibble；簇 < 2 / 值 > 0xFFFF / 切片不够长 →
/// None，None 拒绝契约同款）。mkfat `fat16_set` 同式——python/Rust 双
/// 实现写读共识。
pub fn fat16_set(fat: &mut [u8], clus: u32, v: u32) -> Option<()> {
    if clus < 2 || v > 0xFFFF {
        return None;
    }
    let off = clus as usize * 2;
    if fat.len() < off + 2 {
        return None;
    }
    fat[off..off + 2].copy_from_slice(&(v as u16).to_le_bytes());
    Some(())
}

/// 从 `from` 起找首个空簇（Free）；`from < 2` 钳到 2（保留项不是分配
/// 域），扫描界 = `2 + cluster_count`（开区间）。None = 域内无空簇（调用
/// 方诚实拒—— fsmgr 侧 ST_FULL）。`bits` = 按宽选 entry 解码（V-2——
/// 12 位紧包流按 16 位解（反之亦然）是错位读，构造性不跨宽）。
pub fn find_free_cluster(
    fat: &[u8],
    from: u32,
    cluster_count: u32,
    bits: FatBits,
) -> Option<u32> {
    let max = 2 + cluster_count;
    let mut c = if from < 2 { 2 } else { from };
    while c < max {
        let free = match bits {
            FatBits::Fat12 => matches!(fat12_entry(fat, c), Some(FatEntry::Free)),
            FatBits::Fat16 => matches!(fat16_entry(fat, c), Some(FatEntry::Free)),
        };
        if free {
            return Some(c);
        }
        c += 1;
    }
    None
}

/// 计 FAT 域 2..2+cluster_count 内 Free 项数（C-2 刀 2：df/STATFS 的卷级
/// 空闲真值——[`find_free_cluster`] 的计数版）。任一项解码失败（切片短 /
/// 保留值落文件簇位 = 畸形）→ None 诚实拒：空闲计数是上报真值，不静默
/// 少计（alloc 扫描 [`find_free_cluster`] 跳过畸形项继续找——找得到首个
/// 空簇即够用，两面语义按消费方分立）。
pub fn count_free_clusters(fat: &[u8], cluster_count: u32, bits: FatBits) -> Option<u32> {
    let mut n = 0u32;
    for c in 2..2 + cluster_count {
        let e = match bits {
            FatBits::Fat12 => fat12_entry(fat, c)?,
            FatBits::Fat16 => fat16_entry(fat, c)?,
        };
        if matches!(e, FatEntry::Free) {
            n += 1;
        }
    }
    Some(n)
}

/// 解析 32B 根目录项（长度 != 32 → None）。子目录（attr 0x10）与文件同
/// 构解码（C-0 刀 3：DIR 属性项即目录文件——簇链头的载体）；目录的
/// size 域按规范恒 0、簇 1 保留同拒。
pub fn dir_entry(e: &[u8]) -> Option<DirKind> {
    if e.len() != 32 {
        return None;
    }
    match e[0] {
        0x00 => return Some(DirKind::End),
        0xE5 => return Some(DirKind::Skip), // 删除项
        _ => {}
    }
    let decode = |kind: fn(DirFile) -> DirKind| -> Option<DirKind> {
        let mut name = [0u8; 11];
        name.copy_from_slice(&e[0..11]);
        let first = u16le(e, 26) as u32;
        let size = u32le(e, 28);
        if first == 1 || (first == 0 && size > 0) {
            return None; // 簇 1 保留；首簇 0 仅配 size=0（空文件/空目录——
        }              // A-6 `>` 建空文件面），带长度即畸形
        Some(kind(DirFile { name, first_clus: first, size }))
    };
    match e[11] {
        0x0F => Some(DirKind::Skip), // LFN 长名段
        0x08 => Some(DirKind::Skip), // 卷标
        0x10 => {
            if u32le(e, 28) != 0 {
                return None; // 目录项 size 域规范恒 0（两端同约定——mkfat
            }              // 造 0，非 0 = 畸形拒绝不静默按文件面目走查）
            decode(DirKind::Dir)
        }
        _ => decode(DirKind::File),
    }
}

/// 编码 32B 根目录文件项（A-6 写基元）：attr = 0x20 ARCHIVE、日期定值
/// `FAT_ENTRY_DATE`、时间戳零——与 mkfat `dir_entry` **逐字节同形**（写读
/// 跨实现共识的可对拍面）。`first_clus` 1 / 标记值域（`bits` 分派：Fat12
/// 拒 > 0xFF6、Fat16 拒 > 0xFFF6）/ 0 配 size>0 → None——与 [`dir_entry`]
/// 拒面互为镜像；输出可被 [`dir_entry`] 原样认回。
pub fn encode_file_entry(
    name11: &[u8; 11],
    first_clus: u32,
    size: u32,
    out: &mut [u8; 32],
    bits: FatBits,
) -> Option<()> {
    let clus_max = match bits {
        FatBits::Fat12 => 0xFF6,
        FatBits::Fat16 => 0xFFF6,
    };
    if first_clus == 1 || first_clus > clus_max || (first_clus == 0 && size > 0) {
        return None;
    }
    *out = [0u8; 32];
    out[..11].copy_from_slice(name11);
    out[11] = 0x20; // ATTR_ARCHIVE
    out[24..26].copy_from_slice(&FAT_ENTRY_DATE.to_le_bytes());
    out[26..28].copy_from_slice(&(first_clus as u16).to_le_bytes());
    out[28..32].copy_from_slice(&size.to_le_bytes());
    Some(())
}

/// 8.3 名（"NAME    EXT"）→ 显示名（"NAME.EXT"，去基名/扩展名尾随空格；
/// 无扩展名则无点）。out 太短 → None（None 拒绝契约：编不出就拒绝）。
pub fn fat11_display(name11: &[u8; 11], out: &mut [u8]) -> Option<usize> {
    let base_end = 8 - name11[0..8].iter().rev().take_while(|&&b| b == b' ').count();
    let ext_end = 11 - name11[8..11].iter().rev().take_while(|&&b| b == b' ').count();
    let need = base_end + if ext_end > 8 { 1 + (ext_end - 8) } else { 0 };
    if out.len() < need {
        return None;
    }
    let mut n = 0;
    out[..base_end].copy_from_slice(&name11[..base_end]);
    n += base_end;
    if ext_end > 8 {
        out[n] = b'.';
        n += 1;
        out[n..n + (ext_end - 8)].copy_from_slice(&name11[8..ext_end]);
        n += ext_end - 8;
    }
    Some(n)
}

/// 显示名 → 8.3 名（只收 **[A-Z0-9]**：小写/其它字符全拒——命名空间按
/// 构造隔离 RAM 层小写名；基名 ≤ 8、扩展名 ≤ 3、至多一个点、非空基名）。
pub fn name_to_fat11(name: &[u8]) -> Option<[u8; 11]> {
    let (base, ext) = match name.iter().position(|&b| b == b'.') {
        Some(i) => (&name[..i], &name[i + 1..]),
        None => (name, &[][..]),
    };
    if base.is_empty() || base.len() > 8 || ext.len() > 3 {
        return None;
    }
    if name.iter().filter(|&&b| b == b'.').count() > 1 {
        return None;
    }
    let ok = |b: u8| b.is_ascii_uppercase() || b.is_ascii_digit();
    if !base.iter().all(|&b| ok(b)) || !ext.iter().all(|&b| ok(b)) {
        return None;
    }
    let mut n11 = [b' '; 11];
    n11[..base.len()].copy_from_slice(base);
    n11[8..8 + ext.len()].copy_from_slice(ext);
    Some(n11)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// mkfat.py --fixture 产物（确定性字节进 git）：卷前缀 30 扇区 =
    /// boot(1) + 双 FAT(4) + 根目录(1) + 簇 2..7 数据（文件 + fixture
    /// 专列子目录 DATA——C-0 刀 3 共识基准，默认盘不含）。
    const FX: &[u8] = include_bytes!("../tests/volume_fixture.bin");

    /// mkfat.py --fixture16 产物（V-2 FAT16 共识基准，确定性字节进 git）：
    /// 16384 扇区卷的确定性前缀 58 扇区 = boot(1) + 双 FAT(16×2=32) +
    /// 根目录(1) + 簇 2..7 数据（6 簇 ×4 扇区）；簇数 4087 恰贴 FAT12/16
    /// 分界。子目录构造体与 FAT12 侧不同形（build_volume_puts 产 `./..`
    /// 首两项——EDK2 FatDxe 友好），两端约定见 mkfat.py 头注。
    const FX16: &[u8] = include_bytes!("../tests/volume_fixture_fat16.bin");

    fn fat0() -> &'static [u8] {
        &FX[512..512 + 1024] // FAT0 @ 卷扇区 1
    }

    fn root() -> &'static [u8] {
        &FX[5 * 512..6 * 512] // 根目录 @ 卷扇区 5
    }

    /// FX16 的 FAT0 切片（字面偏移与 parse 派生值互检——两源钉板防漂移）。
    fn fat0_16() -> &'static [u8] {
        let g = parse_bpb(&FX16[..512]).unwrap();
        assert_eq!((g.fat_sector, g.fat_sz), (1, 16));
        &FX16[512..17 * 512] // FAT0 @ 卷扇区 1..17
    }

    /// FX16 的根目录切片（同上互检）。
    fn root_16() -> &'static [u8] {
        let g = parse_bpb(&FX16[..512]).unwrap();
        assert_eq!((g.root_sector, g.root_secs), (33, 1));
        &FX16[33 * 512..34 * 512] // 根目录 @ 卷扇区 33
    }

    /// mkfat `fat12_set` 同式写侧（**独立于** [`fat12_set`] 的钉板实现——
    /// 双绿即跨实现写侧共识；被 `fat12_set_consensus_and_bounds` 对拍、
    /// `fat12_parity_and_exact_fit`/`find_free_cluster_scan` 复用）。
    fn pack(fat: &mut [u8], n: u32, v: u32) {
        let off = (n as usize * 3) / 2;
        if n % 2 == 0 {
            fat[off] = (v & 0xFF) as u8;
            fat[off + 1] = (fat[off + 1] & 0xF0) | ((v >> 8) as u8 & 0x0F);
        } else {
            fat[off] = (fat[off] & 0x0F) | (((v & 0x0F) as u8) << 4);
            fat[off + 1] = (v >> 4) as u8;
        }
    }

    /// mkfat `fat16_set` 同式写侧（`pack` 的宽域对偶——独立于 [`fat16_set`]
    /// 的 u16le 定宽钉板，双绿即跨实现写侧共识）。
    fn pack16(fat: &mut [u8], n: u32, v: u32) {
        let off = n as usize * 2;
        fat[off..off + 2].copy_from_slice(&(v as u16).to_le_bytes());
    }

    #[test]
    fn bpb_fixture_consensus() {
        // 跨实现共识①：mkfat 造的 BPB 被 Rust 解析器逐字段认回。
        let g = parse_bpb(&FX[..512]).unwrap();
        assert_eq!(g.bps, 512);
        assert_eq!(g.spc, 4);
        assert_eq!(g.reserved, 1);
        assert_eq!(g.nfats, 2);
        assert_eq!(g.root_entries, 16);
        assert_eq!(g.total, 2032);
        assert_eq!(g.fat_sz, 2);
        assert_eq!(g.hidden, 16);
        assert_eq!(g.fat_sector, 1);
        assert_eq!(g.root_sector, 5);
        assert_eq!(g.root_secs, 1);
        assert_eq!(g.data_sector, 6);
        assert_eq!(g.cluster_count, 506);
    }

    #[test]
    fn fat_chain_fixture_consensus() {
        // 跨实现共识②：HELLO.TXT 单簇（2, EOC）；PATTERN.BIN 链 3→4→5；
        // fixture 专列 DATA 目录单簇 6、其子 NOTE.TXT 单簇 7（C-0 刀 3）。
        let fat = fat0();
        assert_eq!(fat12_entry(fat, 2), Some(FatEntry::Eoc));
        assert_eq!(fat12_entry(fat, 3), Some(FatEntry::Next(4)));
        assert_eq!(fat12_entry(fat, 4), Some(FatEntry::Next(5)));
        assert_eq!(fat12_entry(fat, 5), Some(FatEntry::Eoc));
        assert_eq!(fat12_entry(fat, 6), Some(FatEntry::Eoc));
        assert_eq!(fat12_entry(fat, 7), Some(FatEntry::Eoc));
        assert_eq!(fat12_entry(fat, 8), Some(FatEntry::Free));
    }

    #[test]
    fn dir_fixture_consensus() {
        // 跨实现共识③：卷标 Skip + 两文件项 + 目录项（fixture 专列）+ End。
        let r = root();
        assert_eq!(dir_entry(&r[0..32]), Some(DirKind::Skip)); // BERRYOS 卷标
        match dir_entry(&r[32..64]).unwrap() {
            DirKind::File(f) => {
                assert_eq!(&f.name, b"HELLO   TXT");
                assert_eq!((f.first_clus, f.size), (2, 28));
            }
            other => panic!("项 1 应为文件：{:?}", other),
        }
        match dir_entry(&r[64..96]).unwrap() {
            DirKind::File(f) => {
                assert_eq!(&f.name, b"PATTERN BIN");
                assert_eq!((f.first_clus, f.size), (3, 5000));
            }
            other => panic!("项 2 应为文件：{:?}", other),
        }
        match dir_entry(&r[96..128]).unwrap() {
            DirKind::Dir(d) => {
                assert_eq!(&d.name, b"DATA       ");
                assert_eq!((d.first_clus, d.size), (6, 0));
            }
            other => panic!("项 3 应为目录：{:?}", other),
        }
        assert_eq!(dir_entry(&r[128..160]), Some(DirKind::End));
    }

    #[test]
    fn subdir_chain_consensus() {
        // 跨实现共识⑤（C-0 刀 3）：子目录链走查——根 DATA@6 → 目录簇 6
        // = 子项表（NOTE.TXT@7 + End 终结 + 尾零）→ 簇 7 = 子文件字节
        // （簇尾零填）。fsmgr resolve 同几何消费，此处先钉。
        let g = parse_bpb(&FX[..512]).unwrap();
        let clus_off = |c: u32| -> usize {
            (g.data_sector as usize + (c as usize - 2) * g.spc as usize) * g.bps as usize
        };
        let d = &FX[clus_off(6)..];
        match dir_entry(&d[0..32]).unwrap() {
            DirKind::File(f) => {
                assert_eq!(&f.name, b"NOTE    TXT");
                assert_eq!((f.first_clus, f.size), (7, 24));
            }
            other => panic!("目录簇首项应为 NOTE.TXT：{:?}", other),
        }
        assert_eq!(dir_entry(&d[32..64]), Some(DirKind::End));
        assert!(d[64..2048].iter().all(|&b| b == 0)); // 终结后整簇余零
        let n = clus_off(7);
        assert_eq!(&FX[n..n + 24], b"berryos fat subdir note\n");
        assert!(FX[n + 24..n + 2048].iter().all(|&b| b == 0)); // 簇尾零填
    }

    #[test]
    fn file_bytes_fixture_consensus() {
        // 跨实现共识④：几何折算（data_sector + (clus-2)*spc）下文件字节
        // 逐位对上——fsmgr 同式消费，此处先钉。
        let g = parse_bpb(&FX[..512]).unwrap();
        let clus_off = |c: u32| -> usize {
            (g.data_sector as usize + (c as usize - 2) * g.spc as usize) * g.bps as usize
        };
        let hello = &FX[clus_off(2)..clus_off(2) + 28];
        assert_eq!(hello, b"berryos fat read-only hello\n");
        // PATTERN.BIN 跨 3 簇（5000B > 2 簇 4096B——链尾簇偏移越过簇界）。
        let mut expect = [0u8; 5000];
        for (i, b) in expect.iter_mut().enumerate() {
            *b = ((i * 31 + 7) % 256) as u8;
        }
        let c3 = clus_off(3);
        assert_eq!(&FX[c3..c3 + 5000], &expect);
        // 簇尾零填（size 之外到簇界）。
        assert!(FX[c3 + 5000..c3 + 3 * 2048].iter().all(|&b| b == 0));
    }

    #[test]
    fn bpb_rejects_malformed() {
        // 局部小工具（切片参数——各 bad 副本共用）。
        fn u16at(b: &mut [u8], o: usize, v: u16) {
            b[o..o + 2].copy_from_slice(&v.to_le_bytes());
        }
        fn u32at(b: &mut [u8], o: usize, v: u32) {
            b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        let mut b = [0u8; 512];
        b[510] = 0x55;
        b[511] = 0xAA;
        // 合法基线（此后逐字段改坏）。
        u16at(&mut b, 11, 512);
        b[13] = 4;
        u16at(&mut b, 14, 1);
        b[16] = 2;
        u16at(&mut b, 17, 16);
        u16at(&mut b, 19, 2032);
        u16at(&mut b, 22, 2);
        u32at(&mut b, 28, 16);
        // 合法基线（此后逐字段改坏）：506 簇 = FAT12 域。
        assert_eq!(parse_bpb(&b).map(|g| g.fat_bits), Some(FatBits::Fat12));
        // 逐字段坏面：签名 / bps 档 / spc 幂二 / reserved / nfats / 根整除 /
        // 双 total 零 / fat_sz 零 / 无数据区 / FAT16 地界。
        let mut bad = b;
        bad[511] = 0x00;
        assert_eq!(parse_bpb(&bad), None);
        let mut bad = b;
        u16at(&mut bad, 11, 513);
        assert_eq!(parse_bpb(&bad), None);
        let mut bad = b;
        u16at(&mut bad, 11, 256);
        assert_eq!(parse_bpb(&bad), None);
        let mut bad = b;
        bad[13] = 3;
        assert_eq!(parse_bpb(&bad), None);
        let mut bad = b;
        u16at(&mut bad, 14, 0);
        assert_eq!(parse_bpb(&bad), None);
        let mut bad = b;
        bad[16] = 0;
        assert_eq!(parse_bpb(&bad), None);
        let mut bad = b;
        u16at(&mut bad, 17, 15); // 15*32=480 不填整 512 扇区
        assert_eq!(parse_bpb(&bad), None);
        let mut bad = b;
        u16at(&mut bad, 19, 0);
        u32at(&mut bad, 32, 0);
        assert_eq!(parse_bpb(&bad), None);
        let mut bad = b;
        u16at(&mut bad, 22, 0);
        assert_eq!(parse_bpb(&bad), None);
        let mut bad = b;
        u16at(&mut bad, 19, 6); // total == data_sector（零数据区）
        assert_eq!(parse_bpb(&bad), None);
        // 簇数分派界（V-2 语义翻转）：4085 起落 FAT16 域（原「FAT16 地界
        // 拒」上移）；拒面 = ≥ 65525（FAT32 地界）。
        let mut wide = b;
        u16at(&mut wide, 19, 16346); // 簇数 (16346-6)/4 = 4085 → Fat16（分界含）
        let g = parse_bpb(&wide).unwrap();
        assert_eq!(g.fat_bits, FatBits::Fat16);
        assert_eq!(g.cluster_count, 4085);
        let mut bad = b;
        u16at(&mut bad, 19, 0);
        u32at(&mut bad, 32, 65536 * 4); // 簇数 (262144-6)/4 = 65534 ≥ 65525 → FAT32 地界
        assert_eq!(parse_bpb(&bad), None);
        // 短输入。
        assert_eq!(parse_bpb(&b[..511]), None);
    }

    #[test]
    fn bpb_total32_path() {
        // total16=0 + total32 在场——32 位合成路径（合法档）。
        let mut b = FX[..512].to_vec();
        b[19] = 0;
        b[20] = 0;
        b[32..36].copy_from_slice(&2032u32.to_le_bytes());
        let g = parse_bpb(&b).unwrap();
        assert_eq!(g.total, 2032);
    }

    #[test]
    fn fat12_parity_and_exact_fit() {
        // 奇偶打包钉板：pack = mkfat `fat12_set` 同式写侧（写读两侧独立实现
        // 共识）；解析按 偶 = b0|(b1&0xF)<<8 / 奇 = b0>>4|b1<<4 折开。
        let mut fat = [0u8; 12]; // 项 7 @ off 10 触及 byte 11——恰合切片用
        for (n, v) in [(2u32, 0xABCu32), (3, 0x123), (4, 5), (5, 0xFF8), (7, 0xFF7)] {
            pack(&mut fat, n, v);
        }
        assert_eq!(fat12_entry(&fat, 2), Some(FatEntry::Next(0xABC)));
        assert_eq!(fat12_entry(&fat, 3), Some(FatEntry::Next(0x123)));
        assert_eq!(fat12_entry(&fat, 4), Some(FatEntry::Next(5)));
        assert_eq!(fat12_entry(&fat, 5), Some(FatEntry::Eoc));
        assert_eq!(fat12_entry(&fat, 7), Some(FatEntry::Bad));
        // 恰合切片（exact fit）：项 7 偶 @ off 10 需 12 字节——11 字节 None。
        assert_eq!(fat12_entry(&fat[..11], 7), None);
        assert_eq!(fat12_entry(&fat[..12], 7), Some(FatEntry::Bad));
        // 保留簇号 0/1 与保留值 0/1 落文件位。
        assert_eq!(fat12_entry(&fat, 0), None);
        assert_eq!(fat12_entry(&fat, 1), None);
        let mut f2 = [0u8; 5]; // 项 2 @ off 3 触及 byte 4
        pack(&mut f2, 2, 1); // 文件簇位存保留值 1 → 畸形
        assert_eq!(fat12_entry(&f2, 2), None);
    }

    #[test]
    fn dir_kinds() {
        let mk = |first: u8, attr: u8| {
            let mut e = [0u8; 32];
            e[0] = first;
            e[11] = attr;
            e
        };
        assert_eq!(dir_entry(&mk(0x00, 0)), Some(DirKind::End));
        assert_eq!(dir_entry(&mk(0xE5, 0x20)), Some(DirKind::Skip)); // 删除
        assert_eq!(dir_entry(&mk(b'A', 0x0F)), Some(DirKind::Skip)); // LFN
        assert_eq!(dir_entry(&mk(b'A', 0x08)), Some(DirKind::Skip)); // 卷标
        // 子目录：簇 0 + size 0 = 空目录（合法——首簇 0 仅配零 size）。
        match dir_entry(&mk(b'A', 0x10)).unwrap() {
            DirKind::Dir(d) => {
                assert_eq!(d.first_clus, 0);
                assert_eq!(d.size, 0);
                assert_eq!(d.name[0], b'A');
            }
            other => panic!("应为目录项：{:?}", other),
        }
        // 文件项：首簇 0 + size 0 = 空文件（A-6 `>` 建空文件面）——合法。
        match dir_entry(&mk(b'A', 0x20)).unwrap() {
            DirKind::File(f) => {
                assert_eq!(f.first_clus, 0);
                assert_eq!(f.size, 0);
                assert_eq!(f.name[0], b'A');
            }
            other => panic!("应为空文件项：{:?}", other),
        }
        // 首簇 0 配 size>0 → 畸形（无簇却带声明长度）。
        let mut e01 = mk(b'A', 0x20);
        e01[28..32].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(dir_entry(&e01), None);
        // 首簇 1（保留项）→ 畸形（即使 size=0）。
        let mut e1 = mk(b'A', 0x20);
        e1[26..28].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(dir_entry(&e1), None);
        // 长度面：31/33 → None。
        assert_eq!(dir_entry(&[0u8; 31]), None);
        assert_eq!(dir_entry(&[0u8; 33]), None);
    }

    #[test]
    fn dir_entry_subdir_faces() {
        // 目录项专面（C-0 刀 3）：合法形（簇 ≥ 2 + size 0）/ 拒面（size>0
        // ——规范目录 size 恒 0；簇 1 保留）。
        let mk = |name: &[u8], clus: u16, size: u32| {
            let mut e = [0u8; 32];
            e[..name.len()].copy_from_slice(name);
            e[11] = 0x10;
            e[26..28].copy_from_slice(&clus.to_le_bytes());
            e[28..32].copy_from_slice(&size.to_le_bytes());
            e
        };
        match dir_entry(&mk(b"DATA", 6, 0)).unwrap() {
            DirKind::Dir(d) => {
                assert_eq!(&d.name[..4], b"DATA");
                assert_eq!((d.first_clus, d.size), (6, 0));
            }
            other => panic!("应为目录项：{:?}", other),
        }
        assert_eq!(dir_entry(&mk(b"DATA", 6, 1)), None); // size>0 畸形
        assert_eq!(dir_entry(&mk(b"DATA", 1, 0)), None); // 簇 1 保留
    }

    #[test]
    fn name_roundtrip() {
        let mut out = [0u8; 16];
        for (disp, n11) in [
            (&b"HELLO.TXT"[..], &b"HELLO   TXT"[..]),
            (&b"PATTERN.BIN"[..], &b"PATTERN BIN"[..]),
            (&b"A.1"[..], &b"A       1  "[..]),
            (&b"README"[..], &b"README     "[..]), // 无扩展名 → 无点
        ] {
            let packed = name_to_fat11(disp).unwrap();
            assert_eq!(&packed[..], n11);
            let n = fat11_display(&packed, &mut out).unwrap();
            assert_eq!(&out[..n], disp);
        }
    }

    #[test]
    fn name_rejects() {
        // 小写 / 长 base / 长 ext / 双点 / 空名 / 空基名 / 非法字符。
        for bad in [
            &b"hello.txt"[..],
            &b"TOOLONGNAME.X"[..],
            &b"A.ABCD"[..],
            &b"A.B.C"[..],
            &b""[..],
            &b".TXT"[..],
            &b"A-B.TXT"[..],
            &b"A B"[..],
            &b"A._1"[..],
        ] {
            assert_eq!(name_to_fat11(bad), None, "应拒: {:?}", bad);
        }
    }

    #[test]
    fn fat11_display_bounds() {
        let mut out = [0u8; 4];
        // "PATTERN.BIN" 11 字符——4 字节 out → None（不截断）。
        let n11 = name_to_fat11(b"PATTERN.BIN").unwrap();
        assert_eq!(fat11_display(&n11, &mut out), None);
        let mut out = [0u8; 11];
        assert_eq!(fat11_display(&n11, &mut out), Some(11));
        // 空 out 一律 None（None 拒绝契约）。
        assert_eq!(fat11_display(&n11, &mut []), None);
        // 无扩展名：need == base 长（"READMEX" 7 字面直断无点路径）。
        let n11 = name_to_fat11(b"READMEX").unwrap();
        let mut out = [0u8; 7];
        assert_eq!(fat11_display(&n11, &mut out), Some(7));
        assert_eq!(&out, b"READMEX");
    }

    #[test]
    fn fat12_set_consensus_and_bounds() {
        // 写侧共识：与 pack（mkfat 同式独立实现）逐字节全等 + fat12_entry
        // 往返认回（写 → 读闭环）。
        for (n, v) in [
            (2u32, 0x345u32),
            (3, 0x678),
            (4, 2),
            (5, FAT12_EOC),
            (6, 0), // Free 写（commit 的旧链释放路径）
            (7, FAT12_BAD),
            (9, 0xFFF),
            (10, 0x00A),
        ] {
            let mut a = [0u8; 20];
            let mut b = [0u8; 20];
            fat12_set(&mut a, n, v).unwrap();
            pack(&mut b, n, v);
            assert_eq!(a, b, "写侧双实现分歧 @ ({n}, {v:#x})");
            let expect = match v {
                0 => FatEntry::Free,
                FAT12_BAD => FatEntry::Bad,
                FAT12_EOC_MIN..=0xFFF => FatEntry::Eoc,
                _ => FatEntry::Next(v),
            };
            assert_eq!(fat12_entry(&a, n), Some(expect));
        }
        // 邻居独立（12 位紧包只动自家 nibble）：奇项写不扰两侧偶项、后写
        // 偶项的共享字节半位不扰先写奇项。
        let mut fat = [0u8; 20];
        fat12_set(&mut fat, 2, 0x123).unwrap();
        fat12_set(&mut fat, 4, 0x456).unwrap();
        fat12_set(&mut fat, 3, 0x789).unwrap(); // 奇项 3 夹在偶项 2/4 间
        assert_eq!(fat12_entry(&fat, 2), Some(FatEntry::Next(0x123)));
        assert_eq!(fat12_entry(&fat, 4), Some(FatEntry::Next(0x456)));
        fat12_set(&mut fat, 5, 0xABC).unwrap(); // 奇项 5 与偶项 4 共享 byte 7
        assert_eq!(fat12_entry(&fat, 4), Some(FatEntry::Next(0x456)));
        assert_eq!(fat12_entry(&fat, 5), Some(FatEntry::Next(0xABC)));
        // 拒面：簇 0/1、值 > 0xFFF、切片短；恰合（exact fit）双向过。
        assert_eq!(fat12_set(&mut [0u8; 20], 0, 2), None);
        assert_eq!(fat12_set(&mut [0u8; 20], 1, 2), None);
        assert_eq!(fat12_set(&mut [0u8; 20], 2, 0x1000), None);
        let mut short = [0u8; 4]; // 项 2 @ off 3 需 5 字节
        assert_eq!(fat12_set(&mut short, 2, 3), None);
        let mut exact = [0u8; 5]; // 恰合：项 2 需 [3,5) 两字节
        assert_eq!(fat12_set(&mut exact, 2, 3), Some(()));
        assert_eq!(fat12_entry(&exact, 2), Some(FatEntry::Next(3)));
    }

    #[test]
    fn encode_entry_roundtrip() {
        // encode → dir_entry 往返认回（含空文件面）。V-2 起 bits 参按宽分派
        // 标记域拒面（FAT12 现役面传 FatBits::Fat12——断言零改动）。
        let n11 = name_to_fat11(b"RW.TXT").unwrap();
        let mut e = [0u8; 32];
        encode_file_entry(&n11, 6, 3000, &mut e, FatBits::Fat12).unwrap();
        match dir_entry(&e).unwrap() {
            DirKind::File(f) => {
                assert_eq!(&f.name, b"RW      TXT");
                assert_eq!(f.first_clus, 6);
                assert_eq!(f.size, 3000);
            }
            other => panic!("应为文件项：{:?}", other),
        }
        // 空文件：首簇 0 + size 0。
        let mut e = [0u8; 32];
        encode_file_entry(&n11, 0, 0, &mut e, FatBits::Fat12).unwrap();
        match dir_entry(&e).unwrap() {
            DirKind::File(f) => assert_eq!((f.first_clus, f.size), (0, 0)),
            other => panic!("应为空文件项：{:?}", other),
        }
        // 拒面（与 dir_entry 镜像）：簇 1 / 簇 0 配 size>0 / 标记值域。
        let mut e = [0u8; 32];
        assert_eq!(encode_file_entry(&n11, 1, 0, &mut e, FatBits::Fat12), None);
        assert_eq!(encode_file_entry(&n11, 0, 1, &mut e, FatBits::Fat12), None);
        assert_eq!(encode_file_entry(&n11, 0xFF7, 0, &mut e, FatBits::Fat12), None);
        assert_eq!(encode_file_entry(&n11, 0xFFF, 0, &mut e, FatBits::Fat12), None);
        // 逐字节共识：Rust 编码器产 HELLO.TXT 项 == mkfat 造的 fixture 项
        // （attr/时间戳定值/字段偏移全同形——python 写 Rust 读再写回去）。
        let mut e = [0u8; 32];
        encode_file_entry(b"HELLO   TXT", 2, 28, &mut e, FatBits::Fat12).unwrap();
        assert_eq!(&e, &root()[32..64]);
    }

    #[test]
    fn find_free_cluster_scan() {
        // fixture：簇 2..7 占用（文件 + DATA 目录面）→ 首空 = 8；from 钳位
        // 与越过语义（FAT12 现役面传 FatBits::Fat12——断言零改动）。
        assert_eq!(find_free_cluster(fat0(), 2, 506, FatBits::Fat12), Some(8));
        assert_eq!(find_free_cluster(fat0(), 0, 506, FatBits::Fat12), Some(8));
        assert_eq!(find_free_cluster(fat0(), 7, 506, FatBits::Fat12), Some(8));
        assert_eq!(find_free_cluster(fat0(), 8, 506, FatBits::Fat12), Some(8));
        // 域界 = 2 + cluster_count（开区间）：count 收窄后扫描界随之。
        let mut fat = [0u8; 16]; // 覆盖项 2..9（项 8 @ off 12 需 14B）
        fat12_set(&mut fat, 2, FAT12_EOC).unwrap();
        assert_eq!(find_free_cluster(&fat, 2, 7, FatBits::Fat12), Some(3)); // 界 9 → 3..8 域内
        assert_eq!(find_free_cluster(&fat, 3, 1, FatBits::Fat12), None); // 界 3 → 域空
        // 盘满：域内全 EOC → None。
        let mut full = [0u8; 16];
        for c in 2..9u32 {
            fat12_set(&mut full, c, FAT12_EOC).unwrap();
        }
        assert_eq!(find_free_cluster(&full, 2, 7, FatBits::Fat12), None);
    }

    #[test]
    fn bpb_fat16_fixture_consensus() {
        // 跨实现共识①（FAT16 面，V-2）：mkfat --fixture16 造的 BPB 被 Rust
        // 解析器逐字段认回；判宽 = 簇数域单一事实源（fstype 串仅记录面）。
        let g = parse_bpb(&FX16[..512]).unwrap();
        assert_eq!(g.fat_bits, FatBits::Fat16);
        assert_eq!(g.bps, 512);
        assert_eq!(g.spc, 4);
        assert_eq!(g.reserved, 1);
        assert_eq!(g.nfats, 2);
        assert_eq!(g.root_entries, 16);
        assert_eq!(g.total, 16384);
        assert_eq!(g.fat_sz, 16);
        assert_eq!(g.hidden, 0); // build_volume_puts 无 vblk 保留区前缀
        assert_eq!(g.fat_sector, 1);
        assert_eq!(g.root_sector, 33);
        assert_eq!(g.root_secs, 1);
        assert_eq!(g.data_sector, 34);
        assert_eq!(g.cluster_count, 4087); // 恰贴 FAT12/16 分界（≥ 4085）
        assert_eq!(&FX16[54..62], b"FAT16   "); // 串不读——判宽只看簇数
    }

    #[test]
    fn fat16_chain_fixture_consensus() {
        // 跨实现共识②（FAT16 面）：HELLO.TXT 单簇（2, EOC）；PATTERN.BIN
        // 链 3→4→5；DATA 目录单簇 6、其子 NOTE.TXT 单簇 7；8 起 Free。
        // 保留项 FAT[0]=0xFF00|media(0xF0)=0xFFF0 / FAT[1]=0xFFFF、链尾
        // 编码恒 0xFFFF——与 mkfat fat16_set 双端同值。
        let fat = fat0_16();
        assert_eq!(fat16_entry(fat, 2), Some(FatEntry::Eoc));
        assert_eq!(fat16_entry(fat, 3), Some(FatEntry::Next(4)));
        assert_eq!(fat16_entry(fat, 4), Some(FatEntry::Next(5)));
        assert_eq!(fat16_entry(fat, 5), Some(FatEntry::Eoc));
        assert_eq!(fat16_entry(fat, 6), Some(FatEntry::Eoc));
        assert_eq!(fat16_entry(fat, 7), Some(FatEntry::Eoc));
        assert_eq!(fat16_entry(fat, 8), Some(FatEntry::Free));
        assert_eq!(u16le(fat, 0), 0xFFF0); // FAT[0] = 0xFF00|media
        assert_eq!(u16le(fat, 2), 0xFFFF); // FAT[1]
        assert_eq!(u16le(fat, 5 * 2), FAT16_EOC as u16); // 链尾编码值
    }

    #[test]
    fn dir16_fixture_consensus() {
        // 跨实现共识③（FAT16 面）：BERRYBOOT 卷标 Skip + 两文件项 + DATA
        // 目录项 + End；DATA 簇子项表 = `.`自指@6 + `..`指根@0 + NOTE.TXT@7
        // + End（build_volume_puts 构造体产 ./..——与 FAT12 侧扁平 dirs
        // 不同形，两端约定见 mkfat.py 头注）。
        let r = root_16();
        assert_eq!(dir_entry(&r[0..32]), Some(DirKind::Skip)); // BERRYBOOT 卷标
        match dir_entry(&r[32..64]).unwrap() {
            DirKind::File(f) => {
                assert_eq!(&f.name, b"HELLO   TXT");
                assert_eq!((f.first_clus, f.size), (2, 28));
            }
            other => panic!("项 1 应为文件：{:?}", other),
        }
        match dir_entry(&r[64..96]).unwrap() {
            DirKind::File(f) => {
                assert_eq!(&f.name, b"PATTERN BIN");
                assert_eq!((f.first_clus, f.size), (3, 5000));
            }
            other => panic!("项 2 应为文件：{:?}", other),
        }
        match dir_entry(&r[96..128]).unwrap() {
            DirKind::Dir(d) => {
                assert_eq!(&d.name, b"DATA       ");
                assert_eq!((d.first_clus, d.size), (6, 0));
            }
            other => panic!("项 3 应为目录：{:?}", other),
        }
        assert_eq!(dir_entry(&r[128..160]), Some(DirKind::End));
        // DATA 簇 6 子项表。
        let g = parse_bpb(&FX16[..512]).unwrap();
        let off6 = (g.data_sector as usize + 4 * g.spc as usize) * g.bps as usize;
        let d = &FX16[off6..];
        match dir_entry(&d[0..32]).unwrap() {
            DirKind::Dir(dd) => {
                assert_eq!(&dd.name, b".          ");
                assert_eq!((dd.first_clus, dd.size), (6, 0)); // 自指
            }
            other => panic!("DATA 首项应为 `.`：{:?}", other),
        }
        match dir_entry(&d[32..64]).unwrap() {
            DirKind::Dir(dd) => {
                assert_eq!(&dd.name, b"..         ");
                assert_eq!((dd.first_clus, dd.size), (0, 0)); // 根上指 0
            }
            other => panic!("DATA 次项应为 `..`：{:?}", other),
        }
        match dir_entry(&d[64..96]).unwrap() {
            DirKind::File(f) => {
                assert_eq!(&f.name, b"NOTE    TXT");
                assert_eq!((f.first_clus, f.size), (7, 24));
            }
            other => panic!("DATA 三项应为 NOTE.TXT：{:?}", other),
        }
        assert_eq!(dir_entry(&d[96..128]), Some(DirKind::End));
        assert!(d[128..2048].iter().all(|&b| b == 0)); // 终结后整簇余零
    }

    #[test]
    fn file16_bytes_fixture_consensus() {
        // 跨实现共识④（FAT16 面）：fat16_entry 驱动内联 16 位链走读
        // PATTERN 5000B 与生成式对拍（fsmgr nth_clus 同几何）；HELLO/
        // NOTE 单簇字节与链尾簇零填（内容与 FAT12 fixture 同串——双宽
        // 同源）。
        let g = parse_bpb(&FX16[..512]).unwrap();
        let csz = g.spc as usize * g.bps as usize;
        let clus_off = |c: u32| -> usize {
            (g.data_sector as usize + (c as usize - 2) * g.spc as usize) * g.bps as usize
        };
        let fat = fat0_16();
        let mut buf = [0u8; 5000];
        let mut off = 0usize;
        let mut c = 3u32;
        while off < buf.len() {
            let take = csz.min(buf.len() - off);
            buf[off..off + take].copy_from_slice(&FX16[clus_off(c)..clus_off(c) + take]);
            off += take;
            if off < buf.len() {
                c = match fat16_entry(fat, c) {
                    Some(FatEntry::Next(v)) => v,
                    Some(FatEntry::Eoc) => panic!("EOC 早到 @ off={off}"),
                    _ => panic!("链断 @ clus={c}"),
                };
            }
        }
        let mut expect = [0u8; 5000];
        for (i, b) in expect.iter_mut().enumerate() {
            *b = ((i * 31 + 7) % 256) as u8;
        }
        assert_eq!(buf, expect);
        assert_eq!(&FX16[clus_off(2)..clus_off(2) + 28], b"berryos fat read-only hello\n");
        assert_eq!(&FX16[clus_off(7)..clus_off(7) + 24], b"berryos fat subdir note\n");
        // 链尾簇零填（3 簇 6144B，size 5000 之外到链尾全零）。
        assert!(FX16[clus_off(3) + 5000..clus_off(3) + 3 * csz].iter().all(|&b| b == 0));
    }

    #[test]
    fn fat16_entry_bounds() {
        // 值域四态分类 + 切片界 + 保留簇号/保留值拒面（fat12 版的宽域
        // 对偶——12 位版见 fat12_parity_and_exact_fit）。
        let mut fat = [0u8; 24]; // 项 2..11 定宽（项 n @ 2n）
        pack16(&mut fat, 2, 0); // Free
        pack16(&mut fat, 3, FAT16_BAD); // Bad 0xFFF7
        pack16(&mut fat, 4, FAT16_EOC_MIN); // Eoc 下界 0xFFF8
        pack16(&mut fat, 5, FAT16_EOC); // Eoc 上界 0xFFFF（写侧恒用值）
        pack16(&mut fat, 6, 2); // Next 下界
        pack16(&mut fat, 7, 0xFFF6); // Next 上界
        assert_eq!(fat16_entry(&fat, 2), Some(FatEntry::Free));
        assert_eq!(fat16_entry(&fat, 3), Some(FatEntry::Bad));
        assert_eq!(fat16_entry(&fat, 4), Some(FatEntry::Eoc));
        assert_eq!(fat16_entry(&fat, 5), Some(FatEntry::Eoc));
        assert_eq!(fat16_entry(&fat, 6), Some(FatEntry::Next(2)));
        assert_eq!(fat16_entry(&fat, 7), Some(FatEntry::Next(0xFFF6)));
        // 保留簇号 0/1——不是文件簇。
        assert_eq!(fat16_entry(&fat, 0), None);
        assert_eq!(fat16_entry(&fat, 1), None);
        // 切片界：项 7 @ off 14 需 [14,16)——15 字节 None、16 字节恰合。
        assert_eq!(fat16_entry(&fat[..15], 7), None);
        assert_eq!(fat16_entry(&fat[..16], 7), Some(FatEntry::Next(0xFFF6)));
        // 保留值 1 出文件簇位 = 畸形（0 是 Free 分类，见上项 2）。
        let mut f1 = [0u8; 8];
        pack16(&mut f1, 2, 1);
        assert_eq!(fat16_entry(&f1, 2), None);
    }

    #[test]
    fn fat16_set_consensus_and_bounds() {
        // 写侧共识（FAT16 面）：与 pack16（mkfat 同式独立实现）逐字节全等
        // + fat16_entry 往返认回（写 → 读闭环）。
        for (n, v) in [
            (2u32, 0x1234u32),
            (3, 2),
            (4, FAT16_EOC),
            (5, 0), // Free 写（commit 的旧链释放路径）
            (6, FAT16_BAD),
            (7, 0xABCD),
            (9, 0xFFFF),
            (10, 0x00FF),
        ] {
            let mut a = [0u8; 24];
            let mut b = [0u8; 24];
            fat16_set(&mut a, n, v).unwrap();
            pack16(&mut b, n, v);
            assert_eq!(a, b, "写侧双实现分歧 @ ({n}, {v:#x})");
            let expect = match v {
                0 => FatEntry::Free,
                FAT16_BAD => FatEntry::Bad,
                FAT16_EOC_MIN..=0xFFFF => FatEntry::Eoc,
                _ => FatEntry::Next(v),
            };
            assert_eq!(fat16_entry(&a, n), Some(expect));
        }
        // 邻居独立（定宽全字节覆盖——无 12 位版的共享 nibble 面）：写 n
        // 不扰 n±1。
        let mut fat = [0u8; 24];
        fat16_set(&mut fat, 4, 0xFFFF).unwrap();
        fat16_set(&mut fat, 5, 0x1234).unwrap();
        fat16_set(&mut fat, 6, 0xABCD).unwrap();
        assert_eq!(fat16_entry(&fat, 4), Some(FatEntry::Eoc));
        assert_eq!(fat16_entry(&fat, 5), Some(FatEntry::Next(0x1234)));
        assert_eq!(fat16_entry(&fat, 6), Some(FatEntry::Next(0xABCD)));
        // 拒面：簇 0/1、值 > 0xFFFF、切片短（半项）；恰合（exact fit）过。
        assert_eq!(fat16_set(&mut [0u8; 24], 0, 2), None);
        assert_eq!(fat16_set(&mut [0u8; 24], 1, 2), None);
        assert_eq!(fat16_set(&mut [0u8; 24], 2, 0x10000), None);
        let mut short = [0u8; 5]; // 项 2 @ [4,6)——半项
        assert_eq!(fat16_set(&mut short, 2, 3), None);
        let mut exact = [0u8; 6]; // 恰合：项 2 需 [4,6) 两字节
        assert_eq!(fat16_set(&mut exact, 2, 3), Some(()));
        assert_eq!(fat16_entry(&exact, 2), Some(FatEntry::Next(3)));
    }

    #[test]
    fn encode_entry_fat16_domain() {
        // 标记值域按宽分派：Fat16 收 0xFFF6 拒 0xFFF7；Fat12 收 0xFF6 拒
        // 0xFF7；Fat12 同拒 FAT16 域簇号（宽域值不静默截断）。双宽
        // roundtrip（dir_entry 无宽度参数——值域内原样认回）。
        let n11 = name_to_fat11(b"RW.TXT").unwrap();
        let mut e = [0u8; 32];
        assert!(encode_file_entry(&n11, 0xFFF6, 0, &mut e, FatBits::Fat16).is_some());
        assert_eq!(encode_file_entry(&n11, 0xFFF7, 0, &mut e, FatBits::Fat16), None);
        assert!(encode_file_entry(&n11, 0xFF6, 0, &mut e, FatBits::Fat12).is_some());
        assert_eq!(encode_file_entry(&n11, 0xFF7, 0, &mut e, FatBits::Fat12), None);
        assert_eq!(encode_file_entry(&n11, 0xFFF6, 0, &mut e, FatBits::Fat12), None);
        encode_file_entry(&n11, 0xFFF6, 3000, &mut e, FatBits::Fat16).unwrap();
        match dir_entry(&e).unwrap() {
            DirKind::File(f) => assert_eq!((f.first_clus, f.size), (0xFFF6, 3000)),
            other => panic!("应为 Fat16 域文件项：{:?}", other),
        }
        encode_file_entry(&n11, 0xFF6, 3000, &mut e, FatBits::Fat12).unwrap();
        match dir_entry(&e).unwrap() {
            DirKind::File(f) => assert_eq!((f.first_clus, f.size), (0xFF6, 3000)),
            other => panic!("应为 Fat12 域文件项：{:?}", other),
        }
    }

    #[test]
    fn find_free_cluster_fat16() {
        // bits 参扫描（FAT16 面）：fixture16 链面 2..7 占用 → 首空 = 8；
        // fat16_set 造盘态的域界/盘满（fat12 版镜像见 find_free_cluster_scan）。
        assert_eq!(find_free_cluster(fat0_16(), 2, 4087, FatBits::Fat16), Some(8));
        assert_eq!(find_free_cluster(fat0_16(), 0, 4087, FatBits::Fat16), Some(8));
        assert_eq!(find_free_cluster(fat0_16(), 8, 4087, FatBits::Fat16), Some(8));
        let mut fat = [0u8; 24]; // 项 2..11 定宽（项 10 @ 20 需 22B）
        fat16_set(&mut fat, 2, FAT16_EOC).unwrap();
        assert_eq!(find_free_cluster(&fat, 2, 7, FatBits::Fat16), Some(3)); // 界 9 → 3..8 域内
        assert_eq!(find_free_cluster(&fat, 3, 1, FatBits::Fat16), None); // 界 3 → 域空
        let mut full = [0u8; 24];
        for c in 2..9u32 {
            fat16_set(&mut full, c, FAT16_EOC).unwrap();
        }
        assert_eq!(find_free_cluster(&full, 2, 7, FatBits::Fat16), None);
    }

    #[test]
    fn count_free_clusters_fixture_consensus() {
        // 跨实现共识⑥（C-2 刀 2）：双宽 fixture 空闲簇字面钉板——簇 2..7
        // 占用（HELLO 单簇 + PATTERN 链 3 簇 + DATA 单簇 + NOTE 单簇）→
        // FAT12 506-6=500 / FAT16 4087-6=4081（df 对拍 oracle 的纯函数侧锚）。
        assert_eq!(count_free_clusters(fat0(), 506, FatBits::Fat12), Some(500));
        assert_eq!(count_free_clusters(fat0_16(), 4087, FatBits::Fat16), Some(4081));
    }

    #[test]
    fn count_free_clusters_bounds() {
        // 全闲（零 FAT = 域内全 Free，count == cluster_count）/ 全忙（全
        // EOC = 0）/ 空域（cluster_count=0）三界 + 非零值族（Bad/EOC/Next）
        // 皆不计。FAT12 面（项 8 @ off 12 需 14B——16B 切片盖 2..8 域）。
        let z12 = [0u8; 16];
        assert_eq!(count_free_clusters(&z12, 7, FatBits::Fat12), Some(7));
        let mut full = [0u8; 16];
        for c in 2..9u32 {
            fat12_set(&mut full, c, FAT12_EOC).unwrap();
        }
        assert_eq!(count_free_clusters(&full, 7, FatBits::Fat12), Some(0));
        assert_eq!(count_free_clusters(&z12, 0, FatBits::Fat12), Some(0));
        let mut mix = [0u8; 16];
        fat12_set(&mut mix, 2, FAT12_BAD).unwrap();
        fat12_set(&mut mix, 3, FAT12_EOC).unwrap();
        fat12_set(&mut mix, 4, 5).unwrap(); // Next
        assert_eq!(count_free_clusters(&mix, 7, FatBits::Fat12), Some(4)); // 5..8 四项闲
        // FAT16 对偶（项 n @ 2n——8 项域需 18B）。
        let z16 = [0u8; 18];
        assert_eq!(count_free_clusters(&z16, 7, FatBits::Fat16), Some(7));
        let mut full16 = [0u8; 18];
        for c in 2..9u32 {
            fat16_set(&mut full16, c, FAT16_EOC).unwrap();
        }
        assert_eq!(count_free_clusters(&full16, 7, FatBits::Fat16), Some(0));
        // 畸形/切片短 → None 诚实拒（上报真值不猜——与 find 的跳过面分立）。
        assert_eq!(count_free_clusters(&[0u8; 4], 506, FatBits::Fat12), None);
        assert_eq!(count_free_clusters(&[0u8; 4], 4087, FatBits::Fat16), None);
        let mut bad = [0u8; 16];
        fat12_set(&mut bad, 2, 1).unwrap(); // 保留值 1 落文件簇位 = 畸形
        assert_eq!(count_free_clusters(&bad, 7, FatBits::Fat12), None);
        let mut bad16 = [0u8; 18];
        fat16_set(&mut bad16, 2, 1).unwrap();
        assert_eq!(count_free_clusters(&bad16, 7, FatBits::Fat16), None);
    }

    #[test]
    fn bpb_fat16_boundary_dispatch() {
        // 簇数分派四界钉板（FX boot 补丁 total——data_sector=6/spc=4 恒定）：
        // cc 4084 → Fat12、4085 → Fat16（分界含）、65524（total32 合成路）→
        // Fat16、65525 → None（FAT32 地界拒）。
        let patch = |t16: u16, t32: u32| {
            let mut b = FX[..512].to_vec();
            b[19..21].copy_from_slice(&t16.to_le_bytes());
            b[32..36].copy_from_slice(&t32.to_le_bytes());
            b
        };
        let g = parse_bpb(&patch(16342, 0)).unwrap(); // (16342-6)/4 = 4084
        assert_eq!(g.fat_bits, FatBits::Fat12);
        assert_eq!(g.cluster_count, 4084);
        let g = parse_bpb(&patch(16346, 0)).unwrap(); // 4085
        assert_eq!(g.fat_bits, FatBits::Fat16);
        assert_eq!(g.cluster_count, 4085);
        let g = parse_bpb(&patch(0, 262102)).unwrap(); // 65524（t16=0 走 32 位合成）
        assert_eq!(g.fat_bits, FatBits::Fat16);
        assert_eq!(g.cluster_count, 65524);
        assert_eq!(parse_bpb(&patch(0, 262106)), None); // 65525 → FAT32 地界
    }

    // ── B4 撕裂面基元族（掉电安全首刀）────────────────────────────
    // 覆写 commit 的撕裂窗「FAT 已写、根未写」在盘面 = 旧链已 Free、根
    // 仍指旧首簇。恢复 commit 会撞上：smoke 单文件覆写负载下旧首簇是
    // 最低 Free → 升序分配复用其为新链头（一般负载可中段复用/不复用）
    // → 释放旧链必须跳过 ∈ 新链者（fsmgr fat.rs ④ 守卫，成员制）。
    // 本族把该窗的 FAT 级基元逐件钉板（毒环前提可复现、不靠运气采样）。

    #[test]
    fn torn_fat_poison_window_primitives() {
        // 毒环态逐基元：FX FAT0 副本上 Free 掉 PATTERN 链头（簇 3）——
        // 盘面 FAT 视角等价于撕裂窗里「旧链已释放」的瞬间。
        let mut fat = fat0().to_vec();
        fat12_set(&mut fat, 3, 0).unwrap();
        assert_eq!(fat12_entry(&fat, 3), Some(FatEntry::Free));
        // 空闲计数 +1（df/STATFS 面对撕裂盘仍出真值）。
        assert_eq!(count_free_clusters(&fat, 506, FatBits::Fat12), Some(501));
        // 毒环前提钉板：本 fixture 内簇 3 是全域最低 Free → find(2..)
        // 升序分配必命中它——smoke 负载下恢复 commit 的新链头就是旧首
        // 簇本身（④ 守卫按成员制不依赖头部复用，此处钉的是复用力学
        // 前提本身，非防御性兜底）。
        assert_eq!(find_free_cluster(&fat, 2, 506, FatBits::Fat12), Some(3));
        // 根目录项不受 FAT 撕裂影响（decode 面）：仍指簇 3——「根指 first、
        // FAT[first]=Free」即毒环态的字面形状。
        match dir_entry(&root()[64..96]).unwrap() {
            DirKind::File(f) => assert_eq!((f.first_clus, f.size), (3, 5000)),
            other => panic!("PATTERN 槽应为文件项：{:?}", other),
        }
        // 重链恢复：新链写回簇 3 → Next(4) 原样认回（守卫跳过释放后，
        // 新链头存活、后续簇号无损）。
        fat12_set(&mut fat, 3, 4).unwrap();
        assert_eq!(fat12_entry(&fat, 3), Some(FatEntry::Next(4)));
        assert_eq!(count_free_clusters(&fat, 506, FatBits::Fat12), Some(500));
    }

    #[test]
    fn torn_fat_midchain_free() {
        // 假设性中链 Free 形态的基元回归（防 decode/find 语义漂移）：由现
        // commit 写序（④ 纯内存 + 链内升序 + 整扇区 FAT 写）在盘面结构性
        // 不可达——中段 Free 先于链头 Free 出现无路径（见评估 §1 与 smoke
        // Boot B 的 midchain 负向断言）。
        let mut fat = fat0().to_vec();
        fat12_set(&mut fat, 4, 0).unwrap();
        assert_eq!(fat12_entry(&fat, 4), Some(FatEntry::Free));
        assert_eq!(fat12_entry(&fat, 3), Some(FatEntry::Next(4)));
        assert_eq!(count_free_clusters(&fat, 506, FatBits::Fat12), Some(501));
        assert_eq!(find_free_cluster(&fat, 2, 506, FatBits::Fat12), Some(4));
        fat12_set(&mut fat, 4, 5).unwrap();
        assert_eq!(fat12_entry(&fat, 4), Some(FatEntry::Next(5)));
    }

    #[test]
    fn torn_fat_value_domain_honesty() {
        // 撕裂盘上的畸形项（保留值 1 落文件簇位——半写 12 位紧包的典型
        // 产物）：count 诚实拒 None（上报真值不猜）、find 跳过畸形继续找
        // ——两面语义按消费方分立的撕裂盘回归钉板。
        let mut fat = fat0().to_vec();
        pack(&mut fat, 8, 1); // 独立写实现造畸形（fat12_set 值域拦不住 1）
        assert_eq!(fat12_entry(&fat, 8), None);
        assert_eq!(count_free_clusters(&fat, 506, FatBits::Fat12), None);
        assert_eq!(find_free_cluster(&fat, 2, 506, FatBits::Fat12), Some(9));
    }

    #[test]
    fn torn_fat16_poison_twin() {
        // FAT16 能力层孪生（V-2 面；生产写路径 v1 仅 FAT12——fat.rs 同
        // 形守卫的未来扩面基线）：簇 3 Free → 升序命中 3 → 重链认回。
        let mut fat = fat0_16().to_vec();
        fat16_set(&mut fat, 3, 0).unwrap();
        assert_eq!(fat16_entry(&fat, 3), Some(FatEntry::Free));
        assert_eq!(count_free_clusters(&fat, 4087, FatBits::Fat16), Some(4082));
        assert_eq!(find_free_cluster(&fat, 2, 4087, FatBits::Fat16), Some(3));
        fat16_set(&mut fat, 3, 4).unwrap();
        assert_eq!(fat16_entry(&fat, 3), Some(FatEntry::Next(4)));
    }
}
