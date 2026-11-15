//! 波2 批 A 组件五件钉板（Tabs/Sbar/Badge/Scroll/List——布局/渲染/
//! 交互/命令面四面接通）+ Button press 视觉：
//!
//! - **布局**：Tabs 内在 = 子聚合 + 48 条高、子面板 y 下移；Sbar 走向
//!   互转内在；Badge count 档（0 隐藏/圆点/数字宽）；Scroll content_h
//!   /List count×item_h；offset 上移消费。
//! - **渲染**：FULL 帧逐 Cmd golden（Tabs 底+指示条+分隔线；Sbar 轨+
//!   thumb 两走向；Badge 红底+数字字形）。
//! - **交互**：Tabs 点击切页 + on_switch 值级；Button press 暗化（press_
//!   map 登记 + PressedVis 查表——ACCENT→ACCENT_PRESSED）+ Release 还原。
//! - **命令面**：set_props 五 kind 开口 + 拒面（Tabs 零页/active 越页、
//!   Sbar thumb 越域/start>end、Scroll offset>content）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, DisplayList, GlyphPos};
use berry_guiproto::{PHASE_DOWN, PHASE_UP};
use berry_ui::desc::{
    BadgeProps, ColProps, ListProps, Orientation, Props, RectProps, ScrollProps, SbarProps,
    TabsProps, TextBuf,
};
use berry_ui::hit::PtrEvent;
use berry_ui::layout::CrossAlign;
use berry_ui::paint::{ACCENT, ACCENT_PRESSED, BADGE_BG, SBAR_THUMB, SBAR_TRACK, TABS_BG, TABS_INDICATOR};
use berry_ui::{NodeId, Ui, UiError, ViewKind, ViewSpec};

/// FULL 帧（widgets.rs 同形标准造法）。
fn paint_full(ui: &mut Ui<64>) -> DisplayList<'static> {
    ui.drain_damage();
    ui.wake(0);
    let scr: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; 32].into_boxed_slice());
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(scr, &mut enc);
    assert!(rep.cmds > 0, "渲染产出非空 op 集（DoD 门）");
    enc.finish()
}

/// FillRect 序列抽取。
fn fills(dl: &DisplayList) -> Vec<(i32, i32, i32, i32, Color)> {
    dl.iter()
        .filter_map(|c| match c {
            Cmd::FillRect { rect, paint } => Some((rect.x, rect.y, rect.w, rect.h, paint.color)),
            _ => None,
        })
        .collect()
}

/// GlyphRun 字形抽取。
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

/// 单事件 pump。
fn pump_one(ui: &mut Ui<64>, phase: u8, x: u32, y: u32, ts: u64) {
    let ev = PtrEvent::from_wire(phase, 1, x, y, ts, 0, 0);
    ui.push_pointer(ev).unwrap();
    ui.pump_input(0);
}

/// 场景：Root → Tabs(2 页 active 0) → 面板 Rect(200×100)。
fn tabs_scene() -> (Ui<64>, NodeId, NodeId) {
    let mut ui = Ui::<64>::new(320, 240);
    let root = ui.root();
    let tabs = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Tabs,
                key: 0,
                props: Props::Tabs(TabsProps { tab_count: 2, active: 0, on_switch: 5 }),
            },
        )
        .unwrap();
    ui.add(
        tabs,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps { bg: Color { r: 9, g: 9, b: 9 }, w: Some(200), h: Some(100) }),
        },
    )
    .unwrap();
    ui.layout();
    let panel = ui.bounds(tabs).unwrap();
    let _ = panel;
    (ui, tabs, tabs)
}

// ---------- ① Tabs 布局 + 渲染 golden ----------

#[test]
fn tabs_layout_and_paint_golden() {
    let (mut ui, tabs, _) = tabs_scene();
    // Tabs 内在：宽 = max(200, TABS_MIN_W 96) = 200；高 = 100 + 48 = 148。
    assert_eq!(ui.bounds(tabs).unwrap(), berry_ui::damage::RectI::new(0, 0, 200, 148));
    let dl = paint_full(&mut ui);
    let f = fills(&dl);
    // 页签条底（自身界）+ 面板 Rect（y=48 起）。
    assert!(f.contains(&(0, 0, 200, 148, TABS_BG)), "Tabs 底：{f:?}");
    assert!(f.contains(&(0, 48, 200, 100, Color { r: 9, g: 9, b: 9 })), "面板 y 下移 48：{f:?}");
    // active=0 段指示条：x=0 宽 100 厚 3 底缘。
    assert!(f.contains(&(0, 145, 100, 3, TABS_INDICATOR)), "指示条段 0：{f:?}");
    // 段间分隔线：x=100 竖线。
    assert!(f.contains(&(100, 0, 2, 148, berry_ui::paint::OUTLINE)), "分隔线：{f:?}");
}

// ---------- ② Tabs 点击切页 ----------

#[test]
fn tabs_click_switches_active_and_fires_on_switch() {
    let (mut ui, tabs, _) = tabs_scene();
    ui.drain_damage();
    ui.wake(0);
    // 击第二页段中央 (150, 20)：段宽 100 → tab=1。
    pump_one(&mut ui, PHASE_DOWN, 150, 20, 1_000_000);
    pump_one(&mut ui, PHASE_UP, 150, 20, 2_000_000);
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(5, 1)],
        "on_switch(1)"
    );
    // 指示条移到段 1：x=100。
    let dl = paint_full(&mut ui);
    assert!(
        fills(&dl).contains(&(100, 145, 100, 3, TABS_INDICATOR)),
        "指示条随 active 段"
    );
    // 同段再击：零操作。
    pump_one(&mut ui, PHASE_DOWN, 150, 20, 3_000_000);
    pump_one(&mut ui, PHASE_UP, 150, 20, 4_000_000);
    assert!(ui.take_events().is_empty(), "active 未变零派发");
}

// ---------- ③ Sbar 双走向渲染 ----------

#[test]
fn sbar_paint_both_orientations() {
    let mut ui = Ui::<64>::new(200, 260);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps { gap: 8, w: None, h: None, cross: CrossAlign::Start }),
            },
        )
        .unwrap();
    let v = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Sbar,
                key: 0,
                props: Props::Sbar(SbarProps {
                    orientation: Orientation::Vertical,
                    thumb_start: 250,
                    thumb_end: 750,
                }),
            },
        )
        .unwrap();
    let h = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Sbar,
                key: 0,
                props: Props::Sbar(SbarProps {
                    orientation: Orientation::Horizontal,
                    thumb_start: 0,
                    thumb_end: 500,
                }),
            },
        )
        .unwrap();
    ui.layout();
    // 内在互转：竖 4×200（w,h）、横 200×4。
    assert_eq!(ui.bounds(v).unwrap(), berry_ui::damage::RectI::new(0, 0, 4, 200));
    assert_eq!(ui.bounds(h).unwrap(), berry_ui::damage::RectI::new(0, 208, 200, 4));
    let dl = paint_full(&mut ui);
    let f = fills(&dl);
    // 竖 thumb：[25%,75%] → y 50..150 高 100。
    assert!(f.contains(&(0, 50, 4, 100, SBAR_THUMB)), "竖 thumb：{f:?}");
    // 横 thumb：[0,50%] → x 0..100 宽 100。
    assert!(f.contains(&(0, 208, 100, 4, SBAR_THUMB)), "横 thumb：{f:?}");
    assert!(f.contains(&(0, 0, 4, 200, SBAR_TRACK)));
}

// ---------- ④ Badge 三档渲染 ----------

#[test]
fn badge_paint_dot_count_and_overflow() {
    let mut ui = Ui::<64>::new(200, 200);
    let root = ui.root();
    let dot = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Badge,
                key: 0,
                props: Props::Badge(BadgeProps { count: 0, show_dot: true }),
            },
        )
        .unwrap();
    let two = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Badge,
                key: 0,
                props: Props::Badge(BadgeProps { count: 42, show_dot: false }),
            },
        )
        .unwrap();
    let over = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Badge,
                key: 0,
                props: Props::Badge(BadgeProps { count: 150, show_dot: false }),
            },
        )
        .unwrap();
    ui.layout();
    // 内在：dot 16×16；两位数 = 2×8+8 = 24 宽 ×16；99+ 三字 = 32 宽。
    assert_eq!(ui.bounds(dot).unwrap(), berry_ui::damage::RectI::new(0, 0, 16, 16));
    assert_eq!(ui.bounds(two).unwrap().w, 24);
    assert_eq!(ui.bounds(over).unwrap().w, 32);
    let dl = paint_full(&mut ui);
    let g: Vec<(u8, i32)> = glyphs(&dl).into_iter().map(|(c, x, _)| (c, x)).collect();
    assert!(g.contains(&(b'4', 4)) && g.contains(&(b'2', 12)), "42 双字形（居中偏移 4）：{g:?}");
    assert!(g.contains(&(b'9', 4)) && g.contains(&(b'9', 12)) && g.contains(&(b'+', 20)), "99+ 三字形（居中偏移 4）：{g:?}");
    let f = fills(&dl);
    assert!(f.contains(&(0, 5, 16, 6, BADGE_BG)), "圆点八角中片红底（oct3）：{f:?}");
}

// ---------- ⑤ Scroll/List 容器语义 ----------

#[test]
fn scroll_offset_and_list_intrinsic() {
    let mut ui = Ui::<64>::new(240, 300);
    let root = ui.root();
    let sc = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Scroll,
                key: 0,
                props: Props::Scroll(ScrollProps { content_height: 400, offset_y: 100 }),
            },
        )
        .unwrap();
    let item = ui
        .add(
            sc,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: Color { r: 5, g: 6, b: 7 }, w: Some(100), h: Some(400) }),
            },
        )
        .unwrap();
    ui.layout();
    // Scroll = 视口容器随父（Root 直子 → viewport 240×300；内在
    // (子宽 100, content_h 400) 供聚合语境）。
    assert_eq!(ui.bounds(sc).unwrap(), berry_ui::damage::RectI::new(0, 0, 240, 300));
    // 子整体上移 offset：绝对 y = -100。
    assert_eq!(ui.bounds(item).unwrap().y, -100, "子上移 offset_y");
    // List 内在 = count×item_h。
    let ls = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::List,
                key: 0,
                props: Props::List(ListProps { item_count: 5, item_height: 48 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(ls).unwrap().h, 300, "List 随父填充（Root 直子 = viewport 高；内在 240 供聚合）");
    // offset 变更纯绘面（无重排——同界 damage）。
    ui.set_props(
        sc,
        Props::Scroll(ScrollProps { content_height: 400, offset_y: 200 }),
    )
    .unwrap();
    let mut ui = ui;
    ui.layout();
    assert_eq!(ui.bounds(item).unwrap().y, -200, "offset 200 → 子 y=-200");
}

// ---------- ⑥ Button press 视觉：ACCENT → ACCENT_PRESSED ----------

#[test]
fn button_press_darkens_and_restores() {
    let mut ui = Ui::<64>::new(200, 120);
    let root = ui.root();
    let btn = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Button,
                key: 0,
                props: Props::Button(berry_ui::desc::ButtonProps {
                    label: TextBuf::new(b"OK").unwrap(),
                    style: berry_ui::desc::ButtonStyle::Filled,
                    on_tap: 3,
                }),
            },
        )
        .unwrap();
    ui.layout();
    // press_map 登记（trigger=visual=btn 自身）。
    ui.set_press_map(&[(btn, btn)]).unwrap();
    ui.drain_damage();
    ui.wake(0);
    // 常态：ACCENT 底。
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 32];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(fills(&enc.finish()).contains(&(0, 0, 64, 48, ACCENT)));
    // Press：暗化底。
    pump_one(&mut ui, PHASE_DOWN, 32, 24, 1_000_000);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 32];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(
        fills(&enc.finish()).contains(&(0, 0, 64, 48, ACCENT_PRESSED)),
        "Press 底暗化"
    );
    // Release：还原 ACCENT。
    pump_one(&mut ui, PHASE_UP, 32, 24, 2_000_000);
    let mut scr = [GlyphPos { ch: 0, x: 0, y: 0 }; 32];
    let mut enc = CommandEncoder::new();
    ui.paint(&mut scr, &mut enc);
    assert!(fills(&enc.finish()).contains(&(0, 0, 64, 48, ACCENT)), "Release 还原");
}

// ---------- ⑦ 命令面拒面 ----------

#[test]
fn set_props_reject_faces_batch_a() {
    let (mut ui, tabs, _) = tabs_scene();
    let root = ui.root();
    // Tabs 零页。
    assert_eq!(
        ui.set_props(tabs, Props::Tabs(TabsProps { tab_count: 0, active: 0, on_switch: 5 })),
        Err(UiError::NodeBad)
    );
    // Tabs active 越页。
    assert_eq!(
        ui.set_props(tabs, Props::Tabs(TabsProps { tab_count: 2, active: 2, on_switch: 5 })),
        Err(UiError::NodeBad)
    );
    // Sbar 越域 + start>end。
    let sbar = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Sbar,
                key: 0,
                props: Props::Sbar(SbarProps {
                    orientation: Orientation::Horizontal,
                    thumb_start: 0,
                    thumb_end: 1000,
                }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.set_props(
            sbar,
            Props::Sbar(SbarProps { orientation: Orientation::Horizontal, thumb_start: 1001, thumb_end: 1000 })
        ),
        Err(UiError::NodeBad),
        "start 越千分域"
    );
    assert_eq!(
        ui.set_props(
            sbar,
            Props::Sbar(SbarProps { orientation: Orientation::Horizontal, thumb_start: 700, thumb_end: 300 })
        ),
        Err(UiError::NodeBad),
        "start > end 拒"
    );
    // Scroll offset > content。
    let (sc, _) = (tabs, 0);
    let _ = sc;
    let scr2 = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Scroll,
                key: 0,
                props: Props::Scroll(ScrollProps { content_height: 100, offset_y: 0 }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.set_props(scr2, Props::Scroll(ScrollProps { content_height: 100, offset_y: 101 })),
        Err(UiError::NodeBad),
        "offset > content 拒"
    );
    // add 面同律。
    assert_eq!(
        ui.add(
            root,
            ViewSpec {
                kind: ViewKind::Tabs,
                key: 0,
                props: Props::Tabs(TabsProps { tab_count: 0, active: 0, on_switch: 0 })
            },
        ),
        Err(UiError::NodeBad),
        "add 面 Tabs 零页拒"
    );
}
