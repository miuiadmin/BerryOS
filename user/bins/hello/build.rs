//! 链接脚本 + 页大小注入（内核 build.rs 同款：绝对路径防 CWD 漂移）。
//! `-zmax-page-size=4096`：rust-lld 的 AArch64 默认 max-page 64KiB 会把
//! PT_LOAD 对齐抬到 0x10000——基址 0x1000 表达不了（elf2flat --base
//! 断言兜底）。lib rlib 天然忽略 link-arg，只本 bin 生效。
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{dir}/link-user.ld");
    println!("cargo:rustc-link-arg=-zmax-page-size=4096");
    println!("cargo:rerun-if-changed={dir}/link-user.ld");
}
