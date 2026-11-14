//! P0 尾四件 TLV 钉板（组件清单收尾批——ViewKind 25→29，tag
//! 25..=28 add-only）。tlv4.rs 同款双轨：
//!
//! - **GOLDEN_V5_STREAM 字面量**：四新 kind（Select/Input/Edit/Canvas）
//!   齐流的样本树现产物钉死（V1..V4 golden 在 reconcile.rs/tlv2.rs/
//!   tlv3.rs/tlv4.rs 各自另钉——五轨不并常量）。roundtrip 测试**辅助**
//!   （编码器漂移由字面量守卫拦，roundtrip 只证格式自洽）。
//! - 拒面值级：BadValue 族（Select selected 越项——option_count > 0
//!   时；零项合法 = 空选择器与 Tabs/Seg 零段拒不同律，双侧钉）——
//!   流面与 builder 生产面同律；UnknownProp 族（input_kind/readonly
//!   子域外）、TextTooLong（placeholder 超长）、截断（Select 12B /
//!   Input 6..=70 / Edit 10..=74 / Canvas 4B）、新四叶带子（流面 +
//!   builder 面）、块×kind（SIZE×新 kind 拒——§8.2 表未扩）。
//! - add() 命令面烟囱：新 kind 可建可编码（数据层通），roundtrip 对拍；
//!   新 kind 父位拒（lib.rs 容器白名单未收编——Phase 1 同款已知边界，
//!   钉为现行为）。
//!
//! 确定性：无随机无时钟；拒面断言到变体载荷字段。

use berry_gfx::cmd::Color;
use berry_ui::desc::{
    AstError, CanvasProps, ColProps, EditProps, InputKind, InputProps, Props, RectProps,
    SelectProps, ViewKind, ViewSpec, KIND_ROOT,
};
use berry_ui::layout::CrossAlign;
use berry_ui::{Ui, ROLE_BUTTON};

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

/// P0 尾四件样本树 builder 产物 oracle：Root(cc=2) → Col gap=0(cc=4)
/// → {Select 3 项 selected=1 on_change=5 [A11Y BUTTON "sel"]，Input
/// 占位 "Name" max_len=32 Text，Edit 占位 "Bio" max_len=200 max_lines=0
/// 非只读，Canvas paint_ops=7}；Rect(9,8,7)。
fn v5_oracle_bytes() -> Vec<u8> {
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Col,
        0,
        &Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }),
    )
    .unwrap();
    b.begin(
        ViewKind::Select,
        0,
        &Props::Select(SelectProps { option_count: 3, selected: 1, on_change: 5 }),
    )
    .unwrap();
    b.a11y(ROLE_BUTTON, b"sel").unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Input,
        0,
        &Props::Input(InputProps {
            placeholder: berry_ui::TextBuf::new(b"Name").unwrap(),
            max_len: 32,
            input_kind: InputKind::Text,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Edit,
        0,
        &Props::Edit(EditProps {
            placeholder: berry_ui::TextBuf::new(b"Bio").unwrap(),
            max_len: 200,
            max_lines: 0,
            readonly: false,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Canvas, 0, &Props::Canvas(CanvasProps { paint_ops: 7 })).unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Col
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(9, 8, 7), w: None, h: None }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Root
    b.bytes().to_vec()
}

/// P0 尾四件 golden：四新 kind 现产物钉字面量（Select 12B、Input 变长
/// 6..=70、Edit 变长 10..=74、Canvas 4B；A11Y×Select 合法——§8.2 全
/// kind；Col 容器带子）。
const GOLDEN_V5_STREAM: &[u8] = &[
    0x00, 0x00, 0x02, // Root（cc=2）
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, // Col gap=0；cc=4
    0x19, 0x04, // Select flags=A11Y
    0x03, 0x00, 0x00, 0x00, // option_count=3
    0x01, 0x00, 0x00, 0x00, // selected=1
    0x05, 0x00, 0x00, 0x00, // on_change=5
    0x01, 0x03, b's', b'e', b'l', // A11Y 块：BUTTON "sel"
    0x00, // cc=0
    0x1A, 0x00, // Input
    0x04, b'N', b'a', b'm', b'e', // placeholder="Name"
    0x20, 0x00, 0x00, 0x00, // max_len=32
    0x00, // input_kind=Text
    0x00, // cc=0
    0x1B, 0x00, // Edit
    0x03, b'B', b'i', b'o', // placeholder="Bio"
    0xC8, 0x00, 0x00, 0x00, // max_len=200
    0x00, 0x00, 0x00, 0x00, // max_lines=0（无限制）
    0x00, // readonly=false
    0x00, // cc=0
    0x1C, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, // Canvas paint_ops=7；cc=0
    0x01, 0x00, 0x09, 0x08, 0x07, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect(9,8,7)
];

#[test]
fn golden_v5_stream_pinned_literal() {
    assert_eq!(
        v5_oracle_bytes().as_slice(),
        GOLDEN_V5_STREAM,
        "v5 流字节漂移——编码器/builder 不兼容变更（字面量守卫）"
    );
}

#[test]
fn v5_roundtrip_through_arena_and_a11y_table() {
    // 辅助面：builder 流 → apply（arena + a11y 侧表落位）→ 再 encode
    // 逐字节相等（A11Y 块经侧表往返；Col 子树照常重建）。
    let mut ui = Ui::<32>::new(100, 100);
    ui.apply_desc(GOLDEN_V5_STREAM).unwrap();
    assert!(ui.damage().full, "apply 成功 → FULL damage");
    let mut out = [0u8; 256];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], GOLDEN_V5_STREAM, "roundtrip 字节相等（含 a11y 侧表往返）");
}

/// golden 未覆盖的对称值档：Select 零项（空选择器——selected 恒 0
/// 合法）/ Input Password + 空占位 + max_len=0 / Edit readonly + 四行
/// 帽 + 空占位 / Canvas paint_ops=0——全环 roundtrip 字节相等。
#[test]
fn v5_alt_values_roundtrip() {
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Select,
        0x55,
        &Props::Select(SelectProps { option_count: 0, selected: 0, on_change: 0 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Input,
        0,
        &Props::Input(InputProps {
            placeholder: berry_ui::TextBuf::new(b"").unwrap(),
            max_len: 0,
            input_kind: InputKind::Password,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Edit,
        0,
        &Props::Edit(EditProps {
            placeholder: berry_ui::TextBuf::new(b"").unwrap(),
            max_len: 0,
            max_lines: 4,
            readonly: true,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Canvas, 0, &Props::Canvas(CanvasProps { paint_ops: 0 })).unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Root
    let mut ui = Ui::<16>::new(50, 50);
    ui.apply_desc(b.bytes()).unwrap();
    let mut out = [0u8; 128];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], b.bytes(), "对称值档 roundtrip 字节相等");
}

// ---------- 值域拒面（BadValue 族——流面与 builder 生产面同律） ----------

/// Root(cc=1) + 目标单节点流（flags=0）。
fn single_stream(kind: u8, props: &[u8], cc: u8) -> Vec<u8> {
    let mut v = vec![KIND_ROOT, 0x00, 0x01, kind, 0x00];
    v.extend_from_slice(props);
    v.push(cc);
    v
}

fn u32b(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

#[test]
fn p0_bad_value_matrix_stream_and_builder_faces() {
    // Select selected == option_count（got = selected）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(3));
    p.extend_from_slice(&u32b(3));
    p.extend_from_slice(&u32b(5));
    assert_eq!(
        ui.apply_desc(&single_stream(0x19, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Select, got: 3 })
    );
    // Select selected > option_count（got = selected）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(2));
    p.extend_from_slice(&u32b(5));
    p.extend_from_slice(&u32b(0));
    assert_eq!(
        ui.apply_desc(&single_stream(0x19, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Select, got: 5 })
    );
    // 对照：option_count == 0 + selected == 0 合法（空选择器——与
    // Tabs/Seg 零段拒不同律，流面正例钉）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(0));
    ui.apply_desc(&single_stream(0x19, &p, 0)).unwrap();

    // builder 生产面同律（validate_p0_four——validate_phase1 双面先例）
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    assert_eq!(
        b.begin(
            ViewKind::Select,
            0,
            &Props::Select(SelectProps { option_count: 3, selected: 3, on_change: 5 })
        ),
        Err(AstError::BadValue { kind: ViewKind::Select, got: 3 })
    );
}

#[test]
fn p0_unknown_prop_matrix_and_truncation() {
    // Input input_kind 域外（3）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = vec![0x04, b'N', b'a', b'm', b'e'];
    p.extend_from_slice(&u32b(32));
    p.push(3);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1A, &p, 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Input, got: 3 })
    );
    // Edit readonly 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = vec![0x03, b'B', b'i', b'o'];
    p.extend_from_slice(&u32b(200));
    p.extend_from_slice(&u32b(0));
    p.push(2);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1B, &p, 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Edit, got: 2 })
    );
    // placeholder 超长（len u8 = 65 > TEXT_INLINE 64）：Input / Edit 同帽
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1A, &[65], 0)),
        Err(AstError::TextTooLong { len: 65 })
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1B, &[65], 0)),
        Err(AstError::TextTooLong { len: 65 })
    );
    // 截断（截断优先于语义检）：Select 12B 只给 11B / Input 空占位
    // 6B 只给 5B / Edit 空占位 10B 只给 9B / Canvas 4B 只给 3B
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(3));
    p.extend_from_slice(&u32b(1));
    p.extend_from_slice(&[5, 0, 0]);
    assert_eq!(ui.apply_desc(&single_stream(0x19, &p, 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1A, &[0x00, 0x20, 0x00, 0x00, 0x00], 0)),
        Err(AstError::Truncated)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(
            0x1B,
            &[0x00, 0xC8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            0
        )),
        Err(AstError::Truncated)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1C, &[0x07, 0x00, 0x00], 0)),
        Err(AstError::Truncated)
    );
}

#[test]
fn p0_leaf_children_reject_both_faces_and_size_block() {
    // 流面：Input cc=1 → BadParent（P0 尾四叶带子拒面——检先于子消费，
    // 子字节不必给全）；Canvas cc=1 同面（叶子立法钉：绘制内容不是子节点）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1A, &[0x00, 0x20, 0x00, 0x00, 0x00, 0x00], 1)),
        Err(AstError::BadParent)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1C, &[0x07, 0x00, 0x00, 0x00], 1)),
        Err(AstError::BadParent)
    );
    // builder 面：Select 下 begin 拒
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Select,
        0,
        &Props::Select(SelectProps { option_count: 3, selected: 1, on_change: 5 }),
    )
    .unwrap();
    assert_eq!(
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None })),
        Err(AstError::BadParent)
    );
    // 对照：Col 容器带子合法（golden 已钉）；SIZE 块×新 kind 拒
    // （§8.2 合法域表未扩——合法域检在长检前）
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x19, 0x02];
    v.extend_from_slice(&u32b(3));
    v.extend_from_slice(&u32b(1));
    v.extend_from_slice(&u32b(5));
    v.extend_from_slice(&u32b(7));
    v.extend_from_slice(&u32b(8));
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::Select, got: 0x02 }),
        "SIZE×Select 拒（§8.2 表不随 P0 尾四批扩）"
    );
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x1C, 0x02];
    v.extend_from_slice(&u32b(9));
    v.extend_from_slice(&u32b(7));
    v.extend_from_slice(&u32b(8));
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::Canvas, got: 0x02 }),
        "SIZE×Canvas 拒（叶同面）"
    );
}

#[test]
fn p0_add_face_smoke() {
    // add() 命令面：新 kind 可建可编码（数据层通）；两场对拍 roundtrip。
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Canvas,
            key: 0,
            props: Props::Canvas(CanvasProps { paint_ops: 2 }),
        },
    )
    .unwrap();
    let mut out = [0u8; 64];
    let n = ui.encode_desc(&mut out).unwrap();
    let mut ui2 = Ui::<8>::new(10, 10);
    ui2.apply_desc(&out[..n]).unwrap();
    let mut out2 = [0u8; 64];
    let n2 = ui2.encode_desc(&mut out2).unwrap();
    assert_eq!(&out[..n], &out2[..n2], "add 面产物与 AST 面产物同流");

    // 新叶父位拒（lib.rs 容器白名单未收编——P1 已知边界钉现行为）
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    let sel = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Select,
                key: 0,
                props: Props::Select(SelectProps { option_count: 2, selected: 0, on_change: 1 }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.add(
            sel,
            ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }) }
        ),
        Err(berry_ui::UiError::BadParent),
        "Select 叶父位拒（白名单未收编——Phase 2 接线面）"
    );
}
