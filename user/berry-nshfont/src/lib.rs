//! Native Shell 系统 HD 字形与 UI 基元。
//!
//! wmd 在 APP `WM_CONNECT` 成功 ack 随行该 APP 私有的 32×48 ASCII A8
//! 字形 VMO（v1 映射恒 UserRw，私有拷贝防跨 app 写穿）。客户端调用
//! [`map`] 固定映射到 [`FONT_VA`] 后，[`text`] 以 straight-alpha 混合
//! 原生 HD 字形（非 8×16 console 放大）；另提供抗锯齿圆角卡/圆/阴影。

#![no_std]

use berry_sys::{berry_vmar_map, RIGHT_READ, RIGHT_WRITE};

pub const FONT_VA: usize = 0x5300_0000;
pub const FONT_VMO_LEN: usize = 0x24000;
const GLYPH_W: usize = 32;
const GLYPH_H: usize = 48;
const GLYPH_BYTES: usize = GLYPH_W * GLYPH_H;

/// 映射 wmd 私有字形 VMO。v1 `vmar_map` 只支持 UserRw；wmd 为每个 APP
/// 创建独立 VMO，因此本进程可写不影响其它 app/global 字体。
pub fn map(vmar: berry_sys::Handle, font_vmo: berry_sys::Handle) -> bool {
    let mut va = FONT_VA;
    berry_vmar_map(
        vmar,
        0,
        font_vmo,
        0,
        &mut va,
        FONT_VMO_LEN,
        RIGHT_READ | RIGHT_WRITE,
    ) == 0 && va == FONT_VA
}

fn coverage(ch: u8) -> Option<&'static [u8]> {
    if !(b' '..=b'~').contains(&ch) {
        return None;
    }
    // SAFETY: `map` 成功后 FONT_VA..+FONT_VMO_LEN 是本进程私有字形页；
    // 布局由 wmd 单源冻结：glyph(ch) @ (ch-32)*1536。
    Some(unsafe {
        core::slice::from_raw_parts(
            (FONT_VA + usize::from(ch - b' ') * GLYPH_BYTES) as *const u8,
            GLYPH_BYTES,
        )
    })
}

/// 32×48 HD A8 文本。`fb` 为 XRGB 窗口局部 buffer，`stride` 单位字节。
pub fn text(
    fb: &mut [u8], stride: usize, s: &[u8], x0: usize, y0: usize,
    color: [u8; 4],
) {
    for (i, &ch) in s.iter().enumerate() {
        let Some(g) = coverage(ch) else { continue };
        for y in 0..GLYPH_H {
            for x in 0..GLYPH_W {
                let a = usize::from(g[y * GLYPH_W + x]);
                if a == 0 {
                    continue;
                }
                let px = x0 + i * GLYPH_W + x;
                let py = y0 + y;
                let off = py * stride + px * 4;
                if off + 4 > fb.len() {
                    continue;
                }
                if a == 255 {
                    let bytes = [color[2], color[1], color[0], 0];
                    fb[off..off + 4].copy_from_slice(&bytes);
                    continue;
                }
                let inv = 255 - a;
                let bg = &fb[off..off + 3];
                let mixed: [u8; 4] = [
                    ((usize::from(color[2]) * a + usize::from(bg[0]) * inv) / 255) as u8,
                    ((usize::from(color[1]) * a + usize::from(bg[1]) * inv) / 255) as u8,
                    ((usize::from(color[0]) * a + usize::from(bg[2]) * inv) / 255) as u8,
                    0,
                ];
                fb[off..off + 4].copy_from_slice(&mixed);
            }
        }
    }
}

/// 以 straight-alpha 写一颗 XRGB 像素（`color` RGB、`a` 0..=255）。
pub fn blend_px(
    fb: &mut [u8], stride: usize, x: usize, y: usize,
    color: [u8; 3], a: u8,
) {
    let off = y * stride + x * 4;
    if off + 4 > fb.len() {
        return;
    }
    if a == 0 {
        return;
    }
    if a == 255 {
        fb[off] = color[2];
        fb[off + 1] = color[1];
        fb[off + 2] = color[0];
        fb[off + 3] = 0;
        return;
    }
    let aa = usize::from(a);
    let inv = 255 - aa;
    let bg = [fb[off], fb[off + 1], fb[off + 2]];
    fb[off] = ((usize::from(color[2]) * aa + usize::from(bg[0]) * inv) / 255) as u8;
    fb[off + 1] = ((usize::from(color[1]) * aa + usize::from(bg[1]) * inv) / 255) as u8;
    fb[off + 2] = ((usize::from(color[0]) * aa + usize::from(bg[2]) * inv) / 255) as u8;
    fb[off + 3] = 0;
}

fn rounded_inside(x: usize, y: usize, w: usize, h: usize, r: usize, px: i32, py: i32) -> bool {
    if px < x as i32 || py < y as i32 || px >= (x + w) as i32 || py >= (y + h) as i32 {
        return false;
    }
    let left = px < (x + r) as i32;
    let right = px >= (x + w).saturating_sub(r) as i32;
    let top = py < (y + r) as i32;
    let bottom = py >= (y + h).saturating_sub(r) as i32;
    if !(left && top) && !(right && top) && !(left && bottom) && !(right && bottom) {
        return true;
    }
    let cx = if left { (x + r) as i32 } else { (x + w - r) as i32 };
    let cy = if top { (y + r) as i32 } else { (y + h - r) as i32 };
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= (r as i32) * (r as i32)
}

/// 3×3 超采样抗锯齿圆角矩形。内部直写，边缘/角做 coverage 混合。
pub fn round_rect(
    fb: &mut [u8], stride: usize, x: usize, y: usize,
    w: usize, h: usize, r: usize, color: [u8; 3],
) {
    if w == 0 || h == 0 {
        return;
    }
    let radius = r.min(w / 2).min(h / 2);
    let x1 = x + w;
    let y1 = y + h;
    let inner_x0 = x + radius;
    let inner_x1 = x1 - radius;
    let inner_y0 = y + radius;
    let inner_y1 = y1 - radius;
    for py in y.saturating_sub(1)..y1.saturating_add(1) {
        'px: for px in x.saturating_sub(1)..x1.saturating_add(1) {
            if px >= inner_x0 && px < inner_x1 && py >= inner_y0 && py < inner_y1 {
                blend_px(fb, stride, px, py, color, 255);
                continue;
            }
            let mut hits = 0usize;
            for sy in 0..3i32 {
                for sx in 0..3i32 {
                    let sample_x = px as i32 * 3 + sx * 2 + 1;
                    let sample_y = py as i32 * 3 + sy * 2 + 1;
                    // 采样坐标在 3x 缩放域：把几何也乘 3 后判定。
                    if rounded_inside(
                        x * 3, y * 3, w * 3, h * 3, radius * 3,
                        sample_x, sample_y,
                    ) {
                        hits += 1;
                    }
                }
            }
            if hits == 0 {
                continue 'px;
            }
            let a = ((hits * 255) / 9) as u8;
            blend_px(fb, stride, px, py, color, a);
        }
    }
}

/// 轻量卡片阴影：右下偏移的半透明圆角矩形。
pub fn shadow(
    fb: &mut [u8], stride: usize, x: usize, y: usize,
    w: usize, h: usize, r: usize,
) {
    // 两层低覆盖圆角矩形近似软阴影（外淡内深）。
    round_rect_alpha(fb, stride, x + 5, y + 9, w, h, r, [0, 0, 0], 18);
    round_rect_alpha(fb, stride, x + 3, y + 5, w, h, r, [0, 0, 0], 30);
}

/// 带 alpha 的圆角矩形（`shadow` 的内部低层）。
fn round_rect_alpha(
    fb: &mut [u8], stride: usize, x: usize, y: usize,
    w: usize, h: usize, r: usize, color: [u8; 3], alpha: u8,
) {
    if alpha == 0 || w == 0 || h == 0 {
        return;
    }
    let radius = r.min(w / 2).min(h / 2);
    let x1 = x + w;
    let y1 = y + h;
    for py in y..y1 {
        for px in x..x1 {
            if rounded_inside(x, y, w, h, radius, px as i32, py as i32) {
                blend_px(fb, stride, px, py, color, alpha);
            }
        }
    }
}

/// 3×3 超采样抗锯齿实心圆（几何中心按像素中心约定）。
pub fn circle(
    fb: &mut [u8], stride: usize, cx: usize, cy: usize,
    radius: usize, color: [u8; 3],
) {
    if radius == 0 {
        return;
    }
    let rr = (radius * 3) as i32 * (radius as i32 * 3);
    let x0 = cx.saturating_sub(radius + 1);
    let x1 = cx + radius + 1;
    let y0 = cy.saturating_sub(radius + 1);
    let y1 = cy + radius + 1;
    for py in y0..y1 {
        'py: for px in x0..x1 {
            let mut hits = 0usize;
            for sy in 0..3i32 {
                for sx in 0..3i32 {
                    let dx = px as i32 * 3 + sx * 2 + 1 - cx as i32 * 3;
                    let dy = py as i32 * 3 + sy * 2 + 1 - cy as i32 * 3;
                    if dx * dx + dy * dy <= rr {
                        hits += 1;
                    }
                }
            }
            if hits == 0 {
                continue 'py;
            }
            blend_px(fb, stride, px, py, color, ((hits * 255) / 9) as u8);
        }
    }
}
