//! camera-app —— CAMERA 官方应用真 UI（波4 批3；体验大改造六波方案步骤
//! 5——「预制应用全部优化」owner 投诉）。settings-app 同型（波4 首件
//! 样板）：std musl 链 + CRT0_SEED 锚，wm 门控 → wm CREATE 全屏 APP 层
//! 窗 → 帧 VMO commit+map → soft 渲取景器页 → SUBMIT（surface_id=0
//! 应用面——guifbd 波3-件② 带裁剪保状态栏常驻）→ 常驻 park。
//!
//! UI 面：取景框（暗底+中心亮框）+ 底中快门钮 + 顶模式文本。
//!
//! 验收：`CAMERA ok`（wm 建窗+首帧 SUBMIT 全链断言过恰一次）。

use std::process::exit;

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun, Rect};
use berry_gfx::font::FontAtlas8x16;
use berry_gfx::pass::{PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_guiproto as gp;
use berry_libc::io::wm_cli_h;
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_handle_duplicate, berry_object_wait_many,
    berry_vmar_map, berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE, SIGNAL_READABLE,
};

/// 帧映射基址（gfxdemo FRAME_VA 同款自选静态 VA）。
const FRAME_VA: usize = 0x5000_0000;

#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn fail(msg: &str) -> ! {
    eprintln!("camera: FAIL {msg}");
    exit(101)
}

fn enc(m: Option<usize>) -> usize {
    match m {
        Some(n) => n,
        None => fail("encode"),
    }
}

fn rpc(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> usize {
    let mut s = unsafe {
        berry_channel_send(ch, req.as_ptr(), req.len(), attach.as_ptr(), attach.len())
    };
    while s == BERRY_ERR_BUSY {
        s = unsafe {
            berry_channel_send(ch, req.as_ptr(), req.len(), attach.as_ptr(), attach.len())
        };
    }
    if s != 0 {
        fail(&format!("rpc send err={s}"));
    }
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s =
            unsafe { berry_channel_recv(ch, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh) };
        if s == 0 {
            if nb == 0 {
                fail("rpc empty reply");
            }
            return nb;
        }
        if s != BERRY_ERR_AGAIN {
            fail(&format!("rpc recv err={s}"));
        }
    }
}

fn main() {
    let hw = wm_cli_h();
    if hw == 0 {
        exit(0);
    }

    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let mut req = [0u8; 256];

    let m = gp::encode_wm_getinfo_req(&mut req);
    let n = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let wminfo = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("wm getinfo bad st={}", rep[0])),
    };
    if wminfo.disp_w == 0 || wminfo.disp_h == 0 {
        fail("wm disp zero");
    }

    let creq = gp::WmCreateReq {
        layer: gp::WM_LAYER_APP,
        flags: gp::WM_CREATE_F_AVOID_SYS,
        parent: 0,
        group: 0,
        x: 0,
        y: 0,
        w: wminfo.disp_w,
        h: wminfo.disp_h,
    };
    let m = enc(gp::encode_wm_create_req(&mut req, &creq));
    let n = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let wid = match gp::parse_wm_create_ack(&rep[..n]) {
        Some(id) if id != 0 => id,
        _ => fail(&format!("wm create bad st={}", rep[0])),
    };
    let _ = wid;

    let m = enc(gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo));
    let n = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let info = match gp::parse_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("getinfo bad st={}", rep[0])),
    };

    let (w, h) = (info.display_w as usize, info.display_h as usize);
    let stride = w * 4;
    let len = (stride * h + 0xFFF) & !0xFFF;
    let mut vmo: Handle = 0;
    if unsafe { berry_vmo_create(len as u64, 0, &mut vmo) } != 0 || vmo == 0 {
        fail("vmo create");
    }
    if unsafe { berry_vmo_commit(vmo, 0, len as u64) } != 0 {
        fail("vmo commit");
    }
    let hv = berry_libc::io::self_vmar_h();
    if hv == 0 {
        fail("no vmar");
    }
    let mut va = FRAME_VA;
    if unsafe { berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) } != 0
        || va != FRAME_VA
    {
        fail("vmar map");
    }
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };

    // —— 取景器 UI：暗底 + 中心取景亮框 + 底中快门钮 + 顶状态条。
    let big = w >= 600 && h >= 1800;
    let topbar_h: i32 = if big { 168 } else { 56 };
    let pad: i32 = if big { 48 } else { 16 };
    let (cw, chh) = if big { (16, 32) } else { (8, 16) };

    let mut e = CommandEncoder::new();
    e.fill_rect(
        Rect::new(0, 0, w as i32, h as i32),
        Color { r: 0x1A, g: 0x1A, b: 0x1A },
    );
    e.fill_rect(
        Rect::new(0, 0, w as i32, topbar_h),
        Color { r: 0x22, g: 0x22, b: 0x22 },
    );
    // 取景亮框：3:4 比例中心框。
    let vw_frame = w as i32 - pad * 2;
    let vh_frame = (vw_frame * 4 / 3).min(h as i32 - topbar_h - pad * 3);
    e.fill_rect(
        Rect::new(pad, topbar_h + pad, vw_frame, vh_frame),
        Color { r: 0x3A, g: 0x3A, b: 0x3A },
    );
    // 快门钮：底中正方双圈（v1 无圆原语）。
    let btn_r: i32 = if big { 72 } else { 28 };
    let btn_cx = w as i32 / 2;
    let btn_cy = h as i32 - pad - btn_r * 2;
    e.fill_rect(
        Rect::new(btn_cx - btn_r, btn_cy - btn_r, btn_r * 2, btn_r * 2),
        Color { r: 0xFF, g: 0xFF, b: 0xFF },
    );
    e.fill_rect(
        Rect::new(btn_cx - btn_r + 8, btn_cy - btn_r + 8, (btn_r - 8) * 2, (btn_r - 8) * 2),
        Color { r: 0x33, g: 0x33, b: 0x33 },
    );
    let mut glyphs = [GlyphPos { ch: b' ', x: 0, y: 0 }; 24];
    let mut gi = 0usize;
    let atlas = FontAtlas8x16;
    let title = b"Camera";
    for (i, &c) in title.iter().enumerate() {
        if gi < glyphs.len() {
            glyphs[gi] = GlyphPos {
                ch: c,
                x: pad + 16 + (i as i32) * cw * 2,
                y: (topbar_h - chh) / 2,
            };
            gi += 1;
        }
    }
    e.draw_glyph_run(GlyphRun {
        atlas: &atlas,
        glyphs: &glyphs[..gi],
        fg: Color { r: 0xFF, g: 0xFF, b: 0xFF },
        bg: None,
    });
    let dl = e.finish();
    let pass = PassDescriptor::new(
        berry_gfx::pass::LoadOp::Clear { r: 0x1A, g: 0x1A, b: 0x1A },
        StoreOp::Store,
        None,
    );
    let desc = match TextureDesc::new(w, h, stride, TextureFormat::Rgbx) {
        Ok(d) => d,
        Err(er) => fail(&format!("texture desc {er:?}")),
    };
    if let Err(er) = Queue::new().submit(&dl, fb, &desc, &pass) {
        fail(&format!("render submit {er:?}"));
    }

    let mut alias: Handle = 0;
    if unsafe { berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) } != 0
        || alias == 0
    {
        fail("dup alias");
    }
    let sreq = gp::PresentReq::Submit {
        surface_id: 0,
        buffer_id: 1,
        flags: gp::SUBMIT_F_DAMAGE_FULL,
        rate_hint: 0,
        target_present_ns: 0,
        rects: [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
        rect_count: 0,
    };
    let m = enc(gp::encode_present_req(&mut req, &sreq));
    let n = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
    let token = match gp::parse_submit_ack(&rep[..n]) {
        Some(t) => t,
        None => fail(&format!("submit ack bad st={}", rep[0])),
    };
    let m = enc(gp::encode_present_req(
        &mut req,
        &gp::PresentReq::FrameCallback { frame_token: token },
    ));
    let n = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    match gp::parse_frame_done_rep(&rep[..n]) {
        Some(d) if d.status == gp::FRAME_PRESENTED => {}
        Some(d) => fail(&format!("frame status={}", d.status)),
        None => fail(&format!("frame done bad st={}", rep[0])),
    }
    let m = enc(gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait));
    let n = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let rel = match gp::parse_release_rep(&rep[..n]) {
        Some(r) => r,
        None => fail(&format!("release bad st={}", rep[0])),
    };
    let _ = rel;

    println!("CAMERA ok");
    let items = [hw];
    loop {
        let mut idx = usize::MAX;
        let mut obs = 0u64;
        let _ = unsafe {
            berry_object_wait_many(
                items.as_ptr(),
                [SIGNAL_READABLE].as_ptr(),
                1,
                u64::MAX,
                &mut idx,
                &mut obs,
            )
        };
        let mut buf = [0u8; 256];
        let mut nb = buf.len();
        let mut hbuf = [0 as Handle; 8];
        let mut nh = hbuf.len();
        let _ = unsafe {
            berry_channel_recv(hw, buf.as_mut_ptr(), &mut nb, hbuf.as_mut_ptr(), &mut nh)
        };
    }
}
