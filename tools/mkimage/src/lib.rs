//! mkimage——确定性整盘镜像打包 Rust 件（b 层 #8 刀序第三位首刀）。
//!
//! 与 Python `tools/mkimage.py` 对齐（单编码器纪律：FAT 写法只在 mkfat
//! 一处——本件经 path 依赖复用 `mkfat::build_volume_puts`，Python 侧
//! `import mkfat` 的对偶面；MBR/manifest/验签组装面在此单源）。首刀：
//! 纯面 lib + CLI 壳 + oracle 字节级对拍；产链现役件仍 Python（二刀换轨
//! 再切 justfile）。
//!
//! 盘布局 = [LBA0 MBR][LBA2048.. FAT16 主分区][尾部 16MiB 密钥材料预留区]：
//! MBR 表项 0 = FAT16 LBA(0x0E)、表项 1 = Non-FS data(0xDA) 预留槽；
//! manifest v1（格式冻结 2026-11-10）= 同目录侧产物，逐字节规约见
//! `build_manifest` 注释。确定性：无时刻输入、同输入同字节。

pub const SECTOR: usize = 512;
/// 分区起始 LBA（1MiB 对齐——现代盘惯例，EDK2/UTM 皆认）。
pub const PART_LBA: usize = 2048;
/// LBA 假几何（protective CHS 自洽用——非寻址依据，草案 §V-3）。
pub const HEADS: u32 = 255;
pub const SPT: u32 = 63;
pub const PART_TYPE_FAT16_LBA: u8 = 0x0E;
/// FAT 段默认容量 MiB（A1-3b：rootfs 40MB+ 验收形态同盘容）。
pub const DEFAULT_SIZE_MIB: usize = 96;

/// 件 4【SEC-7】密钥材料分区预留槽：16MiB 全零、type 0xDA（Android
/// /metadata 同型——v1 只占槽不写内容）。
pub const META_SECTORS: usize = 16 * 1024 * 1024 / SECTOR; // 32768 扇区
pub const PART_TYPE_KEYMETA: u8 = 0xDA;

/// 件 3【SEC-1】manifest v1 格式冻结常量（列宽是契约的一部分——改即破冻）。
pub const MANIFEST_NAME: &str = "manifest.txt";
pub const MANIFEST_FROZEN: &str = "2026-11-10"; // v1 冻结日（V9 加列）
pub const MANIFEST_PATH_W: usize = 24;
pub const MANIFEST_SIZE_W: usize = 12;
pub const MANIFEST_RB_W: usize = 20;
pub const MANIFEST_PR_W: usize = 10;

/// P2【SDK-2 §五】官方源静态实例挂接合同（与 gensourceindex.py
/// INDEX_NAME/KEYS_NAME 同源同步义务——双侧 selftest 互检绊线）。
pub const SRCINST_INDEX_NAME: &str = "source-index.tlv";
pub const SRCINST_KEYS_NAME: &str = "source-keys.txt";
pub const SRCINST_FAT_INDEX: &[u8] = b"/SOURCE/SRCINDEX.TLV";
pub const SRCINST_FAT_KEYS: &[u8] = b"/SOURCE/SRCKEYS.TXT";

/// P3 boot 期验签两件 FAT 路径与几何契约。
pub const SIGN_FAT_PATH: &[u8] = b"/KERNEL.SIG";
pub const TRUSTPK_FAT_PATH: &[u8] = b"/TRUSTPK";
pub const SIGN_LEN: usize = 64; // ed25519 裸签（berry-sign sign-raw stdout）
pub const TRUSTPK_LEN: usize = 32; // ed25519 公钥

/// mkimage 错误面（Python SystemExit loud FAIL 的 Rust 化）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MkimageError(pub String);

impl std::fmt::Display for MkimageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mkimage: FAIL: {}", self.0)
    }
}

impl std::error::Error for MkimageError {}

impl From<mkfat::MkfatError> for MkimageError {
    fn from(e: mkfat::MkfatError) -> Self {
        MkimageError(format!("mkfat 卷编码：{e}"))
    }
}

fn fail<T>(msg: impl Into<String>) -> Result<T, MkimageError> {
    Err(MkimageError(msg.into()))
}

/// 整盘 MiB → 卷扇区数（= FAT 段扇区 − 分区起始 LBA）。造盘与自检共用
/// 同一推导——扩容只动 DEFAULT_SIZE_MIB / --size-mib，期望值随动不滞留。
pub fn volume_sectors(size_mib: usize) -> usize {
    size_mib * 1024 * 1024 / SECTOR - PART_LBA
}

/// LBA → 假几何 CHS 三字节（MBR 表项 5/6/9 位布局：H | S|C 高 2 位 |
/// C 低 8 位）。超 1023 柱面钳顶（protective 语义——LBA 盘上 CHS 仅装饰）。
pub fn lba_to_chs(lba: u64) -> [u8; 3] {
    let cyl = lba / (HEADS as u64 * SPT as u64);
    let r = lba % (HEADS as u64 * SPT as u64);
    let (h, s) = (r / SPT as u64, r % SPT as u64 + 1);
    let (cyl, h, s) = if cyl > 1023 {
        (1023u64, HEADS as u64 - 1, SPT as u64)
    } else {
        (cyl, h, s)
    };
    [h as u8, (s as u8) | ((cyl >> 8) as u8 & 0x03), (cyl & 0xFF) as u8]
}

/// MBR：引导代码区全零 + 表项 0（FAT16 主分区）+ 表项 1（密钥材料预留
/// 槽，件 4）+ 55AA。预留槽起始 = 主分区尾（单一事实源由调用几何推导，
/// 不另传参防错位）；表项 2-3 全零。
pub fn build_mbr(part_lba: usize, part_sectors: usize) -> [u8; SECTOR] {
    let mut mbr = [0u8; SECTOR];
    let mut e = [0u8; 16];
    e[0] = 0x00; // 非 active（UEFI 引导不看活动位）
    e[1..4].copy_from_slice(&lba_to_chs(part_lba as u64));
    e[4] = PART_TYPE_FAT16_LBA;
    e[5..8].copy_from_slice(&lba_to_chs((part_lba + part_sectors - 1) as u64));
    e[8..12].copy_from_slice(&(part_lba as u32).to_le_bytes());
    e[12..16].copy_from_slice(&(part_sectors as u32).to_le_bytes());
    mbr[446..462].copy_from_slice(&e);
    let meta_lba = part_lba + part_sectors;
    let mut m = [0u8; 16];
    m[0] = 0x00; // 同非 active
    m[1..4].copy_from_slice(&lba_to_chs(meta_lba as u64));
    m[4] = PART_TYPE_KEYMETA; // Non-FS data——预留不挂载语义
    m[5..8].copy_from_slice(&lba_to_chs((meta_lba + META_SECTORS - 1) as u64));
    m[8..12].copy_from_slice(&(meta_lba as u32).to_le_bytes());
    m[12..16].copy_from_slice(&(META_SECTORS as u32).to_le_bytes());
    mbr[462..478].copy_from_slice(&m);
    mbr[510..512].copy_from_slice(&[0x55, 0xAA]);
    mbr
}

/// SHA-256 小写 hex（64 位定宽）——零依赖自包含实现（FIPS 180-4，mkfat
/// selftest 同款）。manifest 件哈希（件 3）与整盘哈希（件 5 产链尾打印）
/// 共源。
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
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// 件 5：--key-hash 校验——恰 64 位 hex（SHA-256 锚），归一小写。坏值
/// loud 拒：真机产链烧错长度的公钥锚 = 未来验签面全线静默错。
pub fn parse_key_hash(s: &str) -> Result<String, MkimageError> {
    let v = s.trim().to_ascii_lowercase();
    let ok = v.len() == 64
        && v.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c));
    if !ok {
        return fail(format!("--key-hash 须 64 位 hex（SHA-256），got {s:?}"));
    }
    Ok(v)
}

/// G8：manifest 侧产物路径——<out.img> 同目录 + 名参数化。守卫 = 纯文件名
/// （非空、无路径分隔符、非 ./..）：`--manifest-name a/b.txt` 跨目录改写
/// = 命名面注入口，坏值 loud 拒。返回文件名（拼接归调用方）。
pub fn manifest_file_name(name: Option<&str>) -> Result<String, MkimageError> {
    let n = name.unwrap_or(MANIFEST_NAME);
    if n.is_empty() || n == "." || n == ".." || n.contains('/') || n.contains('\\') {
        return fail(format!(
            "--manifest-name 须纯文件名（非空、无路径分隔符——同目录侧产物语义），got {name:?}"
        ));
    }
    Ok(n.to_string())
}

/// 平台 API level 单源现值：tools/abi.toml `[platform].level`（V9 裁决
/// platform_revision ≡ level——manifest PR 列的唯一取值源）。行级小读器
/// （mkcpio abi_level 同款——数据单源在 abi.toml，各工具自携小读器；
/// 节外 level 不认，行内 # 注释剥）。
pub fn platform_level(root: &std::path::Path) -> Result<u64, MkimageError> {
    let path = root.join("tools").join("abi.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| MkimageError(format!("读 {}：{e}", path.display())))?;
    let mut in_platform = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_platform = l == "[platform]";
            continue;
        }
        if in_platform {
            if let Some(rest) = l.strip_prefix("level") {
                if let Some(v) = rest.trim().strip_prefix('=') {
                    let v = v.split('#').next().unwrap_or("").trim();
                    if let Ok(n) = v.parse::<u64>() {
                        return Ok(n);
                    }
                    return fail(format!(
                        "tools/abi.toml [platform].level 非整数：{v:?}（platform_revision 单源）"
                    ));
                }
            }
        }
    }
    fail("tools/abi.toml [platform].level 缺席（platform_revision 单源）")
}

/// 件 3：manifest 字节（v1 格式冻结于 2026-11-10——改任何列宽/行序/字段
/// = 破冻，须升 v2 并另立黄金字节对拍基准）。
///
/// 格式规约（逐字节冻结）：头行 `berryos-manifest v1 format_frozen=<日>
/// entries=NN`（两位零垫）；条目行五列（path 24 左对齐右补空 / sha256
/// 64 小写 / size 12 零垫 / rollback_index 20 零垫恒 0 /
/// platform_revision 10 零垫）；尾行 `key_hash=none|<64hex>`。行序 =
/// FAT 路径字节序（传参次序无关）；ASCII、LF 行尾、末行有换行。
/// 路径超宽 / PR 超 u32 域 loud 拒（冻结格式的执法面不能只剩固定输入的
/// 黄金字节）。
pub fn build_manifest(
    puts: &[(Vec<u8>, Vec<u8>)],
    key_hash: Option<&str>,
    platform_revision: u64,
) -> Result<Vec<u8>, MkimageError> {
    if platform_revision > u32::MAX as u64 {
        return fail(format!(
            "platform_revision 须 u32 域 [0, 4294967295]（列宽 {MANIFEST_PR_W} 契约），got {platform_revision}"
        ));
    }
    let mut sorted: Vec<&(Vec<u8>, Vec<u8>)> = puts.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0)); // 路径字节序（数据自定序）
    let mut out = String::new();
    out.push_str(&format!(
        "berryos-manifest v1 format_frozen={MANIFEST_FROZEN} entries={:02}\n",
        puts.len()
    ));
    for (path, data) in sorted {
        if path.len() > MANIFEST_PATH_W {
            return fail(format!(
                "manifest path 超冻结列宽 {MANIFEST_PATH_W}（{} = {} 字符）——破 v1 格式冻结；缩短路径或升 manifest v2",
                String::from_utf8_lossy(path),
                path.len()
            ));
        }
        let p = String::from_utf8_lossy(path);
        out.push_str(&format!(
            "item path={p:<MANIFEST_PATH_W$} sha256={} size={:0MANIFEST_SIZE_W$} \
             rollback_index={:0MANIFEST_RB_W$} platform_revision={:0MANIFEST_PR_W$}\n",
            sha256_hex(data),
            data.len(),
            0,
            platform_revision,
        ));
    }
    out.push_str(&format!(
        "key_hash={}\n",
        key_hash.unwrap_or("none")
    ));
    Ok(out.into_bytes())
}

/// 整盘装配（MBR + FAT16 主分区 + 尾部密钥材料全零区）。返回 (盘字节,
/// mkfat 几何摘要)。总盘 = size_mib MiB + 16MiB——FAT 段零漂移，预留区
/// 纯尾部追加（出生全零即预留区内容）。key_hash 有意不入参（件 5
/// 「不动盘字节」的数据流保证）。
pub fn build_image(
    puts: &[(Vec<u8>, Vec<u8>)],
    size_mib: usize,
) -> Result<(Vec<u8>, mkfat::Geo), MkimageError> {
    let vol_sectors = volume_sectors(size_mib);
    if vol_sectors <= 64 {
        return fail(format!(
            "{size_mib}MiB 装不下分区起始 {PART_LBA} + 卷"
        ));
    }
    let (vol, geo) = mkfat::build_volume_puts(puts, vol_sectors, true, PART_LBA as u32)?;
    let total = PART_LBA + vol_sectors + META_SECTORS;
    let mut img = vec![0u8; total * SECTOR];
    img[0..SECTOR].copy_from_slice(&build_mbr(PART_LBA, vol_sectors));
    img[PART_LBA * SECTOR..PART_LBA * SECTOR + vol.len()].copy_from_slice(&vol);
    Ok((img, geo))
}

/// P3 验签两件组装（纯函数）：几何校验（64B 裸签 / 32B 公钥）+
/// [(FAT 路径, 字节)×2]。行序由 build_manifest 自定序，返回序不构成契约。
pub fn sign_face_puts(
    sig: &[u8],
    pk: &[u8],
) -> Result<Vec<(Vec<u8>, Vec<u8>)>, MkimageError> {
    if sig.len() != SIGN_LEN {
        return fail(format!(
            "KERNEL.SIG 须恰 {SIGN_LEN}B 裸签（got {}B——berry-sign sign-raw stdout 契约）",
            sig.len()
        ));
    }
    if pk.len() != TRUSTPK_LEN {
        return fail(format!("TRUSTPK 须恰 {TRUSTPK_LEN}B（got {}B）", pk.len()));
    }
    Ok(vec![
        (SIGN_FAT_PATH.to_vec(), sig.to_vec()),
        (TRUSTPK_FAT_PATH.to_vec(), pk.to_vec()),
    ])
}

/// --trust-pk 读取 + 几何校验（恰 32B raw）。坏值 loud 拒：错长公钥锚
/// 进盘 = 设备侧签名件尺寸门静默降级的注入源。
pub fn read_trust_pk(path: &str) -> Result<Vec<u8>, MkimageError> {
    match std::fs::read(path) {
        Ok(blob) => {
            if blob.len() != TRUSTPK_LEN {
                fail(format!(
                    "--trust-pk 须恰 {TRUSTPK_LEN}B raw 公钥（got {}B：{path}）",
                    blob.len()
                ))
            } else {
                Ok(blob)
            }
        }
        Err(e) => fail(format!("读 {path}：{e}")),
    }
}

/// --source-instance 两件套读取：dir 内 source-index.tlv + source-keys.txt
/// → [(FAT 路径, 字节)×2]。缺席/非文件 loud 拒（TLV 单解释器纪律：索引
/// 解释器只在 gensourceindex 一处，本工具只搬运不解释）。
pub fn load_source_instance(si_dir: &str) -> Result<Vec<(Vec<u8>, Vec<u8>)>, MkimageError> {
    let d = std::path::Path::new(si_dir);
    if !d.is_dir() {
        return fail(format!(
            "--source-instance {si_dir} 非目录（两件套产出 = gensourceindex --out-dir）"
        ));
    }
    let mut puts = Vec::new();
    for (name, fat_path) in [
        (SRCINST_INDEX_NAME, SRCINST_FAT_INDEX),
        (SRCINST_KEYS_NAME, SRCINST_FAT_KEYS),
    ] {
        let p = d.join(name);
        if !p.is_file() {
            return fail(format!(
                "--source-instance 缺 {name}（两件套 = gensourceindex 产物；got {si_dir}）"
            ));
        }
        match std::fs::read(&p) {
            Ok(data) => puts.push((fat_path.to_vec(), data)),
            Err(e) => return fail(format!("读 {path}：{e}", path = p.display())),
        }
    }
    Ok(puts)
}

/// G0 刀 1：BOOTCTL 工厂默认件 52B（与 berry-slot BootControl::factory()
/// .to_bytes() 逐字节一致——slot crate 测试钉板对拍面）。布局：
/// magic BRYC @0 / version 3 @4 / slot A @8 / slot B @24 /
/// recovery_trigger @40 / boot_count @44 / crc32 @48。
pub fn bootctl_factory() -> [u8; 52] {
    let mut b = [0u8; 52];
    b[0..4].copy_from_slice(&0x4252_5943u32.to_le_bytes());
    b[4..8].copy_from_slice(&3u32.to_le_bytes());
    b[8] = 0; b[9] = 15; b[10] = 3; b[11] = 1; // A pri=15 tries=3 ok=1
    b[24] = 1; b[25] = 14; b[26] = 3; b[27] = 0; // B pri=14 tries=3 ok=0
    let crc = crc32_ieee(&b[..48]);
    b[48..52].copy_from_slice(&crc.to_le_bytes());
    b
}

/// IEEE CRC32（与 berry-slot::crc32 同算法——工厂件一致性钉板依赖）。
pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
