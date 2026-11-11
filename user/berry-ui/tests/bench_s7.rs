//! GUI-6 段E S7 钉板 + 基准（`docs/GUI-6实施设计草案.md` §10/§11 项
//! 5/6/7/17 + §12 段E 验收③——ADR #60 钉板先行）。
//!
//! - 双态组织（§11 项 17）：同一钉板内 cfg 分面——默认构建跑 A 面
//!   断言（三漏斗现状 dirty 语义逐字保持——诚实基线），`--features
//!   s7` 跑 B 面断言（ChangeMask 属性级短路）。两面同文件编写、各自
//!   构建面各自绿；mask/props_diff 纯 B 面单测整体 cfg 挂 s7。
//! - 项 5 等长换文（len 与 scale 均同、内容异）：B 面 measured==0
//!   （纯 PAINT 短路）+ damage 含本界；A 面 measured≥1（现状全链
//!   relayout 浪费保留）。
//! - 项 6 钦定钉板：孙 GEOM 变（w 20→30）、中间聚合内在吸收不变
//!   （max(40,30)=40）→ B 面重测恰 = 孙 + 中间聚合（顶层截断）；
//!   A 面沿链预标脏三节点全测。两面 bounds 同值（正确性不随面漂）。
//! - 项 7 同文（字节相等）：B 面零操作直返（damage 空 + measured==0）；
//!   A 面锚 = tests/layout.rs `a_face_same_text_still_full_relayout`
//!   （本文件复钉 A 侧面 damage 非空——现状浪费可见化）。
//! - mask 契约（§6 冻结）：位面常量/GEOM/PAINT 分组/三谓词 + §5
//!   props_diff 字段位映射表（§11 项 4 B 侧）。
//! - 基准（§11 项 18）：标准样例树 ×N 帧 PAINT-only churn——确定性
//!   计数（measured/damage adds）为主门（跨机可比）；墙钟 Instant
//!   仅参考读数不断言（机间漂移——bench-icount 同纪律）。
//!
//! 确定性：全定值驱动，无随机无真钟；术语 §11 头注（同文/等长换文/
//! 变长）。

use berry_gfx::cmd::Color;
use berry_gfx::font::FontSize;
use berry_ui::damage::RectI;
use berry_ui::desc::{
    ColProps, Props, RectProps, TextProps, TextStyle, ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::{NodeId, Ui};

fn bg(r: u8, g: u8, b: u8) -> Color {
    Color { r, g, b }
}

fn rect(w: Option<u32>, h: Option<u32>) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Rect,
        key: 0,
        props: Props::Rect(RectProps { bg: bg(1, 2, 3), w, h }),
    }
}

fn text(bytes: &[u8], scale: FontSize) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg: bg(9, 9, 9), bg: None, scale },
            text: berry_ui::TextBuf::new(bytes).unwrap(),
        }),
    }
}

fn col(gap: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Col,
        key: 0,
        props: Props::Col(ColProps { gap, w: None, h: None, cross: CrossAlign::Stretch }),
    }
}

// ---------- mask 契约（§6 冻结——纯 B 面） ----------

/// 位面常量 + GEOM/PAINT 分组（§6 表逐值）+ 三谓词语义。
#[cfg(feature = "s7")]
#[test]
fn mask_plane_contract_frozen() {
    use berry_ui::mask::ChangeMask as M;
    // 位面（§6 表）：W=1<<0 … CROSS=1<<7
    assert_eq!(M::W.0, 1 << 0);
    assert_eq!(M::H.0, 1 << 1);
    assert_eq!(M::GAP.0, 1 << 2);
    assert_eq!(M::BG.0, 1 << 3);
    assert_eq!(M::FG.0, 1 << 4);
    assert_eq!(M::TEXT.0, 1 << 5);
    assert_eq!(M::SCALE.0, 1 << 6);
    assert_eq!(M::CROSS.0, 1 << 7);
    // 分组：GEOM = W|H|GAP|SCALE|CROSS = 0b1100_0111；PAINT = BG|FG|TEXT
    // = 0b0011_1000（草案 §6.5 代码块内字面量 0b1110_0111 = 231 含
    // bit5=TEXT，与 §6.2 D2 冻结表「GEOM=W|H|GAP|SCALE|CROSS」及
    // 分组不叠自相矛盾——以冻结表为准，字面量为草案笔误）。
    assert_eq!(M::GEOM.0, 0b1100_0111);
    assert_eq!(M::GEOM, M::W.union(M::H).union(M::GAP).union(M::SCALE).union(M::CROSS));
    assert_eq!(M::PAINT.0, 0b0011_1000);
    assert_eq!(M::PAINT, M::BG.union(M::FG).union(M::TEXT));
    // GEOM ∩ PAINT = 空（位面不重叠）
    assert!(!M::GEOM.intersects(M::PAINT));
    // 三谓词
    assert!(M::NONE.is_empty());
    assert!(!M::W.is_empty());
    assert!(M::W.intersects(M::GEOM));
    assert!(!M::W.intersects(M::PAINT));
    assert_eq!(M::W.union(M::BG), berry_ui::mask::ChangeMask(0b0000_1001));
}

/// §5 props_diff 字段位映射表（§11 项 4 B 侧）：字段差 → 位集。
#[cfg(feature = "s7")]
#[test]
fn props_diff_bit_mapping() {
    use berry_ui::desc::{
        GridProps, IconProps, ImageProps, PadProps, RowProps,
    };
    use berry_ui::mask::{props_diff, ChangeMask as M};

    // Rect：w/h→W/H（GEOM）；bg→BG（PAINT）
    let r = |w, h, c| Props::Rect(RectProps { bg: bg(c, c, c), w, h });
    assert_eq!(props_diff(&r(Some(1), None, 1), &r(Some(2), None, 1)), M::W);
    assert_eq!(props_diff(&r(None, Some(1), 1), &r(None, Some(2), 1)), M::H);
    assert_eq!(props_diff(&r(Some(1), Some(1), 1), &r(Some(1), Some(1), 2)), M::BG);
    assert_eq!(props_diff(&r(Some(1), Some(1), 1), &r(Some(1), Some(1), 1)), M::NONE);

    // Text：等长换文→TEXT；变长→TEXT|W|H（D3）；scale→SCALE；fg/bg
    let t = |s: &str, scale| {
        Props::Text(TextProps {
            style: TextStyle { fg: bg(1, 1, 1), bg: None, scale },
            text: berry_ui::TextBuf::new(s.as_bytes()).unwrap(),
        })
    };
    assert_eq!(props_diff(&t("hi", FontSize::S8x16), &t("oy", FontSize::S8x16)), M::TEXT);
    assert_eq!(
        props_diff(&t("hi", FontSize::S8x16), &t("hey", FontSize::S8x16)),
        M::TEXT.union(M::W).union(M::H)
    );
    assert_eq!(
        props_diff(&t("hi", FontSize::S8x16), &t("hi", FontSize::S16x32)),
        M::SCALE
    );
    assert_eq!(props_diff(&t("hi", FontSize::S8x16), &t("hi", FontSize::S8x16)), M::NONE);

    // Col/Row：gap→GAP；cross→CROSS；w/h→W/H
    let c = |gap, w, h, cr| Props::Col(ColProps { gap, w, h, cross: cr });
    assert_eq!(props_diff(&c(0, None, None, CrossAlign::Stretch), &c(4, None, None, CrossAlign::Stretch)), M::GAP);
    assert_eq!(
        props_diff(&c(0, None, None, CrossAlign::Stretch), &c(0, None, None, CrossAlign::Start)),
        M::CROSS
    );
    assert_eq!(props_diff(&c(0, None, None, CrossAlign::Stretch), &c(0, Some(9), None, CrossAlign::Stretch)), M::W);
    let rr = |gap, w, h, cr| Props::Row(RowProps { gap, w, h, cross: cr });
    assert_eq!(props_diff(&rr(0, None, None, CrossAlign::Stretch), &rr(1, None, None, CrossAlign::Stretch)), M::GAP);

    // Pad：all→GAP（兼任）
    assert_eq!(
        props_diff(&Props::Pad(PadProps { all: 0 }), &Props::Pad(PadProps { all: 4 })),
        M::GAP
    );

    // Grid：cols/rows→W|H|GAP 保守；gap→GAP；cross→CROSS
    let g = |cols, rows, gap, cr| {
        Props::Grid(GridProps { cols, rows, gap, cross: cr, w: None, h: None })
    };
    assert_eq!(
        props_diff(&g(2, 2, 0, CrossAlign::Stretch), &g(3, 2, 0, CrossAlign::Stretch)),
        M::W.union(M::H).union(M::GAP)
    );
    assert_eq!(
        props_diff(&g(2, 2, 0, CrossAlign::Stretch), &g(2, 2, 8, CrossAlign::Stretch)),
        M::GAP
    );

    // Image：res→W|H 保守；Icon：id→W|H 保守、fg→FG
    assert_eq!(
        props_diff(&Props::Image(ImageProps { res: 1, w: None, h: None }), &Props::Image(ImageProps { res: 2, w: None, h: None })),
        M::W.union(M::H)
    );
    assert_eq!(
        props_diff(&Props::Icon(IconProps { id: 1, fg: bg(1, 1, 1), w: None, h: None }), &Props::Icon(IconProps { id: 2, fg: bg(1, 1, 1), w: None, h: None })),
        M::W.union(M::H)
    );
    assert_eq!(
        props_diff(&Props::Icon(IconProps { id: 1, fg: bg(1, 1, 1), w: None, h: None }), &Props::Icon(IconProps { id: 1, fg: bg(2, 1, 1), w: None, h: None })),
        M::FG
    );

    // Root 恒空（根 props 无字段面）
    assert_eq!(props_diff(&Props::Root, &Props::Root), M::NONE);
}

/// staging 帽与动画槽帽同源（§6/D1：ANIM_STAGING_MAX = ANIM_MAX——
/// 单 tick 应用数 ≤ 槽数 → 结构性不触帽）。
#[cfg(feature = "s7")]
#[test]
fn anim_staging_cap_same_source_as_anim_slots() {
    assert_eq!(berry_ui::ANIM_STAGING_MAX, berry_ui::ANIM_MAX);
}

// ---------- 三漏斗钉板（§11 项 5/6/7——双态） ----------

/// 项 5 等长换文：len 与 scale 均同、内容异（"hi"→"oy"）。B 面 =
/// TEXT|BG 纯 PAINT：零重测 + damage 含本界；A 面 = 现状全链
/// relayout（measured≥1 诚实基线）。两面 bounds 同值。
#[test]
fn equal_len_swap_paint_shortcut_or_full_relayout() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let c = ui.add(root, col(4)).unwrap();
    let t = ui.add(c, text(b"hi", FontSize::S8x16)).unwrap();
    ui.layout();
    ui.drain_damage();
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 16, 16), "前置：初始摆位");

    ui.set_text(t, b"oy").unwrap(); // 等长换文（§11 术语）
    let stat = ui.layout();
    let d = ui.drain_damage();

    #[cfg(feature = "s7")]
    {
        assert_eq!(stat.measured, 0, "B 面等长换文 = 纯 PAINT——属性级短路零重测");
        assert_eq!(d.count, 1, "damage 恰本界一条");
        assert_eq!(d.rects[0], berry_guiproto::Rect { x: 0, y: 0, w: 16, h: 16 }, "damage 含本界");
    }
    #[cfg(not(feature = "s7"))]
    {
        assert!(stat.measured >= 1, "A 面等长换文仍全量 relayout（现状基线——含浪费面）");
        assert!(d.count >= 1, "A 面旧界+父界照记");
    }
    // 两面正确性同值：等长换文不挪位
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 16, 16));
}

/// 项 6 钦定钉板：Root → ColMid(中间聚合) → {wide(40,10), tgt(20,10)}；
/// tgt set_props w 20→30（GEOM 位）：tgt 内在变，ColMid 内在宽
/// max(40,30)=40 吸收不变 → B 面 D4 截断：重测恰 tgt + ColMid 两节点
/// （顶层 Root 截断）；A 面沿链预标脏三节点全测。两面 bounds 同值
/// （新几何照常生效——截断是优化不是语义）。
#[test]
fn absorbed_geom_change_truncates_before_top() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let mid = ui.add(root, col(0)).unwrap();
    let wide = ui.add(mid, rect(Some(40), Some(10))).unwrap();
    let tgt = ui.add(mid, rect(Some(20), Some(10))).unwrap();
    ui.layout();
    ui.drain_damage();

    ui.set_props(tgt, Props::Rect(RectProps { bg: bg(1, 2, 3), w: Some(30), h: Some(10) }))
        .unwrap(); // w 20→30 = W 位（GEOM）
    let stat = ui.layout();
    let d = ui.drain_damage();

    #[cfg(feature = "s7")]
    {
        assert_eq!(stat.measured, 2, "B 面：tgt 重测 + ColMid 聚合重测；Root 截断（内在 vp 不受影响）");
    }
    #[cfg(not(feature = "s7"))]
    {
        assert_eq!(stat.measured, 3, "A 面沿链预标脏：tgt + ColMid + Root 全测（现状基线）");
    }
    assert!(d.count >= 1, "GEOM 路旧界+父链 damage 入账（两面同）");
    // 两面同值：新几何生效、兄弟与聚合不漂
    assert_eq!(ui.bounds(tgt).unwrap(), RectI::new(0, 10, 30, 10), "tgt 新几何");
    assert_eq!(ui.bounds(wide).unwrap(), RectI::new(0, 0, 40, 10), "wide 原位");
    assert_eq!(ui.bounds(mid).unwrap(), RectI::new(0, 0, 100, 20), "ColMid 聚合界（40 吸收）");
}

/// 项 7 同文（字节相等）：B 面 D3 三态之首 = 零操作直返（零 damage
/// 零重测）；A 面锚 = tests/layout.rs a_face_same_text（本文件钉 A 侧
/// damage 非空——现状浪费可见化，防止段 A 基座悄悄吞掉浪费面）。
#[test]
fn same_text_zero_op_or_current_waste() {
    let mut ui = Ui::<16>::new(100, 200);
    let root = ui.root();
    let c = ui.add(root, col(4)).unwrap();
    let t = ui.add(c, text(b"hi", FontSize::S8x16)).unwrap();
    ui.layout();
    ui.drain_damage();

    ui.set_text(t, b"hi").unwrap(); // 同文（字节相等）
    let stat = ui.layout();

    #[cfg(feature = "s7")]
    {
        assert!(ui.damage().is_empty(), "B 面同文 = 零操作直返（零 damage）");
        assert_eq!(stat.measured, 0, "B 面同文零重测");
    }
    #[cfg(not(feature = "s7"))]
    {
        assert!(!ui.damage().is_empty(), "A 面同文照旧界+父界入账（现状浪费——诚实基线）");
        assert!(stat.measured >= 1, "A 面同文仍全量 relayout");
    }
    assert_eq!(ui.bounds(t).unwrap(), RectI::new(0, 0, 16, 16), "两面 bounds 不变");
}

/// D1 staging 接线钉板（B 面）：Layout 型动画经 staging → render_frame
/// 统一 apply_change（时序 = anim apply 后、relayout 前）→ 新几何当帧
/// 生效；A 面现状直写路径同测同值（两面 bounds 同锚——既有
/// width_layout_anim_consumed_by_relayout 的双态复钉）。
#[test]
fn layout_anim_staging_or_direct_write_same_bounds() {
    use berry_ui::anim::{Anim, AnimProp, Easing};
    let mut ui = Ui::<16>::new(100, 100);
    let root = ui.root();
    let a = ui.add(root, rect(Some(10), Some(10))).unwrap();
    ui.layout();
    ui.drain_damage();
    ui.anim_add(Anim {
        node: a,
        prop: AnimProp::Width,
        from: 10,
        to: 30,
        start_ns: 1000,
        dur_ns: 10_000,
        easing: Easing::Linear,
    })
    .unwrap();

    let mut scratch = [berry_gfx::cmd::GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    let out = {
        let mut enc = berry_gfx::cmd::CommandEncoder::new();
        ui.render_frame(6000, &mut scratch, &mut enc) // 中点 v=20
    };
    assert!(out.drew, "动画帧有 damage 提交（两面同）");
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 20, 10), "中点 w=20 当帧生效（两面同值）");
    let out2 = {
        let mut enc = berry_gfx::cmd::CommandEncoder::new();
        ui.render_frame(11_000, &mut scratch, &mut enc) // 终点 v=30
    };
    assert!(out2.drew);
    assert_eq!(ui.bounds(a).unwrap(), RectI::new(0, 0, 30, 10), "终点精确 to");
    assert!(!ui.anim_active(), "完成即移除");
}

// ---------- 基准（§11 项 18——双态） ----------

/// A 面参照（默认构建本测断言钉住；B 面对拍入册——确定性计数非墙
/// 钟，跨机可比）：每帧 PAINT-only churn（等长换文 + 改色）A 面重测
/// = 文 + Col + Root = 3。
const A_FACE_PAINT_CHURN_MEASURED: u32 = 3;
/// B 面目标：属性级短路 → 0 重测/帧。
const B_FACE_PAINT_CHURN_MEASURED: u32 = 0;
/// 帧数（N 帧 × 2 次 set_*/帧——SDK-02 属性级 churn 压力量级）。
const BENCH_FRAMES: usize = 64;
/// 大树轴帧数（R3-3 理想修：≥100 节点 × ≥1000 帧——脱离冒烟量级）。
const BENCH_LARGE_FRAMES: usize = 1024;
/// 大树轴子数（含 Col+Text 总节点 102 ≥ 100）。
const BENCH_LARGE_RECTS: usize = 100;

/// kill criteria 判定面（§10/R3-3 最小修）：单编译面无法自测对侧墙钟
/// ——A 面打印基线采集指引，B 面读 env 基线（A 面跑 --nocapture 记
/// µs/帧后 export）算 B/A 比值 + 「≥20%/＜20%」判定文案。读数**参考
/// 级**（机间不可比——bench-icount 同纪律）；kill criteria 终判 venue
/// = GUI-8 embedder 双打点 + 更大量级树（§10 修订行随刀入册）。
fn kill_verdict_print(axis: &str, env_key: &str, us_per_frame: u128) {
    #[cfg(feature = "s7")]
    {
        match std::env::var(env_key) {
            Ok(v) => match v.trim().parse::<u128>() {
                Ok(a_us) if a_us > 0 && us_per_frame > 0 => {
                    // B 面帧耗时收益 % = (A − B)/A（≥20% → 转正线；<20% → kill）
                    let gain_x100 = a_us.saturating_sub(us_per_frame) * 100 / a_us;
                    let verdict = if gain_x100 >= 20 { "≥20%（转正线达）" } else { "<20%（kill 线）" };
                    println!(
                        "S7 verdict（{axis}）：A={a_us}µs/帧 B={us_per_frame}µs/帧 → 帧耗时收益 {gain_x100}% {verdict}（参考级读数——终判 venue = GUI-8 embedder 双打点）"
                    );
                }
                _ => println!("S7 verdict（{axis}）：env {env_key} 非正值「{v}」——拒判（loud 不猜）"),
            },
            Err(_) => println!(
                "S7 verdict（{axis}）：未提供 A 面基线（先跑 A 面 --nocapture 记 µs/帧，export {env_key}=<µs/帧> 再跑 B 面）"
            ),
        }
    }
    #[cfg(not(feature = "s7"))]
    {
        println!("S7 基线采集（{axis}）：A 面 {us_per_frame}µs/帧——B 面跑前 export {env_key}={us_per_frame}");
    }
}

/// 标准样例树：Root(100×200) → Col{t1 文 "aa", r(20,10), t2 文 "zz"}。
/// 返回 (col, t1, r)——PAINT-only churn 双漏斗锚。
fn sample_tree(ui: &mut Ui<16>) -> (NodeId, NodeId, NodeId) {
    let root = ui.root();
    let c = ui.add(root, col(0)).unwrap();
    let t1 = ui.add(c, text(b"aa", FontSize::S8x16)).unwrap();
    let r = ui.add(c, rect(Some(20), Some(10))).unwrap();
    let _t2 = ui.add(c, text(b"zz", FontSize::S8x16)).unwrap();
    ui.layout();
    ui.drain_damage();
    (c, t1, r)
}

/// PAINT-only churn 基准（双态同跑）：layout 轴确定性计数为主门；
/// 墙钟 Instant 仅打印参考（--nocapture 观测——机间不可比）。
#[test]
fn bench_paint_only_churn() {
    let mut ui = Ui::<16>::new(100, 200);
    let (_col_id, t1, r) = sample_tree(&mut ui);

    let t0 = std::time::Instant::now();
    let mut total_measured = 0u32;
    for k in 0..BENCH_FRAMES {
        // 等长换文（aa↔bb）+ 改色（PAINT-only 双漏斗——SDK-02 量级面）
        ui.set_text(t1, if k % 2 == 0 { b"bb" } else { b"aa" }).unwrap();
        ui.set_props(
            r,
            Props::Rect(RectProps { bg: bg((k % 2) as u8, 2, 3), w: Some(20), h: Some(10) }),
        )
        .unwrap();
        let stat = ui.layout();
        total_measured += stat.measured;
        ui.drain_damage();
    }
    let dt = t0.elapsed();
    let us_per_frame = dt.as_micros() / BENCH_FRAMES as u128;

    #[cfg(feature = "s7")]
    {
        assert_eq!(
            total_measured / BENCH_FRAMES as u32,
            B_FACE_PAINT_CHURN_MEASURED,
            "B 面 PAINT-only churn = 0 重测/帧（属性级短路）"
        );
        println!(
            "S7 bench（B 面）：{} 帧 PAINT-only churn，重测 {}/帧（A 面参照 {}/帧），layout 轴墙钟 {:?}",
            BENCH_FRAMES,
            total_measured / BENCH_FRAMES as u32,
            A_FACE_PAINT_CHURN_MEASURED,
            dt
        );
    }
    #[cfg(not(feature = "s7"))]
    {
        assert_eq!(
            total_measured / BENCH_FRAMES as u32,
            A_FACE_PAINT_CHURN_MEASURED,
            "A 面参照自钉：等长换文仍全链重测（现状基线——入册参照）"
        );
        println!(
            "S7 bench（A 面）：{} 帧 PAINT-only churn，重测 {}/帧（B 面目标 {}/帧），layout 轴墙钟 {:?}",
            BENCH_FRAMES,
            total_measured / BENCH_FRAMES as u32,
            B_FACE_PAINT_CHURN_MEASURED,
            dt
        );
    }
    kill_verdict_print("PAINT churn", "BERRY_UI_S7_A_PAINT_US", us_per_frame);
}

/// 帧管线轴（双态同跑）：render_frame 驱动同 workload——damage 合并
/// 面（adds 入集数 vs count 存活数）两面各自确定性。
#[test]
fn bench_frame_pipeline_damage_adds() {
    let mut ui = Ui::<16>::new(100, 200);
    let (_col_id, t1, r) = sample_tree(&mut ui);

    let mut scratch = [berry_gfx::cmd::GlyphPos { ch: 0, x: 0, y: 0 }; 64];
    let mut now = 0u64;
    let mut adds_total = 0u32;
    let mut rects_total = 0u32;
    for k in 0..BENCH_FRAMES {
        ui.set_text(t1, if k % 2 == 0 { b"bb" } else { b"aa" }).unwrap();
        ui.set_props(
            r,
            Props::Rect(RectProps { bg: bg((k % 2) as u8, 2, 3), w: Some(20), h: Some(10) }),
        )
        .unwrap();
        now += 1_000_000;
        let out = {
            let mut enc = berry_gfx::cmd::CommandEncoder::new();
            ui.render_frame(now, &mut scratch, &mut enc)
        };
        assert!(out.drew, "churn 帧恒有 damage 提交");
        adds_total += out.damage.adds as u32;
        rects_total += out.damage.count as u32;
    }
    #[cfg(feature = "s7")]
    {
        // B 面：等长换文 = 本界 1 add + 改色 = 本界 1 add → 恰 2 add/帧；
        // 两界边贴（文底 y=16 = r 顶 y=16）→ damage.rs「边贴恒可并」并成
        // 单条 (0,0,20,26)/帧——合并后存活 1 条/帧（零全链泛洪）。
        assert_eq!(adds_total as usize, 2 * BENCH_FRAMES, "B 面 adds = 两漏斗各恰本界一次/帧");
        assert_eq!(rects_total as usize, BENCH_FRAMES, "B 面合并后恰 1 条/帧（边贴并集）");
    }
    #[cfg(not(feature = "s7"))]
    {
        // A 面：set_text 旧界 + 父界 2 add + set_props 旧界 1 add + relayout
        // place 沿途补面——全链入账（参照自钉默认构建实测；B 面对拍口径）。
        assert!(adds_total >= 3 * BENCH_FRAMES as u32, "A 面 set_text/set_props 全量入账基线");
        println!("S7 bench（A 面管线）：{} 帧 adds={} rects={}", BENCH_FRAMES, adds_total, rects_total);
    }
    #[cfg(feature = "s7")]
    println!("S7 bench（B 面管线）：{} 帧 adds={} rects={}", BENCH_FRAMES, adds_total, rects_total);
}

/// 大树 churn 轴（R3-3 理想修——≥100 节点 × ≥1000 帧，脱离段E 自认的
/// 「冒烟量级不作终判依据」面）：Root → Col{Text "aa", 100×Rect(20,10)}；
/// 每帧等长换文 + 轮转改色（PAINT-only 双漏斗）。确定性计数主门：
/// A 面 3/帧（文+Col+Root——bg-only set_props 不重测）、B 面 0/帧
/// （属性级短路）；墙钟 + env 基线比值 = 参考级（判定文案同上单源）。
#[test]
fn bench_churn_large_tree() {
    let mut ui = Ui::<128>::new(100, 800);
    let root = ui.root();
    let c = ui.add(root, col(0)).unwrap();
    let t = ui.add(c, text(b"aa", FontSize::S8x16)).unwrap();
    let mut rects: Vec<NodeId> = Vec::with_capacity(BENCH_LARGE_RECTS);
    for _ in 0..BENCH_LARGE_RECTS {
        rects.push(ui.add(c, rect(Some(20), Some(10))).unwrap());
    }
    ui.layout();
    ui.drain_damage();

    let t0 = std::time::Instant::now();
    let mut total_measured = 0u32;
    for k in 0..BENCH_LARGE_FRAMES {
        ui.set_text(t, if k % 2 == 0 { b"bb" } else { b"aa" }).unwrap();
        let target = rects[k % BENCH_LARGE_RECTS];
        ui.set_props(
            target,
            Props::Rect(RectProps { bg: bg((k % 2) as u8, 2, 3), w: Some(20), h: Some(10) }),
        )
        .unwrap();
        let stat = ui.layout();
        total_measured += stat.measured;
        ui.drain_damage();
    }
    let dt = t0.elapsed();
    let us_per_frame = dt.as_micros() / BENCH_LARGE_FRAMES as u128;

    #[cfg(feature = "s7")]
    {
        assert_eq!(
            total_measured / BENCH_LARGE_FRAMES as u32,
            B_FACE_PAINT_CHURN_MEASURED,
            "B 面大树 PAINT-only churn = 0 重测/帧（属性级短路不随树规模放大）"
        );
        println!(
            "S7 bench（B 面大树）：{} 节点 × {} 帧 churn，重测 {}/帧，layout 轴墙钟 {:?}",
            BENCH_LARGE_RECTS + 3,
            BENCH_LARGE_FRAMES,
            total_measured / BENCH_LARGE_FRAMES as u32,
            dt
        );
    }
    #[cfg(not(feature = "s7"))]
    {
        assert_eq!(
            total_measured / BENCH_LARGE_FRAMES as u32,
            A_FACE_PAINT_CHURN_MEASURED,
            "A 面大树参照：等长换文全链重测 3/帧（文+Col+Root）"
        );
        println!(
            "S7 bench（A 面大树）：{} 节点 × {} 帧 churn，重测 {}/帧，layout 轴墙钟 {:?}",
            BENCH_LARGE_RECTS + 3,
            BENCH_LARGE_FRAMES,
            total_measured / BENCH_LARGE_FRAMES as u32,
            dt
        );
    }
    kill_verdict_print("大树 churn", "BERRY_UI_S7_A_LARGE_US", us_per_frame);
}
