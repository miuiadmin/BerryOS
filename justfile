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
    # set -e：任一 cargo/elf2flat 失败即中止——无它时失败被吞、mkcpio 拿陈旧
    # 产物继续走（P6 A-2 实踩：bsh 漏补 mmio_vmo 字段，E0063 静默、smoke 跑
    # 的是旧 bsh——「缺产物编译期炸响」的 D6 纪律只护了内核侧，这边失守）。
    set -e
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
    # G-eco-d：std 面——stdhello（musl target + berry-libc shim + berry-ld
    # 剥 musl crt；-zmax-page-size=4096 压 PT_LOAD 间隙，G-eco-a 同款教训）。
    # d-3c：hexyl（上游 main.rs 逐字 + 生态依赖树全量——D4 路线①最终验证）。
    # P5 A-2：bsh（交互 shell——prompt/mini-readline/run；stdhello 同链）。
    ROOT=$(cd .. && pwd)
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/工具链/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/stdhello/Cargo.toml \
        --target aarch64-unknown-linux-musl
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/工具链/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/hexyl/Cargo.toml \
        --target aarch64-unknown-linux-musl
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/工具链/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/bsh/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P5 shell v0：fail（`&&` 自验的确定性 exit 42——stdhello 同链）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/工具链/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/fail/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P6 A-1：fsmgr（RAM-fs 服务——bsh `>`/cat/ls 的对端，stdhello 同链）。
    # P6 A-5：+ berry-fatfs 依赖——RAM 写通道之上叠 FAT12 只读底座（blk 端
    # 经 env 块 gift[3] 交付，`>` 命中 FAT 名先物化进 RAM）。
    # P6 A-6：物化退役——8.3 大写名走 FAT 层 staging + TRUNC 即 commit 真写
    # 扇区（簇分配/双 FAT/根目录项，rpc_write 经 vblk OP_WRITE）；小写名
    # 仍走 RAM 表（名字空间按构造隔离）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/工具链/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/fsmgr/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P6 A-2：vprobe（virtio-mmio 探测——MMIO 窗口 VMO 的用户态消费者，
    # stdhello 同链；扫 32 槽窗定位设备——QEMU 倒序建总线、首 -device 落
    # 最高槽且随 -device 顺序漂移，绑死槽号不可为）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/工具链/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/vprobe/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P6 A-3：vblk（virtio-blk 块设备驱动——真设备 IO 第一刀；vprobe 同链
    # + berry-virtio split-ring 纯函数 crate。DMA 面 = vmo_create/commit +
    # get_phys 查后备，不映射；LBA0 读 justfile/smoke 落盘印章 + LBA1 写/
    # 读回对拍自打 `VBLK ok`）。
    # P6 A-4：+ berry-blkproto 协议依赖——自验毕转常驻服务（RPC + VMO 随行
    # 数据面；svcmgr 消费自验打 `BLKD ok`）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/工具链/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/vblk/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P6 A-7：vnet（virtio-net 驱动——网络面第一刀：ARP/ICMP ping 通；
    # vprobe/vblk 同链 + berry-netproto 最小以太网/ARP/IPv4/ICMP-echo 编解码
    # 纯函数 crate。瞬态探针：ARP 解 SLIRP 网关 MAC + ICMP echo 32B 往返
    # 对拍自打 `NET ok ping=32 slot=N` 后 exit(0) + svcmgr reap）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/工具链/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/vnet/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # G-eco-c/d：rootfs.cpio（hello + stdhello + hexyl + bsh + fail + fsmgr + vprobe + vblk + vnet ELF 本体——装载走
    # svcmgr 流式 cpio/ELF 链，非 include_bytes 内嵌；产物在仓库根 =
    # `run-rootfs` 既有约定；条目按名排序 = 确定性输出）。
    python3 ../工具链/mkcpio.py ../rootfs.cpio \
        bin/bsh=cargo-target/aarch64-unknown-linux-musl/release/bsh \
        bin/fail=cargo-target/aarch64-unknown-linux-musl/release/fail \
        bin/fsmgr=cargo-target/aarch64-unknown-linux-musl/release/fsmgr \
        bin/hello=cargo-target/aarch64-unknown-none/release/hello \
        bin/hexyl=cargo-target/aarch64-unknown-linux-musl/release/hexyl \
        bin/stdhello=cargo-target/aarch64-unknown-linux-musl/release/stdhello \
        bin/vblk=cargo-target/aarch64-unknown-linux-musl/release/vblk \
        bin/vnet=cargo-target/aarch64-unknown-linux-musl/release/vnet \
        bin/vprobe=cargo-target/aarch64-unknown-linux-musl/release/vprobe

build: abi-check build-user
    cargo build

check: abi-check build-user
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
# P6 A-2：挂 virtio-blk（QEMU virt 板倒序建总线、首个 -device 落最高槽——
# vprobe 扫 32 槽窗定位；disk.img 缺席则现场造 1MiB 空盘，gitignored——该
# 兜底 A-5 起被 mkfat 每次重造取代，仅余路径常量意义）。
# force-legacy=false = 传输层报现代版 Version=2（QEMU 默认 force-legacy=true
# 报 1——vprobe 断言 ver==2，legacy v1 不收）。
# P6 A-3：每次启动前把 LBA0 印章 `BERRYOSVBLK!` 落进盘首（notrunc 只写
# 12B——vblk 读 LBA0 对拍；写 LBA1 由 vblk 自写自读，盘跨相残留无扰）。
# P6 A-4：vblk 自验毕转常驻服务——svcmgr 消费自验（经 berry-blkproto RPC
# 写/读 LBA5 对拍）随后打 `BLKD ok n=512`（rootfs 相第 14 标记）。
# P6 A-5：盘源 = mkfat.py 确定性 FAT12 卷（[16 扇区保留区][卷 @LBA16]，
# HELLO.TXT/PATTERN.BIN——fsmgr FAT 只读层的读对象，每次重造零残留；
# LBA0 印章落保留区）；bsh 自驱 fs GET 两文件对拍全过打 `FAT ok n=5000`
# （rootfs 相第 15 标记）。
# P6 A-6：`>` 对 8.3 大写名真落盘（bsh FATW 自验 RW.TXT 写读对拍打
# `FATW ok n=3000`，rootfs 相第 16 标记）；盘加 cache.writeback=off——
# SIGTERM/退出前扇区已同步落宿主盘（writeback 模式可丢页缓存）。跨
# boot 持久化验证**不可经本 recipe**：上面 mkfat 每次启动整盘重造，上
# 一 boot 的落盘必被抹——持久化验收走 `just smoke` 持久化相（不重造
# 盘；对抗审查实抓本注释曾把结构性不可达的工作流写成可用）。
# P6 A-7：挂 virtio-net（SLIRP user 网——网关 10.0.2.2 应答 ARP/回 ICMP
# echo；vnet 按设备类扫描认领 net 槽（block 槽合法同席不扰），自打
# `NET ok ping=32 slot=N` = rootfs 相第 17 标记）。
run-rootfs: build
    #!/bin/sh
    set -e
    test -f rootfs.cpio || { echo "缺 rootfs.cpio（G-eco 编译侧产出）"; exit 1; }
    python3 工具链/mkfat.py disk.img
    printf 'BERRYOSVBLK!' | dd of=disk.img conv=notrunc status=none
    {{QEMU}} {{MACH}} -kernel {{KERNEL}} -fw_cfg name=opt/berry/rootfs,file=rootfs.cpio \
        -global virtio-mmio.force-legacy=false \
        -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
        -device virtio-blk-device,drive=drv0 \
        -netdev user,id=n0 -device virtio-net-device,netdev=n0

run-release: abi-check build-user
    cargo build --release
    {{QEMU}} {{MACH}} -kernel {{KERNEL_RELEASE}}

clean:
    cargo clean

# host 三元组：.cargo/config.toml 锁了 build.target=aarch64-unknown-none，
# host 单元测试要用 `--target <host>` 覆盖（否则给 no_std 目标配 std harness）。
# 注意：`export PATH :=` 只进 recipe 环境、不进属性背引号（实测 just 此行为），
# 故这里显式找 ~/.cargo/bin/rustc，找不到再退回 PATH 里的 rustc。
host := `{ command -v "${HOME}/.cargo/bin/rustc" >/dev/null 2>&1 && "${HOME}/.cargo/bin/rustc" -vV || rustc -vV; } | sed -n 's/^host: //p'`

# host 单元测试：berry-mm 的纯 Rust buddy 核心（见 内存管理/src/buddy.rs）
# + berry-loader 的 rootfs 解析纯函数面（G-eco-c 起，见 用户态/berry-loader）
# + berry-fsproto 的 fs 协议编解码纯函数面（P6 A-1 起，见 用户态/berry-fsproto）
# + berry-virtio 的 split-ring 布局/编解码纯函数面（P6 A-3 起，见 用户态/berry-virtio）
# + berry-blkproto 的块设备 RPC 协议编解码纯函数面（P6 A-4 起，见 用户态/berry-blkproto）
# + berry-fatfs 的 FAT12 BPB/目录项/簇链解析纯函数面（P6 A-5 起，见 用户态/berry-fatfs；
#   A-6 起含写基元——fat12_set/find_free_cluster/encode_file_entry，fixture
#   共识基准同时钉住读面与写面编码一致性）
# + berry-netproto 的最小以太网/ARP/IPv4/ICMP-echo 编解码纯函数面（P6 A-7
#   起，见 用户态/berry-netproto——vnet 驱动的协议面）。
# fixture 守卫在 berry-fatfs 测试前置：mkfat --check-fixture 重产比对——
# 改 mkfat 忘重产 --fixture 时共识测试对陈旧基准假绿（A-5 对抗审查实抓）。
test-host:
    cargo test -p berry-mm --target {{host}}
    cargo test --manifest-path 用户态/Cargo.toml -p berry-loader --target {{host}}
    cargo test --manifest-path 用户态/Cargo.toml -p berry-fsproto --target {{host}}
    cargo test --manifest-path 用户态/Cargo.toml -p berry-virtio --target {{host}}
    cargo test --manifest-path 用户态/Cargo.toml -p berry-blkproto --target {{host}}
    python3 工具链/mkfat.py --check-fixture 用户态/berry-fatfs/tests/volume_fixture.bin
    cargo test --manifest-path 用户态/Cargo.toml -p berry-fatfs --target {{host}}
    cargo test --manifest-path 用户态/Cargo.toml -p berry-netproto --target {{host}}

# 内核体积基线报告（12号报告 §10.1：体积讨论从 day-one 有数据）。
# 零依赖 python3 解析 ELF：文件量 / PT_LOAD 镜像与静态内存 / alloc 段明细。
# 只报数不设门禁——阈值等基线稳定后再定（Tock 192KB 教训）。
size: build
    python3 工具链/size.py {{KERNEL}}

size-release: abi-check build-user
    cargo build --release
    python3 工具链/size.py {{KERNEL_RELEASE}}

# ABI 单源工具链（`工具链/abi.toml` = 常量层唯一事实源）：
#   abi-gen    —— 再生三面产物（内核 abi_gen.rs / berry-sys abi_gen.rs / 文档
#                marker 块）；改 ABI 后跑它，产物 diff 随源一起入册。
#   abi-check  —— 逐字节比对守卫（挂 build/check/smoke 前置）：忘再生成 /
#                手改产物即 FAIL。Plan B v0.5 的 abi_check.py 三方对账随之退役
#                （生成即一致，无需对账）。需 Python ≥3.11（tomllib）——
#                CLT 自带 /usr/bin/python3(3.9) 不够，brew python3 可。
abi-gen:
    python3 工具链/gen_abi.py

abi-check:
    python3 工具链/gen_abi.py --check

# 无人值守冒烟（CI 同款入口）：boot 标记断言 + switches 金丝雀严格递增。
# 调试保留串口日志：SMOKE_KEEP_LOG=1 just smoke
smoke: build
    bash 工具链/smoke.sh

# ---------- P7 x86_64 最小集（ADR #41：范围盒子 = 串口/定时器/异常向量/
# 上下文切换/SVC 入口；SMP/APIC/ACPI/PCIe 不在盒内）----------
# 独立小工作区（同 用户态/ 模式——根 workspace 不含本目录）。构建必须显式
# --target：根 .cargo/config.toml 锁了 build.target=aarch64-unknown-none，
# config 发现按 CWD 向上走、跨 workspace 边界照样命中（CLI --target 才能盖）。
X86_DIR := "架构相关/x86_64"
X86_TARGET := "x86_64-unknown-none"
X86_KERNEL := X86_DIR / "cargo-target" / X86_TARGET / "release" / "berry-x86"

# x86 内核构建（PVH 引导 ELF64；缺 x86_64-unknown-none target 先
# `rustup target add x86_64-unknown-none`——stable 预编译 core，免 nightly）
build-x86:
    #!/bin/sh
    set -e
    cd {{X86_DIR}}
    CARGO_TARGET_DIR=cargo-target cargo build --release --target {{X86_TARGET}}

# x86 QEMU 启动（pc 机 + PVH ELF；-no-reboot：早期三重故障直接退出，不刷
# SeaBIOS 重启循环污染日志）。期望：BerryOS boot (x86_64) → PVH ok →
# TIMER ok → X86 ok（P7-x1 起定时器链 100Hz 真中断 100 拍）。
# 退出同 aarch64：Ctrl-A X。
run-x86: build-x86
    qemu-system-x86_64 -m 128M -nographic -no-reboot -kernel {{X86_KERNEL}}

# x86 独立冒烟（4 标记恰一次；五原语收官前不并入主 smoke——ARM64 四跑零
# 扰动；收官时随最小集 gate 一并挂进 CI 面详见 smoke-x86.sh 头注）。
# 内核路径经 X86_KERNEL 传参（单一事实源——对抗审查实抓 smoke-x86.sh
# 曾自带第二份硬编码：cargo-target 是 gitignored 本地长存目录，后续切片
# 改名/挪 bin 时只改 justfile 一侧会让冒烟继续跑旧 ELF 假绿，G-eco-b
# 「陈旧产物静默上线」同族）。
smoke-x86: build-x86
    bash 工具链/smoke-x86.sh "{{X86_KERNEL}}"
