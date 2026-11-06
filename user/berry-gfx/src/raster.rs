//! soft 光栅执行器（**私有模块**——只经 `queue::Queue::submit` 驱动，
//! 无独立公开面）。像素语义与 berry-fbcon render.rs 共识契约逐位同：
//!
//! - 落盘：u32 小端 `to_le_bytes` 字节拷贝（不赌目标缓冲 4 字节对齐）；
//! - 寻址：`off = y*stride + x*4`（stride = 行字节数，**行尾 padding
//!   结构性不可达**——所有写经 [`put_px`] 单口执法）；
//! - glyph 位序：MSB = 最左列（`bits & (0x80 >> gx)`，fbcon 对偶）——
//!   段② 起经 `GlyphBits::bit` 推广到任意几何（字形蜂格与图像/
//!   图标位图同一 [`mono_blit`] 共核，一条位法两处消费）；ARGB 面
//!   经 [`argb_blit`]（BYI1 解码产物消费口——A 位 1-bit 透明键，无
//!   blend，铁律不破）；
//! - blit 重叠：**memmove 语义**——逐行 `core::ptr::copy`（行内水平
//!   重叠安全；`copy_from_slice` 自重叠 = UB，P6 A-7/fbcon 滚动同款
//!   立法）+ 行序按重叠方向取安全向（dst 整体在 src 下方时自底向上，
//!   否则自顶向下——垂直重叠安全；两向合并覆盖对角重叠：任一目的行
//!   被写入前，其可能覆盖的未读源行都已读过）。
//!
//! 裁剪纪律：所有写像素裁到 `clip`（= 纹理界 ∩ dirty 区，submit 期
//! 构造）；[put_px] 对纹理界再直检一次——防御未来调用方传宽松 clip。

use crate::cmd::{Cmd, GlyphRun, ImageSrc, Rect};
use crate::color::TextureDesc;
use crate::font::GlyphBits;

/// 命令分派（回放入口——`Queue::submit` 逐命令驱动）。
pub(crate) fn exec(target: &mut [u8], desc: &TextureDesc, cmd: &Cmd, clip: &Rect) {
    match cmd {
        Cmd::FillRect { rect, paint } => {
            let px = desc.format().pack(paint.color.r, paint.color.g, paint.color.b);
            fill_rect(target, desc, rect, px, clip);
        }
        Cmd::DrawLine { x0, y0, x1, y1, paint } => {
            let px = desc.format().pack(paint.color.r, paint.color.g, paint.color.b);
            line(target, desc, *x0, *y0, *x1, *y1, px, clip);
        }
        Cmd::Blit { src, dst_x, dst_y } => blit(target, desc, src, *dst_x, *dst_y, clip),
        Cmd::DrawGlyphRun { run } => glyph_run(target, desc, run, clip),
        Cmd::DrawImage { src, x, y, fg, bg } => {
            draw_image(target, desc, src, *x, *y, *fg, *bg, clip)
        }
        Cmd::Present { .. } => {
            // soft：整帧已直写，无可「翻」——flip 语义留真后端（双缓冲/
            // virtio-gpu）。此处显式 no-op 而非拒面：Present 是合法命令。
        }
    }
}

/// 单像素落盘（统一写口）：stride 寻址 + u32 小端字节拷贝 + 双重界检。
///
/// 界内证明（直索引无 panic 的依据）：x∈[0,w)、y∈[0,h) 时
/// `off+4 ≤ (h-1)*stride + w*4 = need ≤ target.len()`（desc 构造期 +
/// submit 入口 `check_buf` 双执法钉死）——乘加无需逐写点再 checked
/// （先验界内再算，模块级纪律）。
#[inline]
fn put_px(target: &mut [u8], desc: &TextureDesc, clip: &Rect, x: i32, y: i32, px: u32) {
    // 第一重：clip 界（dirty 区外 = 别人的画面）。
    if x < clip.x
        || y < clip.y
        || x >= clip.x.saturating_add(clip.w)
        || y >= clip.y.saturating_add(clip.h)
    {
        return;
    }
    // 第二重：纹理界（防御面——clip ⊆ 纹理界由构造保证，此检防未来
    // 调用方漂移；负坐标在此一并挡下）。
    let (xu, yu) = match (usize::try_from(x), usize::try_from(y)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => return,
    };
    if xu >= desc.w() || yu >= desc.h() {
        return;
    }
    let off = yu * desc.stride() + xu * 4;
    target[off..off + 4].copy_from_slice(&px.to_le_bytes());
}

/// 矩形填充：rect ∩ clip 后整铺（空交叠 = no-op；零宽/高在此自然短路）。
pub(crate) fn fill_rect(target: &mut [u8], desc: &TextureDesc, rect: &Rect, px: u32, clip: &Rect) {
    let Some(r) = rect.intersect(clip) else { return };
    let (y_end, x_end) = (r.y.saturating_add(r.h), r.x.saturating_add(r.w));
    let mut y = r.y;
    while y < y_end {
        let mut x = r.x;
        while x < x_end {
            put_px(target, desc, clip, x, y, px);
            x += 1;
        }
        y += 1;
    }
}

/// 整数 Bresenham 线（全八分位、零浮点）。i64 中转防端点差值溢出
/// （i32::MIN→MAX 差 2^32）。
///
/// **巨跨度防线（审查 R2-F1）**：先反解「主轴落带步窗」再进循环。
/// 本变体的不变量：主轴（|Δ| 大者）每步必进 1、两轴坐标单调——落在
/// clip 带内的步数区间 [k_lo, k_hi] 可对主轴坐标 `c(k) = c0 + s·k`
/// 直接解不等式；窗外前导/尾随步不迭代。k_lo 处状态（x/y/err）经
/// **闭式重构**：次轴步进数 `b = ⌊(2k·mino + dom)/(2·dom)⌋`（半上
/// 取整 DDA 折算，dom/mino = 主/次轴全程差），err 经残差式
/// `resid = k·mino − b·dom ∈ [−dom/2, dom/2)` 重构（直接展开
/// `e + k·dy + b·dx` 的巨项相消——k·mino 可达 2^64，i128 中转）。
/// 等价性（落点与逐步推进逐像素同）由 host 测试 `lines.rs` 以 2401
/// 段网格端点对拍执法。**迭代上界 = 带宽 + 2**：敌意巨端点
/// （i32::MIN→MAX）与本帧带内的短段同阶完成。
fn line(target: &mut [u8], desc: &TextureDesc, x0: i32, y0: i32, x1: i32, y1: i32, px: u32, clip: &Rect) {
    let (x, y) = (x0 as i64, y0 as i64);
    let (x1, y1) = (x1 as i64, y1 as i64);
    let dx = (x1 - x).abs(); // dom 候选（≥ 0）
    let dy = -(y1 - y).abs(); // ≤ 0（本变体约定）
    let (sx, sy) = (if x < x1 { 1 } else { -1 }, if y < y1 { 1 } else { -1 });
    let total = dx.max(-dy); // 总步数 = 主轴全程（单点 = 0）
    if total == 0 {
        put_px(target, desc, clip, x0, y0, px); // 单点：无窗可解
        return;
    }
    // —— 主轴落带步窗：clip ⊆ 纹理界（submit 构造），投影值域 [0,2^31)
    //    恒 i64 直算；c0 ± band ≤ 2^31 + 2^32 < 2^63 减法安全 ——
    let shallow = dx >= -dy;
    let (c0, s, band_lo, band_hi) = if shallow {
        (x, sx, clip.x as i64, clip.x as i64 + clip.w as i64 - 1)
    } else {
        (y, sy, clip.y as i64, clip.y as i64 + clip.h as i64 - 1)
    };
    let k_lo = if s > 0 { (band_lo - c0).max(0) } else { (c0 - band_hi).max(0) };
    let k_hi = if s > 0 { (band_hi - c0).min(total) } else { (c0 - band_lo).min(total) };
    if k_lo > k_hi {
        return; // 主轴投影与带不相交 → 整线零落带像素（窗外步全被裁）
    }
    // —— 闭式状态重构 @k_lo：dom > 0 两分支皆成立（total > 0 时主轴
    //    差恒正——shallow 分支 dx=0 ⟹ E=0 ⟹ total=0 已被单点挡下）——
    let (dom, mino) = if shallow { (dx, -dy) } else { (-dy, dx) };
    let kk = k_lo as i128;
    let b = ((2 * kk * mino as i128 + dom as i128) / (2 * dom as i128)) as i64;
    let resid = kk * mino as i128 - b as i128 * dom as i128; // 恒小值（|resid| < dom/2 + dom）
    let (mut cx, mut cy, mut err) = if shallow {
        // err_top(k) = (D−E) − (kE − bD)
        (x + sx * k_lo, y + sy * b, (dom - mino) as i64 - resid as i64)
    } else {
        // err_top(k) = (D−E') + (kD − bE')（次轴项走 dx，符号面镜像）
        (x + sx * b, y + sy * k_lo, (mino - dom) as i64 + resid as i64)
    };
    let mut k = k_lo;
    loop {
        // 轨迹坐标单调夹在两端点（均 i32）之间——try_from 纯防御。
        if let (Ok(xi), Ok(yi)) = (i32::try_from(cx), i32::try_from(cy)) {
            put_px(target, desc, clip, xi, yi, px);
        }
        if (cx == x1 && cy == y1) || k >= k_hi {
            break; // 到端点 或 出带（带内最后一步已画）
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
        k += 1;
    }
}

/// 纹理内区域拷贝（memmove 语义——见模块头行序论证）。src 整区在
/// 纹理界内由 submit 预检（`BlitSrcOutOfBounds` loud）；dst 越界部分
/// 裁剪（可见区 = dst ∩ clip，源区按平移量反解）。
fn blit(target: &mut [u8], desc: &TextureDesc, src: &Rect, dst_x: i32, dst_y: i32, clip: &Rect) {
    if src.w == 0 || src.h == 0 {
        return; // 零尺寸 = 合法空操作
    }
    let dst = Rect::new(dst_x, dst_y, src.w, src.h);
    let Some(vis) = dst.intersect(clip) else { return };
    let n = vis.h; // 目的可见行数
    let stride = desc.stride();
    let base = target.as_mut_ptr();
    for i in 0..n {
        // 行序安全向：dst 整体在 src 下方（dst_y > src.y）时自底向上，
        // 否则自顶向下（重叠双向都安全；不重叠时两序等价）。
        let ri = if dst_y > src.y { n - 1 - i } else { i };
        let dy = vis.y.saturating_add(ri); // 目的行（vis ⊆ 纹理界）
        // 源行/源列 = 目的坐标减平移量（i64 中转——dst 偏移可负巨，
        // dy-dst_y 差值可越 i32 域；结果恒落 [src.y, src.y+src.h) ⊆ 帧）。
        let sy = (dy as i64 - dst_y as i64) + src.y as i64;
        let sx0 = (vis.x as i64 - dst_x as i64) + src.x as i64;
        let (sy, sx0) = match (i32::try_from(sy), i32::try_from(sx0)) {
            (Ok(a), Ok(b)) => (a, b),
            _ => return, // 纯防御：几何上不可达（见上注）
        };
        let src_off = sy as usize * stride + sx0 as usize * 4;
        let dst_off = dy as usize * stride + vis.x as usize * 4;
        // unsafe 论证（fbcon scroll_up 同款）：
        // - 界内：sy/sx0 由 src ⊆ 纹理界预检 + vis ⊆ 帧导出，
        //   max(off)+len ≤ need ≤ target.len()；
        // - `core::ptr::copy` = memmove 语义，行内水平重叠合法；
        // - u8 拷贝，无对齐要求；行序安全向覆盖垂直/对角重叠。
        unsafe {
            core::ptr::copy(base.add(src_off), base.add(dst_off), vis.w as usize * 4);
        }
    }
}

/// 单色位图 blit 共核（段②：字形蜂格与任意几何图像同一位法、同一
/// 执行路）：位图画到 `(dx,dy)`——着墨位 fg、零位 bg（`None` = 透明
/// 不写，区外画面穿透）。位序 MSB = 最左列（`GlyphBits::bit` 单口执法
/// ——fbcon 对偶契约的任意几何推广）。
///
/// 有界性（敌意几何防线）：迭代以 `dst ∩ clip` 可见窗为界（clip ⊆
/// 纹理界 ⇒ 窗 ≤ 纹理面——巨位图成本 = 可见面，不是位图面）；巨
/// w/h 夹到 i32::MAX（域外部分结构性不可见，夹掉零语义损失）；源
/// 行/列经 `bit()` 防御采样（短行/越界恒 false，绝不 panic）。
fn mono_blit(
    target: &mut [u8],
    desc: &TextureDesc,
    bits: &GlyphBits<'_>,
    dx: i32,
    dy: i32,
    fg_px: u32,
    bg_px: Option<u32>,
    clip: &Rect,
) {
    // 巨几何夹取：usize 域外的宽/高只可能画在纹理外（纹理 ≤ i32 域）。
    let dw = i32::try_from(bits.w).unwrap_or(i32::MAX);
    let dh = i32::try_from(bits.h).unwrap_or(i32::MAX);
    let Some(vis) = Rect::new(dx, dy, dw, dh).intersect(clip) else {
        return; // 零尺寸/全出界 = no-op
    };
    let (y_end, x_end) = (vis.y.saturating_add(vis.h), vis.x.saturating_add(vis.w));
    let mut y = vis.y;
    while y < y_end {
        // vis ⊆ dst ⇒ 源行 ∈ [0,dh)；usize 直转安全（减法非负——
        // intersect 的 max(dx,·) 下界保证）。
        let sy = (y - dy) as usize;
        let mut x = vis.x;
        while x < x_end {
            let on = bits.bit((x - dx) as usize, sy);
            let px = match (on, bg_px) {
                (true, _) => fg_px,
                (false, Some(b)) => b,
                (false, None) => {
                    x += 1;
                    continue; // 透明底：零位不写
                }
            };
            put_px(target, desc, clip, x, y, px);
            x += 1;
        }
        y += 1;
    }
}

/// glyph-run：逐字形经图集位图走 [`mono_blit`] 共核。几何随图集档位
/// （8×16 / 16×32 / 24×48——`font` 三档）；位序/小端/pack 逐位复刻
/// fbcon `put_char`（共识契约，1x 档的等价性由 caps_misc/consensus
/// 组逐字节对拍钉死）。缺字形 → 空格语义：`bg=Some` 铺整蜂格、
/// `bg=None` no-op——与「全零位图」逐像素等价（免为任意档几何备静态
/// 空表），fbcon 同形。
fn glyph_run(target: &mut [u8], desc: &TextureDesc, run: &GlyphRun, clip: &Rect) {
    let fg_px = desc.format().pack(run.fg.r, run.fg.g, run.fg.b);
    let bg_px = run.bg.map(|c| desc.format().pack(c.r, c.g, c.b));
    for p in run.glyphs {
        match run.atlas.glyph(p.ch) {
            Some(bits) => mono_blit(target, desc, &bits, p.x, p.y, fg_px, bg_px, clip),
            None => {
                let bg = match bg_px {
                    Some(b) => b,
                    None => continue, // 透明底：空格 = 什么也不画
                };
                // 巨几何夹取同 mono_blit（防御敌意图集实现）。
                let cw = i32::try_from(run.atlas.cell_w()).unwrap_or(i32::MAX);
                let ch = i32::try_from(run.atlas.cell_h()).unwrap_or(i32::MAX);
                fill_rect(target, desc, &Rect::new(p.x, p.y, cw, ch), bg, clip);
            }
        }
    }
}

/// DrawImage 执行臂（段②）：按 `ImageSrc` 变体分派——mono 面
/// （`Bits` 直接位图 / `Icon` 图标槽位——缺图标 = 整命令 no-op，稀疏
/// 表合法形态）走 [`mono_blit`] 共核；ARGB 面走 [`argb_blit`]
/// （`fg/bg` 对该面不适用，忽略）。
fn draw_image(
    target: &mut [u8],
    desc: &TextureDesc,
    src: &ImageSrc<'_>,
    x: i32,
    y: i32,
    fg: crate::cmd::Color,
    bg: Option<crate::cmd::Color>,
    clip: &Rect,
) {
    match src {
        ImageSrc::Bits(b) => {
            let fg_px = desc.format().pack(fg.r, fg.g, fg.b);
            let bg_px = bg.map(|c| desc.format().pack(c.r, c.g, c.b));
            mono_blit(target, desc, b, x, y, fg_px, bg_px, clip);
        }
        ImageSrc::Icon { atlas, id } => match atlas.icon(*id) {
            Some(b) => {
                let fg_px = desc.format().pack(fg.r, fg.g, fg.b);
                let bg_px = bg.map(|c| desc.format().pack(c.r, c.g, c.b));
                mono_blit(target, desc, &b, x, y, fg_px, bg_px, clip);
            }
            None => {} // 缺图标 no-op（icon.rs 头注语义）
        },
        ImageSrc::Argb { w, h, px } => argb_blit(target, desc, *w, *h, px, x, y, clip),
        ImageSrc::ScaledArgb { sw, sh, dw, dh, px } => {
            scaled_argb_blit(target, desc, *sw, *sh, *dw, *dh, px, x, y, clip)
        }
        ImageSrc::SmoothArgb { sw, sh, dw, dh, px } => {
            smooth_argb_blit(target, desc, *sw, *sh, *dw, *dh, px, x, y, clip)
        }
    }
}

/// ARGB 彩色位图 blit（段②：`icon::decode` 产物的绘制消费口）：
/// `px` = `0xAARRGGBB` row-major 画到 `(dx,dy)`。
///
/// **A 位 = 1-bit 透明键**：`A==0` 跳过不写（区外画面穿透——图标
/// 圆角/异形轮廓的透明底）；`A!=0` 一律不透明覆盖 RGB（`(px>>16)&0xFF`
/// 等三通道 `pack` 经 [`put_px`] 单口落盘）。**这不是 blend**：无
/// `src*a+dst*(1-a)` 混合——半透明是留位的真 blend 语义，v1 铁律
/// 不破；与 mono 面 `bg=None` 零位不写同族（1-bit 透明先例）。
///
/// 有界性/防御（与 [`mono_blit`] 同纪律）：迭代以 `dst ∩ clip` 可见窗
/// 为界（巨位图成本 = 可见面）；巨 w/h 夹到 i32::MAX（usize 域外结构
/// 性不可见）；源像素经 `px.get()` 防御采样（submit 恰长门是第一道，
/// 手搓旁路的短切片在此恒 `None` = 透明，绝不 panic）。
fn argb_blit(
    target: &mut [u8],
    desc: &TextureDesc,
    w: u32,
    h: u32,
    px: &[u32],
    dx: i32,
    dy: i32,
    clip: &Rect,
) {
    // 巨几何夹取：usize 域外的宽/高只可能画在纹理外（纹理 ≤ i32 域）。
    let dw = i32::try_from(w).unwrap_or(i32::MAX);
    let dh = i32::try_from(h).unwrap_or(i32::MAX);
    let Some(vis) = Rect::new(dx, dy, dw, dh).intersect(clip) else {
        return; // 零尺寸/全出界 = no-op
    };
    let (y_end, x_end) = (vis.y.saturating_add(vis.h), vis.x.saturating_add(vis.w));
    let mut y = vis.y;
    while y < y_end {
        // vis ⊆ dst ⇒ 源行 ∈ [0,dh)——usize 直转安全。行基址乘法在
        // u64 域（行号 < 2^31 × w < 2^32 ⇒ < 2^63，64 位目标无溢出；
        // 本仓目标全 64 位——aarch64/x86_64/host）。
        let row_base = ((y - dy) as usize) * (w as usize);
        let mut x = vis.x;
        while x < x_end {
            let p = px.get(row_base + (x - dx) as usize).copied();
            if let Some(p) = p {
                if p >> 24 != 0 {
                    let r = ((p >> 16) & 0xFF) as u8;
                    let g = ((p >> 8) & 0xFF) as u8;
                    let b = (p & 0xFF) as u8;
                    let packed = desc.format().pack(r, g, b);
                    put_px(target, desc, clip, x, y, packed);
                } // A==0：透明键跳过（不写）
            } // get() None = 防御采样（短切片旁路 → 透明，不 panic）
            x += 1;
        }
        y += 1;
    }
}

/// ScaledArgb 最近邻回放臂：目标可见窗逐像素取源 `(x*sw/dw, y*sh/dh)`
///（中心偏移取整，放大无系统性左偏）。透明键与 Argb 同律；u64 乘法
/// 只做坐标映射，源下标最终以 usize 收窄（可见窗已保证界内）。
fn scaled_argb_blit(
    target: &mut [u8],
    desc: &TextureDesc,
    sw: u32,
    sh: u32,
    dw: u32,
    dh: u32,
    px: &[u32],
    dx: i32,
    dy: i32,
    clip: &Rect,
) {
    let dw_i = i32::try_from(dw).unwrap_or(i32::MAX);
    let dh_i = i32::try_from(dh).unwrap_or(i32::MAX);
    let Some(vis) = Rect::new(dx, dy, dw_i, dh_i).intersect(clip) else {
        return;
    };
    let (x_end, y_end) = (vis.x.saturating_add(vis.w), vis.y.saturating_add(vis.h));
    let mut y = vis.y;
    while y < y_end {
        let sy = (((y - dy) as u64 * sh as u64 + dh.max(1) as u64 / 2)
            / dh.max(1) as u64)
            .min(sh.saturating_sub(1) as u64);
        let mut x = vis.x;
        while x < x_end {
            let sx = (((x - dx) as u64 * sw as u64 + dw.max(1) as u64 / 2)
                / dw.max(1) as u64)
                .min(sw.saturating_sub(1) as u64);
            let p = px.get((sy as usize) * sw as usize + sx as usize).copied();
            if let Some(p) = p {
                if p >> 24 != 0 {
                    let r = ((p >> 16) & 0xFF) as u8;
                    let g = ((p >> 8) & 0xFF) as u8;
                    let b = (p & 0xFF) as u8;
                    let packed = desc.format().pack(r, g, b);
                    put_px(target, desc, clip, x, y, packed);
                }
            }
            x += 1;
        }
        y += 1;
    }
}


/// SmoothArgb 双线性回放臂（放大质量面——缩小无低通会走样）：目标中心
/// 映射回源中心，Q16 求floor/Q8 权重。
/// 透明键仍二值：四个采样 alpha 加权 ≥128 视为不透明；RGB 只按不透明
/// 采样归一，避免透明底 (0,0,0) 在边缘渗黑。壁纸源全不透明，主路径无分支。
fn smooth_argb_blit(
    target: &mut [u8],
    desc: &TextureDesc,
    sw: u32,
    sh: u32,
    dw: u32,
    dh: u32,
    px: &[u32],
    dx: i32,
    dy: i32,
    clip: &Rect,
) {
    let dw_i = i32::try_from(dw).unwrap_or(i32::MAX);
    let dh_i = i32::try_from(dh).unwrap_or(i32::MAX);
    let Some(vis) = Rect::new(dx, dy, dw_i, dh_i).intersect(clip) else {
        return;
    };
    let (x_end, y_end) = (vis.x.saturating_add(vis.w), vis.y.saturating_add(vis.h));
    let mut y = vis.y;
    while y < y_end {
        let mut x = vis.x;
        while x < x_end {
            // 目标像素中心 → 源坐标（Q16；-0.5 边界由 clamp 退化为边缘色）。
            let cx = ((2 * (x - dx) as i64 + 1) * sw as i64 * 65536
                / (2 * dw as i64))
                - 32768;
            let cy = ((2 * (y - dy) as i64 + 1) * sh as i64 * 65536
                / (2 * dh as i64))
                - 32768;
            let x0 = (cx / 65536).clamp(0, sw.saturating_sub(1) as i64) as usize;
            let y0 = (cy / 65536).clamp(0, sh.saturating_sub(1) as i64) as usize;
            let x1 = (x0 + 1).min(sw.saturating_sub(1) as usize);
            let y1 = (y0 + 1).min(sh.saturating_sub(1) as usize);
            let fx = ((cx - (x0 as i64) * 65536).max(0) / 256) as u32; // 0..=255
            let fy = ((cy - (y0 as i64) * 65536).max(0) / 256) as u32;
            let p00 = px.get(y0 * sw as usize + x0).copied().unwrap_or(0);
            let p01 = px.get(y0 * sw as usize + x1).copied().unwrap_or(0);
            let p10 = px.get(y1 * sw as usize + x0).copied().unwrap_or(0);
            let p11 = px.get(y1 * sw as usize + x1).copied().unwrap_or(0);
            let samples = [(p00, (256 - fx) * (256 - fy)), (p01, fx * (256 - fy)), (p10, (256 - fx) * fy), (p11, fx * fy)];
            let mut aw = 0u32;
            for (p, w) in samples {
                if p >> 24 != 0 {
                    aw += w / 256;
                }
            }
            if aw >= 128 {
                let mut rgb = [0u32; 3];
                let mut norm = 0u32;
                for (p, w) in samples {
                    if p >> 24 != 0 {
                        norm += w / 256;
                        rgb[0] += ((p >> 16) & 0xFF) * (w / 256);
                        rgb[1] += ((p >> 8) & 0xFF) * (w / 256);
                        rgb[2] += (p & 0xFF) * (w / 256);
                    }
                }
                if norm != 0 {
                    let r = (rgb[0] / norm) as u8;
                    let g = (rgb[1] / norm) as u8;
                    let b = (rgb[2] / norm) as u8;
                    let packed = desc.format().pack(r, g, b);
                    put_px(target, desc, clip, x, y, packed);
                }
            }
            x += 1;
        }
        y += 1;
    }
}
