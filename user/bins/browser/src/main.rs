//! BROWSER —— GUI F8 迁移后的诚实本地浏览器壳。
//!
//! WM/窗口/frame/输入/销毁与系统字形归 Berry Room；页面、URL pill 文本、
//! 诚实状态卡与返回按钮归 BerryUI/Berry Design。v1 无网络、无 HTML 引擎；
//! 不伪装网页加载。app 不解析 WM RPC、不映射 VMO、不计算 stride、不写像素。

#![no_std]
#![no_main]

use berry_gfx::cmd::{CommandEncoder, GlyphPos};
use berry_gfx::font::FontSize;
use berry_room::{
    AppRoom, INPUT_EVENTS_MAX, INPUT_SAMPLES_MAX, PointerEvent, RoomConfig, Sample,
    SYSTEM_FONT_ATLAS,
};
use berry_ui::hit::PtrEvent;
use berry_ui::sdk::{
    Button, Card, FontSource, InteractionState, Msg, RectI, Scaffold, ScrollView, TextVariant,
    Theme, Ui, WidgetContext, column, text,
};

type Ui64 = Ui<64>;

struct Page {
    scroll: ScrollView,
    back: Button,
}

struct App {
    ui: Ui64,
    ctx: WidgetContext,
    page: Page,
    frame_seq: u32,
}

struct RoomFontSource;
static ROOM_FONT_SOURCE: RoomFontSource = RoomFontSource;

impl FontSource for RoomFontSource {
    fn atlas(&self, scale: FontSize) -> Option<&'static dyn berry_gfx::font::GlyphAtlas> {
        (scale == FontSize::S32x48).then_some(&SYSTEM_FONT_ATLAS)
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
    bytes: [u8; 192],
    len: usize,
}

impl Line {
    fn new(head: &[u8]) -> Self {
        let mut bytes = [0u8; 192];
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

    fn point(&mut self, x: i32, y: i32) {
        let mut buf = [0u8; 12];
        let mut n = if x < 0 { buf[0] = b'-'; 1 } else { 0 };
        let ux = if x == i32::MIN { 2_147_483_648u32 } else { x.unsigned_abs() };
        n += append_u32(&mut buf[n..], ux);
        self.push(&buf[..n]);
        self.push(b",");
        let mut buf = [0u8; 12];
        let mut n = if y < 0 { buf[0] = b'-'; 1 } else { 0 };
        let uy = if y == i32::MIN { 2_147_483_648u32 } else { y.unsigned_abs() };
        n += append_u32(&mut buf[n..], uy);
        self.push(&buf[..n]);
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
    let mut line = [0u8; 96];
    let head = b"browser: ";
    line[..head.len()].copy_from_slice(head);
    let n = head.len() + what.len().min(line.len() - head.len());
    line[head.len()..n].copy_from_slice(&what.as_bytes()[..n - head.len()]);
    room.print(&line[..n]);
    room.exit(134)
}

fn build_page(ui: &mut Ui64, ctx: &WidgetContext) -> Page {
    let root = ui.root();
    let scaffold = match Scaffold::new(ui, root, ctx, b"Browser") {
        Ok(value) => value,
        Err(_) => berry_room::panic_exit(134),
    };
    let (_, viewport_h) = ui.viewport();
    let title_h = ctx.metrics.typography(berry_ui::sdk::TypographyToken::Title).cell_h() as u32;
    let bar = title_h.saturating_add(ctx.metrics.spacing(berry_ui::sdk::SpacingToken::S).saturating_mul(2));
    let scroll = match ScrollView::new(ui, scaffold.body, ctx, viewport_h.saturating_sub(bar)) {
        Ok(value) => value,
        Err(_) => berry_room::panic_exit(134),
    };
    let card = match Card::new(ui, scroll.content, ctx) {
        Ok(value) => value,
        Err(_) => berry_room::panic_exit(134),
    };
    let stack = match column(ui, card.panel, ctx) {
        Ok(value) => value,
        Err(_) => berry_room::panic_exit(134),
    };
    let back = match Button::new(ui, stack, ctx, b"Back", InteractionState::Normal) {
        Ok(value) => value,
        Err(_) => berry_room::panic_exit(134),
    };
    match text(ui, stack, ctx, TextVariant::Title, b"berry://native-shell") {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    match text(ui, stack, ctx, TextVariant::Body, b"Native shell ready") {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    match text(ui, stack, ctx, TextVariant::Body, b"No web engine in v1") {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    Page { scroll, back }
}

impl App {
    fn render(&mut self, room: &mut AppRoom) {
        let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 128];
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
        let mut line = Line::new(b"BROWSERRENDER ok seq=");
        line.push_u32(self.frame_seq);
        line.push(b" cmds=");
        line.push_u32(commands as u32);
        line.push(b"\n");
        line.print(room);
    }

    fn print_layout(&self, room: &AppRoom) {
        let back = self.ui.bounds(self.page.back.panel).unwrap_or_else(|_| berry_room::panic_exit(134));
        let mut line = Line::new(b"BROWSERLAYOUT ok back=");
        line.rect(back);
        line.push(b"\n");
        line.print(room);
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
                if matches!(message, Msg::Click { .. })
                    && app
                        .page
                        .back
                        .on_msg(&mut app.ui, &app.ctx, &message)
                        .unwrap_or_else(|_| berry_room::panic_exit(134))
                {
                    let mut line = Line::new(b"BROWSERHIT ok node=back x=");
                    line.push_u32(x);
                    line.push(b" y=");
                    line.push_u32(y);
                    line.push(b"\n");
                    line.print(room);
                    room.print(b"BROWSERBACK ok\n");
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
        argv0: b"bin/browser",
        uses_fs: false,
        surface_escape: false,
};
    let mut room = match AppRoom::connect(env_ch, config) {
        Ok(room) => room,
        Err(_) => berry_room::panic_exit(134),
    };
    room.print(b"BROWSER up\n");
    let (width, height) = match room.open_window() {
        Ok(size) => size,
        Err(error) => fail(&room, error.as_str()),
    };
    room.print(b"BROWSERWIN ok\n");

    let theme = Theme::light();
    let mut ui = Ui64::new(width as u32, height as u32);
    let ctx = WidgetContext::for_ui(theme, &ui);
    ui.set_font_source(&ROOM_FONT_SOURCE);
    let page = build_page(&mut ui, &ctx);
    let mut scroll = page.scroll;
    let mut app = App { ui, ctx, page, frame_seq: 0 };
    scroll.sync_bounds(&mut app.ui).unwrap_or_else(|_| berry_room::panic_exit(134));
    app.ui.layout();
    app.print_layout(&room);
    app.render(&mut room);
    room.print(b"BROWSERFRAME ok\n");

    if room.boot_probe() {
        room.print(b"BROWSERPROBE ok\n");
    } else {
        run_interactive(&mut app, &mut room);
    }

    if let Err(error) = room.destroy() {
        fail(&room, error.as_str());
    }
    room.print(b"BROWSERDESTROY ok\n");
    room.print(b"BROWSEREXIT ok\n");
    room.exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_room::panic_exit(134)
}
