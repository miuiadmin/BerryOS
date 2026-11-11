# unsafe 台账（SEC-01 §4.5 件 22）

> 内核侧 unsafe 面单一事实源：`kernel/ mm/ sched/ ipc/ arch/` 的每个 unsafe
> 出现点逐条登记（file:line + 类型 + 用途分类），密度对标 Android ~4% 口径。
> 产物由 `tools/unsafe_audit.py` 生成（两个 marker 块内勿手改行号/增删行）；
> 改代码后重跑 `python3 tools/unsafe_audit.py` 再入册（abi-gen 同纪律），CI 以
> `--check` 执法（漂移 = FAIL）。密度趋势线 = 本文档的 git 史（每次重产的
> 密度块 diff 即趋势采样点）。

## 一、口径

- **扫描面**：`kernel/ mm/ sched/ ipc/ arch/` 全部 `.rs`（内核侧 crate）。
  `user/ drivers/ bootloader/` 暂不入表（挂后续扩面——扩面时改工具 SCAN_DIRS）。
- **出现点四类分开计**：`block`（unsafe 块）/ `fn`（unsafe fn，含
  `unsafe extern "C" fn` 定义）/ `impl`（unsafe impl）/ `extern`
  （`unsafe extern "C" {}` 外部块）。类型签名位提及（形如
  `Option<unsafe extern "C" fn(..)>` 的 fn 指针类型）**只计数不登记**——其实际
  调用点必有 unsafe 块在册，登记会双计。
- **六分类**（初判逐块核对于 2026-10-30 全量清单；改判直接改 §三 表的分类列，
  重产按内容锚继承）：
  - **页表**：PTE 读写/表树走查/TTBR0-ASID 激活/TLBI 维护（含写后 dsb 屏障）。
  - **上下文切换**：切换 asm/任务上下文装配/异常向量与帧保存（规则/01
    「上下文切换/寄存器保存」簇；trap 向量表与 x86 IDT/TSS/syscall 入口 MSR 归此）。
  - **MMIO**：设备 MMIO + **系统寄存器/特权指令**（mrs/msr/wfi/hvc/out/in/
    rdmsr…——按 规则/01「MMIO/寄存器访问」簇归并；纯设备 MMIO 与系统寄存器
    未拆两列，口径如实注记）。
  - **分配器**：SlabHeap/GlobalAlloc/freelist/帧分配器静态/Task Box 分配基元。
  - **硬件原子**：手写原子 asm（ldxr/stxr/casa/outline-atomics 面）——**现状
    0 处**（原子操作全走 `core::sync::atomic` 安全面）；类别保留占位。
  - **其它**：裸指针解引用/静态可变槽/对象互钉/ABI 出入参拷贝/内存基元/
    测试与基准面/遥测环静态——规则/01「外层包安全 API」的包裹层主体。
- **密度口径**：unsafe 跨度内**非空非注行** / 扫描面 `.rs` 物理总行。跨度近似
  （unsafe 块内安全子表达式、unsafe fn 整个函数体都计入）→ **偏高估**。
  对标 Android ~4%（Google 2025-11：unsafe 行占 Rust 总行比）时以此口径差为准
  ——两口径同向（行占比）但分母分子定义不同，趋势可比、绝对值慎比。
- **测试面剔除口径**：另报「剔除 ktest.rs/bench.rs」一行（生产面密度）。
- **说明列手改契约**：自动片段之后以「※」分隔可加人工备注；内容锚只认 ※ 前
  部分（行漂移不丢人工判读）。

## 二、密度总览（生成）

<!-- UNSAFE-AUDIT:SUMMARY:BEGIN -->
| 目录 | .rs 文件数 | 总行数 | unsafe 非空行 | 密度 |
|------|-----------|--------|--------------|------|
| kernel/ | 24 | 26916 | 1932 | 7.18% |
| mm/ | 6 | 1739 | 4 | 0.23% |
| sched/ | 12 | 6392 | 1534 | 24.00% |
| ipc/ | 0 | 0 | 0 | — |
| arch/ | 22 | 6806 | 960 | 14.11% |
| **合计** | 64 | 41853 | 4430 | **10.58%** |

- 剔除 ktest.rs/bench.rs（测试/基准面）：总行 33494，unsafe 非空行 3782，密度 11.29%（生产面口径）
- 分类计数：页表 52 | 上下文切换 32 | MMIO 151 | 分配器 13 | 硬件原子 0 | 其它 701
- 类型计数：block 814 | fn 131 | impl 2 | extern 2（另有类型签名位提及 14 处——只计不登记，防双计）
- 对标：Android ~4%（unsafe 行占 Rust 总行比，Google 2025-11）；本表为跨度非空行口径（偏高估），见 §一
<!-- UNSAFE-AUDIT:SUMMARY:END -->

## 三、登记表（生成，逐出现点一行）

<!-- UNSAFE-AUDIT:REGISTRY:BEGIN -->
| 文件 | 行 | 类型 | 分类 | 说明 |
|------|----|------|------|------|
| arch/aarch64/src/fwcfg.rs | 57 | block | MMIO | unsafe { mmio::write_u16(SELECTOR, FW_CFG_SIGNATURE.to_be()); mm |
| arch/aarch64/src/fwcfg.rs | 64 | fn | MMIO | unsafe fn stream_le32() -> u32 { mmio::read(DATA) } /// 顺序读一个 BE |
| arch/aarch64/src/fwcfg.rs | 69 | fn | MMIO | unsafe fn stream_be32() -> u32 { stream_le32().swap_bytes() } // |
| arch/aarch64/src/fwcfg.rs | 86 | block | MMIO | unsafe { mmio::write_u16(SELECTOR, FW_CFG_FILE_DIR.to_be()); let |
| arch/aarch64/src/fwcfg.rs | 119 | block | MMIO | unsafe { mmio::write_u16(SELECTOR, entry.select.to_be()); for ch |
| arch/aarch64/src/fwcfg.rs | 146 | block | MMIO | unsafe { mmio::write_u16(SELECTOR, entry.select.to_be()) }; Some |
| arch/aarch64/src/fwcfg.rs | 169 | block | MMIO | unsafe { for chunk in buf[..want as usize].chunks_mut(4) { let w |
| arch/aarch64/src/fwcfg.rs | 207 | fn | MMIO | unsafe fn put_be32(p: *mut u8, v: u32) { core::ptr::copy_nonover |
| arch/aarch64/src/fwcfg.rs | 212 | fn | MMIO | unsafe fn put_be64(p: *mut u8, v: u64) { core::ptr::copy_nonover |
| arch/aarch64/src/fwcfg.rs | 217 | fn | MMIO | unsafe fn get_be32(p: *const u8) -> u32 { let mut b = [0u8; 4]; |
| arch/aarch64/src/fwcfg.rs | 225 | fn | MMIO | unsafe fn dma_write(select: u16, data_pa: u64, len: u32) -> bool |
| arch/aarch64/src/fwcfg.rs | 263 | block | MMIO | unsafe { let base = core::ptr::addr_of_mut!(RAMFB_CFG.0) as *mut |
| arch/aarch64/src/gic.rs | 88 | fn | MMIO | unsafe fn init() { // ---- distributor：先关，再清状态 ---- mmio::write( |
| arch/aarch64/src/gic.rs | 122 | block | MMIO | unsafe { let en = mmio::read(GICD + GICD_ISENABLER0) & (1 << IRQ |
| arch/aarch64/src/gic.rs | 135 | block | MMIO | unsafe { mmio::read(GICC + GICC_IAR) & 0x3ff } } /// 写 EOIR 结束（必 |
| arch/aarch64/src/gic.rs | 141 | block | MMIO | unsafe { mmio::write(GICC + GICC_EOIR, irq_id & 0x3ff) }; } // - |
| arch/aarch64/src/gic.rs | 146 | fn | MMIO | unsafe fn set_priority(irq: usize, prio: u8) { mmio::write_u8(GI |
| arch/aarch64/src/gic.rs | 150 | fn | MMIO | unsafe fn set_target(irq: usize, target: u8) { mmio::write_u8(GI |
| arch/aarch64/src/gic.rs | 158 | fn | MMIO | unsafe fn set_config(irq: usize, cfg: u32) { let shift = 2 * (ir |
| arch/aarch64/src/gic.rs | 177 | fn | MMIO | unsafe fn enable_spi(irq: usize, prio: u8, target: u8, cfg: u32) |
| arch/aarch64/src/gic.rs | 195 | fn | MMIO | unsafe fn enable_cntv_this_core() { set_priority(IRQ_CNTV, PRIO_ |
| arch/aarch64/src/gic.rs | 222 | fn | MMIO | unsafe fn send_sgi(id: u8, target: SgiTarget) { let (filter, lis |
| arch/aarch64/src/gic.rs | 240 | fn | MMIO | unsafe fn enable_sgi_this_core(id: u8) { mmio::write(GICD + GICD |
| arch/aarch64/src/gic.rs | 251 | block | MMIO | unsafe { mmio::read(GICD + GICD_ISPENDR0) & (1 << (id & 0xf)) != |
| arch/aarch64/src/gic.rs | 273 | block | MMIO | unsafe { enable_spi(r_irq, r_prio, r_target, r_cfg) }; return tr |
| arch/aarch64/src/kern_lock.rs | 68 | block | 其它 | unsafe { NAME_HOOK = Some(f) }; } /// 打印互斥锁（整条消息粒度）。 static PRIN |
| arch/aarch64/src/kern_lock.rs | 110 | block | MMIO | unsafe { core::slice::from_raw_parts(p as *const u8, l) }; crate |
| arch/aarch64/src/kern_lock.rs | 153 | block | 其它 | unsafe { NAME_HOOK } { let (p, l) = h(); KL_HELD_TAG_PTR.store(p |
| arch/aarch64/src/kern_lock.rs | 223 | block | 其它 | unsafe { SVC_WINDOW = Some(f) }; } /// SVC 入口按类包络（slice 2b 真行为变更 |
| arch/aarch64/src/kern_lock.rs | 236 | block | 其它 | unsafe { SVC_WINDOW } { // SAFETY: 槽在 boot 单线程期注册；f 是 C-ABI fn 指 |
| arch/aarch64/src/kern_lock.rs | 238 | block | 其它 | unsafe { f(class, true) }; } if class & SVC_CLASS_TABLE != 0 { k |
| arch/aarch64/src/kern_lock.rs | 253 | block | 其它 | unsafe { SVC_WINDOW } { // SAFETY: 同上。 unsafe { f(class, false) |
| arch/aarch64/src/kern_lock.rs | 255 | block | 其它 | unsafe { f(class, false) }; } } /// SVC 入口包络（全量形）：EL0 fault kill |
| arch/aarch64/src/kern_lock.rs | 341 | block | MMIO | unsafe { core::arch::asm!("mrs {}, daif", out(reg) daif, options |
| arch/aarch64/src/kern_lock.rs | 351 | block | MMIO | unsafe { core::arch::asm!("mrs {}, daif", out(reg) daif, options |
| arch/aarch64/src/kern_lock.rs | 367 | block | 其它 | unsafe { PREEMPT_ENVELOPE = Some((enter, exit)) }; } #[inline] f |
| arch/aarch64/src/kern_lock.rs | 373 | block | 其它 | unsafe { PREEMPT_ENVELOPE } { e(); } } #[inline] fn preempt_exit |
| arch/aarch64/src/kern_lock.rs | 381 | block | 其它 | unsafe { PREEMPT_ENVELOPE } { x(); } } /// 拼行 API（G44）：整条消息在 PRI |
| arch/aarch64/src/lib.rs | 94 | block | 上下文切换 | unsafe { trap::init() }; // SAFETY: MMIO（Device 映射区）；EL1、单核、DAIF |
| arch/aarch64/src/lib.rs | 96 | block | MMIO | unsafe { gic::init() }; } /// 高半别名自校验（2b-2 起只走高半——低半身份已退役）：RAM 魔 |
| arch/aarch64/src/lib.rs | 116 | block | 页表 | unsafe { mmu::activate_ttbr0(ttbr0_pa, asid) }; } /// 2b-2：低半身份表 |
| arch/aarch64/src/lib.rs | 123 | block | 页表 | unsafe { mmu::retire_low_half() }; } /// 全量 ASID 失效（ASID 分配器回卷时调 |
| arch/aarch64/src/lib.rs | 129 | block | 页表 | unsafe { mmu::flush_all_asid() }; } /// 2b-3：按 ASID 失效 TLB——Vmar |
| arch/aarch64/src/lib.rs | 136 | block | 页表 | unsafe { mmu::tlbi_aside1(asid) }; } /// 3-2：读当前 TTBR0_EL1 表基 PA |
| arch/aarch64/src/lib.rs | 175 | block | 其它 | unsafe { IRQ_DISPATCH = Some(f) }; } /// 取已注册的 IRQ 派发回调（`excepti |
| arch/aarch64/src/lib.rs | 182 | fn | 其它 | unsafe fn irq_dispatch() -> Option<IrqDispatch> { IRQ_DISPATCH } |
| arch/aarch64/src/lib.rs | 197 | block | 其它 | unsafe { SGI_DISPATCH = Some(f) }; } /// 取已注册的 SGI 派发回调（`excepti |
| arch/aarch64/src/lib.rs | 204 | fn | 其它 | unsafe fn sgi_dispatch() -> Option<SgiDispatch> { SGI_DISPATCH } |
| arch/aarch64/src/lib.rs | 222 | block | 其它 | unsafe { USER_FAULT_EXIT = Some(f) }; } /// 取已注册的用户故障击杀回调（`excep |
| arch/aarch64/src/lib.rs | 229 | fn | 其它 | unsafe fn user_fault_exit() -> Option<UserFaultExit> { USER_FAUL |
| arch/aarch64/src/lib.rs | 260 | block | 其它 | unsafe { DEMAND_FAULT_PROBE = Some(f) }; } /// 注册 EL0 臂按需调页探针（注册 |
| arch/aarch64/src/lib.rs | 266 | block | 其它 | unsafe { DEMAND_FAULT_PROBE_EL0 = Some(f) }; } /// 取已注册的按需调页探针（` |
| arch/aarch64/src/lib.rs | 274 | fn | 其它 | unsafe fn demand_fault_probe(esr: usize, far: usize) -> bool { / |
| arch/aarch64/src/lib.rs | 277 | block | 其它 | unsafe { f(esr, far) }, None => false, } } /// 取 EL0 臂探针（`except |
| arch/aarch64/src/lib.rs | 287 | fn | 其它 | unsafe fn demand_fault_probe_el0(esr: usize, far: usize) -> bool |
| arch/aarch64/src/lib.rs | 290 | block | 其它 | unsafe { f(esr, far) }, None => false, } } // ---- G3-1b 落点①：sys |
| arch/aarch64/src/lib.rs | 309 | block | 其它 | unsafe { SVC_EXIT_CHECKPOINT = Some(f) }; } /// 解蔽 IRQ（`daifclr |
| arch/aarch64/src/lib.rs | 315 | block | MMIO | unsafe { core::arch::asm!("msr daifclr, #2", options(nostack, pr |
| arch/aarch64/src/lib.rs | 323 | block | MMIO | unsafe { core::arch::asm!("mrs {}, daif", out(reg) daif, options |
| arch/aarch64/src/lib.rs | 331 | block | MMIO | unsafe { core::arch::asm!("msr daif, {}", in(reg) daif, options( |
| arch/aarch64/src/lib.rs | 337 | block | MMIO | unsafe { core::arch::asm!("msr daifset, #2", options(nostack, pr |
| arch/aarch64/src/lib.rs | 343 | block | MMIO | unsafe { core::arch::asm!("wfi", options(nostack, preserves_flag |
| arch/aarch64/src/lib.rs | 351 | block | MMIO | unsafe { core::arch::asm!("wfi") }; } } |
| arch/aarch64/src/lockdep.rs | 110 | block | 其它 | unsafe { *chain.add(i as usize) }; // ① 重入：地板上同类二取 = 非重入 TTAS 同实 |
| arch/aarch64/src/lockdep.rs | 138 | block | 其它 | unsafe { let (d, f) = (*depth, *floor); if let Some(kind) = chec |
| arch/aarch64/src/lockdep.rs | 156 | block | 其它 | unsafe { let (d, f) = (*depth, *floor); if let Some(kind) = chec |
| arch/aarch64/src/lockdep.rs | 173 | block | 其它 | unsafe { *chain.add(depth as usize - 1) } != class { return Some |
| arch/aarch64/src/lockdep.rs | 198 | block | 其它 | unsafe { *floor = *depth }; } /// IRQ 地板收回（`exception_irq` 出口）：断 |
| arch/aarch64/src/lockdep.rs | 210 | block | 其它 | unsafe { if let Some(kind) = check_irq_exit(*depth, *floor) { vi |
| arch/aarch64/src/lockdep.rs | 233 | block | 其它 | unsafe { if *depth != 0 \|\| *floor != 0 { return false; // 非净态——不 |
| arch/aarch64/src/lockdep.rs | 319 | block | MMIO | unsafe { for i in 0..depth { if i != 0 { crate::uart::uart_puts( |
| arch/aarch64/src/mmio.rs | 5 | fn | MMIO | unsafe fn write(addr: usize, val: u32) { (addr as *mut u32).writ |
| arch/aarch64/src/mmio.rs | 12 | fn | MMIO | unsafe fn read(addr: usize) -> u32 { (addr as *const u32).read_v |
| arch/aarch64/src/mmio.rs | 18 | fn | MMIO | unsafe fn write_u8(addr: usize, val: u8) { (addr as *mut u8).wri |
| arch/aarch64/src/mmio.rs | 24 | fn | MMIO | unsafe fn read_u8(addr: usize) -> u8 { (addr as *const u8).read_ |
| arch/aarch64/src/mmio.rs | 30 | fn | MMIO | unsafe fn write_u16(addr: usize, val: u16) { (addr as *mut u16). |
| arch/aarch64/src/mmio.rs | 36 | fn | MMIO | unsafe fn write_u64(addr: usize, val: u64) { (addr as *mut u64). |
| arch/aarch64/src/mmio.rs | 43 | fn | MMIO | unsafe fn read_u64(addr: usize) -> u64 { (addr as *const u64).re |
| arch/aarch64/src/mmu.rs | 88 | fn | 页表 | unsafe extern "C" fn build_boot_pt() { // `&raw mut`：取静态裸指针，避开 ` |
| arch/aarch64/src/mmu.rs | 142 | fn | 页表 | unsafe extern "C" fn enable_mmu() { write_mair_el1(MAIR_EL1); wr |
| arch/aarch64/src/mmu.rs | 169 | block | 页表 | unsafe { (va as *const u64).read_volatile() } == PROBE_MAGIC; co |
| arch/aarch64/src/mmu.rs | 172 | block | 页表 | unsafe { crate::mmio::read(KERNEL_VA_BASE + GICD_PIDR2) }; // PI |
| arch/aarch64/src/mmu.rs | 185 | block | 页表 | unsafe { asm!("mrs {0}, id_aa64mmfr0_el1", out(reg) v, options(n |
| arch/aarch64/src/mmu.rs | 201 | fn | 页表 | unsafe fn tlbi_aside1(asid: u16) { let shift = asid_shift(); asm |
| arch/aarch64/src/mmu.rs | 214 | fn | 页表 | unsafe fn activate_ttbr0(ttbr0_pa: usize, asid: u16) { let table |
| arch/aarch64/src/mmu.rs | 240 | block | 页表 | unsafe { asm!("mrs {0}, ttbr0_el1", out(reg) ttbr, options(nosta |
| arch/aarch64/src/mmu.rs | 250 | block | 页表 | unsafe { asm!("mrs {0}, ttbr0_el1", out(reg) ttbr, options(nosta |
| arch/aarch64/src/mmu.rs | 259 | fn | 页表 | unsafe fn retire_low_half() { if !RETIRE_IDENTITY { return; } ac |
| arch/aarch64/src/mmu.rs | 270 | fn | 页表 | unsafe fn flush_all_asid() { asm!("tlbi vmalle1is", options(nost |
| arch/aarch64/src/mmu.rs | 282 | fn | 页表 | unsafe fn flush_local_all() { asm!("tlbi vmalle1", options(nosta |
| arch/aarch64/src/mmu.rs | 290 | fn | 页表 | unsafe fn write_mair_el1(v: u64) { asm!("msr mair_el1, {0}", in( |
| arch/aarch64/src/mmu.rs | 294 | fn | 页表 | unsafe fn write_tcr_el1(v: u64) { asm!("msr tcr_el1, {0}", in(re |
| arch/aarch64/src/mmu.rs | 298 | fn | 页表 | unsafe fn write_ttbr0_el1(v: u64) { asm!("msr ttbr0_el1, {0}", i |
| arch/aarch64/src/mmu.rs | 302 | fn | 页表 | unsafe fn write_ttbr1_el1(v: u64) { asm!("msr ttbr1_el1, {0}", i |
| arch/aarch64/src/mmu.rs | 306 | fn | 页表 | unsafe fn write_sctlr_el1(v: u64) { asm!("msr sctlr_el1, {0}", i |
| arch/aarch64/src/mmu.rs | 310 | fn | 页表 | unsafe fn isb() { asm!("isb", options(nostack, preserves_flags)) |
| arch/aarch64/src/mmu.rs | 314 | fn | 页表 | unsafe fn dsb_ish() { asm!("dsb ish", options(nostack, preserves |
| arch/aarch64/src/pl061.rs | 64 | block | MMIO | unsafe { *core::ptr::addr_of_mut!(PL061_NOTIFY) = Some(f) }; } / |
| arch/aarch64/src/pl061.rs | 72 | block | MMIO | unsafe { if mmio::read(PL061 + GPIO_MIS) & PIN_MASK == 0 { retur |
| arch/aarch64/src/pl061.rs | 94 | fn | MMIO | unsafe fn pl061_init() { mmio::write(PL061 + GPIO_ICR, PIN_MASK) |
| arch/aarch64/src/psci.rs | 37 | block | 其它 | unsafe { (&raw const BOOT_EL).read_volatile() } } /// 模块本地 HVC 调 |
| arch/aarch64/src/psci.rs | 49 | block | MMIO | unsafe { asm!( "hvc #0", inout("x0") fnid => ret, inout("x1") x1 |
| arch/aarch64/src/smp.rs | 231 | block | MMIO | unsafe { asm!("mrs {}, mpidr_el1", out(reg) mpidr); let p: *mut |
| arch/aarch64/src/smp.rs | 248 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 254 | block | MMIO | unsafe { core::ptr::addr_of_mut!((*(base as *mut SmpCpu)).last_v |
| arch/aarch64/src/smp.rs | 264 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 270 | block | MMIO | unsafe { core::ptr::addr_of_mut!((*(base as *mut SmpCpu)).curren |
| arch/aarch64/src/smp.rs | 281 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 287 | block | MMIO | unsafe { &*(core::ptr::addr_of_mut!((*(base as *mut SmpCpu)).pre |
| arch/aarch64/src/smp.rs | 298 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 304 | block | MMIO | unsafe { &mut *(base as *mut SmpCpu) }; Some(( c.lockdep_chain.a |
| arch/aarch64/src/smp.rs | 317 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 322 | block | MMIO | unsafe { (&raw const (*(base as *mut SmpCpu)).cpu).read_volatile |
| arch/aarch64/src/smp.rs | 346 | block | MMIO | unsafe { &raw mut SMP_SEC_PARK_CTX[cpu_id()] } } /// 副核高半栈（S-2b |
| arch/aarch64/src/smp.rs | 405 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 411 | block | MMIO | unsafe { let my = base as *mut SmpCpu; if is_el0 { (*my).el0_tic |
| arch/aarch64/src/smp.rs | 434 | block | MMIO | unsafe { let c0 = (&raw const SMP_CPU0.el0_ticks).read_volatile( |
| arch/aarch64/src/smp.rs | 449 | block | MMIO | unsafe { let p0 = (&raw const SMP_CPU0.ticks_pre).read_volatile( |
| arch/aarch64/src/smp.rs | 471 | block | MMIO | unsafe { ((&raw const SMP_CPU1.ticks).read_volatile() & 3) == 0 |
| arch/aarch64/src/smp.rs | 479 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 484 | block | MMIO | unsafe { let my = base as *mut SmpCpu; (*my).ticks = (*my).ticks |
| arch/aarch64/src/smp.rs | 495 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 500 | block | MMIO | unsafe { let my = base as *mut SmpCpu; (*my).ticks_pre = (*my).t |
| arch/aarch64/src/smp.rs | 511 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 516 | block | MMIO | unsafe { let my = base as *mut SmpCpu; (*my).sw_cnt = (*my).sw_c |
| arch/aarch64/src/smp.rs | 527 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 532 | block | MMIO | unsafe { let my = base as *mut SmpCpu; (*my).park_cnt = (*my).pa |
| arch/aarch64/src/smp.rs | 544 | block | MMIO | unsafe { let s0 = (&raw const SMP_CPU0.sw_cnt).read_volatile(); |
| arch/aarch64/src/smp.rs | 558 | block | MMIO | unsafe { asm!("mrs {}, tpidr_el1", out(reg) base) }; if base == |
| arch/aarch64/src/smp.rs | 563 | block | MMIO | unsafe { let my = base as *mut SmpCpu; (*my).disp_cnt = (*my).di |
| arch/aarch64/src/smp.rs | 724 | block | MMIO | unsafe { (&raw const BOOT_EL).read_volatile() } } fn cntvct() -> |
| arch/aarch64/src/smp.rs | 730 | block | MMIO | unsafe { asm!("mrs {}, cntvct_el0", out(reg) v) }; v } fn cntfrq |
| arch/aarch64/src/smp.rs | 737 | block | MMIO | unsafe { asm!("mrs {}, cntfrq_el0", out(reg) v) }; v } /// 状态谓词轮 |
| arch/aarch64/src/smp.rs | 780 | block | MMIO | unsafe { (&raw const __smp_sec_entry).read_volatile() } as usize |
| arch/aarch64/src/smp.rs | 787 | block | MMIO | unsafe { asm!( "hvc #0", inout("x0") 0xC400_0003u64 => ret, // P |
| arch/aarch64/src/smp.rs | 813 | block | MMIO | unsafe { (&raw const SMP_SEC_REPORT).read_volatile() }; uart_put |
| arch/aarch64/src/smp.rs | 827 | block | MMIO | unsafe { (&raw const SMP_SEC_REPORT).read_volatile() }; uart_put |
| arch/aarch64/src/smp.rs | 857 | block | MMIO | unsafe { asm!("dsb sy", "sev") }; if !wait_secs(5, \|\| SMP_HB2.lo |
| arch/aarch64/src/smp.rs | 871 | block | MMIO | unsafe { crate::gic::send_sgi(crate::gic::SGI_RESCHEDULE, crate: |
| arch/aarch64/src/smp.rs | 891 | block | MMIO | unsafe { crate::gic::send_sgi(crate::gic::SGI_RESCHEDULE, crate: |
| arch/aarch64/src/smp.rs | 906 | block | MMIO | unsafe { crate::gic::send_sgi(crate::gic::SGI_RESCHEDULE, crate: |
| arch/aarch64/src/smp.rs | 928 | block | MMIO | unsafe { crate::gic::send_sgi(crate::gic::SGI_RESCHEDULE, crate: |
| arch/aarch64/src/syscall.rs | 45 | block | 其它 | unsafe { (*(&raw mut SYSCALLS))[nr] = (h, class) }; } /// 派发入口（` |
| arch/aarch64/src/syscall.rs | 61 | block | 其它 | unsafe { *(&raw const SYSCALLS).cast::<(Handler, u8)>().add(nr) |
| arch/aarch64/src/syscall.rs | 83 | block | 其它 | unsafe { crate::SVC_EXIT_CHECKPOINT } { checkpoint(); } tf.regs[ |
| arch/aarch64/src/task.rs | 277 | fn | 上下文切换 | unsafe fn context_switch(prev: *mut TaskContext, next: *const Ta |
| arch/aarch64/src/task.rs | 279 | block | 上下文切换 | unsafe { switch(prev, next) } } /// 任务入口误返回的兜底（asm `bl` 调）。正常任务 |
| arch/aarch64/src/timer.rs | 34 | fn | MMIO | unsafe fn init() -> u64 { FREQ = read_cntfrq(); SLICE = FREQ / S |
| arch/aarch64/src/timer.rs | 65 | fn | MMIO | unsafe fn arm_at(deadline: u64) { core::arch::asm!( "msr cntv_cv |
| arch/aarch64/src/timer.rs | 102 | fn | MMIO | unsafe fn arm_slice() { let now = cntvct(); let want = now + SLI |
| arch/aarch64/src/timer.rs | 110 | block | MMIO | unsafe { arm_at(want) }; } /// 屏蔽 CNTV 中断（IMASK=1，ENABLE 保持）——ti |
| arch/aarch64/src/timer.rs | 117 | fn | MMIO | unsafe fn mask() { let ctl: u64 = 0b11; // ENABLE=1, IMASK=1 cor |
| arch/aarch64/src/timer.rs | 132 | fn | MMIO | unsafe fn disable() { let ctl: u64 = 0b00; core::arch::asm!( "ms |
| arch/aarch64/src/timer.rs | 151 | block | MMIO | unsafe { arm_slice() } } /// 读虚拟计数器 `CNTVCT_EL0`（周期数）。调度器时间片按**周 |
| arch/aarch64/src/timer.rs | 160 | block | MMIO | unsafe { core::arch::asm!("mrs {0}, cntvct_el0", out(reg) v, opt |
| arch/aarch64/src/timer.rs | 169 | block | MMIO | unsafe { core::arch::asm!("mrs {0}, cntv_cval_el0", out(reg) v, |
| arch/aarch64/src/timer.rs | 179 | block | MMIO | unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, opt |
| arch/aarch64/src/timer.rs | 187 | block | MMIO | unsafe { SLICE } } fn read_cntfrq() -> u64 { let v: u64; unsafe |
| arch/aarch64/src/timer.rs | 192 | block | MMIO | unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, opt |
| arch/aarch64/src/tlbshoot.rs | 88 | block | 页表 | unsafe { crate::mmu::flush_local_all() }; let me = crate::smp::c |
| arch/aarch64/src/tlbshoot.rs | 98 | block | MMIO | unsafe { crate::gic::send_sgi(crate::gic::SGI_TLB_SHOOTDOWN, cra |
| arch/aarch64/src/trap.rs | 319 | fn | 上下文切换 | unsafe fn init() { extern "C" { static __vectors: u8; } let vbar |
| arch/aarch64/src/trap.rs | 355 | block | 上下文切换 | unsafe { crate::demand_fault_probe(tf.esr as usize, tf.far as us |
| arch/aarch64/src/trap.rs | 427 | block | 上下文切换 | unsafe { crate::demand_fault_probe_el0(tf.esr as usize, tf.far a |
| arch/aarch64/src/trap.rs | 445 | block | 上下文切换 | unsafe { crate::user_fault_exit() } { // SAFETY: f 是 C-ABI `-> ! |
| arch/aarch64/src/trap.rs | 447 | block | 上下文切换 | unsafe { f() } } halt(); } /// dump 一个 TrapFrame（EC 名 + 寄存器全集）。 |
| arch/aarch64/src/trap.rs | 491 | block | 上下文切换 | unsafe { read_esr_elr() }; uart_puts(b"\n=== EXCEPTION: unexpect |
| arch/aarch64/src/trap.rs | 566 | block | 上下文切换 | unsafe { crate::irq_dispatch() } { // SAFETY: f 是 C-ABI fn 指针；tf |
| arch/aarch64/src/trap.rs | 568 | block | 上下文切换 | unsafe { f(tf) }; } } crate::gic::IRQ_PL011_RX => { // 排空 FIFO 到 |
| arch/aarch64/src/trap.rs | 587 | block | 上下文切换 | unsafe { crate::SVC_EXIT_CHECKPOINT } { checkpoint(); } } } crat |
| arch/aarch64/src/trap.rs | 602 | block | 上下文切换 | unsafe { crate::SVC_EXIT_CHECKPOINT } { checkpoint(); } } } crat |
| arch/aarch64/src/trap.rs | 627 | block | 上下文切换 | unsafe { crate::sgi_dispatch() } { // SAFETY: f 是 C-ABI fn 指针；tf |
| arch/aarch64/src/trap.rs | 629 | block | 上下文切换 | unsafe { f(tf) }; } } SGI_TLB_US => { // S-4 4-1：TLB shootdown 收 |
| arch/aarch64/src/trap.rs | 676 | fn | 上下文切换 | unsafe fn read_esr_elr() -> (u64, u64) { let esr: u64; let elr: |
| arch/aarch64/src/uart.rs | 70 | block | MMIO | unsafe { mmio::write(UART, c as u32) }; } /// 输出一段字节串。 pub fn ua |
| arch/aarch64/src/uart.rs | 142 | block | MMIO | unsafe { *core::ptr::addr_of_mut!(RX_NOTIFY) = Some(f) }; } /// |
| arch/aarch64/src/uart.rs | 148 | block | MMIO | unsafe { mmio::read(UART + UART_FR) & UART_FR_RXFE == 0 } } /// |
| arch/aarch64/src/uart.rs | 154 | block | MMIO | unsafe { mmio::read(UART + UART_DR) as u8 } } /// 环满丢字节数（console |
| arch/aarch64/src/uart.rs | 160 | block | MMIO | unsafe { (*core::ptr::addr_of!(RX_RING)).lost } } /// RX IRQ han |
| arch/aarch64/src/uart.rs | 171 | block | MMIO | unsafe { while mmio::read(UART + UART_FR) & UART_FR_RXFE == 0 { |
| arch/aarch64/src/uart.rs | 202 | block | MMIO | unsafe { let r = &mut *core::ptr::addr_of_mut!(RX_RING); let mut |
| arch/aarch64/src/uart.rs | 221 | fn | MMIO | unsafe fn rx_init() { // 同掩 RXIC（UART_ICR_STICKY）：boot 期已落 FIFO |
| arch/aarch64/src/uart.rs | 265 | block | MMIO | unsafe { *core::ptr::addr_of_mut!(UART1_NOTIFY) = Some(f) }; } / |
| arch/aarch64/src/uart.rs | 271 | block | MMIO | unsafe { (*core::ptr::addr_of!(UART1_RING)).lost } } /// UART1 R |
| arch/aarch64/src/uart.rs | 280 | block | MMIO | unsafe { while mmio::read(UART1 + UART_FR) & UART_FR_RXFE == 0 { |
| arch/aarch64/src/uart.rs | 305 | block | MMIO | unsafe { let r = &mut *core::ptr::addr_of_mut!(UART1_RING); let |
| arch/aarch64/src/uart.rs | 324 | block | MMIO | unsafe { let mut spin: u32 = 0; while mmio::read(UART1 + UART_FR |
| arch/aarch64/src/uart.rs | 349 | fn | MMIO | unsafe fn uart1_init() { mmio::write(UART1 + UART_ICR, UART_ICR_ |
| arch/x86_64/src/main.rs | 393 | fn | MMIO | unsafe fn outb(port: u16, val: u8) { asm!("out %al, %dx", in("al |
| arch/x86_64/src/main.rs | 397 | fn | MMIO | unsafe fn inb(port: u16) -> u8 { let ret: u8; asm!("in %dx, %al" |
| arch/x86_64/src/main.rs | 404 | block | MMIO | unsafe { outb(COM1 + 1, 0x00); // 关串口中断（串口无 IRQ 面：IRQ4 双重掩闭——设备级 |
| arch/x86_64/src/main.rs | 416 | block | MMIO | unsafe { // LSR bit5 = THR 空；QEMU model 下首写即就绪，轮询为真机保真 while inb |
| arch/x86_64/src/main.rs | 468 | block | MMIO | unsafe { asm!("hlt", options(att_syntax, nomem, nostack, preserv |
| arch/x86_64/src/main.rs | 495 | block | MMIO | unsafe { inb(0x80) }; } fn pic_init() { unsafe { // ICW1：INIT + |
| arch/x86_64/src/main.rs | 499 | block | 其它 | unsafe { // ICW1：INIT + 级联 + ICW4。注意：init 序列期间 IMR 被清零（全部 IRQ // |
| arch/x86_64/src/main.rs | 525 | block | MMIO | unsafe { outb(PIC1_CMD, 0x20) }; // 非特定 EOI } fn pit_init() { // |
| arch/x86_64/src/main.rs | 531 | block | MMIO | unsafe { outb(PIT_CMD, 0x36); outb(PIT_CH0, (div & 0xFF) as u8); |
| arch/x86_64/src/main.rs | 631 | block | 上下文切换 | unsafe { let idt: *mut GateEntry = core::ptr::addr_of_mut!(IDT). |
| arch/x86_64/src/main.rs | 656 | block | MMIO | unsafe { asm!("sti", options(att_syntax, nomem, nostack)) }; } / |
| arch/x86_64/src/main.rs | 706 | block | MMIO | unsafe { asm!("mov %cr2, {}", out(reg) v, options(att_syntax, no |
| arch/x86_64/src/main.rs | 774 | block | MMIO | unsafe { asm!("cli", options(att_syntax, nomem, nostack)) }; par |
| arch/x86_64/src/main.rs | 978 | extern | 上下文切换 | unsafe extern "C" { /// # Safety /// 切走当前执行流（换栈 + 换 callee-saved |
| arch/x86_64/src/main.rs | 1025 | block | 其它 | unsafe { let top = stack as usize + core::mem::size_of::<TaskSta |
| arch/x86_64/src/main.rs | 1043 | block | 上下文切换 | unsafe { context_switch(core::ptr::addr_of_mut!(CTX_A), core::pt |
| arch/x86_64/src/main.rs | 1047 | block | 上下文切换 | unsafe { // 收官切回 main——本任务自此悬停在该 context_switch 内（现场已存 // CTX_A， |
| arch/x86_64/src/main.rs | 1062 | block | 上下文切换 | unsafe { context_switch(core::ptr::addr_of_mut!(CTX_B), core::pt |
| arch/x86_64/src/main.rs | 1082 | fn | MMIO | unsafe fn rdmsr(msr: u32) -> u64 { let lo: u32; let hi: u32; uns |
| arch/x86_64/src/main.rs | 1085 | block | MMIO | unsafe { asm!( "rdmsr", in("ecx") msr, out("eax") lo, out("edx") |
| arch/x86_64/src/main.rs | 1097 | fn | MMIO | unsafe fn wrmsr(msr: u32, val: u64) { unsafe { asm!( "wrmsr", in |
| arch/x86_64/src/main.rs | 1098 | block | MMIO | unsafe { asm!( "wrmsr", in("ecx") msr, in("eax") (val & 0xFFFF_F |
| arch/x86_64/src/main.rs | 1145 | block | 上下文切换 | unsafe { let tss = core::ptr::addr_of_mut!(TSS) as usize as u64; |
| arch/x86_64/src/main.rs | 1208 | block | 上下文切换 | unsafe { // EFER：boot 只设过 LME——运行期增量置 SCE，不整写覆写。 wrmsr(MSR_EFER, |
| arch/x86_64/src/main.rs | 1243 | block | 其它 | unsafe { core::ptr::addr_of!(pvh_start_info).read_volatile() } a |
| arch/x86_64/src/main.rs | 1244 | block | 其它 | unsafe { core::ptr::read_volatile(si as *const u32) }; let ver = |
| arch/x86_64/src/main.rs | 1245 | block | 其它 | unsafe { core::ptr::read_volatile((si + 4) as *const u32) }; ser |
| arch/x86_64/src/main.rs | 1292 | block | 其它 | unsafe { asm!("ud2", inlateout("rax") canary, options(att_syntax |
| arch/x86_64/src/main.rs | 1306 | block | 上下文切换 | unsafe { ctx_init(core::ptr::addr_of_mut!(CTX_A), core::ptr::add |
| arch/x86_64/src/main.rs | 1328 | block | 其它 | unsafe { let src = core::ptr::addr_of!(user_prog_start) as *cons |
| kernel/src/aspace.rs | 54 | block | 页表 | unsafe { let slot = &raw mut ASIDS; if (*slot).is_none() { *slot |
| kernel/src/aspace.rs | 81 | block | 页表 | unsafe { let slot = &raw mut ASIDS; if (*slot).is_none() { *slot |
| kernel/src/aspace.rs | 129 | block | 页表 | unsafe { (USER_VA as *const u64).read_volatile() } } /// 经当前 TTB |
| kernel/src/aspace.rs | 135 | block | 页表 | unsafe { (USER_VA as *mut u64).write_volatile(v) }; } } |
| kernel/src/bench.rs | 164 | block | 其它 | unsafe { &*core::ptr::addr_of!(SRC) }; let dst: &mut [u8] = unsa |
| kernel/src/bench.rs | 165 | block | 其它 | unsafe { &mut *core::ptr::addr_of_mut!(DST) }; let mut vmo = Vmo |
| kernel/src/bench.rs | 227 | block | 其它 | unsafe { &*core::ptr::addr_of!(BUF) }; let base = mm::frames().a |
| kernel/src/bench.rs | 300 | block | 其它 | unsafe { XH_C0 = 0; XH_C1 = 0; } assert_eq!( svc4( NR_CHANNEL_CR |
| kernel/src/bench.rs | 311 | block | 其它 | unsafe { XH_C0 } as u32, unsafe { XH_C1 } as u32); assert_ne!(hc |
| kernel/src/bench.rs | 311 | block | 其它 | unsafe { XH_C1 } as u32); assert_ne!(hc0, 0, "bench: xh 预热端点 0 须 |
| kernel/src/bench.rs | 322 | block | 其它 | unsafe { &mut *core::ptr::addr_of_mut!(XH_HS) }; for h in hs.ite |
| kernel/src/bench.rs | 446 | block | 其它 | unsafe { XH_VOUT = 0; } assert_eq!( svc6( NR_VMO_CREATE as u64, |
| kernel/src/bench.rs | 460 | block | 其它 | unsafe { XH_VOUT } as u32; assert_ne!(hv, 0, "bench: xh vmo 帽边界 |
| kernel/src/bench.rs | 544 | block | 其它 | unsafe { XH_NB = XH_MSG; XH_NH = 4; } assert_eq!( svc6( NR_CHANN |
| kernel/src/bench.rs | 561 | block | 其它 | unsafe { XH_NB }, XH_MSG, "bench: xh recv 字节数回写"); assert_eq!(un |
| kernel/src/bench.rs | 562 | block | 其它 | unsafe { XH_RBUF[0] }, 0x58, "bench: xh recv 载荷首字节"); assert_eq! |
| kernel/src/bench.rs | 563 | block | 其它 | unsafe { XH_NH }, 0, "bench: xh recv handle 数回写"); } // 恢复面：排空后再 |
| kernel/src/bench.rs | 581 | block | 其它 | unsafe { XH_NB = XH_MSG; XH_NH = 4; } assert_eq!( svc6( NR_CHANN |
| kernel/src/bench.rs | 611 | block | 其它 | unsafe { XH_NB = XH_MSG; XH_NH = 4; } assert_eq!( svc6( NR_CHANN |
| kernel/src/channel.rs | 150 | block | 其它 | unsafe { (*owner.as_ptr()).retain() }; true } /// AM-1.5：清由 `own |
| kernel/src/channel.rs | 165 | block | 其它 | unsafe { ObjRef::release(owner) }; true } /// opt-out 标志（recv 结算 |
| kernel/src/channel.rs | 179 | fn | 其它 | unsafe fn pair(a: NonNull<ObjRef>, b: NonNull<ObjRef>) { // SAFE |
| kernel/src/channel.rs | 182 | block | 其它 | unsafe { (*a.as_ptr()).obj.as_channel_mut().unwrap().peer = Some |
| kernel/src/channel.rs | 235 | block | 其它 | unsafe { ObjRef::release(peer) }; } // AM-1.5 冻结属主标配平：正常序 thaw 清 |
| kernel/src/channel.rs | 241 | block | 其它 | unsafe { ObjRef::release(owner) }; } } } |
| kernel/src/dpg.rs | 131 | fn | 其它 | unsafe extern "C" fn probe(esr: usize, far: usize) -> bool { // |
| kernel/src/dpg.rs | 153 | block | 其它 | unsafe { (*root).demand_region_at(far).is_none() } { hal::kern_l |
| kernel/src/dpg.rs | 234 | fn | 其它 | unsafe extern "C" fn probe_el0(esr: usize, far: usize) -> bool { |
| kernel/src/dpg.rs | 252 | block | 其它 | unsafe { (*root).demand_region_at(far).is_none() } { hal::kern_l |
| kernel/src/dpg.rs | 373 | block | 其它 | unsafe { (*root).demand_region_at(far) } { Some((rvmo, _, _)) => |
| kernel/src/dpg.rs | 450 | block | 其它 | unsafe { &mut *core::ptr::addr_of_mut!(SUPPLY_BUF) } } /// 测试 pa |
| kernel/src/dpg.rs | 480 | block | 其它 | unsafe { (*root).demand_region_at(far) } .expect("dpg_pager: 请求 |
| kernel/src/dpg.rs | 483 | block | 其它 | unsafe { (*root).supply_page(mm::frames(), va) } .expect("dpg_pa |
| kernel/src/dpg.rs | 491 | block | 其它 | unsafe { (*vmo).write(off, &DPG_MAGIC.to_le_bytes()) } .expect(" |
| kernel/src/dpg.rs | 504 | block | 其它 | unsafe { core::ptr::read_volatile(DEMAND_VA as *const u64) }; as |
| kernel/src/dpg.rs | 629 | block | MMIO | unsafe { core::ptr::read_volatile(va as *const u64) }; let t1 = |
| kernel/src/dtb.rs | 43 | block | 其它 | unsafe { core::slice::from_raw_parts(phys_to_virt(dtb_pa) as *co |
| kernel/src/dtb.rs | 48 | block | 其它 | unsafe { core::slice::from_raw_parts(phys_to_virt(dtb_pa) as *co |
| kernel/src/handle.rs | 199 | fn | 其它 | unsafe fn release(ptr: NonNull<Self>) { // SAFETY: 调用方契约（retain |
| kernel/src/handle.rs | 244 | block | 其它 | unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) }; self.sl |
| kernel/src/handle.rs | 262 | block | 其它 | unsafe { (*obj.as_ptr()).refs += 1 }; self.slots[slot] = SlotSta |
| kernel/src/handle.rs | 283 | block | 其它 | unsafe { let r = &*obj.as_ptr(); if r.refs == 1 { if let KernelO |
| kernel/src/handle.rs | 294 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); r.refs -= 1; r.refs }; if r |
| kernel/src/handle.rs | 301 | block | 其它 | unsafe { drop(Box::from_raw(obj.as_ptr())) }; } self.slots[slot] |
| kernel/src/handle.rs | 322 | block | 其它 | unsafe { let r = &*hobj.as_ptr(); if r.refs == 1 { if let Kernel |
| kernel/src/handle.rs | 383 | block | 其它 | unsafe { let r = &*obj.as_ptr(); r.refs == 1 && match &r.obj { K |
| kernel/src/handle.rs | 393 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); r.refs -= 1; r.refs }; |
| kernel/src/handle.rs | 400 | block | 其它 | unsafe { drop(Box::from_raw(obj.as_ptr())) }; } } *slot = SlotSt |
| kernel/src/handle.rs | 418 | block | 其它 | unsafe { &*obj.as_ptr() }.obj, rights)) } /// 1.1/S3-b：本表在册 page |
| kernel/src/handle.rs | 431 | block | 其它 | unsafe { (*obj.as_ptr()).obj.as_vmo().map_or(false, \|v\| v.is_dem |
| kernel/src/handle.rs | 466 | block | 其它 | unsafe { if let Some(end) = (*obj.as_ptr()).obj.as_channel_mut() |
| kernel/src/handle.rs | 487 | block | 其它 | unsafe { if let Some(end) = (*obj.as_ptr()).obj.as_channel_mut() |
| kernel/src/handle.rs | 544 | block | 其它 | unsafe { (*obj.as_ptr()).refs } == 0 { return Err("HT integrity: |
| kernel/src/handle.rs | 567 | block | 其它 | unsafe { &*obj.as_ptr() }; if r.obj.obj_type() != ty { return Er |
| kernel/src/handle.rs | 606 | block | 其它 | unsafe { (*obj.as_ptr()).obj.obj_type() == ty } { n += 1; } } } |
| kernel/src/handoff.rs | 166 | block | 其它 | unsafe { &*(phys_to_virt(x0) as *const BerryHandoff) }; if berry |
| kernel/src/handoff.rs | 221 | block | 其它 | unsafe { core::slice::from_raw_parts(base, n) }; // 段几何门（含切片长度一致 |
| kernel/src/heap.rs | 47 | impl | 分配器 | unsafe impl Sync for SlabHeap {} unsafe impl GlobalAlloc for Sla |
| kernel/src/heap.rs | 49 | impl | 分配器 | unsafe impl GlobalAlloc for SlabHeap { unsafe fn alloc(&self, la |
| kernel/src/heap.rs | 50 | fn | 分配器 | unsafe fn alloc(&self, layout: Layout) -> *mut u8 { let eff = co |
| kernel/src/heap.rs | 84 | fn | 分配器 | unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { let eff |
| kernel/src/heap.rs | 114 | block | 分配器 | unsafe { alloc::alloc::alloc(layout) } as *mut T; if ptr.is_null |
| kernel/src/heap.rs | 119 | block | 分配器 | unsafe { ptr.write(val) }; // SAFETY: 该块按全局分配器 + for_value 布局分配， |
| kernel/src/heap.rs | 122 | block | 分配器 | unsafe { Box::from_raw(ptr) }) } // ---- freelist（侵入式：槽首存 next 指 |
| kernel/src/heap.rs | 127 | fn | 分配器 | unsafe fn pop(fl: &mut [*mut u8; SLAB_CLASSES.len()], ci: usize) |
| kernel/src/heap.rs | 136 | fn | 分配器 | unsafe fn push(fl: &mut [*mut u8; SLAB_CLASSES.len()], ci: usize |
| kernel/src/ktest.rs | 162 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_CLOCK_READ as u64, 0, &r |
| kernel/src/ktest.rs | 168 | block | 其它 | unsafe { OUT }; assert!(t1 > 0, "SYSCALL：clock_read 应得正纳秒"); ass |
| kernel/src/ktest.rs | 176 | block | 其它 | unsafe { OUT } >= t1, "SYSCALL：单调钟不单调"); // D. handle 门控全链路（EL1 |
| kernel/src/ktest.rs | 193 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 199 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hd, 0, "SYSCALL：duplicate 须回填新 |
| kernel/src/ktest.rs | 240 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc6(NR_VMO_CREATE as u64, PAGE_ |
| kernel/src/ktest.rs | 246 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hdm, 0, "SYSCALL：DEMAND VMO 须回 |
| kernel/src/ktest.rs | 253 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_S |
| kernel/src/ktest.rs | 256 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hv, 0, "SYSCALL：vmo_create 须回填 |
| kernel/src/ktest.rs | 261 | block | 其它 | unsafe { EBUF[..8].copy_from_slice(&PATTERN.to_le_bytes()) }; as |
| kernel/src/ktest.rs | 268 | block | 其它 | unsafe { EBUF = [0; 16] }; assert_eq!( svc6(NR_VMO_READ as u64, |
| kernel/src/ktest.rs | 275 | block | 其它 | unsafe { u64::from_le_bytes(EBUF[..8].try_into().unwrap()) }, PA |
| kernel/src/ktest.rs | 282 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_S |
| kernel/src/ktest.rs | 284 | block | 其它 | unsafe { OUT as u32 } }; assert_eq!( svc6(NR_VMO_READ as u64, hv |
| kernel/src/ktest.rs | 294 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 300 | block | 其它 | unsafe { OUT } as u32; assert_eq!( svc6(NR_VMO_WRITE as u64, hro |
| kernel/src/ktest.rs | 322 | block | 其它 | unsafe { OUT = crate::aspace::USER_VA as u64 }; // 请求 VA = 0x100 |
| kernel/src/ktest.rs | 329 | block | 其它 | unsafe { OUT } as usize, crate::aspace::USER_VA, "SYSCALL：vmar_m |
| kernel/src/ktest.rs | 365 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc6(NR_VMO_CREATE as u64, PAGE_ |
| kernel/src/ktest.rs | 371 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上（请求 VA 复用 0x1000——E3 已 unmap |
| kernel/src/ktest.rs | 373 | block | 其它 | unsafe { OUT = crate::aspace::USER_VA as u64 }; assert_eq!( svc6 |
| kernel/src/ktest.rs | 394 | block | 其它 | unsafe { OUT = 0x20_0000 }; // 请求子区间基 0x20_0000 static mut CHILD |
| kernel/src/ktest.rs | 397 | block | 其它 | unsafe { CHILD_OUT = 0 }; assert_eq!( svc6(NR_VMAR_ALLOCATE as u |
| kernel/src/ktest.rs | 404 | block | 其它 | unsafe { CHILD_OUT } as u32; assert_ne!(hchild, 0, "SYSCALL：allo |
| kernel/src/ktest.rs | 438 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(p) }); assert!(crate::vmar: |
| kernel/src/ktest.rs | 466 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(p2) }); assert_eq!(crate::v |
| kernel/src/ktest.rs | 470 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(p1) }); assert!(crate::vmar |
| kernel/src/ktest.rs | 487 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(p) }); break; // 帽到（该 v 未登记 |
| kernel/src/ktest.rs | 506 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(p_chk) }); // SAFETY: live |
| kernel/src/ktest.rs | 510 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(p) }); } let asid18 = crate |
| kernel/src/ktest.rs | 522 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(p18) }); // E3d. EL0 槽池属主过滤 |
| kernel/src/ktest.rs | 613 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(root_a as *mut Vmar) }); dr |
| kernel/src/ktest.rs | 614 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(root_b as *mut Vmar) }); dr |
| kernel/src/ktest.rs | 615 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(vmo_a as *mut Vmo) }); drop |
| kernel/src/ktest.rs | 616 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(vmo_b as *mut Vmo) }); drop |
| kernel/src/ktest.rs | 617 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(vmo_c as *mut Vmo) }); } // |
| kernel/src/ktest.rs | 675 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(root_e as *mut Vmar) }); dr |
| kernel/src/ktest.rs | 676 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(vmo_e as *mut Vmo) }); } // |
| kernel/src/ktest.rs | 744 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(root_f as *mut Vmar) }); dr |
| kernel/src/ktest.rs | 745 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(vmo_f as *mut Vmo) }); } // |
| kernel/src/ktest.rs | 780 | block | 其它 | unsafe { TBL = [PagerWindow { dv: 0, sv: 0, delta: 0 }; 2] }; as |
| kernel/src/ktest.rs | 800 | block | 其它 | unsafe { TBL[0].delta = 8 }; assert_eq!( svc6(NR_PAGER_SUPPLY_MA |
| kernel/src/ktest.rs | 830 | block | 其它 | unsafe { (*og.as_ptr()).obj.as_vmo_mut().unwrap() as *mut Vmo }; |
| kernel/src/ktest.rs | 831 | block | 其它 | unsafe { (*oh.as_ptr()).obj.as_vmo_mut().unwrap() as *mut Vmo }; |
| kernel/src/ktest.rs | 845 | block | 其它 | unsafe { root_g .map_demand(&mut *ptr_g, VA_G, 0, PAGE_SIZE, cra |
| kernel/src/ktest.rs | 851 | block | 其它 | unsafe { root_h .map_demand(&mut *ptr_h, VA_H, 0, 2 * PAGE_SIZE, |
| kernel/src/ktest.rs | 864 | block | 其它 | unsafe { TBL = [ PagerWindow { dv: hg, sv: 0, delta: 0 }, PagerW |
| kernel/src/ktest.rs | 882 | block | 其它 | unsafe { TBL = [ PagerWindow { dv: hh, sv: 0, delta: 0 }, PagerW |
| kernel/src/ktest.rs | 899 | block | 其它 | unsafe { TBL[0] = PagerWindow { dv: hg, sv: 0, delta: 0 } }; ass |
| kernel/src/ktest.rs | 927 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(root_g as *mut Vmar) }); dr |
| kernel/src/ktest.rs | 928 | block | 其它 | unsafe { alloc::boxed::Box::from_raw(root_h as *mut Vmar) }); as |
| kernel/src/ktest.rs | 939 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_GET_SIZE as u64, 0xDE |
| kernel/src/ktest.rs | 949 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_CREATE as u64, 0x1800 |
| kernel/src/ktest.rs | 952 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上（预置哨兵值，验回写真发生）。 unsafe { OUT |
| kernel/src/ktest.rs | 954 | block | 其它 | unsafe { OUT = u64::MAX }; assert_eq!(svc6(NR_VMO_GET_SIZE as u6 |
| kernel/src/ktest.rs | 957 | block | 其它 | unsafe { OUT }, PAGE_SIZE as u64 * 2, "SYSCALL：get_size 须回页对齐容量（ |
| kernel/src/ktest.rs | 961 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 968 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上。 unsafe { OUT = u64::MAX }; |
| kernel/src/ktest.rs | 970 | block | 其它 | unsafe { OUT = u64::MAX }; assert_eq!(svc6(NR_VMO_GET_SIZE as u6 |
| kernel/src/ktest.rs | 973 | block | 其它 | unsafe { OUT }, PAGE_SIZE as u64 * 2, "SYSCALL：零 READ 别名 size 同值 |
| kernel/src/ktest.rs | 1059 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc6(NR_VMO_GET_SIZE as u64, hmm |
| kernel/src/ktest.rs | 1066 | block | 其它 | unsafe { OUT }, berry_hal_aarch64::VIRTIO_MMIO_LEN as u64, "MMIO |
| kernel/src/ktest.rs | 1086 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 1092 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上。 unsafe { OUT = crate::aspa |
| kernel/src/ktest.rs | 1094 | block | 其它 | unsafe { OUT = crate::aspace::USER_VA as u64 }; assert_eq!( svc6 |
| kernel/src/ktest.rs | 1104 | block | 其它 | unsafe { OUT = crate::aspace::USER_VA as u64 }; assert_eq!( svc6 |
| kernel/src/ktest.rs | 1111 | block | 其它 | unsafe { OUT }, crate::aspace::USER_VA as u64, "MMIO：map 回写基址不变" |
| kernel/src/ktest.rs | 1133 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_GET_PHYS as u64, 0xDE |
| kernel/src/ktest.rs | 1144 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_S |
| kernel/src/ktest.rs | 1147 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上。 unsafe { OUT = u64::MAX }; |
| kernel/src/ktest.rs | 1149 | block | 其它 | unsafe { OUT = u64::MAX }; assert_eq!(svc6(NR_VMO_GET_PHYS as u6 |
| kernel/src/ktest.rs | 1157 | block | 其它 | unsafe { OUT = u64::MAX }; assert_eq!(svc6(NR_VMO_GET_PHYS as u6 |
| kernel/src/ktest.rs | 1160 | block | 其它 | unsafe { OUT }; assert!((0x4000_0000..berry_hal_aarch64::RAM_TOP |
| kernel/src/ktest.rs | 1165 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hgp |
| kernel/src/ktest.rs | 1168 | block | 其它 | unsafe { OUT }, pa0 + 0x123, "SYSCALL：get_phys 字节语义（页基 + 页内偏移）") |
| kernel/src/ktest.rs | 1176 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 1183 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上。 unsafe { OUT = 0 }; assert |
| kernel/src/ktest.rs | 1185 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hgo |
| kernel/src/ktest.rs | 1188 | block | 其它 | unsafe { OUT }, pa0 + 0x123, "SYSCALL：零 READ 别名 phys 同值"); asser |
| kernel/src/ktest.rs | 1198 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc5(NR_PROCESS_C |
| kernel/src/ktest.rs | 1204 | block | 其它 | unsafe { OUT } as u32; let hv2 = unsafe { CHILD_OUT } as u32; as |
| kernel/src/ktest.rs | 1205 | block | 其它 | unsafe { CHILD_OUT } as u32; assert_ne!(hp, 0, "SYSCALL：process_ |
| kernel/src/ktest.rs | 1226 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc5(NR_THREAD_CREATE as u64, hp |
| kernel/src/ktest.rs | 1232 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hth, 0, "SYSCALL：thread_create |
| kernel/src/ktest.rs | 1294 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4( NR_HANDLE_DUPLICATE as u64 |
| kernel/src/ktest.rs | 1305 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hthr, 0, "SYSCALL：dup 别名须回填"); |
| kernel/src/ktest.rs | 1322 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc5(NR_PROCESS_C |
| kernel/src/ktest.rs | 1328 | block | 其它 | unsafe { OUT } as u32; let hv3 = unsafe { CHILD_OUT } as u32; as |
| kernel/src/ktest.rs | 1329 | block | 其它 | unsafe { CHILD_OUT } as u32; assert_ne!(hp2, 0, "SYSCALL：第二个进程 h |
| kernel/src/ktest.rs | 1343 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(crate::syscall::NR_OBJECT |
| kernel/src/ktest.rs | 1349 | block | 其它 | unsafe { I_OBS }, 0, "SYSCALL：未命中 observed 须保持 0"); // F4c. proc |
| kernel/src/ktest.rs | 1358 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(crate::syscall::NR_PROCESS_ |
| kernel/src/ktest.rs | 1374 | block | 其它 | unsafe { OUT }, 0, "SYSCALL：未退 code 出参须保持 0"); // F4d. process_t |
| kernel/src/ktest.rs | 1397 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4( NR_HANDLE_DUPLICATE as u64 |
| kernel/src/ktest.rs | 1408 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hp2r, 0, "SYSCALL：dup 别名须回填"); |
| kernel/src/ktest.rs | 1424 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5( crate::syscall::NR_OBJEC |
| kernel/src/ktest.rs | 1437 | block | 其它 | unsafe { I_OBS }, crate::handle::SIGNAL_TERMINATED, "SYSCALL：obs |
| kernel/src/ktest.rs | 1443 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(crate::syscall::NR_PROCESS_ |
| kernel/src/ktest.rs | 1449 | block | 其它 | unsafe { OUT } as u32, 42, "SYSCALL：terminate code=42 须落退出码"); a |
| kernel/src/ktest.rs | 1504 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc5(NR_PROCESS_C |
| kernel/src/ktest.rs | 1510 | block | 其它 | unsafe { OUT } as u32; let hv4 = unsafe { CHILD_OUT } as u32; le |
| kernel/src/ktest.rs | 1511 | block | 其它 | unsafe { CHILD_OUT } as u32; let (pobj, _) = ht.get(hp3, ObjType |
| kernel/src/ktest.rs | 1535 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc4(NR_CHANNEL_C |
| kernel/src/ktest.rs | 1541 | block | 其它 | unsafe { OUT } as u32, unsafe { CHILD_OUT } as u32); assert_ne!( |
| kernel/src/ktest.rs | 1541 | block | 其它 | unsafe { CHILD_OUT } as u32); assert_ne!(hc0, 0, "SYSCALL：channe |
| kernel/src/ktest.rs | 1552 | block | 其它 | unsafe { H_NB = 8; H_NH = 4 }; assert_eq!( svc6(NR_CHANNEL_RECV |
| kernel/src/ktest.rs | 1563 | block | 其它 | unsafe { HBUF[..8].copy_from_slice(&PATTERN.to_le_bytes()) }; as |
| kernel/src/ktest.rs | 1570 | block | 其它 | unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 8; H_HND = [0; 8] }; |
| kernel/src/ktest.rs | 1578 | block | 其它 | unsafe { u64::from_le_bytes(HBUF[..8].try_into().unwrap()) }, PA |
| kernel/src/ktest.rs | 1580 | block | 其它 | unsafe { H_NB }, 8, "SYSCALL：recv 须回写实际字节数"); assert_eq!(unsafe |
| kernel/src/ktest.rs | 1581 | block | 其它 | unsafe { H_NH }, 0, "SYSCALL：recv 须回写实际 handle 数"); // SAFETY: 同 |
| kernel/src/ktest.rs | 1583 | block | 其它 | unsafe { H_NB = 8; H_NH = 0 }; assert_eq!( svc6(NR_CHANNEL_RECV |
| kernel/src/ktest.rs | 1611 | block | 其它 | unsafe { H_NB = 4; H_NH = 0 }; assert_eq!( svc6(NR_CHANNEL_RECV |
| kernel/src/ktest.rs | 1619 | block | 其它 | unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 0 }; assert_eq!( svc6 |
| kernel/src/ktest.rs | 1627 | block | 其它 | unsafe { u64::from_le_bytes(HBUF[..8].try_into().unwrap()) }, PA |
| kernel/src/ktest.rs | 1632 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc4(NR_CHANNEL_C |
| kernel/src/ktest.rs | 1638 | block | 其它 | unsafe { OUT } as u32, unsafe { CHILD_OUT } as u32); for _ in 0. |
| kernel/src/ktest.rs | 1638 | block | 其它 | unsafe { CHILD_OUT } as u32); for _ in 0..8 { assert_eq!( svc6(N |
| kernel/src/ktest.rs | 1657 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 1663 | block | 其它 | unsafe { OUT } as u32; assert_eq!( svc6(NR_CHANNEL_SEND as u64, |
| kernel/src/ktest.rs | 1678 | block | 其它 | unsafe { H_HND = [hsrc, 0, 0, 0, 0, 0, 0, 0]; HBUF[..8].copy_fro |
| kernel/src/ktest.rs | 1691 | block | 其它 | unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 8; H_HND = [0; 8] }; |
| kernel/src/ktest.rs | 1698 | block | 其它 | unsafe { H_HND[0] }; assert_eq!(unsafe { H_NH }, 1, "SYSCALL：rec |
| kernel/src/ktest.rs | 1699 | block | 其它 | unsafe { H_NH }, 1, "SYSCALL：recv 须回写 1 个随行 handle"); assert_ne! |
| kernel/src/ktest.rs | 1718 | block | 其它 | unsafe { H_HND = [hgd, 0xDEAD_BEEF, 0, 0, 0, 0, 0, 0] }; assert_ |
| kernel/src/ktest.rs | 1730 | block | 其它 | unsafe { H_HND = [hgd, hgd, 0, 0, 0, 0, 0, 0] }; assert_eq!( svc |
| kernel/src/ktest.rs | 1766 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_EVENT_CREATE as u64, 0, |
| kernel/src/ktest.rs | 1772 | block | 其它 | unsafe { OUT } as u32; assert_ne!(he, 0, "SYSCALL：event_create 须 |
| kernel/src/ktest.rs | 1782 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1801 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1807 | block | 其它 | unsafe { I_OBS }, U1 \| U2, "SYSCALL：observed 须回写全满足集"); // I3. 请 |
| kernel/src/ktest.rs | 1812 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1824 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1840 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 1846 | block | 其它 | unsafe { OUT } as u32; assert_eq!( svc4(NR_OBJECT_SIGNAL as u64, |
| kernel/src/ktest.rs | 1853 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1900 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1921 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc4(NR_CHANNEL_C |
| kernel/src/ktest.rs | 1927 | block | 其它 | unsafe { OUT } as u32, unsafe { CHILD_OUT } as u32); // SAFETY: |
| kernel/src/ktest.rs | 1927 | block | 其它 | unsafe { CHILD_OUT } as u32); // SAFETY: 同上。 unsafe { I_OBS = 0 |
| kernel/src/ktest.rs | 1929 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1940 | block | 其它 | unsafe { I_OBS }, SIGNAL_WRITABLE, "SYSCALL：WRITABLE 全满足集"); ass |
| kernel/src/ktest.rs | 1947 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1955 | block | 其它 | unsafe { I_OBS }, SIGNAL_READABLE \| SIGNAL_WRITABLE, "SYSCALL：RE |
| kernel/src/ktest.rs | 1967 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1974 | block | 其它 | unsafe { H_NB = 8; H_NH = 0 }; assert_eq!( svc6(NR_CHANNEL_RECV |
| kernel/src/ktest.rs | 1982 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1991 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1999 | block | 其它 | unsafe { I_OBS }, SIGNAL_READABLE \| SIGNAL_WRITABLE, "SYSCALL：全满 |
| kernel/src/ktest.rs | 2013 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_EVENT_CREATE as u64, 0, |
| kernel/src/ktest.rs | 2019 | block | 其它 | unsafe { OUT } as u32; assert_eq!( svc4(NR_OBJECT_SIGNAL as u64, |
| kernel/src/ktest.rs | 2025 | block | 其它 | unsafe { WM_IDX = 0 }; assert_eq!( svc6(NR_OBJECT_WAIT_MANY as u |
| kernel/src/ktest.rs | 2039 | block | 其它 | unsafe { WM_H = [3, hw_ev]; WM_S = [U1, U1] }; assert_eq!( svc6( |
| kernel/src/ktest.rs | 2048 | block | 其它 | unsafe { WM_H = [hw_ev, hw_ev]; WM_S = [U2, U2]; WM_IDX = 0 }; a |
| kernel/src/ktest.rs | 2055 | block | 其它 | unsafe { WM_IDX }, u64::MAX, "SYSCALL：未命中 idx 须留哨兵 MAX"); // SAF |
| kernel/src/ktest.rs | 2057 | block | 其它 | unsafe { WM_IDX = 0 }; assert_eq!( svc6(NR_OBJECT_WAIT_MANY as u |
| kernel/src/ktest.rs | 2067 | block | 其它 | unsafe { WM_H = [hw_ev, hw_ev]; WM_S = [U2, U1]; WM_IDX = 0; I_O |
| kernel/src/ktest.rs | 2074 | block | 其它 | unsafe { WM_IDX }, 1, "SYSCALL：idx 须指次项"); assert_eq!(unsafe { I |
| kernel/src/ktest.rs | 2075 | block | 其它 | unsafe { I_OBS }, U1, "SYSCALL：observed 须该对象全满足集"); // SAFETY: 同 |
| kernel/src/ktest.rs | 2077 | block | 其它 | unsafe { WM_S = [U1, U2]; WM_IDX = 0; I_OBS = 0 }; assert_eq!( s |
| kernel/src/ktest.rs | 2084 | block | 其它 | unsafe { WM_IDX }, 0, "SYSCALL：idx 须指首项（入参序最前）"); // 清场。 assert_ |
| kernel/src/ktest.rs | 2097 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/ktest.rs | 2115 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/ktest.rs | 2134 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/ktest.rs | 2163 | block | 其它 | unsafe { core::arch::asm!("brk #0x42", options(nostack, preserve |
| kernel/src/ktest.rs | 2174 | block | 其它 | unsafe { v1.write_volatile(0xDEAD_BEEF_CAFE_F00D); assert_eq!( v |
| kernel/src/ktest.rs | 2223 | block | 其它 | unsafe { for w in 0..512usize { va.add(w).write_volatile(0); } f |
| kernel/src/ktest.rs | 2247 | block | 其它 | unsafe { desc_at(pa as u64, 40, 1) }; assert_eq!(d1.kind, 4); as |
| kernel/src/ktest.rs | 2259 | block | 其它 | unsafe { desc_at(pa as u64, 40, i) }; assert_eq!( desc_range(d, |
| kernel/src/ktest.rs | 2367 | block | 其它 | unsafe { berry_hal_aarch64::uart_puts(b"PING "); berry_hal_aarch |
| kernel/src/ktest.rs | 2382 | block | 其它 | unsafe { let stack_top = (&raw const SWITCH_OTHER_STACK) as *con |
| kernel/src/ktest.rs | 2458 | fn | 其它 | unsafe fn fp_canary_load(pats: *const u128) { unsafe { core::arc |
| kernel/src/ktest.rs | 2459 | block | 其它 | unsafe { core::arch::asm!( ".arch armv8-a+fp+simd", // 局部重开 FP/S |
| kernel/src/ktest.rs | 2482 | fn | 其它 | unsafe fn fp_canary_store(out: *mut u128) { unsafe { core::arch: |
| kernel/src/ktest.rs | 2483 | block | 其它 | unsafe { core::arch::asm!( ".arch armv8-a+fp+simd", // 局部重开 FP/S |
| kernel/src/ktest.rs | 2505 | fn | 其它 | unsafe fn fpcr_read() -> u64 { let v: u64; unsafe { core::arch:: |
| kernel/src/ktest.rs | 2507 | block | 其它 | unsafe { core::arch::asm!(".arch armv8-a+fp+simd", "mrs {}, fpcr |
| kernel/src/ktest.rs | 2511 | fn | 其它 | unsafe fn fpcr_write(v: u64) { unsafe { core::arch::asm!(".arch |
| kernel/src/ktest.rs | 2512 | block | 其它 | unsafe { core::arch::asm!(".arch armv8-a+fp+simd", "msr fpcr, {} |
| kernel/src/ktest.rs | 2515 | fn | 其它 | unsafe fn fpsr_read() -> u64 { let v: u64; unsafe { core::arch:: |
| kernel/src/ktest.rs | 2517 | block | 其它 | unsafe { core::arch::asm!(".arch armv8-a+fp+simd", "mrs {}, fpsr |
| kernel/src/ktest.rs | 2521 | fn | 其它 | unsafe fn fpsr_write(v: u64) { unsafe { core::arch::asm!(".arch |
| kernel/src/ktest.rs | 2522 | block | 其它 | unsafe { core::arch::asm!(".arch armv8-a+fp+simd", "msr fpsr, {} |
| kernel/src/ktest.rs | 2526 | extern | 其它 | unsafe extern "C" { fn memset(s: *mut u8, c: u8, n: usize) -> *m |
| kernel/src/ktest.rs | 2537 | block | 其它 | unsafe { &*(&raw const FP_SNAP) }; snap == pats } /// FP 乒乓对端：装 |
| kernel/src/ktest.rs | 2548 | block | 其它 | unsafe { fpcr_write(FP_FPCR_OTHER); fpsr_write(FP_FPSR_OTHER); f |
| kernel/src/ktest.rs | 2577 | block | 其它 | unsafe { let stack_top = (&raw const FP_OTHER_STACK) as *const u |
| kernel/src/ktest.rs | 2718 | block | 其它 | unsafe { core::ptr::read_volatile(berry_hal_aarch64::RAM_BASE as |
| kernel/src/ktest.rs | 2771 | block | 其它 | unsafe { core::ptr::read_volatile(USER_VA as *const u64) }; asse |
| kernel/src/ktest.rs | 2774 | block | 其它 | unsafe { core::ptr::write_volatile(USER_VA as *mut u64, MAGIC2) |
| kernel/src/ktest.rs | 2782 | block | 其它 | unsafe { core::ptr::read_volatile(USER_VA as *const u64) }; asse |
| kernel/src/ktest.rs | 2789 | block | 其它 | unsafe { core::ptr::read_volatile(USER_VA as *const u64) }; asse |
| kernel/src/ktest.rs | 2793 | block | 其它 | unsafe { core::ptr::write_volatile(USER_VA as *mut u64, MAGIC) } |
| kernel/src/ktest.rs | 2797 | block | 其它 | unsafe { core::ptr::write_volatile(USER_VA as *mut u64, MAGIC) } |
| kernel/src/ktest.rs | 2815 | block | 其它 | unsafe { core::ptr::read_volatile(0x20_0000 as *const u64) }; as |
| kernel/src/ktest.rs | 3536 | block | 其它 | unsafe { rr.enqueue(&mut **p, &mut rq) }; } /// 相位复位：上一态循环推高的 VD |
| kernel/src/ktest.rs | 3545 | block | 其它 | unsafe { (**p).vruntime = 0; (**p).vdeadline = 0; } } rq.resync_ |
| kernel/src/ktest.rs | 4594 | block | 其它 | unsafe { Process::new(vref, pa, asid) }), RIGHT_MANAGE, ) .unwra |
| kernel/src/ktest.rs | 4600 | block | 其它 | unsafe { Thread::new(pref) }), RIGHT_MANAGE) .unwrap(); let (tre |
| kernel/src/ktest.rs | 4604 | block | 其它 | unsafe { let pr = &mut *pref.as_ptr(); let p = pr.obj.as_process |
| kernel/src/ktest.rs | 4678 | block | 其它 | unsafe { Process::new(vref, pa, asid) }), RIGHT_MANAGE, ) .expec |
| kernel/src/ktest.rs | 4687 | block | 其它 | unsafe { Thread::new(pref) }), RIGHT_MANAGE, ) .expect("FREEZE：F |
| kernel/src/ktest.rs | 4694 | block | 其它 | unsafe { let p_key = pref.as_ptr() as usize; let t_key = tref.as |
| kernel/src/ktest.rs | 4904 | block | 其它 | unsafe { Process::new(vref, pa, asid) }), RIGHT_MANAGE) .unwrap( |
| kernel/src/ktest.rs | 4922 | block | 其它 | unsafe { let p = (*pref.as_ptr()).obj.as_process_mut().unwrap(); |
| kernel/src/ktest.rs | 4938 | block | 其它 | unsafe { let p = (*pref.as_ptr()).obj.as_process_mut().unwrap(); |
| kernel/src/ktest.rs | 4967 | block | 其它 | unsafe { let p = (*pref.as_ptr()).obj.as_process_mut().unwrap(); |
| kernel/src/ktest.rs | 5004 | block | 其它 | unsafe { *core::ptr::addr_of!(TOK) }; assert_ne!(tok, 0, "FRZCYC |
| kernel/src/ktest.rs | 5007 | block | 其它 | unsafe { let p = (*pref.as_ptr()).obj.as_process_mut().unwrap(); |
| kernel/src/ktest.rs | 5079 | block | 其它 | unsafe { (*pref.as_ptr()).obj.as_process().unwrap().wake_pending |
| kernel/src/ktest.rs | 5083 | block | 其它 | unsafe { (*aref.as_ptr()).obj.as_channel().unwrap().frozen_owner |
| kernel/src/ktest.rs | 5087 | block | 其它 | unsafe { (*aref.as_ptr()).obj.as_channel().unwrap().has_message( |
| kernel/src/ktest.rs | 5097 | block | 其它 | unsafe { Process::new(vref, pa, asid) }), RIGHT_MANAGE) .unwrap( |
| kernel/src/ktest.rs | 5112 | block | 其它 | unsafe { (*pref.as_ptr()) .obj .as_process_mut() .unwrap() .ht . |
| kernel/src/ktest.rs | 5124 | block | 其它 | unsafe { (*pref.as_ptr()) .obj .as_process_mut() .unwrap() .ht . |
| kernel/src/ktest.rs | 5135 | block | 其它 | unsafe { ChannelEnd::pair(aref, bref) }; // —— 等待者属主 P2/T2（F7 形真 |
| kernel/src/ktest.rs | 5144 | block | 其它 | unsafe { Process::new(vref2, pa2, asid2) }), RIGHT_MANAGE) .unwr |
| kernel/src/ktest.rs | 5148 | block | 其它 | unsafe { Thread::new(pref2) }), RIGHT_MANAGE) .unwrap(); let (tr |
| kernel/src/ktest.rs | 5153 | block | 其它 | unsafe { // 运行钉住（表引 + 钉）+ 末线程簿记位（钩子 live_threads-=1 → 0 走 // Pro |
| kernel/src/ktest.rs | 5185 | block | 其它 | unsafe { *core::ptr::addr_of!(TOK) }; assert_ne!(tok, 0, "WAKEP： |
| kernel/src/ktest.rs | 5199 | block | 其它 | unsafe { *core::ptr::addr_of!(OBS) }, 0, "WAKEP：②未命中 observed 须 |
| kernel/src/ktest.rs | 5227 | block | 其它 | unsafe { *core::ptr::addr_of!(OBS) }, SIGNAL_WAKE_PENDING, "WAKE |
| kernel/src/ktest.rs | 5266 | block | 其它 | unsafe { *core::ptr::addr_of!(TOK) }; assert!(end_frozen(aref), |
| kernel/src/ktest.rs | 5273 | block | 其它 | unsafe { *core::ptr::addr_of!(TOK2) }; assert_eq!( svc6(NR_CHANN |
| kernel/src/ktest.rs | 5301 | block | 其它 | unsafe { berry_sched::kill_all_tasks_by_pinned(wp2, 0) }; assert |
| kernel/src/ktest.rs | 5307 | block | 其它 | unsafe { (*pref.as_ptr()).obj.as_process_mut().unwrap().ht.drain |
| kernel/src/ktest.rs | 5574 | block | 其它 | unsafe { ATTR_BUF[0..4].copy_from_slice(&56u32.to_le_bytes()); A |
| kernel/src/ktest.rs | 5640 | block | 其它 | unsafe { rr.enqueue(&mut **r, &mut rq) }; } let t0 = berry_hal_a |
| kernel/src/ktest.rs | 5705 | block | 其它 | unsafe { rr.enqueue(&mut **p, &mut rq) }; } // 100 轮 pick + 计数（指 |
| kernel/src/ktest.rs | 5720 | block | 其它 | unsafe { (*pp).vdeadline = 100 + i }; } } let spread = hits.iter |
| kernel/src/ktest.rs | 6009 | block | 其它 | unsafe { &mut *cur2 }, unsafe { &mut *core::ptr::addr_of_mut!(rq |
| kernel/src/ktest.rs | 6009 | block | 其它 | unsafe { &mut *core::ptr::addr_of_mut!(rq2) }); assert!(tb.held_ |
| kernel/src/ktest.rs | 6074 | block | 其它 | unsafe { &mut *(ht as *mut HandleTable) }; let h = ht .insert( K |
| kernel/src/ktest.rs | 6089 | block | 其它 | unsafe { ChannelEnd::pair(a, b) }; } /// 一个 Event 进内核侧表（非 channe |
| kernel/src/ktest.rs | 6409 | block | 其它 | unsafe { (*ni.as_ptr()).obj.as_channel().unwrap().no_intent() }, |
| kernel/src/ktest.rs | 6607 | block | 其它 | unsafe { Process::new(vref, pa, asid) }), RIGHT_MANAGE) .unwrap( |
| kernel/src/ktest.rs | 6611 | block | 其它 | unsafe { (*pref.as_ptr()).obj.as_process_mut().unwrap() }; asser |
| kernel/src/ktest.rs | 7237 | block | 其它 | unsafe { ChannelEnd::pair(a, b) }; } // L0. 非法参 + 空队 → None（send |
| kernel/src/main.rs | 347 | block | 其它 | unsafe { mm::init(handoff) }; // GUI P2：超出 GOP 缓冲的模式（如 540×1200） |
| kernel/src/main.rs | 441 | block | 其它 | unsafe { core::slice::from_raw_parts(p, len) }; Some((b"handoff" |
| kernel/src/main.rs | 475 | block | 其它 | unsafe { core::slice::from_raw_parts( phys_to_virt(s.pa as usize |
| kernel/src/main.rs | 517 | block | 其它 | unsafe { core::slice::from_raw_parts(p, len as usize) }; Some((b |
| kernel/src/main.rs | 530 | block | 其它 | unsafe { core::slice::from_raw_parts(pa, (e - s) as usize) }; (b |
| kernel/src/main.rs | 562 | block | 其它 | unsafe { *(&raw mut ROOTFS) = Some(buf) }; berry_hal_aarch64::ua |
| kernel/src/main.rs | 652 | block | 其它 | unsafe { (*p_am2b.as_ptr()).retain(); (*p_am2b.as_ptr()).retain( |
| kernel/src/main.rs | 673 | block | MMIO | unsafe { berry_hal_aarch64::timer::init() }; // 2b-2：两忙循环任务各挂一个私 |
| kernel/src/main.rs | 829 | block | 其它 | unsafe { crate::channel::ChannelEnd::pair(pk, pu) }; // 先于用户矩阵入队 |
| kernel/src/main.rs | 889 | block | 其它 | unsafe { (*p_rx.as_ptr()).retain() }; // 静态槽传递（spawn arg 已满：hi32 |
| kernel/src/main.rs | 894 | block | 其它 | unsafe { *core::ptr::addr_of_mut!(RX_EV_H) = h_rx_ev; *core::ptr |
| kernel/src/main.rs | 918 | block | 其它 | unsafe { *core::ptr::addr_of_mut!(BSH_IN_H) = h_end_k2; } // 3-2 |
| kernel/src/main.rs | 1029 | block | 其它 | unsafe { ChannelEnd::pair(pa, pb) }; // 4-2b：哑 Event 进 ponger 表— |
| kernel/src/main.rs | 1078 | block | 其它 | unsafe { ChannelEnd::pair(pc, ps) }; let h_ev_pi = syscall::boot |
| kernel/src/main.rs | 1117 | block | 其它 | unsafe { ChannelEnd::pair(ps, pr) }; (hs, hr) }; let (h_d1, h_m1 |
| kernel/src/main.rs | 1138 | block | 其它 | unsafe { (*p.as_ptr()).obj.as_channel_mut().unwrap().push(m) }; |
| kernel/src/main.rs | 1157 | block | 其它 | unsafe { for (i, &h) in slots.iter().enumerate() { *core::ptr::a |
| kernel/src/main.rs | 1201 | block | 其它 | unsafe { ChannelEnd::pair(ps, pr) }; (hs, hr) }; let (h_fa, h_fb |
| kernel/src/main.rs | 1231 | block | 其它 | unsafe { ChannelEnd::pair(pa, pb) }; // arg 打包双 handle：低 32 = 自己 |
| kernel/src/main.rs | 1277 | block | 其它 | unsafe { (*(&raw mut ROOTFS)).take() }.map(\|buf\| { let vmo = mat |
| kernel/src/main.rs | 1337 | block | 其它 | unsafe { crate::channel::ChannelEnd::pair(pk2, pu2) }; // ---- P |
| kernel/src/main.rs | 1477 | block | 其它 | unsafe { crate::channel::ChannelEnd::pair(pkf, puf) }; // SAFETY |
| kernel/src/main.rs | 1479 | block | 其它 | unsafe { *core::ptr::addr_of_mut!(FB_PIPE_H) = h_end_kfb; } // A |
| kernel/src/main.rs | 1579 | block | 其它 | unsafe { ChannelEnd::pair(pab_a, pab_b) }; // E_done（bench 完成事件） |
| kernel/src/main.rs | 1781 | block | MMIO | unsafe { berry_hal_aarch64::uart::rx_init() }; // sim-S1（`docs/s |
| kernel/src/main.rs | 1887 | block | 其它 | unsafe { crate::channel::ChannelEnd::pair(p_tx, p_b_rx); // 帧通道 |
| kernel/src/main.rs | 1894 | block | 其它 | unsafe { (*p_ev.as_ptr()).retain(); *core::ptr::addr_of_mut!(UAR |
| kernel/src/main.rs | 1913 | block | MMIO | unsafe { berry_hal_aarch64::uart::uart1_init() }; // sim-S2 A4②： |
| kernel/src/main.rs | 1922 | block | MMIO | unsafe { berry_hal_aarch64::pl061::pl061_init() }; berry_hal_aar |
| kernel/src/main.rs | 2142 | block | MMIO | unsafe { berry_hal_aarch64::timer::arm_slice() }; berry_hal_aarc |
| kernel/src/main.rs | 2162 | block | 其它 | unsafe { core::ptr::read_volatile(aspace::USER_VA as *const u64) |
| kernel/src/main.rs | 2181 | block | 其它 | unsafe { core::ptr::read_volatile(aspace::USER_VA as *const u64) |
| kernel/src/main.rs | 2288 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/main.rs | 2351 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/main.rs | 2536 | block | 其它 | unsafe { *core::ptr::addr_of!(RX_EV_H) } } /// bsh stdin 发端（endK |
| kernel/src/main.rs | 2546 | block | 其它 | unsafe { *core::ptr::addr_of!(BSH_IN_H) } } /// fb 上屏管道 console |
| kernel/src/main.rs | 2558 | block | 其它 | unsafe { *core::ptr::addr_of!(FB_PIPE_H) } } // ---- sim-S1：UART |
| kernel/src/main.rs | 2590 | block | 其它 | unsafe { *core::ptr::addr_of!(UART1_EV_H) } } /// sim 帧通道 simcon |
| kernel/src/main.rs | 2601 | block | 其它 | unsafe { *core::ptr::addr_of!(SIM_TX_H) } } /// 读 SIM_ACK_H 静态槽（ |
| kernel/src/main.rs | 2607 | block | MMIO | unsafe { *core::ptr::addr_of!(SIM_ACK_H) } } /// 「UART1 RX 到数据」通 |
| kernel/src/main.rs | 2615 | fn | 其它 | unsafe extern "C" fn uart1_notify() -> bool { // SAFETY: IRQ 上下文 |
| kernel/src/main.rs | 2617 | block | 其它 | unsafe { *core::ptr::addr_of!(UART1_EV_REF) } .expect("uart1_not |
| kernel/src/main.rs | 2624 | block | 其它 | unsafe { // 写位+扫描同窗（c-2b slice 4 自锁面；KL 外层包络剥除后的纯 SQ 单锁窗）。 let s |
| kernel/src/main.rs | 2653 | fn | 其它 | unsafe extern "C" fn pl061_notify() -> bool { crate::telemetry:: |
| kernel/src/main.rs | 2680 | fn | 其它 | unsafe fn uart1_ev_clear() { // SAFETY: 调用方保证屏蔽包络 + 引用不变。 let p |
| kernel/src/main.rs | 2682 | block | 其它 | unsafe { *core::ptr::addr_of!(UART1_EV_REF) } .expect("uart1_ev_ |
| kernel/src/main.rs | 2684 | block | 其它 | unsafe { if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() { ev |
| kernel/src/main.rs | 2696 | fn | 其它 | unsafe extern "C" fn irq_tick_acct(tf: &mut berry_hal_aarch64::t |
| kernel/src/main.rs | 2702 | block | MMIO | unsafe { berry_sched::on_irq_tick(tf) } } /// 「RX 到数据」通知（HAL `ex |
| kernel/src/main.rs | 2723 | fn | 其它 | unsafe extern "C" fn rx_irq_notify() -> bool { // SAFETY: IRQ 上下 |
| kernel/src/main.rs | 2725 | block | 其它 | unsafe { *core::ptr::addr_of!(RX_EV_REF) } .expect("rx_irq_notif |
| kernel/src/main.rs | 2733 | block | 其它 | unsafe { // c-2b slice 4（自锁面第三处，F-L2 + 义务③ IRQ 面）：**写位+扫描 // 同窗* |
| kernel/src/main.rs | 2765 | fn | 其它 | unsafe fn rx_ev_clear() { // SAFETY: 调用方保证屏蔽包络 + 引用不变。 let p = u |
| kernel/src/main.rs | 2767 | block | 其它 | unsafe { *core::ptr::addr_of!(RX_EV_REF) } .expect("rx_ev_clear: |
| kernel/src/main.rs | 2769 | block | 其它 | unsafe { if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() { ev |
| kernel/src/main.rs | 2805 | block | 其它 | unsafe { *core::ptr::addr_of!(PRESSURE_EV_REF) } .expect("am2b: |
| kernel/src/main.rs | 2809 | block | 其它 | unsafe { if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() { ev |
| kernel/src/main.rs | 2822 | block | 其它 | unsafe { *core::ptr::addr_of!(PRESSURE_EV_REF) } .expect("am2b_p |
| kernel/src/main.rs | 2825 | block | 其它 | unsafe { (*p.as_ptr()).obj.as_event_mut().map(\|e\| e.load()).unwr |
| kernel/src/main.rs | 2834 | block | 其它 | unsafe { *core::ptr::addr_of!(PRESSURE_EV_REF) } .expect("am2b: |
| kernel/src/main.rs | 2837 | block | 其它 | unsafe { (*p.as_ptr()).retain(); crate::syscall::bootstrap_table |
| kernel/src/main.rs | 2975 | block | 其它 | unsafe { rx_ev_clear() }; berry_hal_aarch64::enable_irq(); for i |
| kernel/src/main.rs | 3181 | block | 其它 | unsafe { uart1_ev_clear() }; berry_hal_aarch64::enable_irq(); // |
| kernel/src/main.rs | 3262 | block | 其它 | unsafe { *core::ptr::addr_of!(INTENT_H).cast::<u32>().add(i) } } |
| kernel/src/main.rs | 3891 | block | MMIO | unsafe { &*(arg as *const AiDemoCtx) }; let freq = berry_hal_aar |
| kernel/src/main.rs | 4432 | block | 其它 | unsafe { (*(trig as *mut crate::handle::ObjRef)) .obj .as_event_ |
| kernel/src/main.rs | 4450 | block | 其它 | unsafe { (*(trig as *mut crate::handle::ObjRef)) .obj .as_event_ |
| kernel/src/main.rs | 4624 | block | 其它 | unsafe { (*objp) .obj .as_event_mut() .expect("latsgi_wak: ObjRe |
| kernel/src/main.rs | 4690 | block | 其它 | unsafe { (*(trig as *mut crate::handle::ObjRef)) .obj .as_event_ |
| kernel/src/main.rs | 4706 | block | 其它 | unsafe { (*(trig as *mut crate::handle::ObjRef)) .obj .as_event_ |
| kernel/src/main.rs | 4838 | block | 其它 | unsafe { (*(t as *mut crate::handle::ObjRef)) .obj .as_event_mut |
| kernel/src/main.rs | 4883 | fn | 其它 | unsafe extern "C" fn secondary_main() -> ! { // S-2b 修复：换离低半早期栈（ |
| kernel/src/main.rs | 4893 | block | 其它 | unsafe { core::arch::asm!("mov sp, {0}", in(reg) top, options(no |
| kernel/src/main.rs | 4898 | block | 上下文切换 | unsafe { core::arch::asm!("msr daifset, #0xf", options(nostack, |
| kernel/src/main.rs | 4918 | block | 其它 | unsafe { berry_sched::park_recheck() }; // PAR 清钉（tickless 伴生洞收口 |
| kernel/src/main.rs | 4928 | block | MMIO | unsafe { berry_hal_aarch64::timer::arm_at(d) }, // SAFETY: CNTV |
| kernel/src/main.rs | 4932 | block | MMIO | unsafe { berry_hal_aarch64::timer::mask() }, } } else { // SAFET |
| kernel/src/main.rs | 4937 | block | MMIO | unsafe { berry_hal_aarch64::timer::arm_slice() }; } // masked-wf |
| kernel/src/main.rs | 5198 | block | 其它 | unsafe { &mut *(arg as *mut crate::handle::HandleTable) }; // Δ |
| kernel/src/main.rs | 6164 | block | 其它 | unsafe { &mut *(berry_sched::current_tag() as *mut crate::handle |
| kernel/src/main.rs | 6171 | block | 其它 | unsafe { oref.as_ref() }; objref.obj.as_process().expect("ht: 类型 |
| kernel/src/mem.rs | 36 | block | 其它 | unsafe { while rem >= 8 { (p as *mut u64).write_unaligned(word); |
| kernel/src/mem.rs | 58 | block | 其它 | unsafe { while rem >= 8 { (d as *mut u64).write_unaligned((s as |
| kernel/src/mem.rs | 80 | block | 其它 | unsafe { if (dst as usize) < (src as usize) { memcpy(dst, src, n |
| kernel/src/mem.rs | 114 | block | 其它 | unsafe { while rem >= 8 { let wa = (pa as *const u64).read_unali |
| kernel/src/mm.rs | 74 | block | 其它 | unsafe { berry_mm::memmap::desc_at(h.memmap_pa, h.memmap_desc_si |
| kernel/src/mm.rs | 94 | fn | 其它 | unsafe fn init(handoff: Option<&'static crate::handoff::BerryHan |
| kernel/src/mm.rs | 141 | block | 其它 | unsafe { berry_mm::memmap::desc_at(pa, dsize, i) }; if matches!( |
| kernel/src/mm.rs | 155 | block | 其它 | unsafe { berry_mm::memmap::desc_at(pa, dsize, i) }; if let berry |
| kernel/src/mm.rs | 189 | block | 分配器 | unsafe { FRAMES .as_mut() .expect("mm::init() 未调用就访问 frames()") |
| kernel/src/pi.rs | 149 | block | 其它 | unsafe { &*(raw as *const ObjRef) }; match &obj.obj { KernelObj: |
| kernel/src/pi.rs | 168 | block | 其它 | unsafe { NonNull::new_unchecked(edge.0 as *mut ObjRef) }; Some(( |
| kernel/src/pi.rs | 182 | block | 其它 | unsafe { &*(t.proc_tag as *const HandleTable) }) } else { Some(c |
| kernel/src/process.rs | 124 | fn | 其它 | unsafe fn new(vmar_ref: NonNull<ObjRef>, ttbr0_pa: usize, asid: |
| kernel/src/process.rs | 161 | block | 其它 | unsafe { ObjRef::release(self.vmar) }; PROCESS_LIVE.fetch_sub(1, |
| kernel/src/process.rs | 204 | fn | 其它 | unsafe fn new(proc_ref: NonNull<ObjRef>) -> Self { (*proc_ref.as |
| kernel/src/process.rs | 219 | block | 其它 | unsafe { ObjRef::release(self.proc) }; } } |
| kernel/src/suspend.rs | 43 | block | 其它 | unsafe { let r = &mut *ptr.as_ptr(); match &mut r.obj { crate::h |
| kernel/src/suspend.rs | 90 | block | 其它 | unsafe { ObjRef::release(ptr) } } } |
| kernel/src/syscall.rs | 106 | block | 其它 | unsafe { let slot = &raw mut BOOTSTRAP_HT; if (*slot).is_none() |
| kernel/src/syscall.rs | 136 | block | 其它 | unsafe { &mut *(tag as *mut HandleTable) }) } else { Ok(bootstra |
| kernel/src/syscall.rs | 144 | block | 其它 | unsafe { &mut *(tag as *mut HandleTable) }) } } // ---- copy_fro |
| kernel/src/syscall.rs | 272 | block | 其它 | unsafe { core::ptr::copy_nonoverlapping(src as *const u8, dst.as |
| kernel/src/syscall.rs | 307 | block | 其它 | unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst as *mu |
| kernel/src/syscall.rs | 332 | block | 其它 | unsafe { core::ptr::read_volatile(ptr as *const u64) }) } /// 写一 |
| kernel/src/syscall.rs | 777 | block | 其它 | unsafe { &mut (*obj.as_ptr()).obj }.as_vmo_mut() } /// `vmar_map |
| kernel/src/syscall.rs | 823 | block | 其它 | unsafe { ((*vr.as_ptr()).obj.as_vmar_mut(), (*vo.as_ptr()).obj.a |
| kernel/src/syscall.rs | 1003 | block | 其它 | unsafe { (*vds[i].unwrap().as_ptr()).retain() }; if let Some(s) |
| kernel/src/syscall.rs | 1005 | block | 其它 | unsafe { (*s.as_ptr()).retain() }; } } // dv 本体指针整批预解析一次（retain |
| kernel/src/syscall.rs | 1013 | block | 其它 | unsafe { (*vds[i].unwrap().as_ptr()).obj.as_vmo_mut() } { Some(v |
| kernel/src/syscall.rs | 1017 | block | 其它 | unsafe { release_wins(&vds, &svs) }; return ERR_BAD_HANDLE as i6 |
| kernel/src/syscall.rs | 1076 | block | 其它 | unsafe { release_wins(&vds, &svs) }; ret } /// 0x24 表批 2N ObjRef |
| kernel/src/syscall.rs | 1084 | fn | 其它 | unsafe fn release_wins( vds: &[Option<NonNull<ObjRef>>; PAGER_TA |
| kernel/src/syscall.rs | 1090 | block | 其它 | unsafe { ObjRef::release(*v) }; } for s in svs.iter().flatten() |
| kernel/src/syscall.rs | 1094 | block | 其它 | unsafe { ObjRef::release(*s) }; } } /// 0x21/0x23 源偏选择：Fixed = 调 |
| kernel/src/syscall.rs | 1127 | block | 其它 | unsafe { (*vd.as_ptr()).retain() }; if let Some(s) = sv { unsafe |
| kernel/src/syscall.rs | 1129 | block | 其它 | unsafe { (*s.as_ptr()).retain() }; } // X-5 过滤取输入：dv 本体指针先行解析一次（ |
| kernel/src/syscall.rs | 1135 | block | 其它 | unsafe { (*vd.as_ptr()).obj.as_vmo_mut() } { Some(v) => v as *mu |
| kernel/src/syscall.rs | 1139 | block | 其它 | unsafe { ObjRef::release(vd) }; if let Some(s) = sv { unsafe { O |
| kernel/src/syscall.rs | 1141 | block | 其它 | unsafe { ObjRef::release(s) }; } return ERR_BAD_HANDLE as i64; } |
| kernel/src/syscall.rs | 1182 | block | 其它 | unsafe { ObjRef::release(vd) }; if let Some(s) = sv { unsafe { O |
| kernel/src/syscall.rs | 1184 | block | 其它 | unsafe { ObjRef::release(s) }; } ret } /// 0x21/0x23 供给执行体（一次武装请 |
| kernel/src/syscall.rs | 1206 | block | 其它 | unsafe { (*root).demand_region_at(far) }) else { return ERR_INVA |
| kernel/src/syscall.rs | 1215 | block | 其它 | unsafe { (*vd.as_ptr()).obj.as_vmo_mut() } { Some(v) => v as *mu |
| kernel/src/syscall.rs | 1226 | block | 其它 | unsafe { (*root).supply_page(crate::mm::frames(), va) } { return |
| kernel/src/syscall.rs | 1233 | block | 其它 | unsafe { (*s.as_ptr()).obj.as_vmo_mut() }) else { return ERR_BAD |
| kernel/src/syscall.rs | 1251 | block | 其它 | unsafe { &mut *dv_ptr }.write(toff, buf) { return voe(e) as i64; |
| kernel/src/syscall.rs | 1265 | block | 其它 | unsafe { &mut (*obj.as_ptr()).obj }).as_vmar_mut() else { return |
| kernel/src/syscall.rs | 1294 | block | 其它 | unsafe { &mut (*obj.as_ptr()).obj }).as_vmar_mut() else { return |
| kernel/src/syscall.rs | 1334 | block | 其它 | unsafe { &mut (*obj.as_ptr()).obj }).as_vmar_mut() else { return |
| kernel/src/syscall.rs | 1402 | block | 其它 | unsafe { crate::process::Process::new(vobj, ttbr0, asid) }; matc |
| kernel/src/syscall.rs | 1433 | block | 其它 | unsafe { crate::process::Thread::new(pobj) }; // 1.1/S1/S3-b：创建者 |
| kernel/src/syscall.rs | 1479 | block | 其它 | unsafe { (*tp).intent_declared = attr.intent; (*tp).intent_effec |
| kernel/src/syscall.rs | 1496 | block | 其它 | unsafe { (*tp).prio_base = berry_sched::PRIO_DEADLINE; (*tp).pri |
| kernel/src/syscall.rs | 1527 | block | 其它 | unsafe { (*vt.as_ptr()).obj.as_thread_mut() }; let Some(thrd) = |
| kernel/src/syscall.rs | 1536 | block | 其它 | unsafe { (*proc_ref.as_ptr()).obj.as_process_mut() }; let Some(p |
| kernel/src/syscall.rs | 1558 | block | 其它 | unsafe { (*proc_ref.as_ptr()).retain() }; unsafe { (*vt.as_ptr() |
| kernel/src/syscall.rs | 1559 | block | 其它 | unsafe { (*vt.as_ptr()).retain() }; let eff = crate::process::Sc |
| kernel/src/syscall.rs | 1578 | block | 其它 | unsafe { ObjRef::release(proc_ref) }; unsafe { ObjRef::release(v |
| kernel/src/syscall.rs | 1579 | block | 其它 | unsafe { ObjRef::release(vt) }; return ERR_NO_MEMORY as i64; } } |
| kernel/src/syscall.rs | 1585 | block | 其它 | unsafe { berry_sched::enqueue_prepared_user(task_ptr) }; 0 } /// |
| kernel/src/syscall.rs | 1631 | block | 其它 | unsafe { ((*vr.as_ptr()).obj.as_process_mut(), (*vt.as_ptr()).ob |
| kernel/src/syscall.rs | 1679 | block | 其它 | unsafe { (*vr.as_ptr()).retain() }; unsafe { (*vt.as_ptr()).reta |
| kernel/src/syscall.rs | 1680 | block | 其它 | unsafe { (*vt.as_ptr()).retain() }; let eff = crate::process::Sc |
| kernel/src/syscall.rs | 1700 | block | 其它 | unsafe { ObjRef::release(vr) }; unsafe { ObjRef::release(vt) }; |
| kernel/src/syscall.rs | 1701 | block | 其它 | unsafe { ObjRef::release(vt) }; if bootstrap != 0 { // 迁移回滚：把子表令 |
| kernel/src/syscall.rs | 1713 | block | 其它 | unsafe { berry_sched::enqueue_prepared_user(task_ptr) }; 0 } // |
| kernel/src/syscall.rs | 1742 | fn | 其它 | unsafe extern "C" fn task_exit_hook(pinned: usize, thread_key: u |
| kernel/src/syscall.rs | 1749 | block | 其它 | unsafe { if thread_key != 0 { let tobj = thread_key as *mut ObjR |
| kernel/src/syscall.rs | 1793 | block | 其它 | unsafe { berry_sched::kill_all_tasks_by_pinned(p, 0) }; } berry_ |
| kernel/src/syscall.rs | 1879 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); let Some(p) = r.obj.as_proc |
| kernel/src/syscall.rs | 1969 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); let Some(p) = r.obj.as_proc |
| kernel/src/syscall.rs | 1994 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); let Some(p) = r.obj.as_proc |
| kernel/src/syscall.rs | 2004 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); if let Some(p) = r.obj.as_p |
| kernel/src/syscall.rs | 2020 | block | 其它 | unsafe { if let Some(p) = (*obj.as_ptr()).obj.as_process_mut() { |
| kernel/src/syscall.rs | 2041 | block | 其它 | unsafe { if let Some(p) = (*obj.as_ptr()).obj.as_process_mut() { |
| kernel/src/syscall.rs | 2047 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); if let Some(p) = r.obj.as_p |
| kernel/src/syscall.rs | 2076 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); let Some(thrd) = r.obj.as_t |
| kernel/src/syscall.rs | 2111 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); let Some(thrd) = r.obj.as_t |
| kernel/src/syscall.rs | 2128 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); if let Some(thrd) = r.obj.a |
| kernel/src/syscall.rs | 2152 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); if let Some(thrd) = r.obj.a |
| kernel/src/syscall.rs | 2362 | block | 其它 | unsafe { &*th.owner().as_ptr() }; let base = owner.obj.as_proces |
| kernel/src/syscall.rs | 2387 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); match &mut r.obj { KernelOb |
| kernel/src/syscall.rs | 2442 | block | 其它 | unsafe { let r = &*obj.as_ptr(); match &r.obj { KernelObj::Threa |
| kernel/src/syscall.rs | 2512 | block | 其它 | unsafe { &mut *(p as *mut ObjRef) } } else { let Ok(t) = caller_ |
| kernel/src/syscall.rs | 2517 | block | 其它 | unsafe { &mut *o.as_ptr() }, Err(e) => return e as i64, // 坏值/类型 |
| kernel/src/syscall.rs | 2550 | block | 其它 | unsafe { (*(p as *mut ObjRef)).obj.as_process_mut() }) else { re |
| kernel/src/syscall.rs | 2560 | block | 其它 | unsafe { (*(p as *mut ObjRef)).obj.as_process_mut() } { proc.pen |
| kernel/src/syscall.rs | 2870 | block | 其它 | unsafe { ChannelEnd::pair(a, b) }; let r0 = copy_out_u32(el1, ou |
| kernel/src/syscall.rs | 2978 | block | 其它 | unsafe { let Some(end) = (*self_ptr.as_ptr()).obj.as_channel_mut |
| kernel/src/syscall.rs | 3101 | block | 其它 | unsafe { let Some(peer_end) = (*peer_ptr.as_ptr()).obj.as_channe |
| kernel/src/syscall.rs | 3120 | block | 其它 | unsafe { berry_sched::fast_wake_locked(t) }; berry_sched::sq_unl |
| kernel/src/syscall.rs | 3149 | block | 其它 | unsafe { if let Some(p) = (*owner.as_ptr()).obj.as_process_mut() |
| kernel/src/syscall.rs | 3181 | fn | 其它 | unsafe fn recv_rollback(t: &mut HandleTable, endp: NonNull<ObjRe |
| kernel/src/syscall.rs | 3220 | block | 其它 | unsafe { let Some(end) = (*self_ptr.as_ptr()).obj.as_channel_mut |
| kernel/src/syscall.rs | 3233 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return e as i64 |
| kernel/src/syscall.rs | 3241 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return e as i64 |
| kernel/src/syscall.rs | 3247 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return ERR_INVA |
| kernel/src/syscall.rs | 3252 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return e as i64 |
| kernel/src/syscall.rs | 3262 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return e as i64 |
| kernel/src/syscall.rs | 3274 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) }; retur |
| kernel/src/syscall.rs | 3279 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) }; retur |
| kernel/src/syscall.rs | 3284 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) }; retur |
| kernel/src/syscall.rs | 3293 | block | 其它 | unsafe { (*self_ptr.as_ptr()).obj.as_channel() }).and_then(\|e\| e |
| kernel/src/syscall.rs | 3299 | block | 其它 | unsafe { (*self_ptr.as_ptr()).obj.as_channel() }).map_or(false, |
| kernel/src/syscall.rs | 3380 | fn | 其它 | unsafe fn signal_state(obj: NonNull<ObjRef>) -> Option<u64> { un |
| kernel/src/syscall.rs | 3381 | block | 其它 | unsafe { match &(*obj.as_ptr()).obj { KernelObj::Event(e) => Som |
| kernel/src/syscall.rs | 3448 | block | 其它 | unsafe { let Some(ev) = (*obj.as_ptr()).obj.as_event_mut() else |
| kernel/src/syscall.rs | 3540 | block | 其它 | unsafe { (*obj.as_ptr()).retain() }; // AM-1.5：SuspendToken 等待者挂 |
| kernel/src/syscall.rs | 3551 | block | 其它 | unsafe { &(*obj.as_ptr()).obj } { KernelObj::SuspendToken(tok) = |
| kernel/src/syscall.rs | 3568 | block | 其它 | unsafe { signal_state(obj) }) else { break ERR_BAD_HANDLE as i64 |
| kernel/src/syscall.rs | 3608 | block | 其它 | unsafe { ObjRef::release(obj) }; ret } /// `object_wait_many(han |
| kernel/src/syscall.rs | 3700 | block | 其它 | unsafe { (*o.as_ptr()).retain() }; } let ret = loop { // 逐项重验（醒来 |
| kernel/src/syscall.rs | 3709 | block | 其它 | unsafe { signal_state(o) }) else { hit = Some(Err(ERR_BAD_HANDLE |
| kernel/src/syscall.rs | 3752 | block | 其它 | unsafe { ObjRef::release(*o) }; } ret } // ---- boot 期注册（rust_ma |
| kernel/src/telemetry.rs | 113 | fn | 其它 | unsafe fn ring_write(rec: SwitchRec) { let cpu = cpu_id().min(NC |
| kernel/src/telemetry.rs | 141 | block | 其它 | unsafe { ring_write(rec) } } /// 注册切换钩子（`rust_main` 在 `berry_sch |
| kernel/src/telemetry.rs | 183 | block | 其它 | unsafe { &(*core::ptr::addr_of!(SWITCH_RINGS))[cpu] }; loop { le |
| kernel/src/telemetry.rs | 213 | block | 其它 | unsafe { (*core::ptr::addr_of!(SWITCH_RINGS))[cpu].total }; (t a |
| kernel/src/telemetry.rs | 220 | block | 其它 | unsafe { (*core::ptr::addr_of!(SWITCH_RINGS))[cpu].total } } /// |
| kernel/src/telemetry.rs | 354 | fn | 其它 | unsafe fn wake_ring_write(rec: WakeRec) { let cpu = cpu_id().min |
| kernel/src/telemetry.rs | 424 | block | 其它 | unsafe { wake_ring_write(rec) } // R-0 增量直方图：延迟 = 唤醒时刻 − 入睡时刻（原始 |
| kernel/src/telemetry.rs | 672 | block | 其它 | unsafe { &(*core::ptr::addr_of!(WAKE_RINGS))[cpu] }; loop { let |
| kernel/src/telemetry.rs | 700 | block | 其它 | unsafe { (*core::ptr::addr_of!(WAKE_RINGS))[cpu].total }; (t as |
| kernel/src/telemetry.rs | 706 | block | 其它 | unsafe { (*core::ptr::addr_of!(WAKE_RINGS))[cpu].total } } /// 唤 |
| kernel/src/telemetry.rs | 980 | block | 其它 | unsafe { *core::ptr::addr_of_mut!(PRESSURE_OUTLET) = Some(outlet |
| kernel/src/telemetry.rs | 1016 | block | 其它 | unsafe { *core::ptr::addr_of!(PRESSURE_OUTLET) } { f(); } } } // |
| kernel/src/user.rs | 1609 | fn | 其它 | unsafe fn blob(start: *const u8, end: *const u8) -> &'static [u8 |
| kernel/src/user.rs | 1616 | block | 其它 | unsafe { blob(&__user_good, &__user_good_end) } } /// rogue_ptr |
| kernel/src/user.rs | 1622 | block | 其它 | unsafe { blob(&__user_rogue_ptr, &__user_rogue_ptr_end) } } /// |
| kernel/src/user.rs | 1641 | block | 其它 | unsafe { blob(&__user_spawner, &__user_spawner_end) } } /// wait |
| kernel/src/user.rs | 1648 | block | 其它 | unsafe { blob(&__user_waiter, &__user_waiter_end) } } /// pinger |
| kernel/src/user.rs | 1655 | block | 其它 | unsafe { blob(&__user_pinger, &__user_pinger_end) } } /// ponger |
| kernel/src/user.rs | 1662 | block | 其它 | unsafe { blob(&__user_ponger, &__user_ponger_end) } } /// LB 金丝雀 |
| kernel/src/user.rs | 1671 | block | 其它 | unsafe { blob(&__user_lb, &__user_lb_end) } } /// TLB shootdown |
| kernel/src/user.rs | 1679 | block | 其它 | unsafe { blob(&__user_tlbc, &__user_tlbc_end) } } /// ALLOC chur |
| kernel/src/user.rs | 1687 | block | 其它 | unsafe { blob(&__user_allocst, &__user_allocst_end) } } /// HT 风 |
| kernel/src/user.rs | 1694 | block | 其它 | unsafe { blob(&__user_htst, &__user_htst_end) } } /// PP ping-po |
| kernel/src/user.rs | 1703 | block | 其它 | unsafe { blob(&__user_pp, &__user_pp_end) } } /// soakload 永续载荷子 |
| kernel/src/user.rs | 1712 | block | 其它 | unsafe { blob(&__user_soakload, &__user_soakload_end) } } /// be |
| kernel/src/user.rs | 1721 | block | 其它 | unsafe { blob(&__user_latslp, &__user_latslp_end) } } /// svcmgr |
| kernel/src/vmar.rs | 350 | block | 页表 | unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_ |
| kernel/src/vmar.rs | 361 | block | 页表 | unsafe { slot.write_volatile(hal::page_descriptor(pa, attr, sh) |
| kernel/src/vmar.rs | 367 | block | 页表 | unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_ |
| kernel/src/vmar.rs | 443 | block | 页表 | unsafe { &mut *r.vmo }, r.vmo_off) }; // 幂等门：叶已在（前次供给）→ 零动作成功。 i |
| kernel/src/vmar.rs | 448 | block | 页表 | unsafe { slot.read_volatile() } & 1 != 0 { return Ok(()); } } le |
| kernel/src/vmar.rs | 468 | block | 页表 | unsafe { slot.write_volatile(hal::page_descriptor(pa, hal::ATTR_ |
| kernel/src/vmar.rs | 472 | block | 页表 | unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_ |
| kernel/src/vmar.rs | 517 | block | 页表 | unsafe { let r = &self.regions[idx]; (*r.vmo).untrack_map(r.vmo_ |
| kernel/src/vmar.rs | 526 | block | 页表 | unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_ |
| kernel/src/vmar.rs | 544 | block | 页表 | unsafe { let e = slot.read_volatile(); slot.write_volatile((e & |
| kernel/src/vmar.rs | 551 | block | 页表 | unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_ |
| kernel/src/vmar.rs | 610 | block | 页表 | unsafe { slot.read_volatile() }; if e & TABLE_BIT != 0 { Ok((e a |
| kernel/src/vmar.rs | 617 | block | 页表 | unsafe { slot.write_volatile(hal::table_descriptor(pa)) }; Ok(pa |
| kernel/src/vmar.rs | 624 | block | 页表 | unsafe { Self::table_of(self.l1_pa).add((va >> 30) & IDX_MASK) } |
| kernel/src/vmar.rs | 625 | block | 页表 | unsafe { Self::table_of(l2_pa).add((va >> 21) & IDX_MASK) })?; O |
| kernel/src/vmar.rs | 626 | block | 页表 | unsafe { Self::table_of(l3_pa).add((va >> 12) & IDX_MASK) }) } / |
| kernel/src/vmar.rs | 633 | block | 页表 | unsafe { l1.add((va >> 30) & IDX_MASK).read_volatile() }; if e0 |
| kernel/src/vmar.rs | 639 | block | 页表 | unsafe { l2.add((va >> 21) & IDX_MASK).read_volatile() }; if e1 |
| kernel/src/vmar.rs | 644 | block | 页表 | unsafe { l3.add((va >> 12) & IDX_MASK) }) } /// 清一段区间的 L3 叶（逐槽写 |
| kernel/src/vmar.rs | 654 | block | 页表 | unsafe { slot.write_volatile(0) }; } } } } // ---- asid → root V |
| kernel/src/vmar.rs | 796 | block | 页表 | unsafe { (*r.vmo).untrack_demand(r.len) }; } } if !held { hal::k |
| kernel/src/vmar.rs | 818 | block | 页表 | unsafe { l1.add((va >> 30) & IDX_MASK).read_volatile() }; if e0 |
| kernel/src/vmar.rs | 823 | block | 页表 | unsafe { l2.add((va >> 21) & IDX_MASK).read_volatile() }; if e1 |
| kernel/src/vmar.rs | 828 | block | 页表 | unsafe { l3.add((va >> 12) & IDX_MASK).read_volatile() }; (leaf |
| kernel/src/vmo.rs | 134 | block | 其它 | unsafe { core::ptr::copy_nonoverlapping( phys_to_virt(pa + off) |
| kernel/src/vmo.rs | 157 | block | 其它 | unsafe { core::ptr::copy_nonoverlapping( data.as_ptr().add(pos - |
| kernel/src/vmo.rs | 244 | block | 其它 | unsafe { core::ptr::write_bytes(phys_to_virt(pa) as *mut u8, 0, |
| mm/src/memmap.rs | 117 | fn | 其它 | unsafe fn desc_at(pa: u64, desc_size: u32, i: usize) -> &'static |
| mm/src/memmap.rs | 120 | block | 其它 | unsafe { &*(crate::va::phys_to_virt(p) as *const EfiMemdesc) } } |
| sched/src/band.rs | 90 | block | 其它 | unsafe { &raw mut BANDS[band as usize] }) } /// 带成员桥接：读 Task 的 i |
| sched/src/band.rs | 98 | block | 其它 | unsafe { band_member((*t).intent_effective, (*t).prio_effective, |
| sched/src/band.rs | 118 | block | 其它 | unsafe { *s = if let berry_bandface::ConfigOut::Armed { .. } = o |
| sched/src/band.rs | 135 | block | 其它 | unsafe { armed(&*s) } } /// 槽快照拷出（ktest / telemetry 观测面；越界 → Non |
| sched/src/band.rs | 147 | block | 其它 | unsafe { *s }; snap.deadline = DEADLINES[i].load(Ordering::Acqui |
| sched/src/band.rs | 168 | block | 其它 | unsafe { ((*s).quota, (*s).period) }; if quota == 0 \|\| period == |
| sched/src/band.rs | 173 | block | 其它 | unsafe { &*s }, cycles, prev); // 不可达 None：上面已界检 + 包络门（consumed_ |
| sched/src/band.rs | 179 | block | 其它 | unsafe { (*s).throttled = true }; } crossed } /// 事件驱动周期翻转：`now |
| sched/src/band.rs | 204 | block | 其它 | unsafe { ((*s).quota, (*s).period) }; if quota == 0 \|\| period == |
| sched/src/band.rs | 211 | block | 其它 | unsafe { &*s }, cur, now).unwrap_or(false) { return false; // 未到 |
| sched/src/band.rs | 222 | block | 其它 | unsafe { (*s).throttled = false }; // CAS 防双翻：只有把 deadline 从 cur |
| sched/src/band.rs | 251 | block | 其它 | unsafe { // §A136：先访当前节点，再推进并判回 head。 if task_member(t, band) && |
| sched/src/band.rs | 280 | block | 其它 | unsafe { // §A136：先访当前节点，再推进并判回 head。 if task_member(t, band) && |
| sched/src/class.rs | 125 | fn | 其它 | unsafe fn lazy_unthrottle_deadlines(rq: &Runqueue) { // 快路径：全域环上 |
| sched/src/class.rs | 186 | block | 其它 | unsafe { rq.link_back(NonNull::from(&mut *t)) }; // S-3a 义务①（子案 |
| sched/src/class.rs | 193 | block | 其它 | unsafe { rq.unlink(NonNull::from(&mut *t)) }; t.state = TaskStat |
| sched/src/class.rs | 203 | block | 其它 | unsafe { lazy_unthrottle_deadlines(rq) }; let chosen = self.scan |
| sched/src/class.rs | 206 | block | 其它 | unsafe { rq.advance_past(chosen) }; let t = unsafe { &mut *chose |
| sched/src/class.rs | 207 | block | 其它 | unsafe { &mut *chosen.as_ptr() }; // AI 策略注入留桩（3-4c）：出选后查表——未绑/空 |
| sched/src/class.rs | 220 | block | 其它 | unsafe { match hal::smp::last_vct_slot() { Some(p) => { let last |
| sched/src/class.rs | 305 | block | 其它 | unsafe { &*t }), t as *mut Task == my_cur) } /// W3-Q12：`&Task` |
| sched/src/class.rs | 344 | block | 其它 | unsafe { let mut m = 0u64; let mut cur = h; loop { let p = cur.a |
| sched/src/class.rs | 382 | block | 其它 | unsafe { rq.advance_past(chosen) }; let t = unsafe { &mut *chose |
| sched/src/class.rs | 383 | block | 其它 | unsafe { &mut *chosen.as_ptr() }; crate::policy::apply(t); Some( |
| sched/src/class.rs | 412 | block | 其它 | unsafe { let mut best: Option<NonNull<Task>> = None; let mut bes |
| sched/src/em.rs | 61 | block | 其它 | unsafe { *(&raw mut EM_TABLE[domain]) = d }; EM_VALID[domain].st |
| sched/src/em.rs | 78 | block | 其它 | unsafe { *(&raw const EM_TABLE[domain]) }) } /// 最高频点容量（overutil |
| sched/src/policy.rs | 208 | block | 其它 | unsafe { core::ptr::write(&raw mut SNAPS[w as usize], new) }; fe |
| sched/src/policy.rs | 240 | block | 其它 | unsafe { match idx { 0 => &raw const SNAPS[0], 1 => &raw const S |
| sched/src/policy.rs | 323 | block | 其它 | unsafe { (*active_snap()).gen } } /// 当前活跃快照的世代数（读点观察用）：未绑（哨兵 [` |
| sched/src/policy.rs | 387 | block | 其它 | unsafe { &*active_snap() }; if snap.count == 0 { return; // 空表/哨 |
| sched/src/policy.rs | 421 | block | 其它 | unsafe { &*active_snap() }; if snap.count == 0 { return 0; } let |
| sched/src/policy.rs | 457 | block | 其它 | unsafe { &*active_snap() }; if snap.count == 0 { return 0; // 未绑 |
| sched/src/policy.rs | 509 | block | 其它 | unsafe { &raw const SHD[slot as usize] } } /// shadow 侧 gen 观察（双 |
| sched/src/policy.rs | 519 | block | 其它 | unsafe { (*shadow_snap()).gen } } /// 绑 shadow 表（COMMIT + SHADOW |
| sched/src/policy.rs | 545 | block | 其它 | unsafe { core::ptr::write(&raw mut SHD[next as usize], snap); } |
| sched/src/policy.rs | 576 | block | 其它 | unsafe { if (*src).gen == 0 { policy_unlock(); // 错误路径同解锁（false |
| sched/src/policy.rs | 669 | block | 其它 | unsafe { &*shadow_snap() }; if shd.gen == 0 { return; // shadow |
| sched/src/policy.rs | 674 | block | 其它 | unsafe { &*active_snap() }; // key 0（内核任务）：lookup_o1 早退返 None → |
| sched/src/runqueue.rs | 122 | fn | 其它 | unsafe fn lock_acquire(&self) { self.lock.acquire(); } /// 放本队列自 |
| sched/src/runqueue.rs | 166 | block | 其它 | unsafe { let mut cur = h; let mut sum: u128 = 0; loop { sum += ( |
| sched/src/runqueue.rs | 204 | block | 其它 | unsafe { let mut cur = h; let mut steps = 0; let mut vsum: u128 |
| sched/src/runqueue.rs | 257 | block | 其它 | unsafe { let mut cur = h; let mut steps = 0; let mut n = 0; loop |
| sched/src/runqueue.rs | 287 | fn | 其它 | unsafe fn link_back(&self, task: NonNull<Task>) { let t = task.a |
| sched/src/runqueue.rs | 342 | fn | 其它 | unsafe fn unlink(&self, task: NonNull<Task>) { let t = task.as_p |
| sched/src/runqueue.rs | 404 | fn | 其它 | unsafe fn advance_past(&self, node: NonNull<Task>) { // SAFETY: |
| sched/src/runqueue.rs | 428 | block | 其它 | unsafe { let mut cur = h; loop { if crate::class::eligible(my_cu |
| sched/src/sched.rs | 90 | fn | 其它 | unsafe fn current_task() -> *mut Task { match hal::smp::current_ |
| sched/src/sched.rs | 101 | fn | 其它 | unsafe fn set_current(t: *mut Task) { match hal::smp::current_sl |
| sched/src/sched.rs | 294 | fn | 其它 | unsafe fn rq() -> &'static mut Runqueue { rq_of(hal::smp::cpu_id |
| sched/src/sched.rs | 302 | fn | 其它 | unsafe fn rq_of(cpu: usize) -> &'static mut Runqueue { let arr: |
| sched/src/sched.rs | 310 | fn | 其它 | unsafe fn topo() -> &'static Topo { &*addr_of_mut!(TOPO) } // -- |
| sched/src/sched.rs | 342 | block | 其它 | unsafe { KLOCK_OPS = Some(ops) }; } /// tick/IRQ 语境直取 KERN_LOCK（ |
| sched/src/sched.rs | 350 | block | 其它 | unsafe { KLOCK_OPS } { (o.kern_acquire)(); } } /// 舞蹈位感知 KL 臂直放 |
| sched/src/sched.rs | 358 | block | 其它 | unsafe { KLOCK_OPS } { (o.kern_release)(); } } /// PRINT_LOCK 取（ |
| sched/src/sched.rs | 366 | block | 其它 | unsafe { KLOCK_OPS } { (o.print_lock)(); } } /// PRINT_LOCK 放。 f |
| sched/src/sched.rs | 374 | block | 其它 | unsafe { KLOCK_OPS } { (o.print_unlock)(); } } /// 切走前分诊（舞蹈步「放」） |
| sched/src/sched.rs | 389 | fn | 其它 | unsafe fn klock_release_switch(task: *mut Task) { let held = (*t |
| sched/src/sched.rs | 414 | fn | 其它 | unsafe fn klock_acquire_switch(task: *mut Task) { let held = (*t |
| sched/src/sched.rs | 447 | fn | 其它 | unsafe extern "C" fn svc_window(class: u8, enter: bool) { let cu |
| sched/src/sched.rs | 495 | fn | 其它 | unsafe fn held_mark(bit: u8, on: bool) { let cur = current_task( |
| sched/src/sched.rs | 510 | fn | 其它 | unsafe fn held_test(bit: u8) -> bool { let cur = current_task(); |
| sched/src/sched.rs | 543 | fn | 其它 | unsafe fn rq_lock_of(cpu: usize) -> bool { if held_test(held_rq_ |
| sched/src/sched.rs | 558 | fn | 其它 | unsafe fn rq_unlock_of(cpu: usize, phys: bool) { if !phys { retu |
| sched/src/sched.rs | 572 | fn | 其它 | unsafe fn rq_lock() -> bool { rq_lock_of(hal::smp::cpu_id()) } / |
| sched/src/sched.rs | 579 | fn | 其它 | unsafe fn rq_unlock(phys: bool) { rq_unlock_of(hal::smp::cpu_id( |
| sched/src/sched.rs | 626 | fn | 其它 | unsafe fn rq_acquire(idx: usize) { #[cfg(feature = "lockdep")] h |
| sched/src/sched.rs | 641 | fn | 其它 | unsafe fn rq_release(idx: usize) { #[cfg(feature = "lockdep")] h |
| sched/src/sched.rs | 656 | block | 其它 | unsafe { set_current(addr_of_mut!(IDLE_TASK)); // S-3b：capacity |
| sched/src/sched.rs | 676 | block | 其它 | unsafe { let t = current_task(); if t.is_null() { return (0, 0); |
| sched/src/sched.rs | 724 | block | 其它 | unsafe { let t = &mut *addr_of_mut!(TOPO); t.nr_cpus = 2; t.capa |
| sched/src/sched.rs | 759 | block | 其它 | unsafe { (rq_of(me).len(), rq_of(other).len()) }; if mine >= the |
| sched/src/sched.rs | 772 | fn | 其它 | unsafe fn enqueue_with_kick(task: &mut Task, cpu: usize) { task. |
| sched/src/sched.rs | 782 | block | MMIO | unsafe { hal::gic::send_sgi( hal::gic::SGI_RESCHEDULE, hal::gic: |
| sched/src/sched.rs | 809 | block | 其它 | unsafe { enqueue_with_kick(task, place(task)); } } pub fn spawn_ |
| sched/src/sched.rs | 819 | block | 其它 | unsafe { enqueue_with_kick(task, place(task)); } } /// 同 [`spawn |
| sched/src/sched.rs | 834 | block | 其它 | unsafe { enqueue_with_kick(task, place(task)); } } /// 同 [`spawn |
| sched/src/sched.rs | 854 | block | 其它 | unsafe { enqueue_with_kick(task, place(task)); } } /// 生成一个**用户* |
| sched/src/sched.rs | 902 | fn | 其它 | unsafe fn enqueue_prepared_user(task_ptr: usize) { let task = ta |
| sched/src/sched.rs | 905 | block | 其它 | unsafe { &*task }); // SAFETY: 同 try_spawn_user 旧路径。 unsafe { en |
| sched/src/sched.rs | 907 | block | 其它 | unsafe { enqueue_with_kick(&mut *task, cpu) }; } pub fn try_spaw |
| sched/src/sched.rs | 918 | block | 其它 | unsafe { enqueue_prepared_user(task_ptr) }; Some(task_ptr) } /// |
| sched/src/sched.rs | 927 | block | 其它 | unsafe { current_task() }; if cur.is_null() { 0 } else { unsafe |
| sched/src/sched.rs | 931 | block | 其它 | unsafe { (*cur).proc_tag } } } /// 当前任务钉住的 Process 对象键（C-4b `pro |
| sched/src/sched.rs | 941 | block | 其它 | unsafe { current_task() }; if cur.is_null() { 0 } else { unsafe |
| sched/src/sched.rs | 945 | block | 其它 | unsafe { (*cur).pinned } } } /// 1.1/S3-a：冻结前置谓词——按钉住键扫描，任一任务命中即 |
| sched/src/sched.rs | 962 | block | 其它 | unsafe { // app-1 段 a-1（2026-11-08 实证）：svc_mask 只在 SVC 窗口出口清零 // |
| sched/src/sched.rs | 1034 | block | 其它 | unsafe { // SLEEPQ 扫描 let mut cur = sleepq().head.get(); while l |
| sched/src/sched.rs | 1071 | block | 其它 | unsafe { let mut cur = sleepq().head.get(); while let Some(c) = |
| sched/src/sched.rs | 1108 | block | 其它 | unsafe { enqueue_with_kick(task, place(task)); } } /// 4-3c：send |
| sched/src/sched.rs | 1118 | block | 其它 | unsafe { current_task() }; if cur.is_null() { None } else { Some |
| sched/src/sched.rs | 1122 | block | 其它 | unsafe { &*cur }, now_ns)) } } /// 4-3c：recv 侧 intent 结算（slowpat |
| sched/src/sched.rs | 1136 | block | 其它 | unsafe { current_task() }; if cur.is_null() { None } else { // S |
| sched/src/sched.rs | 1141 | block | 其它 | unsafe { &mut *cur }; let out = crate::intent::settle(t, m, opt_ |
| sched/src/sched.rs | 1159 | fn | 上下文切换 | unsafe fn activate_if_changed(prev: *mut Task, next: *mut Task) |
| sched/src/sched.rs | 1186 | block | 其它 | unsafe { let prev = current_task(); // RQ 窗（c-2b）：出选 + 状态定形同窗——n |
| sched/src/sched.rs | 1268 | block | 上下文切换 | unsafe { switch_away_forever_with(0) } } /// 当前任务带退出码退出（Plan B v |
| sched/src/sched.rs | 1276 | block | 上下文切换 | unsafe { switch_away_forever_with(code) } } /// 用户任务致死计数（G1.5 验收 |
| sched/src/sched.rs | 1299 | fn | 其它 | unsafe extern "C" fn user_fault_exit() -> ! { let cur = current_ |
| sched/src/sched.rs | 1318 | fn | 其它 | unsafe fn assert_exit_hook_windows_closed() { let cur = current_ |
| sched/src/sched.rs | 1344 | fn | 上下文切换 | unsafe fn switch_away_forever_with(code: i32) -> ! { let cur = c |
| sched/src/sched.rs | 1430 | block | 其它 | unsafe { EXIT_HOOK = Some(f) }; } /// 待回收任务队列（cap 32 ≈ 一个 bsh 会话 |
| sched/src/sched.rs | 1450 | fn | 其它 | unsafe fn reapq_push(t: *mut Task) { let phys = !held_test(HELD_ |
| sched/src/sched.rs | 1487 | block | 其它 | unsafe { let phys = !held_test(HELD_REAPQ); // 位感知退化（c-2c slice |
| sched/src/sched.rs | 1547 | fn | 其它 | unsafe fn kill_all_tasks_by_pinned( pinned: usize, code: i32, ) |
| sched/src/sched.rs | 1556 | block | 其它 | unsafe { loop { // ① SLEEPQ 扫描（Blocked：wait_one/wait_many/纯 slee |
| sched/src/sched.rs | 1684 | block | 上下文切换 | unsafe { SWITCH_HOOK = hook }; } /// 切换位点共用的遥测发射点：在 `CURRENT = n |
| sched/src/sched.rs | 1692 | fn | 其它 | unsafe fn note_switch(prev: *mut Task, next: *mut Task, kind: Sw |
| sched/src/sched.rs | 1706 | block | 其它 | unsafe { let hook = SWITCH_HOOK; if let Some(h) = hook { h(&*pre |
| sched/src/sched.rs | 1727 | block | 其它 | unsafe { WAKE_HOOK = hook }; } /// 两个唤醒位共用的遥测发射点：在任务**摘队、置因、入 ru |
| sched/src/sched.rs | 1734 | fn | 其它 | unsafe fn note_wake(t: *mut Task, reason: WakeReason) { // SAFET |
| sched/src/sched.rs | 1739 | block | MMIO | unsafe { (*t).wake_vct.set(hal::timer::cntvct()); } let hook = u |
| sched/src/sched.rs | 1742 | block | 其它 | unsafe { WAKE_HOOK }; if let Some(h) = hook { h(unsafe { &*t }, |
| sched/src/sched.rs | 1744 | block | 其它 | unsafe { &*t }, reason); } } /// 取走**当前任务**的唤醒时刻戳（G3-4 bench-lat |
| sched/src/sched.rs | 1764 | block | 其它 | unsafe { let cur = current_task(); let v = (*cur).wake_vct.get() |
| sched/src/sched.rs | 1786 | block | 其它 | unsafe { SLEEPQ_HOOK = hook }; } /// 睡眠者集合变更的发射点（见 [`SleepqHook` |
| sched/src/sched.rs | 1792 | fn | 其它 | unsafe fn note_sleepq_changed() { let hook = unsafe { SLEEPQ_HOO |
| sched/src/sched.rs | 1793 | block | 其它 | unsafe { SLEEPQ_HOOK }; if let Some(h) = hook { h(); } } // ---- |
| sched/src/sched.rs | 1813 | block | 其它 | unsafe { LB_MATCH = f }; } /// per-CPU LB 子任务 tick 采样计数（义务③ per- |
| sched/src/sched.rs | 1875 | fn | 其它 | unsafe fn sleepq() -> &'static mut SleepQueue { &mut *addr_of_mu |
| sched/src/sched.rs | 1993 | block | 其它 | unsafe { held_test(HELD_SQ) } { return false; // 位已置——外层逻辑持有，物理退 |
| sched/src/sched.rs | 1998 | block | 其它 | unsafe { held_mark(HELD_SQ, true) }; true } /// 放 SLEEPQ 锁（先清位后放 |
| sched/src/sched.rs | 2009 | block | 其它 | unsafe { held_mark(HELD_SQ, false) }; sq_release(); } impl Sleep |
| sched/src/sched.rs | 2019 | fn | 其它 | unsafe fn insert_sorted(&self, task: NonNull<Task>) { let t = ta |
| sched/src/sched.rs | 2078 | fn | 其它 | unsafe fn unlink(&self, task: NonNull<Task>) { let t = task.as_p |
| sched/src/sched.rs | 2117 | block | 其它 | unsafe { assert_preempt_balanced("block_current"); let cur = cur |
| sched/src/sched.rs | 2228 | fn | 其它 | unsafe fn wake_one_inner(t: NonNull<Task>) { // SAFETY: 契约如上；SLE |
| sched/src/sched.rs | 2230 | block | 其它 | unsafe { let p = t.as_ptr(); sleepq().unlink(t); (*p).wakeup.set |
| sched/src/sched.rs | 2305 | block | 其它 | unsafe { let mut cur = sleepq().head.get(); while let Some(c) = |
| sched/src/sched.rs | 2324 | block | 其它 | unsafe { note_sleepq_changed() }; n } // ---- fastpath/slowpath |
| sched/src/sched.rs | 2361 | block | 其它 | unsafe { let mut cur = sleepq().head.get(); while let Some(c) = |
| sched/src/sched.rs | 2399 | fn | 其它 | unsafe fn fast_wake_locked(t: NonNull<Task>) { // SAFETY: 契约如上。 |
| sched/src/sched.rs | 2401 | block | 其它 | unsafe { wake_one_inner(t); note_sleepq_changed(); } } /// 4-4 直 |
| sched/src/sched.rs | 2429 | block | 其它 | unsafe { assert_preempt_balanced("try_preempt"); let cur = curre |
| sched/src/sched.rs | 2490 | fn | MMIO | unsafe fn expire_timeouts() { let now = hal::timer::cntvct(); le |
| sched/src/sched.rs | 2506 | block | 其它 | unsafe { note_sleepq_changed() }; sq_unlock(phys); // SQ 窗闭 } // |
| sched/src/sched.rs | 2520 | block | 其它 | unsafe { let mut cur = sleepq().head.get(); while let Some(n) = |
| sched/src/sched.rs | 2545 | block | 其它 | unsafe { sleepq().head.get() == Some(core::ptr::NonNull::from(t) |
| sched/src/sched.rs | 2555 | block | 其它 | unsafe { sleepq().insert_sorted(NonNull::from(t)) }; } /// 把 `t` |
| sched/src/sched.rs | 2562 | block | 其它 | unsafe { sleepq().unlink(NonNull::from(t)) }; } /// 用户任务致死计数只读访问 |
| sched/src/sched.rs | 2580 | block | 其它 | unsafe { core::slice::from_raw_parts(p, len) } } /// 环/SLEEPQ 完整 |
| sched/src/sched.rs | 2602 | block | 其它 | unsafe { // S-3b：双队列各自走查对账（per-CPU 分诊后账面 = 每核独立环 + 全局 // SLEEPQ |
| sched/src/sched.rs | 2694 | block | 其它 | unsafe { let phys = rq_lock(); let picked = RR .pick_next_filter |
| sched/src/sched.rs | 2705 | block | 其它 | unsafe { try_steal_one() }) }; match picked { Some(next_ptr) => |
| sched/src/sched.rs | 2714 | block | 其它 | unsafe { dispatch_from_idle(next_ptr) }; } None => { // 深浅 park（ |
| sched/src/sched.rs | 2725 | block | MMIO | unsafe { hal::timer::arm_at(d) }, // SAFETY: CNTV IMASK=1（深 park |
| sched/src/sched.rs | 2727 | block | MMIO | unsafe { hal::timer::mask() }, } // masked-wfi 三步闭环（丢唤醒窗收口，secon |
| sched/src/sched.rs | 2745 | block | 其它 | unsafe { rq().is_empty() && sleepq().len.get() == 0 }; if empty |
| sched/src/sched.rs | 2749 | block | MMIO | unsafe { hal::timer::mask() }; hal::smp::note_park_cpu(); // S-5 |
| sched/src/sched.rs | 2758 | block | MMIO | unsafe { hal::timer::arm_slice() }; hal::enable_irq(); yield_now |
| sched/src/sched.rs | 2784 | block | 其它 | unsafe { current_task() }; if cur.is_null() { 0 } else { unsafe |
| sched/src/sched.rs | 2785 | block | 其它 | unsafe { (*cur).thread_key } } } /// 1.1/v1.2：当前任务的运行时 deadline |
| sched/src/sched.rs | 2792 | block | 其它 | unsafe { current_task() }; if cur.is_null() { return (0, 0); } u |
| sched/src/sched.rs | 2796 | block | 其它 | unsafe { if (*cur).prio_effective < crate::class::PRIO_DEADLINE |
| sched/src/sched.rs | 2808 | block | 其它 | unsafe { // app-1 段 a-1 同款修正（见 freeze_precheck_by_pinned 注）：svc_ |
| sched/src/sched.rs | 2840 | block | 其它 | unsafe { for cpu in 0..2 { let head = rq_of(cpu).head(); let mut |
| sched/src/sched.rs | 2872 | block | 其它 | unsafe { for cpu in 0..2 { let head = rq_of(cpu).head(); let mut |
| sched/src/sched.rs | 2898 | fn | 其它 | unsafe extern "C" fn on_irq_tick(_tf: &mut hal::trap::TrapFrame) |
| sched/src/sched.rs | 2932 | block | 其它 | unsafe { LB_MATCH } { if !cur.is_null() && f((*cur).thread_key) |
| sched/src/sched.rs | 3003 | block | 其它 | unsafe { core::slice::from_raw_parts(hits[i].0 as *const u8, hit |
| sched/src/sched.rs | 3033 | block | 其它 | unsafe { (rq_of(me).len(), rq_of(1 - me).len()) }; // 定时拉取只做“近空闲 |
| sched/src/sched.rs | 3043 | block | 其它 | unsafe { try_steal_one() } { let st = PULL_CNT.fetch_add(1, Orde |
| sched/src/sched.rs | 3136 | fn | 其它 | unsafe extern "C" fn on_sgi_resched(_tf: &mut hal::trap::TrapFra |
| sched/src/sched.rs | 3219 | fn | 其它 | unsafe fn park_recheck() { if !hal::smp::par_enabled() { return; |
| sched/src/sched.rs | 3311 | fn | 其它 | unsafe fn try_steal_one() -> Option<*mut Task> { if !hal::smp::p |
| sched/src/sched.rs | 3390 | fn | MMIO | unsafe fn dispatch_from_park_ctx(next_ptr: *mut Task) { (*next_p |
| sched/src/sched.rs | 3394 | block | MMIO | unsafe { hal::timer::arm_slice() }; set_current(next_ptr); bump_ |
| sched/src/sched.rs | 3405 | block | 上下文切换 | unsafe { hal::task::context_switch( hal::smp::sec_park_ctx(), &r |
| sched/src/sched.rs | 3424 | fn | 其它 | unsafe fn dispatch_from_idle(next_ptr: *mut Task) { // G3-1a 断言补 |
| sched/src/sched.rs | 3433 | block | MMIO | unsafe { hal::timer::arm_slice() }; set_current(next_ptr); bump_ |
| sched/src/sched.rs | 3441 | block | 上下文切换 | unsafe { hal::task::context_switch(&raw mut (*idle).ctx, &raw co |
| sched/src/sched.rs | 3461 | block | 其它 | unsafe { sleepq().head.get().map(\|h\| (*h.as_ptr()).wait_deadline |
| sched/src/spin.rs | 24 | block | 其它 | unsafe { SPIN_BARK = Some(f) }; } /// TTAS 自旋锁。`locked == false` |
| sched/src/spin.rs | 56 | block | 其它 | unsafe { SPIN_BARK } { bark(self as *const SpinLock as usize); } |
| sched/src/task.rs | 15 | block | 分配器 | unsafe { alloc::alloc::alloc(layout) } as *mut T; if ptr.is_null |
| sched/src/task.rs | 20 | block | 分配器 | unsafe { ptr.write(val) }; // SAFETY: 该块按全局分配器 + for_value 布局分配， |
| sched/src/task.rs | 23 | block | 分配器 | unsafe { Box::from_raw(ptr) }) } /// 内核线程的默认栈大小（16KiB）。v1 切换在 IR |
<!-- UNSAFE-AUDIT:REGISTRY:END -->

## 四、漏洞分类账维度（SEC-8 §5.1-2——没有维度就没有未来的曲线）

对拍物 = Android 2019 年先建类别化基线、2022 年画出 76%→35% 曲线的方法论
（`research/sec-notes/SEC-8-Rust与内存安全现状.md` §3.1/§5.1）。**从现在起**
issue/回归/审查 finding 一律带下列标签之一（可多标），哪怕长期记 0——维度
day-one 立住，「全 Rust」的安全叙事才有可出示的曲线起点。

| 标签 | 口径（什么进什么不进） | 现状计数 | 备注 |
|------|----------------------|---------|------|
| memory-safety | 走出 unsafe 契约的内存错（UAF/越界/悬垂/数据竞争/未初始化读）——含 C-ABI 桩面布局错（errno 档位/出参宽度/Stat 布局三案若再现即记此） | 0 | 三案均已在当刀闭环修复（见 §五），未流出为漏洞 |
| logic | 语义/时序错（竞速标记、饿死、协议状态机错序、off-by-one） | 0 | 台账只记「已确认的缺陷」；对抗审查抓出即修的记当刀复盘，不重复入账 |
| 协议 | 跨进程/跨端点 wire 面错（编解码不对称、畸形拒面缺口、会话错配） | 0 | 协议 crate host 测试族（psproto/netdproto 等）是第一防线 |
| 供应链 | 依赖投毒/typosquat/弃维护/advisory 命中 | 0 | 防线 = cargo-deny CI（件 23）+ Cargo.lock 全量入库 + uu 家族 =0.12.0 锁步 |
| 工具链 | 构建链/产物链错（elf2pe 槽位案、基准漂移案类） | 0 | 防线 = 双实现共识 fixture 守卫（mkfat --check-fixture 家族） |

> 计数更新纪律：确认缺陷当刀修完 → 该标签计数不加（修在合并前）；流出后才
> 入账并附 issue/复盘链接。本表手维护，不进生成块。

## 五、C-ABI 桩面同源审计（件 23 后半——unsafe 同级 review 面）

本表扫描面是内核侧 crate；BerryOS 的隐性 C 边界还有一块在 **berry-libc 桩与
`#[repr(C)]` 结构**（user 侧），列入 unsafe 同级审计口径：

- **三案实抓存档**（均已闭环，作为该面的「已知错误形态库」）：
  ① errno 桩档位案——`xattr` 族桩给 ENOSYS(38) 而 uucore 白名单只吞 95
  （桩的「正确 errno」由调用方错误分类代码定义）；
  ② 出参宽度案——handle 出参一律 `copy_out_u32`，曾 `copy_out_u64` 砸穿
  调用方 4 字节槽邻接局部（ABI §8 handle=u32）；
  ③ Stat 布局案——musl aarch64 asm-generic 字段序（st_mode@16）与 x86_64
  版同尺寸不同序，size 类消费者恒绿、mode 类首个消费者才显形；修法 = 布局
  定义处 `offset_of!` 全字段编译期钉板。
- **既有防线**：`Stat` 20 字段 offset_of! 钉板（fs.rs const assert 族）；
  get*_r 四桩签名对齐 libc crate 真原型；`tools/shim_audit.py` 符号面审计。
- **挂账**：桩函数签名对照表全量化（桩原型 vs libc crate 原型逐条对拍）——
  随 M-ECO A3 socket 桩批扩面时一并落。

## 六、维护流程

1. 改内核侧代码涉 unsafe 增删/移行 → `python3 tools/unsafe_audit.py` 重产
   → 台账 diff 随源入册（新出现点由工具规则给初判分类，人工复核 §三 表列）。
2. 改判分类/说明：直接编辑 §三 表格对应列（行号/增删行勿手改），下次重产按
   内容锚自动继承。
3. CI（ci.yml「unsafe 台账对拍」step）跑 `--check`：红 = 忘重产或手改越界；
   通过行顺打当前密度锚（趋势线在 CI run 史 + 本文档 git 史双面可读）。
4. 提交定序（多会话并行纪律）：台账基线 = 随行源码树（生成时的工作树快照），
   不是 HEAD——台账必须与其快照的内核源码同批（或其后）提交；先于源码单独
   入册（如快照含兄弟会话未提交 WIP）= CI `--check` 对提交树必红（表有登记
   而源码缺）。
5. 密度趋势：看本文档 `git log -p` 的 §二 diff。
