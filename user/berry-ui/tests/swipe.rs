//! GUI-6 D16 SwipePager 钉板（§11 项 15）：attach 建树（页行/圆点行/
//! 页宽=viewport）+ 翻页三判据（速度 ≥500px/s / 位移 >40% 页宽 / 回弹）
//! + 橡皮筋 ×0.5 + QuadOut 240ms 吸附（tick 补面）+ 圆点高亮直摆 +
//! page_count 域 1..=8 loud + 未 attach no-op + 与宿主树近帽（节点数
//! 精确账 + MAX_NODES 余量）。
//!
//! 确定性：Msg 直构（不走 pump——referee 面已由 input 行钉）+ 定值
//! tick 时刻，全值级断言。

use berry_gfx::cmd::{Cmd, Color, CommandEncoder, GlyphPos};
use berry_ui::desc::{Axis, Msg, Props, RectProps, ViewSpec};
use berry_ui::swipe::{SwipePager, DOT_OFF, DOT_ON, FLIP_VEL_PX_S, SNAP_DUR_NS, SWIPE_PAGES_MAX};
use berry_ui::{Ui, UiError, ViewKind};

fn rect_spec(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: Color { r: 1, g: 2, b: 3 }, w: Some(w), h: Some(h) }),
    }
}

/// 3 页 pager @ viewport 100×100 挂 Root（页行满幅、圆点行覆叠其上）。
fn pager3() -> (Ui<64>, SwipePager, berry_ui::NodeId) {
    let mut ui = Ui::<64>::new(100, 100);
    let root = ui.root();
    let mut pager = SwipePager::new(3);
    let track = pager.attach(&mut ui, root).unwrap();
    ui.layout();
    ui.drain_damage();
    (ui, pager, track)
}

fn drag_start(pager: &mut SwipePager, ui: &mut Ui<64>, node: berry_ui::NodeId, x: u32) -> bool {
    pager.on_msg(ui, &Msg::DragStart { node, axis: Axis::X, x, y: 50 })
}

fn drag_move(pager: &mut SwipePager, ui: &mut Ui<64>, node: berry_ui::NodeId, x: u32) -> bool {
    pager.on_msg(ui, &Msg::DragMove { node, x, y: 50, resampled: false })
}

fn drag_end(pager: &mut SwipePager, ui: &mut Ui<64>, node: berry_ui::NodeId, vx: i32) -> bool {
    pager.on_msg(ui, &Msg::DragEnd { node, vx, vy: 0 })
}

/// 吸附全程收尾：起拍帧（t=0 持 from）→ 完成帧（精确 to）→ 毕后零开销。
fn settle(pager: &mut SwipePager, ui: &mut Ui<64>, t0: u64) {
    assert!(pager.tick(ui, t0), "吸附起拍帧");
    assert!(pager.tick(ui, t0 + SNAP_DUR_NS), "吸附完成帧");
    assert!(!pager.tick(ui, t0 + SNAP_DUR_NS + 1), "吸附毕再 tick 零开销");
}

/// 页 0 绝对 x（= track ox + 页 0 布局位 0）——跟手/吸附的值级读出面。
fn page0_x(ui: &Ui<64>, pager: &SwipePager) -> i32 {
    ui.bounds(pager.page_node(0).unwrap()).unwrap().x
}

/// 收集 display list 中 8×8 @ y=0 的圆点填色（(x, color) 序）。
fn dot_colors(dl: &berry_gfx::cmd::DisplayList) -> Vec<(i32, Color)> {
    let mut out = Vec::new();
    for c in dl.iter() {
        if let Cmd::FillRect { rect, paint } = c {
            if rect.w == 16 && rect.h == 16 && rect.y == 0 {
                out.push((rect.x, paint.color));
            }
        }
    }
    out
}

#[test]
fn attach_builds_tree_and_domain_loud() {
    let (ui, pager, track) = pager3();

    // 建树：页行满幅 (0,0,100,100)；三页各 100 宽横排（空页 h=0——内容
    // 由 app 经 page_node 填）；圆点行 (0,0) 起 16×16 三点 + 16 gap（OH indicator 档）。
    assert_eq!(ui.bounds(track).unwrap(), berry_ui::damage::RectI::new(0, 0, 100, 100));
    for (i, expect_x) in [0, 100, 200].into_iter().enumerate() {
        let p = pager.page_node(i as u8).expect("页 0..3 在域内");
        let b = ui.bounds(p).unwrap();
        assert_eq!((b.x, b.w), (expect_x, 100), "页 {i} 横排步进 = 页宽");
    }
    assert_eq!(pager.page_node(3), None, "域外页 → None");
    assert_eq!(pager.page(), 0);
    // 圆点：三 dot @ x=0/32/64（16 边 + 16 gap）
    for expect_x in [0u32, 32, 64] {
        let hit = ui.node_at(expect_x + 8, 8).expect("圆点可命中");
        let b = ui.bounds(hit).unwrap();
        assert_eq!((b.x, b.y, b.w, b.h), (expect_x as i32, 0, 16, 16));
    }

    // page_count 域端点合法（0/9 的 panic 面见下两测）
    let _ = SwipePager::new(1);
    let _ = SwipePager::new(SWIPE_PAGES_MAX);

    // 未 attach → on_msg 恒 false
    let mut fresh = Ui::<16>::new(100, 100);
    let root2 = fresh.root();
    let mut unattached = SwipePager::new(2);
    let m = Msg::DragStart { node: root2, axis: Axis::X, x: 50, y: 50 };
    assert!(!unattached.on_msg(&mut fresh, &m));
}

#[test]
#[should_panic(expected = "page_count")]
fn new_zero_pages_panics() {
    let _ = SwipePager::new(0);
}

#[test]
#[should_panic(expected = "page_count")]
fn new_nine_pages_panics() {
    let _ = SwipePager::new(9);
}

#[test]
fn follow_finger_and_rubber_band_half() {
    // 跟手：dx=+30 → ox=+30 越首页界（>0）→ 橡皮筋 30/2 = 15
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_move(&mut pager, &mut ui, track, 80));
    assert_eq!(page0_x(&ui, &pager), 15, "首页右拖橡皮筋 ×0.5");

    // 域内跟手：dx=-30 → ox=-30 原值（无带）
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_move(&mut pager, &mut ui, track, 20));
    assert_eq!(page0_x(&ui, &pager), -30, "域内 1:1 跟手");

    // 末页带：两次速度翻页至页 2（每手势翻一页），再左拖越界
    assert!(drag_end(&mut pager, &mut ui, track, -600));
    settle(&mut pager, &mut ui, 1_000_000_000);
    assert_eq!(pager.page(), 1);
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_end(&mut pager, &mut ui, track, -600));
    settle(&mut pager, &mut ui, 2_000_000_000);
    assert_eq!(pager.page(), 2);
    assert_eq!(page0_x(&ui, &pager), -200, "页 2 静止位 = -2×页宽");
    assert!(drag_start(&mut pager, &mut ui, track, 90));
    assert!(drag_move(&mut pager, &mut ui, track, 20)); // base=-200, dx=-70
    assert_eq!(page0_x(&ui, &pager), -235, "末页左拖：min=-200 + (-70)/2 = -235");
}

#[test]
fn flip_three_criteria_and_snap_endpoint() {
    // 判据①速度：|vx| ≥ 500px/s 按速度向翻页（位移无关）
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_move(&mut pager, &mut ui, track, 20)); // dx=-30（< 40 阈）
    assert!(drag_end(&mut pager, &mut ui, track, -FLIP_VEL_PX_S));
    assert_eq!(pager.page(), 1, "速度向翻页即时裁定");
    assert!(pager.tick(&mut ui, 1_000), "吸附起拍帧");
    assert_eq!(page0_x(&ui, &pager), -30, "t=0 持 from（起拍位）");
    assert!(pager.tick(&mut ui, 1_000 + SNAP_DUR_NS), "完成帧");
    assert_eq!(page0_x(&ui, &pager), -100, "QuadOut 240ms 吸附终点精确");
    assert!(!pager.tick(&mut ui, 1_000 + SNAP_DUR_NS + 1), "吸附毕再 tick 零开销");

    // 判据②位移：|dx| > 40% 页宽（100 → 阈 40；dx=-45 翻 / -30 回弹）
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_move(&mut pager, &mut ui, track, 5)); // dx=-45 > 40
    assert!(drag_end(&mut pager, &mut ui, track, 0));
    assert_eq!(pager.page(), 1, "位移 >40% 翻页");

    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_move(&mut pager, &mut ui, track, 20)); // dx=-30 ≤ 40
    assert!(drag_end(&mut pager, &mut ui, track, 0));
    assert_eq!(pager.page(), 0, "位移不足回弹");
    settle(&mut pager, &mut ui, 5_000_000);
    assert_eq!(page0_x(&ui, &pager), 0, "回弹吸附回原页静止位");

    // 速度阈边界：恰 500 翻；499 不翻（≥ 语义 + dx=0 回弹）
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_end(&mut pager, &mut ui, track, -500));
    assert_eq!(pager.page(), 1, "|vx| 恰 500 → ≥ 含边界翻页");
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_end(&mut pager, &mut ui, track, -499));
    assert_eq!(pager.page(), 0, "499 < 500 且零位移 → 回弹");

    // 正向速度（右滑 = 前一页）：页 1 → 页 0；页 0 再前翻饱和
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_end(&mut pager, &mut ui, track, -600));
    settle(&mut pager, &mut ui, 10_000_000);
    assert_eq!(pager.page(), 1);
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_end(&mut pager, &mut ui, track, 600));
    assert_eq!(pager.page(), 0, "vx>0 右滑回前一页");
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_end(&mut pager, &mut ui, track, 700));
    assert_eq!(pager.page(), 0, "页 0 前向饱和不越界");
}

#[test]
fn cancel_springs_back_without_flip() {
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_move(&mut pager, &mut ui, track, 5)); // dx=-45（位移判据本会翻）
    assert!(pager.on_msg(&mut ui, &Msg::Cancel { node: track }));
    assert_eq!(pager.page(), 0, "取消不裁定翻页——回弹当前页");
    settle(&mut pager, &mut ui, 100_000_000);
    assert_eq!(page0_x(&ui, &pager), 0);
}

#[test]
fn mid_snap_grab_rebases_from_current_offset() {
    // 吸附途中抓回：中断动画、跟手基准 = 现行视觉 ox（非静止位）。
    let (mut ui, mut pager, track) = pager3();
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_move(&mut pager, &mut ui, track, 5));
    assert!(drag_end(&mut pager, &mut ui, track, -600)); // 翻页 1，吸附 -30 → -100
    let t0 = 1_000u64;
    assert!(pager.tick(&mut ui, t0)); // 起拍（t=0 持 -30）
    let mid = t0 + SNAP_DUR_NS / 2;
    pager.tick(&mut ui, mid); // 中点（QuadOut t=0.5 → -30 + 0.75×(-70) → -82）
    let grabbed = page0_x(&ui, &pager);
    assert!(grabbed < -30 && grabbed > -100, "中点位在途（grabbed={grabbed}）");

    // 抓回 + 微拖（|dx|=3 < 40 阈）→ 回弹**当前裁定页** 1
    assert!(drag_start(&mut pager, &mut ui, track, 53));
    assert!(drag_move(&mut pager, &mut ui, track, 50));
    assert_eq!(page0_x(&ui, &pager), grabbed - 3, "抓回基准 = 现行 ox（非静止位）");
    assert!(drag_end(&mut pager, &mut ui, track, 0));
    assert_eq!(pager.page(), 1);
    settle(&mut pager, &mut ui, mid + SNAP_DUR_NS);
    assert_eq!(page0_x(&ui, &pager), -100, "松手吸附回页 1 静止位");
}

#[test]
fn dot_highlight_follows_page_immediately() {
    let (mut ui, mut pager, track) = pager3();

    // 初态：dot0 亮（wake FULL 全发射）
    let mut enc = CommandEncoder::new();
    let mut scratch = [GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    ui.wake(0);
    ui.paint(&mut scratch, &mut enc);
    let dl = enc.finish();
    let dots = dot_colors(&dl);
    assert_eq!(dots.len(), 3, "三圆点");
    assert_eq!(dots[0], (0, DOT_ON), "初态页 0 亮");
    assert_eq!(dots[1], (32, DOT_OFF));
    assert_eq!(dots[2], (64, DOT_OFF));

    // 翻页 1：圆点**即时**直摆（不等吸附收尾——on_msg 返 true 时直摆）
    assert!(drag_start(&mut pager, &mut ui, track, 50));
    assert!(drag_end(&mut pager, &mut ui, track, -600));
    let mut enc = CommandEncoder::new();
    let out = ui.render_frame(1_000, &mut scratch, &mut enc);
    assert!(out.drew, "圆点换色 = paint-only damage");
    let dl = enc.finish();
    let seen = dot_colors(&dl);
    assert!(
        seen.contains(&(0, DOT_OFF)) && seen.contains(&(32, DOT_ON)),
        "页 0 点转暗 + 页 1 点转亮：{seen:?}"
    );
}

#[test]
fn pager_node_budget_exact_and_launcher_margin() {
    // 节点精确账：8 页 pager 自建 = 18 节点（track + 8 页 + 圆点行 +
    // 8 点；root 先占 1）——Ui<19> 恰纳，再一叶 NodeFull；Ui<18> 建树
    // 中途即满 loud。
    let mut ui = Ui::<19>::new(100, 100);
    let root = ui.root();
    let mut p8 = SwipePager::new(8);
    assert!(p8.attach(&mut ui, root).is_ok());
    assert_eq!(ui.add(root, rect_spec(1, 1)), Err(UiError::NodeFull), "余量恰 0");

    let mut ui18 = Ui::<18>::new(100, 100);
    let root18 = ui18.root();
    let mut p8b = SwipePager::new(8);
    assert_eq!(p8b.attach(&mut ui18, root18), Err(UiError::NodeFull), "第 19 节点建树中途满");

    // launcher 尺度（§9 预算面）：Ui<512> = MAX_NODES 档——8 页 pager +
    // 每页 3 内容叶（launcher 24 格样例）后填满，总容量恰 512（侧表/
    // 填充不蚀 arena 帽——近帽余量断言的确定性面）。
    let mut big = Ui::<512>::new(360, 640);
    let groot = big.root();
    let mut pager = SwipePager::new(8);
    pager.attach(&mut big, groot).unwrap();
    for p in 0..8u8 {
        let page = pager.page_node(p).unwrap();
        for _ in 0..3 {
            big.add(page, rect_spec(10, 10)).unwrap();
        }
    }
    let mut used = 1 + 1 + 8 + 1 + 8 + 24; // root + track + 页 + 点行 + 点 + 内容
    while big.add(groot, rect_spec(1, 1)).is_ok() {
        used += 1;
    }
    assert_eq!(used, 512, "pager(8 页)+24 内容叶 = 43 节点，余 469 格宿主树预算");
}
