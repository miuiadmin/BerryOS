// W3-Q97：批量 recv（channel_recv_n）纯函数面——批量窗的**资格与界**判定律
// host 钉板（机会五 02 号 IPC 审计：排空式批量 recv ABI）。
//
// **动机**：recv 恒单消息语义，排空队列 = 逐发 syscall——唤醒粒度（wait_many
// 一次醒）与收割粒度（一次一条）天然失配，排空型消费者（console/netd）每
// 消息重复付 SVC 入口结构锁窗 + caller_table + get_raw 门控 + 参数 I/O
// 全套窗。机会五裁案 = add-only 新号 `channel_recv_n(ch, msgs, k_max,
// *n_recv)`：内部循环既有 recv 原子序，**每消息语义逐条保持**，仅 SVC
// 入口/门控/唤醒摊到 k 条。
//
// 本模块只钉**批量窗的判定律**（值域面）；队列机械、令牌入表、唤醒链、
// intent 结算等有状态面留 kernel（W3-Q9 同款域界——「值域判据」与
// 「队列怎么动」分离）：
// - [`RECV_N_MAX`]：单调用批量帽（16——排空型消费者一次唤醒的典型并聚
//   度：C-4 console 实录 ≥2 字节/唤醒、CHAN_DEPTH=8 队列深度本身就是
//   天然上界；16 = 2×DEPTH 留一倍余量，超帽 -EINVAL no-panic 前置）。
// - [`recv_n_k_legal`]：k 参数域判（0 < k ≤ RECV_N_MAX——k=0 恒拒：
//   「收零条」无语义且会与「队空返 0」混淆；超帽拒在预检段，先于任何
//   队列/用户内存触达）。
// - [`recv_n_wake_needed`]：批量循环的 WRITABLE 唤醒判（弹 >0 条 →
//   对端腾 >0 槽 → 唤醒一次即够；弹 0 条〔队空即返〕不唤醒——无槽
//   腾出，唤醒即虚假，单发 recv 空队早退同律）。
// - [`recv_n_slot_legal`]：出参槽地址判（非空 + 8 字节对齐——*n_recv
//   是本号唯一出参，u64 写走查前的 no-panic 前置；对齐律与既有 u64
//   出参面（clock_read *out）同式）。

/// 单调用批量帽：`channel_recv_n` 的 k_max 上界。16 = 2×[`CHAN_DEPTH`]
/// （队列深度的两倍——一次批量窗至多排空一个满队再加一轮补发；再大
/// 无收益：队深 8 排空后需要再等 send，批量窗内恒 ≤ DEPTH + 唤醒后
/// 追加 ≤ DEPTH）。改值 = ABI 事故（ABI §8.3 注记 + ktest 边界样本
/// 同步）。
pub const RECV_N_MAX: usize = 16;

/// `channel_recv_n` 槽位数组的**槽距**（ABI 可见布局——ABI §8.3 注记）：
/// 每槽 = 16B 头（`nbytes:u64`/`nhandles:u64` 出参）+ 256B 载荷区
/// （[`CHAN_MSG_MAX`] 满形预储）+ 32B handle 值区（[`CHAN_HND_MAX`]×4
/// 满形预储）= 304B。改值 = ABI 事故；构成律由 [`tests::stride_layout`]
/// 钉板（三段和恒等）。
pub const RECV_N_SLOT_STRIDE: usize = 16 + CHAN_MSG_MAX + CHAN_HND_MAX * 4;

/// k 参数域判：`0 < k ≤ RECV_N_MAX`。
///
/// k=0 恒拒（-EINVAL）：「收零条」与「队空返 0」不可区分的混淆面，
/// 宁拒不静默成功（ABI §8.0 no-panic 但拒绝语义面的精神）。
#[inline]
pub fn recv_n_k_legal(k: usize) -> bool {
    k > 0 && k <= RECV_N_MAX
}

/// 批量循环 WRITABLE 唤醒判：实弹 n_recv > 0 → true（对端腾出 n_recv
/// 个槽，唤醒一次覆盖全部腾槽——单发 recv 的「仅成功路径唤醒」律在
/// 批量面的自然扩展）；n_recv = 0（队空即返）→ false（无槽腾出，
/// 唤醒即虚假）。
#[inline]
pub fn recv_n_wake_needed(n_recv: usize) -> bool {
    n_recv > 0
}

/// 出参槽地址判：`*n_recv` 槽须存在（非空指针投影）且 8 字节对齐
/// （u64 出参写走查前置——no-panic 界检先于 access_ok 段）。
#[inline]
pub fn recv_n_slot_legal(addr: usize) -> bool {
    addr != 0 && addr % 8 == 0
}

use crate::{CHAN_DEPTH, CHAN_HND_MAX, CHAN_MSG_MAX};

// ---- 表驱动测试（host 钉板——判定律边界值/单调性/混淆面逐条钉）----

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CHAN_DEPTH;

    // k 域全表：0 恒拒；1..=RECV_N_MAX 全收；帽外全拒（含 usize 巨值
    // 溢出面——防御域）。
    #[test]
    fn k_domain_table() {
        assert!(!recv_n_k_legal(0), "k=0 恒拒（与队空返 0 混淆面）");
        for k in 1..=RECV_N_MAX {
            assert!(recv_n_k_legal(k), "k={k} 域内须收");
        }
        for k in (RECV_N_MAX + 1)..(RECV_N_MAX * 3) {
            assert!(!recv_n_k_legal(k), "k={k} 超帽须拒");
        }
        assert!(!recv_n_k_legal(usize::MAX), "防御面：usize 上界拒");
    }

    // 帽与队列深度的关系律：RECV_N_MAX ≥ CHAN_DEPTH（批量帽不小于队列
    // 深——一次批量窗必能排空任意满队；若有人改帽到 < DEPTH，此测先红）。
    #[test]
    fn cap_covers_depth() {
        assert!(RECV_N_MAX >= CHAN_DEPTH, "批量帽 {RECV_N_MAX} 须 ≥ 队深 {CHAN_DEPTH}");
    }

    // 唤醒判全表：0 → false（虚假唤醒免疫）；≥1 → true（单调——实弹
    // 数不改变「须唤醒」结论，只改变腾槽数）。
    #[test]
    fn wake_table() {
        assert!(!recv_n_wake_needed(0), "弹 0 条不唤醒（无槽腾出）");
        for n in 1..=RECV_N_MAX {
            assert!(recv_n_wake_needed(n), "弹 {n} 条须唤醒（对端腾 {n} 槽）");
        }
        // 防御域：超帽巨值（kernel 循环上界是 k，实弹 ≤ min(k, 队深)——
        // 巨值进不到这里，钉判定律自身的单调性）。
        assert!(recv_n_wake_needed(usize::MAX));
    }

    // 出参槽判全表：零址拒；非 8 对齐拒（1/4/9 等奇偶面）；合法对齐
    // （8/16/0x1000）收。与 ktest E 面样本同步（同值对拍）。
    #[test]
    fn slot_table() {
        assert!(!recv_n_slot_legal(0), "零址拒（无出参槽）");
        for off in [1usize, 2, 3, 4, 5, 6, 7, 9, 12, 17] {
            assert!(!recv_n_slot_legal(off), "+{off} 非对齐须拒");
        }
        for addr in [8usize, 16, 24, 4096, 1 << 32] {
            assert!(recv_n_slot_legal(addr), "{addr} 合法对齐槽");
        }
    }

    // 槽距布局钉板：stride = 16B 头 + CHAN_MSG_MAX 载荷 + CHAN_HND_MAX×4
    // handle 值（ABI §8.3 可见布局——三段和恒等，改任一段宽 = ABI 事故
    // 本测先红）。
    #[test]
    fn stride_layout() {
        assert_eq!(RECV_N_SLOT_STRIDE, 16 + CHAN_MSG_MAX + CHAN_HND_MAX * 4);
        assert_eq!(RECV_N_SLOT_STRIDE, 304, "ABI 冻结值（abi.toml 注记同值对拍）");
    }
}
