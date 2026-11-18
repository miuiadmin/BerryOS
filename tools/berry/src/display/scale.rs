//! Geometry: guest frame -> window letterbox, and the inverse input transform.
//! One pure module, one set of tests; the window never duplicates this math.

use crate::display::frame::Frame;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Fit {
    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x + self.w
            && py < self.y + self.h
    }
}

/// Fit `src` into `dst` preserving aspect. `integer` prefers a whole-number
/// scale when it can (>=1); below 1 it falls back to fractional fit so the
/// image is never cropped.
pub fn fit(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32, integer: bool) -> Fit {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return Fit { x: 0, y: 0, w: 0, h: 0 };
    }
    let sx = dst_w as f64 / src_w as f64;
    let sy = dst_h as f64 / src_h as f64;
    let mut scale = sx.min(sy);
    if integer && scale >= 1.0 {
        scale = scale.floor();
    }
    let w = ((src_w as f64 * scale).round() as u32).min(dst_w).max(1);
    let h = ((src_h as f64 * scale).round() as u32).min(dst_h).max(1);
    Fit {
        x: (dst_w - w) / 2,
        y: (dst_h - h) / 2,
        w,
        h,
    }
}

/// Window pixel -> guest pixel, or `None` outside the image.
pub fn window_to_guest(px: u32, py: u32, fit: &Fit, guest_w: u32, guest_h: u32) -> Option<(u16, u16)> {
    if !fit.contains(px, py) || guest_w == 0 || guest_h == 0 {
        return None;
    }
    let gx = (px - fit.x) as u64 * guest_w as u64 / fit.w as u64;
    let gy = (py - fit.y) as u64 * guest_h as u64 / fit.h as u64;
    Some((
        gx.min(guest_w as u64 - 1) as u16,
        gy.min(guest_h as u64 - 1) as u16,
    ))
}

/// Scale `frame` into `out` (RGBA u32, `dst_w * dst_h`). Bilinear by default;
/// exactly 1:1 is a straight copy. The caller fills no background: this does.
pub fn render(frame: &Frame, fit: &Fit, dst_w: u32, dst_h: u32, out: &mut [u32], bg: u32) {
    let n = dst_w as usize * dst_h as usize;
    if out.len() < n {
        return;
    }
    out[..n].fill(bg);
    if fit.w == 0 || fit.h == 0 || frame.width == 0 || frame.height == 0 {
        return;
    }
    if fit.w == frame.width && fit.h == frame.height {
        for dy in 0..fit.h as usize {
            let src = dy * frame.width as usize;
            let dst = (fit.y as usize + dy) * dst_w as usize + fit.x as usize;
            out[dst..dst + fit.w as usize]
                .copy_from_slice(&frame.pixels[src..src + fit.w as usize]);
        }
        return;
    }
    let sw = frame.width as u64;
    let sh = frame.height as u64;
    let dw = fit.w as u64;
    let dh = fit.h as u64;
    let mut xs = Vec::with_capacity(fit.w as usize);
    for dx in 0..dw {
        let fx = dx * sw * 65536 / dw;
        let x0 = (fx >> 16) as usize;
        let x1 = (x0 + 1).min(sw as usize - 1);
        xs.push((x0, x1, (fx & 0xffff) as u32));
    }
    for dy in 0..dh {
        let fy = dy * sh * 65536 / dh;
        let y0 = (fy >> 16) as usize;
        let y1 = (y0 + 1).min(sh as usize - 1);
        let wy = (fy & 0xffff) as u32;
        let row0 = y0 * frame.width as usize;
        let row1 = y1 * frame.width as usize;
        let drow = (fit.y as usize + dy as usize) * dst_w as usize + fit.x as usize;
        for (dx, &(x0, x1, wx)) in xs.iter().enumerate() {
            let p00 = frame.pixels[row0 + x0];
            let p10 = frame.pixels[row0 + x1];
            let p01 = frame.pixels[row1 + x0];
            let p11 = frame.pixels[row1 + x1];
            let top = mix(p00, p10, wx);
            let bot = mix(p01, p11, wx);
            out[drow + dx] = mix(top, bot, wy);
        }
    }
}

#[inline]
fn mix(a: u32, b: u32, w: u32) -> u32 {
    let iw = 65536 - w;
    let r = (((a >> 16) & 0xff) * iw + ((b >> 16) & 0xff) * w) >> 16;
    let g = (((a >> 8) & 0xff) * iw + ((b >> 8) & 0xff) * w) >> 16;
    let bl = ((a & 0xff) * iw + (b & 0xff) * w) >> 16;
    (r << 16) | (g << 8) | bl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_phone() {
        let f = fit(800, 600, 1080, 2250, false);
        assert_eq!(f.w, 1080);
        assert_eq!(f.h, 810);
        assert_eq!(f.x, 0);
        assert_eq!(f.y, 720);
    }

    #[test]
    fn fit_integer_prefers_whole() {
        let f = fit(480, 1000, 1080, 2250, true);
        assert_eq!(f.w, 960);
        assert_eq!(f.h, 2000);
    }

    #[test]
    fn inverse_center() {
        let f = fit(800, 600, 1080, 2250, false);
        let (x, y) = window_to_guest(540, 1125, &f, 800, 600).unwrap();
        assert_eq!((x, y), (400, 300));
        assert!(window_to_guest(10, 10, &f, 800, 600).is_none());
    }

    #[test]
    fn render_2x2() {
        let mut fr = Frame::new(2, 2);
        fr.pixels = vec![0x00ff0000, 0x0000ff00, 0x000000ff, 0x00ffffff];
        let f = fit(2, 2, 4, 4, false);
        let mut out = vec![0; 16];
        render(&fr, &f, 4, 4, &mut out, 0);
        assert_eq!(out[0], 0x00ff0000);
        assert_eq!(out[3], 0x0000ff00);
        assert_eq!(out[12], 0x000000ff);
        assert_eq!(out[15], 0x00ffffff);
    }

    // ===== 九刀深测面（W1-Q25）：几何/逆变换/渲染契约 =====

    #[test]
    fn fit_zero_dims_all_reject() {
        // 任一维为 0 → 全零 Fit（宽高先判，除零面钉板）。
        for (sw, sh, dw, dh) in [
            (0, 600, 1080, 2250),
            (800, 0, 1080, 2250),
            (800, 600, 0, 2250),
            (800, 600, 1080, 0),
        ] {
            let f = fit(sw, sh, dw, dh, false);
            assert_eq!((f.x, f.y, f.w, f.h), (0, 0, 0, 0), "src=({sw},{sh}) dst=({dw},{dh})");
        }
    }

    #[test]
    fn fit_exact_same_and_upscale_identity() {
        // 同尺寸 → 1:1 无 letterbox；整数放大倍数 → integer 取整恰放大。
        let f = fit(1920, 1080, 1920, 1080, false);
        assert_eq!((f.x, f.y, f.w, f.h), (0, 0, 1920, 1080));
        let f = fit(960, 540, 1920, 1080, true);
        assert_eq!((f.w, f.h), (1920, 1080));
        assert_eq!((f.x, f.y), (0, 0));
    }

    #[test]
    fn fit_integer_below_one_falls_back_fractional() {
        // 缩小面（scale<1）integer 不裁剪：回退分数 fit，几何与 false 同值。
        let a = fit(1920, 1080, 960, 540, true);
        let b = fit(1920, 1080, 960, 540, false);
        assert_eq!(a, b);
        assert_eq!((a.w, a.h), (960, 540));
    }

    #[test]
    fn fit_integer_1x_floor_vs_fractional() {
        // 1 < scale < 2 面分岔：integer 地板到 1x，fractional 保分数。
        let i = fit(480, 320, 900, 900, true);
        assert_eq!((i.w, i.h), (480, 320));
        let fl = fit(480, 320, 900, 900, false);
        assert_eq!((fl.w, fl.h), (900, 600));
    }

    #[test]
    fn fit_invariant_aspect_and_bounds() {
        // 性质律（整像素量化界内）：交叉乘法 |w*sh − h*sw| ≤ sw+sh（round
        // 各引入 ≤0.5 像素，两测乘 sh/sw 放大）+ 结果恒按公式居中于 dst。
        for &(sw, sh, dw, dh) in &[
            (800, 600, 1080, 2250),
            (1920, 1080, 480, 800),
            (640, 480, 1280, 720),
            (1080, 2400, 720, 1600),
            (3, 5, 97, 43),
        ] {
            for &integer in &[false, true] {
                let f = fit(sw, sh, dw, dh, integer);
                if f.w == 0 {
                    continue;
                }
                let drift = (f.w as u64 * sh as u64).abs_diff(f.h as u64 * sw as u64);
                assert!(
                    drift <= (sw + sh) as u64,
                    "aspect drift {drift}: src=({sw},{sh}) dst=({dw},{dh}) integer={integer} fit={f:?}"
                );
                // 有界 + 居中公式钉板（x/y 正是地板除产物）。
                assert!(f.w <= dw && f.h <= dh);
                assert_eq!((dw - f.w) / 2, f.x);
                assert_eq!((dh - f.h) / 2, f.y);
            }
        }
        // min 尺度侧必贴满：x 与 y 不可能同时非零（letterbox 至多单轴）。
        let f = fit(800, 600, 1080, 2250, false);
        assert_eq!(f.x, 0, "宽侧贴满则无左右带");
        let f = fit(480, 1000, 1080, 2250, true);
        assert_eq!(f.y, (2250 - 2000) / 2, "高侧贴满则无上下带");
    }

    #[test]
    fn fit_extreme_dims_no_overflow() {
        // u32 极端几何：不 panic、不越界、结果有界。
        let f = fit(u32::MAX, 1, u32::MAX, 1, false);
        assert_eq!(f.w, u32::MAX);
        let f = fit(1, u32::MAX, 1, u32::MAX, false);
        assert_eq!(f.h, u32::MAX);
        let f = fit(2, 1, u32::MAX, 1, false);
        assert!(f.w >= 1 && f.h >= 1 && f.w <= u32::MAX && f.h <= 1);
    }

    #[test]
    fn window_to_guest_corners_and_clamp() {
        // 四角映射：fit 左上角 → guest(0,0)；右下角 → clamp 到 (gw-1, gh-1)。
        let f = fit(800, 600, 1080, 2250, false);
        assert_eq!(window_to_guest(f.x, f.y, &f, 800, 600), Some((0, 0)));
        // fit 内右下角 (x+w-1, y+h-1)：像素宽 w-1 映 guest 全宽比例，clamp 拦 u16 面。
        let px = f.x + f.w - 1;
        let py = f.y + f.h - 1;
        let (gx, gy) = window_to_guest(px, py, &f, 800, 600).unwrap();
        assert_eq!((gx, gy), (799, 599));
        // u16 边界：guest 65535 宽 clamp 面（gx = 65534 → min(65534, 65534)）。
        let f2 = fit(65535, 1, 65535, 1, false);
        assert_eq!(
            window_to_guest(65534, 0, &f2, 65535, 1),
            Some((65534, 0))
        );
    }

    #[test]
    fn window_to_guest_reject_faces() {
        // 拒面：零维 guest、fit 外四侧（x>0 与 y>0 不同时成立，宽受限 fit
        // 测左/右、高受限 fit 测上/下）。
        let f = fit(800, 600, 1080, 720, false);
        assert_eq!(window_to_guest(0, 0, &f, 0, 600), None);
        assert_eq!(window_to_guest(0, 0, &f, 800, 0), None);
        assert_eq!(window_to_guest(f.x + f.w, f.y, &f, 800, 600), None, "右外");
        assert_eq!(window_to_guest(f.x, f.y + f.h, &f, 800, 600), None, "下外");
        // 高受限面（y 带 > 0）：上外。
        let g = fit(800, 600, 1080, 2250, false);
        assert_eq!(window_to_guest(g.x, g.y - 1, &g, 800, 600), None, "上外");
        // 宽受限面（x 带 > 0）：左外。
        let h = fit(800, 600, 1920, 1080, false);
        assert!(h.x > 0, "该几何应有左右带");
        assert_eq!(window_to_guest(h.x - 1, h.y, &h, 800, 600), None, "左外");
        // 零尺寸 fit 恒 None。
        let z = Fit { x: 0, y: 0, w: 0, h: 0 };
        assert_eq!(window_to_guest(0, 0, &z, 800, 600), None);
    }

    #[test]
    fn window_to_guest_roundtrip_grid() {
        // 采样网格双向一致律：guest 均匀网格 → window 中心 → 回 guest 误差 ≤1 像素。
        let f = fit(800, 600, 1080, 2250, false);
        for i in 0..8 {
            for j in 0..6 {
                let gx = i * 100 + 50;
                let gy = j * 100 + 50;
                // guest → window（比例映射 + 居中偏移）。
                let px = f.x + gx * f.w as u64 as u32 / 800;
                let py = f.y + gy * f.h as u64 as u32 / 600;
                let back = window_to_guest(px, py, &f, 800, 600);
                let (bx, by) = back.unwrap();
                assert!(
                    (bx as i32 - gx as i32).abs() <= 1 && (by as i32 - gy as i32).abs() <= 1,
                    "roundtrip drift at guest({gx},{gy}): back=({bx},{by}) fit={f:?}"
                );
            }
        }
    }

    #[test]
    fn render_out_buffer_too_small_noop_fill() {
        // out 不足 n → 静默 return（不 panic 不局部写）；恰 n → 正常。
        let mut fr = Frame::new(2, 2);
        fr.pixels = vec![1, 2, 3, 4];
        let f = fit(2, 2, 4, 4, false);
        let mut small = vec![0xdeadbeef; 15];
        render(&fr, &f, 4, 4, &mut small, 0);
        assert!(small.iter().all(|&p| p == 0xdeadbeef), "不足 n 时不得写入");
        let mut ok = vec![0xdeadbeef; 16];
        render(&fr, &f, 4, 4, &mut ok, 0);
        assert_eq!(ok[0], 1, "恰 n 时正常渲染");
        assert_eq!(ok[15], 4);
    }

    #[test]
    fn render_zero_fit_bg_only() {
        // 零 fit 或零 frame → 纯背景填充（out[..n] 面全 bg，零像素写入）。
        let mut fr = Frame::new(2, 2);
        fr.pixels = vec![9; 4];
        let z = Fit { x: 0, y: 0, w: 0, h: 0 };
        let mut out = vec![0x12345678; 4];
        render(&fr, &z, 2, 2, &mut out, 0x0a0b0c0d);
        assert!(out.iter().all(|&p| p == 0x0a0b0c0d));
        let mut fr0 = Frame::new(0, 0);
        fr0.pixels = vec![];
        let f = fit(2, 2, 4, 4, false);
        let mut out2 = vec![0; 16];
        render(&fr0, &f, 4, 4, &mut out2, 0x0a0b0c0d);
        assert!(out2.iter().all(|&p| p == 0x0a0b0c0d));
        // 零 frame 且 fit.w==frame.width(0) 同样走纯 bg 路。
        let mut out3 = vec![0x1; 4];
        render(&fr0, &Fit { x: 0, y: 0, w: 0, h: 0 }, 2, 2, &mut out3, 0x0a0b0c0d);
        assert!(out3.iter().all(|&p| p == 0x0a0b0c0d));
    }

    #[test]
    fn render_letterbox_bands_pure_bg() {
        // letterbox 边带 = 纯背景：800×600（4:3）→ 3×4 画布，h=3 → 行 0/3
        // 为 bg 带，行 1-2 中列画面。中带像素因 8 位舍入与 0xff00ff 差 ≤1/通道。
        let mut fr = Frame::new(800, 600);
        fr.pixels = vec![0xff00ff; 800 * 600];
        let f = fit(800, 600, 3, 4, false);
        let mut out = vec![0; 12];
        render(&fr, &f, 3, 4, &mut out, 0);
        // 行 0 与行 3 全 bg（上下带；3 宽画布行界 = 0/3/6/9）。
        assert_eq!(out[0..3], [0; 3], "上带 bg");
        assert_eq!(out[9..12], [0; 3], "下带 bg");
        // 行 1-2 全行画面（w=dst_w 贴满，无左右带）。
        for &p in &out[3..9] {
            let is_img = (p >> 16) as i32 >= 254
                && ((p >> 8) & 0xff) as i32 <= 1
                && (p & 0xff) as i32 >= 254;
            assert!(is_img, "画面行非画面色: {p:#x}");
        }
    }

    #[test]
    fn render_upscale_bilinear_midpoint() {
        // 2×1 两色 → 4×2 放大：中列 = 双线性混色（mix 数学钉板）；右缘
        // x1 clamp 到 sw-1 = 边缘复制（契约面：无越界读，非混色）。
        let mut fr = Frame::new(2, 1);
        fr.pixels = vec![0xff0000, 0x0000ff];
        let f = fit(2, 1, 4, 2, false);
        let mut out = vec![0; 8];
        render(&fr, &f, 4, 2, &mut out, 0);
        // 列 0 = 左原色；列 3 = 右缘复制 0x0000ff（x1 clamp 契约）。
        assert_eq!(out[0], 0xff0000);
        assert_eq!(out[4], 0xff0000);
        assert_eq!(out[3], 0x0000ff, "右缘 = 边缘复制");
        assert_eq!(out[7], 0x0000ff);
        // 中列 1/2：fx=dx*32768——dx=1 wx=0x8000 恰 50:50 → 0x7f007f；
        // dx=2 wx=0 纯 x0=1 右色（定点采样契约，非全列混色）。
        assert_eq!(out[1], 0x7f007f, "dx=1 wx=0x8000 50:50 混色");
        assert_eq!(out[2], 0x0000ff, "dx=2 wx=0 纯右色");
        assert_eq!(out[5], 0x7f007f);
        assert_eq!(out[6], 0x0000ff);
        assert_eq!(((out[1] >> 16) & 0xff) + (out[1] & 0xff), 0xfe, "双 floor 量化 127+127");
    }
}
