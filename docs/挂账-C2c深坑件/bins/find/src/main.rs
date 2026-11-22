//! C-2c text 批自写 find：路径走查器（GNU find 语义子集——路径面 +
//! `-maxdepth N` / `-name GLOB` / `-type f|d` 三谓词 + 默认 `-print`
//! 动作；GLOB 支持 `*` `?` 两通配，字符类 `[]` 不做）。
//! 上游缺位记录：findutils 0.10.0 硬依赖 onig（C 正则库）+ chrono/nix
//! 重面——berry-ld 纯 Rust 链结构性阻断（2026-10-25 probe 实证）。
//!
//! fs 面 = berry-libc opendir/readdir/stat 直调（bsh ls 同一台机器——
//! std read_dir 的尾检 errno 语义不契约，见 bsh 头注）；d_type 恒
//! DT_UNKNOWN，判型走 STAT（st_mode S_IFMT 掩码）。
//! exit：0（走查错打 stderr 后继续——GNU 同形）；用法错 1。

use berry_libc::fs::{closedir, opendir, readdir, stat, Stat};

// 链接种子（stdhello 同款）。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

const S_IFMT: u32 = 0o170_000;
const S_IFDIR: u32 = 0o040_000;
const S_IFREG: u32 = 0o100_000;

struct Opts {
    maxdepth: Option<usize>,
    name_glob: Option<Vec<u8>>,
    ty: Option<u32>, // S_IFREG / S_IFDIR
    paths: Vec<String>,
}

fn usage() -> ! {
    eprintln!("usage: find [PATH...] [-maxdepth N] [-name GLOB] [-type f|d]");
    std::process::exit(1);
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut o = Opts { maxdepth: None, name_glob: None, ty: None, paths: Vec::new() };
    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        match a.as_str() {
            "-maxdepth" => {
                i += 1;
                if i >= argv.len() {
                    usage();
                }
                o.maxdepth = argv[i].parse().ok();
                if o.maxdepth.is_none() {
                    usage();
                }
            }
            "-name" => {
                i += 1;
                if i >= argv.len() {
                    usage();
                }
                o.name_glob = Some(argv[i].clone().into_bytes());
            }
            "-type" => {
                i += 1;
                if i >= argv.len() {
                    usage();
                }
                o.ty = match argv[i].as_str() {
                    "f" => Some(S_IFREG),
                    "d" => Some(S_IFDIR),
                    _ => usage(),
                };
            }
            "-print" => {} // 默认动作，接受且忽略
            _ => o.paths.push(a.clone()),
        }
        i += 1;
    }
    if o.paths.is_empty() {
        o.paths.push(".".to_string());
    }
    for p in &o.paths {
        walk(p, 0, &o);
    }
    std::process::exit(0);
}

/// 递归走查：先打印起点（谓词过则），再依 maxdepth 递归子目录。
/// 走查错（不存在/readdir 失败）打 stderr 继续（GNU 同形）。
fn walk(path: &str, depth: usize, o: &Opts) {
    let st = match stat_of(path) {
        Some(s) => s,
        None => {
            eprintln!("find: {}: no such file or directory", path);
            return;
        }
    };
    let ty = st.st_mode & S_IFMT;
    if pass(path, ty, o) {
        println!("{}", path);
    }
    if ty != S_IFDIR {
        return;
    }
    if let Some(md) = o.maxdepth {
        if depth >= md {
            return;
        }
    }
    let mut p = Vec::with_capacity(path.len() + 1);
    p.extend_from_slice(path.as_bytes());
    p.push(0);
    // SAFETY: p 是 NUL 结尾路径串；DIR 生命周期在本函数内 closedir。
    let d = unsafe { opendir(p.as_ptr() as *const _) };
    if d.is_null() {
        eprintln!("find: {}: not readable", path);
        return;
    }
    loop {
        // SAFETY: d 非空；readdir 返回 DIR 内嵌 Dirent 槽或 null（目录尾）。
        let ent = unsafe { readdir(d) };
        if ent.is_null() {
            break;
        }
        // SAFETY: ent 非空；d_name NUL 结尾（readdir 契约）。
        let name = unsafe {
            let c = &(*ent).d_name;
            let nul = c.iter().position(|&b| b == 0).unwrap_or(c.len());
            c[..nul].to_vec()
        };
        if name == b"." || name == b".." {
            continue;
        }
        let child = format!("{}/{}", path, String::from_utf8_lossy(&name));
        walk(&child, depth + 1, o);
    }
    // SAFETY: d 由上面的 opendir 返回，closedir 恰一次。
    unsafe { closedir(d) };
}

/// 谓词合取：-name 匹配 basename（GLOB `*`/`?`，不跨路径分隔符——
/// GNU basename 面）+ -type 匹配。
fn pass(path: &str, ty: u32, o: &Opts) -> bool {
    if let Some(want) = o.ty {
        if ty != want {
            return false;
        }
    }
    if let Some(glob) = &o.name_glob {
        let base = path.rsplit('/').next().unwrap_or(path).as_bytes().to_vec();
        if !glob_match(glob, &base) {
            return false;
        }
    }
    true
}

/// GLOB 匹配（`*` 任意串 / `?` 单字符；无转义面——语义子集）。
fn glob_match(pat: &[u8], s: &[u8]) -> bool {
    match (pat.first(), s.first()) {
        (None, None) => true,
        (None, Some(_)) => false,
        (Some(b'*'), _) => glob_match(&pat[1..], s) || (!s.is_empty() && glob_match(pat, &s[1..])),
        (Some(b'?'), Some(_)) => glob_match(&pat[1..], &s[1..]),
        (Some(&c), Some(&d)) => c == d && glob_match(&pat[1..], &s[1..]),
        (Some(_), None) => false,
    }
}

/// stat 包装：非零返回 → None（不存在/坏路径同面）。
fn stat_of(path: &str) -> Option<Stat> {
    let mut p = Vec::with_capacity(path.len() + 1);
    p.extend_from_slice(path.as_bytes());
    p.push(0);
    // SAFETY: Stat 是纯数据 repr(C) 结构（bsh :683 同型）；p NUL 结尾。
    let mut st: Stat = unsafe { core::mem::zeroed() };
    let r = unsafe { stat(p.as_ptr() as *const _, &mut st) };
    if r == 0 {
        Some(st)
    } else {
        None
    }
}
