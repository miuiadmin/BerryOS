//! Browser Native Shell —— 独立 GUI 进程（诚实边界：无网络 / 无 HTML 引擎）。
//!
//! 真链：APP_SPAWN → manifest `wm=true` → WMD broker fresh dup → 本进程
//! 自建 WM/RPC + input 两根 session 通道 → WM_CONNECT → 全屏窗口 →
//! 窗口局部 VMO → wmd composition SUBMIT。返回键点击走 wmd owner 路由，
//! 收到 window-local UP 后显式 DESTROY 并回读空表。

#![no_std]
#![no_main]

use berry_guiproto as gp;
use berry_nshfont::{circle, map as map_font, round_rect, shadow, text};
use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_clock_read,
    berry_handle_close, berry_handle_duplicate, berry_object_wait_one, berry_process_exit,
    berry_vmar_map, berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN,
    BERRY_ERR_BUSY, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_WRITE, SIGNAL_READABLE,
    SIGNAL_WRITABLE,
};

const FRAME_VA: usize = 0x5200_0000;
const RPC_CAP_NS: u64 = 120 * 1_000_000_000;

struct Env {
    boot_probe: bool,
    console: Handle,
    process: Handle,
    vmar: Handle,
    broker: Handle,
}

fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        berry_process_exit(134);
    }
    t
}

fn fail(console: Handle, what: &str) -> ! {
    let mut line = [0u8; 96];
    let head = b"browser: ";
    line[..head.len()].copy_from_slice(head);
    let n = (what.len()).min(line.len() - head.len());
    line[head.len()..head.len() + n].copy_from_slice(&what.as_bytes()[..n]);
    send(console, &line[..head.len() + n]);
    berry_process_exit(134)
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

fn read_env(env_ch: Handle) -> Env {
    let mut blob = [0u8; 192];
    let mut hs = [0 as Handle; 8];
    let mut nb = blob.len();
    let mut nh = hs.len();
    if berry_channel_recv(env_ch, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh) != 0
        || nh < 5
        || nb <= b"bin/browser".len()
        || &blob[..b"bin/browser".len()] != b"bin/browser"
        || hs[2] == 0
        || hs[3] == 0
        || hs[4] == 0
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
        broker: hs[4],
    }
}

fn rpc(ch: Handle, req: &[u8], attach: &[Handle], rep: &mut [u8; 256], rhs: &mut [Handle; 8]) -> (usize, usize) {
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
    let (n, nh) = rpc(broker, &req[..n], &attach, &mut rep, &mut rhs);
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

fn put_px(fb: &mut [u8], stride: usize, x: usize, y: usize, c: [u8; 4]) {
    let off = y * stride + x * 4;
    // Rgbx 像素 u32 = r<<16|g<<8|b；小端内存字节序即 [b,g,r,x]。
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

fn render(fb: &mut [u8], stride: usize, w: usize, h: usize) -> usize {
    let bar = (h / 12).max(64).min(160);
    fill(fb, stride, 0, 0, w, bar, [0x10, 0x74, 0xE5, 0]);
    // 返回键：圆底 + 左三角。
    let cx = bar / 2;
    let r = bar.saturating_sub(18) / 2;
    circle(fb, stride, cx, cx, r, [0xE8, 0xF2, 0xFF]);
    for y in 0..(r / 2) {
        let half = y as i32 - r as i32 / 4;
        for dx in 0..(r / 2) {
            let x = cx - r / 4 + dx as usize;
            let yy = cx as i32 + half;
            if yy >= 0 && yy < bar as i32 && x < bar {
                put_px(fb, stride, x, yy as usize, [0x0B, 0x57, 0xA4, 0]);
            }
        }
    }
    text(fb, stride, b"Browser", bar + 18, (bar - 48) / 2, [0xFF, 0xFF, 0xFF, 0]);
    // 地址栏与诚实状态页：OH 风格圆角 URL pill + 内容卡片。
    let url_h = bar;
    fill(fb, stride, 0, bar, w, url_h, [0xEC, 0xF4, 0xFC, 0]);
    round_rect(
        fb, stride, 20, bar + 10, w - 40, url_h - 20,
        (url_h - 20) / 2, [0xFF, 0xFF, 0xFF],
    );
    text(fb, stride, b"berry://native-shell", 48, bar + (url_h - 48) / 2, [0x24, 0x2D, 0x37, 0]);
    fill(fb, stride, 0, bar + url_h, w, h - bar - url_h, [0xF2, 0xF6, 0xFB, 0]);
    let card_y = bar + url_h + 36;
    let card_h = h.saturating_sub(card_y + 40);
    shadow(fb, stride, 36, card_y, w - 72, card_h, 56);
    round_rect(fb, stride, 32, card_y, w - 64, card_h, 56, [0xFF, 0xFF, 0xFF]);
    text(fb, stride, b"Native shell ready", 76, card_y + 54, [0x12, 0x34, 0x5C, 0]);
    text(fb, stride, b"No web engine in v1", 76, card_y + 144, [0x5F, 0x6E, 0x81, 0]);
    bar
}

fn frame(console: Handle, wm: Handle, vmo: Handle, surface: u32) -> u32 {
    let mut alias: Handle = 0;
    if berry_handle_duplicate(vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
        fail(console, "frame dup");
    }
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let submit = gp::PresentReq::Submit {
        surface_id: surface,
        buffer_id: 1,
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
    let (n, _) = rpc(wm, &req[..n], &[alias], &mut rep, &mut rhs);
    let token = match gp::parse_submit_ack(&rep[..n]) {
        Some(t) => t,
        None => fail(console, "submit rep"),
    };
    let n = match gp::encode_present_req(&mut req, &gp::PresentReq::FrameCallback { frame_token: token }) {
        Some(n) => n,
        None => fail(console, "frame encode"),
    };
    let (n, _) = rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    match gp::parse_frame_done_rep(&rep[..n]) {
        Some(done) if done.status == gp::FRAME_PRESENTED => {}
        _ => fail(console, "frame rep"),
    }
    let n = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
        Some(n) => n,
        None => fail(console, "release encode"),
    };
    let (n, nh) = rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
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

fn wait_back(console: Handle, input: Handle, bar: usize) {
    let mut req = [0u8; 4];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let n = gp::encode_input_poll_req(&mut req, gp::INPUT_EVENTS_MAX as u8);
    loop {
        let (n, _) = rpc(input, &req[..n], &[], &mut rep, &mut rhs);
        match gp::parse_input_events_rep(&rep[..n]) {
            Some(Ok(view)) => {
                for i in 0..view.count() {
                    let Some(ev) = view.event(i) else { continue };
                    if ev.phase == gp::PHASE_UP && ev.x < bar as u32 && ev.y < bar as u32 {
                        send(console, b"BROWSERBACK ok\n");
                        return;
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
    send(c, b"BROWSER up\n");
    let (wm, input, font_vmo) =
        wm_connect(c, env.broker, env.process, env.vmar);

    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let n = gp::encode_wm_getinfo_req(&mut req);
    let (n, _) = rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    let info = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(c, "getinfo rep"),
    };
    let (w, h) = (info.disp_w as usize, info.disp_h as usize);
    let create = gp::WmCreateReq {
        layer: gp::WM_LAYER_APP,
        flags: 0,
        parent: 0,
        group: 0,
        x: 0,
        y: 0,
        w: info.disp_w,
        h: info.disp_h,
    };
    let n = match gp::encode_wm_create_req(&mut req, &create) {
        Some(n) => n,
        None => fail(c, "create encode"),
    };
    let (n, _) = rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    let win = match gp::parse_wm_create_ack(&rep[..n]) {
        Some(id) if id != 0 => id,
        _ => fail(c, "create rep"),
    };
    send(c, b"BROWSERWIN ok\n");

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
    let bar = render(fb, stride, w, h);
    let token = frame(c, wm, vmo, u32::from(win));
    send(c, b"BROWSERFRAME ok\n");
    let _ = token;

    if env.boot_probe {
        send(c, b"BROWSERPROBE ok\n");
    } else {
        wait_back(c, input, bar);
    }
    let n = gp::encode_wm_destroy_req(&mut req, win);
    let (n, _) = rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    match gp::parse_status_rep(&rep[..n]) {
        Some(gp::ST_OK) => {}
        _ => fail(c, "destroy rep"),
    }
    let n = gp::encode_wm_table_get_req(&mut req);
    let (n, _) = rpc(wm, &req[..n], &[], &mut rep, &mut rhs);
    match gp::parse_wm_snapshot_rep(&rep[..n]) {
        Some(Ok(s)) if s.count == 0 => {}
        _ => fail(c, "table rep"),
    }
    send(c, b"BROWSERDESTROY ok\n");
    let _ = berry_handle_close(input);
    let _ = berry_handle_close(wm);
    let _ = berry_handle_close(vmo);
    let _ = berry_handle_close(font_vmo);
    send(c, b"BROWSEREXIT ok\n");
    berry_process_exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_process_exit(134)
}
