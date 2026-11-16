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

// ---------- Advanced-main OH semantic tokens (ThemeMode; Theme remains the product token struct) ----------

// ---------- OH 官方表常量（light 档——应用语义面） ----------

/// 品牌主色（OH `brand`#0x007DFF 系——本库组件 ACCENT 历史值
/// 0x1976D2 为 Material 蓝；波2 保持 ACCENT 不破 golden，主题面以
/// OH 值单源起步，组件色板收编挂后续刀）。
pub const BRAND: Color = Color { r: 0x00, g: 0x7D, b: 0xFF };
/// 品牌主色按下态（OH brand 暗档）。
pub const BRAND_PRESSED: Color = Color { r: 0x00, g: 0x5C, b: 0xBC };
/// 页面底（OH background#F1F3F5 light）。
pub const BG_PAGE: Color = Color { r: 0xF1, g: 0xF3, b: 0xF5 };
/// 组件面底（OH comp_background 附近色）。
pub const BG_COMP: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// 分隔线（OH divider#000000 10% 预混白底）。
pub const DIVIDER: Color = Color { r: 0xE6, g: 0xE6, b: 0xE6 };
/// 主文本（OH font_primary#000000 90% 预混白底）。
pub const FONT_PRIMARY: Color = Color { r: 0x1A, g: 0x1A, b: 0x1A };
/// 次文本（OH font_secondary 60% 预混白底）。
pub const FONT_SECONDARY: Color = Color { r: 0x66, g: 0x66, b: 0x66 };
/// 警示红（OH warning#E84026 系）。
pub const WARNING: Color = Color { r: 0xE8, g: 0x40, b: 0x26 };
/// 成功绿（OH check 系）。
pub const SUCCESS: Color = Color { r: 0x0F, g: 0xB5, b: 0x6B };
/// 禁用态组件底（OH disabled 组件面灰——pattern.rs `ItemState::Disabled`
/// 五态表同值 0xF2F2F2；语义 token 归属本表，彼处为数据层值面）。
pub const BG_DISABLED: Color = Color { r: 0xF2, g: 0xF2, b: 0xF2 };
/// 禁用态前景/细件弱化灰（38% 灰系——主树既有弱化灰同值族，文档化近似）。
pub const FG_DISABLED: Color = Color { r: 0x9E, g: 0x9E, b: 0x9E };

// ---------- 组件色板（light 档——旧 paint.rs「组件 v1 固定色板」同值迁入） ----------
//
// 维八暗色刀迁移面：下列常量自 paint.rs 迁入（light 档值逐色不变——
// gallery2 像素哈希/widgets 值级对拍零漂移；迁移点对照 = 各常量 doc 的
// 「旧值」行）。pub 供 golden 测试值级对拍（旧 paint.rs 同律）。

/// 组件主色：Filled 按钮底 / Toggle 开轨 / Slider 填充与拖动圆 /
/// Progress 填充 / Check 选中框与勾 / Radio 选中环与内圆 / Seg active
/// 段。旧值 = paint.rs `ACCENT` 0x1976D2（Material 蓝历史冻结——收编
/// BRAND 0x007DFF 挂样式系统后续刀，两档先保族内一致性）。
pub const ACCENT: Color = Color { r: 0x19, g: 0x76, b: 0xD2 };
/// 组件主色按压态。旧值 = paint.rs `ACCENT_PRESSED` 0x145EA8（ACCENT
/// ×4/5 取整——20% 黑遮罩律）。
pub const ACCENT_PRESSED: Color = Color { r: 0x14, g: 0x5E, b: 0xA8 };
/// Toggle 关轨灰。旧值 = paint.rs `TRACK_OFF` 0x9E9E9E。
pub const TRACK_OFF: Color = Color { r: 0x9E, g: 0x9E, b: 0x9E };
/// Slider/Progress 底轨浅灰；Step/Seg/Collapse header 底。旧值 =
/// paint.rs `TRACK_DIM` 0xBDBDBD。
pub const TRACK_DIM: Color = Color { r: 0xBD, g: 0xBD, b: 0xBD };
/// Toggle/Slider 滑块白（拖动圆兼）。旧值 = paint.rs `KNOB` 0xFFFFFF。
pub const KNOB: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Outlined 按钮描边灰；Check/Radio 未选框环；Step/Seg/Tabs 分隔线；
/// Divider 线体。旧值 = paint.rs `OUTLINE` 0x666666。
pub const OUTLINE: Color = Color { r: 0x66, g: 0x66, b: 0x66 };
/// Filled 按钮标签白（ACCENT 底上）。旧值 = paint.rs `ON_ACCENT`。
pub const ON_ACCENT: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Outlined/Text 按钮标签深色；Step 值/加减号前景；Collapse 箭头；
/// Gauge 值文本；DragReorder 手柄杠。旧值 = paint.rs `ON_SURFACE`
/// 0x212121。
pub const ON_SURFACE: Color = Color { r: 0x21, g: 0x21, b: 0x21 };
/// Tabs 页签条底。旧值 = paint.rs `TABS_BG` 0xF1F1F1。
pub const TABS_BG: Color = Color { r: 0xF1, g: 0xF1, b: 0xF1 };
/// Tabs 指示条色（ACCENT 同源）。旧值 = paint.rs `TABS_INDICATOR`
/// 0x1976D2。
pub const TABS_INDICATOR: Color = Color { r: 0x19, g: 0x76, b: 0xD2 };
/// Sbar 轨道色（细弱通道）。旧值 = paint.rs `SBAR_TRACK` 0xE0E0E0。
pub const SBAR_TRACK: Color = Color { r: 0xE0, g: 0xE0, b: 0xE0 };
/// Sbar 滑块色。旧值 = paint.rs `SBAR_THUMB` 0x9E9E9E。
pub const SBAR_THUMB: Color = Color { r: 0x9E, g: 0x9E, b: 0x9E };
/// Badge 红底（数字胶囊与圆点共用——OH 语义色警示红系）。旧值 =
/// paint.rs `BADGE_BG` 0xD92B2B。
pub const BADGE_BG: Color = Color { r: 0xD9, g: 0x2B, b: 0x2B };
/// Badge 数字前景（红底白字）。旧值 = paint.rs `ON_BADGE` 0xFFFFFF。
pub const ON_BADGE: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Select 框底（下拉选择器——组件面白）。旧值 = paint.rs `SELECT_BG`。
pub const SELECT_BG: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Select 框边（OUTLINE 同系）。旧值 = paint.rs `SELECT_BORDER`
/// 0x666666。
pub const SELECT_BORDER: Color = Color { r: 0x66, g: 0x66, b: 0x66 };
/// Select 箭头/索引前景；Input/Edit 内容前景；DatePick/TimePick 数字
/// 前景。旧值 = paint.rs `SELECT_FG` 0x212121。
pub const SELECT_FG: Color = Color { r: 0x21, g: 0x21, b: 0x21 };
/// Input/Edit 框底（输入面白）。旧值 = paint.rs `INPUT_BG` 0xFFFFFF。
pub const INPUT_BG: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Input/Edit 无焦框边灰。旧值 = paint.rs `INPUT_BORDER` 0x666666。
pub const INPUT_BORDER: Color = Color { r: 0x66, g: 0x66, b: 0x66 };
/// Input/Edit 焦点环色（ACCENT 同源——置焦框边）。旧值 = paint.rs
/// `INPUT_FOCUS_RING` 0x1976D2。
pub const INPUT_FOCUS_RING: Color = Color { r: 0x19, g: 0x76, b: 0xD2 };
/// Input/Edit placeholder 前景（弱化灰）。旧值 = paint.rs
/// `INPUT_PLACEHOLDER` 0x9E9E9E。
pub const INPUT_PLACEHOLDER: Color = Color { r: 0x9E, g: 0x9E, b: 0x9E };
/// Canvas 自绘面底（占位浅底）。旧值 = paint.rs `CANVAS_BG` 0xEEEEEE。
pub const CANVAS_BG: Color = Color { r: 0xEE, g: 0xEE, b: 0xEE };
/// Refresh 指示条色（ACCENT 同源）。旧值 = paint.rs `REFRESH_BAR`
/// 0x1976D2。
pub const REFRESH_BAR: Color = Color { r: 0x19, g: 0x76, b: 0xD2 };
/// SwipeItem 动作缘带底（警示红系——BADGE_BG 同源）。旧值 = paint.rs
/// `SWIPE_ACTION_BG` 0xD92B2B。
pub const SWIPE_ACTION_BG: Color = Color { r: 0xD9, g: 0x2B, b: 0x2B };
/// DatePick/TimePick 框底。旧值 = paint.rs `PICK_BG` 0xFFFFFF。
pub const PICK_BG: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// DatePick/TimePick 框边。旧值 = paint.rs `PICK_BORDER` 0x666666。
pub const PICK_BORDER: Color = Color { r: 0x66, g: 0x66, b: 0x66 };
/// Tip 衬底（v1 无 alpha 通道，半透明黑底的实色近似）。旧值 = paint.rs
/// `TIP_BG` 0x404040。
pub const TIP_BG: Color = Color { r: 0x40, g: 0x40, b: 0x40 };
/// Card 面底色（白卡面）。旧值 = paint.rs `CARD_BG` 0xFFFFFF。
pub const CARD_BG: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Card 阴影条色（elevation 档位偏移的双 fill_rect 模拟）。旧值 =
/// paint.rs `CARD_SHADOW` 0xAAAAAA。
pub const CARD_SHADOW: Color = Color { r: 0xAA, g: 0xAA, b: 0xAA };

// ---------- dark 档（同语义名镜像——应用语义面） ----------

/// 品牌主色 dark（OH dark brand 亮档）。
pub const BRAND_DARK: Color = Color { r: 0x3D, g: 0x9F, b: 0xFF };
/// 品牌主色按下态 dark（维八暗色刀：旧档与 BRAND_DARK 同值未区分——
/// 今按 20% 白遮罩律区分 = 0x3D9FFF + (255−c)/5）。
pub const BRAND_PRESSED_DARK: Color = Color { r: 0x63, g: 0xB2, b: 0xFF };
/// 页面底 dark（OH dark background#17181A）。
pub const BG_PAGE_DARK: Color = Color { r: 0x17, g: 0x18, b: 0x1A };
/// 组件面底 dark。
pub const BG_COMP_DARK: Color = Color { r: 0x25, g: 0x26, b: 0x28 };
/// 分隔线 dark（白 10% 预混黑底）。
pub const DIVIDER_DARK: Color = Color { r: 0x33, g: 0x33, b: 0x33 };
/// 主文本 dark（白 90% 预混黑底）。
pub const FONT_PRIMARY_DARK: Color = Color { r: 0xE6, g: 0xE6, b: 0xE6 };
/// 次文本 dark（白 60% 预混黑底）。
pub const FONT_SECONDARY_DARK: Color = Color { r: 0x99, g: 0x99, b: 0x99 };
/// 警示红 dark。
pub const WARNING_DARK: Color = Color { r: 0xF5, g: 0x6C, b: 0x5B };
/// 成功绿 dark（light 档 20% 白预混暗底对应——补齐双档镜像）。
pub const SUCCESS_DARK: Color = Color { r: 0x3F, g: 0xC3, b: 0x88 };
/// 禁用态组件底 dark（dark 档镜像——消费面 `BG_DISABLED` 同语义名取值）。
pub const BG_DISABLED_DARK: Color = Color { r: 0x2A, g: 0x2A, b: 0x2C };
/// 禁用态前景 dark。
pub const FG_DISABLED_DARK: Color = Color { r: 0x5E, g: 0x5E, b: 0x63 };

// ---------- 组件色板（dark 档——同语义名镜像） ----------
//
// 取值律：能对上既有 dark 档语义常量的直取该值（组件面底 →
// BG_COMP_DARK 系；次级灰 → FONT_SECONDARY_DARK 系）；ACCENT 族在
// Material 蓝族内按 20% 白遮罩律提亮（暗档主色向白——见模块头按压
// 遮罩律注）；余为文档化近似（值随 theme 迭代可调——非 wire 契约）。

/// 组件主色 dark（ACCENT 0x1976D2 + (255−c)/5——族内提亮，不换族）。
pub const ACCENT_DARK: Color = Color { r: 0x47, g: 0x91, b: 0xDB };
/// 组件主色按压态 dark（ACCENT_DARK + (255−c)/5——白遮罩律）。
pub const ACCENT_PRESSED_DARK: Color = Color { r: 0x6B, g: 0xA7, b: 0xE2 };
/// Toggle 关轨灰 dark（FG_DISABLED_DARK 同族中灰）。
pub const TRACK_OFF_DARK: Color = Color { r: 0x5E, g: 0x5E, b: 0x63 };
/// Slider/Progress 底轨灰 dark（组件面底上一档——文档化近似）。
pub const TRACK_DIM_DARK: Color = Color { r: 0x44, g: 0x44, b: 0x48 };
/// Toggle/Slider 滑块 dark（FONT_PRIMARY_DARK 白 90% 预混）。
pub const KNOB_DARK: Color = Color { r: 0xE6, g: 0xE6, b: 0xE6 };
/// 描边/未选框环/分隔线 dark（FONT_SECONDARY_DARK 白 60% 预混）。
pub const OUTLINE_DARK: Color = Color { r: 0x99, g: 0x99, b: 0x99 };
/// Filled 按钮标签 dark（ACCENT_DARK 底上白不变）。
pub const ON_ACCENT_DARK: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// 组件上标签深色 dark（FONT_PRIMARY_DARK）。
pub const ON_SURFACE_DARK: Color = Color { r: 0xE6, g: 0xE6, b: 0xE6 };
/// Tabs 页签条底 dark（BG_COMP_DARK）。
pub const TABS_BG_DARK: Color = Color { r: 0x25, g: 0x26, b: 0x28 };
/// Tabs 指示条 dark（ACCENT_DARK 同源）。
pub const TABS_INDICATOR_DARK: Color = Color { r: 0x47, g: 0x91, b: 0xDB };
/// Sbar 轨道 dark（DIVIDER_DARK 同族）。
pub const SBAR_TRACK_DARK: Color = Color { r: 0x33, g: 0x33, b: 0x33 };
/// Sbar 滑块 dark（FONT_SECONDARY_DARK 同族）。
pub const SBAR_THUMB_DARK: Color = Color { r: 0x99, g: 0x99, b: 0x99 };
/// Badge 红底 dark（WARNING_DARK）。
pub const BADGE_BG_DARK: Color = Color { r: 0xF5, g: 0x6C, b: 0x5B };
/// Badge 数字前景 dark（红底白字不变）。
pub const ON_BADGE_DARK: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Select 框底 dark（BG_COMP_DARK）。
pub const SELECT_BG_DARK: Color = Color { r: 0x25, g: 0x26, b: 0x28 };
/// Select 框边 dark（OUTLINE_DARK 同族）。
pub const SELECT_BORDER_DARK: Color = Color { r: 0x99, g: 0x99, b: 0x99 };
/// Select/Input/DatePick/TimePick 前景 dark（FONT_PRIMARY_DARK）。
pub const SELECT_FG_DARK: Color = Color { r: 0xE6, g: 0xE6, b: 0xE6 };
/// Input/Edit 框底 dark（BG_COMP_DARK）。
pub const INPUT_BG_DARK: Color = Color { r: 0x25, g: 0x26, b: 0x28 };
/// Input/Edit 无焦框边 dark（OUTLINE_DARK 同族）。
pub const INPUT_BORDER_DARK: Color = Color { r: 0x99, g: 0x99, b: 0x99 };
/// Input/Edit 焦点环 dark（ACCENT_DARK 同源）。
pub const INPUT_FOCUS_RING_DARK: Color = Color { r: 0x47, g: 0x91, b: 0xDB };
/// Input/Edit placeholder dark（FG_DISABLED_DARK 弱化灰）。
pub const INPUT_PLACEHOLDER_DARK: Color = Color { r: 0x5E, g: 0x5E, b: 0x63 };
/// Canvas 占位面底 dark（BG_DISABLED_DARK 同族——文档化近似）。
pub const CANVAS_BG_DARK: Color = Color { r: 0x2A, g: 0x2A, b: 0x2C };
/// Refresh 指示条 dark（ACCENT_DARK 同源）。
pub const REFRESH_BAR_DARK: Color = Color { r: 0x47, g: 0x91, b: 0xDB };
/// SwipeItem 动作缘带 dark（WARNING_DARK 同源）。
pub const SWIPE_ACTION_BG_DARK: Color = Color { r: 0xF5, g: 0x6C, b: 0x5B };
/// DatePick/TimePick 框底 dark（BG_COMP_DARK）。
pub const PICK_BG_DARK: Color = Color { r: 0x25, g: 0x26, b: 0x28 };
/// DatePick/TimePick 框边 dark（OUTLINE_DARK 同族）。
pub const PICK_BORDER_DARK: Color = Color { r: 0x99, g: 0x99, b: 0x99 };
/// Tip 衬底 dark（页面底上一档实色——暗档对应半透明白底的实色近似，
/// 文档化近似）。
pub const TIP_BG_DARK: Color = Color { r: 0x4D, g: 0x4D, b: 0x52 };
/// Card 面底 dark（BG_COMP_DARK）。
pub const CARD_BG_DARK: Color = Color { r: 0x25, g: 0x26, b: 0x28 };
/// Card 阴影条 dark（暗底阴影 = 近黑一档——文档化近似）。
pub const CARD_SHADOW_DARK: Color = Color { r: 0x12, g: 0x12, b: 0x14 };

// ---------- OH 圆角 token（波 2 横向原语③ 消费面——add-only） ----------

/// OH 通用件圆角 8vp（卡片/Chip/页签/分段族默认——与移植源 theme 同名
/// token 同值；Button 大半径 20vp/小件 14vp 挂样式系统后续刀）。
pub const OH_RADIUS_VP: u32 = 8;
/// 通用件圆角 px 单源（8vp @3x = 24px——paint 圆角消费面直取本值；
/// @3x 密度折算的几何单源在 pattern.rs `DENSITY`，px 值此处直录避免
/// theme→pattern 反向依赖，一致性由本文件测试对拍钉板）。
pub const OH_RADIUS_PX: u32 = 24;

/// 主题档（两套预混单源枚举——消费面 `pick` 单点取值；运行态档位 =
/// `Ui::theme_mode`/`Ui::set_theme_mode` 单源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

/// 组件色板组（维八暗色刀：旧 paint.rs「组件 v1 固定色板」29 槽的
/// 双档形态——light 档逐色同值迁移）。一次取整组随 [`Tokens`] 走，
/// 消费面持组引用避免散点取色漂移。
#[derive(Debug, Clone, Copy)]
pub struct WidgetColors {
    /// 主色（Filled 按钮底 / Toggle 开轨 / Slider·Progress 填充 /
    /// Check·Radio 选中 / Seg active / Fab 圆底 / 各 focus ring 族）。
    pub accent: Color,
    /// 主色按压态（20% 遮罩律双档对应——见模块头）。
    pub accent_pressed: Color,
    /// Toggle 关轨灰。
    pub track_off: Color,
    /// 底轨浅灰（Slider/Progress/Step/Seg/Collapse header）。
    pub track_dim: Color,
    /// 滑块（Toggle/Slider）。
    pub knob: Color,
    /// 描边/未选框环/分隔线（Outlined/Check/Radio/Step/Seg/Tabs/
    /// Divider/Select/Input/Pick 框边族）。
    pub outline: Color,
    /// 主色底上标签（Filled 按钮/Tip/Fab）。
    pub on_accent: Color,
    /// 面/轨底上标签与前景件（Text 按钮/Step/Collapse/Gauge/
    /// DragReorder/Select·Input·Pick 前景）。
    pub on_surface: Color,
    /// Tabs 页签条底。
    pub tabs_bg: Color,
    /// Tabs 指示条（accent 同源）。
    pub tabs_indicator: Color,
    /// Sbar 轨道。
    pub sbar_track: Color,
    /// Sbar 滑块。
    pub sbar_thumb: Color,
    /// Badge 红底（数字胶囊与圆点）。
    pub badge_bg: Color,
    /// Badge 数字前景。
    pub on_badge: Color,
    /// Select 框底。
    pub select_bg: Color,
    /// Select 框边。
    pub select_border: Color,
    /// Select/Input/Edit/DatePick/TimePick 前景。
    pub select_fg: Color,
    /// Input/Edit 框底。
    pub input_bg: Color,
    /// Input/Edit 无焦框边。
    pub input_border: Color,
    /// Input/Edit 焦点环与 caret（accent 同源）。
    pub input_focus_ring: Color,
    /// Input/Edit placeholder。
    pub input_placeholder: Color,
    /// Canvas 占位面底。
    pub canvas_bg: Color,
    /// Refresh 指示条（accent 同源）。
    pub refresh_bar: Color,
    /// SwipeItem 动作缘带（警示红系）。
    pub swipe_action_bg: Color,
    /// DatePick/TimePick 框底。
    pub pick_bg: Color,
    /// DatePick/TimePick 框边。
    pub pick_border: Color,
    /// Tip 衬底。
    pub tip_bg: Color,
    /// Card 面底。
    pub card_bg: Color,
    /// Card 阴影条。
    pub card_shadow: Color,
}

/// 组件色板 light 档整组（旧 paint.rs 色板同值——golden 零漂移单源）。
pub const WIDGETS_LIGHT: WidgetColors = WidgetColors {
    accent: ACCENT,
    accent_pressed: ACCENT_PRESSED,
    track_off: TRACK_OFF,
    track_dim: TRACK_DIM,
    knob: KNOB,
    outline: OUTLINE,
    on_accent: ON_ACCENT,
    on_surface: ON_SURFACE,
    tabs_bg: TABS_BG,
    tabs_indicator: TABS_INDICATOR,
    sbar_track: SBAR_TRACK,
    sbar_thumb: SBAR_THUMB,
    badge_bg: BADGE_BG,
    on_badge: ON_BADGE,
    select_bg: SELECT_BG,
    select_border: SELECT_BORDER,
    select_fg: SELECT_FG,
    input_bg: INPUT_BG,
    input_border: INPUT_BORDER,
    input_focus_ring: INPUT_FOCUS_RING,
    input_placeholder: INPUT_PLACEHOLDER,
    canvas_bg: CANVAS_BG,
    refresh_bar: REFRESH_BAR,
    swipe_action_bg: SWIPE_ACTION_BG,
    pick_bg: PICK_BG,
    pick_border: PICK_BORDER,
    tip_bg: TIP_BG,
    card_bg: CARD_BG,
    card_shadow: CARD_SHADOW,
};

/// 组件色板 dark 档整组。
pub const WIDGETS_DARK: WidgetColors = WidgetColors {
    accent: ACCENT_DARK,
    accent_pressed: ACCENT_PRESSED_DARK,
    track_off: TRACK_OFF_DARK,
    track_dim: TRACK_DIM_DARK,
    knob: KNOB_DARK,
    outline: OUTLINE_DARK,
    on_accent: ON_ACCENT_DARK,
    on_surface: ON_SURFACE_DARK,
    tabs_bg: TABS_BG_DARK,
    tabs_indicator: TABS_INDICATOR_DARK,
    sbar_track: SBAR_TRACK_DARK,
    sbar_thumb: SBAR_THUMB_DARK,
    badge_bg: BADGE_BG_DARK,
    on_badge: ON_BADGE_DARK,
    select_bg: SELECT_BG_DARK,
    select_border: SELECT_BORDER_DARK,
    select_fg: SELECT_FG_DARK,
    input_bg: INPUT_BG_DARK,
    input_border: INPUT_BORDER_DARK,
    input_focus_ring: INPUT_FOCUS_RING_DARK,
    input_placeholder: INPUT_PLACEHOLDER_DARK,
    canvas_bg: CANVAS_BG_DARK,
    refresh_bar: REFRESH_BAR_DARK,
    swipe_action_bg: SWIPE_ACTION_BG_DARK,
    pick_bg: PICK_BG_DARK,
    pick_border: PICK_BORDER_DARK,
    tip_bg: TIP_BG_DARK,
    card_bg: CARD_BG_DARK,
    card_shadow: CARD_SHADOW_DARK,
};

/// 语义 token 组（一次取整套——消费面持组引用，避免散点取色漂移）。
#[derive(Debug, Clone, Copy)]
pub struct Tokens {
    pub brand: Color,
    pub brand_pressed: Color,
    pub bg_page: Color,
    pub bg_comp: Color,
    pub divider: Color,
    pub font_primary: Color,
    pub font_secondary: Color,
    pub warning: Color,
    /// 成功绿（维八暗色刀补档——双档镜像完备）。
    pub success: Color,
    /// 禁用态组件底（paint 三态查表消费——双档随行）。
    pub bg_disabled: Color,
    /// 禁用态前景/弱化灰（paint 三态查表消费——双档随行）。
    pub fg_disabled: Color,
    /// 组件色板整组（旧 paint.rs 固定色板——维八暗色刀迁入）。
    pub w: WidgetColors,
}

impl Tokens {
    /// 按主题档取整套语义 token。
    pub fn pick(dark: ThemeMode) -> Tokens {
        match dark {
            ThemeMode::Light => Tokens {
                brand: BRAND,
                brand_pressed: BRAND_PRESSED,
                bg_page: BG_PAGE,
                bg_comp: BG_COMP,
                divider: DIVIDER,
                font_primary: FONT_PRIMARY,
                font_secondary: FONT_SECONDARY,
                warning: WARNING,
                success: SUCCESS,
                bg_disabled: BG_DISABLED,
                fg_disabled: FG_DISABLED,
                w: WIDGETS_LIGHT,
            },
            ThemeMode::Dark => Tokens {
                brand: BRAND_DARK,
                brand_pressed: BRAND_PRESSED_DARK,
                bg_page: BG_PAGE_DARK,
                bg_comp: BG_COMP_DARK,
                divider: DIVIDER_DARK,
                font_primary: FONT_PRIMARY_DARK,
                font_secondary: FONT_SECONDARY_DARK,
                warning: WARNING_DARK,
                success: SUCCESS_DARK,
                bg_disabled: BG_DISABLED_DARK,
                fg_disabled: FG_DISABLED_DARK,
                w: WIDGETS_DARK,
            },
        }
    }
}
