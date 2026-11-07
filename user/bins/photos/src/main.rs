//! Photos Native Shell —— 真实 fsmgr 图像数据 + 独立 GUI 进程。
//!
//! 链路：`wm=true` + `fs=["rootfs"]` → APP_SPAWN 交付 [rootfs VMO, fs
//! client, WMD broker]。READDIR 根目录后 STAT/GET PATTERN.BIN 5000B，
//! 逐字节验证确定性公式，再渲染成灰度缩略图/大图。v1 只读样本浏览。

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

const PATTERN_LEN: usize = 5000;
const SRC_W: usize = 50;
const SRC_H: usize = 100;
const TILE_SRC: usize = 16;

#[derive(Clone, Copy)]
enum Mode {
    Grid,
    View,
}

#[derive(Clone, Copy)]
enum Action {
    Open,
    Back,
}

static mut PATTERN: [u8; PATTERN_LEN] = [0u8; PATTERN_LEN];

struct PhotoLayout {
    bar: usize,
    tile_x: usize,
    tile_y: usize,
    tile_w: usize,
    view_x: usize,
    view_y: usize,
    view_scale: usize,
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
        || nb <= b"bin/photos".len()
        || &blob[..b"bin/photos".len()] != b"bin/photos"
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

fn fs_call(fs: Handle, req: &[u8], rep: &mut [u8; 256]) -> usize {
    channel_roundtrip(fs, req, rep)
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

fn stat_pattern(console: Handle, fs: Handle) -> Option<u32> {
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let n = match fp::encode_stat(&mut req, b"PATTERN.BIN") {
        Some(n) => n,
        None => fail(console, "stat encode"),
    };
    let n = fs_call(fs, &req[..n], &mut rep);
    match fp::parse_reply_stat(&rep[..n]) {
        Some(Ok((ty, size))) if ty == fp::TYPE_FILE => Some(size),
        // frozen smoke-image 无数据盘：PATTERN 缺席是拓扑真值，boot-probe
        // 走本进程公式样本；其它错误仍 fail loud。
        Some(Err(fp::ST_NOENT)) => None,
        _ => fail(console, "stat rep"),
    }
}

fn read_pattern(console: Handle, fs: Handle, out: &mut [u8; PATTERN_LEN]) -> (usize, bool) {
    let Some(size) = stat_pattern(console, fs) else {
        for (i, dst) in out.iter_mut().enumerate() {
            *dst = ((i * 31 + 7) % 256) as u8;
        }
        return (PATTERN_LEN, false);
    };
    if size as usize != PATTERN_LEN {
        fail(console, "pattern size");
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut off = 0usize;
    loop {
        let n = match fp::encode_get(&mut req, b"PATTERN.BIN", off as u32) {
            Some(n) => n,
            None => fail(console, "get encode"),
        };
        let n = fs_call(fs, &req[..n], &mut rep);
        let got = match fp::parse_reply_get(&rep[..n]) {
            Some(Ok(g)) => g,
            _ => fail(console, "get rep"),
        };
        let take = got.data.len();
        if off + take > PATTERN_LEN {
            fail(console, "pattern long");
        }
        out[off..off + take].copy_from_slice(got.data);
        off += take;
        if !got.more {
            break;
        }
    }
    if off != PATTERN_LEN {
        fail(console, "pattern short");
    }
    for (i, &b) in out.iter().enumerate() {
        if usize::from(b) != (i * 31 + 7) % 256 {
            fail(console, "pattern byte");
        }
    }
    (off, true)
}

fn photo_layout(w: usize, h: usize) -> PhotoLayout {
    let bar = (h / 12).max(64).min(160);
    let tile_w = (w / 3).min((h - bar) / 4).max(96);
    let tile_x = (w.saturating_sub(tile_w)) / 2;
    let tile_y = bar + (h.saturating_sub(bar) / 6);
    let view_scale = ((w - 120) / SRC_W).min((h - bar - 60) / SRC_H).max(2);
    let view_w = SRC_W * view_scale;
    let view_h = SRC_H * view_scale;
    PhotoLayout {
        bar,
        tile_x,
        tile_y,
        tile_w,
        view_x: (w.saturating_sub(view_w)) / 2,
        view_y: bar + ((h.saturating_sub(bar) - view_h) / 2),
        view_scale,
    }
}

fn draw_pattern_tile(
    fb: &mut [u8], stride: usize, data: &[u8; PATTERN_LEN],
    x0: usize, y0: usize, size: usize,
) {
    let scale = size / TILE_SRC;
    for y in 0..size {
        let sy = (y / scale).min(TILE_SRC - 1);
        for x in 0..size {
            let sx = (x / scale).min(TILE_SRC - 1);
            let v = data[sy * TILE_SRC + sx];
            put_px(fb, stride, x0 + x, y0 + y, [v, v, v, 0]);
        }
    }
}

fn draw_pattern_view(
    fb: &mut [u8], stride: usize, data: &[u8; PATTERN_LEN],
    x0: usize, y0: usize, scale: usize,
) {
    for y in 0..SRC_H * scale {
        let sy = y / scale;
        for x in 0..SRC_W * scale {
            let sx = x / scale;
            let v = data[sy * SRC_W + sx];
            put_px(fb, stride, x0 + x, y0 + y, [v, v, v, 0]);
        }
    }
}

fn render_grid(
    fb: &mut [u8], stride: usize, w: usize, h: usize,
    lay: &PhotoLayout, data: &[u8; PATTERN_LEN], count: usize,
) {
    fill(fb, stride, 0, 0, w, h, [0xF7, 0xF9, 0xFC, 0]);
    fill(fb, stride, 0, 0, w, lay.bar, [0x5E, 0x35, 0xB1, 0]);
    let scale = if w >= 1000 { 4 } else { 3 };
    text(fb, stride, b"Photos", lay.bar + 18, (lay.bar - 16 * scale) / 2, scale, [0xFF, 0xFF, 0xFF, 0]);
    let cx = lay.bar / 2;
    let r = lay.bar.saturating_sub(18) / 2;
    for y in 0..lay.bar {
        for x in 0..lay.bar {
            let dx = x as i32 - cx as i32;
            let dy = y as i32 - cx as i32;
            if dx * dx + dy * dy <= r as i32 * r as i32 {
                put_px(fb, stride, x, y, [0xEF, 0xE7, 0xFF, 0]);
            }
        }
    }
    draw_pattern_tile(fb, stride, data, lay.tile_x, lay.tile_y, lay.tile_w);
    let ty = lay.tile_y + lay.tile_w + 12;
    text(fb, stride, b"PATTERN.BIN", lay.tile_x, ty, 3, [0x33, 0x3B, 0x4F, 0]);
    let mut line = *b"Items 0";
    line[6] = b'0' + count as u8;
    text(fb, stride, &line, lay.tile_x, ty + 28, 2, [0x69, 0x74, 0x88, 0]);
}

fn render_view(
    fb: &mut [u8], stride: usize, w: usize, h: usize,
    lay: &PhotoLayout, data: &[u8; PATTERN_LEN],
) {
    render_grid(fb, stride, w, h, lay, data, 1);
    // 覆盖网格主体，保留同一顶栏与返回键。
    fill(fb, stride, 0, lay.bar, w, h.saturating_sub(lay.bar), [0, 0, 0, 0]);
    draw_pattern_view(
        fb, stride, data, lay.view_x, lay.view_y, lay.view_scale,
    );
    let ty = lay.bar + 12;
    text(fb, stride, b"PATTERN.BIN 50x100", 40, ty, 3, [0xFF, 0xFF, 0xFF, 0]);
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


fn wait_action(console: Handle, input: Handle, lay: &PhotoLayout) -> Action {
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
                    if x >= lay.tile_x
                        && x < lay.tile_x + lay.tile_w
                        && y >= lay.tile_y
                        && y < lay.tile_y + lay.tile_w
                    {
                        return Action::Open;
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
    send(c, b"PHOTOS up\n");
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
    send(c, b"PHOTOSWIN ok\n");

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

    let count = readdir_count(c, env.fs);
    // 5000B 照片数据入 BSS：栈上数组会推高 main 帧并触发保护页 D-abort。
    // 单线程 Native Shell，独占 mutable static 是显式所有权面。
    let pattern: &mut [u8; PATTERN_LEN] =
        unsafe { &mut *core::ptr::addr_of_mut!(PATTERN) };
    let (nread, remote) = read_pattern(c, env.fs, pattern);
    if nread != PATTERN_LEN {
        fail(c, "pattern final");
    }
    send(c, b"PHOTOSLIST ok n=\n");
    if remote {
        send(c, b"PHOTOSDATA ok n=5000\n");
    } else {
        send(c, b"PHOTOSLOCAL ok n=5000\n");
    }

    let lay = photo_layout(w, h);
    let mut mode = Mode::Grid;
    render_grid(fb, stride, w, h, &lay, &pattern, count);
    let _ = frame(c, wm, vmo, u32::from(win), 1);
    send(c, b"PHOTOSFRAME ok\n");
    if env.boot_probe {
        send(c, b"PHOTOSPROBE ok\n");
    }

    let mut buffer_id = 2u32;
    if !env.boot_probe {
        loop {
            match wait_action(c, input, &lay) {
                Action::Open => {
                    send(c, b"PHOTOSOPEN ok\n");
                    mode = Mode::View;
                    render_view(fb, stride, w, h, &lay, &pattern);
                    let _ = frame(c, wm, vmo, u32::from(win), buffer_id);
                    buffer_id += 1;
                    send(c, b"PHOTOSVIEW ok\n");
                    send(c, b"PHOTOSFRAME ok\n");
                }
                Action::Back => {
                    if let Mode::View = mode {
                        mode = Mode::Grid;
                        render_grid(fb, stride, w, h, &lay, &pattern, count);
                        let _ = frame(c, wm, vmo, u32::from(win), buffer_id);
                        buffer_id += 1;
                        send(c, b"PHOTOSFRAME ok\n");
                        send(c, b"PHOTOSLIST ok n=\n");
                    } else {
                        send(c, b"PHOTOSBACK ok\n");
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
    send(c, b"PHOTOSDESTROY ok\n");
    let _ = berry_handle_close(input);
    let _ = berry_handle_close(wm);
    let _ = berry_handle_close(vmo);
    send(c, b"PHOTSEXIT ok\n");
    berry_process_exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_process_exit(134)
}
