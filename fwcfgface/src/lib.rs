//! berry-fwcfgface——fw_cfg 文件目录行走/顺序读推进/ramfb 域门纯函数面
//!（W3-Q86，自 `arch/aarch64/src/fwcfg.rs` 外迁）。
//!
//! 单源纪律（同 berry-dtbface/berry-numface）：**判定律唯一事实源在此**，
//! HAL `fwcfg.rs` 机械壳消费——MMIO 物理臂（selector BE u16 写/data 口
//! u32 顺序弹读/DMA 描述符装配）留 HAL 侧；本 crate 零依赖、纯函数、
//! host 可测。
//!
//! 抽离面（与 HAL 侧逐律对齐，改这里的语义 = 改 HAL 行为）：
//! - **目录行走判定**（[`find_file_in`]）：count 帽（>1024 拒——畸形大数
//!   防线）/名字 55 字节帽（56B 定长域——超长查询名恒 None）/项名 NUL
//!   定位（无 NUL = 畸形项跳过不是拒整个目录）/BE 域恢复（select 高
//!   16 位/size 独立 u32）。
//! - **顺序读推进律**（[`SeqState`]）：want = min(buf, remaining)；非末段
//!   `&= !3`（4 字节粒度——中途半 u32 = 跳字节）；余数只许发生在项尾
//!   （前缀截取——被截尾 1-3 字节已消费）；空 buf/读完恒 0。
//! - **ramfb 域门**（[`ramfb_cfg_ok`]）：项长恰 28B + width/height/stride
//!   全非零（零几何 = 无效模式，调用方保持原几何）。
//!
//! 纪律（自 HAL fwcfg.rs 逐字继承）：**纯函数、零 panic**——畸形输入一律
//! `None`/`false`，boot 侧降级存活（ABI §8.0「永不 panic」同源精神）。
//! 目录行走界检全在「读到的 count/项字段」域上做——字节流读臂由注入的
//! [`WordStream`] 闭包提供（MMIO 臂留 HAL，host 测试注入向量）。

#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests;

/// fw_cfg 文件目录项数帽（畸形大数防线——QEMU 实际目录个位数项）。
pub const DIR_MAX_ENTRIES: u32 = 1024;

/// 目录项名字段定长（字节域；NUL 结尾可短不可长）。
pub const NAME_LEN: usize = 56;

/// 查询名帽 = 定长域 - 1（≥56 字节的查询名恒 None——不占满域即无 NUL）。
pub const NAME_MAX: usize = NAME_LEN - 1;

/// ramfb 配置项定长（28B 大端：addr u64 + fourcc u32 + flags u32 +
/// width u32 + height u32 + stride u32）。
pub const RAMFB_CFG_LEN: u32 = 28;

/// fw_cfg 文件目录项（select 键 + size 字节数——BE 域恢复后的值域）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileEntry {
    pub select: u16,
    pub size: u32,
}

/// 顺序字读臂抽象（data 口 u32 弹读的纯面化）：一次读一个「项内字节序」
/// 的 u32——MMIO 臂 = `mmio::read(DATA)`；host 测试注入字节向量游标。
/// 越流（向量耗尽）返回 None = 目录畸形（与 MMIO 语境「QEMU 可信」的
/// 差异只在 host 向量截断，判定面同一出口）。
pub trait WordStream {
    /// 顺序读一个 u32（项内字节序——LE 解释面，BE 数值字段由调用方
    /// `swap_bytes` 还原）。
    fn next_u32(&mut self) -> Option<u32>;
}

/// 字节向量游标实现（host 测试/离线对拍用；no_std 侧零消费）。
#[derive(Debug)]
pub struct VecStream<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> VecStream<'a> {
    /// 从字节向量建游标（从首字节起顺序读）。
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
}

impl WordStream for VecStream<'_> {
    fn next_u32(&mut self) -> Option<u32> {
        let end = self.pos.checked_add(4)?;
        if end > self.buf.len() {
            return None;
        }
        let w = u32::from_le_bytes([
            self.buf[self.pos],
            self.buf[self.pos + 1],
            self.buf[self.pos + 2],
            self.buf[self.pos + 3],
        ]);
        self.pos = end;
        Some(w)
    }
}

/// BE u32 还原（目录 count/size 数值域）——data 口项内字节序是 LE 排列、
/// 字段语义大端：`swap_bytes` 即还原（与 HAL `stream_be32` 同律）。
#[inline]
pub fn be32(w: u32) -> u32 {
    w.swap_bytes()
}

/// 目录项 select 域恢复（BE 拼 u32 的高 16 位 = select、低 16 位
/// reserved——QEMU 目录项布局）。
#[inline]
pub fn select_of(sel_rsv: u32) -> u16 {
    (be32(sel_rsv) >> 16) as u16
}

/// 在 fw_cfg 文件目录里按名查项（判定律单源；读臂注入）。畸形防线：
/// count 超 [`DIR_MAX_ENTRIES`] 拒（None）；查询名 ≥ [`NAME_MAX`] 恒
/// None（56B 域内不可能有无 NUL 的完整匹配）；项名无 NUL = 该项畸形
/// 跳过（不拒目录——后续项仍可匹配）；流截断（项字段读不满）= 目录
/// 畸形 None。**中途匹配项后的剩余项不消费**（MMIO 语境 selector 语境
/// 单向流——匹配即出，与 HAL find_file 逐字同律）。
pub fn find_file_in<S: WordStream>(name: &str, s: &mut S) -> Option<FileEntry> {
    if name.len() >= NAME_LEN {
        return None;
    }
    let Some(count) = s.next_u32().map(be32) else {
        return None;
    };
    if count > DIR_MAX_ENTRIES {
        return None;
    }
    for _ in 0..count {
        let size = s.next_u32().map(be32)?;
        let sel_rsv = s.next_u32()?;
        let select = select_of(sel_rsv);
        // name[56]：14 × u32 顺序读，低字节在前 = 项内字节序原样恢复。
        let mut nbuf = [0u8; NAME_LEN];
        for chunk in nbuf.chunks_mut(4) {
            let w = s.next_u32()?;
            chunk.copy_from_slice(&w.to_le_bytes()[..chunk.len()]);
        }
        let Some(nul) = nbuf.iter().position(|&c| c == 0) else {
            continue; // 无 NUL：畸形项跳过（后续项仍可匹配）
        };
        if &nbuf[..nul] == name.as_bytes() {
            return Some(FileEntry { select, size });
        }
    }
    None
}

/// 顺序读会话推进律（纯面化——A1-3b 4 字节粒度纪律的判定单源）。
///
/// 推进算法（与 HAL `SeqRead::read` 逐字同律）：
/// - `remaining == 0 || buf 空` → 0（会话已尽/零请求）；
/// - `want = min(buf.len(), remaining)`；非末段（`want < remaining`）时
///   `want &= !3`（4 字节粒度——中途半 u32 = 跳字节）；取整后 0 → 0；
/// - 项尾（`want == remaining`）余数合法（前缀截取——尾 1-3 字节已从
///   流中消费）；
/// - 实读数返回，`remaining` 扣减——推进量恒 = 实读数（游标不漂移）。
///
/// **纯判定不做拷贝**——拷贝执行臂（chunk copy_from_slice + MMIO 弹读）
/// 留 HAL `SeqRead::read`；本函数返回「本步实读数」供其执行。
pub struct SeqState {
    /// 剩余未读字节（项尾守卫：越项尾的 data 口行为不承诺，绝不越过）。
    pub remaining: u32,
}

impl SeqState {
    /// 建会话（空项 size==0 恒 None——与 HAL `SeqRead::open` 同律）。
    pub fn open(size: u32) -> Option<SeqState> {
        if size == 0 {
            return None;
        }
        Some(SeqState { remaining: size })
    }

    /// 本步实读数（推进律判定；零拷贝纯面）。
    pub fn plan(&self, buf_len: usize) -> usize {
        if self.remaining == 0 || buf_len == 0 {
            return 0;
        }
        let mut want = core::cmp::min(buf_len as u32, self.remaining);
        if want < self.remaining {
            want &= !3;
        }
        want as usize
    }

    /// 推进（实读后扣减——调用方以 plan 返回值执行拷贝后调）。
    pub fn advance(&mut self, n: usize) {
        self.remaining = self.remaining.saturating_sub(n as u32);
    }

    /// 会话已尽判（remaining == 0）。
    pub fn done(&self) -> bool {
        self.remaining == 0
    }
}

/// ramfb 模式域门（`etc/ramfb` 项域检——纯面）：项长恰 28B 且几何三值
/// 全非零。任一不符 → false（调用方保持原几何——降级不拒 boot）。
#[inline]
pub fn ramfb_cfg_ok(entry_size: u32, width: u32, height: u32, stride: u32) -> bool {
    entry_size == RAMFB_CFG_LEN && width != 0 && height != 0 && stride != 0
}
