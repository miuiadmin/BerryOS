//! berry-ui 消费侧重采样最终一跳（GUI-3——`docs/GUI-3实施设计草案.md`
//! §3.9；语义逐条对齐 gui-inputd `resample_to_vsync`（I:168）——本模块
//! 是**消费侧镜像**，两处常量同引 berry-guiproto 单源，不长第二套）。
//!
//! 语义（§3.9）：
//! - 少于 2 采样 / 时间戳非单调（显式严格递增预检——含首尾同 ts 与
//!   区间重复 ts）/ 跨度 < `RESAMPLE_MIN_DELTA_MS`（信息量不足）或 >
//!   `RESAMPLE_MAX_DELTA_MS`（太陈旧）→ None；
//! - `target ∈ [首, 尾]`：括点线性插值（ts 报 target）；
//! - `target > 尾`：按尾两采样斜率前向外推，**外推视界帽 =
//!   min(跨度/2, `RESAMPLE_MAX_PREDICTION_MS`)**——超帽位置停在帽沿
//!   （保守：假定帽外无运动），ts 仍诚实报 target；
//! - `target < 首`：后向外推不提供 → None；撞 ts 取新采样。
//!
//! 目标推导（Android 同式）：`target = expected_present_ns −
//! RESAMPLE_LATENCY_MS`（[`target_from_expected`]——pump_input 对活跃
//! MOVE 流的应用入口）。
//!
//! 零 panic / 零分配：全部拒绝路径显式 None；坐标 i64 全程 i128 插值
//! 中间量（|(b−a)×num| 上界 i64² 必溢——先乘除后 clamp 回 i64）。

/// 重采样输入采样（§6 冻结字段；坐标 i64 = 内部几何口径——比 gui-inputd
/// 的 wire u32 面宽，引擎内无 u32 收缩损耗）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub ts_ns: u64,
    pub x: i64,
    pub y: i64,
}

/// ms → ns（guiproto 常量以 ms 计）。
const MS: u64 = 1_000_000;

/// 目标推导（§3.9 Android 同式）：目标呈现时刻 − 合成延迟。饱和减
/// （expected 极小值容忍——不 panic 不回绕）。
pub fn target_from_expected(expected_present_ns: u64) -> u64 {
    expected_present_ns.saturating_sub(berry_guiproto::RESAMPLE_LATENCY_MS * MS)
}

/// 线性插值 a→b（num/den ∈ [0,1] 区间外截断由调用面几何保证——与
/// gui-inputd `lerp` 同式，i64 面 i128 中间量）。
fn lerp_i64(a: i64, b: i64, num: u64, den: u64) -> i64 {
    let v = (a as i128) + ((b as i128) - (a as i128)) * (num as i128) / (den as i128);
    v.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// 前向外推：last + (last−prev) × dt/den（signed——支持回退方向的轴；
/// 与 gui-inputd `extrap` 同式）。
fn extrap_i64(last: i64, prev: i64, dt: u64, den: u64) -> i64 {
    let v = (last as i128) + ((last as i128) - (prev as i128)) * (dt as i128) / (den as i128);
    v.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// 纯函数重采样（§6 冻结签名）：`hist` = 手势历史采样（oldest→newest），
/// 输出 = `target_ns` 时刻的插值/外推采样（resampled 标记随 Msg 透出，
/// 由 pump_input 置位——本函数不管标记）。常量全部
/// `use berry_guiproto`（5/2/20/8ms——对齐条 16 单源纪律）。
pub fn resample_to_target(hist: &[Sample], target_ns: u64) -> Option<Sample> {
    let n = hist.len();
    if n < 2 {
        return None;
    }
    let first = &hist[0];
    let last = &hist[n - 1];
    // 显式单调预检：严格递增（a.ts ≥ b.ts 即拒——覆盖「首尾同 ts」与
    // 区间重复/回退 ts；gui-inputd 文档语义的明示执法，对单调输入行为
    // 与其实现逐位一致）
    for w in hist.windows(2) {
        if w[0].ts_ns >= w[1].ts_ns {
            return None;
        }
    }
    let span = last.ts_ns.checked_sub(first.ts_ns)?;
    if span < berry_guiproto::RESAMPLE_MIN_DELTA_MS * MS
        || span > berry_guiproto::RESAMPLE_MAX_DELTA_MS * MS
    {
        return None;
    }
    if target_ns < first.ts_ns {
        return None; // 后向外推不提供
    }
    if target_ns <= last.ts_ns {
        // 括点插值：找 [a, b] 使 a.ts ≤ target ≤ b.ts
        for w in hist.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if a.ts_ns <= target_ns && target_ns <= b.ts_ns {
                let den = b.ts_ns - a.ts_ns;
                if den == 0 {
                    // 撞 ts 双采样：取新采样（确定性仲裁——单调预检后
                    // 不可达，防御保留与 gui-inputd 同形）
                    return Some(Sample { ts_ns: target_ns, x: b.x, y: b.y });
                }
                return Some(Sample {
                    ts_ns: target_ns,
                    x: lerp_i64(a.x, b.x, target_ns - a.ts_ns, den),
                    y: lerp_i64(a.y, b.y, target_ns - a.ts_ns, den),
                });
            }
        }
        return None; // 括点未命中 = 时间戳异常（防御，拒绝不猜测）
    }
    // 前向外推：视界帽 min(跨度/2, MAX_PREDICTION)，按尾两采样斜率
    let cap = core::cmp::min(span / 2, berry_guiproto::RESAMPLE_MAX_PREDICTION_MS * MS);
    let clamped_target = core::cmp::min(target_ns, last.ts_ns + cap);
    let prev = &hist[n - 2];
    let den = last.ts_ns.checked_sub(prev.ts_ns)?;
    if den == 0 {
        return Some(Sample { ts_ns: target_ns, x: last.x, y: last.y });
    }
    Some(Sample {
        ts_ns: target_ns, // ts 诚实报 target（帽沿截停的是位置不是时刻）
        x: extrap_i64(last.x, prev.x, clamped_target - last.ts_ns, den),
        y: extrap_i64(last.y, prev.y, clamped_target - last.ts_ns, den),
    })
}
