//! icons —— app 注册表 + 图标供给桥（GUI-8 段③ W2，docs/
//! GUI-8实施设计草案.md §4 段③「IconSource 供件桥接」）。
//!
//! 责任：boot 期 APP_LIST 拉取（S3 蓝图：**一请求 → 分片流**——svcmgr
//! `app_serve_list` 对单个 APP_LIST_REQ 发整条 ENTRY/CHUNK/END 流，应答
//! 逐帧到账为独立 channel 消息 → 客户端 send-once + recv-loop）→
//! `ListDecoder` 三执法（idx 单调 +1 / blob 收支 / END 对账——任何 None
//! 弃整流，半张清单不进渲染面）→ `AppEntry` 装配 + BYI1 两遍解码 →
//! `LauncherIcons`（berry-ui `IconSource` 真供件）。
//!
//! 冻结相（R1 修约 1/2）：launcher extras 恰 [wm] → `app_cli_h()==0`
//! → `init` 即返（零 RPC 零打印——空注册表常态，desktop 全占位格）。
//!
//! **图标 id 稠密域契约**：仅解码成功的条目按序编 0..n 稠密 id
//! （`NO_ICON` = 域外哨兵）——desktop 只对域内 id 建 Icon 节点；
//! `LauncherIcons` 是**真供件**（`is_empty()==false`）：berry-ui 契约下
//! 真供件对未注册 id 返 None = debug_assert loud（layout.rs icon_dims /
//! paint.rs emit_image 两面同律）。稠密赋值即一致性构造：id ∈ 域 ⟺
//! FACES[id] 在册——两面查表恒 Some，loud 面结构性不可达。
//!
//! **像素常驻帽**（草案 kill criteria ③ 回落）：Σ w*h ≤ 32MiB/4 像素
//! ——超帽条目降 `NO_ICON`（label-only 格）+ eprintln 诊断，不崩不静默
//! 吞。boot 全量常驻（无 LRU 换入换出——D28 供给槽是借用面，缓存编排
//! 归消费侧；berry-gfx `IconLruCache` 服务 GlyphBits mono 面非本面）。
//!
//! 静态面：`OnceLock` 装载（免 static mut 直写 + 单写幂等——boot 单线程
//! 无竞态面）；像素/条目 `Box::leak` 堆置（elf2flat 会把 .bss 物化进平像
//! ——svcmgr 单页墙判例：禁大 static 解码 scratch，解码缓冲走堆 Vec）。

use std::sync::OnceLock;

use berry_appproto as ap;
use berry_gfx::font::FontSize;
use berry_gfx::icon::{decode, IconError};
use berry_layout::ID_MAX;
use berry_libc::io::self_vmar_h;
use berry_sys::{
    berry_handle_close, berry_vmar_map, berry_vmo_commit, berry_vmo_create,
    berry_vmo_write, Handle, RIGHT_READ, RIGHT_WRITE,
};
use berry_ui::navigation::{
    NavigationIconSource, NAV_ICON_BACK, NAV_ICON_HOME, NAV_ICON_TASK,
};
use berry_ui::IconSource;

use crate::{rpc_recv, rpc_send};

#[path = "status_icons.rs"]
mod status_icons;

#[path = "wallpaper.rs"]
mod wallpaper;

/// 内建状态/dock 图标 id（desktop 直建 Icon 节点；位于 app 图标稠密域
/// 前缀，app id 从 7 起——launcher 是唯一消费者，契约不外泄）。
/// 静态壁纸源 id（稠密 app 域外的唯一固定特例——rodata 直供零堆）。
pub const WALLPAPER: u16 = 0xFFFE;
pub const WALLPAPER_DIM: (u32, u32) = (wallpaper::WALLPAPER_W, wallpaper::WALLPAPER_H);
/// dock 分段内建面 id（0..3 号 app 对应 Contacts/Photos/Settings/MMS 官方顺序）。
pub const DOCK0: u16 = 3;
pub const DOCK1: u16 = 4;
pub const DOCK2: u16 = 5;
pub const DOCK3: u16 = 6;
pub const DOCK_FACE_IDS: [u16; 4] = [DOCK0, DOCK1, DOCK2, DOCK3];
pub const DOCK0_DIM: (u32, u32) = (status_icons::DOCK0_W, status_icons::DOCK0_H);
pub const DOCK1_DIM: (u32, u32) = (status_icons::DOCK1_W, status_icons::DOCK1_H);
pub const DOCK2_DIM: (u32, u32) = (status_icons::DOCK2_W, status_icons::DOCK2_H);
pub const DOCK3_DIM: (u32, u32) = (status_icons::DOCK3_W, status_icons::DOCK3_H);
pub const DOCK_DIMS: [(u32, u32); 4] = [DOCK0_DIM, DOCK1_DIM, DOCK2_DIM, DOCK3_DIM];
const BUILTIN_FACES: usize = 7;
const _: () = assert!(BUILTIN_FACES == 7, "状态三 + dock 四内建面钉板");

/// 无图标哨兵（icon 域外值——desktop 不建 Icon 节点）。
pub const NO_ICON: u16 = u16::MAX;

/// 注册表条目帽（防御面：wire 无条目数先验，超帽 = 协议滥用 → 弃整流；
/// CELL_MAX=48 恰满 + 裕量）。
const REG_MAX: usize = 64;

/// 单条目 BYI1 blob 帽（wire blob_len u32 先验拒面：w*h ≤ 65536 px 的
/// 最坏 RLE 流 ≈ 384KiB，512KiB 帽 = 慷慨上界；超帽弃整流）。
const BLOB_MAX: usize = 512 * 1024;

/// 像素常驻帽（225'000 px ≈ 879KiB——launcher 静态堆实为 1MiB；超帽
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

/// boot 注册表（未装载 = 空切片——冻结相常态）。
static REGISTRY: OnceLock<&'static [AppEntry]> = OnceLock::new();

/// 注册表只读面（未装载 = 空切片）。
pub fn registry() -> &'static [AppEntry] {
    REGISTRY.get().copied().unwrap_or(&[])
}

/// 注册表装载（幂等——首装定局，二次 set 恒保首装）。
fn set_registry(entries: &'static [AppEntry]) {
    let _ = REGISTRY.set(entries);
}

/// 解码产物（堆常驻像素面；`Box::leak` 外层切片即整体终身——内层 Vec
/// 随之永不回收 = 分配即常驻，boot 单次无增长面）。
struct IconFace {
    w: u32,
    h: u32,
    px: Vec<u32>,
}

/// 稠密 id → 解码面（`REGISTRY` 同批装配——两表索引一致性由构造序保证）。
static FACES: OnceLock<&'static [IconFace]> = OnceLock::new();

/// boot 期初始化（main 早期单次调用）。`app_h == 0`（冻结相 extras 恰
/// [wm]）→ 即返空注册表零 RPC；在场 → APP_LIST 拉取装载（失败 = eprintln
/// 诊断 + 空注册表——半张清单不进渲染面，launcher 不死）。
pub fn init(app_h: Handle) {
    if app_h == 0 {
        return;
    }
    match pull(app_h) {
        Some(entries) => set_registry(entries),
        None => {
            eprintln!("launcher: app list unavailable -- empty registry");
            set_registry(Box::leak(Vec::new().into_boxed_slice()));
            let _ = FACES.set(Box::leak(status_faces().into_boxed_slice()));
        }
    }
}

/// 宿主生成期 BYI1 → 运行期 IconFace（dock 大面走 RLE，launcher 二进制
/// 只携压缩字节；decode 契约与 APP_LIST 图标同源）。
fn decoded_face(blob: &[u8]) -> IconFace {
    let need = match decode(blob, &mut []) {
        Err(IconError::BufferTooSmall { need, .. }) => need,
        _ => {
            eprintln!("launcher: builtin icon probe bad");
            return IconFace { w: 1, h: 1, px: vec![0] };
        }
    };
    let mut px = vec![0u32; need];
    match decode(blob, &mut px) {
        Ok(desc) => IconFace { w: desc.w, h: desc.h, px },
        Err(e) => {
            eprintln!("launcher: builtin icon decode {e:?}");
            IconFace { w: 1, h: 1, px: vec![0] }
        }
    }
}

/// 状态栏三内建面 + dock 四分段面。
fn status_faces() -> Vec<IconFace> {
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
    ]
}

/// 流中待竣工条目（ENTRY 开工 → CHUNK 供料 → 满员转竣工解码）。
struct Pending {
    id: [u8; ID_MAX],
    id_len: u8,
    label: [u8; 64],
    label_len: u8,
    blob: Vec<u8>,
}

/// APP_LIST 拉取 + 装配（FACES 同批装载）。返 None = 弃整流（诊断已打）。
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
                crate::close_h(rhs[i]);
            }
            eprintln!("launcher: app list reply carries handles");
            return None;
        }
        if let Some(()) = ap::parse_err_reply(&rep, n) {
            eprintln!("launcher: app list st=bad");
            return None;
        }
        let frame = match dec.feed(&rep, n) {
            Some(f) => f,
            None => {
                eprintln!("launcher: app list stream corrupt at {}", entries.len());
                return None;
            }
        };
        match frame {
            ap::ListFrame::Entry { id, label, icon_w, icon_h, blob_len } => {
                if entries.len() >= REG_MAX {
                    eprintln!("launcher: app list > {REG_MAX} entries");
                    return None;
                }
                if blob_len as usize > BLOB_MAX {
                    eprintln!("launcher: app icon blob {blob_len} > cap");
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
                        eprintln!("launcher: app chunk without entry");
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
    // 裸 label 浮在桌面；给一枚中性圆角 tile（48×48，8-bit alpha），
    // app 域/APPUI 记账不受影响。
    let generic_id = faces.len() as u16;
    faces.push(generic_face());
    for e in &mut entries {
        if e.icon == NO_ICON {
            e.icon = generic_id;
        }
    }

    let leaked: &'static [AppEntry] = Box::leak(entries.into_boxed_slice());
    let _ = FACES.set(Box::leak(faces.into_boxed_slice()));
    Some(leaked)
}

/// 条目竣工：BYI1 两遍解码 + 稠密 id 赋值 + 常驻帽执法。
/// 拒面（magic/版本/几何/截断/超帽）→ `NO_ICON` + eprintln（label-only
/// 格降级，不弃整流——单条目坏件不连坐整桌）。
fn finish_entry(entries: &mut Vec<AppEntry>, faces: &mut Vec<IconFace>, px_used: &mut u64, pe: Pending) {
    let mut icon = NO_ICON;
    // 第一遍空缓冲探尺：头合法 → BufferTooSmall{need = blob 真值 w*h}
    //（magic/版本/flags/几何拒面也在此遍全数暴露）；need ≤ 65536 由
    // decode 几何帽执法——帽/账以 blob 头为准（ENTRY 帧尺寸域非真值源）。
    match decode(&pe.blob, &mut []) {
        Ok(_) => {} // w*h ≥ 1 恒不可达（空缓冲必 BufferTooSmall）
        Err(IconError::BufferTooSmall { need, .. }) => {
            if *px_used + need as u64 > PX_CAP {
                eprintln!("launcher: icon residency cap hit (need {need} px) -- label only");
            } else {
                let mut px = vec![0u32; need];
                match decode(&pe.blob, &mut px) {
                    Ok(desc) => {
                        *px_used += need as u64;
                        icon = faces.len() as u16;
                        faces.push(IconFace { w: desc.w, h: desc.h, px });
                    }
                    Err(e) => eprintln!("launcher: icon decode {e:?}"),
                }
            }
        }
        Err(e) => eprintln!("launcher: icon decode {e:?}"),
    }
    entries.push(AppEntry {
        id: pe.id,
        id_len: pe.id_len,
        label: pe.label,
        label_len: pe.label_len,
        icon,
    });
}

/// 通用图标面（无图标 app 的 fallback；中性石板底 + 白环，8-bit alpha）。
fn generic_face() -> IconFace {
    // 108px 源：目标 162px 由 SmoothArgb 1.5x 双线性采样（省 1MiB 堆）。
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

/// launcher 图标供件（berry-ui D28 供给槽真身——`Ui::set_icon_source`
/// 消费）。
pub struct LauncherIcons;

static ICONS: LauncherIcons = LauncherIcons;

struct WallpaperCache {
    w: u32,
    h: u32,
    px: &'static [u32],
    _vmo: Handle,
}

const WALLPAPER_VA: usize = 0x5200_0000;

static WALLPAPER_CACHE: OnceLock<WallpaperCache> = OnceLock::new();

/// 把官方 RGB565 壁纸按当前视口一次性双线性预缩放成不透明 ARGB。
/// Swipe 每帧 damage 包络结构性覆盖全宽；无缓存时同一 2.07M 像素
/// 双线性采样会在每个动画帧重复执行。缓存是视觉等价换内存（约 8.3MiB@1080p）。
pub fn prime_wallpaper(vp_w: u32, vp_h: u32) {
    if WALLPAPER_CACHE.get().is_some() || vp_w == 0 || vp_h == 0 {
        return;
    }
    let pixels = vp_w as usize * vp_h as usize;
    let Some(len) = (pixels * 4).checked_add(0xfff).map(|v| v & !0xfff) else {
        return;
    };
    let mut vmo: Handle = 0;
    if berry_vmo_create(len as u64, 0, &mut vmo) != 0
        || vmo == 0
        || berry_vmo_commit(vmo, 0, len as u64) != 0
    {
        let _ = berry_handle_close(vmo);
        return;
    }
    let hv = self_vmar_h();
    let mut va = WALLPAPER_VA;
    if hv == 0
        || berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0
        || va != WALLPAPER_VA
    {
        let _ = berry_handle_close(vmo);
        return;
    }
    // SAFETY: [WALLPAPER_VA, +len) 刚按页映射并 commit；launcher 终身持有
    // vmo 与映射，px 只取前 w*h 像素，不含页尾。
    let out: &mut [u32] = unsafe { core::slice::from_raw_parts_mut(va as *mut u32, pixels) };
    smooth_wallpaper_into_argb(out, vp_w, vp_h);
    let px: &'static [u32] = unsafe { core::slice::from_raw_parts(va as *const u32, pixels) };
    let _ = WALLPAPER_CACHE.set(WallpaperCache {
        w: vp_w,
        h: vp_h,
        px,
        _vmo: vmo,
    });
}

fn smooth_wallpaper_into_argb(out: &mut [u32], dw: u32, dh: u32) {
    const MAX_W: usize = 1280;
    const SW: u32 = wallpaper::WALLPAPER_W;
    const SH: u32 = wallpaper::WALLPAPER_H;
    if dw as usize > MAX_W || out.len() < dw as usize * dh as usize {
        return;
    }

    let unpack = |v: u16| -> (u8, u8, u8) {
        let r5 = u32::from((v >> 11) & 0x1F);
        let g6 = u32::from((v >> 5) & 0x3F);
        let b5 = u32::from(v & 0x1F);
        (
            ((r5 * 255 + 15) / 31) as u8,
            ((g6 * 255 + 31) / 63) as u8,
            ((b5 * 255 + 15) / 31) as u8,
        )
    };
    let mut xs0 = [0u16; MAX_W];
    let mut xs1 = [0u16; MAX_W];
    let mut fxs = [0u8; MAX_W];
    for i in 0..dw as usize {
        let cx = ((2 * i as i64 + 1) * SW as i64 * 65536 / (2 * dw as i64)) - 32768;
        let x0 = (cx / 65536).clamp(0, (SW - 1) as i64) as usize;
        let x1 = (x0 + 1).min((SW - 1) as usize);
        xs0[i] = x0 as u16;
        xs1[i] = x1 as u16;
        fxs[i] = ((cx - x0 as i64 * 65536).max(0) / 256) as u8;
    }
    let mut row0 = [(0u8, 0u8, 0u8); MAX_W];
    let mut row1 = [(0u8, 0u8, 0u8); MAX_W];
    let mut cached0 = usize::MAX;
    let mut cached1 = usize::MAX;
    for y in 0..dh as usize {
        let cy = ((2 * y as i64 + 1) * SH as i64 * 65536 / (2 * dh as i64)) - 32768;
        let y0 = (cy / 65536).clamp(0, (SH - 1) as i64) as usize;
        let y1 = (y0 + 1).min((SH - 1) as usize);
        let fy = ((cy - y0 as i64 * 65536).max(0) / 256) as u32;
        if cached0 != y0 {
            let base = y0 * SW as usize;
            for i in 0..dw as usize {
                row0[i] = horiz_rgb565(&wallpaper::WALLPAPER_PX, base, xs0[i], xs1[i], fxs[i], unpack);
            }
            cached0 = y0;
        }
        if cached1 != y1 {
            if y1 == y0 {
                row1 = row0;
            } else {
                let base = y1 * SW as usize;
                for i in 0..dw as usize {
                    row1[i] = horiz_rgb565(&wallpaper::WALLPAPER_PX, base, xs0[i], xs1[i], fxs[i], unpack);
                }
            }
            cached1 = y1;
        }
        let inv_y = 256 - fy;
        let dst_row = y * dw as usize;
        for i in 0..dw as usize {
            let (r0, g0, b0) = row0[i];
            let (r1, g1, b1) = row1[i];
            let r = (u32::from(r0) * inv_y + u32::from(r1) * fy) >> 8;
            let g = (u32::from(g0) * inv_y + u32::from(g1) * fy) >> 8;
            let b = (u32::from(b0) * inv_y + u32::from(b1) * fy) >> 8;
            out[dst_row + i] = 0xFF00_0000 | (r << 16) | (g << 8) | b;
        }
    }

    overlay_product_navigation(out, dw as usize, dh as usize);
}

/// Product wallpaper no longer bakes the navigation band. The launcher-owned
/// SYSTEM navigation overlay supplies its translucent scrim and AOSP glyphs.
fn overlay_product_navigation(_out: &mut [u32], _dw: usize, _dh: usize) {}

fn horiz_rgb565(
    px: &[u16],
    base: usize,
    x0: u16,
    x1: u16,
    fx: u8,
    unpack: impl Fn(u16) -> (u8, u8, u8),
) -> (u8, u8, u8) {
    let (r0, g0, b0) = unpack(px.get(base + x0 as usize).copied().unwrap_or(0));
    let (r1, g1, b1) = unpack(px.get(base + x1 as usize).copied().unwrap_or(0));
    let fx = u32::from(fx);
    let inv = 256 - fx;
    (
        ((u32::from(r0) * inv + u32::from(r1) * fx) >> 8) as u8,
        ((u32::from(g0) * inv + u32::from(g1) * fx) >> 8) as u8,
        ((u32::from(b0) * inv + u32::from(b1) * fx) >> 8) as u8,
    )
}

const OVERLAY_MAX_W: usize = 1920;
const STATUS_CLOCK: &[u8] = b"12:00";

/// High-contrast system glyph tint for both light app chrome and wallpaper.
const SYSTEM_INK: u32 = 0x1B2540;

fn blit_argb(
    row: &mut [u32], x0: usize, y0: usize, w: usize, h: usize, face: &[u32],
) {
    for y in 0..h {
        if y0 + y >= row.len() {
            return;
        }
        for x in 0..w {
            let src = face.get(y * w + x).copied().unwrap_or(0);
            let sa = u32::from((src >> 24) & 0xFF);
            if sa == 0 {
                continue;
            }
            let x = x0 + x;
            let Some(dst) = row.get_mut(x) else { continue };
            let inv = 255 - sa;
            let sr = (SYSTEM_INK >> 16) & 0xFF;
            let sg = (SYSTEM_INK >> 8) & 0xFF;
            let sb = SYSTEM_INK & 0xFF;
            let dr = u32::from((*dst >> 16) & 0xFF);
            let dg = u32::from((*dst >> 8) & 0xFF);
            let db = u32::from(*dst & 0xFF);
            let r = (sr * sa + dr * inv) / 255;
            let g = (sg * sa + dg * inv) / 255;
            let b = (sb * sa + db * inv) / 255;
            let a = u32::from((*dst >> 24) & 0xFF).max(sa);
            *dst = (a << 24) | (r << 16) | (g << 8) | b;
        }
    }
}

/// Build the transparent launcher-owned status overlay in a bar-local VMO.
pub fn write_system_status_overlay(vmo: Handle, w: u32, h: u32) -> bool {
    let w = w as usize;
    let h = h as usize;
    if w == 0 || h == 0 || w > OVERLAY_MAX_W {
        return false;
    }
    let mut row = [0u32; OVERLAY_MAX_W];
    if h >= 45 {
        let y = (h - 45) / 2;
        blit_argb(&mut row, 72, y, status_icons::SIGNAL_W as usize, status_icons::SIGNAL_H as usize, &status_icons::SIGNAL_PX);
        blit_argb(&mut row, 72 + status_icons::SIGNAL_W as usize + 15, y, status_icons::WIFI_W as usize, status_icons::WIFI_H as usize, &status_icons::WIFI_PX);
        blit_argb(&mut row, w.saturating_sub(72 + status_icons::BATTERY_W as usize), y, status_icons::BATTERY_W as usize, status_icons::BATTERY_H as usize, &status_icons::BATTERY_PX);
    }
    let atlas = FontSize::S32x48.atlas();
    let cell_w = FontSize::S32x48.cell_w();
    let cell_h = FontSize::S32x48.cell_h();
    if h >= cell_h {
        let x0 = w.saturating_sub(72 + STATUS_CLOCK.len() * cell_w);
        let y0 = (h - cell_h) / 2;
        for (index, byte) in STATUS_CLOCK.iter().enumerate() {
            let Some(coverage) = atlas.coverage(*byte) else { continue };
            for gy in 0..coverage.h {
                for gx in 0..coverage.w {
                    let alpha = coverage.coverage[gy * coverage.w + gx];
                    if alpha == 0 {
                        continue;
                    }
                    let x = x0 + index * cell_w + gx;
                    let y = y0 + gy;
                    if x >= w || y >= h {
                        continue;
                    }
                    let old = row[x];
                    let a = u32::from(alpha).max(u32::from((old >> 24) & 0xFF));
                    row[x] = (a << 24) | SYSTEM_INK;
                }
            }
        }
    }
    for y in 0..h {
        let offset = u64::from(w as u32) * 4 * y as u64;
        if berry_vmo_write(vmo, row.as_ptr().cast(), offset, w * 4) != 0 {
            return false;
        }
    }
    true
}

/// Build the translucent Android-style navigation overlay in a bar-local VMO.
pub fn write_system_navigation_overlay(vmo: Handle, w: u32, h: u32) -> bool {
    let w = w as usize;
    let h = h as usize;
    if w == 0 || h == 0 || w > OVERLAY_MAX_W {
        return false;
    }
    let mut row = [0u32; OVERLAY_MAX_W];
    if h >= 80 {
        for (id, x0) in [
            (NAV_ICON_BACK, 144usize),
            (NAV_ICON_HOME, 504),
            (NAV_ICON_TASK, 864),
        ] {
            if let Some(face) = NavigationIconSource.system_pixels(id) {
                blit_argb(&mut row, x0, 8, 72, 72, face);
            }
        }
    }
    for y in 0..h {
        let offset = u64::from(w as u32) * 4 * y as u64;
        if berry_vmo_write(vmo, row.as_ptr().cast(), offset, w * 4) != 0 {
            return false;
        }
    }
    true
}

/// 供件引用（`&'static dyn`——供给槽生命周期面；unit static 即终身）。
pub fn icon_source() -> &'static dyn IconSource {
    &ICONS
}

impl IconSource for LauncherIcons {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        if id == WALLPAPER {
            return WALLPAPER_CACHE.get().map(|c| (c.w, c.h)).or(Some(WALLPAPER_DIM));
        }
        let faces = FACES.get().copied().unwrap_or(&[]);
        let f = faces.get(id as usize)?;
        Some((f.w, f.h))
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        if id == WALLPAPER {
            return WALLPAPER_CACHE.get().map(|c| c.px);
        }
        let faces = FACES.get().copied().unwrap_or(&[]);
        let f = faces.get(id as usize)?;
        Some(&f.px)
    }

    fn pixels_rgb565(&self, id: u16) -> Option<&[u16]> {
        if id == WALLPAPER && WALLPAPER_CACHE.get().is_none() {
            return Some(&wallpaper::WALLPAPER_PX);
        }
        None
    }

    fn smooth_scale(&self, _id: u16) -> bool {
        true
    }
}

#[cfg(test)]
mod wallpaper_tests {
    use super::*;

    #[test]
    fn wallpaper_cache_is_opaque_and_viewport_sized() {
        let mut px = vec![0; 8 * 7];
        smooth_wallpaper_into_argb(&mut px, 8, 7);
        assert_eq!(px.len(), 56);
        assert!(px.iter().all(|p| p & 0xFF00_0000 == 0xFF00_0000));
    }

    #[test]
    fn product_wallpaper_does_not_bake_navigation_overlay() {
        let mut px = vec![0x1234_5678u32; 1080 * 1920];
        let before = px.clone();
        overlay_product_navigation(&mut px, 1080, 1920);
        assert_eq!(px, before, "SYSTEM overlay owns navigation pixels");
    }
}
