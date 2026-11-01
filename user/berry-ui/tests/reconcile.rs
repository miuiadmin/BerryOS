//! GUI-3 §8 reconcile 行：encode_desc/apply_desc roundtrip + AST 拒面。
//!
//! - roundtrip 字节相等：builder 树 encode → apply 重建 → 再 encode，
//!   两流逐字节相等；DescBuilder 流与引擎 encode 同树字节相等（双入口
//!   收敛）；apply 成功 → FULL damage。
//! - 语义等价：重建树 layout 后逐探测点 node_at → (kind, bounds) 与原
//!   树一致（重建产生新 NodeId——经探测点对拍，不经 id 映射）。
//! - 拒面全变体值级：UnknownKind/UnknownFlags/Truncated×n/TrailingBytes/
//!   BadParent×3/TextTooLong/UnknownProp×2/容量 TooSmall。
//! - 失败零半执行：坏流后场清空（encode Ok(0)），root() 可重启。
//! - 空树非对称：无根 encode → Ok(0)；空流 apply → Truncated。
//!
//! 确定性：无随机无时钟；拒面断言到变体载荷字段。

use berry_ui::desc::{
    AstError, ColProps, DescBuilder, PadProps, Props, RectProps, RowProps, TextProps, TextStyle,
    ViewKind, ViewSpec, KIND_RECT, KIND_ROOT, KIND_TEXT,
};
use berry_ui::damage::RectI;
use berry_ui::Ui;
use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

/// 与 DescBuilder oracle 同构的富树（六 kind 全覆盖 + key + bg 两档）。
fn build_rich_tree(ui: &mut Ui<32>) {
    let root = ui.root();
    let col = ui
        .add(root, ViewSpec { kind: ViewKind::Col, key: 7, props: Props::Col(ColProps { gap: 4 }) })
        .unwrap();
    ui.add(col, ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(20), h: None }),
    })
    .unwrap();
    ui.add(col, ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: bg(9, 9, 9), bg: Some(bg(4, 5, 6)), scale: FontSize::S8x16 },
            text: berry_ui::TextBuf::new(b"hi").unwrap(),
        }),
    })
    .unwrap();
    ui.add(col, ViewSpec {
        kind: ViewKind::Text,
        key: 0xAA,
        props: Props::Text(TextProps {
            style: TextStyle { fg: bg(1, 1, 1), bg: None, scale: FontSize::S16x32 },
            text: berry_ui::TextBuf::new(b"yo").unwrap(),
        }),
    })
    .unwrap();
    let row = ui
        .add(root, ViewSpec { kind: ViewKind::Row, key: 0, props: Props::Row(RowProps { gap: 0 }) })
        .unwrap();
    let pad = ui
        .add(row, ViewSpec { kind: ViewKind::Pad, key: 0, props: Props::Pad(PadProps { all: 3 }) })
        .unwrap();
    ui.add(pad, ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(7, 8, 9), w: None, h: None }),
    })
    .unwrap();
}

fn rich_tree_oracle_bytes() -> Vec<u8> {
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Col, 7, &Props::Col(ColProps { gap: 4 })).unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(20), h: None }))
        .unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Text, 0, &Props::Text(TextProps {
        style: TextStyle { fg: bg(9, 9, 9), bg: Some(bg(4, 5, 6)), scale: FontSize::S8x16 },
        text: berry_ui::TextBuf::new(b"hi").unwrap(),
    }))
    .unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Text, 0xAA, &Props::Text(TextProps {
        style: TextStyle { fg: bg(1, 1, 1), bg: None, scale: FontSize::S16x32 },
        text: berry_ui::TextBuf::new(b"yo").unwrap(),
    }))
    .unwrap();
    b.end().unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Row, 0, &Props::Row(RowProps { gap: 0 })).unwrap();
    b.begin(ViewKind::Pad, 0, &Props::Pad(PadProps { all: 3 })).unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(7, 8, 9), w: None, h: None }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap();
    b.end().unwrap();
    b.end().unwrap(); // 闭 Root（cc 占位回填——begin/end 平衡后取用）
    b.bytes().to_vec()
}

#[test]
fn roundtrip_bytes_equal_and_dual_entry_converge() {
    // 入口①（builder API 树）encode
    let mut ui1 = Ui::<32>::new(100, 100);
    build_rich_tree(&mut ui1);
    let mut buf1 = [0u8; 256];
    let n1 = ui1.encode_desc(&mut buf1).unwrap();
    assert!(n1 > 0);

    // 入口②（DescBuilder 流）与入口① 同树字节相等
    assert_eq!(&buf1[..n1], rich_tree_oracle_bytes().as_slice(), "双入口同树 encode 字节收敛");

    // apply 重建 → FULL damage
    let mut ui2 = Ui::<32>::new(100, 100);
    ui2.apply_desc(&buf1[..n1]).unwrap();
    assert!(ui2.damage().full, "apply 成功 → FULL damage");

    // 重建树再 encode → 与原流逐字节相等（roundtrip 字节相等）
    let mut buf2 = [0u8; 256];
    let n2 = ui2.encode_desc(&mut buf2).unwrap();
    assert_eq!(n1, n2);
    assert_eq!(&buf1[..n1], &buf2[..n2]);
}

/// 语义等价探测：probe 点 → (Option<kind>, bounds) 不依赖 NodeId 映射。
fn probe(ui: &mut Ui<32>, pts: &[(u32, u32)]) -> Vec<(Option<ViewKind>, Option<RectI>)> {
    ui.layout();
    ui.drain_damage();
    pts.iter()
        .map(|&(x, y)| match ui.node_at(x, y) {
            Some(id) => (Some(ui.kind_of(id).unwrap()), Some(ui.bounds(id).unwrap())),
            None => (None, None),
        })
        .collect()
}

#[test]
fn rebuilt_tree_semantically_equivalent() {
    // 探测树：Root(100x100)→Col{gap:0}{ R1(20x20), Text "ab", Row{R2,R3} }
    let mut ui1 = Ui::<32>::new(100, 100);
    let root = ui1.root();
    let col = ui1
        .add(root, ViewSpec { kind: ViewKind::Col, key: 0, props: Props::Col(ColProps { gap: 0 }) })
        .unwrap();
    ui1.add(col, ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(20), h: Some(20) }),
    })
    .unwrap();
    ui1.add(col, ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: bg(9, 9, 9), bg: None, scale: FontSize::S8x16 },
            text: berry_ui::TextBuf::new(b"ab").unwrap(),
        }),
    })
    .unwrap();
    let row = ui1
        .add(col, ViewSpec { kind: ViewKind::Row, key: 0, props: Props::Row(RowProps { gap: 0 }) })
        .unwrap();
    ui1.add(row, ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(4, 5, 6), w: Some(10), h: Some(10) }),
    })
    .unwrap();
    ui1.add(row, ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(7, 8, 9), w: Some(10), h: Some(10) }),
    })
    .unwrap();

    let mut stream = [0u8; 256];
    let n = ui1.encode_desc(&mut stream).unwrap();

    let mut ui2 = Ui::<32>::new(100, 100);
    ui2.apply_desc(&stream[..n]).unwrap();

    // 探测点覆盖四叶 + 空白：R1(5,5) T(1,25) R2(5,40) R3(15,40) 空白(50,50)
    let pts: [(u32, u32); 5] = [(5, 5), (1, 25), (5, 40), (15, 40), (50, 50)];
    let p1 = probe(&mut ui1, &pts);
    let p2 = probe(&mut ui2, &pts);
    // 前置自检：探测点确有命中（防探测点选空导致假绿）
    assert!(p1.iter().any(|(k, _)| k.is_some()), "探测点必须命中至少一叶");
    assert_eq!(p1, p2, "重建树逐探测点 (kind,bounds) 等价");
    // 值级钉板（非仅等价）：R3 落位 (10,36,10,10)
    assert_eq!(p1[3], (Some(ViewKind::Rect), Some(RectI::new(10, 36, 10, 10))));
    assert_eq!(p1[4], (None, None), "空白点无命中");
}

#[test]
fn apply_reject_unknown_kind_and_flags() {
    let mut ui = Ui::<32>::new(100, 100);
    // 未知 kind 字节（值域 0..=5 外）
    assert_eq!(ui.apply_desc(&[0x99, 0]), Err(AstError::UnknownKind { got: 0x99 }));
    // flags 未知位（bit0 = has_key 之外）
    assert_eq!(ui.apply_desc(&[KIND_ROOT, 0x02]), Err(AstError::UnknownFlags { got: 0x02 }));
    assert_eq!(ui.apply_desc(&[KIND_ROOT, 0x80]), Err(AstError::UnknownFlags { got: 0x80 }));
}

#[test]
fn apply_reject_truncated_faces() {
    let mut ui = Ui::<32>::new(100, 100);
    // 空流（无根树不可重建——显式非对称）
    assert_eq!(ui.apply_desc(&[]), Err(AstError::Truncated));
    // 缺 flags
    assert_eq!(ui.apply_desc(&[KIND_ROOT]), Err(AstError::Truncated));
    // key 旗立而键截断
    assert_eq!(ui.apply_desc(&[KIND_ROOT, 1, 1, 2, 3, 4]), Err(AstError::Truncated));
    // Rect props 截断（11B 只给 6B）
    assert_eq!(
        ui.apply_desc(&[KIND_RECT, 0, 1, 2, 3, 4, 5, 6]),
        Err(AstError::Truncated)
    );
    // 节点本体齐而 child_count 尽（Root cc=1；Rect 11B props 毕、cc 缺）
    assert_eq!(
        ui.apply_desc(&[KIND_ROOT, 0, 1, KIND_RECT, 0, 9, 9, 9, 0, 0, 0, 0, 255, 255, 255, 255]),
        Err(AstError::Truncated)
    );
}

#[test]
fn apply_reject_trailing_and_bad_parent_and_text() {
    let mut ui = Ui::<32>::new(100, 100);
    // 尾随字节：合法单根流 + 1 字节
    assert_eq!(ui.apply_desc(&[KIND_ROOT, 0, 0, 0]), Err(AstError::TrailingBytes { extra: 1 }));
    // 流首非 Root（Rect 叶当根）
    assert_eq!(
        ui.apply_desc(&[KIND_RECT, 0, 9, 9, 9, 0, 0, 0, 0, 255, 255, 255, 255, 0]),
        Err(AstError::BadParent)
    );
    // Root 为子（Root cc=1 + 子 Root 本体 → BadParent 先于子 cc 读）
    assert_eq!(
        ui.apply_desc(&[KIND_ROOT, 0, 1, KIND_ROOT, 0]),
        Err(AstError::BadParent)
    );
    // 叶子带子：Rect cc=2
    assert_eq!(
        ui.apply_desc(&[
            KIND_ROOT, 0, 1, KIND_RECT, 0, 9, 9, 9, 0, 0, 0, 0, 255, 255, 255, 255, 2
        ]),
        Err(AstError::BadParent)
    );
    // 文本超长（tlen = TEXT_INLINE+1）
    assert_eq!(
        ui.apply_desc(&[KIND_ROOT, 0, 1, KIND_TEXT, 0, (berry_ui::TEXT_INLINE + 1) as u8]),
        Err(AstError::TextTooLong { len: berry_ui::TEXT_INLINE + 1 })
    );
    // Text scale 未知档
    assert_eq!(
        ui.apply_desc(&[KIND_ROOT, 0, 1, KIND_TEXT, 0, 2, b'h', b'i', 7, 1, 2, 3, 0]),
        Err(AstError::UnknownProp { kind: ViewKind::Text, got: 7 })
    );
    // Text bg tag 未知
    assert_eq!(
        ui.apply_desc(&[KIND_ROOT, 0, 1, KIND_TEXT, 0, 2, b'h', b'i', 0, 1, 2, 3, 5]),
        Err(AstError::UnknownProp { kind: ViewKind::Text, got: 5 })
    );
}

#[test]
fn apply_failure_leaves_empty_scene() {
    let mut ui = Ui::<32>::new(100, 100);
    // 先立一棵好树，再用坏流覆盖 → 失败后场清空（零半执行）
    build_rich_tree(&mut ui);
    let mut buf = [0u8; 256];
    let n = ui.encode_desc(&mut buf).unwrap();
    assert!(n > 0);

    assert!(ui.apply_desc(&[0x99, 0]).is_err());
    let mut buf2 = [0u8; 256];
    assert_eq!(ui.encode_desc(&mut buf2), Ok(0), "失败重建空场");
    assert_eq!(ui.node_at(5, 5), None);
    // root() 重启可用
    let r = ui.root();
    assert_eq!(ui.kind_of(r), Ok(ViewKind::Root));
}

#[test]
fn capacity_rejects_encode_and_apply() {
    // encode 输出缓冲不足：树 17B（Root 3 + Rect 14），out 14B →
    // Root(3)+kind/flags/bg(5)+w(4)=12 消费毕，h 域只剩 2 → TooSmall
    let mut ui = Ui::<32>::new(100, 100);
    let root = ui.root();
    ui.add(root, ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(20), h: Some(10) }),
    })
    .unwrap();
    let mut out = [0u8; 14];
    assert_eq!(ui.encode_desc(&mut out), Err(AstError::TooSmall { need: 4, got: 2 }));

    // apply 超容：2 节点流（Root cc=1）→ Ui<1>
    let mut small = Ui::<1>::new(100, 100);
    let stream = [KIND_ROOT, 0, 1, KIND_RECT, 0, 9, 9, 9, 0, 0, 0, 0, 255, 255, 255, 255, 0];
    assert_eq!(small.apply_desc(&stream), Err(AstError::TooSmall { need: 2, got: 1 }));
}

#[test]
fn empty_tree_encode_ok_zero_apply_rejects() {
    let mut ui = Ui::<32>::new(100, 100);
    let mut buf = [0u8; 16];
    assert_eq!(ui.encode_desc(&mut buf), Ok(0), "无根树 → Ok(0) 空流");
    assert_eq!(ui.apply_desc(&[]), Err(AstError::Truncated), "空流 apply 拒——非对称显式语义");
}

#[test]
fn desc_builder_reject_faces() {
    // 流首必须 Root
    let mut b = DescBuilder::new();
    assert_eq!(
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None })),
        Err(AstError::BadParent)
    );
    // Root 不能为子
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    assert_eq!(b.begin(ViewKind::Root, 0, &Props::Root), Err(AstError::BadParent));
    // 未闭叶下 begin（当前层 = Rect 叶）→ BadParent
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }))
        .unwrap();
    assert_eq!(
        b.begin(ViewKind::Col, 0, &Props::Col(ColProps { gap: 0 })),
        Err(AstError::BadParent),
        "未闭叶下 begin → BadParent"
    );
    // end 无 begin 失配
    let mut b = DescBuilder::new();
    assert_eq!(b.end(), Err(AstError::BadParent));
    // 深度帽 DESC_DEPTH_MAX：Root + 16 层 Col → 第 17 层 begin 拒
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    for _ in 0..(berry_ui::desc::DESC_DEPTH_MAX - 1) {
        b.begin(ViewKind::Col, 0, &Props::Col(ColProps { gap: 0 })).unwrap();
    }
    assert_eq!(
        b.begin(ViewKind::Col, 0, &Props::Col(ColProps { gap: 0 })),
        Err(AstError::TooSmall { need: berry_ui::desc::DESC_DEPTH_MAX + 1, got: berry_ui::desc::DESC_DEPTH_MAX })
    );
}

#[test]
fn desc_builder_child_cap_255_retry_stable() {
    // 单亲 255 子已满（CHILD_MAX=255——u8 children 计数的结构帽）：第
    // 256 个 end() → TooSmall loud，且拒绝在**任何状态变更之前**（先于
    // 计数字节回填/父计数自增）——重试同错零半执行（父计数不增、深度
    // 不减）。字节面：Root 3 + 255×Rect 14 = 3573 ≤ DESC_MAX=4096——
    // 拒因是子数帽非字节帽（两帽分界钉板）
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    for _ in 0..255 {
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }))
            .unwrap();
        b.end().unwrap();
    }
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(4, 5, 6), w: None, h: None }))
        .unwrap();
    assert_eq!(b.end(), Err(AstError::TooSmall { need: 256, got: 255 }));
    assert_eq!(b.end(), Err(AstError::TooSmall { need: 256, got: 255 }), "拒绝零半执行——重试同错");
}
