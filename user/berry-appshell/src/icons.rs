//! icons —— app 注册表 + 图标供给桥（波3-件⑦ 抽取自 berry-launcher
//! icons.rs——GUI-8 段③ W2；行为零变化）。
//!
//! 责任：boot 期 APP_LIST 拉取（S3 蓝图：**一请求 → 分片流**——svcmgr
//! `app_serve_list` 对单个 APP_LIST_REQ 发整条 ENTRY/CHUNK/END 流，应答
//! 逐帧到账为独立 channel 消息 → 客户端 send-once + recv-loop）→
//! `ListDecoder` 三执法（idx 单调 +1 / blob 收支 / END 对账——任何 None
//! 弃整流，半张清单不进渲染面）→ `AppEntry` 装配 + BYI1 两遍解码 →
//! [`ShellIcons`]（berry-ui `IconSource` 真供件）。
//!
//! 冻结相（R1 修约 1/2）：消费应用 extras 恰 [wm] → `app_cli_h()==0`
//! → [`init`] 即返（零 RPC 零打印——空注册表常态，桌面全占位格）。
//!
//! **图标 id 稠密域契约**：仅解码成功的条目按序编 0..n 稠密 id
//! （`NO_ICON` = 域外哨兵）——建树侧只对域内 id 建 Icon 节点；
//! [`ShellIcons`] 是**真供件**（`is_empty()==false`）：berry-ui 契约下
//! 真供件对未注册 id 返 None = debug_assert loud（layout.rs icon_dims /
//! paint.rs emit_image 两面同律）。稠密赋值即一致性构造：id ∈ 域 ⟺
//! FACES[id] 在册——两面查表恒 Some，loud 面结构性不可达。
//!
//! **像素常驻帽**（草案 kill criteria ③ 回落）：Σ w*h ≤ 帽——超帽条目
//! 降 `NO_ICON`（label-only 格）+ 诊断，不崩不静默吞。boot 全量常驻
//! （无 LRU 换入换出——D28 供给槽是借用面，缓存编排归消费侧；
//! berry-gfx `IconLruCache` 服务 GlyphBits mono 面非本面）。
//!
//! 静态面：boot 单线程单写原子指针装载（免 static mut 直写 + 首装定界
//! 幂等——boot 单线程无竞态面；OnceLock 是 std 面，no_std 本 crate 以
//! ptr+len 原子对同语义替代）；像素/条目 `Box::leak` 堆置（elf2flat 会
//! 把 .bss 物化进平像——svcmgr 单页墙判例：禁大 static 解码 scratch，
//! 解码缓冲走堆 Vec）。
//!
//! host 测试面：导航符号几何/面生成（纯像素数学）、注册表缺省态、
//! 供给桥壁纸特例、条目竣工拒面——APP_LIST RPC 半程 `cfg(not(test))`
//! （berry-fsclient 先例）。

#[path = "status_icons.rs"]
mod status_icons;

#[path = "wallpaper.rs"]
mod wallpaper;

/// 全量 SVG 位图库（tools/svg2icons.py 生成：Material Icons + OH
/// sysResource + OH systemui，Apache-2.0；66×66 二值 ARGB rodata 直供，
/// 零堆零 RPC——与 app 稠密域正交的独立 id 域）。
#[path = "icon_data/mod.rs"]
mod icon_data;

#[cfg(not(test))]
use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

#[cfg(not(test))]
use berry_appproto as ap;
use berry_gfx::icon::{decode, IconError};
use berry_layout::ID_MAX;
#[cfg(not(test))]
use berry_sys::Handle;
use berry_ui::IconSource;

use crate::host::diag;
#[cfg(not(test))]
use crate::rpc::{close_h, rpc_recv, rpc_send};

/// 壁纸源顶区像素采样（状态栏 scrim 动态预混）：返回顶 `rows` 行
/// 的 ARGB 像素切片与源宽——scene::status_scrim 消费（icons 私有
/// 模块的最小重导出面）。
pub fn wallpaper_top_rows(rows: u32) -> (&'static [u32], u32) {
    let h = wallpaper::WALLPAPER_H.min(rows);
    let n = (wallpaper::WALLPAPER_W as usize) * (h as usize);
    (&wallpaper::WALLPAPER_PX[..n.min(wallpaper::WALLPAPER_PX.len())], wallpaper::WALLPAPER_W)
}

// —— 位图库 id 域（icon_data 独立稠密域；rodata 直供不占 PX_CAP 堆账） ——

/// 位图库 id 域基（0xFD00..0xFD00+ICON_DATA.len()——u16 域内与 app
/// 稠密域（0..~20）及 WALLPAPER 特例（0xFFFE）三向正交）。
pub const BITMAP_ID_BASE: u16 = 0xFD00;

/// 位图库 id 域上界钉板：域宽必须在 WALLPAPER 特例之下（编译期执法，
/// 图标库扩容越界即炸——loud not silent）。
const _: () = assert!(
    BITMAP_ID_BASE as usize + icon_data::ICON_DATA.len() < WALLPAPER as usize,
    "icon_data id 域越过 WALLPAPER 特例"
);

/// 位图名 → 位图库 id（`BITMAP_ID_BASE` 起稠密；未知名 = None）。
/// 建树侧「位图优先、程序化内建面兜底」的查询面（desktop dock 位 /
/// scene 状态栏位消费）。
pub fn bitmap_id(name: &str) -> Option<u16> {
    icon_data::ICON_DATA
        .iter()
        .position(|&(n, _, _, _)| n == name)
        .map(|i| BITMAP_ID_BASE + i as u16)
}

/// 位图库域条目直查（(name, px, w, h)；域外 = None）——IconSource 供件
/// 与 scene spec 面共用。
pub fn bitmap_entry(id: u16) -> Option<(&'static str, &'static [u32], u32, u32)> {
    if id < BITMAP_ID_BASE {
        return None;
    }
    icon_data::ICON_DATA.get((id - BITMAP_ID_BASE) as usize).copied()
}

/// dock 四位的位图库优先名（OH sysRes 资产；缺名回退 DOCK_FACE_IDS
/// 分段面——位图优先序在 desktop.rs 建树侧）。
// Dock 位图（owner 2026-11-16）：OH SVG Sketch 嵌套结构光栅化后坍缩 +
// 位图 id 被渲染面按 BYI1 解码（BadMagic [0,0,0,0] → 桌面死）——回退
// DOCK_FACE_IDS 程序化 SmartDock 面（genohicons BYI1 官方背板，原行为）。
pub const DOCK_BITMAP_NAMES: [&str; 4] = ["", "", "", ""];

/// 状态栏三位的位图库优先名：前两位 OH systemui 官方 signal/wifi 面；
/// 第三位空串 = 电池走程序化 BATTERY 面（电量真值动态面，不入静态库）。
// 状态栏位图（2026-11-16 同 dock 裁定）：位图 id 与 BYI1 渲染面不兼容，
// 全回退程序化面（原行为，稳定）。
pub const STATUS_BITMAP_NAMES: [&str; 3] = ["", "", "",
];

/// 内建状态/dock 图标 id（建树侧直建 Icon 节点；位于 app 图标稠密域
/// 前缀——app 稠密 id 实从 13 起（finish_entry 按 faces.len() 赋号），
/// 契约不外泄）。
pub const STATUS_SIGNAL: u16 = 0;
pub const STATUS_WIFI: u16 = 1;
pub const STATUS_BATTERY: u16 = 2;
/// 静态壁纸源 id（稠密 app 域外的唯一固定特例——rodata 直供零堆）。
pub const WALLPAPER: u16 = 0xFFFE;
pub const WALLPAPER_DIM: (u32, u32) = (wallpaper::WALLPAPER_W, wallpaper::WALLPAPER_H);
/// dock 分段内建面 id（0..3 号 app 对应 Contacts/Photos/Settings/MMS 官方顺序）。
pub const DOCK0: u16 = 3;
pub const DOCK1: u16 = 4;
pub const DOCK2: u16 = 5;
pub const DOCK3: u16 = 6;
pub const DOCK_FACE_IDS: [u16; 4] = [DOCK0, DOCK1, DOCK2, DOCK3];
/// 五键导航面 id（波一刀 D，审计 02 号 §6.1 M2——back/home/power 自绘
/// 最简形：程序化 Argb「深底圆角块 + 白符号」，命中区 = 键块整体 ≥44px
/// 应用规范 §3.2；OH 官方仓无导航键现成 Apache-2.0 资产可续取，按
/// ADR #63 同等口径自绘单色形——底块色 = CLEAR_BG 同族深蓝灰）。
pub const NAV_BACK: u16 = 7;
pub const NAV_HOME: u16 = 8;
pub const NAV_POWER: u16 = 9;
/// 关机确认态面（二击 3s 窗 armed：同形换暗红警示底——帧循环
/// set_props 换 id，运行期零重建）。
pub const NAV_POWER_ARMED: u16 = 10;
/// 多任务键面（v1 语义 = 重新拉起最近应用）。
pub const NAV_RECENTS: u16 = 11;
/// 旋转键面（v1 语义 = 通知宿主旋转——berry input 键位；键面常驻）。
pub const NAV_ROTATE: u16 = 12;
pub const DOCK0_DIM: (u32, u32) = (status_icons::DOCK0_W, status_icons::DOCK0_H);
pub const DOCK1_DIM: (u32, u32) = (status_icons::DOCK1_W, status_icons::DOCK1_H);
pub const DOCK2_DIM: (u32, u32) = (status_icons::DOCK2_W, status_icons::DOCK2_H);
pub const DOCK3_DIM: (u32, u32) = (status_icons::DOCK3_W, status_icons::DOCK3_H);
pub const DOCK_DIMS: [(u32, u32); 4] = [DOCK0_DIM, DOCK1_DIM, DOCK2_DIM, DOCK3_DIM];
/// 状态三面源真值尺寸（FACES 装配与 status_icon_spec 消费——渲染归一
/// 24vp 由 spec 侧 w/h 显式传递，源尺寸单源在此）。
pub const STATUS_SIGNAL_DIM: (u32, u32) = (status_icons::SIGNAL_W, status_icons::SIGNAL_H);
pub const STATUS_WIFI_DIM: (u32, u32) = (status_icons::WIFI_W, status_icons::WIFI_H);
pub const STATUS_BATTERY_DIM: (u32, u32) = (status_icons::BATTERY_W, status_icons::BATTERY_H);
/// 导航面源边长（66px 源 → big 132 目标 2x ScaledArgb / 紧凑 44 缩采样）。
pub const NAV_SRC_PX: u32 = 66;
const BUILTIN_FACES: usize = 13;
const _: () = assert!(BUILTIN_FACES == 13, "状态三 + dock 四 + 导航六面内建面钉板");

/// 无图标哨兵（icon 域外值——建树侧不建 Icon 节点）。
pub const NO_ICON: u16 = u16::MAX;

/// 注册表条目帽（防御面：wire 无条目数先验，超帽 = 协议滥用 → 弃整流；
/// CELL_MAX=48 恰满 + 裕量）。
const REG_MAX: usize = 64;

/// 单条目 BYI1 blob 帽（wire blob_len u32 先验拒面：w*h ≤ 65536 px 的
/// 最坏 RLE 流 ≈ 384KiB，512KiB 帽 = 慷慨上界；超帽弃整流）。
const BLOB_MAX: usize = 512 * 1024;

/// 像素常驻帽（225'000 px ≈ 879KiB——消费应用静态堆实为 1MiB；超帽
/// 单条降 label-only，防 std 分配失败把桌面进程整杀）。
const PX_CAP: u64 = 225_000;

/// 注册表条目（APP_LIST 应答逐条镜像；id 域 = berry-layout/
/// berry-manifest ID_MAX=24 同源词法；label 域 = wire APP_ENTRY label[64]
/// ——草案 §5，两帽值以彼处为对账锚点）。
#[derive(Clone, Copy)]
pub struct AppEntry {
    pub id: [u8; ID_MAX],
    pub id_len: u8,
    pub label: [u8; 64],
    pub label_len: u8,
    /// 稠密图标 id（`NO_ICON` = 无图标/解码失败/超帽）。
    pub icon: u16,
}

impl AppEntry {
    /// id 有效字节切片。
    pub fn id_bytes(&self) -> &[u8] {
        &self.id[..self.id_len as usize]
    }

    /// 显示名有效字节切片（桌面 label / a11y label 同源）。
    pub fn label_bytes(&self) -> &[u8] {
        &self.label[..self.label_len as usize]
    }
}

// —— boot 注册表面（boot 单线程单写：ptr+len 原子对，null = 未装；
// —— 原 OnceLock「首装定界」语义 = null 检查后装，二次装载恒保首装）——

static REGISTRY_PTR: AtomicPtr<AppEntry> = AtomicPtr::new(ptr::null_mut());
static REGISTRY_LEN: AtomicUsize = AtomicUsize::new(0);

/// 注册表只读面（未装载 = 空切片）。
pub fn registry() -> &'static [AppEntry] {
    let p = REGISTRY_PTR.load(Ordering::Acquire);
    if p.is_null() {
        return &[];
    }
    // SAFETY: 装载面 Box::leak 终身切片；len 先于 ptr 发布（Release 对）。
    unsafe { core::slice::from_raw_parts(p, REGISTRY_LEN.load(Ordering::Acquire)) }
}

/// 注册表装载（幂等——首装定局，二次 set 恒保首装）。
fn set_registry(entries: &'static [AppEntry]) {
    if REGISTRY_PTR.load(Ordering::Relaxed).is_null() {
        REGISTRY_LEN.store(entries.len(), Ordering::Release);
        REGISTRY_PTR.store(entries.as_ptr() as *mut AppEntry, Ordering::Release);
    }
}

/// 解码产物（堆常驻像素面；`Box::leak` 外层切片即整体终身——内层 Vec
/// 随之永不回收 = 分配即常驻，boot 单次无增长面）。
pub(crate) struct IconFace {
    pub(crate) w: u32,
    pub(crate) h: u32,
    pub(crate) px: Vec<u32>,
}

/// 稠密 id → 解码面（`REGISTRY` 同批装配——两表索引一致性由构造序保证；
/// ptr+len 原子对同 REGISTRY 律）。
static FACES_PTR: AtomicPtr<IconFace> = AtomicPtr::new(ptr::null_mut());
static FACES_LEN: AtomicUsize = AtomicUsize::new(0);

/// 稠密 id → 解码面只读切片（未装载 = 空）。
pub(crate) fn faces() -> &'static [IconFace] {
    let p = FACES_PTR.load(Ordering::Acquire);
    if p.is_null() {
        return &[];
    }
    // SAFETY: 同 REGISTRY——装载面 Box::leak 终身。
    unsafe { core::slice::from_raw_parts(p, FACES_LEN.load(Ordering::Acquire)) }
}

fn set_faces(f: &'static [IconFace]) {
    if FACES_PTR.load(Ordering::Relaxed).is_null() {
        FACES_LEN.store(f.len(), Ordering::Release);
        FACES_PTR.store(f.as_ptr() as *mut IconFace, Ordering::Release);
    }
}

/// boot 期初始化（main 早期单次调用）。`app_h == 0`（冻结相 extras 恰
/// [wm]）→ 即返空注册表零 RPC；在场 → APP_LIST 拉取装载（失败 = 诊断
/// + 空注册表——半张清单不进渲染面，应用不死）。
#[cfg(not(test))]
pub fn init(app_h: Handle) {
    if app_h == 0 {
        // 冻结相（extras 恰 [wm]、无 APP_LIST）也补状态栏图标面：FACES 不装
        // = 无数据盘拓扑状态栏零图标——pull 失败分支同款兜底（桌面 UX B 线
        // 存量 bug 一行修，base 刀C 语义移植到原子版 API）。
        set_faces(Box::leak(status_faces().into_boxed_slice()));
        return;
    }
    match pull(app_h) {
        Some(entries) => set_registry(entries),
        None => {
            diag("app list unavailable -- empty registry");
            set_registry(Box::leak(Vec::new().into_boxed_slice()));
            set_faces(Box::leak(status_faces().into_boxed_slice()));
        }
    }
}

/// 宿主生成期 BYI1 → 运行期 IconFace（dock 大面走 RLE，应用二进制
/// 只携压缩字节；decode 契约与 APP_LIST 图标同源）。
fn decoded_face(blob: &[u8]) -> IconFace {
    let need = match decode(blob, &mut []) {
        Err(IconError::BufferTooSmall { need, .. }) => need,
        _ => {
            diag("builtin icon probe bad");
            return IconFace { w: 1, h: 1, px: vec![0] };
        }
    };
    let mut px = vec![0u32; need];
    match decode(blob, &mut px) {
        Ok(desc) => IconFace { w: desc.w, h: desc.h, px },
        Err(e) => {
            diag(&format!("builtin icon decode {e:?}"));
            IconFace { w: 1, h: 1, px: vec![0] }
        }
    }
}

/// 状态栏三内建面 + dock 四分段面 + 导航六面（刀 D）。
pub(crate) fn status_faces() -> Vec<IconFace> {
    vec![
        IconFace {
            w: status_icons::SIGNAL_W,
            h: status_icons::SIGNAL_H,
            px: status_icons::SIGNAL_PX.to_vec(),
        },
        IconFace {
            w: status_icons::WIFI_W,
            h: status_icons::WIFI_H,
            px: status_icons::WIFI_PX.to_vec(),
        },
        IconFace {
            w: status_icons::BATTERY_W,
            h: status_icons::BATTERY_H,
            px: status_icons::BATTERY_PX.to_vec(),
        },
        decoded_face(&status_icons::DOCK0_BYI),
        decoded_face(&status_icons::DOCK1_BYI),
        decoded_face(&status_icons::DOCK2_BYI),
        decoded_face(&status_icons::DOCK3_BYI),
        nav_face(NavGlyph::Back, NAV_BASE_BG),
        nav_face(NavGlyph::Home, NAV_BASE_BG),
        nav_face(NavGlyph::Power, NAV_BASE_BG),
        nav_face(NavGlyph::Power, NAV_ARMED_BG),
        nav_face(NavGlyph::Recents, NAV_BASE_BG),
        nav_face(NavGlyph::Rotate, NAV_BASE_BG),
    ]
}

// —— 五键导航面（波一刀 D——程序化自绘，generic_face 同先例）——

/// 导航键底块色（常态：全透明——P0-① 裁决 2026-11-17：白符号直浮
/// 壁纸、无任何底色块；argb_blit 1-bit 透明键 A==0 跳过即得穿透。
/// 裁决出处：.agents/plans/2026-11-17-图标透明与系统栏跟随综合裁决.md）。
const NAV_BASE_BG: u32 = 0x0000_0000;
/// 导航键底块色（关机确认态：暗红警示）。
const NAV_ARMED_BG: u32 = 0xFF7A_1F1F;
/// 符号白（TEXT_FG 同值系）。
const NAV_GLYPH_FG: u32 = 0xFFF0_F0F4;

/// 导航符号形（back = 左向实心三角；home = 圆环；power = 缺口圆环 +
/// 顶部竖线——电源通用形；recents = 双矩形叠层——多任务通用形；
/// rotate = 带三角头的圆弧箭头——旋转通用形）。
#[derive(Clone, Copy)]
pub(crate) enum NavGlyph {
    Back,
    Home,
    Power,
    Recents,
    Rotate,
}

/// 单符号像素判定（(x,y) → 符号白？）——纯几何，host 测试面。
pub(crate) fn nav_glyph_covers(g: NavGlyph, x: i32, y: i32) -> bool {
    let c = NAV_SRC_PX as i32 / 2; // 33
    match g {
        // 左向实心三角：顶点 (12,c)，上 (52,10)，下 (52,56)——叉积同号
        // 内测试（边长 40px，2x 放大后 80px 视觉面）。
        NavGlyph::Back => {
            let (ax, ay) = (12, c);
            let (bx, by) = (52, 10);
            let (cx, cy) = (52, 2 * c - 10);
            let d1 = (bx - ax) * (y - ay) - (by - ay) * (x - ax);
            let d2 = (cx - bx) * (y - by) - (cy - by) * (x - bx);
            let d3 = (ax - cx) * (y - cy) - (ay - cy) * (x - cx);
            let neg = d1 < 0 || d2 < 0 || d3 < 0;
            let pos = d1 > 0 || d2 > 0 || d3 > 0;
            !(neg && pos)
        }
        // 圆环：中心 (c, c+2)，外 r=21 内 r=12（带宽 9px）。
        NavGlyph::Home => {
            let dx = x - c;
            let dy = y - (c + 2);
            let d2 = dx * dx + dy * dy;
            (144..=441).contains(&d2)
        }
        // 电源形：中心 (c, c+4) 圆环外 r=20 内 r=13，顶部 70° 开口
        //（与竖向夹角 <35° 的环段不画）；竖线 x∈[c-4,c+4] y∈[c-25,c-7]。
        NavGlyph::Power => {
            let dx = x - c;
            let dy = y - (c + 4);
            let d2 = dx * dx + dy * dy;
            if (169..=400).contains(&d2) {
                // 顶部开口：与竖向（-y 向）夹角 <35° 即 |dx| < tan35°·|dy|
                // 近似（dy<0 段 |dx|·10 < 7·|dy|·… 用整数近似 tan35°≈0.70）。
                if dy < 0 && (dx.abs() as i64) * 10 < ((-dy) as i64) * 7 {
                    return false;
                }
                return true;
            }
            (x >= c - 4) && (x <= c + 4) && (y >= c - 25) && (y <= c - 7)
        }
        // 多任务形：两枚圆角矩形斜叠（后卡右上偏移 +9，前卡左下）——
        // 现代多任务通用形。命中 = 任一卡内且不在两卡重叠角内。
        NavGlyph::Recents => {
            let in_rect = |x0: i32, y0: i32| {
                x >= x0 && x < x0 + 24 && y >= y0 && y < y0 + 28
            };
            in_rect(c - 21, c - 14) || in_rect(c - 3, c - 14 + 9)
        }
        // 旋转形：3/4 圆弧（右下象限缺口）+ 弧末端三角箭头——屏幕旋转
        // 通用形。弧心 (c, c) 外 r=20 内 r=12；缺口 = 与 +x 向夹角在
        // -15°..75°（右下象限）不画；箭头三角顶点在弧右端 (c+16, c+12)
        // 附近向右上指。
        NavGlyph::Rotate => {
            let dx = x - c;
            let dy = y - c;
            let d2 = dx * dx + dy * dy;
            if (144..=400).contains(&d2) {
                // 缺口：右下象限（dx>0 且 dy>0 且斜率不过陡）。
                let steep = (dy.abs() as i64) * 10 > (dx.abs() as i64) * 27;
                if dx > 0 && dy > 0 && !steep {
                    return false;
                }
                return true;
            }
            // 箭头：弧右端 (c+16, c+12) 处的小实心三角（指向右上）。
            // 顶点 (c+18, c-2)，底边竖直 x=c+7..c+11, y=c+2..c+12。
            let ax = x - c;
            let ay = y - c;
            ax >= 7 && ax <= 18 && ay >= -2 && ay <= 12 && (ax - 7) >= (ay + 2) / 2
        }
    }
}

/// 导航面生成（66×66 Argb：圆角域内符号白、底透明（P0-①）——argb_blit
/// 直写像素、IconProps.fg 不消费；圆角 r=18 外透明）——纯像素面，host
/// 测试锚。
pub(crate) fn nav_face(glyph: NavGlyph, bg: u32) -> IconFace {
    const S: usize = NAV_SRC_PX as usize;
    const R: i32 = 18;
    let mut px = vec![0u32; S * S];
    let mut y = 0usize;
    while y < S {
        let mut x = 0usize;
        while x < S {
            // 圆角：四角 (R,R) 内距离场外 = 透明（generic_face 同形）。
            let ex = x as i32;
            let ey = y as i32;
            let in_corner =
                (ex < R || ex >= S as i32 - R) && (ey < R || ey >= S as i32 - R);
            let corner_out = in_corner
                && (if ex < R { R - ex } else { ex - (S as i32 - 1 - R) }).pow(2)
                    + (if ey < R { R - ey } else { ey - (S as i32 - 1 - R) }).pow(2)
                    > R * R;
            if !corner_out {
                px[y * S + x] = if nav_glyph_covers(glyph, x as i32, y as i32) {
                    NAV_GLYPH_FG
                } else {
                    bg
                };
            }
            x += 1;
        }
        y += 1;
    }
    IconFace { w: S as u32, h: S as u32, px }
}

/// 流中待竣工条目（ENTRY 开工 → CHUNK 供料 → 满员转竣工解码）。
pub(crate) struct Pending {
    pub id: [u8; ID_MAX],
    pub id_len: u8,
    pub label: [u8; 64],
    pub label_len: u8,
    pub blob: Vec<u8>,
}

/// APP_LIST 拉取 + 装配（FACES 同批装载）。返 None = 弃整流（诊断已打）。
#[cfg(not(test))]
fn pull(app_h: Handle) -> Option<&'static [AppEntry]> {
    let mut req = [0u8; 256];
    let m = ap::encode_list_request(&mut req);
    rpc_send(app_h, &req[..m], &[]);

    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let mut dec = ap::ListDecoder::new();
    let mut entries: Vec<AppEntry> = Vec::new();
    let mut faces: Vec<IconFace> = status_faces();
    let mut px_used: u64 = 0;
    px_used += (status_icons::SIGNAL_W as u64 * status_icons::SIGNAL_H as u64)
        + (status_icons::WIFI_W as u64 * status_icons::WIFI_H as u64)
        + (status_icons::BATTERY_W as u64 * status_icons::BATTERY_H as u64)
        + DOCK_DIMS.iter().map(|&(w, h)| w as u64 * h as u64).sum::<u64>();
    let mut pending: Option<Pending> = None;

    loop {
        let (n, nh) = rpc_recv(app_h, &mut rep, &mut rhs);
        if nh > 0 {
            // 应答流无随行 handle 面——协议破缺，即关防泄漏后弃流。
            for i in 0..nh {
                close_h(rhs[i]);
            }
            diag("app list reply carries handles");
            return None;
        }
        if let Some(()) = ap::parse_err_reply(&rep, n) {
            diag("app list st=bad");
            return None;
        }
        let frame = match dec.feed(&rep, n) {
            Some(f) => f,
            None => {
                diag(&format!("app list stream corrupt at {}", entries.len()));
                return None;
            }
        };
        match frame {
            ap::ListFrame::Entry { id, label, icon_w, icon_h, blob_len } => {
                if entries.len() >= REG_MAX {
                    diag(&format!("app list > {REG_MAX} entries"));
                    return None;
                }
                if blob_len as usize > BLOB_MAX {
                    diag(&format!("app icon blob {blob_len} > cap"));
                    return None;
                }
                let _ = (icon_w, icon_h); // 尺寸真值源 = blob 头（decode 产物）
                // 前条目已满员（decoder 执法 remain==0 才放行 ENTRY）→ 竣工。
                if let Some(pe) = pending.take() {
                    finish_entry(&mut entries, &mut faces, &mut px_used, pe);
                }
                let mut e = Pending {
                    id: [0; ID_MAX],
                    id_len: id.len() as u8,
                    label: [0; 64],
                    label_len: label.len() as u8,
                    blob: Vec::with_capacity(blob_len as usize),
                };
                e.id[..id.len()].copy_from_slice(id);
                e.label[..label.len()].copy_from_slice(label);
                pending = Some(e);
            }
            ap::ListFrame::Chunk { data, .. } => {
                match pending.as_mut() {
                    Some(pe) => pe.blob.extend_from_slice(data),
                    // decoder 已执法「无待收拒 CHUNK」——此臂结构性不可达。
                    None => {
                        diag("app chunk without entry");
                        return None;
                    }
                }
            }
            ap::ListFrame::End { .. } => {
                if let Some(pe) = pending.take() {
                    finish_entry(&mut entries, &mut faces, &mut px_used, pe);
                }
                break;
            }
        }
    }

    // 通用图标 fallback：无 [ui] 图标的 app（测试控制件/系统件）不再
    // 裸 label 浮在桌面；给一枚中性圆角 tile（48×48，1-bit alpha），
    // app 域/APPUI 记账不受影响。
    let generic_id = faces.len() as u16;
    faces.push(generic_face());
    for e in &mut entries {
        if e.icon == NO_ICON {
            e.icon = generic_id;
        }
    }

    let leaked: &'static [AppEntry] = Box::leak(entries.into_boxed_slice());
    set_faces(Box::leak(faces.into_boxed_slice()));
    Some(leaked)
}

/// 条目竣工：BYI1 两遍解码 + 稠密 id 赋值 + 常驻帽执法。
/// 拒面（magic/版本/几何/截断/超帽）→ `NO_ICON` + 诊断（label-only
/// 格降级，不弃整流——单条目坏件不连坐整桌）。
pub(crate) fn finish_entry(
    entries: &mut Vec<AppEntry>,
    faces: &mut Vec<IconFace>,
    px_used: &mut u64,
    pe: Pending,
) {
    let mut icon = NO_ICON;
    // 第一遍空缓冲探尺：头合法 → BufferTooSmall{need = blob 真值 w*h}
    //（magic/版本/flags/几何拒面也在此遍全数暴露）；need ≤ 65536 由
    // decode 几何帽执法——帽/账以 blob 头为准（ENTRY 帧尺寸域非真值源）。
    match decode(&pe.blob, &mut []) {
        Ok(_) => {} // w*h ≥ 1 恒不可达（空缓冲必 BufferTooSmall）
        Err(IconError::BufferTooSmall { need, .. }) => {
            if *px_used + need as u64 > PX_CAP {
                diag(&format!("icon residency cap hit (need {need} px) -- label only"));
            } else {
                let mut px = vec![0u32; need];
                match decode(&pe.blob, &mut px) {
                    Ok(desc) => {
                        *px_used += need as u64;
                        icon = faces.len() as u16;
                        faces.push(IconFace { w: desc.w, h: desc.h, px });
                    }
                    Err(e) => diag(&format!("icon decode {e:?}")),
                }
            }
        }
        Err(e) => diag(&format!("icon decode {e:?}")),
    }
    entries.push(AppEntry {
        id: pe.id,
        id_len: pe.id_len,
        label: pe.label,
        label_len: pe.label_len,
        icon,
    });
}

/// 通用图标面（无图标 app 的 fallback；中性石板底 + 白环，1-bit alpha）
/// ——纯像素面，host 测试锚。
pub(crate) fn generic_face() -> IconFace {
    // 108px 源：目标 162px 由 ScaledArgb 1.5x 采样（省 1MiB 堆）。
    const S: usize = 108;
    let mut px = vec![0u32; S * S];
    let mut y = 0usize;
    while y < S {
        let mut x = 0usize;
        while x < S {
            // 圆角半径 10：四角外 = 透明。
            let cx = if x < 24 { 24 - x } else if x >= S - 24 { x - (S - 25) } else { 0 };
            let cy = if y < 24 { 24 - y } else if y >= S - 24 { y - (S - 25) } else { 0 };
            if cx * cx + cy * cy > 576 {
                px[y * S + x] = 0;
                x += 1;
                continue;
            }
            let dx = x as i32 - 53;
            let dy = y as i32 - 53;
            let d2 = dx * dx + dy * dy;
            // 白环：半径 30..=40。
            px[y * S + x] = if (1225..=2025).contains(&d2) {
                0xFFFF_FFFF
            } else {
                0xFF45_5A7A
            };
            x += 1;
        }
        y += 1;
    }
    IconFace { w: S as u32, h: S as u32, px }
}

/// 应用壳图标供件（berry-ui D28 供给槽真身——`Ui::set_icon_source`
/// 消费）。
pub struct ShellIcons;

static ICONS: ShellIcons = ShellIcons;

/// 供件引用（`&'static dyn`——供给槽生命周期面；unit static 即终身）。
pub fn icon_source() -> &'static dyn IconSource {
    &ICONS
}

impl IconSource for ShellIcons {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        if id == WALLPAPER {
            return Some(WALLPAPER_DIM);
        }
        if let Some((_, _, w, h)) = bitmap_entry(id) {
            return Some((w, h));
        }
        let f = faces().get(id as usize)?;
        Some((f.w, f.h))
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        if id == WALLPAPER {
            return Some(&wallpaper::WALLPAPER_PX);
        }
        if let Some((_, px, _, _)) = bitmap_entry(id) {
            return Some(px);
        }
        let f = faces().get(id as usize)?;
        Some(&f.px)
    }

    fn smooth_scale(&self, id: u16) -> bool {
        id == WALLPAPER
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 内建面钉板：恰 13 面、id 序 = 状态三 + dock 四 + 导航六；导航源
    /// 66×66；armed 面与常态面仅底色异（Power 同符号）。
    #[test]
    fn builtin_faces_are_thirteen_and_dims_pinned() {
        let f = status_faces();
        assert_eq!(f.len(), BUILTIN_FACES);
        // 状态三面 = 生成表真值尺寸。
        assert_eq!((f[0].w, f[0].h), STATUS_SIGNAL_DIM);
        assert_eq!((f[1].w, f[1].h), STATUS_WIFI_DIM);
        assert_eq!((f[2].w, f[2].h), STATUS_BATTERY_DIM);
        // 导航六面 66×66。
        for i in 7..13 {
            assert_eq!((f[i].w, f[i].h), (NAV_SRC_PX, NAV_SRC_PX));
        }
        assert_ne!(f[9].px, f[10].px);
    }

    /// 导航符号几何探针：符号域内白/域外底块透明/圆角外透明三分
    ///（P0-① 裁决后底块 A==0——白符号直浮壁纸）。
    #[test]
    fn nav_faces_geometry_probes() {
        // 常态底透明钉板（P0-① 裁决）：NAV_BASE_BG 恒 A==0。
        assert_eq!(NAV_BASE_BG, 0);
        let back = nav_face(NavGlyph::Back, NAV_BASE_BG);
        const S: usize = NAV_SRC_PX as usize;
        let at = |f: &IconFace, x: usize, y: usize| f.px[y * S + x];
        // 圆角外 = 透明。
        assert_eq!(at(&back, 0, 0), 0);
        assert_eq!(at(&back, S - 1, S - 1), 0);
        // 域内符号白（底块透明后非 0 即符号像素——(33,33) 三角内）。
        assert_ne!(at(&back, S / 2, S / 2), 0);
        // Back = 左向三角：左中点符号白，右缘外底块透明。
        assert_eq!(at(&back, 13, 33), NAV_GLYPH_FG);
        assert_eq!(at(&back, 60, 33), 0);
        // Home 圆环：环带白、环心底块透明。
        let home = nav_face(NavGlyph::Home, NAV_BASE_BG);
        assert_eq!(at(&home, 33, 14), NAV_GLYPH_FG); // 外缘上带
        assert_eq!(at(&home, 33, 35), 0); // 环心
        // Power 顶部开口：正上环位不画（底块透明）、竖线段在位（band 外
        // y=13 ∈ [c-25,c-7] 段）。
        let power = nav_face(NavGlyph::Power, NAV_BASE_BG);
        assert_eq!(at(&power, 33, 33 + 4 - 19), 0); // 开口段
        assert_eq!(at(&power, 33, 13), NAV_GLYPH_FG); // 竖线段（band 外）
        // Recents：卡中心白。
        let recents = nav_face(NavGlyph::Recents, NAV_BASE_BG);
        assert_eq!(at(&recents, 33 - 21 + 12, 33 - 14 + 9 + 14), NAV_GLYPH_FG);
    }

    /// nav_glyph_covers 纯几何真值探针（三角内/外、圆环带内外、电源
    /// 开口段/竖线段）。
    #[test]
    fn nav_glyph_cover_probes() {
        let c = NAV_SRC_PX as i32 / 2;
        // Back 三角：内部、外部。
        assert!(nav_glyph_covers(NavGlyph::Back, 20, c));
        assert!(!nav_glyph_covers(NavGlyph::Back, 60, c));
        // Home 环：带内（r≈16）、心内（r≈4）、带外（r≈24）。
        assert!(nav_glyph_covers(NavGlyph::Home, c + 16, c + 2));
        assert!(!nav_glyph_covers(NavGlyph::Home, c, c + 2));
        assert!(!nav_glyph_covers(NavGlyph::Home, c + 24, c + 2));
        // Power：侧带在、顶部开口段出、竖线在（band 外段）。
        assert!(nav_glyph_covers(NavGlyph::Power, c + 17, c + 4));
        assert!(!nav_glyph_covers(NavGlyph::Power, c, c + 4 - 18));
        assert!(nav_glyph_covers(NavGlyph::Power, c, c + 4 - 12));
    }

    /// 通用 fallback 面：108×108、圆角外透明、白环带/石板底两色。
    #[test]
    fn generic_face_tile_shape() {
        let f = generic_face();
        assert_eq!((f.w, f.h), (108, 108));
        assert_eq!(f.px.len(), 108 * 108);
        let at = |x: usize, y: usize| f.px[y * 108 + x];
        assert_eq!(at(0, 0), 0); // 圆角透明
        // 环带（d2 ∈ 1225..=2025 → 白）；石板底（d2 小）。
        assert_eq!(at(53 + 35, 53), 0xFFFF_FFFF);
        assert_eq!(at(53, 53), 0xFF45_5A7A);
    }

    /// decoded_face 拒面：坏 blob → 1×1 降级面（不炸、px 单像素）。
    #[test]
    fn decoded_face_bad_blob_degrades() {
        let f = decoded_face(&[0u8; 8]);
        assert_eq!((f.w, f.h), (1, 1));
        assert_eq!(f.px.len(), 1);
    }

    /// 注册表缺省态：未装载 = 空切片（boot 门未过的常态面）；AppEntry
    /// 有效字节切片语义。
    #[test]
    fn registry_default_empty_and_entry_slices() {
        // 本测试不写 REGISTRY（boot 单写面）——只验读缺省。若同进程其他
        // 测试已装载（本 crate 测试面无人调 init/pull——cfg(not(test))
        // 门掉），此处恒空。
        if REGISTRY_PTR.load(Ordering::Acquire).is_null() {
            assert!(registry().is_empty());
        }
        let mut e = AppEntry {
            id: [0; ID_MAX],
            id_len: 0,
            label: [0; 64],
            label_len: 0,
            icon: NO_ICON,
        };
        e.id[..7].copy_from_slice(b"APPDEMO");
        e.id_len = 7;
        e.label[..3].copy_from_slice(b"App");
        e.label_len = 3;
        assert_eq!(e.id_bytes(), b"APPDEMO");
        assert_eq!(e.label_bytes(), b"App");
    }

    /// 壁纸顶区采样：行数钳制（min(240, req)）、切片长 = 宽×行、供件
    /// 壁纸特例三面。
    #[test]
    fn wallpaper_top_rows_clamps() {
        let (px, w) = wallpaper_top_rows(15);
        assert_eq!(w, WALLPAPER_DIM.0);
        assert_eq!(px.len(), 135 * 15);
        let (px_all, _) = wallpaper_top_rows(1000);
        assert_eq!(px_all.len(), 135 * 240);
        assert!(icon_source().smooth_scale(WALLPAPER));
        assert_eq!(icon_source().dims(WALLPAPER), Some(WALLPAPER_DIM));
        assert!(icon_source().pixels(WALLPAPER).is_some());
        // 未装载 FACES：app 域 id 查 None（真供件契约——debug_assert 面
        // 的 release 语义）。
        assert!(icon_source().dims(0).is_none());
    }

    /// 条目竣工拒面：坏 blob（非 BYI 头）→ NO_ICON 条目照常入册（不弃
    /// 整流的单条降级语义）；帽账零增。
    #[test]
    fn finish_entry_bad_blob_is_label_only() {
        let mut entries = Vec::new();
        let mut faces = status_faces();
        let mut used = 0u64;
        let mut pe = Pending {
            id: [0; ID_MAX],
            id_len: 7,
            label: [0; 64],
            label_len: 3,
            blob: Vec::new(),
        };
        pe.id[..7].copy_from_slice(b"APPDEMO");
        pe.blob.extend_from_slice(&[0u8; 16]); // 坏头
        finish_entry(&mut entries, &mut faces, &mut used, pe);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].icon, NO_ICON);
        assert_eq!(used, 0);
    }
}
