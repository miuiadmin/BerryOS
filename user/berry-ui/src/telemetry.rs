//! berry-ui 双轨打点遥测（GUI-3——`docs/GUI-3实施设计草案.md` §3.11；
//! P8 一份数据喂三消费者：host 断言（本刀 tests）/ 性能工具 / AI 遥测
//! （guest 侧收口归 GUI-9）；expected/actual 双轨 = ADR #53）。
//!
//! - 环 [`FRAME_RING`] = 64：零分配静态环（[`Telemetry::record`] 覆写
//!   最旧），`[FrameRec; 64]` 常量零值初始化——no_std 无构造顺序坑。
//! - 计数器族（§6 冻结取值面）：`frames/wakes/idle_exits/
//!   spinup_late_frames/deadline_miss`。判定逻辑**单一事实源在 lib.rs**
//!   引擎循环（jank = actual > expected + period/2 在 note_presented 算
//!   ——expected 来自在途表；spinup 起振锚在 wake/render_frame 打），
//!   本模块只落账不判——遥测零策略。
//! - 零 panic / 零分配：seq 不在环内（已被覆写 64 帧）静默容让——
//!   present 应答迟到一环 depth 属显示服务异常，遥测不因此自陷。

/// 环容量（§6 冻结）。
pub const FRAME_RING: usize = 64;

/// 单帧记录（§6 冻结十二字段——expected_ns/actual_ns 双轨：expected =
/// 未来拍点（render 时已知），actual = present 应答回填）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRec {
    pub seq: u32,
    /// 本帧消费的最新输入 ts（无输入 0）。
    pub input_ts_ns: u64,
    pub frame_begin_ns: u64,
    pub frame_end_ns: u64,
    pub expected_ns: u64,
    /// present 应答回填（未应答 0——jank 判定以 expected_ns≠0 为前提）。
    pub actual_ns: u64,
    pub presented: bool,
    pub discarded: bool,
    pub damage_full: bool,
    pub damage_rects: u8,
    /// 本帧 damage `add` 入集尝试数（pre-merge——GUI-6 段E D19 S7
    /// 度量；与 `damage_rects` 对拍 = 合并率可观测面）。
    pub damage_adds: u16,
    /// relayout 耗时 ns（GUI-6 段E D19 S7 度量）。引擎无钟恒 0——
    /// 真耗时 host bench（tests/bench_s7.rs）围帧双打点（frame_end_ns
    /// 同型注记，GUI-9 embedder 双打点接管）。
    pub relayout_ns: u64,
    pub overdraw_x256: u32,
    pub jank: bool,
}

const fn zero_rec() -> FrameRec {
    FrameRec {
        seq: 0,
        input_ts_ns: 0,
        frame_begin_ns: 0,
        frame_end_ns: 0,
        expected_ns: 0,
        actual_ns: 0,
        presented: false,
        discarded: false,
        damage_full: false,
        damage_rects: 0,
        damage_adds: 0,
        relayout_ns: 0,
        overdraw_x256: 0,
        jank: false,
    }
}

/// 遥测面（§6 冻结读面 pub；打点入口 pub(crate) = lib.rs
/// render_frame/wake/note_presented 单点喂入）。
pub struct Telemetry {
    ring: [FrameRec; FRAME_RING],
    /// 下一写入位（环游标）。
    head: usize,
    /// 已存条数（≤ FRAME_RING——未满时 last(n) 只回真数据）。
    live: usize,
    frames: u64,
    wakes: u64,
    idle_exits: u64,
    spinup_late_frames: u32,
    deadline_miss: u32,
}

impl Telemetry {
    pub(crate) const fn new() -> Self {
        Self {
            ring: [zero_rec(); FRAME_RING],
            head: 0,
            live: 0,
            frames: 0,
            wakes: 0,
            idle_exits: 0,
            spinup_late_frames: 0,
            deadline_miss: 0,
        }
    }

    /// 帧记录入环（render_frame 尾调用；lib.rs 单点）：frames++，
    /// 覆写最旧。jank/actual 此刻恒零值——present 应答时
    /// [`Telemetry::fill_presented`] 回填。
    pub(crate) fn record(&mut self, rec: FrameRec) {
        self.ring[self.head] = rec;
        self.head = (self.head + 1) % FRAME_RING;
        if self.live < FRAME_RING {
            self.live += 1;
        }
        self.frames += 1;
    }

    /// wake 打点（§3.8：wake → Run 立即渲不欠拍 + 起振遥测 wakes++）。
    pub(crate) fn bump_wakes(&mut self) {
        self.wakes += 1;
    }

    /// Idle→Run 退出打点（§3.8 idle_exits++——与 bump_wakes 分立：非
    /// Idle 态的 wake 只计 wakes）。
    pub(crate) fn bump_idle_exits(&mut self) {
        self.idle_exits += 1;
    }

    /// 起振迟到帧打点（Weston「extra vblank」1–2 帧延迟可观测，§3.8
    /// 增件②；锚判定在 lib.rs render_frame 的 spinup_anchor）。
    pub(crate) fn bump_spinup_late(&mut self) {
        self.spinup_late_frames += 1;
    }

    /// 按 seq 新→旧找环内记录位（present 应答通常针对最新记录——首中
    /// 即返；找不到（已被覆写/未知 seq）None 容让不 panic）。
    fn index_of_seq(&self, seq: u32) -> Option<usize> {
        for k in 0..self.live {
            let idx = (self.head + FRAME_RING - 1 - k) % FRAME_RING;
            if self.ring[idx].seq == seq {
                return Some(idx);
            }
        }
        None
    }

    /// present 应答回填（lib.rs `note_presented` 委托；§3.8）：actual
    /// 轨落位 + presented/discarded 状态位 + jank 落账（**判定在 lib.rs**
    /// ——expected 取自在途帧表，本模块零策略）；jank=true →
    /// `deadline_miss++`。
    pub(crate) fn fill_presented(
        &mut self,
        seq: u32,
        actual_ns: u64,
        presented: bool,
        discarded: bool,
        jank: bool,
    ) {
        if let Some(i) = self.index_of_seq(seq) {
            let rec = &mut self.ring[i];
            rec.actual_ns = actual_ns;
            rec.presented = presented;
            rec.discarded = discarded;
            rec.jank = jank;
        }
        if jank {
            self.deadline_miss += 1;
        }
    }

    /// 最近 n 帧（时间序 oldest→newest；n 超存量取存量——未满环不回
    /// 零值假数据）。§6 冻结签名。
    pub fn last(&self, n: usize) -> impl Iterator<Item = &FrameRec> {
        let n = n.min(self.live);
        let start = (self.head + FRAME_RING - n) % FRAME_RING;
        (0..n).map(move |i| &self.ring[(start + i) % FRAME_RING])
    }

    /// 累计帧数（§6 冻结识面）。
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// wake 计数（§3.8 起振遥测）。
    pub fn wakes(&self) -> u64 {
        self.wakes
    }

    /// Idle→Run 退出计数。
    pub fn idle_exits(&self) -> u64 {
        self.idle_exits
    }

    /// 起振迟到帧计数（Weston「extra vblank」可观测面，§3.8 增件②）。
    pub fn spinup_late_frames(&self) -> u32 {
        self.spinup_late_frames
    }

    /// jank（deadline miss）计数（actual > expected + period/2，§3.8
    /// ——判定在 lib.rs，此处只累计）。
    pub fn deadline_miss(&self) -> u32 {
        self.deadline_miss
    }
}
