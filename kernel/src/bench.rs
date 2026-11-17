//! 内核微基准 + churn 泄漏金丝雀（B-0/S-0；设计 = `docs/基准与压力测试设计-v1.0.md`）。
//!
//! 定位：**测量是优化的前提**——没有基线数字，任何「变快了/变慢了」都无法
//! 证明。本模块在 `ktest::run()` 之后、`timer::init`/`berry_sched::init` 之前
//! 运行：单线程、IRQ 屏蔽、分配器已活——零调度噪声，纯子系统成本。
//!
//! 三条产出线：
//! 1. **微基准**（`BENCH,<name>,<iters>,<cyc_per_op>` 机器可读行 × 8）——
//!    计时 = CNTVCT **原始周期数**。口径纪律（测试策略 §7）：TCG 下 CNTV
//!    推进 ≈ CNTFRQ/2，绝对值只做**同机同 QEMU 的相对比较**（基线对拍在
//!    宿主侧 `tools/bench.py`，宽松因子门禁）；内核只出数不做门禁。
//!    七项在 `run()` 的 boot 干净窗（ktest 后、timer/sched init 前）；
//!    switch_pair 在 `run_sched_epoch()` 的调度器纪元窗（sched init 后、
//!    telemetry::init 前——04 号审计基线缺口两项，2026-11-17）。
//! 2. **churn 泄漏金丝雀**（`BENCHCHURN ok frame=0 vmo=0 handle=0`）——
//!    确定性**严格断言**：反复创建/销毁后 `mm::allocated_pages()` 水位与
//!    handle 表在用槽数必须精确归位，泄漏 ≠ 0 → panic。这属于「主干永远
//!    绿」红线的一部分（每次 boot 都跑），不是可选项。
//! 3. **S-2 资源耗尽行为**（`BENCHXH ok frame=1 vmo=1 handle=1 chan=1`，
//!    设计 §12）——资源到顶时**错误码回、不 panic、可恢复**的确定性边界
//!    断言：ABI §8.0「永不 panic」承诺的耗尽版执法（全过才打，L0）。
//!
//! 收尾行 `BENCH ok n=8` 自 `run()` 尾迁至 `run_sched_epoch()` 纪元尾恒发
//!（04 号审计基线缺口刀，2026-11-17——须在最后一行 BENCH 之后：bench.py
//! 的 len==done_n 收割契约按本行截尾；门缺席 = 仅缺 switch_pair 行、
//! trailer 照发），是 smoke 裸相关卡标记（标记清单单一事实源 =
//! `tools/markers.sh`，bcts 冻结件随字面滚新归主线）。EL0 视角的 syscall/IPC
//! 路径基准（含 SVC 陷阱开销）留 B-1 用户态基准程序（rootfs 生态）——内核
//! 内测不了异常级往返。

// 与 ktest 同款豁免：基准/金丝雀路径的 expect 表达「不可能失败」——失败即
// panic = 主干红线（见模块文档），不是可恢复路径。
#![allow(clippy::result_unwrap_used, clippy::result_expect_used)]

use crate::channel::CHAN_DEPTH;
use crate::handle::{
    ERR_AGAIN, ERR_BUSY, ERR_NO_MEMORY, HandleTable, KernelObj, ObjType, RIGHT_SIGNAL,
};
use crate::ktest::{svc4, svc6};
use crate::mm;
use crate::syscall::{
    NR_CHANNEL_CREATE, NR_CHANNEL_RECV, NR_CHANNEL_SEND, NR_HANDLE_CLOSE, NR_VMO_CREATE,
    VMO_MAX_SIZE,
};
use crate::vmo::Vmo;
use berry_hal_aarch64::timer::cntvct;
use berry_hal_aarch64::{uart_put_dec, uart_puts};
use berry_mm::{AllocError, MAX_ORDER};
use core::sync::atomic::{AtomicU64, Ordering};

const PAGE: usize = 1 << 12;

/// 基准项计数（`BENCH ok n=` 的定值——增删基准项时同步改这里、smoke 两处
/// 标记表（markers.sh/smoke-image.sh）、bench.py 的 _iter_table 与表头注释、
/// bcts 冻结件滚新（计数面零增减的 bcts@N 滚新——主线收））。
const N_METRICS: usize = 8;

// ---- S-2（设计 §12）常量 ----

/// handle 面 fill 槽数（v1 表无 syscall 硬帽、真帽 = 堆——4096 轮盖
/// 「错误码回 + 可回收」语义，§12 定值）。
const XH_HANDLE_N: usize = 4096;

/// frame 面耗尽块收集槽容量：**定长数组、不用 Vec**——全帧耗尽的瞬态里
/// 「堆不可用」正是被测状态，收集路径自身必须零分配（§12 风险注记的落地
/// 形态）。上界 = 窗口期空闲区的 buddy 块切分总数：QEMU 直载路径空闲区
/// 1–2 段（每段每阶 ≤1 块）、EFI handoff 31 段图残余 ~200 块——512 = 2x
/// 余量；溢出即 panic（L0 执法，届时按 §12 降级注记改「order-13 大块耗尽」
/// 形态）。
const XH_MAX_BLOCKS: usize = 512;

/// chan 面载荷长度（8B 最小有意义载荷；recv 容量/字节数回写断言同值）。
const XH_MSG: u64 = 8;

// ---- S-2 静态缓冲（boot 窗口单线程 IRQ 屏蔽；只经 raw 指针触碰——
// static mut 禁直接取引用，与 ktest `&raw mut` / 本模块 SRC 同纪律）----

// channel_create 双端出参 / vmo_create 出参（出参宽 4B——写 u64 槽低半）。
static mut XH_C0: u64 = 0;
static mut XH_C1: u64 = 0;
static mut XH_VOUT: u64 = 0;
// chan 面收发缓冲 + recv 容量槽（in-out：进 = 容量，出 = 实际值）。
static mut XH_SBUF: [u8; 8] = [0x58; 8];
static mut XH_RBUF: [u8; 8] = [0; 8];
static mut XH_NB: u64 = 0;
static mut XH_NH: u64 = 0;
static mut XH_HND: [u32; 4] = [0; 4];
// handle 面 4096 槽值收集（静态化——16KiB 不吃主栈）。
static mut XH_HS: [u32; XH_HANDLE_N] = [0; XH_HANDLE_N];

pub fn run() {
    bench_cntvct_pair();
    bench_frame_alloc4k();
    bench_frame_alloc64k();
    bench_vmo_commit1p();
    bench_vmo_rw4k();
    bench_handle_evt();
    // pick_scan（04 号审计基线缺口，2026-11-17）：Fair 出选面纯函数成本——
    // 与上六项同窗（ktest 后、timer/sched init 前干净窗）。
    bench_pick_scan();
    // AM 原子观测对（非标记行）：紧随七项官方基准后、churn 前——七行
    // `BENCH,` + BENCHCHURN/BENCHXH 的既有 boot 形状零扰动（`BENCH ok`
    // 收尾行已迁 run_sched_epoch 纪元尾恒发——本窗不再打）。
    bench_am_pair();
    churn_frame();
    churn_vmo();
    churn_handle();
    // 收尾关卡行：三项 churn 金丝雀全部严格断言过了才走到这里（delta 恒 0，
    // 非 0 即已在上面 panic——本行是状态谓词，非竞速面）。
    uart_puts(b"BENCHCHURN ok frame=0 vmo=0 handle=0\n");
    // S-2 六面耗尽/恢复（boot 输出序：BENCHCHURN 行后——设计 §12；全过才打
    // BENCHXH，任一断言破 = panic = smoke FAIL）。BENCH ok 收尾行自此不在
    // 本窗（迁 run_sched_epoch——调度器纪元尾恒发，见模块头注）。
    xh_run();
}

/// 计时循环骨架：`iters` 次 `f`，打一行 `BENCH,<name>,<iters>,<cyc_per_op>`。
/// 整除取每操作周期数（不做小数——相对比较用，精度足够）。
fn bench<F: FnMut()>(name: &[u8], iters: u32, mut f: F) {
    let t0 = cntvct();
    for _ in 0..iters {
        f();
    }
    let t1 = cntvct();
    let per = (t1 - t0) / iters as u64;
    uart_puts(b"BENCH,");
    uart_puts(name);
    uart_puts(b",");
    uart_put_dec(iters as u64);
    uart_puts(b",");
    uart_put_dec(per);
    uart_puts(b"\n");
}

/// 测量基线：一对 cntvct 读 + 循环骨架自身开销——其余基准项与它相减
/// 才是子系统净成本（宿主侧对拍时同表呈现，人工心算口径）。
fn bench_cntvct_pair() {
    bench(b"cntvct_pair", 4096, || {
        let _ = cntvct();
    });
}

/// 单页帧分配 + 释放往返（buddy 最小阶全链）。
fn bench_frame_alloc4k() {
    bench(b"frame_alloc4k", 1024, || {
        let pfn = mm::frames().alloc().expect("bench: frame alloc");
        mm::frames().free(pfn, 0).expect("bench: frame free");
    });
}

/// 64KiB 连续块（order 4 = 16 页）分配 + 释放往返——拆块/合并路径。
fn bench_frame_alloc64k() {
    bench(b"frame_alloc64k", 256, || {
        let pfn = mm::frames()
            .alloc_order(4)
            .expect("bench: frame alloc_order");
        mm::frames().free(pfn, 4).expect("bench: frame free");
    });
}

/// 单页 VMO 全生命周期：create + commit（含 4KiB 清零）+ drop（还帧）。
fn bench_vmo_commit1p() {
    bench(b"vmo_commit1p", 256, || {
        let mut vmo = Vmo::create(PAGE).expect("bench: vmo create");
        vmo.commit(mm::frames(), 0, PAGE).expect("bench: vmo commit");
        drop(vmo);
    });
}

/// 已提交单页 VMO 的 4KiB write + 4KiB read 往返（内核侧拷贝路径——
/// syscall 面 vmo_read/vmo_write 拷贝成本的下界）。源/目的是静态缓冲，
/// 读回首尾断言防死存储消除。
fn bench_vmo_rw4k() {
    static mut SRC: [u8; PAGE] = [0xA5; PAGE];
    static mut DST: [u8; PAGE] = [0; PAGE];
    // SAFETY: bench 期单线程、IRQ 屏蔽；静态缓冲只在本函数经 raw 指针触碰
    //（static mut 禁直接取引用——与 ktest `&raw mut` 同纪律）。
    let src: &[u8] = unsafe { &*core::ptr::addr_of!(SRC) };
    let dst: &mut [u8] = unsafe { &mut *core::ptr::addr_of_mut!(DST) };
    let mut vmo = Vmo::create(PAGE).expect("bench: vmo create");
    vmo.commit(mm::frames(), 0, PAGE).expect("bench: vmo commit");
    bench(b"vmo_rw4k", 128, || {
        vmo.write(0, src).expect("bench: vmo write");
        vmo.read(0, dst).expect("bench: vmo read");
    });
    // 读回形状断言：SRC 全 0xA5（含首尾），读回应逐字节相同。
    assert_eq!(dst[0], 0xA5, "bench: vmo_rw4k readback head");
    assert_eq!(dst[PAGE - 1], 0xA5, "bench: vmo_rw4k readback tail");
}

/// Event 对象经 handle 表全生命周期：new + insert + get（查表热路径）+
/// close（drop Box）——每次 syscall 都要过的查表门在 get 里。
fn bench_handle_evt() {
    let mut ht = HandleTable::new();
    bench(b"handle_evt", 1024, || {
        let h = ht
            .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
            .expect("bench: insert");
        let _ = ht.get(h, ObjType::Event, 0).expect("bench: get");
        ht.close(h).expect("bench: close");
    });
    assert_eq!(ht.used_count(), 0, "bench: handle_evt 后表须空");
}

/// **AM 原子观测对**（AM 热路径首测——`dpg_stall_note` 每停顿 3× fetch_add
/// （telemetry.rs）+ runqueue `sum_vruntime` link/unlink 各 1× fetch_add/
/// fetch_sub（SMP 多写者，2026-11-11 丢更新修复改形）+ `frozen_owner` send
/// 热路径一次判空分支（AM-1.5）——设计承诺「非压力态零额外成本」的测量
/// 底座）。非压力态 = 本模块窗口（单线程、IRQ 屏蔽、独占监视器空闲）——
/// RMW 无重试无一致性流量，测的正是承诺所对应的态。
///
/// 口径 = **逐 op 三明治**：`t0=cntvct → body → t1=cntvct`，ΣΔ 再整除
/// （原始差值累计后一次整除——CNTVCT 粒度噪声在 8192 样本上摊薄）。
/// `fetch_add` 行 − `cntvct_only` 行 = 计时对自身成本之外的 RMW 净开销；
/// frozen_owner 判空 = 单条本位分支（亚周期级），以差值地板覆盖不另设行。
fn bench_am_pair() {
    // 局部安全原子（非 static mut——零 unsafe 台账面；独占缓存行无从谈起
    // 独占，但本窗口无第二写者 = 无假共享）。
    static FA: AtomicU64 = AtomicU64::new(0);
    bench_fa(b"cntvct_only", 8192, || {});
    bench_fa(b"fetch_add", 8192, || {
        FA.fetch_add(1, Ordering::Relaxed);
    });
    // 放大臂：单次 RMW < 1 cyc 时 1× 对被「Σacc/iters 整除」地板截断（首跑
    // 实测两行同为 4/op）——64 连发夹一次计时对，宿主侧以第 5 字段原始 ΣΔ
    // 全精度差分（fetch_add64.acc − 按计时对成本折算的基线）后 /(64×iters)
    // = 亚周期分辨率的单次成本。
    bench_fa(b"fetch_add64", 4096, || {
        let mut i = 0;
        while i < 64 {
            FA.fetch_add(1, Ordering::Relaxed);
            i += 1;
        }
    });
    // RMW 真执行旁证（fetch_add 本不可消除——观察锚，防未来 codegen 变化）。
    assert_eq!(
        FA.load(Ordering::Relaxed),
        8192 + 64 * 4096,
        "bench: fetch_add 计数"
    );
}

/// AM 观测行打印骨架：打 `BENCHFA,<name>,<iters>,<cyc_per_op>,<acc>`（第 5
/// 字段 = 原始 ΣΔ 周期数——宿主侧全精度差分用，整除地板只在第 4 字段）。
/// **非标记观测行**——`BENCHFA,` 前缀与 bench.py 收割面词根隔离（`BENCH_RE`
/// 行锚定 `^BENCH,` 四字段、BENCH1/BENCHRT/BENCHLAT/BENCHWAKE 各自字面均
/// 不命中）⇒ 六行对账与 `n=6` 契约零扰动；字面已对 canon 全表
/// （tools/markers.sh 三数组 + tools/smoke-image.sh KERNEL_MARKERS）grep -F
/// 互查——无任何在册标记串是本行子串。不进 N_METRICS/smoke 标记表 = 观测
/// 面非门禁面（BENCHWAKE 观测行同位先例）；TCG 绝对值只作同机相对比较
/// （模块头口径纪律）。
fn bench_fa<F: FnMut()>(name: &[u8], iters: u32, mut f: F) {
    let mut acc: u64 = 0;
    for _ in 0..iters {
        let t0 = cntvct();
        f();
        let t1 = cntvct();
        acc = acc.wrapping_add(t1.wrapping_sub(t0));
    }
    let per = acc / iters as u64;
    uart_puts(b"BENCHFA,");
    uart_puts(name);
    uart_puts(b",");
    uart_put_dec(iters as u64);
    uart_puts(b",");
    uart_put_dec(per);
    uart_puts(b",");
    uart_put_dec(acc);
    uart_puts(b"\n");
}

// =========================================================================================
// 04 号审计基线缺口（2026-11-17）：pick_scan / switch_pair 两项补面
// =========================================================================================

/// **pick_scan**（04 号审计基线缺口，2026-11-17）：`Fair::pick_next_filtered`
/// 单圈合并扫描的纯函数成本——本地 `Runqueue` + 20 枚合成 unit 带任务直接
/// 出选（不真切上下文、不真跑任务体、不占生产环）。环长 20 = 审计附记二
/// 常态值（128 是 `RING_WALK_CAP` 断言帽非生产形；混合带〔FairBand 分派〕
/// 变体挂账另刀）。setup 同 ktest POLICY 族先例：`Box::leak` + `Fair::
/// enqueue` 正路入环 + vr=i 铺展（vd 由 enqueue 授）+ `resync_vruntime_sum`
/// 直改后洗账（S2b+ 账本调和纪律）；一枚置 Running 作 `my_cur`（is_cur
/// 参选分支——生产 pick 调用形，ktest Y1 同款）。泄漏面：20×(TCB+16KiB
/// 栈) 有界不还（ktest POLICY 同款先例——bench 窗无回收点；churn 金丝雀
/// 的水位基线全在本函数之后取，不受染）。防死存储消除：累计出选指针
/// （bench_am_pair 断言先例），尾断言非零。
fn bench_pick_scan() {
    use berry_sched::{Fair, Runqueue, SchedClass, Task, TaskState, Topo};

    /// 占位入口（pick 只选不切——永不执行；halt 兜底防未定义行为）。
    extern "C" fn ps_dummy(_arg: usize) -> ! {
        berry_hal_aarch64::halt()
    }

    const RING_N: usize = 20;
    let mut rq = Runqueue::new();
    let rr = Fair;
    let topo = Topo::single();
    let mut my_cur: *mut Task = core::ptr::null_mut();
    for i in 0..RING_N as u64 {
        let mut t = Task::new_kernel_with_as(ps_dummy, i as usize, b"ps".as_slice(), 0, 0);
        // vr=i 铺展（enqueue 会把落后 avg 者钳到 avg——首段 ineligible 尾段 eligible 两态覆盖）；
        t.vdeadline = i + 1000; // 注：fair 任务 enqueue 授自身 vd（此值被覆写——审查轮勘正注，探针确定性不受影响）
        let t: &'static mut Task = alloc::boxed::Box::leak(t);
        rr.enqueue(t, &mut rq);
        if i == 0 {
            my_cur = t as *mut Task;
            t.state = TaskState::Running; // my_cur 参选形（生产 pick 调用先例 ktest Y1）
        }
    }
    rq.resync_vruntime_sum(); // 直改 vruntime 后洗账
    let mut acc: usize = 0;
    bench(b"pick_scan", 1024, || {
        // SAFETY: boot 窗单线程 + IRQ 屏蔽 = pick_inner 调用契约（KERN_LOCK 窗
        // 的串行等价）——ktest 直调面同判据；my_cur 在环（Running 参选形）。
        // 每迭代 = 一次真实出选（advance_past 游标推进 + policy 空表零写 +
        // stats 记账——与生产面同路径）。
        let p = rr
            .pick_next_filtered(&rq, &topo, my_cur)
            .expect("bench: pick_scan 出选");
        acc = acc.wrapping_add(p as *mut Task as usize);
    });
    assert_ne!(acc, 0, "bench: pick_scan 出选指针累计（防死存储消除）");
}

// ---- switch_pair：调度器纪元基准窗（run_sched_epoch）----

/// sbench 门控键（fw_cfg 具名项——pilat 同款「值面不解析：键在即全配置」；
/// bench.py 的 icount 族 qemu_cmd 恒递此键 = 基线面恒全八行；trace-dual
/// 两臂红线禁递〔TRACE_CAP 证书面 = 前调度器纪元项——见 bench.py 常量注〕；
/// smoke/生产 boot 缺席 = 门缺席仅缺 switch_pair 行，trailer 恒 n=8）。
const SBENCH_PROBE_NAME: &str = "opt/berry/sbench";

/// switch_pair 唤醒位（USER_SIGNAL_BITS 登记位 1<<53——裸键族共用一位先例
/// = dpg 1<<51；不经 Event 对象，登记 = 防撞/去重面）。
const SB_BIT: u64 = 1 << 53;

/// 测量者 A 的唤醒键（零大小静态地址裸键——dpg.rs FAULT_KEY/PAGER_KEY
/// 先例；只等用户位 → donor_edge 派生位门前置返 None，永不解引用）。
static SB_KEY_A: u8 = 0;
/// 伙伴 B 的唤醒键（同上——两键互异 = wake_matching 键门精确对传）。
static SB_KEY_B: u8 = 0;

/// 测量者 A（prio 8——先被出选）：`bench(b"switch_pair", N, …)` 体每迭代
/// = 唤醒对端 + 自阻塞 = **一完整任务对切换环**（A→B 与 B→A 各一次真切换
/// ——4-3b 起唤醒方不切换，切换全部发生在 block_current 的出选切点）。
/// wake/block 返回值不检（测量体最小形）：握手破 = 双方互睡挂死 = boot
/// 帽红（fail-loud by timeout，无静默错数面）。测毕收尾：唤醒 B（其第 N
/// 轮 block 醒来出环自灭）→ A `exit_current` 自灭——B 的 exit 的
/// pick-None 臂切回 IDLE（boot 上下文）续行。
extern "C" fn sb_task_a(_arg: usize) -> ! {
    bench(b"switch_pair", 128, || {
        let _ = berry_sched::wake_matching(&SB_KEY_B as *const u8 as usize, SB_BIT);
        let _ = berry_sched::block_current(&SB_KEY_A as *const u8 as usize, SB_BIT, u64::MAX);
    });
    let _ = berry_sched::wake_matching(&SB_KEY_B as *const u8 as usize, SB_BIT);
    berry_sched::exit_current()
}

/// 伙伴 B（prio 0）：对称循环——A 首轮 wake 落空（B 尚未入睡，O(1) 空桶
/// 早退无害；B 由 A 的自阻塞出选上场），第 2 轮起每 wake 恰唤醒 1。
extern "C" fn sb_task_b(_arg: usize) -> ! {
    for _ in 0..128 {
        let _ = berry_sched::wake_matching(&SB_KEY_A as *const u8 as usize, SB_BIT);
        let _ = berry_sched::block_current(&SB_KEY_B as *const u8 as usize, SB_BIT, u64::MAX);
    }
    berry_sched::exit_current()
}

/// **调度器纪元基准窗**（04 号审计基线缺口，2026-11-17）：switch_pair——
/// berry_sched::init 与 telemetry::init 之间的唯一槽位（调度器已在 + 遥测
/// 钩子未注册 = TRACE_CAP 0.0 结构性零面 + boot DAIF 屏蔽窗）。收尾行
/// `BENCH ok n=8` 自 `run()` 尾迁此恒发（须在最后一行 BENCH 之后——
/// bench.py 的 len==done_n 收割契约按本行截尾）。
///
/// **boot 手递首切**：spawn A/B 后 `yield_now_keep_irq` 让出一次（idle_loop
/// 「有活」支从 IDLE 让出的同款舞蹈；**keep_irq 形** = 出口保持 IRQ 屏蔽
/// ——本窗仍在 boot DAIF 屏蔽段，plain `yield_now` 的出口解蔽会打破后续
/// spawn 块的屏蔽态前置）。A/B 测毕 exit_current 自灭，环空后 exit 的
/// pick-None 臂切回 IDLE_TASK.ctx = boot 在 yield 行下续行（顺带 reap_drain
/// 回收 A/B 的 Task Box 与 16KiB 栈——零泄漏面）。
///
/// **迭代数 128**：避开 100 倍数（bump_switches 金丝雀打印不落窗——现行
/// SOAK_PRINT_EVERY=10000，纪元总切换 ≈ 2N+O(1) ≈ 259 ≪ 10000，结构性
/// 无行；非 100 倍数为旧 100 档时代的保守带）。
///
/// **口径陷阱记名**：内核任务对是切换热路径的**下界形**——无 SVC 往返
/// 〔归 B-1〕、无 TTBR0/ASID churn〔A/B 均无地址空间，activate_if_changed
/// 恒空转；真实用户任务对的页表切换不在本行口径内〕、FPSIMD 全量照付
/// 〔GUI-0 起切换无条件保存恢复 q0..q31+FPCR/FPSR，含在数内〕——防「EL0
/// ping-pong 收益 = 此行收益」过度外推。
///
/// **锁纪律**：全程 boot DAIF 屏蔽（A/B 继承屏蔽态运行、从不解蔽），故
/// 任务体不带 klock 包络（lat 先例的 klock_exit 会解蔽 = 破窗）；block/
/// wake 的 KERN_LOCK 位=0 直调面（幂等跳过——exit_current 直调同款）。
///
/// **配对悬账（主线收）**：sched `note_switch` 的 SWITCHES_TT 在钩子缺席
/// 期也计（「实际不可达」防御支——本纪元使其可达），G1.5 的
/// `tel >= switches_tt` 断言将差本纪元切换数（≈259）；须随本刀在 sched
/// 侧配对修复（tt 计数移入钩子在位分支，或 telemetry::init 注册时重基）。
pub fn run_sched_epoch() {
    // noktest 生产形态整窗跳（P0-4 合并一门同款：ktest/bench 相互独立但
    // 生产 boot 同跳——零 BENCH 行零 trailer，与旧 noktest 形状逐位一致）。
    if berry_hal_aarch64::fwcfg::find_file(crate::NOKTEST_PROBE_NAME).is_some() {
        return;
    }
    if berry_hal_aarch64::fwcfg::find_file(SBENCH_PROBE_NAME).is_some() {
        // 先 B 后 A：A prio 8 恒先出选（次序无关紧要，记 spawn 序备查）。
        berry_sched::spawn_with(sb_task_b, 0, b"sb_b", 0);
        berry_sched::spawn_with(sb_task_a, 0, b"sb_a", 8);
        // 手递首切（见函数头注——keep_irq 形；A/B 自灭后环空回 boot 续行）。
        berry_sched::yield_now_keep_irq();
    }
    // 收尾关卡行恒发（门缺席 = 仅缺 switch_pair 行；严格后于最后一行 BENCH
    // ——switch_pair 行由 A 在纪元内打毕、本行在其后）。
    uart_puts(b"BENCH ok n=");
    uart_put_dec(N_METRICS as u64);
    uart_puts(b"\n");
}

/// **churn 金丝雀①帧**：混合阶（0/2/4）交错分配释放 64 轮，水位必须精确
/// 归位——buddy 合并路径任何丢失/双释都会在此现形（确定性断言，非统计）。
fn churn_frame() {
    let base = mm::frames().allocated_pages();
    for _ in 0..64 {
        let mut held: [(usize, usize); 11] = [(0, 0); 11];
        let mut n = 0;
        // 混合 8×单页 + 2×4 页块 + 1×16 页块；分配序 ≠ 释放序（洗牌效应）。
        for _ in 0..8 {
            held[n] = (mm::frames().alloc().expect("churn: alloc"), 0);
            n += 1;
        }
        for _ in 0..2 {
            held[n] = (mm::frames().alloc_order(2).expect("churn: alloc2"), 2);
            n += 1;
        }
        held[n] = (mm::frames().alloc_order(4).expect("churn: alloc4"), 4);
        // 逆序释放：与分配序相反的固定序（合并路径走足）。
        for i in (0..=n).rev() {
            let (pfn, o) = held[i];
            mm::frames().free(pfn, o).expect("churn: free");
        }
        // 逐轮对账：哪一轮泄漏当场指认（弱断言会掩盖逐轮漂移）。
        assert_eq!(
            mm::frames().allocated_pages(),
            base,
            "churn: frame watermark drifted (round leak)"
        );
    }
}

/// **churn 金丝雀②VMO**：64KiB VMO（16 页）create + 全量 commit + 首页写 +
/// drop，64 轮——页数最多的归还路径（Vmo::Drop 还帧链）反复走。
fn churn_vmo() {
    static mut BUF: [u8; PAGE] = [0x77; PAGE];
    // SAFETY: 同 bench_vmo_rw4k——raw 指针取用，单线程窗口。
    let buf: &[u8] = unsafe { &*core::ptr::addr_of!(BUF) };
    let base = mm::frames().allocated_pages();
    for _ in 0..64 {
        let mut vmo = Vmo::create(16 * PAGE).expect("churn: vmo create");
        vmo.commit(mm::frames(), 0, 16 * PAGE)
            .expect("churn: vmo commit");
        vmo.write(0, buf).expect("churn: vmo write");
        drop(vmo);
        assert_eq!(
            mm::frames().allocated_pages(),
            base,
            "churn: vmo watermark drifted (Vmo::Drop leak)"
        );
    }
}

/// **churn 金丝雀③handle**：私有表 32 槽 insert/close × 64 轮——表须精确
/// 清空（used_count == 0），Event/Box 分配释放配平。帧水位作旁证——Event
/// 不持帧，恒等；**先预热一轮再取基线**（首轮 slots Vec 懒增长会合法地向
/// 堆要一页——首跑实抓 +1 页假阳性；预热后 Vec 容量已定，漂移即真泄漏）。
fn churn_handle() {
    let mut ht = HandleTable::new();
    // 预热：一次满 32 槽 insert/close——把 Vec 增长/堆 bin 预热全部吃完。
    let mut warm = [0u32; 32];
    for h in warm.iter_mut() {
        *h = ht
            .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
            .expect("churn: warm insert");
    }
    for &h in warm.iter() {
        ht.close(h).expect("churn: warm close");
    }
    assert_eq!(ht.used_count(), 0, "churn: 预热轮后表须空");
    let base = mm::frames().allocated_pages();
    for _ in 0..64 {
        let mut hs = [0u32; 32];
        for h in hs.iter_mut() {
            *h = ht
                .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
                .expect("churn: insert");
        }
        assert_eq!(ht.used_count(), 32, "churn: 32 槽全在用");
        for &h in hs.iter() {
            ht.close(h).expect("churn: close");
        }
        assert_eq!(ht.used_count(), 0, "churn: 全关光后表须空");
    }
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "churn: handle path frame watermark drifted"
    );
}

// =========================================================================================
// S-2：资源耗尽行为（设计 `docs/基准与压力测试设计-v1.0.md` §12）
// =========================================================================================

/// **S-2 资源耗尽行为**：六面确定性严格断言——资源到顶时**错误码回、不
/// panic、可恢复**（ABI §8.0「永不 panic」承诺的耗尽边界执法，进 smoke 裸相
/// L0）。公约 6：boot 窗口单线程 IRQ 屏蔽 = 全帧耗尽瞬态无并发分配者，安全。
///
/// **复位对账结构**（S-0 预热立法的 S-2 版）：窗口内三个**永久性**帧效应全部
/// 预付在全局基线之前——①通道对端点队列（`ChannelEnd::pair` 互 retain →
/// close 不 drop，队列 Vec ≈2.7KB/端 = 1 页/端永不还——channel.rs 环泄漏
/// 安全向政策）；②handle 面 4096 槽的 `ObjRef` Box slab 页（G0 政策：slab
/// 页永不还 buddy——预热满轮把池扩到位，真轮纯复用）；③BOOTSTRAP_HT 槽位
/// Vec 残余增长（ktest 期已基本定型，随①落定）。基线后六面只做
/// 「吃光 → 全量归还」的自愈往返，末尾 `allocated_pages() == base` 恒等。
fn xh_run() {
    // ---- 预热①：SVC 造通道对（EL1 tag==0 → BOOTSTRAP_HT，ktest H4 同路）。
    // 端点队列 ×2 页 + Box ×2（slab）落定；chan 两面复用这对端点。
    // SAFETY: 静态出参槽经 raw 指针传入（4B 写入 u64 槽低半，高半保持 0）。
    unsafe { XH_C0 = 0; XH_C1 = 0; }
    assert_eq!(
        svc4(
            NR_CHANNEL_CREATE as u64,
            0,
            &raw mut XH_C0 as u64,
            &raw mut XH_C1 as u64,
        ),
        0,
        "bench: xh 预热 channel_create"
    );
    let (hc0, hc1) = (unsafe { XH_C0 } as u32, unsafe { XH_C1 } as u32);
    assert_ne!(hc0, 0, "bench: xh 预热端点 0 须非零");
    assert_ne!(hc1, 0, "bench: xh 预热端点 1 须非零");
    assert_ne!(hc0, hc1, "bench: xh 预热两端须互异");

    // ---- 预热②：handle 满表一轮——slab 池扩容 + slots Vec 懒增长全预付
    //（churn_handle 预热先例的一般化）。表**存活到函数尾**：其 slots Vec 的
    // 大块（>2048B 走 buddy 直径）在对账后才随 drop 归还——早还会把水位打到
    // 基线之下、恒等断言反手误伤。
    let mut ht = HandleTable::new();
    // SAFETY: 单线程窗口；静态槽值数组只经 raw 指针触碰。
    let hs: &mut [u32] = unsafe { &mut *core::ptr::addr_of_mut!(XH_HS) };
    for h in hs.iter_mut() {
        *h = ht
            .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
            .expect("bench: xh 预热 insert");
    }
    assert_eq!(ht.used_count(), XH_HANDLE_N, "bench: xh 预热满表");
    for &h in hs.iter() {
        ht.close(h).expect("bench: xh 预热 close");
    }
    assert_eq!(ht.used_count(), 0, "bench: xh 预热清空");

    // ---- 全局基线（三个永久效应已全部入账）----
    let base = mm::frames().allocated_pages();

    // ---- 六面（设计表序）----
    let f_frame = xh_frame_exhaust(base);
    let f_vmo = xh_vmo_cap(base);
    let f_handle = xh_handle_fill(base, &mut ht, hs);
    let f_chan = xh_chan_full(base, hc0, hc1) && xh_chan_drain(hc1);

    // ---- 面⑥复位对账：全部面跑完水位恒等（S-0 同口径金丝雀）----
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh 复位对账（六面后水位须恒等）"
    );
    // 通道对两端 close（环对象互钉不 drop——零帧变化；ktest H4 清场同款）。
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, hc0 as u64, 0, 0),
        0,
        "bench: xh close end0"
    );
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, hc1 as u64, 0, 0),
        0,
        "bench: xh close end1"
    );

    // 状态谓词收尾行：六面全过才走到这里（任一断言破已在上面 panic）。
    uart_puts(b"BENCHXH ok frame=");
    uart_put_dec(f_frame as u64);
    uart_puts(b" vmo=");
    uart_put_dec(f_vmo as u64);
    uart_puts(b" handle=");
    uart_put_dec(f_handle as u64);
    uart_puts(b" chan=");
    uart_put_dec(f_chan as u64);
    uart_puts(b"\n");
    // ht（slots Vec 大块随 drop 归还 buddy）在此函数尾自然失效——对账已毕。
}

/// 面①frame_exhaust：高阶到低阶把 buddy 空闲块**全部吃光** → 再分配必
/// `Err(OutOfMemory)`（「Err 非 panic」即被测行为）→ 逆序全量释放 → 水位
/// 精确归位。收集 = 定长数组（见 `XH_MAX_BLOCKS`——耗尽期间路径零分配，
/// §12 风险注记的执法形态）。
fn xh_frame_exhaust(base: usize) -> bool {
    // 8KiB 定长收集槽（主栈 256KiB——boot 窗口零压力）。
    let mut held: [(usize, u8); XH_MAX_BLOCKS] = [(0, 0); XH_MAX_BLOCKS];
    let mut n: usize = 0;
    let mut pages: usize = 0;
    // 高阶先吃：空闲区按 2^order 整数倍逐级分解，高阶先行让收集块数最少
    //（低阶零头在高位耗尽后由本阶兜住）。
    for order in (0..=MAX_ORDER).rev() {
        while let Ok(pfn) = mm::frames().alloc_order(order) {
            assert!(n < XH_MAX_BLOCKS, "bench: xh 收集槽溢出（§12 降级注记）");
            held[n] = (pfn, order as u8);
            n += 1;
            pages += 1 << order;
        }
    }
    // 全阶皆空：最小阶（最不易整块的粒度）再分配必 OutOfMemory。
    assert!(n > 0, "bench: xh 耗尽轮零分配（窗口内必有空闲——不可能）");
    assert_eq!(
        mm::frames().alloc(),
        Err(AllocError::OutOfMemory),
        "bench: xh 全阶耗尽后 alloc 须 Err(OutOfMemory)"
    );
    // 耗尽中对账：水位 = 基线 + 吃掉的页数（块头 2^order 口径、TAIL 不计——
    // 与 S-0 对账同源；任何阶的错账在此现形）。
    assert_eq!(
        mm::frames().allocated_pages(),
        base + pages,
        "bench: xh 耗尽中对账"
    );
    // 逆序全量释放：与分配序相反（buddy 合并路径走足——相邻块归并回高阶）。
    for i in (0..n).rev() {
        let (pfn, o) = held[i];
        mm::frames().free(pfn, o as usize).expect("bench: xh free");
    }
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh 释放后水位须精确归位"
    );
    // 量级下限：两路（QEMU 直载 / EFI handoff）窗口内空闲都 ≫ 16MiB——
    // 防「耗尽几个零散小块也算过」的空心绿。
    assert!(pages >= 4096, "bench: xh 耗尽页数下限（4k 页 = 16MiB）");
    true
}

/// 面②vmo_cap：尺寸帽两侧——超帽 `VMO_MAX_SIZE+1` 须 `-ENOMEM`（C-2c 在册
/// 帽，帽检先于出参/入表 = 零副作用）；边界恰 `VMO_MAX_SIZE` 须成（不
/// commit 不吃帧——页记账 Vec 走堆不走帧分配器）。syscall 面（真实 ABI 帽
/// 执法，EL1 → BOOTSTRAP_HT）+ 对象层直调（对象层无帽——结构性行为）双验。
fn xh_vmo_cap(base: usize) -> bool {
    // 超帽（syscall 面）：帽检在 copy_out/入表之前（sys_vmo_create 序）——
    // 错误码精确且零副作用。
    // SAFETY: 出参槽经 raw 指针传入（本调用不写——帽拒在写入之前）。
    assert_eq!(
        svc6(
            NR_VMO_CREATE as u64,
            (VMO_MAX_SIZE + 1) as u64,
            0,
            &raw mut XH_VOUT as u64,
            0,
            0,
            0,
        ),
        ERR_NO_MEMORY as i64,
        "bench: xh vmo 超帽须 -ENOMEM"
    );
    // 边界（syscall 面）：恰 VMO_MAX_SIZE（A1-2 起 128MiB）成 → close 即净（记账 Vec 经堆归还）。
    // SAFETY: 出参槽 4B 写入 u64 槽低半。
    unsafe { XH_VOUT = 0; }
    assert_eq!(
        svc6(
            NR_VMO_CREATE as u64,
            VMO_MAX_SIZE as u64,
            0,
            &raw mut XH_VOUT as u64,
            0,
            0,
            0,
        ),
        0,
        "bench: xh vmo 帽边界 create 须成"
    );
    let hv = unsafe { XH_VOUT } as u32;
    assert_ne!(hv, 0, "bench: xh vmo 帽边界 handle 须非零");
    assert_eq!(
        svc4(NR_HANDLE_CLOSE as u64, hv as u64, 0, 0),
        0,
        "bench: xh vmo close"
    );
    // 边界（对象层直调）：对象层无帽——32MiB 结构性成功；drop 即净。
    let vmo = Vmo::create(VMO_MAX_SIZE).expect("bench: xh vmo 对象层边界 create");
    assert_eq!(vmo.size(), VMO_MAX_SIZE, "bench: xh vmo 尺寸");
    drop(vmo);
    // 恒等：两面全程不 commit——帧水位不动（页记账在堆，与帧分配器无关）。
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh vmo 面水位恒等"
    );
    true
}

/// 面③handle_fill：私有表 4096 槽 insert → 全 close——`used_count`
/// 0↔4096 精确翻转 + 帧水位恒等（v1 表无 syscall 硬帽、真帽 = 堆；4096 轮
/// 盖「错误码回 + 可回收」语义，§12 定值）。复用预热表（容量已定型——真轮
/// 零增长零扩池）。
fn xh_handle_fill(base: usize, ht: &mut HandleTable, hs: &mut [u32]) -> bool {
    assert_eq!(hs.len(), XH_HANDLE_N, "bench: xh 槽值数组长度");
    for h in hs.iter_mut() {
        *h = ht
            .insert(KernelObj::Event(crate::event::Event::new()), RIGHT_SIGNAL)
            .expect("bench: xh insert");
    }
    assert_eq!(ht.used_count(), XH_HANDLE_N, "bench: xh 4096 槽全在用");
    for &h in hs.iter() {
        ht.close(h).expect("bench: xh close");
    }
    assert_eq!(ht.used_count(), 0, "bench: xh 全关光后表须空");
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh handle 面水位恒等"
    );
    true
}

/// 面④chan_full：自持双端 send×`CHAN_DEPTH` 全成 → 第 9 条 `-EBUSY`（非
/// 阻塞满即返）→ recv 排空（载荷/字节数逐条对账）→ **再 send 成**（恢复
/// 面：排空后端点可再用）→ 收回该条留空队给面⑤。send/recv 全走 SVC 真路径
///（错误码经 ABI 派发面回，非直调内核函数）。
fn xh_chan_full(base: usize, hc0: u32, hc1: u32) -> bool {
    // 填队：CHAN_DEPTH 连发全成。
    // SAFETY: 发送载荷静态缓冲经 raw 指针传入（EL1 拷贝面只读）。
    for _ in 0..CHAN_DEPTH {
        assert_eq!(
            svc6(
                NR_CHANNEL_SEND as u64,
                hc0 as u64,
                core::ptr::addr_of!(XH_SBUF) as u64,
                XH_MSG,
                0,
                0,
                0,
            ),
            0,
            "bench: xh 填队 send"
        );
    }
    // 第 CHAN_DEPTH+1 条 → -EBUSY（错误码精确——在册 CHAN_DEPTH=8 满点）。
    // SAFETY: 同上。
    assert_eq!(
        svc6(
            NR_CHANNEL_SEND as u64,
            hc0 as u64,
            core::ptr::addr_of!(XH_SBUF) as u64,
            XH_MSG,
            0,
            0,
            0,
        ),
        ERR_BUSY as i64,
        "bench: xh 满队 send 须 -EBUSY"
    );
    // 排水：CHAN_DEPTH 连收全成（每条容量槽进容量、回写实际值）。
    for _ in 0..CHAN_DEPTH {
        // SAFETY: 容量槽 in-out（进 = 容量 8B/4 槽，出 = 实际 8B/0 条）。
        unsafe {
            XH_NB = XH_MSG;
            XH_NH = 4;
        }
        assert_eq!(
            svc6(
                NR_CHANNEL_RECV as u64,
                hc1 as u64,
                &raw mut XH_RBUF as u64,
                &raw mut XH_NB as u64,
                &raw mut XH_HND as u64,
                &raw mut XH_NH as u64,
                0,
            ),
            0,
            "bench: xh 排水 recv"
        );
        assert_eq!(unsafe { XH_NB }, XH_MSG, "bench: xh recv 字节数回写");
        assert_eq!(unsafe { XH_RBUF[0] }, 0x58, "bench: xh recv 载荷首字节");
        assert_eq!(unsafe { XH_NH }, 0, "bench: xh recv handle 数回写");
    }
    // 恢复面：排空后再 send 成 → 收回该条 → 空队。
    // SAFETY: 同填队。
    assert_eq!(
        svc6(
            NR_CHANNEL_SEND as u64,
            hc0 as u64,
            core::ptr::addr_of!(XH_SBUF) as u64,
            XH_MSG,
            0,
            0,
            0,
        ),
        0,
        "bench: xh 恢复后 send 须成"
    );
    // SAFETY: 同排水。
    unsafe {
        XH_NB = XH_MSG;
        XH_NH = 4;
    }
    assert_eq!(
        svc6(
            NR_CHANNEL_RECV as u64,
            hc1 as u64,
            &raw mut XH_RBUF as u64,
            &raw mut XH_NB as u64,
            &raw mut XH_HND as u64,
            &raw mut XH_NH as u64,
            0,
        ),
        0,
        "bench: xh 恢复后 recv 收回"
    );
    // 恒等：send/recv 路径零分配（队列预储、无随行 handle）。
    assert_eq!(
        mm::frames().allocated_pages(),
        base,
        "bench: xh chan 面水位恒等"
    );
    true
}

/// 面⑤chan_drain：空队 recv 须 `-EAGAIN`（非阻塞空即返——与 ktest H1 同
/// 判据，S-2 视角重验「空也是资源边界」）。前提 = 面④收尾留空队。
fn xh_chan_drain(hc1: u32) -> bool {
    // SAFETY: 容量槽进容量（空队路径先检队空——出参不写）。
    unsafe {
        XH_NB = XH_MSG;
        XH_NH = 4;
    }
    assert_eq!(
        svc6(
            NR_CHANNEL_RECV as u64,
            hc1 as u64,
            &raw mut XH_RBUF as u64,
            &raw mut XH_NB as u64,
            &raw mut XH_HND as u64,
            &raw mut XH_NH as u64,
            0,
        ),
        ERR_AGAIN as i64,
        "bench: xh 空队 recv 须 -EAGAIN"
    );
    true
}
