//! AI 策略注入**留桩**（3-4c；`AI调度设计.md` §2/§9、`路线图.md` Phase 3 出口增项）。
//!
//! v1/v2 = **接口在位、表恒空**：决策路径（[`crate::class::RoundRobin::pick_next`]
//! 出选后经 [`apply`]）查表——未绑/空表 → 零写入，与「接口缺席」逐位等价
//! （`AI调度设计.md` §2「AI 不在线时表为空/默认，等价于纯启发式」；等价性由
//! ktest `POLICY ok` 断言，非口号）。真策略源（用户态 AI 经共享环写表、gen
//! 世代计数、事务化**原子换表快照**）随 v3 `policy_bind`（0x83）落地——本模块
//! 只埋词汇表 + 查表钩子，不实现换表协议。
//!
//! 动作空间四类封顶（§2 战略裁决）：①放置 hint；②频率/热限 hint（**走 PM
//! 服务**、不经调度路径——不在表里）；③包络内权重微调；④EPP 二值。**没有
//! 第五类**（尤其没有「直接改 RT 优先级/创建/杀任务」）。
//!
//! 单核顺序访问纪律（同 `sched.rs` 计数器）：[`apply`] 只读表（IRQ 屏蔽的
//! 决策路径调用）；`bind`/`unbind` 写点仅在 IRQ 屏蔽的 boot/ktest 期。

use crate::task::{EppHint, Task};

/// 权重微调包络（±）：注入值由内核**夹紧**到本界内（动作空间③的 min/max
/// 夹紧——包络是内核执法，不信任注入方自律）。
pub const WEIGHT_MAX: i32 = 64;

/// 单条策略指令（v1 词汇表：③权重 + ④EPP + ①放置记录）。
#[derive(Debug, Clone, Copy)]
pub struct Directive {
    /// ① 放置 hint（目标核/簇；0 = 无意见）。v1 单核不消费，仅落字段。
    pub placement: u8,
    /// ③ 包络内权重微调（应用时夹紧 ±[`WEIGHT_MAX`]，写入 `prio_effective`）。
    pub weight: i32,
    /// ④ EPP 二值（`None` = 不覆盖）。
    pub epp: Option<EppHint>,
}

/// 表条目：`task_key` 命中的任务应用 `directive`。
///
/// v1 key = [`Task::proc_tag`]（进程标签；内核任务共 0）——已有唯一稳定载体；
/// v3 真 ABI 的 per-thread 键随 `policy_bind` 协议定（届时换键源，接口不变）。
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub task_key: u64,
    pub directive: Directive,
}

/// 绑定的策略表（`None` = 接口缺席——等价性测试的对照态）。
static mut TABLE: Option<&'static [Entry]> = None;

/// 绑定表（v1 仅内核侧 ktest/boot 期调用；v3 由 `policy_bind` 经共享环换表）。
pub fn bind(tbl: &'static [Entry]) {
    // SAFETY: 写点仅 IRQ 屏蔽的 boot/ktest 期（单核无并发；决策路径只读）。
    unsafe { TABLE = Some(tbl) };
}

/// 解绑：恢复「接口缺席」态（等价性测试的对照分支；ktest 收尾清理）。
pub fn unbind() {
    // SAFETY: 同 bind。
    unsafe { TABLE = None };
}

/// 接口是否在位（测试观察用）。
pub fn bound() -> bool {
    // SAFETY: 只读拷贝（不取静态引用——rust-2024 `static_mut_refs` 纪律，
    // 与 `sched.rs` 计数器读法同款）。
    let tbl = unsafe { TABLE };
    tbl.is_some()
}

/// 查表：快路径纪律（§2「开销恒定」）在**空态**上先兑现——未绑/空表两次
/// 分支即返 `None`（「AI 缺席零影响」的形态学保证）；非空表线性扫是 v1 留桩
/// 实现，v3 换成原子快照 + O(1) 查（哈希/数组下标，随 `policy_bind` 协议定）。
fn lookup(tbl: &'static [Entry], task_key: u64) -> Option<&'static Directive> {
    tbl.iter()
        .find(|e| e.task_key == task_key)
        .map(|e| &e.directive)
}

/// 决策路径挂钩：pick_next 出选后调用。查到指令 → 写 TCB 表示法字段
/// （**幂等**：`prio_effective = max(prio_base + 夹紧权重, pi_inherited)`
/// 自基线重算、EPP 直接赋值——非累积，重复 pick 不漂移）；查不到 → **零
/// 写入**，字段保持基线，与接口缺席逐位等价（ktest `POLICY ok` 断言对象）。
/// IRQ 热路径零分配。
pub fn apply(task: &mut Task) {
    // SAFETY: 只读 TABLE；写点（bind/unbind）仅在 IRQ 屏蔽的 boot/ktest 期。
    let tbl = match unsafe { TABLE } {
        Some(t) => t,
        None => return,
    };
    if tbl.is_empty() {
        return;
    }
    let d = match lookup(tbl, task.proc_tag as u64) {
        Some(d) => d,
        None => return,
    };
    let w = d.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX);
    // 4-3b：策略权重与 PI 捐赠同合成（冻结的 boost 不被 apply 抹平）。
    task.prio_effective = (task.prio_base + w).max(task.pi_inherited);
    if let Some(e) = d.epp {
        task.epp = e;
    }
}

/// 该任务当前绑定的夹紧权重（未绑/无表/空表 → 0）。[`crate::sched::
/// refresh_effective`] 的合成分量——PI 重推刷 effective 时与 pick 侧 apply
/// 用同一份权重，两写入者公式一致才不会互相踩（4-3b）。
pub(crate) fn weight_of(task: &Task) -> i32 {
    // SAFETY: 只读 TABLE（写点仅 IRQ 屏蔽的 boot/ktest 期）。
    match unsafe { TABLE } {
        Some(t) if !t.is_empty() => match lookup(t, task.proc_tag as u64) {
            Some(d) => d.weight.clamp(-WEIGHT_MAX, WEIGHT_MAX),
            None => 0,
        },
        _ => 0,
    }
}
