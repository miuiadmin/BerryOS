//! GUI F4：13 个第一版组件的同页离屏 golden。
//!
//! 页面同时使用 Scaffold、AppBar、ScrollView、Card、Column、Row、Text、
//! Icon、Button、Divider、ListItem、Switch、EmptyState。几何来自 layout
//! dump/bounds，颜色来自 Berry Design theme，像素经 berry-gfx soft Queue；
//! 测试侧不携带第二套视觉常量。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_ui::sdk::{
    IconSource,
    column, divider, icon, row, text, Button, Card, EmptyState, InteractionState,
    ListItem, RectI, Scaffold, ScrollView, Switch, TextVariant, Theme, UiMain, WidgetContext,
};

const W: usize = 800;
const H: usize = 600;
const STRIDE: usize = W * 4;

struct Source;
static SOURCE: Source = Source;
static PX: [u32; 4] = [0xFF_FF_00_00, 0xFF_00_FF_00, 0xFF_00_00_FF, 0xFF_FF_FF_00];

impl IconSource for Source {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        (id == 7).then_some((2, 2))
    }
    fn pixels(&self, id: u16) -> Option<&[u32]> {
        (id == 7).then_some(&PX)
    }
}

fn px(fb: &[u8], x: usize, y: usize) -> Color {
    let off = y * STRIDE + x * 4;
    Color { r: fb[off + 2], g: fb[off + 1], b: fb[off] }
}

#[test]
fn all_f4_widgets_share_one_offscreen_golden_page() {
    let mut ui = UiMain::new(W as u32, H as u32);
    ui.set_icon_source(&SOURCE);
    let theme = Theme::light();
    let ctx = WidgetContext::for_viewport(theme, W as u32, H as u32);
    let root = ui.root();

    let scaffold = Scaffold::new(&mut ui, root, &ctx, b"Settings").unwrap();
    let mut scroll = ScrollView::new(&mut ui, scaffold.body, &ctx, 520).unwrap();
    let card = Card::new(&mut ui, scroll.content, &ctx).unwrap();
    let stack = column(&mut ui, card.panel, &ctx).unwrap();
    let heading = text(&mut ui, stack, &ctx, TextVariant::Title, b"Components").unwrap();
    let mut button =
        Button::new(&mut ui, stack, &ctx, b"Save", InteractionState::Normal).unwrap();
    let line = divider(&mut ui, stack, &ctx).unwrap();
    let controls = row(&mut ui, stack, &ctx).unwrap();
    let mut switch = Switch::new(&mut ui, controls, &ctx, false, false).unwrap();
    let icon_node = icon(&mut ui, controls, &ctx, 7).unwrap();
    let mut item =
        ListItem::new(&mut ui, stack, &ctx, b"WLAN", Some(b"Connected"), false).unwrap();
    let empty = EmptyState::new(
        &mut ui, stack, &ctx, 7, b"No more items", b"Create something first",
    )
    .unwrap();

    // 状态路径先落树，再布局/应用状态，保证 paint 读最终 props。
    ui.layout();
    button
        .on_msg(
            &mut ui,
            &ctx,
            &berry_ui::sdk::Msg::Click { node: button.label },
        )
        .unwrap();
    switch.set_checked(&mut ui, &ctx, true).unwrap();
    item.set_selected(&mut ui, &ctx, true).unwrap();
    ui.layout();
    scroll.sync_bounds(&mut ui).unwrap();
    assert_eq!(scroll.offset(), 0, "golden 页面内容不初始滚动");

    // 几何锚：所有组件都在保留树上，且 AppBar/ScrollView/Card 有真实界。
    let bar_bounds = ui.bounds(scaffold.app_bar.panel).unwrap();
    let view_bounds = ui.bounds(scroll.panel).unwrap();
    let card_bounds = ui.bounds(card.panel).unwrap();
    let button_bounds = ui.bounds(button.panel).unwrap();
    let divider_bounds = ui.bounds(line).unwrap();
    let switch_bounds = ui.bounds(switch.panel).unwrap();
    let item_bounds = ui.bounds(item.panel).unwrap();
    let empty_bounds = ui.bounds(empty.panel).unwrap();
    assert_eq!(bar_bounds, RectI::new(0, 0, 800, 56));
    assert_eq!(view_bounds, RectI::new(0, 56, 800, 520));
    assert!(card_bounds.w > 0 && card_bounds.h > 0);
    assert!(button_bounds.w > 0 && button_bounds.h > 0);
    assert_eq!(divider_bounds.h, 1);
    assert_eq!((switch_bounds.w, switch_bounds.h), (52, 28));
    assert!(item_bounds.h > button_bounds.h);
    assert!(empty_bounds.h > switch_bounds.h);
    assert!(ui.bounds(heading).unwrap().h > 0);
    assert_eq!(ui.bounds(icon_node).unwrap().w, 16);

    // 全量 damage → display list → soft raster。
    ui.drain_damage();
    ui.wake(0);
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 256];
    let mut encoder = CommandEncoder::new();
    let report = ui.paint(&mut scratch, &mut encoder);
    let dl = encoder.finish();
    assert!(report.cmds >= 10, "至少背景/面板/文本/图标/分隔线命令 + Present");
    assert!(
        dl.iter().any(|cmd| matches!(
            cmd,
            berry_gfx::cmd::Cmd::FillRoundRect { radius, .. } if *radius > 0
        )),
        "Card/Button/Switch 必须真实消费圆角 token"
    );
    assert!(report.overdraw_x256 <= berry_ui::OVERDRAW_BUDGET_X256);

    let desc = TextureDesc::new(W, H, STRIDE, TextureFormat::Rgbx).unwrap();
    let mut fb = vec![0xEEu8; STRIDE * H];
    let mut queue = Queue::new();
    let pass = PassDescriptor::new(LoadOp::Load, StoreOp::Store, None);
    queue.submit(&dl, &mut fb, &desc, &pass).unwrap();

    // 像素锚：AppBar、ScrollView 表面、Card、Button、Divider、Switch、
    // ListItem 与 EmptyState 都由 theme token 落到真实像素。
    let center = |r: RectI| ((r.x + r.w / 2) as usize, (r.y + r.h / 2) as usize);
    let (x, y) = center(bar_bounds);
    assert_eq!(px(&fb, x, y), theme.colors.primary);
    let view_anchor = (
        (view_bounds.x + 8) as usize,
        (view_bounds.y + view_bounds.h - 2) as usize,
    );
    assert_eq!(px(&fb, view_anchor.0, view_anchor.1), theme.colors.surface);
    // Card 与 ScrollView 同为 surface；取 Card 左上内侧避开文本/子件。
    let card_anchor = (
        (card_bounds.x + 2) as usize,
        (card_bounds.y + 2) as usize,
    );
    assert_eq!(px(&fb, card_anchor.0, card_anchor.1), theme.colors.surface);
    let (x, y) = center(button_bounds);
    assert_eq!(px(&fb, x, y), theme.state(InteractionState::Normal).background);
    let (x, y) = center(divider_bounds);
    assert_eq!(px(&fb, x, y), theme.colors.outline);
    let (x, y) = center(switch_bounds);
    assert_eq!(px(&fb, x, y), theme.state(InteractionState::Selected).background);
    let (x, y) = center(item_bounds);
    assert_eq!(px(&fb, x, y), theme.state(InteractionState::Selected).background);
    let empty_anchor = ((empty_bounds.x + 2) as usize, (empty_bounds.y + 2) as usize);
    assert_eq!(px(&fb, empty_anchor.0, empty_anchor.1), theme.colors.background);

    // Icon golden：16x16 SmoothArgb 放大后四角分别保留源四像素色。
    let icon_bounds = ui.bounds(icon_node).unwrap();
    let left = icon_bounds.x as usize;
    let right = (icon_bounds.x + icon_bounds.w - 1) as usize;
    let top = icon_bounds.y as usize;
    let bottom = (icon_bounds.y + icon_bounds.h - 1) as usize;
    assert_eq!(px(&fb, left, top), Color { r: 255, g: 0, b: 0 });
    assert_eq!(px(&fb, right, top), Color { r: 0, g: 255, b: 0 });
    assert_eq!(px(&fb, left, bottom), Color { r: 0, g: 0, b: 255 });
    assert_eq!(px(&fb, right, bottom), Color { r: 255, g: 255, b: 0 });

    // 文本 golden：AppBar 标题带同时存在主题背景与 on_primary 字形像素，
    // 避免标题命令退化或整条被纯背景替代。
    let mut on_primary_px = 0usize;
    let mut primary_px = 0usize;
    for y in bar_bounds.y as usize..(bar_bounds.y + bar_bounds.h) as usize {
        for x in 0..W {
            let c = px(&fb, x, y);
            if c == theme.colors.on_primary {
                on_primary_px += 1;
            } else if c == theme.colors.primary {
                primary_px += 1;
            }
        }
    }
    assert!(on_primary_px > 0, "Title on_primary 字形像素缺失");
    assert!(primary_px > 0, "Title 主题背景像素缺失");
    assert_eq!(
        ctx.metrics.typography(berry_ui::sdk::TypographyToken::Title),
        FontSize::S24x48
    );
}
