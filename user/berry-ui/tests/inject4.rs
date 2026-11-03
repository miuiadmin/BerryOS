//! 段③ selftest 合成注入钉板——复刻 berry-launcher `inject_swipe` 原值
//! （800×600、x0=vp_w/2、step=vp_w/20、xf=vp_w/25、ts 基 +0..3 **ns**、
//! BUTTON_L、pressure 500、单泵全序消费）→ 两轮翻页 0→1→2。
//!
//! 起因：smoke-image 4 事件形 FAIL 缺 PAGE ok——host 面钉死注入序列在
//! 引擎侧的产出 Msg 流（Press/DragStart/DragMove[resampled=false]/
//! DragEnd[vx 钳 −32768]），QEMU 侧失配 = 集成面而非引擎面。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};
use berry_guiproto::{BUTTON_L, PHASE_DOWN, PHASE_MOVE, PHASE_UP};
use berry_ui::desc::{Axis, ColProps, Msg, Props, RectProps, ViewSpec};
use berry_ui::hit::PtrEvent;
use berry_ui::layout::CrossAlign;
use berry_ui::swipe::{SwipePager, SNAP_DUR_NS};
use berry_ui::{Ui, ViewKind};

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

fn rect_spec(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(w), h: Some(h) }),
    }
}

/// launcher inject_swipe 复刻：4 事件单泵全序（ts 单位 = ns 原样）。
/// 返回 pump 产出 Msg 流（不喂 pager——由调用方按 launcher 分发序喂）。
fn inject4(ui: &mut Ui<64>, base: u64) -> Vec<Msg> {
    let (vp_w, vp_h) = ui.viewport();
    let x0 = vp_w / 2;
    let y = vp_h / 2;
    let step = (vp_w / 20).max(9);
    let xf = (vp_w / 25).max(1);
    let m1 = x0.saturating_sub(step);
    let xe = xf.min(m1);
    let ev = |phase: u8, x: u32, dt: u64| PtrEvent {
        phase,
        pointer_id: 1,
        x,
        y,
        ts_ns: base + dt * 1_000_000, // dt = 毫秒（launcher 同构）
        buttons: BUTTON_L,
        pressure_mi: 500,
    };
    for (phase, x, dt) in [
        (PHASE_DOWN, x0, 0u64),
        (PHASE_MOVE, m1, 1),
        (PHASE_MOVE, xe, 2),
        (PHASE_UP, xe, 3),
    ] {
        ui.push_pointer(ev(phase, x, dt)).unwrap();
    }
    let out = ui.pump_input(base + 16_666_666); // 真 target（前向外推几何）
    out.iter().copied().collect()
}

/// 吸附收尾：起拍帧 → 完成帧 → 毕后零开销（swipe.rs 同款）。
fn settle(pager: &mut SwipePager, ui: &mut Ui<64>, t0: u64) {
    assert!(pager.tick(ui, t0), "吸附起拍帧");
    assert!(pager.tick(ui, t0 + SNAP_DUR_NS), "吸附完成帧");
    assert!(!pager.tick(ui, t0 + SNAP_DUR_NS + 1), "吸附毕再 tick 零开销");
}

#[test]
fn launcher_inject4_two_flips() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    // swipe.rs 同形：attach 直挂 Root（Root 对子满幅 z-stack——页带满幅
    // 800×600；桌面真实树中带位几何由 desktop.rs 钉，此处在引擎面复刻
    // 注入序列的命中/翻页机制）。
    let mut pager = SwipePager::new(4);
    let _track = pager.attach(&mut ui, root).unwrap();
    // 页内容（desktop.rs 同语义：页内 Grid 满幅——空 Col hug 成 h=0 会让
    // snap 后 (vp/2,vp/2) 无可命中节点，第二轮零 Msg）。
    for p in 0..4 {
        let page = pager.page_node(p).unwrap();
        ui.add(page, rect_spec(800, 600)).unwrap();
    }
    ui.layout();
    ui.drain_damage();

    // —— 第一轮：page 0 → 1 ——
    let base = 10_000_000_000u64;
    let msgs = inject4(&mut ui, base);
    let node0 = match msgs.first() {
        Some(Msg::Press { node, .. }) => *node,
        other => panic!("首 Msg 非 Press: {other:?}"),
    };
    assert_eq!(
        msgs,
        vec![
            Msg::Press { node: node0, x: 400, y: 300 },
            Msg::DragStart { node: node0, axis: Axis::X, x: 360, y: 300 },
            Msg::DragMove { node: node0, x: 32, y: 300, resampled: false },
            Msg::DragEnd { node: node0, vx: -32768, vy: 0 },
        ],
        "4 事件注入 Msg 流值级"
    );
    for m in &msgs {
        pager.on_msg(&mut ui, m);
    }
    assert_eq!(pager.page(), 1, "DragEnd 速度向（vx 钳 −32768 < −500）即翻页");
    settle(&mut pager, &mut ui, base + 1_000_000);
    assert_eq!(pager.page(), 1);

    // —— 第二轮：page 1 → 2（视觉 ox=−800——(400,300) 现落 page1 节点）——
    eprintln!("node_at(400,300) = {:?}", ui.node_at(400, 300));
    for p in 0..4 {
        eprintln!("page{} = {:?} bounds={:?}", p, pager.page_node(p), ui.bounds(pager.page_node(p).unwrap()));
    }
    eprintln!("track bounds = {:?}", ui.bounds(_track));
    let base2 = base + 100_000_000;
    let msgs2 = inject4(&mut ui, base2);
    let node1 = match msgs2.first() {
        Some(Msg::Press { node, .. }) => *node,
        other => panic!("第二轮首 Msg 非 Press: {other:?}"),
    };
    assert_eq!(
        msgs2,
        vec![
            Msg::Press { node: node1, x: 400, y: 300 },
            Msg::DragStart { node: node1, axis: Axis::X, x: 360, y: 300 },
            Msg::DragMove { node: node1, x: 32, y: 300, resampled: false },
            Msg::DragEnd { node: node1, vx: -32768, vy: 0 },
        ],
        "第二轮 4 事件 Msg 流值级"
    );
    for m in &msgs2 {
        pager.on_msg(&mut ui, m);
    }
    assert_eq!(pager.page(), 2);
    settle(&mut pager, &mut ui, base2 + 1_000_000);
    assert_eq!(pager.page(), 2, "两轮各落定——PAGE ok n=2 判据同构");

    // 附：display list 仍可产（结账面——GlyphPos scratch 未用，防 unused 警）。
    let mut enc = CommandEncoder::new();
    let mut scratch = [berry_gfx::cmd::GlyphPos { ch: 0, x: 0, y: 0 }; 8];
    let out = ui.render_frame(base2 + 200_000_000, &mut scratch, &mut enc);
    assert!(out.drew, "翻页后仍有帧产出");
}

/// launcher inject_swipe 同构（push + wake，不 pump——泵在主循环顶）。
fn inject_push(ui: &mut Ui<64>, t_now: u64) {
    let (vp_w, vp_h) = ui.viewport();
    let x0 = vp_w / 2;
    let y = vp_h / 2;
    let step = (vp_w / 20).max(9);
    let xf = (vp_w / 25).max(1);
    let m1 = x0.saturating_sub(step);
    let xe = xf.min(m1);
    for (phase, x, dt) in [
        (PHASE_DOWN, x0, 0u64),
        (PHASE_MOVE, m1, 1),
        (PHASE_MOVE, xe, 2),
        (PHASE_UP, xe, 3),
    ] {
        ui.push_pointer(PtrEvent {
            phase,
            pointer_id: 1,
            x,
            y,
            ts_ns: t_now + dt * 1_000_000,
            buttons: BUTTON_L,
            pressure_mi: 500,
        })
        .unwrap();
    }
    ui.wake(t_now);
}

/// launcher 帧循环 host 复刻（frameloop.rs run() 同构）：手动时钟 =
/// wait_many 语义（醒点 = min(next_wakeup, now+10s 帽)，None → 帽），
/// 逐迭代打 phase/drew/damage/next_wakeup/tick——QEMU 首轮 stall 的
/// 逐帧真相仪。
#[test]
fn launcher_loop_replica() {
    let mut ui = Ui::<64>::new(800, 600);
    let root = ui.root();
    let mut pager = SwipePager::new(4);
    let _track = pager.attach(&mut ui, root).unwrap();
    for p in 0..4 {
        ui.add(pager.page_node(p).unwrap(), rect_spec(800, 600)).unwrap();
    }
    ui.layout();
    ui.drain_damage();

    ui.set_vsync(16_666_666, 0);
    let mut t = 10_000_000_000u64;
    ui.wake(t);

    let mut scratch = [berry_gfx::cmd::GlyphPos { ch: 0, x: 0, y: 0 }; 64];

    let mut last_expected = 0u64;
    let mut presented = 0u64;
    let mut launched = false;
    // 0=WaitFirst 1=Settle1 2=Settle2 3=Done
    let mut st = 0u8;
    let mut pc = 0u64;
    let mut deadline = 0u64;

    for it in 0..32usize {
        let t_now = t;
        // pump + 分发（a11y rehang 无视觉面——略）
        let msgs: Vec<Msg> = ui.pump_input(last_expected).iter().copied().collect();
        for m in &msgs {
            pager.on_msg(&mut ui, m);
        }
        let pager_anim = pager.tick(&mut ui, t_now);
        let mut enc = CommandEncoder::new(); // 循环内新建（'a 不钉 scratch——launcher 同形）
        let out = ui.render_frame(t_now, &mut scratch, &mut enc);
        last_expected = out.expected_present_ns;
        if out.drew {
            ui.note_submitted();
            ui.note_presented(
                (ui.telemetry().frames() - 1) as u32,
                t_now,
                0, // FRAME_PRESENTED
                1, // credits_remaining（在途 ≤1 结构性背压）
            );
            presented += 1;
            if !launched {
                launched = true; // LAUNCH ok（首帧呈现回执过链）
            }
        }
        eprintln!(
            "it={:02} t=+{:>9}ms st={} msgs={} drew={} dmg_full={} tick={} page={} nw={:?}",
            it,
            (t_now - 10_000_000_000) / 1_000_000,
            st,
            msgs.len(),
            out.drew,
            out.damage.full,
            pager_anim,
            pager.page(),
            ui.next_wakeup().map(|v| (v - 10_000_000_000) / 1_000_000),
        );

        match st {
            0 if presented > 0 => {
                inject_push(&mut ui, t_now);
                pc = presented;
                deadline = t_now + 5_000_000_000;
                st = 1;
            }
            1 => {
                // 终态判据同构：裁定 + 帧流（不等动画完成帧——竞态面见
                // frameloop.rs Settle1 注）。
                if pager.page() == 1 && presented > pc {
                    inject_push(&mut ui, t_now);
                    pc = presented;
                    deadline = t_now + 5_000_000_000;
                    st = 2;
                    eprintln!("    >> Settle1 ok + 第二轮注入 @+{}ms", (t_now - 10_000_000_000) / 1_000_000);
                } else if t_now > deadline {
                    eprintln!(
                        "    >> TIMEOUT page1 (page={}) —— tick={} presented={}>{}",
                        pager.page(), pager_anim, presented, pc
                    );
                    st = 3;
                }
            }
            2 => {
                if pager.page() == 2 && presented > pc {
                    st = 3;
                    eprintln!("    >> PAGE ok n=2（判据达成）");
                } else if t_now > deadline {
                    eprintln!(
                        "    >> TIMEOUT page2 (page={}) —— tick={}",
                        pager.page(), pager_anim
                    );
                    st = 3;
                }
            }
            _ => {}
        }
        if st == 3 && it > 26 {
            break;
        }

        // wait_many 同构：醒点 = min(next_wakeup, now+10s)
        let cap = t_now + 10_000_000_000;
        let dl = ui.next_wakeup().map_or(cap, |x| x.min(cap));
        t = if dl <= t_now { t_now + 1 } else { dl };
    }
    assert_eq!(pager.page(), 2, "复刻主循环应达 page2（PAGE ok 判据）");
}
