//! berry-wqmatch —— 等待队列唤醒匹配纯函数面（W3-Q2）。
//!
//! **动机**：v1 单队列匹配唤醒（`sched::wake_matching` / fastpath 谓词
//! `find_single_waiter`）的匹配语义此前只活在 `sched/src/sched.rs` 的
//! raw 指针扫描里——host 测不可达（sched 依赖 HAL global_asm，只
//! aarch64-unknown-none 可编），ktest 只覆盖 boot 相。IPC 等待语义的
//! 回归面（b-1 广播判谳、WAIT_ANY 哨兵、fastpath 资格三律）需要一张
//! host 可测的**钉板**：本 crate 把谓词按值域抽成纯函数 + 表驱动测试，
//! `sched` 侧消费同一函数（零第二实现——抽离即唯一事实源，非镜像）。
//!
//! 抽离面（与 sched 侧逐字对齐，改这里的语义 = 改 sched 行为）：
//! - [`matches_wake`]：单等待者谓词——`wait_obj == obj`（或 WAIT_ANY
//!   哨兵）**且**请求位与 `set_signals` 相交。b-1 判谳（S-3b）的语义
//!   核心：唤醒面 = 纯广播，公平/上限绝不进选择器——谓词里没有也不允许
//!   出现任何到达序/优先级/配对上限项。
//! - [`waiter_set`]/[`wake_set`]：队列 → 唤醒集合（顺序保持 = SLEEPQ
//!   扫描序，仅用于断言「集合相等」——b-1 下顺序无语义）。
//! - [`single_waiter`]:fastpath 资格判——恰一个**非哨兵**匹配者且队列上
//!   无任何位相交哨兵 → `Some(idx)`；否则 `None`（退 slowpath）。含
//!   b-2 LB 修正（位不相交哨兵不挡 fastpath——wait_many 收割者常驻挂
//!   TERMINATED 位，无条件退 slowpath = 全系统 send 直切关停）。
//! - [`WaitKey`]：等待键三形（纯值域——`0` 纯睡 / 对象地址 / WAIT_ANY）。
//!
//! 与冻结面的关系：syscall ABI 零触碰（纯内核内部语义）；SLEEPQ 物理
//! 结构不在此 crate（raw 指针/链表留在 sched——本面只钉「谁该醒」的
//! 判定律，不钉「怎么摘链」的机械）。

#![cfg_attr(not(test), no_std)]
#![allow(clippy::result_unit_err)]

// no_std 面也返回 Vec（宿主测试主消费面；sched 侧零分配路径不触本函数
// ——只消费谓词本体）。alloc 面与 sched/berry-mm 同构。
#[cfg(not(test))]
extern crate alloc;
#[cfg(not(test))]
use alloc::vec::Vec;

pub const WAIT_ANY: usize = usize::MAX;

/// 等待键三形（值域钉板：Task.wait_obj 的合法域 + 各形语义）。
///
/// - `PureSleep`：`wait_obj == 0`——纯睡，只能超时自醒；
///   [`matches_wake`] 恒 false（0 在 [`wake_matching`] 入口即拒——
///   两处防线独立，语义一致）。
/// - `Obj(addr)`：等单一对象（channel 端点 / Event / 线程键）。
/// - `Any`：wait_many 哨兵（4-2b）——任何对象的任何新置位都命中，
///   醒来重验归调用方（虚假唤醒无害）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitKey {
    PureSleep,
    Obj(usize),
    Any,
}

impl WaitKey {
    /// 原始 `wait_obj` 值 → 三形。
    pub fn from_raw(wait_obj: usize) -> Self {
        match wait_obj {
            0 => WaitKey::PureSleep,
            WAIT_ANY => WaitKey::Any,
            a => WaitKey::Obj(a),
        }
    }

    /// 三形 → 原始值（roundtrip 钉板用）。
    pub fn to_raw(self) -> usize {
        match self {
            WaitKey::PureSleep => 0,
            WaitKey::Obj(a) => a,
            WaitKey::Any => WAIT_ANY,
        }
    }
}

/// 单等待者描述（值语义——host 表驱动测试的行形状；sched 侧由
/// `&Task` 字段投影而来，非 Task 别名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaiterDesc {
    /// 等待键三形。
    pub key: WaitKey,
    /// 请求信号位（wait_signals；0 = 纯睡面，与任何 set_signals 不相交）。
    pub wait_signals: u64,
}

/// 唤醒谓词（b-1 广播不变量的全部语义——一字不多）：
/// 键匹配（obj 相等 **或** WAIT_ANY 哨兵）且请求位与新置位集相交。
///
/// 公平/上限/到达序绝不进此函数（b-1 排障定谳：曾试按 wait_since 到达
/// 序醒 1，广播同集合仅换序即断 CHAIN 链——选择器零熵下首个被选者永久
/// 锁死容量槽；容量槽公平归放置面 place 与 b-2 偷取迁移，不归唤醒面）。
#[inline]
pub fn matches_wake(w: &WaiterDesc, obj: usize, set_signals: u64) -> bool {
    let key_match = match w.key {
        WaitKey::Obj(a) => a == obj,
        WaitKey::Any => true,
        WaitKey::PureSleep => false,
    };
    key_match && (w.wait_signals & set_signals) != 0
}

/// 全队列扫描的唤醒集合（保持输入序——SLEEPQ 扫描序；b-1 下序无语义，
/// 返回序仅为断言方便）。
pub fn wake_set<'a>(queue: &'a [WaiterDesc], obj: usize, set_signals: u64) -> Vec<&'a WaiterDesc> {
    queue
        .iter()
        .filter(|w| matches_wake(w, obj, set_signals))
        .collect()
}

/// [`wake_set`] 的下标版（sched 摘链机械的数据源；fastpath 资格判同形）。
pub fn wake_indices(queue: &[WaiterDesc], obj: usize, set_signals: u64) -> Vec<usize> {
    queue
        .iter()
        .enumerate()
        .filter(|(_, w)| matches_wake(w, obj, set_signals))
        .map(|(i, _)| i)
        .collect()
}

/// fastpath 资格判（4-4 检查链 SLEEPQ 侧谓词——`find_single_waiter`
/// 语义的纯面投影）：
///
/// `Some(i)` = 恰存在一个**非哨兵**匹配者（下标 i）且队列上不存在任何
/// 与 `set_signals` **位相交**的 WAIT_ANY 哨兵等待者 → 可定点唤醒 i。
/// `None` = 以下任一 → 退 slowpath（`wake_matching` 全摘）：
/// - 零或 ≥2 个非哨兵匹配者（单点唤醒集合 ≠ 全匹配集）；
/// - 存在位相交哨兵（哨兵也在唤醒集内，唤醒集不可约）。
///
/// **b-2 LB 修正**：位**不相交**的哨兵不在唤醒集内 → 不挡 fastpath
/// （wait_many 收割者常驻挂 TERMINATED 位——若无条件退 slowpath，其
/// 存活期 = 全系统 send 直切关停，ai_ping 类 poll-0 直切确定性测试
/// 结构必断）。
///
/// 边界（与 sched 侧逐字对齐）：`obj == 0 || signals == 0` 恒 `None`。
pub fn single_waiter(queue: &[WaiterDesc], obj: usize, set_signals: u64) -> Option<usize> {
    if obj == 0 || set_signals == 0 {
        return None;
    }
    let mut found: Option<usize> = None;
    for (i, w) in queue.iter().enumerate() {
        // 哨兵分支在前（sched 侧同序）：位相交哨兵 → 立即 None。
        if w.key == WaitKey::Any && (w.wait_signals & set_signals) != 0 {
            return None;
        }
        if matches_wake(w, obj, set_signals) {
            if found.is_some() {
                return None; // 多匹配者 → slowpath
            }
            found = Some(i);
        }
    }
    found
}

#[cfg(test)]
extern crate alloc;

// ---- host 表驱动测试（cfg(test) 链 std；W3-Q2 交付面）----

#[cfg(test)]
mod tests {
    use super::*;

    /// 信号位常量（镜像 kernel abi_gen：SIGNAL_READABLE/WRITABLE/TERMINATED）。
    const READABLE: u64 = 1 << 0;
    const WRITABLE: u64 = 1 << 1;
    const TERMINATED: u64 = 1 << 2;
    const WAKE_PENDING: u64 = 1 << 3;

    fn w(key: WaitKey, sig: u64) -> WaiterDesc {
        WaiterDesc { key, wait_signals: sig }
    }

    fn obj(n: usize) -> WaitKey {
        WaitKey::Obj(0x1000 + n * 0x10)
    }

    fn addr(n: usize) -> usize {
        0x1000 + n * 0x10
    }

    // ---- T1. WaitKey 三形值域钉板 ----

    #[test]
    fn t1_wait_key_roundtrip() {
        // 三形 ↔ 原始值全往返。
        for k in [
            WaitKey::PureSleep,
            WaitKey::Obj(1),
            WaitKey::Obj(usize::MAX - 1),
            WaitKey::Any,
        ] {
            assert_eq!(WaitKey::from_raw(k.to_raw()), k, "roundtrip {k:?}");
        }
        // 值域钉：0 / usize::MAX 是保留形，普通地址落 Obj。
        assert_eq!(WaitKey::from_raw(0), WaitKey::PureSleep);
        assert_eq!(WaitKey::from_raw(usize::MAX), WaitKey::Any);
        assert_eq!(WaitKey::from_raw(1), WaitKey::Obj(1));
        // usize::MAX-1 是最大合法 Obj 地址（不与哨兵冲突）。
        assert_eq!(WaitKey::from_raw(usize::MAX - 1), WaitKey::Obj(usize::MAX - 1));
    }

    // ---- T2. matches_wake 谓词全分支 ----

    #[test]
    fn t2_predicate_branches() {
        let target = addr(0);
        // 键相等 + 位相交 → true。
        assert!(matches_wake(&w(obj(0), READABLE), target, READABLE));
        // 键相等 + 位不相交 → false（请求位门）。
        assert!(!matches_wake(&w(obj(0), WRITABLE), target, READABLE));
        // 键不等 → false（对象选择门）。
        assert!(!matches_wake(&w(obj(1), READABLE), target, READABLE));
        // WAIT_ANY + 位相交 → true（哨兵命中任何对象）。
        assert!(matches_wake(&w(WaitKey::Any, READABLE), target, READABLE));
        // WAIT_ANY + 位不相交 → false。
        assert!(!matches_wake(&w(WaitKey::Any, WRITABLE), target, READABLE));
        // 纯睡 → 恒 false（wait_signals==0 与任何集不相交 + 键门双拒）。
        assert!(!matches_wake(&w(WaitKey::PureSleep, 0), target, READABLE));
        assert!(!matches_wake(&w(WaitKey::PureSleep, READABLE), target, READABLE));
        // 位集部分相交即命中（多Bit请求 vs 单Bit置位）。
        assert!(matches_wake(
            &w(obj(0), READABLE | WRITABLE | TERMINATED),
            target,
            WRITABLE
        ));
        // set_signals 多位同置：任一相交即命中。
        assert!(matches_wake(
            &w(obj(0), TERMINATED),
            target,
            READABLE | TERMINATED | WAKE_PENDING
        ));
    }

    // ---- T3. b-1 广播不变量：谓词无序无选择（核心钉板）----

    #[test]
    fn t3_broadcast_invariant_order_insensitive() {
        // 同一等待者集合，任意排列下唤醒集合（按身份）恒相等——b-1 判谳
        // 「唤醒面 = 纯广播」的形式化：谓词只看 (key, bits)，任何集合内
        // 排列不改变谁醒。公平/上限/到达序绝进不了选择器。
        let queue = [
            w(obj(0), READABLE),
            w(obj(1), READABLE),
            w(WaitKey::Any, TERMINATED),
            w(WaitKey::PureSleep, 0),
            w(obj(0), READABLE | WRITABLE),
            w(obj(2), WRITABLE),
        ];
        let base: Vec<(WaitKey, u64)> = wake_set(&queue, addr(0), READABLE)
            .into_iter()
            .map(|d| (d.key, d.wait_signals))
            .collect();
        // 手算期望：obj(0)×2（READABLE、READABLE|WRITABLE）+ 哨兵
        // （TERMINATED 与 READABLE 不相交 → 不命中）。等下——哨兵等
        // TERMINATED，置位 READABLE：不相交，不在集内。
        assert_eq!(base.len(), 2, "READABLE 置位醒 2 个 obj(0) 等待者");
        // 6 个排列点测（全 720 排列不做——身份集相等性由谓词逐等待者
        // 独立性保证，这里钉「集合输出序与输入序无关」的语义）。
        let perms: Vec<Vec<WaiterDesc>> = vec![
            vec![queue[0], queue[1], queue[2], queue[3], queue[4], queue[5]],
            vec![queue[5], queue[4], queue[3], queue[2], queue[1], queue[0]],
            vec![queue[2], queue[0], queue[4], queue[1], queue[5], queue[3]],
            vec![queue[4], queue[2], queue[5], queue[0], queue[3], queue[1]],
        ];
        for p in perms {
            let got: Vec<(WaitKey, u64)> = wake_set(&p, addr(0), READABLE)
                .into_iter()
                .map(|d| (d.key, d.wait_signals))
                .collect();
            let mut sorted_got: Vec<(bool, usize, u64)> =
                got.iter().map(|(k, s)| (matches!(k, WaitKey::Any), k.to_raw(), *s)).collect();
            sorted_got.sort();
            let mut sorted_base: Vec<(bool, usize, u64)> = base
                .iter()
                .map(|(k, s)| (matches!(k, WaitKey::Any), k.to_raw(), *s))
                .collect();
            sorted_base.sort();
            assert_eq!(sorted_got, sorted_base, "b-1：排列不改变唤醒集合");
        }
    }

    #[test]
    fn t3_broadcast_no_priority_or_cap_in_predicate() {
        // b-1 补钉：多匹配者全部命中（无「醒 1 个」上限路径）——send/recv
        // 容量配对绝不进谓词。8 匹配者全醒。
        let queue: Vec<WaiterDesc> = (0..8).map(|_| w(obj(0), READABLE)).collect();
        assert_eq!(wake_indices(&queue, addr(0), READABLE), (0..8).collect::<Vec<_>>());
        // 唤醒数 = 匹配数（wake_matching 返回值语义对齐）。
        assert_eq!(wake_set(&queue, addr(0), READABLE).len(), 8);
    }

    // ---- T4. 哨兵语义（wait_many，4-2b）----

    #[test]
    fn t4_sentinel_hits_any_object() {
        let queue = [
            w(WaitKey::Any, READABLE | TERMINATED), // wait_many 收割者
            w(obj(9), WRITABLE),
        ];
        // 任何对象、任何新置位 → 哨兵命中（醒来重验归调用方）。
        assert!(matches_wake(&queue[0], addr(1), TERMINATED));
        assert!(matches_wake(&queue[0], addr(9), READABLE));
        // 哨兵与纯广播的组合：TERMINATED 置位醒哨兵（obj(9) 等 WRITABLE
        // 不相交不醒）。
        assert_eq!(wake_indices(&queue, addr(9), TERMINATED), vec![0]);
        // READABLE 置位也醒哨兵（位相交）。
        assert_eq!(wake_indices(&queue, addr(9), READABLE), vec![0]);
        // WRITABLE 置位：哨兵 READABLE|TERMINATED 与 WRITABLE 不相交 →
        // 只醒 obj(9)。
        assert_eq!(wake_indices(&queue, addr(9), WRITABLE), vec![1]);
    }

    // ---- T5. fastpath 资格三律（single_waiter）----

    #[test]
    fn t5_single_waiter_laws() {
        // 律一：恰一个非哨兵匹配者 + 无相交哨兵 → Some(idx)。
        let q1 = [w(obj(0), READABLE), w(obj(1), READABLE)];
        assert_eq!(single_waiter(&q1, addr(0), READABLE), Some(0));
        assert_eq!(single_waiter(&q1, addr(1), READABLE), Some(1));

        // 律二：零匹配者 → None（退 slowpath；空唤醒集时 fastpath 与
        // slowpath 行为等价——sys_channel_send 只在有消息时走 fastpath
        // 检查链，此处钉谓词本身）。
        assert_eq!(single_waiter(&q1, addr(2), READABLE), None);

        // 律三：多匹配者 → None（单点唤醒集合 ≠ 全匹配集）。
        let q2 = [w(obj(0), READABLE), w(obj(0), WRITABLE), w(obj(0), READABLE)];
        assert_eq!(single_waiter(&q2, addr(0), READABLE), None);

        // 位相交哨兵在场 → None（唤醒集不可约——哨兵必须也被醒）。
        let q3 = [w(WaitKey::Any, TERMINATED), w(obj(0), READABLE)];
        assert_eq!(single_waiter(&q3, addr(0), READABLE), Some(1), "不相交哨兵不挡 fastpath");
        assert_eq!(single_waiter(&q3, addr(0), TERMINATED), None,
            "位相交哨兵（等 TERMINATED、置 TERMINATED）→ 退 slowpath");
        // 同一哨兵、置位 READABLE（不相交）→ 不挡。
        assert_eq!(single_waiter(&q3, addr(0), READABLE), Some(1));

        // b-2 LB 修正专钉：wait_many 收割者常驻挂 TERMINATED 位——
        // 普通 READABLE send 不被它挡（直切保留）。
        let harvester = [w(WaitKey::Any, TERMINATED), w(obj(0), READABLE)];
        assert_eq!(single_waiter(&harvester, addr(0), READABLE), Some(1));
        // 但 TERMINATED 置位（进程退出）必须退 slowpath 广播它。
        assert_eq!(single_waiter(&harvester, addr(0), TERMINATED), None);

        // 边界：obj==0 / signals==0 恒 None（sched 入口拒同形）。
        assert_eq!(single_waiter(&q1, 0, READABLE), None);
        assert_eq!(single_waiter(&q1, addr(0), 0), None);
        // 纯睡者在队：不影响 fastpath 资格（不在唤醒集）。
        let q4 = [w(WaitKey::PureSleep, 0), w(obj(0), READABLE)];
        assert_eq!(single_waiter(&q4, addr(0), READABLE), Some(1));
    }

    // ---- T6. fastpath/slowpath 语义对齐（两路一致性钉板）----

    #[test]
    fn t6_fastpath_slowpath_alignment() {
        // 对任意 (queue, obj, signals)：fastpath Some(i) ⇒ wake_set 恰为
        // {queue[i]}（单点唤醒集合 == 全匹配集——两路语义逐条对齐的
        // 形式化）；fastpath None ⇒ 慢路接管不丢唤醒（集合或空或 ≥2/含哨兵）。
        let queues: Vec<Vec<WaiterDesc>> = vec![
            vec![],
            vec![w(obj(0), READABLE)],
            vec![w(obj(0), READABLE), w(obj(0), WRITABLE)],
            vec![w(WaitKey::Any, READABLE), w(obj(0), READABLE)],
            vec![w(WaitKey::Any, TERMINATED), w(obj(1), WRITABLE)],
            vec![w(obj(0), READABLE), w(obj(1), READABLE | WRITABLE), w(WaitKey::PureSleep, 0)],
            vec![w(WaitKey::Any, READABLE | WRITABLE), w(obj(0), WRITABLE)],
        ];
        let objs = [0usize, addr(0), addr(1), 42];
        let sigs = [0u64, READABLE, WRITABLE, READABLE | TERMINATED];
        for q in &queues {
            for &o in &objs {
                for &s in &sigs {
                    let full: Vec<usize> = wake_indices(q, o, s);
                    match single_waiter(q, o, s) {
                        Some(i) => {
                            // 哨兵相交者必不在 full（None 支才允许哨兵）。
                            assert!(!q.iter().any(|x| x.key == WaitKey::Any && (x.wait_signals & s) != 0),
                                "fastpath Some 与相交哨兵共存 = 违约");
                            assert_eq!(full, vec![i],
                                "fastpath 单点 ({o:#x},{s:#b}) 须与全匹配集一致：{full:?}");
                        }
                        None => {
                            // None 的三类合法因由（至少一真）：
                            //  a) obj/signals 零边界
                            //  b) 全匹配集 ≠ 单点（0/≥2/含哨兵）
                            //  c) fastpath 保守退化（位相交哨兵存在但全
                            //     匹配集恰单点——b-2 修正允许：哨兵也在
                            //     广播集内，slowpath 广播它，语义无损）。
                            let boundary = o == 0 || s == 0;
                            let sentinel_block = q
                                .iter()
                                .any(|x| x.key == WaitKey::Any && (x.wait_signals & s) != 0);
                            let multi_or_zero = full.len() != 1;
                            assert!(
                                boundary || multi_or_zero || sentinel_block,
                                "None 无合法因由：full={full:?} o={o:#x} s={s:#b}"
                            );
                        }
                    }
                }
                // 非边界下 fastpath 命中 ⇒ 唤醒数恰 1。
            }
        }
    }

    // ---- T7. FIFO 回滚序（push_front 保律的谓词面投影）----

    #[test]
    fn t7_fifo_projection() {
        // channel 收件队列 FIFO + push_front 回滚保序是 ChannelEnd 的
        // 机械（Vec 面，kernel/src/channel.rs）；本面钉其与唤醒的**组合
        // 不变量**：recv 失败回滚后，同一 (obj, signals) 的唤醒集合与
        // 回滚前逐位一致（消息序不进唤醒判定）。
        let waiters = [w(obj(0), READABLE), w(obj(0), READABLE)];
        // 「回滚」在纯面上 = 消息集合恒等——唤醒集合是等待者的函数，
        // 与队列内容无关（谓词不读消息队列）。同输入恒同输出：
        for _ in 0..3 {
            assert_eq!(wake_indices(&waiters, addr(0), READABLE), vec![0, 1]);
        }
    }

    // ---- T8. 死角扫描：全信号位域 × 全键形 ----

    #[test]
    fn t8_exhaustive_bits() {
        // 低 4 位（READABLE/WRITABLE/TERMINATED/WAKE_PENDING）全组合 ×
        // 三键形 × 请求/置位全对打——无 panic、无意外命中。
        let keys = [WaitKey::PureSleep, WaitKey::Obj(7), WaitKey::Any];
        for kw in keys {
            for req in 0u64..16 {
                for set in 0u64..16 {
                    let d = w(kw, req);
                    let hit = matches_wake(&d, 7, set);
                    // 期望独立推导：键门 + 位门。
                    let key_ok = !matches!(kw, WaitKey::PureSleep);
                    let expect = key_ok && (req & set) != 0;
                    assert_eq!(hit, expect,
                        "key={kw:?} req={req:#b} set={set:#b}");
                }
            }
        }
    }
}
