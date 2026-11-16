//! P1 九件 TLV 钉板（组件清单多数集合批——ViewKind 16→25，tag
//! 16..=24 add-only）。tlv3.rs 同款双轨：
//!
//! - **GOLDEN_V4_STREAM 字面量**：九新 kind（Stack/Spacer/Divider/Check/
//!   Radio/Step/Seg/Badge/Sbar）齐流的样本树现产物钉死（V1/V2/V3
//!   golden 在 reconcile.rs/tlv2.rs/tlv3.rs 各自另钉——四轨不并常量）。
//!   roundtrip 测试**辅助**（编码器漂移由字面量守卫拦，roundtrip 只证
//!   格式自洽）。
//! - 拒面值级：BadValue 族（Step 零步长/min>max/value 越界、Seg 零段/
//!   active 越段、Sbar thumb 越千分域/start>end——流面与 builder 生产
//!   面同律）、UnknownProp 族（align/checked/selected/show_dot/
//!   orientation 子域外）、截断（Step 20B/Seg 12B/Sbar 9B）、新八叶带子
//!   （流面 + builder 面）、块×kind（SIZE×新 kind 拒——§8.2 表未扩）。
//! - add() 命令面烟囱：新 kind 可建可编码（数据层通），roundtrip 对拍；
//!   新 kind 父位拒（lib.rs 容器白名单未收编——含 Stack 容器，Phase 1
//!   同款已知边界，钉为现行为）。
//!
//! 确定性：无随机无时钟；拒面断言到变体载荷字段。

use berry_gfx::cmd::Color;
use berry_ui::desc::{
    AstError, BadgeProps, CheckProps, DividerProps, Orientation, Props, RadioProps, RectProps,
    SbarProps, SegProps, SpacerProps, StackAlign, StackProps, StepProps, ViewKind, ViewSpec,
    KIND_ROOT, PERMILLE_MAX,
};
use berry_ui::{Ui, ROLE_TOGGLE};

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

/// P1 批样本树 builder 产物 oracle：
/// Root(cc=2) → Stack align=Center(cc=8) → {Spacer flex=1，Divider
/// thickness=1 inset 2/3，Check checked on_change=3 [A11Y TOGGLE "ck"]，
/// Radio selected group=7 on_change=4，Step 5/0..10 步 1 on_change=6，
/// Seg 3 段 active=1 on_change=8，Badge count=99 非圆点，Sbar Vertical
/// thumb 0..500‰}；Rect(9,8,7)。
fn v4_oracle_bytes() -> Vec<u8> {
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Stack, 0, &Props::Stack(StackProps { align: StackAlign::Center }))
        .unwrap();
    b.begin(ViewKind::Spacer, 0, &Props::Spacer(SpacerProps { flex: 1 })).unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Divider,
        0,
        &Props::Divider(DividerProps { thickness: 1, inset_start: 2, inset_end: 3 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Check, 0, &Props::Check(CheckProps { checked: true, on_change: 3 }))
        .unwrap();
    b.a11y(ROLE_TOGGLE, b"ck").unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Radio,
        0,
        &Props::Radio(RadioProps { selected: true, group: 7, on_change: 4 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Step,
        0,
        &Props::Step(StepProps { value: 5, min: 0, max: 10, step: 1, on_change: 6 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Seg,
        0,
        &Props::Seg(SegProps { segment_count: 3, active: 1, on_change: 8 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Badge, 0, &Props::Badge(BadgeProps { count: 99, show_dot: false }))
        .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Sbar,
        0,
        &Props::Sbar(SbarProps {
            orientation: Orientation::Vertical,
            thumb_start: 0,
            thumb_end: 500,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Stack
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(9, 8, 7), w: None, h: None, radius: 0 }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Root
    b.bytes().to_vec()
}

/// P1 批 golden：九新 kind 现产物钉字面量（Stack 1B、Spacer 4B、
/// Divider 12B、Check 5B、Radio/Sbar 9B、Step 20B、Seg 12B、Badge 5B；
/// A11Y×Check 合法——§8.2 全 kind；Stack 容器带子）。
const GOLDEN_V4_STREAM: &[u8] = &[
    0x00, 0x00, 0x02, // Root（cc=2）
    0x10, 0x00, 0x01, 0x08, // Stack align=Center cc=8
    0x11, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, // Spacer flex=1；cc=0
    0x12, 0x00, // Divider
    0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, // thickness=1 inset 2/3
    0x00, // cc=0
    0x13, 0x04, // Check flags=A11Y
    0x01, 0x03, 0x00, 0x00, 0x00, // checked on_change=3
    0x04, 0x02, b'c', b'k', // A11Y 块：TOGGLE "ck"
    0x00, // cc=0
    0x14, 0x00, // Radio
    0x01, 0x07, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, // selected group=7 on_change=4
    0x00, // cc=0
    0x15, 0x00, // Step
    0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x06, 0x00, 0x00, 0x00, // value=5 min=0 max=10 step=1 on_change=6
    0x00, // cc=0
    0x16, 0x00, // Seg
    0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, // 3 段 active=1 on_change=8
    0x00, // cc=0
    0x17, 0x00, 0x63, 0x00, 0x00, 0x00, 0x00, 0x00, // Badge count=99 非圆点；cc=0
    0x18, 0x00, // Sbar
    0x00, 0x00, 0x00, 0x00, 0x00, 0xF4, 0x01, 0x00, 0x00, // Vertical thumb 0..500‰
    0x00, // cc=0
    0x01, 0x00, 0x09, 0x08, 0x07, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect(9,8,7)
];

#[test]
fn golden_v4_stream_pinned_literal() {
    assert_eq!(
        v4_oracle_bytes().as_slice(),
        GOLDEN_V4_STREAM,
        "v4 流字节漂移——编码器/builder 不兼容变更（字面量守卫）"
    );
}

#[test]
fn v4_roundtrip_through_arena_and_a11y_table() {
    // 辅助面：builder 流 → apply（arena + a11y 侧表落位）→ 再 encode
    // 逐字节相等（A11Y 块经侧表往返；Stack 子树照常重建）。
    let mut ui = Ui::<32>::new(100, 100);
    ui.apply_desc(GOLDEN_V4_STREAM).unwrap();
    assert!(ui.damage().full, "apply 成功 → FULL damage");
    let mut out = [0u8; 256];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], GOLDEN_V4_STREAM, "roundtrip 字节相等（含 a11y 侧表往返）");
}

/// golden 未覆盖的对称值档：Stack End / Sbar Horizontal / Badge 圆点 /
/// Radio 未选 / Check 未选 / Divider 全零——全环 roundtrip 字节相等。
#[test]
fn v4_alt_values_roundtrip() {
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Stack, 0x33, &Props::Stack(StackProps { align: StackAlign::End }))
        .unwrap();
    b.begin(ViewKind::Check, 0, &Props::Check(CheckProps { checked: false, on_change: 0 }))
        .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Radio,
        0,
        &Props::Radio(RadioProps { selected: false, group: 0, on_change: 0 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Badge,
        0,
        &Props::Badge(BadgeProps { count: 0, show_dot: true }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Sbar,
        0,
        &Props::Sbar(SbarProps {
            orientation: Orientation::Horizontal,
            thumb_start: PERMILLE_MAX,
            thumb_end: PERMILLE_MAX,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Divider,
        0,
        &Props::Divider(DividerProps { thickness: 0, inset_start: 0, inset_end: 0 }),
    )
    .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Stack（key=0x33 → flags=KEY）
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
fn p1_bad_value_matrix_stream_and_builder_faces() {
    // Step 零步长（got = 0）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(5));
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(10));
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(6));
    assert_eq!(
        ui.apply_desc(&single_stream(0x15, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Step, got: 0 })
    );
    // Step min > max（got = min）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(5));
    p.extend_from_slice(&u32b(10));
    p.extend_from_slice(&u32b(2));
    p.extend_from_slice(&u32b(1));
    p.extend_from_slice(&u32b(0));
    assert_eq!(
        ui.apply_desc(&single_stream(0x15, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Step, got: 10 })
    );
    // Step value < min（got = value）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(1));
    p.extend_from_slice(&u32b(2));
    p.extend_from_slice(&u32b(10));
    p.extend_from_slice(&u32b(1));
    p.extend_from_slice(&u32b(0));
    assert_eq!(
        ui.apply_desc(&single_stream(0x15, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Step, got: 1 })
    );
    // Step value > max（got = value）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(11));
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(10));
    p.extend_from_slice(&u32b(1));
    p.extend_from_slice(&u32b(0));
    assert_eq!(
        ui.apply_desc(&single_stream(0x15, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Step, got: 11 })
    );
    // Seg 零段（got = 0）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(5));
    assert_eq!(
        ui.apply_desc(&single_stream(0x16, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Seg, got: 0 })
    );
    // Seg active 越段（got = active）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(3));
    p.extend_from_slice(&u32b(3));
    p.extend_from_slice(&u32b(5));
    assert_eq!(
        ui.apply_desc(&single_stream(0x16, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Seg, got: 3 })
    );
    // Sbar thumb_start > 1000（got = start）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = vec![0u8];
    p.extend_from_slice(&u32b(1001));
    p.extend_from_slice(&u32b(1000));
    assert_eq!(
        ui.apply_desc(&single_stream(0x18, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Sbar, got: 1001 })
    );
    // Sbar thumb_end > 1000（got = end）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = vec![0u8];
    p.extend_from_slice(&u32b(500));
    p.extend_from_slice(&u32b(1001));
    assert_eq!(
        ui.apply_desc(&single_stream(0x18, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Sbar, got: 1001 })
    );
    // Sbar start > end（got = start）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = vec![0u8];
    p.extend_from_slice(&u32b(600));
    p.extend_from_slice(&u32b(400));
    assert_eq!(
        ui.apply_desc(&single_stream(0x18, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Sbar, got: 600 })
    );

    // builder 生产面同律（validate_p1_nine——validate_phase1 双面先例）
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    assert_eq!(
        b.begin(
            ViewKind::Step,
            0,
            &Props::Step(StepProps { value: 11, min: 0, max: 10, step: 1, on_change: 0 })
        ),
        Err(AstError::BadValue { kind: ViewKind::Step, got: 11 })
    );
    assert_eq!(
        b.begin(ViewKind::Seg, 0, &Props::Seg(SegProps { segment_count: 0, active: 0, on_change: 0 })),
        Err(AstError::BadValue { kind: ViewKind::Seg, got: 0 })
    );
    assert_eq!(
        b.begin(
            ViewKind::Sbar,
            0,
            &Props::Sbar(SbarProps {
                orientation: Orientation::Vertical,
                thumb_start: 600,
                thumb_end: 400,
            })
        ),
        Err(AstError::BadValue { kind: ViewKind::Sbar, got: 600 })
    );
}

#[test]
fn p1_unknown_prop_matrix_and_truncation() {
    // Stack align 域外（3）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x10, &[3], 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Stack, got: 3 })
    );
    // Check checked 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x13, &[2, 3, 0, 0, 0], 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Check, got: 2 })
    );
    // Radio selected 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x14, &[2, 7, 0, 0, 0, 4, 0, 0, 0], 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Radio, got: 2 })
    );
    // Badge show_dot 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x17, &[99, 0, 0, 0, 2], 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Badge, got: 2 })
    );
    // Sbar orientation 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = vec![2u8];
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(500));
    assert_eq!(
        ui.apply_desc(&single_stream(0x18, &p, 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Sbar, got: 2 })
    );
    // 截断（截断优先于语义检）：Step 20B 只给 19B / Seg 12B 只给 11B /
    // Sbar 9B 只给 8B
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(5));
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(10));
    p.extend_from_slice(&u32b(1));
    p.extend_from_slice(&[6, 0, 0]);
    assert_eq!(ui.apply_desc(&single_stream(0x15, &p, 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(3));
    p.extend_from_slice(&u32b(1));
    p.extend_from_slice(&[8, 0, 0]);
    assert_eq!(ui.apply_desc(&single_stream(0x16, &p, 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = vec![0u8];
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&[0xF4, 0x01, 0x00]);
    assert_eq!(ui.apply_desc(&single_stream(0x18, &p, 0)), Err(AstError::Truncated));
}

#[test]
fn p1_leaf_children_reject_both_faces_and_size_block() {
    // 流面：Spacer cc=1 → BadParent（P1 八叶带子拒面——检先于子消费，
    // 子字节不必给全）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x11, &[1, 0, 0, 0], 1)),
        Err(AstError::BadParent)
    );
    // builder 面：Check 下 begin 拒
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Stack, 0, &Props::Stack(StackProps { align: StackAlign::Center }))
        .unwrap();
    b.begin(ViewKind::Check, 0, &Props::Check(CheckProps { checked: true, on_change: 3 }))
        .unwrap();
    assert_eq!(
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None, radius: 0 })),
        Err(AstError::BadParent)
    );
    // 对照：Stack 容器带子合法（golden 已钉）；SIZE 块×新 kind 拒
    // （§8.2 合法域表未扩——Stack 容器同拒，合法域检在长检前）
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x10, 0x02, 0x00];
    v.extend_from_slice(&u32b(7));
    v.extend_from_slice(&u32b(8));
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::Stack, got: 0x02 }),
        "SIZE×Stack 拒（§8.2 表不随 P1 批扩）"
    );
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x17, 0x02];
    v.extend_from_slice(&u32b(99));
    v.push(0x00);
    v.extend_from_slice(&u32b(7));
    v.extend_from_slice(&u32b(8));
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::Badge, got: 0x02 }),
        "SIZE×Badge 拒（叶同面）"
    );
}

#[test]
fn p1_add_face_smoke() {
    // add() 命令面：新 kind 可建可编码（数据层通）；两场对拍 roundtrip。
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    ui.add(
        root,
        ViewSpec { kind: ViewKind::Spacer, key: 0, props: Props::Spacer(SpacerProps { flex: 2 }) },
    )
    .unwrap();
    let mut out = [0u8; 64];
    let n = ui.encode_desc(&mut out).unwrap();
    let mut ui2 = Ui::<8>::new(10, 10);
    ui2.apply_desc(&out[..n]).unwrap();
    let mut out2 = [0u8; 64];
    let n2 = ui2.encode_desc(&mut out2).unwrap();
    assert_eq!(&out[..n], &out2[..n2], "add 面产物与 AST 面产物同流");

    // 新叶父位拒（lib.rs 叶子显式列名外走 catch-all——P1 已知边界钉现行为）
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    let ck = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Check,
                key: 0,
                props: Props::Check(CheckProps { checked: false, on_change: 1 }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.add(
            ck,
            ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None, radius: 0 }) }
        ),
        Err(berry_ui::UiError::BadParent),
        "Check 叶父位拒"
    );
    // Stack 容器命令面父位收编（Phase 2 第二批——叠层容器入 add 白名
    // 单；子经 place_children `_` 臂全幅叠放）。
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    let st = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Stack,
                key: 0,
                props: Props::Stack(StackProps { align: StackAlign::Start }),
            },
        )
        .unwrap();
    let _ = st;
    assert!(
        ui.add(
            st,
            ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None, radius: 0 }) }
        )
        .is_ok(),
        "Stack 命令面父位收编（Phase 2 第二批接线）"
    );
}
