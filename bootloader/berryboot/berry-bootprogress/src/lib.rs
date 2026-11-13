//! BerryOS boot progress truth model and native renderer.
//!
//! This crate is deliberately free of EFI, kernel, allocator and service
//! dependencies. The same truth model can therefore drive BerryBoot's early
//! GOP/ramfb surface, a kernel diagnostic surface and a future system boot
//! animation service without forking the progress semantics.
//!
//! Progress is represented in permille (0..=1000). Producers map real byte or
//! stage counters through [`span`]; they never animate by wall-clock alone.
//! The renderer is incremental: initialization clears the owned framebuffer
//! once, while updates only repaint the bar extension and the two small text
//! regions.

#![cfg_attr(not(test), no_std)]

use berry_fbcon::font;
use berry_fbcon::render::PixelFormat;
use core::cmp::{max, min};

pub const PERMILLE_MAX: u16 = 1000;

/// Real boot stages represented on the first-screen surface.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Display,
    Verify,
    Kernel,
    Ramdisk,
    Prepare,
    Handoff,
}

impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Display => "Starting display",
            Self::Verify => "Verifying boot disk",
            Self::Kernel => "Loading kernel",
            Self::Ramdisk => "Loading system",
            Self::Prepare => "Preparing BerryOS",
            Self::Handoff => "Starting BerryOS",
        }
    }
}

/// Map a real counter to a permille interval. `done > total`, zero total, or
/// an impossible interval returns `None`; callers fail loud rather than invent
/// progress. The result is clamped to the interval, not to the global maximum.
pub fn span(start: u16, end: u16, done: u64, total: u64) -> Option<u16> {
    if end > PERMILLE_MAX || start > end || total == 0 || done > total {
        return None;
    }
    let width = u32::from(end - start);
    let value = u32::from(start)
        .checked_add((width as u64 * done / total) as u32)?;
    Some(value as u16)
}

/// 进度数字专用高分辨率几何单元（1MiB 级 EFI 早期路径无字体服务）。
const DIGIT_W: usize = 28;
const DIGIT_H: usize = 54;
const DIGIT_GAP: usize = 10;
const PCT_W: usize = 40;
const PERCENT_W: usize = 3 * DIGIT_W + 2 * DIGIT_GAP + PCT_W;
const PERCENT_H: usize = 56;

#[derive(Clone, Copy)]
enum Shape {
    H { x0: usize, x1: usize, y0: usize, y1: usize },
    V { x: usize, y0: usize, y1: usize },
    Circle { cx: usize, cy: usize, r: usize },
    Slash { sum: usize },
}

fn digit_shapes(d: u8) -> [Shape; 7] {
    const T: (usize, usize) = (1, 27);
    const M: (usize, usize) = (1, 27);
    const B: (usize, usize) = (1, 27);
    const L: usize = 0;
    const R: usize = 23;
    let h = |v: (usize, usize), y: usize| Shape::H { x0: v.0, x1: v.1, y0: y, y1: y + 5 };
    let v = |x: usize, y0: usize, y1: usize| Shape::V { x, y0, y1 };
    let blank = Shape::H { x0: 0, x1: 0, y0: 0, y1: 0 };
    let mut out = [blank; 7];
    let push = |out: &mut [Shape; 7], shape: Shape| {
        for slot in out.iter_mut() {
            if matches!(*slot, Shape::H { x0: 0, x1: 0, y0: 0, y1: 0 }) {
                *slot = shape;
                break;
            }
        }
    };
    match d {
        0 => {
            push(&mut out, h(T, 1));
            push(&mut out, h(B, 49));
            push(&mut out, v(L, 1, 49));
            push(&mut out, v(R, 1, 49));
        }
        1 => {
            push(&mut out, v(R, 1, 49));
            push(&mut out, h((14, 27), 1));
        }
        2 => {
            push(&mut out, h(T, 1));
            push(&mut out, v(R, 1, 25));
            push(&mut out, h(M, 25));
            push(&mut out, v(L, 25, 49));
            push(&mut out, h(B, 49));
        }
        3 => {
            push(&mut out, h(T, 1));
            push(&mut out, v(R, 1, 49));
            push(&mut out, h(M, 25));
            push(&mut out, h(B, 49));
        }
        4 => {
            push(&mut out, v(L, 1, 25));
            push(&mut out, h(M, 25));
            push(&mut out, v(R, 1, 49));
        }
        5 => {
            push(&mut out, h(T, 1));
            push(&mut out, v(L, 1, 25));
            push(&mut out, h(M, 25));
            push(&mut out, v(R, 25, 49));
            push(&mut out, h(B, 49));
        }
        6 => {
            push(&mut out, h(T, 1));
            push(&mut out, v(L, 1, 49));
            push(&mut out, h(M, 25));
            push(&mut out, v(R, 25, 49));
            push(&mut out, h(B, 49));
        }
        7 => {
            push(&mut out, h(T, 1));
            push(&mut out, v(R, 1, 49));
        }
        8 => {
            push(&mut out, h(T, 1));
            push(&mut out, h(M, 25));
            push(&mut out, h(B, 49));
            push(&mut out, v(L, 1, 49));
            push(&mut out, v(R, 1, 49));
        }
        9 => {
            push(&mut out, h(T, 1));
            push(&mut out, v(L, 1, 25));
            push(&mut out, h(M, 25));
            push(&mut out, v(R, 1, 49));
            push(&mut out, h(B, 49));
        }
        _ => {}
    }
    out
}

fn shape_hit(shape: Shape, sx: usize, sy: usize) -> bool {
    match shape {
        Shape::H { x0, x1, y0, y1 } => sx >= x0 * 4 && sx < x1 * 4 && sy >= y0 * 4 && sy < y1 * 4,
        Shape::V { x, y0, y1 } => sx >= x * 4 && sx < (x + 5) * 4 && sy >= y0 * 4 && sy < y1 * 4,
        Shape::Circle { cx, cy, r } => {
            let dx = sx.saturating_sub(cx * 4);
            let dy = sy.saturating_sub(cy * 4);
            dx * dx + dy * dy < (r * 4) * (r * 4)
        }
        Shape::Slash { sum } => {
            let target = sum * 4;
            let value = sx as i32 + sy as i32;
            let target = target as i32;
            (value - target).abs() <= 10
        }
    }
}

/// Layout used by both the initial paint and incremental updates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Layout {
    pub logo_x: usize,
    pub logo_y: usize,
    pub logo_scale: usize,
    pub bar_x: usize,
    pub bar_y: usize,
    pub bar_w: usize,
    pub bar_h: usize,
    pub percent_x: usize,
    pub percent_y: usize,
    pub percent_w: usize,
    pub stage_x: usize,
    pub stage_y: usize,
    pub stage_scale: usize,
}

fn text_width(text: &str, scale: usize) -> Option<usize> {
    if scale == 0 || text.is_empty() {
        return None;
    }
    let cells = text.len();
    cells.checked_mul(9)?.checked_mul(scale)?.checked_sub(scale)
}

fn text_height(scale: usize) -> Option<usize> {
    usize::from(16u8).checked_mul(scale)
}

/// Derive a geometry-relative layout. It keeps portrait 1080×1920 and the
/// GUI8 800×600 verification topology native; it never letterboxes or changes
/// the scanout geometry.
pub fn layout(width: usize, height: usize) -> Option<Layout> {
    if width < 480 || height < 480 {
        return None;
    }
    let portrait = height >= width;
    let logo_scale = if portrait {
        min(8, max(2, width / 150))
    } else {
        min(6, max(2, width / 150))
    };
    let small_scale = max(2, logo_scale / 2);
    let percent_h = PERCENT_H;
    let logo_w = text_width("BerryOS", logo_scale)?;
    let logo_h = text_height(logo_scale)?;
    let logo_x = (width - logo_w) / 2;
    let logo_y = if portrait { height * 40 / 100 } else { height * 30 / 100 };

    let bar_w = width * 70 / 100;
    let bar_x = (width - bar_w) / 2;
    let bar_h = if portrait { max(14, height / 96) } else { max(10, height / 50) };
    let bar_y = if portrait { height * 68 / 100 } else { height * 72 / 100 };

    let percent_w = PERCENT_W;
    let percent_x = (width - percent_w) / 2;
    let percent_y = bar_y + bar_h + 12;

    // Reserve using the longest current label rather than the incoming one;
    // updates can then erase the worst-case text block without geometry drift.
    let stage_scale = max(2, small_scale.saturating_sub(1));
    let stage_w = text_width("Verifying boot disk", stage_scale)?;
    let stage_h = text_height(stage_scale)?;
    let stage_x = (width - stage_w) / 2;
    let stage_y = percent_y + percent_h + stage_h;

    if logo_x + logo_w >= width
        || logo_y + logo_h >= height
        || bar_x + bar_w >= width
        || bar_y + bar_h >= height
        || percent_x + percent_w >= width
        || percent_y + percent_h >= height
        || stage_x + stage_w >= width
        || stage_y + stage_h >= height
    {
        return None;
    }
    Some(Layout {
        logo_x,
        logo_y,
        logo_scale,
        bar_x,
        bar_y,
        bar_w,
        bar_h,
        percent_x,
        percent_y,
        percent_w,
        stage_x,
        stage_y,
        stage_scale,
    })
}

/// Incremental boot-progress renderer over an owned linear framebuffer.
pub struct Renderer<'a> {
    fb: &'a mut [u8],
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    layout: Layout,
    prev_permille: u16,
    prev_stage: Stage,
}

const BG: (u8, u8, u8) = (8, 14, 24);
const FG: (u8, u8, u8) = (245, 247, 250);
const ACCENT: (u8, u8, u8) = (0, 116, 224);
const TRACK: (u8, u8, u8) = (42, 56, 74);

impl<'a> Renderer<'a> {
    /// Validate that the slice owns the complete linear surface, including
    /// stride padding. Unlike console cells, this boot surface may clear all
    /// pixels and padding because QEMU scanout owns the whole framebuffer.
    pub fn new(
        fb: &'a mut [u8],
        width: usize,
        height: usize,
        stride: usize,
        format_raw: u32,
    ) -> Option<Self> {
        let format = PixelFormat::from_raw(format_raw)?;
        if width < 320 || height < 240 {
            return None;
        }
        let row_bytes = width.checked_mul(4)?;
        if stride < row_bytes {
            return None;
        }
        let need = stride.checked_mul(height.checked_sub(1)?)?.checked_add(row_bytes)?;
        if fb.len() < need {
            return None;
        }
        let layout = layout(width, height)?;
        Some(Self {
            fb,
            width,
            height,
            stride,
            format,
            layout,
            prev_permille: PERMILLE_MAX,
            prev_stage: Stage::Display,
        })
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// Restore a renderer with a previous monotone state. This is used when
    /// the framebuffer is borrowed transiently (for example by BerryBoot) but
    /// progress truth is owned by a longer-lived runtime.
    pub fn restore(
        fb: &'a mut [u8],
        width: usize,
        height: usize,
        stride: usize,
        format_raw: u32,
        prev_permille: u16,
        prev_stage: Stage,
    ) -> Option<Self> {
        let mut this = Self::new(fb, width, height, stride, format_raw)?;
        this.prev_permille = min(prev_permille, PERMILLE_MAX);
        this.prev_stage = prev_stage;
        Some(this)
    }

    pub fn prev_state(&self) -> (Stage, u16) {
        (self.prev_stage, self.prev_permille)
    }

    #[inline]
    fn put_px(&mut self, x: usize, y: usize, rgb: (u8, u8, u8)) {
        if x >= self.width || y >= self.height {
            return;
        }
        let off = y * self.stride + x * 4;
        let px = self.format.pack(rgb.0, rgb.1, rgb.2);
        self.fb[off..off + 4].copy_from_slice(&px.to_le_bytes());
    }

    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, rgb: (u8, u8, u8)) {
        for yy in y..y.saturating_add(h) {
            for xx in x..x.saturating_add(w) {
                self.put_px(xx, yy, rgb);
            }
        }
    }

    fn glyph(
        &mut self,
        ch: u8,
        x: usize,
        y: usize,
        scale: usize,
        fg: (u8, u8, u8),
        bg: (u8, u8, u8),
    ) {
        let g = font::glyph(ch).unwrap_or(&[0; 16]);
        for gy in 0..16 {
            for gx in 0..8 {
                let on = g[gy] & (0x80 >> gx) != 0;
                let rgb = if on { fg } else { bg };
                self.rect(
                    x + gx * scale,
                    y + gy * scale,
                    scale,
                    scale,
                    rgb,
                );
            }
        }
    }

    fn text(
        &mut self,
        text: &str,
        x: usize,
        y: usize,
        scale: usize,
        fg: (u8, u8, u8),
        bg: (u8, u8, u8),
    ) {
        let mut cx = x;
        for &b in text.as_bytes() {
            self.glyph(b, cx, y, scale, fg, bg);
            cx += 9 * scale;
        }
    }

    fn clear(&mut self) {
        self.rect(0, 0, self.width, self.height, BG);
    }

    fn bar(&mut self, permille: u16) {
        let l = self.layout;
        self.rect(l.bar_x, l.bar_y, l.bar_w, l.bar_h, TRACK);
        let filled = if permille >= PERMILLE_MAX {
            l.bar_w
        } else {
            (usize::from(l.bar_w) * usize::from(permille)) / usize::from(PERMILLE_MAX)
        };
        if filled > 0 {
            self.rect(l.bar_x, l.bar_y, filled, l.bar_h, ACCENT);
        }
    }

    fn blend_px(&mut self, x: usize, y: usize, fg: (u8, u8, u8), bg: (u8, u8, u8), cov: u8) {
        if cov == 0 {
            return;
        }
        let a = u16::from(cov);
        let b = u16::from(16u8.saturating_sub(cov));
        let mix = |c: u8, d: u8| ((u16::from(c) * a + u16::from(d) * b) / 16) as u8;
        self.put_px(x, y, (mix(fg.0, bg.0), mix(fg.1, bg.1), mix(fg.2, bg.2)));
    }

    fn draw_digit(&mut self, d: u8, x: usize, y: usize) {
        let shapes = digit_shapes(d);
        for py in 0..DIGIT_H {
            for px in 0..DIGIT_W {
                let mut cov = 0u8;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let sample_x = (px * 4 + sx) + 2;
                        let sample_y = (py * 4 + sy) + 2;
                        if shapes.iter().any(|&shape| shape_hit(shape, sample_x, sample_y)) {
                            cov += 1;
                        }
                    }
                }
                if cov != 0 {
                    self.blend_px(x + px, y + py, FG, BG, cov);
                }
            }
        }
    }

    fn draw_percent_sign(&mut self, x: usize, y: usize) {
        for py in 0..DIGIT_H {
            for px in 0..PCT_W {
                let mut cov = 0u8;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let sample_x = (px * 4 + sx) + 2;
                        let sample_y = (py * 4 + sy) + 2;
                        let circles = [
                            Shape::Circle { cx: 8, cy: 11, r: 6 },
                            Shape::Circle { cx: 31, cy: 43, r: 6 },
                            Shape::Slash { sum: 50 },
                        ];
                        if circles.iter().any(|&shape| shape_hit(shape, sample_x, sample_y)) {
                            cov += 1;
                        }
                    }
                }
                if cov != 0 {
                    self.blend_px(x + px, y + py, FG, BG, cov);
                }
            }
        }
    }

    fn percent_text(&mut self, permille: u16) {
        let percent = permille / 10;
        let (digits, d0, d1, d2) = if percent == 100 {
            (3, Some(1u8), Some(0u8), Some(0u8))
        } else {
            (2, Some((percent / 10) as u8), Some((percent % 10) as u8), None)
        };
        let width = digits * DIGIT_W + (digits - 1) * DIGIT_GAP + PCT_W;
        let x = self.width.saturating_sub(width) / 2;
        let y = self.layout.percent_y;
        let mut cx = x;
        for d in [d0, d1, d2].into_iter().flatten() {
            self.draw_digit(d, cx, y);
            cx += DIGIT_W + DIGIT_GAP;
        }
        self.draw_percent_sign(cx, y);
    }

    fn stage_text(&mut self, stage: Stage) {
        let l = self.layout;
        let worst = text_width("Verifying boot disk", l.stage_scale).unwrap_or(0);
        self.rect(l.stage_x, l.stage_y, worst, 16 * l.stage_scale, BG);
        self.text(
            stage.label(),
            l.stage_x,
            l.stage_y,
            l.stage_scale,
            FG,
            BG,
        );
    }

    /// First paint: clear the surface and draw logo, labels and the exact
    /// current progress. Call once after mode-set, before any update.
    pub fn initialize(&mut self, stage: Stage, permille: u16) {
        let p = min(permille, PERMILLE_MAX);
        self.clear();
        let l = self.layout;
        self.text("BerryOS", l.logo_x, l.logo_y, l.logo_scale, FG, BG);
        self.bar(p);
        self.percent_text(p);
        self.stage_text(stage);
        self.prev_permille = p;
        self.prev_stage = stage;
    }

    /// Incremental update. Regression is ignored and reported as `false`;
    /// display progress is monotonic even when a retry rewinds a byte counter.
    pub fn update(&mut self, stage: Stage, permille: u16) -> bool {
        let p = min(permille, PERMILLE_MAX);
        if p < self.prev_permille {
            return false;
        }
        if p > self.prev_permille {
            let l = self.layout;
            let old = (usize::from(l.bar_w) * usize::from(self.prev_permille))
                / usize::from(PERMILLE_MAX);
            let new = (usize::from(l.bar_w) * usize::from(p)) / usize::from(PERMILLE_CEILING);
            if new > old {
                self.rect(l.bar_x + old, l.bar_y, new - old, l.bar_h, ACCENT);
            }
            self.percent_text(p);
        }
        if stage != self.prev_stage {
            self.stage_text(stage);
        }
        self.prev_permille = p;
        self.prev_stage = stage;
        true
    }
}

const PERMILLE_CEILING: u16 = PERMILLE_MAX;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_maps_real_counters_and_fails_closed() {
        assert_eq!(span(30, 80, 0, 10), Some(30));
        assert_eq!(span(30, 80, 5, 10), Some(55));
        assert_eq!(span(30, 80, 10, 10), Some(80));
        assert_eq!(span(30, 80, 11, 10), None);
        assert_eq!(span(30, 80, 1, 0), None);
        assert_eq!(span(80, 30, 0, 1), None);
    }

    #[test]
    fn layout_is_native_for_product_and_gui8() {
        let p = layout(1080, 1920).unwrap();
        assert!(p.logo_x + 7 * 9 * p.logo_scale <= 1080);
        assert_eq!(p.bar_w, 756);
        assert!(p.bar_y + p.bar_h < 1920);

        let g = layout(800, 600).unwrap();
        assert_eq!(g.bar_w, 560);
        assert!(g.bar_y + g.bar_h < 600);
    }

    #[test]
    fn percent_digits_are_anti_aliased_native_geometry() {
        let w = 800;
        let h = 600;
        let stride = w * 4;
        let mut fb = vec![0u8; stride * h];
        let l = layout(w, h).unwrap();
        {
            let mut r = Renderer::new(&mut fb, w, h, stride, 1).unwrap();
            r.initialize(Stage::Kernel, 1000);
        }
        let fmt = berry_fbcon::render::PixelFormat::from_raw(1).unwrap();
        let fg_px = fmt.pack(FG.0, FG.1, FG.2);
        let bg_px = fmt.pack(BG.0, BG.1, BG.2);
        let mut fg = 0usize;
        let mut intermediate = 0usize;
        for y in l.percent_y..(l.percent_y + PERCENT_H).min(h) {
            for x in (l.percent_x.saturating_sub(40))..(l.percent_x + l.percent_w + 40).min(w) {
                let px = u32::from_le_bytes(
                    fb[(y * stride + x * 4)..].get(..4).unwrap().try_into().unwrap(),
                );
                if px == fg_px {
                    fg += 1;
                } else if px != bg_px {
                    intermediate += 1;
                }
            }
        }
        assert!(fg > 100, "large digits absent");
        assert!(intermediate > 100, "anti-aliased coverage absent");
    }

    #[test]
    fn renderer_rejects_short_or_undersized_surface() {
        let mut fb = [0u8; 128];
        assert!(Renderer::new(&mut fb, 1080, 1920, 4320, 1).is_none());
        let mut small = [0u8; 4096];
        assert!(Renderer::new(&mut small, 480, 480, 1920, 1).is_none());
    }

    #[test]
    fn renderer_paints_and_enforces_monotone_progress() {
        let w = 480;
        let h = 480;
        let stride = w * 4;
        let mut fb = vec![0u8; stride * h];
        {
            let mut r = Renderer::new(&mut fb, w, h, stride, 1).unwrap();
            r.initialize(Stage::Kernel, 0);
            assert!(r.update(Stage::Kernel, 500));
            assert!(!r.update(Stage::Kernel, 499));
            assert!(r.update(Stage::Handoff, 1000));
        }
        // Every pixel must be non-zero in its RGB bytes, proving the initial
        // paint owns the native surface rather than a console-cell subset.
        // The reserved/alpha byte remains zero by format contract.
        assert!(fb.chunks_exact(4).all(|px| px[..3].iter().any(|&b| b != 0)));
    }
}
