//! berry-hal-aarch64 —— ARM64 HAL（arch 层，unsafe 收敛地）。
//!
//! Phase 0：boot 入口（EL2→EL1 降级，见 `boot.rs`）+ PL011 UART 输出 + halt。
//! Phase 1 Step 4：stage-1 页表 + 开 MMU（见 [`mmu`]、[`paging`]）。
//! Phase 1 Step 6：异常向量表 + 同步异常/IRQ 处理（见 [`trap`]）。
//! Phase 1 Step 7：GICv2 中断控制器（见 [`gic`]）。
//! Phase 1 Step 8：CNTV 虚拟定时器（见 [`timer`]）。
//! Phase 1 Step 9：SVC syscall 派发（见 [`syscall`]）。
//! Phase 2a Step 1：任务上下文 + 上下文切换 asm（见 [`task`]）。
//! 仅 `#![no_std]`；`panic_handler` / `no_main` 由最终 bin crate（`kernel/`）提供。
#![no_std]

mod boot;
pub mod fwcfg;
pub mod gic;
pub mod kern_lock;
pub mod lockdep;
mod mmio;
mod mmu;
mod paging;
pub mod pl061;
pub mod psci;
pub mod smp;
pub mod syscall;
pub mod task;
pub mod tlbshoot;
pub mod timer;
pub mod trap;
pub mod uart;
pub mod wake;
pub mod wake_gen;

// P5 A-1：UART TX 四函数迁入 `uart` 模块——crate root 重导出转发，crate 内
// 既有 `use crate::uart_putc` 与下游 `berry_hal_aarch64::uart_putc` 均零改动。
pub use uart::{uart_put_dec, uart_put_hex64, uart_putc, uart_puts};

// ---- 平台内存图（QEMU `virt`）----
// RAM 几何原在 berry-mm，现挪进 HAL（平台层；berry-mm 保持 arch 无关，由调用方传入）。

/// 物理内存起点（QEMU `virt` RAM）。
pub const RAM_BASE: usize = 0x4000_0000;
/// 物理内存**最大**窗口（256MiB）：boot 页表/probe 上界；实际可用顶由
/// handoff `dram_top` 决定（128MiB 默认，`-m 256M` 时 256MiB）。
pub const RAM_TOP: usize = 0x5000_0000;
/// 无 handoff（QEMU `-kernel`）时的默认 RAM 顶（128MiB）。
pub const RAM_TOP_DEFAULT: usize = 0x4800_0000;
/// QEMU `virt` PL011 UART0 基址。
pub const UART0_BASE: usize = 0x0900_0000;
/// QEMU `virt` 第二 PL011（UART1）基址（sim-S1：PM-16 §2/§3、`docs/sim设备通道
/// 与契约设计-v1.1.md` §2）。**存在性 = 拓扑事实非常量**：DTB 实证（QEMU 11.0.3
/// dumpdtb + info mtree）——配置第二个 `-serial` 才建设备（节点 pl011@9040000，
/// SPI 8/GIC INTID 40、LEVEL_HIGH）；单串口配置**该地址是洞**（mtree 无映射——
/// 盲探 PID = external abort）。内核面在席门 = fw_cfg 具名键 `opt/berry/simdev`
///（GUI P2 `opt/berry/display` 同款拓扑真值纪律；justfile 同一行配 serial1+键，
/// 裸跑/EFI 零扰动）——门开才许碰此基址。virt 机型稳定 ABI（virt-2.4 起未变）。
pub const UART1_BASE: usize = 0x0904_0000;
/// QEMU `virt` PL061 GPIO 基址（sim-S2 A4②：电源键通道——GPIO3 = QMP
/// `system_powerdown` 脉冲，SPI 7 → GIC INTID 39）。与 UART1 的关键差异：
/// PL061 是**板级常在设备**（非 `-serial` 配置依赖）——内核面在席门仍与
/// UART1 同线（fw_cfg `opt/berry/simdev` 注入族策略门，PM-16 §2/§3）：门开
/// 才使能 SPI 39，裸跑/EFI 零扰动。同落 2MiB Device 块（`mmu.rs
/// build_boot_pt` 已覆盖），零页表工作。
pub const PL061_BASE: usize = 0x0903_0000;
/// GICv2 distributor 基址（QEMU `virt`）。
pub const GICD_BASE: usize = 0x0800_0000;
/// GICv2 CPU interface 基址（QEMU `virt`）。
pub const GICC_BASE: usize = 0x0801_0000;
/// virtio-mmio 传输层窗口基 PA（QEMU `virt` 板 32 槽之槽 0）。板子按槽
/// 31→0 倒序创建总线、首个 `-device` 落**最高槽**（monitor `info qtree`
/// 实证：virtio-blk 挂 virtio-mmio-bus.31 = 0x0A00_3E00）——**设备落位随
/// `-device` 顺序漂移，消费端须扫窗定位、不可绑死槽号**（DTB 枚举随 v2
/// 真机路径——`dtb.rs` 已留位）。P6 A-2 起用户态驱动地基：boot 把
/// `[此值, +VIRTIO_MMIO_LEN)` 整窗包成 MMIO VMO 授 svcmgr。
pub const VIRTIO_MMIO0_PA: usize = 0x0A00_0000;
/// virtio-mmio 传输层整窗幅：32 槽 × 槽距 0x200（QEMU 11 `virt` 板
/// monitor `info mtree` 实证：`[0x0A00_0000, 0x0A00_4000)` 连续 32 个
/// virtio-mmio 区域，空槽也在席应答规范 ID 头）。VMO 页对齐交付 = 4 页。
pub const VIRTIO_MMIO_LEN: usize = 0x4000;

/// 内核高半基址 = 39-bit VA 高半的底（T1SZ=25 → TTBR1 区 = `[此值, 2^64)`）。
/// `KERNEL_VA_BASE + pa` 是内核 direct map 的**唯一规则**（RAM 与 MMIO 统一，见
/// `内存管理设计.md` §5）；bits[38:0]=0 使 KVA 表索引 == PA 表索引。2b-1 起
/// 内核主体已链高半；`berry_mm::va` 的交换点由 `rust_main` 注入此值（x86_64 最小
/// 集移植时按目标另定，注入 0 = 恒等）。
pub const KERNEL_VA_BASE: usize = 0xFFFF_FF80_0000_0000;

/// 启动后期初始化：装异常向量表 + 初始化 GIC。
/// （MMU/页表已在 boot 期由 `.text.boot` 的 `build_boot_pt`/`enable_mmu` 就绪，
/// 2b-1 起 `rust_main` 进来时内核已在高半执行。）
///
/// 必须在 `rust_main`、单核、EL1、MMU 开时调用一次。
pub fn init() {
    // SAFETY: EL1、MMU 已开（向量表在直映的内核镜像内）；任何 DAIF 解蔽之前。
    unsafe { trap::init() };
    // SAFETY: MMIO（Device 映射区）；EL1、单核、DAIF 仍屏蔽。
    unsafe { gic::init() };
}

/// 高半别名自校验（2b-2 起只走高半——低半身份已退役）：RAM 魔数 + GIC PIDR2
/// 经高半直映读回。证 TTBR1 翻译生效——逐进程页表 / 低半退役的地基。
pub fn kva_ok() -> bool {
    mmu::kva_alias_probe()
}

/// 2b-2：探测 ASID 位宽（8/16；`ID_AA64MMFR0_EL1.ASIDBits`）。实测值打进关卡记录
/// （`内存管理设计.md` §5.2）。
pub fn asid_bits() -> u32 {
    mmu::asid_bits()
}

/// 2b-2：激活任务地址空间——TTBR0 ← 表+ASID、CONTEXTIDR、`tlbi aside1is`
/// （变化才调，比较由调度器做）。`ttbr0_pa == 0` = 无 AS → 内核空表（低半必然
/// fault——身份表已退役）。填表方须先 `dsb ish`（本函数只管寄存器）。
pub fn activate_aspace(ttbr0_pa: usize, asid: u16) {
    // SAFETY: EL1、单核、切换临界区（IRQ 屏蔽）内调用。
    unsafe { mmu::activate_ttbr0(ttbr0_pa, asid) };
}

/// 2b-2：低半身份表退役——TTBR0 ← 内核空表 + `tlbi vmalle1is`。此后低半任何
/// 访问必然 fault（ktest 期望故障正向断言）。回退开关见 `mmu::RETIRE_IDENTITY`。
pub fn retire_low_half() {
    // SAFETY: EL1、启动单线程、任何任务切走之前调用一次。
    unsafe { mmu::retire_low_half() };
}

/// 全量 ASID 失效（ASID 分配器回卷时调用；用户叶 nG=1，按号失效可达）。
pub fn flush_all_asid() {
    // SAFETY: EL1。
    unsafe { mmu::flush_all_asid() };
}

/// 2b-3：按 ASID 失效 TLB——Vmar `unmap`/`protect` 改叶后调用（用户叶 nG=1，
/// 按号可达；boot 时代 global 项与此无关）。
pub fn tlbi_aside1(asid: u16) {
    // SAFETY: EL1。
    unsafe { mmu::tlbi_aside1(asid) };
}

/// 3-2：读当前 TTBR0_EL1 表基 PA——syscall copy 路径走查调用者自身表用
/// （EL0 svc 时 TTBR0 仍是调用者页表）。
pub fn current_ttbr0_pa() -> usize {
    mmu::current_ttbr0_pa()
}

/// S-4 4-2：读当前 TTBR0_EL1 的 ASID 字段（bits[63:48]）——per-AS PT 锁的
/// 读者键（与 [`current_ttbr0_pa`] 同一读点，同窗一致）。
pub fn current_asid() -> u16 {
    mmu::current_asid()
}

// 描述符构造导出（2b-2 起内核侧填用户页表用；boot 建表在 crate 内部直接用）。
// P6 A-2 起加 ATTR_DEVICE/SH_NONE：用户侧 MMIO 窗口叶（Device-nGnRnE + 不
// 共享——内核直映 MMIO 同款，见 mmu.rs）。
pub use paging::{
    page_descriptor, table_descriptor, AP_RO, AP_USER_RW, ATTR_DEVICE, ATTR_NORMAL, SH_INNER,
    SH_NONE,
};

// ---- IRQ 派发回调（HAL→上层，函数指针槽反转）----
//
// HAL 是叶子 crate，**不依赖** berry-sched（否则循环依赖：sched → hal → sched）。
// 但 timer/IRQ 抖动需要通知调度器（Step 5 打印 tick、Step 6 驱动抢占）——用**函数指针
// 槽**把调用反转：调度器不进 HAL 的依赖图，`内核` 在 boot 时注册回调（`set_irq_dispatch`），
// `exception_irq` 经槽调。HAL 永不知 scheduler 之名（plan 循环依赖规避 + R10）。

/// IRQ 派发回调类型（EOIR 之后、`eret` 之前调；C-ABI 约定——ADR #18 syscall/FFI 边界）。
pub type IrqDispatch = unsafe extern "C" fn(&mut trap::TrapFrame);

static mut IRQ_DISPATCH: Option<IrqDispatch> = None;

/// 注册 IRQ 派发回调。须在**任何 `enable_irq` 之前**调用一次（否则首 IRQ 落空槽——
/// 由下方 `if let Some` 平凡处理，但 boot 期就该注册好）。
pub fn set_irq_dispatch(f: IrqDispatch) {
    // SAFETY: 启动单线程、DAIF 屏蔽（enable_irq 前）；无并发 IRQ、无竞态。
    unsafe { IRQ_DISPATCH = Some(f) };
}

/// 取已注册的 IRQ 派发回调（`exception_irq` 用）。未注册时返回 `None`。
///
/// # Safety
/// 读 `static mut`。仅 IRQ 上下文（单核顺序）调用。
pub unsafe fn irq_dispatch() -> Option<IrqDispatch> {
    IRQ_DISPATCH
}

/// 入队 SGI（`SGI_RESCHED_US`）接收侧回调槽（义务⑧ S-3b b-1）——与 tick 槽
/// **分立**：SGI 路径无 `timer::handle`/RR.tick 记账面（快速重调度 ≠ tick 语义），
/// 且副核不进（park 承接架构——SGI 醒经 eret 回 park 环，handler 内零调度；
/// gate 在回调体内）。注册/调用模式同上（EOIR 之后、`eret` 之前；C-ABI）。
pub type SgiDispatch = unsafe extern "C" fn(&mut trap::TrapFrame);

static mut SGI_DISPATCH: Option<SgiDispatch> = None;

/// 注册 SGI 派发回调。须在任何 `enable_irq` 之前调用一次（同 `set_irq_dispatch`）。
pub fn set_sgi_dispatch(f: SgiDispatch) {
    // SAFETY: 启动单线程、DAIF 屏蔽（enable_irq 前）；无并发 IRQ、无竞态。
    unsafe { SGI_DISPATCH = Some(f) };
}

/// 取已注册的 SGI 派发回调（`exception_irq` SGI 分支用）。未注册返回 `None`。
///
/// # Safety
/// 读 `static mut`。仅 IRQ 上下文（单核顺序）调用。
pub unsafe fn sgi_dispatch() -> Option<SgiDispatch> {
    SGI_DISPATCH
}

// ---- 用户故障击杀回调（HAL→sched，函数指针槽反转；同上避循环依赖）----
//
// 2b-4 G1.5：EL0 用户任务致命故障（野指针/畸形 syscall/brk）→ `exception_sync_el0`
// 经此槽让**调度器**杀当前任务（出 runqueue + 切走，只死自己）。HAL 永不知
// scheduler 之名。

/// 用户故障击杀回调类型（同步异常上下文调用；`-> !`——切走永不回）。
pub type UserFaultExit = unsafe extern "C" fn() -> !;

static mut USER_FAULT_EXIT: Option<UserFaultExit> = None;

/// 注册用户故障击杀回调。须在**首个用户任务被调度之前**调用一次。
pub fn set_user_fault_exit(f: UserFaultExit) {
    // SAFETY: 启动单线程、DAIF 屏蔽；无并发 IRQ、无竞态。
    unsafe { USER_FAULT_EXIT = Some(f) };
}

/// 取已注册的用户故障击杀回调（`exception_sync_el0` 用）。未注册时返回 `None`。
///
/// # Safety
/// 读 `static mut`。仅同步异常上下文（单核顺序）调用。
pub unsafe fn user_fault_exit() -> Option<UserFaultExit> {
    USER_FAULT_EXIT
}

// ---- G4-1 按需调页故障探针槽（HAL→内核 mm，同上反转模式）----
//
// D-abort（current-EL 0x25 / lower-EL 0x24）在杀链**之前**先问此槽：
// 「FAR 命中 demand 区间吗？」——命中则槽内实现（内核 dpg 模块）走 pager
// 协议（阻塞/供给/唤醒）并返回 true，eret 重试；未命中返回 false，原
// 杀链不变量保持。未注册（boot 早期）恒 false = 零行为面。

/// 按需调页故障探针类型：`esr` = 同步异常 ESR_EL1（实现方自滤 ISS——
/// demand 只认翻译缺页 fsc 4..=7，权限/访问标志 fault 归原杀链，G4-1 三镜
/// mech-M-2 活锁株）；`far` = 故障 VA；`true` = 已处理（fault 线程已
/// 阻塞又唤醒、页已供给），eret 重试即可；`false` = 非 demand 面，调用方
/// 走原路径。实现契约：同步异常上下文调用；**实现方自管 IRQ 屏蔽/KL 窗**
///（same-EL 异常不自动置 PSTATE.I——taken-to-higher-EL 才置；三镜 P1-1）；
/// 返回 true 前须完成供给（或保证重试指令再 fault 时有出路——递归防线归
/// 内核侧）。
pub type DemandFaultProbe = unsafe extern "C" fn(esr: usize, far: usize) -> bool;

static mut DEMAND_FAULT_PROBE: Option<DemandFaultProbe> = None;

/// G4-2 第二槽：EL0 臂专用探针（current-EL 臂与 EL0 臂分流——dpg 内核
/// pager 对不劫 EL0 请求，arm 侧按臂武装不同请求槽/唤不同键，§5-③ 裁决）。
static mut DEMAND_FAULT_PROBE_EL0: Option<DemandFaultProbe> = None;

/// 注册按需调页故障探针。须在首个可能触 demand 区间访问的任务被调度之前
/// 调用一次（boot DAIF 屏蔽窗）。
pub fn set_demand_fault_probe(f: DemandFaultProbe) {
    // SAFETY: 启动单线程、DAIF 屏蔽；无并发 IRQ、无竞态。
    unsafe { DEMAND_FAULT_PROBE = Some(f) };
}

/// 注册 EL0 臂按需调页探针（注册窗合同同 [`set_demand_fault_probe`]）。
pub fn set_demand_fault_probe_el0(f: DemandFaultProbe) {
    // SAFETY: 同上——boot 屏蔽窗单线程。
    unsafe { DEMAND_FAULT_PROBE_EL0 = Some(f) };
}

/// 取已注册的按需调页探针（`exception_sync` current-EL 0x25 臂用）。
/// 未注册/未命中 → `false`（= 原路径不变量保持）。
///
/// # Safety
/// 读 `static mut`。仅同步异常上下文（单核顺序）调用。
pub unsafe fn demand_fault_probe(esr: usize, far: usize) -> bool {
    // SAFETY: 本函数自身契约即异常上下文；槽注册于 boot 屏蔽窗后恒定。
    match DEMAND_FAULT_PROBE {
        Some(f) => unsafe { f(esr, far) },
        None => false,
    }
}

/// 取 EL0 臂探针（`exception_sync_el0` D-abort 0x24 臂用）；合同同
/// [`demand_fault_probe`]。
///
/// # Safety
/// 读 `static mut`。仅同步异常上下文（单核顺序）调用。
pub unsafe fn demand_fault_probe_el0(esr: usize, far: usize) -> bool {
    // SAFETY: 同上。
    match DEMAND_FAULT_PROBE_EL0 {
        Some(f) => unsafe { f(esr, far) },
        None => false,
    }
}

// ---- G3-1b 落点①：syscall 返回边界检查点槽（HAL→sched 反转，避循环依赖；
// 上方 IRQ/SGI/用户故障槽同模式——HAL 永不知 scheduler 之名）----
//
// SVC 派发尾（`syscall::dispatch` 的 svc_exit_class 毕后、x0 写回前）调用
// 的显式抢占检查点；kernel boot 期注册 berry_sched::preempt_checkpoint
//（计数==0 探峰 + 可能直切，自带 irqsave 包络——sched 侧安全 API）。
// 零行为变更步（g3-1b）：现役 SVC 全程 IRQ 屏蔽 ⇒ 单核 rq 状态窗内不可变
// ⇒ 探峰恒空 = 纯空探；g3-1c 域解蔽后承重。
static mut SVC_EXIT_CHECKPOINT: Option<fn()> = None;

/// 注册 syscall 返回边界检查点（kernel boot 在 enable_irq / 首个用户任务
/// 被调度前调用一次；槽空 = 派发尾平凡跳过）。
pub fn set_svc_exit_checkpoint(f: fn()) {
    // SAFETY: boot 单线程（IRQ 未开）注册，此后全系统只读。
    unsafe { SVC_EXIT_CHECKPOINT = Some(f) };
}

/// 解蔽 IRQ（`daifclr #2`，清 DAIF.I）。解蔽序的最后一步（VBAR→GIC→timer→此）。
pub fn enable_irq() {
    // SAFETY: 写 DAIF 标志位；EL1。
    unsafe { core::arch::asm!("msr daifclr, #2", options(nostack, preserves_flags)) };
}

/// 存当前 DAIF 全字（G3-1b 检查点 irqsave 包络用）。返回值只可回喂
/// [`irq_restore`]，不得解读位含义（跨形态只保往返配对语义）。
pub fn irq_save() -> u64 {
    let daif: u64;
    // SAFETY: 读 DAIF 标志寄存器；EL1。
    unsafe { core::arch::asm!("mrs {}, daif", out(reg) daif, options(nostack, preserves_flags)) };
    daif
}

/// 恢复 DAIF（与 [`irq_save`] 配对；含屏蔽→解蔽 / 解蔽→屏蔽两向，恢复的是
/// 保存点快照而非定向操作——G3-1c 域解蔽后窗内 DAIF 形态随落点变，两向都要）。
pub fn irq_restore(daif: u64) {
    // SAFETY: 写 DAIF 标志寄存器；EL1。值仅来自 irq_save 同核快照。
    unsafe { core::arch::asm!("msr daif, {}", in(reg) daif, options(nostack, preserves_flags)) };
}

/// 屏蔽 IRQ（`daifset #2`，置 DAIF.I）。
#[allow(dead_code)]
pub fn disable_irq() {
    unsafe { core::arch::asm!("msr daifset, #2", options(nostack, preserves_flags)) };
}

/// 单次 `wfi`（等中断唤醒一次）。idle 循环用——区别于 [`halt`] 的死循环 wfi。
pub fn wfi() {
    // SAFETY: 特权指令；EL1。
    unsafe { core::arch::asm!("wfi", options(nostack, preserves_flags)) };
}

/// 永久挂起（`wfi` 循环）。DAIF 屏蔽时无中断唤醒 → 真正停；
/// 解蔽后 IRQ 唤醒 wfi、走完向量再回此循环（idle 主循环）。
pub fn halt() -> ! {
    loop {
        // unsafe 收敛在此：特权指令。
        unsafe { core::arch::asm!("wfi") };
    }
}
