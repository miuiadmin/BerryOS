//! SwipePager 分页控制器（GUI-6 D16——§7.4；**非新 ViewKind**：app 侧
//! 普通库代码持 `&mut Ui`，引擎零闭包零特判 = ADR #52 Msg-only 纯度）。
//!
//! - **建树**（`attach`）：parent 下两件——页行 `Row`（w=viewport 定值、
//!   h 随父）+ 页容器 `Col`×page_count（w=viewport 定值——页框 =
//!   viewport 全宽，v1 无 clip 挂账）+ 圆点行 `Row`（h=DOT_SIZE 定值）+
//!   圆点 `Rect`×page_count（w=DOT_SIZE、h 随行）。app 经 `page_node`
//!   向页容器填内容（GUI-8 launcher 接线面）。
//! - **跟手**：DragMove 期 `Ui::set_offset(track)` 直写 ox（D31 Transform
//!   路径免 relayout）；橡皮筋 ×0.5（首/末页越界侧 excess/2）。
//! - **翻页判据**（松手）：速度 |vx| ≥500px/s 按速度向翻一页；否则位移
//!   >40% 页宽按位移向；否则回弹吸附。吸附 = QuadOut 240ms（§7.2
//!   `anim::interp` 单源 Q16.16 定点）。
//! - **时钟**：Msg 无时戳位——吸附推进走 `tick(ui, now_ns)`（帧循环调；
//!   起拍 = 首次 tick 的 now_ns，绕开引擎 anim 表的 start_ns 缺时钟缝）
//!   ——§6 冻结四方法外的最小补面。
//! - **圆点指示器**：自管节点组；页裁定（含吸附途中）即直摆高亮
//!   （`on_msg` 返 true 语义）——paint-only props，measured==0 面。
//! - 零 panic / 零分配 / 无浮点：offset 全 i32、判据阈值 i32 整数算术。

use crate::anim::{self, Easing};
use crate::desc::{Axis, ColProps, Msg, NodeId, Props, RectProps, RowProps, ViewKind, ViewSpec};
use crate::layout::CrossAlign;
use crate::{Ui, UiError};

/// 页数帽（§6 冻结）：`new` 有效域 1..=8，超 = loud。
pub const SWIPE_PAGES_MAX: u8 = 8;

/// 吸附动画时长（§7.4：QuadOut 240ms）。
pub const SNAP_DUR_NS: u64 = 240_000_000;

/// 翻页速度阈（§7.4：|v| ≥ 500px/s 按速度向翻页）。
pub const FLIP_VEL_PX_S: i32 = 500;

/// 圆点边长（px）+ 圆点行 gap。
const DOT_SIZE: u32 = 16;

/// 当前页圆点亮色（白）。
pub const DOT_ON: berry_gfx::cmd::Color = berry_gfx::cmd::Color { r: 255, g: 255, b: 255 };

/// 非当前页圆点暗色（灰）。
pub const DOT_OFF: berry_gfx::cmd::Color = berry_gfx::cmd::Color { r: 128, g: 128, b: 128 };

/// 吸附态（一次至多一条）：`start_ns = None` = 未起拍（首次 tick 定）。
/// 目标页在 `begin_snap` 即裁定进 `self.page`/圆点——此处只余轨迹簿记。
struct Snap {
    from: i32,
    to: i32,
    start_ns: Option<u64>,
}

/// 分页控制器（§6 冻结状态私有——「page 状态 + 动画簿记」）。
pub struct SwipePager {
    page_count: u8,
    page: u8,
    attached: bool,
    /// 页行节点（offset 直写对象；`NodeId(NONE)` 哨兵 = 未 attach）。
    track: NodeId,
    pages: [Option<NodeId>; SWIPE_PAGES_MAX as usize],
    dots: [Option<NodeId>; SWIPE_PAGES_MAX as usize],
    /// attach 时取的页宽（= viewport 宽；viewport 变更后需重 attach——
    /// v1 挂账：offset 基准与页宽冻结在建树时）。
    vp_w: u32,
    /// track 现行 ox 镜像（唯一写者 = 本控制器；`Ui` 无 ox 读面，跟手
    /// base 与页域判定取此镜像，免新增引擎 getter）。
    ox: i32,
    dragging: bool,
    /// 起拖时 ox（跟手 delta 基准）。
    base_ox: i32,
    /// 起拖点 x（位移判据基准；wire u32 → i32 饱和）。
    drag_x0: i32,
    /// 最近 DragMove x（DragEnd 无坐标位——位移判据用）。
    drag_last_x: i32,
    snap: Option<Snap>,
}

impl SwipePager {
    /// 构造（§6 冻结签名非 Result——域外 = loud panic：0 页/超 8 页的
    /// pager 无视觉语义，属编程错）。
    pub fn new(page_count: u8) -> Self {
        assert!(
            page_count >= 1 && page_count <= SWIPE_PAGES_MAX,
            "SwipePager::new: page_count 域 1..={SWIPE_PAGES_MAX}，got {page_count}"
        );
        Self {
            page_count,
            page: 0,
            attached: false,
            track: NodeId(u32::MAX),
            pages: [None; SWIPE_PAGES_MAX as usize],
            dots: [None; SWIPE_PAGES_MAX as usize],
            vp_w: 0,
            ox: 0,
            dragging: false,
            base_ox: 0,
            drag_x0: 0,
            drag_last_x: 0,
            snap: None,
        }
    }

    /// 建树挂接（§6 冻结）：parent 下建页行 + 圆点行；页宽 = 当前
    /// `ui.viewport()` 宽。返回**页行 track 节点**（app 可对其再
    /// `set_offset` 覆写——一般无需）。重复 attach = 再建一组（旧组
    /// 节点仍活——v1 由 app 自管 remove；近帽测试口径按本方法计节点数）。
    pub fn attach<const N: usize>(
        &mut self,
        ui: &mut Ui<N>,
        parent: NodeId,
    ) -> Result<NodeId, UiError> {
        let (vp_w, _vp_h) = ui.viewport();
        self.vp_w = vp_w;
        let track = ui.add(
            parent,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: 0,
                    w: Some(vp_w), // 页框 = viewport 全宽（v1 免 clip——§3 挂账）
                    h: None,
                    cross: CrossAlign::Stretch,
                }),
            },
        )?;
        self.track = track;
        for i in 0..self.page_count {
            let p = ui.add(
                track,
                ViewSpec {
                    kind: ViewKind::Col,
                    key: 0,
                    props: Props::Col(ColProps {
                        gap: 0,
                        w: Some(vp_w), // 页定宽——Row 游标按 meas_w 步进
                        h: None,
                        cross: CrossAlign::Center, // 固定宽 Grid/Dock 水平居中
                    }),
                },
            )?;
            self.pages[i as usize] = Some(p);
        }
        let dot_row = ui.add(
            parent,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: DOT_SIZE,
                    w: None,
                    h: Some(DOT_SIZE),
                    cross: CrossAlign::Stretch,
                }),
            },
        )?;
        for i in 0..self.page_count {
            let d = ui.add(
                dot_row,
                ViewSpec {
                    kind: ViewKind::Rect,
                    key: 0,
                    props: Props::Rect(RectProps {
                        bg: if i == 0 { DOT_ON } else { DOT_OFF },
                        w: Some(DOT_SIZE),
                        h: None, // 随行（行 h=DOT_SIZE 定值 → 16×16）
                    }),
                },
            )?;
            self.dots[i as usize] = Some(d);
        }
        self.ox = 0;
        self.attached = true;
        Ok(track)
    }

    /// 当前页（0 基；吸附途中 = 已裁定的目标页——`page()` 即时反映翻页
    /// 决定，不等动画收尾）。
    pub fn page(&self) -> u8 {
        self.page
    }

    /// 页容器访问（GUI-8 接线面：app 向页内 `ui.add` 内容）；域外/未
    /// attach 页 → None。
    pub fn page_node(&self, i: u8) -> Option<NodeId> {
        if i >= self.page_count {
            return None;
        }
        self.pages[i as usize]
    }

    /// 直接落位到指定页（段④ 自测收尾用：合成翻页只验状态机，验完回
    /// page 0，避免用户开机落在空页；越界/未 attach 静默拒）。
    pub fn snap_to<const N: usize>(&mut self, ui: &mut Ui<N>, page: u8) {
        if !self.attached || page >= self.page_count {
            return;
        }
        self.dragging = false;
        self.snap = None;
        self.page = page;
        self.update_dots(ui);
        self.apply_ox(ui, self.page_ofs(page));
    }

    /// Msg 消费（§6 冻结——app update 内调；true = pager 状态已变需重
    /// 摆；未 attach → false）。页域 = 起拖点落在**全页带**（base..base
    /// + page_count×页宽——由 ox 镜像重建 base，吸附途中判定仍准）；
    /// 纵拖（axis Y）不认（归 app）。
    pub fn on_msg<const N: usize>(&mut self, ui: &mut Ui<N>, msg: &Msg) -> bool {
        if !self.attached {
            return false;
        }
        match msg {
            Msg::DragStart { axis, x, y, .. } => {
                if *axis != Axis::X {
                    return false; // 纵拖 = pager 域外（页带横滚）
                }
                if !self.in_page_strip(ui, *x, *y) {
                    return false;
                }
                // 吸附途中抓回：中断动画，跟手基准 = 现行 ox 镜像。
                self.snap = None;
                self.dragging = true;
                self.base_ox = self.ox;
                let sx = u32_sat_i32(*x);
                self.drag_x0 = sx;
                self.drag_last_x = sx;
                true
            }
            Msg::DragMove { x, .. } => {
                if !self.dragging {
                    return false;
                }
                self.drag_last_x = u32_sat_i32(*x);
                let dx = self.drag_last_x - self.drag_x0;
                let mut ox = self.base_ox + dx;
                // 橡皮筋 ×0.5（§7.4）：越界侧 excess/2——首页 ox>0 /
                // 末页 ox<min_ox。
                let min_ox = self.page_ofs(self.page_count - 1);
                if ox > 0 {
                    ox = ox / 2;
                }
                if ox < min_ox {
                    ox = min_ox + (ox - min_ox) / 2;
                }
                self.apply_ox(ui, ox);
                true
            }
            Msg::DragEnd { vx, .. } => {
                if !self.dragging {
                    return false;
                }
                self.dragging = false;
                let dx = self.drag_last_x - self.drag_x0;
                // 翻页三判据（§7.4）：速度向优先 → 位移 40% → 回弹。
                let target = if vx.abs() >= FLIP_VEL_PX_S {
                    if *vx > 0 { self.page.saturating_sub(1) } else { self.next_page() }
                } else if dx.abs() > flip_threshold(self.vp_w) {
                    if dx < 0 { self.next_page() } else { self.page.saturating_sub(1) }
                } else {
                    self.page
                };
                self.begin_snap(ui, target);
                true
            }
            Msg::Cancel { .. } => {
                if !self.dragging {
                    return false;
                }
                // 取消 = 回弹吸附当前页（无翻页裁定）。
                self.dragging = false;
                self.begin_snap(ui, self.page);
                true
            }
            Msg::Press { .. } | Msg::Release { .. } | Msg::Click { .. } => false,
        }
    }

    /// 吸附推进（帧循环调；无吸附 → false 零开销）。起拍 = 首次 tick 的
    /// now_ns（Msg 无时戳位——§7.4 时钟缝的 tick 补面）；完成帧应用精确
    /// `to` 终点（与 anim::tick 完成语义同式）。
    pub fn tick<const N: usize>(&mut self, ui: &mut Ui<N>, now_ns: u64) -> bool {
        let Some(s) = &mut self.snap else { return false };
        let start = match s.start_ns {
            Some(t) => t,
            None => {
                s.start_ns = Some(now_ns);
                now_ns
            }
        };
        let elapsed = now_ns.saturating_sub(start);
        if elapsed >= SNAP_DUR_NS {
            let to = s.to;
            self.apply_ox(ui, to);
            self.snap = None;
            return true;
        }
        let v = anim::interp(Easing::QuadOut, s.from, s.to, elapsed, SNAP_DUR_NS);
        self.apply_ox(ui, v);
        true
    }

    // —— 私面 ——

    /// 页 p 的静止 ox（页带左起 base：页 0 → 0，页 p → -p×页宽）。
    fn page_ofs(&self, p: u8) -> i32 {
        -((p as i32) * (self.vp_w as i32))
    }

    /// 下一页（末页饱和）。
    fn next_page(&self) -> u8 {
        self.page.saturating_add(1).min(self.page_count - 1)
    }

    /// offset 直写 + 镜像同步（track 死亡（app 误 remove）→ 静默拒：
    /// set_props 同族防御面——后续 Msg 全 no-op，不 panic）。
    fn apply_ox<const N: usize>(&mut self, ui: &mut Ui<N>, ox: i32) {
        if ui.set_offset(self.track, ox, 0).is_ok() {
            self.ox = ox;
        }
    }

    /// 吸附启动：页即时裁定（page()/圆点不等动画——「on_msg 返 true 时
    /// pager 直摆」）；零位移免动画直接落位。
    fn begin_snap<const N: usize>(&mut self, ui: &mut Ui<N>, target_page: u8) {
        self.page = target_page;
        self.update_dots(ui);
        let to = self.page_ofs(target_page);
        if self.ox == to {
            self.snap = None; // 已在位（零位移松手）：免动画
            return;
        }
        self.snap = Some(Snap { from: self.ox, to, start_ns: None });
    }

    /// 圆点直摆（paint-only props 变更——measured==0 面；死点静默跳过）。
    fn update_dots<const N: usize>(&mut self, ui: &mut Ui<N>) {
        for i in 0..self.page_count as usize {
            let Some(d) = self.dots[i] else { continue };
            let bg = if i == self.page as usize { DOT_ON } else { DOT_OFF };
            let _ = ui.set_props(
                d,
                Props::Rect(RectProps { bg, w: Some(DOT_SIZE), h: None }),
            );
        }
    }

    /// 页域判定：点在全页带内（x ∈ [base, base+n×页宽)、y ∈ track 纵界
    /// ——base 由 bounds.x − ox 镜像重建，snap 途中亦准；含越界橡皮筋
    /// 区（宽取全页带非 viewport））。
    fn in_page_strip<const N: usize>(&self, ui: &Ui<N>, x: u32, y: u32) -> bool {
        let Ok(b) = ui.bounds(self.track) else { return false };
        let base = b.x as i64 - self.ox as i64;
        let px = x as i64;
        let py = y as i64;
        px >= base
            && px < base + (self.page_count as i64) * (self.vp_w as i64)
            && py >= b.y as i64
            && py < b.y as i64 + b.h.max(0) as i64
    }
}

/// 翻页位移阈：40% 页宽（i32 整数——`w*2/5`，严格 > 判定在上文）。
fn flip_threshold(vp_w: u32) -> i32 {
    (vp_w as i32 * 2) / 5
}

/// wire u32 → i32 饱和（§0 坐标铁律——同 `hit::u32_to_i32_sat` 口径，
/// app 侧模块自带防跨模块私有依赖）。
fn u32_sat_i32(v: u32) -> i32 {
    if v > i32::MAX as u32 {
        i32::MAX
    } else {
        v as i32
    }
}
