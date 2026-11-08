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
                // F3：A8 文本 coverage 与 XRGB 混合也走 berry-gfx 单源。
                berry_gfx::surface::blend_xrgb(
                    fb,
                    stride,
                    px,
                    py,
                    berry_gfx::cmd::Color { r: color[0], g: color[1], b: color[2] },
                    a as u8,
                );
            }
        }
    }
}

/// 以 straight-alpha 写一颗 XRGB 像素。F3 起像素混合单源在 berry-gfx。
pub fn blend_px(
    fb: &mut [u8], stride: usize, x: usize, y: usize,
    color: [u8; 3], a: u8,
) {
    berry_gfx::surface::blend_xrgb(
        fb, stride, x, y,
        berry_gfx::cmd::Color { r: color[0], g: color[1], b: color[2] },
        a,
    );
}

/// 3×3 超采样抗锯齿圆角矩形。算法单源在 `berry-gfx::surface`。
pub fn round_rect(
    fb: &mut [u8], stride: usize, x: usize, y: usize,
    w: usize, h: usize, r: usize, color: [u8; 3],
) {
    berry_gfx::surface::fill_round_rect_xrgb(
        fb, stride, x, y, w, h, r,
        berry_gfx::cmd::Color { r: color[0], g: color[1], b: color[2] },
    );
}

/// 轻量卡片阴影。算法单源在 `berry-gfx::surface`。
pub fn shadow(
    fb: &mut [u8], stride: usize, x: usize, y: usize,
    w: usize, h: usize, r: usize,
) {
    berry_gfx::surface::draw_shadow_xrgb(fb, stride, x, y, w, h, r);
}

/// 3×3 超采样抗锯齿实心圆。算法单源在 `berry-gfx::surface`。
pub fn circle(
    fb: &mut [u8], stride: usize, cx: usize, cy: usize,
    radius: usize, color: [u8; 3],
) {
    berry_gfx::surface::fill_circle_xrgb(
        fb, stride, cx, cy, radius,
        berry_gfx::cmd::Color { r: color[0], g: color[1], b: color[2] },
    );
}
