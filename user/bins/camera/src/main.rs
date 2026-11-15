//! camera —— 波 4 第五件真 GUI 应用（CAMERA appstub 秒退壳替换；
//! `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.5，侦察/
//! 对账底册 = worktree 根 `W4-RECON.md`）。
//!
//! **页面树**（单窗全幅取景——v1 全幅窗近似，W4-RECON §四）：
//!
//! - 取景器：全幅 Image（程序化模拟景——渐变天 + 落日/山影/流云（后置）
//!   或室内渐变 + 面盘 + 波克光斑（前置），`IconSource` 真供件、帧节拍
//!   微动 = slab 重生成 + `invalidate`）+ 三分网格线（取景辅助线 = 纯
//!   装饰 Rect，非卡片拼装）。
//! - 顶行控制带：`pattern::icon_button`(Flash) + 标题行 "Flash Auto/
//!   Off"；`pattern::icon_button`(Front/Back) + 标题行 "Back/Front"——
//!   切换即时重生成模拟景（补光提升/母题换景）。
//! - 底行控制带：大圆快门（168px 程序化白环 Image + 200px 外扩按压
//!   暗框 chip）+ 瞬态行 "Saved n"（2s 帧循环节拍自撤）。
//!
//! **铁律执法**：按压 = 引擎 `set_press_map`（`PRESSED_BG` 单源换底——
//! v1 无 alpha 通道，呈现为控件四周暗框；0.95 缩放档未落，见模块尾
//! 「v1 降解」）；控制钮 = `pattern::icon_button` 配方；骨架 =
//! `pattern::stack`/`spacer`——应用零手写 press 字面量、零手拼 Rect 卡片；
//! **全屏白 Rect = 设计稿 §2.5 明载的快门白闪特效**（单帧、非拼卡、
//! 不走 press/遮罩/圆角三禁区）；文案全 ASCII（词汇表 Camera 域：
//! Flash/Auto/Off/Front/Back/Saved）。
//!
//! **链形态** = settings/photos crate 镜像（W4-RECON §1 先例）：wm 门控
//!（本 app 带 fs 词 → wm 礼物落 extras[2]，经同槽访问器
//! `berry_libc::io::wm_present_h` 取用；0 = 无 GUI 礼物 → 一行诊断 +
//! park 常驻）→ WM_GETINFO → present 三部曲 → 帧 VMO 三连 →
//! INPUT_POLL 批拉 + `pump_input`（referee + press_react/widget_react
//! 同拍）→ MsgBuf 消费（快门 Click/DragEnd 暗框复位）→ `take_events`
//!（Flash/Flip 切换）→ `render_frame` damage 驱动零空转。RPC 半程族
//!（`rpc_send`/`rpc_recv`/`rpc`）与 `damage_envelope`/`input_arm` 均
//! launcher/settings/photos 同形拷贝。
//!
//! **持久化**：CAMERA.JSN（FAT 数据盘根，`launcher layout_io` 食谱自实现
//! ——appshell::conf 未落，W4-RECON §四降解）：`{"schema":1,"count":n,
//! "last":t}` 快门计数 + 最近时戳 ≤64B 单簇近原子；三约束纪律继承
//!（缺席回默认 + 坏读自愈回写 + 写失败非致命 eprintln 常驻不死）；
//! `fs_h()==0` 结构性早退零 RPC。诚实边界：无传感器——取景器为程序化
//! 模拟景、快门不产图档（BYI 编码器 + 卷容量预算挂 v2）；Flash Auto
//! 无闪光硬件 → 呈现为模拟景补光提升 + 快门白闪节拍面。
//!
//! **v1 降解**（3b/波 2 未落面的就地替代，全清单见 W4-IMPL-CAMERA.md）：
//! 无 `WM_CREATE_F_IMMERSIVE` → 单窗全幅近似（surface 全幅取景常驻，
//! 无状态栏隐藏/避让/回显语义）；无 wm back 路由 → 单页应用无退出路径
//!（settings park 先例同形，fail = eprintln + park，崩溃监督归
//! svcmgr）；Toast 未落 → 瞬态行 Saved n（帧循环节拍 2s 自撤）；theme
//! 样式族（card_style/oh_press_bg/OH_PRESS_SCALE_MIDDLE_PERMIL=950）
//! 未落 → 引擎 press_map 等价面（`PRESSED_BG` 换底）。

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
    ColProps, ImageProps, Msg, NodeId, Props, RectProps, RowProps, TextBuf, TextProps, TextStyle,
    ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::pattern::{self, TOPBAR_H};
use berry_ui::theme::{Theme, Tokens};
use berry_ui::{damage, frame, hit, metrics, UiError, UiMain, ROLE_BUTTON, ROLE_IMAGE, ROLE_TEXT};

// 链接种子（fsmgr/guifbd/berry-launcher/settings/photos 同款）：rlib/LTO
// 按需保留——无引用则 berry-libc 的 crt0（含 `_start`）不进链接。
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

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒；本应用
/// 取景微动常驻 → 实际等待恒 ≤ MOTION_STEP_NS）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// glyph scratch 容量（堆置——paint 对超容 run 截断尾字形；本应用文案
/// 面（标题行/瞬态行）≤16 字形富余）。
const SCRATCH_GLYPHS: usize = 256;

// —— 应用几何（px @3x；pattern 常量族之外的拟定位，单源本块）——
/// 控制带水平留白（16vp）。
const SIDE_PAD: u32 = 48;
/// 底行控制带高（快门外扩按压框 200 + 上下各 32）。
const BOTBAR_H: u32 = 264;
/// 快门钮边长（56vp 白环）。
const SHUTTER_SZ: u32 = 168;
/// 快门按压暗框外扩边长（快门外 16px 暗框带 = 按压可见域）。
const SHUTTER_MASK_SZ: u32 = 200;
/// 三分网格线厚（px）。
const GRID_T: u32 = 2;
/// 瞬态行与快门间缝（16vp）。
const SAVED_GAP: u32 = 48;
/// Flash 标题定宽列（最长 "Flash Auto" 10 字 × S16x32 字格）。
const FLASH_CAP_W: u32 = 160;
/// Front/Back 标题定宽列（最长 "Front" 5 字 × S16x32 字格）。
const FLIP_CAP_W: u32 = 80;
/// 瞬态行定宽列（"Saved " + 6 位计数 = 12 字 × S24x48 字格——数字进位
/// 不推快门位、超 6 位域不溢列）。
const SAVED_W: u32 = 288;

// —— 拍拍节拍（帧循环 app 侧定时——引擎无通用 timer 面，deadline 自管）——
/// 取景微动步进（~10fps：模拟景重生成 + invalidate；活取景器常驻 redraw
/// 是取景器语义本义，功耗面如实记账见报告）。
const MOTION_STEP_NS: u64 = 100 * 1_000_000;
/// 瞬态行 Saved n 驻留（设计稿 §2.5「2s」直录）。
const SAVED_NS: u64 = 2 * 1_000_000_000;

// —— 事件 ID 面（props on_tap 携载——take_events 消费锚；快门走
//    Msg::Click 路由（Image 非 widget，photos 缩略格同形））——
const EV_FLASH: u32 = 1;
const EV_FLIP: u32 = 2;

/// Flash/Front-Back 钮的图标字形 id（引擎 v1 Fab paint 不消费 icon id
/// ——恒绘 '+' 圆钮；id 值仅为 a11y/语义锚，域别于 pattern::icons
/// 0x01xx 约定域与取景/快门资源域——photos ICON_STAR/TRASH 先例同形）。
const ICON_FLASH: u32 = 0x0142;
const ICON_FLIP: u32 = 0x0143;

// —— 程序化供件面（零资产增量——设计稿 §3.3「取景器全部程序化生成」）——
/// 取景模拟景资源 id（ImageProps res 域；避开 pattern::icons 约定域）。
const SCENE_ID: u16 = 0x2200;
/// 快门白环资源 id。
const SHUTTER_ID: u16 = 0x2201;
/// 模拟景内在分辨率（9:16 竖档；全幅显示经 SmoothArgb 双线性上采样
/// ——壁纸/样张同律；230,400 px 重生成 ~10fps 纯整数算术轻担）。
const SCENE_W: u32 = 360;
const SCENE_H: u32 = 640;
const SCENE_PX: usize = SCENE_W as usize * SCENE_H as usize;
/// 快门环内在边长 = 显示边长（1:1 Argb 直载）。
const SHUTTER_PX: usize = SHUTTER_SZ as usize * SHUTTER_SZ as usize;

/// RGB 三元组。
type Rgb = [u8; 3];

/// 双色整数插值（`t` 0..=256——Q8 定点，无浮点；photos 同形）。
fn lerp3(a: Rgb, b: Rgb, t: i32) -> Rgb {
    [
        (a[0] as i32 + (b[0] as i32 - a[0] as i32) * t / 256) as u8,
        (a[1] as i32 + (b[1] as i32 - a[1] as i32) * t / 256) as u8,
        (a[2] as i32 + (b[2] as i32 - a[2] as i32) * t / 256) as u8,
    ]
}

/// ARGB 打包（0xAARRGGBB——launcher wallpaper/photos 同字节序）。
fn pack(px: Rgb) -> u32 {
    0xFF00_0000 | ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32
}

/// 节拍三角波（tick → -64..=64 居中——落日漂移/面盘浮动/波克升腾的
/// 确定性相位源，纯整数无浮点）。
fn tri_c(t: u64) -> i32 {
    let p = (t % 256) as i32;
    (if p < 128 { p } else { 256 - p }) - 64
}

// —— 模拟景色板（程序化确定性生成——深底亮面系；取景艺术面非
//    press/遮罩/圆角禁区值域）——
/// 后置（风景档）：天空顶/地平、落日、山影、地面、流云。
const SKY_TOP: Rgb = [0x14, 0x3E, 0x8C];
const SKY_HOR: Rgb = [0xA8, 0xC8, 0xF0];
const SUN: Rgb = [0xFF, 0xE2, 0x8A];
const MOUNT: Rgb = [0x21, 0x30, 0x46];
const GND_TOP: Rgb = [0x2E, 0x4A, 0x36];
const GND_BOT: Rgb = [0x1B, 0x2C, 0x22];
const CLOUD: Rgb = [0xF4, 0xF7, 0xFC];
/// 前置（自拍档）：室内渐变、面盘、波克光斑。
const FR_TOP: Rgb = [0x2C, 0x1B, 0x3E];
const FR_BOT: Rgb = [0x7C, 0x40, 0x5E];
const FACE: Rgb = [0xD8, 0xB4, 0x96];
const BOKEH: Rgb = [0xFF, 0xDF, 0xA8];
/// 补光提升目标色（Flash Auto 档 = 模拟景预升 ~11%——无闪光硬件的
/// 可判等效面，诚实边界见模块头）。
const LIFT: Rgb = [0xFF, 0xFF, 0xFF];
const LIFT_T: i32 = 28;

/// 生成单帧取景模拟景（确定性纯整数：`(front, flash_auto, tick)` 唯一
/// 决定全像素——同参恒同帧；photos `gen_photo` 同律）。
fn gen_scene(front: bool, flash_auto: bool, tick: u64, out: &mut [u32]) {
    let w = SCENE_W as i32;
    let h = SCENE_H as i32;
    let tc = tri_c(tick);
    if !front {
        // 后置风景：渐变天 + 落日漂移 + 双山剪影 + 三流云 + 地面渐变。
        let horizon = h * 9 / 16;
        let (sx, sy, sr) = (w / 2 + tc * w / 512, h / 4, 26);
        // 山剪影参数：(峰尖 x, 峰尖 y, 半底宽)。
        let mounts: [(i32, i32, i32); 2] =
            [(w * 2 / 5, horizon - 110, 150), (w * 7 / 10, horizon - 70, 170)];
        // 流云参数：(中心 y, 半长, 步速, 相位基)。
        let clouds: [(i32, i32, u64, i32); 3] = [(56, 45, 2, 40), (120, 60, 3, 200), (190, 35, 1, 300)];
        for y in 0..h {
            for x in 0..w {
                let mut px = if y < horizon {
                    lerp3(SKY_TOP, SKY_HOR, y * 256 / horizon)
                } else {
                    lerp3(GND_TOP, GND_BOT, (y - horizon) * 256 / (h - horizon))
                };
                // 落日（先绘——山影随后覆盖下缘成剪影）。
                let (dx, dy) = (x - sx, y - sy);
                if dx * dx + dy * dy <= sr * sr {
                    px = SUN;
                }
                // 双山剪影（整数半平面族：|dx|·(底-峰) ≤ (y-峰)·半底宽）。
                for &(ax, ay, half) in &mounts {
                    if y >= ay && y <= horizon && (x - ax).abs() * (horizon - ay) <= (y - ay) * half {
                        px = MOUNT;
                    }
                }
                // 三流云（横向整数椭圆胶囊；x 随 tick 线性漂移回绕）。
                for &(cy, hw, spd, base) in &clouds {
                    let cdy = y - cy;
                    if cdy.abs() > 10 {
                        continue; // 早出：椭圆竖半径 10
                    }
                    let span = (w + hw) as u64;
                    let cx = ((base as u64 + tick * spd) % span) as i32 - hw;
                    let cdx = x - cx;
                    if cdx.abs() > hw {
                        continue;
                    }
                    if cdx * cdx * 100 + cdy * cdy * hw * hw <= hw * hw * 100 {
                        px = CLOUD;
                    }
                }
                if flash_auto {
                    px = lerp3(px, LIFT, LIFT_T);
                }
                out[(y * w + x) as usize] = pack(px);
            }
        }
    } else {
        // 前置自拍：室内渐变 + 面盘浮动（顶带暗化成发影）+ 八枚波克升腾。
        let (fcx, fcy, fr) = (w / 2, h * 2 / 5 + tc / 8, 90);
        for y in 0..h {
            for x in 0..w {
                let mut px = lerp3(FR_TOP, FR_BOT, y * 256 / h);
                // 波克光斑（先绘——面盘随后覆盖中心域）。
                for i in 0..8i32 {
                    let br = 4 + (i % 3) * 3;
                    let bx = (i * 53 + (tick * 2) as i32) % w;
                    let by = (i * 97 + h * 3 - (tick * 4) as i32).rem_euclid(h);
                    let (dx, dy) = (x - bx, y - by);
                    if dx * dx + dy * dy <= br * br {
                        px = BOKEH;
                    }
                }
                // 面盘（顶四分之一暗化带 = 发影层次）。
                let (dx, dy) = (x - fcx, y - fcy);
                if dx * dx + dy * dy <= fr * fr {
                    px = if dy < -fr / 2 { lerp3(FACE, [0x40, 0x28, 0x20], 96) } else { FACE };
                }
                if flash_auto {
                    px = lerp3(px, LIFT, LIFT_T);
                }
                out[(y * w + x) as usize] = pack(px);
            }
        }
    }
}

/// 生成快门白环位图（168px：外环 r84..60 + 中点 r≤36 双白环、其余 =
/// 底带色——与底带无缝、呈经典快门白环观感；纯艺术面非按压特效。
/// 偶数边长位图圆心在半像素——2× 坐标轴系（2x−(sz−1)）保证四向精确
/// 对称，半径同步 2× 缩放）。
fn gen_shutter(band: Rgb, out: &mut [u32]) {
    let sz = SHUTTER_SZ as i32;
    let m = sz - 1;
    let (ro4, ri4, dot4) = (2 * 84i32, 2 * 60i32, 2 * 36i32);
    let white: Rgb = [0xFF, 0xFF, 0xFF];
    for y in 0..sz {
        for x in 0..sz {
            let (dx, dy) = (2 * x - m, 2 * y - m);
            let d4 = dx * dx + dy * dy;
            let px = if d4 <= dot4 * dot4 || (d4 <= ro4 * ro4 && d4 > ri4 * ri4) {
                white
            } else {
                band
            };
            out[(y * sz + x) as usize] = pack(px);
        }
    }
}

/// 取景/快门供件（berry-ui `IconSource` 真供件——launcher
/// `LauncherIcons`/photos `PhotoSource` 先例）：slab 启动期一次堆置；
/// 取景 slab 每拍重生成（读写不交叠——单线程主循环内严格先后序）。
struct CamSource {
    scene: *const u32,
    shutter: *const u32,
}

// SAFETY: 单线程 app（main 独占，无 worker/中断回灌面）；两 slab 均启动期
// 一次堆置终身有效，指针恒解引用有效内存。`&self` 读取期与
// `scene_regen` 写入期在帧循环内严格先后（regen → invalidate → render），
// 无并发别名。（Send/Send 两立 = OnceLock 静态置放的门面。）
unsafe impl Send for CamSource {}
unsafe impl Sync for CamSource {}

impl metrics::IconSource for CamSource {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        match id {
            SCENE_ID => Some((SCENE_W, SCENE_H)),
            SHUTTER_ID => Some((SHUTTER_SZ, SHUTTER_SZ)),
            _ => None,
        }
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        match id {
            // SAFETY: 见 impl Sync 注——slab 终身有效、单线程先后序。
            SCENE_ID => Some(unsafe { core::slice::from_raw_parts(self.scene, SCENE_PX) }),
            SHUTTER_ID => Some(unsafe { core::slice::from_raw_parts(self.shutter, SHUTTER_PX) }),
            _ => None,
        }
    }

    /// 取景/快门 = 大面观感图——请求 SmoothArgb 双线性缩放（壁纸/样张
    /// 同律；快门 1:1 直载不受扰）。
    fn smooth_scale(&self, _id: u16) -> bool {
        true
    }
}

/// 重生成取景 slab（写侧半程——与 `CamSource::pixels` 读侧半程在帧循环
/// 内严格先后，见 Sync 注）。
fn scene_regen(front: bool, flash_auto: bool, tick: u64, slab: *mut u32) {
    // SAFETY: slab = Box::leak 终身有效；SCENE_PX 恰全长；单线程独占写。
    let out = unsafe { core::slice::from_raw_parts_mut(slab, SCENE_PX) };
    gen_scene(front, flash_auto, tick, out);
}

// —— CAMERA.JSN 持久化（launcher layout_io 食谱：三约束 + 单簇近原子 +
//    严格串行单 RPC 会话 + close 即 commit）——

/// 卷根持久化文件（8.3 大写——fsmgr 单 FAT 卷根；CWD 默认根）。
const JSN_PATH: &[u8] = b"CAMERA.JSN\0";

/// JSN 读写帽（合法形上界 59B（count u32 + last u64 满档）；>64 长读即坏）。
const JSN_MAX: usize = 64;

/// 快门持久态（计数 + 最近时戳——CLOCK_MONOTONIC ns，boot 期归零语义
/// 如实见报告）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CamState {
    count: u32,
    last: u64,
}

/// 尾接辅助（普通 fn——photos 同形）。
fn put(out: &mut [u8], k: &mut usize, s: &[u8]) {
    out[*k..*k + s.len()].copy_from_slice(s);
    *k += s.len();
}

/// 十进制尾接（u64 域；≤20 位——无格式化分配面）。
fn push_dec(out: &mut [u8], k: &mut usize, mut v: u64) {
    let mut tmp = [0u8; 20];
    let mut n = 0usize;
    loop {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    while n > 0 {
        n -= 1;
        out[*k] = tmp[n];
        *k += 1;
    }
}

/// 严格十进制解析（非空、全数字、无前导零、u64 域内——违者 None）。
fn dec_strict(b: &[u8]) -> Option<u64> {
    if b.is_empty() || b.len() > 20 {
        return None;
    }
    if b.len() > 1 && b[0] == b'0' {
        return None;
    }
    let mut v: u64 = 0;
    for &d in b {
        if !d.is_ascii_digit() {
            return None;
        }
        v = v.checked_mul(10)?.checked_add((d - b'0') as u64)?;
    }
    Some(v)
}

/// canonical 编码：`{"schema":1,"count":<u32>,"last":<u64>}`（≤59B）。
fn jsn_encode(c: &CamState, out: &mut [u8; JSN_MAX]) -> usize {
    let mut k = 0usize;
    put(out, &mut k, b"{\"schema\":1,\"count\":");
    push_dec(out, &mut k, c.count as u64);
    put(out, &mut k, b",\"last\":");
    push_dec(out, &mut k, c.last);
    put(out, &mut k, b"}");
    k
}

/// 严格解析（头/中/尾形与两十进制域任一不符 = None——调用方自愈重写）。
fn jsn_parse(b: &[u8]) -> Option<CamState> {
    const HEAD: &[u8] = b"{\"schema\":1,\"count\":";
    const MID: &[u8] = b",\"last\":";
    if b.len() < HEAD.len() + MID.len() + 3 {
        return None; // "0}"+ 至少 3 位尾
    }
    if &b[..HEAD.len()] != HEAD {
        return None;
    }
    let rest = &b[HEAD.len()..];
    let mi = rest.windows(MID.len()).position(|w| w == MID)?;
    let tail = &rest[mi + MID.len()..];
    if tail.last() != Some(&b'}') || tail.len() < 2 {
        return None;
    }
    let count = u32::try_from(dec_strict(&rest[..mi])?).ok()?;
    let last = dec_strict(&tail[..tail.len() - 1])?;
    Some(CamState { count, last })
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
fn cam_save(c: &CamState) {
    if fs_h() == 0 {
        return; // 无 fs 词授权：结构性零 RPC（内存态照用）
    }
    let mut out = [0u8; JSN_MAX];
    let n = jsn_encode(c, &mut out);
    if let Err(e) = jsn_put(&out[..n]) {
        eprintln!("camera: jsn write errno={e}");
    }
}

/// 启动期读入（缺席/坏读自愈：回默认 + 重写；缺席常态零打印）。
fn cam_load() -> CamState {
    let d = CamState { count: 0, last: 0 };
    if fs_h() == 0 {
        return d;
    }
    match jsn_read() {
        JsnRead::Absent => {
            cam_save(&d);
            d
        }
        JsnRead::Bytes(b, n) => match jsn_parse(&b[..n]) {
            Some(c) => c,
            None => {
                eprintln!("camera: jsn bad -- default");
                cam_save(&d);
                d
            }
        },
        JsnRead::Bad(why) => {
            eprintln!("camera: jsn {why} -- default");
            cam_save(&d);
            d
        }
    }
}

// —— RPC 半程族（launcher/settings/photos 同形拷贝）——

/// 诊断 + park 收口（app 常驻不被 reap；崩溃监督归 svcmgr
/// app_supervise 域，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("camera: {what}");
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

// —— 建树小件 ——

/// 单节点 spec 构造（key=0——调和键路径挂账 v1.5）。
fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

/// Text 叶 spec（超 TEXT_INLINE=64B loud Err——本应用文案 ≤16B 域内）。
fn text_spec(text: &[u8], fg: Color, scale: FontSize) -> Result<ViewSpec, UiError> {
    Ok(vspec(
        ViewKind::Text,
        Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(text)?,
        }),
    ))
}

/// 定尺寸 Rect spec（网格线/控制带底/白闪特效共用——非卡片拼装）。
fn rect_spec(bg: Color, w: u32, h: u32) -> ViewSpec {
    vspec(ViewKind::Rect, Props::Rect(RectProps { bg, w: Some(w), h: Some(h) }))
}

/// 应用运行态（建树一次落定；帧循环只换字/重生成/增删白闪节点）。
struct App {
    t: Tokens,
    cam: CamState,
    /// 取景档（false = 后置风景 / true = 前置自拍）。
    front: bool,
    /// 闪光档（true = Auto 补光预升 / false = Off）。
    flash_auto: bool,
    /// 根叠放（子 = [取景, 网格线, 顶带, 底带, (白闪)?]——后子盖上）。
    layers: NodeId,
    /// 取景器 Image（微动 invalidate 锚）。
    vf: NodeId,
    /// Flash 钮 Fab 锚。
    flash_btn: NodeId,
    /// Flash 钮按压 chip（press_map visual）。
    flash_chip: NodeId,
    /// Flash 标题行 Text。
    flash_cap: NodeId,
    /// Front/Back 钮 Fab 锚。
    flip_btn: NodeId,
    flip_chip: NodeId,
    flip_cap: NodeId,
    /// 瞬态行 "Saved n" Text。
    saved_text: NodeId,
    /// 瞬态行撤除时刻（0 = 隐藏）。
    saved_until: u64,
    /// 快门环 Image（Click 首锚）。
    shutter_img: NodeId,
    /// 快门内容 Col（Click 次锚）。
    shutter_col: NodeId,
    /// 快门外扩按压暗框（press_map visual + Click 三锚）。
    shutter_mask: NodeId,
    /// 全屏白闪节点（在席 = 本帧白闪；呈现后即撤）。
    flash_fx: Option<NodeId>,
    /// 取景 slab 裸指针（regen 写侧）。
    scene: *mut u32,
    /// 微动节拍计数（场景确定性相位源）。
    tick: u64,
    /// 下拍时刻（帧循环 deadline 面之一）。
    next_motion: u64,
}

impl App {
    /// 节点 → 按压 chip 映射（DragEnd 误触复位路由）。
    fn chip_of(&self, node: NodeId) -> Option<NodeId> {
        if node == self.flash_btn || node == self.flash_cap {
            Some(self.flash_chip)
        } else if node == self.flip_btn || node == self.flip_cap {
            Some(self.flip_chip)
        } else if node == self.shutter_img || node == self.shutter_col || node == self.shutter_mask {
            Some(self.shutter_mask)
        } else {
            None
        }
    }

    /// 快门命中判定（Click 三锚：环 Image / 内容 Col / 外扩框）。
    fn is_shutter(&self, node: NodeId) -> bool {
        node == self.shutter_img || node == self.shutter_col || node == self.shutter_mask
    }
}

/// "Saved n" 瞬态行文案（≤16B）。
fn saved_bytes(count: u32, out: &mut [u8]) -> usize {
    let mut k = 0usize;
    put(out, &mut k, b"Saved ");
    push_dec(out, &mut k, count as u64);
    k
}

/// press_map 整表登记（trigger = Fab/标题/快门三族锚 → visual = 按压
/// chip Rect；7 条 < PRESS_MAP_MAX=28；visual 全 Rect 过白名单——
/// PRESSED_BG 引擎单源，应用零 press 字面量）。
fn press_rebind(ui: &mut UiMain, app: &App) -> Result<(), UiError> {
    let e = [
        (app.flash_btn, app.flash_chip),
        (app.flash_cap, app.flash_chip),
        (app.flip_btn, app.flip_chip),
        (app.flip_cap, app.flip_chip),
        (app.shutter_img, app.shutter_mask),
        (app.shutter_col, app.shutter_mask),
        (app.shutter_mask, app.shutter_mask),
    ];
    ui.set_press_map(&e)
}

/// 顶行控制钮装配（Stack[chip Rect（常态 = 带底同色无缝，Press 经引擎
/// press_react 换 PRESSED_BG）, 内容 Col[Fab, 标题定宽列]]）。返回
/// (fab 锚, chip 锚, 标题 Text 锚)。
fn hud_button(
    ui: &mut UiMain,
    parent: NodeId,
    t: &Tokens,
    icon: u32,
    on_tap: u32,
    cap: &[u8],
    cap_w: u32,
) -> Result<(NodeId, NodeId, NodeId), UiError> {
    let st = pattern::stack(ui, parent)?;
    let chip = ui.add(st, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_comp, w: None, h: None })))?;
    let col = ui.add(
        st,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: 24, // 8vp：圆钮与标题行间距
            w: None,
            h: None,
            cross: CrossAlign::Center,
        })),
    )?;
    let btn = pattern::icon_button(ui, col, icon, on_tap)?;
    // 标题定宽列（文案换字不推布局——换档零抖动）。
    let cc = ui.add(
        col,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: 0,
            w: Some(cap_w),
            h: None,
            cross: CrossAlign::Center,
        })),
    )?;
    let cap_node = ui.add(cc, text_spec(cap, t.font_secondary, FontSize::S16x32)?)?;
    Ok((btn.node, chip, cap_node))
}

/// 三分网格线（取景辅助线：2 竖 + 2 横，比例布局定位——非卡片拼装；
/// 色取 theme 次级文本 token）。
fn grid_overlay(ui: &mut UiMain, parent: NodeId, t: &Tokens, w: u32, h: u32) -> Result<(), UiError> {
    let (w3, h3) = (w / 3, h / 3);
    let color = t.font_secondary; // 中灰——亮天空/暗山影双底可判
    // 竖线 ×2：Row[spacer(w3), line, spacer(w3−t), line, spacer(w3−t)]。
    let vr = ui.add(
        parent,
        vspec(ViewKind::Row, Props::Row(RowProps {
            gap: 0,
            w: Some(w),
            h: Some(h),
            cross: CrossAlign::Start,
        })),
    )?;
    pattern::spacer(ui, vr, w3, h)?;
    ui.add(vr, rect_spec(color, GRID_T, h))?;
    pattern::spacer(ui, vr, w3.saturating_sub(GRID_T), h)?;
    ui.add(vr, rect_spec(color, GRID_T, h))?;
    pattern::spacer(ui, vr, w3.saturating_sub(GRID_T), h)?;
    // 横线 ×2：Col[spacer(h3), line, spacer(h3−t), line, spacer(h3−t)]。
    let hr = ui.add(
        parent,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: 0,
            w: Some(w),
            h: Some(h),
            cross: CrossAlign::Start,
        })),
    )?;
    pattern::spacer(ui, hr, w, h3)?;
    ui.add(hr, rect_spec(color, w, GRID_T))?;
    pattern::spacer(ui, hr, w, h3.saturating_sub(GRID_T))?;
    ui.add(hr, rect_spec(color, w, GRID_T))?;
    pattern::spacer(ui, hr, w, h3.saturating_sub(GRID_T))?;
    Ok(())
}

/// 建页树：根叠放 → 取景 Image → 网格线 → 顶带（Flash/Flip 钮）→
/// 底带（Saved 行 + 大圆快门）。Err = 容量/文本域违例 → fail 收口。
fn build(ui: &mut UiMain, app: &mut App) -> Result<(), UiError> {
    let t = app.t;
    let (vp_w, vp_h) = ui.viewport();

    // —— 取景器（全幅 Image——v1 全幅窗近似：surface 全幅取景常驻）——
    let vf = ui.add(
        app.layers,
        vspec(ViewKind::Image, Props::Image(ImageProps {
            res: SCENE_ID,
            w: Some(vp_w),
            h: Some(vp_h),
        })),
    )?;
    let _ = ui.set_a11y(vf, ROLE_IMAGE, b"viewfinder");
    app.vf = vf;

    grid_overlay(ui, app.layers, &t, vp_w, vp_h)?;

    // —— 顶行控制带（Stack[带底 Rect, 内容 Row]——取景器上缘 HUD）——
    let band_t = pattern::stack(ui, app.layers)?;
    ui.add(band_t, rect_spec(t.bg_comp, vp_w, TOPBAR_H))?;
    let row_t = ui.add(
        band_t,
        vspec(ViewKind::Row, Props::Row(RowProps {
            gap: 0,
            w: Some(vp_w),
            h: Some(TOPBAR_H),
            cross: CrossAlign::Center,
        })),
    )?;
    pattern::spacer(ui, row_t, SIDE_PAD, 1)?;
    let (flash_btn, flash_chip, flash_cap) =
        hud_button(ui, row_t, &t, ICON_FLASH, EV_FLASH, b"Flash Auto", FLASH_CAP_W)?;
    let _ = ui.set_a11y(flash_cap, ROLE_TEXT, b"flash mode");
    // 中缝推位 = vp_w − 双留白 − 双钮栈宽（钮栈宽 = 标题定宽列宽——
    // 恒 ≥ Fab 56px 内在宽；行内总宽恒 vp_w——右钮贴右缘钉位）。
    let flex_t = vp_w
        .saturating_sub(2 * SIDE_PAD)
        .saturating_sub(FLASH_CAP_W)
        .saturating_sub(FLIP_CAP_W);
    pattern::spacer(ui, row_t, flex_t, 1)?;
    let (flip_btn, flip_chip, flip_cap) =
        hud_button(ui, row_t, &t, ICON_FLIP, EV_FLIP, b"Back", FLIP_CAP_W)?;
    let _ = ui.set_a11y(flip_cap, ROLE_TEXT, b"camera face");
    pattern::spacer(ui, row_t, SIDE_PAD, 1)?;
    app.flash_btn = flash_btn;
    app.flash_chip = flash_chip;
    app.flash_cap = flash_cap;
    app.flip_btn = flip_btn;
    app.flip_chip = flip_chip;
    app.flip_cap = flip_cap;

    // —— 底行控制带（Stack[带底 Rect, 内容 Row]：Saved 行 + 大圆快门）——
    let band_b = pattern::stack(ui, app.layers)?;
    ui.add(band_b, rect_spec(t.bg_comp, vp_w, BOTBAR_H))?;
    let row_b = ui.add(
        band_b,
        vspec(ViewKind::Row, Props::Row(RowProps {
            gap: 0,
            w: Some(vp_w),
            h: Some(BOTBAR_H),
            cross: CrossAlign::Center,
        })),
    )?;
    let flex_b = vp_w
        .saturating_sub(SAVED_W)
        .saturating_sub(SAVED_GAP)
        .saturating_sub(SHUTTER_MASK_SZ) / 2;
    pattern::spacer(ui, row_b, flex_b, 1)?;
    // 瞬态行（定宽列——数字进位不推快门位）。
    let sc = ui.add(
        row_b,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: 0,
            w: Some(SAVED_W),
            h: None,
            cross: CrossAlign::Start,
        })),
    )?;
    let saved_text = ui.add(sc, text_spec(b"", t.font_primary, FontSize::S24x48)?)?;
    let _ = ui.set_a11y(saved_text, ROLE_TEXT, b"saved count");
    pattern::spacer(ui, row_b, SAVED_GAP, 1)?;
    // 大圆快门：Stack[外扩按压暗框 200², 内容 Col 200²(cross=Center) >
    // 白环 Image 168²]（Image 满盖中心 → 按压可见域 = 外扩 16px 暗框带）。
    let shst = pattern::stack(ui, row_b)?;
    let shutter_mask = ui.add(
        shst,
        rect_spec(t.bg_comp, SHUTTER_MASK_SZ, SHUTTER_MASK_SZ),
    )?;
    let shutter_col = ui.add(
        shst,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: 0,
            w: Some(SHUTTER_MASK_SZ),
            h: Some(SHUTTER_MASK_SZ),
            cross: CrossAlign::Center,
        })),
    )?;
    let shutter_img = ui.add(
        shutter_col,
        vspec(ViewKind::Image, Props::Image(ImageProps {
            res: SHUTTER_ID,
            w: Some(SHUTTER_SZ),
            h: Some(SHUTTER_SZ),
        })),
    )?;
    let _ = ui.set_a11y(shutter_img, ROLE_BUTTON, b"shutter");
    app.saved_text = saved_text;
    app.shutter_img = shutter_img;
    app.shutter_col = shutter_col;
    app.shutter_mask = shutter_mask;
    pattern::spacer(ui, row_b, flex_b, 1)?;

    press_rebind(ui, app)?;
    Ok(())
}

fn main() {
    // —— 降级门：wm 客户端端未交付 → 一行诊断 + park 常驻（settings/
    //    photos 先例同形；窗口义务语义不折衷成 exit(0) 秒退）。本 app
    //    声明 fs 词 + [ui] 节 → svcmgr gift 槽位 [rootfs, fs, wm,
    //    input]——wm 落 extras[2]（berry-libc io.rs GIFT_H[4]；
    //    `wm_cli_h()` 的 [0] 位对 wants_fs 程序承载 rootfs VMO，不可
    //    用），取用经同槽访问器 `wm_present_h`（多产面惯例——extras[2]
    //    对 gui-wmd 承载 present 客户端端、对本 app 承载 wm 客户端端；
    //    专用访问器挂 appshell 裁决 A 落地换轨）。
    let hw = wm_present_h();
    if hw == 0 {
        eprintln!("camera: wm absent (no gui gift)");
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
        "camera: wm v={} {}x{} input={}",
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

    // —— 供件（slab 启动期一次堆置：取景 360×640 ≈ 0.9 MiB 可变 +
    //    快门环 168² 静态；快门环底带色 = theme token 同值预热）。
    let t = Tokens::pick(Theme::Dark); // 相机 HUD 暗档（theme.rs 双档单源）
    let scene_slab: &'static mut [u32] = Box::leak(vec![0u32; SCENE_PX].into_boxed_slice());
    let scene_ptr = scene_slab.as_mut_ptr();
    let shutter_slab: &'static mut [u32] = Box::leak(vec![0u32; SHUTTER_PX].into_boxed_slice());
    {
        let band: Rgb = [t.bg_comp.r, t.bg_comp.g, t.bg_comp.b];
        gen_shutter(band, shutter_slab);
    }
    static CAM_SRC: std::sync::OnceLock<CamSource> = std::sync::OnceLock::new();
    let src: &'static CamSource = CAM_SRC
        .get_or_init(|| CamSource { scene: scene_ptr as *const u32, shutter: shutter_slab.as_ptr() });
    ui.set_icon_source(src);

    // —— 持久态读入 + 建树（占位锚先挂根下隐形 spacer，随即覆写——
    //    NodeId 无外部可造哨兵的折衷，photos 同形）。
    let root = ui.root();
    let layers = pattern::stack(ui, root).expect("root stack");
    let ph = pattern::spacer(ui, layers, 1, 1).expect("anchor placeholder");
    let mut app = App {
        t,
        cam: cam_load(),
        front: false,
        flash_auto: true,
        layers,
        vf: ph,
        flash_btn: ph,
        flash_chip: ph,
        flash_cap: ph,
        flip_btn: ph,
        flip_chip: ph,
        flip_cap: ph,
        saved_text: ph,
        saved_until: 0,
        shutter_img: ph,
        shutter_col: ph,
        shutter_mask: ph,
        flash_fx: None,
        scene: scene_ptr,
        tick: 0,
        next_motion: 0,
    };
    if let Err(e) = build(ui, &mut app) {
        fail(&format!("page build err={e:?}"));
    }
    let _ = ui.remove(ph); // 建树占位锚退役
    scene_regen(app.front, app.flash_auto, app.tick, app.scene);

    run(hw, input_h(), ui, app);
}

/// 帧循环 + 输入 + 事件消费（settings/photos `run` 镜像；增量 = 取景
/// 微动节拍 + 瞬态行自撤 + 快门三连（按压暗框/白闪单帧/Saved n）+
/// Flash/Flip 即时换景）。
fn run(hw: Handle, hi_in: Handle, ui: &'static mut UiMain, mut app: App) -> ! {
    // —— present GETINFO 轻断言 + 几何一致性（present 权威——失配 loud）。
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
    //（launcher/settings/photos 同形）；run 永不返回——fb 终身持有。
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };

    // —— glyph scratch 堆置（DisplayList 'a 钉调用方 scratch）。
    let scratch: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());

    // —— vsync 协商 + 起振（首帧整帧 damage）。
    ui.set_vsync(VSYNC_NS, 0);
    ui.wake(now());
    app.next_motion = now() + MOTION_STEP_NS;

    let mut last_expected: u64 = 0; // pump 重采样 target（上帧 expected 轨）
    let mut last_vote: Option<frame::VoteBallot> = None; // 票面变化才发
    let mut presented: u64 = 0; // FRAME_CALLBACK 过链计数（首帧诊断门）
    let mut buf_id: u32 = 1; // SUBMIT buffer 代际
    let mut hi = hi_in; // input 端（失效面置 0——静态页续活）
    let mut input_pending = false; // INPUT_POLL 在途
    let mut input_buf = [0u8; gp::INPUT_REP_MAX];
    let mut input_hs = [0 as Handle; 8];

    loop {
        let t_now = now();

        // —— 真实输入（settings 同形）：先收上一轮 INPUT_POLL 应答，再挂
        //    下一轮；事件逐条 push 进引擎（press_react/widget_react 在
        //    pump_input 期同拍消费——按压暗框换底 / Fab on_tap 落事件）。
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
                                    eprintln!("camera: input push err={e:?}");
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {} // 无事件常态
                    Some(Err(st)) => eprintln!("camera: input poll st={st}"),
                    None => eprintln!("camera: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("camera: input recv err={s}");
                hi = 0;
                input_pending = false;
            }
        }
        if hi != 0 && !input_pending {
            if input_arm(hi) {
                input_pending = true;
            } else {
                // 输入面失效（arm 失败 = 通道死/超时）——置 0 永久降级为
                // 静态取景（页面常驻义务不受输入面拖累）。
                hi = 0;
            }
        }

        // —— 取景微动节拍（~10fps：slab 重生成 + invalidate——活取景器
        //    常驻 redraw；帧循环 deadline 面自管，见底部 wait）。
        if t_now >= app.next_motion {
            app.tick = app.tick.wrapping_add(1);
            scene_regen(app.front, app.flash_auto, app.tick, app.scene);
            if let Err(e) = ui.invalidate(app.vf) {
                eprintln!("camera: vf invalidate err={e:?}");
            }
            app.next_motion = t_now + MOTION_STEP_NS;
            ui.wake(t_now);
        }

        // —— 瞬态行自撤（Saved n 2s 到点即清——帧循环节拍驱动）。
        if app.saved_until != 0 && t_now >= app.saved_until {
            if let Err(e) = ui.set_text(app.saved_text, b"") {
                eprintln!("camera: saved clear err={e:?}");
            }
            app.saved_until = 0;
            ui.wake(t_now);
        }

        // —— 输入泵（referee + 重采样 + widget_react——引擎单口）；
        //    MsgBuf 本应用两消费者：①快门 Click（拍照三连）②拖拽误触
        //    按压暗框复位（引擎 press_react 无 DragEnd 还原臂——photos
        //    同款 app 侧复位）。
        let msgs = ui.pump_input(last_expected);

        for m in msgs.iter() {
            match m {
                Msg::Click { node } => {
                    if app.is_shutter(*node) {
                        do_capture(ui, &mut app);
                    }
                }
                // 引擎 press_react 只在 Release/Cancel 还原换底——拖拽
                // 误触（slop 外抬手）走 DragEnd 无还原臂，暗框会卡死；
                // app 侧复位常态底色（theme token 取色非按压字面量）+
                // press_map 整表重登记（active/saved 归零）。
                Msg::DragEnd { node, .. } => {
                    if let Some(chip) = app.chip_of(*node) {
                        let bg = app.t.bg_comp;
                        let _ = ui.set_props(chip, Props::Rect(RectProps { bg, w: None, h: None }));
                        if let Err(e) = press_rebind(ui, &app) {
                            eprintln!("camera: press rebind err={e:?}");
                        }
                        ui.wake(now());
                    }
                }
                _ => {}
            }
        }

        // —— 组件事件消费（widget_react 产物）：Flash/Flip 即时换景。
        let evs = ui.take_events();
        for ev in evs.iter() {
            match ev.event {
                EV_FLASH => {
                    app.flash_auto = !app.flash_auto;
                    let cap: &[u8] = if app.flash_auto { b"Flash Auto" } else { b"Flash Off" };
                    if let Err(e) = ui.set_text(app.flash_cap, cap) {
                        eprintln!("camera: flash cap err={e:?}");
                    }
                    scene_regen(app.front, app.flash_auto, app.tick, app.scene);
                    let _ = ui.invalidate(app.vf);
                    ui.wake(now());
                }
                EV_FLIP => {
                    app.front = !app.front;
                    let cap: &[u8] = if app.front { b"Front" } else { b"Back" };
                    if let Err(e) = ui.set_text(app.flip_cap, cap) {
                        eprintln!("camera: flip cap err={e:?}");
                    }
                    scene_regen(app.front, app.flash_auto, app.tick, app.scene);
                    let _ = ui.invalidate(app.vf);
                    ui.wake(now());
                }
                _ => {}
            }
        }

        // —— 渲染（damage 空集零提交——跳过帧零 RPC）。
        let mut enc = CommandEncoder::new();
        let out = ui.render_frame(t_now, scratch, &mut enc);
        last_expected = out.expected_present_ns;
        if out.drew {
            let dmg = out.damage;
            let full = dmg.full || dmg.count == 0;
            let pass = if full {
                PassDescriptor::clear(app.t.bg_page.r, app.t.bg_page.g, app.t.bg_page.b)
            } else {
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
                (gp::SUBMIT_F_DAMAGE_FULL, [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX], 0usize)
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
            ui.note_presented((ui.telemetry().frames() - 1) as u32, now(), done.status, done.credits);

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
                println!("camera: first frame"); // 诊断面（console=false 走私有对）
            }

            // —— 白闪单帧特效撤收：白帧已呈现（SUBMIT→FRAME_CALLBACK 过
            //    链恰一帧）→ 撤节点 + wake——下一拍恢复取景画面。设计稿
            //    §2.5 明载（非拼卡特效、不走 press 三禁区）。
            if let Some(fx) = app.flash_fx.take() {
                if let Err(e) = ui.remove(fx) {
                    eprintln!("camera: flash fx remove err={e:?}");
                }
                ui.wake(now());
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

        // —— 稳态等待（[wm READABLE, input READABLE] + deadline =
        //    min(next_wakeup, 微动下拍, 瞬态行到点, now+IDLE 帽)——app
        //    侧定时面（引擎无通用 timer）；TIMEDOUT = 期限到正常醒）。
        let cap = now() + IDLE_CAP_NS;
        let mut deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
        deadline = deadline.min(app.next_motion);
        if app.saved_until != 0 {
            deadline = deadline.min(app.saved_until);
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

/// 快门拍照三连：计数 + 持久化 + 瞬态行 "Saved n"（2s 自撤）+ 全屏白闪
/// 单帧节点入树（呈现后 run 帧循环撤收）。
fn do_capture(ui: &mut UiMain, app: &mut App) {
    app.cam.count = app.cam.count.saturating_add(1);
    app.cam.last = now();
    cam_save(&app.cam);
    let mut b = [0u8; 16];
    let n = saved_bytes(app.cam.count, &mut b);
    if let Err(e) = ui.set_text(app.saved_text, &b[..n]) {
        eprintln!("camera: saved set_text err={e:?}");
    }
    app.saved_until = now() + SAVED_NS;
    if app.flash_fx.is_none() {
        let (vp_w, vp_h) = ui.viewport();
        match ui.add(app.layers, rect_spec(Color { r: 0xFF, g: 0xFF, b: 0xFF }, vp_w, vp_h)) {
            Ok(fx) => app.flash_fx = Some(fx),
            Err(e) => eprintln!("camera: flash fx add err={e:?}"),
        }
    }
    ui.wake(now());
}

/// K 矩形并集包络（增量 pass 的 dirty 域；settings/photos 同形）。
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
            eprintln!("camera: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("camera: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}
