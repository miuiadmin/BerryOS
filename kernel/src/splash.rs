//! boot 启动画面——`FB ok` 到桌面首帧之间的用户可见反馈（黑屏止血）。
//!
//! ## 双模式（fw_cfg `opt/berry/bootmode`：缺省/`splash` = iOS 式；`verbose` = Linux 式）
//! - **Splash（iOS 式）**：黑底 + 白色 B 点阵 logo + 白细进度条 + 百分比
//!   数字。进度由真实内核里程碑驱动（ktest 8 段 / bench / sched / svcmgr）。
//! - **Verbose（Linux 式）**：黑底白字滚日志——boot 单线程窗内内核输出
//!   逐字节镜像上屏（经 HAL `fb_mirror` 钩子）；多任务起步（sched::init
//!   后首切换）即脱钩，矩阵字符流不上屏。字形单源 = `berry-fbcon`。
//!
//! 零漂移纪律：零新增串口输出；无堆、无锁、无 IRQ 依赖（boot 单线程窗
//! 直写）；显示缺席（fb=0）全部 no-op；noktest 生产 boot 同样适用。
//! 生命周期：`init()`（FB ok 后）→ `step()`/`mirror()` → 桌面首帧覆盖。
//!
//! 像素：ramfb XRGB8888（fmt=1），stride 字节——u32 步长 = stride/4。

/// 白（logo/条/文本）。
const WHITE: u32 = 0xFFFFFFFF;
/// 进度条底槽（近黑深灰）。
const TRACK: u32 = 0xFF2A2A2E;

/// splash 总里程碑数（进度分母）——ktest 全量形态 24；noktest 生产形态
/// （ktest/bench/dpg_press 门控缺席）在 init 时按 fw_cfg 探测重设（游标式
/// set_total——见 main.rs init 后立即调）。
pub const TOTAL: u32 = 24;
/// noktest 生产形态总格数（ktest 8 格 + bench 1 格缺席 → 24-9=15，另加
/// 预备起点 2 格视觉补偿：实际起画 2/15）。
pub const TOTAL_NOKTEST: u32 = 15;

/// 平滑显示值分母（每格 64 细分——disp 是 step 的 64 倍尺度；noktest
/// 游标分母 + base_offset 下条/数字均连续渲染，无整格跳变感）。
const SMOOTH_DENOM: u64 = 64;

/// verbose 行缓冲容量（超长行截断——ktest 行最长的 dump 行也远小于此）。
const LINE_CAP: usize = 96;

struct State {
    fb_va: usize,
    w: u32,
    h: u32,
    stride: u32,
    /// true = verbose（滚日志）；false = splash（条）。
    verbose: bool,
    step: u32,
    /// 平滑显示值（分子 × SMOOTH_DENOM 尺度——draw 用它，真实里程碑用 step 钉）。
    disp: u64,
    /// noktest 游标式分母缩放补偿（step + base_offset = 全量尺度值）。
    base_offset: u32,
    total: u32,
    /// verbose 光标（列/行）与行缓冲。
    col: usize,
    row: usize,
    line: [u8; LINE_CAP],
    line_len: usize,
    /// 脱钩标志（sched::init 后首切换 = 多任务期，矩阵流不上屏）。
    detached: bool,
}

/// boot 单线程窗唯一写者。
static mut STATE: State = State {
    fb_va: 0,
    w: 0,
    h: 0,
    stride: 0,
    verbose: false,
    step: 0,
    disp: 0,
    base_offset: 0,
    total: 0,
    col: 0,
    row: 0,
    line: [0; LINE_CAP],
    line_len: 0,
    detached: false,
};

/// fbinfo 第 6 域编码（kernel → guifbd 传递）：0 = 非 GUI；1 = GUI+splash；
/// 2 = GUI+verbose。
pub const FBINFO_GUI_SPLASH: u64 = 1;
pub const FBINFO_GUI_VERBOSE: u64 = 2;

/// 初始化（`FB ok` 后一次；fb_va=0 → 显示缺席，全程 no-op）。
pub fn init(fb_va: usize, w: u32, h: u32, stride: u32, verbose: bool) {
    // SAFETY: boot 单线程窗（sched::init 前唯一执行流）。
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    if fb_va == 0 || w == 0 || h == 0 || stride == 0 {
        return;
    }
    s.fb_va = fb_va;
    s.w = w;
    s.h = h;
    s.stride = stride;
    s.verbose = verbose;
    s.step = 0;
    s.disp = 0;
    s.base_offset = 0;
    s.total = TOTAL;
    s.col = 0;
    s.row = 0;
    s.line_len = 0;
    s.detached = false;
    if verbose {
        clear_screen(s);
    } else {
        redraw(s);
    }
}

/// 游标式总格数重设（noktest 形态在 init 后立即调——ktest/bench 格缺席，
/// 分母缩至 TOTAL_NOKTEST；step 值同缩：内部维护 offset 补偿）。
pub fn set_total(total: u32, offset: u32) {
    // SAFETY: boot 单线程窗。
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    if s.fb_va == 0 {
        return;
    }
    s.total = total.max(1);
    s.base_offset = offset;
    // 重画用当前换算值（offset 补偿后的 disp）。
    s.disp = ((s.step + offset) as u64) * SMOOTH_DENOM;
    redraw(s);
}

/// 里程碑推进（splash 重画条；verbose 打一行「label ok」）。
pub fn step(n: u32, label: &[u8]) {
    // SAFETY: 同 init。
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    if s.total == 0 {
        return;
    }
    if s.verbose {
        // 里程碑行：直接走镜像行缓冲路径（与 uart 流同渲染）。
        for &b in label.iter().chain(b" ok".iter()) {
            mirror(b);
        }
        mirror(b'\n');
    } else if n > s.step {
        s.step = n.min(s.total);
        // 平滑：里程碑到点直接钉住真实值（插值被超越即贴齐）。
        // 全量尺度换算：显示值 = (step + offset) / total——offset 补偿
        // 必加（noktest 缩分母后防倒退：起画 2/15 → step1 钉 3/15）。
        s.disp = ((s.step + s.base_offset) as u64) * SMOOTH_DENOM;
        redraw(s);
    }
}


/// 串口镜像钩子消费面（HAL `fb_mirror` 每 putc 调；verbose 且未脱钩时上屏）。
pub fn mirror(c: u8) {
    // SAFETY: 同 init（boot 单线程窗；多任务期已 detached 直返）。
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    if !s.verbose || s.detached || s.fb_va == 0 {
        return;
    }
    match c {
        b'\n' => {
            flush_line(s);
        }
        b'\r' => {
            s.line_len = 0;
        }
        0x20..=0x7E => {
            if s.line_len < LINE_CAP {
                s.line[s.line_len] = c;
                s.line_len += 1;
            }
        }
        _ => {}
    }
}

/// 脱钩（多任务起步——矩阵流/switches 金丝雀不上屏）。
pub fn detach() {
    // SAFETY: 同 init。
    let s = unsafe { &mut *core::ptr::addr_of_mut!(STATE) };
    s.detached = true;
}

/// 行落屏判定：全字母行 = 矩阵噪声（task_a/b 忙循环字符流），丢弃。
fn flush_line(s: &mut State) {
    if s.line_len == 0 {
        newline(s);
        return;
    }
    let all_alpha = s.line[..s.line_len]
        .iter()
        .all(|&b| b.is_ascii_alphabetic() || b == b' ');
    if all_alpha {
        s.line_len = 0; // 矩阵行不上屏（但 newline 由调用处补）
        newline(s);
        return;
    }
    for i in 0..s.line_len {
        put_char_at_cursor(s, s.line[i]);
    }
    s.line_len = 0;
    newline(s);
}

fn newline(s: &mut State) {
    s.col = 0;
    let max_rows = (s.h as usize) / 16;
    if s.row + 1 >= max_rows {
        scroll_up(s);
        s.row = max_rows - 1;
    } else {
        s.row += 1;
    }
}

fn put_char_at_cursor(s: &mut State, ch: u8) {
    let cols = (s.w as usize) / 8;
    if s.col >= cols {
        newline(s);
    }
    draw_char(s, ch, s.col, s.row);
    s.col += 1;
}

/// fbcon 字形 blit（8×16；MSB=左列；bg 黑 fg 白）。
fn draw_char(s: &State, ch: u8, col: usize, row: usize) {
    let g = berry_fbcon::font::glyph(ch);
    let fb = s.fb_va as *mut u32;
    let w32 = s.stride as usize / 4;
    let x0 = col * 8;
    let y0 = row * 16;
    // 先清格（bg）——字形外的格面全黑。
    for dy in 0..16 {
        // SAFETY: ramfb 线性窗内 8px 段。
        let base = unsafe { fb.add((y0 + dy) * w32 + x0) };
        let seg = unsafe { core::slice::from_raw_parts_mut(base, 8) };
        for px in seg.iter_mut() {
            *px = 0xFF000000;
        }
    }
    if let Some(bits) = g {
        for (dy, &line) in bits.iter().enumerate() {
            for dx in 0..8usize {
                if line & (0x80 >> dx) != 0 {
                    // SAFETY: 单像素写。
                    unsafe { *fb.add((y0 + dy) * w32 + x0 + dx) = WHITE };
                }
            }
        }
    }
}

/// 上滚一行（copy_nonoverlapping 行搬移 + 底行清黑）。
fn scroll_up(s: &mut State) {
    let fb = s.fb_va as *mut u32;
    let w32 = s.stride as usize / 4;
    let w = s.w as usize;
    let total_px = s.h as usize;
    for y in (0..total_px - 16).rev() {
        // SAFETY: 目标行在源行上方——倒序防重叠覆盖。
        unsafe {
            let src = fb.add((y + 16) * w32);
            let dst = fb.add(y * w32);
            core::ptr::copy_nonoverlapping(src, dst, w);
        }
    }
    for yy in total_px - 16..total_px {
        // SAFETY: 底 16 行清黑。
        let base = unsafe { fb.add(yy * w32) };
        let row = unsafe { core::slice::from_raw_parts_mut(base, w) };
        for px in row.iter_mut() {
            *px = 0xFF000000;
        }
    }
}

/// 清整屏。
fn clear_screen(s: &mut State) {
    let fb = s.fb_va as *mut u32;
    let w32 = s.stride as usize / 4;
    let (w, h) = (s.w as usize, s.h as usize);
    for y in 0..h {
        // SAFETY: ramfb 线性窗。
        let base = unsafe { fb.add(y * w32) };
        let row = unsafe { core::slice::from_raw_parts_mut(base, w) };
        for px in row.iter_mut() {
            *px = 0xFF000000;
        }
    }
}

/// splash 整帧重画（logo + 条 + 百分比）。
fn redraw(s: &mut State) {
    clear_screen(s);
    let (w, h, stride) = (s.w as usize, s.h as usize, s.stride as usize);
    let fb = s.fb_va as *mut u32;
    // logo：8×8 点阵「B」（白）。
    const LOGO: [u8; 8] = [
        0b11111100, 0b11000110, 0b11000110, 0b11111100, 0b11000110, 0b11000110, 0b11000110,
        0b11111100,
    ];
    let cell = (w / 60).max(3);
    let lw = 8 * cell;
    let lx = w.saturating_sub(lw) / 2;
    let ly = h * 2 / 5 - 4 * cell;
    for (gy, bits) in LOGO.iter().enumerate() {
        for gx in 0..8usize {
            if bits & (1 << (7 - gx)) != 0 {
                let dot = cell * 3 / 4;
                fill_rect(fb, stride, lx + gx * cell, ly + gy * cell, dot, dot, WHITE);
            }
        }
    }
    // 进度条（白细条 + 深灰槽）——平滑值驱动（disp/64 格尺度换算回格数）。
    let bar_w = w * 7 / 10;
    let bar_h = (h / 150).max(8);
    let bx = (w - bar_w) / 2;
    let by = h * 2 / 3;
    let filled = ((bar_w as u64) * s.disp / (s.total as u64 * SMOOTH_DENOM)) as usize;
    fill_rect(fb, stride, bx, by, bar_w, bar_h, TRACK);
    if filled > 0 {
        fill_rect(fb, stride, bx, by, filled.min(bar_w), bar_h, WHITE);
    }
    // 百分比（HD 字模 1:1 直绘——Helvetica 96px 烘焙，勿放大 fbcon 位图：
    // 像素块放大 = 马赛克，owner 2026-11-13 点名数字渲染须精美）。
    let pct = ((s.disp * 100) / (s.total as u64 * SMOOTH_DENOM)) as u32;
    let chars = [b'0' + (pct / 10 % 10) as u8, b'0' + (pct % 10) as u8, b'%'];
    let gw = |c: u8| crate::splash_pct::PCT_GLYPHS.iter().find(|g| g.0 == c).map(|g| g.1 as usize).unwrap_or(0);
    let tw: usize = chars.iter().map(|c| gw(*c)).sum::<usize>() + (chars.len() - 1) * 10;
    let tx = w.saturating_sub(tw) / 2;
    let ty = by + bar_h + h / 60;
    let mut cx: usize = tx;
    for ch in chars.iter() {
        if let Some(g) = crate::splash_pct::PCT_GLYPHS.iter().find(|g| g.0 == *ch) {
            draw_hd_glyph(s, g, cx as u32, ty as u32);
            cx += g.1 as usize + 10;
        }
    }
}

/// HD 字形 1:1 直绘（u128 行位包；bit (w-1-x) = 列 x）。
fn draw_hd_glyph(s: &State, g: &(u8, u32, u32, [u128; 72]), x: u32, y: u32) {
    let fb = s.fb_va as *mut u32;
    let stride = s.stride as usize;
    let (gw, gh, rows) = (g.1 as usize, g.2 as usize, &g.3);
    for (dy, &line) in rows.iter().take(gh).enumerate() {
        for dx in 0..gw {
            if line & (1u128 << (gw - 1 - dx)) != 0 {
                // SAFETY: ramfb 线性窗内单像素。
                unsafe { *fb.add((y as usize + dy) * (stride / 4) + x as usize + dx) = WHITE };
            }
        }
    }
}

/// 实心矩形。
fn fill_rect(fb: *mut u32, stride: usize, x: usize, y: usize, w: usize, h: usize, c: u32) {
    for yy in y..y.saturating_add(h) {
        // SAFETY: ramfb 线性窗内矩形。
        let base = unsafe { fb.add(yy * (stride / 4) + x) };
        let row = unsafe { core::slice::from_raw_parts_mut(base, w) };
        for px in row.iter_mut() {
            *px = c;
        }
    }
}
