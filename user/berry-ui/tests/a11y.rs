//! GUI-6 D12 a11y 侧表钉板（§11 项 16）：remove 子树连带清（无悬挂）/
//! apply_desc 重建清表（reset_arena 面）/ 第 33 条 NodeFull loud /
//! take 幂等 / a11y_iter 活性过滤 / 悬空 id 与超长标签拒面。
//!
//! 确定性：全值级断言（count/contains/错误变体）。

use berry_ui::desc::{ColProps, Props, RectProps, ViewSpec};
use berry_ui::{A11yEntry, Ui, UiError, ViewKind};

fn rect_spec(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: berry_gfx::cmd::Color { r: 1, g: 2, b: 3 }, w: Some(w), h: Some(h) }),
    }
}

/// Root → Col{A(30×30), B(30×30)}（A@0,0 / B@0,30）。
fn two_leaf(ui: &mut Ui<48>) -> (berry_ui::NodeId, berry_ui::NodeId) {
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 0,
                props: Props::Col(ColProps {
                    gap: 0,
                    w: None,
                    h: None,
                    cross: berry_ui::layout::CrossAlign::Stretch,
                }),
            },
        )
        .unwrap();
    let a = ui.add(col, rect_spec(30, 30)).unwrap();
    let b = ui.add(col, rect_spec(30, 30)).unwrap();
    ui.layout();
    ui.drain_damage();
    (a, b)
}

fn label_of<'a>(e: &'a A11yEntry) -> &'a [u8] {
    e.label.as_bytes()
}

#[test]
fn remove_subtree_clears_a11y_no_dangling() {
    let mut ui = Ui::<48>::new(100, 100);
    let (a, b) = two_leaf(&mut ui);
    ui.set_a11y(a, 1, b"btn-a").unwrap();
    ui.set_a11y(b, 2, b"btn-b").unwrap();
    assert_eq!(ui.a11y_iter().count(), 2);

    // remove 子树成员 → 条目连带清（读侧永不见悬空 id）
    ui.remove(a).unwrap();
    let rest: Vec<&A11yEntry> = ui.a11y_iter().collect();
    assert_eq!(rest.len(), 1);
    assert_eq!(label_of(rest[0]), b"btn-b", "remove 后无悬挂条目");

    // remove 祖先面走 B 直删（Root 独子 Col 不可 remove-root）：
    ui.remove(b).unwrap();
    assert_eq!(ui.a11y_iter().count(), 0, "子树尽 → 表清");

    // 悬空 id 登记 → NodeBad loud（活性先验）
    assert_eq!(ui.set_a11y(a, 1, b"x"), Err(UiError::NodeBad));
}

#[test]
fn apply_desc_rebuild_clears_runtime_entries() {
    // 编码面发生在手动登记**前**（流无 HAS_A11Y 块）→ 重建后运行期条
    // 目随 reset_arena 整清（半落/残留皆归零——满帽拒绝面同语义）。
    let mut ui = Ui::<48>::new(100, 100);
    let (a, b) = two_leaf(&mut ui);
    let mut buf = [0u8; 256];
    let n = ui.encode_desc(&mut buf).unwrap();
    assert!(n > 0);

    ui.set_a11y(a, 1, b"manual-a").unwrap();
    ui.set_a11y(b, 2, b"manual-b").unwrap();
    assert_eq!(ui.a11y_iter().count(), 2);

    ui.apply_desc(&buf[..n]).unwrap();
    assert_eq!(ui.a11y_iter().count(), 0, "apply_desc 重建 → 运行期表整清");
}

#[test]
fn thirty_third_entry_node_full_loud() {
    let mut ui = Ui::<48>::new(100, 100);
    let root = ui.root();
    let mut ids = Vec::new();
    for _ in 0..33 {
        ids.push(ui.add(root, rect_spec(4, 4)).unwrap());
    }
    ui.layout();
    ui.drain_damage();

    for (k, &id) in ids.iter().enumerate() {
        if k < 32 {
            assert!(ui.set_a11y(id, 1, b"e").is_ok(), "第 {} 条应过（帽 32）", k + 1);
        } else {
            assert_eq!(
                ui.set_a11y(id, 1, b"e"),
                Err(UiError::NodeFull),
                "第 33 条 → NodeFull loud（D30 容量族）"
            );
        }
    }
    assert_eq!(ui.a11y_iter().count(), 32);

    // 满表后 in-place 覆写仍过（同节点不占新槽——33 帽只数唯一节点）
    assert!(ui.set_a11y(ids[0], 5, b"upd").is_ok());
    assert_eq!(ui.a11y_iter().count(), 32);
    let first = ui.a11y_iter().find(|e| e.node == ids[0]).unwrap();
    assert_eq!((first.role, label_of(first)), (5, &b"upd"[..]));
}

#[test]
fn take_a11y_idempotent_and_iter_filters_dead() {
    let mut ui = Ui::<48>::new(100, 100);
    let (a, b) = two_leaf(&mut ui);
    ui.set_a11y(a, 1, b"a").unwrap();
    ui.set_a11y(b, 1, b"b").unwrap();

    // take 命中 true → 再 take false（幂等）
    assert!(ui.take_a11y(a));
    assert!(!ui.take_a11y(a), "已摘除再 take = false");
    assert_eq!(ui.a11y_iter().count(), 1);

    // 摘空位可复用：a 槽位空 → 新节点可入（帽不缩水）
    let root = ui.root();
    let c = ui.add(root, rect_spec(8, 8)).unwrap();
    assert!(ui.set_a11y(c, 1, b"c").is_ok());
    assert_eq!(ui.a11y_iter().count(), 2);
}

#[test]
fn set_a11y_label_length_loud() {
    let mut ui = Ui::<48>::new(100, 100);
    let (a, _) = two_leaf(&mut ui);
    // 恰 64 过（TEXT_INLINE 帽内）；65 → TextTooLong loud
    assert!(ui.set_a11y(a, 1, &[b'x'; 64]).is_ok());
    assert_eq!(
        ui.set_a11y(a, 1, &[b'y'; 65]),
        Err(UiError::TextTooLong { len: 65 })
    );
}
