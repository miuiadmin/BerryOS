//! 注入链接脚本（绝对路径，避免 rustflags 里 `-T相对路径` 随 cwd 漂移、lib rlib 忽略 link-arg）。
fn main() {
    let ld = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tools")
        .join("linker-aarch64.ld");
    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rerun-if-changed={}", ld.display());
}
