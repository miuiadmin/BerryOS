//! triage —— 子监督分诊决策与键路仲裁纯面（2026-11-20 berry-term 超时
//! 会流分诊刀，Major-3 清偿）。本模块**只产决策不触 IO**：
//! berry_channel_send / berry_object_wait_one / berry_process_terminate /
//! reap 等 IO 原语调用留 berry-term main.rs 薄壳（两 crate 头注互指——
//! 见 lib.rs 职责边界句）。
//!
//! **死信号层级表**（模块立法——mark_child_dead 分诊判据的权威序）：
//! - L1 stdout 零长 EOF：配合性真死权威（libc exit/_exit/abort 族在
//!   berry_process_exit 前经 send_stdout_eof 主动补发——berry-libc
//!   io.rs 收口约定）。已知缺口＝内核 terminate 杀不走 libc exit 族
//!   （无 EOF）——缺口面由 L2 全时覆盖；
//! - L2 hp SIGNAL_TERMINATED：非配合死亡唯一权威窗（deadline=0 非阻塞
//!   poll 或稳态等待死讯臂——内核 terminate 杀 / G1.5 击杀的唯二可观测
//!   面之一）；
//! - L3 stdout 非 EAGAIN 错与键路非 BUSY 错：通道坏——v1 近不可达面
//!   （通道对端关闭信号 v1 根本不存在——kernel/src/channel.rs 头注明文
//!   裁剪「对端关闭检测不做，v3 诚实缺口」，不作判据）。一旦出现＝
//!   监督契约破产且无恢复路，终止重启是唯一有界恢复（裁决 5）；
//! - **EBUSY 非死信号**（本刀除名——G5）：满队＝子忙不读的正常背压
//!   形态（CHAN_DEPTH=8 有界队列），转本地有界 FIFO（[`Backlog`]），
//!   永不构成 mark_child_dead 依据。
//!
//! **键路仲裁**（[`Backlog`]）：pending 非空时新键一律入缓冲不探测——
//! 排空步为满队期唯一发送者，键字节跨段（通道＋本地缓冲）全程 FIFO
//!（G6 结构性成立，不靠时序假设）；溢出丢新键计数（丢旧保新＝bsh 收
//! 乱序半行——不可取，裁决 4）。

/// berry_channel_send 单发非阻塞三态镜像：0 → [`SendFeed::Sent`]；
/// -16 EBUSY → [`SendFeed::Full`]；其余非零 → [`SendFeed::LinkErr`]。
/// 满队与链断分立成两变体＝编译期除掉「队列满视同链断」合流（本刀
/// 病灶一的形态学除名——EBUSY 从死判面除名的类型面执法）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendFeed {
    /// 单发成功（字节已直达通道——Backlog 记 Delivered 零入缓）。
    Sent,
    /// 满队（子忙不读——正常背压形态，转本地 FIFO，非死信号）。
    Full,
    /// 链断（非 BUSY 错——main 先死分诊，不进 Backlog）。
    LinkErr,
}

/// 死询结果镜像：wait_one(hp, SIGNAL_TERMINATED, deadline=0) 非阻塞
/// poll——命中 → [`Liveness::Terminated`]；ERR_AGAIN →
/// [`Liveness::Alive`]。仅影响诊断面文本，不影响死判收敛（见
/// [`DeathEntry`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// TERMINATED 已落账（死证在先）。
    Terminated,
    /// 未命中（子活——键路 LinkErr 误报形态的诊断标记）。
    Alive,
}

/// 死判入口（mark_child_dead 四入口的纯面镜像）：StdoutEof＝L1（EOF
/// 收割路）；StdoutErr＝L3 stdout 臂；SendLinkErr＝L3 键路臂（附死询
/// 标——main 在 terminate 前以 deadline=0 poll 打标，仅进诊断文本）。
///
/// 收敛规则：**四入口全收敛 [`DeathVerdict::TerminateThenReap`] 单路**
/// ——先无条件 berry_process_terminate(hp, 130)（返 0＝已杀／-37＝
/// 已退，皆死证，幂等安全；EOF 配合退出中 terminate 亦安全——mid-exit
/// 子两返码均死证），TERMINATED 前置保证下 reap 瞬回（pipe_selftest
/// terminate→reap 同码先例）。例外：无——Liveness 标不改变收敛
///（Alive 仅是误报诊断，链破产面终止重启仍是唯一有界恢复）；层级表
/// 全文见模块头注。terminate/reap 调用本身留 main.rs。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathEntry {
    /// L1：stdout 零长 EOF（配合性真死权威）。
    StdoutEof,
    /// L3：stdout 非 EAGAIN 错（通道坏）。
    StdoutErr,
    /// L3：键路非 BUSY 错（通道坏——附死询标，仅诊断面）。
    SendLinkErr(Liveness),
}

/// 死判收敛单路（收口漏斗形态——四入口共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathVerdict {
    /// 无条件 terminate(130) 先行 → reap（Err 降级熔断停自动 respawn
    /// ——裁决 6；reap Ok＝更强死证）。IO 序在 main.rs mark_child_dead
    /// 漏斗（M4：排空残留→exited 行→陪葬计数→关双端→terminate→reap）。
    TerminateThenReap,
}

impl DeathEntry {
    /// 收敛表：四入口 → 单路 TerminateThenReap（host 表测执法——
    /// Liveness 标仅影响诊断面文本不影响收敛）。
    pub const fn converge(&self) -> DeathVerdict {
        DeathVerdict::TerminateThenReap
    }
}

/// 本地键 FIFO 容量（256B——锚 bsh LINE_CAP 256 同量级：一条满行命令的
/// 缓冲纵深；裁决 4）。
pub const KEYPEND_CAP: usize = 256;

/// 忙段提示阈值（5s——子忙不读持续 5s 后提示一次「child busy, N keys
/// buffered」；哑终端无本地回显，忙期屏静默需解释，外部命令 1-2s 短忙
/// 段零噪声、sleep 级忙段有反馈；裁决 7。纯常量面实跑可调零结构牵连）。
pub const CHILD_BUSY_HINT_NS: u64 = 5 * 1_000_000_000;

/// 单键入队裁决（[`Backlog::on_key`] 的返回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAct {
    /// 已直达通道（pending 空＋Sent——零入缓零状态触）。
    Delivered,
    /// 已入本地 FIFO（满队转缓／保序入缓）。
    Buffered,
    /// 丢新键（缓冲满溢出——丢新不丢旧，计数可见；LinkErr 防御面同返
    /// 此值但零状态污染）。
    DroppedNew,
}

/// 键路背压状态机：定长环形 FIFO＋溢出丢新计数＋忙段追踪（提示一次
/// 性）。纯数据零 syscall（no_std 延续）。
///
/// **忙段**（busy episode）＝首键入缓起、至排空毕止；不变式
/// `len > 0 ⇔ busy_since.is_some()`（首键入缓置位、末键出队清零、
/// 子死全清三处维护）。hinted 每忙段恰一次、排空复位后再忙段再提示；
/// dropped 为本忙段溢出丢新计数、排空毕随 episode 回报清零。子死时
/// [`Backlog::on_child_dead`] 回报 pending 数并全态清零（respawn 整体
/// 替换自动清零的显式面——新会话不继承旧键，滞留键随旧子陪葬：半行
/// 命令重投新子＝误执行面，裁决 11）。
pub struct Backlog {
    /// 环形 FIFO 本体（`head` 起逻辑序）。
    buf: [u8; KEYPEND_CAP],
    /// 最老键物理位（出队口）。
    head: usize,
    /// 在缓键数（0..=KEYPEND_CAP）。
    len: usize,
    /// 本忙段溢出丢新计数（排空毕回报后清零）。
    dropped: usize,
    /// 忙段起点（None＝无忙段）。
    busy_since: Option<u64>,
    /// 忙段提示已出行（每忙段恰一次）。
    hinted: bool,
}

impl Backlog {
    /// 全零初始化（const fn——可随 Child 结构零成本构造）。
    pub const fn new() -> Backlog {
        Backlog {
            buf: [0u8; KEYPEND_CAP],
            head: 0,
            len: 0,
            dropped: 0,
            busy_since: None,
            hinted: false,
        }
    }

    /// 在缓键数（非零＝满队期——新键不探测直接入缓冲，G6；排空步条件
    /// 与稳态 WRITABLE 背压臂条件共用本判据）。
    pub fn pending_len(&self) -> usize {
        self.len
    }

    /// 单键入队裁决（键路唯一入口——feed 为 child_probe 单发三态）。
    ///
    /// 语义（P3）：pending 空＋Sent → Delivered（字节已直达通道）；pending
    /// 非空 → **一律入缓冲不旁路**（feed 值被忽略——含 Sent：保序不变式
    /// 的结构性负证钉板；调用方契约＝G6「pending 非空时不探测」，模型面
    /// 即使误喂 Sent 也不出旁路口）；缓冲满 → DroppedNew 丢新键计数
    ///（丢新不丢旧——丢旧保新＝bsh 收乱序半行）。首键入缓开忙段
    ///（busy_since 置位、提示位／溢出计数随新忙段复位）。
    ///
    /// LinkErr **不进 Backlog**（main 先死分诊——调用方契约）；防御语义
    /// ＝丢新键返 DroppedNew、零状态污染（不入 FIFO、不开忙段、不动计数
    /// ——子死收口在即，损失面归 [`Backlog::on_child_dead`] 报账）。
    pub fn on_key(&mut self, feed: SendFeed, b: u8, now: u64) -> KeyAct {
        if feed == SendFeed::LinkErr {
            return KeyAct::DroppedNew;
        }
        if self.len == 0 {
            if feed == SendFeed::Sent {
                return KeyAct::Delivered;
            }
            // 首键入缓＝忙段起点（提示位／溢出计数随新忙段复位）。
            self.busy_since = Some(now);
            self.hinted = false;
            self.dropped = 0;
        }
        if self.len == KEYPEND_CAP {
            // 缓冲满：丢新键计数（不丢已收键——FIFO 序保全）。
            self.dropped += 1;
            return KeyAct::DroppedNew;
        }
        let idx = (self.head + self.len) % KEYPEND_CAP;
        self.buf[idx] = b;
        self.len += 1;
        KeyAct::Buffered
    }

    /// 排空步取键（peek 队头——排空步为满队期唯一发送者）：出队须经
    /// [`Backlog::on_sent`] 确认（发送 Sent 才弹——Full 重试时键留队头
    /// 不动，LinkErr 走死分诊键亦留队待陪葬报账）。
    pub fn next_pending(&self) -> Option<u8> {
        if self.len == 0 {
            None
        } else {
            Some(self.buf[self.head])
        }
    }

    /// 一字节送达确认（next_pending 键发送 Sent 后弹队）：排空毕（缓冲
    /// 清空且忙段在席）返回 `Some(本忙段溢出丢新总数)`——调用方据以打
    /// 「dropped N keys」行（N>0 才打）；忙段态全复位（busy_since /
    /// hinted / dropped 清零——再忙段再提示再计数）。空缓调用＝无操作
    /// 返 None（调用方契约：先 next_pending 有键、Sent 后才确认）。
    pub fn on_sent(&mut self) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        self.head = (self.head + 1) % KEYPEND_CAP;
        self.len -= 1;
        if self.len == 0 && self.busy_since.is_some() {
            let dropped = self.dropped;
            self.busy_since = None;
            self.hinted = false;
            self.dropped = 0;
            return Some(dropped);
        }
        None
    }

    /// 忙段提示到期判定（每忙段恰一次）：阈值到 → 置 hinted、返回
    /// `Some(在缓键数)`（「child busy, N keys buffered」的 N——M6 接线
    /// 面）；未到／已出行／无忙段 → None。
    pub fn hint_due(&mut self, now: u64) -> Option<usize> {
        if self.hinted {
            return None;
        }
        let t0 = self.busy_since?;
        if now >= t0.saturating_add(CHILD_BUSY_HINT_NS) {
            self.hinted = true;
            return Some(self.len);
        }
        None
    }

    /// 忙段提示 deadline（未出行时＝busy_since＋阈值，否则 None）——
    /// 稳态 wait_many 的 deadline min 并入点（M3：不赌 IDLE 帽）。
    pub fn hint_at(&self) -> Option<u64> {
        if self.hinted {
            return None;
        }
        self.busy_since
            .map(|t0| t0.saturating_add(CHILD_BUSY_HINT_NS))
    }

    /// 子死全态清零（mark_child_dead 漏斗 ③）：返回被陪葬的本地在缓键
    /// 数（N>0 → 「dropped N keys」行）；FIFO／计数／忙段态全清——
    /// respawn 整体替换自动清零的显式面（新会话不继承旧键）。已进通道
    /// ≤8 键 berry-term 侧不可数（诚实边界——本返回只记本地 pending，
    /// 通道内陪葬键挂账 4）。
    pub fn on_child_dead(&mut self) -> (usize, usize) {
        // 审查 minor 修：回报 (在缓键数, 本忙段丢新总数)——原只报 len 使
        // 忙段中途死亡的「dropped N keys」陪葬行少报溢出损失。
        let pending = self.len;
        let dropped = self.dropped;
        self.head = 0;
        self.len = 0;
        self.dropped = 0;
        self.busy_since = None;
        self.hinted = false;
        (pending, dropped)
    }
}
