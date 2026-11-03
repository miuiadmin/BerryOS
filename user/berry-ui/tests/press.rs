//! GUI-6 D18 press_map 钉板（§11 项 13）：Press 换底同帧生效（damage
//! 含 visual 界 + paint 载荷 PRESSED_BG）→ Release 还原；Text bg=None
//! 的还原 = 移除 bg；二次 Press 不覆写 saved_bg；trigger/visual 分置；
//! 帽 16 超 = NodeFull loud（D30 容量族）；死节点/非 bg 承载面先验拒 +
//! 整表替换语义。
//!
//! 确定性：惰性 pump（expected=0 → 坐标透传）+ 定值时刻，全值级断言。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos};
use berry_gfx::font::FontSize;
use berry_guiproto::{PHASE_DOWN, PHASE_UP};
use berry_ui::desc::{
    ColProps, Msg, NodeId, Props, RectProps, TextBuf, TextProps, TextStyle, ViewSpec,
};
use berry_ui::{Ui, UiError, ViewKind};

const C_ORIG: Color = Color { r: 1, g: 2, b: 3 };
const C_PRESSED: Color = Color { r: 0x40, g: 0x40, b: 0x40 }; // PRESSED_BG 0x40_40_40

fn rect_spec(c: Color, w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: c, w: Some(w), h: Some(h) }),
    }
}

/// 单事件 pump（expected=0 → 重采样惰性——坐标透传）。
fn pump_one<const N: usize>(
    ui: &mut Ui<N>,
    phase: u8,
    pid: u16,
    x: u32,
    y: u32,
    ts: u64,
) -> Vec<Msg> {
    let ev = berry_ui::hit::PtrEvent::from_wire(phase, pid, x, y, ts, 0, 0);
    ui.push_pointer(ev).unwrap();
    ui.pump_input(0).iter().copied().collect()
}

/// 单 Rect 树：Root → A(30×30, C_ORIG) @ (0,0)。
fn one_rect(ui: &mut Ui<16>) -> NodeId {
    let root = ui.root();
    let a = ui.add(root, rect_spec(C_ORIG, 30, 30)).unwrap();
    ui.layout();
    ui.drain_damage();
    a
}

/// render_frame 后提取唯一 FillRect 颜色 + 帧产 damage（damage 面 =
/// visual 界——恰一条绘制 + Present）。返回 (色, damage)。
fn sole_fill_color(ui: &mut Ui<16>, now_ns: u64) -> (Color, berry_ui::damage::DamageSet) {
    let mut enc = CommandEncoder::new();
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    let out = ui.render_frame(now_ns, &mut scratch, &mut enc);
    assert!(out.drew, "换底帧必提交");
    let dl = enc.finish();
    let mut found: Option<Color> = None;
    for c in dl.iter() {
        if let Cmd::FillRect { paint, .. } = c {
            assert!(found.is_none(), "恰一条 FillRect（damage 面 = visual 界）");
            found = Some(paint.color);
        }
    }
    (found.expect("至少一条 FillRect"), out.damage)
}

#[test]
fn press_swaps_bg_same_frame_and_release_restores() {
    let mut ui = Ui::<16>::new(100, 100);
    let a = one_rect(&mut ui);
    ui.set_press_map(&[(a, a)]).unwrap();

    // DOWN → Press + **同帧**换底：帧产 damage 含 visual 界 (0,0,30,30)
    assert_eq!(
        pump_one(&mut ui, PHASE_DOWN, 0, 10, 10, 1),
        vec![Msg::Press { node: a, x: 10, y: 10 }]
    );
    let (c, d) = sole_fill_color(&mut ui, 1000);
    assert_eq!(c, C_PRESSED, "Press 即换底（paint 载荷 PRESSED_BG）");
    assert_eq!(d.count, 1, "bg 换 = paint-only set_props 同界重绘");
    assert_eq!(d.rects[0], berry_guiproto::Rect { x: 0, y: 0, w: 30, h: 30 });

    // UP → Release+Click + 还原 saved_bg = C_ORIG
    assert_eq!(
        pump_one(&mut ui, PHASE_UP, 0, 12, 10, 2),
        vec![Msg::Release { node: a, x: 12, y: 10 }, Msg::Click { node: a }]
    );
    let (c, _) = sole_fill_color(&mut ui, 2000);
    assert_eq!(c, C_ORIG, "Release 还原原底色");
}

#[test]
fn text_none_bg_press_sets_and_release_removes() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let t = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Text,
                key: 0,
                props: Props::Text(TextProps {
                    style: TextStyle {
                        fg: Color { r: 9, g: 9, b: 9 },
                        bg: None,
                        scale: FontSize::S8x16,
                    },
                    text: TextBuf::new(b"AB").unwrap(), // 16×16 @ (0,0)
                }),
            },
        )
        .unwrap();
    ui.layout();
    ui.drain_damage();
    ui.set_press_map(&[(t, t)]).unwrap();

    let run_bg = |ui: &mut Ui<16>, now_ns: u64| -> Option<Color> {
        let mut enc = CommandEncoder::new();
        let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
        ui.render_frame(now_ns, &mut scratch, &mut enc);
        let dl = enc.finish();
        let mut bg: Option<Option<Color>> = None;
        for c in dl.iter() {
            if let Cmd::DrawGlyphRun { run } = c {
                assert!(bg.is_none(), "恰一条 GlyphRun");
                bg = Some(run.bg);
            }
        }
        bg.expect("至少一条 GlyphRun")
    };

    // DOWN → GlyphRun.bg = Some(PRESSED_BG)
    assert_eq!(
        pump_one(&mut ui, PHASE_DOWN, 0, 4, 4, 1),
        vec![Msg::Press { node: t, x: 4, y: 4 }]
    );
    assert_eq!(run_bg(&mut ui, 1000), Some(C_PRESSED), "None 底 Press → Some(PRESSED_BG)");

    // UP → 还原 = **移除** bg（saved_bg None → bg=None 透明底）
    pump_one(&mut ui, PHASE_UP, 0, 4, 4, 2);
    assert_eq!(run_bg(&mut ui, 2000), None, "Release 还原 = 移除 bg");
}

#[test]
fn second_press_keeps_saved_bg_chain() {
    // 双 DOWN（无 UP 防御面——同 pointer_id 复用槽重开手势）：二次
    // Press 不覆写 saved_bg（active 位防断还原链），UP 仍还原原底色。
    let mut ui = Ui::<16>::new(100, 100);
    let a = one_rect(&mut ui);
    ui.set_press_map(&[(a, a)]).unwrap();

    pump_one(&mut ui, PHASE_DOWN, 0, 10, 10, 1); // Press → 换底 + save C_ORIG
    let msgs = pump_one(&mut ui, PHASE_DOWN, 0, 20, 10, 2); // 二次 DOWN → 再 Press
    assert_eq!(msgs.len(), 1);
    assert!(matches!(msgs[0], Msg::Press { .. }));
    let (c, _) = sole_fill_color(&mut ui, 3000);
    assert_eq!(c, C_PRESSED);
    pump_one(&mut ui, PHASE_UP, 0, 20, 10, 3); // UP → 还原
    let (c, _) = sole_fill_color(&mut ui, 4000);
    assert_eq!(c, C_ORIG, "二次 Press 未断还原链");
}

#[test]
fn trigger_visual_distinct_nodes() {
    // trigger = 顶面小 Rect B(20×20)，visual = 底层大 Rect A(40×40)：
    // Press 命中 B（topmost）→ 换 A 的底（damage 面 = A 界 40×40）。
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let a = ui.add(root, rect_spec(C_ORIG, 40, 40)).unwrap();
    let b = ui.add(root, rect_spec(Color { r: 7, g: 8, b: 9 }, 20, 20)).unwrap();
    ui.layout();
    ui.drain_damage();
    ui.set_press_map(&[(b, a)]).unwrap();

    assert_eq!(
        pump_one(&mut ui, PHASE_DOWN, 0, 10, 10, 1),
        vec![Msg::Press { node: b, x: 10, y: 10 }],
        "命中 topmost B（trigger）"
    );
    let d = ui.drain_damage();
    assert_eq!(d.count, 1);
    assert_eq!(
        d.rects[0],
        berry_guiproto::Rect { x: 0, y: 0, w: 40, h: 40 },
        "damage = visual A 界（非 trigger B 界）"
    );

    // UP（Release 锚 B）→ A 还原
    pump_one(&mut ui, PHASE_UP, 0, 10, 10, 2);
    let mut enc = CommandEncoder::new();
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    ui.render_frame(3000, &mut scratch, &mut enc);
    let dl = enc.finish();
    // A(40×40, C_ORIG) + B(20×20, 原色不变——未换底)
    let mut fills = Vec::new();
    for c in dl.iter() {
        if let Cmd::FillRect { rect, paint } = c {
            fills.push(((rect.x, rect.y, rect.w, rect.h), paint.color));
        }
    }
    assert!(fills.contains(&((0, 0, 40, 40), C_ORIG)), "A 还原原色：{fills:?}");
    assert!(fills.contains(&((0, 0, 20, 20), Color { r: 7, g: 8, b: 9 })), "B 全程未换底");
}

#[test]
fn press_map_capacity_and_reject_faces() {
    let mut ui = Ui::<24>::new(100, 100);
    let root = ui.root();
    let mut ids = Vec::new();
    for _ in 0..17 {
        ids.push(ui.add(root, rect_spec(C_ORIG, 5, 5)).unwrap());
    }
    ui.layout();
    ui.drain_damage();

    // 帽 16：恰 16 对过；第 17 对 → NodeFull loud（D30——冻结 UiError
    // 族无侧表变体，NodeFull 兼任「Ui 容量域满」）
    let sixteen: Vec<(NodeId, NodeId)> = ids[..16].iter().map(|&id| (id, id)).collect();
    assert!(ui.set_press_map(&sixteen).is_ok());
    let seventeen: Vec<(NodeId, NodeId)> = ids.iter().map(|&id| (id, id)).collect();
    assert_eq!(ui.set_press_map(&seventeen), Err(UiError::NodeFull), "17 对 > PRESS_MAP_MAX=16");

    // 死节点（悬空 id）→ NodeBad（活性先验——trigger 与 visual 同律）
    ui.remove(ids[0]).unwrap();
    ui.drain_damage();
    assert_eq!(ui.set_press_map(&[(ids[0], ids[1])]), Err(UiError::NodeBad));

    // visual 非 bg 承载面（Col）→ NodeBad（Rect/Text 外拒）
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
    assert_eq!(ui.set_press_map(&[(ids[1], col)]), Err(UiError::NodeBad));

    // 整表替换：覆写后旧 trigger 不再反应（Press 无映射 → 零 damage）。
    // topmost @ (2,2) = ids[16]（最末 Rect；col 无子零高不参与命中）；
    // 新表只含 (ids[1], ids[1]) → no-op。
    assert!(ui.set_press_map(&[(ids[1], ids[1])]).is_ok());
    ui.layout();
    ui.drain_damage(); // 排空 add(col)/remove 的结构性 damage
    let msgs = pump_one(&mut ui, PHASE_DOWN, 0, 2, 2, 1);
    assert_eq!(msgs.len(), 1);
    assert_eq!(ui.drain_damage().count, 0, "未映射 trigger 的 Press 零 damage");
    let _ = msgs;
}
