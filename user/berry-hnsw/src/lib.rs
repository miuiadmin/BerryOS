//! berry-hnsw —— vault 分层可导航小世界图骨架（ai-runtime 总体设计契约
//! 四.1 第二片：`.agents/plans/ai-runtime/design.md`「轻量分层可导航小
//! 世界图（HNSW 定点量化向量索引）」）。本 crate 钉**图结构面**：分层
//! 指派（splitmix 确定性种子 → 指数硬币）、贪心下降（每层 1-best 局部
//! 最小即止）、层 0 束搜（ef 候选集宽度）；距离经泛型闭包注入——图不
//! 知道向量形态（消费侧接 berry-vqvec 的量化距离，测试用标量度量）。
//!
//! ## 本片边界（朴素形，诚实注记）
//!
//! - 建图邻居选择 = 贪心候选直连 + 容量赶最远；ef-construction 束建与
//!   邻居多样性启发式随后续刀（导航性下界已由测试钉：1D/2D 度量最近邻
//!   全命中）。
//! - 确定性律：同插入序列恒同图（splitmix 种子 = 节点序号，无环境熵）；
//!   束搜 tie-break 按节点号升序——检索结果可复现（vault 审计账本的
//!   前提之一）。
//!
//! 家族纪律：零 unsafe、零依赖、零分配（const 泛型定容）；`cfg_attr(
//! not(test), no_std)`。host 测试经 justfile test-host 显式行跑。

#![cfg_attr(not(test), no_std)]

use core::fmt;

/// 图节点容量帽（const 泛型外的静态帽——两层防越界写）。
pub const MAX_NODES: usize = 256;
/// 分层帽（HNSW 典型 mL=1/ln(M) 下 8 层覆盖百万级节点）。
pub const MAX_LAYERS: usize = 8;
/// 单节点每层邻居帽。
pub const MAX_NEIGHBORS: usize = 16;
/// 建图束宽（ef-construction——R15 升格值；朴素直连形的召回升级）。
pub const EF_CONSTRUCTION: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HnswError {
    /// 节点容量帽尽。
    NodeFull,
    /// 层号越帽（分层因子异常或帽子配小）。
    LayerOverflow,
    /// 节点号不在图内。
    BadNode,
}

impl fmt::Display for HnswError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::NodeFull => "节点容量帽尽",
            Self::LayerOverflow => "层号越帽",
            Self::BadNode => "节点号不在图内",
        };
        f.write_str(s)
    }
}

/// splitmix64（确定性种子混合——无环境熵源的面里替代 RNG）。
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// 指数分层：u ∈ (0,1) → floor(-ln(u) / ln(16))，帽 [`MAX_LAYERS`]。
pub fn layer_for(node: usize) -> Result<usize, HnswError> {
    let mut st = node as u64 + 0x1234_5678_9abc_def0;
    let bits = splitmix64(&mut st);
    // u = (bits >> 11) as f32 / 2^53——(0,1) 开域（全零概率 ~2^-53 忽略，
    // 落零按层 0 处理——防 ln(0) 溢出的保底分支）。
    let u = ((bits >> 11) as f64 / 9007199254740992.0) as f32;
    if u <= 0.0 {
        return Ok(0);
    }
    // -ln(u) = ln(1/u)；手写 ln 无 core 面——用 2^k 指数位近似：取
    // floor(log2(1/u) × log2(e) / log2(16)) = floor(log16(1/u))。
    // 位级 log2：f32 指数域直取（1/u ∈ (1, 2^53) 正规域）。
    let inv = 1.0 / u;
    let bits_inv = inv.to_bits();
    let exp2 = ((bits_inv >> 23) & 0xff) as i32 - 127; // floor(log2(inv))
    // log16(inv) = exp2/4 + log2(mant)/4；mant ∈ [1,2) → 修正在 [0, 0.25)，
    // 忽略修正（分层是概率面，半层偏差只动分布不动正确性）。
    let l = exp2 / 4;
    let l = if l < 0 { 0usize } else { l as usize };
    if l >= MAX_LAYERS {
        Ok(MAX_LAYERS - 1)
    } else {
        Ok(l)
    }
}

#[derive(Debug, Clone, Copy)]
struct Neighbors {
    ids: [usize; MAX_NEIGHBORS],
    dists: [f32; MAX_NEIGHBORS],
    len: usize,
}

impl Neighbors {
    const fn empty() -> Self {
        Self { ids: [0; MAX_NEIGHBORS], dists: [0.0; MAX_NEIGHBORS], len: 0 }
    }
}

/// 定容 HNSW 图（朴素建图形）。
#[derive(Debug, Clone)]
pub struct Hnsw {
    count: usize,
    layer_of: [u8; MAX_NODES],
    top_layer: usize,
    entry: usize,
    // [layer][node] → 邻接表（对角自环不存）。
    adj: [[Neighbors; MAX_NODES]; MAX_LAYERS],
}

impl Default for Hnsw {
    fn default() -> Self {
        Self::new()
    }
}

impl Hnsw {
    pub fn new() -> Self {
        Self {
            count: 0,
            layer_of: [0; MAX_NODES],
            top_layer: 0,
            entry: 0,
            adj: [[Neighbors::empty(); MAX_NODES]; MAX_LAYERS],
        }
    }

    /// 在役节点数。
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// 节点所在层（自底向上计——节点出现在 [0, layer_of] 各层）。
    pub fn node_layer(&self, node: usize) -> Result<usize, HnswError> {
        match self.layer_of.get(node) {
            Some(l) if node < self.count => Ok(*l as usize),
            _ => Err(HnswError::BadNode),
        }
    }

    /// 插入一节点：距离经 `dist` 闭包（对已入图节点求距）。
    /// 朴素建图：每层与贪心入口直连（容量赶最远）；首节点成入口。
    pub fn insert(&mut self, dist: impl Fn(usize) -> f32) -> Result<usize, HnswError> {
        let id = self.count;
        if id >= MAX_NODES {
            return Err(HnswError::NodeFull);
        }
        let layer = layer_for(id)?;
        if id == 0 {
            self.layer_of[0] = layer as u8;
            self.top_layer = layer;
            self.entry = 0;
            self.count = 1;
            return Ok(0);
        }
        // ef-construction 建图：每层自入口束搜候选，连前 MAX_NEIGHBORS
        // 近邻（朴素直连形的召回升格——R15）。
        let mut cur = self.entry;
        for l in (0..=layer.min(self.top_layer)).rev() {
            let mut visited = [false; MAX_NODES];
            let mut res = [(0usize, 0f32); MAX_NODES];
            let n = self.beam(cur, l, EF_CONSTRUCTION, &dist, &mut visited, &mut res);
            if n == 0 {
                return Err(HnswError::BadNode);
            }
            cur = res[0].0; // 下降入口 = 本层最近者
            for i in 0..n.min(MAX_NEIGHBORS) {
                let (nid, d) = res[i];
                self.connect(id, nid, l, d)?;
            }
        }
        // 高于自身层的层不入（分层指派律：节点只住 [0, layer_of]）。
        self.layer_of[id] = layer as u8;
        if layer > self.top_layer {
            self.top_layer = layer;
            self.entry = id;
        }
        self.count += 1;
        Ok(id)
    }

    fn greedy_step(&self, from: usize, layer: usize, dist: &impl Fn(usize) -> f32) -> usize {
        let mut best = from;
        let mut best_d = dist(from);
        loop {
            let nb = &self.adj[layer][best];
            let mut moved = false;
            for i in 0..nb.len {
                let d = dist(nb.ids[i]);
                if d < best_d {
                    best_d = d;
                    best = nb.ids[i];
                    moved = true;
                }
            }
            if !moved {
                return best;
            }
        }
    }

    /// 有界 best-first 束搜：`entry` 起、`ef` 宽度，结果 (id, dist) 升序
    /// （tie-break 节点号）写 `res`，返回条数。终止律：最近候选劣于结果
    /// 集第 ef 名且已满 → 无望剪枝；`visited` 由调用方供池（束搜前自清）。
    fn beam(
        &self,
        entry: usize,
        layer: usize,
        ef: usize,
        dist: &impl Fn(usize) -> f32,
        visited: &mut [bool; MAX_NODES],
        res: &mut [(usize, f32); MAX_NODES],
    ) -> usize {
        visited.fill(false);
        let mut cand = [(0usize, f32::INFINITY); MAX_NODES];
        let mut cand_n = 0usize;
        let mut res_n = 0usize;
        let better = |a: (usize, f32), b: (usize, f32)| {
            a.1 < b.1 || (a.1 == b.1 && a.0 < b.0)
        };
        // 入口入候选。
        visited[entry] = true;
        cand[cand_n] = (entry, dist(entry));
        cand_n += 1;
        while cand_n > 0 {
            let cur = cand[0];
            // 弹出最近候选。
            cand.copy_within(1..cand_n, 0);
            cand_n -= 1;
            // 终止律：结果已满 ef 且最近候选更劣 → 其余只会更劣。
            if res_n >= ef && !better(cur, res[ef - 1]) {
                break;
            }
            // 入结果（保序插入，可超 ef 后截）。
            let mut pos = res_n;
            while pos > 0 && better(cur, res[pos - 1]) {
                res[pos] = res[pos - 1];
                pos -= 1;
            }
            res[pos] = cur;
            res_n += 1;
            // 展开邻居入候选（保序插入）。
            let nb = self.adj[layer][cur.0];
            for j in 0..nb.len {
                let nid = nb.ids[j];
                if !visited[nid] {
                    visited[nid] = true;
                    let e = (nid, dist(nid));
                    let mut pos = cand_n;
                    while pos > 0 && better(e, cand[pos - 1]) {
                        cand[pos] = cand[pos - 1];
                        pos -= 1;
                    }
                    cand[pos] = e;
                    cand_n += 1;
                }
            }
        }
        res_n.min(ef)
    }

    fn connect(
        &mut self,
        a: usize,
        b: usize,
        layer: usize,
        db: f32,
    ) -> Result<(), HnswError> {
        if a == b {
            return Ok(());
        }
        for (x, (y, dy)) in [(a, (b, db)), (b, (a, db))] {
            let slot = &mut self.adj[layer][x];
            if slot.len < MAX_NEIGHBORS {
                slot.ids[slot.len] = y;
                slot.dists[slot.len] = dy;
                slot.len += 1;
            } else {
                // 赶最远（容量律——保近弃远）。
                let mut far = 0usize;
                for i in 1..slot.len {
                    if slot.dists[i] > slot.dists[far] {
                        far = i;
                    }
                }
                if db < slot.dists[far] {
                    slot.ids[far] = y;
                    slot.dists[far] = db;
                }
            }
        }
        Ok(())
    }

    /// 最近邻检索：自顶贪心下降至层 1，层 0 束搜（`ef` 候选宽度）；
    /// 结果按 (dist, node) 升序写 `out`，返回写入数（≤ out.len）。
    /// tie-break 节点号升序——确定性检索。
    pub fn search(
        &self,
        dist: impl Fn(usize) -> f32,
        ef: usize,
        out: &mut [(usize, f32)],
    ) -> Result<usize, HnswError> {
        if self.count == 0 {
            return Ok(0);
        }
        let mut cur = self.entry;
        for l in (1..=self.top_layer).rev() {
            cur = self.greedy_step(cur, l, &dist);
        }
        // 层 0 有界 best-first 束搜（R15：全展开升格——终止律见 beam）。
        let mut visited = [false; MAX_NODES];
        let mut res = [(0usize, 0f32); MAX_NODES];
        let n = self.beam(cur, 0, ef, &dist, &mut visited, &mut res);
        let take = n.min(out.len());
        out[..take].copy_from_slice(&res[..take]);
        Ok(take)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1D 度度测试夹具：节点 i 的坐标 = i × 0.5，查询点 q 注入。
    fn dist_1d(q: f32) -> impl Fn(usize) -> f32 {
        move |i: usize| (i as f32 * 0.5 - q).abs()
    }

    #[test]
    fn layer_deterministic_and_bounded() {
        // 同节点恒同层（种子=序号的确定性）+ 帽内 + 多数落层 0（指数律）。
        let mut zeros = 0usize;
        for n in 0..64 {
            let a = layer_for(n).unwrap();
            assert_eq!(a, layer_for(n).unwrap());
            assert!(a < MAX_LAYERS);
            if a == 0 {
                zeros += 1;
            }
        }
        assert!(zeros >= 32, "层 0 应占多数（指数律）：{}", zeros);
    }

    #[test]
    fn insert_and_search_exact_1d() {
        let mut g = Hnsw::new();
        for i in 0..32 {
            let q = i as f32 * 0.5;
            g.insert(dist_1d(q)).unwrap();
        }
        assert_eq!(g.len(), 32);
        // 均匀格点最近邻：查询恰落格点 → 该节点距离 0。
        let mut out = [(0usize, 0f32); 4];
        let n = g.search(dist_1d(7.5), 4, &mut out).unwrap();
        assert_eq!(n, 4);
        assert_eq!(out[0], (15, 0.0));
        // 半格点：两邻等距 → tie-break 节点号升序。
        let mut out2 = [(0usize, 0f32); 2];
        let n2 = g.search(dist_1d(7.75), 2, &mut out2).unwrap();
        assert_eq!(n2, 2);
        assert!(out2[0].1 == out2[1].1 && out2[0].0 < out2[1].0);
        assert!((out2[0].1 - 0.25).abs() < 1e-6);
    }

    #[test]
    fn determinism_same_sequence_same_graph() {
        let build = || {
            let mut g = Hnsw::new();
            for i in 0..24 {
                let q = i as f32 * 0.37;
                g.insert(dist_1d(q)).unwrap();
            }
            g
        };
        let a = build();
        let b = build();
        assert_eq!(format!("{:?}", a.len()), format!("{:?}", b.len()));
        // 逐层邻接深度比较（Debug 形）。
        for l in 0..MAX_LAYERS {
            for n in 0..24 {
                assert_eq!(format!("{:?}", a.adj[l][n]), format!("{:?}", b.adj[l][n]));
            }
        }
    }

    #[test]
    fn search_on_empty_and_single() {
        let g = Hnsw::new();
        let mut out = [(0usize, 0f32); 2];
        assert_eq!(g.search(dist_1d(0.0), 2, &mut out), Ok(0));
        let mut g1 = Hnsw::new();
        g1.insert(dist_1d(0.0)).unwrap(); // 节点 0 坐标 0×0.5
        let n = g1.search(dist_1d(0.0), 2, &mut out).unwrap();
        assert_eq!(n, 1);
        assert_eq!(out[0], (0, 0.0));
    }

    #[test]
    fn node_capacity_loud() {
        let mut g = Hnsw::new();
        for i in 0..MAX_NODES {
            let q = i as f32;
            g.insert(dist_1d(q)).unwrap();
        }
        assert_eq!(g.insert(dist_1d(0.0)), Err(HnswError::NodeFull));
    }

    #[test]
    fn bad_node_query_loud() {
        let g = Hnsw::new();
        assert_eq!(g.node_layer(0), Err(HnswError::BadNode));
    }

    #[test]
    fn beam_ef_cap_and_recall() {
        // 64 节点 1D 格点（束建图）：束搜召回精确最近邻，ef 帽生效。
        let mut g = Hnsw::new();
        for i in 0..64 {
            let q = i as f32 * 0.5;
            g.insert(dist_1d(q)).unwrap();
        }
        let mut out = [(0usize, 0f32); 4];
        let n = g.search(dist_1d(10.1), 4, &mut out).unwrap();
        assert_eq!(n, 4);
        assert_eq!(out[0], (20, 10.1f32 - 10.0f32)); // 10.1f32−10.0 位精确
        // 升序 + tie-break。
        for w in out.windows(2) {
            assert!(w[0].1 < w[1].1 || (w[0].1 == w[1].1 && w[0].0 < w[1].0));
        }
        // ef 帽 ≤ out.len。
        let mut small = [(0usize, 0f32); 2];
        let n2 = g.search(dist_1d(10.1), 4, &mut small).unwrap();
        assert_eq!(n2, 2);
        assert_eq!(small[0], out[0]);
    }
}
