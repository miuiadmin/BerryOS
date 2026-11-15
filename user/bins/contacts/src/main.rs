//! CONTACTS —— GUI F7 迁移后的联系人 app。
//!
//! WM/窗口/frame/输入/销毁与系统字形归 Berry Room；列表/详情/收藏视觉归
//! BerryUI/Berry Design。CONTACTS.TXT 的 READDIR/STAT/GET 继续走
//! berry-fsclient；无数据盘拓扑下保留编译内同构样本 fallback。

#![no_std]
#![no_main]

use berry_fsclient::{
    dirent_next, get_chunk, readdir, stat as fs_stat, FsErr, RPC_BUF, TYPE_FILE,
};
use berry_gfx::cmd::{CommandEncoder, GlyphPos};
use berry_gfx::font::FontSize;
use berry_room::{
    AppRoom, INPUT_EVENTS_MAX, INPUT_SAMPLES_MAX, PointerEvent, RoomConfig, Sample,
    SYSTEM_FONT_ATLAS,
};
use berry_ui::hit::PtrEvent;
use berry_ui::sdk::{
    Button, Card, FontSource, InteractionState, ListItem, Msg, NavigationIconSource, RectI,
    Scaffold, ScrollView, TextVariant, Theme, Ui, WidgetContext, column, text, NAV_ICON_BACK,
    NAV_ICON_BACK_ACTIVE,
};

type Ui32 = Ui<32>;
const MAX_CONTACTS: usize = 4;
const DATA_MAX: usize = 256;

struct Contact {
    name: [u8; 24],
    name_len: usize,
    number: [u8; 16],
    number_len: usize,
    favorite: bool,
}

impl Contact {
    const EMPTY: Contact = Contact {
        name: [0u8; 24],
        name_len: 0,
        number: [0u8; 16],
        number_len: 0,
        favorite: false,
    };

    fn name(&self) -> &[u8] {
        &self.name[..self.name_len]
    }

    fn number(&self) -> &[u8] {
        &self.number[..self.number_len]
    }
}

struct ListPage {
    scroll: ScrollView,
    back: berry_ui::sdk::IconButton,
    rows: [Option<ListItem>; MAX_CONTACTS],
}

struct DetailPage {
    scroll: ScrollView,
    back: berry_ui::sdk::IconButton,
    favorite: Button,
}

struct App {
    ui: Ui32,
    ctx: WidgetContext,
    contacts: [Contact; MAX_CONTACTS],
    detail: usize,
    mode: bool,
    safe_top: u32,
    safe_bottom: u32,
    page: Page,
    frame_seq: u32,
}

enum Page {
    List(ListPage),
    Detail(DetailPage),
}

struct RoomFontSource;
static ROOM_FONT_SOURCE: RoomFontSource = RoomFontSource;

impl FontSource for RoomFontSource {
    fn atlas(&self, scale: FontSize) -> Option<&'static dyn berry_gfx::font::GlyphAtlas> {
        (scale == FontSize::S32x48).then_some(&SYSTEM_FONT_ATLAS)
    }
}

static NAVIGATION_ICONS: NavigationIconSource = NavigationIconSource;

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
    let head = b"contacts: ";
    line[..head.len()].copy_from_slice(head);
    let n = head.len() + what.len().min(line.len() - head.len());
    line[head.len()..n].copy_from_slice(&what.as_bytes()[..n - head.len()]);
    room.print(&line[..n]);
    room.exit(134)
}

fn fallback_contacts(out: &mut [Contact; MAX_CONTACTS]) {
    const NAMES: [[&[u8]; 2]; MAX_CONTACTS] = [
        [b"Alice Chen", b"13800000001"],
        [b"Berry Wang", b"13800000002"],
        [b"Cedar Li", b"13800000003"],
        [b"Dawn Zhou", b"13800000004"],
    ];
    for (slot, pair) in out.iter_mut().zip(NAMES) {
        *slot = Contact::EMPTY;
        slot.name[..pair[0].len()].copy_from_slice(pair[0]);
        slot.name_len = pair[0].len();
        slot.number[..pair[1].len()].copy_from_slice(pair[1]);
        slot.number_len = pair[1].len();
    }
}

fn parse_contacts(room: &AppRoom, data: &[u8], out: &mut [Contact; MAX_CONTACTS]) -> usize {
    for slot in out.iter_mut() {
        *slot = Contact::EMPTY;
    }
    let mut count = 0usize;
    let mut start = 0usize;
    while start < data.len() {
        let end = match data[start..].iter().position(|&c| c == b'\n') {
            Some(i) => start + i,
            None => data.len(),
        };
        if end == start || count >= MAX_CONTACTS {
            fail(room, "contact shape");
        }
        let line = &data[start..end];
        let Some(comma) = line.iter().position(|&c| c == b',') else {
            fail(room, "contact comma");
        };
        let (name, number) = (&line[..comma], &line[comma + 1..]);
        if name.is_empty()
            || name.len() > 24
            || number.len() != 11
            || !number.iter().all(u8::is_ascii_digit)
        {
            fail(room, "contact fields");
        }
        out[count].name[..name.len()].copy_from_slice(name);
        out[count].name_len = name.len();
        out[count].number[..number.len()].copy_from_slice(number);
        out[count].number_len = number.len();
        count += 1;
        start = if end < data.len() { end + 1 } else { end };
    }
    if count != MAX_CONTACTS {
        fail(room, "contact count");
    }
    count
}

fn load_contacts(room: &AppRoom, out: &mut [Contact; MAX_CONTACTS]) -> (usize, usize, bool) {
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

    let (kind, size) = match fs_stat(handle, b"CONTACTS.TXT") {
        Ok(value) => value,
        Err(FsErr::Noent) => {
            fallback_contacts(out);
            return (MAX_CONTACTS, root_count, false);
        }
        Err(_) => fail(room, "stat"),
    };
    if kind != TYPE_FILE || size as usize > DATA_MAX {
        fail(room, "contacts size");
    }
    let mut data = [0u8; DATA_MAX];
    let mut off = 0usize;
    loop {
        let (take, more) = match get_chunk(handle, b"CONTACTS.TXT", off as u32, &mut wire) {
            Ok(value) => value,
            Err(_) => fail(room, "get"),
        };
        if off + take > data.len() {
            fail(room, "contacts long");
        }
        data[off..off + take].copy_from_slice(&wire[..take]);
        off += take;
        if !more {
            break;
        }
    }
    if off != size as usize {
        fail(room, "contacts short");
    }
    (parse_contacts(room, &data[..off], out), root_count, true)
}

fn build_list(
    ui: &mut Ui32, ctx: &WidgetContext, contacts: &[Contact; MAX_CONTACTS], safe_top: u32,
    safe_bottom: u32,
) -> ListPage {
    let root = ui.root();
    let scaffold = match Scaffold::large_with_back_safe(
        ui, root, ctx, b"Contacts", b"4 contacts", NAV_ICON_BACK,
        NAV_ICON_BACK_ACTIVE, safe_top, safe_bottom,
    ) {
        Ok(value) => value,
        Err(_) => berry_room::panic_exit(134),
    };
    let back = match scaffold.app_bar.back {
        Some(value) => value,
        None => berry_room::panic_exit(134),
    };
    let (_, viewport_h) = ui.viewport();
    let body_h = viewport_h
        .saturating_sub(safe_top)
        .saturating_sub(scaffold.app_bar.instance_height(ctx))
        .saturating_sub(safe_bottom);
    let scroll = match ScrollView::new(ui, scaffold.body, ctx, body_h) {
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
    let mut rows = [None; MAX_CONTACTS];
    for (slot, contact) in rows.iter_mut().zip(contacts.iter()) {
        *slot = Some(match ListItem::new(
            ui,
            stack,
            ctx,
            contact.name(),
            Some(contact.number()),
            contact.favorite,
        ) {
            Ok(value) => value,
            Err(_) => berry_room::panic_exit(134),
        });
    }
    ListPage { scroll, back, rows }
}

fn build_detail(
    ui: &mut Ui32, ctx: &WidgetContext, contacts: &[Contact; MAX_CONTACTS], detail: usize,
    safe_top: u32, safe_bottom: u32,
) -> DetailPage {
    let root = ui.root();
    let scaffold = match Scaffold::with_back_safe(
        ui, root, ctx, b"Contacts", NAV_ICON_BACK, NAV_ICON_BACK_ACTIVE, safe_top, safe_bottom,
    ) {
        Ok(value) => value,
        Err(_) => berry_room::panic_exit(134),
    };
    let back = match scaffold.app_bar.back {
        Some(value) => value,
        None => berry_room::panic_exit(134),
    };
    let (_, viewport_h) = ui.viewport();
    let body_h = viewport_h
        .saturating_sub(safe_top)
        .saturating_sub(scaffold.app_bar.instance_height(ctx))
        .saturating_sub(safe_bottom);
    let scroll = match ScrollView::new(ui, scaffold.body, ctx, body_h) {
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
    match text(ui, stack, ctx, TextVariant::Title, contacts[detail].name()) {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    match text(ui, stack, ctx, TextVariant::Body, contacts[detail].number()) {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    let favorite_state = if contacts[detail].favorite {
        InteractionState::Selected
    } else {
        InteractionState::Normal
    };
    let favorite = match Button::new(ui, stack, ctx, b"Favorite", favorite_state) {
        Ok(value) => value,
        Err(_) => berry_room::panic_exit(134),
    };
    DetailPage { scroll, back, favorite }
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
        let commands = match room.render_display_list_damaged(&list, outcome.damage.envelope().as_ref()) {
            Ok(commands) => commands,
            Err(error) => fail(room, error.as_str()),
        };
        self.frame_seq = self.frame_seq.saturating_add(1);
        if let Err(error) = room.submit(self.frame_seq) {
            fail(room, error.as_str());
        }
        let mut line = Line::new(b"CONTACTSRENDER ok seq=");
        line.push_u32(self.frame_seq);
        line.push(b" cmds=");
        line.push_u32(commands as u32);
        line.push(b"\n");
        line.print(room);
    }

    fn print_layout(&self, room: &AppRoom) {
        let mut line = Line::new(b"CONTACTSLAYOUT ok mode=");
        line.push_u32(u32::from(self.mode));
        let (back, extra) = match &self.page {
            Page::List(page) => (page.back.panel, page.rows[1].map(|b| b.panel)),
            Page::Detail(page) => (page.back.panel, Some(page.favorite.panel)),
        };
        let rect = self.ui.bounds(back).unwrap_or_else(|_| berry_room::panic_exit(134));
        line.push(b" back=");
        line.rect(rect);
        if let Some(extra) = extra {
            let rect = self.ui.bounds(extra).unwrap_or_else(|_| berry_room::panic_exit(134));
            let name: &[u8] = if self.mode { b" favorite=" } else { b" row=" };
            line.push(name);
            line.rect(rect);
        }
        line.push(b"\n");
        line.print(room);
    }

    fn replace_list(&mut self) {
        let root = self.ui.root();
        if self.ui.remove(root).is_err() {
            berry_room::panic_exit(134);
        }
        self.page = Page::List(build_list(
            &mut self.ui, &self.ctx, &self.contacts, self.safe_top, self.safe_bottom,
        ));
        self.mode = false;
    }

    fn replace_detail(&mut self, detail: usize) {
        let root = self.ui.root();
        if self.ui.remove(root).is_err() {
            berry_room::panic_exit(134);
        }
        self.detail = detail;
        self.page = Page::Detail(build_detail(
            &mut self.ui, &self.ctx, &self.contacts, detail, self.safe_top, self.safe_bottom,
        ));
        self.mode = true;
    }

    fn sync_scroll(&mut self) {
        let mut scroll = match &self.page {
            Page::List(page) => page.scroll,
            Page::Detail(page) => page.scroll,
        };
        scroll.sync_bounds(&mut self.ui).unwrap_or_else(|_| berry_room::panic_exit(134));
    }

    fn rebuild_and_render(&mut self, room: &mut AppRoom) {
        self.sync_scroll();
        self.ui.layout();
        self.print_layout(room);
        self.render(room);
        room.print(b"CONTACTSFRAME ok\n");
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
                match &mut app.page {
                    Page::List(page) => {
                        for row in page.rows.iter_mut().flatten() {
                            let _ = row
                                .on_msg(&mut app.ui, &app.ctx, &message)
                                .unwrap_or_else(|_| berry_room::panic_exit(134));
                        }
                    }
                    Page::Detail(page) => {
                        let _ = page
                            .favorite
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134));
                    }
                }
                if !matches!(message, Msg::Click { .. }) {
                    if app.ui.damage().union_px() != 0 {
                        app.render(room);
                        room.print(b"CONTACTSFRAME ok\n");
                    }
                    continue;
                }
                match &mut app.page {
                    Page::List(page) => {
                        if page
                            .back
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134))
                        {
                            let mut line = Line::new(b"CONTACTSHIT ok node=back x=");
                            line.push_u32(x);
                            line.push(b" y=");
                            line.push_u32(y);
                            line.push(b"\n");
                            line.print(room);
                            room.print(b"CONTACTSBACK ok\n");
                            return;
                        }
                        for (index, slot) in page.rows.iter_mut().enumerate() {
                            let Some(button) = slot else { continue };
                            if button
                                .on_msg(&mut app.ui, &app.ctx, &message)
                                .unwrap_or_else(|_| berry_room::panic_exit(134))
                            {
                                app.replace_detail(index);
                                app.rebuild_and_render(room);
                                let mut line = Line::new(b"CONTACTSDETAIL ok i=");
                                line.push_u32(index as u32);
                                line.push(b"\n");
                                line.print(room);
                                break;
                            }
                        }
                    }
                    Page::Detail(page) => {
                        if page
                            .favorite
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134))
                        {
                            let index = app.detail;
                            app.contacts[index].favorite = !app.contacts[index].favorite;
                            app.replace_detail(index);
                            app.rebuild_and_render(room);
                            room.print(b"CONTACTSFAV ok n=1\n");
                        } else if page
                            .back
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134))
                        {
                            app.replace_list();
                            app.rebuild_and_render(room);
                            let mut line = Line::new(b"CONTACTSLIST ok n=");
                            line.push_u32(MAX_CONTACTS as u32);
                            line.push(b"\n");
                            line.print(room);
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
        argv0: b"bin/contacts",
        uses_fs: true,
        surface_escape: false,
};
    let mut room = match AppRoom::connect(env_ch, config) {
        Ok(room) => room,
        Err(_) => berry_room::panic_exit(134),
    };
    room.print(b"CONTACTS up\n");
    let (width, height) = match room.open_window() {
        Ok(size) => size,
        Err(error) => fail(&room, error.as_str()),
    };
    room.print(b"CONTACTSWIN ok\n");

    let mut contacts = [Contact::EMPTY; MAX_CONTACTS];
    let (count, root_count, remote) = load_contacts(&room, &mut contacts);
    if count != MAX_CONTACTS || root_count < 1 {
        fail(&room, "contacts final");
    }
    let mut list_line = Line::new(b"CONTACTSLIST ok n=");
    list_line.push_u32(count as u32);
    list_line.push(b"\n");
    list_line.print(&room);
    if remote {
        room.print(b"CONTACTSREAD ok n=4\n");
    } else {
        room.print(b"CONTACTSLOCAL ok n=4\n");
    }

    let theme = Theme::light();
    let mut ui = Ui32::new(width as u32, height as u32);
    let ctx = WidgetContext::for_ui(theme, &ui);
    ui.set_font_source(&ROOM_FONT_SOURCE);
    ui.set_icon_source(&NAVIGATION_ICONS);
    let insets = room.occlusion().effective_padding();
    let safe_top = u32::from(insets[1]);
    let safe_bottom = u32::from(insets[3]).max(ctx.metrics.effective_px(144));
    let list = build_list(&mut ui, &ctx, &contacts, safe_top, safe_bottom);
    let mut scroll = list.scroll;
    let mut app = App {
        ui,
        ctx,
        contacts,
        detail: 0,
        mode: false,
        safe_top,
        safe_bottom,
        page: Page::List(list),
        frame_seq: 0,
    };
    scroll.sync_bounds(&mut app.ui).unwrap_or_else(|_| berry_room::panic_exit(134));
    app.ui.layout();
    app.print_layout(&room);
    app.render(&mut room);
    room.print(b"CONTACTSFRAME ok\n");

    if room.boot_probe() {
        room.print(b"CONTACTSPROBE ok\n");
    } else {
        run_interactive(&mut app, &mut room);
    }

    if let Err(error) = room.destroy() {
        fail(&room, error.as_str());
    }
    room.print(b"CONTACTSDESTROY ok\n");
    room.print(b"CONTACTSEXIT ok\n");
    room.exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_room::panic_exit(134)
}
