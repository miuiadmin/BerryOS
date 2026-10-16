//! stdhello —— G-eco-d 首个 std 程序（未经修改的 Rust std 面 d-1 探针）
//!
//! 「未经修改」= main 是普通 std 写法，不含任何 berry 特有调用；
//! berry-libc 只以依赖身份挂在链接线上（shim 面）。
//! 验收标记 `STD ok`（smoke rootfs 相断言，d-1c 接线）：所有内部自检过
//! 才打印；断言失败 = panic = abort = park → 标记缺失兜底（G-eco 家族惯例）。
//!
//! 覆盖面（附录 A 真实面 41 的浓缩）：
//! - 分配：Vec collect/push、String、Box<[u8]>（malloc/realloc/free）
//! - fmt：format! + writeln! + flush（write/writev 路径）
//! - eprintln!（stderr 路径）
//! - env::args（d-3a env 块真 argv——argv0 断言 = cpio 名）
//! - is_terminal（ioctl→syscall 桩路径）
//! - Instant/Duration（clock_gettime）+ thread::sleep（clock_nanosleep）
//! - stdin read_to_end（d-3b：read → channel recv + 零长 EOF + 残段缓冲）

use std::io::{IsTerminal, Read, Write};

// TLS 探针（build 侧一次性实证，d-1a）：反汇编 std_tls_probe 读
// `mrs tpidr_el0` 后的地址运算，对照 PROBE 符号地址与 PT_TLS p_vaddr，
// 即得 std/lld 的 TP 惯例 → berry-libc crt0 的 TP_ADJ 常数。运行期不调用。
// `const {}` 初始化让 PROBE 直接落 .tbss（非 const 走惰性静态存储，不进
// TLS 段——首版实踩零符号）；`#[used]` 种子防 gc（--gc-sections 下
// #[no_mangle] 也非自动根，std_tls_probe 曾整件消失）。
thread_local! {
    static PROBE: u32 = const { 0x2B };
}

#[no_mangle]
pub extern "C" fn std_tls_probe() -> u32 {
    // volatile 读：否则 LTO 全程序视野证明值恒 0x2B，把地址运算折成 mov
    // 立即数（首版实踩：函数缩成 2 指令，没留下 TP 数学）。
    PROBE.with(|p| unsafe { core::ptr::read_volatile(p) })
}

#[used]
static TLS_PROBE_SEED: unsafe extern "C" fn() -> u32 = std_tls_probe;

// 链接种子：rlib/LTO 按需保留——无引用则 berry-libc 的 crt0 目标件（含
// `_start`）不进链接（首版实踩：lld 警告 cannot find entry symbol _start）。
// `#[used]` 防死码消除，函数指针初始化产生真实重定位。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    // —— 分配面 ——
    let mut v: Vec<u32> = (1..=100u32).collect();
    let sum: u32 = v.iter().sum();
    assert_eq!(sum, 5050);
    v.push(sum);
    let s = String::from_utf8_lossy(format!("sum={} len={}\n", sum, v.len()).as_bytes()).into_owned();
    let b: Box<[u8]> = s.clone().into_bytes().into_boxed_slice();
    assert_eq!(b.len(), s.len());

    // —— argv / tty 面 ——
    // d-3a 起 crt0 从 env 块收真 argv（svcmgr 送 cpio 名作 argv0）。
    let arg0 = std::env::args().next();
    assert_eq!(arg0.as_deref(), Some("bin/stdhello"));
    let argc = std::env::args().count();
    assert_eq!(argc, 1, "env 块 argv = [argv0, NULL]");
    let _tty = std::io::stdout().is_terminal();

    // —— clock 面（先睡 1ms 让 clock_nanosleep 真过一次）——
    let t0 = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(1));
    assert!(t0.elapsed().as_nanos() > 0);

    // —— stdin 面（d-3b：env 块 stdin 端 + channel recv + 零长 EOF）——
    // feeder（svcmgr）泵 8×"berryos!" = 64B + EOF 标记；read_to_end 的
    // 逐次 read 会以小于消息长的粒度要数据——libc 残段缓冲面的活证。
    let mut stdin_buf = Vec::new();
    std::io::stdin().read_to_end(&mut stdin_buf).unwrap();
    assert_eq!(stdin_buf.len(), 64, "stdin = feeder 64B pattern");
    assert!(stdin_buf.chunks(8).all(|c| c == b"berryos!"));

    // —— 输出面（标记最后打：全部自检过了才轮到它）——
    let mut out = std::io::stdout();
    writeln!(out, "STD ok: sum={} heap_use={} argc={} in={}", sum, b.len(), argc, stdin_buf.len())
        .unwrap();
    out.flush().unwrap();
    eprintln!("STD err ok");
}
