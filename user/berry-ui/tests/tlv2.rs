//! GUI-6 段B TLV v2 钉板（`docs/GUI-6实施设计草案.md` §8/§11.1-3/9——
//! D26 golden 双轨 + §8.2 合法域全矩阵 + Grid 双拒面 + builder 块序面）。
//!
//! - **GOLDEN_V2_STREAM 字面量**：三新 kind（Image/Icon/Grid）+ 三新
//!   flags（SIZE/A11Y/ALIGN）齐流的样本树现产物钉死（V1 golden 在
//!   reconcile.rs 另钉——双轨不并常量）。roundtrip 测试**辅助**（编码
//!   器漂移由字面量守卫拦，roundtrip 只证格式自洽）。
//! - §8.2 合法域矩阵值级：SIZE×kind（Col/Row/Grid/Rect 容读折入合法；
//!   Text/Image/Icon/Pad/Root 拒）/ ALIGN×kind（仅 Col/Row）/ A11Y 全
//!   kind 合法。flags bit4+ 拒在 reconcile.rs 绊线改钉（0x10）。
//! - 规范形执法：SIZE 双 None 空载块拒 / ALIGN 载 Stretch 拒——块在
//!   ⟺ 有义，byte-exact roundtrip 的推导前提。
//! - Grid 双拒面（D21 变体 add-only）：GridZeroDims / GridNotFit 两面
//!   （parse 流面 + builder API 面）+ wire 7B 定长（截断/值域）。
//! - builder 块序面：块序恒 SIZE→A11Y→ALIGN（dup/乱序/子后落块全拒）；
//!   a11y 表满帽（A11Y_MAX=32 → TooSmall loud）。
//!
//! 确定性：无随机无时钟；拒面断言到变体载荷字段。

use berry_ui::desc::{
    AstError, ColProps, DescBuilder, GridProps, IconProps, ImageProps, Props, RectProps, RowProps,
    ViewKind, ViewSpec, KIND_GRID, KIND_IMAGE, KIND_ICON, KIND_ROOT,
};
use berry_ui::layout::CrossAlign;
use berry_ui::{A11Y_MAX, Ui, ROLE_BUTTON, ROLE_IMAGE};
use berry_gfx::cmd::Color;

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

/// 段B 样本树 builder 产物 oracle：
/// Root(cc=2) → Col key=0x11 gap=3 [SIZE w=40 h=None][A11Y BUTTON "grid"]
/// [ALIGN Center](cc=2) → {Icon id=7 fg(10,20,30) w=8 h=None,
/// Image res=9 w=None h=4 [A11Y IMAGE "hero"]}；Grid 3x2 gap=1
/// cross=Stretch(cc=2) → {Rect(1,2,3), Rect(4,5,6) w=5}。
fn v2_oracle_bytes() -> Vec<u8> {
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Col,
        0x11,
        &Props::Col(ColProps { gap: 3, w: None, h: None, cross: CrossAlign::Stretch }),
    )
    .unwrap();
    b.size(Some(40), None).unwrap();
    b.a11y(ROLE_BUTTON, b"grid").unwrap();
    b.align(CrossAlign::Center).unwrap();
    b.begin_icon(0, &IconProps { id: 7, fg: bg(10, 20, 30), w: Some(8), h: None })
        .unwrap();
    b.end().unwrap();
    b.begin_image(0, &ImageProps { res: 9, w: None, h: Some(4) }).unwrap();
    b.a11y(ROLE_IMAGE, b"hero").unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Col
    b.begin_grid(
        0,
        &GridProps { cols: 3, rows: 2, gap: 1, cross: CrossAlign::Stretch, w: None, h: None },
    )
    .unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }))
        .unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(4, 5, 6), w: Some(5), h: None }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Grid
    b.end().unwrap(); // Root
    b.bytes().to_vec()
}

/// D26 v2 golden：现产物钉字面量（块序 SIZE→A11Y→ALIGN、Grid cross 在
/// props 块内、Icon/Image w/h 哨兵对在 props 定长域、a11y 内联标签）。
const GOLDEN_V2_STREAM: &[u8] = &[
    0x00, 0x00, 0x02, // Root（cc=2）
    0x03, 0x0F, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Col key=0x11 flags=KEY|SIZE|A11Y|ALIGN
    0x03, 0x00, 0x00, 0x00, // gap=3
    0x28, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, // SIZE 块：w=40 h=None
    0x01, 0x04, b'g', b'r', b'i', b'd', // A11Y 块：BUTTON "grid"
    0x01, 0x02, // ALIGN 块：Center；cc=2
    0x07, 0x00, 0x07, 0x00, 0x0A, 0x14, 0x1E, // Icon id=7 fg(10,20,30)
    0x08, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // w=8 h=None；cc=0
    0x06, 0x04, 0x09, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x04, 0x00, 0x00, 0x00, // Image res=9 w=None h=4
    0x03, 0x04, b'h', b'e', b'r', b'o', 0x00, // A11Y：IMAGE "hero"；cc=0
    0x08, 0x00, 0x03, 0x02, 0x01, 0x00, 0x00, 0x00, 0x02, 0x02, // Grid 3x2 gap=1 cross=Stretch(2)；cc=2
    0x01, 0x00, 0x01, 0x02, 0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect(1,2,3) w/h=None
    0x01, 0x00, 0x04, 0x05, 0x06, 0x05, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect(4,5,6) w=5 h=None
];

#[test]
fn golden_v2_stream_pinned_literal() {
    assert_eq!(
        v2_oracle_bytes().as_slice(),
        GOLDEN_V2_STREAM,
        "v2 流字节漂移——编码器/builder 不兼容变更（D26 字面量守卫）"
    );
}

#[test]
fn v2_roundtrip_through_arena_and_a11y_table() {
    // 辅助面（D26）：builder 流 → apply（arena + a11y 侧表落位）→ 再
    // encode 逐字节相等——A11Y 块经侧表往返（槽+代数对拍命中即复块）。
    let mut ui = Ui::<32>::new(100, 100);
    ui.apply_desc(GOLDEN_V2_STREAM).unwrap();
    assert!(ui.damage().full, "apply 成功 → FULL damage");
    let mut out = [0u8; 256];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], GOLDEN_V2_STREAM, "roundtrip 字节相等（含 a11y 侧表往返）");
}

// ---------- §8.2 合法域矩阵（SIZE/ALIGN × kind——值级拒面） ----------

/// Root(cc=1) + 目标节点（flags=has_size + 8B 块 w=7 h=None）流。
fn sized_stream(kind: u8, props: &[u8]) -> Vec<u8> {
    let mut v = vec![KIND_ROOT, 0x00, 0x01, kind, 0x02];
    v.extend_from_slice(props);
    v.extend_from_slice(&7u32.to_le_bytes());
    v.extend_from_slice(&u32::MAX.to_le_bytes());
    v.push(0x00); // cc=0
    v
}

/// Root(cc=1) + 目标节点（flags=has_align + 1B 块 cross）流。
fn aligned_stream(kind: u8, props: &[u8], cross: u8) -> Vec<u8> {
    let mut v = vec![KIND_ROOT, 0x00, 0x01, kind, 0x08];
    v.extend_from_slice(props);
    v.push(cross);
    v.push(0x00); // cc=0
    v
}

const RECT_PROPS: &[u8] = &[1, 2, 3, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
const TEXT_PROPS: &[u8] = &[1, b'h', 0x00, 1, 1, 1, 0x00];
const IMAGE_PROPS: &[u8] = &[9, 0, 0xFF, 0xFF, 0xFF, 0xFF, 4, 0, 0, 0];
const ICON_PROPS: &[u8] = &[7, 0, 10, 20, 30, 8, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF];
const GRID_PROPS: &[u8] = &[2, 2, 1, 0, 0, 0, 2];

#[test]
fn has_size_matrix_legal_and_rejects() {
    // 合法四件：Col/Row/Grid 块值落扩展字段；Rect = 容读折入面（块胜）
    let mut ui = Ui::<32>::new(100, 100);
    ui.apply_desc(&sized_stream(3, &[3, 0, 0, 0])).unwrap(); // Col gap=3
    ui.apply_desc(&sized_stream(4, &[0, 0, 0, 0])).unwrap(); // Row gap=0
    ui.apply_desc(&sized_stream(KIND_GRID, GRID_PROPS)).unwrap(); // Grid 2x2
    // Rect：props 块 w=None + SIZE 块 w=7 → 块胜折入 w=Some(7)
    ui.apply_desc(&sized_stream(1, RECT_PROPS)).unwrap();
    let mut out = [0u8; 64];
    let n = ui.encode_desc(&mut out).unwrap();
    // 归一规范形：无块（w 折回 props 定长域）
    assert_eq!(
        &out[..n],
        &[KIND_ROOT, 0x00, 0x01, 1, 0x00, 1, 2, 3, 7, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0x00],
        "Rect+SIZE 容读折入——再编码归一无块形式"
    );

    // 拒五件：Text（TextProps 满载 Node 冻结无落位）/ Image·Icon（props
    // 定长域双源歧义）/ Pad·Root（无定尺寸语义）
    let cases: [(u8, &[u8], ViewKind); 5] = [
        (2, TEXT_PROPS, ViewKind::Text),
        (KIND_IMAGE, IMAGE_PROPS, ViewKind::Image),
        (KIND_ICON, ICON_PROPS, ViewKind::Icon),
        (5, &[1, 0, 0, 0], ViewKind::Pad),
        (KIND_ROOT, &[], ViewKind::Root), // Root 为子面：块检先于父检（拒因可钉）
    ];
    for (kb, props, kind) in cases {
        let mut ui = Ui::<32>::new(100, 100);
        assert_eq!(
            ui.apply_desc(&sized_stream(kb, props)),
            Err(AstError::UnknownProp { kind, got: 0x02 }),
            "SIZE×{kind:?} 拒（§8.2 表）"
        );
    }
}

#[test]
fn has_align_matrix_legal_and_rejects() {
    // 合法两件：Col/Row（cross 落扩展字段 + roundtrip 复块）
    for kb in [3u8, 4u8] {
        let mut ui = Ui::<32>::new(100, 100);
        ui.apply_desc(&aligned_stream(kb, &[0, 0, 0, 0], 0x00)).unwrap(); // Start
        let mut out = [0u8; 64];
        let n = ui.encode_desc(&mut out).unwrap();
        assert_eq!(&out[..n], &aligned_stream(kb, &[0, 0, 0, 0], 0x00)[..], "ALIGN 块 roundtrip");
    }
    // 拒七件：Grid（cross 在 props 块内——ALIGN 块双源歧义）/ Text/Rect/
    // Image/Icon/Pad/Root
    let cases: [(u8, &[u8], ViewKind); 7] = [
        (KIND_GRID, GRID_PROPS, ViewKind::Grid),
        (2, TEXT_PROPS, ViewKind::Text),
        (1, RECT_PROPS, ViewKind::Rect),
        (KIND_IMAGE, IMAGE_PROPS, ViewKind::Image),
        (KIND_ICON, ICON_PROPS, ViewKind::Icon),
        (5, &[1, 0, 0, 0], ViewKind::Pad),
        (KIND_ROOT, &[], ViewKind::Root),
    ];
    for (kb, props, kind) in cases {
        let mut ui = Ui::<32>::new(100, 100);
        assert_eq!(
            ui.apply_desc(&aligned_stream(kb, props, 0x01)),
            Err(AstError::UnknownProp { kind, got: 0x08 }),
            "ALIGN×{kind:?} 拒（§8.2 表）"
        );
    }
}

#[test]
fn canonical_form_empty_block_rejects() {
    // SIZE 双 None = 空载块拒（块在 ⟺ 任一 Some——可推导性前提）
    let mut ui = Ui::<32>::new(100, 100);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 3, 0x02, 0, 0, 0, 0];
    v.extend_from_slice(&u32::MAX.to_le_bytes());
    v.extend_from_slice(&u32::MAX.to_le_bytes());
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::Col, got: 0x02 })
    );
    // ALIGN 载 Stretch(2) = 缺省拒（块在 ⟺ 非 Stretch）
    let mut ui = Ui::<32>::new(100, 100);
    assert_eq!(
        ui.apply_desc(&aligned_stream(3, &[0, 0, 0, 0], 0x02)),
        Err(AstError::UnknownProp { kind: ViewKind::Col, got: 2 })
    );
    // cross 值域外（3）→ UnknownProp（cross_from_u8 单源）
    let mut ui = Ui::<32>::new(100, 100);
    assert_eq!(
        ui.apply_desc(&aligned_stream(3, &[0, 0, 0, 0], 0x03)),
        Err(AstError::UnknownProp { kind: ViewKind::Col, got: 3 })
    );
    // Grid props 块内 cross 值域外（3）同拒
    let mut ui = Ui::<32>::new(100, 100);
    let bad_grid = [KIND_ROOT, 0x00, 0x01, KIND_GRID, 0x00, 2, 2, 1, 0, 0, 0, 3, 0x00];
    assert_eq!(
        ui.apply_desc(&bad_grid),
        Err(AstError::UnknownProp { kind: ViewKind::Grid, got: 3 })
    );
}

#[test]
fn a11y_block_any_kind_truncation_and_cap() {
    // 全 kind 合法：Pad + a11y（role=5 label 空）——流面合法 + roundtrip
    let mut ui = Ui::<32>::new(100, 100);
    let stream = [KIND_ROOT, 0x00, 0x01, 5, 0x04, 1, 0, 0, 0, 0x05, 0x00, 0x00];
    ui.apply_desc(&stream).unwrap();
    let mut out = [0u8; 64];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], &stream, "A11Y×Pad 合法 + 空标签 roundtrip");

    // 截断：len 声明 5 只给 2 字节标签
    let mut ui = Ui::<32>::new(100, 100);
    assert_eq!(
        ui.apply_desc(&[KIND_ROOT, 0x00, 0x01, 5, 0x04, 1, 0, 0, 0, 0x01, 0x05, b'a', b'b']),
        Err(AstError::Truncated)
    );
    // 超长：len=65 > TEXT_INLINE=64
    let mut ui = Ui::<32>::new(100, 100);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 5, 0x04, 1, 0, 0, 0, 0x00, 65];
    v.extend_from_slice(&[b'x'; 65]);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::TextTooLong { len: berry_ui::TEXT_INLINE + 1 })
    );
    // 侧表满帽：33 条 a11y > A11Y_MAX=32 → TooSmall loud（表清零半执行）
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    for _ in 0..(A11Y_MAX + 1) {
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }))
            .unwrap();
        b.a11y(1, b"x").unwrap();
        b.end().unwrap();
    }
    b.end().unwrap();
    let mut ui = Ui::<64>::new(100, 100);
    assert_eq!(
        ui.apply_desc(b.bytes()),
        Err(AstError::TooSmall { need: A11Y_MAX + 1, got: A11Y_MAX })
    );
    let mut out = [0u8; 16];
    assert_eq!(ui.encode_desc(&mut out), Ok(0), "拒面零半执行——场清空");
}

// ---------- Grid 双拒面 + wire 定长 ----------

#[test]
fn grid_zero_dims_rejects_both_faces() {
    // 流面：cols=0（7B 定长读完再语义检——截断优先）
    let mut ui = Ui::<32>::new(100, 100);
    assert_eq!(
        ui.apply_desc(&[KIND_ROOT, 0x00, 0x01, KIND_GRID, 0x00, 0, 2, 1, 0, 0, 0, 2, 0x00]),
        Err(AstError::GridZeroDims)
    );
    // builder 面：begin_grid cols=0
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    assert_eq!(
        b.begin_grid(0, &GridProps { cols: 0, rows: 2, gap: 1, cross: CrossAlign::Stretch, w: None, h: None }),
        Err(AstError::GridZeroDims)
    );
}

#[test]
fn grid_not_fit_rejects_both_faces() {
    // 流面：1x1 格 cc=2 → GridNotFit{children=2}
    let mut ui = Ui::<32>::new(100, 100);
    let stream = [
        KIND_ROOT, 0x00, 0x01, // Root cc=1
        KIND_GRID, 0x00, 1, 1, 0, 0, 0, 0, 2, 2, // Grid 1x1 gap=0 cross=Stretch cc=2
        1, 0x00, 1, 2, 3, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect
        1, 0x00, 4, 5, 6, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect
    ];
    assert_eq!(
        ui.apply_desc(&stream),
        Err(AstError::GridNotFit { cols: 1, rows: 1, children: 2 })
    );
    // builder 面：1x1 格第 2 子 begin 拒（payload = 被拒子序号 2）
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin_grid(0, &GridProps { cols: 1, rows: 1, gap: 0, cross: CrossAlign::Stretch, w: None, h: None })
        .unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }))
        .unwrap();
    b.end().unwrap();
    assert_eq!(
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(4, 5, 6), w: None, h: None })),
        Err(AstError::GridNotFit { cols: 1, rows: 1, children: 2 })
    );
    // 恰满合法：3x2 格第 6 子过、第 7 子拒（边界对拍）
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin_grid(0, &GridProps { cols: 3, rows: 2, gap: 0, cross: CrossAlign::Stretch, w: None, h: None })
        .unwrap();
    for _ in 0..6 {
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }))
            .unwrap();
        b.end().unwrap();
    }
    assert_eq!(
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None })),
        Err(AstError::GridNotFit { cols: 3, rows: 2, children: 7 })
    );
}

#[test]
fn grid_wire_truncation_and_leaf_children() {
    // Grid props 截断（7B 只给 6B）→ Truncated（先于零维语义检）
    let mut ui = Ui::<32>::new(100, 100);
    assert_eq!(
        ui.apply_desc(&[KIND_ROOT, 0x00, 0x01, KIND_GRID, 0x00, 2, 2, 1, 0, 0, 0]),
        Err(AstError::Truncated)
    );
    // Image/Icon 叶带子 → BadParent（流面）
    let mut ui = Ui::<32>::new(100, 100);
    assert_eq!(
        ui.apply_desc(&[KIND_ROOT, 0x00, 0x01, KIND_IMAGE, 0x00, 9, 0, 0xFF, 0xFF, 0xFF, 0xFF, 4, 0, 0, 0, 0x01]),
        Err(AstError::BadParent)
    );
    // builder 面：Image 下 begin 拒
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin_image(0, &ImageProps { res: 9, w: None, h: None }).unwrap();
    assert_eq!(
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None })),
        Err(AstError::BadParent)
    );
    // add 面：Image 父位传子 → UiError::BadParent
    let mut ui = Ui::<32>::new(100, 100);
    let root = ui.root();
    let img = ui
        .add(root, ViewSpec { kind: ViewKind::Image, key: 0, props: Props::Image(ImageProps { res: 1, w: None, h: None }) })
        .unwrap();
    assert_eq!(
        ui.add(img, ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }) }),
        Err(berry_ui::UiError::BadParent)
    );
}

// ---------- wire 深帽（D29/R2-1——apply_desc 直递归面） ----------

/// Grid 1×1 深嵌套链流（`depth` 层含 Root）：每层 kind=8 props =
/// [cols=1, rows=1, gap=0, cross=Stretch(2)]——单格单子在 AST 语义检
/// （GridNotFit 界 = cells ≥ cc）全合法，深帽前唯一界 = 槽位帽
/// `nodes.len()`（R2-1 实锤的绕路面）。
fn deep_grid_chain(depth: usize) -> Vec<u8> {
    let mut v = vec![KIND_ROOT, 0x00];
    let mut d = depth;
    loop {
        if d <= 1 {
            v.push(0x00); // 末层 cc=0
            return v;
        }
        v.push(0x01); // 本层 cc=1
        v.extend_from_slice(&[KIND_GRID, 0x00, 1, 1, 0, 0, 0, 0, 2]);
        d -= 1;
    }
}

/// wire 面深帽 = add() 同单源 TREE_DEPTH_MAX（R2-1）：apply_desc →
/// build_node 递归不经 builder 帽（DESC_DEPTH_MAX 只护 begin 面）——
/// 恰 96 层全合法过、第 97 层 loud（拒因 = 深帽先于槽位帽，可钉）。
#[test]
fn wire_depth_cap_96_boundary_and_deep_attack() {
    // 恰 96 层（帽内）：合法建树 + roundtrip 长度对拍（字节序由 golden
    // 面守卫，此处只证帽内可建可回读）
    let mut ui = Ui::<128>::new(10, 10);
    let expect = deep_grid_chain(96);
    ui.apply_desc(&expect).unwrap();
    let mut out = [0u8; 2048];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(n, expect.len(), "96 层帽内全合法（roundtrip 长度守恒）");

    // 第 97 层：深帽 loud（与 add() 第 97 层 NodeBad 同口径；错误面
    // add-only 复用 TooSmall——need = 被拒层 1 基深度）
    let mut ui = Ui::<128>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&deep_grid_chain(97)),
        Err(AstError::TooSmall { need: 97, got: berry_ui::TREE_DEPTH_MAX as usize }),
        "第 97 层超 wire 深帽 loud（R2-1——绕 add() 帽的直递归面收编）"
    );
    let mut out = [0u8; 16];
    assert_eq!(ui.encode_desc(&mut out), Ok(0), "拒面零半执行——场清空");

    // 远深攻击（300 层）：深帽先于槽位帽拒（拒因可钉——无深帽时旧面
    // 走槽位帽 TooSmall{need:129, got:128}，深树 ≤ 槽数时照建穿栈）
    let mut ui = Ui::<128>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&deep_grid_chain(300)),
        Err(AstError::TooSmall { need: 97, got: berry_ui::TREE_DEPTH_MAX as usize }),
        "深帽先于槽位帽（拒因不随攻击深度漂）"
    );
}

// ---------- builder 块序面（阶段位图执法） ----------

#[test]
fn builder_block_order_dup_and_stage_rejects() {
    // 无层落块（未 begin）→ BadParent
    let mut b = DescBuilder::new();
    assert_eq!(b.size(Some(1), None), Err(AstError::BadParent));

    // dup：size 两次 → BadParent（阶段位图）
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Col, 0, &Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }))
        .unwrap();
    b.size(Some(1), None).unwrap();
    assert_eq!(b.size(Some(2), None), Err(AstError::BadParent));

    // 乱序：a11y 后补 size → BadParent（SIZE 必先）
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Col, 0, &Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }))
        .unwrap();
    b.a11y(1, b"x").unwrap();
    assert_eq!(b.size(Some(1), None), Err(AstError::BadParent));

    // 子后落块：开首子后 count_pos 不再流尾 → BadParent
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Col, 0, &Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }))
        .unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }))
        .unwrap();
    b.end().unwrap();
    assert_eq!(b.a11y(1, b"x"), Err(AstError::BadParent), "已闭子后块不落");

    // 合法面收尾：块序正确产出可解析流（三块齐 + 两子——字节同 golden 段）
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Row, 0, &Props::Row(RowProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }))
        .unwrap();
    b.size(None, Some(9)).unwrap();
    b.a11y(1, b"ok").unwrap();
    b.align(CrossAlign::Start).unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap();
    b.end().unwrap();
    let mut ui = Ui::<16>::new(100, 100);
    ui.apply_desc(b.bytes()).unwrap();
    let mut out = [0u8; 96];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], b.bytes(), "块序三连 roundtrip 字节相等");
}

#[test]
fn builder_block_legality_and_payload_rejects() {
    // kind×块拒（builder 生产面 = §8.2 表 minus Rect 折入面）：Rect/Text/
    // Image/Icon/Pad 的 size 全拒；Grid/Text/Rect/Image/Icon/Pad 的 align 拒
    let rect = Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None });
    let text = Props::Text(berry_ui::TextBuf::new(b"h").map(|t| berry_ui::desc::TextProps {
        style: berry_ui::desc::TextStyle { fg: bg(1, 1, 1), bg: None, scale: berry_gfx::font::FontSize::S8x16 },
        text: t,
    }).unwrap());
    let cases: [(ViewKind, Props); 5] = [
        (ViewKind::Rect, rect),
        (ViewKind::Text, text),
        (ViewKind::Image, Props::Image(ImageProps { res: 1, w: None, h: None })),
        (ViewKind::Icon, Props::Icon(IconProps { id: 1, fg: bg(1, 2, 3), w: None, h: None })),
        (ViewKind::Pad, Props::Pad(berry_ui::desc::PadProps { all: 1 })),
    ];
    for (kind, props) in cases {
        let mut b = DescBuilder::new();
        b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
        b.begin(kind, 0, &props).unwrap();
        assert_eq!(
            b.size(Some(1), None),
            Err(AstError::UnknownProp { kind, got: 0x02 }),
            "builder size×{kind:?} 拒"
        );
        assert_eq!(
            b.align(CrossAlign::Center),
            Err(AstError::UnknownProp { kind, got: 0x08 }),
            "builder align×{kind:?} 拒"
        );
    }
    // Rect+size 拒的所以然：builder 产规范流（Rect 尺寸归 props 域），
    // 折入面（解析容读）不可逆推——双入口收敛的执法点（非 §8.2 偏差，
    // 生产/容读两面的分工）。
    // 载荷规范形：size 双 None 拒 / align Stretch 拒 / label 超长拒
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Grid, 0, &Props::Grid(GridProps { cols: 2, rows: 2, gap: 0, cross: CrossAlign::Stretch, w: None, h: None }))
        .unwrap();
    assert_eq!(
        b.size(None, None),
        Err(AstError::UnknownProp { kind: ViewKind::Grid, got: 0x02 })
    );
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Col, 0, &Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }))
        .unwrap();
    assert_eq!(
        b.align(CrossAlign::Stretch),
        Err(AstError::UnknownProp { kind: ViewKind::Col, got: 2 })
    );
    assert_eq!(
        b.a11y(1, &[b'x'; berry_ui::TEXT_INLINE + 1]),
        Err(AstError::TextTooLong { len: berry_ui::TEXT_INLINE + 1 })
    );
    // Grid kind×props 失配（泛型 begin 面收紧）：Grid 配 Rect props 拒
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    assert_eq!(
        b.begin(
            ViewKind::Grid,
            0,
            &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None })
        ),
        Err(AstError::UnknownProp { kind: ViewKind::Grid, got: KIND_GRID })
    );
}
