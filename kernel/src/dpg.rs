//! G4-1/G4-2 demand paging 内核面（`G4按需调页实施设计草案.md` §4 刀序）。
//!
//! G4-1 闭环形状（内核任务自喂——机制面独立验收）：
//!
//! ```text
//! faulter（内核任务，自带 AS + demand 区间）首触 VA
//!   → current-EL D-abort(0x25) → trap 探针（hal 槽，杀链之前）
//!   → TTBR0 现读 asid → vmar 反查登记表 → demand 区间命中
//!   → 武装请求面（零分配单槽）→ 唤 pager → block_current 睡 fault 键
//! pager（内核任务）poll-first 收请求
//!   → supply_page（pt_lock 窗 commit + 填叶 + tlbi + broadcast）
//!   → 经 VMO direct map 写 magic → wake faulter → 回睡
//! faulter 醒（block_current 返回）→ 探针返 true → eret 重试（ELR 不动）
//!   → 读到 magic → 自证 → `DPG ok`
//! ```
//!
//! 递归防线（草案 §2.2）在本刀的对应：pager 自身无 AS 无 demand 区间
//! （防线 1）；请求面 boot 预分配、探针零分配零用户态访问（防线 2）；
//! 深度帽/多 faulter 面 = G4-3 压测档。
//!
//! 死锁防线（G4-1 三镜 P1-1/P2-2① 修后口径）：探针**两段式 klock 窗**——
//! same-EL 同步异常不自动屏蔽 PSTATE.I（硬件只在 taken-to-higher-EL 置位；
//! trampoline `daifclr,#2` 起步即解蔽），klock_enter = disable_irq + 全谱
//! 结构锁即 block_current 契约的「显式 disable_irq」。第一段 asid 反查留
//! KL 外（append-only 登记锁外读安全，miss 零锁即拒）；第二段（命中臂）
//! 全程 KL 窗——与 pager 供给窗互斥（SMP 双向丢唤窗闭）。block_current
//! 舞蹈放锁入队、醒后重取——睡不持锁（§2.3① 语义保持）；pager 供给全链
//! KL 窗内（SVC vmar_map 同构，§2.3②）；填叶 pt_lock 嵌 KL 正序、
//! broadcast 只发不待（§2.3③）；pager 单线程首形（§2.3④）。
//!
//! G4-2 增 EL0 面（§5-③ 裁决：0x21 阻塞收货）：EL0 D-abort 探针武装
//! EL0 请求槽 + 唤 EL0 pager 键；用户态 pager 经 `pager_supply_pages`
//! (0x21) poll-first 收货（无请求睡 pager 键）→ 供给 → 唤 faulter。
//! 与 EL1 面分槽分键（内核 pager 对不劫 EL0 请求）。

use crate::mm;
use crate::vmar::{self, Perm, Vmar};
use crate::vmo::Vmo;
use berry_hal_aarch64 as hal;
use berry_mm::va::PAGE_SIZE;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};

/// fault-in-fault 递归帽（草案 §2.2 防线 3，G4-3 落地）：per-CPU 深度 >N
/// → panic fail loud——防线 1/2（pager AS 全 eager + 处理路径零分配零用户页
/// 访问）失效时的最后响雷，不静默。
///
/// **shape (a) 计数窗 = 活跃处理段**（区域命中 → 武装/唤醒 + 醒后收尾），
/// `block_current` 睡眠**不计**——并发在飞 faulter（睡在探针内的 N 任务）
/// 各自只占活跃窗瞬间，不灌计数；窗内深度 ≥2 只可能是真嵌套异常（防线破）。
const DPG_DEPTH_CAP: u32 = 2; // 草案 N=2 起步：容忍一层嵌套，第三层响雷

// per-CPU 深度（`cpu_id` 索引；Relaxed——异常上下文同核串行，跨核各自独立；
// v1 双核定容，越界即 panic = 不存在配置的 fail loud）。
static DEPTH: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
/// 峰值观测（活证面：正常链恒 1；boot 证据行 `DPG dm=` 读出，非 markers 面）。
static DEPTH_MAX: AtomicU32 = AtomicU32::new(0);

/// 探针入深（异常上下文，提交路径入口）：>帽 = 防线 1/2 破，panic 响雷。
fn depth_enter() {
    let d = DEPTH[hal::smp::cpu_id()].fetch_add(1, Ordering::Relaxed) + 1;
    DEPTH_MAX.fetch_max(d, Ordering::Relaxed);
    assert!(
        d <= DPG_DEPTH_CAP,
        "dpg: fault-in-fault 深度 {} > 帽 {}（防线 1/2 破——pager 自身 demand 化/处理路径触未映射页）",
        d,
        DPG_DEPTH_CAP
    );
}

/// 探针出深（`block_current` 前/返回前各一次——shape (a) 睡不计）。
fn depth_exit() {
    DEPTH[hal::smp::cpu_id()].fetch_sub(1, Ordering::Relaxed);
}

/// faulter AS 内 demand 区间 VA（页 1——root Vmar 管辖界下界；本 AS 无其他
/// 用户映射，读它必走缺页路径）。
const DEMAND_VA: usize = 0x1000;

/// pager 供给后写入 demand 页的 magic（faulter 醒后重读自证供给链闭环）。
const DPG_MAGIC: u64 = 0x6d61_6769_4450_4731; // "DPG1magi"（LE 读回即此值）

/// 裸键握手位（§5-② 裁决：专用等待键不建 Event 对象；位登记 =
/// main.rs `USER_SIGNAL_BITS` 1<<51——fault/pager 双键共用同位，键 = 等待
/// 对象地址，跨键不串）。
pub const DPG_WAKE_BIT: u64 = 1 << 51;

// 等待键：零大小静态取地址作 wait_obj（G3-5c `LAT_TRIG_*` 同族——obj =
// 任意 usize 键，wake_matching 按 (obj, 位相交) 匹配）。
static FAULT_KEY: u8 = 0; // faulter 睡此（pager 供给毕唤醒）
static PAGER_KEY: u8 = 0; // pager 睡此（探针武装请求后唤醒）
static DONE_KEY: u8 = 0; // faulter 自证毕停泊（无人唤醒——park 形）

// ---- 请求面（ADR #9「内核缺页处理零分配」先形：boot 预分配单槽；G4-2 起
// 环化/多槽扩容。单 faulter 首形：槽满语义不存在——探针与 pager 由
// armed 位握手，写入点唯一（探针），取走点唯一（pager swap(false)）。）----

static REQ_FAR: AtomicU64 = AtomicU64::new(0);
static REQ_ASID: AtomicU16 = AtomicU16::new(0);
static REQ_ARMED: AtomicBool = AtomicBool::new(false);

/// fault 探针（hal `DemandFaultProbe` 槽注册；异常上下文调用）。
///
/// 返回 true = 已处理（fault 线程已睡又醒、页已供给），eret 重试即命中；
/// false = 非 demand 面（非翻译缺页 / 未登记 asid / FAR 未落 demand 区间），
/// 原杀链不变量保持。
///
/// 上下文纪律（G4-1 三镜 P1-1/P2-2① 修）：same-EL 同步异常**不**自动屏蔽
/// PSTATE.I——命中路径全程 klock 窗（disable_irq + 全谱结构锁 =
/// block_current 契约「内核任务显式 disable_irq」），且与 pager 供给窗
///（KL）互斥：SMP 双向丢唤窗（FAULT/PAGER 两键方向）闭。**两段式**
///（f4 复核推荐形）：asid 反查表查找留 KL 外（append-only + Release/
/// Acquire 原子对——锁外读安全无 ABA；miss 零锁即拒，rogue fault 快路径
/// 不付全谱窗——trap.rs EL0 臂「快路径零锁」承诺保持）；命中才入 KL 窗
/// **重验** regions（M-3 读竞窗闭）+ arm/wake/block。block_current 舞蹈
/// 放锁入队、醒后重取（dpg_pager 同构）；eret 弹 SPSR 自然恢复 fault 前
/// 屏蔽态。
///
/// fsc 滤（三镜 mech-M-2）：只收翻译缺页（fsc 4..=7，L0-L3）——权限/
/// 访问标志 fault（已映射只读页写等）走原杀链；否则 armed→供给幂等短路→
/// 回 block = 活锁株。醒后重验 = eret 重试天然重验（供给失败则再 fault
/// 再进探针——pager 侧 fail loud 兜底）。
unsafe extern "C" fn probe(esr: usize, far: usize) -> bool {
    // DFSC = ISS[5:0] = ESR[5:0]（ISS 占 ESR[24:0]——低 6 位直达，勿移位）。
    if !matches!(esr & 0x3f, 4..=7) {
        return false; // 非翻译缺页（权限/访问标志/…）——原杀链
    }
    // 第一段（KL 外零锁快拒）：asid 反查——登记 append-only + 原子对，
    // 锁外读安全；miss = 本 AS 无 demand 面，不付全谱窗。
    let asid = hal::current_asid();
    let Some(root) = vmar::demand_root(asid) else {
        return false;
    };
    // 第二段（KL 窗）：regions 权威重验（M-3——变更方经 PT 锁嵌 KL 同序，
    // 锁外快查与入锁间的变更由本窗收口）。
    hal::kern_lock::klock_enter();
    // SAFETY: 登记方（init / G4-2 process 组装面）保证对象常驻（登记只进
    // 不出，Boot-held 先形）。
    if unsafe { (*root).demand_region_at(far).is_none() } {
        hal::kern_lock::klock_exit();
        return false;
    }
    // 命中 = 提交路径：入深（G4-3 深度帽——此后至 block 前的活跃窗计深度）。
    depth_enter();
    // 命中：武装请求面（far/asid 先行、armed 收口 Release——pager 侧
    // Acquire 读 armed 后 far/asid 必可见）。
    REQ_FAR.store(far as u64, Ordering::Relaxed);
    REQ_ASID.store(asid, Ordering::Relaxed);
    REQ_ARMED.store(true, Ordering::Release);
    // 唤 pager（KL 窗内 SQ 嵌套 = SVC 同构；双序通吃：已睡 → 摘队，未睡 →
    // 位/槽锁存 + pager poll-first 追平——f4 F1 纪律同源）。
    berry_sched::wake_matching(&PAGER_KEY as *const u8 as usize, DPG_WAKE_BIT);
    // 出深再睡（shape (a)：睡眠不计——并发在飞 faulter 不灌计数）；舞蹈放
    // KL 入队、醒后重取。
    depth_exit();
    let r = berry_sched::block_current(
        &FAULT_KEY as *const u8 as usize,
        DPG_WAKE_BIT,
        u64::MAX,
    );
    // 醒 = 活跃窗重开（收尾段计深度——此窗内嵌套异常同样响雷）。
    depth_enter();
    hal::kern_lock::klock_exit();
    assert!(
        matches!(r, berry_sched::WakeReason::Signaled),
        "dpg probe: 醒因非 Signaled（fault 键无超时/无他者——唤醒面破损）"
    );
    depth_exit();
    true
}

// ---- G4-2 EL0 面（§5-③ 裁决：0x21 阻塞收货——请求单槽照旧 + 专用键；
// 与 EL1 面分槽，内核 dpg_pager 不劫 EL0 请求。arm 侧按臂唤不同键。）----

static EREQ_FAR: AtomicU64 = AtomicU64::new(0);
static EREQ_ASID: AtomicU16 = AtomicU16::new(0);
static EREQ_ARMED: AtomicBool = AtomicBool::new(false);

static EFAULT_KEY: u8 = 0; // EL0 faulter 睡此（探针内；0x21 供给毕唤醒）
static EPAGER_KEY: u8 = 0; // EL0 pager 睡此（0x21 无请求时；探针武装后唤醒）

/// EL0 fault 探针（hal 第二槽 `*_el0`；`exception_sync_el0` D-abort 臂）。
/// 协议 = [`probe`] 的 EL0 镜像（两段式 + KL 窗 + fsc 滤同款）：武装 EL0
/// 请求槽 + 唤 EL0 pager 键 + 睡 EL0 fault 键——唯一唤醒者 = 0x21 供给毕
///（§5-⑤：pager 不再收货 = 永久阻塞，fail loud-on-hang）。EL0 异常
/// taken-to-higher-EL **自动**屏蔽 PSTATE.I（与 same-EL 臂的差异）——KL
/// 窗仍取（SMP 与 0x21 供给窗的互斥面同 [`probe`]，P2-2①）。
unsafe extern "C" fn probe_el0(esr: usize, far: usize) -> bool {
    // DFSC = ESR[5:0]（同 [`probe`]——ISS 低 6 位直达，勿移位）。
    if !matches!(esr & 0x3f, 4..=7) {
        return false; // 非翻译缺页——原杀链
    }
    let asid = hal::current_asid();
    let Some(root) = vmar::demand_root(asid) else {
        return false; // KL 外零锁快拒（rogue fault 不付全谱窗）
    };
    hal::kern_lock::klock_enter();
    // SAFETY: 同 [`probe`]——登记面常驻；regions 权威重验在本 KL 窗内。
    if unsafe { (*root).demand_region_at(far).is_none() } {
        hal::kern_lock::klock_exit();
        return false;
    }
    // 命中 = 提交路径：入深（G4-3 深度帽，shape (a) 同 [`probe`]）。
    depth_enter();
    // 命中：武装 + 唤 pager + 睡（时序合同同 [`probe`]——Release 收口后
    // far/asid 对 0x21 Acquire 侧必可见；全 KL 窗）。
    EREQ_FAR.store(far as u64, Ordering::Relaxed);
    EREQ_ASID.store(asid, Ordering::Relaxed);
    EREQ_ARMED.store(true, Ordering::Release);
    berry_sched::wake_matching(&EPAGER_KEY as *const u8 as usize, DPG_WAKE_BIT);
    // 出深再睡（shape (a)：睡眠不计）。
    depth_exit();
    let r = berry_sched::block_current(
        &EFAULT_KEY as *const u8 as usize,
        DPG_WAKE_BIT,
        u64::MAX,
    );
    // 醒 = 活跃窗重开（收尾段计深度）。
    depth_enter();
    hal::kern_lock::klock_exit();
    assert!(
        matches!(r, berry_sched::WakeReason::Signaled),
        "dpg probe_el0: 醒因非 Signaled（EL0 fault 键无超时/无他者——唤醒面破损）"
    );
    depth_exit();
    true
}

// ---- 0x21 面小件（syscall.rs `sys_pager_supply_pages` 用；键/槽私有，经
// 函数面外借——SVC 面不触裸键纪律）----

/// 取走武装请求（consume-on-read，dpg_pager swap 同款）——单核 SVC 屏蔽
/// 窗内调用（探针写点在异常上下文同核，KL 窗 = 无并发窗口）。
pub(crate) fn el0_take() -> Option<(usize, u16)> {
    if EREQ_ARMED.swap(false, Ordering::Acquire) {
        Some((EREQ_FAR.load(Ordering::Relaxed) as usize, EREQ_ASID.load(Ordering::Relaxed)))
    } else {
        None
    }
}

/// EL0 pager 等待键（0x21 阻塞收货的睡点）。
pub(crate) fn el0_pager_key() -> usize {
    &EPAGER_KEY as *const u8 as usize
}

/// 唤 EL0 faulter（0x21 出口恒唤——成功 = eret 命中新叶；失败 = 重试再
/// fault 再武装，pager 重调 0x21 自愈；不再调 = 永久阻塞 §5-⑤）。
pub(crate) fn el0_wake_faulter() {
    berry_sched::wake_matching(&EFAULT_KEY as *const u8 as usize, DPG_WAKE_BIT);
}

/// 0x21 源拷贝缓冲（4K 静态：ADR #9 零分配 + SVC 栈预算——KL 窗内单核
/// 独占，无重入面）。
static mut SUPPLY_BUF: [u8; PAGE_SIZE] = [0; PAGE_SIZE];

/// 借源拷贝缓冲（独占窗 = 调用方 KL 持有期，单核串行）。
pub(crate) fn supply_buf() -> &'static mut [u8] {
    // SAFETY: 仅 sys_pager_supply_pages KL 窗内借用；单核屏蔽串行无并发面。
    unsafe { &mut *core::ptr::addr_of_mut!(SUPPLY_BUF) }
}

/// 测试 pager（内核态首形）：poll-first 收请求 → 供给 → 唤 faulter → 回睡。
extern "C" fn dpg_pager(_arg: usize) -> ! {
    loop {
        hal::kern_lock::klock_enter();
        // poll-first（f4 F1 纪律）：swap 取走请求——查见即直取，未查见才睡
        //（查位与入队同 KL 窗，无丢唤醒窗）。
        let armed = REQ_ARMED.swap(false, Ordering::Acquire);
        if !armed {
            // block_current 返回已重取 KL（soakmon 包络同款）——醒后同窗
            // 续环再 poll（虚醒/位锁存均被下一轮 swap 收割）。
            let _ = berry_sched::block_current(
                &PAGER_KEY as *const u8 as usize,
                DPG_WAKE_BIT,
                u64::MAX,
            );
            hal::kern_lock::klock_exit();
            continue;
        }
        let far = REQ_FAR.load(Ordering::Relaxed) as usize;
        let asid = REQ_ASID.load(Ordering::Relaxed);
        // 供给链全 KL 窗内（SVC vmar_map 同构：pt_lock 嵌 KL 正序、分配器
        // KL 串行）——G4-2 的 0x21 SVC 面即把本窗搬进 syscall 派发。
        let root = vmar::demand_root(asid).expect("dpg_pager: asid 反查失败（登记面被撤？）");
        let (vmo, vmo_off, base) = unsafe { (*root).demand_region_at(far) }
            .expect("dpg_pager: 请求 VA 已不在 demand 区间（unmap 竞窗）");
        let va = far & !(PAGE_SIZE - 1);
        unsafe { (*root).supply_page(mm::frames(), va) }
            .expect("dpg_pager: supply_page 失败（帧耗尽/建表失败——fail loud）");
        // 数据面：供给页是清零页——写 magic（经 VMO direct map 写路径；
        // faulter 经其 VA 读同 PA，单核调度切换序即全屏障）。
        let off = vmo_off + (va - base);
        // SAFETY: Region 的 vmo 指针有效性依托对象常驻（init 泄漏持有）。
        unsafe { (*vmo).write(off, &DPG_MAGIC.to_le_bytes()) }
            .expect("dpg_pager: 写 magic 失败（供给页未提交？）");
        // 供给毕唤 faulter（KL 窗内 SQ 嵌套 = SVC 同构）。
        berry_sched::wake_matching(&FAULT_KEY as *const u8 as usize, DPG_WAKE_BIT);
        hal::kern_lock::klock_exit();
    }
}

/// faulter（内核任务，自带 AS）：首触 demand VA = 缺页 → 闭环 → 重试读到
/// pager 写入的 magic → 打 `DPG ok` 关卡标记（smoke 裸相第 28 标记）。
extern "C" fn dpg_faulter(_arg: usize) -> ! {
    // SAFETY: 读自身 AS 的 demand 页（供给前无翻译——本读即断言「缺页 →
    // 供给 → 重试」全链；错值非安全问题是正确性问题）。
    let v = unsafe { core::ptr::read_volatile(DEMAND_VA as *const u64) };
    assert_eq!(
        v, DPG_MAGIC,
        "dpg_faulter: demand 页内容非 pager 供给值（供给链破损）"
    );
    hal::kern_lock::with_print_lock(|| {
        hal::uart_puts(b"DPG ok\n");
        // G4-3 深度活证（草案 §3「递归不爆」观测面；非 markers 数组——
        // smoke 零漂移）：正常链峰值恒 1（dp=0 即探针没跑 = 假绿）。
        hal::uart_puts(b"DPG dm=");
        hal::uart_put_dec(DEPTH_MAX.load(Ordering::Relaxed) as u64);
        hal::uart_puts(b"\n");
    });
    // 自证毕停泊（v1 任务不退出——sleeper 族 park 形；DONE 键无人唤醒）。
    hal::kern_lock::klock_enter();
    let _ = berry_sched::block_current(
        &DONE_KEY as *const u8 as usize,
        DPG_WAKE_BIT,
        u64::MAX,
    );
    unreachable!("dpg_faulter: 停泊键被唤醒（唤醒面越界）")
}

/// boot 装配（main.rs 于 task_a/task_b spawn 后、用户进程 spawn 前调用——
/// 探针注册须先于任何可能触 demand 访问的任务）。
pub fn init() {
    let fa = mm::frames();
    // 对象先泄漏后映射（Region.vmo 指针存的是调用方对象地址——栈对象移入
    // 堆会悬空；MiniAspace 面的 vmo 指针无解引用点故先例无害，本刀有解引用
    // 点〔pager 写 magic〕，故走「先泄漏后映射」正序）。
    let vmo: &'static mut Vmo =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmo::create_demand(PAGE_SIZE).expect("dpg: 建 demand VMO"),
        ));
    let asid = crate::aspace::alloc_asid();
    let vmar: &'static mut Vmar =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(
            Vmar::new_root(fa, asid).expect("dpg: 建 root Vmar"),
        ));
    vmar.map_demand(vmo, DEMAND_VA, 0, PAGE_SIZE, Perm::Rw)
        .expect("dpg: map_demand");
    vmar::register_demand_root(asid, core::ptr::from_mut(vmar))
        .expect("dpg: 登记 demand root（G4-2 Result 化——boot 面满表/冲突即响雷）");
    // 探针注册（boot DAIF 屏蔽窗——首个可能触 demand 访问的任务调度前）；
    // G4-2：EL0 臂第二槽（process demand 装载面由此臂服务）。
    hal::set_demand_fault_probe(probe);
    hal::set_demand_fault_probe_el0(probe_el0);
    // pager 先入队（faulter 首触时 pager 在席；双序竞速由 poll-first 兜底）。
    berry_sched::spawn(dpg_pager, 0, b"dpg_pager");
    berry_sched::spawn_as(dpg_faulter, 0, b"dpg_flt", vmar.ttbr0_pa(), vmar.asid());
}
