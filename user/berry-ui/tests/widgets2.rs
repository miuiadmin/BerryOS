//! 组件七件 Phase 2 第二批钉板（Check/Radio/Step/Seg + Stack/Spacer/
//! Divider——layout/paint/lib 三面接通）：
//!
//! - **布局**：交互四件固定内在值级（Check/Radio 24×24、Step 120×36、
//!   Seg = 段数×80 宽 36 高）；布局三件语义值级（Stack 子聚合 max×max
//!   + `_` 臂全幅叠放 + z 序命中、Spacer 0×0 内在 + final 吃满 avail、
//!   Divider 高 thickness 宽随父）。
//! - **渲染**：FULL 帧逐 Cmd golden（填充序列值级 + Step 三段字形），
//!   Check 勾双矩形 / Radio 八片环 + 内圆 / Seg active 高亮随击移段。
//! - **交互**：Check 点击翻转两来回、Radio 置位恒派发、Step 半区加减
//!   step（clamp 贴边零派发）、Seg 击点折段（值变才派发）——Msg 面不
//!   劫持（Release/Click 原样交 app）。
//! - **命令面**：set_props 五 kind 开口（checked/selected/value/active/
//!   thickness 生效）+ Step/Seg 值域与 kind 失配拒面（add/set_props 两
//!   口）+ Stack/Spacer 半落拒面（align/flex 未消费）。
//!
//! 纪律：确定性、断言值级、只用公共 API（§6 公开面 + paint 色板常量）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, DisplayList, GlyphPos};
use berry_guiproto::{PHASE_DOWN, PHASE_UP};
use berry_ui::damage::RectI;
use berry_ui::desc::{
    CheckProps, ColProps, DividerProps, Msg, PadProps, Props, RadioProps, RectProps, SegProps,
    SpacerProps, StackAlign, StackProps, StepProps,
};
use berry_ui::hit::PtrEvent;
use berry_ui::layout::CrossAlign;
use berry_ui::theme::{ACCENT, ON_SURFACE, OUTLINE, TRACK_DIM};
use berry_ui::{NodeId, Ui, UiError, ViewKind, ViewSpec};

/// 交互四件场景：Root → Col(gap 0, Stretch){Check 未勾 on_change=21,
/// Radio 未选 group=3 on_change=22, Step 5/0..10/step 3 on_change=23,
/// Seg 3 段 active=0 on_change=24}。返回 (ui, chk, rad, stp, seg)。
fn interactive_scene() -> (Ui<64>, NodeId, NodeId, NodeId, NodeId) {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }),
            },
        )
        .unwrap();
    let chk = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Check,
                key: 0,
                props: Props::Check(CheckProps { checked: false, on_change: 21 }),
            },
        )
        .unwrap();
    let rad = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Radio,
                key: 0,
                props: Props::Radio(RadioProps { selected: false, group: 3, on_change: 22 }),
            },
        )
        .unwrap();
    let stp = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Step,
                key: 0,
                props: Props::Step(StepProps { value: 5, min: 0, max: 10, step: 3, on_change: 23 }),
            },
        )
        .unwrap();
    let seg = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Seg,
                key: 0,
                props: Props::Seg(SegProps { segment_count: 3, active: 0, on_change: 24 }),
            },
        )
        .unwrap();
    ui.layout();
    (ui, chk, rad, stp, seg)
}

/// FULL 帧绘制（drain + wake 置 FULL——offscreen 同形标准造法）。
fn paint_full(ui: &mut Ui<64>) -> DisplayList<'static> {
    ui.drain_damage();
    ui.wake(0);
    let scr: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; 16].into_boxed_slice());
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(scr, &mut enc);
    assert!(rep.cmds > 0, "渲染产出非空 op 集（DoD 门）");
    assert!(rep.drawn_px > 0);
    enc.finish()
}

/// FillRect 序列值级抽取：(x, y, w, h, color)。
fn fills(dl: &DisplayList) -> Vec<(i32, i32, i32, i32, Color)> {
    dl.iter()
        .filter_map(|c| match c {
            Cmd::FillRect { rect, paint } => Some((rect.x, rect.y, rect.w, rect.h, paint.color)),
            _ => None,
        })
        .collect()
}

/// GlyphRun 字形抽取：(ch, x, y)。
fn glyphs(dl: &DisplayList) -> Vec<(u8, i32, i32)> {
    let mut out = Vec::new();
    for c in dl.iter() {
        if let Cmd::DrawGlyphRun { run } = c {
            for g in run.glyphs {
                out.push((g.ch as u8, g.x, g.y));
            }
        }
    }
    out
}

/// 单事件 pump（expected=0 → 重采样惰性——坐标透传，press.rs 同形）。
fn pump_one(ui: &mut Ui<64>, phase: u8, x: u32, y: u32, ts: u64) -> Vec<Msg> {
    let ev = PtrEvent::from_wire(phase, 1, x, y, ts, 0, 0);
    ui.push_pointer(ev).unwrap();
    ui.pump_input(0).iter().copied().collect()
}

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

// ---------- ① 布局 + FULL 帧 golden：渲染产出非空 op 集（DoD 门） ----------

#[test]
fn four_interactives_layout_intrinsic_and_paint_golden() {
    let (ui, chk, rad, stp, seg) = interactive_scene();
    let mut ui = ui;

    // 布局值级：固定内在 + Col 内在型叶不吃十字轴 avail（首批四件同列
    // 语义）；主轴游标按内在高累进。
    assert_eq!(ui.bounds(chk).unwrap(), RectI::new(0, 0, 24, 24), "Check 24×24");
    assert_eq!(ui.bounds(rad).unwrap(), RectI::new(0, 24, 24, 24), "Radio 24×24");
    assert_eq!(ui.bounds(stp).unwrap(), RectI::new(0, 48, 120, 36), "Step 120×36");
    assert_eq!(ui.bounds(seg).unwrap(), RectI::new(0, 84, 240, 36), "Seg 3×80 宽 ×36");

    let dl = paint_full(&mut ui);

    // 填充序列逐值对拍（画家序：Check 框 4 → Radio 环 8 → Step 底+双隔
    // 线 → Seg 底+active+双隔线）。
    let f = fills(&dl);
    assert_eq!(
        f,
        vec![
            // Check 未勾方框（OUTLINE 描边四片，t=2）
            (0, 0, 24, 2, OUTLINE),
            (0, 22, 24, 2, OUTLINE),
            (0, 2, 2, 20, OUTLINE),
            (22, 2, 2, 20, OUTLINE),
            // Radio 未选八片环（OUTLINE：上下/左右直段 + 四角块）
            (4, 24, 16, 3, OUTLINE),
            (4, 45, 16, 3, OUTLINE),
            (0, 28, 3, 16, OUTLINE),
            (21, 28, 3, 16, OUTLINE),
            (2, 26, 3, 3, OUTLINE),
            (19, 26, 3, 3, OUTLINE),
            (2, 43, 3, 3, OUTLINE),
            (19, 43, 3, 3, OUTLINE),
            // Step：底 + 左右区带分隔线（区带 36，隔线钉界内侧）
            (0, 48, 120, 36, TRACK_DIM),
            (34, 48, 2, 36, OUTLINE),
            (84, 48, 2, 36, OUTLINE),
            // Seg：底 + active 段 0 高亮 + 段间双隔线（界内居中）
            (0, 84, 240, 36, TRACK_DIM),
            (0, 84, 80, 36, ACCENT),
            (79, 84, 2, 36, OUTLINE),
            (159, 84, 2, 36, OUTLINE),
        ],
        "填充序列值级（固定色板 + 原语组合）"
    );

    // Step 三段字形：'-' 区带 [0,36) 居中 x=14；值 5 中段 [36,84) 居中
    // x=56；'+' 区带 [84,120) 居中 x=98；y = 48+(36-16)/2 = 58。
    assert_eq!(
        glyphs(&dl),
        vec![(b'-', 14, 58), (b'5', 56, 58), (b'+', 98, 58)],
        "三段字形区带居中"
    );

    // 尾 Present + 首命令 = Check 框顶（发射序前序钉板）。
    let cmds: Vec<&Cmd> = dl.iter().collect();
    assert_eq!(cmds.len(), 23, "4 Check + 8 Radio + 5 Step + 4 Seg + Present");
    assert!(matches!(cmds.last(), Some(Cmd::Present { .. })));
    match cmds[0] {
        Cmd::FillRect { paint, .. } => assert_eq!(paint.color, OUTLINE),
        other => panic!("cmd0 应为 Check 框顶 FillRect，实为 {other:?}"),
    }
    // Step 字形前景 = ON_SURFACE（TRACK_DIM 底上）。
    for c in cmds.iter() {
        if let Cmd::DrawGlyphRun { run } = c {
            assert_eq!(run.fg, ON_SURFACE);
            assert_eq!(run.bg, None, "Step 字形恒透明底");
        }
    }
}

// ---------- ② Check 点击翻转：checked + on_change 事件值 ----------

#[test]
fn check_click_flips_checked_and_fires_on_change() {
    let mut ui = Ui::<64>::new(120, 80);
    let root = ui.root();
    let chk = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Check,
                key: 0,
                props: Props::Check(CheckProps { checked: false, on_change: 21 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(chk).unwrap(), RectI::new(0, 0, 24, 24));
    ui.drain_damage();
    ui.wake(0);

    // 第一击：false → true（事件 value=1）。
    let msgs = pump_one(&mut ui, PHASE_DOWN, 12, 12, 1_000_000);
    assert!(matches!(msgs.as_slice(), [Msg::Press { .. }]));
    let msgs = pump_one(&mut ui, PHASE_UP, 12, 12, 2_000_000);
    assert!(
        matches!(msgs.as_slice(), [Msg::Release { .. }, Msg::Click { .. }]),
        "Msg 面不劫持——Release+Click 原样交 app"
    );
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().copied().collect::<Vec<_>>(),
        vec![berry_ui::WidgetEvent { node: chk, event: 21, value: 1 }],
        "Click → 翻转 + on_change(checked=1)"
    );

    // 视觉随翻转：框描边 OUTLINE → ACCENT + 双矩形勾。
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let f = fills(&enc.finish());
    assert!(f.contains(&(5, 12, 4, 4, ACCENT)), "勾短臂：{f:?}");
    assert!(f.contains(&(8, 6, 3, 9, ACCENT)), "勾长臂：{f:?}");
    assert!(!f.iter().any(|&(_, _, _, _, c)| c == OUTLINE), "未选灰不再出现");

    // 第二击：true → false（事件 value=0，勾消框灰还原）。
    pump_one(&mut ui, PHASE_DOWN, 12, 12, 3_000_000);
    pump_one(&mut ui, PHASE_UP, 12, 12, 4_000_000);
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(21, 0)],
        "再击翻回"
    );
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let f = fills(&enc.finish());
    assert!(f.contains(&(0, 0, 24, 2, OUTLINE)), "框灰还原：{f:?}");
    assert!(!f.iter().any(|&(_, _, _, _, c)| c == ACCENT), "勾不再出现");

    assert!(ui.take_events().is_empty(), "take 语义：取走即清");
    assert_eq!(ui.lost_widget_events(), 0);
}

// ---------- ③ Radio 点击置位：恒派发 + 内圆视觉 ----------

#[test]
fn radio_click_selects_and_fires_on_change() {
    let mut ui = Ui::<64>::new(120, 80);
    let root = ui.root();
    let rad = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Radio,
                key: 0,
                props: Props::Radio(RadioProps { selected: false, group: 7, on_change: 22 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(rad).unwrap(), RectI::new(0, 0, 24, 24));
    ui.drain_damage();
    ui.wake(0);

    // 界内点击 → selected 置位 + on_change（value 恒 1——置位语义）。
    pump_one(&mut ui, PHASE_DOWN, 12, 12, 1_000_000);
    let msgs = pump_one(&mut ui, PHASE_UP, 12, 12, 2_000_000);
    assert!(
        matches!(msgs.as_slice(), [Msg::Release { .. }, Msg::Click { node }] if *node == rad)
    );
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().copied().collect::<Vec<_>>(),
        vec![berry_ui::WidgetEvent { node: rad, event: 22, value: 1 }],
        "Click → 置 selected + on_change(1)"
    );

    // 视觉：环 ACCENT + 内圆八角三片（d/2=12 居中 (6,6)）。
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let f = fills(&enc.finish());
    assert!(f.contains(&(4, 0, 16, 3, ACCENT)), "选中环 ACCENT：{f:?}");
    assert!(f.contains(&(9, 6, 6, 4, ACCENT)), "内圆顶片");
    assert!(f.contains(&(6, 10, 12, 4, ACCENT)), "内圆中片");
    assert!(f.contains(&(9, 14, 6, 4, ACCENT)), "内圆底片");

    // 已选再击：仍派发（置位恒派发——组互斥裁决在 app 面）。
    pump_one(&mut ui, PHASE_DOWN, 12, 12, 3_000_000);
    pump_one(&mut ui, PHASE_UP, 12, 12, 4_000_000);
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(22, 1)],
        "已选再击仍派发 on_change"
    );

    // 界外击不派发。
    pump_one(&mut ui, PHASE_DOWN, 100, 12, 5_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 12, 6_000_000);
    assert!(ui.take_events().is_empty(), "界外击不派发");
}

// ---------- ④ Step 半区加减：clamp 贴边零派发 ----------

#[test]
fn step_click_halves_add_sub_with_clamp() {
    let mut ui = Ui::<64>::new(320, 120);
    let root = ui.root();
    let stp = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Step,
                key: 0,
                props: Props::Step(StepProps { value: 5, min: 0, max: 10, step: 3, on_change: 23 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(stp).unwrap(), RectI::new(0, 0, 120, 36));
    ui.drain_damage();
    ui.wake(0);

    // 右半加：5 → 8 → 10（8+3=11 clamp max）→ 贴边零派发。
    // 无 DOWN 前置的 UP：gesture 无槽静默忽略（零 Msg 零派发——先验面）。
    let msgs = pump_one(&mut ui, PHASE_UP, 100, 18, 1_000_000);
    assert!(msgs.is_empty(), "无 DOWN 的 UP 无面");
    assert!(ui.take_events().is_empty(), "无手势零派发");
    // 正式序列：三击右半。
    pump_one(&mut ui, PHASE_DOWN, 100, 18, 2_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 18, 3_000_000);
    pump_one(&mut ui, PHASE_DOWN, 100, 18, 4_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 18, 5_000_000);
    pump_one(&mut ui, PHASE_DOWN, 100, 18, 6_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 18, 7_000_000);
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(23, 8), (23, 10)],
        "右半加 step：5→8→clamp 10"
    );
    pump_one(&mut ui, PHASE_DOWN, 100, 18, 8_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 18, 9_000_000);
    assert!(ui.take_events().is_empty(), "贴边 max 再加零派发（Slider 同形）");

    // 左半减：10 → 7 → 4 → 1 → 0（1-3 饱和 0）→ 贴边零派发。
    for (i, ts) in [(10u32, 10u64), (11, 11), (12, 12), (13, 13)] {
        let _ = i;
        pump_one(&mut ui, PHASE_DOWN, 20, 18, ts * 1_000_000);
        pump_one(&mut ui, PHASE_UP, 20, 18, (ts + 1) * 1_000_000);
    }
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| e.value).collect::<Vec<_>>(),
        vec![7, 4, 1, 0],
        "左半减 step：10→7→4→1→饱和 0"
    );
    pump_one(&mut ui, PHASE_DOWN, 20, 18, 20_000_000);
    pump_one(&mut ui, PHASE_UP, 20, 18, 21_000_000);
    assert!(ui.take_events().is_empty(), "贴边 min 再减零派发");

    // 视觉随值：值 0 → 字形 '0' 居中中段（x=56, y=10）。
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let g = glyphs(&enc.finish());
    assert_eq!(g, vec![(b'-', 14, 10), (b'0', 56, 10), (b'+', 98, 10)], "值 0 三段字形");
    // 布局零扰动（value 纯绘面——几何不变）。
    assert_eq!(ui.bounds(stp).unwrap(), RectI::new(0, 0, 120, 36));
}

// ---------- ⑤ Seg 击点折段：active 高亮随击移段 ----------

#[test]
fn seg_click_switches_active_segment() {
    let mut ui = Ui::<64>::new(320, 120);
    let root = ui.root();
    let seg = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Seg,
                key: 0,
                props: Props::Seg(SegProps { segment_count: 3, active: 0, on_change: 24 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(seg).unwrap(), RectI::new(0, 0, 240, 36));
    ui.drain_damage();
    ui.wake(0);

    // 击段 2（x=200 落 [160,240)）→ active 2 + on_change(2)。
    pump_one(&mut ui, PHASE_DOWN, 200, 18, 1_000_000);
    let msgs = pump_one(&mut ui, PHASE_UP, 200, 18, 2_000_000);
    assert!(
        matches!(msgs.as_slice(), [Msg::Release { .. }, Msg::Click { node }] if *node == seg)
    );
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().copied().collect::<Vec<_>>(),
        vec![berry_ui::WidgetEvent { node: seg, event: 24, value: 2 }],
        "击点折段 → on_change(段索引)"
    );
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(
        fills(&enc.finish()).contains(&(160, 0, 80, 36, ACCENT)),
        "active 高亮移至段 2"
    );

    // 击段 0（x=10）→ active 0；再击同段零派发（值变才派发）。
    pump_one(&mut ui, PHASE_DOWN, 10, 18, 3_000_000);
    pump_one(&mut ui, PHASE_UP, 10, 18, 4_000_000);
    assert_eq!(
        ui.take_events().iter().map(|e| e.value).collect::<Vec<_>>(),
        vec![0],
        "击点折段 0"
    );
    pump_one(&mut ui, PHASE_DOWN, 10, 18, 5_000_000);
    pump_one(&mut ui, PHASE_UP, 10, 18, 6_000_000);
    assert!(ui.take_events().is_empty(), "同段再击零派发");

    // 界外击（x=300 在 seg 240 宽外）→ 无命中无派发。
    pump_one(&mut ui, PHASE_DOWN, 300, 18, 7_000_000);
    pump_one(&mut ui, PHASE_UP, 300, 18, 8_000_000);
    assert!(ui.take_events().is_empty(), "界外击不派发");
}

// ---------- ⑥ 布局三件：Stack 叠层 / Spacer 占位 / Divider 分隔线 ----------

#[test]
fn stack_spacer_divider_layout_semantics() {
    // —— Stack：子聚合 max×max + 全幅叠放 + z 序命中 ——
    // Col(Start) 使 Stack 拿到内在宽为 avail（非 Root 直子的全幅）——
    // intrinsic max×max 可观测。
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Start }),
            },
        )
        .unwrap();
    let stack = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Stack,
                key: 0,
                props: Props::Stack(StackProps { align: StackAlign::Start }),
            },
        )
        .unwrap();
    let r1 = ui
        .add(
            stack,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(100), h: Some(50) }),
            },
        )
        .unwrap();
    let r2 = ui
        .add(
            stack,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: bg(4, 5, 6), w: Some(60), h: Some(80) }),
            },
        )
        .unwrap();
    ui.layout();
    // Stack 内在 = 子 max×max（100×80——与 Col/Row 的 Σ 聚合不同源）。
    assert_eq!(ui.bounds(stack).unwrap(), RectI::new(0, 0, 100, 80), "Stack max×max");
    // 子全幅叠放：两子同钉 (0,0)（`_` 臂 Root 型分配——叠层合义）。
    assert_eq!(ui.bounds(r1).unwrap(), RectI::new(0, 0, 100, 50), "前序子在下");
    assert_eq!(ui.bounds(r2).unwrap(), RectI::new(0, 0, 60, 80), "后序子盖上");
    // z 序命中：交叠区 topmost = 后序子；单覆盖区 = 各自。
    assert_eq!(ui.node_at(50, 10), Some(r2), "交叠区命中后序子（z 顶）");
    assert_eq!(ui.node_at(70, 10), Some(r1), "r2 界外落 r1");
    // 画家序：前序子先画、后序子覆盖（FULL 帧值级）。
    let dl = paint_full(&mut ui);
    assert_eq!(
        fills(&dl),
        vec![(0, 0, 100, 50, bg(1, 2, 3)), (0, 0, 60, 80, bg(4, 5, 6))]
    );
    // Stack 自身无视觉面（容器列——子各自发射，无额外填充）。
    assert_eq!(dl.iter().count(), 3, "2 Rect + Present");

    // —— Spacer：内在 0×0 + final 吃满 avail ——
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let pad = ui
        .add(
            root,
            ViewSpec { kind: ViewKind::Pad, key: 0, props: Props::Pad(PadProps { all: 4 }) },
        )
        .unwrap();
    let sp = ui
        .add(
            pad,
            ViewSpec { kind: ViewKind::Spacer, key: 0, props: Props::Spacer(SpacerProps { flex: 1 }) },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(sp).unwrap(), RectI::new(4, 4, 312, 232), "Spacer 吃满 Pad 内余幅");
    // Col 游标下随主轴内在 0 折叠（flex 权重分配挂后续布局刀——占位不占位）。
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Start }),
            },
        )
        .unwrap();
    let sp2 = ui
        .add(
            col,
            ViewSpec { kind: ViewKind::Spacer, key: 0, props: Props::Spacer(SpacerProps { flex: 1 }) },
        )
        .unwrap();
    let r3 = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: bg(7, 8, 9), w: Some(10), h: Some(10) }),
            },
        )
        .unwrap();
    ui.layout();
    // col = Root 直子（`_` 臂同位 (0,0)——与 pad 叠放）；Spacer 主轴零占
    // （flex 权重分配挂后续布局刀——占位不占位）。
    assert_eq!(ui.bounds(sp2).unwrap(), RectI::new(0, 0, 0, 0), "Spacer 内在 0×0（主轴零占）");
    assert_eq!(ui.bounds(r3).unwrap(), RectI::new(0, 0, 10, 10), "后续子游标零推进");

    // —— Divider：高 thickness 宽随父 + inset 留白 ——
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }),
            },
        )
        .unwrap();
    let _a = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: bg(1, 1, 1), w: Some(320), h: Some(100) }),
            },
        )
        .unwrap();
    let dv = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Divider,
                key: 0,
                props: Props::Divider(DividerProps { thickness: 4, inset_start: 10, inset_end: 20 }),
            },
        )
        .unwrap();
    let _b = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: bg(2, 2, 2), w: Some(320), h: Some(100) }),
            },
        )
        .unwrap();
    ui.layout();
    // 高 = thickness（内在），宽 = 随父（Col 宽 320——intrinsic 宽 0 不占聚合）。
    assert_eq!(ui.bounds(dv).unwrap(), RectI::new(0, 100, 320, 4), "Divider 高 4 宽随父");
    // 渲染：单 fill_rect 带 inset（x=10、w=320-10-20=290）。
    let dl = paint_full(&mut ui);
    assert!(
        fills(&dl).contains(&(10, 100, 290, 4, OUTLINE)),
        "Divider 单线带 inset：{:?}",
        fills(&dl)
    );
}

// ---------- ⑦ 命令面：set_props 五 kind 生效 + 拒面 ----------

#[test]
fn set_props_updates_five_kinds_and_rejects_bad_values() {
    let (ui, chk, rad, stp, seg) = interactive_scene();
    let mut ui = ui;
    ui.drain_damage();
    ui.wake(0);
    let _ = paint_full(&mut ui); // 结账基线（damage 清面）

    // —— Check：checked 变 → 框 ACCENT + 双矩形勾（纯绘面）——
    ui.set_props(chk, Props::Check(CheckProps { checked: true, on_change: 21 })).unwrap();
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(fills(&enc.finish()).contains(&(5, 12, 4, 4, ACCENT)), "checked → 勾现");

    // —— Radio：selected 变 → 环 ACCENT + 内圆 ——
    ui.set_props(rad, Props::Radio(RadioProps { selected: true, group: 3, on_change: 22 }))
        .unwrap();
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(fills(&enc.finish()).contains(&(6, 34, 12, 4, ACCENT)), "selected → 内圆中片");

    // —— Step：value 9 → 字形 '9'（纯绘面，无重排）——
    ui.set_props(stp, Props::Step(StepProps { value: 9, min: 0, max: 10, step: 3, on_change: 23 }))
        .unwrap();
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert_eq!(
        glyphs(&enc.finish()),
        vec![(b'-', 14, 58), (b'9', 56, 58), (b'+', 98, 58)],
        "值 9 字形"
    );

    // —— Seg：segment_count 3→4 → 内在宽 240→320（几何面重排）——
    ui.set_props(seg, Props::Seg(SegProps { segment_count: 4, active: 0, on_change: 24 }))
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(seg).unwrap(), RectI::new(0, 84, 320, 36), "段数 4 → 4×80 宽");

    // —— 同值 set_props：几何等价 → 零重测（双面分录——首批同形）：
    // A 面 props_geom_equal 精确判定 → measured==0；B 面（s7）
    // mask::props_diff 未随本批扩表——组件对落保守失配臂 → measured≥1
    //（正确但保守，扩表面挂 S7 后续刀）。
    ui.layout(); // 先结账
    ui.set_props(chk, Props::Check(CheckProps { checked: true, on_change: 21 })).unwrap();
    let st = ui.layout();
    #[cfg(not(feature = "s7"))]
    assert_eq!(st.measured, 0, "纯绘值变零重测（Measure 保留）");
    #[cfg(feature = "s7")]
    assert!(st.measured >= 1, "s7 面保守臂重排（props_diff 未扩表——文档化）");

    // —— Divider：thickness 几何面 / inset 纯绘面 ——
    let root = ui.root();
    let col2 = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }),
            },
        )
        .unwrap();
    let dv = ui
        .add(
            col2,
            ViewSpec {
                kind: ViewKind::Divider,
                key: 0,
                props: Props::Divider(DividerProps { thickness: 4, inset_start: 0, inset_end: 0 }),
            },
        )
        .unwrap();
    ui.layout();
    // col2 = Root 直子（`_` 臂同位 (0,0) 起步——与 col1 叠放，非游标续接）。
    assert_eq!(ui.bounds(dv).unwrap(), RectI::new(0, 0, 320, 4), "col2 首件 Divider");
    ui.set_props(dv, Props::Divider(DividerProps { thickness: 6, inset_start: 0, inset_end: 0 }))
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(dv).unwrap(), RectI::new(0, 0, 320, 6), "thickness 6 → 高 6");
    ui.layout(); // 结账
    ui.set_props(dv, Props::Divider(DividerProps { thickness: 6, inset_start: 30, inset_end: 30 }))
        .unwrap();
    let st = ui.layout();
    #[cfg(not(feature = "s7"))]
    assert_eq!(st.measured, 0, "inset 纯绘面零重测");
    #[cfg(feature = "s7")]
    assert!(st.measured >= 1, "s7 面保守臂重排（文档化）");

    // —— 拒面：Step/Seg 值域（set_props + add 两口）——
    assert_eq!(
        ui.set_props(
            stp,
            Props::Step(StepProps { value: 5, min: 0, max: 10, step: 0, on_change: 23 })
        ),
        Err(UiError::NodeBad),
        "Step step=0 拒"
    );
    assert_eq!(
        ui.set_props(
            stp,
            Props::Step(StepProps { value: 5, min: 10, max: 5, step: 3, on_change: 23 })
        ),
        Err(UiError::NodeBad),
        "Step min>max 拒"
    );
    assert_eq!(
        ui.set_props(
            stp,
            Props::Step(StepProps { value: 11, min: 0, max: 10, step: 3, on_change: 23 })
        ),
        Err(UiError::NodeBad),
        "Step value>max 拒"
    );
    assert_eq!(
        ui.set_props(
            stp,
            Props::Step(StepProps { value: 0, min: 1, max: 10, step: 3, on_change: 23 })
        ),
        Err(UiError::NodeBad),
        "Step value<min 拒"
    );
    assert_eq!(
        ui.set_props(seg, Props::Seg(SegProps { segment_count: 0, active: 0, on_change: 24 })),
        Err(UiError::NodeBad),
        "Seg 零段拒"
    );
    assert_eq!(
        ui.set_props(seg, Props::Seg(SegProps { segment_count: 3, active: 3, on_change: 24 })),
        Err(UiError::NodeBad),
        "Seg active 越段拒"
    );
    assert_eq!(
        ui.add(
            root,
            ViewSpec {
                kind: ViewKind::Step,
                key: 0,
                props: Props::Step(StepProps { value: 11, min: 0, max: 10, step: 1, on_change: 0 })
            },
        ),
        Err(UiError::NodeBad),
        "add 面 Step 值域同律拒"
    );
    assert_eq!(
        ui.add(
            root,
            ViewSpec {
                kind: ViewKind::Seg,
                key: 0,
                props: Props::Seg(SegProps { segment_count: 0, active: 0, on_change: 0 })
            },
        ),
        Err(UiError::NodeBad),
        "add 面 Seg 零段同律拒"
    );

    // —— 拒面：kind 失配 + 半落件（Stack/Spacer——align/flex 未消费）——
    assert_eq!(
        ui.set_props(chk, Props::Radio(RadioProps { selected: true, group: 3, on_change: 22 })),
        Err(UiError::NodeBad),
        "kind 失配拒（Radio props 塞 Check 槽）"
    );
    let stack = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Stack,
                key: 0,
                props: Props::Stack(StackProps { align: StackAlign::Center }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.set_props(stack, Props::Stack(StackProps { align: StackAlign::End })),
        Err(UiError::NodeBad),
        "Stack 未开口——set_props 拒（align 未消费，防半落）"
    );
    let spacer = ui
        .add(
            root,
            ViewSpec { kind: ViewKind::Spacer, key: 0, props: Props::Spacer(SpacerProps { flex: 2 }) },
        )
        .unwrap();
    assert_eq!(
        ui.set_props(spacer, Props::Spacer(SpacerProps { flex: 3 })),
        Err(UiError::NodeBad),
        "Spacer 未开口——set_props 拒（flex 未消费，防半落）"
    );
    // 叶带子拒面（add 口——AST 面「叶带子拒面」同律）。
    assert_eq!(
        ui.add(
            chk,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: bg(0, 0, 0), w: None, h: None })
            },
        ),
        Err(UiError::BadParent),
        "Check 叶父位拒"
    );
}
