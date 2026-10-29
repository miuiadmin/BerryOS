//! 图标（段②：图标格式 + LRU 图集——`docs/GUI里程碑规划-2026-10.md`
//! GUI-2 行「图标格式定义 + ARGB 解码 + LRU 缓存」三件全落）。三件分
//! 两半：
//!
//! - **数据面**（[`IconFormat`]/[`ImageDesc`]/[`decode`]，本模块下半）：
//!   BYI1 线上格式解码——**裁决 B**（自定义紧凑格式；PNG→ARGB blob/
//!   RLE 打包在宿主 mkapp 侧完成，guest 只吃定长小头 + RLE 段流）。
//!   alpha 位**原样保留**（解码不消费——v1 无 blend 铁律；绘制面消费
//!   与否是后端/后续刀决策，此处只保证不丢不猜）；
//! - **图集面**（[`IconAtlas`]/[`IconLruCache`]，本模块上半）：图标 v1
//!   绘制面 = **任意几何单色位图**（[`GlyphBits`] 同一位法：行主序、
//!   每行 `(w+7)/8` 字节、MSB = 最左列）——与字形共用光栅核
//!   `raster::mono_blit`，绘制走 `cmd::Cmd::DrawImage`（fg 着墨位 /
//!   bg 零位或透明）。
//!
//! 两半的接驳（图标 id → BYI1 blob → `decode` → LRU 装载）是**消费侧
//! 组合编排**，不进本仓（GUI-4/7——no_std 下解码缓冲归属是消费方决策，
//! 见 `crate::lru` 头注）。ARGB 像素的绘制消费口 = `cmd::ImageSrc::Argb`
//! （decode 产物同序直用，`raster::argb_blit` 执行——A 位 1-bit 透明
//! 键：`A==0` 跳过 / `A!=0` 不透明覆盖，无 blend，v1 铁律不破）。
//!
//! 图集两件：
//! - [`IconAtlas`] trait：id → 位图的查询面（`&self` 口——LRU 碰热经
//!   内部可变性，见 [`crate::lru`]）；
//! - [`IconLruCache`]：定容 N 槽 LRU 实现（stamp-clock 逐出），miss
//!   由调用方自行装载（`put` 后 `get`——本类型不做 miss 回调，回调面
//!   留给真数据源接入时再定形）。
//!
//! 缺图标语义 = **整命令 no-op**（`DrawImage` 对 `icon()==None` 不画，
//! v1 无占位框/缺省图——稀疏表是合法形态，宁缺勿猜；几何错才是拒面，
//! 见 queue.rs `ImagePixelMismatch`）。

use crate::font::GlyphBits;
use crate::lru::LruCache;

/// 图标 id（v1 = 调用方自定义编号面 0..=65535；新类型防与裸整数混用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconId(pub u16);

/// 图标图集查询面。实现持有图标数据（静态表/LRU 缓存均可）。
pub trait IconAtlas {
    /// 查图标位图。缺图标返 `None`（→ `DrawImage` no-op，见模块头）。
    ///
    /// 实现契约：返回的 `GlyphBits` 几何须自洽（权威构造口
    /// `GlyphBits::new` 或生成器产物；畸形位图经 `bit()` 防御采样
    /// 不 panic——缺位恒 false）。
    fn icon(&self, id: IconId) -> Option<GlyphBits<'static>>;
}

/// LRU 图标图集（定容 N 槽；stamp-clock 逐出——`crate::lru` 核）。
///
/// 装载面（`&self` 内部可变性）：`put` 装载/覆写、`get` 查询碰热；
/// 查询同时经 [`IconAtlas`] trait 面（`&dyn` 消费——命令内嵌共享引用，
/// 回放期查询即碰热，无需 `&mut`）。
#[derive(Debug, Clone)]
pub struct IconLruCache<const N: usize> {
    lru: LruCache<N, IconId, GlyphBits<'static>>,
}

impl<const N: usize> IconLruCache<N> {
    /// 构造空图集。
    pub fn new() -> IconLruCache<N> {
        IconLruCache { lru: LruCache::new() }
    }

    /// 容量（槽位数）。
    pub fn capacity(&self) -> usize {
        self.lru.capacity()
    }

    /// 在槽图标数。
    pub fn len(&self) -> usize {
        self.lru.len()
    }

    /// 空谓词。
    pub fn is_empty(&self) -> bool {
        self.lru.is_empty()
    }

    /// 装载/覆写图标。返被逐出的旧图标（满表逐出最久未用；`None` =
    /// 有空槽或同 id 覆写）。
    pub fn put(&self, id: IconId, bits: GlyphBits<'static>) -> Option<(IconId, GlyphBits<'static>)> {
        self.lru.put(id, bits)
    }

    /// 查询 + 碰热（与 [`IconAtlas::icon`] 同效——语义面便捷口）。
    pub fn get(&self, id: &IconId) -> Option<GlyphBits<'static>> {
        self.lru.get(id)
    }

    /// 清空。
    pub fn clear(&self) {
        self.lru.clear();
    }
}

impl<const N: usize> Default for IconLruCache<N> {
    fn default() -> Self {
        IconLruCache::new()
    }
}

impl<const N: usize> IconAtlas for IconLruCache<N> {
    fn icon(&self, id: IconId) -> Option<GlyphBits<'static>> {
        self.lru.get(&id)
    }
}

// ---------------------------------------------------------------------------
// BYI1 线上格式（段② 数据面）：宿主打包 ↔ guest 解码。
// ---------------------------------------------------------------------------

/// 图标线上格式档位（wire magic 面——裁决 B）。v1 只实现
/// [`IconFormat::Byi1`] 的解码；RGB565/1bpp 两槽**砍实现不砍协议位**
/// （[`IconFormat::from_magic`] 可识别——真机容量/带宽面定型后再定档，
/// 新档入册不改 BYI1 语义，老流不重打包）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconFormat {
    /// ARGB8888 像素 RLE（magic `"BYI1"`——v1 唯一已实现档）。
    Byi1,
    /// RGB565 像素（magic `"BYR5"`——协议留位档，解码未实现）。
    Rgb565,
    /// 1bpp 单色（magic `"BYM1"`——协议留位档，解码未实现）。
    Mono1bpp,
}

impl IconFormat {
    /// 四字节 wire magic（字节序即字面序——magic 是标签不是整数，无
    /// LE/BE 之分）。
    pub const fn magic(self) -> [u8; 4] {
        match self {
            IconFormat::Byi1 => *b"BYI1",
            IconFormat::Rgb565 => *b"BYR5",
            IconFormat::Mono1bpp => *b"BYM1",
        }
    }

    /// magic → 档位。三档全识别（留位档也返 `Some`——「砍实现不砍协议
    /// 位」：诊断面能报「这是哪一档」，拒译发生在 [`decode`]（留位档
    /// 同走 `BadMagic`——**格式可识别 ≠ 可解码**，两者分层））。
    pub const fn from_magic(m: [u8; 4]) -> Option<IconFormat> {
        // 逐字节比对（const 语境无数组 PartialEq 面）。
        if m[0] == b'B' && m[1] == b'Y' && m[2] == b'I' && m[3] == b'1' {
            Some(IconFormat::Byi1)
        } else if m[0] == b'B' && m[1] == b'Y' && m[2] == b'R' && m[3] == b'5' {
            Some(IconFormat::Rgb565)
        } else if m[0] == b'B' && m[1] == b'Y' && m[2] == b'M' && m[3] == b'1' {
            Some(IconFormat::Mono1bpp)
        } else {
            None
        }
    }
}

/// 解码产物几何（成功面的轻量回执——像素本体已直写 `out`，本结构只回
/// 尺寸与像素数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDesc {
    /// 像素宽（线格式域 1..=65535，且 `w*h ≤ 65536`）。
    pub w: u32,
    /// 像素高（同域同帽）。
    pub h: u32,
}

impl ImageDesc {
    /// 像素总数（= 解码所需 `out` 长度的恰等值——`BufferTooSmall` 的
    /// `need` 同一口径）。
    pub const fn pixels(self) -> u32 {
        self.w * self.h // 两参先验 ≤ 65536，u32 乘法无溢出面
    }
}

/// BYI1 解码拒面（全 loud 值级错误）。**零半写承诺**：任何 `Err` 下
/// `out` 逐字节原样（两遍解码——走查全过才落笔，见 [`decode`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconError {
    /// magic 不识别。含留位档 BYR5/BYM1（格式可识别但解码未实现，同
    /// 拒）；`got` 随错携带供诊断行直打；短于 4B 的输入零填充后归此面
    /// （连 magic 都读不满 = 流的根本不是图标格式）。
    BadMagic { got: [u8; 4] },
    /// version ≠ 1（v1 只认 1——未来版本走新档位而非静默猜旧义）。
    UnsupportedVersion { got: u8 },
    /// flags ≠ 0（保留位非零——语义未定即拒，不猜）。
    NonZeroFlags { got: u8 },
    /// 几何非法：零宽/零高，或 `w*h` 超 64Ki 像素帽（65536——解码界内
    /// 产物的先验上界；两维各自合法而乘积超帽同样在此面）。
    BadDimensions { w: u32, h: u32 },
    /// RLE 段 count = 0（零长段 = 纯 padding——语义未定即拒，不吞）。
    ZeroCount,
    /// 字节流不完整：头不足 10B / 段不足 6B / 像素和不足 `w*h`。
    Truncated,
    /// 像素和超 `w*h`（RLE 展开多于声明几何——`want` = 声明值，
    /// `got` = 越界时的累计值）。
    PixelOverflow { want: usize, got: usize },
    /// 像素和恰等但字节有剩（`extra` = 剩余字节数——尾部夹带即拒，
    /// 不静默吞）。
    TrailingBytes { extra: usize },
    /// `out` 长度 < `w*h`（need/got 随错携带——与 `color::DescError::
    /// BufferTooSmall` 同形诊断）。
    BufferTooSmall { need: usize, got: usize },
}

/// BYI1 解码（段② 数据面核）。wire 布局（全 LE）：
///
/// ```text
/// 偏移  长度  字段
/// 0     4     magic "BYI1"
/// 4     1     version（=1）
/// 5     1     flags（=0 保留）
/// 6     2     width  LE u16（≥1）
/// 8     2     height LE u16（≥1）
/// 10    ..    RLE 段 ×N：count LE u16（≥1）+ pixel LE u32（ARGB8888）
/// ```
///
/// 约束：`Σcount == w*h`、`w*h ≤ 65536`、段流恰尽（不多不少一字节）。
/// 像素按段序展开直写 `out`（ARGB 原样——A 位不消费不丢弃）。
///
/// **两遍解码**（零半写承诺的执法形态）：第一遍纯走查（头 + 段流的
/// 全部拒面），全过才第二遍落笔——`Err` 下 `out` 逐字节原样。
/// 拒面次序：magic → 头完整性 → version → flags → 几何 → `out`
/// 容量（头导出的廉价拒绝先于段流走查）→ 段流。
pub fn decode(buf: &[u8], out: &mut [u32]) -> Result<ImageDesc, IconError> {
    // —— magic：短于 4B 零填充比对（get 而非直索引——短流不 panic）。
    let mut magic = [0u8; 4];
    for i in 0..4 {
        magic[i] = buf.get(i).copied().unwrap_or(0);
    }
    if magic != IconFormat::Byi1.magic() {
        return Err(IconError::BadMagic { got: magic });
    }
    if buf.len() < 10 {
        return Err(IconError::Truncated);
    }
    let version = buf[4];
    if version != 1 {
        return Err(IconError::UnsupportedVersion { got: version });
    }
    let flags = buf[5];
    if flags != 0 {
        return Err(IconError::NonZeroFlags { got: flags });
    }
    let w = u32::from(u16::from_le_bytes([buf[6], buf[7]]));
    let h = u32::from(u16::from_le_bytes([buf[8], buf[9]]));
    // 乘法域：两参 ≤ 65535，u32 乘积无溢出（65535² < 2^32）。
    if w == 0 || h == 0 || w * h > 65536 {
        return Err(IconError::BadDimensions { w, h });
    }
    let want = (w * h) as usize;
    if out.len() < want {
        return Err(IconError::BufferTooSmall { need: want, got: out.len() });
    }
    // —— 第一遍：段流走查（像素和恰等 w*h 且字节恰尽）——
    // 不变量：i 每轮 +6（段长），total 每轮 +count ≥ 1（ZeroCount 先
    // 拒）——循环必终；i ≤ buf.len() 恒成立（余量 ≥ 6 才前进）。
    let mut total = 0usize;
    let mut i = 10usize;
    while total < want {
        if buf.len() - i < 6 {
            return Err(IconError::Truncated); // 流耗尽 / 段不完整
        }
        let count = u16::from_le_bytes([buf[i], buf[i + 1]]) as usize;
        if count == 0 {
            return Err(IconError::ZeroCount);
        }
        // checked 形态防守（域上 total+count ≤ 65536+65535，64 位上
        // 结构性不可溢——保持 checked 是纪律不是补救）。
        total = total.checked_add(count).unwrap_or(usize::MAX);
        if total > want {
            return Err(IconError::PixelOverflow { want, got: total });
        }
        i += 6;
    }
    if i != buf.len() {
        return Err(IconError::TrailingBytes { extra: buf.len() - i });
    }
    // —— 第二遍：落像素（几何/容量/字节长已全验证——pos 恒 < want ≤
    //    out.len()，try_into 恒 4B 不败）——
    let mut pos = 0usize;
    let mut i = 10usize;
    while i < buf.len() {
        let count = u16::from_le_bytes([buf[i], buf[i + 1]]) as usize;
        let px = u32::from_le_bytes(buf[i + 2..i + 6].try_into().unwrap());
        for _ in 0..count {
            out[pos] = px;
            pos += 1;
        }
        i += 6;
    }
    debug_assert_eq!(pos, want, "第一遍验证过的段流，第二遍必然恰等");
    Ok(ImageDesc { w, h })
}

#[cfg(test)]
mod tests {
    use super::*;

    // —— 装载/查询/trait 面 + LRU 逐出（核行为经图标语义面复验）——
    #[test]
    fn icon_lru_put_get_trait_and_eviction() {
        let c: IconLruCache<2> = IconLruCache::new();
        assert!(c.is_empty() && c.capacity() == 2);
        let a = GlyphBits::new(2, 2, &[0b1100_0000, 0b1100_0000]).unwrap();
        let b = GlyphBits::new(1, 1, &[0b1000_0000]).unwrap();
        let d = GlyphBits::new(4, 1, &[0b1111_0000]).unwrap();
        assert_eq!(c.put(IconId(1), a), None, "空槽");
        assert_eq!(c.put(IconId(2), b), None);
        assert_eq!(c.len(), 2);
        // trait 面查询（&dyn IconAtlas——命令消费路径）碰热同效。
        let atlas: &dyn IconAtlas = &c;
        let got = atlas.icon(IconId(1)).unwrap();
        assert_eq!((got.w, got.h, got.rows), (2, 2, &[0b1100_0000, 0b1100_0000][..]));
        assert!(atlas.icon(IconId(9)).is_none(), "缺图标 None（→ DrawImage no-op）");
        // id1 已碰热 → 满表插入逐出 id2。
        let ev = c.put(IconId(3), d).unwrap();
        assert_eq!(ev.0, IconId(2));
        assert!(c.get(&IconId(2)).is_none());
        assert!(c.get(&IconId(1)).is_some());
        assert!(c.get(&IconId(3)).is_some());
        // 同 id 覆写不逐出。
        let a2 = GlyphBits::new(3, 1, &[0b1110_0000]).unwrap();
        assert_eq!(c.put(IconId(3), a2), None);
        assert_eq!(c.len(), 2);
        assert_eq!(c.get(&IconId(3)).unwrap().w, 3, "覆写后取新位图");
        c.clear();
        assert!(c.is_empty());
    }

    // —— 位图采样辅助自检（图标位法与字形同一条：MSB=最左列）——
    #[test]
    fn helper_bits_shape() {
        let g = GlyphBits::new(8, 2, &[0xFF, 0x0F]).unwrap();
        assert!(g.bit(0, 0) && g.bit(7, 0));
        // 0x0F = 右半字节：左 4 列（含 x=3）off、右 4 列（x=4..8）on。
        assert!(!g.bit(3, 1) && g.bit(4, 1));
    }

    // —— IconFormat magic 双射 + 留位档识别（砍实现不砍协议位）——
    #[test]
    fn icon_format_magic_bijection() {
        for f in [IconFormat::Byi1, IconFormat::Rgb565, IconFormat::Mono1bpp] {
            assert_eq!(IconFormat::from_magic(f.magic()), Some(f), "{f:?}");
        }
        assert_eq!(IconFormat::from_magic(*b"XXXX"), None, "非图标流");
        assert_eq!(IconFormat::from_magic([0; 4]), None, "空流");
        assert_eq!(IconFormat::from_magic(*b"BYI2"), None, "邻号不留情");
    }

    // —— BYI1 解码烟测（完整拒面矩阵在 tests/icon_format.rs 集成面）——
    #[test]
    fn byi1_decode_smoke() {
        // 2×2：两段 (1, 不透红)(3, 蓝) → 展开 [红, 蓝, 蓝, 蓝]。
        let mut wire = Vec::new();
        wire.extend_from_slice(b"BYI1");
        wire.extend_from_slice(&[1, 0]); // version=1 flags=0
        wire.extend_from_slice(&2u16.to_le_bytes());
        wire.extend_from_slice(&2u16.to_le_bytes());
        wire.extend_from_slice(&1u16.to_le_bytes());
        wire.extend_from_slice(&0xFF00_0000u32.to_le_bytes()); // A 位原样保留
        wire.extend_from_slice(&3u16.to_le_bytes());
        wire.extend_from_slice(&0x0000_00FFu32.to_le_bytes());
        let mut out = [0u32; 4];
        assert_eq!(decode(&wire, &mut out), Ok(ImageDesc { w: 2, h: 2 }));
        assert_eq!(out, [0xFF00_0000, 0x0000_00FF, 0x0000_00FF, 0x0000_00FF]);
        assert_eq!(ImageDesc { w: 2, h: 2 }.pixels(), 4);
        // 头导出的廉价拒面先于段流走查：out 短 → BufferTooSmall。
        let mut short = [0u32; 3];
        assert_eq!(
            decode(&wire, &mut short),
            Err(IconError::BufferTooSmall { need: 4, got: 3 })
        );
        assert_eq!(short, [0; 3], "零半写：拒面下 out 原样");
    }
}
