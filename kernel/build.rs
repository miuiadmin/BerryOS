//! 注入链接脚本（绝对路径，避免 rustflags 里 `-T相对路径` 随 cwd 漂移、lib rlib 忽略 link-arg）。
fn main() {
    let ld = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tools")
        .join("linker-aarch64.ld");
    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rerun-if-changed={}", ld.display());
    println!("cargo:rustc-env=BERRY_GIT_IDENT={}", git_ident());
}

/// VER-01 第一档：构建标识注入——git short-hash + dirty 位经 cargo:rustc-env
/// 编译期烙进 boot banner（崩溃归因/镜像对账的可查询版本面）。rerun 不挂
/// git 状态（每次构建重查成本低、且 dirty 位变化本就该重编 banner——挂
/// CARGO_MANIFEST_DIR 已覆盖大多数触发，hash 漂移由 clean build 兜底）。
fn git_ident() -> String {
    let short = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);
    if short.is_empty() {
        // 无 git 环境（例如源码快照构建）：标识退化为 nogit——诚实降级不编造。
        return "nogit".to_string();
    }
    if dirty {
        format!("{}-dirty", short)
    } else {
        short
    }
}
