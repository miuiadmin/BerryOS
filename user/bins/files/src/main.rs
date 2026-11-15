//! FILES —— GUI F7 迁移后的文件浏览 app。
//!
//! WM/窗口/frame/输入/销毁与系统字形归 Berry Room；页面、布局、主题与
//! 按钮状态归 BerryUI。READDIR/STAT/GET 继续走 berry-fsclient 会话机，
//! 但 app 不解析 WM RPC、不映射 VMO、不计算 stride、不写像素循环。

#![no_std]
#![no_main]

use berry_fsclient::{
    dirent_next, get_chunk, readdir, stat as fs_stat, NAME_MAX, RPC_BUF, TYPE_FILE,
};
use berry_gfx::cmd::{CommandEncoder, GlyphPos};
use berry_gfx::font::FontSize;
use berry_room::{
    AppRoom, INPUT_EVENTS_MAX, INPUT_SAMPLES_MAX, PHASE_UP, PointerEvent, RoomConfig, Sample,
    SYSTEM_FONT_ATLAS,
};
use berry_ui::hit::PtrEvent;
use berry_ui::sdk::{
    Card, ContextMenu, ContextMenuOutcome, FontSource, ListItem, LongPressGesture,
    LongPressOutcome, MenuItemSpec, Msg, NavigationIconSource, RectI, Scaffold, ScrollView,
    TextVariant, Theme, Ui, WidgetContext, column, text, LONG_PRESS_NS, NAV_ICON_BACK,
    NAV_ICON_BACK_ACTIVE,
};

type Ui96 = Ui<96>;
const MAX_ENTRIES: usize = 8;
const MENU_ITEMS: [MenuItemSpec; 2] = [
    MenuItemSpec { label: b"Open", dangerous: false },
    MenuItemSpec { label: b"Properties", dangerous: false },
];

fn row_owned(item: &ListItem, node: berry_ui::NodeId) -> bool {
    node == item.panel || node == item.title || item.detail == Some(node)
}

struct FileEntry {
    name: [u8; NAME_MAX],
    len: usize,
}

impl FileEntry {
    const EMPTY: FileEntry = FileEntry { name: [0u8; NAME_MAX], len: 0 };

    fn name(&self) -> &[u8] {
        &self.name[..self.len]
    }
}

struct FileData {
    index: usize,
    size: u32,
    bytes: [u8; 96],
    len: usize,
}

struct ListPage {
    scroll: ScrollView,
    back: berry_ui::sdk::IconButton,
    open: [Option<ListItem>; MAX_ENTRIES],
}

struct DetailPage {
    scroll: ScrollView,
    back: berry_ui::sdk::IconButton,
}

struct App {
    ui: Ui96,
    ctx: WidgetContext,
    entries: [FileEntry; MAX_ENTRIES],
    count: usize,
    selected: usize,
    mode: bool, // false=list, true=detail
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
    let head = b"files: ";
    line[..head.len()].copy_from_slice(head);
    let n = head.len() + what.len().min(line.len() - head.len());
    line[head.len()..n].copy_from_slice(&what.as_bytes()[..n - head.len()]);
    room.print(&line[..n]);
    room.exit(134)
}

fn load_entries(room: &AppRoom) -> ([FileEntry; MAX_ENTRIES], usize, usize) {
    let mut wire = [0u8; RPC_BUF];
    let handle = room.fs_handle();
    let blob_len = match readdir(handle, b"", &mut wire) {
        Ok(len) => len,
        Err(_) => fail(room, "readdir"),
    };
    let mut entries = [FileEntry::EMPTY; MAX_ENTRIES];
    let mut pos = 0usize;
    let mut count = 0usize;
    while let Some(name) = dirent_next(&wire[..blob_len], &mut pos) {
        if count == MAX_ENTRIES || name.len() > NAME_MAX {
            break;
        }
        entries[count].name[..name.len()].copy_from_slice(name);
        entries[count].len = name.len();
        count += 1;
    }
    let selected = (0..count)
        .find(|&i| entries[i].name() == b"HELLO.TXT")
        .unwrap_or(0);
    (entries, count, selected)
}

fn load_file(room: &AppRoom, entry: &FileEntry) -> FileData {
    let handle = room.fs_handle();
    let (kind, size) = match fs_stat(handle, entry.name()) {
        Ok(value) => value,
        Err(_) => fail(room, "stat"),
    };
    if kind != TYPE_FILE {
        fail(room, "not file");
    }
    let mut wire = [0u8; RPC_BUF];
    let mut out = [0u8; 96];
    let mut off = 0usize;
    loop {
        let (take, more) = match get_chunk(handle, entry.name(), off as u32, &mut wire) {
            Ok(value) => value,
            Err(_) => fail(room, "get"),
        };
        if off + take > out.len() {
            fail(room, "file long");
        }
        out[off..off + take].copy_from_slice(&wire[..take]);
        off += take;
        if !more {
            break;
        }
    }
    FileData { index: 0, size, bytes: out, len: off }
}

fn build_list(
    ui: &mut Ui96, ctx: &WidgetContext, entries: &[FileEntry; MAX_ENTRIES], count: usize,
    safe_top: u32, safe_bottom: u32,
) -> ListPage {
    let root = ui.root();
    let scaffold = match Scaffold::with_back_safe(
        ui, root, ctx, b"Files", NAV_ICON_BACK, NAV_ICON_BACK_ACTIVE, safe_top, safe_bottom,
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
    let mut open = [None; MAX_ENTRIES];
    for (slot, entry) in open.iter_mut().zip(entries.iter()).take(count) {
        *slot = Some(match ListItem::new(
            ui,
            stack,
            ctx,
            entry.name(),
            Some(b"Open file"),
            false,
        ) {
            Ok(value) => value,
            Err(_) => berry_room::panic_exit(134),
        });
    }
    let _ = count;
    ListPage { scroll, back, open }
}

fn build_detail(
    ui: &mut Ui96, ctx: &WidgetContext, entry: &FileEntry, data: &FileData, safe_top: u32,
    safe_bottom: u32,
) -> DetailPage {
    let root = ui.root();
    let scaffold = match Scaffold::with_back_safe(
        ui, root, ctx, b"Files", NAV_ICON_BACK, NAV_ICON_BACK_ACTIVE, safe_top, safe_bottom,
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
    match text(ui, stack, ctx, TextVariant::Title, entry.name()) {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    let mut size_line = [b'S', b'i', b'z', b'e', b' ', 0, 0, 0, 0, 0, 0, 0];
    let n = 5 + append_u32(&mut size_line[5..], data.size);
    match text(ui, stack, ctx, TextVariant::Body, &size_line[..n]) {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    let mut preview = [0u8; 96];
    for (dst, src) in preview.iter_mut().zip(data.bytes[..data.len].iter()) {
        *dst = if src.is_ascii_graphic() || *src == b' ' { *src } else { b'.' };
    }
    match text(ui, stack, ctx, TextVariant::Body, &preview[..data.len]) {
        Ok(_) => {}
        Err(_) => berry_room::panic_exit(134),
    }
    DetailPage { scroll, back }
}

impl App {
    fn menu_row_for(&self, node: berry_ui::NodeId) -> Option<usize> {
        let Page::List(page) = &self.page else { return None };
        page.open
            .iter()
            .position(|row| row.as_ref().is_some_and(|row| row_owned(row, node)))
    }

    fn open_detail(&mut self, room: &mut AppRoom, index: usize) {
        let mut data = load_file(room, &self.entries[index]);
        data.index = index;
        room.print(b"FILESOPEN ok\n");
        let mut line = Line::new(b"FILESSTAT ok size=");
        line.push_u32(data.size);
        line.push(b"\n");
        line.print(room);
        let mut line = Line::new(b"FILESREAD ok n=");
        line.push_u32(data.len as u32);
        line.push(b"\n");
        line.print(room);
        self.selected = index;
        self.replace_detail(&data);
        self.rebuild_and_render(room);
    }

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
        let mut line = Line::new(b"FILESRENDER ok seq=");
        line.push_u32(self.frame_seq);
        line.push(b" cmds=");
        line.push_u32(commands as u32);
        line.push(b"\n");
        line.print(room);
    }

    fn print_layout(&self, room: &AppRoom) {
        let mut line = Line::new(b"FILESLAYOUT ok mode=");
        line.push_u32(u32::from(self.mode));
        let (back, open) = match &self.page {
            Page::List(page) => (page.back.panel, page.open[self.selected].map(|b| b.panel)),
            Page::Detail(page) => (page.back.panel, None),
        };
        let back_rect = self.ui.bounds(back).unwrap_or_else(|_| berry_room::panic_exit(134));
        line.push(b" back=");
        line.rect(back_rect);
        if let Some(open) = open {
            let rect = self.ui.bounds(open).unwrap_or_else(|_| berry_room::panic_exit(134));
            line.push(b" open=");
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
            &mut self.ui, &self.ctx, &self.entries, self.count, self.safe_top, self.safe_bottom,
        ));
        self.mode = false;
    }

    fn replace_detail(&mut self, data: &FileData) {
        let root = self.ui.root();
        if self.ui.remove(root).is_err() {
            berry_room::panic_exit(134);
        }
        self.page = Page::Detail(build_detail(
            &mut self.ui, &self.ctx, &self.entries[data.index], data, self.safe_top, self.safe_bottom,
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
        room.print(b"FILESFRAME ok\n");
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
    let mut long_press = LongPressGesture::new(LONG_PRESS_NS);
    let mut menu: Option<ContextMenu> = None;
    let mut menu_row = 0usize;
    loop {
        let count = match room.poll_input(&mut events) {
            Ok(count) => count,
            Err(error) => fail(room, error.as_str()),
        };
        for event in events.iter_mut().take(count) {
            let (x, y) = (event.x, event.y);
            let phase = event.phase;
            let mut pending_menu = None;
            // UP 先用 Berry Room 单调时钟定谳长按，再泵出 Release/Click；
            // Fired 后尾随 Click 由 gesture 抑制，原列表行不会误进详情。
            if phase == PHASE_UP {
                if let LongPressOutcome::Fired { node, x: ax, y: ay } =
                    long_press.tick(room.now_ns())
                {
                    if let Some(index) = app.menu_row_for(node) {
                        pending_menu = Some((
                            index,
                            i32::try_from(ax).unwrap_or(i32::MAX),
                            i32::try_from(ay).unwrap_or(i32::MAX),
                        ));
                    }
                }
            }
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
                let gesture_outcome = long_press.on_msg(&message);
                if let Some(context_menu) = menu.as_mut() {
                    match context_menu
                        .on_msg(&mut app.ui, &app.ctx, &message)
                        .unwrap_or_else(|_| berry_room::panic_exit(134))
                    {
                        ContextMenuOutcome::Selected(index) => {
                            menu = None;
                            long_press.reset();
                            let mut line = Line::new(b"FILESMENUSEL ok i=");
                            line.push_u32(index as u32);
                            line.push(b"\n");
                            line.print(room);
                            if index == 0 && menu_row < app.count {
                                app.open_detail(room, menu_row);
                            } else {
                                let mut line = Line::new(b"FILESPROP ok i=");
                                line.push_u32(menu_row as u32);
                                line.push(b"\n");
                                line.print(room);
                            }
                            continue;
                        }
                        ContextMenuOutcome::Dismissed => {
                            menu = None;
                            long_press.reset();
                            room.print(b"FILESMENUCLOSE ok\n");
                            if app.ui.damage().union_px() != 0 {
                                app.render(room);
                                room.print(b"FILESFRAME ok\n");
                            }
                            continue;
                        }
                        ContextMenuOutcome::None => {}
                    }
                    if app.ui.damage().union_px() != 0 {
                        app.render(room);
                        room.print(b"FILESFRAME ok\n");
                    }
                    continue;
                }
                if let Page::List(page) = &mut app.page {
                    for row in page.open.iter_mut().flatten() {
                        let _ = row
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134));
                    }
                }
                if !matches!(message, Msg::Click { .. }) {
                    if app.ui.damage().union_px() != 0 {
                        app.render(room);
                        room.print(b"FILESFRAME ok\n");
                    }
                    continue;
                }
                if matches!(gesture_outcome, LongPressOutcome::ClickSuppressed { .. }) {
                    continue;
                }
                match &mut app.page {
                    Page::List(page) => {
                        if page
                            .back
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134))
                        {
                            let mut line = Line::new(b"FILESHIT ok node=back x=");
                            line.push_u32(x);
                            line.push(b" y=");
                            line.push_u32(y);
                            line.push(b"\n");
                            line.print(room);
                            room.print(b"FILESBACK ok\n");
                            return;
                        }
                        for (index, slot) in page.open.iter_mut().enumerate() {
                            let Some(button) = slot else { continue };
                            if button
                                .on_msg(&mut app.ui, &app.ctx, &message)
                                .unwrap_or_else(|_| berry_room::panic_exit(134))
                            {
                                app.open_detail(room, index);
                                break;
                            }
                        }
                    }
                    Page::Detail(page) => {
                        if page
                            .back
                            .on_msg(&mut app.ui, &app.ctx, &message)
                            .unwrap_or_else(|_| berry_room::panic_exit(134))
                        {
                            app.replace_list();
                            app.rebuild_and_render(room);
                            let mut line = Line::new(b"FILESLIST ok n=");
                            line.push_u32(app.count as u32);
                            line.push(b"\n");
                            line.print(room);
                        }
                    }
                }
            }
            if phase != PHASE_UP {
                if let LongPressOutcome::Fired { node, x: ax, y: ay } =
                    long_press.tick(room.now_ns())
                {
                    if let Some(index) = app.menu_row_for(node) {
                        pending_menu = Some((
                            index,
                            i32::try_from(ax).unwrap_or(i32::MAX),
                            i32::try_from(ay).unwrap_or(i32::MAX),
                        ));
                    }
                }
            }
            if let Some((index, ax, ay)) = pending_menu {
                if menu.is_none() {
                    if let Ok(value) =
                        ContextMenu::new(&mut app.ui, &app.ctx, ax, ay, &MENU_ITEMS)
                    {
                        menu = Some(value);
                        menu_row = index;
                        app.ui.layout();
                        let mut line = Line::new(b"FILESMENU ok i=");
                        line.push_u32(index as u32);
                        line.push(b" item0=");
                        if let Some(context_menu) = menu.as_ref() {
                            if let Ok(item) = context_menu.menu.item_panel(0) {
                                line.rect(
                                    app.ui.bounds(item)
                                        .unwrap_or_else(|_| berry_room::panic_exit(134)),
                                );
                            }
                        }
                        line.push(b"\n");
                        line.print(room);
                        app.render(room);
                        room.print(b"FILESFRAME ok\n");
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
        argv0: b"bin/files",
        uses_fs: true,
        surface_escape: false,
};
    let mut room = match AppRoom::connect(env_ch, config) {
        Ok(room) => room,
        Err(_) => berry_room::panic_exit(134),
    };
    room.print(b"FILES up\n");
    let (width, height) = match room.open_window() {
        Ok(size) => size,
        Err(error) => fail(&room, error.as_str()),
    };
    room.print(b"FILESWIN ok\n");

    let (entries, count, selected) = load_entries(&room);
    let mut list_line = Line::new(b"FILESLIST ok n=");
    list_line.push_u32(count as u32);
    list_line.push(b"\n");
    list_line.print(&room);
    let mut line = Line::new(b"FILESSEL ok i=");
    line.push_u32(selected as u32);
    line.push(b"\n");
    line.print(&room);

    let theme = Theme::light();
    let mut ui = Ui96::new(width as u32, height as u32);
    let ctx = WidgetContext::for_ui(theme, &ui);
    ui.set_font_source(&ROOM_FONT_SOURCE);
    ui.set_icon_source(&NAVIGATION_ICONS);
    let insets = room.occlusion().effective_padding();
    let safe_top = u32::from(insets[1]);
    let safe_bottom = u32::from(insets[3]).max(ctx.metrics.effective_px(144));
    let list = build_list(&mut ui, &ctx, &entries, count, safe_top, safe_bottom);
    let mut scroll = list.scroll;
    let mut app = App {
        ui,
        ctx,
        entries,
        count,
        selected,
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
    room.print(b"FILESFRAME ok\n");

    if room.boot_probe() {
        room.print(b"FILESPROBE ok\n");
    } else {
        run_interactive(&mut app, &mut room);
    }

    if let Err(error) = room.destroy() {
        fail(&room, error.as_str());
    }
    room.print(b"FILESDESTROY ok\n");
    room.print(b"FILESEXIT ok\n");
    room.exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_room::panic_exit(134)
}
