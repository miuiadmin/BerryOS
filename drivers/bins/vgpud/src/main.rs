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
use berry_bootprogress::{Face, Milestone, Renderer, Stage, VERBOSE_LOG_MAX};
use berry_guiproto as gp;
use berry_libc::io::{gift2_h, gift3_h, self_vmar_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_event_create,
    berry_handle_close, berry_handle_duplicate, berry_object_signal, berry_object_wait_many,
    berry_object_wait_one, berry_thread_sleep, berry_thread_yield,
    berry_vmo_commit, berry_vmo_create,
    berry_vmar_map, berry_vmo_get_phys, berry_vmo_get_size, berry_vmo_read,
    berry_vmo_write, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, BERRY_ERR_TIMEDOUT,
    CLOCK_MONOTONIC, RIGHT_READ, RIGHT_WRITE, RIGHT_SIGNAL, SIGNAL_READABLE,
    SIGNAL_WRITABLE,
};
use berry_virtio as vio;

#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

const QSIZE: u16 = 16;
const DMA_LEN: u64 = 20_480;
const RING_PAGE: u64 = 0;
const REQ_PAGE: u64 = 4096;
const RESP_PAGE: u64 = 8192;
const BATCH_REQ_PAGE: u64 = 12_288;
const BATCH_RESP_PAGE: u64 = 16_384;
const BATCH_MAX: usize = 8;
const FRAME_VMO_LEN: u64 = 4096;
const RESOURCE_IDS: [u32; 3] = [1, 2, 3];
const SCANOUT_ID: u32 = 0;
const BACKING_POOL: usize = RESOURCE_IDS.len();
/// wait_used 单命令总超时：设备真卡死时的 fail-loud 上限（10s）。
const POLL_CAP_NS: u64 = 10_000_000_000;
/// 睡眠臂单步名义 100us；内核 10Hz slice 下实际被量化为 ~100ms/步，
/// 只作病理兜底——正常命令绝不能落到此臂（60fps 审计 137ms DMA 成因）。
const POLL_STEP_NS: u64 = 100_000;
/// 自旋臂上限 2ms。依据：1080×1920 全幅 2.07MPix/8.3MB 实测 wire
/// ~0.15ms；最坏批量 BATCH_MAX=8 条串行 ≈1.2ms；2ms ≈13× 单幅 wire，
/// 覆盖 TCG 抖动。自旋每轮含 vmo_read+clock 两次 syscall（是 poll 不
/// 是纯空转），故不升到数十 ms 级烧 CPU；超窗才入睡眠臂（≈100ms/步，
/// 命中即在遥测里显形为异常长尾）。
const POLL_SPIN_NS: u64 = 2_000_000;
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
    batch_req_pa: u64,
    batch_resp_pa: u64,
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
    // 分辨率真值来自 virtio-gpu display info：同一驱动服务产品 1080×1920
    // 与 GUI8 TCG 共识拓扑 800×600，禁止写死产品几何。
    let mode = info
        .modes
        .iter()
        .find(|m| m.enabled != 0 && m.rect.width != 0 && m.rect.height != 0)
        .unwrap_or_else(|| {
            println!("vgpu: no enabled display mode");
            trap();
        });
    let mode_w = mode.rect.width;
    let mode_h = mode.rect.height;

    for resource in RESOURCE_IDS {
        ctrl_simple(&d, regs, vio::gpu::GPU_RESOURCE_CREATE_2D_SZ, |b| {
            vio::gpu::put_resource_create_2d(
                b,
                0,
                resource,
                vio::gpu::FORMAT_B8G8R8X8_UNORM,
                mode_w,
                mode_h,
            )
            .is_some()
        });
    }

    attach_frame(&d, regs, &frame, RESOURCE_IDS[0]);

    let rect = vio::gpu::GpuRect { x: 0, y: 0, width: mode_w, height: mode_h };
    ctrl_simple(&d, regs, vio::gpu::GPU_SET_SCANOUT_SZ, |b| {
        vio::gpu::put_set_scanout(b, 0, rect, SCANOUT_ID, RESOURCE_IDS[0]).is_some()
    });

    let probe = vio::gpu::GpuRect { x: 0, y: 0, width: 2, height: 2 };
    ctrl_simple(&d, regs, vio::gpu::GPU_TRANSFER_2D_SZ, |b| {
        vio::gpu::put_transfer_to_host_2d(b, 0, RESOURCE_IDS[0], probe, 0).is_some()
    });
    ctrl_simple(&d, regs, vio::gpu::GPU_RESOURCE_FLUSH_SZ, |b| {
        vio::gpu::put_resource_flush(b, 0, RESOURCE_IDS[0], probe).is_some()
    });

    println!(
        "VGPU ok w={} h={} scanout={} resources={} slot={}",
        mode.rect.width,
        mode.rect.height,
        num_scanouts,
        BACKING_POOL,
        slot
    );
    let present = gift2_h();
    if present != 0 {
        send_rep(present, b"V");
    }
    let face = if std::env::args().any(|arg| arg == "--boot-face=verbose") {
        Face::VerboseText
    } else {
        Face::Capsule
    };
    serve_present(&d, regs, mode_w, mode_h, face);
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
    let mut batch_req_pa = 0u64;
    let mut batch_resp_pa = 0u64;
    if berry_vmo_get_phys(vmo, RING_PAGE, &mut ring_pa) != 0
        || berry_vmo_get_phys(vmo, REQ_PAGE, &mut req_pa) != 0
        || berry_vmo_get_phys(vmo, RESP_PAGE, &mut resp_pa) != 0
        || berry_vmo_get_phys(vmo, BATCH_REQ_PAGE, &mut batch_req_pa) != 0
        || berry_vmo_get_phys(vmo, BATCH_RESP_PAGE, &mut batch_resp_pa) != 0
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
    Dma {
        vmo,
        lay,
        req_pa,
        resp_pa,
        batch_req_pa,
        batch_resp_pa,
    }
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

fn attach_frame(d: &Dma, regs: *mut u32, frame: &Frame, resource: u32) {
    // SAFETY: 单线程独占窗。
    let req: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(REQ) };
    req.fill(0);
    if vio::gpu::put_attach_backing(req, 0, resource, 1).is_none()
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

/// 批量发布 ≤8 条已写入 BATCH_REQ_PAGE 的控制命令，并一次 doorbell/wait。
/// 每条命令仍独立 request→response 描述符链；响应按 512B 槽位落
/// BATCH_RESP_PAGE。调用方必须已按 512B 槽写好 request。
fn ctrl_batch_wait_nodata(d: &Dma, regs: *mut u32, n: usize) {
    if n == 0 || n > BATCH_MAX || 2 * n > QSIZE as usize {
        trap();
    }
    let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(RING) };
    let old = vio::get_u16(ring, d.lay.avail_off + 2).unwrap_or(0);
    for i in 0..n {
        let did = (i * 2) as u16;
        let _ = vio::put_desc(
            ring,
            &d.lay,
            did as usize,
            d.batch_req_pa + (i as u64) * 512,
            vio::gpu::GPU_TRANSFER_2D_SZ as u32,
            vio::DESC_F_NEXT,
            did + 1,
        );
        let _ = vio::put_desc(
            ring,
            &d.lay,
            (did + 1) as usize,
            d.batch_resp_pa + (i as u64) * 512,
            vio::gpu::GPU_CTRL_HDR_SZ as u32,
            vio::DESC_F_WRITE,
            0,
        );
        if vio::publish(ring, &d.lay, did).is_none() {
            trap();
        }
    }
    if berry_vmo_write(d.vmo, ring.as_ptr(), RING_PAGE, 4096) != 0 {
        trap();
    }
    unsafe { core::arch::asm!("dmb ish") };
    wr(regs, vio::mmio::Q_NOTIFY, 0);
    wait_used(d, old.wrapping_add(n as u16));
    let resp: &[u8; 512] = unsafe { &*core::ptr::addr_of!(RESP) };
    for i in 0..n {
        if berry_vmo_read(
            d.vmo,
            resp.as_ptr() as *mut u8,
            BATCH_RESP_PAGE + (i as u64) * 512,
            vio::gpu::GPU_CTRL_HDR_SZ,
        ) != 0
        {
            trap();
        }
        let Some(hdr) = vio::gpu::parse_ctrl_hdr(resp, 0) else {
            println!("vgpu: malformed batch response");
            trap();
        };
        if hdr.typ != vio::gpu::RESP_OK_NODATA {
            println!("vgpu: batch response type={:#x}", hdr.typ);
            trap();
        }
    }
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
    let started = now();
    let deadline = started + POLL_CAP_NS;
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
        // 先在自旋臂内 poll 到完成：窗口必须罩住 1080p 全幅 DMA（实测
        // wire ~0.15ms，依据见 POLL_SPIN_NS）。每轮 = vmo_read+clock 两次
        // syscall，完成即返、不白等；只有真超窗（设备病理慢）才落睡眠臂
        // ——那里被 10Hz slice 量化为 ~100ms/步，命中即遥测可见异常。
        if t < started + POLL_SPIN_NS {
            core::hint::spin_loop();
            continue;
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
const VBLANK_SIG: u64 = gp::HDI_USER_SIGNAL;
const HDI_SLOTS: usize = gp::HDI_SLOTS_MAX as usize;
const MAX_BACKING_PAGES: usize = 4096;
const MAX_ENTRY_PAGES: usize = 16;

#[derive(Clone, Copy)]
struct Backing {
    /// frame VMO READ 别名：钉住全部已 commit 页，防 PA 指纹被页复用
    /// 假命中（内核 VMO 页在任一 handle 存活期不可回收/复用）。
    _pin: Handle,
    /// entries VMO 只持生命周期；物理 PA 已进 descriptor。
    _entries_vmo: Handle,
    resource: u32,
    first_pa: u64,
    last_pa: u64,
    stamp: u64,
}

#[derive(Clone, Copy)]
struct HdiSlot {
    vmo: Handle,
    resource: u32,
    width: u32,
    height: u32,
    stride: u32,
    fence: Handle,
    commit_target: u64,
    committed: bool,
}

struct PendingFlip {
    token: u32,
    full: bool,
    resource: u32,
    /// 本 flip 在 held 表的关联 alias（submit 时随行收入的 buffer 只读
    /// 别名）。flip 完成（DMA 终态）即无在途读者——pump_pending 里
    /// close 掉并释放 credit；held 表因此恒等于「在途 pending flip 集」。
    alias: Handle,
    rects: [gp::Rect; gp::DAMAGE_MAX],
    rect_n: usize,
    flush: gp::Rect,
}

struct PresentSrv {
    dma: Dma,
    regs: *mut u32,
    w: u32,
    h: u32,
    present_seq: u32,
    submit_seq: u32,
    tokens: [(u32, u64, u64, u32, u8); TOKEN_RING],
    /// 在途 flip 的 buffer alias 账（1:1 对应 pending 槽——flip 完成时
    /// `reclaim_held` 自动回收，RELEASE 只兜剩余；count ∈ 0..CREDITS）。
    held: [(u32, Handle); CREDITS],
    held_n: usize,
    pending: [Option<PendingFlip>; CREDITS],
    cb_park: Option<(u32, Handle)>,
    boot_submit_done: bool,
    backings: [Option<Backing>; BACKING_POOL],
    scanout_resource: u32,
    pool_epoch: u64,
    perf_submits: u64,
    perf_submit_ns: u64,
    perf_last_ns: u64,
    perf_dma_ns: u64,
    perf_dma_last_ns: u64,
    perf_reattaches: u64,
    perf_jitter_ns: u64,
    vb_seq: u32,
    vb_next_ns: u64,
    vb_scheduled: bool,
    vb_armed: bool,
    vb_ev: Handle,
    vb_parked: bool,
    vb_park_ch: Handle,
    vb_park_timing: bool,
    slots: [HdiSlot; HDI_SLOTS],
    boot_vmo: Handle,
    boot_va: usize,
    boot_started: bool,
    boot_stage: Stage,
    boot_permille: u16,
    boot_face: Face,
    boot_milestones: [bool; gp::BOOT_M_MAX as usize + 1],
    boot_logs: [&'static [u8]; VERBOSE_LOG_MAX],
    boot_log_n: usize,
    // Software-LTPO arbitration state. QEMU has no panel mode-set; these are
    // the display service's soft-vblank cadence decisions, not hardware claims.
    ltpo_hz: u8,
    ltpo_min_hz: u16,
    ltpo_max_hz: u16,
    ltpo_preferred_hz: u16,
    ltpo_sticky: u8,
    power_mode: u8,
    last_submit_ns: u64,
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

        let submit_started = now();
        let linear_bytes = (self.w as u64) * 4 * self.h as u64;
        let Some(resource) = self.resource_for(hvmo, linear_bytes) else {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_BAD);
            return;
        };

        let fullrect = gp::Rect { x: 0, y: 0, w: self.w, h: self.h };
        let targets: &[gp::Rect] = if full {
            core::slice::from_ref(&fullrect)
        } else {
            &rects[..rect_count]
        };

        // Damage 合并策略按实际拷贝字节数择优：rect 面积和 vs 包络面积。
        // 重叠/碎片重叠时一次包络 TRANSFER+FLUSH 可把 8 次命令降为 2 次；
        // 稀疏小损伤仍保留 rect，避免把整屏拉进 host。
        let mut clipped = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
        let mut clipped_n = 0usize;
        let mut sum_area = 0u64;
        let mut ux0 = u32::MAX;
        let mut uy0 = u32::MAX;
        let mut ux1 = 0u32;
        let mut uy1 = 0u32;
        for r in targets {
            let r = clip(r, self.w, self.h);
            if r.w == 0 || r.h == 0 {
                continue;
            }
            clipped[clipped_n] = r;
            clipped_n += 1;
            sum_area = sum_area.saturating_add(u64::from(r.w) * u64::from(r.h));
            ux0 = ux0.min(r.x);
            uy0 = uy0.min(r.y);
            ux1 = ux1.max(r.x.saturating_add(r.w));
            uy1 = uy1.max(r.y.saturating_add(r.h));
        }
        let union_area = if clipped_n == 0 {
            0
        } else {
            u64::from(ux1 - ux0) * u64::from(uy1 - uy0)
        };
        // Splitting saves bytes but multiplies fixed virtio-gpu command/QEMU
        // scheduling cost. Model that cost as screen_area/12 equivalent pixels
        // per extra rect (1080p field data: six ~0.45MPix bands could cost more
        // than one 2.07MPix full-frame transfer). Prefer the union envelope
        // once its byte saving is below that fixed-cost estimate.
        let per_extra_rect_cost = u64::from(self.w) * u64::from(self.h) / 12;
        let split_cost = sum_area.saturating_add(
            per_extra_rect_cost.saturating_mul(clipped_n.saturating_sub(1) as u64),
        );
        let envelope_is_cheaper = clipped_n > 1 && union_area <= split_cost;
        let plan: &[gp::Rect] = if clipped_n != 0 && (union_area < sum_area || envelope_is_cheaper) {
            clipped[1] = gp::Rect {
                x: ux0,
                y: uy0,
                w: ux1 - ux0,
                h: uy1 - uy0,
            };
            &clipped[1..2]
        } else {
            &clipped[..clipped_n]
        };
        let plan_n = plan.len();
        let mut plan_storage = [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX];
        plan_storage[..plan_n].copy_from_slice(plan);
        let flush = gp::Rect {
            x: ux0,
            y: uy0,
            w: ux1.saturating_sub(ux0),
            h: uy1.saturating_sub(uy0),
        };
        let first_synchronous = !self.boot_submit_done
            && self.pending.iter().flatten().next().is_none()
            && self.held_n == 0;

        self.submit_seq = self.submit_seq.wrapping_add(1);
        let token = self.submit_seq;
        if first_synchronous {
            // Boot/first frame remains flush-at-ack: the boot milestone and the
            // first visible desktop replacement must not race their own truth.
            let dma_started = now();
            self.transfer_and_flush(resource, &plan_storage[..plan_n], plan_n, flush);
            let dma_elapsed = now().saturating_sub(dma_started);
            self.perf_dma_ns = self.perf_dma_ns.saturating_add(dma_elapsed);
            self.perf_dma_last_ns = dma_elapsed;
            let t = now();
            self.present_seq = self.present_seq.wrapping_add(1);
            self.tokens[token as usize % TOKEN_RING] =
                (token, t, u64::from(self.present_seq), 0, 0);
            self.boot_submit_done = true;
            // 同步完成面等价于「已完成的 pending flip」：DMA/token 已终态，
            // alias 无在途读者——inline 回收（close，不入 held）。若把它
            // 留进 held，将没有任何完成事件来回收它（无 PendingFlip 关联），
            // credit 被 boot 帧永占到下一次 RELEASE——时序坑就此封死。
            let _ = berry_handle_close(hvmo);
        } else {
            let Some(slot) = self.pending.iter_mut().find(|p| p.is_none()) else {
                trap();
            };
            *slot = Some(PendingFlip {
                token,
                full,
                resource,
                alias: hvmo,
                rects: plan_storage,
                rect_n: plan_n,
                flush,
            });
            self.held[self.held_n] = (buffer_id, hvmo);
            self.held_n += 1;
        }
        self.perf_submits = self.perf_submits.saturating_add(1);
        self.last_submit_ns = now();
        self.refresh_ltpo("submit");
        let refresh_ns = Self::period_ns(self.ltpo_hz) as u32;
        let token_record = &mut self.tokens[token as usize % TOKEN_RING];
        *token_record = (
            token,
            token_record.1,
            token_record.2,
            refresh_ns,
            if full { gp::FRAME_TIMING_F_PRESENTED | gp::FRAME_TIMING_F_FULL }
            else { gp::FRAME_TIMING_F_PRESENTED },
        );
        let submit_elapsed = now().saturating_sub(submit_started);
        self.perf_last_ns = submit_elapsed;
        self.perf_submit_ns = self.perf_submit_ns.saturating_add(submit_elapsed);
        let mut out = [0u8; 8];
        let n = gp::encode_submit_ack(&mut out, token);
        send_rep(ch, &out[..n]);
        // Start this flip before telemetry formatting: after ACK, wmd/launcher
        // can immediately request the next frame and starve a deferred pump for
        // hundreds of milliseconds during catch-up.
        let _ = self.pump_pending();
        // Telemetry 在 ACK 后打印：`last_us` 是入队/校验耗时；DMA 完成耗时
        // 由 `dma_us` 在完成侧累计，二者不可再混成一个“呈现延迟”。
        let avg_us = self.perf_submit_ns / (self.perf_submits * 1_000);
        let plan_area: u64 = plan_storage
            .iter()
            .take(plan_n)
            .map(|r| u64::from(r.w) * u64::from(r.h))
            .sum();
        let union_area = u64::from(flush.w) * u64::from(flush.h);
        println!(
            "VGPUPERF submits={} avg_us={} last_us={} dma_us={} reattaches={} jitter_us={} rects={} area={} union={}",
            self.perf_submits,
            avg_us,
            self.perf_last_ns / 1_000,
            self.perf_dma_last_ns / 1_000,
            self.perf_reattaches,
            self.perf_jitter_ns / 1_000,
            plan_n,
            plan_area,
            union_area
        );
    }

    /// 返回 (TRANSFER 批量等待耗时, FLUSH 等待耗时)，供 VGPUDMA 遥测
    /// 拆分归因；submit 同步首帧路径不消费，忽略即可。
    fn transfer_and_flush(
        &mut self,
        resource: u32,
        rects: &[gp::Rect],
        rect_n: usize,
        flush: gp::Rect,
    ) -> (u64, u64) {
        // Select only when this flip is about to transfer: an early SUBMIT must
        // not point scanout at a back resource before the older front flip runs.
        self.select_resource(resource);
        for (i, r) in rects.iter().take(rect_n).enumerate() {
            let rect = vio::gpu::GpuRect {
                x: r.x,
                y: r.y,
                width: r.w,
                height: r.h,
            };
            let off = (r.y as u64 * self.w as u64 + r.x as u64) * 4;
            let req: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(REQ) };
            req[..vio::gpu::GPU_TRANSFER_2D_SZ].fill(0);
            if vio::gpu::put_transfer_to_host_2d(req, 0, resource, rect, off).is_none() {
                trap();
            }
            if berry_vmo_write(
                self.dma.vmo,
                req.as_ptr(),
                BATCH_REQ_PAGE + (i as u64) * 512,
                vio::gpu::GPU_TRANSFER_2D_SZ,
            ) != 0
            {
                trap();
            }
        }
        let mut xfer_ns = 0u64;
        if rect_n != 0 {
            let t0 = now();
            ctrl_batch_wait_nodata(&self.dma, self.regs, rect_n);
            xfer_ns = now().saturating_sub(t0);
        }
        let mut flush_ns = 0u64;
        if rect_n != 0 {
            let t0 = now();
            let rect = vio::gpu::GpuRect {
                x: flush.x,
                y: flush.y,
                width: flush.w,
                height: flush.h,
            };
            ctrl_simple(&self.dma, self.regs, vio::gpu::GPU_RESOURCE_FLUSH_SZ, |b| {
                vio::gpu::put_resource_flush(b, 0, resource, rect).is_some()
            });
            flush_ns = now().saturating_sub(t0);
        }
        (xfer_ns, flush_ns)
    }

    fn pending_token(&self, token: u32) -> bool {
        self.pending
            .iter()
            .flatten()
            .any(|p| p.token == token)
    }

    fn send_parked_or_current_callback(&self, token: u32, ch: Handle) {
        let Some(&record) = self
            .tokens
            .iter()
            .find(|&record| record.0 == token && record.0 != 0)
        else {
            trap();
        };
        let presented_ns = record.1;
        let rep = gp::FrameDoneRep {
            status: gp::FRAME_PRESENTED,
            credits: (CREDITS - self.held_n) as u8,
            present_seq: self.present_seq,
            presented_ns,
        };
        let mut out = [0u8; 16];
        let n = gp::encode_frame_done_rep(&mut out, &rep);
        send_callback_rep(ch, &out[..n]);
    }

    /// flip 完成回收：按 PendingFlip 关联的 alias 定位 held 槽，close 别名
    /// 并释放 credit（swap-remove——RELEASE 只按剩余集合归还，无序依赖）。
    /// 找不到关联槽 = 记账破缺，fail loud（不静默泄漏 credit/alias）。
    fn reclaim_held(&mut self, alias: Handle) {
        let Some(i) = self.held[..self.held_n]
            .iter()
            .position(|&(_, h)| h == alias)
        else {
            trap();
        };
        let _ = berry_handle_close(alias);
        self.held_n -= 1;
        self.held[i] = self.held[self.held_n];
    }

    fn pump_pending(&mut self) -> bool {
        let Some(index) = self
            .pending
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.as_ref().map(|_| i))
            .min_by_key(|&i| self.pending[i].as_ref().expect("checked").token)
        else {
            return false;
        };
        let flip = self.pending[index].take().expect("checked");
        let started = now();
        let (xfer_ns, flush_ns) =
            self.transfer_and_flush(flip.resource, &flip.rects, flip.rect_n, flip.flush);
        let dma_elapsed = now().saturating_sub(started);
        self.perf_dma_ns = self.perf_dma_ns.saturating_add(dma_elapsed);
        self.perf_dma_last_ns = dma_elapsed;
        println!(
            "VGPUDMA ok token={} us={} xfer_us={} flush_us={}",
            flip.token,
            dma_elapsed / 1_000,
            xfer_ns / 1_000,
            flush_ns / 1_000
        );
        let t = now();
        self.present_seq = self.present_seq.wrapping_add(1);
        self.refresh_ltpo("age");
        let refresh_ns = Self::period_ns(self.ltpo_hz) as u32;
        self.tokens[flip.token as usize % TOKEN_RING] = (
            flip.token,
            t,
            u64::from(self.present_seq),
            refresh_ns,
            if flip.full {
                gp::FRAME_TIMING_F_PRESENTED | gp::FRAME_TIMING_F_FULL
            } else {
                gp::FRAME_TIMING_F_PRESENTED
            },
        );
        // 完成即回收（在 parked callback 应答**之前**）：alias close +
        // credit 释放一步到位，FRAME_CALLBACK 的 credits 域因此报「回收
        // 后」真值；后到 callback 走 tokens ring 即答，同样见真值。
        self.reclaim_held(flip.alias);
        if let Some((park_token, ch)) = self.cb_park {
            if park_token == flip.token {
                self.cb_park = None;
                self.send_parked_or_current_callback(flip.token, ch);
            }
        }
        true
    }

    /// frame VMO → virtio-gpu resource。命中池只刷新 LRU stamp；未中选
    /// 空位/最旧位，先 DETACH 旧 backing，再重建 2025 页 mem-entry 链。
    fn resource_for(&mut self, hvmo: Handle, bytes: u64) -> Option<u32> {
        let mut first_pa = 0u64;
        let mut last_pa = 0u64;
        if berry_vmo_get_phys(hvmo, 0, &mut first_pa) != 0
            || berry_vmo_get_phys(
                hvmo,
                bytes.saturating_sub(1) & !0xFFF,
                &mut last_pa,
            ) != 0
        {
            return None;
        }
        self.pool_epoch = self.pool_epoch.saturating_add(1);
        let stamp = self.pool_epoch;
        let hit = self.backings.iter().position(|b| {
            b.as_ref()
                .is_some_and(|b| b.first_pa == first_pa && b.last_pa == last_pa)
        });
        if let Some(i) = hit {
            if let Some(backing) = self.backings[i].as_mut() {
                backing.stamp = stamp;
                return Some(backing.resource);
            }
        }

        let target = self
            .backings
            .iter()
            .position(|b| b.is_none())
            .unwrap_or_else(|| {
                self.backings
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, b)| b.as_ref().map(|b| b.stamp).unwrap_or(0))
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });
        let resource = RESOURCE_IDS[target];
        // Never rebind a resource while a queued flip still references it. The
        // stable product uses exactly pool-sized backings, but an HDI prepare
        // can introduce pressure; drain that flip before DETACH/ATTACH.
        if self
            .pending
            .iter()
            .flatten()
            .any(|flip| flip.resource == resource)
        {
            while self.pump_pending() {}
        }
        let old = self.backings[target].take();
        let pool_empty = self.backings.iter().all(Option::is_none);
        // 只有两种状态需要 DETACH：池表中的旧 backing，或 resource 1 首
        // 次换下启动 4KiB 自验 backing。resource 2/3 尚未 ATTACH，空槽
        // DETACH 在 QEMU 返回 0x1200 并会杀死服务。
        if old.is_some() || (pool_empty && resource == RESOURCE_IDS[0]) {
            ctrl_simple(
                &self.dma,
                self.regs,
                vio::gpu::GPU_BACKING_CMD_SZ,
                |b| vio::gpu::put_detach_backing(b, 0, resource).is_some(),
            );
        }
        if let Some(old) = old {
            let _ = berry_handle_close(old._entries_vmo);
            let _ = berry_handle_close(old._pin);
        }
        self.perf_reattaches = self.perf_reattaches.saturating_add(1);
        self.attach_frame_backing(hvmo, resource, stamp, first_pa, last_pa, bytes);
        Some(resource)
    }

    /// 切换 scanout 指向。TRANSFER 前设置，保证同拍完整资源呈现。
    fn select_resource(&mut self, resource: u32) {
        if self.scanout_resource == resource {
            return;
        }
        let rect = vio::gpu::GpuRect {
            x: 0,
            y: 0,
            width: self.w,
            height: self.h,
        };
        ctrl_simple(&self.dma, self.regs, vio::gpu::GPU_SET_SCANOUT_SZ, |b| {
            vio::gpu::put_set_scanout(b, 0, rect, SCANOUT_ID, resource).is_some()
        });
        self.scanout_resource = resource;
    }

    fn attach_frame_backing(
        &mut self,
        hvmo: Handle,
        resource: u32,
        stamp: u64,
        first_pa: u64,
        last_pa: u64,
        bytes: u64,
    ) {
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
        let mut pin = 0 as Handle;
        if berry_vmo_create(entries_len, 0, &mut entries_vmo) != 0
            || entries_vmo == 0
            || berry_vmo_commit(entries_vmo, 0, entries_len) != 0
            || berry_handle_duplicate(hvmo, RIGHT_READ, &mut pin) != 0
            || pin == 0
        {
            let _ = berry_handle_close(entries_vmo);
            let _ = berry_handle_close(pin);
            println!("vgpu: entries/pin VMO failure");
            trap();
        }

        // SAFETY: 单线程独占窗。
        let page: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(ENTRY_PAGE) };
        let mut in_page = 0usize;
        let mut out_page = 0usize;
        for i in 0..pages {
            let mut pa = 0u64;
            if berry_vmo_get_phys(hvmo, (i as u64) * 4096, &mut pa) != 0 {
                println!("vgpu: frame phys page={}", i);
                trap();
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
        if vio::gpu::put_attach_backing(req, 0, resource, pages as u32).is_none() {
            trap();
        }
        ctrl_chain(
            &self.dma,
            self.regs,
            vio::gpu::GPU_BACKING_CMD_SZ,
            &pas[..entry_pages],
        );
        let target = RESOURCE_IDS.iter().position(|&id| id == resource).unwrap_or(0);
        self.backings[target] = Some(Backing {
            _pin: pin,
            _entries_vmo: entries_vmo,
            resource,
            first_pa,
            last_pa,
            stamp,
        });
    }

    fn refresh_ltpo(&mut self, reason: &str) {
        let t = now();
        let idle_ns = t.saturating_sub(self.last_submit_ns);
        let hz = gp::software_ltpo_hz(
            self.ltpo_min_hz,
            self.ltpo_max_hz,
            self.ltpo_preferred_hz,
            self.ltpo_sticky,
            self.power_mode,
            idle_ns,
        );
        let old_hz = self.ltpo_hz;
        self.ltpo_hz = hz;
        if hz == 0 {
            self.vb_scheduled = false;
        } else if self.vb_scheduled && hz > old_hz && old_hz != 0 {
            // An upward transition must not wait out a prior 1Hz/10Hz slot.
            self.vb_next_ns = t + Self::period_ns(hz);
        }
        if hz != old_hz {
            println!("VGPULTPO ok hz={hz} old={old_hz} reason={reason}");
        }
    }

    fn period_ns(hz: u8) -> u64 {
        if hz == 0 {
            return 0;
        }
        1_000_000_000 / u64::from(hz)
    }

    fn vsync_schedule(&mut self) {
        if !self.vb_scheduled {
            if self.ltpo_hz == 0 {
                return;
            }
            self.vb_next_ns = now() + Self::period_ns(self.ltpo_hz);
            self.vb_scheduled = true;
        }
    }

    fn tick_vsync(&mut self) {
        if !self.vb_scheduled {
            return;
        }
        let t = now();
        if t < self.vb_next_ns {
            return;
        }
        let point = self.vb_next_ns;
        self.perf_jitter_ns = self.perf_jitter_ns.max(t.saturating_sub(point));
        if self.vb_parked {
            let seq = self.vb_seq.wrapping_add(1);
            if self.vb_park_timing {
                let rep = gp::VblankTimingRep {
                    seq: u64::from(seq),
                    ts_ns: point,
                    refresh_ns: Self::period_ns(self.ltpo_hz) as u32,
                };
                let mut out = [0u8; 24];
                let n = gp::encode_vblank_timing_rep(&mut out, &rep);
                send_rep(self.vb_park_ch, &out[..n]);
            } else {
                let rep = gp::VblankRep { seq, ts_ns: point };
                let mut out = [0u8; 16];
                let n = gp::encode_vblank_rep(&mut out, &rep);
                send_rep(self.vb_park_ch, &out[..n]);
            }
            self.vb_parked = false;
            self.vb_park_ch = 0;
            self.vb_park_timing = false;
        }
        if self.vb_armed && self.vb_ev != 0 {
            let _ = berry_object_signal(self.vb_ev, VBLANK_SIG, VBLANK_SIG);
        }
        // COMMIT 在本拍或 target 后首拍真正呈现：select resource → 全幅
        // TRANSFER → FLUSH → fence。fence 因此是“像素已到 host”的完成
        // 信号，不再是纯记账。target=0 = 下一拍。
        let mut pending = [None; HDI_SLOTS];
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.committed && slot.commit_target <= point {
                pending[i] = Some(*slot);
            }
        }
        for (i, pending_slot) in pending.iter().enumerate() {
            let Some(slot) = *pending_slot else { continue };
            let resource = slot.resource;
            self.select_resource(resource);
            let rect = vio::gpu::GpuRect {
                x: 0,
                y: 0,
                width: slot.width,
                height: slot.height,
            };
            ctrl_simple(&self.dma, self.regs, vio::gpu::GPU_TRANSFER_2D_SZ, |b| {
                vio::gpu::put_transfer_to_host_2d(b, 0, resource, rect, 0).is_some()
            });
            ctrl_simple(&self.dma, self.regs, vio::gpu::GPU_RESOURCE_FLUSH_SZ, |b| {
                vio::gpu::put_resource_flush(b, 0, resource, rect).is_some()
            });
            if slot.fence != 0 {
                let _ = berry_object_signal(slot.fence, VBLANK_SIG, VBLANK_SIG);
            }
            self.slots[i].committed = false;
            self.present_seq = self.present_seq.wrapping_add(1);
        }
        self.vb_seq = self.vb_seq.wrapping_add(1);
        self.refresh_ltpo("age");
        let period = Self::period_ns(self.ltpo_hz);
        if period == 0 {
            self.vb_scheduled = false;
            return;
        }
        let mut next = point + period;
        while next <= t {
            next += period;
        }
        self.vb_next_ns = next;
        if !self.vb_armed && !self.vb_parked {
            self.vb_scheduled = false;
        }
    }

    fn handle_hdi(&mut self, ch: Handle, msg: &[u8], hs: &[Handle]) {
        match msg.first() {
            Some(&gp::OP_REG_VBLANK) => match gp::parse_reg_vblank_req(msg) {
                Some(1) => {
                    if hs.len() != 1 {
                        close_all(hs);
                        st_rep(ch, gp::ST_BAD);
                        return;
                    }
                    if self.vb_ev != 0 {
                        let _ = berry_handle_close(self.vb_ev);
                    }
                    self.vb_ev = hs[0];
                    self.vb_armed = true;
                    self.vsync_schedule();
                    st_rep(ch, gp::ST_OK);
                }
                Some(0) => {
                    if self.vb_ev != 0 {
                        let _ = berry_handle_close(self.vb_ev);
                        self.vb_ev = 0;
                    }
                    close_all(hs);
                    self.vb_armed = false;
                    if !self.vb_parked {
                        self.vb_scheduled = false;
                    }
                    st_rep(ch, gp::ST_OK);
                }
                _ => {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                }
            },
            Some(&gp::OP_VBLANK_WAIT) if gp::parse_vblank_wait_req(msg).is_some() => {
                if !hs.is_empty() || self.vb_parked {
                    close_all(hs);
                    st_rep(ch, gp::ST_BUSY);
                    return;
                }
                self.vb_parked = true;
                self.vb_park_ch = ch;
                self.vb_park_timing = false;
                self.vsync_schedule();
            }
            Some(&gp::OP_VBLANK_TIMING) if gp::parse_vblank_timing_req(msg).is_some() => {
                if !hs.is_empty() || self.vb_parked {
                    close_all(hs);
                    st_rep(ch, gp::ST_BUSY);
                    return;
                }
                self.vb_parked = true;
                self.vb_park_ch = ch;
                self.vb_park_timing = true;
                self.vsync_schedule();
            }
            Some(&gp::OP_PREPARE) => match gp::parse_prepare_req(msg) {
                Some(req) => self.prepare(ch, hs, req),
                None => {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                }
            },
            Some(&gp::OP_COMMIT) => match gp::parse_commit_req(msg) {
                Some((slot, target)) => {
                    if !hs.is_empty() {
                        close_all(hs);
                        st_rep(ch, gp::ST_BAD);
                        return;
                    }
                    if self.slots[slot as usize].vmo == 0 {
                        st_rep(ch, gp::ST_NOENT);
                        return;
                    }
                    self.slots[slot as usize].commit_target = target;
                    self.slots[slot as usize].committed = true;
                    self.vsync_schedule();
                    st_rep(ch, gp::ST_OK);
                }
                None => {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                }
            },
            Some(&gp::OP_PANEL_CAPS) if gp::parse_panel_caps_req(msg).is_some() => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                let cfg = gp::softpanel::PanelCfg::DEFAULT;
                let mut tiers = [gp::PanelTier {
                    hz: 0,
                    class: 0,
                    stage: 0,
                    period_ns: 0,
                }; gp::PANEL_TIERS_MAX];
                for (i, tier) in gp::softpanel::TIERS.iter().enumerate() {
                    tiers[i] = gp::PanelTier {
                        hz: tier.hz,
                        class: tier.class,
                        stage: tier.stage,
                        period_ns: tier.period_ns as u32,
                    };
                }
                let (mut min_hz, mut max_hz) = (u16::MAX, 0u16);
                for tier in gp::softpanel::TIERS.iter() {
                    min_hz = min_hz.min(tier.hz);
                    max_hz = max_hz.max(tier.hz);
                }
                let rep = gp::PanelCapsRep {
                    version: 2,
                    rate_model: gp::RATE_MODEL_TIERED,
                    flags: if cfg.per_frame { gp::PANEL_FLAG_PER_FRAME } else { 0 },
                    min_hz,
                    max_hz,
                    granularity_hz: 0,
                    vrr: cfg.vrr,
                    current_hz: u16::from(self.ltpo_hz),
                    current_period_ns: Self::period_ns(self.ltpo_hz) as u32,
                    current_phase_ns: 0,
                    tiers,
                    n_tiers: gp::softpanel::TIERS.len() as u8,
                };
                let mut out = [0u8; gp::PANEL_CAPS_REP_MAX];
                let n = gp::encode_panel_caps_rep(&mut out, &rep);
                send_rep(ch, &out[..n]);
            }
            _ => {
                close_all(hs);
                st_rep(ch, gp::ST_BAD);
            }
        }
    }

    fn prepare(&mut self, ch: Handle, hs: &[Handle], req: gp::PrepareReq) {
        if hs.len() != 1
            || req.format != gp::FMT_XRGB8888
            || req.width == 0
            || req.width > self.w
            || req.height == 0
            || req.height > self.h
            || req.stride != req.width.checked_mul(4).unwrap_or(u32::MAX)
        {
            close_all(hs);
            st_rep(ch, if hs.len() == 1 { gp::ST_UNSUPPORTED } else { gp::ST_BAD });
            return;
        }
        let hvmo = hs[0];
        let mut sz = 0u64;
        if berry_vmo_get_size(hvmo, &mut sz) != 0
            || sz < (req.stride as u64) * req.height as u64
        {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        let linear_bytes = (req.stride as u64) * req.height as u64;
        let Some(resource) = self.resource_for(hvmo, linear_bytes) else {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_BAD);
            return;
        };
        let mut slot = self.slots.iter().position(|s| s.vmo == 0);
        if slot.is_none() {
            slot = self.slots.iter().position(|s| {
                s.width == req.width && s.height == req.height && s.stride == req.stride
            });
            if let Some(i) = slot {
                let old = &mut self.slots[i];
                let _ = berry_handle_close(old.vmo);
                let _ = berry_handle_close(old.fence);
                old.vmo = 0;
                old.fence = 0;
            }
        }
        let Some(i) = slot else {
            let _ = berry_handle_close(hvmo);
            st_rep(ch, gp::ST_FULL);
            return;
        };
        let mut fence = 0 as Handle;
        let mut fence_signal = 0 as Handle;
        if berry_event_create(0, &mut fence) != 0
            || fence == 0
            || berry_handle_duplicate(fence, RIGHT_SIGNAL, &mut fence_signal) != 0
            || fence_signal == 0
        {
            let _ = berry_handle_close(hvmo);
            let _ = berry_handle_close(fence);
            let _ = berry_handle_close(fence_signal);
            st_rep(ch, gp::ST_FULL);
            return;
        }
        self.slots[i] = HdiSlot {
            vmo: hvmo,
            resource,
            width: req.width,
            height: req.height,
            stride: req.stride,
            fence: fence_signal,
            commit_target: 0,
            committed: false,
        };
        let mut out = [0u8; 4];
        let n = gp::encode_prepare_rep(&mut out, i as u8);
        send_rep_handles(ch, &out[..n], &[fence]);
    }

    fn boot_stage(raw: u8) -> Option<Stage> {
        match raw {
            gp::BOOT_STAGE_DISPLAY => Some(Stage::Display),
            gp::BOOT_STAGE_VERIFY => Some(Stage::Verify),
            gp::BOOT_STAGE_KERNEL => Some(Stage::Kernel),
            gp::BOOT_STAGE_RAMDISK => Some(Stage::Ramdisk),
            gp::BOOT_STAGE_PREPARE => Some(Stage::Prepare),
            gp::BOOT_STAGE_HANDOFF => Some(Stage::Handoff),
            _ => None,
        }
    }

    /// BOOT-P1：把系统阶段真值渲染到 vgpu 专属 boot surface。该 surface
    /// 不进入 app credits，不改变 present submit 语义；第一帧 wmd SUBMIT
    /// 会自然切走 scanout。渲染 VMO 由 vgpud 私有，app 无能力接触。
    fn boot_progress(&mut self, raw_stage: u8, permille: u16, milestone: u8) -> bool {
        let Some(stage) = Self::boot_stage(raw_stage) else {
            return false;
        };
        // Milestone 0 is the legacy absent form. Recover its id from the
        // permille truth table so old producers remain valid.
        let truth = if milestone == 0 {
            Milestone::ALL.iter().copied().find(|m| m.permille() == permille)
        } else {
            Milestone::from_id(milestone)
        };
        let Some(truth) = truth else {
            println!(
                "VGPUBOOT fail stage=milestone kind=unknown id={} permille={}",
                milestone, permille
            );
            return false;
        };
        if stage != truth.stage() || permille != truth.permille() {
            println!(
                "VGPUBOOT fail stage=milestone kind=triple id={} stage={} permille={}",
                milestone, raw_stage, permille
            );
            return false;
        }
        let id = truth.id() as usize;
        if id >= self.boot_milestones.len() || self.boot_milestones[id] {
            println!(
                "VGPUBOOT fail stage=milestone kind=dup id={} permille={}",
                milestone, permille
            );
            return false;
        }
        let bytes = (self.w as u64) * 4 * self.h as u64;
        // 800×600 线性帧是 468.75 页；VMO 创建/commit 面必须整页，
        // 渲染/resource 指纹仍用精确线性长度。
        let Some(vmo_bytes) = bytes.checked_add(4095).map(|v| v & !4095) else {
            return false;
        };
        let Some(len) = usize::try_from(vmo_bytes).ok() else {
            return false;
        };
        let Some(frame_len) = usize::try_from(bytes).ok() else {
            return false;
        };
        if self.boot_vmo == 0 {
            let hv = self_vmar_h();
            let mut vmo = 0 as Handle;
            let mut va = 0x7900_0000usize;
            if hv == 0
                || berry_vmo_create(vmo_bytes, 0, &mut vmo) != 0
                || vmo == 0
                || berry_vmo_commit(vmo, 0, vmo_bytes) != 0
                || berry_vmar_map(
                    hv,
                    0,
                    vmo,
                    0,
                    &mut va,
                    len,
                    RIGHT_READ | RIGHT_WRITE,
                ) != 0
                || va != 0x7900_0000
            {
                let _ = berry_handle_close(vmo);
                println!("VGPUBOOT fail stage=map");
                return false;
            }
            self.boot_vmo = vmo;
            self.boot_va = va;
        }
        // Build the prospective log view without consuming the milestone. A
        // map/render/backing/flush failure must leave the id retryable and
        // must not leak a ghost diagnostic into a later frame.
        let mut logs = self.boot_logs;
        let mut log_n = self.boot_log_n;
        if log_n == logs.len() {
            logs.copy_within(1.., 0);
            log_n -= 1;
        }
        logs[log_n] = truth.log_line();
        log_n += 1;
        let first = !self.boot_started;
        let fb = unsafe { core::slice::from_raw_parts_mut(self.boot_va as *mut u8, frame_len) };
        let rendered = if self.boot_started {
            Renderer::restore(
                fb,
                self.w as usize,
                self.h as usize,
                (self.w as usize) * 4,
                // XRGB8888 byte order; raw 0 maps to Rgbx.pack.
                0,
                self.boot_permille,
                self.boot_stage,
            )
            .map(|mut r| {
                let ok = if self.boot_face == Face::VerboseText {
                    r.update_verbose(stage, permille, &logs[..log_n])
                } else {
                    r.update(stage, permille)
                };
                let (stage, permille) = r.prev_state();
                self.boot_stage = stage;
                if ok {
                    self.boot_permille = permille;
                }
                ok
            })
            .unwrap_or(false)
        } else {
            Renderer::new(
                fb,
                self.w as usize,
                self.h as usize,
                (self.w as usize) * 4,
                // XRGB8888 byte order; raw 0 maps to Rgbx.pack.
                0,
            )
            .map(|mut r| {
                if self.boot_face == Face::VerboseText {
                    r.initialize_verbose(stage, permille, &logs[..log_n]);
                } else {
                    r.initialize(stage, permille);
                }
                self.boot_stage = stage;
                self.boot_permille = permille;
                true
            })
            .unwrap_or(false)
        };
        if !rendered {
            println!("VGPUBOOT fail stage=render");
            return false;
        }
        let Some(resource) = self.resource_for(self.boot_vmo, bytes) else {
            println!("VGPUBOOT fail stage=backing");
            return false;
        };
        self.select_resource(resource);
        // 首帧必须带 logo/整面背景；后续 renderer 只改 bar/percent/stage，
        // 底部条带 transfer/flush 比 1080p 整面少约 6× 字节。
        let rect = if first || self.boot_face == Face::VerboseText {
            vio::gpu::GpuRect {
                x: 0,
                y: 0,
                width: self.w,
                height: self.h,
            }
        } else if let Some(l) = berry_bootprogress::layout(
            self.w as usize,
            self.h as usize,
        ) {
            let top = l.bar_y.saturating_sub(2);
            let bottom = (l.bar_y + l.bar_h + 2).min(self.h as usize);
            let y = top.min(bottom);
            vio::gpu::GpuRect {
                x: 0,
                y: y as u32,
                width: self.w,
                height: (bottom - y) as u32,
            }
        } else {
            vio::gpu::GpuRect {
                x: 0,
                y: 0,
                width: self.w,
                height: self.h,
            }
        };
        let off = u64::from(rect.y) * (self.w as u64 * 4);
        ctrl_simple(&self.dma, self.regs, vio::gpu::GPU_TRANSFER_2D_SZ, |b| {
            vio::gpu::put_transfer_to_host_2d(b, 0, resource, rect, off).is_some()
        });
        ctrl_simple(&self.dma, self.regs, vio::gpu::GPU_RESOURCE_FLUSH_SZ, |b| {
            vio::gpu::put_resource_flush(b, 0, resource, rect).is_some()
        });
        self.boot_milestones[id] = true;
        self.boot_logs = logs;
        self.boot_log_n = log_n;
        if self.boot_face == Face::VerboseText {
            println!(
                "VGPUBOOT ok stage={} permille={} rect={}x{}@{} m={} face=v",
                raw_stage,
                permille,
                rect.width,
                rect.height,
                rect.y,
                truth.id()
            );
        } else {
            println!(
                "VGPUBOOT ok stage={} permille={} rect={}x{}@{} m={}",
                raw_stage,
                permille,
                rect.width,
                rect.height,
                rect.y,
                truth.id()
            );
        }
        self.boot_started = true;
        true
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
                    version_present: gp::VER_PRESENT,
                    version_input: gp::VER_INPUT,
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
                if self.pending_token(frame_token) {
                    if self.cb_park.is_some() {
                        // Present is a serial request/response channel. A second
                        // parked callback would overwrite and strand the first;
                        // fail loudly instead of violating that invariant.
                        st_rep(ch, gp::ST_BUSY);
                        return;
                    }
                    self.cb_park = Some((frame_token, ch));
                    return;
                }
                let hit = self
                    .tokens
                    .iter()
                    .find(|&&record| record.0 == frame_token && record.0 != 0)
                    .map(|&record| record.1);
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
                        send_callback_rep(ch, &out[..n]);
                    }
                }
            }
            Some(gp::PresentReq::ReleaseWait) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                // RELEASE is the recoverability boundary. If an async flip is
                // still in flight, finish it before returning any alias. Flip
                // 完成已在 pump_pending 内自动回收（held 恒只剩在途项）；
                // 此处归还**剩余** alias，count ∈ 0..CREDITS——常态排干后
                // 为 0（合法形：客户端按 count 收，wmd/launcher 零假设）。
                while self.pump_pending() {}
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
            Some(gp::PresentReq::Vote {
                sticky,
                scene: _,
                min_hz,
                max_hz,
                preferred_hz,
            }) => {
                close_all(hs);
                self.ltpo_min_hz = min_hz;
                self.ltpo_max_hz = max_hz;
                self.ltpo_preferred_hz = preferred_hz;
                self.ltpo_sticky = sticky;
                self.refresh_ltpo("vote");
                st_rep(ch, gp::ST_OK);
            }
            Some(gp::PresentReq::PowerMode { mode }) => {
                close_all(hs);
                self.power_mode = mode;
                self.refresh_ltpo("power");
                st_rep(ch, gp::ST_OK);
            }
            Some(gp::PresentReq::InputPulse) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                self.last_submit_ns = now();
                self.refresh_ltpo("input");
                st_rep(ch, gp::ST_OK);
            }
            Some(gp::PresentReq::FrameTiming { frame_token }) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                let Some(record) = self
                    .tokens
                    .iter()
                    .find(|&&record| record.0 == frame_token && record.0 != 0)
                    .copied()
                else {
                    st_rep(ch, gp::ST_NOENT);
                    return;
                };
                let rep = gp::FrameTimingRep {
                    frame_token: record.0,
                    presented_ns: record.1,
                    seq: record.2,
                    refresh_ns: record.3,
                    flags: record.4,
                };
                let mut out = [0u8; 28];
                let n = gp::encode_frame_timing_rep(&mut out, &rep);
                send_rep(ch, &out[..n]);
            }
            Some(gp::PresentReq::BootProgress { stage, permille, milestone }) => {
                if !hs.is_empty() {
                    close_all(hs);
                    st_rep(ch, gp::ST_BAD);
                    return;
                }
                if self.boot_submit_done {
                    // A real frame owns scanout. A late or replayed system
                    // milestone is acknowledged as idempotent no-op rather
                    // than treated as a display-control failure.
                    st_rep(ch, gp::ST_OK);
                    return;
                }
                let ok = self.boot_progress(stage, permille, milestone);
                st_rep(ch, if ok { gp::ST_OK } else { gp::ST_BUSY });
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

fn serve_present(d: &Dma, regs: *mut u32, w: u32, h: u32, face: Face) -> ! {
    let present = gift2_h();
    let hdi = gift3_h();
    if present == 0 || hdi == 0 {
        berry_libc::park();
    }
    let mut srv = PresentSrv {
        dma: Dma {
            vmo: d.vmo,
            lay: d.lay,
            req_pa: d.req_pa,
            resp_pa: d.resp_pa,
            batch_req_pa: d.batch_req_pa,
            batch_resp_pa: d.batch_resp_pa,
        },
        regs,
        w,
        h,
        present_seq: 0,
        submit_seq: 0,
        tokens: [(0, 0, 0, 0, 0); TOKEN_RING],
        held: [(0, 0); CREDITS],
        held_n: 0,
        pending: [None, None],
        cb_park: None,
        boot_submit_done: false,
        backings: [None; BACKING_POOL],
        scanout_resource: RESOURCE_IDS[0],
        pool_epoch: 0,
        perf_submits: 0,
        perf_submit_ns: 0,
        perf_last_ns: 0,
        perf_dma_ns: 0,
        perf_dma_last_ns: 0,
        perf_reattaches: 0,
        perf_jitter_ns: 0,
        vb_seq: 0,
        vb_next_ns: 0,
        vb_scheduled: false,
        vb_armed: false,
        vb_ev: 0,
        vb_parked: false,
        vb_park_ch: 0,
        vb_park_timing: false,
        boot_vmo: 0,
        boot_va: 0,
        boot_started: false,
        boot_stage: Stage::Display,
        boot_permille: 0,
        boot_face: face,
        boot_milestones: [false; gp::BOOT_M_MAX as usize + 1],
        boot_logs: [b""; VERBOSE_LOG_MAX],
        boot_log_n: 0,
        ltpo_hz: 60,
        ltpo_min_hz: 0,
        ltpo_max_hz: 60,
        ltpo_preferred_hz: 60,
        ltpo_sticky: 0,
        power_mode: gp::POWER_NORMAL,
        last_submit_ns: 0,
        slots: [HdiSlot {
            vmo: 0,
            resource: 0,
            width: 0,
            height: 0,
            stride: 0,
            fence: 0,
            commit_target: 0,
            committed: false,
        }; HDI_SLOTS],
    };
    srv.last_submit_ns = now();
    let items = [present, hdi];
    let sigs = [SIGNAL_READABLE, SIGNAL_READABLE];
    let mut obs = [0u64; 2];
    let mut ready_idx = 0usize;
    loop {
        srv.tick_vsync();
        let mut handled = false;
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
                srv.handle_hdi(ch, &msg[..nb], &hs[..nh]);
            }
            handled = true;
        }
        if handled {
            continue;
        }
        // No queued request remains: run one queued DMA flip. A second SUBMIT
        // already waiting on the present channel was handled above before this
        // point, preserving the intended two-frame pipeline.
        if srv.pump_pending() {
            continue;
        }
        let t = now();
        let mut deadline = t + IDLE_CAP_NS;
        if srv.vb_scheduled && srv.vb_next_ns < deadline {
            deadline = srv.vb_next_ns;
        }
        let st = berry_object_wait_many(
            items.as_ptr(),
            sigs.as_ptr(),
            2,
            deadline,
            &mut ready_idx,
            obs.as_mut_ptr(),
        );
        if st != 0 && st != BERRY_ERR_TIMEDOUT {
            trap();
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

/// FRAME_CALLBACK-only cooperative handoff. The channel fast path records the
/// sole woken wmd relay; yielding here returns the completion without waiting
/// for another fair round. Other replies (notably SUBMIT ACK) stay ordinary.
fn send_callback_rep(h: Handle, rep: &[u8]) {
    send_rep(h, rep);
    let _ = berry_thread_yield();
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
