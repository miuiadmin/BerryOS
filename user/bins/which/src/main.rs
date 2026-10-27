//! C-2c text 批自写 which：命令定位器——NAME → PATH 逐目录 stat，存在
//! 即中（v1 偏差：fs STAT 面 mode 恒 0644 无 x 位，GNU which 的可执行位
//! 判据不可表达——存在性判据在此明示，mode 面升级后补 x 位判据）。
//! PATH 未设默认 /bin（v1 rootfs 全量 bin 目录）；每中一行全路径。
//! 上游缺位：crates.io 无 uu_which 发布线（uutils which 在独立仓库）。
//! exit：全中 0 / 部分 miss 1 / usage 2（GNU 同形）。

use berry_libc::fs::{stat, Stat};

// 链接种子（stdhello 同款）。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<&String> = argv.iter().filter(|a| !a.starts_with('-')).collect();
    if names.is_empty() {
        eprintln!("usage: which NAME...");
        std::process::exit(2);
    }
    // PATH 拆分；getenv 空环境（environ = NULL 表）→ 默认 /bin。
    let path_env = std::env::var("PATH").unwrap_or_else(|_| "/bin".to_string());
    let dirs: Vec<&str> = path_env.split(':').filter(|s| !s.is_empty()).collect();
    let dirs = if dirs.is_empty() { vec!["/bin"] } else { dirs };

    let mut all_found = true;
    for name in &names {
        let mut found = false;
        for d in &dirs {
            let full = format!("{}/{}", d, name);
            if exists(&full) {
                println!("{}", full);
                found = true;
                break;
            }
        }
        if !found {
            eprintln!("which: no {} in ({})", name, path_env);
            all_found = false;
        }
    }
    std::process::exit(if all_found { 0 } else { 1 });
}

/// stat 存在性判据（mode 不看——v1 恒 0644 无 x 位，头注偏差）。
fn exists(path: &str) -> bool {
    let mut p = Vec::with_capacity(path.len() + 1);
    p.extend_from_slice(path.as_bytes());
    p.push(0);
    // SAFETY: Stat 是纯数据 repr(C) 结构（bsh :683 同型）；p NUL 结尾。
    let mut st: Stat = unsafe { core::mem::zeroed() };
    unsafe { stat(p.as_ptr() as *const _, &mut st) == 0 }
}
