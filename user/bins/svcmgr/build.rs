//! 链接脚本 + 页大小注入 + 内嵌子镜像依赖声明（G-eco-b 两遍构建的 seam：
//! pass 1 先产 `user/userbins/{echo,talker}.bin`，pass 2 才编本 crate）。
//! `rerun-if-changed` 显式列全（含两张子镜像）：任何一条都会让 cargo 的
//! 默认「包内文件变更即重跑」被替换掉，缺列 = 改子镜像不重编 svcmgr。
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg=-T{dir}/link-user.ld");
    println!("cargo:rustc-link-arg=-zmax-page-size=4096");
    println!("cargo:rerun-if-changed={dir}/link-user.ld");
    println!("cargo:rerun-if-changed={dir}/src");
    println!("cargo:rerun-if-changed={dir}/Cargo.toml");
    println!("cargo:rerun-if-changed={dir}/../../../userbins/echo.bin");
    println!("cargo:rerun-if-changed={dir}/../../../userbins/talker.bin");
    println!("cargo:rerun-if-changed={dir}/src/hexyl-plain.bin");
}
