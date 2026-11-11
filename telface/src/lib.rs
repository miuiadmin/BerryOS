//! berry-telface —— telemetry 行编码与聚合纯函数面（W3-Q10）。
//!
//! **动机**：遥测行的数值编码律此前散在 `kernel/src/telemetry.rs`（name_id
//! FNV-1a 哈希 / log2 延迟桶律 `63 - leading_zeros`）与 `kernel/src/user.rs`
//! （`benchwake_percentile` 分位秩——直方图桶 → 保守分位上界）里——host 测
//! 不可达（kernel 依赖 HAL global_asm，只 aarch64-unknown-none 可编），ktest
//! 只覆盖 boot 相。编码律的回归面（哈希稳定性、桶边界、分位秩单调、行格式
//! 不变量）需要一张 host 可测的**钉板**：本 crate 把这些纯函数按值域抽成
//! 单源 + 表驱动测试，`kernel` 侧消费同一函数（零第二实现——抽离即唯一
//! 事实源，非镜像；hdlface encode/decode 同款纪律）。
//!
//! 抽离面（与 kernel 侧逐字对齐，改这里的语义 = 改 kernel 行为）：
//! - [`name_id`]：任务名 → 稳定短 id（FNV-1a-32）——环条目 `prev`/`next`/
//!   `task_id` 与槽 `last_woken_id` 的编码器；**稳定性 = 跨 boot/跨编译器
//!   不变**（展示端反查名字表靠它，改算法 = 撕裂既有日志对照面）。
//! - [`HIST_BUCKETS`] 与 [`latency_bucket`]：R-0 唤醒延迟直方图的 log2 桶
//!   律（延迟 ∈ [2^i, 2^(i+1)) 归桶 i；0 归桶 0；≥ 2^63 归桶 63——无下溢
//!   无溢出）。
//! - [`bucket_upper_bound`]：桶 i 的保守分位上界（`1 << (i+1)`，顶桶
//!   `u64::MAX`——2^64 溢出防）。
//! - [`hist_percentile`]：直方图分位秩（最近秩 ceil(p·n/100) 的 log2 形
//!   ——user.rs `benchwake_percentile` 单源迁此；p50/p90/p99 消费）。
//! - [`WakesrcRow`] / [`wakesrc_row`]：G1.5 WAKESRC 单行的**字段序律**钉板
//!   （acct/T/U/R/L/I/K/P/X 九段 + U/P 三段形 `w/i/nw`、其余 `w/i` 两段
//!   形——行格式的字段序与段形是 smoke 收割正则的契约面，散拼一处错序
//!   即红一整轮 CI）。
//!
//! 与冻结面的关系：syscall ABI 零触碰；环/槽表的原子机械（seqlock 写方
//! 协议、fetch_min/max 序）不在此 crate（有状态并发面留 kernel——本面只
//! 钉「值怎么编码」的判定律，不钉「环怎么写」的机械）。
//!
//! 与 berry-chanface 的域界（W3-Q9 定谳）：chanface 钉 channel 队列侧
//! 判据；本 crate 钉 telemetry 编码侧——两 crate 域不重叠。

#![cfg_attr(not(test), no_std)]

// ---- name_id：FNV-1a-32 稳定哈希（单源：自 kernel/src/telemetry.rs 迁此，
// kernel 侧 `use berry_telface::name_id` 消费——原私有 fn 删除，零第二
// 实现）----

/// 任务名 → 稳定短 id（FNV-1a-32；0 = idle/boot 前，展示端反查靠名字表，
/// 环内只存定宽 id）。
///
/// 稳定契约：同名字恒同 id（offset basis 0x811c_9dc5 + prime 0x0100_0193，
/// 逐字节 xor-then-mul；空名 = basis 0x811c_9dc5 非 0——idle 判据靠名字表
/// 而非 id 值，此函数不承担语义判）。算法参数是**日志对照面的契约常量**
/// ——改动 = 既有串口日志/环内 id 全部失效，禁止。
#[inline]
pub fn name_id(name: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in name {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

// ---- R-0 直方图 log2 桶律（单源：自 kernel/src/telemetry.rs 迁此；
// WAKE_HIST_BUCKETS 常量 kernel 侧保留（telemetry.rs 静态数组长度承重），
// 桶函数与桶数语义在此钉板）----

/// log2 延迟桶数（桶 i 计延迟 ∈ [2^i, 2^(i+1)) 的唤醒次数；延迟 0（同刻
/// 醒）归桶 0，延迟 ≥ 2^63 归桶 63——打印端上界给 u64::MAX 防 2^64 溢出）。
pub const HIST_BUCKETS: usize = 64;

/// 延迟 → log2 桶下标（on_wake 写点律：lat=0 归桶 0；否则 idx = 63 −
/// leading_zeros——lat ≥ 1 ⇒ lz ≤ 63，lat ≥ 2^63 恰归桶 63，无下溢无溢出）。
#[inline]
pub fn latency_bucket(lat: u64) -> usize {
    if lat == 0 {
        0
    } else {
        63 - lat.leading_zeros() as usize
    }
}

/// 桶 i 的保守分位上界（桶 i 覆盖 [2^i, 2^(i+1))——上界 = 下一桶底；
/// 顶桶 i=63 的上界 = u64::MAX 而非 2^64（溢出防）。[`hist_percentile`] 的
/// 返回值即此——「分位是桶上界的保守近似」的诚实取舍单源。
#[inline]
pub fn bucket_upper_bound(idx: usize) -> u64 {
    if idx >= 63 {
        u64::MAX
    } else {
        1 << (idx + 1)
    }
}

/// 直方图分位（最近秩 ceil(p·n/100) 的 log2 形）：返回覆盖该秩的最小桶的
/// 保守上界。
///
/// 单源自 `kernel/src/user.rs::benchwake_percentile`（p50 = (1,2)、p90 =
/// (9,10)、p99 = (99,100) 消费）——`num/den` 有理数形避免浮点（no_std +
/// 确定性）。`sum(buckets) ≥ n ≥ target` 恒立时循环必命中；空直方图
/// （n=0 → target=0 → 首桶即命中）与溢出面（saturating_mul + 保守回
/// u64::MAX）的防御路径原样保留。
#[inline]
pub fn hist_percentile(buckets: &[u64], n: u64, num: u64, den: u64) -> u64 {
    let prod = n.saturating_mul(num);
    let target = prod / den + u64::from(prod % den != 0);
    let mut acc: u64 = 0;
    for (idx, &c) in buckets.iter().enumerate() {
        acc += c;
        if acc >= target {
            return bucket_upper_bound(idx);
        }
    }
    // 不可达（sum ≥ n ≥ target）；保守回 u64::MAX——打印路径非执法点。
    u64::MAX
}

// ---- G1.5 WAKESRC 行字段序律（user.rs watchdog 散拼 → 结构化钉板；
// kernel 侧暂按段序消费（拼行仍 uart 直写——PRINT_LOCK 窗内逐段 puts 是
// G43 契约），本类型钉「段序 + 段形」的判定律，防散拼错序）----

/// WAKESRC 单行九段快照（user.rs G1.5 块的快照面——T/U/R/L/I/K/P/X 八源
/// + acct 总账）。
///
/// 段形律：U（UART_RX）与 P（POWER_KEY）为三段形 `w/i/nw`（no_waiter 计
/// 数面），其余六段两段形 `w/i`——**段形与段序是 smoke 收割正则的契约**，
/// 错序/漏段即整轮 CI 红。字段名与 telemetry `WakeSrcSnapshot` 对齐（此
/// 处独立声明保持本 crate 零依赖）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakesrcRow {
    /// 独立总账（Σ 全槽 wakes——守恒断言右侧）。
    pub acct: u64,
    /// TIMER（wakes, irq）。
    pub t: (u64, u64),
    /// UART_RX（wakes, irq, no_waiter）——三段形。
    pub u: (u64, u64, u64),
    /// SGI_RESCHED（wakes, irq）。
    pub r: (u64, u64),
    /// SGI_TLB（wakes, irq）。
    pub l: (u64, u64),
    /// IPC（wakes, irq=0——无 HAL 臂，恒两段形）。
    pub i: (u64, u64),
    /// KERNEL_BOOT（wakes, irq=0）。
    pub k: (u64, u64),
    /// POWER_KEY（wakes, irq, no_waiter）——三段形（boot 期快照恒 0/0/0，
    /// 注入期数值面由 `PWRKEY unowned n=` 行承载）。
    pub p: (u64, u64, u64),
    /// UNKNOWN（wakes, irq）。
    pub x: (u64, u64),
}

/// WAKESRC 行段序（钉板常量——消费侧拼行按此序，smoke 正则对齐）。
pub const WAKESRC_FIELD_ORDER: [&str; 9] =
    ["acct", "T", "U", "R", "L", "I", "K", "P", "X"];

/// 行形状自检（host 钉板 + kernel 侧拼行前断言面）：九段全在、U/P 三段形、
/// U/P 的 no_waiter 段位不与其余段混淆。
///
/// 空实现护栏：本函数钉「段序常量与段形声明一致」的元律——`U`/`P` 在
/// [`WAKESRC_FIELD_ORDER`] 的下标（2/7）与 `WakesrcRow` 字段序对齐，防止
/// 后续加段时错位。
pub fn wakesrc_row_shape_ok() -> bool {
    WAKESRC_FIELD_ORDER.len() == 9
        && WAKESRC_FIELD_ORDER[0] == "acct"
        && WAKESRC_FIELD_ORDER[2] == "U"
        && WAKESRC_FIELD_ORDER[7] == "P"
        && WAKESRC_FIELD_ORDER[8] == "X"
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- name_id：FNV-1a-32 稳定哈希钉板 ----

    /// 已知向量（独立重算的对照值——算法参数 0x811c_9dc5/0x0100_0193 的
    /// 字面钉板；改参数/改序即红）。
    #[test]
    fn name_id_known_vectors() {
        fn ref_fnv(data: &[u8]) -> u32 {
            let mut h: u32 = 0x811c_9dc5;
            for &b in data {
                h ^= b as u32;
                h = h.wrapping_mul(0x0100_0193);
            }
            h
        }
        // 对照向量：空名 = basis；单字节；多字节任务名（内核任务名形）。
        let cases: &[&[u8]] = &[
            b"",
            b"A",
            b"idle",
            b"watchdog",
            b"console",
            b"task_simcon",
            b"abuser",
            b"rwake",
            b"\xff\x00\x80",
        ];
        for &c in cases {
            assert_eq!(name_id(c), ref_fnv(c), "name_id({:?}) 偏离 FNV-1a-32", c);
        }
        // 字面值钉板（防「对照函数与被测函数同源漂移」——两处独立写死）。
        assert_eq!(name_id(b""), 0x811c_9dc5);
        assert_eq!(name_id(b"a"), 0xe40c_292c);
    }

    /// 稳定性：同输入恒同输出（重复调用 + 切片形变不漂移）。
    #[test]
    fn name_id_stable() {
        let a = name_id(b"ktest:hdr_wake");
        for _ in 0..8 {
            assert_eq!(name_id(b"ktest:hdr_wake"), a);
        }
        // 前缀扩展不改前缀段行为（逐字节流性质——无整块混淆）。
        assert_ne!(name_id(b"ab"), name_id(b"abc"));
        assert_ne!(name_id(b"ab"), name_id(b"ba"));
        // 值域：u32 全域可现（非零段映射后截断仍满域）。
        assert_ne!(name_id(b"x"), name_id(b"y"));
    }

    /// 陷阱向量：xor-then-mul 序（mul-then-xor 伪实现即红——判别性向量）。
    #[test]
    fn name_id_order_discriminating() {
        // 伪实现（mul-then-xor）对照：两实现对 "ab" 必分歧。
        let mut wrong: u32 = 0x811c_9dc5;
        for &b in b"ab" {
            wrong = wrong.wrapping_mul(0x0100_0193);
            wrong ^= b as u32;
        }
        assert_ne!(
            name_id(b"ab"),
            wrong,
            "name_id 与 mul-then-xor 伪实现不可同值（xor-then-mul 序钉板）"
        );
    }

    // ---- log2 桶律 ----

    /// 边界律：lat=0 → 桶 0；2^i（i<63）→ 桶 i；2^(i+1)−1 → 桶 i；
    /// 2^63 → 桶 63；u64::MAX → 桶 63（无下溢无溢出全域钉板）。
    #[test]
    fn latency_bucket_boundaries() {
        assert_eq!(latency_bucket(0), 0, "lat=0（同刻醒）归桶 0");
        for i in 0..63u32 {
            let base = 1u64 << i;
            assert_eq!(latency_bucket(base), i as usize, "2^{i} 归桶 {i}");
            if i >= 1 {
                assert_eq!(latency_bucket(base + 1), i as usize, "2^{i}+1 仍桶 {i}");
            }
            assert_eq!(
                latency_bucket((base << 1) - 1),
                i as usize,
                "2^{}−1（桶内顶）归桶 {i}",
                i + 1
            );
        }
        assert_eq!(latency_bucket(1 << 63), 63, "2^63 恰归桶 63");
        assert_eq!(latency_bucket(u64::MAX), 63, "值域顶归桶 63（无溢出）");
    }

    /// 律式全域抽查：bucket(lat) == floor(log2(lat))（lat ≥ 1 时）。
    #[test]
    fn latency_bucket_matches_log2() {
        let mut lat = 1u64;
        while lat < 1 << 40 {
            assert_eq!(latency_bucket(lat) as u32, 63 - lat.leading_zeros());
            lat = lat.wrapping_mul(3).wrapping_add(7); // 伪随机游走
        }
        // 大值域远端抽查（2^60..2^63）。
        for i in 60..63 {
            assert_eq!(latency_bucket(1u64 << i), i as usize);
            assert_eq!(latency_bucket((1u64 << i) + (1u64 << (i - 1))), i as usize);
        }
    }

    /// 桶上界：bucket_upper_bound(i) = 2^(i+1)（i<63）；顶桶 = u64::MAX。
    #[test]
    fn bucket_upper_bound_law() {
        for i in 0..63 {
            assert_eq!(bucket_upper_bound(i), 1u64 << (i + 1));
        }
        assert_eq!(bucket_upper_bound(63), u64::MAX, "顶桶上界 = u64::MAX（2^64 溢出防）");
        // 越界防御（消费侧误传 63+）：同顶桶（保守收敛，不 panic）。
        assert_eq!(bucket_upper_bound(64), u64::MAX);
    }

    /// 覆盖律：bucket_upper_bound(latency_bucket(v)) ≥ v（全值域保守性
    /// ——分位是桶上界近似的前提）。
    #[test]
    fn bucket_covers_value() {
        let mut v = 0u64;
        for _ in 0..200 {
            assert!(bucket_upper_bound(latency_bucket(v)) >= v);
            v = v.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        }
        for i in 0..64 {
            let base = if i == 0 { 0 } else { 1u64 << (i - 1) };
            let top = if i == 63 { u64::MAX } else { (1u64 << i) - 1 };
            assert!(bucket_upper_bound(i) >= top, "桶 {i} 上界须覆盖桶内顶 {top}");
            let _ = base;
        }
    }

    // ---- hist_percentile：分位秩 ----

    /// 单点直方图：全样本同桶 → 任意分位 = 该桶上界。
    #[test]
    fn percentile_single_bucket() {
        let mut b = [0u64; HIST_BUCKETS];
        b[10] = 32;
        assert_eq!(hist_percentile(&b, 32, 1, 2), 1 << 11); // p50
        assert_eq!(hist_percentile(&b, 32, 9, 10), 1 << 11); // p90
        assert_eq!(hist_percentile(&b, 32, 99, 100), 1 << 11); // p99
    }

    /// 最近秩律：n=32 的 p50 = 第 16 样本（秩 ceil(32·1/2)=16）——恰分
    /// 两个桶时 p50 取**高**桶上界（≥ 半数样本所在桶）。
    #[test]
    fn percentile_rank_rounding() {
        let mut b = [0u64; HIST_BUCKETS];
        b[4] = 15; // 桶 4：15 样本（低）
        b[8] = 17; // 桶 8：17 样本（高）
        // target = ceil(32*1/2) = 16 > 15 → 落桶 8。
        assert_eq!(hist_percentile(&b, 32, 1, 2), 1 << 9);
        // 15 样本恰过 p50 半界（ceil(30/2)=15 ≤ 15）→ 桶 4。
        let mut c = [0u64; HIST_BUCKETS];
        c[4] = 15;
        c[8] = 15;
        assert_eq!(hist_percentile(&c, 30, 1, 2), 1 << 5);
    }

    /// 顶桶样本 → u64::MAX（2^64 溢出防——打印面上界诚实）。
    #[test]
    fn percentile_top_bucket_saturates() {
        let mut b = [0u64; HIST_BUCKETS];
        b[63] = 8;
        assert_eq!(hist_percentile(&b, 8, 99, 100), u64::MAX);
        assert_eq!(hist_percentile(&b, 8, 1, 2), u64::MAX);
    }

    /// n=0（零样本直方图）：target=0 → 首桶命中 = 桶 0 上界 2（消费侧
    /// `h.n > 0` 门控在前，此处钉防御路径确定性）。
    #[test]
    fn percentile_zero_n_deterministic() {
        let b = [0u64; HIST_BUCKETS];
        assert_eq!(hist_percentile(&b, 0, 1, 2), 2);
    }

    /// 单调律：分位参数 p 越大，返回值不减（同一分布）。
    #[test]
    fn percentile_monotone_in_p() {
        let mut b = [0u64; HIST_BUCKETS];
        b[3] = 10;
        b[7] = 10;
        b[20] = 10;
        b[40] = 4;
        let mut prev = 0;
        for k in 1..=100u64 {
            let v = hist_percentile(&b, 34, k, 100);
            assert!(v >= prev, "p={k} 分位 {v} < p={} 的 {prev}（分位单调破）", k - 1);
            prev = v;
        }
    }

    /// 与全排序对照（随机小样本 → 精确秩验证）：target 秩的样本落在桶 X
    /// ⟹ hist_percentile = 桶 X 上界（log2 桶是保守上界近似——恰是
    /// benchwake_percentile 的契约）。
    #[test]
    fn percentile_matches_sorted_rank() {
        // 合成 20 样本：15 个延迟 ~2^5 量级、5 个 ~2^9 量级。
        let samples: [u64; 20] = core::array::from_fn(|i| {
            if i < 15 { 32 + (i as u64) } else { 512 + (i as u64) * 7 }
        });
        let mut b = [0u64; HIST_BUCKETS];
        for &s in &samples {
            b[latency_bucket(s)] += 1;
        }
        // p50（秩 10）与 p90（秩 18）都落在高段（≥15 样本在低桶）：
        // p50 秩 10 ≤ 15 → 低桶 5；p90 秩 18 > 15 → 桶 9。
        assert_eq!(hist_percentile(&b, 20, 1, 2), 1 << 6);
        assert_eq!(hist_percentile(&b, 20, 9, 10), 1 << 10);
        assert_eq!(hist_percentile(&b, 20, 99, 100), 1 << 10);
    }

    /// 溢出面：n·num 饱和（saturating_mul）后仍确定性返回，不 panic。
    #[test]
    fn percentile_saturating_no_panic() {
        let mut b = [0u64; HIST_BUCKETS];
        b[0] = u64::MAX;
        assert_eq!(hist_percentile(&b, u64::MAX, u64::MAX, u64::MAX), 2);
    }

    // ---- WAKESRC 行段序律 ----

    #[test]
    fn wakesrc_field_order_shape() {
        assert!(wakesrc_row_shape_ok(), "段序常量与段形声明错位");
        assert_eq!(WAKESRC_FIELD_ORDER[0], "acct");
        assert_eq!(WAKESRC_FIELD_ORDER[1], "T");
        // U/P 三段形（段形声明在 WakesrcRow——元钉板防加段错位）。
        let row = WakesrcRow {
            acct: 102,
            t: (40, 500),
            u: (10, 30, 2),
            r: (5, 15),
            l: (1, 8),
            i: (44, 0),
            k: (2, 0),
            p: (0, 0, 0),
            x: (0, 3),
        };
        assert_eq!(row.acct, 102);
        assert_eq!(row.u.2, 2, "U 段第三位 = no_waiter");
        assert_eq!(row.p.2, 0, "P 段第三位 = no_waiter（boot 期恒 0）");
        // 守恒律（G1.5 ① 的行内自洽）：acct == Σ wakes。
        let sum = row.t.0 + row.u.0 + row.r.0 + row.l.0 + row.i.0 + row.k.0 + row.p.0 + row.x.0;
        assert_eq!(row.acct, sum, "WAKESRC 行内 acct ≠ Σ 槽 wakes（两路失联）");
    }
}
