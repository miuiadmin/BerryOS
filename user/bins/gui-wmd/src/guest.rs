//! gui-wmd —— GUI-5（ADR #61）窗口管理守护：呈现（guifbd）/窗口策略
//! （本服务）分离。guifbd 零改动；gfxdemo 改连 wm 域（同 op 换端——
//! present/hdi app-facing 六+四 op 经本服务透传中继，wm 六 op 本地权威处理；
//! BootProgress op 7 是 wmd 自产系统控制面；
//! inputd 追加推送端收窗口表快照（WM_TABLE_WAIT 阻塞应答形）。
//!
//! **boot 授能链**：EFI 相 svcmgr guifbd arm Ok 后 wmd arm 预拉本服务
//! （门 = present_cli≠0 ∧ cpio 命中），extras 4 枚 = [wm_srv（gfxdemo
//! 客户端面）, push_srv（inputd 客户端面）, present_cli, hdi_cli（guifbd
//! 客户端端换手件）] 加 base 4 恰 8/8（§7 Q12 冻结）。rootfs 相（-kernel
//! 无 fb）svcmgr 探测失败不 spawn——本服务恒只在 EFI 链存活。
//!
//! **启动序五步**（§3 #4）：①收割四端（0 值 = 协议破缺面：eprintln +
//! park 挂起——**非 exit(0)**，svcmgr 已 ps_reg ALIVE，真退出无终态回写
//! 路径 = 永久 ALIVE 谎账；标记缺失 = smoke FAIL loud 兜底）→ ②present
//! 首发 RELEASE_WAIT 清前世孤儿别名（wmd 崩溃重启残留：guifbd held 表
//! 持前世在途别名 credits 永占；新 wmd 首发 ReleaseWait 全量清账，guifbd
//! :474-496 语义零 guifbd 改动；boot 期 guifbd 空转即答 = 无长 park 面）
//! → ③present GETINFO 缓存几何（真值源恒 guifbd——wmd 不持第二份 present
//! 常量副本，Q15）→ ④表初始化空表 version≥1 → ⑤`gui-wmd: up` 诊断行
//! （小写 wmd 前缀防误配 `WM ok` 标记——本行永不进 MARKERS）转 serve。
//!
//! **serve 三族 poll-first**：wm_srv broker→push_srv→session 表确定性
//! 轮询（每通道一请求一应答，串行 RPC），全 EAGAIN 才 wait_many
//! broker/push/session READABLE + Process TERMINATED；deadline=now+10s
//! 进调用本身。到点 = 合法空闲重臂；recv 非 EAGAIN / Process wait 错码
//! = 死亡面 trap。
//!
//! **Phase 1 session/owner 形态**：wm_srv 只收 WM_CONNECT；客户端随行迁
//! per-session endpoint + self Process，业务通道按 session 分流。窗口
//! owner 归 session，跨 owner SET/DESTROY = ST_DENIED；Process TERMINATED
//! 自动 destroy_owned。多客户端下 TABLE 快照是「应答生成时」 truth：
//! present 中继阻塞期间其它 session 可变更表，快照不声称「请求到达时」
//! 一致；需要该语义时后续加 per-request snapshot stamp，勿静默扩大承诺。

use berry_gfx::font::{FontAtlas32x48, GlyphAtlas};
use berry_guiproto as gp;
use berry_libc::io::{wm_hdi_h, wm_present_h, wm_push_srv_h, wm_srv_h};
use core::sync::atomic::{AtomicBool, Ordering};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_object_wait_many, berry_object_wait_one, berry_thread_yield,
    berry_vmo_commit, berry_vmo_create, berry_vmo_write, Handle, BERRY_ERR_AGAIN,
    BERRY_ERR_BUSY, BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC,
    SIGNAL_READABLE, SIGNAL_TERMINATED, SIGNAL_WRITABLE,
};

#[path = "relay.rs"]
mod relay;

use gui_wmd::table;

// 链接种子（guifbd 同款）：rlib/LTO 按需保留——无引用则 berry-libc 的 crt0
// （含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// serve 空闲并等帽：wait_many deadline 外层上界（合法空闲非死线——到点
/// 重臂续等；guifbd `IDLE_CAP_NS` 同款）。
const WMD_IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// 系统字形供给：95 个 ASCII HD A8 字形，恰 32×48×95B。每个 APP
/// WM_CONNECT 成功应答随行一枚私有填充 VMO（v1 映射恒 UserRw，私有拷贝
/// 防跨 app 写穿）；rootfs 不再由各 app 重复携带字表。
const FONT_GLYPH_BYTES: usize = 32 * 48;
const FONT_VMO_LEN: usize = 0x24000;

/// MOVE 低延迟帽：v1 曾在 4 条后合并，防止下游把积压 MOVE 当回放动画。
/// 60Hz 目标下 launcher 已按真实样本逐帧呈现；队列满前保留全部离散 MOVE，
/// 只在真正溢出时才把旧真实采样并入 history（仍不合成新位置）。
const MOVE_QUEUE_LATENCY_MAX: usize = 16;

/// Phase 3 per-session 输入环（16 深；溢出 loud 计数，不覆盖旧事件）。
struct InputQueue {
    events: [gp::PointerEvent; 16],
    head: usize,
    tail: usize,
    len: usize,
    dropped: u64,
    coalesced: u64,
}

const fn zero_key_event() -> gp::KeyEvent {
    gp::KeyEvent {
        timestamp_ns: 0,
        code: 0,
        action: gp::KEY_ACTION_UP,
        modifiers: 0,
        device_id: 0,
    }
}

/// P2 per-session 键盘环（16 深；与 pointer 环分域，互不挤占）。
struct KeyQueue {
    events: [gp::KeyEvent; 16],
    head: usize,
    tail: usize,
    len: usize,
    dropped: u64,
}

impl KeyQueue {
    fn new() -> Self {
        Self {
            events: [zero_key_event(); 16],
            head: 0,
            tail: 0,
            len: 0,
            dropped: 0,
        }
    }

    fn push(&mut self, ev: gp::KeyEvent) -> bool {
        if self.len == self.events.len() {
            self.dropped += 1;
            return false;
        }
        self.events[self.tail] = ev;
        self.tail = (self.tail + 1) % self.events.len();
        self.len += 1;
        true
    }

    fn pop_into(&mut self, out: &mut [gp::KeyEvent]) -> usize {
        let mut n = 0usize;
        while n < out.len() && self.len > 0 {
            out[n] = self.events[self.head];
            self.head = (self.head + 1) % self.events.len();
            self.len -= 1;
            n += 1;
        }
        n
    }
}

impl InputQueue {
    fn new() -> Self {
        Self {
            events: [zero_event(); 16],
            head: 0,
            tail: 0,
            len: 0,
            dropped: 0,
            coalesced: 0,
        }
    }

    /// 队列压力下合并同指针 MOVE：DOWN/UP/CANCEL 永不让位，最新位置保留；
    /// 被压缩的只是 history 中较旧的真实采样（wire 本来就只容 3 个历史点）。
    fn coalesce_move(&mut self, ev: gp::PointerEvent) -> bool {
        debug_assert!(ev.phase == gp::PHASE_MOVE);
        let cap = self.events.len();
        let mut cursor = self.head;
        let mut best = None;
        for _ in 0..self.len {
            let i = cursor;
            cursor = (cursor + 1) % cap;
            if self.events[i].phase == gp::PHASE_MOVE
                && self.events[i].pointer_id == ev.pointer_id
            {
                best = Some(i);
            }
        }
        let Some(i) = best else {
            return false;
        };

        // 组成 oldest→newest 的真实采样序列，然后保留最新 3 个作为
        // 新事件 history；header 保持 incoming MOVE（最新坐标/时间）。
        let old = self.events[i];
        let mut samples = [gp::Sample { timestamp_ns: 0, x: 0, y: 0 }; 8];
        let mut n = 0usize;
        for sample in old.history[..old.history_len].iter() {
            samples[n] = *sample;
            n += 1;
        }
        samples[n] = gp::Sample {
            timestamp_ns: old.timestamp_ns,
            x: old.x,
            y: old.y,
        };
        n += 1;
        for sample in ev.history[..ev.history_len].iter() {
            samples[n] = *sample;
            n += 1;
        }
        samples[n] = gp::Sample {
            timestamp_ns: ev.timestamp_ns,
            x: ev.x,
            y: ev.y,
        };
        n += 1;

        let start = n.saturating_sub(gp::INPUT_SAMPLES_MAX - 1);
        let history_len = n - start;
        let mut merged = ev;
        merged.history =
            [gp::Sample { timestamp_ns: 0, x: 0, y: 0 }; gp::INPUT_SAMPLES_MAX - 1];
        merged.history[..history_len].copy_from_slice(&samples[start..n]);
        merged.history_len = history_len;
        self.events[i] = merged;
        self.coalesced += 1;
        true
    }

    fn push_control_evict_move(&mut self, ev: gp::PointerEvent) -> bool {
        let cap = self.events.len();
        let found = (0..self.len).find(|i| {
            self.events[(self.head + i) % cap].phase == gp::PHASE_MOVE
        });
        let Some(found) = found else {
            return false;
        };
        for k in found..self.len - 1 {
            let src = (self.head + k + 1) % cap;
            let dst = (self.head + k) % cap;
            self.events[dst] = self.events[src];
        }
        self.events[(self.head + self.len - 1) % cap] = ev;
        return true;
    }

    fn push(&mut self, ev: gp::PointerEvent) -> bool {
        // 保留少量 MOVE 供手势 referee 计算位移/速度；超过低延迟帽后
        // 合并最新 MOVE，避免 1080p 拖动把下游帧循环压成移动回放动画。
        if ev.phase == gp::PHASE_MOVE {
            let cap = self.events.len();
            let move_n = (0..self.len)
                .filter(|i| {
                    self.events[(self.head + i) % cap].phase == gp::PHASE_MOVE
                })
                .count();
            if move_n >= MOVE_QUEUE_LATENCY_MAX && self.coalesce_move(ev) {
                return true;
            }
        }
        if self.len == self.events.len() {
            if ev.phase == gp::PHASE_MOVE && self.coalesce_move(ev) {
                return true;
            }
            if ev.phase != gp::PHASE_MOVE && self.push_control_evict_move(ev) {
                return true;
            }
            self.dropped += 1;
            return false;
        }
        self.events[self.tail] = ev;
        self.tail = (self.tail + 1) % self.events.len();
        self.len += 1;
        true
    }

    fn pop_into(&mut self, out: &mut [gp::PointerEvent]) -> usize {
        let mut n = 0usize;
        while n < out.len() && self.len > 0 {
            out[n] = self.events[self.head];
            self.head = (self.head + 1) % self.events.len();
            self.len -= 1;
            n += 1;
        }
        n
    }
}

/// 单个 wm 客户端 session：WM/RPC 专通道、输入专通道、owner id、
/// client_type 与 self Process 死亡观察柄。
struct WmSession {
    id: u16,
    client_type: u8,
    ch: Handle,
    input: Handle,
    process: Handle,
    queue: InputQueue,
    keys: KeyQueue,
    /// P1：WM_CONNECT 附带的 svcmgr 一次性 escape token 是否已消费验证。
    escape: bool,
    /// P1：逐窗口 Surface Mode；None = 未显式协商的 Retained。
    surface_modes: [Option<gp::surface::SurfaceModeInfo>; gp::WM_WINDOWS_MAX],
    /// 本 session 首个成功合成 frame 是否已通知 SYSTEM 嵌入器。
    frame_notified: bool,
}

/// P1 surface 状态使 session 增大；wmd 主栈 8 页。16KiB 结构帽先炸编译，
/// 防未来逐窗状态扩容静默逼近栈底。
const _: () = assert!(core::mem::size_of::<WmSessions>() <= 16 * 1024);

/// 接触粘滞锁：pointer_id → (session, window_id)；window_id=0 表示桌面。
#[derive(Clone, Copy)]
struct InputLock {
    pointer_id: u16,
    session: u16,
    window: u16,
}

/// Phase 1/3/Browser session 表（静态定容；帽 7 = gfxdemo×2、launcher、
/// 路由验证、Browser 与两个未来真实 app）。WM RPC 与输入分通道，避免
/// launcher 异步 INPUT_POLL 与 present RPC 在同一 channel 上响应错配。
struct WmSessions {
    slots: [Option<WmSession>; 7],
    /// P1 per-boot escape seed：svcmgr 预共享后才有值；不进公开常量。
    escape_seed: [u8; 8],
    escape_seed_set: bool,
    /// R1 per-boot SYSTEM client seed：仅 svcmgr 启动期预共享，SYSTEM
    /// WM_CONNECT 必须随行同 seed channel。
    system_seed: [u8; 8],
    system_seed_set: bool,
    next_id: u16,
    input_lock: Option<InputLock>,
}

const fn zero_event() -> gp::PointerEvent {
    gp::PointerEvent {
        phase: gp::PHASE_CANCEL,
        device_id: 0,
        buttons: 0,
        flags: 0,
        pointer_id: 0,
        timestamp_ns: 0,
        x: 0,
        y: 0,
        pressure: gp::PRESSURE_UNAVAILABLE,
        contact_major: 0,
        contact_minor: 0,
        history: [gp::Sample { timestamp_ns: 0, x: 0, y: 0 };
            gp::INPUT_SAMPLES_MAX - 1],
        history_len: 0,
    }
}

impl WmSessions {
    fn new() -> Self {
        Self {
            slots: [None, None, None, None, None, None, None],
            escape_seed: [0; 8],
            escape_seed_set: false,
            system_seed: [0; 8],
            system_seed_set: false,
            next_id: 0,
            input_lock: None,
        }
    }

    /// P1 seed 只接受一次，且必须在任何 WM_CONNECT 前（svcmgr 在 app
    /// broker 端被 dup/赠出前完成预共享并阻塞等 ack）。
    fn set_escape_seed(&mut self, seed: [u8; 8]) -> bool {
        if self.escape_seed_set || self.slots.iter().any(|s| s.is_some()) {
            return false;
        }
        self.escape_seed = seed;
        self.escape_seed_set = true;
        true
    }

    /// R1 SYSTEM seed 同样只接受一次，且必须在任何 WM_CONNECT 前。
    fn set_system_seed(&mut self, seed: [u8; 8]) -> bool {
        if self.system_seed_set || self.slots.iter().any(|s| s.is_some()) {
            return false;
        }
        self.system_seed = seed;
        self.system_seed_set = true;
        true
    }

    /// 读取第 4 handle 上的 8B seed。channel 消息必须恰 8B、零随行。
    fn read_seed_channel(h: Handle) -> Option<[u8; 8]> {
        let mut token = [0u8; 8];
        let mut nbytes = token.len();
        let mut nhandles = 0usize;
        if berry_channel_recv(
            h,
            token.as_mut_ptr(),
            &mut nbytes,
            core::ptr::null_mut(),
            &mut nhandles,
        ) != 0 || nbytes != token.len() || nhandles != 0
        {
            return None;
        }
        Some(token)
    }

    /// broker 面 WM_CONNECT：请求恰携 [WM endpoint, input endpoint,
    /// self process]。成功后 WM/present/hdi 与输入分别走两根专通道。
    fn connect(&mut self, broker: Handle, msg: &[u8], hs: &[Handle]) {
        let bad = |hs: &[Handle]| {
            close_all(hs);
            st_rep(broker, gp::ST_BAD);
        };
        if (hs.len() != 3 && hs.len() != 4) || hs.iter().any(|&h| h == 0) {
            bad(hs);
            return;
        }
        let Some(client_type) = gp::parse_wm_connect_req(msg) else {
            bad(hs);
            return;
        };
        // R1：第 4 handle 是 per-boot capability channel，语义按 client_type
        // 分流——APP 只能携带 escape seed，SYSTEM 只能携带 system seed。
        // 公开常量/自铸 channel 无法通过。
        let escape = match client_type {
            gp::WM_CLIENT_APP => {
                if hs.len() == 4 {
                    let token = Self::read_seed_channel(hs[3]);
                    if !self.escape_seed_set
                        || token.as_ref().map(|x| x.as_slice()) != Some(self.escape_seed.as_slice())
                    {
                        close_all(hs);
                        st_rep(broker, gp::ST_DENIED);
                        return;
                    }
                    if berry_handle_close(hs[3]) != 0 {
                        trap();
                    }
                    true
                } else {
                    false
                }
            }
            gp::WM_CLIENT_SYSTEM => {
                if hs.len() != 4 {
                    close_all(hs);
                    st_rep(broker, gp::ST_DENIED);
                    return;
                }
                let token = Self::read_seed_channel(hs[3]);
                if !self.system_seed_set
                    || token.as_ref().map(|x| x.as_slice()) != Some(self.system_seed.as_slice())
                {
                    close_all(hs);
                    st_rep(broker, gp::ST_DENIED);
                    return;
                }
                if berry_handle_close(hs[3]) != 0 {
                    trap();
                }
                false
            }
            _ => {
                close_all(hs);
                st_rep(broker, gp::ST_BAD);
                return;
            }
        };
        // seed channel 成功读取后已关闭；后续失败路径只回收前三柄，防止
        // ST_FULL 等路径对已关 handle 二次 close。
        let hs = &hs[..3];
        if client_type == gp::WM_CLIENT_SYSTEM
            && self.system_slot().is_some()
        {
            close_all(hs);
            st_rep(broker, gp::ST_FULL);
            return;
        }
        let Some(slot) = (0..self.slots.len()).find(|&i| self.slots[i].is_none()) else {
            close_all(hs);
            st_rep(broker, gp::ST_FULL);
            return;
        };
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let id = self.next_id;
        self.slots[slot] = Some(WmSession {
            id,
            client_type,
            ch: hs[0],
            input: hs[1],
            process: hs[2],
            queue: InputQueue::new(),
            keys: KeyQueue::new(),
            escape,
            surface_modes: [None; gp::WM_WINDOWS_MAX],
            frame_notified: false,
        });
        let mut out = [0u8; 8];
        let n = match gp::encode_wm_connect_rep(&mut out, id) {
            Some(n) => n,
            None => trap(),
        };
        if client_type == gp::WM_CLIENT_APP {
            // APP connect ack：body 8B 不变，随行 [私有 font VMO]。
            // v1 vmar_map 只有 UserRw，若共享同一字形 VMO，任一 app 可
            // 写穿破坏全局字体；因此每个 APP 收独立填充 VMO（物理页私有，
            // rootfs 不重复字表）。system 消费者不带该能力。
            let font_vmo = make_font_vmo();
            send_rep_handles(broker, &out[..n], &[font_vmo]);
        } else {
            send_rep(broker, &out[..n]);
        }
        println!("WMCONN ok id={id}");
        if client_type == gp::WM_CLIENT_SYSTEM {
            // R1 正例：SYSTEM session 只有在 per-boot seed channel 验证
            // 通过后才会到达这里；自报 client_type 的路径已在前置分支拒绝。
            println!("WMSYSAUTH ok id={id}");
            SYSTEM_UI_PROGRESS_READY.store(true, Ordering::Relaxed);
        }
    }

    /// WM 权威可见性已变化时唤醒 SYSTEM 嵌入器并请求全屏重绘。事件使用
    /// CANCEL + SYSTEM_INVALIDATE，不进入普通 hit/referee 语义。
    fn invalidate_system_surface(&mut self, action: u32) {
        let Some(i) = self.system_slot() else {
            return;
        };
        let mut ev = zero_event();
        ev.flags = gp::PE_FL_SYNTHESIZED | gp::PE_FL_SYSTEM_INVALIDATE;
        ev.x = action;
        let sess = self.slots[i].as_mut().expect("system slot checked");
        let sid = sess.id;
        if sess.queue.push(ev) {
            println!("WMNAVINVAL ok sid={sid}");
        } else {
            eprintln!("WMNAVINVAL drop sid={sid} dropped={}", sess.queue.dropped);
        }
    }

    /// APP owner 第一个成功合成 frame 已到达；用于 SYSTEM launcher 清除真实
    /// Opening 状态。x 携带 owner id，事件本体不参与普通 hit/referee。
    fn notify_system_app_frame(&mut self, owner: u16) {
        let Some(i) = self.system_slot() else {
            return;
        };
        let mut ev = zero_event();
        ev.flags = gp::PE_FL_SYNTHESIZED | gp::PE_FL_SYSTEM_APP_FRAME;
        ev.x = u32::from(owner);
        let sess = self.slots[i].as_mut().expect("system slot checked");
        if sess.queue.push(ev) {
            println!("WMAPPFRAME ok owner={owner}");
        } else {
            eprintln!("WMAPPFRAME drop owner={owner} dropped={}", sess.queue.dropped);
        }
    }

    fn slot_index(&self, id: u16) -> Option<usize> {
        (0..self.slots.len()).find(|&i| self.slots[i].as_ref().map(|s| s.id) == Some(id))
    }

    fn system_slot(&self) -> Option<usize> {
        (0..self.slots.len()).find(|&i| {
            self.slots[i]
                .as_ref()
                .map(|s| s.client_type == gp::WM_CLIENT_SYSTEM)
                .unwrap_or(false)
        })
    }

    fn localize(ev: &mut gp::PointerEvent, x: u16, y: u16) {
        ev.x = ev.x.saturating_sub(u32::from(x));
        ev.y = ev.y.saturating_sub(u32::from(y));
        for s in ev.history[..ev.history_len].iter_mut() {
            s.x = s.x.saturating_sub(u32::from(x));
            s.y = s.y.saturating_sub(u32::from(y));
        }
    }

    /// inputd 原始事件 → 权威 owner 路由。DOWN hit-test；MOVE/UP 粘滞；
    /// 锁定窗失踪合成 CANCEL；桌面 MISS 落 SYSTEM session（launcher）。
    fn route(&mut self, table: &mut table::WmTable, ev: &gp::PointerEvent) {
        let mut out = *ev;
        let mut clear_lock = false;
        let (sid, window, origin) = match out.phase {
            gp::PHASE_DOWN => {
                let target = match table.hit_route(out.x, out.y) {
                    Some((owner, win, x, y)) => {
                        if table.focus_from_pointer(win) {
                            println!(
                                "WMFOCUS ok sid={} window={} v={}",
                                owner,
                                win,
                                table.version()
                            );
                        }
                        (owner, win, Some((x, y)))
                    }
                    None => match self.system_slot() {
                        Some(i) => (self.slots[i].as_ref().unwrap().id, 0, None),
                        None => return,
                    },
                };
                self.input_lock = Some(InputLock {
                    pointer_id: out.pointer_id,
                    session: target.0,
                    window: target.1,
                });
                target
            }
            _ => {
                let Some(lock) = self.input_lock else { return };
                if lock.pointer_id != out.pointer_id {
                    return;
                }
                clear_lock = out.phase == gp::PHASE_UP || out.phase == gp::PHASE_CANCEL;
                let origin = if lock.window == 0 {
                    None
                } else {
                    match table.route_geometry(lock.session, lock.window) {
                        Some(xy) => Some(xy),
                        None => {
                            out.phase = gp::PHASE_CANCEL;
                            out.flags |= gp::PE_FL_SYNTHESIZED;
                            clear_lock = true;
                            None
                        }
                    }
                };
                (lock.session, lock.window, origin)
            }
        };
        // SYSTEM shell windows use display-global pointer coordinates (the
        // launcher owns status/navigation/recents semantics). APP windows keep
        // raw window-local coordinates required by their fixed frame VMO.
        let system_target = self
            .system_slot()
            .and_then(|i| self.slots[i].as_ref().map(|s| s.id))
            .is_some_and(|system_sid| system_sid == sid);
        if let Some((x, y)) = origin.filter(|_| !system_target) {
            Self::localize(&mut out, x, y);
        }
        let Some(i) = self.slot_index(sid) else {
            self.input_lock = None;
            return;
        };
        let sess = self.slots[i].as_mut().unwrap();
        if !sess.queue.push(out) {
            let dropped = sess.queue.dropped;
            // 交互拖动会瞬间产生数百个 MOVE；每个溢出都同步写串口会把诊断
            // 本身变成延迟源。首溢出 + 每 64 个累计溢出保留审计面。
            if dropped == 1 || dropped % 64 == 0 {
                eprintln!(
                    "WMIDROP bad sid={} window={} dropped={}",
                    sid, window, dropped
                );
            }
        } else if out.phase == gp::PHASE_DOWN {
            println!("WMROUTE ok sid={} window={}", sid, window);
        }
        if clear_lock {
            self.input_lock = None;
        }
    }

    /// P2：键盘事件按权威 focus 窗路由；只有该窗的 0x67 合约声明
    /// INPUT_CLASS_KEYBOARD 才入队。默认 Retained 无显式合约 = fail closed。
    fn route_key(&mut self, table: &mut table::WmTable, ev: &gp::KeyEvent) {
        if matches!(ev.code, gp::KEY_SYSTEM_BACK | gp::KEY_SYSTEM_HOME | gp::KEY_SYSTEM_TASK) {
            if ev.action != gp::KEY_ACTION_DOWN {
                return;
            }
            let version_before = table.version();
            let (name, action, owner) = if ev.code == gp::KEY_SYSTEM_BACK {
                let owner = table.navigation_back();
                ("back", gp::SYSTEM_NAV_ACTION_BACK, owner)
            } else if ev.code == gp::KEY_SYSTEM_HOME {
                let count = table.navigation_home();
                ("home", gp::SYSTEM_NAV_ACTION_HOME, Some(count as u16))
            } else {
                // Recents 是系统 UI，不先抢走 APP 状态；launcher 收到 TASK
                // action 后以 SYSTEM overlay 显示权威 owner 列表。
                let owner = table.recents_owners().count.min(u16::MAX as usize) as u16;
                ("task", gp::SYSTEM_NAV_ACTION_TASK, Some(owner))
            };
            println!("WMNAV ok {name} owner={}", owner.unwrap_or(0));
            // HOME is the canonical desktop state even when no APP window was
            // visible (for example the user swiped to desktop page 1). WmTable
            // version intentionally stays unchanged in that no-op case, so the
            // SYSTEM launcher still needs the action event to snap back to 0.
            if table.version() != version_before
                || ev.code == gp::KEY_SYSTEM_HOME
                || ev.code == gp::KEY_SYSTEM_TASK
            {
                self.invalidate_system_surface(action);
            }
            return;
        }
        let Some((sid, window)) = table.focused() else {
            println!("WMKEYDROP bad sid=0 window=0 reason=focus code={}", ev.code);
            return;
        };
        let Some(i) = self.slot_index(sid) else {
            println!("WMKEYDROP bad sid={} window={} reason=session code={}", sid, window, ev.code);
            return;
        };
        let allowed = self.slots[i]
            .as_ref()
            .map(|s| {
                s.surface_modes
                    .iter()
                    .flatten()
                    .any(|info| info.surface_id == window
                        && info.input_classes & gp::surface::INPUT_CLASS_KEYBOARD != 0)
            })
            .unwrap_or(false);
        if !allowed {
            println!(
                "WMKEYDROP bad sid={} window={} reason=class code={}",
                sid, window, ev.code
            );
            return;
        }
        let sess = self.slots[i].as_mut().unwrap();
        if !sess.keys.push(*ev) {
            println!(
                "WMKEYDROP bad sid={} window={} dropped={} code={}",
                sid, window, sess.keys.dropped, ev.code
            );
            return;
        }
        println!(
            "WMKEYROUTE ok sid={} window={} code={} action={}",
            sid, window, ev.code, ev.action
        );
    }

    /// 键盘输入通道 INPUT_KEY_POLL：非阻塞批答；空批 1B ST_EAGAIN。
    fn poll_keys(&mut self, ch: Handle, max: u8) {
        let Some(i) = self.slots.iter().position(|s| {
            s.as_ref().map(|s| s.input == ch).unwrap_or(false)
        }) else {
            st_rep(ch, gp::ST_BAD);
            return;
        };
        let sess = self.slots[i].as_mut().unwrap();
        let mut events = [zero_key_event(); gp::INPUT_KEY_EVENTS_MAX];
        let n = sess.keys.pop_into(&mut events[..max as usize]);
        if n == 0 {
            let mut rep = [0u8; 1];
            let m = gp::encode_status_rep(&mut rep, gp::ST_EAGAIN);
            send_rep(ch, &rep[..m]);
            return;
        }
        let mut rep = [0u8; gp::INPUT_KEY_ROUTE_MAX];
        let m = match gp::encode_input_key_events_rep(&mut rep, &events[..n]) {
            Some(m) => m,
            None => trap(),
        };
        send_rep(ch, &rep[..m]);
    }

    /// 输入专通道 INPUT_POLL：非阻塞批答。空批 ST_EAGAIN 由客户端重臂。
    fn poll_input(&mut self, ch: Handle, max: u8) {
        let Some(i) = self.slots.iter().position(|s| {
            s.as_ref().map(|s| s.input == ch).unwrap_or(false)
        }) else {
            st_rep(ch, gp::ST_BAD);
            return;
        };
        let sess = self.slots[i].as_mut().unwrap();
        let mut events = [zero_event(); gp::INPUT_EVENTS_MAX];
        let n = sess.queue.pop_into(&mut events[..max as usize]);
        if n == 0 {
            let mut rep = [0u8; 1];
            let m = gp::encode_status_rep(&mut rep, gp::ST_EAGAIN);
            send_rep(ch, &rep[..m]);
            return;
        }
        let mut rep = [0u8; gp::INPUT_REP_MAX];
        let m = match gp::encode_input_events_rep(&mut rep, &events[..n]) {
            Some(m) => m,
            None => trap(),
        };
        send_handoff_rep(ch, &rep[..m]);
    }

    /// INPUT_ROUTE deposits can queue while the producer already has an
    /// INPUT_POLL armed. Flush that armed poll immediately instead of letting a
    /// continuous push stream defer every session to a later serve pass.
    fn flush_armed_input_polls(&mut self) {
        for i in 0..self.slots.len() {
            let Some(sess) = self.slots[i].as_ref() else {
                continue;
            };
            let input_ch = sess.input;
            let Some((msg, _hs)) = poll(input_ch) else {
                continue;
            };
            if msg.0.first() == Some(&gp::OP_INPUT_KEY_POLL) {
                match gp::parse_input_key_poll_req(&msg.0[..msg.1]) {
                    Some(max) => self.poll_keys(input_ch, max),
                    None => st_rep(input_ch, gp::ST_BAD),
                }
            } else {
                match gp::parse_input_poll_req(&msg.0[..msg.1]) {
                    Some(max) => self.poll_input(input_ch, max),
                    None => st_rep(input_ch, gp::ST_BAD),
                }
            }
        }
    }

    /// poll-first 死亡探测：Process TERMINATED 即摘 session、清 owner 窗。
    fn reap_dead(&mut self, table: &mut table::WmTable, comp: &mut relay::Compositor) {
        let mut i = 0;
        while i < self.slots.len() {
            let Some(sess) = self.slots[i].as_ref() else {
                i += 1;
                continue;
            };
            let mut observed = 0u64;
            let st = berry_object_wait_one(
                sess.process,
                SIGNAL_TERMINATED,
                0,
                &mut observed,
            );
            if st == BERRY_ERR_AGAIN {
                i += 1;
                continue;
            }
            if st != 0 {
                trap();
            }
            let sess = self.slots[i].take().expect("checked slot");
            // 先丢 parking 位，再 destroy_owned：后者会 bump 并可能向 parked
            // 通道应答快照；避免对将关 handle 的时序依赖。
            table.forget_parked(sess.ch);
            let wins = table.destroy_owned(sess.id);
            comp.forget_owner_cache(sess.id);
            if self.input_lock.map(|l| l.session == sess.id).unwrap_or(false) {
                self.input_lock = None;
            }
            println!("WMDEAD ok id={} wins={}", sess.id, wins);
            if berry_handle_close(sess.ch) != 0
                || berry_handle_close(sess.input) != 0
                || berry_handle_close(sess.process) != 0
            {
                trap();
            }
        }
    }
}


/// 为一个 APP 构建私有字形 VMO。HD A8 表由 berry-gfx 单源提供；每次
/// connect 独立填充，避免 v1 UserRw 映射下跨 app 写穿。失败 = GUI 关键
/// 能力断链，trap loud（WMCONN 缺失由 smoke 兜底）。
fn make_font_vmo() -> Handle {
    let mut vmo: Handle = 0;
    if berry_vmo_create(FONT_VMO_LEN as u64, 0, &mut vmo) != 0
        || vmo == 0
        || berry_vmo_commit(vmo, 0, FONT_VMO_LEN as u64) != 0
    {
        trap();
    }
    for ch in 32u8..=126 {
        let Some(g) = FontAtlas32x48.coverage(ch) else {
            trap();
        };
        let off = u64::try_from(usize::from(ch - 32) * FONT_GLYPH_BYTES)
            .unwrap_or_else(|_| trap());
        if g.coverage.len() != FONT_GLYPH_BYTES
            || berry_vmo_write(vmo, g.coverage.as_ptr(), off, g.coverage.len()) != 0
        {
            trap();
        }
    }
    vmo
}

/// WmTable 的 guest I/O 供给：表状态机在 lib 面，本实现只提供真实
/// channel/clock/fatal 面，host harness 不复用第二套策略。
struct GuestTableIo;

impl table::WmIo for GuestTableIo {
    fn now_ns(&self) -> u64 {
        now_ns()
    }

    fn send(&self, channel: berry_sys::Handle, reply: &[u8]) {
        send_rep(channel, reply)
    }

    fn status(&self, channel: berry_sys::Handle, status: u8) {
        st_rep(channel, status)
    }

    fn close_handles(&self, handles: &[berry_sys::Handle]) {
        close_all(handles)
    }

    fn diag_wrap_clear(&self) {
        println!("WMCONFIGWRAP clear");
    }

    fn trap(&self) -> ! {
        trap()
    }
}

static SYSTEM_UI_PROGRESS_READY: AtomicBool = AtomicBool::new(false);
static SYSTEM_APP_FRAME_OWNER: core::sync::atomic::AtomicU16 = core::sync::atomic::AtomicU16::new(0);

static GUEST_TABLE_IO: GuestTableIo = GuestTableIo;

/// 单调钟现值（失败 = clock 面断链，loud）。
pub(crate) fn now_ns() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        trap();
    }
    t
}

/// 应答 send（无随行 handle 面；EBUSY → wait WRITABLE 重臂重验）。与
/// guifbd 无限重臂不同：**总帽 10s 到点 trap**（§3 #6 失败面——「持续
/// EBUSY = trap」，对端不排空 10s = 链病态，不留无限空转面）。
pub(crate) fn send_rep(h: Handle, rep: &[u8]) {
    let deadline = now_ns() + WMD_IDLE_CAP_NS;
    loop {
        let s = berry_channel_send(h, rep.as_ptr(), rep.len(), core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            println!("gui-wmd: send 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            println!("gui-wmd: send busy 帽到点");
            trap(); // 持续 EBUSY（10s 未排空）= fail loud
        }
        // 命中 WRITABLE → 下一轮 send 重验（obs 不作判据——重验循环兜底）
    }
}

/// 应答 send（随行 handle 面——快照/present 域中继应答：handle 随应答
/// 迁回客户端表）；busy 帽语义同 [`send_rep`]。
pub(crate) fn send_rep_handles(h: Handle, rep: &[u8], hs: &[Handle]) {
    let deadline = now_ns() + WMD_IDLE_CAP_NS;
    loop {
        let s = berry_channel_send(h, rep.as_ptr(), rep.len(), hs.as_ptr(), hs.len());
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            println!("gui-wmd: send 失败 {}", s);
            trap();
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            println!("gui-wmd: send busy 帽到点");
            trap();
        }
    }
}

/// Cooperative reply handoff for a client that is known to be waiting on this
/// response (FRAME_CALLBACK or INPUT_POLL). SUBMIT ACK stays ordinary so a
/// producer cannot enqueue ahead of wmd's pending drain.
pub(crate) fn send_handoff_rep(h: Handle, rep: &[u8]) {
    send_rep(h, rep);
    let _ = berry_thread_yield();
}
/// 1B 状态应答（非 OK 烘焙线 + DESTROY/SET 成功形）。
pub(crate) fn st_rep(h: Handle, st: u8) {
    let mut out = [0u8; 1];
    let n = gp::encode_status_rep(&mut out, st);
    send_rep(h, &out[..n]);
}

/// close 请求随行迁入的全部 handle（拒收公共出口——坏客户端不得杀服务，
/// close 失败忽略）。
pub(crate) fn close_all(hs: &[Handle]) {
    for &h in hs {
        let _ = berry_handle_close(h);
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （guifbd 同款——链断 loud，标记缺失 = smoke FAIL 兜底）。
#[cold]
pub(crate) fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（0x4000_0000 与用户布局无交集——家族惯例值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}

impl WmSession {
    /// 0x67：校验窗口 owner/可见性后调用纯策略并记录逐窗合约。
    fn configure_surface(
        &mut self,
        table: &table::WmTable,
        req: gp::surface::SurfaceModeReq,
    ) -> Result<gp::surface::SurfaceModeInfo, u8> {
        match table.config_ready(self.id, req.surface_id) {
            None => return Err(gp::ST_NOENT),
            Some(false) => return Err(gp::ST_DENIED),
            Some(true) => {}
        }
        let Some(geometry) = table.surface_geometry(self.id, req.surface_id) else {
            return Err(gp::ST_NOENT);
        };
        let old = self
            .surface_modes
            .iter()
            .find(|e| e.map(|i| i.surface_id) == Some(req.surface_id))
            .copied()
            .flatten();
        let (w, h) = table.display_size();
        let mut info = gp::surface::surface_mode_session_policy(
            self.client_type, req, self.escape, old, w, h,
        )?;
        if self.client_type == gp::WM_CLIENT_APP {
            // R2：persistent occlusion 以 raw surface-local 为基准写入既有
            // safe_* 字段。它是 host 授予值，不是 app 自报；IME/transient
            // occlusion 不在此通道。
            let (left, top, right, bottom) = geometry.occlusion_insets();
            info.safe_left = left;
            info.safe_top = top;
            info.safe_right = right;
            info.safe_bottom = bottom;
        }
        let (raw_w, raw_h) = geometry.raw_size();
        if !gp::surface::surface_mode_info_fits(&info, raw_w, raw_h) {
            return Err(gp::ST_BAD);
        }
        if let Some(slot) = self
            .surface_modes
            .iter_mut()
            .find(|e| e.map(|i| i.surface_id) == Some(req.surface_id))
        {
            *slot = Some(info);
        } else if let Some(slot) = self.surface_modes.iter_mut().find(|e| e.is_none()) {
            *slot = Some(info);
        } else {
            return Err(gp::ST_FULL);
        }
        Ok(info)
    }

    fn clear_surface_mode(&mut self, surface_id: u16) {
        for entry in self.surface_modes.iter_mut() {
            if entry.map(|i| i.surface_id) == Some(surface_id) {
                *entry = None;
            }
        }
    }
}

/// broker 面只收 WM_CONNECT；present/hdi/wm 业务必须走 per-session 通道。
fn on_broker(sessions: &mut WmSessions, ch: Handle, msg: &[u8], hs: &[Handle]) {
    if msg.first() == Some(&gp::OP_WM_SURFACE_ESCAPE_SEED) {
        if !hs.is_empty() {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        match gp::surface::parse_wm_surface_escape_seed_req(msg) {
            Some(seed) if sessions.set_escape_seed(seed) => st_rep(ch, gp::ST_OK),
            _ => st_rep(ch, gp::ST_DENIED),
        }
        return;
    }
    if msg.first() == Some(&gp::OP_WM_SYSTEM_SEED) {
        if !hs.is_empty() {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        match gp::surface::parse_wm_system_seed_req(msg) {
            Some(seed) if sessions.set_system_seed(seed) => st_rep(ch, gp::ST_OK),
            _ => st_rep(ch, gp::ST_DENIED),
        }
        return;
    }
    if msg.first() == Some(&gp::OP_WM_CONNECT) {
        sessions.connect(ch, msg, hs);
        return;
    }
    close_all(hs);
    st_rep(ch, gp::ST_BAD);
}

/// session 面分发：present/hdi relay；wm 域带 owner 执法。
fn on_session(table: &mut table::WmTable, present_cli: Handle, hdi_cli: Handle,
              comp: &mut relay::Compositor, sess: &mut WmSession,
              msg: &[u8], hs: &[Handle]) {
    let ch = sess.ch;
    match msg.first() {
        Some(&gp::OP_SUBMIT) => {
            let owner = sess.id;
            let client_type = sess.client_type;
            let submitted = relay::submit_composite(
                ch,
                present_cli,
                owner,
                client_type,
                table.focused_app_owner().is_some(),
                |owner, id| table.surface_geometry(owner, id),
                |owner| table.owner_windows(owner),
                comp,
                msg,
                hs,
            );
            if submitted
                && client_type == gp::WM_CLIENT_APP
                && !sess.frame_notified
            {
                sess.frame_notified = true;
                SYSTEM_APP_FRAME_OWNER.store(owner, Ordering::Relaxed);
            }
        }
        Some(&op) if (gp::OP_GETINFO..=gp::OP_POWER_MODE).contains(&op) => {
            relay::relay(ch, present_cli, msg, hs);
        }
        Some(&op) if (gp::OP_REG_VBLANK..=gp::OP_COMMIT).contains(&op) => {
            relay::relay(ch, hdi_cli, msg, hs);
        }
        Some(&gp::OP_WM_SURFACE_MODE) => {
            if !hs.is_empty() {
                close_all(hs);
                st_rep(ch, gp::ST_BAD);
                return;
            }
            match gp::surface::parse_wm_surface_mode_req(msg) {
                Some(req) => match sess.configure_surface(table, req) {
                    Ok(info) => {
                        let mut out = [0u8; gp::surface::SURFACE_MODE_REP_LEN];
                        match gp::surface::encode_wm_surface_mode_rep(&mut out, &info) {
                            Some(n) => send_rep(ch, &out[..n]),
                            None => trap(),
                        }
                    }
                    Err(st) => st_rep(ch, st),
                },
                None => st_rep(ch, gp::ST_BAD),
            }
        }
        Some(&gp::OP_WM_TASK_SELECT) => match gp::parse_wm_task_select_req(msg) {
            Some(req) if sess.client_type == gp::WM_CLIENT_SYSTEM => {
                if hs.is_empty() && table.task_select(req.owner).is_some() {
                    let (windows, window_n) = table.owner_windows(req.owner);
                    relay::replay_owner(present_cli, comp, req.owner, windows, window_n);
                    st_rep(ch, gp::ST_OK);
                } else {
                    close_all(hs);
                    st_rep(ch, gp::ST_DENIED);
                }
            }
            Some(_) => {
                close_all(hs);
                st_rep(ch, gp::ST_DENIED)
            }
            None => {
                close_all(hs);
                st_rep(ch, gp::ST_BAD)
            }
        },
        Some(&op) if (gp::OP_WM_GETINFO..=0x7F).contains(&op) => {
            // CREATE/SET 会先改有效遮挡几何；在表变更前捕获 fresh APP 的
            // 真实可见像素，避免 Recents overlay 尚未 submit 时 eff 已变空。
            if matches!(op, gp::OP_WM_CREATE | gp::OP_WM_SET)
                && comp.fresh_owner().is_some_and(|fresh| fresh != sess.id)
            {
                let fresh = comp.fresh_owner().expect("checked");
                let (windows, window_n) = table.owner_windows(fresh);
                comp.capture_fresh_owner(windows, window_n);
            }
            if msg.first() == Some(&gp::OP_WM_DESTROY) {
                if let Some(id) = gp::parse_wm_destroy_req(msg) {
                    table.handle_wm(ch, sess.id, sess.client_type, msg, hs);
                    sess.clear_surface_mode(id);
                    return;
                }
            }
            table.handle_wm(ch, sess.id, sess.client_type, msg, hs);
        }
        _ => {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
        }
    }
}

/// push_srv 帧分发：inputd 面收 TABLE_WAIT 与 Phase 3 INPUT_ROUTE；
/// 其余帧（含 wm op 错投）1B ST_BAD + close 随行。
fn on_push(
    table: &mut table::WmTable,
    sessions: &mut WmSessions,
    ch: Handle,
    msg: &[u8],
    hs: &[Handle],
) {
    if msg.first() == Some(&gp::OP_INPUT_ROUTE) {
        if !hs.is_empty() {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        match gp::parse_input_route_req(msg) {
            Some(route) => {
                for i in 0..route.count() {
                    if let Some(ev) = route.event(i) {
                        sessions.route(table, &ev);
                    }
                }
                sessions.flush_armed_input_polls();
            }
            None => st_rep(ch, gp::ST_BAD),
        }
        return;
    }
    if msg.first() == Some(&gp::OP_INPUT_KEY_ROUTE) {
        if !hs.is_empty() {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        match gp::parse_input_key_route_req(msg) {
            Some(route) => {
                for i in 0..route.count() {
                    if let Some(ev) = route.event(i) {
                        sessions.route_key(table, &ev);
                    }
                }
            }
            None => st_rep(ch, gp::ST_BAD),
        }
        return;
    }
    if !hs.is_empty() {
        close_all(hs);
        st_rep(ch, gp::ST_BAD);
        return;
    }
    match gp::parse_wm_table_wait_req(msg) {
        Some(last_seen) => table.wait(ch, last_seen),
        None => st_rep(ch, gp::ST_BAD),
    }
}

pub fn run() {
    // —— 启动序①：收割四端（extras 4 枚恰 8/8——Q12 冻结面）。0 值 =
    // 协议破缺（EFI 链 svcmgr arm Ok 恒随行）：eprintln + park 兜底。
    let wm_srv = wm_srv_h();
    if wm_srv == 0 {
        eprintln!("gui-wmd: no wm srv gift");
        berry_libc::park();
    }
    let push_srv = wm_push_srv_h();
    if push_srv == 0 {
        eprintln!("gui-wmd: no push srv gift");
        berry_libc::park();
    }
    let present_cli = wm_present_h();
    if present_cli == 0 {
        eprintln!("gui-wmd: no present cli gift");
        berry_libc::park();
    }
    let hdi_cli = wm_hdi_h();
    if hdi_cli == 0 {
        eprintln!("gui-wmd: no hdi cli gift");
        berry_libc::park();
    }

    // —— 启动序②：present 首发 RELEASE_WAIT 清前世孤儿别名（guifbd
    // boot 期空转即答——无长 park 边界问题；应答随行归还别名全量即关）。
    relay::boot_release_all(present_cli);

    // —— 启动序③：present GETINFO 缓存几何（快照头/wm GETINFO 的
    // disp_w/disp_h 域同源取值）。
    let info = relay::boot_getinfo(present_cli);
    // wm wire 几何域 = u16——表示不了的屏 = 协议破缺面（宁可不起）。
    if info.display_w > u16::MAX as u32 || info.display_h > u16::MAX as u32 {
        eprintln!("gui-wmd: display {}x{} over u16", info.display_w, info.display_h);
        berry_libc::park();
    }

    // —— 启动序④：表初始化（空表 + version≥1：counter 起 1 → 冷启动
    // WAIT(0) 恒立即答；epoch = 启动 CNTVCT 派生 u8——无持久化下唯一跨
    // 启动区分源）。
    let mut table = table::WmTable::new(&GUEST_TABLE_IO, info.display_w as u16, info.display_h as u16);
    let mut sessions = WmSessions::new();
    let mut compositor =
        relay::Compositor::new(info.display_w, info.display_h);

    // —— 启动序⑤：Phase 2 合成 VMO（wmd 成为 guifbd 唯一 present
    // client）+ 诊断行转 serve（小写 wmd 前缀——永不进 MARKERS）。
    println!("gui-wmd: up");

    // —— serve：broker/push/session 三族 poll-first；全 EAGAIN 才
    // wait_many（session 通道 READABLE，Process handle TERMINATED）。
    loop {
        sessions.reap_dead(&mut table, &mut compositor);
        if let Some((msg, hs)) = poll(wm_srv) {
            on_broker(&mut sessions, wm_srv, &msg.0[..msg.1], &hs.0[..hs.1]);
            if SYSTEM_UI_PROGRESS_READY.swap(false, Ordering::Relaxed) {
                relay::boot_progress_system_ui_once(present_cli);
            }
            continue;
        }
        if let Some((msg, hs)) = poll(push_srv) {
            on_push(&mut table, &mut sessions, push_srv, &msg.0[..msg.1], &hs.0[..hs.1]);
            continue;
        }
        let mut handled = false;
        for i in 0..sessions.slots.len() {
            let Some(sess) = sessions.slots[i].as_ref() else { continue };
            let (session_ch, input_ch) = (sess.ch, sess.input);
            if let Some((msg, _hs)) = poll(input_ch) {
                if msg.0.first() == Some(&gp::OP_INPUT_KEY_POLL) {
                    match gp::parse_input_key_poll_req(&msg.0[..msg.1]) {
                        Some(max) => sessions.poll_keys(input_ch, max),
                        None => st_rep(input_ch, gp::ST_BAD),
                    }
                } else {
                    match gp::parse_input_poll_req(&msg.0[..msg.1]) {
                        Some(max) => sessions.poll_input(input_ch, max),
                        None => st_rep(input_ch, gp::ST_BAD),
                    }
                }
                handled = true;
                continue;
            }
            if let Some((msg, hs)) = poll(session_ch) {
                if msg.0.first() == Some(&gp::OP_INPUT_POLL) {
                    match gp::parse_input_poll_req(&msg.0[..msg.1]) {
                        Some(max) => sessions.poll_input(input_ch, max),
                        None => st_rep(input_ch, gp::ST_BAD),
                    }
                } else if msg.0.first() == Some(&gp::OP_INPUT_KEY_POLL) {
                    match gp::parse_input_key_poll_req(&msg.0[..msg.1]) {
                        Some(max) => sessions.poll_keys(input_ch, max),
                        None => st_rep(input_ch, gp::ST_BAD),
                    }
                } else {
                    let slot = sessions.slots[i].as_mut().unwrap();
                    on_session(
                        &mut table, present_cli, hdi_cli, &mut compositor,
                        slot, &msg.0[..msg.1], &hs.0[..hs.1],
                    );
                    let frame_owner = SYSTEM_APP_FRAME_OWNER.swap(0, Ordering::Relaxed);
                    if frame_owner != 0 {
                        sessions.notify_system_app_frame(frame_owner);
                    }
                }
                handled = true;
                continue;
            }
        }
        if handled {
            continue;
        }

        // wait_many 内核帽 16：broker+push + 7×(WM RPC/input) 恰 16。
        // Process TERMINATED 不入等待集；serve 顶 poll-first + 10s idle 帽
        // 兜死亡探测（无事件最长 10s 清理，事件路径即时）。
        let mut hs = [0 as Handle; 16];
        let mut sigs = [0u64; 16];
        hs[0] = wm_srv;
        sigs[0] = SIGNAL_READABLE;
        hs[1] = push_srv;
        sigs[1] = SIGNAL_READABLE;
        let mut n = 2usize;
        for sess in sessions.slots.iter().flatten() {
            hs[n] = sess.ch;
            sigs[n] = SIGNAL_READABLE;
            n += 1;
            hs[n] = sess.input;
            sigs[n] = SIGNAL_READABLE;
            n += 1;
        }
        let mut idx = 0usize;
        let mut obs = 0u64;
        let s = berry_object_wait_many(
            hs.as_ptr(),
            sigs.as_ptr(),
            n,
            now_ns() + WMD_IDLE_CAP_NS,
            &mut idx,
            &mut obs,
        );
        if s != 0 && s != BERRY_ERR_TIMEDOUT {
            trap(); // 到点 = 空闲重臂；余错 = 死亡面
        }
    }
}

/// 单路轮询（一路至多一条：两客户端面各自严格串行 RPC）。**每轮 poll 前
/// 重置 nb/nh 容量**（recv 出参 in-out，漏重置 = 容量 0 恒 -EINVAL 死
/// 循环）；出参 8 槽（wm 域零随行——非空即拒面 close；中继域随行透传）。
fn poll(ch: Handle) -> Option<(([u8; 256], usize), ([Handle; 8], usize))> {
    let mut msg = [0u8; 256];
    let mut hout = [0u32; 8];
    let mut nb = msg.len();
    let mut nh = hout.len();
    let s = berry_channel_recv(ch, msg.as_mut_ptr(), &mut nb, hout.as_mut_ptr(), &mut nh);
    if s == BERRY_ERR_AGAIN {
        return None;
    }
    if s != 0 {
        trap(); // 非 -EAGAIN 错码 = 通道链断（boot 定值接线，真死亡面）
    }
    Some(((msg, nb), (hout, nh)))
}
