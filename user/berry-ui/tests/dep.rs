//! GUI-3 §8 dep 行：dep_bind/dep_dependents 记录语义（v1 只记录不消费）
//! + DEP_EDGES_MAX 满帽 DepFull。
//!
//! - 记录形状：插入序回读、多 dep 多节点交叉、out 截断、悬空 id 拒。
//! - 只记录不消费：bind 不产 damage、不触发重排；失效仍走视图级
//!  invalidate（裁决不跳步——§3.10）。
//! - 满帽：64 边界 loud；remove 连带清边（悬空不残留）。
//!
//! 确定性：无随机无时钟；回读集合值级断言。

use berry_ui::damage::RectI;
use berry_ui::dep::DepId;
use berry_ui::desc::{ColProps, Props, RectProps, ViewSpec};
use berry_ui::layout::LayoutStat;
use berry_ui::{Ui, UiError, ViewKind};

fn bg(r: u8, g: u8, b: u8) -> berry_gfx::cmd::Color {
    berry_gfx::cmd::Color { r, g, b }
}

fn rect_spec(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(w), h: Some(h) }),
    }
}

/// 三叶树（已静默：首排毕 + damage 排空）。
fn settled(ui: &mut Ui<16>) -> (berry_ui::NodeId, berry_ui::NodeId, berry_ui::NodeId) {
    let root = ui.root();
    let a = ui.add(root, rect_spec(10, 10)).unwrap();
    let b = ui.add(root, rect_spec(10, 10)).unwrap();
    let c = ui.add(root, rect_spec(10, 10)).unwrap();
    ui.layout();
    ui.drain_damage();
    (a, b, c)
}

/// damage 覆盖判定（逐点采样——不依赖合并策略内部形态）。
fn covers(d: &berry_ui::damage::DamageSet, r: RectI) -> bool {
    if d.full || r.is_empty() {
        return true;
    }
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            let inside = d.iter().any(|w| {
                (x as i64) >= w.x as i64
                    && (x as i64) < w.x as i64 + w.w as i64
                    && (y as i64) >= w.y as i64
                    && (y as i64) < w.y as i64 + w.h as i64
            });
            if !inside {
                return false;
            }
        }
    }
    true
}

#[test]
fn bind_record_shape_and_insertion_order() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, b, c) = settled(&mut ui);

    // 交叉绑定：d1 三节点、d2 单节点
    ui.dep_bind(a, DepId(1)).unwrap();
    ui.dep_bind(b, DepId(1)).unwrap();
    ui.dep_bind(a, DepId(2)).unwrap();
    ui.dep_bind(c, DepId(1)).unwrap();

    // d1 依赖者按插入序回读（a,b,c——b 挂 y=10、c 挂 y=20 摆位互异可辨）
    let mut out = [a; 8];
    let n = ui.dep_dependents(DepId(1), &mut out);
    assert_eq!(n, 3);
    assert_eq!(&out[..n], &[a, b, c], "插入序即回读序");
    // d2 单依赖
    let n = ui.dep_dependents(DepId(2), &mut out);
    assert_eq!(n, 1);
    assert_eq!(out[0], a);
    // 未绑 dep → 零依赖
    let n = ui.dep_dependents(DepId(99), &mut out);
    assert_eq!(n, 0);
}

#[test]
fn dependents_out_len_truncates() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, b, c) = settled(&mut ui);
    ui.dep_bind(a, DepId(7)).unwrap();
    ui.dep_bind(b, DepId(7)).unwrap();
    ui.dep_bind(c, DepId(7)).unwrap();

    // out 容量 2：填 2 截断，返回**真实总数 3**（零分配哨兵面——调用
    // 方以 n > out.len() 判截断，不炸不静默缩水）
    let mut out = [a; 2];
    let n = ui.dep_dependents(DepId(7), &mut out);
    assert_eq!(n, 3, "返回真实总数——非已填数");
    assert_eq!(&out, &[a, b]);
}

#[test]
fn stale_id_bind_refused() {
    let mut ui = Ui::<16>::new(100, 100);
    let (_, b, _) = settled(&mut ui);
    ui.remove(b).unwrap();
    assert_eq!(ui.dep_bind(b, DepId(1)), Err(UiError::NodeBad), "悬空 id bind loud");
}

#[test]
fn remove_clears_attached_edges() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, b, c) = settled(&mut ui);
    ui.dep_bind(a, DepId(1)).unwrap();
    ui.dep_bind(b, DepId(1)).unwrap();
    ui.dep_bind(c, DepId(1)).unwrap();

    ui.remove(b).unwrap();
    let mut out = [a; 8];
    let n = ui.dep_dependents(DepId(1), &mut out);
    assert_eq!(n, 2, "remove 连带清边");
    assert_eq!(&out[..n], &[a, c], "幸存边原序保留");
}

#[test]
fn edges_capacity_sixtyfour_loud() {
    // 单节点可持多边（异 dep 各占槽；同边幂等不占——见尾部）：灌满
    // DEP_EDGES_MAX → 第 65 条**新边** DepFull loud
    let mut ui = Ui::<2>::new(100, 100);
    let root = ui.root();
    let a = ui.add(root, rect_spec(10, 10)).unwrap();
    ui.layout();
    ui.drain_damage();

    for i in 0..berry_ui::DEP_EDGES_MAX as u32 {
        ui.dep_bind(a, DepId(i)).unwrap();
    }
    assert_eq!(ui.dep_bind(a, DepId(999)), Err(UiError::DepFull), "满帽 loud");

    // 幂等重绑：已存在的同边在**满帽**下仍 Ok（幂等查重先于空槽查找，
    // 不占新槽）；dependents 计数不翻倍
    assert_eq!(ui.dep_bind(a, DepId(0)), Ok(()), "同边幂等——满帽不拒");
    let mut out1 = [a; 1];
    assert_eq!(ui.dep_dependents(DepId(0), &mut out1), 1, "幂等重绑不重复计边");

    // 回读容量照常：每 dep 恰一边（dep id = 插入序 0..64），逐 dep 收割
    let mut out = [a; berry_ui::DEP_EDGES_MAX];
    let mut total = 0usize;
    for i in 0..berry_ui::DEP_EDGES_MAX as u32 {
        total += ui.dep_dependents(DepId(i), &mut out);
    }
    assert_eq!(total, berry_ui::DEP_EDGES_MAX, "64 边全可回读");
}

#[test]
fn bind_records_but_never_consumes() {
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _, _) = settled(&mut ui);
    // 静默前置自检
    assert!(ui.damage().is_empty());

    // 只记录不消费：bind 零 damage、零重排
    ui.dep_bind(a, DepId(1)).unwrap();
    ui.dep_bind(a, DepId(2)).unwrap();
    assert!(ui.damage().is_empty(), "bind 不产 damage");
    assert_eq!(ui.layout(), LayoutStat { measured: 0, placed: 0 }, "bind 不触发重排");

    // 失效仍视图级 invalidate（裁决不跳步）：A 界 (0,0,10,10) 必入 damage
    ui.invalidate(a).unwrap();
    let d = ui.drain_damage();
    assert!(covers(&d, RectI::new(0, 0, 10, 10)), "invalidate 界覆盖 A");
    assert_eq!(ui.layout(), LayoutStat { measured: 0, placed: 0 }, "纯重绘不重排");
}

#[test]
fn set_props_kind_mismatch_loud_state_preserved() {
    // kind 失配拒面（审查 #6 守卫）：Rect 槽塞 Col props → NodeBad loud，
    // 拒绝先于任何状态变更——damage 零产出、节点原 props 完好（同 kind
    // set_props 照常 Ok 且 damage = 旧界）。防「错配 props 静默入槽」：
    // paint 发射面按 kind 匹配 props，错配 = 永不发射/读错载荷。
    let mut ui = Ui::<16>::new(100, 100);
    let (a, _, _) = settled(&mut ui);
    assert!(ui.damage().is_empty(), "前置自检：已静默");

    assert_eq!(
        ui.set_props(a, Props::Col(ColProps { gap: 4, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch })),
        Err(UiError::NodeBad),
        "Rect 槽塞 Col props → NodeBad（无 kind 失配变体，NodeBad 兼任）"
    );
    assert!(ui.damage().is_empty(), "拒绝零副作用——不产 damage");
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 10, 10), "几何未被触碰");

    // 同 kind 同几何改色：Ok + damage = 旧界（状态完好、合法路照常）
    ui.set_props(a, Props::Rect(RectProps { bg: bg(9, 9, 9), w: Some(10), h: Some(10) }))
        .unwrap();
    let d = ui.drain_damage();
    assert!(!d.full);
    assert_eq!(d.count, 1);
    assert_eq!(d.rects[0], berry_guiproto::Rect { x: 0, y: 0, w: 10, h: 10 });
}

#[test]
fn remove_parent_unbinds_descendant_edges() {
    // remove 连子树：**后裔边先行解绑**（先于 gen 推进——悬空 id 即使
    // 被旧引用持有也查无此边）；隔代孙边同样清、幸存者边保留
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let col = ui
        .add(root, ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }) })
        .unwrap();
    let a = ui.add(col, rect_spec(10, 10)).unwrap();
    let b = ui.add(col, rect_spec(10, 10)).unwrap();
    ui.layout();
    ui.drain_damage();
    ui.dep_bind(a, DepId(5)).unwrap();
    ui.dep_bind(b, DepId(5)).unwrap();
    ui.dep_bind(root, DepId(5)).unwrap();

    ui.remove(col).unwrap(); // 连子树（a/b 随殁）
    let mut out = [a; 4];
    let n = ui.dep_dependents(DepId(5), &mut out);
    assert_eq!(n, 1, "子树后裔边全清——仅幸存 root 边");
    assert_eq!(out[0], root);
}
