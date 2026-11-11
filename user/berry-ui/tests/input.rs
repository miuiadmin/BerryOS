//! GUI-3 §8 input 行：push_pointer/pump_input 手势全相 + node_at 命中。
//!
//! - 手势链：Press→Click / slop 圈内不出 DragStart、超圈 DragStart→
//!   DragMove→DragEnd / CANCEL 双相（拖中 DragEnd+Cancel、非拖仅
//!   Cancel）/ 无 DOWN 的 MOVE·UP 忽略 / 空白按压零 Msg。
//! - 多指：pointer_id = 身份键，各指各锚。
//! - pressure 0x8000 哨兵折 500（对齐条 13 引擎侧义务）。
//! - node_at：topmost（后画兄弟在顶）、Root 不参与命中、命中随 paint
//!   偏移走（Transform 后视觉命中）。
//! - 输入缓冲满 MsgFull loud。
//!
//! 确定性：pump_input(0) 使重采样惰性（target=0 < 首采样 ts≥1 →
//! None——坐标原样透传），手势断言全值级。

use berry_ui::desc::{Axis, ColProps, Msg, Props, RectProps, ViewSpec};
use berry_ui::{Ui, UiError, ViewKind};
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_guiproto::{
    PHASE_CANCEL, PHASE_DOWN, PHASE_HOVER_MOVE, PHASE_MOVE, PHASE_UP, PRESSURE_UNAVAILABLE,
};

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

fn rect_spec(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(w), h: Some(h) }),
    }
}

/// 两叶分区树：Root→Col{gap:0}{A, B}——A(0,0,30,30) + B(0,30,30,30)
/// （Root 对子是 (0,0) 起满幅 z-stack，分区必须经 Col 游标）。
fn two_pane(ui: &mut Ui<16>) -> (berry_ui::NodeId, berry_ui::NodeId) {
    let root = ui.root();
    let col = ui
        .add(root, ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }) })
        .unwrap();
    let a = ui.add(col, rect_spec(30, 30)).unwrap();
    let b = ui.add(col, rect_spec(30, 30)).unwrap();
    ui.layout();
    ui.drain_damage();
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(0, 0, 30, 30));
    assert_eq!(ui.bounds(b).unwrap(), berry_ui::damage::RectI::new(0, 30, 30, 30));
    (a, b)
}

/// 单事件 pump（expected=0 → 重采样惰性——坐标透传）。
fn pump_one(ui: &mut Ui<16>, phase: u8, pid: u16, x: u32, y: u32, ts: u64) -> Vec<Msg> {
    let ev = berry_ui::hit::PtrEvent::from_wire(phase, pid, x, y, ts, 0, 0);
    ui.push_pointer(ev).unwrap();
    let out = ui.pump_input(0);
    out.iter().copied().collect()
}

#[test]
fn press_click_full_chain() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);

    // DOWN 在 A 内 → Press（携 wire 坐标原值）
    assert_eq!(
        pump_one(&mut ui, PHASE_DOWN, 0, 10, 10, 1),
        vec![Msg::Press { node: a, x: 10, y: 10 }]
    );
    // UP（未拖）→ Release + Click 双发（序 = 手势时序）
    assert_eq!(
        pump_one(&mut ui, PHASE_UP, 0, 12, 14, 2),
        vec![Msg::Release { node: a, x: 12, y: 14 }, Msg::Click { node: a }]
    );
}

#[test]
fn slop_boundary_eight_stays_nine_drags() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);

    pump_one(&mut ui, PHASE_DOWN, 0, 10, 10, 1);
    // slop 圈上（dx=8：64 不严格大于 64）→ 不出手势
    assert_eq!(pump_one(&mut ui, PHASE_MOVE, 0, 18, 10, 2), vec![], "恰在 slop 圈上不出 DragStart");
    // 圈外（dx=9：81 > 64）→ DragStart（D15：携锁轴——纯横向 dx>dy → X）
    assert_eq!(
        pump_one(&mut ui, PHASE_MOVE, 0, 19, 10, 3),
        vec![Msg::DragStart { node: a, axis: Axis::X, x: 19, y: 10 }]
    );
    // 拖中 MOVE → DragMove（v1 引擎非重采样路恒 resampled=false）
    assert_eq!(
        pump_one(&mut ui, PHASE_MOVE, 0, 25, 10, 4),
        vec![Msg::DragMove { node: a, x: 25, y: 10, resampled: false }]
    );
    // 拖后 UP → 仅 DragEnd（D25 扩域：速度环差分——环 [(2M,18),(3M,19),
    // (4M,25)] 全落 96ms 窗，dt=2ns、dx=7px → 3.5e9 px/s 钳 ±32768）
    assert_eq!(
        pump_one(&mut ui, PHASE_UP, 0, 25, 12, 5),
        vec![Msg::DragEnd { node: a, vx: 32768, vy: 0 }]
    );
}

#[test]
fn cancel_terminates_without_up() {
    // 拖中 CANCEL → DragEnd + Cancel 双发
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    pump_one(&mut ui, PHASE_DOWN, 0, 10, 10, 1);
    pump_one(&mut ui, PHASE_MOVE, 0, 19, 10, 2); // DragStart
    assert_eq!(
        pump_one(&mut ui, PHASE_CANCEL, 0, 19, 10, 3),
        vec![Msg::DragEnd { node: a, vx: 0, vy: 0 }, Msg::Cancel { node: a }]
    );
    // 手势已终止：后续 MOVE/UP 无状态可续——全忽略
    assert_eq!(pump_one(&mut ui, PHASE_MOVE, 0, 20, 10, 4), vec![]);
    assert_eq!(pump_one(&mut ui, PHASE_UP, 0, 20, 10, 5), vec![]);

    // 非拖 CANCEL → 仅 Cancel（不补 Release/Click）
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    pump_one(&mut ui, PHASE_DOWN, 0, 10, 10, 1);
    assert_eq!(pump_one(&mut ui, PHASE_CANCEL, 0, 10, 10, 2), vec![Msg::Cancel { node: a }]);
}

#[test]
fn no_down_events_and_hover_ignored() {
    let mut ui = Ui::<16>::new(100, 100);
    let _ = two_pane(&mut ui);
    // 无 DOWN 前置的 MOVE / UP → 忽略
    assert_eq!(pump_one(&mut ui, PHASE_MOVE, 0, 10, 10, 1), vec![]);
    assert_eq!(pump_one(&mut ui, PHASE_UP, 0, 10, 10, 2), vec![]);
    // hover 留位相（5–7）→ v1 无 hover 面，静默忽略
    assert_eq!(pump_one(&mut ui, PHASE_HOVER_MOVE, 0, 10, 10, 3), vec![]);
}

#[test]
fn empty_area_press_tracks_silently() {
    let mut ui = Ui::<16>::new(100, 100);
    let _ = two_pane(&mut ui);
    // 按在空白（两叶外、Root 不参与命中）→ 零 Msg 全程
    assert_eq!(pump_one(&mut ui, PHASE_DOWN, 0, 90, 90, 1), vec![]);
    assert_eq!(pump_one(&mut ui, PHASE_MOVE, 0, 95, 95, 2), vec![]);
    assert_eq!(pump_one(&mut ui, PHASE_UP, 0, 95, 95, 3), vec![]);
}

#[test]
fn two_pointers_independent_gestures() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, b) = two_pane(&mut ui);

    assert_eq!(
        pump_one(&mut ui, PHASE_DOWN, 1, 10, 10, 1),
        vec![Msg::Press { node: a, x: 10, y: 10 }]
    );
    assert_eq!(
        pump_one(&mut ui, PHASE_DOWN, 2, 10, 40, 2),
        vec![Msg::Press { node: b, x: 10, y: 40 }]
    );
    // 各指独立收尾——pointer_id = 身份键
    assert_eq!(
        pump_one(&mut ui, PHASE_UP, 2, 10, 40, 3),
        vec![Msg::Release { node: b, x: 10, y: 40 }, Msg::Click { node: b }]
    );
    assert_eq!(
        pump_one(&mut ui, PHASE_UP, 1, 11, 11, 4),
        vec![Msg::Release { node: a, x: 11, y: 11 }, Msg::Click { node: a }]
    );
}

#[test]
fn input_buffer_full_loud() {
    let mut ui = Ui::<16>::new(100, 100);
    let _ = two_pane(&mut ui);
    // INPUT_BUF_MAX=8：第 9 条 push → MsgFull loud
    for i in 0..berry_ui::INPUT_BUF_MAX {
        let ev = berry_ui::hit::PtrEvent::from_wire(PHASE_MOVE, 0, 10, 10, 1 + i as u64, 0, 0);
        ui.push_pointer(ev).unwrap();
    }
    let ninth = berry_ui::hit::PtrEvent::from_wire(PHASE_MOVE, 0, 10, 10, 100, 0, 0);
    assert_eq!(ui.push_pointer(ninth), Err(UiError::MsgFull));

    // pump 排空（无 DOWN 的 MOVE 全忽略）后缓冲复元
    let out = ui.pump_input(0);
    assert_eq!(out.len(), 0);
    let ok = berry_ui::hit::PtrEvent::from_wire(PHASE_DOWN, 0, 10, 10, 200, 0, 0);
    assert!(ui.push_pointer(ok).is_ok());
}

#[test]
fn pressure_sentinel_folds_and_buttons_passthrough() {
    // 0x8000 哨兵 → 500 毫单位（W3C 0.5——引擎侧义务，对齐条 13）
    let e = berry_ui::hit::PtrEvent::from_wire(PHASE_DOWN, 0, 0, 0, 1, 0, PRESSURE_UNAVAILABLE);
    assert_eq!(e.pressure_mi, 500);
    // 真实值原样透传
    let e = berry_ui::hit::PtrEvent::from_wire(PHASE_DOWN, 0, 0, 0, 1, 0, 300);
    assert_eq!(e.pressure_mi, 300);
    let e = berry_ui::hit::PtrEvent::from_wire(PHASE_DOWN, 0, 0, 0, 1, 0, 1000);
    assert_eq!(e.pressure_mi, 1000);
    // buttons 位域原样透传
    let e = berry_ui::hit::PtrEvent::from_wire(PHASE_DOWN, 0, 0, 0, 1, 0b101, 0);
    assert_eq!(e.buttons, 0b101);
}

#[test]
fn node_at_topmost_root_excluded_paint_offset_follows() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    // 同区重叠双叶：后画者 B 在顶
    let a = ui.add(root, rect_spec(40, 40)).unwrap();
    let b = ui.add(root, rect_spec(40, 40)).unwrap();
    ui.layout();
    ui.drain_damage();

    assert_eq!(ui.node_at(10, 10), Some(b), "后画兄弟 = topmost");
    assert_eq!(ui.kind_of(ui.node_at(10, 10).unwrap()), Ok(ViewKind::Rect));
    // 空白（Root 域内但两叶外）→ None：Root 不参与命中
    assert_eq!(ui.node_at(90, 90), None, "Root 不参与命中");

    // Transform 动画挪走 B（dur=0 即刻完成）→ 命中随视觉走
    ui.anim_add(berry_ui::anim::Anim {
        node: b,
        prop: berry_ui::anim::AnimProp::OffsetX,
        from: 0,
        to: 50,
        start_ns: 1000,
        dur_ns: 0,
        easing: berry_ui::anim::Easing::Linear,
    })
    .unwrap();
    let mut enc = CommandEncoder::new();
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    let out = ui.render_frame(2000, &mut scratch, &mut enc);
    assert!(out.drew, "动画帧必提交");

    assert_eq!(ui.node_at(10, 10), Some(a), "B 已挪走——原位露出下层 A");
    let hit_b = ui.node_at(60, 10);
    assert_eq!(hit_b, Some(b), "B 新视觉位可命中（paint 偏移入命中界）");
    assert_eq!(ui.bounds(b).unwrap(), berry_ui::damage::RectI::new(50, 0, 40, 40));
}

#[test]
fn resampled_move_flags_resampled_bit() {
    // Seam 19：MOVE 重采样命中 → DragMove 携 resampled=true 且坐标 = 重
    // 采样值（替换 wire 原值）；惰性 pump（target < 首采样）→ 原样透传
    // resampled=false（raw 路恒 false——slop 测试的姊妹钉板）
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);

    // 三事件一 pump：DOWN 建锚 + MOVE×2 建历史（1M/2M/4M 全单调）
    let down = berry_ui::hit::PtrEvent::from_wire(PHASE_DOWN, 0, 10, 10, 1_000_000, 0, 0);
    let m1 = berry_ui::hit::PtrEvent::from_wire(PHASE_MOVE, 0, 19, 10, 2_000_000, 0, 0);
    let m2 = berry_ui::hit::PtrEvent::from_wire(PHASE_MOVE, 0, 29, 10, 4_000_000, 0, 0);
    ui.push_pointer(down).unwrap();
    ui.push_pointer(m1).unwrap();
    ui.push_pointer(m2).unwrap();
    let msgs: Vec<Msg> = ui.pump_input(8_000_000).iter().copied().collect();
    assert_eq!(
        msgs,
        vec![
            Msg::Press { node: a, x: 10, y: 10 },
            // 首采样 n<2 → 无重采样，透传 wire 原值
            Msg::DragStart { node: a, axis: Axis::X, x: 19, y: 10 },
            // target = 8M − 5ms = 3M ∈ [2M,4M] → 括值 19 + 10×(1M/2M) = 24
            Msg::DragMove { node: a, x: 24, y: 10, resampled: true },
        ]
    );

    // 惰性 pump（target 0 < 首采样 2M）→ None → 原样透传 false
    let m3 = berry_ui::hit::PtrEvent::from_wire(PHASE_MOVE, 0, 35, 10, 6_000_000, 0, 0);
    ui.push_pointer(m3).unwrap();
    let msgs: Vec<Msg> = ui.pump_input(0).iter().copied().collect();
    assert_eq!(msgs, vec![Msg::DragMove { node: a, x: 35, y: 10, resampled: false }]);
}

// ---------- GUI-6 D15 referee v2 钉板（§11 项 11/12） ----------

/// 多事件一 pump（指定 expected）——首 MOVE ts ≥ 100M，expected 可控
/// 激活/惰性重采样面。
fn pump_seq(ui: &mut Ui<16>, evs: &[(u8, u16, u32, u32, u64)], expected: u64) -> Vec<Msg> {
    for &(phase, pid, x, y, ts) in evs {
        let ev = berry_ui::hit::PtrEvent::from_wire(phase, pid, x, y, ts, 0, 0);
        ui.push_pointer(ev).unwrap();
    }
    ui.pump_input(expected).iter().copied().collect()
}

/// §11 项 11a：96ms 窗差分精确值 + 窗外老样本剔除。环（ms→x）：
/// (100,324),(200,100),(240,132),(280,164)——最新 280ms 回看：240/200
/// 落窗（40/80ms ≤ 96ms），100ms 样本 d=180ms 出窗即停。差分 =
/// (164-100)px / 80ms = **800 px/s**（若误纳 (100,324)：-888——判别值）。
#[test]
fn drag_end_velocity_window_drops_old_samples() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let m = 1_000_000u64;
    let msgs = pump_seq(
        &mut ui,
        &[
            (PHASE_DOWN, 0, 10, 10, 1),
            (PHASE_MOVE, 0, 324, 10, 100 * m), // dx=314 → DragStart（入环）
            (PHASE_MOVE, 0, 100, 10, 200 * m),
            (PHASE_MOVE, 0, 132, 10, 240 * m),
            (PHASE_MOVE, 0, 164, 10, 280 * m),
            (PHASE_UP, 0, 164, 10, 281 * m),
        ],
        0,
    );
    let end = msgs.iter().find(|m| matches!(m, Msg::DragEnd { .. }));
    assert_eq!(
        end,
        Some(&Msg::DragEnd { node: a, vx: 800, vy: 0 }),
        "96ms 窗首末差分：64px/80ms = 800 px/s（(100ms,324) 出窗不参与）"
    );
}

/// §11 项 11b：resample 激活时速度取**原始流**——环入点在替换前。
/// DOWN(1M) → MOVE(2M,19) DragStart → MOVE(4M,29)（重采样替换 x=24，
/// 但环持原始 29）→ UP(8M)：差分 = (29-19)px/2ms = **5000 px/s**
/// （若 resampled 入环：(24-19)/2ms = 2500——判别值）。
#[test]
fn drag_end_velocity_uses_raw_samples_under_resample() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let msgs = pump_seq(
        &mut ui,
        &[
            (PHASE_DOWN, 0, 10, 10, 1_000_000),
            (PHASE_MOVE, 0, 19, 10, 2_000_000),
            (PHASE_MOVE, 0, 29, 10, 4_000_000), // target 3M → 括值 24（替换）
            (PHASE_UP, 0, 29, 10, 8_000_000),
        ],
        8_000_000,
    );
    // 前置自检：重采样确激活（DragMove 携 24 + resampled=true）
    assert!(msgs.contains(&Msg::DragMove { node: a, x: 24, y: 10, resampled: true }));
    assert_eq!(
        msgs.iter().find(|m| matches!(m, Msg::DragEnd { .. })),
        Some(&Msg::DragEnd { node: a, vx: 5000, vy: 0 }),
        "速度环持原始样本（29）非重采样值（24）"
    );
}

/// §11 项 11c/11d：Δt==0（同拍双样本）→ 0；样本 <2 → 0。
#[test]
fn drag_end_velocity_zero_edges() {
    // 同拍：两 MOVE 同 ts=2——差分 Δt=0 → (0,0)
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let msgs = pump_seq(
        &mut ui,
        &[
            (PHASE_DOWN, 0, 10, 10, 1),
            (PHASE_MOVE, 0, 19, 10, 2), // DragStart（入环 (2,19)）
            (PHASE_MOVE, 0, 25, 10, 2), // 同拍（入环 (2,25)——非单调由 resample 拒，环照收）
            (PHASE_UP, 0, 25, 10, 3),
        ],
        0,
    );
    assert_eq!(
        msgs.iter().find(|m| matches!(m, Msg::DragEnd { .. })),
        Some(&Msg::DragEnd { node: a, vx: 0, vy: 0 }),
        "Δt==0 → 速度 0"
    );

    // 单样本：DOWN → 一条 MOVE（DragStart）→ UP——环 n=1 <2 → (0,0)
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let msgs = pump_seq(
        &mut ui,
        &[
            (PHASE_DOWN, 0, 10, 10, 1),
            (PHASE_MOVE, 0, 30, 10, 2),
            (PHASE_UP, 0, 30, 10, 3),
        ],
        0,
    );
    assert_eq!(
        msgs.iter().find(|m| matches!(m, Msg::DragEnd { .. })),
        Some(&Msg::DragEnd { node: a, vx: 0, vy: 0 }),
        "样本 <2 → 速度 0"
    );
}

/// §11 项 12：锁轴三面——(6,6) 对角 = 欧氏触发（72 > 64）且 tie 取 X；
/// (2,9) → Y；DragMove 跨轴分量冻结在 DragStart 触发样本值。
#[test]
fn axis_lock_dominant_axis_and_cross_freeze() {
    // 对角 (6,6)：欧氏 6²+6²=72 > 64 触发（曼哈顿/单轴判定会拒——欧氏
    // 零漂判别）；|dx|>=|dy| tie → X。
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let msgs = pump_seq(
        &mut ui,
        &[
            (PHASE_DOWN, 0, 10, 10, 1),
            (PHASE_MOVE, 0, 16, 16, 2), // dx=6 dy=6 → DragStart axis=X
            (PHASE_MOVE, 0, 20, 40, 3), // 纵向大幅走——X 锁下 y 冻结 16
            (PHASE_UP, 0, 20, 40, 4),
        ],
        0,
    );
    assert_eq!(
        msgs.iter().find(|m| matches!(m, Msg::DragStart { .. })),
        Some(&Msg::DragStart { node: a, axis: Axis::X, x: 16, y: 16 }),
        "对角 (6,6) 欧氏径 8.49>8 触发；tie |dx|>=|dy| → X"
    );
    assert_eq!(
        msgs.iter().find(|m| matches!(m, Msg::DragMove { .. })),
        Some(&Msg::DragMove { node: a, x: 20, y: 16, resampled: false }),
        "X 锁：只透传 x 分量，y 冻结在触发样本值 16"
    );

    // (2,9)：|dy|>|dx| → Y；x 分量冻结 12。
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let msgs = pump_seq(
        &mut ui,
        &[
            (PHASE_DOWN, 0, 10, 10, 1),
            (PHASE_MOVE, 0, 12, 19, 2), // dx=2 dy=9 → 4+81=85>64 → axis=Y
            (PHASE_MOVE, 0, 45, 25, 3), // 横向大幅走——Y 锁下 x 冻结 12
            (PHASE_UP, 0, 45, 25, 4),
        ],
        0,
    );
    assert_eq!(
        msgs.iter().find(|m| matches!(m, Msg::DragStart { .. })),
        Some(&Msg::DragStart { node: a, axis: Axis::Y, x: 12, y: 19 })
    );
    assert_eq!(
        msgs.iter().find(|m| matches!(m, Msg::DragMove { .. })),
        Some(&Msg::DragMove { node: a, x: 12, y: 25, resampled: false }),
        "Y 锁：只透传 y 分量，x 冻结在触发样本值 12"
    );

    // slop 圈内对角 (5,5)：50 < 64 不触发（欧氏保持——单轴判据 5<8 也
    // 不触发，此处钉「圈上原语义不动」）。
    let mut ui = Ui::<16>::new(100, 100);
    let _ = two_pane(&mut ui);
    let msgs = pump_seq(
        &mut ui,
        &[(PHASE_DOWN, 0, 10, 10, 1), (PHASE_MOVE, 0, 15, 15, 2)],
        0,
    );
    assert_eq!(msgs.len(), 1, "对角 (5,5) 欧氏 50 ≤ 64 不出手势（仅 Press）");
}
