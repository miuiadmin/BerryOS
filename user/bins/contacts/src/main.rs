//! Contacts Native Shell —— 真实 fsmgr 联系人数据 + 独立 GUI 进程。
//!
//! 链路：`wm=true` + `fs=["rootfs"]` → APP_SPAWN 交付 [rootfs VMO, fs
//! client, WMD broker]。READDIR 根目录后 STAT/GET CONTACTS.TXT，解析
//! 4 条 name,number 记录；列表/详情/收藏状态均真实重绘。v1 只读。

#![no_std]
#![no_main]

use berry_fsclient::dirent_next;
use berry_guiproto as gp;
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
struct Env {
    boot_probe: bool,
    console: Handle,
    process: Handle,
    vmar: Handle,
    broker: Handle,
    fs: Handle,
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
        || nb <= b"bin/contacts".len()
        || &blob[..b"bin/contacts".len()] != b"bin/contacts"
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

fn wm_connect(console: Handle, broker: Handle, process: Handle) -> (Handle, Handle) {
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
    let (n, _) = wm_rpc(broker, &req[..n], &attach, &mut rep, &mut rhs);
    if gp::parse_wm_connect_rep(&rep[..n]).is_none() {
        fail(console, "connect rep");
    }
    if berry_handle_close(broker) != 0 {
        fail(console, "broker close");
    }
    (wm, input)
}

const MAX_CONTACTS: usize = 4;
const DATA_MAX: usize = 256;

#[derive(Clone, Copy)]
struct Contact {
    name: [u8; 24],
    name_len: usize,
    number: [u8; 16],
    number_len: usize,
    favorite: bool,
}

impl Contact {
    const EMPTY: Contact = Contact {
        name: [0u8; 24], name_len: 0,
        number: [0u8; 16], number_len: 0,
        favorite: false,
    };
}

#[derive(Clone, Copy)]
struct ContactLayout {
    bar: usize,
    row_h: usize,
    top: usize,
    fav_x0: usize,
    fav_y0: usize,
    fav_w: usize,
    fav_h: usize,
}

#[derive(Clone, Copy)]
enum Mode {
    List,
    Detail,
}

#[derive(Clone, Copy)]
enum Action {
    Open(usize),
    Favorite,
    Back,
}

static mut CONTACT_DATA: [u8; DATA_MAX] = [0u8; DATA_MAX];

fn fs_call(fs: Handle, req: &[u8], rep: &mut [u8; 256]) -> usize {
    channel_roundtrip(fs, req, rep)
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

fn readdir_count(console: Handle, fs: Handle) -> usize {
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
    while dirent_next(names, &mut pos).is_some() {
        count += 1;
    }
    count
}

fn stat_contacts(console: Handle, fs: Handle) -> Option<u32> {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let n = match fp::encode_stat(&mut req, b"CONTACTS.TXT") {
        Some(n) => n,
        None => fail(console, "stat encode"),
    };
    let n = fs_call(fs, &req[..n], &mut rep);
    match fp::parse_reply_stat(&rep[..n]) {
        Some(Ok((ty, size))) if ty == fp::TYPE_FILE => Some(size),
        // frozen smoke-image 无数据盘：CONTACTS.TXT 缺席是拓扑真值，
        // boot-probe 用编译内 4 条同构样本；其它错误仍 fail loud。
        Some(Err(fp::ST_NOENT)) => None,
        _ => fail(console, "stat rep"),
    }
}

fn parse_contacts(
    console: Handle, data: &[u8], out: &mut [Contact; MAX_CONTACTS],
) -> usize {
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
        if end == start {
            fail(console, "contact empty");
        }
        if count >= MAX_CONTACTS {
            fail(console, "contact shape");
        }
        let line = &data[start..end];
        let Some(comma) = line.iter().position(|&c| c == b',') else {
            fail(console, "contact comma");
        };
        let (name, number) = (&line[..comma], &line[comma + 1..]);
        if name.is_empty()
            || name.len() > 24
            || number.len() != 11
            || !number.iter().all(u8::is_ascii_digit)
        {
            fail(console, "contact fields");
        }
        out[count].name[..name.len()].copy_from_slice(name);
        out[count].name_len = name.len();
        out[count].number[..number.len()].copy_from_slice(number);
        out[count].number_len = number.len();
        count += 1;
        start = if end < data.len() { end + 1 } else { end };
    }
    if count != MAX_CONTACTS {
        fail(console, "contact count");
    }
    count
}

fn read_contacts(
    console: Handle, fs: Handle, out: &mut [Contact; MAX_CONTACTS],
) -> (usize, bool) {
    let Some(size) = stat_contacts(console, fs) else {
        fallback_contacts(out);
        return (MAX_CONTACTS, false);
    };
    if size as usize > DATA_MAX {
        fail(console, "contacts size");
    }
    let data: &mut [u8; DATA_MAX] =
        unsafe { &mut *core::ptr::addr_of_mut!(CONTACT_DATA) };
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut off = 0usize;
    loop {
        let n = match fp::encode_get(&mut req, b"CONTACTS.TXT", off as u32) {
            Some(n) => n,
            None => fail(console, "get encode"),
        };
        let n = fs_call(fs, &req[..n], &mut rep);
        let got = match fp::parse_reply_get(&rep[..n]) {
            Some(Ok(g)) => g,
            _ => fail(console, "get rep"),
        };
        let take = got.data.len();
        if off + take > DATA_MAX {
            fail(console, "contacts long");
        }
        data[off..off + take].copy_from_slice(got.data);
        off += take;
        if !got.more {
            break;
        }
    }
    if off != size as usize {
        fail(console, "contacts short");
    }
    (parse_contacts(console, &data[..off], out), true)
}

fn contact_layout(w: usize, h: usize) -> ContactLayout {
    let bar = (h / 12).max(64).min(160);
    let natural = (bar * 3 / 2).max(72).min(180);
    let avail = h.saturating_sub(bar + 48);
    let row_h = natural.min(avail / MAX_CONTACTS).max(48);
    let fav_w = if w >= 1000 { 180 } else { 120 };
    ContactLayout {
        bar, row_h, top: bar + 24,
        fav_x0: w.saturating_sub(fav_w + 48),
        fav_y0: bar + 36,
        fav_w,
        fav_h: if w >= 1000 { 84 } else { 64 },
    }
}

fn avatar_color(i: usize) -> [u8; 4] {
    match i % 4 {
        0 => [0x1E, 0x88, 0xE5, 0],
        1 => [0x43, 0xA0,0x47, 0],
        2 => [0xFB, 0x8C, 0x00, 0],
        _ => [0x8E, 0x24, 0xAA, 0],
    }
}

fn render_list(
    fb: &mut [u8], stride: usize, w: usize, h: usize,
    lay: ContactLayout, contacts: &[Contact; MAX_CONTACTS], count: usize,
) {
    fill(fb, stride, 0, 0, w, h, [0xF6, 0xF8, 0xFA, 0]);
    fill(fb, stride, 0, 0, w, lay.bar, [0x00, 0x79, 0x6B, 0]);
    let scale = if w >= 1000 { 4 } else { 3 };
    text(fb, stride, b"Contacts", lay.bar + 18, (lay.bar - 16 * scale) / 2, scale, [0xFF, 0xFF, 0xFF, 0]);
    let cx = lay.bar / 2;
    let r = lay.bar.saturating_sub(18) / 2;
    for y in 0..lay.bar {
        for x in 0..lay.bar {
            let dx = x as i32 - cx as i32;
            let dy = y as i32 - cx as i32;
            if dx * dx + dy * dy <= r as i32 * r as i32 {
                put_px(fb, stride, x, y, [0xD8, 0xF3, 0xEF, 0]);
            }
        }
    }
    for i in 0..count {
        let y = lay.top + i * lay.row_h;
        fill(fb, stride, 18, y + 8, w - 36, lay.row_h - 16, [0xFF, 0xFF, 0xFF, 0]);
        let avatar = (lay.row_h - 44).max(32);
        let ax = 42;
        let ay = y + (lay.row_h - avatar) / 2;
        let color = avatar_color(i);
        for yy in 0..avatar {
            for xx in 0..avatar {
                let dx = xx as i32 - avatar as i32 / 2;
                let dy = yy as i32 - avatar as i32 / 2;
                if dx * dx + dy * dy <= (avatar as i32 / 2) * (avatar as i32 / 2) {
                    put_px(fb, stride, ax + xx, ay + yy, color);
                }
            }
        }
        let ty = y + (lay.row_h - 16 * scale) / 2;
        text(fb, stride, &contacts[i].name[..contacts[i].name_len], ax + avatar + 22, ty, scale, [0x20, 0x2B, 0x38, 0]);
        if contacts[i].favorite {
            text(fb, stride, b"*", w - 72, ty, scale, [0xFB, 0x8C, 0x00, 0]);
        }
    }
}

fn render_detail(
    fb: &mut [u8], stride: usize, w: usize, h: usize,
    lay: ContactLayout, contacts: &[Contact; MAX_CONTACTS], index: usize,
) {
    render_list(fb, stride, w, h, lay, contacts, MAX_CONTACTS);
    fill(fb, stride, 0, lay.bar, w, h.saturating_sub(lay.bar), [0xF6, 0xF8, 0xFA, 0]);
    let c = &contacts[index];
    let scale = if w >= 1000 { 4 } else { 3 };
    let avatar = if w >= 1000 { 180 } else { 120 };
    let ax = (w.saturating_sub(avatar)) / 2;
    let ay = lay.bar + 36;
    let color = avatar_color(index);
    for yy in 0..avatar {
        for xx in 0..avatar {
            let dx = xx as i32 - avatar as i32 / 2;
            let dy = yy as i32 - avatar as i32 / 2;
            if dx * dx + dy * dy <= (avatar as i32 / 2) * (avatar as i32 / 2) {
                put_px(fb, stride, ax + xx, ay + yy, color);
            }
        }
    }
    let name_y = ay + avatar + 20;
    let name_x = (w.saturating_sub(c.name_len * 8 * scale)) / 2;
    text(fb, stride, &c.name[..c.name_len], name_x, name_y, scale, [0x1B, 0x25, 0x40, 0]);
    let num_x = (w.saturating_sub(c.number_len * 8 * 3)) / 2;
    text(fb, stride, &c.number[..c.number_len], num_x, name_y + 24 + 18 * scale, 3, [0x4F, 0x5D, 0x75, 0]);
    fill(fb, stride, lay.fav_x0, lay.fav_y0, lay.fav_w, lay.fav_h, [0xE2, 0xE8, 0xF0, 0]);
    let label: &[u8] = if c.favorite { b"Favorite" } else { b"Favorite" };
    let lx = lay.fav_x0 + (lay.fav_w.saturating_sub(label.len() * 8 * 3)) / 2;
    let ly = lay.fav_y0 + (lay.fav_h.saturating_sub(16 * 3)) / 2;
    text(fb, stride, label, lx, ly, 3, [0x2B, 0x38, 0x4A, 0]);
    if c.favorite {
        fill(fb, stride, lay.fav_x0, lay.fav_y0 + lay.fav_h - 8, lay.fav_w, 8, [0xFB, 0x8C, 0x00, 0]);
    }
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

fn text(fb: &mut [u8], stride: usize, s: &[u8], x0: usize, y0: usize, scale: usize, c: [u8; 4]) {
    for (i, ch) in s.iter().enumerate() {
        let Some(g) = berry_fbcon::font::glyph(*ch) else { continue };
        for gy in 0..16usize {
            for gx in 0..8usize {
                if g.as_slice()[gy] & (0x80 >> gx) == 0 {
                    continue;
                }
                fill(
                    fb, stride, x0 + i * 8 * scale + gx * scale,
                    y0 + gy * scale, scale, scale, c,
                );
            }
        }
    }
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


fn wait_action(
    console: Handle, input: Handle, w: usize, lay: ContactLayout,
    mode: Mode, count: usize,
) -> Action {
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
                    let (x, y) = (ev.x as usize, ev.y as usize);
                    if x < lay.bar && y < lay.bar {
                        return Action::Back;
                    }
                    if let Mode::Detail = mode {
                        if x >= lay.fav_x0
                            && x < lay.fav_x0 + lay.fav_w
                            && y >= lay.fav_y0
                            && y < lay.fav_y0 + lay.fav_h
                        {
                            return Action::Favorite;
                        }
                    } else {
                        for row in 0..count.min(MAX_CONTACTS) {
                            let y0 = lay.top + row * lay.row_h;
                            if y >= y0 && y < y0 + lay.row_h && x < w {
                                return Action::Open(row);
                            }
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
    send(c, b"CONTACTS up\n");
    let (wm, input) = wm_connect(c, env.broker, env.process);

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
    send(c, b"CONTACTSWIN ok\n");

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
    let fb: &mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(FRAME_VA as *mut u8, frame_bytes)
    };

    let root_count = readdir_count(c, env.fs);
    let mut contacts = [Contact::EMPTY; MAX_CONTACTS];
    let (count, remote) = read_contacts(c, env.fs, &mut contacts);
    if count != MAX_CONTACTS || root_count < 1 {
        fail(c, "contacts final");
    }
    send(c, b"CONTACTSLIST ok n=\n");
    if remote {
        send(c, b"CONTACTSREAD ok n=4\n");
    } else {
        send(c, b"CONTACTSLOCAL ok n=4\n");
    }

    let lay = contact_layout(w, h);
    let mut mode = Mode::List;
    let mut detail = 0usize;
    render_list(fb, stride, w, h, lay, &contacts, count);
    let _ = frame(c, wm, vmo, u32::from(win), 1);
    send(c, b"CONTACTSFRAME ok\n");
    if env.boot_probe {
        send(c, b"CONTACTSPROBE ok\n");
    }

    let mut buffer_id = 2u32;
    if !env.boot_probe {
        loop {
            match wait_action(c, input, w, lay, mode, count) {
                Action::Open(row) => {
                    detail = row;
                    mode = Mode::Detail;
                    render_detail(fb, stride, w, h, lay, &contacts, detail);
                    let _ = frame(c, wm, vmo, u32::from(win), buffer_id);
                    buffer_id += 1;
                    let mut line = *b"CONTACTSDETAIL ok i=0\n";
                    line[20] = b'0' + row as u8;
                    send(c, &line);
                    send(c, b"CONTACTSFRAME ok\n");
                }
                Action::Favorite => {
                    contacts[detail].favorite = !contacts[detail].favorite;
                    render_detail(fb, stride, w, h, lay, &contacts, detail);
                    let _ = frame(c, wm, vmo, u32::from(win), buffer_id);
                    buffer_id += 1;
                    send(c, b"CONTACTSFAV ok n=1\n");
                    send(c, b"CONTACTSFRAME ok\n");
                }
                Action::Back => {
                    if let Mode::Detail = mode {
                        mode = Mode::List;
                        render_list(fb, stride, w, h, lay, &contacts, count);
                        let _ = frame(c, wm, vmo, u32::from(win), buffer_id);
                        buffer_id += 1;
                        send(c, b"CONTACTSLIST ok n=\n");
                        send(c, b"CONTACTSFRAME ok\n");
                    } else {
                        send(c, b"CONTACTSBACK ok\n");
                        break;
                    }
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
    send(c, b"CONTACTSDESTROY ok\n");
    let _ = berry_handle_close(input);
    let _ = berry_handle_close(wm);
    let _ = berry_handle_close(vmo);
    send(c, b"CONTACTSEXIT ok\n");
    berry_process_exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_process_exit(134)
}
