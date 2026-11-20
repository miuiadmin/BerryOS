//! inputfeed —— launcher 输入焦点路由（云桌面根治刀 · 架构面 B）。
//!
//! **拓扑定谳**（2026-11-20 事故刀）：inputd input 域 = 单 FIFO 严格串行
//! RPC——多 poller 应答互截（实锤缺陷形），v1 输入拓扑 = 单订阅者
//! launcher。app 的输入因此改 **launcher 焦点代答**面：svcmgr 为每个
//! [ui] app 建 per-app 输入通道对（cli 随 gift 尾位交 app 的 `input_h()`、
//! srv 端〔Feeder〕经 APP_SPAWN ack 随行迁本进程）。app 在 Feeder 上发
//! INPUT_POLL，本路由器按覆盖焦点代答：
//!
//! - **路由带**：事件 y 落应用带（状态栏顶带与导航底带之间——几何镜像
//!   guifbd `band_geometry` 单源注：big 档 117/135px = scene.rs OH_STATUS_H
//!   ×OH_DENSITY / desktop.rs NAV_BAR_H_BIG；紧凑 44/44 = scene.rs 130 /
//!   desktop.rs NAV_KEY_PX。改值须与 guifbd/scene/desktop 三源同步）之内
//!   → 归 app；带外（状态栏/导航条）恒 launcher——导航三键是系统级保留
//!   面（裁决四-c），覆盖态逃生门恒活。
//! - **手势粘滞**（inputd Q10 同构）：DOWN 锁定目标 feeder，后续
//!   MOVE/UP/CANCEL 全投锁定者（跟手不跳窗）；UP/CANCEL 解锁。
//! - **焦点开关**（[`Router::on`]）：最近一次成功 APP_SPAWN（ack 随行
//!   Feeder 注册）置位；back/home/覆盖收盖（launcher 导航动作 = 「回桌
//!   面」语义）复位——回桌面后应用带点击回落桌面命中面（spawn/翻页），
//!   不再喂给不可见 app。v1 单可见 app 模型下与像素面（谁最后画谁可见）
//!   一致。
//! - **代答面**：app 的 INPUT_POLL 在席时事件即答（单事件
//!   `encode_input_events_rep`——inputd 应答同帧形）；未在席缓冲
//!   （[`FEED_BUF`] 满丢最旧——poll 常驻在席，缓冲只是 arm 间隙的弹性）。
//!   **不代答 EAGAIN**（app 侧会立即重挂 → busy 环）；无事件时 poll 驻留
//!   （app 阻塞等待——inputd 阻塞批拉同语义）。
//! - **feeder 生命周期**：满表（[`FEEDERS_MAX`] = svcmgr
//!   APP_SPAWN_LIVE_MAX 同值）关新柄 loud（app 静态页续活）；通道错码
//!   （app 退出）摘除；手势锁定者死亡 = 手势中断回落本地。
//!
//! 纯状态面（I/O 调用点在 frameloop——recv/send 由彼处自守）：路由判定/
//! 缓冲序/开关真值全纯函数，host 单测直盖。

use berry_guiproto as gp;
use berry_sys::Handle;
use berry_ui::hit::PtrEvent;

/// feeder 表帽（= svcmgr `APP_SPAWN_LIVE_MAX` 同值——活 app 上界即 feeder
/// 上界；满 = 新柄关收 loud，app 静态页续活）。
pub const FEEDERS_MAX: usize = 4;

/// 每 feeder 待投缓冲深（poll arm 间隙弹性；满丢最旧）。
pub const FEED_BUF: usize = 4;

/// 单个 app 输入 feeder（通道端 + 代答状态）。
pub struct Feeder {
    /// 通道端（app 侧持 cli 端发 INPUT_POLL——本端收请求/答事件）。
    pub h: Handle,
    /// app 的 INPUT_POLL 在席（true = 下一事件即答）。
    pub poll: bool,
    /// 待投事件缓冲（oldest → newest；poll 未在席时暂存）。
    buf: [PtrEvent; FEED_BUF],
    n: usize,
}

impl Feeder {
    pub fn new(h: Handle) -> Self {
        Feeder { h, poll: false, buf: [zero_event(); FEED_BUF], n: 0 }
    }

    /// 缓冲压入（满丢最旧——保新语义，手势尾事件优先）。
    pub fn push(&mut self, ev: PtrEvent) {
        if self.n == FEED_BUF {
            self.buf.copy_within(1..FEED_BUF, 0);
            self.n -= 1;
        }
        self.buf[self.n] = ev;
        self.n += 1;
    }

    /// 弹出至多 `out.len()`（帽 [`gp::INPUT_EVENTS_MAX`]）个待投事件进
    /// `out`（oldest 先出——时序保真），返回实际条数；剩余前移。
    pub fn pop_into(&mut self, out: &mut [PtrEvent]) -> usize {
        let k = self.n.min(out.len()).min(gp::INPUT_EVENTS_MAX);
        out[..k].copy_from_slice(&self.buf[..k]);
        self.buf.copy_within(k..FEED_BUF, 0);
        self.n -= k;
        k
    }
}

/// 路由器（LoopHost 持有；frameloop I/O 消费面）。
pub struct Router {
    /// 焦点开关（最近成功 spawn 置位 / 导航「回桌面」动作复位）。
    pub on: bool,
    /// 手势粘滞锁定（feeder 槽位；None = 无在途手势）。
    pub gesture: Option<usize>,
    /// feeder 表（表序 = 注册序；最新 spawn 恒在最后一个 Some 槽）。
    pub feeders: [Option<Feeder>; FEEDERS_MAX],
}

impl Router {
    pub fn new() -> Self {
        Router { on: false, gesture: None, feeders: [const { None }; FEEDERS_MAX] }
    }

    /// 注册 feeder（ack 随行柄）：满表 = false（调用方关收 loud）；成功 =
    /// 焦点开关置位（新 app 即覆盖焦点——v1 单可见 app 模型）。
    pub fn register(&mut self, h: Handle) -> bool {
        for slot in self.feeders.iter_mut() {
            if slot.is_none() {
                *slot = Some(Feeder::new(h));
                self.on = true;
                return true;
            }
        }
        false
    }

    /// 摘除槽位（通道死亡面；保序紧凑——表序 = 注册序不变量）。
    pub fn remove(&mut self, i: usize) {
        if i < FEEDERS_MAX {
            self.feeders[i] = None;
            if self.gesture == Some(i) {
                self.gesture = None; // 锁定者死亡——手势中断回落
            }
        }
    }

    /// 最新注册的活 feeder 槽位（覆盖焦点的 v1 近似——最后 spawn 者即
    /// 像素可见者）。None = 无活 app。
    pub fn newest(&self) -> Option<usize> {
        (0..FEEDERS_MAX).rev().find(|&i| self.feeders[i].is_some())
    }

    /// 「回桌面」复位（back/home/覆盖收盖拍调用）：焦点开关断 + 手势清
    /// （导航动作在 Click 拍 = UP 已完，不裂手势）。
    pub fn retire(&mut self) {
        self.on = false;
        self.gesture = None;
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

/// 应用带几何（guifbd `band_geometry` 镜像——见模块头单源注；返回
/// (顶带高, 底带高)）。满档判据同源 vp_w>=600 ∧ vp_h>=1800。
pub fn band_geometry(w: u32, h: u32) -> (u32, u32) {
    let big = w >= 600 && h >= 1800;
    if big { (117, 135) } else { (44, 44) }
}

/// 事件是否落应用带内（路由判据——顶/底带恒归 launcher 系统面）。
pub fn in_app_strip(y: u32, vp_w: u32, vp_h: u32) -> bool {
    let (top, bot) = band_geometry(vp_w, vp_h);
    y >= top && y + bot < vp_h
}

/// 路由判定纯函数（on_input 门的核心真值表）：
/// - 开关断 / 无活 feeder → None（本地消费）；
/// - 手势锁定中 → 锁定槽（维持到 UP/CANCEL——调用方解锁）；
/// - DOWN 落应用带 → 最新 feeder（新锁）；DOWN 在带外 → None（导航/
///   状态栏恒本地）；
/// - 非 DOWN 未锁定（DOWN 丢失角）→ None 本地（保守）。
pub fn route(router: &Router, ev: &PtrEvent, vp_w: u32, vp_h: u32) -> Option<usize> {
    if !router.on {
        return None;
    }
    if let Some(i) = router.gesture {
        if router.feeders[i].is_some() {
            return Some(i);
        }
    }
    if ev.phase == gp::PHASE_DOWN && in_app_strip(ev.y, vp_w, vp_h) {
        return router.newest();
    }
    None
}

/// 零值指针事件（缓冲/批量暂存的定容初始化——非语义面）。
pub const fn zero_event() -> PtrEvent {
    PtrEvent {
        phase: 0,
        pointer_id: 0,
        x: 0,
        y: 0,
        ts_ns: 0,
        buttons: 0,
        pressure_mi: 0,
    }
}

/// PtrEvent → wire PointerEvent（代答帧构造——字段镜像 from_wire 的逆；
/// device_id=0 指针面、history 空（代答面逐事件投递无重采样窗）。
pub fn to_wire(ev: &PtrEvent) -> gp::PointerEvent {
    let zero = gp::Sample { timestamp_ns: 0, x: 0, y: 0 };
    gp::PointerEvent {
        phase: ev.phase,
        device_id: 0,
        buttons: ev.buttons,
        flags: 0,
        pointer_id: ev.pointer_id,
        timestamp_ns: ev.ts_ns,
        x: ev.x,
        y: ev.y,
        pressure: ev.pressure_mi,
        contact_major: 0,
        contact_minor: 0,
        history: [zero; gp::INPUT_SAMPLES_MAX - 1],
        history_len: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(phase: u8, y: u32) -> PtrEvent {
        PtrEvent {
            phase,
            pointer_id: 1,
            x: 400,
            y,
            ts_ns: 1_000_000,
            buttons: gp::BUTTON_L,
            pressure_mi: 500,
        }
    }

    /// 紧凑档 800x600：应用带 = y∈[44, 556)——带内 DOWN 路由、带外本地。
    #[test]
    fn strip_geometry_routes_between_bands() {
        assert!(in_app_strip(44, 800, 600));
        assert!(in_app_strip(300, 800, 600));
        assert!(in_app_strip(555, 800, 600));
        assert!(!in_app_strip(43, 800, 600)); // 状态栏带
        assert!(!in_app_strip(556, 800, 600)); // 导航带
        assert!(!in_app_strip(0, 800, 600));
        assert!(!in_app_strip(599, 800, 600));
        // big 档 1080x1920：带 = [117, 1785)。
        assert!(in_app_strip(500, 1080, 1920));
        assert!(!in_app_strip(1800, 1080, 1920));
        assert!(!in_app_strip(100, 1080, 1920));
    }

    /// 路由真值表：开关断恒 None；DOWN 带内锁最新；粘滞期全投锁定者；
    /// DOWN 带外（导航键）恒本地；非 DOWN 未锁本地。
    #[test]
    fn route_truth_table() {
        let mut r = Router::new();
        // 开关断（无 spawn）：一切本地。
        assert_eq!(route(&r, &ev(gp::PHASE_DOWN, 300), 800, 600), None);

        // 注册两 feeder（槽 0/1——最新 = 1）。
        assert!(r.register(90));
        assert!(r.register(91));
        assert!(r.on);
        // DOWN 带内 → 最新槽。
        assert_eq!(route(&r, &ev(gp::PHASE_DOWN, 300), 800, 600), Some(1));
        // DOWN 带外（导航条）→ 本地（导航三键恒活）。
        assert_eq!(route(&r, &ev(gp::PHASE_DOWN, 580), 800, 600), None);
        // 锁定槽 0（手势粘滞）：后续 MOVE 即使带外也投 0。
        r.gesture = Some(0);
        assert_eq!(route(&r, &ev(gp::PHASE_MOVE, 580), 800, 600), Some(0));
        assert_eq!(route(&r, &ev(gp::PHASE_UP, 300), 800, 600), Some(0));
        // 未锁定非 DOWN（DOWN 丢失角）→ 本地保守。
        r.gesture = None;
        assert_eq!(route(&r, &ev(gp::PHASE_MOVE, 300), 800, 600), None);

        // retire（back/home）：开关断 + 手势清。
        r.gesture = Some(0);
        r.retire();
        assert!(!r.on);
        assert_eq!(r.gesture, None);
        assert_eq!(route(&r, &ev(gp::PHASE_DOWN, 300), 800, 600), None);
    }

    /// 手势锁定者死亡：route 回落 None（本地）——remove 清 gesture。
    #[test]
    fn gesture_owner_removal_falls_back() {
        let mut r = Router::new();
        assert!(r.register(90));
        r.gesture = Some(0);
        r.remove(0);
        assert_eq!(r.gesture, None);
        assert_eq!(route(&r, &ev(gp::PHASE_MOVE, 300), 800, 600), None);
    }

    /// 满表拒注册（第 5 柄）+ 缓冲满丢最旧。
    #[test]
    fn register_cap_and_buffer_drop_oldest() {
        let mut r = Router::new();
        for i in 0..FEEDERS_MAX {
            assert!(r.register(90 + i as Handle));
        }
        assert!(!r.register(99)); // 满表
        assert_eq!(r.newest(), Some(FEEDERS_MAX - 1));

        let mut f = Feeder::new(7);
        for i in 0..(FEED_BUF + 2) {
            f.push(ev(gp::PHASE_MOVE, 100 + i as u32));
        }
        // 满丢最旧：缓冲深恒 FEED_BUF，最旧 = i=2 起。
        assert_eq!(f.n, FEED_BUF);
        let mut out = [ev(0, 0); 3];
        let k = f.pop_into(&mut out);
        assert_eq!(k, 3);
        assert_eq!(out[0].y, 102);
        assert_eq!(out[2].y, 104);
        // 剩余一条（i=5）。
        let mut out2 = [ev(0, 0); 3];
        assert_eq!(f.pop_into(&mut out2), 1);
        assert_eq!(out2[0].y, 105);
    }

    /// wire 事件镜像：字段往返（phase/x/y/ts/buttons/pressure）。
    #[test]
    fn to_wire_mirrors_fields() {
        let e = ev(gp::PHASE_DOWN, 321);
        let w = to_wire(&e);
        assert_eq!(w.phase, gp::PHASE_DOWN);
        assert_eq!(w.x, 400);
        assert_eq!(w.y, 321);
        assert_eq!(w.timestamp_ns, 1_000_000);
        assert_eq!(w.buttons, gp::BUTTON_L);
        assert_eq!(w.pressure, 500);
        assert_eq!(w.history_len, 0);
        assert_eq!(w.device_id, 0);
    }
}
