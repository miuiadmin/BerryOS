//! host 测试：统一 Driver trait 框架三面——mock Driver 的 probe_all 成功/
//! 失败混合（失败不炸继续走）、生命周期 start→stop→start 循环、
//! virtio_example 在假 MMIO 面板上的全套（probe 判据 + STATUS 寄存器
//! 状态机序列）。

use berry_driver::registry::{DriverRegistry, ProbeResult};
use berry_driver::virtio_example::VirtioMmioDriver;
use berry_driver::{Driver, DriverError, ProbeInfo};
use berry_virtio::mmio;
use berry_virtio::{ST_ACKNOWLEDGE, ST_DRIVER, ST_DRIVER_OK, ST_FEATURES_OK};

// ---------- mock 驱动与挂具 ----------

/// mock 驱动：irq 参数即假设备 ID（mock 不碰 MMIO 内存——probe 语义纯参数
/// 面，真 MMIO 语义归 virtio_example 测试组）；null 窗走 MmioFault。
struct MockDriver {
    name: &'static str,
    accepts: u32,
    probed: bool,
}

impl MockDriver {
    fn new(name: &'static str, accepts: u32) -> Self {
        Self { name, accepts, probed: false }
    }
}

impl Driver for MockDriver {
    fn name(&self) -> &'static str {
        self.name
    }

    fn probe(&mut self, mmio_base: *mut u8, irq: u32) -> Result<ProbeInfo, DriverError> {
        if mmio_base.is_null() {
            return Err(DriverError::MmioFault);
        }
        if irq != self.accepts {
            return Err(DriverError::UnsupportedDevice { device_id: irq });
        }
        self.probed = true;
        Ok(ProbeInfo { device_id: irq, vendor_id: 0x1af4, version: 2 })
    }

    fn start(&mut self) -> Result<(), DriverError> {
        if !self.probed {
            return Err(DriverError::NotReady);
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), DriverError> {
        if !self.probed {
            return Err(DriverError::NotReady);
        }
        Ok(())
    }
}

/// 静态生命周期驱动实例（registry 的持有形态——真机 = 全局泄漏实例同形）。
fn leak<T>(v: T) -> &'static mut T {
    Box::leak(Box::new(v))
}

// ---------- probe_all：成功/失败混合 ----------

#[test]
fn probe_all_mixed_success_and_failure() {
    let mut reg = DriverRegistry::new();
    reg.register(leak(MockDriver::new("mock-blk", 2)));
    reg.register(leak(MockDriver::new("mock-net", 1)));

    // 两槽：槽 0 挂设备 2（blk 家认领）、槽 1 挂设备 1（net 家认领）。
    // base 是假非 null 指针——mock 不 deref，仅过空窗门。
    let slots: [(*mut u8, u32); 2] = [(0x1000 as *mut u8, 2), (0x2000 as *mut u8, 1)];
    let rs = reg.probe_all(&slots);

    // 全组合全记录：2 槽 × 2 驱动 = 4 条（失败也在场，不炸不断）。
    assert_eq!(rs.len(), 4);
    assert_eq!(rs.iter().filter(|r| r.is_ok()).count(), 2);

    let ok_info = ProbeInfo { device_id: 2, vendor_id: 0x1af4, version: 2 };
    assert_eq!(
        rs[0],
        ProbeResult { driver: "mock-blk", slot: 0, outcome: Ok(ok_info) }
    );
    // 同槽竞争：net 家对槽 0 拒（设备 2 不是它的），错误值带读数。
    assert_eq!(
        rs[1],
        ProbeResult {
            driver: "mock-net",
            slot: 0,
            outcome: Err(DriverError::UnsupportedDevice { device_id: 2 }),
        }
    );
    // 槽 1：blk 家拒、net 家认领——前一轮失败不影响后续组合。
    assert_eq!(
        rs[2],
        ProbeResult {
            driver: "mock-blk",
            slot: 1,
            outcome: Err(DriverError::UnsupportedDevice { device_id: 1 }),
        }
    );
    assert_eq!(
        rs[3],
        ProbeResult {
            driver: "mock-net",
            slot: 1,
            outcome: Ok(ProbeInfo { device_id: 1, vendor_id: 0x1af4, version: 2 }),
        }
    );
}

#[test]
fn probe_all_mmio_fault_recorded_not_fatal() {
    let mut reg = DriverRegistry::new();
    reg.register(leak(MockDriver::new("mock-blk", 2)));
    reg.register(leak(MockDriver::new("mock-net", 1)));

    // null 窗（真机语义 = 未授 VMO/破窗）：两条组合各记一条 MmioFault，
    // 循环不断——后续组合照走。
    let slots = [(core::ptr::null_mut::<u8>(), 2)];
    let rs = reg.probe_all(&slots);
    assert_eq!(rs.len(), 2);
    assert!(rs.iter().all(|r| r.outcome == Err(DriverError::MmioFault)));

    // 同一 registry 紧接着探好槽照常工作（炸面不残留状态）。
    let good = [(0x3000 as *mut u8, 2)];
    let rs2 = reg.probe_all(&good);
    assert_eq!(rs2[0].outcome, Ok(ProbeInfo { device_id: 2, vendor_id: 0x1af4, version: 2 }));
}

#[test]
fn probe_all_empty_faces() {
    let mut reg = DriverRegistry::new();
    assert!(reg.is_empty());
    // 空注册表 × 有槽 / 有注册 × 空槽表：空 Vec，零 panic。
    let slots = [(0x1000 as *mut u8, 2)];
    assert!(reg.probe_all(&slots).is_empty());

    reg.register(leak(MockDriver::new("mock-blk", 2)));
    assert_eq!(reg.len(), 1);
    // 有注册 × 空槽表：空 Vec。
    assert!(reg.probe_all(&[]).is_empty());
}

// ---------- 生命周期：start→stop→start 循环 ----------

#[test]
fn lifecycle_start_stop_start() {
    let d = leak(MockDriver::new("mock-blk", 2));

    // 未 probe 就起/停：状态机门挡下（NotReady）。
    assert_eq!(d.start(), Err(DriverError::NotReady));
    assert_eq!(d.stop(), Err(DriverError::NotReady));

    // probe 认领 → 起停循环三轮（stop 不注销驱动，可反复服役）。
    d.probe(0x1000 as *mut u8, 2).unwrap();
    for _ in 0..3 {
        d.start().unwrap();
        d.stop().unwrap();
    }
    d.start().unwrap(); // 循环末态：start 在役
}

// ---------- virtio_example：假 MMIO 面板上的全套 ----------

/// 页对齐假 MMIO 窗（u32 volatile 访问的对齐前提；真机同构 = 授窗 VMO）。
#[repr(align(4096))]
struct MmioWindow([u8; 4096]);

/// 造一块填好 virtio-mmio 鉴定寄存器的面板（QEMU vendor = "QEMU" 0x554d4551）。
fn window_with(did: u32) -> MmioWindow {
    let mut w = MmioWindow([0u8; 4096]);
    put32(&mut w.0, mmio::MAGIC, mmio::MAGIC_VAL);
    put32(&mut w.0, mmio::VERSION, mmio::VERSION_MODERN);
    put32(&mut w.0, mmio::DEVICE_ID, did);
    put32(&mut w.0, mmio::VENDOR_ID, 0x554d_4551);
    w
}

fn put32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn get32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

#[test]
fn virtio_example_probe_criteria() {
    // 认领路径：blk 驱动对 blk 面板——info 三件套 = 寄存器读数。
    let mut w = window_with(mmio::DEVICE_ID_BLOCK);
    let mut drv = VirtioMmioDriver::new(mmio::DEVICE_ID_BLOCK);
    let info = drv.probe(w.0.as_mut_ptr(), 77).unwrap();
    assert_eq!(
        info,
        ProbeInfo { device_id: 2, vendor_id: 0x554d_4551, version: 2 }
    );
    assert_eq!(drv.info(), info);

    // did 不符（面板是 blk、驱动要 net）：拒，错误值带实际 did。
    let mut net = VirtioMmioDriver::new(mmio::DEVICE_ID_NETWORK);
    assert_eq!(
        net.probe(w.0.as_mut_ptr(), 0),
        Err(DriverError::UnsupportedDevice { device_id: mmio::DEVICE_ID_BLOCK })
    );

    // 坏 magic（非 virtio 槽）：拒，device_id 带回原始魔数读数。
    let mut bad = window_with(mmio::DEVICE_ID_BLOCK);
    put32(&mut bad.0, mmio::MAGIC, 0xdead_beef);
    let mut drv2 = VirtioMmioDriver::new(mmio::DEVICE_ID_BLOCK);
    assert_eq!(
        drv2.probe(bad.0.as_mut_ptr(), 0),
        Err(DriverError::UnsupportedDevice { device_id: 0xdead_beef })
    );

    // legacy 传输层（version=1）：拒——vprobe 拒 legacy 同律。
    let mut legacy = window_with(mmio::DEVICE_ID_BLOCK);
    put32(&mut legacy.0, mmio::VERSION, 1);
    let mut drv3 = VirtioMmioDriver::new(mmio::DEVICE_ID_BLOCK);
    assert_eq!(
        drv3.probe(legacy.0.as_mut_ptr(), 0),
        Err(DriverError::UnsupportedDevice { device_id: mmio::DEVICE_ID_BLOCK })
    );

    // null 窗：MmioFault。
    let mut drv4 = VirtioMmioDriver::new(mmio::DEVICE_ID_BLOCK);
    assert_eq!(
        drv4.probe(core::ptr::null_mut::<u8>(), 5),
        Err(DriverError::MmioFault)
    );
}

#[test]
fn virtio_example_lifecycle_status_writes() {
    let mut w = window_with(mmio::DEVICE_ID_BLOCK);
    let mut drv = VirtioMmioDriver::new(mmio::DEVICE_ID_BLOCK);

    // 未 probe 就 start：NotReady。
    assert_eq!(drv.start(), Err(DriverError::NotReady));

    // start 后 STATUS 终值 = 规范 §2.1 全四步状态位。
    drv.probe(w.0.as_mut_ptr(), 77).unwrap();
    drv.start().unwrap();
    assert_eq!(
        get32(&w.0, mmio::STATUS),
        ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK | ST_DRIVER_OK
    );
    // 重复 start 幂等（零特判restart 面）。
    drv.start().unwrap();

    // stop = STATUS 复位 0（规范复位动作）。
    drv.stop().unwrap();
    assert_eq!(get32(&w.0, mmio::STATUS), 0);

    // 生命周期循环：stop 后可再 start，状态位重新拉满。
    drv.start().unwrap();
    assert_eq!(
        get32(&w.0, mmio::STATUS),
        ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK | ST_DRIVER_OK
    );
}

#[test]
fn virtio_example_in_registry() {
    // 框架合面：真 virtio 包装驱动注册进 registry，与 mock 同坐一表——
    // 槽 0 = 好 blk 面板（认领）、槽 1 = 坏 magic 面板（拒但不炸）。
    let mut reg = DriverRegistry::new();
    reg.register(leak(VirtioMmioDriver::new(mmio::DEVICE_ID_BLOCK)));
    reg.register(leak(MockDriver::new("mock-net", 1)));

    let mut good = window_with(mmio::DEVICE_ID_BLOCK);
    let mut bad = window_with(mmio::DEVICE_ID_BLOCK);
    put32(&mut bad.0, mmio::MAGIC, 0);

    let slots = [(good.0.as_mut_ptr(), 77), (bad.0.as_mut_ptr(), 0)];
    let rs = reg.probe_all(&slots);

    assert_eq!(rs.len(), 4);
    // 槽 0：virtio 驱动认领 blk。
    assert!(matches!(rs[0].outcome, Ok(info) if info.device_id == mmio::DEVICE_ID_BLOCK));
    // 槽 1：virtio 驱动拒（坏 magic），mock-net 也拒（did=0 不认）——全记录。
    assert!(matches!(rs[2].outcome, Err(DriverError::UnsupportedDevice { .. })));
    assert!(matches!(rs[3].outcome, Err(DriverError::UnsupportedDevice { .. })));
}
