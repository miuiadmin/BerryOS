//! G4-1 demand paging 内核机制最小面（`G4按需调页实施设计草案.md` §4 刀序）。
//!
//! 闭环形状（**无 EL0 pager 服务、无 0x21**——机制面独立验收）：
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
//! 死锁防线：探针异常上下文 IRQ 天然屏蔽、block 前零锁在手（§2.3①）；
//! pager 供给全链 KL 窗内（SVC vmar_map 同构，§2.3②）；填叶 pt_lock 嵌
//! KL 正序、broadcast 只发不待（§2.3③）；pager 单线程首形（§2.3④）。

use crate::mm;
use crate::vmar::{self, Perm, Vmar};
use crate::vmo::Vmo;
use berry_hal_aarch64 as hal;
use berry_mm::va::PAGE_SIZE;
use core::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};

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
/// false = 非 demand 面（未登记 asid / FAR 未落 demand 区间），原杀链
/// 不变量保持。block_current 契约对位：IRQ 屏蔽窗 = 异常入口 PSTATE.I=1
/// 天然满足；条件已验 = 缺页指令本身；醒后重验 = eret 重试天然重验（供给
/// 失败则再 fault 再进探针——pager 侧 fail loud 兜底）。
unsafe extern "C" fn probe(far: usize) -> bool {
    let asid = hal::current_asid();
    let Some(root) = vmar::demand_root(asid) else {
        return false;
    };
    // SAFETY: 登记方（init / G4-2 process 组装面）保证对象常驻（登记只进
    // 不出，Boot-held 先形）；异常上下文单核无并发注销面。
    if unsafe { (*root).demand_region_at(far).is_none() } {
        return false;
    }
    // 命中：武装请求面（far/asid 先行、armed 收口 Release——pager 侧
    // Acquire 读 armed 后 far/asid 必可见）。
    REQ_FAR.store(far as u64, Ordering::Relaxed);
    REQ_ASID.store(asid, Ordering::Relaxed);
    REQ_ARMED.store(true, Ordering::Release);
    // 唤 pager（双序通吃：已睡 → 摘队；未睡 → 位/槽锁存 + pager poll-first
    // 追平——f4 F1 纪律同源）。异常上下文裸调（SQ 锁自取，与 IRQ 生产者
    // `wake_matching_locked` 同窗纪律）。
    berry_sched::wake_matching(&PAGER_KEY as *const u8 as usize, DPG_WAKE_BIT);
    // 睡 fault 等待面（无限等——无超时语义；唯一唤醒者 = pager 供给毕）。
    let r = berry_sched::block_current(
        &FAULT_KEY as *const u8 as usize,
        DPG_WAKE_BIT,
        u64::MAX,
    );
    assert!(
        matches!(r, berry_sched::WakeReason::Signaled),
        "dpg probe: 醒因非 Signaled（fault 键无超时/无他者——唤醒面破损）"
    );
    true
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
    vmar::register_demand_root(asid, core::ptr::from_mut(vmar));
    // 探针注册（boot DAIF 屏蔽窗——首个可能触 demand 访问的任务调度前）。
    hal::set_demand_fault_probe(probe);
    // pager 先入队（faulter 首触时 pager 在席；双序竞速由 poll-first 兜底）。
    berry_sched::spawn(dpg_pager, 0, b"dpg_pager");
    berry_sched::spawn_as(dpg_faulter, 0, b"dpg_flt", vmar.ttbr0_pa(), vmar.asid());
}
