# BerryOS Phase 0 构建脚本
# 非交互 shell 的 PATH 不含 ~/.cargo/bin，这里固化。

export PATH := env_var("HOME") + "/.cargo/bin:" + env_var("PATH")

# 内核产物路径收敛在 工具链/paths.sh（单一事实源，smoke.sh/CI 同源）：
# target 目录可能被全局 ~/.cargo/config.toml 的 target-dir 覆盖（本机指向
# ~/.cargo-target），故动态解析不硬编码；PATH 修补也在脚本内自足——
# `export PATH :=` 只进 recipe 环境、不进属性背引号（实测 just 此行为）。
KERNEL := `bash 工具链/paths.sh kernel`
KERNEL_RELEASE := `bash 工具链/paths.sh kernel-release`

# 不用 `-bios none`：QEMU ≥11.x 把 "none" 当文件名，报
# `Could not find ROM image 'none'`。virt + -kernel ELF 直加载即可。
MACH   := "-machine virt -cpu cortex-a72 -m 128M -nographic"
QEMU   := "qemu-system-aarch64"

default: run

# G-eco 编译侧：用户程序 → flat binary（草案 D6）。builtin aarch64-unknown-none
# + 各 bin 自带链接脚本（build.rs 绝对路径注入）——**免 nightly**（custom target
# 推迟到确需 target 级差异再上，草案 D1 修订）。elf2flat.py 解 PT_LOAD 出平像
# 并补零到页（.bss 覆盖在文件侧兑现）。两遍构建（G-eco-b）：pass 1 先产
# echo/talker 平像进 用户态/userbins/，pass 2 svcmgr 才 include_bytes! 内嵌
# （单遍 workspace 无编排保证，鸡生蛋只能拆两刀）。--max 墙：子程序 4096
# （2 页映射 [0x1000,0x3000) 里首页装镜像、次页是栈——越页即吃栈）；
# svcmgr 65536 = UserProcess::build 的 D3 帽。内核侧产物 内核/src/userbins/
# *.bin gitignored：缺产物 = 内核 include_bytes 编译期炸响（loud not
# silent），故 build/check/run-release/size-release 都依赖本配方。
# CARGO_TARGET_DIR 显式钉 用户态/cargo-target：防交互环境（全局 target-dir
# 覆盖）把产物漂到内核共享 target。
build-user:
    #!/bin/sh
    cd 用户态
    CARGO_TARGET_DIR=cargo-target cargo build --release -p echo -p talker
    mkdir -p userbins ../内核/src/userbins
    python3 ../工具链/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/echo \
        userbins/echo.bin --max 4096
    python3 ../工具链/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/talker \
        userbins/talker.bin --max 4096
    CARGO_TARGET_DIR=cargo-target cargo build --release
    python3 ../工具链/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/abuser \
        ../内核/src/userbins/abuser.bin --max 4096
    python3 ../工具链/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/svcmgr \
        ../内核/src/userbins/svcmgr.bin --max 65536

build: build-user
    cargo build

check: build-user
    cargo check

# 默认机型 virt + -kernel ELF：QEMU 直加载，起始 EL1。期望输出: 1BerryOS boot
run: build
    {{QEMU}} {{MACH}} -kernel {{KERNEL}}

# virtualization=on：强制起始 EL2，测 boot.asm 的 EL2→EL1 eret 降级。期望输出: 21BerryOS boot
run-el2: build
    {{QEMU}} -machine virt,virtualization=on -cpu cortex-a72 -m 128M -nographic -kernel {{KERNEL}}

# G-eco 入机侧开发：带 rootfs 启动——fw_cfg 具名文件递送（QEMU 非 Linux ELF
# 直加载不走 Linux boot 协议：x0=0/RAM 无 DTB/-initrd 整体不加载，2026-10 实证；
# 名字 opt/berry/rootfs 与 HAL fwcfg::ROOTFS_NAME 单一事实源对读）。
# 期望输出多一行 ROOTFS ok src=fwcfg bytes=<文件大小>。rootfs.cpio 由 G-eco 编译侧产出。
run-rootfs: build
    #!/bin/sh
    test -f rootfs.cpio || { echo "缺 rootfs.cpio（G-eco 编译侧产出）"; exit 1; }
    {{QEMU}} {{MACH}} -kernel {{KERNEL}} -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio

run-release: build-user
    cargo build --release
    {{QEMU}} {{MACH}} -kernel {{KERNEL_RELEASE}}

clean:
    cargo clean

# host 三元组：.cargo/config.toml 锁了 build.target=aarch64-unknown-none，
# host 单元测试要用 `--target <host>` 覆盖（否则给 no_std 目标配 std harness）。
# 注意：`export PATH :=` 只进 recipe 环境、不进属性背引号（实测 just 此行为），
# 故这里显式找 ~/.cargo/bin/rustc，找不到再退回 PATH 里的 rustc。
host := `{ command -v "${HOME}/.cargo/bin/rustc" >/dev/null 2>&1 && "${HOME}/.cargo/bin/rustc" -vV || rustc -vV; } | sed -n 's/^host: //p'`

# host 单元测试：berry-mm 的纯 Rust buddy 核心（见 内存管理/src/buddy.rs）。
test-host:
    cargo test -p berry-mm --target {{host}}

# 内核体积基线报告（12号报告 §10.1：体积讨论从 day-one 有数据）。
# 零依赖 python3 解析 ELF：文件量 / PT_LOAD 镜像与静态内存 / alloc 段明细。
# 只报数不设门禁——阈值等基线稳定后再定（Tock 192KB 教训）。
size: build
    python3 工具链/size.py {{KERNEL}}

size-release: build-user
    cargo build --release
    python3 工具链/size.py {{KERNEL_RELEASE}}

# 无人值守冒烟（CI 同款入口）：boot 标记断言 + switches 金丝雀严格递增。
# 调试保留串口日志：SMOKE_KEEP_LOG=1 just smoke
smoke: build
    bash 工具链/smoke.sh
