# BerryOS Phase 0 构建脚本
# 非交互 shell 的 PATH 不含 ~/.cargo/bin，这里固化。

export PATH := env_var("HOME") + "/.cargo/bin:" + env_var("PATH")

# 内核产物路径收敛在 tools/paths.sh（单一事实源，smoke.sh/CI 同源）：
# target 目录可能被全局 ~/.cargo/config.toml 的 target-dir 覆盖（本机指向
# ~/.cargo-target），故动态解析不硬编码；PATH 修补也在脚本内自足——
# `export PATH :=` 只进 recipe 环境、不进属性背引号（实测 just 此行为）。
KERNEL := `bash tools/paths.sh kernel`
KERNEL_RELEASE := `bash tools/paths.sh kernel-release`

# 不用 `-bios none`：QEMU ≥11.x 把 "none" 当文件名，报
# `Could not find ROM image 'none'`。virt + -kernel ELF 直加载即可。
# 不带 `-smp`（QEMU 默认 1 核）是 v1 有意定值：免锁根基 = 单核串行
# （SMP设计草案 §1.1）；S-1 起双核只进 smoke-smp 线（`-smp 2`）。
MACH   := "-machine virt -cpu cortex-a72 -m 128M -nographic"
QEMU   := "qemu-system-aarch64"

default: run

# G-eco 编译侧：用户程序 → flat binary（草案 D6）。builtin aarch64-unknown-none
# + 各 bin 自带链接脚本（build.rs 绝对路径注入）——**免 nightly**（custom target
# 推迟到确需 target 级差异再上，草案 D1 修订）。elf2flat.py 解 PT_LOAD 出平像
# 并补零到页（.bss 覆盖在文件侧兑现）。两遍构建（G-eco-b）：pass 1 先产
# echo/talker/thrchild 平像进 user/userbins/，pass 2 svcmgr 才 include_bytes!
# 内嵌（单遍 workspace 无编排保证，鸡生蛋只能拆两刀）。--max 墙：子程序
# 4096（2 页映射 [0x1000,0x3000) 里首页装镜像、次页是栈——越页即吃栈）；
# svcmgr 131072 = UserProcess::build 的 D3 帽（A7-4 段② 64→128KiB 同步抬
# ——libc 信号面并入后平像 69.6KB 超 64KiB 旧帽）；T-1 thrchild 16384（4 页墙
# ——t2 栈 2048B 内嵌 .bss 与代码共页，平像恰 1 页；镜像最大 4 页时 svcmgr
# 映射窗 [0x1000,0x7000) 的主栈区仍余 2 页）。内核侧产物 kernel/src/userbins/
# *.bin gitignored：缺产物 = 内核 include_bytes 编译期炸响（loud not
# silent），故 build/check/run-release/size-release 都依赖本配方。
# CARGO_TARGET_DIR 显式钉 user/cargo-target：防交互环境（全局 target-dir
# 覆盖）把产物漂到内核共享 target。
build-user:
    #!/bin/sh
    # set -e：任一 cargo/elf2flat 失败即中止——无它时失败被吞、mkcpio 拿陈旧
    # 产物继续走（P6 A-2 实踩：bsh 漏补 mmio_vmo 字段，E0063 静默、smoke 跑
    # 的是旧 bsh——「缺产物编译期炸响」的 D6 纪律只护了内核侧，这边失守）。
    set -e
    # GUI-0（2026-10-29）：user 侧 none-target 家族（echo/talker/thrchild/svcmgr/
    # abuser/hello）**保留 NEON codegen**——EL0 任务 FP 态已由 TaskContext 全量保存
    # 兜底、GUI 光栅后续正要用 SIMD；根 .cargo/config.toml 的 -neon 旗只属内核
    # SIMD-free 合同。cwd=user/ 会向上吃到根 config，env RUSTFLAGS 优先于 config
    # → 显式清空还原用户仓 codegen（零漂移；musl 面本就无旗）。
    export RUSTFLAGS=""
    cd user
    CARGO_TARGET_DIR=cargo-target cargo build --release -p echo -p talker -p thrchild
    mkdir -p userbins ../kernel/src/userbins
    python3 ../tools/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/echo \
        userbins/echo.bin --max 4096
    python3 ../tools/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/talker \
        userbins/talker.bin --max 4096
    # T-1 §2：thrchild（进程内双线程自验子——bootstrap 自句柄经 process_start
    # x0 递入；pass 1 平像 + svcmgr include_bytes 内嵌，零 rootfs 新条目）。
    python3 ../tools/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/thrchild \
        userbins/thrchild.bin --max 16384
    CARGO_TARGET_DIR=cargo-target cargo build --release
    python3 ../tools/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/abuser \
        ../kernel/src/userbins/abuser.bin --max 4096
    python3 ../tools/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/svcmgr \
        ../kernel/src/userbins/svcmgr.bin --max 131072
    # G-eco-d：std 面——stdhello（musl target + berry-libc shim + berry-ld
    # 剥 musl crt；-zmax-page-size=4096 压 PT_LOAD 间隙，G-eco-a 同款教训）。
    # d-3c：hexyl（上游 main.rs 逐字 + 生态依赖树全量——D4 路线①最终验证）。
    # P5 A-2：bsh（交互 shell——prompt/mini-readline/run；stdhello 同链）。
    ROOT=$(cd .. && pwd)
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/stdhello/Cargo.toml \
        --target aarch64-unknown-linux-musl
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/hexyl/Cargo.toml \
        --target aarch64-unknown-linux-musl
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/bsh/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P5 shell v0：fail（`&&` 自验的确定性 exit 42——stdhello 同链）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/fail/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # B-1（docs/基准与压力测试设计.md §8）：bench1——EL0 用户态四项微基准
    # （sys_null/sys_vmo_rw4k/ipc_enqueue8/evt_signal；stdhello 同链 std 面，
    # vmo/channel/event 全 syscall 自造 = 零新管道）；bsh bench1_selftest
    # spawn + 私有通道收齐格式断言后打 `B1 ok n=4`（rootfs 相第 28 标记）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/bench1/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P6 A-1：fsmgr（RAM-fs 服务——bsh `>`/cat/ls 的对端，stdhello 同链）。
    # P6 A-5：+ berry-fatfs 依赖——RAM 写通道之上叠 FAT12 只读底座（blk 端
    # 经 env 块 gift[3] 交付，`>` 命中 FAT 名先物化进 RAM）。
    # P6 A-6：物化退役——8.3 大写名走 FAT 层 staging + TRUNC 即 commit 真写
    # 扇区（簇分配/双 FAT/根目录项，rpc_write 经 vblk OP_WRITE）；小写名
    # 仍走 RAM 表（名字空间按构造隔离）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/fsmgr/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P6 A-2：vprobe（virtio-mmio 探测——MMIO 窗口 VMO 的用户态消费者，
    # stdhello 同链；扫 32 槽窗定位设备——QEMU 倒序建总线、首 -device 落
    # 最高槽且随 -device 顺序漂移，绑死槽号不可为）。
    # 2026-10-24：驱动一族（vprobe/vblk/vnet + berry-virtio）自 user/ 迁入
    # drivers/ 独立工作区（顶层目录提前启用）；musl + berry-ld 链不变，
    # 产物出 drivers/cargo-target（family 单锁 = drivers/Cargo.lock）。
    CARGO_TARGET_DIR=../drivers/cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path ../drivers/bins/vprobe/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P6 A-3：vblk（virtio-blk 块设备驱动——真设备 IO 第一刀；vprobe 同链
    # + berry-virtio split-ring 纯函数 crate。DMA 面 = vmo_create/commit +
    # get_phys 查后备，不映射；LBA0 读 justfile/smoke 落盘印章 + LBA1 写/
    # 读回对拍自打 `VBLK ok`）。
    # P6 A-4：+ berry-blkproto 协议依赖——自验毕转常驻服务（RPC + VMO 随行
    # 数据面；svcmgr 消费自验打 `BLKD ok`）。
    CARGO_TARGET_DIR=../drivers/cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path ../drivers/bins/vblk/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # P6 A-7：vnet（virtio-net 驱动——网络面第一刀：ARP/ICMP ping 通；
    # vprobe/vblk 同链 + berry-netproto 最小以太网/ARP/IPv4/ICMP-echo 编解码
    # 纯函数 crate。自验：ARP 解 SLIRP 网关 MAC + ICMP echo 32B 往返对拍
    # 自打 `NET ok ping=32 slot=N`。
    # P6 A-8：+ berry-netdproto 协议依赖——自验毕转常驻服务（DNS_QUERY
    # 严格串行 RPC；svcmgr 消费自验打 `DNS ok addr=`；不再退出，不 reap）。
    CARGO_TARGET_DIR=../drivers/cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path ../drivers/bins/vnet/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # GUI-4 段②：gui-inputd（virtio-tablet 输入服务——段①已落 bins/，本刀
    # 入编译链；musl + berry-libc shim + berry-ld 同链）。
    CARGO_TARGET_DIR=../drivers/cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path ../drivers/bins/gui-inputd/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # app-0（ADR #43/#44）：demo 双 app（appdemo console=true / appquiet
    # console=false——能力授予/缺席两半活证）musl 同链 → staging 组装
    # {manifest.toml, bin/<名>} → mkapp 门禁五道产 .bap（cpio(newc) 包 =
    # manifest + 唯一 bin/ 条目）。staging 与产物都在 userbins/（gitignored）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/appdemo/Cargo.toml \
        --target aarch64-unknown-linux-musl
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/appquiet/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # C-2a：uutils coreutils 批（VM 草案 §C-2a）——家族 workspace bins/uu/
    # 单次构建 12 命令（=0.12.0 锁步，MIT，上游逐字不动；薄 harness =
    # CRT0 锚 + uumain 直传——`#[uucore::main]` proc 宏已统一改写 -> i32）。
    # berry-libc 补桩批见 stubs.rs C-2a 段：pow 纯算术实现（indicatif ETA
    # 经 LTO 真可达）+ mknod ENOSYS + sigfillset ok——lto=fat 把 berry-libc
    # 折进 bin 自身 rcgu.o（rlib 不上链接线），-lc 保险网拉 musl 成员会与
    # 折入定义 duplicate，缺口在 libc shim 侧补齐 = 零 musl 字节哲学执法点
    # （详见 tools/berry-ld 头注）。
    # C-2c（中批）：head/wc/cut/tr/sort/uniq 六件（=0.12.0 锁步，uu 家族
    # 18 成员）；补桩批见 stubs.rs C-2c 段。深坑件 tail/env/date/grep/
    # find/which 挂账专项另刀（grep/find 供源 onig 互斥实证不通）。
    # C-2 刀 2：+df/du（=0.12.0 同锁步，uu 家族 20 成员）——df 消费
    # berry-libc statfs 真值桩（OP_STATFS RPC）+ /etc/mtab（fsmgr 启动
    # 预建，挂载表回落件）；du 零新 libc 面（stat/opendir/readdir 全
    # C-1 已有）。bsh df_selftest 验收（`DF ok`）。
    # C-2c 挂账刀：+tail/env/date（=0.12.0 同锁步，uu 家族 23 成员；
    # 源码档案 docs/挂账-C2c深坑件/uu/ 归位）。补桩面：tail = epoll/
    # eventfd/inotify 七枚 ENOSYS（-f 尾随链 v1 无实现，运行期不可达）；
    # env = sigprocmask 手写桩 + sigrtmin/max 定值（34/64）；date =
    # clock_getres 谎报 1ns 成功（上游 unwrap 档失败即 panic）+
    # clock_settime EPERM。bsh u4_selftest 验收（`U4 ok`）。
    CARGO_TARGET_DIR=bins/uu/cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/uu/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # C-2c 挂账刀：自写三件 grep/find/which（uu_* 供源硬依赖 onig C 正则
    # 库无 feature 门可关——互斥实证不通，自写语义子集替代；stdhello
    # 同链 std bin 独立小工作区，在 user/bins/{grep,find,which} 不入
    # uu 家族 workspace）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/grep/Cargo.toml \
        --target aarch64-unknown-linux-musl
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/find/Cargo.toml \
        --target aarch64-unknown-linux-musl
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/which/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # C-3 段①：ps（ps 查询 CLI——svcmgr 拉起账本的渲染面；stdhello 同链
    # std bin 独立小工作区，gift[2] 持查询端 ps_b + psproto PS_LIST 单 op
    # 严格串行 RPC；svcmgr ps_selftest 消费自验打 `PS ok n=`）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/ps/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # C-3 段②：berryctl（ver/ps/mem 三子命令控制 CLI 雏形——ps 同链 +
    # psproto 三 op 全面；svcmgr bc_selftest 消费自验打 `BC ok`）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/berryctl/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # GUI-1：guifbd（framebuffer 显示服务——boot 授能 fb MMIO VMO 映
    # FB_VA + 几何 info VMO 随 env extras；自验三步全过自打 `FBW ok`，
    # EFI-GOP 路专属（smoke-image ECO 标记）；段②起 serve 循环收 console
    # fan-out 上屏）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/guifbd/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # A5：netprobe（std::net guest 对拍件——bsh net_gifts 三元 spawn 面
    # 的消费者；TcpStream::connect 域名/IP 双路 + 回显逐字节对拍。产链
    # 同 stdhello 独立小工作区 musl 链；自验/标记 `NETS ok` 随 A5-2
    # nets_selftest）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/netprobe/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # A7-4 段④：sigchild（信号终止靶——bsh sig_selftest 步④ spawn 后
    # berry_signal_send(hp, SIGTERM) 置位；200ms 睡循环醒来 = libc
    # clock_nanosleep 链尾投递点 → DFL → io::exit(143) EOF 先行。零显式
    # 信号代码即靶面；自验/标记 `SIG ok n=5` 随 sig_selftest）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/sigchild/Cargo.toml \
        --target aarch64-unknown-linux-musl
    rm -rf userbins/app-stage userbins/app
    mkdir -p userbins/app-stage/APPDEMO/bin userbins/app-stage/APPQUIET/bin userbins/app
    cp bins/appdemo/manifest.toml userbins/app-stage/APPDEMO/
    cp cargo-target/aarch64-unknown-linux-musl/release/appdemo userbins/app-stage/APPDEMO/bin/
    cp bins/appquiet/manifest.toml userbins/app-stage/APPQUIET/
    cp cargo-target/aarch64-unknown-linux-musl/release/appquiet userbins/app-stage/APPQUIET/bin/
    python3 ../tools/mkapp.py userbins/app/APPDEMO.bap userbins/app-stage/APPDEMO
    python3 ../tools/mkapp.py userbins/app/APPQUIET.bap userbins/app-stage/APPQUIET
    # G-eco-c/d + app-0：rootfs.cpio（hello + stdhello + hexyl + bsh + fail + bench1 + fsmgr + vprobe + vblk + vnet ELF 本体——装载走
    # svcmgr 流式 cpio/ELF 链，非 include_bytes 内嵌；产物在仓库根 =
    # `run-rootfs` 既有约定；条目按名排序 = 确定性输出。app-0 起加
    # app/<ID>.bap 整包条目（'a' < 'b' 排序在 bin/ 前；a-1 阶段 svcmgr
    # 主走查对未知名算术跳过 = 零消费、零行为变化——appd 消费随 a-2）。
    python3 ../tools/mkcpio.py ../rootfs.cpio \
        app/APPDEMO.bap=userbins/app/APPDEMO.bap \
        app/APPQUIET.bap=userbins/app/APPQUIET.bap \
        bin/bench1=cargo-target/aarch64-unknown-linux-musl/release/bench1 \
        bin/berryctl=cargo-target/aarch64-unknown-linux-musl/release/berryctl \
        bin/bsh=cargo-target/aarch64-unknown-linux-musl/release/bsh \
        bin/cat=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/cat \
        bin/cp=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/cp \
        bin/cut=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/cut \
        bin/date=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/date \
        bin/df=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/df \
        bin/du=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/du \
        bin/echo=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/echo \
        bin/env=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/env \
        bin/fail=cargo-target/aarch64-unknown-linux-musl/release/fail \
        bin/false=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/false \
        bin/find=cargo-target/aarch64-unknown-linux-musl/release/find \
        bin/fsmgr=cargo-target/aarch64-unknown-linux-musl/release/fsmgr \
        bin/grep=cargo-target/aarch64-unknown-linux-musl/release/grep \
        bin/gui-inputd=../drivers/cargo-target/aarch64-unknown-linux-musl/release/gui-inputd \
        bin/guifbd=cargo-target/aarch64-unknown-linux-musl/release/guifbd \
        bin/hello=cargo-target/aarch64-unknown-none/release/hello \
        bin/head=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/head \
        bin/hexyl=cargo-target/aarch64-unknown-linux-musl/release/hexyl \
        bin/ls=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/ls \
        bin/mkdir=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/mkdir \
        bin/mv=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/mv \
        bin/netprobe=cargo-target/aarch64-unknown-linux-musl/release/netprobe \
        bin/ps=cargo-target/aarch64-unknown-linux-musl/release/ps \
        bin/pwd=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/pwd \
        bin/rm=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/rm \
        bin/rmdir=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/rmdir \
        bin/sigchild=cargo-target/aarch64-unknown-linux-musl/release/sigchild \
        bin/sort=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/sort \
        bin/stdhello=cargo-target/aarch64-unknown-linux-musl/release/stdhello \
        bin/tail=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/tail \
        bin/touch=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/touch \
        bin/tr=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/tr \
        bin/true=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/true \
        bin/uniq=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/uniq \
        bin/vblk=../drivers/cargo-target/aarch64-unknown-linux-musl/release/vblk \
        bin/vnet=../drivers/cargo-target/aarch64-unknown-linux-musl/release/vnet \
        bin/vprobe=../drivers/cargo-target/aarch64-unknown-linux-musl/release/vprobe \
        bin/wc=bins/uu/cargo-target/aarch64-unknown-linux-musl/release/wc \
        bin/which=cargo-target/aarch64-unknown-linux-musl/release/which \
        --split 32
    # A1-3b（余量刀）：mkcpio --split 32 = 主路恒走多卷产链——≈28MiB
    # rootfs = 1 卷 rootfs0.cpio（单卷 ≤32MiB 与内核 fwcfg 单卷帽同口径，
    # 容量工程前置-A1 §3.1）；越帽自动分卷、消费端（内核连续前缀发现序
    # opt/berry/rootfs0..9 + VMO 逐页拼装）零改动。run-rootfs/smoke 按
    # rootfs0 在席性动态取卷名；rootfs.cpio 单档照常产出（既有消费面
    # 零扰动）。陈卷（旧构建多卷残留）由 mkcpio 写卷前自清。
    # T-1 §1：TLS lint FAIL 门（挂 mkcpio 后——smoke/check/run 全经
    # build-user = CI 自然被护）。F1 放置可行性 = crt0::tls_place 的
    # python 镜像（FAIL ⟺ 运行期 brk #0x7C1 会炸，无任意阈值）；F2
    # .tdata 自指针（拷贝式迁移后 = 悬垂直址）。CAP/算术 python↔rust
    # 双份同源义务，见 tools/tlsscan.py 头注。
    python3 ../tools/tlsscan.py --check ../rootfs.cpio

build: abi-check build-user
    cargo build

# K-2 release 链接守卫：release 优化（fat LTO + O3）才生成的引用形态（.text.boot
# ADRP 引 .rodata.cst16 越 ±4GB）可断链而 debug 全绿——只构建不跑 QEMU，挂 CI。
# K-3：构建后尾挂 size.py --assert-flip——release 面 .text.boot 全部 ADRP 目标
# 必低半（bit63=0）断言（debug 面经 smoke.sh 既有调用零改动生效）。
build-release: abi-check build-user
    cargo build --release
    python3 tools/size.py --assert-flip {{KERNEL_RELEASE}}

check: abi-check build-user
    cargo check

# 默认机型 virt + -kernel ELF：QEMU 直加载，起始 EL1。期望输出: 1BerryOS boot
run: build
    {{QEMU}} {{MACH}} -kernel {{KERNEL}}

# virtualization=on：强制起始 EL2，测 boot.asm 的 EL2→EL1 eret 降级。期望输出: 21BerryOS boot
run-el2: build
    {{QEMU}} -machine virt,virtualization=on -cpu cortex-a72 -m 128M -nographic -kernel {{KERNEL}}

# S-1 副核 bring-up 手跑（`SMP设计草案.md` §3/§4）：默认机型 + -smp 2——
# PSCI CPU_ON 拉副核走 §3.2 十项 → 心跳握手 → `SMP ok ncores=2`。
# **-smp 只进本线**（run-smp/smoke-smp）：主 run/run-el2/run-rootfs/smoke
# 一律单核——单核零漂移硬线（S-1 盒子边界）。默认机型（boot EL1）才有
# HVC→PSCI 拦截；virtualization=on 下内核 BOOT_EL 门先行跳过（conduit 分型）。
run-smp: build
    {{QEMU}} {{MACH}} -smp 2 -kernel {{KERNEL}}

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
# P6 A-8：vnet 自验毕转常驻 DNS 服务（berry-netdproto DNS_QUERY 严格串行
# RPC，不退出不 reap）——svcmgr 消费自验（"localhost" → 127.0.0.1 对拍，
# SLIRP 虚拟 DNS 10.0.2.3 离线确定）打 `DNS ok addr=127.0.0.1` =
# rootfs 相第 18 标记。
# T-c：NET ok 后 vnet 跑 TCP 自验会话（berry-tcp 状态机 + tcp.rs IO——
# guest 主动 connect 10.0.2.2:18080 = SLIRP 转发宿主 echod，握手/64B 回显
# 对拍/主动 FIN 挥手到 Closed）打 `TCP ok echo=64` = rootfs 相第 19 标记。
# T-d：vnet 常驻服务扩 TCP 三 op（netdproto OPEN/ROUNDTRIP/CLOSE，单会话
# 独占 ST_BUSY）——svcmgr 作 client 三 op 一轮回显（同一 echod）全 ST_OK
# 才打 `TCPS ok echo=64` = rootfs 相第 20 标记（「栈本身能跑」与「栈经
# 服务化 RPC 面能被消费」两半，§9 话术闭环）。
# A2-2 段③：TCP ok 后 vnet 跑服务端半边自验（TcpMux 会话表 + LISTEN/
# ACCEPT/POLL op + hostfwd 环回：guest 主动 connect 10.0.2.2:18081 →
# SLIRP host-alias 转 hostfwd listener → 转回 guest:18081 = 自连自回，
# 七步链全过才打 `LSN ok n=7` = rootfs 相新增标记；netdev 挂
# `hostfwd=tcp::18081-:18081`（无盘路/裸相不带 netdev 不受扰）。
# A2-3：POLL ok 后 bsh 跑事件循环 echo server 自验（netdproto LISTEN/
# ACCEPT/POLL/DRAIN/RT/CLOSE 会话面，hostfwd `tcp::18082-:18082` 专线
# 外接宿主）——两连接 64B/24B 异型载荷逐字节对拍全过才打 `EVS ok n=2`
# = rootfs 相第 39 标记；evsclient（宿主对拍 client）由本 recipe 起
#（QEMU 前）/收（trap EXIT），缺席则 bsh 卡 accept-wait 120s 后 bail
#（echod 必抄同款：对端在席是 guest 自验链的结构前提）。
# echod 由本 recipe 起（QEMU 前）/收（trap EXIT）；smoke.sh 同款自足。
# GUI-4 段②：+virtio-tablet（id=tab0——QEMU virtio-mmio 总线**倒序建**、
# 首 -device 落最高槽，tablet 追加在 blk/net 之后取更低槽 = 既有设备槽位
# 零漂移；gui-inputd 按设备类扫 input 槽恰一认领）。**手跑无 QMP**：
# INPUT ok 照打（boot 自验不依赖注入）；INPUTP 只属 smoke RX 注入相。
run-rootfs: build
    #!/bin/sh
    set -e
    test -f rootfs.cpio || { echo "缺 rootfs.cpio（G-eco 编译侧产出）"; exit 1; }
    python3 tools/mkfat.py disk.img
    printf 'BERRYOSVBLK!' | dd of=disk.img conv=notrunc status=none
    # T-c：宿主 TCP 回显服务（vnet TCP 自验对端；QEMU 退出后 trap 统一收）。
    ECHO_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-echod.XXXXXX")
    python3 tools/echod.py > "$ECHO_LOG" 2>&1 &
    ECHO_PID=$!
    # A2-3：EVS 对拍 client（bsh evs_selftest 对端——先于 QEMU 起，自带
    # 重试环吃 hostfwd 未就绪态；与 echod 同 trap 统一收）。
    EVS_LOG=$(mktemp "${TMPDIR:-/tmp}/berryos-evsclient.XXXXXX")
    python3 tools/evsclient.py > "$EVS_LOG" 2>&1 &
    EVS_PID=$!
    trap 'kill "$ECHO_PID" "$EVS_PID" 2>/dev/null; rm -f "$ECHO_LOG" "$EVS_LOG"' EXIT
    i=0
    until grep -q 'listening on' "$ECHO_LOG" 2>/dev/null; do
        kill -0 "$ECHO_PID" 2>/dev/null || { cat "$ECHO_LOG"; exit 1; }
        i=$((i+1)); [ "$i" -gt 25 ] && { echo "echod 5s 未 listening（18080 被占？）"; exit 1; }
        sleep 0.2
    done
    # A1-3b：rootfs fw_cfg 卷动态递送——build-user 起 mkcpio --split 32
    # 旁路产 rootfs0.cpio..（内核连续前缀发现序 opt/berry/rootfs0..9 优先，
    # 单卷帽 32MiB）；rootfs0 在席走多卷名，缺席回落旧名单单卷（混跑兼容
    # ——手动 mkcpio 无 --split 的树也照跑）。FWCFG 串一次构建、不加引号
    # 代入（词切分即多 -fw_cfg 参数，卷名无空格）。
    FWCFG=""
    i=0
    while [ -f "rootfs$i.cpio" ]; do
        FWCFG="$FWCFG -fw_cfg name=opt/berry/rootfs$i,file=rootfs$i.cpio"
        i=$((i+1))
    done
    [ -n "$FWCFG" ] || FWCFG="-fw_cfg name=opt/berry/rootfs,file=rootfs.cpio"
    {{QEMU}} {{MACH}} -kernel {{KERNEL}} $FWCFG \
        -global virtio-mmio.force-legacy=false \
        -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
        -device virtio-blk-device,drive=drv0 \
        -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082 -device virtio-net-device,netdev=n0 \
        -device virtio-tablet-device,id=tab0

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

# host 单元测试：berry-mm 的纯 Rust buddy 核心（见 mm/src/buddy.rs）
# + berry-loader 的 rootfs 解析纯函数面（G-eco-c 起，见 user/berry-loader）
# + berry-fsproto 的 fs 协议编解码纯函数面（P6 A-1 起，见 user/berry-fsproto）
# + berry-virtio 的 split-ring 布局/编解码纯函数面（P6 A-3 起，2026-10-24 起住 drivers/）
# + berry-blkproto 的块设备 RPC 协议编解码纯函数面（P6 A-4 起，见 user/berry-blkproto）
# + berry-fatfs 的 FAT12 BPB/目录项/簇链解析纯函数面（P6 A-5 起，见 user/berry-fatfs；
#   A-6 起含写基元——fat12_set/find_free_cluster/encode_file_entry，fixture
#   共识基准同时钉住读面与写面编码一致性）
# + berry-netproto 的以太网/ARP/IPv4/ICMP-echo/UDP/DNS 编解码纯函数面
#   （P6 A-7 起、A-8a 扩 UDP/DNS——vnet 驱动的协议面：ICMP ping 自验 +
#   DNS 查询往返，见 user/berry-netproto）
# + berry-netdproto 的网络服务 RPC 协议编解码纯函数面（P6 A-8 起，见
#   user/berry-netdproto——vnet 常驻服务化后的 DNS 查询协议面）
# + berry-tcp 的最小 TCP 客户端状态机纯函数面（T-c 起，见 user/berry-tcp
#   ——握手/序号推进/RTO/TIME_WAIT 状态转移，IO 驱动在 vnet tcp.rs）。
# fixture 守卫在 berry-fatfs 测试前置：mkfat --check-fixture 重产比对——
# 改 mkfat 忘重产 --fixture 时共识测试对陈旧基准假绿（A-5 对抗审查实抓）。
test-host:
    cargo test -p berry-mm --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-loader --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-fsproto --target {{host}}
    # C-0 刀 2：berry-fstree（RAM overlay 目录树纯函数面——v1 单段语义
    # 零漂移 + v2 五 op 面，host 17 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-fstree --target {{host}}
    # C-1 刀 1：berry-fsclient（fs 会话机自 bsh 迁居 cfg(not(test)) +
    # 纯函数面——fd 表状态机/cwd 拼接/seek 游标数学/stat 整形/dirent
    # 解码/ST→FsErr 映射，host 19 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-fsclient --target {{host}}
    # T-1 §1：berry-tls host 测（TLS 拷贝式纯函数面——tls_place 放置算术/
    # 拒绝面 + find_ptls 合成 mini-ehdr 补读 p_align@0x30 + 模板四全局；
    # 与 tools/tlsscan.py python 镜像同向量共识钉板。独立纯 crate 形态
    # （§6 回退）：berry-libc 的 libc 同名符号在 host 测试二进制里对
    # libSystem write 符号插入劫自旋，纯面外迁后 host 测试零 libc 面）。
    cargo test --manifest-path user/Cargo.toml -p berry-tls --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-mmap --target {{host}}
    # A7-4 §3.2：berry-signal host 测（POSIX 信号面纯函数底座——sigaction
    # 表/掩码状态机 + held 集/musl 128B sigset 位图层/投递分诊 classify，
    # host 24 测）。berry-tls/mmap 同款独立纯 crate 形态：
    # berry-libc 的 libc 同名符号在 host 测试二进制里符号插入，纯面外迁
    # 后 host 测试零 libc 面。
    cargo test --manifest-path user/Cargo.toml -p berry-signal --target {{host}}
    cargo test --manifest-path drivers/Cargo.toml -p berry-virtio --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-blkproto --target {{host}}
    python3 tools/mkfat.py --check-fixture user/berry-fatfs/tests/volume_fixture.bin
    # V-2：FAT16 双宽基准同位守卫（--check-fixture16——一卷一宽双 fixture，
    # 漂移即 FAIL 带 --fixture16 重产提示）。
    python3 tools/mkfat.py --check-fixture16 user/berry-fatfs/tests/volume_fixture_fat16.bin
    cargo test --manifest-path user/Cargo.toml -p berry-fatfs --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-netproto --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-netdproto --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-tcp --target {{host}}
    # C-3：berry-psproto（ps 查询 RPC 协议纯函数面——表/ver/mem 三 op
    # 编解码 + 严格解析全拒面 + 黄金字节钉板，host 15 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-psproto --target {{host}}
    # C-4：berry-complete（bsh Tab 补全纯函数面——token 切分四形/前缀
    # 匹配唯一·多候选·零命中/排序去重/列示字节，host 10 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-complete --target {{host}}
    # A2-1：berry-poll（多路等待聚合器纯函数面——wait_many n≤16 翻越：
    # 批切分 + poll-first 收割到干 + 分段阻塞轮转 + LT/非阻塞/唤醒三免费
    # 语义，host 13 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-poll --target {{host}}
    # GUI-1：berry-fbcon（自绘 8×16 字形 + 蜂格渲染核纯函数面——V-6 刀 2
    # 前置段产物，GUI-1 迁正 user/ members 随挂 host 测试：字形锚点/蜂格
    # 渲染/滚动/格式打包，host 28 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-fbcon --target {{host}}
    # GUI-2 段①：berry-gfx（pass 形态 API + soft 光栅核纯函数面——六件套
    # day-one：pass 边界/Caps/无运行时编译/display list/transient/文字三段；
    # glyph 快速路径与 fbcon 蜂格渲染逐字节像素共识 + golden 回归）。
    cargo test --manifest-path user/Cargo.toml -p berry-gfx --target {{host}}
    # GUI-3：berry-ui（保留树视图 crate——arena 代数 NodeId 防悬空 +
    # damage Σ×1.25 退化 + 遮挡剔除 paint + hit topmost + 手势
    # referee + 三态停帧 FramePhase + 重采样常量 guiproto 单源；
    # 验收三件 = damage/frame/offscreen）。
    cargo test --manifest-path user/Cargo.toml -p berry-ui --target {{host}}
    # GUI-4 段②：guiproto 协议面 + gui-inputd 泵/服务单测入 host 台账。
    cargo test --manifest-path user/Cargo.toml -p berry-guiproto --target {{host}}
    cargo test --manifest-path drivers/Cargo.toml -p gui-inputd --target {{host}}
    # app-0：berry-manifest（.bap manifest v0 纯函数解析——「解析器即产品」
    # 第 8 例，host 23 测：完整钉板/缺省合并/坏输入族 + 恰满边界正例/真
    # manifest fixture 共识/intent 判别值钉板）。
    cargo test --manifest-path user/Cargo.toml -p berry-manifest --target {{host}}
    # mkapp 门禁自检（a-3 U4：mkfat --check-fixture 同位前置——打包器门禁
    # 面此前只在 build-user 产物链上被动覆盖，门禁自身坏 = 产物链静默劣化）。
    python3 tools/mkapp.py --selftest
    # V-0a 刀 1：berry-efi（EFI 结构最小面——偏移/尺寸/签名/UTF-16 编码
    # host 钉板）+ elf2pe 自检（合成 ELF 全链转换钉板 + 未知重定位型负样本）
    # + mkfat --selftest（--put 引导盘构造面：链回读/嵌套目录/负样本——此前
    # 只手跑，门禁化同 a-3 U4 纪律）。
    cargo test --manifest-path bootloader/berryboot/Cargo.toml -p berry-efi --target {{host}}
    # V-0a 刀 3：berryboot app 单源共识——内嵌 HELLO.TXT 基准 ↔ 仓库 fixture
    # hello.txt 逐字节对拍（mkfat fixture 共识基准同族，改一侧忘同步即炸）。
    cargo test --manifest-path bootloader/berryboot/Cargo.toml -p berryboot --target {{host}}
    python3 tools/elf2pe.py --selftest
    python3 tools/mkfat.py --selftest
    # A1-3b：mkcpio --split 切分面守卫（重装恒等/恰整数倍/单卷/命名/
    # 拒面/陈卷清理）——rootfs 多卷产链（build-user --split 32）的编码
    # 单源执法，改 mkcpio 必跑。
    python3 tools/mkcpio.py --selftest-split
    # V-3：mkimage MBR 薄面自检（CHS 向量 + 表项钉板——卷编码面归
    # mkfat --selftest，单编码器纪律不在此长第二套）。
    python3 tools/mkimage.py --selftest
    # K-3：size.py ADRP 解码器向量自检（--assert-flip 扩面的门禁自身坏 =
    # smoke/build-release 断言静默劣化，同 a-3 U4 纪律门禁化）。
    python3 tools/size.py --selftest
    # T-1 §1：tlsscan 门禁自检（tls_place python 镜像 + F1/F2 全链 + cpio/
    # .bap 递归；mkfat --selftest 同位先例）+ census 审计（恒绿——拷贝式
    # 后 .tbss∩文件字节无害只配审计；rootfs.cpio 缺席打跳过不 FAIL）。
    python3 tools/tlsscan.py --selftest
    python3 tools/tlsscan.py --audit
    # A1-3a：mkcpio 切分器守卫（--split 卷产链：重装恒等/卷帽/无空尾卷/
    # 命名三形/非正卷宽拒——门禁自身坏 = A1-3b 内核 VMO 拼装核心不变量
    # 静默劣化，mkfat --selftest 同位纪律）。
    python3 tools/mkcpio.py --selftest-split

# 内核体积基线报告（12号报告 §10.1：体积讨论从 day-one 有数据）。
# 零依赖 python3 解析 ELF：文件量 / PT_LOAD 镜像与静态内存 / alloc 段明细。
# 只报数不设门禁——阈值等基线稳定后再定（Tock 192KB 教训）。
size: build
    python3 tools/size.py {{KERNEL}}

# B-0 性能基准单口（docs/基准与压力测试设计.md）：QEMU 裸跑收割 BENCH 行 +
# 基线对拍（tools/bench_baseline.txt，宽松因子 3x 拦数量级劣化）。改内核后
# 刷新基线：python3 tools/bench.py --update（diff 随源入册）。严格确定性
# 断言（churn 泄漏金丝雀/标记到齐）已挂 just smoke 永绿红线——本口只管数值。
bench: build
    python3 tools/bench.py

# R-1 实时基准单口（docs/基准与压力测试设计.md §10）：cyclictest 式 oversleep
# 三档（slice/4、slice、4·slice）× 32 样本。fw_cfg 具名项 opt/berry/rt 门控
# （缺席 = run/smoke 零扰动——公约 7：~34s 采样循环不进常规 boot；EFI 路
# 无 fw_cfg 具名文件 = smoke-image 亦天然缺席）；裸跑等 RT ok n=96，帽 120s；
# 无基线（oversleep>=0 严格断言在内核侧 panic 执法 = L0，分布数值只出表
# ——统计归统计）。
bench-rt: build
    python3 tools/bench.py --rt

# B-2 -icount 确定性通道单口（docs/基准与压力测试设计.md §13）：QEMU 追加
# 定值 `-icount shift=1,sleep=off`——周期数变指令数代理（shift=1 虚拟时钟
# ≈0.125 cyc/instr），与 TCG 实时基线**不可比、分文件**
# tools/bench_baseline_icount.txt（跨机/跨 QEMU 版本亦不可比，CI 首跑自立）。
# boot 时序全变 → 只跑 bench 收割（六行 BENCH 门禁面 + BENCHWAKE 观测行），
# 不用于功能面。L2 紧门禁形态（精确相等/±5%）由五跑证明烘焙为 bench.py
# ICOUNT_TOL 常量；证明可重验：
#   python3 tools/bench.py --icount --prove
# （五跑漂移谱 + verdict；漂移是合法结局——按 verdict 定 ICOUNT_TOL。）
bench-icount: build
    python3 tools/bench.py --icount

# S-1 soak 长跑单口（docs/基准与压力测试设计.md §11）：内核常驻任务 soakmon
# 每 60s vct（本机实测 ≈60s wall——CNTVCT ≈1.0x，历史半率机型 ≈120s，规划
# 按半率保守）打一心跳行 `SOAK t= sw= pages= lost=`，soak.sh 轮询四断言
# （t 严格 +1 / sw 严格递增 / pages 窗口 [首拍,首拍+64] / lost 恒 0）+ boot
# 覆盖守卫（SOAK gate on 门控自证 + ECO/VIRTIO/FAT/NET/DNS/TCP/TCPS 七标记
# ——vnet 服务链尾三标记必收，echod/TCP 链死亡必须 loud）。fw_cfg 具名项
# opt/berry/soak 门控（缺席 = run/smoke/EFI 零扰动；EFI 路无 fw_cfg 具名文件
# 天然缺席）；run-rootfs 全量形态 + echod（vnet TCP 自验对端——缺席则
# DNS/TCP 常驻族静默缩水）；首拍潜伏 5 周期（≈300-600s wall，让 boot 尾部
# 帧回收瞬态落在 pages 基线之前——3 周期 ≈182s 实测恰贴尾部末段，宿主负载
# 即失保证）。帽 = 300 + (N+5)×150 + 120 s。不进 CI（小时级，周跑/手跑）；
# 勿与 smoke/run-rootfs 并行（echod 18080 端口冲突；盘名 disk.soak.img 专属
# 与 smoke 的 disk.img 零交集）。默认 60 分钟（设计 ≥1h 目标，本机实测
# ≈1h）；短验 `just soak 2`（本机实测 ~6-7min）。SOAK_KEEP_LOG=1 留日志
# （FAIL 恒留）。参数经 `{{quote(minutes)}}` 引号包裹传递（对抗审查
# finding 9：裸 `{{minutes}}` 插值使 `just soak '60 && …'` 在组壳层注入
# 执行——quote 后载荷作为单 argv 落进脚本的 *[!0-9]* 数字拒面）。
soak minutes="60": build
    bash tools/soak.sh {{quote(minutes)}}

size-release: abi-check build-user
    cargo build --release
    python3 tools/size.py {{KERNEL_RELEASE}}

# ABI 单源工具链（`tools/abi.toml` = 常量层唯一事实源）：
#   abi-gen    —— 再生三面产物（内核 abi_gen.rs / berry-sys abi_gen.rs / 文档
#                marker 块）；改 ABI 后跑它，产物 diff 随源一起入册。
#   abi-check  —— 逐字节比对守卫（挂 build/check/smoke 前置）：忘再生成 /
#                手改产物即 FAIL。Plan B v0.5 的 abi_check.py 三方对账随之退役
#                （生成即一致，无需对账）。需 Python ≥3.11（tomllib）——
#                CLT 自带 /usr/bin/python3(3.9) 不够，brew python3 可。
abi-gen:
    python3 tools/gen_abi.py

abi-check:
    python3 tools/gen_abi.py --check

# 无人值守冒烟（CI 同款入口）：boot 标记断言 + switches 金丝雀严格递增。
# 调试保留串口日志：SMOKE_KEEP_LOG=1 just smoke
smoke: build
    bash tools/smoke.sh

# ---------- P7 x86_64 最小集（ADR #41：范围盒子 = 串口/定时器/异常向量/
# 上下文切换/SVC 入口；SMP/APIC/ACPI/PCIe 不在盒内）----------
# 独立小工作区（同 user/ 模式——根 workspace 不含本目录）。构建必须显式
# --target：根 .cargo/config.toml 锁了 build.target=aarch64-unknown-none，
# config 发现按 CWD 向上走、跨 workspace 边界照样命中（CLI --target 才能盖）。
X86_DIR := "arch/x86_64"
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
# TIMER ok → EXC dump vec=6 + EXC ok → CTX ok n=128 → SYS ok n=128 →
# X86 ok 七标记（P7-x1 定时器 100Hz 真中断 100 拍；P7-x2 ud2 触发 #UD
# dump 后 rip+2 恢复续行；P7-x3 A/B 两内核任务在独立 .bss 栈上严格乒乓
# 128 轮互验计数；P7-x4 CPL3 用户程序 SYSCALL/SYSRET 128 轮对拍 + DONE
# 哨兵经内核恢复路径归位）。
# 退出同 aarch64：Ctrl-A X。
run-x86: build-x86
    qemu-system-x86_64 -m 128M -nographic -no-reboot -kernel {{X86_KERNEL}}

# x86 独立冒烟（7 标记恰一次；不并入主 smoke——ARM64 四跑零扰动；P7-x4
# 收官起已挂 CI：ci.yml 两条 x86 步 = cargo 直调构建 + 本脚本）。
# 内核路径经 X86_KERNEL 传参（单一事实源——对抗审查实抓 smoke-x86.sh
# 曾自带第二份硬编码：cargo-target 是 gitignored 本地长存目录，后续切片
# 改名/挪 bin 时只改 justfile 一侧会让冒烟继续跑旧 ELF 假绿，G-eco-b
# 「陈旧产物静默上线」同族）。
smoke-x86: build-x86
    bash tools/smoke-x86.sh "{{X86_KERNEL}}"

# S-2b 副核参与冒烟——双相（`SMP-S2实施设计草案.md` §5.1）：相 1 裸相
# （27 标记 + SMP 握手）+ 相 2 rootfs 生态相（45 标记 + EVS/SKT 对拍
# client + PAR absent d1 副核参与活证——真并行 witness 归 S-3）。
# 断言面：`SMP ok ncores=2` 按相各恰一次 + 标记与 smoke.sh 计数对拍机器
# 守卫 + 金丝雀按相严格递增；时限 = 单核 ×2（双核 MTTCG 0.61x 吞吐），
# 脚本尾打 R3 定标三数（首跑定标义务）。内核路径经 KERNEL 传参（单一
# 事实源——smoke-x86 先例）。**build-user 依赖（§5.1 随迁件②）**：相 2
# 消费 rootfs*.cpio——新环境/worktree 缺产物时 loud 快败不烧 840s 帽。
# **-smp 只进本线**（与 run-smp 同注：主 smoke 族一律单核）。
smoke-smp: build build-user
    bash tools/smoke-smp.sh "{{KERNEL}}"

# V-4 img-boot 独立冒烟（M-G1 关卡；smoke-x86 同款形态——不并入主 smoke，
# ARM64 五跑零扰动，M-G1 收官时另裁升第六跑）。零 -kernel / 零 fw_cfg：AAVMF
# 挂 berryos.img 自举全链——EFI 十段恰一次 + 接续 src=handoff + 裸相 27 +
# EFI 路生态 15 + kfile/init 字节对拍 + 金丝雀严格递增 + RX 注入 + 注入后
# 活性复验。镜像/固件路径经参注入（单一事实源——smoke-x86 先例）。已挂 CI
#（ci.yml 步：Ubuntu 侧经 AAVMF_CODE/AAVMF_VARS 环境变量覆盖 brew 默认路径）。
smoke-image: image
    bash tools/smoke-image.sh "{{IMAGE}}" "{{AAVMF_CODE}}" "{{AAVMF_VARS}}"

# ---------- V-0a BerryBoot（EFI 骨架；VM 草案 §V-0，AAVMF 引导链）----------
# 独立小工作区（同 x86_64 模式——根 workspace 不含本目录；构建必须显式
# --target，根 .cargo/config.toml 锁的 build.target 跨 workspace 边界仍命中）。
# 构建链四步（路线修正记录 → docs/阶段沿革.md V-0a 条目）：cargo staticlib
# （builtin aarch64-unknown-none，免 nightly）→ rust-lld 静态全链接
# （--emit-relocs 保留重定位 + berryboot.ld 钉单 .img @0x1000、--image-base
# 0 ⇒ RVA==vaddr）→ elf2pe.py 转 PE32+（AArch64 EFI application；「staticlib
# + lld COFF 直链」在 stable 实测死路，故走 ELF 全链接+转换）。产物全在
# bootloader/berryboot/out/（gitignored）。V-3 起造盘段移交 `just image`
#（mkimage.py MBR + FAT16 整盘——单口吸收，V-1c 8MiB FAT12 盘约定随
# rootfs 14.4MB 已断，24MiB 修复在此；C-2c text 批后 rootfs ≈18-21MB，
# 盘容再抬 48MiB；A1-3b 96MiB——40MB+ 级 rootfs 验收形态同盘容，cap
# 传播见 mkimage --size-mib）。
# rust-lld 随 rustup 走（sysroot 内），无 brew 依赖；--verify 回读断言
# 恒开（entry 落 .img / .reloc 闭合 / subsystem=EFI app）。
BOOT_DIR := "bootloader/berryboot"
BOOT_TARGET := "aarch64-unknown-none"
BOOT_OUT := BOOT_DIR / "out"
BOOT_EFI := BOOT_OUT / "BERRYBOOT.EFI"
IMAGE := "out/berryos.img"
# AAVMF 固件对本机默认 brew 路径；CI（Ubuntu qemu-efi-aarch64 包）在
# /usr/share/AAVMF/AAVMF_{CODE,VARS}.fd——经环境变量覆盖，勿写第二份硬编码
#（Debian 系无 /usr/share/qemu/edk2-*.fd 布局，对抗审查 F1 实证）。
AAVMF_CODE := env_var_or_default("AAVMF_CODE", "/opt/homebrew/share/qemu/edk2-aarch64-code.fd")
AAVMF_VARS := env_var_or_default("AAVMF_VARS", "/opt/homebrew/share/qemu/edk2-arm-vars.fd")
# rust-lld 解析走 paths.sh（属性背引号环境无 `export PATH :=` 修补——
# :8-9 gotcha，GUI-1 段① just image 实踩 /lib/rustlib//bin/rust-lld；
# paths.sh 自足 PATH 同 KERNEL 行，单一事实源）。
RUST_LLD := `bash tools/paths.sh rust-lld`

# BerryBoot 构建链（EFI 应用——V-3 起纯 EFI 产链；造盘段已吸收进 just image）
build-boot:
    #!/bin/sh
    set -e
    cd {{BOOT_DIR}}
    # GUI-0 修复轮：根 .cargo/config.toml 的 -fp-armv8,-neon 旗经 config 发现
    # 向上吃到这里——但那是内核 SIMD-free 合同（kernel 限定）；BerryBoot 是
    # EFI 应用非内核（FPSIMD 态在 quiesce 交接后无保存义务），env RUSTFLAGS
    # 置空顶掉 config（与 build-user 同款对称——旗作用域 = 内核仓独占）。
    export RUSTFLAGS=""
    CARGO_TARGET_DIR=cargo-target cargo build --release --target {{BOOT_TARGET}}
    mkdir -p out
    "{{RUST_LLD}}" -flavor ld -static -nostdlib --emit-relocs \
        -z max-page-size=4096 -z noseparate-code \
        -T berryboot.ld -e efi_main --image-base 0 \
        -o out/berryboot.elf cargo-target/{{BOOT_TARGET}}/release/libberryboot.a
    python3 ../../tools/elf2pe.py out/berryboot.elf out/BERRYBOOT.EFI

# V-3 `just image` 单口：MBR + 单 FAT16 分区整盘镜像（吸收 build-boot 造盘
# 段）。mkimage.py = MBR 薄层（LBA 假几何 255H/63S，protective CHS 自洽即可
# ——草案 §V-3 原话）+ mkfat.build_volume_puts 单编码器（FAT 写法单源）；
# 四件套 BOOTAA64.EFI/HELLO.TXT/KERNEL.ELF/INITRD.BIN；hidden = 分区起始
# LBA（BPB 与分区表自洽）。盘容 96MiB（A1-3b：rootfs 40MB+ 验收形态
# （尾部放大件凑量）+ EFI/hello/kernel 三件后余量 ~2x；C-2c 史 48MiB
# = text 批后 rootfs ≈21MB 抬 2x；V-3 史：24MiB 修 C-2a 后 14.4MB
# 超 V-1c 8MiB FAT12 盘约定的存量断裂，FatDxe 实验已证 24MiB FAT16
# AAVMF 十段+接续全绿，2026-10-25）。产物 out/berryos.img（gitignored）
# ——UTM/真机挂盘即用；EDK2 PartitionDxe+FatDxe 消化 MBR/分区偏移，
# BerryBoot SFS 面零改动。
image: build build-boot
    #!/bin/sh
    set -e
    # V-0b 刀 1（裁决 C）口径沿承：KERNEL.ELF = 内核 debug ELF 直进盘——
    # 缺失即 loud；INITRD.BIN = rootfs.cpio（build-user 产出）同查。盘名
    # INITRD.BIN：草案 L91 曾误记 INIT.CPIO（8.3 三字符界），偏差记沿革。
    test -f {{KERNEL}} || { echo "kernel ELF 缺失（{{KERNEL}}）——先 just build"; exit 1; }
    test -f rootfs.cpio || { echo "rootfs.cpio 缺失——先 just build-user"; exit 1; }
    mkdir -p out
    python3 tools/mkimage.py {{IMAGE}} \
        --efi {{BOOT_EFI}} \
        --hello {{BOOT_DIR}}/hello.txt \
        --kernel {{KERNEL}} \
        --initrd rootfs.cpio \
        --size-mib 96

# AAVMF 引导验收（V-0b 刀 3 起）：QEMU virt + edk2 固件（pflash 对）+ xhci
# usb-storage 递盘。V-3 起盘 = out/berryos.img（just image 单口：MBR + 单
# FAT16 分区 96MiB（A1-3b 起，C-2c 史 48MiB、V-3 史 24MiB）——EDK2 PartitionDxe+FatDxe
# 消化分区偏移，BerryBoot
# SFS 面零改动；FatDxe 实验消雷 2026-10-25）。期望串口：BdsDxe starting
# Boot0001 …USB HARDDRIVE →
# 九行横幅 `BerryBoot EFI skeleton (V-0a)` / `fw: EDK II rev=0x…` /
# `gop: <WxH> fmt=<n> mode=<cur>/<max> fb=0x…` / `fs: HELLO.TXT 28B match` /
# `kfile: <n>B ok`（V-0b 刀 1：KERNEL.ELF GetInfo 探长 + AllocatePool +
# 循环 Read 整档收齐，n = 内核 ELF 字节数——与宿主 ls 对拍）/ `init:
# <n>B ok`（V-1c：INITRD.BIN = rootfs.cpio 整档读入 → AllocatePages
# (EfiLoaderData) 页钉（先于终段 GetMemoryMap——页进图非 Conventional，
# 内核 V-1b pass2 reserve 自动保）→ pa/len 填 handoff init_blob_*；盘上
# 缺席打 `init: absent` 续行）/ `elf:
# segs=<n> entry=0x… ok`（V-0b 刀 2：berry-loader boot_parse 引导视图——
# p_paddr 口径 + DRAM 窗拒面，坏档打 `elf: parse e=<码>`）/ `load:
# segs=<n> pages=<n> ok`（V-0b 刀 3：boot.span() 整窗 AllocateAddress
# 认领 + 整窗清零 + 逐段拷贝回读对拍 + handoff 页写 BerryHandoff v0，
# 坏段打 `load: verify i=<n>`）/ `BERRYBOOT ok`
# （全段过才打——状态谓词）。我们的 PE32+ 被 DXE 加载执行、ConOut 落
# PL011 串口；返回后固件续行进 UiApp 前台页。
# -device ramfb：virt -nographic 默认**无任何显示设备** → 固件不装 GOP
# 实例 → gop 行只会打 `locate fail r=EFI_NOT_FOUND`（刀 2 实证；ramfb =
# edk2 RamfbDxe 驱动面，GOP 查询的确定性供给）。vars.fd 自产 blank 64MiB
# 0xFF（2026-10-29 实抓：拷贝共享模板会继承手动 boot 写入的 BootOrder
# 陈旧项 → BDS 试败尾落 EFI Internal Shell；blank = 固件每跑重新枚举
# 在位设备自举——真「不带上一轮 BootOrder 状态」）。退出 Ctrl-A X。
# AAVMF 固件来自 brew qemu（edk2-aarch64-code.fd；vars 不再消费共享件）。
run-efi: image
    #!/bin/sh
    set -e
    python3 -c "import sys; sys.stdout.buffer.write(b'\xff'*(64<<20))" > {{BOOT_OUT}}/vars.fd
    {{QEMU}} -machine virt -cpu cortex-a72 -m 128M -nographic \
        -pflash "{{AAVMF_CODE}}" -pflash {{BOOT_OUT}}/vars.fd \
        -drive if=none,file={{IMAGE}},format=raw,id=bb \
        -device qemu-xhci -device usb-storage,drive=bb \
        -device ramfb
