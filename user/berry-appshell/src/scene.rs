//! scene —— 窗树装配底座（波3-件⑦ 抽取自 berry-launcher desktop.rs 的
//! 通用建树面——scrim/组对齐/图标桥；行为零变化）。
//!
//! 职责：状态栏整条装配（scrim 衬底色动态预混 + OH StatusbarConfig
//! LayoutGroups 1/3 三组对齐 + 内建图标桥 + 时钟/角标文本面）+ 文本叶
//! spec 单源 + "HH:MM" 时钟格式化。**桌面特化（网格/壁纸/dock/TabBar
//! 三页/五键导航条）留 launcher**——本模块只收与具体应用无关的壳面
//!（波 4 消费：任何 appshell 应用都能拼同款状态栏/文本叶）。
//!
//! 建树序纪律（launcher desktop.rs 裁决六承袭）：Root 直接子插入序 =
//! 自底向上 z 序（paint 前序头先发 = 先插者在底）——scrim 必须在内容
//! 列之前插入（后插者盖上：尾插会把状态内容整条压在 scrim 之下，boot
//! 实证 2026-11-13）。本模块不拥有 root 装配序——scrim 色值由消费侧
//! 取 [`status_scrim`] 后自行按序插入。
//!
//! host 测试面：全模块纯函数（scrim 均值预混/组宽推算/档位表/时钟换算/
//! 文本 spec 域执法）——零 syscall 零 RPC。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_ui::desc::{ColProps, IconProps, Props, RowProps, TextProps, TextStyle};
use berry_ui::layout::CrossAlign;
use berry_ui::{NodeId, TextBuf, UiError, UiMain, ViewKind, ViewSpec, ROLE_TEXT};

use crate::icons;

// —— OpenHarmony phone 几何（applications_launcher 源仓
//    PhonePresetStyleConstants；1080×1920 = 360×640vp @3x 折算）——
/// 密度（1080 物理宽 / 360vp 逻辑宽）——状态栏/dock/网格共源（底座
/// 单源导出面）。
pub const OH_DENSITY: u32 = 3;
/// 状态栏可见高 39vp（phone statusbar float；布局保留 44vp——状态栏
/// 重做 44vp 立法归波3 状态栏件）。
pub const OH_STATUS_H: u32 = 39;
/// 状态栏左右 padding 24vp。
pub const OH_STATUS_PAD: u32 = 24;

/// 状态栏时钟占位（REALTIME 注入缺席时的静态回退——波一刀 B；在场 =
/// [`build_status_bar`] 参数 `clock_text` 首摆真值，帧循环每分钟
/// set_text 续摆）。
pub const STATUS_CLOCK: &[u8] = b"12:00";
/// 状态栏 scrim 衬底静态回退色（Rect 无 alpha 通道——预混 = 50% 黑混
/// 壁纸顶色；动态面 = [`status_scrim`] 壁纸顶区采样，壁纸缺席时回退
/// 本值（防御面：非 phone 几何））。
pub const STATUS_SCRIM: Color = Color { r: 0x03, g: 0x5E, b: 0x63 };
/// 壁纸顶区采样行数（135×240 源图顶 15 行 ≈ 屏顶 6%——OH scrim 语义的
///「背景就是壁纸暗化」近似）。
pub const SCRIM_SAMPLE_ROWS: u32 = 15;
/// 模拟态角标字（无信号/电池真值源——恒满格为模拟值，如实注；模拟器
/// 静态满格+标注是两大生态通行做法）。
pub const SIM_TAG: &[u8] = b"SIM";
/// 时钟 UTC 角标（epoch 注入口径 = UTC；本地时区换算挂账）。
pub const UTC_TAG: &[u8] = b"UTC";
/// 状态栏文本配色（白色 = systemui TintStateManager 默认 contentColor）
/// ——组内文本/角标与下游页面标题共用单源。
pub const TEXT_FG: Color = Color { r: 240, g: 240, b: 244 };

/// 状态栏 scrim 动态预混：壁纸源顶 15 行 RGB 均值 × 0.5（等价 50% 黑
/// 遮罩，随壁纸跟随——换壁纸自动换 scrim，不再出现「暗带与壁纸无关」
/// 的实色条）。壁纸数据缺席时回退 [`STATUS_SCRIM`] 静态值。
pub fn status_scrim() -> Color {
    let (px, w) = icons::wallpaper_top_rows(SCRIM_SAMPLE_ROWS);
    if w == 0 || px.is_empty() {
        return STATUS_SCRIM;
    }
    let (mut sr, mut sg, mut sb) = (0u64, 0u64, 0u64);
    for &p in px {
        sr += ((p >> 16) & 0xFF) as u64;
        sg += ((p >> 8) & 0xFF) as u64;
        sb += (p & 0xFF) as u64;
    }
    let n = px.len() as u64;
    let mix = |v: u64| -> u8 { ((v / n) / 2).min(255) as u8 };
    Color { r: mix(sr), g: mix(sg), b: mix(sb) }
}

/// S 文本叶 spec（label/状态栏共用字号档；超 `TEXT_INLINE`=64B
/// loud Err——id≤24/label≤64 域内不可达）。
pub fn text_spec(text: &[u8], fg: Color, scale: FontSize) -> Result<ViewSpec, UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(text)?,
        }),
    })
}

/// REALTIME 纳秒 → "HH:MM"（UTC 口径；恒 5B 等长——set_text 等长换文
/// = 纯 PAINT 零重测，metrics 等宽契约吃得住）。建树首摆与帧循环每分钟
/// 续摆共用单源（波一刀 B）。
pub fn hhmm_utc(rt_ns: u64) -> [u8; 5] {
    let s = rt_ns / 1_000_000_000;
    let h = (s / 3600) % 24;
    let m = (s / 60) % 60;
    [
        b'0' + (h / 10) as u8,
        b'0' + (h % 10) as u8,
        b':',
        b'0' + (m / 10) as u8,
        b'0' + (m % 10) as u8,
    ]
}

/// 首摆时钟文本对（`clock` = REALTIME 换算的 "HH:MM"；`None` = 注入
/// 缺席 → 12:00 静态占位 + UTC 角标空文本——占位钟态不误导）。
pub fn clock_texts<'a>(clock: &'a Option<[u8; 5]>) -> (&'a [u8], &'static [u8]) {
    let clock_text: &[u8] = match clock.as_ref() {
        Some(hm) => hm.as_slice(),
        None => STATUS_CLOCK,
    };
    let utc_text: &[u8] = if clock.is_some() { UTC_TAG } else { b"" };
    (clock_text, utc_text)
}

/// 状态栏档位尺寸（big = OH phone 1080×1920；紧凑 = 800×600 调试档）。
pub struct StatusSizes {
    /// 状态栏高（px）。
    pub h: u32,
    /// 时钟/主字号档。
    pub main: FontSize,
    /// 角标字号档（大档 S16x32：117px 带内 32px 小注；紧凑 S8x16）。
    pub tag: FontSize,
}

/// 档位表（纯函数，host 测试面）。
pub fn status_sizes(big: bool) -> StatusSizes {
    StatusSizes {
        h: if big { OH_STATUS_H * OH_DENSITY } else { 44 },
        main: if big { FontSize::S32x48 } else { FontSize::S16x32 },
        tag: if big { FontSize::S16x32 } else { FontSize::S8x16 },
    }
}

/// 状态栏左右 padding（OH 24vp×3；紧凑 6px）。
pub fn status_pad(big: bool) -> u32 {
    if big { OH_STATUS_PAD * OH_DENSITY } else { 6 }
}

/// 图标间距（OH StatusbarConfig 8vp × 3 = 24px；紧凑 6px）。
pub fn status_icon_gap(big: bool) -> u32 {
    if big { 8 * OH_DENSITY } else { 6 }
}

/// 三组宽推算（纯函数，host 测试面）：左组 = signal+gap+wifi+gap+SIM
/// 角标；右组 = battery+gap+时钟+gap+UTC 角标；flex = 余量（窄屏
/// saturating 归零——左对齐优先，组内永不溢出视口断言归消费侧）。
pub fn status_group_widths(
    vp_w: u32,
    pad: u32,
    gap: u32,
    sig_w: u32,
    wifi_w: u32,
    sim_w: u32,
    bat_w: u32,
    time_w: u32,
    utc_w: u32,
) -> (u32, u32, u32) {
    let left_w = sig_w + gap + wifi_w + gap + sim_w;
    let right_w = bat_w + gap + time_w + gap + utc_w;
    let flex = vp_w.saturating_sub(pad * 2 + left_w + right_w);
    (left_w, right_w, flex)
}

/// 状态栏图标源尺寸（id → (w, h)）：内建三面走 OH 资产真值；位图库域
///（bitmap_id 命中的 66×66 方源）等比缩至状态栏图标高 45——布局宽与
/// 渲染 spec 共用同一真相源（两口径不分叉）。
fn status_dims(id: u16) -> (u32, u32) {
    match id {
        icons::STATUS_SIGNAL => icons::STATUS_SIGNAL_DIM,
        icons::STATUS_WIFI => icons::STATUS_WIFI_DIM,
        icons::STATUS_BATTERY => icons::STATUS_BATTERY_DIM,
        _ => (icons::STATUS_SIGNAL_DIM.1, icons::STATUS_SIGNAL_DIM.1),
    }
}

/// 状态栏 Icon spec（内建 id 或位图库 id；宽高见 [`status_dims`]，
/// 不做运行时缩放）。
fn status_icon_spec(id: u16) -> Result<ViewSpec, UiError> {
    match id {
        icons::STATUS_SIGNAL | icons::STATUS_WIFI | icons::STATUS_BATTERY => {}
        _ if icons::bitmap_entry(id).is_some() => {}
        _ => return Err(UiError::NodeBad),
    }
    let (w, h) = status_dims(id);
    Ok(ViewSpec {
        kind: ViewKind::Icon,
        key: 0,
        props: Props::Icon(IconProps { id, fg: TEXT_FG, w: Some(w), h: Some(h) }),
    })
}

/// [`build_status_bar`] 装配参数。
pub struct StatusBarArgs<'a> {
    /// 视口宽（`ui.viewport()` 真值——裁决 D：几何全 GETINFO 动态）。
    pub vp_w: u32,
    /// 状态栏高（[`status_sizes`] 档位表产物）。
    pub status_h: u32,
    /// 档位（big/紧凑——padding 与图标间距分档）。
    pub big: bool,
    /// 时钟字号档。
    pub status_size: FontSize,
    /// 角标字号档。
    pub tag_size: FontSize,
    /// 首摆时钟文本（[`clock_texts`] 产物）。
    pub clock_text: &'a [u8],
    /// UTC 角标文本（在场 "UTC"/缺席空文本——空文本零宽不误导）。
    pub utc_text: &'a [u8],
}

/// 状态栏整条装配（OH systemui phone：左 = signal/wifi（+SIM 模拟态
/// 角标），右 = battery/clock（+UTC 角标）；StatusbarConfig
/// LayoutGroups 1/3，clock rightmost）。返回时钟文本节点（帧循环每分钟
/// set_text 等长换文 = 纯 PAINT；a11y label 同步 in-place 覆写）。
/// UTC 角标节点建树即定格，运行期无改笔——不入返回值。
///
/// a11y 一件 ROLE_TEXT（仅 clock 真值；信号/WiFi/电池模拟值退位——GUI
/// 实证 NodeFull 判例，32 帽让位给波3-件⑤ tabs 三键；SIM/UTC 角标不占
/// a11y 槽）。
pub fn build_status_bar(
    ui: &mut UiMain,
    parent: NodeId,
    a: &StatusBarArgs<'_>,
) -> Result<NodeId, UiError> {
    let status = ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(a.vp_w),
                h: Some(a.status_h),
                cross: CrossAlign::Stretch,
            }),
        },
    )?;
    let status_pad = status_pad(a.big);
    // 时钟内在宽走 ProportionalTextMetrics 真实 advance（消费侧已装
    // text_metrics）——排布与渲染同源单轨（对齐口径统一，消灭「估算
    // vs 真测」两套口径）。scale id 映射 = desc::scale_to_u8 同序
    //（berry-ui crate 私有，此处镜像）。
    let status_scale = match a.status_size {
        FontSize::S8x16 => 0,
        FontSize::S16x32 => 1,
        FontSize::S24x48 => 2,
        FontSize::S32x48 => 3,
    };
    let time_w = ui.text_width(a.clock_text, status_scale);
    let sim_w = SIM_TAG.len() as u32 * a.tag_size.cell_w() as u32;
    let utc_w = a.utc_text.len() as u32 * a.tag_size.cell_w() as u32;
    let status_icon_gap = status_icon_gap(a.big);
    // 状态栏前两位位图库优先（OH systemui 官方 signal/wifi 面）；缺名
    // 回退程序化内建面——第三位电池恒程序化（STATUS_BITMAP_NAMES[2] 空）。
    let sig_id =
        icons::bitmap_id(icons::STATUS_BITMAP_NAMES[0]).unwrap_or(icons::STATUS_SIGNAL);
    let wifi_id =
        icons::bitmap_id(icons::STATUS_BITMAP_NAMES[1]).unwrap_or(icons::STATUS_WIFI);
    let (left_w, right_w, status_flex) = status_group_widths(
        a.vp_w,
        status_pad,
        status_icon_gap,
        status_dims(sig_id).0,
        status_dims(wifi_id).0,
        sim_w,
        icons::STATUS_BATTERY_DIM.0,
        time_w,
        utc_w,
    );
    let srow = ui.add(
        status,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: 0,
                w: Some(a.vp_w),
                h: Some(a.status_h),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let add_status_spacer = |ui: &mut UiMain, w: u32| -> Result<NodeId, UiError> {
        ui.add(
            srow,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: Some(w),
                    h: Some(1),
                    cross: CrossAlign::Center,
                }),
            },
        )
    };
    add_status_spacer(ui, status_pad)?;
    let left_group = ui.add(
        srow,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: status_icon_gap,
                w: Some(left_w),
                h: None,
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let sig = ui.add(left_group, status_icon_spec(sig_id)?)?;
    // 模拟值 a11y 退位（GUI 实证 NodeFull 判例）：模拟器无真值源，信号/
    // WiFi/电池三个模拟值无读屏价值——视觉保留、a11y 不入表（32 帽让位
    // 给波3-件⑤ tabs 三键；「模拟值退位优先于扩帽」先例）。clock 真值
    // 保留（REALTIME 在场，读屏有价值）。
    let wifi = ui.add(left_group, status_icon_spec(wifi_id)?)?;
    // SIM 角标不占 a11y 槽（模拟态如实标注由左组两 icon 的
    //「simulated」label 承载）。
    ui.add(left_group, text_spec(SIM_TAG, TEXT_FG, a.tag_size)?)?;
    add_status_spacer(ui, status_flex)?;
    let right_group = ui.add(
        srow,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: status_icon_gap,
                w: Some(right_w),
                h: None,
                cross: CrossAlign::Center,
            }),
        },
    )?;
    let battery = ui.add(right_group, status_icon_spec(icons::STATUS_BATTERY)?)?;
    let time_node = ui.add(right_group, text_spec(a.clock_text, TEXT_FG, a.status_size)?)?;
    ui.set_a11y(time_node, ROLE_TEXT, a.clock_text)?;
    // UTC 角标：仅 REALTIME 在场显字（空文本零宽——占位钟态不误导）；
    // 建树定格不入句柄（运行期无改笔）。
    ui.add(right_group, text_spec(a.utc_text, TEXT_FG, a.tag_size)?)?;
    add_status_spacer(ui, status_pad)?;
    Ok(time_node)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REALTIME → "HH:MM"：零点/整点/23:59/跨日回卷恒 5B 等长。
    #[test]
    fn hhmm_utc_known_times() {
        assert_eq!(hhmm_utc(0), *b"00:00");
        assert_eq!(hhmm_utc(3600 * 1_000_000_000), *b"01:00");
        assert_eq!(hhmm_utc((23 * 3600 + 59 * 60) * 1_000_000_000), *b"23:59");
        // 25h → 回卷 01:00（%24 语义）。
        assert_eq!(hhmm_utc(25 * 3600 * 1_000_000_000), *b"01:00");
        assert_eq!(hhmm_utc((7 * 3600 + 5 * 60) * 1_000_000_000 + 999_999_999), *b"07:05");
    }

    /// 首摆文本对：在场 = 真值 + UTC 角标；缺席 = 12:00 占位 + 空角标。
    #[test]
    fn clock_texts_fallback_pair() {
        let none: Option<[u8; 5]> = None;
        assert_eq!(clock_texts(&none), (STATUS_CLOCK, b"".as_slice()));
        let hm = Some(*b"09:30");
        assert_eq!(clock_texts(&hm), (b"09:30".as_slice(), UTC_TAG));
    }

    /// 档位表：big = 117px/S32x48/S16x32；紧凑 = 44px/S16x32/S8x16；
    /// padding/图标间距双档同表。
    #[test]
    fn status_sizes_and_pads_by_tier() {
        let big = status_sizes(true);
        assert_eq!(big.h, 39 * 3);
        assert!(matches!(big.main, FontSize::S32x48));
        assert!(matches!(big.tag, FontSize::S16x32));
        let compact = status_sizes(false);
        assert_eq!(compact.h, 44);
        assert!(matches!(compact.main, FontSize::S16x32));
        assert!(matches!(compact.tag, FontSize::S8x16));
        assert_eq!((status_pad(true), status_icon_gap(true)), (24 * 3, 8 * 3));
        assert_eq!((status_pad(false), status_icon_gap(false)), (6, 6));
    }

    /// 三组宽推算：常规档 flex = 余量；窄屏 saturating 归零不回卷。
    #[test]
    fn status_group_widths_saturates() {
        let (l, r, flex) = status_group_widths(1080, 72, 24, 66, 66, 3 * 32, 45, 160, 3 * 16);
        assert_eq!(l, 66 + 24 + 66 + 24 + 96);
        assert_eq!(r, 45 + 24 + 160 + 24 + 48);
        assert_eq!(flex, 1080 - 144 - l - r);
        // 窄屏：pad×2 + 两组已超视口 → flex 0（u32 不下溢）。
        let (_, _, zero) = status_group_widths(100, 72, 24, 66, 66, 96, 45, 160, 48);
        assert_eq!(zero, 0);
    }

    /// scrim 动态预混 = 壁纸顶 15 行均值 × 0.5（独立复算对拍）；非空
    /// 域恒可算。
    #[test]
    fn scrim_is_half_of_wallpaper_top_mean() {
        let expect = {
            let (px, w) = icons::wallpaper_top_rows(SCRIM_SAMPLE_ROWS);
            assert!(w > 0 && !px.is_empty());
            let n = px.len() as u64;
            let ch = |shift: u32| -> u8 {
                let s: u64 = px.iter().map(|&p| ((p >> shift) & 0xFF) as u64).sum();
                ((s / n) / 2).min(255) as u8
            };
            Color { r: ch(16), g: ch(8), b: ch(0) }
        };
        assert_eq!(status_scrim(), expect);
    }

    /// 文本叶 spec：域内 Ok；超 64B（TEXT_INLINE）loud Err。
    #[test]
    fn text_spec_enforces_inline_cap() {
        assert!(text_spec(b"ok", TEXT_FG, FontSize::S16x32).is_ok());
        let long = [b'a'; 65];
        assert!(text_spec(&long, TEXT_FG, FontSize::S16x32).is_err());
        let exact = [b'a'; 64];
        assert!(text_spec(&exact, TEXT_FG, FontSize::S16x32).is_ok());
    }

    /// 状态栏整条装配：返时钟节点 + 可等长换文（帧循环续摆面）；离屏
    /// UiMain 1080×1920 大档。
    #[test]
    fn build_status_bar_assembles_clock_node() {
        let mut ui = UiMain::new(1080, 1920);
        let root = ui.root();
        let sizes = status_sizes(true);
        let (clock_text, utc_text) = clock_texts(&None);
        let clock = build_status_bar(
            &mut ui,
            root,
            &StatusBarArgs {
                vp_w: 1080,
                status_h: sizes.h,
                big: true,
                status_size: sizes.main,
                tag_size: sizes.tag,
                clock_text,
                utc_text,
            },
        )
        .expect("status bar builds");
        // 时钟节点可 set_text（等长换文路径——帧循环续摆面）。
        assert!(ui.set_text(clock, b"13:45").is_ok());
    }
}
