//! telemetry 环读面纯函数（W3-Q81 自 kernel/src/telemetry.rs 迁此）。
//!
//! **动机**：per-CPU 双环（切换环/唤醒环）的「按新→旧序合并取第 i 条」
//! vct 归并循环在 `last()` 与 `wake_last()` 逐字重复两份（各 ~30 行游标
//! 机械），合并律——**vct 大者胜、平局取环 0**（`a.vct >= b.vct` → 0）、
//! 游标推进 `k[winner] += 1`、命中判 `k0+k1-1 == i`、流尽 `None`——至今
//! 无 host 钉板（ktest 只覆盖 boot 相单调样本，平局/流尽/越界分支不可
//! 达）。归并序错 = `check()` 单调断言假红/遥测时序面撕裂，且两份重复
//! 漂移即两环行为分叉。本模块把归并判定律抽成单源（泛型 `T` + `vct_of`
//! 访问器——切换环/唤醒环共用一具），kernel 侧 `last`/`wake_last` 委托
//! 消费（零第二实现——hdlface 同款纪律）。
//!
//! 有效条数帽律 `span_cap`（`min(total, RING_LEN)`——环形缓冲「写了 N 条
//! 只留最近 RING_LEN 条」的几何）同批单源：kernel 侧 `ring_span`/
//! `wake_ring_span` 四点消费。
//!
//! 域界（telface 内分模块）：lib.rs 钉「值怎么编码」（name_id/桶律/分位
//! 秩），本模块钉「多环值怎么归并」——环**写**机械（seqlock 协议、
//! per-CPU 静态、unsafe 解引用）留 kernel（有状态并发面不外迁）。

/// 环形缓冲有效条数帽律：写了 `total` 条、环容量 `len` → 环内现存
/// `min(total, len)` 条（wrap 丢头——最旧条目被顶掉）。
///
/// kernel 消费点：`ring_span`/`wake_ring_span`（全环合并 = 各环帽律之和）。
/// `total` 是 u64（每环累计写入数单调不减），返回 usize 截断安全
/// （`len: usize` 帽住上界）。
#[inline]
pub fn span_cap(total: u64, len: usize) -> usize {
    if total >= len as u64 {
        len
    } else {
        total as usize
    }
}

/// 双流 vct 归并取第 `i` 条（0 = 最新；**新→旧序**——vct 大者先出）。
///
/// 判定律（与 kernel 原 `last`/`wake_last` 逐字对齐，改此 = 改两环行为）：
/// - 每轮两流各按游标取一条（`peek0(k0)`/`peek1(k1)`——None = 该流已尽）；
/// - **vct 大者胜出**（新→旧）；**平局（vct 相等）取流 0**——两环同刻写
///   （boot 期同 vct 戳）时稳定序 = 环 0 在前，归并序确定性钉死；
/// - 单流存活直取；双流尽 → `None`（合并流尽——`i` 越界同路）；
/// - 胜者游标推进，累计取数 `k0+k1` 达 `i+1` 时返回该条。
///
/// 泛型 `T: Clone`（kernel 侧 `SwitchRec`/`WakeRec` 均值语义 Copy 形）；
/// `vct_of` 抽 vct 字段（两 Rec 字段名同、类型同——访问器闭包注入免
/// trait 面）。
pub fn merged_at<T, F0, F1, V>(i: usize, mut peek0: F0, mut peek1: F1, vct_of: V) -> Option<T>
where
    F0: FnMut(usize) -> Option<T>,
    F1: FnMut(usize) -> Option<T>,
    V: Fn(&T) -> u64,
    T: Clone,
{
    let mut k = [0usize; 2]; // 各流已取条数（游标）
    for _ in 0..=i {
        // 两流各取当前游标条，vct 大者（更新）胜出。
        let cands = [peek0(k[0]), peek1(k[1])];
        let winner = match (&cands[0], &cands[1]) {
            (Some(a), Some(b)) => {
                if vct_of(a) >= vct_of(b) {
                    0
                } else {
                    1
                }
            }
            (Some(_), None) => 0,
            (None, Some(_)) => 1,
            (None, None) => return None, // 合并流尽
        };
        let picked = cands[winner].clone();
        k[winner] += 1;
        if k[0] + k[1] - 1 == i {
            return picked;
        }
    }
    None // 不可达（循环内必命中或流尽）；保守 None 与原实现同形
}

/// 合并流全量校验律：`merged_at` 产出的全序列按新→旧 vct 单调不减
/// （每对相邻条 `prev.vct >= next.vct`）。
///
/// 定位：host 侧执法锚——与 kernel `check`/`wake_check` 的单调断言同律
/// 钉板（「CNTVCT 只前进——两环各单调（单写者），vct 归并不破单调」）。
/// kernel 断言循环是对 `last`/`wake_last` 合并读的机械控制流（no_std
/// 无 slice 收集面，逐对比较就地判——非第二实现），不复用本函数；
/// 本函数把该律升为 host 可测面（见 `merge_matches_sorted_reference`
/// 全序列自洽断言）。items 为旧→新或新→旧均可断（相邻对双侧比较对称）。
pub fn monotonic_nondecr<T: Copy, V: Fn(&T) -> u64>(items: &[T], vct_of: V) -> bool {
    items.windows(2).all(|w| vct_of(&w[0]) <= vct_of(&w[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试流：Vec 按「写入序」（index 0 最旧）存 vct——peek(k) = 倒数第
    /// k 新（k=0 最新），与 kernel 环语义同构（ring_peek 契约）。
    struct Stream(Vec<u64>);
    impl Stream {
        fn peek(&self, k: usize) -> Option<u64> {
            let n = self.0.len();
            if k >= n {
                None
            } else {
                Some(self.0[n - 1 - k])
            }
        }
    }
    fn merge_streams(i: usize, s0: &Stream, s1: &Stream) -> Option<u64> {
        merged_at(i, |k| s0.peek(k), |k| s1.peek(k), |&v| v)
    }

    #[test]
    fn span_cap_law() {
        assert_eq!(span_cap(0, 64), 0);
        assert_eq!(span_cap(63, 64), 63);
        assert_eq!(span_cap(64, 64), 64);
        assert_eq!(span_cap(65, 64), 64, "wrap 丢头——帽住容量");
        assert_eq!(span_cap(u64::MAX, 64), 64, "敌意巨值安全截断");
        assert_eq!(span_cap(5, 0), 0, "零容量环恒空");
    }

    #[test]
    fn merge_interleaved_order() {
        // 交错流：环0 [1,3,5]、环1 [2,4]（旧→新）——合并新→旧 = 5,4,3,2,1。
        let s0 = Stream(vec![1, 3, 5]);
        let s1 = Stream(vec![2, 4]);
        let got: Vec<u64> = (0..5).map(|i| merge_streams(i, &s0, &s1).unwrap()).collect();
        assert_eq!(got, vec![5, 4, 3, 2, 1]);
        assert_eq!(merge_streams(5, &s0, &s1), None, "合并流尽");
        assert_eq!(merge_streams(999, &s0, &s1), None, "远越界同流尽");
    }

    #[test]
    fn merge_tie_prefers_stream0() {
        // 平局律：两流同 vct（boot 同刻戳形态）——平局轮恒取流 0。
        let s0 = Stream(vec![7]);
        let s1 = Stream(vec![7]);
        assert_eq!(merge_streams(0, &s0, &s1), Some(7));
        assert_eq!(merge_streams(1, &s0, &s1), Some(7));
        assert_eq!(merge_streams(2, &s0, &s1), None);
        // 交叉平局：环0 写入序 [6,7]、环1 写入序 [5,7]（各流 vct 单调）
        // → peek 序环0 [7,6]、环1 [7,5]，新→旧 = 7(环0),7(环1),6,5。
        let a = Stream(vec![6, 7]);
        let b = Stream(vec![5, 7]);
        let got: Vec<u64> = (0..4).map(|i| merge_streams(i, &a, &b).unwrap()).collect();
        assert_eq!(got, vec![7, 7, 6, 5]);
    }

    #[test]
    fn merge_single_stream_and_empty() {
        // 单流存活：另流空——直取（无比较面）。
        let s0 = Stream(vec![10, 20]);
        let empty = Stream(vec![]);
        let got: Vec<u64> = (0..2).map(|i| merge_streams(i, &s0, &empty).unwrap()).collect();
        assert_eq!(got, vec![20, 10]);
        // 双空：任意 i 恒 None。
        assert_eq!(merge_streams(0, &empty, &empty), None);
    }

    #[test]
    fn merge_dup_vct_within_stream() {
        // 环内重复 vct（同核同刻多笔——seqlock 语义合法）：稳定倒序。
        let s0 = Stream(vec![5, 5, 5]);
        let s1 = Stream(vec![5]);
        let got: Vec<u64> = (0..4).map(|i| merge_streams(i, &s0, &s1).unwrap()).collect();
        assert_eq!(got, vec![5, 5, 5, 5]);
        assert_eq!(merge_streams(4, &s0, &s1), None);
    }

    #[test]
    fn merge_matches_sorted_reference() {
        // 性质对拍：随机双流合并 = 全体值降序排序（稳定平局归并的参考真值
        // ——重复值按流序保位，降序多重集等价）。
        let mut seed = 0xC0FFEEu64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _case in 0..50 {
            let n0 = (rnd() % 20) as usize;
            let n1 = (rnd() % 20) as usize;
            let mut v0: Vec<u64> = (0..n0).map(|_| rnd() % 100).collect();
            let mut v1: Vec<u64> = (0..n1).map(|_| rnd() % 100).collect();
            v0.sort_unstable();
            v1.sort_unstable();
            let s0 = Stream(v0.clone());
            let s1 = Stream(v1.clone());
            let mut all = v0;
            all.extend(v1);
            all.sort_unstable_by(|a, b| b.cmp(a)); // 新→旧降序
            for (i, want) in all.iter().enumerate() {
                assert_eq!(
                    merge_streams(i, &s0, &s1),
                    Some(*want),
                    "case 流0={n0} 流1={n1} i={i} 合并序须等于降序参考"
                );
            }
            assert_eq!(merge_streams(all.len(), &s0, &s1), None);
            // 全序列单调不减（旧→新读向）：check 断言律自洽。
            let asc: Vec<u64> = all.iter().rev().copied().collect();
            assert!(monotonic_nondecr(&asc, |&v| v));
        }
    }

    #[test]
    fn monotonic_law_edges() {
        assert!(monotonic_nondecr::<u64, _>(&[], |&v| v), "空序列恒过");
        assert!(monotonic_nondecr::<u64, _>(&[5], |&v| v), "单条恒过");
        assert!(monotonic_nondecr::<u64, _>(&[1, 2, 2, 9], |&v| v), "平值单调不减过");
        assert!(!monotonic_nondecr::<u64, _>(&[3, 2], |&v| v), "回退即拒");
    }
}
