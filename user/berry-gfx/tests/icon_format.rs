//! BYI1 线上格式组（段② `icon` 数据面）：[`decode`] 的**全拒面矩阵 +
//! 展开语义 + 边界接受**验收。私有编码助手 `byi1` 在测试侧手工组流
//! （宿主 mkapp 打包面的对偶——格式共识的两侧独立实现，wire 钉板）。
//!
//! 咬住的雷（矩阵逐格执法）：
//! - **零半写**：任何 `Err` 下 `out` 逐字节原样（哨兵填充 + 失败后复
//!   验——两遍解码的执法面，不是注释里的承诺）；
//! - **段流恰尽**：像素和超 → `PixelOverflow`、不足 → `Truncated`、
//!   恰等但有剩字节 → `TrailingBytes`（尾部夹带不吞）；
//! - **留位档**：BYR5/BYM1 走 `BadMagic` 拒译但 `from_magic` 可识别
//!   （「砍实现不砍协议位」的两层分层活证）；
//! - 边界接受 256×256 = 65536 像素 = 帽上合法（count 域 u16 ≤ 65535
//!   → 必须两段——单段装不下帽上图像本身就是格式事实）。
//!
//! 拒面次序（头导出的廉价拒绝先于段流走查）：magic → 头完整 →
//! version → flags → 几何 → out 容量 → 段流。

use berry_gfx::icon::{decode, IconError, IconFormat, ImageDesc};

/// A=0x80 的半透明红 / 纯蓝 / 纯绿——alpha 保留面的三常量。
const ARGB_HALF_RED: u32 = 0x8000_0080;
const BLUE: u32 = 0x0000_00FF;
const GREEN: u32 = 0x0000_FF00;

/// 测试侧手工编码：`byi1(w, h, segs, extra)`——segs 展开 + 尾部夹带
/// extra 字节（TrailingBytes 面的造源）。**不与 decode 共享任何代码**
/// （wire 共识要两侧独立）。
fn byi1(w: u16, h: u16, segs: &[(u16, u32)], extra: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"BYI1");
    v.push(1); // version
    v.push(0); // flags
    v.extend_from_slice(&w.to_le_bytes());
    v.extend_from_slice(&h.to_le_bytes());
    for (c, p) in segs {
        v.extend_from_slice(&c.to_le_bytes());
        v.extend_from_slice(&p.to_le_bytes());
    }
    v.extend_from_slice(extra);
    v
}

/// 改头（保段流）：直接改写 wire 的单字节（version/flags 面的造源）。
fn poke(v: &[u8], idx: usize, val: u8) -> Vec<u8> {
    let mut c = v.to_vec();
    c[idx] = val;
    c
}

/// magic 破坏（保其余全部）：换前 4 字节。
fn remagic(v: &[u8], m: &[u8; 4]) -> Vec<u8> {
    let mut c = v.to_vec();
    c[..4].copy_from_slice(m);
    c
}

// —— 展开语义：多段 RLE 按段序铺 + desc 回执 ——
#[test]
fn byi1_decodes_rle_in_order() {
    // 3×2，三段 (1,半透红)(2,蓝)(3,绿)——恰 6 像素。
    let wire = byi1(3, 2, &[(1, ARGB_HALF_RED), (2, BLUE), (3, GREEN)], b"");
    let mut out = [0xdead_beefu32; 6];
    assert_eq!(decode(&wire, &mut out), Ok(ImageDesc { w: 3, h: 2 }));
    assert_eq!(
        out,
        [ARGB_HALF_RED, BLUE, BLUE, GREEN, GREEN, GREEN],
        "段序展开：段内同色、段间按流序"
    );
    // alpha 位原样保留（解码不消费——A=0x80 逐位到 out）。
    assert_eq!(out[0] >> 24, 0x80, "A 位不丢不改");
}

// —— 单段满图 + 头十字节各字段（wire 钉板）——
#[test]
fn byi1_header_wire_layout_pinned() {
    // 2×2 单段 (4, 蓝)：手排 wire 的字节序（LE 逐字节钉板）。
    let wire = byi1(2, 2, &[(4, BLUE)], b"");
    assert_eq!(
        &wire[..10],
        &[b'B', b'Y', b'I', b'1', 1, 0, 2, 0, 2, 0],
        "magic 字面序 + version/flags + w/h 双 LE u16"
    );
    assert_eq!(&wire[10..], &[4, 0, 0xFF, 0, 0, 0], "count LE u16 + pixel LE u32");
    let mut out = [0u32; 4];
    assert_eq!(decode(&wire, &mut out), Ok(ImageDesc { w: 2, h: 2 }));
    assert_eq!(out, [BLUE; 4]);
    assert_eq!(ImageDesc { w: 2, h: 2 }.pixels(), 4);
}

// —— 边界接受：256×256 = 65536 像素（帽上合法；count 域 u16 → 必两段）——
#[test]
fn byi1_accepts_full_cap_256x256() {
    // 单段 count 上限 65535 < 65536 → 帽上图像结构性必须 ≥2 段。
    let wire = byi1(256, 256, &[(65535, BLUE), (1, GREEN)], b"");
    let mut out = vec![0u32; 65536];
    assert_eq!(decode(&wire, &mut out), Ok(ImageDesc { w: 256, h: 256 }));
    assert_eq!(out[0], BLUE, "首像素");
    assert_eq!(out[65534], BLUE, "末段前一像素");
    assert_eq!(out[65535], GREEN, "第二段的恰 1 像素落在最末");
    // 1×65535 与 65535×1：单维上界（帽内合法——两维各自 u16 全域；
    // n=65535 < 帽 → 单段可容）。
    for (w, h) in [(1u16, 65535u16), (65535, 1)] {
        let n = w as usize * h as usize;
        let wire = byi1(w, h, &[(n as u16, BLUE)], b"");
        let mut out = vec![0u32; n];
        assert_eq!(decode(&wire, &mut out), Ok(ImageDesc { w: w as u32, h: h as u32 }));
        assert_eq!(out[n - 1], BLUE, "单段末像素");
    }
}

// —— magic 面：错 magic / 短流 / 留位档（可识别 ≠ 可解码）——
#[test]
fn byi1_rejects_bad_magic() {
    let mut out = [0u32; 4];
    // 错 magic：got 随错携带。
    let bad = remagic(&byi1(2, 2, &[(4, BLUE)], b""), b"XYI1");
    assert_eq!(decode(&bad, &mut out), Err(IconError::BadMagic { got: *b"XYI1" }));
    // 留位档：from_magic 认得（协议位在）……
    assert_eq!(IconFormat::from_magic(*b"BYR5"), Some(IconFormat::Rgb565));
    assert_eq!(IconFormat::from_magic(*b"BYM1"), Some(IconFormat::Mono1bpp));
    // ……但 decode 拒译（BadMagic 携带原 magic——诊断能报出是哪档）。
    for m in [b"BYR5", b"BYM1"] {
        let wire = remagic(&byi1(2, 2, &[(4, BLUE)], b""), m);
        assert_eq!(decode(&wire, &mut out), Err(IconError::BadMagic { got: *m }));
    }
    // 短流：空 / 2B——连 magic 都读不满，零填充归 BadMagic。
    assert_eq!(decode(&[], &mut out), Err(IconError::BadMagic { got: [0; 4] }));
    assert_eq!(
        decode(b"BY", &mut out),
        Err(IconError::BadMagic { got: [b'B', b'Y', 0, 0] })
    );
    // 3B（差一字节满 magic）：同样零填充。
    assert_eq!(
        decode(b"BYI", &mut out),
        Err(IconError::BadMagic { got: [b'B', b'Y', b'I', 0] })
    );
    assert_eq!(out, [0u32; 4], "零半写：magic 拒面下 out 原样");
}

// —— 头拒面：截短 / version / flags ——
#[test]
fn byi1_rejects_header_faces() {
    let good = byi1(2, 2, &[(4, BLUE)], b"");
    let mut out = [0u32; 4];
    // 头不足 10B（magic 合法）：Truncated（9B——差 1 字节到 h）。
    let cut = &good[..9];
    assert_eq!(decode(cut, &mut out), Err(IconError::Truncated));
    // version = 2 / 0xFF。
    assert_eq!(
        decode(&poke(&good, 4, 2), &mut out),
        Err(IconError::UnsupportedVersion { got: 2 })
    );
    assert_eq!(
        decode(&poke(&good, 4, 0xFF), &mut out),
        Err(IconError::UnsupportedVersion { got: 0xFF })
    );
    // flags 非零（哪怕低位 1）。
    assert_eq!(
        decode(&poke(&good, 5, 1), &mut out),
        Err(IconError::NonZeroFlags { got: 1 })
    );
    assert_eq!(decode(&poke(&good, 5, 0x80), &mut out), Err(IconError::NonZeroFlags { got: 0x80 }));
    assert_eq!(out, [0u32; 4], "零半写：头拒面下 out 原样");
}

// —— 几何拒面：零宽 / 零高 / 乘积超帽（65536 帽——257×256 = 65792）——
//
// 注：契约例的 `w*h=65537` 在 u16 双维下不可构造（65537 为素数、因子
// 只有 1×65537 超 u16 域）——乘积超帽面用最小可构造超帽对 257×256。
#[test]
fn byi1_rejects_bad_dimensions() {
    let mut out = vec![0u32; 65792]; // 容量给足：证明拒因是几何不是 out
    assert_eq!(
        decode(&byi1(0, 2, &[(4, BLUE)], b""), &mut out),
        Err(IconError::BadDimensions { w: 0, h: 2 }),
        "零宽"
    );
    assert_eq!(
        decode(&byi1(2, 0, &[(4, BLUE)], b""), &mut out),
        Err(IconError::BadDimensions { w: 2, h: 0 }),
        "零高"
    );
    assert_eq!(
        decode(&byi1(257, 256, &[(65535, BLUE), (257, BLUE)], b""), &mut out),
        Err(IconError::BadDimensions { w: 257, h: 256 }),
        "乘积 65792 > 65536 帽（两维各自合法也拒）"
    );
    assert_eq!(
        decode(&byi1(65535, 65535, &[(65535, BLUE)], b""), &mut out),
        Err(IconError::BadDimensions { w: 65535, h: 65535 }),
        "双维上界乘积巨超帽"
    );
    assert!(out.iter().all(|&p| p == 0), "零半写：几何拒面下 out 原样");
}

// —— 段流拒面：ZeroCount / Truncated 三形 / PixelOverflow / TrailingBytes ——
#[test]
fn byi1_rejects_segment_stream_faces() {
    let mut out = [0u32; 4];
    // ZeroCount：饥饿态零长段即拒（段内 count=0 非法——RLE 无空段语义）。
    assert_eq!(
        decode(&byi1(2, 2, &[(0, BLUE)], b""), &mut out),
        Err(IconError::ZeroCount)
    );
    assert_eq!(
        decode(&byi1(2, 2, &[(2, BLUE), (0, GREEN)], b""), &mut out),
        Err(IconError::ZeroCount),
        "未铺满时撞零长段：walk 仍在饥饿态，按 ZeroCount 拒"
    );
    // 铺满后的零长 padding 段 = 尾巴（walk 在 total==want 处停——其后
    // 的一切字节无论是否长得像段，统一按 TrailingBytes 计数拒；「满图
    // 后的段流结构」不是 decode 的诊断职责，多余字节数才是）。
    assert_eq!(
        decode(&byi1(2, 2, &[(4, BLUE), (0, GREEN)], b""), &mut out),
        Err(IconError::TrailingBytes { extra: 6 }),
        "铺满后零长段 = 夹带（extra=6 计段全长）"
    );
    // Truncated 形①：段被斩尾（count 在、像素缺 2B）。
    let chopped = byi1(2, 2, &[(4, BLUE)], b"");
    let chopped = chopped[..chopped.len() - 2].to_vec();
    assert_eq!(decode(&chopped, &mut out), Err(IconError::Truncated));
    // Truncated 形②：像素和不足（1 段 1 像素 vs 需要 4）。
    assert_eq!(
        decode(&byi1(2, 2, &[(1, BLUE)], b""), &mut out),
        Err(IconError::Truncated)
    );
    // Truncated 形③：恰好满图但段流中腰斩（第 2 段只到 count）。
    let midcut = byi1(2, 2, &[(2, BLUE), (2, GREEN)], b"");
    let midcut = midcut[..midcut.len() - 4].to_vec(); // 掉第 2 段的像素
    assert_eq!(decode(&midcut, &mut out), Err(IconError::Truncated));
    // PixelOverflow：段和 5 > 4（want/got 随错携带）。
    assert_eq!(
        decode(&byi1(2, 2, &[(3, BLUE), (2, GREEN)], b""), &mut out),
        Err(IconError::PixelOverflow { want: 4, got: 5 })
    );
    // TrailingBytes：恰满 + 夹带（半字节碎尾 / 整段尾两形）。
    assert_eq!(
        decode(&byi1(2, 2, &[(4, BLUE)], &[0xAA]), &mut out),
        Err(IconError::TrailingBytes { extra: 1 })
    );
    assert_eq!(
        decode(&byi1(2, 2, &[(4, BLUE), (1, GREEN)], b""), &mut out),
        Err(IconError::TrailingBytes { extra: 6 }),
        "已满图后的完整段 = 夹带（extra=6 计段全长）"
    );
    assert_eq!(out, [0u32; 4], "零半写：段流拒面下 out 原样");
}

// —— out 容量拒面：need/got 随错携带 + 拒面次序（头导出先于段流）——
#[test]
fn byi1_rejects_short_out_before_stream_walk() {
    // 段流本身也坏（PixelOverflow 形态在场）——但 out 短是头导出的
    // 廉价拒绝，必须先报：拒面次序是诊断契约（报 BufferTooSmall 而非
    // PixelOverflow）。
    let wire = byi1(2, 2, &[(3, BLUE), (9, GREEN)], b"");
    let mut out = [0u32; 3];
    assert_eq!(
        decode(&wire, &mut out),
        Err(IconError::BufferTooSmall { need: 4, got: 3 })
    );
    assert_eq!(out, [0u32; 3], "零半写：容量拒面下 out 原样");
}

// —— 零半写哨兵总验：全拒面走一遍，out 全域哨兵逐字节不动 ——
#[test]
fn byi1_every_error_leaves_out_untouched() {
    let good = byi1(2, 2, &[(4, BLUE)], b"");
    let cases: Vec<Vec<u8>> = vec![
        remagic(&good, b"XYI1"),                                   // BadMagic
        remagic(&good, b"BYR5"),                                   // 留位档
        poke(&good, 4, 2),                                         // version
        poke(&good, 5, 1),                                         // flags
        byi1(0, 2, &[(4, BLUE)], b""),                             // 零宽
        byi1(257, 256, &[(65535, BLUE), (257, BLUE)], b""),        // 超帽
        byi1(2, 2, &[(0, BLUE)], b""),                             // ZeroCount
        good[..9].to_vec(),                                        // 头截短
        byi1(2, 2, &[(1, BLUE)], b""),                             // 和不足
        byi1(2, 2, &[(3, BLUE), (2, GREEN)], b""),                 // 和超
        byi1(2, 2, &[(4, BLUE)], &[0xEE]),                         // 夹带
    ];
    for (i, wire) in cases.iter().enumerate() {
        let mut out = [0x5555_5555u32; 4];
        let r = decode(wire, &mut out);
        assert!(r.is_err(), "case {i} 须拒");
        assert_eq!(out, [0x5555_5555u32; 4], "case {i}：Err 下 out 逐字节原样");
    }
}

// —— 端到端：BYI1 wire → decode → DrawImage(Argb) → 像素断言 ——
//
// 段② 资产管线的闭环验收：解码产物经 `cmd::ImageSrc::Argb` 绘制消费
// （同序直用零转换——decode 的 out 即 Argb 的 px）；A 位 1-bit 透明键
// 在绘制面执法（数据面只保证 A 原样保留，两半各司其职）。
#[test]
fn byi1_to_draw_image_end_to_end() {
    use berry_gfx::cmd::{Color, CommandEncoder, ImageSrc};
    use berry_gfx::color::{TextureDesc, TextureFormat};
    use berry_gfx::pass::PassDescriptor;
    use berry_gfx::queue::Queue;

    // 3×2，四段恰 6 像素：[透明, 半透绿, 蓝, 纯绿, 蓝, 蓝]——半透绿
    // （A=0x80, RGB=绿）与纯绿渲染必须同值（A 只做非零键、无 blend）。
    // 注意：本文件既有 BLUE/GREEN 常量的 A 位 = 0（decode 面不消费
    // alpha 故随手定义）——1-bit 键下是透明像素，此处局部带 A=0xFF。
    let wire = byi1(
        3,
        2,
        &[(1, 0x0000_0000), (1, 0x8000_FF00), (3, 0xFF00_00FF), (1, 0xFF00_FF00)],
        b"",
    );
    let mut out = [0u32; 6];
    let desc = decode(&wire, &mut out).unwrap();
    assert_eq!((desc.w, desc.h), (3, 2));

    // 画到 16×16 纹理 (2,3)（最小纹理几何 8×16——蜂格下界；四周留
    // 对照位证明区外未触）。
    let d = TextureDesc::new(16, 16, 64, TextureFormat::Rgbx).unwrap();
    let mut fb = vec![0xEEu8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_image(
        ImageSrc::Argb { w: desc.w, h: desc.h, px: &out },
        2,
        3,
        Color { r: 255, g: 255, b: 255 }, // fg 不适用面——被忽略
        None,
    )
    .unwrap();
    let dl = enc.finish();
    Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();

    let rd = |x: usize, y: usize| {
        let o = y * 64 + x * 4;
        u32::from_le_bytes(fb[o..o + 4].try_into().unwrap())
    };
    assert_eq!(rd(2, 3), 0xEEEE_EEEE, "A=0 透明键：底穿透");
    assert_eq!(rd(3, 3), 0x0000_FF00, "半透绿 → 不透明绿（A 只做非零键，无 blend）");
    assert_eq!(rd(4, 3), 0x0000_00FF, "蓝");
    assert_eq!(rd(2, 4), 0x0000_00FF, "蓝（段序展开：第 3 段跨行延续）");
    assert_eq!(rd(3, 4), 0x0000_00FF);
    assert_eq!(rd(4, 4), 0x0000_FF00, "纯绿——与半透绿同值（no-blend 对照）");
    assert_eq!(rd(1, 3), 0xEEEE_EEEE, "图像区外未触（左）");
    assert_eq!(rd(2, 2), 0xEEEE_EEEE, "图像区外未触（上）");
    assert_eq!(rd(5, 4), 0xEEEE_EEEE, "图像区外未触（右）");
    assert_eq!(rd(2, 5), 0xEEEE_EEEE, "图像区外未触（下）");
}
