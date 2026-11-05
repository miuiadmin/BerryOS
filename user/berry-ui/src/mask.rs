//! ChangeMask 位面单源（GUI-6 段E S7——`docs/GUI-6实施设计草案.md` §6
//! 冻结契约 + §5 字段映射表；ADR #60 实验轨道 B 面，cfg(feature = "s7")
//! 编译——A 面（默认构建）不编入本模块，三漏斗现状语义逐字保持）。
//!
//! - 属性级生死门：`set_props`/`set_text`/anim Layout 三漏斗统一产
//!   `ChangeMask` → [`apply_change`]（lib.rs）单源分派——GEOM 位走
//!   重排脏 + 旧界/父链 damage，PAINT 位走纯重绘脏 + 本界 damage，
//!   mask=0 零操作直返（D2）。
//! - 位面（§6 表逐位冻结）：W/H/GAP/BG/FG/TEXT/SCALE/CROSS 一位一义；
//!   GEOM = W|H|GAP|SCALE|CROSS（0b1100_0111 = 199——§6.5 代码块字面量
//!   0b1110_0111 系草案笔误，以 D2 冻结表 W|H|GAP|SCALE|CROSS 实算为准；
//!   231 含 bit5=TEXT 会破「分组不叠」）、PAINT = BG|FG|TEXT
//!   （0b0011_1000）两组不叠——分组即分派，无第三义。
//! - [`props_diff`] = §5 字段映射表单源（Props → mask）：供
//!   `set_props` B 面；变体失配臂保守返 GEOM ∪ PAINT 全置（禁静默
//!   吞——kind 门在 set_props 先验，失配到此处 = 状态已异常）。
//!
//! 零 panic / 零分配 / 禁浮点（同 crate 纪律）。

use crate::desc::Props;
use crate::layout::CrossAlign;

/// 线性容器（Col/Row）字段差单源（§5 表）：gap→GAP、cross→CROSS、
/// w→W、h→H。两型字段面同构但型别名不共享——值参直提绕（型安全面
/// 由 kind 门先验保证 Col 对 Col / Row 对 Row）。
fn linear_diff(
    a: (u32, Option<u32>, Option<u32>, CrossAlign),
    b: (u32, Option<u32>, Option<u32>, CrossAlign),
) -> ChangeMask {
    let mut m = ChangeMask::NONE;
    if a.0 != b.0 {
        m = m.union(ChangeMask::GAP);
    }
    if a.3 != b.3 {
        m = m.union(ChangeMask::CROSS);
    }
    if a.1 != b.1 {
        m = m.union(ChangeMask::W);
    }
    if a.2 != b.2 {
        m = m.union(ChangeMask::H);
    }
    m
}

/// 属性级变更位面（§6 冻结）：u8 八位一位一义，pub 元组结构（测试直
/// 钉位值——契约面公开）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChangeMask(pub u8);

impl ChangeMask {
    /// 空 mask（D2 零操作判据）。
    pub const NONE: ChangeMask = ChangeMask(0);
    /// 宽度位：内在宽可能变（w 字段 / 变长换文 / 格列）。
    pub const W: ChangeMask = ChangeMask(1 << 0);
    /// 高度位：内在高可能变（h 字段 / scale 档 / 变长换文）。
    pub const H: ChangeMask = ChangeMask(1 << 1);
    /// 间距位：Col/Row/Pad/Grid gap 族（Pad all 兼任）。
    pub const GAP: ChangeMask = ChangeMask(1 << 2);
    /// 背景位：纯重绘（Rect bg / Text style bg）。
    pub const BG: ChangeMask = ChangeMask(1 << 3);
    /// 前景位：纯重绘（Text fg / Icon fg）。
    pub const FG: ChangeMask = ChangeMask(1 << 4);
    /// 文本位：内容变（等长换文 = 纯 PAINT；变长由 W|H 追加）。
    pub const TEXT: ChangeMask = ChangeMask(1 << 5);
    /// 缩放位：字号档变（内在 w/h 双轴变——属 GEOM 组）。
    pub const SCALE: ChangeMask = ChangeMask(1 << 6);
    /// 十字轴位：对齐变（重摆子位——属 GEOM 组）。
    pub const CROSS: ChangeMask = ChangeMask(1 << 7);

    /// 几何组（§6 冻结 0b1100_0111——§6.5 字面量 0b1110_0111 系草案
    /// 笔误，以 D2 冻结表实算为准）：命中即重排路径。
    pub const GEOM: ChangeMask =
        ChangeMask(Self::W.0 | Self::H.0 | Self::GAP.0 | Self::SCALE.0 | Self::CROSS.0);
    /// 纯绘组（§6 冻结 0b0011_1000）：命中即重绘路径。
    pub const PAINT: ChangeMask = ChangeMask(Self::BG.0 | Self::FG.0 | Self::TEXT.0);

    /// 空判（D2：mask=0 → apply_change 零操作直返）。
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// 交集判（分组分派：GEOM/PAINT 命中判）。
    pub const fn intersects(&self, other: ChangeMask) -> bool {
        self.0 & other.0 != 0
    }

    /// 并集（位累积）。
    pub const fn union(&self, other: ChangeMask) -> ChangeMask {
        ChangeMask(self.0 | other.0)
    }
}

/// §5 字段映射表单源：Props 新旧差 → 位集。kind 门在 set_props 先验
/// （失配 NodeBad），失配臂到此处不可达——保守返全置（GEOM ∪ PAINT）
/// loud 于静默吞。Col/Row/Grid 的 w/h 字段 → W/H（§5 表 Image 行同型
/// 补全——显式定尺寸变即内在变）。
pub fn props_diff(old: &Props, new: &Props) -> ChangeMask {
    match (old, new) {
        (Props::Root, Props::Root) => ChangeMask::NONE, // 根无字段面
        (Props::Rect(a), Props::Rect(b)) => {
            let mut m = ChangeMask::NONE;
            if a.w != b.w {
                m = m.union(ChangeMask::W);
            }
            if a.h != b.h {
                m = m.union(ChangeMask::H);
            }
            if a.bg != b.bg {
                m = m.union(ChangeMask::BG);
            }
            m
        }
        (Props::Text(a), Props::Text(b)) => {
            let mut m = ChangeMask::NONE;
            if a.text.as_bytes() != b.text.as_bytes() {
                m = m.union(ChangeMask::TEXT);
                if a.text.len != b.text.len {
                    // 变长（§11 术语）：内在宽变 → W|H 追加（D3 同型）
                    m = m.union(ChangeMask::W).union(ChangeMask::H);
                }
            }
            if a.style.scale != b.style.scale {
                m = m.union(ChangeMask::SCALE); // 档位变 = 内在双轴变（GEOM 组）
            }
            if a.style.fg != b.style.fg {
                m = m.union(ChangeMask::FG);
            }
            if a.style.bg != b.style.bg {
                m = m.union(ChangeMask::BG);
            }
            m
        }
        (Props::Col(a), Props::Col(b)) => linear_diff(
            (a.gap, a.w, a.h, a.cross),
            (b.gap, b.w, b.h, b.cross),
        ),
        (Props::Row(a), Props::Row(b)) => linear_diff(
            (a.gap, a.w, a.h, a.cross),
            (b.gap, b.w, b.h, b.cross),
        ),
        (Props::Pad(a), Props::Pad(b)) => {
            if a.all != b.all {
                ChangeMask::GAP // 兼任间距位（§5 表）
            } else {
                ChangeMask::NONE
            }
        }
        (Props::Grid(a), Props::Grid(b)) => {
            let mut m = ChangeMask::NONE;
            if a.cols != b.cols || a.rows != b.rows {
                // 轨格重排保守面（§5 表）：内在/间距皆可能变
                m = m.union(ChangeMask::W)
                    .union(ChangeMask::H)
                    .union(ChangeMask::GAP);
            }
            if a.gap != b.gap {
                m = m.union(ChangeMask::GAP);
            }
            if a.cross != b.cross {
                m = m.union(ChangeMask::CROSS);
            }
            if a.w != b.w {
                m = m.union(ChangeMask::W);
            }
            if a.h != b.h {
                m = m.union(ChangeMask::H);
            }
            m
        }
        (Props::Image(a), Props::Image(b)) => {
            let mut m = ChangeMask::NONE;
            if a.res != b.res {
                m = m.union(ChangeMask::W).union(ChangeMask::H); // 保守（§5 表）
            }
            if a.w != b.w {
                m = m.union(ChangeMask::W);
            }
            if a.h != b.h {
                m = m.union(ChangeMask::H);
            }
            m
        }
        (Props::Icon(a), Props::Icon(b)) => {
            let mut m = ChangeMask::NONE;
            if a.id != b.id {
                m = m.union(ChangeMask::W).union(ChangeMask::H); // 保守（§5 表）
            }
            if a.fg != b.fg {
                m = m.union(ChangeMask::FG);
            }
            if a.w != b.w {
                m = m.union(ChangeMask::W);
            }
            if a.h != b.h {
                m = m.union(ChangeMask::H);
            }
            m
        }
        // kind 门先验后不可达（失配 = 状态异常）：保守全置，禁静默吞。
        _ => ChangeMask::GEOM.union(ChangeMask::PAINT),
    }
}
