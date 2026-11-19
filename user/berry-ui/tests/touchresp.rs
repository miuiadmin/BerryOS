//! 触摸响应设计 v1.0（`docs/触摸响应设计-v1.0.md` §5）A′/C′ 钉板：
//!
//! - **A′（铁律 3）**：首触免 FULL damage 放大——`pump_input_at` 内建
//!   pump→wake 时序：press_react 先标精确按压矩形，wake 的条件化 FULL
//!   只命中空集（正例）；旧序 wake→pump（本刀杀死的形状）对照 = FULL
//!   兜底吞掉精确矩形（负例）。
//! - **C′（铁律 5）**：`pump_input_at(now)` 重采样目标新鲜——从 now 推
//!   下一拍（`next_vsync_tick(now) − RESAMPLE_LATENCY_MS` 单源推导）；
//!   旧面 `pump_input(上帧 expected)` 的陈旧目标（idle 长睡后远古值）
//!   对照 = target < 首采样 → 后向外推不提供 → 拒采直通原始坐标。
//!
//! 值级钉法（C′）：事件窗 [t0+2ms, t0+6ms]，now 恰在拍沿 tick（含沿
//! 不欠拍 → expected = tick 本拍）且 tick − 5ms = t0+4ms 落窗内——末
//! MOVE 的括点插值 x 唯一钉死 target 推导（任何别的 target 产别的 x）。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_guiproto::{Rect, PHASE_DOWN, PHASE_MOVE};
use berry_ui::desc::{Msg, Props, RectProps, ViewSpec};
use berry_ui::frame::FramePhase;
use berry_ui::hit::PtrEvent;
use berry_ui::{NodeId, Ui, ViewKind};

const P: u64 = 16_666_666; // 60Hz（launcher VSYNC_NS 同值）
const C_BG: Color = Color { r: 1, g: 2, b: 3 };

fn rect_spec(c: Color, w: u32, h: u32) -> ViewSpec {
    ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg: c, w: Some(w), h: Some(h), radius: 0 }) }
}

/// A′ 树（press.rs 同形）：Root → A(30×30) @ (0,0)，press_map 自映射。
fn press_tree(ui: &mut Ui<16>) -> NodeId {
    let root = ui.root();
    let a = ui.add(root, rect_spec(C_BG, 30, 30)).unwrap();
    ui.layout();
    ui.drain_damage();
    ui.set_press_map(&[(a, a)]).unwrap();
    a
}

/// 帧产 damage 提取（render_frame 出口镜像）。
fn frame_damage(ui: &mut Ui<16>, now_ns: u64) -> berry_ui::damage::DamageSet {
    let mut enc = CommandEncoder::new();
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    let out = ui.render_frame(now_ns, &mut scratch, &mut enc);
    assert!(out.drew, "首触帧必有产出（不欠拍）");
    out.damage
}

#[test]
fn a_prime_pump_then_wake_keeps_precise_first_touch() {
    // 正例：pump_input_at = pump（press_react 标 30×30 精确矩形）→ 有
    // 事件才内建 wake——damage 非空 → 保矩形不落 FULL。
    let mut ui = Ui::<16>::new(100, 100);
    ui.set_vsync(P, 0);
    press_tree(&mut ui);

    ui.push_pointer(PtrEvent::from_wire(PHASE_DOWN, 0, 10, 10, P, 0, 0)).unwrap();
    let msgs = ui.pump_input_at(P);
    assert!(matches!(msgs.iter().next(), Some(Msg::Press { .. })), "DOWN → Press");
    assert_eq!(ui.frame_phase(), FramePhase::Run, "有输入事件 → 内建 wake 起振");

    let d = frame_damage(&mut ui, P);
    assert!(!d.full, "A′：首触 damage 非空 → 不触发 FULL 兜底");
    assert_eq!(d.count, 1, "精确按压矩形直用（PRESSED_BG 换底同界）");
    assert_eq!(d.rects[0], Rect { x: 0, y: 0, w: 30, h: 30 });
}

#[test]
fn a_prime_wake_before_pump_swallows_precise_damage() {
    // 负例对照（旧序——本刀杀死的形状）：wake 先行 → damage 空集 →
    // FULL 兜底；随后 pump 的 press_react 精确矩形被整屏语义吞掉
    // （FULL 后 add no-op）——首触帧被放大成整屏重绘。
    let mut ui = Ui::<16>::new(100, 100);
    ui.set_vsync(P, 0);
    press_tree(&mut ui);

    ui.wake(P); // 旧序：wake 在 pump 之前（damage 必空 → FULL）
    ui.push_pointer(PtrEvent::from_wire(PHASE_DOWN, 0, 10, 10, P, 0, 0)).unwrap();
    let _ = ui.pump_input(0); // 裸 pump（不自动 wake——旧消费方时序）

    let d = frame_damage(&mut ui, P);
    assert!(d.full, "旧序：FULL 兜底吞掉精确按压矩形");
    assert_eq!(d.count, 0, "FULL = 矩形集已清（整屏语义）");
}

/// C′ 拖树：Root → 大 Rect（60×60 @ (0,0)——(10,10) 与 MOVE 轨迹恒命中）。
fn drag_tree() -> Ui<16> {
    let mut ui = Ui::<16>::new(100, 100);
    ui.set_vsync(P, 0);
    let root = ui.root();
    let _pane = ui.add(root, rect_spec(C_BG, 60, 60)).unwrap();
    ui.layout();
    ui.drain_damage();
    ui
}

/// 拖流（inject4 同构时间基）：DOWN(10)@t0 → MOVE(20)@t0+2ms（dx=10 >
/// slop 8 → DragStart）→ MOVE(30)@t0+4ms → MOVE(40)@t0+6ms——滑窗
/// [t0+2, t0+6] span 4ms ∈ [RESAMPLE_MIN_DELTA_MS, RESAMPLE_MAX]。
fn push_drag(ui: &mut Ui<16>, t0: u64) {
    for (phase, x, dt) in [
        (PHASE_DOWN, 10u32, 0u64),
        (PHASE_MOVE, 20, 2),
        (PHASE_MOVE, 30, 4),
        (PHASE_MOVE, 40, 6),
    ] {
        ui.push_pointer(PtrEvent::from_wire(phase, 0, x, 10, t0 + dt * 1_000_000, 0, 0))
            .unwrap();
    }
}

/// 末位 DragMove（每 MOVE 至多一条——inject4 钉板）。
fn last_drag_move(msgs: &[Msg]) -> &Msg {
    msgs.iter()
        .rev()
        .find(|m| matches!(m, Msg::DragMove { .. }))
        .expect("拖流必产 DragMove")
}

#[test]
fn c_prime_pump_input_at_target_fresh() {
    // 拍沿 tick（phase 0——含沿不欠拍：next_vsync_tick(tick) = tick 本拍）；
    // 事件窗基 t0 = tick − 9ms → 推导 target = tick − 5ms = t0+4ms 落窗内。
    let tick = 600 * P;
    let t0 = tick - 9_000_000;

    // 新鲜目标（本刀）：末 MOVE 重采样命中——括点插值 [t0+4, t0+6] 的
    // num=0 端 → x=30（原始 40 被替换），resampled=true；x=30 唯一钉死
    // target = tick−5ms（任何别的推导产别的 x）。
    let mut ui = drag_tree();
    push_drag(&mut ui, t0);
    let msgs: Vec<Msg> = ui.pump_input_at(tick).iter().copied().collect();    match last_drag_move(&msgs) {
        Msg::DragMove { x, resampled, .. } => {
            assert!(*resampled, "C′：新鲜目标不拒采（重采样命中）");
            assert_eq!(*x, 30, "target = next_vsync_tick(now)−5ms 的插值锚点");
        }
        other => panic!("末位非 DragMove: {other:?}"),
    }

    // 陈旧目标对照（旧面 pump_input(上帧 expected)——idle 长睡后远古
    // 值形）：target < 首采样 → 后向外推不提供 → None 直通原始坐标。
    let mut ui2 = drag_tree();
    push_drag(&mut ui2, t0);
    let msgs2: Vec<Msg> = ui2.pump_input(t0 - 40_000_000).iter().copied().collect();
    match last_drag_move(&msgs2) {
        Msg::DragMove { x, resampled, .. } => {
            assert!(!*resampled, "旧面：陈旧目标拒采（resample None 直通）");
            assert_eq!(*x, 40, "原始坐标保真（未被替换）");
        }
        other => panic!("旧面末位非 DragMove: {other:?}"),
    }
}
