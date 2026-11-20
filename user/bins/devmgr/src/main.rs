//! devmgr —— 用户态设备管理服务 v1（波 2 核心交付：枚举 + 匹配 + 报告）。
//!
//! 驱动子系统研究报告 §4.2 交付 1.1（devmgr 最小面）的 v1 先行版：跳过
//! DTB 枚举（v2），先落 MMIO 槽枚举 + 驱动匹配 + 服务状态报告链。逻辑核
//! 在 lib（host `cargo test --lib` 可测），本文件只做 guest 接线：
//!
//! - **枚举**：扫 32 槽 virtio-mmio 窗（[`berry_exec::VIRTIO_MMIO_VA`] 单源）
//!   读 MagicValue(0x74726976)/DeviceID——发现所有在席 virtio 设备。窗契约
//!   = vprobe 同款：拉起者经 `SpawnSpec::mmio_vmo` 授窗（boot 手铸 → v2
//!   devmgr 铸造），**本进程不自造物理窗**（EL0 铁律）；无窗拉起 = 读窗
//!   D-abort 只死自己。
//! - **服务状态**：查 svcmgr 拉起账本（berry-psproto PS_LIST 严格串行
//!   RPC，ps bin 同拓扑）——通道契约 = ps 同款：env 块 gift[2]
//!   （[`gift2_h`]）交付 svcmgr 预建端点。v1 无 `/svc` 命名空间（v2
//!   设计位），账本行名 × ALIVE = 现役等价物。通道缺席/查询失败 = 状态
//!   降级 unknown（诊断行 + 枚举报告仍完整输出——查询面不致命）。
//! - **报告**：启动时打完整设备清单 + 缺失告警（设备在而服务不在 =
//!   WARN），格式见 lib [`devmgr::render`]。**不拉起任何服务**——svcmgr
//!   点名拉起链不动（v2 才做自动拉起）。
//!
//! 退出码：窗几何错（bad magic）= 1；其余（含 WARN——报告性输出）= 0。
//! main 返回 = libc exit(0)（vprobe/ps 同链）。

use std::process::exit;

use berry_libc::io::gift2_h;
use berry_psproto::{encode_request, parse_reply, REPLY_MAX};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_object_wait_one, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};

use devmgr::{render, BadMagic, DeviceInventory, LedgerRow, SlotReader, SlotRegs, SvcLedger};

// 链接种子（stdhello/ps 同款）：rlib/LTO 按需保留——无引用则 berry-libc
// 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

// 几何互证（lib 本地镜像 ↔ berry-exec 单源——vprobe 四常量同款惯例的
// 编译期执法）：漂移即编不过，杜绝两套几何。
const _: () = {
    assert!(devmgr::MMIO_SLOTS == berry_exec::VIRTIO_MMIO_SLOTS, "slot count drift");
    assert!(devmgr::MMIO_STRIDE == berry_exec::VIRTIO_MMIO_STRIDE, "stride drift");
};

/// RPC 全帽（发 + 收各一）：30s——svcmgr 单线程顺序走到查询面才服务本
/// 请求（慢而活着不误杀，ps `RPC_CAP_NS` 同档；真挂死交帽尽 loud）。
const RPC_CAP_NS: u64 = 30 * 1_000_000_000;

/// 真机枚举器：MMIO 窗 volatile 读（vprobe 同款——设备访问全在用户态，
/// 内核只给了窗口）。
struct MmioSlots {
    base: *const u32,
}

impl MmioSlots {
    fn new() -> Self {
        Self { base: berry_exec::VIRTIO_MMIO_VA as *const u32 }
    }
}

impl SlotReader for MmioSlots {
    // SAFETY: 窗经 spawn 映射（Device-nGnRnE + SH_NONE——设备访问不可
    // 缓存不可推测）；逐字 read_volatile（MMIO 读不可合并/重排）。u32
    // 对齐（VA 页对齐、槽内偏移 4 的倍数）。只读 ID 头四字（0x000–0x00F
    // 全 RO、无副作用——不碰队列/中断类寄存器）。
    fn read_slot(&mut self, slot: usize) -> SlotRegs {
        unsafe {
            let regs = self.base.add(slot * (berry_exec::VIRTIO_MMIO_STRIDE / 4));
            SlotRegs {
                magic: core::ptr::read_volatile(regs),
                version: core::ptr::read_volatile(regs.add(1)),
                device_id: core::ptr::read_volatile(regs.add(2)),
                vendor_id: core::ptr::read_volatile(regs.add(3)),
            }
        }
    }
}

/// now = clock_read（失败按 0 → deadline 0 = 立即超时，活证死路 loud）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 查 svcmgr 拉起账本：gift[2] 无通道 → None（静默——bsh 手跑常态，非
/// 错误）；有通道走 PS_LIST 严格串行 RPC（ps 同款收发），失败 → 诊断行
/// + None（降级不死——枚举报告优先）。
fn query_ledger() -> Option<SvcLedger> {
    let h: Handle = gift2_h();
    if h == 0 {
        return None; // 无查询端可用（拉起者未交付——v1 常态）
    }
    // ① 发请求（1B 恰长；满 -EBUSY/空不足 -EAGAIN → 等 WRITABLE 重试）。
    let mut req = [0u8; 1];
    let rn = encode_request(&mut req); // PS_LIST（ps bin 同款简写形）
    loop {
        let s = berry_channel_send(h, req.as_ptr(), rn, core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_AGAIN && s != BERRY_ERR_BUSY {
            eprintln!("devmgr: ledger send err {}", s);
            return None;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            eprintln!("devmgr: ledger send timeout");
            return None;
        }
    }
    // ② 收应答（单帧 ≤ REPLY_MAX；-EAGAIN → 等 READABLE；随行 handle 不
    // 在协议内——收下即弃）。
    let mut rep = [0u8; REPLY_MAX];
    let nb;
    loop {
        let mut n = rep.len();
        let mut hh = [0 as Handle; 6];
        let mut nh = hh.len();
        let s = berry_channel_recv(h, rep.as_mut_ptr(), &mut n, hh.as_mut_ptr(), &mut nh);
        if s == 0 {
            for i in 0..nh {
                // 多带 handle：丢弃勿用（ps 同款纪律）。
                berry_handle_close(hh[i]);
            }
            nb = n;
            break;
        }
        if s != BERRY_ERR_AGAIN {
            eprintln!("devmgr: ledger recv err {}", s);
            return None;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            eprintln!("devmgr: ledger reply timeout");
            return None;
        }
    }
    // ③ 解析（畸形帧整体拒——半张表不进报告面，psproto 执法）。
    let mut rows = Vec::new();
    if parse_reply(&rep, nb, |name, st, code| {
        rows.push(LedgerRow { name: name.to_vec(), state: st, code })
    })
    .is_none()
    {
        eprintln!("devmgr: bad ledger reply");
        return None;
    }
    Some(SvcLedger::from_rows(rows))
}

fn main() {
    // ① 枚举（窗几何错 = 诊断 + exit 1；单槽异常进报告 WARN 不在这里拦）。
    let inv: DeviceInventory = match devmgr::enumerate(&mut MmioSlots::new()) {
        Ok(inv) => inv,
        Err(BadMagic { slot, magic }) => {
            eprintln!("devmgr: slot {} bad magic={:#x} (window geometry broken?)", slot, magic);
            exit(1);
        }
    };
    // ② 服务状态（psproto 账本查询；无通道/失败 = unknown 降级）。
    let ledger = query_ledger();
    // ③ 报告（清单头行 + WARN 行——lib 单源渲染，host 测试盖同面）。
    let mut out = String::new();
    render(&mut out, &inv, ledger.as_ref());
    print!("{}", out);
    exit(0);
}
