//! guifbd —— GUI-1（M-GUI 首刀）framebuffer 显示服务：V-6 刀 2 后段
//! 挂账收口（fbcon 自绘字形 + 蜂格渲染核的首个真机消费面）。
//!
//! **boot 授能链**（内核 main.rs fb 授能块，A-2 virtio MMIO 窗同模式）：
//! EFI-GOP 命中时 fb 窗（RamfbDxe AllocatePages 产物，普通 DRAM）包成
//! MMIO VMO 插 svcmgr 表第 5 插（`H_FB=0x0001_0005`）；几何五域
//! fb_pa/w/h/stride/fmt 落 40B 匿名 VMO（u64 LE）第 6 插
//! （`H_FBINFO=0x0001_0006`）。svcmgr 预拉段 spawn 本服务：fb 窗经
//! `SpawnSpec::mmio_vmo` 映到固定 [`FB_VA`]（berry-exec 单源），info
//! VMO 经 env 块 extras[0] 随行（libc `gift2_h()`——rootfs_h 同槽别名
//! 惯例）。非 EFI 路（-kernel 无 GOP）svcmgr 探测失败不 spawn——本服务
//! 恒只在 EFI 链存活。
//!
//! **自验三步**（像素读写对拍，非 grep 串口——smoke 断言在标记本身）：
//! ①clear 双相位差分（bg0→抽样==pack(bg0)→bg1→同点复验——防「写没生效
//! 读回恒旧」的 buddy 双写假绿）；②put_str("AB\nCD") 蜂格全对拍（字形
//! 位图逐像素，MSB=最左列——font 模块约定）；③scroll_up(1) 后行移对拍
//! （memmove 路径 + 滚出空行=bg）。全过才 `println!("FBW ok …")`（stdout
//! = console 发端，vblk `VBLK ok` 同款）→ park 挂起（标记缺失 =
//! smoke-image FAIL loud 兜底）。抽样读回走裸指针 `read_volatile`——
//! Fbcon 持有 fb slice 的 &mut 借用，读回须独立通道；ramfb 实为 DRAM，
//! volatile 是 Device 映射语义下的保守保险（零成本）。
//!
//! 段②（console fan-out 上屏）：park 换 **serve 循环**（fsmgr 形态）——
//! 收 console 转发的输出字节流 put_str 上屏（窗口与串口同内容；boot 早期
//! guifbd 未起时的输出丢 = console 侧 best-effort 既定语义）。首条消息到达
//! 一次性自打 `FBSERVE ok`（稳态矩阵打印持续不断 → 首条必达 = 状态谓词
//! 非竞速，4-5a 铁律）；空队 -EAGAIN → wait READABLE 10s 帽**重臂**续睡
//! （console 长静默合法空闲非断链——echo trap-on-timeout 反例见 fsmgr 头
//! 注）；非 -EAGAIN 错码 = 通道链断 trap（boot 定值接线，错即接线 bug，
//! 只死自己 + `FBSERVE ok` 缺失 = smoke FAIL loud 兜底）。

use berry_exec::FB_VA;
use berry_fbcon::font::{self, GLYPH_H};
use berry_fbcon::render::{Fbcon, PixelFormat};
use berry_libc::io::{gift2_h, gift3_h};
use berry_sys::{
    berry_channel_recv, berry_clock_read, berry_object_wait_one, berry_vmo_read, Handle,
    BERRY_ERR_AGAIN, CLOCK_MONOTONIC, SIGNAL_READABLE,
};

// 链接种子（fsmgr 同款）：rlib/LTO 按需保留——无引用则 berry-libc 的 crt0
// （含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// 抽样读单像素（界内坐标）——裸指针 `read_volatile`，见模块头（避开
/// Fbcon 的 slice 借用）。
fn px(stride: usize, x: usize, y: usize) -> u32 {
    let off = y * stride + x * 4;
    // SAFETY: FB_VA 映射幅 = ceil_page(stride×h)（boot create_mmio 双页
    // 对齐执法）；调用点全部来自 boot info VMO 的几何界内坐标。
    unsafe { core::ptr::read_volatile((FB_VA as usize + off) as *const u32) }
}

/// 蜂格全对拍：字形 GLYPH_H×8 位图逐像素（bit = glyph[y] & (0x80>>x)，
/// MSB=最左列）——期望值与 Fbcon::put_char 同一编码路径（pack(fg/bg)）。
fn cell_ok(stride: usize, fmt: PixelFormat, col: usize, row: usize, ch: u8,
           fg: (u8, u8, u8), bg: (u8, u8, u8)) -> bool {
    let g = match font::glyph(ch) {
        Some(g) => g,
        None => return false,
    };
    for y in 0..GLYPH_H {
        for x in 0..8 {
            let want = if g[y] & (0x80 >> x) != 0 {
                fmt.pack(fg.0, fg.1, fg.2)
            } else {
                fmt.pack(bg.0, bg.1, bg.2)
            };
            if px(stride, col * 8 + x, row * GLYPH_H + y) != want {
                return false;
            }
        }
    }
    true
}

fn main() {
    // 几何 info VMO（boot 40B 五域 u64 LE：fb_pa/w/h/stride/fmt）。
    let h: Handle = gift2_h();
    if h == 0 {
        // svcmgr 探测门后恒随行 info——理论不可达；eprintln loud 兜底。
        eprintln!("guifbd: no fbinfo gift");
        berry_libc::park();
    }
    let mut info = [0u8; 40];
    if berry_vmo_read(h, info.as_mut_ptr(), 0, info.len()) != 0 {
        eprintln!("guifbd: fbinfo read failed");
        berry_libc::park();
    }
    let rd = |i: usize| {
        u64::from_le_bytes(info[i * 8..i * 8 + 8].try_into().unwrap()) as usize
    };
    let (_fb_pa, w, h, stride) = (rd(0), rd(1), rd(2), rd(3));
    let fmt_raw = (rd(4) as u64 & 0xFFFF_FFFF) as u32;
    let fmt = match PixelFormat::from_raw(fmt_raw) {
        Some(f) => f,
        None => {
            // BitMask(2)/BltOnly(3)：from_raw 宁缺勿猜（ramfb 实测恒
            // Bgrx=1，此路理论不可达）——降级不阻塞主链。
            eprintln!("guifbd: unsupported fmt={}", fmt_raw);
            berry_libc::park();
        }
    };
    // fb 窗借用（映射幅 = ceil_page(stride*h) ≥ 本幅——boot 双页对齐执法）。
    // SAFETY: mmio_vmo 映射在 spawn 期由 berry-exec 落位 FB_VA；几何来自
    // boot info VMO（与映射幅同源构造），界内必然已映射。
    let fb: &mut [u8] =
        unsafe { core::slice::from_raw_parts_mut(FB_VA as *mut u8, stride * h) };
    let mut con = match Fbcon::new(fb, w, h, stride, fmt) {
        Some(c) => c,
        None => {
            eprintln!("guifbd: fbcon geometry rejected");
            berry_libc::park();
        }
    };

    // —— 自验三步（确定性抽样点：四角内缩 + 中心 + 两个四分位）——
    // 抽样面收敛**蜂格区**（cols×8 × rows×16）：fbcon 铁律——右 w%8 /
    // 底 h%16 sliver 不属 console「一律不触碰」（render.rs 模块头），
    // clear 只盖蜂格面；首跑实踩 @1,598（600 高 → 蜂格 592，底 sliver
    // 是固件残留字节）差分误炸——sliver 内容本服务无权假设。
    let bg0 = (0x00u8, 0x00u8, 0x80u8); // 深蓝
    let bg1 = (0x00u8, 0x00u8, 0x00u8); // 黑
    let fg = (0xFFu8, 0xFFu8, 0xFFu8); // 白
    let (cw, chh) = (con.cols() * 8, con.rows() * 16);
    let pts = [
        (1usize, 1usize), (cw - 2, 1), (1, chh - 2), (cw - 2, chh - 2),
        (cw / 2, chh / 2), (cw / 4, chh / 4), (3 * cw / 4, 3 * chh / 4),
    ];
    // ① clear 双相位差分。
    con.clear(bg0);
    for &(x, y) in &pts {
        if px(stride, x, y) != fmt.pack(bg0.0, bg0.1, bg0.2) {
            eprintln!("guifbd: fill phase0 mismatch @{},{}", x, y);
            berry_libc::park();
        }
    }
    con.clear(bg1);
    for &(x, y) in &pts {
        if px(stride, x, y) != fmt.pack(bg1.0, bg1.1, bg1.2) {
            eprintln!("guifbd: fill phase1 mismatch @{},{}", x, y);
            berry_libc::park();
        }
    }
    // ② put_str 蜂格全对拍（glyph blit 路径）：A(0,0) B(1,0) '\n'→C(0,1) D(1,1)。
    con.put_str("AB\nCD", fg, bg1);
    for (col, row, ch) in [(0usize, 0usize, b'A'), (1, 0, b'B'), (0, 1, b'C'), (1, 1, b'D')] {
        if !cell_ok(stride, fmt, col, row, ch, fg, bg1) {
            eprintln!("guifbd: glyph cell mismatch '{}' @{},{}", ch as char, col, row);
            berry_libc::park();
        }
    }
    // ③ scroll_up(1)：原次行上移对拍（滚出空行 = 最近 bg = bg1）。
    con.scroll_up(1);
    if !cell_ok(stride, fmt, 0, 0, b'C', fg, bg1) || !cell_ok(stride, fmt, 1, 0, b'D', fg, bg1) {
        eprintln!("guifbd: scroll mismatch");
        berry_libc::park();
    }
    println!("FBW ok w={} h={} cells={}x{}", w, h, con.cols(), con.rows());
    // —— 段②：serve 循环（console fan-out 上屏；fsmgr 形态）——
    let hpipe = gift3_h();
    if hpipe == 0 {
        // svcmgr 探测门后 extras[1] 恒随行——理论不可达；eprintln loud +
        // park 兜底（FBSERVE ok 缺失 = smoke FAIL）。
        eprintln!("guifbd: no fb pipe gift");
        berry_libc::park();
    }
    // 空闲重臂帽：wait READABLE 超时 = 重臂续睡（console 长静默合法空闲），
    // 非死亡面（fsmgr 同款——真断链面 = recv 非 -EAGAIN 错码，照 trap）。
    const TIMEOUT_NS: u64 = 10 * 1_000_000_000;
    let mut served = false;
    let mut msg = [0u8; 256];
    loop {
        let mut nb = msg.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(
            hpipe,
            msg.as_mut_ptr(),
            &mut nb,
            core::ptr::null_mut(),
            &mut nh,
        );
        if s != 0 {
            if s == BERRY_ERR_AGAIN {
                // 空队：阻塞等 READABLE（10s 帽，超时重臂——空闲合法面）。
                let mut t = 0u64;
                if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
                    trap();
                }
                let mut obs = 0u64;
                let _ = berry_object_wait_one(hpipe, SIGNAL_READABLE, t + TIMEOUT_NS, &mut obs);
                continue;
            }
            trap(); // 非 -EAGAIN 错码 = 通道链断（boot 定值接线，真死亡面）
        }
        if !served {
            served = true;
            // 首条消息标记（状态谓词非竞速：矩阵打印持续不断 → 首条必达；
            // 标记本身经 stdout 回 console → 串口 + 回扇 fb = 自证闭环单
            // 发，无风暴——put_str 上屏不再回发）。
            println!("FBSERVE ok");
        }
        // 上屏：字节流 put_str（自维护行缓冲/换行滚动/行尾回绕在 Fbcon——
        // '\n'→换行、col==cols 回绕、越底自动 scroll_up）。串口字节流非保证
        // UTF-8，lossy 容错（无效序列 U+FFFD——显示面语义，串口主路不受
        // 影响）。
        let s = String::from_utf8_lossy(&msg[..nb]);
        con.put_str(&s, fg, bg1);
    }
}

/// 自陷：读未映射地址 → D-abort(lower) → `user_fault_exit` 只死自己
/// （fsmgr 同款——链断 loud，`FBSERVE ok` 缺失 = smoke FAIL 兜底）。
#[cold]
fn trap() -> ! {
    // SAFETY: 故意触发的未映射读（0x4000_0000 与用户布局无交集——家族惯例值）。
    unsafe { core::ptr::read_volatile(0x4000_0000usize as *const u64) };
    loop {
        core::hint::spin_loop();
    }
}
