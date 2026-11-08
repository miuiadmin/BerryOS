//! BerryGFX XRGB 表面基元（GUI F3）。
//!
//! 这里是 soft 直接表面路径的**形状单源**：straight-alpha 混合、3×3
//! 超采样抗锯齿圆角矩形/圆和轻量阴影。目标内存形态固定为窗口局部
//! XRGB little-endian：`[B, G, R, X]`、`stride` 单位字节、行尾 padding
//! 永不写。函数不认识窗口、主题或业务，只做纯像素几何。

use crate::cmd::Color;

#[inline]
fn offset(fb: &[u8], stride: usize, x: usize, y: usize) -> Option<usize> {
    let off = y.checked_mul(stride)?.checked_add(x.checked_mul(4)?)?;
    (off.checked_add(4)? <= fb.len()).then_some(off)
}

/// 8-bit straight-alpha 写一颗 XRGB 像素；a=0 不写。
pub fn blend_xrgb(
    fb: &mut [u8], stride: usize, x: usize, y: usize, color: Color, a: u8,
) {
    let Some(off) = offset(fb, stride, x, y) else { return };
    if a == 0 {
        return;
    }
    if a == 255 {
        fb[off] = color.b;
        fb[off + 1] = color.g;
        fb[off + 2] = color.r;
        fb[off + 3] = 0;
        return;
    }
    let aa = u16::from(a);
    let inv = u16::from(255 - a);
    let mix = |src: u8, dst: u8| -> u8 {
        ((u16::from(src) * aa + u16::from(dst) * inv + 127) / 255) as u8
    };
    let bg = [fb[off], fb[off + 1], fb[off + 2]];
    fb[off] = mix(color.b, bg[0]);
    fb[off + 1] = mix(color.g, bg[1]);
    fb[off + 2] = mix(color.r, bg[2]);
    fb[off + 3] = 0;
}

/// 圆 coverage 单源：像素中心按 6×域采样，3×3 样本折成 straight alpha。
/// raster 命令路径与直接 XRGB 表面路径共用，禁止两套圆算法。
pub(crate) fn circle_coverage(cx: i64, cy: i64, radius: i64, x: i32, y: i32) -> u8 {
    if radius <= 0 {
        return 0;
    }
    let Some(diameter_scale) = radius.checked_mul(6) else { return 0 };
    let Some(rr) = diameter_scale.checked_mul(diameter_scale) else { return 0 };
    let Some(cx6) = cx.checked_mul(6) else { return 0 };
    let Some(cy6) = cy.checked_mul(6) else { return 0 };
    let center_x = cx6.saturating_add(3);
    let center_y = cy6.saturating_add(3);
    let mut hits = 0usize;
    for sy in 0..3i64 {
        for sx in 0..3i64 {
            let sample_x = i64::from(x).saturating_mul(6).saturating_add(sx * 2 + 1);
            let sample_y = i64::from(y).saturating_mul(6).saturating_add(sy * 2 + 1);
            let dx = sample_x.saturating_sub(center_x);
            let dy = sample_y.saturating_sub(center_y);
            let dist = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));
            if dist <= rr {
                hits += 1;
            }
        }
    }
    ((hits * 255) / 9) as u8
}

fn rounded_inside(
    x: i64, y: i64, w: i64, h: i64, r: i64, px: i64, py: i64,
) -> bool {
    if px < x || py < y || px >= x.saturating_add(w) || py >= y.saturating_add(h) {
        return false;
    }
    let radius = r.min(w / 2).min(h / 2);
    let left = px < x.saturating_add(radius);
    let right = px >= x.saturating_add(w).saturating_sub(radius);
    let top = py < y.saturating_add(radius);
    let bottom = py >= y.saturating_add(h).saturating_sub(radius);
    if !(left && top) && !(right && top) && !(left && bottom) && !(right && bottom) {
        return true;
    }
    let cx = if left { x.saturating_add(radius) } else { x.saturating_add(w).saturating_sub(radius) };
    let cy = if top { y.saturating_add(radius) } else { y.saturating_add(h).saturating_sub(radius) };
    let dx = px.saturating_sub(cx);
    let dy = py.saturating_sub(cy);
    dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
        <= radius.saturating_mul(radius)
}

/// 3×3 超采样抗锯齿圆角矩形。半径自动夹到宽高半值；零宽/高为 no-op。
pub fn fill_round_rect_xrgb(
    fb: &mut [u8], stride: usize, x: usize, y: usize, w: usize, h: usize,
    radius: usize, color: Color,
) {
    if w == 0 || h == 0 {
        return;
    }
    let radius = radius.min(w / 2).min(h / 2);
    let x1 = x.saturating_add(w);
    let y1 = y.saturating_add(h);
    let to_i64 = |v: usize| i64::try_from(v).ok();
    let (Some(gx), Some(gy), Some(gw), Some(gh), Some(gr)) =
        (to_i64(x), to_i64(y), to_i64(w), to_i64(h), to_i64(radius)) else {
            return;
    };
    let (Some(gx3), Some(gy3), Some(gw3), Some(gh3), Some(gr3)) = (
        gx.checked_mul(3), gy.checked_mul(3), gw.checked_mul(3),
        gh.checked_mul(3), gr.checked_mul(3),
    ) else {
        return;
    };
    let inner_x0 = x.saturating_add(radius);
    let inner_x1 = x1.saturating_sub(radius);
    let inner_y0 = y.saturating_add(radius);
    let inner_y1 = y1.saturating_sub(radius);
    for py in y.saturating_sub(1)..y1.saturating_add(1) {
        'px: for px in x.saturating_sub(1)..x1.saturating_add(1) {
            if px >= inner_x0 && px < inner_x1 && py >= inner_y0 && py < inner_y1
            {
                blend_xrgb(fb, stride, px, py, color, 255);
                continue;
            }
            let mut hits = 0usize;
            for sy in 0..3i64 {
                for sx in 0..3i64 {
                    let Some(base_x) = i64::try_from(px).ok().and_then(|v| v.checked_mul(3)) else {
                        continue;
                    };
                    let Some(base_y) = i64::try_from(py).ok().and_then(|v| v.checked_mul(3)) else {
                        continue;
                    };
                    let sample_x = base_x.saturating_add(sx * 2 + 1);
                    let sample_y = base_y.saturating_add(sy * 2 + 1);
                    if rounded_inside(gx3, gy3, gw3, gh3, gr3, sample_x, sample_y) {
                        hits += 1;
                    }
                }
            }
            if hits == 0 {
                continue 'px;
            }
            let a = ((hits * 255) / 9) as u8;
            blend_xrgb(fb, stride, px, py, color, a);
        }
    }
}

/// 3×3 超采样抗锯齿实心圆；几何中心按像素中心约定。
pub fn fill_circle_xrgb(
    fb: &mut [u8], stride: usize, cx: usize, cy: usize, radius: usize,
    color: Color,
) {
    if radius == 0 {
        return;
    }
    let center_x = i64::try_from(cx).unwrap_or(0);
    let center_y = i64::try_from(cy).unwrap_or(0);
    let radius_i64 = i64::try_from(radius).unwrap_or(0);
    let x0 = cx.saturating_sub(radius.saturating_add(1));
    let x1 = cx.saturating_add(radius).saturating_add(1);
    let y0 = cy.saturating_sub(radius.saturating_add(1));
    let y1 = cy.saturating_add(radius).saturating_add(1);
    for py in y0..y1 {
        'py: for px in x0..x1 {
            let a = circle_coverage(
                center_x,
                center_y,
                radius_i64,
                px as i32,
                py as i32,
            );
            if a == 0 {
                continue 'py;
            }
            blend_xrgb(fb, stride, px, py, color, a);
        }
    }
}

fn blend_round_rect_alpha(
    fb: &mut [u8], stride: usize, x: usize, y: usize, w: usize, h: usize,
    radius: usize, color: Color, alpha: u8,
) {
    if alpha == 0 || w == 0 || h == 0 {
        return;
    }
    let radius = radius.min(w / 2).min(h / 2);
    let to_i64 = |v: usize| i64::try_from(v).ok();
    let (Some(gx), Some(gy), Some(gw), Some(gh), Some(gr)) =
        (to_i64(x), to_i64(y), to_i64(w), to_i64(h), to_i64(radius)) else {
            return;
    };
    for py in y..y.saturating_add(h) {
        for px in x..x.saturating_add(w) {
            if let (Some(sx), Some(sy)) = (i64::try_from(px).ok(), i64::try_from(py).ok()) {
                if rounded_inside(gx, gy, gw, gh, gr, sx, sy) {
                    blend_xrgb(fb, stride, px, py, color, alpha);
                }
            }
        }
    }
}

/// 轻量双层卡片阴影（右下偏移；外层淡、内层深）。
pub fn draw_shadow_xrgb(
    fb: &mut [u8], stride: usize, x: usize, y: usize, w: usize, h: usize,
    radius: usize,
) {
    let black = Color { r: 0, g: 0, b: 0 };
    blend_round_rect_alpha(fb, stride, x.saturating_add(5), y.saturating_add(9), w, h, radius, black, 18);
    blend_round_rect_alpha(fb, stride, x.saturating_add(3), y.saturating_add(5), w, h, radius, black, 30);
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRIDE: usize = 48;

    fn fb() -> [u8; 576] {
        [0xFF; 576]
    }

    fn px(buf: &[u8], x: usize, y: usize) -> (u8, u8, u8) {
        let off = y * STRIDE + x * 4;
        (buf[off + 2], buf[off + 1], buf[off])
    }

    #[test]
    fn blend_xrgb_matches_memory_order_and_alpha() {
        let mut b = fb();
        blend_xrgb(&mut b, STRIDE, 1, 1, Color { r: 10, g: 20, b: 30 }, 255);
        assert_eq!(px(&b, 1, 1), (10, 20, 30));
        blend_xrgb(&mut b, STRIDE, 1, 1, Color { r: 0, g: 0, b: 0 }, 128);
        let got = px(&b, 1, 1);
        assert_eq!(got, (5, 10, 15));
    }

    #[test]
    fn round_rect_center_is_opaque_and_corner_is_antialiased() {
        let mut b = fb();
        let c = Color { r: 0, g: 0, b: 0 };
        fill_round_rect_xrgb(&mut b, STRIDE, 1, 1, 8, 8, 3, c);
        assert_eq!(px(&b, 4, 4), (0, 0, 0));
        // 角外未写，几何内角经 3×3 coverage 混合。
        assert_eq!(px(&b, 0, 0), (255, 255, 255));
        let edge = px(&b, 1, 1);
        assert!(edge.0 < 255 && edge.0 > 0, "edge={edge:?}");
    }

    #[test]
    fn circle_center_opaque_and_boundary_has_coverage() {
        let mut b = fb();
        fill_circle_xrgb(&mut b, STRIDE, 3, 3, 2, Color { r: 255, g: 0, b: 0 });
        assert_eq!(px(&b, 3, 3), (255, 0, 0));
        assert_eq!(px(&b, 0, 0), (255, 255, 255));
        let boundary = px(&b, 5, 3);
        assert!(boundary.1 < 255 && boundary.1 > 0, "boundary={boundary:?}");
    }

    #[test]
    fn hostile_geometry_is_clipped_without_panic() {
        let mut b = fb();
        let c = Color { r: 1, g: 2, b: 3 };
        fill_round_rect_xrgb(&mut b, STRIDE, usize::MAX, usize::MAX, 2, 2, 1, c);
        fill_circle_xrgb(&mut b, STRIDE, usize::MAX, usize::MAX, 2, c);
        draw_shadow_xrgb(&mut b, STRIDE, usize::MAX, usize::MAX, 2, 2, 1);
        assert_eq!(px(&b, 0, 0), (255, 255, 255));
    }

    #[test]
    fn shadow_has_two_alpha_layers_without_opaque_write() {
        let mut b = fb();
        draw_shadow_xrgb(&mut b, STRIDE, 2, 2, 6, 6, 2);
        let channels = b[..576]
            .chunks_exact(4)
            .flat_map(|c| &c[..3])
            .copied()
            .filter(|v| *v != 255)
            .collect::<Vec<_>>();
        assert!(!channels.is_empty(), "阴影必须产生混合像素");
        assert!(channels.iter().all(|v| *v >= 192 && *v <= 240));
        assert_eq!(px(&b, 0, 0), (255, 255, 255));
    }
}
