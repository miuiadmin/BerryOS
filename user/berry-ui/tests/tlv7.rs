//! P2 借来件七组件 TLV 钉板（ViewKind 34→41，tag 34..=40 add-only）。
//! tlv6.rs 同款双轨：
//!
//! - **GOLDEN_V7_STREAM 字面量**：七新 kind（Collapse/Chip/Fab/Card/
//!   Gauge/Qr/DragReorder）齐流的样本树现产物钉死（V1..V6 golden 在
//!   reconcile.rs/tlv2.rs~tlv6.rs 各自另钉——七轨不并常量）。roundtrip
//!   测试**辅助**（编码器漂移由字面量守卫拦，roundtrip 只证格式自洽）。
//! - 拒面值级：BadValue 族（Card elevation > 3；Gauge value > 1000；
//!   Qr module_count 不在合法 QR 版本域 {21,25,29,...,177}；DragReorder
//!   item_index >= total_items（total_items > 0 时））——流面与 builder
//!   生产面同律；UnknownProp 族（expanded/selected bool 子域外、gauge
//!   style 枚举域外）、TextTooLong（Chip/Fab 文本超长）、截断（Chip
//!   变长 6..=70 / Fab 变长 9..=73 / Collapse 5B / Card 8B / Gauge 5B /
//!   Qr 12B / DragReorder 12B）、五新叶带子（流面 + builder 面；
//!   Collapse/Card = 容器正例对照）、块×kind（SIZE×新 kind 拒——
//!   §8.2 表未扩）。
//! - add() 命令面烟囱：新 kind 可建可编码（数据层通），roundtrip 对拍；
//!   新容器父位拒（lib.rs 容器白名单未收编——前批同款已知边界，钉
//!   为现行为）。
//!
//! 确定性：无随机无时钟；拒面断言到变体载荷字段。

use berry_gfx::cmd::Color;
use berry_ui::desc::{
    AstError, CardProps, ChipProps, ColProps, CollapseProps, DragReorderProps, FabProps,
    GaugeProps, GaugeStyle, Props, QrProps, RectProps, ViewKind, ViewSpec, KIND_ROOT,
};
use berry_ui::layout::CrossAlign;
use berry_ui::{Ui, ROLE_BUTTON};

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

/// P2 七件样本树 builder 产物 oracle：Root(cc=1) → Col gap=0(cc=7)
/// → {Collapse 展开中 on_toggle=1 内包 Rect(1,2,3)，Chip "Tag" 选中
/// on_tap=2 [A11Y BUTTON "chip"]，Fab icon=5 空标签（纯圆形）on_tap=3，
/// Card elevation=2 corner_radius=8 内包 Rect(4,5,6)，Gauge value=250
/// Ring，Qr 25 模块黑/白，DragReorder 1/3 on_reorder=4}。
fn v7_oracle_bytes() -> Vec<u8> {
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Col,
        0,
        &Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }),
    )
    .unwrap();
    // Collapse = 容器（折叠内容——AST/builder 面可带子的本批正例一）
    b.begin(
        ViewKind::Collapse,
        0,
        &Props::Collapse(CollapseProps { expanded: true, on_toggle: 1 }),
    )
    .unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Collapse
    b.begin(
        ViewKind::Chip,
        0,
        &Props::Chip(ChipProps {
            label: berry_ui::TextBuf::new(b"Tag").unwrap(),
            selected: true,
            on_tap: 2,
        }),
    )
    .unwrap();
    // Chip = 独立 kind 为 a11y role 锚——A11Y 块钉BUTTON 角色
    b.a11y(ROLE_BUTTON, b"chip").unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Fab,
        0,
        &Props::Fab(FabProps {
            icon: 5,
            extended_label: berry_ui::TextBuf::new(b"").unwrap(),
            on_tap: 3,
        }),
    )
    .unwrap();
    b.end().unwrap();
    // Card = 容器（卡片内容——本批正例二）
    b.begin(
        ViewKind::Card,
        0,
        &Props::Card(CardProps { elevation: 2, corner_radius: 8 }),
    )
    .unwrap();
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(4, 5, 6), w: None, h: None }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Card
    b.begin(
        ViewKind::Gauge,
        0,
        &Props::Gauge(GaugeProps { value: 250, style: GaugeStyle::Ring }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Qr,
        0,
        &Props::Qr(QrProps {
            module_count: 25,
            dark_color: 0xFF00_0000,
            light_color: 0xFFFF_FFFF,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::DragReorder,
        0,
        &Props::DragReorder(DragReorderProps { item_index: 1, total_items: 3, on_reorder: 4 }),
    )
    .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Col
    b.end().unwrap(); // Root
    b.bytes().to_vec()
}

/// P2 七件 golden：七新 kind 现产物钉字面量（Collapse 5B、Chip 变长
/// 6..=70、Fab 变长 9..=73、Card 8B、Gauge 5B、Qr 12B、DragReorder 12B；
/// A11Y×Chip 合法——§8.2 全 kind；Collapse/Card 容器带子）。
const GOLDEN_V7_STREAM: &[u8] = &[
    0x00, 0x00, 0x01, // Root（cc=1）
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, // Col gap=0；cc=7
    0x22, 0x00, // Collapse
    0x01, // expanded=true
    0x01, 0x00, 0x00, 0x00, // on_toggle=1
    0x01, // cc=1（容器——折叠内容）
    0x01, 0x00, 0x01, 0x02, 0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect(1,2,3)
    0x23, 0x04, // Chip flags=A11Y
    0x03, b'T', b'a', b'g', // label="Tag"
    0x01, // selected=true
    0x02, 0x00, 0x00, 0x00, // on_tap=2
    0x01, 0x04, b'c', b'h', b'i', b'p', // A11Y 块：BUTTON "chip"
    0x00, // cc=0
    0x24, 0x00, // Fab
    0x05, 0x00, 0x00, 0x00, // icon=5
    0x00, // len=0（空标签 = 纯圆形）
    0x03, 0x00, 0x00, 0x00, // on_tap=3
    0x00, // cc=0
    0x25, 0x00, // Card
    0x02, 0x00, 0x00, 0x00, // elevation=2
    0x08, 0x00, 0x00, 0x00, // corner_radius=8
    0x01, // cc=1（容器——卡片内容）
    0x01, 0x00, 0x04, 0x05, 0x06, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect(4,5,6)
    0x26, 0x00, // Gauge
    0xFA, 0x00, 0x00, 0x00, // value=250
    0x00, // style=Ring
    0x00, // cc=0
    0x27, 0x00, // Qr
    0x19, 0x00, 0x00, 0x00, // module_count=25
    0x00, 0x00, 0x00, 0xFF, // dark=0xFF000000
    0xFF, 0xFF, 0xFF, 0xFF, // light=0xFFFFFFFF
    0x00, // cc=0
    0x28, 0x00, // DragReorder
    0x01, 0x00, 0x00, 0x00, // item_index=1
    0x03, 0x00, 0x00, 0x00, // total_items=3
    0x04, 0x00, 0x00, 0x00, // on_reorder=4
    0x00, // cc=0
];

#[test]
fn golden_v7_stream_pinned_literal() {
    assert_eq!(
        v7_oracle_bytes().as_slice(),
        GOLDEN_V7_STREAM,
        "v7 流字节漂移——编码器/builder 不兼容变更（字面量守卫）"
    );
}

#[test]
fn v7_roundtrip_through_arena_and_a11y_table() {
    // 辅助面：builder 流 → apply（arena + a11y 侧表落位）→ 再 encode
    // 逐字节相等（A11Y 块经侧表往返；Collapse/Card 子树照常重建）。
    let mut ui = Ui::<32>::new(100, 100);
    ui.apply_desc(GOLDEN_V7_STREAM).unwrap();
    assert!(ui.damage().full, "apply 成功 → FULL damage");
    let mut out = [0u8; 256];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], GOLDEN_V7_STREAM, "roundtrip 字节相等（含 a11y 侧表往返）");
}

/// golden 未覆盖的对称值档：Collapse 收起 on_toggle=0 / Chip 空标签
/// 未选 / Fab icon=0 扩展标签 "Compose"（非空 = 扩展形态）/ Card
/// elevation=0 corner_radius=0（零阴影零圆角）/ Gauge value=1000 帽
/// Arc / Qr module_count=177（版本 40 上界）/ DragReorder 0/0（空表
/// ——零项合法，Select 同律）——全环 roundtrip 字节相等。
#[test]
fn v7_alt_values_roundtrip() {
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Collapse,
        0,
        &Props::Collapse(CollapseProps { expanded: false, on_toggle: 0 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Chip,
        0x55,
        &Props::Chip(ChipProps {
            label: berry_ui::TextBuf::new(b"").unwrap(),
            selected: false,
            on_tap: 0,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Fab,
        0,
        &Props::Fab(FabProps {
            icon: 0,
            extended_label: berry_ui::TextBuf::new(b"Compose").unwrap(),
            on_tap: 0,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(ViewKind::Card, 0, &Props::Card(CardProps { elevation: 0, corner_radius: 0 }))
        .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Gauge,
        0,
        &Props::Gauge(GaugeProps { value: 1000, style: GaugeStyle::Arc }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Qr,
        0,
        &Props::Qr(QrProps {
            module_count: 177,
            dark_color: 0x0000_0000,
            light_color: 0x00FF_00FF,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::DragReorder,
        0,
        &Props::DragReorder(DragReorderProps { item_index: 0, total_items: 0, on_reorder: 0 }),
    )
    .unwrap();
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

/// Collapse 5B props 域（expanded/on_toggle）。
fn collapse_props(expanded: u8, on_toggle: u32) -> Vec<u8> {
    let mut p = vec![expanded];
    p.extend_from_slice(&u32b(on_toggle));
    p
}

/// Chip 变长 props 域（len + label + selected + on_tap）。
fn chip_bytes(label: &[u8], selected: u8, on_tap: u32) -> Vec<u8> {
    let mut p = vec![label.len() as u8];
    p.extend_from_slice(label);
    p.push(selected);
    p.extend_from_slice(&u32b(on_tap));
    p
}

/// Fab 变长 props 域（icon + len + label + on_tap）。
fn fab_bytes(icon: u32, label: &[u8], on_tap: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(icon));
    p.push(label.len() as u8);
    p.extend_from_slice(label);
    p.extend_from_slice(&u32b(on_tap));
    p
}

/// Card 8B props 域。
fn card_props(elevation: u32, corner_radius: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(elevation));
    p.extend_from_slice(&u32b(corner_radius));
    p
}

/// Gauge 5B props 域。
fn gauge_bytes(value: u32, style: u8) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(value));
    p.push(style);
    p
}

/// Qr 12B props 域。
fn qr_props(module_count: u32, dark: u32, light: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(module_count));
    p.extend_from_slice(&u32b(dark));
    p.extend_from_slice(&u32b(light));
    p
}

/// DragReorder 12B props 域。
fn dr_props(item_index: u32, total_items: u32, on_reorder: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(item_index));
    p.extend_from_slice(&u32b(total_items));
    p.extend_from_slice(&u32b(on_reorder));
    p
}

#[test]
fn p2_bad_value_matrix_stream_and_builder_faces() {
    // Card elevation=4（got = elevation）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x25, &card_props(4, 8), 0)),
        Err(AstError::BadValue { kind: ViewKind::Card, got: 4 })
    );
    // 对照：elevation=3 帽合法（流面正例）
    let mut ui = Ui::<8>::new(10, 10);
    ui.apply_desc(&single_stream(0x25, &card_props(3, 0), 0)).unwrap();

    // Gauge value=1001（got = value）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x26, &gauge_bytes(1001, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::Gauge, got: 1001 })
    );
    // 对照：value=1000 帽合法
    let mut ui = Ui::<8>::new(10, 10);
    ui.apply_desc(&single_stream(0x26, &gauge_bytes(1000, 2), 0)).unwrap();

    // Qr module_count=22（区间内但非 21+4k 步长——got = module_count）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x27, &qr_props(22, 0, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::Qr, got: 22 })
    );
    // Qr module_count=17（< 21 下界外）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x27, &qr_props(17, 0, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::Qr, got: 17 })
    );
    // Qr module_count=181（> 177 上界外）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x27, &qr_props(181, 0, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::Qr, got: 181 })
    );
    // 对照：module_count=29 合法（版本 3）
    let mut ui = Ui::<8>::new(10, 10);
    ui.apply_desc(&single_stream(0x27, &qr_props(29, 0, 0), 0)).unwrap();

    // DragReorder item_index=2 total_items=2（越项——got = item_index）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x28, &dr_props(2, 2, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::DragReorder, got: 2 })
    );
    // 对照：total_items=0 空表合法（item_index >= total_items 仅
    // total > 0 时检——Select 零项同律）
    let mut ui = Ui::<8>::new(10, 10);
    ui.apply_desc(&single_stream(0x28, &dr_props(0, 0, 0), 0)).unwrap();

    // builder 生产面同律（validate_p2_seven——validate_phase1 双面先例，
    // 四件各钉一刀）
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    assert_eq!(
        b.begin(ViewKind::Card, 0, &Props::Card(CardProps { elevation: 4, corner_radius: 0 })),
        Err(AstError::BadValue { kind: ViewKind::Card, got: 4 })
    );
    assert_eq!(
        b.begin(
            ViewKind::Gauge,
            0,
            &Props::Gauge(GaugeProps { value: 1001, style: GaugeStyle::Bar })
        ),
        Err(AstError::BadValue { kind: ViewKind::Gauge, got: 1001 })
    );
    assert_eq!(
        b.begin(
            ViewKind::Qr,
            0,
            &Props::Qr(QrProps { module_count: 22, dark_color: 0, light_color: 0 })
        ),
        Err(AstError::BadValue { kind: ViewKind::Qr, got: 22 })
    );
    assert_eq!(
        b.begin(
            ViewKind::DragReorder,
            0,
            &Props::DragReorder(DragReorderProps { item_index: 5, total_items: 5, on_reorder: 0 })
        ),
        Err(AstError::BadValue { kind: ViewKind::DragReorder, got: 5 })
    );
    // Collapse/Chip/Fab 无 BadValue 面（bool 域在 wire 面、文本帽在
    // TextBuf 构造面）——对照正例：
    b.begin(
        ViewKind::Collapse,
        0,
        &Props::Collapse(CollapseProps { expanded: false, on_toggle: 0 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Chip,
        0,
        &Props::Chip(ChipProps {
            label: berry_ui::TextBuf::new(b"x").unwrap(),
            selected: true,
            on_tap: 0,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Fab,
        0,
        &Props::Fab(FabProps {
            icon: 1,
            extended_label: berry_ui::TextBuf::new(b"").unwrap(),
            on_tap: 0,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.end().unwrap();
}

#[test]
fn p2_unknown_prop_matrix_and_truncation() {
    // Collapse expanded 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x22, &collapse_props(2, 0), 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Collapse, got: 2 })
    );
    // Chip selected 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x23, &chip_bytes(b"x", 2, 0), 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Chip, got: 2 })
    );
    // Gauge style 域外（3）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x26, &gauge_bytes(0, 3), 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Gauge, got: 3 })
    );
    // Chip 文本超长（len u8 = 65 > TEXT_INLINE 64）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x23, &[65], 0)),
        Err(AstError::TextTooLong { len: 65 })
    );
    // Fab 文本超长（icon 4B 后 len u8 = 65）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x24, &[0, 0, 0, 0, 65], 0)),
        Err(AstError::TextTooLong { len: 65 })
    );
    // 截断（截断优先于语义检——载荷带越界值可证检序）：Collapse 5B
    // 只给 3B（expanded=2 待拒——need 先响）/ Chip len=2 文本只给 1B /
    // Fab 9..=73 给 icon(4)+len(1) 后缺 label+on_tap / Card 8B 只给 6B
    //（elevation=4 待拒）/ Gauge 5B 只给 3B（value=1001 待拒）/ Qr 12B
    // 只给 10B（module_count=22 待拒）/ DragReorder 12B 只给 10B
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(ui.apply_desc(&single_stream(0x22, &collapse_props(2, 0)[..3], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(ui.apply_desc(&single_stream(0x23, &[0x02, b'x'], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(ui.apply_desc(&single_stream(0x24, &[0, 0, 0, 0, 2, b'a', b'b'], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let p = card_props(4, 8);
    assert_eq!(ui.apply_desc(&single_stream(0x25, &p[..6], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let p = gauge_bytes(1001, 0);
    assert_eq!(ui.apply_desc(&single_stream(0x26, &p[..3], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let p = qr_props(22, 0, 0);
    assert_eq!(ui.apply_desc(&single_stream(0x27, &p[..10], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let p = dr_props(2, 2, 0);
    assert_eq!(ui.apply_desc(&single_stream(0x28, &p[..10], 0)), Err(AstError::Truncated));
}

#[test]
fn p2_leaf_children_reject_both_faces_and_size_block() {
    // 流面：五新叶 cc=1 → BadParent（检先于子消费，子字节不必给全）；
    // Collapse/Card cc=1 是容器正例（golden 已钉，此处只钉叶）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x23, &chip_bytes(b"", 0, 0), 1)),
        Err(AstError::BadParent)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x24, &fab_bytes(0, b"", 0), 1)),
        Err(AstError::BadParent)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x26, &gauge_bytes(0, 0), 1)),
        Err(AstError::BadParent)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x27, &qr_props(21, 0, 0), 1)),
        Err(AstError::BadParent)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x28, &dr_props(0, 1, 0), 1)),
        Err(AstError::BadParent)
    );
    // builder 面：Chip 下 begin 拒（Fab/Gauge/Qr/DragReorder 同律——
    // begin 父位叶卫单源）
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Chip,
        0,
        &Props::Chip(ChipProps {
            label: berry_ui::TextBuf::new(b"t").unwrap(),
            selected: false,
            on_tap: 0,
        }),
    )
    .unwrap();
    assert_eq!(
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None })),
        Err(AstError::BadParent)
    );
    // SIZE 块×新 kind 拒（§8.2 合法域表未扩——合法域检在长检前）
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x27, 0x02];
    v.extend_from_slice(&qr_props(21, 0, 0));
    v.extend_from_slice(&u32b(7));
    v.extend_from_slice(&u32b(8));
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::Qr, got: 0x02 }),
        "SIZE×Qr 拒（§8.2 表不随 P2 批扩）"
    );
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x25, 0x02];
    v.extend_from_slice(&card_props(1, 4));
    v.extend_from_slice(&u32b(9));
    v.extend_from_slice(&u32b(7));
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::Card, got: 0x02 }),
        "SIZE×Card 拒（容器同拒——§8.2 通则只 Col/Row/Grid；Collapse 同面）"
    );
}

#[test]
fn p2_add_face_smoke() {
    // add() 命令面：新 kind 可建可编码（数据层通）；两场对拍 roundtrip。
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Qr,
            key: 0,
            props: Props::Qr(QrProps {
                module_count: 21,
                dark_color: 0xFF00_0000,
                light_color: 0xFFFF_FFFF,
            }),
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

    // 第三批后 Collapse 已收编容器白名单（place_children Collapse 臂
    // 接线）——前批「父位拒」边界钉随白名单推进翻为收编正例；Card 同
    // 批收编（`_` 臂全幅给子），一并钉。
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    let cpse = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Collapse,
                key: 0,
                props: Props::Collapse(CollapseProps { expanded: false, on_toggle: 1 }),
            },
        )
        .unwrap();
    ui.add(
        cpse,
        ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }) },
    )
    .expect("Collapse 父位收编（第三批容器接线——子 = 折叠内容）");
    let card = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Card,
                key: 0,
                props: Props::Card(CardProps { elevation: 1, corner_radius: 0 }),
            },
        )
        .unwrap();
    ui.add(
        card,
        ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg: bg(4, 5, 6), w: None, h: None }) },
    )
    .expect("Card 父位收编（`_` 臂全幅给子——子 = 卡片内容）");
}
