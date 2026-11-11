//! 链接脚本 + 页大小注入（hello 同款：绝对路径防 CWD 漂移）。
//! `-zmax-page-size=4096`：rust-lld 的 AArch64 默认 max-page 64KiB 会把
//! PT_LOAD 对齐抬到 0x10000——基址 0x1000 表达不了（G-eco-a 判例）。
//! **仅 none target 发射**：host 单测构建（`cargo test --target <host>`）
//! 走 clang——`-zmax-page-size` 是 lld 专用旗，混入即链接失败（ld.lld
//! flavor 判别不可靠，按 TARGET 门控最简）。
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    if std::env::var("TARGET").as_deref() == Ok("aarch64-unknown-none") {
        println!("cargo:rustc-link-arg=-T{dir}/link-user.ld");
        println!("cargo:rustc-link-arg=-zmax-page-size=4096");
    }
    println!("cargo:rerun-if-changed={dir}/link-user.ld");
}
