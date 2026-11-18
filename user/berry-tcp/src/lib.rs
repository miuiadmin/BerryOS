//! berry-tcp：最小 TCP 状态机——客户端 + 服务端半边（「解析器即产品」第 7 例，
//! `docs/TCP栈设计草案-v1.0.md` §3 T-b；服务端半边 = A2-2 段②，`docs/事件循环A2设计草案-v1.0.md` §3.2）。
//!
//! - 零分配 / 零 unsafe / 零依赖 / `#![no_std]`；输入只借用（`TcpSeg::payload` 切片）。
//! - **状态机与 IO 分离**（smoltcp 组织借鉴，只借思路不搬代码）：本 crate 只做状态与
//!   序号推进，出段动作经 `EvOut::action` 交还调用方（T-c 时 vnet `tcp.rs` 翻译成
//!   tx_send），定时（RTO 重挂 / TIME_WAIT 到期）由 vnet 侧时钟驱动——纯函数层不吃时钟。
//! - **`TcpSeg` 是自定义输入视图**（seq/ack/flags/window/mss/payload 借用）——与
//!   berry-netproto 的 parse_tcp（T-a）互不依赖（T-a ∥ T-b 并行的解耦点）；T-c 时
//!   vnet 把 parse_tcp 输出翻译成 TcpSeg 喂进来。
//! - 定值表（草案 §4 对账）：MSS=128（通告+收帽）/ 窗口恒 512 / RTO 3s 起 ×2 退避
//!   3 次帽 / TIME_WAIT 2s / 在途窗 [`TX_SLOTS`]=4 定槽环（多段在途 +
//!   Go-Back-N 队头重发，二刀）/ 发送窗消费（`snd_wnd` 记账 + 零窗停发，二刀）/
//!   乱序段丢弃回当前 ACK（靠对端重传）/ 收 RST 任意状态直转 Closed
//!   （对陌生四元组回 RST = v2）。
//! - 服务端半边（A2-2 段②）：`Conn::listen` 哨兵（vnet 会话表同槽复用的身份标记
//!   ——忽略一切入段**含 RST**：监听口无四元组上下文，误路由的杂散 RST 不得击毙
//!   整个监听服务）、`passive_open` 被动打开（SYN→SynRcvd 答 SYN|ACK；握手第三步
//!   ACK→Established 并同段收捎带数据/FIN——echo 首包快路）、`AcceptQueue` 固定槽
//!   FIFO（队满丢 SYN 的决策在调用方——TCP 规范行为：不应答即丢）。**客户端面
//!   （open/send/close/on_seg 原语义）零改动**。
//! - 每个入段至多产生**一个**出段动作（`Action`）+ 至多一个数据交付切片（`deliver`）
//!   ——ACK+数据+FIN 同段也只回一个 ACK。
//! - 序号回绕 v1 不做专门处理：一律 wrapping 算术防溢出 panic（ABI §8.0「永不 panic」
//!   同族纪律），echo 场景（64B 单会话）远离 2^32 回绕点。
//! - RFC793 已知偏差（F89 钉板，SLIRP 恒 65535 窗域下全为理论面）：① 窗采纳无
//!   `seg.ack ≥ snd_una` 下界——旧 ACK + 新 seq 段可改窗（RFC793 不可接受 ACK 不
//!   采窗）；② 弹槽（`on_seg_open` ①）与 LastAck 覆盖判定用非回绕 `>=`（序号域
//!   4GB 内不可达，其余比较均 wrapping 式）；③ 窗新鲜度等值采纳（同 seq 同 ack
//!   仍采窗、重复段携缩窗可回缩）系有意为之——零窗重开需之，理由见 `on_seg_open`。
#![no_std]

/// 最大段长（通告 + 收取帽）：RX_BUF_SZ 256 − 帧开销 66 = 190 上限内留余量（草案 §5；
/// 该定值安全由 vnet tcp.rs 消费侧 const assert 机器钉住——T-c 对抗审查 ③）。
pub const MSS: u16 = 128;
/// 在途环容量（槽，二刀）：一段 250B RPC 载荷拆 ceil(250/128)=2 槽 + close 排队
/// FIN 1 槽 + 余量 1 ⇒ 4。环容量 4×128 ≥ TCP_DATA_MAX 由 vnet tcp.rs 消费侧
/// const assert 机器钉住（单侧缩环/抬帽即编译期炸响，与 MSS 钉同族）。
pub const TX_SLOTS: usize = 4;
/// 固定通告窗口（非目标滑窗；本 crate 是**收侧**通告——发送窗消费见 `snd_wnd`）。
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
    /// 监听套接字哨兵（A2-2 段②）：SYN 经 vnet demux 后走 `passive_open` 造新连接，
    /// 本 conn 永不收段（收了也全忽略含 RST）——vnet 会话表同槽复用的身份标记。
    Listen,
    /// 服务端被动打开（A2-2 段②）：SYN 收讫、SYN|ACK 已发，等握手第三步 ACK。
    SynRcvd,
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
    /// 服务端握手应答 SYN|ACK（A2-2 段②）：seq = snd_iss、ack = rcv_nxt（= 对端
    /// SYN seq+1）；编码面带 MSS 选项通告同 `Syn`。重传臂 = SynRcvd 态的 `on_rto`。
    SynAck { seq: u32, ack: u32 },
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

/// 在途槽三形（二刀）：SYN / 数据段 / FIN 统一进 [`TX_SLOTS`] 定槽 FIFO
/// （AcceptQueue 同款家法——零分配/no_std 定数组环）。累计 ACK 语义下 ACK
/// 覆盖段尾的槽从环头弹出（跨段覆盖一次弹多头）；straddling 段（部分 ACK
/// 未及段尾）保槽、Go-Back-N 队头重发时整段重发。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxSlot {
    Syn,
    Data { seq: u32, len: u32 },
    Fin { seq: u32 },
}

/// TCP 连接控制块（草案 §3 v0 形状 + 在途环/退避/发送窗记账）。
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
    /// 对端通告窗口记账（二刀发送窗消费）：握手及此后一切携 ACK 段更新；
    /// send 门控「在途 + 新段 ≤ snd_wnd」——零窗停发，窗更新 ACK 解锁。
    pub snd_wnd: u16,
    /// 窗新鲜度左沿（F72 护栏，RFC793 SND.WL1/SND.WL2）：最近一次采纳的
    /// 窗更新所据段序号——`seg.seq` 更新（或相等且 `seg.ack` 不旧于 wl2）
    /// 才覆写 `snd_wnd`，迟到/乱序段不回缩窗。SLIRP 恒 65535 域为理论面，
    /// 护栏廉价保持（多 op 在途 ACK 面扩大顺带收口）。
    pub snd_wl1: u32,
    /// 窗新鲜度右沿（F72，同上——`seg.ack` 侧）。
    pub snd_wl2: u32,
    /// 在途环（定槽 FIFO，环头 = 最老未确认段）。
    tx: [Option<TxSlot>; TX_SLOTS],
    tx_head: usize,
    tx_len: usize,
    /// 已退避次数（snd_una 有覆盖进度即归零）。
    pub rto_backoff: u8,
}

impl Conn {
    /// 在途环清空（RST / 到帽放弃 / 终态收口用）。
    fn tx_clear(&mut self) {
        self.tx = [None; TX_SLOTS];
        self.tx_head = 0;
        self.tx_len = 0;
    }

    /// 压入在途环尾（调用方保证非满——send/close 的拒面已执法）。
    fn tx_push(&mut self, slot: TxSlot) {
        let tail = (self.tx_head + self.tx_len) % TX_SLOTS;
        self.tx[tail] = Some(slot);
        self.tx_len += 1;
    }

    /// 环头槽（最老未确认段；空环 None）。
    fn tx_head_slot(&self) -> Option<TxSlot> {
        if self.tx_len == 0 {
            None
        } else {
            self.tx[self.tx_head]
        }
    }

    /// 在途段数（环占用——vnet op 入口先验「上一 op 已收口」与测试/harness
    /// 观察面用；只读不改状态）。
    pub fn tx_inflight(&self) -> usize {
        self.tx_len
    }

    /// in-flight 占用诊断位（0=空/1=Syn/2=Data/3=Fin——按**环头**重实现，二刀；
    /// 只读，不改状态；vnet send_data 拒面 loud 诊断用）。
    pub fn inflight_kind(&self) -> u8 {
        match self.tx_head_slot() {
            None => 0,
            Some(TxSlot::Syn) => 1,
            Some(TxSlot::Data { .. }) => 2,
            Some(TxSlot::Fin { .. }) => 3,
        }
    }

    /// 暂存紧缩弃置水印的权威源（五刀②，vnet post_account 紧缩臂用）：环头 Data 段
    /// 的**段首** seq——部分覆盖（straddling）ACK 保槽 + RTO 整段重发取段首
    /// （`on_rto` 环头 Data 臂）语义下，**绝不取 `snd_una`**：部分覆盖 ACK
    /// 已推进 snd_una 时取它会切掉仍需重发的段前缀、击重发臂（五刀 plan
    /// 裁决 2）。环空或头为 Syn|Fin → `snd_nxt`（无数据重发引用；环空 =
    /// floor==snd_nxt 的紧缩特例）。只读，不改状态。
    pub fn tx_retx_floor(&self) -> u32 {
        match self.tx_head_slot() {
            Some(TxSlot::Data { seq, .. }) => seq,
            _ => self.snd_nxt,
        }
    }

    /// 主动打开：SynSent + 发 SYN。ISS 由调用方给定（确定性测试需要控制；
    /// vnet 侧可取时钟/计数）。
    pub fn open(iss: u32) -> (Conn, EvOut<'static>) {
        let mut conn = Conn {
            state: State::SynSent,
            snd_iss: iss,
            snd_nxt: iss.wrapping_add(1),
            snd_una: iss,
            rcv_nxt: 0,
            window: WINDOW,
            peer_mss: None,
            snd_wnd: WINDOW,
            snd_wl1: 0,
            snd_wl2: 0,
            tx: [None; TX_SLOTS],
            tx_head: 0,
            tx_len: 0,
            rto_backoff: 0,
        };
        conn.tx_push(TxSlot::Syn);
        (conn, EvOut { action: Action::Syn { seq: iss }, deliver: None })
    }

    /// 监听套接字哨兵（A2-2 段②）：vnet 会话表同槽复用的身份标记。永不收段
    /// （SYN 由 vnet demux 后经 `passive_open` 造新连接）；撤表/close 由 vnet
    /// 管理面直接驱动，不经状态机。
    pub fn listen() -> Conn {
        Conn {
            state: State::Listen,
            snd_iss: 0,
            snd_nxt: 0,
            snd_una: 0,
            rcv_nxt: 0,
            window: WINDOW,
            peer_mss: None,
            snd_wnd: WINDOW,
            snd_wl1: 0,
            snd_wl2: 0,
            tx: [None; TX_SLOTS],
            tx_head: 0,
            tx_len: 0,
            rto_backoff: 0,
        }
    }

    /// 服务端被动打开（A2-2 段②）：收对端 SYN → SynRcvd + 回 SYN|ACK。
    /// 非 SYN 段 → None（协议层执法，不赌调用方 demux 正确）。ISS 由调用方给定
    /// （同 `open` 约定——确定性测试 / vnet 取时钟）；accept 队满的丢弃决策也在
    /// 调用方（`AcceptQueue::is_full` 先验——TCP 规范行为：不应答即丢）。
    pub fn passive_open(iss: u32, syn: &TcpSeg) -> Option<(Conn, EvOut<'static>)> {
        if !syn.flags.syn {
            return None;
        }
        let mut conn = Conn {
            state: State::SynRcvd,
            snd_iss: iss,
            snd_nxt: iss.wrapping_add(1),
            snd_una: iss,
            rcv_nxt: syn.seq.wrapping_add(1),
            window: WINDOW,
            peer_mss: syn.mss,
            snd_wnd: syn.window, // 握手消费对端窗（SYN 面通告；第三步 ACK 再刷新）
            snd_wl1: syn.seq, // F72 基线：SYN 无 ACK 位，wl2 = 0（此后携 ACK 段按护栏推进）
            snd_wl2: 0,
            tx: [None; TX_SLOTS],
            tx_head: 0,
            tx_len: 0,
            rto_backoff: 0,
        };
        conn.tx_push(TxSlot::Syn);
        Some((
            conn,
            EvOut {
                action: Action::SynAck { seq: iss, ack: syn.seq.wrapping_add(1) },
                deliver: None,
            },
        ))
    }

    /// 发一段数据（len ≤ MSS，签名不变）。二刀拒面四形：非 Established /
    /// len 越域（0 或 >MSS）/ 环满（TX_SLOTS 占满）/ 越窗（在途 + 新段 >
    /// snd_wnd——零窗停发，窗更新 ACK 解锁）。段内容由调用方按 (seq, len)
    /// 从自己的缓冲取。
    pub fn send(&mut self, len: u32) -> EvOut<'static> {
        if self.state != State::Established {
            return EvOut::none();
        }
        if len == 0 || len > MSS as u32 {
            return EvOut::none();
        }
        if self.tx_len == TX_SLOTS {
            return EvOut::none(); // 环满：ACK 弹槽腾位
        }
        let inflight = self.snd_nxt.wrapping_sub(self.snd_una);
        if inflight + len > self.snd_wnd as u32 {
            return EvOut::none(); // 越窗/零窗：窗更新 ACK 解锁
        }
        let seq = self.snd_nxt;
        self.snd_nxt = seq.wrapping_add(len);
        self.tx_push(TxSlot::Data { seq, len });
        EvOut { action: Action::Data { seq, len }, deliver: None }
    }

    /// 主动关闭：Established → FinWait1 / CloseWait → LastAck，发 FIN。
    /// 二刀起 FIN **排队尾**——数据在途时 FIN 排在未确认数据后（不再覆盖其
    /// 重传面；累计 ACK 语义下挥手四帧序不变，FIN 的 ACK 隐含确认在途数据）。
    /// 环满时拒（None 不动状态——防御面：vnet 流程在 op 收口后 close，环恒空）。
    pub fn close(&mut self) -> EvOut<'static> {
        let next = match self.state {
            State::Established => State::FinWait1,
            State::CloseWait => State::LastAck,
            _ => return EvOut::none(),
        };
        if self.tx_len == TX_SLOTS {
            return EvOut::none(); // 环满：FIN 无槽可排
        }
        self.state = next;
        let seq = self.snd_nxt;
        self.snd_nxt = seq.wrapping_add(1);
        self.tx_push(TxSlot::Fin { seq });
        EvOut { action: Action::Fin { seq }, deliver: None }
    }

    /// 处理一个入段（已通过四元组/校验和过滤——那是 vnet 编码面的职责）。
    pub fn on_seg<'s>(&mut self, seg: &'s TcpSeg) -> EvOut<'s> {
        // Listen：监听哨兵忽略一切入段——含 RST（「RST 任意状态」规则的唯一豁免面：
        // 监听口无四元组上下文，误路由的杂散 RST 不得击毙整个监听服务）。
        if self.state == State::Listen {
            return EvOut::none();
        }
        // RST：任意状态直转 Closed（会话作废）；Closed 下幂等。
        if seg.flags.rst {
            self.state = State::Closed;
            self.tx_clear();
            self.rto_backoff = 0;
            return EvOut::none();
        }
        match self.state {
            State::Closed | State::Listen => EvOut::none(),
            State::SynSent => self.on_seg_synsent(seg),
            State::SynRcvd => self.on_seg_synrcvd(seg),
            State::Established
            | State::FinWait1
            | State::FinWait2
            | State::CloseWait => self.on_seg_open(seg),
            State::TimeWait => self.on_seg_timewait(seg),
            State::LastAck => self.on_seg_lastack(seg),
        }
    }

    /// RTO 到期（vnet 侧定时驱动）：Go-Back-N 重发**环头最老未确认段**并退避
    /// +1；后续段不随发（各自到期各自重发，接收侧乱序丢弃 + 当前 ACK 语义兜
    /// 底正确性）；backoff 到帽 → 放弃会话转 Closed 清环。环空 Action::None。
    /// 下一次等待时长 = `rto_ms(backoff)`。
    pub fn on_rto(&mut self) -> EvOut<'static> {
        if self.rto_backoff >= RTO_MAX_RETRIES {
            self.state = State::Closed;
            self.tx_clear();
            return EvOut::none();
        }
        let action = match self.tx_head_slot() {
            None => return EvOut::none(),
            Some(TxSlot::Syn) => {
                if self.state == State::SynRcvd {
                    // 服务端：重发 SYN|ACK（ack 取 rcv_nxt 现值——同 incarnation 不变）。
                    Action::SynAck { seq: self.snd_iss, ack: self.rcv_nxt }
                } else {
                    Action::Syn { seq: self.snd_iss }
                }
            }
            Some(TxSlot::Data { seq, len }) => Action::Data { seq, len },
            Some(TxSlot::Fin { seq }) => Action::Fin { seq },
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
            self.snd_wnd = seg.window; // 握手消费对端窗（二刀发送窗消费；F72 基线同立）
            self.snd_wl1 = seg.seq;
            self.snd_wl2 = seg.ack;
            if let Some(m) = seg.mss {
                self.peer_mss = Some(m);
            }
            self.tx_clear(); // Syn 槽被覆盖（环内仅 Syn）
            self.rto_backoff = 0;
            self.state = State::Established;
            return EvOut::ack(); // 握手第三步
        }
        EvOut::none()
    }

    /// SynRcvd（A2-2 段②）：① 对端重传 SYN（我方 SYN|ACK 丢失）——同 incarnation
    /// （seq+1 对齐 rcv_nxt）重答 SYN|ACK；不同 seq = 新 incarnation，v1 忽略。
    /// ② 握手第三步 ACK（ack == iss+1）→ Established，并直接走 open 共路处理
    /// 同段捎带的数据/FIN（ACK+数据同段一步到位——echo 首包快路）。
    fn on_seg_synrcvd<'s>(&mut self, seg: &'s TcpSeg<'s>) -> EvOut<'s> {
        if seg.flags.syn && !seg.flags.ack {
            if seg.seq.wrapping_add(1) == self.rcv_nxt {
                return EvOut {
                    action: Action::SynAck { seq: self.snd_iss, ack: self.rcv_nxt },
                    deliver: None,
                };
            }
            return EvOut::none();
        }
        if seg.flags.ack && seg.ack == self.snd_iss.wrapping_add(1) {
            self.state = State::Established;
            return self.on_seg_open(seg); // ①弹 Syn 槽 ②交付捎带数据 ③同段 FIN
        }
        EvOut::none()
    }

    /// Established/FinWait1/FinWait2/CloseWait 共路：ACK 推进 → 数据交付 → FIN
    /// 收尾 → FinWait1 的 FIN-ACK 升级。每段至多回一个 ACK。
    fn on_seg_open<'s>(&mut self, seg: &'s TcpSeg) -> EvOut<'s> {
        let mut deliver: Option<&[u8]> = None;
        let mut ack_out = false;

        // ① ACK（四刀 P13 域上界 + P72 新鲜度护栏）：SEG.ACK ≤ SND.NXT
        //    违约（幻 ACK 越 snd_nxt）整段 ① 拒——不推进 snd_una、不弹槽、
        //    不采纳窗（RFC793 不可接受 ACK 面；幻 ACK 弹未发槽/spuriously
        //    推进的已知限制收口，多 op 在途放大该面）。合法域内：有任何
        //    覆盖进度即退避归零；窗更新过 F72 护栏（seg.seq 更新，或相等
        //    且 seg.ack 不旧于 wl2——迟到/乱序段不回缩窗）；ACK 覆盖段尾
        //    的槽从环头弹出（跨段覆盖一次弹多头；straddling 段保槽整段重发）。
        if seg.flags.ack {
            let ack_le_nxt = self.snd_nxt.wrapping_sub(seg.ack) < 0x8000_0000;
            if ack_le_nxt {
                if seg.ack != self.snd_una
                    && seg.ack.wrapping_sub(self.snd_una) < 0x8000_0000
                {
                    self.snd_una = seg.ack;
                    self.rto_backoff = 0;
                }
                // RFC793 序号比较：d ∈ (0, 2^31) = seg.seq 严格更新。
                let d = seg.seq.wrapping_sub(self.snd_wl1);
                let seq_newer = d != 0 && d < 0x8000_0000;
                let seq_eq = d == 0;
                // 等值采纳面（F89 钉板）：RFC793 原文为 SND.WL2 < SEG.ACK
                // 严格小于，此处取「不旧于」（含等值）是有意偏差——对端
                // 零窗重开等「同 seq 同 ack 新窗」段必须采纳（本 crate 无
                // persist 探测，严格 < 会把重开窗丢弃到 seq 推进为止）；
                // 陈旧段已被 seq_newer/ack_ge_wl2 护栏挡在门外。残余面：
                // 同 seq 同 ack 的重复段携更小窗仍会覆写（可回缩）——
                // SLIRP 恒窗域结构性不可达，记 crate 头注已知限制。
                let ack_ge_wl2 = seg.ack.wrapping_sub(self.snd_wl2) < 0x8000_0000;
                if seq_newer || (seq_eq && ack_ge_wl2) {
                    self.snd_wnd = seg.window;
                    self.snd_wl1 = seg.seq;
                    self.snd_wl2 = seg.ack;
                }
                while self.tx_len > 0 {
                    let covered = match self.tx[self.tx_head] {
                        Some(TxSlot::Syn) => seg.ack >= self.snd_iss.wrapping_add(1),
                        Some(TxSlot::Data { seq, len }) => seg.ack >= seq.wrapping_add(len),
                        Some(TxSlot::Fin { seq }) => seg.ack >= seq.wrapping_add(1),
                        None => false,
                    };
                    if !covered {
                        break; // 环头 straddling：保槽（部分 ACK）
                    }
                    self.tx[self.tx_head] = None;
                    self.tx_head = (self.tx_head + 1) % TX_SLOTS;
                    self.tx_len -= 1;
                }
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

        // ④ FinWait1 的 FIN 被 ACK（环已空）→ FinWait2。
        //    close() 后环尾必为 Fin{seq}，仅「ACK 覆盖 fin+1」会弹尽它。
        if self.state == State::FinWait1
            && seg.flags.ack
            && self.tx_len == 0
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
            if let Some(TxSlot::Fin { seq }) = self.tx_head_slot() {
                if seg.ack >= seq.wrapping_add(1) {
                    self.tx_clear();
                    self.rto_backoff = 0;
                    self.state = State::Closed;
                }
            }
        }
        EvOut::none()
    }
}

/// accept 队列容量帽（A2-2 段②）：echo 串行验收 2 连接即够——**与 netdproto
/// `TCP_SESSIONS_MAX`(=8) 独立**：会话表帽管 RPC 会话槽（listen + 数据会话共享），
/// 本帽只管「已建立待 accept」的排队深度；队满时对新 SYN 的丢弃决策在调用方
/// （`is_full` 先验——TCP 规范行为：不应答即丢）。
pub const ACCEPT_Q_MAX: usize = 2;

/// accept 队列（A2-2 段②）：已建立连接的 FIFO 容器，固定槽环（零分配/no_std）。
/// 压队只收 Established conn（半开 SynRcvd / 已死 conn 拒——accept 语义协议层
/// 执法）；vnet 在握手完成侧 `push`、消费方经 `accept` 弹最老。
#[derive(Debug, Clone)]
pub struct AcceptQueue {
    slots: [Option<Conn>; ACCEPT_Q_MAX],
    /// 弹出端（最老）。
    head: usize,
    len: usize,
}

impl AcceptQueue {
    pub const fn new() -> Self {
        AcceptQueue { slots: [None; ACCEPT_Q_MAX], head: 0, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 队满先验：SYN 到达且已满 → 调用方直接丢弃（不 `passive_open` 不应答）。
    pub fn is_full(&self) -> bool {
        self.len == ACCEPT_Q_MAX
    }

    /// 压队：仅收 Established conn；满队/状态不符 → false（不静默挤占——调用方
    /// 以返回值执法自己的容量纪律）。
    pub fn push(&mut self, conn: Conn) -> bool {
        if self.len == ACCEPT_Q_MAX || conn.state != State::Established {
            return false;
        }
        let tail = (self.head + self.len) % ACCEPT_Q_MAX;
        self.slots[tail] = Some(conn);
        self.len += 1;
        true
    }

    /// accept：FIFO 弹出最老的已建立连接；空队 → None（非阻塞语义——
    /// netdproto `ST_EAGAIN` 的上游来源）。
    pub fn accept(&mut self) -> Option<Conn> {
        if self.len == 0 {
            return None;
        }
        let conn = self.slots[self.head].take();
        self.head = (self.head + 1) % ACCEPT_Q_MAX;
        self.len -= 1;
        conn
    }
}
