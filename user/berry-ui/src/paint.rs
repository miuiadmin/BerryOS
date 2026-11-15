//! berry-ui paint 段（GUI-3——`docs/GUI-3实施设计草案.md` §3.5）。
//!
//! - **前到后剔除、后到前发射**（OH ui_lite 同路）：按 damage 矩形逐
//!   个走树——逆前序（尾兄先、子先于父 = 画家算法后画者先看）维护
//!   [`RectSet`] 已覆盖集；**仅 Rect 叶子（实心不透明）计遮挡源**
//!   （Text 蜂格有洞不计、Image/Icon 恒不计——§4.3，保守正确）。
//!   可见区（bounds ∩ damage − covered）空 → skip（`culled++`）。
//! - 发射序 = 收集序的逆 = 前序（画家算法背到前：后者盖前者）。逐节
//!   点：Rect → `fill_rect`；Text → GlyphPos run 组进调用方 scratch →
//!   `draw_glyph_run`（`style.bg` 衬底透传，None = 透明底——§6
//!   TextStyle 字段语义优先）；Image/Icon → `draw_image`（§4.3——
//!   Argb 像素经 D28 icons 槽，blit = 源内在尺寸 v1 无缩放）。
//! - **组件四件**（Button/Toggle/Slider/Progress——Phase 2 首批）：入
//!   可见叶列，固定色板 + 已有原语组合发射（「组件四件 v1 固定色板」
//!   块）——Button 底/描边 + 居中标签、Toggle 胶囊轨 + 滑块、Slider
//!   轨 + 填充 + 拖动圆、Progress 轨 + 填充条；不透明度不可证（描边/
//!   透明底有洞）——恒不计遮挡源（与 Text 同列保守正确）。
//! - **尾部必录 `Present{damage 包络}`**（soft no-op 也录——FULL →
//!   viewport、空集 → 零矩形）。
//! - overdraw_x256 = drawn_px×256/vp 面素（发射足迹 ∩ viewport；u128
//!   中间量饱和收口）——预算 ≤ [`crate::OVERDRAW_BUDGET_X256`] 的断
//!   言面在测试侧。
//! - DisplayList `'a` 钉**调用方 scratch**（不钉引擎）：多个 Text run
//!   经 `split_at_mut` 游标取互斥子片（零 unsafe——mem::replace 借移
//!   产出与整体同寿命 'a 的独占片）；scratch 不足的 run 截断到余量。
//! - 零 panic / 零分配：cmds 满帽（CmdError::Full）停录不炸；树环/死
//!   槽防御以 nodes.len() 为帽。

use crate::damage::{i64_to_i32_sat, DamageSet, RectI, RectSet};
use crate::desc::{ButtonStyle, Props, ViewKind};
use crate::metrics::IconSource;
use crate::Node;
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun, ImageSrc, Rect as GfxRect};
use berry_gfx::font::FontSize;

/// 槽索引空哨兵（§6 Node 注：NONE = u32::MAX——hit.rs 同值，本模块自
/// 带防跨模块未定名依赖）。
const NONE: u32 = u32::MAX;

// ---------- 组件四件 v1 固定色板（Phase 2 首批——渲染起步面） ----------
//
// v1 无样式系统：固定色值起步（文档化豁免——同 lib.rs `PRESSED_BG`
// 先例；主题系统落地后由此块迁出）。pub 供 golden 测试值级对拍。

/// 主色：Filled 按钮底 / Toggle 开轨 / Slider 填充与拖动圆 / Progress
/// 填充。
pub const ACCENT: Color = Color { r: 0x19, g: 0x76, b: 0xD2 };
/// Toggle 关轨灰。
pub const TRACK_OFF: Color = Color { r: 0x9E, g: 0x9E, b: 0x9E };
/// Slider/Progress 底轨浅灰。
pub const TRACK_DIM: Color = Color { r: 0xBD, g: 0xBD, b: 0xBD };
/// Toggle/Slider 滑块白（拖动圆兼）。
pub const KNOB: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Outlined 按钮描边灰。
pub const OUTLINE: Color = Color { r: 0x66, g: 0x66, b: 0x66 };
/// Filled 按钮标签白（ACCENT 底上）。
pub const ON_ACCENT: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Outlined/Text 按钮标签深色。
pub const ON_SURFACE: Color = Color { r: 0x21, g: 0x21, b: 0x21 };
/// 按钮标签字号档（S8x16——cell 8×16；`crate::BUTTON_CHAR_W` 与
/// cell_w() 同值，布局测量/渲染推进单源）。
const BUTTON_LABEL_FONT: FontSize = FontSize::S8x16;
/// 按钮标签字高（= S8x16 cell_h——居中摆位用）。
const BUTTON_LABEL_H: i64 = 16;
/// Outlined 按钮描边厚度（四边 fill_rect 组合）。
const BUTTON_BORDER: i32 = 2;
/// Toggle 滑块边距（轨道内滑块四周留白）。
const TOGGLE_KNOB_MARGIN: i32 = 4;
/// Slider 轨道线厚。
const SLIDER_TRACK_H: i32 = 4;

/// 遮挡覆盖集容量（damage 矩形 ≤ 8 的两倍余量；溢出保守少记覆盖 →
/// 多画不少画，见 RectSet 类型注）。
const COVERED_CAP: usize = 16;

/// 单 damage 矩形的可见发射序缓冲（>`2*MAX_NODES` 可见叶子的病态树触发
/// cull 溢出 → `emit_walk_all` 回退直发射——不丢画面；D10 单源常量
/// 表达式 = MAX_NODES=512 契约的两倍余量）。
pub(crate) const ORDER_CAP: usize = 2 * crate::MAX_NODES;

/// paint 产出统计（§6 冻结字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaintReport {
    /// 成功录入 display list 的命令数（含尾 Present）。
    pub cmds: usize,
    /// 发射足迹面素（节点界 ∩ viewport 之和）。
    pub drawn_px: u64,
    /// 可见区空被剔除的叶子数（含 damage 外与被盖两面）。
    pub culled: u32,
    /// overdraw 倍率×256（drawn_px×256/vp 面素）。
    pub overdraw_x256: u32,
}

/// paint 主入口（§6 冻结内部签名 + GUI-6 D28 icons 进参）：`scratch`
/// 与 encoder 的 `'a` 同源——glyph run 引用钉调用方缓冲，引擎不持引用。
/// `icons` = D7 资源缝经 D28 槽供给（段C：Image/Icon 叶发射消费——
/// `'a` 同源让 Argb 像素借用随命令表存续）。
pub(crate) fn paint<'a>(
    nodes: &[Option<Node>],
    root: u32,
    vp: (u32, u32),
    damage: &DamageSet,
    scratch: &'a mut [GlyphPos],
    enc: &mut CommandEncoder<'a>,
    icons: &'a dyn IconSource,
) -> PaintReport {
    let mut rep =
        PaintReport { cmds: 0, drawn_px: 0, culled: 0, overdraw_x256: 0 };
    if root == NONE {
        return rep; // 无树（lib.rs 入口已挡，防御双保险——不录 Present）
    }
    match nodes.get(root as usize) {
        Some(Some(n)) if n.alive => {}
        _ => return rep,
    }
    let vp_rect = RectI::new(
        0,
        0,
        crate::damage::u32_to_i32_sat(vp.0),
        crate::damage::u32_to_i32_sat(vp.1),
    );

    // damage 矩形族（FULL → 单一 viewport 矩形）
    let mut dmgs = [RectI::new(0, 0, 0, 0); berry_guiproto::DAMAGE_MAX];
    let mut n_dmgs = 0usize;
    if damage.full {
        dmgs[0] = vp_rect;
        n_dmgs = 1;
    } else {
        for r in damage.iter() {
            let ri = RectI::from_wire(r);
            if !ri.is_empty() && n_dmgs < dmgs.len() {
                dmgs[n_dmgs] = ri;
                n_dmgs += 1;
            }
        }
    }

    // scratch 游标（互斥子片切分——每 Text run 冻结一片）
    let mut rest: &'a mut [GlyphPos] = scratch;

    for d in dmgs[..n_dmgs].iter() {
        let mut covered: RectSet<COVERED_CAP> = RectSet::new();
        let mut order = [NONE; ORDER_CAP];
        let mut bnds = [RectI::new(0, 0, 0, 0); ORDER_CAP];
        let mut n_order = 0usize;
        let mut overflow = false;
        let culled_before = rep.culled;
        cull_walk(
            nodes, root, 0, 0, d, &mut covered, &mut order, &mut bnds, &mut n_order,
            &mut rep.culled, &mut overflow, 0,
        );
        if overflow {
            // 病态巨树回退（>ORDER_CAP 可见叶子）：本 damage 矩形整段重走
            // ——前序直发射、无遮挡剔除、无序缓冲（painter 序由走查序
            // 自证，overdraw 偏高但**不丢画面**；主路径的 culled 计数回
            // 卷到本段前值，由回退走查按 damage 外叶子重计）。契约树
            // （N ≤ MAX_NODES=512 < ORDER_CAP/2）结构性不达此面——防御
            // 路径不设竞速断言，仅 offscreen 回归测试钉板行为。
            rep.culled = culled_before;
            emit_walk_all(nodes, root, 0, 0, d, &vp_rect, icons, &mut rest, enc, &mut rep, 0);
        } else {
            // 发射 = 收集序之逆（前序 = 画家算法背到前）
            for k in (0..n_order).rev() {
                emit_node(nodes, order[k], &bnds[k], &vp_rect, icons, &mut rest, enc, &mut rep);
            }
        }
    }

    // 尾部必录 Present{damage 包络}（FULL → viewport；空集 → 零矩形）
    let pres = present_rect(damage, &vp_rect);
    if enc.present(GfxRect::new(pres.x, pres.y, pres.w, pres.h)).is_ok() {
        rep.cmds += 1;
    }

    // overdraw 倍率（u128 中间量；零面素 viewport 折 0）
    let vp_area = vp.0 as u128 * vp.1 as u128;
    rep.overdraw_x256 = if vp_area == 0 {
        0
    } else {
        ((rep.drawn_px as u128 * 256 / vp_area).min(u32::MAX as u128)) as u32
    };
    rep
}

/// Present 载荷：FULL → viewport、有矩形 → 包络、空集 → 零矩形。
fn present_rect(damage: &DamageSet, vp_rect: &RectI) -> RectI {
    if damage.full {
        return *vp_rect;
    }
    match damage.envelope() {
        Some(r) => RectI::from_wire(&r),
        None => RectI::new(0, 0, 0, 0),
    }
}

/// scratch 游标切分：产出与整体同寿命 `'a` 的独占子片（mem::replace
/// 借移——零 unsafe 的流式 split）；余量不足 n → None。
fn take_scratch<'a>(rest: &mut &'a mut [GlyphPos], n: usize) -> Option<&'a mut [GlyphPos]> {
    if n == 0 || rest.len() < n {
        return None;
    }
    let owned: &'a mut [GlyphPos] = core::mem::replace(rest, &mut []);
    let (head, tail): (&'a mut [GlyphPos], &'a mut [GlyphPos]) = owned.split_at_mut(n);
    *rest = tail;
    Some(head)
}

/// 逆前序剔除走查（front-to-back）：子（尾兄先）→ 自身。可见叶子记
/// 入发射序（槽 + 视觉界快照免重算）；Rect 叶子界∩damage 计遮挡源。
/// `overflow` = 发射序缓冲满帽哨兵（调用方切回退走查——本走查的收集
/// 产物在溢出面被整段弃用）。
#[allow(clippy::too_many_arguments)]
fn cull_walk(
    nodes: &[Option<Node>],
    slot: u32,
    ex: i64,
    ey: i64,
    d: &RectI,
    covered: &mut RectSet<COVERED_CAP>,
    order: &mut [u32; ORDER_CAP],
    bnds: &mut [RectI; ORDER_CAP],
    n_order: &mut usize,
    culled: &mut u32,
    overflow: &mut bool,
    hops: u32,
) {
    if hops as usize > nodes.len() {
        return; // 环防御
    }
    let Some(n) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else { return };
    if !n.alive {
        return;
    }
    let bx = ex + n.x as i64 + n.ox as i64;
    let by = ey + n.y as i64 + n.oy as i64;

    // 子先：沿 next_sib 走到链尾再 prev_sib 回走（尾兄先 = 后画者先看）
    let mut tail = n.first_child;
    let mut guard = 0usize;
    while tail != NONE {
        guard += 1;
        if guard > nodes.len() {
            return; // next_sib 环防御
        }
        match nodes.get(tail as usize) {
            Some(Some(c)) if c.alive => {
                if c.next_sib == NONE {
                    break;
                }
                tail = c.next_sib;
            }
            _ => return,
        }
    }
    let mut sib = tail;
    let mut back = 0usize;
    while sib != NONE {
        back += 1;
        if back > nodes.len() {
            return; // prev_sib 环防御
        }
        cull_walk(nodes, sib, bx, by, d, covered, order, bnds, n_order, culled, overflow, hops + 1);
        match nodes.get(sib as usize) {
            Some(Some(c)) if c.alive => sib = c.prev_sib,
            _ => return,
        }
    }

    // 自身（子树最底 = 最后画）。容器无视觉内容不参与（Grid 同列——
    // 子格各自发射）；Image/Icon = 可见叶（§4.3——Text 同列走查，
    // 恒不计遮挡源）。组件四件（Button/Toggle/Slider/Progress——
    // Phase 2 首批）入可见叶列（各自 emit_node 合成视觉）；不透明度
    // 不可证（Outlined 有洞 / Text 式无底）——与 Text 同列**恒不计
    // 遮挡源**（保守正确）。余组件件无视觉面——与容器同列走查（子
    // 仍发射；渲染挂后续刀；Stack = 叠层容器；Refresh/Collapse/
    // Card = 包裹容器）。
    match n.kind {
        ViewKind::Rect
        | ViewKind::Text
        | ViewKind::Image
        | ViewKind::Icon
        | ViewKind::Button
        | ViewKind::Toggle
        | ViewKind::Slider
        | ViewKind::Progress => {}
        ViewKind::Root
        | ViewKind::Col
        | ViewKind::Row
        | ViewKind::Pad
        | ViewKind::Grid
        | ViewKind::Scroll
        | ViewKind::List
        | ViewKind::Tabs
        | ViewKind::Stack
        | ViewKind::Spacer
        | ViewKind::Divider
        | ViewKind::Check
        | ViewKind::Radio
        | ViewKind::Step
        | ViewKind::Seg
        | ViewKind::Badge
        | ViewKind::Sbar
        | ViewKind::Select
        | ViewKind::Input
        | ViewKind::Edit
        | ViewKind::Canvas
        | ViewKind::Tip
        | ViewKind::Refresh
        | ViewKind::SwipeItem
        | ViewKind::DatePick
        | ViewKind::TimePick
        | ViewKind::Collapse
        | ViewKind::Chip
        | ViewKind::Fab
        | ViewKind::Card
        | ViewKind::Gauge
        | ViewKind::Qr
        | ViewKind::DragReorder => return,
    }
    let b = RectI::new(i64_to_i32_sat(bx), i64_to_i32_sat(by), n.w, n.h);
    let vis = b.intersect(d);
    if vis.is_empty() || covered.is_covered(&vis) {
        *culled += 1; // §3.5：可见区空 → skip（culled++）
        return;
    }
    if n.kind == ViewKind::Rect {
        covered.add(vis); // 实心不透明才计遮挡源（Text 蜂格有洞、Image/Icon 与组件四件恒不计——§4.3 + 组件不透明度不可证，保守正确）
    }
    if *n_order < ORDER_CAP {
        order[*n_order] = slot;
        bnds[*n_order] = b;
        *n_order += 1;
    } else {
        *culled += 1; // 病态巨树缓冲收口（>512 可见叶子——契约 2x 外）
        *overflow = true; // 调用方切回退走查（可见不丢画面）
    }
}

/// 回退走查（cull_walk 溢出面）：前序（自身先于子、头兄先）**直接发
/// 射**——无遮挡剔除、无序缓冲（画家序由走查序自证：先访问者先画、
/// 后访问者覆盖 = 与主路径前序发射同序）。damage 外叶子计 culled（与
/// 主路径语义一致）；环/死槽防御同主路径以 nodes.len() 为帽。
#[allow(clippy::too_many_arguments)]
fn emit_walk_all<'a>(
    nodes: &[Option<Node>],
    slot: u32,
    ex: i64,
    ey: i64,
    d: &RectI,
    vp_rect: &RectI,
    icons: &'a dyn IconSource,
    rest: &mut &'a mut [GlyphPos],
    enc: &mut CommandEncoder<'a>,
    rep: &mut PaintReport,
    hops: u32,
) {
    if hops as usize > nodes.len() {
        return; // 环防御
    }
    let Some(n) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else { return };
    if !n.alive {
        return;
    }
    let bx = ex + n.x as i64 + n.ox as i64;
    let by = ey + n.y as i64 + n.oy as i64;

    // 自身先（前序 = 画家算法背到前：先画者先发射）。容器无视觉内容不
    // 参与（Grid 同列）；Image/Icon = 可见叶同主路径发射（§4.3）。
    // 组件四件（Phase 2 首批）入可见叶列（emit_node 合成视觉）；余组
    // 件件同容器列——无视觉面，子仍走查（渲染挂后续刀）。
    match n.kind {
        ViewKind::Rect
        | ViewKind::Text
        | ViewKind::Image
        | ViewKind::Icon
        | ViewKind::Button
        | ViewKind::Toggle
        | ViewKind::Slider
        | ViewKind::Progress => {
            let b = RectI::new(i64_to_i32_sat(bx), i64_to_i32_sat(by), n.w, n.h);
            if b.intersect(d).is_empty() {
                rep.culled += 1; // damage 外叶子
            } else {
                emit_node(nodes, slot, &b, vp_rect, icons, rest, enc, rep);
            }
        }
        ViewKind::Root
        | ViewKind::Col
        | ViewKind::Row
        | ViewKind::Pad
        | ViewKind::Grid
        | ViewKind::Scroll
        | ViewKind::List
        | ViewKind::Tabs
        | ViewKind::Stack
        | ViewKind::Spacer
        | ViewKind::Divider
        | ViewKind::Check
        | ViewKind::Radio
        | ViewKind::Step
        | ViewKind::Seg
        | ViewKind::Badge
        | ViewKind::Sbar
        | ViewKind::Select
        | ViewKind::Input
        | ViewKind::Edit
        | ViewKind::Canvas
        | ViewKind::Tip
        | ViewKind::Refresh
        | ViewKind::SwipeItem
        | ViewKind::DatePick
        | ViewKind::TimePick
        | ViewKind::Collapse
        | ViewKind::Chip
        | ViewKind::Fab
        | ViewKind::Card
        | ViewKind::Gauge
        | ViewKind::Qr
        | ViewKind::DragReorder => {}
    }

    // 子：头兄起流序（painter 序：后画者后访问 = 覆盖在前画者上）。
    let mut c = n.first_child;
    let mut guard = 0usize;
    while c != NONE {
        guard += 1;
        if guard > nodes.len() {
            return; // 子链环防御
        }
        match nodes.get(c as usize) {
            Some(Some(ch)) if ch.alive => {
                emit_walk_all(nodes, c, bx, by, d, vp_rect, icons, rest, enc, rep, hops + 1);
                c = ch.next_sib;
            }
            _ => return, // 链上死槽 = 树结构异常，防御放弃余段
        }
    }
}

/// 单节点发射（painter 序段）：Rect → fill_rect；Text → glyph run 组
/// scratch 子片；Image/Icon → `draw_image`（§4.3：源 = `pixels(id)`
/// Argb——blit 尺寸 = 源内在尺寸、v1 无缩放，`fg` Icon = 调色前景
/// （Argb 惰性——alpha 键即 tint，文档化）/ Image = 中性黑（无调色语
/// 义），`bg` 恒 None 透明底；dims 与 pixels 不一致 = 供件矛盾，loud
/// 后跳过不录——空绘不炸）。命令满帽（CmdError::Full）停录不炸；
/// drawn_px 按节点界 ∩ viewport 计（发射足迹——与录入成功无关）。
fn emit_node<'a>(
    nodes: &[Option<Node>],
    slot: u32,
    b: &RectI,
    vp_rect: &RectI,
    icons: &'a dyn IconSource,
    rest: &mut &'a mut [GlyphPos],
    enc: &mut CommandEncoder<'a>,
    rep: &mut PaintReport,
) {
    let Some(n) = nodes.get(slot as usize).and_then(|o| o.as_ref()) else { return };
    match &n.props {
        Props::Rect(p) => {
            if enc.fill_rect(GfxRect::new(b.x, b.y, b.w, b.h), p.bg).is_ok() {
                rep.cmds += 1;
            }
        }
        Props::Image(p) => {
            emit_image(
                icons, p.res, Color { r: 0, g: 0, b: 0 }, p.w, p.h, b.x, b.y, enc, rep,
            );
        }
        Props::Icon(p) => {
            emit_image(icons, p.id, p.fg, p.w, p.h, b.x, b.y, enc, rep);
        }
        Props::Text(p) => {
            // run 截断到 scratch 余量（容量不足的尾部字形丢弃——调用
            // 方 scratch 尺寸义务见 §7 用例）
            let take = (p.text.len as usize).min(rest.len());
            if take > 0 {
                if let Some(piece) = take_scratch(rest, take) {
                    // 逐字形累计推进（痛点⑥根修 03 号 §1.3-3）：推进量经
                    // GlyphAtlas::advance——三档点阵缺省 = cell_w()，与旧
                    // `i * cw` 等宽推进逐值恒等（零漂移）；HD 档覆写读
                    // ADVANCE_HD 比例表（「i」9px 而非 32px——观感劣化
                    // 根因消除）。测量面配套 = ProportionalTextMetrics
                    //（metrics.rs，布局/渲染同源）。
                    let atlas = p.style.scale.atlas();
                    let mut x = b.x as i64;
                    for i in 0..take {
                        piece[i] = GlyphPos {
                            ch: p.text.bytes[i],
                            x: i64_to_i32_sat(x),
                            y: b.y,
                        };
                        x += atlas.advance(p.text.bytes[i]) as i64;
                    }
                    let glyphs: &'a [GlyphPos] = piece; // 冻结为共享——run 引用钉调用方缓冲
                    let run = GlyphRun {
                        atlas,
                        glyphs,
                        fg: p.style.fg,
                        bg: p.style.bg,
                    };
                    if enc.draw_glyph_run(run).is_ok() {
                        rep.cmds += 1;
                    }
                }
            }
        }
        // 组件四件（Phase 2 首批——固定色板 + 已有原语组合，见模块头
        // 色板块）：Button = 底/描边 + 居中标签；Toggle = 胶囊轨道 +
        // 滑块；Slider = 轨道线 + 填充 + 拖动圆；Progress = 轨道 +
        // 填充条（style Circular/Indeterminate v1 统一线性渲染——圆/
        // 动画面挂后续刀，数据已随 props 携载）。
        Props::Button(p) => {
            match p.style {
                ButtonStyle::Filled => {
                    fill(enc, b.x, b.y, b.w, b.h, ACCENT, rep);
                    emit_button_label(p, ON_ACCENT, b, rest, enc, rep);
                }
                ButtonStyle::Outlined => {
                    // 描边 = 四 fill_rect 组合（v1 无 stroke 原语——「已有
                    // 原语组合」纪律）；底不填（Color 无 alpha——Outlined
                    // 语义 = 透应用底，实心 surface 会盖住背景）。
                    let t = BUTTON_BORDER.min(b.w / 2).min(b.h / 2).max(1);
                    fill(enc, b.x, b.y, b.w, t, OUTLINE, rep);
                    fill(enc, b.x, b.y + b.h - t, b.w, t, OUTLINE, rep);
                    fill(enc, b.x, b.y + t, t, b.h - 2 * t, OUTLINE, rep);
                    fill(enc, b.x + b.w - t, b.y + t, t, b.h - 2 * t, OUTLINE, rep);
                    emit_button_label(p, ON_SURFACE, b, rest, enc, rep);
                }
                ButtonStyle::Text => {
                    emit_button_label(p, ON_SURFACE, b, rest, enc, rep);
                }
            }
        }
        Props::Toggle(p) => {
            let track = if p.checked { ACCENT } else { TRACK_OFF };
            capsule_h(enc, b.x, b.y, b.w, b.h, track, rep);
            let d = (b.h - 2 * TOGGLE_KNOB_MARGIN).max(0); // 滑块边长（32 高 → 24）
            let kx = if p.checked {
                b.x + b.w - TOGGLE_KNOB_MARGIN - d
            } else {
                b.x + TOGGLE_KNOB_MARGIN
            };
            oct3(enc, kx, b.y + TOGGLE_KNOB_MARGIN, d, KNOB, rep);
        }
        Props::Slider(p) => {
            let yc = b.y + b.h / 2;
            let ty = yc - SLIDER_TRACK_H / 2;
            fill(enc, b.x, ty, b.w, SLIDER_TRACK_H, TRACK_DIM, rep);
            // 拖动圆行程 = [x+inset, x+w−inset]（inset = 圆半径——与
            // lib.rs slider_permille_at 命中映射同式单源
            // crate::SLIDER_KNOB_D/2）；填充至圆心。
            let inset = (crate::SLIDER_KNOB_D / 2) as i32;
            let usable = (b.w as i64 - 2 * inset as i64).max(1);
            let frac = p.value.min(crate::PERMILLE_MAX) as i64 * usable
                / crate::PERMILLE_MAX as i64;
            let thumb_cx = b.x as i64 + inset as i64 + frac;
            let fill_w = i64_to_i32_sat(thumb_cx - b.x as i64);
            fill(enc, b.x, ty, fill_w, SLIDER_TRACK_H, ACCENT, rep);
            let kd = crate::SLIDER_KNOB_D as i32;
            oct3(
                enc,
                i64_to_i32_sat(thumb_cx) - kd / 2,
                yc - kd / 2,
                kd,
                KNOB,
                rep,
            );
        }
        Props::Progress(p) => {
            fill(enc, b.x, b.y, b.w, b.h, TRACK_DIM, rep);
            let fw = p.value.min(crate::PERMILLE_MAX) as i64 * b.w as i64
                / crate::PERMILLE_MAX as i64;
            fill(enc, b.x, b.y, i64_to_i32_sat(fw), b.h, ACCENT, rep);
        }
        // Root/Col/Row/Pad/Grid + 余组件件：无视觉内容（子树各自发射）
        _ => {}
    }
    rep.drawn_px = rep.drawn_px.saturating_add(b.intersect(vp_rect).area());
}

// ---------- 组件四件发射共核（原语组合小工具） ----------

/// fill_rect 收口：零面积跳过 + cmds 计数（CmdError::Full 静默停录同
/// 既有臂——满帽不炸）。
fn fill(
    enc: &mut CommandEncoder,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    c: Color,
    rep: &mut PaintReport,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    if enc.fill_rect(GfxRect::new(x, y, w, h), c).is_ok() {
        rep.cmds += 1;
    }
}

/// 八角近似圆（v1 无圆原语——「不发明新绘制指令」纪律的三 fill_rect
/// 组合：上/下条内缩 d/4 厚 d/3、中条满宽）。Toggle 滑块 / Slider 拖
/// 动圆共用。
fn oct3(enc: &mut CommandEncoder, x: i32, y: i32, d: i32, c: Color, rep: &mut PaintReport) {
    if d <= 0 {
        return;
    }
    let inset = d / 4;
    let edge = d / 3;
    fill(enc, x + inset, y, d - 2 * inset, edge, c, rep);
    fill(enc, x, y + edge, d, d - 2 * edge, c, rep);
    fill(enc, x + inset, y + d - edge, d - 2 * inset, edge, c, rep);
}

/// 胶囊近似（横向圆角轨道——Toggle 专用；左右端帽纵向内缩 h/4、宽
/// h/3，中段满高）。
fn capsule_h(
    enc: &mut CommandEncoder,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    c: Color,
    rep: &mut PaintReport,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let cap = h / 3;
    let inset = h / 4;
    fill(enc, x, y + inset, cap, h - 2 * inset, c, rep);
    fill(enc, x + cap, y, w - 2 * cap, h, c, rep);
    fill(enc, x + w - cap, y + inset, cap, h - 2 * inset, c, rep);
}

/// 按钮标签发射（ButtonProps 无样式面——fg 随按钮风格定、bg 恒透明；
/// S8x16 档 + 节点界内双轴居中；与 Text 臂同走 glyph run scratch 切分
/// ——余量不足截断尾部字形同 §7 用例义务）。
fn emit_button_label<'a>(
    p: &crate::desc::ButtonProps,
    fg: Color,
    b: &RectI,
    rest: &mut &'a mut [GlyphPos],
    enc: &mut CommandEncoder<'a>,
    rep: &mut PaintReport,
) {
    let take = (p.label.len as usize).min(rest.len());
    if take == 0 {
        return;
    }
    let Some(piece) = take_scratch(rest, take) else { return };
    let atlas = BUTTON_LABEL_FONT.atlas();
    let text_w = p.label.len as i64 * crate::BUTTON_CHAR_W as i64;
    let mut x = b.x as i64 + (b.w as i64 - text_w).max(0) / 2;
    let y = i64_to_i32_sat(b.y as i64 + (b.h as i64 - BUTTON_LABEL_H).max(0) / 2);
    for i in 0..take {
        piece[i] = GlyphPos { ch: p.label.bytes[i], x: i64_to_i32_sat(x), y };
        x += atlas.advance(p.label.bytes[i]) as i64;
    }
    let glyphs: &'a [GlyphPos] = piece;
    let run = GlyphRun { atlas, glyphs, fg, bg: None };
    if enc.draw_glyph_run(run).is_ok() {
        rep.cmds += 1;
    }
}

/// Image/Icon 叶发射共核（§4.3）：`pixels(id)` 借用 → Argb/ScaledArgb
///（恰长 `px.len() == w*h` 由 submit 期硬门双保险——此处先验矛盾源即
/// 跳过）。未注册分档 loud：默认空供件 = 静默空绘（guest 未注册不炸）
///；真供件缺注册/pixels-dims 不一致 = debug_assert loud（注册 bug）。
fn emit_image<'a>(
    icons: &'a dyn IconSource,
    id: u16,
    fg: Color,
    dw: Option<u32>,
    dh: Option<u32>,
    x: i32,
    y: i32,
    enc: &mut CommandEncoder<'a>,
    rep: &mut PaintReport,
) {
    let px = icons.pixels(id);
    if px.is_none() && !icons.is_empty() {
        debug_assert!(false, "IconSource 真供件缺注册 id={id}（pixels None——注册 bug）");
    }
    if let Some(px) = px {
        match icons.dims(id) {
            // 一致源：dims × pixels 恰长 → Argb 直发（blit = 源内在尺寸）
            Some((w, h)) if px.len() as u64 == w as u64 * h as u64 => {
                let same = dw.is_none() || dh.is_none() || (dw == Some(w) && dh == Some(h));
                let smooth = icons.smooth_scale(id);
                let src = match (dw, dh) {
                    (Some(tw), Some(th)) if !same && smooth => ImageSrc::SmoothArgb {
                        sw: w,
                        sh: h,
                        dw: tw,
                        dh: th,
                        px,
                    },
                    (Some(tw), Some(th)) if !same => ImageSrc::ScaledArgb {
                        sw: w,
                        sh: h,
                        dw: tw,
                        dh: th,
                        px,
                    },
                    _ => ImageSrc::Argb { w, h, px },
                };
                if enc.draw_image(src, x, y, fg, None).is_ok() {
                    rep.cmds += 1;
                }
            }
            _ => {
                debug_assert!(
                    false,
                    "IconSource id={id} 供件矛盾（dims 与 pixels 恰长不符）——跳过不录"
                );
            }
        }
    }
}
