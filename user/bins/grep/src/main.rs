//! C-2c text 批自写 grep：字面子串匹配器（GNU grep `-F` 固定串语义为
//! 默认；BRE/ERE 不做——语义子集，usage 面明示）。上游缺位记录：
//! uu_grep 0.2.0（crates.io 线顶）硬依赖 onig/onig_sys（C 正则库），
//! berry-ld 纯 Rust 链结构性不兼容（2026-10-25 probe 实证无 feature 门）。
//!
//! 旗标面：-i（ASCII 折叠）/ -v（反选）/ -c（计数）/ -n（行号）/ -q
//! （静默）/ -l（文件名）。stdin 回落（无 FILE 时——管道面主消费）。
//! exit：0 命中（-q 命中最优先）/ 1 无命中 / 2 用法或打开/读失败
//! （GNU 同形——含 -v 反选态：选中行存在即 0）。
//!
//! fs 面 = berry-libc open/read/close 直调（bsh cat 同一台机器）。

use std::io::Read;

use berry_libc::fs::{open, O_RDONLY};
use berry_libc::io::{close, read};

// 链接种子（stdhello 同款）：无引用则 berry-libc crt0 不进链接。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

struct Opts {
    inv: bool, // -v
    count: bool, // -c
    lineno: bool, // -n
    quiet: bool, // -q
    files_with: bool, // -l
    fold: bool, // -i
}

fn fold_ascii(s: &[u8]) -> Vec<u8> {
    s.iter()
        .map(|&b| if b.is_ascii_uppercase() { b + 32 } else { b })
        .collect()
}

fn usage() -> ! {
    eprintln!("usage: grep [-i] [-v] [-c] [-n] [-q] [-l] PATTERN [FILE...]");
    std::process::exit(2);
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut o = Opts { inv: false, count: false, lineno: false, quiet: false, files_with: false, fold: false };
    let mut pos: Vec<String> = Vec::new();
    for a in &argv {
        if let Some(rest) = a.strip_prefix('-') {
            // 组合短旗（-in）；裸 "-" 不算旗（stdin 记号，GNU 同形）。
            if !rest.is_empty() && rest.bytes().all(|b| b"ivcnql".contains(&b)) {
                for b in rest.bytes() {
                    match b {
                        b'i' => o.fold = true,
                        b'v' => o.inv = true,
                        b'c' => o.count = true,
                        b'n' => o.lineno = true,
                        b'q' => o.quiet = true,
                        b'l' => o.files_with = true,
                        _ => usage(),
                    }
                }
                continue;
            }
        }
        pos.push(a.clone());
    }
    if pos.is_empty() {
        usage();
    }
    let pat_raw = pos.remove(0);
    let needle = if o.fold { fold_ascii(pat_raw.as_bytes()) } else { pat_raw.into_bytes() };

    let mut any = false;
    let mut worst: i32 = 0;
    if pos.is_empty() {
        let mut buf = Vec::new();
        // read 错误 loud 不吞（C-2c 步 2 排障教训：吞错后空输入 → 真码 1
        // 与「无命中」不可分辨；应答偷取/链断类故障在这里先现形）。
        if let Err(e) = std::io::stdin().read_to_end(&mut buf) {
            eprintln!("grep: stdin: {}", e);
        }
        any = grep_buf(&buf, &needle, &o, None) > 0;
    } else {
        for f in &pos {
            let mut path = Vec::with_capacity(f.len() + 1);
            path.extend_from_slice(f.as_bytes());
            path.push(0);
            // SAFETY: path 是 NUL 结尾路径串；open 只读面。
            let fd = unsafe { open(path.as_ptr() as *const _, O_RDONLY) };
            if fd < 0 {
                eprintln!("grep: {}: not readable", f);
                worst = 2;
                continue;
            }
            let mut buf = Vec::new();
            let r = read_fd_all(fd, &mut buf);
            close(fd);
            if r.is_err() {
                // read<0 不吞（与 stdin 面 loud 纪律同档）：静默截断 → 真码
                // 1 与「无命中」同形（C-2c 步 2 教训的文件参数版）。
                eprintln!("grep: {}: read error", f);
                worst = 2;
                continue;
            }
            let n = grep_buf(&buf, &needle, &o, Some(f));
            if n > 0 && o.files_with {
                println!("{}", f);
            }
            any |= n > 0;
            if o.quiet && any {
                break;
            }
        }
    }
    // GNU exit 口径（三镜审查修正）：-q 且选中 → 0 最先（GNU 手册：-q
    // 命中时即使途中有错也 0）；否则错 2 优先；再按选中行存在（-v 反选
    // 同权）→ 0 / 无选中 → 1。旧式 `any != o.inv` 在 -v 下两态恰反转
    // （有选中退 1、零选中退 0），勿回写。
    let code = if o.quiet && any { 0 } else if worst > 0 { worst } else if any { 0 } else { 1 };
    std::process::exit(code);
}

/// 单缓冲走查：分行逐行匹配，按旗标打印。返回命中行数（-c 打印之）。
fn grep_buf(buf: &[u8], needle: &[u8], o: &Opts, name: Option<&str>) -> usize {
    use std::io::Write;
    let mut out = std::io::stdout();
    let mut hits = 0usize;
    let mut lineno = 0usize;
    let mut start = 0usize;
    for i in 0..=buf.len() {
        if i != buf.len() {
            if buf[i] != b'\n' {
                continue;
            }
        } else if start == buf.len() {
            // 缓冲以 \n 收尾 → 无残余不完整行：不造第 N+1 个幻影空行
            // （GNU 行数口径——U4 -v 钉板首跑实抓：幻影空行在 -v 下被
            // 选中打印/翻转退出码；\n 收尾文件恰 N 行，空缓冲零行）。
            break;
        }
        // GNU 不剥尾 \r（三镜审查修正：剥掉后含 \r 的 pattern 永不命中、
        // 命中行输出也少一字节——按原始行字节匹配与打印）。
        let line = &buf[start..i];
        start = i + 1;
        lineno += 1;
        let hay = if o.fold { fold_ascii(line) } else { line.to_vec() };
        let hit = find_sub(&hay, needle);
        if hit != o.inv {
            hits += 1;
            if o.quiet || o.files_with || o.count {
                continue;
            }
            let prefix = match (o.lineno, name) {
                (true, Some(f)) => format!("{}:{}:", f, lineno),
                (true, None) => format!("{}:", lineno),
                (false, Some(f)) => format!("{}:", f),
                (false, None) => String::new(),
            };
            let _ = writeln!(out, "{}{}", prefix, String::from_utf8_lossy(line));
        }
    }
    if o.count {
        let _ = writeln!(out, "{}", hits);
    }
    hits
}

/// 子串查找（needle 空恒中——GNU -F 同形）。
fn find_sub(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > hay.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

/// fd 全量读（cat 同型：read 循环至 EOF）。read<0 → Err（调用方 loud
/// 收口——不静默当 EOF 截断）。
fn read_fd_all(fd: i32, buf: &mut Vec<u8>) -> Result<(), ()> {
    let mut chunk = [0u8; 4096];
    loop {
        let n = read(fd, chunk.as_mut_ptr() as *mut _, chunk.len());
        if n == 0 {
            return Ok(());
        }
        if n < 0 {
            return Err(());
        }
        buf.extend_from_slice(&chunk[..n as usize]);
    }
}
