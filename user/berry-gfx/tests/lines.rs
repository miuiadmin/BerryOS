//! 线段守卫组（GUI-2 段①审查 R3-1/R2-F1 收口）：`DrawLine` 像素语义
//! 三层钉死——
//!
//! 1. **手算字面像素集**：非平凡斜率（两端都不在轴上）的期望像素 =
//!    手工逐步推演 err 状态机的字面坐标表。tie 咬合点两例专列
//!    （`e2 == dy` 咬 `>=` / `e2 == dx` 咬 `<=`）——恰等步的取舍是
//!    本 Bresenham 变体的**身份指纹**，变体漂移（`>=` 改 `>`、`<=`
//!    改 `<`）此组必炸（原套件 mutation 实证漏网处，R3-1）；
//! 2. **等价性语料**：src `line()` 的带窗实现（主轴步窗反解 + 闭式
//!    状态重构，R2-F1）↔ 本文件内**逐字复刻的原算法**参考，7^4=2401
//!    段网格端点（含帧外端点）×（整帧 / dirty 窗）对拍——重构错一
//!    步（窗解/折算/残差任一处）即炸；
//! 3. **敌意巨跨度有界**：i32::MIN→MAX 级端点在带窗实现下与带内短段
//!    同阶完成（迭代上界 = 带宽 + 2），且落带像素集恰为几何期望
//!    （水平/垂直/主对角三向——闭式重构的巨 k_lo 路径专测）。

use berry_gfx::cmd::{Color, CommandEncoder, Rect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;

const W: i32 = 16;
const H: i32 = 16;
const WHITE: Color = Color { r: 255, g: 255, b: 255 };

/// 渲染一条线，返回亮像素序列（行主序扫描非零 u32——白线画在零底上，
/// 亮集 = 写入集；线不重访像素，序列 = 集合）。
fn line_pixels(dirty: Option<Rect>, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let d = TextureDesc::new(W as usize, H as usize, W as usize * 4, TextureFormat::Rgbx).unwrap();
    let mut fb = vec![0u8; d.need_bytes()];
    let mut enc = CommandEncoder::new();
    enc.draw_line(x0, y0, x1, y1, WHITE).unwrap();
    let dl = enc.finish();
    Queue::new()
        .submit(&dl, &mut fb, &d, &PassDescriptor::new(LoadOp::Load, StoreOp::Store, dirty))
        .unwrap();
    let mut px = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let off = y as usize * d.stride() + x as usize * 4;
            if u32::from_le_bytes(fb[off..off + 4].try_into().unwrap()) != 0 {
                px.push((x, y));
            }
        }
    }
    px.sort(); // 集合语义：与绘制序无关（扫描序/推演序不进断言）
    px
}

// —— 手算字面集：推演口径 dx=|Δx|、dy=−|Δy|（≤0）、err=dx+dy；每步
// 先画后进，e2=2·err：e2≥dy → x 进、e2≤dx → y 进；到端点 break。
//
// tie 咬合两例（变体身份指纹——推演实录）：
// - (0,0)→(4,2)：dx=4, dy=−2, err=2。k0 画(0,0)，e2=4 == dx（tie 咬
//   `<=` → y 进）→ (1,1)；k1 e2=4... err=(2−2)+4=4 → e2=8>dx 只 x 进
//   → (2,1)；k2 err=4−2=2, e2=4 == dx（第二次 tie）→ y 进 → (3,2)；
//   k3 err=2−2+4=4, e2=8 → 只 x 进 → (4,2) 端点。
// - (0,0)→(2,4)：dx=2, dy=−4, err=−2。k0 画(0,0)，e2=−4 == dy（tie
//   咬 `>=` → x 进）→ (1,1)；k1 err=−2−4+2=−4, e2=−8<dy 只 y 进 →
//   (1,2)；k2 err=−4+2=−2, e2=−4 == dy（第二次 tie）→ x 进 → (2,3)；
//   k3 → (2,4) 端点。
// 改 `>` / 改 `<` 任一处，对应 tie 例像素集漂移、此测试必炸。
#[test]
fn line_literals_with_tie_bites() {
    let cases: &[((i32, i32, i32, i32), &[(i32, i32)])] = &[
        // 浅正双正：y 序 0,0,1,1,2,2。
        ((0, 0, 5, 2), &[(0, 0), (1, 0), (2, 1), (3, 1), (4, 2), (5, 2)]),
        // tie e2==dx ×2（咬 `<=`）。
        ((0, 0, 4, 2), &[(0, 0), (1, 1), (2, 1), (3, 2), (4, 2)]),
        // steep，tie e2==dy ×2（咬 `>=`）。
        ((0, 0, 2, 4), &[(0, 0), (1, 1), (1, 2), (2, 3), (2, 4)]),
        // steep + sx=−1（负向主轴）。
        ((2, 0, 0, 5), &[(2, 0), (2, 1), (1, 2), (1, 3), (0, 4), (0, 5)]),
        // 浅反双负（sx=sy=−1），9 步。
        ((10, 3, 1, 1), &[
            (10, 3), (9, 3), (8, 3), (7, 2), (6, 2), (5, 2), (4, 2), (3, 1), (2, 1), (1, 1),
        ]),
        // 浅 E/D=2/3 非整比（y 序 0,1,1,2,3,3,4）。
        ((0, 0, 6, 4), &[(0, 0), (1, 1), (2, 1), (3, 2), (4, 3), (5, 3), (6, 4)]),
    ];
    for &((x0, y0, x1, y1), expect) in cases {
        let mut exp = expect.to_vec();
        exp.sort(); // 集合对拍（字面表按推演序书写，断言按 canonical 序）
        assert_eq!(
            line_pixels(None, x0, y0, x1, y1),
            exp,
            "线 ({x0},{y0})→({x1},{y1}) 像素集（手算钉板）"
        );
    }
}

/// 原算法逐字参考（带窗化前的实现——等价基准；与 src 推进逻辑零共享，
/// 手工抄录自审查前版本）。返回全程像素序列（不裁剪）。
fn line_reference(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i64, i64)> {
    let (mut x, mut y) = (x0 as i64, y0 as i64);
    let (x1, y1) = (x1 as i64, y1 as i64);
    let dx = (x1 - x).abs();
    let dy = -(y1 - y).abs();
    let sx = if x < x1 { 1 } else { -1 };
    let sy = if y < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut out = Vec::new();
    loop {
        out.push((x, y));
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    out
}

// —— 等价性语料：7^4 = 2401 段（网格含帧外端点 −8/−3/11），双窗面
// （整帧 = clip 帧界路径；dirty 8×8 = 带窗 + put_px 复合裁剪路径）。
// 带窗实现的窗解/闭式折算/残差重构任一处在任一八分位出错，此测试炸。
#[test]
fn line_equivalence_corpus() {
    const GRID: [i32; 7] = [-8, -3, 0, 1, 2, 5, 11];
    let dirty = Rect::new(4, 4, 8, 8);
    let filter = |pts: Vec<(i64, i64)>, bx: i64, by: i64, bw: i64, bh: i64| -> Vec<(i32, i32)> {
        let mut v: Vec<(i32, i32)> = pts
            .into_iter()
            .filter(|&(x, y)| x >= bx && x < bx + bw && y >= by && y < by + bh)
            .map(|(x, y)| (x as i32, y as i32))
            .collect();
        v.sort(); // 集合对拍（参考序列为绘制序）
        v
    };
    let mut n = 0usize;
    for &x0 in &GRID {
        for &y0 in &GRID {
            for &x1 in &GRID {
                for &y1 in &GRID {
                    let full = line_reference(x0, y0, x1, y1);
                    assert_eq!(
                        line_pixels(None, x0, y0, x1, y1),
                        filter(full.clone(), 0, 0, W as i64, H as i64),
                        "带窗 ↔ 原算法（整帧）({x0},{y0})→({x1},{y1})"
                    );
                    assert_eq!(
                        line_pixels(Some(dirty), x0, y0, x1, y1),
                        filter(full, dirty.x as i64, dirty.y as i64, dirty.w as i64, dirty.h as i64),
                        "带窗 ↔ 原算法（dirty 窗）({x0},{y0})→({x1},{y1})"
                    );
                    n += 2;
                }
            }
        }
    }
    assert_eq!(n, 2401 * 2, "语料规模钉板（7^4 段 × 双窗）");
}

// —— 敌意巨跨度：带窗实现下迭代上界 = 带宽 + 2（R2-F1 的执法面——
// 若退回逐像素全程推进，i32::MIN→MAX = 2^32 次迭代，本测试挂死到
// harness 超时 = 炸）。三向几何期望逐像素断言：闭式重构在巨 k_lo
// （2^31 级）处算错坐标/err，落带像素集即漂。
#[test]
fn line_hostile_spans_bounded() {
    // 水平：行 8 全亮（k_lo = 2^31 级前导步被闭式跳过）。
    let expect: Vec<(i32, i32)> = (0..W).map(|x| (x, 8)).collect();
    assert_eq!(line_pixels(None, i32::MIN, 8, i32::MAX, 8), expect, "巨跨度水平线");
    // 垂直：列 5 全亮（steep 窗 + b=0 无 x 漂移）。
    let expect: Vec<(i32, i32)> = (0..H).map(|y| (5, y)).collect();
    assert_eq!(line_pixels(None, 5, i32::MIN, 5, i32::MAX), expect, "巨跨度垂直线");
    // 主对角：x==y 的 16 像素（shallow 界上 dx==E，每步双进；i128
    // 重构路径 + 残差恰为零的形态）。
    let expect: Vec<(i32, i32)> = (0..W).map(|t| (t, t)).collect();
    assert_eq!(
        line_pixels(None, i32::MIN, i32::MIN, i32::MAX, i32::MAX),
        expect,
        "巨跨度主对角线"
    );
    // 整线在帧外：零像素 + 窗空早退（不进循环）。
    assert_eq!(line_pixels(None, 100, 100, 200, 200), Vec::new(), "帧外线零像素");
    assert_eq!(
        line_pixels(Some(Rect::new(4, 4, 8, 8)), 100, 100, 200, 200),
        Vec::new(),
        "帧外线经 dirty 复验"
    );
    // 单点：巨坐标单点在带外 = no-op。
    assert_eq!(line_pixels(None, i32::MIN, i32::MIN, i32::MIN, i32::MIN), Vec::new());
}
