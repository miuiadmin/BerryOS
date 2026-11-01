//! berry-ui 属性级依赖记录骨架（GUI-3——`docs/GUI-3实施设计草案.md`
//! §3.10 增件①；ADR #52 berryui 声明式框架的数据面预留缝）。
//!
//! - [`DepId`] = app 状态属性句柄（app 自定义语义，引擎不解释）；
//!   定容边表 `[Option<(NodeId, DepId)>; DEP_EDGES_MAX=64]`。
//! - **v1 记录不消费**：失效仍视图级 `invalidate`（裁决不跳步）；缝的
//!   验收 = 形状钉板测试 + v1.5 键路径 diff 对拍挂账（S7 spike 定生
//!   死：属性级收益 <20% 帧耗时 → 维持视图级至 v2）。
//! - 线性扫描 v1 明示：容量 64 微量、失败路径 = 满帽 loud
//!   （UiError::DepFull）——K-1「分配器线性扫骰子」在此不成立（无
//!   按阶空查的暗面，满查 = 拒绝语义的一部分）。
//! - 零 panic / 零分配：重复 bind 幂等 Ok（同边不重复占槽）；节点
//!   liveness 校验是调用方（lib.rs dep_bind）的义务——本表只记形状。
//!
//! **集成接缝（已收敛）**：Ui 的边表已迁本表消费（`deps: DepTable` +
//! dep_bind/dep_dependents 委托、remove 连子树 unbind_node 先于 gen 推
//! 进、reset_arena 走 `clear()`——幂等去重语义激活；`#[allow(dead_code)]`
//! 全摘）。节点 liveness 校验仍在 lib.rs 入口（本表只记形状）。

use crate::desc::NodeId;
use crate::UiError;

/// app 状态属性句柄（§6 冻结——透明包装，app 侧自定义语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DepId(pub u32);

/// 内部边形态（node 槽打包值 gen<<16|idx——与 NodeId.0 同构存裸 u32，
/// Copy 平面数据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Edge {
    node: u32,
    dep: DepId,
}

/// 依赖边定容表（§3.10 表形态；Ui.deps 的单源实现——lib.rs 委托消费）。
pub(crate) struct DepTable {
    edges: [Option<Edge>; crate::DEP_EDGES_MAX],
}

impl DepTable {
    pub(crate) const fn new() -> Self {
        Self { edges: [None; crate::DEP_EDGES_MAX] }
    }

    /// 记录依赖边（node→dep，§3.10 dep_bind 的表侧）。同边重复 bind =
    /// 幂等 Ok（不重复占槽）；槽满 → `UiError::DepFull` loud（§6 定容
    /// 族）。节点 liveness/合法位校验在 lib.rs 入口（本表只记形状）。
    pub(crate) fn bind(&mut self, node: NodeId, dep: DepId) -> Result<(), UiError> {
        let raw = node.0;
        // 幂等查重（线性扫——v1 明示，见模块头注）
        if self
            .edges
            .iter()
            .any(|e| matches!(e, Some(x) if x.node == raw && x.dep == dep))
        {
            return Ok(());
        }
        let slot = self
            .edges
            .iter()
            .position(|e| e.is_none())
            .ok_or(UiError::DepFull)?;
        self.edges[slot] = Some(Edge { node: raw, dep });
        Ok(())
    }

    /// 查询 dep 的依赖节点（§3.10 dep_dependents 表侧）：填入 out（截
    /// 断到 out.len()），**返回真实依赖总数**——调用方以返回值 >
    /// out.len() 判截断（零分配哨兵面：不炸不静默缩水）。
    pub(crate) fn dependents(&self, dep: DepId, out: &mut [NodeId]) -> usize {
        let mut filled = 0usize;
        let mut total = 0usize;
        for e in self.edges.iter().flatten() {
            if e.dep == dep {
                total += 1;
                if filled < out.len() {
                    out[filled] = NodeId(e.node);
                    filled += 1;
                }
            }
        }
        total
    }

    /// 节点（连子树场景由 lib.rs 对子树逐节点调用）移除时解绑其全部
    /// 边——防悬空 id 残留边表（形状完整性，非消费语义）。
    pub(crate) fn unbind_node(&mut self, node: NodeId) {
        let raw = node.0;
        for e in self.edges.iter_mut() {
            if let Some(x) = e {
                if x.node == raw {
                    *e = None;
                }
            }
        }
    }

    /// 全清（lib.rs `reset_arena` 的随行面——整树重建后旧边全悬空）。
    pub(crate) fn clear(&mut self) {
        self.edges = [None; crate::DEP_EDGES_MAX];
    }
}
