//! PHOTOS —— GUI F8 迁移后的照片样本浏览 app。
//!
//! WM/窗口/frame/输入/销毁与系统字形归 Berry Room；页面、主题与按钮状态归
//! BerryUI/Berry Design。PATTERN.BIN 的 READDIR/STAT/GET 与 5000B 逐字节
//! 校验继续走 berry-fsclient；图像经 BerryUI IconSource / SmoothArgb 进入
//! DisplayList。app 不解析 WM RPC、不映射 VMO、不计算 stride、不写像素循环。

#![no_std]
#![no_main]

use berry_fsclient::{dirent_next, get_chunk, readdir, stat as fs_stat, FsErr, RPC_BUF, TYPE_FILE};
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_gfx::font::FontSize;
use berry_room::{
    AppRoom, INPUT_EVENTS_MAX, INPUT_SAMPLES_MAX, PointerEvent, RoomConfig, Sample,
    SYSTEM_FONT_ATLAS,
};
use berry_ui::hit::PtrEvent;
use berry_ui::sdk::{
    Button, Card, FontSource, IconProps, InteractionState, Msg, Props, RectI, Scaffold,
    ScrollView, TextVariant, Theme, Ui, ViewKind, ViewSpec, WidgetContext, column, text,
};

type Ui32 = Ui<32>;
const PATTERN_LEN: usize = 5000;
const SRC_W: u32 = 50;
const SRC_H: u32 = 100;

struct GridPage {
    scroll: ScrollView,
    back: Button,
    image: berry_ui::sdk::NodeId,
    open: Button,
}

struct ViewPage {
    scroll: ScrollView,
    back: Button,
    image: berry_ui::sdk::NodeId,
}

struct App {
    ui: Ui32,
    ctx: WidgetContext,
    mode: bool, // false=grid, true=view
    page: Page,
    frame_seq: u32,
}

enum Page {
    Grid(GridPage),
    View(ViewPage),
}

struct RoomFontSource;
static ROOM_FONT_SOURCE: RoomFontSource = RoomFontSource;

impl FontSource for RoomFontSource {
    fn atlas(&self, scale: FontSize) -> Option<&'static dyn berry_gfx::font::GlyphAtlas> {
        (scale == FontSize::S32x48).then_some(&SYSTEM_FONT_ATLAS)
    }
}

const fn pattern_byte(index: usize) -> u8 {
    ((index * 31 + 7) % 256) as u8
}

const fn pattern_argb() -> [u32; PATTERN_LEN] {
    let mut out = [0u32; PATTERN_LEN];
    let mut i = 0usize;
    while i < PATTERN_LEN {
        let v = pattern_byte(i) as u32;
        out[i] = 0xFF00_0000 | (v << 16) | (v << 8) | v;
        i += 1;
    }
    out
}

/// PATTERN.BIN 是确定性公式数据；GET 后逐字节校验，再由同一公式生成
/// 只读 ARGB 源。Remote/local 两种拓扑的像素语义完全一致。
static PATTERN_ARGB: [u32; PATTERN_LEN] = pattern_argb();

struct PatternSource;

impl berry_ui::sdk::IconSource for PatternSource {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        (id == 1).then_some((SRC_W, SRC_H))
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        (id == 1).then_some(&PATTERN_ARGB)
    }

    fn smooth_scale(&self, id: u16) -> bool {
        id == 1
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
    bytes: [u8; 224],
    len: usize,
}

impl Line {
    fn new(head: &[u8]) -> Self {
        let mut bytes = [0u8; 224];
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
    let head = b"photos: ";
    line[..head.len()].copy_from_slice(head);
    let n = head.len() + what.len().min(line.len() - head.len());
    line[head.len()..n].copy_from_slice(&what.as_bytes()[..n - head.len()]);
    room.print(&line[..n]);
    room.exit(134)
}

fn load_pattern(room: &AppRoom) -> (usize, bool) {
    let handle = room.fs_handle();
    let mut wire = [0u8; RPC_BUF];
    let root_len = match readdir(handle, b"", &mut wire) {
        Ok(len) => len,
        Err(_) => fail(room, "readdir"),
    };
    let mut pos = 0usize;
    let mut root_count = 0usize;
    while dirent_next(&wire[..root_len], &mut pos).is_some() {
        root_count += 1;
    }

    let (kind, size) = match fs_stat(handle, b"PATTERN.BIN") {
        Ok(value) => value,
        Err(FsErr::Noent) => return (root_count, false),
        Err(_) => fail(room, "stat"),
    };
    if kind != TYPE_FILE || size as usize != PATTERN_LEN {
        fail(room, "pattern size");
    }
    let mut data = [0u8; PATTERN_LEN];
    let mut off = 0usize;
    loop {
        let (take, more) = match get_chunk(handle, b"PATTERN.BIN", off as u32, &mut wire) {
            Ok(value) => value,
            Err(_) => fail(room, "get"),
        };
        if off + take > data.len() {
            fail(room, "pattern long");
        }
        data[off..off + take].copy_from_slice(&wire[..take]);
        off += take;
        if !more {
            break;
        }
    }
    if off != PATTERN_LEN {
        fail(room, "pattern short");
    }
    for (index, &byte) in data.iter().enumerate() {
        if byte != pattern_byte(index) {
            fail(room, "pattern byte");
        }
    }
    (root_count, true)
}

fn add_image<const N: usize>(
    ui: &mut Ui<N>, parent: berry_ui::sdk::NodeId, width: u32, height: u32,
) -> berry_ui::sdk::NodeId {
    match ui.add(
        parent,
        ViewSpec {
            kind: ViewKind::Icon,
            key: 0,
            props: Props::Icon(IconProps {
                id: 1,
                fg: Color { r: 0, g: 0, b: 0 },
                w: Some(width),
                h: Some(height),
            }),
        },
    ) {
        Ok(id) => id,
        Err(_) => berry_room::panic_exit(134),
    }
}

fn build_grid(ui: &mut Ui32, ctx: &WidgetContext) -> GridPage {
    let root = ui.root();
    let scaffold = match Scaffold::new(ui, root, ctx, b"Photos") {
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
    match text(ui, stack, ctx, TextVariant::Title, b"PATTERN.BIN") {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    let image = add_image(ui, stack, 360, 720);
    let open = match Button::new(ui, stack, ctx, b"View", InteractionState::Normal) {
        Ok(value) => value,
        Err(_) => berry_room::panic_exit(134),
    };
    GridPage { scroll, back, image, open }
}

fn build_view(ui: &mut Ui32, ctx: &WidgetContext) -> ViewPage {
    let root = ui.root();
    let scaffold = match Scaffold::new(ui, root, ctx, b"Photos") {
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
    match text(ui, stack, ctx, TextVariant::Title, b"PATTERN.BIN 50x100") {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    let image = add_image(ui, stack, 540, 1080);
    ViewPage { scroll, back, image }
}

impl App {
    fn render(&mut self, room: &mut AppRoom) {
        let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 160];
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
        let mut line = Line::new(b"PHOTOSRENDER ok seq=");
        line.push_u32(self.frame_seq);
        line.push(b" cmds=");
        line.push_u32(commands as u32);
        line.push(b"\n");
        line.print(room);
    }

    fn print_layout(&self, room: &AppRoom) {
        let mut line = Line::new(b"PHOTOSLAYOUT ok mode=");
        line.push_u32(u32::from(self.mode));
        let (back, image, open) = match &self.page {
            Page::Grid(page) => (page.back.panel, page.image, Some(page.open.panel)),
            Page::View(page) => (page.back.panel, page.image, None),
        };
        let rect = self.ui.bounds(back).unwrap_or_else(|_| berry_room::panic_exit(134));
        line.push(b" back=");
        line.rect(rect);
        let rect = self.ui.bounds(image).unwrap_or_else(|_| berry_room::panic_exit(134));
        line.push(b" image=");
        line.rect(rect);
        if let Some(open) = open {
            let rect = self.ui.bounds(open).unwrap_or_else(|_| berry_room::panic_exit(134));
            line.push(b" open=");
            line.rect(rect);
        }
        line.push(b"\n");
        line.print(room);
    }

    fn replace_grid(&mut self) {
        let root = self.ui.root();
        if self.ui.remove(root).is_err() {
            berry_room::panic_exit(134);
        }
        self.page = Page::Grid(build_grid(&mut self.ui, &self.ctx));
        self.mode = false;
    }

    fn replace_view(&mut self) {
        let root = self.ui.root();
        if self.ui.remove(root).is_err() {
            berry_room::panic_exit(134);
        }
        self.page = Page::View(build_view(&mut self.ui, &self.ctx));
        self.mode = true;
    }

    fn sync_scroll(&mut self) {
        let mut scroll = match &self.page {
            Page::Grid(page) => page.scroll,
            Page::View(page) => page.scroll,
        };
        scroll.sync_bounds(&mut self.ui).unwrap_or_else(|_| berry_room::panic_exit(134));
    }

    fn rebuild_and_render(&mut self, room: &mut AppRoom) {
        self.sync_scroll();
        self.ui.layout();
        self.print_layout(room);
        self.render(room);
        room.print(b"PHOTOSFRAME ok\n");
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
                if !matches!(message, Msg::Click { .. }) {
                    continue;
                }
                match &mut app.page {
                    Page::Grid(page) => {
                        if page
                            .open
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134))
                        {
                            room.print(b"PHOTOSOPEN ok\n");
                            app.replace_view();
                            app.rebuild_and_render(room);
                            room.print(b"PHOTOSVIEW ok\n");
                        } else if page
                            .back
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134))
                        {
                            let mut line = Line::new(b"PHOTOSHIT ok node=back x=");
                            line.push_u32(x);
                            line.push(b" y=");
                            line.push_u32(y);
                            line.push(b"\n");
                            line.print(room);
                            room.print(b"PHOTOSBACK ok\n");
                            return;
                        }
                    }
                    Page::View(page) => {
                        if page
                            .back
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134))
                        {
                            app.replace_grid();
                            app.rebuild_and_render(room);
                            room.print(b"PHOTOSLIST ok n=1\n");
                        }
                    }
                }
            }
        }
    }
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env_ch: u32) -> ! {
    let config = RoomConfig {
        argv0: b"bin/photos",
        uses_fs: true,
        surface_escape: false,
};
    let mut room = match AppRoom::connect(env_ch, config) {
        Ok(room) => room,
        Err(_) => berry_room::panic_exit(134),
    };
    room.print(b"PHOTOS up\n");
    let (width, height) = match room.open_window() {
        Ok(size) => size,
        Err(error) => fail(&room, error.as_str()),
    };
    room.print(b"PHOTOSWIN ok\n");

    let (root_count, remote) = load_pattern(&room);
    if root_count < 1 {
        fail(&room, "pattern final");
    }
    room.print(b"PHOTOSLIST ok n=1\n");
    if remote {
        room.print(b"PHOTOSDATA ok n=5000\n");
    } else {
        room.print(b"PHOTOSLOCAL ok n=5000\n");
    }

    let theme = Theme::light();
    let mut ui = Ui32::new(width as u32, height as u32);
    let ctx = WidgetContext::for_ui(theme, &ui);
    ui.set_font_source(&ROOM_FONT_SOURCE);
    static PATTERN_SOURCE: PatternSource = PatternSource;
    ui.set_icon_source(&PATTERN_SOURCE);
    let grid = build_grid(&mut ui, &ctx);
    let mut scroll = grid.scroll;
    let mut app = App {
        ui,
        ctx,
        mode: false,
        page: Page::Grid(grid),
        frame_seq: 0,
    };
    scroll.sync_bounds(&mut app.ui).unwrap_or_else(|_| berry_room::panic_exit(134));
    app.ui.layout();
    app.print_layout(&room);
    app.render(&mut room);
    room.print(b"PHOTOSFRAME ok\n");

    if room.boot_probe() {
        room.print(b"PHOTOSPROBE ok\n");
    } else {
        run_interactive(&mut app, &mut room);
    }

    if let Err(error) = room.destroy() {
        fail(&room, error.as_str());
    }
    room.print(b"PHOTOSDESTROY ok\n");
    room.print(b"PHOTSEXIT ok\n");
    room.exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_room::panic_exit(134)
}
