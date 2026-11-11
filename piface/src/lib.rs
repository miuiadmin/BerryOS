//! berry-piface —— 优先级继承（PI）捐赠图纯函数面（W3-Q5）。
//!
//! **动机**：v1 PI（`IPC设计.md` §2.1「QNX 发送者语义」的全图重推形）此前
//! 只活在 `kernel/src/pi.rs` 的 raw 指针 SLEEPQ 扫描里——host 测不可达
//! （kernel 依赖 HAL global_asm，只 aarch64-unknown-none 可编），ktest 只
//! 覆盖 boot 相。PI 语义的回归面（donor 资格三拒、peer/right 解锁配对律、
//! 链式传播/轮帽、持有环单调有界）需要一张 host 可测的**钉板**：本 crate
//! 把捐赠图按值域抽成纯函数 + 表驱动测试；`kernel/src/pi.rs` 的
//! [`donor_edge`] 资格判**消费同一函数**（零第二实现——抽离即唯一事实源，
//! 非镜像），传播循环因零分配纪律留在 kernel（IRQ 屏蔽热路径不可建快照
//! Vec——本面以同构钉板钉其语义，见 [`propagate`] 注释）。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改内核行为）：
//! - [`donor_edge`]：donor 资格判——等 channel 派生位的单一对象等待者 →
//!   `(peer 端点, 解锁所需 right 集)`。三拒：纯睡 / WAIT_ANY 哨兵 / 只等
//!   用户位；非 channel 对象（Event 等，无 peer 结构）拒；peer 缺失拒。
//! - [`PI_MAX_HOPS`]：阻塞链深上限（§2.1「链式传播有界，如 8 层」）——
//!   不动点迭代轮帽，到帽按已达 boost 处理（不 panic，syscall 边界纪律）。
//!   kernel 侧 re-export 本常量（单源）。
//! - [`HolderFace`]：受捐判定的持有面抽象（值域表驱动：`(对象, rights)`
//!   槽集）——kernel 侧 `HandleTable::holder_has` 指针反查的纯投影。
//! - [`propagate`]：不动点传播核心的值域钉板（每轮沿边一跳、同轮实时
//!   可见、环单调有界）——语义对拍面，非 kernel 消费面（零分配纪律）。
//!
//! 值域常量（SIGNAL_/RIGHT_）是 `tools/abi.toml` ABI §8.2 冻结值的镜像
//! 声明（本 crate 不依赖 kernel——依赖方向单向 kernel → piface），测试
//! [`test_abi_mirror_frozen_values`] 钉死字面值；改值先改 abi.toml。
//!
//! 与冻结面的关系：syscall ABI 零触碰（纯内核内部语义）；SLEEPQ 物理结构、
//! KL/SQ 锁窗、retain 钉住纪律不在此 crate——本面只钉「谁捐给谁、捐多少、
//! 何时停」的判定律，不钉锁窗机械。

#![cfg_attr(not(test), no_std)]
#![allow(clippy::result_unit_err)]

// host 测试主消费面用 Vec（同 berry-wqmatch 形态）；kernel 消费面
// （donor_edge/PI_MAX_HOPS）零分配不触 Vec。
#[cfg(not(test))]
extern crate alloc;
#[cfg(not(test))]
use alloc::vec::Vec;

use berry_wqmatch::WaitKey;

// ---- ABI §8.2 冻结值镜像（kernel/src/abi_gen.rs 同值；测试钉死）----
pub const SIGNAL_READABLE: u64 = 1 << 0;
pub const SIGNAL_WRITABLE: u64 = 1 << 1;
pub const RIGHT_SEND: u64 = 1 << 8;
pub const RIGHT_RECV: u64 = 1 << 9;

/// 阻塞链深上限（§2.1「链式传播有界，如 8 层」）：不动点迭代轮帽——每轮沿
/// 捐赠边传播一跳，轮数即跳数；到帽按已达 boost 处理（**不 panic**，syscall
/// 边界纪律）。持有环在 max 语义下单调有界不发散，轮帽兜底。
///
/// W3-Q5 起单源在此（kernel/src/pi.rs re-export；值不变 = 8）。
pub const PI_MAX_HOPS: usize = 8;

/// 对象标识（值域）：kernel 侧为 ObjRef 指针地址；本面只做相等比较，语义
/// 由构造方保证。
pub type ObjId = usize;
/// rights 位集（值域；kernel 侧 `Rights = u64` 同型）。
pub type Rights = u64;

/// 等待对象的类型（值域）：v1 只有 channel 端点构成捐赠边（Event 无 peer
/// 结构、signal 方多方——v2 再议）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjKind {
    /// channel 端点（有 peer 结构——候选捐赠边）。
    Channel,
    /// Event 等无 peer 对象（kernel 侧归并为本形/`Other` 均拒）。
    Event,
    /// 其余对象（线程键等）。
    Other,
}

/// donor 的捐赠边（纯值域形；kernel/src/pi.rs 消费的单一事实源）：
/// 等 channel 派生位的单一对象等待者 → `(peer 端点, 解锁所需 right 集)`。
///
/// 拒面（五拒，与 kernel 侧逐字对齐）：
/// 1. 纯睡（`wait_obj == 0`）→ 无解锁语义，只能超时自醒；
/// 2. WAIT_ANY 哨兵 → 无单一键（v2 收口）；
/// 3. 只等用户位（`wait_signals` 与 READABLE|WRITABLE 不交）→ channel 上
///    永不置用户位（4-2a）；
/// 4. 非 channel 对象（Event 等）→ 无 peer 结构，v1 不构成边；
/// 5. channel 但 peer 缺失（对端已 close 的极端态）→ 无受捐面。
///
/// 配对律：等 WRITABLE（对端满，等腾位）→ 解锁动作 = 有人 **recv peer**
/// （RIGHT_RECV）；等 READABLE（本端空，等消息）→ **send peer**
/// （RIGHT_SEND）。两位都等 → 并集（任一动作可解锁——持有面「含任一位」
/// 语义）。
pub fn donor_edge(
    key: WaitKey,
    wait_signals: u64,
    obj_kind: ObjKind,
    peer: Option<ObjId>,
) -> Option<(ObjId, Rights)> {
    let WaitKey::Obj(_) = key else {
        return None; // 纯睡 / WAIT_ANY：无单一对象键（拒 1/2）
    };
    let derived = wait_signals & (SIGNAL_READABLE | SIGNAL_WRITABLE);
    if derived == 0 {
        return None; // 只等用户位（拒 3）
    }
    if obj_kind != ObjKind::Channel {
        return None; // Event 等：无 peer 结构（拒 4）
    }
    let peer = peer?; // peer 缺失（拒 5）
    let mut need: Rights = 0;
    if derived & SIGNAL_WRITABLE != 0 {
        need |= RIGHT_RECV; // 等 WRITABLE → recv peer 腾位
    }
    if derived & SIGNAL_READABLE != 0 {
        need |= RIGHT_SEND; // 等 READABLE → send peer 灌入
    }
    Some((peer, need))
}

/// 受捐判定的持有面（值域表驱动）：一任务 handle 表的 `(对象, rights)` 投影。
///
/// kernel 侧投影 = `HandleTable::holder_has(obj, any)`（ObjRef 指针相等 +
/// rights 任一位相交）；本面把「持有面」钉成纯数据——构造时同对象别名槽
/// rights 并集（表内多槽共对象的语义抹平），查询与 holder_has 逐字同义：
/// 存在槽 `(o, r)` 使 `o == obj && r & any != 0`；`any == 0` 先拒。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HolderFace {
    slots: Vec<(ObjId, Rights)>,
}

impl HolderFace {
    /// 空持有面。
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// 从 `(对象, rights)` 对构造（同对象别名并集成一槽）。
    pub fn from_pairs(pairs: &[(ObjId, Rights)]) -> Self {
        let mut slots = Vec::new();
        for (obj, rights) in pairs {
            match slots.iter_mut().find(|(o, _)| o == obj) {
                Some((_, r)) => *r |= rights, // 别名槽并集
                None => slots.push((*obj, *rights)),
            }
        }
        Self { slots }
    }

    /// 持有 `obj` 的 rights 并集；未持有 → `None`。
    pub fn get(&self, obj: ObjId) -> Option<Rights> {
        self.slots
            .iter()
            .filter(|(o, _)| *o == obj)
            .fold(None, |acc, (_, r)| Some(acc.unwrap_or(0) | r))
    }

    /// 是否持有 `obj` 且 rights 与 `any` 任一位相交（kernel 侧
    /// `HandleTable::holder_has` 的纯投影，逐字同义）。
    pub fn holder_has(&self, obj: ObjId, any: Rights) -> bool {
        if any == 0 {
            return false; // 无需求位先拒（holder_has 同款守卫）
        }
        self.get(obj).map_or(false, |r| r & any != 0)
    }
}

/// 传播输入：sleeper 的值域快照（下标即身份——kernel 侧对应 SLEEPQ 在队者）。
#[derive(Debug, Clone)]
pub struct Sleeper {
    pub key: WaitKey,
    pub wait_signals: u64,
    pub obj_kind: ObjKind,
    pub peer: Option<ObjId>,
    /// 尾趟才刷的有效优先级快照（`prio_base + 策略权重`——kernel 侧
    /// `Task::prio_effective`）。
    pub prio_effective: i32,
    /// 受捐额快照（kernel 侧重推第一趟清零在队者后才进传播——冻结语义：
    /// 输入不清零，已离队 runnable 者根本不在 sleepers 内）。
    pub pi_inherited: i32,
}

/// 不动点传播核心（值域钉板）：`sleepers`/`holders` 下标一一对应；返回各
/// sleeper 终态 `pi_inherited`。
///
/// **语义对拍面，非 kernel 消费面**：kernel 的 `recompute_locked` 中段在
/// IRQ 屏蔽热路径上跑（零分配纪律——不可建快照 Vec），循环体留在
/// kernel/src/pi.rs 并逐字对齐本函数（同轮实时可见/轮帽/环有界三律在这
/// 里钉板；改本函数语义 = 改内核行为，须双侧同步）。
///
/// 三律（kernel 侧注释的值域化）：
/// 1. **同轮实时可见**：donor 有效值 = `max(prio_effective, pi_inherited)`
///    实时读——上游本轮更新，同轮/下轮下游立即可见（只读 prio_effective
///    会把传播截死在一跳）；
/// 2. **每轮一跳、轮帽兜底**：`PI_MAX_HOPS` 轮后仍有变化即按已达 boost
///    处理返回（不 panic）；
/// 3. **不动点早停**：一轮无变化即停。
pub fn propagate(sleepers: &[Sleeper], holders: &[HolderFace]) -> Vec<i32> {
    assert_eq!(sleepers.len(), holders.len(), "下标一一对应契约");
    let mut pi: Vec<i32> = sleepers.iter().map(|s| s.pi_inherited).collect();
    for _ in 0..PI_MAX_HOPS {
        let mut changed = false;
        for (di, donor) in sleepers.iter().enumerate() {
            let Some((peer, need)) =
                donor_edge(donor.key, donor.wait_signals, donor.obj_kind, donor.peer)
            else {
                continue;
            };
            // 三律之 1：donor 有效值实时读（max(尾趟快照, 本轮实时 pi)）。
            let d_eff = donor.prio_effective.max(pi[di]);
            for (ci, _) in sleepers.iter().enumerate() {
                // kernel 侧不特判 cand == donor（真实拓扑下等待者不可能持
                // 有解锁自身的 peer——值域面同构不添判）。
                if holders[ci].holder_has(peer, need) && d_eff > pi[ci] {
                    pi[ci] = d_eff;
                    changed = true;
                }
            }
        }
        if !changed {
            break; // 三律之 3：不动点
        }
    }
    pi // 三律之 2：轮帽外按已达值返回
}

#[cfg(test)]
mod tests {
    use super::*;
    use berry_wqmatch::WAIT_ANY;

    // ---- donor_edge：五拒 + 配对律 ----

    #[test]
    fn test_donor_edge_pure_sleep_rejected() {
        // 拒 1：纯睡（wait_obj == 0）只能超时自醒，无解锁语义。
        assert_eq!(
            donor_edge(WaitKey::PureSleep, SIGNAL_WRITABLE, ObjKind::Channel, Some(9)),
            None
        );
    }

    #[test]
    fn test_donor_edge_wait_any_rejected() {
        // 拒 2：WAIT_ANY 哨兵无单一键（v2 收口）。
        assert_eq!(
            donor_edge(WaitKey::Any, SIGNAL_READABLE, ObjKind::Channel, Some(9)),
            None
        );
    }

    #[test]
    fn test_donor_edge_user_signals_only_rejected() {
        // 拒 3：只等用户位（与派生位不交）——channel 永不置用户位（4-2a）。
        // 用户位域任一位（含与 TERMINATED 等其它位混合但不交派生位）皆拒。
        assert_eq!(
            donor_edge(WaitKey::Obj(7), 1 << 20, ObjKind::Channel, Some(9)),
            None
        );
        assert_eq!(
            donor_edge(
                WaitKey::Obj(7),
                (1 << 20) | (1 << 21),
                ObjKind::Channel,
                Some(9)
            ),
            None
        );
    }

    #[test]
    fn test_donor_edge_non_channel_rejected() {
        // 拒 4：Event / 其它对象无 peer 结构，v1 不构成捐赠边。
        assert_eq!(
            donor_edge(WaitKey::Obj(7), SIGNAL_READABLE, ObjKind::Event, Some(9)),
            None
        );
        assert_eq!(
            donor_edge(WaitKey::Obj(7), SIGNAL_WRITABLE, ObjKind::Other, Some(9)),
            None
        );
    }

    #[test]
    fn test_donor_edge_no_peer_rejected() {
        // 拒 5：channel 但 peer 缺失（对端已 close 的极端态）。
        assert_eq!(
            donor_edge(WaitKey::Obj(7), SIGNAL_READABLE, ObjKind::Channel, None),
            None
        );
    }

    #[test]
    fn test_donor_edge_pairing_law() {
        // 配对律：等 WRITABLE → RECV peer（recv 腾位）；等 READABLE →
        // SEND peer（send 灌入）。
        let e = donor_edge(WaitKey::Obj(7), SIGNAL_WRITABLE, ObjKind::Channel, Some(9));
        assert_eq!(e, Some((9, RIGHT_RECV)));
        let e = donor_edge(WaitKey::Obj(7), SIGNAL_READABLE, ObjKind::Channel, Some(9));
        assert_eq!(e, Some((9, RIGHT_SEND)));
    }

    #[test]
    fn test_donor_edge_both_bits_union() {
        // 双派生位都等 → RECV|SEND 并集（任一动作可解锁——「含任一位」语义）。
        let e = donor_edge(
            WaitKey::Obj(7),
            SIGNAL_READABLE | SIGNAL_WRITABLE,
            ObjKind::Channel,
            Some(9),
        );
        assert_eq!(e, Some((9, RIGHT_RECV | RIGHT_SEND)));
    }

    #[test]
    fn test_donor_edge_user_plus_derived_passes() {
        // 派生位 + 用户位混合：派生位相交即过（掩码语义不是「仅派生位」）。
        let e = donor_edge(
            WaitKey::Obj(7),
            SIGNAL_READABLE | (1 << 20),
            ObjKind::Channel,
            Some(9),
        );
        assert_eq!(e, Some((9, RIGHT_SEND)));
    }

    #[test]
    fn test_abi_mirror_frozen_values() {
        // ABI §8.2 冻结值镜像钉死（与 kernel/src/abi_gen.rs / tools/abi.toml
        // 一致；改值先改 abi.toml 再双侧同步）。
        assert_eq!(SIGNAL_READABLE, 1 << 0);
        assert_eq!(SIGNAL_WRITABLE, 1 << 1);
        assert_eq!(RIGHT_SEND, 1 << 8);
        assert_eq!(RIGHT_RECV, 1 << 9);
        assert_eq!(PI_MAX_HOPS, 8);
    }

    // ---- HolderFace：持有面纯投影 ----

    #[test]
    fn test_holder_face_alias_union() {
        // 同对象别名槽（多 handle 共对象）rights 并集。
        let ht = HolderFace::from_pairs(&[(5, RIGHT_SEND), (5, RIGHT_RECV)]);
        assert_eq!(ht.get(5), Some(RIGHT_SEND | RIGHT_RECV));
        assert_eq!(ht.get(6), None);
        // 并集后任一位相交即持。
        assert!(ht.holder_has(5, RIGHT_RECV));
        assert!(ht.holder_has(5, RIGHT_SEND));
    }

    #[test]
    fn test_holder_face_any_bit_semantics() {
        // 「含任一位」语义：rights 与 any 相交即持（holder_has 逐字同义）。
        let ht = HolderFace::from_pairs(&[(5, RIGHT_SEND)]);
        assert!(ht.holder_has(5, RIGHT_SEND));
        assert!(ht.holder_has(5, RIGHT_SEND | RIGHT_RECV));
        assert!(!ht.holder_has(5, RIGHT_RECV));
        assert!(!ht.holder_has(6, RIGHT_SEND)); // 未持有
        assert!(!ht.holder_has(5, 0)); // any == 0 先拒
    }

    #[test]
    fn test_holder_face_empty() {
        let ht = HolderFace::new();
        assert!(!ht.holder_has(5, RIGHT_SEND));
        assert_eq!(ht.get(5), None);
    }

    // ---- propagate：传播三律 ----

    /// 构链工具：`waiter`（高优，等 `end` 满）→ `holder`（持 `peer` 端点
    /// 带 `right`）的经典反转二元组。
    fn inversion_pair(end: ObjId, peer: ObjId, right: Rights, hi: i32, lo: i32) -> (Vec<Sleeper>, Vec<HolderFace>) {
        let sleepers = vec![
            Sleeper {
                key: WaitKey::Obj(end),
                wait_signals: SIGNAL_WRITABLE, // 对端满 → 等 RECV peer
                obj_kind: ObjKind::Channel,
                peer: Some(peer),
                prio_effective: hi,
                pi_inherited: 0,
            },
            Sleeper {
                key: WaitKey::PureSleep,
                wait_signals: 0,
                obj_kind: ObjKind::Other,
                peer: None,
                prio_effective: lo,
                pi_inherited: 0,
            },
        ];
        let holders = vec![HolderFace::new(), HolderFace::from_pairs(&[(peer, right)])];
        (sleepers, holders)
    }

    #[test]
    fn test_propagate_single_hop_inversion() {
        // 经典反转（Mars Pathfinder 形）：高优 C 等 channel 满对端、低优
        // server P 持 peer 的 RECV → C 捐给 P（单跳 pi = C 的有效值）。
        let (sleepers, holders) = inversion_pair(7, 9, RIGHT_RECV, 9, 3);
        let out = propagate(&sleepers, &holders);
        assert_eq!(out, vec![0, 9]);
    }

    #[test]
    fn test_propagate_no_edge_no_donation() {
        // 持有面无解锁 right（只有 SEND 不含 RECV）→ 不构成边，零传播。
        let (sleepers, holders) = inversion_pair(7, 9, RIGHT_SEND, 9, 3);
        let out = propagate(&sleepers, &holders);
        assert_eq!(out, vec![0, 0]);
    }

    #[test]
    fn test_propagate_chain_two_hops() {
        // 链式两跳：C（9）等 e2 满 → B 持 e2-peer(RECV) 且 B 等 e1 满 →
        // A 持 e1-peer(RECV)。C→B→A 两跳，B/A 终态 = 9。
        let e1 = 0x11;
        let e1_peer = 0x21;
        let e2 = 0x12;
        let e2_peer = 0x22;
        let sleepers = vec![
            // 0: C——等 e2 满（对端 = e2_peer）
            Sleeper {
                key: WaitKey::Obj(e2),
                wait_signals: SIGNAL_WRITABLE,
                obj_kind: ObjKind::Channel,
                peer: Some(e2_peer),
                prio_effective: 9,
                pi_inherited: 0,
            },
            // 1: B——持 e2_peer(RECV)、自身等 e1 满（对端 = e1_peer）
            Sleeper {
                key: WaitKey::Obj(e1),
                wait_signals: SIGNAL_WRITABLE,
                obj_kind: ObjKind::Channel,
                peer: Some(e1_peer),
                prio_effective: 5,
                pi_inherited: 0,
            },
            // 2: A——持 e1_peer(RECV)，纯睡
            Sleeper {
                key: WaitKey::PureSleep,
                wait_signals: 0,
                obj_kind: ObjKind::Other,
                peer: None,
                prio_effective: 3,
                pi_inherited: 0,
            },
        ];
        let holders = vec![
            HolderFace::new(),
            HolderFace::from_pairs(&[(e2_peer, RIGHT_RECV)]),
            HolderFace::from_pairs(&[(e1_peer, RIGHT_RECV)]),
        ];
        let out = propagate(&sleepers, &holders);
        assert_eq!(out, vec![0, 9, 9]); // C→B→A 两跳全达
    }

    #[test]
    fn test_propagate_same_round_visibility() {
        // 三律之 1（同轮实时可见）：上表中即使把 C 排在队首、下游在同轮
        // 后扫，一轮回合内链尾已达标——「每轮一跳」是按扫描序的下界语义，
        // 同轮下游立即可见是命门（截死一跳 = 只看 prio_effective 的错）。
        // 链两跳（上测拓扑）单轮即达 ⇒ 与轮帽语义（下测）联合钉死。
        let (mut sleepers, holders) = {
            let (s, h) = test_propagate_chain_two_hops_inputs();
            (s, h)
        };
        sleepers[0].pi_inherited = 0;
        let out = propagate(&sleepers, &holders);
        assert_eq!(out, vec![0, 9, 9]);
    }

    fn test_propagate_chain_two_hops_inputs() -> (Vec<Sleeper>, Vec<HolderFace>) {
        let e1 = 0x11;
        let e1_peer = 0x21;
        let e2 = 0x12;
        let e2_peer = 0x22;
        let sleepers = vec![
            Sleeper {
                key: WaitKey::Obj(e2),
                wait_signals: SIGNAL_WRITABLE,
                obj_kind: ObjKind::Channel,
                peer: Some(e2_peer),
                prio_effective: 9,
                pi_inherited: 0,
            },
            Sleeper {
                key: WaitKey::Obj(e1),
                wait_signals: SIGNAL_WRITABLE,
                obj_kind: ObjKind::Channel,
                peer: Some(e1_peer),
                prio_effective: 5,
                pi_inherited: 0,
            },
            Sleeper {
                key: WaitKey::PureSleep,
                wait_signals: 0,
                obj_kind: ObjKind::Other,
                peer: None,
                prio_effective: 3,
                pi_inherited: 0,
            },
        ];
        let holders = vec![
            HolderFace::new(),
            HolderFace::from_pairs(&[(e2_peer, RIGHT_RECV)]),
            HolderFace::from_pairs(&[(e1_peer, RIGHT_RECV)]),
        ];
        (sleepers, holders)
    }

    #[test]
    fn test_propagate_hops_cap_bounded_no_panic() {
        // 三律之 2（轮帽）：长链（源 H prio 9 在队尾，L0..L8 逐级持有
        // 下游 peer）最坏扫描序下每轮至多推进一跳——9 跳需求 > 轮帽 8 ⇒
        // 链尾 L8 按已达值（0）返回，其余全达 9；不 panic（syscall 边界
        // 纪律）。基数全 0 → 传播值只携带源的 9，断言面纯净。
        let n = 10usize; // 索引 0..8 = L8..L0（反序），索引 9 = H（源）
        let mut sleepers = Vec::new();
        let mut holders = vec![HolderFace::new(); n];
        // L_i（i=0..8）等 end_i 满；peer(end_i) 由 L_{i+1} 持有（L8 的
        // 持有者在集外——链终于此）。H 纯睡（源不是 donor，只被 L0 的
        // 边指向？否——H 等自己的 end_H，peer(end_H) 由 L0 持有）。
        let end = |i: usize| 0x100 + i;
        let peer_of = |i: usize| 0x200 + i;
        // L8..L0 反序入队（索引 0 = L8 最先扫、H 最后扫 = 最坏序）。
        for i in (0..9).rev() {
            sleepers.push(Sleeper {
                key: WaitKey::Obj(end(i)),
                wait_signals: SIGNAL_WRITABLE,
                obj_kind: ObjKind::Channel,
                peer: Some(peer_of(i)),
                prio_effective: 0,
                pi_inherited: 0,
            });
        }
        // H：等 end_H 满，prio 9；peer(end_H) = peer_of(9) 由 L0 持有。
        sleepers.push(Sleeper {
            key: WaitKey::Obj(end(9)),
            wait_signals: SIGNAL_WRITABLE,
            obj_kind: ObjKind::Channel,
            peer: Some(peer_of(9)),
            prio_effective: 9,
            pi_inherited: 0,
        });
        // 持有面：L_{i+1} 持 peer_of(i)（i=0..7 ⇒ L1..L8；反序数组中
        // L_i 的下标 = 8 - i）；L0（下标 8）持 peer_of(9)（H 的 peer）。
        for i in 0..8 {
            let holder_idx = 8 - (i + 1); // L_{i+1} 的下标（i≤7 → ≥0）
            holders[holder_idx] = HolderFace::from_pairs(&[(peer_of(i), RIGHT_RECV)]);
        }
        holders[8] = HolderFace::from_pairs(&[(peer_of(9), RIGHT_RECV)]); // L0 持 H 的 peer
        let out = propagate(&sleepers, &holders);
        // 不 panic + 单调有界：全员 ≤ 9。轮帽 8 内可达 L0..L7（下标 8..1），
        // 链尾 L8（下标 0）差一跳 → 保持 0（「到帽按已达 boost 处理」）。
        assert!(out.iter().all(|&v| v <= 9));
        assert_eq!(out[9], 0); // H：无人向其捐赠（集外持有者），且基数 0
        assert_eq!(out[8], 9); // L0：首跳（H 在同轮最后扫到，轮 1 即达）
        assert_eq!(out[1], 9); // L7：第 8 轮达（帽内最后一跳）
        assert_eq!(out[0], 0); // L8：需第 9 轮 > 帽 → 按已达值（0）
    }

    #[test]
    fn test_propagate_same_round_visibility_long_chain() {
        // 三律之 1（同轮实时可见）的极端对照：同上长链，但 H 排在队首
        // （最优扫描序）——H→L0 后 L0 在同轮稍后被扫，d_eff 立即含 9，
        // 一轮内整链到尾（对照上测：最坏序需 9 轮、帽内差一跳）。
        let n = 10usize; // 索引 0 = H，索引 1..9 = L0..L8（正序）
        let end = |i: usize| 0x100 + i;
        let peer_of = |i: usize| 0x200 + i;
        let mut sleepers = vec![Sleeper {
            key: WaitKey::Obj(end(9)),
            wait_signals: SIGNAL_WRITABLE,
            obj_kind: ObjKind::Channel,
            peer: Some(peer_of(9)),
            prio_effective: 9,
            pi_inherited: 0,
        }];
        for i in 0..9 {
            sleepers.push(Sleeper {
                key: WaitKey::Obj(end(i)),
                wait_signals: SIGNAL_WRITABLE,
                obj_kind: ObjKind::Channel,
                peer: Some(peer_of(i)),
                prio_effective: 0,
                pi_inherited: 0,
            });
        }
        let mut holders = vec![HolderFace::new(); n];
        holders[1] = HolderFace::from_pairs(&[(peer_of(9), RIGHT_RECV)]); // L0 持 H 的 peer
        for i in 0..8 {
            holders[i + 2] = HolderFace::from_pairs(&[(peer_of(i), RIGHT_RECV)]); // L_{i+1}
        }
        let out = propagate(&sleepers, &holders);
        // 一轮全达（H 首扫、链上各环同轮续传）——「每轮一跳」是按扫描序
        // 的下界，同轮下游立即可见是命门。
        assert_eq!(out, vec![0, 9, 9, 9, 9, 9, 9, 9, 9, 9]);
    }

    #[test]
    fn test_propagate_ring_monotone_bounded() {
        // 持有环（A 持 e1 睡 e2、B 持 e2 睡 e1）：max 语义下单调不发散，
        // 不动点 = 双方都抬到环上最大有效值；轮帽兜底不 panic。
        let e1 = 0x31;
        let e2 = 0x32;
        let e1_peer = 0x41;
        let e2_peer = 0x42;
        let sleepers = vec![
            // A：持 e1_peer(RECV)、等 e2 满（peer = e2_peer 由 B 持）
            Sleeper {
                key: WaitKey::Obj(e2),
                wait_signals: SIGNAL_WRITABLE,
                obj_kind: ObjKind::Channel,
                peer: Some(e2_peer),
                prio_effective: 7,
                pi_inherited: 0,
            },
            // B：持 e2_peer(RECV)、等 e1 满（peer = e1_peer 由 A 持）
            Sleeper {
                key: WaitKey::Obj(e1),
                wait_signals: SIGNAL_WRITABLE,
                obj_kind: ObjKind::Channel,
                peer: Some(e1_peer),
                prio_effective: 4,
                pi_inherited: 0,
            },
        ];
        let holders = vec![
            HolderFace::from_pairs(&[(e1_peer, RIGHT_RECV)]),
            HolderFace::from_pairs(&[(e2_peer, RIGHT_RECV)]),
        ];
        let out = propagate(&sleepers, &holders);
        // 环上 max = 7 且双向可达：A(7)→B 边 + B(受捐 7 后 d_eff=7)→A 边
        // （A 原 7 不小于 7 → A 保持 7，输出 pi=7）；max 单调有界不发散。
        assert_eq!(out, vec![7, 7]);
        assert!(out.iter().all(|&v| v <= 7));
    }

    #[test]
    fn test_propagate_freeze_snapshot_not_reset() {
        // 冻结语义钉板：propagate 不清零输入快照（kernel 侧重推第一趟清
        // 在队者后才进传播；已离队 runnable 者不在 sleepers 内）——预置
        // 受捐额 5 的高优 donor 把下游抬到 max(5, d_eff) 而非覆盖。
        let (mut sleepers, holders) = inversion_pair(7, 9, RIGHT_RECV, 6, 3);
        sleepers[1].pi_inherited = 5; // 预置冻结值（< 即将到来的 6）
        let out = propagate(&sleepers, &holders);
        assert_eq!(out, vec![0, 6]); // 6 > 5 抬升；若 5 > d_eff 则保持 5
        sleepers[0].prio_effective = 4; // d_eff = 4 < 预置 5
        let out = propagate(&sleepers, &holders);
        assert_eq!(out, vec![0, 5]); // 不回落——单调 max，无重置
    }

    #[test]
    fn test_propagate_empty_and_no_donors() {
        // 空面 / 全拒面：零传播、返回快照、不动点即刻。
        assert_eq!(propagate(&[], &[]), Vec::<i32>::new());
        let sleepers = vec![Sleeper {
            key: WaitKey::PureSleep,
            wait_signals: 0,
            obj_kind: ObjKind::Other,
            peer: None,
            prio_effective: 9,
            pi_inherited: 0,
        }];
        let holders = vec![HolderFace::from_pairs(&[(1, RIGHT_RECV)])];
        assert_eq!(propagate(&sleepers, &holders), vec![0]);
    }

    #[test]
    fn test_wait_key_domain_reuse() {
        // 值域复用钉板：WaitKey 单源在 berry_wqmatch（W3-Q2）——三形与
        // WAIT_ANY 哨兵值经 re-export 恒一致（防两 crate 值域漂移）。
        assert_eq!(WaitKey::from_raw(0), WaitKey::PureSleep);
        assert_eq!(WaitKey::from_raw(WAIT_ANY), WaitKey::Any);
        assert_eq!(WaitKey::from_raw(0xdeadbeef), WaitKey::Obj(0xdeadbeef));
    }
}
