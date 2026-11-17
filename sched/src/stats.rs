//! 调度可观测性计数面（OBS-01——06 号 §5 观测维 7.5 扣分项的最小补面，
//! 2026-11-16）：per-band pick/runtime 累积 + per-intent pick 累积 + 周期
//! `SCHED` 串口行。
//!
//! 设计约束（全部承既有纪律）：
//! - **纯统计、非合同面**：ktest 对本面零断言（观测行不进门禁——与 hb/
//!   stuck 诊断行同政策，但本面是**长期观测面**非排障探针，不随定谳退役
//!   ——06 号「跨核唤醒上界不可测」的补面第一步）。
//! - **Relaxed 原子、IRQ 热路径零锁零分配**：写点 = pick/tick 热路径
//!   （屏蔽窗/锁窗内），fetch_add 单指令序无同步角色（`LB_TICKS` 同款
//!   口径；`IRQ_TICKS` 机会三〔2026-11-17〕已退役）；读点 = 串口行拼装。原子 static（非
//!   `static mut`）免入册 `tools/writable_globals.txt`（`band.rs::
//!   DEADLINES` 注同款 A12 纪律）。
//! - **跨核一份全局账**：与 `band.rs::CONSUMED` 同裁决——per-band 计数
//!   是系统级观测信号（带级公平性/吞吐对比），非 per-CPU 决策输入；
//!   双核各自 fetch_add 自然聚合。
//!
//! 记账点（消费面单源，别处不重复记账）：
//! - pick：`class.rs::Fair::pick_next_filtered`——生产出选**唯一**路径
//!   （sched.rs 八调用点：yield/exit/block 重验/tick 抢占/try_preempt/
//!   park 派发族全经此；`SchedClass::pick_next` 是 ktest 直调面不经此，
//!   生产不可达——记「调度器出选决策」而非「上下文切换」，picked==prev
//!   的续选也计〔出选决策确已发生〕）。
//! - runtime：`class.rs::FairBand::tick`——delta 口径 = `peek_tick_delta_at`
//!   （与 `Fair::tick` 记账同一基准的只读孪生；04 号审计机会四②
//!   （2026-11-17）起 now 自 tick 入口单读线程化下发——与 `Fair::tick`
//!   正规读的差 = 两次 cntvct 读之间的指令窗，微秒以下）。unit Fair
//!   路径零触碰（unit 任务零漂移纪律）。
//!
//! 【推】runtime 账面粒度 = tick 采样（片边界 + oneshot 提前醒），非全量
//! 计时——与 `Fair::tick` 的 slice 记账同粒度，作带间**相对**对比信号
//! 足够，绝对时长以 slice_left 口径为准。

use core::sync::atomic::{AtomicU64, Ordering};

use berry_hal_aarch64 as hal;

use crate::band::BAND_SLOTS;
use crate::class::TaskClass;
use crate::task::Task;

/// per-band pick 计数（索引 = band id = intent 档编码；`class_of_task` 的
/// Band 臂才有账——unit 任务不入带账，intent 全档账在 [`INTENT_PICKS`]）。
static BAND_PICKS: [AtomicU64; BAND_SLOTS] = [const { AtomicU64::new(0) }; BAND_SLOTS];

/// per-band 累积运行周期数（CNTVCT cycles——`FairBand::tick` 的 delta 累加；
/// 换算时长 = rt / `cntfrq()`）。
static BAND_RUNTIME: [AtomicU64; BAND_SLOTS] = [const { AtomicU64::new(0) }; BAND_SLOTS];

/// per-intent pick 计数槽位数（编码域 0x00..=0x05 + 越域防御余量；键 =
/// `intent_effective`——§11「intent 即带键」，含 unit 档 0x00 的全体出选）。
const INTENT_SLOTS: usize = 8;

/// per-intent pick 计数（吞吐面：INTERACTIVE vs UTILITY 的出选占比可直接
/// 读出——06 号「per-intent 吞吐计数」最小兑现）。
static INTENT_PICKS: [AtomicU64; INTENT_SLOTS] = [const { AtomicU64::new(0) }; INTENT_SLOTS];

/// intent 档名（串口行用；索引 = 档编码——与 `berry_intface` 编码冻结同序）。
const INTENT_NAMES: [&[u8]; 6] = [
    b"UNSPEC", b"LATCRIT", b"INTERACTIVE", b"UTILITY", b"BACKGROUND", b"MAINT",
];

/// pick 记账（`Fair::pick_next_filtered` 出选后调）：intent 全档 +1；
/// 带任务（`class_of_task` Band 臂）另入 per-band 账。屏蔽窗内调用
/// （pick 语境）——Relaxed 纯计数。
pub(crate) fn note_pick(t: &Task) {
    let intent = t.intent_effective as usize;
    if intent < INTENT_SLOTS {
        INTENT_PICKS[intent].fetch_add(1, Ordering::Relaxed);
    }
    if let TaskClass::Band(fb) = crate::class::class_of_task(t) {
        // band_id 域 = intent 现役集（< INTENT_SLOTS < BAND_SLOTS），界检防御。
        let b = fb.band_id as usize;
        if b < BAND_SLOTS {
            BAND_PICKS[b].fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// runtime 记账（`FairBand::tick` 调；`cycles` = 本拍 delta 周期数）。
pub(crate) fn note_runtime(band: u8, cycles: u64) {
    let b = band as usize;
    if b < BAND_SLOTS {
        BAND_RUNTIME[b].fetch_add(cycles, Ordering::Relaxed);
    }
}

/// per-band pick 计数读出口（telemetry/DUMP 消费面预留）。
pub fn band_picks(band: u8) -> u64 {
    if (band as usize) < BAND_SLOTS {
        BAND_PICKS[band as usize].load(Ordering::Relaxed)
    } else {
        0
    }
}

/// per-band 累积 runtime 读出口（CNTVCT 周期数）。
pub fn band_runtime(band: u8) -> u64 {
    if (band as usize) < BAND_SLOTS {
        BAND_RUNTIME[band as usize].load(Ordering::Relaxed)
    } else {
        0
    }
}

/// per-intent pick 计数读出口。
pub fn intent_picks(intent: u8) -> u64 {
    if (intent as usize) < INTENT_SLOTS {
        INTENT_PICKS[intent as usize].load(Ordering::Relaxed)
    } else {
        0
    }
}

// ---- SCHED 周期串口行 ----

/// 节流锚（上次 SCHED 行的 cntvct；0 起点 = boot 后首窗即过门——首行
/// 约 5s 虚拟时打出，boot 期 fleet 已跑、账面非全零）。CAS 单赢家：
/// 双核同窗竞争只有 CAS 成功者打行（`HB_LAST` swap 单赢家同义，但 delta
/// 节流不能用 swap——锚被窗内调用持续前推 = 滑窗永不到期，故 CAS 形）。
static SCHED_LINE_LAST: AtomicU64 = AtomicU64::new(0);

/// 行间隔（虚拟秒；cntvct/cntfrq 换算——`spin_bark` 1s 节流同源数学，
/// 取 5s：smoke 数分钟窗 ≈ 数十行内、boot 期必见 ≥1 行）。
const SCHED_LINE_SECS: u64 = 5;

/// SCHED 行轮询（`sched.rs::on_irq_tick` 的 slice-燃尽 + preempt_count==0
/// 区调——hb 行同语境：G43 防 PRINT_LOCK 持窗自死锁的既有门内）。节流
/// 过门才拼行：`with_print_lock` 行原子（hb 先例），打印内容 =
/// intent 全档 picks + 非零带的 picks/runtime。OBS-01 观测面——不进门禁。
pub(crate) fn sched_line_poll() {
    let now = hal::timer::cntvct();
    let interval = hal::timer::cntfrq().saturating_mul(SCHED_LINE_SECS);
    if interval == 0 {
        return; // cntfrq 未缓存（timer::init 前）——防御面，不可达
    }
    let last = SCHED_LINE_LAST.load(Ordering::Relaxed);
    if now.saturating_sub(last) < interval {
        return; // 窗内（锚不动——load 判+CAS 写两段，无滑窗面）
    }
    if SCHED_LINE_LAST
        .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return; // 他核同窗先赢（锚已推进）——本窗已打/将打，退
    }
    hal::kern_lock::with_print_lock(|| {
        hal::uart_puts(b"SCHED picks i:");
        for i in 0..INTENT_NAMES.len() {
            hal::uart_puts(INTENT_NAMES[i]);
            hal::uart_puts(b"=");
            hal::uart_put_dec(intent_picks(i as u8));
            if i + 1 < INTENT_NAMES.len() {
                hal::uart_puts(b" ");
            }
        }
        // 非零带账（band id = intent 编码，现役集 0x02/0x03）。
        let mut any_band = false;
        for b in 0..BAND_SLOTS as u8 {
            let p = band_picks(b);
            if p == 0 {
                continue;
            }
            if !any_band {
                hal::uart_puts(b" | band");
                any_band = true;
            } else {
                hal::uart_puts(b" band");
            }
            hal::uart_put_dec(b as u64);
            hal::uart_puts(b" picks=");
            hal::uart_put_dec(p);
            hal::uart_puts(b" rt=");
            hal::uart_put_dec(band_runtime(b));
            hal::uart_puts(b"cy");
        }
        hal::uart_puts(b"\n");
    });
}
