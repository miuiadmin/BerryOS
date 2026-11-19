//! BERRYDEMO —— Berry Demo。
//!
//! 这是一个真实运行在 BerryOS 桌面上的系统级 demo：Berry Room 负责
//! WM/窗口/frame/输入/销毁；BerryUI/Berry Design 负责控件、主题、状态
//! 与页面组合。本 app 不解析 WM RPC、不映射 VMO、不计算 stride、不写
//! 像素，也不用 surface escape；manifest/.bap/Berry Room/BerryUI/IconSource
//! 同页展示系统特性与应用开发标准。

#![no_std]
#![no_main]

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_gfx::font::FontSize;
use berry_room::{
    AppRoom, INPUT_EVENTS_MAX, INPUT_KEY_EVENTS_MAX, INPUT_SAMPLES_MAX, KeyEvent, PointerEvent,
    RoomConfig, Sample, SurfaceModeKind, SurfaceRequest, SYSTEM_FONT_ATLAS,
    SURFACE_A11Y_TREE, SURFACE_IME_DISABLED, SURFACE_INPUT_KEYBOARD, SURFACE_INPUT_TOUCH,
};
use berry_ui::hit::PtrEvent;
use berry_ui::sdk::{
    Button, Card, Checkbox, EmptyState, FontSource, IconSource, InteractionState, ListDetail,
    ListDetailSpec, ListItem, ListTile, ListTileSpec, LoadingIndicator, Msg, NodeId,
    ProgressIndicator, Radio, RectI, Scaffold, ScrollView, SearchField, Section,
    SegmentedControl, SegmentedItemSpec, Slider, Stepper, Switch, TabBarStyle, TabItemSpec,
    Tabs, TextPicker, TextVariant, TextArea, TextAreaSpec, TextField, TextFieldSlots,
    TextFieldSlotsSpec,
    TextFieldSpec, Theme, Ui, WidgetContext, column, FieldMessage, FieldMessageKind,
    divider, icon, row, text, Badge, Chip, ChipKind, IconButton, NavigationIconSource,
    ScrollViewAxis, KeyRoute, TextInputKeyOp, TextInputKind, key_route,
    NAV_ICON_BACK, NAV_ICON_BACK_ACTIVE, TextInputState,
};

type Ui192 = Ui<192>;

const PAGE_HOME: u8 = 0;
const PAGE_WIDGETS: u8 = 1;
const PAGE_INPUT: u8 = 2;
const PAGE_LIST: u8 = 3;
const PAGE_THEME: u8 = 4;
const PAGE_SYSTEM: u8 = 5;

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

static NAVIGATION_ICONS: NavigationIconSource = NavigationIconSource;

impl IconSource for DemoIconSource {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        NAVIGATION_ICONS.dims(id).or(match id {
            100 | 101 | 102 => Some((DEMO_ICON_DIM, DEMO_ICON_DIM)),
            _ => None,
        })
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        NAVIGATION_ICONS
            .pixels(id)
            .or(match id {
                100 => Some(&ICON_GRID),
                101 => Some(&ICON_DOT),
                102 => Some(&ICON_RING),
                _ => None,
            })
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
    let head = b"berrydemo: ";
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
    tabs: Tabs,
    back: Button,
    tour: Button,
    nav: [Option<ListTile>; 5],
}

struct WidgetsPage {
    root: NodeId,
    card: Card,
    scroll: ScrollView,
    tabs: Tabs,
    back: IconButton,
    counter: Button,
    switch: Switch,
    disabled: Button,
    selected: ListItem,
    checkbox: Checkbox,
    radio: Radio,
    slider: Slider,
    loading: LoadingIndicator,
    filter: Chip,
    badge: Badge,
    icon_strip: ScrollView,
}

struct InputPage {
    root: NodeId,
    scroll: ScrollView,
    tabs: Tabs,
    back: IconButton,
    fields: [TextField; 3],
    labelled: TextFieldSlots,
    labelled_error: bool,
    area: TextArea,
    area_state: TextInputState,
    search: SearchField,
    search_state: TextInputState,
}

struct ListPage {
    root: NodeId,
    scroll: ScrollView,
    tabs: Tabs,
    back: IconButton,
    detail: Option<ListDetail>,
    items: [Option<ListItem>; 4],
    segments: SegmentedControl,
    clear: Option<Button>,
    restore: Option<Button>,
}

struct ThemePage {
    root: NodeId,
    scroll: ScrollView,
    tabs: Tabs,
    back: IconButton,
    info: IconButton,
    cycle: Button,
    picker: TextPicker,
}

struct SystemPage {
    root: NodeId,
    scroll: ScrollView,
    tabs: Tabs,
    back: IconButton,
    stepper: Stepper,
}

enum Page {
    Home(HomePage),
    Widgets(WidgetsPage),
    Input(InputPage),
    List(ListPage),
    Theme(ThemePage),
    System(SystemPage),
}

impl Page {
    fn kind(&self) -> u8 {
        match self {
            Self::Home(_) => PAGE_HOME,
            Self::Widgets(_) => PAGE_WIDGETS,
            Self::Input(_) => PAGE_INPUT,
            Self::List(_) => PAGE_LIST,
            Self::Theme(_) => PAGE_THEME,
            Self::System(_) => PAGE_SYSTEM,
        }
    }

    fn root(&self) -> NodeId {
        match self {
            Self::Home(page) => page.root,
            Self::Widgets(page) => page.root,
            Self::Input(page) => page.root,
            Self::List(page) => page.root,
            Self::Theme(page) => page.root,
            Self::System(page) => page.root,
        }
    }

    fn scroll_mut(&mut self) -> &mut ScrollView {
        match self {
            Self::Home(page) => &mut page.scroll,
            Self::Widgets(page) => &mut page.scroll,
            Self::Input(page) => &mut page.scroll,
            Self::List(page) => &mut page.scroll,
            Self::Theme(page) => &mut page.scroll,
            Self::System(page) => &mut page.scroll,
        }
    }

    fn tabs_selected(&self) -> u8 {
        match self {
            Self::Home(page) => page.tabs.selected() as u8,
            Self::Widgets(page) => page.tabs.selected() as u8,
            Self::Input(page) => page.tabs.selected() as u8,
            Self::List(page) => page.tabs.selected() as u8,
            Self::Theme(page) => page.tabs.selected() as u8,
            Self::System(page) => page.tabs.selected() as u8,
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
    ta_edits: u32,
    frame_seq: u32,
    safe_top: u32,
    safe_bottom: u32,
}

const BOTTOM_TAB_ITEMS: [TabItemSpec; 6] = [
    TabItemSpec { label: b"Home", icon: Some(100) },
    TabItemSpec { label: b"Widgets", icon: Some(101) },
    TabItemSpec { label: b"Inputs", icon: Some(102) },
    TabItemSpec { label: b"Lists", icon: Some(100) },
    TabItemSpec { label: b"Theme", icon: Some(101) },
    TabItemSpec { label: b"System", icon: Some(102) },
];

fn bottom_tab_height(ctx: &WidgetContext) -> u32 {
    ctx.metrics.effective_px(144)
}

fn build_bottom_tabs<const N: usize>(
    ui: &mut Ui<N>,
    parent: NodeId,
    ctx: &WidgetContext,
    kind: u8,
) -> Tabs {
    let mut tabs = Tabs::new(
        ui,
        parent,
        ctx,
        TabBarStyle::Bottom,
        &BOTTOM_TAB_ITEMS,
        kind as usize,
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));
    // Inputs owns a persistent demo count. The badge is visual-only: no new
    // a11y/SEM anchor is attached and its hit remains part of the owning tab.
    tabs.set_badge(ui, ctx, 2, 3)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    tabs
}

fn page_body(
    ui: &mut Ui192,
    ctx: &WidgetContext,
    kind: u8,
    safe_top: u32,
    safe_bottom: u32,
) -> (Scaffold, ScrollView, Tabs) {
    let root = ui.root();
    let bottom_h = bottom_tab_height(ctx);
    let scaffold = if kind == PAGE_HOME {
        Scaffold::new_light_bottom_safe(
            ui,
            root,
            ctx,
            page_title(kind),
            bottom_h,
            safe_top,
            safe_bottom,
        )
    } else {
        Scaffold::with_back_bottom_safe(
            ui,
            root,
            ctx,
            page_title(kind),
            NAV_ICON_BACK,
            NAV_ICON_BACK_ACTIVE,
            bottom_h,
            safe_top,
            safe_bottom,
        )
    }
    .unwrap_or_else(|_| berry_room::panic_exit(134));
    let scroll = ScrollView::new(ui, scaffold.body, ctx, scaffold.body_height())
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let bottom = scaffold
        .bottom
        .unwrap_or_else(|| berry_room::panic_exit(134));
    let tabs = build_bottom_tabs(ui, bottom, ctx, kind);
    (scaffold, scroll, tabs)
}

fn page_title(kind: u8) -> &'static [u8] {
    match kind {
        PAGE_WIDGETS => b"Widgets",
        PAGE_INPUT => b"Inputs",
        PAGE_LIST => b"Lists",
        PAGE_THEME => b"Theme",
        PAGE_SYSTEM => b"System",
        _ => b"Berry Gallery",
    }
}

fn build_home(
    ui: &mut Ui192, ctx: &WidgetContext, safe_top: u32, safe_bottom: u32,
) -> HomePage {
    let (scaffold, scroll, tabs) = page_body(
        ui,
        ctx,
        PAGE_HOME,
        safe_top,
        safe_bottom,
    );
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
    text(ui, hero, ctx, TextVariant::Title, b"Berry Demo Gallery")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, hero, ctx, TextVariant::Body, b"System features and app standards")
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
    text(ui, stack, ctx, TextVariant::Title, b"Demo catalog")
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    let titles: [(&[u8], &[u8]); 5] = [
        (b"Buttons & states", b"Press, focus, disable, count"),
        (b"Text inputs", b"Text, number, password"),
        (b"Lists & panes", b"Selection, detail, empty state"),
        (b"Theme & density", b"Colors, spacing, radius, schemes"),
        (b"System & standards", b"Spawn, package, window, lifecycle"),
    ];
    let icons: [u16; 5] = [100, 101, 102, 100, 101];
    let tile_width = ui.viewport().0.saturating_sub(24);
    let first = ListTile::new(
        ui,
        stack,
        ctx,
        &ListTileSpec {
            title: titles[0].0,
            detail: Some(titles[0].1),
            leading_icon: icons[0],
            trailing: b">",
            selected: false,
            width: tile_width,
        },
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));
    let mut nav = [Some(first); 5];
    for index in 1..5 {
        nav[index] = Some(
            ListTile::new(
                ui,
                stack,
                ctx,
                &ListTileSpec {
                    title: titles[index].0,
                    detail: Some(titles[index].1),
                    leading_icon: icons[index],
                    trailing: b">",
                    selected: false,
                    width: tile_width,
                },
            )
            .unwrap_or_else(|_| berry_room::panic_exit(134)),
        );
    }

    let tour = Button::new(ui, stack, ctx, b"Start tour", InteractionState::Selected)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let back = Button::new(ui, stack, ctx, b"Exit", InteractionState::Normal)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Body, b"BerryUI + Berry Room + Berry Design")
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    HomePage { root: scaffold.root, scroll, tabs, back, tour, nav }
}

fn build_widgets(
    ui: &mut Ui192, ctx: &WidgetContext, clicks: u32, switch_on: bool, safe_top: u32,
    safe_bottom: u32,
) -> WidgetsPage {
    let (scaffold, scroll, tabs) = page_body(
        ui,
        ctx,
        PAGE_WIDGETS,
        safe_top,
        safe_bottom,
    );
    let app_back = scaffold
        .app_bar
        .back
        .unwrap_or_else(|| berry_room::panic_exit(134));
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

    let token_row = row(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let filter = Chip::new(
        ui,
        token_row,
        ctx,
        b"Live filter",
        ChipKind::Filter,
        false,
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));
    let badge = Badge::new(
        ui,
        token_row,
        ctx,
        3,
        ctx.metrics.effective_px(240) as i32,
        0,
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));
    let _ = badge;

    let selection_section = Section::new(ui, stack, ctx, b"Selection and value")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let selection_row = row(ui, selection_section.content, ctx)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let checkbox = Checkbox::new(ui, selection_row, ctx, false, false)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let radio = Radio::new(ui, selection_row, ctx, false, false, b"Berry radio")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let slider = Slider::new(ui, selection_section.content, ctx, 500, false, b"Brightness")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let loading = LoadingIndicator::new(ui, selection_section.content, ctx)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    loading
        .set_phase(ui, ctx, 1)
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    divider(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Title, b"Progress")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let _progress = ProgressIndicator::new(ui, stack, ctx, 984, 640)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Body, b"64% demo progress")
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    let mut icon_strip = ScrollView::new_axis(
        ui,
        stack,
        ctx,
        ctx.metrics.effective_px(1280),
        ScrollViewAxis::Horizontal,
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));
    for icon_id in [100, 101, 102] {
        icon(ui, icon_strip.content, ctx, icon_id)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
    }
    icon_strip
        .sync_bounds(ui)
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    WidgetsPage {
        root: scaffold.root,
        card,
        scroll,
        tabs,
        back: app_back,
        counter,
        switch,
        disabled,
        selected,
        checkbox,
        radio,
        slider,
        loading,
        filter,
        badge,
        icon_strip,
    }
}

fn build_input(
    ui: &mut Ui192, ctx: &WidgetContext, safe_top: u32, safe_bottom: u32,
) -> InputPage {
    let (scaffold, scroll, tabs) = page_body(
        ui,
        ctx,
        PAGE_INPUT,
        safe_top,
        safe_bottom,
    );
    let app_back = scaffold
        .app_bar
        .back
        .unwrap_or_else(|| berry_room::panic_exit(134));
    let card = Card::new(ui, scroll.content, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let stack = column(ui, card.panel, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));

    text(ui, stack, ctx, TextVariant::Title, b"Text fields")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let width = (ui.viewport().0.saturating_sub(120)).min(960);
    let mut search = SearchField::new(ui, stack, ctx, width, 100, 102)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let search_state = TextInputState::new(b"Berry")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    search
        .field
        .sync(ui, ctx, &search_state)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let labelled = TextFieldSlots::new(
        ui,
        stack,
        ctx,
        TextFieldSlotsSpec {
            field: TextFieldSpec { kind: berry_ui::sdk::TextInputKind::Text, width },
            label: Some(b"User name"),
            supporting: Some(b"3-32 characters"),
        },
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));
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
    text(ui, stack, ctx, TextVariant::Body, b"Multiline")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let mut area = TextArea::new(
        ui,
        stack,
        ctx,
        TextAreaSpec { width, lines: 3 },
    )
    .unwrap_or_else(|_| berry_room::panic_exit(134));
    let area_state = TextInputState::new(b"Berry notes\nline two")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    area.sync(ui, ctx, &area_state)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Body, b"Tap a field to focus its caret")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    InputPage {
        area,
        area_state,
        root: scaffold.root,
        scroll,
        tabs,
        back: app_back,
        fields,
        labelled,
        labelled_error: false,
        search,
        search_state,
    }
}

fn build_list(
    ui: &mut Ui192, ctx: &WidgetContext, selected: u8, empty: bool, safe_top: u32,
    safe_bottom: u32,
) -> ListPage {
    let (scaffold, scroll, tabs) = page_body(
        ui,
        ctx,
        PAGE_LIST,
        safe_top,
        safe_bottom,
    );
    let app_back = scaffold
        .app_bar
        .back
        .unwrap_or_else(|| berry_room::panic_exit(134));
    let card = Card::new(ui, scroll.content, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let stack = column(ui, card.panel, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));

    let segment_items = [
        SegmentedItemSpec { label: b"All", disabled: false },
        SegmentedItemSpec { label: b"Recent", disabled: false },
    ];
    let segments = SegmentedControl::new(ui, stack, ctx, &segment_items, 0)
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    if empty {
        EmptyState::new(ui, stack, ctx, 102, b"No items", b"Restore the demo list")
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let restore = Button::new(ui, stack, ctx, b"Restore list", InteractionState::Normal)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        return ListPage {
            root: scaffold.root,
            scroll,
            tabs,
            back: app_back,
            detail: None,
            items: [None; 4],
            segments,
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
    ListPage {
        root: scaffold.root,
        scroll,
        tabs,
        back: app_back,
        detail: Some(detail),
        items,
        segments,
        clear: Some(clear),
        restore: None,
    }
}

fn build_theme(
    ui: &mut Ui192, ctx: &WidgetContext, safe_top: u32, safe_bottom: u32,
) -> ThemePage {
    let (scaffold, scroll, tabs) = page_body(
        ui,
        ctx,
        PAGE_THEME,
        safe_top,
        safe_bottom,
    );
    let app_back = scaffold
        .app_bar
        .back
        .unwrap_or_else(|| berry_room::panic_exit(134));
    let info = IconButton::new(
        ui,
        scaffold.app_bar.actions,
        ctx,
        102,
        102,
        b"Theme info",
        InteractionState::Normal,
        false,
    )
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

    let picker_options: [&'static [u8]; 3] = [b"Compact", b"Medium", b"Comfortable"];
    let picker = TextPicker::new(ui, stack, ctx, &picker_options, 1)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let cycle = Button::new(ui, stack, ctx, b"Cycle scheme", InteractionState::Selected)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Body, b"Light / dark / high contrast")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    ThemePage { root: scaffold.root, scroll, tabs, back: app_back, info, cycle, picker }
}

fn build_system(
    ui: &mut Ui192, ctx: &WidgetContext, frame_seq: u32, safe_top: u32, safe_bottom: u32,
) -> SystemPage {
    let (scaffold, scroll, tabs) = page_body(
        ui,
        ctx,
        PAGE_SYSTEM,
        safe_top,
        safe_bottom,
    );
    let app_back = scaffold
        .app_bar
        .back
        .unwrap_or_else(|| berry_room::panic_exit(134));
    let card = Card::new(ui, scroll.content, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let stack = column(ui, card.panel, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));

    text(ui, stack, ctx, TextVariant::Title, b"System chain")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let system_lines: [&[u8]; 6] = [
        b"Desktop click issues real APP_SPAWN",
        b"BERRYDEMO is a packaged .bap process",
        b"Berry Room owns WM window and frame VMO",
        b"DisplayList submit is the only render path",
        b"Pointer events arrive owner-local",
        b"Destroy and exit clean the WM session",
    ];
    for line in system_lines {
        text(ui, stack, ctx, TextVariant::Body, line)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
    }

    divider(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    text(ui, stack, ctx, TextVariant::Title, b"App standard")
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let standard_lines: [&[u8]; 7] = [
        b"manifest.toml declares id, entry, wm",
        b"mkapp emits a checked .bap archive",
        b"BerryUI widgets own layout and state",
        b"IconSource supplies all demo pixels",
        b"Guest font VMO supplies HD text",
        b"No WM RPC, VMO mapping, or pixels",
        b"No surface escape in a normal app",
    ];
    for line in standard_lines {
        text(ui, stack, ctx, TextVariant::Body, line)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
    }

    let stepper = Stepper::new(ui, stack, ctx, 2, 0, 5)
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    let icons = row(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    icon(ui, icons, ctx, 100).unwrap_or_else(|_| berry_room::panic_exit(134));
    icon(ui, icons, ctx, 101).unwrap_or_else(|_| berry_room::panic_exit(134));
    icon(ui, icons, ctx, 102).unwrap_or_else(|_| berry_room::panic_exit(134));

    let mut frame_label = [b'0'; 48];
    let head = b"Current frame sequence ";
    frame_label[..head.len()].copy_from_slice(head);
    let n = append_u32(&mut frame_label[head.len()..], frame_seq) + head.len();
    text(ui, stack, ctx, TextVariant::Body, &frame_label[..n])
        .unwrap_or_else(|_| berry_room::panic_exit(134));

    SystemPage { root: scaffold.root, scroll, tabs, back: app_back, stepper }
}

fn build_page(
    ui: &mut Ui192, ctx: &WidgetContext, kind: u8, clicks: u32, switch_on: bool, selected: u8,
    empty: bool, frame_seq: u32, safe_top: u32, safe_bottom: u32,
) -> Page {
    match kind {
        PAGE_WIDGETS => Page::Widgets(build_widgets(
            ui, ctx, clicks, switch_on, safe_top, safe_bottom,
        )),
        PAGE_INPUT => Page::Input(build_input(ui, ctx, safe_top, safe_bottom)),
        PAGE_LIST => Page::List(build_list(
            ui, ctx, selected, empty, safe_top, safe_bottom,
        )),
        PAGE_THEME => Page::Theme(build_theme(ui, ctx, safe_top, safe_bottom)),
        PAGE_SYSTEM => Page::System(build_system(
            ui, ctx, frame_seq, safe_top, safe_bottom,
        )),
        _ => Page::Home(build_home(ui, ctx, safe_top, safe_bottom)),
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
        if let Page::Widgets(page) = &mut self.page {
            let _ = page
                .loading
                .set_phase(&mut self.ui, &self.ctx, self.frame_seq as u8);
        }
        let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 192];
        let mut encoder = CommandEncoder::new();
        self.ui.wake(0);
        let outcome = self.ui.render_frame(0, &mut scratch, &mut encoder);
        if !outcome.drew {
            fail(room, "no damage");
        }
        let list = encoder.finish();
        let commands = match room.render_display_list_damaged(&list, outcome.damage.envelope().as_ref()) {
            Ok(commands) => commands,
            Err(error) => fail(room, error.as_str()),
        };
        self.frame_seq = self.frame_seq.saturating_add(1);
        if let Err(error) = room.submit(self.frame_seq) {
            fail(room, error.as_str());
        }
        let mut line = Line::new(b"BERRYDEMORENDER ok seq=");
        line.push_u32(self.frame_seq);
        line.push(b" cmds=");
        line.push_u32(commands as u32);
        line.push(b"\n");
        line.print(room);
        room.print(b"BERRYDEMOFRAME ok\n");
    }

    fn page_tabs_badge(&self) -> u32 {
        match &self.page {
            Page::Home(page) => page.tabs.badge(2).unwrap_or(0),
            Page::Widgets(page) => page.tabs.badge(2).unwrap_or(0),
            Page::Input(page) => page.tabs.badge(2).unwrap_or(0),
            Page::List(page) => page.tabs.badge(2).unwrap_or(0),
            Page::Theme(page) => page.tabs.badge(2).unwrap_or(0),
            Page::System(page) => page.tabs.badge(2).unwrap_or(0),
        }
    }

    fn page_tabs_badge_panel(&self) -> Option<berry_ui::NodeId> {
        match &self.page {
            Page::Home(page) => page.tabs.badge_panel(2),
            Page::Widgets(page) => page.tabs.badge_panel(2),
            Page::Input(page) => page.tabs.badge_panel(2),
            Page::List(page) => page.tabs.badge_panel(2),
            Page::Theme(page) => page.tabs.badge_panel(2),
            Page::System(page) => page.tabs.badge_panel(2),
        }
    }

    fn print_layout(&self, room: &AppRoom) {
        let mut line = Line::new(b"BERRYDEMOLAYOUT ok page=");
        line.push_u32(self.page.kind() as u32);
        let (back, extra, system, tab, p1) = match &self.page {
            Page::Home(page) => (
                page.back.panel,
                page.nav[0].as_ref().map(|item| item.panel),
                Some(page.tour.panel),
                page.tabs.panel,
                None,
            ),
            Page::Widgets(page) => (
                page.back.panel,
                Some(page.switch.panel),
                Some(page.icon_strip.panel),
                page.tabs.panel,
                Some(page.filter.panel),
            ),
            Page::Input(page) => (
                page.back.panel,
                Some(page.fields[0].panel),
                Some(page.labelled.root),
                page.tabs.panel,
                Some(page.search.clear.panel),
            ),
            Page::List(page) => (
                page.back.panel,
                page.detail.as_ref().map(|detail| detail.row),
                None,
                page.tabs.panel,
                page.segments.item_panel(1).ok(),
            ),
            Page::Theme(page) => (
                page.back.panel,
                Some(page.cycle.panel),
                Some(page.info.panel),
                page.tabs.panel,
                Some(page.picker.next.panel),
            ),
            Page::System(page) => (
                page.back.panel,
                None,
                None,
                page.tabs.panel,
                Some(page.stepper.increment.panel),
            ),
        };
        let badge_count = match &self.page {
            Page::Widgets(page) => page.badge.count(),
            _ => 0,
        };
        line.push(b" back=");
        line.rect(self.ui.bounds(back).unwrap_or_else(|_| berry_room::panic_exit(134)));
        if let Some(node) = extra {
            line.push(b" target=");
            line.rect(self.ui.bounds(node).unwrap_or_else(|_| berry_room::panic_exit(134)));
        }
        if let Some(node) = system {
            line.push(b" system=");
            line.rect(self.ui.bounds(node).unwrap_or_else(|_| berry_room::panic_exit(134)));
        }
        line.push(b" tab=");
        line.rect(self.ui.bounds(tab).unwrap_or_else(|_| berry_room::panic_exit(134)));
        line.push(b" a11y=");
        line.push_u32(self.ui.a11y_iter().count() as u32);
        line.push(b" sem=");
        line.push_u32(self.ui.sem_iter().count() as u32);
        line.push(b" badge=");
        line.push_u32(badge_count);
        let tab_badge = self.page_tabs_badge();
        line.push(b" tabbadge=");
        line.push_u32(tab_badge);
        if let Some(node) = self.page_tabs_badge_panel() {
            line.push(b" tbadge=");
            line.rect(self.ui.bounds(node).unwrap_or_else(|_| berry_room::panic_exit(134)));
        }
        if let Some(node) = p1 {
            line.push(b" p1=");
            line.rect(self.ui.bounds(node).unwrap_or_else(|_| berry_room::panic_exit(134)));
        }
        if let Page::Input(page) = &self.page {
            line.push(b" ta=");
            line.rect(self.ui.bounds(page.area.panel).unwrap_or_else(|_| berry_room::panic_exit(134)));
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
            self.frame_seq,
            self.safe_top,
            self.safe_bottom,
        );
        self.page
            .scroll_mut()
            .sync_bounds(&mut self.ui)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        // Page replacement is a structural tree swap. Damage bookkeeping for
        // nodes with no old-page counterpart (for example the new AppBar back
        // glyph) must not leave stale top-bar pixels; repaint the page fully.
        self.ui.invalidate_full();
        self.render(room);
        self.print_layout(room);
        let mut line = Line::new(b"BERRYDEMOPAGE ok page=");
        line.push_u32(kind as u32);
        line.push(b"\n");
        line.print(room);
    }

    fn deliver_widget(&mut self, message: &Msg) -> bool {
        let mut handled = false;
        let tab_handled = match &mut self.page {
            Page::Home(page) => page
                .tabs
                .on_msg(&mut self.ui, &self.ctx, message)
                .unwrap_or_else(|_| berry_room::panic_exit(134)),
            Page::Widgets(page) => page
                .tabs
                .on_msg(&mut self.ui, &self.ctx, message)
                .unwrap_or_else(|_| berry_room::panic_exit(134)),
            Page::Input(page) => page
                .tabs
                .on_msg(&mut self.ui, &self.ctx, message)
                .unwrap_or_else(|_| berry_room::panic_exit(134)),
            Page::List(page) => page
                .tabs
                .on_msg(&mut self.ui, &self.ctx, message)
                .unwrap_or_else(|_| berry_room::panic_exit(134)),
            Page::Theme(page) => page
                .tabs
                .on_msg(&mut self.ui, &self.ctx, message)
                .unwrap_or_else(|_| berry_room::panic_exit(134)),
            Page::System(page) => page
                .tabs
                .on_msg(&mut self.ui, &self.ctx, message)
                .unwrap_or_else(|_| berry_room::panic_exit(134)),
        };
        handled |= tab_handled;
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
            Page::Widgets(page) => {
                let _ = page
                    .card
                    .on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
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
                handled |= page.checkbox.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.radio.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.filter.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                let _ = page
                    .slider
                    .on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.icon_strip.on_msg(&mut self.ui, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
            }
            Page::Input(page) => {
                handled |= page.back.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                for field in page.fields.iter_mut() {
                    handled |= field.on_msg(&mut self.ui, &self.ctx, message)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                }
                handled |= page.labelled.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.area.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.search.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
            }
            Page::List(page) => {
                handled |= page.back.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.segments.on_msg(&mut self.ui, &self.ctx, message)
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
                handled |= page.info.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.picker.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
            }
            Page::System(page) => {
                handled |= page.back.on_msg(&mut self.ui, &self.ctx, message)
                    .unwrap_or_else(|_| berry_room::panic_exit(134));
                handled |= page.stepper.on_msg(&mut self.ui, &self.ctx, message)
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
            // Press/Release/Focus/Drag 也必须进组件：顶栏返回键的按压态、
            // 焦点 ring 与 ScrollView 跟手滚动都是真实 UI 状态，不是 Click
            // 的附属品。无 damage 的消息零提交，保持停帧合同。
            self.deliver_widget(message);
            if self.ui.damage().union_px() != 0 {
                self.render(room);
            }
            return false;
        };
        let _ = self.deliver_widget(message);

        let mut hit_line = Line::new(b"BERRYDEMOHIT ok node=");
        let mut action: u8 = 0;
        let mut focused_field: u32 = 0;
        let mut target_page = self.page.kind();
        let tab_target = self.page.tabs_selected();
        if tab_target != target_page && tab_target <= PAGE_SYSTEM {
            hit_line.push(b"tab");
            target_page = tab_target;
            action = 2;
        }

        match &mut self.page {
            Page::Home(page) => {
                if page.back.panel == node || page.back.label == node {
                    hit_line.push(b"exit");
                    action = 1;
                } else if page.tour.panel == node || page.tour.label == node {
                    hit_line.push(b"system-tour");
                    target_page = PAGE_SYSTEM;
                    action = 9;
                } else if let Some(index) =
                    page.nav.iter().flatten().position(|item| item.owns(node))
                {
                    hit_line.push(b"nav");
                    target_page = (index as u8) + 1;
                    action = 2;
                }
            }
            Page::Widgets(page) => {
                if page.back.panel == node || page.back.icon == node {
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
                } else if page.radio.panel == node || page.radio.dot == node {
                    hit_line.push(b"radio");
                    page.radio
                        .set_checked(&mut self.ui, &self.ctx, true)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    action = 10;
                } else if page.filter.panel == node || page.filter.label == node {
                    hit_line.push(b"chip");
                    action = 14;
                }
            }
            Page::Input(page) => {
                if page.back.panel == node || page.back.icon == node {
                    hit_line.push(b"back");
                    target_page = PAGE_HOME;
                    action = 2;
                } else if page.labelled.owns(node) {
                    hit_line.push(b"labelled-field");
                    let was_error = page.labelled_error;
                    let mut labelled = page.labelled;
                    let messages = labelled.request_focus(&mut self.ui, &self.ctx)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    for focus in messages.iter() {
                        let _ = labelled.on_msg(&mut self.ui, &self.ctx, &focus);
                    }
                    let message = if was_error {
                        None
                    } else {
                        Some(FieldMessage {
                            kind: FieldMessageKind::Error,
                            text: b"Name required",
                        })
                    };
                    labelled
                        .set_message(&mut self.ui, &self.ctx, message)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    page.labelled = labelled;
                    page.labelled_error = !was_error;
                    focused_field = 3;
                    action = 21;
                } else if page.search.field.owns(node) {
                    hit_line.push(b"search-field");
                    let mut field = page.search.field;
                    let messages = field.request_focus(&mut self.ui, &self.ctx)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    page.search.field = field;
                    for focus in messages.iter() {
                        let _ = self.deliver_widget(&focus);
                    }
                    focused_field = u32::MAX;
                    action = 15;
                } else if node == page.search.clear.panel || node == page.search.clear.icon {
                    hit_line.push(b"search-clear");
                    page.search_state = TextInputState::new(b"")
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    page.search
                        .field
                        .sync(&mut self.ui, &self.ctx, &page.search_state)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    action = 16;
                } else if page.area.owns(node) {
                    hit_line.push(b"textarea");
                    let mut area = page.area;
                    let area_state = page.area_state;
                    let messages = area.request_focus(&mut self.ui, &self.ctx)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    page.area = area;
                    for focus in messages.iter() {
                        let _ = area.on_msg(&mut self.ui, &self.ctx, &focus);
                    }
                    area.sync(&mut self.ui, &self.ctx, &area_state)
                        .unwrap_or_else(|_| berry_room::panic_exit(134));
                    page.area = area;
                    focused_field = 4;
                    action = 22;
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
                if page.back.panel == node || page.back.icon == node {
                    hit_line.push(b"back");
                    target_page = PAGE_HOME;
                    action = 2;
                } else if page.segments.owns(node) {
                    hit_line.push(b"segment");
                    action = 17;
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
                if page.back.panel == node || page.back.icon == node {
                    hit_line.push(b"back");
                    target_page = PAGE_HOME;
                    action = 2;
                } else if node == page.info.panel || node == page.info.icon {
                    hit_line.push(b"info");
                    action = 20;
                } else if node == page.picker.previous.panel
                    || node == page.picker.previous.label
                    || node == page.picker.next.panel
                    || node == page.picker.next.label
                {
                    hit_line.push(b"picker");
                    action = 18;
                } else if page.cycle.panel == node || page.cycle.label == node {
                    hit_line.push(b"theme");
                    self.theme_index = (self.theme_index + 1) % 3;
                    action = 7;
                }
            }
            Page::System(page) => {
                if page.back.panel == node || page.back.icon == node {
                    hit_line.push(b"back");
                    target_page = PAGE_HOME;
                    action = 2;
                } else if node == page.stepper.decrement.panel
                    || node == page.stepper.decrement.label
                    || node == page.stepper.increment.panel
                    || node == page.stepper.increment.label
                {
                    hit_line.push(b"stepper");
                    action = 19;
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
                let mut line = Line::new(b"BERRYDEMOCOUNTER ok n=");
                line.push_u32(self.clicks);
                line.push(marker);
                line.print(room);
                self.render(room);
                false
            }
            4 => {
                let mut line = Line::new(b"BERRYDEMOSWITCH ok checked=");
                line.push_u32(self.switch_on as u32);
                line.push(b"\n");
                line.print(room);
                self.render(room);
                false
            }
            5 => {
                let mut line = Line::new(b"BERRYDEMOFOCUS ok field=");
                line.push_u32(focused_field);
                line.push(b"\n");
                line.print(room);
                self.render(room);
                false
            }
            6 => {
                let mut line = Line::new(b"BERRYDEMOLIST ok selected=");
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
                let mut line = Line::new(b"BERRYDEMOTHEME ok scheme=");
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
            9 => {
                room.print(b"BERRYDEMOSYSTEM ok\n");
                self.replace_page(room, PAGE_SYSTEM);
                false
            }
            _ => {
                self.render(room);
                false
            }
        }
    }
}

impl App {
    fn handle_key(&mut self, room: &mut AppRoom, event: &KeyEvent) {
        let (mut area, mut state) = match &self.page {
            Page::Input(page) => (page.area, page.area_state),
            _ => return,
        };
        if self.ui.focus_node() != Some(area.panel) {
            return;
        }
        let Ok(code) = u8::try_from(event.code) else {
            return;
        };
        let route = key_route(
            code,
            event.action,
            event.modifiers,
            TextInputKind::Multiline,
            true,
        );
        let KeyRoute::Text(TextInputKeyOp::Edit(edit)) = route else {
            // Tab still belongs to C1 focus navigation. This demo keeps the
            // TextArea as the only keyboard-editable node, so UP/None/Focus
            // are intentionally non-mutating here.
            return;
        };
        let outcome = state.apply_edit(edit);
        if outcome.rejected {
            return;
        }
        if outcome.changed || outcome.selection_changed {
            area.sync(&mut self.ui, &self.ctx, &state)
                .unwrap_or_else(|_| berry_room::panic_exit(134));
            if let Page::Input(page) = &mut self.page {
                page.area = area;
                page.area_state = state;
            }
            self.ta_edits = self.ta_edits.saturating_add(1);
            let (line, col) =
                berry_ui::text_input::line_col_of(state.value().as_bytes(), state.caret_cp());
            let mut marker = Line::new(b"BERRYDEMOTEXTEDIT ok n=");
            marker.push_u32(self.ta_edits);
            marker.push(b" lines=");
            marker.push_u32(u32::from(berry_ui::text_input::line_count(
                state.value().as_bytes(),
            )));
            marker.push(b" caret=");
            marker.push_u32(u32::from(line));
            marker.push(b",");
            marker.push_u32(u32::from(col));
            marker.push(b" bytes=");
            marker.push_u32(state.value().len as u32);
            marker.push(b"\n");
            marker.print(room);
            self.render(room);
        }
    }
}

fn run_interactive(app: &mut App, room: &mut AppRoom) {
    let mut events = [empty_event(); INPUT_EVENTS_MAX];
    let mut keys = [KeyEvent {
        timestamp_ns: 0,
        code: 0,
        action: 0,
        modifiers: 0,
        device_id: 0,
    }; INPUT_KEY_EVENTS_MAX];
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
                    room.print(b"BERRYDEMOBACK ok\n");
                    return;
                }
            }
        }
        let key_count = match room.poll_keys(&mut keys) {
            Ok(count) => count,
            Err(error) => fail(room, error.as_str()),
        };
        for event in keys.iter().take(key_count) {
            app.handle_key(room, event);
        }
    }
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env_ch: u32) -> ! {
    let config = RoomConfig {
        argv0: b"bin/berrydemo",
        uses_fs: false,
        surface_escape: false,
    };
    let mut room = match AppRoom::connect(env_ch, config) {
        Ok(room) => room,
        Err(_) => berry_room::panic_exit(134),
    };
    room.print(b"BERRYDEMO up\n");
    let (width, height) = match room.open_window() {
        Ok(size) => size,
        Err(error) => fail(&room, error.as_str()),
    };
    room.print(b"BERRYDEMOWIN ok\n");

    let mut ui = Ui192::new(width as u32, height as u32);
    let ctx = WidgetContext::for_ui(Theme::light(), &ui);
    ui.set_font_source(&ROOM_FONT_SOURCE);
    ui.set_icon_source(&DEMO_ICONS);
    let insets = room.occlusion().effective_padding();
    let safe_top = u32::from(insets[1]);
    let safe_bottom = u32::from(insets[3]).max(ctx.metrics.effective_px(144));
    // Retained remains the ordinary app surface; this only adds Keyboard to
    // the input contract. Safe insets are snapshotted first because a surface
    // grant replaces Room's cached occlusion with the granted values.
    let grant = match room.configure_surface(SurfaceRequest {
        mode: SurfaceModeKind::Retained,
        input_classes: SURFACE_INPUT_TOUCH | SURFACE_INPUT_KEYBOARD,
        ime_mode: SURFACE_IME_DISABLED,
        a11y_mode: SURFACE_A11Y_TREE,
    }) {
        Ok(value) => value,
        Err(error) => fail(&room, error.as_str()),
    };
    if grant.input_classes & SURFACE_INPUT_KEYBOARD == 0 {
        fail(&room, "keyboard grant");
    }
    room.print(b"BERRYDEMOSURFACE ok keys=1\n");
    let page = build_page(
        &mut ui,
        &ctx,
        PAGE_HOME,
        0,
        false,
        0,
        false,
        0,
        safe_top,
        safe_bottom,
    );
    let mut app = App {
        ui,
        ctx,
        page,
        theme_index: 0,
        clicks: 0,
        switch_on: false,
        selected_item: 0,
        list_empty: false,
        ta_edits: 0,
        frame_seq: 0,
        safe_top,
        safe_bottom,
    };
    app.page
        .scroll_mut()
        .sync_bounds(&mut app.ui)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    app.print_layout(&room);
    app.render(&mut room);

    if room.boot_probe() {
        room.print(b"BERRYDEMOPROBE ok\n");
    } else {
        run_interactive(&mut app, &mut room);
    }

    if let Err(error) = room.destroy() {
        fail(&room, error.as_str());
    }
    room.print(b"BERRYDEMODESTROY ok\n");
    room.print(b"BERRYDEMOEXIT ok\n");
    room.exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_room::panic_exit(134)
}
