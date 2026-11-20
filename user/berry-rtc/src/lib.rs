//! # berry-rtc —— 用户态 RTC 消费面（epoch 读取 + UTC 历法换算）
//!
//! 驱动子系统研究报告（2026-11-18）P1-7 的用户侧半件：内核壁钟已在席
//! （fw_cfg `opt/berry/rtc` 宿主注入 epoch + 单调锚，
//! `berry_clock_read(CLOCK_REALTIME)` 出纳秒），用户态却零公共消费库——
//! launcher/appshell 各自单点自持（直取 syscall + `hhmm_utc` 的 ns 口径
//! 5B "HH:MM" 面），秒级/日期级历法换算全仓无处可拿。本 crate 收口两
//! 件事：epoch 读取（syscall 单源经 berry-sys，ABI 镜像禁第二套）+ 纯
//! 函数历法换算（epoch 秒 → "HH:MM:SS" UTC / epoch 秒 → 年-月-日）。日期核心 = 简化
//! 儒略日算法（Howard Hinnant civil_from_days 形状：日数 + 719468 平移到
//! 3 月起年 → 146097 日/400 年 era 分解 → (5·doy+2)/153 月序——闰年判定
//! 收进 era 算术自身，无分支表；u64 epoch 恒非负 → 负值支路整体裁掉）。
//!
//! 时基口径：本 crate 全部以 **epoch 秒** 为参（[`read_epoch`] 与格式化
//! 函数同一时基；POSIX time_t 的常规教学口径）。内核 REALTIME 出 ns、
//! launcher 现状自行 /1e9——ns 精度面（时钟角标每秒续摆的亚秒抖动）在
//! 消费接线刀再裁。
//!
//! 形态照 berry-layout/berry-softime 家族：`cfg_attr(not(test), no_std)`
//! + host 测试；syscall 面 `cfg(not(test))`（berry-fsclient 纪律——host
//! 测试二进制零 svc 面，svc 在宿主上执行即 SIGILL）。消费接线（launcher
//! 时钟改经本 crate、appshell::scene::hhmm_utc 归一到本面）挂账后续刀
//! ——本刀纯新增面，零行为变化。

#![cfg_attr(not(test), no_std)]

// —— epoch 读取（syscall 面；cfg(not(test))——host 测试零 svc 面）——

/// 壁钟 epoch 秒：`berry_clock_read(CLOCK_REALTIME)` ns → 秒（截断不四舍
/// 五入——时钟读旧不读新）。注入缺席（-ENOTSUP——宿主未配 fw_cfg
/// `opt/berry/rtc` 键，如 `-kernel` 自建拓扑）→ 0：与真值 1970-01-01
/// 00:00:00 兼双义，消费面要区分用 [`read_epoch_opt`]。
#[cfg(not(test))]
pub fn read_epoch() -> u64 {
    read_epoch_opt().unwrap_or(0)
}

/// 壁钟 epoch 秒的诚实面：`None` = 壁钟不可用（内核 -ENOTSUP——宿主未
/// 注入 rtc 键）；`Some(秒)` = epoch 秒。
#[cfg(not(test))]
pub fn read_epoch_opt() -> Option<u64> {
    let mut ns: berry_sys::Time = 0;
    let st = berry_sys::berry_clock_read(berry_sys::CLOCK_REALTIME, &mut ns);
    if st == 0 {
        Some(ns / 1_000_000_000)
    } else {
        None
    }
}

// —— 历法换算（纯函数，host 可测）——

/// epoch 秒 → 当日时分秒三件（值域 h 0..=23 / m·s 0..=59——`% 86400`
/// 先落当日，跨日回卷由取模自证）。
pub fn hms_utc(epoch_s: u64) -> (u8, u8, u8) {
    let sod = epoch_s % 86_400;
    ((sod / 3600) as u8, ((sod % 3600) / 60) as u8, (sod % 60) as u8)
}

/// epoch 秒 → "HH:MM:SS"（UTC；恒 8B 等长——等宽字体换文零重测的契约
/// 同 appshell 时钟角标 "HH:MM" 先例）。
pub fn hhmmss_utc(epoch_s: u64) -> [u8; 8] {
    let (h, m, s) = hms_utc(epoch_s);
    let mut out = [0u8; 8];
    put2(&mut out[0..2], h as u64);
    out[2] = b':';
    put2(&mut out[3..5], m as u64);
    out[5] = b':';
    put2(&mut out[6..8], s as u64);
    out
}

/// UTC 日期三件（简化儒略日算法的值域产物：year ≥ 1970、month 1..=12、
/// day 1..=31——u64 epoch 全域无 panic 无分配）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateUtc {
    /// 公元年（1970 起；epoch 上限下可达千亿级——文本面域外截断见
    /// [`ymd_utc_text`]）。
    pub year: u64,
    pub month: u8,
    pub day: u8,
}

/// epoch 秒 → UTC 年-月-日（简化儒略日算法）。三步：日数 `epoch/86400`
/// + 719468 平移（1970-01-01 → 0 年 3 月 1 日起算的儒略纪元）→ era
/// 分解（400 年 = 146097 日整周期，闰规则全部吸收在 `yoe` 算术里）→
/// 月序 `(5·doy+2)/153`（3 月起年——2 月永远是年尾，闰日 = 年尾多一日，
/// 无需任何 if is_leap 分支）。
pub fn ymd_utc(epoch_s: u64) -> DateUtc {
    let z = epoch_s / 86_400 + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097; // [0, 146096]：era 内日序
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]：年内日序（3 月起）
    let mp = (5 * doy + 2) / 153; // [0, 11]：月序（0 = 三月）
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let year = yoe + era * 400 + u64::from(m <= 2); // 1/2 月属下一年（3 月起年）
    DateUtc { year, month: m as u8, day: d as u8 }
}

/// epoch 秒 → "YYYY-MM-DD"（UTC；恒 10B 等长）。年份 4 位值域
/// 1970..=9999（epoch ≤ 253_402_300_799）；域外年份高位截断
/// （`year % 10_000`——显式截断非真相，教学值域内结构性不可达，钉板
/// 行为见测试 `four_digit_year_ceiling`）。
pub fn ymd_utc_text(epoch_s: u64) -> [u8; 10] {
    let date = ymd_utc(epoch_s);
    let y = date.year % 10_000;
    let mut out = [0u8; 10];
    out[0] = b'0' + (y / 1000) as u8;
    out[1] = b'0' + (y / 100 % 10) as u8;
    out[2] = b'0' + (y / 10 % 10) as u8;
    out[3] = b'0' + (y % 10) as u8;
    out[4] = b'-';
    put2(&mut out[5..7], date.month as u64);
    out[7] = b'-';
    put2(&mut out[8..10], date.day as u64);
    out
}

/// 两位十进制注入（v < 100 由调用点值域保证——时/分/秒/月/日全部两位域）。
fn put2(buf: &mut [u8], v: u64) {
    buf[0] = b'0' + (v / 10) as u8;
    buf[1] = b'0' + (v % 10) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 已知向量（Python datetime UTC 独立对拍，2026-11-20）：epoch 秒 →
    /// 日期 + 时间双面同钉。
    #[test]
    fn known_datetimes() {
        // (epoch 秒, "YYYY-MM-DD", "HH:MM:SS")
        let v: &[(u64, &[u8; 10], &[u8; 8])] = &[
            (0, b"1970-01-01", b"00:00:00"),
            (1, b"1970-01-01", b"00:00:01"),
            (59, b"1970-01-01", b"00:00:59"),
            (60, b"1970-01-01", b"00:01:00"),
            (3599, b"1970-01-01", b"00:59:59"),
            (3600, b"1970-01-01", b"01:00:00"),
            (86_399, b"1970-01-01", b"23:59:59"),
            (86_400, b"1970-01-02", b"00:00:00"), // 跨日
            (86_461, b"1970-01-02", b"00:01:01"),
            (1_000_000_000, b"2001-09-09", b"01:46:40"),
            (1_700_000_000, b"2023-11-14", b"22:13:20"),
            (1_735_689_600, b"2025-01-01", b"00:00:00"),
        ];
        for &(s, d, t) in v {
            assert_eq!(ymd_utc_text(s), *d, "epoch {s}");
            assert_eq!(hhmmss_utc(s), *t, "epoch {s}");
        }
    }

    /// 闰年三类边界（简化儒略日算法的执法面）：4 年闰 / 100 年不闰 /
    /// 400 年闰——全部向量 Python 独立对拍。
    #[test]
    fn leap_rules_three_kinds() {
        let date = |y: u64, m: u8, d: u8| DateUtc { year: y, month: m, day: d };
        // 400 年闰：2000-02-29 在席。
        assert_eq!(ymd_utc(951_782_400), date(2000, 2, 29));
        // 4 年闰：2024-02-29 在席（正午时刻向量）。
        assert_eq!(ymd_utc(1_709_208_000), date(2024, 2, 29));
        assert_eq!(hhmmss_utc(1_709_208_000), *b"12:00:00");
        // 100 年不闰：2100-02-28 即二月末。
        assert_eq!(ymd_utc(4_107_456_000), date(2100, 2, 28));
        // 次日直接 03-01（2100 二月仅 28 天——与闰年的差异收在这一日）。
        assert_eq!(ymd_utc(4_107_542_400), date(2100, 3, 1));
        assert_eq!(4_107_542_400u64 - 4_107_456_000u64, 86_400);
        // 对照：闰年 2020 二月 29 天（02-28/02-29/03-01 三连向量）。
        assert_eq!(ymd_utc(1_582_848_000), date(2020, 2, 28));
        assert_eq!(ymd_utc(1_582_934_400), date(2020, 2, 29));
        assert_eq!(ymd_utc(1_583_020_800), date(2020, 3, 1));
    }

    /// 四位年值域上界（Y10K 边界）：域内末日 23:59:59 钉板；域外首秒
    /// 年份截断行为（10000 % 10_000 → "0000"）显式钉板。
    #[test]
    fn four_digit_year_ceiling() {
        assert_eq!(ymd_utc(253_402_300_799), DateUtc { year: 9999, month: 12, day: 31 });
        assert_eq!(ymd_utc_text(253_402_300_799), *b"9999-12-31");
        assert_eq!(hhmmss_utc(253_402_300_799), *b"23:59:59");
        assert_eq!(ymd_utc(253_402_300_800), DateUtc { year: 10_000, month: 1, day: 1 });
        assert_eq!(ymd_utc_text(253_402_300_800), *b"0000-01-01");
    }

    /// 时分秒值域钉板：整分/整时/日末回卷（`% 86400` 语义，对照 appshell
    /// hhmm_utc 的 %24 同族）。
    #[test]
    fn hms_wraps_at_day_boundary() {
        assert_eq!(hms_utc(0), (0, 0, 0));
        assert_eq!(hms_utc(86_400 + 7 * 3600 + 5 * 60 + 9), (7, 5, 9)); // 跨日余量
        assert_eq!(hms_utc(2 * 86_400 + 86_399), (23, 59, 59));
    }
}
