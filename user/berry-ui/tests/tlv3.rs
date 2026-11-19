//! Phase 1 组件七件 TLV 钉板（组件清单 Phase 1——ViewKind 9→16，
//! tag 9..=15 add-only）。tlv2.rs 同款双轨：
//!
//! - **GOLDEN_V3_STREAM 字面量**：七新 kind（Button/Toggle/Slider/
//!   Progress/Scroll/List/Tabs）齐流的样本树现产物钉死（V1/V2 golden
//!   在 reconcile.rs/tlv2.rs 各自另钉——三轨不并常量）。roundtrip 测试
//!   **辅助**（编码器漂移由字面量守卫拦，roundtrip 只证格式自洽）。
//! - 拒面值级：BadValue 族（Slider value>1000 / min>max、Progress
//!   value>1000、Scroll offset>content、Tabs 零页/active 越页——流面
//!   与 builder 生产面同律）、UnknownProp 族（style/checked 域外）、
//!   截断（变长 Button 与定长 Slider/Progress）、新叶带子（流面 +
//!   builder 面）、块×kind（SIZE×新 kind 拒——§8.2 表未扩）。
//! - add() 命令面烟囱：新 kind 可建可编码（数据层通），roundtrip 对拍；
//!   新 kind 父位拒（lib.rs 容器白名单未收编——Phase 1 已知边界，钉为
//!   现行为）。
//!
//! 确定性：无随机无时钟；拒面断言到变体载荷字段。

use berry_gfx::cmd::Color;
use berry_ui::desc::{
    AstError, ButtonProps, ButtonStyle, ColProps, DescBuilder, ListProps, ProgressProps,
    ProgressStyle, Props, RectProps, ScrollProps, SliderProps, TabsProps, TextBuf, ToggleProps,
    ViewKind, ViewSpec, KIND_ROOT, PERMILLE_MAX,
};
use berry_ui::layout::CrossAlign;
use berry_ui::{Ui, ROLE_BUTTON};

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

/// Phase 1 样本树 builder 产物 oracle：
/// Root(cc=2) → Col key=0x22 gap=1(cc=4) → {Button label="OK" Filled
/// on_tap=7 [A11Y BUTTON "ok"]，Toggle checked on_change=9，Slider
/// value=250 min=0 max=100 on_change=11，Progress value=1000 Linear}；
/// Tabs tab_count=2 active=0 on_switch=5(cc=1) → Rect(9,8,7)。
fn v3_oracle_bytes() -> Vec<u8> {
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Col,
        0x22,
        &Props::Col(ColProps { gap: 1, w: None, h: None, cross: CrossAlign::Stretch }),
    )
    .unwrap();
    b.begin(
        ViewKind::Button,
        0,
        &Props::Button(ButtonProps {
            label: TextBuf::new(b"OK").unwrap(),
            style: ButtonStyle::Filled,
            on_tap: 7,
        }),
    )
    .unwrap();
    b.a11y(ROLE_BUTTON, b"ok").unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Toggle, 0, &Props::Toggle(ToggleProps { checked: true, on_change: 9 }))
        .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Slider,
        0,
        &Props::Slider(SliderProps { value: 250, min: 0, max: 100, on_change: 11 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Progress,
        0,
        &Props::Progress(ProgressProps { value: PERMILLE_MAX, style: ProgressStyle::Linear }),
    )
    .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Col
    b.begin(ViewKind::Tabs, 0, &Props::Tabs(TabsProps { tab_count: 2, active: 0, on_switch: 5 }))
        .unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(9, 8, 7), w: None, h: None, radius: 0 }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Tabs
    b.end().unwrap(); // Root
    b.bytes().to_vec()
}

/// Phase 1 golden：七新 kind 现产物钉字面量（Button 变长 6+len、
/// Toggle/Progress 5B、Slider 16B、Tabs 12B；A11Y×Button 合法——§8.2
/// 全 kind；Tabs 容器带子）。Scroll/List 的 8B 定长在拒面测里另钉。
const GOLDEN_V3_STREAM: &[u8] = &[
    0x00, 0x00, 0x02, // Root（cc=2）
    0x03, 0x01, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Col key=0x22 flags=KEY
    0x01, 0x00, 0x00, 0x00, // gap=1
    0x04, // cc=4
    0x09, 0x04, // Button flags=A11Y
    0x02, b'O', b'K', 0x00, 0x07, 0x00, 0x00, 0x00, // label="OK" Filled on_tap=7
    0x01, 0x02, b'o', b'k', // A11Y 块：BUTTON "ok"
    0x00, // cc=0
    0x0A, 0x00, 0x01, 0x09, 0x00, 0x00, 0x00, 0x00, // Toggle checked on_change=9；cc=0
    0x0B, 0x00, // Slider
    0xFA, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x0B, 0x00, 0x00,
    0x00, // value=250 min=0 max=100 on_change=11
    0x00, // cc=0
    0x0C, 0x00, 0xE8, 0x03, 0x00, 0x00, 0x00, 0x00, // Progress value=1000 Linear；cc=0
    0x0F, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, // Tabs 2 页 active=0 on_switch=5
    0x01, // cc=1
    0x01, 0x00, 0x09, 0x08, 0x07, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect(9,8,7)
];

#[test]
fn golden_v3_stream_pinned_literal() {
    assert_eq!(
        v3_oracle_bytes().as_slice(),
        GOLDEN_V3_STREAM,
        "v3 流字节漂移——编码器/builder 不兼容变更（字面量守卫）"
    );
}

#[test]
fn v3_roundtrip_through_arena_and_a11y_table() {
    // 辅助面：builder 流 → apply（arena + a11y 侧表落位）→ 再 encode
    // 逐字节相等（A11Y 块经侧表往返；Tabs 子树照常重建）。
    let mut ui = Ui::<32>::new(100, 100);
    ui.apply_desc(GOLDEN_V3_STREAM).unwrap();
    assert!(ui.damage().full, "apply 成功 → FULL damage");
    let mut out = [0u8; 256];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], GOLDEN_V3_STREAM, "roundtrip 字节相等（含 a11y 侧表往返）");
}

/// Scroll/List 也进 roundtrip（golden 树未含——两件 8B 定长在拒面测的
/// 合法对照组里走全环）。
#[test]
fn scroll_list_roundtrip_and_add_face() {
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(ViewKind::Scroll, 0, &Props::Scroll(ScrollProps { content_height: 2000, offset_y: 100 }))
        .unwrap();
    b.begin(ViewKind::List, 0, &Props::List(ListProps { item_count: 3, item_height: 24 }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap();
    b.end().unwrap();
    let mut ui = Ui::<16>::new(100, 100);
    ui.apply_desc(b.bytes()).unwrap();
    let mut out = [0u8; 128];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], b.bytes(), "Scroll>List roundtrip 字节相等");

    // add() 命令面：新 kind 可建可编码（数据层通）；两场对拍 roundtrip。
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    let list = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::List,
                key: 0,
                props: Props::List(ListProps { item_count: 2, item_height: 20 }),
            },
        )
        .unwrap();
    let _ = list;
    let mut out = [0u8; 64];
    let n = ui.encode_desc(&mut out).unwrap();
    let mut ui2 = Ui::<8>::new(10, 10);
    ui2.apply_desc(&out[..n]).unwrap();
    let mut out2 = [0u8; 64];
    let n2 = ui2.encode_desc(&mut out2).unwrap();
    assert_eq!(&out[..n], &out2[..n2], "add 面产物与 AST 面产物同流");

    // 新 kind 父位拒（lib.rs 容器白名单未收编——Phase 1 已知边界钉现行为）
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    let btn = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Button,
                key: 0,
                props: Props::Button(ButtonProps {
                    label: TextBuf::new(b"x").unwrap(),
                    style: ButtonStyle::Text,
                    on_tap: 1,
                }),
            },
        )
        .unwrap();
    assert_eq!(
        ui.add(
            btn,
            ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None, radius: 0 }) }
        ),
        Err(berry_ui::UiError::BadParent),
        "Button 叶父位拒（含新叶）"
    );
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
fn bad_value_matrix_stream_and_builder_faces() {
    // Slider value > 1000（got = value）
    let mut ui = Ui::<8>::new(10, 10);
    let mut s = vec![0x0B, 0x00];
    s.extend_from_slice(&u32b(1001));
    s.extend_from_slice(&u32b(0));
    s.extend_from_slice(&u32b(100));
    s.extend_from_slice(&u32b(0));
    assert_eq!(
        ui.apply_desc(&single_stream(0x0B, &s[2..], 0)),
        Err(AstError::BadValue { kind: ViewKind::Slider, got: 1001 })
    );
    // Slider min > max（got = min）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(250));
    p.extend_from_slice(&u32b(5));
    p.extend_from_slice(&u32b(2));
    p.extend_from_slice(&u32b(0));
    assert_eq!(
        ui.apply_desc(&single_stream(0x0B, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Slider, got: 5 })
    );
    // Progress value > 1000
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(PERMILLE_MAX + 1));
    p.push(0);
    assert_eq!(
        ui.apply_desc(&single_stream(0x0C, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Progress, got: 1001 })
    );
    // Scroll offset > content
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(100));
    p.extend_from_slice(&u32b(101));
    assert_eq!(
        ui.apply_desc(&single_stream(0x0D, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Scroll, got: 101 })
    );
    // Tabs 零页（got = 0）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(5));
    assert_eq!(
        ui.apply_desc(&single_stream(0x0F, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Tabs, got: 0 })
    );
    // Tabs active 越页（got = active）
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(2));
    p.extend_from_slice(&u32b(2));
    p.extend_from_slice(&u32b(5));
    assert_eq!(
        ui.apply_desc(&single_stream(0x0F, &p, 0)),
        Err(AstError::BadValue { kind: ViewKind::Tabs, got: 2 })
    );

    // builder 生产面同律（validate_phase1——Grid 双面先例）
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    assert_eq!(
        b.begin(
            ViewKind::Slider,
            0,
            &Props::Slider(SliderProps { value: 1001, min: 0, max: 100, on_change: 0 })
        ),
        Err(AstError::BadValue { kind: ViewKind::Slider, got: 1001 })
    );
    assert_eq!(
        b.begin(ViewKind::Tabs, 0, &Props::Tabs(TabsProps { tab_count: 2, active: 2, on_switch: 0 })),
        Err(AstError::BadValue { kind: ViewKind::Tabs, got: 2 })
    );
}

#[test]
fn unknown_prop_matrix_and_truncation() {
    // Button style 域外（3）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x09, &[1, b'x', 3, 7, 0, 0, 0], 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Button, got: 3 })
    );
    // Toggle checked 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x0A, &[2, 9, 0, 0, 0], 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Toggle, got: 2 })
    );
    // Progress style 域外（3）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x0C, &[0xE8, 0x03, 0x00, 0x00, 3], 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Progress, got: 3 })
    );
    // Button label 超长（len=65 > TEXT_INLINE=64）
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x09, 0x00, 65];
    v.extend_from_slice(&[b'x'; 65]);
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::TextTooLong { len: berry_ui::TEXT_INLINE + 1 })
    );
    // 截断（截断优先于语义检）：Slider 16B 只给 15B / Progress 5B 只给
    // 4B / Button 变长 declared len 越可用字节
    let mut ui = Ui::<8>::new(10, 10);
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(250));
    p.extend_from_slice(&u32b(0));
    p.extend_from_slice(&u32b(100));
    p.extend_from_slice(&[0, 0, 0]);
    assert_eq!(ui.apply_desc(&single_stream(0x0B, &p, 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x0C, &[0xE8, 0x03, 0x00, 0x00], 0)),
        Err(AstError::Truncated)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x09, &[4, b'a', b'b'], 0)),
        Err(AstError::Truncated)
    );
}

#[test]
fn new_leaf_children_reject_both_faces_and_size_block() {
    // 流面：Button cc=1 → BadParent（叶带子拒面扩列——检先于子消费，
    // 子字节不必给全）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x09, &[0, 0, 7, 0, 0, 0], 1)),
        Err(AstError::BadParent)
    );
    // builder 面：Button 下 begin 拒
    let mut b = DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Button,
        0,
        &Props::Button(ButtonProps {
            label: TextBuf::new(b"OK").unwrap(),
            style: ButtonStyle::Filled,
            on_tap: 7,
        }),
    )
    .unwrap();
    assert_eq!(
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None, radius: 0 })),
        Err(AstError::BadParent)
    );
    // 对照：Tabs 容器带子合法（golden 已钉）；SIZE 块×新 kind 拒
    // （§8.2 合法域表未扩——Toggle+SIZE）
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x0A, 0x02, 1];
    v.extend_from_slice(&u32b(9));
    v.extend_from_slice(&u32b(7));
    v.extend_from_slice(&u32b(u32::MAX));
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::Toggle, got: 0x02 }),
        "SIZE×Toggle 拒（§8.2 表不随 Phase 1 扩）"
    );
}
