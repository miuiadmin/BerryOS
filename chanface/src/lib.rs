//! berry-chanface —— channel 队列资格判与容量门纯函数面（W3-Q9）。
//!
//! **动机**：channel 的资格判与容量门语义（IPC设计 §2 非阻塞有界队列：
//! send 满拒 -EBUSY / recv 空返 -EAGAIN / 参数超限 -EINVAL / recv 容量
//! 不足 -EINVAL 保消息原样）此前散在 `kernel/src/channel.rs`（queue_full/
//! has_message 谓词裸式 + CHAN_* 界常量）与 `kernel/src/syscall.rs`
//! （send 参数上限裸判 + 数组内重复值两两比 + recv 容量门裸判）里——
//! host 测不可达（kernel 依赖 HAL global_asm，只 aarch64-unknown-none
//! 可编），ktest 只覆盖 boot 相。判定律的回归面（边界值、单调性、派生位
//! 键律）需要一张 host 可测的**钉板**：本 crate 把界常量与判据抽成纯
//! 函数 + 表驱动测试，`kernel` 侧消费同一函数（零第二实现——抽离即唯一
//! 事实源，非镜像；hdlface encode/decode 同款纪律）。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 kernel 行为）：
//! - [`CHAN_MSG_MAX`]/[`CHAN_HND_MAX`]/[`CHAN_DEPTH`]：界常量单源
//!   （自 channel.rs 迁此，kernel `pub use` re-export——ABI §8.3
//!   send/recv 数组界 + IPC设计 §2 有界深度）。
//! - [`send_args_legal`]：send 参数上限律（nbytes ≤ MSG_MAX 且
//!   nhandles ≤ HND_MAX——超限 -EINVAL，no-panic 前置）。
//! - [`handles_distinct`]：随行 handle 数组内无重复判（重复值先 take 会
//!   作废后值、回滚也换值——原子序「任一步失败 = 调用者表零变化」的
//!   前置预检，违者 -EINVAL）。
//! - [`queue_admits`]/[`queue_full_of`]：容量门（len < DEPTH 可入队；
//!   满 = send -EBUSY 来源 + WRITABLE 派生位判）。
//! - [`queue_readable`]：READABLE 信号状态律（len > 0 = 有消息可收）。
//! - [`recv_capacity_legal`]：recv 容量门（msg 字节/handle 数 ≤ 调用方
//!   容量——不足 -EINVAL 且消息保原样，ABI §8.3 注记）。
//! - [`wait_signal_of`]：4-2 内核派生位映射（READABLE=本端队列非空 /
//!   WRITABLE=对端队列未满——判据单源，wait 面与 fastpath 检查链共用）。
//!
//! 与冻结面的关系：syscall ABI 零触碰（错误码与参数序不变——错误码映射
//! 留在 syscall 调用点，本面只供判据）；`ChannelEnd` 的队列机械
//! （Vec/push/pop/互钉/冻结属主 retain 配平）不在此 crate（有状态面留
//! kernel——本面只钉「值域判据」的判定律，不钉「队列怎么动」的机械）。
//!
//! 与 berry-wqmatch 的域界（W3-Q2 定谳）：wake 谓词与单等待者 fastpath
//! 资格判在 wqmatch（键门 + 信号相交）；本 crate 只钉**队列侧**判据
//! （界常量 + 容量门 + 派生位判）——两 crate 域不重叠。

#![cfg_attr(not(test), no_std)]

// ---- 界常量（单源：自 kernel/src/channel.rs 迁此，kernel re-export；
// 改值 = ABI 事故，host test 字面值钉板先红）----

/// 单条消息 inline 字节上限（IPC设计 §2：内联载荷 ≤256B；更大走共享 VMO）。
pub const CHAN_MSG_MAX: usize = 256;
/// 单条消息随行 handle 上限（ABI §8.3 send/recv 数组界）。T-1：4→6——
/// berry-exec env 块 gift 扩至 6 位；A2-3：6→8——bsh arm 三 extras
/// （rootfs/fs/netd 端）+ base 2 + T-1 自描述对 2 = env 块 7 位（首跑实抓：
/// nhandles=7 > 6 → send -EINVAL → spawn 链 trap 自陷 svcmgr）；libc
/// `io.rs` 收窗与 ktest H3 超限样本同步 8/9 口径。
pub const CHAN_HND_MAX: usize = 8;
/// 每端收件队列深度（有界性 = send -EBUSY 的来源）。
pub const CHAN_DEPTH: usize = 8;

/// send 参数上限律（no-panic 前置：先于一切指针解引用——`sys_channel_send`
/// 首检，超限 -EINVAL）。
///
/// 判据：`nbytes ≤ CHAN_MSG_MAX` 且 `nhandles ≤ CHAN_HND_MAX`。
#[inline]
pub fn send_args_legal(nbytes: usize, nhandles: usize) -> bool {
    nbytes <= CHAN_MSG_MAX && nhandles <= CHAN_HND_MAX
}

/// 随行 handle 数组前缀无重复判（send 全量预检的一环——两两比对律）。
///
/// 语义依据（`sys_channel_send` 原子序注）：重复值先 `take` 会作废后值，
/// 回滚也换值——「任一步失败 = 调用者表零变化」的原子承诺要求预检拒绝
/// 数组内重复（-EINVAL）。
#[inline]
pub fn handles_distinct(hvals: &[u32]) -> bool {
    for i in 0..hvals.len() {
        for j in 0..i {
            if hvals[j] == hvals[i] {
                return false;
            }
        }
    }
    true
}

/// 容量门（send 满检的过侧）：len < DEPTH → 可入队。
#[inline]
pub fn queue_admits(len: usize) -> bool {
    len < CHAN_DEPTH
}

/// 容量门满侧（send -EBUSY 来源 + WRITABLE 派生位判）：len ≥ DEPTH。
#[inline]
pub fn queue_full_of(len: usize) -> bool {
    len >= CHAN_DEPTH
}

/// READABLE 信号状态律（len > 0 = 有消息可收）。
#[inline]
pub fn queue_readable(len: usize) -> bool {
    len > 0
}

/// recv 容量门：消息载荷（nbytes/nhandles）≤ 调用方容量
/// （cap_bytes/cap_hnd）→ true；不足 → false（-EINVAL 且消息保原样——
/// `recv_rollback` 压回队首，FIFO 复原；ABI §8.3 注：v1 无「回写所需
/// 尺寸」面）。
#[inline]
pub fn recv_capacity_legal(nbytes: usize, nhandles: usize, cap_bytes: usize, cap_hnd: usize) -> bool {
    nbytes <= cap_bytes && nhandles <= cap_hnd
}

/// 4-2 内核派生位映射（wait 面 READABLE/WRITABLE 判据单源）：
/// 返回 `(readable, writable)`。
///
/// 键律（`sys_channel_send`/`sys_channel_recv` 唤醒点注）：A 的可读性
/// 由 A 自己的队列决定；A 的**可写性由对端 B 的队列决定**——send 压对端、
/// recv 腾对端，唤醒键 = 对端 ObjRef 地址。
#[inline]
pub fn wait_signal_of(len_self: usize, len_peer: usize) -> (bool, bool) {
    (queue_readable(len_self), !queue_full_of(len_peer))
}

// ---- 表驱动测试（host 钉板——判据边界值/单调性/映射律逐条钉）----

#[cfg(test)]
mod tests {
    use super::*;

    // 界常量字面值钉死（ABI §8.3 冻结值——迁移后此即单源，漂移 = 编译红；
    // 字面值钉板保历史锚：MSG_MAX 256 / HND_MAX 8 / DEPTH 8）。
    #[test]
    fn frozen_constants() {
        assert_eq!(CHAN_MSG_MAX, 256);
        assert_eq!(CHAN_HND_MAX, 8);
        assert_eq!(CHAN_DEPTH, 8);
    }

    // send 参数上限律：边界值钉板（0/界值/界值+1 三点 + 表驱动抽查）。
    #[test]
    fn send_args_boundary() {
        assert!(send_args_legal(0, 0)); // 空消息合法（零载荷零令牌）
        assert!(send_args_legal(CHAN_MSG_MAX, CHAN_HND_MAX)); // 恰满界
        assert!(!send_args_legal(CHAN_MSG_MAX + 1, 0)); // 字节超限
        assert!(!send_args_legal(0, CHAN_HND_MAX + 1)); // 令牌超限
        assert!(!send_args_legal(usize::MAX, 0));
        assert!(!send_args_legal(0, usize::MAX));
        for &(nb, nh) in &[(0, 1), (1, 0), (128, 4), (255, 7), (256, 8)] {
            assert!(send_args_legal(nb, nh), "({nb},{nh}) 应过");
        }
        for &(nb, nh) in &[(257, 0), (0, 9), (257, 9), (usize::MAX, usize::MAX)] {
            assert!(!send_args_legal(nb, nh), "({nb},{nh}) 应拒");
        }
    }

    // 数组内重复判：表驱动（空/单/重复在头/尾/中间/全同/任意位）。
    #[test]
    fn distinct_table() {
        assert!(handles_distinct(&[])); // 空数组（nhandles=0 send 合法）
        assert!(handles_distinct(&[1]));
        assert!(handles_distinct(&[1, 2, 3, 4, 5, 6, 7, 8])); // 满额全异
        assert!(!handles_distinct(&[1, 1])); // 相邻重复
        assert!(!handles_distinct(&[1, 2, 1])); // 首尾重复
        assert!(!handles_distinct(&[1, 2, 3, 2])); // 中间重复
        assert!(!handles_distinct(&[5; 8])); // 全同
        assert!(!handles_distinct(&[9, 1, 2, 3, 4, 5, 6, 9])); // 任意位重复
    }

    // 容量门：len 0..=DEPTH+1 全扫描（单调性——过界即恒满）+ admits/full
    // 互补律（同 len 恒相反）。
    #[test]
    fn queue_gate_scan() {
        for len in 0..CHAN_DEPTH {
            assert!(queue_admits(len), "len={len} 应可入");
            assert!(!queue_full_of(len), "len={len} 不应满");
        }
        assert!(!queue_admits(CHAN_DEPTH));
        assert!(queue_full_of(CHAN_DEPTH));
        // 防御面：超深同样满（永不 panic、单调不回退）。
        assert!(queue_full_of(CHAN_DEPTH + 1));
        assert!(queue_full_of(usize::MAX));
        assert!(!queue_admits(usize::MAX));
    }

    // READABLE 律：空=非可读、非空=可读（1..=DEPTH 全过）。
    #[test]
    fn readable_scan() {
        assert!(!queue_readable(0));
        for len in 1..=CHAN_DEPTH {
            assert!(queue_readable(len), "len={len} 应可读");
        }
    }

    // recv 容量门：两轴独立 + 恰等过（表驱动）。
    #[test]
    fn recv_capacity_table() {
        assert!(recv_capacity_legal(256, 8, 256, 8)); // 恰等过
        assert!(recv_capacity_legal(0, 0, 0, 0)); // 空消息零容量合法
        assert!(recv_capacity_legal(128, 4, 256, 8)); // 过盈
        assert!(!recv_capacity_legal(257, 0, 256, 8)); // 字节轴不足
        assert!(!recv_capacity_legal(0, 9, 256, 8)); // handle 轴不足
        assert!(!recv_capacity_legal(1, 0, 0, 0)); // 消息非空容量零
        assert!(!recv_capacity_legal(usize::MAX, usize::MAX, 0, 0)); // 防御面
    }

    // 派生位映射律：A 读 A 队、A 写看对端（键律钉板）。
    #[test]
    fn wait_signal_mapping() {
        assert_eq!(wait_signal_of(0, 0), (false, true)); // 空对空
        assert_eq!(wait_signal_of(1, 0), (true, true)); // 本端有消息
        assert_eq!(wait_signal_of(0, CHAN_DEPTH), (false, false)); // 对端满
        assert_eq!(wait_signal_of(3, 3), (true, true)); // 双端中点
        assert_eq!(wait_signal_of(CHAN_DEPTH, CHAN_DEPTH), (true, false)); // 满读+对端满
        assert_eq!(wait_signal_of(CHAN_DEPTH, 0), (true, true)); // 满读+对端空
    }

    // WRITABLE 只看对端深度（与本端无关——键律独立轴钉板）。
    #[test]
    fn writable_only_peer() {
        assert!(wait_signal_of(0, 0).1); // 对端空 → 可写
        assert!(wait_signal_of(0, CHAN_DEPTH - 1).1); // 对端恰未满
        assert!(!wait_signal_of(0, CHAN_DEPTH).1); // 对端满 → 不可写
        assert!(!wait_signal_of(0, usize::MAX).1); // 防御面
        // 本端任意深度不改变 WRITABLE（只看对端）。
        for len in 0..=CHAN_DEPTH {
            assert!(wait_signal_of(len, 0).1, "len={len} 本端深度不影响 WRITABLE");
        }
    }

    // kernel 消费点对齐律：syscall.rs/channel.rs 的调用面与本 crate 判据
    // 逐字同式（改一侧不改另一侧 = 此测提醒对拍——hdlface/evface 同款）。
    #[test]
    fn kernel_alignment_documented() {
        // sys_channel_send 首检 == send_args_legal；peer_end.queue_full()
        // == queue_full_of(len)；hvals 预检环 == handles_distinct；
        // recv 容量门 == recv_capacity_legal；wait 状态位 == wait_signal_of
        // （消费点清单见 lib.rs 模块注——此处钉互补/等价关系而非重复式）。
        assert_eq!(queue_admits(0), !queue_full_of(0));
        assert_eq!(send_args_legal(CHAN_MSG_MAX, CHAN_HND_MAX), true);
        assert_eq!(recv_capacity_legal(0, 0, 0, 0), true);
    }
}
