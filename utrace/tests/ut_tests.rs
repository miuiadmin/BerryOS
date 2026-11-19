//! berry-utrace 表驱动钉板（W3-Q23）——sched/src/util.rs 窗口机盒
//! （per-CPU 槽推进律）的 host 可测面。判序/折叠数学单源
//! berry-emface（本 crate 消费），此处钉「三数组怎么被三态结果改写」
//! 的存储机械 + 折叠对拍手工算例。窗长锚 `UTIL_WINDOW_VCT_REF` =
//! 625_000（QEMU 62.5MHz × 10ms），数值先独立推演再落测试。
//!
//! 纪律注：开窗须在 `now > 0` 处——`start == 0` 是未开窗哨兵，
//! `tick_window(cpu, 0)` 存 0 后下次仍判 NewWindow（kernel 同款
//! 哨兵碰撞边角，`boot_zero_now_reopens` 钉板之）。

use berry_emface::{ewma_fold, UTIL_NR_CPU, UTIL_WINDOW_VCT_REF};
use berry_utrace::{TickOutcome, UtilSlots};

const W: u64 = UTIL_WINDOW_VCT_REF; // 625_000

/// 满载单窗手工算例：accum=elapsed → raw=1024 → ewma 0→128。
/// 对拍 ewma_fold 手工值 + Folded 载荷 + 全字段状态断言。
#[test]
fn fold_math_hand_check() {
    let mut s = UtilSlots::<2>::new();
    assert_eq!(
        s.tick_window(0, 1, W),
        TickOutcome::NewWindow,
        "首调 start==0 开窗"
    );
    s.accumulate(0, W); // 满载整窗
    let out = s.tick_window(0, 1 + W, W);
    assert_eq!(
        out,
        TickOutcome::Folded {
            util_raw: 1024,
            ewma: 128
        }
    );
    // 手工对拍：ewma_fold 单源值一致（防本 crate 内第二实现漂移）。
    assert_eq!(ewma_fold(W, W, 0), 128);
    assert_eq!(s.read(0), 128);
    assert_eq!(s.accum[0], 0, "折叠后 accum 清零（swap 语义）");
    assert_eq!(s.window_start[0], 1 + W, "start ← now");
}

/// 首调开窗丢弃既往 accum：start==0 时不论账多少都不折不断，直接
/// store 0 + start=now（半窗账无窗长可比——记进首窗会虚高首样）。
#[test]
fn first_call_drops_prior_accum() {
    let mut s = UtilSlots::<2>::new();
    s.accumulate(0, 999_999); // 开窗前已记的账
    assert_eq!(s.tick_window(0, 42, W), TickOutcome::NewWindow);
    assert_eq!(s.accum[0], 0, "既往账丢弃");
    assert_eq!(s.window_start[0], 42);
    assert_eq!(s.ewma[0], 0, "ewma 不动");
    assert_eq!(s.read(0), 0);
}

/// 回绕防御 now<start 重开（正常不可达，计数器回绕保守路）：同
/// NewWindow 路——不折叠（防 elapsed 巨值破窗）、丢弃账、重开。
#[test]
fn wraparound_reopens() {
    let mut s = UtilSlots::<2>::new();
    s.tick_window(0, 10_000, W);
    s.accumulate(0, W);
    // now 回绕到 start 之前：window_state 判 NewWindow 优先于到期判。
    assert_eq!(s.tick_window(0, 5_000, W), TickOutcome::NewWindow);
    assert_eq!(s.accum[0], 0, "回绕窗账丢弃不折叠");
    assert_eq!(s.window_start[0], 5_000);
    assert_eq!(s.ewma[0], 0);
}

/// NotDue 零动作：未到期全三字段逐字不变（含已记 accum 原样保留），
/// 返回 Ignored。
#[test]
fn not_due_no_action() {
    let mut s = UtilSlots::<2>::new();
    s.tick_window(0, 1_000, W); // 开窗 @1_000
    s.accumulate(0, 123);
    let before = s;
    assert_eq!(s.tick_window(0, 1_000 + W - 1, W), TickOutcome::Ignored);
    assert_eq!(s, before, "NotDue 状态逐字段不变");
    // 半窗同样零动作（密集 tick 防过折叠——窗长是最短翻窗间隔）。
    assert_eq!(s.tick_window(0, 1_000 + W / 2, W), TickOutcome::Ignored);
    assert_eq!(s, before);
}

/// 窗口边界恰到期/未到期：elapsed == W 折 / W-1 不折（`<` 严格界）。
#[test]
fn window_boundary() {
    let mut a = UtilSlots::<2>::new();
    a.tick_window(0, 1_000, W);
    a.accumulate(0, W / 2);
    assert_eq!(a.tick_window(0, 1_000 + W - 1, W), TickOutcome::Ignored);
    // 恰到期：elapsed = W 整 → Fold。
    let mut b = UtilSlots::<2>::new();
    b.tick_window(0, 1_000, W);
    b.accumulate(0, W / 2);
    assert_eq!(
        b.tick_window(0, 1_000 + W, W),
        TickOutcome::Folded {
            util_raw: 512,
            ewma: 64
        },
        "半载整窗首折 raw=512 → 512>>3=64"
    );
    // 深睡跨多窗：elapsed = 10W 一步折（不做窗均摊——摊大 elapsed 自动
    // 稀释 busy 占比）。
    let mut c = UtilSlots::<2>::new();
    c.tick_window(0, 1_000, W);
    c.accumulate(0, W);
    assert_eq!(
        c.tick_window(0, 1_000 + 10 * W, W),
        TickOutcome::Folded {
            util_raw: 102, // W×1024/(10W) 截断 = 102
            ewma: 12       // 0 + 102>>3 = 12
        }
    );
}

/// 饱和钳位：accum×1024/elapsed 巨量不破 1024（超载账钳满格首折 128，
/// 与 emface `ewma_folding` 饱和锚同款）。
#[test]
fn saturation_clamp_1024() {
    let mut s = UtilSlots::<2>::new();
    s.tick_window(0, 1_000, W);
    s.accumulate(0, W.saturating_mul(1 << 40)); // 超载 2^40 倍
    assert_eq!(
        s.tick_window(0, 1_000 + W, W),
        TickOutcome::Folded {
            util_raw: 1024,
            ewma: 128
        },
        "钳位后同满载首折"
    );
    assert_eq!(s.read(0), 128);
}

/// 越界 cpu 全操作静默：accumulate/read/reset/tick_window 均不 panic
/// 不改状态（与 kernel IRQ 热路径同形——越界静默拒）。
#[test]
fn out_of_range_silent() {
    let mut s = UtilSlots::<2>::new();
    s.tick_window(0, 1_000, W);
    s.accumulate(0, 7);
    let before = s;
    s.accumulate(2, 1_000); // N=2：cpu 2 越界
    s.accumulate(999, 1);
    s.reset(2);
    assert_eq!(s.tick_window(2, 1_000 + W, W), TickOutcome::Ignored);
    assert_eq!(s.read(2), 0, "越界读恒 0（同 cpu_util 口径）");
    assert_eq!(s.read(usize::MAX), 0);
    assert_eq!(s, before, "越界写零痕");
}

/// 连续多窗满载收敛趋势：busy 满窗（accum=elapsed）每窗 +1/8 差，
/// 12 折至 815（< 819.2 = 80% 阈）——emface 满载收敛锚的机盒级复验，
/// 且逐窗单调升（PM 频率信号缓升律）。
#[test]
fn busy_windows_converge() {
    let mut s = UtilSlots::<2>::new();
    s.tick_window(0, 1, W);
    let mut now = 1u64;
    let mut prev = 0u32;
    for i in 1..=12u32 {
        now += W;
        s.accumulate(0, W); // 满载整窗
        let TickOutcome::Folded { util_raw, ewma } = s.tick_window(0, now, W) else {
            panic!("第 {i} 窗须 Fold");
        };
        assert_eq!(util_raw, 1024);
        assert!(ewma > prev, "满载链单调升（{prev} → {ewma}）");
        assert!(ewma < 1024);
        assert_eq!(ewma, ewma_fold(W, W, prev), "逐窗与单源对拍");
        prev = ewma;
    }
    assert_eq!(s.read(0), 815, "12 折满载至 815");
}

/// idle 空窗衰减：accum=0 多窗 → ewma 逐窗 -1/8 差直到死区后趋 0。
#[test]
fn idle_windows_decay_to_zero() {
    let mut s = UtilSlots::<2>::new();
    // 先武装一个高 ewma：满载 4 窗 → 128, 240, 338, 423（逐窗 +diff>>3）。
    s.tick_window(0, 1, W);
    let mut now = 1u64;
    for _ in 0..4 {
        now += W;
        s.accumulate(0, W);
        s.tick_window(0, now, W);
    }
    let armed = s.read(0);
    assert!(armed > 400, "武装基线须高（{armed}）");
    // 空窗序列：每窗 accum=0 折叠衰减。
    let mut prev = armed;
    let mut folds = 0u32;
    while prev > 7 && folds < 200 {
        now += W;
        let TickOutcome::Folded { util_raw, ewma } = s.tick_window(0, now, W) else {
            panic!("空窗到期须 Fold");
        };
        assert_eq!(util_raw, 0);
        assert!(ewma <= prev, "衰减链不升（{prev} → {ewma}）");
        assert_eq!(ewma, ewma_fold(0, W, prev), "逐窗与单源对拍");
        prev = ewma;
        folds += 1;
    }
    // 量化死区落底：差 <8 折 0——idle 衰减渐近停在 ≤7（0.7% 残留，
    // 对频率信号无害——emface `ewma_fold` 死区注同款口径）。
    assert!(prev <= 7, "死区落底 ≤7（实 {prev}，{folds} 折）");
    assert_eq!(prev % 8, prev, "死区内每折严格不动");
    now += W;
    s.tick_window(0, now, W);
    assert_eq!(s.read(0), prev, "死区内空窗零动作");
}

/// reset 清场：三字段全零 → 后续 tick 走首调开窗路（start==0 哨兵
/// 复活），与 kernel `util_reset`（ktest/boot 清场）同形。
#[test]
fn reset_clears_all() {
    let mut s = UtilSlots::<2>::new();
    s.tick_window(0, 1, W);
    s.accumulate(0, W);
    s.tick_window(0, 1 + W, W); // ewma=128
    assert_eq!(s.read(0), 128);
    s.reset(0);
    assert_eq!(UtilSlots::<2>::new(), s, "reset 后与全新构造逐字段相等");
    // start 哨兵复活：下一 tick 是 NewWindow 不是 Fold。
    assert_eq!(s.tick_window(0, 100 * W, W), TickOutcome::NewWindow);
    // 邻槽不受牵连。
    s.accumulate(1, 5);
    s.reset(0);
    assert_eq!(s.accum[1], 5, "邻槽账保留");
}

/// 槽独立性（per-CPU 隔离）：cpu0 满载折叠不扰 cpu1（读面/账面/
/// 窗口起点三轴互不串）。
#[test]
fn per_cpu_isolation() {
    let mut s = UtilSlots::<2>::new();
    s.tick_window(0, 1, W);
    s.tick_window(1, 1, W);
    s.accumulate(0, W);
    s.accumulate(1, W / 4);
    let now = 1 + W;
    let a = s.tick_window(0, now, W);
    let b = s.tick_window(1, now, W);
    assert_eq!(a, TickOutcome::Folded { util_raw: 1024, ewma: 128 });
    assert_eq!(b, TickOutcome::Folded { util_raw: 256, ewma: 32 });
    assert_eq!((s.read(0), s.read(1)), (128, 32));
}

/// N 泛化：非默认槽数（N=4）同机械；默认 N=UTIL_NR_CPU=2 与 kernel
/// static 口径对齐。
#[test]
fn generic_n_and_default() {
    let mut s = UtilSlots::<4>::new();
    assert_eq!(s.read(3), 0);
    assert_eq!(s.tick_window(3, 1, W), TickOutcome::NewWindow);
    s.accumulate(3, W);
    assert_eq!(
        s.tick_window(3, 1 + W, W),
        TickOutcome::Folded { util_raw: 1024, ewma: 128 }
    );
    assert_eq!(s.read(3), 128);
    assert_eq!(s.tick_window(4, 1, W), TickOutcome::Ignored, "N=4 越界");
    // 默认参 N = UTIL_NR_CPU。
    let d: UtilSlots = UtilSlots::new();
    let _k: UtilSlots<UTIL_NR_CPU> = d;
}

/// 判序与 accum 无关（真实数据流还原）：判序只用 start/now/len，折叠
/// 用真 accum——纯面直传与 kernel 传 0 的优化形判序结果一致
/// （window_state 对 accum 载荷透传不判）。
#[test]
fn ordering_independent_of_accum() {
    // 同 start/now、不同 accum：判序三态相同（Fold 载荷不同）。
    let mut a = UtilSlots::<2>::new();
    a.tick_window(0, 1_000, W);
    let mut b = a;
    a.accumulate(0, W);
    b.accumulate(0, 0);
    let ra = a.tick_window(0, 1_000 + W, W);
    let rb = b.tick_window(0, 1_000 + W, W);
    assert!(matches!(ra, TickOutcome::Folded { .. }));
    assert!(matches!(rb, TickOutcome::Folded { .. }));
    // NotDue/NewWindow 态下 accum 多少都不改判序。
    let mut c = UtilSlots::<2>::new();
    c.accumulate(0, 42);
    assert_eq!(c.tick_window(0, 7, W), TickOutcome::NewWindow);
    let mut d = UtilSlots::<2>::new();
    d.tick_window(0, 1_000, W);
    d.accumulate(0, W.saturating_mul(2));
    assert_eq!(d.tick_window(0, 1_000 + W - 1, W), TickOutcome::Ignored);
}

/// EWMA 量化死区在机盒面的投影：差 <8 折 0（util_raw 与 ewma 差 3
/// 不动 / 差 8 前进 1 / 负差 3 不动），与 emface `ewma_folding` 死区锚
/// （elapsed=1024 纯数学面）同款数值、机盒推进复验。
#[test]
fn quantization_deadzone_through_slots() {
    // elapsed=1024 使 raw=accum 恰直读（纯数学面注入）。
    let mut s = UtilSlots::<2>::new();
    s.tick_window(0, 1, 1024);
    s.accumulate(0, 823); // raw=823 vs ewma=0：非死区（差 823）
    s.tick_window(0, 1 + 1024, 1024); // → 823>>3 = 102
    assert_eq!(s.read(0), 102);
    // 直接武装 ewma=820 再验死区两侧。
    let mut t = UtilSlots::<2>::new();
    t.ewma[0] = 820;
    t.tick_window(0, 1, 1024);
    t.accumulate(0, 823); // 正差 3 → 不动
    assert_eq!(
        t.tick_window(0, 1 + 1024, 1024),
        TickOutcome::Folded { util_raw: 823, ewma: 820 }
    );
    let mut u = UtilSlots::<2>::new();
    u.ewma[0] = 820;
    u.tick_window(0, 1, 1024);
    u.accumulate(0, 828); // 正差 8 → 前进 1
    assert_eq!(
        u.tick_window(0, 1 + 1024, 1024),
        TickOutcome::Folded { util_raw: 828, ewma: 821 }
    );
    let mut v = UtilSlots::<2>::new();
    v.ewma[0] = 820;
    v.tick_window(0, 1, 1024);
    v.accumulate(0, 817); // 负差 3 → 不动
    assert_eq!(
        v.tick_window(0, 1 + 1024, 1024),
        TickOutcome::Folded { util_raw: 817, ewma: 820 }
    );
}

/// boot 零点哨兵碰撞边角（kernel 同款行为钉板）：`tick_window(cpu, 0)`
/// 开窗存 start=0——哨兵未消，下次 tick 仍判 NewWindow 重开（丢弃
/// 期间账）。CNTVCT==0 只在 boot 瞬时存在，首个 tick 前 CNTVCT 已
/// 非零，正常不可达；本测钉住「哨兵 0 与首开窗 now=0 碰撞」的诚实
/// 语义（不修 kernel——两实现逐字同形）。
#[test]
fn boot_zero_now_reopens() {
    let mut s = UtilSlots::<2>::new();
    assert_eq!(s.tick_window(0, 0, W), TickOutcome::NewWindow);
    s.accumulate(0, 5); // 哨兵窗期记的账
    // start 仍 == 0（哨兵）→ 再判 NewWindow：账丢弃、不折叠。
    assert_eq!(s.tick_window(0, W, W), TickOutcome::NewWindow);
    assert_eq!(s.accum[0], 0);
    assert_eq!(s.ewma[0], 0);
    assert_eq!(s.window_start[0], W);
}

/// Default 构造与 const new 等价（静态可用性——host/ktest 均可静态
/// 初始化，与 kernel static 原子数组初值口径一致）。
#[test]
fn default_matches_new() {
    assert_eq!(UtilSlots::<2>::default(), UtilSlots::<2>::new());
    const S: UtilSlots<2> = UtilSlots::new();
    assert_eq!(S.ewma, [0, 0]);
    assert_eq!(S.accum, [0, 0]);
    assert_eq!(S.window_start, [0, 0]);
}
