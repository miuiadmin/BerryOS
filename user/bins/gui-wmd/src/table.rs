//! 权威窗口表（GUI-5 §3 #5）——静态定容（guifbd `Srv` 先例：无堆分配、
//! 无生长面）。服务状态四件：
//!
//! - **entries**：两层窗（APP/SYSTEM）+ Z 序 + parent/group 缝 + 单焦点；
//!   表项携**有效几何**（AVOID_SYS 避让推导后——wire 回显面），申请几何
//!   不回显但内部在案（重推导需要申请值，Q10 wire 语义钉板）。
//! - **version**：`(epoch u8 << 24) | counter u24`，counter ∈ 1..=0xFFFFFF
//!   跳 0 → **恒 ≥1**（空表初始化即 version≥1——冷启动 WAIT(0) 恒立即
//!   答）；epoch = 启动 CNTVCT 派生 u8（无持久化下唯一跨启动区分源——
//!   新 wmd 纪元的 version 与前世不可比，inputd 超龄 WAIT(0) 重探测
//!   兜底闭环，Q11-③）。
//! - **parked**：单车位 TABLE_WAIT（应答通道 + 其 last_seen）。**单车位
//!   语义钉死（R2 备注⑦）：后到 WAIT 现答现快照并覆盖 parked 不排队**
//!   ——位移者立即以现快照应答（防悬挂；v1 两客户端面各自严格串行 RPC
//!   结构性不可达，后刀多客户端注意）；表变更向**当前** parked 应答
//!   全表快照并清空（消息序 = 表序，无竞面）。
//! - **有效几何不变量式避让**：任何 CREATE/DESTROY/SET（含系统层窗
//!   变更）触发受影响表项重算 O(N)（N≤15 零成本）——AVOID_SYS 旗标窗
//!   对 layer=SYSTEM **可见**窗求差集写有效几何（隐藏系统窗不挖坑——
//!   与 hit-test 隐藏恒不中一致）；单矩形保形裁剪（四条剩余带取面积
//!   最大者：状态栏顶条 → 应用窗自顶收缩；全覆盖 → w/h=0 恒不中）。
//!
//! 错误码面：id/parent 无效 ST_NOENT、表满 ST_FULL、野值已在 wire 层拒
//! （guiproto parse 域执法——本表不做第二解析器）。
//!
//! **波3 批A 四件（设计稿 §3 wmd 配套立法，全部 add-only）**：
//! - **属主事件通道**：CREATE 时建 channel 对（`berry_channel_create`
//!   musl 形态原语——gui-wmd 是 musl 链独立小工作区，与 svcmgr 的 no_std
//!   平像不同途），wmd 留一端记入表项 `evt`（0 = 无通道——建联失败/未来
//!   门控面），另一端随 8B ack **随行 handle gift**（帧字节零变化，旧客户
//!   端不解析随行 handle 零破坏——gfxdemo 8 槽收窗闲置）；DESTROY 关联
//!   关闭 wmd 端（**对端死检测不做——v1 语义**，kernel channel.rs 诚实
//!   缺口：客户端余端随其进程关闭回收，孤儿对互钉泄漏为安全方向已接受）。
//! - **OP_WM_BACK 分派**（0x67，波2 冻结 wire——现答 1B ST_OK=已受理）：
//!   焦点窗有通道 = 非阻塞投 WM_EVENT_BACK(wid) + 起 500ms 单槽软定时；
//!   无通道（旧式客户端目标窗）= **直接 DESTROY**（返回语义正确性优先，
//!   设计稿 §3：无响应应用也得退出）。
//! - **500ms 单槽兜底**：同一时刻至多一个 pending back；超时未收 BACK_ACK
//!   且表未变（version 仍 = 起臂值）→ 兜底 DESTROY(目标窗)（refocus 复用
//!   `refocus_after_removal`：表序最末可见窗接焦）；定时器形态 = serve
//!   循环内 deadline 检查（main.rs `back_tick` 喂入——零新增线程）；表变
//!   （version 递增）即失锚解除（R3 优雅退出路径的窗已自毁，强 DESTROY
//!   失语义）；BACK_ACK 经属主端上行收割即解除。
//! - **表变更广播**：每次 version 递增点（`bump`——CREATE/DESTROY/SET 全
//!   走此口）向全体有通道属主非阻塞 send WM_EVENT_TABLE(ver)，**满即丢**
//!   （-EBUSY 忽略）——通知尽力而为，真值恒经 TABLE_GET 快照。
//! - **TABLE_WAIT 冻结**（设计稿 §9 立规矩）：零新增驻留者——多客户端下
//!   单车位「后到覆盖」退化乒乓自旋，表跟随一律走事件广播 + TABLE_GET。

use crate::{close_all, now_ns, send_rep, send_rep_handles, st_rep, trap};
use berry_guiproto as gp;
use berry_sys::{
    berry_channel_create, berry_channel_recv, berry_channel_send, berry_handle_close, Handle,
};

/// 返回键兜底软定时：500ms（设计稿 §3 R2——超时未收 BACK_ACK 且表未变
/// → wmd 兜底 DESTROY(焦点窗) → refocus 回桌面）。
const WM_BACK_TIMEOUT_NS: u64 = 500 * 1_000_000;

/// 内部窗口项：申请几何（推导源）+ 有效几何（wire 回显面）分域携带——
/// 表项只回显有效几何，重推导需要申请值在案。
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
    /// 沉浸位（WM_CREATE_F_IMMERSIVE——快照 WM_WIN_F_IMMERSIVE 镜像，波3 批A）。
    immersive: bool,
    /// 输入法声明位（CREATE 旗/SET mask 落点——快照 WM_WIN_F_IME_WANTED
    /// 镜像，softimed 显隐依据，波3 批A）。
    ime_wanted: bool,
    /// 属主事件通道 wmd 端（0 = 无通道——建联失败/未来门控面，波3 批A）。
    evt: Handle,
}

/// pending back 单槽（波3 批A——同一时刻至多一个）：目标窗 + 起臂版本
/// （表未变判据）+ 到期线。
struct PendingBack {
    wid: u16,
    version_at_arm: u32,
    deadline_ns: u64,
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
    /// 单车位 TABLE_WAIT（应答通道 + 其 last_seen）。
    parked: Option<(Handle, u32)>,
    /// pending back 单槽（波3 批A——至多一个，后到覆盖先到）。
    pending_back: Option<PendingBack>,
    disp_w: u16,
    disp_h: u16,
}

impl WmTable {
    /// 空表初始化：version = 新纪元 counter 1（恒 ≥1）；epoch = 启动
    /// CNTVCT 派生 u8。
    pub fn new(disp_w: u16, disp_h: u16) -> Self {
        let epoch = (now_ns() & 0xFF) as u8;
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
            immersive: false,
            ime_wanted: false,
            evt: 0,
        };
        WmTable {
            wins: [blank; gp::WM_WINDOWS_MAX],
            count: 0,
            focus: None,
            version: ((epoch as u32) << 24) | 1,
            next_id: 0,
            parked: None,
            pending_back: None,
            disp_w,
            disp_h,
        }
    }

    /// 表变更公共出口：version+1（counter 回绕跳 0——恒 ≥1 不变量）→ 向
    /// 当前 parked 应答全表快照并清空。
    fn bump(&mut self) {
        let counter = (self.version & 0x00FF_FFFF) + 1;
        let counter = if counter > 0x00FF_FFFF { 1 } else { counter };
        self.version = (self.version & 0xFF00_0000) | counter;
        // 波3 批A：表变解除 pending back（设计稿 §3 R2「无表变更才兜底」
        // ——表序已变（常见 = R3 优雅退出自毁窗），强 DESTROY 失锚）。
        if self
            .pending_back
            .as_ref()
            .is_some_and(|p| p.version_at_arm != self.version)
        {
            self.pending_back = None;
        }
        if let Some((ch, _)) = self.parked.take() {
            self.send_snapshot(ch);
        }
        // 波3 批A：表变更广播——全体有通道属主非阻塞 send WM_EVENT_TABLE
        // (ver)，满即丢（-EBUSY 忽略；余错同丢——通知面尽力而为恒不
        // trap，真值恒经 TABLE_GET，设计稿 §3）。
        let mut out = [0u8; gp::WM_EVENT_TABLE_LEN];
        let n = gp::encode_wm_event_table(&mut out, self.version);
        for i in 0..self.count {
            let evt = self.wins[i].evt;
            if evt != 0 {
                let _ = berry_channel_send(evt, out.as_ptr(), n, core::ptr::null(), 0);
            }
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
            // 波3 批A：沉浸/IME 镜像位（CREATE 旗/SET mask 落点的快照回显）。
            if win.immersive {
                flags |= gp::WM_WIN_F_IMMERSIVE;
            }
            if win.ime_wanted {
                flags |= gp::WM_WIN_F_IME_WANTED;
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
    fn send_snapshot(&self, ch: Handle) {
        let mut out = [0u8; gp::WM_REP_MAX];
        match gp::encode_wm_snapshot_rep(&mut out, &self.snapshot()) {
            Some(n) => send_rep(ch, &out[..n]),
            None => trap(), // 表项值域由本表构造保证——防御面
        }
    }

    /// 有效几何重算（不变量式避让）：对每 AVOID_SYS 窗，申请几何依次减
    /// 全部**可见**系统层窗（表序确定性；自窗除外——SYSTEM 层携旗不
    /// 自避）。
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
    fn getinfo(&self, ch: Handle) {
        let rep = gp::WmGetinfoRep {
            version_wm: gp::VER_WM,
            windows_max: gp::WM_WINDOWS_MAX as u8,
            disp_w: self.disp_w,
            disp_h: self.disp_h,
        };
        let mut out = [0u8; 12]; // = WM_GETINFO_REP_LEN（guiproto 私有——本地定容）
        let n = gp::encode_wm_getinfo_rep(&mut out, &rep);
        send_rep(ch, &out[..n]);
    }

    /// WM_CREATE：parent≠0 须在册（ST_NOENT）；表满 ST_FULL；新窗默认
    /// 可见 + 接焦（CREATE 无 vis 域——显隐走 SET_F_VIS）；AVOID_SYS 旗
    /// 驱动有效几何避让。应答 8B ack(window_id)——波3 批A 起随行 gift
    /// 属主事件通道端（帧字节零变化，仅随行 handle 增一）。
    fn create(&mut self, ch: Handle, req: gp::WmCreateReq) {
        if req.parent != 0 && self.find(req.parent).is_none() {
            st_rep(ch, gp::ST_NOENT);
            return;
        }
        if self.count >= gp::WM_WINDOWS_MAX {
            st_rep(ch, gp::ST_FULL);
            return;
        }
        let id = self.alloc_id();
        // 波3 批A：属主事件通道——CREATE 恒建 channel 对（两端出生即全权
        // SEND|RECV|DUPLICATE，任务书「SEND 端」为口语：客户端端可收事件
        // 亦可回 BACK_ACK）；wmd 留 end0 记入表项，end1 随 ack gift。建联
        // 失败 = evt=0 落表（无通道窗：BACK 投递走直接 DESTROY 臂）。
        let mut evt = 0 as Handle;
        let mut gift = 0 as Handle;
        if berry_channel_create(0, &mut evt, &mut gift) != 0 {
            evt = 0;
            gift = 0;
        }
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
            immersive: req.flags & gp::WM_CREATE_F_IMMERSIVE != 0,
            ime_wanted: req.flags & gp::WM_CREATE_F_IME_WANTED != 0,
            evt,
        };
        self.count += 1;
        self.focus = Some(id);
        self.recompute();
        self.bump();
        let mut out = [0u8; 8]; // = WM_CREATE_ACK_LEN（guiproto 私有——本地定容）
        let n = gp::encode_wm_create_ack(&mut out, id);
        if gift != 0 {
            send_rep_handles(ch, &out[..n], &[gift]);
        } else {
            send_rep(ch, &out[..n]);
        }
    }

    /// 摘窗核心（WM_DESTROY 与波3 批A back 兜底共用）：属主通道关联关闭
    /// → pending back 目标被摘即解除 → 表序移除保建序（hit-test 平局表序
    /// 语义）→ 持焦点则归还 → 系统窗摘除 = 解除避让（recompute）→ bump
    /// （广播/parked）。应答由调用方出。
    fn remove(&mut self, id: u16) -> bool {
        let Some(i) = self.find(id) else {
            return false;
        };
        // 波3 批A：DESTROY 关联关闭属主事件通道 wmd 端（对端死检测不做
        // ——v1 语义：客户端余端随其进程关闭回收；R3 优雅退出序「窗
        // DESTROY 后 BACK_ACK」落本臂时 ACK 入孤儿队列，无害吸收）。
        if self.wins[i].evt != 0 {
            let _ = berry_handle_close(self.wins[i].evt);
        }
        if self.pending_back.as_ref().is_some_and(|p| p.wid == id) {
            self.pending_back = None;
        }
        for j in i..self.count - 1 {
            self.wins[j] = self.wins[j + 1];
        }
        self.count -= 1;
        if self.focus == Some(id) {
            self.refocus_after_removal();
        }
        self.recompute();
        self.bump();
        true
    }

    /// WM_DESTROY：摘窗（核心见 [`remove`]）。
    fn destroy(&mut self, ch: Handle, id: u16) {
        if self.remove(id) {
            st_rep(ch, gp::ST_OK);
        } else {
            st_rep(ch, gp::ST_NOENT);
        }
    }

    /// WM_SET：masked-out 槽位不动（「编什么收什么」——wire 层不清零，
    /// 本表只应用掩内槽）；SET_F_VIS 显隐驱动焦点策略（SHOW 接焦 / HIDE
    /// 持焦则归还）；几何/Z 变更（含系统窗）触发重算。1B OK。
    fn set(&mut self, ch: Handle, req: gp::WmSetReq) {
        let Some(i) = self.find(req.window_id) else {
            st_rep(ch, gp::ST_NOENT);
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
        // 波3 批A：ime_wanted 布尔槽——槽值 = vis 字节复用（guiproto
        // WM_SET_F_IME_WANTED 常量注：SET 定长 20B 无预留槽可认领，单值
        // 域 0/1 双槽复用，帧字节面零变化）。
        if req.mask & gp::WM_SET_F_IME_WANTED != 0 {
            self.wins[i].ime_wanted = req.vis == gp::WM_VIS_SHOW;
        }
        if req.mask & gp::WM_SET_F_Z != 0 {
            self.wins[i].z = req.z;
        }
        self.recompute();
        self.bump();
        st_rep(ch, gp::ST_OK);
    }

    /// WM_TABLE_GET：全表快照即答（无 version 门槛）。
    fn table_get(&self, ch: Handle) {
        self.send_snapshot(ch);
    }

    /// WM_TABLE_WAIT：version≠last_seen 立即答（≠非 >——纪元回退角由
    /// inputd 超龄重探测兜底，Q11-③，不依赖「必>」）；同 version 则 park
    /// 单车位（后到覆盖不排队——位移者现答现快照防悬挂）。
    ///
    /// **TABLE_WAIT 冻结（波3 批A 立法，设计稿 §9）**：零新增驻留者——多
    /// 客户端下单车位「后到覆盖」退化为乒乓自旋；表跟随一律走属主事件
    /// 通道 WM_EVENT_TABLE 广播 + TABLE_GET 真值（本表 `bump`）。
    pub fn wait(&mut self, ch: Handle, last_seen: u32) {
        if self.version != last_seen {
            self.send_snapshot(ch);
            return;
        }
        if let Some((old_ch, _)) = self.parked.take() {
            self.send_snapshot(old_ch);
        }
        self.parked = Some((ch, last_seen));
    }

    // —— 波3 批A：OP_WM_BACK 分派 + 500ms 单槽兜底（设计稿 §3 R2） ——

    /// OP_WM_BACK（波2 冻结 wire 0x67 无参——wmd 查表路由）：TABLE 态取
    /// 焦点窗（快照 WM_WIN_F_FOCUSED 同源）；有属主通道 = 非阻塞投
    /// WM_EVENT_BACK(wid) + 起臂单槽软定时；无通道（旧式客户端目标窗/
    /// 建联失败窗）= **直接 DESTROY**（返回语义正确性优先——设计稿 §3：
    /// 无响应应用也得退出）。应答恒 1B ST_OK（已受理——目标窗处理与兜底
    /// 是服务端异步面，wire 不回传）。事件投递满/失败不重试：500ms 兜底
    /// 盖住（语义不悬空）。
    fn op_back(&mut self, ch: Handle) {
        let Some(wid) = self.focus else {
            // 无焦点窗 = 无路由目标（空操作受理——诚实面：BACK 无「必达
            // 目标」契约，R1 桌面态由 launcher 本地即答不经本 op）。
            st_rep(ch, gp::ST_OK);
            return;
        };
        let Some(i) = self.find(wid) else {
            st_rep(ch, gp::ST_OK); // 防御：focus 在册由本表不变量保证
            return;
        };
        let evt = self.wins[i].evt;
        if evt == 0 {
            self.remove(wid);
            st_rep(ch, gp::ST_OK);
            return;
        }
        let mut out = [0u8; gp::WM_EVENT_BACK_LEN];
        let n = gp::encode_wm_event_back(&mut out, wid);
        let _ = berry_channel_send(evt, out.as_ptr(), n, core::ptr::null(), 0);
        self.arm_back(wid, evt);
        st_rep(ch, gp::ST_OK);
    }

    /// 起臂单槽 back 兜底：同一时刻至多一个 pending——**后到覆盖先到**
    /// （返回键可重按：最新 back 为活面；旧 pending 的事件已投、其迟到
    /// BACK_ACK 无归因域——覆盖即弃，v1 诚实语义）。起臂前先排空属主端
    /// 陈旧 BACK_ACK（陈货必弃——防旧回执误解除新臂）。
    fn arm_back(&mut self, wid: u16, evt: Handle) {
        loop {
            let mut b = [0u8; gp::WM_EVENT_BACK_LEN]; // 8B 窗：BACK_ACK 1B 恒容
            let mut hout = [0 as Handle; 8];
            let mut nb = b.len();
            let mut nh = hout.len();
            let s = berry_channel_recv(evt, b.as_mut_ptr(), &mut nb, hout.as_mut_ptr(), &mut nh);
            if s != 0 {
                break; // -EAGAIN = 排空；余错防御性同断（收割面恒在 serve 轮询）
            }
            close_all(&hout[..nh]); // 属主→wmd 方向零随行契约——违规面即收即关
        }
        self.pending_back = Some(PendingBack {
            wid,
            version_at_arm: self.version,
            deadline_ns: now_ns() + WM_BACK_TIMEOUT_NS,
        });
    }

    /// BACK_ACK 收割（serve 循环 poll 属主端喂入）：1B 回程帧解除兜底；
    /// 畸形帧忽略（pending 保持——兜底定时仍盖住）。
    pub fn back_ack(&mut self, msg: &[u8]) {
        if gp::parse_back_ack(msg).is_some() {
            self.pending_back = None;
        }
    }

    /// 兜底到期检查（serve 循环逐轮喂 now——零新增线程）：到期未收
    /// BACK_ACK 且表未变（version 仍 = 起臂值）→ 兜底 DESTROY(目标窗)
    /// （remove 核心：refocus = 表序最末可见窗接焦，`refocus_after_
    /// removal` 既有链）；表已变 = 失锚解除（bump 内已摘——本臂双保险）。
    /// 服务端自发动作无应答面。到期线是 serve 单线程语义下的**最早检查
    /// 点**——中继阻塞（RELAY_CAP 帽内）顺延触发，v1 接受（新鲜度注记
    /// 同类结构限制）。
    pub fn back_tick(&mut self, now: u64) {
        let Some(p) = self.pending_back.as_ref() else {
            return;
        };
        if now < p.deadline_ns {
            return;
        }
        let (wid, armed_at) = (p.wid, p.version_at_arm);
        self.pending_back = None;
        if self.version == armed_at {
            self.remove(wid);
        }
    }

    /// pending back 的属主端（serve 轮询/等待集挂接——单槽至多一路）；
    /// 目标窗在册才有收割面（version 守卫下恒在册——防御 None）。
    pub fn back_evt(&self) -> Option<Handle> {
        let p = self.pending_back.as_ref()?;
        self.find(p.wid).map(|i| self.wins[i].evt).filter(|&h| h != 0)
    }

    /// pending back 到期线（等待集 deadline 取近用）。
    pub fn back_deadline(&self) -> Option<u64> {
        self.pending_back.as_ref().map(|p| p.deadline_ns)
    }

    /// wm 域分发（wm_srv 面）：GETINFO/CREATE/DESTROY/SET/TABLE_GET/
    /// TABLE_WAIT 六 op + BACK（波3 批A 接线）之外（0x66 FOCUS 等未定形）
    /// 1B ST_BAD；wm 域请求**零随行 handle**——非空即拒面 close。
    pub fn handle_wm(&mut self, ch: Handle, msg: &[u8], hs: &[Handle]) {
        if !hs.is_empty() {
            close_all(hs);
            st_rep(ch, gp::ST_BAD);
            return;
        }
        match msg.first() {
            Some(&gp::OP_WM_GETINFO) if gp::parse_wm_getinfo_req(msg).is_some() => self.getinfo(ch),
            Some(&gp::OP_WM_CREATE) => match gp::parse_wm_create_req(msg) {
                Some(req) => self.create(ch, req),
                None => st_rep(ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_DESTROY) => match gp::parse_wm_destroy_req(msg) {
                Some(id) => self.destroy(ch, id),
                None => st_rep(ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_SET) => match gp::parse_wm_set_req(msg) {
                Some(req) => self.set(ch, req),
                None => st_rep(ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_TABLE_GET) if gp::parse_wm_table_get_req(msg).is_some() => {
                self.table_get(ch)
            }
            Some(&gp::OP_WM_TABLE_WAIT) => match gp::parse_wm_table_wait_req(msg) {
                Some(last_seen) => self.wait(ch, last_seen),
                None => st_rep(ch, gp::ST_BAD),
            },
            // 波3 批A：BACK 分派（焦点切换 FOCUS 0x66 仍 ST_BAD——波3 后续
            // 批接线面，本刀不预接）。
            Some(&gp::OP_WM_BACK) if gp::parse_wm_back_req(msg).is_some() => self.op_back(ch),
            _ => st_rep(ch, gp::ST_BAD),
        }
    }
}
