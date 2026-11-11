//! 验收①（GUI-3 §8）：RectI 换算饱和 / Σ×1.25 合并判定（双侧边界 +
//! 相交分形：并集为矩形恒可并、L 形角交受帽裁决）/ DamageSet K≤8 与
//! FULL 退化 / RectSet 不相交集语义 / Ui 侧 clip viewport 收口。
//!
//! 纪律：确定性（无随机无时间）、断言值级（不断言「非零」）、只用
//! 公共 API（§6 公开面）。

use berry_gfx::cmd::Color;
use berry_guiproto::Rect;
use berry_ui::damage::{
    DamageSet, RectI, RectSet, MERGE_WASTE_DEN, MERGE_WASTE_NUM, try_merge,
};
use berry_ui::desc::RectProps;
use berry_ui::{Props, UiMain, ViewKind, ViewSpec};

/// wire 矩形速写。
fn r(x: u32, y: u32, w: u32, h: u32) -> Rect {
    Rect { x, y, w, h }
}

// ---------- RectI：构造收口与坐标三套换算 ----------

#[test]
fn recti_clamps_conversions_and_geometry() {
    // 负 w/h 构造期折零（敌意/溢出中间量不带毒入集）
    let z = RectI::new(-5, -5, -3, 10);
    assert_eq!((z.x, z.y, z.w, z.h), (-5, -5, 0, 10));
    assert!(z.is_empty());

    // 半开区间包含
    let a = RectI::new(0, 0, 10, 10);
    assert!(a.contains(0, 0));
    assert!(a.contains(9, 9));
    assert!(!a.contains(10, 0)); // 右界开
    assert!(!a.contains(0, 10)); // 下界开

    // 交集（无交 → 零尺寸；i64 中间量不溢出）
    let d = a.intersect(&RectI::new(20, 0, 5, 5));
    assert!(d.is_empty());
    assert_eq!(d.w, 0);
    let i = a.intersect(&RectI::new(5, 5, 10, 10));
    assert_eq!((i.x, i.y, i.w, i.h), (5, 5, 5, 5));

    // 包络
    let e = a.envelope(&RectI::new(15, 5, 10, 10));
    assert_eq!((e.x, e.y, e.w, e.h), (0, 0, 25, 15));

    // 面积 u64：65536² = 2^32（u32 域放不下、u64 恰容纳）
    assert_eq!(RectI::new(0, 0, 65536, 65536).area(), 4_294_967_296);

    // i32 → wire：负坐标侧饱和 0（clip 前提下的保守折界），尺寸直转
    let w = RectI::new(-8, -4, 10, 6).to_wire();
    assert_eq!((w.x, w.y, w.w, w.h), (0, 0, 10, 6));
    let p = RectI::new(3, 4, 10, 6).to_wire();
    assert_eq!((p.x, p.y, p.w, p.h), (3, 4, 10, 6));

    // wire → i32：> i32::MAX 饱和到 i32::MAX
    let f = RectI::from_wire(&r(u32::MAX, 0, u32::MAX, u32::MAX));
    assert_eq!((f.x, f.y, f.w, f.h), (i32::MAX, 0, i32::MAX, i32::MAX));
}

// ---------- try_merge：Σ×1.25 浪费帽 ----------

#[test]
fn merge_waste_constants_single_source() {
    // Σ×1.25 = ×5/4（§6 冻结分子分母单源钉板）
    assert_eq!(MERGE_WASTE_NUM, 5);
    assert_eq!(MERGE_WASTE_DEN, 4);
}

#[test]
fn try_merge_exactly_125_merges_both_axes() {
    // 水平：env 25×10=250，Σ=200 → 250×4 == 200×5（恰 1.25）→ 可并
    let m = try_merge(&r(0, 0, 10, 10), &r(15, 0, 10, 10));
    assert_eq!(
        m,
        Some(Rect { x: 0, y: 0, w: 25, h: 10 }),
        "恰 1.25 倍（×4 == ×5 整数式）应允许合并"
    );
    // 垂直同式
    let mv = try_merge(&r(0, 0, 10, 10), &r(0, 15, 10, 10));
    assert_eq!(mv, Some(Rect { x: 0, y: 0, w: 10, h: 25 }));
}

#[test]
fn try_merge_one_px_past_rejects_both_axes() {
    // +1px：env 26×10=260 → 260×4=1040 > 1000 → 拒并
    assert_eq!(try_merge(&r(0, 0, 10, 10), &r(16, 0, 10, 10)), None);
    assert_eq!(try_merge(&r(0, 0, 10, 10), &r(0, 16, 10, 10)), None);
    // 双侧夹逼确认边界就是 15（水平向）
    assert!(try_merge(&r(0, 0, 10, 10), &r(14, 0, 10, 10)).is_some());
}

#[test]
fn try_merge_intersecting_cases_value_pinned() {
    // 相交可并面采样（§3.4 判定式 = 包络×4 ≤ Σ×5 为冻结规范）。
    // 「包络 ≤ Σ」强性质对 包含/十字/边贴 三形成立（值级钉板）——
    // 这三形并集即矩形（包络 = 并集），对相交对是充分不必要条件。
    let contain = (r(0, 0, 20, 20), r(5, 5, 2, 2));
    let cross = (r(0, 4, 16, 8), r(4, 0, 8, 16));
    let touch = (r(0, 0, 10, 10), r(10, 0, 10, 10));
    for (a, b) in [contain, cross, touch] {
        let env = try_merge(&a, &b).expect("相交/贴合可并");
        // 包络数学性质（本三形成立）：env_area ≤ Σ
        let env_area = env.w as u128 * env.h as u128;
        let sum = a.w as u128 * a.h as u128 + b.w as u128 * b.h as u128;
        assert!(env_area <= sum, "包含/十字/边贴的包络面积不得超过面积和");
        // 包络确实覆盖两者
        for s in [a, b] {
            assert!(env.x <= s.x && env.y <= s.y);
            assert!(env.x + env.w >= s.x + s.w && env.y + env.h >= s.y + s.h);
        }
    }
    // 值级：十字交包络 = (0,0,16,16)（恰 == Σ——零浪费）
    assert_eq!(
        try_merge(&r(0, 4, 16, 8), &r(4, 0, 8, 16)),
        Some(Rect { x: 0, y: 0, w: 16, h: 16 })
    );
    // 值级：边贴合包络 = 面积和（零浪费）
    assert_eq!(
        try_merge(&r(0, 0, 10, 10), &r(10, 0, 10, 10)),
        Some(Rect { x: 0, y: 0, w: 20, h: 10 })
    );
    // 值级：包含 → 大者即包络
    assert_eq!(
        try_merge(&r(0, 0, 20, 20), &r(5, 5, 2, 2)),
        Some(Rect { x: 0, y: 0, w: 20, h: 20 })
    );

    // 角相交（25px² 重叠）：包络 225 > Σ 200，但 225×4=900 ≤ 1000
    // → 判定式内仍可并（值级）
    assert_eq!(
        try_merge(&r(0, 0, 10, 10), &r(5, 5, 10, 10)),
        Some(Rect { x: 0, y: 0, w: 15, h: 15 })
    );

    // L 形薄条角点相交（重叠仅 1px²）：包络 10000×4 > 200×5 → 判定式
    // 拒并。注：design §3.4/§8「相交恒可并（包络 < Σ）」对该形不成立
    // （数学断言过强——角交/L 形反例在上）——冻结判定式为准；src 头
    // 注/try_merge 注已随审查改为分形陈述（文档 §3.4/§8 文字面待文档
    // 刀收口）。
    assert_eq!(try_merge(&r(0, 0, 100, 1), &r(0, 0, 1, 100)), None);
}

#[test]
fn try_merge_zero_area_none_and_huge_params_no_panic() {
    // 零面积不入合并代数（双侧防御）
    assert_eq!(try_merge(&r(0, 0, 0, 5), &r(0, 0, 4, 4)), None);
    assert_eq!(try_merge(&r(0, 0, 5, 0), &r(0, 0, 4, 4)), None);
    assert_eq!(try_merge(&r(0, 0, 4, 4), &r(0, 0, 0, 5)), None);

    // u32::MAX 量级巨参：u128 中间量不溢出不 panic
    let g = r(0, 0, u32::MAX, u32::MAX);
    let m = try_merge(&g, &g); // 同矩形：env==g，4E ≤ 10E 可并
    assert_eq!(m, Some(Rect { x: 0, y: 0, w: u32::MAX, h: u32::MAX }));

    // 全宽单行相邻行：env=2E == Σ×2 → 可并
    let row0 = r(0, 0, u32::MAX, 1);
    let row1 = r(0, 1, u32::MAX, 1);
    assert!(try_merge(&row0, &row1).is_some());

    // 全宽单行隔一行：env=4E > Σ×5/4×2 → 拒并
    let row3 = r(0, 3, u32::MAX, 1);
    assert_eq!(try_merge(&row0, &row3), None);
}

// ---------- DamageSet：K≤8 定容集 ----------

#[test]
fn damage_set_merges_in_add_and_drops_zero_area() {
    let mut d = DamageSet::new();
    assert!(d.is_empty());

    // 可并对（恰 1.25 不相交）在 add 内原位并——count 不涨
    d.add(r(0, 0, 10, 10));
    d.add(r(15, 0, 10, 10));
    assert_eq!(d.count, 1);
    assert_eq!(d.rects[0], Rect { x: 0, y: 0, w: 25, h: 10 });

    // 零面积入集即丢弃（count 不变）
    d.add(r(5, 5, 0, 10));
    d.add(r(5, 5, 10, 0));
    assert_eq!(d.count, 1);

    // 值级：union_px = Σ；envelope = 包络
    assert_eq!(d.union_px(), 250);
    assert_eq!(d.envelope(), Some(Rect { x: 0, y: 0, w: 25, h: 10 }));

    // clear 复位
    d.clear();
    assert!(d.is_empty());
    assert_eq!(d.envelope(), None);
    assert_eq!(d.union_px(), 0);
}

#[test]
fn damage_set_two_disjoint_kept_separate() {
    // 不可并对并存（count 2）
    let mut d = DamageSet::new();
    d.add(r(0, 0, 10, 10));
    d.add(r(30, 0, 5, 5)); // env 35×10=350，Σ=125 → 拒
    assert_eq!(d.count, 2);
    assert_eq!(d.union_px(), 125);
    assert_eq!(d.envelope(), Some(Rect { x: 0, y: 0, w: 35, h: 10 }));
}

/// 8 条两两不可并的 4×4 矩形（水平间距 20 → env 96 > Σ×5/4=160）。
fn eight_unmergeable() -> DamageSet {
    let mut d = DamageSet::new();
    for i in 0..8u32 {
        d.add(r(i * 20, 0, 4, 4));
    }
    assert_eq!(d.count, 8);
    d
}

#[test]
fn damage_set_ninth_unmergeable_degrades_full() {
    let mut d = eight_unmergeable();
    d.add(r(200, 0, 4, 4)); // 与所有既有均不可并（最近 x=140 → env 64 宽）
    // FULL 退化：矩形集清、full 位置、is_empty 语义 = 非空
    assert!(d.full);
    assert_eq!(d.count, 0);
    assert!(!d.is_empty());
    assert_eq!(d.envelope(), None);
    assert_eq!(d.union_px(), u64::MAX);
    assert_eq!(d.iter().count(), 0);

    // FULL 后 add no-op（矩形与 FULL 均不再变）
    d.add(r(0, 0, 5, 5));
    assert!(d.full);
    assert_eq!(d.count, 0);
    d.add_full(); // 幂等
    assert!(d.full);
}

#[test]
fn damage_set_ninth_mergeable_converges_at_cap() {
    // 第 9 条与既有可并 → 原位并，count 收敛 8、不 FULL
    let mut d = eight_unmergeable();
    d.add(r(6, 0, 4, 4)); // 与 rects[0]=(0,0,4,4)：env (0,0,10,4)=40，Σ=32 → 恰可并
    assert!(!d.full);
    assert_eq!(d.count, 8);
    // 并出的包络在集内（值级）
    assert!(d.iter().any(|x| *x == Rect { x: 0, y: 0, w: 10, h: 4 }));
    // 第 9 条本身不再单列
    assert!(!d.iter().any(|x| x.x == 6 && x.w == 4));
}

#[test]
fn damage_set_cap_pairwise_merge_path() {
    // 定向触发 add 流程③：count==8 且新条与所有既有不可并、但集内
    // 存在可并对 → 两两并出一槽再挂。构造：
    //   A=(0,0,4,10)；B=(12,0,4,10)（与 A 不可并：env 160 > 80×5/4）
    //   C=(6,0,4,10) 与 A、B 均恰可并（env (·,0,10,10)=100 == 80×5/4）
    //   → add 选包络面积并列后者（area_le ≤ 取后见）——并入 B 得
    //   M=(6,0,10,10)；(A,M) 对 env (0,0,16,10)=160 ≤ Σ140×5/4=175 可并。
    let mut d = DamageSet::new();
    d.add(r(0, 0, 4, 10)); // A
    d.add(r(12, 0, 4, 10)); // B
    d.add(r(6, 0, 4, 10)); // C → 与 B 并成 M=(6,0,10,10)
    assert_eq!(d.count, 2);
    assert!(d.iter().any(|x| *x == Rect { x: 6, y: 0, w: 10, h: 10 }));

    // 补 6 条远矩形到 8（两两与谁都不可并）
    for i in 0..6u32 {
        d.add(r(40 + i * 20, 0, 4, 10));
    }
    assert_eq!(d.count, 8);

    // 第 9 条与所有既有不可并 → 走流程③：(A,M) 并出一槽
    d.add(r(200, 0, 4, 10));
    assert!(!d.full, "集内存在可并对 → 不应 FULL");
    assert_eq!(d.count, 8);
    // 并出的 (A,M) 包络与新条均在集内（值级）
    assert!(d.iter().any(|x| *x == Rect { x: 0, y: 0, w: 16, h: 10 }));
    assert!(d.iter().any(|x| *x == Rect { x: 200, y: 0, w: 4, h: 10 }));
    // 全集包络收敛（A..r 右界 = 204）
    assert_eq!(d.envelope(), Some(Rect { x: 0, y: 0, w: 204, h: 10 }));
}

// ---------- RectSet：遮挡剔除几何底座 ----------

#[test]
fn rectset_subtract_strips_grid() {
    let mut rs: RectSet<8> = RectSet::new();
    rs.add(RectI::new(0, 0, 10, 10));
    rs.subtract(RectI::new(3, 3, 4, 4)); // 中心挖洞 → 4 条带

    assert_eq!(rs.len(), 4); // 上/下/左/右各一片
    assert_eq!(rs.area(), 100 - 16); // 84——洞被完整扣除

    // 覆盖判定（is_covered = r ⊆ 集）：洞区已挖 → 未盖；顶带完整在集
    assert!(!rs.is_covered(&RectI::new(3, 3, 4, 4)));
    assert!(rs.is_covered(&RectI::new(0, 0, 10, 3)));
    assert!(!rs.is_covered(&RectI::new(0, 0, 10, 10))); // 整体有洞
    // 可见性：洞 = 整体唯一未被盖区域——visible(whole) 收敛到洞矩形
    assert_eq!(rs.visible(&RectI::new(0, 0, 10, 10)), RectI::new(3, 3, 4, 4));

    // 全减 → 空
    rs.subtract(RectI::new(0, 0, 10, 10));
    assert!(rs.is_empty());
    assert_eq!(rs.area(), 0);
}

#[test]
fn rectset_covered_and_visible_pieces() {
    // 两片恰覆盖整矩形
    let mut rs: RectSet<8> = RectSet::new();
    assert!(rs.add(RectI::new(0, 0, 5, 10)));
    assert!(rs.add(RectI::new(5, 0, 5, 10)));
    assert!(rs.is_covered(&RectI::new(0, 0, 10, 10)));
    assert_eq!(rs.area(), 100);
    assert_eq!(rs.len(), 2);

    // 顶条带遮挡 → 可见最大片 = 下带
    let mut rs2: RectSet<8> = RectSet::new();
    rs2.add(RectI::new(0, 0, 10, 3));
    assert!(rs2.is_covered(&RectI::new(0, 0, 10, 3)));
    assert!(!rs2.is_covered(&RectI::new(0, 0, 10, 10)));
    assert_eq!(rs2.visible(&RectI::new(0, 0, 10, 10)), RectI::new(0, 3, 10, 7));

    // 零面积恒覆盖
    assert!(rs2.is_covered(&RectI::new(4, 4, 0, 0)));
    assert_eq!(rs2.visible(&RectI::new(4, 4, 0, 0)), RectI::new(0, 0, 0, 0));
}

#[test]
fn rectset_cap_and_overlap_disjointify() {
    // 容量帽：满帽拒入 → 保守少记覆盖（多画不少画）
    let mut rs: RectSet<2> = RectSet::new();
    assert!(rs.add(RectI::new(0, 0, 10, 10)));
    assert!(rs.add(RectI::new(20, 0, 10, 10)));
    assert!(!rs.add(RectI::new(40, 0, 5, 5))); // 满帽拒入
    assert_eq!(rs.len(), 2);
    assert!(!rs.is_covered(&RectI::new(40, 0, 5, 5))); // 覆盖丢失（保守方向）

    // 入集自动不相交化：重叠 add 先减后入——面积守恒无双计
    let mut ro: RectSet<8> = RectSet::new();
    assert!(ro.add(RectI::new(0, 0, 10, 10)));
    assert!(ro.add(RectI::new(5, 0, 10, 10))); // 覆盖右半
    assert_eq!(ro.len(), 2);
    assert_eq!(ro.area(), 150); // 5×10 + 10×10——重叠区只计一次
    assert!(ro.is_covered(&RectI::new(5, 0, 10, 10)));
    assert!(ro.is_covered(&RectI::new(0, 0, 10, 10)));

    // 零面积 add 恒 true（无覆盖贡献）
    assert!(ro.add(RectI::new(0, 0, 0, 5)));
    assert_eq!(ro.len(), 2);
}

// ---------- Ui 侧 clip viewport 收口 ----------

#[test]
fn ui_damage_clipped_to_viewport() {
    let mut ui = UiMain::new(64, 48);
    let root = ui.root();
    // 固定宽 100 > viewport 64 的 Rect——视觉界越界
    let wide = ui
        .add(
            root,
            ViewSpec {
                kind: ViewKind::Rect,
                key: 0,
                props: Props::Rect(RectProps {
                    bg: Color { r: 1, g: 2, b: 3 },
                    w: Some(100),
                    h: Some(16),
                }),
            },
        )
        .unwrap();
    let stat = ui.layout();
    assert_eq!((stat.measured, stat.placed), (2, 2)); // 根 + 新叶各一次

    // bounds = 视觉界（未剪）
    assert_eq!(ui.bounds(wide).unwrap(), RectI::new(0, 0, 100, 16));

    // damage 全部 clip 到 viewport：根界与新叶界（越界者折界内）相交
    // 合并成单一 viewport 矩形——值级钉板（绝无 w>64 的矩形入集）
    let d = ui.damage();
    assert!(!d.full);
    assert_eq!(d.count, 1);
    assert_eq!(d.rects[0], Rect { x: 0, y: 0, w: 64, h: 48 });
    for x in d.iter() {
        assert!(x.w <= 64 && x.h <= 48, "damage 矩形必须在 viewport 内");
    }

    // 负坐标侧：to_wire 负侧饱和 0（damage 入集前 clip 义务的换算面）
    let neg = RectI::new(-8, -4, 10, 6).to_wire();
    assert_eq!((neg.x, neg.y), (0, 0));
}
