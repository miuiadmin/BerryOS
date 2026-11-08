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

/// 排版 token。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypographyToken {
    Display,
    Title,
    Body,
    Caption,
}

/// Berry Design light 主题。`Theme::light()` 是显式构造口，未来 dark 主题
/// 以同级构造口加入，不改变组件消费形状。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub colors: ThemeColors,
}

impl Theme {
    pub const fn light() -> Self {
        Self {
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

    pub const fn state(&self, state: InteractionState) -> StateColors {
        match state {
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

    pub const fn metrics_for_viewport(&self, w: u32, h: u32) -> ThemeMetrics {
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
