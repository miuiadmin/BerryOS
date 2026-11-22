//! 总攻批 2 双击/长按 referee 语义钉板（P2 缺陷 4/5——hit.rs 时序判据）。
//!
//! - 双击窗（300ms，含界）：同节点两次 Click 的 UP 间隔 ≤ 窗 → 二击 UP
//!   产 Release+Click+DoubleClick（Click 不抑制——add-only）；出窗仅
//!   Release+Click；异节点不成对；成对清锚（三连击 = 新窗首击）。
//! - 长按阈（450ms，严格超）：DOWN 后 slop 内静止——静止期有事件流
//!   （微抖 MOVE）随流早发 LongPress；零流由 UP 收尾兜底（Release+
//!   LongPress，无 Click）；长按消费后的 UP 仅 Release；越 slop 拖拽
//!   胜出（长按语义失效）；空白按压全程零 Msg 不变。
//!
//! 确定性：pump_input(0) 使重采样惰性（target=0 < 首采样 ts≥1 →
//! None——坐标原样透传），断言全值级；阈界两侧以 ±1ns 钉（450_000_000
//! 恰为阈下、450_000_001 为阈上；双击窗 300_000_000 恰为窗内）。

use berry_ui::desc::{ColProps, Msg, Props, RectProps, ViewSpec};
use berry_ui::{Ui, ViewKind};
use berry_gfx::cmd::Color;
use berry_guiproto::{PHASE_DOWN, PHASE_MOVE, PHASE_UP};

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

fn rect_spec(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(w), h: Some(h), radius: 0 }),
    }
}

/// 两叶分区树（tests/input.rs 同形）：A(0,0,30,30) + B(0,30,30,30)。
fn two_pane(ui: &mut Ui<16>) -> (berry_ui::NodeId, berry_ui::NodeId) {
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: None,
                    h: None,
                    cross: berry_ui::layout::CrossAlign::Stretch,
                }),
            },
        )
        .unwrap();
    let a = ui.add(col, rect_spec(30, 30)).unwrap();
    let b = ui.add(col, rect_spec(30, 30)).unwrap();
    ui.layout();
    ui.drain_damage();
    (a, b)
}

/// 单事件 pump（expected=0 → 重采样惰性——坐标透传）。
fn pump_one(ui: &mut Ui<16>, phase: u8, x: u32, y: u32, ts: u64) -> Vec<Msg> {
    let ev = berry_ui::hit::PtrEvent::from_wire(phase, 0, x, y, ts, 0, 0);
    ui.push_pointer(ev).unwrap();
    let out = ui.pump_input(0);
    out.iter().copied().collect()
}

/// 完整单击（DOWN+UP 两拍）——返回 UP 拍 Msg（Click 面断言点）。
fn tap_up(ui: &mut Ui<16>, x: u32, y: u32, down_ts: u64, up_ts: u64) -> Vec<Msg> {
    pump_one(ui, PHASE_DOWN, x, y, down_ts);
    pump_one(ui, PHASE_UP, x, y, up_ts)
}

/// 双击窗值（与 hit.rs DOUBLE_TAP_WINDOW_NS 单源语义——值级钉板）。
const WINDOW_NS: u64 = 300_000_000;
/// 长按阈值（与 hit.rs LONG_PRESS_NS 单源语义——值级钉板）。
const LONG_NS: u64 = 450_000_000;

// ---------- 双击窗两分支 ----------

/// 窗内（含界 300ms 恰达）：二击 UP = Release+Click+DoubleClick；成对清
/// 锚——三连击第三击无 DoubleClick（新窗首击），第四击重新成对。
#[test]
fn double_click_within_window_inclusive() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let m = 1_000_000u64;

    // 首击：Release+Click（DoubleClick 不发——窗锚始建）。
    assert_eq!(
        tap_up(&mut ui, 10, 10, 1 * m, 2 * m),
        vec![Msg::Release { node: a, x: 10, y: 10 }, Msg::Click { node: a }]
    );
    // 二击：UP 间隔恰 = 窗值 300ms（含界）→ Click 后追加 DoubleClick。
    let up2 = 2 * m + WINDOW_NS;
    assert_eq!(
        tap_up(&mut ui, 10, 10, up2 - m, up2),
        vec![
            Msg::Release { node: a, x: 10, y: 10 },
            Msg::Click { node: a },
            Msg::DoubleClick { node: a },
        ],
        "UP→UP 间隔恰 300ms = 窗内（含界）"
    );
    // 三击（成对已清锚）：仅 Release+Click——三连击不连判。
    assert_eq!(
        tap_up(&mut ui, 10, 10, 303 * m, 304 * m),
        vec![Msg::Release { node: a, x: 10, y: 10 }, Msg::Click { node: a }],
        "成对清锚：第三击 = 新窗首击"
    );
    // 四击：与三击成新对 → DoubleClick 复现。
    assert_eq!(
        tap_up(&mut ui, 10, 10, 305 * m, 306 * m),
        vec![
            Msg::Release { node: a, x: 10, y: 10 },
            Msg::Click { node: a },
            Msg::DoubleClick { node: a },
        ],
        "新窗重新成对"
    );
}

/// 窗外（300ms+1ns）：仅 Release+Click——双击不成立。
#[test]
fn double_click_outside_window() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let m = 1_000_000u64;

    assert_eq!(
        tap_up(&mut ui, 10, 10, 1 * m, 2 * m),
        vec![Msg::Release { node: a, x: 10, y: 10 }, Msg::Click { node: a }]
    );
    // 二击 UP = 首击 UP + 300ms + 1ns → 出窗。
    let up2 = 2 * m + WINDOW_NS + 1;
    assert_eq!(
        tap_up(&mut ui, 10, 10, up2 - m, up2),
        vec![Msg::Release { node: a, x: 10, y: 10 }, Msg::Click { node: a }],
        "UP→UP 间隔 300ms+1ns = 窗外"
    );
}

/// 异节点不成对：A 击后速击 B 无 DoubleClick；窗锚随 B 重建，B 自身可
/// 再成对（锚语义 = 上一 Click 的节点）。
#[test]
fn double_click_requires_same_node() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, b) = two_pane(&mut ui);
    let m = 1_000_000u64;

    assert_eq!(
        tap_up(&mut ui, 10, 10, 1 * m, 2 * m),
        vec![Msg::Release { node: a, x: 10, y: 10 }, Msg::Click { node: a }]
    );
    // A 后 2ms 击 B（异节点）：不成对。
    assert_eq!(
        tap_up(&mut ui, 10, 40, 3 * m, 4 * m),
        vec![Msg::Release { node: b, x: 10, y: 40 }, Msg::Click { node: b }],
        "异节点快速两击不产 DoubleClick"
    );
    // B 再速击 B：与上一 B Click（UP@4M）间隔 2ms → 成对。
    assert_eq!(
        tap_up(&mut ui, 10, 40, 5 * m, 6 * m),
        vec![
            Msg::Release { node: b, x: 10, y: 40 },
            Msg::Click { node: b },
            Msg::DoubleClick { node: b },
        ],
        "窗锚随上一 Click 重建——B 自身成对"
    );
}

// ---------- 长按阈两侧 ----------

/// 早出面（事件流驱动）：slop 内微抖 MOVE 越阈即发 LongPress——恰 450ms
/// 阈下零产出、+1ns 阈上即发；已发后 UP 只产 Release（长按已消费）。
#[test]
fn long_press_mid_hold_move_stream_threshold_sides() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let m = 1_000_000u64;

    // DOWN → Press 照产。
    assert_eq!(
        pump_one(&mut ui, PHASE_DOWN, 10, 10, 1 * m),
        vec![Msg::Press { node: a, x: 10, y: 10 }]
    );
    // 静止微抖 MOVE @ DOWN+450ms 恰达（阈下——严格超阈语义）→ 零产出。
    assert_eq!(
        pump_one(&mut ui, PHASE_MOVE, 12, 10, 1 * m + LONG_NS),
        vec![],
        "slop 内静止恰 450ms（dx=2 未出手势）阈下不发"
    );
    // +1ns 越阈 → LongPress 随流早发。
    assert_eq!(
        pump_one(&mut ui, PHASE_MOVE, 12, 10, 1 * m + LONG_NS + 1),
        vec![Msg::LongPress { node: a }],
        "静止超 450ms+1ns 随事件流即发"
    );
    // 长按已消费：UP 只产 Release（不补发 LongPress、不再产 Click）。
    assert_eq!(
        pump_one(&mut ui, PHASE_UP, 12, 10, 600 * m),
        vec![Msg::Release { node: a, x: 12, y: 10 }],
        "长按消费后的 UP：仅 Release 无 Click"
    );
}

/// 兜底面（零事件流）：静止保持到 UP——held 恰 450ms 阈下仍 Click；
/// +1ns 阈上 UP = Release+LongPress（无 Click）。
#[test]
fn long_press_backstop_at_up_threshold_sides() {
    let m = 1_000_000u64;

    // 阈下：UP@DOWN+450ms 恰达 → 原单击链不变。
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    pump_one(&mut ui, PHASE_DOWN, 10, 10, 1 * m);
    assert_eq!(
        pump_one(&mut ui, PHASE_UP, 10, 10, 1 * m + LONG_NS),
        vec![Msg::Release { node: a, x: 10, y: 10 }, Msg::Click { node: a }],
        "held 恰 450ms = 阈下（严格超阈）——单击链不变"
    );

    // 阈上：UP@DOWN+450ms+1ns → Release+LongPress 兜底，无 Click。
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    pump_one(&mut ui, PHASE_DOWN, 10, 10, 1 * m);
    assert_eq!(
        pump_one(&mut ui, PHASE_UP, 10, 10, 1 * m + LONG_NS + 1),
        vec![Msg::Release { node: a, x: 10, y: 10 }, Msg::LongPress { node: a }],
        "零流静止 held 450ms+1ns：UP 兜底产 LongPress 抑 Click"
    );
}

/// 越 slop 拖拽胜出：超阈同一拍先走 slop 判据——DragStart 无 LongPress；
/// 拖后 UP 产 DragEnd（长按语义整体失效）。
#[test]
fn drag_beats_long_press() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _) = two_pane(&mut ui);
    let m = 1_000_000u64;

    pump_one(&mut ui, PHASE_DOWN, 10, 10, 1 * m);
    // 超 450ms 后一次大幅 MOVE（dx=15 越圈）→ slop 先判：DragStart。
    assert_eq!(
        pump_one(&mut ui, PHASE_MOVE, 25, 10, 1 * m + LONG_NS + 1),
        vec![Msg::DragStart { node: a, axis: berry_ui::desc::Axis::X, x: 25, y: 10 }],
        "越圈 MOVE 拖拽胜出——不发 LongPress"
    );
    assert_eq!(
        pump_one(&mut ui, PHASE_UP, 25, 10, 600 * m),
        vec![Msg::DragEnd { node: a, vx: 0, vy: 0 }],
        "拖后 UP = DragEnd（无 Click/LongPress 面）"
    );
}

/// 空白按压不产长按：全程零 Msg（原语义不动）。
#[test]
fn long_press_blank_area_silent() {
    let mut ui = Ui::<16>::new(100, 100);
    let _ = two_pane(&mut ui);
    let m = 1_000_000u64;

    assert_eq!(pump_one(&mut ui, PHASE_DOWN, 90, 90, 1 * m), vec![]);
    assert_eq!(pump_one(&mut ui, PHASE_MOVE, 92, 90, 1 * m + LONG_NS + 1), vec![]);
    assert_eq!(pump_one(&mut ui, PHASE_UP, 95, 90, 600 * m), vec![]);
}
