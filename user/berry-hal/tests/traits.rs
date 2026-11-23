//! host 测试：统一 HAL 四 trait 三面——mock 各 trait（IrqCtl 函数指针表/
//! DmaAlloc 配额分配器/Clock 虚拟推进）、库参考实现（BumpDmaAlloc 蓄水池/
//! VirtualClock）、trait object 使用（`Box<dyn Mmio>` 等四trait 的 dyn 面
//! ——统一抽象让驱动经 dyn 持平台实现，mock 与真实现同契约互换）+
//! VirtioMmio 假面板全套（volatile 读写 roundtrip/界检值不炸/寄存器
//! 便捷面偏移单源）。

use berry_hal::{BumpDmaAlloc, Clock, DmaAlloc, HalError, IrqCtl, Mmio, VirtioMmio, VirtualClock};
use berry_virtio::mmio;
use berry_virtio::{ST_ACKNOWLEDGE, ST_DRIVER, ST_DRIVER_OK, ST_FEATURES_OK};

// ---------- mock IrqCtl：函数指针表 + 使能位图 ----------

/// 静态处理函数（fn() 面——取地址即用，不被调用也无 dead_code 面）。
fn handler_a() {}
fn handler_b() {}

#[derive(Default)]
struct MockIrqCtl {
    /// 已注册表（irq → handler 函数指针）。
    table: Vec<(u32, fn())>,
    /// 使能集（irq → true 使能/false 屏蔽；未注即不在册）。
    enabled: Vec<(u32, bool)>,
}

impl MockIrqCtl {
    fn handler_of(&self, irq: u32) -> Option<fn()> {
        self.table.iter().find(|(i, _)| *i == irq).map(|(_, h)| *h)
    }

    fn is_enabled(&self, irq: u32) -> Option<bool> {
        self.enabled.iter().find(|(i, _)| *i == irq).map(|(_, e)| *e)
    }
}

impl IrqCtl for MockIrqCtl {
    fn register(&mut self, irq: u32, handler: fn()) -> Result<(), HalError> {
        if self.handler_of(irq).is_some() {
            return Err(HalError::IrqTaken); // 单主人：占号拒绝
        }
        self.table.push((irq, handler));
        self.enabled.push((irq, false)); // 注册即入册（默认屏蔽，待 enable）
        Ok(())
    }

    fn unregister(&mut self, irq: u32) {
        self.table.retain(|(i, _)| *i != irq);
        self.enabled.retain(|(i, _)| *i != irq);
    }

    fn enable(&mut self, irq: u32) {
        // 值不炸：未注号 enable 静默入使能册（真机 = 无主号写寄存器同哑）。
        match self.enabled.iter_mut().find(|(i, _)| *i == irq) {
            Some(slot) => slot.1 = true,
            None => self.enabled.push((irq, true)),
        }
    }

    fn disable(&mut self, irq: u32) {
        if let Some(slot) = self.enabled.iter_mut().find(|(i, _)| *i == irq) {
            slot.1 = false;
        }
    }
}

// ---------- mock DmaAlloc：配额分配器（与 BumpDmaAlloc 互换验证） ----------

/// 配额分配器：全局配额耗尽即 Oom（演示多实现同契约互换——HAL 的本质面）。
struct QuotaDmaAlloc {
    quota_bytes: usize,
    used: usize,
}

impl QuotaDmaAlloc {
    fn new(quota_bytes: usize) -> Self {
        Self { quota_bytes, used: 0 }
    }
}

impl DmaAlloc for QuotaDmaAlloc {
    fn alloc_coherent(&mut self, size: usize) -> Result<*mut u8, HalError> {
        if size == 0 {
            return Err(HalError::Invalid);
        }
        // 先验后记：失败路径不记账（Oom 拒绝后配额不被虚耗）。
        let next = self.used + size;
        if next > self.quota_bytes {
            return Err(HalError::Oom);
        }
        self.used = next;
        // mock 面：64B 对齐哑指针（leak 形态，测试进程尾收）。
        Ok(Box::into_raw(Box::new([0u8; 64])).cast::<u8>())
    }

    fn free_coherent(&mut self, _ptr: *mut u8) {
        // mock 面只记账（真 free 归真机实现）。
    }
}

// ---------- mock Clock：Cell 直推（与 VirtualClock 互换验证） ----------

/// mock 时钟：`Cell` 直持零原子开销（host 单线程挂具形态——trait 的
/// `&self` 契约下自持状态走内部可变性；VirtualClock 的原子面是跨核形态）。
struct MockClock {
    now: std::cell::Cell<u64>,
}

impl MockClock {
    fn new() -> Self {
        Self { now: std::cell::Cell::new(0) }
    }
}

impl Clock for MockClock {
    fn now_us(&self) -> u64 {
        self.now.get()
    }

    fn spin_us(&self, us: u64) {
        self.now.set(self.now.get() + us);
    }
}

// ---------- 假 MMIO 面板（berry-driver 测试同款形态） ----------

/// 页对齐假 MMIO 窗（u32 volatile 访问的对齐前提；真机同构 = 授窗 VMO）。
#[repr(align(4096))]
struct MmioWindow([u8; 4096]);

fn put32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}

fn get32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(buf[off..off + 4].try_into().unwrap())
}

/// 填好 virtio-mmio 鉴定寄存器的面板（QEMU vendor = 0x554d4551）。
fn window_with(did: u32) -> MmioWindow {
    let mut w = MmioWindow([0u8; 4096]);
    put32(&mut w.0, mmio::MAGIC, mmio::MAGIC_VAL);
    put32(&mut w.0, mmio::VERSION, mmio::VERSION_MODERN);
    put32(&mut w.0, mmio::DEVICE_ID, did);
    put32(&mut w.0, mmio::VENDOR_ID, 0x554d_4551);
    w
}

// ---------- IrqCtl：注册/占用/注销/使能 ----------

/// 函数指针相等断言辅助（`==` 对 fn 指针触发
/// `unpredictable_function_pointer_comparisons` lint——地址唯一性不保；
/// `fn_addr_eq` 是规范语义面）。
fn same_handler(a: fn(), b: fn()) -> bool {
    core::ptr::fn_addr_eq(a, b)
}

#[test]
fn irq_register_taken_unregister_cycle() {
    let mut ctl = MockIrqCtl::default();

    // 注册成功：入册 + 默认屏蔽。
    assert_eq!(ctl.register(77, handler_a), Ok(()));
    assert!(ctl.handler_of(77).is_some_and(|h| same_handler(h, handler_a)));
    assert_eq!(ctl.is_enabled(77), Some(false));

    // 单主人占用：同号二注拒（IrqTaken），原 handler 不被顶。
    assert_eq!(ctl.register(77, handler_b), Err(HalError::IrqTaken));
    assert!(ctl.handler_of(77).is_some_and(|h| same_handler(h, handler_a)));

    // 异号并行注册互不扰。
    assert_eq!(ctl.register(78, handler_b), Ok(()));
    assert!(ctl.handler_of(78).is_some_and(|h| same_handler(h, handler_b)));

    // 使能/屏蔽翻转。
    ctl.enable(77);
    assert_eq!(ctl.is_enabled(77), Some(true));
    ctl.disable(77);
    assert_eq!(ctl.is_enabled(77), Some(false));

    // 注销后可再注（占用释放——handler_b 顶上）。
    ctl.unregister(77);
    assert!(ctl.handler_of(77).is_none());
    assert_eq!(ctl.register(77, handler_b), Ok(()));
    assert!(ctl.handler_of(77).is_some_and(|h| same_handler(h, handler_b)));

    // 值不炸：注销未注号/屏蔽未注号都是哑操作。
    ctl.unregister(999);
    ctl.disable(999);
    assert!(ctl.handler_of(999).is_none());
}

// ---------- DmaAlloc：BumpDmaAlloc 蓄水池 + 配额 mock 互换 ----------

#[test]
fn bump_dma_alloc_alignment_and_oom() {
    let mut dma = BumpDmaAlloc::new(256).unwrap();
    assert_eq!(dma.capacity(), 256);
    assert_eq!(dma.live_count(), 0);

    // 首分配：64B 对齐（cacheline 契约）。
    let p1 = dma.alloc_coherent(100).unwrap();
    assert_eq!(p1 as usize % berry_hal::DMA_ALIGN, 0);
    assert_eq!(dma.live_count(), 1);

    // bump 单调：次分配指针严格前进（不复用）。
    let p2 = dma.alloc_coherent(28).unwrap();
    assert!(p2 as usize > p1 as usize);
    assert_eq!(p2 as usize % berry_hal::DMA_ALIGN, 0);
    assert_eq!(dma.live_count(), 2);

    // free 只销账不回收：live 减、used 不退（bump 单调契约）。
    let used_before = dma.used();
    let pool_before = dma.pool_used();
    assert_eq!(used_before, 100 + 28, "配额只计交付字节，垫片不入账");
    assert!(pool_before >= used_before, "物理游标含垫片，≥ 配额已计");
    dma.free_coherent(p1);
    assert_eq!(dma.live_count(), 1);
    assert_eq!((dma.used(), dma.pool_used()), (used_before, pool_before));

    // 池尽 Oom：剩余不足即拒（配额门 128 + 1000 > 256）。
    assert_eq!(dma.alloc_coherent(1000), Err(HalError::Oom));
    assert_eq!(dma.used(), used_before, "失败路径零记账");
    assert_eq!(dma.live_count(), 1);

    // 零长分配 Invalid。
    assert_eq!(dma.alloc_coherent(0), Err(HalError::Invalid));

    // 池外指针 free 静默忽略（值不炸：哑指针不炸账）。
    dma.free_coherent(0xdead_0000 as *mut u8);
    assert_eq!(dma.live_count(), 1);

    // 零容量池建池即拒（Invalid——哑池无诊断价值）。
    assert_eq!(BumpDmaAlloc::new(0).err(), Some(HalError::Invalid));
}

#[test]
fn dma_alloc_impls_interchangeable() {
    // trait 的本质面：同一使用点换实现即换行为（驱动经 dyn DmaAlloc
    // 持有，平台差异不可见）。共同契约 = 零长 Invalid + 分配/耗尽走
    // 错误值；耗尽门各按各的后备（配额计数 vs 蓄水池配额门——同 cap
    // 同结果：垫片不计配额，由蓄水池物理长兜底）。
    let mut quota = QuotaDmaAlloc::new(128);
    assert!(quota.alloc_coherent(100).is_ok());
    assert_eq!(quota.alloc_coherent(100), Err(HalError::Oom));
    assert_eq!(quota.alloc_coherent(0), Err(HalError::Invalid));

    let mut bump = BumpDmaAlloc::new(128).unwrap();
    assert!(bump.alloc_coherent(100).is_ok());
    assert_eq!(bump.alloc_coherent(100), Err(HalError::Oom)); // 配额门 200 > 128
    assert_eq!(bump.alloc_coherent(0), Err(HalError::Invalid));
    assert_eq!(bump.used(), 100);

    // 配额恰兑满：cap=128 两笔 64 必成（首笔 64 倍数 → 零分配间垫片，
    // 任意残差下物理门 p0 + 128 ≤ 191 恒过），第三笔由配额门拒。
    let mut bump = BumpDmaAlloc::new(128).unwrap();
    assert!(bump.alloc_coherent(64).is_ok());
    assert!(bump.alloc_coherent(64).is_ok());
    assert_eq!(bump.used(), 128);
    assert_eq!(bump.alloc_coherent(1), Err(HalError::Oom));
    assert_eq!(bump.used(), 128);
}

// ---------- DmaAlloc：配额记账回归（2026-11-24 垫片不计配额） ----------

#[test]
fn bump_dma_single_alloc_of_cap_always_succeeds() {
    // 基线缺陷面之一：Oom 门按含基址垫片的相对偏移执法时，堆基址非 64
    // 对齐的池单笔 cap 字节即 Oom（最差少 63 字节）。修正后基址垫片由
    // 物理长 cap + 63 吸收，单笔 cap 必成、配额恰满。
    for cap in [1usize, 7, 100, 4096] {
        let mut d = BumpDmaAlloc::new(cap).unwrap();
        let p = d.alloc_coherent(cap).unwrap();
        assert_eq!(p as usize % berry_hal::DMA_ALIGN, 0);
        assert_eq!((d.used(), d.capacity(), d.live_count()), (cap, cap, 1));
        assert!(d.pool_used() >= cap && d.pool_used() <= cap + berry_hal::DMA_ALIGN - 1);
        assert_eq!(d.alloc_coherent(1), Err(HalError::Oom)); // 配额门
        assert_eq!(d.used(), cap, "失败路径零记账");
    }
}

#[test]
fn bump_dma_two_half_allocs_quota_gate_not_padding() {
    // 基线缺陷回归钉板：cap=100 两笔 50——基线第二笔恒 Oom（0..50 →
    // 对齐到 64 → 64 + 50 = 114 > 100）且 used()=50 < cap 自相矛盾。
    // 修正后配额只计交付字节：基址垫片 p0 ≤ 49 时两笔必成、第三笔由
    // 配额门拒；p0 ≥ 50（仅 1 字节对齐分配器可致）时第二笔由物理门拒。
    // p0 由首笔后的物理游标反推（A1 = p0，end = p0 + 50）。
    let mut d = BumpDmaAlloc::new(100).unwrap();
    let p1 = d.alloc_coherent(50).unwrap();
    let p0 = d.pool_used() - 50;
    assert_eq!(p1 as usize % berry_hal::DMA_ALIGN, 0);
    assert_eq!(d.used(), 50);
    match d.alloc_coherent(50) {
        Ok(p2) => {
            assert!(p0 <= 49, "p0={p0} 不该放下第二笔");
            assert!(p2 as usize >= p1 as usize + 50, "不重叠");
            assert_eq!(p2 as usize % berry_hal::DMA_ALIGN, 0);
            assert_eq!(d.used(), 100);
            assert_eq!(d.live_count(), 2);
            assert_eq!(d.alloc_coherent(1), Err(HalError::Oom)); // 配额门恰满
            assert_eq!(d.used(), 100);
        }
        Err(HalError::Oom) => {
            assert!(p0 >= 50, "p0={p0} 本该放下第二笔");
            assert_eq!((d.used(), d.live_count()), (50, 1), "物理门拒零记账");
        }
        Err(e) => panic!("unexpected {e:?}"),
    }
}

#[test]
fn bump_dma_cacheline_multiples_redeem_quota_exactly() {
    // size 全为 64 倍数：零分配间垫片，Oom ⟺ Σ size > cap——64×64 恰兑满
    // cap=4096（基线在宿主首实例实抓残差 16 时第 64 笔即 Oom，只兑 4032）。
    let mut d = BumpDmaAlloc::new(4096).unwrap();
    let mut prev = 0usize;
    for i in 1..=64usize {
        let p = d.alloc_coherent(64).unwrap();
        assert_eq!(p as usize % berry_hal::DMA_ALIGN, 0, "第 {i} 笔");
        assert!(p as usize >= prev + 64, "第 {i} 笔重叠或回退");
        prev = p as usize;
        assert_eq!(d.used(), 64 * i);
    }
    assert_eq!((d.used(), d.capacity()), (4096, 4096));
    assert!(d.pool_used() <= 4096 + 63);
    assert_eq!(d.alloc_coherent(64), Err(HalError::Oom));
    assert_eq!(d.alloc_coherent(1), Err(HalError::Oom));
    assert_eq!(d.used(), 4096);
}

#[test]
fn bump_dma_sub_cacheline_runs_hit_physical_gate_pool_used_diagnoses() {
    // 1 字节 × N：每笔 63 字节分配间垫片累积，物理门先于配额门——64 或
    // 65 笔后 Oom（取决于基址残差），此时 used() 远小于 capacity()，但
    // pool_used() 已 ≥ cap（物理池将尽）——两门可分辨。
    let mut d = BumpDmaAlloc::new(4096).unwrap();
    let mut n = 0usize;
    while d.alloc_coherent(1).is_ok() {
        n += 1;
        assert!(n <= 65, "物理长 4159 最多容 65 笔 1 字节，已 {n}");
    }
    assert!(n == 64 || n == 65, "n={n}");
    assert_eq!(d.used(), n);
    assert_eq!(d.live_count(), n);
    assert!(d.used() < d.capacity());
    assert!(d.pool_used() >= d.capacity(), "pool_used={} cap={}", d.pool_used(), d.capacity());
    assert!(d.pool_used() <= d.capacity() + berry_hal::DMA_ALIGN - 1);
    // 再拒也零记账（配额门或物理门皆不动游标）。
    let snap = (d.used(), d.pool_used(), d.live_count());
    assert_eq!(d.alloc_coherent(1), Err(HalError::Oom));
    assert_eq!((d.used(), d.pool_used(), d.live_count()), snap);
}

#[test]
fn bump_dma_allocated_blocks_stable_across_later_allocs_and_frees() {
    // 不改已分配指针：后续分配不重叠、不改写前块；free 不动内存。
    let mut d = BumpDmaAlloc::new(4096).unwrap();
    let sizes = [64usize, 1, 100, 7, 64];
    let mut blocks = Vec::new();
    for (i, &s) in sizes.iter().enumerate() {
        let p = d.alloc_coherent(s).unwrap();
        assert_eq!(p as usize % berry_hal::DMA_ALIGN, 0);
        unsafe { core::ptr::write_bytes(p, 0xf0 + i as u8, s) };
        blocks.push((s, p));
    }
    assert_eq!(d.used(), sizes.iter().sum::<usize>());
    for w in blocks.windows(2) {
        assert!(w[0].1 as usize + w[0].0 <= w[1].1 as usize, "块重叠");
    }
    d.free_coherent(blocks[0].1);
    let _ = d.alloc_coherent(64).unwrap();
    d.free_coherent(blocks[2].1);
    for (i, &(s, p)) in blocks.iter().enumerate() {
        let buf = unsafe { core::slice::from_raw_parts(p, s) };
        assert!(buf.iter().all(|&b| b == 0xf0 + i as u8), "块 {i} 被改写");
    }
    assert_eq!(d.live_count(), 4);
    assert_eq!(d.used(), sizes.iter().sum::<usize>() + 64, "free 不退配额");
}

// ---------- Clock：VirtualClock 虚拟推进 ----------

#[test]
fn virtual_clock_monotonic_spin() {
    let c = VirtualClock::new();
    assert_eq!(c.now_us(), 0);

    // spin 直推虚拟时间（零真延时——host 测试快）。
    c.spin_us(150);
    assert_eq!(c.now_us(), 150);
    c.spin_us(50);
    assert_eq!(c.now_us(), 200); // 单调累加

    // Default 同零起点。
    assert_eq!(VirtualClock::default().now_us(), 0);
}

// ---------- trait object：四 trait 的 dyn 面 ----------

#[test]
fn trait_objects_all_four_traits() {
    // Mmio：Box<dyn Mmio> 持真实现（假面板窗）。
    let mut w = window_with(mmio::DEVICE_ID_BLOCK);
    let m: Box<dyn Mmio> = Box::new(VirtioMmio::new(w.0.as_mut_ptr(), 4096).unwrap());
    assert_eq!(m.read32(mmio::DEVICE_ID), mmio::DEVICE_ID_BLOCK);
    let mut m = m;
    m.write32(mmio::STATUS, 0x5a);
    assert_eq!(get32(&w.0, mmio::STATUS), 0x5a); // 经 dyn 写直落面板

    // IrqCtl：&mut dyn 持 mock。
    let mut ctl = MockIrqCtl::default();
    let i: &mut dyn IrqCtl = &mut ctl;
    assert_eq!(i.register(41, handler_a), Ok(()));
    assert_eq!(i.register(41, handler_b), Err(HalError::IrqTaken));
    i.enable(41);
    assert_eq!(ctl.is_enabled(41), Some(true));

    // DmaAlloc：Box<dyn DmaAlloc> 持蓄水池（配额 mock 同坐一列——互换）。
    let d: Box<dyn DmaAlloc> = Box::new(BumpDmaAlloc::new(512).unwrap());
    let mut d = d;
    let p = d.alloc_coherent(64).unwrap();
    assert!(!p.is_null());
    d.free_coherent(p);

    // Clock：&dyn Clock 持 mock（Cell 直推——与 VirtualClock 原子面互换）。
    let mc = MockClock::new();
    let c: &dyn Clock = &mc;
    c.spin_us(42);
    assert_eq!(c.now_us(), 42);
    assert_eq!(mc.now_us(), 42); // 同一实例经具名面再见
}

/// 面板全零但窗合法的窗（鉴定寄存器全 0——非 virtio 槽）。
fn zero_window() -> MmioWindow {
    MmioWindow([0u8; 4096])
}

// ---------- VirtioMmio：volatile 读写 + 界检值不炸 ----------

#[test]
fn virtio_mmio_roundtrip_and_bounds() {
    let mut w = zero_window();
    let mut mm = VirtioMmio::new(w.0.as_mut_ptr(), 4096).unwrap();
    assert_eq!(mm.len(), 4096);
    assert!(!mm.is_empty());

    // 全窗 roundtrip：首/尾寄存器写读回（volatile 路径直落面板）。
    let last = 4096 - 4;
    mm.write32(0, 0x1122_3344);
    mm.write32(last, 0x5566_7788);
    assert_eq!(mm.read32(0), 0x1122_3344);
    assert_eq!(mm.read32(last), 0x5566_7788);
    assert_eq!(get32(&w.0, 0), 0x1122_3344);
    assert_eq!(get32(&w.0, last), 0x5566_7788);

    // 界外读 0（尾邻/远外/溢出面）。
    assert_eq!(mm.read32(4096), 0);
    assert_eq!(mm.read32(0x1_0000_0000), 0);
    assert_eq!(mm.read32(usize::MAX - 2), 0);

    // 界外写弃（面板原值不动）。
    mm.write32(4096, 0xffff_ffff);
    mm.write32(4094, 0xffff_ffff); // 界内起点但跨尾 = 越界
    assert_eq!(get32(&w.0, last), 0x5566_7788);

    // 非对齐偏移拒（读 0 写弃——u32 volatile 对齐前提）。
    assert_eq!(mm.read32(2), 0);
    mm.write32(2, 0xaabb_ccdd);
    assert_eq!(get32(&w.0, 0), 0x1122_3344); // 原值未被搅动

    // null/零长窗建窗即拒（Invalid——裸指针字段不可 derive，走 .err() 面）。
    assert_eq!(VirtioMmio::new(core::ptr::null_mut(), 4096).err(), Some(HalError::Invalid));
    assert_eq!(VirtioMmio::new(w.0.as_mut_ptr(), 0).err(), Some(HalError::Invalid));
}

#[test]
fn virtio_mmio_register_semantics() {
    // 鉴定便捷面：偏移单源引 berry_virtio::mmio（面板填啥读啥）。
    let mut w = window_with(mmio::DEVICE_ID_NETWORK);
    let mm = VirtioMmio::new(w.0.as_mut_ptr(), 4096).unwrap();
    assert_eq!(mm.magic(), mmio::MAGIC_VAL);
    assert_eq!(mm.version(), mmio::VERSION_MODERN);
    assert_eq!(mm.device_id(), mmio::DEVICE_ID_NETWORK);
    assert_eq!(mm.vendor_id(), 0x554d_4551);
    assert!(mm.is_virtio_modern());

    // 非 virtio 槽（全零窗）：探测判据复合面拒。
    let mut z = zero_window();
    let zm = VirtioMmio::new(z.0.as_mut_ptr(), 4096).unwrap();
    assert!(!zm.is_virtio_modern());

    // legacy 传输层（version=1）：modern 判据拒——vprobe 拒 legacy 同律。
    let mut l = window_with(mmio::DEVICE_ID_BLOCK);
    put32(&mut l.0, mmio::VERSION, 1);
    let lm = VirtioMmio::new(l.0.as_mut_ptr(), 4096).unwrap();
    assert!(!lm.is_virtio_modern());

    // 窄窗截尾：鉴定寄存器落窗外读 0（值不炸——授窗长度执法）。
    let mut n = window_with(mmio::DEVICE_ID_BLOCK);
    let nm = VirtioMmio::new(n.0.as_mut_ptr(), 0x0c).unwrap(); // 只授到 DEVICE_ID 尾
    assert_eq!(nm.magic(), mmio::MAGIC_VAL); // 0x00..0x04 在窗内
    assert_eq!(nm.version(), mmio::VERSION_MODERN); // 0x04..0x08 在窗内
    assert_eq!(nm.device_id(), mmio::DEVICE_ID_BLOCK); // 0x08..0x0c 恰压窗尾
    assert_eq!(nm.vendor_id(), 0); // 0x0c..0x10 越界读 0
    assert!(nm.is_virtio_modern()); // 判据只看 magic/version——均在内

    // 写面便捷：STATUS 状态机 + INT_ACK/Q_NOTIFY 直落面板。
    let mut w2 = window_with(mmio::DEVICE_ID_BLOCK);
    let mut sm = VirtioMmio::new(w2.0.as_mut_ptr(), 4096).unwrap();
    sm.set_status(ST_ACKNOWLEDGE | ST_DRIVER);
    sm.set_status(ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK);
    sm.set_status(ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK | ST_DRIVER_OK);
    assert_eq!(sm.status(), ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK | ST_DRIVER_OK);
    sm.int_status();
    sm.int_ack(0x3);
    assert_eq!(get32(&w2.0, mmio::INT_ACK), 0x3);
    sm.queue_notify(0);
    assert_eq!(get32(&w2.0, mmio::Q_NOTIFY), 0);
}

// ---------- 合面：经 trait object 跑驱动业务序（HAL 的价值钉板） ----------

#[test]
fn hal_composition_driver_business_via_dyn() {
    // 「驱动只写业务逻辑」合面：探测（dyn Mmio 鉴定读）→ 服役（STATUS
    // 状态机四步）→ kick 队列 → 延时（dyn Clock 虚拟推进）——全程零
    // unsafe、零平台面（换平台只换 Box 里的实现，业务序不动）。
    let mut w = window_with(mmio::DEVICE_ID_BLOCK);
    let m: Box<dyn Mmio> = Box::new(VirtioMmio::new(w.0.as_mut_ptr(), 4096).unwrap());
    let c: &dyn Clock = &VirtualClock::new();

    // 探测：传输层鉴定 + 设备认领。
    assert_eq!(m.read32(mmio::MAGIC), mmio::MAGIC_VAL);
    assert_eq!(m.read32(mmio::VERSION), mmio::VERSION_MODERN);
    assert_eq!(m.read32(mmio::DEVICE_ID), mmio::DEVICE_ID_BLOCK);

    // 服役：规范 §2.1 状态机四步（经 dyn 写）。
    let mut m = m;
    m.write32(mmio::STATUS, ST_ACKNOWLEDGE | ST_DRIVER);
    m.write32(mmio::STATUS, ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK);
    assert_eq!(m.read32(mmio::STATUS) & ST_FEATURES_OK, ST_FEATURES_OK); // 回读验
    m.write32(
        mmio::STATUS,
        ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK | ST_DRIVER_OK,
    );

    // 业务：kick + 延时轮询形态（虚拟时钟零真延时）。
    m.write32(mmio::Q_NOTIFY, 0);
    c.spin_us(1000);
    assert_eq!(c.now_us(), 1000);
    assert_eq!(m.read32(mmio::STATUS), ST_ACKNOWLEDGE | ST_DRIVER | ST_FEATURES_OK | ST_DRIVER_OK);
}
