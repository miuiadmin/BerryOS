//! GUI-3 §8 tree 行：arena 代数与父子链形状。
//!
//! - remove 连子树 + 旧界 damage；代数防复用（remove 后旧 NodeId 一切
//!   消费面 NodeBad loud——不静默串槽）。
//! - 容量上限（Ui&lt;N&gt; 满场 NodeFull；释放后槽可复用但代数不同）。
//! - 父子链形状：add/remove 后前序遍历序（first_child/next_sib 链）经
//!   encode_desc 字节流钉板（对照 DescBuilder 同树字节——值级对拍）。
//! - root() 幂等 / remove(root) 清场 / BadParent 三面。
//!
//! 确定性：无随机无时钟；全部值级断言（矩形逐字节、错误变体全字段）。

use berry_ui::desc::{
    ColProps, DescBuilder, Props, RectProps, TextProps, TextStyle, ViewKind, ViewSpec,
};
use berry_ui::damage::RectI;
use berry_ui::{Ui, UiError};

fn bg(r: u8, g: u8, b: u8) -> berry_gfx::cmd::Color {
    berry_gfx::cmd::Color { r, g, b }
}

fn rect_spec(w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w, h }),
    }
}

fn text_spec(bytes: &[u8]) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle {
                fg: bg(9, 9, 9),
                bg: None,
                scale: berry_gfx::font::FontSize::S8x16,
            },
            text: berry_ui::TextBuf::new(bytes).unwrap(),
        }),
    }
}

fn col_spec() -> ViewSpec {
    ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }) }
}

/// damage 覆盖判定（逐点全采样——值级，不依赖合并策略内部形态）。
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
fn root_idempotent_single_instance() {
    let mut ui = Ui::<16>::new(100, 100);
    let r1 = ui.root();
    let r2 = ui.root();
    assert_eq!(r1, r2, "root() 幂等——二次调用返同一 id");
    assert_eq!(ui.kind_of(r1), Ok(ViewKind::Root));
    let c = ui.add(r1, rect_spec(Some(10), Some(10))).unwrap();
    assert_eq!(ui.kind_of(c), Ok(ViewKind::Rect));
}

#[test]
fn capacity_full_then_slot_reuse_with_new_generation() {
    let mut ui = Ui::<4>::new(100, 100);
    let root = ui.root();
    let a = ui.add(root, rect_spec(Some(5), Some(5))).unwrap();
    let b = ui.add(root, rect_spec(Some(5), Some(5))).unwrap();
    let _c = ui.add(root, rect_spec(Some(5), Some(5))).unwrap();
    // 满场（4 槽 = root + 3 子）→ NodeFull loud
    assert_eq!(ui.add(root, rect_spec(Some(5), Some(5))), Err(UiError::NodeFull));
    // 释放中间子 → 槽可复用
    ui.remove(b).unwrap();
    let d = ui.add(root, rect_spec(Some(6), Some(6))).unwrap();
    assert_ne!(d, b, "复用槽的新 id 与旧 id 必须不同（gen<<16|idx 代数）");
    assert_eq!(ui.kind_of(d), Ok(ViewKind::Rect));

    // 代数防悬空：旧 id 的一切消费面 NodeBad
    assert_eq!(ui.kind_of(b), Err(UiError::NodeBad));
    assert_eq!(ui.bounds(b), Err(UiError::NodeBad));
    assert_eq!(ui.remove(b), Err(UiError::NodeBad), "double-remove 拒");
    assert_eq!(ui.add(b, rect_spec(Some(1), Some(1))), Err(UiError::NodeBad));
    assert_eq!(
        ui.set_props(b, Props::Rect(RectProps { bg: bg(0, 0, 0), w: Some(1), h: Some(1) })),
        Err(UiError::NodeBad)
    );
    assert_eq!(ui.set_text(b, b"x"), Err(UiError::NodeBad));
    assert_eq!(ui.invalidate(b), Err(UiError::NodeBad));
    assert_eq!(ui.dep_bind(b, berry_ui::dep::DepId(1)), Err(UiError::NodeBad));
    // root 与其它活 id 不受影响
    assert_eq!(ui.kind_of(root), Ok(ViewKind::Root));
    assert_eq!(ui.kind_of(a), Ok(ViewKind::Rect));
}

#[test]
fn remove_takes_whole_subtree_and_dangles_descendants() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let col = ui
        .add(root, ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }) })
        .unwrap();
    let a = ui.add(col, rect_spec(Some(10), Some(10))).unwrap();
    let t = ui.add(col, text_spec(b"ab")).unwrap();
    let bystander = ui.add(root, rect_spec(Some(4), Some(4))).unwrap();

    ui.remove(col).unwrap();
    // 容器连子树整删：三个 id 全悬空
    assert_eq!(ui.kind_of(col), Err(UiError::NodeBad));
    assert_eq!(ui.kind_of(a), Err(UiError::NodeBad));
    assert_eq!(ui.kind_of(t), Err(UiError::NodeBad));
    // 局外兄弟与 root 存活
    assert_eq!(ui.kind_of(bystander), Ok(ViewKind::Rect));
    assert_eq!(ui.kind_of(root), Ok(ViewKind::Root));
}

#[test]
fn remove_leaf_records_old_bounds_damage() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let col = ui
        .add(root, ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 0, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }) })
        .unwrap();
    let a = ui.add(col, rect_spec(Some(20), Some(10))).unwrap();
    ui.layout();
    let drained = ui.drain_damage();
    assert!(!drained.is_empty(), "首排必有摆位 damage（前置自检）");

    // remove 先收旧绝对界进 damage（§3.1）：A 的旧界 (0,0,20,10) 必被覆盖
    ui.remove(a).unwrap();
    let d = ui.drain_damage();
    assert!(!d.is_empty(), "remove 必产出 damage");
    assert!(covers(&d, RectI::new(0, 0, 20, 10)), "旧界 (0,0,20,10) 必在 damage 内");
}

#[test]
fn sibling_chain_shape_via_preorder_bytes() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let col = ui
        .add(root, ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 2, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch }) })
        .unwrap();
    let col_props = Props::Col(ColProps { gap: 2, w: None, h: None, cross: berry_ui::layout::CrossAlign::Stretch });
    let _a = ui.add(col, rect_spec(Some(20), Some(10))).unwrap();
    let b = ui.add(col, rect_spec(Some(20), Some(10))).unwrap();
    let _c = ui.add(col, rect_spec(Some(20), Some(10))).unwrap();

    // 前序字节 oracle：与引擎同树（Root→Col→N×Rect）的 DescBuilder 流
    let oracle = |n_children: usize| -> Vec<u8> {
        let mut bld = DescBuilder::new();
        bld.begin(ViewKind::Root, 0, &Props::Root).unwrap();
        bld.begin(ViewKind::Col, 0, &col_props).unwrap();
        for _ in 0..n_children {
            bld
                .begin(
                    ViewKind::Rect,
                    0,
                    &Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(20), h: Some(10) }),
                )
                .unwrap();
            bld.end().unwrap();
        }
        bld.end().unwrap();
        bld.end().unwrap();
        bld.bytes().to_vec()
    };

    // E1：三子前序 [Root, Col, A, B, C]——与 DescBuilder 同树字节相等
    let mut buf = [0u8; 128];
    let n1 = ui.encode_desc(&mut buf).unwrap();
    assert_eq!(&buf[..n1], oracle(3).as_slice());

    // E2：remove 中间子 B → [Root, Col, A, C]（next_sib 链跳过 B）
    ui.remove(b).unwrap();
    let n2 = ui.encode_desc(&mut buf).unwrap();
    assert_eq!(&buf[..n2], oracle(2).as_slice());

    // E3：再 add 追加尾位 D → [Root, Col, A, C, D]（尾插不重排既有序）
    let _d = ui.add(col, rect_spec(Some(20), Some(10))).unwrap();
    let n3 = ui.encode_desc(&mut buf).unwrap();
    assert_eq!(&buf[..n3], oracle(3).as_slice());
}

#[test]
fn remove_root_clears_scene_then_rebuild() {
    let mut ui = Ui::<8>::new(100, 100);
    let root = ui.root();
    let _a = ui.add(root, rect_spec(Some(10), Some(10))).unwrap();

    ui.remove(root).unwrap();
    assert_eq!(ui.kind_of(root), Err(UiError::NodeBad), "root id 同样走代数悬空");
    let mut buf = [0u8; 64];
    assert_eq!(ui.encode_desc(&mut buf), Ok(0), "无根树 → 空流");
    assert_eq!(ui.node_at(5, 5), None, "无根树无命中");

    // 重新立根可继续用（root() 幂等性在清场后重启）
    let root2 = ui.root();
    assert_ne!(root, root2);
    assert_eq!(ui.kind_of(root2), Ok(ViewKind::Root));
    assert!(ui.add(root2, rect_spec(Some(10), Some(10))).is_ok());
}

#[test]
fn tree_depth_cap_96_loud() {
    // GUI-6 D29：arena 树深帽 TREE_DEPTH_MAX=96——递归面（measure/place/
    // cull/emit）未迭代化，512 深链 × ~200B/帧 ≈ 数十 KB 穿 32KB guest
    // 栈；96 层内可建、第 97 层 add → NodeBad loud（与 builder 帽
    // DESC_DEPTH_MAX=64 分工——64 < 96 自洽）。
    let mut ui = Ui::<128>::new(10, 10);
    let mut cur = ui.root(); // 深度 1（根）
    for _ in 1..berry_ui::TREE_DEPTH_MAX {
        cur = ui.add(cur, col_spec()).unwrap(); // 深度 2..=96 全可建
    }
    assert_eq!(ui.add(cur, col_spec()), Err(UiError::NodeBad), "第 97 层超帽 loud");
    // 深帽内兄弟分支不受牵连（帽按链深非总量）
    let root = ui.root();
    assert!(ui.add(root, col_spec()).is_ok(), "浅层兄弟照常可建");
}

#[test]
fn capacity_pin_max_nodes_absolute_value() {
    // R3-1（§11-8 钦定 const 路径形态补面）：MAX_NODES **绝对值**运行时
    // 钉板。const 比值断言族（CAP/ORDER_CAP = 2*MAX_NODES）与随源右移的
    // Ui<OVERFLOW_NODES> 对帽值整体漂移（512→4096）同缩放不响——唯本测
    // 红：Ui<MAX_NODES> 满场必恰在第 MAX_NODES 节点（含 root）后 loud。
    const CAP_PROBE: usize = berry_ui::MAX_NODES;
    // 绝对值钉：符号面（CAP_PROBE 随源右移）对 512→N 漂移同缩放不响 =
    // 假守护——唯此字面量断言红。§9 栈账锚 512，改帽须同步重审。
    assert_eq!(CAP_PROBE, 512, "MAX_NODES 绝对值漂移——§9 栈账锚 512 须同步重审");
    let mut ui = Ui::<CAP_PROBE>::new(100, 100);
    let root = ui.root();
    for _ in 1..CAP_PROBE {
        ui.add(root, rect_spec(Some(1), Some(1))).unwrap(); // 恰填满 = root + CAP_PROBE-1 子
    }
    assert_eq!(
        ui.add(root, rect_spec(Some(1), Some(1))),
        Err(UiError::NodeFull),
        "满场恰在总节点数 = MAX_NODES 处（帽值漂移本测即红）"
    );
}

#[test]
fn bad_parent_faces() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let leaf = ui.add(root, rect_spec(Some(10), Some(10))).unwrap();
    let text = ui.add(root, text_spec(b"x")).unwrap();

    // 容器位传叶子 → BadParent
    assert_eq!(ui.add(leaf, rect_spec(Some(1), Some(1))), Err(UiError::BadParent));
    assert_eq!(ui.add(text, rect_spec(Some(1), Some(1))), Err(UiError::BadParent));
    // Root 不能为子
    assert_eq!(
        ui.add(root, ViewSpec { kind: ViewKind::Root, key: 0, props: Props::Root }),
        Err(UiError::BadParent)
    );
}

#[test]
fn text_inline_limit_loud() {
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    // 超限拒在 TextBuf 构造口（ViewSpec 无从携超长文本入树——源头 loud）
    let long: &[u8] = &[b'x'; berry_ui::TEXT_INLINE + 1];
    assert_eq!(
        berry_ui::TextBuf::new(long),
        Err(UiError::TextTooLong { len: berry_ui::TEXT_INLINE + 1 })
    );
    // 恰 64B 内联可用
    let ok: &[u8] = &[b'x'; berry_ui::TEXT_INLINE];
    assert!(ui.add(root, text_spec(ok)).is_ok());
}
