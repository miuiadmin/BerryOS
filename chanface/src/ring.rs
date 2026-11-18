//! channel 收件队列定容环形化机械（W3-Q95——02 号机会三刀 a）。
//!
//! **动机**：`ChannelEnd.queue` 曾为 `Vec<Message>`——pop 用 `remove(0)`、
//! recv 回滚用 `insert(0)`，两条都是队首 O(n) memmove。3b3 H2 定长内联化
//! （`Message` ≈440B 全内联）之后，「深度 ≤8：O(n) 可忽略」的旧注记失准：
//! 满队 recv 一次搬 ~3.1KB（7×440），回滚路径再搬一次——每发恒付、真机
//! IPC 预算下占比可观（`research/内核深度进化/02-IPC快路径-Top5优化机会
//! -v1.0.md` 机会三现状证据段）。
//!
//! **本模块钉的机械**（值域纯函数，状态面留 kernel）：
//! - [`ring_ok`]：不变量判（head < DEPTH、len ≤ DEPTH——tail 不另存，
//!   由 head/len 经 [`ring_tail_of`] 导出，少一份可漂移状态）。
//! - [`ring_slot_of`]：逻辑位 i → 物理槽（`(head + i) % DEPTH`——FIFO
//!   序的单源；kernel 的 front/random 访问经它寻槽）。
//! - [`ring_next_head`]：pop 后 head 步进（+1 mod DEPTH）。
//! - [`ring_advance`]：push/tail 步进（+1 mod DEPTH）。
//!
//! **序语义逐位等价**：pop 头出 = `Vec::remove(0)`；push 尾进 = `push`；
//! push_front 头复原名 = `insert(0)`——三操作在「逻辑序」上与 Vec 机械
//! 完全一致，差别只在物理搬运是 O(1) 槽步进。host 测以 Vec 为模型对拍
//! （全序列 + 伪随机），钉等价性。
//!
//! 与 lib.rs 判据的关系：容量门（`queue_admits`/`queue_readable`）吃
//! len——环形化后 len 语义不变（元素计数，非物理下标），W3-Q9 判据面
//! **零改动**；W3-Q49 序守卫（`push_ok`/`pop_ok`/`push_front_ok`）同吃
//! len，继续执法。chanface 四判据与 kernel 消费点（queue_full/
//! has_message/派生位）全部经 len 供给，钉板零触碰。

use crate::CHAN_DEPTH;

/// 环形不变量判：`head < DEPTH 且 len ≤ DEPTH`。
///
/// tail 不存储（由 [`ring_tail_of`] 导出）——可漂移状态少一份。空/满
/// 以 len 判（`0` / `DEPTH`），head 无歧义位（空队 head 可为任意合法
/// 槽位，pop/push_front 前置 len 门已挡）。
#[inline]
pub fn ring_ok(head: usize, len: usize) -> bool {
    head < CHAN_DEPTH && len <= CHAN_DEPTH
}

/// 逻辑位 `i`（0 = 队首）→ 物理槽下标。
#[inline]
pub fn ring_slot_of(head: usize, i: usize) -> usize {
    (head + i) % CHAN_DEPTH
}

/// tail 物理槽（下一 push 落点）：`(head + len) % DEPTH`。
#[inline]
pub fn ring_tail_of(head: usize, len: usize) -> usize {
    ring_slot_of(head, len)
}

/// pop 后的 head（步进一）。
#[inline]
pub fn ring_next_head(head: usize) -> usize {
    (head + 1) % CHAN_DEPTH
}

/// push 落槽后的 tail 步进（= 任意指针的环形步进；len 同步 +1）。
#[inline]
pub fn ring_advance(slot: usize) -> usize {
    (slot + 1) % CHAN_DEPTH
}

// ---- 表驱动测试（host 钉板——Vec 机械对拍 + 环绕序列 + 不变量）----

#[cfg(test)]
mod tests {
    use super::*;

    /// 模型队列：物理槽数组 + head/len，操作面与 kernel `ChannelEnd` 的
    /// push/pop/push_front 三机械同构（值用 u8 槽内打标）。
    struct Ring {
        slots: [u8; CHAN_DEPTH],
        head: usize,
        len: usize,
    }

    impl Ring {
        fn new() -> Self {
            Self { slots: [0; CHAN_DEPTH], head: 0, len: 0 }
        }

        fn push(&mut self, v: u8) {
            assert!(ring_ok(self.head, self.len) && crate::push_ok(self.len));
            self.slots[ring_tail_of(self.head, self.len)] = v;
            self.len += 1;
        }

        fn pop(&mut self) -> u8 {
            assert!(ring_ok(self.head, self.len) && crate::pop_ok(self.len));
            let v = self.slots[ring_slot_of(self.head, 0)];
            self.head = ring_next_head(self.head);
            self.len -= 1;
            v
        }

        fn push_front(&mut self, v: u8) {
            assert!(ring_ok(self.head, self.len) && crate::push_front_ok(self.len));
            self.head = (self.head + CHAN_DEPTH - 1) % CHAN_DEPTH; // 环形退一
            self.slots[ring_slot_of(self.head, 0)] = v;
            self.len += 1;
        }

        fn logical(&self) -> Vec<u8> {
            (0..self.len).map(|i| self.slots[ring_slot_of(self.head, i)]).collect()
        }
    }

    // 不变量判：合法域全扫（head×len 笛卡尔），越界单点。
    #[test]
    fn ring_invariant_scan() {
        for head in 0..CHAN_DEPTH {
            for len in 0..=CHAN_DEPTH {
                assert!(ring_ok(head, len), "({head},{len}) 应合法");
            }
        }
        assert!(!ring_ok(CHAN_DEPTH, 0)); // head 越界
        assert!(!ring_ok(0, CHAN_DEPTH + 1)); // len 超深
        assert!(!ring_ok(usize::MAX, 0));
        assert!(!ring_ok(0, usize::MAX));
    }

    // 槽映射恒等律：i=0 恒 = head；环绕（head+i ≥ DEPTH）取模正确；
    // DEPTH 整倍数锚点归零。
    #[test]
    fn slot_mapping_law() {
        assert_eq!(ring_slot_of(0, 0), 0);
        assert_eq!(ring_slot_of(5, 0), 5);
        assert_eq!(ring_slot_of(7, 1), 0); // 环绕
        assert_eq!(ring_slot_of(4, 4), 0); // 整倍
        assert_eq!(ring_slot_of(6, 3), 1);
        assert_eq!(ring_tail_of(6, 3), 1); // tail = slot(head, len)
        assert_eq!(ring_next_head(7), 0); // pop 步进环绕
        assert_eq!(ring_next_head(0), 1);
        assert_eq!(ring_advance(7), 0); // push 步进环绕
        // 恒等式：tail(head,len) == next_n 步进链的终点（对拍两径）。
        for head in 0..CHAN_DEPTH {
            for len in 0..=CHAN_DEPTH {
                assert_eq!(ring_tail_of(head, len), ring_slot_of(head, len));
            }
        }
    }

    // Vec 模型全序列对拍：push 到满 → pop 到空 → 再 push——逻辑序与
    // Vec FIFO 逐位等价（remove(0)/insert(0) 的序语义镜像）。
    #[test]
    fn fifo_vec_parity_full_cycle() {
        let mut r = Ring::new();
        let mut model: Vec<u8> = Vec::new();
        // 填满（0..DEPTH 每值两次：标记唯一性靠值错开）。
        for v in 0..CHAN_DEPTH as u8 {
            r.push(v);
            model.push(v);
            assert_eq!(r.logical(), model);
            assert!(ring_ok(r.head, r.len));
        }
        // 满拒由 W3-Q9 容量门执法（此处只证序）：弹出 3 条。
        for _ in 0..3 {
            assert_eq!(r.pop(), model.remove(0));
            assert_eq!(r.logical(), model);
        }
        // 回滚复原形（push_front）：压回首弹值。
        r.push_front(0);
        model.insert(0, 0);
        assert_eq!(r.logical(), model);
        // 排空：全程序等价。
        while r.len > 0 {
            assert_eq!(r.pop(), model.remove(0));
            assert_eq!(r.logical(), model);
        }
        assert!(model.is_empty());
    }

    // 环绕高压：多次满↔空循环跨任意 head 相位（换相位再验 FIFO）。
    #[test]
    fn wraparound_cycles_all_phases() {
        for phase in 0..CHAN_DEPTH {
            let mut r = Ring::new();
            // 相位前置：push k 条再弹 k 条，落 head = phase。
            for v in 0..phase as u8 {
                r.push(v);
            }
            for _ in 0..phase {
                r.pop();
            }
            assert_eq!(r.head, phase);
            let mut model: Vec<u8> = Vec::new();
            let mut next = 0u8;
            // 三轮满↔空 + 每轮交错 push_front 复原。
            for _ in 0..3 {
                while r.len < CHAN_DEPTH {
                    r.push(next);
                    model.push(next);
                    next = next.wrapping_add(1);
                    assert_eq!(r.logical(), model);
                }
                let v = r.pop();
                assert_eq!(v, model.remove(0));
                r.push_front(v);
                model.insert(0, v);
                assert_eq!(r.logical(), model);
                while r.len > 0 {
                    assert_eq!(r.pop(), model.remove(0));
                    assert_eq!(r.logical(), model);
                }
            }
        }
    }

    // 伪随机操作序列对拍（确定性 LCG——无依赖、可复现）：push/pop/
    // push_front 按容量门随机混走 500 步，任意时刻逻辑序与 Vec 等价、
    // 不变量恒立。
    #[test]
    fn random_op_parity_lcg() {
        let mut state: u64 = 0x9E3779B97F4A7C15; // 固定种子
        let mut rng = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut r = Ring::new();
        let mut model: Vec<u8> = Vec::new();
        let mut next = 0u8;
        for _ in 0..500 {
            let dice = rng() % 100;
            match dice {
                0..=44 if r.len < CHAN_DEPTH => {
                    r.push(next);
                    model.push(next);
                    next = next.wrapping_add(1);
                }
                45..=89 if r.len > 0 => {
                    assert_eq!(r.pop(), model.remove(0));
                }
                _ if r.len > 0 && r.len < CHAN_DEPTH => {
                    // 弹一压回（recv 回滚形）。
                    let v = r.pop();
                    assert_eq!(v, model.remove(0));
                    r.push_front(v);
                    model.insert(0, v);
                }
                _ => {} // 容量门拒面（满 push / 空 pop）——跳步
            }
            assert!(ring_ok(r.head, r.len), "不变量失守 @len={}", r.len);
            assert_eq!(r.logical(), model);
        }
    }

    // 容量门复用律：环形化不改 W3-Q9 判据消费面（len 供给不变——
    // queue_full_of/queue_readable 吃 len 与 Vec 时代同式）。
    #[test]
    fn capacity_gates_unchanged_via_len() {
        let mut r = Ring::new();
        assert!(!crate::queue_full_of(r.len));
        assert!(!crate::queue_readable(r.len));
        for v in 0..CHAN_DEPTH as u8 {
            r.push(v);
        }
        assert!(crate::queue_full_of(r.len)); // 满 = send -EBUSY 源
        assert!(crate::queue_readable(r.len));
        while r.len > 0 {
            r.pop();
        }
        assert!(!crate::queue_readable(r.len)); // 空 = recv -EAGAIN 源
    }
}
