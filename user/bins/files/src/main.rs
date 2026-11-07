//! Files Native Shell —— 真实 fsmgr RPC + 独立 GUI 进程。
//!
//! 链路：`wm=true` + `fs=["rootfs"]` → APP_SPAWN 交付 [rootfs VMO, fs
//! client, WMD broker] → 本进程自建 WM/input session 与全屏窗口。根目录
//! 列表、文件 STAT、内容 GET 都是真的 fs RPC；返回键经 wmd owner-local
//! 路由。v1 只读，不声称文件管理完成。

#![no_std]
#![no_main]

use berry_fsclient::dirent_next;
use berry_guiproto as gp;
use berry_nshfont::{circle, map as map_font, round_rect, shadow, text};
use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_clock_read,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_one, berry_process_exit,
    berry_vmar_map, berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN,
    BERRY_ERR_BUSY, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_WRITE, SIGNAL_READABLE,
    SIGNAL_WRITABLE,
};
use berry_fsproto as fp;

const FRAME_VA: usize = 0x5200_0000;
const RPC_CAP_NS: u64 = 120 * 1_000_000_000;
const MAX_ENTRIES: usize = 8;
const NAME_MAX: usize = berry_fsclient::NAME_MAX;

struct Env {
    boot_probe: bool,
    console: Handle,
    process: Handle,
    vmar: Handle,
    broker: Handle,
    fs: Handle,
}

#[derive(Clone, Copy)]
struct Layout {
    bar: usize,
    row_h: usize,
    top: usize,
}

#[derive(Clone, Copy)]
struct FileEntry {
    name: [u8; NAME_MAX],
    len: usize,
}

#[derive(Clone, Copy)]
struct FileData {
    index: usize,
    size: u32,
    bytes: [u8; 96],
    len: usize,
}

#[derive(Clone, Copy)]
enum Mode {
    List,
    Detail,
}

#[derive(Clone, Copy)]
enum Action {
    Open(usize),
    Back,
}

fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        berry_process_exit(134);
    }
    t
}

fn send(console: Handle, bytes: &[u8]) {
    let deadline = now() + RPC_CAP_NS;
    loop {
        let s = berry_channel_send(console, bytes.as_ptr(), bytes.len(), core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            berry_process_exit(134);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(console, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            berry_process_exit(134);
        }
    }
}

fn fail(console: Handle, what: &str) -> ! {
    let mut line = [0u8; 96];
    let head = b"files: ";
    line[..head.len()].copy_from_slice(head);
    let n = what.len().min(line.len() - head.len());
    line[head.len()..head.len() + n].copy_from_slice(&what.as_bytes()[..n]);
    send(console, &line[..head.len() + n]);
    berry_process_exit(134)
}

fn read_env(env_ch: Handle) -> Env {
    let mut blob = [0u8; 192];
    let mut hs = [0 as Handle; 8];
    let mut nb = blob.len();
    let mut nh = hs.len();
    if berry_channel_recv(env_ch, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh) != 0
        || nh < 7
        || nb <= b"bin/files".len()
        || &blob[..b"bin/files".len()] != b"bin/files"
        || hs[2] == 0
        || hs[3] == 0
        || hs[5] == 0
        || hs[6] == 0
    {
        berry_process_exit(134);
    }
    let boot_probe = blob[..nb]
        .windows(b"\0boot-probe".len())
        .any(|w| w == b"\0boot-probe");
    Env {
        boot_probe,
        console: hs[0],
        process: hs[2],
        vmar: hs[3],
        fs: hs[5],
        broker: hs[6],
    }
}

fn channel_roundtrip(
    ch: Handle,
    req: &[u8],
    rep: &mut [u8; 256],
) -> usize {
    let deadline = now() + RPC_CAP_NS;
    loop {
        let s = berry_channel_send(ch, req.as_ptr(), req.len(), core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            berry_process_exit(134);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            berry_process_exit(134);
        }
    }
    loop {
        let mut nb = rep.len();
        let s = berry_channel_recv(ch, rep.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut 0);
        if s == 0 {
            return nb;
        }
        if s != BERRY_ERR_AGAIN {
            berry_process_exit(134);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            berry_process_exit(134);
        }
    }
}

fn wm_rpc(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (usize, usize) {
    let deadline = now() + RPC_CAP_NS;
    loop {
        let (hp, hn) = if attach.is_empty() {
            (core::ptr::null(), 0)
        } else {
            (attach.as_ptr(), attach.len())
        };
        let s = berry_channel_send(ch, req.as_ptr(), req.len(), hp, hn);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            berry_process_exit(134);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            berry_process_exit(134);
        }
    }
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(ch, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        if s == 0 {
            return (nb, nh);
        }
        if s != BERRY_ERR_AGAIN {
            berry_process_exit(134);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            berry_process_exit(134);
        }
    }
}

fn pair(console: Handle) -> (Handle, Handle) {
    let mut a: Handle = 0;
    let mut b: Handle = 0;
    if berry_channel_create(0, &mut a, &mut b) != 0 || a == 0 || b == 0 {
        fail(console, "channel");
    }
    (a, b)
}

fn wm_connect(
    console: Handle, broker: Handle, process: Handle, vmar: Handle,
) -> (Handle, Handle, Handle) {
    let (wm, wm_peer) = pair(console);
    let (input, input_peer) = pair(console);
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let n = match gp::encode_wm_connect_req(&mut req, gp::WM_CLIENT_APP) {
        Some(n) => n,
        None => fail(console, "connect encode"),
    };
    let attach = [wm_peer, input_peer, process];
    let (n, nh) = wm_rpc(broker, &req[..n], &attach, &mut rep, &mut rhs);
    if gp::parse_wm_connect_rep(&rep[..n]).is_none() || nh != 1 || rhs[0] == 0 {
        fail(console, "connect rep");
    }
    let font_vmo = rhs[0];
    if !map_font(vmar, font_vmo) {
        fail(console, "font map");
    }
    if berry_handle_close(broker) != 0 {
        fail(console, "broker close");
    }
    (wm, input, font_vmo)
}

fn fs_call(fs: Handle, req: &[u8], rep: &mut [u8; 256]) -> usize {
    channel_roundtrip(fs, req, rep)
}

fn readdir_root(console: Handle, fs: Handle, out: &mut [FileEntry; MAX_ENTRIES]) -> usize {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let n = match fp::encode_readdir(&mut req, b"") {
        Some(n) => n,
        None => fail(console, "readdir encode"),
    };
    let n = fs_call(fs, &req[..n], &mut rep);
    let names = match fp::parse_reply_list(&rep[..n]) {
        Some(Ok(names)) => names,
        _ => fail(console, "readdir rep"),
    };
    let mut pos = 0usize;
    let mut count = 0usize;
    while let Some(name) = dirent_next(names, &mut pos) {
        if count == MAX_ENTRIES || name.len() > NAME_MAX {
            break;
        }
        out[count].name = [0u8; NAME_MAX];
        out[count].name[..name.len()].copy_from_slice(name);
        out[count].len = name.len();
        count += 1;
    }
    count
}

fn stat_file(console: Handle, fs: Handle, e: &FileEntry) -> (u8, u32) {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let n = match fp::encode_stat(&mut req, &e.name[..e.len]) {
        Some(n) => n,
        None => fail(console, "stat encode"),
    };
    let n = fs_call(fs, &req[..n], &mut rep);
    match fp::parse_reply_stat(&rep[..n]) {
        Some(Ok(v)) => v,
        _ => fail(console, "stat rep"),
    }
}

fn get_file(console: Handle, fs: Handle, e: &FileEntry) -> FileData {
    let (ty, size) = stat_file(console, fs, e);
    if ty != fp::TYPE_FILE {
        fail(console, "not file");
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let n = match fp::encode_get(&mut req, &e.name[..e.len], 0) {
        Some(n) => n,
        None => fail(console, "get encode"),
    };
    let n = fs_call(fs, &req[..n], &mut rep);
    let got = match fp::parse_reply_get(&rep[..n]) {
        Some(Ok(g)) => g,
        _ => fail(console, "get rep"),
    };
    let take = got.data.len().min(96);
    let mut out = FileData {
        index: 0,
        size,
        bytes: [0u8; 96],
        len: take,
    };
    out.bytes[..take].copy_from_slice(&got.data[..take]);
    out
}

fn put_px(fb: &mut [u8], stride: usize, x: usize, y: usize, c: [u8; 4]) {
    let off = y * stride + x * 4;
    let bytes = [c[2], c[1], c[0], 0];
    fb[off..off + 4].copy_from_slice(&bytes);
}

fn fill(fb: &mut [u8], stride: usize, x0: usize, y0: usize, w: usize, h: usize, c: [u8; 4]) {
    for y in y0..y0.saturating_add(h) {
        for x in x0..x0.saturating_add(w) {
            put_px(fb, stride, x, y, c);
        }
    }
}

fn layout(_w: usize, h: usize) -> Layout {
    let bar = (h / 12).max(64).min(160);
    let natural = (bar * 3 / 2).max(72).min(180);
    let avail = h.saturating_sub(bar + 48);
    let row_h = natural.min(avail / MAX_ENTRIES).max(48);
    Layout { bar, row_h, top: bar + 24 }
}

fn render_list(
    fb: &mut [u8], stride: usize, w: usize, _h: usize,
    lay: Layout, entries: &[FileEntry; MAX_ENTRIES], count: usize, selected: usize,
) {
    fill(fb, stride, 0, 0, w, _h, [0xF5, 0xF7, 0xFA, 0]);
    fill(fb, stride, 0, 0, w, lay.bar, [0x3F, 0x51, 0xB5, 0]);
    text(fb, stride, b"Files", lay.bar + 18, (lay.bar - 48) / 2, [0xFF, 0xFF, 0xFF, 0]);
    let cx = lay.bar / 2;
    let r = lay.bar.saturating_sub(18) / 2;
    circle(fb, stride, cx, cx, r, [0xE8, 0xEB, 0xFF]);
    for i in 0..count {
        let y = lay.top + i * lay.row_h;
        let bg = if i == selected { [0xE3, 0xE9, 0xFF] } else { [0xFF, 0xFF, 0xFF] };
        round_rect(fb, stride, 18, y + 8, w - 36, lay.row_h - 16, 36, bg);
        let ty = y + (lay.row_h - 48) / 2;
        text(fb, stride, &entries[i].name[..entries[i].len], 40, ty, [0x21, 0x2B, 0x36, 0]);
        text(fb, stride, b">", w - 80, ty, [0x8A, 0x96, 0xA5, 0]);
    }
}

fn render_detail(
    fb: &mut [u8], stride: usize, w: usize, h: usize,
    lay: Layout, entries: &[FileEntry; MAX_ENTRIES], data: &FileData,
) {
    let e = &entries[data.index];
    render_list(fb, stride, w, h, lay, entries, data.index + 1, data.index);
    // 用详情页覆盖列表主体，保留同一顶栏/返回键。
    let y0 = lay.bar + 12;
    let card_h = h.saturating_sub(y0 + 12);
    shadow(fb, stride, 24, y0 + 4, w - 44, card_h, 52);
    round_rect(fb, stride, 18, y0, w - 36, card_h, 48, [0xFF, 0xFF, 0xFF]);
    text(fb, stride, &e.name[..e.len], 40, y0 + 28, [0x1B, 0x25, 0x40, 0]);
    let mut size_line = [0u8; 32];
    let head = b"Size ";
    size_line[..head.len()].copy_from_slice(head);
    let mut n = head.len();
    let mut v = data.size;
    let mut dig = [0u8; 10];
    let mut dn = 0;
    if v == 0 {
        dig[0] = b'0';
        dn = 1;
    }
    while v > 0 && dn < dig.len() {
        dig[dn] = b'0' + (v % 10) as u8;
        v /= 10;
        dn += 1;
    }
    while dn > 0 {
        dn -= 1;
        size_line[n] = dig[dn];
        n += 1;
    }
    text(fb, stride, &size_line[..n], 40, y0 + 104, [0x4F, 0x5D, 0x75, 0]);
    let mut preview = [0u8; 96];
    for (dst, src) in preview.iter_mut().zip(data.bytes[..data.len].iter()) {
        *dst = if src.is_ascii_graphic() || *src == b' ' { *src } else { b'.' };
    }
    text(fb, stride, &preview[..data.len], 40, y0 + 188, [0x21, 0x2B, 0x36, 0]);
}

fn frame(console: Handle, wm: Handle, vmo: Handle, surface: u32, buffer_id: u32) -> u32 {
    let mut alias: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
        fail(console, "frame dup");
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let submit = gp::PresentReq::Submit {
        surface_id: surface,
        buffer_id,
        flags: gp::SUBMIT_F_DAMAGE_FULL,
        rate_hint: 0,
        target_present_ns: 0,
        rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
        rect_count: 0,
    };
    let n = match gp::encode_present_req(&mut req, &submit) {
        Some(n) => n,
        None => fail(console, "submit encode"),
    };
    let (n, _) = wm_rpc(wm, &req[..n], &[alias], &mut rep, &mut rhs);
    let token = match gp::parse_submit_ack(&rep[..n]) {
        Some(t) => t,
        None => fail(console, "submit rep"),
    };
    let n = match gp::encode_present_req(&mut req, &gp::PresentReq::FrameCallback { frame_token: token }) {
        Some(n) => n,
        None => fail(console, "frame encode"),
    };
    let (n, _) = wm_rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    match gp::parse_frame_done_rep(&rep[..n]) {
        Some(done) if done.status == gp::FRAME_PRESENTED => {}
        _ => fail(console, "frame rep"),
    }
    let n = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
        Some(n) => n,
        None => fail(console, "release encode"),
    };
    let (n, nh) = wm_rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    let rel = match gp::parse_release_rep(&rep[..n]) {
        Some(r) => r,
        None => fail(console, "release rep"),
    };
    if rel.count != 1 || nh != 1 {
        fail(console, "release count");
    }
    if berry_handle_close(rhs[0]) != 0 {
        fail(console, "release close");
    }
    token
}

fn wait_action(console: Handle, input: Handle, w: usize, lay: Layout, count: usize) -> Action {
    let mut req = [0u8; 4];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let n = gp::encode_input_poll_req(&mut req, gp::INPUT_EVENTS_MAX as u8);
    loop {
        let (n, _) = wm_rpc(input, &req[..n], &[], &mut rep, &mut rhs);
        match gp::parse_input_events_rep(&rep[..n]) {
            Some(Ok(view)) => {
                for i in 0..view.count() {
                    let Some(ev) = view.event(i) else { continue };
                    if ev.phase != gp::PHASE_UP {
                        continue;
                    }
                    if ev.x < lay.bar as u32 && ev.y < lay.bar as u32 {
                        return Action::Back;
                    }
                    for row in 0..count.min(MAX_ENTRIES) {
                        let y0 = (lay.top + row * lay.row_h) as u32;
                        let y1 = y0 + lay.row_h as u32;
                        if ev.y >= y0 && ev.y < y1 && ev.x < w as u32 {
                            return Action::Open(row);
                        }
                    }
                }
            }
            Some(Err(_)) => {}
            None => fail(console, "poll rep"),
        }
    }
}

#[link_section = ".text.entry"]
#[no_mangle]
pub extern "C" fn _start(env_ch: Handle) -> ! {
    let env = read_env(env_ch);
    let c = env.console;
    send(c, b"FILES up\n");
    let (wm, input, font_vmo) =
        wm_connect(c, env.broker, env.process, env.vmar);

    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let n = gp::encode_wm_getinfo_req(&mut req);
    let (n, _) = wm_rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    let info = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(c, "getinfo rep"),
    };
    let (w, h) = (info.disp_w as usize, info.disp_h as usize);
    let create = gp::WmCreateReq {
        layer: gp::WM_LAYER_APP, flags: 0, parent: 0, group: 0,
        x: 0, y: 0, w: info.disp_w, h: info.disp_h,
    };
    let n = match gp::encode_wm_create_req(&mut req, &create) {
        Some(n) => n,
        None => fail(c, "create encode"),
    };
    let (n, _) = wm_rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    let win = match gp::parse_wm_create_ack(&rep[..n]) {
        Some(id) if id != 0 => id,
        _ => fail(c, "create rep"),
    };
    send(c, b"FILESWIN ok\n");

    let stride = w.checked_mul(4).unwrap_or_else(|| fail(c, "stride"));
    let frame_bytes = stride.checked_mul(h).unwrap_or_else(|| fail(c, "frame size"));
    let len = (frame_bytes + 0xFFF) & !0xFFF;
    let mut vmo: Handle = 0;
    if berry_vmo_create(len as u64, 0, &mut vmo) != 0
        || vmo == 0
        || berry_vmo_commit(vmo, 0, len as u64) != 0
    {
        fail(c, "vmo");
    }
    let mut va = FRAME_VA;
    if berry_vmar_map(env.vmar, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0
        || va != FRAME_VA
    {
        fail(c, "map");
    }
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(FRAME_VA as *mut u8, frame_bytes) };

    let mut entries = [FileEntry { name: [0u8; NAME_MAX], len: 0 }; MAX_ENTRIES];
    let count = readdir_root(c, env.fs, &mut entries);
    send(c, b"FILESLIST ok n=\n");
    let selected = (0..count)
        .find(|&i| &entries[i].name[..entries[i].len] == b"HELLO.TXT")
        .unwrap_or(0);
    let mut marker = *b"FILESSEL ok i=0\n";
    marker[14] = b'0' + selected as u8;
    send(c, &marker);

    let lay = layout(w, h);
    let mut mode = Mode::List;
    let mut data: FileData;
    render_list(fb, stride, w, h, lay, &entries, count, selected);
    let _ = frame(c, wm, vmo, u32::from(win), 1);
    send(c, b"FILESFRAME ok\n");
    if env.boot_probe {
        send(c, b"FILESPROBE ok\n");
    }

    let mut buffer_id = 2u32;
    if !env.boot_probe {
        loop {
            match wait_action(c, input, w, lay, count) {
                Action::Back => {
                    if let Mode::Detail = mode {
                        mode = Mode::List;
                        render_list(fb, stride, w, h, lay, &entries, count, selected);
                        let _ = frame(c, wm, vmo, u32::from(win), buffer_id);
                        buffer_id += 1;
                        send(c, b"FILESFRAME ok\n");
                        send(c, b"FILESLIST ok n=\n");
                    } else {
                        send(c, b"FILESBACK ok\n");
                        break;
                    }
                }
                Action::Open(index) => {
                    data = get_file(c, env.fs, &entries[index]);
                    data.index = index;
                    send(c, b"FILESOPEN ok\n");
                    let mut line = *b"FILESSTAT ok size=0\n";
                    let mut v = data.size;
                    let mut dig = [0u8; 10];
                    let mut dn = 0;
                    if v == 0 {
                        dig[0] = b'0';
                        dn = 1;
                    }
                    while v > 0 && dn < dig.len() {
                        dig[dn] = b'0' + (v % 10) as u8;
                        v /= 10;
                        dn += 1;
                    }
                    let mut pos = line.len() - 2;
                    while dn > 0 {
                        dn -= 1;
                        line[pos] = dig[dn];
                        pos += 1;
                    }
                    send(c, &line);
                    let mut read_line = *b"FILESREAD ok n=0\n";
                    let mut v = data.len as u32;
                    let mut dn = 0;
                    if v == 0 {
                        dig[0] = b'0';
                        dn = 1;
                    }
                    while v > 0 && dn < dig.len() {
                        dig[dn] = b'0' + (v % 10) as u8;
                        v /= 10;
                        dn += 1;
                    }
                    let mut pos = read_line.len() - 2;
                    while dn > 0 {
                        dn -= 1;
                        read_line[pos] = dig[dn];
                        pos += 1;
                    }
                    send(c, &read_line);
                    mode = Mode::Detail;
                    render_detail(fb, stride, w, h, lay, &entries, &data);
                    let _ = frame(c, wm, vmo, u32::from(win), buffer_id);
                    buffer_id += 1;
                    send(c, b"FILESFRAME ok\n");
                }
            }
        }
    }

    let n = gp::encode_wm_destroy_req(&mut req, win);
    let (n, _) = wm_rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(gp::ST_OK) => {}
        _ => fail(c, "destroy rep"),
    }
    let n = gp::encode_wm_table_get_req(&mut req);
    let (n, _) = wm_rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    match gp::parse_wm_snapshot_rep(&rep[..n]) {
        Some(Ok(s)) if s.count == 0 => {}
        _ => fail(c, "table rep"),
    }
    send(c, b"FILESDESTROY ok\n");
    let _ = berry_handle_close(input);
    let _ = berry_handle_close(wm);
    let _ = berry_handle_close(vmo);
    let _ = berry_handle_close(font_vmo);
    send(c, b"FILESEXIT ok\n");
    berry_process_exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_process_exit(134)
}
