//! berry-ui 主题 token（波2 控件底座——六波方案步骤 3「主题 token
//! user/berry-ui/src/theme.rs（OH 官方表常量单源）」）。
//!
//! 单源纪律：全 berry-ui 的语义色 token 唯一归属本文件；paint.rs 的
//! 「组件 v1 固定色板」块是**过渡豁免**（Phase 2 起步面先例——主题系
//! 统全量落地后由本表迁出收编，见 PRESSED_BG 注）。两套预混 = light/
//! dark 双主题（同一语义名两值——消费面 `Theme::pick(dark)` 单点取值，
//! 无 alpha 通道下的「预混」= 与底色组合后的最终色直接入表）。
//!
//! 色值出处：OpenHarmony 官方设计规范色板（sys resources 色资源表
//! `brand`/`background`/`font`/`icon`/`warning` 系——v3.x phone 档）。
//! 常量面为文档化近似（OH 表按资源名给 ARGB 全量；本表只录 UI 底座
//! 消费到的语义面，值随 theme 迭代可调——非 wire 契约）。

use berry_gfx::cmd::Color;

// ---------- OH 官方表常量（light 档） ----------

/// 品牌主色（OH `brand`#0x007DFF 系——本库 ACCENT 历史值 0x1976D2 为
/// Material 蓝；波2 保持 ACCENT 不破 golden，主题面以 OH 值单源起步，
/// 控件色板收编挂后续刀）。
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

// ---------- dark 档（同语义名镜像） ----------

/// 品牌主色 dark（OH dark brand 亮档）。
pub const BRAND_DARK: Color = Color { r: 0x3D, g: 0x9F, b: 0xFF };
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
/// 禁用态组件底 dark（dark 档镜像——消费面 `BG_DISABLED` 同语义名取值）。
pub const BG_DISABLED_DARK: Color = Color { r: 0x2A, g: 0x2A, b: 0x2C };
/// 禁用态前景 dark。
pub const FG_DISABLED_DARK: Color = Color { r: 0x5E, g: 0x5E, b: 0x63 };

// ---------- OH 圆角 token（波 2 横向原语③ 消费面——add-only） ----------

/// OH 通用件圆角 8vp（卡片/Chip/页签/分段族默认——与移植源 theme 同名
/// token 同值；Button 大半径 20vp/小件 14vp 挂样式系统后续刀）。
pub const OH_RADIUS_VP: u32 = 8;
/// 通用件圆角 px 单源（8vp @3x = 24px——paint 圆角消费面直取本值；
/// @3x 密度折算的几何单源在 pattern.rs `DENSITY`，px 值此处直录避免
/// theme→pattern 反向依赖，一致性由本文件测试对拍钉板）。
pub const OH_RADIUS_PX: u32 = 24;

/// 主题档（两套预混单源枚举——消费面 `pick` 单点取值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

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
}

impl Tokens {
    /// 按主题档取整套语义 token。
    pub fn pick(dark: Theme) -> Tokens {
        match dark {
            Theme::Light => Tokens {
                brand: BRAND,
                brand_pressed: BRAND_PRESSED,
                bg_page: BG_PAGE,
                bg_comp: BG_COMP,
                divider: DIVIDER,
                font_primary: FONT_PRIMARY,
                font_secondary: FONT_SECONDARY,
                warning: WARNING,
            },
            Theme::Dark => Tokens {
                brand: BRAND_DARK,
                brand_pressed: BRAND_DARK,
                bg_page: BG_PAGE_DARK,
                bg_comp: BG_COMP_DARK,
                divider: DIVIDER_DARK,
                font_primary: FONT_PRIMARY_DARK,
                font_secondary: FONT_SECONDARY_DARK,
                warning: WARNING_DARK,
            },
        }
    }
}

// ---------- 主题单元测试（文件内联——不占 tests/ 面） ----------

#[cfg(test)]
mod tests {
    use super::*;

    /// 两套预混逐 token 不同（light/dark 可区分——单源表的完整性门）。
    #[test]
    fn two_themes_distinct_per_token() {
        let l = Tokens::pick(Theme::Light);
        let d = Tokens::pick(Theme::Dark);
        assert_ne!(l.bg_page, d.bg_page);
        assert_ne!(l.font_primary, d.font_primary);
        assert_ne!(l.divider, d.divider);
        assert_ne!(l.brand, d.brand);
    }

    /// 品牌按下态与常态度可区分（light 档；dark 档按压语义挂后续刀）。
    #[test]
    fn brand_pressed_distinct_light() {
        let t = Tokens::pick(Theme::Light);
        assert_ne!(t.brand, t.brand_pressed);
    }

    /// 禁用档 token 完备性（波 2 移植件 XA）：light/dark 各自成对镜像、
    /// 与常态度可区分（禁用观感 = 与正常面不同即达意——值非 wire 契约）。
    #[test]
    fn disabled_tokens_paired_and_distinct() {
        assert_ne!(BG_DISABLED, BG_COMP, "light 禁用底 ≠ 组件面底");
        assert_ne!(FG_DISABLED, FONT_PRIMARY, "light 禁用前景 ≠ 主文本");
        assert_ne!(BG_DISABLED_DARK, BG_COMP_DARK, "dark 禁用底 ≠ 组件面底");
        assert_ne!(FG_DISABLED_DARK, FONT_PRIMARY_DARK, "dark 禁用前景 ≠ 主文本");
        assert_ne!(BG_DISABLED, BG_DISABLED_DARK, "双档镜像各自取值");
    }

    /// 圆角 token px 折算钉板：8vp @3x = 24px（与 pattern.rs `DENSITY`
    /// 几何单源对拍——theme 不反向依赖，一致性在此执法）。
    #[test]
    fn radius_token_px_pinned() {
        assert_eq!(OH_RADIUS_VP, 8);
        assert_eq!(OH_RADIUS_PX, OH_RADIUS_VP * crate::pattern::DENSITY);
    }
}
