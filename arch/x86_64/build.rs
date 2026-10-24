// 链接脚本经绝对路径注入（用户态 bins 同款：`-C link-arg` 相对路径随
// CWD 漂移，绝对路径才稳）；rerun-if 全列 = link-x86.ld + 自身源。
// 注意：这里没有也不该有 -zmax-page-size=4096——那是 aarch64 lld 把
// PT_LOAD 对齐抬到 64K 的坑；x86-64 lld 默认 max-page-size 就是 4096。
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{dir}/link-x86.ld");
    println!("cargo:rerun-if-changed={dir}/link-x86.ld");
    println!("cargo:rerun-if-changed={dir}/src/main.rs");
}
