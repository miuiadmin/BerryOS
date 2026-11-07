//! Settings Native Shell —— 独立 GUI 进程与真实 owner 输入状态机。
//!
//! 链路与 Browser Native Shell 同族：`wm=true` → WMD broker → 自建
//! WM/input session → 全屏窗口 → 局部 VMO → wmd composition。区别在
//! runtime 不是单返回键：WLAN/Bluetooth 两行接收 window-local UP，切换
//! 进程内状态并重绘新帧；v1 不声称驱动或持久化已接入。

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
const ROWS: [&[u8]; 6] = [
    b"WLAN", b"Bluetooth", b"Display", b"Sounds", b"Battery", b"About phone",
];

struct Env {
    boot_probe: bool,
    console: Handle,
    process: Handle,
    vmar: Handle,
    broker: Handle,
}

#[derive(Clone, Copy)]
struct Layout {
    bar: usize,
    row_h: usize,
    top: usize,
}

#[derive(Clone, Copy)]
struct State {
    wlan: bool,
    bluetooth: bool,
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
    let head = b"settings: ";
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
        || nh < 5
        || nb <= b"bin/settings".len()
        || &blob[..b"bin/settings".len()] != b"bin/settings"
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

fn rpc(
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
    let natural = (bar * 3 / 2).max(88).min(180);
    let avail = h.saturating_sub(bar + 48);
    let row_h = natural.min(avail / ROWS.len()).max(48);
    Layout { bar, row_h, top: bar + 24 }
}

fn render(fb: &mut [u8], stride: usize, w: usize, h: usize, lay: Layout, st: State) {
    fill(fb, stride, 0, 0, w, h, [0xF5, 0xF7, 0xFA, 0]);
    fill(fb, stride, 0, 0, w, lay.bar, [0x0D, 0x6E, 0x8F, 0]);
    text(fb, stride, b"Settings", lay.bar + 18, (lay.bar - 48) / 2, [0xFF, 0xFF, 0xFF, 0]);

    // 返回键。
    let cx = lay.bar / 2;
    let r = lay.bar.saturating_sub(18) / 2;
    circle(fb, stride, cx, cx, r, [0xE6, 0xF5, 0xFA]);
    for y in 0..(r / 2) {
        let half = y as i32 - r as i32 / 4;
        for dx in 0..(r / 2) {
            let x = cx - r / 4 + dx as usize;
            let yy = cx as i32 + half;
            if yy >= 0 && yy < lay.bar as i32 && x < lay.bar {
                put_px(fb, stride, x, yy as usize, [0x07, 0x4B, 0x63, 0]);
            }
        }
    }

    // OH 设置分组卡：一张大圆角卡 + 行间细分隔线，替代零散白条。
    let list_h = ROWS.len() * lay.row_h + 12;
    shadow(fb, stride, 24, lay.top - 2, w - 44, list_h, 52);
    round_rect(fb, stride, 18, lay.top - 8, w - 36, list_h, 48, [0xFF, 0xFF, 0xFF]);
    for (i, label) in ROWS.iter().enumerate() {
        let y = lay.top + i * lay.row_h;
        if i > 0 {
            fill(fb, stride, 72, y + 2, w - 144, 2, [0xE9, 0xED, 0xF3, 0]);
        }
        let ty = y + (lay.row_h - 48) / 2;
        text(fb, stride, label, 40, ty, [0x21, 0x2B, 0x36, 0]);
        if i < 2 {
            let on = if i == 0 { st.wlan } else { st.bluetooth };
            let pill_w = lay.bar * 3 / 2;
            let pill_h = lay.bar / 2;
            let px = w.saturating_sub(pill_w + 40);
            let py = y + (lay.row_h - pill_h) / 2;
            let bg = if on { [0x00, 0x8B, 0x6F, 0] } else { [0xC7, 0xD0, 0xDB, 0] };
            round_rect(fb, stride, px, py, pill_w, pill_h, pill_h / 2, [bg[0], bg[1], bg[2]]);
            let knob = pill_h - 8;
            let kx = if on { px + pill_w - knob - 4 } else { px + 4 };
            fill(fb, stride, kx, py + 4, knob, knob, [0xFF, 0xFF, 0xFF, 0]);
            let word: &[u8] = if on { b"On" } else { b"Off" };
            text(fb, stride, word, px - 80, ty, [0x55, 0x62, 0x72, 0]);
        } else {
            text(fb, stride, b">", w - 80, ty, [0x8A, 0x96, 0xA5, 0]);
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

#[derive(Clone, Copy)]
enum Action {
    Toggle(usize),
    Back,
}

fn wait_action(console: Handle, input: Handle, w: usize, lay: Layout) -> Action {
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
                    if ev.phase != gp::PHASE_UP {
                        continue;
                    }
                    if ev.x < lay.bar as u32 && ev.y < lay.bar as u32 {
                        return Action::Back;
                    }
                    for row in 0..2 {
                        let y0 = (lay.top + row * lay.row_h) as u32;
                        let y1 = y0 + lay.row_h as u32;
                        if ev.y >= y0 && ev.y < y1 && ev.x < w as u32 {
                            return Action::Toggle(row);
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
    send(c, b"SETTINGS up\n");
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
        layer: gp::WM_LAYER_APP, flags: 0, parent: 0, group: 0,
        x: 0, y: 0, w: info.disp_w, h: info.disp_h,
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
    send(c, b"SETTINGSWIN ok\n");

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
    let lay = layout(w, h);
    let mut state = State { wlan: false, bluetooth: false };
    render(fb, stride, w, h, lay, state);
    let _ = frame(c, wm, vmo, u32::from(win), 1);
    send(c, b"SETTINGSFRAME ok\n");
    send(
        c,
        b"SETTINGSGEOM ok\n",
    );

    let mut buffer_id = 2u32;
    let mut toggles = 0usize;
    if !env.boot_probe {
        loop {
            match wait_action(c, input, w, lay) {
                Action::Back => {
                    send(c, b"SETTINGSBACK ok\n");
                    break;
                }
                Action::Toggle(row) => {
                    if row == 0 {
                        state.wlan = !state.wlan;
                    } else {
                        state.bluetooth = !state.bluetooth;
                    }
                    render(fb, stride, w, h, lay, state);
                    let _ = frame(c, wm, vmo, u32::from(win), buffer_id);
                    buffer_id += 1;
                    toggles += 1;
                    if toggles == 1 {
                        send(c, b"SETTINGSTOGGLE ok n=1\n");
                    } else if toggles == 2 {
                        send(c, b"SETTINGSTOGGLE ok n=2\n");
                    }
                    send(c, b"SETTINGSFRAME ok\n");
                }
            }
        }
    } else {
        send(c, b"SETTINGSPROBE ok\n");
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
    send(c, b"SETTINGSDESTROY ok\n");
    let _ = berry_handle_close(input);
    let _ = berry_handle_close(wm);
    let _ = berry_handle_close(vmo);
    let _ = berry_handle_close(font_vmo);
    send(c, b"SETTINGSEXIT ok\n");
    berry_process_exit(0)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    berry_process_exit(134)
}
