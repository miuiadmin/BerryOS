//! berry-memface——mem* 整数版语义模型（W3-Q80，自 `kernel/src/mem.rs` 外迁）。
//!
//! 单源纪律（同 berry-dtbface/tickface）：**算法语义唯一事实源在此**，
//! `kernel/src/mem.rs` 的 `#[cfg(target_os = "none")]` 覆写层改为消费本
//! crate 的块尾算法（`#[no_mangle] extern "C"` 壳留 kernel——符号定义与
//! 链接期强符号防线不迁；host 面与 libSystem 冲突面同样留 kernel 侧门控）。
//!
//! 为什么 host 钉板此前不可达：mem.rs 全模块 `target_os = "none"` 门控，
//! host 测试面不链接——8 字节块 unaligned 读写、memmove 方向分治、
//! memcmp LE 首异字节定位的语义正确性由 ktest `test_fp` ③ + 每次 boot
//! 事实覆盖（间接），无独立 host 钉板。本 crate 用安全原语
//! （`to_le_bytes`/`from_le_bytes`）重建同构算法模型——语义（块切分/
//! 方向/首异序）逐字对齐，指针算术臂留壳。
//!
//! 四面：
//! 1. [`block_model`]: n → (块数, 尾字节数) 的切分几何（8B 块）。
//! 2. [`fill_word`]: memset 的字节广播（c × 0x0101…0101）。
//! 3. [`cmp_first_diff`]: memcmp 块内首异字节定位（LE 装载低地址在低位
//!    ——升序逐字节比较即 memcmp 符号语义）。
//! 4. [`move_split`]: memmove 重叠方向分治（前向正拷 / 后向逆拷 / 同址
//!    恒等）。
//!
//! 纪律：纯函数、零 unsafe、零 panic——敌意 n（含 usize::MAX 经块数
//! 溢出判拒 None）全兜。

#![cfg_attr(not(test), no_std)]

/// 8 字节块宽（mem.rs 实现同值——块读写经 unaligned 原语不赌对齐）。
pub const BLOCK: usize = 8;

/// memset 字节广播：c → 64 位重复字（`c as u64 * 0x0101_0101_0101_0101`
/// 同式）。
pub const fn fill_word(c: u8) -> u64 {
    c as u64 * 0x0101_0101_0101_0101
}

/// 块切分几何：n → (8B 块数, 尾字节数)。n 溢出块计数（n / 8 + 余数回卷
/// 原 n——理论上仅 usize::MAX 邻域）拒 None（模型防御；kernel 侧调用点
/// 布局常量有界不触发）。
pub const fn block_model(n: usize) -> Option<(usize, usize)> {
    let blocks = n / BLOCK;
    let tail = n % BLOCK;
    if blocks > usize::MAX - 1 {
        return None;
    }
    Some((blocks, tail))
}

/// memcmp 块内首异定位：两 8B 块值（LE 装载）→ 首异字节对（index, a, b）
/// ——低地址字节在低位，升序逐字节比较即 memcmp 语义；全等 None。
pub const fn cmp_first_diff(wa: u64, wb: u64) -> Option<(usize, u8, u8)> {
    let mut i = 0;
    while i < 8 {
        let xa = ((wa >> (i * 8)) & 0xFF) as u8;
        let xb = ((wb >> (i * 8)) & 0xFF) as u8;
        if xa != xb {
            return Some((i, xa, xb));
        }
        i += 1;
    }
    None
}

/// memmove 方向分治：dst/src 关系 → 三态（前向正拷 / 后向从尾逆拷 /
/// 同址恒等空操作）。返回 (后向?, 恒等?)。
pub const fn move_split(dst: usize, src: usize) -> (bool, bool) {
    if dst == src {
        (false, true)
    } else if dst < src {
        (false, false)
    } else {
        (true, false)
    }
}

/// 语义参考实现（host 对拍用）：memset 逐字节模型。
pub fn memset_model(dst: &mut [u8], c: u8, n: usize) {
    let n = n.min(dst.len());
    for i in 0..n {
        dst[i] = c;
    }
}

/// 语义参考实现：memcpy 逐字节模型（不重叠——调用方合同同 mem.rs）。
pub fn memcpy_model(dst: &mut [u8], src: &[u8], n: usize) {
    let n = n.min(dst.len()).min(src.len());
    for i in 0..n {
        dst[i] = src[i];
    }
}

/// 语义参考实现：memmove 逐字节模型（重叠安全——方向分治由块序保证，
/// 模型直接按分治序逐字节）。dst_off/src_off 均为显式偏移（分治判据
/// 须用真实两偏移比较，任一侧硬编码都会误判同址）。
pub fn memmove_model(dst: &mut [u8], src_off: usize, dst_off: usize, n: usize) {
    let (back, same) = move_split(dst_off, src_off);
    if same {
        return;
    }
    if back {
        let mut i = n;
        while i > 0 {
            i -= 1;
            dst[dst_off + i] = dst[src_off + i];
        }
    } else {
        for i in 0..n {
            dst[dst_off + i] = dst[src_off + i];
        }
    }
}

/// 语义参考实现：memcmp 符号语义模型（返回首异字节差的符号面）。
pub fn memcmp_model(a: &[u8], b: &[u8], n: usize) -> i32 {
    let n = n.min(a.len()).min(b.len());
    for i in 0..n {
        if a[i] != b[i] {
            return a[i] as i32 - b[i] as i32;
        }
    }
    0
}

// ---- ARMv8-A 缓存与零化系统寄存器解码律（F0-1：DCZID_EL0 + CTR_EL0 PoU）----

/// `CTR_EL0.L1Ip`（bits[15:14]）策略值：`0b11` = PIPT（物理索引物理标记），
/// 其余（如 `0b10` VIPT）在高半 KVA 与低半 EL0 VA 不同色时存在按 VA 失效别名风险。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcachePolicy {
    /// PIPT（`L1Ip == 0b11`）：`ic ivau, kva` 跨别名完备。
    Pipt,
    /// VIPT 或保留编码（`L1Ip != 0b11`）：跨地址空间装填须走 `ic ialluis`。
    ViptOrAliasing,
}

/// `CTR_EL0` PoU 指令/数据一致性同步计划（ARM DDI 0487 B2.4.4 / D24.2.41）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PouSyncPlan {
    /// `IDC == 0`（bit 28）时需 `dc cvau` 刷到 PoU 的步长（`4 << DminLine`）；`IDC == 1` 为 `None`。
    pub d_clean_step: Option<usize>,
    /// `DIC == 0`（bit 29）时需失效 I-cache 的步长（`4 << IminLine`）；`DIC == 1` 为 `None`。
    pub i_inval_step: Option<usize>,
    /// L1 指令缓存索引策略（决定 `i_inval_step.is_some()` 时用 `ic ivau` 还是 `ic ialluis`）。
    pub icache_policy: IcachePolicy,
}

/// 解码 `DCZID_EL0`（ARM DDI 0487 D24.2.42）：
/// - `DZP`（bit 4）：`0` = 允许执行 `DC ZVA`；`1` = 禁止（执行即 UNDEFINED 异常）；
/// - `BS`（bits[3:0]）：零化块字节数为 `4 << BS`。
/// 仅当 `DZP == 0`、`page_size > 0` 且块大小整除 `page_size` 时返回 `Some(block_bytes)`。
pub const fn dczid_block_size(dczid: u64, page_size: usize) -> Option<usize> {
    if page_size == 0 || ((dczid >> 4) & 1) != 0 {
        return None;
    }
    let bs = (dczid & 0xF) as u32;
    let block = 4usize << bs;
    if block == 0 || block > page_size || (page_size % block) != 0 {
        return None;
    }
    Some(block)
}

/// 解码 `CTR_EL0`（ARM DDI 0487 D24.2.41）：
/// - `IDC`（bit 28）：`0` 需按 `DminLine`（bits[19:16]）执行 `dc cvau`；
/// - `DIC`（bit 29）：`0` 需按 `IminLine`（bits[3:0]）失效指令缓存；
/// - `L1Ip`（bits[15:14]）：`0b11` 为 PIPT，其余按 VIPT 别名保守处理。
pub const fn ctr_pou_plan(ctr: u64) -> PouSyncPlan {
    let idc = ((ctr >> 28) & 1) != 0;
    let dic = ((ctr >> 29) & 1) != 0;
    let dmin = 4usize << (((ctr >> 16) & 0xF) as u32);
    let imin = 4usize << ((ctr & 0xF) as u32);
    let l1ip = (ctr >> 14) & 0b11;
    PouSyncPlan {
        d_clean_step: if idc { None } else { Some(dmin) },
        i_inval_step: if dic { None } else { Some(imin) },
        icache_policy: if l1ip == 0b11 {
            IcachePolicy::Pipt
        } else {
            IcachePolicy::ViptOrAliasing
        },
    }
}

/// 计算缓存维护半开区间 `[start, end)`（按 2 的幂 `line` 向下/向上对齐）。
/// `len == 0` 返回空区间 `Some((va, va))`；算术溢出或非法 `line` 返回 `None`。
pub const fn cache_line_span(va: usize, len: usize, line: usize) -> Option<(usize, usize)> {
    if line == 0 || !line.is_power_of_two() {
        return None;
    }
    if len == 0 {
        return Some((va, va));
    }
    let mask = line - 1;
    let start = va & !mask;
    let Some(end_raw) = va.checked_add(len) else {
        return None;
    };
    let Some(end_padded) = end_raw.checked_add(mask) else {
        return None;
    };
    Some((start, end_padded & !mask))
}
