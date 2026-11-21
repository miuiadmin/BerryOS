//! berry-scene-ir——G3 刀 11：声明式 SceneIR 小组件二进制序列化与只读密封。
//!
//! SceneIR 是小组件的紧凑二进制「场景快照」格式：
//! - 应用沙箱构建场景树（容器/文本/图像/圆角矩形）→ 序列化为定长+变长
//!   二进制 → 通过共享 VMO 提交 → `berry_vmo_seal(0x1E)` 封死写权
//!   → compositor 只读消费渲染。
//!
//! 二进制布局（大端，确定性序列化——同一场景恒同字节串）：
//! ```text
//! [Header 16B]
//!   magic: u32 = 0x5343_4952 ("SCIR")
//!   version: u16 = 1
//!   node_count: u16
//!   payload_len: u32
//! [Nodes：每个定长 24B]
//!   kind: u8（Container=0/Text=1/Image=2/RRect=3）
//!   flags: u8
//!   reserved: u16
//!   x: i16, y: i16, w: u16, h: u16
//!   child_start: u16, child_count: u16
//!   payload_off: u32, payload_len: u16
//! [Payload：变长区（文本 UTF-8/色彩 RGBA/图像引用名）]
//! ```
//!
//! 纯面纪律：全序列化/反序列化/验证 = 零 I/O 纯函数（host 可测）。
#![cfg_attr(not(test), no_std)]

#[cfg(not(test))]
extern crate alloc;
#[cfg(not(test))]
use alloc::vec::Vec;

/// SceneIR 魔数："SCIR"。
pub const MAGIC: u32 = 0x5343_4952;
/// 协议版本。
pub const VERSION: u16 = 1;
/// Header 尺寸（字节）。
pub const HEADER_LEN: usize = 16;
/// 单节点定长（字节）。
pub const NODE_LEN: usize = 24;
/// 节点数上限（单页 4KiB 内自含：16+24*40+payload ≈ 1KiB）。
pub const MAX_NODES: usize = 40;

/// 节点类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeKind {
    Container = 0,
    Text = 1,
    Image = 2,
    RoundedRect = 3,
}

/// 场景节点（构建面——应用侧填充）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneNode {
    pub kind: NodeKind,
    /// x 坐标（逻辑像素，相对父容器原点）。
    pub x: i16,
    /// y 坐标。
    pub y: i16,
    /// 宽度。
    pub w: u16,
    /// 高度。
    pub h: u16,
    /// 子节点起始索引（同级连续存储）。
    pub child_start: u16,
    /// 子节点数。
    pub child_count: u16,
    /// payload 偏移（相对 payload 区首）。
    pub payload_off: u32,
    /// payload 长度。
    pub payload_len: u16,
}

/// 序列化结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneIr {
    pub nodes: [SceneNode; MAX_NODES],
    pub node_count: u16,
    pub payload: [u8; 1024],
    pub payload_len: u32,
}

impl SceneIr {
    pub fn new() -> SceneIr {
        SceneIr {
            nodes: [SceneNode {
                kind: NodeKind::Container,
                x: 0, y: 0, w: 0, h: 0,
                child_start: 0, child_count: 0,
                payload_off: 0, payload_len: 0,
            }; MAX_NODES],
            node_count: 0,
            payload: [0; 1024],
            payload_len: 0,
        }
    }

    /// 添加节点——返回索引；满返 None。
    pub fn push(&mut self, node: SceneNode) -> Option<u16> {
        if self.node_count as usize >= MAX_NODES {
            return None;
        }
        let idx = self.node_count;
        self.nodes[idx as usize] = node;
        self.node_count += 1;
        Some(idx)
    }

    /// 追加 payload——返回偏移；满返 None。
    pub fn push_payload(&mut self, data: &[u8]) -> Option<u32> {
        let off = self.payload_len;
        let end = off as usize + data.len();
        if end > self.payload.len() {
            return None;
        }
        self.payload[off as usize..end].copy_from_slice(data);
        self.payload_len = end as u32;
        Some(off)
    }

    /// 序列化为二进制（确定性——同场景恒同字节串）。
    pub fn serialize(&self) -> Vec<u8> {
        let total = HEADER_LEN + NODE_LEN * self.node_count as usize + self.payload_len as usize;
        let mut out = Vec::with_capacity(total);
        // Header
        out.extend_from_slice(&MAGIC.to_be_bytes());
        out.extend_from_slice(&VERSION.to_be_bytes());
        out.extend_from_slice(&self.node_count.to_be_bytes());
        out.extend_from_slice(&self.payload_len.to_be_bytes());
        out.extend_from_slice(&[0; 4]); // header 垫至 16B
        // Nodes
        for i in 0..self.node_count as usize {
            let n = &self.nodes[i];
            out.push(n.kind as u8);
            out.push(0); // flags
            out.extend_from_slice(&[0, 0]); // reserved
            out.extend_from_slice(&n.x.to_be_bytes());
            out.extend_from_slice(&n.y.to_be_bytes());
            out.extend_from_slice(&n.w.to_be_bytes());
            out.extend_from_slice(&n.h.to_be_bytes());
            out.extend_from_slice(&n.child_start.to_be_bytes());
            out.extend_from_slice(&n.child_count.to_be_bytes());
            out.extend_from_slice(&n.payload_off.to_be_bytes());
            out.extend_from_slice(&n.payload_len.to_be_bytes());
            out.extend_from_slice(&[0; 2]); // node 垫至 24B
        }
        // Payload
        out.extend_from_slice(&self.payload[..self.payload_len as usize]);
        out
    }

    /// 反序列化+全验证（compositor 侧消费前必过）。
    pub fn deserialize(bytes: &[u8]) -> Option<SceneIr> {
        if bytes.len() < HEADER_LEN {
            return None;
        }
        let magic = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != MAGIC {
            return None;
        }
        let ver = u16::from_be_bytes([bytes[4], bytes[5]]);
        if ver != VERSION {
            return None;
        }
        let nc = u16::from_be_bytes([bytes[6], bytes[7]]);
        if nc as usize > MAX_NODES {
            return None;
        }
        let pl = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let expected = HEADER_LEN + NODE_LEN * nc as usize + pl as usize;
        if bytes.len() != expected {
            return None;
        }
        let mut ir = SceneIr::new();
        ir.node_count = nc;
        ir.payload_len = pl;
        for i in 0..nc as usize {
            let base = HEADER_LEN + NODE_LEN * i;
            let n = &mut ir.nodes[i];
            n.kind = match bytes[base] {
                0 => NodeKind::Container,
                1 => NodeKind::Text,
                2 => NodeKind::Image,
                3 => NodeKind::RoundedRect,
                _ => return None,
            };
            n.x = i16::from_be_bytes([bytes[base + 4], bytes[base + 5]]);
            n.y = i16::from_be_bytes([bytes[base + 6], bytes[base + 7]]);
            n.w = u16::from_be_bytes([bytes[base + 8], bytes[base + 9]]);
            n.h = u16::from_be_bytes([bytes[base + 10], bytes[base + 11]]);
            n.child_start = u16::from_be_bytes([bytes[base + 12], bytes[base + 13]]);
            n.child_count = u16::from_be_bytes([bytes[base + 14], bytes[base + 15]]);
            n.payload_off = u32::from_be_bytes([bytes[base + 16], bytes[base + 17], bytes[base + 18], bytes[base + 19]]);
            n.payload_len = u16::from_be_bytes([bytes[base + 20], bytes[base + 21]]);
        }
        let payload_start = HEADER_LEN + NODE_LEN * nc as usize;
        ir.payload[..pl as usize].copy_from_slice(&bytes[payload_start..]);
        Some(ir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_scene() -> SceneIr {
        let mut ir = SceneIr::new();
        // 根容器
        let text_off = ir.push_payload(b"Hello Widget").unwrap();
        ir.push(SceneNode {
            kind: NodeKind::Container, x: 0, y: 0, w: 320, h: 160,
            child_start: 1, child_count: 1, payload_off: 0, payload_len: 0,
        }).unwrap();
        ir.push(SceneNode {
            kind: NodeKind::Text, x: 16, y: 24, w: 288, h: 40,
            child_start: 0, child_count: 0,
            payload_off: text_off, payload_len: 12,
        }).unwrap();
        ir
    }

    #[test]
    fn roundtrip() {
        let ir = sample_scene();
        let bytes = ir.serialize();
        let back = SceneIr::deserialize(&bytes).unwrap();
        assert_eq!(back.node_count, 2);
        assert_eq!(back.payload_len, 12);
        assert_eq!(back.nodes[0].kind, NodeKind::Container);
        assert_eq!(back.nodes[1].kind, NodeKind::Text);
        assert_eq!(&back.payload[..12], b"Hello Widget");
    }

    #[test]
    fn deterministic_serialization() {
        let a = sample_scene().serialize();
        let b = sample_scene().serialize();
        assert_eq!(a, b, "同场景恒同字节串");
    }

    #[test]
    fn bad_magic_rejected() {
        let bytes = sample_scene().serialize();
        let mut bad = bytes.clone();
        bad[0] = 0xFF;
        assert!(SceneIr::deserialize(&bad).is_none());
    }

    #[test]
    fn truncated_rejected() {
        let bytes = sample_scene().serialize();
        assert!(SceneIr::deserialize(&bytes[..bytes.len() - 1]).is_none());
    }

    #[test]
    fn node_cap_enforced() {
        let mut ir = SceneIr::new();
        for _ in 0..MAX_NODES {
            assert!(ir.push(SceneNode { kind: NodeKind::Container, x: 0, y: 0, w: 1, h: 1, child_start: 0, child_count: 0, payload_off: 0, payload_len: 0 }).is_some());
        }
        assert!(ir.push(SceneNode { kind: NodeKind::Container, x: 0, y: 0, w: 1, h: 1, child_start: 0, child_count: 0, payload_off: 0, payload_len: 0 }).is_none());
    }

    #[test]
    fn binary_layout() {
        let bytes = sample_scene().serialize();
        assert_eq!(bytes[0..4], [0x53, 0x43, 0x49, 0x52]); // "SCIR"
        assert_eq!(&bytes[4..6], &[0, 1]); // version 1
        assert_eq!(&bytes[6..8], &[0, 2]); // 2 nodes
    }
}
