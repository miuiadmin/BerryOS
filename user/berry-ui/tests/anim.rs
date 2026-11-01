//! GUI-3 §8 anim 行：tick 推进（两缓动/终点钉板/结束不再动/Transform
//! 不重排）+ anim_add 满帽 AnimFull + anim_clear + 死节点动画清理。
//!
//! - Linear 中点值级 + 终点精确 `to`（完成即移除——后续帧零动静）。
//! - StepEnd 恒持 from、终点跳 to。
//! - Offset* = Transform 型：只落 paint 偏移（bounds 含 ox/oy 显形），
//!  **不触发重排**（layout() 全缓存命中）；damage = 新旧界包络。
//! - Width/Gap = Layout 型：改 props + 重排消费（新 bounds 生效）。
//! - 未起始占帧（anim_active 真、不应用）；remove 连带清动画。
//!
//! 确定性：全定值时刻推进，无随机无真钟。

use berry_ui::anim::{Anim, AnimProp, Easing};
use berry_ui::desc::{ColProps, Props, RectProps, ViewSpec};
use berry_ui::layout::LayoutStat;
use berry_ui::{Ui, UiError, ViewKind};
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos};

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

/// 已静默的 A(10x10) 单叶树（首排毕 + damage 排空）。
fn settled(ui: &mut Ui<16>) -> berry_ui::NodeId {
    let root = ui.root();
    let a = ui.add(root, rect_spec(10, 10)).unwrap();
    ui.layout();
    ui.drain_damage();
    a
}

fn frame(ui: &mut Ui<16>, now_ns: u64) -> berry_ui::frame::FrameOutcome {
    let mut enc = CommandEncoder::new();
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    ui.render_frame(now_ns, &mut scratch, &mut enc)
}

#[test]
fn linear_offset_midpoint_envelope_and_endpoint() {
    let mut ui = Ui::<16>::new(100, 100);
    let a = settled(&mut ui);
    ui.anim_add(Anim {
        node: a,
        prop: AnimProp::OffsetX,
        from: 0,
        to: 10,
        start_ns: 1000,
        dur_ns: 10_000,
        easing: Easing::Linear,
    })
    .unwrap();
    assert!(ui.anim_active());

    // 中点 now=6000：elapsed/dur=1/2 → ox=5；damage = 新旧界包络 (0,0,15,10)
    let out = frame(&mut ui, 6000);
    assert!(out.drew);
    assert_eq!(out.damage.count, 1);
    assert_eq!(out.damage.rects[0], berry_guiproto::Rect { x: 0, y: 0, w: 15, h: 10 });
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(5, 0, 10, 10));
    // Transform 型不重排：全缓存命中
    assert_eq!(ui.layout(), LayoutStat { measured: 0, placed: 0 });
    assert!(ui.damage().is_empty(), "帧末已排空");

    // 终点 now=11000 ≥ start+dur：精确 to=10、完成移除
    let out = frame(&mut ui, 11_000);
    assert!(out.drew);
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(10, 0, 10, 10));
    assert!(!ui.anim_active(), "完成项已移除");

    // 结束态再推一帧：零动静（无 damage 不提交）
    let out = frame(&mut ui, 12_000);
    assert!(!out.drew, "动画毕再帧不提交");
    assert!(out.damage.is_empty());
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(10, 0, 10, 10));
}

#[test]
fn step_end_holds_from_until_exact_end() {
    let mut ui = Ui::<16>::new(100, 100);
    let a = settled(&mut ui);
    ui.anim_add(Anim {
        node: a,
        prop: AnimProp::OffsetY,
        from: 0,
        to: 10,
        start_ns: 1000,
        dur_ns: 10_000,
        easing: Easing::StepEnd,
    })
    .unwrap();

    // 中点：恒持 from=0（不插值）
    frame(&mut ui, 6000);
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(0, 0, 10, 10));
    assert!(ui.anim_active());
    // 终点帧：跳到精确 to
    frame(&mut ui, 11_000);
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(0, 10, 10, 10));
    assert!(!ui.anim_active());
}

#[test]
fn width_layout_anim_consumed_by_relayout() {
    let mut ui = Ui::<16>::new(100, 100);
    let a = settled(&mut ui);
    ui.anim_add(Anim {
        node: a,
        prop: AnimProp::Width,
        from: 10,
        to: 30,
        start_ns: 1000,
        dur_ns: 10_000,
        easing: Easing::Linear,
    })
    .unwrap();

    // 中点：v=20 → props w=Some(20) → 帧内重排消费
    frame(&mut ui, 6000);
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(0, 0, 20, 10));
    // 终点：v=30
    let out = frame(&mut ui, 11_000);
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(0, 0, 30, 10));
    // Layout 型 damage：A 脏位沿链上传 → Root 重测重摆（old+new 同界照记
    // ——保守并集），其 (0,0,100,100) 全含 A 旧界 (0,0,20,10) 与新界
    // (0,0,30,10) → 合并后单矩形 = Root 界（确定性钉板）。
    assert_eq!(out.damage.count, 1);
    assert_eq!(out.damage.rects[0], berry_guiproto::Rect { x: 0, y: 0, w: 100, h: 100 });
}

#[test]
fn gap_layout_anim_moves_sibling() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let col = ui
        .add(root, ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 0 }) })
        .unwrap();
    let a = ui.add(col, rect_spec(10, 10)).unwrap();
    let b = ui.add(col, rect_spec(10, 10)).unwrap();
    ui.layout();
    ui.drain_damage();
    assert_eq!(ui.bounds(b).unwrap(), berry_ui::damage::RectI::new(0, 10, 10, 10));

    ui.anim_add(Anim {
        node: col,
        prop: AnimProp::Gap,
        from: 0,
        to: 8,
        start_ns: 1000,
        dur_ns: 10_000,
        easing: Easing::Linear,
    })
    .unwrap();
    frame(&mut ui, 11_000);
    // gap=8：B 下移 18；Col 主轴 = 10+8+10 = 28
    assert_eq!(ui.bounds(b).unwrap(), berry_ui::damage::RectI::new(0, 18, 10, 10));
    assert_eq!(ui.bounds(col).unwrap(), berry_ui::damage::RectI::new(0, 0, 100, 28));
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(0, 0, 10, 10));
}

#[test]
fn anim_full_at_sixteen_then_clear() {
    let mut ui = Ui::<16>::new(100, 100);
    let a = settled(&mut ui);
    for i in 0..berry_ui::ANIM_MAX {
        ui.anim_add(Anim {
            node: a,
            prop: AnimProp::OffsetX,
            from: 0,
            to: 10,
            start_ns: 100_000 + i as u64, // 互异 start 防互相干扰
            dur_ns: 10_000,
            easing: Easing::Linear,
        })
        .unwrap();
    }
    assert!(ui.anim_active());
    // 第 17 条 → AnimFull loud
    assert_eq!(
        ui.anim_add(Anim {
            node: a,
            prop: AnimProp::OffsetX,
            from: 0,
            to: 1,
            start_ns: 1,
            dur_ns: 1,
            easing: Easing::Linear,
        }),
        Err(UiError::AnimFull)
    );

    // anim_clear 全撤：now 过全部 start 亦零动静
    ui.anim_clear();
    assert!(!ui.anim_active());
    let out = frame(&mut ui, 1_000_000);
    assert!(!out.drew, "清空后无动画无脏——不提交");
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(0, 0, 10, 10));
}

#[test]
fn not_started_anim_occupies_frame_without_applying() {
    let mut ui = Ui::<16>::new(100, 100);
    let a = settled(&mut ui);
    ui.anim_add(Anim {
        node: a,
        prop: AnimProp::OffsetX,
        from: 0,
        to: 10,
        start_ns: 10_000,
        dur_ns: 10_000,
        easing: Easing::Linear,
    })
    .unwrap();
    // now=5000 < start：占帧（anim_active 真）但不应用
    let out = frame(&mut ui, 5000);
    assert!(!out.drew, "未起始无动静——不提交");
    assert!(ui.anim_active(), "未起始项占帧（帧循环保持 RUN）");
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(0, 0, 10, 10));
    // 到点照常推进
    frame(&mut ui, 20_000);
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(10, 0, 10, 10));
    assert!(!ui.anim_active());
}

#[test]
fn remove_node_drops_attached_anims() {
    let mut ui = Ui::<16>::new(100, 100);
    let a = settled(&mut ui);
    ui.anim_add(Anim {
        node: a,
        prop: AnimProp::OffsetX,
        from: 0,
        to: 10,
        start_ns: 1000,
        dur_ns: 10_000,
        easing: Easing::Linear,
    })
    .unwrap();
    assert!(ui.anim_active());
    // remove 连带清随行动画（悬空 id 判据 = 代数失配）
    ui.remove(a).unwrap();
    assert!(!ui.anim_active(), "remove 清动画——防 anim_active 卡死");

    // anim_add 不验活（§6 注记）：死 id 可入表，tick 侧不应用但完成移除
    ui.anim_add(Anim {
        node: a,
        prop: AnimProp::OffsetX,
        from: 0,
        to: 10,
        start_ns: 1000,
        dur_ns: 10_000,
        easing: Easing::Linear,
    })
    .unwrap();
    assert!(ui.anim_active());
    frame(&mut ui, 11_000);
    assert!(!ui.anim_active(), "死节点动画完成即移除");
}

#[test]
fn offset_damage_envelopes_descendant_footprint() {
    // v1 无裁剪：子显式宽可越父分配（Col 十字轴吃满父给、子 explicit w
    // 优先）——动画祖先 Offset 的 damage 必须覆盖**子树视觉足迹**（子
    // 随祖先挪动；只记动画节点自身界会漏子树尾影——§3.4 残像素对策）
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let col = ui
        .add(root, ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 0 }) })
        .unwrap();
    let big = ui.add(col, rect_spec(200, 20)).unwrap();
    ui.layout();
    ui.drain_damage();
    // 前置自检：子确实越出 Col 界（Col 十字轴 = 父分配 100，子 200）
    assert_eq!(ui.bounds(col).unwrap(), berry_ui::damage::RectI::new(0, 0, 100, 20));
    assert_eq!(ui.bounds(big).unwrap(), berry_ui::damage::RectI::new(0, 0, 200, 20));

    ui.anim_add(Anim {
        node: col,
        prop: AnimProp::OffsetX,
        from: 0,
        to: 10,
        start_ns: 1000,
        dur_ns: 10_000,
        easing: Easing::Linear,
    })
    .unwrap();
    // 中点 ox=5：新旧子树包络并 = (0,0,205,20)——含子越界段 [100,205)；
    // 自身界口径只记 (0,0,105,20)——本断言即两口径分界钉板
    let out = frame(&mut ui, 6000);
    assert!(out.drew);
    assert_eq!(out.damage.count, 1);
    assert_eq!(out.damage.rects[0], berry_guiproto::Rect { x: 0, y: 0, w: 205, h: 20 });
}

#[test]
fn prop_kind_mismatch_no_op_but_completes() {
    // Gap 动画挂在 Rect 节点（prop-kind 不匹配）→ 视觉 no-op（零
    // damage 不提交），但完成项照移除——不卡帧循环（anim_active 卡真
    // = RUN 永不落 Settle/Idle 的反卡死面）
    let mut ui = Ui::<16>::new(100, 100);
    let a = settled(&mut ui);
    ui.anim_add(Anim {
        node: a,
        prop: AnimProp::Gap,
        from: 0,
        to: 8,
        start_ns: 1000,
        dur_ns: 10_000,
        easing: Easing::Linear,
    })
    .unwrap();
    assert!(ui.anim_active());
    let out = frame(&mut ui, 6000);
    assert!(!out.drew, "no-op 无 damage——不提交");
    assert!(ui.anim_active(), "未完成仍占帧");
    let out2 = frame(&mut ui, 11_000);
    assert!(!out2.drew);
    assert!(!ui.anim_active(), "完成即移除——prop 不匹配不卡死");
    assert_eq!(ui.bounds(a).unwrap(), berry_ui::damage::RectI::new(0, 0, 10, 10));
}
