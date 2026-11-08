//! GUI F2：布局 dump 与 hit-test dump 的确定性/反 mock 钉板。
//!
//! 只用 `berry_ui::sdk` 的冻结面：诊断必须覆盖树形、绝对界、dirty 语义、
//! topmost 命中、Root 排除、输出满 loud 拒，不允许靠字符串快照或静默截断。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_ui::sdk::{
    ColProps, CrossAlign, HitDumpEntry, LayoutDumpEntry, NodeId, Props, RectI, RectProps,
    TextBuf, TextProps, TextStyle, Ui, UiError, ViewKind, ViewSpec,
};

fn rect(bg: Color, w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg, w, h }),
    }
}

fn text(bytes: &[u8]) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: Color { r: 20, g: 224, b: 64 }, bg: None, scale: FontSize::S8x16 },
            text: TextBuf::new(bytes).unwrap(),
        }),
    }
}

fn blank(id: NodeId) -> LayoutDumpEntry {
    LayoutDumpEntry {
        id, parent: None, first_child: None, next_sibling: None,
        kind: ViewKind::Root, key: 0, depth: 0, x: 0, y: 0, w: 0, h: 0,
        bounds: RectI::new(0, 0, 0, 0), measured_w: 0, measured_h: 0,
        layout_dirty: false, paint_dirty: false,
    }
}

#[test]
fn layout_dump_is_preorder_geometry_and_dirty_truth() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let col = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Col,
                key: 7,
                props: Props::Col(ColProps { gap: 4, w: None, h: None, cross: CrossAlign::Stretch }),
            },
        )
        .unwrap();
    let a = ui.add(col, rect(Color { r: 1, g: 2, b: 3 }, Some(20), Some(10))).unwrap();
    let b = ui.add(col, rect(Color { r: 4, g: 5, b: 6 }, None, None)).unwrap();
    let c = ui.add(col, text(b"hi")).unwrap();

    let mut before = [blank(root); 8];
    let n = ui.layout_dump(&mut before).unwrap();
    assert_eq!(n, 5, "Root/Col/A/B/C 全量前序");
    assert_eq!((before[0].id, before[0].depth), (root, 0));
    assert_eq!(before[0].first_child, Some(col));
    assert_eq!(before[1].id, col);
    assert_eq!(before[1].parent, Some(root));
    assert_eq!(before[1].key, 7);
    assert_eq!(before[2].id, a);
    assert_eq!(before[3].id, b);
    assert_eq!(before[4].id, c);
    assert!(before[..n].iter().all(|entry| entry.layout_dirty), "layout 前全树脏");

    let stat = ui.layout();
    assert_eq!((stat.measured, stat.placed), (5, 5));
    let mut after = [blank(root); 8];
    assert_eq!(ui.layout_dump(&mut after).unwrap(), 5);
    assert!(after.iter().all(|entry| !entry.layout_dirty), "layout 消费脏位");
    assert_eq!(after[2].bounds, RectI::new(0, 0, 20, 10));
    assert_eq!(after[3].bounds, RectI::new(0, 14, 100, 0));
    assert_eq!(after[4].bounds, RectI::new(0, 18, 16, 16));
    for entry in &after[..5] {
        assert_eq!(entry.bounds, ui.bounds(entry.id).unwrap(), "dump 绝对界 == Ui::bounds 单源");
    }

    // 纯着色变：几何/dirty 语义可见，A 的 paint_dirty 置位但 relayout 为 0。
    ui.set_props(a, Props::Rect(RectProps { bg: Color { r: 9, g: 9, b: 9 }, w: Some(20), h: Some(10) }))
        .unwrap();
    let mut paint = [blank(root); 8];
    assert_eq!(ui.layout_dump(&mut paint).unwrap(), 5);
    assert!(paint[2].paint_dirty);
    assert!(!paint[2].layout_dirty);
    assert_eq!(ui.layout().measured, 0, "纯着色变零重测");

    // 文本几何变：只点亮文本到祖先链，A 保持干净。
    ui.set_text(c, b"hey").unwrap();
    let mut text_dirty = [blank(root); 8];
    assert_eq!(ui.layout_dump(&mut text_dirty).unwrap(), 5);
    assert!(!text_dirty[2].layout_dirty, "兄弟 A 不脏");
    assert!(text_dirty[4].layout_dirty, "Text 几何变");
    assert_eq!(ui.layout().measured, 3, "Text + Col + Root 聚合链");
}

#[test]
fn hit_dump_is_topmost_and_excludes_root() {
    let mut ui = Ui::<12>::new(40, 30);
    let root = ui.root();
    let bottom = ui.add(root, rect(Color { r: 1, g: 2, b: 3 }, Some(20), Some(20))).unwrap();
    let top = ui.add(root, rect(Color { r: 4, g: 5, b: 6 }, Some(10), Some(10))).unwrap();
    ui.layout();

    let points = [(5, 5), (15, 15), (25, 25), (35, 5)];
    let mut out = [HitDumpEntry { x: 0, y: 0, node: None, kind: None, bounds: None }; 4];
    assert_eq!(ui.hit_test_dump(&points, &mut out).unwrap(), 4);

    assert_eq!(out[0].node, Some(top), "重叠区后兄弟 topmost");
    assert_eq!(out[0].kind, Some(ViewKind::Rect));
    assert_eq!(out[0].bounds, Some(RectI::new(0, 0, 10, 10)));
    assert_eq!(out[1].node, Some(bottom), "仅 bottom 覆盖");
    assert_eq!(out[1].bounds, Some(RectI::new(0, 0, 20, 20)));
    assert_eq!(out[2].node, None, "空白不命中 Root");
    assert_eq!(out[2].kind, None);
    assert_eq!(out[3].node, None);
    for entry in &out {
        assert_eq!(entry.node, ui.node_at(entry.x, entry.y), "dump 与 node_at 单源");
    }
}

#[test]
fn diagnostic_output_overflow_is_loud() {
    let mut ui = Ui::<8>::new(20, 20);
    let root = ui.root();
    ui.add(root, rect(Color { r: 1, g: 2, b: 3 }, Some(10), Some(10))).unwrap();
    ui.layout();

    let mut layout = [blank(root); 1];
    assert_eq!(ui.layout_dump(&mut layout), Err(UiError::NodeFull));

    let points = [(1, 1), (2, 2)];
    let mut hits = [HitDumpEntry { x: 0, y: 0, node: None, kind: None, bounds: None }; 1];
    assert_eq!(ui.hit_test_dump(&points, &mut hits), Err(UiError::NodeFull));
}
