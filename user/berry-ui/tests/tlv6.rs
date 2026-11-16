//! P1 尾五件 TLV 钉板（组件清单 P1 kind 件收尾批——ViewKind 29→34，
//! tag 29..=33 add-only）。tlv5.rs 同款双轨：
//!
//! - **GOLDEN_V6_STREAM 字面量**：五新 kind（Tip/Refresh/SwipeItem/
//!   DatePick/TimePick）齐流的样本树现产物钉死（V1..V5 golden 在
//!   reconcile.rs/tlv2.rs/tlv3.rs/tlv4.rs/tlv5.rs 各自另钉——六轨不并
//!   常量）。roundtrip 测试**辅助**（编码器漂移由字面量守卫拦，
//!   roundtrip 只证格式自洽）。
//! - 拒面值级：BadValue 族（Refresh offset 越千分域；SwipeItem reveal
//!   任一端越千分域；DatePick month/day 越域、min_year > max_year、
//!   year 越界；TimePick hour > 23 / minute > 59）——流面与 builder
//!   生产面同律；UnknownProp 族（placement/refreshing/use_24h 子域外）、
//!   TextTooLong（Tip 文本超长）、截断（Tip 2..=66 / Refresh 9B /
//!   SwipeItem 16B / DatePick 24B / TimePick 13B）、新四叶带子（流面 +
//!   builder 面；Refresh = 容器正例对照）、块×kind（SIZE×新 kind 拒
//!   ——§8.2 表未扩）。
//! - add() 命令面烟囱：新 kind 可建可编码（数据层通），roundtrip 对拍；
//!   新 kind 父位拒（lib.rs 容器白名单未收编——前批同款已知边界，钉
//!   为现行为）。
//!
//! 确定性：无随机无时钟；拒面断言到变体载荷字段。

use berry_gfx::cmd::Color;
use berry_ui::desc::{
    AstError, ColProps, DatePickProps, ListProps, Props, RefreshProps, RectProps, SwipeItemProps,
    TimePickProps, TipPlacement, TipProps, ViewKind, ViewSpec, KIND_ROOT,
};
use berry_ui::layout::CrossAlign;
use berry_ui::{Ui, ROLE_LIST_ITEM};

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

/// P1 尾五件样本树 builder 产物 oracle：Root(cc=2) → Col gap=0(cc=5)
/// → {Tip "Hint" Top [A11Y LIST_ITEM "ok"]，Refresh 非刷新中 offset=250
/// on_refresh=9 内包 List(10×20)，SwipeItem 2 动作 reveal_start=100
/// reveal_end=0 on_action=7，DatePick 2026-11-14 年域 [1900,2100]
/// on_change=3，TimePick 23:59 24 小时制 on_change=4}；Rect(9,8,7)。
fn v6_oracle_bytes() -> Vec<u8> {
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Col,
        0,
        &Props::Col(ColProps { gap: 0, w: None, h: None, cross: CrossAlign::Stretch }),
    )
    .unwrap();
    b.begin(
        ViewKind::Tip,
        0,
        &Props::Tip(TipProps {
            text: berry_ui::TextBuf::new(b"Hint").unwrap(),
            placement: TipPlacement::Top,
        }),
    )
    .unwrap();
    b.a11y(ROLE_LIST_ITEM, b"ok").unwrap();
    b.end().unwrap();
    // Refresh = 容器（包裹 List——AST/builder 面可带子的本批正例）
    b.begin(
        ViewKind::Refresh,
        0,
        &Props::Refresh(RefreshProps { refreshing: false, offset: 250, on_refresh: 9 }),
    )
    .unwrap();
    b.begin(ViewKind::List, 0, &Props::List(ListProps { item_count: 10, item_height: 20 }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Refresh
    b.begin(
        ViewKind::SwipeItem,
        0,
        &Props::SwipeItem(SwipeItemProps {
            action_count: 2,
            reveal_start: 100,
            reveal_end: 0,
            on_action: 7,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::DatePick,
        0,
        &Props::DatePick(DatePickProps {
            year: 2026,
            month: 11,
            day: 14,
            min_year: 1900,
            max_year: 2100,
            on_change: 3,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::TimePick,
        0,
        &Props::TimePick(TimePickProps { hour: 23, minute: 59, use_24h: true, on_change: 4 }),
    )
    .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Col
    b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(9, 8, 7), w: None, h: None }))
        .unwrap();
    b.end().unwrap();
    b.end().unwrap(); // Root
    b.bytes().to_vec()
}

/// P1 尾五件 golden：五新 kind 现产物钉字面量（Tip 变长 2..=66、
/// Refresh 9B、SwipeItem 16B、DatePick 24B、TimePick 13B；A11Y×Tip
/// 合法——§8.2 全 kind；Refresh 容器带子）。
const GOLDEN_V6_STREAM: &[u8] = &[
    0x00, 0x00, 0x02, // Root（cc=2）
    0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, // Col gap=0；cc=5
    0x1D, 0x04, // Tip flags=A11Y
    0x04, b'H', b'i', b'n', b't', // text="Hint"
    0x00, // placement=Top
    0x05, 0x02, b'o', b'k', // A11Y 块：LIST_ITEM "ok"
    0x00, // cc=0
    0x1E, 0x00, // Refresh
    0x00, // refreshing=false
    0xFA, 0x00, 0x00, 0x00, // offset=250
    0x09, 0x00, 0x00, 0x00, // on_refresh=9
    0x01, // cc=1（容器——包裹 List）
    0x0E, 0x00, // List
    0x0A, 0x00, 0x00, 0x00, // item_count=10
    0x14, 0x00, 0x00, 0x00, // item_height=20
    0x00, // cc=0
    0x1F, 0x00, // SwipeItem
    0x02, 0x00, 0x00, 0x00, // action_count=2
    0x64, 0x00, 0x00, 0x00, // reveal_start=100
    0x00, 0x00, 0x00, 0x00, // reveal_end=0
    0x07, 0x00, 0x00, 0x00, // on_action=7
    0x00, // cc=0
    0x20, 0x00, // DatePick
    0xEA, 0x07, 0x00, 0x00, // year=2026
    0x0B, 0x00, 0x00, 0x00, // month=11
    0x0E, 0x00, 0x00, 0x00, // day=14
    0x6C, 0x07, 0x00, 0x00, // min_year=1900
    0x34, 0x08, 0x00, 0x00, // max_year=2100
    0x03, 0x00, 0x00, 0x00, // on_change=3
    0x00, // cc=0
    0x21, 0x00, // TimePick
    0x17, 0x00, 0x00, 0x00, // hour=23
    0x3B, 0x00, 0x00, 0x00, // minute=59
    0x04, 0x00, 0x00, 0x00, // on_change=4
    0x01, // use_24h=true
    0x00, // cc=0
    0x01, 0x00, 0x09, 0x08, 0x07, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, // Rect(9,8,7)
];

#[test]
fn golden_v6_stream_pinned_literal() {
    assert_eq!(
        v6_oracle_bytes().as_slice(),
        GOLDEN_V6_STREAM,
        "v6 流字节漂移——编码器/builder 不兼容变更（字面量守卫）"
    );
}

#[test]
fn v6_roundtrip_through_arena_and_a11y_table() {
    // 辅助面：builder 流 → apply（arena + a11y 侧表落位）→ 再 encode
    // 逐字节相等（A11Y 块经侧表往返；Refresh 子树照常重建）。
    let mut ui = Ui::<32>::new(100, 100);
    ui.apply_desc(GOLDEN_V6_STREAM).unwrap();
    assert!(ui.damage().full, "apply 成功 → FULL damage");
    let mut out = [0u8; 256];
    let n = ui.encode_desc(&mut out).unwrap();
    assert_eq!(&out[..n], GOLDEN_V6_STREAM, "roundtrip 字节相等（含 a11y 侧表往返）");
}

/// golden 未覆盖的对称值档：Tip 空文本 End / Refresh 刷新中 offset=1000
/// 帽 / SwipeItem 零动作 + 双端 1000（两端独立无序）/ DatePick 年域
/// [0,0] 退化为单点（数据层不立法历法纪元；2 月 31 结构合法另钉）/
/// TimePick 0:00 12 小时制——全环 roundtrip 字节相等。
#[test]
fn v6_alt_values_roundtrip() {
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Tip,
        0x55,
        &Props::Tip(TipProps {
            text: berry_ui::TextBuf::new(b"").unwrap(),
            placement: TipPlacement::End,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::Refresh,
        0,
        &Props::Refresh(RefreshProps { refreshing: true, offset: 1000, on_refresh: 0 }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::SwipeItem,
        0,
        &Props::SwipeItem(SwipeItemProps {
            action_count: 0,
            reveal_start: 1000,
            reveal_end: 1000,
            on_action: 0,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::DatePick,
        0,
        &Props::DatePick(DatePickProps {
            year: 0,
            month: 1,
            day: 1,
            min_year: 0,
            max_year: 0,
            on_change: 0,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.begin(
        ViewKind::TimePick,
        0,
        &Props::TimePick(TimePickProps { hour: 0, minute: 0, use_24h: false, on_change: 0 }),
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

/// Refresh 9B props 域（refreshing/offset/on_refresh）。
fn refresh_props(refreshing: u8, offset: u32, on_refresh: u32) -> Vec<u8> {
    let mut p = vec![refreshing];
    p.extend_from_slice(&u32b(offset));
    p.extend_from_slice(&u32b(on_refresh));
    p
}

/// SwipeItem 16B props 域。
fn swipe_props(action_count: u32, reveal_start: u32, reveal_end: u32, on_action: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(action_count));
    p.extend_from_slice(&u32b(reveal_start));
    p.extend_from_slice(&u32b(reveal_end));
    p.extend_from_slice(&u32b(on_action));
    p
}

/// DatePick 24B props 域。
fn date_props(year: u32, month: u32, day: u32, min: u32, max: u32, on_change: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(year));
    p.extend_from_slice(&u32b(month));
    p.extend_from_slice(&u32b(day));
    p.extend_from_slice(&u32b(min));
    p.extend_from_slice(&u32b(max));
    p.extend_from_slice(&u32b(on_change));
    p
}

/// TimePick 13B props 域。
fn time_props(hour: u32, minute: u32, on_change: u32, use_24h: u8) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&u32b(hour));
    p.extend_from_slice(&u32b(minute));
    p.extend_from_slice(&u32b(on_change));
    p.push(use_24h);
    p
}

#[test]
fn p1f_bad_value_matrix_stream_and_builder_faces() {
    // Refresh offset=1001（got = offset）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1E, &refresh_props(0, 1001, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::Refresh, got: 1001 })
    );
    // 对照：offset=1000 帽合法（流面正例）
    let mut ui = Ui::<8>::new(10, 10);
    ui.apply_desc(&single_stream(0x1E, &refresh_props(0, 1000, 0), 0)).unwrap();

    // SwipeItem reveal_start=1001（got = reveal_start）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1F, &swipe_props(1, 1001, 0, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::SwipeItem, got: 1001 })
    );
    // SwipeItem reveal_end=5000（got = reveal_end）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1F, &swipe_props(1, 0, 5000, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::SwipeItem, got: 5000 })
    );

    // DatePick month=0 / month=13（got = month）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x20, &date_props(2026, 0, 14, 1900, 2100, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::DatePick, got: 0 })
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x20, &date_props(2026, 13, 14, 1900, 2100, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::DatePick, got: 13 })
    );
    // DatePick day=0 / day=32（got = day）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x20, &date_props(2026, 11, 0, 1900, 2100, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::DatePick, got: 0 })
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x20, &date_props(2026, 11, 32, 1900, 2100, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::DatePick, got: 32 })
    );
    // DatePick min_year > max_year（got = min_year）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x20, &date_props(2026, 11, 14, 2101, 2100, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::DatePick, got: 2101 })
    );
    // DatePick year < min / year > max（got = year）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x20, &date_props(1899, 11, 14, 1900, 2100, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::DatePick, got: 1899 })
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x20, &date_props(2101, 11, 14, 1900, 2100, 0), 0)),
        Err(AstError::BadValue { kind: ViewKind::DatePick, got: 2101 })
    );
    // 对照：2 月 31 结构合法（day 不做月长感知——消费面钳，正例钉）
    let mut ui = Ui::<8>::new(10, 10);
    ui.apply_desc(&single_stream(0x20, &date_props(2026, 2, 31, 1900, 2100, 0), 0)).unwrap();

    // TimePick hour=24 / minute=60（got = 违规值）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x21, &time_props(24, 0, 0, 1), 0)),
        Err(AstError::BadValue { kind: ViewKind::TimePick, got: 24 })
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x21, &time_props(0, 60, 0, 1), 0)),
        Err(AstError::BadValue { kind: ViewKind::TimePick, got: 60 })
    );

    // builder 生产面同律（validate_p1_last_five——validate_phase1 双面先例，
    // 五件各钉一刀）
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    assert_eq!(
        b.begin(
            ViewKind::Refresh,
            0,
            &Props::Refresh(RefreshProps { refreshing: false, offset: 1001, on_refresh: 0 })
        ),
        Err(AstError::BadValue { kind: ViewKind::Refresh, got: 1001 })
    );
    assert_eq!(
        b.begin(
            ViewKind::SwipeItem,
            0,
            &Props::SwipeItem(SwipeItemProps {
                action_count: 1,
                reveal_start: 0,
                reveal_end: 1001,
                on_action: 0
            })
        ),
        Err(AstError::BadValue { kind: ViewKind::SwipeItem, got: 1001 })
    );
    assert_eq!(
        b.begin(
            ViewKind::DatePick,
            0,
            &Props::DatePick(DatePickProps {
                year: 2026,
                month: 11,
                day: 14,
                min_year: 2101,
                max_year: 2100,
                on_change: 0
            })
        ),
        Err(AstError::BadValue { kind: ViewKind::DatePick, got: 2101 })
    );
    assert_eq!(
        b.begin(
            ViewKind::TimePick,
            0,
            &Props::TimePick(TimePickProps { hour: 0, minute: 60, use_24h: true, on_change: 0 })
        ),
        Err(AstError::BadValue { kind: ViewKind::TimePick, got: 60 })
    );
    // Tip 无 BadValue 面（文本帽在 TextBuf 构造面）——builder 域外
    // placement 不经 wire（枚举值面无域外），对照正例：
    b.begin(
        ViewKind::Tip,
        0,
        &Props::Tip(TipProps {
            text: berry_ui::TextBuf::new(b"x").unwrap(),
            placement: TipPlacement::Start,
        }),
    )
    .unwrap();
    b.end().unwrap();
    b.end().unwrap();
}

#[test]
fn p1f_unknown_prop_matrix_and_truncation() {
    // Tip placement 域外（4）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1D, &[0x01, b'x', 4], 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Tip, got: 4 })
    );
    // Refresh refreshing 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1E, &refresh_props(2, 0, 0), 0)),
        Err(AstError::UnknownProp { kind: ViewKind::Refresh, got: 2 })
    );
    // TimePick use_24h 域外（2）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x21, &time_props(0, 0, 0, 2), 0)),
        Err(AstError::UnknownProp { kind: ViewKind::TimePick, got: 2 })
    );
    // Tip 文本超长（len u8 = 65 > TEXT_INLINE 64）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1D, &[65], 0)),
        Err(AstError::TextTooLong { len: 65 })
    );
    // 截断（截断优先于语义检——各件给足 kind/flags 但 props 短 ≥2B，
    // cc 单字节回填不了 need 帽；载荷带越界值可证检序）：Tip 文本 4B
    // 只给 2B / Refresh 9B 只给 7B（offset=1001 待拒——need 先响）/
    // SwipeItem 16B 只给 14B（reveal_start=1001 同）/ DatePick 24B 只给
    // 22B（month=13 同）/ TimePick 13B 只给 11B（hour=24 同）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(ui.apply_desc(&single_stream(0x1D, &[0x02, b'x'], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let p = refresh_props(0, 1001, 0);
    assert_eq!(ui.apply_desc(&single_stream(0x1E, &p[..7], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let p = swipe_props(0, 1001, 0, 0);
    assert_eq!(ui.apply_desc(&single_stream(0x1F, &p[..14], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let p = date_props(2026, 13, 14, 1900, 2100, 0);
    assert_eq!(ui.apply_desc(&single_stream(0x20, &p[..22], 0)), Err(AstError::Truncated));
    let mut ui = Ui::<8>::new(10, 10);
    let p = time_props(24, 60, 0, 1);
    assert_eq!(ui.apply_desc(&single_stream(0x21, &p[..11], 0)), Err(AstError::Truncated));
}

#[test]
fn p1f_leaf_children_reject_both_faces_and_size_block() {
    // 流面：四新叶 cc=1 → BadParent（检先于子消费，子字节不必给全）；
    // Refresh cc=1 是容器正例（golden 已钉，此处只钉叶）
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1D, &[0x00, 0x00], 1)),
        Err(AstError::BadParent)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x1F, &swipe_props(0, 0, 0, 0), 1)),
        Err(AstError::BadParent)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x20, &date_props(2026, 11, 14, 1900, 2100, 0), 1)),
        Err(AstError::BadParent)
    );
    let mut ui = Ui::<8>::new(10, 10);
    assert_eq!(
        ui.apply_desc(&single_stream(0x21, &time_props(0, 0, 0, 0), 1)),
        Err(AstError::BadParent)
    );
    // builder 面：Tip 下 begin 拒（DatePick/SwipeItem/TimePick 同律——
    // begin 父位叶卫单源）
    let mut b = berry_ui::desc::DescBuilder::new();
    b.begin(ViewKind::Root, 0, &Props::Root).unwrap();
    b.begin(
        ViewKind::Tip,
        0,
        &Props::Tip(TipProps {
            text: berry_ui::TextBuf::new(b"t").unwrap(),
            placement: TipPlacement::Top,
        }),
    )
    .unwrap();
    assert_eq!(
        b.begin(ViewKind::Rect, 0, &Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None })),
        Err(AstError::BadParent)
    );
    // SIZE 块×新 kind 拒（§8.2 合法域表未扩——合法域检在长检前）
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x1D, 0x02];
    v.extend_from_slice(&[0x00, 0x00]); // Tip 空文本 Top
    v.extend_from_slice(&u32b(7));
    v.extend_from_slice(&u32b(8));
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::Tip, got: 0x02 }),
        "SIZE×Tip 拒（§8.2 表不随 P1 尾五批扩）"
    );
    let mut ui = Ui::<8>::new(10, 10);
    let mut v = vec![KIND_ROOT, 0x00, 0x01, 0x21, 0x02];
    v.extend_from_slice(&time_props(0, 0, 0, 0));
    v.extend_from_slice(&u32b(9));
    v.extend_from_slice(&u32b(7));
    v.push(0);
    assert_eq!(
        ui.apply_desc(&v),
        Err(AstError::UnknownProp { kind: ViewKind::TimePick, got: 0x02 }),
        "SIZE×TimePick 拒（叶同面；Refresh 容器同拒——§8.2 通则只 Col/Row/Grid）"
    );
}

#[test]
fn p1f_add_face_smoke() {
    // add() 命令面：新 kind 可建可编码（数据层通）；两场对拍 roundtrip。
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    ui.add(
        root,
        ViewSpec {
            kind: ViewKind::TimePick,
            key: 0,
            props: Props::TimePick(TimePickProps { hour: 9, minute: 30, use_24h: true, on_change: 2 }),
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

    // Refresh 容器父位收编（B1 白名单补漏——desc.rs AST/builder 面本就
    // 按容器受理，add 面同律；回归钉板）
    let mut ui = Ui::<8>::new(10, 10);
    let root = ui.root();
    let rf = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Refresh,
                key: 0,
                props: Props::Refresh(RefreshProps { refreshing: false, offset: 0, on_refresh: 1 }),
            },
        )
        .unwrap();
    assert!(
        ui.add(
            rf,
            ViewSpec { kind: ViewKind::Rect, key: 0, props: Props::Rect(RectProps { bg: bg(1, 2, 3), w: None, h: None }) }
        )
        .is_ok(),
        "Refresh 父位收（容器白名单补漏）"
    );
}
