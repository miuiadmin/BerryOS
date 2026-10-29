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
  - **上下文切换**：切换 asm/任务上下文装配/异常向量与帧保存（CLAUDE.md
    「上下文切换/寄存器保存」簇；trap 向量表与 x86 IDT/TSS/syscall 入口 MSR 归此）。
  - **MMIO**：设备 MMIO + **系统寄存器/特权指令**（mrs/msr/wfi/hvc/out/in/
    rdmsr…——按 CLAUDE.md「MMIO/寄存器访问」簇归并；纯设备 MMIO 与系统寄存器
    未拆两列，口径如实注记）。
  - **分配器**：SlabHeap/GlobalAlloc/freelist/帧分配器静态/Task Box 分配基元。
  - **硬件原子**：手写原子 asm（ldxr/stxr/casa/outline-atomics 面）——**现状
    0 处**（原子操作全走 `core::sync::atomic` 安全面）；类别保留占位。
  - **其它**：裸指针解引用/静态可变槽/对象互钉/ABI 出入参拷贝/内存基元/
    测试与基准面/遥测环静态——CLAUDE.md「外层包安全 API」的包裹层主体。
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
| kernel/ | 20 | 13086 | 862 | 6.59% |
| mm/ | 6 | 1578 | 4 | 0.25% |
| sched/ | 7 | 2168 | 549 | 25.32% |
| ipc/ | 0 | 0 | 0 | — |
| arch/ | 15 | 3999 | 536 | 13.40% |
| **合计** | 48 | 20831 | 1951 | **9.37%** |

- 剔除 ktest.rs/bench.rs（测试/基准面）：总行 17227，unsafe 非空行 1714，密度 9.95%（生产面口径）
- 分类计数：页表 43 | 上下文切换 23 | MMIO 76 | 分配器 13 | 硬件原子 0 | 其它 401
- 类型计数：block 479 | fn 74 | impl 2 | extern 1（另有类型签名位提及 6 处——只计不登记，防双计）
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
| arch/aarch64/src/gic.rs | 68 | fn | MMIO | unsafe fn init() { // ---- distributor：先关，再清状态 ---- mmio::write( |
| arch/aarch64/src/gic.rs | 93 | block | MMIO | unsafe { let en = mmio::read(GICD + GICD_ISENABLER0) & (1 << IRQ |
| arch/aarch64/src/gic.rs | 106 | block | MMIO | unsafe { mmio::read(GICC + GICC_IAR) & 0x3ff } } /// 写 EOIR 结束（必 |
| arch/aarch64/src/gic.rs | 112 | block | MMIO | unsafe { mmio::write(GICC + GICC_EOIR, irq_id & 0x3ff) }; } // - |
| arch/aarch64/src/gic.rs | 117 | fn | MMIO | unsafe fn set_priority(irq: usize, prio: u8) { mmio::write_u8(GI |
| arch/aarch64/src/gic.rs | 121 | fn | MMIO | unsafe fn set_target(irq: usize, target: u8) { mmio::write_u8(GI |
| arch/aarch64/src/gic.rs | 129 | fn | MMIO | unsafe fn set_config(irq: usize, cfg: u32) { let shift = 2 * (ir |
| arch/aarch64/src/gic.rs | 148 | fn | MMIO | unsafe fn enable_spi(irq: usize, prio: u8, target: u8, cfg: u32) |
| arch/aarch64/src/gic.rs | 173 | block | MMIO | unsafe { enable_spi(r_irq, r_prio, r_target, r_cfg) }; return tr |
| arch/aarch64/src/lib.rs | 69 | block | 上下文切换 | unsafe { trap::init() }; // SAFETY: MMIO（Device 映射区）；EL1、单核、DAIF |
| arch/aarch64/src/lib.rs | 71 | block | MMIO | unsafe { gic::init() }; } /// 高半别名自校验（2b-2 起只走高半——低半身份已退役）：RAM 魔 |
| arch/aarch64/src/lib.rs | 91 | block | 页表 | unsafe { mmu::activate_ttbr0(ttbr0_pa, asid) }; } /// 2b-2：低半身份表 |
| arch/aarch64/src/lib.rs | 98 | block | 页表 | unsafe { mmu::retire_low_half() }; } /// 全量 ASID 失效（ASID 分配器回卷时调 |
| arch/aarch64/src/lib.rs | 104 | block | 页表 | unsafe { mmu::flush_all_asid() }; } /// 2b-3：按 ASID 失效 TLB——Vmar |
| arch/aarch64/src/lib.rs | 111 | block | 页表 | unsafe { mmu::tlbi_aside1(asid) }; } /// 3-2：读当前 TTBR0_EL1 表基 PA |
| arch/aarch64/src/lib.rs | 144 | block | 其它 | unsafe { IRQ_DISPATCH = Some(f) }; } /// 取已注册的 IRQ 派发回调（`excepti |
| arch/aarch64/src/lib.rs | 151 | fn | 其它 | unsafe fn irq_dispatch() -> Option<IrqDispatch> { IRQ_DISPATCH } |
| arch/aarch64/src/lib.rs | 169 | block | 其它 | unsafe { USER_FAULT_EXIT = Some(f) }; } /// 取已注册的用户故障击杀回调（`excep |
| arch/aarch64/src/lib.rs | 176 | fn | 其它 | unsafe fn user_fault_exit() -> Option<UserFaultExit> { USER_FAUL |
| arch/aarch64/src/lib.rs | 183 | block | MMIO | unsafe { core::arch::asm!("msr daifclr, #2", options(nostack, pr |
| arch/aarch64/src/lib.rs | 189 | block | MMIO | unsafe { core::arch::asm!("msr daifset, #2", options(nostack, pr |
| arch/aarch64/src/lib.rs | 195 | block | MMIO | unsafe { core::arch::asm!("wfi", options(nostack, preserves_flag |
| arch/aarch64/src/lib.rs | 203 | block | MMIO | unsafe { core::arch::asm!("wfi") }; } } |
| arch/aarch64/src/mmio.rs | 5 | fn | MMIO | unsafe fn write(addr: usize, val: u32) { (addr as *mut u32).writ |
| arch/aarch64/src/mmio.rs | 12 | fn | MMIO | unsafe fn read(addr: usize) -> u32 { (addr as *const u32).read_v |
| arch/aarch64/src/mmio.rs | 18 | fn | MMIO | unsafe fn write_u8(addr: usize, val: u8) { (addr as *mut u8).wri |
| arch/aarch64/src/mmio.rs | 24 | fn | MMIO | unsafe fn read_u8(addr: usize) -> u8 { (addr as *const u8).read_ |
| arch/aarch64/src/mmio.rs | 30 | fn | MMIO | unsafe fn write_u16(addr: usize, val: u16) { (addr as *mut u16). |
| arch/aarch64/src/mmu.rs | 88 | fn | 页表 | unsafe extern "C" fn build_boot_pt() { // `&raw mut`：取静态裸指针，避开 ` |
| arch/aarch64/src/mmu.rs | 142 | fn | 页表 | unsafe extern "C" fn enable_mmu() { write_mair_el1(MAIR_EL1); wr |
| arch/aarch64/src/mmu.rs | 169 | block | 页表 | unsafe { (va as *const u64).read_volatile() } == PROBE_MAGIC; co |
| arch/aarch64/src/mmu.rs | 172 | block | 页表 | unsafe { crate::mmio::read(KERNEL_VA_BASE + GICD_PIDR2) }; // PI |
| arch/aarch64/src/mmu.rs | 185 | block | 页表 | unsafe { asm!("mrs {0}, id_aa64mmfr0_el1", out(reg) v, options(n |
| arch/aarch64/src/mmu.rs | 201 | fn | 页表 | unsafe fn tlbi_aside1(asid: u16) { let shift = asid_shift(); asm |
| arch/aarch64/src/mmu.rs | 214 | fn | 页表 | unsafe fn activate_ttbr0(ttbr0_pa: usize, asid: u16) { let table |
| arch/aarch64/src/mmu.rs | 240 | block | 页表 | unsafe { asm!("mrs {0}, ttbr0_el1", out(reg) ttbr, options(nosta |
| arch/aarch64/src/mmu.rs | 249 | fn | 页表 | unsafe fn retire_low_half() { if !RETIRE_IDENTITY { return; } ac |
| arch/aarch64/src/mmu.rs | 260 | fn | 页表 | unsafe fn flush_all_asid() { asm!("tlbi vmalle1is", options(nost |
| arch/aarch64/src/mmu.rs | 269 | fn | 页表 | unsafe fn write_mair_el1(v: u64) { asm!("msr mair_el1, {0}", in( |
| arch/aarch64/src/mmu.rs | 273 | fn | 页表 | unsafe fn write_tcr_el1(v: u64) { asm!("msr tcr_el1, {0}", in(re |
| arch/aarch64/src/mmu.rs | 277 | fn | 页表 | unsafe fn write_ttbr0_el1(v: u64) { asm!("msr ttbr0_el1, {0}", i |
| arch/aarch64/src/mmu.rs | 281 | fn | 页表 | unsafe fn write_ttbr1_el1(v: u64) { asm!("msr ttbr1_el1, {0}", i |
| arch/aarch64/src/mmu.rs | 285 | fn | 页表 | unsafe fn write_sctlr_el1(v: u64) { asm!("msr sctlr_el1, {0}", i |
| arch/aarch64/src/mmu.rs | 289 | fn | 页表 | unsafe fn isb() { asm!("isb", options(nostack, preserves_flags)) |
| arch/aarch64/src/mmu.rs | 293 | fn | 页表 | unsafe fn dsb_ish() { asm!("dsb ish", options(nostack, preserves |
| arch/aarch64/src/smp.rs | 285 | block | MMIO | unsafe { (&raw const BOOT_EL).read_volatile() } } fn cntvct() -> |
| arch/aarch64/src/smp.rs | 291 | block | MMIO | unsafe { asm!("mrs {}, cntvct_el0", out(reg) v) }; v } fn cntfrq |
| arch/aarch64/src/smp.rs | 298 | block | MMIO | unsafe { asm!("mrs {}, cntfrq_el0", out(reg) v) }; v } /// 状态谓词轮 |
| arch/aarch64/src/smp.rs | 341 | block | MMIO | unsafe { (&raw const __smp_sec_entry).read_volatile() } as usize |
| arch/aarch64/src/smp.rs | 348 | block | MMIO | unsafe { asm!( "hvc #0", inout("x0") 0xC400_0003u64 => ret, // P |
| arch/aarch64/src/smp.rs | 374 | block | MMIO | unsafe { (&raw const SMP_SEC_REPORT).read_volatile() }; uart_put |
| arch/aarch64/src/smp.rs | 388 | block | MMIO | unsafe { (&raw const SMP_SEC_REPORT).read_volatile() }; uart_put |
| arch/aarch64/src/smp.rs | 418 | block | MMIO | unsafe { asm!("dsb sy", "sev") }; if !wait_secs(5, \|\| SMP_HB2.lo |
| arch/aarch64/src/syscall.rs | 41 | block | 其它 | unsafe { (*(&raw mut SYSCALLS))[nr] = h }; } /// 派发入口（`exception |
| arch/aarch64/src/syscall.rs | 52 | block | 其它 | unsafe { (*table)[nr](tf) } } else { ERR_NOT_SUPPORTED }; tf.reg |
| arch/aarch64/src/task.rs | 190 | fn | 上下文切换 | unsafe fn context_switch(prev: *mut TaskContext, next: *const Ta |
| arch/aarch64/src/timer.rs | 33 | fn | MMIO | unsafe fn init() -> u64 { FREQ = read_cntfrq(); SLICE = FREQ / S |
| arch/aarch64/src/timer.rs | 66 | fn | MMIO | unsafe fn arm_slice() { let deadline = cntvct() + SLICE; core::a |
| arch/aarch64/src/timer.rs | 87 | fn | MMIO | unsafe fn mask() { let ctl: u64 = 0b11; // ENABLE=1, IMASK=1 cor |
| arch/aarch64/src/timer.rs | 102 | fn | MMIO | unsafe fn disable() { let ctl: u64 = 0b00; core::arch::asm!( "ms |
| arch/aarch64/src/timer.rs | 116 | block | MMIO | unsafe { arm_slice() } } /// 读虚拟计数器 `CNTVCT_EL0`（周期数）。调度器时间片按**周 |
| arch/aarch64/src/timer.rs | 125 | block | MMIO | unsafe { core::arch::asm!("mrs {0}, cntvct_el0", out(reg) v, opt |
| arch/aarch64/src/timer.rs | 135 | block | MMIO | unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, opt |
| arch/aarch64/src/timer.rs | 143 | block | MMIO | unsafe { SLICE } } fn read_cntfrq() -> u64 { let v: u64; unsafe |
| arch/aarch64/src/timer.rs | 148 | block | MMIO | unsafe { core::arch::asm!("mrs {0}, cntfrq_el0", out(reg) v, opt |
| arch/aarch64/src/trap.rs | 258 | fn | 上下文切换 | unsafe fn init() { extern "C" { static __vectors: u8; } let vbar |
| arch/aarch64/src/trap.rs | 336 | block | 上下文切换 | unsafe { crate::user_fault_exit() } { // SAFETY: f 是 C-ABI `-> ! |
| arch/aarch64/src/trap.rs | 338 | block | 上下文切换 | unsafe { f() } } halt(); } /// dump 一个 TrapFrame（EC 名 + 寄存器全集）。 |
| arch/aarch64/src/trap.rs | 376 | block | 上下文切换 | unsafe { read_esr_elr() }; uart_puts(b"\n=== EXCEPTION: unexpect |
| arch/aarch64/src/trap.rs | 417 | block | 上下文切换 | unsafe { crate::irq_dispatch() } { // SAFETY: f 是 C-ABI fn 指针；tf |
| arch/aarch64/src/trap.rs | 419 | block | 上下文切换 | unsafe { f(tf) }; } } crate::gic::IRQ_PL011_RX => { // 排空 FIFO 到 |
| arch/aarch64/src/trap.rs | 444 | fn | 上下文切换 | unsafe fn read_esr_elr() -> (u64, u64) { let esr: u64; let elr: |
| arch/aarch64/src/uart.rs | 70 | block | MMIO | unsafe { mmio::write(UART, c as u32) }; } /// 输出一段字节串。 pub fn ua |
| arch/aarch64/src/uart.rs | 138 | block | MMIO | unsafe { *core::ptr::addr_of_mut!(RX_NOTIFY) = Some(f) }; } /// |
| arch/aarch64/src/uart.rs | 144 | block | MMIO | unsafe { mmio::read(UART + UART_FR) & UART_FR_RXFE == 0 } } /// |
| arch/aarch64/src/uart.rs | 150 | block | MMIO | unsafe { mmio::read(UART + UART_DR) as u8 } } /// 环满丢字节数（console |
| arch/aarch64/src/uart.rs | 156 | block | MMIO | unsafe { (*core::ptr::addr_of!(RX_RING)).lost } } /// RX IRQ han |
| arch/aarch64/src/uart.rs | 165 | block | MMIO | unsafe { while mmio::read(UART + UART_FR) & UART_FR_RXFE == 0 { |
| arch/aarch64/src/uart.rs | 193 | block | MMIO | unsafe { let r = &mut *core::ptr::addr_of_mut!(RX_RING); let mut |
| arch/aarch64/src/uart.rs | 212 | fn | MMIO | unsafe fn rx_init() { // 同掩 RXIC（UART_ICR_STICKY）：boot 期已落 FIFO |
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
| kernel/src/aspace.rs | 21 | block | 页表 | unsafe { let slot = &raw mut ASIDS; if (*slot).is_none() { *slot |
| kernel/src/aspace.rs | 69 | block | 页表 | unsafe { (USER_VA as *const u64).read_volatile() } } /// 经当前 TTB |
| kernel/src/aspace.rs | 75 | block | 页表 | unsafe { (USER_VA as *mut u64).write_volatile(v) }; } } |
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
| kernel/src/channel.rs | 105 | fn | 其它 | unsafe fn pair(a: NonNull<ObjRef>, b: NonNull<ObjRef>) { // SAFE |
| kernel/src/channel.rs | 108 | block | 其它 | unsafe { (*a.as_ptr()).obj.as_channel_mut().unwrap().peer = Some |
| kernel/src/channel.rs | 161 | block | 其它 | unsafe { ObjRef::release(peer) }; } } } |
| kernel/src/dtb.rs | 124 | block | 其它 | unsafe { core::slice::from_raw_parts(phys_to_virt(dtb_pa) as *co |
| kernel/src/dtb.rs | 129 | block | 其它 | unsafe { core::slice::from_raw_parts(phys_to_virt(dtb_pa) as *co |
| kernel/src/handle.rs | 214 | fn | 其它 | unsafe fn release(ptr: NonNull<Self>) { // SAFETY: 调用方契约（retain |
| kernel/src/handle.rs | 259 | block | 其它 | unsafe { NonNull::new_unchecked(Box::into_raw(boxed)) }; self.sl |
| kernel/src/handle.rs | 277 | block | 其它 | unsafe { (*obj.as_ptr()).refs += 1 }; self.slots[slot] = SlotSta |
| kernel/src/handle.rs | 294 | block | 其它 | unsafe { let r = &*obj.as_ptr(); if r.refs == 1 { if let KernelO |
| kernel/src/handle.rs | 305 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); r.refs -= 1; r.refs }; if r |
| kernel/src/handle.rs | 312 | block | 其它 | unsafe { drop(Box::from_raw(obj.as_ptr())) }; } self.slots[slot] |
| kernel/src/handle.rs | 332 | block | 其它 | unsafe { let r = &*hobj.as_ptr(); if r.refs == 1 { if let Kernel |
| kernel/src/handle.rs | 391 | block | 其它 | unsafe { let r = &*obj.as_ptr(); r.refs == 1 && match &r.obj { K |
| kernel/src/handle.rs | 401 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); r.refs -= 1; r.refs }; |
| kernel/src/handle.rs | 408 | block | 其它 | unsafe { drop(Box::from_raw(obj.as_ptr())) }; } } *slot = SlotSt |
| kernel/src/handle.rs | 426 | block | 其它 | unsafe { &*obj.as_ptr() }.obj, rights)) } /// 本表是否持有 `obj`（ObjRe |
| kernel/src/handle.rs | 465 | block | 其它 | unsafe { &*obj.as_ptr() }; if r.obj.obj_type() != ty { return Er |
| kernel/src/handoff.rs | 138 | block | 其它 | unsafe { &*(phys_to_virt(x0) as *const BerryHandoff) }; if h.mag |
| kernel/src/handoff.rs | 174 | block | 其它 | unsafe { core::slice::from_raw_parts(base, count) }; for s in se |
| kernel/src/heap.rs | 47 | impl | 分配器 | unsafe impl Sync for SlabHeap {} unsafe impl GlobalAlloc for Sla |
| kernel/src/heap.rs | 49 | impl | 分配器 | unsafe impl GlobalAlloc for SlabHeap { unsafe fn alloc(&self, la |
| kernel/src/heap.rs | 50 | fn | 分配器 | unsafe fn alloc(&self, layout: Layout) -> *mut u8 { let eff = co |
| kernel/src/heap.rs | 84 | fn | 分配器 | unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { let eff |
| kernel/src/heap.rs | 114 | block | 分配器 | unsafe { alloc::alloc::alloc(layout) } as *mut T; if ptr.is_null |
| kernel/src/heap.rs | 119 | block | 分配器 | unsafe { ptr.write(val) }; // SAFETY: 该块按全局分配器 + for_value 布局分配， |
| kernel/src/heap.rs | 122 | block | 分配器 | unsafe { Box::from_raw(ptr) }) } // ---- freelist（侵入式：槽首存 next 指 |
| kernel/src/heap.rs | 127 | fn | 分配器 | unsafe fn pop(fl: &mut [*mut u8; SLAB_CLASSES.len()], ci: usize) |
| kernel/src/heap.rs | 136 | fn | 分配器 | unsafe fn push(fl: &mut [*mut u8; SLAB_CLASSES.len()], ci: usize |
| kernel/src/ktest.rs | 66 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_CLOCK_READ as u64, 0, &r |
| kernel/src/ktest.rs | 72 | block | 其它 | unsafe { OUT }; assert!(t1 > 0, "SYSCALL：clock_read 应得正纳秒"); ass |
| kernel/src/ktest.rs | 80 | block | 其它 | unsafe { OUT } >= t1, "SYSCALL：单调钟不单调"); // D. handle 门控全链路（EL1 |
| kernel/src/ktest.rs | 97 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 103 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hd, 0, "SYSCALL：duplicate 须回填新 |
| kernel/src/ktest.rs | 138 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_S |
| kernel/src/ktest.rs | 141 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hv, 0, "SYSCALL：vmo_create 须回填 |
| kernel/src/ktest.rs | 146 | block | 其它 | unsafe { EBUF[..8].copy_from_slice(&PATTERN.to_le_bytes()) }; as |
| kernel/src/ktest.rs | 153 | block | 其它 | unsafe { EBUF = [0; 16] }; assert_eq!( svc6(NR_VMO_READ as u64, |
| kernel/src/ktest.rs | 160 | block | 其它 | unsafe { u64::from_le_bytes(EBUF[..8].try_into().unwrap()) }, PA |
| kernel/src/ktest.rs | 167 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_S |
| kernel/src/ktest.rs | 169 | block | 其它 | unsafe { OUT as u32 } }; assert_eq!( svc6(NR_VMO_READ as u64, hv |
| kernel/src/ktest.rs | 179 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 185 | block | 其它 | unsafe { OUT } as u32; assert_eq!( svc6(NR_VMO_WRITE as u64, hro |
| kernel/src/ktest.rs | 207 | block | 其它 | unsafe { OUT = crate::aspace::USER_VA as u64 }; // 请求 VA = 0x100 |
| kernel/src/ktest.rs | 214 | block | 其它 | unsafe { OUT } as usize, crate::aspace::USER_VA, "SYSCALL：vmar_m |
| kernel/src/ktest.rs | 247 | block | 其它 | unsafe { OUT = 0x20_0000 }; // 请求子区间基 0x20_0000 static mut CHILD |
| kernel/src/ktest.rs | 250 | block | 其它 | unsafe { CHILD_OUT = 0 }; assert_eq!( svc6(NR_VMAR_ALLOCATE as u |
| kernel/src/ktest.rs | 257 | block | 其它 | unsafe { CHILD_OUT } as u32; assert_ne!(hchild, 0, "SYSCALL：allo |
| kernel/src/ktest.rs | 270 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_GET_SIZE as u64, 0xDE |
| kernel/src/ktest.rs | 280 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_CREATE as u64, 0x1800 |
| kernel/src/ktest.rs | 283 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上（预置哨兵值，验回写真发生）。 unsafe { OUT |
| kernel/src/ktest.rs | 285 | block | 其它 | unsafe { OUT = u64::MAX }; assert_eq!(svc6(NR_VMO_GET_SIZE as u6 |
| kernel/src/ktest.rs | 288 | block | 其它 | unsafe { OUT }, PAGE_SIZE as u64 * 2, "SYSCALL：get_size 须回页对齐容量（ |
| kernel/src/ktest.rs | 292 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 299 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上。 unsafe { OUT = u64::MAX }; |
| kernel/src/ktest.rs | 301 | block | 其它 | unsafe { OUT = u64::MAX }; assert_eq!(svc6(NR_VMO_GET_SIZE as u6 |
| kernel/src/ktest.rs | 304 | block | 其它 | unsafe { OUT }, PAGE_SIZE as u64 * 2, "SYSCALL：零 READ 别名 size 同值 |
| kernel/src/ktest.rs | 390 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc6(NR_VMO_GET_SIZE as u64, hmm |
| kernel/src/ktest.rs | 397 | block | 其它 | unsafe { OUT }, berry_hal_aarch64::VIRTIO_MMIO_LEN as u64, "MMIO |
| kernel/src/ktest.rs | 417 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 423 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上。 unsafe { OUT = crate::aspa |
| kernel/src/ktest.rs | 425 | block | 其它 | unsafe { OUT = crate::aspace::USER_VA as u64 }; assert_eq!( svc6 |
| kernel/src/ktest.rs | 435 | block | 其它 | unsafe { OUT = crate::aspace::USER_VA as u64 }; assert_eq!( svc6 |
| kernel/src/ktest.rs | 442 | block | 其它 | unsafe { OUT }, crate::aspace::USER_VA as u64, "MMIO：map 回写基址不变" |
| kernel/src/ktest.rs | 464 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_GET_PHYS as u64, 0xDE |
| kernel/src/ktest.rs | 475 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_CREATE as u64, PAGE_S |
| kernel/src/ktest.rs | 478 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上。 unsafe { OUT = u64::MAX }; |
| kernel/src/ktest.rs | 480 | block | 其它 | unsafe { OUT = u64::MAX }; assert_eq!(svc6(NR_VMO_GET_PHYS as u6 |
| kernel/src/ktest.rs | 488 | block | 其它 | unsafe { OUT = u64::MAX }; assert_eq!(svc6(NR_VMO_GET_PHYS as u6 |
| kernel/src/ktest.rs | 491 | block | 其它 | unsafe { OUT }; assert!(pa0 >= 0x4000_0000 && pa0 < 0x4000_0000 |
| kernel/src/ktest.rs | 496 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hgp |
| kernel/src/ktest.rs | 499 | block | 其它 | unsafe { OUT }, pa0 + 0x123, "SYSCALL：get_phys 字节语义（页基 + 页内偏移）") |
| kernel/src/ktest.rs | 507 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 514 | block | 其它 | unsafe { OUT } as u32; // SAFETY: 同上。 unsafe { OUT = 0 }; assert |
| kernel/src/ktest.rs | 516 | block | 其它 | unsafe { OUT = 0 }; assert_eq!(svc6(NR_VMO_GET_PHYS as u64, hgo |
| kernel/src/ktest.rs | 519 | block | 其它 | unsafe { OUT }, pa0 + 0x123, "SYSCALL：零 READ 别名 phys 同值"); asser |
| kernel/src/ktest.rs | 529 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc5(NR_PROCESS_C |
| kernel/src/ktest.rs | 535 | block | 其它 | unsafe { OUT } as u32; let hv2 = unsafe { CHILD_OUT } as u32; as |
| kernel/src/ktest.rs | 536 | block | 其它 | unsafe { CHILD_OUT } as u32; assert_ne!(hp, 0, "SYSCALL：process_ |
| kernel/src/ktest.rs | 557 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc5(NR_THREAD_CREATE as u64, hp |
| kernel/src/ktest.rs | 563 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hth, 0, "SYSCALL：thread_create |
| kernel/src/ktest.rs | 625 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4( NR_HANDLE_DUPLICATE as u64 |
| kernel/src/ktest.rs | 636 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hthr, 0, "SYSCALL：dup 别名须回填"); |
| kernel/src/ktest.rs | 653 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc5(NR_PROCESS_C |
| kernel/src/ktest.rs | 659 | block | 其它 | unsafe { OUT } as u32; let hv3 = unsafe { CHILD_OUT } as u32; as |
| kernel/src/ktest.rs | 660 | block | 其它 | unsafe { CHILD_OUT } as u32; assert_ne!(hp2, 0, "SYSCALL：第二个进程 h |
| kernel/src/ktest.rs | 674 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(crate::syscall::NR_OBJECT |
| kernel/src/ktest.rs | 680 | block | 其它 | unsafe { I_OBS }, 0, "SYSCALL：未命中 observed 须保持 0"); // F4c. proc |
| kernel/src/ktest.rs | 689 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(crate::syscall::NR_PROCESS_ |
| kernel/src/ktest.rs | 705 | block | 其它 | unsafe { OUT }, 0, "SYSCALL：未退 code 出参须保持 0"); // F4d. process_t |
| kernel/src/ktest.rs | 728 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4( NR_HANDLE_DUPLICATE as u64 |
| kernel/src/ktest.rs | 739 | block | 其它 | unsafe { OUT } as u32; assert_ne!(hp2r, 0, "SYSCALL：dup 别名须回填"); |
| kernel/src/ktest.rs | 755 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5( crate::syscall::NR_OBJEC |
| kernel/src/ktest.rs | 768 | block | 其它 | unsafe { I_OBS }, crate::handle::SIGNAL_TERMINATED, "SYSCALL：obs |
| kernel/src/ktest.rs | 774 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(crate::syscall::NR_PROCESS_ |
| kernel/src/ktest.rs | 780 | block | 其它 | unsafe { OUT } as u32, 42, "SYSCALL：terminate code=42 须落退出码"); a |
| kernel/src/ktest.rs | 835 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc5(NR_PROCESS_C |
| kernel/src/ktest.rs | 841 | block | 其它 | unsafe { OUT } as u32; let hv4 = unsafe { CHILD_OUT } as u32; le |
| kernel/src/ktest.rs | 842 | block | 其它 | unsafe { CHILD_OUT } as u32; let (pobj, _) = ht.get(hp3, ObjType |
| kernel/src/ktest.rs | 866 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc4(NR_CHANNEL_C |
| kernel/src/ktest.rs | 872 | block | 其它 | unsafe { OUT } as u32, unsafe { CHILD_OUT } as u32); assert_ne!( |
| kernel/src/ktest.rs | 872 | block | 其它 | unsafe { CHILD_OUT } as u32); assert_ne!(hc0, 0, "SYSCALL：channe |
| kernel/src/ktest.rs | 883 | block | 其它 | unsafe { H_NB = 8; H_NH = 4 }; assert_eq!( svc6(NR_CHANNEL_RECV |
| kernel/src/ktest.rs | 894 | block | 其它 | unsafe { HBUF[..8].copy_from_slice(&PATTERN.to_le_bytes()) }; as |
| kernel/src/ktest.rs | 901 | block | 其它 | unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 8; H_HND = [0; 8] }; |
| kernel/src/ktest.rs | 909 | block | 其它 | unsafe { u64::from_le_bytes(HBUF[..8].try_into().unwrap()) }, PA |
| kernel/src/ktest.rs | 911 | block | 其它 | unsafe { H_NB }, 8, "SYSCALL：recv 须回写实际字节数"); assert_eq!(unsafe |
| kernel/src/ktest.rs | 912 | block | 其它 | unsafe { H_NH }, 0, "SYSCALL：recv 须回写实际 handle 数"); // SAFETY: 同 |
| kernel/src/ktest.rs | 914 | block | 其它 | unsafe { H_NB = 8; H_NH = 0 }; assert_eq!( svc6(NR_CHANNEL_RECV |
| kernel/src/ktest.rs | 942 | block | 其它 | unsafe { H_NB = 4; H_NH = 0 }; assert_eq!( svc6(NR_CHANNEL_RECV |
| kernel/src/ktest.rs | 950 | block | 其它 | unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 0 }; assert_eq!( svc6 |
| kernel/src/ktest.rs | 958 | block | 其它 | unsafe { u64::from_le_bytes(HBUF[..8].try_into().unwrap()) }, PA |
| kernel/src/ktest.rs | 963 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc4(NR_CHANNEL_C |
| kernel/src/ktest.rs | 969 | block | 其它 | unsafe { OUT } as u32, unsafe { CHILD_OUT } as u32); for _ in 0. |
| kernel/src/ktest.rs | 969 | block | 其它 | unsafe { CHILD_OUT } as u32); for _ in 0..8 { assert_eq!( svc6(N |
| kernel/src/ktest.rs | 988 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 994 | block | 其它 | unsafe { OUT } as u32; assert_eq!( svc6(NR_CHANNEL_SEND as u64, |
| kernel/src/ktest.rs | 1009 | block | 其它 | unsafe { H_HND = [hsrc, 0, 0, 0, 0, 0, 0, 0]; HBUF[..8].copy_fro |
| kernel/src/ktest.rs | 1022 | block | 其它 | unsafe { HBUF = [0; 320]; H_NB = 8; H_NH = 8; H_HND = [0; 8] }; |
| kernel/src/ktest.rs | 1029 | block | 其它 | unsafe { H_HND[0] }; assert_eq!(unsafe { H_NH }, 1, "SYSCALL：rec |
| kernel/src/ktest.rs | 1030 | block | 其它 | unsafe { H_NH }, 1, "SYSCALL：recv 须回写 1 个随行 handle"); assert_ne! |
| kernel/src/ktest.rs | 1049 | block | 其它 | unsafe { H_HND = [hgd, 0xDEAD_BEEF, 0, 0, 0, 0, 0, 0] }; assert_ |
| kernel/src/ktest.rs | 1061 | block | 其它 | unsafe { H_HND = [hgd, hgd, 0, 0, 0, 0, 0, 0] }; assert_eq!( svc |
| kernel/src/ktest.rs | 1097 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_EVENT_CREATE as u64, 0, |
| kernel/src/ktest.rs | 1103 | block | 其它 | unsafe { OUT } as u32; assert_ne!(he, 0, "SYSCALL：event_create 须 |
| kernel/src/ktest.rs | 1113 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1132 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1138 | block | 其它 | unsafe { I_OBS }, U1 \| U2, "SYSCALL：observed 须回写全满足集"); // I3. 请 |
| kernel/src/ktest.rs | 1143 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1155 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1171 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_HANDLE_DUPLICATE as u64, |
| kernel/src/ktest.rs | 1177 | block | 其它 | unsafe { OUT } as u32; assert_eq!( svc4(NR_OBJECT_SIGNAL as u64, |
| kernel/src/ktest.rs | 1184 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1231 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1252 | block | 其它 | unsafe { OUT = 0; CHILD_OUT = 0 }; assert_eq!( svc4(NR_CHANNEL_C |
| kernel/src/ktest.rs | 1258 | block | 其它 | unsafe { OUT } as u32, unsafe { CHILD_OUT } as u32); // SAFETY: |
| kernel/src/ktest.rs | 1258 | block | 其它 | unsafe { CHILD_OUT } as u32); // SAFETY: 同上。 unsafe { I_OBS = 0 |
| kernel/src/ktest.rs | 1260 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1271 | block | 其它 | unsafe { I_OBS }, SIGNAL_WRITABLE, "SYSCALL：WRITABLE 全满足集"); ass |
| kernel/src/ktest.rs | 1278 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1286 | block | 其它 | unsafe { I_OBS }, SIGNAL_READABLE \| SIGNAL_WRITABLE, "SYSCALL：RE |
| kernel/src/ktest.rs | 1298 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1305 | block | 其它 | unsafe { H_NB = 8; H_NH = 0 }; assert_eq!( svc6(NR_CHANNEL_RECV |
| kernel/src/ktest.rs | 1313 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1322 | block | 其它 | unsafe { I_OBS = 0 }; assert_eq!( svc5(NR_OBJECT_WAIT_ONE as u64 |
| kernel/src/ktest.rs | 1330 | block | 其它 | unsafe { I_OBS }, SIGNAL_READABLE \| SIGNAL_WRITABLE, "SYSCALL：全满 |
| kernel/src/ktest.rs | 1344 | block | 其它 | unsafe { OUT = 0 }; assert_eq!( svc4(NR_EVENT_CREATE as u64, 0, |
| kernel/src/ktest.rs | 1350 | block | 其它 | unsafe { OUT } as u32; assert_eq!( svc4(NR_OBJECT_SIGNAL as u64, |
| kernel/src/ktest.rs | 1356 | block | 其它 | unsafe { WM_IDX = 0 }; assert_eq!( svc6(NR_OBJECT_WAIT_MANY as u |
| kernel/src/ktest.rs | 1370 | block | 其它 | unsafe { WM_H = [3, hw_ev]; WM_S = [U1, U1] }; assert_eq!( svc6( |
| kernel/src/ktest.rs | 1379 | block | 其它 | unsafe { WM_H = [hw_ev, hw_ev]; WM_S = [U2, U2]; WM_IDX = 0 }; a |
| kernel/src/ktest.rs | 1386 | block | 其它 | unsafe { WM_IDX }, u64::MAX, "SYSCALL：未命中 idx 须留哨兵 MAX"); // SAF |
| kernel/src/ktest.rs | 1388 | block | 其它 | unsafe { WM_IDX = 0 }; assert_eq!( svc6(NR_OBJECT_WAIT_MANY as u |
| kernel/src/ktest.rs | 1398 | block | 其它 | unsafe { WM_H = [hw_ev, hw_ev]; WM_S = [U2, U1]; WM_IDX = 0; I_O |
| kernel/src/ktest.rs | 1405 | block | 其它 | unsafe { WM_IDX }, 1, "SYSCALL：idx 须指次项"); assert_eq!(unsafe { I |
| kernel/src/ktest.rs | 1406 | block | 其它 | unsafe { I_OBS }, U1, "SYSCALL：observed 须该对象全满足集"); // SAFETY: 同 |
| kernel/src/ktest.rs | 1408 | block | 其它 | unsafe { WM_S = [U1, U2]; WM_IDX = 0; I_OBS = 0 }; assert_eq!( s |
| kernel/src/ktest.rs | 1415 | block | 其它 | unsafe { WM_IDX }, 0, "SYSCALL：idx 须指首项（入参序最前）"); // 清场。 assert_ |
| kernel/src/ktest.rs | 1428 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/ktest.rs | 1446 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/ktest.rs | 1465 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/ktest.rs | 1494 | block | 其它 | unsafe { core::arch::asm!("brk #0x42", options(nostack, preserve |
| kernel/src/ktest.rs | 1505 | block | 其它 | unsafe { v1.write_volatile(0xDEAD_BEEF_CAFE_F00D); assert_eq!( v |
| kernel/src/ktest.rs | 1554 | block | 其它 | unsafe { for w in 0..512usize { va.add(w).write_volatile(0); } f |
| kernel/src/ktest.rs | 1578 | block | 其它 | unsafe { desc_at(pa as u64, 40, 1) }; assert_eq!(d1.kind, 4); as |
| kernel/src/ktest.rs | 1590 | block | 其它 | unsafe { desc_at(pa as u64, 40, i) }; assert_eq!( desc_range(d, |
| kernel/src/ktest.rs | 1692 | block | 其它 | unsafe { berry_hal_aarch64::uart_puts(b"PING "); berry_hal_aarch |
| kernel/src/ktest.rs | 1707 | block | 其它 | unsafe { let stack_top = (&raw const SWITCH_OTHER_STACK) as *con |
| kernel/src/ktest.rs | 1756 | block | 其它 | unsafe { core::ptr::read_volatile(berry_hal_aarch64::RAM_BASE as |
| kernel/src/ktest.rs | 1805 | block | 其它 | unsafe { core::ptr::read_volatile(USER_VA as *const u64) }; asse |
| kernel/src/ktest.rs | 1808 | block | 其它 | unsafe { core::ptr::write_volatile(USER_VA as *mut u64, MAGIC2) |
| kernel/src/ktest.rs | 1816 | block | 其它 | unsafe { core::ptr::read_volatile(USER_VA as *const u64) }; asse |
| kernel/src/ktest.rs | 1823 | block | 其它 | unsafe { core::ptr::read_volatile(USER_VA as *const u64) }; asse |
| kernel/src/ktest.rs | 1827 | block | 其它 | unsafe { core::ptr::write_volatile(USER_VA as *mut u64, MAGIC) } |
| kernel/src/ktest.rs | 1831 | block | 其它 | unsafe { core::ptr::write_volatile(USER_VA as *mut u64, MAGIC) } |
| kernel/src/ktest.rs | 1849 | block | 其它 | unsafe { core::ptr::read_volatile(0x20_0000 as *const u64) }; as |
| kernel/src/ktest.rs | 2370 | block | 其它 | unsafe { &mut *(ht as *mut HandleTable) }; let h = ht .insert( K |
| kernel/src/ktest.rs | 2385 | block | 其它 | unsafe { ChannelEnd::pair(a, b) }; } /// 一个 Event 进内核侧表（非 channe |
| kernel/src/ktest.rs | 2705 | block | 其它 | unsafe { (*ni.as_ptr()).obj.as_channel().unwrap().no_intent() }, |
| kernel/src/ktest.rs | 2775 | block | 其它 | unsafe { ChannelEnd::pair(a, b) }; } // L0. 非法参 + 空队 → None（send |
| kernel/src/main.rs | 255 | block | 其它 | unsafe { mm::init(handoff) }; // G-eco 入机侧第一刀：rootfs（initrd 角色）装 |
| kernel/src/main.rs | 316 | block | 其它 | unsafe { core::slice::from_raw_parts(p, len) }; Some((b"handoff" |
| kernel/src/main.rs | 350 | block | 其它 | unsafe { core::slice::from_raw_parts( phys_to_virt(s.pa as usize |
| kernel/src/main.rs | 392 | block | 其它 | unsafe { core::slice::from_raw_parts(p, len as usize) }; Some((b |
| kernel/src/main.rs | 405 | block | 其它 | unsafe { core::slice::from_raw_parts(pa, (e - s) as usize) }; (b |
| kernel/src/main.rs | 437 | block | 其它 | unsafe { *(&raw mut ROOTFS) = Some(buf) }; berry_hal_aarch64::ua |
| kernel/src/main.rs | 460 | block | MMIO | unsafe { berry_hal_aarch64::timer::init() }; // 2b-2：两忙循环任务各挂一个私 |
| kernel/src/main.rs | 549 | block | 其它 | unsafe { crate::channel::ChannelEnd::pair(pk, pu) }; // 先于用户矩阵入队 |
| kernel/src/main.rs | 605 | block | 其它 | unsafe { (*p_rx.as_ptr()).retain() }; // 静态槽传递（spawn arg 已满：hi32 |
| kernel/src/main.rs | 610 | block | 其它 | unsafe { *core::ptr::addr_of_mut!(RX_EV_H) = h_rx_ev; *core::ptr |
| kernel/src/main.rs | 634 | block | 其它 | unsafe { *core::ptr::addr_of_mut!(BSH_IN_H) = h_end_k2; } // 3-2 |
| kernel/src/main.rs | 745 | block | 其它 | unsafe { ChannelEnd::pair(pa, pb) }; // 4-2b：哑 Event 进 ponger 表— |
| kernel/src/main.rs | 794 | block | 其它 | unsafe { ChannelEnd::pair(pc, ps) }; let h_ev_pi = syscall::boot |
| kernel/src/main.rs | 833 | block | 其它 | unsafe { ChannelEnd::pair(ps, pr) }; (hs, hr) }; let (h_d1, h_m1 |
| kernel/src/main.rs | 854 | block | 其它 | unsafe { (*p.as_ptr()).obj.as_channel_mut().unwrap().push(m) }; |
| kernel/src/main.rs | 873 | block | 其它 | unsafe { for (i, &h) in slots.iter().enumerate() { *core::ptr::a |
| kernel/src/main.rs | 917 | block | 其它 | unsafe { ChannelEnd::pair(ps, pr) }; (hs, hr) }; let (h_fa, h_fb |
| kernel/src/main.rs | 947 | block | 其它 | unsafe { ChannelEnd::pair(pa, pb) }; // arg 打包双 handle：低 32 = 自己 |
| kernel/src/main.rs | 993 | block | 其它 | unsafe { (*(&raw mut ROOTFS)).take() }.map(\|buf\| { let vmo = mat |
| kernel/src/main.rs | 1053 | block | 其它 | unsafe { crate::channel::ChannelEnd::pair(pk2, pu2) }; // ---- P |
| kernel/src/main.rs | 1192 | block | 其它 | unsafe { crate::channel::ChannelEnd::pair(pkf, puf) }; // SAFETY |
| kernel/src/main.rs | 1194 | block | 其它 | unsafe { *core::ptr::addr_of_mut!(FB_PIPE_H) = h_end_kfb; } } Er |
| kernel/src/main.rs | 1249 | block | 其它 | unsafe { ChannelEnd::pair(pab_a, pab_b) }; // E_done（bench 完成事件） |
| kernel/src/main.rs | 1375 | block | MMIO | unsafe { berry_hal_aarch64::uart::rx_init() }; // S-1 副核 bring-u |
| kernel/src/main.rs | 1385 | block | MMIO | unsafe { berry_hal_aarch64::timer::arm_slice() }; berry_hal_aarc |
| kernel/src/main.rs | 1405 | block | 其它 | unsafe { core::ptr::read_volatile(aspace::USER_VA as *const u64) |
| kernel/src/main.rs | 1421 | block | 其它 | unsafe { core::ptr::read_volatile(aspace::USER_VA as *const u64) |
| kernel/src/main.rs | 1518 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/main.rs | 1578 | block | 其它 | unsafe { core::arch::asm!( "svc #0", inout("x0") nr => ret, in(" |
| kernel/src/main.rs | 1759 | block | 其它 | unsafe { *core::ptr::addr_of!(RX_EV_H) } } /// bsh stdin 发端（endK |
| kernel/src/main.rs | 1769 | block | 其它 | unsafe { *core::ptr::addr_of!(BSH_IN_H) } } /// fb 上屏管道 console |
| kernel/src/main.rs | 1781 | block | MMIO | unsafe { *core::ptr::addr_of!(FB_PIPE_H) } } /// 「RX 到数据」通知（HAL |
| kernel/src/main.rs | 1791 | fn | 其它 | unsafe extern "C" fn rx_irq_notify() { // SAFETY: IRQ 上下文（单核屏蔽窗串 |
| kernel/src/main.rs | 1793 | block | 其它 | unsafe { *core::ptr::addr_of!(RX_EV_REF) } .expect("rx_irq_notif |
| kernel/src/main.rs | 1795 | block | 其它 | unsafe { if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() { ev |
| kernel/src/main.rs | 1810 | fn | 其它 | unsafe fn rx_ev_clear() { // SAFETY: 调用方保证屏蔽包络 + 引用不变。 let p = u |
| kernel/src/main.rs | 1812 | block | 其它 | unsafe { *core::ptr::addr_of!(RX_EV_REF) } .expect("rx_ev_clear: |
| kernel/src/main.rs | 1814 | block | 其它 | unsafe { if let Some(ev) = (*p.as_ptr()).obj.as_event_mut() { ev |
| kernel/src/main.rs | 1946 | block | 其它 | unsafe { rx_ev_clear() }; berry_hal_aarch64::enable_irq(); for i |
| kernel/src/main.rs | 2075 | block | 其它 | unsafe { *core::ptr::addr_of!(INTENT_H).cast::<u32>().add(i) } } |
| kernel/src/main.rs | 2614 | block | MMIO | unsafe { &*(arg as *const AiDemoCtx) }; let freq = berry_hal_aar |
| kernel/src/mm.rs | 66 | fn | 其它 | unsafe fn init(handoff: Option<&'static crate::handoff::BerryHan |
| kernel/src/mm.rs | 106 | block | 其它 | unsafe { berry_mm::memmap::desc_at(pa, dsize, i) }; if matches!( |
| kernel/src/mm.rs | 120 | block | 其它 | unsafe { berry_mm::memmap::desc_at(pa, dsize, i) }; if let berry |
| kernel/src/mm.rs | 154 | block | 分配器 | unsafe { FRAMES .as_mut() .expect("mm::init() 未调用就访问 frames()") |
| kernel/src/pi.rs | 103 | block | 其它 | unsafe { &*(t.wait_obj as *const ObjRef) }; let KernelObj::Chann |
| kernel/src/pi.rs | 131 | block | 其它 | unsafe { &*(t.proc_tag as *const HandleTable) }) } else { Some(c |
| kernel/src/process.rs | 91 | fn | 其它 | unsafe fn new(vmar_ref: NonNull<ObjRef>, ttbr0_pa: usize, asid: |
| kernel/src/process.rs | 122 | block | 其它 | unsafe { ObjRef::release(self.vmar) }; PROCESS_LIVE.fetch_sub(1, |
| kernel/src/process.rs | 150 | fn | 其它 | unsafe fn new(proc_ref: NonNull<ObjRef>) -> Self { (*proc_ref.as |
| kernel/src/process.rs | 164 | block | 其它 | unsafe { ObjRef::release(self.proc) }; } } |
| kernel/src/syscall.rs | 99 | block | 其它 | unsafe { let slot = &raw mut BOOTSTRAP_HT; if (*slot).is_none() |
| kernel/src/syscall.rs | 128 | block | 其它 | unsafe { &mut *(tag as *mut HandleTable) }) } else { Ok(bootstra |
| kernel/src/syscall.rs | 136 | block | 其它 | unsafe { &mut *(tag as *mut HandleTable) }) } } // ---- copy_fro |
| kernel/src/syscall.rs | 208 | block | 其它 | unsafe { core::ptr::copy_nonoverlapping(src as *const u8, dst.as |
| kernel/src/syscall.rs | 220 | block | 其它 | unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst as *mu |
| kernel/src/syscall.rs | 231 | block | 其它 | unsafe { core::ptr::read_volatile(ptr as *const u64) }) } /// 写一 |
| kernel/src/syscall.rs | 610 | block | 其它 | unsafe { &mut (*obj.as_ptr()).obj }.as_vmo_mut() } /// `vmar_map |
| kernel/src/syscall.rs | 656 | block | 其它 | unsafe { ((*vr.as_ptr()).obj.as_vmar_mut(), (*vo.as_ptr()).obj.a |
| kernel/src/syscall.rs | 678 | block | 其它 | unsafe { &mut (*obj.as_ptr()).obj }).as_vmar_mut() else { return |
| kernel/src/syscall.rs | 707 | block | 其它 | unsafe { &mut (*obj.as_ptr()).obj }).as_vmar_mut() else { return |
| kernel/src/syscall.rs | 747 | block | 其它 | unsafe { &mut (*obj.as_ptr()).obj }).as_vmar_mut() else { return |
| kernel/src/syscall.rs | 815 | block | 其它 | unsafe { crate::process::Process::new(vobj, ttbr0, asid) }; matc |
| kernel/src/syscall.rs | 846 | block | 其它 | unsafe { crate::process::Thread::new(pobj) }; match t.insert(Ker |
| kernel/src/syscall.rs | 886 | block | 其它 | unsafe { (*vt.as_ptr()).obj.as_thread_mut() }; let Some(thrd) = |
| kernel/src/syscall.rs | 895 | block | 其它 | unsafe { (*proc_ref.as_ptr()).obj.as_process_mut() }; let Some(p |
| kernel/src/syscall.rs | 923 | block | 其它 | unsafe { (*proc_ref.as_ptr()).retain() }; berry_sched::set_task_ |
| kernel/src/syscall.rs | 925 | block | 其它 | unsafe { (*vt.as_ptr()).retain() }; berry_sched::set_task_thread |
| kernel/src/syscall.rs | 972 | block | 其它 | unsafe { ((*vr.as_ptr()).obj.as_process_mut(), (*vt.as_ptr()).ob |
| kernel/src/syscall.rs | 1042 | block | 其它 | unsafe { (*vr.as_ptr()).retain() }; berry_sched::set_task_pinned |
| kernel/src/syscall.rs | 1050 | block | 其它 | unsafe { (*vt.as_ptr()).retain() }; berry_sched::set_task_thread |
| kernel/src/syscall.rs | 1080 | fn | 其它 | unsafe extern "C" fn task_exit_hook(pinned: usize, thread_key: u |
| kernel/src/syscall.rs | 1087 | block | 其它 | unsafe { if thread_key != 0 { let tobj = thread_key as *mut ObjR |
| kernel/src/syscall.rs | 1131 | block | 其它 | unsafe { berry_sched::kill_all_tasks_by_pinned(p, 0) }; } berry_ |
| kernel/src/syscall.rs | 1212 | block | 其它 | unsafe { let r = &mut *obj.as_ptr(); let Some(p) = r.obj.as_proc |
| kernel/src/syscall.rs | 1305 | block | 其它 | unsafe { ChannelEnd::pair(a, b) }; let r0 = copy_out_u32(el1, ou |
| kernel/src/syscall.rs | 1368 | block | 其它 | unsafe { let Some(end) = (*self_ptr.as_ptr()).obj.as_channel_mut |
| kernel/src/syscall.rs | 1447 | block | 其它 | unsafe { let Some(peer_end) = (*peer_ptr.as_ptr()).obj.as_channe |
| kernel/src/syscall.rs | 1458 | block | 其它 | unsafe { berry_sched::fast_wake(t) }; crate::telemetry::ipc_fast |
| kernel/src/syscall.rs | 1482 | fn | 其它 | unsafe fn recv_rollback(t: &mut HandleTable, endp: NonNull<ObjRe |
| kernel/src/syscall.rs | 1515 | block | 其它 | unsafe { let Some(end) = (*self_ptr.as_ptr()).obj.as_channel_mut |
| kernel/src/syscall.rs | 1528 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return e as i64 |
| kernel/src/syscall.rs | 1536 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return e as i64 |
| kernel/src/syscall.rs | 1542 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return ERR_INVA |
| kernel/src/syscall.rs | 1547 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return e as i64 |
| kernel/src/syscall.rs | 1557 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &[]) }; return e as i64 |
| kernel/src/syscall.rs | 1569 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) }; retur |
| kernel/src/syscall.rs | 1574 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) }; retur |
| kernel/src/syscall.rs | 1579 | block | 其它 | unsafe { recv_rollback(t, self_ptr, msg, &newvals[..n]) }; retur |
| kernel/src/syscall.rs | 1586 | block | 其它 | unsafe { (*self_ptr.as_ptr()).obj.as_channel() }).and_then(\|e\| e |
| kernel/src/syscall.rs | 1592 | block | 其它 | unsafe { (*self_ptr.as_ptr()).obj.as_channel() }).map_or(false, |
| kernel/src/syscall.rs | 1647 | fn | 其它 | unsafe fn signal_state(obj: NonNull<ObjRef>) -> Option<u64> { un |
| kernel/src/syscall.rs | 1648 | block | 其它 | unsafe { match &(*obj.as_ptr()).obj { KernelObj::Event(e) => Som |
| kernel/src/syscall.rs | 1701 | block | 其它 | unsafe { let Some(ev) = (*obj.as_ptr()).obj.as_event_mut() else |
| kernel/src/syscall.rs | 1785 | block | 其它 | unsafe { (*obj.as_ptr()).retain() }; let ret = loop { // SAFETY: |
| kernel/src/syscall.rs | 1789 | block | 其它 | unsafe { signal_state(obj) }) else { break ERR_BAD_HANDLE as i64 |
| kernel/src/syscall.rs | 1813 | block | 其它 | unsafe { ObjRef::release(obj) }; ret } /// `object_wait_many(han |
| kernel/src/syscall.rs | 1902 | block | 其它 | unsafe { (*o.as_ptr()).retain() }; } let ret = loop { // 逐项重验（醒来 |
| kernel/src/syscall.rs | 1911 | block | 其它 | unsafe { signal_state(o) }) else { hit = Some(Err(ERR_BAD_HANDLE |
| kernel/src/syscall.rs | 1945 | block | 其它 | unsafe { ObjRef::release(*o) }; } ret } // ---- boot 期注册（rust_ma |
| kernel/src/telemetry.rs | 79 | block | MMIO | unsafe { let rec = SwitchRec { vct: berry_hal_aarch64::timer::cn |
| kernel/src/telemetry.rs | 103 | block | 其它 | unsafe { TOTAL } } /// 环内有效条数（min(TOTAL, RING_LEN)）。 pub fn span |
| kernel/src/telemetry.rs | 109 | block | 其它 | unsafe { TOTAL }; (t as usize).min(RING_LEN) } /// 按新→旧序取第 `i` 条 |
| kernel/src/telemetry.rs | 121 | block | 其它 | unsafe { HEAD }; let idx = (head + RING_LEN - 1 - i) % RING_LEN; |
| kernel/src/telemetry.rs | 124 | block | 其它 | unsafe { RING[idx] }) } /// 环健康自检（watchdog 周期断言）：①有货；②vct 从旧到新单调 |
| kernel/src/telemetry.rs | 200 | block | MMIO | unsafe { let rec = WakeRec { vct: berry_hal_aarch64::timer::cntv |
| kernel/src/telemetry.rs | 245 | block | 其它 | unsafe { WAKE_TOTAL } } /// 唤醒环内有效条数（min(WAKE_TOTAL, WAKE_RING_L |
| kernel/src/telemetry.rs | 251 | block | 其它 | unsafe { WAKE_TOTAL }; (t as usize).min(WAKE_RING_LEN) } /// 按新→ |
| kernel/src/telemetry.rs | 262 | block | 其它 | unsafe { WAKE_HEAD }; let idx = (head + WAKE_RING_LEN - 1 - i) % |
| kernel/src/telemetry.rs | 264 | block | 其它 | unsafe { WAKE_RING[idx] }) } /// 环内出现过的唤醒原因掩码（bit0=Signaled、bit1 |
| kernel/src/telemetry.rs | 348 | block | 其它 | unsafe { *core::ptr::addr_of!(WAKE_HIST_N) }; let src = core::pt |
| kernel/src/telemetry.rs | 353 | block | 其它 | unsafe { (*src)[i] }; } // SAFETY: 只读。 let min = unsafe { *core: |
| kernel/src/telemetry.rs | 356 | block | 其它 | unsafe { *core::ptr::addr_of!(WAKE_HIST_MIN) }; // SAFETY: 只读（须后 |
| kernel/src/telemetry.rs | 358 | block | 其它 | unsafe { *core::ptr::addr_of!(WAKE_HIST_MAX) }; // SAFETY: 只读。 l |
| kernel/src/telemetry.rs | 360 | block | 其它 | unsafe { *core::ptr::addr_of!(WAKE_HIST_SIG) }; // SAFETY: 只读。 l |
| kernel/src/telemetry.rs | 362 | block | 其它 | unsafe { *core::ptr::addr_of!(WAKE_HIST_TMO) }; WakeHist { bucke |
| kernel/src/telemetry.rs | 380 | block | 其它 | unsafe { INTENT_BORROWS += 1 }; } /// 记一次跳数帽拒绝（recv 结算 slowpath） |
| kernel/src/telemetry.rs | 386 | block | 其它 | unsafe { INTENT_HOP_CAPPED += 1 }; } /// 记一次时间帽拒绝（send 快照降级 / re |
| kernel/src/telemetry.rs | 392 | block | 其它 | unsafe { INTENT_BUDGET_CAPPED += 1 }; } /// 借入生效累计。 pub fn inten |
| kernel/src/telemetry.rs | 398 | block | 其它 | unsafe { INTENT_BORROWS } } /// 跳数帽累计。 pub fn intent_hop_capped_ |
| kernel/src/telemetry.rs | 404 | block | 其它 | unsafe { INTENT_HOP_CAPPED } } /// 时间帽累计。 pub fn intent_budget_c |
| kernel/src/telemetry.rs | 410 | block | 其它 | unsafe { INTENT_BUDGET_CAPPED } } // ---- 4-4 IPC fastpath/slowp |
| kernel/src/telemetry.rs | 427 | block | 其它 | unsafe { IPC_FAST_HITS += 1 }; } /// 记一次 slowpath（检查链不满足或无等待者）。 |
| kernel/src/telemetry.rs | 433 | block | 其它 | unsafe { IPC_FAST_MISSES += 1 }; } /// 记一次直切（`try_preempt` 切走）。 |
| kernel/src/telemetry.rs | 439 | block | 其它 | unsafe { IPC_DIRECT += 1 }; } /// fastpath 命中累计。 pub fn ipc_fast |
| kernel/src/telemetry.rs | 445 | block | 其它 | unsafe { IPC_FAST_HITS } } /// slowpath 累计。 pub fn ipc_fast_miss |
| kernel/src/telemetry.rs | 451 | block | 其它 | unsafe { IPC_FAST_MISSES } } /// 直切累计。 pub fn ipc_direct_total() |
| kernel/src/telemetry.rs | 457 | block | 其它 | unsafe { IPC_DIRECT } } |
| kernel/src/user.rs | 1100 | fn | 其它 | unsafe fn blob(start: *const u8, end: *const u8) -> &'static [u8 |
| kernel/src/user.rs | 1107 | block | 其它 | unsafe { blob(&__user_good, &__user_good_end) } } /// rogue_ptr |
| kernel/src/user.rs | 1113 | block | 其它 | unsafe { blob(&__user_rogue_ptr, &__user_rogue_ptr_end) } } /// |
| kernel/src/user.rs | 1132 | block | 其它 | unsafe { blob(&__user_spawner, &__user_spawner_end) } } /// wait |
| kernel/src/user.rs | 1139 | block | 其它 | unsafe { blob(&__user_waiter, &__user_waiter_end) } } /// pinger |
| kernel/src/user.rs | 1146 | block | 其它 | unsafe { blob(&__user_pinger, &__user_pinger_end) } } /// ponger |
| kernel/src/user.rs | 1153 | block | 其它 | unsafe { blob(&__user_ponger, &__user_ponger_end) } } /// svcmgr |
| kernel/src/vmar.rs | 226 | block | 页表 | unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_ |
| kernel/src/vmar.rs | 232 | block | 页表 | unsafe { slot.write_volatile(hal::page_descriptor(pa, attr, sh) |
| kernel/src/vmar.rs | 238 | block | 页表 | unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_ |
| kernel/src/vmar.rs | 258 | block | 页表 | unsafe { let r = &self.regions[idx]; (*r.vmo).untrack_map(r.vmo_ |
| kernel/src/vmar.rs | 264 | block | 页表 | unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_ |
| kernel/src/vmar.rs | 277 | block | 页表 | unsafe { let e = slot.read_volatile(); slot.write_volatile((e & |
| kernel/src/vmar.rs | 284 | block | 页表 | unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_ |
| kernel/src/vmar.rs | 327 | block | 页表 | unsafe { slot.read_volatile() }; if e & TABLE_BIT != 0 { Ok((e a |
| kernel/src/vmar.rs | 334 | block | 页表 | unsafe { slot.write_volatile(hal::table_descriptor(pa)) }; Ok(pa |
| kernel/src/vmar.rs | 341 | block | 页表 | unsafe { Self::table_of(self.l1_pa).add((va >> 30) & IDX_MASK) } |
| kernel/src/vmar.rs | 342 | block | 页表 | unsafe { Self::table_of(l2_pa).add((va >> 21) & IDX_MASK) })?; O |
| kernel/src/vmar.rs | 343 | block | 页表 | unsafe { Self::table_of(l3_pa).add((va >> 12) & IDX_MASK) }) } / |
| kernel/src/vmar.rs | 350 | block | 页表 | unsafe { l1.add((va >> 30) & IDX_MASK).read_volatile() }; if e0 |
| kernel/src/vmar.rs | 356 | block | 页表 | unsafe { l2.add((va >> 21) & IDX_MASK).read_volatile() }; if e1 |
| kernel/src/vmar.rs | 361 | block | 页表 | unsafe { l3.add((va >> 12) & IDX_MASK) }) } /// 清一段区间的 L3 叶（逐槽写 |
| kernel/src/vmar.rs | 371 | block | 页表 | unsafe { slot.write_volatile(0) }; } } } } /// 只读走查任意表树的 L3 叶描述符 |
| kernel/src/vmar.rs | 386 | block | 页表 | unsafe { l1.add((va >> 30) & IDX_MASK).read_volatile() }; if e0 |
| kernel/src/vmar.rs | 391 | block | 页表 | unsafe { l2.add((va >> 21) & IDX_MASK).read_volatile() }; if e1 |
| kernel/src/vmar.rs | 396 | block | 页表 | unsafe { l3.add((va >> 12) & IDX_MASK).read_volatile() }; (leaf |
| kernel/src/vmo.rs | 188 | block | 其它 | unsafe { core::ptr::copy_nonoverlapping( phys_to_virt(pa + off) |
| kernel/src/vmo.rs | 211 | block | 其它 | unsafe { core::ptr::copy_nonoverlapping( data.as_ptr().add(pos - |
| kernel/src/vmo.rs | 318 | block | 其它 | unsafe { core::ptr::write_bytes(phys_to_virt(pa) as *mut u8, 0, |
| mm/src/memmap.rs | 117 | fn | 其它 | unsafe fn desc_at(pa: u64, desc_size: u32, i: usize) -> &'static |
| mm/src/memmap.rs | 120 | block | 其它 | unsafe { &*(crate::va::phys_to_virt(p) as *const EfiMemdesc) } } |
| sched/src/class.rs | 53 | block | 其它 | unsafe { rq.link_back(NonNull::from(&mut *t)) }; } fn dequeue(&s |
| sched/src/class.rs | 58 | block | 其它 | unsafe { rq.unlink(NonNull::from(&mut *t)) }; t.state = TaskStat |
| sched/src/class.rs | 65 | block | 其它 | unsafe { let h = rq.head()?; // 一圈记最高 effective（环上节点必有 next——环形不 |
| sched/src/class.rs | 86 | block | 其它 | unsafe { &mut *chosen.as_ptr() }; // AI 策略注入留桩（3-4c，`AI调度设计.md` |
| sched/src/class.rs | 100 | block | 其它 | unsafe { LAST_VCT }; unsafe { LAST_VCT = now }; t.slice_left = t |
| sched/src/class.rs | 101 | block | 其它 | unsafe { LAST_VCT = now }; t.slice_left = t.slice_left.saturatin |
| sched/src/policy.rs | 50 | block | 其它 | unsafe { TABLE = Some(tbl) }; } /// 解绑：恢复「接口缺席」态（等价性测试的对照分支；ktes |
| sched/src/policy.rs | 56 | block | 其它 | unsafe { TABLE = None }; } /// 接口是否在位（测试观察用）。 pub fn bound() -> |
| sched/src/policy.rs | 63 | block | 其它 | unsafe { TABLE }; tbl.is_some() } /// 查表：快路径纪律（§2「开销恒定」）在**空态**上 |
| sched/src/policy.rs | 83 | block | 其它 | unsafe { TABLE } { Some(t) => t, None => return, }; if tbl.is_em |
| sched/src/policy.rs | 107 | block | 其它 | unsafe { TABLE } { Some(t) if !t.is_empty() => match lookup(t, t |
| sched/src/runqueue.rs | 62 | block | 其它 | unsafe { let mut cur = h; let mut steps = 0; loop { steps += 1; |
| sched/src/runqueue.rs | 93 | fn | 其它 | unsafe fn link_back(&self, task: NonNull<Task>) { let t = task.a |
| sched/src/runqueue.rs | 130 | fn | 其它 | unsafe fn unlink(&self, task: NonNull<Task>) { let t = task.as_p |
| sched/src/runqueue.rs | 170 | fn | 其它 | unsafe fn advance_past(&self, node: NonNull<Task>) { // SAFETY: |
| sched/src/runqueue.rs | 186 | block | 其它 | unsafe { let mut cur = h; loop { if (*cur.as_ptr()).prio_effecti |
| sched/src/sched.rs | 72 | fn | 其它 | unsafe fn rq() -> &'static mut Runqueue { &mut *addr_of_mut!(RUN |
| sched/src/sched.rs | 80 | block | 其它 | unsafe { CURRENT = addr_of_mut!(IDLE_TASK); } } /// 生成一个内核线程：分配 |
| sched/src/sched.rs | 101 | block | 其它 | unsafe { RR.enqueue(task, rq()); } } /// 同 [`spawn`]，但带基础优先级（4-3 |
| sched/src/sched.rs | 116 | block | 其它 | unsafe { RR.enqueue(task, rq()); } } /// 同 [`spawn_with`]，但挂内核侧* |
| sched/src/sched.rs | 136 | block | 其它 | unsafe { RR.enqueue(task, rq()); } } /// 生成一个**用户**线程（2b-4 首个 EL |
| sched/src/sched.rs | 169 | block | 其它 | unsafe { RR.enqueue(task, rq()); } Some(task as *mut Task as usi |
| sched/src/sched.rs | 180 | block | 其它 | unsafe { CURRENT }; if cur.is_null() { 0 } else { unsafe { (*cur |
| sched/src/sched.rs | 184 | block | 其它 | unsafe { (*cur).proc_tag } } } /// 当前任务钉住的 Process 对象键（C-4b `pro |
| sched/src/sched.rs | 194 | block | 其它 | unsafe { CURRENT }; if cur.is_null() { 0 } else { unsafe { (*cur |
| sched/src/sched.rs | 198 | block | 其它 | unsafe { (*cur).pinned } } } /// Plan B v0.5：写任务的 `pinned` 钉住字段（ |
| sched/src/sched.rs | 209 | block | 其它 | unsafe { (*(task as *mut Task)).pinned = pinned }; } /// T-1：写任务 |
| sched/src/sched.rs | 219 | block | 其它 | unsafe { (*(task as *mut Task)).thread_key = thread_key }; } /// |
| sched/src/sched.rs | 233 | block | 其它 | unsafe { RR.enqueue(task, rq()); } } /// 4-3c：send 侧 intent 快照（s |
| sched/src/sched.rs | 243 | block | 其它 | unsafe { CURRENT }; if cur.is_null() { None } else { Some(crate: |
| sched/src/sched.rs | 247 | block | 其它 | unsafe { &*cur }, now_ns)) } } /// 4-3c：recv 侧 intent 结算（slowpat |
| sched/src/sched.rs | 256 | block | 其它 | unsafe { CURRENT }; if cur.is_null() { None } else { Some(crate: |
| sched/src/sched.rs | 260 | block | 其它 | unsafe { &mut *cur }, m, opt_out, now_ns)) } } /// 2b-2：任务地址空间激活 |
| sched/src/sched.rs | 269 | fn | 上下文切换 | unsafe fn activate_if_changed(prev: *mut Task, next: *mut Task) |
| sched/src/sched.rs | 280 | block | 上下文切换 | unsafe { let prev = CURRENT; if let Some(next) = RR.pick_next(rq |
| sched/src/sched.rs | 313 | block | 上下文切换 | unsafe { switch_away_forever_with(0) } } /// 当前任务带退出码退出（Plan B v |
| sched/src/sched.rs | 321 | block | 上下文切换 | unsafe { switch_away_forever_with(code) } } /// 用户任务致死计数（G1.5 验收 |
| sched/src/sched.rs | 337 | fn | 其它 | unsafe extern "C" fn user_fault_exit() -> ! { let cur = CURRENT; |
| sched/src/sched.rs | 358 | fn | 上下文切换 | unsafe fn switch_away_forever_with(code: i32) -> ! { let cur = C |
| sched/src/sched.rs | 403 | block | 其它 | unsafe { EXIT_HOOK = Some(f) }; } /// 待回收任务队列（cap 32 ≈ 一个 bsh 会话 |
| sched/src/sched.rs | 415 | fn | 其它 | unsafe fn reapq_push(t: *mut Task) { if REAPQ_LEN < REAPQ_CAP { |
| sched/src/sched.rs | 435 | block | 其它 | unsafe { let n = REAPQ_LEN; for i in 0..n { let p = REAPQ[i]; RE |
| sched/src/sched.rs | 483 | fn | 其它 | unsafe fn kill_all_tasks_by_pinned( pinned: usize, code: i32, ) |
| sched/src/sched.rs | 492 | block | 其它 | unsafe { loop { // ① SLEEPQ 扫描（Blocked：wait_one/wait_many/纯 slee |
| sched/src/sched.rs | 562 | block | 其它 | unsafe { SWITCHES } } // ---- 切换遥测钩子（3-4c；`可观测性设计.md` per-CPU 无锁 |
| sched/src/sched.rs | 591 | block | 上下文切换 | unsafe { SWITCH_HOOK = hook }; } /// 切换位点共用的遥测发射点：在 `CURRENT = n |
| sched/src/sched.rs | 597 | fn | 其它 | unsafe fn note_switch(prev: *mut Task, next: *mut Task, kind: Sw |
| sched/src/sched.rs | 599 | block | 其它 | unsafe { let hook = SWITCH_HOOK; if let Some(h) = hook { h(&*pre |
| sched/src/sched.rs | 620 | block | 其它 | unsafe { WAKE_HOOK = hook }; } /// 两个唤醒位共用的遥测发射点：在任务**摘队、置因、入 ru |
| sched/src/sched.rs | 627 | fn | 其它 | unsafe fn note_wake(t: *mut Task, reason: WakeReason) { // SAFET |
| sched/src/sched.rs | 629 | block | 其它 | unsafe { WAKE_HOOK }; if let Some(h) = hook { h(unsafe { &*t }, |
| sched/src/sched.rs | 631 | block | 其它 | unsafe { &*t }, reason); } } // ---- SLEEPQ 变更钩子（4-3b PI；内核 crat |
| sched/src/sched.rs | 649 | block | 其它 | unsafe { SLEEPQ_HOOK = hook }; } /// 睡眠者集合变更的发射点（见 [`SleepqHook` |
| sched/src/sched.rs | 655 | fn | 其它 | unsafe fn note_sleepq_changed() { let hook = unsafe { SLEEPQ_HOO |
| sched/src/sched.rs | 656 | block | 其它 | unsafe { SLEEPQ_HOOK }; if let Some(h) = hook { h(); } } // ---- |
| sched/src/sched.rs | 696 | fn | 其它 | unsafe fn sleepq() -> &'static mut SleepQueue { &mut *addr_of_mu |
| sched/src/sched.rs | 706 | fn | 其它 | unsafe fn insert_sorted(&self, task: NonNull<Task>) { let t = ta |
| sched/src/sched.rs | 762 | fn | 其它 | unsafe fn unlink(&self, task: NonNull<Task>) { let t = task.as_p |
| sched/src/sched.rs | 798 | block | 其它 | unsafe { let cur = CURRENT; // 填等待描述（唤醒方 wake_matching 的匹配依据）+ 入 |
| sched/src/sched.rs | 850 | fn | 其它 | unsafe fn wake_one_inner(t: NonNull<Task>) { // SAFETY: 契约如上；SLE |
| sched/src/sched.rs | 852 | block | 其它 | unsafe { let p = t.as_ptr(); sleepq().unlink(t); (*p).wakeup.set |
| sched/src/sched.rs | 882 | block | 其它 | unsafe { let mut cur = sleepq().head.get(); while let Some(c) = |
| sched/src/sched.rs | 897 | block | 其它 | unsafe { note_sleepq_changed() }; n } // ---- fastpath/slowpath |
| sched/src/sched.rs | 915 | block | 其它 | unsafe { let mut cur = sleepq().head.get(); while let Some(c) = |
| sched/src/sched.rs | 942 | fn | 其它 | unsafe fn fast_wake(t: NonNull<Task>) { // SAFETY: 契约如上。 unsafe |
| sched/src/sched.rs | 944 | block | 其它 | unsafe { wake_one_inner(t); note_sleepq_changed(); } } /// 4-4 直 |
| sched/src/sched.rs | 972 | block | 其它 | unsafe { let cur = CURRENT; // ktest 护栏（复盘雷 3）：`sched::init` 前 C |
| sched/src/sched.rs | 1015 | fn | MMIO | unsafe fn expire_timeouts() { let now = hal::timer::cntvct(); lo |
| sched/src/sched.rs | 1030 | block | 其它 | unsafe { note_sleepq_changed() }; } // ---- PI 图推导支撑原语（4-3b；消费者 |
| sched/src/sched.rs | 1041 | block | 其它 | unsafe { let mut cur = sleepq().head.get(); while let Some(n) = |
| sched/src/sched.rs | 1066 | block | 其它 | unsafe { sleepq().head.get() == Some(core::ptr::NonNull::from(t) |
| sched/src/sched.rs | 1076 | block | 其它 | unsafe { sleepq().insert_sorted(NonNull::from(t)) }; } /// 把 `t` |
| sched/src/sched.rs | 1083 | block | 其它 | unsafe { sleepq().unlink(NonNull::from(t)) }; } /// 用户任务致死计数只读访问 |
| sched/src/sched.rs | 1089 | block | 其它 | unsafe { USER_KILLS } } /// 最近被杀用户任务名只读访问（watchdog 断言「杀对了人」用；空串 |
| sched/src/sched.rs | 1095 | block | 其它 | unsafe { LAST_KILLED } } /// 环/SLEEPQ 完整性周期走查（复盘雷 2 残留诊断）：环分裂成「头 |
| sched/src/sched.rs | 1108 | block | 其它 | unsafe { let steps = rq().ring_check(); let want = rq().len(); a |
| sched/src/sched.rs | 1152 | block | 其它 | unsafe { rq().is_empty() && sleepq().len.get() == 0 }; if empty |
| sched/src/sched.rs | 1156 | block | MMIO | unsafe { hal::timer::mask() }; hal::enable_irq(); // 允许 IRQ 唤醒 w |
| sched/src/sched.rs | 1164 | block | MMIO | unsafe { hal::timer::arm_slice() }; hal::enable_irq(); yield_now |
| sched/src/sched.rs | 1186 | fn | 其它 | unsafe extern "C" fn on_irq_tick(_tf: &mut hal::trap::TrapFrame) |
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
