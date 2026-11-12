//! Berry Design 主题 token（GUI F3）。
//!
//! 本模块是 BerryUI 组件的视觉常量单源：颜色、控件状态、间距、圆角、
//! 排版档与密度换算。它不绘制、不布局、不认识窗口；F4 组件只能消费
//! 这些 token，不得在 app/组件内散装硬编码视觉常量。
//!
//! 密度口径：1080×1920 phone = 360×640vp @3x；800×600 调试档按 @2x
//! 折算。间距/圆角 token 以 quarter-vp 存储，换算使用四舍五入，避免
//! 不同 token 共享同一像素值时产生系统性左偏。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;

/// 视觉密度档。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    /// 800×600 调试档：@2x。
    Compact,
    /// 1080×1920 phone 档：360×640vp @3x。
    Product,
}

impl Density {
    /// 按现行产品口径选择密度：宽 ≥1000 且高 ≥1800 才进 phone 档。
    pub const fn for_viewport(w: u32, h: u32) -> Self {
        if w >= 1000 && h >= 1800 {
            Self::Product
        } else {
            Self::Compact
        }
    }

    pub const fn px(self) -> u32 {
        match self {
            Self::Compact => 2,
            Self::Product => 3,
        }
    }
}

/// 语义颜色集合。字段是稳定 token 面；具体色值可在 Berry Design 版本内
/// 原地修订，组件不得复制字面量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    pub background: Color,
    pub surface: Color,
    pub surface_variant: Color,
    pub outline: Color,
    pub primary: Color,
    pub on_primary: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
}

/// 单个交互状态的背景/内容/边框三件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateColors {
    pub background: Color,
    pub content: Color,
    pub border: Color,
}

/// 交互状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionState {
    Normal,
    Pressed,
    Disabled,
    Selected,
    Focused,
}

/// 间距 token。数值单位是 quarter-vp，非像素。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacingToken {
    Xs,
    S,
    M,
    L,
    Xl,
}

/// 圆角 token。数值单位是 quarter-vp，非像素。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadiusToken {
    Small,
    Medium,
    Large,
}

/// 线宽 token。Thin 以 half-vp 存储时在 @2x/@3x 分别折 1px/2px。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeToken {
    Thin,
}

/// 排版 token。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypographyToken {
    Display,
    Title,
    Body,
    Caption,
}

/// D2 配色方案。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
    HighContrastLight,
}

/// 应用/系统偏好到实际方案的纯函数解析。forced 形态优先；System 跟随
/// 系统快照。v1 不新增配置通道，快照仍由 app/宿主在启动时提供。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSchemePreference {
    System,
    Light,
    Dark,
    HighContrastLight,
}

pub const fn resolve_color_scheme(
    preference: ColorSchemePreference,
    system: ColorScheme,
) -> ColorScheme {
    match preference {
        ColorSchemePreference::System => system,
        ColorSchemePreference::Light => ColorScheme::Light,
        ColorSchemePreference::Dark => ColorScheme::Dark,
        ColorSchemePreference::HighContrastLight => ColorScheme::HighContrastLight,
    }
}

/// 必须成对消费的背景/前景 token。D2 之后新增视觉面不得只暴露孤立
/// background；F3 legacy `ThemeColors` 保留为兼容薄面。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorPair {
    pub background: Color,
    pub foreground: Color,
}

/// 主题级背景 pair 词表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPairToken {
    Background,
    Surface,
    SurfaceVariant,
    Primary,
    Success,
    Warning,
    Danger,
}

/// WCAG 对比度阈值，单位百分之一。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContrastLevel {
    /// 图形/大字最低 3.0:1。
    NonText,
    /// 正文最低 4.5:1；D2 全部 active 文本 pair 以此为默认门。
    Text,
    /// 失能组件按 WCAG inactive-component 豁免；仍进入 pair/夹具，
    /// 显式声明而不是隐式跳过。
    Inactive,
}

impl ContrastLevel {
    pub const fn minimum_centi(self) -> u32 {
        match self {
            Self::NonText => 300,
            Self::Text => 450,
            Self::Inactive => 0,
        }
    }
}

/// WCAG sRGB 线性化 LUT（Q16）。用表驱动避免 no_std `powf`/浮点依赖；
/// 数值按 0..=255 预计算，未消费者可被 LTO 剥离。
const LINEAR_RGB_Q16: [u16; 256] = [
    0, 20, 40, 60, 80, 99, 119, 139,
    159, 179, 199, 219, 241, 264, 288, 313,
    340, 367, 396, 427, 458, 491, 526, 562,
    599, 637, 677, 718, 761, 805, 851, 898,
    947, 997, 1048, 1101, 1156, 1212, 1270, 1330,
    1391, 1453, 1517, 1583, 1651, 1720, 1790, 1863,
    1937, 2013, 2090, 2170, 2250, 2333, 2418, 2504,
    2592, 2681, 2773, 2866, 2961, 3058, 3157, 3258,
    3360, 3464, 3570, 3678, 3788, 3900, 4014, 4129,
    4247, 4366, 4488, 4611, 4736, 4864, 4993, 5124,
    5257, 5392, 5530, 5669, 5810, 5953, 6099, 6246,
    6395, 6547, 6700, 6856, 7014, 7174, 7335, 7500,
    7666, 7834, 8004, 8177, 8352, 8528, 8708, 8889,
    9072, 9258, 9445, 9635, 9828, 10022, 10219, 10417,
    10619, 10822, 11028, 11235, 11446, 11658, 11873, 12090,
    12309, 12530, 12754, 12980, 13209, 13440, 13673, 13909,
    14146, 14387, 14629, 14874, 15122, 15371, 15623, 15878,
    16135, 16394, 16656, 16920, 17187, 17456, 17727, 18001,
    18277, 18556, 18837, 19121, 19407, 19696, 19987, 20281,
    20577, 20876, 21177, 21481, 21787, 22096, 22407, 22721,
    23038, 23357, 23678, 24002, 24329, 24658, 24990, 25325,
    25662, 26001, 26344, 26688, 27036, 27386, 27739, 28094,
    28452, 28813, 29176, 29542, 29911, 30282, 30656, 31033,
    31412, 31794, 32179, 32567, 32957, 33350, 33745, 34143,
    34544, 34948, 35355, 35764, 36176, 36591, 37008, 37429,
    37852, 38278, 38706, 39138, 39572, 40009, 40449, 40891,
    41337, 41785, 42236, 42690, 43147, 43606, 44069, 44534,
    45002, 45473, 45947, 46423, 46903, 47385, 47871, 48359,
    48850, 49344, 49841, 50341, 50844, 51349, 51858, 52369,
    52884, 53401, 53921, 54445, 54971, 55500, 56032, 56567,
    57105, 57646, 58190, 58737, 59287, 59840, 60396, 60955,
    61517, 62082, 62650, 63221, 63795, 64372, 64952, 65535,
];

/// WCAG 2.x 相对亮度（sRGB→linear→Y）的 Q16 值。
const fn relative_luminance_q16(color: Color) -> u32 {
    (LINEAR_RGB_Q16[color.r as usize] as u32 * 2126
        + LINEAR_RGB_Q16[color.g as usize] as u32 * 7152
        + LINEAR_RGB_Q16[color.b as usize] as u32 * 722)
        / 10000
}

/// WCAG 对比度，返回百分之一（x100）。输入 pair 顺序无关；
/// 黑/白参考值恰为 2100。
pub const fn contrast_ratio_centi(pair: ColorPair) -> u32 {
    let a = relative_luminance_q16(pair.foreground) + 3277; // +0.05 in Q16
    let b = relative_luminance_q16(pair.background) + 3277;
    let (light, dark) = if a >= b { (a, b) } else { (b, a) };
    (light * 100 + dark / 2) / dark
}

pub const fn contrast_meets(pair: ColorPair, level: ContrastLevel) -> bool {
    contrast_ratio_centi(pair) >= level.minimum_centi()
}


/// 非线性文本缩放档。映射到既有离散 `FontSize` 档，不做连续线性乘法；
/// 到达当前字形资产上限时饱和，布局必须重新测量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextScale {
    Standard,
    Large,
    Maximum,
}

/// Berry Design 主题。`light/dark/high_contrast_light` 是同级构造口；
/// 组件仍消费 `ThemeColors`/`StateColors`，不认识窗口或配置事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub scheme: ColorScheme,
    pub colors: ThemeColors,
}

impl Theme {
    pub const fn light() -> Self {
        Self {
            scheme: ColorScheme::Light,
            colors: ThemeColors {
                background: Color { r: 0xF5, g: 0xF7, b: 0xFA },
                surface: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                surface_variant: Color { r: 0xE9, g: 0xED, b: 0xF3 },
                outline: Color { r: 0xDC, g: 0xE1, b: 0xE8 },
                primary: Color { r: 0x00, g: 0x74, b: 0xE0 },
                on_primary: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                text_primary: Color { r: 0x1B, g: 0x25, b: 0x40 },
                text_secondary: Color { r: 0x4F, g: 0x5D, b: 0x75 },
                success: Color { r: 0x00, g: 0x8B, b: 0x6F },
                warning: Color { r: 0xFB, g: 0x8C, b: 0x00 },
                danger: Color { r: 0xD0, g: 0x2F, b: 0x3D },
            },
        }
    }

    pub const fn dark() -> Self {
        Self {
            scheme: ColorScheme::Dark,
            colors: ThemeColors {
                background: Color { r: 0x0F, g: 0x14, b: 0x1A },
                surface: Color { r: 0x1A, g: 0x21, b: 0x2B },
                surface_variant: Color { r: 0x24, g: 0x2D, b: 0x3A },
                outline: Color { r: 0x39, g: 0x44, b: 0x53 },
                primary: Color { r: 0x8A, g: 0xBE, b: 0xFF },
                on_primary: Color { r: 0x0F, g: 0x17, b: 0x24 },
                text_primary: Color { r: 0xED, g: 0xF2, b: 0xFA },
                text_secondary: Color { r: 0xB9, g: 0xC4, b: 0xD4 },
                success: Color { r: 0x62, g: 0xD8, b: 0xB5 },
                warning: Color { r: 0xFF, g: 0xC6, b: 0x6D },
                danger: Color { r: 0xFF, g: 0x8A, b: 0x95 },
            },
        }
    }

    pub const fn high_contrast_light() -> Self {
        Self {
            scheme: ColorScheme::HighContrastLight,
            colors: ThemeColors {
                background: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                surface: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                surface_variant: Color { r: 0xF2, g: 0xF5, b: 0xFA },
                outline: Color { r: 0x6A, g: 0x75, b: 0x85 },
                primary: Color { r: 0x00, g: 0x3E, b: 0x9C },
                on_primary: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                text_primary: Color { r: 0x00, g: 0x00, b: 0x00 },
                text_secondary: Color { r: 0x38, g: 0x41, b: 0x52 },
                success: Color { r: 0x00, g: 0x69, b: 0x4F },
                warning: Color { r: 0x8A, g: 0x4B, b: 0x00 },
                danger: Color { r: 0x9E, g: 0x1B, b: 0x27 },
            },
        }
    }

    pub const fn for_scheme(scheme: ColorScheme) -> Self {
        match scheme {
            ColorScheme::Light => Self::light(),
            ColorScheme::Dark => Self::dark(),
            ColorScheme::HighContrastLight => Self::high_contrast_light(),
        }
    }

    pub const fn pair(&self, token: ColorPairToken) -> ColorPair {
        match token {
            ColorPairToken::Background => ColorPair {
                background: self.colors.background,
                foreground: self.colors.text_primary,
            },
            ColorPairToken::Surface => ColorPair {
                background: self.colors.surface,
                foreground: self.colors.text_primary,
            },
            ColorPairToken::SurfaceVariant => ColorPair {
                background: self.colors.surface_variant,
                foreground: self.colors.text_primary,
            },
            ColorPairToken::Primary => ColorPair {
                background: self.colors.primary,
                foreground: self.colors.on_primary,
            },
            ColorPairToken::Success => match self.scheme {
                ColorScheme::Light => ColorPair {
                    background: self.colors.success,
                    foreground: Color { r: 0x00, g: 0x00, b: 0x00 },
                },
                ColorScheme::Dark => ColorPair {
                    background: self.colors.success,
                    foreground: Color { r: 0x07, g: 0x1B, b: 0x16 },
                },
                ColorScheme::HighContrastLight => ColorPair {
                    background: self.colors.success,
                    foreground: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                },
            },
            ColorPairToken::Warning => match self.scheme {
                ColorScheme::Light => ColorPair {
                    background: self.colors.warning,
                    foreground: self.colors.text_primary,
                },
                ColorScheme::Dark => ColorPair {
                    background: self.colors.warning,
                    foreground: Color { r: 0x22, g: 0x12, b: 0x00 },
                },
                ColorScheme::HighContrastLight => ColorPair {
                    background: self.colors.warning,
                    foreground: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                },
            },
            ColorPairToken::Danger => match self.scheme {
                ColorScheme::Light | ColorScheme::HighContrastLight => ColorPair {
                    background: self.colors.danger,
                    foreground: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                },
                ColorScheme::Dark => ColorPair {
                    background: self.colors.danger,
                    foreground: Color { r: 0x2A, g: 0x06, b: 0x08 },
                },
            },
        }
    }

    pub const fn state_pair(&self, state: InteractionState) -> ColorPair {
        let colors = self.state(state);
        ColorPair { background: colors.background, foreground: colors.content }
    }

    pub const fn state(&self, state: InteractionState) -> StateColors {
        match self.scheme {
            ColorScheme::Light => match state {
                InteractionState::Normal => StateColors {
                    background: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                    content: Color { r: 0x1B, g: 0x25, b: 0x40 },
                    border: Color { r: 0xDC, g: 0xE1, b: 0xE8 },
                },
                InteractionState::Pressed => StateColors {
                    background: Color { r: 0xE1, g: 0xEA, b: 0xF8 },
                    content: Color { r: 0x17, g: 0x2B, b: 0x49 },
                    border: Color { r: 0x9F, g: 0xC0, b: 0xF0 },
                },
                InteractionState::Disabled => StateColors {
                    background: Color { r: 0xF0, g: 0xF2, b: 0xF5 },
                    content: Color { r: 0xA6, g: 0xAD, b: 0xBA },
                    border: Color { r: 0xE3, g: 0xE7, b: 0xEC },
                },
                InteractionState::Selected => StateColors {
                    background: Color { r: 0xD6, g: 0xE4, b: 0xFF },
                    content: Color { r: 0x10, g: 0x2A, b: 0x66 },
                    border: Color { r: 0x9D, g: 0xBB, b: 0xFF },
                },
                InteractionState::Focused => StateColors {
                    background: Color { r: 0xE9, g: 0xF2, b: 0xFF },
                    content: Color { r: 0x0B, g: 0x25, b: 0x54 },
                    border: Color { r: 0x00, g: 0x74, b: 0xE0 },
                },
            },
            ColorScheme::Dark => match state {
                InteractionState::Normal => StateColors {
                    background: Color { r: 0x22, g: 0x2B, b: 0x38 },
                    content: Color { r: 0xE7, g: 0xEE, b: 0xF9 },
                    border: Color { r: 0x47, g: 0x56, b: 0x69 },
                },
                InteractionState::Pressed => StateColors {
                    background: Color { r: 0x2D, g: 0x3A, b: 0x4C },
                    content: Color { r: 0xDC, g: 0xE8, b: 0xFA },
                    border: Color { r: 0x8A, g: 0xBE, b: 0xFF },
                },
                InteractionState::Disabled => StateColors {
                    background: Color { r: 0x1E, g: 0x25, b: 0x2F },
                    content: Color { r: 0x8E, g: 0x9A, b: 0xAB },
                    border: Color { r: 0x3B, g: 0x46, b: 0x56 },
                },
                InteractionState::Selected => StateColors {
                    background: Color { r: 0x31, g: 0x41, b: 0x5C },
                    content: Color { r: 0xDC, g: 0xEA, b: 0xFB },
                    border: Color { r: 0x8A, g: 0xBE, b: 0xFF },
                },
                InteractionState::Focused => StateColors {
                    background: Color { r: 0x26, g: 0x34, b: 0x47 },
                    content: Color { r: 0xE5, g: 0xF0, b: 0xFF },
                    border: Color { r: 0x8A, g: 0xBE, b: 0xFF },
                },
            },
            ColorScheme::HighContrastLight => match state {
                InteractionState::Normal => StateColors {
                    background: Color { r: 0xFF, g: 0xFF, b: 0xFF },
                    content: Color { r: 0x00, g: 0x00, b: 0x00 },
                    border: Color { r: 0x6A, g: 0x75, b: 0x85 },
                },
                InteractionState::Pressed => StateColors {
                    background: Color { r: 0xE7, g: 0xEE, b: 0xFA },
                    content: Color { r: 0x00, g: 0x1A, b: 0x4D },
                    border: Color { r: 0x00, g: 0x3E, b: 0x9C },
                },
                InteractionState::Disabled => StateColors {
                    background: Color { r: 0xF3, g: 0xF4, b: 0xF7 },
                    content: Color { r: 0x5A, g: 0x62, b: 0x6E },
                    border: Color { r: 0x76, g: 0x7E, b: 0x8A },
                },
                InteractionState::Selected => StateColors {
                    background: Color { r: 0xDC, g: 0xE6, b: 0xFF },
                    content: Color { r: 0x00, g: 0x14, b: 0x44 },
                    border: Color { r: 0x00, g: 0x3E, b: 0x9C },
                },
                InteractionState::Focused => StateColors {
                    background: Color { r: 0xE1, g: 0xEB, b: 0xFF },
                    content: Color { r: 0x00, g: 0x10, b: 0x40 },
                    border: Color { r: 0x00, g: 0x3E, b: 0x9C },
                },
            },
        }
    }

    /// quarter-vp token → px，四舍五入（换算单源 = ThemeMetrics）。
    pub const fn spacing(&self, density: Density, token: SpacingToken) -> u32 {
        ThemeMetrics::new(density).spacing(token)
    }

    /// quarter-vp 圆角 token → px，四舍五入（换算单源 = ThemeMetrics）。
    pub const fn radius(&self, density: Density, token: RadiusToken) -> u32 {
        ThemeMetrics::new(density).radius(token)
    }

    pub const fn typography(&self, density: Density, token: TypographyToken) -> FontSize {
        ThemeMetrics::new(density).typography(token)
    }

    pub const fn stroke(&self, density: Density, token: StrokeToken) -> u32 {
        ThemeMetrics::new(density).stroke(token)
    }

    pub fn metrics_for_viewport(&self, w: u32, h: u32) -> ThemeMetrics {
        let density = Density::for_viewport(w, h);
        ThemeMetrics { density }
    }
}

/// 当前视口的密度换算结果。F4 组件持有一份，所有 token 经该上下文换算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeMetrics {
    pub density: Density,
}

impl ThemeMetrics {
    pub const fn new(density: Density) -> Self {
        Self { density }
    }

    pub const fn spacing(self, token: SpacingToken) -> u32 {
        let quarter = match token {
            SpacingToken::Xs => 4,
            SpacingToken::S => 8,
            SpacingToken::M => 16,
            SpacingToken::L => 32,
            SpacingToken::Xl => 48,
        };
        (quarter * self.density.px() + 2) / 4
    }

    pub const fn radius(self, token: RadiusToken) -> u32 {
        let quarter = match token {
            RadiusToken::Small => 24,
            RadiusToken::Medium => 48,
            RadiusToken::Large => 64,
        };
        (quarter * self.density.px() + 2) / 4
    }

    pub const fn stroke(self, token: StrokeToken) -> u32 {
        match token {
            StrokeToken::Thin => self.density.px().div_ceil(2),
        }
    }

    /// quarter-vp 网格 → effective px，四舍五入。Spacing/Radius 都必须经
    /// 本口，禁止组件私算线性像素。
    pub const fn effective_px(self, quarter_vp: u32) -> u32 {
        (quarter_vp * self.density.px() + 2) / 4
    }

    /// 离散文本缩放档。映射到既有 FontSize 档而非连续乘法；到达资产
    /// 上限时饱和，调用方必须让文本重新测量/布局。
    pub const fn typography_with_scale(
        self,
        token: TypographyToken,
        scale: TextScale,
    ) -> FontSize {
        match self.density {
            Density::Compact => match (token, scale) {
                (TypographyToken::Caption, TextScale::Standard) => FontSize::S8x16,
                (TypographyToken::Caption, TextScale::Large) => FontSize::S16x32,
                (TypographyToken::Caption, TextScale::Maximum) => FontSize::S24x48,
                (_, TextScale::Standard) => self.typography(token),
                (TypographyToken::Body, TextScale::Large) => FontSize::S24x48,
                (TypographyToken::Body, TextScale::Maximum) => FontSize::S32x48,
                (_, TextScale::Large | TextScale::Maximum) => FontSize::S32x48,
            },
            Density::Product => match (token, scale) {
                (TypographyToken::Caption, TextScale::Standard) => FontSize::S24x48,
                (_, TextScale::Standard) => FontSize::S32x48,
                (_, TextScale::Large | TextScale::Maximum) => FontSize::S32x48,
            },
        }
    }

    pub const fn typography(self, token: TypographyToken) -> FontSize {
        match (self.density, token) {
            (Density::Compact, TypographyToken::Display) => FontSize::S24x48,
            (Density::Compact, TypographyToken::Title) => FontSize::S24x48,
            (Density::Compact, TypographyToken::Body) => FontSize::S16x32,
            (Density::Compact, TypographyToken::Caption) => FontSize::S8x16,
            (Density::Product, TypographyToken::Display) => FontSize::S32x48,
            (Density::Product, TypographyToken::Title) => FontSize::S32x48,
            (Density::Product, TypographyToken::Body) => FontSize::S32x48,
            (Density::Product, TypographyToken::Caption) => FontSize::S24x48,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_selects_product_only_for_phone_geometry() {
        assert_eq!(Density::for_viewport(800, 600), Density::Compact);
        assert_eq!(Density::for_viewport(1080, 1920), Density::Product);
        assert_eq!(Density::for_viewport(1000, 1799), Density::Compact);
        assert_eq!(Density::for_viewport(999, 1920), Density::Compact);
    }

    #[test]
    fn same_tokens_scale_from_debug_to_product() {
        let theme = Theme::light();
        let compact = theme.metrics_for_viewport(800, 600);
        let product = theme.metrics_for_viewport(1080, 1920);
        assert_eq!(compact.density.px(), 2);
        assert_eq!(product.density.px(), 3);
        assert_eq!((compact.spacing(SpacingToken::S), product.spacing(SpacingToken::S)), (4, 6));
        assert_eq!((compact.spacing(SpacingToken::L), product.spacing(SpacingToken::L)), (16, 24));
        assert_eq!(
            (compact.radius(RadiusToken::Medium), product.radius(RadiusToken::Medium)),
            (24, 36)
        );
        assert_eq!(compact.typography(TypographyToken::Body), FontSize::S16x32);
        assert_eq!(product.typography(TypographyToken::Body), FontSize::S32x48);
        assert_eq!((compact.stroke(StrokeToken::Thin), product.stroke(StrokeToken::Thin)), (1, 2));
    }

    #[test]
    fn interaction_states_have_distinct_visual_truth() {
        let theme = Theme::light();
        let normal = theme.state(InteractionState::Normal);
        let pressed = theme.state(InteractionState::Pressed);
        let disabled = theme.state(InteractionState::Disabled);
        let selected = theme.state(InteractionState::Selected);
        assert_ne!(normal, pressed);
        assert_ne!(pressed, disabled);
        assert_ne!(disabled, selected);
        assert_ne!(normal, selected);
    }
}
