//! berry-itally —— intent 档位计数/通胀计数遥测纯函数面（W3-Q6）。
//!
//! **动机**：4-3c intent 传播三计数器（borrows/hop_capped/budget_capped——
//! 「值通胀」的可观测信号，`IPC设计.md` §2.1 五件套之 5）在
//! `kernel/src/telemetry.rs` 只有裸原子加法与读点，**档位分布**（哪些 intent
//! 档在被结算/借入）与**通胀程度**（借入抬升的档位差合计——「一切皆顶带 =
//! 没有 band」的量化面）无处落盘；`tele:` 行落盘格式（`tools/advisor.py`
//! F6 第一级 schema 消费面）散在 `main.rs::tele_dump` 手拼裸串——host 测
//! 不可达（kernel 依赖 HAL global_asm，只 aarch64-unknown-none 可编）。
//!
//! 本 crate 按值域抽成纯函数 + 静态计数面 + 行格式器，host 表驱动钉板：
//! - [`IntentTally`]：六档位计数器（`#[repr(C)]` 定容 6 槽——0x00–0x05
//!   值直索引；保留域 0x06+ **不设槽不拒记**：`tally_key` 收敛 UNSPECIFIED
//!   行，与 `intent_index`「保留域回落保守」同构——计数面永不丢账、永不
//!   越界）。
//! - [`inflate`]：借入档位差（declared − effective，u8 值越小档越高——差
//!   即被抬升的档数；非借入恒 0）。
//! - [`TallyLine`]：`tele:` 行装配纯函数——`intent_tally u0=.. i1=..`
//!   档位直方 + `borrowed=.. infl=.. hop_cap=.. bud_cap=..` 通胀四联，定
//!   长 ASCII 缓冲写出（内核 uart 直写零分配纪律同 `BENCHWAKE` 行）。
//!
//! 语义钉板（表驱动测试逐条钉）：
//! - 计数不变量：borrowed = Σ inflate>0 的结算数（借入生效 ⇔ 档位差>0）；
//!   通胀累计 = Σ inflate（每次借入抬升的档数合计）。
//! - 落盘格式：单行纯 ASCII、`k=v` 空格分隔、无尾随空格、非零档省略行
//!   （全零 = `intent_tally none`——advisor.py 面自然忽略不误判）。
//! - 键名冻结：`intent_tally`/`borrowed`/`infl`/`hop_cap`/`bud_cap`——
//!   advisor.py 的 per-task 行收割面不动，本行是**新增摘要行族**（同
//!   `tele: sw/wake/billing` 摘要行——不认即自然忽略，schema 向后兼容）。
//!
//! 与冻结面的关系：syscall ABI 零触碰（纯内核侧遥测面）；intent u8 编码
//! 单源 = `berry_sched::intent`（本 crate 经 `berry_iprofile::intent_index`
//! 消费，不二写编码）。kernel 消费点 = `telemetry.rs`（计数器族）+
//! `main.rs::tele_dump`（行装配）——接线零第二实现。

#![cfg_attr(not(test), no_std)]

use core::sync::atomic::{AtomicU64, Ordering};

// ---- 档位计数槽（值直索引定容表）----

/// 档位槽容量（0x00 UNSPECIFIED ..= 0x05 MAINTENANCE 六行全槽——含编码
/// 冻结但 v1 不启用的 MAINTENANCE：计数面先落槽，启用日零迁移）。
pub const INTENT_SLOTS: usize = 6;

/// 单档计数器族（`#[repr(C)]`：将来 `telemetry_open` 映射的 ABI 形状
/// day-one 定型；字段全定宽）。
#[derive(Debug, Default)]
#[repr(C)]
pub struct IntentSlot {
    /// 本档结算次数（recv 结算 `intent_effective` 落此档——含 Own 常态与
    /// Borrowed 借入后的档位）。
    pub settles: AtomicU64,
    /// 本档借入生效次数（SettleOut::Borrowed 且落此档）。
    pub borrows: AtomicU64,
}

impl IntentSlot {
    /// 全零 const 构造（[`IntentTally::NEW`] 槽件）。
    pub const NEW: Self = Self {
        settles: AtomicU64::new(0),
        borrows: AtomicU64::new(0),
    };
}

/// 静态计数表（kernel 侧 `static` 装载；host 测自建实例驱动——计数语义
/// 在 [`record`] 纯投影，槽表只是定容容器）。`NEW` = const 构造器
/// （`Default` 非 const 进不了 static——同 crate `AtomicU64` 全零
/// const new 先例）。
#[derive(Debug, Default)]
#[repr(C)]
pub struct IntentTally {
    slots: [IntentSlot; INTENT_SLOTS],
    /// 通胀累计（Σ 每次借入的档位差——「值通胀」的量化面：高档客户端把
    /// 低档 server 抬升的总档数）。
    inflate_total: AtomicU64,
}

impl IntentTally {
    /// 全零 const 构造（kernel `static` 装载位）。
    pub const NEW: Self = Self {
        slots: [
            IntentSlot::NEW, IntentSlot::NEW, IntentSlot::NEW,
            IntentSlot::NEW, IntentSlot::NEW, IntentSlot::NEW,
        ],
        inflate_total: AtomicU64::new(0),
    };
}

/// intent u8 → 档位槽键（保留域 0x06+ 收敛 0 号 UNSPECIFIED 行——计数面
/// 永不丢账永不越界；与 `berry_iprofile::intent_index`「保留域回落」同构，
/// 但**不镜像编码**：只消费其返回值）。
#[inline]
pub fn tally_key(v: u8) -> usize {
    match berry_iprofile::intent_index(v) {
        Some(i) => i,
        None => 0,
    }
}

/// 借入档位差：`(declared, effective) → 抬升档数`。u8 值越小档越高，借入
/// = effective < declared；非借入/回落（effective ≥ declared）恒 0。
/// **不饱和下溢**：call 方保证 settled 档来自 `min(declared, msg)` 结算律，
/// effective > declared 的「负通胀」不存在——防御性 `saturating_sub` 收口。
#[inline]
pub fn inflate(declared: u8, effective: u8) -> u64 {
    (declared.saturating_sub(effective)) as u64
}

/// 记一次结算的纯投影：返回 `(槽键, 借入档差)`——kernel 写点据此打槽表
/// 原子（写点纪律 = `telemetry.rs` 计数器族先例：Relaxed fetch_add，单写者
/// 锁窗内、读者锁窗外安全）。本函数零原子零副作用，host 直测。
#[inline]
pub fn record(declared: u8, effective: u8) -> (usize, u64) {
    (tally_key(effective), inflate(declared, effective))
}

impl IntentTally {
    /// 写点：记一次结算（Own/Borrowed 皆记档位分布；借入另累通胀）。
    pub fn note_settle(&self, declared: u8, effective: u8) {
        let (key, infl) = record(declared, effective);
        self.slots[key].settles.fetch_add(1, Ordering::Relaxed);
        if infl > 0 {
            self.slots[key].borrows.fetch_add(1, Ordering::Relaxed);
            self.inflate_total.fetch_add(infl, Ordering::Relaxed);
        }
    }

    /// 读点：`(档位, 结算数, 借入数)` 迭代面（打印/断言零拷贝快照走
    /// [`snapshot`]）。
    pub fn at(&self, key: usize) -> (u64, u64) {
        let s = &self.slots[key.min(INTENT_SLOTS - 1)];
        (s.settles.load(Ordering::Relaxed), s.borrows.load(Ordering::Relaxed))
    }

    /// 通胀累计读点。
    pub fn inflate_get(&self) -> u64 {
        self.inflate_total.load(Ordering::Relaxed)
    }

    /// 全零判（零样本行省略面——`tele:` 行只在本表有货时打）。
    pub fn is_empty(&self) -> bool {
        self.inflate_get() == 0 && (0..INTENT_SLOTS).all(|i| self.at(i).0 == 0)
    }
}

// ---- tele: 行落盘格式（advisor.py F6 消费面同族摘要行）----

/// 单行缓冲上界：头 `tele: intent_tally`（19B）+ 六档 ` u0=18446744073709551615`
/// 最坏 22B×6 + 通胀四联 ` borrowed=.. infl=.. hop_cap=.. bud_cap=..`
/// 最坏 4×(10+20)B + 换行 = 19 + 132 + 121 + 1 = 273B（u64 十进制 20 位
/// 上界算术；非零省略远低于此——实践界与 BENCHWAKE 同量级）。
pub const LINE_MAX: usize = 320;

/// u64 → 十进制 ASCII（定长缓冲写、返回写出长度；kernel `tele_dump` 与
/// 本 crate 行装配共用的最小 fmt 面——`uart_put_dec` 的值域纯投影）。
pub fn fmt_u64(buf: &mut [u8], v: u64) -> usize {
    let mut tmp = [0u8; 20];
    let mut n = 0;
    let mut x = v;
    loop {
        tmp[n] = b'0' + (x % 10) as u8;
        n += 1;
        x /= 10;
        if x == 0 {
            break;
        }
    }
    let len = n.min(buf.len());
    for i in 0..len {
        buf[i] = tmp[n - 1 - i];
    }
    len
}

/// 追加辅助：`k=v` 段（含前置空格）——返回新长度；缓冲满即截断返回原长
/// （行上界 `LINE_MAX` 保证不触；防御性不 panic）。
fn push_kv(buf: &mut [u8], len: usize, key: &[u8], val: u64) -> usize {
    let mut l = len;
    if l > 0 && l < buf.len() {
        buf[l] = b' ';
        l += 1;
    }
    if l < buf.len() {
        let k = key.len().min(buf.len() - l);
        buf[l..l + k].copy_from_slice(&key[..k]);
        l += k;
        if l < buf.len() {
            buf[l] = b'=';
            l += 1;
            l += fmt_u64(&mut buf[l..], val);
        }
    }
    l
}

/// 档位短名（行键）：`u0`..`u5`——u = intent 档（值越小档越高）；与
/// advisor.py per-task 行的 `intent=` 数值域同源（0x00–0x05），摘要行族
/// 前缀区分（`u` 档位直方 vs `intent` 单值）。
fn slot_key(i: usize) -> [u8; 2] {
    [b'u', b'0' + i as u8]
}

/// `tele:` 行装配纯函数：`tally` 快照 + 通胀四联计数 → 定长 ASCII 行
/// （无尾随空格、含换行；返回写出长度）。**非零省略**：全零表 →
/// `tele: intent_tally none\n`（advisor 面自然忽略）；六档只打非零档。
pub fn tally_line(
    tally: &IntentTally,
    borrowed: u64,
    hop_cap: u64,
    bud_cap: u64,
    out: &mut [u8; LINE_MAX],
) -> usize {
    let mut l = 0;
    let head = b"tele: intent_tally";
    let h = head.len().min(out.len());
    out[..h].copy_from_slice(&head[..h]);
    l = h;
    if tally.is_empty() {
        let tail = b" none\n";
        let t = tail.len().min(out.len() - l);
        out[l..l + t].copy_from_slice(&tail[..t]);
        return l + t;
    }
    for i in 0..INTENT_SLOTS {
        let (s, b) = tally.at(i);
        if s > 0 {
            let k = slot_key(i);
            l = push_kv(out, l, &k, s);
            // 借入数只在非零时跟写（同档 `u3=5/2` 会破 k=v 面——斜杠后置）。
            if b > 0 {
                let mut seg = [0u8; 26]; // "/18446744073709551615" 上界 21B
                let mut sl = 0;
                seg[sl] = b'/';
                sl += 1;
                sl += fmt_u64(&mut seg[sl..], b);
                let c = sl.min(out.len().saturating_sub(l));
                out[l..l + c].copy_from_slice(&seg[..c]);
                l += c;
            }
        }
    }
    l = push_kv(out, l, b"borrowed", borrowed);
    l = push_kv(out, l, b"infl", tally.inflate_get());
    l = push_kv(out, l, b"hop_cap", hop_cap);
    l = push_kv(out, l, b"bud_cap", bud_cap);
    if l < out.len() {
        out[l] = b'\n';
        l += 1;
    }
    l
}

// ---- 表驱动测试（host 钉板）----

#[cfg(test)]
mod tests {
    use super::*;

    /// 手搭表：`[(declared, effective) × n]` 序列 → 槽表（写点语义纯投影）。
    fn tally_from(seq: &[(u8, u8)]) -> IntentTally {
        let t = IntentTally::default();
        for &(d, e) in seq {
            t.note_settle(d, e);
        }
        t
    }

    #[test]
    fn test_slot_layout_six_rows() {
        // 六槽定容：0x00–0x05 值直索引全落位，repr(C) 形状 day-one 定型。
        assert_eq!(INTENT_SLOTS, 6);
        for v in 0u8..=5 {
            assert_eq!(tally_key(v), v as usize, "0x{:02x} 值直索引", v);
        }
    }

    #[test]
    fn test_reserved_folds_to_unspecified() {
        // 保留域 0x06..=0xFF 收敛 0 号槽（计数面不丢账不越界——与
        // intent_index 保留域回落同构，编码不镜像只消费）。
        for v in [0x06, 0x07, 0x42, 0xFF] {
            assert_eq!(tally_key(v), 0, "保留域 0x{:02x}", v);
        }
    }

    #[test]
    fn test_inflate_math() {
        // 借入档位差：值越小档越高 → 差 = 抬升档数；非借入/回落恒 0。
        assert_eq!(inflate(0x04, 0x01), 3, "BACKGROUND 借入 LC 档 = 抬 3 档");
        assert_eq!(inflate(0x03, 0x02), 1, "UTILITY 借入 INTERACTIVE = 抬 1 档");
        assert_eq!(inflate(0x02, 0x02), 0, "同档（Own 常态）恒 0");
        assert_eq!(inflate(0x01, 0x04), 0, "effective 弱于 declared（不可达态）防御 0");
        assert_eq!(inflate(0x00, 0x05), 0, "UNSPECIFIED→MAINT 回落恒 0");
    }

    #[test]
    fn test_note_settle_distribution() {
        // 档位分布：结算数落 effective 档（min(declared, msg) 结算律投影），
        // 借入数只落借入生效档。
        let t = tally_from(&[(0x03, 0x01), (0x03, 0x01), (0x04, 0x02), (0x02, 0x02)]);
        assert_eq!(t.at(1), (2, 2), "LC 档：两借入两结算");
        assert_eq!(t.at(2), (2, 1), "INTERACTIVE：一借入一 Own");
        assert_eq!(t.at(3), (0, 0), "UTILITY declared 从不落（effective 恒更强）");
        assert_eq!(t.inflate_get(), 2 + 2 + 2, "通胀 = Σ 档差（(3-1)×2 + 4-2 + 0）");
    }

    #[test]
    fn test_borrowed_invariant() {
        // 计数不变量：Σ 各档 borrows == inflate>0 的结算数；borrowed 遥测
        // 计数器（telemetry::intent_borrows）与本表 Σ borrows 同源同值。
        let seq: Vec<(u8, u8)> = (0..50)
            .map(|i| match i % 5 {
                0 => (0x04, 0x01),
                1 => (0x03, 0x03),
                2 => (0x02, 0x00),
                3 => (0x05, 0x04),
                _ => (0x04, 0x02),
            })
            .collect();
        let t = tally_from(&seq);
        let borrows: u64 = (0..INTENT_SLOTS).map(|i| t.at(i).1).sum();
        let expected: u64 = seq.iter().filter(|&&(d, e)| d > e).count() as u64;
        assert_eq!(borrows, expected, "Σ 槽 borrows == 借入结算数");
        let infl: u64 = seq.iter().map(|&(d, e)| inflate(d, e)).sum();
        assert_eq!(t.inflate_get(), infl, "通胀累计 == Σ 档差");
    }

    #[test]
    fn test_empty_and_omission() {
        // 全零表：is_empty 真 + 行落 `none` 省略形。
        let t = IntentTally::default();
        assert!(t.is_empty());
        let mut out = [0u8; LINE_MAX];
        let n = tally_line(&t, 0, 0, 0, &mut out);
        assert_eq!(&out[..n], b"tele: intent_tally none\n");
    }

    #[test]
    fn test_line_shape_nonzero() {
        // 非零行：k=v 空格分隔、无尾随空格、换行收尾、非零档省略、借入
        // 跟斜杠后置、四联计数齐全——advisor.py 摘要行族面。
        let t = tally_from(&[(0x03, 0x01), (0x04, 0x02), (0x02, 0x02)]);
        let mut out = [0u8; LINE_MAX];
        let n = tally_line(&t, 3, 1, 2, &mut out);
        let s = core::str::from_utf8(&out[..n]).unwrap();
        assert_eq!(s, "tele: intent_tally u1=1/1 u2=2/1 borrowed=3 infl=4 hop_cap=1 bud_cap=2\n");
        assert!(s.is_ascii(), "单行纯 ASCII");
        assert!(!s.trim_end().ends_with(' '), "无尾随空格");
    }

    #[test]
    fn test_line_all_slots_and_bounds() {
        // 六档全非零 + u64 极值：行上界内收口（LINE_MAX 算术界）、键名
        // u0..u5 序正确、极值 20 位十进制不溢出。
        let t = IntentTally::default();
        for i in 0..6u8 {
            t.note_settle(0x05, i);
        }
        t.note_settle(0x05, 0x00); // 档 0 再加一借入
        let mut out = [0u8; LINE_MAX];
        let n = tally_line(&t, u64::MAX, u64::MAX, u64::MAX, &mut out);
        assert!(n <= LINE_MAX, "行上界收口（{} ≤ {}）", n, LINE_MAX);
        let s = core::str::from_utf8(&out[..n]).unwrap();
        // 通胀算术：Σ(0x05−i, i=0..5) = 15 + 追加一借 0x05→0x00 差 5 = 20。
        assert!(s.starts_with("tele: intent_tally u0=2/2 u1=1/1 u2=1/1 u3=1/1 u4=1/1 u5=1 borrowed=18446744073709551615 infl=20"));
        assert!(s.ends_with("hop_cap=18446744073709551615 bud_cap=18446744073709551615\n"));
    }

    #[test]
    fn test_fmt_u64_value_domain() {
        // fmt_u64 值域：0/1/边界/极值 roundtrip（uart_put_dec 值域纯投影）。
        let cases = [
            (0u64, "0"),
            (1, "1"),
            (9, "9"),
            (10, "10"),
            (99, "99"),
            (100, "100"),
            (1_000_003, "1000003"),
            (u32::MAX as u64, "4294967295"),
            (u64::MAX, "18446744073709551615"),
        ];
        for (v, want) in cases {
            let mut buf = [0u8; 20];
            let n = fmt_u64(&mut buf, v);
            assert_eq!(core::str::from_utf8(&buf[..n]).unwrap(), want, "fmt {}", v);
        }
    }

    #[test]
    fn test_record_is_pure_projection() {
        // record 纯投影 = tally_key + inflate 的二元组（写点只做原子，语义
        // 全在此——host 钉板语义、kernel 打原子，零第二实现）。
        assert_eq!(record(0x04, 0x01), (1, 3));
        assert_eq!(record(0x05, 0x06), (0, 0), "保留域 effective 收敛档 0 差 0");
        assert_eq!(record(0x02, 0x02), (2, 0));
    }

    #[test]
    fn test_at_clamps_out_of_range() {
        // 越界槽读防御：收敛尾槽（与 WAKE_SRC_TABLE 索引纪律同款）。
        let t = tally_from(&[(0x05, 0x05)]);
        assert_eq!(t.at(99), t.at(5), "越界读收敛尾槽");
    }
}
