//! 波2 批 C——Select 接线钉板（渲染/布局/交互/命令面四面）：
//!
//! - **布局**：固定内在 200×48（SELECT_W × BUTTON_H——下拉标准行）。
//! - **渲染**：FULL 帧 golden（白底框描边四边 + v 箭头三行 + 选中索引
//!   数字 1 起显示）。
//! - **交互**：Click 循环下一项（末项回 0）+ on_change 值级；空选择器
//!   零操作。
//! - **命令面**：set_props 开口（selected/option_count 生效——渲染随
//!   变）+ 拒面（selected ≥ option_count、add 面同律）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, DisplayList, GlyphPos};
use berry_guiproto::{PHASE_DOWN, PHASE_UP};
use berry_ui::desc::{Props, SelectProps};
use berry_ui::hit::PtrEvent;
use berry_ui::paint::{SELECT_BG, SELECT_BORDER, SELECT_FG};
use berry_ui::{Ui, UiError, ViewKind, ViewSpec};

fn paint_full(ui: &mut Ui<64>) -> DisplayList<'static> {
    ui.drain_damage();
    ui.wake(0);
    let scr: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; 16].into_boxed_slice());
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(scr, &mut enc);
    assert!(rep.cmds > 0, "渲染产出非空 op 集（DoD 门）");
    enc.finish()
}

fn fills(dl: &DisplayList) -> Vec<(i32, i32, i32, i32, Color)> {
    dl.iter()
        .filter_map(|c| match c {
            Cmd::FillRect { rect, paint } => Some((rect.x, rect.y, rect.w, rect.h, paint.color)),
            _ => None,
        })
        .collect()
}

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

fn pump_one(ui: &mut Ui<64>, phase: u8, x: u32, y: u32, ts: u64) {
    ui.push_pointer(PtrEvent::from_wire(phase, 1, x, y, ts, 0, 0)).unwrap();
    ui.pump_input(0);
}

fn select_scene(count: u32, selected: u32) -> (Ui<64>, berry_ui::NodeId) {
    let mut ui = Ui::<64>::new(320, 120);
    let root = ui.root();
    let sel = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Select,
                key: 0,
                props: Props::Select(SelectProps { option_count: count, selected, on_change: 6 }),
            },
        )
        .unwrap();
    ui.layout();
    (ui, sel)
}

// ---------- ① 布局 + 渲染 golden ----------

#[test]
fn select_layout_and_paint_golden() {
    let (mut ui, sel) = select_scene(3, 1);
    assert_eq!(ui.bounds(sel).unwrap(), berry_ui::damage::RectI::new(0, 0, 200, 48));
    let dl = paint_full(&mut ui);
    let f = fills(&dl);
    // 框描边（2px 四边）+ 白底。
    assert!(f.contains(&(0, 0, 200, 2, SELECT_BORDER)), "顶边：{f:?}");
    assert!(f.contains(&(0, 46, 200, 2, SELECT_BORDER)), "底边：{f:?}");
    assert!(f.contains(&(0, 2, 2, 44, SELECT_BORDER)), "左边：{f:?}");
    assert!(f.contains(&(198, 2, 2, 44, SELECT_BORDER)), "右边：{f:?}");
    assert!(f.contains(&(2, 2, 196, 44, SELECT_BG)), "白底：{f:?}");
    // v 箭头（右端 16px 带内三行）。
    assert!(f.contains(&(184, 20, 8, 2, SELECT_FG)), "箭头行 1：{f:?}");
    assert!(f.contains(&(186, 22, 4, 2, SELECT_FG)), "箭头行 2：{f:?}");
    assert!(f.contains(&(187, 24, 2, 2, SELECT_FG)), "箭头行 3：{f:?}");
    // 选中索引 1 → 显示 "2"（1 起；文本区带（b.x+4, w−20）内居中 →
    // x = 4+(180−8)/2 = 90。批 C 出生 golden 误钉左对齐 x=6——批 F
    // 收口套件全绿时修正为渲染真值）。
    let g = glyphs(&dl);
    assert!(g.contains(&(b'2', 90, 16)), "索引 1 → '2'：{g:?}");
}

// ---------- ② 点击循环 + 命令面 ----------

#[test]
fn select_click_cycles_and_set_props() {
    let (mut ui, sel) = select_scene(3, 0);
    ui.drain_damage();
    ui.wake(0);
    // 点击 → 下一项 1。
    pump_one(&mut ui, PHASE_DOWN, 100, 24, 1_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 24, 2_000_000);
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(6, 1)],
        "on_change(1)"
    );
    // 连击到末项回 0：1→2→0。
    pump_one(&mut ui, PHASE_DOWN, 100, 24, 3_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 24, 4_000_000);
    pump_one(&mut ui, PHASE_DOWN, 100, 24, 5_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 24, 6_000_000);
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| e.value).collect::<Vec<_>>(),
        vec![2, 0],
        "循环 2 → 0（末项回零）"
    );
    // set_props 直改 selected → 渲染随变（索引 2 → '3'）。
    ui.set_props(sel, Props::Select(SelectProps { option_count: 3, selected: 2, on_change: 6 }))
        .unwrap();
    let dl = paint_full(&mut ui);
    assert!(glyphs(&dl).contains(&(b'3', 90, 16)), "selected=2 → '3'（区带居中 x=90）");
    // 拒面：selected ≥ option_count。
    assert_eq!(
        ui.set_props(sel, Props::Select(SelectProps { option_count: 3, selected: 3, on_change: 6 })),
        Err(UiError::NodeBad),
        "selected 越域拒"
    );
    // add 面同律。
    let root = ui.root();
    assert_eq!(
        ui.add(
            root,
            ViewSpec {
                kind: ViewKind::Select,
                key: 0,
                props: Props::Select(SelectProps { option_count: 2, selected: 5, on_change: 0 })
            },
        ),
        Err(UiError::NodeBad),
        "add 面越域拒"
    );
    // 空选择器：Click 零操作。
    let (mut ui2, _) = select_scene(0, 0);
    ui2.drain_damage();
    ui2.wake(0);
    pump_one(&mut ui2, PHASE_DOWN, 100, 24, 7_000_000);
    pump_one(&mut ui2, PHASE_UP, 100, 24, 8_000_000);
    assert!(ui2.take_events().is_empty(), "空选择器点击零操作");
}
