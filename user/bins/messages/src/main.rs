//! messages —— 波 4 第六件真 GUI 应用（MMS appstub 秒退壳的翻新；
//! .agents/plans/2026-11-15-波4预制应用翻新实施设计.md §2.6、worktree 根
//! W4-RECON.md §四 MMS 行）。
//!
//! **使命**（v1 四件）：
//! - 会话列表页：AppBar(title=Messages) + Scroll[List]（
//!   `pattern::list_item` 行 = 首字母头像位 + 名 + 末条摘要 + Draft
//!   角标）——条目按压 push 会话页；
//! - 会话页：气泡列（**Card 变体**——`pattern::card` 行卡：对方左 /
//!   Me 右分侧 + Me 侧 Sent→Delivered 状态次行）+ 输入区；
//! - v1 输入 = 固定快捷短语 `pattern::chip` 组选择（真 typing 敲键挂
//!   softimed，3b）+ Input kind 草稿值面（`set_input_text` 值面可显
//!   不可敲——点按置焦环即显，无键盘无输入，诚实边界 hint 瞬态行）；
//! - Send → 气泡入列（Me 侧）+ 瞬态行反馈（Sending→Sent→Delivered
//!   帧循环节拍驱动）+ 对端本地节拍模拟回复（无 modem 仅 UI 反馈，
//!   诚实标注 Me/对方侧别）。
//!
//! **零手拼铁律**（W4-RECON §二/§五）：条目/气泡一律 `pattern::card`
//! （Card kind 自绘卡面/阴影，8vp 圆角单源；`list_item` = Card(0,None)
//! 薄别名）；骨架一律 pattern 配方（app_bar/stack/spacer/divider）；
//! 按压反馈单源 = `pattern::ItemState` 五态预混表（主树等价面——RECON
//! §二末段裁定「用 ItemState 面」；行按压 = app 侧按 press_react 同形
//! set_props 换底，色值零字面量）+ Button/Chip kind 引擎面（Button 走
//! press_map/PressedVis 查表；Chip selected 色变 = kind 自绘）；文案全
//! ASCII（HD 档 95 字模域）；本 crate 禁手写 Rect 拼卡/press 遮罩字面
//! 量。页底/面层 Rect 仅用于 pattern 叠层语（app_bar 面层同款——非卡片
//! 拼装）。
//!
//! **链形态** = settings/contacts 镜像（9035af061 先例；berry-launcher
//! 同源食谱）：wm_cli_h() 门控（0 = GUI 通道未授权 → 单行诊断 + park
//! 常驻，不秒退）→ WM_GETINFO 几何 → present GETINFO 轻断言 → 帧缓冲
//! VMO 三连（create/commit/map @ FRAME_VA）→ 三态停帧 + SUBMIT →
//! FRAME_CALLBACK → RELEASE 严格内联串行 + vote 票面变化才发。输入 =
//! input_h() INPUT_POLL 批拉 → push_pointer → pump_input（手势 referee
//! + widget_react 同拍）→ Msg/组件事件双面消费。会话页 NavHost 值面
//! 页序自管（contacts 同形：返回 = app 内 AppBar 返回 glyph 手势位，
//! 无系统 back 链——3b 依赖）。
//!
//! **v1 降解面**（3b 未落——完整清单见 W4-IMPL-MMS.md）：
//! - 无 berry-appshell（rpc/pump/scene/icons/conf 五模块）——RPC 半程
//!   手拼 + 自管帧循环照 settings v0 形；
//! - 无 softimed——输入 = 快捷短语 chips 点选 + Input 草稿值面（不可
//!   敲）；无 ime_wanted 焦点环接线（点按置焦 = 引擎 Input 焦点环近似）；
//! - 无 appshell::conf/MMS.JSN——会话/消息/草稿 v1 内存态（种子内置，
//!   退出即弃；持久化挂终态）；
//! - 无 wm back 路由——会话返回走 app 内 glyph（系统 back 链挂 3b）；
//! - 退出形态照先例：无退出路径（fail = eprintln + park；崩溃监督归
//!   svcmgr app_supervise 域）。
//!
//! **输出面**：manifest console=false → stdout 走 svcmgr 私有对；首帧
//! 呈现回执一行 `mms: first frame`（诊断面，非串口标记）。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::io::{input_h, self_vmar_h, wm_cli_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_many, berry_object_wait_one, berry_vmar_map,
    berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_ui::desc::{
    ButtonProps, ButtonStyle, ChipProps, ColProps, InputKind, InputProps, Props, RectProps,
    RowProps, ScrollProps, TextBuf, TextProps, TextStyle, ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::pattern::{self, ItemState, NavHost};
use berry_ui::theme::{self, ThemeMode, Tokens};
use berry_ui::{
    IconSource, Msg, NodeId, ProportionalTextMetrics, UiError, UiMain, ROLE_BUTTON,
    ROLE_LIST_ITEM,
};

// 链接种子（fsmgr/guifbd/berry-launcher/settings/contacts 同款）：rlib/
// LTO 按需保留——无引用则 berry-libc 的 crt0（含 `_start`）不进链接；
// `#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（launcher/settings/contacts 同值 300s：app 拉起
/// 时点 = 用户点击，首 RPC 距 boot 稳态可远）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 帧缓冲映射 VA（launcher frameloop 同位：mmap VA 窗内固定位 +16MiB）。
const FRAME_VA: usize = 0x5100_0000;

/// 默认 vsync 周期（60Hz；present GETINFO 无周期域——S2 食谱默认）。
const VSYNC_NS: u64 = 16_666_666;

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// glyph scratch 容量（堆置——paint 对超容 run 截断尾字形，视觉面非
/// 正确性面；scratch 为**全帧共享游标**（paint.rs 互斥子片切分），会话
/// 页全帧最坏 = 16 气泡 ×26 字 + 16 状态行 ×9 字 + 输入区/铬层 ~80 +
/// nav 过渡期下列表页 ~90 ≈ 750——1024 富余；settings/contacts 256 档
/// 对本页气泡列不够）。
const SCRATCH_GLYPHS: usize = 1024;

/// 整帧清屏底色 = 页面底 token 单源（light 档 BG_PAGE；卡片白面/文本
/// 全部叠画其上——theme.rs 唯一归属，本文件零清屏色值字面量）。
const CLEAR_BG: Color = theme::BG_PAGE;

/// 强调底上的前景字色（pattern.rs 私有 const `ON_ACCENT` 按值镜像并
/// 注明单源——contacts 同款镜像先例；theme OnPrimary 档收编后随迁）。
const ON_ACCENT: Color = Color { r: 255, g: 255, b: 255 };

// —— 节拍（发送瞬态行反馈 + 对端模拟回复 + hint 自撤；帧循环 deadline
//    三源取早驱动）——
/// 发送 → Sent（1.2s 节拍）。
const SENDING_NS: u64 = 1_200_000_000;
/// Sent → Delivered（再 1.8s——累计 3s 走完瞬态反馈）。
const SENT_NS: u64 = 1_800_000_000;
/// 对端本地模拟回复节拍（设计稿 §2.6「对端回复 v1 = 本地节拍模拟」）。
const REPLY_NS: u64 = 4_500_000_000;
/// 输入区 hint 瞬态行驻留（contacts Calling 瞬态行同拍 2s）。
const HINT_NS: u64 = 2 * 1_000_000_000;

// —— 几何（px @3x；pattern 公开常量直引，私有常量按值镜像注明单源）——
/// 气泡行间缝（会话内容 Col gap）。
const ROW_GAP: u32 = 12;
/// 气泡行步进高（Card kind 行高 + 缝；contacts 行高近似口径同款）。
const ROW_STRIDE: u32 = pattern::LISTITEM_H + ROW_GAP;
/// 气泡左右页边留白（对方左贴 / Me 右贴的呼吸位）。
const GUTTER: u32 = 24;
/// 气泡最小宽（卡内衬 96 + 8 字位起步——「OK」短档不塌细条）。
const MIN_BUBBLE_W: u32 = pattern::LISTITEM_PAD_X * 2 + 8 * 24;
/// 气泡正文档字格宽（S24x48 = 24px/字——bubble_w 估值单源）。
const MSG_CW: u32 = 24;
/// 状态行档字格宽（S16x32 = 16px/字）。
const ST_CW: u32 = 16;
/// 状态行最长文案「Delivered」9 字。
const ST_CHARS: u32 = 9;
/// 滚动尾留白（Scroll 内容尾部呼吸位）。
const TAIL_PAD: u32 = 24;
/// 会话页内容顶让位（AppBar 高 + 8 呼吸）。
const TOP_PAD: u32 = pattern::TOPBAR_H + 8;
/// 列表页顶让位（AppBar 高）。
const LIST_TOP_PAD: u32 = pattern::TOPBAR_H;

// —— 输入区几何（berry-ui lib.rs「组件 v1 几何常量」为 pub(crate) 非
//    契约面——CHIP_H=32 / INPUT_H=48 / BUTTON_H=48 按值镜像并注明单源；
//    theme 收编后随迁）——
/// Chip 内在高。
const CHIP_H: u32 = 32;
/// Input 内在高（= 输入行高；Button 同值）。
const INPUT_ROW_H: u32 = 48;
/// hint 瞬态行高（S16x32 行格）。
const HINT_H: u32 = 32;
/// 输入区纵向呼吸位。
const PAD_V: u32 = 12;
/// 输入区总高（divider 1 + pad 12 + hint 32 + pad 12 + chips 32 +
/// pad 12 + 输入行 48 + pad 12 = 161——滚动层尾让位/拖拽门共用真值）。
const INPUT_AREA_H: u32 = 1 + PAD_V + HINT_H + PAD_V + CHIP_H + PAD_V + INPUT_ROW_H + PAD_V;
/// chips 行缝。
const CHIP_GAP: u32 = 16;
/// 输入行缝（Input → 推位 → Send）。
const DRAFT_GAP: u32 = 16;

// —— 组件事件 ID 面（props on_tap 携载——take_events 消费锚）——
/// Send 按钮。
const EV_SEND: u32 = 1;
/// 快捷短语 chip 基（+i = 短语索引；Button/无碰撞域）。
const EV_PHRASE0: u32 = 0x10;

// —— 消息状态机（瞬态行反馈三档）——
/// 对端气泡（无状态行）。
const ST_NONE: u8 = 0;
/// 刚发出（行反馈「Sending」）。
const ST_SENDING: u8 = 1;
/// 节拍一（行反馈「Sent」）。
const ST_SENT: u8 = 2;
/// 终态（行反馈「Delivered」）。
const ST_DELIVERED: u8 = 3;

/// 单会话消息容量（定容满 → 丢最旧——气泡列下界保证）。
const MSGS_MAX: usize = 16;
/// 单条消息文本帽（TextBuf 内联 64B 域内；byte 数组取 32 紧凑档）。
const MSG_TEXT_MAX: usize = 32;

// ---------------------------------------------------------------------------
// —— 静态种子数据（v1 内置——MMS.JSN 持久化挂 appshell::conf 终态）——

/// 单条种子消息（文案全 ASCII——HD 档 95 字模域硬约束；≤26B）。
struct SeedMsg {
    me: bool,
    text: &'static [u8],
}

/// 种子会话（设计稿 §2.6「种子会话内置」——3 条，末条摘要/Draft 角标
/// 有可判差异）。
struct SeedConv {
    name: &'static [u8],
    seed: &'static [SeedMsg],
}

const CONVS: [SeedConv; 3] = [
    SeedConv {
        name: b"Alice",
        seed: &[
            SeedMsg { me: false, text: b"Are we still on for lunch?" },
            SeedMsg { me: true, text: b"Yes, see you at noon" },
            SeedMsg { me: false, text: b"Great, see you then" },
        ],
    },
    SeedConv {
        name: b"Bob",
        seed: &[
            SeedMsg { me: true, text: b"Did you get the report?" },
            SeedMsg { me: false, text: b"Yes, reviewing it now" },
            SeedMsg { me: false, text: b"Looks good so far" },
        ],
    },
    SeedConv {
        name: b"Carol",
        seed: &[SeedMsg { me: false, text: b"Call me when you are free" }],
    },
];

/// 固定快捷短语组（v1 输入源——真 typing 敲键挂 softimed；标签 ≤11B
/// 域 Chip 内联）。
const PHRASES: [&[u8]; 7] =
    [b"Hello", b"Thanks", b"On my way", b"Sounds good", b"Yes", b"No", b"See you"];

/// 对端模拟回复轮换表（本地节拍模拟——无 modem 诚实边界）。
const REPLIES: [&[u8]; 4] = [b"Got it, thanks", b"Sounds good to me", b"OK, talk later", b"Sure thing"];

// ---------------------------------------------------------------------------
// —— 运行期数据（内存态——v1 无持久化）——

/// 单条消息运行态（定容字节槽——零 alloc 纪律）。
#[derive(Clone, Copy)]
struct MsgRec {
    me: bool,
    len: u8,
    bytes: [u8; MSG_TEXT_MAX],
    /// `ST_*`（对端恒 `ST_NONE`）。
    status: u8,
}

impl MsgRec {
    fn text(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// 空消息槽。
const fn blank_msg() -> MsgRec {
    MsgRec { me: false, len: 0, bytes: [0; MSG_TEXT_MAX], status: ST_NONE }
}

/// 单会话运行态。
#[derive(Clone, Copy)]
struct ConvState {
    msgs: [MsgRec; MSGS_MAX],
    n: usize,
    /// 草稿（快捷短语索引——退出会话暂存，列表页 Draft 角标回显）。
    draft: Option<u8>,
    /// 模拟回复轮换计数。
    replies: u8,
}

/// 状态档 → 行反馈文案（None = 无状态行——对端气泡）。
fn status_bytes(st: u8) -> Option<&'static [u8]> {
    match st {
        ST_SENDING => Some(b"Sending"),
        ST_SENT => Some(b"Sent"),
        ST_DELIVERED => Some(b"Delivered"),
        _ => None,
    }
}

/// chip 事件 ID → 短语索引（域外 None——事件面单解）。
fn phrase_index(ev: u32) -> Option<usize> {
    ev.checked_sub(EV_PHRASE0)
        .filter(|i| (*i as usize) < PHRASES.len())
        .map(|i| i as usize)
}

/// 气泡宽估值（正文 n 字 ×24 + 卡内衬 96；Me 侧至少容「Delivered」
/// 状态行；[MIN_BUBBLE_W, vp_w*3/4] 取界——max/min 形防窄窗 clamp 倒
/// 挂 panic（零 panic 纪律）；比例 advance 恒 ≤ 字格宽，估值只影响
/// 胶囊宽不进命中/卡面语义）。
fn bubble_w(len: usize, vp_w: u32, me: bool) -> u32 {
    let text = (len.max(1) as u32).saturating_mul(MSG_CW);
    let inner = if me { text.max(ST_CHARS * ST_CW) } else { text }
        .saturating_add(pattern::LISTITEM_PAD_X * 2);
    let cap = vp_w / 4 * 3;
    inner.max(MIN_BUBBLE_W).min(cap.max(GUTTER * 2))
}

/// 列表行末条摘要（「Me: 」前缀诚实标注侧别；帽 24B = 前缀 4 + 文 20）。
fn summary_bytes(m: &MsgRec, out: &mut [u8; 24]) -> usize {
    const PRE: &[u8] = b"Me: ";
    let mut n = 0usize;
    if m.me {
        for &b in PRE {
            out[n] = b;
            n += 1;
        }
    }
    let take = (m.len as usize).min(out.len() - n);
    out[n..n + take].copy_from_slice(&m.bytes[..take]);
    n + take
}

// ---------------------------------------------------------------------------
// —— 应用内功能 glyph（程序化绘制——设计稿 §3.3「v1 = berry-ui 程序
//    绘制（波1 导航键自绘先例——ADR #63 备选路），不新增位图资产」；
//    contacts BACK glyph 同技法独立实现）——IconSource 真供件：Argb
//    1-bit 透明键（A==0 跳过——raster argb_blit 同律），色值 theme
//    token 单源打包。源 24px：按 APP_BAR_BACK=72px 三倍整数最近邻。

/// glyph 源边长（px）。
const GLYPH_N: usize = 24;

/// Color → RGB 打包（const 面——theme 常量进 const 求值）。
const fn pack_rgb(c: Color) -> u32 {
    ((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32
}

/// 点到线段垂距 ≤ r（整数：cross² ≤ r²·len² 且投影落在段内；线段端点
/// 退化按点距判定——glyph 光栅足够）。
const fn near_seg(px: i32, py: i32, sx: i32, sy: i32, ex: i32, ey: i32, r2: i32) -> bool {
    let dx = ex - sx;
    let dy = ey - sy;
    let len2 = dx * dx + dy * dy;
    if len2 == 0 {
        return (px - sx) * (px - sx) + (py - sy) * (py - sy) <= r2;
    }
    let t = (px - sx) * dx + (py - sy) * dy;
    if t < 0 || t > len2 {
        return false;
    }
    let cross = dx * (py - sy) - dy * (px - sx);
    cross * cross <= r2 * len2
}

/// 返回箭头「<」位图（两臂线段 r=3px——72px 目标下 9px 硬边臂宽）。
const fn build_back(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    let r2 = 9; // r=3px
    let mut px = [0u32; GLYPH_N * GLYPH_N];
    let mut i = 0usize;
    while i < GLYPH_N * GLYPH_N {
        let x = (i % GLYPH_N) as i32;
        let y = (i / GLYPH_N) as i32;
        // 上臂 (16,5)->(8,12) + 下臂 (8,12)->(16,19)。
        let on = near_seg(x, y, 16, 5, 8, 12, r2) || near_seg(x, y, 8, 12, 16, 19, r2);
        px[i] = if on { 0xFF00_0000 | rgb } else { 0 };
        i += 1;
    }
    px
}

/// glyph 表（const 求值落 rodata——零堆零运行期构造）。
static BACK_PX: [u32; GLYPH_N * GLYPH_N] = build_back(pack_rgb(theme::FONT_PRIMARY));

/// 本应用 IconSource 真供件（berry-ui D28 供给槽——app 侧只用注册域
/// 内 id，域外值空绘不炸）。
struct MmsIcons;

static ICONS: MmsIcons = MmsIcons;

impl IconSource for MmsIcons {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        match id {
            pattern::icons::BACK => Some((GLYPH_N as u32, GLYPH_N as u32)),
            _ => None,
        }
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        match id {
            pattern::icons::BACK => Some(&BACK_PX),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// —— RPC 半程族 + 帧循环小件（launcher/settings/contacts 同形拷贝）——

/// 诊断 + park 收口（settings `fail` 同形——app 常驻不被 reap；崩溃
/// 监督归 svcmgr app_supervise 域，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("mms: {what}");
    berry_libc::park()
}

/// now = clock_read；失败按 0 → deadline 即刻到期 = fail-fast。
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

/// 严格串行 RPC **发送半程**（launcher `rpc_send` 同形）：-EBUSY → 阻塞
/// 等 WRITABLE（帽重臂不自旋）；send 原子性——EBUSY/错码时随行 handle
/// 零迁移。**send 即 take**（§9 A6-b）。
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

/// 严格串行 RPC **收侧半程**（launcher `rpc_recv` 同形）：空队 -EAGAIN →
/// 阻塞等 READABLE（帽）；每轮重置容量（in-out 出参漏重置 = 死循环）。
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

/// 严格串行 RPC 单趟（一请求一应答）。
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

/// K 矩形并集包络（增量 pass 的 dirty 域；launcher/settings/contacts
/// 同形——engine damage 恒 viewport 裁剪，i32 折 try_from 饱和防御）。
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

/// 挂一轮 INPUT_POLL（launcher `input_arm` 同形）：inputd 阻塞批拉 ≤3
/// 事件；-EBUSY 等 WRITABLE 重试；其余错 = 输入面失效（调用方置 hi=0
/// 静态页续活不死）。
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
            eprintln!("mms: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("mms: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}

// ---------------------------------------------------------------------------
// —— 建树小件 ——

/// Text 叶 spec（bg = 头像/Draft 角标衬底；超 `TEXT_INLINE`=64B loud
/// Err——本页文案 ≤26B 域内）。
fn text_spec(
    fg: Color,
    bg: Option<Color>,
    scale: FontSize,
    bytes: &[u8],
) -> Result<ViewSpec, UiError> {
    Ok(ViewSpec {
        kind: ViewKind::Text,
        key: 0,
        props: Props::Text(TextProps {
            style: TextStyle { fg, bg, scale },
            text: TextBuf::new(bytes)?,
        }),
    })
}

/// 单节点 spec（key=0——调和键路径挂账 v1.5，pattern 同形）。
fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

/// ItemState 五态底色 → Color（预混表 u32 拆包——值单源 pattern.rs）。
fn state_color(v: u32) -> Color {
    Color { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 }
}

// ---------------------------------------------------------------------------
// —— 应用状态（建树一次落定 + 运行期翻转面）——

/// 单气泡行句柄（rows[i] ↔ convs[cur].msgs[i] 同序不变式）。
#[derive(Clone, Copy)]
struct Row {
    /// Card kind 行卡锚（remove/命中几何锚）。
    card: NodeId,
    /// Me 侧状态行 Text（对端 None）。
    status: Option<NodeId>,
}

/// 会话页运行态句柄。
struct Chat {
    /// AppBar 返回键（app 内手势位——pop 消费锚）。
    back: NodeId,
    /// 气泡列滚动容器。
    scroll: NodeId,
    /// 气泡挂载 Col。
    msg_col: NodeId,
    /// 气泡行表。
    rows: [Option<Row>; MSGS_MAX],
    /// 快捷短语 chip 表。
    chips: [NodeId; PHRASES.len()],
    /// 草稿值面（Input kind——`set_input_text` 可显不可敲）。
    input: NodeId,
    /// Send 按钮。
    send: NodeId,
    /// 输入区 hint 瞬态行（空文 = 隐藏）。
    hint: NodeId,
    /// 滚动内容总高。
    content_h: u32,
    /// 当前滚动偏移（clamp [0, content_h − vp_h]）。
    offset: u32,
    /// 在途状态行 Text 节点（None = 无在途）。
    pending_node: Option<NodeId>,
    /// 在途消息索引（rows/msg 同序）。
    pending_msg: usize,
    /// 在途状态档（0 = 无在途）。
    pending_stage: u8,
    /// 下一状态节拍期限。
    status_dl: Option<u64>,
    /// hint 自撤期限。
    hint_dl: Option<u64>,
    /// 气泡列拖拽纵坐标。
    drag_y: Option<i32>,
}

/// 应用运行态（列表页常驻 + 会话页 push/pop）。
struct App {
    /// 主题 token 组（light 档——全页配色单源）。
    t: Tokens,
    /// 视口宽（px）。
    vp_w: u32,
    /// 视口高（px）。
    vp_h: u32,
    /// 页栈状态机（NavHost——会话 push/pop 300ms 推拉）。
    nav: NavHost,
    /// NavHost 栈容器。
    nav_root: NodeId,
    /// 会话数据（内存态）。
    convs: [ConvState; CONVS.len()],
    // —— 列表页（常驻根页）——
    /// 列表滚动容器。
    scroll: NodeId,
    /// 行挂载 Col。
    content_col: NodeId,
    /// 行卡表。
    row_cards: [NodeId; CONVS.len()],
    /// 行头像 Text（按压换底锚）。
    row_avatar: [NodeId; CONVS.len()],
    /// 滚动内容总高。
    content_h: u32,
    /// 当前滚动偏移。
    offset: u32,
    /// 按压中行索引（Press 置 / Release·Cancel·DragStart 还原）。
    pressed_row: Option<usize>,
    /// 列表拖拽纵坐标。
    drag_y: Option<i32>,
    // —— 会话页 ——
    /// 当前打开会话（None = 列表页）。
    cur: Option<usize>,
    /// 会话页句柄（随 push 建 / pop 出栈完结撤）。
    chat: Option<Chat>,
    /// 对端模拟回复节拍（(会话索引, 期限)——跨 pop 存活，数据面入列）。
    reply: Option<(usize, u64)>,
}

/// 会话页内容总高（顶让位 + n 行步进 + 尾让位）。
fn chat_content_h(n: usize) -> u32 {
    TOP_PAD + n as u32 * ROW_STRIDE + INPUT_AREA_H + TAIL_PAD
}

/// 列表页内容总高。
fn list_content_h() -> u32 {
    LIST_TOP_PAD + CONVS.len() as u32 * ROW_STRIDE + TAIL_PAD
}

/// 几何命中：坐标 → 会话行索引（`ui.bounds` 行卡界包含判定——Card
/// 子树全幅，行内任意节点命中都落本界；pattern 内部节点 id 不外露的
/// 补偿面；Click 无坐标面故走 Release 判定）。
fn row_at(ui: &mut UiMain, a: &App, x: u32, y: u32) -> Option<usize> {
    for (i, card) in a.row_cards.iter().enumerate() {
        if let Ok(b) = ui.bounds(*card) {
            let (x, y) = (x as i64, y as i64);
            if x >= b.x as i64
                && y >= b.y as i64
                && x < b.x as i64 + b.w as i64
                && y < b.y as i64 + b.h as i64
            {
                return Some(i);
            }
        }
    }
    None
}

/// 行按压换底（ItemState 五态预混表单源——引擎 press_react 同形
/// set_props 面：Pressed = 头像衬底换 brand 20% 预混白；还原 = brand。
/// TextBuf 按种子首字母重建（引擎无 props 读面——确定性数据可重建）。
fn tint_row(ui: &mut UiMain, a: &App, row: usize, on: bool) {
    let initial = [CONVS[row].name[0]];
    let text = match TextBuf::new(&initial) {
        Ok(tb) => tb,
        Err(e) => {
            eprintln!("mms: avatar rebuild err={e:?}");
            return;
        }
    };
    let bg = if on { state_color(ItemState::Pressed.bg()) } else { a.t.brand };
    let props = Props::Text(TextProps {
        style: TextStyle { fg: ON_ACCENT, bg: Some(bg), scale: FontSize::S32x48 },
        text,
    });
    if let Err(e) = ui.set_props(a.row_avatar[row], props) {
        eprintln!("mms: press tint err={e:?}");
    }
}

/// 滚动偏移累进（clamp 非负 + 不越尾——overscroll 无橡皮筋，v1 直钳；
/// 返回新偏移（未变 = 原值）。
fn scroll_step(ui: &mut UiMain, node: NodeId, content_h: u32, offset: u32, vp_h: u32, dy: i32) -> u32 {
    let max_off = content_h.saturating_sub(vp_h);
    let next = (offset as i64 + dy as i64).clamp(0, max_off as i64) as u32;
    if next != offset {
        let props = Props::Scroll(ScrollProps { content_height: content_h, offset_y: next });
        if let Err(e) = ui.set_props(node, props) {
            eprintln!("mms: scroll set err={e:?}");
            return offset;
        }
    }
    next
}

/// 重建列表行集（初建/会话返回两消费点——旧卡整子树 remove 后重挂；
/// Draft 角标随 convs[].draft 回显；Scroll content_height/偏移钳随行
/// 更新，偏移保留（钳新域）。
fn build_rows(ui: &mut UiMain, a: &mut App) -> Result<(), UiError> {
    for card in a.row_cards {
        ui.remove(card)?;
    }
    a.pressed_row = None;

    let t = a.t;
    for (i, sc) in CONVS.iter().enumerate() {
        let conv = &a.convs[i];
        // — 行卡（pattern::list_item = Card(0, None) 薄别名——48vp 行、
        //   卡面/8vp 圆角/阴影 Card kind 自绘，零手拼）—
        let card = pattern::list_item(ui, a.content_col, &t)?;
        // 行内容：Row[首字母头像, 名/摘要列, 推位, Draft 角标]。
        let line = ui.add(
            card.content,
            vspec(
                ViewKind::Row,
                Props::Row(RowProps {
                    gap: 24,
                    w: None,
                    h: Some(pattern::LISTITEM_H),
                    cross: CrossAlign::Center,
                }),
            ),
        )?;
        // 首字母头像位（brand 衬底 + 首字母——「圆标」的 v1 方形降解，
        // contacts 同款）。
        let initial = [sc.name[0]];
        let avatar =
            ui.add(line, text_spec(ON_ACCENT, Some(t.brand), FontSize::S32x48, &initial)?)?;
        // 名主行 + 末条摘要次行。
        let labels = ui.add(
            line,
            vspec(
                ViewKind::Col,
                Props::Col(ColProps { gap: 4, w: None, h: None, cross: CrossAlign::Start }),
            ),
        )?;
        ui.add(labels, text_spec(t.font_primary, None, FontSize::S24x48, sc.name)?)?;
        let mut sbuf = [0u8; 24];
        let sn = summary_bytes(&conv.msgs[conv.n - 1], &mut sbuf);
        ui.add(labels, text_spec(t.font_secondary, None, FontSize::S16x32, &sbuf[..sn])?)?;
        // 推位（定宽——角标右对齐；溢出折 0 无害）。
        let flex = a
            .vp_w
            .saturating_sub(2 * pattern::LISTITEM_PAD_X)
            .saturating_sub(32)
            .saturating_sub(24)
            .saturating_sub(20 * ST_CW)
            .saturating_sub(80);
        pattern::spacer(ui, line, flex, 1)?;
        // Draft 角标（草稿退出暂存回显——brand 衬底白字 mini 标；无草稿
        // 零占位）。
        if conv.draft.is_some() {
            ui.add(line, text_spec(ON_ACCENT, Some(t.brand), FontSize::S16x32, b"Draft")?)?;
        }

        let _ = ui.set_a11y(card.node, ROLE_LIST_ITEM, sc.name);
        a.row_cards[i] = card.node;
        a.row_avatar[i] = avatar;
    }

    a.content_h = list_content_h();
    a.offset = a.offset.min(a.content_h.saturating_sub(a.vp_h));
    ui.set_props(
        a.scroll,
        Props::Scroll(ScrollProps { content_height: a.content_h, offset_y: a.offset }),
    )?;
    Ok(())
}

/// 单气泡行装配（append/初建共用）：Row[侧别推位, 定宽 Col{Card 变体}]
/// ——对方左贴（elevation 1）/ Me 右贴（elevation 2 + brand 正文 +
/// 状态次行）；行卡宽 = bubble_w 估值（Card 宽随父 100% → 定宽 Col
/// 承载胶囊宽）。形参取值面（t/vp_w）而非 &App——append_msg 的借用
/// 拆分面。Err = 容量/文本域违例。
fn add_bubble(
    ui: &mut UiMain,
    t: Tokens,
    vp_w: u32,
    ch: &mut Chat,
    m: &MsgRec,
    idx: usize,
) -> Result<(), UiError> {
    let line = ui.add(
        ch.msg_col,
        vspec(
            ViewKind::Row,
            Props::Row(RowProps { gap: 0, w: None, h: None, cross: CrossAlign::Start }),
        ),
    )?;
    let bw = bubble_w(m.len as usize, vp_w, m.me);
    let flex = vp_w.saturating_sub(bw).saturating_sub(2 * GUTTER);
    if m.me {
        pattern::spacer(ui, line, flex, 1)?;
    } else {
        pattern::spacer(ui, line, GUTTER, 1)?;
    }
    // 定宽承载 Col（Card 宽随父 → 胶囊宽单源在此）。
    let holder = ui.add(
        line,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps { gap: 0, w: Some(bw), h: None, cross: CrossAlign::Start }),
        ),
    )?;
    // 气泡行卡（Card 变体——卡面/圆角/阴影 kind 自绘；Me 侧 elevation 2
    // 上浮一档与对方分侧可判）。
    let card = pattern::card(ui, holder, &t, if m.me { 2 } else { 1 }, None)?;
    let body = if m.me { t.brand } else { t.font_primary };
    ui.add(card.content, text_spec(body, None, FontSize::S24x48, m.text())?)?;
    let mut st_node = None;
    if m.me {
        // 状态次行（瞬态行反馈值面——Sending/Sent/Delivered 节拍换文）。
        let s = status_bytes(m.status).unwrap_or(b"Delivered");
        let st = ui.add(card.content, text_spec(t.font_secondary, None, FontSize::S16x32, s)?)?;
        st_node = Some(st);
    }
    let tail = if m.me { GUTTER } else { flex };
    pattern::spacer(ui, line, tail, 1)?;

    let _ = ui.set_a11y(card.node, ROLE_LIST_ITEM, m.text());
    ch.rows[idx] = Some(Row { card: card.node, status: st_node });
    Ok(())
}

/// 消息入列（数据面恒落；会话页打开且为当前会话 → 气泡节点同拍入列 +
/// 滚底）。返回消息索引。
fn append_msg(ui: &mut UiMain, a: &mut App, c: usize, me: bool, text: &[u8], status: u8) -> usize {
    if a.convs[c].n == MSGS_MAX {
        drop_oldest(ui, a, c);
    }
    let conv = &mut a.convs[c];
    let idx = conv.n;
    let take = text.len().min(MSG_TEXT_MAX);
    let m = &mut conv.msgs[idx];
    m.me = me;
    m.len = take as u8;
    m.bytes[..take].copy_from_slice(&text[..take]);
    m.status = if me { status } else { ST_NONE };
    conv.n += 1;

    if a.cur == Some(c) {
        // 借用拆分：值面先拷（Tokens/MsgRec 均 Copy），Chat 独占可变借。
        let t = a.t;
        let vp_w = a.vp_w;
        let vp_h = a.vp_h;
        let n = a.convs[c].n;
        let rec = a.convs[c].msgs[idx];
        if let Some(ch) = a.chat.as_mut() {
            if let Err(e) = add_bubble(ui, t, vp_w, ch, &rec, idx) {
                fail(&format!("bubble build err={e:?}"));
            }
            ch.content_h = chat_content_h(n);
            ch.offset = ch.content_h.saturating_sub(vp_h); // 新消息滚底
            let props = Props::Scroll(ScrollProps { content_height: ch.content_h, offset_y: ch.offset });
            if let Err(e) = ui.set_props(ch.scroll, props) {
                fail(&format!("chat scroll set err={e:?}"));
            }
        }
        ui.wake(now());
    }
    idx
}

/// 丢最旧（容量满——数据槽左移 + 气泡节点整子树 remove，rows/msgs
/// 同序不变式保持；在途索引随移）。
fn drop_oldest(ui: &mut UiMain, a: &mut App, c: usize) {
    {
        let conv = &mut a.convs[c];
        conv.msgs.copy_within(1..MSGS_MAX, 0);
        conv.msgs[MSGS_MAX - 1] = blank_msg();
        conv.n -= 1;
    }
    let n = a.convs[c].n;
    if a.cur == Some(c) {
        if let Some(ch) = a.chat.as_mut() {
            if let Some(r) = ch.rows[0].take() {
                if let Err(e) = ui.remove(r.card) {
                    fail(&format!("bubble remove err={e:?}"));
                }
            }
            ch.rows.copy_within(1..MSGS_MAX, 0);
            ch.rows[MSGS_MAX - 1] = None;
            if ch.pending_msg > 0 {
                ch.pending_msg -= 1;
            }
            ch.content_h = chat_content_h(n);
            ch.offset = ch.offset.min(ch.content_h.saturating_sub(a.vp_h));
            let props = Props::Scroll(ScrollProps { content_height: ch.content_h, offset_y: ch.offset });
            if let Err(e) = ui.set_props(ch.scroll, props) {
                fail(&format!("chat scroll set err={e:?}"));
            }
        }
    }
}

/// chip 选中态置换（引擎已翻转击中 chip——此处补前一枚还原；label
/// TextBuf 按词表重建）。
fn set_chip_sel(ui: &mut UiMain, a: &App, i: usize, sel: bool) {
    let node = match a.chat.as_ref() {
        Some(ch) => ch.chips[i],
        None => return,
    };
    let text = match TextBuf::new(PHRASES[i]) {
        Ok(tb) => tb,
        Err(e) => {
            eprintln!("mms: chip label err={e:?}");
            return;
        }
    };
    let props = Props::Chip(ChipProps { label: text, selected: sel, on_tap: EV_PHRASE0 + i as u32 });
    if let Err(e) = ui.set_props(node, props) {
        eprintln!("mms: chip set err={e:?}");
    }
}

/// 草稿值面换文（Input kind 值面——可显不可敲）。
fn set_draft_value(ui: &mut UiMain, a: &App, bytes: &[u8]) {
    let input = match a.chat.as_ref() {
        Some(ch) => ch.input,
        None => return,
    };
    if let Err(e) = ui.set_input_text(input, bytes) {
        eprintln!("mms: draft set err={e:?}");
    }
}

/// 输入区 hint 瞬态行（2s 自撤——「键盘挂 softime」「先选短语」两条
/// 诚实边界文案的消费面）。
fn flash_hint(ui: &mut UiMain, a: &mut App, text: &[u8]) {
    let hint = match a.chat.as_ref() {
        Some(ch) => ch.hint,
        None => return,
    };
    if let Err(e) = ui.set_text(hint, text) {
        eprintln!("mms: hint set err={e:?}");
    }
    if let Some(ch) = a.chat.as_mut() {
        ch.hint_dl = Some(now() + HINT_NS);
    }
    ui.wake(now());
}

/// chip 点选（引擎 widget_react 已翻转 selected 并派 on_tap——value =
/// 翻转后位）：选中 → 换草稿（前一枚还原 + 值面换文）；再击 → 草稿清。
fn phrase_tap(ui: &mut UiMain, a: &mut App, i: usize, selected: bool) {
    let Some(c) = a.cur else { return };
    if selected {
        if let Some(prev) = a.convs[c].draft {
            if (prev as usize) != i {
                set_chip_sel(ui, a, prev as usize, false);
            }
        }
        a.convs[c].draft = Some(i as u8);
        set_draft_value(ui, a, PHRASES[i]);
    } else if a.convs[c].draft == Some(i as u8) {
        a.convs[c].draft = None;
        set_draft_value(ui, a, b"");
    }
    ui.wake(now());
}

/// Send 消费（无草稿 = hint 引导不空转）：在途状态先收口（直跳
/// Delivered）→ Me 气泡入列（Sending 起拍）→ 草稿清（chip 还原 +
/// 值面清文）→ 状态节拍 + 对端回复节拍起拍。
fn send_draft(ui: &mut UiMain, a: &mut App) {
    let Some(c) = a.cur else { return };
    let Some(i) = a.convs[c].draft else {
        flash_hint(ui, a, b"Pick a chip below, then Send");
        return;
    };
    a.convs[c].draft = None;

    // 在途收口（新发送令前一条瞬态反馈直落终态——单在途简化）。
    let (p_node, p_msg, p_stage) = match a.chat.as_ref() {
        Some(ch) => (ch.pending_node, ch.pending_msg, ch.pending_stage),
        None => (None, 0, 0),
    };
    if p_stage != 0 && p_stage < ST_DELIVERED {
        a.convs[c].msgs[p_msg].status = ST_DELIVERED;
        if let Some(n) = p_node {
            if let Err(e) = ui.set_text(n, b"Delivered") {
                eprintln!("mms: pending set err={e:?}");
            }
        }
    }

    let idx = append_msg(ui, a, c, true, PHRASES[i as usize], ST_SENDING);

    if let Some(ch) = a.chat.as_mut() {
        ch.pending_msg = idx;
        ch.pending_stage = ST_SENDING;
        ch.status_dl = Some(now() + SENDING_NS);
        ch.pending_node = ch.rows[idx].and_then(|r| r.status);
    }
    a.reply = Some((c, now() + REPLY_NS));

    // 草稿清面（chip 还原 + 值面清文）。
    set_chip_sel(ui, a, i as usize, false);
    set_draft_value(ui, a, b"");
    ui.wake(now());
}

/// 会话页建树 + 入栈（行命中消费点）：Stack[不透明页底, 滚动层, 铬层]
/// ——铬层 = AppBar(带返回) + 底部输入区（叠层盖住滚动溢出，app_bar
/// 面层语同款）。输入区 = Stack[bg_page 面层, divider + hint 瞬态行 +
/// chips 行 + Input/Send 行]。Err = 容量/文本域违例 → fail 收口。
fn build_chat(ui: &mut UiMain, a: &mut App, c: usize) -> Result<(), UiError> {
    let t = a.t;
    let name = CONVS[c].name;

    let page = pattern::stack(ui, a.nav_root)?;
    // 不透明页底（NavHost 滑入期页下内容不可透——app_bar 面层语同款）。
    ui.add(page, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_page, w: None, h: None, radius: 0 })))?;
    // 滚动层（Scroll kind——Stack `_` 摆位吃满页 = 视口帽）。
    let scroll = ui.add(page, vspec(ViewKind::Scroll, Props::Scroll(ScrollProps { content_height: 0, offset_y: 0 })))?;
    let msg_col = ui.add(
        scroll,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps { gap: ROW_GAP, w: Some(a.vp_w), h: None, cross: CrossAlign::Stretch }),
        ),
    )?;
    pattern::spacer(ui, msg_col, 1, TOP_PAD)?;
    // 铬层（后画盖上 = 铬区滚动溢出遮盖面；缝 0 防露缝）。
    let chrome = ui.add(
        page,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps { gap: 0, w: Some(a.vp_w), h: None, cross: CrossAlign::Stretch }),
        ),
    )?;
    let bar = pattern::app_bar(ui, chrome, &t, name, true)?;
    let mid = a.vp_h.saturating_sub(pattern::TOPBAR_H).saturating_sub(INPUT_AREA_H);
    pattern::spacer(ui, chrome, 1, mid)?;

    // —— 输入区（v1 快捷短语降解面）——
    let input_stack = pattern::stack(ui, chrome)?;
    ui.add(
        input_stack,
        vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_page, w: None, h: None, radius: 0 })),
    )?;
    let input_col = ui.add(
        input_stack,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps { gap: 0, w: Some(a.vp_w), h: None, cross: CrossAlign::Stretch }),
        ),
    )?;
    pattern::divider(ui, input_col)?;
    pattern::spacer(ui, input_col, 1, PAD_V)?;
    // hint 瞬态行（空文隐藏；「键盘挂 softime」诚实边界）。
    let hint = ui.add(input_col, text_spec(t.warning, None, FontSize::S16x32, b"")?)?;
    pattern::spacer(ui, input_col, 1, PAD_V)?;
    // chips 行（快捷短语组——点选即草稿；再击取消）。
    let chips_row = ui.add(
        input_col,
        vspec(
            ViewKind::Row,
            Props::Row(RowProps { gap: CHIP_GAP, w: None, h: Some(CHIP_H), cross: CrossAlign::Center }),
        ),
    )?;
    let mut chips = [page; PHRASES.len()];
    for (i, p) in PHRASES.iter().enumerate() {
        let sel = a.convs[c].draft == Some(i as u8);
        let ch = pattern::chip(ui, chips_row, p, sel, EV_PHRASE0 + i as u32)?;
        chips[i] = ch.node;
    }
    pattern::spacer(ui, input_col, 1, PAD_V)?;
    // 草稿值面 + Send（Input kind 200×48 内在——引擎组件 v1 几何）。
    let draft_row = ui.add(
        input_col,
        vspec(
            ViewKind::Row,
            Props::Row(RowProps { gap: DRAFT_GAP, w: None, h: Some(INPUT_ROW_H), cross: CrossAlign::Center }),
        ),
    )?;
    let input = ui.add(
        draft_row,
        vspec(
            ViewKind::Input,
            Props::Input(InputProps {
                placeholder: TextBuf::new(b"Tap a chip to draft")?,
                max_len: MSG_TEXT_MAX as u32,
                input_kind: InputKind::Text,
            }),
        ),
    )?;
    let draft_flex = a.vp_w.saturating_sub(200).saturating_sub(DRAFT_GAP * 2).saturating_sub(4 * 8 + 32);
    pattern::spacer(ui, draft_row, draft_flex, 1)?;
    let send = ui.add(
        draft_row,
        vspec(
            ViewKind::Button,
            Props::Button(ButtonProps {
                label: TextBuf::new(b"Send")?,
                style: ButtonStyle::Filled,
                on_tap: EV_SEND,
            }),
        ),
    )?;
    let _ = ui.set_a11y(send, ROLE_BUTTON, b"send");
    pattern::spacer(ui, input_col, 1, PAD_V)?;

    // 气泡列（既有消息全量入列——rows/msgs 同序）。
    let n = a.convs[c].n;
    let t = a.t;
    let vp_w = a.vp_w;
    let bar_back = bar.back.ok_or(UiError::NodeBad)?;
    let mut chat = Chat {
        back: bar_back,
        scroll,
        msg_col,
        rows: [None; MSGS_MAX],
        chips,
        input,
        send,
        hint,
        content_h: chat_content_h(n),
        offset: 0,
        pending_node: None,
        pending_msg: 0,
        pending_stage: 0,
        status_dl: None,
        hint_dl: None,
        drag_y: None,
    };
    for idx in 0..n {
        let rec = a.convs[c].msgs[idx];
        add_bubble(ui, t, vp_w, &mut chat, &rec, idx)?;
    }
    chat.content_h = chat_content_h(n);
    chat.offset = chat.content_h.saturating_sub(a.vp_h); // 开页滚底（最新在底）
    ui.set_props(
        chat.scroll,
        Props::Scroll(ScrollProps { content_height: chat.content_h, offset_y: chat.offset }),
    )?;
    // 草稿值面回显（Input 值面首装——可显不可敲）。
    if let Some(d) = a.convs[c].draft {
        if let Err(e) = ui.set_input_text(chat.input, PHRASES[d as usize]) {
            eprintln!("mms: draft init err={e:?}");
        }
    }
    // Send 按压反馈 = 引擎 PressedVis 查表（press_map 登记——chips 走
    // kind selected 色变，不入表）。
    ui.set_press_map(&[(chat.send, chat.send)])?;

    // 入栈（页已挂 nav_root 下 = z 顶）+ 300ms 滑入起拍。
    a.nav.push(ui, page)?;
    a.cur = Some(c);
    a.chat = Some(chat);
    Ok(())
}

/// 建树总装（main 一次调用）：NavHost 挂根 → 列表页 Stack[滚动层,
/// 铬层]（铬层后画盖上 = Scroll v1 无 clip 的滚动溢出遮盖面）→ 行集。
/// Err = 容量/文本域违例 fail 收口。
fn build_all(ui: &mut UiMain) -> Result<App, UiError> {
    let (vp_w, vp_h) = ui.viewport();
    let t = Tokens::pick(ThemeMode::Light);
    let root = ui.root();
    let mut nav = NavHost::new();
    let nav_root = nav.attach(ui, root)?;

    // 列表页 stack（常驻底页——不 push：depth 0 = 根语义）。
    let page = pattern::stack(ui, nav_root)?;
    // 滚动层（Scroll kind——Stack `_` 摆位吃满页；内容 Col 顶让位
    // TOPBAR_H，行卡随 offset 上移滑入铬层之下）。
    let scroll = ui.add(
        page,
        vspec(ViewKind::Scroll, Props::Scroll(ScrollProps { content_height: 0, offset_y: 0 })),
    )?;
    let content_col = ui.add(
        scroll,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps { gap: ROW_GAP, w: Some(vp_w), h: None, cross: CrossAlign::Stretch }),
        ),
    )?;
    pattern::spacer(ui, content_col, 1, LIST_TOP_PAD)?;
    // 铬层（AppBar——后画 = 遮盖滚动溢出）。
    let chrome = ui.add(
        page,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps { gap: 0, w: Some(vp_w), h: None, cross: CrossAlign::Stretch }),
        ),
    )?;
    let _bar = pattern::app_bar(ui, chrome, &t, b"Messages", false)?; // 根页无返回键

    // 行表占位 NodeId（NodeId 构造子 pub(crate)——外部 crate 不能字面
    // 构造；root() 幂等取真节点充占位，build_rows 即全量覆写）。
    let ph = ui.root();
    let mut convs = [blank_conv(); CONVS.len()];
    for (i, sc) in CONVS.iter().enumerate() {
        for (j, sm) in sc.seed.iter().enumerate() {
            let take = sm.text.len().min(MSG_TEXT_MAX);
            convs[i].msgs[j].me = sm.me;
            convs[i].msgs[j].len = take as u8;
            convs[i].msgs[j].bytes[..take].copy_from_slice(&sm.text[..take]);
            convs[i].msgs[j].status = if sm.me { ST_DELIVERED } else { ST_NONE };
        }
        convs[i].n = sc.seed.len();
    }
    let mut a = App {
        t,
        vp_w,
        vp_h,
        nav,
        nav_root,
        convs,
        scroll,
        content_col,
        row_cards: [ph; CONVS.len()],
        row_avatar: [ph; CONVS.len()],
        content_h: 0,
        offset: 0,
        pressed_row: None,
        drag_y: None,
        cur: None,
        chat: None,
        reply: None,
    };
    build_rows(ui, &mut a)?;
    Ok(a)
}

/// 空会话槽。
const fn blank_conv() -> ConvState {
    ConvState { msgs: [blank_msg(); MSGS_MAX], n: 0, draft: None, replies: 0 }
}

fn main() {
    // —— 降级门：wm 客户端端未交付（GUI 通道授权未落——W4-RECON §1.2
    //    过渡形 [ui] 资产节兼判；无 fs 词 → wm 落 [0] launcher 镜像位）
    //    → 一行诊断 + park 常驻（窗口义务语义不折衷成秒退）。
    let hw = wm_cli_h();
    if hw == 0 {
        eprintln!("mms: wm absent (no gui gift)");
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
        "mms: wm v={} {}x{} input={}",
        info.version_wm,
        info.disp_w,
        info.disp_h,
        input_h()
    );

    // —— UI arena 堆置（settings/contacts 同形：UiMain 构造中间体 LTO
    //    后部分落栈 ~29KB——8 页栈穿先例，Box::leak = 常驻无回收语义）。
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    static TEXT_METRICS: ProportionalTextMetrics = ProportionalTextMetrics;
    ui.set_text_metrics(&TEXT_METRICS);
    // glyph 真供件（程序化单面：BACK——设计稿 §3.3 自绘路，零位图资产
    // 增量）。
    ui.set_icon_source(&ICONS);

    // —— 建树（Err = 容量/文本域违例 → fail 收口）。
    let mut a = match build_all(ui) {
        Ok(a) => a,
        Err(e) => fail(&format!("build err={e:?}")),
    };

    // —— 帧循环（常驻义务——永不返回）。
    run(hw, input_h(), ui, &mut a);
}

/// 帧循环 + 输入 + 事件消费（settings/contacts 食谱 + 四增量：Msg 双
/// 面消费（行命中/双页拖拽滚动/返回/输入 hint）、NavHost tick +
/// take_popped、瞬态反馈节拍（状态行/回复/hint 三源 deadline 取早）。
fn run(hw: Handle, hi_in: Handle, ui: &'static mut UiMain, a: &mut App) -> ! {
    // —— present GETINFO（wmd 中继同端）：能力面轻断言 + 几何一致性。
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

    // —— 跨代清账（前代实例孤儿别名全量排空）。
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

    // —— 帧缓冲 VMO 三连（stride = w*4 tight；len 页取整）。
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
    // SAFETY: [FRAME_VA, FRAME_VA+len) 刚由 vmar_map 落位且整段已
    // commit（settings/contacts 同形）；run 永不返回——fb 终身持有。
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };

    // —— glyph scratch 堆置（DisplayList 'a 钉调用方 scratch）。
    let scratch: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());

    // —— vsync 协商 + 起振（Idle→Run + 条件 FULL——首帧整帧 damage）。
    ui.set_vsync(VSYNC_NS, 0);
    ui.wake(now());

    let mut last_expected: u64 = 0; // pump 重采样 target（上帧 expected 轨）
    let mut last_vote: Option<berry_ui::frame::VoteBallot> = None; // 票面变化才发
    let mut presented: u64 = 0; // FRAME_CALLBACK 过链计数（首帧诊断门）
    let mut buf_id: u32 = 1; // SUBMIT buffer 代际
    let mut hi = hi_in; // input 端（失效面置 0——静态页续活）
    let mut input_pending = false; // INPUT_POLL 在途
    let mut input_buf = [0u8; gp::INPUT_REP_MAX];
    let mut input_hs = [0 as Handle; 8];

    loop {
        let t_now = now();

        // —— 真实输入（settings/contacts 同形）：先收上一轮 INPUT_POLL
        //    应答，再挂下一轮 poll；事件逐条 push 进引擎。
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
                                    eprintln!("mms: input push err={e:?}");
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {} // 无事件常态
                    Some(Err(st)) => eprintln!("mms: input poll st={st}"),
                    None => eprintln!("mms: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("mms: input recv err={s}");
                hi = 0;
                input_pending = false;
            }
        }
        if hi != 0 && !input_pending {
            if input_arm(hi) {
                input_pending = true;
            } else {
                // 输入面失效——置 0 永久降级静态页（页面常驻义务不受拖累）。
                hi = 0;
            }
        }

        // —— 输入泵（referee + 重采样 + widget_react/press_react 同拍）
        //    + **Msg 双面消费**：行命中 = Release 几何判定（Click 无坐标
        //    面）；拖拽 = 双页各自滚动域；返回/输入 hint = Click 锚判定。
        let msgs = ui.pump_input(last_expected);
        for m in msgs.iter() {
            match *m {
                Msg::Press { x, y, .. } => {
                    // 行按压反馈（ItemState 五态换底——列表页域）。
                    if a.cur.is_none() && y >= LIST_TOP_PAD {
                        if let Some(r) = row_at(ui, a, x, y) {
                            tint_row(ui, a, r, true);
                            a.pressed_row = Some(r);
                        }
                    }
                }
                Msg::Release { x, y, .. } => {
                    if let Some(r) = a.pressed_row.take() {
                        tint_row(ui, a, r, false);
                    }
                    // 行命中 → 会话页入栈（非拖 up 路径——Release 与
                    // Click 同拍成对，几何面在 Release）。
                    if a.cur.is_none() && y >= LIST_TOP_PAD {
                        if let Some(r) = row_at(ui, a, x, y) {
                            match build_chat(ui, a, r) {
                                Ok(()) => ui.wake(now()),
                                Err(e) => fail(&format!("chat build err={e:?}")),
                            }
                        }
                    }
                }
                Msg::Cancel { .. } => {
                    if let Some(r) = a.pressed_row.take() {
                        tint_row(ui, a, r, false);
                    }
                    a.drag_y = None;
                    if let Some(ch) = a.chat.as_mut() {
                        ch.drag_y = None;
                    }
                }
                Msg::DragStart { y, .. } => {
                    // 拖起 = 按压视觉还原（拖路径无 Release——补还原面）。
                    if let Some(r) = a.pressed_row.take() {
                        tint_row(ui, a, r, false);
                    }
                    a.drag_y = None;
                    if let Some(ch) = a.chat.as_mut() {
                        ch.drag_y = None;
                    }
                    if a.cur.is_none() && y >= LIST_TOP_PAD {
                        a.drag_y = Some(y as i32);
                    } else if a.cur.is_some()
                        && y > pattern::TOPBAR_H
                        && y < a.vp_h.saturating_sub(INPUT_AREA_H)
                    {
                        if let Some(ch) = a.chat.as_mut() {
                            ch.drag_y = Some(y as i32);
                        }
                    }
                }
                Msg::DragMove { y, .. } => {
                    if a.cur.is_none() {
                        if let Some(ly) = a.drag_y {
                            let dy = ly - y as i32;
                            if dy != 0 {
                                a.offset = scroll_step(
                                    ui,
                                    a.scroll,
                                    a.content_h,
                                    a.offset,
                                    a.vp_h,
                                    dy,
                                );
                                a.drag_y = Some(y as i32);
                                ui.wake(now());
                            }
                        }
                    } else if let Some(ch) = a.chat.as_mut() {
                        if let Some(ly) = ch.drag_y {
                            let dy = ly - y as i32;
                            if dy != 0 {
                                ch.offset = scroll_step(
                                    ui,
                                    ch.scroll,
                                    ch.content_h,
                                    ch.offset,
                                    a.vp_h,
                                    dy,
                                );
                                ch.drag_y = Some(y as i32);
                                ui.wake(now());
                            }
                        }
                    }
                }
                Msg::DragEnd { .. } => {
                    a.drag_y = None;
                    if let Some(ch) = a.chat.as_mut() {
                        ch.drag_y = None;
                    }
                }
                Msg::Click { node } => {
                    // 判先行后动（flash_hint 需 &mut App——先结束只读借）。
                    let (is_back, is_input) = match a.chat.as_ref() {
                        Some(ch) => (ch.back == node, ch.input == node),
                        None => (false, false),
                    };
                    // 会话页返回键（app 内手势位——系统 back 链挂 3b）。
                    if is_back {
                        if a.nav.pop(ui) {
                            ui.wake(now());
                        }
                    } else if is_input {
                        // 草稿值面点按（引擎已置焦环/caret——无 softime
                        // 不可敲，hint 瞬态行诚实标注）。
                        flash_hint(ui, a, b"Typing hangs on softime - pick a chip");
                    }
                }
            }
        }

        // —— NavHost 推拉动画步进（在途 → 每帧推进 + 保活）。
        if a.nav.tick(ui, t_now) {
            ui.wake(t_now);
        }
        // —— 出栈完结页移交 remove + 列表刷新（Draft 角标回显；保留滚
        //    动位）。在途状态收口数据面（直落 Delivered）；回复节拍跨
        //    pop 存活（数据面入列）。
        if let Some(node) = a.nav.take_popped() {
            if let Some(c) = a.cur {
                let (p_msg, p_stage) = match a.chat.as_ref() {
                    Some(ch) => (ch.pending_msg, ch.pending_stage),
                    None => (0, 0),
                };
                if p_stage != 0 && p_stage < ST_DELIVERED {
                    a.convs[c].msgs[p_msg].status = ST_DELIVERED;
                }
            }
            if let Err(e) = ui.set_focus(None) {
                eprintln!("mms: focus clear err={e:?}");
            }
            if let Err(e) = ui.remove(node) {
                fail(&format!("chat remove err={e:?}"));
            }
            a.cur = None;
            a.chat = None;
            if let Err(e) = build_rows(ui, a) {
                fail(&format!("rows rebuild err={e:?}"));
            }
            ui.wake(t_now);
        }

        // —— 组件事件消费（widget_react 已派——此处纯 app 面）：chip
        //    点选（value = 翻转后 selected 位）/ Send。
        let evs = ui.take_events();
        for ev in evs.iter() {
            if ev.event == EV_SEND {
                send_draft(ui, a);
            } else if let Some(i) = phrase_index(ev.event) {
                phrase_tap(ui, a, i, ev.value == 1);
            }
        }

        // —— 发送瞬态行反馈节拍（Sending → Sent → Delivered）。
        let status_due = a.chat.as_ref().and_then(|ch| ch.status_dl);
        if let Some(dl) = status_due {
            if t_now >= dl {
                let (p_node, p_msg, p_stage) = match a.chat.as_ref() {
                    Some(ch) => (ch.pending_node, ch.pending_msg, ch.pending_stage),
                    None => (None, 0, 0),
                };
                let mut acted = false;
                if let Some(c) = a.cur {
                    match p_stage {
                        ST_SENDING => {
                            a.convs[c].msgs[p_msg].status = ST_SENT;
                            if let Some(n) = p_node {
                                if let Err(e) = ui.set_text(n, b"Sent") {
                                    eprintln!("mms: status set err={e:?}");
                                }
                            }
                            if let Some(ch) = a.chat.as_mut() {
                                ch.pending_stage = ST_SENT;
                                ch.status_dl = Some(t_now + SENT_NS);
                            }
                            acted = true;
                        }
                        ST_SENT => {
                            a.convs[c].msgs[p_msg].status = ST_DELIVERED;
                            if let Some(n) = p_node {
                                if let Err(e) = ui.set_text(n, b"Delivered") {
                                    eprintln!("mms: status set err={e:?}");
                                }
                            }
                            if let Some(ch) = a.chat.as_mut() {
                                ch.pending_stage = ST_DELIVERED;
                                ch.status_dl = None;
                                ch.pending_node = None;
                            }
                            acted = true;
                        }
                        _ => {}
                    }
                }
                if !acted {
                    // 空转档（在途已收口）——期限清除防忙等。
                    if let Some(ch) = a.chat.as_mut() {
                        ch.status_dl = None;
                    }
                }
                ui.wake(t_now);
            }
        }

        // —— 对端本地模拟回复节拍（跨 pop 存活——数据面入列；会话页
        //    打开则气泡同拍入列 + 滚底；列表页可见则行集刷新（末条
        //    摘要活更新）。
        if let Some((rc, dl)) = a.reply {
            if t_now >= dl {
                a.reply = None;
                let r = a.convs[rc].replies as usize % REPLIES.len();
                append_msg(ui, a, rc, false, REPLIES[r], ST_NONE);
                a.convs[rc].replies = a.convs[rc].replies.wrapping_add(1);
                if a.cur.is_none() {
                    if let Err(e) = build_rows(ui, a) {
                        fail(&format!("rows rebuild err={e:?}"));
                    }
                }
                ui.wake(t_now);
            }
        }

        // —— hint 瞬态行自撤（2s 驻留帧循环驱动）。
        let hint_due = a.chat.as_ref().and_then(|ch| ch.hint_dl);
        if let Some(dl) = hint_due {
            if t_now >= dl {
                if let Some(ch) = a.chat.as_mut() {
                    ch.hint_dl = None;
                    if let Err(e) = ui.set_text(ch.hint, b"") {
                        eprintln!("mms: hint clear err={e:?}");
                    }
                }
                ui.wake(t_now);
            }
        }

        // —— 渲染（damage 空集零提交——跳过帧零 RPC）。
        let mut enc = CommandEncoder::new();
        let out = ui.render_frame(t_now, scratch, &mut enc);
        last_expected = out.expected_present_ns;
        if out.drew {
            let dmg = out.damage;
            // K>0 xor FULL 立法兜底：count==0 非空集视同整帧。
            let full = dmg.full || dmg.count == 0;
            let pass = if full {
                PassDescriptor::clear(CLEAR_BG.r, CLEAR_BG.g, CLEAR_BG.b)
            } else {
                // 增量帧：Load+Store + K 矩形并集包络（soft 光栅只在
                // dirty 域动笔，域外像素保持上帧）。
                PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(damage_envelope(&dmg)))
            };
            let dl = enc.finish();
            if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
                fail(&format!("raster submit {e:?}"));
            }

            // SUBMIT：fresh READ dup 别名随行（send 即 take；TRANSFER 保位）。
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

            // FRAME_CALLBACK：断 PRESENTED（呈现链破相 fail loud）。
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

            // RELEASE（归还别名即关——在途 ≤1 结构性背压）。
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
                // 首帧呈现回执过链（诊断面）。
                println!("mms: first frame");
            }
        }

        // —— vote（票面变化才发——省流；幂等面在服务端）。
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

        // —— 稳态等待：deadline = min(next_wakeup, IDLE 帽, 状态节拍,
        //    回复节拍, hint 自撤)——五源取早（引擎 Idle 相无定时器，app
        //    面节拍补源）。
        let cap = now() + IDLE_CAP_NS;
        let mut deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
        if let Some(dl) = a.chat.as_ref().and_then(|ch| ch.status_dl) {
            deadline = deadline.min(dl);
        }
        if let Some((_, dl)) = a.reply {
            deadline = deadline.min(dl);
        }
        if let Some(dl) = a.chat.as_ref().and_then(|ch| ch.hint_dl) {
            deadline = deadline.min(dl);
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

// ---------------------------------------------------------------------------
// —— 纯函数单测（host 面可检——bin 链 berry-libc 的 aarch64 ELF 专属
//    asm 不可宿主执行，执行面归 QEMU/镜像冒烟）——

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_bytes_map() {
        assert_eq!(status_bytes(ST_NONE), None);
        assert_eq!(status_bytes(ST_SENDING), Some(b"Sending".as_slice()));
        assert_eq!(status_bytes(ST_SENT), Some(b"Sent".as_slice()));
        assert_eq!(status_bytes(ST_DELIVERED), Some(b"Delivered".as_slice()));
        // 「Delivered」9 字 = ST_CHARS 单源核对。
        assert_eq!(status_bytes(ST_DELIVERED).unwrap().len() as u32, ST_CHARS);
    }

    #[test]
    fn phrase_index_decodes() {
        assert_eq!(phrase_index(EV_PHRASE0), Some(0));
        assert_eq!(phrase_index(EV_PHRASE0 + 6), Some(6));
        assert_eq!(phrase_index(EV_PHRASE0 + PHRASES.len() as u32), None);
        assert_eq!(phrase_index(EV_SEND), None);
    }

    #[test]
    fn bubble_width_clamps() {
        // 短文 → 最小宽（内衬 96 + 8×24）。
        assert_eq!(bubble_w(2, 1080, false), MIN_BUBBLE_W);
        assert_eq!(bubble_w(0, 1080, true), MIN_BUBBLE_W.max(ST_CHARS * ST_CW + pattern::LISTITEM_PAD_X * 2));
        // Me 侧至少容状态行（9×16 + 96 = 240 < 288 → 仍最小宽）。
        assert_eq!(bubble_w(3, 1080, true), MIN_BUBBLE_W);
        // 正文 26 字 = 624 + 96 = 720 ≤ 上帽 810。
        assert_eq!(bubble_w(26, 1080, false), 26 * 24 + pattern::LISTITEM_PAD_X * 2);
        // 上帽 = vp_w*3/4（窄窗防御）。
        assert_eq!(bubble_w(60, 400, false), 300);
        // 最小宽不越窄窗上帽。
        assert!(bubble_w(1, 320, false) <= 320 / 4 * 3);
    }

    #[test]
    fn summary_prefix_and_cap() {
        let mut m = blank_msg();
        m.me = false;
        m.len = 6;
        m.bytes[..6].copy_from_slice(b"hello!");
        let mut out = [0u8; 24];
        let n = summary_bytes(&m, &mut out);
        assert_eq!(&out[..n], b"hello!");
        m.me = true;
        let mut out = [0u8; 24];
        let n = summary_bytes(&m, &mut out);
        assert_eq!(&out[..n], b"Me: hello!");
        // 帽 24：20 字正文 + 4 字前缀。
        m.len = 32;
        let mut out = [0u8; 24];
        let n = summary_bytes(&m, &mut out);
        assert_eq!(n, 24);
    }

    #[test]
    fn back_glyph_shape() {
        // 「<」双臂：顶点在、右远端空、臂上方空（形状面抽查）。
        assert_eq!(BACK_PX.len(), GLYPH_N * GLYPH_N);
        assert!(BACK_PX[12 * GLYPH_N + 8] != 0, "chevron 顶点在");
        assert_eq!(BACK_PX[12 * GLYPH_N + 20], 0, "右远端空");
        assert_eq!(BACK_PX[3 * GLYPH_N + 12], 0, "臂上方空");
    }
}
