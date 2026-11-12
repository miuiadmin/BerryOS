//! CONTROLS —— BerryUI Rich Controls Demo。
//!
//! 这是一个真实运行在 BerryOS 桌面上的产品面 demo：Berry Room 负责
//! WM/窗口/frame/输入/销毁；BerryUI/Berry Design 负责控件、主题、状态
//! 与页面组合。本 app 不解析 WM RPC、不映射 VMO、不计算 stride、不写
//! 像素，也不用 surface escape。

#![no_std]
#![no_main]

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_gfx::font::FontSize;
use berry_room::{
    AppRoom, INPUT_EVENTS_MAX, INPUT_SAMPLES_MAX, PointerEvent, RoomConfig, Sample,
    SYSTEM_FONT_ATLAS,
};
use berry_ui::hit::PtrEvent;
use berry_ui::sdk::{
    Button, Card, EmptyState, FontSource, IconSource, InteractionState, ListDetail,
    ListDetailSpec, ListItem, Msg, NodeId, RectI, Scaffold, ScrollView, Switch, TextVariant,
    TextField, TextFieldSpec, Theme, Ui, WidgetContext, column, divider, icon, row, text,
};

type Ui192 = Ui<192>;

const PAGE_HOME: u8 = 0;
const PAGE_CONTROLS: u8 = 1;
const PAGE_INPUT: u8 = 2;
const PAGE_LIST: u8 = 3;
const PAGE_THEME: u8 = 4;

struct RoomFontSource;
static ROOM_FONT_SOURCE: RoomFontSource = RoomFontSource;

impl FontSource for RoomFontSource {
    fn atlas(&self, scale: FontSize) -> Option<&'static dyn berry_gfx::font::GlyphAtlas> {
        (scale == FontSize::S32x48).then_some(&SYSTEM_FONT_ATLAS)
    }
}

// —— Demo 图标源：三个 48×48 常量 ARGB 图，证明普通 app 只经 IconSource
// 供给位图，不触碰 framebuffer/stride。 ——

const DEMO_ICON_DIM: u32 = 48;
const DEMO_ICON_PIXELS: usize = (DEMO_ICON_DIM * DEMO_ICON_DIM) as usize;

const fn icon_pixels(kind: u32) -> [u32; DEMO_ICON_PIXELS] {
    let mut out = [0u32; DEMO_ICON_PIXELS];
    let mut i = 0usize;
    while i < DEMO_ICON_PIXELS {
        let p = i as u32;
        let x = p % DEMO_ICON_DIM;
        let y = p / DEMO_ICON_DIM;
        let centered = (x >= 6 && x < 42 && y >= 6 && y < 42) as u32;
        let x4 = x / 4;
        let y4 = y / 4;
        let checker = ((x4 + y4) & 1) as u32;
        let diagonal = if x == y || x + 12 == y { 1 } else { 0 };
        let ring = {
            let dx = x as i32 - 24;
            let dy = y as i32 - 24;
            let distance = dx * dx + dy * dy;
            (distance >= 144 && distance <= 208) as u32
        };
        let mode = kind & 0b11;
        let r = match mode {
            0 => 32 + checker * 48 + diagonal * 80,
            1 => 210 - centered * 88,
            _ => 36 + ring * 190,
        };
        let g = match mode {
            0 => 96 + checker * 72 + ring * 70,
            1 => 116 + centered * 90,
            _ => 140 - checker * 66,
        };
        let b = match mode {
            0 => 210 - checker * 58,
            1 => 36 + diagonal * 110 + centered * 40,
            _ => 226 - ring * 82,
        };
        let alpha = if mode == 1 && centered == 0 { 0x30 } else { 0xFF };
        out[i] = (alpha << 24) | (r << 16) | (g << 8) | b;
        i += 1;
    }
    out
}

static ICON_GRID: [u32; DEMO_ICON_PIXELS] = icon_pixels(0);
static ICON_DOT: [u32; DEMO_ICON_PIXELS] = icon_pixels(1);
static ICON_RING: [u32; DEMO_ICON_PIXELS] = icon_pixels(2);

struct DemoIconSource;
static DEMO_ICONS: DemoIconSource = DemoIconSource;

impl IconSource for DemoIconSource {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        match id {
            100 | 101 | 102 => Some((DEMO_ICON_DIM, DEMO_ICON_DIM)),
            _ => None,
        }
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        match id {
            100 => Some(&ICON_GRID),
            101 => Some(&ICON_DOT),
            102 => Some(&ICON_RING),
            _ => None,
        }
    }

    fn smooth_scale(&self, _id: u16) -> bool {
        true
    }
}

fn append_u32(out: &mut [u8], mut value: u32) -> usize {
    let mut digits = [0u8; 10];
    let mut n = 0usize;
    if value == 0 {
        digits[0] = b'0';
        n = 1;
    }
    while value != 0 && n < digits.len() {
        digits[n] = b'0' + (value % 10) as u8;
        value /= 10;
        n += 1;
    }
    let mut written = 0usize;
    while n > 0 && written < out.len() {
        n -= 1;
        out[written] = digits[n];
        written += 1;
    }
    written
}

struct Line {
    bytes: [u8; 256],
    len: usize,
}

impl Line {
    fn new(head: &[u8]) -> Self {
        let mut bytes = [0u8; 256];
        let n = head.len().min(bytes.len());
        bytes[..n].copy_from_slice(&head[..n]);
        Self { bytes, len: n }
    }

    fn push(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(self.bytes.len() - self.len);
        self.bytes[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
    }

    fn push_u32(&mut self, value: u32) {
        let mut buf = [0u8; 10];
        let n = append_u32(&mut buf, value);
        self.push(&buf[..n]);
    }

    fn push_i32(&mut self, value: i32) {
        let mut buf = [0u8; 11];
        let mut n = if value < 0 {
            buf[0] = b'-';
            1
        } else {
            0
        };
        let unsigned = value.unsigned_abs();
        n += append_u32(&mut buf[n..], unsigned);
        self.push(&buf[..n]);
    }

    fn point(&mut self, x: i32, y: i32) {
        self.push_i32(x);
        self.push(b",");
        self.push_i32(y);
    }

    fn rect(&mut self, rect: RectI) {
        self.point(rect.x, rect.y);
        self.push(b",");
        self.point(rect.w, rect.h);
    }

    fn print(&self, room: &AppRoom) {
        room.print(&self.bytes[..self.len]);
    }
}

fn fail(room: &AppRoom, what: &str) -> ! {
    let mut line = [0u8; 112];
    let head = b"controls: ";
    line[..head.len()].copy_from_slice(head);
    let n = head.len() + what.len().min(line.len() - head.len());
    line[head.len()..n].copy_from_slice(&what.as_bytes()[..n - head.len()]);
    room.print(&line[..n]);
    room.exit(134)
}

fn add_panel<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    bg: Color,
    w: Option<u32>,
    h: Option<u32>,
    padding: u32,
    gap: u32,
    radius: u32,
) -> NodeId {
    let spec = berry_ui::sdk::ViewSpec {
        kind: berry_ui::sdk::ViewKind::Panel,
        key: 0,
        props: berry_ui::sdk::Props::Panel(berry_ui::sdk::PanelProps {
            bg,
            w,
            h,
            padding,
            gap,
            radius,
        }),
    };
    ui.add(parent, spec).unwrap_or_else(|_| berry_room::panic_exit(134))
}

fn color_chip<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    color: Color,
    size: u32,
) -> NodeId {
    let spec = berry_ui::sdk::ViewSpec {
        kind: berry_ui::sdk::ViewKind::Circle,
        key: 0,
        props: berry_ui::sdk::Props::Circle(berry_ui::sdk::CircleProps {
            bg: color,
            w: Some(size),
            h: Some(size),
        }),
    };
    ui.add(parent, spec).unwrap_or_else(|_| berry_room::panic_exit(134))
}

struct HomePage {
    root: NodeId,
    scroll: ScrollView,
    back: Button,
    tour: Button,
    nav: [Option<ListItem>; 4],
}

struct ControlsPage {
    root: NodeId,
    scroll: ScrollView,
    back: Button,
    counter: Button,
    switch: Switch,
    disabled: Button,
    selected: ListItem,
}

struct InputPage {
    root: NodeId,
    scroll: ScrollView,
    back: Button,
    fields: [TextField; 3],
}

struct ListPage {
    root: NodeId,
    scroll: ScrollView,
    back: Button,
    detail: Option<ListDetail>,
    items: [Option<ListItem>; 4],
    clear: Option<Button>,
    restore: Option<Button>,
}

struct ThemePage {
    root: NodeId,
    scroll: ScrollView,
    back: Button,
    cycle: Button,
}

enum Page {
    Home(HomePage),
    Controls(ControlsPage),
    Input(InputPage),
    List(ListPage),
    Theme(ThemePage),
}

impl Page {
    fn kind(&self) -> u8 {
        match self {
            Self::Home(_) => PAGE_HOME,
            Self::Controls(_) => PAGE_CONTROLS,
            Self::Input(_) => PAGE_INPUT,
            Self::List(_) => PAGE_LIST,
            Self::Theme(_) => PAGE_THEME,
        }
    }

    fn root(&self) -> NodeId {
        match self {
            Self::Home(page) => page.root,
            Self::Controls(page) => page.root,
            Self::Input(page) => page.root,
            Self::List(page) => page.root,
            Self::Theme(page) => page.root,
        }
    }

    fn scroll_mut(&mut self) -> &mut ScrollView {
        match self {
            Self::Home(page) => &mut page.scroll,
            Self::Controls(page) => &mut page.scroll,
            Self::Input(page) => &mut page.scroll,
            Self::List(page) => &mut page.scroll,
            Self::Theme(page) => &mut page.scroll,
        }
    }
}

struct App {
    ui: Ui192,
    ctx: WidgetContext,
    page: Page,
    theme_index: u8,
    clicks: u32,
    switch_on: bool,
    selected_item: u8,
    list_empty: bool,
    frame_seq: u32,
}

fn page_title(kind: u8) -> &'static [u8] {
    match kind {
        PAGE_CONTROLS => b"Controls",
        PAGE_INPUT => b"Inputs",
        PAGE_LIST => b"Lists",
        PAGE_THEME => b"Theme",
        _ => b"Berry Gallery",
    }
}

fn build_home(ui: &mut Ui192, ctx: &WidgetContext) -> HomePage {
    let root = ui.root();
    let scaffold = Scaffold::new(ui, root, ctx, page_title(PAGE_HOME))
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let (_, viewport_h) = ui.viewport();
    let scroll = ScrollView::new(ui, scaffold.body, ctx, viewport_h / 4 * 3)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let card = Card::new(ui, scroll.content, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let stack = column(ui, card.panel, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));

    let hero = add_panel(
        ui,
        stack,
        ctx.theme.colors.primary,
        None,
        None,
        ctx.metrics.spacing(berry_ui::sdk::SpacingToken::L),
        ctx.metrics.spacing(berry_ui::sdk::SpacingToken::S),
        ctx.metrics.radius(berry_ui::sdk::RadiusToken::Large),
    );
    text(ui, hero, ctx, TextVariant::Title, b"Berry Controls Gallery")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, hero, ctx, TextVariant::Body, b"Real widgets, real window, real input")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let chips = row(ui, hero, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    for color in [
        ctx.theme.colors.on_primary,
        ctx.theme.colors.success,
        ctx.theme.colors.warning,
        ctx.theme.colors.danger,
        ctx.theme.colors.surface_variant,
    ] {
        color_chip(ui, chips, color, ctx.metrics.spacing(berry_ui::sdk::SpacingToken::M));
    }

    divider(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Title, b"Control catalog")
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    let titles: [(&[u8], &[u8]); 4] = [
        (b"Buttons & states", b"Press, focus, disable, count"),
        (b"Text inputs", b"Text, number, password"),
        (b"Lists & panes", b"Selection, detail, empty state"),
        (b"Theme & density", b"Colors, spacing, radius, schemes"),
    ];
    let first = ListItem::new(ui, stack, ctx, titles[0].0, Some(titles[0].1), true)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let mut nav = [Some(first); 4];
    for index in 1..4 {
        nav[index] = Some(
            ListItem::new(ui, stack, ctx, titles[index].0, Some(titles[index].1), false)
                .unwrap_or_else(|_| berry_room::panic_exit(134)),
        );
    }

    let tour = Button::new(ui, stack, ctx, b"Start tour", InteractionState::Selected)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let back = Button::new(ui, stack, ctx, b"Exit", InteractionState::Normal)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Body, b"BerryUI + Berry Room + Berry Design")
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    HomePage { root: scaffold.root, scroll, back, tour, nav }
}

fn build_controls(ui: &mut Ui192, ctx: &WidgetContext, clicks: u32, switch_on: bool) -> ControlsPage {
    let root = ui.root();
    let scaffold = Scaffold::new(ui, root, ctx, page_title(PAGE_CONTROLS))
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let (_, viewport_h) = ui.viewport();
    let scroll = ScrollView::new(ui, scaffold.body, ctx, viewport_h / 4 * 3)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let card = Card::new(ui, scroll.content, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let stack = column(ui, card.panel, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));

    let mut counter_label = [b'0'; 32];
    let head = b"Press me ";
    counter_label[..head.len()].copy_from_slice(head);
    let mut n = append_u32(&mut counter_label[head.len()..], clicks);
    n += head.len();

    let counter = Button::new(
        ui,
        stack,
        ctx,
        &counter_label[..n],
        InteractionState::Normal,
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));
    let switch = Switch::new(ui, stack, ctx, switch_on, false)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Body, b"Switch above is live")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let disabled = Button::new(ui, stack, ctx, b"Disabled", InteractionState::Disabled)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let selected = ListItem::new(
        ui,
        stack,
        ctx,
        b"Selected item",
        Some(b"List selection state"),
        true,
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));

    divider(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Title, b"Progress")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let track = add_panel(
        ui,
        stack,
        ctx.theme.colors.surface_variant,
        None,
        Some(ctx.metrics.spacing(berry_ui::sdk::SpacingToken::S)),
        0,
        0,
        ctx.metrics.radius(berry_ui::sdk::RadiusToken::Small),
    );
    let progress_spec = berry_ui::sdk::ViewSpec {
        kind: berry_ui::sdk::ViewKind::Rect,
        key: 0,
        props: berry_ui::sdk::Props::Rect(berry_ui::sdk::RectProps {
            bg: ctx.theme.colors.success,
            w: Some(640),
            h: Some(ctx.metrics.spacing(berry_ui::sdk::SpacingToken::S)),
        }),
    };
    ui.add(track, progress_spec).unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Body, b"64% demo progress")
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    let icons = row(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    icon(ui, icons, ctx, 100).unwrap_or_else(|_| berry_room::panic_exit(134));
    icon(ui, icons, ctx, 101).unwrap_or_else(|_| berry_room::panic_exit(134));
    icon(ui, icons, ctx, 102).unwrap_or_else(|_| berry_room::panic_exit(134));

    let back = Button::new(ui, stack, ctx, b"Back", InteractionState::Normal)
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    ControlsPage { root: scaffold.root, scroll, back, counter, switch, disabled, selected }
}

fn build_input(ui: &mut Ui192, ctx: &WidgetContext) -> InputPage {
    let root = ui.root();
    let scaffold = Scaffold::new(ui, root, ctx, page_title(PAGE_INPUT))
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let (_, viewport_h) = ui.viewport();
    let scroll = ScrollView::new(ui, scaffold.body, ctx, viewport_h / 4 * 3)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let card = Card::new(ui, scroll.content, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let stack = column(ui, card.panel, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));

    text(ui, stack, ctx, TextVariant::Title, b"Text fields")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let width = (ui.viewport().0.saturating_sub(120)).min(960);
    text(ui, stack, ctx, TextVariant::Body, b"Plain text")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let kinds: [(berry_ui::sdk::TextInputKind, &[u8]); 3] = [
        (berry_ui::sdk::TextInputKind::Text, b"Plain text"),
        (berry_ui::sdk::TextInputKind::Number, b"Number only"),
        (berry_ui::sdk::TextInputKind::Password, b"Password"),
    ];
    let first = TextField::new(ui, stack, ctx, TextFieldSpec { kind: kinds[0].0, width })
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let mut fields = [first; 3];
    for index in 1..3 {
        text(ui, stack, ctx, TextVariant::Body, kinds[index].1)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        fields[index] = TextField::new(
            ui,
            stack,
            ctx,
            TextFieldSpec { kind: kinds[index].0, width },
        )
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    }
    text(ui, stack, ctx, TextVariant::Body, b"Tap a field to focus its caret")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let back = Button::new(ui, stack, ctx, b"Back", InteractionState::Normal)
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    InputPage { root: scaffold.root, scroll, back, fields }
}

fn build_list(ui: &mut Ui192, ctx: &WidgetContext, selected: u8, empty: bool) -> ListPage {
    let root = ui.root();
    let scaffold = Scaffold::new(ui, root, ctx, page_title(PAGE_LIST))
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let (_, viewport_h) = ui.viewport();
    let scroll = ScrollView::new(ui, scaffold.body, ctx, viewport_h / 4 * 3)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let card = Card::new(ui, scroll.content, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let stack = column(ui, card.panel, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));

    if empty {
        EmptyState::new(ui, stack, ctx, 102, b"No items", b"Restore the demo list")
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let restore = Button::new(ui, stack, ctx, b"Restore list", InteractionState::Normal)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let back = Button::new(ui, stack, ctx, b"Back", InteractionState::Normal)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        return ListPage {
            root: scaffold.root,
            scroll,
            back,
            detail: None,
            items: [None; 4],
            clear: None,
            restore: Some(restore),
        };
    }

    let detail = ListDetail::new(
        ui,
        stack,
        ctx,
        ListDetailSpec {
            width: 984,
            height: 600,
            gap: ctx.metrics.spacing(berry_ui::sdk::SpacingToken::S),
            list_weight: 1,
            detail_weight: 2,
        },
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));

    let titles: [&[u8]; 4] = [b"Buttons", b"Inputs", b"Lists", b"Theme"];
    let first = ListItem::new(ui, detail.list, ctx, titles[0], None, selected == 0)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let mut items = [Some(first); 4];
    for index in 1..4 {
        items[index] = Some(
            ListItem::new(ui, detail.list, ctx, titles[index], None, selected == index as u8)
                .unwrap_or_else(|_| berry_room::panic_exit(134)),
        );
    }
    icon(ui, detail.detail, ctx, 101).unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, detail.detail, ctx, TextVariant::Title, b"Detail pane")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, detail.detail, ctx, TextVariant::Body, b"ListDetail splits available width 1:2")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, detail.detail, ctx, TextVariant::Body, b"Selection is app state")
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    divider(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let clear = Button::new(ui, stack, ctx, b"Clear list", InteractionState::Normal)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let back = Button::new(ui, stack, ctx, b"Back", InteractionState::Normal)
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    ListPage {
        root: scaffold.root,
        scroll,
        back,
        detail: Some(detail),
        items,
        clear: Some(clear),
        restore: None,
    }
}

fn build_theme(ui: &mut Ui192, ctx: &WidgetContext) -> ThemePage {
    let root = ui.root();
    let scaffold = Scaffold::new(ui, root, ctx, page_title(PAGE_THEME))
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let (_, viewport_h) = ui.viewport();
    let scroll = ScrollView::new(ui, scaffold.body, ctx, viewport_h / 4 * 3)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let card = Card::new(ui, scroll.content, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let stack = column(ui, card.panel, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));

    text(ui, stack, ctx, TextVariant::Title, b"Theme tokens")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let palette = row(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let colors = [
        ctx.theme.colors.primary,
        ctx.theme.colors.on_primary,
        ctx.theme.colors.success,
        ctx.theme.colors.warning,
        ctx.theme.colors.danger,
        ctx.theme.colors.surface,
        ctx.theme.colors.surface_variant,
        ctx.theme.colors.outline,
        ctx.theme.colors.text_primary,
        ctx.theme.colors.text_secondary,
    ];
    for color in colors {
        color_chip(ui, palette, color, ctx.metrics.spacing(berry_ui::sdk::SpacingToken::M));
    }

    divider(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Body, b"Spacing and radius are density-derived")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let radius_board = row(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    for token in [
        berry_ui::sdk::RadiusToken::Small,
        berry_ui::sdk::RadiusToken::Medium,
        berry_ui::sdk::RadiusToken::Large,
    ] {
        let size = ctx.metrics.spacing(berry_ui::sdk::SpacingToken::L);
        add_panel(ui, radius_board, ctx.theme.colors.primary, Some(size), Some(size), 0, 0, ctx.metrics.radius(token));
    }

    let icons = row(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    icon(ui, icons, ctx, 100).unwrap_or_else(|_| berry_room::panic_exit(134));
    icon(ui, icons, ctx, 102).unwrap_or_else(|_| berry_room::panic_exit(134));

    let cycle = Button::new(ui, stack, ctx, b"Cycle scheme", InteractionState::Selected)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Body, b"Light / dark / high contrast")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let back = Button::new(ui, stack, ctx, b"Back", InteractionState::Normal)
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    ThemePage { root: scaffold.root, scroll, back, cycle }
}

fn build_page(ui: &mut Ui192, ctx: &WidgetContext, kind: u8, clicks: u32, switch_on: bool, selected: u8, empty: bool) -> Page {
    match kind {
        PAGE_CONTROLS => Page::Controls(build_controls(ui, ctx, clicks, switch_on)),
        PAGE_INPUT => Page::Input(build_input(ui, ctx)),
        PAGE_LIST => Page::List(build_list(ui, ctx, selected, empty)),
        PAGE_THEME => Page::Theme(build_theme(ui, ctx)),
        _ => Page::Home(build_home(ui, ctx)),
    }
}

fn pointer_event(event: &PointerEvent) -> PtrEvent {
    PtrEvent::from_wire(
        event.phase,
        event.pointer_id,
        event.x,
        event.y,
        event.timestamp_ns,
        event.buttons,
        event.pressure,
    )
}

fn empty_event() -> PointerEvent {
    PointerEvent {
        phase: 0,
        device_id: 0,
        buttons: 0,
        flags: 0,
        pointer_id: 0,
        timestamp_ns: 0,
        x: 0,
        y: 0,
        pressure: 0,
        contact_major: 0,
        contact_minor: 0,
        history: [Sample { timestamp_ns: 0, x: 0, y: 0 }; INPUT_SAMPLES_MAX - 1],
        history_len: 0,
    }
}

impl App {
    fn render(&mut self, room: &mut AppRoom) {
        let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 192];
        let mut encoder = CommandEncoder::new();
        self.ui.wake(0);
        let outcome = self.ui.render_frame(0, &mut scratch, &mut encoder);
        if !outcome.drew {
            fail(room, "no damage");
        }
        let list = encoder.finish();
        let commands = match room.render_display_list(&list) {
            Ok(commands) => commands,
            Err(error) => fail(room, error.as_str()),
        };
        self.frame_seq = self.frame_seq.saturating_add(1);
        if let Err(error) = room.submit(self.frame_seq) {
            fail(room, error.as_str());
        }
        let mut line = Line::new(b"CONTROLSRENDER ok seq=");
        line.push_u32(self.frame_seq);
        line.push(b" cmds=");
        line.push_u32(commands as u32);
        line.push(b"\n");
        line.print(room);
        room.print(b"CONTROLSFRAME ok\n");
    }

    fn print_layout(&self, room: &AppRoom) {
        let mut line = Line::new(b"CONTROLSLAYOUT ok page=");
        line.push_u32(self.page.kind() as u32);
        let (back, extra) = match &self.page {
            Page::Home(page) => (page.back.panel, page.nav[0].as_ref().map(|item| item.panel)),
            Page::Controls(page) => (page.back.panel, Some(page.switch.panel)),
            Page::Input(page) => (page.back.panel, Some(page.fields[0].panel)),
            Page::List(page) => (
                page.back.panel,
                page.detail.as_ref().map(|detail| detail.row),
            ),
            Page::Theme(page) => (page.back.panel, Some(page.cycle.panel)),
        };
        line.push(b" back=");
        line.rect(self.ui.bounds(back).unwrap_or_else(|_| berry_room::panic_exit(134)));
        if let Some(node) = extra {
            line.push(b" target=");
            line.rect(self.ui.bounds(node).unwrap_or_else(|_| berry_room::panic_exit(134)));
        }
        line.push(b"\n");
        line.print(room);
    }

    fn replace_page(&mut self, room: &mut AppRoom, kind: u8) {
        if self.ui.remove(self.page.root()).is_err() {
            fail(room, "page remove");
        }
        self.page = build_page(
            &mut self.ui,
            &self.ctx,
            kind,
            self.clicks,
            self.switch_on,
            self.selected_item,
            self.list_empty,
        );
        self.page
            .scroll_mut()
            .sync_bounds(&mut self.ui)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        self.render(room);
        self.print_layout(room);
        let mut line = Line::new(b"CONTROLPAGE ok page=");
        line.push_u32(kind as u32);
        line.push(b"\n");
        line.print(room);
    }

    fn deliver_widget(&mut self, message: &Msg) -> bool {
        let mut handled = false;
        match &mut self.page {
            Page::Home(page) => {
                handled |= page.back.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.tour.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                for item in page.nav.iter_mut().flatten() {
                    handled |= item.on_msg(&mut self.ui, &self.ctx, message)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                }
            }
            Page::Controls(page) => {
                handled |= page.back.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.counter.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.disabled.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.switch.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.selected.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
            }
            Page::Input(page) => {
                handled |= page.back.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                for field in page.fields.iter_mut() {
                    handled |= field.on_msg(&mut self.ui, &self.ctx, message)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                }
            }
            Page::List(page) => {
                handled |= page.back.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                if let Some(button) = page.restore.as_mut() {
                    handled |= button.on_msg(&mut self.ui, &self.ctx, message)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                }
                if let Some(button) = page.clear.as_mut() {
                    handled |= button.on_msg(&mut self.ui, &self.ctx, message)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                }
                for item in page.items.iter_mut().flatten() {
                    handled |= item.on_msg(&mut self.ui, &self.ctx, message)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                }
            }
            Page::Theme(page) => {
                handled |= page.back.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.cycle.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
            }
        }
        handled |= self
            .page
            .scroll_mut()
            .on_msg(&mut self.ui, message)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        handled
    }

    fn handle_message(&mut self, room: &mut AppRoom, message: &Msg, x: u32, y: u32) -> bool {
        let Msg::Click { node } = *message else {
            return false;
        };
        let _ = self.deliver_widget(message);

        let mut hit_line = Line::new(b"CONTROLSHIT ok node=");
        let mut action: u8 = 0;
        let mut focused_field: u32 = 0;
        let mut target_page = self.page.kind();

        match &mut self.page {
            Page::Home(page) => {
                if page.back.panel == node || page.back.label == node {
                    hit_line.push(b"exit");
                    action = 1;
                } else if page.tour.panel == node || page.tour.label == node {
                    hit_line.push(b"tour");
                    target_page = PAGE_CONTROLS;
                    action = 2;
                } else if let Some(index) = page.nav.iter().flatten().position(|item| {
                    item.panel == node
                        || item.title == node
                        || item.detail.is_some_and(|detail| detail == node)
                }) {
                    hit_line.push(b"nav");
                    target_page = (index as u8) + 1;
                    action = 2;
                }
            }
            Page::Controls(page) => {
                if page.back.panel == node || page.back.label == node {
                    hit_line.push(b"back");
                    target_page = PAGE_HOME;
                    action = 2;
                } else if page.counter.panel == node || page.counter.label == node {
                    hit_line.push(b"counter");
                    self.clicks = self.clicks.saturating_add(1);
                    let mut label = [b'0'; 32];
                    let head = b"Press me ";
                    label[..head.len()].copy_from_slice(head);
                    let n = append_u32(&mut label[head.len()..], self.clicks) + head.len();
                    self.ui.set_text(page.counter.label, &label[..n])
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    action = 3;
                } else if page.switch.panel == node
                    || page.switch.track == node
                    || page.switch.spacer == node
                    || page.switch.thumb == node
                {
                    hit_line.push(b"switch");
                    self.switch_on = page.switch.is_checked();
                    action = 4;
                }
            }
            Page::Input(page) => {
                if page.back.panel == node || page.back.label == node {
                    hit_line.push(b"back");
                    target_page = PAGE_HOME;
                    action = 2;
                } else if let Some(index) = page.fields.iter().position(|field| field.owns(node)) {
                    hit_line.push(b"field");
                    let mut field = page.fields[index];
                    let messages = field.request_focus(&mut self.ui, &self.ctx)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    page.fields[index] = field;
                    for focus in messages.iter() {
                        let _ = self.deliver_widget(&focus);
                    }
                    focused_field = index as u32;
                    action = 5;
                }
            }
            Page::List(page) => {
                if page.back.panel == node || page.back.label == node {
                    hit_line.push(b"back");
                    target_page = PAGE_HOME;
                    action = 2;
                } else if let Some(button) = page.restore.as_ref() {
                    if button.panel == node || button.label == node {
                        hit_line.push(b"restore");
                        self.list_empty = false;
                        action = 8;
                    }
                } else if let Some(button) = page.clear.as_ref() {
                    if button.panel == node || button.label == node {
                        hit_line.push(b"clear");
                        self.list_empty = true;
                        action = 8;
                    }
                } else if let Some(index) = page.items.iter().flatten().position(|item| {
                    item.panel == node
                        || item.title == node
                        || item.detail.is_some_and(|detail| detail == node)
                }) {
                    hit_line.push(b"list");
                    self.selected_item = index as u8;
                    action = 6;
                }
            }
            Page::Theme(page) => {
                if page.back.panel == node || page.back.label == node {
                    hit_line.push(b"back");
                    target_page = PAGE_HOME;
                    action = 2;
                } else if page.cycle.panel == node || page.cycle.label == node {
                    hit_line.push(b"theme");
                    self.theme_index = (self.theme_index + 1) % 3;
                    action = 7;
                }
            }
        }

        if action == 0 {
            return false;
        }
        hit_line.push(b" x=");
        hit_line.push_u32(x);
        hit_line.push(b" y=");
        hit_line.push_u32(y);
        hit_line.push(b"\n");
        hit_line.print(room);

        match action {
            1 => true,
            2 => {
                if self.page.kind() != target_page {
                    self.replace_page(room, target_page);
                } else {
                    self.render(room);
                }
                false
            }
            3 => {
                let marker: &[u8] = if self.clicks % 2 == 0 {
                    b" even\n"
                } else {
                    b" odd\n"
                };
                let mut line = Line::new(b"CONTROLSCOUNTER ok n=");
                line.push_u32(self.clicks);
                line.push(marker);
                line.print(room);
                self.render(room);
                false
            }
            4 => {
                let mut line = Line::new(b"CONTROLSSWITCH ok checked=");
                line.push_u32(self.switch_on as u32);
                line.push(b"\n");
                line.print(room);
                self.render(room);
                false
            }
            5 => {
                let mut line = Line::new(b"CONTROLSFOCUS ok field=");
                line.push_u32(focused_field);
                line.push(b"\n");
                line.print(room);
                self.render(room);
                false
            }
            6 => {
                let mut line = Line::new(b"CONTROLSLIST ok selected=");
                line.push_u32(self.selected_item as u32);
                line.push(b"\n");
                line.print(room);
                // Rebuild preserves app selection and updates all item states.
                self.replace_page(room, PAGE_LIST);
                false
            }
            7 => {
                let theme = match self.theme_index {
                    0 => Theme::light(),
                    1 => Theme::dark(),
                    _ => Theme::high_contrast_light(),
                };
                self.ctx = WidgetContext::new(theme, self.ctx.metrics);
                let mut line = Line::new(b"CONTROLSTHEME ok scheme=");
                line.push_u32(self.theme_index as u32);
                line.push(b"\n");
                line.print(room);
                self.replace_page(room, PAGE_THEME);
                false
            }
            8 => {
                // EmptyState <-> list 是同页形态切换，必须强制重建。
                self.replace_page(room, PAGE_LIST);
                false
            }
            _ => false,
        }
    }
}

fn run_interactive(app: &mut App, room: &mut AppRoom) {
    let mut events = [empty_event(); INPUT_EVENTS_MAX];
    loop {
        let count = match room.poll_input(&mut events) {
            Ok(count) => count,
            Err(error) => fail(room, error.as_str()),
        };
        for event in events.iter_mut().take(count) {
            let (x, y) = (event.x, event.y);
            if app.ui.push_pointer(pointer_event(event)).is_err() {
                fail(room, "input buffer");
            }
            let messages = app.ui.pump_input(0);
            let mut copied = [None; berry_ui::MSG_MAX];
            for (index, message) in messages.iter().enumerate() {
                if index < copied.len() {
                    copied[index] = Some(*message);
                }
            }
            for slot in copied.iter().take(messages.len()) {
                let Some(message) = *slot else { continue };
                if app.handle_message(room, &message, x, y) {
                    room.print(b"CONTROLSBACK ok\n");
                    return;
                }
            }
        }
    }
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env_ch: u32) -> ! {
    let config = RoomConfig {
        argv0: b"bin/controls",
        uses_fs: false,
        surface_escape: false,
    };
    let mut room = match AppRoom::connect(env_ch, config) {
        Ok(room) => room,
        Err(_) => berry_room::panic_exit(134),
    };
    room.print(b"CONTROLS up\n");
    let (width, height) = match room.open_window() {
        Ok(size) => size,
        Err(error) => fail(&room, error.as_str()),
    };
    room.print(b"CONTROLSWIN ok\n");

    let mut ui = Ui192::new(width as u32, height as u32);
    let ctx = WidgetContext::for_ui(Theme::light(), &ui);
    ui.set_font_source(&ROOM_FONT_SOURCE);
    ui.set_icon_source(&DEMO_ICONS);
    let page = build_page(&mut ui, &ctx, PAGE_HOME, 0, false, 0, false);
    let mut app = App {
        ui,
        ctx,
        page,
        theme_index: 0,
        clicks: 0,
        switch_on: false,
        selected_item: 0,
        list_empty: false,
        frame_seq: 0,
    };
    app.page
        .scroll_mut()
        .sync_bounds(&mut app.ui)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    app.print_layout(&room);
    app.render(&mut room);

    if room.boot_probe() {
        room.print(b"CONTROLSPROBE ok\n");
    } else {
        run_interactive(&mut app, &mut room);
    }

    if let Err(error) = room.destroy() {
        fail(&room, error.as_str());
    }
    room.print(b"CONTROLSDESTROY ok\n");
    room.print(b"CONTROLSEXIT ok\n");
    room.exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_room::panic_exit(134)
}
