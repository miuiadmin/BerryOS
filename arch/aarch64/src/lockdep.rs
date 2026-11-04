//! G3-3 lockdep（运行时锁序检测器——`G3全抢占实施设计草案.md` §3；
//! cargo feature `lockdep`，**默认关**：一切钩子 `#[cfg]` 编译消除，
//! release/CI 主面零成本）。
//!
//! **先验序执法**（§3「检测器校验实现不违立法而非重新发明序」）：锁序立法
//! 七类全序（S-3 §9.7：SQ > 队列 > REAPQ > 表 > 分配器 > ASID > 打印，KL
//! 最内/末取）作为先验——物理取锁时校验「地板上已持类 rank ≤ 新类 rank」，
//! 违序 = fail loud（`LOCKDEP VIOLATION` 直写 UART + 停机，smoke 可 grep）。
//! RQ0/RQ1 分立类号同 rank + **升序专项**（「双核升序」立法机器化：持 RQ1
//! 再取 RQ0 = 降序违约）。
//!
//! **持有链数据源**：per-CPU 类栈（[`crate::smp::SmpCpu`] 尾插第四组，
//! S-5a/G3-1a 同款尾插纪律）——不复用 `Task::held_locks` 位图（位图只盖
//! 结构位、随任务走；链须盖 KL/PRINT 且随**执行流**走）。本核独占访问：
//! 一切取放点恒在 IRQ 屏蔽窗（雷 2 纪律）⇒ 无原子需求。
//!
//! **IRQ 地板**（跨流分层）：**不变量——一切 IRQ 解蔽持锁窗必须段级
//! `preempt_disable`（计数 >0 禁切，`syscall.rs` irq_window 包络）**；现役
//! 两实例（PRINT 包络 G43/G3-1c-1 + send/recv 拷贝窗 g3-1c-2——SVC 0x2E
//! 包络）；新增解蔽持锁窗先过审（缺计数 ⇒ floor>0 期 context_switch 可达
//! ⇒ 幽灵 irq-unbalanced 停机）。IRQ 打断持锁者时，handler 取结构锁（如
//! RX → wake_matching → SQ）对**本流**合法（被断者不持结构锁、无争用）
//! ——链按 `exception_irq` 入口快照分层：地板下 = 被断流词条（不参与序
//! 检），地板上 = 本流词条（序检/配平域）。出口断言深度恰归地板（handler
//! 配平执法）。
//!
//! **切换点不变量**：切换原语入口断言 `count==0 ∧ held==0` ⇒ 一切切换点
//! 链恒配平（任务态归 0 / IRQ 态归地板）⇒ per-CPU 链跨 context_switch
//! 连贯（切走者舞蹈先放、切回者重取，钩子对称记录）。
//!
//! **覆盖缺口（v1 注记）**：TPIDR 未立窗（ktest/极早期 boot）槽空跳过——
//! 检测自 `cpu0_init` 起全覆盖（enable_irq/用户任务全在其后）；ktest 窗
//! 挂账后续刀。**类覆盖（g3-3c 勘正：ASID/PT 原误记「预留无实例」）**：
//! 现役八类全挂钩——SQ/RQ0/RQ1/REAPQ/TABLE/PRINT 原批 + PT（S-4 4-2
//! `vmar::PT_LOCKS`，集中挂 `pt_lock`/`PtGuard::drop` 一对位点覆盖全树
//! 6 物理取点）+ ASID（S-4 4-3 `aspace::ASID_LOCK`，`asid_lock` 集中取
//! 点 + 两放点）；ALLOC 预留属实（帧分配器纯 KL 串行——无独立分配器锁
//! 实例，类号占位，实例出现即挂钩）。
//!
//! **Kill criteria**（§3）：debug 构建慢 >3× 或误报率 >10% → 降级取放
//! trace 面。**panic bypass 对齐**：违约打印直写 UART 不取 PRINT_LOCK
//! （检测点可能持锁，取锁 = 递归违约；panic 路径同款纪律）。

/// SLEEPQ 锁类（最外层）。
pub const CLASS_SQ: u8 = 1;
/// CPU0 就绪队列锁类。
pub const CLASS_RQ0: u8 = 2;
/// CPU1 就绪队列锁类（与 RQ0 同 rank——同类跨取合法，升降序另检）。
pub const CLASS_RQ1: u8 = 3;
/// REAPQ 锁类。
pub const CLASS_REAPQ: u8 = 4;
/// 表类（KL——KERN_LOCK，类内末取）。
pub const CLASS_TABLE: u8 = 5;
/// PT 页表锁类（per-ASID `vmar::PT_LOCKS`——S-4 4-2 实锁，g3-3c 入册）。
/// **与 TABLE 同 rank=4**（RQ0/RQ1 分立类号先例）：rank 表无层内序，双向
/// 跨取合法；现役单向（「PT 恒在 KL 窗内取放」单一嵌套向——`vmar.rs`
/// 无环论证）。类号是标签非 rank（9 > PRINT 的 8——rank 见 [`rank`]）。
/// **重入检穷举前提（g3-3c 枚举）**：v1 全树无「持 A 槽 PT 再取 B 槽 PT」
/// 形态（写者 map/unmap/protect 方法体零 pt_lock 调用；copy 三辅助互不
/// 嵌套、vmo 分块与 channel 六趟皆顺序取放；跨 ASID 构造纯顺序——
/// `new_root(fa, alloc_asid())` 实参先走完锁周期）——同类号跨槽映射无
/// 假阳性面。未来出现合法跨槽嵌套须 (类,实例) 粒度或立法豁免。
pub const CLASS_PT: u8 = 9;
/// 分配器锁类（**预留**：帧分配器纯 KL 串行——现役无独立分配器锁实例）。
pub const CLASS_ALLOC: u8 = 6;
/// ASID 锁类（`aspace::ASID_LOCK`——S-4 4-3 实锁，g3-3c 挂钩；临界区纯
/// 计数器推进，与 PT 永不同持）。
pub const CLASS_ASID: u8 = 7;
/// 打印锁类（最内层——「持 PRINT_LOCK 者禁取任何锁」由 rank 检自动执法）。
pub const CLASS_PRINT: u8 = 8;
/// 类号域上界（1..=CLASS_MAX 在册；[`check_acquire`] 前置校验用——F5）。
pub const CLASS_MAX: u8 = CLASS_PT;

/// 链深帽：理论最坏同持 = SVC 全类（SQ+RQ0+RQ1+REAPQ）+ KL + PT + PRINT
/// = 8；g3-3c 抬 8→12 留裕（PT/ASID 入检测域 + 未来类扩展）——`SmpCpu`
/// 逻辑尺寸 126 仍 ≤ 128 帽（smp.rs 偏移断言同步）。
pub const DEPTH_MAX: usize = 12;

/// 类号 → 立法 rank（1 最外 … 7 最内；S-3 §9.7 七类全序）。RQ0/RQ1 同
/// rank=2（跨取合法，升降序由 [`acquire`] 专项检）。
const fn rank(class: u8) -> u8 {
    match class {
        CLASS_SQ => 1,
        CLASS_RQ0 | CLASS_RQ1 => 2,
        CLASS_REAPQ => 3,
        CLASS_TABLE | CLASS_PT => 4,
        CLASS_ALLOC => 5,
        CLASS_ASID => 6,
        CLASS_PRINT => 7,
        // 未知类号：入检侧已 fail loud 拦截（check_acquire 前置校验——F5），
        // 链内词条恒经入检，此臂对合法流不可达 = 腐写兜底。
        _ => 7,
    }
}

/// 取锁检查面（纯检查——[`acquire`] 与 [`selftest`] 阳性对照共用）：
/// 返回违约定名（Some）或通过（None）。检查域 = `chain[floor..depth]`
///（地板上 = 本流词条）。
fn check_acquire(chain: *const u8, depth: u8, floor: u8, class: u8) -> Option<&'static [u8]> {
    if depth as usize >= DEPTH_MAX {
        return Some(b"chain-overflow");
    }
    // 前置校验（F5）：类号域外 = fail loud——防未来加类漏登 rank 表被静默
    // 错分（原兜底静默映射 rank 7=PRINT，错分方向不可预测）。
    if class == 0 || class > CLASS_MAX {
        return Some(b"unknown-class");
    }
    for i in floor..depth {
        // SAFETY: 调用方契约——i < DEPTH_MAX 且链本核独占。
        let held = unsafe { *chain.add(i as usize) };
        // ① 重入：地板上同类二取 = 非重入 TTAS 同实例自死锁形（wrapper
        //    位感知退化应已杜绝，此处兜底；RQ0/RQ1 分立类号故同类必同锁）。
        if held == class {
            return Some(b"reentry");
        }
        // ② 先验序：地板上已持类 rank 须 ≤ 新类 rank（七类全序执法）。
        if rank(held) > rank(class) {
            return Some(b"order");
        }
        // ③ RQ 升序专项：「双核升序」立法（持 RQ1 再取 RQ0 = 降序；偷取
        //    特则「只持远端」不构成双持——S-3 §9.7，无误报面）。
        if class == CLASS_RQ0 && held == CLASS_RQ1 {
            return Some(b"rq-descend");
        }
    }
    None
}

/// 物理取锁记账 + 三检。**只在物理取点调**（位感知退化支不触——wrapper
/// 层纪律，见 sched 侧包装组注释）。
#[inline]
pub fn acquire(class: u8) {
    let Some((chain, depth, floor)) = crate::smp::lockdep_slot() else {
        return; // TPIDR 未立（ktest/极早 boot 窗）——覆盖缺口，模块头注记
    };
    // SAFETY: 本核独占写（取放点恒 IRQ 屏蔽窗——雷 2）；裸指针 = SmpCpu
    // 尾插字段（链栈/深度/地板三元组，lockdep_slot 契约）。
    unsafe {
        let (d, f) = (*depth, *floor);
        if let Some(kind) = check_acquire(chain, d, f, class) {
            violation(kind, class, chain, d, f);
        }
        *chain.add(d as usize) = class;
        *depth = d + 1;
    }
}

/// 物理放锁记账 + 配平检（深度恰减、弹出的恰是本类）。**只在物理放点调**
/// （位感知退化支不触——同 [`acquire`]）。
#[inline]
pub fn release(class: u8) {
    let Some((chain, depth, floor)) = crate::smp::lockdep_slot() else {
        return;
    };
    // SAFETY: 同 [`acquire`]。
    unsafe {
        let (d, f) = (*depth, *floor);
        if let Some(kind) = check_release(chain, d, f, class) {
            violation(kind, class, chain, d, f);
        }
        *depth = d - 1;
    }
}

/// 放锁检查面（纯检查——[`release`] 与 [`selftest`] 阳性对照共用，F4）：
/// 返回违约定名（Some）或通过（None）。两臂：地板下放锁（越流放锁/配平
/// 早已破）+ 栈顶非本类（放错对/序错）。
fn check_release(chain: *const u8, depth: u8, floor: u8, class: u8) -> Option<&'static [u8]> {
    if depth == floor {
        return Some(b"release-underflow");
    }
    // SAFETY: 调用方契约——depth > floor 且链本核独占。
    if unsafe { *chain.add(depth as usize - 1) } != class {
        return Some(b"release-unpair");
    }
    None
}

/// IRQ 出口配平检查面（纯检查——[`irq_floor_lower`] 与 [`selftest`] 阳性
/// 对照共用，F4）：深度须恰归地板（handler 配平执法——留残 = 有锁未放
/// 跨流泄漏）。
fn check_irq_exit(depth: u8, floor: u8) -> Option<&'static [u8]> {
    if depth != floor {
        Some(b"irq-unbalanced")
    } else {
        None
    }
}

/// IRQ 地板抬高（`exception_irq` 入口）：地板 = 入口深度快照——被断流
/// 词条沉底，本流（handler）词条全在其上，序检域 = 地板..深度。
#[inline]
pub fn irq_floor_raise() {
    let Some((_, depth, floor)) = crate::smp::lockdep_slot() else {
        return;
    };
    // SAFETY: IRQ 上下文本核独占。
    unsafe { *floor = *depth };
}

/// IRQ 地板收回（`exception_irq` 出口）：断言深度恰归地板（handler 配平
/// 执法——留残 = 有锁未放跨流泄漏）后地板归 0（IRQ 不可嵌套：handler 全程
/// PSTATE.I=1）。
#[inline]
pub fn irq_floor_lower() {
    let Some((chain, depth, floor)) = crate::smp::lockdep_slot() else {
        return;
    };
    // SAFETY: 同上。
    unsafe {
        if let Some(kind) = check_irq_exit(*depth, *floor) {
            violation(kind, 0, chain, *depth, *floor);
        }
        *floor = 0;
    }
}

/// 检测器阳性自证（g3-3b；g3-3c 扩放锁/IRQ 侧三臂）：①活性 = 真链立法
/// 全序九连推弹（走 [`acquire`]/[`release`] 本体——槽寻址/检查/记账全路径
/// 激活证，PT 同 rank 位与 ALLOC/ASID 类的检查面同被走到）；②阳性对照 =
/// 六条合成违约**只走纯检查面**（[`check_acquire`] order/reentry/rq-descend
/// + [`check_release`] underflow/unpair + [`check_irq_exit`] unbalanced——
/// 不落链不停机）+ 地板隔离正对照（被断流词条不进序检域）+ 放锁/IRQ 配平
/// 负对照（合法路径须全过——「删死」类突变不再与健康态同指纹）——证明
/// 「零 VIOLATION」≠「检测器死了」。boot 窗调用契约：IRQ 屏蔽 + TPIDR
/// 已钉 + 链净态；marker 直写 UART（`LOCKDEP selftest ok/FAIL`——smoke 门
/// grep 面；FAIL 不停机，冒烟自然红）。
pub fn selftest() -> bool {
    let Some((chain, depth, floor)) = crate::smp::lockdep_slot() else {
        return false; // TPIDR 未钉——调用点契约违（selftest 只许 cpu0_init 后）
    };
    // SAFETY: boot 单线程窗（IRQ 屏蔽）本核链独占；净态由入口判保证。
    unsafe {
        if *depth != 0 || *floor != 0 {
            return false; // 非净态——不破坏现场，拒绝自检
        }
        // ① 活性：全序九连（rank 非降序列——PT 与 TABLE 同 rank 4 相邻）真推真弹。
        for c in [
            CLASS_SQ,
            CLASS_RQ0,
            CLASS_RQ1,
            CLASS_REAPQ,
            CLASS_TABLE,
            CLASS_PT,
            CLASS_ALLOC,
            CLASS_ASID,
            CLASS_PRINT,
        ] {
            acquire(c);
        }
        let live = *depth == 9 && *chain.add(8) == CLASS_PRINT;
        for c in [
            CLASS_PRINT,
            CLASS_ASID,
            CLASS_ALLOC,
            CLASS_PT,
            CLASS_TABLE,
            CLASS_REAPQ,
            CLASS_RQ1,
            CLASS_RQ0,
            CLASS_SQ,
        ] {
            release(c);
        }
        let balanced = *depth == 0;
        // ② 阳性对照：合成态只走纯检查面（毕后复原净态）。
        *chain.add(0) = CLASS_TABLE;
        let neg_order = check_acquire(chain, 1, 0, CLASS_SQ).is_some();
        *chain.add(0) = CLASS_SQ;
        *chain.add(1) = CLASS_SQ;
        let neg_reentry = check_acquire(chain, 2, 0, CLASS_SQ).is_some();
        *chain.add(0) = CLASS_RQ1;
        let neg_rq = check_acquire(chain, 1, 0, CLASS_RQ0).is_some();
        // 放锁侧（F4）：underflow = 地板下放锁（depth==floor）；unpair =
        // 栈顶非本类（链顶 RQ1、放 REAPQ）。
        let neg_underflow = check_release(chain, 1, 1, CLASS_SQ).is_some();
        let neg_unpair = check_release(chain, 1, 0, CLASS_REAPQ).is_some();
        // IRQ 出口侧（F4）：depth≠floor = handler 留残。
        let neg_irqbal = check_irq_exit(2, 1).is_some();
        // 负对照：合法路径须全过（检查面无双杀）。
        *chain.add(0) = CLASS_PRINT;
        let pos_floor = check_acquire(chain, 1, 1, CLASS_SQ).is_none();
        *chain.add(0) = CLASS_SQ;
        let pos_release = check_release(chain, 1, 0, CLASS_SQ).is_none();
        let pos_irqbal = check_irq_exit(1, 1).is_none();
        *depth = 0;
        *floor = 0;
        let ok = live
            && balanced
            && neg_order
            && neg_reentry
            && neg_rq
            && neg_underflow
            && neg_unpair
            && neg_irqbal
            && pos_floor
            && pos_release
            && pos_irqbal;
        crate::uart::uart_puts(if ok {
            b"\nLOCKDEP selftest ok\n"
        } else {
            b"\nLOCKDEP selftest FAIL\n"
        });
        ok
    }
}

/// 违约报告：直写 UART（panic bypass 同款——不取 PRINT_LOCK）+ 停机。
/// smoke 面 grep `LOCKDEP VIOLATION` 即门禁标记。
fn violation(kind: &[u8], class: u8, chain: *const u8, depth: u8, floor: u8) -> ! {
    crate::uart::uart_puts(b"\nLOCKDEP VIOLATION: ");
    crate::uart::uart_puts(kind);
    crate::uart::uart_puts(b" class=");
    crate::uart::uart_put_dec(class as u64);
    crate::uart::uart_puts(b" cpu=");
    crate::uart::uart_put_dec(crate::smp::cpu_id() as u64);
    crate::uart::uart_puts(b" chain=[");
    // SAFETY: 只读链快照（停机面前无并发意义）。
    unsafe {
        for i in 0..depth {
            if i != 0 {
                crate::uart::uart_puts(b" ");
            }
            let c = *chain.add(i as usize);
            if i == floor {
                crate::uart::uart_puts(b"|"); // 地板分隔符：左 = 被断流
            }
            crate::uart::uart_put_dec(c as u64);
        }
    }
    crate::uart::uart_puts(b"] depth=");
    crate::uart::uart_put_dec(depth as u64);
    crate::uart::uart_puts(b" floor=");
    crate::uart::uart_put_dec(floor as u64);
    crate::uart::uart_puts(b"\n");
    loop {
        core::hint::spin_loop(); // fail loud：停机等复位/超时面
    }
}
