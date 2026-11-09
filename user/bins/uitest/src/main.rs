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
    Button, Card, EmptyState, FontSource, InteractionState, ListItem, Msg, RectI, Scaffold,
    ScrollView, Switch, TextVariant, Theme, Ui, WidgetContext, column, divider, icon, row,
    text,
};
use berry_ui::hit::PtrEvent;

type Ui128 = Ui<128>;

/// F5 组件页句柄。业务状态仍由 `AppModel` 持有，这里只保存 NodeId。
struct Page {
    scaffold: Scaffold,
    scroll: ScrollView,
    clip_item: Option<ListItem>,
    button: Button,
    switch: Option<Switch>,
    index: u8,
}

struct AppModel {
    ui: Ui128,
    ctx: WidgetContext,
    page: Page,
    page_index: u8,
    switch_on: bool,
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

fn build_page(ui: &mut Ui128, ctx: &WidgetContext, index: u8, switch_on: bool) -> Page {
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
        Page {
            scaffold,
            scroll,
            clip_item: Some(clip_item),
            button,
            switch: Some(switch),
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
        Page { scaffold, scroll, clip_item: None, button, switch: None, index }
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
        let clicked = app
            .page
            .button
            .on_msg(&mut app.ui, &app.ctx, &message)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        if clicked {
            target = if app.page_index == 0 { 1 } else { 0 };
        }
        if let Some(switch) = app.page.switch.as_mut() {
            if switch
                .on_msg(&mut app.ui, &app.ctx, &message)
                .unwrap_or_else(|_| berry_room::panic_exit(134))
            {
                app.switch_on = switch.is_checked();
            }
        }
        app.page
            .scroll
            .on_msg(&mut app.ui, &message)
            .unwrap_or_else(|_| berry_room::panic_exit(134));
    }
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

/// P2 runtime 真键盘链：Back 返回 page0 后，把当前 surface 协商为
/// Canvas + Touch|Keyboard，等待 QMP 注入 KEY_A 的 down/up 同码配对。
fn wait_real_keyboard(room: &mut AppRoom) {
    let grant = room
        .configure_surface(SurfaceRequest {
            mode: SurfaceModeKind::Canvas,
            input_classes: SURFACE_INPUT_TOUCH | SURFACE_INPUT_KEYBOARD,
            ime_mode: SURFACE_IME_DISABLED,
            a11y_mode: SURFACE_A11Y_HYBRID,
        })
        .unwrap_or_else(|e| fail(room, e.as_str()));
    if grant.input_classes & SURFACE_INPUT_KEYBOARD == 0
        || grant.contract_flags != 31
    {
        fail(room, "keyboard grant");
    }
    let ready = Line::new(b"UITESTKEYWAIT ok input=9\n");
    ready.print(room);

    let mut events = [KeyEvent {
        timestamp_ns: 0,
        code: 0,
        action: 0,
        modifiers: 0,
        device_id: 0,
    }; INPUT_KEY_EVENTS_MAX];
    let mut downs = 0u32;
    let mut ups = 0u32;
    loop {
        let count = match room.poll_keys(&mut events) {
            Ok(count) => count,
            Err(e) => fail(room, e.as_str()),
        };
        for event in events.iter().take(count) {
            if event.code != 30 {
                fail(room, "keyboard code");
            }
            match event.action {
                KEY_ACTION_DOWN => downs += 1,
                KEY_ACTION_UP => ups += 1,
                _ => {}
            }
        }
        if downs >= 1 && ups >= 1 {
            break;
        }
    }
    let line = Line::new(b"UITESTKEY ok code=30 downs=1 ups=1\n");
    line.print(room);
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
    let mut page = build_page(&mut ui, &ctx, 0, false);
    let stat = ui.layout();
    if stat.placed == 0 {
        fail(&room, "initial layout");
    }
    page.scroll.sync_bounds(&mut ui).unwrap_or_else(|_| berry_room::panic_exit(134));
    // Runtime 首帧即保留 24px 溢出样本：GUI8 截图与 boot-probe 使用同一
    // Clip 几何，不靠“刚好放得下”的样本假绿。
    page.scroll.scroll_by(&mut ui, 24).unwrap_or_else(|_| berry_room::panic_exit(134));
    let mut app = AppModel {
        ui,
        ctx,
        page,
        page_index: 0,
        switch_on: false,
        frame_seq: 0,
    };

    app.print_layout(&room);
    app.print_hit(&room);
    app.print_data(&mut room);
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
        app.assert_clip(&mut room);
        app.page.button
            .on_msg(
                &mut app.ui,
                &app.ctx,
                &Msg::Click { node: app.page.button.label },
            )
            .unwrap_or_else(|_| berry_room::panic_exit(134));
        app.replace_page(&mut room, 1);
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
        room.print(b"UITESTPROBE ok\n");
    } else {
        let (x, y) = wait_two_clicks(&mut app, &mut room);
        wait_real_keyboard(&mut room);
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
