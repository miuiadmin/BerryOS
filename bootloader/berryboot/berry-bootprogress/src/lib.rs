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

/// System boot milestones visible on the product vgpu console. Every value is
/// tied to a real producer state; no wall-clock interpolation is permitted.
pub const M_VGPU_READY: u16 = 450;
pub const M_PRESENT_BACKEND_READY: u16 = 500;
pub const M_WMD_SPAWNED: u16 = 520;
pub const M_INPUT_ARMED: u16 = 560;
pub const M_INPUTD_SPAWNED: u16 = 620;
/// Truth is "svcmgr successfully spawned fsmgr"; eager FAT initialization may
/// still be running. This is deliberately not a readiness claim.
pub const M_FSMGR_SPAWNED: u16 = 650;
/// Truth is "svcmgr successfully spawned vnet"; the driver self-test may still
/// be running. This is deliberately not a readiness claim.
pub const M_VNET_SPAWNED: u16 = 680;
pub const M_PROGRESS_CONTROL_HANDOFF: u16 = 700;
pub const M_SYSTEM_UI_CONNECTED: u16 = 800;
pub const M_FIRST_REAL_SUBMIT: u16 = 900;
pub const M_DESKTOP_HANDOFF: u16 = PERMILLE_MAX;

/// Wire discriminator for a system milestone. Zero means "absent" and remains
/// valid for legacy BootProgress frames; 1..=[`BOOT_M_MAX`] is a stable enum
/// id rather than a new op or wire version.
pub const BOOT_M_MAX: u8 = 11;

/// A real system-boot truth point. Progress and stage are derived from one
/// table so producers cannot hand-assemble inconsistent triples.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Milestone {
    VgpuReady,
    PresentBackendReady,
    WmdSpawned,
    InputArmed,
    InputdSpawned,
    FsmgrSpawned,
    VnetSpawned,
    ProgressControlHandoff,
    SystemUiConnected,
    FirstRealSubmit,
    DesktopHandoff,
}

impl Milestone {
    pub const ALL: [Self; 11] = [
        Self::VgpuReady,
        Self::PresentBackendReady,
        Self::WmdSpawned,
        Self::InputArmed,
        Self::InputdSpawned,
        Self::FsmgrSpawned,
        Self::VnetSpawned,
        Self::ProgressControlHandoff,
        Self::SystemUiConnected,
        Self::FirstRealSubmit,
        Self::DesktopHandoff,
    ];

    pub const fn id(self) -> u8 {
        match self {
            Self::VgpuReady => 1,
            Self::PresentBackendReady => 2,
            Self::WmdSpawned => 3,
            Self::InputArmed => 4,
            Self::InputdSpawned => 5,
            Self::FsmgrSpawned => 6,
            Self::VnetSpawned => 7,
            Self::ProgressControlHandoff => 8,
            Self::SystemUiConnected => 9,
            Self::FirstRealSubmit => 10,
            Self::DesktopHandoff => 11,
        }
    }

    pub const fn from_id(id: u8) -> Option<Self> {
        match id {
            1 => Some(Self::VgpuReady),
            2 => Some(Self::PresentBackendReady),
            3 => Some(Self::WmdSpawned),
            4 => Some(Self::InputArmed),
            5 => Some(Self::InputdSpawned),
            6 => Some(Self::FsmgrSpawned),
            7 => Some(Self::VnetSpawned),
            8 => Some(Self::ProgressControlHandoff),
            9 => Some(Self::SystemUiConnected),
            10 => Some(Self::FirstRealSubmit),
            11 => Some(Self::DesktopHandoff),
            _ => None,
        }
    }

    pub const fn permille(self) -> u16 {
        match self {
            Self::VgpuReady => M_VGPU_READY,
            Self::PresentBackendReady => M_PRESENT_BACKEND_READY,
            Self::WmdSpawned => M_WMD_SPAWNED,
            Self::InputArmed => M_INPUT_ARMED,
            Self::InputdSpawned => M_INPUTD_SPAWNED,
            Self::FsmgrSpawned => M_FSMGR_SPAWNED,
            Self::VnetSpawned => M_VNET_SPAWNED,
            Self::ProgressControlHandoff => M_PROGRESS_CONTROL_HANDOFF,
            Self::SystemUiConnected => M_SYSTEM_UI_CONNECTED,
            Self::FirstRealSubmit => M_FIRST_REAL_SUBMIT,
            Self::DesktopHandoff => M_DESKTOP_HANDOFF,
        }
    }

    pub const fn stage(self) -> Stage {
        match self {
            Self::VgpuReady
            | Self::PresentBackendReady
            | Self::WmdSpawned
            | Self::InputArmed
            | Self::InputdSpawned
            | Self::FsmgrSpawned
            | Self::VnetSpawned => Stage::Prepare,
            Self::ProgressControlHandoff
            | Self::SystemUiConnected
            | Self::FirstRealSubmit
            | Self::DesktopHandoff => Stage::Handoff,
        }
    }

    pub const fn log_line(self) -> &'static [u8] {
        match self {
            Self::VgpuReady => b"vgpu: display ready",
            Self::PresentBackendReady => b"gpu: present backend ready",
            Self::WmdSpawned => b"wm: compositor spawned",
            Self::InputArmed => b"input: service armed",
            Self::InputdSpawned => b"input: virtio service spawned",
            Self::FsmgrSpawned => b"fs: service spawned",
            Self::VnetSpawned => b"net: service spawned",
            Self::ProgressControlHandoff => b"boot: progress handoff",
            Self::SystemUiConnected => b"shell: system ui connected",
            Self::FirstRealSubmit => b"shell: first frame submitted",
            Self::DesktopHandoff => b"desktop: ready",
        }
    }
}

/// Presentation face selected by the boot mode; both consume the same truth.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    Capsule,
    VerboseText,
}

/// Maximum diagnostic lines retained by a verbose boot surface. The ring is
/// large enough for every system milestone plus one spare loader diagnostic.
pub const VERBOSE_LOG_MAX: usize = 12;

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

/// Smoothstep a real byte/segment counter into a permille interval.
///
/// The fraction comes only from `done/total`; wall clock is not an input. The
/// Q16.16 cubic curve makes chunked file reads advance gently at both ends,
/// while `span()`'s fail-closed and monotone properties are preserved.
pub fn span_smooth(start: u16, end: u16, done: u64, total: u64) -> Option<u16> {
    if end > PERMILLE_MAX || start > end || total == 0 || done > total {
        return None;
    }
    let x = (done.checked_mul(65_536)? / total).min(65_536) as u64;
    let x2 = (x * x) >> 16;
    let x3 = (x2 * x) >> 16;
    let numerator = 3 * x2 - 2 * x3;
    let width = u32::from(end - start);
    let value = u32::from(start).checked_add(
        (u64::from(width) * numerator / 65_536) as u32,
    )?;
    Some(value as u16)
}

/// Presentation slew toward a truth target. At most eight permille move per
/// 100 ms; a jump of at least 80 permille (or completion) snaps immediately.
/// The result never regresses and never advances past the producer truth.
pub fn slew(prev: u16, truth: u16, dt_ms: u32) -> u16 {
    let prev = min(prev, PERMILLE_MAX);
    let truth = min(truth, PERMILLE_MAX);
    if truth <= prev {
        return prev;
    }
    let delta = u32::from(truth - prev);
    if truth == PERMILLE_MAX || delta >= 80 {
        return truth;
    }
    let ticks = (dt_ms / 100).max(1);
    let step = delta.min(ticks.saturating_mul(8)).max(1);
    prev.saturating_add(u16::try_from(step).unwrap_or(PERMILLE_MAX - prev))
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
    let logo_w = text_width("BerryOS", logo_scale)?;
    let logo_h = text_height(logo_scale)?;
    let logo_x = (width - logo_w) / 2;
    let logo_y = if portrait { height * 40 / 100 } else { height * 30 / 100 };

    let bar_w = width * 70 / 100;
    let bar_x = (width - bar_w) / 2;
    let bar_h = if portrait { max(14, height / 96) } else { max(10, height / 50) };
    let bar_y = if portrait { height * 68 / 100 } else { height * 72 / 100 };


    if logo_x + logo_w >= width
        || logo_y + logo_h >= height
        || bar_x + bar_w >= width
        || bar_y + bar_h >= height
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
    face: Face,
    prev_permille: u16,
    prev_stage: Stage,
}

const BG: (u8, u8, u8) = (0, 0, 0);
const FG: (u8, u8, u8) = (255, 255, 255);
/// QEMU ramfb/virtio-gpu boot surface: little-endian XRGB8888.
const FORMAT_RAW_XRGB: u32 = 0;
const ACCENT: (u8, u8, u8) = FG;
const TRACK: (u8, u8, u8) = (58, 58, 60);

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
        if format_raw != FORMAT_RAW_XRGB {
            return None;
        }
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
            face: Face::Capsule,
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

    fn integer_sqrt(value: usize) -> usize {
        if value < 2 {
            return value;
        }
        let mut x = value;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + value / x) / 2;
        }
        x
    }

    fn capsule(&mut self, x: usize, y: usize, w: usize, h: usize, rgb: (u8, u8, u8)) {
        if w == 0 || h == 0 {
            return;
        }
        let radius = h / 2;
        let left_cx = x + radius;
        let right_cx = x + w - 1 - radius;
        for yy in y..y.saturating_add(h) {
            let dy = (yy as i32 - (y + radius) as i32).abs();
            let squared = (radius as i32 * radius as i32 - dy * dy).max(0);
            let chord = Self::integer_sqrt(squared as usize) + 1;
            for xx in x..x.saturating_add(w) {
                let on = if xx < left_cx {
                    left_cx.saturating_sub(xx) < chord
                } else if xx > right_cx {
                    xx.saturating_sub(right_cx) < chord
                } else {
                    true
                };
                if on {
                    self.put_px(xx, yy, rgb);
                }
            }
        }
    }

    fn bar(&mut self, permille: u16) {
        let l = self.layout;
        // Repaint the complete capsule bounding box. This clears rectangular
        // corners and lets incremental updates remain exact without tracking
        // the previous rounded-end geometry.
        self.rect(
            l.bar_x.saturating_sub(2),
            l.bar_y.saturating_sub(2),
            l.bar_w + 4,
            l.bar_h + 4,
            BG,
        );
        self.capsule(l.bar_x, l.bar_y, l.bar_w, l.bar_h, TRACK);
        let filled = if permille >= PERMILLE_MAX {
            l.bar_w
        } else {
            (usize::from(l.bar_w) * usize::from(permille)) / usize::from(PERMILLE_MAX)
        };
        if filled > 0 {
            let filled = filled.max(l.bar_h).min(l.bar_w);
            self.capsule(l.bar_x, l.bar_y, filled, l.bar_h, ACCENT);
        }
    }

    fn clipped_text(&mut self, text: &[u8], x: usize, y: usize, scale: usize) {
        let cell_w = 8 * scale;
        if y >= self.height || x >= self.width || scale == 0 {
            return;
        }
        let cols = (self.width - x) / cell_w;
        for (col, &b) in text.iter().take(cols).enumerate() {
            self.glyph(b, x + col * (cell_w + scale), y, scale, FG, BG);
        }
    }

    fn paint_verbose(&mut self, stage: Stage, permille: u16, logs: &[&[u8]]) {
        self.clear();
        let l = self.layout;
        let scale = if self.height >= 1080 { 2 } else { 1 };
        let label_y = l.bar_y.saturating_sub(16 * scale + 18);
        self.clipped_text(stage.label().as_bytes(), l.bar_x, label_y, scale);

        let log_y = l.bar_y.saturating_add(l.bar_h + 24);
        let row_h = 16 * scale + 4;
        let capacity = ((self.height - log_y) / row_h).min(VERBOSE_LOG_MAX);
        let start = logs.len().saturating_sub(capacity);
        for (row, line) in logs[start..].iter().enumerate() {
            let y = log_y.saturating_add(row * row_h);
            if y >= self.height {
                break;
            }
            self.clipped_text(line, 24, y, scale);
        }
        self.bar(permille);
    }

    /// First paint for the verbose command-line face. Diagnostic lines are
    /// presentation-only; `(stage, permille)` remains the sole progress truth.
    pub fn initialize_verbose(
        &mut self,
        stage: Stage,
        permille: u16,
        logs: &[&[u8]],
    ) {
        self.face = Face::VerboseText;
        let p = min(permille, PERMILLE_MAX);
        self.paint_verbose(stage, p, logs);
        self.prev_permille = p;
        self.prev_stage = stage;
    }

    /// Incremental verbose update. The full visible face is small enough to
    /// repaint atomically; progress still refuses regression.
    pub fn update_verbose(&mut self, stage: Stage, permille: u16, logs: &[&[u8]]) -> bool {
        self.face = Face::VerboseText;
        let p = min(permille, PERMILLE_MAX);
        if p < self.prev_permille {
            return false;
        }
        self.paint_verbose(stage, p, logs);
        self.prev_permille = p;
        self.prev_stage = stage;
        true
    }

    /// First paint: clear the surface, draw the white mark and exact current
    /// white progress. Call once after mode-set, before any update.
    pub fn initialize(&mut self, stage: Stage, permille: u16) {
        let p = min(permille, PERMILLE_MAX);
        self.clear();
        let l = self.layout;
        self.text("BerryOS", l.logo_x, l.logo_y, l.logo_scale, FG, BG);
        self.bar(p);
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
            self.bar(p);
        }
        self.prev_permille = p;
        self.prev_stage = stage;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_milestones_are_strict_monotonic_truth_points() {
        let milestones = [
            M_VGPU_READY,
            M_PRESENT_BACKEND_READY,
            M_WMD_SPAWNED,
            M_INPUT_ARMED,
            M_INPUTD_SPAWNED,
            M_FSMGR_SPAWNED,
            M_VNET_SPAWNED,
            M_PROGRESS_CONTROL_HANDOFF,
            M_SYSTEM_UI_CONNECTED,
            M_FIRST_REAL_SUBMIT,
            M_DESKTOP_HANDOFF,
        ];
        assert_eq!(milestones.len(), 11);
        assert!(milestones.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(*milestones.last().unwrap(), PERMILLE_MAX);
    }

    #[test]
    fn milestone_table_is_total_and_wire_stable() {
        assert_eq!(Milestone::ALL.len(), usize::from(BOOT_M_MAX));
        for (index, milestone) in Milestone::ALL.iter().enumerate() {
            let id = u8::try_from(index + 1).unwrap();
            assert_eq!(milestone.id(), id);
            assert_eq!(Milestone::from_id(id), Some(*milestone));
            assert!(milestone.permille() <= PERMILLE_MAX);
            let line = milestone.log_line();
            assert!(!line.is_empty() && line.len() <= 40);
            assert!(line.iter().all(|b| b.is_ascii()));
        }
        assert_eq!(Milestone::from_id(0), None);
        assert_eq!(Milestone::from_id(BOOT_M_MAX + 1), None);
        assert!(Milestone::ALL.windows(2).all(|pair| pair[0].permille() < pair[1].permille()));
        assert!(VERBOSE_LOG_MAX >= Milestone::ALL.len());
    }

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
        assert!(Renderer::new(&mut fb, 1080, 1920, 4320, FORMAT_RAW_XRGB).is_none());
        let mut small = [0u8; 4096];
        assert!(Renderer::new(&mut small, 480, 480, 1920, FORMAT_RAW_XRGB).is_none());
        let mut wrong_format = vec![0u8; 480 * 480 * 4];
        assert!(Renderer::new(
            &mut wrong_format,
            480,
            480,
            480 * 4,
            1
        )
        .is_none());
    }

    #[test]
    fn smooth_span_and_slew_are_truth_monotone() {
        assert_eq!(span_smooth(30, 300, 0, 5), Some(30));
        assert_eq!(span_smooth(30, 300, 5, 5), Some(300));
        let mut prev = 0;
        for done in 0..=16 {
            let got = span_smooth(100, 200, done, 16).unwrap();
            assert!(got >= prev);
            prev = got;
        }
        assert_eq!(slew(100, 104, 100), 104);
        assert_eq!(slew(100, 190, 100), 190);
        assert_eq!(slew(500, 400, 100), 500);
        assert_eq!(slew(900, 1000, 0), 1000);
    }

    #[test]
    fn boot_bar_is_a_white_capsule_not_a_rectangle() {
        let w = 480;
        let h = 480;
        let stride = w * 4;
        let mut fb = vec![0u8; stride * h];
        let l = layout(w, h).unwrap();
        {
            let mut r = Renderer::new(&mut fb, w, h, stride, FORMAT_RAW_XRGB).unwrap();
            r.initialize(Stage::Kernel, 1000);
        }
        let px = |x: usize, y: usize| -> [u8; 3] {
            let off = y * stride + x * 4;
            [fb[off], fb[off + 1], fb[off + 2]]
        };
        assert_eq!(px(l.bar_x + 2, l.bar_y + l.bar_h / 2), [255, 255, 255]);
        // The first pixel of the bounding row is outside the rounded end and
        // must remain background even at 100%.
        assert_eq!(px(l.bar_x, l.bar_y), [0, 0, 0]);
        assert_eq!(px(l.bar_x, l.bar_y + l.bar_h - 1), [0, 0, 0]);
    }

    #[test]
    fn verbose_face_paints_stage_and_last_diagnostics() {
        let w = 800;
        let h = 600;
        let stride = w * 4;
        let mut fb = vec![0u8; stride * h];
        let l = layout(w, h).unwrap();
        {
            let mut r = Renderer::new(&mut fb, w, h, stride, FORMAT_RAW_XRGB).unwrap();
            r.initialize_verbose(Stage::Kernel, 100, &[b"kernel: ok", b"driver: virtio-gpu"]);
        }
        let mut non_black = 0usize;
        for y in 0..h {
            for x in 0..w {
                if fb[y * stride + x * 4..y * stride + x * 4 + 3] != [0, 0, 0] {
                    non_black += 1;
                }
            }
        }
        assert!(non_black > 200, "verbose stage/logs/bar are visible");
        assert!(l.bar_y > 0);
    }

    #[test]
    fn verbose_system_milestone_lines_render_in_both_geometries() {
        let lines: [&[u8]; 11] = Milestone::ALL.map(|milestone| milestone.log_line());
        for (w, h) in [(1080, 1920), (800, 600)] {
            let stride = w * 4;
            let mut fb = vec![0u8; stride * h];
            {
                let mut renderer =
                    Renderer::new(&mut fb, w, h, stride, FORMAT_RAW_XRGB).unwrap();
                renderer.initialize_verbose(Stage::Prepare, M_VGPU_READY, &lines);
                assert!(renderer.update_verbose(
                    Stage::Handoff,
                    PERMILLE_MAX,
                    &lines
                ));
            }
            let non_black = fb
                .chunks_exact(4)
                .filter(|px| px[..3] != [0, 0, 0])
                .count();
            assert!(non_black > 500, "{w}x{h} verbose system face is visible");
        }
    }

    #[test]
    fn renderer_paints_white_macos_bar_and_enforces_monotone_progress() {
        let w = 480;
        let h = 480;
        let stride = w * 4;
        let mut fb = vec![0u8; stride * h];
        let l = layout(w, h).unwrap();
        {
            let mut r = Renderer::new(&mut fb, w, h, stride, 0).unwrap();
            r.initialize(Stage::Kernel, 0);
            assert!(r.update(Stage::Kernel, 500));
            assert!(!r.update(Stage::Kernel, 499));
            assert!(r.update(Stage::Handoff, 1000));
        }
        // XRGB8888 memory is B,G,R,X. A pure macOS-style white fill must be
        // recognizable by its exact channel tuple, not an R/B-swapped accent.
        let fill_x = l.bar_x + l.bar_w / 2;
        let fill_y = l.bar_y + l.bar_h / 2;
        let fill = &fb[(fill_y * stride + fill_x * 4)..][..4];
        assert_eq!((&fill[0], &fill[1], &fill[2]), (&255u8, &255u8, &255u8));
        // No percentage or stage row exists below the bar: the area stays black.
        let below_y = l.bar_y + l.bar_h + 24;
        assert!(below_y < h);
        assert!(fb[below_y * stride..(below_y + 1) * stride]
            .chunks_exact(4)
            .all(|px| px[..3] == [0, 0, 0]));
    }
}
