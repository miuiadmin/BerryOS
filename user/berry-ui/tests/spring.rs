//! 维三最小刀 SpringProp 钉板（UI-02 §二维三裁决两刀之一）：
//! 收敛性（多组参数终点到位/双阈值吸附）、单调段（Smooth 临界阻尼
//! 无过冲）、Bouncy 气质（欠阻尼过冲后回摆）、重定目标连续（位置与
//! 速度无双跳——中断恢复主干）、初速度方向正确、Q16.16 溢出负例
//! （极端参数钳制不 panic）、引擎表挂载（spring_add → render_frame
//! 同帧推进收敛排空 + Transform/Layout 双 prop 落点 + 死节点纪律）。
//!
//! 确定性：全定值 16ms 帧步推进，无随机无真钟；Q16.16 纯整数路径
//! 同输入同输出。

use berry_ui::anim::{AnimProp, SpringPreset, SpringProp};
use berry_ui::desc::{NodeId, Props, RectProps, ViewSpec};
use berry_ui::Ui;
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};

/// 帧步（= SPRING_SUBSTEP_NS 单子步——每 tick 恰一步，值级可钉）。
const FRAME_NS: u64 = 16_000_000;

/// 解算器测试用哑节点（NodeId 无公共构造面——借真树 root；不经引擎
/// 表，node/prop 字段不被消费）。
fn dummy() -> NodeId {
    Ui::<16>::new(1, 1).root()
}

fn spring(from: i32, target: i32, vel0: i32, preset: SpringPreset) -> SpringProp {
    SpringProp::new(dummy(), AnimProp::OffsetX, from, target, vel0, preset)
}

/// 推进至收敛（guard 防死循环——双阈值不满足 = 逻辑炸响）。
fn drive_to_done(sp: &mut SpringProp, guard: usize) {
    sp.step(0); // 起拍基线（dt=0 持初值）
    let mut now = FRAME_NS;
    let mut n = 0usize;
    while !sp.done() {
        sp.step(now);
        now += FRAME_NS;
        n += 1;
        assert!(n < guard, "{:?} 未在 {guard} 帧内收敛", sp.preset);
    }
}

#[test]
fn convergence_grid_reaches_exact_target() {
    // 多组参数（正向/反向/带初速/远距/零位移零速即时收敛）：双阈值
    // 收敛后 value() = 精确 target（终点钉板——与 anim::tick 完成帧
    // 同语义）。
    let cases: [(i32, i32, i32); 6] = [
        (0, 100, 0),
        (0, 100, 800),
        (100, 0, -800),
        (-50, 250, 300),
        (1000, 0, 0),
        (100, 100, 0),
    ];
    for preset in [SpringPreset::Smooth, SpringPreset::Bouncy] {
        for &(from, target, vel0) in &cases {
            let mut sp = spring(from, target, vel0, preset);
            drive_to_done(&mut sp, 2000);
            assert_eq!(sp.value(), target, "{preset:?} ({from},{target},{vel0})");
        }
    }
}

#[test]
fn smooth_preset_monotonic_no_overshoot() {
    // Smooth = 临界阻尼（离散特征值实且 ∈(0,1)）：整数采样非降（反向
    // 非升）、绝不过冲目标——Q16.16 截断只增阻尼不生逆转。
    for (from, target) in [(0i32, 100i32), (100, 0), (-30, -100)] {
        let mut sp = spring(from, target, 0, SpringPreset::Smooth);
        sp.step(0);
        let mut prev = from;
        let mut now = FRAME_NS;
        while !sp.done() {
            let v = sp.step(now);
            if target >= from {
                assert!(v >= prev && v <= target, "单调段 {from}->{target}: {prev} → {v}");
            } else {
                assert!(v <= prev && v >= target, "单调段 {from}->{target}: {prev} → {v}");
            }
            prev = v;
            now += FRAME_NS;
        }
        assert_eq!(sp.value(), target);
    }
}

#[test]
fn bouncy_preset_overshoots_then_settles() {
    // Bouncy = 欠阻尼（ζ=0.625 → 过冲≈8%）：iOS .bouncy 气质可见回弹，
    // 终值仍精确吸附。
    let mut sp = spring(0, 100, 0, SpringPreset::Bouncy);
    sp.step(0);
    let mut peak = 0i32;
    let mut now = FRAME_NS;
    while !sp.done() {
        peak = peak.max(sp.step(now));
        now += FRAME_NS;
    }
    assert!(peak > 104, "过冲可见（峰值 {peak}，期望 ≈8%）");
    assert_eq!(sp.value(), 100, "回摆后终点钉板");
}

#[test]
fn retarget_continues_from_current_state() {
    // 重定目标 = 以当前位置与当前速度重起算：位置/速度读出在
    // retarget 前后逐位不变（无双跳），续步位移连续，新终点到位。
    // 场景一：新目标在前方（加速追赶）。
    let mut sp = spring(0, 100, 0, SpringPreset::Smooth);
    sp.step(0);
    let mut now = FRAME_NS;
    for _ in 0..10 {
        sp.step(now);
        now += FRAME_NS;
    }
    let pos_before = sp.value();
    let vel_before = sp.vel_px_s();
    assert!(vel_before > 0, "在途向目标运动（vel={vel_before}）");
    sp.retarget(200);
    assert_eq!(sp.value(), pos_before, "重定目标位置无跳变");
    assert_eq!(sp.vel_px_s(), vel_before, "速度续接（无双跳）");
    let v_next = sp.step(now);
    let delta = v_next - pos_before;
    assert!(delta > 0 && delta <= 20, "续步连续同向（Δ={delta}）");
    while !sp.done() {
        sp.step(now);
        now += FRAME_NS;
    }
    assert_eq!(sp.value(), 200, "新终点到位");

    // 场景二：新目标折返到身后（速度先被吃掉再反向——终点跨当前值）。
    let mut sp2 = spring(0, 100, 0, SpringPreset::Smooth);
    sp2.step(0);
    let mut now2 = FRAME_NS;
    for _ in 0..10 {
        sp2.step(now2);
        now2 += FRAME_NS;
    }
    let mid = sp2.value();
    assert!(mid > 0 && mid < 100);
    sp2.retarget(0);
    while !sp2.done() {
        sp2.step(now2);
        now2 += FRAME_NS;
    }
    assert_eq!(sp2.value(), 0, "折返终点到位");
}

#[test]
fn initial_velocity_points_motion() {
    // 起手在目标位、带初速度：首步位移方向 = 初速度方向（速度续接
    // 主干——松手速度直入的等价物）；带速起跳不即时收敛。
    for (vel0, positive) in [(600i32, true), (-600, false)] {
        let mut sp = spring(100, 100, vel0, SpringPreset::Bouncy);
        assert_eq!(sp.step(0), 100, "起拍 dt=0 持初值");
        let v = sp.step(FRAME_NS);
        assert_eq!(v > 100, positive, "初速度 {vel0} 方向主导首步（v={v}）");
        assert!(!sp.done(), "带速起跳未即时收敛");
        drive_to_done(&mut sp, 2000);
        assert_eq!(sp.value(), 100);
    }
}

#[test]
fn extreme_params_clamped_without_panic() {
    // Q16.16 溢出负例：i32 端点位移 + 巨初速度 + 秒级时钟大跳（子步帽
    // 慢动作）——全程不 panic，value() 恒 i32 域内（饱和钳制），有限
    // 收敛吸附目标。
    for preset in [SpringPreset::Smooth, SpringPreset::Bouncy] {
        let mut sp = spring(i32::MIN, i32::MAX, i32::MAX, preset);
        assert_eq!(sp.step(0), i32::MIN, "起拍持初值");
        let mut now = 10_000_000_000u64; // 首步 10s 大跳 → 16 子步帽
        let mut guard = 0usize;
        while !sp.done() {
            let _ = sp.step(now);
            now += 1_000_000_000;
            guard += 1;
            assert!(guard < 200, "{preset:?} 极端参数应有限收敛（已 {guard} tick）");
        }
        assert_eq!(sp.value(), i32::MAX, "吸附目标（终点钉板）");
    }
}

// —— 引擎表挂载（spring_add → render_frame 同帧推进）——

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

fn rect_spec(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: berry_ui::ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(w), h: Some(h) }),
    }
}

fn settled(ui: &mut Ui16) -> berry_ui::NodeId {
    let root = ui.root();
    let a = ui.add(root, rect_spec(10, 10)).unwrap();
    ui.layout();
    ui.drain_damage();
    a
}

type Ui16 = berry_ui::Ui<16>;

fn frame(ui: &mut Ui16, now_ns: u64) -> berry_ui::frame::FrameOutcome {
    let mut enc = CommandEncoder::new();
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    ui.render_frame(now_ns, &mut scratch, &mut enc)
}

#[test]
fn engine_spring_transform_converges_and_drains() {
    // OffsetX（Transform 型）：经 anim::apply 同一落点直写 ox；收敛后
    // 表排空（anim_active 假——帧循环可落 Idle 的反卡死面）。
    let mut ui = Ui16::new(100, 100);
    let a = settled(&mut ui);
    ui.spring_add(SpringProp::new(a, AnimProp::OffsetX, 0, 100, 0, SpringPreset::Smooth))
        .unwrap();
    assert!(ui.anim_active(), "弹簧入表即占用帧");
    let mut now = 1_000u64;
    let mut frames = 0usize;
    while ui.anim_active() {
        frame(&mut ui, now);
        now += FRAME_NS;
        frames += 1;
        assert!(frames < 1000, "有限收敛");
    }
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(100, 0, 10, 10), "终点钉板");
    // 毕后零动静（无 damage 不提交——现状帧循环纪律同源）
    let out = frame(&mut ui, now + FRAME_NS);
    assert!(!out.drew, "弹簧毕再帧不提交");
}

#[test]
fn engine_spring_layout_prop_and_dead_node_discipline() {
    // Width（Layout 型）：props 定值 + relayout 消费——prop 经同一
    // apply 分派（Spring 是驱动方式，五属性皆可指认）。
    let mut ui = Ui16::new(100, 100);
    let a = settled(&mut ui);
    ui.spring_add(SpringProp::new(a, AnimProp::Width, 10, 40, 0, SpringPreset::Bouncy))
        .unwrap();
    let mut now = 1_000u64;
    let mut frames = 0usize;
    while ui.anim_active() {
        frame(&mut ui, now);
        now += FRAME_NS;
        frames += 1;
        assert!(frames < 1000);
    }
    let b = ui.bounds(a).unwrap();
    assert_eq!((b.x, b.w, b.h), (0, 40, 10), "Width 弹簧终点（Layout 消费）");

    // 死节点弹簧：apply no-op 但照常收敛移除（与 anim 表同纪律——
    // anim_active 卡死 = 帧循环永不 Idle 的电量坑）。
    ui.remove(a).unwrap();
    ui.spring_add(SpringProp::new(a, AnimProp::OffsetX, 0, 100, 0, SpringPreset::Smooth))
        .unwrap();
    assert!(ui.anim_active());
    let mut frames = 0usize;
    while ui.anim_active() {
        frame(&mut ui, now);
        now += FRAME_NS;
        frames += 1;
        assert!(frames < 1000, "死节点弹簧照常收敛移除");
    }
}

#[test]
fn engine_spring_cap_shares_anim_full() {
    // 帽/错位与 anims 表同源：满 16 条后第 17 条 AnimFull loud；
    // anim_clear 全撤双面一体。
    let mut ui = Ui16::new(100, 100);
    let a = settled(&mut ui);
    for i in 0..berry_ui::ANIM_MAX {
        ui.spring_add(SpringProp::new(
            a,
            AnimProp::OffsetX,
            0,
            10 + i as i32,
            0,
            SpringPreset::Smooth,
        ))
        .unwrap();
    }
    assert_eq!(
        ui.spring_add(SpringProp::new(a, AnimProp::OffsetX, 0, 1, 0, SpringPreset::Smooth)),
        Err(berry_ui::UiError::AnimFull),
        "弹簧表满 loud（与 anim_add 同错位）"
    );
    ui.anim_clear();
    assert!(!ui.anim_active(), "anim_clear 双面全撤");
}
