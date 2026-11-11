//! berry-ui 脏区引擎（GUI-3——`docs/GUI-3实施设计草案.md` §3.4；gfx
//! 坑 5「负 w/h max(0,·) 收口」+ 坑 10「坐标三套显式换算」的执法面）。
//!
//! - [`RectI`]：内部几何 i32 面（**负 w/h 内部恒非法**——`new` 收口）；
//!   与 wire 面（`berry_guiproto::Rect` u32）互转一律显式饱和，无裸
//!   `as`（§0 铁律）。
//! - [`try_merge`]：Σ×1.25 浪费帽合并判定（包络×`MERGE_WASTE_DEN` ≤
//!   面积和×`MERGE_WASTE_NUM` 用 u128 中间量——u64 巨参不溢出不 panic）。
//!   相交对**非恒可并**（分形）：并集为矩形的相交形（包含/十字/边贴）
//!   包络 = 并集 ≤ 面积和——恒可并；L 形角交包络可超面积和
//!   （(0,0,100,1)+(0,0,1,100)：包络 10000 > Σ 200 → 拒并）。无相交
//!   特判，一律走浪费帽（design §3.4「相交恒可并」为过强断言——以
//!   §6 冻结判定式为准）。
//! - [`DamageSet`]：K≤`DAMAGE_MAX`=8 定容集——与既有矩形试并（优先
//!   相交者）；满 8 找最小包络可并对；无可并可并 → `full=true` 清矩
//!   形集（FULL 后 add no-op）。**零面积矩形入集即丢弃**（clip 后残
//!   片的常规归宿）。
//! - [`RectSet`]：不相交矩形集（遮挡剔除的几何底座）——入集自动与既
//!   有不相交化；容量溢出保守**少记覆盖**（is_covered 更常返 false →
//!   多画不少画——画家算法下多画是正确性冗余、少画是视觉洞）。
//!
//! 零 panic / 零分配：全定容栈缓冲；u64 巨参（w/h = u32::MAX 量级）
//! 的面积/比较走 u128 中间量饱和收口。

use berry_guiproto::Rect;

/// 合并浪费帽分子（Σ×1.25 = ×5/4——§6 冻结）。
pub const MERGE_WASTE_NUM: u64 = 5;
/// 合并浪费帽分母。
pub const MERGE_WASTE_DEN: u64 = 4;

// ---------- 内部几何（i32 面） ----------

/// 内部几何矩形：坐标三套之「内部 i32」（§0 铁律）。负 w/h 内部恒非
/// 法——[`RectI::new`] 收口（gfx 坑 5）；坐标可负（屏幕外/动画偏移），
/// 尺寸恒 ≥ 0。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectI {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// 零矩形常量（RectSet 定容数组的初值面）。
pub(crate) const RECTI_ZERO: RectI = RectI { x: 0, y: 0, w: 0, h: 0 };

impl RectI {
    /// 构造收口：w/h 取 `max(0, ·)`——负尺寸（敌意/溢出中间量）折零
    /// 而非带毒入集。
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w: if w < 0 { 0 } else { w }, h: if h < 0 { 0 } else { h } }
    }

    /// 零尺寸判空（w 或 h 为 0）。
    pub const fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// 右/下开边界（饱和加——i32::MAX 侧不 panic）。
    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.w)
    }

    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.h)
    }

    /// 面积（u64——(2^32−1)² < u64::MAX 恰容纳）。
    pub fn area(&self) -> u64 {
        self.w as i64 as u64 * self.h as i64 as u64
    }

    /// 点包含（半开区间）。
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }

    /// 交集（无交 → 零尺寸；i64 中间量防 i32 减法溢出）。
    pub fn intersect(&self, other: &RectI) -> RectI {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = (self.right() as i64).min(other.right() as i64);
        let b = (self.bottom() as i64).min(other.bottom() as i64);
        RectI::new(x, y, i64_to_i32_sat(r - x as i64), i64_to_i32_sat(b - y as i64))
    }

    /// 包络（最小覆盖两者的矩形）。
    pub fn envelope(&self, other: &RectI) -> RectI {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let r = (self.right() as i64).max(other.right() as i64);
        let b = (self.bottom() as i64).max(other.bottom() as i64);
        RectI::new(x, y, i64_to_i32_sat(r - x as i64), i64_to_i32_sat(b - y as i64))
    }

    /// i32 → wire u32：负侧饱和 0（前提 = 已 clip viewport——Ui::
    /// damage_rect 收口侧义务；正侧 i32::MAX < u32::MAX 直转）。
    pub fn to_wire(&self) -> Rect {
        Rect {
            x: neg_sat_u32(self.x),
            y: neg_sat_u32(self.y),
            w: self.w.max(0) as u32,
            h: self.h.max(0) as u32,
        }
    }

    /// wire u32 → i32：checked 饱和（> i32::MAX → i32::MAX）。
    pub fn from_wire(r: &Rect) -> RectI {
        RectI::new(
            u32_to_i32_sat(r.x),
            u32_to_i32_sat(r.y),
            u32_to_i32_sat(r.w),
            u32_to_i32_sat(r.h),
        )
    }
}

/// i32 负值 → u32 饱和 0（坐标单侧饱和——尺寸侧由 `new` 收口恒 ≥0）。
fn neg_sat_u32(v: i32) -> u32 {
    if v < 0 {
        0
    } else {
        v as u32
    }
}

/// u32 → i32 饱和（> i32::MAX → i32::MAX）。
pub(crate) fn u32_to_i32_sat(v: u32) -> i32 {
    if v > i32::MAX as u32 {
        i32::MAX
    } else {
        v as i32
    }
}

/// i64 → i32 饱和（双侧）。
pub(crate) fn i64_to_i32_sat(v: i64) -> i32 {
    if v > i32::MAX as i64 {
        i32::MAX
    } else if v < i32::MIN as i64 {
        i32::MIN
    } else {
        v as i32
    }
}

// ---------- 合并判定 ----------

/// wire u32 两矩形交集判（u64 右/下界——x+w 可越 u32 域）。
fn wire_intersects(a: &Rect, b: &Rect) -> bool {
    let ar = a.x as u64 + a.w as u64;
    let ab = a.y as u64 + a.h as u64;
    let br = b.x as u64 + b.w as u64;
    let bb = b.y as u64 + b.h as u64;
    (a.x.max(b.x) as u64) < ar.min(br) && (a.y.max(b.y) as u64) < ab.min(bb)
}

/// wire u32 两矩形包络（u64 几何 → u32 饱和收口）。
fn wire_envelope(a: &Rect, b: &Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let r = (a.x as u64 + a.w as u64).max(b.x as u64 + b.w as u64);
    let bo = (a.y as u64 + a.h as u64).max(b.y as u64 + b.h as u64);
    Rect {
        x,
        y,
        w: u64_to_u32_sat(r - x as u64),
        h: u64_to_u32_sat(bo - y as u64),
    }
}

/// u64 → u32 饱和（pub(crate) 单源：本面 wire_envelope + layout/metrics
/// 的 u64 宽累计收口共用——GUI-6 D28 metrics 抽出）。
pub(crate) fn u64_to_u32_sat(v: u64) -> u32 {
    if v > u32::MAX as u64 {
        u32::MAX
    } else {
        v as u32
    }
}

/// Σ×1.25 浪费帽合并判定（§6 冻结签名）：`envelope.area() ×
/// MERGE_WASTE_DEN ≤ (a.area + b.area) × MERGE_WASTE_NUM` 则可并、返
/// 包络；**严格大于即拒并**（返 None）。u128 中间量——u32::MAX 量级
/// 巨参的 ×5 乘积不溢出不 panic。相交对分形：并集为矩形（包含/十
/// 字/边贴）时包络 = 并集 ≤ 面积和——恒可并；L 形角交包络可超面积
/// 和（(0,0,100,1)+(0,0,1,100)：包络 10000 > Σ 200 → 拒并）。无相
/// 交特判，一律走浪费帽（design §3.4「相交恒可并」为过强断言——
/// §6 冻结判定式为准）。
pub fn try_merge(a: &Rect, b: &Rect) -> Option<Rect> {
    if a.w == 0 || a.h == 0 || b.w == 0 || b.h == 0 {
        return None; // 零面积不入合并代数（调用侧 add 已丢弃，防御双保险）
    }
    let env = wire_envelope(a, b);
    let env_area = env.w as u128 * env.h as u128;
    let sum = (a.w as u128 * a.h as u128) + (b.w as u128 * b.h as u128);
    if env_area * MERGE_WASTE_DEN as u128 <= sum * MERGE_WASTE_NUM as u128 {
        Some(env)
    } else {
        None
    }
}

// ---------- 脏区集（wire 面） ----------

/// 脏区定容集（K≤8 + FULL 哨兵）。`rects[..count]` 恒有效；`full` 真
/// 时 count 恒 0（FULL = 全屏重绘语义，矩形集已清）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageSet {
    pub rects: [Rect; berry_guiproto::DAMAGE_MAX],
    pub count: u8,
    pub full: bool,
    /// pre-merge 入集尝试计数（GUI-6 段E D19 S7 度量面）：`add` 入口
    /// 逐次 +1（饱和 u16；含 FULL/零面积早退者——「入集尝试」口径），
    /// 与合并后存活 `count` 对拍 = 合并率可观测。`add_full` 不计（非
    /// add 入口）；`new`/`clear` 归零（drain 换新即每帧重置）。
    pub adds: u16,
}

impl DamageSet {
    pub const fn new() -> Self {
        Self {
            rects: [Rect { x: 0, y: 0, w: 0, h: 0 }; berry_guiproto::DAMAGE_MAX],
            count: 0,
            full: false,
            adds: 0,
        }
    }

    /// 空集判（full=false 且 count=0——FULL 语义 = 非空）。
    pub const fn is_empty(&self) -> bool {
        !self.full && self.count == 0
    }

    /// 追加一条脏区矩形（**入集前应已 clip viewport**——Ui::
    /// damage_rect 侧义务；本面零面积/FULL 后 no-op）：
    /// 1. 与既有矩形试并（优先相交者，其次包络最小者）——并成即位替换；
    /// 2. 未并可且 count < 8 → 直挂；
    /// 3. count == 8 → 找既有两两最小包络可并对（并出一槽）再挂；
    /// 4. 无可并可并 → `full=true` 清矩形集。
    pub fn add(&mut self, r: Rect) {
        self.adds = self.adds.saturating_add(1); // 入集尝试计数（见字段注）
        if self.full || r.w == 0 || r.h == 0 {
            return;
        }
        // ① 优先相交者中的可并（最小包络）
        let mut hit: Option<(usize, Rect)> = None;
        for pass in 0..2 {
            for i in 0..self.count as usize {
                let inter = wire_intersects(&self.rects[i], &r);
                if pass == 0 && !inter {
                    continue; // 第一轮只看相交者
                }
                if let Some(m) = try_merge(&self.rects[i], &r) {
                    let better = match &hit {
                        Some((_, hm)) => area_le(&m, hm),
                        None => true,
                    };
                    if better {
                        hit = Some((i, m));
                    }
                }
            }
            if hit.is_some() {
                break; // 有相交可并对即取（非相交者不让位）
            }
        }
        if let Some((i, m)) = hit {
            self.rects[i] = m;
            return;
        }
        // ② 未并可并：未满直挂
        if (self.count as usize) < berry_guiproto::DAMAGE_MAX {
            self.rects[self.count as usize] = r;
            self.count += 1;
            return;
        }
        // ③ 满 8：既有两两找最小包络可并对并出一槽再挂
        let mut best: Option<(usize, usize, Rect)> = None;
        for i in 0..self.count as usize {
            for j in (i + 1)..self.count as usize {
                if let Some(m) = try_merge(&self.rects[i], &self.rects[j]) {
                    let better = match &best {
                        Some((_, _, bm)) => area_le(&m, bm),
                        None => true,
                    };
                    if better {
                        best = Some((i, j, m));
                    }
                }
            }
        }
        if let Some((i, j, m)) = best {
            // j 挪尾收缩、i 位放包络（槽序无语义——纯集）
            let last = self.count as usize - 1;
            self.rects[i] = m;
            self.rects[j] = self.rects[last];
            self.count -= 1;
            self.rects[self.count as usize] = r;
            self.count += 1;
            return;
        }
        // ④ 无可并可并 → FULL 清集
        self.full = true;
        self.count = 0;
    }

    /// FULL 直置（动画 Transform/大改面——全屏重绘语义）。
    pub fn add_full(&mut self) {
        self.full = true;
        self.count = 0;
    }

    /// 有效矩形迭代（FULL 时空——FULL 语义由 full 位承载）。
    pub fn iter(&self) -> impl Iterator<Item = &Rect> {
        self.rects[..self.count as usize].iter()
    }

    /// 全集包络（full → None = 「包络无意义，按全屏处理」；空集 →
    /// None）。u64 几何收口。
    pub fn envelope(&self) -> Option<Rect> {
        if self.full || self.count == 0 {
            return None;
        }
        let mut acc = self.rects[0];
        for r in self.rects[1..self.count as usize].iter() {
            acc = wire_envelope(&acc, r);
        }
        Some(acc)
    }

    /// Σ面积（FULL → u64::MAX 语义——「至少这么大」的上界陈述）。
    pub fn union_px(&self) -> u64 {
        if self.full {
            return u64::MAX;
        }
        let mut sum: u128 = 0;
        for r in self.rects[..self.count as usize].iter() {
            sum += r.w as u128 * r.h as u128;
        }
        if sum > u64::MAX as u128 {
            u64::MAX
        } else {
            sum as u64
        }
    }

    pub fn clear(&mut self) {
        self.full = false;
        self.count = 0;
        self.adds = 0; // 换新归零（drain 面每帧重置口径）
    }
}

fn area_le(a: &Rect, b: &Rect) -> bool {
    let aa = a.w as u128 * a.h as u128;
    let ab = b.w as u128 * b.h as u128;
    aa <= ab
}

// ---------- 不相交矩形集（遮挡剔除几何底座） ----------

/// 可见片族的固定缓冲帽（N≤16 时 4N+4=68 的上界 + 4 余量；更大 N 的
/// 病态几何走保守丢弃——见 [`RectSet::visible`]）。
const VISIBLE_PIECE_CAP: usize = 72;

/// 不相交矩形集（遮挡覆盖面）：入集自动与既有不相交化（`add` 先减后
/// 入）；容量 N 溢出保守**少记覆盖**——`is_covered` 更常 false → 剔除
/// 更保守（多画不少画）。paint 的遮挡源 = 实心 Rect 叶子（Text 蜂格
/// 有洞恒不遮挡——§3.5）。
#[derive(Debug, Clone, Copy)]
pub struct RectSet<const N: usize> {
    rects: [RectI; N],
    len: usize,
}

/// 单矩形挖洞的余片（≤4 条带：上/下/左/右）。无交 → 原样一片。
fn subtract_one(target: RectI, hole: RectI) -> ([RectI; 4], usize) {
    let inter = target.intersect(&hole);
    if inter.is_empty() {
        return ([target, RECTI_ZERO, RECTI_ZERO, RECTI_ZERO], 1);
    }
    let mut n = 0usize;
    let mut out = [RECTI_ZERO; 4];
    // 上带（target 顶到 inter 顶）
    out[n] = RectI::new(target.x, target.y, target.w, inter.y - target.y);
    if !out[n].is_empty() {
        n += 1;
    }
    // 下带（inter 底到 target 底）
    out[n] = RectI::new(target.x, inter.bottom(), target.w, target.bottom() - inter.bottom());
    if !out[n].is_empty() {
        n += 1;
    }
    // 左带（inter 竖带左翼）
    out[n] = RectI::new(target.x, inter.y, inter.x - target.x, inter.h);
    if !out[n].is_empty() {
        n += 1;
    }
    // 右带（inter 竖带右翼）
    out[n] = RectI::new(inter.right(), inter.y, target.right() - inter.right(), inter.h);
    if !out[n].is_empty() {
        n += 1;
    }
    (out, n)
}

impl<const N: usize> RectSet<N> {
    pub const fn new() -> Self {
        Self { rects: [RECTI_ZERO; N], len: 0 }
    }

    /// 入集（返回 false = 满帽拒入）：先 `subtract` 自身于既有（保持
    /// 不相交不变量）再占槽。零面积 = 无覆盖贡献，恒 true。
    pub fn add(&mut self, r: RectI) -> bool {
        if r.is_empty() {
            return true;
        }
        self.subtract(r);
        if self.len >= N {
            return false; // 满帽：r 覆盖丢失（保守少记——多画不少画）
        }
        self.rects[self.len] = r;
        self.len += 1;
        true
    }

    /// 补集减：自集内每片挖去 r（挖后 ≤4 片；容量溢出的余片保守丢弃
    /// ——少记覆盖方向，见类型注）。
    pub fn subtract(&mut self, r: RectI) {
        if r.is_empty() {
            return;
        }
        let mut write = 0usize;
        let mut read = 0usize;
        while read < self.len {
            let cur = self.rects[read];
            read += 1;
            let (pieces, np) = subtract_one(cur, r);
            for k in 0..np {
                if write < N {
                    self.rects[write] = pieces[k];
                    write += 1;
                } // else 余片丢弃（保守）
            }
        }
        self.len = write;
    }

    /// r ⊆ 集？（集内矩形两两不相交 → Σ 交面积 == r 面积 ⇔ 覆盖；
    /// 零面积 r 恒 true）。u128 累计防巨参溢出。
    pub fn is_covered(&self, r: &RectI) -> bool {
        if r.is_empty() {
            return true;
        }
        let mut covered: u128 = 0;
        for i in 0..self.len {
            covered += self.rects[i].intersect(r).area() as u128;
        }
        covered == r.area() as u128
    }

    /// r − 集 的最大可见片（空 → 零尺寸）。逐矩形挖洞保留余片族，取
    /// 面积最大者；余片族超 [`VISIBLE_PIECE_CAP`]（N≤16 的 4N+4 精确
    /// 上界外）的病态几何保守丢弃新片——报告偏小，剔除侧安全。
    pub fn visible(&self, r: &RectI) -> RectI {
        if r.is_empty() {
            return RECTI_ZERO;
        }
        let mut pieces = [RECTI_ZERO; VISIBLE_PIECE_CAP];
        pieces[0] = *r;
        let mut np = 1usize;
        for i in 0..self.len {
            let mut next = [RECTI_ZERO; VISIBLE_PIECE_CAP];
            let mut nn = 0usize;
            for k in 0..np {
                let (subs, ns) = subtract_one(pieces[k], self.rects[i]);
                for s in 0..ns {
                    if nn < VISIBLE_PIECE_CAP {
                        next[nn] = subs[s];
                        nn += 1;
                    } // else 病态几何余片丢弃（保守）
                }
            }
            pieces = next;
            np = nn;
            if np == 0 {
                return RECTI_ZERO; // 全盖
            }
        }
        let mut best = RECTI_ZERO;
        for k in 0..np {
            if pieces[k].area() > best.area() {
                best = pieces[k];
            }
        }
        best
    }

    /// 覆盖总面积（集内不相交 → 直和 无双计）。
    pub fn area(&self) -> u64 {
        let mut sum: u128 = 0;
        for i in 0..self.len {
            sum += self.rects[i].area() as u128;
        }
        if sum > u64::MAX as u128 {
            u64::MAX
        } else {
            sum as u64
        }
    }

    /// 集内矩形数。
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
