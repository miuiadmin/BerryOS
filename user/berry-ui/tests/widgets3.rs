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
//! - **第三批追加段（⑧..⑫）**：显示三件 golden（Badge 三档 / Sbar 双
//!   走向 / Tip 衬底）、Collapse 折叠三面守卫（摆位/渲染/命中）、
//!   Chip·Fab 交互值级、Card 双 fill 阴影、七 kind set_props 开口与
//!   拒面（Sbar 域 / Card elevation / kind 失配 / 叶父位）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, DisplayList, GlyphPos};
use berry_guiproto::{PHASE_DOWN, PHASE_UP};
use berry_ui::desc::{
    BadgeProps, ColProps, ListProps, Orientation, Props, RectProps, ScrollProps, SbarProps,
    TabsProps, TextBuf,
};
use berry_ui::hit::PtrEvent;
use berry_ui::layout::CrossAlign;
use berry_ui::theme::{ACCENT, ACCENT_PRESSED, BADGE_BG, SBAR_THUMB, SBAR_TRACK, TABS_BG, TABS_INDICATOR};
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
    assert!(f.contains(&(100, 0, 2, 148, berry_ui::theme::OUTLINE)), "分隔线：{f:?}");
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

// ---------- 第三批七件追加段（Badge/Sbar 已上 ③④；Tip/Collapse/Chip/Fab/Card——⑧..⑫） ----------

use berry_gfx::font::FontSize;
use berry_ui::desc::{
    CardProps, ChipProps, CollapseProps, FabProps, Msg, TextProps, TextStyle, TipPlacement,
    TipProps,
};
use berry_ui::theme::{CARD_BG, CARD_SHADOW, ON_ACCENT, ON_BADGE, ON_SURFACE, TIP_BG, TRACK_DIM};

/// 单事件 pump（Msg 面携出——不劫持断言用；pump_one 同形）。
fn pump_msgs(ui: &mut Ui<64>, phase: u8, x: u32, y: u32, ts: u64) -> Vec<Msg> {
    let ev = PtrEvent::from_wire(phase, 1, x, y, ts, 0, 0);
    ui.push_pointer(ev).unwrap();
    ui.pump_input(0).iter().copied().collect()
}

// ---------- ⑧ 显示三件：布局内在 + FULL 帧 golden ----------

#[test]
fn display_three_layout_and_paint_golden() {
    let mut ui = Ui::<64>::new(320, 480);
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
    let dot = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Badge,
                key: 0,
                props: Props::Badge(BadgeProps { count: 0, show_dot: true }),
            },
        )
        .unwrap();
    let five = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Badge,
                key: 0,
                props: Props::Badge(BadgeProps { count: 5, show_dot: false }),
            },
        )
        .unwrap();
    let over = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Badge,
                key: 0,
                props: Props::Badge(BadgeProps { count: 150, show_dot: false }),
            },
        )
        .unwrap();
    let sv = ui
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
    let sh = ui
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
    let tip = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Tip,
                key: 0,
                props: Props::Tip(TipProps {
                    text: TextBuf::new(b"Hi").unwrap(),
                    placement: TipPlacement::Top,
                }),
            },
        )
        .unwrap();
    ui.layout();
    // 内在值级：圆点 16×16；单位数 16 宽（1×8+8）；99+ 档 32 宽；
    // Sbar 竖 4×200 / 横 200×4；Tip 随文本 16×16。
    assert_eq!(ui.bounds(dot).unwrap(), berry_ui::damage::RectI::new(0, 0, 16, 16));
    assert_eq!(ui.bounds(five).unwrap(), berry_ui::damage::RectI::new(0, 16, 16, 16));
    assert_eq!(ui.bounds(over).unwrap(), berry_ui::damage::RectI::new(0, 32, 32, 16));
    assert_eq!(ui.bounds(sv).unwrap(), berry_ui::damage::RectI::new(0, 48, 4, 200));
    assert_eq!(ui.bounds(sh).unwrap(), berry_ui::damage::RectI::new(0, 248, 200, 4));
    assert_eq!(ui.bounds(tip).unwrap(), berry_ui::damage::RectI::new(0, 252, 16, 16));

    let dl = paint_full(&mut ui);
    // 填充序列逐值对拍（画家序 = 文档序）：圆点八角三片、两档红底、
    // 竖/横轨 + thumb permille 像素映射、Tip 实色衬底。
    assert_eq!(
        fills(&dl),
        vec![
            (4, 0, 8, 5, BADGE_BG),
            (0, 5, 16, 6, BADGE_BG),
            (4, 11, 8, 5, BADGE_BG),
            (0, 16, 16, 16, BADGE_BG),
            (0, 32, 32, 16, BADGE_BG),
            (0, 48, 4, 200, SBAR_TRACK),
            (0, 98, 4, 100, SBAR_THUMB),
            (0, 248, 200, 4, SBAR_TRACK),
            (0, 248, 100, 4, SBAR_THUMB),
            (0, 252, 16, 16, TIP_BG),
        ],
        "显示三件填充序列值级"
    );
    assert_eq!(
        glyphs(&dl),
        vec![
            (b'5', 4, 16),
            (b'9', 4, 32),
            (b'9', 12, 32),
            (b'+', 20, 32),
            (b'H', 0, 252),
            (b'i', 8, 252),
        ],
        "角标数字（fmt_u32）与 99+ 封顶 + Tip 文本居中"
    );
    // 前景值级：Badge 白字 / Tip 亮字（暗衬底）。
    for c in dl.iter() {
        if let Cmd::DrawGlyphRun { run } = c {
            if run.glyphs.first().is_some_and(|g| g.y == 252) {
                assert_eq!(run.fg, ON_ACCENT, "Tip 亮字");
            } else {
                assert_eq!(run.fg, ON_BADGE, "Badge 白字");
            }
        }
    }
    // 命令数 = 10 fill + 3 run + Present。
    assert_eq!(dl.iter().count(), 14);
}

// ---------- ⑨ Collapse：Click 翻转 + 子三面守卫（摆位/渲染/命中） ----------

#[test]
fn collapse_toggle_and_children_visibility() {
    let mut ui = Ui::<64>::new(200, 300);
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
    let cpse = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Collapse,
                key: 0,
                props: Props::Collapse(CollapseProps { expanded: true, on_toggle: 7 }),
            },
        )
        .unwrap();
    let inner = ui
        .add(
            cpse,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: Color { r: 1, g: 2, b: 3 },
                    w: Some(100),
                    h: Some(50),
                }),
            },
        )
        .unwrap();
    ui.layout();
    // 展开：header 32 + 子高 50；子在 header 下方。
    assert_eq!(ui.bounds(cpse).unwrap(), berry_ui::damage::RectI::new(0, 0, 100, 82));
    assert_eq!(ui.bounds(inner).unwrap(), berry_ui::damage::RectI::new(0, 32, 100, 50));
    assert_eq!(ui.node_at(50, 60), Some(inner), "展开态子可命中");
    assert_eq!(ui.node_at(10, 10), Some(cpse), "header 区命中容器");

    // 展开态 golden：header 带 + 朝下箭头三横带 + 子 Rect。
    let dl = paint_full(&mut ui);
    assert_eq!(
        fills(&dl),
        vec![
            (0, 0, 100, 32, TRACK_DIM),
            (8, 10, 12, 4, ON_SURFACE),
            (10, 14, 8, 4, ON_SURFACE),
            (12, 18, 4, 4, ON_SURFACE),
            (0, 32, 100, 50, Color { r: 1, g: 2, b: 3 }),
        ],
        "展开：带 + 朝下箭头 + 子"
    );

    // Click header → 翻转收起 + on_toggle(0)；Msg 面不劫持。
    let msgs = pump_msgs(&mut ui, PHASE_DOWN, 10, 10, 1_000_000);
    assert!(matches!(msgs.as_slice(), [Msg::Press { .. }]));
    let msgs = pump_msgs(&mut ui, PHASE_UP, 10, 10, 2_000_000);
    assert!(
        matches!(msgs.as_slice(), [Msg::Release { .. }, Msg::Click { node }] if *node == cpse),
        "Release+Click 原样交 app"
    );
    let ev = ui.take_events();
    assert_eq!(
        ev.iter().copied().collect::<Vec<_>>(),
        vec![berry_ui::WidgetEvent { node: cpse, event: 7, value: 0 }],
        "Click → 收起 + on_toggle(0)"
    );
    assert_eq!(ui.lost_widget_events(), 0);

    // 收起态：内在折叠 32；子不摆位（bounds 冻结）不渲染不命中。
    ui.layout();
    assert_eq!(ui.bounds(cpse).unwrap(), berry_ui::damage::RectI::new(0, 0, 100, 32));
    assert_eq!(ui.node_at(10, 10), Some(cpse), "header 仍可命中（再展开入口）");
    assert_eq!(ui.node_at(50, 60), None, "收起态子不命中（三面同守卫）");
    let dl = paint_full(&mut ui);
    assert_eq!(
        fills(&dl),
        vec![
            (0, 0, 100, 32, TRACK_DIM),
            (8, 10, 4, 12, ON_SURFACE),
            (12, 12, 4, 8, ON_SURFACE),
            (16, 14, 4, 4, ON_SURFACE),
        ],
        "收起：带 + 朝右箭头三竖带，子不发射"
    );

    // 再击展开：高还原 + 子回到画面。
    pump_one(&mut ui, PHASE_DOWN, 10, 10, 3_000_000);
    pump_one(&mut ui, PHASE_UP, 10, 10, 4_000_000);
    assert_eq!(
        ui.take_events().iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(7, 1)],
        "再击展开 on_toggle(1)"
    );
    ui.layout();
    assert_eq!(ui.bounds(cpse).unwrap(), berry_ui::damage::RectI::new(0, 0, 100, 82));
    assert_eq!(ui.node_at(50, 60), Some(inner), "展开态子恢复命中");
    let dl = paint_full(&mut ui);
    assert!(
        fills(&dl).contains(&(0, 32, 100, 50, Color { r: 1, g: 2, b: 3 })),
        "子恢复发射"
    );
}

// ---------- ⑩ Chip / Fab：Click 值级 + 视觉 ----------

#[test]
fn chip_and_fab_click_events_and_visuals() {
    // —— Chip：胶囊底 + 标签 + 选中态色变；Click 翻转 selected ——
    let mut ui = Ui::<64>::new(120, 80);
    let root = ui.root();
    let chip = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Chip,
                key: 0,
                props: Props::Chip(ChipProps {
                    label: TextBuf::new(b"Tag").unwrap(),
                    selected: false,
                    on_tap: 9,
                }),
            },
        )
        .unwrap();
    ui.layout();
    // 内在 = 标签 3×8 + 16 留白 ×32 高。
    assert_eq!(ui.bounds(chip).unwrap(), berry_ui::damage::RectI::new(0, 0, 40, 32));
    let dl = paint_full(&mut ui);
    assert_eq!(fills(&dl), vec![(0, 0, 40, 32, TRACK_DIM)], "未选浅底（v1 直角胶囊）");
    assert_eq!(glyphs(&dl), vec![(b'T', 8, 8), (b'a', 16, 8), (b'g', 24, 8)]);

    pump_one(&mut ui, PHASE_DOWN, 20, 16, 1_000_000);
    let msgs = pump_msgs(&mut ui, PHASE_UP, 20, 16, 2_000_000);
    assert!(matches!(msgs.as_slice(), [Msg::Release { .. }, Msg::Click { .. }]));
    assert_eq!(
        ui.take_events().iter().copied().collect::<Vec<_>>(),
        vec![berry_ui::WidgetEvent { node: chip, event: 9, value: 1 }],
        "Click → selected=1 + on_tap"
    );
    let dl = paint_full(&mut ui);
    assert_eq!(fills(&dl), vec![(0, 0, 40, 32, ACCENT)], "选中 ACCENT 底");

    pump_one(&mut ui, PHASE_DOWN, 20, 16, 3_000_000);
    pump_one(&mut ui, PHASE_UP, 20, 16, 4_000_000);
    assert_eq!(
        ui.take_events().iter().map(|e| (e.event, e.value)).collect::<Vec<_>>(),
        vec![(9, 0)],
        "再击翻回"
    );
    // 界外击不派发。
    pump_one(&mut ui, PHASE_DOWN, 100, 16, 5_000_000);
    pump_one(&mut ui, PHASE_UP, 100, 16, 6_000_000);
    assert!(ui.take_events().is_empty(), "界外击不派发");

    // —— Fab 纯圆形：八角 ACCENT 圆 + '+' 占位图标；Click 恒派发 ——
    let mut ui = Ui::<64>::new(120, 120);
    let root = ui.root();
    let fab = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Fab,
                key: 0,
                props: Props::Fab(FabProps {
                    icon: 5,
                    extended_label: TextBuf::new(b"").unwrap(),
                    on_tap: 11,
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(fab).unwrap(), berry_ui::damage::RectI::new(0, 0, 56, 56));
    let dl = paint_full(&mut ui);
    assert_eq!(
        fills(&dl),
        vec![(14, 0, 28, 18, ACCENT), (0, 18, 56, 20, ACCENT), (14, 38, 28, 18, ACCENT)],
        "八角圆底"
    );
    assert_eq!(glyphs(&dl), vec![(b'+', 24, 20)], "图标位文本占位居中");

    pump_one(&mut ui, PHASE_DOWN, 28, 28, 7_000_000);
    pump_one(&mut ui, PHASE_UP, 28, 28, 8_000_000);
    assert_eq!(
        ui.take_events().iter().copied().collect::<Vec<_>>(),
        vec![berry_ui::WidgetEvent { node: fab, event: 11, value: 0 }],
        "Click → on_tap(0)（Button 同形）"
    );
    assert_eq!(ui.bounds(fab).unwrap(), berry_ui::damage::RectI::new(0, 0, 56, 56), "无状态件界不变");

    // —— Fab 扩展形态：56 + 8 间隙 + 标签带 ——
    let mut ui = Ui::<64>::new(160, 120);
    let root = ui.root();
    let fab2 = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Fab,
                key: 0,
                props: Props::Fab(FabProps {
                    icon: 0,
                    extended_label: TextBuf::new(b"Go").unwrap(),
                    on_tap: 12,
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(fab2).unwrap(), berry_ui::damage::RectI::new(0, 0, 80, 56));
    let dl = paint_full(&mut ui);
    assert_eq!(
        glyphs(&dl),
        vec![(b'+', 24, 20), (b'G', 64, 20), (b'o', 72, 20)],
        "圆内图标 + 圆右扩展标签"
    );
}

// ---------- ⑪ Card：子 max 内在 + 双 fill 阴影 + elevation 纯绘 ----------

#[test]
fn card_shadow_double_fill_and_child_intrinsic() {
    let mut ui = Ui::<64>::new(200, 150);
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
    let card = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Card,
                key: 0,
                props: Props::Card(CardProps { elevation: 2, corner_radius: 8 }),
            },
        )
        .unwrap();
    let txt = ui
        .add(
            card,
            ViewSpec {
                kind: ViewKind::Text,
                key: 0,
                props: Props::Text(TextProps {
                    style: TextStyle { fg: ON_SURFACE, bg: None, scale: FontSize::S8x16 },
                    text: TextBuf::new(b"A").unwrap(),
                }),
            },
        )
        .unwrap();
    // 无子 Card：内在 0×0（子聚合 max 空集）。
    let empty = ui
        .add(
            col,
            ViewSpec {
                kind: ViewKind::Card,
                key: 0,
                props: Props::Card(CardProps { elevation: 0, corner_radius: 0 }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(card).unwrap(), berry_ui::damage::RectI::new(0, 0, 8, 16), "子 max×max");
    assert_eq!(ui.bounds(txt).unwrap(), berry_ui::damage::RectI::new(0, 0, 8, 16), "子全幅叠放");
    assert_eq!(
        ui.bounds(empty).unwrap(),
        berry_ui::damage::RectI::new(0, 16, 0, 0),
        "无子 Card 0×0"
    );

    // 双 fill 阴影：整界阴影色 + 面底右下内缩 4（elevation 2 × 2px）；
    // 子先画盖上（Text 蜂格不计遮挡）。
    let dl = paint_full(&mut ui);
    assert_eq!(
        fills(&dl),
        vec![(0, 0, 8, 16, CARD_SHADOW), (0, 0, 4, 12, CARD_BG)],
        "elevation 2 → off 4 双 fill"
    );
    assert_eq!(glyphs(&dl), vec![(b'A', 0, 0)]);

    // elevation 2→0：纯绘面（A 零重测；s7 保守臂）+ 单 fill。
    ui.layout(); // 结账
    ui.set_props(card, Props::Card(CardProps { elevation: 0, corner_radius: 8 }))
        .unwrap();
    let st = ui.layout();
    #[cfg(not(feature = "s7"))]
    assert_eq!(st.measured, 0, "elevation 纯绘零重测（阴影条在界内）");
    #[cfg(feature = "s7")]
    assert!(st.measured >= 1, "s7 面保守臂重排（props_diff 未扩表——文档化）");
    let dl = paint_full(&mut ui);
    assert_eq!(fills(&dl), vec![(0, 0, 8, 16, CARD_BG)], "零阴影单 fill");
}

// ---------- ⑫ 第三批命令面：set_props 七 kind 开口 + 拒面 ----------

#[test]
fn third_batch_set_props_and_reject_faces() {
    // Tip 换文：几何面（宽随文）——同长换文重排但界不变、字形随换。
    let mut ui = Ui::<64>::new(120, 80);
    let root = ui.root();
    let tip = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Tip,
                key: 0,
                props: Props::Tip(TipProps {
                    text: TextBuf::new(b"Hi").unwrap(),
                    placement: TipPlacement::Top,
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(tip).unwrap(), berry_ui::damage::RectI::new(0, 0, 16, 16));
    ui.set_props(
        tip,
        Props::Tip(TipProps { text: TextBuf::new(b"Yo").unwrap(), placement: TipPlacement::End }),
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(tip).unwrap(), berry_ui::damage::RectI::new(0, 0, 16, 16));
    let dl = paint_full(&mut ui);
    assert_eq!(glyphs(&dl), vec![(b'Y', 0, 0), (b'o', 8, 0)], "换文字形");

    // Badge 宽档：5→9 同档纯绘（A 零重测）；9→10 跨档重排。
    let mut ui = Ui::<64>::new(120, 80);
    let root = ui.root();
    let badge = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Badge,
                key: 0,
                props: Props::Badge(BadgeProps { count: 5, show_dot: false }),
            },
        )
        .unwrap();
    ui.layout();
    ui.layout(); // 结账
    ui.set_props(badge, Props::Badge(BadgeProps { count: 9, show_dot: false })).unwrap();
    let st = ui.layout();
    #[cfg(not(feature = "s7"))]
    assert_eq!(st.measured, 0, "同宽档纯绘零重测");
    #[cfg(feature = "s7")]
    assert!(st.measured >= 1, "s7 面保守臂（文档化）");
    assert_eq!(ui.bounds(badge).unwrap().w, 16);
    ui.set_props(badge, Props::Badge(BadgeProps { count: 10, show_dot: false })).unwrap();
    ui.layout();
    assert_eq!(ui.bounds(badge).unwrap().w, 24, "两位数跨档 16→24");

    // Sbar 走向互转：几何面 4×200 ↔ 200×4。
    let mut ui = Ui::<64>::new(240, 240);
    let root = ui.root();
    let sbar = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Sbar,
                key: 0,
                props: Props::Sbar(SbarProps {
                    orientation: Orientation::Vertical,
                    thumb_start: 0,
                    thumb_end: 1000,
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(sbar).unwrap(), berry_ui::damage::RectI::new(0, 0, 4, 200));
    ui.set_props(
        sbar,
        Props::Sbar(SbarProps {
            orientation: Orientation::Horizontal,
            thumb_start: 0,
            thumb_end: 1000,
        }),
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(sbar).unwrap(), berry_ui::damage::RectI::new(0, 0, 200, 4), "走向互转");

    // Collapse set_props expanded：几何面 82 ↔ 32。
    let mut ui = Ui::<64>::new(200, 300);
    let root = ui.root();
    let cpse = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Collapse,
                key: 0,
                props: Props::Collapse(CollapseProps { expanded: true, on_toggle: 1 }),
            },
        )
        .unwrap();
    ui.add(
        cpse,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps { bg: Color { r: 0, g: 0, b: 0 }, w: Some(100), h: Some(50) }),
        },
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(cpse).unwrap().h, 82);
    ui.set_props(cpse, Props::Collapse(CollapseProps { expanded: false, on_toggle: 1 }))
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(cpse).unwrap().h, 32, "set_props 收起同律");

    // Fab 扩展标签：几何面 56×56 → 80×56。
    let mut ui = Ui::<64>::new(160, 120);
    let root = ui.root();
    let fab = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Fab,
                key: 0,
                props: Props::Fab(FabProps {
                    icon: 5,
                    extended_label: TextBuf::new(b"").unwrap(),
                    on_tap: 0,
                }),
            },
        )
        .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(fab).unwrap(), berry_ui::damage::RectI::new(0, 0, 56, 56));
    ui.set_props(
        fab,
        Props::Fab(FabProps { icon: 5, extended_label: TextBuf::new(b"Go").unwrap(), on_tap: 0 }),
    )
    .unwrap();
    ui.layout();
    assert_eq!(ui.bounds(fab).unwrap(), berry_ui::damage::RectI::new(0, 0, 80, 56), "扩展形态");

    // Chip selected 翻转：纯绘面（label 不动）。
    let mut ui = Ui::<64>::new(120, 80);
    let root = ui.root();
    let chip = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Chip,
                key: 0,
                props: Props::Chip(ChipProps {
                    label: TextBuf::new(b"Tag").unwrap(),
                    selected: false,
                    on_tap: 9,
                }),
            },
        )
        .unwrap();
    ui.layout();
    ui.layout(); // 结账
    ui.set_props(
        chip,
        Props::Chip(ChipProps { label: TextBuf::new(b"Tag").unwrap(), selected: true, on_tap: 9 }),
    )
    .unwrap();
    let st = ui.layout();
    #[cfg(not(feature = "s7"))]
    assert_eq!(st.measured, 0, "selected 纯绘零重测");
    #[cfg(feature = "s7")]
    assert!(st.measured >= 1, "s7 面保守臂（文档化）");

    // —— 拒面：Sbar thumb 域（set_props + add）/ Card elevation（两口）/
    // kind 失配 / 叶父位 ——
    let mut ui = Ui::<64>::new(240, 240);
    let root = ui.root();
    let sbar2 = ui
        .add(
            root,
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
    assert_eq!(
        ui.set_props(
            sbar2,
            Props::Sbar(SbarProps {
                orientation: Orientation::Horizontal,
                thumb_start: 0,
                thumb_end: 1001
            })
        ),
        Err(UiError::NodeBad),
        "thumb_end 越千分域拒"
    );
    assert_eq!(
        ui.add(
            root,
            ViewSpec {
                kind: ViewKind::Sbar,
                key: 0,
                props: Props::Sbar(SbarProps {
                    orientation: Orientation::Vertical,
                    thumb_start: 600,
                    thumb_end: 400
                })
            },
        ),
        Err(UiError::NodeBad),
        "add 面 start>end 同律拒"
    );
    let card2 = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Card,
                key: 0,
                props: Props::Card(CardProps { elevation: 3, corner_radius: 4 }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.set_props(card2, Props::Card(CardProps { elevation: 4, corner_radius: 4 })),
        Err(UiError::NodeBad),
        "Card elevation>3 拒"
    );
    assert_eq!(
        ui.add(
            root,
            ViewSpec {
                kind: ViewKind::Card,
                key: 0,
                props: Props::Card(CardProps { elevation: 4, corner_radius: 0 })
            },
        ),
        Err(UiError::NodeBad),
        "add 面 elevation 同律拒"
    );
    // kind 失配：Tip props 塞 Chip 槽。
    let chip2 = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Chip,
                key: 0,
                props: Props::Chip(ChipProps {
                    label: TextBuf::new(b"x").unwrap(),
                    selected: false,
                    on_tap: 0,
                }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.set_props(
            chip2,
            Props::Tip(TipProps { text: TextBuf::new(b"x").unwrap(), placement: TipPlacement::Top })
        ),
        Err(UiError::NodeBad),
        "kind 失配拒"
    );
    // 叶父位拒（第三批五叶——AST 面叶带子拒同律）。
    let badge2 = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Badge,
                key: 0,
                props: Props::Badge(BadgeProps { count: 3, show_dot: false }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.add(
            badge2,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: Color { r: 0, g: 0, b: 0 }, w: None, h: None })
            },
        ),
        Err(UiError::BadParent),
        "Badge 叶父位拒"
    );
    assert_eq!(
        ui.add(
            chip2,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: Color { r: 0, g: 0, b: 0 }, w: None, h: None })
            },
        ),
        Err(UiError::BadParent),
        "Chip 叶父位拒"
    );
    assert_eq!(ui.lost_widget_events(), 0);
}
