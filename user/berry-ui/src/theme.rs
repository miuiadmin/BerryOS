//! berry-ui 主题 token（波2 控件底座——六波方案步骤 3「主题 token
//! user/berry-ui/src/theme.rs（OH 官方表常量单源）」；维八暗色刀扩容
//! 运行态双档）。
//!
//! 单源纪律：全 berry-ui 的语义色 token 唯一归属本文件。paint.rs 原
//! 「组件 v1 固定色板」块已整块迁入本表（维八暗色刀——UI-02 裁决
//! 「paint 固定色板豁免清零改查 token」）：light 档逐色同值迁移
//! （golden 零漂移），dark 档逐色给对应值；paint 侧零固定色，豁免
//! 清单与 grep 门见 paint.rs 色板迁出注与 tests/dark.rs。两套预混 =
//! light/dark 双主题（同一语义名两值——消费面 `Tokens::pick(dark)`
//! 单点取值，无 alpha 通道下的「预混」= 与底色组合后的最终色直接
//! 入表）。
//!
//! 色值出处：OpenHarmony 官方设计规范色板（sys resources 色资源表
//! `brand`/`background`/`font`/`icon`/`warning` 系——v3.x phone 档；
//! 深浅双档同语义名换值见 ux-design/visual-colors.md）。常量面为文档
//! 化近似（OH 表按资源名给 ARGB 全量；本表只录 UI 底座消费到的语义
//! 面，值随 theme 迭代可调——非 wire 契约）。
//!
//! 按压遮罩律（双档对应）：light 档按压 = 20% 黑遮罩 = `c×4/5` 取整
//! （ACCENT_PRESSED 先例）；dark 档对应 = 20% 白遮罩 = `c+(255−c)/5`
//! ——暗底上向黑方向不可辨，按压反馈面向白侧（Material/iOS 暗色档
//! 同向；OH 官方页只立法「同语义色双档换值」，遮罩方向为本仓文档化
//! 近似）。

use berry_gfx::cmd::Color;

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
pub enum Theme {
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
                success: SUCCESS,
                bg_disabled: BG_DISABLED,
                fg_disabled: FG_DISABLED,
                w: WIDGETS_LIGHT,
            },
            Theme::Dark => Tokens {
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
        assert_ne!(l.success, d.success, "成功绿双档镜像（维八补档）");
    }

    /// 品牌按下态与常态度可区分（light 档）。
    #[test]
    fn brand_pressed_distinct_light() {
        let t = Tokens::pick(Theme::Light);
        assert_ne!(t.brand, t.brand_pressed);
    }

    /// 品牌按下态与常态度可区分（dark 档——维八暗色刀：旧档同值未
    /// 区分，今按 20% 白遮罩律给对应值）。
    #[test]
    fn brand_pressed_distinct_dark() {
        let t = Tokens::pick(Theme::Dark);
        assert_ne!(t.brand, t.brand_pressed, "dark 档 brand_pressed 不得与 normal 同值");
    }

    /// 组件色板 light 档 = 旧 paint.rs 固定色板逐色同值（迁移零漂移
    /// 值级钉板——gallery2 像素哈希之外的值面第二道门）。
    #[test]
    fn widget_palette_light_pinned_to_v1() {
        let w = WIDGETS_LIGHT;
        assert_eq!(w.accent, Color { r: 0x19, g: 0x76, b: 0xD2 }, "ACCENT");
        assert_eq!(w.accent_pressed, Color { r: 0x14, g: 0x5E, b: 0xA8 }, "ACCENT_PRESSED");
        assert_eq!(w.track_off, Color { r: 0x9E, g: 0x9E, b: 0x9E }, "TRACK_OFF");
        assert_eq!(w.track_dim, Color { r: 0xBD, g: 0xBD, b: 0xBD }, "TRACK_DIM");
        assert_eq!(w.knob, Color { r: 0xFF, g: 0xFF, b: 0xFF }, "KNOB");
        assert_eq!(w.outline, Color { r: 0x66, g: 0x66, b: 0x66 }, "OUTLINE");
        assert_eq!(w.on_accent, Color { r: 0xFF, g: 0xFF, b: 0xFF }, "ON_ACCENT");
        assert_eq!(w.on_surface, Color { r: 0x21, g: 0x21, b: 0x21 }, "ON_SURFACE");
        assert_eq!(w.tabs_bg, Color { r: 0xF1, g: 0xF1, b: 0xF1 }, "TABS_BG");
        assert_eq!(w.tabs_indicator, Color { r: 0x19, g: 0x76, b: 0xD2 }, "TABS_INDICATOR");
        assert_eq!(w.sbar_track, Color { r: 0xE0, g: 0xE0, b: 0xE0 }, "SBAR_TRACK");
        assert_eq!(w.sbar_thumb, Color { r: 0x9E, g: 0x9E, b: 0x9E }, "SBAR_THUMB");
        assert_eq!(w.badge_bg, Color { r: 0xD9, g: 0x2B, b: 0x2B }, "BADGE_BG");
        assert_eq!(w.on_badge, Color { r: 0xFF, g: 0xFF, b: 0xFF }, "ON_BADGE");
        assert_eq!(w.select_bg, Color { r: 0xFF, g: 0xFF, b: 0xFF }, "SELECT_BG");
        assert_eq!(w.select_border, Color { r: 0x66, g: 0x66, b: 0x66 }, "SELECT_BORDER");
        assert_eq!(w.select_fg, Color { r: 0x21, g: 0x21, b: 0x21 }, "SELECT_FG");
        assert_eq!(w.input_bg, Color { r: 0xFF, g: 0xFF, b: 0xFF }, "INPUT_BG");
        assert_eq!(w.input_border, Color { r: 0x66, g: 0x66, b: 0x66 }, "INPUT_BORDER");
        assert_eq!(w.input_focus_ring, Color { r: 0x19, g: 0x76, b: 0xD2 }, "INPUT_FOCUS_RING");
        assert_eq!(w.input_placeholder, Color { r: 0x9E, g: 0x9E, b: 0x9E }, "INPUT_PLACEHOLDER");
        assert_eq!(w.canvas_bg, Color { r: 0xEE, g: 0xEE, b: 0xEE }, "CANVAS_BG");
        assert_eq!(w.refresh_bar, Color { r: 0x19, g: 0x76, b: 0xD2 }, "REFRESH_BAR");
        assert_eq!(w.swipe_action_bg, Color { r: 0xD9, g: 0x2B, b: 0x2B }, "SWIPE_ACTION_BG");
        assert_eq!(w.pick_bg, Color { r: 0xFF, g: 0xFF, b: 0xFF }, "PICK_BG");
        assert_eq!(w.pick_border, Color { r: 0x66, g: 0x66, b: 0x66 }, "PICK_BORDER");
        assert_eq!(w.tip_bg, Color { r: 0x40, g: 0x40, b: 0x40 }, "TIP_BG");
        assert_eq!(w.card_bg, Color { r: 0xFF, g: 0xFF, b: 0xFF }, "CARD_BG");
        assert_eq!(w.card_shadow, Color { r: 0xAA, g: 0xAA, b: 0xAA }, "CARD_SHADOW");
    }

    /// 组件色板 dark 档实改（防「dark 档照抄 light」回归——按槽断言
    /// 双档不同；ON_ACCENT/ON_BADGE 双档同白为合法同值，不在断言列）。
    #[test]
    fn widget_palette_dark_differs_per_slot() {
        let l = WIDGETS_LIGHT;
        let d = WIDGETS_DARK;
        assert_ne!(l.accent, d.accent);
        assert_ne!(l.accent_pressed, d.accent_pressed);
        assert_ne!(l.track_off, d.track_off);
        assert_ne!(l.track_dim, d.track_dim);
        assert_ne!(l.knob, d.knob);
        assert_ne!(l.outline, d.outline);
        assert_ne!(l.on_surface, d.on_surface);
        assert_ne!(l.tabs_bg, d.tabs_bg);
        assert_ne!(l.tabs_indicator, d.tabs_indicator);
        assert_ne!(l.sbar_track, d.sbar_track);
        assert_ne!(l.badge_bg, d.badge_bg);
        assert_ne!(l.select_bg, d.select_bg);
        assert_ne!(l.select_fg, d.select_fg);
        assert_ne!(l.input_bg, d.input_bg);
        assert_ne!(l.input_focus_ring, d.input_focus_ring);
        assert_ne!(l.input_placeholder, d.input_placeholder);
        assert_ne!(l.canvas_bg, d.canvas_bg);
        assert_ne!(l.refresh_bar, d.refresh_bar);
        assert_ne!(l.swipe_action_bg, d.swipe_action_bg);
        assert_ne!(l.pick_bg, d.pick_bg);
        assert_ne!(l.tip_bg, d.tip_bg);
        assert_ne!(l.card_bg, d.card_bg);
        assert_ne!(l.card_shadow, d.card_shadow);
        // 主色按压态双档各随其主（族内遮罩律自洽）。
        assert_ne!(d.accent, d.accent_pressed, "dark 档按压可区分");
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
        // Tokens 面双档随行（paint 三态查表消费按档取）。
        let l = Tokens::pick(Theme::Light);
        let d = Tokens::pick(Theme::Dark);
        assert_eq!(l.bg_disabled, BG_DISABLED);
        assert_eq!(d.bg_disabled, BG_DISABLED_DARK);
        assert_eq!(l.fg_disabled, FG_DISABLED);
        assert_eq!(d.fg_disabled, FG_DISABLED_DARK);
    }

    /// 圆角 token px 折算钉板：8vp @3x = 24px（与 pattern.rs `DENSITY`
    /// 几何单源对拍——theme 不反向依赖，一致性在此执法）。
    #[test]
    fn radius_token_px_pinned() {
        assert_eq!(OH_RADIUS_VP, 8);
        assert_eq!(OH_RADIUS_PX, OH_RADIUS_VP * crate::pattern::DENSITY);
    }
}
