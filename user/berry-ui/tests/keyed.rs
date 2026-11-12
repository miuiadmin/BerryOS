//! RUN-K1：stable-key re-apply 的确定性钉板。重点不是“能重建”，而是
//! key 身份跨 apply 存活、失败零半执行、无 key 不匹配、full rebuild 只作
//! oracle/fallback。

use berry_gfx::cmd::Color;
use berry_ui::desc::{
    AstError, DescBuilder, Props, RectProps, TextProps, TextStyle, TextBuf, ViewKind,
};
use berry_ui::sdk::{ReapplyStat, SETTLE_DERIVE_MAX, KEYED_DEPTH_MAX, Ui};
use berry_gfx::font::FontSize;

fn rect(w: u32) -> Props {
    Props::Rect(RectProps { bg: Color { r: 1, g: 2, b: 3 }, w: Some(w), h: Some(20) })
}

fn text() -> Props {
    Props::Text(TextProps {
        style: TextStyle {
            fg: Color { r: 4, g: 5, b: 6 },
            bg: None,
            scale: FontSize::S16x32,
        },
        text: TextBuf::new(b"keyed").unwrap(),
    })
}

fn stream(children: &[(u64, ViewKind, Props)]) -> Vec<u8> {
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    for &(key, kind, props) in children {
        b.begin(kind, key, &props).unwrap();
        b.end().unwrap();
    }
    b.end().unwrap();
    b.bytes().to_vec()
}

fn dump<const N: usize>(ui: &mut Ui<N>) -> Vec<berry_ui::diagnostic::LayoutDumpEntry> {
    let placeholder_id = ui.root();
    let mut out = vec![
        berry_ui::diagnostic::LayoutDumpEntry {
            id: placeholder_id,
            parent: None,
            first_child: None,
            next_sibling: None,
            kind: ViewKind::Root,
            key: 0,
            depth: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            bounds: berry_ui::sdk::RectI::new(0, 0, 0, 0),
            measured_w: 0,
            measured_h: 0,
            layout_dirty: false,
            paint_dirty: false,
        };
        N + 8
    ];
    let n = ui.layout_dump(&mut out).unwrap();
    out.truncate(n);
    out
}

fn id_for_key<const N: usize>(ui: &mut Ui<N>, key: u64) -> berry_ui::NodeId {
    dump(ui).into_iter().find(|e| e.key == key).unwrap().id
}

fn encoded<const N: usize>(ui: &mut Ui<N>) -> Vec<u8> {
    let mut out = vec![0; 8192];
    let n = ui.encode_desc(&mut out).unwrap();
    out.truncate(n);
    out
}

#[test]
fn keyed_identity_survives_reorder_and_second_apply_settles() {
    let first = stream(&[
        (11, ViewKind::Rect, rect(20)),
        (22, ViewKind::Rect, rect(30)),
    ]);
    let reordered = stream(&[
        (22, ViewKind::Rect, rect(30)),
        (11, ViewKind::Rect, rect(20)),
    ]);

    let mut ui = Ui::<16>::new(100, 100);
    ui.apply_desc(&first).unwrap();
    let old11 = id_for_key(&mut ui, 11);
    let old22 = id_for_key(&mut ui, 22);
    let stat = ui.apply_desc_keyed(&reordered).unwrap();
    assert_eq!(stat.matched_clean, 1); // Root
    assert_eq!(stat.moved, 2);
    assert_eq!(stat.updated, 0);
    assert_eq!(stat.added, 0);
    assert_eq!(stat.removed, 0);
    assert_eq!(stat.rederives, 1);
    assert!(!stat.fallback_full);
    assert_eq!(id_for_key(&mut ui, 11), old11);
    assert_eq!(id_for_key(&mut ui, 22), old22);
    assert_eq!(encoded(&mut ui), reordered);

    ui.layout();
    let before = dump(&mut ui);
    ui.drain_damage();
    let again = ui.apply_desc_keyed(&reordered).unwrap();
    assert_eq!(
        again,
        ReapplyStat {
            matched_clean: 3,
            updated: 0,
            moved: 0,
            added: 0,
            removed: 0,
            rederives: 1,
            fallback_full: false,
        }
    );
    assert_eq!(dump(&mut ui), before);
    assert!(ui.damage().is_empty());
    assert_eq!(ui.layout().measured, 0);
    assert!(SETTLE_DERIVE_MAX >= again.rederives);
}

#[test]
fn keyed_props_update_preserves_identity_and_full_rebuild_oracle() {
    let first = stream(&[(7, ViewKind::Rect, rect(20))]);
    let changed = stream(&[(7, ViewKind::Rect, rect(42))]);

    let mut keyed = Ui::<16>::new(100, 100);
    keyed.apply_desc(&first).unwrap();
    let old = id_for_key(&mut keyed, 7);
    let stat = keyed.apply_desc_keyed(&changed).unwrap();
    assert_eq!(stat.updated, 1);
    assert_eq!(stat.matched_clean, 1);
    assert_eq!(id_for_key(&mut keyed, 7), old);

    let mut oracle = Ui::<16>::new(100, 100);
    oracle.apply_desc(&changed).unwrap();
    assert_eq!(encoded(&mut keyed), encoded(&mut oracle));

    let kd = dump(&mut keyed);
    let od = dump(&mut oracle);
    assert_eq!(kd.len(), od.len());
    for (k, o) in kd.iter().zip(od.iter()) {
        assert_eq!((k.kind, k.key, k.x, k.y, k.w, k.h), (o.kind, o.key, o.x, o.y, o.w, o.h));
    }
}

#[test]
fn zero_key_never_matches_and_replaces_node() {
    let first = stream(&[(0, ViewKind::Rect, rect(20))]);
    let second = stream(&[(0, ViewKind::Rect, rect(20))]);
    let mut ui = Ui::<16>::new(100, 100);
    ui.apply_desc(&first).unwrap();
    let old = dump(&mut ui).into_iter().find(|e| e.kind == ViewKind::Rect).unwrap().id;
    let stat = ui.apply_desc_keyed(&second).unwrap();
    // 两节点树中替换唯一子树已达到 full-churn 阈值：走 full oracle。
    assert!(stat.fallback_full);
    assert_eq!(stat.added, 2);
    assert_eq!(stat.removed, 2);
    let new = dump(&mut ui).into_iter().find(|e| e.kind == ViewKind::Rect).unwrap().id;
    assert_ne!(old, new);
}

#[test]
fn duplicate_key_is_loud_and_keyed_failure_preserves_tree() {
    let good = stream(&[(9, ViewKind::Rect, rect(20))]);
    let bad = stream(&[
        (9, ViewKind::Rect, rect(20)),
        (9, ViewKind::Rect, rect(30)),
    ]);
    let mut ui = Ui::<16>::new(100, 100);
    ui.apply_desc(&good).unwrap();
    let before = encoded(&mut ui);
    assert_eq!(
        ui.apply_desc_keyed(&bad),
        Err(AstError::DuplicateKey { key: 9 })
    );
    assert_eq!(encoded(&mut ui), before);
}

#[test]
fn key_kind_change_is_rejected_without_rebuild() {
    let first = stream(&[(5, ViewKind::Rect, rect(20))]);
    let changed = stream(&[(5, ViewKind::Text, text())]);
    let mut ui = Ui::<16>::new(100, 100);
    ui.apply_desc(&first).unwrap();
    let before = encoded(&mut ui);
    assert_eq!(
        ui.apply_desc_keyed(&changed),
        Err(AstError::KeyKindMismatch { key: 5 })
    );
    assert_eq!(encoded(&mut ui), before);
}

#[test]
fn large_churn_uses_full_rebuild_fallback_and_reports_both_sides() {
    let initial = stream(&[]);
    let grown = stream(&[(31, ViewKind::Rect, rect(20))]);
    let mut ui = Ui::<16>::new(100, 100);
    ui.apply_desc(&initial).unwrap();
    let stat = ui.apply_desc_keyed(&grown).unwrap();
    assert!(stat.fallback_full);
    assert_eq!(stat.removed, 1); // old Root identity
    assert_eq!(stat.added, 2);
    assert_eq!(encoded(&mut ui), grown);
}

#[test]
fn source_capacity_failure_preserves_previous_tree() {
    let first = stream(&[(1, ViewKind::Rect, rect(20))]);
    let too_big = stream(&[
        (1, ViewKind::Rect, rect(20)),
        (2, ViewKind::Rect, rect(20)),
        (3, ViewKind::Rect, rect(20)),
    ]);
    let mut ui = Ui::<3>::new(100, 100);
    ui.apply_desc(&first).unwrap();
    let before = encoded(&mut ui);
    assert!(matches!(
        ui.apply_desc_keyed(&too_big),
        Err(AstError::TooSmall { .. })
    ));
    assert_eq!(encoded(&mut ui), before);
}

#[test]
fn source_a11y_capacity_failure_preserves_runtime_a11y_and_tree() {
    fn a11y_stream(keys: &[u64]) -> Vec<u8> {
        let mut b = DescBuilder::new();
        b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
        for key in keys {
            b.begin(ViewKind::Rect, *key, &rect(20)).unwrap();
            if *key >= 32 {
                b.a11y(1, b"new").unwrap();
            }
            b.end().unwrap();
        }
        b.end().unwrap();
        b.bytes().to_vec()
    }

    let keys: Vec<u64> = (1..=64).collect();
    let old_keys: Vec<u64> = keys[..32].to_vec();
    let first = a11y_stream(&old_keys);
    let second = a11y_stream(&keys);

    let mut ui = Ui::<80>::new(800, 600);
    ui.apply_desc(&first).unwrap();
    for key in 1..=32_u64 {
        let id = id_for_key(&mut ui, key);
        ui.set_a11y(id, 1, b"old").unwrap();
    }
    // AST 本身无 a11y 块，运行期已填满 32 槽；source 要为另外 32 个
    // matched 节点新增声明面，必须在修改前 loud 拒绝。
    let before = encoded(&mut ui);
    assert!(matches!(
        ui.apply_desc_keyed(&second),
        Err(AstError::TooSmall { .. })
    ));
    assert_eq!(encoded(&mut ui), before);
    assert_eq!(ui.a11y_iter().count(), 32);
}

#[test]
fn keyed_experimental_source_depth_cap_preserves_tree() {
    fn chain_stream(depth: usize) -> Vec<u8> {
        let mut b = DescBuilder::new();
        b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
        for _ in 0..depth {
            b.begin(ViewKind::Col, 0, &Props::Col(berry_ui::desc::ColProps {
                gap: 0,
                w: None,
                h: None,
                cross: berry_ui::layout::CrossAlign::Stretch,
            }))
            .unwrap();
        }
        for _ in 0..depth {
            b.end().unwrap();
        }
        b.end().unwrap();
        b.bytes().to_vec()
    }

    let deep = chain_stream(KEYED_DEPTH_MAX as usize + 1);
    let mut ui = Ui::<64>::new(200, 200);
    ui.apply_desc(&deep).unwrap();
    let before = encoded(&mut ui);
    assert!(matches!(
        ui.apply_desc_keyed(&deep),
        Err(AstError::TooSmall { .. })
    ));
    assert_eq!(encoded(&mut ui), before);
}
