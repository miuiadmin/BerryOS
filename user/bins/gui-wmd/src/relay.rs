//! present/hdi 双域透传中继（GUI-5 §3 #6）——guifbd 零改动，客户端端
//! 换手 wmd：wm 客户端的 present/hdi 请求原样转发 guifbd，应答原样回投
//! （帧字节级零观测差）。**禁 wmd 合成 present 域应答**（真值源恒
//! guifbd，Q15）——本模块只转发收到的字节；wmd 自产面仅 wm 域与 1B
//! ST_BAD 拒线。
//!
//! **分发表执法**（R1 应修①，在 main.rs `on_wm` 入口）：present
//! 1..=6 / hdi 0x40..=0x43 → 本模块；wm 0x60..=0x7F → 本地表；其余 op
//! （含 0x20/0x21 input 域错投）→ 1B ST_BAD。随行 handle 数与两域协议
//! 定值不符 → ST_BAD + close（防坏客户端向中继塞 handle——send 即
//! take，违规面必须先收下再关，不 close 即泄漏）。
//!
//! **失败面**（§3 #6：超时/持续 EBUSY/应答随行数超窗 = `trap()` fail
//! loud，绝不合成应答）：三重兜底链前置——①客户端 gfxdemo 自带 10s
//! RPC 帽先炸（中继帽 ≥ 客户端帽，客户端侧永远先超时降级）；②inputd
//! 超龄 10s 重发 WAIT(0) 兜底；③标记缺失 = smoke FAIL loud。wmd 本面
//! trap = 第四层（guifbd 已死/链断——真死亡面，只死自己）。
//!
//! RELEASE=ReleaseWait **长 park 边界**：透传面无边界问题——启动序②
//! boot 期 guifbd 空转即答；运行期 wm 客户端 ReleaseWait 经中继最多
//! park 一个 guifbd IDLE 帽（客户端 10s 帽同量级先兜）。present GETINFO
//! 逐调用中继（真值源恒 guifbd——wmd 不持第二份 present 常量副本），
//! 启动期一次缓存例外（`boot_getinfo`）。

use crate::{close_all, now_ns, send_rep_handles, st_rep, trap};
use berry_guiproto as gp;
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_handle_close, berry_handle_duplicate,
    berry_object_wait_one, berry_vmo_commit, berry_vmo_create, berry_vmo_get_size,
    berry_vmo_read, berry_vmo_write, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, RIGHT_READ,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};

/// 中继收侧 READABLE 帽（10s 量级——gfxdemo `RPC_CAP_NS` 同值；帽 ≥
/// 客户端帽 = 客户端侧先超时降级，wmd trap 面只兜真死亡）。
const RELAY_CAP_NS: u64 = 300 * 1_000_000_000;

/// 期望随行数（`Some((min, max))` = 执法；`None` = 不执法——畸形帧透传
/// 交 guifbd 拒（wmd 不做 present/hdi 域第二解析器，帧形校验单源）。
/// 定值：SUBMIT=恰 1（damage VMO）；PREPARE=恰 1（buffer VMO）；REG_
/// VBLANK 使能形恰 1（event）/关闭形 {0,1}（guifbd 服务端宽收同款）；
/// 余 op 恒 0。
fn expect_attach(msg: &[u8]) -> Option<(usize, usize)> {
    match msg.first() {
        Some(&gp::OP_SUBMIT) | Some(&gp::OP_PREPARE) => Some((1, 1)),
        Some(&gp::OP_REG_VBLANK) => match msg.get(1) {
            Some(&1) => Some((1, 1)),
            Some(&0) => Some((0, 1)),
            _ => None,
        },
        Some(&op)
            if (gp::OP_GETINFO..=gp::OP_POWER_MODE).contains(&op)
                || (gp::OP_VBLANK_WAIT..=gp::OP_COMMIT).contains(&op) =>
        {
            Some((0, 0))
        }
        _ => None,
    }
}

/// 透传单趟：随行数执法 → 转发（send 即 take——随行 handle 迁入 guifbd
/// 表）→ 收应答 → 原样回投 wm 客户端（应答随行 handle 一并迁回——
/// PREPARE fence / RELEASE 归还别名面）。recv 窗 `[u8;256]+[Handle;8]`
/// （gfxdemo :190-191 同形；wm 客户端面按 CHAN_MSG_MAX/CHAN_HND_MAX
/// 全窗收——应答超窗 = kernel 侧拒发，本面防御性 trap）。
pub fn relay(ch: Handle, cli: Handle, msg: &[u8], hs: &[Handle]) {
    if let Some((lo, hi)) = expect_attach(msg) {
        if hs.len() < lo || hs.len() > hi {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
    }
    // —— 转发（持续 EBUSY = trap；非 BUSY 错码 = 链断 trap）——
    let deadline = now_ns() + RELAY_CAP_NS;
    loop {
        let s = berry_channel_send(cli, msg.as_ptr(), msg.len(), hs.as_ptr(), hs.len());
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            println!("gui-wmd: relay send 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(cli, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            println!("gui-wmd: relay send busy 帽到点");
            trap();
        }
    }
    // —— 收应答（超时 = trap：真值源已死/链断，禁合成）——
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let deadline = now_ns() + RELAY_CAP_NS;
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(cli, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        if s == 0 {
            if nh > rhs.len() {
                trap(); // 应答随行数超窗——防御面（kernel 侧先拒）
            }
            if nb == 0 {
                trap(); // 空应答 = 协议破缺
            }
            send_rep_handles(ch, &rep[..nb], &rhs[..nh]);
            return;
        }
        if s != BERRY_ERR_AGAIN {
            println!("gui-wmd: relay recv 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(cli, SIGNAL_READABLE, deadline, &mut obs) != 0 {
            println!("gui-wmd: relay recv 帽到点");
            trap();
        }
    }
}

/// 无随行单趟 RPC（启动期专用：②RELEASE_WAIT / ③GETINFO；send/recv 失败
/// 面同 [`relay`]——boot 期 guifbd 空转即答，无长 park）。
fn rpc0(cli: Handle, req: &[u8]) -> ([u8; 256], usize, [Handle; 8], usize) {
    let deadline = now_ns() + RELAY_CAP_NS;
    loop {
        let s = berry_channel_send(cli, req.as_ptr(), req.len(), core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            println!("gui-wmd: rpc send 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(cli, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            println!("gui-wmd: rpc send busy 帽到点");
            trap();
        }
    }
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let deadline = now_ns() + RELAY_CAP_NS;
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(cli, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        if s == 0 {
            return (rep, nb, rhs, nh);
        }
        if s != BERRY_ERR_AGAIN {
            println!("gui-wmd: rpc recv 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(cli, SIGNAL_READABLE, deadline, &mut obs) != 0 {
            println!("gui-wmd: rpc recv 帽到点");
            trap();
        }
    }
}

/// 启动序②：present 首发 RELEASE_WAIT 清前世孤儿别名（wmd 崩溃重启
/// 残留——guifbd held 表持前世在途别名 credits 永占；guifbd :474-496
/// 语义零改动）。应答随行归还别名全量即关（本 wmd 不持 present credits
/// 状态——中继面无账可清）。
pub fn boot_release_all(cli: Handle) {
    let mut req = [0u8; 256];
    let n = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
        Some(n) => n,
        None => trap(), // 256B 窗恒容 4B 帧——防御面
    };
    let (rep, nb, rhs, nh) = rpc0(cli, &req[..n]);
    match gp::parse_release_rep(&rep[..nb]) {
        Some(rel) => {
            if nh != rel.count {
                trap(); // 随行数与 count 失配 = 协议破缺
            }
            close_all(&rhs[..nh]);
        }
        None => trap(), // boot 期 guifbd 恒在（wmd spawn 门在 guifbd Ok 后）
    }
}

/// 启动序③：present GETINFO 缓存几何（快照头/wm GETINFO 的 disp 域
/// 同源；此后 wm GETINFO 应答自缓存，present GETINFO 本域仍逐调用中继）。
pub fn boot_getinfo(cli: Handle) -> gp::GetinfoRep {
    let mut req = [0u8; 256];
    let n = match gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo) {
        Some(n) => n,
        None => trap(),
    };
    let (rep, nb, _rhs, nh) = rpc0(cli, &req[..n]);
    if nh != 0 {
        trap(); // GETINFO 应答零随行——防御面
    }
    match gp::parse_getinfo_rep(&rep[..nb]) {
        Some(info) => info,
        None => trap(),
    }
}


/// Phase 2 wmd 合成面：全屏 XRGB8888 VMO + 行弹舱。wmd 是 guifbd 唯一
/// present client；每个 session SUBMIT 先按 owner 窗口拷入此 VMO，再以
/// surface_id=0 聚合提交 guifbd。
pub struct Compositor {
    pub vmo: Handle,
    pub w: u32,
    pub h: u32,
    pub stride: usize,
    bounce: &'static mut [u8],
}

impl Compositor {
    pub fn new(w: u32, h: u32) -> Compositor {
        let stride = w as usize * 4;
        let len = ((stride * h as usize + 0xFFF) / 0x1000) * 0x1000;
        let mut vmo: Handle = 0;
        if berry_vmo_create(len as u64, 0, &mut vmo) != 0
            || vmo == 0
            || berry_vmo_commit(vmo, 0, len as u64) != 0
        {
            trap();
        }
        let bounce: &'static mut [u8] =
            std::boxed::Box::leak(std::vec![0u8; 16384].into_boxed_slice());
        Compositor { vmo, w, h, stride, bounce }
    }

    /// 拷一个已裁剪矩形。src stride 由 surface 宽决定；dst stride 是全屏。
    fn copy_rect(&mut self, src: Handle, src_stride: usize, dst_xy: (u32, u32),
                 r: gp::Rect) -> bool {
        let x0 = r.x.min(self.w);
        let y0 = r.y.min(self.h);
        let x1 = r.x.saturating_add(r.w).min(self.w);
        let y1 = r.y.saturating_add(r.h).min(self.h);
        if x1 <= x0 || y1 <= y0 {
            return true;
        }
        let row = (x1 - x0) as usize * 4;
        if row > self.bounce.len() {
            return false;
        }
        for y in y0..y1 {
            let dst_x = dst_xy.0.saturating_add(x0);
            let dst_y = dst_xy.1.saturating_add(y);
            if dst_x >= self.w || dst_y >= self.h {
                return false;
            }
            let src_off = y as usize * src_stride + x0 as usize * 4;
            let dst_off = dst_y as usize * self.stride + dst_x as usize * 4;
            if berry_vmo_read(src, self.bounce.as_mut_ptr(), src_off as u64, row) != 0
                || berry_vmo_write(self.vmo, self.bounce.as_ptr(), dst_off as u64, row) != 0
            {
                return false;
            }
        }
        true
    }
}

/// SUBMIT 专用合成中继：校验 surface owner → 拷入 wmd 全屏合成 VMO →
/// guifbd 收到的永远是 surface_id=0 的 wmd 聚合帧。真实 token/credits 仍
///由 guifbd 产生，wmd 不合成 present 成功应答。
pub fn submit_composite(
    ch: Handle,
    cli: Handle,
    owner: u16,
    client_type: u8,
    surface_geometry: impl Fn(u16, u16) -> Option<gp::authority::SurfaceGeometry>,
    comp: &mut Compositor,
    msg: &[u8],
    hs: &[Handle],
) {
    if hs.len() != 1 {
        close_all(hs);
        st_rep(ch, gp::ST_BAD);
        return;
    }
    let src = hs[0];
    let Some(gp::PresentReq::Submit { surface_id, flags, rects, rect_count, .. }) =
        gp::parse_present_req(msg)
    else {
        let _ = berry_handle_close(src);
        st_rep(ch, gp::ST_BAD);
        return;
    };

    // R2 双几何合约：raw 是客户端 buffer/stride 基准；visible 是遮挡后
    // 可呈现域。APP 窗口两者都来自 WmTable；全局桌面 surface 无遮挡，
    // raw=visible=合成 VMO 全屏。
    let geometry = if surface_id == 0 {
        if client_type != gp::WM_CLIENT_SYSTEM {
            let _ = berry_handle_close(src);
            st_rep(ch, gp::ST_DENIED);
            return;
        }
        let dims = (comp.w as u16, comp.h as u16);
        gp::authority::SurfaceGeometry::new(
            (0, 0, dims.0, dims.1),
            (0, 0, dims.0, dims.1),
        )
    } else {
        let id = u16::try_from(surface_id).unwrap_or(0);
        surface_geometry(owner, id)
    };
    let Some(geometry) = geometry else {
        let _ = berry_handle_close(src);
        st_rep(ch, gp::ST_DENIED);
        return;
    };
    let (raw_x, raw_y, raw_w, raw_h) = geometry.raw_bounds();
    let dst_xy = (u32::from(raw_x), u32::from(raw_y));
    let src_w = raw_w;
    let src_h = raw_h;
    let visible_local = geometry.visible_local();
    let src_stride = src_w as usize * 4;
    let mut size = 0u64;
    if berry_vmo_get_size(src, &mut size) != 0
        || size < src_stride.saturating_mul(src_h as usize) as u64
    {
        let _ = berry_handle_close(src);
        st_rep(ch, gp::ST_BAD);
        return;
    }

    let full = flags & gp::SUBMIT_F_DAMAGE_FULL != 0;
    let mut out_rects = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
    let mut out_n = 0usize;
    let ok = if full {
        out_n = 0;
        // FULL 也是 raw buffer-local 的整窗语义；实际拷贝前先收缩到
        // visible 本地矩形，避免读/写持久遮挡区。
        comp.copy_rect(src, src_stride, dst_xy, visible_local)
    } else {
        let mut ok = true;
        for i in 0..rect_count {
            let Some(r) = geometry.clip_damage_to_visible(rects[i]) else {
                continue;
            };
            ok = ok && comp.copy_rect(src, src_stride, dst_xy, r);
            if !ok {
                break;
            }
            out_rects[out_n] = gp::Rect {
                x: dst_xy.0.saturating_add(r.x),
                y: dst_xy.1.saturating_add(r.y),
                w: r.w,
                h: r.h,
            };
            out_n += 1;
        }
        ok
    };
    let _ = berry_handle_close(src);
    if !ok || (!full && out_n == 0) {
        st_rep(ch, gp::ST_BAD);
        return;
    }

    let mut alias: Handle = 0;
    if berry_handle_duplicate(comp.vmo, RIGHT_READ, &mut alias) != 0 || alias == 0 {
        trap();
    }
    let creq = gp::PresentReq::Submit {
        surface_id: 0,
        buffer_id: 0x57444D43,
        flags,
        rate_hint: msg.get(13).copied().unwrap_or(0),
        target_present_ns: u64::from_le_bytes(
            msg.get(16..24).unwrap_or(&[0; 8]).try_into().unwrap(),
        ),
        rects: out_rects,
        rect_count: out_n,
    };
    let mut req = [0u8; 256];
    let n = match gp::encode_present_req(&mut req, &creq) {
        Some(n) => n,
        None => trap(),
    };
    let (rep, nb, _rhs, nh) = rpc_present(cli, &req[..n], &[alias]);
    send_rep_handles(ch, &rep[..nb], &_rhs[..nh]);
}

/// present 域内部 RPC（wmd 自产 SUBMIT 用）：send 即 take alias；应答
///随行恒 0（SUBMIT ack 无 handle）。
fn rpc_present(cli: Handle, req: &[u8], attach: &[Handle]) -> ([u8; 256], usize, [Handle; 8], usize) {
    let deadline = now_ns() + RELAY_CAP_NS;
    loop {
        let s = berry_channel_send(cli, req.as_ptr(), req.len(), attach.as_ptr(), attach.len());
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_BUSY {
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(cli, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            trap();
        }
    }
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let deadline = now_ns() + RELAY_CAP_NS;
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(cli, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        if s == 0 {
            return (rep, nb, rhs, nh);
        }
        if s != BERRY_ERR_AGAIN {
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(cli, SIGNAL_READABLE, deadline, &mut obs) != 0 {
            trap();
        }
    }
}
