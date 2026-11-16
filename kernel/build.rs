//! 注入链接脚本（绝对路径，避免 rustflags 里 `-T相对路径` 随 cwd 漂移、lib rlib 忽略 link-arg）。
fn main() {
    let ld = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tools")
        .join("linker-aarch64.ld");
    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rerun-if-changed={}", ld.display());
    // VER 硬化（沿革一八五实弹）：ident 新鲜度——.git/HEAD（检出态变化）
    // + HEAD 指向的分支 ref 文件（新提交变化）双触发。否则只有链接脚本的
    // rerun-if-changed 在册 = BERRY_GIT_IDENT 冻结首跑缓存，增量机新提交
    // 后 ident 陈旧 → svcmgr 三元组对拍 VER mix 假红（哨兵二次咬真异常
    // 实证形态）。无 git 环境（源码快照）两路径不存在 = 静默无触发，与
    // nogit 降级链自洽。路径相对 kernel/（build.rs CWD = manifest 目录）。
    println!("cargo:rerun-if-changed=../.git/HEAD");
    if let Ok(head) = std::fs::read_to_string("../.git/HEAD") {
        if let Some(r) = head.trim().strip_prefix("ref: ") {
            println!("cargo:rerun-if-changed=../.git/{}", r);
        }
    }
    println!("cargo:rustc-env=BERRY_GIT_IDENT={}", git_ident());
    let rustc_ver = std::process::Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    // 取形如 "rustc 1.9x.y (hash date)" 的版本号段（剥前缀词与尾缀）。
    let short = rustc_ver
        .split_whitespace()
        .nth(1)
        .unwrap_or("?")
        .to_string();
    println!("cargo:rustc-env=BERRY_RUSTC_VER={}", short);
}

/// VER-01 第一档：构建标识注入——git short-hash + dirty 位经 cargo:rustc-env
/// 编译期烙进 boot banner（崩溃归因/镜像对账的可查询版本面）。rerun 不挂
/// git 状态（每次构建重查成本低、且 dirty 位变化本就该重编 banner——挂
/// CARGO_MANIFEST_DIR 已覆盖大多数触发，hash 漂移由 clean build 兜底）。
fn git_ident() -> String {
    // 构建版本（VER 标准 §1 构建标识轨）：`git describe --tags` 形态
    // `v0.2.0-5-ga8f22bc` = 版本 tag 后第 N 提交 + 哈希——单调可排序
    // （N）+ 锚定版本 + 唯一身份，纯 git 派生零中央计数器（多会话共笔
    // 仓里 build number 数据库必漂——沿革 174/177 撞号实录为证）。
    let describe = std::process::Command::new("git")
        .args(["describe", "--tags", "--long"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);
    // describe 失败（无 tag 或无 git）回退短哈希；全失败 nogit 诚实降级。
    let base = if describe.is_empty() {
        std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    } else {
        describe
    };
    if base.is_empty() {
        return "nogit".to_string();
    }
    if dirty {
        format!("{}-dirty", base)
    } else {
        base
    }
}
