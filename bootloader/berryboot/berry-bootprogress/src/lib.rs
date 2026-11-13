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
    pub percent_scale: usize,
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
    let logo_w = text_width("BerryOS", logo_scale)?;
    let logo_h = text_height(logo_scale)?;
    let logo_x = (width - logo_w) / 2;
    let logo_y = if portrait { height * 40 / 100 } else { height * 30 / 100 };

    let bar_w = width * 70 / 100;
    let bar_x = (width - bar_w) / 2;
    let bar_h = if portrait { max(14, height / 96) } else { max(10, height / 50) };
    let bar_y = if portrait { height * 68 / 100 } else { height * 72 / 100 };

    let percent = "100%";
    let percent_w = text_width(percent, small_scale)?;
    let percent_h = text_height(small_scale)?;
    let percent_x = (width - percent_w) / 2;
    let percent_y = bar_y + bar_h + percent_h;

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
        percent_scale: small_scale,
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

    fn percent_text(&mut self, permille: u16) {
        let percent = permille / 10;
        let mut buf = [b'0'; 4];
        buf[0] = b'0' + (percent / 10) as u8;
        buf[1] = b'0' + (percent % 10) as u8;
        buf[2] = b'%';
        // 0..99 用三位（00%..99%）；100% 用完整四位，终态不丢百分号。
        let n = if percent == 100 { 4 } else { 3 };
        let text = core::str::from_utf8(&buf[..n]).unwrap_or("0%");
        let l = self.layout;
        let w = text_width(text, l.percent_scale).unwrap_or(0);
        let x = self.width.saturating_sub(w) / 2;
        self.text(text, x, l.percent_y, l.percent_scale, FG, BG);
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
