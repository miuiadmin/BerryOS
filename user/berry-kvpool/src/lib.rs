//! berry-kvpool —— 块级分页 KV-Cache 的池语义基线（ai-runtime 总体设计
//! 契约三.1 首刀：`.agents/plans/ai-runtime/design.md`「块级分页
//! KV-Cache」）。契约语义：上下文键值缓存划分为固定 16 Token 一块的
//! `KvBlock` 池；系统提示词与常用工具定义前缀跨会话共享只读块
//! （Copy-on-Write 前缀缓存——首次响应延迟缩短 70% 的结构前提）。
//! 本 crate 钉**池语义**（分配/共享/分叉 COW/引用计数回收/容量帽），
//! 不钉注意力算子与缺页重装（真消费者 = airtd 运行时，后续刀）。
//!
//! 家族纪律（berry-aiproto/berry-tensor 同款）：零 unsafe、零依赖、零
//! 分配（定容静态槽，const 泛型 [`KV_POOL_CAP`] 由消费侧定）；
//! `cfg_attr(not(test), no_std)`。host 测试经 justfile test-host 显式行跑。
//!
//! ## 语义律（loud 拒 / 显式拒）
//!
//! - **共享块只读**：块引用计数 > 1 时 [`Session::append`] 不得原地写
//!   ——写共享尾块即 [`PoolError::SharedTail`]，调用方应先 fork 或分新块
//!   （COW 边界由数据结构执法，不靠约定）。
//! - **fork 前缀共享**：[`Session::fork_prefix`] 产新会话共享前 n 块
//!   （引用计数 +1），物理块恒一份——「跨会话共享只读 KvBlock」的机器
//!   形态；n 越界即 [`PoolError::BadPrefix`]。
//! - **回收**：[`Session::drop_by`]（no_std 无 Drop 消费边的显式回收口）
//!   逐块引用计数 -1，归零即槽位回空闲表；容量帽尽即
//!   [`PoolError::PoolFull`]（loud——内存水位联动回收归 airtd，本层只
//!   报实况）。
//! - 块内 token 数达 [`KV_TOKENS_PER_BLOCK`] 即满（append 隐式开新块——
//!   除共享尾外，独占尾块可续写）。

#![cfg_attr(not(test), no_std)]

use core::fmt;

/// 每块 Token 数（契约钉值）。
pub const KV_TOKENS_PER_BLOCK: usize = 16;
/// 单会话块链帽（防越界写——真会话上限归 airtd 配置面）。
pub const SESSION_MAX_BLOCKS: usize = 64;

/// 块槽位号。
pub type BlockId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolError {
    /// 池容量帽尽。
    PoolFull,
    /// 会话块链帽尽。
    SessionFull,
    /// 尾块被共享（引用计数 > 1），禁原地写——COW 边界执法。
    SharedTail,
    /// fork 前缀长度越界。
    BadPrefix,
    /// 块号不在池内。
    BadBlock,
}

impl fmt::Display for PoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::PoolFull => "池容量帽尽",
            Self::SessionFull => "会话块链帽尽",
            Self::SharedTail => "尾块被共享禁原地写（先 fork 或分新块）",
            Self::BadPrefix => "fork 前缀长度越界",
            Self::BadBlock => "块号不在池内",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Copy)]
struct BlockState {
    tokens: [u32; KV_TOKENS_PER_BLOCK],
    len: usize,
    refs: usize,
}

/// 定容 KvBlock 池（CAP = 槽位数，const 泛型由消费侧定）。
#[derive(Debug)]
pub struct KvPool<const CAP: usize> {
    blocks: [Option<BlockState>; CAP],
}

impl<const CAP: usize> Default for KvPool<CAP> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAP: usize> KvPool<CAP> {
    pub fn new() -> Self {
        // const 泛型数组无 Default——手搬 None 数组的 const 展开式。
        Self { blocks: [None; CAP] }
    }

    fn alloc(&mut self, first_tok: u32) -> Result<BlockId, PoolError> {
        for (i, slot) in self.blocks.iter_mut().enumerate() {
            if slot.is_none() {
                let mut t = [0u32; KV_TOKENS_PER_BLOCK];
                t[0] = first_tok;
                *slot = Some(BlockState { tokens: t, len: 1, refs: 1 });
                return Ok(i);
            }
        }
        Err(PoolError::PoolFull)
    }

    /// 块引用计数（块号非法即 [`PoolError::BadBlock`]）。
    pub fn refs(&self, id: BlockId) -> Result<usize, PoolError> {
        self.blocks.get(id).ok_or(PoolError::BadBlock)?
            .as_ref().map(|b| b.refs).ok_or(PoolError::BadBlock)
    }

    /// 块内 token 快照（调试/审计面；返回长度 = len）。
    pub fn block_tokens(&self, id: BlockId) -> Result<&[u32], PoolError> {
        self.blocks.get(id).ok_or(PoolError::BadBlock)?
            .as_ref().map(|b| &b.tokens[..b.len]).ok_or(PoolError::BadBlock)
    }

    /// 池内在役块数（共享块计一份——「物理恒一份」的对账口）。
    pub fn live_blocks(&self) -> usize {
        self.blocks.iter().filter(|s| s.is_some()).count()
    }

    fn acquire(&mut self, id: BlockId) -> Result<(), PoolError> {
        let s = self.blocks.get_mut(id).ok_or(PoolError::BadBlock)?;
        match s {
            Some(b) => {
                b.refs += 1;
                Ok(())
            }
            None => Err(PoolError::BadBlock),
        }
    }

    fn release(&mut self, id: BlockId) -> Result<(), PoolError> {
        let s = self.blocks.get_mut(id).ok_or(PoolError::BadBlock)?;
        match s {
            Some(b) => {
                b.refs -= 1;
                if b.refs == 0 {
                    *s = None;
                }
                Ok(())
            }
            None => Err(PoolError::BadBlock),
        }
    }
}

/// 会话视图 = 块链（前缀可共享，尾块独占时可续写）。
#[derive(Debug, Clone, Copy)]
pub struct Session {
    blocks: [BlockId; SESSION_MAX_BLOCKS],
    len: usize,
}

impl Session {
    /// 空会话。
    pub fn empty() -> Self {
        Self { blocks: [0; SESSION_MAX_BLOCKS], len: 0 }
    }

    /// 会话 token 深度（块数）。
    pub fn block_len(&self) -> usize {
        self.len
    }

    /// 追加一 token：独占尾块有空间即续写；满尾（含共享满尾——滚新块
    /// 非原地写，不属 COW 执法面）开新块；尾块有空间且被共享即拒。
    pub fn append<const CAP: usize>(
        &mut self,
        pool: &mut KvPool<CAP>,
        tok: u32,
    ) -> Result<(), PoolError> {
        if self.len > 0 {
            let tail = self.blocks[self.len - 1];
            let s = pool.blocks.get_mut(tail).ok_or(PoolError::BadBlock)?;
            if let Some(b) = s {
                if b.len < KV_TOKENS_PER_BLOCK {
                    if b.refs > 1 {
                        return Err(PoolError::SharedTail);
                    }
                    b.tokens[b.len] = tok;
                    b.len += 1;
                    return Ok(());
                }
                // 满块 → 落到下方开新块（共享满尾不阻——fork 续写主路径）。
            } else {
                return Err(PoolError::BadBlock);
            }
        }
        if self.len >= SESSION_MAX_BLOCKS {
            return Err(PoolError::SessionFull);
        }
        let id = pool.alloc(tok)?;
        self.blocks[self.len] = id;
        self.len += 1;
        Ok(())
    }

    /// fork 前缀：新会话共享本会话前 n 块（引用计数 +1，物理恒一份）。
    pub fn fork_prefix<const CAP: usize>(
        &self,
        pool: &mut KvPool<CAP>,
        n: usize,
    ) -> Result<Session, PoolError> {
        if n > self.len {
            return Err(PoolError::BadPrefix);
        }
        let mut out = Session::empty();
        for i in 0..n {
            pool.acquire(self.blocks[i])?;
            out.blocks[i] = self.blocks[i];
        }
        out.len = n;
        Ok(out)
    }

    /// 显式回收（no_std 无 Drop 消费边）：逐块引用计数 -1，归零回空闲。
    pub fn drop_by<const CAP: usize>(
        &mut self,
        pool: &mut KvPool<CAP>,
    ) -> Result<(), PoolError> {
        for i in 0..self.len {
            pool.release(self.blocks[i])?;
        }
        self.len = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_fills_and_rolls_blocks() {
        let mut pool = KvPool::<8>::new();
        let mut s = Session::empty();
        for i in 0..(KV_TOKENS_PER_BLOCK + 3) as u32 {
            s.append(&mut pool, i).unwrap();
        }
        assert_eq!(s.block_len(), 2);
        assert_eq!(pool.block_tokens(0).unwrap().len(), KV_TOKENS_PER_BLOCK);
        assert_eq!(pool.block_tokens(1).unwrap(), &[16, 17, 18]);
        assert_eq!(pool.live_blocks(), 2);
    }

    #[test]
    fn fork_prefix_shares_physical_blocks() {
        let mut pool = KvPool::<8>::new();
        let mut sys = Session::empty();
        for i in 0..KV_TOKENS_PER_BLOCK as u32 {
            sys.append(&mut pool, 100 + i).unwrap();
        }
        let a = sys.fork_prefix(&mut pool, 1).unwrap();
        let b = sys.fork_prefix(&mut pool, 1).unwrap();
        // 前缀共享：refs=3（原 + 两 fork），物理块恒一份。
        assert_eq!(pool.refs(0), Ok(3));
        assert_eq!(pool.live_blocks(), 1);
        // 两个 fork 各自续写 → 各开私有尾块（COW 边界在块粒度）。
        let mut a = a;
        let mut b = b;
        a.append(&mut pool, 1).unwrap();
        b.append(&mut pool, 2).unwrap();
        assert_eq!(a.block_len(), 2);
        assert_eq!(b.block_len(), 2);
        assert_eq!(pool.live_blocks(), 3); // 1 共享 + 2 私有
    }

    #[test]
    fn shared_tail_rejects_inplace_write() {
        let mut pool = KvPool::<8>::new();
        let mut sys = Session::empty();
        for i in 0..4u32 {
            sys.append(&mut pool, i).unwrap();
        }
        let mut f = sys.fork_prefix(&mut pool, 1).unwrap();
        // 尾块被 fork 共享：fork 原地续写被拒（COW 执法）。
        assert_eq!(f.append(&mut pool, 9), Err(PoolError::SharedTail));
        // 但开新块路径不受阻：先填满尾块语义等价面由调用方处理，
        // 池层只执法「不原地写共享块」。
        drop(f);
    }

    #[test]
    fn fork_partial_prefix_and_bad_prefix() {
        let mut pool = KvPool::<8>::new();
        let mut s = Session::empty();
        for i in 0..20u32 {
            s.append(&mut pool, i).unwrap();
        }
        let f = s.fork_prefix(&mut pool, 1).unwrap();
        assert_eq!(f.block_len(), 1);
        assert_eq!(pool.refs(0), Ok(2));
        assert_eq!(s.fork_prefix(&mut pool, 3).unwrap_err(), PoolError::BadPrefix);
    }

    #[test]
    fn drop_by_releases_to_free_pool() {
        let mut pool = KvPool::<4>::new();
        let mut s = Session::empty();
        for i in 0..40u32 {
            s.append(&mut pool, i).unwrap();
        }
        assert_eq!(pool.live_blocks(), 3);
        let mut f = s.fork_prefix(&mut pool, 2).unwrap();
        s.drop_by(&mut pool).unwrap();
        // fork 仍持前 2 块（refs 2→1），尾块归零回收。
        assert_eq!(pool.live_blocks(), 2);
        f.drop_by(&mut pool).unwrap();
        assert_eq!(pool.live_blocks(), 0);
        // 池净空后可重新分配。
        let mut s2 = Session::empty();
        s2.append(&mut pool, 7).unwrap();
        assert_eq!(pool.live_blocks(), 1);
    }

    #[test]
    fn pool_full_loud() {
        let mut pool = KvPool::<1>::new();
        let mut a = Session::empty();
        a.append(&mut pool, 1).unwrap();
        let mut b = Session::empty();
        assert_eq!(b.append(&mut pool, 1), Err(PoolError::PoolFull));
    }

    #[test]
    fn session_full_loud() {
        let mut pool = KvPool::<256>::new();
        let mut s = Session::empty();
        // 填满会话块链帽（每块先满再滚——独占尾块续写路径）。
        for i in 0..(SESSION_MAX_BLOCKS * KV_TOKENS_PER_BLOCK) as u32 {
            s.append(&mut pool, i).unwrap();
        }
        assert_eq!(s.block_len(), SESSION_MAX_BLOCKS);
        assert_eq!(
            s.append(&mut pool, 0),
            Err(PoolError::SessionFull)
        );
    }

    #[test]
    fn bad_block_queries_rejected() {
        let pool = KvPool::<2>::new();
        assert_eq!(pool.refs(0), Err(PoolError::BadBlock));
        assert_eq!(pool.refs(2), Err(PoolError::BadBlock));
        assert_eq!(pool.block_tokens(0), Err(PoolError::BadBlock));
    }

    #[test]
    fn exclusive_tail_extends_inplace() {
        // 独占尾块（refs=1）续写不 COW——块内摊销，契约「块级分页」形态。
        let mut pool = KvPool::<8>::new();
        let mut s = Session::empty();
        s.append(&mut pool, 1).unwrap();
        s.append(&mut pool, 2).unwrap();
        s.append(&mut pool, 3).unwrap();
        assert_eq!(s.block_len(), 1);
        assert_eq!(pool.block_tokens(0).unwrap(), &[1, 2, 3]);
    }
}
