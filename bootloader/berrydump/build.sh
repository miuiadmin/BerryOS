#!/bin/sh
# berrydump 产链（V-5a-0）——自含脚本，不进 justfile（T-1 在途不共文件；
# justfile build-boot recipe 的镜像复刻）：
#   1. cargo build --release（staticlib，target 由 .cargo/config.toml 钉）
#   2. rust-lld 静态全链接（--emit-relocs + 单 .img 链接脚本）
#   3. elf2pe（Rust 现役件）转 PE32+ → out/BERRYDUMP.EFI
# 部署 = 把 BERRYDUMP.EFI 放进 FAT 卷 \EFI\BOOT\BOOTAA64.EFI（可移动介质
# 默认路径，与 BerryBoot 同形）——见 docs/VM里程碑设计草案.md V-5a runbook。
set -e
cd "$(dirname "$0")"

# 裸 shell 下 cargo 不在 PATH（justfile 已固化 ~/.cargo/bin——独立脚本自带同款）
command -v cargo >/dev/null 2>&1 || export PATH="$HOME/.cargo/bin:$PATH"

# elf2pe 换轨（b 层 #8 三刀）：产链走 Rust 现役件（tools/elf2pe，#421 首刀）；
# 脚本 cd 到 bootloader/berrydump 后仓库根相对路径失效——bin 路径在 cd 前
# 解析并先编（elf2flat build-user / justfile build-boot 同款纪律：
# CARGO_TARGET_DIR 显式钉防漂移 + 显式 --target host 顶根 config 锁 target）。
ROOT_ABS="$(pwd)/../.."
HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
CARGO_TARGET_DIR="$ROOT_ABS/tools/elf2pe/cargo-target" \
    cargo build --release --manifest-path "$ROOT_ABS/tools/elf2pe/Cargo.toml" \
    --target "$HOST_TRIPLE"
ELF2PE="$ROOT_ABS/tools/elf2pe/cargo-target/$HOST_TRIPLE/release/elf2pe"

CARGO_TARGET_DIR=cargo-target cargo build --release

RUST_LLD="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-lld"
mkdir -p out
"$RUST_LLD" -flavor ld -static -nostdlib --emit-relocs \
    -z max-page-size=4096 -z noseparate-code \
    -T berrydump.ld -e efi_main --image-base 0 \
    -o out/berrydump.elf \
    cargo-target/aarch64-unknown-none/release/libberrydump.a

"$ELF2PE" out/berrydump.elf out/BERRYDUMP.EFI
echo "berrydump: out/BERRYDUMP.EFI ok"
