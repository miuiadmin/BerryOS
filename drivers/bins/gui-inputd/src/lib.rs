//! gui-inputd lib —— GUI-4 段① I3 纯逻辑面：virtio-input wire 事件流 →
//! 归一化 → PointerEvent 泵（实施依据 = docs/GUI-4实施设计草案.md §5.1/§6-I3）。
//!
//! 三段职责：
//! 1. **wire 适配**（`raw_from_wire`）：`VirtioInputEvent`（8B LE，value 线上
//!    u32）→ `RawInputEvent`（value 按 i32 解释——REL 轴语义可负）+ 驱动侧
//!    dequeue 时刻 ts_ns（规范无设备时间戳，Linux evdev 同款打点）。
//! 2. **归一化**（`normalize_axis`）：设备自述 ABS 域 `[cal_min, cal_max]` →
//!    屏幕像素 `[0, screen_len-1]`（i64 定点、端点钉板；域退化/零屏宽拒 None）。
//! 3. **泵**（`PointerPump`）：EV_SYN(SYN_REPORT) 关批，批内事件聚合为一个
//!    `PumpEvent`（坐标取批内最新采样，history oldest→newest、header=newest
//!    ——Android MotionSample 同序）；接触判据 = BTN_TOUCH 或 BTN_LEFT 任一
//!    value=1（等价）；空闲+接触→DOWN、接触中坐标变→MOVE、接触+释放→UP，
//!    **同批多条相边按事件序 emit**（press+release 同批 = DOWN+UP 完整配对
//!    ——QMP 单命令 down+up 的标准 tap 形态；连续同向按压/释放并边）；
//!    SYN_DROPPED→下一批合成 CANCEL（PE_FL_SYNTHESIZED）；未知 ev_type/code
//!    忽略但计数（诊断面）；history 溢出分批（>INPUT_SAMPLES_MAX 拆多个
//!    PHASE_MOVE 不丢采样）；pump 本体不做重采样（`resample_to_vsync` 是
//!    消费侧可选的独立纯函数）；delivery 仅透传。
//!
//! 采样模型（§5.1「批内事件聚合」的轴对实现口径）：evdev 绝对轴逐轴下发
//! （ABS_X 与 ABS_Y 是两个独立事件）——**一对 X/Y 轴事件构成一个完整位置
//! 采样**：X 开对、Y 关对成快照；落单 Y 与上个 X 配对、批关时落单 X 与
//! 当前 Y 补配（半更新采样不入 history——Android 历史采样同为完整位置）。
//!
//! 纯函数纪律（berry-virtio/berry-guiproto 同款）：零分配、零 unsafe、
//! 零 panic 路径（`new` 的 `assert` 是构造期配置执法，非数据面）；host
//! 单测全盖（inline `tests`——FakeSource 喂确定性序列 + 状态谓词断言，
//! C-4 FakeSrc 同法）。

#![cfg_attr(not(test), no_std)]

use berry_guiproto::Sample;
use berry_virtio::input::VirtioInputEvent;

use berry_guiproto::{
    encode_pointer_event, PointerEvent, BUTTON_L, BUTTON_M, BUTTON_R, INPUT_SAMPLES_MAX,
    PE_FL_SYNTHESIZED, PHASE_CANCEL, PHASE_DOWN, PHASE_MOVE, PHASE_UP, PRESSURE_UNAVAILABLE,
};
use berry_virtio::input::{
    ABS_X, ABS_Y, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_TOUCH, EV_ABS, EV_KEY, EV_SYN,
    SYN_DROPPED, SYN_REPORT,
};

/// 批累积帽（采样数）：常态批 ≤ 数采样；32/批 = 事件风暴面——诚实截断
/// （丢最旧保新 + 计数），与「溢出分批」互补（分批兜 ≤ 帽的常态）。
const BATCH_CAP: usize = 32;
/// 完整事件队列帽：drain 消费节奏下的弹性容量；满则丢新 + 计数
/// （常驻输入源恒有消费方——POLL 阻塞语义兜底，帽只是防御面）。
const QUEUE_CAP: usize = 32;

/// 驱动侧原始事件（wire 解码后、归一化前）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawInputEvent {
    /// dequeue 时刻（ns，CLOCK_MONOTONIC——驱动打点，非设备侧）。
    pub ts_ns: u64,
    pub ev_type: u16,
    pub code: u16,
    /// 线上 u32 的 i32 解释（REL 轴语义可负——转换点单源）。
    pub value: i32,
}

/// wire → 驱动侧事件（value u32→i32 转换点；ts_ns 由调用方打点）。
pub fn raw_from_wire(ts_ns: u64, ev: VirtioInputEvent) -> RawInputEvent {
    RawInputEvent { ts_ns, ev_type: ev.ev_type, code: ev.code, value: ev.value as i32 }
}

/// 原始事件源抽象（泵的喂数面——真源 = main 的 eventq 收割循环）。
pub trait RawSource {
    fn pop(&mut self) -> Option<RawInputEvent>;
}

/// host 测试喂键源（C-4 FakeSrc 同法：确定性序列 + 状态谓词断言）。
///
/// 定容 FIFO 环（零分配）：push 超幅 panic（host 测试序列写错——构造期
/// loud，非运行期静默截断）。
pub struct FakeSource {
    buf: [RawInputEvent; FAKE_CAP],
    head: usize,
    len: usize,
}

const FAKE_CAP: usize = 64;

impl FakeSource {
    pub fn new() -> Self {
        Self {
            buf: [RawInputEvent { ts_ns: 0, ev_type: 0, code: 0, value: 0 }; FAKE_CAP],
            head: 0,
            len: 0,
        }
    }

    pub fn push(&mut self, ev: RawInputEvent) {
        assert!(self.len < FAKE_CAP, "FakeSource 满（{FAKE_CAP} 槽）——测试序列超幅");
        self.buf[(self.head + self.len) % FAKE_CAP] = ev;
        self.len += 1;
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

impl Default for FakeSource {
    fn default() -> Self {
        Self::new()
    }
}

impl RawSource for FakeSource {
    fn pop(&mut self) -> Option<RawInputEvent> {
        if self.len == 0 {
            return None;
        }
        let ev = self.buf[self.head];
        self.head = (self.head + 1) % FAKE_CAP;
        self.len -= 1;
        Some(ev)
    }
}

/// 屏幕几何（整数物理像素，Y 向下为正——与 PointerEvent 同口径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenGeom {
    pub w: u32,
    pub h: u32,
}

/// 设备自述 ABS 校准域（config ABS_INFO 的 min/max——以设备为准，勿硬编
/// 32767；GUI-4 草案 §4.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsCal {
    pub min_x: i32,
    pub max_x: i32,
    pub min_y: i32,
    pub max_y: i32,
}

/// 绝对域 → 屏幕像素归一化：钳位后 i64 定点
/// `(v-cl_min)*(len-1)/(cl_max-cl_min)`——端点钉板（min→0、max→len-1），
/// 单调。拒绝 None：cal_max ≤ cal_min（域退化）/ screen_len == 0。
pub fn normalize_axis(value: i32, cal_min: i32, cal_max: i32, screen_len: u32) -> Option<u32> {
    if cal_max <= cal_min || screen_len == 0 {
        return None;
    }
    let v = value.clamp(cal_min, cal_max);
    let num = (v as i64 - cal_min as i64) * (screen_len as i64 - 1);
    let den = cal_max as i64 - cal_min as i64;
    Some((num / den) as u32)
}

/// vsync 对齐重采样（消费侧可选——泵本体不做，§5.1）。
///
/// 输入 = 手势历史采样（oldest→newest），输出 = `vsync_ns` 时刻的插值/
/// 外推采样（PE_FL_RESAMPLED 由调用方置位）：
/// - 少于 2 采样 / 时间戳非单调 / 首尾同 ts → None；
/// - 首尾跨度 Δ < `RESAMPLE_MIN_DELTA_MS`（信息量不足）或 >
///   `RESAMPLE_MAX_DELTA_MS`（太陈旧）→ None；
/// - `vsync ∈ [首, 尾]`：括点线性插值（ts 报 vsync）；
/// - `vsync > 尾`：按尾两采样斜率前向外推，**外推视界帽 =
///   min(Δ/2, RESAMPLE_MAX_PREDICTION_MS)**——超帽位置停在帽沿（保守：
///   假定帽外无运动），ts 仍诚实报 vsync；
/// - `vsync < 首`：后向外推不提供 → None。
///
/// `RESAMPLE_LATENCY_MS` 是调用侧目标推导参数（目标 vsync = 显示 vsync −
/// 合成延迟——Android 同式），本函数不消费它。
pub fn resample_to_vsync(samples: &[Sample], vsync_ns: u64) -> Option<Sample> {
    const MS: u64 = 1_000_000;
    let n = samples.len();
    if n < 2 {
        return None;
    }
    let first = &samples[0];
    let last = &samples[n - 1];
    let span = last.timestamp_ns.checked_sub(first.timestamp_ns)?;
    if span == 0
        || span < berry_guiproto::RESAMPLE_MIN_DELTA_MS * MS
        || span > berry_guiproto::RESAMPLE_MAX_DELTA_MS * MS
    {
        return None;
    }
    if vsync_ns < first.timestamp_ns {
        return None;
    }
    if vsync_ns <= last.timestamp_ns {
        // 括点插值：找 [a, b] 使 a.ts ≤ vsync ≤ b.ts
        for w in samples.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if a.timestamp_ns <= vsync_ns && vsync_ns <= b.timestamp_ns {
                let den = b.timestamp_ns - a.timestamp_ns;
                if den == 0 {
                    // 撞 ts 双采样：取新采样（确定性仲裁）
                    return Some(Sample { timestamp_ns: vsync_ns, x: b.x, y: b.y });
                }
                return Some(Sample {
                    timestamp_ns: vsync_ns,
                    x: lerp(a.x, b.x, vsync_ns - a.timestamp_ns, den),
                    y: lerp(a.y, b.y, vsync_ns - a.timestamp_ns, den),
                });
            }
        }
        return None; // 括点未命中 = 时间戳非单调（防御，拒绝不猜测）
    }
    // 前向外推：视界帽 min(Δ/2, MAX_PREDICTION)，按尾两采样斜率
    let cap = core::cmp::min(span / 2, berry_guiproto::RESAMPLE_MAX_PREDICTION_MS * MS);
    let target = core::cmp::min(vsync_ns, last.timestamp_ns + cap);
    let prev = &samples[n - 2];
    let den = last.timestamp_ns.checked_sub(prev.timestamp_ns)?;
    if den == 0 {
        return Some(Sample { timestamp_ns: vsync_ns, x: last.x, y: last.y });
    }
    Some(Sample {
        timestamp_ns: vsync_ns,
        x: extrap(last.x, prev.x, target - last.timestamp_ns, den),
        y: extrap(last.y, prev.y, target - last.timestamp_ns, den),
    })
}

/// 线性插值 a→b（num/den ∈ [0,1] 区间外的截断由调用面几何保证）。
fn lerp(a: u32, b: u32, num: u64, den: u64) -> u32 {
    let d = b as i64 - a as i64;
    (a as i64 + d * (num as i64) / (den as i64)).clamp(0, u32::MAX as i64) as u32
}

/// 前向外推：last + (last-prev) * dt/den（signed——支持回退方向的轴）。
fn extrap(last: u32, prev: u32, dt: u64, den: u64) -> u32 {
    let d = last as i64 - prev as i64;
    (last as i64 + d * (dt as i64) / (den as i64)).clamp(0, u32::MAX as i64) as u32
}

/// 泵配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PumpConfig {
    pub geom: ScreenGeom,
    pub cal: AbsCal,
    /// 透传面（SUBSCRIBE 的 delivery——v0 服务端按 BATCHED 对待 IMMEDIATE，
    /// 泵本体不消费）。
    pub delivery: u8,
}

/// 泵输出事件（`PointerEvent` 的驱动侧等价物——encode 单源经
/// `encode_pump_event` 转协议结构）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PumpEvent {
    pub phase: u8,
    pub device_id: u8,
    pub buttons: u8,
    pub flags: u8,
    pub pointer_id: u16,
    pub timestamp_ns: u64,
    pub x: u32,
    pub y: u32,
    pub pressure: u16,
    /// 历史采样幅（history 前 history_len 项有效，oldest→newest）。
    pub history_len: usize,
    pub history: [Sample; berry_guiproto::INPUT_SAMPLES_MAX - 1],
}

/// history 零值（队列初始化用——u64 ts 无「无效」哨兵，数组幅由
/// history_len 圈定有效面）。
const ZERO_SAMPLE: Sample = Sample { timestamp_ns: 0, x: 0, y: 0 };
const ZERO_EVENT: PumpEvent = PumpEvent {
    phase: 0,
    device_id: 0,
    buttons: 0,
    flags: 0,
    pointer_id: 0,
    timestamp_ns: 0,
    x: 0,
    y: 0,
    pressure: 0,
    history_len: 0,
    history: [ZERO_SAMPLE; INPUT_SAMPLES_MAX - 1],
};

/// 批内原始采样（raw 坐标 + ts；归一化在事件构装时一次完成）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawSample {
    ts_ns: u64,
    x: i32,
    y: i32,
}

/// 指针泵：RawInputEvent 流 → PumpEvent 流（§5.1 状态机）。
pub struct PointerPump {
    cfg: PumpConfig,
    // 批累积面（feed → SYN_REPORT 关批）
    samples: [RawSample; BATCH_CAP],
    n_samples: usize,
    /// 落单 X 的半对缓存（轴对采样模型——见模块头注）。
    pend_x: Option<i32>,
    /// 位置追踪（绝对域 last known——button-only 批的合成采样源）。
    last_x: i32,
    last_y: i32,
    // 手势状态
    contact: bool,
    buttons: u8,
    /// 当前手势 pointer_id（DOWN 时自 next_id 单调分配，u16 环绕）。
    cur_id: u16,
    next_id: u16,
    // 批内相变边序（close_batch 消费复位）：feed 只在「按 live buttons 位
    // 推进的接触态」实际翻转时记边——转移边严格交替自首边起（连续同向
    // 按压/释放并边），首边方向 + 计数即完整序列（QMP 单命令 down+up 的
    // tap 形态 = press+release 同批，配对不变量见 close_batch）。
    n_edges: u32,
    first_edge_press: bool,
    dropped_pending: bool,
    batch_ts: u64,
    batch_any: bool,
    // 输出队列（FIFO 环）
    queue: [PumpEvent; QUEUE_CAP],
    q_head: usize,
    q_len: usize,
    // 诊断计数（§5.1 忽略计数 + 截断/丢弃诚实面）
    ignored: u64,
    void_events: u64,
    truncated_samples: u64,
    dropped_events: u64,
}

impl PointerPump {
    /// 构造。**配置执法**（构造期 loud，非数据面）：校准域退化（max ≤ min
    /// 任一轴）或零几何 = 配置错误——`assert` panic（调用方 = main 的设备
    /// 自述装配，错误配置不该带病上岗）。
    pub fn new(cfg: PumpConfig) -> Self {
        assert!(cfg.cal.max_x > cfg.cal.min_x, "gui-inputd: X 校准域退化（max ≤ min）");
        assert!(cfg.cal.max_y > cfg.cal.min_y, "gui-inputd: Y 校准域退化（max ≤ min）");
        assert!(cfg.geom.w > 0 && cfg.geom.h > 0, "gui-inputd: 屏幕几何为零");
        PointerPump {
            cfg,
            samples: [RawSample { ts_ns: 0, x: 0, y: 0 }; BATCH_CAP],
            n_samples: 0,
            pend_x: None,
            last_x: cfg.cal.min_x,
            last_y: cfg.cal.min_y,
            contact: false,
            buttons: 0,
            cur_id: 0,
            next_id: 0,
            n_edges: 0,
            first_edge_press: false,
            dropped_pending: false,
            batch_ts: 0,
            batch_any: false,
            queue: [ZERO_EVENT; QUEUE_CAP],
            q_head: 0,
            q_len: 0,
            ignored: 0,
            void_events: 0,
            truncated_samples: 0,
            dropped_events: 0,
        }
    }

    /// 喂原始事件（一个 wire 事件一次调用）。
    pub fn feed(&mut self, ev: RawInputEvent) {
        if self.dropped_pending {
            // SYN_DROPPED 后窗口整体作废（Linux evdev 同型）：直到下个
            // SYN_REPORT 合成 CANCEL 收口重启；期间一切事件计 void。
            if ev.ev_type == EV_SYN && ev.code == SYN_REPORT {
                self.close_batch(ev.ts_ns);
            } else if !(ev.ev_type == EV_SYN && ev.code == SYN_DROPPED) {
                self.void_events += 1;
            }
            return;
        }
        match (ev.ev_type, ev.code) {
            (EV_SYN, SYN_REPORT) => {
                // 落单 X 补配当前 Y（轴对模型的关批冲刷）——ts 取 X 事件
                // 自身（batch_ts 即批内最后消费事件）
                if let Some(px) = self.pend_x.take() {
                    let ts = if self.batch_any { self.batch_ts } else { ev.ts_ns };
                    self.push_sample(ts, px, self.last_y);
                }
                self.close_batch(ev.ts_ns);
            }
            (EV_SYN, SYN_DROPPED) => {
                self.dropped_pending = true;
            }
            (EV_KEY, BTN_LEFT) | (EV_KEY, BTN_TOUCH) => {
                // 接触判据两路等价（§5.1）；BTN_TOUCH 映射 BUTTON_L 位。
                // 相边按事件序记录（接触态 = live buttons 位——转移才记
                // 边，同向重复并边：批内边序保真，press+release 同批可配对）。
                let in_contact = self.buttons & BUTTON_L != 0;
                match ev.value {
                    1 => {
                        if !in_contact {
                            self.record_edge(true);
                        }
                        self.buttons |= BUTTON_L;
                    }
                    0 => {
                        if in_contact {
                            self.record_edge(false);
                        }
                        self.buttons &= !BUTTON_L;
                    }
                    _ => self.ignored += 1,
                }
                self.note_ts(ev.ts_ns);
            }
            (EV_KEY, BTN_RIGHT) => {
                match ev.value {
                    1 => self.buttons |= BUTTON_R,
                    0 => self.buttons &= !BUTTON_R,
                    _ => self.ignored += 1,
                }
                self.note_ts(ev.ts_ns);
            }
            (EV_KEY, BTN_MIDDLE) => {
                match ev.value {
                    1 => self.buttons |= BUTTON_M,
                    0 => self.buttons &= !BUTTON_M,
                    _ => self.ignored += 1,
                }
                self.note_ts(ev.ts_ns);
            }
            (EV_ABS, ABS_X) => {
                self.last_x = ev.value;
                self.pend_x = Some(ev.value);
                self.note_ts(ev.ts_ns);
            }
            (EV_ABS, ABS_Y) => {
                self.last_y = ev.value;
                let x = self.pend_x.take().unwrap_or(self.last_x);
                self.push_sample(ev.ts_ns, x, ev.value);
                self.note_ts(ev.ts_ns);
            }
            // EV_REL / EV_MSC / ABS_MT_* / 未知 code：v1 忽略计数（§5.1）
            _ => self.ignored += 1,
        }
    }

    /// 待取事件数（EV_SYN 已关批的完整事件）。
    pub fn pending(&self) -> usize {
        self.q_len
    }

    /// 取走全部完整批：至多填满 `out`，返回写入数（剩余留队）。
    pub fn drain(&mut self, out: &mut [PumpEvent]) -> usize {
        let n = out.len().min(self.q_len);
        for i in 0..n {
            out[i] = self.queue[self.q_head];
            self.q_head = (self.q_head + 1) % QUEUE_CAP;
        }
        self.q_len -= n;
        n
    }

    // ---- 私有状态机 ----

    fn note_ts(&mut self, ts_ns: u64) {
        self.batch_ts = ts_ns;
        self.batch_any = true;
    }

    fn push_sample(&mut self, ts_ns: u64, x: i32, y: i32) {
        if self.n_samples == BATCH_CAP {
            // 事件风暴面：丢最旧保新（诚实截断 + 计数——常态溢出走
            // 「分批」不进这里）
            self.samples.copy_within(1..BATCH_CAP, 0);
            self.n_samples = BATCH_CAP - 1;
            self.truncated_samples += 1;
        }
        self.samples[self.n_samples] = RawSample { ts_ns, x, y };
        self.n_samples += 1;
    }

    /// 关批（EV_SYN SYN_REPORT）：相变仲裁 + 事件构装入队。
    fn close_batch(&mut self, close_ts: u64) {
        let dropped = self.dropped_pending;
        let edges = self.n_edges;
        let first_press = self.first_edge_press;
        let was_contact = self.contact;
        let k = self.n_samples; // 复位前捕获（借用工期内构装）
        // 批内最新有效 ts：采样尾 > 批内最后消费事件 > 关批 SYN 自身
        let synth_ts = if k > 0 {
            self.samples[k - 1].ts_ns
        } else if self.batch_any {
            self.batch_ts
        } else {
            close_ts
        };
        // 批局部状态复位（先行——构装读的是手势面而非批面；采样已在
        // `samples[..k]` 定格，复位数组头供下批使用）
        self.dropped_pending = false;
        self.n_edges = 0;
        self.first_edge_press = false;
        self.n_samples = 0;
        self.pend_x = None;
        self.batch_any = false;

        if dropped {
            // SYN_DROPPED：序列完整性丢失——在场手势作废，合成 CANCEL
            // （位置 = last known，采样弃置——窗口整体作废的关批半边）。
            // 不在场则无可作废（静默）。此后序列重启。
            if was_contact {
                self.emit_phase(PHASE_CANCEL, 0, synth_ts);
            }
            self.contact = false;
            // 作废窗口同时击穿接触位知识：窗口内被 void 的按压/释放均未
            // 落地 buttons——按位与 contact 对齐（否则 CANCEL 后 buttons
            // 残留 BUTTON_L，重按压被记为重复不产边 = 新手势静默）。
            self.buttons &= !BUTTON_L;
            return;
        }

        // 相变边回放：批内相边按事件序 emit——配对不变量「每条 DOWN 终以
        // UP/CANCEL 收口」在批内同样成立（QMP 单命令 down+up 即
        // press+release 同批的标准注入形态——QEMU 缓冲到 SYN_REPORT 一次
        // 投递，§4.1）。首条相边事件承批内采样（k），后续相边零采样合成
        // （位置 = last known，feed 已维持）；边严格交替自首边起（record_edge
        // 只记转移）。每条 DOWN 边单独分配 pointer_id（新手势各自成对）。
        let mut edge_is_press = first_press;
        let mut in_contact = was_contact;
        for i in 0..edges as usize {
            let kk = if i == 0 { k } else { 0 };
            if edge_is_press {
                // 新手势：pointer_id 单调分配（u16 环绕是显式语义）
                self.cur_id = self.next_id;
                self.next_id = self.next_id.wrapping_add(1);
                self.emit_phase(PHASE_DOWN, kk, synth_ts);
                in_contact = true;
            } else {
                self.emit_phase(PHASE_UP, kk, synth_ts);
                in_contact = false;
            }
            edge_is_press = !edge_is_press;
        }
        self.contact = in_contact;
        if edges == 0 && self.contact && k > 0 {
            self.emit_phase(PHASE_MOVE, k, synth_ts);
        }
        // 其余形态（悬停批 / 无相变批 / 离席后杂散 release / 空批）：无
        // 事件，采样弃置——位置追踪（last_x/last_y）已在 feed 维持
        // （离席后杂散 release 在 record_edge 处已并边为零边）。
    }

    /// 记批内相变边（转移边——同向重复由调用面并边不进此）。
    fn record_edge(&mut self, press: bool) {
        if self.n_edges == 0 {
            self.first_edge_press = press;
        }
        self.n_edges += 1;
    }

    /// 相位构装：k 采样按 INPUT_SAMPLES_MAX 分批（§5.1 溢出分批——零丢失）。
    /// 单批全相；DOWN 首批承相、后续 MOVE；UP/CANCEL 末批承相、先前 MOVE。
    fn emit_phase(&mut self, phase: u8, k: usize, synth_ts: u64) {
        if k == 0 {
            let ev = self.synth_event(phase, synth_ts);
            self.enqueue(ev);
            return;
        }
        let total_chunks = (k + INPUT_SAMPLES_MAX - 1) / INPUT_SAMPLES_MAX;
        let mut off = 0;
        let mut chunk_no = 0;
        while off < k {
            let m = core::cmp::min(INPUT_SAMPLES_MAX, k - off);
            let is_first = chunk_no == 0;
            let is_last = off + m == k;
            let ph = if total_chunks == 1 {
                phase
            } else if phase == PHASE_DOWN {
                if is_first { PHASE_DOWN } else { PHASE_MOVE }
            } else if phase == PHASE_UP || phase == PHASE_CANCEL {
                if is_last { phase } else { PHASE_MOVE }
            } else {
                PHASE_MOVE
            };
            let ev = self.chunk_event(ph, &self.samples[off..off + m]);
            self.enqueue(ev);
            off += m;
            chunk_no += 1;
        }
    }

    /// 采样片 → 事件（header = 片尾最新；history = 片前段 oldest→newest）。
    fn chunk_event(&self, phase: u8, chunk: &[RawSample]) -> PumpEvent {
        let m = chunk.len();
        debug_assert!(m >= 1 && m <= INPUT_SAMPLES_MAX);
        let header = self.norm_sample(&chunk[m - 1]);
        let mut history = [ZERO_SAMPLE; INPUT_SAMPLES_MAX - 1];
        for i in 0..m - 1 {
            history[i] = self.norm_sample(&chunk[i]);
        }
        let flags = if phase == PHASE_CANCEL { PE_FL_SYNTHESIZED } else { 0 };
        PumpEvent {
            phase,
            device_id: 0,
            buttons: self.buttons,
            flags,
            pointer_id: self.cur_id,
            timestamp_ns: header.timestamp_ns,
            x: header.x,
            y: header.y,
            pressure: PRESSURE_UNAVAILABLE,
            history_len: m - 1,
            history,
        }
    }

    /// 无采样批的合成事件（位置 = last known——button-only 批/CANCEL）。
    fn synth_event(&self, phase: u8, ts_ns: u64) -> PumpEvent {
        let flags = if phase == PHASE_CANCEL { PE_FL_SYNTHESIZED } else { 0 };
        PumpEvent {
            phase,
            device_id: 0,
            buttons: self.buttons,
            flags,
            pointer_id: self.cur_id,
            timestamp_ns: ts_ns,
            x: self.norm_x(self.last_x),
            y: self.norm_y(self.last_y),
            pressure: PRESSURE_UNAVAILABLE,
            history_len: 0,
            history: [ZERO_SAMPLE; INPUT_SAMPLES_MAX - 1],
        }
    }

    fn norm_x(&self, v: i32) -> u32 {
        normalize_axis(v, self.cfg.cal.min_x, self.cfg.cal.max_x, self.cfg.geom.w)
            .unwrap_or(0) // new() 已 assert 域合法——防御性回退，零 panic
    }

    fn norm_y(&self, v: i32) -> u32 {
        normalize_axis(v, self.cfg.cal.min_y, self.cfg.cal.max_y, self.cfg.geom.h).unwrap_or(0)
    }

    fn norm_sample(&self, rs: &RawSample) -> Sample {
        Sample { timestamp_ns: rs.ts_ns, x: self.norm_x(rs.x), y: self.norm_y(rs.y) }
    }

    fn enqueue(&mut self, ev: PumpEvent) {
        if self.q_len == QUEUE_CAP {
            self.dropped_events += 1; // 丢新 + 计数（消费方缺席的防御面）
            return;
        }
        self.queue[(self.q_head + self.q_len) % QUEUE_CAP] = ev;
        self.q_len += 1;
    }
}

/// 泵事件 → 协议 wire（编码单源：转 `PointerEvent` 后走 berry-guiproto
/// `encode_pointer_event`——phase/flags/buttons 值域校验在那侧，坏域 None）。
pub fn encode_pump_event(out: &mut [u8], ev: &PumpEvent) -> Option<usize> {
    if ev.history_len > INPUT_SAMPLES_MAX - 1 {
        return None;
    }
    let mut hist = [ZERO_SAMPLE; INPUT_SAMPLES_MAX - 1];
    hist[..ev.history_len].copy_from_slice(&ev.history[..ev.history_len]);
    let pe = PointerEvent {
        phase: ev.phase,
        device_id: ev.device_id,
        buttons: ev.buttons,
        flags: ev.flags,
        pointer_id: ev.pointer_id,
        timestamp_ns: ev.timestamp_ns,
        x: ev.x,
        y: ev.y,
        pressure: ev.pressure,
        contact_major: 0,
        contact_minor: 0,
        history: hist,
        history_len: ev.history_len,
    };
    encode_pointer_event(out, &pe)
}

// ---- host 单测（§8.3——FakeSource 喂确定性序列 + 状态谓词断言）----

#[cfg(test)]
mod tests {
    use super::*;
    use berry_guiproto::{parse_pointer_event, DELIVERY_BATCHED};

    const MS: u64 = 1_000_000;

    /// 事件速记。
    fn ev(ts_ms: u64, ev_type: u16, code: u16, value: i32) -> RawInputEvent {
        RawInputEvent { ts_ns: ts_ms * MS, ev_type, code, value }
    }

    /// SYN_REPORT 速记。
    fn syn(ts_ms: u64) -> RawInputEvent {
        ev(ts_ms, EV_SYN, SYN_REPORT, 0)
    }

    /// X/Y 轴对速记（ts 递增 1ms——轴对采样模型：一对 = 一个采样）。
    fn xy(ts_ms: u64, x: i32, y: i32) -> [RawInputEvent; 2] {
        [ev(ts_ms, EV_ABS, ABS_X, x), ev(ts_ms + 1, EV_ABS, ABS_Y, y)]
    }

    /// 泵测几何：域 {0,1000} → 屏 {800,600}（心算友好：v*799//1000 /
    /// v*599//1000——500→399/299、250→199/149、100→79/59）。
    fn pump() -> PointerPump {
        PointerPump::new(PumpConfig {
            geom: ScreenGeom { w: 800, h: 600 },
            cal: AbsCal { min_x: 0, max_x: 1000, min_y: 0, max_y: 1000 },
            delivery: DELIVERY_BATCHED,
        })
    }

    /// drain 全部并断言恰 n 个。
    fn take(p: &mut PointerPump, n: usize) -> Vec<PumpEvent> {
        let mut out = [ZERO_EVENT; 8];
        let k = p.drain(&mut out);
        assert_eq!(k, n, "drain 数：期望 {n} 实得 {k}");
        out[..k].to_vec()
    }

    // ---- wire 适配 + FakeSource ----

    #[test]
    fn raw_from_wire_fields() {
        // 字段逐拷 + value u32→i32 转换点（REL 负值位型还原）
        let r = raw_from_wire(
            42,
            VirtioInputEvent { ev_type: EV_REL, code: REL_WHEEL_TEST, value: 0xffff_ffff },
        );
        assert_eq!(r.ts_ns, 42);
        assert_eq!(r.ev_type, EV_REL);
        assert_eq!(r.code, REL_WHEEL_TEST);
        assert_eq!(r.value, -1);
        let r = raw_from_wire(0, VirtioInputEvent { ev_type: EV_ABS, code: ABS_X, value: 0x7fff_ffff });
        assert_eq!(r.value, i32::MAX);
        let r = raw_from_wire(0, VirtioInputEvent { ev_type: 0, code: 0, value: 0x8000_0000 });
        assert_eq!(r.value, i32::MIN);
    }

    const REL_WHEEL_TEST: u16 = 0x08; // REL_WHEEL（勿引入生产常量进测面）
    use berry_virtio::input::EV_REL; // 仅测面用（生产 import 不收——防非测构建 unused 警告）

    #[test]
    fn fake_source_fifo() {
        let mut src = FakeSource::new();
        assert_eq!(src.len(), 0);
        assert!(src.pop().is_none());
        let e0 = ev(1, EV_SYN, SYN_REPORT, 0);
        let e1 = ev(2, EV_KEY, BTN_LEFT, 1);
        src.push(e0);
        src.push(e1);
        assert_eq!(src.len(), 2);
        // FIFO 序（喂序即弹序）
        assert_eq!(src.pop(), Some(e0));
        assert_eq!(src.pop(), Some(e1));
        assert!(src.pop().is_none());
        assert_eq!(src.len(), 0);
        // Default = new 等价
        assert_eq!(FakeSource::default().len(), 0);
        // 环回：灌满 64 再排空，序不乱
        let mut src = FakeSource::new();
        for i in 0..64 {
            src.push(ev(i, 0, 0, i as i32));
        }
        for i in 0..64 {
            assert_eq!(src.pop().map(|e| e.value), Some(i));
        }
    }

    // ---- normalize_axis ----

    #[test]
    fn normalize_axis_endpoints() {
        // 端点钉板：min→0、max→len-1（两端任意屏宽）
        assert_eq!(normalize_axis(0, 0, 32767, 800), Some(0));
        assert_eq!(normalize_axis(32767, 0, 32767, 800), Some(799));
        assert_eq!(normalize_axis(0, 0, 32767, 600), Some(0));
        assert_eq!(normalize_axis(32767, 0, 32767, 600), Some(599));
        // 非零下界的域（真机 ABS min 可非 0）
        assert_eq!(normalize_axis(100, 100, 200, 100), Some(0));
        assert_eq!(normalize_axis(200, 100, 200, 100), Some(99));
        assert_eq!(normalize_axis(150, 100, 200, 100), Some(49)); // 50*99//100
    }

    #[test]
    fn normalize_axis_monotonic() {
        // 全域扫描：非降 + 恒在 [0, len-1]（映射不回卷、不出界）
        let mut prev = 0u32;
        for v in 0..=32767i32 {
            let n = normalize_axis(v, 0, 32767, 800).unwrap();
            assert!(n >= prev && n < 800, "v={v} n={n} prev={prev}");
            prev = n;
        }
        assert_eq!(prev, 799);
    }

    #[test]
    fn normalize_axis_rejects_bad_cal() {
        // 域退化：max == min / max < min / 零屏宽 全拒 None
        assert_eq!(normalize_axis(50, 100, 100, 800), None);
        assert_eq!(normalize_axis(50, 200, 100, 800), None);
        assert_eq!(normalize_axis(50, 0, 1000, 0), None);
    }

    #[test]
    fn normalize_axis_clamps_out_of_range() {
        // 越域钳位（ABS fuzz 抖动出界的防御面）
        assert_eq!(normalize_axis(-5, 0, 1000, 800), Some(0));
        assert_eq!(normalize_axis(100000, 0, 1000, 800), Some(799));
    }

    #[test]
    fn normalize_axis_qemu_tablet_pin() {
        // QEMU virtio-tablet 事实对拍值抽样钉板（§8.3）：{0,32767}→{800,600}
        // ——手算期望独立于被测实现。
        // x: 16384*799//32767 = 13090816//32767 = 399
        assert_eq!(normalize_axis(16384, 0, 32767, 800), Some(399));
        // y: 16384*599//32767 = 9814016//32767 = 299
        assert_eq!(normalize_axis(16384, 0, 32767, 600), Some(299));
        // 对称采样点
        assert_eq!(normalize_axis(8192, 0, 32767, 800), Some(199));
        assert_eq!(normalize_axis(24576, 0, 32767, 800), Some(599));
        assert_eq!(normalize_axis(8192, 0, 32767, 600), Some(149));
        assert_eq!(normalize_axis(24576, 0, 32767, 600), Some(449));
    }

    // ---- resample_to_vsync ----

    #[test]
    fn resample_empty_or_single_none() {
        // 空 / 单采样：无插值依据
        let s = Sample { timestamp_ns: 0, x: 1, y: 2 };
        assert!(resample_to_vsync(&[], 10 * MS).is_none());
        assert!(resample_to_vsync(&[s], 10 * MS).is_none());
    }

    #[test]
    fn resample_interpolation_identity() {
        // 两采样 8ms 跨度：端点恒等 + 中点插值钉板（手算：100+(300-100)/2）
        let s0 = Sample { timestamp_ns: 0, x: 100, y: 200 };
        let s1 = Sample { timestamp_ns: 8 * MS, x: 300, y: 400 };
        let ss = [s0, s1];
        // vsync = 首采样 → 恒等
        let r = resample_to_vsync(&ss, 0).unwrap();
        assert_eq!((r.x, r.y), (100, 200));
        assert_eq!(r.timestamp_ns, 0);
        // vsync = 尾采样 → 恒等
        let r = resample_to_vsync(&ss, 8 * MS).unwrap();
        assert_eq!((r.x, r.y), (300, 400));
        // 中点：ts 报 vsync、位置取半程
        let r = resample_to_vsync(&ss, 4 * MS).unwrap();
        assert_eq!(r.timestamp_ns, 4 * MS);
        assert_eq!((r.x, r.y), (200, 300));
        // 非 1/2 分点（1/4 程）
        let r = resample_to_vsync(&ss, 2 * MS).unwrap();
        assert_eq!((r.x, r.y), (150, 250));
        // 多采样括点定位：中段采样间的插值不走首尾外推
        let sm = Sample { timestamp_ns: 4 * MS, x: 500, y: 600 };
        let r = resample_to_vsync(&[s0, sm, s1], 6 * MS).unwrap();
        assert_eq!((r.x, r.y), (400, 500)); // (500,600)-(300,400) 的中点
    }

    #[test]
    fn resample_extrapolation_capped() {
        // s0=(0,0,0) s1=(8ms,800,1600)：斜率 100/ms、200/ms；帽 =
        // min(8ms/2, 8ms) = 4ms
        let s0 = Sample { timestamp_ns: 0, x: 0, y: 0 };
        let s1 = Sample { timestamp_ns: 8 * MS, x: 800, y: 1600 };
        let ss = [s0, s1];
        // 帽内外推（vsync = 尾+2ms）：全额外推 2ms
        let r = resample_to_vsync(&ss, 10 * MS).unwrap();
        assert_eq!(r.timestamp_ns, 10 * MS);
        assert_eq!((r.x, r.y), (1000, 2000));
        // 超帽（vsync = 尾+10ms > 尾+4ms 帽沿）：位置停在帽沿（保守——
        // 假定帽外无运动），ts 诚实报 vsync
        let r = resample_to_vsync(&ss, 18 * MS).unwrap();
        assert_eq!(r.timestamp_ns, 18 * MS);
        assert_eq!((r.x, r.y), (1200, 2400));
        // 帽沿恰达（vsync = 尾+4ms）
        let r = resample_to_vsync(&ss, 12 * MS).unwrap();
        assert_eq!((r.x, r.y), (1200, 2400));
    }

    #[test]
    fn resample_min_delta_and_stale_none() {
        let s0 = Sample { timestamp_ns: 0, x: 0, y: 0 };
        // Δ < 2ms（信息量不足）
        let s1 = Sample { timestamp_ns: MS, x: 100, y: 100 };
        assert!(resample_to_vsync(&[s0, s1], MS / 2).is_none());
        // Δ == 2ms 恰过（MIN_DELTA 是「小于才拒」的下开界）
        let s1 = Sample { timestamp_ns: 2 * MS, x: 100, y: 100 };
        assert!(resample_to_vsync(&[s0, s1], MS).is_some());
        // Δ > 20ms（太陈旧——整段手势信息可信度不足）
        let s1 = Sample { timestamp_ns: 21 * MS, x: 100, y: 100 };
        assert!(resample_to_vsync(&[s0, s1], 10 * MS).is_none());
        // 首尾同 ts
        let s1 = Sample { timestamp_ns: 0, x: 100, y: 100 };
        assert!(resample_to_vsync(&[s0, s1], 0).is_none());
        // 非单调输入（尾早于首）
        let sa = Sample { timestamp_ns: 8 * MS, x: 0, y: 0 };
        let sb = Sample { timestamp_ns: 0, x: 100, y: 100 };
        assert!(resample_to_vsync(&[sa, sb], 4 * MS).is_none());
        // vsync 早于首采样（后向外推不提供；首采样 ts 须 > 0 才构造得出
        // 「严格早于」——s0 自身 ts=0 时 vsync==first 属插值恒等命中）
        let t0 = Sample { timestamp_ns: 2 * MS, x: 0, y: 0 };
        let t1 = Sample { timestamp_ns: 8 * MS, x: 100, y: 100 };
        assert!(resample_to_vsync(&[t0, t1], 0).is_none());
        // vsync == 首采样：插值恒等命中（首位置原样返回）
        let s1 = Sample { timestamp_ns: 8 * MS, x: 100, y: 100 };
        let r = resample_to_vsync(&[s0, s1], 0).unwrap();
        assert_eq!((r.timestamp_ns, r.x, r.y), (0, 0, 0));
    }

    // ---- PointerPump ----

    #[test]
    fn pump_down_move_up_touch_path() {
        // BTN_TOUCH 路（QEMU virtio-tablet 形态）：按下批（含坐标）→
        // DOWN；移动批 → MOVE；释放批（button-only）→ UP（位置 = last
        // known，§5.1 无采样合成）。
        let mut p = pump();
        p.feed(ev(10, EV_KEY, BTN_TOUCH, 1));
        for e in xy(11, 100, 200) {
            p.feed(e);
        }
        p.feed(syn(13));
        assert_eq!(p.pending(), 1);
        let evs = take(&mut p, 1);
        assert_eq!(evs[0].phase, PHASE_DOWN);
        assert_eq!(evs[0].pointer_id, 0);
        assert_eq!((evs[0].x, evs[0].y), (79, 119)); // 100→79 / 200→119
        assert_eq!(evs[0].buttons, BUTTON_L);
        assert_eq!(evs[0].history_len, 0);
        assert_eq!(evs[0].timestamp_ns, 12 * MS); // 最新采样（Y）ts
        assert_eq!(evs[0].pressure, PRESSURE_UNAVAILABLE);

        for e in xy(20, 300, 400) {
            p.feed(e);
        }
        p.feed(syn(22));
        let evs = take(&mut p, 1);
        assert_eq!(evs[0].phase, PHASE_MOVE);
        assert_eq!(evs[0].pointer_id, 0); // 手势内稳定
        assert_eq!((evs[0].x, evs[0].y), (239, 239)); // 300→239 / 400→239
        assert_eq!(evs[0].buttons, BUTTON_L);

        p.feed(ev(30, EV_KEY, BTN_TOUCH, 0));
        p.feed(syn(31));
        let evs = take(&mut p, 1);
        assert_eq!(evs[0].phase, PHASE_UP);
        assert_eq!(evs[0].pointer_id, 0);
        assert_eq!((evs[0].x, evs[0].y), (239, 239)); // last known 位置
        assert_eq!(evs[0].buttons, 0); // 释放后接触位清零
        assert_eq!(evs[0].timestamp_ns, 30 * MS); // button 事件 ts
        assert_eq!(p.pending(), 0);
    }

    #[test]
    fn pump_down_move_up_btn_left_equivalence() {
        // BTN_LEFT 路（鼠标形态）与 BTN_TOUCH 路等价（§5.1 接触判据）；
        // 第二手势 pointer_id 单调递增。
        let mut p = pump();
        p.feed(ev(1, EV_KEY, BTN_LEFT, 1));
        for e in xy(2, 500, 500) {
            p.feed(e);
        }
        p.feed(syn(4));
        for e in xy(5, 250, 250) {
            p.feed(e);
        }
        p.feed(syn(7));
        p.feed(ev(8, EV_KEY, BTN_LEFT, 0));
        p.feed(syn(9));
        let mut out = [ZERO_EVENT; 8];
        let k = p.drain(&mut out);
        assert_eq!(k, 3);
        assert_eq!((out[0].phase, out[1].phase, out[2].phase), (PHASE_DOWN, PHASE_MOVE, PHASE_UP));
        assert_eq!((out[0].pointer_id, out[1].pointer_id, out[2].pointer_id), (0, 0, 0));
        assert_eq!(out[0].buttons, BUTTON_L);
        assert_eq!(out[2].buttons, 0);

        // 第二手势：pointer_id = 1（单调分配）
        p.feed(ev(20, EV_KEY, BTN_LEFT, 1));
        for e in xy(21, 100, 100) {
            p.feed(e);
        }
        p.feed(syn(23));
        p.feed(ev(24, EV_KEY, BTN_LEFT, 0));
        p.feed(syn(25));
        let k = p.drain(&mut out);
        assert_eq!(k, 2);
        assert_eq!((out[0].phase, out[0].pointer_id), (PHASE_DOWN, 1));
        assert_eq!((out[1].phase, out[1].pointer_id), (PHASE_UP, 1));
    }

    #[test]
    fn pump_same_batch_tap_pairs_down_up() {
        // QMP 单命令 tap 形态（§4.1：QEMU 缓冲到 SYN_REPORT 一次投递）：
        // press+release 同批 → DOWN+UP 完整配对（「每条 DOWN 终以 UP/CANCEL
        // 收口」在批内同样成立）；DOWN 承批内采样、UP 零采样合成；批终态回
        // 离席（后续悬停批零事件）；第二 tap = 新 pointer_id。
        let mut p = pump();
        p.feed(ev(10, EV_KEY, BTN_TOUCH, 1));
        for e in xy(11, 100, 200) {
            p.feed(e);
        }
        p.feed(ev(13, EV_KEY, BTN_TOUCH, 0));
        p.feed(syn(14));
        let evs = take(&mut p, 2);
        assert_eq!((evs[0].phase, evs[1].phase), (PHASE_DOWN, PHASE_UP));
        assert_eq!((evs[0].pointer_id, evs[1].pointer_id), (0, 0)); // 同手势
        assert_eq!((evs[0].x, evs[0].y), (79, 119)); // DOWN 承采样
        assert_eq!(evs[0].timestamp_ns, 12 * MS);
        assert_eq!(evs[1].history_len, 0); // UP 零采样合成
        assert_eq!((evs[1].x, evs[1].y), (79, 119)); // 位置 = last known
        assert_eq!((evs[0].buttons, evs[1].buttons), (0, 0)); // 批终态（既有口径）

        // 终态确已离席：悬停批零事件
        for e in xy(20, 500, 500) {
            p.feed(e);
        }
        p.feed(syn(22));
        assert_eq!(p.pending(), 0);

        // 第二 tap（纯 button 对）：新 pointer_id、同样配对
        p.feed(ev(30, EV_KEY, BTN_TOUCH, 1));
        p.feed(ev(31, EV_KEY, BTN_TOUCH, 0));
        p.feed(syn(32));
        let evs = take(&mut p, 2);
        assert_eq!((evs[0].phase, evs[1].phase), (PHASE_DOWN, PHASE_UP));
        assert_eq!((evs[0].pointer_id, evs[1].pointer_id), (1, 1));
        assert_eq!((evs[0].x, evs[0].y), (399, 299)); // last known = (500,500)
        assert_eq!(p.pending(), 0);
    }

    #[test]
    fn pump_same_batch_release_repress_continues() {
        // 接触中批内 release→re-press：UP 收旧手势 + 新 pointer_id 的 DOWN
        // 开新手势；随后移动批照常 MOVE（新按下不被吞——回归钉板：旧实现
        // 只 emit UP 且终态离席，后续移动批结构性静默）。
        let mut p = pump();
        p.feed(ev(1, EV_KEY, BTN_TOUCH, 1));
        for e in xy(2, 100, 100) {
            p.feed(e);
        }
        p.feed(syn(4));
        take(&mut p, 1); // DOWN id=0

        p.feed(ev(10, EV_KEY, BTN_TOUCH, 0));
        p.feed(ev(11, EV_KEY, BTN_TOUCH, 1));
        p.feed(syn(12));
        let evs = take(&mut p, 2);
        assert_eq!((evs[0].phase, evs[1].phase), (PHASE_UP, PHASE_DOWN));
        assert_eq!(evs[0].pointer_id, 0); // 旧手势收口
        assert_eq!((evs[0].x, evs[0].y), (79, 59)); // last known = (100,100)
        assert_eq!(evs[1].pointer_id, 1); // 新手势
        assert_eq!(evs[1].buttons, BUTTON_L); // 批终态 = 在席

        // 后续移动：新手势内 MOVE（不静默）
        for e in xy(20, 300, 300) {
            p.feed(e);
        }
        p.feed(syn(22));
        let evs = take(&mut p, 1);
        assert_eq!((evs[0].phase, evs[0].pointer_id), (PHASE_MOVE, 1));

        // 收尾干净
        p.feed(ev(30, EV_KEY, BTN_TOUCH, 0));
        p.feed(syn(31));
        assert_eq!(take(&mut p, 1)[0].phase, PHASE_UP);
        assert_eq!(p.pending(), 0);
    }

    #[test]
    fn pump_duplicate_press_coalesced() {
        // 同向重复按压并边（接触判据两路等价共享 BUTTON_L 位）：idle 起
        // [BTN_TOUCH=1, BTN_LEFT=1, BTN_TOUCH=0, SYN] 仍是单 DOWN+UP 对——
        // 重复 press 不产第二边（不分配第二 pointer_id）。
        let mut p = pump();
        p.feed(ev(1, EV_KEY, BTN_TOUCH, 1));
        p.feed(ev(2, EV_KEY, BTN_LEFT, 1));
        p.feed(ev(3, EV_KEY, BTN_TOUCH, 0));
        p.feed(syn(4));
        let evs = take(&mut p, 2);
        assert_eq!((evs[0].phase, evs[1].phase), (PHASE_DOWN, PHASE_UP));
        assert_eq!((evs[0].pointer_id, evs[1].pointer_id), (0, 0));
        assert_eq!(p.pending(), 0);
    }

    #[test]
    fn pump_multi_sample_history_order() {
        // 批内 2 采样对：header = 最新、history[0] = 最旧（oldest→newest，
        // Android 同序——§8.3）
        let mut p = pump();
        p.feed(ev(1, EV_KEY, BTN_TOUCH, 1));
        for e in xy(2, 100, 100) {
            p.feed(e);
        }
        for e in xy(4, 300, 300) {
            p.feed(e);
        }
        p.feed(syn(6));
        let evs = take(&mut p, 1);
        let e = &evs[0];
        assert_eq!(e.phase, PHASE_DOWN);
        assert_eq!(e.history_len, 1);
        assert_eq!((e.x, e.y), (239, 179)); // header = (300,300)
        assert_eq!(e.timestamp_ns, 5 * MS); // 最新采样 ts（第二对的 Y）
        // history 单项 = 最旧采样（100,100）@3ms（第一对的 Y ts）
        assert_eq!(e.history[0].timestamp_ns, 3 * MS);
        assert_eq!((e.history[0].x, e.history[0].y), (79, 59));
    }

    #[test]
    fn pump_syn_dropped_cancel() {
        // 接触中 SYN_DROPPED → 下批（SYN_REPORT）合成 CANCEL +
        // PE_FL_SYNTHESIZED；手势作废、后续重按 = 新 pointer_id。
        let mut p = pump();
        p.feed(ev(1, EV_KEY, BTN_TOUCH, 1));
        for e in xy(2, 500, 500) {
            p.feed(e);
        }
        p.feed(syn(4));
        take(&mut p, 1); // DOWN

        p.feed(ev(5, EV_SYN, SYN_DROPPED, 0));
        // 作废窗口内的事件（含坐标）整体 void——不更新位置、不产出事件
        for e in xy(6, 100, 100) {
            p.feed(e);
        }
        p.feed(ev(7, EV_KEY, BTN_TOUCH, 0)); // 窗口内的释放也 void
        assert_eq!(p.pending(), 0);
        p.feed(syn(8)); // 关批 → CANCEL
        let evs = take(&mut p, 1);
        assert_eq!(evs[0].phase, PHASE_CANCEL);
        assert_eq!(evs[0].flags, PE_FL_SYNTHESIZED);
        assert_eq!((evs[0].x, evs[0].y), (399, 299)); // last known = (500,500)
        assert_eq!(evs[0].pointer_id, 0);
        assert_eq!(p.void_events, 3); // xy 对 2 + 释放 1

        // 序列重启：重按 = 新手势（pointer_id 1）；正常收尾回离席态
        p.feed(ev(20, EV_KEY, BTN_TOUCH, 1));
        for e in xy(21, 250, 250) {
            p.feed(e);
        }
        p.feed(syn(23));
        let evs = take(&mut p, 1);
        assert_eq!((evs[0].phase, evs[0].pointer_id), (PHASE_DOWN, 1));
        p.feed(ev(25, EV_KEY, BTN_TOUCH, 0));
        p.feed(syn(26));
        assert_eq!(take(&mut p, 1)[0].phase, PHASE_UP);

        // 离席态的 SYN_DROPPED：无可作废 → 静默
        p.feed(ev(30, EV_SYN, SYN_DROPPED, 0));
        p.feed(syn(31));
        assert_eq!(p.pending(), 0);
    }

    #[test]
    fn pump_unknown_ignored_counted() {
        // 未知 ev_type/code：忽略但计数（§5.1——诊断面）；不产出事件
        let mut p = pump();
        p.feed(ev(1, 4, 0, 0)); // EV_MSC
        p.feed(ev(2, 2, 0x08, 1)); // EV_REL REL_WHEEL
        p.feed(ev(3, 3, 0x2f, 0)); // ABS_MT_SLOT（多点留位）
        p.feed(ev(4, 3, 0x35, 100)); // ABS_MT_POSITION_X
        p.feed(ev(5, 1, 0x1e, 1)); // EV_KEY KEY_A（未知按钮 code）
        p.feed(ev(6, 3, 2, 500)); // ABS_Z（未知轴）
        p.feed(syn(7));
        assert_eq!(p.pending(), 0);
        assert_eq!(p.ignored, 6);
        // 已知按钮的野 value（非 0/1）同样忽略计数
        p.feed(ev(8, EV_KEY, BTN_TOUCH, 7));
        p.feed(syn(9));
        assert_eq!(p.ignored, 7);
        assert_eq!(p.pending(), 0);
    }

    #[test]
    fn pump_hover_batch_no_event() {
        // 无接触的坐标批（悬停/杂散）：不产出事件，但位置追踪维持——
        // 随后 button-only 按下批的 DOWN 落在追踪位置。
        let mut p = pump();
        for e in xy(1, 500, 250) {
            p.feed(e);
        }
        p.feed(syn(3));
        assert_eq!(p.pending(), 0); // 无接触 → 无事件

        p.feed(ev(4, EV_KEY, BTN_TOUCH, 1));
        p.feed(syn(5)); // button-only 批 → DOWN @ last known
        let evs = take(&mut p, 1);
        assert_eq!(evs[0].phase, PHASE_DOWN);
        assert_eq!((evs[0].x, evs[0].y), (399, 149)); // (500,250) 归一化
        assert_eq!(evs[0].timestamp_ns, 4 * MS);
    }

    #[test]
    fn pump_overflow_split_single_batch() {
        // 单批 6 采样（> INPUT_SAMPLES_MAX=4）：拆 MOVE(4)+MOVE(2)——
        // 全部采样按序在场、零丢失（§5.1 溢出分批）。
        let mut p = pump();
        p.feed(ev(1, EV_KEY, BTN_TOUCH, 1));
        p.feed(syn(2));
        take(&mut p, 1); // DOWN（无坐标 → 位置 = 校准下界归一化 0）

        // 6 个采样对，X 坐标 100..=600（可读序）
        for i in 0..6 {
            let x = 100 * (i as i32 + 1);
            for e in xy(10 + i as u64 * 2, x, 500) {
                p.feed(e);
            }
        }
        p.feed(syn(30));
        let evs = take(&mut p, 2);
        assert_eq!((evs[0].phase, evs[1].phase), (PHASE_MOVE, PHASE_MOVE));
        assert_eq!(evs[0].history_len, 3); // 4 采样
        assert_eq!(evs[1].history_len, 1); // 2 采样
        // 零丢失：8 个采样值按 oldest→newest 逐一核（600 = header）
        let mut all_x = [0u32; 6];
        for i in 0..3 {
            all_x[i] = evs[0].history[i].x;
        }
        all_x[3] = evs[0].x;
        all_x[4] = evs[1].history[0].x;
        all_x[5] = evs[1].x;
        let expect = [79u32, 159, 239, 319, 399, 479]; // 100..600 → v*799//1000
        assert_eq!(all_x, expect);
        assert_eq!(evs[1].x, 479); // header = 最新采样
    }

    #[test]
    fn pump_overflow_split_across_two_syns() {
        // §8.3 字面形：5 采样跨两 SYN → 两 MOVE、采样零丢失。
        let mut p = pump();
        p.feed(ev(1, EV_KEY, BTN_TOUCH, 1));
        p.feed(syn(2));
        take(&mut p, 1);

        // 批 1：2 采样
        for i in 0..2 {
            for e in xy(10 + i as u64 * 2, 200 * (i as i32 + 1), 100) {
                p.feed(e);
            }
        }
        p.feed(syn(20));
        // 批 2：3 采样
        for i in 2..5 {
            for e in xy(30 + (i - 2) as u64 * 2, 200 * (i as i32 + 1), 100) {
                p.feed(e);
            }
        }
        p.feed(syn(40));
        let evs = take(&mut p, 2);
        assert_eq!((evs[0].phase, evs[1].phase), (PHASE_MOVE, PHASE_MOVE));
        assert_eq!(evs[0].history_len, 1); // 2 采样
        assert_eq!(evs[1].history_len, 2); // 3 采样
        assert_eq!(evs[0].x, 319); // 400 → 319
        assert_eq!(evs[1].x, 799); // 1000 → 799（600→479、800→639 history）
        assert_eq!(evs[1].history[0].x, 479);
        assert_eq!(evs[1].history[1].x, 639);
    }

    #[test]
    fn pump_pending_and_drain_bounds() {
        // drain 语义：至多填满 out；剩余留队；排空后恒 0。
        let mut p = pump();
        p.feed(ev(1, EV_KEY, BTN_TOUCH, 1));
        p.feed(syn(2));
        for i in 0..3 {
            for e in xy(10 + i as u64 * 2, 100 * (i as i32 + 1), 100) {
                p.feed(e);
            }
            p.feed(syn(20 + i as u64 * 10));
        }
        assert_eq!(p.pending(), 4); // DOWN + 3 MOVE
        let mut out = [ZERO_EVENT; 1];
        assert_eq!(p.drain(&mut out), 1); // 帽在 out.len()
        assert_eq!(out[0].phase, PHASE_DOWN);
        assert_eq!(p.pending(), 3);
        let mut out = [ZERO_EVENT; 8];
        assert_eq!(p.drain(&mut out), 3);
        assert_eq!(p.pending(), 0);
        assert_eq!(p.drain(&mut out), 0);
    }

    #[test]
    fn encode_pump_event_roundtrip() {
        // 泵事件 → wire → parse_pointer_event 对拍闭环（§8.3）：
        // 含 history（3 采样事件）与各域值。
        let mut p = pump();
        p.feed(ev(1, EV_KEY, BTN_LEFT, 1));
        for e in xy(2, 100, 200) {
            p.feed(e);
        }
        for e in xy(4, 300, 400) {
            p.feed(e);
        }
        for e in xy(6, 500, 600) {
            p.feed(e);
        }
        p.feed(syn(8));
        let evs = take(&mut p, 1);
        let src = &evs[0];
        assert_eq!(src.history_len, 2);

        let mut buf = [0u8; 80];
        let n = encode_pump_event(&mut buf, src).unwrap();
        assert_eq!(n, 32 + 2 * 16); // 头 32B + 2×16B
        let pe = parse_pointer_event(&buf[..n]).unwrap();
        assert_eq!(pe.phase, src.phase);
        assert_eq!(pe.buttons, src.buttons);
        assert_eq!(pe.flags, src.flags);
        assert_eq!(pe.pointer_id, src.pointer_id);
        assert_eq!(pe.timestamp_ns, src.timestamp_ns);
        assert_eq!((pe.x, pe.y), (src.x, src.y));
        assert_eq!(pe.pressure, src.pressure);
        assert_eq!(pe.history_len, src.history_len);
        for i in 0..src.history_len {
            assert_eq!(pe.history[i], src.history[i]);
        }
        // 坏相域 → None（值域校验在 berry-guiproto 编码器——单源）
        let mut bad = *src;
        bad.phase = 9;
        assert!(encode_pump_event(&mut buf, &bad).is_none());
        // history_len 越幅 → None（防御面——泵产出恒 ≤3）
        let mut bad = *src;
        bad.history_len = INPUT_SAMPLES_MAX;
        assert!(encode_pump_event(&mut buf, &bad).is_none());
    }
}
