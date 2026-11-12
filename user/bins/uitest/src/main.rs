//! UITEST —— GUI F5 的 BerryUI + Berry Room 系统验收 app。
//!
//! F1 零像素契约升级为真实 UI 消费者：本 app 只组合 BerryUI SDK 组件，
//! 不解析 WM RPC、不映射 VMO、不计算 stride、不写像素循环。Berry Room
//! 负责窗口/frame/输入/销毁与 DisplayList 光栅化；系统 HD A8 字形经
//! per-app VMO 图集供给，避免 app 复制静态字表。

#![no_std]
#![no_main]

use berry_gfx::cmd::{CommandEncoder, GlyphPos};
use berry_gfx::font::FontSize;
use berry_room::{
    AppRoom, INPUT_EVENTS_MAX, INPUT_SAMPLES_MAX, PHASE_UP, PointerEvent, RoomConfig, Sample,
    SURFACE_A11Y_HYBRID, SURFACE_A11Y_REGIONS,
    SURFACE_IME_DISABLED, SURFACE_INPUT_KEYBOARD, SURFACE_INPUT_MOUSE, SURFACE_INPUT_TOUCH,
    INPUT_KEY_EVENTS_MAX, KEY_ACTION_DOWN, KEY_ACTION_UP, KeyEvent, SurfaceModeKind,
    SurfaceRequest, SYSTEM_FONT_ATLAS,
};
use berry_ui::sdk::{
    BoxConstraints, ColorScheme, focus_key_intent, ime_overlap_px, key_route, slot_variant,
    FocusDir, FocusOp, KeyRoute,
    SlotVariant, Button, Card, EmptyState, FontSource, InteractionState, ListDetail,
    ListDetailSpec, ListItem, Msg, RectI, Scaffold, ScrollView, Switch, TextInputKind,
    a11y_region_roundtrip, A11yRegion, TextInputKeyOp, TextInputState, TextVariant,
    TextField, TextFieldSpec, Theme, Ui, WidgetContext, SEM_ACTION_TEXT_EDIT, column,
    divider, icon, row, text,
};
use berry_ui::hit::PtrEvent;
use berry_ui::ROLE_TOGGLE;

type Ui128 = Ui<128>;

/// F5 组件页句柄。业务状态仍由 `AppModel` 持有，这里只保存 NodeId。
struct Page {
    scaffold: Scaffold,
    scroll: ScrollView,
    clip_item: Option<ListItem>,
    button: Button,
    switch: Option<Switch>,
    adaptive: Option<ListDetail>,
    input: Option<TextField>,
    index: u8,
}

struct AppModel {
    ui: Ui128,
    ctx: WidgetContext,
    page: Page,
    page_index: u8,
    switch_on: bool,
    text_state: TextInputState,
    frame_seq: u32,
}

struct Source;
static SOURCE: Source = Source;

/// guest-font-vmo 档：S32 用 Berry Room 系统 A8 VMO；本页不发射其他
/// 字号档，静态 HD/CJK 表不进 app。
struct RoomFontSource;
static ROOM_FONT_SOURCE: RoomFontSource = RoomFontSource;
impl FontSource for RoomFontSource {
    fn atlas(&self, scale: FontSize) -> Option<&'static dyn berry_gfx::font::GlyphAtlas> {
        match scale {
            FontSize::S32x48 => Some(&SYSTEM_FONT_ATLAS),
            _ => None,
        }
    }
}
static ICON_PX: [u32; 4] =
    [0xFF_FF_00_00, 0xFF_00_FF_00, 0xFF_00_00_FF, 0xFF_FF_FF_00];

impl berry_ui::sdk::IconSource for Source {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        (id == 7).then_some((2, 2))
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        (id == 7).then_some(&ICON_PX)
    }
}

fn append_u32(out: &mut [u8], mut value: u32) -> usize {
    let mut digits = [0u8; 10];
    let mut count = 0usize;
    if value == 0 {
        digits[0] = b'0';
        count = 1;
    }
    while value != 0 && count < digits.len() {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
    }
    let mut written = 0usize;
    while count > 0 && written < out.len() {
        count -= 1;
        out[written] = digits[count];
        written += 1;
    }
    written
}

fn append_i32(out: &mut [u8], value: i32) -> usize {
    if value < 0 {
        if out.is_empty() {
            return 0;
        }
        out[0] = b'-';
        let unsigned = if value == i32::MIN {
            2_147_483_648u32
        } else {
            (-value) as u32
        };
        1 + append_u32(&mut out[1..], unsigned)
    } else {
        append_u32(out, value as u32)
    }
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
        let n = append_i32(&mut buf, value);
        self.push(&buf[..n]);
    }

    fn push_hex_byte(&mut self, value: u8) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let pair = [HEX[(value >> 4) as usize], HEX[(value & 0x0F) as usize]];
        self.push(&pair);
    }

    fn color(&mut self, value: berry_gfx::cmd::Color) {
        self.push_hex_byte(value.r);
        self.push_hex_byte(value.g);
        self.push_hex_byte(value.b);
    }

    fn rect(&mut self, r: RectI) {
        self.push_i32(r.x);
        self.push(b",");
        self.push_i32(r.y);
        self.push(b",");
        self.push_i32(r.w);
        self.push(b",");
        self.push_i32(r.h);
    }

    fn print(&self, room: &AppRoom) {
        room.print(&self.bytes[..self.len]);
    }
}

fn geometry_line(width: usize, height: usize) -> Line {
    let mut line = Line::new(b"UITESTWIN ok w=");
    line.push_u32(width as u32);
    line.push(b" h=");
    line.push_u32(height as u32);
    line.push(b"\n");
    line
}

fn input_line(x: u32, y: u32) -> Line {
    let mut line = Line::new(b"UITESTINPUT ok x=");
    line.push_u32(x);
    line.push(b" y=");
    line.push_u32(y);
    line.push(b"\n");
    line
}

fn fail(room: &AppRoom, what: &str) -> ! {
    let mut line = [0u8; 96];
    let head = b"uitest: ";
    line[..head.len()].copy_from_slice(head);
    let n = head.len() + what.len().min(line.len() - head.len());
    line[head.len()..n].copy_from_slice(&what.as_bytes()[..n - head.len()]);
    room.print(&line[..n]);
    room.exit(134)
}

fn build_page(
    ui: &mut Ui128,
    ctx: &WidgetContext,
    index: u8,
    switch_on: bool,
) -> Page {
    let root = ui.root();
    let scaffold = Scaffold::new(ui, root, ctx, if index == 0 {
        b"Components"
    } else {
        b"Details"
    })
    .unwrap_or_else(|_| berry_room::panic_exit(134));
    let (_, viewport_h) = ui.viewport();
    let title_h = ctx.metrics.typography(berry_ui::sdk::TypographyToken::Title).cell_h() as u32;
    let bar_h = title_h.saturating_add(ctx.metrics.spacing(berry_ui::sdk::SpacingToken::S).saturating_mul(2));
    let view_h = viewport_h.saturating_sub(bar_h);
    let scroll = ScrollView::new(ui, scaffold.body, ctx, view_h)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    let card = Card::new(ui, scroll.content, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
    let stack = column(ui, card.panel, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));

    if index == 0 {
        text(ui, stack, ctx, TextVariant::Title, b"BerryUI system test")
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let button = Button::new(ui, stack, ctx, b"Details", InteractionState::Normal)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        divider(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
        let switch = Switch::new(ui, stack, ctx, switch_on, false)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        ListItem::new(ui, stack, ctx, b"Owner-local input", Some(b"WM routed"), false)
            .unwrap_or_else(|_| berry_room::panic_exit(134));

        // 真实滚动/裁剪样本：单行 ListItem 比 viewport 高 48px，
        // offset 24 后仍有 24px 溢出 port，由 C1 Clip 真实裁剪。
        let body_h = ctx
            .metrics
            .typography(berry_ui::sdk::TypographyToken::Body)
            .cell_h() as u32;
        let list_pad = ctx.metrics.spacing(berry_ui::sdk::SpacingToken::M);
        let sample_h = body_h
            .saturating_add(list_pad.saturating_mul(2))
            .saturating_sub(48)
            .max(1);
        let scroll = ScrollView::new(ui, stack, ctx, sample_h)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let clip_item = ListItem::new(ui, scroll.content, ctx, b"Scroll sample", None, false)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let controls = row(ui, stack, ctx).unwrap_or_else(|_| berry_room::panic_exit(134));
        icon(ui, controls, ctx, 7).unwrap_or_else(|_| berry_room::panic_exit(134));
        EmptyState::new(
            ui, stack, ctx, 7, b"F5 ready", b"Frame, layout, hit and state",
        )
        .unwrap_or_else(|_| berry_room::panic_exit(134));
        // L3 ListDetail 最小生产样：basis 仍是 card 内容局部宽 1056，
        // 两个 Panel pane 以 runtime flex 1:2 分配 1044 主轴空间。
        let adaptive = ListDetail::new(
            ui,
            stack,
            ctx,
            ListDetailSpec {
                width: 1056,
                height: 120,
                gap: 12,
                list_weight: 1,
                detail_weight: 2,
            },
        )
        .unwrap_or_else(|_| berry_room::panic_exit(134));
        // GUI-T1：追加在 page0 最后，既有 Button/Switch C1 两 Tab 序与
        // ListDetail 像素锚不漂移；TextField 是树前序第 5 个 focusable。
        let input = TextField::new(
            ui,
            stack,
            ctx,
            TextFieldSpec { kind: TextInputKind::Text, width: 512 },
        )
        .unwrap_or_else(|_| berry_room::panic_exit(134));
        Page {
            scaffold,
            scroll,
            clip_item: Some(clip_item),
            button,
            switch: Some(switch),
            adaptive: Some(adaptive),
            input: Some(input),
            index,
        }
    } else {
        text(ui, stack, ctx, TextVariant::Title, b"Return stack page")
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        ListItem::new(ui, stack, ctx, b"Page 1", Some(b"Pushed by Details"), true)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let button = Button::new(ui, stack, ctx, b"Back", InteractionState::Normal)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        EmptyState::new(
            ui, stack, ctx, 7, b"Back ready", b"Click Back to pop page",
        )
        .unwrap_or_else(|_| berry_room::panic_exit(134));
        Page {
            scaffold,
            scroll,
            clip_item: None,
            button,
            switch: None,
            adaptive: None,
            input: None,
            index,
        }
    }
}

impl AppModel {
    fn render(&mut self, room: &mut AppRoom) -> u32 {
        let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 192];
        let mut encoder = CommandEncoder::new();
        self.ui.wake(0);
        let outcome = self.ui.render_frame(0, &mut scratch, &mut encoder);
        if !outcome.drew {
            fail(room, "no damage");
        }
        let dl = encoder.finish();
        let cmds = match room.render_display_list(&dl) {
            Ok(cmds) => cmds,
            Err(e) => fail(room, e.as_str()),
        };
        self.frame_seq = self.frame_seq.saturating_add(1);
        if let Err(e) = room.submit(self.frame_seq) {
            fail(room, e.as_str());
        }
        let mut line = Line::new(b"UITESTRENDER ok seq=");
        line.push_u32(self.frame_seq);
        line.push(b" cmds=");
        line.push_u32(cmds as u32);
        line.push(b"\n");
        line.print(room);
        self.frame_seq
    }

    /// C1 guest 自证：ScrollView port 与 Clip 链一致，滚出 item 不可命中，
    /// 并真实重绘一帧。打印 port/item/probe 几何供宿主反 mock 使用。
    fn assert_clip(&mut self, room: &mut AppRoom) {
        let Some(item) = self.page.clip_item else {
            fail(room, "clip item");
        };
        let port = self
            .ui
            .bounds(self.page.scroll.panel)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let effective = self
            .ui
            .clip_of(item.panel)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        if effective != port {
            fail(room, "clip bounds");
        }
        let item_bounds = self
            .ui
            .bounds(item.panel)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let x = item_bounds.x.saturating_add(item_bounds.w / 2).max(0) as u32;
        let y = port.y.saturating_add(port.h).saturating_add(8).max(0) as u32;
        if self.ui.node_at(x, y) == Some(item.panel) {
            fail(room, "clip hit");
        }
        let seq = self.render(room);
        let mut line = Line::new(b"UITESTCLIP ok port=");
        line.rect(port);
        line.push(b" item=");
        line.rect(item_bounds);
        line.push(b" probe=");
        line.push_u32(x);
        line.push(b",");
        line.push_u32(y);
        line.push(b" seq=");
        line.push_u32(seq);
        line.push(b"\n");
        line.print(room);
    }

    /// P1 Surface 真链：manifest 授权 → per-boot seed channel → Berry Room
    /// 0x67 → wmd 授予 → buffer 借写 → SUBMIT。Canvas→Immersive 只在
    /// 授权 session 内重协商，不降回 Retained。
    fn assert_surface(&mut self, room: &mut AppRoom) {
        // P1 授权前不得借出像素 buffer。
        if !matches!(
            room.surface_frame_mut(),
            Err(berry_room::RoomError::SurfaceDenied)
        ) {
            fail(room, "surface buffer before grant");
        }

        // P2 fail-closed 负例：Gamepad 仍无生产者；Keyboard 已由真实
        // virtio-keyboard → inputd → wmd 链承载。
        let unsupported = room.configure_surface(SurfaceRequest {
            mode: SurfaceModeKind::Canvas,
            input_classes: SURFACE_INPUT_TOUCH | (1 << 4),
            ime_mode: SURFACE_IME_DISABLED,
            a11y_mode: SURFACE_A11Y_HYBRID,
        });
        if !matches!(
            unsupported,
            Err(berry_room::RoomError::SurfaceUnsupported)
        ) {
            fail(room, "surface unsupported input");
        }

        let canvas = room
            .configure_surface(SurfaceRequest {
                mode: SurfaceModeKind::Canvas,
                input_classes: SURFACE_INPUT_TOUCH
                    | SURFACE_INPUT_MOUSE
                    | SURFACE_INPUT_KEYBOARD,
                ime_mode: SURFACE_IME_DISABLED,
                a11y_mode: SURFACE_A11Y_HYBRID,
            })
            .unwrap_or_else(|e| fail(room, e.as_str()));
        if canvas.mode != SurfaceModeKind::Canvas
            || canvas.contract_flags != 31
            || canvas.scale_permille != 1000
            || canvas.safe_left != 0
            || canvas.safe_top != 0
        {
            fail(room, "canvas grant");
        }
        let canvas_seq = self.render_surface_fill(room, 0x30);

        let immersive = room
            .configure_surface(SurfaceRequest {
                mode: SurfaceModeKind::Immersive,
                input_classes: SURFACE_INPUT_TOUCH | SURFACE_INPUT_KEYBOARD,
                ime_mode: SURFACE_IME_DISABLED,
                a11y_mode: SURFACE_A11Y_REGIONS,
            })
            .unwrap_or_else(|e| fail(room, e.as_str()));
        if immersive.mode != SurfaceModeKind::Immersive
            || immersive.contract_flags != 31
            || immersive.scale_permille != 1000
            || immersive.safe_right != 0
            || immersive.safe_bottom != 0
        {
            fail(room, "immersive grant");
        }
        let immersive_seq = self.render_surface_fill(room, 0x60);
        let mut line = Line::new(b"UITESTSURFACE ok modes=1,2 frames=");
        line.push_u32(canvas_seq);
        line.push(b",");
        line.push_u32(immersive_seq);
        line.push(b" flags=31 scale=1000 safe=0,0,0,0\n");
        line.print(room);
    }

    fn render_surface_fill(&mut self, room: &mut AppRoom, byte: u8) -> u32 {
        match room.surface_frame_mut() {
            Ok(frame) => frame.fill(byte),
            Err(e) => fail(room, e.as_str()),
        }
        self.frame_seq = self.frame_seq.saturating_add(1);
        if let Err(e) = room.submit(self.frame_seq) {
            fail(room, e.as_str());
        }
        self.frame_seq
    }

    fn replace_page(&mut self, room: &mut AppRoom, index: u8) {
        if self.ui.remove(self.page.scaffold.root).is_err() {
            fail(room, "page remove");
        }
        self.page = build_page(&mut self.ui, &self.ctx, index, self.switch_on);
        self.page_index = index;
        self.page.scroll.sync_bounds(&mut self.ui).unwrap_or_else(|_| berry_room::panic_exit(134));
        if let Some(input) = self.page.input.as_mut() {
            input
                .sync(&mut self.ui, &self.ctx, &self.text_state)
                .unwrap_or_else(|_| berry_room::panic_exit(134));
        }
        self.render(room);
    }

    fn print_layout(&self, room: &AppRoom) {
        let bar = self.ui.bounds(self.page.scaffold.app_bar.panel).unwrap_or_else(|_| berry_room::panic_exit(134));
        let body = self.ui.bounds(self.page.scaffold.body).unwrap_or_else(|_| berry_room::panic_exit(134));
        let button = self.ui.bounds(self.page.button.panel).unwrap_or_else(|_| berry_room::panic_exit(134));
        let mut line = Line::new(b"UITESTLAYOUT ok page=");
        line.push_u32(self.page.index as u32);
        line.push(b" appbar=");
        line.rect(bar);
        line.push(b" body=");
        line.rect(body);
        line.push(b" button=");
        line.rect(button);
        let scroll_rect = self
            .ui
            .bounds(self.page.scroll.panel)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        line.push(b" scroll=");
        line.rect(scroll_rect);
        line.push(b" clip=");
        line.rect(self.ui.clip_of(self.page.scroll.content).unwrap_or_else(|_| berry_room::panic_exit(134)));
        line.push(b"\n");
        line.print(room);
    }

    fn print_hit(&self, room: &AppRoom) {
        let button = self.ui.bounds(self.page.button.panel).unwrap_or_else(|_| berry_room::panic_exit(134));
        let x = button.x.saturating_add(button.w / 2).max(0) as u32;
        let y = button.y.saturating_add(button.h / 2).max(0) as u32;
        let node = self.ui.node_at(x, y);
        let valid = node == Some(self.page.button.panel) || node == Some(self.page.button.label);
        let mut line = Line::new(b"UITESTHIT ok node=");
        line.push(if valid { b"button" } else { b"other" });
        line.push(b" x=");
        line.push_u32(x);
        line.push(b" y=");
        line.push_u32(y);
        line.push(b"\n");
        if !valid {
            berry_room::panic_exit(134);
        }
        line.print(room);
    }

    fn print_adaptive(&self, room: &AppRoom) {
        let Some(layout) = self.page.adaptive else { return };
        let (row, list, detail) = (layout.row, layout.list, layout.detail);
        let row_bounds = self.ui.bounds(row).unwrap_or_else(|_| berry_room::panic_exit(134));
        let list_bounds = self.ui.bounds(list).unwrap_or_else(|_| berry_room::panic_exit(134));
        let detail_bounds = self.ui.bounds(detail).unwrap_or_else(|_| berry_room::panic_exit(134));
        let basis = row_bounds.w.max(0) as u32;
        if slot_variant(basis, 600) != SlotVariant::Expanded
            || basis != 1056
            || list_bounds.w != 348
            || detail_bounds.w != 696
            || detail_bounds.w != list_bounds.w * 2
            || list_bounds.h != 120
            || detail_bounds.h != 120
            || detail_bounds.x != list_bounds.x + list_bounds.w + 12
        {
            fail(room, "adaptive slot");
        }
        let mut line = Line::new(b"UITESTADAPT ok v=2 basis=");
        line.push_u32(basis);
        line.push(b" list=");
        line.rect(list_bounds);
        line.push(b" detail=");
        line.rect(detail_bounds);
        line.push(b"\n");
        line.print(room);

        let mut line = Line::new(b"UITESTLISTDETAIL ok weights=1,2 row=");
        line.rect(row_bounds);
        line.push(b" list=");
        line.rect(list_bounds);
        line.push(b" detail=");
        line.rect(detail_bounds);
        line.push(b"\n");
        line.print(room);
    }

    fn print_data(&mut self, room: &AppRoom) {
        let mut ast = [0u8; 4096];
        let len = match self.ui.encode_desc(&mut ast) {
            Ok(len) => len,
            Err(_) => fail(room, "ast"),
        };
        let a11y = self.ui.a11y_iter().count();
        let mut line = Line::new(b"UITESTAST ok bytes=");
        line.push_u32(len as u32);
        line.push(b" a11y=");
        line.push_u32(a11y as u32);
        line.push(b"\n");
        line.print(room);
    }

    fn print_token(&self, room: &AppRoom) {
        let colors = self.ctx.theme.colors;
        let mut line = Line::new(b"UITESTTOKEN ok scheme=");
        match self.ctx.theme.scheme {
            ColorScheme::Light => line.push(b"light"),
            ColorScheme::Dark => line.push(b"dark"),
            ColorScheme::HighContrastLight => line.push(b"high_contrast_light"),
        }
        line.push(b" n=12 background=");
        line.color(colors.background);
        line.push(b" surface=");
        line.color(colors.surface);
        line.push(b" surface_variant=");
        line.color(colors.surface_variant);
        line.push(b" outline=");
        line.color(colors.outline);
        line.push(b" primary=");
        line.color(colors.primary);
        line.push(b" on_primary=");
        line.color(colors.on_primary);
        line.push(b" text_primary=");
        line.color(colors.text_primary);
        line.push(b" text_secondary=");
        line.color(colors.text_secondary);
        line.push(b" success=");
        line.color(colors.success);
        line.push(b" warning=");
        line.color(colors.warning);
        line.push(b" danger=");
        line.color(colors.danger);
        line.push(b" selected=");
        line.color(self.ctx.theme.state(InteractionState::Selected).background);
        line.push(b"\n");
        line.print(room);
    }

    fn print_l4(&mut self, room: &AppRoom) {
        let root = self.ui.root();
        let (vp_w, vp_h) = self.ui.viewport();

        self.ui.set_viewport(vp_w, vp_h);
        let Some(tight_stat) = self.ui.layout_box(BoxConstraints::tight(800, 1200)).ok() else {
            fail(room, "L4 tight box");
        };
        let Some(tight) = self.ui.bounds(root).ok() else {
            fail(room, "L4 tight bounds");
        };

        self.ui.set_viewport(vp_w, vp_h);
        let Some(loose_stat) = self.ui.layout_box(BoxConstraints::loose(700, 900)).ok() else {
            fail(room, "L4 loose box");
        };
        let Some(loose) = self.ui.bounds(root).ok() else {
            fail(room, "L4 loose bounds");
        };

        self.ui.set_viewport(vp_w, vp_h);
        let Some(infinite_stat) = self.ui.layout_box(BoxConstraints::infinite()).ok() else {
            fail(room, "L4 infinite box");
        };
        let Some(infinite) = self.ui.bounds(root).ok() else {
            fail(room, "L4 infinite bounds");
        };

        let Some(noop) = self.ui.layout_box(BoxConstraints::infinite()).ok() else {
            fail(room, "L4 noop box");
        };
        self.ui.set_viewport(vp_w, vp_h);
        let _restored = self.ui.layout();

        let mut line = Line::new(b"UITESTL4 ok tight=");
        line.rect(tight);
        line.push(b" loose=");
        line.rect(loose);
        line.push(b" inf=");
        line.rect(infinite);
        line.push(b" single=");
        line.push_u32(tight_stat.measured);
        line.push(b",");
        line.push_u32(loose_stat.measured);
        line.push(b",");
        line.push_u32(infinite_stat.measured);
        line.push(b" noop=");
        line.push_u32(noop.measured);
        line.push(b",");
        line.push_u32(noop.placed);
        line.push(b"\n");
        if noop.measured != 0 || noop.placed != 0 {
            fail(room, "L4 noop boundary");
        }
        if tight_stat.measured != loose_stat.measured
            || loose_stat.measured != infinite_stat.measured
        {
            fail(room, "L4 single measure boundary");
        }
        line.print(room);
    }

    fn print_sem(&mut self, room: &AppRoom) {
        let mut regions = [zero_region(); 32];
        let count = match self.ui.sem_snapshot_into(&mut regions) {
            Ok(count) => count,
            Err(_) => fail(room, "sem snapshot"),
        };
        if count != 9 {
            fail(room, "sem count");
        }
        let button = self.region_for(&regions, count, self.page.button.panel)
            .unwrap_or_else(|| fail(room, "sem button"));
        let switch_node = self.page.switch.map(|widget| widget.panel);
        let switch = switch_node
            .and_then(|node| self.region_for(&regions, count, node))
            .unwrap_or_else(|| fail(room, "sem switch"));
        let item_node = self.page.clip_item.map(|widget| widget.panel);
        let item = item_node
            .and_then(|node| self.region_for(&regions, count, node))
            .unwrap_or_else(|| fail(room, "sem item"));
        let input_node = self.page.input.map(|widget| widget.panel);
        let input = input_node
            .and_then(|node| self.region_for(&regions, count, node))
            .unwrap_or_else(|| fail(room, "sem input"));
        if button.role != 1 || button.actions != 3 || button.state != 1 {
            fail(room, "sem button tuple");
        }
        if switch.role != 4 || switch.actions != 3 || switch.state != 1 {
            fail(room, "sem switch tuple");
        }
        if item.role != 5 || item.actions != 3 || item.state != 1 {
            fail(room, "sem item tuple");
        }
        if input.role != 2 || input.actions != 9 || input.state != 1 {
            fail(room, "sem input tuple");
        }
        if regions.iter().take(count).any(|region| !a11y_region_roundtrip(region)) {
            fail(room, "sem codec");
        }
        let line = Line::new(b"UITESTSEM ok n=9 codec=9 btn=1.3.1 sw=4.3.1 li=5.3.1 tf=2.9.1\n");
        line.print(room);
    }

    fn print_sem_state(&mut self, room: &AppRoom) {
        let mut regions = [zero_region(); 32];
        let count = match self.ui.sem_snapshot_into(&mut regions) {
            Ok(count) => count,
            Err(_) => fail(room, "sem state snapshot"),
        };
        if count != 9 {
            fail(room, "sem state count");
        }
        let switch_node = self.page.switch.map(|widget| widget.panel);
        let switch = switch_node
            .and_then(|node| self.region_for(&regions, count, node))
            .unwrap_or_else(|| fail(room, "sem state switch"));
        let item_node = self.page.clip_item.map(|widget| widget.panel);
        let clip = item_node
            .and_then(|node| self.region_for(&regions, count, node))
            .unwrap_or_else(|| fail(room, "sem state clip"));
        if switch.role != 4 || switch.actions != 3 || switch.state != 9 {
            fail(room, "sem checked tuple");
        }
        let switch_node_value = switch_node
            .and_then(|node| self.ui.sem_of(node))
            .map(|semantic| semantic.value);
        if switch_node_value.as_ref().map(|value| value.as_bytes()) != Some(&b"on"[..]) {
            fail(room, "sem checked value");
        }
        let mut line = Line::new(b"UITESTSEMSTATE ok n=9 sw=4.3.9.on clip=");
        line.push_u32(clip.x as u32);
        line.push(b",");
        line.push_u32(clip.y as u32);
        line.push(b",");
        line.push_u32(clip.w as u32);
        line.push(b",");
        line.push_u32(clip.h as u32);
        line.push(b"\n");
        line.print(room);
    }

    fn print_sem_page(&mut self, room: &AppRoom) {
        let mut regions = [zero_region(); 32];
        let count = match self.ui.sem_snapshot_into(&mut regions) {
            Ok(count) => count,
            Err(_) => fail(room, "sem page snapshot"),
        };
        if count != 6 {
            fail(room, "sem page count");
        }
        let button = self.region_for(&regions, count, self.page.button.panel)
            .unwrap_or_else(|| fail(room, "sem page button"));
        if button.role != 1 || button.actions != 3 || button.state != 1 {
            fail(room, "sem page button");
        }
        if regions.iter().take(count).any(|region| region.role == ROLE_TOGGLE) {
            fail(room, "sem page stale switch");
        }
        if regions.iter().take(count).any(|region| region.actions & SEM_ACTION_TEXT_EDIT != 0) {
            fail(room, "sem page stale input");
        }
        room.print(b"UITESTSEMPAGE ok n=6 btn=1.3.1 sw=0 tf=0\n");
    }

    fn print_sem_text(&mut self, room: &AppRoom) {
        let Some(input) = self.page.input else {
            fail(room, "sem text field");
        };
        let Some(semantic) = self.ui.sem_of(input.panel) else {
            fail(room, "sem text entry");
        };
        let mut regions = [zero_region(); 32];
        let count = match self.ui.sem_snapshot_into(&mut regions) {
            Ok(count) => count,
            Err(_) => fail(room, "sem text snapshot"),
        };
        let region = self
            .region_for(&regions, count, input.panel)
            .unwrap_or_else(|| fail(room, "sem text region"));
        if semantic.actions != 8 || region.actions != 9 || region.role != 2 || region.state != 3 {
            fail(room, "sem text tuple");
        }
        if semantic.value.len != 2 || semantic.text_range.anchor_cp != 2 || semantic.text_range.caret_cp != 2 {
            fail(room, "sem text sync");
        }
        let mut line = Line::new(b"UITESTSEMTEXT ok tf=2.9.3 len=");
        line.push_u32(semantic.value.len as u32);
        line.push(b" anchor=");
        line.push_u32(semantic.text_range.anchor_cp as u32);
        line.push(b" caret=");
        line.push_u32(semantic.text_range.caret_cp as u32);
        line.push(b"\n");
        line.print(room);
    }

    fn region_for<'a>(
        &self, regions: &'a [A11yRegion], count: usize, node: berry_ui::NodeId,
    ) -> Option<&'a A11yRegion> {
        let snapshot_id = self.ui.semantic_snapshot_id(node)?;
        regions.iter().take(count).find(|region| region.node_id == snapshot_id)
    }
}

fn zero_region() -> A11yRegion {
    A11yRegion {
        node_id: 0,
        parent_id: 0,
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        role: 0,
        actions: 0,
        state: 0,
        label_len: 0,
        label: [0; 32],
    }
}

fn ui_event(event: &PointerEvent) -> PtrEvent {
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

fn handle_messages(app: &mut AppModel, room: &mut AppRoom, messages: &berry_ui::sdk::MsgBuf) -> u8 {
    let mut target = app.page_index;
    let mut copied = [None; berry_ui::MSG_MAX];
    for (i, message) in messages.iter().enumerate() {
        if i < copied.len() {
            copied[i] = Some(*message);
        }
    }
    for slot in copied.iter().take(messages.len()) {
        let Some(message) = *slot else { continue };
        target = handle_message_one(app, room, &message);
    }
    target
}

/// 单 Msg 消费口（focus activation 合成 Click 时无需构造 MsgBuf）。
fn handle_message_one(app: &mut AppModel, room: &mut AppRoom, message: &Msg) -> u8 {
    let mut target = app.page_index;
    let clicked = app
        .page
        .button
        .on_msg(&mut app.ui, &app.ctx, message)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    if clicked {
        target = if app.page_index == 0 { 1 } else { 0 };
    }
    if let Some(switch) = app.page.switch.as_mut() {
        if switch
            .on_msg(&mut app.ui, &app.ctx, message)
            .unwrap_or_else(|_| berry_room::panic_exit(134))
        {
            app.switch_on = switch.is_checked();
        }
    }
    if let Some(item) = app.page.clip_item.as_mut() {
        item.on_msg(&mut app.ui, &app.ctx, message)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
    }
    if let Some(input) = app.page.input.as_mut() {
        input
            .on_msg(&mut app.ui, &app.ctx, message)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
    }
    app.page.scroll.on_msg(&mut app.ui, message)
        .unwrap_or_else(|_| berry_room::panic_exit(134));
    if target != app.page_index {
        app.replace_page(room, target);
    }
    target
}

fn wait_two_clicks(app: &mut AppModel, room: &mut AppRoom) -> (u32, u32) {
    let mut events = [PointerEvent {
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
        history: [Sample {
            timestamp_ns: 0,
            x: 0,
            y: 0,
        }; INPUT_SAMPLES_MAX - 1],
        history_len: 0,
    }; INPUT_EVENTS_MAX];
    let mut clicks = 0usize;
    let mut last = (0u32, 0u32);
    while clicks < 2 {
        let count = match room.poll_input(&mut events) {
            Ok(count) => count,
            Err(e) => fail(room, e.as_str()),
        };
        for event in events.iter_mut().take(count) {
            last = (event.x, event.y);
            if app.ui.push_pointer(ui_event(event)).is_err() {
                fail(room, "input buffer");
            }
            let messages = app.ui.pump_input(0);
            handle_messages(app, room, &messages);
            if event.phase == PHASE_UP {
                clicks += 1;
                if clicks == 1 {
                    app.print_layout(room);
                    app.print_hit(room);
                    let mut line = Line::new(b"UITESTSTATE ok page=");
                    line.push_u32(app.page_index as u32);
                    line.push(b" switch=");
                    line.push_u32(app.switch_on as u32);
                    line.push(b"\n");
                    line.print(room);
                }
            }
        }
    }
    last
}

/// P2/C1 runtime 真键盘链：surface 协商 Keyboard 后，QMP 注入两对 Tab
///（component focus 从 Button 前进到 Switch）与两对 Enter（激活 Switch
/// 两次，最终状态回 false）。窗口焦点仍由 wmd 持有；本函数只消费本窗
/// 已路由键并移动 BerryUI 树内焦点。
fn wait_real_keyboard(app: &mut AppModel, room: &mut AppRoom) {
    let grant = room
        .configure_surface(SurfaceRequest {
            mode: SurfaceModeKind::Canvas,
            input_classes: SURFACE_INPUT_TOUCH | SURFACE_INPUT_KEYBOARD,
            ime_mode: SURFACE_IME_DISABLED,
            a11y_mode: SURFACE_A11Y_HYBRID,
        })
        .unwrap_or_else(|e| fail(room, e.as_str()));
    if grant.input_classes & SURFACE_INPUT_KEYBOARD == 0 || grant.contract_flags != 31 {
        fail(room, "keyboard grant");
    }
    room.print(b"UITESTKEYWAIT ok input=9\n");

    let mut events = [KeyEvent {
        timestamp_ns: 0,
        code: 0,
        action: 0,
        modifiers: 0,
        device_id: 0,
    }; INPUT_KEY_EVENTS_MAX];
    let mut tab_downs = 0u32;
    let mut tab_ups = 0u32;
    let mut enter_downs = 0u32;
    let mut enter_ups = 0u32;
    let mut moves = 0u32;
    let initial_switch = app.switch_on;
    let mut switch_states = [false; 4];
    let mut activations = 0usize;
    while tab_downs < 2 || tab_ups < 2 || enter_downs < 2 || enter_ups < 2 {
        let count = match room.poll_keys(&mut events) {
            Ok(count) => count,
            Err(e) => fail(room, e.as_str()),
        };
        for event in events.iter().take(count) {
            if event.code != 15 && event.code != 28 {
                fail(room, "keyboard code");
            }
            let Some(op) = focus_key_intent(u8::try_from(event.code).unwrap_or(0xFF), event.action, event.modifiers) else {
                if event.code == 15 && event.action == KEY_ACTION_UP {
                    tab_ups += 1;
                } else if event.code == 28 && event.action == KEY_ACTION_UP {
                    enter_ups += 1;
                }
                continue;
            };
            match op {
                FocusOp::Next => {
                    let before = app.ui.focus_node().map(|_| focus_index(app)).unwrap_or(255);
                    let messages = app.ui.focus_move(FocusDir::Next);
                    handle_messages(app, room, &messages);
                    let after = focus_index(app);
                    moves += 1;
                    let mut line = Line::new(b"UITESTFOCUSMOVE ok step=");
                    line.push_u32(moves);
                    line.push(b" from=");
                    line.push_u32(before);
                    line.push(b" to=");
                    line.push_u32(after);
                    line.push(b" state=");
                    line.push_u32(app.ui.focus_node().and_then(|n| app.ui.a11y_state(n)).unwrap_or(1).into());
                    line.push(b"\n");
                    line.print(room);
                    if event.action == KEY_ACTION_DOWN {
                        tab_downs += 1;
                    }
                }
                FocusOp::Prev => fail(room, "focus prev in runtime"),
                FocusOp::Activate => {
                    let Some(node) = app.ui.focus_node() else {
                        fail(room, "focus activate absent");
                    };
                    let message = Msg::Click { node };
                    handle_message_one(app, room, &message);
                    if event.action == KEY_ACTION_DOWN {
                        enter_downs += 1;
                        if activations < switch_states.len() {
                            switch_states[activations] = app.switch_on;
                        }
                        activations += 1;
                    }
                }
            }
        }
    }
    app.render(room);
    let mut line = Line::new(b"UITESTFOCUSACT ok node=");
    line.push_u32(focus_index(app));
    line.push(b" toggles=");
    line.push_u32(activations as u32);
    line.push(b" states=");
    line.push_u32(initial_switch as u32);
    for state in switch_states.iter().take(activations.min(switch_states.len())) {
        line.push(b",");
        line.push_u32(*state as u32);
    }
    line.push(b" final=");
    line.push_u32(app.switch_on as u32);
    line.push(b"\n");
    line.print(room);
    let mut line = Line::new(b"UITESTKEY ok code=28 tab=2 downs=");
    line.push_u32(enter_downs);
    line.push(b" ups=");
    line.push_u32(enter_ups);
    line.push(b"\n");
    line.print(room);
    run_text_keyboard_phase(app, room);
}

/// GUI-T1 真键链：在既有 Button -> Switch C1 证明后，继续 Tab 到
/// TextField（树前序第 5 个 focusable），再用硬件键编辑并提交。IME 恒
/// Disabled；Enter/Space 由文本状态机消费，不合成组件 Click。
fn run_text_keyboard_phase(app: &mut AppModel, room: &mut AppRoom) {
    const CODES: [u16; 7] = [14, 15, 28, 30, 57, 105, 106];
    const PAIRS: [u32; 7] = [1, 3, 1, 2, 1, 1, 1];
    const SORTED_CODES: [u16; 7] = [14, 15, 28, 30, 57, 105, 106];
    let mut events = [KeyEvent {
        timestamp_ns: 0,
        code: 0,
        action: 0,
        modifiers: 0,
        device_id: 0,
    }; INPUT_KEY_EVENTS_MAX];
    let mut down_counts = [0u32; 128];
    let mut up_counts = [0u32; 128];
    let mut edits = 0u32;
    let mut submits = 0u32;
    let mut focus_marker = false;
    let toggles_before = app.switch_on;

    fn complete(codes: &[u16; 7], pairs: &[u32; 7], down: &[u32; 128], up: &[u32; 128]) -> bool {
        codes.iter().enumerate().all(|(i, code)| {
            let index = *code as usize;
            down[index] >= pairs[i] && up[index] >= pairs[i]
        })
    }
    fn count_pair(code: u16, action: u8, down: &mut [u32; 128], up: &mut [u32; 128]) {
        let index = code as usize;
        if index >= 128 {
            return;
        }
        if action == KEY_ACTION_DOWN {
            down[index] += 1;
        } else if action == KEY_ACTION_UP {
            up[index] += 1;
        }
    }

    while !complete(&CODES, &PAIRS, &down_counts, &up_counts) {
        let count = match room.poll_keys(&mut events) {
            Ok(count) => count,
            Err(e) => fail(room, e.as_str()),
        };
        for event in events.iter().take(count) {
            if !CODES.contains(&event.code) {
                fail(room, "text keyboard code");
            }
            count_pair(event.code, event.action, &mut down_counts, &mut up_counts);
            let code = u8::try_from(event.code).unwrap_or(0xFF);
            let field_focused = app
                .page
                .input
                .as_ref()
                .map(|field| app.ui.focus_node() == Some(field.panel))
                .unwrap_or(false);
            let route = key_route(
                code,
                event.action,
                event.modifiers,
                TextInputKind::Text,
                field_focused,
            );
            match route {
                KeyRoute::None => {}
                KeyRoute::Focus(FocusOp::Next) => {
                    let messages = app.ui.focus_move(FocusDir::Next);
                    handle_messages(app, room, &messages);
                }
                KeyRoute::Focus(FocusOp::Prev) => fail(room, "text focus prev"),
                KeyRoute::Focus(FocusOp::Activate) => {
                    fail(room, "text activate must not synthesize click")
                }
                KeyRoute::Text(TextInputKeyOp::FocusNext | TextInputKeyOp::FocusPrev) => {
                    fail(room, "text route focus mismatch")
                }
                KeyRoute::Text(TextInputKeyOp::Submit) => {
                    let outcome = app.text_state.submit();
                    if !outcome.submitted {
                        fail(room, "text submit");
                    }
                    submits += 1;
                }
                KeyRoute::Text(TextInputKeyOp::Edit(edit)) => {
                    let outcome = app.text_state.apply_edit(edit);
                    if outcome.rejected {
                        fail(room, "text edit rejected");
                    }
                    if let Some(input) = app.page.input.as_mut() {
                        input
                            .sync(&mut app.ui, &app.ctx, &app.text_state)
                            .unwrap_or_else(|_| berry_room::panic_exit(134));
                    }
                    edits += 1;
                }
            }

            let field_focused = app
                .page
                .input
                .as_ref()
                .map(|field| app.ui.focus_node() == Some(field.panel))
                .unwrap_or(false);
            if field_focused && !focus_marker {
                let index = focus_index(app);
                if index != 4 {
                    fail(room, "text focus index");
                }
                let mut line = Line::new(b"UITESTTEXTFOCUS ok node=");
                line.push_u32(index);
                line.push(b"\n");
                line.print(room);
                focus_marker = true;
            }
        }
    }

    if !focus_marker || submits != 1 || edits != 6 {
        fail(room, "text phase counters");
    }
    if app.text_state.value().as_bytes() != b"a " || app.text_state.caret_cp() != 2 {
        fail(room, "text state");
    }
    let selection = app.text_state.selection();
    if selection.anchor_cp != 2 || selection.caret_cp != 2 {
        fail(room, "text selection");
    }
    if app.switch_on != toggles_before {
        fail(room, "text enter toggled switch");
    }

    let (bottom, overlap) = {
        let Some(field) = app.page.input.as_ref() else {
            fail(room, "text field absent");
        };
        let bounds = app
            .ui
            .bounds(field.panel)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        let viewport_h = app.ui.viewport().1 as i32;
        let inset = room.occlusion().view_insets[3] as i32;
        (bounds.y.saturating_add(bounds.h), ime_overlap_px(bounds.y.saturating_add(bounds.h), viewport_h, inset))
    };

    let mut line = Line::new(b"UITESTTEXTEDIT ok edits=");
    line.push_u32(edits);
    line.push(b" bytes=");
    line.push_u32(app.text_state.value().len as u32);
    line.push(b" caret=");
    line.push_u32(app.text_state.caret_cp() as u32);
    line.push(b"\n");
    line.print(room);
    let mut line = Line::new(b"UITESTTEXTSELECT ok anchor=");
    line.push_u32(selection.anchor_cp as u32);
    line.push(b" caret=");
    line.push_u32(selection.caret_cp as u32);
    line.push(b"\n");
    line.print(room);
    app.print_sem_text(room);
    let mut line = Line::new(b"UITESTTEXTSUBMIT ok len=");
    line.push_u32(app.text_state.value().len as u32);
    line.push(b" toggles=0\n");
    line.print(room);
    let mut line = Line::new(b"UITESTIMEINSET ok bottom=");
    line.push_i32(bottom);
    line.push(b" overlap=");
    line.push_u32(overlap);
    line.push(b"\n");
    line.print(room);
    let mut line = Line::new(b"UITESTKEY2 ok codes=");
    for (i, code) in SORTED_CODES.iter().enumerate() {
        if i != 0 {
            line.push(b",");
        }
        line.push_u32(*code as u32);
    }
    line.push(b"\n");
    line.print(room);
    app.render(room);
}

/// 树前序 focus index（0 起；无焦点 = 0 语义仅用于标记，不影响状态）。
fn focus_index(app: &AppModel) -> u32 {
    let seed = app.page.button.panel;
    let mut order = [seed; 128];
    let n = app.ui.focus_order_into(&mut order);
    let Some(node) = app.ui.focus_node() else { return 0 };
    (0..n)
        .find(|i| order[*i] == node)
        .map(|i| i as u32)
        .unwrap_or(0)
}


#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env_ch: u32) -> ! {
    let config = RoomConfig {
        argv0: b"bin/uitest",
        uses_fs: false,
        surface_escape: true,
    };
    let mut room = match AppRoom::connect(env_ch, config) {
        Ok(room) => room,
        Err(_) => berry_room::panic_exit(134),
    };
    room.print(b"UITEST up\n");
    let (width, height) = match room.open_window() {
        Ok(size) => size,
        Err(e) => fail(&room, e.as_str()),
    };
    room.print(b"UITESTROOM ok\n");
    geometry_line(width, height).print(&room);

    let theme = Theme::light();
    let ctx = WidgetContext::for_viewport(theme, width as u32, height as u32);
    let mut ui = Ui128::new(width as u32, height as u32);
    ui.set_icon_source(&SOURCE);
    ui.set_font_source(&ROOM_FONT_SOURCE);
    let text_state = TextInputState::new(b"").unwrap_or_else(|_| berry_room::panic_exit(134));
    let mut page = build_page(&mut ui, &ctx, 0, false);
    let stat = ui.layout();
    if stat.placed == 0 {
        fail(&room, "initial layout");
    }
    page.scroll.sync_bounds(&mut ui).unwrap_or_else(|_| berry_room::panic_exit(134));
    // Runtime 首帧即保留 24px 溢出样本：GUI8 截图与 boot-probe 使用同一
    // Clip 几何，不靠“刚好放得下”的样本假绿。
    page.scroll.scroll_by(&mut ui, 24).unwrap_or_else(|_| berry_room::panic_exit(134));
    if let Some(input) = page.input.as_mut() {
        input.sync(&mut ui, &ctx, &text_state)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
    }
    let mut app = AppModel {
        ui,
        ctx,
        page,
        page_index: 0,
        switch_on: false,
        text_state,
        frame_seq: 0,
    };

    app.print_layout(&room);
    app.print_hit(&room);
    app.print_data(&mut room);
    app.print_token(&mut room);
    app.print_l4(&mut room);
    app.print_sem(&mut room);
    app.print_adaptive(&room);
    app.render(&mut room);
    room.print(b"UITESTFRAME ok\n");

    if room.boot_probe() {
        // 组件/状态/滚动/返回栈的 headless 全链：真实树、真实 damage、
        // 真实 DisplayList、真实 Berry Room frame submit。
        if let Some(switch) = app.page.switch.as_mut() {
            switch.set_checked(&mut app.ui, &app.ctx, true).unwrap_or_else(|_| berry_room::panic_exit(134));
            app.switch_on = true;
        }
        let scroll_offset = app.page.scroll.offset();
        if scroll_offset != 24 {
            fail(&mut room, "scroll offset");
        }
        app.render(&mut room);
        let mut scrolled = Line::new(b"UITESTSCROLL ok offset=");
        scrolled.push_i32(scroll_offset);
        scrolled.push(b" switch=1\n");
        scrolled.print(&room);
        app.print_sem_state(&mut room);
        app.assert_clip(&mut room);
        app.page.button
            .on_msg(
                &mut app.ui,
                &app.ctx,
                &Msg::Click { node: app.page.button.label },
            )
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        app.replace_page(&mut room, 1);
        app.print_sem_page(&mut room);
        let state = Line::new(b"UITESTSTATE ok page=1 switch=1\n");
        state.print(&room);
        app.page.button
            .on_msg(
                &mut app.ui,
                &app.ctx,
                &Msg::Click { node: app.page.button.label },
            )
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        app.replace_page(&mut room, 0);
        room.print(b"UITESTBACK ok page=0 switch=1\n");
        // Runtime 启动态重置为 page0/switch false，供 GUI 冒烟注入输入。
        app.switch_on = false;
        app.replace_page(&mut room, 0);
        app.assert_clip(&mut room);
        app.assert_surface(&mut room);
        if slot_variant(320, 600) != SlotVariant::Compact {
            berry_room::panic_exit(134);
        }
        room.print(b"UITESTADAPT2 ok v=0 basis=320\n");
        room.print(b"UITESTPROBE ok\n");
    } else {
        let (x, y) = wait_two_clicks(&mut app, &mut room);
        wait_real_keyboard(&mut app, &mut room);
        input_line(x, y).print(&room);
        let mut back = Line::new(b"UITESTBACK ok page=");
        back.push_u32(app.page_index as u32);
        back.push(b" switch=");
        back.push_u32(app.switch_on as u32);
        back.push(b"\n");
        back.print(&room);
    }

    if room.destroy().is_err() {
        fail(&room, "destroy");
    }
    if !matches!(
        room.surface_frame_mut(),
        Err(berry_room::RoomError::SurfaceDenied)
    ) {
        fail(&room, "surface buffer after destroy");
    }
    room.print(b"UITESTDESTROY ok\n");
    room.print(b"UITESTEXIT ok\n");
    room.exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_room::panic_exit(134)
}
