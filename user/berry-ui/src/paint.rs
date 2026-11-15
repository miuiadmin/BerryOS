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
//! - **组件件**（Phase 2 两批：Button/Toggle/Slider/Progress 首批 +
//!   Check/Radio/Step/Seg/Divider 第二批）：入可见叶列，固定色板 +
//!   已有原语组合发射（「组件 v1 固定色板」块）——Button 底/描边 +
//!   居中标签、Toggle 胶囊轨 + 滑块、Slider 轨 + 填充 + 拖动圆、
//!   Progress 轨 + 填充条、Check 方框 + 双矩形勾、Radio 八片环 +
//!   内圆、Step 三段底 + 加减号/值字形、Seg 等分底 + active 高亮 +
//!   分隔线、Divider 单 fill_rect 带 inset；不透明度不可证（描边/
//!   透明底有洞）——恒不计遮挡源（与 Text 同列保守正确）。Stack/
//!   Spacer 无视觉面（容器/占位——子与邻件各自发射）。
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

// ---------- 组件 v1 固定色板（Phase 2 两批——渲染起步面） ----------
//
// v1 无样式系统：固定色值起步（文档化豁免——同 lib.rs `PRESSED_BG`
// 先例；主题系统落地后由此块迁出）。pub 供 golden 测试值级对拍。

/// 主色：Filled 按钮底 / Toggle 开轨 / Slider 填充与拖动圆 / Progress
/// 填充 / Check 选中框与勾 / Radio 选中环与内圆 / Seg active 段。
pub const ACCENT: Color = Color { r: 0x19, g: 0x76, b: 0xD2 };
/// Toggle 关轨灰。
pub const TRACK_OFF: Color = Color { r: 0x9E, g: 0x9E, b: 0x9E };
/// Slider/Progress 底轨浅灰；Step/Seg 底。
pub const TRACK_DIM: Color = Color { r: 0xBD, g: 0xBD, b: 0xBD };
/// Toggle/Slider 滑块白（拖动圆兼）。
pub const KNOB: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Outlined 按钮描边灰；Check/Radio 未选框环；Step/Seg 分隔线；
/// Divider 线体。
pub const OUTLINE: Color = Color { r: 0x66, g: 0x66, b: 0x66 };
/// Filled 按钮标签白（ACCENT 底上）。
pub const ON_ACCENT: Color = Color { r: 0xFF, g: 0xFF, b: 0xFF };
/// Outlined/Text 按钮标签深色；Step 值/加减号前景（TRACK_DIM 底上）。
pub const ON_SURFACE: Color = Color { r: 0x21, g: 0x21, b: 0x21 };
/// 按钮标签字号档（S8x16——cell 8×16；`crate::BUTTON_CHAR_W` 与
/// cell_w() 同值，布局测量/渲染推进单源）。Step 值/加减号同档共用。
const BUTTON_LABEL_FONT: FontSize = FontSize::S8x16;
/// 按钮标签字高（= S8x16 cell_h——居中摆位用）。
const BUTTON_LABEL_H: i64 = 16;
/// Outlined 按钮描边厚度（四边 fill_rect 组合）。
const BUTTON_BORDER: i32 = 2;
/// Toggle 滑块边距（轨道内滑块四周留白）。
const TOGGLE_KNOB_MARGIN: i32 = 4;
/// Slider 轨道线厚。
const SLIDER_TRACK_H: i32 = 4;
/// Check 方框描边厚（四边 fill_rect 组合——Outlined 按钮同形）。
const CHECK_BORDER: i32 = 2;
/// Step 两侧加减号区带宽（节点宽 120 的两翼 36；中段值区 = 余 48）。
const STEP_ZONE_W: i32 = 36;
/// Step 区带分隔线宽。
const STEP_DIV_W: i32 = 2;
/// Seg 段间分隔线宽。
const SEG_DIV_W: i32 = 2;

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
    // 恒不计遮挡源）。组件件（Phase 2 两批）入可见叶列（各自
    // emit_node 合成视觉）；不透明度不可证（Outlined 有洞 / Text 式
    // 无底）——与 Text 同列**恒不计遮挡源**（保守正确）。余组件件
    // 无视觉面——与容器同列走查（子仍发射；渲染挂后续刀；Stack =
    // 叠层容器、Spacer = 纯占位；Refresh/Collapse/Card = 包裹容器）。
    match n.kind {
        ViewKind::Rect
        | ViewKind::Text
        | ViewKind::Image
        | ViewKind::Icon
        | ViewKind::Button
        | ViewKind::Toggle
        | ViewKind::Slider
        | ViewKind::Progress
        | ViewKind::Check
        | ViewKind::Radio
        | ViewKind::Step
        | ViewKind::Seg
        | ViewKind::Divider => {}
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
    // 组件件（Phase 2 两批）入可见叶列（emit_node 合成视觉）；余组
    // 件件同容器列——无视觉面，子仍走查（渲染挂后续刀）。
    match n.kind {
        ViewKind::Rect
        | ViewKind::Text
        | ViewKind::Image
        | ViewKind::Icon
        | ViewKind::Button
        | ViewKind::Toggle
        | ViewKind::Slider
        | ViewKind::Progress
        | ViewKind::Check
        | ViewKind::Radio
        | ViewKind::Step
        | ViewKind::Seg
        | ViewKind::Divider => {
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
        // 组件件（Phase 2 两批——固定色板 + 已有原语组合，见模块头
        // 色板块）：Button = 底/描边 + 居中标签；Toggle = 胶囊轨道 +
        // 滑块；Slider = 轨道线 + 填充 + 拖动圆；Progress = 轨道 +
        // 填充条（style Circular/Indeterminate v1 统一线性渲染——圆/
        // 动画面挂后续刀，数据已随 props 携载）；Check = 方框描边 +
        // 选中双矩形勾；Radio = 八片环 + 选中内圆；Step = 三段底 +
        // 加减号/值字形（S8x16 同档）；Seg = 等分底 + active 高亮 +
        // 段间分隔线；Divider = 单 fill_rect 带 inset（走向随容器主轴
        // ——v1 恒横线，纵线面挂后续刀）。
        Props::Button(p) => {
            match p.style {
                ButtonStyle::Filled => {
                    fill(enc, b.x, b.y, b.w, b.h, ACCENT, rep);
                    emit_widget_text(p.label.as_bytes(), b.x, b.w, b.y, b.h, ON_ACCENT, rest, enc, rep);
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
                    emit_widget_text(p.label.as_bytes(), b.x, b.w, b.y, b.h, ON_SURFACE, rest, enc, rep);
                }
                ButtonStyle::Text => {
                    emit_widget_text(p.label.as_bytes(), b.x, b.w, b.y, b.h, ON_SURFACE, rest, enc, rep);
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
        // Check：方框描边（Outlined 按钮同形四 fill_rect）+ 选中态双
        // fill_rect 勾（短臂下角块 + 长臂上行条——24×24 定格比例，内
        // 在恒 24 时无缩放面）。
        Props::Check(p) => {
            let bc = if p.checked { ACCENT } else { OUTLINE };
            let t = CHECK_BORDER.min(b.w / 2).min(b.h / 2).max(1);
            fill(enc, b.x, b.y, b.w, t, bc, rep);
            fill(enc, b.x, b.y + b.h - t, b.w, t, bc, rep);
            fill(enc, b.x, b.y + t, t, b.h - 2 * t, bc, rep);
            fill(enc, b.x + b.w - t, b.y + t, t, b.h - 2 * t, bc, rep);
            if p.checked {
                // 勾形两片：短臂（左下角块）+ 长臂（上行竖条），交叠
                // 成勾角。
                fill(enc, b.x + 5, b.y + 12, 4, 4, ACCENT, rep);
                fill(enc, b.x + 8, b.y + 6, 3, 9, ACCENT, rep);
            }
        }
        // Radio：八片环近似（四直段 + 四角块——八角同族；选中态内圆
        // = oct3 半径减半，与 Toggle 滑块同核）。
        Props::Radio(p) => {
            let rc = if p.selected { ACCENT } else { OUTLINE };
            let d = b.w.min(b.h);
            let ox = b.x + (b.w - d) / 2;
            let oy = b.y + (b.h - d) / 2;
            ring8(enc, ox, oy, d, rc, rep);
            if p.selected {
                let di = d / 2;
                let ix = b.x + (b.w - di) / 2;
                let iy = b.y + (b.h - di) / 2;
                oct3(enc, ix, iy, di, ACCENT, rep);
            }
        }
        // Step：三段 = 底 + 两道分隔线 + 三组字形（左 '-'、中值（十
        // 进制 u32）、右 '+'——S8x16 同档，区带内双轴居中）。
        Props::Step(p) => {
            fill(enc, b.x, b.y, b.w, b.h, TRACK_DIM, rep);
            let z = STEP_ZONE_W.min(b.w / 2).max(0);
            fill(enc, b.x + z - STEP_DIV_W, b.y, STEP_DIV_W, b.h, OUTLINE, rep);
            fill(enc, b.x + b.w - z, b.y, STEP_DIV_W, b.h, OUTLINE, rep);
            emit_widget_text(b"-", b.x, z, b.y, b.h, ON_SURFACE, rest, enc, rep);
            let mut buf = [0u8; 10];
            let n = fmt_u32(p.value, &mut buf);
            emit_widget_text(&buf[..n], b.x + z, b.w - 2 * z, b.y, b.h, ON_SURFACE, rest, enc, rep);
            emit_widget_text(b"+", b.x + b.w - z, z, b.y, b.h, ON_SURFACE, rest, enc, rep);
        }
        // Seg：等分底 + active 段 ACCENT 高亮 + 段间分隔线（active 后
        // 发——高亮段两界分隔线恒可见）。
        Props::Seg(p) => {
            fill(enc, b.x, b.y, b.w, b.h, TRACK_DIM, rep);
            let cnt = p.segment_count.max(1) as i64;
            let sw = b.w as i64 / cnt;
            if sw > 0 {
                let ax = b.x as i64 + sw * p.active.min(p.segment_count) as i64;
                fill(
                    enc,
                    i64_to_i32_sat(ax),
                    b.y,
                    i64_to_i32_sat(sw),
                    b.h,
                    ACCENT,
                    rep,
                );
                let mut i = 1i64;
                while i < cnt {
                    let sx = b.x as i64 + sw * i;
                    fill(
                        enc,
                        i64_to_i32_sat(sx - SEG_DIV_W as i64 / 2),
                        b.y,
                        SEG_DIV_W,
                        b.h,
                        OUTLINE,
                        rep,
                    );
                    i += 1;
                }
            }
        }
        // Divider：单 fill_rect 带 inset（thickness 已入节点高——final_
        // size 臂；宽随父）。inset 和 > 宽 = 零面积空绘（fill 卫跳过）。
        Props::Divider(p) => {
            let is = crate::damage::u32_to_i32_sat(p.inset_start);
            let ie = crate::damage::u32_to_i32_sat(p.inset_end);
            let x = b.x.saturating_add(is);
            let w = b.w.saturating_sub(is).saturating_sub(ie);
            fill(enc, x, b.y, w, b.h, OUTLINE, rep);
        }
        // Root/Col/Row/Pad/Grid + 余组件件：无视觉内容（子树各自发射）
        _ => {}
    }
    rep.drawn_px = rep.drawn_px.saturating_add(b.intersect(vp_rect).area());
}

// ---------- 组件件发射共核（原语组合小工具） ----------

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
/// 动圆 / Radio 选中内圆共用。
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

/// 八片近似环（Radio 圆环——「不发明新绘制指令」纪律的八 fill_rect
/// 组合：上下/左右四直段 + 四角块斜切；直段内缩 s、环厚 t = d/8、角
/// 块 t×t 钉角内侧，段块相邻交叠连线）。比例随 d 推导（24 → t=3、
/// s=4、角块偏移 2）。
fn ring8(enc: &mut CommandEncoder, x: i32, y: i32, d: i32, c: Color, rep: &mut PaintReport) {
    if d <= 2 {
        return;
    }
    let t = (d / 8).max(1);
    let s = (d / 6).max(t + 1);
    let off = s - t + 1;
    fill(enc, x + s, y, d - 2 * s, t, c, rep);
    fill(enc, x + s, y + d - t, d - 2 * s, t, c, rep);
    fill(enc, x, y + s, t, d - 2 * s, c, rep);
    fill(enc, x + d - t, y + s, t, d - 2 * s, c, rep);
    fill(enc, x + off, y + off, t, t, c, rep);
    fill(enc, x + d - off - t, y + off, t, t, c, rep);
    fill(enc, x + off, y + d - off - t, t, t, c, rep);
    fill(enc, x + d - off - t, y + d - off - t, t, t, c, rep);
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

/// 组件件字形发射共核（Button 标签 / Step 三段字形共用）：S8x16 档 +
/// 给定区带内水平居中、节点纵界内垂直居中；与 Text 臂同走 glyph run
/// scratch 切分——余量不足截断尾部字形同 §7 用例义务。fg 随调用方定、
/// bg 恒透明。
fn emit_widget_text<'a>(
    bytes: &[u8],
    x0: i32,
    zone_w: i32,
    y0: i32,
    h: i32,
    fg: Color,
    rest: &mut &'a mut [GlyphPos],
    enc: &mut CommandEncoder<'a>,
    rep: &mut PaintReport,
) {
    let take = bytes.len().min(rest.len());
    if take == 0 {
        return;
    }
    let Some(piece) = take_scratch(rest, take) else { return };
    let atlas = BUTTON_LABEL_FONT.atlas();
    let text_w = bytes.len() as i64 * crate::BUTTON_CHAR_W as i64;
    let mut x = x0 as i64 + (zone_w as i64 - text_w).max(0) / 2;
    let y = i64_to_i32_sat(y0 as i64 + (h as i64 - BUTTON_LABEL_H).max(0) / 2);
    for i in 0..take {
        piece[i] = GlyphPos { ch: bytes[i], x: i64_to_i32_sat(x), y };
        x += atlas.advance(bytes[i]) as i64;
    }
    let glyphs: &'a [GlyphPos] = piece;
    let run = GlyphRun { atlas, glyphs, fg, bg: None };
    if enc.draw_glyph_run(run).is_ok() {
        rep.cmds += 1;
    }
}

/// u32 十进制 ASCII 化（no_std 无 itoa——Step 值字形专用；返字节数
/// ≤ 10，低位在后翻转写出）。
fn fmt_u32(mut v: u32, out: &mut [u8; 10]) -> usize {
    let mut tmp = [0u8; 10];
    let mut n = 0usize;
    loop {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for i in 0..n {
        out[i] = tmp[n - 1 - i];
    }
    n
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
