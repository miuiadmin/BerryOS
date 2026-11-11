//! GUI-6 段A D28 供给槽制：TextMetrics/IconSource 缝 + Monospace 逐位
//! 复刻（既有 layout 测零漂的生命线）+ 槽注入随行 + 默认空图标件。
//!
//! 确定性：全值级断言（cell 档位 FontSize 单源对拍、bounds 值级）。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_ui::damage::RectI;
use berry_ui::desc::{ColProps, Props, TextProps, TextStyle, ViewKind, ViewSpec};
use berry_ui::metrics::{EmptyIconSource, IconSource, MonospaceTextMetrics, TextMetrics};
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
