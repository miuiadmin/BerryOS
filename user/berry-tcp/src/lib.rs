//! berry-tcp：最小 TCP 客户端状态机（「解析器即产品」第 7 例，`docs/TCP栈设计草案.md` §3 T-b）。
//!
//! - 零分配 / 零 unsafe / 零依赖 / `#![no_std]`；输入只借用（`TcpSeg::payload` 切片）。
//! - **状态机与 IO 分离**（smoltcp 组织借鉴，只借思路不搬代码）：本 crate 只做状态与
//!   序号推进，出段动作经 `EvOut::action` 交还调用方（T-c 时 vnet `tcp.rs` 翻译成
//!   tx_send），定时（RTO 重挂 / TIME_WAIT 到期）由 vnet 侧时钟驱动——纯函数层不吃时钟。
//! - **`TcpSeg` 是自定义输入视图**（seq/ack/flags/window/mss/payload 借用）——与
//!   berry-netproto 的 parse_tcp（T-a）互不依赖（T-a ∥ T-b 并行的解耦点）；T-c 时
//!   vnet 把 parse_tcp 输出翻译成 TcpSeg 喂进来。
//! - 定值表（草案 §4 对账）：MSS=128（通告+收帽）/ 窗口恒 512 / RTO 3s 起 ×2 退避
//!   3 次帽 / TIME_WAIT 2s / in-flight=1（顺序单发，收 ACK 再发）/ 乱序段丢弃回当前
//!   ACK（靠对端重传）/ 收 RST 任意状态直转 Closed（对陌生四元组回 RST = v2）。
//! - 每个入段至多产生**一个**出段动作（`Action`）+ 至多一个数据交付切片（`deliver`）
//!   ——ACK+数据+FIN 同段也只回一个 ACK。
//! - 序号回绕 v1 不做专门处理：一律 wrapping 算术防溢出 panic（ABI §8.0「永不 panic」
//!   同族纪律），echo 场景（64B 单会话）远离 2^32 回绕点。
#![no_std]

/// 最大段长（通告 + 收取帽）：RX_BUF_SZ 256 − 帧开销 66 = 190 上限内留余量（草案 §5；
/// 该定值安全由 vnet tcp.rs 消费侧 const assert 机器钉住——T-c 对抗审查 ③）。
pub const MSS: u16 = 128;
/// 固定通告窗口（非目标滑窗；in-flight=1 下窗口仅保对端健康）。
pub const WINDOW: u16 = 512;
/// RTO 起始值（毫秒）：TCG 友好；SLIRP 本地几乎不丢——重传是防御面非主路径。
pub const RTO_START_MS: u64 = 3_000;
/// RTO 退避帽（重传次数上限）：`on_rto` 在 backoff 到帽时放弃会话。
pub const RTO_MAX_RETRIES: u8 = 3;
/// TIME_WAIT 时长（毫秒）：对端 SLIRP 非敌意；缩短防 vnet 会话循环拖 smoke。
pub const TIME_WAIT_MS: u64 = 2_000;

/// 第 N 次重传后的下一次 RTO 等待时长（3s → 6s → 12s → 24s）。
pub const fn rto_ms(backoff: u8) -> u64 {
    RTO_START_MS << backoff
}

/// TCP 状态枚举（smoltcp 最小形状；客户端主线 Closed→SynSent→Established→
/// FinWait1→FinWait2→TimeWait，被动关闭侧 CloseWait→LastAck）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Closed,
    SynSent,
    Established,
    FinWait1,
    FinWait2,
    TimeWait,
    CloseWait,
    LastAck,
}

/// 入段标志位（位掩码 crate 引入违背零依赖，四 bool 最直白）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SegFlags {
    pub syn: bool,
    pub ack: bool,
    pub fin: bool,
    pub rst: bool,
}

/// 入段视图：T-c 时 vnet 把 berry-netproto `parse_tcp` 的输出翻译进来。
/// payload 只借用——本 crate 不拷贝不持有数据。
#[derive(Debug)]
pub struct TcpSeg<'a> {
    pub seq: u32,
    pub ack: u32,
    pub flags: SegFlags,
    pub window: u16,
    /// SYN 段携带的对端 MSS 通告（无选项 = None）。
    pub mss: Option<u16>,
    pub payload: &'a [u8],
}

/// 出段动作：调用方（vnet）翻译成 wire 段。Ack 段的 seq/ack 从 `Conn` 现值取
/// （seq = `snd_nxt`，ack = `rcv_nxt`——出口处两者必已更新到应答点）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    /// 发/重发 SYN（seq = snd_iss；编码面带 MSS 选项通告）。
    Syn { seq: u32 },
    /// 发/重发数据段（内容 vnet 侧持有——本 crate 零分配不存数据，按 (seq, len) 取回）。
    Data { seq: u32, len: u32 },
    /// 发/重发 FIN。
    Fin { seq: u32 },
    /// 发纯 ACK（数据交付 / FIN 收讫 / 乱序丢弃的隐式重复 ACK）。
    Ack,
}

/// 事件输出：至多一个出段动作 + 至多一个数据交付切片（borrow 输入 payload）。
#[derive(Debug, Clone, Copy)]
pub struct EvOut<'a> {
    pub action: Action,
    pub deliver: Option<&'a [u8]>,
}

impl<'a> EvOut<'a> {
    pub const fn none() -> Self {
        EvOut { action: Action::None, deliver: None }
    }
    pub const fn ack() -> Self {
        EvOut { action: Action::Ack, deliver: None }
    }
}

/// in-flight=1（顺序单发）：任一时刻至多一个未确认段——SYN / 单个数据段 / FIN。
/// 累计 ACK 语义下单槽即够：ACK 覆盖到段尾即清（早于 FIN 的数据被 FIN 的 ACK 隐含确认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InFlight {
    None,
    Syn,
    Data { seq: u32, len: u32 },
    Fin { seq: u32 },
}

/// TCP 连接控制块（草案 §3 v0 形状 + in-flight/退避记账）。
#[derive(Debug, Clone, Copy)]
pub struct Conn {
    pub state: State,
    /// 我方初始序号（SYN 占用）。
    pub snd_iss: u32,
    /// 下一个待发字节序号（open 后 = iss+1；send/close 推进）。
    pub snd_nxt: u32,
    /// 最老未确认序号（对端累计 ACK 推进）。
    pub snd_una: u32,
    /// 期望收到的下一个字节序号（对端 SYN/数据/FIN 推进）。
    pub rcv_nxt: u32,
    /// 通告窗口（恒 WINDOW=512；滑窗 v2 的扩展点）。
    pub window: u16,
    /// 对端 SYN 携带的 MSS 通告（无选项 = None）。
    pub peer_mss: Option<u16>,
    inflight: InFlight,
    /// 已退避次数（ACK 覆盖 in-flight 即归零）。
    pub rto_backoff: u8,
}

impl Conn {
    /// 主动打开：SynSent + 发 SYN。ISS 由调用方给定（确定性测试需要控制；
    /// vnet 侧可取时钟/计数）。
    pub fn open(iss: u32) -> (Conn, EvOut<'static>) {
        let conn = Conn {
            state: State::SynSent,
            snd_iss: iss,
            snd_nxt: iss.wrapping_add(1),
            snd_una: iss,
            rcv_nxt: 0,
            window: WINDOW,
            peer_mss: None,
            inflight: InFlight::Syn,
            rto_backoff: 0,
        };
        (conn, EvOut { action: Action::Syn { seq: iss }, deliver: None })
    }

    /// 发一段数据（len ≤ MSS，in-flight=1：上一段未确认前拒发——返回 None 不动状态）。
    /// 段内容由调用方按 (seq, len) 从自己的缓冲取；多段流 v2。
    pub fn send(&mut self, len: u32) -> EvOut<'static> {
        if self.state != State::Established {
            return EvOut::none();
        }
        if !matches!(self.inflight, InFlight::None) {
            return EvOut::none(); // in-flight=1：收 ACK 再发
        }
        if len == 0 || len > MSS as u32 {
            return EvOut::none();
        }
        let seq = self.snd_nxt;
        self.snd_nxt = seq.wrapping_add(len);
        self.inflight = InFlight::Data { seq, len };
        EvOut { action: Action::Data { seq, len }, deliver: None }
    }

    /// 主动关闭：Established → FinWait1 / CloseWait → LastAck，发 FIN。
    /// v1 调用约定：等 in-flight 数据确认后再 close（echo 面成立）；若数据在途
    /// 直接 close，FIN 覆盖单槽（数据的重传面让位——累计 ACK 语义下可接受）。
    pub fn close(&mut self) -> EvOut<'static> {
        let next = match self.state {
            State::Established => State::FinWait1,
            State::CloseWait => State::LastAck,
            _ => return EvOut::none(),
        };
        self.state = next;
        let seq = self.snd_nxt;
        self.snd_nxt = seq.wrapping_add(1);
        self.inflight = InFlight::Fin { seq };
        EvOut { action: Action::Fin { seq }, deliver: None }
    }

    /// 处理一个入段（已通过四元组/校验和过滤——那是 vnet 编码面的职责）。
    pub fn on_seg<'s>(&mut self, seg: &'s TcpSeg) -> EvOut<'s> {
        // RST：任意状态直转 Closed（会话作废）；Closed 下幂等。
        if seg.flags.rst {
            self.state = State::Closed;
            self.inflight = InFlight::None;
            self.rto_backoff = 0;
            return EvOut::none();
        }
        match self.state {
            State::Closed => EvOut::none(),
            State::SynSent => self.on_seg_synsent(seg),
            State::Established
            | State::FinWait1
            | State::FinWait2
            | State::CloseWait => self.on_seg_open(seg),
            State::TimeWait => self.on_seg_timewait(seg),
            State::LastAck => self.on_seg_lastack(seg),
        }
    }

    /// RTO 到期（vnet 侧定时驱动）：重发 in-flight 段并退避 +1；backoff 到帽 →
    /// 放弃会话转 Closed。下一次等待时长 = `rto_ms(backoff)`。
    pub fn on_rto(&mut self) -> EvOut<'static> {
        if self.rto_backoff >= RTO_MAX_RETRIES {
            self.state = State::Closed;
            self.inflight = InFlight::None;
            return EvOut::none();
        }
        let action = match self.inflight {
            InFlight::None => return EvOut::none(),
            InFlight::Syn => Action::Syn { seq: self.snd_iss },
            InFlight::Data { seq, len } => Action::Data { seq, len },
            InFlight::Fin { seq } => Action::Fin { seq },
        };
        self.rto_backoff += 1;
        EvOut { action, deliver: None }
    }

    /// TIME_WAIT 到期（vnet 侧 2s 定时）：转 Closed。非 TimeWait 状态调用无操作。
    pub fn expire_timewait(&mut self) {
        if self.state == State::TimeWait {
            self.state = State::Closed;
        }
    }

    /// SynSent：只认 SYN+ACK 且 ack == iss+1（同时打开 v1 不支持——纯 SYN 忽略）。
    fn on_seg_synsent<'s>(&mut self, seg: &TcpSeg<'s>) -> EvOut<'s> {
        if seg.flags.syn
            && seg.flags.ack
            && seg.ack == self.snd_iss.wrapping_add(1)
        {
            self.snd_una = self.snd_iss.wrapping_add(1);
            self.rcv_nxt = seg.seq.wrapping_add(1);
            if let Some(m) = seg.mss {
                self.peer_mss = Some(m);
            }
            self.inflight = InFlight::None;
            self.rto_backoff = 0;
            self.state = State::Established;
            return EvOut::ack(); // 握手第三步
        }
        EvOut::none()
    }

    /// Established/FinWait1/FinWait2/CloseWait 共路：ACK 推进 → 数据交付 → FIN
    /// 收尾 → FinWait1 的 FIN-ACK 升级。每段至多回一个 ACK。
    fn on_seg_open<'s>(&mut self, seg: &'s TcpSeg) -> EvOut<'s> {
        let mut deliver: Option<&[u8]> = None;
        let mut ack_out = false;

        // ① ACK：snd_una 单调推进（序号域远离回绕，max 即够）；in-flight 段被
        //    完全覆盖即清 + 退避归零。
        if seg.flags.ack {
            if seg.ack > self.snd_una {
                self.snd_una = seg.ack;
            }
            let cleared = match self.inflight {
                InFlight::None => false,
                InFlight::Syn => seg.ack >= self.snd_iss.wrapping_add(1),
                InFlight::Data { seq, len } => seg.ack >= seq.wrapping_add(len),
                InFlight::Fin { seq } => seg.ack >= seq.wrapping_add(1),
            };
            if cleared {
                self.inflight = InFlight::None;
                self.rto_backoff = 0;
            }
        }

        // ② 数据：seq 对齐 rcv_nxt 才交付；乱序/重复一律丢弃（不缓存）——
        //    两种情况都回当前 ACK（丢弃 = 隐式重复 ACK，靠对端重传恢复）。
        let seg_end = seg.seq.wrapping_add(seg.payload.len() as u32);
        if !seg.payload.is_empty() {
            if seg.seq == self.rcv_nxt {
                deliver = Some(seg.payload);
                self.rcv_nxt = seg_end;
            }
            ack_out = true;
        }

        // ③ FIN：段尾对齐 rcv_nxt 才收（乱序 FIN 丢弃靠重传）；FIN 占一个序号。
        if seg.flags.fin && seg_end == self.rcv_nxt {
            self.rcv_nxt = self.rcv_nxt.wrapping_add(1);
            ack_out = true;
            self.state = match self.state {
                State::Established => State::CloseWait,
                // FinWait1 同段带 FIN（同时挥手）直入 TimeWait——④ 不再降级。
                State::FinWait1 | State::FinWait2 => State::TimeWait,
                s => s, // CloseWait 重复 FIN 忽略（防御）
            };
        }

        // ④ FinWait1 的 FIN 被 ACK（in-flight 已清）→ FinWait2。
        //    close() 后 inflight 必为 Fin{seq}，仅「ACK 覆盖 fin+1」会清它。
        if self.state == State::FinWait1
            && seg.flags.ack
            && matches!(self.inflight, InFlight::None)
        {
            self.state = State::FinWait2;
        }

        EvOut {
            action: if ack_out { Action::Ack } else { Action::None },
            deliver,
        }
    }

    /// TimeWait：重复 FIN（我方 ACK 丢了）→ 重 ACK（vnet 处于 TimeWait 即重挂 2s 计时）。
    fn on_seg_timewait<'s>(&mut self, seg: &TcpSeg<'s>) -> EvOut<'s> {
        let seg_end = seg
            .seq
            .wrapping_add(seg.payload.len() as u32)
            .wrapping_add(1); // FIN 占一个序号
        if seg.flags.fin && seg_end == self.rcv_nxt {
            return EvOut::ack();
        }
        EvOut::none()
    }

    /// LastAck：只等 FIN 的 ACK——覆盖即 Closed（正常终态）。
    fn on_seg_lastack<'s>(&mut self, seg: &TcpSeg<'s>) -> EvOut<'s> {
        if seg.flags.ack {
            if let InFlight::Fin { seq } = self.inflight {
                if seg.ack >= seq.wrapping_add(1) {
                    self.inflight = InFlight::None;
                    self.rto_backoff = 0;
                    self.state = State::Closed;
                }
            }
        }
        EvOut::none()
    }
}
