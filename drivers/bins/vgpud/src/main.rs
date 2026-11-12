//! vgpud —— virtio-gpu 2D 显示驱动 Phase 2（真实 ctrlq 命令链）。
//!
//! 本刀不是展示补丁：它把 ADR #50/#51 预留的第二显示后端先接到真实
//! virtio-gpu 设备上，验证 BerryOS 的 MMIO/negotiate/VMO DMA 框架足以承载
//! GPU 控制队列。自验链：
//!
//! ```text
//! GET_DISPLAY_INFO
//! → RESOURCE_CREATE_2D(1080×1920, BGRX8888)
//! → RESOURCE_ATTACH_BACKING(VMO)
//! → SET_SCANOUT
//! → TRANSFER_TO_HOST_2D
//! → RESOURCE_FLUSH
//! → VGPU ok w=1080 h=1920 scanout=1
//! ```
//!
//! 自验通过后向 svcmgr 发一帧 ready，再接管 present 域：SUBMIT 随行
//! frame VMO 首次按物理页 ATTACH_BACKING，后续 damage 直接
//! TRANSFER_TO_HOST_2D + RESOURCE_FLUSH。设备缺席时不发 ready，svcmgr
//! 回落 guifbd/ramfb；验证 TCG 拓扑零新标记。

use berry_exec::{VIRTIO_MMIO_SLOTS, VIRTIO_MMIO_STRIDE, VIRTIO_MMIO_VA};
use berry_guiproto as gp;
use berry_libc::io::{gift2_h, gift3_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_object_wait_many, berry_object_wait_one, berry_thread_sleep, berry_vmo_commit,
    berry_vmo_create, berry_vmo_get_phys, berry_vmo_get_size, berry_vmo_read, berry_vmo_write,
    Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_virtio as vio;

#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

const QSIZE: u16 = 16;
const DMA_LEN: u64 = 12_288;
const RING_PAGE: u64 = 0;
const REQ_PAGE: u64 = 4096;
const RESP_PAGE: u64 = 8192;
const FRAME_VMO_LEN: u64 = 4096;
const RESOURCE_ID: u32 = 1;
const SCANOUT_ID: u32 = 0;
const EXPECT_W: u32 = 1080;
const EXPECT_H: u32 = 1920;
const POLL_CAP_NS: u64 = 10_000_000_000;
const POLL_STEP_NS: u64 = 1_000_000;
const RESET_CAP_NS: u64 = 1_000_000_000;

static mut RING: [u8; 4096] = [0; 4096];
static mut REQ: [u8; 512] = [0; 512];
static mut RESP: [u8; 512] = [0; 512];
static mut ENTRY_PAGE: [u8; 4096] = [0; 4096];

struct Dma {
    vmo: Handle,
    lay: vio::RingLayout,
    req_pa: u64,
    resp_pa: u64,
}

fn main() {
    let Some(slot) = find_device() else {
        berry_libc::park();
    };
    let regs = mmio(slot);
    negotiate(regs);
    let d = setup_queue(regs);
    let frame = setup_frame();

    wr(
        regs,
        vio::mmio::STATUS,
        vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK | vio::ST_DRIVER_OK,
    );

    // config 对齐 u32 读（HVF no-ISV 教训已在 gui-inputd 定谳）。
    let num_scanouts = rd(regs, vio::mmio::CONFIG + 8);
    if num_scanouts == 0 || num_scanouts > vio::gpu::GPU_MAX_SCANOUTS as u32 {
        println!("vgpu: scanouts={num_scanouts}");
        trap();
    }

    let info = get_display_info(&d, regs);
    let mode = info
        .modes
        .iter()
        .find(|m| m.enabled != 0 && m.rect.width == EXPECT_W && m.rect.height == EXPECT_H)
        .unwrap_or_else(|| {
            println!("vgpu: no enabled 1080x1920 mode");
            trap();
        });

    ctrl_simple(&d, regs, vio::gpu::GPU_RESOURCE_CREATE_2D_SZ, |b| {
        vio::gpu::put_resource_create_2d(
            b,
            0,
            RESOURCE_ID,
            vio::gpu::FORMAT_B8G8R8X8_UNORM,
            EXPECT_W,
            EXPECT_H,
        )
        .is_some()
    });

    attach_frame(&d, regs, &frame);

    let rect = vio::gpu::GpuRect { x: 0, y: 0, width: EXPECT_W, height: EXPECT_H };
    ctrl_simple(&d, regs, vio::gpu::GPU_SET_SCANOUT_SZ, |b| {
        vio::gpu::put_set_scanout(b, 0, rect, SCANOUT_ID, RESOURCE_ID).is_some()
    });

    let probe = vio::gpu::GpuRect { x: 0, y: 0, width: 2, height: 2 };
    ctrl_simple(&d, regs, vio::gpu::GPU_TRANSFER_2D_SZ, |b| {
        vio::gpu::put_transfer_to_host_2d(b, 0, RESOURCE_ID, probe, 0).is_some()
    });
    ctrl_simple(&d, regs, vio::gpu::GPU_RESOURCE_FLUSH_SZ, |b| {
        vio::gpu::put_resource_flush(b, 0, RESOURCE_ID, probe).is_some()
    });

    println!(
        "VGPU ok w={} h={} scanout={} resource={} slot={}",
        mode.rect.width,
        mode.rect.height,
        num_scanouts,
        RESOURCE_ID,
        slot
    );
    let present = gift2_h();
    if present != 0 {
        send_rep(present, b"V");
    }
    serve_present(&d, regs, EXPECT_W, EXPECT_H);
}

fn find_device() -> Option<usize> {
    let base = VIRTIO_MMIO_VA as *const u32;
    let mut found = None;
    let mut n = 0usize;
    for slot in 0..VIRTIO_MMIO_SLOTS {
        let regs = unsafe { base.add(slot * (VIRTIO_MMIO_STRIDE / 4)) };
        if unsafe { core::ptr::read_volatile(regs) } != vio::mmio::MAGIC_VAL {
            break;
        }
        let did = unsafe { core::ptr::read_volatile(regs.add(2)) };
        if did == vio::mmio::DEVICE_ID_GPU {
            n += 1;
            found = Some(slot);
        }
    }
    if n != 1 {
        return None;
    }
    let slot = found?;
    let regs = mmio(slot);
    let ver = unsafe { core::ptr::read_volatile((regs as *const u32).add(vio::mmio::VERSION / 4)) };
    if ver != vio::mmio::VERSION_MODERN {
        println!("vgpu: slot {} version={}", slot, ver);
        trap();
    }
    Some(slot)
}

fn mmio(slot: usize) -> *mut u32 {
    (VIRTIO_MMIO_VA + (slot * VIRTIO_MMIO_STRIDE) as u64) as *mut u32
}

fn rd(regs: *mut u32, off: usize) -> u32 {
    unsafe { core::ptr::read_volatile((regs as *const u32).add(off / 4)) }
}

fn wr(regs: *mut u32, off: usize, v: u32) {
    unsafe { core::ptr::write_volatile(regs.add(off / 4), v) }
}

fn negotiate(regs: *mut u32) {
    wr(regs, vio::mmio::STATUS, 0);
    let mut tick = now();
    let deadline = tick + RESET_CAP_NS;
    while rd(regs, vio::mmio::STATUS) != 0 {
        if tick >= deadline {
            println!("vgpu: STATUS reset timeout");
            trap();
        }
        if berry_thread_sleep(tick + POLL_STEP_NS) != 0 {
            trap();
        }
        tick = now();
    }
    wr(regs, vio::mmio::STATUS, vio::ST_ACKNOWLEDGE | vio::ST_DRIVER);
    wr(regs, vio::mmio::DEV_FEAT_SEL, 1);
    let f1 = rd(regs, vio::mmio::DEV_FEAT);
    if f1 & vio::FEAT_VERSION_1 == 0 {
        println!("vgpu: VERSION_1 absent f1={:#x}", f1);
        trap();
    }
    wr(regs, vio::mmio::DRV_FEAT_SEL, 0);
    wr(regs, vio::mmio::DRV_FEAT, 0);
    wr(regs, vio::mmio::DRV_FEAT_SEL, 1);
    wr(regs, vio::mmio::DRV_FEAT, vio::FEAT_VERSION_1);
    wr(
        regs,
        vio::mmio::STATUS,
        vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK,
    );
    if rd(regs, vio::mmio::STATUS)
        != vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK
    {
        println!("vgpu: FEATURES_OK rejected");
        trap();
    }
}

fn setup_queue(regs: *mut u32) -> Dma {
    let Some(lay) = vio::RingLayout::new(QSIZE as usize) else { trap() };
    let mut vmo = 0 as Handle;
    if berry_vmo_create(DMA_LEN, 0, &mut vmo) != 0
        || vmo == 0
        || berry_vmo_commit(vmo, 0, DMA_LEN) != 0
    {
        println!("vgpu: DMA VMO failure");
        trap();
    }
    let mut ring_pa = 0u64;
    let mut req_pa = 0u64;
    let mut resp_pa = 0u64;
    if berry_vmo_get_phys(vmo, RING_PAGE, &mut ring_pa) != 0
        || berry_vmo_get_phys(vmo, REQ_PAGE, &mut req_pa) != 0
        || berry_vmo_get_phys(vmo, RESP_PAGE, &mut resp_pa) != 0
    {
        println!("vgpu: DMA phys failure");
        trap();
    }
    wr(regs, vio::mmio::Q_SEL, 0);
    if rd(regs, vio::mmio::Q_NUM_MAX) < QSIZE as u32 {
        println!("vgpu: ctrlq too small");
        trap();
    }
    wr(regs, vio::mmio::Q_NUM, QSIZE as u32);
    wr(regs, vio::mmio::Q_READY, 0);
    wr(regs, vio::mmio::Q_DESC_LOW, ring_pa as u32);
    wr(regs, vio::mmio::Q_DESC_HIGH, (ring_pa >> 32) as u32);
    wr(regs, vio::mmio::Q_AVAIL_LOW, (ring_pa + lay.avail_off as u64) as u32);
    wr(
        regs,
        vio::mmio::Q_AVAIL_HIGH,
        ((ring_pa + lay.avail_off as u64) >> 32) as u32,
    );
    wr(regs, vio::mmio::Q_USED_LOW, (ring_pa + lay.used_off as u64) as u32);
    wr(
        regs,
        vio::mmio::Q_USED_HIGH,
        ((ring_pa + lay.used_off as u64) >> 32) as u32,
    );
    wr(regs, vio::mmio::Q_READY, 1);
    if rd(regs, vio::mmio::Q_READY) != 1 {
        println!("vgpu: Q_READY failure");
        trap();
    }
    Dma { vmo, lay, req_pa, resp_pa }
}

struct Frame {
    /// 帧后备 VMO 本体不读取：字段存在即持有生命周期到进程 park。
    _vmo: Handle,
    pa: u64,
}

fn setup_frame() -> Frame {
    let mut vmo = 0 as Handle;
    if berry_vmo_create(FRAME_VMO_LEN, 0, &mut vmo) != 0
        || vmo == 0
        || berry_vmo_commit(vmo, 0, FRAME_VMO_LEN) != 0
    {
        println!("vgpu: frame VMO failure");
        trap();
    }
    // 2×2 BGRX pattern：黑、白、主题蓝、白——只作 TRANSFER/FLUSH 真链载荷。
    let pattern = [
        0x00u8, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0x18u8, 0x10, 0x28, 0xff, 0xff,
        0xff, 0xff, 0xff,
    ];
    if berry_vmo_write(vmo, pattern.as_ptr(), 0, pattern.len()) != 0 {
        println!("vgpu: frame pattern failure");
        trap();
    }
    let mut pa = 0u64;
    if berry_vmo_get_phys(vmo, 0, &mut pa) != 0 {
        println!("vgpu: frame phys failure");
        trap();
    }
    Frame { _vmo: vmo, pa }
}

fn get_display_info(d: &Dma, regs: *mut u32) -> vio::gpu::GpuDisplayInfo {
    // SAFETY: 单线程独占窗。
    let req: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(REQ) };
    if vio::gpu::put_ctrl_hdr(
        req,
        0,
        &vio::gpu::GpuCtrlHdr::command(vio::gpu::CMD_GET_DISPLAY_INFO),
    )
    .is_none()
    {
        trap();
    }
    let resp = ctrl_raw(d, regs, vio::gpu::GPU_CTRL_HDR_SZ, vio::gpu::GPU_DISPLAY_INFO_SZ);
    let Some(parsed) = vio::gpu::parse_display_info(resp) else {
        println!("vgpu: malformed display info");
        trap();
    };
    parsed
}

fn attach_frame(d: &Dma, regs: *mut u32, frame: &Frame) {
    // SAFETY: 单线程独占窗。
    let req: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(REQ) };
    req.fill(0);
    if vio::gpu::put_attach_backing(req, 0, RESOURCE_ID, 1).is_none()
        || vio::gpu::put_mem_entry(req, vio::gpu::GPU_BACKING_CMD_SZ as usize, frame.pa, 4096)
            .is_none()
    {
        trap();
    }
    ctrl_expect_nodata(
        d,
        regs,
        vio::gpu::GPU_BACKING_CMD_SZ as usize + vio::gpu::GPU_MEM_ENTRY_SZ,
    );
}

fn ctrl_simple(
    d: &Dma,
    regs: *mut u32,
    req_len: usize,
    encode: impl FnOnce(&mut [u8; 512]) -> bool,
) {
    // SAFETY: 单线程独占窗。
    let req: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(REQ) };
    req[..req_len].fill(0);
    if !encode(req) {
        trap();
    }
    ctrl_expect_nodata(d, regs, req_len);
}

fn ctrl_expect_nodata(d: &Dma, regs: *mut u32, req_len: usize) {
    let resp = ctrl_raw(d, regs, req_len, vio::gpu::GPU_CTRL_HDR_SZ);
    let Some(hdr) = vio::gpu::parse_ctrl_hdr(resp, 0) else {
        println!("vgpu: malformed response");
        trap();
    };
    if hdr.typ != vio::gpu::RESP_OK_NODATA {
        println!("vgpu: response type={:#x}", hdr.typ);
        trap();
    }
}

fn ctrl_raw<'a>(
    d: &Dma,
    regs: *mut u32,
    req_len: usize,
    resp_len: usize,
) -> &'a [u8] {
    // SAFETY: 单线程独占窗；返回缓冲在下一命令前由调用方消费完。
    let req: &[u8; 512] = unsafe { &*core::ptr::addr_of!(REQ) };
    let resp: &[u8; 512] = unsafe { &*core::ptr::addr_of!(RESP) };
    let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(RING) };
    if req_len > req.len() || resp_len > resp.len() {
        trap();
    }
    if berry_vmo_write(d.vmo, req.as_ptr(), REQ_PAGE, req_len) != 0
        || berry_vmo_write(d.vmo, resp.as_ptr(), RESP_PAGE, resp_len) != 0
    {
        trap();
    }
    let _ = vio::put_desc(
        ring,
        &d.lay,
        0,
        d.req_pa,
        req_len as u32,
        vio::DESC_F_NEXT,
        1,
    );
    let _ = vio::put_desc(
        ring,
        &d.lay,
        1,
        d.resp_pa,
        resp_len as u32,
        vio::DESC_F_WRITE,
        0,
    );
    let expect = {
        let old = vio::get_u16(ring, d.lay.avail_off + 2).unwrap_or(0);
        if vio::publish(ring, &d.lay, 0).is_none() {
            trap();
        }
        old.wrapping_add(1)
    };
    if berry_vmo_write(d.vmo, ring.as_ptr(), RING_PAGE, 4096) != 0 {
        trap();
    }
    // SAFETY: 纯发布屏障。
    unsafe { core::arch::asm!("dmb ish") };
    wr(regs, vio::mmio::Q_NOTIFY, 0);
    wait_used(d, expect);
    if berry_vmo_read(d.vmo, resp.as_ptr() as *mut u8, RESP_PAGE, resp_len) != 0 {
        trap();
    }
    &resp[..resp_len]
}

fn wait_used(d: &Dma, expect: u16) {
    let deadline = now() + POLL_CAP_NS;
    loop {
        let mut u = [0u8; 2];
        if berry_vmo_read(
            d.vmo,
            u.as_mut_ptr(),
            RING_PAGE + d.lay.used_off as u64 + 2,
            2,
        ) != 0
        {
            trap();
        }
        if u16::from_le_bytes(u) == expect {
            return;
        }
        let t = now();
        if t >= deadline {
            println!("vgpu: ctrlq timeout idx={}", expect);
            trap();
        }
        if berry_thread_sleep(t + POLL_STEP_NS) != 0 {
            trap();
        }
    }
}

fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}

#[cold]
fn trap() -> ! {
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}


// ---------- Phase 3：present 域真实 virtio-gpu 后端 ----------

const TOKEN_RING: usize = 8;
const CREDITS: usize = gp::CREDITS_MAX as usize;
const IDLE_CAP_NS: u64 = 10_000_000_000;
const SEND_REARM_NS: u64 = 10_000_000_000;
const MAX_BACKING_PAGES: usize = 4096;
const MAX_ENTRY_PAGES: usize = 16;

struct Backing {
    /// entries VMO 只持生命周期；物理 PA 已进 descriptor。
    _entries_vmo: Handle,
    first_pa: u64,
    _pages: usize,
}

struct PresentSrv {
    dma: Dma,
    regs: *mut u32,
    w: u32,
    h: u32,
    present_seq: u32,
    submit_seq: u32,
    tokens: [(u32, u64); TOKEN_RING],
    held: [(u32, Handle); CREDITS],
    held_n: usize,
    backing: Option<Backing>,
}

impl PresentSrv {
    fn submit(
        &mut self,
        ch: Handle,
        hs: &[Handle],
        buffer_id: u32,
        flags: u8,
        rects: &[gp::Rect; gp::DAMAGE_MAX],
        rect_count: usize,
    ) {
        if hs.len() != 1 {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        let hvmo = hs[0];
        let mut sz = 0u64;
        if berry_vmo_get_size(hvmo, &mut sz) != 0
            || sz < (self.w as u64) * 4 * self.h as u64
        {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        let full = flags & gp::SUBMIT_F_DAMAGE_FULL != 0;
        if (rect_count > 0 && full) || (rect_count == 0 && !full) {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        if self.held_n >= CREDITS {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_FULL);
            return;
        }

        // Backing 物理页只在首帧/换 VMO 时重建；同一 client frame VMO 的
        // fresh alias 每帧到达，不重复 2025 次 get_phys/attach。
        let mut first_pa = 0u64;
        if berry_vmo_get_phys(hvmo, 0, &mut first_pa) != 0 {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        if self.backing.as_ref().map(|b| b.first_pa) != Some(first_pa) {
            // Phase 2 自验已给 resource 附过 4KiB 测试 backing；完整帧
            // 换背前显式 DETACH，并回收旧 entries VMO。
            ctrl_simple(
                &self.dma,
                self.regs,
                vio::gpu::GPU_BACKING_CMD_SZ,
                |b| vio::gpu::put_detach_backing(b, 0, RESOURCE_ID).is_some(),
            );
            if let Some(old) = self.backing.take() {
                let _ = berry_handle_close(old._entries_vmo);
            }
            self.attach_frame_backing(hvmo);
        }

        let fullrect = gp::Rect { x: 0, y: 0, w: self.w, h: self.h };
        let targets: &[gp::Rect] = if full {
            core::slice::from_ref(&fullrect)
        } else {
            &rects[..rect_count]
        };
        for r in targets {
            let r = clip(r, self.w, self.h);
            if r.w == 0 || r.h == 0 {
                continue;
            }
            let rect = vio::gpu::GpuRect {
                x: r.x,
                y: r.y,
                width: r.w,
                height: r.h,
            };
            let off = (r.y as u64 * self.w as u64 + r.x as u64) * 4;
            ctrl_simple(&self.dma, self.regs, vio::gpu::GPU_TRANSFER_2D_SZ, |b| {
                vio::gpu::put_transfer_to_host_2d(b, 0, RESOURCE_ID, rect, off).is_some()
            });
            ctrl_simple(&self.dma, self.regs, vio::gpu::GPU_RESOURCE_FLUSH_SZ, |b| {
                vio::gpu::put_resource_flush(b, 0, RESOURCE_ID, rect).is_some()
            });
        }

        let t = now();
        self.present_seq = self.present_seq.wrapping_add(1);
        self.submit_seq = self.submit_seq.wrapping_add(1);
        let token = self.submit_seq;
        self.tokens[token as usize % TOKEN_RING] = (token, t);
        self.held[self.held_n] = (buffer_id, hvmo);
        self.held_n += 1;
        let mut out = [0u8; 8];
        let n = gp::encode_submit_ack(&mut out, token);
        send_rep(ch, &out[..n]);
    }

    fn attach_frame_backing(&mut self, hvmo: Handle) {
        let bytes = self.w as u64 * 4 * self.h as u64;
        let pages = ((bytes + 4095) / 4096) as usize;
        if pages == 0 || pages > MAX_BACKING_PAGES {
            println!("vgpu: backing pages={}", pages);
            trap();
        }
        let entry_pages = (pages * vio::gpu::GPU_MEM_ENTRY_SZ + 4095) / 4096;
        if entry_pages > MAX_ENTRY_PAGES || entry_pages + 2 > QSIZE as usize {
            println!("vgpu: entry pages={}", entry_pages);
            trap();
        }
        let entries_len = (entry_pages * 4096) as u64;
        let mut entries_vmo = 0 as Handle;
        if berry_vmo_create(entries_len, 0, &mut entries_vmo) != 0
            || entries_vmo == 0
            || berry_vmo_commit(entries_vmo, 0, entries_len) != 0
        {
            println!("vgpu: entries VMO failure");
            trap();
        }

        // SAFETY: 单线程独占窗。
        let page: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(ENTRY_PAGE) };
        let mut in_page = 0usize;
        let mut out_page = 0usize;
        let mut first_pa = 0u64;
        for i in 0..pages {
            let mut pa = 0u64;
            if berry_vmo_get_phys(hvmo, (i as u64) * 4096, &mut pa) != 0 {
                println!("vgpu: frame phys page={}", i);
                trap();
            }
            if i == 0 {
                first_pa = pa;
            }
            if vio::gpu::put_mem_entry(
                page,
                in_page * vio::gpu::GPU_MEM_ENTRY_SZ,
                pa,
                4096,
            )
            .is_none()
            {
                trap();
            }
            in_page += 1;
            if in_page == 4096 / vio::gpu::GPU_MEM_ENTRY_SZ {
                if berry_vmo_write(
                    entries_vmo,
                    page.as_ptr(),
                    (out_page as u64) * 4096,
                    4096,
                ) != 0
                {
                    trap();
                }
                in_page = 0;
                out_page += 1;
            }
        }
        if in_page != 0
            && berry_vmo_write(
                entries_vmo,
                page.as_ptr(),
                (out_page as u64) * 4096,
                in_page * vio::gpu::GPU_MEM_ENTRY_SZ,
            ) != 0
        {
            trap();
        }

        let mut pas = [0u64; MAX_ENTRY_PAGES];
        for i in 0..entry_pages {
            if berry_vmo_get_phys(entries_vmo, (i as u64) * 4096, &mut pas[i]) != 0 {
                println!("vgpu: entries phys page={}", i);
                trap();
            }
        }
        // SAFETY: 单线程独占窗。
        let req: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(REQ) };
        if vio::gpu::put_attach_backing(req, 0, RESOURCE_ID, pages as u32).is_none() {
            trap();
        }
        ctrl_chain(
            &self.dma,
            self.regs,
            vio::gpu::GPU_BACKING_CMD_SZ,
            &pas[..entry_pages],
        );
        self.backing = Some(Backing { _entries_vmo: entries_vmo, first_pa, _pages: pages });
    }

    fn handle_present(&mut self, ch: Handle, msg: &[u8], hs: &[Handle]) {
        match gp::parse_present_req(msg) {
            Some(gp::PresentReq::Getinfo) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                let rep = gp::GetinfoRep {
                    version_present: 1,
                    version_input: 1,
                    version_hdi: 1,
                    format_bits: gp::FMT_XRGB8888,
                    display_w: self.w,
                    display_h: self.h,
                    max_credits: gp::CREDITS_MAX,
                };
                let mut out = [0u8; gp::PRESENT_REP_MAX];
                let n = gp::encode_getinfo_rep(&mut out, &rep);
                send_rep(ch, &out[..n]);
            }
            Some(gp::PresentReq::Submit {
                surface_id: _,
                buffer_id,
                flags,
                rate_hint: _,
                target_present_ns: _,
                rects,
                rect_count,
            }) => self.submit(ch, hs, buffer_id, flags, &rects, rect_count),
            Some(gp::PresentReq::FrameCallback { frame_token }) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                let hit = self
                    .tokens
                    .iter()
                    .find(|&&(t, _)| t == frame_token && t != 0)
                    .map(|&(_, ns)| ns);
                match hit {
                    None => st_rep(ch, gp::ST_NOENT),
                    Some(presented_ns) => {
                        let rep = gp::FrameDoneRep {
                            status: gp::FRAME_PRESENTED,
                            credits: (CREDITS - self.held_n) as u8,
                            present_seq: self.present_seq,
                            presented_ns,
                        };
                        let mut out = [0u8; 16];
                        let n = gp::encode_frame_done_rep(&mut out, &rep);
                        send_rep(ch, &out[..n]);
                    }
                }
            }
            Some(gp::PresentReq::ReleaseWait) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                let k = self.held_n;
                let mut ids = [0u32; gp::RELEASE_MAX];
                let mut hout = [0u32; gp::RELEASE_MAX];
                for i in 0..k {
                    ids[i] = self.held[i].0;
                    hout[i] = self.held[i].1;
                }
                self.held_n = 0;
                let mut out = [0u8; 4 + 4 * gp::RELEASE_MAX];
                match gp::encode_release_rep(&mut out, &ids[..k]) {
                    Some(n) => send_rep_handles(ch, &out[..n], &hout[..k]),
                    None => trap(),
                }
            }
            Some(gp::PresentReq::Vote { .. }) => {
                close_all(hs);
                st_rep(ch, gp::ST_OK);
            }
            Some(gp::PresentReq::PowerMode { .. }) => {
                close_all(hs);
                st_rep(ch, gp::ST_OK);
            }
            None => {
                close_all(hs);
                st_rep(ch, gp::ST_BAD);
            }
        }
    }
}

fn clip(r: &gp::Rect, w: u32, h: u32) -> gp::Rect {
    let x0 = r.x.min(w);
    let y0 = r.y.min(h);
    let x1 = r.x.saturating_add(r.w).min(w);
    let y1 = r.y.saturating_add(r.h).min(h);
    gp::Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
}

fn serve_present(d: &Dma, regs: *mut u32, w: u32, h: u32) -> ! {
    let present = gift2_h();
    let hdi = gift3_h();
    if present == 0 || hdi == 0 {
        berry_libc::park();
    }
    let mut srv = PresentSrv {
        dma: Dma { vmo: d.vmo, lay: d.lay, req_pa: d.req_pa, resp_pa: d.resp_pa },
        regs,
        w,
        h,
        present_seq: 0,
        submit_seq: 0,
        tokens: [(0, 0); TOKEN_RING],
        held: [(0, 0); CREDITS],
        held_n: 0,
        backing: None,
    };
    let items = [present, hdi];
    let sigs = [SIGNAL_READABLE, SIGNAL_READABLE];
    let mut obs = [0u64; 2];
    let mut ready_idx = 0usize;
    loop {
        let _ = berry_object_wait_many(
            items.as_ptr(),
            sigs.as_ptr(),
            2,
            now() + IDLE_CAP_NS,
            &mut ready_idx,
            obs.as_mut_ptr(),
        );
        for (ch, is_present) in [(present, true), (hdi, false)] {
            let mut msg = [0u8; 256];
            let mut hs = [0 as Handle; 8];
            let mut nb = msg.len();
            let mut nh = hs.len();
            let st = berry_channel_recv(
                ch,
                msg.as_mut_ptr(),
                &mut nb,
                hs.as_mut_ptr(),
                &mut nh,
            );
            if st == BERRY_ERR_AGAIN {
                continue;
            }
            if st != 0 {
                trap();
            }
            if is_present {
                srv.handle_present(ch, &msg[..nb], &hs[..nh]);
            } else {
                // Phase 3 产品主链不消费 hdi；保留通道并 loud 拒未知请求。
                close_all(&hs[..nh]);
                st_rep(ch, gp::ST_UNSUPPORTED);
            }
        }
    }
}

fn send_rep(h: Handle, rep: &[u8]) {
    loop {
        let st = berry_channel_send(h, rep.as_ptr(), rep.len(), core::ptr::null(), 0);
        if st == 0 {
            return;
        }
        if st != BERRY_ERR_BUSY {
            trap();
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, now() + SEND_REARM_NS, &mut obs);
    }
}

fn send_rep_handles(h: Handle, rep: &[u8], hs: &[Handle]) {
    loop {
        let st = berry_channel_send(h, rep.as_ptr(), rep.len(), hs.as_ptr(), hs.len());
        if st == 0 {
            return;
        }
        if st != BERRY_ERR_BUSY {
            trap();
        }
        let mut obs = 0u64;
        let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, now() + SEND_REARM_NS, &mut obs);
    }
}

fn st_rep(h: Handle, st: u8) {
    let mut out = [0u8; 1];
    let n = gp::encode_status_rep(&mut out, st);
    send_rep(h, &out[..n]);
}

fn close_all(hs: &[Handle]) {
    for &h in hs {
        let _ = berry_handle_close(h);
    }
}

/// ATTACH_BACKING 的 request payload 跨 descriptor：头 32B + N 个 4KiB
/// mem-entry 页（QEMU 把链视作连续请求流；无需物理连续大命令缓冲）。
fn ctrl_chain(d: &Dma, regs: *mut u32, head_len: usize, entry_pas: &[u64]) {
    // SAFETY: 单线程独占窗。
    let req: &[u8; 512] = unsafe { &*core::ptr::addr_of!(REQ) };
    let resp: &[u8; 512] = unsafe { &*core::ptr::addr_of!(RESP) };
    let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(RING) };
    if berry_vmo_write(d.vmo, req.as_ptr(), REQ_PAGE, head_len) != 0
        || berry_vmo_write(
            d.vmo,
            resp.as_ptr(),
            RESP_PAGE,
            vio::gpu::GPU_CTRL_HDR_SZ,
        ) != 0
    {
        trap();
    }
    let mut did = 0usize;
    let mut next = 1usize;
    let _ = vio::put_desc(
        ring,
        &d.lay,
        did,
        d.req_pa,
        head_len as u32,
        vio::DESC_F_NEXT,
        next as u16,
    );
    did = next;
    next += 1;
    for (i, &pa) in entry_pas.iter().enumerate() {
        let last_entry = i + 1 == entry_pas.len();
        let flags = if last_entry { vio::DESC_F_NEXT } else { vio::DESC_F_NEXT };
        let _ = vio::put_desc(ring, &d.lay, did, pa, 4096, flags, next as u16);
        did = next;
        next += 1;
    }
    let _ = vio::put_desc(
        ring,
        &d.lay,
        did,
        d.resp_pa,
        vio::gpu::GPU_CTRL_HDR_SZ as u32,
        vio::DESC_F_WRITE,
        0,
    );
    let expect = {
        let old = vio::get_u16(ring, d.lay.avail_off + 2).unwrap_or(0);
        if vio::publish(ring, &d.lay, 0).is_none() {
            trap();
        }
        old.wrapping_add(1)
    };
    if berry_vmo_write(d.vmo, ring.as_ptr(), RING_PAGE, 4096) != 0 {
        trap();
    }
    // SAFETY: 发布屏障。
    unsafe { core::arch::asm!("dmb ish") };
    wr(regs, vio::mmio::Q_NOTIFY, 0);
    wait_used(d, expect);
    let mut hdr = [0u8; vio::gpu::GPU_CTRL_HDR_SZ];
    if berry_vmo_read(d.vmo, hdr.as_mut_ptr(), RESP_PAGE, hdr.len()) != 0 {
        trap();
    }
    match vio::gpu::parse_ctrl_hdr(&hdr, 0) {
        Some(h) if h.typ == vio::gpu::RESP_OK_NODATA => {}
        Some(h) => {
            println!("vgpu: attach response={:#x}", h.typ);
            trap();
        }
        None => trap(),
    }
}
