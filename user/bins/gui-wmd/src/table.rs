//! 权威窗口表（GUI-5 §3 #5 + 波3 焦点/返回路由/事件通道）——静态定容
//! （guifbd `Srv` 先例：无堆分配、无生长面）。服务状态六件：
//!
//! - **entries**：三层窗（APP/SYSTEM/SERVICE——波3 收宽 layer=2）+ Z 序 +
//!   parent/group 缝 + 单焦点 + **属主事件通道**（波3：CREATE 应答随行
//!   gift 通道端，表项记属主；DESTROY 关联关闭）；表项携**有效几何**
//!   （AVOID_SYS 避让推导后——wire 回显面），申请几何不回显但内部在案
//!   （重推导需要申请值，Q10 wire 语义钉板）。
//! - **version**：`(epoch u8 << 24) | counter u24`，counter ∈ 1..=0xFFFFFF
//!   跳 0 → **恒 ≥1**（空表初始化即 version≥1——冷启动 WAIT(0) 恒立即
//!   答）；epoch = 启动 CNTVCT 派生 u8（无持久化下唯一跨启动区分源——
//!   新 wmd 纪元的 version 与前世不可比，inputd 超龄 WAIT(0) 重探测
//!   兜底闭环，Q11-③）。
//! - **parked**：单车位 TABLE_WAIT（应答通道 + 其 last_seen）。**单车位
//!   语义钉死（R2 备注⑦）：后到 WAIT 现答现快照并覆盖 parked 不排队**
//!   ——位移者立即以现快照应答（防悬挂）。**波3 立法（波3设计 §9）：
//!   TABLE_WAIT 冻结、零新增驻留者**——表跟随一律走事件通道通知
//!   （WM_EVENT_TABLE）+ TABLE_GET 快照；inputd 推送面（push_srv）仍是
//!   唯一合法驻留者，新鲜度窗口（一次中继时长）接受为 v1 语义。
//! - **pending_back**：波3 返回路由兜底单槽（目标窗 + 截止时刻，§3 R2）
//!   ——OP_WM_BACK 向焦点窗属主投递 WM_EVENT_BACK 后武装；BACK_ACK 或
//!   任何表变更解除；500ms 到点 wmd 兜底 DESTROY（表序内建 refocus 回
//!   最末可见窗）。单槽语义同 parked：后到覆盖重臂不排队（位移者已收
//!   事件，兜底网只剩新槽——旧窗 ACK 迟到按 wid 失配忽略）。
//! - **subs**：无窗订阅者事件通道（OP_WM_SUBSCRIBE——guifbd 表快照
//!   「通知驱动 + 异步拉取」面，波3设计 §4）；与属主通道同收表变更
//!   广播。静态定容帽 [`WM_EVT_SUBS_MAX`]。
//! - **有效几何不变量式避让**：任何 CREATE/DESTROY/SET（含系统层窗
//!   变更）触发受影响表项重算 O(N)（N≤15 零成本）——AVOID_SYS 旗标窗
//!   对 layer=SYSTEM **可见**窗求差集写有效几何（**SERVICE 不挖坑**——
//!   波3设计 §5 裁决：软键盘暂态压顶，避让反致 APP 窗反复重排；层序
//!   SERVICE>SYSTEM>APP 由 hit-test layer 降序天然覆盖）；单矩形保形
//!   裁剪（四条剩余带取面积最大者：状态栏顶条 → 应用窗自顶收缩；
//!   全覆盖 → w/h=0 恒不中）。
//!
//! I/O 全走 [`Port`] 缝（host 单测 mock 客户端面）；错误码面：id/parent
//! 无效 ST_NOENT、表满 ST_FULL、野值已在 wire 层拒（guiproto parse 域
//! 执法——本表不做第二解析器）。

use crate::{Port, SendEvtStatus};
use berry_guiproto as gp;
use berry_sys::Handle;

/// 无窗订阅者事件通道帽（静态定容纪律——guifbd 面实测 1 路，4 路 = 测试
/// 与防御余量；满即 ST_FULL）。
pub const WM_EVT_SUBS_MAX: usize = 4;

/// 返回键兜底帽：WM_EVT_BACK 投递后无 BACK_ACK 且无表变更的容忍窗
/// （波3设计 §3 R2——500ms 单槽软定时，进 serve 循环零新增线程）。
pub const WMD_BACK_FALLBACK_NS: u64 = 500 * 1_000_000;

/// 内部窗口项：申请几何（推导源）+ 有效几何（wire 回显面）分域携带——
/// 表项只回显有效几何，重推导需要申请值在案。`evt` = 属主事件通道
/// （wmd 侧端；None = 旧式/降级无通道窗——BACK 兜底直接 DESTROY）。
#[derive(Clone, Copy)]
struct Win {
    id: u16,
    parent: u16,
    group: u8,
    /// 申请几何（客户端 CREATE/SET 原值——有效几何的推导输入）。
    req: (u16, u16, u16, u16),
    /// 有效几何（避让推导产物——快照回显面）。
    eff: (u16, u16, u16, u16),
    layer: u8,
    z: u8,
    vis: bool,
    avoid_sys: bool,
    /// 属主事件通道（wmd 侧端）——CREATE 建联、DESTROY 关联关闭（波3 §3）。
    evt: Option<Handle>,
}

/// 单矩形裁剪：C 减 S（半开矩形），保形取四条剩余带面积最大者（并列取
/// 序靠前——确定性）；空交 = C 原样。u32 中间量（u16+u16 越界面）；结果
/// ⊆ C ⊆ u16 值域，收窄回写安全。
fn clip_rect(c: (u32, u32, u32, u32), s: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let (x, y, w, h) = c;
    let (sx, sy, sw, sh) = s;
    let ix0 = x.max(sx);
    let iy0 = y.max(sy);
    let ix1 = (x + w).min(sx + sw);
    let iy1 = (y + h).min(sy + sh);
    if ix0 >= ix1 || iy0 >= iy1 {
        return c; // 空交——不挖坑
    }
    // 四条剩余带（top/bottom/left/right；面积 0 带合法——全覆盖兜底）。
    let bands = [
        (x, y, w, iy0 - y),
        (x, iy1, w, y + h - iy1),
        (x, y, ix0 - x, h),
        (ix1, y, x + w - ix1, h),
    ];
    let mut best = bands[0];
    let mut best_area = bands[0].2 * bands[0].3;
    for &b in &bands[1..] {
        let area = b.2 * b.3;
        if area > best_area {
            best = b;
            best_area = area;
        }
    }
    best
}

pub struct WmTable {
    wins: [Win; gp::WM_WINDOWS_MAX],
    count: usize,
    focus: Option<u16>,
    version: u32,
    next_id: u16,
    /// 单车位 TABLE_WAIT（应答通道 + 其 last_seen）——波3 冻结面（见模块注）。
    parked: Option<(Handle, u32)>,
    /// 返回路由兜底单槽（目标窗 + 截止时刻）——见模块注。
    pending_back: Option<(u16, u64)>,
    /// 无窗订阅者事件通道（定容数组，表序稳定）。
    subs: [Handle; WM_EVT_SUBS_MAX],
    subs_n: usize,
    disp_w: u16,
    disp_h: u16,
}

impl WmTable {
    /// 空表初始化：version = 新纪元 counter 1（恒 ≥1）；epoch = 启动
    /// 时刻（`boot_ns`——单调钟现值由调用方注入，Port 缝外的唯一时间
    /// 入参）派生 u8。
    pub fn new(disp_w: u16, disp_h: u16, boot_ns: u64) -> Self {
        let epoch = (boot_ns & 0xFF) as u8;
        let blank = Win {
            id: 0,
            parent: 0,
            group: 0,
            req: (0, 0, 0, 0),
            eff: (0, 0, 0, 0),
            layer: gp::WM_LAYER_APP,
            z: 0,
            vis: false,
            avoid_sys: false,
            evt: None,
        };
        WmTable {
            wins: [blank; gp::WM_WINDOWS_MAX],
            count: 0,
            focus: None,
            version: ((epoch as u32) << 24) | 1,
            next_id: 0,
            parked: None,
            pending_back: None,
            subs: [0; WM_EVT_SUBS_MAX],
            subs_n: 0,
            disp_w,
            disp_h,
        }
    }

    /// 表变更公共出口：version+1（counter 回绕跳 0——恒 ≥1 不变量）→ 向
    /// 当前 parked 应答全表快照并清空 → 解除返回兜底槽（§3 R2「无表
    /// 变更」条件——客户端以 DESTROY/SET 自证存活）→ 广播
    /// WM_EVENT_TABLE（尽力而为）。
    fn bump<P: Port>(&mut self, p: &mut P) {
        let counter = (self.version & 0x00FF_FFFF) + 1;
        let counter = if counter > 0x00FF_FFFF { 1 } else { counter };
        self.version = (self.version & 0xFF00_0000) | counter;
        self.pending_back = None;
        if let Some((ch, _)) = self.parked.take() {
            self.send_snapshot(p, ch);
        }
        self.broadcast_table(p);
    }

    /// 表变更广播（波3 §3）：WM_EVENT_TABLE(version) 非阻塞单发到全部
    /// 属主/订阅者通道——满即丢（真值恒经 TABLE_GET 快照，丢弃至多迟一
    /// 拍）；余错码 = 通道病态 → 关闭拆除（属主降级无通道窗、订阅者摘
    /// 除；内核 v1 无对端死亡检测，错码面实为防御）。
    fn broadcast_table<P: Port>(&mut self, p: &mut P) {
        let mut frame = [0u8; gp::WM_EVT_TABLE_LEN];
        let n = gp::encode_wm_evt_table(&mut frame, self.version);
        for i in 0..self.count {
            let Some(h) = self.wins[i].evt else { continue };
            if p.send_nowait(h, &frame[..n]) == SendEvtStatus::Dead {
                p.close(h);
                self.wins[i].evt = None;
            }
        }
        let mut s = 0;
        while s < self.subs_n {
            let h = self.subs[s];
            if p.send_nowait(h, &frame[..n]) == SendEvtStatus::Dead {
                p.close(h);
                // 摘除保序（表序稳定——wait 集构造确定性）。
                for j in s..self.subs_n - 1 {
                    self.subs[j] = self.subs[j + 1];
                }
                self.subs_n -= 1;
                continue;
            }
            s += 1;
        }
    }

    /// 快照现值（有效几何 + 焦点/可见旗标面）。
    fn snapshot(&self) -> gp::WmSnapshot {
        let mut entries = [gp::WmEntry {
            id: 0,
            parent: 0,
            group: 0,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            layer: gp::WM_LAYER_APP,
            z: 0,
            flags: 0,
        }; gp::WM_WINDOWS_MAX];
        for i in 0..self.count {
            let win = &self.wins[i];
            let mut flags = 0u8;
            if win.vis {
                flags |= gp::WM_WIN_F_VISIBLE;
            }
            if self.focus == Some(win.id) {
                flags |= gp::WM_WIN_F_FOCUSED;
            }
            entries[i] = gp::WmEntry {
                id: win.id,
                parent: win.parent,
                group: win.group,
                x: win.eff.0,
                y: win.eff.1,
                w: win.eff.2,
                h: win.eff.3,
                layer: win.layer,
                z: win.z,
                flags,
            };
        }
        gp::WmSnapshot {
            version: self.version,
            disp_w: self.disp_w,
            disp_h: self.disp_h,
            entries,
            count: self.count,
        }
    }

    /// 应答快照（252B ≤ CHAN_MSG_MAX 256 单帧）。
    fn send_snapshot<P: Port>(&self, p: &mut P, ch: Handle) {
        let mut out = [0u8; gp::WM_REP_MAX];
        match gp::encode_wm_snapshot_rep(&mut out, &self.snapshot()) {
            Some(n) => p.send(ch, &out[..n]),
            None => p.fail(), // 表项值域由本表构造保证——防御面
        }
    }

    /// 1B 状态应答（非 OK 烘焙线 + DESTROY/SET/FOCUS/BACK 成功形）。
    fn st_rep<P: Port>(p: &mut P, ch: Handle, st: u8) {
        let mut out = [0u8; 1];
        let n = gp::encode_status_rep(&mut out, st);
        p.send(ch, &out[..n]);
    }

    /// 有效几何重算（不变量式避让）：对每 AVOID_SYS 窗，申请几何依次减
    /// 全部**可见**系统层窗（表序确定性；自窗除外——SYSTEM 层携旗不
    /// 自避；SERVICE 层不挖坑——波3 §5 裁决，覆盖语义）。
    fn recompute(&mut self) {
        for i in 0..self.count {
            if !self.wins[i].avoid_sys {
                self.wins[i].eff = self.wins[i].req;
                continue;
            }
            let (mut ex, mut ey, mut ew, mut eh) = (
                self.wins[i].req.0 as u32,
                self.wins[i].req.1 as u32,
                self.wins[i].req.2 as u32,
                self.wins[i].req.3 as u32,
            );
            let my_id = self.wins[i].id;
            for j in 0..self.count {
                let s = &self.wins[j];
                if s.id == my_id || s.layer != gp::WM_LAYER_SYSTEM || !s.vis {
                    continue;
                }
                let (nx, ny, nw, nh) = clip_rect(
                    (ex, ey, ew, eh),
                    (s.eff.0 as u32, s.eff.1 as u32, s.eff.2 as u32, s.eff.3 as u32),
                );
                ex = nx;
                ey = ny;
                ew = nw;
                eh = nh;
            }
            self.wins[i].eff = (ex as u16, ey as u16, ew as u16, eh as u16);
        }
    }

    fn find(&self, id: u16) -> Option<usize> {
        (0..self.count).find(|&i| self.wins[i].id == id)
    }

    /// id 分配（u16 单调回绕；跳 0 哨兵与在册 id——N≤15 恒可解）。
    fn alloc_id(&mut self) -> u16 {
        loop {
            self.next_id = self.next_id.wrapping_add(1);
            if self.next_id != 0 && self.find(self.next_id).is_none() {
                return self.next_id;
            }
        }
    }

    /// 焦点归还：被摘窗若持焦点 → 表序（= 建序）最末可见窗接焦；无可见
    /// 窗 → 无焦点。
    fn refocus_after_removal(&mut self) {
        self.focus = (0..self.count).rev().find(|&i| self.wins[i].vis).map(|i| self.wins[i].id);
    }

    /// WM_GETINFO：能力/版本面（disp 自启动③ present GETINFO 缓存）。
    fn getinfo<P: Port>(&self, p: &mut P, ch: Handle) {
        let rep = gp::WmGetinfoRep {
            version_wm: gp::VER_WM,
            windows_max: gp::WM_WINDOWS_MAX as u8,
            disp_w: self.disp_w,
            disp_h: self.disp_h,
        };
        let mut out = [0u8; 12]; // = WM_GETINFO_REP_LEN（guiproto 私有——本地定容）
        let n = gp::encode_wm_getinfo_rep(&mut out, &rep);
        p.send(ch, &out[..n]);
    }

    /// WM_CREATE：parent≠0 须在册（ST_NOENT）；表满 ST_FULL；新窗默认
    /// 可见 + 接焦（CREATE 无 vis 域——显隐走 SET_F_VIS）；AVOID_SYS 旗
    /// 驱动有效几何避让。应答 8B ack(window_id) + 随行属主事件通道端
    /// （波3 §3：CREATE 建联——gift 面随 ack 迁入客户端表）；内核通道
    /// 资源拒绝时降级无通道窗（ack 零随行——BACK 兜底直接 DESTROY，
    /// 返回语义仍正确）。
    fn create<P: Port>(&mut self, p: &mut P, ch: Handle, req: gp::WmCreateReq) {
        if req.parent != 0 && self.find(req.parent).is_none() {
            Self::st_rep(p, ch, gp::ST_NOENT);
            return;
        }
        if self.count >= gp::WM_WINDOWS_MAX {
            Self::st_rep(p, ch, gp::ST_FULL);
            return;
        }
        let id = self.alloc_id();
        let gift = p.channel_create();
        let geom = (req.x, req.y, req.w, req.h);
        self.wins[self.count] = Win {
            id,
            parent: req.parent,
            group: req.group,
            req: geom,
            eff: geom, // recompute 立即覆写
            layer: req.layer,
            z: 0,
            vis: true,
            avoid_sys: req.flags & gp::WM_CREATE_F_AVOID_SYS != 0,
            evt: gift.map(|(srv, _)| srv),
        };
        self.count += 1;
        self.focus = Some(id);
        self.recompute();
        self.bump(p);
        let mut out = [0u8; 8]; // = WM_CREATE_ACK_LEN（guiproto 私有——本地定容）
        let n = gp::encode_wm_create_ack(&mut out, id);
        match gift {
            Some((_, cli)) => p.send_handles(ch, &out[..n], &[cli]),
            None => p.send(ch, &out[..n]),
        }
    }

    /// WM_DESTROY 摘窗（表序移除保建序——hit-test 平局表序语义）；持
    /// 焦点则归还；系统窗摘除 = 解除避让（recompute）；属主事件通道
    /// 关联关闭（波3 §3）。无应答形 = 兜底 DESTROY 内部复用（§3 R2
    /// ——500ms 到点 wmd 自主摘窗，应答面归触发者）。
    fn destroy_internal<P: Port>(&mut self, p: &mut P, id: u16) -> bool {
        let Some(i) = self.find(id) else {
            return false;
        };
        if let Some(h) = self.wins[i].evt {
            p.close(h);
        }
        for j in i..self.count - 1 {
            self.wins[j] = self.wins[j + 1];
        }
        self.count -= 1;
        if self.focus == Some(id) {
            self.refocus_after_removal();
        }
        self.recompute();
        self.bump(p);
        true
    }

    /// WM_DESTROY：摘窗 + 1B 应答（ST_OK/ST_NOENT）。
    fn destroy<P: Port>(&mut self, p: &mut P, ch: Handle, id: u16) {
        if self.destroy_internal(p, id) {
            Self::st_rep(p, ch, gp::ST_OK);
        } else {
            Self::st_rep(p, ch, gp::ST_NOENT);
        }
    }

    /// WM_SET：masked-out 槽位不动（「编什么收什么」——wire 层不清零，
    /// 本表只应用掩内槽）；SET_F_VIS显隐驱动焦点策略（SHOW 接焦 / HIDE
    /// 持焦则归还）；几何/Z 变更（含系统窗）触发重算。1B OK。
    fn set<P: Port>(&mut self, p: &mut P, ch: Handle, req: gp::WmSetReq) {
        let Some(i) = self.find(req.window_id) else {
            Self::st_rep(p, ch, gp::ST_NOENT);
            return;
        };
        if req.mask & gp::WM_SET_F_GEOM != 0 {
            self.wins[i].req = (req.x, req.y, req.w, req.h);
        }
        if req.mask & gp::WM_SET_F_VIS != 0 {
            self.wins[i].vis = req.vis == gp::WM_VIS_SHOW;
            if self.wins[i].vis {
                self.focus = Some(self.wins[i].id);
            } else if self.focus == Some(self.wins[i].id) {
                self.refocus_after_removal();
            }
        }
        if req.mask & gp::WM_SET_F_Z != 0 {
            self.wins[i].z = req.z;
        }
        self.recompute();
        self.bump(p);
        Self::st_rep(p, ch, gp::ST_OK);
    }

    /// WM_TABLE_GET：全表快照即答（无 version 门槛）。
    fn table_get<P: Port>(&self, p: &mut P, ch: Handle) {
        self.send_snapshot(p, ch);
    }

    /// WM_TABLE_WAIT：version≠last_seen 立即答（≠非 >——纪元回退角由
    /// inputd 超龄重探测兜底，Q11-③，不依赖「必>」）；同 version 则 park
    /// 单车位（后到覆盖不排队——位移者现答现快照防悬挂）。**波3 冻结面**
    /// ——唯一合法驻留者 = inputd push 面，表跟随走事件通道（§9）。
    pub fn wait<P: Port>(&mut self, p: &mut P, ch: Handle, last_seen: u32) {
        if self.version != last_seen {
            self.send_snapshot(p, ch);
            return;
        }
        if let Some((old_ch, _)) = self.parked.take() {
            self.send_snapshot(p, old_ch);
        }
        self.parked = Some((ch, last_seen));
    }

    /// WM_FOCUS（波3 §3 焦点维护）：win≠0 = 授予（须在册且可见——持焦
    /// 窗恒可见不变量，隐藏窗 ST_BAD 策略拒＝OP_INPUT_INJECT 拒面先例）；
    /// win=0 = 让出（焦点清空不动他窗——wm_srv 共享面发送者不可辨，v1
    /// 单焦点在席者即发送者的现实下语义正确，波2 编码注释口径）。
    fn focus<P: Port>(&mut self, p: &mut P, ch: Handle, win: u16) {
        if win == 0 {
            self.focus = None;
            self.bump(p);
            Self::st_rep(p, ch, gp::ST_OK);
            return;
        }
        match self.find(win) {
            None => Self::st_rep(p, ch, gp::ST_NOENT),
            Some(i) if !self.wins[i].vis => Self::st_rep(p, ch, gp::ST_BAD),
            Some(_) => {
                self.focus = Some(win);
                self.bump(p);
                Self::st_rep(p, ch, gp::ST_OK);
            }
        }
    }

    /// OP_WM_BACK（波3 §3 R2 wmd 侧）：路由到焦点窗——无焦点 ST_NOENT；
    /// 无事件通道目标窗（旧式/降级）直接 DESTROY（返回语义仍正确）；
    /// 有通道则非阻塞投递 WM_EVENT_BACK(wid) 并武装 500ms 单槽兜底
    /// （满即丢同武装——投递失败网 = 兜底 DESTROY；通道病态 = 关闭降
    /// 级 + 直接 DESTROY）。应答 ST_OK = 已受理（目标窗处理与兜底 =
    /// wmd 自主面，wire 只管受理）。
    fn back<P: Port>(&mut self, p: &mut P, ch: Handle) {
        let Some(wid) = self.focus else {
            Self::st_rep(p, ch, gp::ST_NOENT);
            return;
        };
        let Some(i) = self.find(wid) else {
            Self::st_rep(p, ch, gp::ST_NOENT); // 焦点恒在表——防御面
            return;
        };
        let Some(h) = self.wins[i].evt else {
            self.destroy_internal(p, wid);
            Self::st_rep(p, ch, gp::ST_OK);
            return;
        };
        let mut frame = [0u8; gp::WM_EVT_BACK_LEN];
        let n = gp::encode_wm_evt_back(&mut frame, wid);
        if p.send_nowait(h, &frame[..n]) == SendEvtStatus::Dead {
            p.close(h);
            self.wins[i].evt = None;
            self.destroy_internal(p, wid);
            Self::st_rep(p, ch, gp::ST_OK);
            return;
        }
        // Queued：已投递；Full：满即丢——两态同靠 500ms 兜底网武装。
        // 单槽覆盖重臂（parked 同款——位移者旧槽 ACK 迟到按 wid 失配忽略）。
        self.pending_back = Some((wid, p.now_ns() + WMD_BACK_FALLBACK_NS));
        Self::st_rep(p, ch, gp::ST_OK);
    }

    /// BACK_ACK 解除兜底（属主事件通道来帧——wid 失配（迟到/位移）忽略）。
    fn on_back_ack(&mut self, wid: u16) {
        if let Some((pw, _)) = self.pending_back {
            if pw == wid {
                self.pending_back = None;
            }
        }
    }

    /// 返回兜底截止时刻（None = 无在途 BACK——serve 循环 wait 帽计算面）。
    pub fn back_deadline(&self) -> Option<u64> {
        self.pending_back.map(|(_, dl)| dl)
    }

    /// 返回兜底软定时到点检查（serve 循环每轮顶调用）：到期 → wmd 兜底
    /// DESTROY(目标窗)（表序内建 refocus 回最末可见窗；bump 内清槽 +
    /// 广播——launcher 经事件通知/TABLE_GET 观察 refocus）。
    pub fn on_back_tick<P: Port>(&mut self, p: &mut P, now: u64) {
        if let Some((wid, dl)) = self.pending_back {
            if now >= dl {
                self.destroy_internal(p, wid);
            }
        }
    }

    /// WM_SUBSCRIBE（波3 §4/§9）：无窗跟随者订阅表变更事件通道——ack
    /// 1B ST_OK + 随行恰 1 handle（订阅者客户端端）；帽满/内核资源拒绝
    /// ST_FULL。
    fn subscribe<P: Port>(&mut self, p: &mut P, ch: Handle) {
        if self.subs_n >= WM_EVT_SUBS_MAX {
            Self::st_rep(p, ch, gp::ST_FULL);
            return;
        }
        let Some((srv, cli)) = p.channel_create() else {
            Self::st_rep(p, ch, gp::ST_FULL);
            return;
        };
        self.subs[self.subs_n] = srv;
        self.subs_n += 1;
        let mut out = [0u8; 1];
        let n = gp::encode_status_rep(&mut out, gp::ST_OK);
        p.send_handles(ch, &out[..n], &[cli]);
    }

    /// 事件通道非阻塞收割（serve 循环每轮顶调用）：属主通道收 BACK_ACK
    /// （余帧型/订阅者来帧一律忽略 drain——坏客户端不得杀服务）。
    pub fn sweep_evt<P: Port>(&mut self, p: &mut P) {
        let mut buf = [0u8; 256];
        let mut i = 0;
        while i < self.count {
            let Some(h) = self.wins[i].evt else {
                i += 1;
                continue;
            };
            let wid = self.wins[i].id;
            while let Some(n) = p.recv_nowait(h, &mut buf) {
                if matches!(gp::parse_wm_evt(&buf[..n]), Some(gp::WmEvt::BackAck)) {
                    self.on_back_ack(wid);
                }
            }
            i += 1;
        }
        let mut s = 0;
        while s < self.subs_n {
            while p.recv_nowait(self.subs[s], &mut buf).is_some() {}
            s += 1;
        }
    }

    /// 事件通道 wait 集（属主 + 订阅者，表序稳定——serve 循环
    /// wait_many 拼装面；越窗截断由调用方短帽兜底）。
    pub fn evt_handles(&self, out: &mut [Handle]) -> usize {
        let mut n = 0;
        for i in 0..self.count {
            if let Some(h) = self.wins[i].evt {
                if n < out.len() {
                    out[n] = h;
                    n += 1;
                }
            }
        }
        for s in 0..self.subs_n {
            if n < out.len() {
                out[n] = self.subs[s];
                n += 1;
            }
        }
        n
    }

    /// wm 域分发（wm_srv 面）：九 op 之外（0x69..=0x7F 未定形）1B
    /// ST_BAD；wm 域 op **零随行 handle**——非空即拒面 close。
    pub fn handle_wm<P: Port>(&mut self, p: &mut P, ch: Handle, msg: &[u8], hs: &[Handle]) {
        if !hs.is_empty() {
            p.close_all(hs);
            Self::st_rep(p, ch, gp::ST_BAD);
            return;
        }
        match msg.first() {
            Some(&gp::OP_WM_GETINFO) if gp::parse_wm_getinfo_req(msg).is_some() => {
                self.getinfo(p, ch)
            }
            Some(&gp::OP_WM_CREATE) => match gp::parse_wm_create_req(msg) {
                Some(req) => self.create(p, ch, req),
                None => Self::st_rep(p, ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_DESTROY) => match gp::parse_wm_destroy_req(msg) {
                Some(id) => self.destroy(p, ch, id),
                None => Self::st_rep(p, ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_SET) => match gp::parse_wm_set_req(msg) {
                Some(req) => self.set(p, ch, req),
                None => Self::st_rep(p, ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_TABLE_GET) if gp::parse_wm_table_get_req(msg).is_some() => {
                self.table_get(p, ch)
            }
            Some(&gp::OP_WM_TABLE_WAIT) => match gp::parse_wm_table_wait_req(msg) {
                Some(last_seen) => self.wait(p, ch, last_seen),
                None => Self::st_rep(p, ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_FOCUS) => match gp::parse_wm_focus_req(msg) {
                Some(win) => self.focus(p, ch, win),
                None => Self::st_rep(p, ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_BACK) => match gp::parse_wm_back_req(msg) {
                Some(()) => self.back(p, ch),
                None => Self::st_rep(p, ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_SUBSCRIBE) if gp::parse_wm_subscribe_req(msg).is_some() => {
                self.subscribe(p, ch)
            }
            _ => Self::st_rep(p, ch, gp::ST_BAD),
        }
    }
}
