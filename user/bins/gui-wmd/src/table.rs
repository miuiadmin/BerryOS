//! 权威窗口表（GUI-5 §3 #5）——静态定容（guifbd `Srv` 先例：无堆分配、
//! 无生长面）。服务状态四件：
//!
//! - **entries**：两层窗（APP/SYSTEM）+ owner session + Z 序 +
//!   parent/group 缝 + 单焦点；
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
//! 错误码面：id/parent 无效 ST_NOENT、表满 ST_FULL、跨 owner
//! SET/DESTROY 为 ST_DENIED、野值已在 wire 层拒
//! （guiproto parse 域执法——本表不做第二解析器）。

use berry_guiproto as gp;
use berry_sys::Handle;

/// RUN-K1 host harness 供给缝：WmTable 的状态机与策略留在 lib 面；时钟、
/// channel send、close、诊断与 fatal trap 由 guest/host 分别注入。guest
/// 实现委托原 serve I/O；host 实现为确定性 recorder，不造第二权威。
pub trait WmIo {
    fn now_ns(&self) -> u64;
    fn send(&self, channel: Handle, reply: &[u8]);
    fn status(&self, channel: Handle, status: u8);
    fn close_handles(&self, handles: &[Handle]);
    fn diag_wrap_clear(&self);
    fn trap(&self) -> !;
}

/// 内部窗口项：申请几何（推导源）+ 有效几何（wire 回显面）分域携带——
/// 表项只回显有效几何，重推导需要申请值在案。
#[derive(Clone, Copy)]
struct Win {
    id: u16,
    /// 拥有者 session id（Phase 1；CREATE 绑定，SET/DESTROY 同 owner 执法）。
    owner: u16,
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
    /// R3 per-window pending config event。None = 已 ack 或无待消费事件；
    /// 新事件直接覆盖旧事件（last-one-wins），serial 由 [`Self::bump`] 统一
    /// 落为本次表版本。
    config: Option<gp::WmConfigEvent>,
    /// Wayland “未 configure 不得 commit” 的 Berry teeth：CREATE 后为
    /// false，精确 ACK 当前事件后 true；极端 serial 终局只清 pending。
    config_synced: bool,
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

/// RUN-K1 合成裁剪产物：目标全屏矩形与 surface raw-local 源子矩形。
/// 两组坐标必须成对产生，禁止先裁 source 再平移到 destination。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompositeClip {
    pub dst: gp::Rect,
    pub src: gp::Rect,
}

/// 把 raw-local damage `rect` 平移到窗口原点 `dst_xy` 后裁到全屏。
/// 返回 None 表示空交；右/下越界会同步缩短 source 子区间，防一行写穿
/// 到下一屏幕行。
pub fn composite_clip(
    dst_xy: (u32, u32),
    rect: gp::Rect,
    screen_w: u32,
    screen_h: u32,
) -> Option<CompositeClip> {
    let dst_x0 = (u64::from(dst_xy.0) + u64::from(rect.x)).min(u64::from(screen_w));
    let dst_y0 = (u64::from(dst_xy.1) + u64::from(rect.y)).min(u64::from(screen_h));
    let dst_x1 = (u64::from(dst_xy.0) + u64::from(rect.x) + u64::from(rect.w))
        .min(u64::from(screen_w));
    let dst_y1 = (u64::from(dst_xy.1) + u64::from(rect.y) + u64::from(rect.h))
        .min(u64::from(screen_h));
    if dst_x1 <= dst_x0 || dst_y1 <= dst_y0 {
        return None;
    }
    let src_x = u32::try_from(dst_x0 - u64::from(dst_xy.0)).ok()?;
    let src_y = u32::try_from(dst_y0 - u64::from(dst_xy.1)).ok()?;
    Some(CompositeClip {
        dst: gp::Rect {
            x: dst_x0 as u32,
            y: dst_y0 as u32,
            w: (dst_x1 - dst_x0) as u32,
            h: (dst_y1 - dst_y0) as u32,
        },
        src: gp::Rect {
            x: src_x,
            y: src_y,
            w: (dst_x1 - dst_x0) as u32,
            h: (dst_y1 - dst_y0) as u32,
        },
    })
}

pub struct WmTable {
    wins: [Win; gp::WM_WINDOWS_MAX],
    count: usize,
    focus: Option<u16>,
    version: u32,
    next_id: u16,
    /// 单车位 TABLE_WAIT（应答通道 + 其 last_seen）。
    parked: Option<(Handle, u32)>,
    io: &'static dyn WmIo,
    disp_w: u16,
    disp_h: u16,
}

impl WmTable {
    /// 空表初始化：version = 新纪元 counter 1（恒 ≥1）；epoch = 启动
    /// CNTVCT 派生 u8。
    pub fn new(io: &'static dyn WmIo, disp_w: u16, disp_h: u16) -> Self {
        let epoch = (io.now_ns() & 0xFF) as u8;
        let blank = Win {
            id: 0,
            owner: 0,
            parent: 0,
            group: 0,
            req: (0, 0, 0, 0),
            eff: (0, 0, 0, 0),
            layer: gp::WM_LAYER_APP,
            z: 0,
            vis: false,
            avoid_sys: false,
            config: None,
            config_synced: false,
        };
        WmTable {
            wins: [blank; gp::WM_WINDOWS_MAX],
            count: 0,
            focus: None,
            version: ((epoch as u32) << 24) | 1,
            next_id: 0,
            parked: None,
            io,
            disp_w,
            disp_h,
        }
    }

    /// 表变更公共出口：version+1（counter 回绕跳 0——恒 ≥1 不变量）→ 向
    /// 当前 parked 应答全表快照并清空。
    fn bump(&mut self) {
        let counter = (self.version & 0x00FF_FFFF) + 1;
        let wrapped = counter > 0x00FF_FFFF;
        let mut epoch = self.version >> 24;
        let counter = if wrapped { 1 } else { counter };
        if wrapped && epoch < 0xFF {
            // counter 回绕时抬 epoch：serial 在同一 wmd 纪元内保持不重复，
            // inputd 的 WAIT 采用“!=”判新，无需大小比较。256 次 counter
            // 回绕（≈2^32 次表变）才触及下方终局防御。
            epoch += 1;
        }
        self.version = (epoch << 24) | counter;
        if wrapped && epoch == 0xFF {
            // 极端终局：只清已成旧 advisory pending，防旧 ACK 在 serial
            // 值域重复后误中；本 bump 新事件保留并可 ACK。
            for win in self.wins.iter_mut().take(self.count) {
                // serial=0 是本 bump 刚生成的新事件：终局防御只清已成旧
                // pending，不吞掉本次 CREATE/SET 的可恢复状态。
                if win.config.map(|ev| ev.serial != 0).unwrap_or(false) {
                    win.config = None;
                }
            }
            self.io.diag_wrap_clear();
        }
        // 本次表变更新生成的事件先以 serial=0 占位，这里统一落成新表版本；
        // 已存在的未 ack 旧事件不被无关 focus bump 改号。
        for win in self.wins.iter_mut().take(self.count) {
            if let Some(ev) = &mut win.config {
                if ev.serial == 0 {
                    ev.serial = self.version;
                }
            }
        }
        if let Some((ch, _)) = self.parked.take() {
            self.send_snapshot(ch);
        }
    }

    /// 生成/覆盖某窗 pending config（serial 暂 0，待 bump 统一赋值）。
    fn mark_config(&mut self, i: usize, cause: u8) {
        let win = &self.wins[i];
        self.wins[i].config = Some(gp::WmConfigEvent {
            serial: 0,
            window_id: win.id,
            cause,
            visible: win.vis,
            raw_x: win.req.0,
            raw_y: win.req.1,
            raw_w: win.req.2,
            raw_h: win.req.3,
            visible_x: win.eff.0,
            visible_y: win.eff.1,
            visible_w: win.eff.2,
            visible_h: win.eff.3,
        });
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
    fn send_snapshot(&self, ch: Handle) {
        let mut out = [0u8; gp::WM_REP_MAX];
        match gp::encode_wm_snapshot_rep(&mut out, &self.snapshot()) {
            Some(n) => self.io.send(ch, &out[..n]),
            None => self.io.trap(), // 表项值域由本表构造保证——防御面
        }
    }

    /// 有效几何重算（不变量式避让）：对每 AVOID_SYS 窗，申请几何依次减
    /// 全部**可见**系统层窗（表序确定性；自窗除外——SYSTEM 层携旗不
    /// 自避）。
    fn recompute(&mut self) {
        let mut before = [(0u16, 0u16, 0u16, 0u16); gp::WM_WINDOWS_MAX];
        for (dst, src) in before.iter_mut().zip(self.wins.iter()).take(self.count) {
            *dst = src.eff;
        }
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
                // SYSTEM 层禁止 AVOID_SYS，其 eff≡req；直接消费 req 防御
                // 未来推导面漂移。
                let (nx, ny, nw, nh) = clip_rect(
                    (ex, ey, ew, eh),
                    (s.req.0 as u32, s.req.1 as u32, s.req.2 as u32, s.req.3 as u32),
                );
                ex = nx;
                ey = ny;
                ew = nw;
                eh = nh;
            }
            self.wins[i].eff = (ex as u16, ey as u16, ew as u16, eh as u16);
            if self.wins[i].avoid_sys && self.wins[i].eff != before[i] {
                self.mark_config(i, gp::WM_CONFIG_CAUSE_OCCLUSION);
            }
        }
    }

    fn find(&self, id: u16) -> Option<usize> {
        (0..self.count).find(|&i| self.wins[i].id == id)
    }

    /// 当前表版本（focus bump 的快照证据面）。
    pub fn version(&self) -> u32 {
        self.version
    }

    /// P2 键盘焦点真值：当前 focus 窗的 (owner, window)。
    pub fn focused(&self) -> Option<(u16, u16)> {
        let id = self.focus?;
        let i = self.find(id)?;
        Some((self.wins[i].owner, id))
    }

    /// P3 click-to-focus：pointer DOWN 命中真实可见窗时接焦。桌面 miss
    /// 不清焦（v1 无桌面窗对象，避免造 window=0 第二语义）；隐藏窗不参
    /// hit-test。变更后 bump，让 WM_TABLE_WAIT 消费者看到 focused flag。
    pub fn focus_from_pointer(&mut self, id: u16) -> bool {
        let Some(i) = self.find(id) else { return false };
        if !self.wins[i].vis
            || self.wins[i].layer != gp::WM_LAYER_APP
            || self.focus == Some(id)
        {
            return false;
        }
        self.focus = Some(id);
        self.bump();
        true
    }

    /// Phase 3 权威输入命中：返回 (owner, window_id, raw 窗口原点)。
    /// 命中判定用 visible 矩形；坐标本地化用 raw 原点——R2 合约下 app
    /// buffer 仍是 raw 窗口坐标，visible 顶部的 pointer y 必须保留遮挡
    /// 偏移，才能读到正确 raw row。
    pub fn hit_route(&self, x: u32, y: u32) -> Option<(u16, u16, u16, u16)> {
        let mut entries = [gp::WmEntry {
            id: 0, parent: 0, group: 0, x: 0, y: 0, w: 0, h: 0,
            layer: gp::WM_LAYER_APP, z: 0, flags: 0,
        }; gp::WM_WINDOWS_MAX];
        for (dst, src) in entries.iter_mut().zip(self.wins.iter()).take(self.count) {
            *dst = gp::WmEntry {
                id: src.id, parent: src.parent, group: src.group,
                x: src.eff.0, y: src.eff.1, w: src.eff.2, h: src.eff.3,
                layer: src.layer, z: src.z,
                flags: if src.vis { gp::WM_WIN_F_VISIBLE } else { 0 },
            };
        }
        let id = gp::wm_hit_test(&entries[..self.count], x, y)?;
        let i = self.find(id)?;
        Some((self.wins[i].owner, id, self.wins[i].req.0, self.wins[i].req.1))
    }

    /// 输入锁定窗的 raw 几何原点（理由同 [`Self::hit_route`]）。
    pub fn route_geometry(&self, owner: u16, id: u16) -> Option<(u16, u16)> {
        let i = (0..self.count).find(|&i| {
            self.wins[i].id == id && self.wins[i].owner == owner && self.wins[i].vis
        })?;
        Some((self.wins[i].req.0, self.wins[i].req.1))
    }

    /// present GETINFO 启动缓存尺寸；0x67 fit 检查的显示上限。
    pub fn display_size(&self) -> (u32, u32) {
        (u32::from(self.disp_w), u32::from(self.disp_h))
    }

    /// R3 config 同步门：Some(false) 表示窗口在册但未 ACK 当前配置。
    /// Some(true) 不隐含可呈现（hidden/full-occluded 仍由双几何判定）。
    pub fn config_ready(&self, owner: u16, id: u16) -> Option<bool> {
        let i = (0..self.count).find(|&i| {
            self.wins[i].id == id && self.wins[i].owner == owner
        })?;
        Some(self.wins[i].config_synced && self.wins[i].config.is_none())
    }

    /// R2 SUBMIT 双几何：raw = 申请/buffer 基准；visible = 遮挡后可呈现
    /// /可命中域。全遮挡（visible w/h=0）不构造假几何，调用侧拒绝。
    pub fn surface_geometry(
        &self,
        owner: u16,
        id: u16,
    ) -> Option<gp::authority::SurfaceGeometry> {
        let i = (0..self.count).find(|&i| {
            self.wins[i].id == id && self.wins[i].owner == owner && self.wins[i].vis
        })?;
        let w = &self.wins[i];
        if !w.config_synced || w.config.is_some() {
            return None;
        }
        gp::authority::SurfaceGeometry::new(w.req, w.eff)
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
        self.focus = (0..self.count)
            .rev()
            .find(|&i| self.wins[i].vis && gp::authority::wm_policy_focus_allowed(self.wins[i].layer))
            .map(|i| self.wins[i].id);
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
        self.io.send(ch, &out[..n]);
    }

    /// WM_CREATE：parent≠0 须在册（ST_NOENT）；表满 ST_FULL；新窗默认
    /// 可见 + 接焦（CREATE 无 vis 域——显隐走 SET_F_VIS）；AVOID_SYS 旗
    /// 驱动有效几何避让。应答 8B ack(window_id)。
    fn create(&mut self, ch: Handle, owner: u16, client_type: u8,
              req: gp::WmCreateReq) {
        if !gp::authority::wm_create_layer_allowed(client_type, req.layer)
            || !gp::authority::wm_create_flags_allowed(client_type, req.flags)
        {
            self.io.status(ch, gp::ST_DENIED);
            return;
        }
        // find() 保持权威：parent id 不连续时不得用下标冒充。
        let parent_owner = if req.parent == 0 {
            None
        } else {
            match self.find(req.parent) {
                Some(i) => Some(self.wins[i].owner),
                None => {
                    self.io.status(ch, gp::ST_NOENT);
                    return;
                }
            }
        };
        // 拒绝路径不得消耗 alloc_id：父窗权限先以 prospective id=0 裁定
        //（非零 parent 不可能等于尚未分配的新 id），表满也先于 id 分配。
        if !gp::authority::wm_parent_allowed(req.parent, parent_owner, owner, 0) {
            self.io.status(ch, gp::ST_DENIED);
            return;
        }
        if self.count >= gp::WM_WINDOWS_MAX {
            self.io.status(ch, gp::ST_FULL);
            return;
        }
        let id = self.alloc_id();
        let new_index = self.count;
        let geom = (req.x, req.y, req.w, req.h);
        self.wins[self.count] = Win {
            id,
            owner,
            parent: req.parent,
            group: req.group,
            req: geom,
            eff: geom, // recompute 立即覆写
            layer: req.layer,
            z: 0,
            vis: true,
            avoid_sys: req.flags & gp::WM_CREATE_F_AVOID_SYS != 0,
            config: None,
            config_synced: false,
        };
        self.count += 1;
        if gp::authority::wm_policy_focus_allowed(req.layer) {
            self.focus = Some(id);
        }
        self.recompute();
        // CREATE 后必有一条初始 config：Fuchsia “首次 Present 前可取
        // LayoutInfo” 的 pull-once 对应面。若同表变更也触发了遮挡推导，
        // CREATED 覆盖 OCCLUSION——初始状态本身已包含最终 visible bounds。
        self.mark_config(new_index, gp::WM_CONFIG_CAUSE_CREATED);
        self.bump();
        let mut out = [0u8; 8]; // = WM_CREATE_ACK_LEN（guiproto 私有——本地定容）
        let n = gp::encode_wm_create_ack(&mut out, id);
        self.io.send(ch, &out[..n]);
    }

    /// WM_DESTROY：摘窗（表序移除保建序——hit-test 平局表序语义）；持
    /// 焦点则归还；系统窗摘除 = 解除避让（recompute）。
    fn destroy(&mut self, ch: Handle, owner: u16, id: u16) {
        let Some(i) = self.find(id) else {
            self.io.status(ch, gp::ST_NOENT);
            return;
        };
        if self.wins[i].owner != owner {
            self.io.status(ch, gp::ST_DENIED);
            return;
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
        self.io.status(ch, gp::ST_OK);
    }

    /// WM_SET：masked-out 槽位不动（「编什么收什么」——wire 层不清零，
    /// 本表只应用掩内槽）；SET_F_VIS 显隐驱动焦点策略（SHOW 接焦 / HIDE
    /// 持焦则归还）；几何/Z 变更（含系统窗）触发重算。1B OK。
    fn set(&mut self, ch: Handle, owner: u16, req: gp::WmSetReq) {
        let Some(i) = self.find(req.window_id) else {
            self.io.status(ch, gp::ST_NOENT);
            return;
        };
        if self.wins[i].owner != owner {
            self.io.status(ch, gp::ST_DENIED);
            return;
        }
        let old_req = self.wins[i].req;
        let old_vis = self.wins[i].vis;
        if req.mask & gp::WM_SET_F_GEOM != 0 {
            self.wins[i].req = (req.x, req.y, req.w, req.h);
        }
        if req.mask & gp::WM_SET_F_VIS != 0 {
            self.wins[i].vis = req.vis == gp::WM_VIS_SHOW;
            if self.wins[i].vis
                && gp::authority::wm_policy_focus_allowed(self.wins[i].layer)
            {
                self.focus = Some(self.wins[i].id);
            } else if self.focus == Some(self.wins[i].id) {
                self.refocus_after_removal();
            }
        }
        if req.mask & gp::WM_SET_F_Z != 0 {
            self.wins[i].z = req.z;
        }
        let own_cause = if self.wins[i].req != old_req {
            Some(gp::WM_CONFIG_CAUSE_GEOMETRY)
        } else if self.wins[i].vis != old_vis {
            Some(gp::WM_CONFIG_CAUSE_VISIBILITY)
        } else {
            None
        };
        self.recompute();
        if let Some(cause) = own_cause {
            self.mark_config(i, cause);
        }
        self.bump();
        self.io.status(ch, gp::ST_OK);
    }

    /// WM_TABLE_GET：全表快照即答（无 version 门槛）。
    fn table_get(&self, ch: Handle) {
        self.send_snapshot(ch);
    }

    /// WM_TABLE_WAIT：version≠last_seen 立即答（≠非 >——纪元回退角由
    /// inputd 超龄重探测兜底，Q11-③，不依赖「必>」）；同 version 则 park
    /// 单车位（后到覆盖不排队——位移者现答现快照防悬挂）。
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

    /// R3 WM_CONFIG_POLL：按表序返回该 owner 第一条 pending event；无事件
    /// 返回 ST_EAGAIN。多窗 session 由 ACK 推进游标，避免排队状态机。
    fn config_poll(&self, ch: Handle, owner: u16) {
        let Some(ev) = self.wins.iter().take(self.count).find_map(|w| {
            (w.owner == owner).then_some(w.config).flatten()
        }) else {
            self.io.status(ch, gp::ST_EAGAIN);
            return;
        };
        let mut out = [0u8; 32]; // = WM_CONFIG_REP_LEN
        match gp::encode_wm_config_event_rep(&mut out, &ev) {
            Some(n) => self.io.send(ch, &out[..n]),
            None => self.io.trap(), // 表内事件由 mark_config 构造——防御面
        }
    }

    /// R3 WM_CONFIG_ACK：只接受当前 pending 精确 serial。事件被新配置覆盖
    /// 后旧 ack 变 ST_DENIED；客户端必须重新 POLL 最新状态再 ACK。
    fn config_ack(&mut self, ch: Handle, owner: u16, req: gp::WmConfigAckReq) {
        let Some(i) = self.find(req.window_id) else {
            self.io.status(ch, gp::ST_NOENT);
            return;
        };
        if self.wins[i].owner != owner {
            self.io.status(ch, gp::ST_DENIED);
            return;
        }
        match &self.wins[i].config {
            Some(ev) if ev.serial == req.serial => {
                self.wins[i].config = None;
                self.wins[i].config_synced = true;
                self.io.status(ch, gp::ST_OK);
            }
            _ => self.io.status(ch, gp::ST_DENIED),
        }
    }

    /// session 死亡清理：若其 RPC 通道恰好 parked 在 TABLE_WAIT，先丢弃
    /// parking 位，避免后续 bump 对已关 handle send 导致 wmd trap。
    pub fn forget_parked(&mut self, ch: Handle) {
        if let Some((parked, _)) = self.parked {
            if parked == ch {
                self.parked = None;
            }
        }
    }

    /// wm 域分发（session 面）：GETINFO/TABLE 只读无 owner 门；
    /// CREATE 绑定当前 session；SET/DESTROY 必须 owner。
    pub fn handle_wm(
        &mut self,
        ch: Handle,
        owner: u16,
        client_type: u8,
        msg: &[u8],
        hs: &[Handle],
    ) {
        if !hs.is_empty() {
            self.io.close_handles(hs);
            self.io.status(ch, gp::ST_BAD);
            return;
        }
        match msg.first() {
            Some(&gp::OP_WM_GETINFO) if gp::parse_wm_getinfo_req(msg).is_some() => self.getinfo(ch),
            Some(&gp::OP_WM_CREATE) => match gp::parse_wm_create_req(msg) {
                Some(req) => self.create(ch, owner, client_type, req),
                None => self.io.status(ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_DESTROY) => match gp::parse_wm_destroy_req(msg) {
                Some(id) => self.destroy(ch, owner, id),
                None => self.io.status(ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_SET) => match gp::parse_wm_set_req(msg) {
                Some(req) => self.set(ch, owner, req),
                None => self.io.status(ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_TABLE_GET) if gp::parse_wm_table_get_req(msg).is_some() => {
                self.table_get(ch)
            }
            Some(&gp::OP_WM_TABLE_WAIT) => match gp::parse_wm_table_wait_req(msg) {
                Some(last_seen) => self.wait(ch, last_seen),
                None => self.io.status(ch, gp::ST_BAD),
            },
            Some(&gp::OP_WM_CONFIG_POLL) if gp::parse_wm_config_poll_req(msg).is_some() => {
                self.config_poll(ch, owner)
            }
            Some(&gp::OP_WM_CONFIG_ACK) => match gp::parse_wm_config_ack_req(msg) {
                Some(req) => self.config_ack(ch, owner, req),
                None => self.io.status(ch, gp::ST_BAD),
            },
            _ => self.io.status(ch, gp::ST_BAD),
        }
    }

    fn set_visible(&mut self, i: usize, vis: bool) -> bool {
        if i >= self.count || self.wins[i].vis == vis {
            return false;
        }
        self.wins[i].vis = vis;
        self.mark_config(i, gp::WM_CONFIG_CAUSE_VISIBILITY);
        true
    }

    fn change_complete(&mut self, changed: bool) -> bool {
        if !changed {
            return false;
        }
        if let Some((_, id)) = self.focused() {
            let focus_hidden = self
                .find(id)
                .is_none_or(|i| !self.wins[i].vis || self.wins[i].layer != gp::WM_LAYER_APP);
            if focus_hidden {
                self.focus = None;
            }
        }
        self.recompute();
        self.bump();
        true
    }

    fn app_owner_indexes(&self, owner: u16) -> [usize; gp::WM_WINDOWS_MAX] {
        let mut out = [usize::MAX; gp::WM_WINDOWS_MAX];
        let mut n = 0usize;
        for i in 0..self.count {
            if self.wins[i].layer == gp::WM_LAYER_APP && self.wins[i].owner == owner {
                out[n] = i;
                n += 1;
            }
        }
        out
    }

    fn set_owner_visibility(&mut self, owner: u16, vis: bool) -> bool {
        let indexes = self.app_owner_indexes(owner);
        let mut changed = false;
        for &i in indexes.iter() {
            if i == usize::MAX {
                break;
            }
            changed |= self.set_visible(i, vis);
        }
        changed
    }

    fn focus_owner(&mut self, owner: u16) -> bool {
        let indexes = self.app_owner_indexes(owner);
        let Some(i) = indexes.iter().copied().find(|&i| i != usize::MAX) else {
            return false;
        };
        self.focus = Some(self.wins[i].id);
        true
    }

    /// System BACK (v1): hide the focused app owner and expose the desktop or
    /// the next visible app. Apps with internal navigation still receive their
    /// own keys first in a later revision; this is the global fallback.
    pub fn navigation_back(&mut self) -> Option<u16> {
        let owner = self.focused()?.0;
        if !self.set_owner_visibility(owner, false) {
            return None;
        }
        self.change_complete(true);
        Some(owner)
    }

    /// System HOME: hide every APP-layer owner. The desktop compositor's
    /// surface_id=0 is not a WM window and remains visible by construction.
    pub fn navigation_home(&mut self) -> usize {
        let mut owners = [0u16; gp::WM_WINDOWS_MAX];
        let mut n = 0usize;
        for i in 0..self.count {
            if self.wins[i].layer != gp::WM_LAYER_APP {
                continue;
            }
            let owner = self.wins[i].owner;
            if !owners[..n].contains(&owner) {
                owners[n] = owner;
                n += 1;
            }
        }
        let mut changed = false;
        for &owner in owners[..n].iter() {
            changed |= self.set_owner_visibility(owner, false);
        }
        self.change_complete(changed);
        n
    }

    /// System TASK: cycle to the next APP owner, hide other app owners, and
    /// focus that owner's first window. This is the deterministic v1 task
    /// switcher; a visual recents overlay can consume the same authority later.
    pub fn navigation_task(&mut self) -> Option<u16> {
        let mut owners = [0u16; gp::WM_WINDOWS_MAX];
        let mut n = 0usize;
        for i in 0..self.count {
            if self.wins[i].layer != gp::WM_LAYER_APP {
                continue;
            }
            let owner = self.wins[i].owner;
            if !owners[..n].contains(&owner) {
                owners[n] = owner;
                n += 1;
            }
        }
        if n == 0 {
            return None;
        }
        let current = self.focused().map(|(owner, _)| owner).or_else(|| {
            owners[..n]
                .iter()
                .copied()
                .find(|&owner| {
                    self.app_owner_indexes(owner)
                        .iter()
                        .any(|&i| i != usize::MAX && self.wins[i].vis)
                })
        });
        let next = match current {
            Some(cur) => {
                let pos = owners[..n].iter().position(|&o| o == cur)?;
                owners[(pos + 1) % n]
            }
            None => owners[0],
        };
        let mut changed = false;
        for &owner in owners[..n].iter() {
            changed |= self.set_owner_visibility(owner, owner == next);
        }
        self.focus_owner(next);
        self.change_complete(changed || true);
        Some(next)
    }

    /// session 死亡清理：一次性摘除该 owner 全部窗口。返回清理数。
    pub fn destroy_owned(&mut self, owner: u16) -> usize {
        let mut removed = 0usize;
        let mut i = 0usize;
        while i < self.count {
            if self.wins[i].owner != owner {
                i += 1;
                continue;
            }
            for j in i..self.count - 1 {
                self.wins[j] = self.wins[j + 1];
            }
            self.count -= 1;
            removed += 1;
        }
        if removed > 0 {
            self.refocus_after_removal();
            self.recompute();
            self.bump();
        }
        removed
    }
}
