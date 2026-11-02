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

use crate::{close_all, now_ns, send_rep, st_rep, trap};
use berry_guiproto as gp;
use berry_sys::Handle;

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
        };
        WmTable {
            wins: [blank; gp::WM_WINDOWS_MAX],
            count: 0,
            focus: None,
            version: ((epoch as u32) << 24) | 1,
            next_id: 0,
            parked: None,
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
        if let Some((ch, _)) = self.parked.take() {
            self.send_snapshot(ch);
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
    /// 驱动有效几何避让。应答 8B ack(window_id)。
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
        };
        self.count += 1;
        self.focus = Some(id);
        self.recompute();
        self.bump();
        let mut out = [0u8; 8]; // = WM_CREATE_ACK_LEN（guiproto 私有——本地定容）
        let n = gp::encode_wm_create_ack(&mut out, id);
        send_rep(ch, &out[..n]);
    }

    /// WM_DESTROY：摘窗（表序移除保建序——hit-test 平局表序语义）；持
    /// 焦点则归还；系统窗摘除 = 解除避让（recompute）。
    fn destroy(&mut self, ch: Handle, id: u16) {
        let Some(i) = self.find(id) else {
            st_rep(ch, gp::ST_NOENT);
            return;
        };
        for j in i..self.count - 1 {
            self.wins[j] = self.wins[j + 1];
        }
        self.count -= 1;
        if self.focus == Some(id) {
            self.refocus_after_removal();
        }
        self.recompute();
        self.bump();
        st_rep(ch, gp::ST_OK);
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

    /// wm 域分发（wm_srv 面）：六 op 之外（0x66..=0x7F 未定形）1B
    /// ST_BAD；wm 域六 op **零随行 handle**——非空即拒面 close。
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
            _ => st_rep(ch, gp::ST_BAD),
        }
    }
}
