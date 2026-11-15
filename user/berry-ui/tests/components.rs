//! 刀 3（OH 形）组件层钉板：mix_under 预混数学 / 圆角 Rect 发射命令形态 /
//! ListItem 组件装配树形 / TopBar/BottomTabs Spec 值级。确定性：无随机
//! 无时钟；断言到值。

use berry_gfx::cmd::Color;
use berry_ui::components::{
    add_list_item, bottom_tabs_spec, card_pressed_bg, card_spec, mix_under, topbar_spec,
    topbar_title_spec, CARD_PRESSED_BG, TABS_VP, TOPBAR_VP,
};
use berry_ui::desc::ViewKind;
use berry_ui::UiMain;

const W: u32 = 480;
const H: u32 = 854;

fn mk_ui() -> UiMain {
    UiMain::new(W, H)
}

// ---------- mix_under 预混数学 ----------

#[test]
fn mix_under_endpoints_and_midpoint() {
    let white = Color { r: 255, g: 255, b: 255 };
    let black = Color { r: 0, g: 0, b: 0 };
    // 端点：alpha=255 → fg；0 → under。
    assert_eq!(mix_under(black, white, 255), black);
    assert_eq!(mix_under(black, white, 0), white);
    // 中点：50% 黑混白 → 127（(0*128 + 255*127)/255 = 127）。
    let m = mix_under(black, white, 128);
    assert_eq!(m, Color { r: 127, g: 127, b: 127 });
    // 按压常量 = 20% 黑（alpha=51）混白卡 → 255*(255-51)/255 = 204。
    assert_eq!(card_pressed_bg(white), CARD_PRESSED_BG);
    assert_eq!(CARD_PRESSED_BG, mix_under(Color { r: 0, g: 0, b: 0 }, white, 51));
}

// ---------- 圆角 Rect 发射（paint 层命令形态） ----------

#[test]
fn rounded_rect_emits_banded_fill_cmds() {
    use berry_gfx::cmd::{Cmd, CommandEncoder};
    use berry_ui::desc::{Props, RectProps};
    use berry_ui::{ViewKind, ViewSpec};
    let mut ui = mk_ui();
    let root = ui.root();
    // 24×16 卡 radius=4：顶/底各 4 行带 + 中带 = ≤9 条 FillRect。
    ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps {
                bg: Color { r: 0xAA, g: 0xBB, b: 0xCC },
                w: Some(24),
                h: Some(16),
                radius: 4,
            }),
        },
    )
    .unwrap();
    ui.layout(); // paint 前置：圆角带发射需已布局界
    ui.wake(0); // FULL → 全树发射
    let mut scr = [berry_gfx::cmd::GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    // 命令数：radius 带 = 2r 条 + 中带 1 条 + Present 尾 = 10。
    assert_eq!(rep.cmds, 10, "radius=4 → 2×4 带 + 1 中带 + Present");
    // 帧头几条命令全是 FillRect（圆角带形态——无整矩形 fill）。
    let dl = enc.finish();
    let fills: Vec<&Cmd> = dl.iter().filter(|c| matches!(c, Cmd::FillRect { .. })).collect();
    assert_eq!(fills.len(), 9, "9 条 FillRect（Present 另 1 条）");
    // 中带宽 = 全宽（y=4..12 的某条 w=24）。
    let has_full_width_mid = fills.iter().any(|c| match c {
        Cmd::FillRect { rect, .. } => rect.w == 24,
        _ => false,
    });
    assert!(has_full_width_mid, "中带必在（整宽 24px）");
}

#[test]
fn zero_radius_rect_single_cmd_zero_drift() {
    use berry_gfx::cmd::{Cmd, CommandEncoder};
    use berry_ui::desc::{Props, RectProps};
    use berry_ui::{ViewKind, ViewSpec};
    let mut ui = mk_ui();
    let root = ui.root();
    ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps {
                bg: Color { r: 1, g: 2, b: 3 },
                w: Some(10),
                h: Some(10),
                radius: 0,
            }),
        },
    )
    .unwrap();
    ui.layout(); // paint 前置：界未定 → paint 无可发（wake 空集直过）
    ui.wake(0);
    let mut scr = [berry_gfx::cmd::GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    let mut enc = CommandEncoder::new();
    let rep = ui.paint(&mut scr, &mut enc);
    assert_eq!(rep.cmds, 2, "radius=0 单条 fill + Present = 旧形态零漂移");
    let dl = enc.finish();
    assert!(matches!(dl.iter().next(), Some(Cmd::FillRect { .. })));
}

// ---------- 组件装配 ----------

#[test]
fn list_item_builds_card_icon_title_subtitle_tree() {
    let mut ui = mk_ui();
    let root = ui.root();
    let card = add_list_item(&mut ui, root, 1, 7, b"Settings", b"System", false).unwrap();
    // 卡片是 Pad 叠载容器（kind_of 公开面；Rect 面在其内）。
    assert_eq!(ui.kind_of(card).unwrap(), ViewKind::Pad);
    // 布局后面卡的界在视口内（Pad 内含 Icon+两行文本——卡高 > pad 高）。
    ui.layout();
    let b = ui.bounds(card).unwrap();
    assert!(b.w > 0 && b.h > 0, "卡片布局非零：{b:?}");
    // 命中面：卡中点命中卡片本身（node_at 走 hit 树）。
    let mid = ui.node_at((b.x + b.w / 2) as u32, (b.y + b.h / 2) as u32);
    assert!(mid.is_some(), "卡中点必有命中节点");
}

#[test]
fn topbar_and_tabs_heights_match_oh() {
    let bar = topbar_spec(3);
    match bar.props {
        berry_ui::Props::Rect(p) => {
            assert_eq!(p.h, Some(TOPBAR_VP * 3));
            assert_eq!(p.radius, 0);
        }
        _ => panic!("topbar must be Rect"),
    }
    let tabs = bottom_tabs_spec(3);
    match tabs.props {
        berry_ui::Props::Rect(p) => assert_eq!(p.h, Some(TABS_VP * 3)),
        _ => panic!("tabs must be Rect"),
    }
    let title = topbar_title_spec(b"Berry", berry_gfx::font::FontSize::S16x32).unwrap();
    assert_eq!(title.kind, ViewKind::Text);
}

#[test]
fn card_spec_pressed_swaps_bg() {
    let normal = card_spec(1, false);
    let pressed = card_spec(1, true);
    match (normal.props, pressed.props) {
        (berry_ui::Props::Rect(a), berry_ui::Props::Rect(b)) => {
            assert_eq!(a.radius, 16);
            assert_eq!(b.bg, CARD_PRESSED_BG);
            assert_ne!(a.bg, b.bg);
        }
        _ => panic!("cards must be Rect"),
    }
}

// ---------- 刀 4 增补：Button / Switch / Progress ----------

#[test]
fn button_spec_oh_m3_merged_values() {
    use berry_ui::components::{button_spec, ACCENT, BUTTON_H_VP, ON_ACCENT};
    let (face, text) = button_spec(3, b"OK", false).unwrap();
    match face.props {
        berry_ui::Props::Rect(p) => {
            assert_eq!(p.h, Some(BUTTON_H_VP * 3), "40vp×3 = M3/OH 合体高");
            assert_eq!(p.radius, 60, "20vp 圆角 ×3");
            assert_eq!(p.bg, ACCENT);
        }
        _ => panic!("face must be Rect"),
    }
    match text.props {
        berry_ui::Props::Text(p) => {
            assert_eq!(p.style.fg, ON_ACCENT);
            assert_eq!(p.text.as_bytes(), b"OK");
        }
        _ => panic!("label must be Text"),
    }
    // pressed 态换底（20% 黑预混强调色）
    let (pface, _) = button_spec(3, b"OK", true).unwrap();
    match pface.props {
        berry_ui::Props::Rect(p) => assert_ne!(p.bg, ACCENT),
        _ => panic!(),
    }
}

#[test]
fn switch_spec_track_knob_positions() {
    use berry_ui::components::{add_switch, switch_spec, ACCENT, DIVIDER, SWITCH_KNOB_VP, SWITCH_W_VP};
    let (on_track, on_knob) = switch_spec(1, true);
    match (on_track.props, on_knob.props) {
        (berry_ui::Props::Rect(t), berry_ui::Props::Rect(k)) => {
            assert_eq!(t.bg, ACCENT, "on 轨道强调色");
            assert_eq!((t.w, t.h), (Some(52), Some(28)), "OH 52×28vp");
            assert_eq!((k.w, k.h), (Some(SWITCH_KNOB_VP), Some(SWITCH_KNOB_VP)), "24vp 球");
        }
        _ => panic!(),
    }
    let (off_track, _) = switch_spec(1, false);
    match off_track.props {
        berry_ui::Props::Rect(t) => assert_eq!(t.bg, DIVIDER, "off 轨道灰"),
        _ => panic!(),
    }
    // 装配 + 滑块偏移：on 右置 (52-24)=28px；off 0。
    let mut ui = mk_ui();
    let root = ui.root();
    let s_on = add_switch(&mut ui, root, 1, true).unwrap();
    ui.layout();
    let b = ui.bounds(s_on).unwrap();
    assert!(b.w >= 52 && b.h >= 28, "开关界 {b:?}");
}

#[test]
fn progress_spec_capsule_pair() {
    use berry_ui::components::progress_spec;
    let (track, filled) = progress_spec(3, 0.5);
    match (track.props, filled.props) {
        (berry_ui::Props::Rect(t), berry_ui::Props::Rect(f)) => {
            assert_eq!(t.h, Some(24), "8vp×3");
            assert_eq!(f.w, Some(50 * 3), "50% × density");
        }
        _ => panic!(),
    }
}
