//! 组件四件 Phase 2 首批钉板（Button/Toggle/Slider/Progress——
//! layout/paint/lib 三面接通）：
//!
//! - **布局**：固定内在值级（Button 标签宽式 + 下限 64/恒高 48、
//!   Toggle 52×32、Slider 200×44、Progress 200×8）+ Col 内在型叶
//!   不吃十字轴 avail（Text 同列语义）。
//! - **渲染**：FULL 帧逐 Cmd golden（填充序列值级 + 标签字形居中）
//!   ——「渲染产出非空 op 集」DoD 门；固定色板对拍（paint 色板常量）。
//! - **交互**：Toggle 点击翻转（checked + on_change 事件值）两来回、
//!   Button on_tap 派发、Slider 拖跟（DragStart 跳位 + DragMove 跟
//!   手 → permille 值级 + 填充宽随值）——Msg 面不劫持（Click 原样
//!   交 app）。
//! - **命令面**：set_props 四 kind 全开口（label/checked/value 生效——
//!   bounds/填充值级对拍）+ permille 值域与 kind 失配拒面（add/
//!   set_props 两口）。
//!
//! 纪律：确定性、断言值级、只用公共 API（§6 公开面 + paint 色板常量）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, DisplayList, GlyphPos};
use berry_guiproto::{PHASE_DOWN, PHASE_MOVE, PHASE_UP};
use berry_ui::damage::RectI;
use berry_ui::desc::{
    ButtonProps, ButtonStyle, ColProps, Msg, ProgressProps, ProgressStyle, Props, RectProps,
    SliderProps, ToggleProps,
};
use berry_ui::hit::PtrEvent;
use berry_ui::layout::CrossAlign;
use berry_ui::theme::{ACCENT, KNOB, ON_ACCENT, TRACK_DIM, TRACK_OFF};
use berry_ui::{NodeId, TextBuf, Ui, UiError, ViewKind, ViewSpec};

/// 四件套场景：Root → Col(gap 0, Stretch){Button "OK" Filled on_tap=7,
/// Toggle off on_change=9, Slider 250/0..1000 on_change=11, Progress
/// 500 Linear}。返回 (ui, btn, tog, sld, prg)。
fn widget_scene() -> (Ui<64>, NodeId, NodeId, NodeId, NodeId) {
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
    let btn = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Button,
                key: 0,
                props: Props::Button(ButtonProps {
                    label: TextBuf::new(b"OK").unwrap(),
                    style: ButtonStyle::Filled,
                    on_tap: 7,
                }),
            },
        )
        .unwrap();
    let tog = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Toggle,
                key: 0,
                props: Props::Toggle(ToggleProps { checked: false, on_change: 9 }),
            },
        )
        .unwrap();
    let sld = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Slider,
                key: 0,
                props: Props::Slider(SliderProps { value: 250, min: 0, max: 1000, on_change: 11 }),
            },
        )
        .unwrap();
    let prg = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Progress,
                key: 0,
                props: Props::Progress(ProgressProps { value: 500, style: ProgressStyle::Linear }),
            },
        )
        .unwrap();
    ui.layout();
    (ui, btn, tog, sld, prg)
}

/// FULL 帧绘制（drain + wake 置 FULL——offscreen 同形标准造法）。
/// scratch 经 Box::leak 钉 'static（测试面豁免：每帧 16×12B——
/// DisplayList 的 'a 钉调用方缓冲语义不改）。
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
                out.push((g.ch, g.x, g.y));
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

// ---------- ① 布局 + FULL 帧 golden：渲染产出非空 op 集（DoD 门） ----------

#[test]
fn four_widgets_layout_intrinsic_and_paint_golden() {
    let (ui, btn, tog, sld, prg) = widget_scene();
    let mut ui = ui;

    // 布局值级：固定内在 + Col 内在型叶不吃十字轴 avail（320 宽列内
    // 各保内在宽——Text 同列语义）；主轴游标按内在高累进。
    assert_eq!(ui.bounds(btn).unwrap(), RectI::new(0, 0, 64, 48), "OK=2 字节 → max(2×8+32, 64) = 64");
    assert_eq!(ui.bounds(tog).unwrap(), RectI::new(0, 48, 52, 32), "Toggle 52×32");
    assert_eq!(ui.bounds(sld).unwrap(), RectI::new(0, 80, 200, 44), "Slider 200×44");
    assert_eq!(ui.bounds(prg).unwrap(), RectI::new(0, 124, 200, 8), "Progress 200×8");

    let dl = paint_full(&mut ui);

    // 填充序列逐值对拍（画家序：Button 底 → Toggle 轨 3 + 滑块 3 →
    // Slider 轨/填充/圆 3 → Progress 轨/填充）。
    let f = fills(&dl);
    assert_eq!(
        f,
        vec![
            // Button Filled：ACCENT 底
            (0, 0, 64, 48, ACCENT),
            // Toggle 关轨胶囊（TRACK_OFF）：左帽/中段/右帽
            (0, 56, 10, 16, TRACK_OFF),
            (10, 48, 32, 32, TRACK_OFF),
            (42, 56, 10, 16, TRACK_OFF),
            // 滑块白（关位左侧）：八角三片
            (10, 52, 12, 8, KNOB),
            (4, 60, 24, 8, KNOB),
            (10, 68, 12, 8, KNOB),
            // Slider：底轨 + 填充（250‰ → 行程 45 + inset 10 = 55 宽）
            (0, 100, 200, 4, TRACK_DIM),
            (0, 100, 55, 4, ACCENT),
            // 拖动圆（圆心 x = 10 + 180×250/1000 = 55）：八角三片
            (50, 92, 10, 6, KNOB),
            (45, 98, 20, 8, KNOB),
            (50, 106, 10, 6, KNOB),
            // Progress：底轨 + 500‰ 填充
            (0, 124, 200, 8, TRACK_DIM),
            (0, 124, 100, 8, ACCENT),
        ],
        "填充序列值级（固定色板 + 原语组合）"
    );

    // Button 标签："OK" 16px 宽居中 64 宽 → x=24；16px 高居中 48 → y=16。
    assert_eq!(glyphs(&dl), vec![(b'O', 24, 16), (b'K', 32, 16)], "标签双轴居中");

    // 尾 Present + 首命令 = Button 底（发射序前序钉板）。
    let cmds: Vec<&Cmd> = dl.iter().collect();
    assert_eq!(cmds.len(), 16, "2 Button + 6 Toggle + 5 Slider + 2 Progress + Present");
    assert!(matches!(cmds.last(), Some(Cmd::Present { .. })));
    match cmds[0] {
        Cmd::FillRect { paint, .. } => assert_eq!(paint.color, ACCENT),
        other => panic!("cmd0 应为 Button 底 FillRect，实为 {other:?}"),
    }
    // 标签前景 = Filled 白（ON_ACCENT）。
    for c in cmds.iter() {
        if let Cmd::DrawGlyphRun { run } = c {
            assert_eq!(run.fg, ON_ACCENT);
            assert_eq!(run.bg, None, "按钮标签恒透明底");
        }
    }
}

// ---------- ② Toggle 点击翻转：checked + on_change 事件值 ----------

#[test]
fn toggle_click_flips_checked_and_fires_on_change() {
    let mut ui = Ui::<64>::new(120, 80);
    let root = ui.root();
    let tog = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Toggle,
                key: 0,
                props: Props::Toggle(ToggleProps { checked: false, on_change: 9 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(tog).unwrap(), RectI::new(0, 0, 52, 32));
    ui.drain_damage();
    ui.wake(0);

    // 第一击：false → true（事件 value=1）。
    let msgs = pump_one(&mut ui, PHASE_DOWN, 26, 16, 1_000_000);
    assert!(matches!(msgs.as_slice(), [Msg::Press { .. }]));
    let msgs = pump_one(&mut ui, PHASE_UP, 26, 16, 2_000_000);
    assert!(
        matches!(msgs.as_slice(), [Msg::Release { .. }, Msg::Click { .. }]),
        "Msg 面不劫持——Release+Click 原样交 app"
    );
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().copied().collect::<Vec<_>>(),
        vec![berry_ui::WidgetEvent { node: tog, event: 9, value: 1 }],
        "Click → 翻转 + on_change(checked=1)"
    );

    // 视觉随翻转变色：关轨灰 → ACCENT（增量 damage 重发本界）。
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    let dl = enc.finish();
    let f = fills(&dl);
    assert!(f.contains(&(10, 0, 32, 32, ACCENT)), "开轨 ACCENT 中段：{f:?}");
    assert!(!f.iter().any(|&(_, _, _, _, c)| c == TRACK_OFF), "关轨灰不再出现");

    // 第二击：true → false（事件 value=0，轨色还原）。
    pump_one(&mut ui, PHASE_DOWN, 26, 16, 3_000_000);
    pump_one(&mut ui, PHASE_UP, 26, 16, 4_000_000);
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(9, 0)],
        "再击翻回"
    );
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(fills(&enc.finish()).contains(&(10, 0, 32, 32, TRACK_OFF)));

    // take 语义：取走即清。
    assert!(ui.take_events().is_empty());
    assert_eq!(ui.lost_widget_events(), 0);
}

// ---------- ③ Button 点击：on_tap 派发 ----------

#[test]
fn button_click_fires_on_tap() {
    let mut ui = Ui::<64>::new(120, 80);
    let root = ui.root();
    let btn = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Button,
                key: 0,
                props: Props::Button(ButtonProps {
                    label: TextBuf::new(b"OK").unwrap(),
                    style: ButtonStyle::Filled,
                    on_tap: 7,
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(btn).unwrap(), RectI::new(0, 0, 64, 48));

    // 界外点击（(100,24) 在 120 宽 vp 内、按钮外）→ 无命中无事件。
    pump_one(&mut ui, PHASE_DOWN, 100, 24, 1_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 24, 2_000_000);
    assert!(ui.take_events().is_empty(), "界外击不派发");

    // 界内点击 → on_tap=7（value 恒 0）。
    pump_one(&mut ui, PHASE_DOWN, 32, 24, 3_000_000);
    let msgs = pump_one(&mut ui, PHASE_UP, 32, 24, 4_000_000);
    assert!(matches!(msgs.as_slice(), [Msg::Release { .. }, Msg::Click { node }] if *node == btn));
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().copied().collect::<Vec<_>>(),
        vec![berry_ui::WidgetEvent { node: btn, event: 7, value: 0 }],
        "Click → on_tap"
    );
}

// ---------- ④ Slider 拖跟：DragStart 跳位 + DragMove 跟手 ----------

#[test]
fn slider_drag_follows_and_fires_on_change() {
    let mut ui = Ui::<64>::new(320, 120);
    let root = ui.root();
    let sld = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Slider,
                key: 0,
                props: Props::Slider(SliderProps { value: 0, min: 0, max: 1000, on_change: 11 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(sld).unwrap(), RectI::new(0, 0, 200, 44));
    ui.drain_damage();
    ui.wake(0);

    // 按下 (100,22) → 越 slop 移到 (120,22) 触发 DragStart（跳位到按压
    // 横坐标）→ 跟手 (190,22) → 抬起。permille 行程：usable = 200−20 =
    // 180，rel = x − 10：120 → 110×1000/180 = 611；190 → 钳满 1000。
    pump_one(&mut ui, PHASE_DOWN, 100, 22, 1_000_000);
    let msgs = pump_one(&mut ui, PHASE_MOVE, 120, 22, 2_000_000);
    assert!(matches!(msgs.as_slice(), [Msg::DragStart { .. }]), "slop 越 8 触发拖拽");
    let msgs = pump_one(&mut ui, PHASE_MOVE, 190, 22, 3_000_000);
    assert!(matches!(msgs.as_slice(), [Msg::DragMove { .. }]));
    let msgs = pump_one(&mut ui, PHASE_UP, 190, 22, 4_000_000);
    assert!(matches!(msgs.as_slice(), [Msg::DragEnd { .. }]));

    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(11, 611), (11, 1000)],
        "DragStart 跳位 + DragMove 跟手（值变才派发）"
    );

    // 填充随值：1000‰ → 圆心 x = 10+180 = 190，填充宽 190。
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(fills(&enc.finish()).contains(&(0, 20, 190, 4, ACCENT)), "满值填充宽 = 圆心 190");
    // 布局零扰动（value 纯绘面——几何不变）。
    assert_eq!(ui.bounds(sld).unwrap(), RectI::new(0, 0, 200, 44));
}

// ---------- ⑤ 命令面：set_props 四 kind 生效 + 拒面 ----------

#[test]
fn set_props_updates_all_four_and_rejects_bad_values() {
    let (ui, btn, tog, sld, prg) = widget_scene();
    let mut ui = ui;
    ui.drain_damage();
    ui.wake(0);
    let _ = paint_full(&mut ui); // 结账基线（damage 清面）

    // —— Button：label 变 → 内在宽变（80 = 6×8+32）——
    ui.set_props(
        btn,
        Props::Button(ButtonProps {
            label: TextBuf::new(b"Cancel").unwrap(),
            style: ButtonStyle::Filled,
            on_tap: 7,
        }),
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(btn).unwrap(), RectI::new(0, 0, 80, 48), "标签 6 字节 → 80 宽");

    // —— Toggle：checked 变 → 轨色换（纯绘面，无重排）——
    ui.set_props(tog, Props::Toggle(ToggleProps { checked: true, on_change: 9 })).unwrap();
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(fills(&enc.finish()).contains(&(10, 48, 32, 32, ACCENT)), "checked → ACCENT 轨");

    // —— Slider：value 1000 → 填充满行程 ——
    ui.set_props(
        sld,
        Props::Slider(SliderProps { value: 1000, min: 0, max: 1000, on_change: 11 }),
    )
    .unwrap();
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(fills(&enc.finish()).contains(&(0, 100, 190, 4, ACCENT)), "1000‰ → 填充宽 190");

    // —— Progress：value 900 → 填充 180 ——
    ui.set_props(prg, Props::Progress(ProgressProps { value: 900, style: ProgressStyle::Linear }))
        .unwrap();
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 16];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(fills(&enc.finish()).contains(&(0, 124, 180, 8, ACCENT)), "900‰ → 填充宽 180");

    // —— 同值 set_props：几何等价 → 零重测（双面分录——bench_s7 同形）：
    // A 面 props_geom_equal 四臂精确判定 → measured==0；B 面（s7）
    // mask::props_diff 未随本批扩表——组件对落保守失配臂（GEOM ∪
    // PAINT 全置）→ 重排路径 measured≥1（正确但保守，扩表面挂 S7
    // 后续刀；本文件不改 mask.rs——约束面）。
    ui.layout(); // 先结账
    ui.set_props(prg, Props::Progress(ProgressProps { value: 900, style: ProgressStyle::Linear }))
        .unwrap();
    let st = ui.layout();
    #[cfg(not(feature = "s7"))]
    assert_eq!(st.measured, 0, "纯绘值变零重测（Measure 保留）");
    #[cfg(feature = "s7")]
    assert!(st.measured >= 1, "s7 面保守臂重排（props_diff 未扩表——文档化）");

    // —— 拒面：permille 域 / min>max / kind 失配（add + set_props 两口）——
    assert_eq!(
        ui.set_props(sld, Props::Slider(SliderProps { value: 1001, min: 0, max: 1000, on_change: 11 })),
        Err(UiError::NodeBad),
        "Slider value > 1000 拒"
    );
    assert_eq!(
        ui.set_props(sld, Props::Slider(SliderProps { value: 0, min: 10, max: 5, on_change: 11 })),
        Err(UiError::NodeBad),
        "Slider min > max 拒"
    );
    assert_eq!(
        ui.set_props(prg, Props::Progress(ProgressProps { value: 1001, style: ProgressStyle::Linear })),
        Err(UiError::NodeBad),
        "Progress value > 1000 拒"
    );
    assert_eq!(
        ui.set_props(
            btn,
            Props::Rect(RectProps { bg: Color { r: 9, g: 9, b: 9 }, w: None, h: None, radius: 0 })
        ),
        Err(UiError::NodeBad),
        "kind 失配拒（Rect props 塞 Button 槽）"
    );
    let root = ui.root();
    assert_eq!(
        ui.add(
            root,
            ViewSpec {
                kind: ViewKind::Slider,
                key: 0,
                props: Props::Slider(SliderProps { value: 1001, min: 0, max: 1000, on_change: 0 }),
            },
        ),
        Err(UiError::NodeBad),
        "add 面 permille 域同律拒"
    );
    // 未开口组件哨位退役（批 F 收口）：Scroll 批 A、Select 批 C、
    // Input/Edit/Canvas 批 D、Refresh/SwipeItem/DatePick/TimePick 批 E、
    // Gauge/Qr/DragReorder 批 F 已全数开口——41 kind 无「未开口防半落」
    // 残余（三新件开口/拒面钉板见 widgets7.rs）。
}
