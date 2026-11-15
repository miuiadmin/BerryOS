//! 波2 批 D——Input/Edit/Canvas 接线钉板（焦点/光标/placeholder/焦点环）：
//!
//! - **布局**：Input 200×48、Edit 200×96、Canvas 200×200（内在单源）。
//! - **渲染**：无焦灰框 + placeholder 弱化灰；置焦 ACCENT 环 + caret
//!   竖线；内容前景（Password 掩码 `*`）；Canvas 浅底面。
//! - **交互**：Click Input → set_focus（焦点环/caret 随焦）；再击幂等；
//!   清焦点还原灰框。
//! - **内容面**：set_input_text 侧表 upsert + max_len 执法 + kind 拒 +
//!   input_text 读；渲染随内容（caret 位置随文本尾）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, DisplayList, GlyphPos};
use berry_guiproto::{PHASE_DOWN, PHASE_UP};
use berry_ui::desc::{EditProps, InputKind, InputProps, Props};
use berry_ui::hit::PtrEvent;
use berry_ui::paint::{
    CANVAS_BG, INPUT_BG, INPUT_BORDER, INPUT_FOCUS_RING, INPUT_PLACEHOLDER, SELECT_FG,
};
use berry_ui::{Ui, UiError, ViewKind, ViewSpec};

fn paint_full(ui: &mut Ui<64>) -> DisplayList<'static> {
    ui.drain_damage();
    ui.wake(0);
    let scr: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; 32].into_boxed_slice());
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(scr, &mut enc);
    assert!(rep.cmds > 0);
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

fn input_scene() -> (Ui<64>, berry_ui::NodeId) {
    let mut ui = Ui::<64>::new(320, 140);
    let root = ui.root();
    let inp = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Input,
                key: 0,
                props: Props::Input(InputProps {
                    placeholder: berry_ui::TextBuf::new(b"Type here").unwrap(),
                    max_len: 16,
                    input_kind: InputKind::Text,
                }),
            },
        )
        .unwrap();
    ui.layout();
    (ui, inp)
}

// ---------- ① Input 布局 + 无焦/置焦渲染 ----------

#[test]
fn input_focus_ring_placeholder_and_caret() {
    let (mut ui, inp) = input_scene();
    assert_eq!(ui.bounds(inp).unwrap(), berry_ui::damage::RectI::new(0, 0, 200, 48));
    // 无焦：灰框 + placeholder。
    let dl = paint_full(&mut ui);
    let f = fills(&dl);
    assert!(f.contains(&(0, 0, 200, 2, INPUT_BORDER)), "无焦顶边灰：{f:?}");
    assert!(f.contains(&(2, 2, 196, 44, INPUT_BG)));
    let g = glyphs(&dl);
    assert!(
        g.iter().any(|&(c, _, _)| c == b'T'),
        "placeholder 前景：{g:?}"
    );
    // 置焦：ACCENT 环 + caret。
    ui.set_focus(Some(inp)).unwrap();
    let dl = paint_full(&mut ui);
    let f = fills(&dl);
    assert!(f.contains(&(0, 0, 200, 2, INPUT_FOCUS_RING)), "焦点环顶：{f:?}");
    assert!(f.contains(&(6, 14, 2, 16, INPUT_FOCUS_RING)), "caret 竖线：{f:?}");
    // placeholder 让位 caret 同帧（空内容 placeholder 仍显——视觉共存 v1）。
    ui.set_focus(None).unwrap();
    let dl = paint_full(&mut ui);
    assert!(fills(&dl).contains(&(0, 0, 200, 2, INPUT_BORDER)), "清焦还原灰框");
}

// ---------- ② 内容面：set_input_text + 渲染 + 掩码 ----------

#[test]
fn input_text_upsert_mask_and_limits() {
    let (mut ui, inp) = input_scene();
    // 写内容 + 置焦 → 渲染前景 + caret 随尾（caret 仅焦点态画）。
    ui.set_focus(Some(inp)).unwrap();
    ui.set_input_text(inp, b"hi").unwrap();
    let dl = paint_full(&mut ui);
    let g = glyphs(&dl);
    assert!(g.contains(&(b'h', 92, 16)) && g.contains(&(b'i', 100, 16)), "内容前景（区带居中）：{g:?}");
    let f = fills(&dl);
    assert!(f.contains(&(22, 14, 2, 16, INPUT_FOCUS_RING)), "caret 随文本尾：{f:?}");
    // upsert 覆写。
    ui.set_input_text(inp, b"hey").unwrap();
    assert_eq!(ui.input_text(inp), Some(&b"hey"[..]));
    // max_len 执法（16 帽）。
    assert_eq!(
        ui.set_input_text(inp, b"aaaaaaaaaaaaaaaaa"),
        Err(UiError::NodeBad),
        "17 字节 > max_len 16 拒"
    );
    // kind 拒（对 Canvas 节点写文本）。
    let root2 = ui.root();
    let cv = ui
        .add(
            root2,
            ViewSpec {
                kind: ViewKind::Canvas,
                key: 0,
                props: Props::Canvas(berry_ui::desc::CanvasProps { paint_ops: 3 }),
            },
        )
        .unwrap();
    assert_eq!(ui.set_input_text(cv, b"x"), Err(UiError::NodeBad), "非输入 kind 拒");
}

// ---------- ③ Password 掩码 + Edit/Canvas 布局渲染 ----------

#[test]
fn password_mask_edit_canvas_layout_paint() {
    let mut ui = Ui::<64>::new(320, 400);
    let root = ui.root();
    let pw = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Input,
                key: 0,
                props: Props::Input(InputProps {
                    placeholder: berry_ui::TextBuf::new(b"pw").unwrap(),
                    max_len: 8,
                    input_kind: InputKind::Password,
                }),
            },
        )
        .unwrap();
    let ed = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Edit,
                key: 0,
                props: Props::Edit(EditProps {
                    placeholder: berry_ui::TextBuf::new(b"notes").unwrap(),
                    max_len: 64,
                    max_lines: 3,
                    readonly: false,
                }),
            },
        )
        .unwrap();
    let cv = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Canvas,
                key: 0,
                props: Props::Canvas(berry_ui::desc::CanvasProps { paint_ops: 5 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(pw).unwrap().w, 200);
    // Root 直子 = 全幅叠放（`_` 臂 z 序）——三件同原点叠放。
    assert_eq!(ui.bounds(ed).unwrap(), berry_ui::damage::RectI::new(0, 0, 200, 96));
    assert_eq!(ui.bounds(cv).unwrap(), berry_ui::damage::RectI::new(0, 0, 200, 200));
    // Password 掩码：内容 "abc" → 三 '*'。
    ui.set_input_text(pw, b"abc").unwrap();
    let dl = paint_full(&mut ui);
    let g = glyphs(&dl);
    assert!(g.contains(&(b'*', 88, 16)) && g.contains(&(b'*', 96, 16)) && g.contains(&(b'*', 104, 16)), "掩码三星（居中）：{g:?}");
    assert!(!g.iter().any(|&(c, _, _)| c == b'a' && c != b'*'), "明文不显");
    // Canvas 底面。
    assert!(fills(&dl).contains(&(0, 0, 200, 200, CANVAS_BG)), "Canvas 浅底（叠放 z 序 = 后插者盖上——Canvas 最后插入在最顶）");
    // Edit placeholder。
    assert!(glyphs(&dl).iter().any(|&(c, _, _)| c == b'n'), "Edit placeholder 'notes'");
}

// ---------- ④ Click 置焦交互 ----------

#[test]
fn input_click_sets_focus() {
    let (mut ui, inp) = input_scene();
    ui.drain_damage();
    ui.wake(0);
    assert_eq!(ui.focus(), None);
    pump_one(&mut ui, PHASE_DOWN, 100, 24, 1_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 24, 2_000_000);
    assert_eq!(ui.focus(), Some(inp), "Click Input → 置焦");
    // 再击幂等。
    pump_one(&mut ui, PHASE_DOWN, 100, 24, 3_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 24, 4_000_000);
    assert_eq!(ui.focus(), Some(inp));
    // 焦点非输入 kind 拒。
    let root3 = ui.root();
    let btn = ui
        .add(
            root3,
            ViewSpec {
                kind: ViewKind::Button,
                key: 0,
                props: Props::Button(berry_ui::desc::ButtonProps {
                    label: berry_ui::TextBuf::new(b"OK").unwrap(),
                    style: berry_ui::desc::ButtonStyle::Filled,
                    on_tap: 0,
                }),
            },
        )
        .unwrap();
    assert_eq!(ui.set_focus(Some(btn)), Err(UiError::NodeBad), "Button 置焦拒");
}
