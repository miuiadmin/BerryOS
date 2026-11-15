//! browser —— 波 4 第四件真 GUI 应用（BROWSER appstub 秒退壳替换；
//! `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.7，侦察/
//! 对账底册 = worktree 根 `W4-RECON.md`）。
//!
//! **页面树**（pattern 模式件消费——波 4 首批真实消费者之一）：
//!
//! - 书签页（根，NavHost depth 0）：`pattern::top_bar`(Browser) + 上次
//!   地址行（BOOKS.JSN 回显）+ 两列书签卡片网格（格 = `pattern::card`
//!   配方 elevation 1 + 程序化 globe glyph 前导图标 + 站名/地址双行）。
//! - 页页（push，NavHost 300ms 右滑入）：`pattern::app_bar`(站名,
//!   back=true → pop 回书签) + 尾随刷新 glyph（bar 行补挂——top_bar
//!   配方无返回位的补偿组合）+ 地址行（Input 值面 = placeholder 承载
//!   + Go/Stop Button）+ Progress(Linear) 模拟加载（时间基 permille
//!   帧驱动爬升，满后揭页体）+ 程序化页体（标题 + 段落 + 链接行，
//!   全 ASCII 本地模拟页）。
//! - 历史 v1 单级：页内链接点按**就地换页**（同栈位重启加载，不叠
//!   NavStack）——back 恒回书签；多级历史链挂终态（3b back 路由）。
//!
//! **铁律执法**：卡片 = `pattern::card` 配方（Card kind 自绘面/圆角/
//! 阴影，非手写 Rect 拼卡）；按压反馈色单源 = `pattern::ItemState` 五
//! 态预混表（files 批同款 Stack 面层 Rect 叠层习语——Normal = 卡面白
//! 常态隐形，Pressed = brand 20% 预混白）；Go/Stop 按压 = 引擎
//! press_map（Button 在 visual 白名单）；文案全 ASCII；帧循环零分配。
//!
//! **链形态** = settings crate 镜像（W4-RECON §1 先例）：wm 门控
//! （本 app 带 fs 词 → wm 礼物落 extras[2]，经同槽访问器
//! `berry_libc::io::wm_present_h` 取用——settings（无 fs 词）落 [0]
//! 走 `wm_cli_h`，两形分野单源 = svcmgr GUI 授权链 5452649af；0 = 无
//! GUI 礼物 → 一行诊断 + park 常驻）→ WM_GETINFO → present 三部曲
//! → 帧 VMO 三连 → INPUT_POLL 批拉 + `pump_input`（referee +
//! `widget_react` 同拍）→ `render_frame` damage 驱动零空转。RPC 半程族
//! （`rpc_send`/`rpc_recv`/`rpc`）与 `damage_envelope`/`input_arm` 均
//! launcher/settings 同形拷贝。
//!
//! **持久化**：BOOKS.JSN（FAT 数据盘根，launcher layout_io 食谱自实现
//! ——appshell::conf 未落，W4-RECON §四降解）：`{"schema":1,"last":N}`
//! 恰 21B 单簇近原子（N = 上次到访书签序，9 = 无）；三约束纪律继承
//! （缺席回默认 + 坏读自愈回写 + 写失败非致命 eprintln 常驻不死）；
//! `fs_h()==0` 结构性早退零 RPC。书签集 = 内置静态种子（v1 无编辑面，
//! 「书签集真存」降解为集序回显——差异清单在册）。
//!
//! **诚实边界**：无 net 面——页体为本地程序化模拟页（状态行如实
//! "simulated page - no net"），Loading/Page 词汇如实；无 softime——
//! 地址行不可敲（Input 值面经 placeholder 承载），换址走链接点按。
//!
//! **v1 降解**（3b/波 2 未落面的就地替代，全清单见 W4-IMPL-BROWSER.md）：
//! 无 wm back 路由 → 返回 = 页页内 app_bar 返回 glyph 手势位；无系统
//! 优雅退出（根页无退出路径——settings park 先例同形）；theme 样式族
//! （card_style/oh_press_bg/OH_PRESS_SCALE_MIDDLE_PERMIL=950）未落 →
//! ItemState 五态表等价面；无 alpha 通道 → 按压为整面换底非半透遮罩。

use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::fs::{open, O_CREAT, O_RDONLY, O_WRONLY};
use berry_libc::io::{close, fs_h, input_h, read, self_vmar_h, wm_present_h, write};
use berry_libc::{get_errno, park, EIO, ENOENT};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_many, berry_object_wait_one, berry_vmar_map,
    berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_ui::desc::{
    ButtonProps, ButtonStyle, ColProps, GridProps, IconProps, InputKind, InputProps, Msg, NodeId,
    ProgressProps, ProgressStyle, Props, RectProps, RowProps, TextBuf, TextProps, TextStyle,
    ViewKind, ViewSpec, PERMILLE_MAX,
};
use berry_ui::layout::CrossAlign;
use berry_ui::pattern::{self, icons, ItemState, NavHost, APP_BAR_BACK, ICON_SZ, LISTITEM_H,
    LISTITEM_PAD_X};
use berry_ui::theme::{self, Theme, Tokens};
use berry_ui::{damage, frame, hit, metrics, UiError, UiMain, ROLE_BUTTON, ROLE_LIST_ITEM,
    ROLE_TEXT};

// 链接种子（fsmgr/guifbd/berry-launcher/settings 同款）：rlib/LTO 按需保留
// ——无引用则 berry-libc 的 crt0（含 `_start`）不进链接。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（launcher/settings 同值 300s：app 拉起时点 =
/// 用户点击，首 RPC 距 boot 稳态可远）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 帧缓冲映射 VA（launcher frameloop 同位：mmap VA 窗
/// [0x5000_0000,0x6000_0000) 内固定位 +16MiB；页对齐）。
const FRAME_VA: usize = 0x5100_0000;

/// 默认 vsync 周期（60Hz；present GETINFO 无周期域——launcher 同值）。
const VSYNC_NS: u64 = 16_666_666;

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// glyph scratch 容量（堆置——paint 对超容 run 截断尾字形）。v1 无 clip：
/// 页页叠书签页期**全树绘制**（后画盖上），单帧文案峰值 = 书签页 ~132
///（顶栏 7 + 上次行 17 + 六卡 × 站名/地址 ~108）+ 页页已载 ~156（顶栏 5
/// + 地址 13 + Done 4 + 页体 ~134）≈ 290——384 留三分之一余量。
const SCRATCH_GLYPHS: usize = 384;

// —— 应用几何（px @3x；pattern 常量族之外的拟定位，单源本块）——
/// 页面水平留白（16vp）。
const SIDE_PAD: u32 = 48;
/// 书签网格列数（两列卡片网格）。
const GRID_COLS: u32 = 2;
/// 网格行列缝（8vp）。
const GRID_GAP: u32 = 24;
/// 卡内容与图标缝（8vp）。
const CARD_GAP: u32 = 24;
/// 前导图标在席的最小格宽（窄视口防御：图标隐藏防内容溢出）。
const CARD_ICON_MIN_W: u32 = 336;
/// 地址行高（32vp）。
const ADDR_ROW_H: u32 = 96;
/// 进度行高。
const PROG_ROW_H: u32 = 64;
/// 页体行缝（8vp）。
const BODY_GAP: u32 = 24;
/// 页体首行让位（呼吸位）。
const BODY_LEAD: u32 = 48;
/// 模拟加载时长（1.2s——进度爬升可判且不拖沓）。
const LOAD_DUR_NS: u64 = 1_200_000_000;
/// 地址行输入帽（数据层约束面——值域执法挂输入焦点协议）。
const ADDR_MAX: u32 = 48;

// —— 事件 ID 面（props on_tap 携载——take_events 消费锚）——
/// Go/Stop 按钮（加载中 = Stop 定格揭页；已载 = Go 重载）。
const EV_GO: u32 = 1;

// ---------------------------------------------------------------------------
// —— 模拟站点表（静态书签集 + 页面内容——全 ASCII、行 ≤ 32B、链接目标
//    域内自洽（单测钉板））——

/// 页体行（段落 / 链接——链接点按就地换页）。
#[derive(Debug, Clone, Copy)]
enum PLine {
    Para(&'static [u8]),
    Link { label: &'static [u8], site: usize, page: usize },
}

/// 单页（地址 + 标题 + 四行：两段落 + 两链接）。
#[derive(Debug, Clone, Copy)]
struct PPage {
    addr: &'static [u8],
    title: &'static [u8],
    lines: [PLine; 4],
}

/// 站点（书签卡面 = name + home addr + globe glyph）。
#[derive(Debug, Clone, Copy)]
struct Site {
    name: &'static [u8],
    pages: [PPage; 2],
}

/// 六站书签集（两列网格 × 三行——恰一屏纳）。
const SITES: [Site; 6] = [
    Site {
        name: b"News",
        pages: [
            PPage {
                addr: b"news.berry",
                title: b"Berry News",
                lines: [
                    PLine::Para(b"Top story: wave 4 apps land"),
                    PLine::Para(b"Weather: light rain at 18C"),
                    PLine::Link { label: b"> Sports recap", site: 0, page: 1 },
                    PLine::Link { label: b"> wiki.berry", site: 1, page: 0 },
                ],
            },
            PPage {
                addr: b"news.berry/s",
                title: b"Sports",
                lines: [
                    PLine::Para(b"Berries win the cup 3-1"),
                    PLine::Para(b"Next match is on friday"),
                    PLine::Link { label: b"> Front page", site: 0, page: 0 },
                    PLine::Link { label: b"> forum.berry", site: 5, page: 0 },
                ],
            },
        ],
    },
    Site {
        name: b"Wiki",
        pages: [
            PPage {
                addr: b"wiki.berry",
                title: b"Berry Wiki",
                lines: [
                    PLine::Para(b"BerryOS is a small OS in Rust"),
                    PLine::Para(b"This page is simulated locally"),
                    PLine::Link { label: b"> History of BerryOS", site: 1, page: 1 },
                    PLine::Link { label: b"> docs.berry", site: 4, page: 0 },
                ],
            },
            PPage {
                addr: b"wiki.berry/h",
                title: b"History",
                lines: [
                    PLine::Para(b"Wave 1: kernel and boot base"),
                    PLine::Para(b"Wave 4: app refresh lands"),
                    PLine::Link { label: b"> Main page", site: 1, page: 0 },
                    PLine::Link { label: b"> news.berry", site: 0, page: 0 },
                ],
            },
        ],
    },
    Site {
        name: b"Mail",
        pages: [
            PPage {
                addr: b"mail.berry",
                title: b"Berry Mail",
                lines: [
                    PLine::Para(b"Inbox: 3 unread messages"),
                    PLine::Para(b"Quota: 42 of 64 KiB used"),
                    PLine::Link { label: b"> Drafts folder", site: 2, page: 1 },
                    PLine::Link { label: b"> docs.berry", site: 4, page: 0 },
                ],
            },
            PPage {
                addr: b"mail.berry/d",
                title: b"Drafts",
                lines: [
                    PLine::Para(b"1 draft: hello team"),
                    PLine::Para(b"No pending sends"),
                    PLine::Link { label: b"> Inbox", site: 2, page: 0 },
                    PLine::Link { label: b"> forum.berry", site: 5, page: 0 },
                ],
            },
        ],
    },
    Site {
        name: b"Shop",
        pages: [
            PPage {
                addr: b"shop.berry",
                title: b"Berry Shop",
                lines: [
                    PLine::Para(b"Deal: keyboard at 12 berry"),
                    PLine::Para(b"Cart: 1 item ready"),
                    PLine::Link { label: b"> Order list", site: 3, page: 1 },
                    PLine::Link { label: b"> mail.berry", site: 2, page: 0 },
                ],
            },
            PPage {
                addr: b"shop.berry/o",
                title: b"Orders",
                lines: [
                    PLine::Para(b"Order 77: shipped"),
                    PLine::Para(b"Order 78: packing"),
                    PLine::Link { label: b"> Shop front", site: 3, page: 0 },
                    PLine::Link { label: b"> wiki.berry", site: 1, page: 0 },
                ],
            },
        ],
    },
    Site {
        name: b"Docs",
        pages: [
            PPage {
                addr: b"docs.berry",
                title: b"Berry Docs",
                lines: [
                    PLine::Para(b"Guides: build, flash, test"),
                    PLine::Para(b"API refs: 41 ui kinds"),
                    PLine::Link { label: b"> Quickstart", site: 4, page: 1 },
                    PLine::Link { label: b"> wiki.berry", site: 1, page: 0 },
                ],
            },
            PPage {
                addr: b"docs.berry/q",
                title: b"Quickstart",
                lines: [
                    PLine::Para(b"Step 1: cargo build"),
                    PLine::Para(b"Step 2: just image"),
                    PLine::Link { label: b"> Docs home", site: 4, page: 0 },
                    PLine::Link { label: b"> news.berry", site: 0, page: 0 },
                ],
            },
        ],
    },
    Site {
        name: b"Forum",
        pages: [
            PPage {
                addr: b"forum.berry",
                title: b"Berry Forum",
                lines: [
                    PLine::Para(b"Threads: 128, posts: 4096"),
                    PLine::Para(b"Hot thread: best berry pie?"),
                    PLine::Link { label: b"> Pie thread", site: 5, page: 1 },
                    PLine::Link { label: b"> shop.berry", site: 3, page: 0 },
                ],
            },
            PPage {
                addr: b"forum.berry/p",
                title: b"Pie thread",
                lines: [
                    PLine::Para(b"Use 2 cups of berries"),
                    PLine::Para(b"Bake 40 min at 180C"),
                    PLine::Link { label: b"> Forum home", site: 5, page: 0 },
                    PLine::Link { label: b"> docs.berry", site: 4, page: 0 },
                ],
            },
        ],
    },
];

/// 页体行数（表结构常量——两段落 + 两链接）。
const LINE_N: usize = 4;

// ---------------------------------------------------------------------------
// —— 应用内功能 glyph（程序化绘制——设计稿 §3.3 自绘路，contacts 批
//    先例同形）：IconSource 真供件三面，Argb 1-bit 透明键（A==0 跳过
//    ——raster argb_blit 同律），色值全部 theme token 单源打包。源 24px，
//    目标按 w/h 三倍整数最近邻（硬边无走样）。

/// glyph 源边长（px）。
const GLYPH_N: usize = 24;

/// 书签卡前导 globe glyph id（自域 0x02xx——pattern::icons 约定域外）。
const GLOBE: u16 = 0x0220;

/// Color → RGB 打包（const 面——theme 常量进 const 求值）。
const fn pack_rgb(c: Color) -> u32 {
    ((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32
}

/// 点到线段垂距 ≤ r（整数：cross² ≤ r²·len² 且投影落在段内；contacts
/// 批同源——端点退化按投影门拒，glyph 光栅足够）。
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

/// 返回箭头「<」位图（两臂线段 r=3px——contacts 批同源同形）。
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

/// 刷新箭头位图：环形（r ∈ [7,10]、心 (12,12)）+ 东北向缺口（dx-dy>6
/// 的半平面楔）+ 缺口内「>」箭头尖（两臂线段 r≈2.5px）——环流入尖，
/// 圆箭头语义。
const fn build_refresh(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    let r2 = 6; // r≈2.5px（箭头臂较细——与环带宽近似）
    let mut px = [0u32; GLYPH_N * GLYPH_N];
    let mut i = 0usize;
    while i < GLYPH_N * GLYPH_N {
        let x = (i % GLYPH_N) as i32;
        let y = (i / GLYPH_N) as i32;
        let (dx, dy) = (x - 12, y - 12);
        let d2 = dx * dx + dy * dy;
        let ring = d2 >= 49 && d2 <= 100 && dx - dy <= 6;
        let head = near_seg(x, y, 15, 7, 20, 12, r2) || near_seg(x, y, 20, 12, 15, 17, r2);
        px[i] = if ring || head { 0xFF00_0000 | rgb } else { 0 };
        i += 1;
    }
    px
}

/// 线框地球位图：外环（r ∈ [7,10]）+ 赤道横线 + 中央经线（±10 域内）
/// ——globe 语义（书签卡前导图标）。
const fn build_globe(rgb: u32) -> [u32; GLYPH_N * GLYPH_N] {
    let mut px = [0u32; GLYPH_N * GLYPH_N];
    let mut i = 0usize;
    while i < GLYPH_N * GLYPH_N {
        let x = (i % GLYPH_N) as i32;
        let y = (i / GLYPH_N) as i32;
        let (dx, dy) = (x - 12, y - 12);
        let d2 = dx * dx + dy * dy;
        let ring = d2 >= 49 && d2 <= 100;
        let equator = dy == 0 && dx.abs() <= 10;
        let meridian = dx == 0 && dy.abs() <= 10;
        px[i] = if ring || equator || meridian { 0xFF00_0000 | rgb } else { 0 };
        i += 1;
    }
    px
}

/// glyph 表（const 求值落 rodata——零堆零运行期构造；BACK/REFRESH =
/// 前景字色，GLOBE = brand 强调色）。
static BACK_PX: [u32; GLYPH_N * GLYPH_N] = build_back(pack_rgb(theme::FONT_PRIMARY));
static REFRESH_PX: [u32; GLYPH_N * GLYPH_N] = build_refresh(pack_rgb(theme::FONT_PRIMARY));
static GLOBE_PX: [u32; GLYPH_N * GLYPH_N] = build_globe(pack_rgb(theme::BRAND));

/// 本应用 IconSource 真供件（berry-ui D28 供给槽——app 侧只用注册域
/// 内 id，域外值空绘不炸；BACK/REFRESH 用 pattern::icons 约定域值，
/// GLOBE 自域）。
struct BrowserIcons;

static ICONS: BrowserIcons = BrowserIcons;

impl metrics::IconSource for BrowserIcons {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        match id {
            icons::BACK | icons::REFRESH | GLOBE => Some((GLYPH_N as u32, GLYPH_N as u32)),
            _ => None,
        }
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        match id {
            icons::BACK => Some(&BACK_PX),
            icons::REFRESH => Some(&REFRESH_PX),
            GLOBE => Some(&GLOBE_PX),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// —— BOOKS.JSN 持久化（launcher layout_io 食谱：三约束 + 单簇近原子 +
//    严格串行单 RPC 会话 + close 即 commit）——

/// 卷根持久化文件（8.3 大写——fsmgr 单 FAT 卷根；CWD 默认根）。
const JSN_PATH: &[u8] = b"BOOKS.JSN\0";

/// JSN 读写帽（合法形恰 21B；>32 长读即坏）。
const JSN_MAX: usize = 32;

/// 「无上次到访」哨兵值（digit 域 9——站点序恒 < 6）。
const LAST_NONE: u8 = 9;

/// canonical 编码：`{"schema":1,"last":N}`（恰 21B——N = 站序 0..=5 或
/// 9 = 无）。返回字节数。
fn jsn_encode(last: Option<usize>, out: &mut [u8; JSN_MAX]) -> usize {
    const HEAD: &[u8] = b"{\"schema\":1,\"last\":";
    let d = match last {
        Some(s) => s as u8,
        None => LAST_NONE,
    };
    out[..HEAD.len()].copy_from_slice(HEAD);
    out[HEAD.len()] = b'0' + d;
    out[HEAD.len() + 1] = b'}';
    HEAD.len() + 2
}

/// 严格解析（长度/前缀/数字域任一不符 = None——调用方自愈重写）。
fn jsn_parse(b: &[u8]) -> Option<Option<usize>> {
    const HEAD: &[u8] = b"{\"schema\":1,\"last\":";
    if b.len() != HEAD.len() + 2 || &b[..HEAD.len()] != HEAD || b[b.len() - 1] != b'}' {
        return None;
    }
    let d = b[HEAD.len()];
    if !d.is_ascii_digit() {
        return None;
    }
    Some(if d == b'0' + LAST_NONE { None } else { Some((d - b'0') as usize) })
}

/// 读结局（layout_io `ReadOutcome` 同形精简）。
enum JsnRead {
    Absent,
    Bytes([u8; JSN_MAX], usize),
    Bad(&'static str),
}

/// 全量读（open(O_RDONLY) → read 循环 → close；帽 JSN_MAX——长读即 Bad）。
fn jsn_read() -> JsnRead {
    // SAFETY: JSN_PATH 是 NUL 结尾字面量；fd 按 libc 契约使用。
    let fd = unsafe { open(JSN_PATH.as_ptr() as *const _, O_RDONLY) };
    if fd < 0 {
        return match get_errno() {
            ENOENT => JsnRead::Absent,
            _ => JsnRead::Bad("read errno"),
        };
    }
    let mut buf = [0u8; JSN_MAX];
    let mut n = 0usize;
    let mut chunk = [0u8; 64];
    loop {
        let r = read(fd, chunk.as_mut_ptr() as *mut _, chunk.len());
        if r < 0 {
            close(fd);
            return JsnRead::Bad("read errno");
        }
        if r == 0 {
            break; // EOF
        }
        let r = r as usize;
        if n + r > JSN_MAX {
            close(fd);
            return JsnRead::Bad("too big");
        }
        buf[n..n + r].copy_from_slice(&chunk[..r]);
        n += r;
    }
    close(fd); // 只读 fd 的 close 无 commit 面
    JsnRead::Bytes(buf, n)
}

/// 全量写（open(O_WRONLY|O_CREAT) → write 循环 → close=commit——TRUNC
/// 游标两段式；中途失败仍 close，POSIX close 恒消费）。
fn jsn_put(data: &[u8]) -> Result<(), i32> {
    // SAFETY: JSN_PATH NUL 结尾；fd 按 libc 契约使用。
    let fd = unsafe { open(JSN_PATH.as_ptr() as *const _, O_WRONLY | O_CREAT) };
    if fd < 0 {
        return Err(get_errno());
    }
    let mut off = 0usize;
    while off < data.len() {
        let r = write(fd, data[off..].as_ptr() as *const _, data.len() - off);
        if r < 0 {
            let e = get_errno();
            close(fd); // 部分 commit（layout_io 文档化取舍同款）
            return Err(e);
        }
        if r == 0 {
            close(fd);
            return Err(EIO); // 零写非 EOF = 协议破缺面
        }
        off += r as usize;
    }
    if close(fd) != 0 {
        return Err(get_errno());
    }
    Ok(())
}

/// 持久化收口（写失败非致命——eprintln 后照用内存值常驻不死）。
fn books_save(last: Option<usize>) {
    if fs_h() == 0 {
        return; // 无 fs 词授权：结构性零 RPC（内存态照用）
    }
    let mut out = [0u8; JSN_MAX];
    let n = jsn_encode(last, &mut out);
    if let Err(e) = jsn_put(&out[..n]) {
        eprintln!("browser: jsn write errno={e}");
    }
}

/// 启动期读入（缺席/坏读自愈：回默认 + 重写；缺席常态零打印）。
fn books_load() -> Option<usize> {
    let d = None;
    if fs_h() == 0 {
        return d;
    }
    match jsn_read() {
        JsnRead::Absent => {
            books_save(d);
            d
        }
        JsnRead::Bytes(b, n) => match jsn_parse(&b[..n]) {
            Some(v) if v.is_none_or(|s| s < SITES.len()) => v,
            _ => {
                eprintln!("browser: jsn bad -- default");
                books_save(d);
                d
            }
        },
        JsnRead::Bad(why) => {
            eprintln!("browser: jsn {why} -- default");
            books_save(d);
            d
        }
    }
}

// ---------------------------------------------------------------------------
// —— RPC 半程族（launcher/settings 同形拷贝）——

/// 诊断 + park 收口（app 常驻不被 reap；崩溃监督归 svcmgr
/// app_supervise 域，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("browser: {what}");
    park()
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

/// 严格串行 RPC **发送半程**：-EBUSY → 阻塞等 WRITABLE（帽重臂）；
/// send 原子性——EBUSY/错码时随行 handle 零迁移。send 即 take。
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

/// 严格串行 RPC **收侧半程**：空队 -EAGAIN → 阻塞等 READABLE（帽）；
/// 每轮重置容量（in-out 出参——漏重置 = 恒 -EINVAL 死循环）。
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

// ---------------------------------------------------------------------------
// —— 建树小件 ——

/// 单节点 spec 构造（key=0——调和键路径挂账 v1.5，pattern 同形）。
fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

/// Text 叶 spec（超 TEXT_INLINE=64B loud Err——本应用文案 ≤ 32B 域内）。
fn text_spec(fg: Color, bg: Option<Color>, scale: FontSize, bytes: &[u8]) -> Result<ViewSpec, UiError> {
    Ok(vspec(
        ViewKind::Text,
        Props::Text(TextProps {
            style: TextStyle { fg, bg, scale },
            text: TextBuf::new(bytes)?,
        }),
    ))
}

/// Icon 叶 spec（显式 w/h——空供件下几何不塌；pattern icon_spec 同形）。
fn icon_spec(id: u16, fg: Color, w: u32, h: u32) -> ViewSpec {
    vspec(ViewKind::Icon, Props::Icon(IconProps { id, fg, w: Some(w), h: Some(h) }))
}

/// ItemState 五态底色 → Color（预混表 u32 拆包——值单源 pattern.rs）。
fn state_color(v: u32) -> Color {
    Color { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 }
}

/// 尾接辅助（零分配文案装配）。
fn put(out: &mut [u8], k: &mut usize, s: &[u8]) {
    out[*k..*k + s.len()].copy_from_slice(s);
    *k += s.len();
}

/// 「Loading n%」文案（permille → 0..=100 百分数；满格 100 三位档；
/// ≤ 13B）。
fn pct_bytes(pm: u32, out: &mut [u8; 16]) -> usize {
    let mut k = 0usize;
    put(out, &mut k, b"Loading ");
    let pct = (pm / 10).min(100);
    if pct >= 100 {
        put(out, &mut k, b"100");
    } else {
        if pct >= 10 {
            out[k] = b'0' + (pct / 10) as u8;
            k += 1;
        }
        out[k] = b'0' + (pct % 10) as u8;
        k += 1;
    }
    put(out, &mut k, b"%");
    k
}

/// 「Last: <addr>」文案（None = 空文隐藏；≤ 24B）。
fn last_bytes(last: Option<usize>, out: &mut [u8; 32]) -> usize {
    let Some(s) = last else { return 0 };
    let mut k = 0usize;
    put(out, &mut k, b"Last: ");
    put(out, &mut k, SITES[s].pages[0].addr);
    k
}

// ---------------------------------------------------------------------------
// —— 应用状态（建树一次落定 + 运行期翻转面）——

/// 书签卡注册（命中域 = 卡 Stack 界（Card 子树全幅）；按压层 = 面层
/// Rect——ItemState 五态换底）。
#[derive(Debug, Clone, Copy)]
struct CardReg {
    /// 卡叠放根（bounds 命中域 + remove 锚）。
    stack: NodeId,
    /// 按压面层 Rect（Normal = 卡面白常态隐形 / Pressed = brand 20% 预混）。
    press: NodeId,
    /// 站点序。
    site: usize,
}

/// 页页运行态（open_page 落位 / pop 经 NavHost `take_popped` 移交
/// remove——contacts 详情页同形的复用形；全字段 Copy——帧循环消费免借
/// 用舞）。
#[derive(Debug, Clone, Copy)]
struct PageView {
    /// app_bar 返回 glyph（Click 锚 → pop 回书签）。
    back: NodeId,
    /// 尾随刷新 glyph（Click 锚 → 重载当前页）。
    refresh: NodeId,
    /// 地址行 Input（值面 = placeholder 承载——无输入焦点协议）。
    addr: NodeId,
    /// Go/Stop 按钮（label 随加载态换文；EV_GO 事件锚）。
    go: NodeId,
    /// 进度条（Progress kind——permille 值面 set_props 步进）。
    prog: NodeId,
    /// 进度行文案（"Loading n%" / "Done"）。
    pct: NodeId,
    /// 页体锚（Col cross=Center——子树整体重建面）。
    body: NodeId,
    /// 现行页体子树根（rebuild 先 remove）。
    body_child: Option<NodeId>,
    /// 当前站点序。
    site: usize,
    /// 当前页序（站内 0/1）。
    page: usize,
    /// 加载相（true = Loading 爬升中 / false = Loaded 页体在席）。
    loading: bool,
    /// 加载起拍（时间基 permille = elapsed/1.2ms）。
    load_start: u64,
    /// 上次已落屏 permille（幂等门——省 set_props 空转）。
    permille: u32,
    /// 页内链接锚表（Loaded 页体的可点行——(Text 节点, 目标站, 目标页)）。
    links: [Option<(NodeId, usize, usize)>; LINE_N],
}

/// 应用运行态。
struct App {
    /// 主题 token 组（light 档——全页配色单源）。
    t: Tokens,
    /// 视口宽（px）。
    vp_w: u32,
    /// 视口高（px）。
    vp_h: u32,
    /// 页栈状态机（NavHost——页页 push/pop 300ms 推拉；书签页 = depth 0
    /// 常驻底页）。
    nav: NavHost,
    /// NavHost 栈容器（页挂载锚）。
    nav_root: NodeId,
    /// 上次地址行 Text（BOOKS.JSN 回显——空文隐藏）。
    last_text: NodeId,
    /// 书签卡表。
    cards: [Option<CardReg>; SITES.len()],
    /// 页页（Some = 在栈/在过渡——书签命中门控）。
    page: Option<PageView>,
    /// 上次到访站（BOOKS.JSN 持久面）。
    last: Option<usize>,
    /// 按压中书签卡（Press 置 / Release·Cancel·DragStart 还原）。
    pressed: Option<usize>,
}

/// 几何命中：坐标 → 书签卡站序（`ui.bounds` 卡叠放界包含判定——Card
/// 子树全幅，卡内任意节点命中都落本界；Click 无坐标面故走 Release
/// 判定；站序取 `CardReg.site` 单源——非表位假设）。
fn card_at(ui: &mut UiMain, a: &App, x: u32, y: u32) -> Option<usize> {
    for c in a.cards.iter().flatten() {
        if let Ok(b) = ui.bounds(c.stack) {
            let (x, y) = (x as i64, y as i64);
            if x >= b.x as i64
                && y >= b.y as i64
                && x < b.x as i64 + b.w as i64
                && y < b.y as i64 + b.h as i64
            {
                return Some(c.site);
            }
        }
    }
    None
}

/// 卡按压换底（ItemState 五态预混表单源——Normal = 卡面白还原 /
/// Pressed = brand 20% 预混白；files 批行按压同形）。
fn card_tint(ui: &mut UiMain, a: &App, idx: usize, on: bool) {
    let Some(c) = a.cards[idx] else { return };
    let st = if on { ItemState::Pressed } else { ItemState::Normal };
    let props = Props::Rect(RectProps { bg: state_color(st.bg()), w: None, h: None });
    if let Err(e) = ui.set_props(c.press, props) {
        eprintln!("browser: card press err={e:?}");
    }
}

/// 上次地址行回显（last 变更点同拍调用——open/navigate）。
fn refresh_last(ui: &mut UiMain, a: &App) {
    let mut buf = [0u8; 32];
    let n = last_bytes(a.last, &mut buf);
    if let Err(e) = ui.set_text(a.last_text, &buf[..n]) {
        eprintln!("browser: last set err={e:?}");
    }
}

/// Go/Stop 按钮 label 换文（加载态双档——contacts Star 按钮同形 set_props）。
fn set_go_label(ui: &mut UiMain, pv: &PageView, loading: bool) -> Result<(), UiError> {
    let label: &[u8] = if loading { b"Stop" } else { b"Go" };
    ui.set_props(
        pv.go,
        Props::Button(ButtonProps { label: TextBuf::new(label)?, style: ButtonStyle::Filled, on_tap: EV_GO }),
    )
}

/// 页体重建（Loading/Loaded 两态——旧子树整 retire 后按当前相挂新体；
/// Loaded 相链接行入 `links` 锚表）。
fn rebuild_body(ui: &mut UiMain, a: &mut App) -> Result<(), UiError> {
    let t = a.t;
    let vp_w = a.vp_w;
    let (site, page, loading) = match a.page.as_mut() {
        Some(pv) => {
            if let Some(old) = pv.body_child.take() {
                ui.remove(old)?;
            }
            pv.links = [None; LINE_N];
            (pv.site, pv.page, pv.loading)
        }
        None => return Ok(()),
    };
    let pv = a.page.as_mut().expect("page just matched");
    let body = pv.body;
    let content_w = vp_w.saturating_sub(2 * SIDE_PAD);

    let inner = ui.add(
        body,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: BODY_GAP,
            w: Some(content_w),
            h: None,
            cross: CrossAlign::Start,
        })),
    )?;
    if loading {
        // Loading 相：占位文案 + 诚实标注（页体满格后由 finish_load 揭）。
        pattern::spacer(ui, inner, 1, BODY_LEAD)?;
        ui.add(inner, text_spec(t.font_secondary, None, FontSize::S24x48, b"Loading page...")?)?;
        ui.add(inner, text_spec(t.font_secondary, None, FontSize::S16x32, b"local page - simulated net")?)?;
    } else {
        // Loaded 相：页标题 + 行集（段落次级色 / 链接 brand 色 + "> " 前
        // 缀）+ 底部诚实标注。
        let pg = &SITES[site].pages[page];
        pattern::spacer(ui, inner, 1, BODY_LEAD)?;
        let title = ui.add(inner, text_spec(t.font_primary, None, FontSize::S32x48, pg.title)?)?;
        let _ = ui.set_a11y(title, ROLE_TEXT, pg.title);
        let mut li = 0usize;
        for line in pg.lines.iter() {
            match *line {
                PLine::Para(bytes) => {
                    ui.add(inner, text_spec(t.font_secondary, None, FontSize::S16x32, bytes)?)?;
                }
                PLine::Link { label, site: ts, page: tp } => {
                    let node = ui.add(inner, text_spec(t.brand, None, FontSize::S24x48, label)?)?;
                    let _ = ui.set_a11y(node, ROLE_BUTTON, label);
                    pv.links[li] = Some((node, ts, tp));
                    li += 1;
                }
            }
        }
        ui.add(inner, text_spec(t.font_secondary, None, FontSize::S16x32, b"simulated page - no net")?)?;
    }
    pv.body_child = Some(inner);
    Ok(())
}

/// 加载完成定格（progress=1000 + 页体揭载 + Go 档换文；Stop 按钮/
/// 爬升到期两消费点）。
fn finish_load(ui: &mut UiMain, a: &mut App) -> Result<(), UiError> {
    let Some(pv) = a.page.as_mut() else { return Ok(()) };
    if !pv.loading {
        return Ok(());
    }
    pv.loading = false;
    pv.permille = PERMILLE_MAX;
    let (prog, pct, pvc) = (pv.prog, pv.pct, *pv);
    ui.set_props(prog, Props::Progress(ProgressProps { value: PERMILLE_MAX, style: ProgressStyle::Linear }))?;
    ui.set_text(pct, b"Done")?;
    set_go_label(ui, &pvc, false)?;
    rebuild_body(ui, a)
}

/// 重启加载（当前页 progress 归零重爬——刷新 glyph / Go（已载档）/ 链接
/// 点按同页位）。
fn restart_load(ui: &mut UiMain, a: &mut App) -> Result<(), UiError> {
    let Some(pv) = a.page.as_mut() else { return Ok(()) };
    pv.loading = true;
    pv.load_start = now();
    pv.permille = 0;
    let (prog, pct, pvc) = (pv.prog, pv.pct, *pv);
    ui.set_props(prog, Props::Progress(ProgressProps { value: 0, style: ProgressStyle::Linear }))?;
    ui.set_text(pct, b"Loading 0%")?;
    set_go_label(ui, &pvc, true)?;
    rebuild_body(ui, a)
}

/// 就地换页（链接点按——历史 v1 单级：同栈位换 site/page + 重启加载，
/// 不叠 NavStack；同页加载中 = 幂等忽略）。
fn navigate(ui: &mut UiMain, a: &mut App, site: usize, page: usize) -> Result<(), UiError> {
    let addr = SITES[site].pages[page].addr;
    {
        let Some(pv) = a.page.as_mut() else { return Ok(()) };
        if pv.site == site && pv.page == page && pv.loading {
            return Ok(()); // 同页爬升中——幂等
        }
        pv.site = site;
        pv.page = page;
        ui.set_props(
            pv.addr,
            Props::Input(InputProps {
                placeholder: TextBuf::new(addr)?,
                max_len: ADDR_MAX,
                input_kind: InputKind::Text,
            }),
        )?;
    }
    a.last = Some(site);
    books_save(a.last);
    refresh_last(ui, a);
    restart_load(ui, a)
}

/// 建页页 + 入栈（书签卡命中消费点——contacts 详情页同形：Stack[不透明
/// 页底, 内容 Col] + app_bar(back) + 尾随刷新 glyph + 地址行 + 进度行 +
/// 页体锚；push 300ms 右滑入）。Err = 容量/文本域违例 → 调用方收口。
fn open_page(ui: &mut UiMain, a: &mut App, site: usize) -> Result<(), UiError> {
    if a.page.is_some() {
        return Ok(()); // 连点门（contacts 同款：在栈/在过渡不开新页）
    }
    let t = a.t;
    let (vp_w, vp_h) = (a.vp_w, a.vp_h);
    let name = SITES[site].name;
    let addr = SITES[site].pages[0].addr;

    let root = pattern::stack(ui, a.nav_root)?;
    // 不透明页底（NavHost 滑入期页下内容不可透——pattern app_bar「面层
    // Rect」叠层语同款 + 页域任意 damage 恒有重绘覆盖源）。
    ui.add(root, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_page, w: None, h: None })))?;
    let body = ui.add(
        root,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: BODY_GAP,
            w: Some(vp_w),
            h: Some(vp_h),
            cross: CrossAlign::Stretch,
        })),
    )?;

    // —— 顶栏：app_bar(站名, back=true) + 尾随刷新 glyph（bar 行补挂——
    //    top_bar 配方无返回位、app_bar 无动作位，「Back+刷新」两键的
    //    pattern 组合承载）——
    let bar = pattern::app_bar(ui, body, &t, name, true)?;
    let name_w = name.len() as u32 * 24; // S24x48 字格宽估值（spacer 用）
    // 尾随推位（刷新 glyph 钉右缘：bar 行 = [lead, back, gap, title,
    // 本推位, refresh]——估宽 saturating，窄视口折 0 无害）。
    let flex = vp_w.saturating_sub(
        pattern::APP_BAR_LEAD + pattern::APP_BAR_BACK + pattern::APP_BAR_GAP + name_w + APP_BAR_BACK + pattern::APPBAR_TITLE_INSET,
    );
    pattern::spacer(ui, bar.bar, flex, 1)?;
    let refresh = ui.add(bar.bar, icon_spec(icons::REFRESH, t.font_primary, APP_BAR_BACK, APP_BAR_BACK))?;

    // —— 地址行：Input（值面 = placeholder 承载）+ 推位 + Go/Stop ——
    let addr_row = ui.add(
        body,
        vspec(ViewKind::Row, Props::Row(RowProps {
            gap: 0,
            w: Some(vp_w),
            h: Some(ADDR_ROW_H),
            cross: CrossAlign::Center,
        })),
    )?;
    pattern::spacer(ui, addr_row, SIDE_PAD, 1)?;
    let addr_in = ui.add(
        addr_row,
        vspec(ViewKind::Input, Props::Input(InputProps {
            placeholder: TextBuf::new(addr)?,
            max_len: ADDR_MAX,
            input_kind: InputKind::Text,
        })),
    )?;
    // 推位（Input 内在 200 + Go 64 + 双留白后余量；窄视口折 0 无害）。
    let addr_flex = vp_w.saturating_sub(2 * SIDE_PAD + 200 + 24 + 64);
    pattern::spacer(ui, addr_row, addr_flex, 1)?;
    let go = ui.add(
        addr_row,
        vspec(ViewKind::Button, Props::Button(ButtonProps {
            label: TextBuf::new(b"Stop")?,
            style: ButtonStyle::Filled,
            on_tap: EV_GO,
        })),
    )?;
    pattern::spacer(ui, addr_row, SIDE_PAD, 1)?;

    // —— 进度行：Progress(Linear) + 百分文案 ——
    let prog_row = ui.add(
        body,
        vspec(ViewKind::Row, Props::Row(RowProps {
            gap: 0,
            w: Some(vp_w),
            h: Some(PROG_ROW_H),
            cross: CrossAlign::Center,
        })),
    )?;
    pattern::spacer(ui, prog_row, SIDE_PAD, 1)?;
    let prog = ui.add(
        prog_row,
        vspec(ViewKind::Progress, Props::Progress(ProgressProps { value: 0, style: ProgressStyle::Linear })),
    )?;
    pattern::spacer(ui, prog_row, CARD_GAP, 1)?;
    let pct = ui.add(prog_row, text_spec(t.font_secondary, None, FontSize::S16x32, b"Loading 0%")?)?;
    let prog_flex = vp_w.saturating_sub(2 * SIDE_PAD + 200 + CARD_GAP + 13 * 16);
    pattern::spacer(ui, prog_row, prog_flex, 1)?;
    pattern::spacer(ui, prog_row, SIDE_PAD, 1)?;

    // —— 页体锚（Col cross=Center——子树整体重建面，rebuild_body 挂载）。
    let body_anchor = ui.add(
        body,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: 0,
            w: Some(vp_w),
            h: None,
            cross: CrossAlign::Center,
        })),
    )?;

    // a11y（表帽 32 内：pattern 内建 3 + 本页 4 + 页体链接 ≤ 2 ≈ 9）。
    let _ = ui.set_a11y(refresh, ROLE_BUTTON, b"refresh");
    let _ = ui.set_a11y(addr_in, ROLE_TEXT, b"address");
    let _ = ui.set_a11y(go, ROLE_BUTTON, b"go");

    // Go 按钮按压视觉 = 引擎 press_map（Button 在 visual 白名单——files
    // Up 按钮同形；整表替换语义，页页唯一登记件）。
    ui.set_press_map(&[(go, go)])?;

    let pv = PageView {
        back: bar.back.expect("app_bar back=true"),
        refresh,
        addr: addr_in,
        go,
        prog,
        pct,
        body: body_anchor,
        body_child: None,
        site,
        page: 0,
        loading: true,
        load_start: now(),
        permille: 0,
        links: [None; LINE_N],
    };
    a.page = Some(pv);
    rebuild_body(ui, a)?;

    // 入栈（页已挂 nav_root 下 = z 顶）+ 300ms 滑入起拍。
    a.nav.push(ui, root)?;

    a.last = Some(site);
    books_save(a.last);
    refresh_last(ui, a);
    Ok(())
}

/// 建树总装（main 一次调用）：NavHost 挂根 → 书签页（TopBar + 上次
/// 地址行 + 两列卡片网格——常驻底页不 push）。
fn build_all(ui: &mut UiMain, last: Option<usize>) -> Result<App, UiError> {
    let (vp_w, vp_h) = ui.viewport();
    let t = Tokens::pick(Theme::Light);
    let root = ui.root();
    let mut nav = NavHost::new();
    let nav_root = nav.attach(ui, root)?;

    // —— 书签页（depth 0 常驻底页）——
    let home = pattern::stack(ui, nav_root)?;
    ui.add(home, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_page, w: None, h: None })))?;
    let content = ui.add(
        home,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: BODY_GAP,
            w: Some(vp_w),
            h: None,
            cross: CrossAlign::Stretch,
        })),
    )?;
    // 顶栏（pattern 配方——56vp 高、标题起始 16vp、零动作位）。
    pattern::top_bar(ui, content, &t, b"Browser", &[])?;
    // 上次地址行（BOOKS.JSN 回显——空文隐藏；驻留常显非瞬态）。
    let mut lbuf = [0u8; 32];
    let ln = last_bytes(last, &mut lbuf);
    let last_text = ui.add(content, text_spec(t.font_secondary, None, FontSize::S16x32, &lbuf[..ln])?)?;
    let _ = ui.set_a11y(last_text, ROLE_TEXT, b"last");

    // —— 两列书签卡片网格（格宽 = 等分轨（Grid §4.2 同式整数除）；
    //    卡 = pattern::card 配方 elevation 1 + globe glyph 前导图标 +
    //    站名/地址双行；按压层 = Stack 面层 Rect（ItemState 单源），
    //    内容行置顶层盖按压层——files 批行装配同形）——
    let track = vp_w.saturating_sub(2 * SIDE_PAD + (GRID_COLS - 1) * GRID_GAP) / GRID_COLS;
    let grid_w = GRID_COLS * track + (GRID_COLS - 1) * GRID_GAP;
    let rows = (SITES.len() as u32 + GRID_COLS - 1) / GRID_COLS;
    let holder = ui.add(
        content,
        vspec(ViewKind::Row, Props::Row(RowProps {
            gap: 0,
            w: Some(vp_w),
            h: None,
            cross: CrossAlign::Start,
        })),
    )?;
    pattern::spacer(ui, holder, SIDE_PAD, 1)?;
    let grid = ui.add(
        holder,
        vspec(ViewKind::Grid, Props::Grid(GridProps {
            cols: GRID_COLS as u8,
            rows: rows as u8,
            gap: GRID_GAP,
            cross: CrossAlign::Start,
            w: Some(grid_w),
            h: None,
        })),
    )?;
    pattern::spacer(ui, holder, SIDE_PAD, 1)?;

    let mut cards = [None; SITES.len()];
    for (i, s) in SITES.iter().enumerate() {
        let sk = pattern::stack(ui, grid)?;
        // 卡片本体（pattern::card 配方——elevation 0：elevation > 0 的阴影
        // 带会被下方不透明按压面层盖没（v1 无 alpha 通道）故取平卡，files
        // /photos 批同裁定；无配方内置前导图标（内容层自挂 glyph——按压
        // 层叠层习语下配方内置件会被面层盖没））。
        pattern::card(ui, sk, &t, 0, None)?;
        // 五态按压层（Normal = 卡面白——常态隐形；Pressed = brand 20% 预混）。
        let press = ui.add(
            sk,
            vspec(ViewKind::Rect, Props::Rect(RectProps { bg: state_color(ItemState::Normal.bg()), w: None, h: None })),
        )?;
        // 内容行（置顶层——图标 + 站名/地址双行盖按压层之上）。
        let crow = ui.add(
            sk,
            vspec(ViewKind::Row, Props::Row(RowProps {
                gap: CARD_GAP,
                w: Some(track),
                h: Some(LISTITEM_H),
                cross: CrossAlign::Center,
            })),
        )?;
        pattern::spacer(ui, crow, LISTITEM_PAD_X, 1)?;
        let with_icon = track >= CARD_ICON_MIN_W; // 窄视口防御：藏图标防溢出
        if with_icon {
            let icon = ui.add(crow, icon_spec(GLOBE, t.brand, ICON_SZ, ICON_SZ))?;
            let _ = ui.set_a11y(icon, ROLE_LIST_ITEM, s.name);
        }
        let labels = ui.add(
            crow,
            vspec(ViewKind::Col, Props::Col(ColProps {
                gap: 8,
                w: None,
                h: None,
                cross: CrossAlign::Start,
            })),
        )?;
        let name = ui.add(labels, text_spec(t.font_primary, None, FontSize::S24x48, s.name)?)?;
        ui.add(labels, text_spec(t.font_secondary, None, FontSize::S16x32, s.pages[0].addr)?)?;
        pattern::spacer(ui, crow, LISTITEM_PAD_X, 1)?;
        if !with_icon {
            let _ = ui.set_a11y(name, ROLE_LIST_ITEM, s.name);
        }
        cards[i] = Some(CardReg { stack: sk, press, site: i });
    }

    Ok(App {
        t,
        vp_w,
        vp_h,
        nav,
        nav_root,
        last_text,
        cards,
        page: None,
        last,
        pressed: None,
    })
}

fn main() {
    // —— 降级门：wm 客户端端未交付 → 一行诊断 + park 常驻（settings
    //    先例同形；窗口义务语义不折衷成 exit(0) 秒退）。本 app 声明
    //    fs 词 + [ui] 节 → svcmgr gift 槽位 [rootfs, fs, wm, input]——
    //    wm 落 extras[2]（berry-libc io.rs GIFT_H[4]；`wm_cli_h()` 的
    //    [0] 位对 wants_fs 程序承载 rootfs VMO，不可用），取用经**同槽
    //    访问器** `wm_present_h`（「同一 gift 位对不同程序承载不同对象
    //    的多产面惯例——extras[2] 对 gui-wmd 承载 present 客户端端、对
    //    本 app 承载 wm 客户端端」；专用访问器挂 appshell 裁决 A 落地
    //    换轨）。settings（无 fs 词）wm 落 [0] 走 `wm_cli_h`——两形
    //    分野单源 = svcmgr app_launch GUI 授权链（5452649af）。
    let hw = wm_present_h();
    if hw == 0 {
        eprintln!("browser: wm absent (no gui gift)");
        park();
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
        "browser: wm v={} {}x{} input={}",
        info.version_wm,
        info.disp_w,
        info.disp_h,
        input_h()
    );

    // —— UI arena 堆置（settings 先例：UiMain 构造中间体 LTO 后仍部分
    //    落栈 ~29KB；Box::leak = 常驻面 leak 即分配）。
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    static TEXT_METRICS: berry_ui::ProportionalTextMetrics = berry_ui::ProportionalTextMetrics;
    ui.set_text_metrics(&TEXT_METRICS);
    // glyph 真供件（程序化三面：BACK/REFRESH/GLOBE——设计稿 §3.3 自绘
    // 路，零位图资产增量）。
    ui.set_icon_source(&ICONS);

    // —— BOOKS.JSN 读入（三约束）+ 建树（Err = 容量/文本域违例 → 收口）。
    let last = books_load();
    let mut a = match build_all(ui, last) {
        Ok(a) => a,
        Err(e) => fail(&format!("build err={e:?}")),
    };

    // —— 帧循环（常驻义务——永不返回）。
    run(hw, input_h(), ui, &mut a);
}

/// 帧循环 + 输入 + 事件消费（settings 食谱 + 四增量：Msg 双面消费
///（卡按压/命中几何面 + Click 锚面（back/刷新/链接））、NavHost tick +
/// take_popped、进度爬升时间基步进、EV_GO 双档）。
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
    if berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0 || va != FRAME_VA {
        fail("vmar map");
    }
    let desc = match TextureDesc::new(w, h, stride, TextureFormat::Rgbx) {
        Ok(d) => d,
        Err(e) => fail(&format!("texture desc {e:?}")),
    };
    // SAFETY: [FRAME_VA, FRAME_VA+len) 刚由 vmar_map 落位且整段已 commit
    //（launcher/settings 同形）；run 永不返回——fb 终身持有。
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };

    // —— glyph scratch 堆置（DisplayList 'a 钉调用方 scratch）。
    let scratch: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());

    // —— vsync 协商 + 起振（首帧整帧 damage）。
    ui.set_vsync(VSYNC_NS, 0);
    ui.wake(now());

    let mut last_expected: u64 = 0; // pump 重采样 target（上帧 expected 轨）
    let mut last_vote: Option<frame::VoteBallot> = None; // 票面变化才发
    let mut presented: u64 = 0; // FRAME_CALLBACK 过链计数（首帧诊断门）
    let mut buf_id: u32 = 1; // SUBMIT buffer 代际
    let mut hi = hi_in; // input 端（失效面置 0——静态页续活）
    let mut input_pending = false; // INPUT_POLL 在途
    let mut input_buf = [0u8; gp::INPUT_REP_MAX];
    let mut input_hs = [0 as Handle; 8];
    let mut pct_buf = [0u8; 16]; // 进度文案 scratch

    loop {
        let t_now = now();

        // —— 真实输入（settings 同形）：先收上一轮 INPUT_POLL 应答，再挂
        //    下一轮；事件逐条 push 进引擎（press_react/widget_react 在
        //    pump_input 期同拍消费——Go 按钮 on_tap 落事件）。
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
                                let pe = hit::PtrEvent::from_wire(
                                    ev.phase,
                                    ev.pointer_id,
                                    ev.x,
                                    ev.y,
                                    ev.timestamp_ns,
                                    ev.buttons,
                                    ev.pressure,
                                );
                                if let Err(e) = ui.push_pointer(pe) {
                                    eprintln!("browser: input push err={e:?}");
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {} // 无事件常态
                    Some(Err(st)) => eprintln!("browser: input poll st={st}"),
                    None => eprintln!("browser: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("browser: input recv err={s}");
                hi = 0;
                input_pending = false;
            }
        }
        if hi != 0 && !input_pending {
            if input_arm(hi) {
                input_pending = true;
            } else {
                // 输入面失效（arm 失败 = 通道死/超时）——置 0 永久降级为
                // 静态页（页面常驻义务不受输入面拖累）。
                hi = 0;
            }
        }

        // —— 输入泵（referee + 重采样 + widget_react——引擎单口）；
        //    Msg 双面消费：卡命中 = Release 几何判定（Click 无坐标面）；
        //    back/刷新/链接 = Click 锚判定。
        let msgs = ui.pump_input(last_expected);
        for m in msgs.iter() {
            match *m {
                Msg::Press { x, y, .. } => {
                    // 卡按压反馈（ItemState 五态换底——书签页域）。
                    if a.page.is_none() {
                        if let Some(c) = card_at(ui, a, x, y) {
                            card_tint(ui, a, c, true);
                            a.pressed = Some(c);
                        }
                    }
                }
                Msg::Release { x, y, .. } => {
                    if let Some(c) = a.pressed.take() {
                        card_tint(ui, a, c, false);
                    }
                    // 卡命中 → 页页入栈（非拖 up 路径——slop 内 up 才有
                    // Release，拖拽路径自然豁免误触）。
                    if a.page.is_none() {
                        if let Some(site) = card_at(ui, a, x, y) {
                            match open_page(ui, a, site) {
                                Ok(()) => ui.wake(now()),
                                Err(e) => eprintln!("browser: page open err={e:?}"),
                            }
                        }
                    }
                }
                Msg::Cancel { .. } => {
                    if let Some(c) = a.pressed.take() {
                        card_tint(ui, a, c, false);
                    }
                }
                Msg::DragStart { .. } => {
                    // 拖起 = 按压视觉还原（拖路径无 Release——补还原面）。
                    if let Some(c) = a.pressed.take() {
                        card_tint(ui, a, c, false);
                    }
                }
                Msg::Click { node } => {
                    // 页页三锚：返回 glyph（pop 回书签）/ 刷新 glyph（重载
                    // 当前页）/ 链接行（就地换页——历史 v1 单级）。
                    if let Some(pv) = a.page {
                        if node == pv.back {
                            if a.nav.pop(ui) {
                                ui.wake(now());
                            }
                        } else if node == pv.refresh {
                            if let Err(e) = restart_load(ui, a) {
                                eprintln!("browser: reload err={e:?}");
                            }
                            ui.wake(now());
                        } else if let Some((ts, tp)) = pv.links.iter().flatten().find(|(n, _, _)| *n == node).map(|&(_, s, p)| (s, p)) {
                            if let Err(e) = navigate(ui, a, ts, tp) {
                                eprintln!("browser: nav err={e:?}");
                            }
                            ui.wake(now());
                        }
                    }
                }
                _ => {}
            }
        }

        // —— NavHost 推拉动画步进（在途 → 每帧推进 + 保活）。
        if a.nav.tick(ui, t_now) {
            ui.wake(t_now);
        }
        // —— 出栈完结页移交 remove（back 路径收口——书签页回显零重建）。
        if let Some(node) = a.nav.take_popped() {
            if let Err(e) = ui.remove(node) {
                fail(&format!("page remove err={e:?}"));
            }
            a.page = None;
            a.pressed = None;
            ui.wake(t_now);
        }

        // —— 组件事件消费（widget_react 产物）：Go/Stop 双档（加载中 =
        //    Stop 定格揭页；已载 = Go 重载）。
        let evs = ui.take_events();
        for ev in evs.iter() {
            match ev.event {
                EV_GO => {
                    let loading = a.page.as_ref().is_some_and(|p| p.loading);
                    let r = if loading { finish_load(ui, a) } else { restart_load(ui, a) };
                    if let Err(e) = r {
                        eprintln!("browser: go err={e:?}");
                    }
                    ui.wake(now());
                }
                _ => {}
            }
        }

        // —— 进度爬升（Loading 相：时间基 permille = elapsed/1.2ms——
        //    帧率无关；Run 保活每 vsync 一格 set_props/set_text）。
        let mut loaded_now = false;
        if let Some(pv) = a.page.as_mut() {
            if pv.loading {
                ui.wake(t_now);
                let el = t_now.saturating_sub(pv.load_start);
                let pm = ((el / (LOAD_DUR_NS / 1000)) as u32).min(PERMILLE_MAX);
                if pm != pv.permille {
                    pv.permille = pm;
                    let (prog, pct) = (pv.prog, pv.pct);
                    if let Err(e) = ui.set_props(
                        prog,
                        Props::Progress(ProgressProps { value: pm, style: ProgressStyle::Linear }),
                    ) {
                        eprintln!("browser: prog set err={e:?}");
                    }
                    let pn = pct_bytes(pm, &mut pct_buf);
                    if let Err(e) = ui.set_text(pct, &pct_buf[..pn]) {
                        eprintln!("browser: pct set err={e:?}");
                    }
                }
                if pm >= PERMILLE_MAX {
                    loaded_now = true;
                }
            }
        }
        if loaded_now {
            if let Err(e) = finish_load(ui, a) {
                eprintln!("browser: finish err={e:?}");
            }
            ui.wake(now());
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
                PassDescriptor::clear(a.t.bg_page.r, a.t.bg_page.g, a.t.bg_page.b)
            } else {
                // 增量帧：Load+Store + K 矩形并集包络。
                PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(damage_envelope(&dmg)))
            };
            let dl = enc.finish();
            if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
                fail(&format!("raster submit {e:?}"));
            }

            // SUBMIT：fresh READ dup 别名随行（send 即 take；TRANSFER 保位）。
            let mut alias: Handle = 0;
            if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0 || alias == 0 {
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
                println!("browser: first frame");
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

        // —— 稳态等待：deadline = min(next_wakeup, IDLE 帽)——Loading 相
        //    由本循环逐拍 wake 保 Run（vsync 拍点醒），无需第三源。
        let cap = now() + IDLE_CAP_NS;
        let deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
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

/// K 矩形并集包络（增量 pass 的 dirty 域；launcher/settings 同形——
/// engine damage 恒 viewport 裁剪，i32 折 try_from 饱和防御）。
fn damage_envelope(dmg: &damage::DamageSet) -> GfxRect {
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

/// 挂一轮 INPUT_POLL（settings 同形）：inputd 阻塞批拉 ≤3 事件；
/// -EBUSY = 通道满等 WRITABLE 后重试；其余错 = 输入面失效。
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
            eprintln!("browser: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("browser: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}

// ---------------------------------------------------------------------------
// —— 纯函数单测（宿主可编译面——RPC/帧循环 berry-libc AArch64 专属不
//    入测；执行面归 QEMU/镜像冒烟，files 批同口径）——

#[cfg(test)]
mod tests {
    use super::*;

    /// JSN canonical 编码恰 21B + 逐字节钉板。
    #[test]
    fn jsn_encode_canonical() {
        let mut out = [0u8; JSN_MAX];
        let n = jsn_encode(Some(3), &mut out);
        assert_eq!(n, 21);
        assert_eq!(&out[..n], b"{\"schema\":1,\"last\":3}");
        let n = jsn_encode(None, &mut out);
        assert_eq!(&out[..n], b"{\"schema\":1,\"last\":9}");
    }

    /// JSN 严格解析：合法形往返全等 + 坏形（长度/前缀/尾形/非数字/
    /// 域外站序）全拒。
    #[test]
    fn jsn_parse_roundtrip() {
        for v in [None, Some(0), Some(5)] {
            let mut out = [0u8; JSN_MAX];
            let n = jsn_encode(v, &mut out);
            assert_eq!(jsn_parse(&out[..n]), Some(v));
        }
        assert_eq!(jsn_parse(b""), None);
        assert_eq!(jsn_parse(b"{\"schema\":1,\"last\":}"), None);
        assert_eq!(jsn_parse(b"{\"schema\":2,\"last\":3}"), None);
        assert_eq!(jsn_parse(b"{\"schema\":1,\"last\":3 "), None);
        assert_eq!(jsn_parse(b"{\"schema\":1,\"last\":x}"), None);
        // 域外站序（6..=8 无站点）按坏读自愈（books_load 面拒绝）。
        let mut out = [0u8; JSN_MAX];
        out[..21].copy_from_slice(b"{\"schema\":1,\"last\":7}");
        assert_eq!(jsn_parse(&out[..21]), Some(Some(7)));
    }

    /// books_load 域外值自愈判据（站序 < SITES.len() 才收）。
    #[test]
    fn jsn_domain_guard() {
        let accept = |v: Option<usize>| v.is_none_or(|s| s < SITES.len());
        assert!(!accept(Some(7))); // 域外站序 = 坏读自愈面
        assert!(accept(Some(5)));
        assert!(accept(None));
    }

    /// 站点表自洽：名称/地址/标题/行文全 ASCII 且 ≤ 域帽；链接目标域内。
    #[test]
    fn sites_table_sane() {
        let ascii = |b: &[u8]| b.is_ascii() && !b.is_empty();
        for s in SITES.iter() {
            assert!(ascii(s.name) && s.name.len() <= 10);
            for p in s.pages.iter() {
                assert!(ascii(p.addr) && p.addr.len() <= 13);
                assert!(ascii(p.title) && p.title.len() <= 16);
                for line in p.lines.iter() {
                    match *line {
                        PLine::Para(b) => {
                            assert!(ascii(b) && b.len() <= 32);
                        }
                        PLine::Link { label, site, page } => {
                            assert!(ascii(label) && label.len() <= 24);
                            assert!(label.starts_with(b"> "));
                            assert!(site < SITES.len());
                            assert!(page < 2);
                        }
                    }
                }
            }
        }
    }

    /// 每站恰 2 页、每页恰 2 链接（网格/锚表容量钉板）。
    #[test]
    fn sites_shape() {
        for (si, s) in SITES.iter().enumerate() {
            assert_eq!(s.pages.len(), 2);
            for p in s.pages.iter() {
                assert_eq!(p.lines.len(), LINE_N);
                assert!(p.lines.iter().filter(|l| matches!(l, PLine::Link { .. })).count() == 2);
            }
            // 站名互异 + home 地址互异（书签卡可判性）。
            for (oj, o) in SITES.iter().enumerate() {
                if si != oj {
                    assert_ne!(s.name, o.name);
                    assert_ne!(s.pages[0].addr, o.pages[0].addr);
                }
            }
        }
    }

    /// 进度/上次地址文案装配钉板。
    #[test]
    fn text_assembly() {
        let mut b = [0u8; 16];
        let n = pct_bytes(0, &mut b);
        assert_eq!(&b[..n], b"Loading 0%");
        let n = pct_bytes(507, &mut b);
        assert_eq!(&b[..n], b"Loading 50%");
        let n = pct_bytes(1000, &mut b);
        assert_eq!(&b[..n], b"Loading 100%");
        let mut l = [0u8; 32];
        assert_eq!(last_bytes(None, &mut l), 0);
        let n = last_bytes(Some(0), &mut l);
        assert_eq!(&l[..n], b"Last: news.berry");
        let n = last_bytes(Some(5), &mut l);
        assert_eq!(&l[..n], b"Last: forum.berry");
    }

    /// glyph 表：非空、全像素 ARGB 域内（A=0 或 0xFF……）、三表互异、
    /// 中心区（环内空白）透亮。
    #[test]
    fn glyph_tables_sane() {
        let tables: [&[u32]; 3] = [&BACK_PX, &REFRESH_PX, &GLOBE_PX];
        let nonempty: Vec<usize> = tables
            .iter()
            .map(|t| t.iter().filter(|&&p| p != 0).count())
            .collect();
        for n in nonempty.iter() {
            assert!(*n > 40, "glyph too thin: {n}");
        }
        for t in tables.iter() {
            for &p in t.iter() {
                assert!(p == 0 || (p & 0xFF00_0000) == 0xFF00_0000);
            }
        }
        assert_ne!(&BACK_PX[..], &REFRESH_PX[..]);
        assert_ne!(&BACK_PX[..], &GLOBE_PX[..]);
        assert_ne!(&REFRESH_PX[..], &GLOBE_PX[..]);
        // BACK 折点 (8,12) 双臂交汇着墨；GLOBE 十字心 (12,12) = 赤道×
        // 经线交点着墨（线框语义）。
        assert_ne!(BACK_PX[12 * GLYPH_N + 8], 0);
        assert_ne!(GLOBE_PX[12 * GLYPH_N + 12], 0);
    }
}
