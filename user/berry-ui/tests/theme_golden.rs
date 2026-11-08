//! GUI F3：Berry Design token + BerryGFX 表面基元的双几何离屏 golden。
//!
//! 同一个“状态卡片”描述分别在 800×600（@2x）与 1080×1920（@3x）
//! 渲染。几何只来自 ThemeMetrics；颜色/状态只来自 Theme；圆角/边缘混合
//! 只来自 berry-gfx::surface。测试侧不携带第二套视觉常量。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_gfx::surface::{blend_xrgb, fill_round_rect_xrgb};
use berry_ui::sdk::{
    Density, InteractionState, RadiusToken, SpacingToken, Theme, ThemeMetrics, TypographyToken,
};

#[derive(Clone, Copy)]
struct StateCard {
    state: InteractionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CardGeometry {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    radius: usize,
}

fn render_card(
    description: StateCard, viewport_w: usize, viewport_h: usize,
) -> (Vec<u8>, CardGeometry, ThemeMetrics) {
    let theme = Theme::light();
    let metrics = theme.metrics_for_viewport(viewport_w as u32, viewport_h as u32);
    let margin = metrics.spacing(SpacingToken::L) as usize;
    let width = viewport_w.saturating_sub(margin * 2);
    let height = metrics.spacing(SpacingToken::Xl) as usize * 4;
    let geometry = CardGeometry {
        x: margin,
        y: margin,
        w: width,
        h: height,
        radius: metrics.radius(RadiusToken::Medium) as usize,
    };

    let stride = viewport_w * 4;
    let mut fb = vec![0xFF; stride * viewport_h];
    fill_round_rect_xrgb(
        &mut fb, stride, 0, 0, viewport_w, viewport_h, 0,
        theme.colors.background,
    );
    let state = theme.state(description.state);
    fill_round_rect_xrgb(
        &mut fb, stride, geometry.x, geometry.y, geometry.w, geometry.h,
        geometry.radius, state.background,
    );
    // 语义内容色经一颗中心锚点落到画面，证明 state 三件不是纸面 token。
    blend_xrgb(
        &mut fb, stride,
        geometry.x + geometry.w / 2,
        geometry.y + geometry.h / 2,
        state.content, 255,
    );
    (fb, geometry, metrics)
}

fn px(fb: &[u8], stride: usize, x: usize, y: usize) -> Color {
    let off = y * stride + x * 4;
    Color { r: fb[off + 2], g: fb[off + 1], b: fb[off] }
}

#[test]
fn same_card_description_scales_from_debug_to_product() {
    let theme = Theme::light();
    let description = StateCard { state: InteractionState::Normal };

    let (debug_fb, debug_geo, debug_metrics) = render_card(description, 800, 600);
    let (product_fb, product_geo, product_metrics) = render_card(description, 1080, 1920);

    assert_eq!(debug_metrics.density, Density::Compact);
    assert_eq!(product_metrics.density, Density::Product);
    assert_eq!(
        (debug_geo.x, debug_geo.y, debug_geo.w, debug_geo.h, debug_geo.radius),
        (16, 16, 768, 96, 24),
        "800×600 @2x 卡片几何"
    );
    assert_eq!(
        (product_geo.x, product_geo.y, product_geo.w, product_geo.h, product_geo.radius),
        (24, 24, 1032, 144, 36),
        "1080×1920 @3x 卡片几何"
    );
    assert_eq!(
        (debug_metrics.typography(TypographyToken::Title), product_metrics.typography(TypographyToken::Title)),
        (FontSize::S24x48, FontSize::S32x48)
    );

    let background = theme.colors.background;
    let card = theme.state(InteractionState::Normal).background;
    assert_eq!(px(&debug_fb, 800 * 4, 0, 0), background);
    assert_eq!(px(&product_fb, 1080 * 4, 0, 0), background);
    // 半径内侧一点是卡片面；同一描述在两档都命中。
    assert_eq!(px(&debug_fb, 800 * 4, debug_geo.x + debug_geo.radius + 2, debug_geo.y + 2), card);
    assert_eq!(px(&product_fb, 1080 * 4, product_geo.x + product_geo.radius + 2, product_geo.y + 2), card);
    // 圆角外仍是背景，圆角带存在连续 alpha 层级。
    assert_eq!(px(&debug_fb, 800 * 4, debug_geo.x - 1, debug_geo.y - 1), background);
    let corner = px(&debug_fb, 800 * 4, debug_geo.x + 6, debug_geo.y + 6);
    assert_ne!(corner, background);
    assert_ne!(corner, card);
}

#[test]
fn interaction_state_golden_changes_only_state_truth() {
    let normal = render_card(StateCard { state: InteractionState::Normal }, 1080, 1920);
    let pressed = render_card(StateCard { state: InteractionState::Pressed }, 1080, 1920);
    let theme = Theme::light();

    assert_eq!(normal.1, pressed.1, "状态变化不得改变布局几何");
    let stride = 1080 * 4;
    let anchor = (normal.1.x + normal.1.radius + 2, normal.1.y + 2);
    assert_eq!(px(&normal.0, stride, anchor.0, anchor.1), theme.state(InteractionState::Normal).background);
    assert_eq!(px(&pressed.0, stride, anchor.0, anchor.1), theme.state(InteractionState::Pressed).background);
}
