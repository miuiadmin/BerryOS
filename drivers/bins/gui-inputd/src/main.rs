//! gui-inputd —— GUI-4 段① I3 guest 入口：virtio-input 用户态输入服务
//! （常驻，实施依据 = docs/GUI-4实施设计草案.md §5.2）。
//!
//! vnet/vblk 同链（musl target + berry-libc shim；MMIO 窗口经
//! `SpawnSpec::mmio_vmo` 映到 [`VIRTIO_MMIO_VA`]，berry-exec 单源）。扫 32
//! 槽按设备类（did=18）认领**恰一个** input 设备 → modern v2 协商（仅
//! VERSION_1——virtio-input 设备特性位为零，规范 §5.8.3）→ config ABS_INFO
//! 读 X/Y 校准域（**以设备自述为准**，勿硬编 32767——§4.2）→ eventq(q0)
//! always-posted 预投 16×8B 读缓冲（QEMU 逐描述符一事件——`get_input_event`
//! 恰 8B）→ DRIVER_OK → serve() 经 berry-guiproto input 域分发
//! （INPUT_SUBSCRIBE / INPUT_POLL 严格串行 RPC）。
//!
//! **缺席/拒绝 = 优雅退出**（§5.2-5——与 vnet 的 trap 面分工：设备缺席/
//! 传输层拒绝/校准域缺席 → 一行诊断 + park 不 trap；真链断（syscall 面/
//! 设备完成项契约面）→ trap 只死自己）。**真机接线 = 段②**（rootfs 产链/
//! mkcpio/spawn 装配不在本刀）。
//!
//! 调度铁律（§5.2-4）：serve 空闲 = `berry_object_wait_many` 阻塞形态 +
//! 心跳 tick 重臂（v1 无设备可等待内核对象——poll 模型、GIC 不使能
//! virtio SPI，「设备项条件化入列」退化为单元素数组 + tick 兜底重查
//! used 环；恒满足项铁律天然满足——数组里没有恒真项）；INPUT_POLL =
//! 阻塞服务（1ms 步进收割直到泵有货或 30s 帽 → ST_EAGAIN——服务帽 <
//! client 会话帽纪律）；recv 收口容量 4（协议随行 handle 定值 0 + 富余，
//! nh != 0 → ST_BAD 分流不 trap——P6 A-4）；服务端永不主动 send（应答
//! 均为请求受激）。用户态进程自有 handle 表——私有表铁律天然满足。
//!
//! **GUI-5（docs/GUI-5实施设计草案.md §3 #10）**：serve 升格两路
//! poll-first——wmd 推送端在席（`wm_push_h()`≠0，EFI 链 wmd 起）时并监
//! wm 表快照通道：WM_TABLE_WAIT 串行 park-应答循环（快照静态缓存 +
//! 超龄三层失效 Q11）+ drain 后逐事件 `wm_hit_test` **内存旁注**（Q9
//! 单源驻 berry-guiproto；Q10 粘滞锁定；Q16——INPUT_POLL 恒为泵唯一
//! drain 者，旁注非第二消费、不进 wire）。==0（rootfs 相 wmd 未起）时
//! 两路面/旁注/步进睡改形全门控——单通道现行为逐字节保留（46 标记面
//! 零漂移的构造保证）。

// host 兜底 main：host `cargo test` 连带构建 bin target 时 guest 面整体
// 缺席（berry-libc 按目标门控——见 Cargo.toml 注），空壳防符号插入劫
// （T-1 实录：berry-libc 的 libc 同名符号劫 libSystem write 自旋）。
#[cfg(target_os = "linux")]
fn main() {
    guest::run();
}

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
mod guest {
    use berry_exec::{VIRTIO_MMIO_SLOTS, VIRTIO_MMIO_STRIDE, VIRTIO_MMIO_VA};
    use berry_guiproto as gp;
    use berry_libc::io::{gift2_h, wm_push_h};
    use berry_sys::{
        berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
        berry_object_wait_many, berry_object_wait_one, berry_thread_sleep, berry_vmo_commit,
        berry_vmo_create, berry_vmo_get_phys, berry_vmo_read, berry_vmo_write, Handle,
        BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
    };
    use berry_virtio as vio;
    use gui_inputd::{raw_from_wire, AbsCal, PointerPump, PumpConfig, PumpEvent, ScreenGeom};

    // 链接种子（vprobe/vnet 同款）：无引用则 berry-libc 的 crt0 不进链接。
    #[used]
    static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

    /// 队列深（vnet/vblk 同档：2 的幂、整环落一页）。
    const QSIZE: u16 = 16;
    /// eventq 读缓冲数（= qsize）：每缓冲一描述符恰容一事件（QEMU
    /// virtio_input_send 逐描述符取一元素——8B 恰长是最优投递形态）。
    const EVT_BUF_SZ: usize = vio::input::INPUT_EVENT_SZ;
    /// DMA VMO 几何：页 0 event ring / 页 1 事件缓冲带（16×8B——结构分页
    /// vnet 惯例；缓冲带只用页首 128B，清晰优先于字节节省）。
    const DMA_LEN: u64 = 8192;
    const RING_PAGE: u64 = 0;
    const BUF_PAGE: u64 = 4096;
    /// STATUS 复位回读帽：1s（真机复位 ms 级；QEMU 同步即 0）。
    const RESET_CAP_NS: u64 = 1_000_000_000;
    /// INPUT_POLL 阻塞服务步进：1ms 绝对 deadline 睡（让出 CPU 不空转）。
    const POLL_STEP_NS: u64 = 1_000_000;
    /// INPUT_POLL 阻塞服务帽：30s（< client 侧会话帽 90s——「服务帽 <
    /// client 帽」纪律；到点 ST_EAGAIN 而非自陷）。
    const POLL_BLOCK_CAP_NS: u64 = 30_000_000_000;
    /// serve 空闲心跳 tick：100ms（超时 = 心跳非死线——回环顶重查 used 环
    /// 后重臂续等；P6 A-3 echo 同款语义，段② 按消费节奏可调）。tick 同时是
    /// **无消费窗口的设备层收割节奏**：eventq 预投 QSIZE=16 缓冲、QEMU
    /// virtio_input_send 逐事件取描述符，耗尽即丢批尾——空闲期输入超过
    /// 16 事件/tick 时尾部被设备静默丢弃（press 送达而 release 丢弃可致泵
    /// 接触态滞留，至下次真实接触自愈）；100ms = 160 事件/s 窗口，覆盖
    /// 注入/人工形态。离散转移零丢的结构性根治 = v2 IRQ 面（§5.2-5 注）。
    const SVC_TICK_NS: u64 = 100_000_000;
    /// 应答 send EBUSY 重臂帽：10s（vnet 同档）。
    const SEND_REARM_NS: u64 = 10_000_000_000;
    /// GUI-5：wmd 推送快照超龄帽（Q11-②——age 超此值视同缺席，旁注降级
    /// 记账不命中；③降级态每帽值一跳重发 WAIT(0) 重探测）。
    const SNAPSHOT_STALE_CAP_NS: u64 = 10_000_000_000;
    /// 屏幕几何兜底：泵构造先于首张 wmd 快照时的 800×600 初值；快照
    /// 到达即经 `PointerPump::set_geom` 换真显示域（Q15）。快照缺席/畸形
    /// 的降级态保留初值，事件坐标按兜底域归一。
    const SCREEN_W: u32 = 800;
    const SCREEN_H: u32 = 600;

    const ZERO_SAMPLE: gp::Sample = gp::Sample { timestamp_ns: 0, x: 0, y: 0 };
    const ZERO_PE: PumpEvent = PumpEvent {
        phase: 0,
        device_id: 0,
        buttons: 0,
        flags: 0,
        pointer_id: 0,
        timestamp_ns: 0,
        x: 0,
        y: 0,
        pressure: 0,
        history_len: 0,
        history: [ZERO_SAMPLE; gp::INPUT_SAMPLES_MAX - 1],
    };
    const ZERO_WPE: gp::PointerEvent = gp::PointerEvent {
        phase: 0,
        device_id: 0,
        buttons: 0,
        flags: 0,
        pointer_id: 0,
        timestamp_ns: 0,
        x: 0,
        y: 0,
        pressure: 0,
        contact_major: 0,
        contact_minor: 0,
        history: [ZERO_SAMPLE; gp::INPUT_SAMPLES_MAX - 1],
        history_len: 0,
    };

    /// ring 暂存页（初始建制专用——单线程独占窗；refill 走字节粒度
    /// vmo_write 不经此页，vnet RXSTAGE 同款）。
    static mut RINGSTAGE: [u8; 4096] = [0; 4096];

    /// 已认领设备的寄存器基（trap 整洁面用——单线程进程、认领后恒定；
    /// null = 未认领：find 面自陷时设备尚未到手，无队列可撤）。
    static mut CLAIMED_REGS: *mut u32 = core::ptr::null_mut();

    /// DMA 面（vnet Dma 同款纯 VMO 机制——从不进任何地址空间）。
    struct Dma {
        vmo: Handle,
        lay: vio::RingLayout,
        ring_pa: u64,
        buf_pa: u64,
        /// 已收割数（与设备 used.idx 对账的自由运行 u16）。
        harvested: u16,
        /// 已发布数（avail.idx 的驱动侧权威——refill 续挂对账用）。
        posted: u16,
    }

    pub(super) fn run() {
        let slot = match find_input_device() {
            Some(s) => s,
            None => berry_libc::park(), // 诊断行已在 find 内打——优雅退出
        };
        let regs = mmio(slot);
        // SAFETY: 单线程进程；认领后恒定（trap() 的整洁面读它）。
        unsafe { core::ptr::write_volatile(core::ptr::addr_of_mut!(CLAIMED_REGS), regs) };
        if !negotiate(regs) {
            println!("gui-inputd: 协商拒绝（无 VERSION_1/复位超时）；park");
            berry_libc::park();
        }
        let (ax, ay) = match (
            read_absinfo(regs, vio::input::ABS_X),
            read_absinfo(regs, vio::input::ABS_Y),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => {
                println!("gui-inputd: ABS_INFO 缺席（归一化域不可得）；park");
                berry_libc::park()
            }
        };
        if ax.max <= ax.min || ay.max <= ay.min {
            // 设备自述域退化：泵构造 assert 会 panic——缺席面提前优雅退出
            println!("gui-inputd: ABS 域退化 x=[{},{}] y=[{},{}]；park", ax.min, ax.max, ay.min, ay.max);
            berry_libc::park();
        }
        let mut pump = PointerPump::new(PumpConfig {
            geom: ScreenGeom { w: SCREEN_W, h: SCREEN_H },
            cal: AbsCal { min_x: ax.min, max_x: ax.max, min_y: ay.min, max_y: ay.max },
            delivery: gp::DELIVERY_BATCHED,
        });
        let mut d = setup_dma();
        // eventq = q0（规范 §5.8.6；statusq q1 v1 不建）。DRIVER_OK 前先挂
        // 缓冲——设备从此有缓冲可写。
        setup_queue(regs, 0, d.ring_pa, &d.lay);
        post_events(&mut d);
        wr(
            regs,
            vio::mmio::STATUS,
            vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK | vio::ST_DRIVER_OK,
        );
        // 初始填充后的 kick（规范整洁面；dmb 先行——整页写在门铃前排序）。
        // SAFETY: 纯屏障指令，无内存操作数。
        unsafe { core::arch::asm!("dmb ish") };
        wr(regs, vio::mmio::Q_NOTIFY, 0);
        println!(
            "gui-inputd: up slot={} q={}x{}B cal=x[{},{}) y[{},{})",
            slot, QSIZE, EVT_BUF_SZ, ax.min, ax.max, ay.min, ay.max
        );
        serve(&mut d, regs, &mut pump);
    }

    /// 扫 32 槽按设备类认领恰一个 input 设备（vnet find_device 对称面；
    /// did=18 按类计数——net/block 槽合法同席不算）。**None = 优雅退出**
    /// （设备缺席/多设备歧义/legacy 传输层——诊断行 + park，§5.2-5）；
    /// 坏 magic = 窗口破坏（系统级）→ trap loud（vnet 同口径）。
    fn find_input_device() -> Option<usize> {
        // SAFETY: 窗口经 spawn 映射（Device-nGnRnE）；逐字 volatile；只读
        // ID 头（写面在协商序内）。
        let base = VIRTIO_MMIO_VA as *const u32;
        let mut hit: Option<usize> = None;
        let mut n = 0usize;
        for slot in 0..VIRTIO_MMIO_SLOTS {
            let regs = unsafe { base.add(slot * (VIRTIO_MMIO_STRIDE / 4)) };
            if unsafe { core::ptr::read_volatile(regs) } != vio::mmio::MAGIC_VAL {
                println!("gui-inputd: slot {} bad magic", slot);
                trap();
            }
            let did = unsafe { core::ptr::read_volatile(regs.add(vio::mmio::DEVICE_ID / 4)) };
            if did == vio::mmio::DEVICE_ID_INPUT {
                n += 1;
                hit = Some(slot);
            }
        }
        let slot = match (n, hit) {
            (1, Some(s)) => s,
            _ => {
                println!("gui-inputd: input slots = {}（须恰 1）；park", n);
                return None;
            }
        };
        let regs = mmio(slot);
        let ver = unsafe { core::ptr::read_volatile(regs.add(vio::mmio::VERSION / 4)) };
        if ver != vio::mmio::VERSION_MODERN {
            println!("gui-inputd: slot {} ver={}（legacy 不支）；park", slot, ver);
            return None;
        }
        Some(slot)
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

    /// 单调钟现值（失败 = clock 面断链，loud）。
    fn now() -> u64 {
        let mut t = 0u64;
        if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
            trap();
        }
        t
    }

    /// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
    /// （vprobe/vnet 同款——链断 loud）。
    ///
    /// **整洁面先行**（A-7 host-writer 同类残面）：设备已认领则 STATUS=0
    /// 复位撤 avail 队列再自陷——trap 只死自己，但 DRIVER_OK 下 avail 环
    /// 仍指向 DMA VMO 帧时，后续宿主输入事件会让设备继续向这些帧 DMA
    /// 8B；进程死后 VMO 归还 buddy = 设备写已回收帧、静默腐化无关进程
    /// （vnet 退出复位同款）。回读帽 = 定次自旋（QEMU 同步即 0；clock/
    /// sleep 可能正是 trap 成因，不依赖两者）；复位失败不拦死路。
    #[cold]
    fn trap() -> ! {
        // SAFETY: 单线程进程；CLAIMED_REGS 认领后恒定。
        let regs = unsafe { core::ptr::read_volatile(core::ptr::addr_of!(CLAIMED_REGS)) };
        if !regs.is_null() {
            wr(regs, vio::mmio::STATUS, 0);
            for _ in 0..1_000_000 {
                if rd(regs, vio::mmio::STATUS) == 0 {
                    break;
                }
                core::hint::spin_loop();
            }
        }
        // SAFETY: 故意触发的未映射读（0x4000_0000 与用户布局无交集——家族
        // 惯例值）。
        unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
        loop {
            core::hint::spin_loop();
        }
    }

    /// modern 协商（vnet 同形裁剪）：复位（1s 帽）→ ACK|DRIVER → 特性页读
    /// （页 1 须供 VERSION_1）→ 驱动侧全零收 + VERSION_1（virtio-input 设备
    /// 特性位为零，规范 §5.8.3）→ FEATURES_OK 回读验。**false = 优雅退出**
    /// （传输层拒绝/复位超时——设备病 ≠ 链断，§5.2-2）。
    fn negotiate(regs: *mut u32) -> bool {
        wr(regs, vio::mmio::STATUS, 0);
        let deadline = now() + RESET_CAP_NS;
        loop {
            if rd(regs, vio::mmio::STATUS) == 0 {
                break;
            }
            let t = now();
            if t >= deadline {
                println!("gui-inputd: STATUS 复位回读超时");
                return false;
            }
            if berry_thread_sleep(t + POLL_STEP_NS) != 0 {
                trap();
            }
        }
        wr(regs, vio::mmio::STATUS, vio::ST_ACKNOWLEDGE | vio::ST_DRIVER);
        wr(regs, vio::mmio::DEV_FEAT_SEL, 1);
        let f1 = rd(regs, vio::mmio::DEV_FEAT);
        if f1 & vio::FEAT_VERSION_1 == 0 {
            println!("gui-inputd: 无 VERSION_1（f1={:#x}）", f1);
            return false; // modern 传输层必供（规范 MUST）——legacy 优雅退出
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
            println!("gui-inputd: FEATURES_OK 回读异常");
            return false; // 设备撤 FEATURES_OK/置 FAILED = 协商拒绝
        }
        true
    }

    // ---- config 查询区（136B @0x100 字节口径——select/subsel 是单字节
    // 寄存器，整字写会砸邻位；规范 §5.8.4）----

    fn cfg_rd8(regs: *mut u32, off: usize) -> u8 {
        // SAFETY: 窗口已映射；单字节 volatile 读。
        unsafe { core::ptr::read_volatile((regs as *const u8).add(off)) }
    }

    fn cfg_wr8(regs: *mut u32, off: usize, v: u8) {
        // SAFETY: 同上；单字节 volatile 写。
        unsafe { core::ptr::write_volatile((regs as *mut u8).add(off), v) }
    }

    /// ABS_INFO 查询（subsel = ABS 轴号）：select/subsel 写 → size 回读 →
    /// union 前 20B 逐字节读 → `parse_input_absinfo` 恰长解析（size 撒谎/
    /// 轴缺席 → None——归一化域不可得走优雅退出）。
    fn read_absinfo(regs: *mut u32, axis: u16) -> Option<vio::input::InputAbsInfo> {
        cfg_wr8(regs, vio::mmio::CONFIG, vio::input::INPUT_CFG_SEL_ABS_INFO);
        cfg_wr8(regs, vio::mmio::CONFIG + 1, axis as u8);
        let size = cfg_rd8(regs, vio::mmio::CONFIG + 2) as usize;
        if size != 20 {
            return None; // 恰长执法（§4.3——size 域撒谎早拒）
        }
        let mut buf = [0u8; 20];
        for i in 0..20 {
            buf[i] = cfg_rd8(regs, vio::mmio::CONFIG + 8 + i);
        }
        vio::input::parse_input_absinfo(&buf)
    }

    /// DMA VMO + 几何（vnet setup_dma 同款）：页 0 ring PA / 页 1 缓冲带 PA。
    fn setup_dma() -> Dma {
        let lay = match vio::RingLayout::new(QSIZE as usize) {
            Some(l) => l,
            None => trap(), // qsize=16 恒成立；防御面
        };
        let mut vmo = 0 as Handle;
        if berry_vmo_create(DMA_LEN, 0, &mut vmo) != 0 || vmo == 0 {
            println!("gui-inputd: vmo_create 失败");
            trap();
        }
        if berry_vmo_commit(vmo, 0, DMA_LEN) != 0 {
            println!("gui-inputd: commit 失败");
            trap();
        }
        let mut ring_pa = 0u64;
        let mut buf_pa = 0u64;
        if berry_vmo_get_phys(vmo, RING_PAGE, &mut ring_pa) != 0
            || berry_vmo_get_phys(vmo, BUF_PAGE, &mut buf_pa) != 0
        {
            println!("gui-inputd: get_phys 失败");
            trap();
        }
        Dma { vmo, lay, ring_pa, buf_pa, harvested: 0, posted: 0 }
    }

    /// 建队列 q0（vnet setup_queue 同形）：Q_SEL → QueueNumMax 检 → Q_NUM →
    /// Q_READY=0 → 三址 LOW+HIGH → Q_READY=1 回读（modern 序）。
    fn setup_queue(regs: *mut u32, qsel: u32, ring_pa: u64, lay: &vio::RingLayout) {
        wr(regs, vio::mmio::Q_SEL, qsel);
        let qmax = rd(regs, vio::mmio::Q_NUM_MAX);
        if qmax < QSIZE as u32 {
            println!("gui-inputd: q{} QueueNumMax={} < {}", qsel, qmax, QSIZE);
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
            println!("gui-inputd: q{} Q_READY 回读 != 1", qsel);
            trap();
        }
    }

    /// 挂 16 个事件读缓冲（页 1 逐 8B 一格，DESC_F_WRITE——设备写；QEMU
    /// virtio_input_send 逐描述符取一元素，8B 恰长 = 最优投递）：RINGSTAGE
    /// 零基组 desc 表 + avail 环 → 整页落盘（vnet post_rx 同形）。
    fn post_events(d: &mut Dma) {
        // SAFETY: 单线程独占窗；RINGSTAGE 首次使用即零基。
        let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(RINGSTAGE) };
        for i in 0..QSIZE as usize {
            let addr = d.buf_pa + (i * EVT_BUF_SZ) as u64;
            let _ = vio::put_desc(
                ring,
                &d.lay,
                i,
                addr,
                EVT_BUF_SZ as u32,
                vio::DESC_F_WRITE,
                0,
            );
        }
        for i in 0..QSIZE as usize {
            if vio::publish(ring, &d.lay, i as u16).is_none() {
                trap(); // 布局几何恒在页内；防御面
            }
        }
        if berry_vmo_write(d.vmo, ring.as_ptr(), RING_PAGE, 4096) != 0 {
            trap();
        }
        d.posted = QSIZE;
    }

    /// 收割后回填事件缓冲（**同 id 重发布**，vnet refill_rx 同形——desc 表
    /// addr/len/WRITE 恒有效不重写）：只动 avail 区两处 2B——槽先落、idx
    /// 后进（次序 = 正确性：设备按 idx 变化消费新项）→ `dmb ish` → 门铃
    /// q0。字节粒度写避开整页回写砸设备拥有的 used 环（读写窗竞速丢完成）。
    fn refill_event(d: &mut Dma, regs: *mut u32, id: u16) {
        let slot = (d.posted % d.lay.qsize as u16) as usize;
        let b = [id as u8, (id >> 8) as u8];
        let off = RING_PAGE + d.lay.avail_off as u64 + 4 + (slot as u64) * 2;
        if berry_vmo_write(d.vmo, b.as_ptr(), off, 2) != 0 {
            trap();
        }
        d.posted = d.posted.wrapping_add(1);
        let b = [d.posted as u8, (d.posted >> 8) as u8];
        let off = RING_PAGE + d.lay.avail_off as u64 + 2;
        if berry_vmo_write(d.vmo, b.as_ptr(), off, 2) != 0 {
            trap();
        }
        // SAFETY: 纯屏障指令，无内存操作数（发布屏障——avail 两写在门铃
        // 前排序）。
        unsafe { core::arch::asm!("dmb ish") };
        wr(regs, vio::mmio::Q_NOTIFY, 0);
    }

    /// 收割已完成事件：poll used.idx（2B 短读——`used_idx` 的 vmo_read 直
    /// 达形态，vnet 同款「诊断与被测同源」纪律）→ 逐完成项读 8B 缓冲 →
    /// **ts_ns = dequeue 时刻打点**（规范无设备时间戳——Linux evdev 同款）
    /// → `raw_from_wire` → 泵 feed → 同 id 回填。无完成即返（非阻塞）。
    fn harvest(d: &mut Dma, regs: *mut u32, pump: &mut PointerPump) {
        loop {
            let mut u2 = [0u8; 2];
            if berry_vmo_read(d.vmo, u2.as_mut_ptr(), RING_PAGE + d.lay.used_off as u64 + 2, 2)
                != 0
            {
                trap();
            }
            let idx = u16::from_le_bytes([u2[0], u2[1]]);
            if idx == d.harvested {
                return;
            }
            let i = (d.harvested % QSIZE) as usize;
            let mut u8b = [0u8; 8];
            if berry_vmo_read(
                d.vmo,
                u8b.as_mut_ptr(),
                RING_PAGE + d.lay.used_off as u64 + 4 + (i as u64) * 8,
                8,
            ) != 0
            {
                trap();
            }
            let id = vio::get_u32(&u8b, 0).unwrap_or(u32::MAX);
            let len = vio::get_u32(&u8b, 4).unwrap_or(u32::MAX);
            if id >= QSIZE as u32 || len != EVT_BUF_SZ as u32 {
                // 设备完成项契约面断裂（id 越幅/长度非 8）——loud
                println!("gui-inputd: 坏完成项 id={} len={}", id, len);
                trap();
            }
            let mut eb = [0u8; 8];
            if berry_vmo_read(d.vmo, eb.as_mut_ptr(), BUF_PAGE + id as u64 * EVT_BUF_SZ as u64, 8)
                != 0
            {
                trap();
            }
            let wire_ev = match vio::input::get_input_event(&eb, 0) {
                Some(e) => e,
                None => trap(), // 8B 恒可解；防御面
            };
            let ts = now();
            pump.feed(raw_from_wire(ts, wire_ev));
            d.harvested = d.harvested.wrapping_add(1);
            refill_event(d, regs, id as u16);
        }
    }

    /// 应答发送（受激面——服务端永不主动 send）：EBUSY → wait WRITABLE
    /// 10s 重臂重发（阻塞形态，自旋形态的公平分槽面——G-eco-b 铁律）；
    /// 其它错 = 链断 loud。
    fn send_rep(h: Handle, rep: &[u8]) {
        loop {
            let s = berry_channel_send(h, rep.as_ptr(), rep.len(), core::ptr::null(), 0);
            if s == 0 {
                return;
            }
            if s != BERRY_ERR_BUSY {
                println!("gui-inputd: send 失败 {}", s);
                trap();
            }
            let mut obs = 0u64;
            let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, now() + SEND_REARM_NS, &mut obs);
            // 到点未通走下一轮 send 重验（obs 不作判据——重验循环兜底）
        }
    }

    /// 泵事件 → 协议事件（字段镜像——编码单源在 berry-guiproto）。
    fn to_wire(e: &PumpEvent) -> gp::PointerEvent {
        let mut hist = [ZERO_SAMPLE; gp::INPUT_SAMPLES_MAX - 1];
        hist[..e.history_len].copy_from_slice(&e.history[..e.history_len]);
        gp::PointerEvent {
            phase: e.phase,
            device_id: e.device_id,
            buttons: e.buttons,
            flags: e.flags,
            pointer_id: e.pointer_id,
            timestamp_ns: e.timestamp_ns,
            x: e.x,
            y: e.y,
            pressure: e.pressure,
            contact_major: 0,
            contact_minor: 0,
            history: hist,
            history_len: e.history_len,
        }
    }

    // ---- GUI-5：wmd 推送消费 + 命中旁注（§3 #10 / §7 Q9-Q11/Q16）----

    /// 命中旁注结果（**内存面记账，v1 不进 wire**——Q9 注解形）。
    enum WmTarget {
        /// 命中在册窗（命中/锁定 id）。
        Win(u16),
        /// 快照缺席/超龄——记账不命中（Q11-② 的 WM_TARGET_STALE 哨兵：
        /// 视同缺席，不主张派发目标）。
        Stale,
        /// 锁定窗不在最新快照——丢弃至解锁不重新命中（Q10 防手势跳窗
        /// clickjacking）。
        Gone,
        /// 无锁定未命中（DOWN 落点无窗 / 无 DOWN 面事件）。
        Miss,
    }

    /// wmd 快照缓存 + 旁注粘滞锁 + 记账（静态存储——version 在快照头内、
    /// age 用到达时刻打点；单线程进程 serve/serve_poll 两址独占）。
    struct WmCache {
        /// 最新快照（None = 从未到达）。
        snap: Option<gp::WmSnapshot>,
        /// 快照到达时刻（Q11-② 超龄判据）。
        born_ns: u64,
        /// 降级态上次重探测时刻（Q11-③——每帽值一跳）。
        probe_ns: u64,
        /// WAIT 在途位（guiproto 头注「每域严格串行 RPC、队列深 1」的
        /// 发送侧执法）：send 置位、应答到达清位；探测重发（Q11-③）在位
        /// 期间禁发——防「park 中 WAIT + 探测 WAIT(0)」双在途触发 wmd
        /// parked 单车位位移应答（R2 备注⑦ 现答现快照）联锁的自持回声环。
        outstanding: bool,
        /// 首快照诊断行已打（防逐快照刷屏——推送相 smoke 覆盖 = 零，行只
        /// 在手跑/联调面可见）。
        logged: bool,
        /// 降级迁沿记账（fresh→degraded 一行 tally 的防重打位）。
        degraded: bool,
        /// 粘滞锁 (pointer_id, 目标窗 id)（Q10——多指各自独立锁定；v1 泵
        /// 单接触位手势永不重叠，单车位结构性充分，GUI-8 多指泵扩表）。
        lock: Option<(u16, u16)>,
        // 旁注记账（诊断面——降级迁沿行打印）。
        n_hit: u64,
        /// 最近命中窗 id（tally 行回显——0 = 无）。
        hit_last: u16,
        n_stale: u64,
        n_gone: u64,
        n_miss: u64,
    }

    static mut WM_CACHE: WmCache = WmCache {
        snap: None,
        born_ns: 0,
        probe_ns: 0,
        outstanding: false,
        logged: false,
        degraded: false,
        lock: None,
        n_hit: 0,
        hit_last: 0,
        n_stale: 0,
        n_gone: 0,
        n_miss: 0,
    };

    /// 快照鲜度裁决（Q11-②）：`Some` = 鲜（携引用）；`None` = 缺席/超龄
    /// （超龄视同缺席——记账不命中）。
    fn snap_fresh(c: &WmCache) -> Option<&gp::WmSnapshot> {
        let s = c.snap.as_ref()?;
        if now().saturating_sub(c.born_ns) > SNAPSHOT_STALE_CAP_NS {
            return None;
        }
        Some(s)
    }

    /// 锁定窗旁注（MOVE/收尾相位共面）：无锁定 Miss；锁定在 + 超龄 Stale；
    /// 锁定在 + 鲜 → 窗在册 Win / 窗失踪 Gone（丢弃至解锁）。
    fn lock_target(c: &mut WmCache) -> WmTarget {
        let Some((_, id)) = c.lock else {
            return WmTarget::Miss;
        };
        let Some(s) = snap_fresh(c) else {
            return WmTarget::Stale;
        };
        if s.entries[..s.count].iter().any(|w| w.id == id) {
            WmTarget::Win(id)
        } else {
            WmTarget::Gone
        }
    }

    /// 逐事件命中旁注（drain 后 to_wire 前调用——**不改 wire 事件**；Q16：
    /// INPUT_POLL 恒为泵唯一 drain 者，旁注非第二消费）。粘滞语义（Q10）：
    /// DOWN → `wm_hit_test`（Q9 单源）锁 pointer_id 目标窗；MOVE → 全投
    /// 锁定窗（跟手）；UP/CANCEL（含合成取消）→ 解锁；DOWN 无命中不锁。
    fn wm_annotate(e: &PumpEvent) {
        // SAFETY: 单线程进程（serve_poll 调用链内独占——RINGSTAGE 同款面）。
        let c = unsafe { &mut *core::ptr::addr_of_mut!(WM_CACHE) };
        let target = match e.phase {
            gp::PHASE_DOWN => match snap_fresh(c) {
                Some(s) => match gp::wm_hit_test(&s.entries[..s.count], e.x, e.y) {
                    Some(id) => {
                        c.lock = Some((e.pointer_id, id));
                        WmTarget::Win(id)
                    }
                    None => {
                        c.lock = None;
                        WmTarget::Miss
                    }
                },
                None => {
                    c.lock = None; // 无可信快照不锁（记账不命中——Q11-②）
                    WmTarget::Stale
                }
            },
            gp::PHASE_MOVE => lock_target(c),
            gp::PHASE_UP | gp::PHASE_CANCEL => {
                let target = lock_target(c);
                c.lock = None; // 解锁（UP/CANCEL 含合成取消——Q10）
                target
            }
            _ => WmTarget::Miss, // HOVER_* 留位（v1 泵不产）
        };
        match target {
            WmTarget::Win(id) => {
                c.n_hit += 1;
                c.hit_last = id;
            }
            WmTarget::Stale => c.n_stale += 1,
            WmTarget::Gone => c.n_gone += 1,
            WmTarget::Miss => c.n_miss += 1,
        }
    }

    /// WAIT 重臂发送（EBUSY → WRITABLE 10s 重臂重验——`send_rep` 同形；
    /// 其余错 = push 链 syscall 面断 loud）。发送即记在途位——应答到达前
    /// 本端禁再发（深 1 的发送侧执法，见 `WmCache::outstanding`）。
    fn wm_send_wait(push: Handle, last_seen: u32) {
        // SAFETY: 单线程进程（serve/serve_poll 调用链内独占）。
        let c = unsafe { &mut *core::ptr::addr_of_mut!(WM_CACHE) };
        c.outstanding = true;
        let mut req = [0u8; 8];
        let n = gp::encode_wm_table_wait_req(&mut req, last_seen);
        send_rep(push, &req[..n]);
    }

    /// push 路应答消费：快照三态解析——`Some(Ok)` 存储 + age 打点 + 立刻
    /// 重臂 WAIT(new_version)（表变更恰醒一次收一张，Q17）；`Some(Err)` /
    /// `None`（1B 拒 / 畸形）= wmd 侧病——**降级记账不 trap**（失败隔离：
    /// wmd 病不连坐输入服务；Q11-③ 每 10s WAIT(0) 重探测兜底闭环）。
    fn wm_on_reply(push: Handle, buf: &[u8], pump: &mut PointerPump) {
        // SAFETY: 单线程进程（serve 调用链内独占）。
        let c = unsafe { &mut *core::ptr::addr_of_mut!(WM_CACHE) };
        // 应答到达 = 在途 WAIT 已消费（一请求一应答——1B 拒/畸形同面，
        // 三态解析前先清位；Ok 路随重臂再置位）。
        c.outstanding = false;
        match gp::parse_wm_snapshot_rep(buf) {
            Some(Ok(snap)) if snap.disp_w == 0 || snap.disp_h == 0 => {
                println!("gui-inputd: wm 快照显示域退化（{}x{}）", snap.disp_w, snap.disp_h);
                c.probe_ns = now();
                return;
            }
            Some(Ok(snap)) => {
                if !c.logged {
                    c.logged = true;
                    println!(
                        "gui-inputd: wm 快照 v={} 窗={} 域={}x{}",
                        snap.version, snap.count, snap.disp_w, snap.disp_h
                    );
                }
                pump.set_geom(ScreenGeom {
                    w: u32::from(snap.disp_w),
                    h: u32::from(snap.disp_h),
                });
                let v = snap.version;
                c.snap = Some(snap);
                c.born_ns = now();
                c.degraded = false;
                wm_send_wait(push, v);
            }
            Some(Err(st)) => {
                println!("gui-inputd: wm wait rep st={}", st);
                c.probe_ns = now(); // 降级态起算（10s 后 WAIT(0) 重探测）
            }
            None => {
                println!("gui-inputd: wm 快照畸形（{}B）", buf.len());
                c.probe_ns = now();
            }
        }
    }

    /// 降级态重探测（Q11-③）：快照缺席/超龄每帽值重发 WAIT(0)——**在途
    /// 门控**：WAIT 已在途（「wmd 活、表安静」健康稳态 = 唯一在途即 park
    /// 在 wmd 的那条，表一变即被应答刷新；超龄降级 Q11-② 已独立生效不
    /// 依赖探测）时禁发，防第二在途请求违反深 1 并与 wmd 单车位位移应答
    /// （后到 WAIT 现答现快照并覆盖 parked）联锁成「位移应答 → 重臂 →
    /// 再位移」自持回声环；「send 入死通道无应答 = 惰性无害」面（Q11-③
    /// 括注）因无在途不受门控。fresh→degraded 迁沿打一行旁注 tally（有
    /// 旁注才打——wmd 崩溃/推送停滞的诊断面；冷启动缺席静默）。
    fn wm_probe_if_degraded(push: Handle) {
        // SAFETY: 单线程进程（serve 调用链内独占）。
        let c = unsafe { &mut *core::ptr::addr_of_mut!(WM_CACHE) };
        let t = now();
        let degraded = match &c.snap {
            Some(_) => t.saturating_sub(c.born_ns) > SNAPSHOT_STALE_CAP_NS,
            None => true,
        };
        if !degraded {
            c.degraded = false;
            return;
        }
        if !c.degraded && c.n_hit + c.n_stale + c.n_gone + c.n_miss > 0 {
            println!(
                "gui-inputd: wm 快照超龄/缺席（旁注 hit={} 窗={} stale={} gone={} miss={}）",
                c.n_hit, c.hit_last, c.n_stale, c.n_gone, c.n_miss
            );
        }
        c.degraded = true;
        if c.outstanding {
            return; // 深 1 已被在途 WAIT 占用——探测让位（见函数注）
        }
        if t.saturating_sub(c.probe_ns) >= SNAPSHOT_STALE_CAP_NS {
            c.probe_ns = t;
            wm_send_wait(push, 0);
        }
    }

    /// INPUT_POLL 阻塞服务（§5.2-4）：收割 → drain，泵有货即应答（≤
    /// max_events 事件）；否则 1ms 步进续割，30s 帽到点 ST_EAGAIN（1B
    /// 非 OK 应答——协议烘焙线）。k ≤ 3 ≤ INPUT_EVENTS_MAX 恒可编——
    /// encode None 结构性不可达（防御 trap）。
    ///
    /// GUI-5：drain 后 to_wire 前逐事件命中旁注（`wm_annotate`——内存面
    /// 不改 wire）。**步进睡改形 R2 阻断①门控**：`push != 0` 时步进睡改
    /// wait_many([push]) 可读并等（快照应答到达即醒——收割/消费恒在 serve
    /// 回环顶，本处只借它做步进钟不双消费；deadline = min(步进, 轮询死线)
    /// 防睡过 30s 帽）；`push == 0`（rootfs 相 wmd 未起）时 thread_sleep
    /// 现形逐字节保留——wait_many 空转 = 活锁自旋。
    fn serve_poll(
        d: &mut Dma,
        regs: *mut u32,
        pump: &mut PointerPump,
        h: Handle,
        push: Handle,
        max_events: u8,
        rep: &mut [u8],
    ) {
        let deadline = now() + POLL_BLOCK_CAP_NS;
        let mut evs = [ZERO_PE; gp::INPUT_EVENTS_MAX];
        loop {
            harvest(d, regs, pump);
            let k = pump.drain(&mut evs[..max_events as usize]);
            if k > 0 {
                let mut wes = [ZERO_WPE; gp::INPUT_EVENTS_MAX];
                for i in 0..k {
                    wm_annotate(&evs[i]);
                    wes[i] = to_wire(&evs[i]);
                }
                match gp::encode_input_events_rep(rep, &wes[..k]) {
                    Some(n) => send_rep(h, &rep[..n]),
                    None => trap(), // ≤3 事件恒 ≤ INPUT_REP_MAX；防御面
                }
                return;
            }
            let t = now();
            if t >= deadline {
                let n = gp::encode_status_rep(rep, gp::ST_EAGAIN);
                send_rep(h, &rep[..n]);
                return;
            }
            if push != 0 {
                let items = [push];
                let sigs = [SIGNAL_READABLE];
                let mut idx = usize::MAX;
                let mut obs = 0u64;
                let _ = berry_object_wait_many(
                    items.as_ptr(),
                    sigs.as_ptr(),
                    1,
                    core::cmp::min(t + POLL_STEP_NS, deadline),
                    &mut idx,
                    &mut obs,
                );
            } else if berry_thread_sleep(t + POLL_STEP_NS) != 0 {
                trap();
            }
        }
    }

    /// 常驻服务循环（§5.2-4 调度铁律面；GUI-5 升格**两路 poll-first**）：
    /// gift 端点缺席 = 装配面缺席——诊断 + park（§5.2-5 优雅退出）。
    /// 路① input 域（既有——SUBSCRIBE/INPUT_POLL 分派零扰动）→ 路② wmd
    /// 推送（WAIT 应答到达：快照存储 + age 打点 + 立刻重臂
    /// WAIT(new_version)；EBUSY 重臂 10s = `send_rep` 形）；全 EAGAIN 才
    /// wait_many n=1|2 并等（**wm_push_h()==0 → n=1 现单通道路径逐字节
    /// 保留**——rootfs 相 wmd 未起，push 项不入等待集；数组尾项 0 值随
    /// n=1 不进内核等待集）+ 心跳 tick 重臂（tick = 心跳非死线——超时
    /// 回环顶重查 used 环后重臂续等；v1 设备无可等待内核对象，「设备项
    /// 条件化入列」由 tick 兜底执行）。
    fn serve(d: &mut Dma, regs: *mut u32, pump: &mut PointerPump) -> ! {
        let h = gift2_h();
        if h == 0 {
            println!("gui-inputd: gift 端点缺席（input 域通道未授）；park");
            berry_libc::park();
        }
        // GUI-5：wmd 推送客户端端（0 = wmd 未起——两路面/旁注/超龄探测
        // 全门控在此值上）。
        let push = wm_push_h();
        if push != 0 {
            // 冷启动 WAIT(0)：wmd version 恒 ≥1（空表初始化即 ≥1）→ 立即
            // 答现快照；此后每次应答到达即重臂（park-应答-park 循环 = 表
            // 变更推送的串行 RPC 建模，§8-①）。
            wm_send_wait(push, 0);
        }
        let mut msg = [0u8; 256];
        let mut rep = [0u8; gp::INPUT_REP_MAX];
        // recv 收口容量 4（协议随行 handle 定值 0 + 富余——P6 A-4 纪律：
        // 恰按定值收时垃圾消息的容量不足 -EINVAL 会误入 trap 杀服务）
        let mut hs = [0 as Handle; 4];
        // push 路收口同口径（快照应答零随行 + 富余）
        let mut phs = [0 as Handle; 4];
        let mut sub_delivery: Option<u8> = None;
        loop {
            harvest(d, regs, pump); // 空闲期收割（tick 心跳的消费点）
            // —— 路①：input 域（既有分派零扰动）
            let mut nb = msg.len();
            let mut nh = hs.len();
            let s = berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh);
            if s == 0 {
                // 随行 handle 分流（P6 A-4）：协议定值 0——非 0 恒 ST_BAD 不
                // trap；收到的礼貌关闭（EBUSY 类有界泄漏容错）
                if nh != 0 {
                    for i in 0..nh {
                        let _ = berry_handle_close(hs[i]);
                    }
                    let n = gp::encode_status_rep(&mut rep, gp::ST_BAD);
                    send_rep(h, &rep[..n]);
                    continue;
                }
                if nb == 0 {
                    let n = gp::encode_status_rep(&mut rep, gp::ST_BAD);
                    send_rep(h, &rep[..n]);
                    continue;
                }
                match msg[0] {
                    gp::OP_INPUT_SUBSCRIBE => match gp::parse_input_sub_req(&msg[..nb]) {
                        Some(dv) => {
                            // v0 服务端按 BATCHED 对待 IMMEDIATE（协议许可）；
                            // delivery 透传面——泵本体不消费（lib 注）
                            if sub_delivery != Some(dv) {
                                println!("gui-inputd: delivery={}（v0 按 BATCHED 对待）", dv);
                                sub_delivery = Some(dv);
                            }
                            let n = gp::encode_status_rep(&mut rep, gp::ST_OK);
                            send_rep(h, &rep[..n]);
                        }
                        None => {
                            let n = gp::encode_status_rep(&mut rep, gp::ST_BAD);
                            send_rep(h, &rep[..n]);
                        }
                    },
                    gp::OP_INPUT_POLL => match gp::parse_input_poll_req(&msg[..nb]) {
                        Some(max) => serve_poll(d, regs, pump, h, push, max, &mut rep),
                        None => {
                            let n = gp::encode_status_rep(&mut rep, gp::ST_BAD);
                            send_rep(h, &rep[..n]);
                        }
                    },
                    _ => {
                        // input 域只此两 op——未知 op 拒面（不 trap）
                        let n = gp::encode_status_rep(&mut rep, gp::ST_BAD);
                        send_rep(h, &rep[..n]);
                    }
                }
                continue;
            }
            if s != BERRY_ERR_AGAIN {
                println!("gui-inputd: recv 失败 {}", s);
                trap(); // 真链断 loud（错误码自理自陷只死自己）
            }
            // —— 路②：wmd 推送（push 端在席才走——0 时零新行零行为，
            // rootfs 相标记面零漂移的构造保证）
            if push != 0 {
                let mut nb = msg.len();
                let mut nh = phs.len();
                let s =
                    berry_channel_recv(push, msg.as_mut_ptr(), &mut nb, phs.as_mut_ptr(), &mut nh);
                if s == 0 {
                    // 快照应答零随行（协议面）——非 0 礼貌关闭（P6 A-4 同款）
                    if nh != 0 {
                        for i in 0..nh {
                            let _ = berry_handle_close(phs[i]);
                        }
                    }
                    wm_on_reply(push, &msg[..nb], pump);
                    continue;
                }
                if s != BERRY_ERR_AGAIN {
                    println!("gui-inputd: push recv 失败 {}", s);
                    trap(); // push 链 syscall 面断——真链断 loud
                }
                // 降级态重探测（Q11-③：缺席/超龄每帽值重发 WAIT(0)）
                wm_probe_if_degraded(push);
            }
            // GUI-4 段②占位（无代码）：PM 交互脉冲——输入事件到达即向
            // PM 服务投交互 hint（提频投票）的缝留这里。候选点三（GUI-5
            // §3 #10 口径）：serve 循环 100ms tick / EV_SYN 收批点 / wm
            // 快照到达点。**GUI 只投票不设频**（GUI 规划 GUI-4 行裁决：
            // PM 仍唯一决策者）；v1 无 PM 服务面，本刀零代码零标记——
            // GUI-6/+ 落地时回填。
            // 空闲合法态：阻塞等下个请求；tick = 心跳非死线——超时
            // 回环顶重查 used 环后重臂续等（P6 A-3 echo 同款）
            let items = [h, push];
            let sigs = [SIGNAL_READABLE, SIGNAL_READABLE];
            let n = if push != 0 { 2 } else { 1 };
            let mut idx = usize::MAX;
            let mut obs = 0u64;
            let _ = berry_object_wait_many(
                items.as_ptr(),
                sigs.as_ptr(),
                n,
                now() + SVC_TICK_NS,
                &mut idx,
                &mut obs,
            );
        }
    }
}
