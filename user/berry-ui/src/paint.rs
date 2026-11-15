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
//!   Check/Radio/Step/Seg/Divider 第二批）：入可见叶列，**主题 token
//!   取色**（维八暗色刀：原「组件 v1 固定色板」整块迁 theme.rs
//!   `WidgetColors`——`tokens` 入参按档取，light 档同值零漂移；豁免
//!   清单见下方色板迁出注）+ 已有原语组合发射——Button 底/描边 +
//!   居中标签、Toggle 胶囊轨 + 滑块、Slider 轨 + 填充 + 拖动圆、
//!   Progress 轨 + 填充条、Check 方框 + 双矩形勾、Radio 八片环 +
//!   内圆、Step 三段底 + 加减号/值字形、Seg 等分底 + active 高亮 +
//!   分隔线、Divider 单 fill_rect 带 inset；不透明度不可证（描边/
//!   透明底有洞）——恒不计遮挡源（与 Text 同列保守正确）。Stack/
//!   Spacer 无视觉面（容器/占位——子与邻件各自发射）。批 A 追加
//!   Tabs 条区/Sbar 轨 + thumb/Badge 红底角标；第三批追加（本刀）
//!   ——显示三件 Tip 实色衬底 + 文本 / Badge·Sbar 见批 A；组合四件
//!   Collapse header 带 + 箭头（收起态子被走查守卫跳过——三面同
//!   守卫）、Chip 直角胶囊底 + 标签 + 选中色变、Fab 八角圆底 + '+'
//!   占位图标 + 扩展标签带、Card 双 fill_rect 阴影 + 卡面（Collapse/
//!   Card = 视觉容器：子先画盖其上）。批 C/D/E 追加 Select 下拉框 /
//!   Input·Edit·Canvas 输入面 / Refresh·SwipeItem·DatePick·TimePick
//!   表单件；批 F（末刀）追加 Gauge 粗条 + 百分值文本、Qr 三角定位
//!   方块 + 确定性数据图案（占位）、DragReorder 三杠手柄——41 kind
//!   渲染面全收口。
//! - **波 2 移植件 XA 三面**（ui-wave2 对账第 1/2/3 项——详见
//!   `PORT-XA.md`）：①真圆角档（opt-in，[`set_true_corners`]）——
//!   Toggle 轨/Chip 底/Card 阴影+卡面走 `Cmd::FillRectRounded` 行 LUT
//!   （默认档保持 v1 直角/胶囊近似——既有 golden 钉板零漂移）；②
//!   disabled 档——[`PressedVis`] 扩三态查表（normal/pressed/
//!   disabled，disabled 优先），首批消费 Button/Toggle/Check/Radio/
//!   Seg（色值 theme.rs 单源）；③clip 面——Scroll/List kind 即登记
//!   开裁剪（eff 有效窗 + MARK 段标记 + berry-gfx 裁剪栈回放——溢出
//!   内容不再画出视口侵入后续区；泛化 per-slot 侧表挂 lib.rs 面后
//!   续刀）。
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
use crate::desc::{ButtonStyle, Orientation, Props, ViewKind};
use crate::metrics::IconSource;
use crate::Node;
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, GlyphRun, ImageSrc, Rect as GfxRect};
use berry_gfx::font::FontSize;
use core::sync::atomic::{AtomicBool, Ordering};

/// 槽索引空哨兵（§6 Node 注：NONE = u32::MAX——hit.rs 同值，本模块自
/// 带防跨模块未定名依赖）。
const NONE: u32 = u32::MAX;

/// clip 段标记哨兵（波 2 横向原语④）：`order` 流内嵌 PUSH/POP 段标
/// （与槽值/NONE 不撞——槽 < MAX_NODES=512 ≪ u32::MAX−2）。
const MARK_POP: u32 = u32::MAX - 1;
const MARK_PUSH: u32 = u32::MAX - 2;

// ---------- 组件 v1 色板（维八暗色刀：固定色板已整块迁 theme.rs） ----------
//
// 迁移台账：原「组件 v1 固定色板」29 色（Phase 2 起步面的过渡豁免）
// 已逐色迁 `theme.rs` 组件色板组（`WidgetColors`——light 档同值迁移
// golden 零漂移，dark 档逐色给对应值；迁移点对照旧值见 theme.rs 各
// 常量 doc）。本文件自此**零固定色**：所有组件取色走 `paint` 的
// `tokens: &theme::Tokens` 入参（`tokens.w.*` 组件面 / `tokens.*`
// 语义面）。
//
// 豁免清单（豁免数只减不能增——tests/dark.rs `paint_fixed_palette_
// exempt_gate` grep 门钉板；新增固定色必须先在 theme.rs 立双档 token；
// 每项豁免在**使用点**以半角方括号 paint-exempt 标记登记）：
// 1. emit_node Image 臂的黑色哨兵实参——Image 像素即色（ARGB 经 D28
//    icons 槽），`fg` 形参对该臂是死实参非视觉色，无 token 语义可挂
//    （Icon 臂的 p.fg 是 props 数据面，非本文件色）。

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
/// Tabs 指示条厚度（页签行底缘）。
const TABS_INDICATOR_H: i32 = 3;
/// Sbar 滑块细边（横竖向最小厚度 4）。
const SBAR_MIN_THICK: i32 = 4;
/// Input caret 竖线宽。
const INPUT_CARET_W: i32 = 2;
/// pressed 视觉活动集（波2 批 A：press_map active 条目的 visual 槽值
/// 快照——emit 期查表换暗色底；容量 = PRESS_MAP_MAX 同源上界，调用方
/// 保证不超）。波 2 移植件 XA 扩 disabled 档：三态查表
/// normal/pressed/disabled（[`NodeVisState`]——disabled 优先于
/// pressed，禁用件不表达按压反馈；写侧 set_disabled 挂 lib.rs 面后
/// 续刀，本面只立查表机制）。
pub struct PressedVis {
    pub slots: [u32; 32],
    pub n: usize,
    /// disabled 槽集（空集 = 全 normal——`Ui::pressed_snapshot` 现行写
    /// 法只填 slots/n，本字段缺省零值即常态）。
    pub disabled: [u32; 32],
    pub n_disabled: usize,
}

/// 组件视觉三态（波 2 移植件 XA：PressedVis 查表机制的 disabled 档
/// 扩展——源实现 NodeState 验收三态的 paint 消费面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeVisState {
    Normal,
    Pressed,
    Disabled,
}

impl PressedVis {
    /// 空集。
    pub fn empty() -> Self {
        PressedVis { slots: [0; 32], n: 0, disabled: [0; 32], n_disabled: 0 }
    }

    /// slot 是否在活动集（线性查——n ≤ PRESS_MAP_MAX 小表）。
    pub fn contains(&self, slot: u32) -> bool {
        self.slots[..self.n].contains(&slot)
    }

    /// slot 是否在禁用集（线性查——同 contains 小表纪律）。
    pub fn is_disabled(&self, slot: u32) -> bool {
        self.disabled[..self.n_disabled].contains(&slot)
    }

    /// slot 三态查表（disabled > pressed > normal——禁用锁死优先，
    /// 源实现消息机 Disabled 锁死语义的 paint 面同律）。
    pub fn state(&self, slot: u32) -> NodeVisState {
        if self.is_disabled(slot) {
            NodeVisState::Disabled
        } else if self.contains(slot) {
            NodeVisState::Pressed
        } else {
            NodeVisState::Normal
        }
    }
}

// ---------- 真圆角渲染档（波 2 移植件 XA——横向原语③ 消费开关） ----------
//
// 主树 v1 近似（直角 fill / capsule_h 胶囊三 fill）为**默认档**——既有
// golden 钉板面（widgets 族逐值对拍）零漂移；真圆角档 opt-in：Toggle 轨 /
// Chip 底 / Card 阴影+卡面走 `Cmd::FillRectRounded` 行 LUT（berry-gfx
// 波 2 横向原语③）。进程级开关 = 无 lib.rs 样式侧表的最小消费面
// （per-node radius 数据面挂样式系统后续刀；画廊波/宿主翻档消费）。

/// 真圆角档开关（默认 false = v1 近似；core AtomicBool——no_std 可用）。
static TRUE_CORNERS: AtomicBool = AtomicBool::new(false);

/// 切换真圆角渲染档（宿主/测试面；返回先前值便于消费方还原）。
pub fn set_true_corners(on: bool) -> bool {
    TRUE_CORNERS.swap(on, Ordering::Relaxed)
}

/// 真圆角档当前值。
pub fn true_corners() -> bool {
    TRUE_CORNERS.load(Ordering::Relaxed)
}

/// 输入内容快照（批 D：lib 侧表 → paint 消费面；槽值 keyed）。
pub struct InputTexts {
    pub entries: [Option<(u32, crate::desc::TextBuf)>; crate::INPUT_TEXT_MAX],
    pub n: usize,
}

impl InputTexts {
    /// 空表。
    pub fn empty() -> Self {
        InputTexts { entries: [None; crate::INPUT_TEXT_MAX], n: 0 }
    }

    /// slot 的内容（无则空切片）。
    pub fn text_of(&self, slot: u32) -> &[u8] {
        for e in self.entries[..self.n].iter().flatten() {
            if e.0 == slot {
                return e.1.as_bytes();
            }
        }
        &[]
    }
}
/// Button 压下态底色已迁 theme.rs `tokens.w.accent_pressed`（tokens.w 取——
/// 20% 遮罩律双档对应）。
/// Card 每档阴影偏移（px/档——elevation 0..=3 → 条宽 0/2/4/6）。
const CARD_SHADOW_STEP: i32 = 2;
/// Collapse 箭头边长（12×12——三带三角形逼近，随 header 高收缩）。
const COLLAPSE_ARROW: i32 = 12;
/// Collapse 箭头带数（三带：横带递减 = 朝下 / 竖带递减 = 朝右）。
const COLLAPSE_ARROW_BANDS: i32 = 3;
/// Collapse 箭头左缘留白（header 内 x 偏移）。
const COLLAPSE_ARROW_X: i32 = 8;
/// Check 方框描边厚（四边 fill_rect 组合——Outlined 按钮同形）。
const CHECK_BORDER: i32 = 2;
/// Step 两侧加减号区带宽（节点宽 120 的两翼 36；中段值区 = 余 48）。
const STEP_ZONE_W: i32 = 36;
/// Step 区带分隔线宽。
const STEP_DIV_W: i32 = 2;
/// Seg 段间分隔线宽。
const SEG_DIV_W: i32 = 2;
/// Gauge 值条厚（批 F：64×64 表盘上带粗条——Progress 8px 轨的两倍）。
const GAUGE_BAR_H: i32 = 16;
/// DragReorder 短杠长（批 F：≡ 手柄三杠，24 宽内居中）。
const DRAG_BAR_W: i32 = 12;
/// DragReorder 短杠厚（= 杠间间隙——36 高内 5×bh=10 占位居中）。
const DRAG_BAR_H: i32 = 2;

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
    pressed: &PressedVis,
    focus_slot: u32,
    texts: &InputTexts,
    tokens: &crate::theme::Tokens,
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
            emit_walk_all(nodes, root, 0, 0, d, &vp_rect, icons, pressed, focus_slot, texts, tokens, &mut rest, enc, &mut rep, 0);
        } else {
            // 发射 = 收集序之逆（前序 = 画家算法背到前）；MARK_PUSH/POP
            // 在逆序流里读作「PUSH 子树 POP」（收集序注见 cull_walk——
            // 波 2 横向原语④ clip 段标记）。
            for k in (0..n_order).rev() {
                match order[k] {
                    MARK_PUSH => {
                        // clip 段开：界取 bnds 槽（收集期与标记同槽落位）。
                        let r = bnds[k];
                        if enc.push_clip(GfxRect::new(r.x, r.y, r.w, r.h)).is_ok() {
                            rep.cmds += 1;
                        }
                    }
                    MARK_POP => {
                        if enc.pop_clip().is_ok() {
                            rep.cmds += 1;
                        }
                    }
                    slot => {
                        emit_node(nodes, slot, &bnds[k], &vp_rect, icons, pressed, focus_slot, texts, tokens, &mut rest, enc, &mut rep);
                    }
                }
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
///
/// 波 2 移植件 XA 扩两面：①`d` 参语义扩为**有效窗 eff**（= damage ∩
/// 祖先 clip 界——叶子可见区按 eff 判，Scroll/List 子树溢出部分整剪）；
/// ②clip 节点子树段以 MARK_POP/`子树`/MARK_PUSH(界) 序入流——发射取
/// 逆恰成 `PUSH 子树 POP` 嵌套段（berry-gfx 裁剪栈回放）。clip 开关 =
/// kind 即登记（Scroll/List——「登记即开」的最小内嵌形态；泛化
/// per-slot 侧表挂 lib.rs 面后续刀）。
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

    // clip 子窗：空窗 → 子树整剪（不复发不标段）。
    let clip_here = matches!(n.kind, ViewKind::Scroll | ViewKind::List);
    let child_eff = if clip_here { d.intersect(&RectI::new(i64_to_i32_sat(bx), i64_to_i32_sat(by), n.w, n.h)) } else { *d };
    let mark_clip = clip_here && !child_eff.is_empty();

    // 子先：沿 next_sib 走到链尾再 prev_sib 回走（尾兄先 = 后画者先看）。
    // clip 段开标记先入流（收集序 POP 前置——发射逆序读作 PUSH 首位；
    // 标记溢出 = 段界破缺 → overflow 切回退走查）。
    // 收起态 Collapse：子不发射（第三批三面同守卫——layout 不摆位 +
    // paint 不发射 + hit 不命中；陈旧 bounds 一致跳过）。
    if mark_clip {
        if *n_order < ORDER_CAP {
            order[*n_order] = MARK_POP;
            bnds[*n_order] = RectI::new(0, 0, 0, 0);
            *n_order += 1;
        } else {
            *overflow = true;
        }
    }
    let kids_live = !matches!(&n.props, Props::Collapse(cp) if !cp.expanded);
    if !(clip_here && child_eff.is_empty()) {
        let mut tail = if kids_live { n.first_child } else { NONE };
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
            cull_walk(nodes, sib, bx, by, &child_eff, covered, order, bnds, n_order, culled, overflow, hops + 1);
            match nodes.get(sib as usize) {
                Some(Some(c)) if c.alive => sib = c.prev_sib,
                _ => return,
            }
        }
    }
    // clip 段闭标记尾入流（发射逆序读作 POP 末位；界携 child_eff）。
    if mark_clip && *n_order < ORDER_CAP {
        order[*n_order] = MARK_PUSH;
        bnds[*n_order] = child_eff;
        *n_order += 1;
    } else if mark_clip {
        *overflow = true; // 标记溢出 = 段界破缺——整段弃用切回退走查
    }

    // 自身（子树最底 = 最后画）。容器无视觉内容不参与（Grid 同列——
    // 子格各自发射）；Image/Icon = 可见叶（§4.3——Text 同列走查，
    // 恒不计遮挡源）。组件件（Phase 2 三批 + 波2 批 C/D/E/F）入可见
    // 叶列（各自 emit_node 合成视觉）；不透明度不可证（Outlined 有
    // 洞 / Text 式无底）——与 Text 同列**恒不计遮挡源**（保守正确）。
    // Collapse/Card = 视觉容器（header 带/卡面自发射，子先画盖其
    // 上）。纯容器余员（Root/Col/Row/Pad/Grid/Scroll/List/Stack/
    // Spacer）无自身视觉面——子仍各自发射；批 F 后 41 kind 全渲染，
    // 无「渲染挂后续刀」残余。
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
        | ViewKind::Divider
        | ViewKind::Tabs
        | ViewKind::Sbar
        | ViewKind::Badge
        | ViewKind::Tip
        | ViewKind::Collapse
        | ViewKind::Chip
        | ViewKind::Fab
        | ViewKind::Card
        | ViewKind::Select
        | ViewKind::Input
        | ViewKind::Edit
        | ViewKind::Canvas
        | ViewKind::Refresh
        | ViewKind::SwipeItem
        | ViewKind::DatePick
        | ViewKind::TimePick
        // 批 F（末刀三件）：Gauge/Qr/DragReorder 入可见叶列——41 kind
        // 渲染面收口（return 分支仅剩纯容器）。
        | ViewKind::Gauge
        | ViewKind::Qr
        | ViewKind::DragReorder => {}
        ViewKind::Root
        | ViewKind::Col
        | ViewKind::Row
        | ViewKind::Pad
        | ViewKind::Grid
        | ViewKind::Scroll
        | ViewKind::List
        | ViewKind::Stack
        | ViewKind::Spacer => return,
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
///
/// 波 2 移植件 XA：`d` 参语义同主路径扩为 eff 有效窗 + clip 段直接
/// Push/Pop 包夹（前序天然正确序；空窗跳整段——cull_walk 注同律）。
#[allow(clippy::too_many_arguments)]
fn emit_walk_all<'a>(
    nodes: &[Option<Node>],
    slot: u32,
    ex: i64,
    ey: i64,
    d: &RectI,
    vp_rect: &RectI,
    icons: &'a dyn IconSource,
    pressed: &PressedVis,
    focus_slot: u32,
    texts: &InputTexts,
    tokens: &crate::theme::Tokens,
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
    let b = RectI::new(i64_to_i32_sat(bx), i64_to_i32_sat(by), n.w, n.h);
    let clip_here = matches!(n.kind, ViewKind::Scroll | ViewKind::List);
    let child_eff = if clip_here { d.intersect(&b) } else { *d };

    // 自身先（前序 = 画家算法背到前：先画者先发射）。容器无视觉内容不
    // 参与（Grid 同列）；Image/Icon = 可见叶同主路径发射（§4.3）。
    // 组件件（Phase 2 三批）入可见叶列（emit_node 合成视觉）；余组
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
        | ViewKind::Divider
        | ViewKind::Tabs
        | ViewKind::Sbar
        | ViewKind::Badge
        | ViewKind::Tip
        | ViewKind::Collapse
        | ViewKind::Chip
        | ViewKind::Fab
        | ViewKind::Card
        | ViewKind::Select
        | ViewKind::Input
        | ViewKind::Edit
        | ViewKind::Canvas
        | ViewKind::Refresh
        | ViewKind::SwipeItem
        | ViewKind::DatePick
        | ViewKind::TimePick
        // 批 F（末刀三件）：Gauge/Qr/DragReorder 入发射列（cull_walk
        // 可见叶列同源——回退走查路径同律收口）。
        | ViewKind::Gauge
        | ViewKind::Qr
        | ViewKind::DragReorder => {
            if b.intersect(d).is_empty() {
                rep.culled += 1; // damage 外叶子
            } else {
                emit_node(nodes, slot, &b, vp_rect, icons, pressed, focus_slot, texts, tokens, rest, enc, rep);
            }
        }
        ViewKind::Root
        | ViewKind::Col
        | ViewKind::Row
        | ViewKind::Pad
        | ViewKind::Grid
        | ViewKind::Scroll
        | ViewKind::List
        | ViewKind::Stack
        | ViewKind::Spacer => {}
    }

    // 子：头兄起流序（painter 序：后画者后访问 = 覆盖在前画者上）。
    // 收起态 Collapse：子不发射（主路径 kids_live 同守卫）。
    // 波 2 移植件 XA：clip 段直接包夹（前序正确序；空窗跳整段）。
    let clip_open = clip_here && !child_eff.is_empty();
    if clip_open {
        if enc.push_clip(GfxRect::new(child_eff.x, child_eff.y, child_eff.w, child_eff.h)).is_ok() {
            rep.cmds += 1;
        }
    }
    if !clip_here || !child_eff.is_empty() {
        let kids_live = !matches!(&n.props, Props::Collapse(cp) if !cp.expanded);
        let mut c = if kids_live { n.first_child } else { NONE };
        let mut guard = 0usize;
        while c != NONE {
            guard += 1;
            if guard > nodes.len() {
                return; // 子链环防御
            }
            match nodes.get(c as usize) {
                Some(Some(ch)) if ch.alive => {
                    emit_walk_all(nodes, c, bx, by, &child_eff, vp_rect, icons, pressed, focus_slot, texts, tokens, rest, enc, rep, hops + 1);
                    c = ch.next_sib;
                }
                _ => return, // 链上死槽 = 树结构异常，防御放弃余段
            }
        }
    }
    if clip_open {
        if enc.pop_clip().is_ok() {
            rep.cmds += 1;
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
    pressed: &PressedVis,
    focus_slot: u32,
    texts: &InputTexts,
    tokens: &crate::theme::Tokens,
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
            // [paint-exempt] 黑色哨兵实参：Image 像素即色（ARGB 经 D28
            // icons 槽），fg 形参对此臂是死实参非视觉色（豁免清单 #1）。
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
                    // 波 2 移植件 XA 三态查表：disabled > pressed > normal
                    // （禁用锁死优先——禁用件不表达按压反馈；色值 theme
                    // 单源，tokens.w.accent_pressed 先例同律）。
                    let (bg, fg) = match pressed.state(slot) {
                        NodeVisState::Disabled => {
                            (tokens.bg_disabled, tokens.fg_disabled)
                        }
                        NodeVisState::Pressed => (tokens.w.accent_pressed, tokens.w.on_accent),
                        NodeVisState::Normal => (tokens.w.accent, tokens.w.on_accent),
                    };
                    fill(enc, b.x, b.y, b.w, b.h, bg, rep);
                    emit_widget_text(p.label.as_bytes(), b.x, b.w, b.y, b.h, fg, rest, enc, rep);
                }
                ButtonStyle::Outlined => {
                    // 描边 = 四 fill_rect 组合（v1 无 stroke 原语——「已有
                    // 原语组合」纪律）；底不填（Color 无 alpha——Outlined
                    // 语义 = 透应用底，实心 surface 会盖住背景）。三态查
                    // 表（Filled 同律 disabled > pressed > normal）：
                    // disabled 描边/标签换禁用灰；pressed 描边换 tokens.w.accent
                    // 暗档（无底面可暗化——描边即反馈面）。
                    let (bc, fg) = match pressed.state(slot) {
                        NodeVisState::Disabled => {
                            (tokens.fg_disabled, tokens.fg_disabled)
                        }
                        NodeVisState::Pressed => (tokens.w.accent_pressed, tokens.w.on_surface),
                        NodeVisState::Normal => (tokens.w.outline, tokens.w.on_surface),
                    };
                    let t = BUTTON_BORDER.min(b.w / 2).min(b.h / 2).max(1);
                    fill(enc, b.x, b.y, b.w, t, bc, rep);
                    fill(enc, b.x, b.y + b.h - t, b.w, t, bc, rep);
                    fill(enc, b.x, b.y + t, t, b.h - 2 * t, bc, rep);
                    fill(enc, b.x + b.w - t, b.y + t, t, b.h - 2 * t, bc, rep);
                    emit_widget_text(p.label.as_bytes(), b.x, b.w, b.y, b.h, fg, rest, enc, rep);
                }
                ButtonStyle::Text => {
                    // 三态查表同律：disabled 标签换禁用灰；pressed 标签换
                    // tokens.w.accent 暗档（无底无描边——标签即反馈面）。
                    let fg = match pressed.state(slot) {
                        NodeVisState::Disabled => tokens.fg_disabled,
                        NodeVisState::Pressed => tokens.w.accent_pressed,
                        NodeVisState::Normal => tokens.w.on_surface,
                    };
                    emit_widget_text(p.label.as_bytes(), b.x, b.w, b.y, b.h, fg, rest, enc, rep);
                }
            }
        }
        Props::Toggle(p) => {
            // 波 2 移植件 XA：disabled 档轨道换禁用灰（三态查表——
            // 禁用优先于 checked 表色）；真圆角档轨走 FillRectRounded
            // (r=h/2) 真胶囊（raster 侧钳 min(w,h)/2 同义），v1 capsule_h
            // 三 fill 近似为默认档（既有 golden 钉板零漂移）。滑块圆
            // 保持 oct3 近似（弧族原语面 = 波 3+，非本原语射程）。
            let track = if pressed.is_disabled(slot) {
                tokens.bg_disabled
            } else if p.checked {
                tokens.w.accent
            } else {
                tokens.w.track_off
            };
            if true_corners() {
                fill_rounded(enc, b.x, b.y, b.w, b.h, (b.h.max(0) as u32) / 2, track, rep);
            } else {
                capsule_h(enc, b.x, b.y, b.w, b.h, track, rep);
            }
            let d = (b.h - 2 * TOGGLE_KNOB_MARGIN).max(0); // 滑块边长（32 高 → 24）
            let kx = if p.checked {
                b.x + b.w - TOGGLE_KNOB_MARGIN - d
            } else {
                b.x + TOGGLE_KNOB_MARGIN
            };
            oct3(enc, kx, b.y + TOGGLE_KNOB_MARGIN, d, tokens.w.knob, rep);
        }
        Props::Slider(p) => {
            let yc = b.y + b.h / 2;
            let ty = yc - SLIDER_TRACK_H / 2;
            fill(enc, b.x, ty, b.w, SLIDER_TRACK_H, tokens.w.track_dim, rep);
            // 拖动圆行程 = [x+inset, x+w−inset]（inset = 圆半径——与
            // lib.rs slider_permille_at 命中映射同式单源
            // crate::SLIDER_KNOB_D/2）；填充至圆心。
            let inset = (crate::SLIDER_KNOB_D / 2) as i32;
            let usable = (b.w as i64 - 2 * inset as i64).max(1);
            let frac = p.value.min(crate::PERMILLE_MAX) as i64 * usable
                / crate::PERMILLE_MAX as i64;
            let thumb_cx = b.x as i64 + inset as i64 + frac;
            let fill_w = i64_to_i32_sat(thumb_cx - b.x as i64);
            fill(enc, b.x, ty, fill_w, SLIDER_TRACK_H, tokens.w.accent, rep);
            let kd = crate::SLIDER_KNOB_D as i32;
            oct3(
                enc,
                i64_to_i32_sat(thumb_cx) - kd / 2,
                yc - kd / 2,
                kd,
                tokens.w.knob,
                rep,
            );
        }
        Props::Progress(p) => {
            fill(enc, b.x, b.y, b.w, b.h, tokens.w.track_dim, rep);
            let fw = p.value.min(crate::PERMILLE_MAX) as i64 * b.w as i64
                / crate::PERMILLE_MAX as i64;
            fill(enc, b.x, b.y, i64_to_i32_sat(fw), b.h, tokens.w.accent, rep);
        }
        // Check：方框描边（Outlined 按钮同形四 fill_rect）+ 选中态双
        // fill_rect 勾（短臂下角块 + 长臂上行条——24×24 定格比例，内
        // 在恒 24 时无缩放面）。波 2 移植件 XA：disabled 档框/勾换禁用
        // 弱化灰（theme 单源）。
        Props::Check(p) => {
            let disabled = pressed.is_disabled(slot);
            let bc = if disabled {
                tokens.fg_disabled
            } else if p.checked {
                tokens.w.accent
            } else {
                tokens.w.outline
            };
            let t = CHECK_BORDER.min(b.w / 2).min(b.h / 2).max(1);
            fill(enc, b.x, b.y, b.w, t, bc, rep);
            fill(enc, b.x, b.y + b.h - t, b.w, t, bc, rep);
            fill(enc, b.x, b.y + t, t, b.h - 2 * t, bc, rep);
            fill(enc, b.x + b.w - t, b.y + t, t, b.h - 2 * t, bc, rep);
            if p.checked {
                // 勾形两片：短臂（左下角块）+ 长臂（上行竖条），交叠
                // 成勾角。
                let mark = if disabled { tokens.fg_disabled } else { tokens.w.accent };
                fill(enc, b.x + 5, b.y + 12, 4, 4, mark, rep);
                fill(enc, b.x + 8, b.y + 6, 3, 9, mark, rep);
            }
        }
        // Radio：八片环近似（四直段 + 四角块——八角同族；选中态内圆
        // = oct3 半径减半，与 Toggle 滑块同核）。波 2 移植件 XA：
        // disabled 档环/内圆换禁用弱化灰。
        Props::Radio(p) => {
            let disabled = pressed.is_disabled(slot);
            let rc = if disabled {
                tokens.fg_disabled
            } else if p.selected {
                tokens.w.accent
            } else {
                tokens.w.outline
            };
            let d = b.w.min(b.h);
            let ox = b.x + (b.w - d) / 2;
            let oy = b.y + (b.h - d) / 2;
            ring8(enc, ox, oy, d, rc, rep);
            if p.selected {
                let ic = if disabled { tokens.fg_disabled } else { tokens.w.accent };
                let di = d / 2;
                let ix = b.x + (b.w - di) / 2;
                let iy = b.y + (b.h - di) / 2;
                oct3(enc, ix, iy, di, ic, rep);
            }
        }
        // Step：三段 = 底 + 两道分隔线 + 三组字形（左 '-'、中值（十
        // 进制 u32）、右 '+'——S8x16 同档，区带内双轴居中）。
        Props::Step(p) => {
            fill(enc, b.x, b.y, b.w, b.h, tokens.w.track_dim, rep);
            let z = STEP_ZONE_W.min(b.w / 2).max(0);
            fill(enc, b.x + z - STEP_DIV_W, b.y, STEP_DIV_W, b.h, tokens.w.outline, rep);
            fill(enc, b.x + b.w - z, b.y, STEP_DIV_W, b.h, tokens.w.outline, rep);
            emit_widget_text(b"-", b.x, z, b.y, b.h, tokens.w.on_surface, rest, enc, rep);
            let mut buf = [0u8; 10];
            let n = fmt_u32(p.value, &mut buf);
            emit_widget_text(&buf[..n], b.x + z, b.w - 2 * z, b.y, b.h, tokens.w.on_surface, rest, enc, rep);
            emit_widget_text(b"+", b.x + b.w - z, z, b.y, b.h, tokens.w.on_surface, rest, enc, rep);
        }
        // Seg：等分底 + active 段 tokens.w.accent 高亮 + 段间分隔线（active 后
        // 发——高亮段两界分隔线恒可见）。波 2 移植件 XA：disabled 档
        // active 段换禁用底灰（ tokens.w.track_dim 底上 washed 观感）。
        Props::Seg(p) => {
            fill(enc, b.x, b.y, b.w, b.h, tokens.w.track_dim, rep);
            let cnt = p.segment_count.max(1) as i64;
            let sw = b.w as i64 / cnt;
            if sw > 0 {
                // 末段宽吃整数余数（b.w − sw×(cnt−1)）——整除时与 sw 同
                // 值零漂移；非整除时末段补齐右缘（原先恒 sw 会在末段
                // 右留 1..cnt−1 px 缺口）。
                let act = p.active.min(p.segment_count) as i64;
                let aw = if act == cnt - 1 {
                    b.w as i64 - sw * (cnt - 1)
                } else {
                    sw
                };
                let ax = b.x as i64 + sw * act;
                let active_c = if pressed.is_disabled(slot) {
                    tokens.bg_disabled
                } else {
                    tokens.w.accent
                };
                fill(
                    enc,
                    i64_to_i32_sat(ax),
                    b.y,
                    i64_to_i32_sat(aw),
                    b.h,
                    active_c,
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
                        tokens.w.outline,
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
            fill(enc, x, b.y, w, b.h, tokens.w.outline, rep);
        }
        // 波2 批 A（Tabs/Sbar/Badge）：Tabs = 页签条底 + active 段指示
        // 条 + 段间分隔线（视觉 = 自身条区，内容区子各自发射）；Sbar =
        // 细轨 + thumb 区间（permille 两端）；Badge = 红底数字胶囊（≤99
        // 直显两位档，>99 显 99+）或纯圆点（show_dot）。
        Props::Tabs(p) => {
            fill(enc, b.x, b.y, b.w, b.h, tokens.w.tabs_bg, rep);
            // 段宽 = w/tab_count（下取整）；active 段底缘指示条。末段宽
            // 吃整数余数（Seg 同式——b.w − seg_w×(n−1)，整除零漂移、非
            // 整除补齐右缘）。
            let n_tabs = p.tab_count.max(1) as i64;
            let seg_w = b.w as i64 / n_tabs;
            let act = p.active.min(p.tab_count.saturating_sub(1)) as i64;
            let ind_w = if act == n_tabs - 1 {
                b.w as i64 - seg_w * (n_tabs - 1)
            } else {
                seg_w
            };
            let ax = b.x as i64 + seg_w * act;
            fill(
                enc,
                i64_to_i32_sat(ax),
                b.y + b.h - TABS_INDICATOR_H,
                i64_to_i32_sat(ind_w),
                TABS_INDICATOR_H,
                tokens.w.tabs_indicator,
                rep,
            );
            // 段间分隔线（tab_count−1 条竖线，tokens.w.outline）。
            for k in 1..n_tabs {
                let dx = b.x as i64 + seg_w * k;
                fill(
                    enc,
                    i64_to_i32_sat(dx),
                    b.y,
                    SEG_DIV_W,
                    b.h,
                    tokens.w.outline,
                    rep,
                );
            }
        }
        Props::Sbar(p) => {
            // 细轨全长 + thumb 区间（permille 两端）：Vertical → 竖轨
            //（thumb 纵向区间）；Horizontal → 横轨（镜像）。
            let st = p.thumb_start.min(crate::PERMILLE_MAX) as i64;
            let en = p.thumb_end.min(crate::PERMILLE_MAX) as i64;
            let span = en.saturating_sub(st);
            match p.orientation {
                Orientation::Vertical => {
                    fill(enc, b.x, b.y, b.w, b.h, tokens.w.sbar_track, rep);
                    let ty = b.y as i64 + b.h as i64 * st / crate::PERMILLE_MAX as i64;
                    let th = b.h as i64 * span / crate::PERMILLE_MAX as i64;
                    fill(
                        enc,
                        b.x,
                        i64_to_i32_sat(ty),
                        b.w.max(SBAR_MIN_THICK),
                        i64_to_i32_sat(th),
                        tokens.w.sbar_thumb,
                        rep,
                    );
                }
                Orientation::Horizontal => {
                    fill(enc, b.x, b.y, b.w, b.h, tokens.w.sbar_track, rep);
                    let tx = b.x as i64 + b.w as i64 * st / crate::PERMILLE_MAX as i64;
                    let tw = b.w as i64 * span / crate::PERMILLE_MAX as i64;
                    fill(
                        enc,
                        i64_to_i32_sat(tx),
                        b.y,
                        i64_to_i32_sat(tw),
                        b.h.max(SBAR_MIN_THICK),
                        tokens.w.sbar_thumb,
                        rep,
                    );
                }
            }
        }
        Props::Badge(p) => {
            // 圆点模式：八角小圆点（红）；count=0 隐藏（内在 0×0 → b
            // 空绘自然短路）。
            if p.show_dot {
                oct3(enc, b.x, b.y, b.w, tokens.w.badge_bg, rep);
            } else if p.count > 0 {
                // 数字角标：红底 + 白字（>99 显 99+ 三字节封顶档；≤99
                // 走 fmt_u32 十进制——Step 值字形同核）。
                fill(enc, b.x, b.y, b.w, b.h, tokens.w.badge_bg, rep);
                let mut buf = [0u8; 10];
                let n = if p.count > 99 {
                    buf[..3].copy_from_slice(b"99+");
                    3
                } else {
                    fmt_u32(p.count, &mut buf)
                };
                emit_widget_text(&buf[..n], b.x, b.w, b.y, b.h, tokens.w.on_badge, rest, enc, rep);
            }
        }
        // 组件第三批五件：Tip = 实色衬底（v1 无 alpha 的半透明近似）+
        // 文本；Collapse = header 带 + 箭头指示（展开朝下 / 收起朝右
        //——三带三角形逼近；子先画盖其上，收起态子被走查守卫跳过）；
        // Chip = 胶囊底 + 居中标签 + 选中态色变——波 2 移植件 XA：真
        // 圆角档底走 FillRectRounded(r=OH_RADIUS_PX 8vp @3x；raster 侧
        // 钳 min(w,h)/2，32 高自然折胶囊)，v1 直角 fill 为默认档（既有
        // golden 钉板零漂移）；Fab = 八角圆形 tokens.w.accent 底 + 图标位
        //（v1 文本 '+' 占位——icon 资源面挂 D28 接线刀）+ 扩展标签带；
        // Card = 双 fill_rect 阴影（整界阴影色 + 面底右下内缩 off =
        // elevation×2px——视觉足迹恒在界内，damage 正确性优先）——
        // 波 2 移植件 XA：真圆角档消费 p.corner_radius（TLV 数据已在
        // 流里跑）——阴影条与卡面同半径圆角（角区阴影随面收口，足迹
        // 仍在界内），v1 直角为默认档。
        Props::Tip(p) => {
            fill(enc, b.x, b.y, b.w, b.h, tokens.w.tip_bg, rep);
            emit_widget_text(p.text.as_bytes(), b.x, b.w, b.y, b.h, tokens.w.on_accent, rest, enc, rep);
        }
        Props::Collapse(p) => {
            let hh = crate::damage::u32_to_i32_sat(crate::COLLAPSE_HEADER_H)
                .min(b.h)
                .max(0);
            fill(enc, b.x, b.y, b.w, hh, tokens.w.track_dim, rep);
            let a = COLLAPSE_ARROW.min(hh); // 极矮界防御：箭头随 header 高收缩
            if a > 0 {
                let t = a / COLLAPSE_ARROW_BANDS; // 带厚（12 → 4）
                let s = a / (2 * COLLAPSE_ARROW_BANDS); // 每带内缩步（12 → 2）
                let ax = b.x + COLLAPSE_ARROW_X;
                let ay = b.y + (hh - a) / 2;
                let mut i = 0;
                while i < COLLAPSE_ARROW_BANDS {
                    if p.expanded {
                        // 朝下：三横带顶宽底窄
                        fill(enc, ax + s * i, ay + t * i, a - 2 * s * i, t, tokens.w.on_surface, rep);
                    } else {
                        // 朝右：三竖带左高右矮
                        fill(enc, ax + t * i, ay + s * i, t, a - 2 * s * i, tokens.w.on_surface, rep);
                    }
                    i += 1;
                }
            }
        }
        Props::Chip(p) => {
            let (bgc, fg) = if p.selected {
                (tokens.w.accent, tokens.w.on_accent)
            } else {
                (tokens.w.track_dim, tokens.w.on_surface)
            };
            fill_rounded(enc, b.x, b.y, b.w, b.h, crate::theme::OH_RADIUS_PX, bgc, rep);
            emit_widget_text(p.label.as_bytes(), b.x, b.w, b.y, b.h, fg, rest, enc, rep);
        }
        Props::Fab(p) => {
            let d = crate::damage::u32_to_i32_sat(crate::FAB_D)
                .min(b.w)
                .min(b.h)
                .max(0);
            oct3(enc, b.x, b.y, d, tokens.w.accent, rep);
            if d > 0 {
                emit_widget_text(b"+", b.x, d, b.y, b.h, tokens.w.on_accent, rest, enc, rep);
            }
            if !p.extended_label.is_empty() {
                let gap = crate::damage::u32_to_i32_sat(crate::FAB_LABEL_GAP);
                let zx = b.x.saturating_add(d).saturating_add(gap);
                let zw = b.w.saturating_sub(d).saturating_sub(gap);
                emit_widget_text(
                    p.extended_label.as_bytes(),
                    zx,
                    zw,
                    b.y,
                    b.h,
                    tokens.w.on_accent,
                    rest,
                    enc,
                    rep,
                );
            }
        }
        Props::Card(p) => {
            let off = crate::damage::u32_to_i32_sat(
                p.elevation.saturating_mul(CARD_SHADOW_STEP as u32),
            );
            if off > 0 {
                fill_rounded(enc, b.x, b.y, b.w, b.h, p.corner_radius, tokens.w.card_shadow, rep);
                fill_rounded(enc, b.x, b.y, b.w - off, b.h - off, p.corner_radius, tokens.w.card_bg, rep);
            } else {
                fill_rounded(enc, b.x, b.y, b.w, b.h, p.corner_radius, tokens.w.card_bg, rep);
            }
        }
        // 波2 批 C（Select）：下拉选择器 = 白底框（描边四边组合——
        // Outlined 按钮同形）+ 右侧下拉箭头 v 形 + 选中索引十进制数字
        //（选项文本不进数据层——Seg 同律计数面；数字为索引值级可对拍）。
        Props::Select(p) => {
            let t = BUTTON_BORDER.min(b.w / 2).min(b.h / 2).max(1);
            fill(enc, b.x, b.y, b.w, t, tokens.w.select_border, rep);
            fill(enc, b.x, b.y + b.h - t, b.w, t, tokens.w.select_border, rep);
            fill(enc, b.x, b.y + t, t, b.h - 2 * t, tokens.w.select_border, rep);
            fill(enc, b.x + b.w - t, b.y + t, t, b.h - 2 * t, tokens.w.select_border, rep);
            if b.w > 2 * t && b.h > 2 * t {
                fill(enc, b.x + t, b.y + t, b.w - 2 * t, b.h - 2 * t, tokens.w.select_bg, rep);
            }
            // 箭头 v：右端 16px 区带内三行递缩（fill 组合近似）。
            let ax = b.x + b.w - 16;
            let cy = b.y + b.h / 2;
            fill(enc, ax, cy - 4, 8, 2, tokens.w.select_fg, rep);
            fill(enc, ax + 2, cy - 2, 4, 2, tokens.w.select_fg, rep);
            fill(enc, ax + 3, cy, 2, 2, tokens.w.select_fg, rep);
            // 选中索引数字（1 起——人类序；0 = 空选择器/未选）。
            if p.selected > 0 || p.option_count > 0 {
                let idx1 = p.selected.saturating_add(1); // 1 起显示
                let mut buf = [b'0'; 3];
                let n = if idx1 > 99 {
                    buf.copy_from_slice(b"99+");
                    3
                } else if idx1 >= 10 {
                    buf[0] = b'0' + (idx1 / 10) as u8;
                    buf[1] = b'0' + (idx1 % 10) as u8;
                    2
                } else {
                    buf[0] = b'0' + idx1 as u8;
                    1
                };
                emit_widget_text(&buf[..n], b.x + 4, b.w - 20, b.y, b.h, tokens.w.select_fg, rest, enc, rep);
            }
        }
        // 波2 批 D（Input/Edit/Canvas）：Input = 白底框（焦点 = tokens.w.accent
        // 环）/内容或 placeholder/caret（有焦且内容未满时尾置 2×16 竖
        // 线——内容经 lib 侧表传入无从查，v1 caret 恒随内容尾近似 =
        // max(8, len×8) 位；Edit = 多行版（Canvas 同底框无 caret）；
        // Canvas = 浅底占位面（自绘协议挂后续刀——paint_ops 计数面）。
        Props::Input(p) => {
            let ring = if focus_slot == slot { tokens.w.input_focus_ring } else { tokens.w.input_border };
            let t = BUTTON_BORDER.min(b.w / 2).min(b.h / 2).max(1);
            fill(enc, b.x, b.y, b.w, t, ring, rep);
            fill(enc, b.x, b.y + b.h - t, b.w, t, ring, rep);
            fill(enc, b.x, b.y + t, t, b.h - 2 * t, ring, rep);
            fill(enc, b.x + b.w - t, b.y + t, t, b.h - 2 * t, ring, rep);
            if b.w > 2 * t && b.h > 2 * t {
                fill(enc, b.x + t, b.y + t, b.w - 2 * t, b.h - 2 * t, tokens.w.input_bg, rep);
            }
            let focused = focus_slot == slot;
            let text: &[u8] = texts.text_of(slot);
            if text.is_empty() {
                if !p.placeholder.is_empty() {
                    emit_widget_text(
                        p.placeholder.as_bytes(),
                        b.x + 6, b.w - 12, b.y, b.h, tokens.w.input_placeholder, rest, enc, rep,
                    );
                }
            } else {
                let fg = match p.input_kind {
                    crate::desc::InputKind::Password => b'*',
                    _ => 0,
                };
                if fg == 0 {
                    emit_widget_text(text, b.x + 6, b.w - 12, b.y, b.h, tokens.w.select_fg, rest, enc, rep);
                } else {
                    let mask = [fg; 32];
                    let n = text.len().min(32);
                    emit_widget_text(&mask[..n], b.x + 6, b.w - 12, b.y, b.h, tokens.w.select_fg, rest, enc, rep);
                }
            }
            if focused {
                let cx = b.x + 6 + (text.len() as i32 * 8).min(b.w - 16);
                // 与文本居中同式（emit_widget_text：(h−16)/2——48 高即
                // +16；原硬编 +14 对文本偏 2px）。
                let cy = b.y + (b.h - 16).max(0) / 2;
                fill(enc, cx, cy, INPUT_CARET_W, 16, tokens.w.input_focus_ring, rep);
            }
        }
        Props::Edit(p) => {
            let ring = if focus_slot == slot { tokens.w.input_focus_ring } else { tokens.w.input_border };
            let t = BUTTON_BORDER.min(b.w / 2).min(b.h / 2).max(1);
            fill(enc, b.x, b.y, b.w, t, ring, rep);
            fill(enc, b.x, b.y + b.h - t, b.w, t, ring, rep);
            fill(enc, b.x, b.y + t, t, b.h - 2 * t, ring, rep);
            fill(enc, b.x + b.w - t, b.y + t, t, b.h - 2 * t, ring, rep);
            if b.w > 2 * t && b.h > 2 * t {
                fill(enc, b.x + t, b.y + t, b.w - 2 * t, b.h - 2 * t, tokens.w.input_bg, rep);
            }
            let etext = texts.text_of(slot);
            if etext.is_empty() && !p.placeholder.is_empty() {
                emit_widget_text(
                    p.placeholder.as_bytes(),
                    b.x + 6, b.w - 12, b.y, 40, tokens.w.input_placeholder, rest, enc, rep,
                );
            } else if !etext.is_empty() {
                emit_widget_text(etext, b.x + 6, b.w - 12, b.y, 40, tokens.w.select_fg, rest, enc, rep);
            }
            if focus_slot == slot {
                // 首行 caret 与文本带居中同式（文本发射带 h=40 → (40−16)/2
                // = +12；原 +2/+8 组合对文本偏 2px）；次行步进 20 保持。
                let cy = b.y + (40 - 16) / 2 + ((etext.len() / 24) as i32).min(1) * 20;
                let cx = b.x + 6 + ((etext.len() % 24) as i32 * 8).min(b.w - 16);
                fill(enc, cx, cy, INPUT_CARET_W, 16, tokens.w.input_focus_ring, rep);
            }
        }
        Props::Canvas(_) => {
            fill(enc, b.x, b.y, b.w, b.h, tokens.w.canvas_bg, rep);
        }
        // 波2 批 E（Refresh/SwipeItem/DatePick/TimePick）：Refresh = 包裹
        // 容器 + 顶缘拉动指示条（offset permille → 帽 32px 高比例；子仍
        // 各自发射）；SwipeItem = 行项 + 两端动作缘带（reveal permille →
        // 24px 帽宽比例）；DatePick/TimePick = 白底框 + 数字显示（YYYY-
        // MM-DD / HH:MM——ASCII；滚轮交互挂波3/波4 消费面）。
        Props::Refresh(p) => {
            let h = (b.h.min(32) as i64 * p.offset.min(crate::PERMILLE_MAX) as i64
                / crate::PERMILLE_MAX as i64) as i32;
            if h > 0 {
                fill(enc, b.x, b.y, b.w, h, tokens.w.refresh_bar, rep);
            }
        }
        Props::SwipeItem(p) => {
            let ws = (b.w.min(96) as i64 * p.reveal_start.min(crate::PERMILLE_MAX) as i64
                / crate::PERMILLE_MAX as i64) as i32;
            let we = (b.w.min(96) as i64 * p.reveal_end.min(crate::PERMILLE_MAX) as i64
                / crate::PERMILLE_MAX as i64) as i32;
            if ws > 0 {
                fill(enc, b.x, b.y, ws, b.h, tokens.w.swipe_action_bg, rep);
            }
            if we > 0 {
                fill(enc, b.x + b.w - we, b.y, we, b.h, tokens.w.swipe_action_bg, rep);
            }
        }
        Props::DatePick(p) => {
            let t = BUTTON_BORDER.max(1);
            fill(enc, b.x, b.y, b.w, t, tokens.w.pick_border, rep);
            fill(enc, b.x, b.y + b.h - t, b.w, t, tokens.w.pick_border, rep);
            fill(enc, b.x, b.y + t, t, b.h - 2 * t, tokens.w.pick_border, rep);
            fill(enc, b.x + b.w - t, b.y + t, t, b.h - 2 * t, tokens.w.pick_border, rep);
            if b.w > 2 * t && b.h > 2 * t {
                fill(enc, b.x + t, b.y + t, b.w - 2 * t, b.h - 2 * t, tokens.w.pick_bg, rep);
            }
            // YYYY-MM-DD 十字节定长（u32 四位 + 月日两位零垫）。
            let mut buf = [b'-'; 10];
            let y = p.year;
            buf[0] = b'0' + (y / 1000) as u8;
            buf[1] = b'0' + (y / 100 % 10) as u8;
            buf[2] = b'0' + (y / 10 % 10) as u8;
            buf[3] = b'0' + (y % 10) as u8;
            buf[5] = b'0' + (p.month / 10) as u8;
            buf[6] = b'0' + (p.month % 10) as u8;
            buf[8] = b'0' + (p.day / 10) as u8;
            buf[9] = b'0' + (p.day % 10) as u8;
            emit_widget_text(&buf, b.x, b.w, b.y, b.h, tokens.w.select_fg, rest, enc, rep);
        }
        Props::TimePick(p) => {
            let t = BUTTON_BORDER.max(1);
            fill(enc, b.x, b.y, b.w, t, tokens.w.pick_border, rep);
            fill(enc, b.x, b.y + b.h - t, b.w, t, tokens.w.pick_border, rep);
            fill(enc, b.x, b.y + t, t, b.h - 2 * t, tokens.w.pick_border, rep);
            fill(enc, b.x + b.w - t, b.y + t, t, b.h - 2 * t, tokens.w.pick_border, rep);
            if b.w > 2 * t && b.h > 2 * t {
                fill(enc, b.x + t, b.y + t, b.w - 2 * t, b.h - 2 * t, tokens.w.pick_bg, rep);
            }
            // HH:MM 五字节（24 小时制原值；12 折算消费面）。
            let mut buf = [b':'; 5];
            buf[0] = b'0' + (p.hour / 10) as u8;
            buf[1] = b'0' + (p.hour % 10) as u8;
            buf[3] = b'0' + (p.minute / 10) as u8;
            buf[4] = b'0' + (p.minute % 10) as u8;
            emit_widget_text(&buf, b.x, b.w, b.y, b.h, tokens.w.select_fg, rest, enc, rep);
        }
        // 批 F（末刀三件）：
        // - Gauge = v1 简化粗水平条（原设计环形仪表 ring8 底环 + 部分
        //   弧——弧原语 v1 缺席，与 Progress 同律统一条形；style Ring/
        //   Bar/Arc 数据携载、v1 同渲染）：上带 tokens.w.track_off 底轨 + tokens.w.accent
        //   填充（×value/1000），下带值文本（permille 折百分整数 + '%'
        //   ——免 FP；750‰ → "75%"）。
        // - Qr = v1 视觉占位：三角落定位方块（TL/TR/BL 各 7×7 模块的
        //   方框套方框——外环暗/中环亮/3×3 暗心）+ 数据区确定性伪随机
        //   图案（module_count 作种子整数散列——同帧同图可对拍；真 QR
        //   编码挂后续刀）。全底亮色 1 fill + 暗模块逐格 4×4 fill；大
        //   版本 cmds 满帽（MAX_CMDS=256）停录不炸——文档化降级。
        // - DragReorder = 三条水平短杠（≡ 手柄）：三 fill_rect（12×2
        //   短杠、杠厚即间距——24×36 定格内 y+13/17/21）。
        Props::Gauge(p) => {
            let bar_h = GAUGE_BAR_H.min(b.h / 2).max(0); // 极矮界防御：条带不过半高
            if bar_h > 0 {
                let ty = b.y + (b.h / 2 - bar_h) / 2; // 上带内居中（64 高 → y+8）
                fill(enc, b.x, ty, b.w, bar_h, tokens.w.track_off, rep);
                let fw = p.value.min(crate::PERMILLE_MAX) as i64 * b.w as i64
                    / crate::PERMILLE_MAX as i64;
                fill(enc, b.x, ty, i64_to_i32_sat(fw), bar_h, tokens.w.accent, rep);
            }
            // 值文本：下带（节点下半区）双轴居中——value/10 折百分。
            let mut buf = [0u8; 10];
            let n = fmt_u32(p.value.min(crate::PERMILLE_MAX) / 10, &mut buf);
            buf[n] = b'%';
            emit_widget_text(
                &buf[..n + 1],
                b.x,
                b.w,
                b.y + b.h / 2,
                b.h - b.h / 2,
                tokens.w.on_surface,
                rest,
                enc,
                rep,
            );
        }
        Props::Qr(p) => {
            let light = crate::unpack_color(p.light_color);
            let dark = crate::unpack_color(p.dark_color);
            fill(enc, b.x, b.y, b.w, b.h, light, rep);
            let px = crate::QR_MODULE_PX as i32;
            let m = b.w.min(b.h) / px; // 节点界折模块数（内在型叶恒 = module_count）
            let mut r = 0i32;
            while r < m {
                let mut c = 0i32;
                while c < m {
                    if qr_module_dark(m, r, c) {
                        fill(enc, b.x + c * px, b.y + r * px, px, px, dark, rep);
                    }
                    c += 1;
                }
                r += 1;
            }
        }
        Props::DragReorder(_) => {
            let bw = DRAG_BAR_W.min(b.w).max(0);
            // 杠厚即间隙：3 杠 + 2 隙 = 5×bh 不越界（36 高 → bh=2、y+13 起）。
            let bh = DRAG_BAR_H.min(b.h / 5).max(0);
            if bw > 0 && bh > 0 {
                let x = b.x + (b.w - bw) / 2;
                let y0 = b.y + (b.h - 5 * bh) / 2;
                let mut i = 0;
                while i < 3 {
                    fill(enc, x, y0 + 2 * bh * i, bw, bh, tokens.w.on_surface, rep);
                    i += 1;
                }
            }
        }
        // Root/Col/Row/Pad/Grid + 余容器：无视觉内容（子树各自发射）
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

/// 圆角填充收口（波 2 移植件 XA——真圆角档消费面）：开档且 radius > 0
/// → `fill_rect_rounded`（raster 行 LUT；半径钳 min(w,h)/2 在 raster 侧
/// 单源）；否则回落 [`fill`] 同义直角（默认档零漂移）。零面积跳过 +
/// cmds 计数同律。
fn fill_rounded(
    enc: &mut CommandEncoder,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius_px: u32,
    c: Color,
    rep: &mut PaintReport,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let ok = if true_corners() && radius_px > 0 {
        enc.fill_rect_rounded(GfxRect::new(x, y, w, h), radius_px, c)
    } else {
        enc.fill_rect(GfxRect::new(x, y, w, h), c)
    };
    if ok.is_ok() {
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

/// QR 定位方块域判定（v1 占位图案——TL/TR/BL 三角落各 7×7 模块）：
/// 返回 Some(方块局部坐标)（原点 = 方块左上）；None = 数据区。域面
/// 不足（m < 7）无方块——值域执法保证 m ≥ 21，此为防御面。
fn qr_finder_local(m: i32, r: i32, c: i32) -> Option<(i32, i32)> {
    const F: i32 = 7;
    if m < F {
        return None;
    }
    let off = m - F;
    if r < F && c < F {
        Some((r, c)) // TL
    } else if r < F && c >= off {
        Some((r, c - off)) // TR
    } else if r >= off && c < F {
        Some((r - off, c)) // BL
    } else {
        None
    }
}

/// QR 模块明暗裁决（v1 确定性占位——真 QR 编码接入后整体退役）：
/// 定位方块内 = 方框套方框（切比雪夫半径 3 外环暗 / 2 中环亮 / ≤1
/// 3×3 暗心）；数据区 = module_count 作种子的整数散列（wrapping 乘
/// + 位移搅打——免 FP/免分配，同模块跨帧恒等可 golden 对拍），< 40%
/// 判暗。
fn qr_module_dark(m: i32, r: i32, c: i32) -> bool {
    if let Some((lr, lc)) = qr_finder_local(m, r, c) {
        let cheb = (lr - 3).abs().max((lc - 3).abs());
        cheb == 3 || cheb <= 1
    } else {
        let mut h = (m as u32).wrapping_mul(0x9E37_79B1)
            ^ (r as u32).wrapping_mul(0x85EB_CA6B)
            ^ (c as u32).wrapping_mul(0xC2B2_AE35);
        h ^= h >> 13;
        h = h.wrapping_mul(0x27D4_EB2F);
        h ^= h >> 16;
        h % 100 < 40
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

// ---------- 波 2 移植件 XA 单测（文件内联——不占 tests/ 面） ----------
//
// 覆盖三面：①真圆角档 Toggle/Chip/Card 发射与像素（含默认档零漂移锚）；
//②disabled 三态查表首批五 kind（Button/Toggle/Check/Radio/Seg）；③
// Scroll/List clip 溢出裁剪（eff 窗 + 裁剪栈回放）。树走查直建 Node 面
//（pub(crate) 字段全 pub——paint 入口免经 lib.rs 门面，disabled 写侧表
// 未落地前的机制级验收）。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desc::{
        ButtonProps, CardProps, CheckProps, ChipProps, ListProps, RadioProps, RectProps,
        ScrollProps, SegProps, TextBuf, ToggleProps,
    };
    use crate::metrics::EmptyIconSource;
    use berry_gfx::cmd::{Cmd, DisplayList};
    use berry_gfx::color::{TextureDesc, TextureFormat};
    use berry_gfx::pass::PassDescriptor;
    use berry_gfx::queue::Queue;

    /// 全局档位锁（TRUE_CORNERS 进程级开关——测线程串行化；档位还原
    /// 守卫 Drop 兜底 panic 安全）。
    static CORNER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// 档位还原守卫：构造时记住旧值，Drop 时还原（含 panic 展开路径）。
    struct CornerRestore(bool);
    impl Drop for CornerRestore {
        fn drop(&mut self) {
            set_true_corners(self.0);
        }
    }

    /// 命令摘要（生命周期解耦——`Cmd` 借用调用方 scratch，摘要有值化）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Sum {
        Fill(i32, i32, i32, i32, u32),
        Rounded(i32, i32, i32, i32, u32, u32),
        PushClip(i32, i32, i32, i32),
        PopClip,
        Other,
    }

    /// Color → Rgbx u32（回放面 pack 同式——断言与像素读数同域对拍）。
    fn packed(c: Color) -> u32 {
        (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32
    }

    fn summary(dl: &DisplayList<'_>) -> Vec<Sum> {
        dl.iter()
            .map(|c| match c {
                Cmd::FillRect { rect, paint } => {
                    Sum::Fill(rect.x, rect.y, rect.w, rect.h, packed(paint.color))
                }
                Cmd::FillRectRounded { rect, radius, paint } => {
                    Sum::Rounded(rect.x, rect.y, rect.w, rect.h, *radius, packed(paint.color))
                }
                Cmd::PushClip { rect } => Sum::PushClip(rect.x, rect.y, rect.w, rect.h),
                Cmd::PopClip => Sum::PopClip,
                _ => Sum::Other,
            })
            .collect()
    }

    /// 最小节点（paint 走查只读几何/亲缘/props 面——其余字段缺省）。
    fn mk_node(kind: ViewKind, props: Props, x: i32, y: i32, w: i32, h: i32) -> Node {
        Node {
            gen: 0,
            alive: true,
            kind,
            key: 0,
            props,
            parent: NONE,
            first_child: NONE,
            next_sib: NONE,
            prev_sib: NONE,
            x,
            y,
            w,
            h,
            meas_w: 0,
            meas_h: 0,
            layout_dirty: false,
            paint_dirty: false,
            ox: 0,
            oy: 0,
        }
    }

    /// root（槽 0）+ 子链（槽 1..，头兄起 next_sib 顺序——layout 产物模拟）。
    fn root_with_children(children: Vec<Node>) -> Vec<Option<Node>> {
        let mut root = mk_node(ViewKind::Root, Props::Root, 0, 0, 240, 300);
        root.first_child = if children.is_empty() { NONE } else { 1 };
        let mut nodes = vec![Some(root)];
        let n = children.len() as u32;
        for (i, mut c) in children.into_iter().enumerate() {
            let slot = 1 + i as u32;
            c.parent = 0;
            c.prev_sib = if slot == 1 { NONE } else { slot - 1 };
            c.next_sib = if slot == n { NONE } else { slot + 1 };
            nodes.push(Some(c));
        }
        nodes
    }

    /// 给 parent 挂头子（测试树单子容器——链首插入即可），返子槽。
    fn attach(nodes: &mut Vec<Option<Node>>, parent: u32, child: Node) -> u32 {
        let slot = nodes.len() as u32;
        let mut child = child;
        child.parent = parent;
        nodes[parent as usize].as_mut().unwrap().first_child = slot;
        nodes.push(Some(child));
        slot
    }

    const FB_STRIDE: usize = 240 * 4;

    /// FULL damage 走查 + soft 回放（Load 保 0x11 哨兵——未写像素可辨）。
    fn render(nodes: &[Option<Node>], pressed: &PressedVis) -> (Vec<Sum>, Vec<u8>, usize) {
        let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
        let mut enc = CommandEncoder::new();
        let icons = EmptyIconSource;
        let rep = paint(
            nodes,
            0,
            (240, 300),
            &DamageSet { full: true, ..DamageSet::new() },
            &mut scr,
            &mut enc,
            &icons,
            pressed,
            NONE,
            &InputTexts::empty(),
            // 内联测试面恒 light 档（旧固定色板行为的值级钉板——迁移后
            // light 档同值，本面即零漂移自证）。
            &crate::theme::Tokens::pick(crate::theme::Theme::Light),
        );
        let dl = enc.finish();
        let d = TextureDesc::new(240, 300, FB_STRIDE, TextureFormat::Rgbx).unwrap();
        let mut fb = vec![0x11u8; d.need_bytes()];
        Queue::new().submit(&dl, &mut fb, &d, &PassDescriptor::load()).unwrap();
        (summary(&dl), fb, rep.cmds)
    }

    fn px(fb: &[u8], x: usize, y: usize) -> u32 {
        let o = y * FB_STRIDE + x * 4;
        u32::from_le_bytes(fb[o..o + 4].try_into().unwrap())
    }

    /// 三件常备树：Toggle 52×32（unchecked）/ Chip 40×32（未选）/
    /// Card 30×20（elevation 2、corner_radius 8）。
    fn corner_scene() -> Vec<Option<Node>> {
        root_with_children(vec![
            mk_node(
                ViewKind::Toggle,
                Props::Toggle(ToggleProps { checked: false, on_change: 0 }),
                0,
                0,
                52,
                32,
            ),
            mk_node(
                ViewKind::Chip,
                Props::Chip(ChipProps {
                    label: TextBuf::new(b"T").unwrap(),
                    selected: false,
                    on_tap: 0,
                }),
                60,
                0,
                40,
                32,
            ),
            mk_node(
                ViewKind::Card,
                Props::Card(CardProps { elevation: 2, corner_radius: 8 }),
                0,
                50,
                30,
                20,
            ),
        ])
    }

    // —— ① 默认档零漂移锚：无 FillRectRounded 命令；Toggle 轨 =
    //    capsule_h 三 fill（widgets.rs golden 同形几何）；Chip/Card 直角 ——

    /// 测试面 light 档 token 组（旧固定色板行为的值级钉板——迁移后
    /// light 档同值，本面即零漂移自证）。
    fn tokens() -> crate::theme::Tokens {
        crate::theme::Tokens::pick(crate::theme::Theme::Light)
    }

    #[test]
    fn xa_legacy_default_no_rounded_cmds() {
        let _lock = CORNER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = CornerRestore(set_true_corners(false));
        let tokens = tokens();
        let (sum, _, _) = render(&corner_scene(), &PressedVis::empty());
        assert!(
            !sum.iter().any(|s| matches!(s, Sum::Rounded(..))),
            "默认档零 FillRectRounded：{sum:?}"
        );
        // Toggle 胶囊三 fill（cap=10、inset=8——52×32 手算）。
        assert!(sum.contains(&Sum::Fill(0, 8, 10, 16, packed(tokens.w.track_off))), "{sum:?}");
        assert!(sum.contains(&Sum::Fill(10, 0, 32, 32, packed(tokens.w.track_off))), "{sum:?}");
        assert!(sum.contains(&Sum::Fill(42, 8, 10, 16, packed(tokens.w.track_off))), "{sum:?}");
        // Chip 直角 / Card 直角双 fill（corner_radius 数据在默认档不消费）。
        assert!(sum.contains(&Sum::Fill(60, 0, 40, 32, packed(tokens.w.track_dim))), "{sum:?}");
        assert!(sum.contains(&Sum::Fill(0, 50, 30, 20, packed(tokens.w.card_shadow))), "{sum:?}");
        assert!(sum.contains(&Sum::Fill(0, 50, 26, 16, packed(tokens.w.card_bg))), "{sum:?}");
    }

    // —— ①' 真圆角档：Toggle r=h/2 真胶囊 / Chip r=8vp@3x / Card 消费
    //     corner_radius——命令面 + 像素面（角区不写、中段照铺）——
    #[test]
    fn xa_true_corners_rounded_toggle_chip_card() {
        let _lock = CORNER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = CornerRestore(set_true_corners(true));
        let tokens = tokens();
        let (sum, fb, _) = render(&corner_scene(), &PressedVis::empty());
        // 命令面：radius 原值入命令（Chip 24 由 raster 侧钳 min(w,h)/2）。
        assert!(
            sum.contains(&Sum::Rounded(0, 0, 52, 32, 16, packed(tokens.w.track_off))),
            "Toggle 真胶囊 r=h/2：{sum:?}"
        );
        assert!(
            sum.contains(&Sum::Rounded(60, 0, 40, 32, 24, packed(tokens.w.track_dim))),
            "Chip r=8vp@3x：{sum:?}"
        );
        assert!(sum.contains(&Sum::Rounded(0, 50, 30, 20, 8, packed(tokens.w.card_shadow))), "{sum:?}");
        assert!(sum.contains(&Sum::Rounded(0, 50, 26, 16, 8, packed(tokens.w.card_bg))), "{sum:?}");
        assert!(!sum.iter().any(|s| matches!(s, Sum::Fill(0, 8, 10, 16, _))), "capsule_h 退场");
        // 像素面：Toggle r=16 行 0 内缩 16 → 角 (0,0) 不写（哨兵）、
        // (26,0) 轨色；Chip 钳 16 同律；Card 角 sentinel、面心 tokens.w.card_bg、
        // 阴影下缘带（y=68 ∈ 阴影不 ∈ 面）。
        assert_eq!(px(&fb, 0, 0), 0x1111_1111, "Toggle 角区不写");
        assert_eq!(px(&fb, 26, 0), packed(tokens.w.track_off), "Toggle 顶行中段照铺");
        assert_eq!(px(&fb, 60, 0), 0x1111_1111, "Chip 角区不写");
        assert_eq!(px(&fb, 80, 16), packed(tokens.w.track_dim), "Chip 中心照铺");
        assert_eq!(px(&fb, 0, 50), 0x1111_1111, "Card 角区不写");
        assert_eq!(px(&fb, 13, 58), packed(tokens.w.card_bg), "Card 面心");
        assert_eq!(px(&fb, 10, 68), packed(tokens.w.card_shadow), "Card 阴影下缘带");
    }

    // —— ② disabled 三态首批五 kind + disabled 优先于 pressed ——
    #[test]
    fn xa_disabled_three_state_first_batch() {
        let _lock = CORNER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = CornerRestore(set_true_corners(false));
        let tokens = tokens();
        let nodes = root_with_children(vec![
            // slot1：Button disabled（BG_DISABLED 底 + FG_DISABLED 标签）。
            mk_node(
                ViewKind::Button,
                Props::Button(ButtonProps {
                    label: TextBuf::new(b"OK").unwrap(),
                    style: ButtonStyle::Filled,
                    on_tap: 0,
                }),
                0,
                0,
                64,
                48,
            ),
            // slot2：Toggle unchecked disabled（轨换 BG_DISABLED）。
            mk_node(
                ViewKind::Toggle,
                Props::Toggle(ToggleProps { checked: false, on_change: 0 }),
                0,
                48,
                52,
                32,
            ),
            // slot3：Check checked disabled（框/勾 FG_DISABLED）。
            mk_node(
                ViewKind::Check,
                Props::Check(CheckProps { checked: true, on_change: 0 }),
                0,
                80,
                24,
                24,
            ),
            // slot4：Radio selected disabled（环/内圆 FG_DISABLED）。
            mk_node(
                ViewKind::Radio,
                Props::Radio(RadioProps { selected: true, group: 0, on_change: 0 }),
                40,
                80,
                24,
                24,
            ),
            // slot5：Seg active=1 disabled（active 段 BG_DISABLED）。
            mk_node(
                ViewKind::Seg,
                Props::Seg(SegProps { segment_count: 3, active: 1, on_change: 0 }),
                0,
                104,
                90,
                32,
            ),
            // slot6：Button 仅 pressed（tokens.w.accent_pressed 对照——三态完备）。
            mk_node(
                ViewKind::Button,
                Props::Button(ButtonProps {
                    label: TextBuf::new(b"Go").unwrap(),
                    style: ButtonStyle::Filled,
                    on_tap: 0,
                }),
                0,
                136,
                64,
                48,
            ),
            // slot7：Button normal（tokens.w.accent 对照）。
            mk_node(
                ViewKind::Button,
                Props::Button(ButtonProps {
                    label: TextBuf::new(b"No").unwrap(),
                    style: ButtonStyle::Filled,
                    on_tap: 0,
                }),
                0,
                184,
                64,
                48,
            ),
        ]);
        let mut pv = PressedVis::empty();
        pv.slots[0] = 6; // pressed：slot6
        pv.n = 1;
        pv.disabled = [1, 2, 3, 4, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        pv.n_disabled = 5;
        // 查表机制值级：三态分派 + disabled 优先于 pressed。
        assert_eq!(pv.state(1), NodeVisState::Disabled);
        assert_eq!(pv.state(6), NodeVisState::Pressed);
        assert_eq!(pv.state(7), NodeVisState::Normal);
        let mut both = PressedVis::empty();
        both.slots[0] = 1;
        both.n = 1;
        both.disabled[0] = 1;
        both.n_disabled = 1;
        assert_eq!(both.state(1), NodeVisState::Disabled, "禁用锁死优先于按压");
        // 像素面：五 kind 禁用观感 + 两对照。
        let (_, fb, _) = render(&nodes, &pv);
        assert_eq!(px(&fb, 32, 24), packed(tokens.bg_disabled), "Button 禁用底");
        assert_eq!(px(&fb, 1, 64), packed(tokens.bg_disabled), "Toggle 禁用轨（左帽区——避开滑块）");
        assert_eq!(px(&fb, 0, 90), packed(tokens.fg_disabled), "Check 禁用框");
        assert_eq!(px(&fb, 46, 81), packed(tokens.fg_disabled), "Radio 禁用环");
        assert_eq!(px(&fb, 50, 92), packed(tokens.fg_disabled), "Radio 禁用内圆");
        assert_eq!(px(&fb, 45, 120), packed(tokens.bg_disabled), "Seg 禁用 active 段");
        assert_eq!(px(&fb, 5, 120), packed(tokens.w.track_dim), "Seg 底轨不变");
        assert_eq!(px(&fb, 32, 160), packed(tokens.w.accent_pressed), "pressed 对照暗化底");
        assert_eq!(px(&fb, 32, 208), packed(tokens.w.accent), "normal 对照主色底");
    }

    // —— ③ Scroll/List clip：溢出子裁到容器界（像素级）+ PushClip/PopClip
    //    段标记 + 段外妹妹不受影响（painter 序）——
    #[test]
    fn xa_scroll_list_clip_overflow() {
        // Scroll 视口 100×100 + 溢出子（上下皆越界 300 高）+ 界下妹妹。
        let mut nodes = root_with_children(vec![
            mk_node(
                ViewKind::Scroll,
                Props::Scroll(ScrollProps { content_height: 300, offset_y: 50 }),
                0,
                0,
                100,
                100,
            ),
            mk_node(
                ViewKind::Rect,
                Props::Rect(RectProps {
                    bg: Color { r: 4, g: 5, b: 6 },
                    w: None,
                    h: None,
                }),
                0,
                120,
                40,
                40,
            ),
        ]);
        attach(
            &mut nodes,
            1,
            mk_node(
                ViewKind::Rect,
                Props::Rect(RectProps {
                    bg: Color { r: 1, g: 2, b: 3 },
                    w: None,
                    h: None,
                }),
                0,
                -50,
                100,
                300,
            ),
        );
        let (sum, fb, cmds) = render(&nodes, &PressedVis::empty());
        // 命令面：clip 段标记（界 = Scroll 视口）。
        assert!(sum.contains(&Sum::PushClip(0, 0, 100, 100)), "{sum:?}");
        assert!(sum.contains(&Sum::PopClip), "{sum:?}");
        // 子 fill 原几何入命令（裁剪在回放面——记录/回放分离纪律）。
        assert!(sum.contains(&Sum::Fill(0, -50, 100, 300, 0x0001_0203)), "{sum:?}");
        // push + child + pop + sibling + present。
        assert_eq!(cmds, 5, "{sum:?}");
        // 像素面：窗内照画、窗外哨兵（溢出不再侵入后续区）、妹妹照常。
        assert_eq!(px(&fb, 50, 0), 0x0001_0203, "窗顶照画");
        assert_eq!(px(&fb, 50, 99), 0x0001_0203, "窗底行照画");
        assert_eq!(px(&fb, 50, 100), 0x1111_1111, "窗外（原溢出区）不写");
        assert_eq!(px(&fb, 50, 249), 0x1111_1111, "深溢出区不写");
        assert_eq!(px(&fb, 10, 140), 0x0004_0506, "段外妹妹不受 clip 影响");

        // List 同律：kind 即登记开裁剪。
        let mut nodes = root_with_children(vec![mk_node(
            ViewKind::List,
            Props::List(ListProps { item_count: 10, item_height: 48 }),
            0,
            0,
            240,
            100,
        )]);
        attach(
            &mut nodes,
            1,
            mk_node(
                ViewKind::Rect,
                Props::Rect(RectProps {
                    bg: Color { r: 7, g: 8, b: 9 },
                    w: None,
                    h: None,
                }),
                0,
                0,
                240,
                120,
            ),
        );
        let (sum, fb, cmds) = render(&nodes, &PressedVis::empty());
        assert!(sum.contains(&Sum::PushClip(0, 0, 240, 100)), "{sum:?}");
        assert_eq!(px(&fb, 10, 99), 0x0007_0809, "List 窗内照画");
        assert_eq!(px(&fb, 10, 100), 0x1111_1111, "List 窗外不写");
        assert_eq!(cmds, 4, "push + child + pop + present");
    }
}
