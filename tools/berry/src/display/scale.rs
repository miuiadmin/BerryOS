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
}
