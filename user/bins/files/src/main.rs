//! files —— 波 4 第二件真 GUI 应用（FILES appstub 秒退壳的替换；
//! .agents/plans/2026-11-15-波4预制应用翻新实施设计.md §2.2，先例 =
//! settings 垂直切片 9035af061）。
//!
//! **使命**：双根浏览页（apps / data）+ 面包屑 chip 逐级跳层 + 过滤
//! chip（v1 固定词表降解）+ Scroll[List] 目录条目（pattern::list_item
//! 名 + 类型角标、目录在前）+ 文件条目只读详情（Dialog 降解为应用内
//! 瞬态行）。零手拼铁律：卡片/条目 = berry-ui pattern 配方
//! （[`list_item`]/[`chip`]/[`stack`]/[`top_bar`]），按压反馈色单源 =
//! pattern [`ItemState`] 五态预混表（theme.rs token 消费面），app 面
//! 零自混色、零手写 Rect 拼卡。
//!
//! **fs 面**（data 根真读 FAT12 数据盘）：berry-fsclient 直面
//! （`readdir` 列名 + `stat` 定型 + `statfs` 空闲），礼物 = manifest
//! `fs = ["rootfs"]` 词 → svcmgr fs 四元装配的 `fs_h()`（env hs[5]）。
//! 纪律照 launcher layout_io：严格串行单 RPC 会话（fsclient 会话机
//! 内建——一请求一应答、txid 匹配、收侧超时毒化）、失败非致命（目录
//! 读失败 → Empty 态如实展示，页面常驻不死）。/apps 树 v1 如缺席
//! （launcher 预建 mkdir 未落——波 3 §2.2）→ Empty 如实。
//!
//! **GUI 通道形态**（svcmgr app_launch GUI 授权链 5452649af）：本 app
//! 声明 fs 词 + [ui] 节 → gift 槽位 `[rootfs, fs, wm, input]`——wm 落
//! extras[2]（berry-libc io.rs GIFT_H[4]；`wm_cli_h()` 的 [0] 位对
//! wants_fs 程序承载 rootfs VMO，不可用）。取用经**同槽访问器**
//! [`berry_libc::io::wm_present_h`]（io.rs「同一 gift 位对不同程序
//! 承载不同对象」的多产面惯例——extras[2] 对 gui-wmd 承载 present
//! 客户端端、对本 app 承载 wm 客户端端，同为 wmd 通道客户端端）；
//! 专用访问器挂 appshell 裁决 A（波 3 §6）落地换轨。input 固定位
//! [3] = `input_h()` 不受影响。
//!
//! **帧循环/退出形态照 settings 先例**：wm 门控（0 = 一行诊断 + park
//! 常驻不秒退）→ WM_GETINFO 几何 → present GETINFO 轻断言 → 帧 VMO
//! 三连 → vsync 协商 → 主循环（INPUT_POLL 批拉 → push_pointer →
//! pump_input（referee + widget_react 同拍）→ Msg 消费（行按压/滚动/
//! 点击导航）→ take_events（Up 按钮 + chip 词选）→ render_frame →
//! SUBMIT/FRAME_CALLBACK/RELEASE_WAIT 严格内联串行 → vote 变化才发 →
//! wait_many n≤2 + IDLE 帽）。无退出路径——fail() = eprintln + park，
//! 崩溃监督归 svcmgr app_supervise 域。首帧回执 `files: first frame`
//! （诊断面）。
//!
//! **v1 降解面**（3b/appshell 未落——详 W4-IMPL-FILES.md 差异清单）：
//! Search 无 softimed → 固定词表 chip 前缀过滤替代敲键；Dialog → 应
//! 用内瞬态行（2.5s 自撤）；无 wm back → 面包屑 chip + Up 按钮承担
//! 全部回退导航；fs 读同步阻塞帧循环（无异步 fs 面）。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::io::{fs_h, input_h, self_vmar_h, wm_present_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_many, berry_object_wait_one, berry_vmar_map,
    berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_fsclient::{
    dirent_next, readdir, stat as fs_stat, statfs as fs_statfs, FsErr, NAME_MAX, PATH_DEPTH_MAX,
    PATH_MAX, RPC_BUF, TYPE_DIR,
};
use berry_ui::desc::{
    ButtonProps, ButtonStyle, ColProps, RectProps, RowProps, ScrollProps, TextProps, TextStyle,
};
use berry_ui::layout::CrossAlign;
use berry_ui::pattern::{
    chip, list_item, spacer, stack, top_bar, ItemState, LISTITEM_H, LISTITEM_PAD_X,
};
use berry_ui::theme::{Theme, Tokens};
use berry_ui::{
    NodeId, ProportionalTextMetrics, Props, TextBuf, UiError, UiMain, ViewKind, ViewSpec,
    ROLE_BUTTON, ROLE_LIST_ITEM, ROLE_TEXT,
};

// 链接种子（settings/berry-launcher 同款）：rlib/LTO 按需保留——无引用
// 则 berry-libc 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（settings 同值 300s：app 拉起时点 = 用户点击，
/// 首 RPC 距 boot 稳态可远）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 帧缓冲映射 VA（settings/berry-launcher frameloop 同位）。
const FRAME_VA: usize = 0x5100_0000;

/// 默认 vsync 周期（60Hz；S2 食谱默认，settings 同值）。
const VSYNC_NS: u64 = 16_666_666;

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// 文件详情瞬态行驻留（Contacts 拨号 Toast 2s 节拍同族；2.5s 覆盖一
/// 次 vsync 唤醒余量）。
const DETAIL_HOLD_NS: u64 = 2_500_000_000;

/// glyph scratch 容量（堆置——paint 对超容 run 截断尾字形；本页最长
/// run = 详情行 ~44 字形，256 富余）。
const SCRATCH_GLYPHS: usize = 256;

/// 目录条目原始容量（READDIR blob ≤ LIST_MAX=200B 收名上界的宽松帽；
/// fstree DIR_COUNT_MAX=17 / FILE_COUNT_MAX=9 现相位恒不触）。
const ENTRIES_MAX: usize = 32;

/// 可见行帽（MAX_NODES=512 预算：骨架 ~60 + 行 ×11 → 24 行 ≈ 324）。
const ROWS_MAX: usize = 24;

/// 面包屑层级帽（wire path 深帽 PATH_DEPTH_MAX=8 内留一段余量——apps
/// 根的 "apps" 段占 1）。
const NAV_DEPTH_MAX: usize = PATH_DEPTH_MAX - 1;

/// 过滤词表字母帽（chip 行宽预算：all + 6 字母 ≤ 7 chips）。
const FILTER_MAX: usize = 6;

/// 显示名截断（行宽预算：20 字 × 16px = 320px < 行宽）。
const NAME_DISP_MAX: usize = 20;

/// 面包屑 chip 标签截断（chip 行宽预算）。
const CRUMB_MAX: usize = 8;

// —— 事件 ID 面（props on_tap 携载——take_events 消费锚）——
/// TopBar 尾 Up 按钮（Button kind on_tap）。
const EV_UP: u32 = 1;
/// 面包屑 chip 基址（chip i = 层级 i；帽 = 槽位 12 内）。
const EV_CRUMB: u32 = 16;
/// 过滤 chip 基址（chip 0 = all，1..=FILTER_MAX = 首字母）。
const EV_FILTER: u32 = 32;

// —— 空态文案（ASCII；如实的诚实边界）——
const MSG_NO_FS: &str = "fs not available";
const MSG_NO_APPS: &str = "apps tree absent";
const MSG_EMPTY: &str = "empty";
const MSG_FS_ERR: &str = "fs error";

/// 诊断 + park 收口（settings `fail` 同形——app 常驻不被 reap）。
fn fail(what: &str) -> ! {
    eprintln!("files: {what}");
    berry_libc::park()
}

/// now = clock_read；失败按 0 → fail-fast（settings 同口径）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 即关（RELEASE 归还别名——关失败 = handle 表泄漏，FAIL loud）。
fn close_h(h: Handle) {
    if berry_handle_close(h) != 0 {
        fail("handle close");
    }
}

/// 严格串行 RPC **发送半程**（settings `rpc_send` 同形）：-EBUSY → 阻塞
/// 等 WRITABLE（帽重臂）；send 原子性。**send 即 take**（§9 A6-b）。
fn rpc_send(ch: Handle, req: &[u8], attach: &[Handle]) {
    let (hptr, hn) = if attach.is_empty() {
        (core::ptr::null(), 0)
    } else {
        (attach.as_ptr(), attach.len())
    };
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(ch, req.as_ptr(), req.len(), hptr, hn);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            fail(&format!("rpc send err={s}"));
        }
        if deadline == 0 {
            deadline = now() + RPC_CAP_NS;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            fail("rpc send wait timeout");
        }
        deadline = 0; // 重臂（命中后仍满则续等——公平排队）
    }
}

/// 严格串行 RPC **收侧半程**（settings `rpc_recv` 同形）。
fn rpc_recv(ch: Handle, rep: &mut [u8; 256], rhs: &mut [Handle; 8]) -> (usize, usize) {
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(ch, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        if s == 0 {
            if nb == 0 {
                fail("rpc empty reply");
            }
            return (nb, nh);
        }
        if s != BERRY_ERR_AGAIN {
            fail(&format!("rpc recv err={s}"));
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            fail("rpc reply timeout");
        }
    }
}

/// 严格串行 RPC 单趟（settings `rpc` 同形）。
fn rpc(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (usize, usize) {
    rpc_send(ch, req, attach);
    rpc_recv(ch, rep, rhs)
}

// ---------- 纯函数面（路径/格式化/排序/词表——host 可测） ----------

/// u32 → 十进制 ASCII（定容写出，返回有效长；0 → "0"）。
fn dec_bytes(v: u32, out: &mut [u8; 12]) -> usize {
    if v == 0 {
        out[0] = b'0';
        return 1;
    }
    let mut tmp = [0u8; 12];
    let mut i = 0usize;
    let mut x = v;
    while x > 0 && i < tmp.len() {
        tmp[i] = b'0' + (x % 10) as u8;
        x /= 10;
        i += 1;
    }
    let mut n = 0usize;
    while i > 0 {
        i -= 1;
        out[n] = tmp[i];
        n += 1;
    }
    n
}

/// 名首字节归一大写（过滤词表键：a-z → A-Z，其余原样）。
fn name_key(b: u8) -> u8 {
    if b.is_ascii_lowercase() {
        b - 32
    } else {
        b
    }
}

/// 单条目录项（名全量留存——路径拼段与显示截断单源）。
#[derive(Clone, Copy)]
struct Ent {
    name: [u8; NAME_MAX],
    len: usize,
    is_dir: bool,
    size: u32,
}

impl Ent {
    fn name(&self) -> &[u8] {
        &self.name[..self.len]
    }
}

/// 目录读取产物：条目（排序后）+ 原始总数 + 空态文案。
struct Dir {
    ents: [Ent; ENTRIES_MAX],
    n: usize,
    total: usize,
    msg: Option<&'static str>,
}

impl Dir {
    fn new() -> Self {
        Dir { ents: [Ent { name: [0; NAME_MAX], len: 0, is_dir: false, size: 0 }; ENTRIES_MAX], n: 0, total: 0, msg: None }
    }

    /// 排序插入（目录在前、名 bytewise 升序——定容插入排序，帽满弃尾
    /// 并保 `total` 全量计数）。
    fn insert(&mut self, name: &[u8], is_dir: bool, size: u32) {
        self.total += 1;
        if self.n >= ENTRIES_MAX {
            return;
        }
        let e = Ent { name: { let mut b = [0u8; NAME_MAX]; let l = name.len().min(NAME_MAX); b[..l].copy_from_slice(&name[..l]); b }, len: name.len().min(NAME_MAX), is_dir, size };
        let mut i = self.n;
        while i > 0 {
            let p = &self.ents[i - 1];
            let after = if p.is_dir != e.is_dir {
                p.is_dir // 目录在前
            } else {
                p.name[..p.len] <= e.name[..e.len]
            };
            if after {
                break;
            }
            self.ents[i] = self.ents[i - 1];
            i -= 1;
        }
        self.ents[i] = e;
        self.n += 1;
    }

    /// 过滤词表（当前条目集首字母去重升序，帽 FILTER_MAX——「固定过滤
    /// 词表降解」的确定性派生面：无敲键，点选前缀过滤）。
    fn letters(&self) -> ([u8; FILTER_MAX], usize) {
        let mut out = [0u8; FILTER_MAX];
        let mut k = 0usize;
        for i in 0..self.n {
            let key = name_key(self.ents[i].name[0]);
            if !key.is_ascii_alphanumeric() {
                continue;
            }
            let mut j = 0usize;
            while j < k && out[j] < key {
                j += 1;
            }
            if j < k && out[j] == key {
                continue;
            }
            if k >= FILTER_MAX {
                break;
            }
            let mut m = k;
            while m > j {
                out[m] = out[m - 1];
                m -= 1;
            }
            out[j] = key;
            k += 1;
        }
        (out, k)
    }

    /// 过滤命中（首字母前缀，大小写归一；filter=None 全过）。
    fn matches(&self, i: usize, filter: Option<u8>) -> bool {
        match filter {
            None => true,
            Some(k) => name_key(self.ents[i].name[0]) == k,
        }
    }
}

/// 子项全径拼接（wire + NUL + 名——单源自由函数；名 ≤ NAME_MAX 由服
/// 务端名域保证，调用方帽内防御）。
fn wire_child(wire: &[u8], name: &[u8], out: &mut [u8]) -> usize {
    let mut n = wire.len();
    out[..n].copy_from_slice(wire);
    if n > 0 {
        out[n] = 0;
        n += 1;
    }
    let l = name.len().min(NAME_MAX);
    out[n..n + l].copy_from_slice(&name[..l]);
    n + l
}

/// 浏览根（双根页两入口；wire 路径前缀的单源）。
#[derive(Clone, Copy, PartialEq)]
enum Root {
    /// RAM 树 /apps（launcher 预建未落则 READDIR Noent → Empty 如实）。
    Apps,
    /// 数据盘根 = fsmgr overlay 全景（"/"——FAT12 数据盘挂载位；空
    /// wire 即根，C-1 刀 5 立法）。
    Data,
}

impl Root {
    /// 根标签（面包屑 chip 1 + 双根页条目名）。
    fn label(&self) -> &'static [u8] {
        match self {
            Root::Apps => b"apps",
            Root::Data => b"data",
        }
    }
}

/// 文件详情瞬态行（Dialog 的 v1 应用内降解——驻留 `DETAIL_HOLD_NS`
/// 自撤，帧循环节拍驱动）。
struct Detail {
    text: [u8; 64],
    len: usize,
    until: u64,
}

impl Detail {
    fn new() -> Self {
        Detail { text: [0; 64], len: 0, until: 0 }
    }
    fn active(&self) -> bool {
        self.len > 0
    }
    fn clear(&mut self) {
        self.len = 0;
    }
}

/// 应用导航/数据状态（单源——refresh 全量重建动态面的依据）。
struct St {
    /// 双根页位（true = apps/data 两行；false = 目录浏览）。
    home: bool,
    root: Root,
    segs: [[u8; NAME_MAX]; NAV_DEPTH_MAX],
    lens: [usize; NAV_DEPTH_MAX],
    depth: usize,
    dir: Dir,
    filter: Option<u8>,
    /// 数据盘空闲（STATFS 真值缓存——首次进 data 根取一次；卷级量，
    /// 浏览期不变）。
    free_kib: Option<u32>,
    detail: Detail,
    /// Scroll 偏移/内容高（refresh 归零）。
    offset: u32,
    content_h: u32,
    /// 行按压态（refresh 清——重建后行注册表换代）。
    pressed: Option<usize>,
    /// 列表拖滚跟手样本（refresh 清——重建即中止滚动手势）。
    drag_y: Option<u32>,
}

impl St {
    fn new() -> Self {
        St {
            home: true,
            root: Root::Data,
            segs: [[0; NAME_MAX]; NAV_DEPTH_MAX],
            lens: [0; NAV_DEPTH_MAX],
            depth: 0,
            dir: Dir::new(),
            filter: None,
            free_kib: None,
            detail: Detail::new(),
            offset: 0,
            content_h: 0,
            pressed: None,
            drag_y: None,
        }
    }

    /// 当前目录 wire 路径（NUL 连接段——fsproto path_valid 形；data 根
    /// = 空 wire 即根寻址）。
    fn wire(&self, out: &mut [u8]) -> usize {
        let mut n = 0usize;
        if self.root == Root::Apps {
            out[..4].copy_from_slice(b"apps");
            n = 4;
        }
        for i in 0..self.depth {
            if n > 0 {
                out[n] = 0;
                n += 1;
            }
            let l = self.lens[i];
            out[n..n + l].copy_from_slice(&self.segs[i][..l]);
            n += l;
        }
        n
    }

    /// 当前 wire 路径长（PATH_MAX 域帽执法的度量面——零写测长）。
    fn wire_len(&self) -> usize {
        let mut n = if self.root == Root::Apps { 4 } else { 0 };
        for i in 0..self.depth {
            n += 1 + self.lens[i]; // NUL + 段
        }
        n
    }

    /// 压层（返回 false = 深帽/路径长帽拒——调用方不动；PATH_MAX 单源
    /// 域帽：超帽路径服务端 path_valid 拒，客户端先拒免白跑 RPC）。
    fn push_seg(&mut self, name: &[u8]) -> bool {
        if self.depth >= NAV_DEPTH_MAX {
            return false;
        }
        let l = name.len().min(NAME_MAX);
        let sep = if self.wire_len() > 0 { 1 } else { 0 };
        if self.wire_len() + sep + l > PATH_MAX {
            return false;
        }
        self.segs[self.depth][..l].copy_from_slice(&name[..l]);
        self.lens[self.depth] = l;
        self.depth += 1;
        true
    }
}

/// 目录真读（READDIR 列名 + 逐名 STAT 定型；fsclient 严格串行单会话，
/// 阻塞最坏 = FS_COMMIT_CAP_NS 90s——TCG 链走查成本，v1 同步降解）。
/// 失败非致命：`msg` 空态文案如实（layout_io 三约束纪律）。
fn read_dir(h: u32, wire: &[u8], noent_msg: &'static str, dir: &mut Dir) {
    dir.n = 0;
    dir.total = 0;
    dir.msg = None;
    if h == 0 {
        dir.msg = Some(MSG_NO_FS);
        return;
    }
    let mut buf = [0u8; RPC_BUF];
    let blob = match readdir(h, wire, &mut buf) {
        Ok(n) => n,
        Err(FsErr::Noent) => {
            dir.msg = Some(noent_msg);
            return;
        }
        Err(FsErr::NoFs) => {
            dir.msg = Some(MSG_NO_FS);
            return;
        }
        Err(_) => {
            dir.msg = Some(MSG_FS_ERR);
            return;
        }
    };
    let mut pos = 0usize;
    let mut child = [0u8; RPC_BUF];
    while let Some(name) = dirent_next(&buf[..blob], &mut pos) {
        if name.is_empty() || name.len() > NAME_MAX {
            continue; // 防御（服务端名域已执法——双保险不猜）
        }
        let cl = wire_child(wire, name, &mut child);
        // STAT 定型：目录在前排序的依据 + 详情尺寸。定型失败 = 诚实
        // 降级按文件（角标 file）——条目不丢（列名已证在场）。
        let (is_dir, size) = match fs_stat(h, &child[..cl]) {
            Ok((ty, sz)) => (ty == TYPE_DIR, sz),
            Err(_) => (false, 0),
        };
        dir.insert(name, is_dir, size);
    }
    if dir.n == 0 && dir.msg.is_none() {
        dir.msg = Some(MSG_EMPTY);
    }
}

// ---------- UI 面 ----------

/// ItemState 五态底色 → Color（pattern 单源表消费——app 零自混色）。
fn state_color(s: ItemState) -> Color {
    let c = s.bg();
    Color { r: (c >> 16) as u8, g: (c >> 8) as u8, b: c as u8 }
}

/// Text 叶 spec（settings `text_spec` 同形；超 TEXT_INLINE=64B loud）。
fn text_spec(text: &[u8], fg: Color, scale: FontSize) -> Result<ViewSpec, UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(text)?,
        }),
    })
}

/// Col 容器 spec。
fn col_spec(w: Option<u32>, h: Option<u32>, gap: u32, cross: CrossAlign) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Col,
        key: 0,
        props: Props::Col(ColProps { gap, w, h, cross }),
    }
}

/// 档位几何（settings 档位判别同形：OH 1080×1920 big / 800×600 紧凑）。
#[derive(Clone, Copy)]
struct Metrics {
    content_w: u32,
    line_h: u32,
    row_gap: u32,
    crumb_h: u32,
    list_h: u32,
    name_size: FontSize,
    meta_size: FontSize,
    chip_gap: u32,
}

fn metrics(vp_w: u32, vp_h: u32) -> Metrics {
    let big = vp_w >= 600 && vp_h >= 1800;
    if big {
        let content_w = vp_w - 96; // 16vp×2 padding
        // 恒高带：TopBar 168 + 24 缝 + 面包屑 48 + 12 + 过滤 48 + 12 +
        // 状态行 48 + 12 + 详情行 48 + 24 底 pad = 444；余给列表。
        let list_h = vp_h.saturating_sub(444);
        Metrics {
            content_w,
            line_h: 48,
            row_gap: 12,
            crumb_h: 48,
            list_h,
            name_size: FontSize::S16x32,
            meta_size: FontSize::S8x16,
            chip_gap: 16,
        }
    } else {
        let content_w = vp_w.saturating_sub(32).max(8);
        let list_h = vp_h.saturating_sub(168 + 12 + 48 + 8 + 48 + 8 + 32 + 8 + 32 + 12);
        Metrics {
            content_w,
            line_h: 32,
            row_gap: 8,
            crumb_h: 48,
            list_h,
            name_size: FontSize::S8x16,
            meta_size: FontSize::S8x16,
            chip_gap: 8,
        }
    }
}

/// 行命中注册（Msg 最深叶命中面——launcher 槽命中映射同律：一行全部
/// 可命中节点归本格；press = 五态按压层 Rect 节点）。
#[derive(Clone, Copy)]
struct RowReg {
    hits: [NodeId; 3],
    press: NodeId,
}

/// 页面句柄（骨架一次落定；动态面 = crumb/filter/list 三锚的子树重建；
/// 可选位 = 双根页/重建间隙的缺席态——NodeId 无公共零值构造）。
struct Page {
    crumb_anchor: NodeId,
    filter_anchor: NodeId,
    scroll: NodeId,
    list_col: NodeId,
    status: NodeId,
    detail: NodeId,
    /// 当前 Up 按钮（双根页无；随面包屑行重建，press_map 同步重挂）。
    up_btn: Option<NodeId>,
    crumb_row: Option<NodeId>,
    filter_row: Option<NodeId>,
    kids: [Option<NodeId>; ROWS_MAX + 1],
    kids_n: usize,
    rows: [Option<RowReg>; ROWS_MAX],
    rows_n: usize,
    m: Metrics,
    t: Tokens,
}

/// 建骨架（一次）：TopBar（pattern 配方）+ 面包屑/过滤/列表三锚 + 状态
/// 与详情两行。Err = 容量/文本域违例 → 调用方 fail 收口。
fn build(ui: &mut UiMain) -> Result<Page, UiError> {
    let (vp_w, vp_h) = ui.viewport();
    let m = metrics(vp_w, vp_h);
    let t = Tokens::pick(Theme::Light);
    let root = ui.root();
    let page = ui.add(root, col_spec(Some(vp_w), Some(vp_h), 0, CrossAlign::Center))?;
    // TopBar（pattern 配方——56vp 高、标题起始 16vp）。
    top_bar(ui, page, &t, b"Files", &[])?;
    spacer(ui, page, 1, if vp_h >= 1800 { 24 } else { 12 })?;
    // 面包屑锚（行级重建：Up 按钮 + 逐层 chip）。
    let crumb_anchor = ui.add(page, col_spec(Some(m.content_w), None, 0, CrossAlign::Start))?;
    spacer(ui, page, 1, if vp_h >= 1800 { 12 } else { 8 })?;
    // 过滤词表锚（v1 Search 降解——chip 行）。
    let filter_anchor = ui.add(page, col_spec(Some(m.content_w), None, 0, CrossAlign::Start))?;
    spacer(ui, page, 1, if vp_h >= 1800 { 12 } else { 8 })?;
    // 列表区：定高 Col（视口语义单源）→ Scroll（offset/clip 面）→ 行列。
    let zone = ui.add(page, col_spec(Some(m.content_w), Some(m.list_h), 0, CrossAlign::Stretch))?;
    let scroll = ui.add(
        zone,
        ViewSpec {
            kind: ViewKind::Scroll,
            key: 0,
            props: Props::Scroll(ScrollProps { content_height: 0, offset_y: 0 }),
        },
    )?;
    let list_col = ui.add(scroll, col_spec(Some(m.content_w), None, m.row_gap, CrossAlign::Stretch))?;
    spacer(ui, page, 1, if vp_h >= 1800 { 12 } else { 8 })?;
    // 状态行（items/free——定高容器内 Text，空文不塌）。
    let status_zone = ui.add(page, col_spec(Some(m.content_w), Some(m.line_h), 0, CrossAlign::Start))?;
    let status = ui.add(status_zone, text_spec(b"apps + data", t.font_secondary, m.meta_size)?)?;
    let _ = ui.set_a11y(status, ROLE_TEXT, b"status");
    spacer(ui, page, 1, if vp_h >= 1800 { 12 } else { 8 })?;
    // 详情瞬态行（Dialog 的 v1 应用内降解——文件条目只读详情，2.5s 自撤）。
    let detail_zone = ui.add(page, col_spec(Some(m.content_w), Some(m.line_h), 0, CrossAlign::Start))?;
    let detail = ui.add(detail_zone, text_spec(b"", t.font_primary, m.meta_size)?)?;
    let _ = ui.set_a11y(detail, ROLE_TEXT, b"detail");
    Ok(Page {
        crumb_anchor,
        filter_anchor,
        scroll,
        list_col,
        status,
        detail,
        up_btn: None,
        crumb_row: None,
        filter_row: None,
        kids: [None; ROWS_MAX + 1],
        kids_n: 0,
        rows: [None; ROWS_MAX],
        rows_n: 0,
        m,
        t,
    })
}

/// 单行装配（零手拼铁律：条目本体 = pattern::list_item Card；按压层 =
/// Stack 面层 Rect（pattern app_bar/top_bar 同款叠层习语），色单源 =
/// ItemState 五态表；内容行 = [pad, 名, 推位, 类型角标, pad]）。
#[allow(clippy::too_many_arguments)]
fn build_row(
    ui: &mut UiMain,
    page: &mut Page,
    name: &[u8],
    is_dir: bool,
) -> Result<usize, UiError> {
    if page.rows_n >= ROWS_MAX || page.kids_n >= page.kids.len() {
        return Ok(0); // 帽满静默止（状态行 total 口径如实）
    }
    let t = page.t;
    let m = page.m;
    let idx = page.rows_n;
    // 显示名截断（全量名留 St——路径拼段单源）。
    let dn = &name[..name.len().min(NAME_DISP_MAX)];
    let badge: &[u8] = if is_dir { b"DIR" } else { b"FILE" };
    let sk = stack(ui, page.list_col)?;
    // 卡片本体（elevation 0、无前导图标）。
    list_item(ui, sk, &t)?;
    // 五态按压层（Normal = 卡面白——常态隐形；Pressed = brand 20% 预混）。
    let press = ui.add(
        sk,
        ViewSpec {
            kind: ViewKind::Rect,
            key: 0,
            props: Props::Rect(RectProps {
                bg: state_color(ItemState::Normal),
                w: None,
                h: None, radius: 0 }),
        },
    )?;
    // 内容行（置顶层——文本盖按压层之上）。
    let crow = ui.add(
        sk,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: m.chip_gap,
                w: Some(m.content_w),
                h: Some(LISTITEM_H),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    spacer(ui, crow, LISTITEM_PAD_X, 1)?;
    let name_node = ui.add(crow, text_spec(dn, t.font_primary, m.name_size)?)?;
    let name_w = dn.len() as u32 * m.name_size.cell_w() as u32;
    let badge_w = badge.len() as u32 * m.meta_size.cell_w() as u32;
    let flex = m
        .content_w
        .saturating_sub(2 * LISTITEM_PAD_X)
        .saturating_sub(name_w)
        .saturating_sub(badge_w)
        .saturating_sub(4 * m.chip_gap);
    spacer(ui, crow, flex, 1)?;
    let badge_node = ui.add(crow, text_spec(badge, t.font_secondary, m.meta_size)?)?;
    spacer(ui, crow, LISTITEM_PAD_X, 1)?;
    let _ = ui.set_a11y(name_node, ROLE_LIST_ITEM, dn);
    page.rows[idx] = Some(RowReg { hits: [crow, name_node, badge_node], press });
    page.rows_n += 1;
    page.kids[page.kids_n] = Some(sk);
    page.kids_n += 1;
    Ok(idx)
}

/// 动态面全量重建（导航/过滤/空态后调用——crumb 行、过滤行、列表行、
/// 状态/详情文本、Scroll 几何；子树 remove 连带 a11y/press 清理 = 引
/// 擎 D12 面，重建面零泄漏）。
fn refresh(ui: &mut UiMain, page: &mut Page, st: &mut St) {
    if let Err(e) = refresh_inner(ui, page, st) {
        fail(&format!("refresh err={e:?}"));
    }
    // 换代收口：滚动归零 + 按压/拖滚态清（行注册表已换代）。
    st.offset = 0;
    st.pressed = None;
    st.drag_y = None;
    ui.wake(now());
}

fn refresh_inner(ui: &mut UiMain, page: &mut Page, st: &mut St) -> Result<(), UiError> {
    let t = page.t;
    let m = page.m;

    // —— 面包屑行（Up 按钮 + 逐层 chip；chip i = 层级 i）——
    if let Some(c) = page.crumb_row {
        ui.remove(c)?;
    }
    page.up_btn = None;
    let crumb = ui.add(
        page.crumb_anchor,
        ViewSpec {
            kind: ViewKind::Row,
            key: 0,
            props: Props::Row(RowProps {
                gap: m.chip_gap,
                w: Some(m.content_w),
                h: Some(m.crumb_h),
                cross: CrossAlign::Center,
            }),
        },
    )?;
    page.crumb_row = Some(crumb);
    if !st.home {
        // Up 按钮（Button kind——on_tap 事件 + PressedVis 按压视觉均引
        // 擎内建；「TopBar 右 icon_button=Up」的 v1 承载位注：top_bar
        // 动作位 = Icon kind 无注册字形不可见，故承载于此行首）。
        let up = ui.add(
            crumb,
            ViewSpec {
                kind: ViewKind::Button,
                key: 0,
                props: Props::Button(ButtonProps {
                    label: TextBuf::new(b"Up")?,
                    style: ButtonStyle::Filled,
                    on_tap: EV_UP,
                }),
            },
        )?;
        let _ = ui.set_a11y(up, ROLE_BUTTON, b"up one level");
        page.up_btn = Some(up);
    }
    // 层 chip 0 = Files（双根页位）。
    chip(ui, crumb, b"Files", st.home, EV_CRUMB)?;
    if !st.home {
        let mut lab = [0u8; CRUMB_MAX];
        let rl = root_label(&st.root, &mut lab);
        chip(ui, crumb, &lab[..rl], st.depth == 0, EV_CRUMB + 1)?;
        for i in 0..st.depth {
            let mut lab = [0u8; CRUMB_MAX];
            let l = seg_label(st, i, &mut lab);
            chip(ui, crumb, &lab[..l], i + 1 == st.depth, EV_CRUMB + 2 + i as u32)?;
        }
    }
    // Up 按钮按压视觉（engine press_map——Button 在 visual 白名单，色
    // 单源 = paint PressedVis 查表；整表替换语义随重建重挂）。
    match page.up_btn {
        Some(up) => ui.set_press_map(&[(up, up)])?,
        None => ui.set_press_map(&[])?,
    }

    // —— 过滤词表行（v1 Search 降解：all + 首字母 chip 前缀过滤）——
    if let Some(f) = page.filter_row {
        ui.remove(f)?;
        page.filter_row = None;
    }
    if !st.home && st.dir.n > 0 {
        let (letters, k) = st.dir.letters();
        let frow = ui.add(
            page.filter_anchor,
            ViewSpec {
                kind: ViewKind::Row,
                key: 0,
                props: Props::Row(RowProps {
                    gap: m.chip_gap,
                    w: Some(m.content_w),
                    h: Some(m.crumb_h),
                    cross: CrossAlign::Center,
                }),
            },
        )?;
        chip(ui, frow, b"all", st.filter.is_none(), EV_FILTER)?;
        for j in 0..k {
            let lb = [letters[j], 0];
            chip(ui, frow, &lb[..1], st.filter == Some(letters[j]), EV_FILTER + 1 + j as u32)?;
        }
        page.filter_row = Some(frow);
    }

    // —— 列表行（双根页两固定行 / 目录条目：目录在前 + 过滤命中）——
    for i in 0..page.kids_n {
        if let Some(k) = page.kids[i] {
            ui.remove(k)?;
            page.kids[i] = None;
        }
    }
    page.kids_n = 0;
    page.rows_n = 0;
    if st.home {
        build_row(ui, page, Root::Apps.label(), true)?;
        build_row(ui, page, Root::Data.label(), true)?;
    } else if let Some(msg) = st.dir.msg {
        let kid = ui.add(page.list_col, text_spec(msg.as_bytes(), t.font_secondary, m.meta_size)?)?;
        page.kids[page.kids_n] = Some(kid);
        page.kids_n += 1;
    } else {
        let mut shown = 0usize;
        for i in 0..st.dir.n {
            if !st.dir.matches(i, st.filter) {
                continue;
            }
            let e = st.dir.ents[i];
            build_row(ui, page, e.name(), e.is_dir)?;
            shown += 1;
            if shown >= ROWS_MAX {
                break;
            }
        }
        if shown == 0 {
            // 过滤零命中（词表点选后的如实空态——非目录空）。
            let kid = ui.add(page.list_col, text_spec(b"no match", t.font_secondary, m.meta_size)?)?;
            page.kids[page.kids_n] = Some(kid);
            page.kids_n += 1;
        }
    }
    // Scroll 几何（内容高 = 行数 × (LISTITEM_H + 缝) - 缝；偏移随刷新
    // 归零——导航换页语义）。
    let rows = page.rows_n as u32;
    let content_h = if rows > 0 {
        rows * (LISTITEM_H + m.row_gap) - m.row_gap
    } else {
        m.line_h.max(m.row_gap)
    };
    st.content_h = content_h;
    ui.set_props(
        page.scroll,
        Props::Scroll(ScrollProps { content_height: content_h, offset_y: 0 }),
    )?;

    // —— 状态行（items n（of total）/ free m KiB）——
    let mut sb = [0u8; 64];
    let mut sn;
    if st.home {
        sb[..10].copy_from_slice(b"2 roots: [");
        sn = 10;
    } else {
        sb[..6].copy_from_slice(b"items ");
        sn = 6;
    }
    let mut db = [0u8; 12];
    if st.home {
        let l1 = Root::Apps.label();
        sb[sn..sn + l1.len()].copy_from_slice(l1);
        sn += l1.len();
        sb[sn] = b' ';
        sb[sn + 1] = b'+';
        sb[sn + 2] = b' ';
        sn += 3;
        let l2 = Root::Data.label();
        sb[sn..sn + l2.len()].copy_from_slice(l2);
        sn += l2.len();
        sb[sn.min(63)] = b']';
    } else {
        let vis = visible_count(st) as u32;
        sn += copy_dec(&mut sb, sn, vis, &mut db);
        if (vis as usize) < st.dir.total {
            sb[sn..sn + 4].copy_from_slice(b" of ");
            sn += 4;
            sn += copy_dec(&mut sb, sn, st.dir.total as u32, &mut db);
        }
        if let Some(kib) = st.free_kib {
            if st.root == Root::Data {
                sb[sn..sn + 7].copy_from_slice(b" / free ");
                sn += 7;
                sn += copy_dec(&mut sb, sn, kib, &mut db);
                sb[sn..sn + 4].copy_from_slice(b" KiB");
                sn += 4;
            }
        }
    }
    ui.set_text(page.status, &sb[..sn.min(64)])?;

    // —— 详情瞬态行（清除态 = 空文；活跃态由触发点 set_text）——
    if !st.detail.active() {
        ui.set_text(page.detail, b"")?;
    }
    Ok(())
}

/// 可见行数（过滤命中 × 行帽——状态行口径）。
fn visible_count(st: &St) -> usize {
    let hit = (0..st.dir.n).filter(|&i| st.dir.matches(i, st.filter)).count();
    hit.min(ROWS_MAX)
}

/// 十进制写出辅助（返回写出长）。
fn copy_dec(sb: &mut [u8; 64], at: usize, v: u32, db: &mut [u8; 12]) -> usize {
    let dn = dec_bytes(v, db);
    let end = (at + dn).min(sb.len());
    let n = end - at;
    sb[at..end].copy_from_slice(&db[..n]);
    n
}

/// 根标签写出（截断 CRUMB_MAX）。
fn root_label(r: &Root, out: &mut [u8; CRUMB_MAX]) -> usize {
    let l = r.label();
    let n = l.len().min(CRUMB_MAX);
    out[..n].copy_from_slice(&l[..n]);
    n
}

/// 层段标签写出（截断 CRUMB_MAX）。
fn seg_label(st: &St, i: usize, out: &mut [u8; CRUMB_MAX]) -> usize {
    let n = st.lens[i].min(CRUMB_MAX);
    out[..n].copy_from_slice(&st.segs[i][..n]);
    n
}

/// 命中行（最深叶命中 → 行注册表；launcher 槽命中映射同律）。
fn row_of(page: &Page, node: NodeId) -> Option<usize> {
    (0..page.rows_n).find(|&i| {
        page.rows[i].map_or(false, |r| r.hits.contains(&node))
    })
}

/// 行按压视觉置位/复位（色单源 = ItemState 五态表）。
fn row_press(ui: &mut UiMain, page: &Page, row: usize, on: bool) {
    let Some(r) = page.rows[row] else { return };
    let st = if on { ItemState::Pressed } else { ItemState::Normal };
    let props = Props::Rect(RectProps { bg: state_color(st), w: None, h: None, radius: 0 });
    if let Err(e) = ui.set_props(r.press, props) {
        eprintln!("files: press set err={e:?}");
    }
}

fn main() {
    // —— 降级门：wm 客户端端未交付 → 一行诊断 + park 常驻（settings
    //    同形；窗口义务语义不折衷成 exit(0) 秒退）。wants_fs GUI app 的
    //    wm 礼物落 extras[2]（模块头注——wm_present_h 同槽访问器）。
    let hw = wm_present_h();
    if hw == 0 {
        eprintln!("files: wm absent (no gui gift)");
        berry_libc::park();
    }

    // —— WM_GETINFO 一次握手（几何真值源；parse 失败 = 诊断 + park）。
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_getinfo_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let info = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("wm getinfo rep bad st={}", rep[0])),
    };
    eprintln!(
        "files: wm v={} {}x{} input={} fs={}",
        info.version_wm,
        info.disp_w,
        info.disp_h,
        input_h(),
        if fs_h() == 0 { 0 } else { 1 }
    );

    // —— UI arena 堆置（settings 同形：UiMain 构造中间体 ~29KB 落栈——
    //    stack_pages=64 前提；Box::leak = 常驻面无回收语义）。
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    static TEXT_METRICS: ProportionalTextMetrics = ProportionalTextMetrics;
    ui.set_text_metrics(&TEXT_METRICS);

    // —— 建树（骨架 + 双根页首帧；fs 零 RPC——首触在进目录时）。
    let mut page = match build(ui) {
        Ok(p) => p,
        Err(e) => fail(&format!("page build err={e:?}")),
    };
    let mut st = St::new();
    refresh(ui, &mut page, &mut st);

    // —— 帧循环（常驻义务——永不返回；settings 食谱 + fs/导航/滚动面）。
    run(hw, input_h(), ui, &mut page, &mut st);
}

/// 帧循环 + 输入 + 事件消费 + fs 读取（settings `run` 的应用扩展形：
/// 增 Msg 行按压/滚动/点击导航、take_events 的 Up/chip 事件、详情行
/// 瞬态节拍）。
fn run(
    hw: Handle,
    hi_in: Handle,
    ui: &'static mut UiMain,
    page: &mut Page,
    st: &mut St,
) -> ! {
    // —— present GETINFO 轻断言 + 几何一致性（settings 同形）——
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = match gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo) {
        Some(m) => m,
        None => fail("present getinfo encode"),
    };
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let pinfo = match gp::parse_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("present getinfo bad st={}", rep[0])),
    };
    if pinfo.version_present != gp::VER_PRESENT
        || pinfo.format_bits & gp::FMT_XRGB8888 == 0
        || pinfo.max_credits != gp::CREDITS_MAX
    {
        fail(&format!(
            "present getinfo vp={} fmt={:#x} credits={}",
            pinfo.version_present, pinfo.format_bits, pinfo.max_credits
        ));
    }
    let (vp_w, vp_h) = ui.viewport();
    if pinfo.display_w != vp_w || pinfo.display_h != vp_h {
        fail(&format!(
            "disp mismatch wm {}x{} present {}x{}",
            vp_w, vp_h, pinfo.display_w, pinfo.display_h
        ));
    }

    // —— 跨代清账（settings 同形）——
    let m = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
        Some(m) => m,
        None => fail("release encode"),
    };
    let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    if gp::parse_release_rep(&rep[..n]).is_none() {
        fail(&format!("boot release bad st={}", rep[0]));
    }
    for h in &rhs[..nh] {
        close_h(*h);
    }

    // —— 帧缓冲 VMO 三连（settings 同形）——
    let (w, h) = (pinfo.display_w as usize, pinfo.display_h as usize);
    let stride = w.checked_mul(4).unwrap_or_else(|| fail("stride overflow"));
    let frame_bytes = stride.checked_mul(h).unwrap_or_else(|| fail("frame overflow"));
    let len = frame_bytes
        .checked_add(0xFFF)
        .map(|v| v & !0xFFF)
        .unwrap_or_else(|| fail("page round overflow"));
    let mut vmo: Handle = 0;
    if berry_vmo_create(len as u64, 0, &mut vmo) != 0 || vmo == 0 {
        fail("vmo create");
    }
    if berry_vmo_commit(vmo, 0, len as u64) != 0 {
        fail("vmo commit");
    }
    let hv = self_vmar_h();
    if hv == 0 {
        fail("no self vmar gift");
    }
    let mut va = FRAME_VA;
    if berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0 || va != FRAME_VA
    {
        fail("vmar map");
    }
    let desc = match TextureDesc::new(w, h, stride, TextureFormat::Rgbx) {
        Ok(d) => d,
        Err(e) => fail(&format!("texture desc {e:?}")),
    };
    // SAFETY: [FRAME_VA, FRAME_VA+len) 刚由 vmar_map 落位且整段已 commit
    //（settings 同形）；run 永不返回——fb 终身持有。
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };

    // —— glyph scratch 堆置（paint.rs 头注契约）——
    let scratch: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());

    // —— vsync 协商 + 起振（settings 同形）——
    ui.set_vsync(VSYNC_NS, 0);
    ui.wake(now());

    let t_tokens = page.t;
    let clear = (
        t_tokens.bg_page.r,
        t_tokens.bg_page.g,
        t_tokens.bg_page.b,
    );

    let mut last_expected: u64 = 0;
    let mut last_vote: Option<berry_ui::frame::VoteBallot> = None;
    let mut presented: u64 = 0;
    let mut buf_id: u32 = 1;
    let mut hi = hi_in;
    let mut input_pending = false;
    let mut input_buf = [0u8; gp::INPUT_REP_MAX];
    let mut input_hs = [0 as Handle; 8];

    loop {
        let t_now = now();

        // —— 真实输入（settings 同形）：先收上一轮 INPUT_POLL 应答，再
        //    挂下一轮；事件逐条 push 进引擎。
        if hi != 0 && input_pending {
            let mut nb = input_buf.len();
            let mut nh = input_hs.len();
            let s = berry_channel_recv(
                hi,
                input_buf.as_mut_ptr(),
                &mut nb,
                input_hs.as_mut_ptr(),
                &mut nh,
            );
            if s == 0 {
                input_pending = false;
                for h in &input_hs[..nh] {
                    close_h(*h);
                }
                match gp::parse_input_events_rep(&input_buf[..nb]) {
                    Some(Ok(view)) => {
                        for i in 0..view.count() {
                            if let Some(ev) = view.event(i) {
                                let pe = berry_ui::hit::PtrEvent::from_wire(
                                    ev.phase,
                                    ev.pointer_id,
                                    ev.x,
                                    ev.y,
                                    ev.timestamp_ns,
                                    ev.buttons,
                                    ev.pressure,
                                );
                                if let Err(e) = ui.push_pointer(pe) {
                                    eprintln!("files: input push err={e:?}");
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {}
                    Some(Err(stt)) => eprintln!("files: input poll st={stt}"),
                    None => eprintln!("files: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("files: input recv err={s}");
                hi = 0;
                input_pending = false;
            }
        }
        if hi != 0 && !input_pending {
            if input_arm(hi) {
                input_pending = true;
            } else {
                hi = 0; // 输入面失效 → 静态页续活（settings 同形）
            }
        }

        // —— 输入泵（referee + widget_react 同拍）+ Msg 消费 ——
        let msgs = ui.pump_input(last_expected);
        for msg in msgs.iter() {
            match msg {
                berry_ui::desc::Msg::Press { node, .. } => {
                    if let Some(r) = row_of(page, *node) {
                        row_press(ui, page, r, true);
                        st.pressed = Some(r);
                        ui.wake(now());
                    }
                }
                berry_ui::desc::Msg::Release { node, .. }
                | berry_ui::desc::Msg::Cancel { node } => {
                    // 行按压还原（任意 Release/Cancel 收口——含指位滑出
                    // 本行的常态；node 仅为命中诊断面）。
                    if let Some(r) = st.pressed.take() {
                        row_press(ui, page, r, false);
                        ui.wake(now());
                    }
                    let _ = node;
                }
                berry_ui::desc::Msg::Click { node } => {
                    if let Some(r) = row_of(page, *node) {
                        on_row(ui, page, st, r);
                    }
                }
                berry_ui::desc::Msg::DragStart { node, x, y, .. } => {
                    // 列表域起拖 = 拖滚（行按压视觉先撤——点击语义取消）。
                    if let Some(r) = st.pressed.take() {
                        row_press(ui, page, r, false);
                    }
                    if in_list(ui, page, *x, *y) || row_of(page, *node).is_some() {
                        st.drag_y = Some(*y);
                        ui.wake(now());
                    }
                }
                berry_ui::desc::Msg::DragMove { y, .. } => {
                    if let Some(ly) = st.drag_y {
                        scroll_by(ui, page, st, ly as i64 - *y as i64);
                        st.drag_y = Some(*y);
                        ui.wake(now());
                    }
                }
                berry_ui::desc::Msg::DragEnd { .. } => {
                    st.drag_y = None;
                }
            }
        }

        // —— 组件事件（Up 按钮 + 面包屑/过滤 chip——engine widget_react
        //    派发面；chip 选中翻转由重建面矫正）——
        let evs = ui.take_events();
        for ev in evs.iter() {
            if ev.event == EV_UP && page.up_btn == Some(ev.node) {
                nav_up(ui, page, st);
            } else if (EV_CRUMB..EV_CRUMB + 12).contains(&ev.event) {
                nav_crumb(ui, page, st, (ev.event - EV_CRUMB) as usize);
            } else if (EV_FILTER..EV_FILTER + 1 + FILTER_MAX as u32).contains(&ev.event) {
                set_filter(ui, page, st, ev.event - EV_FILTER);
            }
        }

        // —— 详情瞬态行节拍（到期自撤——Dialog 降解面的驻留语义）——
        if st.detail.active() && t_now >= st.detail.until {
            st.detail.clear();
            if let Err(e) = ui.set_text(page.detail, b"") {
                fail(&format!("detail clear err={e:?}"));
            }
            ui.wake(now());
        }

        // —— 渲染（damage 空集零提交——跳过帧零 RPC；settings 同形）——
        let mut enc = CommandEncoder::new();
        let out = ui.render_frame(t_now, scratch, &mut enc);
        last_expected = out.expected_present_ns;
        if out.drew {
            let dmg = out.damage;
            let full = dmg.full || dmg.count == 0;
            let pass = if full {
                PassDescriptor::clear(clear.0, clear.1, clear.2)
            } else {
                PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(damage_envelope(&dmg)))
            };
            let dl = enc.finish();
            if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
                fail(&format!("raster submit {e:?}"));
            }

            // SUBMIT（fresh READ dup 别名随行——settings 同形）。
            let mut alias: Handle = 0;
            if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0
                || alias == 0
            {
                fail("dup alias");
            }
            let (flags, rects, k) = if full {
                (
                    gp::SUBMIT_F_DAMAGE_FULL,
                    [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
                    0usize,
                )
            } else {
                (0u8, dmg.rects, dmg.count as usize)
            };
            let sreq = gp::PresentReq::Submit {
                surface_id: 0,
                buffer_id: buf_id,
                flags,
                rate_hint: out.rate_hint_hz,
                target_present_ns: out.expected_present_ns,
                rects,
                rect_count: k,
            };
            let m = match gp::encode_present_req(&mut req, &sreq) {
                Some(m) => m,
                None => fail("submit encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
            let token = match gp::parse_submit_ack(&rep[..n]) {
                Some(t) => t,
                None => fail(&format!("submit ack bad st={}", rep[0])),
            };
            ui.note_submitted();

            // FRAME_CALLBACK：断 PRESENTED（settings 同形）。
            let m = match gp::encode_present_req(
                &mut req,
                &gp::PresentReq::FrameCallback { frame_token: token },
            ) {
                Some(m) => m,
                None => fail("frame callback encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            let done = match gp::parse_frame_done_rep(&rep[..n]) {
                Some(d) => d,
                None => fail(&format!("frame done bad st={}", rep[0])),
            };
            if done.status != gp::FRAME_PRESENTED {
                fail(&format!("frame status={}", done.status));
            }
            ui.note_presented(
                (ui.telemetry().frames() - 1) as u32,
                now(),
                done.status,
                done.credits,
            );

            // RELEASE（归还别名——在途 ≤1 结构性背压；settings 同形）。
            let m = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
                Some(m) => m,
                None => fail("release encode"),
            };
            let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            let rel = match gp::parse_release_rep(&rep[..n]) {
                Some(r) => r,
                None => fail(&format!("release rep bad st={}", rep[0])),
            };
            if rel.count < 1 || nh != rel.count {
                fail(&format!("release count={} nh={}", rel.count, nh));
            }
            for h in &rhs[..nh] {
                close_h(*h);
            }

            presented += 1;
            buf_id = buf_id.wrapping_add(1);
            if buf_id == 0 {
                buf_id = 1;
            }
            if presented == 1 {
                println!("files: first frame");
            }
        }

        // —— vote（票面变化才发——settings 同形）——
        let ballot = ui.vote_hint();
        if last_vote != Some(ballot) {
            let vreq = gp::PresentReq::Vote {
                sticky: ballot.sticky,
                scene: ballot.scene,
                min_hz: ballot.min_hz,
                max_hz: ballot.max_hz,
                preferred_hz: ballot.preferred_hz,
            };
            let m = match gp::encode_present_req(&mut req, &vreq) {
                Some(m) => m,
                None => fail("vote encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            match gp::parse_status_rep(&rep[..n]) {
                Some(gp::ST_OK) => last_vote = Some(ballot),
                Some(st) => fail(&format!("vote st={st}")),
                None => fail(&format!("vote rep bad len={n}")),
            }
        }

        // —— 稳态等待（settings 同形 + 详情节拍 deadline 并入）——
        let cap = now() + IDLE_CAP_NS;
        let mut deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
        if st.detail.active() {
            deadline = deadline.min(st.detail.until);
        }
        let mut hs = [hw, 0 as Handle];
        let ss = [SIGNAL_READABLE, SIGNAL_READABLE];
        let mut n = 1usize;
        if hi != 0 && input_pending {
            hs[1] = hi;
            n = 2;
        }
        let mut idx = 0usize;
        let mut obs = 0u64;
        let wst = berry_object_wait_many(hs.as_ptr(), ss.as_ptr(), n, deadline, &mut idx, &mut obs);
        if wst != 0 && wst != BERRY_ERR_TIMEDOUT {
            fail(&format!("wait err={wst}"));
        }
    }
}

/// 击点在列表视口内（拖滚受理域）。
fn in_list(ui: &UiMain, page: &Page, x: u32, y: u32) -> bool {
    match ui.bounds(page.scroll) {
        Ok(b) => {
            x >= b.x as u32
                && y >= b.y as u32
                && x < (b.x + b.w) as u32
                && y < (b.y + b.h) as u32
        }
        Err(_) => false,
    }
}

/// 拖滚步进（跟手 dy = 上一样本 y - 本样本 y；上滑正值 → 内容上移）。
fn scroll_by(ui: &mut UiMain, page: &Page, st: &mut St, dy: i64) {
    let view_h = page.m.list_h as i64;
    let max = (st.content_h as i64 - view_h).max(0) as u32;
    let next = (st.offset as i64 + dy).clamp(0, max as i64) as u32;
    if next == st.offset {
        return;
    }
    st.offset = next;
    if let Err(e) = ui.set_props(
        page.scroll,
        Props::Scroll(ScrollProps { content_height: st.content_h, offset_y: next }),
    ) {
        eprintln!("files: scroll set err={e:?}");
    }
}

/// 行点击（双根页两行 = 根切换；目录行 = 进层；文件行 = 只读详情瞬
/// 态行——Dialog 的 v1 应用内降解）。
fn on_row(ui: &mut UiMain, page: &mut Page, st: &mut St, row: usize) {
    if row >= page.rows_n {
        return;
    }
    if st.home {
        st.root = if row == 0 { Root::Apps } else { Root::Data };
        st.home = false;
        st.depth = 0;
        st.filter = None;
        load_dir(ui, page, st);
        return;
    }
    // 目录浏览行 → 过滤序 ≠ 条目序：按可见序取条目。
    let Some(ent) = visible_ent(st, row) else { return };
    if ent.is_dir {
        if st.push_seg(ent.name()) {
            st.filter = None;
            load_dir(ui, page, st);
        }
    } else {
        show_detail(ui, page, st, ent);
    }
}

/// 可见序取条目（过滤命中 × 行帽——与 refresh 建行同序单源）。
fn visible_ent(st: &St, row: usize) -> Option<Ent> {
    let mut seen = 0usize;
    for i in 0..st.dir.n {
        if !st.dir.matches(i, st.filter) {
            continue;
        }
        if seen == row {
            return Some(st.dir.ents[i]);
        }
        seen += 1;
        if seen >= ROWS_MAX {
            break;
        }
    }
    None
}

/// 目录装载（READDIR+STAT 真读 + data 根 STATFS 惰性一次 + 重建）。
fn load_dir(ui: &mut UiMain, page: &mut Page, st: &mut St) {
    let mut wire = [0u8; RPC_BUF];
    let wl = st.wire(&mut wire);
    let noent = if st.root == Root::Apps { MSG_NO_APPS } else { MSG_EMPTY };
    read_dir(fs_h(), &wire[..wl], noent, &mut st.dir);
    if st.root == Root::Data && st.depth == 0 && st.free_kib.is_none() {
        // STATFS 真值（卷级——一次取值缓存；失败非致命略过）。
        if let Ok(s) = fs_statfs(fs_h()) {
            let kib = s.free_clusters as u64 * s.spc as u64 * s.sector_sz as u64 / 1024;
            st.free_kib = Some(kib.min(u32::MAX as u64) as u32);
        }
    }
    refresh(ui, page, st);
}

/// Up 一级（data/apps 根 → 双根页）。
fn nav_up(ui: &mut UiMain, page: &mut Page, st: &mut St) {
    if st.home {
        return;
    }
    if st.depth == 0 {
        st.home = true;
        st.filter = None;
        refresh(ui, page, st);
    } else {
        st.depth -= 1;
        st.filter = None;
        load_dir(ui, page, st);
    }
}

/// 面包屑跳级（层级 i：0 = 双根页，1 = 根目录，i≥2 = 段 i-2）。恒
/// refresh：engine 对 chip 的 selected 自动翻转（widget_react）须由重建
/// 面矫正——含无状态变化的空击（否则选中态视觉失同步）。
fn nav_crumb(ui: &mut UiMain, page: &mut Page, st: &mut St, level: usize) {
    if level == 0 {
        if !st.home {
            st.home = true;
            st.filter = None;
        }
        refresh(ui, page, st);
        return;
    }
    if st.home {
        return; // 非法层级（无 chip 可发）——防御
    }
    let target = (level - 1).min(st.depth);
    if target < st.depth {
        st.depth = target;
        st.filter = None;
        load_dir(ui, page, st);
    } else if st.filter.is_some() {
        st.filter = None;
        refresh(ui, page, st);
    } else {
        refresh(ui, page, st);
    }
}

/// 过滤词表点选（slot 0 = all；1..=FILTER_MAX = 首字母）。恒 refresh 同
/// `nav_crumb`（chip 自动翻转矫正面）。
fn set_filter(ui: &mut UiMain, page: &mut Page, st: &mut St, slot: u32) {
    if st.home {
        return;
    }
    let next = if slot == 0 {
        None
    } else {
        let (letters, k) = st.dir.letters();
        if (slot as usize) <= k {
            Some(letters[slot as usize - 1])
        } else {
            refresh(ui, page, st);
            return;
        }
    };
    st.filter = next;
    refresh(ui, page, st);
}

/// 文件只读详情（瞬态行：`NAME - file - N B`；2.5s 自撤）。
fn show_detail(ui: &mut UiMain, page: &mut Page, st: &mut St, e: Ent) {
    let mut b = [0u8; 64];
    let mut n = (e.len).min(NAME_DISP_MAX);
    b[..n].copy_from_slice(&e.name[..n]);
    let tag: &[u8] = b" - file - ";
    b[n..n + tag.len()].copy_from_slice(tag);
    n += tag.len();
    let mut db = [0u8; 12];
    let dn = dec_bytes(e.size, &mut db).min(64 - n);
    b[n..n + dn].copy_from_slice(&db[..dn]);
    n += dn;
    if n + 2 <= 64 {
        b[n] = b' ';
        b[n + 1] = b'B';
        n += 2;
    }
    st.detail.text = [0; 64];
    st.detail.text[..n].copy_from_slice(&b[..n]);
    st.detail.len = n;
    st.detail.until = now() + DETAIL_HOLD_NS;
    if let Err(err) = ui.set_text(page.detail, &st.detail.text[..n]) {
        fail(&format!("detail set err={err:?}"));
    }
    ui.wake(now());
}

/// K 矩形并集包络（增量 pass 的 dirty 域；settings 同形）。
fn damage_envelope(dmg: &berry_ui::damage::DamageSet) -> GfxRect {
    let mut x0 = u32::MAX;
    let mut y0 = u32::MAX;
    let mut x1 = 0u32;
    let mut y1 = 0u32;
    for r in &dmg.rects[..dmg.count as usize] {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x.saturating_add(r.w));
        y1 = y1.max(r.y.saturating_add(r.h));
    }
    let (ew, eh) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    GfxRect::new(
        i32::try_from(x0).unwrap_or(0),
        i32::try_from(y0).unwrap_or(0),
        i32::try_from(ew).unwrap_or(0).max(0),
        i32::try_from(eh).unwrap_or(0).max(0),
    )
}

/// 挂一轮 INPUT_POLL（settings `input_arm` 同形）。
fn input_arm(h: Handle) -> bool {
    let mut req = [0u8; 4];
    let n = gp::encode_input_poll_req(&mut req, gp::INPUT_EVENTS_MAX as u8);
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, req.as_ptr(), n, core::ptr::null(), 0);
        if s == 0 {
            return true;
        }
        if s != BERRY_ERR_BUSY {
            eprintln!("files: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("files: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}

// —— 纯函数单测（host 可跑：零 syscall 面） ——
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec_bytes_domains() {
        let mut b = [0u8; 12];
        assert_eq!(dec_bytes(0, &mut b), 1);
        assert_eq!(&b[..1], b"0");
        assert_eq!(dec_bytes(42, &mut b), 2);
        assert_eq!(&b[..2], b"42");
        assert_eq!(dec_bytes(3184, &mut b), 4);
        assert_eq!(&b[..4], b"3184");
    }

    #[test]
    fn wire_paths() {
        let mut st = St::new();
        st.home = false;
        st.root = Root::Apps;
        let mut out = [0u8; RPC_BUF];
        let n0 = st.wire(&mut out);
        assert_eq!(&out[..n0], b"apps");
        st.push_seg(b"sub");
        st.push_seg(b"deep");
        let n = st.wire(&mut out);
        assert_eq!(&out[..n], b"apps\0sub\0deep");
        st.root = Root::Data;
        st.depth = 0;
        assert_eq!(st.wire(&mut out), 0); // 空 wire = 数据盘根
        st.push_seg(b"dir1");
        let n = st.wire(&mut out);
        assert_eq!(&out[..n], b"dir1");
        // 子径拼段
        let mut c = [0u8; RPC_BUF];
        let cl = wire_child(b"dir1", b"F.TXT", &mut c);
        assert_eq!(&c[..cl], b"dir1\0F.TXT");
        let cl2 = wire_child(b"", b"F.TXT", &mut c);
        assert_eq!(&c[..cl2], b"F.TXT");
    }

    #[test]
    fn dir_sort_dirs_first() {
        let mut d = Dir::new();
        d.insert(b"b.txt", false, 1);
        d.insert(b"Zdir", true, 0);
        d.insert(b"adir", true, 0);
        d.insert(b"a.txt", false, 2);
        let names: Vec<&[u8]> = (0..d.n).map(|i| d.ents[i].name()).collect();
        assert_eq!(names, vec![b"Zdir".as_slice(), b"adir".as_slice(), b"a.txt".as_slice(), b"b.txt".as_slice()]);
        assert_eq!(d.total, 4);
    }

    #[test]
    fn filter_letters_and_match() {
        let mut d = Dir::new();
        d.insert(b"LAYOUT.JSN", false, 109);
        d.insert(b"gui8.txt", false, 4);
        d.insert(b"S", true, 0);
        let (letters, k) = d.letters();
        assert_eq!(&letters[..k], b"GLS");
        assert!(d.matches(0, Some(b'L')));
        assert!(d.matches(1, Some(b'G')));
        assert!(!d.matches(1, Some(b'L')));
        assert!(d.matches(2, None));
    }
}
