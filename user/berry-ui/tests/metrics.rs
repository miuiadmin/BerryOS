//! GUI-6 段A D28 供给槽制：TextMetrics/IconSource 缝 + Monospace 逐位
//! 复刻（既有 layout 测零漂的生命线）+ 槽注入随行 + 默认空图标件。
//! 痛点⑥根修（03 号 §1.3-3）：ProportionalTextMetrics 比例供件——三档
//! 恒等等宽、HD 档真实比例宽 + set_text 等长换文判据改经内在宽比对。
//!
//! 确定性：全值级断言（cell 档位 FontSize 单源对拍、bounds 值级）。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_ui::damage::RectI;
use berry_ui::desc::{ColProps, Props, TextProps, TextStyle, ViewKind, ViewSpec};
use berry_ui::metrics::{
    EmptyIconSource, IconSource, MonospaceTextMetrics, ProportionalTextMetrics, TextMetrics,
};
use berry_ui::{TextBuf, UiMain};

fn text_spec(bytes: &[u8], scale: FontSize) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: Color { r: 9, g: 9, b: 9 }, bg: None, scale },
            text: TextBuf::new(bytes).unwrap(),
        }),
    }
}

#[test]
fn monospace_replicates_len_times_cell_all_tiers() {
    // MonospaceTextMetrics 逐位复刻现状 Text 内在测量（宽 = len × cell_w、
    // 高 = cell_h；scale id = 档位序号 0/1/2——与 desc wire 域同序）。
    let tm = MonospaceTextMetrics;
    for (id, fs) in [(0u8, FontSize::S8x16), (1, FontSize::S16x32), (2, FontSize::S24x48), (3, FontSize::S32x48)] {
        let (w, h) = tm.measure(b"berry", id);
        assert_eq!(w, (5 * fs.cell_w()) as u32, "宽 = len × cell_w（档 {id}）");
        assert_eq!(h, fs.cell_h() as u32, "高 = cell_h（档 {id}）");
    }
}

/// cell 宽 ×2 供件——自定义度量经槽随行。
struct Wide2(MonospaceTextMetrics);

impl TextMetrics for Wide2 {
    fn measure(&self, text: &[u8], scale: u8) -> (u32, u32) {
        let (w, h) = self.0.measure(text, scale);
        (w.saturating_mul(2), h)
    }
}

static WIDE2: Wide2 = Wide2(MonospaceTextMetrics);

#[test]
fn text_metrics_slot_swaps_intrinsic_measurement() {
    let mut ui = UiMain::new(100, 100);
    let root = ui.root();
    let col = ui
        .add(root, ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }) })
        .unwrap();
    let t = ui.add(col, text_spec(b"hi", FontSize::S8x16)).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 16, 16), "默认 Monospace：2×8=16 宽");

    // 换 ×2 供件 + 触发重测（set_viewport 全量置脏）→ 内在宽随供件变
    ui.set_text_metrics(&WIDE2);
    ui.set_viewport(100, 100);
    ui.drain_damage();
    ui.layout();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 32, 16), "×2 供件经槽生效（D28）");
}

#[test]
fn empty_icon_source_defaults_none() {
    // 默认供件：dims/pixels 恒 None（资源未注册 = 空绘不炸的供给面）；
    // 槽注入 + 未注册 Icon 走 layout → 零几何不炸（R3-5：原尾断言
    // viewport==(8,8) 与供件无因果恒真——改钉 layout 行为面，与
    // offscreen 的 paint 面互补成两半）。
    let src = EmptyIconSource;
    assert_eq!(src.dims(0), None);
    assert_eq!(src.pixels(0), None);
    static E: EmptyIconSource = EmptyIconSource;
    let mut ui = UiMain::new(8, 8);
    ui.set_icon_source(&E);
    let root = ui.root();
    let icon = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Icon,
                key: 0,
                props: Props::Icon(berry_ui::desc::IconProps {
                    id: 5, // 未注册（EmptyIconSource 恒 None）
                    fg: Color { r: 1, g: 2, b: 3 },
                    w: None,
                    h: None,
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(icon).unwrap(), RectI::new(0, 0, 0, 0), "dims 缺 → 零几何（空供件静默档——不炸不 assert）");
}

// ---------- 痛点⑥：ProportionalTextMetrics 比例供件（03 号 §1.3-3） ----------

static PROP: ProportionalTextMetrics = ProportionalTextMetrics;

/// 等宽供件句柄（判据对照面）。
static MONO: MonospaceTextMetrics = MonospaceTextMetrics;

#[test]
fn proportional_equals_monospace_on_bitmap_tiers() {
    // 三档点阵（advance 缺省 = cell_w）：比例供件与等宽供件**逐值恒等**
    // ——换装无三档回归面（零漂移钉板；全域字节含域外/控制字节）。
    let texts: [&[u8]; 5] = [b"", b"hi", b"Browser Settings", b"\x00\x7f\xff", b"~~~~~~~~~~"];
    for (id, fs) in [(0u8, FontSize::S8x16), (1, FontSize::S16x32), (2, FontSize::S24x48)] {
        for t in texts {
            assert_eq!(
                PROP.measure(t, id),
                MONO.measure(t, id),
                "三档恒等（档 {id}，{t:?}）"
            );
            assert_eq!(PROP.measure(t, id).1, fs.cell_h() as u32);
        }
    }
}

#[test]
fn proportional_hd_true_advance_width() {
    // HD 档（Noto Sans CJK 比例源）：真实比例宽——03 号 §1.1 实测重实
    // 现（等宽 16×32=512px vs 比例 269px；'i'×3 = 27px vs 96px）。
    let (w, h) = PROP.measure(b"Browser Settings", 3);
    assert_eq!((w, h), (269, 48), "'Browser Settings' HD 比例宽 = 269×48");
    assert_eq!(MONO.measure(b"Browser Settings", 3).0, 512, "等宽对照 = 512");
    assert_eq!(PROP.measure(b"iii", 3).0, 27, "'i'×3 = 3×9");
    // 空串零宽（两供件同律）。
    assert_eq!(PROP.measure(b"", 3), (0, 48));
}

#[test]
fn proportional_slot_swaps_hd_bounds() {
    // 槽换装生效（D28 同款）：HD 文本内在宽 512（默认 Monospace）→
    // 换比例供件 + 全量置脏重测 → 269。
    let mut ui = UiMain::new(640, 64);
    let root = ui.root();
    let t = ui.add(root, text_spec(b"Browser Settings", FontSize::S32x48)).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 512, 48), "默认等宽 512");
    ui.set_text_metrics(&PROP);
    ui.set_viewport(640, 64);
    ui.drain_damage();
    ui.layout();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 269, 48), "比例供件 269");
}

#[test]
fn set_text_equal_len_wide_change_remeasures_under_proportional() {
    // R2-8 演化面（03 号 §1.2-4 架构注记收口）：比例供件下等长换文不再
    // 蕴含等宽——"ii"（2×9=18）→ "WW"（2×30=60）等长但宽变，set_text
    // 须走 GEOM 重测分支（bounds 随之变）；等宽供件下同长换文 bounds
    // 零漂移（v1 三态判据行为保留）。
    let mut ui = UiMain::new(640, 64);
    let root = ui.root();
    let t = ui.add(root, text_spec(b"ii", FontSize::S32x48)).unwrap();
    ui.set_text_metrics(&PROP);
    ui.set_viewport(640, 64);
    ui.drain_damage();
    ui.layout();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 18, 48), "ii = 2×9");

    // 等长换文宽变（ii→WW）：内在宽 18→60——GEOM 分支重测（若判据仍
    // 假设等长即等宽，此处 bounds 将滞留 18 = 漂移咬点）。
    ui.set_text(t, b"WW").unwrap();
    ui.layout();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 60, 48), "WW = 2×30 → 重测生效");

    // 等宽供件 + 等宽档对照：等长换文 bounds 零漂移（v1 三态判据行为
    // 保留——bench_s7 项 5 纯 PAINT 面的同值对照）。
    ui.set_text_metrics(&MONO);
    ui.set_viewport(640, 64);
    ui.drain_damage();
    ui.layout();
    let t2 = ui.add(root, text_spec(b"hi", FontSize::S8x16)).unwrap();
    ui.layout();
    let before = ui.bounds(t2).unwrap();
    ui.set_text(t2, b"oy").unwrap(); // 等长换文（S8x16 等宽档）
    ui.layout();
    assert_eq!(ui.bounds(t2).unwrap(), before, "等宽供件等长换文 bounds 零漂移（v1 判据保留）");
}
