//! present/hdi 双域透传中继（GUI-5 §3 #6）——guifbd 零改动，客户端端
//! 换手 wmd：wm 客户端的 present/hdi 请求原样转发 guifbd，应答原样回投
//! （帧字节级零观测差）。**禁 wmd 合成 present 域应答**（真值源恒
//! guifbd，Q15）——本模块只转发收到的字节；wmd 自产面仅 wm 域与 1B
//! ST_BAD 拒线。
//!
//! **分发表执法**（R1 应修①，在 main.rs `on_wm` 入口）：present
//! 1..=8 / hdi 0x40..=0x45 → 本模块；op 9 仅 wmd 自产 BootProgress，不接
//! app 请求；wm 0x60..=0x7F → 本地表；其余 op
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

use super::table;
use super::{close_all, now_ns, send_handoff_rep, send_rep_handles, st_rep, trap};
use berry_guiproto as gp;
use core::sync::atomic::{AtomicBool, Ordering};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_handle_close, berry_handle_duplicate,
    berry_object_wait_one, berry_vmo_commit, berry_vmo_create, berry_vmo_get_size,
    berry_vmo_read, berry_vmo_write, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, RIGHT_READ,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};

/// 中继收侧 READABLE 帽（10s 量级——gfxdemo `RPC_CAP_NS` 同值；帽 ≥
/// 客户端帽 = 客户端侧先超时降级，wmd trap 面只兜真死亡）。
const RELAY_CAP_NS: u64 = 300 * 1_000_000_000;

/// wmd 单线程合成器的静态 bounce。64KiB 避免用户堆单次大分配，同时把
/// 1080 宽整屏合成的 VMO syscall 从逐行数千次降到数十次。
static mut COMPOSITE_BOUNCE: [u8; 64 * 1024] = [0; 64 * 1024];

/// BOOT-P1：wmd 首个自产 SUBMIT 前只发一次 100% 系统进度；随后真实桌面
/// 帧提交。ramfb 后端按 unsupported 烘焙线忽略，vgpu 后端渲染终态。
static BOOT_PROGRESS_DONE: AtomicBool = AtomicBool::new(false);
static BOOT_PROGRESS_FIRST_SUBMIT: AtomicBool = AtomicBool::new(false);
static BOOT_PROGRESS_SYSTEM_UI: AtomicBool = AtomicBool::new(false);

/// Recents 可即时恢复的最近 APP owner 数。3 个 slot ≈ 23.7MiB@1080p；
/// 超出走 LRU，未命中者保持今天的“等 app 自绘”真值行为。
const OWNER_CACHE_SLOTS: usize = 3;

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
            if (gp::OP_GETINFO..=gp::OP_FRAME_TIMING).contains(&op)
                || (gp::OP_VBLANK_WAIT..=gp::OP_VBLANK_TIMING).contains(&op) =>
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
    // Only a completed FRAME_CALLBACK may cooperatively hand CPU back to the
    // waiting producer. SUBMIT ACK stays a normal reply: handing off there let
    // producers enqueue another frame before wmd drained pending work.
    let callback_reply = msg.first() == Some(&gp::OP_FRAME_CALLBACK);
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
            if callback_reply {
                send_handoff_rep(ch, &rep[..nb]);
            } else {
                send_rep_handles(ch, &rep[..nb], &rhs[..nh]);
            }
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


struct OwnerFrameCacheSlot {
    owner: u16,
    vmo: Handle,
    windows: [table::OwnerWindow; gp::WM_WINDOWS_MAX],
    window_n: usize,
    stamp: u64,
}

/// Cached alpha source for a launcher-owned SYSTEM bar window.
struct SystemOverlay {
    owner: u16,
    id: u16,
    vmo: Handle,
}

/// Phase 2 wmd 合成面：全屏 XRGB8888 VMO + 行弹舱。wmd 是 guifbd 唯一
/// present client；每个 session SUBMIT 先按 owner 窗口拷入此 VMO，再以
/// surface_id=0 聚合提交 guifbd。
pub struct Compositor {
    vmos: [Handle; 2],
    compose_idx: usize,
    scanout_idx: usize,
    pub w: u32,
    pub h: u32,
    pub stride: usize,
    bytes: u64,
    bounce: &'static mut [u8],
    cache: [Option<OwnerFrameCacheSlot>; OWNER_CACHE_SLOTS],
    cache_epoch: u64,
    fresh_owner: Option<u16>,
    overlays: [Option<SystemOverlay>; 2],
    /// back buffer 相对 front 的陈旧域（屏幕坐标包络）。恒等于「back 落后
    /// 于 front 的区域」：成功提交后 = 该次**实际写入域**（finish_submit
    /// 唯一记账点）；前写路径（`use_scanout_buffer` 写 front）并入写入域。
    /// 初始值/失效值 = 全屏（首提交 back 为零页；写失败后内容不可信）。
    /// 上次 FULL（写入域恰为全屏）自然落回全屏继承——partial 提交不再
    /// 整屏拷贝，SYSTEM/APP/replay 混合序列下像素仍逐域对齐 front。
    back_stale: gp::Rect,
}

impl Compositor {
    pub fn new(w: u32, h: u32) -> Compositor {
        let stride = w as usize * 4;
        let len = ((stride * h as usize + 0xFFF) / 0x1000) * 0x1000;
        let mut vmos: [Handle; 2] = [0, 0];
        for vmo in &mut vmos {
            if berry_vmo_create(len as u64, 0, vmo) != 0
                || *vmo == 0
                || berry_vmo_commit(*vmo, 0, len as u64) != 0
            {
                trap();
            }
        }

        // SAFETY: wmd is single-threaded and creates exactly one compositor.
        let bounce: &'static mut [u8] =
            unsafe { &mut *core::ptr::addr_of_mut!(COMPOSITE_BOUNCE) };
        Compositor {
            vmos,
            compose_idx: 0,
            scanout_idx: 0,
            w,
            h,
            stride,
            bytes: len as u64,
            bounce,
            cache: [None, None, None],
            cache_epoch: 0,
            fresh_owner: None,
            overlays: [None, None],
            back_stale: gp::Rect { x: 0, y: 0, w, h },
        }
    }

    /// 拷一个已裁剪矩形。damage `r` 是 surface raw-local 坐标；窗口原点
    /// `dst_xy` 平移后必须再对**全屏目标域**裁剪。旧实现把 raw-local r 直
    /// 接按屏幕宽高裁，窗口越出右/下边时会把一行写穿到下一屏幕行。
    fn copy_rect(&mut self, src: Handle, src_stride: usize, dst_xy: (u32, u32),
                 r: gp::Rect) -> bool {
        let Some(clip) = table::composite_clip(dst_xy, r, self.w, self.h) else {
            return true;
        };
        let row = clip.dst.w as usize * 4;
        if row == 0 || row > self.bounce.len() {
            return row == 0;
        }
        let rows_per_chunk = self.bounce.len() / row;
        let mut row_index = 0usize;
        while row_index < clip.dst.h as usize {
            let rows = (clip.dst.h as usize - row_index).min(rows_per_chunk);
            let len = rows * row;
            let src_off = (clip.src.y as usize + row_index) * src_stride
                + clip.src.x as usize * 4;
            if berry_vmo_read(src, self.bounce.as_mut_ptr(), src_off as u64, len) != 0 {
                return false;
            }
            // Full-width rows have identical source/destination stride and are
            // contiguous in both VMOs. One bulk write per bounce chunk replaces
            // per-row writes (product surface 0: ~1920 -> ~128 syscalls).
            if src_stride == self.stride
                && clip.src.x == 0
                && clip.dst.x == 0
                && clip.dst.w == self.w
            {
                let write_dst = (clip.dst.y as usize + row_index) * self.stride;
                let dst = self.active_vmo();
                if berry_vmo_write(dst, self.bounce.as_ptr(), write_dst as u64, len) != 0 {
                    return false;
                }
            } else {
                // Source rows use the window stride; destination rows use the
                // full-screen stride. Bulk-read packed source rows, then write each
                // destination row without crossing a screen row end.
                for chunk_row in 0..rows {
                    let src_base = chunk_row * row;
                    let write_dst = (clip.dst.y as usize + row_index + chunk_row) * self.stride
                        + clip.dst.x as usize * 4;
                    let bounce_ptr = unsafe { self.bounce.as_ptr().add(src_base) };
                    let dst = self.active_vmo();
                    if berry_vmo_write(dst, bounce_ptr, write_dst as u64, row) != 0 {
                        return false;
                    }
                }
            }
            row_index += rows;
        }
        true
    }

    pub fn fresh_owner(&self) -> Option<u16> {
        self.fresh_owner
    }

    /// Replace the cached source for a SYSTEM overlay. The handle is a READ
    /// duplicate owned by wmd; source alpha is straight ARGB8888.
    fn cache_system_overlay(
        &mut self,
        owner: u16,
        id: u16,
        vmo: Handle,
    ) -> bool {
        let slot = self
            .overlays
            .iter()
            .position(|o| o.is_none() || o.as_ref().is_some_and(|o| o.owner == owner && o.id == id))
            .unwrap_or_else(|| {
                let oldest = 0;
                if let Some(old) = self.overlays[oldest].take() {
                    let _ = berry_handle_close(old.vmo);
                }
                oldest
            });
        if let Some(old) = self.overlays[slot].take() {
            let _ = berry_handle_close(old.vmo);
        }
        self.overlays[slot] = Some(SystemOverlay { owner, id, vmo });
        true
    }

    /// Resolve cached overlays against current WM visibility/geometry.
    fn visible_system_overlays(
        &self,
        mut geometry: impl FnMut(u16, u16) -> Option<gp::authority::SurfaceGeometry>,
    ) -> [(u16, u16, u32, u32, u32, u32); 2] {
        let mut out = [(0, 0, 0, 0, 0, 0); 2];
        let mut count = 0usize;
        for overlay in self.overlays.iter().flatten() {
            if count == out.len() {
                break;
            }
            if let Some(geom) = geometry(overlay.owner, overlay.id) {
                let (x, y, w, h) = geom.visible_bounds();
                out[count] = (overlay.owner, overlay.id, u32::from(x), u32::from(y), u32::from(w), u32::from(h));
                count += 1;
            }
        }
        out
    }

    /// Blend visible overlays over the active composition buffer. Alpha is
    /// read from the overlay source; destination remains opaque XRGB.
    fn apply_system_overlays(
        &mut self,
        visible: &[(u16, u16, u32, u32, u32, u32); 2],
    ) -> bool {
        for &(owner, id, x, y, w, h) in visible {
            let Some(slot) = self
                .overlays
                .iter()
                .position(|o| o.as_ref().is_some_and(|o| o.owner == owner && o.id == id))
            else {
                continue;
            };
            let overlay = self.overlays[slot].as_ref().expect("checked");
            let row = w as usize * 4;
            if row == 0 || row * 2 > self.bounce.len() {
                return false;
            }
            let rows_per_chunk = (self.bounce.len() / 2 / row).max(1);
            let mut y0 = 0usize;
            while y0 < h as usize {
                let rows = (h as usize - y0).min(rows_per_chunk);
                let src_off = y0 * row;
                if berry_vmo_read(
                    overlay.vmo,
                    self.bounce.as_mut_ptr(),
                    src_off as u64,
                    rows * row,
                ) != 0
                {
                    return false;
                }
                let dst_base = ((y + y0 as u32) as usize * self.stride + x as usize * 4)
                    as u64;
                let dst_len = rows * row;
                // Read destination into the second half of the bounce buffer,
                // then blend source alpha in place without raw-pointer writes.
                let destination_vmo = self.active_vmo();
                let (source, destination) = self.bounce.split_at_mut(dst_len);
                if berry_vmo_read(
                    destination_vmo,
                    destination.as_mut_ptr(),
                    dst_base,
                    dst_len,
                ) != 0
                {
                    return false;
                }
                for i in 0..dst_len / 4 {
                    let sp = i * 4;
                    let dp = sp;
                    let sa = source[sp + 3];
                    if sa == 0 {
                        continue;
                    }
                    let sr = source[sp];
                    let sg = source[sp + 1];
                    let sb = source[sp + 2];
                    if sa == 255 {
                        destination[dp] = sr;
                        destination[dp + 1] = sg;
                        destination[dp + 2] = sb;
                    } else {
                        let a = u32::from(sa);
                        let inv = 255 - a;
                        destination[dp] =
                            ((u32::from(sr) * a + u32::from(destination[dp]) * inv) / 255) as u8;
                        destination[dp + 1] = ((u32::from(sg) * a
                            + u32::from(destination[dp + 1]) * inv)
                            / 255) as u8;
                        destination[dp + 2] = ((u32::from(sb) * a
                            + u32::from(destination[dp + 2]) * inv)
                            / 255) as u8;
                    }
                }
                if berry_vmo_write(
                    destination_vmo,
                    destination.as_ptr(),
                    dst_base,
                    dst_len,
                ) != 0
                {
                    return false;
                }
                y0 += rows;
            }
        }
        true
    }

    fn active_vmo(&self) -> Handle {
        self.vmos[self.compose_idx]
    }

    fn scanout_vmo(&self) -> Handle {
        self.vmos[self.scanout_idx]
    }

    /// 屏幕域裁剪（结果恒 ⊆ 屏幕；x0/y0 先 min，w/h 无下溢）。
    fn clip_to_screen(&self, r: gp::Rect) -> gp::Rect {
        let x0 = r.x.min(self.w);
        let y0 = r.y.min(self.h);
        let x1 = r.x.saturating_add(r.w).min(self.w);
        let y1 = r.y.saturating_add(r.h).min(self.h);
        gp::Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    /// 两屏幕域矩形并集包络（空矩形 = 中性元——「未写入」不放大继承域）。
    fn union_screen_rect(a: gp::Rect, b: gp::Rect) -> gp::Rect {
        let empty = |r: gp::Rect| r.w == 0 || r.h == 0;
        if empty(a) {
            return b;
        }
        if empty(b) {
            return a;
        }
        let x0 = a.x.min(b.x);
        let y0 = a.y.min(b.y);
        let x1 = a.x.saturating_add(a.w).max(b.x.saturating_add(b.w));
        let y1 = a.y.saturating_add(a.h).max(b.y.saturating_add(b.h));
        gp::Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    /// 一批屏幕坐标矩形的实际写入域包络（越屏项由 finish_submit 裁剪）。
    fn envelope(rects: &[gp::Rect]) -> gp::Rect {
        let mut e = gp::Rect { x: 0, y: 0, w: 0, h: 0 };
        for &r in rects {
            e = Self::union_screen_rect(e, r);
        }
        e
    }

    /// back buffer 继承：只补 [`Self::back_stale`] 域——back 恰好只在上一
    /// 次提交的实际写入域落后于 front，其余区域两缓冲逐像素相等。本提交
    /// 将整屏覆盖（SYSTEM surface 0 的 FULL）时无需继承；`compose ==
    /// scanout`（首提交 / 前写路径）本就写 front，同样无需继承。
    fn prepare_back_buffer(&mut self, covers_screen: bool) -> bool {
        if covers_screen || self.compose_idx == self.scanout_idx {
            return true;
        }
        if self.back_stale.w == 0 || self.back_stale.h == 0 {
            return true; // 上次提交空写入域：back 已与 front 全等
        }
        let front = self.scanout_vmo();
        let back = self.active_vmo();
        self.copy_vmo_rect(front, back, self.back_stale)
    }

    /// 唯一记账点：成功提交后按本次实际写入域（屏幕包络）更新 back 陈旧
    /// 度。翻转路径（写 back 并换 scanout）：新 back = 旧 front，恰好只
    /// 陈旧于本次写入域；前写路径（`use_scanout_buffer` 写 front）：back
    /// 未动，陈旧度并入本次写入域。空写入域（FULL 但 visible 为空）= 本
    /// 次提交未改像素，陈旧度归零/保持。失败路径禁入——走
    /// [`Self::invalidate_back_buffer`]。
    fn finish_submit(&mut self, written: gp::Rect) {
        let written = self.clip_to_screen(written);
        if self.compose_idx == self.scanout_idx {
            self.back_stale = Self::union_screen_rect(self.back_stale, written);
        } else {
            self.back_stale = written;
        }
        self.scanout_idx = self.compose_idx;
        self.compose_idx = 1 - self.compose_idx;
    }

    /// A full-screen HOME/boot redraw leaves the new back stale over the whole
    /// screen. Mirror it immediately after replying: the copy overlaps the
    /// queued front DMA and prevents the first post-HOME swipe from paying a
    /// serial full-screen inheritance before its first banded frame.
    fn back_stale_covers_screen(&self) -> bool {
        self.back_stale.x == 0
            && self.back_stale.y == 0
            && self.back_stale.w == self.w
            && self.back_stale.h == self.h
    }

    fn prefetch_back_after_full(&mut self) {
        if self.prepare_back_buffer(false) {
            self.back_stale = gp::Rect { x: 0, y: 0, w: 0, h: 0 };
        } else {
            self.invalidate_back_buffer();
        }
    }

    /// Build the backend damage plan for a composition that has just been
    /// written but not yet flipped. On the double-buffer path the host resource
    /// also lacks the inherited `back_stale` pixels, so the submitted plan must
    /// cover inheritance ∪ actual writes. The front-write path (APP windows)
    /// submits only its actual writes.
    fn submit_plan(
        &self,
        written: &[gp::Rect],
        written_n: usize,
    ) -> ([gp::Rect; gp::DAMAGE_MAX], usize) {
        let mut out = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
        if self.compose_idx == self.scanout_idx {
            let count = written_n.min(out.len());
            for i in 0..count {
                out[i] = written[i];
            }
            return (out, count);
        }
        let mut plan = self.back_stale;
        for &rect in written.iter().take(written_n) {
            plan = Self::union_screen_rect(plan, rect);
        }
        out[0] = plan;
        (out, 1)
    }

    /// 失败保守面：prepare/拷贝部分落笔后放弃、或聚合提交被后端拒绝时，
    /// back 的实际内容已偏离记账（写域超出 back_stale 记录），陈旧度回退
    /// 全屏——下次继承整屏重拷，宁可多拷不静默取陈旧像素。
    fn invalidate_back_buffer(&mut self) {
        self.back_stale = gp::Rect { x: 0, y: 0, w: self.w, h: self.h };
    }

    /// APP/window submissions stay on the authoritative front buffer. The
    /// double-buffer fast path is reserved for full-screen SYSTEM composition;
    /// draining present credits before each APP update keeps front writes safe.
    fn use_scanout_buffer(&mut self) {
        self.compose_idx = self.scanout_idx;
    }

    fn tuple_rect(&self, r: (u16, u16, u16, u16)) -> gp::Rect {
        let x0 = u32::from(r.0).min(self.w);
        let y0 = u32::from(r.1).min(self.h);
        let x1 = (u32::from(r.0) + u32::from(r.2)).min(self.w);
        let y1 = (u32::from(r.1) + u32::from(r.3)).min(self.h);
        gp::Rect {
            x: x0,
            y: y0,
            w: x1.saturating_sub(x0),
            h: y1.saturating_sub(y0),
        }
    }

    fn copy_vmo_rect(&mut self, src: Handle, dst: Handle, rect: gp::Rect) -> bool {
        if rect.w == 0 || rect.h == 0 {
            return true;
        }
        let row = rect.w as usize * 4;
        if row == 0 || row > self.bounce.len() {
            return false;
        }
        let rows_per_chunk = self.bounce.len() / row;
        let mut row_index = 0usize;
        while row_index < rect.h as usize {
            let rows = (rect.h as usize - row_index).min(rows_per_chunk);
            let len = rows * row;
            let off = (rect.y as usize + row_index) * self.stride + rect.x as usize * 4;
            if berry_vmo_read(src, self.bounce.as_mut_ptr(), off as u64, len) != 0
                || berry_vmo_write(dst, self.bounce.as_ptr(), off as u64, len) != 0
            {
                return false;
            }
            row_index += rows;
        }
        true
    }

    /// 在他人覆盖 composition 前保存 fresh APP owner 的真实可见像素。
    pub fn capture_fresh_owner(
        &mut self,
        windows: [table::OwnerWindow; gp::WM_WINDOWS_MAX],
        window_n: usize,
    ) {
        let Some(owner) = self.fresh_owner else { return };
        if window_n == 0 {
            return;
        }
        self.cache_epoch = self.cache_epoch.saturating_add(1);
        let stamp = self.cache_epoch;
        let existing = self
            .cache
            .iter()
            .position(|s| s.as_ref().map(|s| s.owner) == Some(owner));
        let target = existing.unwrap_or_else(|| {
            self.cache
                .iter()
                .enumerate()
                .min_by_key(|&(i, s)| (s.is_some(), s.as_ref().map(|s| s.stamp).unwrap_or(0), i))
                .map(|(i, _)| i)
                .unwrap_or(0)
        });
        if existing.is_none() {
            if let Some(old) = self.cache[target].take() {
                if berry_handle_close(old.vmo) != 0 {
                    trap();
                }
            }
        }
        let mut cache_vmo = match existing {
            Some(i) => self.cache[i].as_ref().expect("existing checked").vmo,
            None => {
                let mut vmo: Handle = 0;
                if berry_vmo_create(self.bytes, 0, &mut vmo) != 0
                    || vmo == 0
                    || berry_vmo_commit(vmo, 0, self.bytes) != 0
                {
                    let _ = berry_handle_close(vmo);
                    eprintln!("WMCACHE bad alloc owner={owner}");
                    return;
                }
                vmo
            }
        };
        let mut ok = true;
        for win in windows.iter().take(window_n) {
            let rect = self.tuple_rect(win.eff);
            ok = ok && self.copy_vmo_rect(self.scanout_vmo(), cache_vmo, rect);
        }
        if !ok {
            eprintln!("WMCAPTURE bad owner={owner}");
            if existing.is_none() {
                let _ = berry_handle_close(cache_vmo);
            }
            self.cache[target] = None;
            return;
        }
        if let Some(slot) = self.cache[target].as_ref() {
            cache_vmo = slot.vmo;
        }
        self.cache[target] = Some(OwnerFrameCacheSlot {
            owner,
            vmo: cache_vmo,
            windows,
            window_n,
            stamp,
        });
        println!("WMCAPTURE ok owner={owner} rects={window_n}");
    }

    fn windows_match(
        cached: &[table::OwnerWindow; gp::WM_WINDOWS_MAX],
        cached_n: usize,
        current: &[table::OwnerWindow; gp::WM_WINDOWS_MAX],
        current_n: usize,
    ) -> bool {
        // Raw geometry identifies the app's submitted frame. Effective geometry
        // may legitimately change when a temporary SYSTEM overlay (Recents)
        // hides or restores the window; replay below uses the current effective
        // region and therefore clips—not fabricates—those occlusion changes.
        cached_n == current_n
            && cached[..cached_n]
                .iter()
                .zip(current[..current_n].iter())
                .all(|(a, b)| a.raw == b.raw)
    }

    /// 把缓存真实帧写回 composition，返回 present damage 计划。
    fn replay_owner_pixels(
        &mut self,
        owner: u16,
        current: [table::OwnerWindow; gp::WM_WINDOWS_MAX],
        current_n: usize,
    ) -> Option<([gp::Rect; gp::DAMAGE_MAX], usize)> {
        let index = match self.cache.iter().position(|s| {
            s.as_ref().map(|s| s.owner) == Some(owner)
                && Self::windows_match(
                    &s.as_ref().expect("owner checked").windows,
                    s.as_ref().expect("owner checked").window_n,
                    &current,
                    current_n,
                )
        }) {
            Some(index) => index,
            None => {
                for slot in self.cache.iter().flatten().filter(|s| s.owner == owner) {
                    eprintln!(
                        "WMREPLAY stale owner={owner} cached_n={} raw={:?} eff={:?} current_n={} current_raw={:?} current_eff={:?}",
                        slot.window_n,
                        slot.windows[0].raw,
                        slot.windows[0].eff,
                        current_n,
                        current.get(0).map(|w| w.raw),
                        current.get(0).map(|w| w.eff)
                    );
                }
                return None;
            }
        };
        let cache_vmo = {
            let slot = self.cache[index].as_ref()?;
            slot.vmo
        };
        let (windows, window_n) = (current, current_n);
        if !self.prepare_back_buffer(false) {
            eprintln!("WMREPLAY bad prepare owner={owner}");
            self.invalidate_back_buffer();
            return None;
        }
        let mut written_env = gp::Rect { x: 0, y: 0, w: 0, h: 0 };
        for win in windows.iter().take(window_n) {
            let rect = self.tuple_rect(win.eff);
            if rect.w == 0 || rect.h == 0 {
                continue;
            }
            if !self.copy_vmo_rect(cache_vmo, self.active_vmo(), rect) {
                eprintln!("WMREPLAY bad owner={owner}");
                self.invalidate_back_buffer();
                return None;
            }
            written_env = Self::union_screen_rect(written_env, rect);
        }
        // WM_WINDOWS_MAX exceeds DAMAGE_MAX. A monotonic envelope over every
        // written window (plus inherited pixels in submit_plan) is conservative
        // and cannot drop early windows the way per-slot overflow once did.
        let (rects, rect_n) = self.submit_plan(core::slice::from_ref(&written_env), 1);
        Some((rects, rect_n))
    }

    pub fn note_submit_owner(&mut self, owner: u16, client_type: u8) {
        self.fresh_owner = (client_type == gp::WM_CLIENT_APP).then_some(owner);
    }

    pub fn clear_fresh_owner(&mut self) {
        self.fresh_owner = None;
    }

    pub fn forget_owner_cache(&mut self, owner: u16) {
        if let Some(slot) = self
            .cache
            .iter()
            .position(|s| s.as_ref().map(|s| s.owner) == Some(owner))
            .and_then(|i| self.cache[i].take())
        {
            let _ = berry_handle_close(slot.vmo);
            println!("WMCACHEFREE ok owner={owner}");
        }
    }
}

fn boot_progress_point_once(once: &AtomicBool, cli: Handle, milestone: u8) {
    if once.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut req = [0u8; 32];
    let Some(n) = gp::encode_present_req(
        &mut req,
        &match gp::PresentReq::milestone(milestone) {
            Some(request) => request,
            None => return,
        },
    ) else {
        return;
    };
    let (rep, nb, _rhs, nh) = rpc_present(cli, &req[..n], &[]);
    // guifbd 的 ST_UNSUPPORTED 是声明的忽略面；vgpu 非 OK 是显示控制链
    // 死亡面，trap 保持 wmd fail-loud。
    if nb == 1 && nh == 0 {
        if rep[0] == gp::ST_OK || rep[0] == gp::ST_UNSUPPORTED {
            return;
        }
    }
    trap();
}

/// SYSTEM launcher 已通过 per-boot seed 建立权威系统 UI session。
pub fn boot_progress_system_ui_once(cli: Handle) {
    boot_progress_point_once(
        &BOOT_PROGRESS_SYSTEM_UI,
        cli,
        gp::BOOT_M_SYSTEM_UI_CONNECTED,
    );
}

fn boot_progress_first_submit_once(cli: Handle) {
    boot_progress_point_once(
        &BOOT_PROGRESS_FIRST_SUBMIT,
        cli,
        gp::BOOT_M_FIRST_REAL_SUBMIT,
    );
}

fn boot_progress_complete_once(cli: Handle) {
    boot_progress_point_once(
        &BOOT_PROGRESS_DONE,
        cli,
        gp::BOOT_M_DESKTOP_HANDOFF,
    );
}

/// SUBMIT 专用合成中继：校验 surface owner → 拷入 wmd 全屏合成 VMO →
/// guifbd 收到的永远是 surface_id=0 的 wmd 聚合帧。真实 token/credits 仍
///由 guifbd 产生，wmd 不合成 present 成功应答。
pub fn submit_composite(
    ch: Handle,
    cli: Handle,
    owner: u16,
    client_type: u8,
    app_body_active: bool,
    surface_geometry: impl Fn(u16, u16) -> Option<gp::authority::SurfaceGeometry>,
    owner_windows: impl Fn(u16) -> ([table::OwnerWindow; gp::WM_WINDOWS_MAX], usize),
    comp: &mut Compositor,
    msg: &[u8],
    hs: &[Handle],
) -> bool {
    if hs.len() != 1 {
        close_all(hs);
        st_rep(ch, gp::ST_BAD);
        return false;
    }
    let src = hs[0];
    let Some(gp::PresentReq::Submit { surface_id, flags, rects, rect_count, target_present_ns, .. }) =
        gp::parse_present_req(msg)
    else {
        let _ = berry_handle_close(src);
        st_rep(ch, gp::ST_BAD);
        return false;
    };

    // R2 双几何合约：raw 是客户端 buffer/stride 基准；visible 是遮挡后
    // 可呈现域。APP 窗口两者都来自 WmTable；全局桌面 surface 无遮挡，
    // raw=visible=合成 VMO 全屏。
    let geometry = if surface_id == 0 {
        if client_type != gp::WM_CLIENT_SYSTEM {
            let _ = berry_handle_close(src);
            st_rep(ch, gp::ST_DENIED);
            return false;
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
        return false;
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
        return false;
    }

    // 第一个通过几何/尺寸硬门的真实 SUBMIT 已到达：这是 boot surface 即将
    // 被真实帧替换前的系统真值点，不表示像素已经呈现。
    let system_surface = client_type == gp::WM_CLIENT_SYSTEM && surface_id == 0;
    if system_surface && app_body_active {
        // A focused visible APP owns the body. A stale desktop Opening/cleanup
        // repaint must not cover it; reply BUSY without inventing a present
        // token. HOME/BACK clears focus first, so the true desktop may repaint.
        let _ = berry_handle_close(src);
        st_rep(ch, gp::ST_BUSY);
        return false;
    }
    if system_surface {
        boot_progress_first_submit_once(cli);
    }

    // Launcher SYSTEM bar windows are alpha overlay sources. wmd keeps a READ
    // duplicate so later APP/full-desktop submissions can re-blend the bars
    // after edge-to-edge content has passed underneath them.
    let bar_overlay = client_type == gp::WM_CLIENT_SYSTEM
        && !system_surface
        && (u32::from(raw_w) < comp.w || u32::from(raw_h) < comp.h);
    if bar_overlay {
        let mut overlay_vmo = 0;
        if berry_handle_duplicate(src, RIGHT_READ, &mut overlay_vmo) != 0
            || overlay_vmo == 0
            || !comp.cache_system_overlay(
                owner,
                u16::try_from(surface_id).unwrap_or(0),
                overlay_vmo,
            )
        {
            if overlay_vmo != 0 {
                let _ = berry_handle_close(overlay_vmo);
            }
            let _ = berry_handle_close(src);
            st_rep(ch, gp::ST_BAD);
            return false;
        }
        // Bar sources are cache-only: the first real backend SUBMIT remains
        // surface0's desktop frame, preserving m=10/m=11 BootProgress truth.
        let _ = berry_handle_close(src);
        st_rep(ch, gp::ST_OK);
        return false;
    }

    // 他人（SYSTEM overlay、桌面 surface 或另一个 APP）即将覆盖合成面；
    // 先按 fresh owner 的有效窗口保存真实像素。缓存是 Recents 静态复显
    // 的增强面，失败时走既有“等待 app 自绘”路径。
    if comp.fresh_owner().is_some_and(|fresh| fresh != owner) {
        let fresh = comp.fresh_owner().expect("checked");
        let (windows, window_n) = owner_windows(fresh);
        comp.capture_fresh_owner(windows, window_n);
    }
    comp.clear_fresh_owner();

    // P2 latch probes carry a future target. Draining the backend here would
    // force every deferred frame to present before its successor, destroying
    // depth-1 squash semantics. Normal window updates keep the safe front-write
    // path; future-target submissions stay on the compositor back buffer.
    let deferred = target_present_ns > now_ns();
    if !system_surface && !deferred {
        // Window updates are not the swipe fast path. Drain any queued DMA and
        // update the authoritative front buffer atomically with this submit.
        boot_release_all(cli);
        comp.use_scanout_buffer();
    }

    let full = flags & gp::SUBMIT_F_DAMAGE_FULL != 0;
    let mut out_rects = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
    let mut out_n = 0usize;
    let mut out_flags = flags;
    if !comp.prepare_back_buffer(system_surface && full) {
        comp.invalidate_back_buffer();
        let _ = berry_handle_close(src);
        st_rep(ch, gp::ST_BAD);
        return false;
    }
    let ok = if full {
        out_n = 0;
        // FULL 也是 raw buffer-local 的整窗语义；实际拷贝前先收缩到
        // visible 本地矩形，避免读/写持久遮挡区。下游同样只转发这个
        // 已翻译可见矩形，禁止“窗口 FULL → 整屏 FULL”的放大。
        if !comp.copy_rect(src, src_stride, dst_xy, visible_local) {
            comp.invalidate_back_buffer();
            let _ = berry_handle_close(src);
            st_rep(ch, gp::ST_BAD);
            return false;
        }
        if visible_local.w > 0 && visible_local.h > 0 {
            out_rects[0] = gp::Rect {
                x: dst_xy.0.saturating_add(visible_local.x),
                y: dst_xy.1.saturating_add(visible_local.y),
                w: visible_local.w,
                h: visible_local.h,
            };
            out_n = 1;
            out_flags = 0;
        }
        true
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
        // 拷贝半途失败：目标缓冲（back 或前写路径的 front）内容已偏离
        // 记账，回退全屏继承。前写路径同样失效——back 与被改 front 的
        // 差异域已超出 back_stale 记录。
        comp.invalidate_back_buffer();
        st_rep(ch, gp::ST_BAD);
        return false;
    }

    let visible_overlays = comp.visible_system_overlays(|overlay_owner, overlay_id| {
        surface_geometry(overlay_owner, overlay_id)
    });
    if !comp.apply_system_overlays(&visible_overlays) {
        comp.invalidate_back_buffer();
        st_rep(ch, gp::ST_BAD);
        return false;
    }
    for &(.., x, y, w, h) in visible_overlays.iter() {
        if w == 0 || h == 0 {
            continue;
        }
        // Keep damage conservative under DAMAGE_MAX by collapsing existing
        // writes to their envelope before appending both bar rectangles.
        if out_n == gp::DAMAGE_MAX {
            let envelope = Compositor::envelope(&out_rects);
            out_rects[0] = envelope;
            out_n = 1;
        }
        out_rects[out_n] = gp::Rect { x, y, w, h };
        out_n += 1;
    }

    let (submit_rects, submit_n) = comp.submit_plan(&out_rects, out_n);
    let mut alias: Handle = 0;
    if berry_handle_duplicate(comp.active_vmo(), RIGHT_READ, &mut alias) != 0 || alias == 0 {
        trap();
    }
    let creq = gp::PresentReq::Submit {
        surface_id: 0,
        buffer_id: 0x57444D43,
        flags: out_flags,
        rate_hint: msg.get(13).copied().unwrap_or(0),
        target_present_ns: u64::from_le_bytes(
            msg.get(16..24).unwrap_or(&[0; 8]).try_into().unwrap(),
        ),
        rects: submit_rects,
        rect_count: submit_n,
    };
    let mut req = [0u8; 256];
    let n = match gp::encode_present_req(&mut req, &creq) {
        Some(n) => n,
        None => trap(),
    };
    if system_surface {
        boot_progress_complete_once(cli);
    }
    let (rep, nb, _rhs, nh) = rpc_present(cli, &req[..n], &[alias]);
    let ok = rep.first() == Some(&gp::ST_OK);
    if ok {
        comp.finish_submit(Compositor::envelope(&out_rects[..out_n]));
        comp.note_submit_owner(owner, client_type);
    } else {
        // 聚合提交被后端拒绝：back 已承载完整新合成但未翻转，与 front 的
        // 差异域超出 back_stale 记录——回退全屏继承。
        comp.invalidate_back_buffer();
    }
    if system_surface && nh == 0 {
        // The system desktop producer is waiting on this ACK and has another
        // real sampled position staged. Return directly to it; APP submits stay
        // ordinary replies so app render loops cannot self-flood wmd.
        send_handoff_rep(ch, &rep[..nb]);
    } else {
        send_rep_handles(ch, &rep[..nb], &_rhs[..nh]);
    }
    if ok && system_surface && comp.back_stale_covers_screen() {
        comp.prefetch_back_after_full();
    }
    ok
}

/// Recents 选择隐藏 APP 后，wmd 用 owner 最近真实帧重放合成面。缓存命中
/// 仅在窗口 raw/eff 指纹未变时生效；miss 或几何变化不伪造像素。
pub fn replay_owner(
    cli: Handle,
    comp: &mut Compositor,
    owner: u16,
    windows: [table::OwnerWindow; gp::WM_WINDOWS_MAX],
    window_n: usize,
) -> bool {
    let Some((rects, rect_n)) = comp.replay_owner_pixels(owner, windows, window_n) else {
        println!("WMREPLAY miss owner={owner}");
        return false;
    };
    if rect_n == 0 {
        println!("WMREPLAY miss owner={owner} rects=0");
        return false;
    }
    // wmd 自产提交不占客户端 credits；先排干 held，再提交，成功后立即
    // RETURN-ALL，连续 task_select 不会第三次 ST_FULL。
    boot_release_all(cli);
    let mut alias: Handle = 0;
    if berry_handle_duplicate(comp.active_vmo(), RIGHT_READ, &mut alias) != 0 || alias == 0 {
        trap();
    }
    let creq = gp::PresentReq::Submit {
        surface_id: 0,
        buffer_id: 0x57444D43,
        flags: 0,
        rate_hint: 0,
        target_present_ns: 0,
        rects,
        rect_count: rect_n,
    };
    let mut req = [0u8; 256];
    let n = match gp::encode_present_req(&mut req, &creq) {
        Some(n) => n,
        None => trap(),
    };
    let (rep, _nb, _rhs, nh) = rpc_present(cli, &req[..n], &[alias]);
    let ok = rep.first() == Some(&gp::ST_OK) && nh == 0;
    if ok {
        boot_release_all(cli);
        comp.finish_submit(Compositor::envelope(&rects[..rect_n]));
        comp.note_submit_owner(owner, gp::WM_CLIENT_APP);
        println!("WMREPLAY ok owner={owner} rects={rect_n}");
    } else {
        comp.clear_fresh_owner();
        // 重放帧已写入 back 但未被后端接受：back 内容偏离记账，回退全屏。
        comp.invalidate_back_buffer();
        eprintln!("WMREPLAY bad owner={owner} st={}", rep.first().copied().unwrap_or(0xff));
    }
    ok
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
