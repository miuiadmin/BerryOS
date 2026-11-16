//! 维八暗色刀钉板（UI-02 裁决「暗色成系统」三小刀的回归面）：
//!
//! - [`theme_mode_runtime_switch`]：`Ui::set_theme_mode` 运行态——换档
//!   置 FULL damage、paint 取色随档换（dark token 入命令流）、切回
//!   light 档命令流字节级复原（往返零漂移）。
//! - [`paint_fixed_palette_exempt_gate`]：paint.rs 固定色板豁免 grep 门
//!   ——源面零未登记 `Color` 字面量，豁免数只减不能增（现值 1 =
//!   Image 哨兵死实参；色板本体已整块迁 theme.rs `WidgetColors`）。
//!
//! light 档全量像素零漂移由 gallery2 golden 像素哈希门另守（本刀未
//! 重生成任何画廊格——「若有格子重生成须自证」的前件不成立）。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos};
use berry_ui::desc::{
    CheckProps, ColProps, Props, SliderProps, TextBuf, ToggleProps, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::theme::{self, ThemeMode as Theme, Tokens, TRACK_OFF, TRACK_OFF_DARK};
use berry_ui::{NodeId, UiMain, ViewKind};

const SCRATCH_GLYPHS: usize = 64;

fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

/// 组件三件树（Toggle 关轨 / Slider 轨+填充 / Check 框——色板消费面
/// 代表格）：FULL 帧 DisplayList（scenes.rs `dl_of` 同形——scratch 泄漏
/// 一次 = 测试面豁免同律）。
fn dl_of(ui: &mut UiMain) -> Vec<Cmd<'static>> {
    ui.drain_damage();
    ui.wake(0);
    let scr: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());
    let mut enc = CommandEncoder::new();
    ui.paint(scr, &mut enc);
    enc.finish().iter().cloned().collect()
}

/// DisplayList 内的 fill 色 collecting（坐标无关——档位取色断言面）。
fn fill_colors(dl: &[Cmd]) -> Vec<Color> {
    dl.iter()
        .filter_map(|c| match c {
            Cmd::FillRect { paint, .. } => Some(paint.color),
            _ => None,
        })
        .collect()
}

fn build_tree(ui: &mut UiMain) {
    let root = ui.root();
    let col = ui
        .add(
            root,
            vspec(
                ViewKind::Col,
                Props::Col(ColProps { gap: 8, w: Some(240), h: None, cross: CrossAlign::Start }),
            ),
        )
        .unwrap();
    ui.add(
        col,
        vspec(ViewKind::Toggle, Props::Toggle(ToggleProps { checked: false, on_change: 0 })),
    )
    .unwrap();
    ui.add(
        col,
        vspec(
            ViewKind::Slider,
            Props::Slider(SliderProps { value: 400, min: 0, max: 1000, on_change: 0 }),
        ),
    )
    .unwrap();
    ui.add(
        col,
        vspec(ViewKind::Check, Props::Check(CheckProps { checked: false, on_change: 0 })),
    )
    .unwrap();
    ui.layout(); // 内在尺寸落位（无 layout 零面积空绘——scenes 同律）
}

/// 运行态换档全链：默认 light → dark（FULL damage + dark token 入流）→
/// light（命令流与首帧全等——往返零漂移）。
#[test]
fn theme_mode_runtime_switch() {
    let mut ui = UiMain::new(240, 300);
    build_tree(&mut ui);

    // 默认 light 档（旧行为零漂移的运行态锚）。
    assert_eq!(ui.theme_mode(), Theme::Light);
    let dl_light1 = dl_of(&mut ui);
    let cols1 = fill_colors(&dl_light1);
    assert!(cols1.contains(&TRACK_OFF), "light 档 Toggle 关轨 = 旧色板值");
    assert!(!cols1.contains(&TRACK_OFF_DARK), "light 档无 dark 档色混入");
    assert!(cols1.contains(&Tokens::pick(Theme::Light).w.accent), "Slider 填充 = light accent");

    // 换 dark：档位翻转 + FULL damage（set_viewport 同形）+ paint 取色
    // 随档换。
    ui.set_theme_mode(Theme::Dark);
    assert_eq!(ui.theme_mode(), Theme::Dark);
    assert!(ui.damage().full, "换档置 FULL damage（全帧重画）");
    let dl_dark = dl_of(&mut ui);
    let cols_dark = fill_colors(&dl_dark);
    assert!(cols_dark.contains(&TRACK_OFF_DARK), "dark 档 Toggle 关轨换档");
    assert!(!cols_dark.contains(&TRACK_OFF), "旧 light 档色退场");
    assert!(cols_dark.contains(&Tokens::pick(Theme::Dark).w.accent), "Slider 填充 = dark accent");

    // 幂等：同档重设零脏帧。
    ui.drain_damage();
    ui.set_theme_mode(Theme::Dark);
    assert!(ui.damage().is_empty(), "同档重设幂等——零脏帧");

    // 切回 light：命令流与首帧全等（byte-exact 往返零漂移）。
    ui.set_theme_mode(Theme::Light);
    let dl_light2 = dl_of(&mut ui);
    assert_eq!(
        format!("{dl_light1:?}"),
        format!("{dl_light2:?}"),
        "dark 往返后 light 档命令流复原"
    );
}

/// 空树换档防御（无树不炸、档位照翻——render_frame 首帧前换档面）。
#[test]
fn theme_mode_switch_before_first_frame() {
    let mut ui = UiMain::new(240, 300);
    ui.set_theme_mode(Theme::Dark);
    assert_eq!(ui.theme_mode(), Theme::Dark);
    ui.set_theme_mode(Theme::Light);
    assert_eq!(ui.theme_mode(), Theme::Light);
}

/// paint.rs 固定色板豁免 grep 门：源面（测试模块前）每个 `Color` 字面量
/// 必须逐个携带半角方括号 paint-exempt 登记标记；豁免数只减不能增
/// （现值 1——Image 哨兵死实参；清零路径 = theme.rs 双档 token 化）。
#[test]
fn paint_fixed_palette_exempt_gate() {
    let src = include_str!("../src/paint.rs");
    let code = src.split("#[cfg(test)]").next().unwrap();
    let literals = code.matches("Color { r:").count();
    let exempt = code.matches("[paint-exempt]").count();
    assert_eq!(
        literals, exempt,
        "paint.rs 固定色字面量须逐个登记豁免标记（literals={literals} exempt={exempt}）"
    );
    assert_eq!(exempt, 1, "豁免数只减不能增：现值 1（Image 哨兵实参）");
}

// 防未用导入告警面（Node/TextBuf 随树构造在）。
#[allow(dead_code)]
fn _anchors() -> Option<(NodeId, TextBuf)> {
    None
}

// theme 模块再导出锚（双档常量单源消费自证）。
const _: (Color, Color) = (theme::ACCENT, theme::ACCENT_DARK);
