//! contacts —— 波 4 第三件真 GUI 应用（CONTACTS appstub 秒退壳的翻新；
//! .agents/plans/2026-11-15-波4预制应用翻新实施设计.md §2.3、worktree 根
//! W4-RECON.md §四 Contacts 行）。
//!
//! **使命**（v1 四件，静态数据）：
//! - 列表页：AppBar(title=Contacts) + Seg(All/Starred 收藏过滤) +
//!   Scroll[List]（`pattern::list_item` 行 = 首字母头像位 + 姓名 + 号码 +
//!   收藏星标 glyph）；
//! - 详情页：`NavHost` push/pop（300ms 推拉动画——pattern 页栈状态机，
//!   值面页序自管：返回 = app 内 AppBar 返回 glyph 的手势位，无系统
//!   back 链——3b 依赖，见 W4-IMPL-CONTACTS.md 差异清单）；
//! - Call → 「Calling <name>...」底部瞬态行（appshell Toast 的 v1 降解：
//!   帧循环节拍 2s 自撤；无 modem 仅 UI 反馈——诚实边界）；
//! - Star 收藏切换（详情页 Button）+ Seg 过滤联动（回列表刷新行集）。
//!
//! **零手拼铁律**（W4-RECON §二/§五）：条目一律 `pattern::list_item`
//!（Card kind 自绘卡面/阴影，8vp 圆角单源）；骨架一律 pattern 配方
//!（app_bar/seg/stack/spacer）；按压反馈单源 = `pattern::ItemState` 五态
//! 预混表（主树等价面——RECON §二末段裁定「用 ItemState 面」；行按压 =
//! app 侧按 press_react 同形 set_props 换底，色值零字面量）+ Button kind
//! 引擎 PressedVis 查表（press_map 登记两枚）；文案全 ASCII（HD 档 95
//! 字模域）；本 crate 禁手写 Rect 拼卡/press 遮罩字面量。页底/面层 Rect
//! 仅用于 pattern 叠层语（app_bar 面层同款——非卡片拼装）。
//!
//! **链形态** = settings 垂直切片镜像（9035af061 先例；berry-launcher
//! 同源食谱）：wm_cli_h() 门控（0 = GUI 通道未授权 → 单行诊断 + park
//! 常驻，不秒退）→ WM_GETINFO 几何 → present GETINFO 轻断言 → 帧缓冲
//! VMO 三连（create/commit/map @ FRAME_VA）→ 三态停帧 + SUBMIT →
//! FRAME_CALLBACK → RELEASE 严格内联串行 + vote 票面变化才发。
//! 输入 = input_h() INPUT_POLL 批拉 → push_pointer → pump_input（手势
//! referee + widget_react 同拍）→ Msg/组件事件双面消费。
//!
//! **v1 降解面**（3b 未落——完整清单见 W4-IMPL-CONTACTS.md）：
//! - 无 berry-appshell（rpc/pump/scene/conf 五模块）——RPC 半程手拼 +
//!   自管帧循环照 settings v0 形；
//! - 无 wm back 路由/softimed——详情返回走 app 内 glyph，无 Search 敲键面；
//! - 无 appshell::conf/CONTACTS.JSN——数据内置静态 10 条（持久化挂终态）；
//! - 退出形态照先例：无退出路径（fail = eprintln + park；崩溃监督归
//!   svcmgr app_supervise 域）。
//!
//! **输出面**：manifest console=false → stdout 走 svcmgr 私有对；首帧
//! 呈现回执一行 `contacts: first frame`（诊断面，非串口标记）。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::io::{input_h, self_vmar_h, wm_cli_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_many, berry_object_wait_one, berry_thread_sleep,
    berry_vmar_map, berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_ui::desc::{
    ButtonProps, ButtonStyle, ColProps, IconProps, Props, RectProps, RowProps, ScrollProps,
    TextBuf, TextProps, TextStyle, ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::pattern::{self, ItemState, NavHost, Seg};
use berry_ui::theme::{self, Theme, Tokens};
use berry_ui::{
    IconSource, Msg, NodeId, ProportionalTextMetrics, UiError, UiMain, ROLE_BUTTON,
    ROLE_LIST_ITEM, ROLE_TEXT,
};

// 链接种子（fsmgr/guifbd/berry-launcher/settings 同款）：rlib/LTO 按需
// 保留——无引用则 berry-libc 的 crt0（含 `_start`）不进链接；`#[used]`
// 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（launcher/settings 同值 300s：app 拉起时点 =
/// 用户点击，首 RPC 距 boot 稳态可远）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// present 域 1B ST_BUSY 拒答重试总帽（berry-appshell pump.rs
/// PRESENT_BUSY_RETRY_CAP_NS 同值）：wmd presarb 对他客户端事务在途的
/// SUBMIT/RELEASE 快答 ST_BUSY（属主租约 5s 内让位），帽 8s > 租约——到帽前
/// 属主事务必已收口或被租约抢占，重试必过；到帽才 fail（真死链面）。
const BUSY_RETRY_CAP_NS: u64 = 8 * 1_000_000_000;

/// ST_BUSY 步进重试步进（100ms——步进睡让出 CPU，醒即重发）。
const BUSY_RETRY_STEP_NS: u64 = 100 * 1_000_000;

/// 帧缓冲映射 VA（launcher frameloop 同位：mmap VA 窗内固定位 +16MiB）。
const FRAME_VA: usize = 0x5100_0000;

/// 默认 vsync 周期（60Hz；present GETINFO 无周期域——S2 食谱默认）。
const VSYNC_NS: u64 = 16_666_666;

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// glyph scratch 容量（堆置——paint 对超容 run 截断尾字形；列表页
/// ~120 字形 + 详情页 ~40 富余）。
const SCRATCH_GLYPHS: usize = 256;

/// 整帧清屏底色 = 页面底 token 单源（light 档 BG_PAGE；卡片白面/文本
/// 全部叠画其上——theme.rs 唯一归属，本文件零清屏色值字面量）。
const CLEAR_BG: Color = theme::BG_PAGE;

/// 强调底上的前景字色（pattern.rs 私有 const `ON_ACCENT` 按值镜像并
/// 注明单源——settings crate 镜像 berry-ui pub(crate) 几何常量同款先例；
/// theme OnPrimary 档收编后随迁）。
const ON_ACCENT: Color = Color { r: 255, g: 255, b: 255 };

/// Toast 瞬态行驻留时长（appshell Toast「底部 2s 短驻留」同拍——
/// W4-RECON §四 Contacts 行「帧循环节拍 2s 自撤」）。
const CALLING_NS: u64 = 2 * 1_000_000_000;

/// 行内姓名/号码列宽估值上界（8 字 × S24x48 cell_w=24——HD 比例 advance
/// 恒 ≤ cell_w，估值只影响星标右对齐推位，不进命中/卡宽语义）。
const NAME_COL_W: u32 = 8 * 24;

/// 列尾留白（Scroll 内容尾部——尾行与视口底缘呼吸位）。
const TAIL_PAD: u32 = 24;

/// 行间缝（内容 Col gap——pattern 行卡间距）。
const ROW_GAP: u32 = 8;

/// 详情页瞬态行高（空文时预留带 = 页底锚位；S24x48 字格高同值）。
const STRIP_H: u32 = 48;

/// 组件事件 ID 面（Button props on_tap 携载——take_events 消费锚）。
const EV_CALL: u32 = 1;
const EV_STAR: u32 = 2;

// ---------------------------------------------------------------------------
// —— 静态种子数据（v1 内置——CONTACTS.JSN 持久化挂 appshell::conf 终态）——

/// 单条联系人（文案全 ASCII——HD 档 95 字模域硬约束）。
struct Seed {
    name: &'static [u8],
    phone: &'static [u8],
    star: bool,
}

/// 种子 10 条（设计稿 §2.3「ASCII 拟名 10 条」；3 条预置收藏——Seg
/// 过滤有可判差异）。
const SEEDS: [Seed; 10] = [
    Seed { name: b"Alice", phone: b"555-0101", star: false },
    Seed { name: b"Bob", phone: b"555-0102", star: false },
    Seed { name: b"Carol", phone: b"555-0103", star: true },
    Seed { name: b"David", phone: b"555-0104", star: false },
    Seed { name: b"Eve", phone: b"555-0105", star: false },
    Seed { name: b"Frank", phone: b"555-0106", star: false },
    Seed { name: b"Grace", phone: b"555-0107", star: true },
    Seed { name: b"Heidi", phone: b"555-0108", star: false },
    Seed { name: b"Ivan", phone: b"555-0109", star: true },
    Seed { name: b"Judy", phone: b"555-0110", star: false },
];

// ---------------------------------------------------------------------------
// —— 应用内功能 glyph（程序化绘制——设计稿 §3.3「v1 = berry-ui 程序化
//    绘制（波1 导航键自绘先例——ADR #63 备选路），不新增位图资产」）——
//    IconSource 真供件：Argb 1-bit 透明键（A==0 跳过——raster argb_blit
//    同律），色值全部 theme token 单源打包。源 24px：back 按
//    APP_BAR_BACK=72px 三倍整数最近邻（硬边无走样）；星标 48px 二倍。

/// glyph 源边长（px）。
const GLYPH_N: usize = 24;

/// 星标 glyph id（自域 0x02xx——pattern::icons 约定域外自定义位）。
const STAR_ON: u16 = 0x0210;
const STAR_OFF: u16 = 0x0211;

/// Color → RGB 打包（const 面——theme 常量进 const 求值）。
const fn pack_rgb(c: Color) -> u32 {
    ((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32
}

/// 五角星十顶点表（中心 (12,12)、外径 11、内径 4.6——手工取整坐标；
/// 几何常量非样式面，不入 theme 单源纪律）。
const STAR_VERTS: [(i32, i32); 10] = [
    (12, 1),
    (15, 8),
    (22, 9),
    (16, 13),
    (18, 21),
    (12, 17),
    (6, 21),
    (8, 13),
    (2, 9),
    (9, 8),
];

/// 偶奇填充点在多边形内判定（整数运算 const fn——扫描线交点奇偶；
/// 除法四舍五入（符号归一）——截断除会令亚像素腿宽塌 0）。
const fn in_star(x: i32, y: i32) -> bool {
    let mut inside = false;
    let mut i = 0usize;
    let mut j = 9usize;
    while i < 10 {
        let (xi, yi) = STAR_VERTS[i];
        let (xj, yj) = STAR_VERTS[j];
        if (yi > y) != (yj > y) {
            // 交点横坐标 = xi + (y-yi)(xj-xi)/(yj-yi)（分支内分母非零）。
            let mut num = (y - yi) * (xj - xi);
            let mut den = yj - yi;
            if den < 0 {
                num = -num;
                den = -den;
            }
            let hit = xi + (2 * num + den) / (2 * den);
            if x < hit {
                inside = !inside;
            }
        }
        j = i;
        i += 1;
    }
    inside
}

/// 五角星位图（rgb = theme token 打包——两态各一表）。
const fn build_star(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    let mut px = [0u32; GLYPH_N * GLYPH_N];
    let mut i = 0usize;
    while i < GLYPH_N * GLYPH_N {
        let x = (i % GLYPH_N) as i32;
        let y = (i / GLYPH_N) as i32;
        px[i] = if in_star(x, y) { 0xFF00_0000 | rgb } else { 0 };
        i += 1;
    }
    px
}

/// 点到线段垂距 ≤ r（整数：cross² ≤ r²·len² 且投影落在段内；线段端点
/// 方帽退化按投影门拒——glyph 光栅足够）。
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
static STAR_ON_PX: [u32; GLYPH_N * GLYPH_N] = build_star(pack_rgb(theme::BRAND));
static STAR_OFF_PX: [u32; GLYPH_N * GLYPH_N] = build_star(pack_rgb(theme::FONT_SECONDARY));

/// 本应用 IconSource 真供件（berry-ui D28 供给槽——app 侧只用注册域
/// 内 id，域外值空绘不炸）。
struct ContactsIcons;

static ICONS: ContactsIcons = ContactsIcons;

impl IconSource for ContactsIcons {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        match id {
            pattern::icons::BACK | STAR_ON | STAR_OFF => Some((GLYPH_N as u32, GLYPH_N as u32)),
            _ => None,
        }
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        match id {
            pattern::icons::BACK => Some(&BACK_PX),
            STAR_ON => Some(&STAR_ON_PX),
            STAR_OFF => Some(&STAR_OFF_PX),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// —— RPC 半程族 + 帧循环小件（launcher `rpc`/settings 同形拷贝）——

/// 诊断 + park 收口（settings `fail` 同形——app 常驻不被 reap；崩溃
/// 监督归 svcmgr app_supervise 域，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("contacts: {what}");
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

/// present 域 1B ST_BUSY 拒答的步进重试（berry-appshell pump.rs busy_step
/// 同形）：100ms 步进让出 CPU，总帽 [`BUSY_RETRY_CAP_NS`]（`cap_dl` = 调用点
/// 局部帽位，0 = 首次起算）；到帽 fail。快答未转发 + 随行已关收，重发安全
///（RELEASE_WAIT 零随行——无别名重 dup 面）。
fn busy_step(what: &str, cap_dl: &mut u64) {
    if *cap_dl == 0 {
        *cap_dl = now() + BUSY_RETRY_CAP_NS;
        eprintln!("contacts: {what}");
    }
    if now() >= *cap_dl {
        fail(&format!("{what}: 帽到点"));
    }
    if berry_thread_sleep(now() + BUSY_RETRY_STEP_NS) != 0 {
        fail("busy retry sleep");
    }
}

/// K 矩形并集包络（增量 pass 的 dirty 域；launcher/settings 同形——
/// engine damage 恒 viewport 裁剪，i32 折 try_from 饱和防御）。
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
            eprintln!("contacts: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("contacts: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}

// ---------------------------------------------------------------------------
// —— 建树小件 ——

/// Text 叶 spec（bg = 头像衬底/瞬态行底；超 `TEXT_INLINE`=64B loud Err
/// ——本页文案 ≤24B 域内）。
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

/// 「Calling <name>...」字节串装配（≤64B 域内——name ≤8B）。
fn calling_bytes(name: &[u8], out: &mut [u8; 64]) -> usize {
    const PRE: &[u8] = b"Calling ";
    const POST: &[u8] = b"...";
    let mut n = 0usize;
    for &b in PRE {
        out[n] = b;
        n += 1;
    }
    for &b in name {
        out[n] = b;
        n += 1;
    }
    for &b in POST {
        out[n] = b;
        n += 1;
    }
    n
}

// ---------------------------------------------------------------------------
// —— 应用状态（建树一次落定 + 运行期翻转面）——

/// 联系人表运行态句柄（建树/交互/帧循环三面共用）。
struct App {
    /// 主题 token 组（light 档——全页配色单源）。
    t: Tokens,
    /// 视口宽（px）。
    vp_w: u32,
    /// 视口高（px）。
    vp_h: u32,
    /// 顶栏 + Seg 铬层高（pattern TOPBAR_H + SEG_BAR_H——滚动遮盖边界）。
    chrome_h: u32,
    /// 页栈状态机（NavHost——详情 push/pop 300ms 推拉）。
    nav: NavHost,
    /// NavHost 栈容器（页挂载锚）。
    nav_root: NodeId,
    /// 列表滚动容器（Scroll kind——app 自管 offset 域）。
    scroll: NodeId,
    /// 滚动内容 Col（行卡挂载锚 + 顶让位 spacer）。
    content_col: NodeId,
    /// Seg 过滤器（All/Starred）。
    seg: Seg,
    /// 当前过滤档（false = All / true = Starred）。
    filter_starred: bool,
    /// 行卡表（当前过滤档可见集——remove/命中几何锚）。
    row_cards: [NodeId; SEEDS.len()],
    /// 行卡 → 种子索引（星标态/详情数据源）。
    row_contact: [usize; SEEDS.len()],
    /// 行头像 Text 节点（按压换底锚）。
    row_avatar: [NodeId; SEEDS.len()],
    /// 可见行数。
    rows_n: usize,
    /// 空态提示节点（Starred 档无收藏——None = 无在树；build_rows 先回收）。
    empty_hint: Option<NodeId>,
    /// 收藏态运行副本（Star 翻转面——v1 会话态，持久化挂终态）。
    stars: [bool; SEEDS.len()],
    /// 滚动内容总高（ScrollProps.content_height 真值）。
    content_h: u32,
    /// 当前滚动偏移（app 自管域——clamp [0, content_h − vp_h]）。
    offset: u32,
    /// 详情页节点（Some = 在栈/在过渡——行命中/Seg 门控）。
    detail_page: Option<NodeId>,
    /// 详情页当前联系人（种子索引）。
    detail_contact: Option<usize>,
    /// 详情页返回键（AppBar back glyph——app 内手势位）。
    detail_back: Option<NodeId>,
    /// 详情页 Star 按钮（label 随收藏态换文）。
    detail_star_btn: Option<NodeId>,
    /// 详情页底部瞬态行 Text（空文 = 隐藏）。
    detail_calling: Option<NodeId>,
    /// 瞬态行自撤期限（None = 空闲——帧循环节拍 2s）。
    calling_until: Option<u64>,
    /// 按压中行索引（Press 置 / Release·Cancel·DragStart 还原）。
    pressed_row: Option<usize>,
    /// 列表拖拽纵坐标（DragStart 置 / DragMove 累进 / DragEnd 清）。
    drag_y: Option<i32>,
}

/// 几何命中：坐标 → 可见行索引（`ui.bounds` 行卡界包含判定——Card
/// 子树全幅，行内任意节点命中都落本界；pattern 内部节点 id 不外露的
/// 补偿面；Click 无坐标面故走 Release 判定）。
fn row_at(ui: &mut UiMain, a: &App, x: u32, y: u32) -> Option<usize> {
    for i in 0..a.rows_n {
        if let Ok(b) = ui.bounds(a.row_cards[i]) {
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
    let contact = a.row_contact[row];
    let initial = [SEEDS[contact].name[0]];
    let text = match TextBuf::new(&initial) {
        Ok(tb) => tb,
        Err(e) => {
            eprintln!("contacts: avatar rebuild err={e:?}");
            return;
        }
    };
    let bg = if on { state_color(ItemState::Pressed.bg()) } else { a.t.brand };
    let props = Props::Text(TextProps {
        style: TextStyle { fg: ON_ACCENT, bg: Some(bg), scale: FontSize::S32x48 },
        text,
    });
    if let Err(e) = ui.set_props(a.row_avatar[row], props) {
        eprintln!("contacts: press tint err={e:?}");
    }
}

/// 滚动偏移累进（clamp 非负 + 不越尾——overscroll 无橡皮筋，v1 直钳）。
fn scroll_by(ui: &mut UiMain, a: &mut App, dy: i32) {
    let max_off = a.content_h.saturating_sub(a.vp_h);
    let next = (a.offset as i64 + dy as i64).clamp(0, max_off as i64) as u32;
    if next == a.offset {
        return;
    }
    a.offset = next;
    let props = Props::Scroll(ScrollProps { content_height: a.content_h, offset_y: a.offset });
    if let Err(e) = ui.set_props(a.scroll, props) {
        eprintln!("contacts: scroll set err={e:?}");
    }
}

/// 重建行集（初建/Seg 切档/详情返回三消费点——旧卡整子树 remove 后按
/// 当前过滤档重挂；Scroll content_height/a11y/偏移钳随行更新。
/// `reset_scroll`：true = 过滤切档回顶；false = 保留偏移（钳新域）。
fn build_rows(ui: &mut UiMain, a: &mut App, reset_scroll: bool) -> Result<(), UiError> {
    // 旧行整子树回收（remove 连带清 a11y/子槽——代数翻新悬空 id 无义；
    // 空态提示同回收——防残树占位打断新行序）。
    for i in 0..a.rows_n {
        ui.remove(a.row_cards[i])?;
    }
    a.rows_n = 0;
    a.pressed_row = None;
    if let Some(h) = a.empty_hint.take() {
        ui.remove(h)?;
    }

    let t = a.t;
    let mut n = 0usize;
    for (idx, s) in SEEDS.iter().enumerate() {
        if a.filter_starred && !a.stars[idx] {
            continue;
        }
        // — 行卡（pattern::list_item = Card(0, None) 薄别名——48vp 行、
        //   卡面/8vp 圆角/阴影 Card kind 自绘，零手拼）—
        let card = pattern::list_item(ui, a.content_col, &t)?;
        // 行内容：Row[首字母头像, 姓名列, 推位, 收藏星标]。
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
        // 见报告差异清单）。
        let initial = [s.name[0]];
        let avatar = ui.add(
            line,
            text_spec(ON_ACCENT, Some(t.brand), FontSize::S32x48, &initial)?,
        )?;
        // 姓名主行 + 号码次行。
        let labels = ui.add(
            line,
            vspec(
                ViewKind::Col,
                Props::Col(ColProps { gap: 4, w: None, h: None, cross: CrossAlign::Start }),
            ),
        )?;
        ui.add(labels, text_spec(t.font_primary, None, FontSize::S24x48, s.name)?)?;
        ui.add(labels, text_spec(t.font_secondary, None, FontSize::S16x32, s.phone)?)?;
        // 推位（估值定宽——星标右对齐；溢出折 0 无害）。
        let flex = a
            .vp_w
            .saturating_sub(2 * pattern::LISTITEM_PAD_X)
            .saturating_sub(48)
            .saturating_sub(NAME_COL_W)
            .saturating_sub(48);
        pattern::spacer(ui, line, flex, 1)?;
        // 收藏星标 glyph（两态 Argb 表——theme token 打包单源）。
        let star_id = if a.stars[idx] { STAR_ON } else { STAR_OFF };
        ui.add(
            line,
            vspec(
                ViewKind::Icon,
                Props::Icon(IconProps {
                    id: star_id,
                    fg: t.font_secondary,
                    w: Some(48),
                    h: Some(48),
                }),
            ),
        )?;

        ui.set_a11y(card.node, ROLE_LIST_ITEM, s.name)?;
        a.row_cards[n] = card.node;
        a.row_contact[n] = idx;
        a.row_avatar[n] = avatar;
        n += 1;
    }
    a.rows_n = n;

    // 空态提示（Starred 档无收藏——诚实展示非空白；占一行高位）。
    if n == 0 {
        let hint = ui.add(
            a.content_col,
            text_spec(t.font_secondary, None, FontSize::S24x48, b"No starred contacts")?,
        )?;
        ui.set_a11y(hint, ROLE_TEXT, b"empty")?;
        a.empty_hint = Some(hint);
    }

    // content_height 精确记账：顶让位 + 行集（含缝）+ 尾留白。
    let rows_h = if n == 0 {
        pattern::LISTITEM_H
    } else {
        n as u32 * pattern::LISTITEM_H + n as u32 * ROW_GAP
    };
    a.content_h = a.chrome_h + TAIL_PAD + rows_h;
    a.offset = if reset_scroll {
        0
    } else {
        a.offset.min(a.content_h.saturating_sub(a.vp_h))
    };
    ui.set_props(
        a.scroll,
        Props::Scroll(ScrollProps { content_height: a.content_h, offset_y: a.offset }),
    )?;
    Ok(())
}

/// 详情页建树 + 入栈（行命中消费点）：Stack[不透明页底, 内容 Col]——
/// NavHost 滑入期覆盖底下列表页；内容 = AppBar(带返回) + 居中名片 +
/// 按钮行 + 底部瞬态行。Err = 容量/文本域违例 → fail 收口。
fn build_detail(ui: &mut UiMain, a: &mut App, contact: usize) -> Result<(), UiError> {
    let t = a.t;
    let s = &SEEDS[contact];

    let page = pattern::stack(ui, a.nav_root)?;
    // 不透明页底（NavHost 滑入期页下内容不可透——pattern app_bar「面层
    // Rect」叠层语同款）。
    ui.add(page, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_page, w: None, h: None, radius: 0 })))?;
    let body = ui.add(
        page,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps {
                gap: 0,
                w: Some(a.vp_w),
                h: Some(a.vp_h),
                cross: CrossAlign::Stretch,
            }),
        ),
    )?;

    // 顶栏（返回键 = app 内手势位——系统 back 链挂 3b；标题 = 姓名）。
    let bar = pattern::app_bar(ui, body, &t, s.name, true)?;

    // 名片区（居中容器）：首字母头像 + 姓名 + 号码 + 按钮行。
    let initial = [s.name[0]];
    let center_h = 48 + 24 + 48 + 24 + 48 + 24 + 48; // 头像/姓名/号码/按钮 + 三缝
    let pad = a
        .vp_h
        .saturating_sub(pattern::TOPBAR_H)
        .saturating_sub(center_h)
        .saturating_sub(STRIP_H)
        / 2; // 窄窗防御：饱和折 0（内容顶格贴底，无下溢 panic）
    pattern::spacer(ui, body, 1, pad)?;
    let center = ui.add(
        body,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps {
                gap: 24,
                w: Some(a.vp_w),
                h: None,
                cross: CrossAlign::Center,
            }),
        ),
    )?;
    let avatar = ui.add(center, text_spec(ON_ACCENT, Some(t.brand), FontSize::S32x48, &initial)?)?;
    let name = ui.add(center, text_spec(t.font_primary, None, FontSize::S32x48, s.name)?)?;
    let phone = ui.add(center, text_spec(t.font_secondary, None, FontSize::S24x48, s.phone)?)?;
    let buttons = ui.add(
        center,
        vspec(
            ViewKind::Row,
            Props::Row(RowProps { gap: 48, w: None, h: None, cross: CrossAlign::Center }),
        ),
    )?;
    let call = ui.add(
        buttons,
        vspec(
            ViewKind::Button,
            Props::Button(ButtonProps {
                label: TextBuf::new(b"Call")?,
                style: ButtonStyle::Filled,
                on_tap: EV_CALL,
            }),
        ),
    )?;
    let star_label: &[u8] = if a.stars[contact] { b"Unstar" } else { b"Star" };
    let star = ui.add(
        buttons,
        vspec(
            ViewKind::Button,
            Props::Button(ButtonProps {
                label: TextBuf::new(star_label)?,
                style: ButtonStyle::Outlined,
                on_tap: EV_STAR,
            }),
        ),
    )?;

    // 尾部推位 + 底部瞬态行（居中容器内 Text 内在宽居中；空文 = 隐藏，
    // 预留带钉页底）。
    let tail = a
        .vp_h
        .saturating_sub(pattern::TOPBAR_H)
        .saturating_sub(pad)
        .saturating_sub(center_h)
        .saturating_sub(STRIP_H);
    pattern::spacer(ui, body, 1, tail)?;
    let strip_col = ui.add(
        body,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps { gap: 0, w: Some(a.vp_w), h: None, cross: CrossAlign::Center }),
        ),
    )?;
    let calling = ui.add(
        strip_col,
        text_spec(ON_ACCENT, Some(t.brand), FontSize::S24x48, b"")?,
    )?;

    // a11y（表帽 32 内——行 10 + 铬 4 + 详情 5 ≈ 19）。
    ui.set_a11y(avatar, ROLE_TEXT, s.name)?;
    ui.set_a11y(name, ROLE_TEXT, s.name)?;
    ui.set_a11y(phone, ROLE_TEXT, s.phone)?;
    ui.set_a11y(call, ROLE_BUTTON, b"call")?;
    ui.set_a11y(star, ROLE_BUTTON, b"star")?;

    // Button 按压反馈 = 引擎 PressedVis 查表（press_map 两枚登记——
    // widget 件视觉单源在 paint 面）。
    ui.set_press_map(&[(call, call), (star, star)])?;

    // 入栈（页已挂 nav_root 下 = z 顶）+ 300ms 滑入起拍。
    a.nav.push(ui, page)?;
    a.detail_page = Some(page);
    a.detail_contact = Some(contact);
    a.detail_back = bar.back;
    a.detail_star_btn = Some(star);
    a.detail_calling = Some(calling);
    a.calling_until = None;
    Ok(())
}

/// 建树总装（main 一次调用）：NavHost 挂根 → 列表页 Stack[滚动层,
/// 铬层]（铬层后画盖上 = Scroll v1 无 clip 的滚动溢出遮盖面——行滑入
/// 铬区被不透明铬底掩没）→ 行集。Err = 容量/文本域违例 fail 收口。
fn build_all(ui: &mut UiMain) -> Result<App, UiError> {
    let (vp_w, vp_h) = ui.viewport();
    let t = Tokens::pick(Theme::Light);
    let chrome_h = pattern::TOPBAR_H + pattern::SEG_BAR_H;
    let root = ui.root();
    let mut nav = NavHost::new();
    let nav_root = nav.attach(ui, root)?;

    // 列表页 stack（常驻底页——不 push：depth 0 = 根语义）。
    let page = pattern::stack(ui, nav_root)?;
    // 滚动层（Scroll kind——Stack `_` 摆位吃满页 = 视口帽；内容 Col 顶
    // 让位 chrome_h，行卡随 offset 上移滑入铬层之下）。
    let scroll = ui.add(
        page,
        vspec(
            ViewKind::Scroll,
            Props::Scroll(ScrollProps { content_height: 0, offset_y: 0 }),
        ),
    )?;
    let content_col = ui.add(
        scroll,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps {
                gap: ROW_GAP,
                w: Some(vp_w),
                h: None,
                cross: CrossAlign::Stretch,
            }),
        ),
    )?;
    pattern::spacer(ui, content_col, 1, chrome_h)?;
    // 铬层（AppBar + Seg——后画 = 遮盖滚动溢出；缝 0 防露缝）。
    let chrome = ui.add(
        page,
        vspec(
            ViewKind::Col,
            Props::Col(ColProps {
                gap: 0,
                w: Some(vp_w),
                h: None,
                cross: CrossAlign::Stretch,
            }),
        ),
    )?;
    let _bar = pattern::app_bar(ui, chrome, &t, b"Contacts", false)?; // 根页无返回键
    let seg = pattern::seg(ui, chrome, &t, &[b"All", b"Starred"], 0)?;

    // 行表占位 NodeId（NodeId 构造子 pub(crate)——外部 crate 不能字面
    // 构造；root() 幂等取真节点充占位，rows_n=0 保证占位值永不被读，
    // build_rows 即全量覆写）。
    let ph = ui.root();
    let mut a = App {
        t,
        vp_w,
        vp_h,
        chrome_h,
        nav,
        nav_root,
        scroll,
        content_col,
        seg,
        filter_starred: false,
        row_cards: [ph; SEEDS.len()],
        row_contact: [0; SEEDS.len()],
        row_avatar: [ph; SEEDS.len()],
        rows_n: 0,
        empty_hint: None,
        stars: SEEDS.map(|s| s.star),
        content_h: 0,
        offset: 0,
        detail_page: None,
        detail_contact: None,
        detail_back: None,
        detail_star_btn: None,
        detail_calling: None,
        calling_until: None,
        pressed_row: None,
        drag_y: None,
    };
    build_rows(ui, &mut a, true)?;
    Ok(a)
}

fn main() {
    // —— 降级门：wm 客户端端未交付（GUI 通道授权未落——W4-RECON §1.2
    //    过渡形 [ui] 资产节兼判）→ 一行诊断 + park 常驻（窗口义务语义
    //    不折衷成秒退；wiring 落地后本门自然放行）。
    let hw = wm_cli_h();
    if hw == 0 {
        eprintln!("contacts: wm absent (no gui gift)");
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
        "contacts: wm v={} {}x{} input={}",
        info.version_wm,
        info.disp_w,
        info.disp_h,
        input_h()
    );

    // —— UI arena 堆置（settings 同形：UiMain 构造中间体 LTO 后部分落
    //    栈 ~29KB——8 页栈穿先例，Box::leak = 常驻无回收语义）。
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    static TEXT_METRICS: ProportionalTextMetrics = ProportionalTextMetrics;
    ui.set_text_metrics(&TEXT_METRICS);
    // glyph 真供件（程序化三面：BACK/STAR_ON/STAR_OFF——设计稿 §3.3
    // 自绘路，零位图资产增量）。
    ui.set_icon_source(&ICONS);

    // —— 建树（Err = 容量/文本域违例 → fail 收口）。
    let mut a = match build_all(ui) {
        Ok(a) => a,
        Err(e) => fail(&format!("build err={e:?}")),
    };

    // —— 帧循环（常驻义务——永不返回）。
    run(hw, input_h(), ui, &mut a);
}

/// 帧循环 + 输入 + 事件消费（settings 食谱 + 三增量：Msg 双面消费
///（行命中/拖拽滚动/Seg/返回）、NavHost tick + take_popped、瞬态行
/// 节拍自撤）。
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

    // —— 跨代清账（前代实例孤儿别名全量排空）。ST_BUSY 步进重试环：本端
    //    拉起时 launcher 常有在途 present 事务（busy 灯帧 SUBMIT→
    //    FRAME_CALLBACK→RELEASE，TCG 下 >5s 租约），wmd presarb 快答
    //    ST_BUSY——旧面单趟即 fail→park（首开不出窗根因）；BUSY 步进重发，
    //    非 BUSY 应答原样跳出交下方原判。
    let m = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
        Some(m) => m,
        None => fail("release encode"),
    };
    let mut busy_dl = 0u64;
    let (n, nh) = loop {
        let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
        if gp::parse_release_rep(&rep[..n]).is_some() || !(n == 1 && rep[0] == gp::ST_BUSY) {
            break (n, nh);
        }
        busy_step("boot release busy（present 单飞让位重试）", &mut busy_dl);
    };
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
    // commit（settings/launcher 同形）；run 永不返回——fb 终身持有。
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
    let mut calling_buf = [0u8; 64]; // 瞬态行字节 scratch

    loop {
        let t_now = now();

        // —— 真实输入（settings 同形）：先收上一轮 INPUT_POLL 应答，再
        //    挂下一轮 poll；事件逐条 push 进引擎。
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
                                    eprintln!("contacts: input push err={e:?}");
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {} // 无事件常态
                    Some(Err(st)) => eprintln!("contacts: input poll st={st}"),
                    None => eprintln!("contacts: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("contacts: input recv err={s}");
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
        //    + **Msg 双面消费**（settings 丢弃面在此展开）：行命中 =
        //    Release 几何判定（Click 无坐标面）；拖拽 = 列表滚动；
        //    Seg/返回 = Click 锚判定。
        let msgs = ui.pump_input(last_expected);
        for m in msgs.iter() {
            match *m {
                Msg::Press { x, y, .. } => {
                    // 行按压反馈（ItemState 五态换底——根页列表域）。
                    if a.detail_page.is_none() && y >= a.chrome_h {
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
                    // 行命中 → 详情入栈（非拖 up 路径——Release 与 Click
                    // 同拍成对，几何面在 Release）。
                    if a.detail_page.is_none() && y >= a.chrome_h {
                        if let Some(r) = row_at(ui, a, x, y) {
                            let contact = a.row_contact[r];
                            match build_detail(ui, a, contact) {
                                Ok(()) => ui.wake(now()),
                                Err(e) => fail(&format!("detail build err={e:?}")),
                            }
                        }
                    }
                }
                Msg::Cancel { .. } => {
                    if let Some(r) = a.pressed_row.take() {
                        tint_row(ui, a, r, false);
                    }
                    a.drag_y = None;
                }
                Msg::DragStart { y, .. } => {
                    // 拖起 = 按压视觉还原（拖路径无 Release——补还原面）。
                    if let Some(r) = a.pressed_row.take() {
                        tint_row(ui, a, r, false);
                    }
                    a.drag_y = None;
                    if a.detail_page.is_none() && y >= a.chrome_h {
                        a.drag_y = Some(y as i32);
                    }
                }
                Msg::DragMove { y, .. } => {
                    if let Some(ly) = a.drag_y {
                        let dy = ly - y as i32;
                        if dy != 0 {
                            scroll_by(ui, a, dy);
                            a.drag_y = Some(y as i32);
                            ui.wake(now());
                        }
                    }
                }
                Msg::DragEnd { .. } => {
                    a.drag_y = None;
                }
                Msg::Click { node } => {
                    // 详情页返回键（app 内手势位——系统 back 链挂 3b）。
                    if a.detail_page.is_some() && a.detail_back == Some(node) {
                        if a.nav.pop(ui) {
                            ui.wake(now());
                        }
                    }
                    // Seg 过滤切档（根页）。
                    if a.detail_page.is_none() {
                        if let Some(i) = a.seg.hit(node) {
                            let want = i == 1;
                            if want != a.filter_starred {
                                a.filter_starred = want;
                                a.seg.set_active(ui, i);
                                if let Err(e) = build_rows(ui, a, true) {
                                    fail(&format!("rows rebuild err={e:?}"));
                                }
                                ui.wake(now());
                            }
                        }
                    }
                }
                Msg::DoubleClick { .. } | Msg::LongPress { .. } => {
                    // 双击/长按（总攻批 2 引擎扩域 add-only）：v1 通讯录无
                    // 消费面——Click 照产已覆盖既有行为，语义面忽略。
                }
            }
        }

        // —— NavHost 推拉动画步进（在途 → 每帧推进 + 保活）。
        if a.nav.tick(ui, t_now) {
            ui.wake(t_now);
        }
        // —— 出栈完结页移交 remove + 行集刷新（星标态回显；保留滚动位）。
        if let Some(node) = a.nav.take_popped() {
            if let Err(e) = ui.remove(node) {
                fail(&format!("detail remove err={e:?}"));
            }
            a.detail_page = None;
            a.detail_contact = None;
            a.detail_back = None;
            a.detail_star_btn = None;
            a.detail_calling = None;
            a.calling_until = None;
            if let Err(e) = build_rows(ui, a, false) {
                fail(&format!("rows rebuild err={e:?}"));
            }
            ui.wake(t_now);
        }

        // —— 组件事件消费（Button on_tap——引擎 widget_react 已发，此处
        //    纯 app 面）：Call → 瞬态行 2s；Star → 收藏翻转 + 按钮换文。
        let evs = ui.take_events();
        for ev in evs.iter() {
            match ev.event {
                EV_CALL => {
                    if let (Some(text_node), Some(c)) = (a.detail_calling, a.detail_contact) {
                        let len = calling_bytes(SEEDS[c].name, &mut calling_buf);
                        if let Err(e) = ui.set_text(text_node, &calling_buf[..len]) {
                            eprintln!("contacts: calling set err={e:?}");
                        }
                        a.calling_until = Some(now() + CALLING_NS);
                        ui.wake(now());
                    }
                }
                EV_STAR => {
                    if let Some(c) = a.detail_contact {
                        a.stars[c] = !a.stars[c];
                        if let Some(btn) = a.detail_star_btn {
                            let label: &[u8] = if a.stars[c] { b"Unstar" } else { b"Star" };
                            let text = match TextBuf::new(label) {
                                Ok(tb) => tb,
                                Err(e) => {
                                    eprintln!("contacts: star label err={e:?}");
                                    continue;
                                }
                            };
                            let props = Props::Button(ButtonProps {
                                label: text,
                                style: ButtonStyle::Outlined,
                                on_tap: EV_STAR,
                            });
                            if let Err(e) = ui.set_props(btn, props) {
                                eprintln!("contacts: star set err={e:?}");
                            }
                        }
                        ui.wake(now());
                    }
                }
                _ => {}
            }
        }

        // —— 瞬态行节拍自撤（Toast 2s 驻留的帧循环驱动面）。
        if let Some(dl) = a.calling_until {
            if t_now >= dl {
                a.calling_until = None;
                if let Some(text_node) = a.detail_calling {
                    if let Err(e) = ui.set_text(text_node, b"") {
                        eprintln!("contacts: calling clear err={e:?}");
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
                // 增量帧：Load+Store + K 矩形并集包络。
                PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(damage_envelope(&dmg)))
            };
            let dl = enc.finish();
            if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
                fail(&format!("raster submit {e:?}"));
            }

            // SUBMIT：fresh READ dup 别名随行（send 即 take；TRANSFER 保位）。
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
            // ST_BUSY 步进重试环（berry-appshell pump.rs SUBMIT 面同形）：他
            // 客户端（launcher 点击/分钟沿整帧）事务在途时 wmd presarb 快答
            // ST_BUSY 并 close 随行别名——每轮重发前 fresh dup 一枚新别名。
            let mut busy_dl = 0u64;
            let token = loop {
                let mut alias: Handle = 0;
                if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0
                    || alias == 0
                {
                    fail("dup alias");
                }
                let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
                if let Some(t) = gp::parse_submit_ack(&rep[..n]) {
                    break t;
                }
                if n == 1 && rep[0] == gp::ST_BUSY {
                    busy_step("submit busy（present 单飞让位重试）", &mut busy_dl);
                    continue;
                }
                fail(&format!("submit ack bad st={}", rep[0]));
            };
            ui.note_submitted();

            // FRAME_CALLBACK：断 PRESENTED（呈现链破相 fail loud）。ST_BUSY
            // 步进重试同 SUBMIT（pump.rs 同形——stale 清场封锁瞬态快答，本端
            // 帧已在途，重试零副作用）。
            let m = match gp::encode_present_req(
                &mut req,
                &gp::PresentReq::FrameCallback { frame_token: token },
            ) {
                Some(m) => m,
                None => fail("frame callback encode"),
            };
            let mut cb_dl = 0u64;
            let done = loop {
                let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
                if let Some(d) = gp::parse_frame_done_rep(&rep[..n]) {
                    break d;
                }
                if n == 1 && rep[0] == gp::ST_BUSY {
                    busy_step("frame callback busy（present 单飞让位重试）", &mut cb_dl);
                    continue;
                }
                fail(&format!("frame done bad st={}", rep[0]));
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
                println!("contacts: first frame");
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

        // —— 稳态等待：deadline = min(next_wakeup, IDLE 帽, 瞬态行自撤
        //    期限)——三源取早（引擎 Idle 相无定时器，app 面节拍补源）。
        let cap = now() + IDLE_CAP_NS;
        let mut deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
        if let Some(dl) = a.calling_until {
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
