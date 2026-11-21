//! devmgr lib —— 设备枚举/驱动匹配/服务状态报告纯面（bin 的可测核）。
//!
//! devmgr v1（驱动子系统研究报告 §4.2 交付 1.1 的最小先行版）：devmgr
//! v2 五件事（DTB 枚举 → 资源池 → 匹配 → 授权 → /svc 注册）里先落
//! 「MMIO 槽枚举 + 驱动匹配 + 服务状态报告」纯观察半边——**不授权、
//! 不拉起**（svcmgr 点名拉起链不动，v2 才做自动拉起）。
//!
//! 分层（Linux `bus_type` 形状的极小化身——枚举器 → 统一设备记录 →
//! 匹配表，研究报告 §3.4 借形状清单）：
//! - 枚举器 = [`SlotReader`] trait（真机臂 MMIO volatile 读在 bin 侧实现，
//!   测试 mock 数组实现——零 IO 依赖进本 lib）；
//! - 统一设备记录 = [`DeviceFound`]（slot/did/ven/ver 四域）；
//! - 匹配表 = [`DRIVER_TABLE`]（设备 ID → 驱动绑定，单源）。
//!
//! 服务状态 = svcmgr 拉起账本（berry-psproto PS_LIST 应答帧）的视图
//! [`SvcLedger`]——v1 无 `/svc` 命名空间（v2 设计位），现役等价物 = 账本
//! 行名 × [`STATE_ALIVE`]。账本缺席（无查询通道）= 状态 unknown 降级，
//! 枚举报告仍完整输出（查询面不致命）。
//!
//! 几何常量本地镜像 + 与 berry-exec 编译期互证（bin 侧 `const` 断言——
//! vprobe 四常量同款惯例；本 lib 不依赖 berry-exec 保持零 syscall 面）：
//! [`MMIO_SLOTS`] = `berry_exec::VIRTIO_MMIO_SLOTS`、[`MMIO_STRIDE`] =
//! `VIRTIO_MMIO_STRIDE`（槽头寄存器只读前 0x10 四字，stride 0x200 界内）。
#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use berry_psproto::{STATE_ALIVE, STATE_EXITED, STATE_SPAWNFAIL};

// ---- virtio-mmio 槽几何与规范常量（vprobe 四常量同款本地镜像） ----

/// virtio-mmio 规范签名（"virt" LE）——空槽也在席应答（32 槽全验 = 窗口
/// 几何的总检查）。
pub const VIRTIO_MAGIC: u32 = 0x7472_6976;
/// virtio 1.x 版本字（现代版——占用槽预期值；legacy 报 WARN 不拒）。
pub const VIRTIO_VERSION_MODERN: u32 = 2;
/// MMIO 槽总数（berry-virtio mmio 槽布局 32 槽 × 0x200）。
pub const MMIO_SLOTS: usize = 32;
/// 槽距（berry-exec `VIRTIO_MMIO_STRIDE` 钉板互证——bin 侧编译期断言）。
pub const MMIO_STRIDE: usize = 0x200;

/// 槽头四字（LE u32：偏移 0x000/0x004/0x008/0x00C 全 RO 无副作用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotRegs {
    pub magic: u32,
    pub version: u32,
    pub device_id: u32,
    pub vendor_id: u32,
}

/// 空槽的规范应答（magic 在席、无设备占用）。
pub const SLOT_EMPTY: SlotRegs = SlotRegs {
    magic: VIRTIO_MAGIC,
    version: 0,
    device_id: 0,
    vendor_id: 0,
};

/// 枚举器抽象：按槽号读槽头四字（真机 = MMIO volatile；测试 = mock 数组）。
pub trait SlotReader {
    fn read_slot(&mut self, slot: usize) -> SlotRegs;
}

/// 发现的在席设备（did != 0 的占用槽；did=0 = 空槽不入表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceFound {
    pub slot: usize,
    pub device_id: u32,
    pub vendor_id: u32,
    pub version: u32,
}

/// 单槽非致命异常（记入报告 WARN，不断枚举链——管理器全景优先）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotAnomaly {
    /// 占用槽 version != 2（QEMU 默认 force-legacy=true 报 1——启动参数
    /// 缺 `-global virtio-mmio.force-legacy=false`；vprobe 是 trap 面，
    /// devmgr 是观察者报 WARN）。
    LegacyVersion { slot: usize, version: u32 },
}

/// 全窗几何错（读到非规范签名 = 窗口不在席/错位——vprobe trap 的 Err 化，
/// main 侧打诊断行 exit 1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadMagic {
    pub slot: usize,
    pub magic: u32,
}

/// 枚举产出：设备清单（槽号升序）+ 单槽异常表。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DeviceInventory {
    pub devices: Vec<DeviceFound>,
    pub anomalies: Vec<SlotAnomaly>,
}

/// 扫 [`MMIO_SLOTS`] 全槽：magic 非 [`VIRTIO_MAGIC`] → [`BadMagic`]（整窗
/// 失败）；占用槽（did != 0）version != 2 记 [`SlotAnomaly::LegacyVersion`]
/// 后照入设备表。
pub fn enumerate(reader: &mut dyn SlotReader) -> Result<DeviceInventory, BadMagic> {
    let mut inv = DeviceInventory::default();
    for slot in 0..MMIO_SLOTS {
        let regs = reader.read_slot(slot);
        if regs.magic != VIRTIO_MAGIC {
            return Err(BadMagic { slot, magic: regs.magic });
        }
        if regs.device_id == 0 {
            continue; // 空槽（transport 在席无设备——QEMU 未挂 -device 的槽）
        }
        if regs.version != VIRTIO_VERSION_MODERN {
            inv.anomalies.push(SlotAnomaly::LegacyVersion { slot, version: regs.version });
        }
        inv.devices.push(DeviceFound {
            slot,
            device_id: regs.device_id,
            vendor_id: regs.vendor_id,
            version: regs.version,
        });
    }
    Ok(inv)
}

// ---- 驱动匹配表（设备 ID → 驱动绑定；v1 静态表 = v2 `hw.match` 键前身） ----

/// 一条驱动绑定：设备类 → 驱动服务名 × 账本行名。
///
/// `driver` = rootfs bin 名/服务名；`ledger_name` = svcmgr ps 账本行名
/// （8B 帽内短名——gui-inputd 注册名是 `inputd`，svcmgr `ps_reg(b"inputd")`
/// 实录）。两名字段分立：v2 `/svc` 注册面落成后 `driver` 升命名空间路径。
pub struct DriverBinding {
    /// virtio DeviceID（2=block / 1=net / 18=input / 16=gpu / 3=console /
    /// 4=rng）。
    pub device_id: u32,
    /// 设备类短名（报告用："blk"/"net"/"input"/"gpu"/"console"/"rng"）。
    pub kind: &'static str,
    /// 驱动服务名（报告与缺失告警用）。
    pub driver: &'static str,
    /// svcmgr ps 账本行名（服务在席性判定的查表键；psproto NAME_MAX=8
    /// 帽内短名——超帽驱动名取缩写：gui-inputd→`inputd` 实录、
    /// berry-rng→`rngd` 同款）。
    pub ledger_name: &'static str,
}

/// v1 匹配表（研究报告 §4.2 四类：blk=2→vblk、net=1→vnet、input=18→
/// gui-inputd、gpu=16→guifbd）+ 驱动表补全刀两行（console=3、rng=4——
/// 先行绑定：服务本体未落（rng = 报告交付 1.5 未动面），设备在而账本
/// 无行时 devmgr 报 absent WARN，正是「缺谁」的正确观察面）。did 数值
/// 与 vprobe 白名单 / berry-virtio `mmio::DEVICE_ID_GPU` 等值钉板互证
/// （console/rng 暂无 vprobe 白名——挂即 vprobe trap，smoke 拓扑面归
/// 各自服务落地刀）。
pub const DRIVER_TABLE: [DriverBinding; 6] = [
    DriverBinding { device_id: 2, kind: "blk", driver: "vblk", ledger_name: "vblk" },
    DriverBinding { device_id: 1, kind: "net", driver: "vnet", ledger_name: "vnet" },
    DriverBinding { device_id: 18, kind: "input", driver: "gui-inputd", ledger_name: "inputd" },
    DriverBinding { device_id: 16, kind: "gpu", driver: "guifbd", ledger_name: "guifbd" },
    DriverBinding { device_id: 3, kind: "console", driver: "console", ledger_name: "console" },
    DriverBinding { device_id: 4, kind: "rng", driver: "berry-rng", ledger_name: "rngd" },
];

/// 按设备 ID 查绑定（未知 did = None → 报告 WARN，管理器不死）。
pub fn driver_for(device_id: u32) -> Option<&'static DriverBinding> {
    DRIVER_TABLE.iter().find(|b| b.device_id == device_id)
}

// ---- 服务状态：svcmgr 拉起账本视图 ----

/// 账本行（name + psproto 行状态码；code 仅 EXITED 有意义，报告用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerRow {
    pub name: Vec<u8>,
    pub state: u8,
    pub code: i32,
}

/// svcmgr 拉起账本视图（PS_LIST 应答帧的解码产物；bin 侧 RPC 收集，测试
/// 直接构造）。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SvcLedger {
    pub rows: Vec<LedgerRow>,
}

impl SvcLedger {
    pub fn from_rows(rows: Vec<LedgerRow>) -> Self {
        Self { rows }
    }

    /// 按账本行名查状态码（None = 无行 = 服务从未被拉起）。
    pub fn lookup(&self, ledger_name: &str) -> Option<u8> {
        self.rows.iter().find(|r| r.name == ledger_name.as_bytes()).map(|r| r.state)
    }

    /// 服务在席判定（行在且 ALIVE）。
    pub fn is_alive(&self, ledger_name: &str) -> bool {
        self.lookup(ledger_name) == Some(STATE_ALIVE)
    }
}

/// 每设备的服务状态标记（报告渲染三态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvcMark {
    /// 账本行在且 ALIVE。
    Up,
    /// 账本在场但行缺/非 ALIVE（设备在而服务不在——任务书缺失告警面）。
    Down { state: Option<u8> },
    /// 账本缺席（无 svcmgr 查询通道——状态未知，不算缺失告警）。
    Unknown,
}

fn mark_for(binding: &DriverBinding, ledger: Option<&SvcLedger>) -> SvcMark {
    match ledger {
        Some(l) => {
            if l.is_alive(binding.ledger_name) {
                SvcMark::Up
            } else {
                SvcMark::Down { state: l.lookup(binding.ledger_name) }
            }
        }
        None => SvcMark::Unknown,
    }
}

/// 渲染完整报告（任务书枚举报告 + 缺失告警形态）：
///
/// ```text
/// devmgr: 3 devices: [slot 4] blk → vblk ✓, [slot 7] net → vnet ✓, [slot 30] input → gui-inputd ✗
/// devmgr: WARN: [slot 30] input driver gui-inputd not registered (state: absent)
/// ```
///
/// 行序：账本缺席说明行（仅 Unknown 态）→ 设备清单头行 → WARN 行（服务
/// 缺失 / 未知设备 ID / legacy 槽）。零设备 = `devmgr: 0 devices` 单行。
pub fn render(out: &mut String, inv: &DeviceInventory, ledger: Option<&SvcLedger>) {
    if ledger.is_none() {
        out.push_str("devmgr: svc ledger unavailable (no svcmgr query channel; service states unknown)\n");
    }
    if inv.devices.is_empty() {
        // 零设备裸行（无冒号列表）——anomaly 只发生在占用槽，零设备必零告警。
        out.push_str("devmgr: 0 devices\n");
        return;
    }
    let _ = write!(out, "devmgr: {} device{}:", inv.devices.len(), if inv.devices.len() == 1 { "" } else { "s" });
    for (i, dev) in inv.devices.iter().enumerate() {
        if i > 0 {
            out.push_str(",");
        }
        match driver_for(dev.device_id) {
            Some(b) => {
                let mark = match mark_for(b, ledger) {
                    SvcMark::Up => "✓",
                    SvcMark::Down { .. } => "✗",
                    SvcMark::Unknown => "?",
                };
                let _ = write!(out, " [slot {}] {} → {} {}", dev.slot, b.kind, b.driver, mark);
            }
            None => {
                let _ = write!(out, " [slot {}] did={} → ?", dev.slot, dev.device_id);
            }
        }
    }
    out.push('\n');
    for dev in &inv.devices {
        match driver_for(dev.device_id) {
            Some(b) => {
                if let SvcMark::Down { state } = mark_for(b, ledger) {
                    let why = match state {
                        Some(STATE_EXITED) => "exited",
                        Some(STATE_SPAWNFAIL) => "spawnfail",
                        Some(_) => "not alive",
                        None => "absent",
                    };
                    let _ = writeln!(
                        out,
                        "devmgr: WARN: [slot {}] {} driver {} not registered (state: {})",
                        dev.slot, b.kind, b.driver, why
                    );
                }
            }
            None => {
                let _ = writeln!(
                    out,
                    "devmgr: WARN: [slot {}] device id {} has no driver binding",
                    dev.slot, dev.device_id
                );
            }
        }
    }
    for a in &inv.anomalies {
        match *a {
            SlotAnomaly::LegacyVersion { slot, version } => {
                let _ = writeln!(
                    out,
                    "devmgr: WARN: [slot {}] virtio legacy version={} (modern {} expected; missing force-legacy=false?)",
                    slot, version, VIRTIO_VERSION_MODERN
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// mock 枚举器：32 槽数组（缺省 [`SLOT_EMPTY`]——magic 在席空槽）。
    struct MockBus {
        slots: [SlotRegs; MMIO_SLOTS],
    }

    impl MockBus {
        fn new() -> Self {
            Self { slots: [SLOT_EMPTY; MMIO_SLOTS] }
        }

        fn plug(&mut self, slot: usize, device_id: u32) -> &mut Self {
            self.slots[slot] = SlotRegs {
                magic: VIRTIO_MAGIC,
                version: VIRTIO_VERSION_MODERN,
                device_id,
                vendor_id: 0x554D_4551, // "QEMU"（仿真器特有——只报告不断言）
            };
            self
        }
    }

    impl SlotReader for MockBus {
        fn read_slot(&mut self, slot: usize) -> SlotRegs {
            self.slots[slot]
        }
    }

    fn report(inv: &DeviceInventory, ledger: Option<&SvcLedger>) -> String {
        let mut s = String::new();
        render(&mut s, inv, ledger);
        s
    }

    fn ledger_of(pairs: &[(&str, u8)]) -> SvcLedger {
        SvcLedger::from_rows(
            pairs
                .iter()
                .map(|(n, st)| LedgerRow { name: n.as_bytes().to_vec(), state: *st, code: 0 })
                .collect(),
        )
    }

    // 1. 空总线：32 槽全空 → 零设备单行（unavailable 说明行随无账本态）。
    #[test]
    fn empty_bus_reports_zero_devices() {
        let mut bus = MockBus::new();
        let inv = enumerate(&mut bus).unwrap();
        assert!(inv.devices.is_empty());
        assert!(inv.anomalies.is_empty());
        assert_eq!(
            report(&inv, None),
            "devmgr: svc ledger unavailable (no svcmgr query channel; service states unknown)\n\
             devmgr: 0 devices\n"
        );
    }

    // 2. 混合总线：blk/net/input 三设备 → 清单头行 + 槽号/类/驱动逐项。
    #[test]
    fn mixed_bus_reports_device_list() {
        let mut bus = MockBus::new();
        bus.plug(4, 2).plug(7, 1).plug(30, 18);
        let inv = enumerate(&mut bus).unwrap();
        assert_eq!(inv.devices.len(), 3);
        assert_eq!(inv.devices[0].slot, 4);
        assert_eq!(inv.devices[2].device_id, 18);
        let s = report(&inv, None);
        assert_eq!(
            s,
            "devmgr: svc ledger unavailable (no svcmgr query channel; service states unknown)\n\
             devmgr: 3 devices: [slot 4] blk → vblk ?, [slot 7] net → vnet ?, [slot 30] input → gui-inputd ?\n"
        );
    }

    // 3. 驱动匹配表：六设备类全命中，名字域（driver 与 ledger_name）钉板；
    //    gui-inputd 的账本行名是 inputd（svcmgr ps_reg 实录）；console/rng
    //    是先行绑定（服务未落——rngd 行名按 gui-inputd→inputd 同款缩）。
    #[test]
    fn driver_table_bindings() {
        let blk = driver_for(2).unwrap();
        assert_eq!((blk.kind, blk.driver, blk.ledger_name), ("blk", "vblk", "vblk"));
        let net = driver_for(1).unwrap();
        assert_eq!((net.kind, net.driver, net.ledger_name), ("net", "vnet", "vnet"));
        let input = driver_for(18).unwrap();
        assert_eq!((input.kind, input.driver, input.ledger_name), ("input", "gui-inputd", "inputd"));
        let gpu = driver_for(16).unwrap();
        assert_eq!((gpu.kind, gpu.driver, gpu.ledger_name), ("gpu", "guifbd", "guifbd"));
        let console = driver_for(3).unwrap();
        assert_eq!((console.kind, console.driver, console.ledger_name), ("console", "console", "console"));
        let rng = driver_for(4).unwrap();
        assert_eq!((rng.kind, rng.driver, rng.ledger_name), ("rng", "berry-rng", "rngd"));
        assert_eq!(DRIVER_TABLE.len(), 6);
        assert!(driver_for(9).is_none());
        assert!(driver_for(0).is_none());
    }

    // 4. 坏 magic：窗口几何错 → 整枚举 Err（槽号与魔数随行）。
    #[test]
    fn bad_magic_fails_whole_enumeration() {
        let mut bus = MockBus::new();
        bus.slots[5].magic = 0xdead_beef;
        match enumerate(&mut bus) {
            Err(BadMagic { slot, magic }) => assert_eq!((slot, magic), (5, 0xdead_beef)),
            other => panic!("expected Err, got {:?}", other),
        }
    }

    // 5. legacy 槽：占用但 version=1 → 设备照记 + WARN 行（观察者不死）。
    #[test]
    fn legacy_slot_reports_anomaly_but_stays_in_inventory() {
        let mut bus = MockBus::new();
        bus.plug(2, 2);
        bus.slots[2].version = 1;
        let inv = enumerate(&mut bus).unwrap();
        assert_eq!(inv.devices.len(), 1);
        assert_eq!(
            inv.anomalies,
            vec![SlotAnomaly::LegacyVersion { slot: 2, version: 1 }]
        );
        let s = report(&inv, None);
        assert!(s.contains("[slot 2] blk → vblk ?"));
        assert!(s.contains("devmgr: WARN: [slot 2] virtio legacy version=1"));
    }

    // 6. 服务状态三态：ALIVE ✓ / 缺行 ✗+WARN(absent) / EXITED ✗+WARN(exited)。
    #[test]
    fn ledger_states_drive_marks_and_warnings() {
        let mut bus = MockBus::new();
        bus.plug(4, 2).plug(7, 1);
        let inv = enumerate(&mut bus).unwrap();

        let led = ledger_of(&[("vblk", STATE_ALIVE), ("vnet", STATE_EXITED)]);
        let s = report(&inv, Some(&led));
        assert!(s.contains("[slot 4] blk → vblk ✓"));
        assert!(s.contains("[slot 7] net → vnet ✗"));
        assert!(s.contains("devmgr: WARN: [slot 7] net driver vnet not registered (state: exited)"));
        assert!(!s.contains("[slot 4] blk → vblk ✗"));

        // 缺行 = absent（设备在而服务从未被拉起——任务书缺失告警主面）。
        let led = ledger_of(&[("vblk", STATE_ALIVE)]);
        let s = report(&inv, Some(&led));
        assert!(s.contains("devmgr: WARN: [slot 7] net driver vnet not registered (state: absent)"));

        // spawnfail 同面。
        let led = ledger_of(&[("vblk", STATE_ALIVE), ("vnet", STATE_SPAWNFAIL)]);
        let s = report(&inv, Some(&led));
        assert!(s.contains("devmgr: WARN: [slot 7] net driver vnet not registered (state: spawnfail)"));
    }

    // 7. gui-inputd 账本名：inputd 行 ALIVE → gui-inputd ✓（两名字段分立
    //    的意义——账本查 inputd，报告打 gui-inputd）。
    #[test]
    fn input_ledger_name_is_inputd() {
        let mut bus = MockBus::new();
        bus.plug(30, 18);
        let inv = enumerate(&mut bus).unwrap();
        let led = ledger_of(&[("inputd", STATE_ALIVE)]);
        let s = report(&inv, Some(&led));
        assert!(s.contains("[slot 30] input → gui-inputd ✓"));
        assert!(!s.contains("WARN"));
    }

    // 8. 未知 did：did=9（virtio-9p——本仓无消费者的真未知类；4=rng 已入
    //    驱动表补全刀先行绑定，不再当未知样例）→ 清单 did=? + WARN 无绑定
    //    （管理器全景不死）。
    #[test]
    fn unknown_device_id_warns_without_binding() {
        let mut bus = MockBus::new();
        bus.plug(12, 9);
        let inv = enumerate(&mut bus).unwrap();
        let s = report(&inv, Some(&SvcLedger::default()));
        assert!(s.contains("[slot 12] did=9 → ?"));
        assert!(s.contains("devmgr: WARN: [slot 12] device id 9 has no driver binding"));
    }

    // 8b. 先行绑定渲染：rng 设备在而服务未落（交付 1.5 未动面）→ ✗ +
    //     WARN absent——「设备在而服务缺」的 loud 观察面（表补全的意义：
    //     did 有主，缺的是服务本体）。
    #[test]
    fn rng_forward_binding_renders_absent_warn() {
        let mut bus = MockBus::new();
        bus.plug(9, 4);
        let inv = enumerate(&mut bus).unwrap();
        let s = report(&inv, Some(&SvcLedger::default()));
        assert!(s.contains("[slot 9] rng → berry-rng ✗"));
        assert!(s.contains(
            "devmgr: WARN: [slot 9] rng driver berry-rng not registered (state: absent)"
        ));
    }

    // 9. 单设备语法（device 不带复数 s——烟测对拍面友好）。
    #[test]
    fn single_device_grammar() {
        let mut bus = MockBus::new();
        bus.plug(4, 2);
        let inv = enumerate(&mut bus).unwrap();
        let s = report(&inv, None);
        assert!(s.contains("\ndevmgr: 1 device: [slot 4] blk → vblk ?\n"));
    }

    // 10. 账本视图查表：无行 None、行在按码返回。
    #[test]
    fn ledger_lookup_semantics() {
        let led = ledger_of(&[("vblk", STATE_ALIVE), ("vnet", STATE_EXITED)]);
        assert_eq!(led.lookup("vblk"), Some(STATE_ALIVE));
        assert_eq!(led.lookup("vnet"), Some(STATE_EXITED));
        assert_eq!(led.lookup("inputd"), None);
        assert!(led.is_alive("vblk"));
        assert!(!led.is_alive("vnet"));
        assert!(!led.is_alive("inputd"));
    }
}
