//! gui-inputd lib —— GUI-4 段① I3 纯逻辑面：virtio-input wire 事件流 →
//! 归一化 → PointerEvent 泵（实施依据 = docs/GUI-4实施设计草案-v1.0.md §5.1/§6-I3）。
//!
//! 三段职责：
//! 1. **wire 适配**（`raw_from_wire`）：`VirtioInputEvent`（8B LE，value 线上
//!    u32）→ `RawInputEvent`（value 按 i32 解释——REL 轴语义可负）+ 驱动侧
//!    dequeue 时刻 ts_ns（规范无设备时间戳，Linux evdev 同款打点）。
//! 2. **归一化**（`normalize_axis`）：设备自述 ABS 域 `[cal_min, cal_max]` →
//!    屏幕像素 `[0, screen_len-1]`（u64 定点、端点钉板；域退化/零屏宽拒 None）。
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
    encode_pointer_event, PointerEvent, BUTTON_L, BUTTON_M, BUTTON_R, DEV_KEYBOARD,
    INPUT_SAMPLES_MAX, PE_FL_SYNTHESIZED, PHASE_CANCEL, PHASE_DOWN, PHASE_MOVE, PHASE_UP,
    PRESSURE_UNAVAILABLE,
};
use berry_virtio::input::{
    ABS_X, ABS_Y, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, BTN_TOUCH, EV_ABS, EV_KEY, EV_SYN,
    SYN_DROPPED, SYN_REPORT, KEY_0, KEY_1, KEY_9, KEY_A, KEY_APOSTROPHE, KEY_BACKSLASH,
    KEY_BACKSPACE, KEY_COMMA, KEY_DOT, KEY_DOWN, KEY_ENTER, KEY_EQUAL, KEY_ESC, KEY_GRAVE,
    KEY_KPENTER, KEY_L, KEY_LEFTALT, KEY_LEFTBRACE, KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_MINUS,
    KEY_P, KEY_Q, KEY_RIGHTBRACE, KEY_RIGHTCTRL, KEY_RIGHTSHIFT, KEY_SEMICOLON, KEY_SLASH,
    KEY_SPACE, KEY_TAB, KEY_UP, KEY_Z, KEY_M,
};

/// 批累积帽（采样数）：常态批 ≤ 数采样；32/批 = 事件风暴面——诚实截断
/// （丢最旧保新 + 计数），与「溢出分批」互补（分批兜 ≤ 帽的常态）。
const BATCH_CAP: usize = 32;
/// 完整事件队列帽：drain 消费节奏下的弹性容量；满则丢新 + 计数
/// （常驻输入源恒有消费方——POLL 阻塞语义兜底，帽只是防御面）。
/// 总攻批 32→128 扩容（input 路 P1 三重帽之一）：设备层收割面
/// （QSIZE=64×20ms tick）扩后，客户端应答-重挂间隙（1080P 软渲染帧窗/
/// 点击引发的同步 RPC 链停顿）的泵内积压深度须同档跟上——60Hz 指针流
/// 约 1 事件/报 ≈ 50 事件/s，128 槽 ≈ 2.5s 停顿容忍（原 32 槽 ≈ 0.6s）。
const QUEUE_CAP: usize = 128;

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

/// 绝对域 → 屏幕像素归一化：钳位后 u64 定点
/// `(v-cl_min)*(len-1)/(cl_max-cl_min)`——端点钉板（min→0、max→len-1），
/// 单调。拒绝 None：cal_max ≤ cal_min（域退化）/ screen_len == 0。
///
/// 中间量走 u64 而非 i64：钳位后三因子 `v-cl_min ≤ cal_max-cal_min ≤
/// 2^32-1`、`len-1 ≤ 2^32-2` 全在 u32 域，乘积上限 `(2^32-1)(2^32-2) <
/// 2^64` 恒不溢出。旧 i64 写法在极端三元组（i32::MIN/i32::MAX/u32::MAX）
/// 下乘积约 1.8e19 超 i64::MAX——debug panic、release 静默回绕；真实路径
/// screen_len 来自 u16 显示尺寸（乘积上限约 2.8e14）到不了，属防御面。
/// 结果与旧式逐位相同：三因子非负，u64 截断除 = floor = 旧 i64 除。
pub fn normalize_axis(value: i32, cal_min: i32, cal_max: i32, screen_len: u32) -> Option<u32> {
    if cal_max <= cal_min || screen_len == 0 {
        return None;
    }
    let v = value.clamp(cal_min, cal_max);
    let num = (v as i64 - cal_min as i64) as u64 * (screen_len as u64 - 1);
    let den = (cal_max as i64 - cal_min as i64) as u64;
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

    /// 更新屏幕几何（wmd 快照域真值；分辨率热切/首拍均走这里）。
    /// 零几何 = 配置错误，构造同口径 loud；接触中更新按新域归一后续事件，
    /// 历史 raw 采样保持旧域（v1 分辨率热切非同帧精确面，wmd 版本已换）。
    pub fn set_geom(&mut self, geom: ScreenGeom) {
        assert!(geom.w > 0 && geom.h > 0, "gui-inputd: 屏幕几何为零");
        self.cfg.geom = geom;
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

// ---- 键盘泵（与 PointerPump 平行不混道）----

/// 键出向队列帽（`QUEUE_CAP` 同纪律：drain 消费节奏下的弹性容量；满则
/// 丢新 + 计数——常驻输入源恒有消费方，帽只是防御面）。
const KEY_QUEUE_CAP: usize = 32;

/// 修正键位拼装：bit0=shift、bit1=ctrl、bit2=alt（字段整体读写；alt 位
/// v1 维护不消费——为 Alt 组合词汇预留）；bit3=CapsLock **锁存档位**（与
/// bit0-2 即时修饰不同——press 翻转、release 不清，泵内锁存的档位状态，
/// Linux 键盘 caps 语义）。
const MOD_SHIFT: u8 = 1 << 0;
const MOD_CTRL: u8 = 1 << 1;
const MOD_ALT: u8 = 1 << 2;
const MOD_CAPS: u8 = 1 << 3;

// ---- 键词汇扩展常量（总攻批 input 路 P2：导航七键 + CapsLock）----
// berry-virtio 常量面按「按需再冻」纪律未收这批（input.rs 自注
// 「KEY_CAPSLOCK=58 等不在表——按需再冻」）——该文件是本刀域外文件，
// 不越域补冻，消费侧私有镜像钉板（值域 = linux/input-event-codes.h
// 冻结值，host 测试对拍钉死；berry-virtio 后续收编时以此为准）。
/// CapsLock（泵内锁存档位——见 [`MOD_CAPS`]）。
const KEY_CAPSLOCK: u16 = 58;
/// 上翻页（出向 ESC 序列 `5~`）。
const KEY_PAGEUP: u16 = 75;
/// Home（出向 ESC 序列 `H`）。
const KEY_HOME: u16 = 102;
/// 左箭头（出向 ESC 序列 `D`）。
const KEY_LEFT: u16 = 105;
/// 右箭头（出向 ESC 序列 `C`）。
const KEY_RIGHT: u16 = 106;
/// End（出向 ESC 序列 `F`）。
const KEY_END: u16 = 107;
/// 下翻页（出向 ESC 序列 `6~`）。
const KEY_PAGEDOWN: u16 = 78;
/// Delete（出向 ESC 序列 `3~`）。
const KEY_DELETE: u16 = 111;

/// 键盘泵：RawInputEvent 流（Linux EV_KEY 码）→ 出向字节流（US 布局
/// 翻译、修正键感知）。与 [`PointerPump`] 平行不混道——键盘事件不进
/// 手势状态机，指针事件不进翻译表；两泵在 gui-inputd serve 面汇流出向
/// （serve_poll 混排，帧形见 [`key_frame`]）。
///
/// 词汇与语义：
/// - 修正键（SHIFT×2 / CTRL×2 / ALT）：value 1/0 置/清位不出向（两枚
///   物理 Shift 共享一个修正位、Ctrl 同理——不按键计数，双按住松一即清
///   ，v1 已知简化）；**CapsLock 是锁存档位非即时修饰**——press 翻转
///   `MOD_CAPS`、release 不清（泵内锁存；只作用于字母档——caps 在席时
///   字母选大写、caps+Shift 双在席回小写，US 布局档位反转语义；数字/
///   符号/控制码不受）；
/// - EV_KEY 主键 value=1：按表产 1..=4 个出向字节（普通键恰一个——
///   Ctrl+字母 → `c & 0x1F`、Shift 选双档上档字符、caps 见上；导航键
///   产 ESC 序列连发——UP/DOWN `A`/`B` + 本批扩七键 RIGHT/LEFT `C`/`D`、
///   HOME/END `H`/`F`、DELETE/PGUP/PGDN `3~`/`5~`/`6~`，出向帧形态不变
///   见 [`translate_seq`]，修正键不作用、Shift+导航键仍裸序列）；value=0
///   吞掉（无按住态追踪——重复字节靠设备重发不靠泵，CapsLock 的档位
///   翻转不受影响）；表外码 ignored 计数、value 非 0/1 同 ignored
///   （PointerPump §5.1 忽略计数同纪律）；
/// - EV_SYN(SYN_REPORT)：**消费不聚合**——键字节在按压时刻已入队，批界
///   对键无语义（与 PointerPump 的关批聚合相对）；其余 SYN code 落
///   ignored；
/// - 其余 ev_type（EV_ABS/EV_REL 等——键盘设备不该发）：ignored 计数。
///
/// [`post_char`] 直喂出向字节（DEV_TEXT 注入面）：绕过翻译表，字节原样
/// 入队，修正键态不影响——ASCII 恒原样（消灭旧 INJECT 把 ASCII 当 EV_KEY
/// code 的码域碰撞形态：'a'=97 撞 KEY_RIGHTCTRL）。
pub struct KeyPump {
    mods: u8,
    // 输出队列（FIFO 环——出向字节）
    queue: [u8; KEY_QUEUE_CAP],
    q_head: usize,
    q_len: usize,
    // 诊断计数（§5.1 忽略计数 + 丢弃诚实面）
    ignored: u64,
    dropped_bytes: u64,
}

impl KeyPump {
    pub fn new() -> Self {
        KeyPump {
            mods: 0,
            queue: [0; KEY_QUEUE_CAP],
            q_head: 0,
            q_len: 0,
            ignored: 0,
            dropped_bytes: 0,
        }
    }

    /// 喂原始事件（一个 wire 事件一次调用；语义见类型注）。
    pub fn feed(&mut self, ev: RawInputEvent) {
        match (ev.ev_type, ev.code) {
            (EV_KEY, KEY_LEFTSHIFT | KEY_RIGHTSHIFT) => self.set_mod(MOD_SHIFT, ev.value),
            (EV_KEY, KEY_LEFTCTRL | KEY_RIGHTCTRL) => self.set_mod(MOD_CTRL, ev.value),
            (EV_KEY, KEY_LEFTALT) => self.set_mod(MOD_ALT, ev.value),
            (EV_KEY, KEY_CAPSLOCK) => match ev.value {
                // 锁存档位：press 翻转、release 不清（Linux caps 语义——
                // 与即时修正键的置/清位不同面）；野值 ignored 同纪律。
                1 => self.mods ^= MOD_CAPS,
                0 => {}
                _ => self.ignored += 1,
            },
            (EV_KEY, code) => match ev.value {
                1 => {
                    // 先查导航序列表（UP/DOWN/RIGHT/LEFT/HOME/END/
                    // DELETE/PGUP/PGDN 产 3-4 字节逐帧入队），miss 回落
                    // 单字节翻译路径（普通键零漂移）。
                    if let Some(seq) = translate_seq(code) {
                        for &b in seq {
                            self.enqueue(b);
                        }
                    } else {
                        match translate_key(code, self.mods) {
                            Some(b) => self.enqueue(b),
                            None => self.ignored += 1,
                        }
                    }
                }
                0 => {} // 释放吞掉（无按住态追踪面）
                _ => self.ignored += 1,
            },
            (EV_SYN, SYN_REPORT) => {} // 批界消费不聚合（见类型注）
            _ => self.ignored += 1,
        }
    }

    /// 直喂出向字节（DEV_TEXT 注入面——翻译表旁路，见类型注）。
    pub fn post_char(&mut self, b: u8) {
        self.enqueue(b);
    }

    /// 待取字节数。
    pub fn pending(&self) -> usize {
        self.q_len
    }

    /// 取走 pending 出向字节：至多填满 `out`，返回写入数（剩余留队——
    /// serve_poll 批帽共享，溢出字节留队下轮）。
    pub fn drain(&mut self, out: &mut [u8]) -> usize {
        let n = out.len().min(self.q_len);
        for i in 0..n {
            out[i] = self.queue[self.q_head];
            self.q_head = (self.q_head + 1) % KEY_QUEUE_CAP;
        }
        self.q_len -= n;
        n
    }

    fn set_mod(&mut self, bit: u8, value: i32) {
        match value {
            1 => self.mods |= bit,
            0 => self.mods &= !bit,
            _ => self.ignored += 1,
        }
    }

    fn enqueue(&mut self, b: u8) {
        if self.q_len == KEY_QUEUE_CAP {
            self.dropped_bytes += 1; // 丢新 + 计数（QUEUE_CAP 同纪律）
            return;
        }
        self.queue[(self.q_head + self.q_len) % KEY_QUEUE_CAP] = b;
        self.q_len += 1;
    }
}

impl Default for KeyPump {
    fn default() -> Self {
        Self::new()
    }
}

/// QWERTY 首排小写档（`KEY_Q..=KEY_P` 下标序——Linux 码 QWERTY 序与
/// ASCII 不连号，查表不外推）。
const ROW_QWERTY: [u8; 10] = [b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p'];
/// 家排小写档（`KEY_A..=KEY_L`）。
const ROW_HOME: [u8; 9] = [b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l'];
/// 末排小写档（`KEY_Z..=KEY_M`）。
const ROW_BOTTOM: [u8; 7] = [b'z', b'x', b'c', b'v', b'b', b'n', b'm'];
/// 数字行上档（下标 0..8 = `KEY_1..=KEY_9`、下标 9 = `KEY_0`——US 布局
/// `!@#$%^&*()`）。
const SHIFT_DIGITS: [u8; 10] = [b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')'];

/// US 布局键翻译（修正键应用后；表外码 None——ignored 计数归调用面）。
/// Ctrl 只作用于字母（`c & 0x1F`，小写基、Shift 不改果）；Shift 选主键区
/// 双档上档；**CapsLock 只作用于字母档**——caps ^ shift 恰一在席选大写
/// （双在席回小写 = Shift 反转档位的 US 语义），数字/符号/控制码不受；
/// Alt 不参与（v1 无 Alt 组合词汇）。
fn translate_key(code: u16, mods: u8) -> Option<u8> {
    let shift = mods & MOD_SHIFT != 0;
    let ctrl = mods & MOD_CTRL != 0;
    let caps = mods & MOD_CAPS != 0;
    let letter = if (KEY_Q..=KEY_P).contains(&code) {
        Some(ROW_QWERTY[(code - KEY_Q) as usize])
    } else if (KEY_A..=KEY_L).contains(&code) {
        Some(ROW_HOME[(code - KEY_A) as usize])
    } else if (KEY_Z..=KEY_M).contains(&code) {
        Some(ROW_BOTTOM[(code - KEY_Z) as usize])
    } else {
        None
    };
    if let Some(c) = letter {
        // 恰一在席（caps 或 shift）选大写；ctrl 控制码小写基不受 caps。
        return Some(if ctrl { c & 0x1f } else if shift != caps { c - 32 } else { c });
    }
    match code {
        KEY_1..=KEY_9 => Some(if shift {
            SHIFT_DIGITS[(code - KEY_1) as usize]
        } else {
            b'1' + (code - KEY_1) as u8
        }),
        KEY_0 => Some(if shift { SHIFT_DIGITS[9] } else { b'0' }),
        // 控制键字节（US 终端惯例——Shift 无上档）。
        KEY_ESC => Some(0x1b),
        KEY_BACKSPACE => Some(0x08),
        KEY_TAB => Some(0x09),
        KEY_ENTER | KEY_KPENTER => Some(0x0a),
        KEY_SPACE => Some(b' '),
        // 符号键 US 双档（下档/上档）。
        KEY_MINUS => Some(if shift { b'_' } else { b'-' }),
        KEY_EQUAL => Some(if shift { b'+' } else { b'=' }),
        KEY_LEFTBRACE => Some(if shift { b'{' } else { b'[' }),
        KEY_RIGHTBRACE => Some(if shift { b'}' } else { b']' }),
        KEY_SEMICOLON => Some(if shift { b':' } else { b';' }),
        KEY_APOSTROPHE => Some(if shift { b'"' } else { b'\'' }),
        KEY_GRAVE => Some(if shift { b'~' } else { b'`' }),
        KEY_BACKSLASH => Some(if shift { b'|' } else { b'\\' }),
        KEY_COMMA => Some(if shift { b'<' } else { b',' }),
        KEY_DOT => Some(if shift { b'>' } else { b'.' }),
        KEY_SLASH => Some(if shift { b'?' } else { b'/' }),
        _ => None,
    }
}

/// 导航键出向序列表（xterm 同形 ESC 序列）：UP `A` / DOWN `B` / RIGHT
/// `C` / LEFT `D` / HOME `H` / END `F` / DELETE `3~` / PGUP `5~` /
/// PGDN `6~`。总攻批扩七键（原 UP/DOWN 两枚——彼时「bsh editor 非 A/B
/// finals 一律吞，出向无消费语义」的裁决反转：词汇先通、消费端 bsh
/// editor finals 集与 berry-term 扩集是兄弟路挂账，域外不动；当前键盘
/// 消费端整体未接线——总攻 input 路 P1 键盘死三连——无即时回归面）。
/// F1-F12 维持不冻——无消费面再冻（总攻同裁决）。「表外 None 不出向」
/// 哲学同 [`translate_key`]。修正键不作用：导航键无修饰形，Shift+UP
/// 仍裸序列（记面）。
///
/// 产 3-4 字节按序入队 = 逐帧单字节连发：drain/key_frame/serve_poll 零
/// 改动——字节队列 seam 既有「一帧一字节」形态直接承载（KEY_QUEUE_CAP=32
/// 容 4B/键 8 键无压力）。serve 面批帽共享（max_events ≤
/// INPUT_EVENTS_MAX=3）恰容一枚 3B 序列；4B 序列（DELETE/翻页）与指针
/// 帧混批时溢出字节留队下轮——延迟一拍不丢失。
fn translate_seq(code: u16) -> Option<&'static [u8]> {
    match code {
        KEY_UP => Some(&[0x1b, b'[', b'A']),
        KEY_DOWN => Some(&[0x1b, b'[', b'B']),
        KEY_RIGHT => Some(&[0x1b, b'[', b'C']),
        KEY_LEFT => Some(&[0x1b, b'[', b'D']),
        KEY_HOME => Some(&[0x1b, b'[', b'H']),
        KEY_END => Some(&[0x1b, b'[', b'F']),
        KEY_DELETE => Some(&[0x1b, b'[', b'3', b'~']),
        KEY_PAGEUP => Some(&[0x1b, b'[', b'5', b'~']),
        KEY_PAGEDOWN => Some(&[0x1b, b'[', b'6', b'~']),
        _ => None,
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

/// 键帧构装（serve_poll 出向混排单源）：键字节 → [`PumpEvent`] 键帧。
/// 覆载约定：`device_id = DEV_KEYBOARD`、**x = 翻译后字节**（y=0——键无
/// 坐标语义；窗位注记 wm_annotate 对键帧无意义，serve 面跳过）；其余
/// 字段恒定：phase=PHASE_DOWN、buttons/flags/pointer_id=0、
/// pressure=PRESSURE_UNAVAILABLE、零 history。全字段在 berry-guiproto
/// encode 值域内（host 单测钉 encode/parse 往返）；ts 取调用侧混排时刻。
pub fn key_frame(ts_ns: u64, byte: u8) -> PumpEvent {
    PumpEvent {
        phase: PHASE_DOWN,
        device_id: DEV_KEYBOARD,
        buttons: 0,
        flags: 0,
        pointer_id: 0,
        timestamp_ns: ts_ns,
        x: byte as u32,
        y: 0,
        pressure: PRESSURE_UNAVAILABLE,
        history_len: 0,
        history: [ZERO_SAMPLE; INPUT_SAMPLES_MAX - 1],
    }
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

    #[test]
    fn set_geom_updates_normalization_domain() {
        // wmd 快照域是几何真值：800×600 泵切 1080×1920 后，同一 ABS
        // 原始对必须按新域归一（2026-11-06 竖屏实抓回归：固定 800×600
        // 会把 (135,200) 映成 (99,62)，真实点击全偏）。
        let mut p = pump();
        p.set_geom(ScreenGeom { w: 1080, h: 1920 });
        p.feed(ev(1, EV_KEY, BTN_LEFT, 1));
        for e in xy(2, 500, 500) {
            p.feed(e);
        }
        p.feed(syn(4));
        let evs = take(&mut p, 1);
        assert_eq!(evs[0].phase, PHASE_DOWN);
        assert_eq!((evs[0].x, evs[0].y), (539, 959));
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

    /// 旧公式的无溢出参考实现（i128——仅测试侧；生产面用 u64 中间量，
    /// 理由见 [`normalize_axis`] 文档）。与旧 i64 写法逐字同形，只是乘除
    /// 域放宽到 i128。
    fn normalize_axis_ref(value: i32, cal_min: i32, cal_max: i32, screen_len: u32) -> Option<u32> {
        if cal_max <= cal_min || screen_len == 0 {
            return None;
        }
        let v = value.clamp(cal_min, cal_max);
        let num = (v as i128 - cal_min as i128) * (screen_len as i128 - 1);
        let den = cal_max as i128 - cal_min as i128;
        Some((num / den) as u32)
    }

    /// 测试侧简易 LCG（Numerical Recipes 64 位常数；取高 32 位——低位
    /// 周期短不取；零外部 crate）。
    struct Lcg(u64);

    impl Lcg {
        fn next_u32(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 32) as u32
        }

        fn next_i32(&mut self) -> i32 {
            self.next_u32() as i32
        }

        /// 约每 4 次抽 1 次边界值（提高边界密度），否则全随机。
        fn pick_i32(&mut self, edges: &[i32]) -> i32 {
            if self.next_u32() % 4 == 0 {
                edges[(self.next_u32() as usize) % edges.len()]
            } else {
                self.next_i32()
            }
        }
    }

    #[test]
    fn normalize_axis_extreme_triple_no_wrap() {
        // 极端三元组（cal_min=i32::MIN、cal_max=i32::MAX、screen_len=u32::MAX）
        // ——旧 i64 写法乘积 (2^32-1)(2^32-2) ≈ 1.8e19 超 i64::MAX：debug
        // panic「attempt to multiply with overflow」、release 静默回绕成
        // Some(4294967293)（修前实测）。u64 中间量下端点与中点逐位正确。
        // 手算：den = 2^32-1 = 4294967295、len-1 = 2^32-2 = 4294967294。
        const LO: i32 = i32::MIN;
        const HI: i32 = i32::MAX;
        const LEN: u32 = u32::MAX;
        // 端点钉板：min→0；max→len-1（A=den，den·(len-1)/den = len-1）
        assert_eq!(normalize_axis(LO, LO, HI, LEN), Some(0));
        assert_eq!(normalize_axis(HI, LO, HI, LEN), Some(4_294_967_294));
        // max-1：A·B = den·4294967293 + 1 → 4294967293（旧式回绕成 …292）
        assert_eq!(normalize_axis(HI - 1, LO, HI, LEN), Some(4_294_967_293));
        // 中点：域共 2^32 个值无单点中心，取相邻两值 -1/0
        //   v=-1：A=2^31-1，A·B = den·2147483646 + 2147483648 → 2147483646
        //   v= 0：A=2^31，  A·B = den·2147483647 + 2147483647 → 2147483647
        assert_eq!(normalize_axis(-1, LO, HI, LEN), Some(2_147_483_646));
        assert_eq!(normalize_axis(0, LO, HI, LEN), Some(2_147_483_647));
        // 结果恒 < len（u32 域内单调阶梯：抽查四点非降）
        let a = normalize_axis(-1, LO, HI, LEN).unwrap();
        let b = normalize_axis(0, LO, HI, LEN).unwrap();
        let c = normalize_axis(HI - 1, LO, HI, LEN).unwrap();
        let d = normalize_axis(HI, LO, HI, LEN).unwrap();
        assert!(a <= b && b <= c && c <= d && d < LEN);
        // 反证本测非空洞：旧 i64 乘法在该入参下确实回绕出 …293（修前值）
        let num = (HI as i64 - LO as i64).wrapping_mul(LEN as i64 - 1);
        let den = HI as i64 - LO as i64;
        assert_eq!((num / den) as u32, 4_294_967_293);
    }

    #[test]
    fn normalize_axis_equivalent_to_i128_reference() {
        // 等价性：生产 u64 实现 vs 旧公式 i128 参考实现，20 万组随机加边界
        // 入参（含 None 面：域退化 / 零屏宽）逐位相同。
        let mut rng = Lcg(0x5eed_1234_abcd_0001);
        let edges_i32: [i32; 9] =
            [i32::MIN, i32::MIN + 1, -32768, -1, 0, 1, 32767, i32::MAX - 1, i32::MAX];
        let edges_u32: [u32; 8] =
            [0, 1, 2, 0xffff, 0x1_0000, 0x7fff_ffff, u32::MAX - 1, u32::MAX];
        let mut n_none = 0u64;
        for _ in 0..200_000 {
            let value = rng.pick_i32(&edges_i32);
            let cal_min = rng.pick_i32(&edges_i32);
            let cal_max = rng.pick_i32(&edges_i32);
            // 屏宽：1/4 边界值、1/4 可达域（≤ 65536）、1/2 全随机
            let screen_len = match rng.next_u32() % 4 {
                0 => edges_u32[(rng.next_u32() as usize) % edges_u32.len()],
                1 => rng.next_u32() % 65537,
                _ => rng.next_u32(),
            };
            let got = normalize_axis(value, cal_min, cal_max, screen_len);
            let want = normalize_axis_ref(value, cal_min, cal_max, screen_len);
            assert_eq!(
                got, want,
                "value={value} cal_min={cal_min} cal_max={cal_max} screen_len={screen_len}"
            );
            if got.is_none() {
                n_none += 1;
            }
        }
        // 采样面自证：None 面确有命中（否则域退化分支没被随机覆盖到）
        assert!(n_none > 10_000, "None 样本 {n_none}——域退化分支覆盖不足");
    }

    #[test]
    fn normalize_axis_reachable_domain_spot_check() {
        // 可达域抽查：screen_len ≤ 65536（真实路径来自 u16 显示尺寸，乘积
        // 上限 (2^32-1)·65535 ≈ 2.8e14，本就不溢出——改前改后必须逐位同）
        // × 典型校准域：对拍 i128 参考 + 界内 + 端点钉板 + 全域单调。
        let lens: [u32; 10] = [1, 2, 480, 800, 600, 1080, 1920, 2160, 65535, 65536];
        let doms: [(i32, i32); 5] =
            [(0, 32767), (0, 4095), (-32768, 32767), (i32::MIN, i32::MAX), (100, 200)];
        let mut rng = Lcg(0x5eed_1234_abcd_0002);
        for &len in &lens {
            for &(lo, hi) in &doms {
                // 端点钉板（越域钳位仅在域端还有 i32 余量时断言——
                // i32::MIN-1 / i32::MAX+1 不是合法 i32 入参）
                assert_eq!(normalize_axis(lo, lo, hi, len), Some(0));
                assert_eq!(normalize_axis(hi, lo, hi, len), Some(len - 1));
                if lo > i32::MIN {
                    assert_eq!(normalize_axis(lo - 1, lo, hi, len), Some(0));
                }
                if hi < i32::MAX {
                    assert_eq!(normalize_axis(hi + 1, lo, hi, len), Some(len - 1));
                }
                // 随机 + 域内边界值对拍，结果恒在 [0, len-1]
                let edge = [lo, hi, lo + 1, hi - 1, 0, -1, 1, lo / 2 + hi / 2];
                for _ in 0..2000 {
                    let v = rng.pick_i32(&edge);
                    let r = normalize_axis(v, lo, hi, len).unwrap();
                    assert_eq!(r, normalize_axis_ref(v, lo, hi, len).unwrap());
                    assert!(r < len, "v={v} [{lo},{hi}] len={len} → {r}");
                }
            }
            // 全域单调扫描（{0,32767} 域）：非降、恒 < len、尾值 = len-1
            let mut prev = 0u32;
            for v in 0..=32767i32 {
                let n = normalize_axis(v, 0, 32767, len).unwrap();
                assert!(n >= prev && n < len, "len={len} v={v} n={n} prev={prev}");
                prev = n;
            }
            assert_eq!(prev, len - 1);
        }
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
    fn pump_queue_cap_drop_new() {
        // 事件队帽 128（总攻批 32→128 扩容钉板——input 路 P1 三重帽之一）：
        // 满则丢新 + 计数（消费方缺席的防御面——应答-重挂间隙的泵内积压
        // 深度）；FIFO 保前 128。65 个 tap 批（press+release 同批 = DOWN+UP
        // 两事件）= 130 事件 → 持前 128、丢尾 2。
        assert_eq!(QUEUE_CAP, 128);
        let mut p = pump();
        for i in 0..65u64 {
            p.feed(ev(i * 3 + 1, EV_KEY, BTN_TOUCH, 1));
            p.feed(ev(i * 3 + 2, EV_KEY, BTN_TOUCH, 0));
            p.feed(syn(i * 3 + 3));
        }
        assert_eq!(p.pending(), QUEUE_CAP);
        assert_eq!(p.dropped_events, 2);
        let mut out = [ZERO_EVENT; QUEUE_CAP];
        assert_eq!(p.drain(&mut out), QUEUE_CAP);
        // FIFO + 丢新：队首 = 首批 DOWN，队尾 = 第 64 批 UP（id 自 0 起）。
        assert_eq!(out[0].phase, PHASE_DOWN);
        assert_eq!(out[QUEUE_CAP - 1].phase, PHASE_UP);
        assert_eq!(out[QUEUE_CAP - 1].pointer_id, 63);
        assert_eq!(p.pending(), 0);
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

    // ---- KeyPump（键盘泵——与 PointerPump 平行面；KEY_A 进 PointerPump
    // 依旧 ignored 的钉板见上 pump_unknown_ignored_counted，两泵不混道）----

    use berry_virtio::input::{KEY_B, KEY_H, KEY_N, KEY_W}; // 仅测面用（生产 import 不收——防非测构建 unused 警告）

    /// 键事件速记（ts 恒定——键泵不消费 ts，帧 ts 在 serve 混排侧打点）。
    fn kev(code: u16, value: i32) -> RawInputEvent {
        RawInputEvent { ts_ns: MS, ev_type: EV_KEY, code, value }
    }

    /// 键 drain 全部并断言字节序列。
    fn take_k(p: &mut KeyPump, want: &[u8]) {
        let mut out = [0u8; KEY_QUEUE_CAP];
        let k = p.drain(&mut out);
        assert_eq!(k, want.len(), "键 drain 数：期望 {} 实得 {}", want.len(), k);
        assert_eq!(&out[..k], want);
    }

    #[test]
    fn keypump_letter_rows_and_shift_combo() {
        // 三排小写档（排界抽查：Q/P、A/L、Z/M + 排内中段 W/H/N）+ Shift
        // 大写档；释放事件（修正键与主键）全吞掉不出向。
        let mut p = KeyPump::new();
        p.feed(kev(KEY_Q, 1));
        p.feed(kev(KEY_P, 1));
        p.feed(kev(KEY_A, 1));
        p.feed(kev(KEY_L, 1));
        p.feed(kev(KEY_Z, 1));
        p.feed(kev(KEY_M, 1));
        p.feed(syn(9)); // 批界消费不聚合——不产不吞
        take_k(&mut p, b"qpalzm");
        assert_eq!(p.ignored, 0);

        p.feed(kev(KEY_LEFTSHIFT, 1));
        p.feed(kev(KEY_W, 1));
        p.feed(kev(KEY_H, 1));
        p.feed(kev(KEY_N, 1));
        take_k(&mut p, b"WHN");
        p.feed(kev(KEY_LEFTSHIFT, 0));
        p.feed(kev(KEY_W, 1));
        take_k(&mut p, b"w");

        // 释放吞掉：主键释放 / 修正键置清全零字节。
        p.feed(kev(KEY_W, 0));
        p.feed(kev(KEY_RIGHTSHIFT, 1));
        p.feed(kev(KEY_RIGHTSHIFT, 0));
        assert_eq!(p.pending(), 0);
    }

    #[test]
    fn keypump_digit_row_and_symbol_pairs() {
        // 数字行 1234567890 / Shift !@#$%^&*()；符号行 US 双档全表
        //（下档、上档各过一遍）。
        let mut p = KeyPump::new();
        for code in KEY_1..=KEY_9 {
            p.feed(kev(code, 1));
        }
        p.feed(kev(KEY_0, 1));
        take_k(&mut p, b"1234567890");

        p.feed(kev(KEY_LEFTSHIFT, 1));
        for code in KEY_1..=KEY_9 {
            p.feed(kev(code, 1));
        }
        p.feed(kev(KEY_0, 1));
        take_k(&mut p, b"!@#$%^&*()");
        p.feed(kev(KEY_LEFTSHIFT, 0));

        p.feed(kev(KEY_MINUS, 1));
        p.feed(kev(KEY_EQUAL, 1));
        p.feed(kev(KEY_LEFTBRACE, 1));
        p.feed(kev(KEY_RIGHTBRACE, 1));
        p.feed(kev(KEY_SEMICOLON, 1));
        p.feed(kev(KEY_APOSTROPHE, 1));
        p.feed(kev(KEY_GRAVE, 1));
        p.feed(kev(KEY_BACKSLASH, 1));
        p.feed(kev(KEY_COMMA, 1));
        p.feed(kev(KEY_DOT, 1));
        p.feed(kev(KEY_SLASH, 1));
        take_k(&mut p, b"-=[];'`\\,./");

        p.feed(kev(KEY_LEFTSHIFT, 1));
        p.feed(kev(KEY_MINUS, 1));
        p.feed(kev(KEY_EQUAL, 1));
        p.feed(kev(KEY_LEFTBRACE, 1));
        p.feed(kev(KEY_RIGHTBRACE, 1));
        p.feed(kev(KEY_SEMICOLON, 1));
        p.feed(kev(KEY_APOSTROPHE, 1));
        p.feed(kev(KEY_GRAVE, 1));
        p.feed(kev(KEY_BACKSLASH, 1));
        p.feed(kev(KEY_COMMA, 1));
        p.feed(kev(KEY_DOT, 1));
        p.feed(kev(KEY_SLASH, 1));
        take_k(&mut p, b"_+{}:\"~|<>?");
    }

    #[test]
    fn keypump_control_keys() {
        // 控制键字节：Enter/KPEnter→0x0A、Backspace→0x08、Tab→0x09、
        // Esc→0x1B、Space→0x20（Shift 无上档）。
        let mut p = KeyPump::new();
        p.feed(kev(KEY_ENTER, 1));
        p.feed(kev(KEY_KPENTER, 1));
        p.feed(kev(KEY_BACKSPACE, 1));
        p.feed(kev(KEY_TAB, 1));
        p.feed(kev(KEY_ESC, 1));
        p.feed(kev(KEY_SPACE, 1));
        p.feed(kev(KEY_LEFTSHIFT, 1));
        p.feed(kev(KEY_SPACE, 1)); // Shift+Space 仍空格
        take_k(&mut p, &[0x0a, 0x0a, 0x08, 0x09, 0x1b, 0x20, 0x20]);
    }

    #[test]
    fn keypump_ctrl_letter_control_codes() {
        // Ctrl+字母 → c&0x1F（小写基）；Ctrl+Shift 不改果；Ctrl 松开回
        // 普通档；Ctrl 对非字母不生效。
        let mut p = KeyPump::new();
        p.feed(kev(KEY_LEFTCTRL, 1));
        p.feed(kev(KEY_A, 1));
        p.feed(kev(KEY_Z, 1));
        take_k(&mut p, &[0x01, 0x1a]);
        p.feed(kev(KEY_LEFTSHIFT, 1));
        p.feed(kev(KEY_A, 1)); // Ctrl+Shift+A = 同一控制码
        take_k(&mut p, &[0x01]);
        p.feed(kev(KEY_LEFTCTRL, 0));
        p.feed(kev(KEY_A, 1)); // Ctrl 已松、Shift 在 → 大写
        take_k(&mut p, b"A");
        p.feed(kev(KEY_LEFTCTRL, 1));
        p.feed(kev(KEY_SPACE, 1)); // 非字母不吃 Ctrl
        take_k(&mut p, b" ");
    }

    #[test]
    fn keypump_capslock_latch() {
        // CapsLock 泵内锁存档位（总攻批 P2——死键修面）：press 翻转、
        // release 不清（Linux caps 档位语义，非即时修饰）；只作用字母档
        // ——caps ^ shift 恰一在席选大写、双在席回小写（Shift 反转档位的
        // US 语义）；数字/符号/控制码不受；再 press 翻回。冻结值对拍
        // linux/input-event-codes.h（常量镜像钉板在 lib 本侧）。
        assert_eq!(KEY_CAPSLOCK, 58);
        let mut p = KeyPump::new();
        p.feed(kev(KEY_CAPSLOCK, 1));
        p.feed(kev(KEY_CAPSLOCK, 0)); // release 不清档位
        p.feed(kev(KEY_A, 1));
        take_k(&mut p, b"A");
        // caps+Shift 双在席 → 小写（反转）
        p.feed(kev(KEY_LEFTSHIFT, 1));
        p.feed(kev(KEY_A, 1));
        take_k(&mut p, b"a");
        p.feed(kev(KEY_LEFTSHIFT, 0));
        // 数字/符号/控制码不吃 caps
        p.feed(kev(KEY_1, 1));
        p.feed(kev(KEY_MINUS, 1));
        p.feed(kev(KEY_ENTER, 1));
        take_k(&mut p, &[b'1', b'-', 0x0a]);
        // 再 press 翻回小写档
        p.feed(kev(KEY_CAPSLOCK, 1));
        p.feed(kev(KEY_A, 1));
        take_k(&mut p, b"a");
        assert_eq!(p.ignored, 0);
        // 野 value 不动档位、进 ignored（修正键同纪律）
        let mut p2 = KeyPump::new();
        p2.feed(kev(KEY_CAPSLOCK, 2));
        p2.feed(kev(KEY_A, 1));
        take_k(&mut p2, b"a");
        assert_eq!(p2.ignored, 1);
    }

    #[test]
    fn keypump_unknown_and_wild_values_ignored() {
        // 表外码按压 ignored；表外码释放吞掉；野 value（2=autorepeat）
        // ignored——修正键野 value 不动位；非键盘事件面全 ignored。
        let mut p = KeyPump::new();
        p.feed(kev(0x200, 1));
        assert_eq!(p.pending(), 0);
        assert_eq!(p.ignored, 1);
        p.feed(kev(0x200, 0));
        assert_eq!(p.ignored, 1);
        p.feed(kev(KEY_A, 2));
        assert_eq!(p.ignored, 2);
        p.feed(kev(KEY_LEFTSHIFT, 2));
        p.feed(kev(KEY_A, 1));
        take_k(&mut p, b"a");
        p.feed(ev(9, EV_ABS, ABS_X, 100));
        p.feed(ev(10, EV_SYN, SYN_DROPPED, 0));
        // 非 REPORT 的 SYN 码（DROPPED 等）不在消费面——同 unknown 计
        // ignored（吞而不产字节，但进诊断计数）。
        assert_eq!(p.ignored, 5);
    }

    #[test]
    fn keypump_post_char_passthrough() {
        // post_char 直通：绕过翻译表、修正键态不影响、字节原样（含非
        // ASCII 值域 0xff）。
        let mut p = KeyPump::new();
        p.feed(kev(KEY_LEFTSHIFT, 1));
        p.post_char(b'a');
        p.post_char(b'A');
        p.post_char(0xff);
        take_k(&mut p, &[b'a', b'A', 0xff]);
    }

    #[test]
    fn keypump_code_domain_collision_pin() {
        // 码域碰撞钉板：KEY_RIGHTCTRL=97 与 ASCII 'a' 同值——右 Ctrl 是
        // 独立修正键，按下（含完整 press+SYN 批形态）不得译出 'a'（旧
        // DEV_TEXT 把 ASCII 直当 EV_KEY code 的同族风险形态）。
        let mut p = KeyPump::new();
        assert_eq!(KEY_RIGHTCTRL, 97);
        assert_eq!(KEY_RIGHTCTRL as u8, b'a');
        p.feed(kev(KEY_RIGHTCTRL, 1));
        p.feed(syn(2));
        assert_eq!(p.pending(), 0);
        p.feed(kev(KEY_A, 1)); // 右 Ctrl 在席：控制码 0x01，非 'a' 非 97
        take_k(&mut p, &[0x01]);
        p.feed(kev(KEY_RIGHTCTRL, 0));
        p.feed(kev(KEY_A, 1)); // 松开后回小写
        take_k(&mut p, b"a");
    }

    #[test]
    fn keypump_queue_cap_and_drain_fifo() {
        // 队帽 32：溢出丢新 + 计数；drain 帽在 out.len()、剩余留队
        //（serve_poll 批帽共享的留队语义）；FIFO 序 + 环回。
        let mut p = KeyPump::new();
        for i in 0u8..40 {
            p.post_char(i);
        }
        assert_eq!(p.pending(), 32);
        assert_eq!(p.dropped_bytes, 8);
        let mut out = [0u8; 8];
        assert_eq!(p.drain(&mut out), 8);
        assert_eq!(out, [0u8, 1, 2, 3, 4, 5, 6, 7]); // 丢新：0..31 在队
        let mut big = [0u8; 64];
        assert_eq!(p.drain(&mut big), 24);
        assert_eq!(big[23], 31);
        assert_eq!(p.drain(&mut big), 0);
        assert_eq!(p.pending(), 0);
        p.post_char(0xaa); // 环回：排空后再灌再排
        p.post_char(0xbb);
        take_k(&mut p, &[0xaa, 0xbb]);
    }

    #[test]
    fn keypump_fake_source_sequence() {
        // FakeSource 喂真批形态（QEMU virtio-keyboard 缓冲到 SYN 投递）：
        // Shift+a 完整批 + 释放批 → 恰 'A' 一字节；pop-feed 循环即 serve
        // 回环的收割形态（C-4 FakeSrc 同法）。
        let mut src = FakeSource::new();
        src.push(kev(KEY_LEFTSHIFT, 1));
        src.push(kev(KEY_A, 1));
        src.push(syn(3));
        src.push(kev(KEY_A, 0));
        src.push(kev(KEY_LEFTSHIFT, 0));
        src.push(syn(4));
        let mut p = KeyPump::new();
        while let Some(e) = src.pop() {
            p.feed(e);
        }
        take_k(&mut p, b"A");
        assert_eq!(p.ignored, 0);
    }

    // ---- 导航键出向（UP/DOWN 两枚 + 总攻批七键——ESC 序列 3-4 字节逐帧
    // 连发；混批时 serve 面溢出留队下轮的批帽交互见 translate_seq 注）----

    #[test]
    fn keypump_arrow_up_esc_seq_fifo() {
        // UP press → ESC `[` A 三字节按 FIFO 序入队（xterm 同形）。
        // 冻结值对拍 linux/input-event-codes.h（常量冻在 berry-virtio，
        // 此处消费侧字面钉板——RIGHTCTRL=97 碰撞钉板同法）。
        assert_eq!((KEY_UP, KEY_DOWN), (103, 108));
        let mut p = KeyPump::new();
        p.feed(kev(KEY_UP, 1));
        p.feed(syn(2)); // 完整 wire 批（QEMU 缓冲到 SYN 投递）——批界对键无语义
        assert_eq!(p.pending(), 3);
        take_k(&mut p, &[0x1b, 0x5b, 0x41]);
        assert_eq!(p.ignored, 0);
    }

    #[test]
    fn keypump_arrow_down_esc_seq_fifo() {
        // DOWN press → ESC `[` B（UP 同款）。
        let mut p = KeyPump::new();
        p.feed(kev(KEY_DOWN, 1));
        p.feed(syn(2));
        take_k(&mut p, &[0x1b, 0x5b, 0x42]);
        assert_eq!(p.ignored, 0);
    }

    #[test]
    fn keypump_arrow_release_zero_frame() {
        // 释放零帧：未 press 的 release 吞掉（无按住态追踪面）；press 三
        // 字节排空后的 release 不再产。
        let mut p = KeyPump::new();
        p.feed(kev(KEY_UP, 0));
        p.feed(syn(2));
        assert_eq!(p.pending(), 0);
        p.feed(kev(KEY_DOWN, 1));
        p.feed(kev(KEY_DOWN, 0));
        p.feed(syn(4));
        take_k(&mut p, &[0x1b, 0x5b, 0x42]);
        assert_eq!(p.pending(), 0);
        assert_eq!(p.ignored, 0);
    }

    #[test]
    fn keypump_arrow_mixed_with_letters_fifo() {
        // 与字母键混按：整队 FIFO 序——单字节键与三字节序列按喂序交错，
        // 不重排不合批。
        let mut p = KeyPump::new();
        p.feed(kev(KEY_A, 1));
        p.feed(kev(KEY_UP, 1));
        p.feed(kev(KEY_B, 1));
        p.feed(kev(KEY_DOWN, 1));
        p.feed(syn(5));
        take_k(&mut p, b"a\x1b[Ab\x1b[B");
        assert_eq!(p.ignored, 0);
    }

    #[test]
    fn keypump_arrow_shift_bare_seq() {
        // 修正键不作用：Shift/Ctrl 在席仍裸序列（箭头无修饰形，记面）。
        let mut p = KeyPump::new();
        p.feed(kev(KEY_LEFTSHIFT, 1));
        p.feed(kev(KEY_UP, 1));
        take_k(&mut p, &[0x1b, 0x5b, 0x41]);
        p.feed(kev(KEY_LEFTSHIFT, 0));
        p.feed(kev(KEY_LEFTCTRL, 1));
        p.feed(kev(KEY_DOWN, 1));
        take_k(&mut p, &[0x1b, 0x5b, 0x42]);
    }

    #[test]
    fn keypump_nav_seven_keys_esc_seq() {
        // 导航七键扩表（总攻批 P2——xterm ESC 序列，UP/DOWN 既有两枚之外
        // 新增）：RIGHT `C` / LEFT `D` / HOME `H` / END `F` / DELETE `3~` /
        // PGUP `5~` / PGDN `6~`；4B 序列（DELETE/翻页）逐帧连发。冻结值
        // 对拍 linux/input-event-codes.h（常量镜像钉板在 lib 本侧——
        // berry-virtio 按需再冻未收，见 KEY_HOME 注）。
        assert_eq!(
            (KEY_HOME, KEY_PAGEUP, KEY_PAGEDOWN, KEY_CAPSLOCK),
            (102, 75, 78, 58)
        );
        assert_eq!((KEY_LEFT, KEY_RIGHT, KEY_END, KEY_DELETE), (105, 106, 107, 111));
        let mut p = KeyPump::new();
        p.feed(kev(KEY_RIGHT, 1));
        p.feed(syn(2));
        take_k(&mut p, &[0x1b, b'[', b'C']);
        p.feed(kev(KEY_LEFT, 1));
        p.feed(syn(3));
        take_k(&mut p, &[0x1b, b'[', b'D']);
        p.feed(kev(KEY_HOME, 1));
        p.feed(syn(4));
        take_k(&mut p, &[0x1b, b'[', b'H']);
        p.feed(kev(KEY_END, 1));
        p.feed(syn(5));
        take_k(&mut p, &[0x1b, b'[', b'F']);
        p.feed(kev(KEY_DELETE, 1));
        p.feed(syn(6));
        assert_eq!(p.pending(), 4); // 4B 序列
        take_k(&mut p, &[0x1b, b'[', b'3', b'~']);
        p.feed(kev(KEY_PAGEUP, 1));
        p.feed(kev(KEY_PAGEDOWN, 1));
        p.feed(syn(7));
        take_k(&mut p, &[0x1b, b'[', b'5', b'~', 0x1b, b'[', b'6', b'~']);
        // 释放零帧 + 表外负测（F1）同口径见下测。
        assert_eq!(p.ignored, 0);
    }

    #[test]
    fn keypump_nav_release_and_shift_bare() {
        // 释放零帧：未 press 的 release 吞掉；4B 序列键（PGUP）同款。
        // Shift 在席仍裸序列（导航键无修饰形）。
        let mut p = KeyPump::new();
        p.feed(kev(KEY_LEFT, 0));
        p.feed(kev(KEY_DELETE, 0));
        p.feed(syn(2));
        assert_eq!(p.pending(), 0);
        p.feed(kev(KEY_LEFTSHIFT, 1));
        p.feed(kev(KEY_HOME, 1));
        p.feed(kev(KEY_PAGEUP, 1));
        p.feed(syn(3));
        take_k(&mut p, &[0x1b, 0x5b, b'H', 0x1b, 0x5b, b'5', b'~']);
        p.feed(kev(KEY_PAGEUP, 0));
        assert_eq!(p.pending(), 0);
        assert_eq!(p.ignored, 0);
    }

    #[test]
    fn keypump_f1_still_ignored() {
        // F1=59 表外负测保持：不出向（F1-F12 维持不冻——无消费面再冻，
        // 总攻同裁决）。旧负测 KEY_LEFT=105 已随七键扩表反转（见
        // `keypump_nav_seven_keys_esc_seq`），负测位换 F1 字面码钉板。
        let mut p = KeyPump::new();
        p.feed(kev(59, 1));
        p.feed(syn(2));
        assert_eq!(p.pending(), 0);
        assert_eq!(p.ignored, 1);
        p.feed(kev(59, 0));
        assert_eq!(p.ignored, 1); // 释放吞掉不进计数
    }

    #[test]
    fn key_frame_wire_roundtrip() {
        // 键帧全字段过 berry-guiproto encode 值域校验（guiproto §encode
        // 单源）+ parse 对拍闭环：覆载约定 device_id=DEV_KEYBOARD、x=字节。
        for &b in &[0x01, 0x08, 0x0a, 0x1b, 0x20, b'a', b'Z', 0x7f, 0xff] {
            let ev = key_frame(1234, b);
            assert_eq!(ev.x, b as u32);
            let mut buf = [0u8; 80];
            let n = encode_pump_event(&mut buf, &ev).unwrap();
            let pe = parse_pointer_event(&buf[..n]).unwrap();
            assert_eq!(pe.device_id, DEV_KEYBOARD);
            assert_eq!(pe.phase, PHASE_DOWN);
            assert_eq!((pe.x, pe.y), (b as u32, 0));
            assert_eq!(pe.buttons, 0);
            assert_eq!(pe.flags, 0);
            assert_eq!(pe.pointer_id, 0);
            assert_eq!(pe.timestamp_ns, 1234);
            assert_eq!(pe.pressure, PRESSURE_UNAVAILABLE);
            assert_eq!(pe.history_len, 0);
        }
    }
}
