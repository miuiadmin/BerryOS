//! vblk —— P6 A-3 virtio-blk 用户态块设备驱动（真设备 IO 第一刀）。
//!
//! vprobe 同链（musl target + berry-libc shim；MMIO 窗口经
//! `SpawnSpec::mmio_vmo` 映到 [`VIRTIO_MMIO_VA`]，berry-exec 单源）。
//! vprobe 只读规范头四字，本程序把**整条设备 IO 链**搬到用户态：
//! modern v2 特性协商（收 `VIRTIO_F_VERSION_1`）→ split virtqueue 建立
//! （qsize=16 整环落一页）→ blk 请求三描述符链（hdr/data/status）→
//! LBA0 读魔数验 + LBA1 写/读回逐字节对拍 → 自打 `VBLK ok cap=… slot=…`
//! （rootfs 相第 13 标记）→ main 返回 = exit(0)，svcmgr reap。
//!
//! **DMA 面 = 纯 VMO 机制**（微内核纪律的完整演示）：`vmo_create/commit`
//! 自建 2 页缓冲 VMO——页 0 = virtqueue（desc/avail/used），页 1 = 请求
//! 缓冲（hdr@0 data@16 status@528）；ring 构造与收割走 `vmo_write/vmo_read`
//! （**该 VMO 从不进任何地址空间**——无需 vmar 能力、无映射即无清理面），
//! 描述符地址用新 0x1B `vmo_get_phys` 查页后备 PA（qsize=16 时环与缓冲各
//! 自单页 → 无跨页连续性需求，帧分配器无连续保证也无碍——逐 desc 各持
//! 自己的 PA）。
//!
//! 一致性口径（诚实缺口）：发布排序已执法——门铃（Q_NOTIFY）前 `dmb ish`
//! （berry-virtio publish() 调用方契约：此前 Normal 后备帧写 → Device 门
//! 铃写排序；TCG 无内存模型恒绿，真机缺此 = 设备可取陈旧 avail.idx 假超
//! 时）。仍留 v2 的缺口：cache 维护（`dc cvac` 清写/`dc ivac` 失效读——
//! TCG 无 cache 模型天然一致，随 v2 DMA API 收口）。中断面：poll used
//! ring（10s 帽 + 1ms 睡步进），GIC 从不使能 virtio SPI → 无派发污染；
//! InterruptStatus 读 + ACK 只是整洁面。v1 无 IOMMU 板上 PA 披露 = 已授
//! MMIO 窗口的设备本可 DMA 全内存（ABI 注记），v1 接受并文档化。

use berry_exec::{VIRTIO_MMIO_SLOTS, VIRTIO_MMIO_STRIDE, VIRTIO_MMIO_VA};
use berry_sys::{
    berry_clock_read, berry_handle_close, berry_thread_sleep, berry_vmo_commit, berry_vmo_create,
    berry_vmo_get_phys, berry_vmo_read, berry_vmo_write, Handle, CLOCK_MONOTONIC,
};
use berry_virtio as vio;

// 链接种子（stdhello/vprobe 同款）：无引用则 berry-libc 的 crt0 不进链接。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// smoke/justfile 落盘印章（LBA0 首 12B，`printf | dd conv=notrunc`）。
const LBA0_MAGIC: &[u8; 12] = b"BERRYOSVBLK!";
/// LBA1 写入 pattern（写后读回对拍——16B 即可证字节落盘往返）。
const LBA1_PATTERN: &[u8; 16] = b"berryos-vblk-w1!";
/// 盘容断言：1MiB / 512B = 2048 扇区（smoke/run-rootfs 造 1MiB 盘）。
const EXPECT_SECTORS: u64 = 2048;
/// 队列深（QEMU virtio-blk QueueNumMax 远大于此；2 的幂、整环落一页）。
const QSIZE: u16 = 16;
/// DMA VMO 几何：页 0 = virtqueue，页 1 = 请求缓冲（hdr 16 + data 512 +
/// status 1 ≤ 529B，全在页内——desc 地址域不跨页）。
const DMA_LEN: u64 = 8192;
const RING_PAGE: u64 = 0;
const BUF_PAGE: u64 = 4096;
const HDR_OFF: u64 = BUF_PAGE + 0;
const DATA_OFF: u64 = BUF_PAGE + vio::BLK_HDR_SZ as u64;
const STATUS_OFF: u64 = BUF_PAGE + 528;
/// poll 帽：10s（设备完成在 ms 级——超帽 = 传输层/几何断了，loud）。
const POLL_CAP_NS: u64 = 10_000_000_000;
/// poll 步进：1ms 绝对 deadline 睡（让出 CPU，不空转烧 slice）。
const POLL_STEP_NS: u64 = 1_000_000;
/// STATUS 复位回读帽：1s（真机复位 ms 级；QEMU 同步即 0）。
const RESET_CAP_NS: u64 = 1_000_000_000;

// SAFETY: 调用方单线程（main 顺序流），两窗独占；static 免 4KiB 栈帧。
/// virtqueue 暂存页（整页 vmo_write——avail.idx 状态留在这，跨请求续用）。
static mut RING: [u8; 4096] = [0; 4096];
/// 请求缓冲暂存（hdr + data + status，529B 用量）。
static mut IOB: [u8; 544] = [0; 544];

/// 扫 32 槽定位唯一 block 设备（vprobe 同款判据；细节见 vprobe 头注）。
fn find_device() -> usize {
    // SAFETY: 窗口经 spawn 映射（Device-nGnRnE）；逐字 volatile；只读
    // ID 头四字 + 特性/队列寄存器（写面在协商序内，见 main）。
    let base = VIRTIO_MMIO_VA as *const u32;
    let mut occupied: Option<usize> = None;
    let mut n = 0usize;
    for slot in 0..VIRTIO_MMIO_SLOTS {
        let regs = unsafe { base.add(slot * (VIRTIO_MMIO_STRIDE / 4)) };
        if unsafe { core::ptr::read_volatile(regs) } != vio::mmio::MAGIC_VAL {
            println!("vblk: slot {} bad magic", slot);
            trap();
        }
        if unsafe { core::ptr::read_volatile(regs.add(2)) } != 0 {
            n += 1;
            occupied = Some(slot);
        }
    }
    let slot = match (n, occupied) {
        (1, Some(s)) => s,
        _ => {
            println!("vblk: occupied slots = {}（须恰 1）", n);
            trap();
        }
    };
    let regs = mmio(slot);
    let (ver, did) = (
        unsafe { core::ptr::read_volatile(regs.add(vio::mmio::VERSION / 4)) },
        unsafe { core::ptr::read_volatile(regs.add(vio::mmio::DEVICE_ID / 4)) },
    );
    if ver != vio::mmio::VERSION_MODERN || did != vio::mmio::DEVICE_ID_BLOCK {
        println!("vblk: slot {} ver={} did={}", slot, ver, did);
        trap(); // legacy 模式（缺 force-legacy=false）或非 block
    }
    slot
}

/// 占用槽的寄存器基（u32 字指针——偏移一律 `/4` 进字单位）。
fn mmio(slot: usize) -> *mut u32 {
    (VIRTIO_MMIO_VA + (slot * VIRTIO_MMIO_STRIDE) as u64) as *mut u32
}

fn rd(regs: *mut u32, off: usize) -> u32 {
    // SAFETY: 窗口已映射；对齐 u32 volatile 读。
    unsafe { core::ptr::read_volatile((regs as *const u32).add(off / 4)) }
}

fn wr(regs: *mut u32, off: usize, v: u32) {
    // SAFETY: 同上；volatile 写（Device-nGnRnE，写不合并）。
    unsafe { core::ptr::write_volatile(regs.add(off / 4), v) }
}

/// modern 协商：复位 → ACK|DRIVER → 特性页读 → 只收 VERSION_1（页 1
/// bit0 = 全局 bit32）→ FEATURES_OK 回读验。blk 特性页 0 一概不收
/// （RO/EVENT_IDX/INDIRECT/MULTIQUEUE 全不需要——v1 最小面）。
fn negotiate(regs: *mut u32) {
    // 规范复位序：写 0 后轮询读回 STATUS==0 再置 ACK|DRIVER（真机复位异
    // 步、未完成即置位可致状态机错乱——Linux virtio_mmio 同款回读等）。
    wr(regs, vio::mmio::STATUS, 0);
    let mut now = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
        trap();
    }
    let reset_deadline = now + RESET_CAP_NS;
    while rd(regs, vio::mmio::STATUS) != 0 {
        if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 || now >= reset_deadline {
            println!("vblk: STATUS 复位回读超时");
            trap();
        }
        if berry_thread_sleep(now + POLL_STEP_NS) != 0 {
            trap();
        }
    }
    wr(
        regs,
        vio::mmio::STATUS,
        vio::ST_ACKNOWLEDGE | vio::ST_DRIVER,
    );
    wr(regs, vio::mmio::DEV_FEAT_SEL, 0);
    let _f0 = rd(regs, vio::mmio::DEV_FEAT); // 读面完整走（收下哪些在写侧表达）
    wr(regs, vio::mmio::DEV_FEAT_SEL, 1);
    let f1 = rd(regs, vio::mmio::DEV_FEAT);
    if f1 & vio::FEAT_VERSION_1 == 0 {
        println!("vblk: 无 VERSION_1（f1={:#x}）", f1);
        trap(); // modern 传输层必供（规范 MUST）；缺席 = 传输层异常
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
    let st = rd(regs, vio::mmio::STATUS);
    if st != vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK {
        println!("vblk: FEATURES_OK 回读 {:#x}", st);
        trap(); // 设备撤 FEATURES_OK/置 FAILED = 协商拒绝
    }
}

/// DMA VMO + 队列几何（desc/avail/used 三址 + 缓冲三址的后备 PA）。
struct Dma {
    vmo: Handle,
    lay: vio::RingLayout,
    hdr_pa: u64,
    data_pa: u64,
    status_pa: u64,
}

/// 建队列：VMO 自建两页 → get_phys 查后备 → Q_SEL/Q_NUM/三址 LOW+HIGH/
/// Q_READY=1（modern 序；v2 专属拆址寄存器，legacy PFN 不在册）。
fn setup_queue(regs: *mut u32) -> Dma {
    let lay = match vio::RingLayout::new(QSIZE as usize) {
        Some(l) => l,
        None => trap(), // qsize=16 恒成立；防御面（布局几何是编译期已知）
    };
    let mut vmo = 0 as Handle;
    if berry_vmo_create(DMA_LEN, 0, &mut vmo) != 0 || vmo == 0 {
        println!("vblk: vmo_create 失败");
        trap();
    }
    if berry_vmo_commit(vmo, 0, DMA_LEN) != 0 {
        println!("vblk: commit 失败");
        trap();
    }
    // 页后备 PA（已 commit——NotCommitted 面不可达，防御行）。
    let mut ring_pa = 0u64;
    let mut buf_pa = 0u64;
    if berry_vmo_get_phys(vmo, RING_PAGE, &mut ring_pa) != 0
        || berry_vmo_get_phys(vmo, BUF_PAGE, &mut buf_pa) != 0
    {
        println!("vblk: get_phys 失败");
        trap();
    }
    let d = Dma {
        vmo,
        lay,
        hdr_pa: buf_pa + (HDR_OFF - BUF_PAGE),
        data_pa: buf_pa + (DATA_OFF - BUF_PAGE),
        status_pa: buf_pa + (STATUS_OFF - BUF_PAGE),
    };
    // 队列 0 建制（Q_READY=0 复位起手——重跑幂等面）。
    wr(regs, vio::mmio::Q_SEL, 0);
    let qmax = rd(regs, vio::mmio::Q_NUM_MAX);
    if qmax < QSIZE as u32 {
        println!("vblk: QueueNumMax={} < {}", qmax, QSIZE);
        trap();
    }
    wr(regs, vio::mmio::Q_NUM, QSIZE as u32);
    wr(regs, vio::mmio::Q_READY, 0);
    wr(regs, vio::mmio::Q_DESC_LOW, ring_pa as u32);
    wr(regs, vio::mmio::Q_DESC_HIGH, (ring_pa >> 32) as u32);
    wr(regs, vio::mmio::Q_AVAIL_LOW, (ring_pa + lay.avail_off as u64) as u32);
    wr(regs, vio::mmio::Q_AVAIL_HIGH, ((ring_pa + lay.avail_off as u64) >> 32) as u32);
    wr(regs, vio::mmio::Q_USED_LOW, (ring_pa + lay.used_off as u64) as u32);
    wr(regs, vio::mmio::Q_USED_HIGH, ((ring_pa + lay.used_off as u64) >> 32) as u32);
    wr(regs, vio::mmio::Q_READY, 1);
    if rd(regs, vio::mmio::Q_READY) != 1 {
        println!("vblk: Q_READY 回读 != 1");
        trap();
    }
    d
}

/// 单请求执行：暂存面组帧（hdr/data/status=0xff）→ ring 页重排（desc 三
/// 链 + publish）→ 整页 vmo_write → QueueNotify → poll used.idx 前进 +
/// status==0 → 读回数据面（`iob[16..528]`）。`out` 返数据切片视图由调用
/// 方对拍（None = 写方向不回读）。同步单在途（v1 纪律：不记账在途数，
/// publish 的 idx 单调即对账）。
fn blk_op(d: &Dma, regs: *mut u32, typ: u32, sector: u64, wr_data: Option<&[u8]>) {
    // SAFETY: 单线程独占窗（同 fsmgr 惯例）。
    let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(RING) };
    let iob: &mut [u8; 544] = unsafe { &mut *core::ptr::addr_of_mut!(IOB) };
    // —— 请求缓冲：hdr + （写方向）数据 + status 置 0xff（完成后设备写 0）。
    if vio::put_blk_hdr(iob, 0, typ, sector).is_none() {
        trap(); // 544B 放 16B 恒成立；防御面
    }
    if let Some(w) = wr_data {
        iob[vio::BLK_HDR_SZ..vio::BLK_HDR_SZ + w.len()].copy_from_slice(w);
    }
    iob[(STATUS_OFF - BUF_PAGE) as usize] = 0xff;
    if berry_vmo_write(d.vmo, iob.as_ptr(), BUF_PAGE, iob.len() as usize) != 0 {
        trap();
    }
    // —— ring 页：三描述符链（hdr→data→status；读方向 data 置 WRITE）。
    let data_f = vio::DESC_F_NEXT
        | if typ == vio::BLK_T_IN {
            vio::DESC_F_WRITE
        } else {
            0
        };
    let _ = vio::put_desc(ring, &d.lay, 0, d.hdr_pa, vio::BLK_HDR_SZ as u32, vio::DESC_F_NEXT, 1);
    let _ = vio::put_desc(ring, &d.lay, 1, d.data_pa, vio::BLK_SECTOR_SZ as u32, data_f, 2);
    let _ = vio::put_desc(ring, &d.lay, 2, d.status_pa, 1, vio::DESC_F_WRITE, 0);
    let expect = {
        let old = vio::get_u16(ring, d.lay.avail_off + 2).unwrap_or(0);
        if vio::publish(ring, &d.lay, 0).is_none() {
            trap(); // 布局几何恒在页内；防御面
        }
        old.wrapping_add(1)
    };
    if berry_vmo_write(d.vmo, ring.as_ptr(), RING_PAGE, 4096) != 0 {
        trap();
    }
    // 发布屏障（publish() 调用方契约）：此前一切 Normal 写（缓冲页 + ring
    // 页经 SVC 落后备帧）→ Device 门铃写排序——dmb ish 后再敲 QueueNotify。
    // TCG 无内存模型恒绿；真机缺此 = 门铃先达总线、设备读陈旧 avail.idx
    // → poll 10s 假超时（Linux virtio_wmb/__iowmb 同位）。
    // SAFETY: 纯屏障指令，无内存操作数。
    unsafe { core::arch::asm!("dmb ish") };
    wr(regs, vio::mmio::Q_NOTIFY, 0);
    // —— poll：used.idx 前进 + status 归零（10s 帽；1ms 步进让出 CPU）。
    // idx 短读 2 字节、偏移 used_off+2——used_off 本身是 flags 位（无
    // EVENT_IDX 时恒 0），读错位 = 恒不前进的假超时（排障实踩）。
    let mut now = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
        trap();
    }
    let deadline = now + POLL_CAP_NS;
    loop {
        let mut u = [0u8; 2];
        if berry_vmo_read(d.vmo, u.as_mut_ptr(), RING_PAGE + d.lay.used_off as u64 + 2, 2) != 0 {
            trap();
        }
        if u[0] as u16 | (u[1] as u16) << 8 == expect {
            break;
        }
        if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 || now >= deadline {
            println!("vblk: sector {} used ring 超时（期 idx={}）", sector, expect);
            trap();
        }
        if berry_thread_sleep(now + POLL_STEP_NS) != 0 {
            trap();
        }
    }
    let mut st = [0u8; 1];
    if berry_vmo_read(d.vmo, st.as_mut_ptr(), STATUS_OFF, 1) != 0 || st[0] != 0 {
        println!("vblk: sector {} status={:#x}", sector, st[0]);
        trap();
    }
    // 整洁面：中断状态确认（poll 模型下设备侧 INT_SOURCE 已置——ACK 清之，
    // 防 config change 位滞留误导后续驱动）。
    let isr = rd(regs, vio::mmio::INT_STATUS);
    if isr != 0 {
        wr(regs, vio::mmio::INT_ACK, isr);
    }
    // —— 读方向：收回数据面。
    if typ == vio::BLK_T_IN && berry_vmo_read(d.vmo, iob.as_mut_ptr(), BUF_PAGE, iob.len() as usize) != 0
    {
        trap();
    }
}

fn main() {
    let slot = find_device();
    let regs = mmio(slot);
    negotiate(regs);
    let d = setup_queue(regs);
    // 盘容（config capacity u64 = 两寄存器拼）——smoke 造 1MiB 盘 = 2048 扇区。
    let cap = (rd(regs, vio::mmio::CONFIG) as u64) | ((rd(regs, vio::mmio::CONFIG + 4) as u64) << 32);
    if cap != EXPECT_SECTORS {
        println!("vblk: capacity={} 期 {}", cap, EXPECT_SECTORS);
        trap();
    }
    // 队制就绪——此后设备可消费 avail（规范序：DRIVER_OK 最后置）。
    wr(
        regs,
        vio::mmio::STATUS,
        vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK | vio::ST_DRIVER_OK,
    );
    // ①读 LBA0：对拍 justfile/smoke 落盘印章。
    blk_op(&d, regs, vio::BLK_T_IN, 0, None);
    // SAFETY: 单线程，读回数据在 IOB[16..]。
    let got: &[u8] = unsafe {
        let iob: &[u8; 544] = &*core::ptr::addr_of!(IOB);
        &iob[vio::BLK_HDR_SZ..vio::BLK_HDR_SZ + LBA0_MAGIC.len()]
    };
    if got != &LBA0_MAGIC[..] {
        println!("vblk: LBA0 魔数不符（got={:?}）", &got[..4]);
        trap();
    }
    // ②写 LBA1 pattern → ③读回逐字节对拍（写读往返 = 块设备双向活证）。
    blk_op(&d, regs, vio::BLK_T_OUT, 1, Some(&LBA1_PATTERN[..]));
    blk_op(&d, regs, vio::BLK_T_IN, 1, None);
    // SAFETY: 同上。
    let got: &[u8] = unsafe {
        let iob: &[u8; 544] = &*core::ptr::addr_of!(IOB);
        &iob[vio::BLK_HDR_SZ..vio::BLK_HDR_SZ + LBA1_PATTERN.len()]
    };
    if got != &LBA1_PATTERN[..] {
        println!("vblk: LBA1 读回不符");
        trap();
    }
    println!("VBLK ok cap={} slot={}", cap, slot);
    // 收尾：缓冲 VMO 无映射零 unmap 面——close 即 Drop 还帧（映射数为 0，
    // 销毁性 close 无 -EBUSY 拒面）。
    if berry_handle_close(d.vmo) != 0 {
        trap();
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （vprobe 同款——链断 loud，`VBLK ok` 缺失 = smoke FAIL 兜底）。
#[cold]
fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（0x4000_0000 家族惯例值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}
