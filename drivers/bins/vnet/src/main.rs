//! vnet —— P6 A-7 virtio-net 用户态驱动（ARP/ICMP ping 通）+ P6 A-8b 起
//! **常驻网络服务**（DNS 查询 RPC）+ T-c 起 TCP 自验会话（主动 connect
//! SLIRP 宿主回显服务，握手/双向/挥手全链——[`tcp`]，`TCP ok echo=`）。
//!
//! vprobe/vblk 同链（musl target + berry-libc shim；MMIO 窗口经
//! `SpawnSpec::mmio_vmo` 映到 [`VIRTIO_MMIO_VA`]，berry-exec 单源）。扫 32
//! 槽认领**唯一 net 设备**（did=1 按类计数——block 槽合法同席不扰，vprobe/
//! vblk 对称面）→ modern v2 协商（只收 NET_F_MAC + VERSION_1）→ config 读
//! MAC → 双队列建制（rx q0 + tx q1，qsize=16）→ 挂 16 个 rx 单描述符缓冲 →
//! DRIVER_OK → **ARP 解网关 MAC**（SLIRP user 网的 10.0.2.2 应答 broadcast
//! 请求）→ **ICMP echo 往返对拍**（32B payload 逐字节全等）→ 自打
//! `NET ok ping=32 slot=N`（rootfs 相第 17 标记，单消息原子）后**转常驻**
//! （A-8b，vblk A-4 同款纪律）：serve() 经 berry-netdproto 应答 DNS_QUERY
//! RPC（name → UDP:53 → SLIRP 虚拟 DNS 10.0.2.3 代答 → addr），空队 wait
//! READABLE 10s 帽超时重臂、client 侧一切错误 ST_BAD/ST_TIMEOUT 应答**服务
//! 不死**；无退出路径（A-7 瞬态探针的 STATUS=0 退出复位义务随之消失——
//! 复位只在 negotiate 起点用）。
//!
//! **virtio-net 请求头**（12B mrg 变体，[`vio::NET_HDR_SZ`]——协商
//! MRG_RXBUF 后收发同形，见 negotiate 注）：非 checksum/gso 驱动恒写全零，
//! 帧本体紧随同缓冲——rx 单描述符即整收（设备写 12B 头再写帧进
//! 同一 256B 缓冲（T-c TCP 扩容后仍单 desc：12+66+128=206 ≤ 256）；拆
//! hdr→frame 链式两 desc 需 32 个 > qsize 16 放不下，
//! 单 desc 是几何结论）。
//!
//! **SLIRP user 网事实**（`-netdev user` + `-device virtio-net-device`）：
//! 网关 10.0.2.2 应答对自身的 ARP、回 ICMP echo（保 id/seq/payload）；
//! guest 规范地址 10.0.2.15；**短帧补零到以太网最小线宽**（42B ARP 应答
//! 实测以 64B 帧/50B eth payload 到达——pcap 实证；解析按报文自身长度
//! 权威、容忍尾随 padding，见 berry-netproto parse_arp/parse_ipv4——**勿
//! 收紧回恰长执法**）。net 也吃 force-legacy=false（ver==2）。
//!
//! **DMA 面**（vblk 同款纯 VMO 机制——4 页从不进任何地址空间）：页 0 rx
//! ring / 页 1 tx ring / 页 2 rx 缓冲 16×256B（恰满单页，§5）/ 页 3 tx
//! 缓冲。[`TXRING`]
//! 暂存是 tx ring 的驱动侧权威副本（avail.idx 跨帧续用——vblk RING 同款；
//! 整页写在门铃前、上一帧已 poll 完成后——同步单在途纪律下设备对 tx ring
//! 空闲，无并发写面）；rx ring 的 used 环设备拥有，驱动只读（2B 短读 idx +
//! 整页读入 [`RXSTAGE`] 取 used 项）。**rx 缓冲收割即回填**（A-8b——v1
//! 探针不回填、16 缓冲耗尽即假死；常驻面靠 [`refill_rx`] 字节粒度续挂同
//! id 重发布，见 refill 注）。
//!
//! 一致性口径（vblk 同款诚实缺口）：门铃前 `dmb ish`（publish() 调用方
//! 契约）；cache 维护（dc cvac/ivac）留 v2 DMA API；中断面 = poll used
//! ring（10s 帽 + 1ms 睡步进），GIC 不使能 virtio SPI；INT_STATUS 读 +
//! ACK 整洁面。链断 → 诊断行 + trap 只死自己（`NET ok` 缺失 = smoke FAIL
//! 兜底）。

use berry_exec::{VIRTIO_MMIO_SLOTS, VIRTIO_MMIO_STRIDE, VIRTIO_MMIO_VA};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_object_wait_one, berry_thread_sleep, berry_vmo_commit, berry_vmo_create,
    berry_vmo_get_phys, berry_vmo_read, berry_vmo_write, Handle, BERRY_ERR_AGAIN,
    BERRY_ERR_BUSY, CLOCK_MONOTONIC, SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_virtio as vio;
// A-7：最小以太网/ARP/IPv4/ICMP-echo 编解码（「解析器即产品」第 5 例）。
use berry_netproto as np;
// A-8：网络服务 RPC 协议（DNS_QUERY 单 op 严格串行——服务端侧）。
use berry_netdproto as ndp;
use berry_libc::io::gift2_h;
use core::sync::atomic::{AtomicU16, Ordering};

// T-c：TCP 自验会话模块（berry-tcp 状态机 × 本 crate IO 驱动——插入点在
// main 的 `NET ok` 之后、serve 之前）。
mod tcp;

// 链接种子（vprobe/vblk 同款）：无引用则 berry-libc 的 crt0 不进链接。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// 队列深（vblk 同档：2 的幂、整环落一页；QEMU virtio-net QueueNumMax
/// 远大于此）。
const QSIZE: u16 = 16;
/// rx 缓冲数（= qsize：每缓冲一描述符整收一帧）。
const RX_BUFS: usize = 16;
/// rx 缓冲幅：12B net 头 + ≤244B 帧（T-c TCP 扩容：帧开销 66（vnet_hdr
/// + eth + IP + TCP 头）+ MSS 128 = 194 上限 + 选项余量；16×256 = 4096
/// 恰满 RX_BUF_PAGE 单页——DMA 四页排布零重排，「rx 单描述符整收」几何
/// 结论不变）。消费侧 tcp.rs const assert 机器钉住（T-c ③）。
pub(crate) const RX_BUF_SZ: usize = 256;
/// DMA VMO 几何：页 0 rx ring / 页 1 tx ring / 页 2 rx 缓冲带 / 页 3 tx
/// 缓冲（各结构单页内——desc 地址域不跨页，vblk 同前提）。
const DMA_LEN: u64 = 16384;
const RX_RING_PAGE: u64 = 0;
const TX_RING_PAGE: u64 = 4096;
const RX_BUF_PAGE: u64 = 8192;
const TX_BUF_PAGE: u64 = 12288;
/// poll 帽：10s（SLIRP 应答在 ms 级——超帽 = 传输层/几何断了，loud）。
const POLL_CAP_NS: u64 = 10_000_000_000;
/// poll 步进：1ms 绝对 deadline 睡（让出 CPU，不空转烧 slice）。
const POLL_STEP_NS: u64 = 1_000_000;
/// STATUS 复位回读帽：1s（真机复位 ms 级；QEMU 同步即 0）。
const RESET_CAP_NS: u64 = 1_000_000_000;
/// SLIRP user 网：guest 规范地址 / 网关。
const OUR_IP: [u8; 4] = [10, 0, 2, 15];
const GW_IP: [u8; 4] = [10, 0, 2, 2];
const BROADCAST_MAC: [u8; 6] = [0xff; 6];
/// echo 载荷（32B——标记 ping 计数与往返对拍基准；id 0x6265 = "be"）。
const ICMP_PAYLOAD: &[u8; 32] = b"berryos-vnet-ping-0123456789abcd";
const ICMP_ID: u16 = 0x6265;
const _: () = assert!(ICMP_PAYLOAD.len() == 32);
/// SLIRP user 网：虚拟 DNS = **10.0.2.3**（非网关——guest 发往它的 UDP:53
/// 由宿主网栈代答；A-8 标记的地基："localhost" → 宿主 /etc/hosts →
/// 127.0.0.1 离线确定，无外部网络依赖）。
const DNS_IP: [u8; 4] = [10, 0, 2, 3];
const DNS_PORT: u16 = 53;
/// DNS 查询源端口（固定——过滤链按四元组 + sport/dport 精确匹配，SLIRP
/// 原路回发）。
const DNS_SPORT: u16 = 5353;
/// DNS 查询整体帽（服务侧 20s < client 侧 60s 应答帽（A-8c svcmgr
/// `DNS_REPLY_CAP` 实定——worst 路径含本服务自验尾巴 + 本帽 + 余量）——
/// ST_TIMEOUT 应答先于 client 超时到达，vblk BLKD「服务帽 < client 帽」
/// 同款纪律）。
const DNS_CAP_NS: u64 = 20_000_000_000;
/// serve() 空闲/wait 重臂帽：10s（常驻服务空闲合法面——vblk SVC_IDLE
/// 同款，超时重臂非链断）。
const SVC_IDLE_NS: u64 = 10_000_000_000;
/// DNS 查询 id 计数器（每查询 fetch_add——应答按 id 精确匹配，杂散/迟到
/// 应答天然滤除；u16 回卷 = id 空间复用，与「id 匹配」过滤共存）。Relaxed
/// 单消费者足够。
static DNS_ID: AtomicU16 = AtomicU16::new(0);
/// TCP 服务三 op 相位帽（T-d，均 < client 侧 svcmgr TCPS_REPLY_CAP 60s——
/// vblk BLKD「服务帽 < client 帽」同款纪律）。OPEN 20s（握手 ~3 帧往返）/
/// ROUNDTRIP 30s（数据+回显+ACK ~3 帧）/ CLOSE 30s（挥手四帧 + TIME_WAIT
/// 2s——TCG 单帧 poll ~1s 级，§6 RPC 成本模型）。
const TCPS_OPEN_CAP_NS: u64 = 20_000_000_000;
const TCPS_RT_CAP_NS: u64 = 30_000_000_000;
const TCPS_CLOSE_CAP_NS: u64 = 30_000_000_000;
/// TCP 服务会话源端口发号器（自验固定 48080 之外从 48081 起——两会话源口
/// 错开；u16 回卷 = 端口空间复用，会话内四元组按现值匹配）。Relaxed 单
/// 消费者（serve 串行）足够。
static TCP_SPORT_NEXT: AtomicU16 = AtomicU16::new(48081);

// SAFETY: 调用方单线程（main 顺序流），三窗各自独占；static 免大栈帧。
/// tx ring 驱动侧权威暂存（desc 0 + avail 跨帧续用——vblk RING 同款）。
static mut TXRING: [u8; 4096] = [0; 4096];
/// rx ring 暂存（初始挂缓冲时组页；收割时整页读入取 used 项——只读面，
/// used 环设备拥有）。
static mut RXSTAGE: [u8; 4096] = [0; 4096];
/// 帧暂存尺寸常量（静态数组与 tcp.rs 消费侧 const assert 同源——T-c ③）。
pub(crate) const IOB_SZ: usize = 512;
/// 帧暂存（tx 组 12B 头 + 帧 / rx 收帧；512 ≥ 12+186——TCP 段最大帧 =
/// eth 14 + IP 20 + 段 152（头 20 + MSS 选项 4 + 载荷 128））。
static mut IOB: [u8; IOB_SZ] = [0; IOB_SZ];

/// DMA VMO + 双环几何（vblk Dma 同形扩双队列）。
struct Dma {
    vmo: Handle,
    lay: vio::RingLayout,
    rx_ring_pa: u64,
    tx_ring_pa: u64,
    rx_buf_pa: u64,
    tx_buf_pa: u64,
    /// rx 已收割数（与设备 used.idx 对账的自由运行 u16）。
    rx_harvested: u16,
    /// rx 已发布数（avail.idx 的驱动侧权威——refill 续挂对账用）。
    rx_posted: u16,
}

fn main() {
    let slot = find_device();
    let regs = mmio(slot);
    negotiate(regs);
    let mac = read_mac(regs);
    if mac == [0u8; 6] || mac[0] & 1 != 0 {
        // 全零 = config 未生效（F_MAC 未收成？）；组播位 = 非单播地址——
        // QEMU 默认 52:54:00:12:34:56 确定性单播，两异象都 loud。
        println!("vnet: bad mac {:?}", mac);
        trap();
    }
    let mut d = setup_dma();
    // 队列 0 = rx（DRIVER_OK 前先挂缓冲——设备从此有缓冲可写）；1 = tx。
    setup_queue(regs, 0, d.rx_ring_pa, &d.lay);
    setup_queue(regs, 1, d.tx_ring_pa, &d.lay);
    post_rx(&mut d);
    wr(
        regs,
        vio::mmio::STATUS,
        vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK | vio::ST_DRIVER_OK,
    );
    // rx 初始填充后的 kick（规范整洁面——QEMU 在 DRIVER_OK 后自会消费
    // avail，门铃对 rx 非必需）；dmb 先行：post_rx 的整页写在门铃前排序。
    // SAFETY: 纯屏障指令，无内存操作数。
    unsafe { core::arch::asm!("dmb ish") };
    wr(regs, vio::mmio::Q_NOTIFY, 0);
    let gw_mac = arp_roundtrip(&mut d, regs, &mac);
    icmp_roundtrip(&mut d, regs, &mac, &gw_mac);
    println!("NET ok ping={} slot={}", ICMP_PAYLOAD.len(), slot);
    // T-c：TCP 自验会话（主动 connect 10.0.2.2:18080——QEMU user 网 10.0.2.2
    // = host 本机，SLIRP 转发宿主 127.0.0.1 同端口，`tools/echod.py` 回显；
    // 打 `TCP ok echo=64` = rootfs 相第 19 标记；失败 println 诊断 + trap
    // loud）。
    tcp::selftest(&mut d, regs, &mac, &gw_mac);
    // A-8b：自验毕**转常驻**（不再退出——A-7 瞬态探针的 STATUS=0 退出复位
    // 随退出路径消失，复位只在 negotiate 起点用；DMA VMO 服务存续恒用不
    // 回收）。svcmgr 预拉后不 reap（vblk A-4 同款取舍：wait TERMINATED 30s
    // 帽对不退出的服务必超时空烧，表槽有界 = 服务生命周期）。
    serve(&mut d, regs, &mac, &gw_mac);
}

/// 扫 32 槽认领唯一 net 设备（vblk find_device 对称面——**按设备类计数**：
/// 只认 did==net 恰一，block 槽合法同席不算；未知 did 的 loud 面由 vprobe
/// 全窗扫描兜）。细节见 vprobe 头注。
fn find_device() -> usize {
    // SAFETY: 窗口经 spawn 映射（Device-nGnRnE）；逐字 volatile；只读
    // ID 头四字 + 特性/队列寄存器（写面在协商序内，见 main）。
    let base = VIRTIO_MMIO_VA as *const u32;
    let mut occupied: Option<usize> = None;
    let mut n = 0usize;
    let mut n_occupied = 0usize;
    for slot in 0..VIRTIO_MMIO_SLOTS {
        let regs = unsafe { base.add(slot * (VIRTIO_MMIO_STRIDE / 4)) };
        if unsafe { core::ptr::read_volatile(regs) } != vio::mmio::MAGIC_VAL {
            println!("vnet: slot {} bad magic", slot);
            trap();
        }
        let did = unsafe { core::ptr::read_volatile(regs.add(vio::mmio::DEVICE_ID / 4)) };
        if did != 0 {
            n_occupied += 1;
            if did == vio::mmio::DEVICE_ID_NETWORK {
                n += 1;
                occupied = Some(slot);
            }
        }
    }
    let slot = match (n, occupied) {
        (1, Some(s)) => s,
        _ => {
            println!(
                "vnet: net slots = {}（占用共 {}，net 须恰 1）",
                n, n_occupied
            );
            trap();
        }
    };
    let regs = mmio(slot);
    let (ver, did) = (
        unsafe { core::ptr::read_volatile(regs.add(vio::mmio::VERSION / 4)) },
        unsafe { core::ptr::read_volatile(regs.add(vio::mmio::DEVICE_ID / 4)) },
    );
    if ver != vio::mmio::VERSION_MODERN || did != vio::mmio::DEVICE_ID_NETWORK {
        println!("vnet: slot {} ver={} did={}", slot, ver, did);
        trap(); // legacy 模式（缺 force-legacy=false）或非 net
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

/// 单调钟现值（失败 = clock 面断链，loud）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}

/// STATUS=0 复位 + 回读确认（1s 帽）。A-8b 起**唯一调用点 = negotiate()
/// 协商序起点**（真机复位异步、未完成即置位可致状态机错乱——Linux
/// virtio_mmio 同款回读等）；A-7 的「退出序终点复位」已随退出路径删除
/// （serve() 常驻 `-> !` 不退出，见模块头注）。**未来若加退出/重启路径
/// （v2 服务监督方向），退出前必须复用本函数撤队列断 DMA 面**——rx
/// avail 挂着指向 DMA VMO 页的缓冲，SLIRP 周期性 IPv6 RA 会不请自来，
/// 不复位 = 设备 DMA 写已还帧（A-7 对抗审查 medium 实抓形态）。
fn reset_device(regs: *mut u32) {
    wr(regs, vio::mmio::STATUS, 0);
    let mut t = now();
    let reset_deadline = t + RESET_CAP_NS;
    while rd(regs, vio::mmio::STATUS) != 0 {
        t = now();
        if t >= reset_deadline {
            println!("vnet: STATUS 复位回读超时");
            trap();
        }
        if berry_thread_sleep(t + POLL_STEP_NS) != 0 {
            trap();
        }
    }
}

/// modern 协商（vblk 同形）：复位 → ACK|DRIVER → 特性页读 → 收
/// NET_F_MAC + NET_F_MRG_RXBUF（页 0）+ VERSION_1（页 1）→ FEATURES_OK
/// 回读验。MAC 是 config 读取前提——不收 F_MAC 则 config.mac 无承诺
/// （规范 §6.5）；MRG_RXBUF 把 net 头钉成 12B 规范值（见 berry-virtio
/// NET_F_MRG_RXBUF 注——QEMU VERSION_1 路径本就按 12 剥头，不协商 =
/// 10B 排包被斩头、SLIRP 静默丢包 rx 假超时）。
fn negotiate(regs: *mut u32) {
    // 规范复位序起点（真机复位异步、未完成即置位可致状态机错乱——
    // Linux virtio_mmio 同款回读等）。
    reset_device(regs);
    wr(regs, vio::mmio::STATUS, vio::ST_ACKNOWLEDGE | vio::ST_DRIVER);
    wr(regs, vio::mmio::DEV_FEAT_SEL, 0);
    let f0 = rd(regs, vio::mmio::DEV_FEAT);
    if f0 & vio::NET_F_MAC == 0 {
        println!("vnet: 无 NET_F_MAC（f0={:#x}）", f0);
        trap(); // QEMU virtio-net 恒供；缺席 = 传输层异常
    }
    if f0 & vio::NET_F_MRG_RXBUF == 0 {
        println!("vnet: 无 NET_F_MRG_RXBUF（f0={:#x}）", f0);
        trap(); // QEMU virtio-net 恒供；缺席 = 传输层异常
    }
    wr(regs, vio::mmio::DEV_FEAT_SEL, 1);
    let f1 = rd(regs, vio::mmio::DEV_FEAT);
    if f1 & vio::FEAT_VERSION_1 == 0 {
        println!("vnet: 无 VERSION_1（f1={:#x}）", f1);
        trap(); // modern 传输层必供（规范 MUST）
    }
    wr(regs, vio::mmio::DRV_FEAT_SEL, 0);
    wr(regs, vio::mmio::DRV_FEAT, vio::NET_F_MAC | vio::NET_F_MRG_RXBUF);
    wr(regs, vio::mmio::DRV_FEAT_SEL, 1);
    wr(regs, vio::mmio::DRV_FEAT, vio::FEAT_VERSION_1);
    wr(
        regs,
        vio::mmio::STATUS,
        vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK,
    );
    let st = rd(regs, vio::mmio::STATUS);
    if st != vio::ST_ACKNOWLEDGE | vio::ST_DRIVER | vio::ST_FEATURES_OK {
        println!("vnet: FEATURES_OK 回读 {:#x}", st);
        trap(); // 设备撤 FEATURES_OK/置 FAILED = 协商拒绝
    }
}

/// 读 MAC（设备类 config 区 @0x100 起前 6 字，规范 §5.1.4）：两次 u32
/// volatile 读拼低 6 字节。收了 NET_F_MAC 后 config.mac 有效。
fn read_mac(regs: *mut u32) -> [u8; 6] {
    let lo = rd(regs, vio::mmio::CONFIG);
    let hi = rd(regs, vio::mmio::CONFIG + 4);
    [
        lo as u8,
        (lo >> 8) as u8,
        (lo >> 16) as u8,
        (lo >> 24) as u8,
        hi as u8,
        (hi >> 8) as u8,
    ]
}

/// DMA VMO + 几何（vblk 同款纯 VMO 机制——从不进任何地址空间）：环构造
/// 与收割全走 vmo_write/vmo_read 暂存，描述符地址 0x1B get_phys 查后备。
fn setup_dma() -> Dma {
    let lay = match vio::RingLayout::new(QSIZE as usize) {
        Some(l) => l,
        None => trap(), // qsize=16 恒成立；防御面（布局几何是编译期已知）
    };
    let mut vmo = 0 as Handle;
    if berry_vmo_create(DMA_LEN, 0, &mut vmo) != 0 || vmo == 0 {
        println!("vnet: vmo_create 失败");
        trap();
    }
    if berry_vmo_commit(vmo, 0, DMA_LEN) != 0 {
        println!("vnet: commit 失败");
        trap();
    }
    // 页后备 PA（已 commit——NotCommitted 面不可达，防御行）。
    let mut rx_ring_pa = 0u64;
    let mut tx_ring_pa = 0u64;
    let mut rx_buf_pa = 0u64;
    let mut tx_buf_pa = 0u64;
    if berry_vmo_get_phys(vmo, RX_RING_PAGE, &mut rx_ring_pa) != 0
        || berry_vmo_get_phys(vmo, TX_RING_PAGE, &mut tx_ring_pa) != 0
        || berry_vmo_get_phys(vmo, RX_BUF_PAGE, &mut rx_buf_pa) != 0
        || berry_vmo_get_phys(vmo, TX_BUF_PAGE, &mut tx_buf_pa) != 0
    {
        println!("vnet: get_phys 失败");
        trap();
    }
    Dma {
        vmo,
        lay,
        rx_ring_pa,
        tx_ring_pa,
        rx_buf_pa,
        tx_buf_pa,
        rx_harvested: 0,
        rx_posted: 0,
    }
}

/// 建队列 qsel（vblk setup_queue 同形，队列号参数化——net 双队列 rx=0/
/// tx=1）：Q_SEL → QueueNumMax 检 → Q_NUM → Q_READY=0 → 三址 LOW+HIGH →
/// Q_READY=1 回读（modern 序）。
fn setup_queue(regs: *mut u32, qsel: u32, ring_pa: u64, lay: &vio::RingLayout) {
    wr(regs, vio::mmio::Q_SEL, qsel);
    let qmax = rd(regs, vio::mmio::Q_NUM_MAX);
    if qmax < QSIZE as u32 {
        println!("vnet: q{} QueueNumMax={} < {}", qsel, qmax, QSIZE);
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
        println!("vnet: q{} Q_READY 回读 != 1", qsel);
        trap();
    }
}

/// 挂 16 个 rx 单描述符缓冲（页 2 逐 256B 一格，DESC_F_WRITE——设备写）：
/// RXSTAGE 零基（fresh commit 页内核保证清零——.bss 同款依赖；整页写与
/// 设备侧零页一致）组 desc 表 + avail 环（avail.ring[i] = i——第 i 次
/// publish 落 head=i）→ 整页落盘。
fn post_rx(d: &mut Dma) {
    // SAFETY: 单线程独占窗；RXSTAGE 首次使用即零基。
    let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(RXSTAGE) };
    for i in 0..RX_BUFS {
        let addr = d.rx_buf_pa + (i * RX_BUF_SZ) as u64;
        let _ = vio::put_desc(
            ring,
            &d.lay,
            i,
            addr,
            RX_BUF_SZ as u32,
            vio::DESC_F_WRITE,
            0,
        );
    }
    for i in 0..RX_BUFS {
        if vio::publish(ring, &d.lay, i as u16).is_none() {
            trap(); // 布局几何恒在页内；防御面
        }
    }
    if berry_vmo_write(d.vmo, ring.as_ptr(), RX_RING_PAGE, 4096) != 0 {
        trap();
    }
    d.rx_posted = RX_BUFS as u16;
}

/// 收割后回填 rx 缓冲（A-8b 常驻面的关键——v1 探针不回填、16 缓冲耗尽
/// 即 rx 永久假死）：**同 id 重发布**（desc 表 addr/len/WRITE 恒有效不重
/// 写），只动 avail 区两处 2B——`avail.ring[posted % qsize] = id` 槽 +
/// `avail.idx = posted+1`（**次序 = 正确性**：槽先落、idx 后进——设备按
/// idx 变化消费新项）→ `dmb ish` → 门铃 q0。**不可整页写回**：RXSTAGE
/// 快照含设备拥有的 used 环，读写窗之间设备新写的完成项会被陈旧快照砸掉
/// （丢完成 → 假死）——字节粒度 vmo_write 只碰 avail 区是唯一安全窗
/// （avail 环驱动拥有，desc/used 各归其主，virtio split ring 所有权分界）。
fn refill_rx(d: &mut Dma, regs: *mut u32, id: u16) {
    let slot = (d.rx_posted % d.lay.qsize as u16) as usize;
    let b = [id as u8, (id >> 8) as u8];
    let off = RX_RING_PAGE + d.lay.avail_off as u64 + 4 + (slot as u64) * 2;
    if berry_vmo_write(d.vmo, b.as_ptr(), off, 2) != 0 {
        trap();
    }
    d.rx_posted = d.rx_posted.wrapping_add(1);
    let b = [d.rx_posted as u8, (d.rx_posted >> 8) as u8];
    let off = RX_RING_PAGE + d.lay.avail_off as u64 + 2;
    if berry_vmo_write(d.vmo, b.as_ptr(), off, 2) != 0 {
        trap();
    }
    // 发布屏障（publish 同款调用方契约）：avail 两写在门铃前排序。
    // SAFETY: 纯屏障指令，无内存操作数。
    unsafe { core::arch::asm!("dmb ish") };
    wr(regs, vio::mmio::Q_NOTIFY, 0);
}

/// 发一帧（同步单在途，vblk blk_op 同纪律）：**帧字节由调用方预先组在
/// IOB[NET_HDR_SZ..] 连续区间**（arp/icmp_roundtrip 就地编码——同缓冲组
/// 帧再整段搬移是 copy_from_slice 自重叠 = memcpy 契约 UB，对抗审查实抓
/// 后撤掉搬移步；tx 只补 12B 零头）→ 缓冲页落盘 → TXRING 暂存重排
/// （desc 0 + publish——**avail.idx 留在暂存跨帧续用**）→ 整页 vmo_write
/// → `dmb ish` → 门铃 q1 → poll used.idx 前进。整页写在门铃前、上一帧
/// 已 poll 完成后——单在途纪律下设备对 tx ring 空闲，无并发写面（vblk
/// 全页写同款安全性论证）。
fn tx_send(d: &Dma, regs: *mut u32, frame_len: usize) {
    // SAFETY: 单线程独占窗（同 vblk 惯例）。
    let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(TXRING) };
    let iob: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(IOB) };
    let total = vio::NET_HDR_SZ + frame_len;
    if total > iob.len() {
        println!("vnet: tx 帧超幅 {}B", total);
        trap(); // 调用方帧幅由常量约束（TCP 段最大 total ≤198B）；防御面
    }
    iob[..vio::NET_HDR_SZ].fill(0); // 非 checksum/gso 驱动恒零头（规范 §5.1.6）
    if berry_vmo_write(d.vmo, iob.as_ptr(), TX_BUF_PAGE, total) != 0 {
        trap();
    }
    let _ = vio::put_desc(ring, &d.lay, 0, d.tx_buf_pa, total as u32, 0, 0);
    let expect = {
        let old = vio::get_u16(ring, d.lay.avail_off + 2).unwrap_or(0);
        if vio::publish(ring, &d.lay, 0).is_none() {
            trap(); // 布局几何恒在页内；防御面
        }
        old.wrapping_add(1)
    };
    if berry_vmo_write(d.vmo, ring.as_ptr(), TX_RING_PAGE, 4096) != 0 {
        trap();
    }
    // 发布屏障（publish() 调用方契约，vblk 同位）：此前一切 Normal 写
    // （缓冲页 + ring 页经 SVC 落后备帧）→ Device 门铃写排序。TCG 无内存
    // 模型恒绿；真机缺此 = 设备取陈旧 avail.idx → poll 假超时。
    // SAFETY: 纯屏障指令，无内存操作数。
    unsafe { core::arch::asm!("dmb ish") };
    wr(regs, vio::mmio::Q_NOTIFY, 1);
    // poll：used.idx 前进（2B 短读、偏移 used_off+2——used_off 本身是
    // flags 位恒 0，读错位 = 恒不前进的假超时，vblk 排障实踩）。
    let mut now = now();
    let deadline = now + POLL_CAP_NS;
    loop {
        let mut u = [0u8; 2];
        if berry_vmo_read(d.vmo, u.as_mut_ptr(), TX_RING_PAGE + d.lay.used_off as u64 + 2, 2) != 0 {
            trap();
        }
        if u[0] as u16 | (u[1] as u16) << 8 == expect {
            break;
        }
        if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 || now >= deadline {
            println!("vnet: tx used ring 超时（期 idx={}）", expect);
            trap();
        }
        if berry_thread_sleep(now + POLL_STEP_NS) != 0 {
            trap();
        }
    }
    // 整洁面：中断状态确认（vblk 同款——poll 模型下 ACK 清之）。
    let isr = rd(regs, vio::mmio::INT_STATUS);
    if isr != 0 {
        wr(regs, vio::mmio::INT_ACK, isr);
    }
}

/// 收一帧（阻塞至有新完成项或 deadline 到点）：poll rx used.idx 前进（2B
/// 短读）→ 整页读 rx ring 入暂存（只读——used 环设备拥有）→ used_ent 取
/// (id,len) → 短读对应缓冲 → **refill 回填**（A-8b 常驻面——见
/// [`refill_rx`]）→ 返 [`IOB`] 的帧视图（跳过 12B net 头；调用方即用即
/// 弃——下一次收帧覆盖，单线程独占窗纪律）。到点返 None——超时语义由
/// 调用方定夺（自验路径带上下文 trap / DNS 路径映射 ST_TIMEOUT）；clock
/// 面失败仍 trap（时钟 = 启动关键原语，真断链）。
fn recv_frame(d: &mut Dma, regs: *mut u32, deadline: u64) -> Option<&'static [u8]> {
    loop {
        let mut u = [0u8; 2];
        if berry_vmo_read(d.vmo, u.as_mut_ptr(), RX_RING_PAGE + d.lay.used_off as u64 + 2, 2) != 0 {
            trap();
        }
        if u[0] as u16 | (u[1] as u16) << 8 != d.rx_harvested {
            // SAFETY: 单线程独占窗。
            let ring: &mut [u8; 4096] = unsafe { &mut *core::ptr::addr_of_mut!(RXSTAGE) };
            if berry_vmo_read(d.vmo, ring.as_mut_ptr(), RX_RING_PAGE, 4096) != 0 {
                trap();
            }
            let i = (d.rx_harvested % d.lay.qsize as u16) as usize;
            let (id, len) = match vio::used_ent(ring, &d.lay, i) {
                Some(e) => e,
                None => trap(), // i < qsize 恒成立；防御面
            };
            if id as usize >= RX_BUFS
                || len < vio::NET_HDR_SZ as u32
                || len as usize > RX_BUF_SZ
            {
                println!("vnet: rx 坏完成项 id={} len={}", id, len);
                trap();
            }
            // SAFETY: 帧读入静态 IOB 后经 'static 视图返出（即用即弃）。
            let iob: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(IOB) };
            if berry_vmo_read(
                d.vmo,
                iob.as_mut_ptr(),
                RX_BUF_PAGE + id as u64 * RX_BUF_SZ as u64,
                len as usize,
            ) != 0
            {
                trap();
            }
            d.rx_harvested = d.rx_harvested.wrapping_add(1);
            // id 已过 < RX_BUFS 界检——u32→u16 收窄安全。
            refill_rx(d, regs, id as u16);
            let iob: &[u8; 512] = unsafe { &*core::ptr::addr_of!(IOB) };
            return Some(&iob[vio::NET_HDR_SZ..len as usize]);
        }
        let mut now = now();
        if berry_clock_read(CLOCK_MONOTONIC, &mut now) != 0 {
            trap();
        }
        if now >= deadline {
            return None;
        }
        if berry_thread_sleep(now + POLL_STEP_NS) != 0 {
            trap();
        }
    }
}

/// ARP 往返：broadcast 请求 who-has 10.0.2.2 → 收 SLIRP 网关应答
/// （oper=REPLY 且 spa==GW_IP 且 tpa==OUR_IP）取其 MAC。非目标帧（杂散
/// ARP 请求等）跳过重收；整体 10s 帽兜底。
fn arp_roundtrip(d: &mut Dma, regs: *mut u32, mac: &[u8; 6]) -> [u8; 6] {
    {
        // SAFETY: 单线程独占窗。
        let iob: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(IOB) };
        if np::encode_eth(&mut iob[vio::NET_HDR_SZ..], &BROADCAST_MAC, mac, np::ET_ARP).is_none() {
            trap(); // 42B 帧恒在 512 内；防御面
        }
        if np::encode_arp(
            &mut iob[vio::NET_HDR_SZ + np::ETH_HDR_SZ..],
            np::ARP_OPER_REQ,
            mac,
            OUR_IP,
            &[0; 6],
            GW_IP,
        )
        .is_none()
        {
            trap();
        }
    }
    let frame_len = np::ETH_HDR_SZ + np::ARP_LEN;
    // 帧字节已在 IOB[NET_HDR_SZ..] 连续（上段就地编码）——tx_send 只补
    // 零头 + 暂存/门铃，不再搬移（自重叠拷贝 = UB，对抗审查实抓后撤）。
    tx_send(d, regs, frame_len);
    let deadline = now() + POLL_CAP_NS;
    loop {
        let f = match recv_frame(d, regs, deadline) {
            Some(f) => f,
            None => {
                println!("vnet: ARP 应答超时");
                trap();
            }
        };
        if let Some(e) = np::parse_eth(f) {
            if e.ethertype == np::ET_ARP {
                if let Some(a) = np::parse_arp(e.payload) {
                    if a.oper == np::ARP_OPER_REPLY && a.spa == GW_IP && a.tpa == OUR_IP {
                        return a.sha; // 网关 MAC（应答源硬件地址）
                    }
                }
            }
        }
    }
}

/// ICMP echo 往返：对网关发 echo req（ttl=64，32B payload）→ 收 reply 且
/// src/dst/proto 对号 + id/seq/payload 逐字节全等才过。非目标帧跳过重收；
/// 整体 10s 帽兜底。
fn icmp_roundtrip(d: &mut Dma, regs: *mut u32, mac: &[u8; 6], gw_mac: &[u8; 6]) {
    let mut icmp = [0u8; np::ICMP_HDR_SZ + ICMP_PAYLOAD.len()];
    if np::encode_icmp_echo(&mut icmp, np::ICMP_TY_ECHO_REQ, ICMP_ID, 1, &ICMP_PAYLOAD[..])
        .is_none()
    {
        trap(); // 40B 恒成立；防御面
    }
    {
        // SAFETY: 单线程独占窗。
        let iob: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(IOB) };
        if np::encode_eth(&mut iob[vio::NET_HDR_SZ..], gw_mac, mac, np::ET_IPV4).is_none() {
            trap();
        }
        if np::encode_ipv4(
            &mut iob[vio::NET_HDR_SZ + np::ETH_HDR_SZ..],
            64,
            np::IP_PROTO_ICMP,
            OUR_IP,
            GW_IP,
            &icmp,
        )
        .is_none()
        {
            trap();
        }
    }
    // icmp 切片已含 8B ICMP 头（ICMP_HDR_SZ + payload）——勿再显式加头长
    // （首版 +8 = 发送切片多带 IOB 尾零，逃过是因 IP total_len 权威、
    // 接收方按 total 截；线上形态该是恰好 74B）。
    let frame_len = np::ETH_HDR_SZ + np::IPV4_HDR_SZ + icmp.len();
    // 帧字节已在 IOB[NET_HDR_SZ..] 连续（同 ARP 段）。
    tx_send(d, regs, frame_len);
    let deadline = now() + POLL_CAP_NS;
    loop {
        let f = match recv_frame(d, regs, deadline) {
            Some(f) => f,
            None => {
                println!("vnet: ICMP 应答超时");
                trap();
            }
        };
        if let Some(e) = np::parse_eth(f) {
            if e.ethertype == np::ET_IPV4 {
                if let Some(ip) = np::parse_ipv4(e.payload) {
                    if ip.src == GW_IP && ip.dst == OUR_IP && ip.proto == np::IP_PROTO_ICMP {
                        if let Some(r) = np::parse_icmp_echo(ip.payload) {
                            if r.ty == np::ICMP_TY_ECHO_REPLY
                                && r.id == ICMP_ID
                                && r.seq == 1
                                && r.payload == &ICMP_PAYLOAD[..]
                            {
                                return; // echo 往返逐字节对拍通过
                            }
                        }
                    }
                }
            }
        }
    }
}

/// DNS 查询结果三态（服务应答映射：Ok → ST_OK+addr / Bad → ST_BAD /
/// Timeout → ST_TIMEOUT）。
enum DnsRes {
    Ok([u8; 4]),
    Bad,
    Timeout,
}

/// DNS A 查询往返（A-8b 服务路径）：`encode_dns_query_a`（DNS_ID 计数器
/// 发号——应答按 id 精确匹配滤杂散）→ UDP（src 10.0.2.15:5353 → dst
/// 10.0.2.3:53，SLIRP 虚拟 DNS）→ IPv4 → eth（网关 MAC——10.0.2.3 非
/// 链路内直连，帧仍走网关转发）→ tx_send；等应答整体帽 [`DNS_CAP_NS`]
/// （< client 侧 60s 收侧帽 svcmgr `DNS_REPLY_CAP`——见上方 [`DNS_CAP_NS`]
/// 注；服务慢 ≠ 服务死由 client 帽兜底），按
/// src==10.0.2.3 + sport==53 + dport==5353 + id 匹配收
/// `parse_dns_a_reply`；非目标帧（杂散 ARP/ICMP、IPv6 RA 等）跳过重收。
/// 超幅应答（>rx 256B 缓冲）在 recv_frame 完成项检查处已拒——DNS 应答
/// localhost 实测 ~70B 安全；诚实缺口文档化：巨型应答 = Timeout 非 Bad。
fn dns_roundtrip(
    d: &mut Dma,
    regs: *mut u32,
    mac: &[u8; 6],
    gw_mac: &[u8; 6],
    name: &str,
) -> DnsRes {
    let id = DNS_ID.fetch_add(1, Ordering::Relaxed);
    let mut q = [0u8; 128];
    let qlen = match np::encode_dns_query_a(&mut q, id, name) {
        Some(n) => n,
        None => return DnsRes::Bad, // 名越 wire 帽（NAME_MAX=64 已拦大头）；防御面
    };
    let mut u = [0u8; 128];
    let udp_len = np::UDP_HDR_SZ + qlen;
    if np::encode_udp(&mut u, OUR_IP, DNS_IP, DNS_SPORT, DNS_PORT, &q[..qlen]).is_none() {
        return DnsRes::Bad;
    }
    {
        // SAFETY: 单线程独占窗。
        let iob: &mut [u8; 512] = unsafe { &mut *core::ptr::addr_of_mut!(IOB) };
        if np::encode_eth(&mut iob[vio::NET_HDR_SZ..], gw_mac, mac, np::ET_IPV4).is_none() {
            return DnsRes::Bad;
        }
        if np::encode_ipv4(
            &mut iob[vio::NET_HDR_SZ + np::ETH_HDR_SZ..],
            64,
            np::IP_PROTO_UDP,
            OUR_IP,
            DNS_IP,
            &u[..udp_len],
        )
        .is_none()
        {
            return DnsRes::Bad;
        }
    }
    // 帧字节已在 IOB[NET_HDR_SZ..] 连续（同 ARP/ICMP 段——不再搬移）。
    tx_send(d, regs, np::ETH_HDR_SZ + np::IPV4_HDR_SZ + udp_len);
    let deadline = now() + DNS_CAP_NS;
    loop {
        let f = match recv_frame(d, regs, deadline) {
            Some(f) => f,
            None => return DnsRes::Timeout,
        };
        if let Some(e) = np::parse_eth(f) {
            if e.ethertype == np::ET_IPV4 {
                if let Some(ip) = np::parse_ipv4(e.payload) {
                    if ip.src == DNS_IP && ip.dst == OUR_IP && ip.proto == np::IP_PROTO_UDP {
                        if let Some(ud) = np::parse_udp(ip.payload, ip.src, ip.dst) {
                            if ud.sport == DNS_PORT && ud.dport == DNS_SPORT {
                                if let Some(addr) = np::parse_dns_a_reply(ud.payload, id) {
                                    return DnsRes::Ok(addr);
                                }
                            }
                        }
                    }
                }
            }
        }
        // 非目标帧/解析失败 = 杂散（SLIRP IPv6 RA 等）——跳过重收
    }
}

/// 常驻服务循环（A-8b DNS + T-d TCP 三 op，vblk A-4 同款纪律）：env 块
/// gift[2] 端点收 berry-netdproto 请求——按首字节 op 分派：DNS_QUERY 查询 /
/// TCP OPEN·ROUNDTRIP·CLOSE（草案 §3.4 单会话三 op 折叠，在席再 OPEN →
/// ST_BUSY）→ 真网卡往返 → 应答。空队 = 合法空闲（wait READABLE 10s 帽
/// **超时重臂**——链断判定交发问方 roundtrip 超时，「常驻服务 wait 超时是
/// 心跳不是死线」）；client 错误一律 ST_* 应答**服务不死**；多带 handle
/// 逐个 close（EBUSY → ST 降级不死、其余非 0 → trap）；协议无随行面
/// （nh != 0 / nb == 0 / 未知 op = 坏请求，DNS 形 1B ST_BAD——旧垃圾消息
/// 行为零改动）。
fn serve(d: &mut Dma, regs: *mut u32, mac: &[u8; 6], gw_mac: &[u8; 6]) -> ! {
    let h = gift2_h();
    if h == 0 {
        // gift 缺席 = boot 预拉链断——park 挂起（fsmgr/vblk 同款：不 spin
        // 烧 CPU，标记缺失 = smoke FAIL 兜底）。
        berry_libc::park();
    }
    let mut msg = [0u8; 256];
    // 一只缓冲两面按 op 出长度：DNS 应答 5B 前缀 / TCP ROUNDTRIP 应答
    // [st][dlen][data≤128] = TCP_REP_MAX 130B。
    let mut rep = [0u8; ndp::TCP_REP_MAX];
    let mut hs = [0 as Handle; 4];
    // TCP 单会话独占（草案 §8）：None = 无会话；OPEN 建 / CLOSE 弃——
    // selftest 与本服务同一 Sess 驱动（「不长第二套」）。
    let mut sess: Option<tcp::Sess> = None;
    // ROUNDTRIP 回显暂存（服务端真实收到的字节——应答随行回传，对拍归 client）。
    let mut rt = [0u8; ndp::TCP_DATA_MAX];
    loop {
        let mut nb = msg.len();
        let mut nh = hs.len();
        let s = berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh);
        if s != 0 {
            if s == BERRY_ERR_AGAIN {
                let mut obs = 0u64;
                let _ = berry_object_wait_one(h, SIGNAL_READABLE, now() + SVC_IDLE_NS, &mut obs);
                continue;
            }
            trap(); // 非空队错码 = 链断（fsmgr/vblk 同口径）
        }
        let (mut st, mut addr) = (ndp::ST_BAD, [0u8; 4]);
        let mut dl = 0usize; // TCP 应答回传长度（rt 前缀；非 TCP 恒 0）
        let mut is_tcp = false;
        if nh == 0 && nb >= 1 {
            match msg[0] {
                ndp::OP_DNS_QUERY => match ndp::parse_request(&msg[..nb]) {
                    Some(ndp::Req::DnsQuery { name }) => match core::str::from_utf8(name) {
                        // encode_dns_query_a 收 &str——字节面桥接；DNS 名是
                        // ASCII 域，非 UTF-8 即坏请求（ST_BAD 不发包）。
                        Ok(nm) => match dns_roundtrip(d, regs, mac, gw_mac, nm) {
                            DnsRes::Ok(a) => {
                                st = ndp::ST_OK;
                                addr = a;
                            }
                            DnsRes::Bad => st = ndp::ST_BAD,
                            DnsRes::Timeout => st = ndp::ST_TIMEOUT,
                        },
                        Err(_) => st = ndp::ST_BAD,
                    },
                    None => st = ndp::ST_BAD,
                },
                ndp::OP_TCP_OPEN | ndp::OP_TCP_ROUNDTRIP | ndp::OP_TCP_CLOSE => {
                    is_tcp = true;
                    match ndp::parse_tcp_request(&msg[..nb]) {
                        Some(req) => {
                            let (s2, n2) = tcp_op(&mut sess, req, d, regs, mac, gw_mac, &mut rt);
                            st = s2;
                            dl = n2;
                        }
                        None => st = ndp::ST_BAD, // 尾随/越界 = 野值拒（协议面）
                    }
                }
                _ => st = ndp::ST_BAD, // 未知 op：DNS 形 BAD（旧垃圾消息行为）
            }
        }
        // 协议无随行面：多带 handle 全部回收（EBUSY = client 送了映射中
        // 唯一 handle——降级 ST_BAD 服务不死；其余错 = 链断 trap）。降级
        // **三清** st/addr/dl：编码器烘焙线「非 OK 恒全零 addr / 非 OK 恒
        // 不携 data」两侧都不许残留成功值（DNS 成功后撞 EBUSY 时 addr 不清
        // → encode_reply None → trap 服务死——防御面顺带修复）。
        for i in 0..nh {
            if hs[i] != 0 {
                let s = berry_handle_close(hs[i]);
                if s == BERRY_ERR_BUSY {
                    st = ndp::ST_BAD;
                    addr = [0u8; 4];
                    dl = 0;
                } else if s != 0 {
                    trap();
                }
            }
        }
        let rl = if is_tcp {
            match ndp::encode_tcp_reply(&mut rep, st, &rt[..dl]) {
                Some(n) => n,
                None => trap(), // st ∈ {OK,BAD,TIMEOUT,BUSY} + 非 OK 恒空 data 恒过；防御面
            }
        } else {
            match ndp::encode_reply(&mut rep, st, addr) {
                Some(n) => n,
                None => trap(), // st ∈ {OK,BAD,TIMEOUT} + 非 OK 恒零 addr 恒过；防御面
            }
        };
        let mut obs = 0u64;
        loop {
            let s = berry_channel_send(h, rep.as_ptr(), rl, core::ptr::null(), 0);
            if s == 0 {
                break;
            }
            if s != BERRY_ERR_BUSY {
                trap();
            }
            let _ = berry_object_wait_one(h, SIGNAL_WRITABLE, now() + SVC_IDLE_NS, &mut obs);
        }
    }
}

/// TCP 三 op 服务路径（T-d，草案 §3.4 单会话三 op 折叠）：一次 RPC 走
/// 一段会话——OPEN 建会话 / ROUNDTRIP 会话上发收 / CLOSE 挥手弃置；
/// 单会话独占（在席再 OPEN → ST_BUSY）。返回 (st, data_len)——data 在
/// rt 前缀（服务端真实收到的回显字节，对拍归 client）。
#[allow(clippy::too_many_arguments)]
fn tcp_op(
    sess: &mut Option<tcp::Sess>,
    req: ndp::TcpReq<'_>,
    d: &mut Dma,
    regs: *mut u32,
    mac: &[u8; 6],
    gw_mac: &[u8; 6],
    rt: &mut [u8; ndp::TCP_DATA_MAX],
) -> (u8, usize) {
    match req {
        ndp::TcpReq::Open { port } => {
            if sess.is_some() {
                return (ndp::ST_BUSY, 0); // 单会话独占（草案 §8 文档化语义）
            }
            let sport = TCP_SPORT_NEXT.fetch_add(1, Ordering::Relaxed);
            match tcp::Sess::connect(d, regs, mac, gw_mac, sport, port, now() + TCPS_OPEN_CAP_NS) {
                Some(s) => {
                    *sess = Some(s);
                    (ndp::ST_OK, 0)
                }
                None => (ndp::ST_TIMEOUT, 0), // 握手帽到/RST/重传到帽——会话不成立无残留
            }
        }
        ndp::TcpReq::Roundtrip { data } => {
            let s2 = match sess.as_mut() {
                Some(s) => s,
                None => return (ndp::ST_BAD, 0), // 无会话先发 ROUNDTRIP = 协议面错
            };
            match s2.roundtrip(d, regs, mac, gw_mac, data, rt, now() + TCPS_RT_CAP_NS) {
                tcp::RtRes::Ok(n) => (ndp::ST_OK, n),
                // Timeout 会话保命——但 in-flight 占位未清：后续 ROUNDTRIP
                // 恒 ST_BAD（berry-tcp send 门控 inflight=Some 即拒；迟到
                // ACK 躺 rx 环无人收割——serve 循环 op 之外不驱动会话），
                // 唯 CLOSE 可收口（shutdown 的 FIN 覆盖单槽 + step 真收帧）。
                tcp::RtRes::Timeout => (ndp::ST_TIMEOUT, 0),
                tcp::RtRes::Bad => (ndp::ST_BAD, 0),
            }
        }
        ndp::TcpReq::Close => {
            let mut s2 = match sess.take() {
                Some(s) => s,
                None => return (ndp::ST_BAD, 0),
            };
            // take() = 无论成败会话弃置（内存态，无设备面残留——Sess 无 Drop
            // 面，conn 纯内存几何；对端超时自收）。
            if s2.shutdown(d, regs, mac, gw_mac, now() + TCPS_CLOSE_CAP_NS) {
                (ndp::ST_OK, 0)
            } else {
                (ndp::ST_TIMEOUT, 0) // 挥手帽到——本地已弃置
            }
        }
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （vprobe/vblk 同款——链断 loud，`NET ok` 缺失 = smoke FAIL 兜底）。
#[cold]
fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（0x4000_0000 与用户布局无交集——家族惯例值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}
