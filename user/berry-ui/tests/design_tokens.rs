//! DESIGN-D2 host fixtures：配色 token 必须成对、三 scheme 全量过正文
//! 对比度门，text scale 走离散档，主题切换只改颜色不改几何。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_ui::sdk::{
    Button, ColorPair, ColorPairToken, ColorScheme, ColorSchemePreference, ContrastLevel,
    InteractionState, TextScale, Theme, ThemeMetrics, TypographyToken, WidgetContext,
    contrast_meets, contrast_ratio_centi, resolve_color_scheme,
};
use berry_ui::Ui;


const fn hex_nibble(byte: u8) -> u32 {
    match byte {
        b'0'..=b'9' => (byte - b'0') as u32,
        _ => (byte - b'a') as u32 + 10,
    }
}

const fn hex_channel(bytes: &[u8], index: usize) -> u8 {
    ((hex_nibble(bytes[index]) << 4) | hex_nibble(bytes[index + 1])) as u8
}

const fn color(hex: &str) -> Color {
    let bytes = hex.as_bytes();
    Color {
        r: hex_channel(bytes, 0),
        g: hex_channel(bytes, 2),
        b: hex_channel(bytes, 4),
    }
}

const COLOR_SCHEMES: [ColorScheme; 3] = [
    ColorScheme::Light,
    ColorScheme::Dark,
    ColorScheme::HighContrastLight,
];
const PAIR_TOKENS: [ColorPairToken; 7] = [
    ColorPairToken::Background,
    ColorPairToken::Surface,
    ColorPairToken::SurfaceVariant,
    ColorPairToken::Primary,
    ColorPairToken::Success,
    ColorPairToken::Warning,
    ColorPairToken::Danger,
];
const INTERACTION_STATES: [InteractionState; 5] = [
    InteractionState::Normal,
    InteractionState::Pressed,
    InteractionState::Disabled,
    InteractionState::Selected,
    InteractionState::Focused,
];

#[test]
fn all_scheme_pairs_and_interaction_states_pass_text_contrast() {
    let schemes = COLOR_SCHEMES;
    for scheme in schemes {
        let theme = Theme::for_scheme(scheme);
        for &token in PAIR_TOKENS.iter() {
            let pair = theme.pair(token);
            assert!(
                contrast_meets(pair, ContrastLevel::Text),
                "{:?}/{:?} contrast {}",
                scheme,
                token,
                contrast_ratio_centi(pair)
            );
        }
        for &state in INTERACTION_STATES.iter() {
            let pair = theme.state_pair(state);
            let level = if state == InteractionState::Disabled {
                ContrastLevel::Inactive
            } else {
                ContrastLevel::Text
            };
            assert!(
                contrast_meets(pair, level),
                "{:?}/{:?} contrast {}",
                scheme,
                state,
                contrast_ratio_centi(pair)
            );
        }
    }
}

#[test]
fn ledger_pairs_match_theme_bytes_and_q16_ratios_exactly() {
    const EXPECTED_PAIRS: [(ColorScheme, ColorPairToken, Color, Color, u32); 21] = [
        (
            ColorScheme::Light,
            ColorPairToken::Background,
            color("f5f7fa"),
            color("1b2540"),
            1413,
        ),
        (
            ColorScheme::Light,
            ColorPairToken::Surface,
            color("ffffff"),
            color("1b2540"),
            1516,
        ),
        (
            ColorScheme::Light,
            ColorPairToken::SurfaceVariant,
            color("e9edf3"),
            color("1b2540"),
            1290,
        ),
        (
            ColorScheme::Light,
            ColorPairToken::Primary,
            color("0074e0"),
            color("ffffff"),
            459,
        ),
        (
            ColorScheme::Light,
            ColorPairToken::Success,
            color("008b6f"),
            color("000000"),
            492,
        ),
        (
            ColorScheme::Light,
            ColorPairToken::Warning,
            color("fb8c00"),
            color("1b2540"),
            639,
        ),
        (
            ColorScheme::Light,
            ColorPairToken::Danger,
            color("d02f3d"),
            color("ffffff"),
            505,
        ),
        (
            ColorScheme::Dark,
            ColorPairToken::Background,
            color("0f141a"),
            color("edf2fa"),
            1646,
        ),
        (
            ColorScheme::Dark,
            ColorPairToken::Surface,
            color("1a212b"),
            color("edf2fa"),
            1441,
        ),
        (
            ColorScheme::Dark,
            ColorPairToken::SurfaceVariant,
            color("242d3a"),
            color("edf2fa"),
            1236,
        ),
        (
            ColorScheme::Dark,
            ColorPairToken::Primary,
            color("8abeff"),
            color("0f1724"),
            932,
        ),
        (
            ColorScheme::Dark,
            ColorPairToken::Success,
            color("62d8b5"),
            color("071b16"),
            1020,
        ),
        (
            ColorScheme::Dark,
            ColorPairToken::Warning,
            color("ffc66d"),
            color("221200"),
            1174,
        ),
        (
            ColorScheme::Dark,
            ColorPairToken::Danger,
            color("ff8a95"),
            color("2a0608"),
            826,
        ),
        (
            ColorScheme::HighContrastLight,
            ColorPairToken::Background,
            color("ffffff"),
            color("000000"),
            2100,
        ),
        (
            ColorScheme::HighContrastLight,
            ColorPairToken::Surface,
            color("ffffff"),
            color("000000"),
            2100,
        ),
        (
            ColorScheme::HighContrastLight,
            ColorPairToken::SurfaceVariant,
            color("f2f5fa"),
            color("000000"),
            1922,
        ),
        (
            ColorScheme::HighContrastLight,
            ColorPairToken::Primary,
            color("003e9c"),
            color("ffffff"),
            968,
        ),
        (
            ColorScheme::HighContrastLight,
            ColorPairToken::Success,
            color("00694f"),
            color("ffffff"),
            670,
        ),
        (
            ColorScheme::HighContrastLight,
            ColorPairToken::Warning,
            color("8a4b00"),
            color("ffffff"),
            680,
        ),
        (
            ColorScheme::HighContrastLight,
            ColorPairToken::Danger,
            color("9e1b27"),
            color("ffffff"),
            796,
        ),
    ];

    const EXPECTED_STATES: [(ColorScheme, InteractionState, Color, Color, u32); 15] = [
        (
            ColorScheme::Light,
            InteractionState::Normal,
            color("ffffff"),
            color("1b2540"),
            1516,
        ),
        (
            ColorScheme::Light,
            InteractionState::Pressed,
            color("e1eaf8"),
            color("172b49"),
            1172,
        ),
        (
            ColorScheme::Light,
            InteractionState::Disabled,
            color("f0f2f5"),
            color("a6adba"),
            201,
        ),
        (
            ColorScheme::Light,
            InteractionState::Selected,
            color("d6e4ff"),
            color("102a66"),
            1062,
        ),
        (
            ColorScheme::Light,
            InteractionState::Focused,
            color("e9f2ff"),
            color("0b2554"),
            1323,
        ),
        (
            ColorScheme::Dark,
            InteractionState::Normal,
            color("222b38"),
            color("e7eef9"),
            1224,
        ),
        (
            ColorScheme::Dark,
            InteractionState::Pressed,
            color("2d3a4c"),
            color("dce8fa"),
            932,
        ),
        (
            ColorScheme::Dark,
            InteractionState::Disabled,
            color("1e252f"),
            color("8e9aab"),
            541,
        ),
        (
            ColorScheme::Dark,
            InteractionState::Selected,
            color("31415c"),
            color("dceafb"),
            843,
        ),
        (
            ColorScheme::Dark,
            InteractionState::Focused,
            color("263447"),
            color("e5f0ff"),
            1096,
        ),
        (
            ColorScheme::HighContrastLight,
            InteractionState::Normal,
            color("ffffff"),
            color("000000"),
            2100,
        ),
        (
            ColorScheme::HighContrastLight,
            InteractionState::Pressed,
            color("e7eefa"),
            color("001a4d"),
            1435,
        ),
        (
            ColorScheme::HighContrastLight,
            InteractionState::Disabled,
            color("f3f4f7"),
            color("5a626e"),
            560,
        ),
        (
            ColorScheme::HighContrastLight,
            InteractionState::Selected,
            color("dce6ff"),
            color("001444"),
            1420,
        ),
        (
            ColorScheme::HighContrastLight,
            InteractionState::Focused,
            color("e1ebff"),
            color("001040"),
            1527,
        ),
    ];

    for scheme in COLOR_SCHEMES {
        for token in PAIR_TOKENS {
            assert_eq!(
                EXPECTED_PAIRS
                    .iter()
                    .filter(|(want_scheme, want_token, _, _, _)| {
                        *want_scheme == scheme && *want_token == token
                    })
                    .count(),
                1,
                "{scheme:?}/{token:?} coverage"
            );
        }
    }
    for scheme in COLOR_SCHEMES {
        for state in INTERACTION_STATES {
            assert_eq!(
                EXPECTED_STATES
                    .iter()
                    .filter(|(want_scheme, want_state, _, _, _)| {
                        *want_scheme == scheme && *want_state == state
                    })
                    .count(),
                1,
                "{scheme:?}/{state:?} coverage"
            );
        }
    }
    for (scheme, token, background, foreground, expected_ratio) in EXPECTED_PAIRS {
        let pair = Theme::for_scheme(scheme).pair(token);
        assert_eq!(pair.background, background, "{scheme:?}/{token:?} background");
        assert_eq!(pair.foreground, foreground, "{scheme:?}/{token:?} foreground");
        assert_eq!(contrast_ratio_centi(pair), expected_ratio, "{scheme:?}/{token:?}");
    }
    for (scheme, state, background, foreground, expected_ratio) in EXPECTED_STATES {
        let pair = Theme::for_scheme(scheme).state_pair(state);
        assert_eq!(pair.background, background, "{scheme:?}/{state:?} background");
        assert_eq!(pair.foreground, foreground, "{scheme:?}/{state:?} foreground");
        assert_eq!(contrast_ratio_centi(pair), expected_ratio, "{scheme:?}/{state:?}");
    }
}

#[test]
fn contrast_ratio_is_wcag_reference_values() {
    let black_white = ColorPair {
        background: berry_gfx::cmd::Color { r: 0, g: 0, b: 0 },
        foreground: berry_gfx::cmd::Color { r: 255, g: 255, b: 255 },
    };
    assert_eq!(contrast_ratio_centi(black_white), 2100);
    assert_eq!(ContrastLevel::Text.minimum_centi(), 450);
    assert_eq!(ContrastLevel::NonText.minimum_centi(), 300);
    assert_eq!(ContrastLevel::Inactive.minimum_centi(), 0);

    let light_primary = Theme::light().pair(ColorPairToken::Primary);
    assert_eq!(contrast_ratio_centi(light_primary), 459);
    let dark_primary = Theme::dark().pair(ColorPairToken::Primary);
    assert_eq!(contrast_ratio_centi(dark_primary), 932);
    let high_primary = Theme::high_contrast_light().pair(ColorPairToken::Primary);
    assert_eq!(contrast_ratio_centi(high_primary), 968);
}

#[test]
fn color_scheme_preferences_are_pure_and_fail_closed_to_snapshot() {
    assert_eq!(
        resolve_color_scheme(ColorSchemePreference::System, ColorScheme::Dark),
        ColorScheme::Dark
    );
    assert_eq!(
        resolve_color_scheme(ColorSchemePreference::Light, ColorScheme::Dark),
        ColorScheme::Light
    );
    assert_eq!(
        resolve_color_scheme(ColorSchemePreference::HighContrastLight, ColorScheme::Dark),
        ColorScheme::HighContrastLight
    );
    for left in 0..COLOR_SCHEMES.len() {
        for right in (left + 1)..COLOR_SCHEMES.len() {
            assert_ne!(
                Theme::for_scheme(COLOR_SCHEMES[left]),
                Theme::for_scheme(COLOR_SCHEMES[right])
            );
        }
    }
}

#[test]
fn text_scale_is_a_nonlinear_font_tier_ladder() {
    let compact = ThemeMetrics::new(berry_ui::sdk::Density::Compact);
    let product = ThemeMetrics::new(berry_ui::sdk::Density::Product);

    assert_eq!(
        (
            compact.typography_with_scale(TypographyToken::Body, TextScale::Standard),
            compact.typography_with_scale(TypographyToken::Body, TextScale::Large),
            compact.typography_with_scale(TypographyToken::Body, TextScale::Maximum),
        ),
        (FontSize::S16x32, FontSize::S24x48, FontSize::S32x48)
    );
    assert_eq!(
        (
            compact.typography_with_scale(TypographyToken::Caption, TextScale::Standard),
            compact.typography_with_scale(TypographyToken::Caption, TextScale::Large),
            compact.typography_with_scale(TypographyToken::Caption, TextScale::Maximum),
        ),
        (FontSize::S8x16, FontSize::S16x32, FontSize::S24x48)
    );
    // Product Body 已在 32px 资产档；放大档饱和而不是伪造不存在的线性字号。
    assert_eq!(
        (
            product.typography_with_scale(TypographyToken::Body, TextScale::Standard),
            product.typography_with_scale(TypographyToken::Body, TextScale::Large),
            product.typography_with_scale(TypographyToken::Body, TextScale::Maximum),
        ),
        (FontSize::S32x48, FontSize::S32x48, FontSize::S32x48)
    );
    assert_eq!(
        (
            compact.typography_with_scale(TypographyToken::Display, TextScale::Standard),
            compact.typography_with_scale(TypographyToken::Display, TextScale::Large),
            compact.typography_with_scale(TypographyToken::Display, TextScale::Maximum),
        ),
        (FontSize::S24x48, FontSize::S32x48, FontSize::S32x48)
    );
    assert_eq!(
        (
            product.typography_with_scale(TypographyToken::Caption, TextScale::Standard),
            product.typography_with_scale(TypographyToken::Caption, TextScale::Large),
            product.typography_with_scale(TypographyToken::Caption, TextScale::Maximum),
        ),
        (FontSize::S24x48, FontSize::S32x48, FontSize::S32x48)
    );
}

#[test]
fn effective_pixel_grid_is_quarter_vp_rounded_single_source() {
    let compact = ThemeMetrics::new(berry_ui::sdk::Density::Compact);
    let product = ThemeMetrics::new(berry_ui::sdk::Density::Product);
    assert_eq!((compact.effective_px(4), product.effective_px(4)), (2, 3));
    assert_eq!((compact.effective_px(6), product.effective_px(6)), (3, 5));
    assert_eq!((compact.effective_px(1), product.effective_px(1)), (1, 1));
}

#[test]
fn theme_swap_preserves_geometry_and_changes_visual_pair() {
    let mut ui = Ui::<64>::new(320, 240);
    let light_ctx = WidgetContext::for_viewport(Theme::light(), 320, 240);
    let root = ui.root();
    let mut button = Button::new(
        &mut ui,
        root,
        &light_ctx,
        b"OK",
        InteractionState::Normal,
    )
    .unwrap();
    let _ = ui.layout();
    let before = ui.bounds(button.panel).unwrap();

    let dark_ctx = WidgetContext::new(Theme::dark(), light_ctx.metrics);
    button
        .set_base(&mut ui, &dark_ctx, InteractionState::Normal)
        .unwrap();
    let _ = ui.layout();
    let after = ui.bounds(button.panel).unwrap();
    assert_eq!(before, after);
    assert_ne!(
        light_ctx.theme.state_pair(InteractionState::Normal),
        dark_ctx.theme.state_pair(InteractionState::Normal)
    );
    assert!(contrast_meets(
        dark_ctx.theme.state_pair(InteractionState::Normal),
        ContrastLevel::Text
    ));
}
