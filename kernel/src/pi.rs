//! PI（优先级继承）捐赠——`IPC设计.md` §2.1「QNX 发送者语义」的 v1 落地
//! （4-3b，拆步表步 4-3 前半；intent 五件套随 4-3c 并入同一框架）。
//!
//! 微内核服务化后软实时的命门在 IPC：高优 client 阻塞在 channel、低优
//! server 拿不到 CPU，优先级反转经服务链放大（Mars Pathfinder 教科书案例）。
//! BerryOS 的有界队列（深度 8）把 QNX「send/recv 双方同睡一个 channel」的
//! 结构解耦了——**接收方持有 peer 端点 handle、可能阻塞在任何对象上**，故
//! 捐赠边不靠 channel 结构本身，而靠 handle 表反查：
//!
//! - **donor**（捐赠方）= SLEEPQ 上挂在**单一 channel 对象**（非 WAIT_ANY
//!   哨兵）且请求位含内核派生位（READABLE/WRITABLE）的睡眠者：
//!   等 WRITABLE（对端队列满，等腾位）→ 解锁动作 = 有人 **recv peer**；
//!   等 READABLE（本端队列空，等消息）→ 解锁动作 = 有人 **send peer**。
//! - **受捐方** = 持有 peer 端点且带对应解锁 right（RECV / SEND）的任务
//!   ——**SLEEPQ 在队者，或本核 RQ 在环且持有面可识别（`proc_tag != 0`）
//!   者**（形 B 前只扫 SLEEPQ = runnable 受捐盲区：高优 donor 阻塞时低优
//!   持有者已在 RUNQUEUE，boost 永远落不到持有者头上——教科书 Mars
//!   Pathfinder 无界反转；R3 实时分析定性为最大残留风险，本形收口）——
//!   多持有者全捐（任一可解锁）。共享表（`proc_tag==0`）runnable 者不作
//!   受捐方（「视作持有一切」粗粒度对环上是 4-5a 式喷洒——fa 对实证；
//!   睡眠者面 J7 语义不变）。Event 等待不构成边（无 peer 结构、signal 方
//!   多方——v2 再议）。
//!
//! **全图重推、零增量簿记**（对 QNX 式「链解除」的取舍）：每次 SLEEPQ 变更
//! （block 睡下后 / wake 摘队后，经 `set_sleepq_hook` 注入）+ handle close
//! 后，从零推导整张捐赠图——清零在队者与本核在环者的受捐额 → 不动点迭代
//! 传播（每轮一跳）→ 刷 effective。无解除遗漏、无 stale boost；代价是每次
//! 事件 O(睡眠者 × (睡眠者+环上者) × 表槽) 的重算，v1 睡眠者个位数可忽略
//! （SMP/规模扩大时换增量簿记）。
//! **P2 门控**（常态免付）：重推要产生任何改动，必要条件 = 队上有 donor
//! 可能（等派生位者，[`recompute_locked`] 入口门）或有待清 boost——双零
//! 时整体跳过（计数维护/不变式见 `sched.rs` P2 块注释；真有捐赠边的场景
//! 计数必非零，门恒放行）。
//!
//! **冻结语义（形 B 后）**（「server 处理消息期间持有」的 v1 形态）：重推
//! 清/算 **SLEEPQ 在队者 + 本核 RQ 在环者**——受捐方被唤醒离队入环后由
//! 后续重推**继续重算**（资源已就绪的边自然消失、仍被等资源的边保真重
//! 推导）；环上 boost 只在 **P2 门闭窗**（双零跳过）内冻结随行，门开或
//! 任务自身入睡时清零回落（=「reply 即回退」的无 reply 等价物，见 §2.1
//! intent 五件套之 1）。长跑不睡且门常闭者 boost 挂着——QNX 语义本就
//! 如此，v1 接受。**SMP 残留**：RQ 扫描只罩本核环——对核环上的持有者
//! 收口需远核 RQ 锁（SQ→双 RQ 升序序合法，v2 议）。
//!
//! **链式有界**：不动点迭代轮帽 [`PI_MAX_HOPS`]（8）= 阻塞链深上限的执法点
//! ——每轮传播一跳，8 轮到不了的链尾按已达 boost 处理，**不 panic**（syscall
//! 边界纪律）。持有环（A 持 e1 睡 e2、B 持 e2 睡 e1）在 max 语义下单调有界
//! 不发散，轮帽兜底。
//!
//! **零分配纪律**：`recompute` 挂在 IRQ 屏蔽热路径（block/wake 尾）——全程
//! 只读指针、写字段，无任何分配（3-4c try 化批堵住的 OOM-panic 面不得从这里
//! 重开）。

use crate::handle::{HandleTable, KernelObj, ObjRef, Rights};
use berry_sched::{Task, WAIT_ANY};
use berry_wqmatch::WaitKey;
use core::cell::Cell;
use core::ptr::NonNull;

// ---- W3-Q5 单源外迁（berry-piface 纯函数面）----
// donor 资格判（五拒 + 配对律）与轮帽常量的唯一事实源在
// `berry_piface`（host 可测钉板，18 测）；本模块保留锁窗/SLEEPQ 反查/
// retain 钉住的机械面，资格判定逐字消费 crate 函数——零第二实现。
pub use berry_piface::PI_MAX_HOPS;
use berry_piface::ObjKind;

/// 注册 PI 重推钩子（boot 期、首个切换发生前调用——`telemetry::init` 同位）。
/// 此后每次 SLEEPQ 变更（block 睡下后 / wake 摘队后）自动全图重推。
/// c-2b slice 4 起注册**持窗版** [`recompute_locked`]（发射点 SQ 已持——
/// 钩子体不自取 SQ，F-L2 不可重入立法）。
pub fn init() {
    berry_sched::set_sleepq_hook(Some(recompute_locked));
}

/// 全图重推捐赠图（零增量簿记；见模块注释）——**自持窗版**（c-2b slice 4
/// 起拆层）：SQ 窗自包 body，手动调用面（ktest PI 直调）用本版。表读随
/// SQ→表 冻结序正嵌套（复核 C1）。
pub fn recompute() {
    let sq_phys = berry_sched::sq_lock(); // 位感知退化形（c-2c slice 2a）
    recompute_locked();
    berry_sched::sq_unlock(sq_phys);
}

/// [`recompute`] 的持窗版 body：**SQ 已持**契约——全部发射点（block 双切
/// 点 / `wake_matching_locked` / `fast_wake_locked` / `expire_timeouts` /
/// kill SLEEPQ 臂）一律在 SQ 窗内发射本钩子，钩子体不再自取 SQ（同锁重入
/// = 同核自锁）。拆层前本函数体即钩子体且零自取 = KL 在场时段纯结构等价。
/// 表读（`holder_table`）3-1b 起收 **KL 子窗**（SQ→表 正序嵌套 = 终态
/// 七层冻结序——KL 归表锁本职、最内层；C20 后表读必经 KL 的常驻面之一）。
fn recompute_locked() {
    // P2 门控：等派生位睡眠者（donor 面必要条件）与 pi≠0 睡眠者（冻结
    // boost 清零面）双零 ⇒ 清零趟 / 传播趟 / 尾刷全程空转——整体跳过
    // （省 KL 取放 + 双趟全扫；常态 IPC 无捐赠边即此路）。计数由 sched
    // 侧在 SLEEPQ 两机械面精确维护（不变式与安全性论证 → sched.rs P2
    // 块注释）；本入口恒在 SQ 窗内发射（五发射点 + 自持窗手动面），读
    // 写点同窗无竞态。门常开方向的劣化 = 性能损失非正确性损失。
    // **门对形 B 的覆盖**：环上受捐的必要条件仍是「队上有 donor 可能」
    //（①非零 ⇒ 传播趟能到环上持有者）；②只数睡眠者 = 环上 stale boost
    // 的冻结面（门闭 ⇒ 冻结随行，任务入睡时经 pi_count_add 重开——
    // 语义见模块注「冻结语义（形 B 后）」）。
    if !berry_sched::pi_recompute_needed() {
        return;
    }
    // 形 B：RQ 窗先行——受捐盲区收口（runnable 持有者进重推全域）。锁序
    // = 终态七层冻结序升序前缀 SQ（已持）→ RQ →（KL 子窗）；KL 子窗必须
    // 晚开早闭（KL 取用点一律结构先取 KL 后取）。SVC 窗入口已预取结构位
    // 的发射面（wait/send 族 / thread_sleep / kill 全谱）位感知退化免物理
    // 取放——热路径零新增锁活动；IRQ/tick 与 ktest/boot 窗真取（无同核
    // 重入，论证 → sched.rs `pi_rq_lock` 注）。
    let rq_phys = berry_sched::pi_rq_lock();
    // 第一趟：清零**在队者 + 本核在环者**的受捐额（形 B 前 runnable 者
    // 只能冻结尾趟值——盲区半边即此）。纯 Task 字段写，不触表——KL 窗
    // 外合法。
    berry_sched::for_each_sleeper(|t| t.pi_inherited = 0);
    berry_sched::for_each_runnable(|t| t.pi_inherited = 0);

    // 不动点迭代：每轮沿边传播一跳。`Cell` 规避嵌套闭包的可变捕获冲突。
    // 3-1b：整段包 KL 子窗——内层 `holder_table` 反查读 bootstrap 表，
    // 表读必经 KL（终态锁序 SQ→KL 正序嵌套；粗窗 = 重推期间表变更被
    // KL 串行，与发射点 SQ 串行叠成一致快照）。**位感知退化（判据 =
    // HAL 持有者镜像非任务位）**：SVC TABLE 类主体（KL 已持）内经
    // wake/block/close 发射本钩 → 跳物理取放防同核重入自旋死锁——
    // 任务位不够（ktest/boot 窗 CURRENT=null 位记账不存在，而 HAL 直取
    // 的 KL 物理持有态真实，sys_handle_close 直调面即此形态）；IRQ/tick
    // 语境镜像**g3-1c-2 起非恒 0**——解蔽窗内 tick/SGI 打断持 KL 的 send/recv
    // 主体时走同款退化跳过支（持有者被断未并跑，判据问锁本身故两语境同正）；
    // 窗外语境镜像仍恒 0 照常真取（`kern_lock::kl_held` 判据注）。
    let kl_phys = !berry_hal_aarch64::kern_lock::kl_held();
    if kl_phys {
        berry_hal_aarch64::kern_lock::kern_acquire();
    }
    for _ in 0..PI_MAX_HOPS {
        let changed = Cell::new(false);
        berry_sched::for_each_sleeper(|donor| {
            let Some((peer, need)) = donor_edge(donor) else { return };
            // donor 的有效优先级 = max(尾趟才刷的 prio_effective, 本轮**实时**的
            // pi_inherited)——后者是链式传播的命门：pi 在轮内被上游更新后，同轮/
            // 下轮的下游捐赠立刻可见（只读 prio_effective 会把传播截死在一跳，
            // 轮帽「每轮一跳」的语义也随之落空）。也避免嵌套闭包再借 donor。
            let d_eff = donor.prio_effective.max(donor.pi_inherited);
            berry_sched::for_each_sleeper(|cand| {
                let Some(ht) = holder_table(cand) else { return };
                if ht.holder_has(peer, need) && d_eff > cand.pi_inherited {
                    cand.pi_inherited = d_eff;
                    changed.set(true);
                }
            });
            // 形 B：本核 RQ 受捐扫描（与 SLEEPQ 受捐同形——判定同一
            // `holder_has`）。donor 所等资源的 peer 端点持有者可能正
            // runnable（高优 client 阻塞、低优 server 在环等 CPU——
            // Mars Pathfinder 形态）：boost 只有写进环上持有者的
            // pi_inherited 并经尾趟刷进 prio_effective，pick 才看得见。
            // 钩子发射点与入睡摘环序保证 SLEEPQ/RQ 任务集此刻不相交
            //（block 先摘环后发钩 / wake 先入环后发钩）——无双访。
            // **共享表护栏（形 B 边界）**：`proc_tag==0`（内核侧共享表——
            // 「视作持有表内一切」的 v1 粗粒度）的 runnable 者不作受捐方
            // ——粗粒度对睡眠者无害（J7 面，4-3b 起语义不变），对环上是
            // 4-5a 式 boost 喷洒：任一 donor 入睡即全环抬升（fa 对实证
            // ——pong(32) 一睡、共享表全环任务喷到 32：矩阵优先级塌平
            // G1.5 超时 + ping 升 32 塌掉 32>16 直切契约）。受捐方须
            // 真实可识别（proc_tag≠0 = EL0 进程表/spawn_with_tag 私有表
            // ——生产 client/server 拓扑即此形）。
            berry_sched::for_each_runnable(|cand| {
                if cand.proc_tag == 0 {
                    return; // 共享表 runnable：不可识别持有面——不捐（喷洒护栏）
                }
                let Some(ht) = holder_table(cand) else { return };
                if ht.holder_has(peer, need) && d_eff > cand.pi_inherited {
                    cand.pi_inherited = d_eff;
                    changed.set(true);
                }
            });
        });
        if !changed.get() {
            break; // 不动点：再无传播
        }
    }
    if kl_phys {
        berry_hal_aarch64::kern_lock::kern_release();
    }

    // 尾趟：刷在队者 + 本核在环者 effective。环上者同刷——boost 只有落进
    // prio_effective 才被 pick/try_preempt 消费（形 B 的行为出口即此；公式
    // 与 pick 侧 policy::apply 同一：max(base + 策略权重, pi_inherited)，
    // weight_of 免锁读——RQ 窗内合法）。纯字段算术不触表。顺带重数回写
    // 两门控计数（P2 第三收口点**双扩展**——W3-Q94 刀 a）：② pi≠0 在队者
    // 个数（重推是在队者 pi_inherited 唯一写者，写尾即精确对账）与
    // ① donor 判据命中数（入队同款判据 keyroute 单源——d_eff 此刻含本
    // 轮传播终值，入队 d_eff=0 睡眠中被抬 pi 者在此补登；**环上受捐者不
    // 进两计数**——计数语义 = 睡眠者面，环上 boost 的清零由「donor 在队
    // ⇒ ①非零 ⇒ 门开」罩住，门闭冻结随行经 pi_count_add 于入睡时重新
    // 入账，口径闭环）。
    let boosted = Cell::new(0u32);
    let donors = Cell::new(0u32);
    berry_sched::for_each_sleeper(|t| {
        berry_sched::refresh_effective(t);
        if t.pi_inherited != 0 {
            boosted.set(boosted.get() + 1);
        }
        // W3-Q94 判定单源（入队同款三合判据——d_eff 与传播环同构）。
        if berry_wqmatch::keyroute::pi_donor_candidate(
            t.wait_obj,
            t.wait_signals,
            t.prio_effective.max(t.pi_inherited),
        ) {
            donors.set(donors.get() + 1);
        }
    });
    berry_sched::for_each_runnable(|t| berry_sched::refresh_effective(t));
    berry_sched::pi_boost_recount(boosted.get());
    berry_sched::pi_derived_recount(donors.get());
    // RQ 窗闭（后于 KL 子窗——开窗序的严格逆）。
    berry_sched::pi_rq_unlock(rq_phys);
}

/// donor 的捐赠边（机械壳——资格判定单源 `berry_piface::donor_edge`）：
/// 等 channel 派生位的单一对象等待者 → (peer 端点, 解锁需的 right 集)。
/// 不是 channel donor（纯 sleep / WAIT_ANY / Event / 只等用户位）→ `None`。
///
/// W3-Q5：五拒 + 配对律（等 WRITABLE → RECV peer、等 READABLE →
/// SEND peer、双位并集）全部在 crate 钉板；本壳只做 raw 指针 → 值域的
/// 投影（wait_obj 解引用判 KernelObj 形 + peer 反查），语义零复制。
fn donor_edge(t: &Task) -> Option<(NonNull<ObjRef>, Rights)> {
    // 派生位门前置（§A40 判序还原）：只等用户位的睡眠者不解引用 wait_obj
    // ——它的 wait_obj 可以是任意裸键（dpg.rs FAULT_KEY/PAGER_KEY 零大小
    // 静态地址、ktest 手搭睡眠者的非对象值——4-2a「用户位永不来自 channel」
    // 下无解锁语义，dereference 前必须拒。#77 外迁时此门从壳挪进 crate
    // 参数门，但壳仍需解引用才能填 ObjKind → 裸键漏进 deref，QEMU smoke
    // RX 相 panic（pi.rs:140 misaligned）。还原判序：信号门在解引用前）。
    if t.wait_signals & (berry_evface::SIGNAL_READABLE | berry_evface::SIGNAL_WRITABLE) == 0 {
        return None;
    }
    let (obj_kind, peer) = match t.wait_obj {
        0 | WAIT_ANY => (ObjKind::Other, None), // 纯睡/哨兵：crate 侧键门拒
        raw => {
            // SAFETY: 睡眠者全程 retain 钉住其等待对象（4-1 钉住纪律）——
            // 在队期间 wait_obj 指针有效；本函数只在屏蔽窗内的重推里调用，
            // 无并发销毁。
            let obj: &ObjRef = unsafe { &*(raw as *const ObjRef) };
            match &obj.obj {
                KernelObj::Channel(end) => {
                    (ObjKind::Channel, end.peer().map(|p| p.as_ptr() as usize))
                }
                _ => (ObjKind::Other, None), // Event 等：无 peer 结构
            }
        }
    };
    let edge = berry_piface::donor_edge(
        WaitKey::from_raw(t.wait_obj),
        t.wait_signals,
        obj_kind,
        peer,
    )?;
    // SAFETY: peer 值源自 `ChannelEnd::peer()` 返回的 NonNull（端点存活
    // 期间常驻——睡眠者 retain 钉住等待对象、channel 双端互钉）；本函数
    // 只在屏蔽窗内的重推里调用，无并发销毁。重建 NonNull 不解引用、
    // 仅作 holder_has 的指针相等键。
    let peer = unsafe { NonNull::new_unchecked(edge.0 as *mut ObjRef) };
    Some((peer, edge.1))
}

/// 任务 `t` 的 handle 表（受捐判定的持有面）。`proc_tag != 0`（EL0 所属进程表
/// 或 `spawn_with_tag` 常驻内核服务的私有表）→ 指向的 leak 常驻表（3-4a）；
/// 内核任务（`proc_tag == 0`，ktest/共享表短命任务）→ 内核侧表
/// `BOOTSTRAP_HT`（与 `caller_table` 的 tag 优先派发同源）——共享表睡眠者被
/// 视作持有表内一切 handle（v1 粒度，短命 bench 任务无害；常驻服务勿用）。
fn holder_table(t: &Task) -> Option<&'static HandleTable> {
    if t.proc_tag != 0 {
        // SAFETY: proc_tag 由 spawn_user/process 语义层填入——非 0 即指向该
        // 进程的 leak 常驻 HandleTable（3-4a 三不变量）；任务活着（在 SLEEPQ
        // 上）→ 其表活着（Task 钉 Process、Process 拥有表）。屏蔽窗内无并发。
        Some(unsafe { &*(t.proc_tag as *const HandleTable) })
    } else {
        Some(crate::syscall::bootstrap_table())
    }
}
