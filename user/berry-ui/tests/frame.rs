//! 验收②（GUI-3 §8）：FramePhase 三态状态机全转移（真值表全格）+
//! IDLE next_wakeup None / wake 不欠拍 / 2000ms 静默强降 / SETTLE 在途
//! 确认收口 / note_presented actual 轨 + jank 判定 / credits 簿记 /
//! VoteBallot 票面形状。
//!
//! 纪律：确定性（无随机无时间依赖）、断言值级、只用公共 API（§6）。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_guiproto::{
    CREDITS_MAX, DAMAGE_MAX, FRAME_DISCARDED, FRAME_PRESENTED, Intent, PHASE_DOWN, PHASE_MOVE,
    PHASE_UP,
};
use berry_ui::anim::{Anim, AnimProp, Easing};
use berry_ui::desc::{Props, RectProps, ViewSpec};
use berry_ui::frame::{FramePhase, FrameSignals, VoteBallot, next_phase};
use berry_ui::telemetry::FrameRec;
use berry_ui::{UiMain, ViewKind, IDLE_AFTER_MS};

/// 软 vsync 周期（≈60Hz；1e9/P = 60 整除成立：60×P = 999_999_960 ≤ 1e9）。
const P: u64 = 16_666_666;

fn scratch() -> [GlyphPos; 8] {
    [GlyphPos { ch: 0, x: 0, y: 0 }; 8]
}

fn sig(damage: bool, anim: bool, input: bool, present_pending: bool) -> FrameSignals {
    FrameSignals { damage, anim, input, present_pending }
}

/// 单帧渲染便捷口：scratch/encoder 随帧即弃（DisplayList 'a 钉调用方
/// scratch——两次渲染不可共用同一 scratch 借用）。
fn rf(ui: &mut UiMain, now_ns: u64) -> berry_ui::frame::FrameOutcome {
    let mut scr = scratch();
    let mut enc = CommandEncoder::new();
    ui.render_frame(now_ns, &mut scr, &mut enc)
}

// ---------- next_phase 纯函数真值表 ----------

#[test]
fn next_phase_truth_table_all_cells() {
    // 16 格信号全组合（位序 d|a<<1|i<<2|p<<3——工作位低三位、在途位
    // bit3；判定序：工作 > 静默强降 > 在途 > Idle）
    const EXPECT: [FramePhase; 16] = [
        FramePhase::Idle,   // 0000：无工作无在途 → Idle
        FramePhase::Run,    // 0001：damage
        FramePhase::Run,    // 0010：anim
        FramePhase::Run,    // 0011：damage+anim
        FramePhase::Run,    // 0100：input
        FramePhase::Run,    // 0101
        FramePhase::Run,    // 0110
        FramePhase::Run,    // 0111
        FramePhase::Settle, // 1000：仅 present_pending → Settle
        FramePhase::Run,    // 1001：damage|pending——工作位压在途位
        FramePhase::Run,    // 1010
        FramePhase::Run,    // 1011
        FramePhase::Run,    // 1100
        FramePhase::Run,    // 1101
        FramePhase::Run,    // 1110
        FramePhase::Run,    // 1111
    ];
    for cur in [FramePhase::Run, FramePhase::Settle, FramePhase::Idle] {
        for mask in 0..16u8 {
            let s = sig(mask & 1 != 0, mask & 2 != 0, mask & 4 != 0, mask & 8 != 0);
            assert_eq!(
                next_phase(cur, s, 0),
                EXPECT[mask as usize],
                "cur={:?} mask={:04b}",
                cur,
                mask
            );
        }
    }
    // 值级关键行钉板（非仅表驱动）
    assert_eq!(next_phase(FramePhase::Settle, sig(false, true, false, false), 0), FramePhase::Run);
    assert_eq!(next_phase(FramePhase::Run, sig(false, false, false, true), 0), FramePhase::Settle);
    assert_eq!(next_phase(FramePhase::Idle, sig(false, false, false, false), 0), FramePhase::Idle);
}

#[test]
fn next_phase_idle_strongarm_boundaries() {
    assert_eq!(IDLE_AFTER_MS, 2000); // OH 常量单源钉板

    let pending = sig(false, false, false, true);
    // 恰 2000ms：不强降（> 严格比较）→ Settle
    assert_eq!(next_phase(FramePhase::Settle, pending, 2000), FramePhase::Settle);
    // 2001ms：强降 Idle——在途回调未清也裁（卡死 Settle 兜底）
    assert_eq!(next_phase(FramePhase::Settle, pending, 2001), FramePhase::Idle);
    assert_eq!(next_phase(FramePhase::Run, pending, 2001), FramePhase::Idle);

    // 信号在先：有工作即 Run，强降只裁静默
    assert_eq!(next_phase(FramePhase::Settle, sig(true, false, false, true), 2001), FramePhase::Run);
    assert_eq!(next_phase(FramePhase::Settle, sig(false, true, false, true), 2001), FramePhase::Run);
    assert_eq!(next_phase(FramePhase::Settle, sig(false, false, true, true), 2001), FramePhase::Run);

    // 无工作无在途：任何静默时长（含 0）→ Idle
    let none = sig(false, false, false, false);
    assert_eq!(next_phase(FramePhase::Run, none, 0), FramePhase::Idle);
    assert_eq!(next_phase(FramePhase::Run, none, 10_000), FramePhase::Idle);
}

// ---------- Ui 集成：IDLE / wake / 强降 / SETTLE ----------

#[test]
fn idle_no_wakeup_and_idle_ballot() {
    let ui = UiMain::new(64, 48);
    assert_eq!(ui.frame_phase(), FramePhase::Idle);
    // IDLE = None（撤软 vsync 定时器、睡事件——零 tick）
    assert_eq!(ui.next_wakeup(), None);
    assert_eq!(ui.telemetry().frames(), 0);
    assert_eq!(ui.telemetry().wakes(), 0);
    assert_eq!(ui.telemetry().idle_exits(), 0);
    assert_eq!(
        ui.vote_hint(),
        VoteBallot {
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 0,
            scene: Intent::Background as u8,
            sticky: 1
        }
    );
}

#[test]
fn wake_on_tick_renders_immediately_no_debt() {
    let mut ui = UiMain::new(64, 48);
    ui.set_vsync(P, 0);
    ui.root();

    ui.wake(P); // 恰在拍沿唤醒
    assert_eq!(ui.frame_phase(), FramePhase::Run);
    // 含沿不欠拍：下一应醒 = 本拍 P（不是 2P）
    assert_eq!(ui.next_wakeup(), Some(P));
    assert_eq!(ui.telemetry().wakes(), 1);
    assert_eq!(ui.telemetry().idle_exits(), 1); // Idle→Run 转移计数

    let out = rf(&mut ui, P);
    assert!(out.drew, "wake 置 FULL → 必有产出");
    assert_eq!(out.expected_present_ns, P, "拍沿唤醒 → expected 即本拍");
    assert_eq!(out.rate_hint_hz, 60); // 1e9 / 16_666_666 = 60
    assert!(out.damage.full, "wake 的 FULL 语义经排水镜像回传");
    assert_eq!(ui.frame_phase(), FramePhase::Run); // 有 damage → Run
    assert_eq!(ui.telemetry().frames(), 1);
    assert_eq!(ui.telemetry().deadline_miss(), 0);
}

#[test]
fn consecutive_wakes_single_frame_debt() {
    let mut ui = UiMain::new(64, 48);
    ui.set_vsync(P, 0);
    ui.root();

    ui.wake(P);
    ui.wake(P); // 连续 wake——wakes 递增、idle_exits 只计转移
    assert_eq!(ui.telemetry().wakes(), 2);
    assert_eq!(ui.telemetry().idle_exits(), 1);

    let out1 = rf(&mut ui, P);
    assert!(out1.drew);
    assert_eq!(out1.rate_hint_hz, 60, "绘制帧 rate_hint = 协商倒数");
    // 第二次 wake 不追加第二帧债务：damage 排空后下一帧无产出
    let out2 = rf(&mut ui, P + 1_000);
    assert!(!out2.drew);
    assert!(out2.damage.is_empty());
    // 跳过帧不提交 → rate_hint 诚实填 0（frame.rs 字段注——无提交的
    // 帧无帧率话语权；回归钉板：曾无条件填 rate_hz() 报 60）
    assert_eq!(out2.rate_hint_hz, 0);
    assert_eq!(ui.telemetry().frames(), 2);
}

#[test]
fn settle_confirms_presented_then_idle() {
    let mut ui = UiMain::new(64, 48);
    ui.set_vsync(P, 0);
    let root = ui.root();

    ui.wake(P);
    let out1 = rf(&mut ui, P);
    assert!(out1.drew);
    assert_eq!(out1.expected_present_ns, P);
    ui.note_submitted(); // 在途 (seq 0, expected P)

    // 工作空 + 在途未清 → Settle（有兜底醒）
    let out2 = rf(&mut ui, 2 * P);
    assert!(!out2.drew);
    assert_eq!(ui.frame_phase(), FramePhase::Settle);
    // Settle 不追 vsync 拍：唯一醒点 = 静默强降闹钟（last_work P +
    // IDLE_AFTER_MS，与 now 取大——强降单源，卡死在途的兜底醒）
    assert_eq!(ui.next_wakeup(), Some(P + IDLE_AFTER_MS as u64 * 1_000_000));

    // 呈现应答（期限内）——note_presented 本身不直改相位（下一帧收口）
    ui.note_presented(0, P + 5_000_000, FRAME_PRESENTED, 2);
    assert_eq!(ui.frame_phase(), FramePhase::Settle);

    let out3 = rf(&mut ui, 3 * P);
    assert!(!out3.drew);
    assert_eq!(ui.frame_phase(), FramePhase::Idle);
    assert_eq!(ui.next_wakeup(), None); // IDLE = 撤定时器
    assert_eq!(ui.telemetry().deadline_miss(), 0);
    let _ = root;
}

#[test]
fn idle_strongarm_beats_pending_settle() {
    // 2001ms 静默：在途回调未清也强降 Idle
    let mut ui = UiMain::new(64, 48);
    ui.set_vsync(P, 0);
    ui.root();
    ui.wake(0);
    let out = rf(&mut ui, 0);
    assert!(out.drew);
    assert_eq!(out.expected_present_ns, 0);
    ui.note_submitted();
    let _ = rf(&mut ui, 2_001_000_000);
    assert_eq!(ui.frame_phase(), FramePhase::Idle);
    assert_eq!(ui.next_wakeup(), None);

    // 边界对照：恰 2000ms 静默不裁 → Settle 保持
    let mut ui2 = UiMain::new(64, 48);
    ui2.set_vsync(P, 0);
    ui2.root();
    ui2.wake(0);
    let o = rf(&mut ui2, 0);
    assert!(o.drew);
    ui2.note_submitted();
    let _ = rf(&mut ui2, 2_000_000_000);
    assert_eq!(ui2.frame_phase(), FramePhase::Settle);
}

// ---------- note_presented：actual 轨 / jank / credits ----------

#[test]
fn note_presented_tracks_actual_and_jank_boundary() {
    let mut ui = UiMain::new(64, 48);
    ui.set_vsync(P, 0);
    let root = ui.root();

    // F0：expected = P，deadline = P + P/2 = 24_999_999。恰限期应答非 jank（严格 >）
    ui.wake(P);
    let out0 = rf(&mut ui, P);
    assert!(out0.drew);
    ui.note_submitted();
    let deadline0 = P + P / 2;
    ui.note_presented(0, deadline0, FRAME_PRESENTED, 2);

    // F1：invalidate 造 damage → 超半周期 1ns 应答 → jank + deadline_miss
    ui.invalidate(root).unwrap();
    let out1 = rf(&mut ui, 2 * P);
    assert!(out1.drew);
    assert_eq!(out1.expected_present_ns, 2 * P);
    ui.note_submitted();
    let late1 = 2 * P + P / 2 + 1;
    ui.note_presented(1, late1, FRAME_PRESENTED, 2);

    // F2：丢弃帧轨（discarded 置位、presented 不置）
    ui.invalidate(root).unwrap();
    let _ = rf(&mut ui, 3 * P);
    ui.note_submitted();
    ui.note_presented(2, 3 * P, FRAME_DISCARDED, 1);

    // 未知 seq 优雅 no-op（不 panic、不误记）
    ui.note_presented(999, 0, FRAME_PRESENTED, 2);

    let t = ui.telemetry();
    assert_eq!(t.frames(), 3);
    assert_eq!(t.deadline_miss(), 1);
    let recs: Vec<&FrameRec> = t.last(3).collect();
    assert_eq!(recs.len(), 3);
    // last(n) = 旧→新
    assert_eq!(recs[0].seq, 0);
    assert_eq!(recs[0].expected_ns, P);
    assert_eq!(recs[0].actual_ns, deadline0, "actual 轨回填");
    assert!(recs[0].presented);
    assert!(!recs[0].jank, "恰限期（=deadline）非 jank——严格 > 才计");
    assert_eq!(recs[1].seq, 1);
    assert_eq!(recs[1].actual_ns, late1);
    assert!(recs[1].jank, "超半周期 1ns 即 jank");
    assert!(recs[1].presented);
    assert_eq!(recs[2].seq, 2);
    assert!(recs[2].discarded);
    assert!(!recs[2].presented);
    assert!(!recs[2].jank);
    assert_eq!(recs[2].damage_full, false);
}

#[test]
fn credits_and_inflight_bookkeeping() {
    // 帽常量单源钉板
    assert_eq!(CREDITS_MAX, 2);
    assert_eq!(DAMAGE_MAX, 8);

    let mut ui = UiMain::new(64, 48);
    ui.set_vsync(P, 0);
    ui.root();
    ui.wake(P);
    let out = rf(&mut ui, P);
    assert!(out.drew);

    // credits 饱和：连续 5 次提交簿记不 panic（帽满不提交不轰炸）
    for _ in 0..5 {
        ui.note_submitted();
    }
    // inflight 定容 2：两条同 (seq 0, expected P) 在途——逐条应答清空
    ui.note_presented(0, P + 1_000_000, FRAME_PRESENTED, 2);
    ui.note_presented(0, P + 2_000_000, FRAME_PRESENTED, 0);
    // credits_remaining 越帽（>CREDITS_MAX）也不 panic——内部收口
    ui.note_presented(42, 0, FRAME_PRESENTED, 200);
    assert_eq!(ui.telemetry().deadline_miss(), 0);

    // 在途清空后：无工作 → 下一帧直接 Idle（背压面不卡状态机）
    let out2 = rf(&mut ui, 2 * P);
    assert!(!out2.drew);
    assert_eq!(ui.frame_phase(), FramePhase::Idle);
    assert_eq!(ui.next_wakeup(), None);
}

// ---------- VoteBallot 票面形状 ----------

#[test]
fn vote_hint_shapes_by_phase_and_scene() {
    let mut ui = UiMain::new(64, 48);
    // IDLE：全 0 + BACKGROUND（旧 SCENE_IDLE 档折算）+ sticky 1（停帧粘滞）
    assert_eq!(
        ui.vote_hint(),
        VoteBallot {
            min_hz: 0,
            max_hz: 0,
            preferred_hz: 0,
            scene: Intent::Background as u8,
            sticky: 1
        }
    );

    // Run 常态：{0, 60, 60, INTERACTIVE, sticky 0}——min 0 = 可降频下限
    // （常态不锁底；帧率话语权在协商倒数面 rate_hint_hz）
    ui.set_vsync(P, 0);
    let root = ui.root();
    ui.wake(0);
    assert_eq!(
        ui.vote_hint(),
        VoteBallot {
            min_hz: 0,
            max_hz: 60,
            preferred_hz: 60,
            scene: Intent::Interactive as u8,
            sticky: 0
        }
    );

    // 动画在表 → {30, 120, 60}（动画保底 30——与常态 {0,60,60} 不同档，
    // 动画存在本身是提档信号；scene 位 GUI-8 折算后仍 INTERACTIVE——
    // hz 三元组才是图形相位特化面）
    ui.anim_add(Anim {
        node: root,
        prop: AnimProp::OffsetX,
        from: 0,
        to: 8,
        start_ns: 0,
        dur_ns: 10 * P,
        easing: Easing::Linear,
    })
    .unwrap();
    assert!(ui.anim_active());
    let v = ui.vote_hint();
    assert_eq!(v.scene, Intent::Interactive as u8);
    assert_eq!((v.min_hz, v.max_hz, v.preferred_hz), (30, 120, 60));
    assert_eq!(v.sticky, 0);

    // 清动画 → 回常态
    ui.anim_clear();
    assert!(!ui.anim_active());
    assert_eq!(ui.vote_hint().scene, Intent::Interactive as u8);

    // 周期 0（vsync 未协商）→ 票面不依周期（场景定值表）不 panic；
    // rate_hint_hz 才是协商倒数面（跳过帧诚实 0——上见）
    ui.set_vsync(0, 0);
    let z = ui.vote_hint();
    assert_eq!((z.min_hz, z.max_hz, z.preferred_hz), (0, 60, 60));
    assert_eq!(z.scene, Intent::Interactive as u8);
}

#[test]
fn vote_drag_scene_interacting_ballot() {
    // interacting（任一指 dragging）= 第三信号 → {60,120,120}（旧
    // SCENE_DRAG 档折算后 scene = INTERACTIVE——指尖跟手最高档，压过
    // 动画/常态两档）；抬指后回常态
    let mut ui = UiMain::new(100, 100);
    ui.set_vsync(P, 0);
    let root = ui.root();
    let _pane = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: Color { r: 1, g: 2, b: 3 }, w: Some(40), h: Some(40) }),
            },
        )
        .unwrap();
    ui.layout();
    ui.drain_damage();
    ui.wake(1);

    let down = berry_ui::hit::PtrEvent::from_wire(PHASE_DOWN, 0, 10, 10, 1, 0, 0);
    ui.push_pointer(down).unwrap();
    let _ = ui.pump_input(2);
    // 恰在 slop 圈上（dx=8）：按压 ≠ dragging——票面仍常态（scene 位折算
    // 后常态/拖拽同档 INTERACTIVE，判别信号移 hz 三元组——GUI-8 §4 段①）
    let mv0 = berry_ui::hit::PtrEvent::from_wire(PHASE_MOVE, 0, 18, 10, 3, 0, 0);
    ui.push_pointer(mv0).unwrap();
    let _ = ui.pump_input(4);
    let v0 = ui.vote_hint();
    assert_eq!(v0.scene, Intent::Interactive as u8);
    assert_eq!((v0.min_hz, v0.max_hz, v0.preferred_hz), (0, 60, 60), "圈内挪动不是拖——interacting 不亮");

    // 出圈（dx=15 > slop）→ dragging → INTERACTIVE 票（120 档）
    let mv = berry_ui::hit::PtrEvent::from_wire(PHASE_MOVE, 0, 25, 10, 5, 0, 0);
    ui.push_pointer(mv).unwrap();
    let _ = ui.pump_input(6);
    let v = ui.vote_hint();
    assert_eq!(
        (v.min_hz, v.max_hz, v.preferred_hz, v.scene, v.sticky),
        (60, 120, 120, Intent::Interactive as u8, 0)
    );

    // 抬指 → 手势终止 → 回常态（判别同上：hz 回 {0,60,60}）
    let up = berry_ui::hit::PtrEvent::from_wire(PHASE_UP, 0, 25, 10, 7, 0, 0);
    ui.push_pointer(up).unwrap();
    let _ = ui.pump_input(8);
    let v2 = ui.vote_hint();
    assert_eq!(v2.scene, Intent::Interactive as u8);
    assert_eq!((v2.min_hz, v2.max_hz, v2.preferred_hz), (0, 60, 60));
}

#[test]
#[allow(deprecated)] // 对拍源域 = 退役 SCENE 词表（guiproto 折算表键域）
fn vote_scene_intent_tiers_all_phases() {
    // intent 面（GUI-8 §4 段①/ADR #58 收官新增）：四相位票值逐项断言——
    // scene 位携 berry_intent 六档值，交互谱系（常态/动画/拖拽）恒
    // INTERACTIVE(0x02)、idle 恒 BACKGROUND(0x04)；并与 guiproto
    // `scene_to_intent` 折算表单源对拍（两侧共识钉板——旧词表退役后
    // 映射仍以彼为键域，任何一侧改动此处炸响）。
    let mut ui = UiMain::new(100, 100);
    ui.set_vsync(P, 0);
    let root = ui.root();
    let _pane = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: Color { r: 1, g: 2, b: 3 }, w: Some(40), h: Some(40) }),
            },
        )
        .unwrap();
    ui.layout();
    ui.drain_damage();

    // idle 相位（未醒）：BACKGROUND + sticky 1。
    let v = ui.vote_hint();
    assert_eq!((v.scene, v.sticky), (Intent::Background as u8, 1));
    assert_eq!((v.min_hz, v.max_hz, v.preferred_hz), (0, 0, 0));
    assert_eq!(
        v.scene,
        berry_guiproto::scene_to_intent(berry_guiproto::SCENE_IDLE).unwrap()
    );

    // 常态相位：INTERACTIVE + {0,60,60}。
    ui.wake(1);
    let v = ui.vote_hint();
    assert_eq!(v.scene, Intent::Interactive as u8);
    assert_eq!((v.min_hz, v.max_hz, v.preferred_hz), (0, 60, 60));
    assert_eq!(
        v.scene,
        berry_guiproto::scene_to_intent(berry_guiproto::SCENE_NORMAL).unwrap()
    );

    // 动画相位：INTERACTIVE（旧 ANIMATED 档折算）+ {30,120,60}。
    ui.anim_add(Anim {
        node: root,
        prop: AnimProp::OffsetX,
        from: 0,
        to: 8,
        start_ns: 1,
        dur_ns: 10 * P,
        easing: Easing::Linear,
    })
    .unwrap();
    let v = ui.vote_hint();
    assert_eq!(v.scene, Intent::Interactive as u8);
    assert_eq!((v.min_hz, v.max_hz, v.preferred_hz), (30, 120, 60));
    assert_eq!(
        v.scene,
        berry_guiproto::scene_to_intent(berry_guiproto::SCENE_ANIMATED).unwrap()
    );
    ui.anim_clear();

    // 拖拽相位（interacting 第三信号）：INTERACTIVE（旧 DRAG 档折算）+
    // {60,120,120}。
    ui.wake(2);
    ui.push_pointer(berry_ui::hit::PtrEvent::from_wire(PHASE_DOWN, 0, 10, 10, 3, 0, 0)).unwrap();
    let _ = ui.pump_input(4);
    ui.push_pointer(berry_ui::hit::PtrEvent::from_wire(PHASE_MOVE, 0, 25, 10, 5, 0, 0)).unwrap();
    let _ = ui.pump_input(6);
    let v = ui.vote_hint();
    assert_eq!(v.scene, Intent::Interactive as u8);
    assert_eq!((v.min_hz, v.max_hz, v.preferred_hz), (60, 120, 120));
    assert_eq!(
        v.scene,
        berry_guiproto::scene_to_intent(berry_guiproto::SCENE_DRAG).unwrap()
    );
}

#[test]
fn run_wakeup_min_of_vsync_anim_and_strongarm() {
    // Run 醒点 = min(vsync 拍, 动画期限, 强降闹钟)——三源取小：动画期
    // 限早于拍 → 动画胜；拍早于期限 → 拍胜（Settle 只走强降闹钟单源面
    // 见 settle_confirms_presented_then_idle）
    let mut ui = UiMain::new(64, 48);
    ui.set_vsync(P, 0);
    let root = ui.root();
    // 动画 A：期限 P+16M（早于下一拍 2P≈33.3M）；动画 B：期限 3P+1M
    //（远期——未起始占帧保 Run，供第二检查点）
    ui.anim_add(Anim {
        node: root,
        prop: AnimProp::OffsetX,
        from: 0,
        to: 8,
        start_ns: P + 1_000_000,
        dur_ns: 15_000_000,
        easing: Easing::Linear,
    })
    .unwrap();
    ui.anim_add(Anim {
        node: root,
        prop: AnimProp::OffsetY,
        from: 0,
        to: 4,
        start_ns: 3 * P,
        dur_ns: 1_000_000,
        easing: Easing::Linear,
    })
    .unwrap();

    ui.wake(P + 1_000_000);
    assert!(rf(&mut ui, P + 1_000_000).drew);
    assert_eq!(ui.next_wakeup(), Some(P + 16_000_000), "动画 A 期限胜出（< 2P）");

    // A 完成（位移 damage 产出）；B 未起始占帧 → 仍 Run；下一醒回拍 2P
    assert!(rf(&mut ui, P + 16_000_000).drew);
    assert!(ui.anim_active());
    assert_eq!(ui.next_wakeup(), Some(2 * P), "远期期限下 vsync 拍胜出");
}

#[test]
fn telemetry_ring_wrap_late_fill_noop() {
    // 环 64 覆写：70 帧后 seq 0 已出环——迟到应答（在途表仍持）回填
    // no-op：不误写覆盖位（原槽现持 seq 64）、期限内不产 jank
    let mut ui = UiMain::new(64, 48);
    ui.set_vsync(P, 0);
    ui.root();
    ui.wake(P);
    assert!(rf(&mut ui, P).drew);
    ui.note_submitted(); // 在途 (seq 0, expected P)
    for k in 2u64..=70 {
        assert!(!rf(&mut ui, k * P).drew, "静默期跳帧");
    }
    assert_eq!(ui.telemetry().frames(), 70);

    ui.note_presented(0, P, FRAME_PRESENTED, 2);
    assert_eq!(ui.telemetry().deadline_miss(), 0, "期限内应答零 jank");
    let t = ui.telemetry();
    let recs: Vec<&FrameRec> = t.last(2).collect();
    assert_eq!((recs[0].seq, recs[1].seq), (68, 69), "last(2) = 最新两帧");
    assert!(
        !recs[0].presented && !recs[1].presented,
        "覆写位 no-op——旧应答不污染新帧"
    );
}

#[test]
fn vsync_phase_mod_normalized_hostile_phase() {
    // 敌意 phase ≥ period：mod 归一容忍——(P, P+5) ≡ (P, 5) 同拍；归一
    // 后恰在拍沿 → 本拍即返（含沿不欠拍的敌意版）
    let expected_at = |phase: u64| -> u64 {
        let mut ui = UiMain::new(64, 48);
        ui.set_vsync(P, phase);
        ui.root();
        ui.wake(7);
        rf(&mut ui, 7).expected_present_ns
    };
    assert_eq!(expected_at(5), P + 5);
    assert_eq!(expected_at(P + 5), P + 5, "敌意 phase：mod 归一同拍");
    assert_eq!(expected_at(7), 7, "恰在拍沿 → 本拍即返");
    assert_eq!(expected_at(P + 7), 7);
}

#[test]
fn wake_conditional_full_preserves_incremental_damage() {
    // 条件化 FULL：wake 时已有脏区 → 保留既有矩形集（不放大成整屏；
    // 空集才置 FULL——无条件 FULL 会吞掉先前 invalidate 的精确脏区）
    let mut ui = UiMain::new(64, 48);
    ui.set_vsync(P, 0);
    let root = ui.root();
    let leaf = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps { bg: Color { r: 1, g: 2, b: 3 }, w: Some(10), h: Some(10) }),
            },
        )
        .unwrap();
    ui.layout();
    ui.drain_damage();

    ui.invalidate(leaf).unwrap();
    let d = ui.damage();
    assert_eq!((d.count, d.full), (1, false), "前置自检：局部脏区一条");
    ui.wake(1_000);
    let d = ui.drain_damage();
    assert!(!d.full, "增量 wake 不放大成 FULL");
    assert_eq!(d.count, 1);
    assert_eq!((d.rects[0].x, d.rects[0].y, d.rects[0].w, d.rects[0].h), (0, 0, 10, 10));
}
