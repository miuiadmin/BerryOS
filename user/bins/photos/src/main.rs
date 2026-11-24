//! photos —— 波 4 第三件真 GUI 应用（PHOTOS appstub 秒退壳替换；
//! `.agents/plans/2026-11-15-波4预制应用翻新实施设计.md` §2.4，侦察/
//! 对账底册 = worktree 根 `W4-RECON.md`）。
//!
//! **页面树**（pattern 模式件消费——波 4 首批真实消费者之一）：
//!
//! - 网格页：`pattern::top_bar`(Photos) + `pattern::seg`(All/Favorites)
//!   + 三列缩略网格（格 = Card kind 直载，`pattern::LISTITEM_RADIUS`
//!   8vp@3x 圆角单源；缩略图 = Image kind 经 `IconSource` 程序化供件）
//!   + 无相册源诚实标注行。
//! - 看片页（点缩略懒建、Back 退出即整树 remove、再点重建——「首点懒
//!   建、退出 DESTROY、复用」的 v1 单窗近似）：`SwipePager` 翻片（>8 张
//!   时 8 页滑窗惰性换绑——`SWIPE_PAGES_MAX`=8 帽内）+ 顶行
//!   `pattern::icon_button`(Back/Star/Delete) + 页位指示 "n / N"。
//!
//! **铁律执法**：卡片 = Card kind（圆角常量单源 pattern 面，非手写 Rect
//! 拼卡）；分段 = `pattern::seg`；按压 = 引擎 `set_press_map`（`PRESSED_BG`
//! 单源换底——v1 无 alpha 通道，整面遮罩不可达，呈现为缩略图四周暗框；
//! 0.95 缩放档未落，见模块尾「v1 降解」）；文案全 ASCII；帧循环零分配
//! （样张像素板为启动期一次堆置）。
//!
//! **链形态** = settings crate 镜像（W4-RECON §1 先例）：wm 门控
//! （本 app 带 fs 词 → wm 礼物落 extras[2]，经同槽访问器
//! `berry_libc::io::wm_present_h` 取用——settings（无 fs 词）落 [0] 走
//! `wm_cli_h`，两形分野单源 = svcmgr GUI 授权链 5452649af；0 = 无 GUI
//! 礼物 → 一行诊断 + park 常驻）→ WM_GETINFO → present 三部曲
//! → 帧 VMO 三连 → INPUT_POLL 批拉 + `pump_input`（referee +
//! `widget_react` 同拍）→ `render_frame` damage 驱动零空转。RPC 半程族
//! （`rpc_send`/`rpc_recv`/`rpc`）与 `damage_envelope`/`input_arm` 均
//! launcher/settings 同形拷贝。
//!
//! **持久化**：PHOTOS.JSN（FAT 数据盘根，`launcher layout_io` 食谱自实现
//! ——appshell::conf 未落，W4-RECON §四降解）：`{"schema":1,"del":"..",
//! "fav":".."}` 位串形 ≤64B 单簇近原子；三约束纪律继承（缺席回默认 +
//! 坏读自愈回写 + 写失败非致命 eprintln 常驻不死）；`fs_h()==0` 结构性
//! 早退零 RPC（无 fs 词授权面）。诚实边界：无相机卷真源——12 张样张为
//! 应用内确定性程序化图案（渐变 + 几何，零资产增量），Camera 产物联动
//! 挂 v2。
//!
//! **v1 降解**（3b/波 2 未落面的就地替代，全清单见 W4-IMPL-PHOTOS.md）：
//! 看片窗无 `WM_CREATE_F_IMMERSIVE` → 单窗全幅近似（surface 全幅 + 看片
//! 页自绘整幅底，无状态栏避让/沉浸语义）；无 wm back 路由 → Back = 应用
//! 内 icon_button（无系统返回链、无根页优雅退出）；theme 样式族
//! （card_style/oh_press_bg/OH_PRESS_SCALE_MIDDLE_PERMIL=950）未落 →
//! Card kind 自绘面 + 引擎 press_map 等价面（`PRESSED_BG` 换底）；无退出
//! 路径（settings 先例同形——fail = eprintln + park，崩溃监督归 svcmgr）。

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
    berry_handle_duplicate, berry_object_wait_many, berry_object_wait_one, berry_thread_sleep,
    berry_vmar_map, berry_vmo_commit, berry_vmo_create, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY,
    BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, RIGHT_READ, RIGHT_TRANSFER, RIGHT_WRITE,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_ui::desc::{
    CardProps, ColProps, GridProps, ImageProps, Msg, NodeId, Props, RectProps, RowProps,
    StackAlign, StackProps, TextBuf, TextProps, TextStyle, ViewKind, ViewSpec,
};
use berry_ui::layout::CrossAlign;
use berry_ui::pattern::{self, icons, Seg, LISTITEM_RADIUS, TOPBAR_H};
use berry_ui::swipe::SwipePager;
use berry_ui::theme::{Theme, Tokens};
use berry_ui::{damage, frame, hit, metrics, UiError, UiMain, ROLE_IMAGE, ROLE_TEXT};

// 链接种子（fsmgr/guifbd/berry-launcher/settings 同款）：rlib/LTO 按需保留
// ——无引用则 berry-libc 的 crt0（含 `_start`）不进链接。
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

/// 帧缓冲映射 VA（launcher frameloop 同位：mmap VA 窗
/// [0x5000_0000,0x6000_0000) 内固定位 +16MiB；页对齐）。
const FRAME_VA: usize = 0x5100_0000;

/// 默认 vsync 周期（60Hz；present GETINFO 无周期域——launcher 同值）。
const VSYNC_NS: u64 = 16_666_666;

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// glyph scratch 容量（堆置——paint 对超容 run 截断尾字形；本应用文案
/// 面（标题/分段/状态行/页位）≤40 字形富余）。
const SCRATCH_GLYPHS: usize = 256;

// —— 应用几何（px @3x；pattern 常量族之外的拟定位，单源本块）——
/// 网格/页面水平留白（16vp）。
const SIDE_PAD: u32 = 48;
/// 网格行列缝（8vp——设计稿「行距 8vp」直录）。
const GRID_GAP: u32 = 24;
/// 缩略图按压暗框边带（4vp——press_map 遮罩近似的可见域）。
const PRESS_INSET: u32 = 12;
/// 网格列数（设计稿「三列」）。
const GRID_COLS: u32 = 3;
/// 页面纵向缝（8vp）。
const PAGE_GAP: u32 = 24;
/// 看片页顶行水平留白（大视口 16vp / 小视口 8vp）。
fn bar_pad(vp_w: u32) -> u32 {
    if vp_w >= 1000 { SIDE_PAD } else { SIDE_PAD / 2 }
}

// —— 事件 ID 面（props on_tap 携载——take_events 消费锚）——
const EV_VBACK: u32 = 1;
const EV_VSTAR: u32 = 2;
const EV_VDEL: u32 = 3;

/// 看片页 Star/Delete 的图标字形 id（引擎 v1 Fab paint 不消费 icon id
/// ——恒绘 '+' 圆钮；id 值此处仅为 a11y/语义锚，域别于 pattern::icons
/// 约定域与样张域）。
const ICON_STAR: u32 = 0x0140;
const ICON_TRASH: u32 = 0x0141;

// —— 样张面（程序化确定性生成，零资产增量）——
/// 样张数（设计稿「确定性生成 12 张」）。
const PHOTO_N: usize = 12;
/// 样张内在分辨率（正方 240×240 ARGB；缩略/全幅经 SmoothArgb 双线性
/// 缩放——供件 `smooth_scale` 置位）。
const PHOTO_SZ: u32 = 240;
/// 单张像素数。
const PHOTO_PX: usize = PHOTO_SZ as usize * PHOTO_SZ as usize;
/// 样张资源 id 基（ImageProps res 域；避开 pattern::icons 0x01xx 约定域）。
const PHOTO_ID_BASE: u16 = 0x2100;

/// 样张 id → 序号（域外 None）。
fn photo_idx(id: u16) -> Option<usize> {
    (id >= PHOTO_ID_BASE && id < PHOTO_ID_BASE + PHOTO_N as u16)
        .then(|| (id - PHOTO_ID_BASE) as usize)
}

/// 样张供件（berry-ui `IconSource` 真供件——launcher `LauncherIcons`
/// 先例）：像素板启动期一次生成、终身只读。
struct PhotoSource {
    slab: &'static [u32],
}

impl metrics::IconSource for PhotoSource {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        photo_idx(id).map(|_| (PHOTO_SZ, PHOTO_SZ))
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        let i = photo_idx(id)?;
        Some(&self.slab[i * PHOTO_PX..(i + 1) * PHOTO_PX])
    }

    /// 样张 = 大面全不透明图——请求 SmoothArgb 双线性缩放（壁纸同律；
    /// 图标类最近邻硬边默认档不适用于照片观感）。
    fn smooth_scale(&self, id: u16) -> bool {
        photo_idx(id).is_some()
    }
}

/// 全部样张像素板（一次性堆置：12 × 240×240 × 4B ≈ 2.7 MiB 常驻）。
fn gen_slab() -> &'static [u32] {
    let mut v = vec![0u32; PHOTO_N * PHOTO_PX];
    for i in 0..PHOTO_N {
        gen_photo(i, &mut v[i * PHOTO_PX..(i + 1) * PHOTO_PX]);
    }
    Box::leak(v.into_boxed_slice())
}

/// RGB 三元组。
type Rgb = [u8; 3];

/// 双色整数插值（`t` 0..=256——Q8 定点，无浮点）。
fn lerp3(a: Rgb, b: Rgb, t: i32) -> Rgb {
    [
        (a[0] as i32 + (b[0] as i32 - a[0] as i32) * t / 256) as u8,
        (a[1] as i32 + (b[1] as i32 - a[1] as i32) * t / 256) as u8,
        (a[2] as i32 + (b[2] as i32 - a[2] as i32) * t / 256) as u8,
    ]
}

/// ARGB 打包（0xAARRGGBB——launcher wallpaper 同字节序）。
fn pack(px: Rgb) -> u32 {
    0xFF00_0000 | ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32
}

/// 三套色板（i%3）：（顶色，底色，几何强调色）——深底亮面系。
fn palette(i: usize) -> (Rgb, Rgb, Rgb) {
    match i {
        0 => ([0x0A, 0x2A, 0x5E], [0x37, 0x7D, 0xF0], [0xFF, 0xD5, 0x4F]), // 海空/金
        1 => ([0x3A, 0x0D, 0x2E], [0xE8, 0x5A, 0x4F], [0xFF, 0xE0, 0x8A]), // 熔岩/暖沙
        _ => ([0x07, 0x2B, 0x24], [0x2F, 0xB5, 0x6B], [0xC8, 0xF4, 0xE3]), // 林野/雾白
    }
}

/// 单张确定性生成（渐变 + 几何；纯整数算术，i 唯一决定全像素——
/// 3 色板 × 4 母题 = 12 组合恰各不相同）。
fn gen_photo(i: usize, out: &mut [u32]) {
    let (top, bot, acc) = palette(i % 3);
    let motif = i % 4;
    let sz = PHOTO_SZ as i32;
    let c = (sz - 1) / 2;
    for y in 0..sz {
        for x in 0..sz {
            // 对角渐变底（t = (x+y)/(2·SZ-2) 归一到 0..=256）。
            let t = (x + y) * 256 / (2 * sz - 2);
            let mut px = lerp3(top, bot, t);
            match motif {
                // 同心环：距心平方除环宽²的奇偶带。
                0 => {
                    let (dx, dy) = (x - c, y - c);
                    if (dx * dx + dy * dy) / 900 % 2 == 1 {
                        px = acc;
                    }
                }
                // 斜纹条带：x+y 每 30px 翻转。
                1 => {
                    if (x + y) / 30 % 2 == 1 {
                        px = acc;
                    }
                }
                // 棋盘格：60px 方格奇偶。
                2 => {
                    if (x / 60 + y / 60) % 2 == 1 {
                        px = acc;
                    }
                }
                // 落日：强调色圆盘 + 下四分之一暗化地平带。
                _ => {
                    let (dx, dy) = (x - c, y - c);
                    if dx * dx + dy * dy <= 80 * 80 {
                        px = acc;
                    } else if y > sz * 3 / 4 {
                        px = lerp3(px, [0, 0, 0], 96);
                    }
                }
            }
            out[(y * sz + x) as usize] = pack(px);
        }
    }
}

// —— PHOTOS.JSN 持久化（launcher layout_io 食谱：三约束 + 单簇近原子 +
//    严格串行单 RPC 会话 + close 即 commit）——

/// 卷根持久化文件（8.3 大写——fsmgr 单 FAT 卷根；CWD 默认根）。
const JSN_PATH: &[u8] = b"PHOTOS.JSN\0";

/// JSN 读写帽（合法形恰 56B；>64 长读即坏）。
const JSN_MAX: usize = 64;

/// 相册持久态（位掩码：bit i = 样张 i 的删除/收藏位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Album {
    del: u16,
    fav: u16,
}

impl Album {
    fn is_del(&self, i: usize) -> bool {
        self.del & (1 << i) != 0
    }
    fn is_fav(&self, i: usize) -> bool {
        self.fav & (1 << i) != 0
    }
}

/// 过滤后样张序（del 剔除；Favorites 档只留收藏）——(数组, 有效长)。
fn filtered_list(album: &Album, fav_only: bool) -> ([usize; PHOTO_N], usize) {
    let mut out = [0usize; PHOTO_N];
    let mut n = 0usize;
    for i in 0..PHOTO_N {
        if album.is_del(i) || (fav_only && !album.is_fav(i)) {
            continue;
        }
        out[n] = i;
        n += 1;
    }
    (out, n)
}

/// 尾接辅助（闭包捕获双 &mut 的借用冲突规避——普通 fn）。
fn put(out: &mut [u8], k: &mut usize, s: &[u8]) {
    out[*k..*k + s.len()].copy_from_slice(s);
    *k += s.len();
}

/// canonical 编码：`{"schema":1,"del":"..12..","fav":"..12.."}`（恰 56B）。
fn jsn_encode(a: &Album, out: &mut [u8; JSN_MAX]) -> usize {
    let mut k = 0usize;
    put(out, &mut k, b"{\"schema\":1,\"del\":\"");
    for i in 0..PHOTO_N {
        out[k] = if a.is_del(i) { b'1' } else { b'0' };
        k += 1;
    }
    put(out, &mut k, b"\",\"fav\":\"");
    for i in 0..PHOTO_N {
        out[k] = if a.is_fav(i) { b'1' } else { b'0' };
        k += 1;
    }
    put(out, &mut k, b"\"}");
    k
}

/// 严格解析（前缀/位串/尾形任一不符 = None——调用方自愈重写）。
fn jsn_parse(b: &[u8]) -> Option<Album> {
    const HEAD: &[u8] = b"{\"schema\":1,\"del\":\"";
    const MID: &[u8] = b"\",\"fav\":\"";
    if b.len() != 2 * PHOTO_N + HEAD.len() + MID.len() + 2 {
        return None;
    }
    if &b[..HEAD.len()] != HEAD || &b[HEAD.len() + PHOTO_N..HEAD.len() + PHOTO_N + MID.len()] != MID {
        return None;
    }
    if &b[b.len() - 2..] != b"\"}" {
        return None;
    }
    let mut del = 0u16;
    let mut fav = 0u16;
    for i in 0..PHOTO_N {
        let d = b[HEAD.len() + i];
        let f = b[HEAD.len() + PHOTO_N + MID.len() + i];
        if (d != b'0' && d != b'1') || (f != b'0' && f != b'1') {
            return None;
        }
        if d == b'1' {
            del |= 1 << i;
        }
        if f == b'1' {
            fav |= 1 << i;
        }
    }
    Some(Album { del, fav })
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
fn album_save(a: &Album) {
    if fs_h() == 0 {
        return; // 无 fs 词授权：结构性零 RPC（内存态照用）
    }
    let mut out = [0u8; JSN_MAX];
    let n = jsn_encode(a, &mut out);
    if let Err(e) = jsn_put(&out[..n]) {
        eprintln!("photos: jsn write errno={e}");
    }
}

/// 启动期读入（缺席/坏读自愈：回默认 + 重写；缺席常态零打印）。
fn album_load() -> Album {
    let d = Album { del: 0, fav: 0 };
    if fs_h() == 0 {
        return d;
    }
    match jsn_read() {
        JsnRead::Absent => {
            album_save(&d);
            d
        }
        JsnRead::Bytes(b, n) => match jsn_parse(&b[..n]) {
            Some(a) => a,
            None => {
                eprintln!("photos: jsn bad -- default");
                album_save(&d);
                d
            }
        },
        JsnRead::Bad(why) => {
            eprintln!("photos: jsn {why} -- default");
            album_save(&d);
            d
        }
    }
}

// —— RPC 半程族（launcher/settings 同形拷贝）——

/// 诊断 + park 收口（app 常驻不被 reap；崩溃监督归 svcmgr
/// app_supervise 域，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("photos: {what}");
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

/// present 域 1B ST_BUSY 拒答的步进重试（berry-appshell pump.rs busy_step
/// 同形）：100ms 步进让出 CPU，总帽 [`BUSY_RETRY_CAP_NS`]（`cap_dl` = 调用点
/// 局部帽位，0 = 首次起算）；到帽 fail。快答未转发 + 随行已关收，重发安全
///（RELEASE_WAIT 零随行——无别名重 dup 面）。
fn busy_step(what: &str, cap_dl: &mut u64) {
    if *cap_dl == 0 {
        *cap_dl = now() + BUSY_RETRY_CAP_NS;
        eprintln!("photos: {what}");
    }
    if now() >= *cap_dl {
        fail(&format!("{what}: 帽到点"));
    }
    if berry_thread_sleep(now() + BUSY_RETRY_STEP_NS) != 0 {
        fail("busy retry sleep");
    }
}

// —— 建树小件 ——

/// 单节点 spec 构造（key=0——调和键路径挂账 v1.5）。
fn vspec(kind: ViewKind, props: Props) -> ViewSpec {
    ViewSpec { kind, key: 0, props }
}

/// Text 叶 spec（超 TEXT_INLINE=64B loud Err——本应用文案 ≤32B 域内）。
fn text_spec(text: &[u8], fg: Color, scale: FontSize) -> Result<ViewSpec, UiError> {
    Ok(vspec(
        ViewKind::Text,
        Props::Text(TextProps {
            style: TextStyle { fg, bg: None, scale },
            text: TextBuf::new(text)?,
        }),
    ))
}

/// 整幅页叠放（Stack + 全幅底 Rect + 内容 Col）——网格页/看片页共用：
/// 底 Rect 保证页域内任意 damage 恒有重绘覆盖源（透明容器 below 内容
/// 的残像素风险封堵）。返回内容锚。
fn page_stack(
    ui: &mut UiMain,
    parent: NodeId,
    bg: Color,
    w: u32,
    h: u32,
) -> Result<NodeId, UiError> {
    let st = ui.add(parent, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Start })))?;
    ui.add(st, vspec(ViewKind::Rect, Props::Rect(RectProps { bg, w: None, h: None, radius: 0 })))?;
    ui.add(
        st,
        vspec(ViewKind::Col, Props::Col(ColProps {
            gap: PAGE_GAP,
            w: Some(w),
            h: Some(h),
            cross: CrossAlign::Stretch,
        })),
    )
}

/// 网格格锚（点击路由 + 按压复位面）。
#[derive(Debug, Clone, Copy)]
struct CellAnchors {
    /// 缩略 Image（命中首锚——内在型叶满盖图域）。
    img: NodeId,
    /// 图域外边带命中的内容 Col（次锚）。
    col: NodeId,
    /// 按压暗框视觉（press_map visual + DragEnd 复位锚——底层面 Rect）。
    mask: NodeId,
    /// 样张序（0..PHOTO_N）。
    photo: usize,
}

/// 看片页运行态（懒建/退出整树 remove 的复用形）。
struct Viewer {
    /// 看片页叠放根（remove 即整页退役）。
    root: NodeId,
    /// 翻片控制器（页数 = min(过滤后长, 8)）。
    pager: SwipePager,
    /// 页数快照（`SwipePager` 无公开计数面——建时侧录）。
    pages: u8,
    /// 滑窗基（过滤列表内窗首序；过滤长 ≤8 时恒 0）。
    base: usize,
    /// 开窗时过滤列表长度。
    count: usize,
    /// 全幅图边长（换绑重挂同尺寸）。
    big: u32,
    /// 各页现行 Image 子锚（换绑 = remove + add）。
    imgs: [Option<NodeId>; 8],
    /// 页位指示 Text（"n / N"）。
    idx_text: NodeId,
    /// 收藏态 Text（"Starred"/空）。
    star_text: NodeId,
}

/// 应用运行态（建树一次落定；网格/看片子树按需重建）。
struct App {
    t: Tokens,
    album: Album,
    /// Seg 过滤档（false = All / true = Favorites）。
    fav_only: bool,
    /// 根叠放（子 = [网格页, 看片页?]——后子盖上）。
    layers: NodeId,
    /// 网格页内容锚。
    page: NodeId,
    /// 分段器（build_page 落位）。
    seg: Option<Seg>,
    /// 诚实标注行 Text。
    status: NodeId,
    /// 网格节点（过滤/删除后整树重建）。
    grid: Option<NodeId>,
    /// 网格格锚（重建时整组换）。
    cells: [Option<CellAnchors>; PHOTO_N],
    viewer: Option<Viewer>,
}

impl App {
    /// 当前过滤列表。
    fn list(&self) -> ([usize; PHOTO_N], usize) {
        filtered_list(&self.album, self.fav_only)
    }

    /// 看片页现行样张序（滑窗 + 当前页）。
    fn cur_photo(&self) -> Option<usize> {
        let v = self.viewer.as_ref()?;
        let i = v.base + v.pager.page() as usize;
        self.list().0.get(i).copied()
    }
}

/// 诚实标注行文案（ASCII；样张 = 程序化图案、无相册真源）。
fn status_bytes(fav_only: bool, n: usize, out: &mut [u8]) -> usize {
    let mut k = 0usize;
    if n == 0 {
        put(out, &mut k, if fav_only { b"No favorites yet" } else { b"No photos left" });
        return k;
    }
    if n >= 10 {
        out[k] = b'0' + (n / 10) as u8;
        k += 1;
    }
    out[k] = b'0' + (n % 10) as u8;
    k += 1;
    put(
        out,
        &mut k,
        if fav_only { b" favorites - sample art" } else { b" photos - sample art" },
    );
    k
}

/// 两位十进制尾接（<100 域——本应用计数面恒 1 位或 2 位）。
fn push_num(out: &mut [u8], k: &mut usize, v: usize) {
    if v >= 10 {
        out[*k] = b'0' + (v / 10) as u8;
        *k += 1;
    }
    out[*k] = b'0' + (v % 10) as u8;
    *k += 1;
}

/// "n / N" 页位文案（≤8B）。
fn idx_bytes(cur: usize, total: usize, out: &mut [u8]) -> usize {
    let mut k = 0usize;
    push_num(out, &mut k, cur + 1);
    put(out, &mut k, b" / ");
    push_num(out, &mut k, total);
    k
}

/// 建网格页静态骨架（TopBar + Seg + 状态行；网格本体见 `rebuild_grid`）。
fn build_page(ui: &mut UiMain, app: &mut App) -> Result<(), UiError> {
    let t = app.t;
    let (vp_w, vp_h) = ui.viewport();
    let content = page_stack(ui, app.layers, t.bg_page, vp_w, vp_h)?;

    // 顶栏（pattern 配方消费——56vp 高、标题起始 16vp、零动作位）。
    pattern::top_bar(ui, content, &t, b"Photos", &[])?;

    // 分段过滤（pattern 配方——32vp 高 Grid 等分 + active 高亮；点击
    // 路由见 run 帧循环：Click 走 Seg::hit 锚 + Release 坐标域兜底）。
    let seg = pattern::seg(ui, content, &t, &[b"All", b"Favorites"], 0)?;

    // 诚实标注行（无相机卷真源——样张为程序化图案）。
    let status = ui.add(content, text_spec(b"12 photos - sample art", t.font_secondary, FontSize::S16x32)?)?;
    let _ = ui.set_a11y(status, ROLE_TEXT, b"status");

    app.page = content;
    app.seg = Some(seg);
    app.status = status;
    Ok(())
}

/// (重)建缩略网格：整树 remove 旧格 + 新 Grid + Card 格 + press_map 整表
/// 重登记 + 状态行换字。cell 尺寸 = 宽向 (vp_w − 2·边距 − 2·缝)/3 与纵向
/// 可用高取小（小视口防溢出——v1 无滚动面，网格恒一屏纳）。
fn rebuild_grid(ui: &mut UiMain, app: &mut App) -> Result<(), UiError> {
    let t = app.t;
    let (vp_w, vp_h) = ui.viewport();
    // 旧格整树退役（press_map 旧锚随新表整替——死锚查询恒 miss 无害）。
    if let Some(g) = app.grid.take() {
        ui.remove(g)?;
    }
    app.cells = [None; PHOTO_N];

    let (list, n) = app.list();
    let mut sbuf = [0u8; 48];
    let sn = status_bytes(app.fav_only, n, &mut sbuf);
    ui.set_text(app.status, &sbuf[..sn])?;
    if n == 0 {
        return Ok(()); // 空网格：仅状态行（网格节点不建）
    }

    // 纵向可用高 = 视口 − 顶栏 − 分段 − 状态行(~32) − 三缝 − 上下留白。
    let rows = ((n as u32 + GRID_COLS - 1) / GRID_COLS).max(1);
    let avail_h = vp_h.saturating_sub(TOPBAR_H + 96 + 32 + 3 * PAGE_GAP + 2 * SIDE_PAD);
    let cell = (vp_w.saturating_sub(2 * SIDE_PAD + (GRID_COLS - 1) * GRID_GAP) / GRID_COLS)
        .min(avail_h.saturating_sub((rows - 1) * GRID_GAP) / rows)
        .max(24); // 下限 8vp：极小调试视口退化微缩格而非负值
    let grid_w = GRID_COLS * cell + (GRID_COLS - 1) * GRID_GAP;

    // 网格容器（行主序等分格；cross=Start——格自带 cell×cell 内在尺寸）。
    let grid = ui.add(
        app.page,
        vspec(ViewKind::Grid, Props::Grid(GridProps {
            cols: GRID_COLS as u8,
            rows: rows as u8,
            gap: GRID_GAP,
            cross: CrossAlign::Start,
            w: Some(grid_w),
            h: None,
        })),
    )?;
    app.grid = Some(grid);

    let inset = PRESS_INSET.min(cell / 4); // 按压暗框边带（小格自适应收窄）
    let imm = cell - 2 * inset; // 缩略图边长（四周留边带）
    for slot in 0..n {
        let photo = list[slot];
        // 格 = Card kind 直载（pattern::card 配方是 48vp 行卡——不适配方
        // 形缩略格；圆角/面底视觉仍 Card kind 自绘单源，圆角常量取
        // pattern::LISTITEM_RADIUS——与 list_item 同源同值）。
        let card = ui.add(
            grid,
            vspec(ViewKind::Card, Props::Card(CardProps { elevation: 0, corner_radius: LISTITEM_RADIUS })),
        )?;
        let st = ui.add(card, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Start })))?;
        // 按压暗框层（底层面 Rect；常态 = 组件面白——与 Card 卡面同色
        // 无缝；Press 经引擎 press_react 换 PRESSED_BG 呈边带暗框——
        // theme 单源，应用零 press 字面量）。
        let mask = ui.add(st, vspec(ViewKind::Rect, Props::Rect(RectProps { bg: t.bg_comp, w: None, h: None, radius: 0 })))?;
        // 内容层（定尺寸撑 Stack；cross=Center 居中缩略图）。
        let col = ui.add(
            st,
            vspec(ViewKind::Col, Props::Col(ColProps {
                gap: 0,
                w: Some(cell),
                h: Some(cell),
                cross: CrossAlign::Center,
            })),
        )?;
        pattern::spacer(ui, col, 1, inset)?;
        let img = ui.add(
            col,
            vspec(ViewKind::Image, Props::Image(ImageProps {
                res: PHOTO_ID_BASE + photo as u16,
                w: Some(imm),
                h: Some(imm),
            })),
        )?;
        pattern::spacer(ui, col, 1, inset)?;
        let _ = ui.set_a11y(img, ROLE_IMAGE, b"photo");
        app.cells[slot] = Some(CellAnchors { img, col, mask, photo });
    }
    press_rebind(ui, &app)?;
    Ok(())
}

/// press_map 整表登记/重登记（trigger = 格 Image + 内容 Col 双锚 →
/// visual = 按压暗框 Rect；帽 PRESS_MAP_MAX=28 ≥ 2×12；visual 全 Rect
/// 过白名单。重登记面 = 网格重建 + DragEnd 暗框复位（active/saved 归零））。
fn press_rebind(ui: &mut UiMain, app: &App) -> Result<(), UiError> {
    let mut press = [(app.status, app.status); 2 * PHOTO_N]; // 值即覆写
    let mut pn = 0usize;
    for c in app.cells.iter().flatten() {
        press[pn] = (c.img, c.mask);
        pn += 1;
        press[pn] = (c.col, c.mask);
        pn += 1;
    }
    ui.set_press_map(&press[..pn])
}

/// (重)建看片页：整幅页叠放 + 顶行 icon_button(Back/Star/Delete) +
/// SwipePager 翻片 + 页位/收藏态文案。`target` = 过滤列表内目标序。
fn open_viewer(ui: &mut UiMain, app: &mut App, target: usize) -> Result<(), UiError> {
    let t = app.t;
    let (vp_w, vp_h) = ui.viewport();
    let (list, n) = app.list();
    if n == 0 || target >= n {
        return Ok(());
    }
    // 旧页退役（复用形：退出即整树 remove，再点重建）。
    if let Some(v) = app.viewer.take() {
        ui.remove(v.root)?;
    }

    let root = ui.add(app.layers, vspec(ViewKind::Stack, Props::Stack(StackProps { align: StackAlign::Start })))?;
    let content = page_stack(ui, root, t.bg_page, vp_w, vp_h)?;

    // —— 顶行：Back … 页位 … 收藏态 … Star Delete（gap=0——间距由
    //    spacer 承载，防定宽列在小视口溢出）——
    let pad = bar_pad(vp_w);
    let star_col_w = if vp_w >= 1200 { 192 } else { 0 }; // 小视口退化：藏收藏态列
    let idx_col_w = 168; // "12 / 12" 7 字 × S24x48 字格上界
    let row = ui.add(
        content,
        vspec(ViewKind::Row, Props::Row(RowProps {
            gap: 0,
            w: Some(vp_w),
            h: Some(TOPBAR_H),
            cross: CrossAlign::Center,
        })),
    )?;
    pattern::spacer(ui, row, pad, 1)?;
    let back = pattern::icon_button(ui, row, icons::BACK as u32, EV_VBACK)?;
    let _ = ui.set_a11y(back.node, ROLE_TEXT, b"viewer back");
    let fixed = 2 * pad + 3 * 168 + idx_col_w + star_col_w + 2 * (SIDE_PAD / 2);
    let flex = vp_w.saturating_sub(fixed) / 2;
    pattern::spacer(ui, row, flex, 1)?;
    let ic = ui.add(
        row,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: Some(idx_col_w), h: None, cross: CrossAlign::Start })),
    )?;
    let idx_text = ui.add(ic, text_spec(b"1 / 1", t.font_primary, FontSize::S24x48)?)?;
    pattern::spacer(ui, row, flex, 1)?;
    let sc = ui.add(
        row,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 0, w: Some(star_col_w), h: None, cross: CrossAlign::Start })),
    )?;
    let star_text = ui.add(sc, text_spec(b"", t.brand, FontSize::S16x32)?)?;
    pattern::spacer(ui, row, SIDE_PAD / 2, 1)?;
    let star = pattern::icon_button(ui, row, ICON_STAR, EV_VSTAR)?;
    let _ = ui.set_a11y(star.node, ROLE_TEXT, b"star");
    pattern::spacer(ui, row, SIDE_PAD / 2, 1)?;
    let del = pattern::icon_button(ui, row, ICON_TRASH, EV_VDEL)?;
    let _ = ui.set_a11y(del.node, ROLE_TEXT, b"delete");

    // —— 翻片区（SwipePager：track + 圆点行由 attach 落位）——
    let area = ui.add(
        content,
        vspec(ViewKind::Col, Props::Col(ColProps { gap: 24, w: Some(vp_w), h: None, cross: CrossAlign::Center })),
    )?;
    let pages = n.min(8) as u8;
    let base = if n > 8 { target.min(n - 8) } else { 0 };
    let mut pager = SwipePager::new(pages);
    pager.attach(ui, area)?;
    let big = (vp_w.saturating_sub(2 * SIDE_PAD))
        .min(vp_h.saturating_sub(TOPBAR_H + 16 + PAGE_GAP + 2 * SIDE_PAD));
    let mut imgs = [None; 8];
    for p in 0..pages as usize {
        let page_node = pager.page_node(p as u8).expect("page in range");
        imgs[p] = Some(ui.add(
            page_node,
            vspec(ViewKind::Image, Props::Image(ImageProps {
                res: PHOTO_ID_BASE + list[base + p] as u16,
                w: Some(big),
                h: Some(big),
            })),
        )?);
        let _ = ui.set_a11y(imgs[p].unwrap(), ROLE_IMAGE, b"photo full");
    }
    pager.snap_to(ui, (target - base) as u8);

    app.viewer = Some(Viewer { root, pager, pages, base, count: n, big, imgs, idx_text, star_text });
    viewer_refresh(ui, app)
}

/// 看片页可变文案刷新（页位 + 收藏态；翻片/换绑/收藏切换后同拍调用）。
fn viewer_refresh(ui: &mut UiMain, app: &mut App) -> Result<(), UiError> {
    let Some(v) = app.viewer.as_ref() else { return Ok(()) };
    let cur = (v.base + v.pager.page() as usize).min(v.count.saturating_sub(1));
    let mut ibuf = [0u8; 8];
    let il = idx_bytes(cur, v.count, &mut ibuf);
    ui.set_text(v.idx_text, &ibuf[..il])?;
    let starred = app.list().0.get(cur).is_some_and(|&p| app.album.is_fav(p));
    ui.set_text(v.star_text, if starred { b"Starred" } else { b"" })?;
    Ok(())
}

/// 滑窗换绑（>8 张时越界侧移一页：8 页 Image 整组 remove+add——
/// `SWIPE_PAGES_MAX`=8 帽的惰性窗口；换页后照片视觉连续（同片重挂））。
fn viewer_shift(ui: &mut UiMain, app: &mut App) -> Result<bool, UiError> {
    let (list, _) = app.list();
    let Some(v) = app.viewer.as_mut() else { return Ok(false) };
    if v.count <= 8 {
        return Ok(false); // 全量在窗：无滑窗面
    }
    let p = v.pager.page() as usize;
    let np = v.pages as usize;
    let (shift, snap) = if p + 1 == np && v.base + np < v.count {
        (1i32, v.pages - 2) // 末页前推：窗右移一，回钉倒数第二页
    } else if p == 0 && v.base > 0 {
        (-1i32, 1) // 首页回溯：窗左移一，钉第二页
    } else {
        return Ok(false);
    };
    v.base = (v.base as i32 + shift) as usize;
    for q in 0..np {
        if let Some(old) = v.imgs[q].take() {
            ui.remove(old)?;
        }
        let page_node = v.pager.page_node(q as u8).expect("page in range");
        v.imgs[q] = Some(ui.add(
            page_node,
            vspec(ViewKind::Image, Props::Image(ImageProps {
                res: PHOTO_ID_BASE + list[v.base + q] as u16,
                w: Some(v.big),
                h: Some(v.big),
            })),
        )?);
    }
    v.pager.snap_to(ui, snap);
    Ok(true)
}

/// 看片页整树退役（Back/Delete 路径）。
fn close_viewer(ui: &mut UiMain, app: &mut App) {
    if let Some(v) = app.viewer.take() {
        if let Err(e) = ui.remove(v.root) {
            eprintln!("photos: viewer remove err={e:?}");
        }
    }
}

/// Seg 过滤切换（set_active 换档 + 网格整树重建 + press_map 重登记）。
fn switch_filter(ui: &mut UiMain, app: &mut App, fav_only: bool) {
    if app.fav_only == fav_only {
        return;
    }
    app.fav_only = fav_only;
    let want = if fav_only { 1 } else { 0 };
    if let Some(seg) = app.seg.as_mut() {
        seg.set_active(ui, want);
    }
    if let Err(e) = rebuild_grid(ui, app) {
        eprintln!("photos: grid rebuild err={e:?}");
    }
    ui.wake(now());
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
        eprintln!("photos: wm absent (no gui gift)");
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
        "photos: wm v={} {}x{} input={}",
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

    // —— 样张供件（启动期一次生成 ≈2.7 MiB 像素板；IconSource 真供件）。
    static PHOTO_SRC: std::sync::OnceLock<PhotoSource> = std::sync::OnceLock::new();
    let src: &'static PhotoSource = PHOTO_SRC.get_or_init(|| PhotoSource { slab: gen_slab() });
    ui.set_icon_source(src);

    // —— 相册态（PHOTOS.JSN 三约束读入）+ 建树（页面骨架占位锚经
    //    build_page 一次落位——NodeId 无外部可造哨兵，两字段先挂根
    //    下的隐形 1×1 spacer 占位，随即被 build_page 覆写）。
    let t = Tokens::pick(Theme::Dark); // 画廊暗档（theme.rs 双档单源）
    let root = ui.root();
    let layers = pattern::stack(ui, root).expect("root stack");
    let ph = pattern::spacer(ui, layers, 1, 1).expect("status placeholder");
    let mut app = App {
        t,
        album: album_load(),
        fav_only: false,
        layers,
        page: layers,
        seg: None,
        status: ph,
        grid: None,
        cells: [None; PHOTO_N],
        viewer: None,
    };
    if let Err(e) = build_page(ui, &mut app) {
        fail(&format!("page build err={e:?}"));
    }
    let _ = ui.remove(ph); // 建树占位锚退役（NodeId 无外部可造哨兵的折衷）
    if let Err(e) = rebuild_grid(ui, &mut app) {
        fail(&format!("grid build err={e:?}"));
    }

    run(hw, input_h(), ui, app);
}

/// 帧循环 + 输入 + 事件消费（settings `run` 镜像；增量 = MsgBuf 消费
/// （SwipePager 翻片 + Seg 过滤 + 缩略格点击）与 take_events 消费
/// （看片页三 icon_button）+ pager.tick 吸附推进）。
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
                                    eprintln!("photos: input push err={e:?}");
                                }
                            }
                        }
                        ui.wake(now());
                    }
                    Some(Err(gp::ST_EAGAIN)) => {} // 无事件常态
                    Some(Err(st)) => eprintln!("photos: input poll st={st}"),
                    None => eprintln!("photos: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("photos: input recv err={s}");
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
        //    MsgBuf 本应用三消费者：①SwipePager 翻片 ②Seg 过滤（Release
        //    坐标域 + Click 锚）③缩略格点击（Click 锚路由）。
        let msgs = ui.pump_input(last_expected);

        // ① 看片页翻片（DragStart/Move/End/Cancel 消费；true = 状态已变
        //    ——滑窗越界侧随拍换绑）。MsgBuf 与 ui/app 互不借用——逐条
        //    直喂，零分配。
        if app.viewer.is_some() {
            let mut moved = false;
            if let Some(v) = app.viewer.as_mut() {
                for m in msgs.iter() {
                    if v.pager.on_msg(ui, m) {
                        moved = true;
                    }
                }
            }
            if moved {
                match viewer_shift(ui, &mut app) {
                    Ok(_) => {}
                    Err(e) => eprintln!("photos: viewer shift err={e:?}"),
                }
                if let Err(e) = viewer_refresh(ui, &mut app) {
                    eprintln!("photos: viewer refresh err={e:?}");
                }
                ui.wake(now());
            }
        }

        // ②③ Seg 过滤 + 缩略格点击 + 拖拽误触暗框复位（看片页在席时
        //    网格被整幅盖住——三路均门控跳过）。
        let mut release_xy: Option<(u32, u32)> = None;
        for m in msgs.iter() {
            match m {
                Msg::Release { x, y, .. } => release_xy = Some((*x, *y)),
                // 引擎 press_react 只在 Release/Cancel 还原换底——拖滚
                // 误触（slop 外抬手）走 DragEnd 无还原臂，暗框会卡死；
                // app 侧复位常态底色（theme token 取色非按压字面量）+
                // press_map 整表重登记（active/saved 归零，还原链重建）。
                Msg::DragEnd { node, .. } => {
                    if app.viewer.is_none() {
                        let hit = app.cells.iter().flatten().find(|c| c.img == *node || c.col == *node);
                        if let Some(c) = hit {
                            let bg = app.t.bg_comp;
                            let _ = ui.set_props(c.mask, Props::Rect(RectProps { bg, w: None, h: None, radius: 0 }));
                            if let Err(e) = press_rebind(ui, &app) {
                                eprintln!("photos: press rebind err={e:?}");
                            }
                            ui.wake(now());
                        }
                    }
                }
                Msg::Click { node } => {
                    if app.viewer.is_none() {
                        let seg_hit = app.seg.as_ref().and_then(|s| s.hit(*node));
                        match seg_hit {
                            Some(i) => switch_filter(ui, &mut app, i == 1),
                            None => {
                                if let Some(slot) = app
                                    .cells
                                    .iter()
                                    .position(|c| c.is_some_and(|c| c.img == *node || c.col == *node))
                                {
                                    let photo = app.cells[slot].unwrap().photo;
                                    let (list, n) = app.list();
                                    let target = (0..n).find(|&k| list[k] == photo).unwrap_or(0);
                                    if let Err(e) = open_viewer(ui, &mut app, target) {
                                        eprintln!("photos: viewer open err={e:?}");
                                    }
                                    ui.wake(now());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if let (Some((x, y)), None) = (release_xy, app.viewer.as_ref()) {
            if let Some(seg) = app.seg.as_ref() {
                if let Ok(b) = ui.bounds(seg.frame) {
                    let (bx, by, bw, bh) = (b.x as u32, b.y as u32, b.w as u32, b.h as u32);
                    if x >= bx && x < bx + bw && y >= by && y < by + bh {
                        let idx = ((x - bx) / (bw / 2).max(1)) as usize;
                        if idx != seg.active {
                            switch_filter(ui, &mut app, idx == 1);
                        }
                    }
                }
            }
        }

        // —— 组件事件消费（widget_react 产物）：看片页三 icon_button。
        let evs = ui.take_events();
        for ev in evs.iter() {
            match ev.event {
                EV_VBACK if app.viewer.is_some() => {
                    close_viewer(ui, &mut app);
                    ui.wake(now());
                }
                EV_VSTAR => {
                    if let Some(p) = app.cur_photo() {
                        app.album.fav ^= 1 << p;
                        album_save(&app.album);
                        if let Err(e) = viewer_refresh(ui, &mut app) {
                            eprintln!("photos: viewer refresh err={e:?}");
                        }
                        ui.wake(now());
                    }
                }
                EV_VDEL => {
                    if let Some(p) = app.cur_photo() {
                        app.album.del |= 1 << p;
                        album_save(&app.album);
                        close_viewer(ui, &mut app); // v1：删即回网格（无邻位续看面）
                        if let Err(e) = rebuild_grid(ui, &mut app) {
                            eprintln!("photos: grid rebuild err={e:?}");
                        }
                        ui.wake(now());
                    }
                }
                _ => {}
            }
        }

        // —— SwipePager 吸附推进（无吸附 false 零开销）。
        if let Some(v) = app.viewer.as_mut() {
            if v.pager.tick(ui, t_now) {
                ui.wake(now());
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
            // ST_BUSY 步进重试环（berry-appshell pump.rs SUBMIT 面同形）：他
            // 客户端（launcher 点击/分钟沿整帧）事务在途时 wmd presarb 快答
            // ST_BUSY 并 close 随行别名——每轮重发前 fresh dup 一枚新别名。
            let mut busy_dl = 0u64;
            let token = loop {
                let mut alias: Handle = 0;
                if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0 || alias == 0 {
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
                println!("photos: first frame"); // 诊断面（console=false 走私有对）
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
        //    min(next_wakeup, now+IDLE 帽)；TIMEDOUT = 期限到正常醒）。
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

/// K 矩形并集包络（增量 pass 的 dirty 域；settings 同形）。
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
            eprintln!("photos: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("photos: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}
