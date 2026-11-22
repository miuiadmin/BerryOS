# BerryOS Phase 0 构建脚本
# 非交互 shell 的 PATH 不含 ~/.cargo/bin，这里固化。

export PATH := env_var("HOME") + "/.cargo/bin:" + env_var("PATH")

# 内核产物路径收敛在 tools/paths.sh（单一事实源，smoke.sh/CI 同源）：
# target 目录可能被全局 ~/.cargo/config.toml 的 target-dir 覆盖（本机指向
# ~/.cargo-target），故动态解析不硬编码；PATH 修补也在脚本内自足——
# `export PATH :=` 只进 recipe 环境、不进属性背引号（实测 just 此行为）。
KERNEL := `bash tools/paths.sh kernel`

# bash5 优先（/opt/homebrew/bin/bash、/usr/local/bin/bash）：smoke 系脚本含
# CJK 全角字符贴 $var 的字符串——bash 3.2 词法把多字节邻接吞进变量名（set -u
# 下 unbound variable 直炸）；bash5 无此雷。无 bash5 的环境回退 /bin/bash
# （Linux 发行版 / CI Linux runner 自带 ≥4，不受影响）。
BASH := `for b in /opt/homebrew/bin/bash /usr/local/bin/bash /bin/bash; do [ -x "$b" ] && { echo "$b"; break; }; done`
KERNEL_RELEASE := `bash tools/paths.sh kernel-release`

# 不用 `-bios none`：QEMU ≥11.x 把 "none" 当文件名，报
# `Could not find ROM image 'none'`。virt + -kernel ELF 直加载即可。
# 不带 `-smp`（QEMU 默认 1 核）是 v1 有意定值：免锁根基 = 单核串行
# （SMP设计草案 §1.1）；S-1 起双核只进 smoke-smp 线（`-smp 2`）。
MACH   := "-machine virt -cpu cortex-a72 -m 128M -nographic"
QEMU   := "qemu-system-aarch64"

# 演示面门透传（06 号修窗件 1）：`just DEMO=off run` = 注入 fw_cfg
# `opt/berry/demo-face,string=off`——task_a/task_b 忙环 + boot EL0 演示舰队
# + 验收 watchdog 不出（runqueue 清空 → tickless park → wfi）。缺席/on =
# 现状全演示（smoke/boot 相逐行零漂移）。宿主真通道 = berry CLI fw_cfg 面
# （display/rtc 同形，owner 域）；本变量是仓库侧直跑便门。
DEMO := ""

default: run

# G-eco 编译侧：用户程序 → flat binary（草案 D6）。builtin aarch64-unknown-none
# + 各 bin 自带链接脚本（build.rs 绝对路径注入）——**免 nightly**（custom target
# （推迟到确需 target 级差异再上，草案 D1 修订）。elf2flat（Rust 件，彻底
# Rust 化轨道 b 层 #8 二刀起产链现役）解 PT_LOAD 出平像
# 并补零到页（.bss 覆盖在文件侧兑现）。两遍构建（G-eco-b）：pass 1 先产
# echo/talker/thrchild/dpgp 平像进 user/userbins/，pass 2 svcmgr 才 include_bytes!
# 内嵌（单遍 workspace 无编排保证，鸡生蛋只能拆两刀）。--max 墙：子程序
# 4096（2 页映射 [0x1000,0x3000) 里首页装镜像、次页是栈——越页即吃栈）；
# svcmgr 163840 = UserProcess::build 的 D3 帽（A7-4 段② 64→128KiB 同步抬
# ——libc 信号面并入后平像 69.6KB 超 64KiB 旧帽；CP-08 再抬 128→160KiB——
# PM 预拉 arm 落段 + pager 钉位 0x1E000→0x28000 同值双改，平像恰满帽零
# 余量 by design）；T-1 thrchild 16384（4 页墙
# ——t2 栈 2048B 内嵌 .bss 与代码共页，平像恰 1 页；镜像最大 4 页时 svcmgr
# 映射窗 [0x1000,0x7000) 的主栈区仍余 2 页）；G4-2 dpgp 8192（2 页墙——零
# 句柄受验子像 1 页）。内核侧产物 kernel/src/userbins/
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
    # elf2flat 换轨二刀（彻底 Rust 化轨道 b 层 #8，§5 钉板五步第 5 步）：
    # 产链现役件 = tools/elf2flat（Rust 独立小工作区，#393 首刀件）——先编
    # 后用，六处消费点全走本 bin；Python 版降格为 elf2flat_oracle.py（对拍
    # 取证专用，不进产链）。独立小工作区 + 根 config 锁 build.target=
    # aarch64-unknown-none → 显式 --target host（test-host 接线同款纪律）；
    # CARGO_TARGET_DIR 显式钉 tools/elf2flat/cargo-target（防全局 target-dir
    # 覆盖漂移；该目录已 gitignore，同 tools/berry 形态）。对拍背书：六真实 ELF（echo/talker/
    # thrchild/dpgp/abuser/svcmgr）oracle↔Rust 产物 sha256 6/6 恒等 +
    # stdout 数值行全等（#393 首刀 + 本刀换轨前复验）。
    # 注意：本块在 `cd user` 之前执行，cwd = 仓库根——ROOT_ABS 取 $(pwd)
    # 即仓库根（勿用 `cd ..`——那会落到仓库父目录，换轨首跑实捕）。
    ROOT_ABS=$(pwd)
    HOST_TRIPLE="{{host}}"
    CARGO_TARGET_DIR="$ROOT_ABS/tools/elf2flat/cargo-target" \
        cargo build --release --manifest-path tools/elf2flat/Cargo.toml \
        --target "$HOST_TRIPLE"
    ELF2FLAT="$ROOT_ABS/tools/elf2flat/cargo-target/$HOST_TRIPLE/release/elf2flat"
    # mkapp 换轨五刀（彻底 Rust 化轨道 b 层 #4 刀序第四位，§5 钉板五步第
    # 5 步）：产链现役件 = tools/mkapp（Rust 独立小工作区，#522-#534 四刀
    # 件）——先编后用，下方四处 .bap 打包位全走本 bin。Python 版
    # mkapp.py 不退役（p3sim/gensourceindex/berry_assemble import 图根 +
    # CLI 对拍 oracle + selftest 语义钉板——selftest_guard 零豁免契约不
    # 动）。对拍背书：三组真实 staging（无 UI/全 UI/能力面）Rust↔Python
    # 产物 sha256 恒等 + 摘要行全等 + Python --check 交叉复验（tests/
    # build_parity.rs，cargo test test-host 前置跑）。mkcpio path 依赖 =
    # 单编码器纪律（mkimage→mkfat 同款先例）。cd user 后相对路径失效——
    # 一律走 $ROOT_ABS 绝对路径（mkcpio 块同坑纪律）。
    CARGO_TARGET_DIR="$ROOT_ABS/tools/mkapp/cargo-target" \
        cargo build --release --manifest-path "$ROOT_ABS/tools/mkapp/Cargo.toml" \
        --target "$HOST_TRIPLE"
    MKAPP="$ROOT_ABS/tools/mkapp/cargo-target/$HOST_TRIPLE/release/mkapp"
    # GUI-0（2026-10-29）：user 侧 none-target 家族（echo/talker/thrchild/svcmgr/
    # abuser/hello）**保留 NEON codegen**——EL0 任务 FP 态已由 TaskContext 全量保存
    # 兜底、GUI 光栅后续正要用 SIMD；根 .cargo/config.toml 的 -neon 旗只属内核
    # SIMD-free 合同。cwd=user/ 会向上吃到根 config，env RUSTFLAGS 优先于 config
    # → 显式清空还原用户仓 codegen（零漂移；musl 面本就无旗）。
    export RUSTFLAGS=""
    cd user
    CARGO_TARGET_DIR=cargo-target cargo build --release -p echo -p talker -p thrchild -p dpgp
    mkdir -p userbins ../kernel/src/userbins
    "$ELF2FLAT" \
        cargo-target/aarch64-unknown-none/release/echo \
        userbins/echo.bin --max 4096
    "$ELF2FLAT" \
        cargo-target/aarch64-unknown-none/release/talker \
        userbins/talker.bin --max 4096
    # T-1 §2：thrchild（进程内双线程自验子——bootstrap 自句柄经 process_start
    # x0 递入；pass 1 平像 + svcmgr include_bytes 内嵌，零 rootfs 新条目）。
    "$ELF2FLAT" \
        cargo-target/aarch64-unknown-none/release/thrchild \
        userbins/thrchild.bin --max 16384
    # G4-2：dpgp（demand 装载受验子——跨进程供给形 EL0 侧，bootstrap=0 零
    # 句柄；2 页墙 = 像 1 页 + 头寸，像窗 [0x1000,0x6000) 外 0x6000 起
    # demand 页由 svcmgr dpg_selftest 另映）。同产链内嵌 svcmgr。
    "$ELF2FLAT" \
        cargo-target/aarch64-unknown-none/release/dpgp \
        userbins/dpgp.bin --max 8192
    CARGO_TARGET_DIR=cargo-target cargo build --release
    "$ELF2FLAT" \
        cargo-target/aarch64-unknown-none/release/abuser \
        ../kernel/src/userbins/abuser.bin --max 4096
    "$ELF2FLAT" \
        cargo-target/aarch64-unknown-none/release/svcmgr \
        ../kernel/src/userbins/svcmgr.bin --max 163840
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
    # B-1（docs/基准与压力测试设计-v1.0.md §8）：bench1——EL0 用户态四项微基准
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
    # devmgr v1（驱动子系统研究报告 §4.2 交付 1.1 最小先行版：MMIO 槽
    # 枚举 + 驱动匹配 + 服务状态报告）：扫 32 槽 virtio-mmio 窗建设备
    # 清单，did→驱动匹配表（blk=2→vblk/net=1→vnet/input=18→gui-inputd/
    # gpu=16→guifbd + 表补全刀先行绑定 console=3/rng=4→berry-rng〔账本
    # 行名 rngd，psproto NAME_MAX=8 帽〕），查 svcmgr ps 账本（psproto
    # PS_LIST，ps 同拓扑）打每设备服务在席状态，缺席 WARN。**不拉起
    # 服务**（svcmgr 点名链不动，v2 自动拉起）；逻辑核在 lib（host
    # `cargo test --lib` 11 测）。
    # 独立小工作区 musl 链（ps/guifbd 同链）；rootfs 条目走
    # tools/rootfs-manifest.toml（svcmgr cpio_find 点名制不识即零扰动）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/devmgr/Cargo.toml \
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
    # GUI-8 视觉补全退役（刀 A 应用槽合流）：appstub ELF 构建面退役——
    # 七槽（SETTINGS/FILES/PHOTOS/CAMERA/CONTACTS/MMS/BROWSER）全数换真
    # crate（下段），零 bin/ 消费者；appstub 仅存占位 fallback 职能 =
    # apps/<ID>/{icons,views} 资产兜底源（contacts/settings 两 crate 无
    # 自有资产目录，下段 if 兜底消费）+ genohicons.py 图标再生成输出位。
    # ELF 壳本体（bins/appstub/src）在册保留——后续新增槽未落 crate 前
    # 的应急占位（重建接线 = 恢复本构建行 + cp 到 staging bin/）。
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
    # GUI-2 段③：gfxdemo（消费侧首刀——present/hdi 域首个真实消费者闭环：
    # berry-gfx soft 渲帧 → VMO → SUBMIT/FRAME_CALLBACK/RELEASE → hdi 软
    # vsync mini 序；svcmgr EFI 相预拉直递 extras 6/8；全断言过打 `GFX ok`，
    # smoke-image ECO 标记；rootfs 相随 guifbd 门静默跳过零新行）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/gfxdemo/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # GUI-5：gui-wmd（窗口管理守护——ADR #61 呈现/策略分离：权威窗口表
    # 两层窗/Z 序/AVOID_SYS 避让有效几何/单焦点 + present/hdi 双域透传
    # 中继 + inputd 推送面；svcmgr EFI 相预拉 extras 4 枚恰 8/8 满配；
    # guifbd/gfxdemo 同型独立小工作区；`gui-wmd: up` 诊断行非标记）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/gui-wmd/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # GUI-8 可先行面（侦察 §5）：berry-launcher 桌面骨架——guifbd 同形
    # 独立小工作区 musl 链。v0 = fbinfo gift 门控镜像（缺席单行降级 park
    # 零标记扰动；在场单行 LAUNCH 日志 + park）；svcmgr 预拉/present/fs/
    # spawn/标记全挂 gui3→main 并线后 wiring 刀（本刀 svcmgr 面零触碰）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/berry-launcher/Cargo.toml \
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
    # sim-S1 M2-b：simbattery（sim 电池快照注入服务——UART1 帧字节流经
    # task_simcon 转发至 fd0，逐字节 pwproto 解码 → seq 单调校验 → 9B ACK
    # 回 gift[0] 端 + `SIMBAT ok seq=N` 单消息标记；svcmgr 门控预拉 = 表
    # 0x0001_0005 在席（fw_cfg simdev 门开相）。stdhello 同链。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/simbattery/Cargo.toml \
        --target aarch64-unknown-linux-musl
    # CP-08 装配件（04 号复核 §6-5/§6-6）：PM 双服务入 rootfs——
    # berry-power（电源中枢服务骨架：频率帽聚合/充电限流票仲裁/util
    # EWMA 消费位/pwproto 帧会话纯核全在 lib〔host 单测盖〕，bin 壳 =
    # 起机自检 dry-run 诊断行后 park 常驻）+ berry-thermal（热治理仲裁
    # 引擎 + 自检/park 壳）。bin 面经 wired 门链 berry-libc shim
    # （println/park 原语——04 号 F4 缺注随本刀收口；berry-thermal
    # Cargo.toml 门注同款：host cargo test 相整跳 bin，纯核全在 lib）。
    # **消费端已挂（CP-08 闭合刀）**：svcmgr 预拉 arm 落段（vnet 后
    # 预拉位，拉起成功打 `eco: berry-power up`/`eco: berry-thermal up`
    # 观测行）——原几何阻断已解除：D3 帽 128→160KiB + pager 钉位
    # 0x1E000→0x28000 同值双改（kernel/src/user.rs USER_CODE_MAX +
    # main.rs X-3c-1 + link-user.ld；阻断实证 = 调度彻底计划 10 号
    # §2.5——旧 .bss 终点 0x1dff0 距旧钉仅 0x10，arm 最小面 ~0x1a0
    # 装不进，链接期 loud 炸响）。通道端点 gift/账本面随分端执法刀。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/berry-power/Cargo.toml --features wired \
        --target aarch64-unknown-linux-musl
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/berry-thermal/Cargo.toml --features wired \
        --target aarch64-unknown-linux-musl
    # G4 X-4：dpgx（exec 装载差分子——单 PT_LOAD 特形链接脚本
    # bins/dpgx/link-dpgx.ld：单 LOAD 单 k=−0x200000（X-4e：FILEHDR
    # PHDRS 吃头进段——先前 −0x1ff000 时头在段外，__ehdr_start=0 杀胎）
    # + tdata pad 页齐 →
    # filesz 端==fs_top = bss 文件域窗空 → berry-exec congruent 六查全过
    # = demand 首个真消费者（现役 musl 族分段多 k 全走 eager 兜底，X-4
    # 宿主清点实证）。仅 bench dpgx_foot 配对拉起，rootfs `dpgx.on` 缺席
    # 即整段静默 = smoke 零漂移 by construction。-T 走绝对路径（linker
    # CWD 随 cargo/rustc 调用链漂移，相对路径不可靠——lab 实证）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096 -C link-arg=-T -C link-arg=$ROOT/user/bins/dpgx/link-dpgx.ld" \
        cargo build --release --manifest-path bins/dpgx/Cargo.toml \
        --target aarch64-unknown-linux-musl
    rm -rf userbins/app-stage userbins/app
    mkdir -p userbins/app-stage/APPDEMO/bin userbins/app-stage/APPQUIET/bin userbins/app
    cp bins/appdemo/manifest.toml userbins/app-stage/APPDEMO/
    cp cargo-target/aarch64-unknown-linux-musl/release/appdemo userbins/app-stage/APPDEMO/bin/
    # GUI-7：[ui] 资产入 staging——icons/ PNG 源由 mkapp 编码成 .byi 入包
    # （源不入包），views/ 原样整包；对应 manifest icon/views 声明位。
    cp -R bins/appdemo/icons userbins/app-stage/APPDEMO/
    cp -R bins/appdemo/views userbins/app-stage/APPDEMO/
    cp bins/appquiet/manifest.toml userbins/app-stage/APPQUIET/
    cp cargo-target/aarch64-unknown-linux-musl/release/appquiet userbins/app-stage/APPQUIET/bin/
    "$MKAPP" userbins/app/APPDEMO.bap userbins/app-stage/APPDEMO
    "$MKAPP" userbins/app/APPQUIET.bap userbins/app-stage/APPQUIET
    # app-2 刀2：APPTOMB 迁 libc 路（docs/app-2实施设计草案-v1.0.md §4
    # 换链——musl target + berry-libc shim + berry-ld 剥 musl crt 同
    # stdhello 链；none target 全量 build 行不再产出〔user/ members 已摘
    # 除〕，改本段显式构建。不进 boot 批〔svcmgr 白名单 skip〕，拉起权归
    # tomb_selftest——`FROZEN ok n=1`/`WAKE ok n=1`/`TOMB ok n=1` rootfs
    # 相标记；协议帧码/counter 对账面对 svcmgr 零改 = 迁移不改协议字节。
    # 装配径不变：cp ELF 本体直进 staging → mkapp（门禁第 5 道要求
    # `\x7fELF` 魔数——K2 互斥注：不经 elf2flat）。
    CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
        cargo build --release --manifest-path bins/apptomb/Cargo.toml \
        --target aarch64-unknown-linux-musl
    mkdir -p userbins/app-stage/APPTOMB/bin
    cp bins/apptomb/manifest.toml userbins/app-stage/APPTOMB/
    cp cargo-target/aarch64-unknown-linux-musl/release/apptomb userbins/app-stage/APPTOMB/bin/
    "$MKAPP" userbins/app/APPTOMB.bap userbins/app-stage/APPTOMB
    # 波 4 先行批（W4-RECON §5.4 改动点 1）+ 刀 A SETTINGS 接线：七应用
    # crate 接线——appstub 秒退壳退役，APPTOMB :315-327 食谱（musl +
    # berry-ld 绝对路径 RUSTFLAGS → staging → mkapp 五道门禁）。资产单源
    # = crate 内 icons/views（contacts/settings 批资产留 appstub——两
    # crate 无资产目录，W4-IMPL-CONTACTS §1.1 口径，if 兜底两形）。SETTINGS
    # 体 = 主树 settings crate（9035af061 垂直切片，A-RECON §二裁决：交互
    # 帧循环 + crate 级全形态 manifest，弃 wave3 settings-app 静态手绘页）。
    # 常驻 park 形入镜像的前置 = svcmgr 裁决 B（boot 批 [ui] 入册不拉起 +
    # `app <id> gui deferred` 行——W4-RECON §1.7 铁律），同批已落；smoke
    # APP/APPUI 计数零漂（n=9/n=8 仍打，语义随裁决 B 两支化），PS 行数随
    # appdemo 不再 boot 拉起而 -1（前缀断言面零扰动）——W4-VERIFY.md 落账。
    # 终端刀 +1：berry-term（TERMINAL——GUI 终端首件，settings 镜像骨架 +
    # 行环与回显字节解释器（berry-termio 纯 crate）/持久子 bsh 哑终端/
    # 启动管道自检与自动 respawn 四件面）入 build/spec 两循环；恰
    # 满 APP_SLOTS=10 第 10 槽（svcmgr 已参数化 n≥10 十位入拼），smoke
    # APP 计数 n=9→10、APPUI n=8→9（标记面随迁）。
    for c in settings files photos camera contacts messages browser berry-term; do
        CARGO_TARGET_DIR=cargo-target RUSTFLAGS="-C linker=$ROOT/tools/berry-ld -C linker-flavor=ld.lld -C link-arg=-zmax-page-size=4096" \
            cargo build --release --manifest-path "bins/$c/Cargo.toml" \
            --target aarch64-unknown-linux-musl
    done
    for spec in SETTINGS:settings FILES:files PHOTOS:photos CAMERA:camera CONTACTS:contacts MMS:messages BROWSER:browser TERMINAL:berry-term; do
        id=${spec%%:*}; c=${spec##*:}
        mkdir -p "userbins/app-stage/$id/bin"
        cp "bins/$c/manifest.toml" "userbins/app-stage/$id/"
        cp "cargo-target/aarch64-unknown-linux-musl/release/$c" "userbins/app-stage/$id/bin/"
        if [ -d "bins/$c/icons" ]; then
            cp -R "bins/$c/icons" "userbins/app-stage/$id/"
        else
            cp -R "bins/appstub/apps/$id/icons" "userbins/app-stage/$id/"
        fi
        if [ -d "bins/$c/views" ]; then
            cp -R "bins/$c/views" "userbins/app-stage/$id/"
        else
            cp -R "bins/appstub/apps/$id/views" "userbins/app-stage/$id/"
        fi
        "$MKAPP" "userbins/app/$id.bap" "userbins/app-stage/$id"
    done
    # G-eco-c/d + app-0：rootfs.cpio（hello + stdhello + hexyl + bsh + fail + bench1 + fsmgr + vprobe + vblk + vnet ELF 本体——装载走
    # svcmgr 流式 cpio/ELF 链，非 include_bytes 内嵌；产物在仓库根 =
    # `run-rootfs` 既有约定；条目按名排序 = 确定性输出。app-0 起加
    # app/<ID>.bap 整包条目（'a' < 'b' 排序在 bin/ 前；a-1 阶段 svcmgr
    # 主走查对未知名算术跳过 = 零消费、零行为变化——appd 消费随 a-2）。
    # app-1 D1（调和书）：APPTOMB 打包出 app/ 前缀改 selftest/ 专用位
    # ——不入 bap_scan 收集 = 不占 APP_SLOTS（app-1 时点 9/9 恰满、
    # 2026-11-17 终端刀后现役 10 槽；入册必挤 SETTINGS
    # = APP ok 静默漂 8），tomb_selftest 走 cpio 直查，白名单立法退役。
    # V13（ADR #82 预埋刀）：58 条 name=rootfs 硬编映射外化
    # tools/rootfs-manifest.toml（平台基座自描述——加 bin 只改该表；
    # mkcpio --manifest 加载/拒面见其头注）。--split 32 与出档路径留配方。
    # mkcpio 换轨二刀（彻底 Rust 化轨道 b 层 #8 刀序首位，§5 钉板五步第
    # 5 步）：产链现役件 = tools/mkcpio（Rust 独立小工作区，#435 首刀件）
    # ——elf2flat 换轨二刀同款纪律：先编后用；CARGO_TARGET_DIR 显式钉
    # tools/mkcpio/cargo-target（防全局 target-dir 覆盖漂移，elf2flat 同款）。
    # 注意：本块在 `cd user` 之后执行，相对路径失效——cargo build 与产物
    # 定位一律走 $ROOT_ABS 绝对路径（ROOT_ABS 在 cd 前已取仓库根；
    # build-boot 的 elf2pe 同坑纪律）。
    # Python 版 mkcpio.py 不改名退役——它是六处模块 import 的图根
    # （mkapp/tlsscan/bench/smoke-dpgsoak 的 entry/build_archive、
    # berry_assemble/gensourceindex 的 load_manifest，单编码器纪律），
    # 保留 = 模块面现役 + CLI 对拍 oracle（真产链形态对拍背书：--manifest
    # rootfs-manifest.toml + --split 32 真构建树 44.6MB/63 条目 → 主档+
    # 双卷 sha256 6/6 恒等 + stdout 行全等 + .berryver 同烙印）。
    CARGO_TARGET_DIR="$ROOT_ABS/tools/mkcpio/cargo-target" \
        cargo build --release --manifest-path "$ROOT_ABS/tools/mkcpio/Cargo.toml" \
        --target "$HOST_TRIPLE"
    MKCPIO="$ROOT_ABS/tools/mkcpio/cargo-target/$HOST_TRIPLE/release/mkcpio"
    "$MKCPIO" ../rootfs.cpio \
        --manifest ../tools/rootfs-manifest.toml \
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

# SDK-2 集成化（P3 前置刀）：现役签名链「在签」——rootfs-manifest 九 app
# dev 签发一键化。流程 = berry-sign keygen（tools/testkeys/dev 缺则产——
# dev 密钥纪律 = tools/testkeys/README.md，钥本体不入仓）→ 逐 app 条目
# berry-sign sign 产 .bap.sig（真 .bap 从构建树 userbins/；manifest
# [security] 三键 = 信封回声源，berry-sign 拒收缺席件）→ gensourceindex
# 产官方源静态实例两件套（真签名索引：.sig 在席 → 公钥见证 witnessed；
# selftest/* 夹具不计——load_app_entries 同口径）→ stdout 汇总。签名面
# 显式可选：build-user 默认行为零动（不签名照常出 rootfs）。
sign-user: build-user
    #!/bin/sh
    set -e
    # berry-sign 发现序 = mkapp._find_berry_sign 同序镜像（BERRY_SIGN →
    # target-cargo → /tmp/target-p{1,2} × debug/release）；全缺现场构建
    #（host target，产物落 target-cargo 惯用位——下次发现序自然命中）。
    BS=${BERRY_SIGN:-}
    if [ -z "$BS" ] || [ ! -x "$BS" ]; then
        for tdir in target-cargo /tmp/target-p1 /tmp/target-p2; do
            for prof in debug release; do
                if [ -x "$tdir/aarch64-apple-darwin/$prof/berry-sign" ]; then
                    BS=$tdir/aarch64-apple-darwin/$prof/berry-sign
                    break 2
                fi
            done
        done
    fi
    if [ -z "$BS" ] || [ ! -x "$BS" ]; then
        echo "sign-user: berry-sign 不可得——现场构建（target-cargo 惯用位）"
        CARGO_TARGET_DIR=target-cargo cargo build --manifest-path user/Cargo.toml \
            -p berry-crypto --bin berry-sign --target aarch64-apple-darwin
        BS=target-cargo/aarch64-apple-darwin/debug/berry-sign
    fi
    # dev 密钥对：缺则产（sk 0600；永不真签——真签面随 #77/P3，纪律全文
    # = tools/testkeys/README.md）。
    if [ ! -f tools/testkeys/dev/dev.sk ] || [ ! -f tools/testkeys/dev/dev.pk ]; then
        mkdir -p tools/testkeys/dev
        "$BS" keygen tools/testkeys/dev/dev.sk tools/testkeys/dev/dev.pk
    fi
    # 逐 rootfs-manifest app 条目签发（路径相对 user/、条目名无空白——
    # 表头注契约见 tools/rootfs-manifest.toml）。
    n=0
    for rel in $(python3 -c 'import tomllib; rows = sorted(tomllib.load(open("tools/rootfs-manifest.toml", "rb"))["entries"].items()); print(" ".join(p for k, p in rows if k.startswith("app/") and k.endswith(".bap")))'); do
        "$BS" sign tools/testkeys/dev/dev.sk "user/$rel"
        n=$((n + 1))
    done
    # 官方源静态实例两件套 → user/userbins/source/（gitignored 产物）。
    # 五刀产链换轨（b 层 #5）：先编后用 + CARGO_TARGET_DIR 钉
    # tools/gensourceindex/cargo-target（elf2flat/mkcpio/mkfat/mkimage/
    # mkapp 同款纪律）；Python gensourceindex.py 不退役——CLI 对拍
    # oracle（test-host 挂双侧，tools/gensourceindex cargo test 行）+
    # berry_assemble import 面单源。
    HOSTG=$(rustc -vV | sed -n 's/^host: //p')
    CARGO_TARGET_DIR="$PWD/tools/gensourceindex/cargo-target" \
        cargo build --release --manifest-path tools/gensourceindex/Cargo.toml \
        --target "$HOSTG"
    "$PWD/tools/gensourceindex/cargo-target/$HOSTG/release/gensourceindex" \
        --sign-key tools/testkeys/dev/dev.sk \
        --rootfs-manifest tools/rootfs-manifest.toml \
        --user-dir user \
        --out-dir user/userbins/source \
        --source-pk tools/testkeys/dev/dev.pk
    echo "sign-user: ok n=$n .sig + source-index/keys → user/userbins/source（mkimage --source-instance user/userbins/source 挂接）"

build: docs-guard abi-check build-user
    cargo build

# K-2 release 链接守卫：release 优化（fat LTO + O3）才生成的引用形态（.text.boot
# ADRP 引 .rodata.cst16 越 ±4GB）可断链而 debug 全绿——只构建不跑 QEMU，挂 CI。
# K-3：构建后尾挂 size.py --assert-flip——release 面 .text.boot 全部 ADRP 目标
# 必低半（bit63=0）断言（debug 面经 smoke.sh 既有调用零改动生效）。
build-release: docs-guard abi-check build-user
    cargo build --release
    python3 tools/size.py --assert-flip {{KERNEL_RELEASE}}

# 入口规则门禁（AGENTS 注入预算 + 探针/锚点/迁出目标）——build/check/CI 前置
docs-guard:
    python3 tools/docs_guard.py

# 计数事实制品对拍（工程纪律刀阶段 4；规则/16 §3）——数字自证面；
# 刷新 = facts --update（动计数输入面的同一提交带刷）。
# 二期 P1 起走信封+回执双包裹（gate-wrap）：跑前跑后键一致才落回执，
# 判决文件化——启动器退出码与工作判决分家。
facts:
    bash tools/gate-wrap.sh facts facts --files tools/facts.py tools/facts.txt -- python3 tools/facts.py

facts-update:
    python3 tools/facts.py --update

# 可写全局清单门禁（Ginux writable-globals 借法）：新增 static mut 必须入册
writable-globals:
    python3 tools/writable_globals.py --check

check: docs-guard writable-globals abi-check build-user
    cargo check

# 默认机型 virt + -kernel ELF：QEMU 直加载，起始 EL1。期望输出: 1BerryOS boot
# DEMO=off 时多一行 DEMO gate off（门态自证）+ 舰队/忙环零输出。
# M1 止血：直跑族日志 tee 落盘 out/logs/<recipe>-<epoch>.log（stdio 即焚
# 是 AI 取证面最大空白）；pipefail 保 QEMU 退出码不被 tee 吞。
run: build
    #!/bin/sh
    set -e
    set -o pipefail
    mkdir -p out/logs
    {{QEMU}} {{MACH}} {{ if DEMO == "" { "" } else { "-fw_cfg name=opt/berry/demo-face,string=" + DEMO } }} -kernel {{KERNEL}} 2>&1 | tee "out/logs/run-$(date +%s).log"

# virtualization=on：强制起始 EL2，测 boot.asm 的 EL2→EL1 eret 降级。期望输出: 21BerryOS boot
run-el2: build
    #!/bin/sh
    set -e
    set -o pipefail
    mkdir -p out/logs
    {{QEMU}} -machine virt,virtualization=on -cpu cortex-a72 -m 128M -nographic -kernel {{KERNEL}} 2>&1 | tee "out/logs/run-el2-$(date +%s).log"

# S-1 副核 bring-up 手跑（`SMP设计草案.md` §3/§4）：默认机型 + -smp 2——
# PSCI CPU_ON 拉副核走 §3.2 十项 → 心跳握手 → `SMP ok ncores=2`。
# **-smp 只进本线**（run-smp/smoke-smp）：主 run/run-el2/run-rootfs/smoke
# 一律单核——单核零漂移硬线（S-1 盒子边界）。默认机型（boot EL1）才有
# HVC→PSCI 拦截；virtualization=on 下内核 BOOT_EL 门先行跳过（conduit 分型）。
run-smp: build
    #!/bin/sh
    set -e
    set -o pipefail
    mkdir -p out/logs
    {{QEMU}} {{MACH}} -smp 2 -kernel {{KERNEL}} 2>&1 | tee "out/logs/run-smp-$(date +%s).log"

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
# sim-S1（`docs/sim设备通道与契约设计-v1.1.md` §7）：+第二 PL011（serial1 =
# sim 注入通道，unix socket 宿主外接）+ fw_cfg 具名键 `opt/berry/simdev`
#（= UART1 在席门——**serial1 与键同线配对**是拓扑契约：只配键不配 serial
# 则 0x0904_0000 是洞、内核门开即 boot 崩；run/smoke/smoke-image/EFI 不带
# 二者 = 零漂移）。serial0 显式钉 mon:stdio（-nographic 隐式默认的显式形
# ——防唯一 -serial 被解释为 serial0 吞走 console）。注入 client 连
# /tmp/berryos-sim1.sock（M3 smoke 面同路径）；boot 标记 `SIM gate on`。
# mkfat 换轨二刀（b 层 #8 刀序第二位）：产链位切 Rust 件——先编后用 +
# CARGO_TARGET_DIR 钉 tools/mkfat/cargo-target（elf2flat/mkcpio 同款
# 纪律）；Python mkfat.py 保模块面（mkimage/pwrcls import 单编码器）+
# oracle 对拍（test-host 挂双侧）。
run-rootfs: build
    #!/bin/sh
    set -e
    set -o pipefail
    test -f rootfs.cpio || { echo "缺 rootfs.cpio（G-eco 编译侧产出）"; exit 1; }
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    CARGO_TARGET_DIR="$PWD/tools/mkfat/cargo-target" \
        cargo build --release --manifest-path tools/mkfat/Cargo.toml \
        --target "$HOST"
    "$PWD/tools/mkfat/cargo-target/$HOST/release/mkfat" disk.img
    printf 'BERRYOSVBLK!' | dd of=disk.img conv=notrunc status=none
    # sim-S1：残留 socket 清理（QEMU 字段协议——server=on 对残文件 bind 拒）。
    rm -f /tmp/berryos-sim1.sock
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
    mkdir -p out/logs
    {{QEMU}} {{MACH}} -kernel {{KERNEL}} $FWCFG \
        -global virtio-mmio.force-legacy=false \
        -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
        -device virtio-blk-device,drive=drv0 \
        -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082 -device virtio-net-device,netdev=n0 \
        -device virtio-tablet-device,id=tab0 \
        -serial mon:stdio \
        -serial unix:/tmp/berryos-sim1.sock,server=on,wait=off \
        -fw_cfg name=opt/berry/simdev,string=1 \
        2>&1 | tee "out/logs/run-rootfs-$(date +%s).log"

run-release: docs-guard abi-check build-user
    #!/bin/sh
    set -e
    set -o pipefail
    cargo build --release
    mkdir -p out/logs
    {{QEMU}} {{MACH}} -kernel {{KERNEL_RELEASE}} 2>&1 | tee "out/logs/run-release-$(date +%s).log"

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
    # W1-Q6 前置：bash 3.2 吞名守卫——`$NAME` 后紧邻多字节字符在 macOS
    # 系统 bash 3.2 下吞进变量名（set -u 直炸/否则静默吃值），CI bash 5
    # 免疫故挂账潜伏。tools/*.sh 77 处已改花括号包裹（字节级同义），
    # 本门禁防回归（smoke 本地复现的宿主断点收口）。
    python3 tools/shvarcheck.py --check
    cargo test -p berry-mm --target {{host}}
    cargo test -p berry-asidface --target {{host}}
    cargo test -p berry-vmarface --target {{host}}
    # W3-Q2：berry-wqmatch（等待队列唤醒匹配纯函数面——b-1 广播不变量/
    # WAIT_ANY 哨兵/fastpath 单等待者资格三律钉板 + 两路语义对齐穷举，
    # host 9 测；谓词单源，sched wake_matching/find_single_waiter 消费）。
    cargo test -p berry-wqmatch --target {{host}}
    # W3-Q18：berry-dtbface（FDT walker 纯函数面——rd32/cstr_at/prop_u64
    # 界检读 + find_initrd 迭代 walk 状态机：token 推进律//chosen 深度判/
    # 4 对齐步进/畸形九态全 None 零 panic，host 13 测；单源 crate，
    # kernel dtb.rs 壳化消费（boot_probe 物理臂留内核侧））。
    cargo test -p berry-dtbface --target {{host}}
    # W3-Q66：berry-wakeface（唤醒归因链纯函数面——GIC INTID→源映射
    # 板上六活锚/SGI 段正交/SPURIOUS 不进号表/保存值域 = 九源集/槽位
    # 钳位，host 12 测；单源 crate，HAL wake.rs 消费〔src_of 号表锚
    # 参数化注入 + irq_exit 值域执法；wsrcface 姊妹件——号归源/源分类
    # 进环两件合钉刀1 归因链〕）。
    cargo test -p berry-wakeface --target {{host}}
    # W3-Q106：berry-vdsoface（vDSO 数据页布局与换算纯函数面——布局冻结
    # 钉板/magic+freq 谓词四态/换算式与内核 sys_clock_read 逐位对拍含
    # 非二次幂频率精确域，host 5 测；单源 crate，kernel/src/vdso.rs 建页
    # + berry-sys fast path 双消费〔03 号审计机会三〕）。
    cargo test -p berry-vdsoface --target {{host}}
    # W3-Q28：berry-wsrcface（唤醒源分类/白名单/钳位纯函数面——WSRC_F_*
    # 五常量 + M_* 九源冻结值镜像 + class_of/valid/slot_index 三谓词，
    # host 8 测（9 源两列全表/256 值全域扫描/钳位律/POWER_KEY 到达-only
    # 三角/预留段律/位面冻结/到达置位门判序镜像）；单源 crate，kernel
    # telemetry.rs 薄壳 re-export + 五钳位点换单源。
    cargo test -p berry-wsrcface --target {{host}}
    # W3-Q3：berry-vmoledger（VMO 页面生命周期簿记纯函数面——提交/归还
    # 资格、映射计数悬空 PTE 防线、demand 钉计数、MMIO 窗口恒空不变量，
    # host 13 测；记账状态机单源，kernel/src/vmo.rs 物理臂消费）。
    cargo test -p berry-vmoledger --target {{host}}
    # W3-Q80：berry-memface（mem* 整数版语义模型——块切分几何/方向
    # 分治/LE 首异定位/广播字 + 三模型对拍（memset/memcpy/memmove/
    # memcmp 块算法 vs 逐字节参考全量一致），host 8 测；kernel mem.rs
    # no_mangle 符号壳消费单源）。
    cargo test -p berry-memface --target {{host}}
    # W3-Q17：berry-vslotface（Vmar 槽位/区间判定律纯函数面——三查判序
    # 对齐→界→重叠错误码优先级/半开重叠谓词/精确查找 kind 收 Mapped|Demand
    # 与 Mapped-only 分流/探针命中闭开域钉板，host 12 测；check_slot/
    # find_exact/find_mapped/demand_index 单源，kernel vmar.rs 壳化消费）。
    cargo test -p berry-vslotface --target {{host}}
    # W3-Q1：berry-iprofile（intent 六档 IntentProfile 映射表纯函数面——
    # ADR #42 第一段「查表键先于消费者落地」的宿主可测面：两表整表执法/
    # 档位序单调性/LC 铁律三件/maintenance 门控/回退保守性/夹紧包络/
    # sanitize 保守门，host 14 测；EppHint 单源 crate）。
    cargo test -p berry-iprofile --target {{host}}
    # W3-Q13：berry-intface（intent 随消息传播机制纯函数面——编码域/
    # snapshot 三态/settle 判序 opt_out→链预算→跳数/每消息重导非单调/
    # 帽拒不动记录三元组/转发链往返与跳数帽终止/伪 acc 链帽独立闸，
    # host 18 测；INTENT_* 常量与 snapshot/settle 判定律单源，sched
    # intent.rs 机械壳 re-export 消费）。
    cargo test -p berry-intface --target {{host}}
    # W3-Q5：berry-piface（PI 捐赠图纯函数面——donor 五拒/配对律/传播三律
    # 钉板，host 18 测；donor_edge/PI_MAX_HOPS 单源，kernel pi.rs 消费）。
    cargo test -p berry-piface --target {{host}}
    # W3-Q12：berry-ringface（runqueue 环游标与扫描序不变量纯函数面）
    cargo test -p berry-ringface --target {{host}}
    # W3-Q10：berry-telface（telemetry 行编码纯函数面——FNV-1a 稳定哈希/
    # log2 桶律/分位秩单源）
    cargo test -p berry-telface --target {{host}}
    # W3-Q7：berry-evface（Event 信号位空间纯函数面——用户位门/置清位律/
    # wait 命中/唤醒资格钉板，host 14 测；判定律单源，kernel event.rs/
    # syscall.rs 消费；位值冻结面仍在 abi_gen）。
    cargo test -p berry-evface --target {{host}}
    # W3-Q8：berry-hdlface（handle 槽位编码纯函数面——位域 roundtrip/
    # 预留位别名拒/gen 发行域/表满门值域钉板，host 8 测；encode/decode
    # 单源，kernel handle.rs re-export 消费）。
    cargo test -p berry-hdlface --target {{host}}
    # W3-Q9：berry-chanface（channel 队列资格判纯函数面——send 参数上限
    # 律/数组内无重复/容量门/READABLE·WRITABLE 派生位键律钉板，host 8 测；
    # 判据单源，kernel syscall.rs/channel.rs 消费。W3-Q49 freeze 模块
    # （mark/clear 资格 + 序守卫）+9。W3-Q95 ring 模块（02 号机会三刀 a
    # 环形机械：不变量/槽映射/Vec 模型对拍〔全序列 + 全相位环绕 + 伪随机
    # 500 步〕/容量门复用律）+6——chanface 23 测。
    cargo test -p berry-chanface --target {{host}}
    # W3-Q6：berry-itally（intent 档位计数/通胀计数遥测纯函数面——档位
    # 直方六槽/保留域收敛/borrowed==Σ借入不变量/tele: 行格式（纯 ASCII、
    # 非零档省略、u64 极值界）钉板，host 11 测；tally_key/inflate/tally_line
    # 单源，kernel telemetry.rs 计数面 + main.rs tele_dump 行装配消费）。
    cargo test -p berry-itally --target {{host}}
    # W3-Q76：berry-billface（billing 域账户槽分派 + dpg 压力窗折卷纯函数
    # 面——槽查找/空槽抢占/表满覆写最小账户三态判序/CAS 竞态收敛/θ_p 恰界/
    # 回绕窗/饱和病态/ktest test_billing·overflow 全周期模型对拍钉板，
    # host 16 测；charge_step/domain_total/fold_step/win_len_of 单源，
    # kernel telemetry.rs 原子表物理臂留壳消费）。
    cargo test -p berry-billface --target {{host}}
    cargo test -p berry-sliceface --target {{host}}
    # W3-Q31（重放）：berry-tickface（timer 滴答账纯函数面——tick→ns
    # 倒数乘-移对预计算 const fn（QEMU (2048,7) 二进精准/24MHz (2667,6)
    # 半 ulp/异常频率 (0,0) 域卫/<244_171Hz 快路弃）+ 快慢双路资格判与合流
    # + S-6 deadline 单调夹取律（跑步机饿死防御）+ slice 预算再装 + LAST_VCT
    # 回绕窗增量，host 24 测；判定律单源，arch/aarch64 timer.rs 机械壳
    # re-export + sched class.rs tick 记账 delta 消费）。
    cargo test -p berry-tickface --target {{host}}
    # W3-Q67：berry-svcface（SVC 入口锁类位集与号→锁类注册表纯函数面
    # ——类位集六常量对 sched HELD_* 位值组装律〔host 可测形，与 kernel
    # main 编译期断言双执法〕/42 号注册表全表往返/未注册号 NONE 默认/
    # NONE 实装号归零律/REAPQ 位只在 ALL 档/敌意 0..=255 全域零 panic
    # 钉板；常量单源迁此，HAL kern_lock.rs re-export 消费）。
    cargo test -p berry-svcface --target {{host}}
    # W3-Q67 执法件：三方对拍（install() 42 行 ↔ svcface REGISTRY ↔ NR
    # 常量域）——reg() 字面量漂移（改 kernel 落类忘同步注册表）文本级
    # 咬合，正负例验证（负例 = 单项类替换即咬出）。
    python3 tools/svc_registry_check.py
    # W3-Q14：berry-emface（EM 表形状执法 + util EWMA 窗口数学纯函数面
    # ——em_shape_ok 六拒面/em_domain_from 装配不变量/极值首尾投影/
    # window_state 翻窗三态判序/ewma_fold 折叠·收敛·死区·钳位/
    # overutil_judgment 80% 边界两侧 + EM 缺席保守回退钉板，host 6 测；
    # 形状与判定律单源，sched em.rs/util.rs 机械壳 re-export 消费）。
    cargo test -p berry-emface --target {{host}}
    # W3-Q19：berry-bandface（band 预算数学纯函数面——配置三态/首期
    # deadline 饱和加/首越线单赢家律全域抽查/翻期追赶环跨多期一步追/
    # 饱和钉 MAX 防死循环/翻期资格门/带成员 deadline 豁免判钉板，
    # host 6 测；判定律单源，sched band.rs 机械壳 re-export 消费）。
    cargo test -p berry-bandface --target {{host}}
    # W3-Q23：berry-utrace（util EWMA 窗口机盒纯面——per-CPU 三数组槽
    # 机械的纯结构投影 UtilSlots{ewma,accum,window_start}：首调/回绕开
    # 窗丢弃既往账/NotDue 零动作/Fold swap 语义折叠清账/边界恰到期·未
    # 到期/饱和钳 1024/越界全操作静默/满载 12 折收敛 815/idle 空窗衰减
    # 死区落底/reset 清场·哨兵复活/per-CPU 隔离/boot 零点哨兵碰撞钉板，
    # host 16 测；判序 window_state + 折叠 ewma_fold 单源调 berry-emface
    # （零第二实现），sched util.rs 原子壳保持现状——本件定位 host 钉板
    # + 真账流文档化基准（接线取舍见 PR）。
    cargo test -p berry-utrace --target {{host}}
    # W3-Q24：berry-dpgface（dpg EL0 槽池认领制纯面——fsc 滤形 DFSC
    # ∈ 4..=7 位域合法集/恰界 3·8 拒 + 空闲槽最低位选择/池满溢出
    # convoy 键路由 + 属主过滤取三态（匹配最低位/错配让位零消费/
    # 全池空）+ 孤儿槽 GC 臂（消费续扫不置 any_armed）+ 溢出有界性
    # N-K 数学钉板，host 20 测；判定律单源，kernel dpg.rs 判定段
    # 消费（原子槽/wake 机械留壳））。
    cargo test -p berry-dpgface --target {{host}}
    # W3-Q27：berry-frzface（sched 冻结面纯函数钉板——by_pinned 族三谓词
    # sleep_ok/rq_ok〔pi 边全拒 + wait_signals 内核派生位拒〔SLEEPQ 侧〕 +
    # svc_mask/held 结构位只对 RQ 在席拒 + HELD_FROZEN/THROTTLE 剥离〕 +
    # by_thread 族两张脸现状钉〔睡眠者查 held 残影照拒/不查派生位/
    # RQ 不剥 bit6·7——与 §A144「双级同修」宣示相抵的差异面单列〕+
    # §A136 环终止先访后判 ring_visit_order 投影〔三元/单元/二元环全访
    # 含尾 + 防呆帽〕+ 区序两族〔pinned 族 SLEEPQ 先/thread 族 RQ 先〕+
    # 幂等置位/清位律，host 12 测；判定律单源，sched.rs 六函数消费）。
    cargo test -p berry-frzface --target {{host}}
    # W3-Q82：berry-balface（on_irq_tick 内嵌 SMP 门纯函数钉板——定时
    # 拉取近空闲联合门〔mine<=1 ∧ theirs>=mine+2，反复迁移防线〕+ 偷取
    # victim 三值合取〔spread ∧ ttbr0!=0 ∧ eligible〕+ 金丝雀时间节流门
    # 〔canary_due——探针族退役刀 2026-11-19 起，心跳/卡扫族随探针退役、
    # soak_crossed 由 canary_due 继任〕，host 15 测；判定律单源，sched.rs
    # 消费）。
    cargo test -p berry-balface --target {{host}}
    # W3-Q36：berry-lkface（lockdep 检查面纯函数——rank 七类全序表/
    # check_acquire 三检判序/check_release 两臂/check_irq_exit 配平判，
    # host 13 测；判定律单源，HAL kern_lock.rs lockdep 面消费。
    # 〔探针族退役刀 2026-11-19：kl_diag_gate/KL_SPIN_BARK_MASK 随
    # spin_bark 族退役除册。#305 入库起未接线 test-host（base 预存缺
    # 口），W3-Q103 随刀补线——「有 host 测零自动执法 = 静默腐烂」判例族。〕
    cargo test -p berry-lkface --target {{host}}

    # W3-Q83：berry-numface（UART 整数格式化族纯面钉板——u64 十进制〔无前
    # 导/0 单位/20 位极值〕+ 固定 16 位小写 hex〔trap dump 形〕+ 8 位大写
    # hex〔PSCI 诊断行形〕+ 两位十进制〔寄存器下标〕+ 键值行/寄存器行装配
    # 序 + 敌意短缓冲安全律 + 数字表冻结锚，host 10 测；装配律单源，HAL
    # uart.rs·trap.rs·psci.rs 四消费点壳化）。
    cargo test -p berry-numface --target {{host}}
    # W3-Q140：berry-bpface（boot-params 纯函数面——cpio newc 定址查找
    # 状态机〔魔数双形/页垫 4096 契约/TRAILER 自终止/checked 防回卷〕+
    # TOML 子集解析〔schema 门/三键值域/未知键宽严分工/行数帽〕host 钉
    # 板 20 测；判定律单源外迁 kernel bootparams.rs，Vmo 物理臂与 eco 注
    # 入留壳）。
    cargo test -p berry-bpface --target {{host}}
    # W3-Q20：berry-polface（policy O(1) 快照查表数学 + SchedAttr 位选律
    # 纯函数面——散列锚点指纹/256 键桶覆盖摊平律/饱和窗全占+窗尽缺席/
    # 空槽早退缺席/桶 255 回绕落位/first-wins 不覆写/窗满拒收不入表/
    # build 点夹紧五点/EPP 两位打包往返+域外位/merge 八位全表+FREEZE_READY
    # 特判+flags=0 回默认钉板，host 14 测；常量族与判定律单源，sched
    # policy.rs / kernel process.rs 机械壳 re-export 消费）。
    cargo test -p berry-polface --target {{host}}
    # W3-Q72：berry-psface（PSCI 版本门/探测路由门/FEATURES 侦察纯面——
    # version 编码域 major/minor 16+16 拆装往返/最小有效值门〔≥1.0 且非
    # 0xFFFFFFFF——SMCCC -1 截断律〕/探测路由三形〔EFI 门→非 EL1 门→EL1
    # 探测，门住形零 HVC〕/FEATURES 三档现实表〔0 过 -1 拒〕/SYSTEM_OFF
    # conduit 活证判〔0.2 起强制函数不查 FEATURES〕钉板，host 9 测；
    # 判定律单源，HAL psci.rs probe/能力门 re-export 消费〔HVC asm 臂/
    # 诊断行/原子缓存留壳〕）。
    cargo test -p berry-psface --target {{host}}
    # W3-Q22：berry-safeck（berry_sched_attr 结构解码+校验纯函数面——
    # size 门/尾零前向兼容律/version 门/reserved0·未知 flags/weight·
    # intent·policy·freeze_clock 域门/policy=3↔BUDGET 联动/RESERVE 与
    # policy=3 互斥/BUDGET 五拒面/admission 3891 Q12 恰过恰拒/DEADLINE
    # 依赖 BUDGET/布局偏移回读/判序 ABI 面钉板，host 19 测；sa_check
    # 单源，kernel syscall.rs sa_parse 薄壳消费（copy_from_user 物理臂
    # + admission 原子读留壳））。
    cargo test -p berry-safeck --target {{host}}
    # W3-Q70：berry-aokface（access_ok 用户缓冲准入判定律纯面——EL0 值域
    # 四门〔len==0 恒过/checked_add 回卷拒/null 页 0x1000 下界/USER_VA_END
    # =2^39 上界〕+ EL1 高半单比较 + 回卷拒〔原判序两路共门保形〕+ 叶 AP
    # 位域读写拒矩阵〔写需 <0b10、读需 ≠0b11〕+ span_pages 页序单源 +
    # window_should_walk 同页跳过窗律〔SAME_PAGE_NONE 哨兵首块必查/
    # 缓存只记最近页〕钉板，host 17 测；
    # 判定律单源，kernel syscall.rs access_ok/access_ok_window 判定段
    # 换消费〔walk_leaf TTBR0 走查 + PT 锁窗物理臂留壳〕）。
    cargo test -p berry-aokface --target {{host}}
    # W3-Q16：berry-hdrface（HDR 延迟直方图形状与分位数学纯函数面——
    # 桶映射全域往返/保守上界严格递增/分位秩单式/Hdr 记账·钳位·reset/
    # 合并读面桶位直和钉板，host 7 测；HDR_* 常量与 bucket_of/bucket_upper/
    # rank_of/Hdr/CpuHdr 单源，sched hdr.rs 机械壳 re-export 消费）。
    cargo test -p berry-hdrface --target {{host}}
    # W3-Q15：berry-hofface（handoff 交接几何纯函数面——probe 对齐/窗界/
    # version 门 + init_segs 同页自相对按 version 分派/段几何回卷拒 +
    # gop 四非零门/fmt≤2 钉板，host 12 测；判定律单源，kernel handoff.rs
    # 判定段换消费，raw 解引用留壳）。
    cargo test -p berry-hofface --target {{host}}
    # W3-Q89：berry-heapface（slab 堆路由算术纯面 + realloc 原位资格
    # ——内核深度进化审计 01 号机会 4 落地：同 slab class/同 buddy 阶
    # 原指针直返〔Vec push 增长 24B→32B/600B→1024B 同 class 热面省
    # 整块搬运〕，跨 class/跨阶/跨臂/slab↔pages 搬移/零大小防御 None，
    # host 12 测〔8 旧 + 4 新：同 class 原位/pages 阶窗/防御与 align/
    # 全域一致性对拍〕；realloc_inplace 单源，kernel heap.rs realloc
    # 覆写消费〔Q39 遗留 test-host 接线随刀补〕）。
    cargo test -p berry-heapface --target {{host}}
    # W3-Q84：berry-dmface（fw_cfg display 模式请求解析纯面——五门判定
    # 链：size 域 (0,32]/UTF-8/trim NUL+ASCII 空白/@ 后缀截断/WxH u32
    # 十进制域，host 6 测；parse_display_mode 单源，kernel main.rs
    # display_mode_req 薄壳消费——fw_cfg find_file/read_to 物理臂留壳）。
    cargo test -p berry-dmface --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-loader --target {{host}}
    # X-5 補：berry-winplan（demand 装载窗规划纯函数面——自 berry-exec 外迁
    # 的 plan_wins/cap 参数化；三色窗完备铺满 + congruent 门 (a')-(e') 各
    # 拒面 + musl 族多 k 布局过门 + 合成 ELF 全链，host 16 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-winplan --target {{host}}
    # argv blob 组装律纯面（自 berry-exec env 块组装段外迁，winplan 同款
    # 纪律）：argv0 截 191 律 + 整参或零参单调截断 + 恰满边界 + wire
    # 钉板，host 11 测（argv 5 + env 六族——export 面刀扩）。
    cargo test --manifest-path user/Cargo.toml -p berry-argv --target {{host}}
    # spawn 几何前置门纯面（自 berry-exec process_create 前置段外迁，winplan
    # 同款纪律）：映幅帽 ELF_MAP_MAX 恰帽边界 + 栈顶 USER_TOP 恰顶/+1 页 +
    # checked 链两溢 + MMIO 三带重叠/相邻/端点进位 + 现役 std/平像/bsh 三
    # 形态，host 10 测。
    cargo test --manifest-path user/Cargo.toml -p berry-spawngeo --target {{host}}
    # mem/str 族字节序语义纯面（自 berry-libc strmem.rs 外迁——本 crate
    # 结构性不可 host test〔crt0 global_asm〕，该面此前零测试；先例族第
    # 7 例）：volatile 拷/填对齐双形态 + memmove 重叠三态 + memcmp 无符
    # 号差 + strncpy pad-to-n + atoi 双饱和（顺修无符号面 debug 回绕炸）
    # + strchr/strrchr c=0 NUL 位共识，host 15 测。
    cargo test --manifest-path user/Cargo.toml -p berry-strmem --target {{host}}
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
    # app-2 刀1：berry-lifecycle host 测（生命周期三口纯函数底座——32B
    # 容器头 v1 编解码 roundtrip/hlen 分派（v0 12B 兼容 + 坏档四型
    # BadFmt）/CRC32（png.rs 镜像·"123456789"→0xCBF43926 + 金件 blob
    # 逐字节快照 + counter 翻转域外钉板）/前缀 12B 布局同位两条/读侧
    # buf 两臂/8B 帧 roundtrip/counter 代持全序，host 17 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-lifecycle --target {{host}}
    cargo test --manifest-path drivers/Cargo.toml -p berry-virtio --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-blkproto --target {{host}}
    # 存储栈 P0-2 刀 1：berry-partition（GPT/MBR 分区表解析纯函数面——
    # MBR 4 槽/0xEE 保护条转 GPT/GPT 头与条目区解码/坏签名与病态域 loud
    # 拒 + display() 分区列表单行格式化〔教学可见性，vblk 手拼形状收口〕，
    # host 18 测；消费 = vblk 多分区暴露（下一刀接线））。
    cargo test --manifest-path user/Cargo.toml -p berry-partition --target {{host}}
    # 驱动子系统研究报告 P1 框架件 + 波 2.2 先行刀：berry-driver（统一
    # Driver trait 生命周期框架——probe/start/stop 三口可循环 + registry
    # 全槽探测失败不炸，借 Linux probe 生命周期形状；virtio_example 钉
    # berry-virtio 包装法〔鉴定值单源引常量 + MMIO 读写经 berry-hal
    # VirtioMmio，vprobe 同款接线〕，消费面 vblk/vnet 接线后续刀，
    # host 7 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-driver --target {{host}}
    # 波 2 架构件：berry-hal（统一硬件抽象层——Mmio/IrqCtl/DmaAlloc/Clock
    # 四窄 trait + HalError 统一错误面；virtio_impl = berry-virtio 偏移单源
    # 的 Mmio 实现（volatile 包装为安全接口 + 窗界检值不炸）；BumpDmaAlloc/
    # VirtualClock 参考实现消费 alloc 面，真消费面 vblk/vnet 接线后续刀，
    # host 8 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-hal --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-rtc --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-watchdog --target {{host}}
    # 刀 3 B1-a：berry-gpuproto（virtio-gpu 2D 协议纯函数面——九命令/头/
    # 响应/event/config 编解码 + uapi 常量两源钉板，02 号 §3.1/§3.2；
    # 传输接线归 B1-c）。
    cargo test --manifest-path user/Cargo.toml -p berry-gpuproto --target {{host}}
    # mkfat 换轨二刀：fixture 守卫切 Rust 件（先编后用——cargo-target 钉位
    # 纪律同 elf2flat/mkcpio；oracle 对拍腿在 tools/mkfat cargo test 行内，
    # Python 侧漂移同被咬）。守卫语义钉板：改 mkfat 忘重产 fixture 即红。
    CARGO_TARGET_DIR="$PWD/tools/mkfat/cargo-target" cargo build --release --manifest-path tools/mkfat/Cargo.toml --target {{host}}
    "$PWD/tools/mkfat/cargo-target/{{host}}/release/mkfat" --check-fixture user/berry-fatfs/tests/volume_fixture.bin
    # V-2：FAT16 双宽基准同位守卫（--check-fixture16——一卷一宽双 fixture，
    # 漂移即 FAIL 带 --fixture16 重产提示）。
    "$PWD/tools/mkfat/cargo-target/{{host}}/release/mkfat" --check-fixture16 user/berry-fatfs/tests/volume_fixture_fat16.bin
    cargo test --manifest-path user/Cargo.toml -p berry-fatfs --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-netproto --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-netdproto --target {{host}}
    # sim-S1：berry-pwproto（sim 三设备快照协议纯函数面——帧头/定长快照/
    # ACK/流式再同步解码 + 值域双向执法 + 黄金字节钉板，host 9 测；
    # PM-16 §4.1 契约冻结）。
    cargo test --manifest-path user/Cargo.toml -p berry-pwproto --target {{host}}
    # CP-08 接线刀（04 号复核 F3/§6-13 收口）：PM 四 crate host 测试行——
    # berry-power(32)/berry-thermal(13)/berry-em(8)/berry-slackface(12)
    # 全是独立小工作区（空 [workspace] escape 根/父 workspace，不入
    # user/Cargo.toml members），--manifest-path 直指 crate（berry-ksproto
    # 先例形态）。berry-power/berry-thermal bin 挂 wired required-features
    # 门（musl 产线面——build-user PM 段开 --features wired 落链；host
    # 测试相整跳 bin，聚合/仲裁纯核全在 lib）。berry-power 36 = 31 +
    # CP-08 刀③ util 快照载荷面 1 测（ingest_snapshot loopback/拒面）+
    # EAS-PA 刀 placement hint 产面律 1 测（低核偏好/平局/通道同律）+
    # POLICY4 刀（v3 四动作激活）hint 产面 3 测（weight 三带+包络自律/
    # EPP 热档联动+位自律/四动作联合一致+EM 缺席零污染）。
    cargo test --manifest-path user/bins/berry-power/Cargo.toml --target {{host}}
    cargo test --manifest-path user/bins/berry-thermal/Cargo.toml --target {{host}}
    cargo test --manifest-path user/berry-em/Cargo.toml --target {{host}}
    cargo test --manifest-path user/berry-slackface/Cargo.toml --target {{host}}
    # pm-integration（CP-08 联合场景 harness，host 14 测）：PM 四 crate 按
    # ADR #66/#68 数据流接成三个端到端场景（热降频全链/slack+EM 联合/
    # 充电票+关机边界）——只读 path 依赖零改被测件，验证「上游出→下游
    # 收」跨 crate 正确性；host 验证面 harness 非产线服务，不进 rootfs。
    cargo test --manifest-path user/pm-integration/Cargo.toml --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-tcp --target {{host}}
    # C-3：berry-psproto（ps 查询 RPC 协议纯函数面——表/ver/mem 三 op
    # 编解码 + 严格解析全拒面 + 黄金字节钉板，host 15 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-psproto --target {{host}}
    # C-4：berry-complete（bsh Tab 补全纯函数面——token 切分四形/前缀
    # 匹配唯一·多候选·零命中/排序去重/列示字节，host 10 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-complete --target {{host}}
    # T7（终端刀 2026-11-17）：berry-termio（berry-term 哑终端显示核纯面
    # ——行环/尾段/回显字节解释器/软换行/视图计算，host 19 测＝lib 17 +
    # pipe_selftest 流仿真 2）。
    cargo test --manifest-path user/Cargo.toml -p berry-termio --target {{host}}
    # A2-1：berry-poll（多路等待聚合器纯函数面——wait_many n≤16 翻越：
    # 批切分 + poll-first 收割到干 + 分段阻塞轮转 + LT/非阻塞/唤醒三免费
    # 语义，host 13 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-poll --target {{host}}
    # 作业控制 v2 刀：berry-jobs 纯面（alloc_id/parse_job_ref/渲染行——
    # host 14 测；strip_bg 自 v2.5 引号刀起 Superseded 保留导出——消费面
    # 已整迁 berry-lex、边缘测随面同留；2026-11-19 愈合复通——活线合并曾
    # 丢此线，随 free 刀同批复接）。
    cargo test --manifest-path user/Cargo.toml -p berry-jobs --target {{host}}
    # bsh v2.5 引号与统一分词器刀：berry-lex 词法单源（单遍 Item 流/三视图
    # render·fields·word_texts/strip_bg 引号感知版——host 72 测：引号三件
    # 全分支＋render 对拍 expand 恒等律＋恒等金样族；bsh 解析层词边界/操作符
    # 识别全经此单源，berry-jobs strip_bg 语料整迁退役——Superseded 保留
    # 导出，berry-jobs 活跃面归纯作业数据面）。
    cargo test --manifest-path user/Cargo.toml -p berry-lex --target {{host}}
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
    # 波3-件⑦：berry-appshell（应用壳 crate——launcher 五模块纯抽取的
    # host 纯函数面：rpc 重试分类/conf 读写谓词/scene 档位-scrim-组宽/
    # icons 导航几何-pump damage 包络-等待帽；syscall 半程 cfg(not(test))
    # 门（berry-fsclient 先例）——首消费者 berry-launcher musl 链由
    # build-user 段覆盖）。
    cargo test --manifest-path user/Cargo.toml -p berry-appshell --target {{host}}
    # GUI-4 段②：guiproto 协议面 + gui-inputd 泵/服务单测入 host 台账。
    cargo test --manifest-path user/Cargo.toml -p berry-guiproto --target {{host}}
    cargo test --manifest-path drivers/Cargo.toml -p gui-inputd --target {{host}}
    # 波3（六波方案步骤 4）：gui-wmd 策略核（lib 双面 crate——窗口表/
    # 焦点维护/OP_WM_BACK 500ms 兜底/事件通道广播状态机，MockPort mock
    # 客户端喂真 wire 帧；gui-inputd 同法 host 台账，musl 面走 build-user）。
    cargo test --manifest-path user/bins/gui-wmd/Cargo.toml --target {{host}}
    # app-0：berry-manifest（.bap manifest v0 纯函数解析——「解析器即产品」
    # 第 8 例，host 44 测：完整钉板/缺省合并/坏输入族 + 恰满边界正例/真
    # manifest fixture 共识/intent 判别值钉板 + ID_MAX 绝对钉（berry-layout
    # 跨面镜像对偶侧）+ ADR #82 V1 AppType 词表 {app,driver} 钉板）。
    cargo test --manifest-path user/Cargo.toml -p berry-manifest --target {{host}}
    # GUI-8：berry-layout（桌面布局 JSON v1 纯函数解析/发射——「解析器即
    # 产品」第 9 例，host 61 测：schema/canonical 钉板 + 帽双侧执法 +
    # 负例全家桶（parse 侧 39 逐条独立 + emit 侧收口 5，at 位点逐条钉死））。
    cargo test --manifest-path user/Cargo.toml -p berry-layout --target {{host}}
    # GUI-8 段①：berry-appproto（app 域 RPC 协议纯函数面——APP_LIST
    # 分片流/APP_SPAWN 编解码 + idx 单调 +1（乱序/重放/跳帧拒，0c 钉
    # v1 冻结）/blob 收支/END 对账三执法 + 帧帽 256 编译期断言 + 黄金
    # 字节钉板 + recv 复用缓冲脏尾回归钉，host 22 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-appproto --target {{host}}
    # cbox 刀 2：berry-cbox（黑匣子账本纯函数面——排液快照解码/行编解码
    # 冻结字段序/合并轮转 + boot_pick 续号；黄金行钉板 + 负例全家桶 +
    # 轮转年龄序钉死，.agents/plans/cbox/design.md）。
    cargo test --manifest-path user/Cargo.toml -p berry-cbox --target {{host}}
    # cbox 刀 3：berry CLI 工具面（symbolize ELF64 零依赖解析 + crash-log
    # FAT12 直读/指纹聚合——对真 mkfat --put 造盘对拍 + 撕链负例；userbins
    # 现实全 strip，双空如实报错指路不静默）。
    cargo test --manifest-path tools/berry/Cargo.toml --target {{host}}
    # SEC-02 刀 A2：berry-ksproto（keystored 密钥服务 RPC 协议纯函数面——
    # KeyMint 借号段 tag 编号表 add-only 冻结钉板 + AuthorizationList 软硬
    # 双域/SecurityLevel 三档 wire + KEY_GENERATE/KEY_ATTEST/COUNTER 三 op
    # 编解码黄金字节 + 严格串行会话机，host 20 测；独立小工作区绕开
    # user/Cargo.toml——manifest-path 直指 crate，同 stdhello/bsh 先例）。
    cargo test --manifest-path user/berry-ksproto/Cargo.toml --target {{host}}
    cargo test --manifest-path rootface/Cargo.toml --target {{host}}
    # W1 接线（D-审计 §4 同款零接线面）：berry-softime（波3 件 C 软键盘
    # 引擎 v1——QWERTY-lite ASCII 键位模型 + OP_INPUT_INJECT 载荷编码 +
    # 显隐状态机，host 5 测落地时未接线 test-host；SERVICE 底窗本体随波 3
    # 分支并主接线，此 crate 零改动吸收——接线不依赖底窗面；berry-appshell
    # 接线去重——#356 已带 workspace 形 -p berry-appshell 行（上方波3-件⑦
    # 段），manifest-path 重复行移除（appshell 33 测曾双跑，实测去重）。
    cargo test --manifest-path user/berry-softime/Cargo.toml --target {{host}}
    # ai-runtime §二 Tier 0 首刀：berry-tensor（量化算子核纯函数面——
    # Q4_0/Q8_0 块布局单源〔llama.cpp 兼容字节形〕+ f16 位级解码含次
    # 正规/去量化/量化×量化整数内积点积〔块内 i32、scale 后乘、f32 顺
    # 序累积——Tier 0 确定性基线，SIMD/卸载后端对拍锚〕+ 第二片
    # matvec_q4_q8〔GEMV 行基元〕/rmsnorm〔inv_sqrt 位黑客种子 + 3 轮牛
    # 顿纯 core〕，host 16 测；
    # 消费 = airtd/berry-aiproto 张量段〔后续刀〕）。
    cargo test --manifest-path user/Cargo.toml -p berry-tensor --target {{host}}
    # ai-runtime §三.1 首刀：berry-kvpool（块级分页 KV-Cache 池语义——
    # 16-Token KvBlock 定容池 + 前缀 Copy-on-Write 共享〔引用计数 +1 物
    # 理恒一份〕+ 共享尾块禁原地写执法〔满尾滚新块不属 COW 面——fork
    # 续写主路径〕+ 显式回收口，host 9 测；真消费
    # = airtd Paged KV-Cache 运行时〔后续刀〕）。
    cargo test --manifest-path user/Cargo.toml -p berry-kvpool --target {{host}}
    # ai-runtime §四.1 首刀：berry-vqvec（vault 定点量化向量基元——每
    # vec 单 scale 的 i8 标量量化〔round-half-away 手写〕+ 非对称整数
    # 域距离〔内积/L2²〕，全零 vec 退化无除零，host 8 测；HNSW 图与
    # FBE/VaultQueryCap 门控归 berry-vault 服务刀〔后续〕）。
    cargo test --manifest-path user/Cargo.toml -p berry-vqvec --target {{host}}
    # ai-runtime §四.1 第二片：berry-hnsw（HNSW 图骨架——splitmix 确定性
    # 分层指派〔位级 log16〕+ 贪心下降 + 层 0 全展开束搜〔朴素 ef 形〕
    # 距离闭包注入与量化距离解耦，tie-break 节点号升序确定性检索，
    # host 6 测；ef-construction 束建与多样性剪枝随后续刀）。
    cargo test --manifest-path user/Cargo.toml -p berry-hnsw --target {{host}}
    # ai-runtime §三.2 首刀：berry-jsonmask（JSON 字节级约束解码掩蔽器
    # ——单源转移函数 step + 256 字节探测派生 mask〔合法集永不双写〕+
    # 拒绝不改态〔100% 合法输出机制〕+ can_finish 流式终判〔裸数字态〕，
    # R12 深度 N〔显式容器栈 ≤16 层越帽拒〕，host 10 测；Schema 编译随后续刀）。
    cargo test --manifest-path user/Cargo.toml -p berry-jsonmask --target {{host}}
    # am §一首刀：berry-amgrad（终态五级弹性处置梯度决策核——五级表
    # 单源 choose〔RAM 谱系 L0<L2<L1<L3 升格 + L4 磁盘正交〕、租约豁
    # 免压制冰封与深化、防抖窗双钉值 3s/0s，无触发即 None 禁默认动作，
    # host 8 测；执行机构归内核机制与 appd 接线刀）。
    cargo test --manifest-path user/Cargo.toml -p berry-amgrad --target {{host}}
    # am §二.2 首刀：berry-wakeflood（唤醒风暴熔断核——每客户端分钟
    # 桶计数〔环形零分配〕+ 12 次/分钟钉值熔断 + 冷却窗释放与再熔断
    # 环〔释放纯时间驱动〕，host 7 测；计时器/退避投递/耗电台账归
    # appd 与 power-thermal 接线刀）。
    cargo test --manifest-path user/Cargo.toml -p berry-wakeflood --target {{host}}
    # ai-runtime §二 Tier 1 客机侧首刀：berry-npuproto（virtio-compute
    # wire 编解码——config/命令描述符〔virtio-gpu 控制头同形〕/blob 页
    # 帧表〔off 4096 对齐执法〕/完成环三态〔OK/Unsupported 降级依据/
    # Watchdog 看门狗〕，畸形即 None 不截断，op 值镜像 berry-tensor，
    # host 7 测；宿主 Metal 桥归 tools/berry 边界面）。
    cargo test --manifest-path user/Cargo.toml -p berry-npuproto --target {{host}}
    # mkapp 门禁自检（a-3 U4：mkfat --check-fixture 同位前置——打包器门禁
    # 面此前只在 build-user 产物链上被动覆盖，门禁自身坏 = 产物链静默劣化）。
    python3 tools/mkapp.py --selftest
    # W1（彻底 Rust 化轨道 b 层 #4 刀序第四位首刀）：mkapp Rust 件 lib 纯面
    # 测试（canonical 规范形重发射——Python 现役件 M.canonical() 13 形黄金
    # 向量字节级对拍：v0 旧形/词表/可选节组合/尾增节全谱/kitchen sink +
    # 幂等律 + min_level 孤立不发射钉板 + 常量镜像对账）。独立小工作区，
    # 须显式 --target {{host}}。产链现役件仍 Python mkapp.py（下方
    # --selftest 行同挂），后续刀（check_manifest_text 门禁/BYI1/信封/
    # 签名）依彻底Rust化计划 §4 刀序推进后再换轨。
    cargo test --manifest-path tools/mkapp/Cargo.toml --target {{host}}
    # W1（同上五刀·产链换轨后置注）：mkapp cargo test 现含 build 编排面
    # 镜像测试 + tests/build_parity.rs oracle 对拍（三组真实 staging
    # Rust↔Python 产物 sha256 恒等 + 摘要行全等 + --check 交叉复验 +
    # 拒面同文案）——产链现役件已切本 bin（build-user 四处打包位），
    # Python mkapp.py = 模块面 + oracle（--selftest 行同挂，语义钉板）。
    # W1（同上六刀·--check）：tests/check_parity.rs——check_bap 五步
    # Rust↔Python oracle 对拍（OK 行全等 + 拒面十族同 code）；CLI --check
    # 臂实装（出口码同 Python），Python --check 仍为 CLI oracle 面。
    # W1（同上七刀·--json）：src/json.rs 信封面 + tests/json_parity.rs
    # oracle 对拍（正例双 op result 舱逐键全等 + 负例六类钉板族七舱全等
    # + io 兜底 + 单行纪律 + 双读一致）；CLI --json 臂实装（build/check
    # 两 op，拒因 exit 1/用法错 2 同 Python），其余形态仍走 Python 面。
    # W1（同上八刀·--new）：src/new.rs 脚手架面 + tests/new_parity.rs
    # oracle 对拍；CLI --new 臂实装（恰四参形同判）。
    # W1（同上九刀·签名面）：src/sign.rs（_find_berry_sign 发现序镜像 +
    # sign/verify 子进程封装）+ tests/sign_parity.rs；CLI --sign-key/
    # --verify-key 旗标提取。
    # W1（彻底 Rust 化轨道 b 层 #5 刀序第五位首刀）：gensourceindex
    # Rust 件 lib 纯面测试（TLV 索引编码/解析 canonical 单形 + 源钥清单
    # 冻结行格式 + 自持 SHA-512/256——Python 现役件 build_index/
    # parse_index/build_keys_listing 五组黄金形字节恒等对拍 + 拒面文案
    # 全等 + NIST 向量钉板）。二刀（berry-sign 面）：src/sign.rs 发现序/
    # 子进程封装/sign_index 单点缝/verify_index_signature 三态 +
    # tests/sign_parity.rs oracle 真流对拍（签名字节恒等 + 验签三态
    # 双侧同构 + unsigned 降级注全等；berry-sign 缺席 skip 纯面兜底）。
    # 独立小工作区，须显式 --target {{host}}。
    # 三刀（CLI 消费半边）：src/main.rs --verify/--gen-keys 臂实装
    # （Python do_verify/do_gen_keys 镜像）+ tests/cli_parity.rs oracle
    # 真流对拍（--verify stdout 逐行全等/单调门/两件套对拍拒/gen-keys
    # 真流/用法错族）。四刀（生成面库刀）：src/gen.rs rootfs-manifest 路
    # （load_app_entries/build_entry/do_generate——依赖全走已 Rust 化件：
    # mkcpio load_manifest 表加载 + mkapp gate/cpio manifest 语义与走查
    # 单源复用）+ tests/gen_parity.rs oracle 真流对拍（两件套字节恒等 +
    # stdout 逐行全等 + 单调门/[security]/pk 长度拒面同构；--preset 面
    # berry_assemble Python 现役不镜像）。五刀（生成面 CLI 换轨）：
    # src/main.rs 生成臂全旗标集（argparse 判序镜像：--verify 独占/
    # --sign-key 同席门/生成双必选 + 让位面 --preset/--selftest 指回
    # python3）+ tests/gen_cli_parity.rs CLI 级对拍（两件套字节恒等含
    # --sign-key 真签腿〔BERRY_SIGN env 合同位注入〕+ stdout 逐行全等 +
    # 拒面/用法错族 exit 码与 stderr 全等）。sign-user 产链位已切本
    # bin（先编后用 + cargo-target 钉位）；Python 件不退役——CLI 对拍
    # oracle + berry_assemble import 面单源（--preset 让位）。
    cargo test --manifest-path tools/gensourceindex/Cargo.toml --target {{host}}
    # W1（彻底 Rust 化轨道 b 层 #6 刀序第六位首刀）：berryassemble Rust 件
    # lib 纯面测试——boot-params 词表执法+序列化（三键 v1/u32 恰界/intent
    # 六档/越帽 32/schema=1 头+字典序键值行黄金形）/ kernel 锚形（name@
    # sha256:<64hex> + 大写归一 + 拒梯）/ 装配反回滚门（u32 域界+PR≤level
    # 两臂）/ platform level 单源读器。tests/parity.rs Python 现役件真跑
    # oracle 对拍（serialize 字节恒等 + validate_platform_revision 拒面
    # 文案全等 + ANCHOR_RE accept/拒梯 + abi.toml 真源同读 + 坏源分支）。
    # 二刀第一段：toml_doc 解析器（product.toml 语面——嵌套节/多行数组/
    # 行尾注释/引号键/重复键与表互覆拒；空节在场性经 parse_with_sections
    # 清单保留——tomllib「节头出现 = table 在场」语义钉平）+ tomllib
    # flatten oracle 对拍（acme 参照物 + SELFTEST 模板 + 合成语料逐行全等）。
    # 二刀第二段第一片：lite.rs load_product(lite=True) 结构面镜像——
    # 顶层键/schema=1/[product] 全节键集类型值域执法；parity oracle 真跑
    # 文件面（42 拒面文案逐字对拍 + acme/SELFTEST 双正面过检视图对账）。
    # 二刀第二段第二片：[platform] 全节执法（check_platform_section——
    # 六键必填/三枚举词表/sideload bool+user 型收窄/boot 锚+params 词表
    # 单源复用/storage 三键域界+atomicity 收窄；params/storage 域界执法
    # 经 to_lib_val 转换桥复用 lib check_boot_params/need_u32 单源——零
    # 第二套词表/域界文案；parity 21 拒面语料单点变异对拍逐字全等）。
    # 二刀第三段：[product.defaults.locale] i18n 八键执法（languages
    # BCP 47 手写段匹配器+1..=8 帽+大小写不敏感查重/timezone IANA 段形/
    # region·regulatory_domain ISO 3166-1 alpha-2/calendar 等四枚举词表；
    # parity 20 拒面语料 + BCP47 accept/拒梯分支对齐 + 八键全席正面）。
    # 三刀·CLI 消费半边：src/main.rs --check-lite 臂换轨（Python main()
    # 判序逐行镜像：旗标循环/互斥单模式/out-dir 值对/用法错 2/拒因 1；
    # do_check(lite=True) OK 行逐字同构含 preset 计数与 locale·trust
    # 开关位 + OSError 读失败文案镜像）+ tests/cli_parity.rs oracle 真跑
    # 对拍（stdout 逐行全等剥路径段 + exit 码/stderr 全等 + 用法错族 +
    # 让位臂五旗标指回 python3——--check/--dry-run/--emit/--selftest/
    # --sample 现役单源不镜像）。
    # 四刀第一片：full.rs 产物面对账（load_product(lite=False) 文件面四臂
    # 镜像——override OEM 件/branding 资产在席 + source 两件套 + trust 锚
    # 32B 恰长 + gensourceindex parse_index path 依赖单解释器复用）；
    # tests/full_parity.rs oracle 真跑文件面对拍（gsi 两件套真产 + 7 拒腿
    # 变异文案逐字全等 + 结构拒面前置）。
    # 四刀第三片：anchor.rs kernel 锚现算对账（measure_kernel_anchor 三态
    # 镜像——件缺=Ok(false) 只形校验过 / 件在且等=true 现算对账过 / 件在
    # 不等=loud 拒「锚对账不等」；SHA-256 = mkapp cpio::sha256_hex 单解释
    # 器 path 依赖）；tests/anchor_parity.rs oracle 真跑对拍（三态双侧
    # 同构 + 拒面文案逐字全等 + 5MiB 大件分块读腿——摘要经独立 hashlib
    # 管道自算防同错对拍）。
    # 五刀收口：validate.rs --check 全量臂组合面（_validate_all :985 镜像
    # ——check_full → resolve_entries → measure_kernel_anchor 聚合 + OK
    # 行六段 format_check_ok :1012 逐字镜像）+ main.rs --check 臂换轨
    # （读面 read_product_text 抽单源 + tools/ 目录发现 repo_tools_dir
    # 〔gsi 同款 CWD/exe 双上溯——DEFAULT_BASE/DEFAULT_USER_DIR 等价物〕）
    # + tests/validate_parity.rs CLI oracle 对拍（读失败/schema=0 结构拒/
    # override 文件面拒/真源基座拒〔双侧同咬仓内 rootfs-manifest 构建树
    # 缺席〕/坏 TOML 分支标识/用法错 2 六腿；OK 行正面 = lib 黄金形 +
    # Python do_check 同现场模块级逐字对拍钉板）；cli_parity 让位臂族
    # 滚新（--check 移出让位族）。
    # 六刀：plan.rs --dry-run 计划面（Python build_plan :890 + do_dry_run
    # :1020 逐字镜像——行面全分支 + 装配门两臂前置 + relpath user_disp）；
    # main.rs --dry-run 臂换轨（pr 旗标解析收值 + abi.toml 单源读 + 组合
    # 面聚合 + 逐行打印）+ tests/plan_parity.rs oracle 模块级对拍（合成
    # 现场 7 腿行面逐行全等：全席基线/sideload 形/最小面/user_disp 变/
    # params 双形/source 透传〔gsi 两件套真产〕/kernel_measured 真件态）
    # + validate_parity CLI 腿补（真源基座拒/装配门域界与反回滚两臂/
    # 用法错与坏 pr 值）；cli_parity 让位臂族滚新（--dry-run 移出让位族）。
    # 七刀：plan.rs emit_boot_params 产件面（Python do_emit :1046 尾段
    # 镜像——空 params 零产件零目录缺省零扰动行 / 非空 makedirs 实写
    # <out>/etc/boot-params.toml + 产件行；params 串面存储 → 序列化键值
    # 形转换 params_paramvals〔u32 parse 十进制串 + intent 经 INTENTS 找
    # 回 static〕）；main.rs --emit 臂换轨（dry_run_core 共核拆分——
    # 计划行打印 + emit 尾段；缺 --out-dir 判序前置不变）+ tests/
    # emit_parity.rs oracle 模块级对拍（合成现场 5 腿：全席三键 params
    # 产件字节 sha256 恒等 + 尾行剥段全等 / 空 params 零扰动双侧同构 /
    # 深位 out-dir makedirs 递归 / 结构拒面前置 / IO 失败 out-dir 被占位
    # 同构）+ validate_parity CLI 腿⑩（--emit 真源基座拒——双侧同咬仓内
    # 真基座表，拒面剥仓根段逐字全等 + 拒路径零产件）；cli_parity 让位
    # 臂族滚新（--emit 移出让位族——七刀后五模式全镜像，让位面余
    # --selftest/--sample）。独立小工作区，须显式 --target {{host}}。
    # Python 件不退役——CLI 对拍 oracle + gensourceindex --preset
    # import 面单源。
    cargo test --manifest-path tools/berryassemble/Cargo.toml --target {{host}}
    # W1（彻底 Rust 化轨道 b 层 #7 刀序末档首刀）：bctsrun Rust 件——Tier 1
    # 冻结件 schema 校验器镜像（现役单源 = tools/bcts/t1_check.py〔bcts-run.sh
    # PYEOF①段机械抽出——heredoc→模块零行为变更〕）。lib：冻结件 TOML 子集
    # 解析（顶层键 + [[marker]]/[[host_gate]] 数组节；点分节/数组值/内联表/
    # hex/日期非目标形拒）+ 顶层八键执法 + marker/gate 值域全谱 + 空集守卫 +
    # 文件名冗余自证 + META/MARK/GATE TSV 发射（拒面文案与 Python 逐字同源
    # ——py_repr/True·False/单引号 list repr 形）。parity：仓内 bcts@0..6 七
    # 件全量 TSV 逐行对拍 + 20 变异拒面逐字对拍 + TOML 语法面六语料双侧同拒
    # （语法域文案各自实现——分支标识对齐）。二刀：src/t2.rs（T2 冻结件
    # schema 校验镜像 = tools/bcts/t2_check.py）+ parity T2 三面（两冻结件
    # 全量 + 26 变异 + 语法面八语料）。三刀：src/reconcile.rs（T2 静态对账
    # 镜像 = tools/bcts/reconcile_t2.py——TSV 消费 + OP_* 常量扫描对账 + 五类
    # 覆盖行；parity 真源两冻结件 stdout 逐字对拍 + 变异族 8 腿 + 扫描器边界
    # （崩溃面诚实处理分歧注记）。四刀：src/abipr.rs（PYEOF④⑤镜像 =
    # tools/bcts/abi_level.py + img_pr_check.py——[platform].level 读器〔值域
    # 拒面 repr 同源：串/bool/float/None + OS Errno py repr 形〕+ manifest v1
    # platform_revision 对账〔`item .* platform_revision=(\d{10})$` 贪心最右
    # 位 + universal newlines 归一 + set 去重 sorted〕）；parity abipr 三面
    # （真源 abi.toml stdout 全等 + 值域/Errno/坏 TOML 变异族 + 正则边界族
    # 〔贪心/紧贴/双空格/CRLF/11 位回溯失败〕）。五刀：src/live.rs（活树清单
    # 合成镜像 = bcts-run.sh build_live_tsv heredoc 体——markers.sh 三数组
    # + 尾随 GATE 恰一行；id 相内序 %02d；语面 = bash 数组赋量子集〔引号
    # 元素/续行/体内注释/后写覆盖〕，非目标形〔非引号元素/转义/展开 glob〕
    # 诚实拒；守卫序镜像真文件 && 链〔缺席 → unbound 崩文案 + 行号 = 守卫
    # 行、空 → 空集守卫文案、短路序精确〕；bash 实测分歧注记钉板：unbound
    # 在 && 链内不杀非交互壳 + unset 数组 "${X[@]}" bash 4.4+ 静默空扩 =
    # 假绿面（真 bcts-run.sh 靠步② floor 91 兜底），镜像同形严拒早于发射〕
    # + parity live 三面（真源 markers.sh 92 行 stdout 逐字全等 + 计数
    # 32/56/3/91 钉板 + 绿语料族 + 拒面族〔空集守卫文案逐字/unbound 分支
    # + 短路序/合成语料〕）。六刀候选：CLI 参数面镜像。独立小工作区，须
    # 显式 --target {{host}}。
    # Python 件不退役——CLI 对拍 oracle。
    cargo test --manifest-path tools/bctsrun/Cargo.toml --target {{host}}
    # W1 接线（SEC-02 A2 同款纪律）：berry_assemble 门禁自检——V13 组装器
    # （ADR #82「组装不 fork」product.toml 校验/dry-run 计划/装配反回滚门）
    # 落地时自称 51 腿绿但从未接线 test-host：有 selftest 零自动执法 =
    # 门禁自身坏静默劣化（ksproto 未接线判例同族）。改 berry_assemble.py /
    # rootfs-manifest.toml / abi.toml 平台 level 联动面必跑。
    python3 tools/berry_assemble.py --selftest
    # V-0a 刀 1：berry-efi（EFI 结构最小面——偏移/尺寸/签名/UTF-16 编码
    # host 钉板）+ elf2pe 自检（合成 ELF 全链转换钉板 + 未知重定位型负样本）
    # + mkfat --selftest（--put 引导盘构造面：链回读/嵌套目录/负样本——此前
    # 只手跑，门禁化同 a-3 U4 纪律）。
    cargo test --manifest-path bootloader/berryboot/Cargo.toml -p berry-efi --target {{host}}
    # V-0a 刀 3：berryboot app 单源共识——内嵌 HELLO.TXT 基准 ↔ 仓库 fixture
    # hello.txt 逐字节对拍（mkfat fixture 共识基准同族，改一侧忘同步即炸）。
    cargo test --manifest-path bootloader/berryboot/Cargo.toml -p berryboot --target {{host}}
    # G0 刀 1：berry-slot（A/B 槽位仲裁纯面——CRC 向量/双副本/仲裁三律/
    # 三靴崩溃走查 host 钉板）。
    cargo test --manifest-path bootloader/berryboot/Cargo.toml -p berry-slot --target {{host}}
    # G1 刀 5：berry-sbom 协议白名单执法接线（zero dependency=PASS 面）。
    cargo run --manifest-path tools/sbom/Cargo.toml --target {{host}} -- --check
    # V-5a-0：berrydump（Fusion/AAVMF 设备面事实 dump 纯取证工具——解析
    # 纯函数面 host 钉板：RSDP/XSDT/MADT/SPCR/DBG2/FDT 六表面合成向量 +
    # 表名注册，host 8 测；独立 workspace 与 berryboot 主引导链解耦，
    # 唯一依赖 berry-efi 纯数据 crate）。
    cargo test --manifest-path bootloader/berrydump/Cargo.toml --target {{host}}
    # Q1（a-3 U4 纪律收口，elf2flat 换轨二刀后双形态）：elf2flat 门禁——
    # 产链现役件 = Rust（下行 cargo test，tools/elf2flat）；Python 版已降格
    # elf2flat_oracle.py（对拍取证专用，§5 第 1/5 步——转换核逐字保留，
    # selftest 照跑守语义钉板，Rust 件有疑时拿本件复算对拍；不降豁免表
    # ——selftest_guard 零豁免契约不动，接线即纪律）。合成双段 ELF（gap
    # 零填 + bss 尾零 + 页取整 + 确定性）+ 8 拒面（elf2pe 同位纪律）。
    python3 tools/elf2flat_oracle.py --selftest
    # W1 接线（彻底 Rust 化轨道 b 层 #8）：elf2flat Rust 件 lib 纯面
    # 测试（双段钉板 + 8 拒面 + Rust 侧短输入面 + synth 模式重导——与
    # Python oracle 字节级对拍见 docs/彻底Rust化计划-v1.0.md §5 五步；首刀
    # 落位第 2/3/4 步，二刀（本刀）落位第 5 步：build-user 产链六处
    # python3 → rust bin 换轨，oracle 降格；对拍背书 = 六真实 ELF
    # sha256 6/6 恒等 + stdout 全等）。
    # 独立小工作区（tools/berry 同款形态），根 .cargo/config.toml 锁
    # build.target=aarch64-unknown-none 故必须显式 --target {{host}}。
    cargo test --manifest-path tools/elf2flat/Cargo.toml --target {{host}}
    # W1 三刀（彻底 Rust 化轨道 b 层 #8）：elf2pe Rust 件 lib 纯面测试
    # （合成 ELF 全链钉板 + §A90 r_offset 槽位律回归 + 拒面 + 真实
    # berryboot.elf 对 oracle 字节级对拍〔在位时〕——elf2flat 同位纪律；
    # 产链已换轨（elf2flat 二刀同款）：build-boot 走 Rust bin，Python 版
    # 降格 elf2pe_oracle.py 对拍取证专用）。
    cargo test --manifest-path tools/elf2pe/Cargo.toml --target {{host}}
    # Q1 续挖（a-3 U4 纪律同款）：pwrcls 门禁自检——smoke B4 掉电相三 boot
    # 的采样断言（A/B 档分类 + Boot C 后必 clean）与毒环注入都吃本件，此前
    # 只在 smoke 产物链上被动覆盖，门禁自身坏 = 掉电相静默劣化。
    # 合成平盘 mkfat import 单编码器同形 + classify 七态钉板 + 双 FAT
    # 注入对账 + 5 拒面（elf2flat --selftest 同位纪律）。
    python3 tools/pwrcls.py --selftest
    # Q1 再续（a-3 U4 纪律同款）：siminj 门禁自检——smoke SIMBAT 注入相
    # （sim-S1 M3 四矢 + sim-S2 A8 九矢毒环）的宿主 client，此前只在 smoke
    # 注入链上被动覆盖，门禁自身坏 = SIMBAT 相静默劣化成 60s 超时排查。
    # 线契约钉板（per-dev 冻结帧长 {1:50,2:98,3:19} 与 berry-pwproto 同源）
    # + 双 profile 向量审计（帧↔ACK 恒等 + a8 恰一 NO_ACK + 拒帧零腐蚀账面）。
    python3 tools/siminj.py --selftest
    # Q1c 续挖（a-3 U4 纪律同款）：genohicons 门禁自检——7 桌面图标此前零
    # 守卫：stdlib PNG 解码钉**像素级**黄金哈希 ×7（编码器字节随 PIL 版本
    # 漂移属合法通道，实测 11.3/12.3 像素等而字节 7/7 不等）+ 消费契约
    # （alpha 二值 BYI1 前置 + EXACT 族 6-bit 量化网格 + 不透明账面）+
    # 锚点 ×3 + 8 拒面；PIL 在位时加做 gradient/量化向量 + 重产像素对拍。
    python3 tools/genohicons.py --selftest
    # Q1c（a-3 U4 纪律同款）：genohwallpaper 门禁自检——launcher 内建壁纸源
    # 此前零守卫：135×240 结构钉板 + 头部 SHA-256 ↔ 源 jpg 溯源对账 +
    # 32400px 黄金哈希 + 角锚/唯一色 + 8 拒面；PIL 在位时另做重产逐字节
    # 对拍（缺席打 skip，黄金哈希兜底——genhdfont 同族降级面）。
    python3 tools/genohwallpaper.py --selftest
    # W1-Q4 十续（a-3 U4 纪律同款）：genohstatus 门禁自检——状态栏三图标 +
    # dock 四段内建面（status_icons.rs，launcher 视觉面）此前 import 即
    # 执行、零守卫：7 组 W/H 结构钉板 + PX 黄金哈希 ×3 + alpha 二值域 +
    # BYI wire 走查 ×4（magic/ver/尺寸/Σcount/帧尾——对读 berry-gfx decode
    # 契约）+ 黄金 sha256 ×4 + 五输入源溯源对账 + 8 拒面；PIL 在位时另做
    # 重产逐字节对拍（缺席打 skip，黄金哈希兜底）。
    python3 tools/genohstatus.py --selftest
    # Q1b（a-3 U4 纪律同款）：genhdfont 门禁自检——HD 32×48 档点阵此前零
    # 守卫（genfbfont 三档有 --check，HD 档无）：95 形结构钉板 + 95×192B
    # 黄金哈希 + 基线/下行部几何锚 + 8 拒面；PIL 在位时另做重产逐字节对拍
    # （缺席打 skip，黄金哈希兜底——host 门禁不硬依赖 PIL）。
    python3 tools/genhdfont.py --selftest
    # Q1c 三续（a-3 U4 纪律同款）：starvation_check 门禁自检——饥饿界证书
    # （X-6 bench 对比轮消费面）此前零守卫：fair 等权/一般权界手算向量 +
    # 权比不变性与 slice 线性 + unbounded 双通道（上层饱和/EDF 不可行）+
    # 证书结构钉板 + 拒面（非正权/空 classes/非法 JSON/exit-code 契约）。
    python3 tools/starvation_check.py --selftest

    # elf2pe 换轨二刀后双形态（elf2flat 同位纪律）：产链现役件 = Rust
    # （上方 cargo test tools/elf2pe）；Python 版已降格 elf2pe_oracle.py
    # ——selftest 照跑守语义钉板，Rust 件有疑时拿本件复算对拍，不降豁免表
    # （selftest_guard 零豁免契约不动）。
    python3 tools/elf2pe_oracle.py --selftest
    # mkfat 换轨二刀后双形态（elf2flat/elf2pe/mkcpio 同位纪律）：产链
    # 现役件 = Rust（上方 cargo test tools/mkfat）；Python 版不降格改名
    # ——模块面现役（mkimage/pwrcls import build_volume_puts 单编码器），
    # selftest 照跑守 Python 侧语义钉板 + fixture 重产责任面。
    python3 tools/mkfat.py --selftest
    # A1-3a/3b：mkcpio --split 切分面守卫（重装恒等/恰整数倍/单卷/命名/
    # 拒面/陈卷清理）——rootfs 多卷产链（build-user --split 32）的编码
    # 单源执法，改 mkcpio 必跑。2026-11-11 W1 去重：A1-3a/3b 两刀各接线
    # 一次致同命令跑两遍（实抓），保留单次覆盖面零损失。
    # 换轨二刀后双形态（elf2flat/elf2pe 同位纪律）：产链现役件 = Rust
    # （下方 cargo test tools/mkcpio）；Python 版 mkcpio.py 降格 = 模块
    # import 图根（mkapp/tlsscan/bench/smoke-dpgsoak/berry_assemble/
    # gensourceindex 六处，单编码器纪律——不退役）+ CLI 对拍 oracle
    # ——selftest 照跑守语义钉板，Rust 件有疑时拿本件复算对拍，不降豁免
    # 表（selftest_guard 零豁免契约不动）。
    python3 tools/mkcpio.py --selftest-split
    # W1（彻底 Rust 化轨道 b 层 #8 刀序首位）：mkcpio Rust 件 lib 纯面
    # 测试（垫页几何走查/重装恒等/卷命名/陈卷清理/manifest 拒面/伴生
    # .sig 收编）+ CLI 与 Python 现役件字节级对拍（合成条目集 → 双方
    # 归档 sha256 恒等，Python 在席时执行；缺席打 skip 纯面钉板兜底）——
    # 换轨二刀后本行 = 产链现役件门禁（build-user 走本 bin）。独立小工作
    # 区，须显式 --target {{host}}。
    cargo test --manifest-path tools/mkcpio/Cargo.toml --target {{host}}
    # W1（彻底 Rust 化轨道 b 层 #8 刀序第二位）：mkfat Rust 件 lib 纯面
    # 测试（FAT12/16 编码往返/默认盘烘焙簇锚/--put 嵌套目录链回读/SPC
    # 自适应/total 域/拒面全谱）+ CLI 与 Python 件字节级对拍（合成 puts
    # 三面：--put 整盘 + 默认盘裸调 + FAT16 8MiB → 双方产物 sha256 恒等；
    # 二刀新增守卫/重产四面：--check-fixture{,16} 输出行+漂移红面 rc、
    # --fixture{,16} 重产字节恒等并 = 在册基准、互斥拒面 rc 同形——
    # Python 在席时执行；缺席打 skip 纯面钉板兜底）。独立小工作区，须
    # 显式 --target {{host}}。换轨二刀后本行 = 产链现役件门禁（run-rootfs/
    # test-host 守卫/desktop 走本 bin）；下方 mkfat.py --selftest = Python
    # oracle 语义钉板（模块面现役：mkimage/pwrcls import）。
    cargo test --manifest-path tools/mkfat/Cargo.toml --target {{host}}
    # W1（彻底 Rust 化轨道 b 层 #8 刀序第三位首刀）：mkimage Rust 件 lib 纯面
    # 测试（CHS 向量/MBR 双表项黄金字节含 0xDA 预留槽/manifest v1 黄金字节
    # +PR 单源 abi.toml 绊线+key_hash/name 拒面/整盘装配确定性+FAT 段与
    # mkfat 直产零漂移（path 依赖单编码器纪律）/源实例与验签两件组装面/
    # sha256 FIPS 180-4 标准向量——WIP 期三常量笔误（K[25]/K[59]/h[2]）
    # + CLI 与 Python 现役件 oracle 字节级对拍（整盘+manifest
    # 三面：9MiB 合成四件套/96MiB 默认盘/key-hash 注入，sha256 与 stdout
    # 全等；Python 缺席 skip 纯面钉板兜底）。独立小工作区，须显式
    # --target {{host}}。换轨二刀后本行 = 产链现役件门禁（image 配方
    # 走本 bin）；下方 mkimage.py --selftest = Python oracle 语义钉板
    # （模块面现役：gensourceindex import build_image——单编码器纪律）。
    cargo test --manifest-path tools/mkimage/Cargo.toml --target {{host}}
    # V-3：mkimage MBR 薄面自检（CHS 向量 + 表项钉板——卷编码面归
    # mkfat --selftest，单编码器纪律不在此长第二套）。换轨二刀后双形态
    # （elf2flat/mkcpio 同位纪律）：产链现役件 = Rust（上行 cargo test
    # tools/mkimage）；Python 版不降格改名——模块面现役（gensourceindex
    # import）+ CLI 对拍 oracle，selftest 照跑守 Python 侧语义钉板。
    python3 tools/mkimage.py --selftest
    # W1-Q4 续（a-3 U4 纪律同款）：releng_pack 门禁自检——G1 刀 6 发布
    # 打包流水线（产物检查→尺寸门→mkimage 造盘→manifest）此前零守卫，
    # 只在真发布链上被动覆盖：帽值不变量（G0 刀 1 双槽 256MiB——随刀
    # 单源化 docstring/文案 128 残留同步清偿）/ sha256 分段读对拍 / 尺寸
    # 门恰界两侧 / 缺件 loud 退场 / berry-sign 发现序缺席分支 / run 双路。
    python3 tools/releng_pack.py --selftest
    # W1 接线（releng_pack 同族·零自检清尾）：make_desktop_disk 门禁自检——
    # 确定性桌面数据盘（product-e2e/desktop-calib 消费面）宣称「产物字节
    # 确定性可重复重建」此前零机器执法：LAYOUT 常量契约（schema/页坐标
    # 域/8.3 名形/印章常量）+ 真流双产 sha256 恒等（走真 mkfat 子进程 +
    # gui8 Fat12Volume 注入）+ LBA0 印章前缀与 LBA16 卷签名（vol@LBA16
    # 铁律——印章区不得伪造 BPB）+ LAYOUT.JSN 写读往返字节级对拍 + 缺席
    # 件 None 容忍与重复 add_file 拒形 + cwd 无关确定性（子进程 cwd=ROOT
    # 钉位）。三突变红分支实证（常量漂移/印章换字/注入内容截尾均咬）。
    python3 tools/make_desktop_disk.py --selftest
    # W1 接线（tools 弱面清偿·PIL 渲染生成器族首件）：gen_splash_logo /
    # gen_splash_pct 门禁自检——splash HD 字模两生成器（kernel 消费面
    # splash.rs）此前零守卫：结构律（常量↔行几何互锁/字形序恰 11 形/
    # 类型行钉板）+ 位域律（尾包无效位区全零/值域 v<2^w/72 行恰容/
    # 零填充段）+ 紧致 bbox 律（四缘有墨——getbbox 语义机器镜像）+
    # 黄金哈希 ×2（字源/字号/阈值变更必漂移）+ 突变红 ×6 + PIL 重产
    # 逐字节对拍（缺席 skip——CI Linux 面黄金哈希兜底）；--check 漂移
    # 守卫同位（genfbfont --check 同族）。
    python3 tools/gen_splash_logo.py --selftest
    python3 tools/gen_splash_pct.py --selftest
    # W1 接线（tools 弱面清偿·PIL 渲染生成器族二件）：gencjkfont 门禁
    # 自检——CJK 3500 字 A2 分页字库（berry-gfx 运行期消费面）此前只有
    # --check 漂移守卫零结构执法：常量互锁（.rs ↔ 本件 ↔ 数学关系）+
    # A2 页流全链解码对账（zlib 分页 → 逐页几何 → 3500 形切分）+
    # CJK_GOLDEN 19 条语义 golden 从解出 A2 字节重算（位打包/量化双律）+
    # 码点表律（升序去重/CJK 区段/chars_sorted 排序律/标签十字）+
    # 量化数学钉板 + 突变红 x3；字源 16MiB 恒不入库，零字源面设计
    # （PIL 烟测缺席 skip——bin sha256 互锁兜底）。
    python3 tools/gencjkfont.py --selftest
    # W1 接线（tools 弱面清偿·PIL 渲染生成器族三件）：gen_android_nav_icons
    # 门禁自检——AOSP SystemUI 导航 VectorDrawable 光栅化器（297 行，源
    # XML+产物均入库）此前零守卫：源哈希互锁 x3（AOSP 资产篡改即红）+
    # 结构律（NAV_ICON 六 id/九数组 5184 尺寸）+ ARGB 打包律（RGB 域恒为
    # 配对墨色）+ 几何律零 PIL 全链（pathData 纯 stdlib 解析 → polygon
    # bbox(72 网格) 与九数组重算 mask bbox 包络对账——CI 无 PIL 面可跑）+
    # 掩码互异 + 突变红 x3；PIL 重产逐字节对拍缺席 skip（#644 判例：CI
    # runner 无 PIL，import 惰性化）。
    python3 tools/gen_android_nav_icons.py --selftest
    # W1 接线（tools 弱面清偿·串口判读契约族）：desktop-calib 门禁自检——
    # Mac HVF 双臂标定件（不进 CI）的串口判读契约此前零机器执法：
    # VGPUPERF/VGPUDMA 解析律（字段提取/坏值容忍/token==submits 契约）
    # + 基线快照 + median/percentile 统计数学（numpy 线性插值口径）+
    # snap240 滑窗闭窗恰界与达标阈值（round 非 ceil）+ move 数换算 +
    # SerialTail 增量尾读（半行跨读/close 重开从头读契约）+ 突变红 x3。
    # 改 desktop-calib 判读/统计面必跑。
    python3 tools/desktop-calib.py --selftest
    # K-3：size.py ADRP 解码器向量自检（--assert-flip 扩面的门禁自身坏 =
    # smoke/build-release 断言静默劣化，同 a-3 U4 纪律门禁化）。
    python3 tools/size.py --selftest
    # T-1 §1：tlsscan 门禁自检（tls_place python 镜像 + F1/F2 全链 + cpio/
    # .bap 递归；mkfat --selftest 同位先例）+ census 审计（恒绿——拷贝式
    # 后 .tbss∩文件字节无害只配审计；rootfs.cpio 缺席打跳过不 FAIL）。
    python3 tools/tlsscan.py --selftest
    python3 tools/tlsscan.py --audit
    # Q1 续（a-3 U4 纪律同款）：genfbfont 门禁自检——三档字体表单源生成器
    # （fbcon 1x + gfx s2/s3），此前只有 --check 漂移守卫（两侧同源时生成器
    # 逻辑改坏仍绿），selftest 另钉生成器语义：结构契约+拒面 5、NN 放大
    # 数学性质、'!'/'=' 手算字节 ×3 档、表头契约、在册产物对偶。
    python3 tools/genfbfont.py --selftest
    # 漂移守卫（遗漏审计 A3 判例——与 gen_abi --check 同款纪律，此前零接线）：
    python3 tools/genfbfont.py --check
    # W1-Q4（a-3 U4 纪律同款）：policy_ab_eval 门禁自检——S6 策略服务 A/B
    # 升级判定器（PROMOTE/DEMOTE/HOLD）此前只在升级工作流里被一次性消费：
    # judge 三区间手算 + 阈值严格边界（恰等 = HOLD）+ packed u64 位布局 +
    # 载体行族/键别名/噪声免疫 + --delta 首尾差 + exit-code 契约 + 确定性
    # （shadow 评估前提）——25 面。
    python3 tools/policy_ab_eval.py --selftest
    # W1-Q4 续（a-3 U4 纪律同款）：advisor 门禁自检——首个真实策略生成器
    # （遥测进、PolicyEntry 出）此前只有 --demo 一次性展示：五规则手算 +
    # 严格边界（wake 恰等 100 不加成）+ RT/哨兵屏蔽 + headroom 贪心序
    # 手算向量 + 字段序镜像 PolicyEntry + exit-code 契约 + 确定性——36 面。
    python3 tools/advisor.py --selftest
    # W1-Q4 再续（a-3 U4 纪律同款）：sched_perf_analyze 门禁自检——调度 1.1
    # 验收判官（pick<5µs/freeze<5ms/billing<1µs 对照）此前只有 --demo 一次
    # 性展示：规格常量钉板（LIMITS/FREQ 漂移即咬）+ 手算向量（62.5MHz 下
    # ns=cycles×16：pick 312/313 跨 5µs 界、freeze 312500cyc 恰等拒、
    # billing 62/63 跨界）+ 噪声免疫/保序 + no-limit 三值逻辑不连坐 +
    # exit-code 契约 + byte 级确定性——18 面。
    python3 tools/sched_perf_analyze.py --selftest
    # W1-Q4 四续（a-3 U4 纪律同款）：sched_dashboard 门禁自检——调度统一
    # 仪表盘（验收 3/3 + 基线回归双门判官，sched_ci 全流程 subprocess 消费）
    # 此前只有 --demo 一次性展示：内核断言口径常量钉板（FAIR spread 20/
    # policy delta 50%/tol 0.10/PP 5.0 四突变红分支实证均咬）+ parse 八类
    # 行（尾逗号/空 hits/重复取最后/字段序敏感/噪声免疫）+ compare 恰界
    # 双向（rel 1100 恰放 1101 咬、pp +5.0 恰放 +5.1 咬）+ base=0 特判 +
    # 缺席计 FAIL 分母恒 3 + exit-code 契约 + 确定性——29 面。
    python3 tools/sched_dashboard.py --selftest
    # W1-Q4 五续（a-3 U4 纪律同款）：shim_audit 门禁自检——G-eco-d libc
    # shim 路线裁决器（d-3c hexyl 终验「全可 shim」证据的数据源）此前只有
    # 一次性人工跑：GLOBALS_DEF 分类口径钉板 + nm 行形四态（成员头/三段/
    # 两段/空行）+ U/w 覆盖序（后行 U 升强、setdefault 保强不降级）+ 本地
    # 小写 t/N 调试段不计定义 + 定义覆盖净面 + elf 两段形收/三段不收 +
    # 全链合成归档向量手算对拍 + exit-code 契约——19 面。
    python3 tools/shim_audit.py --selftest
    # W1-Q4 六续（a-3 U4 纪律同款）：gen_abi 门禁自检——ABI 单源生成器
    # （abi.toml → 五面产物）此前只有 --check 漂移守卫（两侧同源时生成器
    # 逻辑改坏仍绿）：求值器四语法族+注入拒面/C 宏映射（非规范拼写归一
    # 不透传）/唤醒号段活算（占满/跳锚/中段空洞）/validate 合成单源 12
    # 拒面（账四元组/同 scope 位图/语义锚）/行筛选三分口径/four_tuple
    # 手算/syz 投影（宽度映射拒面+同形不重复+stage 四态）/marker 分流
    # +重复对拒/计数锚交叉断言/c_defines 对齐数学/exit-code 契约 + 在册
    # 五产物逐字节 oracle + 全链确定性——53 面。
    python3 tools/gen_abi.py --selftest
    # W1-Q4 七续（a-3 U4 纪律同款）：sched_ci 门禁自检——调度 CI 编排面
    # （build→boot→dashboard 判定串联的全流程判官）此前零守卫：build_env
    # 三律（~/.cargo/bin 前置判定/CARGO_TARGET_DIR 恒钉/不变异）+ boot_cmd
    # 钉板（MACH 同款无 -smp）+ ktest_seen 子串口径 + analyze 四态
    # （mode/gate_label/argv 一一对应）+ **挪位陷阱全链实证**（同路径
    # --save+--baseline 自比恒稳 rc=0 静默漏放 vs 挪位对拍真回归被咬，
    # 子进程真跑 dashboard）+ exit-code 契约——24 面。
    python3 tools/sched_ci.py --selftest
    # W1-Q4 八续（a-3 U4 纪律同款）：unsafe_audit 门禁自检——unsafe 台账
    # 审计器（CI 双 workflow --check 执法、945 条在册登记的漂移守卫）
    # 此前零自检：词法掩码（注释/raw/C 串/字符字面量）+ 跨度配平 +
    # 四类出现点分型 + 签名位只计不登记 + classify 四层优先级链 +
    # ※ 内容锚继承（行漂移不丢人工判读）+ splice marker + live 树
    # 结构不变量（行号单调/越界/kind·分类域）+ 确定性——40 面，三突变
    # 红分支实证（掩码退化/分类翻转/锚继承丢失均被咬）。
    python3 tools/unsafe_audit.py --selftest
    # W1-Q4 九续（a-3 U4 纪律同款）：writable_globals 门禁自检——可写全局
    # 清单执法面（just check / CI 前置，76 条在册）此前零自检：auto_group
    # 七组分类链 + 声明点正则（static mut 可见性前缀/UnsafeCell 收编/
    # &'static mut 局部不误算/注释 raw 串掩后不登记）+ manifest 契约
    # （四坏行 SystemExit）+ 三态 main 子进程真跑红绿分支（MISSING/STALE
    # 红 rc=1→update 收敛→check 转绿）+ live 对拍 + 确定性——59 面，
    # 四突变红分支实证（分类翻转/正则放宽/漏报/守卫拆除均被咬）。
    python3 tools/writable_globals.py --selftest
    # W1-Q4 十一续（a-3 U4 纪律同款）：bench 门禁自检——B-0/B-1/R-1/lat/
    # icount/trace-dual/dpg 六通道 QEMU 门禁的宿主收割对拍面此前零自检：
    # 收割正则族（B-0 行锚定 vs B-1/RT/LAT/WAKE 全文子串——矩阵巨行中段
    # 教训族；WAKE 撕裂半行结构性不命中）+ 双段基线 roundtrip（--update
    # 单段刷新另一段保留）+ icount on/off 双文件头契约 + _l2_bad L2 带
    # （0→非零无界/双侧漂移/项消失）+ 三镜迭代定值 live 对读（bench.rs/
    # bench1/bsh WANT ↔ 两 iter 表——「改值三处同步」的宿主执法面）+
    # RT/LAT wire 镜像 + 回执 sha256 + 六通道组合拒绝 exit-code 契约——
    # 57 面，三突变红分支实证（wire 冻结/单段保留/iter 漂移均被咬）。
    python3 tools/bench.py --selftest
    # W1-Q4 十二续（a-3 U4 纪律同款）：echod/evsclient 宿主链自检——smoke
    # T-c/A2-3/A3-3 相的宿主网络件（TCP 回显对端 + EVS/SKT 对拍 client）
    # 此前零守卫：echod 环回四面（echo 语义/半关被动 close/并发连接/段化
    # delivery——bind 临时口 0 不碰 18080-83 互斥口）+ evsclient 五面
    # （载荷黄金哈希 vs bsh EVS_PAYLOAD 同源定值/try_conn 全链/短斤拒面/
    # 帽序契约/main 整链输出全定）。产线 serve 路径零改动（拆
    # serve_bind/_accept_loop 复用同一段，被测 = 产线本体非影子副本）。
    python3 tools/echod.py --selftest
    python3 tools/evsclient.py --selftest
    python3 tools/svg2icons.py --selftest
    # SDK-2 P0-1：berry-crypto（.sig 信封 v1 TLV + 信任账本 TOML + 水位比较器
    # + RFC 8032 向量门空转框架——对选型零敏感件先行，26 号 §3-2；原语本体 =
    # P0-2 另刀，向量门不过不合入；P0 合入 = 闸 B，24 号 §4.2 终核三条）。
    cargo test --manifest-path user/Cargo.toml -p berry-crypto --target {{host}}
    # P3-0 收官（DoD-10 裁定执行·33 号）：berry-sign selftest 复跑 + p3sim
    # --selftest 接线——两件此前零 CI/测试挂载（selftest 是 CLI 子命令非
    # #[test]，cargo test 对 main.rs 0 测实证不覆盖 bin 编排面 keygen 0600
    # 落盘/cpio 走查/admit 目录循环；p3sim 自检只手跑）= 有 selftest 零
    # 自动执法，ksproto/genwycheproof 未接线判例同族。cargo run 同
    # CARGO_TARGET_DIR/host 轨自建 bin（cargo test 不产可执行件，deps/
    # harness 副本实证）；测试钥纪律 = berry-sign 头注：/tmp 现产清场
    # 永不真签。
    cargo run --manifest-path user/Cargo.toml -p berry-crypto --bin berry-sign --target {{host}} -- selftest
    python3 tools/p3sim.py --selftest
    cargo test --manifest-path user/Cargo.toml -p berry-verify --target {{host}}
    # W1 接线（echod/evsclient 未接线判例同族）：genwycheproof 门禁自检——
    # Wycheproof 向量表生成器（fixture JSON → wycheproof_vectors.rs）随
    # SDK-2 P0-2 落地，selftest 37 面自称绿但从未接线 test-host：有
    # selftest 零自动执法 = 生成器逻辑改坏且重产时 --check 照样绿（自检
    # 用合成 fixture 钉 validate 拒面/映射/黄金锚/确定性/产物对偶，逻辑
    # 改坏即咬）。改 genwycheproof.py / wycheproof_ed25519_test.json /
    # wycheproof_vectors.rs 联动面必跑。
    python3 tools/genwycheproof.py --selftest
    # 漂移守卫（遗漏审计 A3 判例——fixture↔产物对拍此前从未自动执行）：
    python3 tools/genwycheproof.py --check
    # W1 接线（genwycheproof 同族·程序化扫描清尾 1/2）：gensourceindex
    # 门禁自检——源索引/源钥清单生成器（SDK-2 分发签名链 v1.2 §五镜像，
    # .sig 信封两件套随镜像 FAT /SOURCE/ 挂入），selftest 38 面（TLV
    # round-trip 字节恒等/八字段对账/单调门回降拒/结构拒面 16 型/黄金
    # 字节/构造确定性/mkimage SRCINST_* 挂接合同对拍/FAT16 回读对拍）
    # 自称绿但从未接线 test-host：有 selftest 零自动执法 = TLV 布局或
    # berry-sign 合同面改坏无机器咬合（⑧ 跨工具常量对拍 = 同步义务唯一
    # 执法点）。改 gensourceindex.py / mkimage.py SRCINST_* 段 / source-
    # index.tlv 布局面必跑。全合成夹具零构建依赖（cwd 无关已验）。
    python3 tools/gensourceindex.py --selftest
    # W1 接线（同上扫描清尾 2/2）：fastlane_sim 门禁自检——快道重研题一
    # 桌面推演模拟器（28 号 §2 状态机 × 不变量机器验证：整编队 M/X + 快道
    # F 槽对翻指针，I-PR/I-SL-1..4 随随机双轴版本流全场景断言），selftest
    # 45 面（不变量保持 + B1 反命题 P 证伪重现 + P′ 对抗搜索 + 语料
    # digest 黄金锚 + 案 α 策略扫描）。零接线 = 状态机语义（回退门/地板
    # 吸收/关窗触发）改动无机器咬合，黄金锚漂移无人咬。改 fastlane_sim.py
    # 或 28 号语义面必跑。固定种子确定性（DEFAULT_SEED 20261111）。
    python3 tools/fastlane_sim.py --selftest
    # W1 接线（程序化扫描清尾补遗·连字符拼写漏网件）：gui8-smoke
    # plumbing 自检——GUI-8 交互相验收脚本的纯函数面（哨兵/几何/标记
    # 精确计数与顺序断言/canonical 109B/QMP mock），flag 拼写是
    # --self-test（连字符），此前两轮扫描按 --selftest 口径漏网：
    # 有 self-test 零自动执法 = 07.md 已列 plumbing 命令但断言逻辑
    # 改坏无机器咬合（Boot A/B 精确计数负样本错序/重复/缺失均应
    # FAIL——selftest 直接覆盖负样本）。零 QEMU 秒级。改
    # gui8-smoke.py 断言/哨兵/几何/QMP 面必跑。
    python3 tools/gui8-smoke.py --self-test
    # W1 接线（*.sh 件清尾·ADR #84 立法门）：bcts-run 门禁自检——BCTS v0
    # runner 的工件 schema/拒面/两态清单对拍 8 腿（零 QEMU）。此前两轮
    # 扫描只扫 *.py，bcts-run.sh --selftest 自称 8/8 绿但从未接线：
    # 有 selftest 零自动执法 = parse_frozen 校验逻辑改坏无机器咬合，
    # 冻结件 ↔ markers.sh 单源漂移无人咬。注意两腿义务：活树计数
    # 32/56/3 钉板 + 冻结快照全等腿——加标记的 PR 会红到此门，须随刀
    # 更新 TIER1_FLOOR_LIVE/冻结件（ADR #84 兼容面演进显式化，非误红）。
    # 改 bcts-run.sh / markers.sh 标记面 / bcts@*.toml 冻结件必跑。
    {{BASH}} tools/bcts-run.sh --selftest
    # W1 接线（2026-11-13 run 43306506456 实证；base 重写剥落重放）：dns-stub
    # 门禁自检——CI netns 内 SLIRP DNS 转发死路的 UDP:53 应答器（bsh EPL⑤
    # getaddrinfo park → rootfs 相 RPC 族 12 标记全灭 600s FAIL）。协议腿
    # （A/NXDOMAIN/压缩指针/畸形包）+ live UDP 闭环 13 腿；改 dns-stub.py
    # 或 bcts wrap 必跑。
    python3 tools/dns-stub.py --selftest
    # W1 接线（2026-11-19；base fc99d2b79 GUI 合并批带入两件零接线实捕）：
    # cpio_filter 门禁自检——mkcpio newc 归档确定性 marker 移除/追加（产品
    # 桌面镜像与 GUI8 验证镜像共享 rootfs 条目的 probe.on/gpuhdi.on 分叉
    # 面）；垫料解析/重复 remove/追加冲突 loud 拒面。改 cpio_filter.py 或
    # 镜像 marker 分叉面必跑。
    python3 tools/cpio_filter.py --selftest
    # W1 接线（同批实捕）：gui_guard 门禁自检——F9 普通 app 边界静态守卫
    # （wm=true 必依赖 berry-room/berry-ui、fs RPC 走 fsclient、源码禁
    # syscall 面/WM RPC 解码/像素基元、nshfont 不得在场）；正样本 + 22 类
    # 负样本 fail closed。改 gui_guard.py 或普通 app 授权面必跑。
    python3 tools/gui_guard.py --self-test
    # W1 接线（#257 后继件；base 重写剥落重放）：bcts-parkprobe 门禁自检
    # ——berryos-smoke.* 串口日志 park 签名分类器（dns_park/churn_starve/
    # net_live/clean_boot 四分支钉板 + unreadable 负样本）；bcts 失败路径
    # 自动消费（ci.yml bcts 步），签名库漂移即红。改 bcts-parkprobe.py
    # 或 bcts 失败取证面必跑。
    python3 tools/bcts-parkprobe.py --selftest
    # W1 接线（#290 REJECT 修面；base 重写剥落重放）：bcts-hostfwdprobe
    # 门禁自检——CI netns 内 SLIRP hostfwd 入向段预检（腿 A ns loopback/
    # 18080 回显环境锚 + 腿 B QEMU -S 冻结态 18081-83 connect 探活）；
    # netdev 单源对拍/argv/回显环回/poll 收敛/恒拒面/定谳行钉板 6 腿零
    # QEMU。改 bcts-hostfwdprobe.py 或 smoke.sh hostfwd 面必跑。
    python3 tools/bcts-hostfwdprobe.py --selftest
    # W1 接线（同轮·shvarcheck 自检本体）：--check 面（tools/*.sh 花括号
    # 守卫）已挂 test-host 首位，但 selftest 钉板（吞名 7 形 + 免疫 6 形
    # + 修复口径）零接线——check 的检测逻辑自身改坏（吞名形漏检/免疫形
    # 误报）无机器咬合。检测器自检（selftest 钉板）与检测器执法（--check
    # 全仓扫描）分层——同 gen_abi --check/--selftest 双面纪律。改
    # shvarcheck.py 检测逻辑必跑。
    python3 tools/shvarcheck.py --selftest
    # W1 接线（独立冒烟族零接线判例同族）：smoke-lockdep / smoke-dpgsoak /
    # soak-g3 三件独立关卡脚本 selftest 挂载——前置拒面（用法/缺件/marker
    # 串/分钟数/QEMU 缺件）+ lockdep 假 QEMU 相 1 判定引擎（计数/行序红绿
    # 两向），零真 QEMU 零端口零真仓副作用（脚本副本沙箱）。此前有
    # selftest 零自动执法 = 拒面或相 1 判定面改坏无机器咬合。改三件脚本
    # 前置拒面或判定面必跑。
    {{BASH}} tools/smoke-lockdep.sh --selftest
    {{BASH}} tools/smoke-dpgsoak.sh --selftest
    {{BASH}} tools/soak-g3.sh --selftest
    # W1 接线（fwcfg 键表对拍门禁化——「有守卫零自检零接线」判例族）：
    # fwcfg-check.sh 对拍执法面（tools/fwcfg-keys.toml 单源键表 vs 仓内
    # tracked 注入/消费面零漂移）+ selftest 9 腿（对拍绿红双向/fixture
    # 排除/rootfsN 归并/键长 55B 上限门/非 git 树拒）。引擎 git grep
    # （tracked 单源；rg runner 缺件判例 #371），此前 rg 版在 base 假红
    # 两月（fwcfgface 合成 fixture trac/missing 误入代码面）无人咬合。
    # 改键表/改注入消费面必跑。
    {{BASH}} tools/fwcfg-check.sh
    {{BASH}} tools/fwcfg-check.sh --selftest
    # W1 接线（tools 零自检纯函数件清零族）：paths.sh 路径解析单源——
    # justfile KERNEL/RUST_LLD 属性背引号 + smoke.sh source 双消费面，
    # 此前零自检（解析律/拒面/CLI 形态改坏无机器咬合）。selftest 10 腿
    # （合成 cargo/rustc 垫片沙箱：target-dir/kernel 组装/垫片哑拒/缺席
    # 拒/usage 拒/source 形态三函数）。改解析逻辑或拒面措辞必跑。
    {{BASH}} tools/paths.sh --selftest
    # W1 接线（同族）：ver-check 版本一致性验收——selftest 8 腿（合成 git
    # 沙箱：G-ver-3 对拍绿/不匹配红点名/无 version 红/git_ident 缺席红/
    # 段作用域防串号/零 tag 豁免/非 git 豁免）。发版门形态改坏可被咬合。
    {{BASH}} tools/ver-check.sh --selftest
    # W1 接线（同族·G1 刀 4 密封构建包装器）：reproducible_build.sh——
    # SOURCE_DATE_EPOCH/路径 remap/离线/umask/TZ/LC_ALL 六密封面 + git
    # 缺席回退 epoch 0 + just 目标透传，此前零自检（密封面改坏 = 产物
    # 不可复现静默漂移，无机器咬合）。selftest 5 腿（合成 git+just 垫片
    # 沙箱：exec 落地视图逐项对账/非 git 回退/多目标透传/RUSTFLAGS 叠加
    # 保留/just 缺席 loud 拒）。改密封面必跑。
    {{BASH}} tools/reproducible_build.sh --selftest
    # W1 接线（D-审计 §4.3「从未跑过」面）：guifbd lib 单测 23 测
    # （B1-b/c vgpu 后端纯面——后端分派/INDIRECT 链/ATTACH 段/提交序）入
    # host 台账。独立小工作区（空 [workspace] escape），berry-libc
    # 按 target_os=linux 门控故 host 编译零 libc shim 连接；此前仅
    # B1-b 落地时手动跑过，无 just/CI 接线 = 改坏无机器咬合。
    cargo test --manifest-path user/bins/guifbd/Cargo.toml --target {{host}}
    # W1 接线（D-审计 §4.2 尾「无 just/CI 接线」面）：tools/berry 设备壳
    # host 单测（VNC/VeNCrypt/DES/QR/网页壳状态机等纯面——README §测试
    # 原只记手动咒语）入 host 台账。独立小工作区，host 纯面零交叉编译；
    # 此前手动 cargo test 从未接线 = 面改坏无机器咬合。2026-11-22 与上方
    # cbox 刀 3 接线位去重——同 crate 双跑每轮白跑 369 测且 07 口径翻倍
    # 计（沿革「链内 berry 双跑净增 X=N×2」注记即此债痕迹），本位注释
    # 留档、执行归上位单跑。
    # W1 接线（R136 判例族·「有 host 测零自动执法 = 静默腐烂」）：
    # user/widgets/ 四独立小工作区（G3 刀 11-14 落地，26 host 测）——
    # berry-scene-ir（声明式小组件二进制序列化 16B header+24B 定长
    # 节点四型 + 40 节点帽 + 全验证反序列化）、berry-widgetd（WidgetGrid
    # 4×4 网格/TimelineScheduler 过期时间线/SmartStack 同格叠放轮换）、
    # berry-island（Spring2D 临界阻尼弹簧 ζ=1 无超调 + 四态机合法转移
    # 表）、berry-intentd（Intent 8 型 + 路由优先级 32 帽 + 意图链
    # ≤8 步）。非 user workspace members（空 [workspace] escape，sysnr
    # 先例形）此前零 just/CI 接线 = 语义面改坏无机器咬合。W2 布局树
    # 移交注所点名（接线归 owner/G3 线——W1 代接线，先例 = lkface
    # R136 建议件/guifbd D-审计接线）。
    cargo test --manifest-path user/widgets/scene-ir/Cargo.toml --target {{host}}
    cargo test --manifest-path user/widgets/widgetd/Cargo.toml --target {{host}}
    cargo test --manifest-path user/widgets/island/Cargo.toml --target {{host}}
    cargo test --manifest-path user/widgets/intentd/Cargo.toml --target {{host}}
    # W1 接线（R136 判例族同款）：user/nfc/ + user/accessories/ 四独立
    # 小工作区（G4 刀 15-18 落地，18 host 测）——berry-nci（NCI 协议栈
    # 纯面：MT/GID/OID 帧 + ISO 7816 APDU + SW1/SW2 + eSE 状态机）、
    # berry-walletd（四卡型生命周期 + 优先级出卡 + Power Reserve 断电
    # 策略 + 余额扣费）、berry-stylus（1D 卡尔曼滤波位置/速度双通道 +
    # 240Hz 预测 ≤4ms）、berry-uwb（ToF 测距 + PDoA 方位角 + 三边定位
    # Cramer 2×2 含 λ 修正）。同 widgets 形零接线 = 同款静默腐烂面；
    # W2 布局树移交注点名件（dc4fb5601）。
    cargo test --manifest-path user/nfc/nci/Cargo.toml --target {{host}}
    cargo test --manifest-path user/nfc/walletd/Cargo.toml --target {{host}}
    cargo test --manifest-path user/accessories/stylus/Cargo.toml --target {{host}}
    cargo test --manifest-path user/accessories/uwb/Cargo.toml --target {{host}}
    # W1 接线（R136 判例族同款·G2 刀 8-10 兼容子系统三件）：user/compat/
    # 三独立小工作区（15 host 测）——linux-runner（syscall 翻译守护：
    # ELF64 头解析/优先级扫参序/未知 syscall loud 拒）、berry-vmm
    # （MicroVM 管理器：vCPU 生命周期/客户机物理内存布局/中断注入
    # 裁决）、compat-wayland-bridge（wl_surface/wl_shm → BerryOS
    # SurfaceId/VMO 桥接：surface 映射能力面）。空 [workspace] escape
    # 同 widgets/nfc 形，此前零 just/CI 接线 = 语义面改坏无机器咬合。
    cargo test --manifest-path user/compat/linux_runner/Cargo.toml --target {{host}}
    cargo test --manifest-path user/compat/vmm/Cargo.toml --target {{host}}
    cargo test --manifest-path user/compat/wayland-bridge/Cargo.toml --target {{host}}
    # W1 接线（R136 判例族同款·终端刀 5 纯核两件，base 9cfb160ca 直推
    # 零接线）：user/faces/utf8width（UTF-8 显示宽度：East Asian Wide/
    # 全角锚/组合符零宽）+ user/ptyface（伪终端面：线路规程状态机/
    # termios 域）。15 host 测，空 [workspace] escape 同 widgets/nfc/
    # compat 形——零 just/CI 接线 = 语义面改坏无机器咬合。
    cargo test --manifest-path user/faces/utf8width/Cargo.toml --target {{host}}
    cargo test --manifest-path user/ptyface/Cargo.toml --target {{host}}
    # W1 接线（base 重写剥落重放，#304 面）：sync-public members 对账门——
    # 白名单 ⊇ workspace members 机器执法（#153/#171/#227 三连 REJECT
    # 判例族根因：新增顶层 crate 忘补白名单 = 公开仓 build 必炸）。本刀
    # 实捕 fwcfgface/susface 两漏（#299/#307 合入后白名单笔未随刀）。
    # 新增顶层 crate 或改 WHITELIST 必跑。
    {{BASH}} tools/sync-public.sh --check-members
    # W1 接线（#290 后继件，base 重写剥落重放）：sync-public 门禁自检——
    # 出口形态钉板 21 腿（端到端同步/安检拦泄漏/members 红分支/独立入口；
    # 零网络零真实镜像仓）。改 sync-public.sh 出口安检或形态适配面必跑。
    {{BASH}} tools/sync-public.sh --selftest
    # W1 接线（#214 面，base 重写剥落重放）：selftest_guard——tools 自检
    # 接线门禁本体。「有 selftest 零自动执法」判例族 7 连发根治：tools/
    # 下声明 --selftest 而 justfile 无接线行即红。本刀元自证——恢复时
    # 门禁先咬到自己与 sync-public 两件未接线。改 tools/ 新增自检 /
    # justfile test-host 面必跑（本行即自证）。
    python3 tools/selftest_guard.py --selftest
    python3 tools/selftest_guard.py
    # 工程纪律刀阶段 2 接线：判据单源与新鲜度三门自检——markers（canon
    # 互非子串 + 哨兵↔消费者 1:1 映射）、image-fresh（manifest 对拍夹具
    # 四腿）、floor-check（版本地板三腿）。改 markers.sh 标记/banner 面、
    # image-fresh.sh、floor-check.sh 必跑（selftest_guard 扫描自动执法）。
    {{BASH}} tools/markers.sh --selftest
    {{BASH}} tools/image-fresh.sh --selftest
    {{BASH}} tools/floor-check.sh --selftest
    # 工程纪律刀阶段 3 接线：门禁链负例自检（合成 git 沙箱六腿——坏拒/
    # 好过+印记耗/no-verify 拦/merge 免检/索引留毒拒/未跟踪不误伤）。
    # 改 .githooks/* 或 check-docs 检查面必跑。
    {{BASH}} tools/gate-selftest.sh
    # 自动化开发二期 P0 接线：E lane 六件自检——e-guard（punch oracle/
    # FROZEN 自指/stop-hook 环境腿/四门拒写）、e-doctor（缺件红/全配过）、
    # e-supervisor（机制分判/revert 通道/repair 判据）、pack-work（开工包
    # 切分）、push-green（supersession/取消弱证据）、e-receipts（内容寻址）。
    # 改 tools/e-*.py、run-e.sh、pack-work/push-green 必跑。
    python3 tools/e-guard.py --selftest
    python3 tools/e-doctor.py --selftest
    python3 tools/e-supervisor.py --selftest
    python3 tools/pack-work.py --selftest
    python3 tools/push-green.py --selftest
    python3 tools/e-receipts.py --selftest
    # 自动化开发二期 P1/P2 接线：评审凭据三件（pin 校验/协议行回执验收/
    # 欠账记账）+ 证物链两件（信封/包裹器）+ 治理四件（控制面门/两张债网/
    # 决策读回/配置面审计）。改对应工具必跑。
    python3 tools/review-pin-check.py --selftest
    python3 tools/review-recv.py --selftest
    python3 tools/review-debt.py --selftest
    bash tools/run-artifact.sh --selftest
    bash tools/gate-wrap.sh --selftest
    bash tools/control-plane-gate.sh --selftest
    bash tools/run-e.sh --selftest
    python3 tools/punch_slice.py --selftest
    python3 -I tools/task-graph.py --selftest
    python3 tools/task-oracle.py --selftest
    python3 -I tools/stale-parks.py --selftest
    python3 -I tools/stranded-deferrals.py --selftest
    python3 -I tools/answers-readback.py --selftest
    python3 -I tools/claude-audit.py --selftest
    # 工程纪律刀阶段 4 接线：数字自证面——facts 对拍自检（合成仓 11 键/
    # 篡改红/幂等）+ check-docs 检测器自检（四组坏样例必触发——「a detector
    # that goes quiet is an ERROR」）。改 facts.py、check-docs 检查面必跑。
    python3 tools/facts.py --selftest
    node .agents/tools/check-docs.mjs --selftest
    # W1 接线（tools 弱面扫描件）：sdk/design 两本 ledger 门禁自检——
    # CI smoke 消费位（ci.yml sdk/design ledger 步）此前零自检：门禁
    # 自身坏 = 静默劣化（a-3 U4 纪律同款）。sdk_ledger 6 腿（facade
    # 提取三形/validate 拒面族 12 变异/双向对账 missing·stale/matrix
    # 黄金形/doc 往返漂移红）；design_ledger 7 腿（WCAG 数学锚 21:1
    # 极值与 448/451 门槛夹逼/parse_color 拒梯/enum 提取/36 对全谱
    # matrix/validate 全链拒面 9 变异含 rogue widgets.rs 硬编码色与
    # cfg(test) 豁免/doc 往返）。改两件门禁逻辑或 design_tokens/
    # sdk_api 词表必跑。
    python3 tools/sdk_ledger.py --selftest
    python3 tools/design_ledger.py --selftest

# 忙核 arm-to-nearest-deadline（oneshot 收紧律）host 钉板：律单源
# sched/src/oneshot.rs（sched crate 本体 no_std + HAL 依赖不编 host），
# 面板 sched/host/oneshot_law_test.rs 经 #[path] mod 同源编译——
# cargo test -p berry-sched 走不通（lib 先于 tests 编译即败于 HAL），
# 故 rustc --test 直跑单文件。改收紧律 / sched.rs 两消费臂必跑。
test-oneshot:
    # 刀 A 顺手修：rustc -o 落 target-cargo/ 但目录未必在（首跑即败
    # "couldn't create a temp dir"）——mkdir 前置（mkfat --check-fixture
    # 同位自足纪律）。
    mkdir -p target-cargo
    rustc --test sched/host/oneshot_law_test.rs -o target-cargo/oneshot_law_test
    target-cargo/oneshot_law_test

# 内核体积基线报告（12号报告 §10.1：体积讨论从 day-one 有数据）。
# 零依赖 python3 解析 ELF：文件量 / PT_LOAD 镜像与静态内存 / alloc 段明细。
# 只报数不设门禁——阈值等基线稳定后再定（Tock 192KB 教训）。
size: build
    python3 tools/size.py {{KERNEL}}

# B-0 性能基准单口（docs/基准与压力测试设计-v1.0.md）：QEMU 裸跑收割 BENCH 行 +
# 基线对拍（tools/bench_baseline.txt，宽松因子 3x 拦数量级劣化）。改内核后
# 刷新基线：python3 tools/bench.py --update（diff 随源入册）。严格确定性
# 断言（churn 泄漏金丝雀/标记到齐）已挂 just smoke 永绿红线——本口只管数值。
bench: build
    python3 tools/bench.py

# R-1 实时基准单口（docs/基准与压力测试设计-v1.0.md §10）：cyclictest 式 oversleep
# 三档（slice/4、slice、4·slice）× 32 样本。fw_cfg 具名项 opt/berry/rt 门控
# （缺席 = run/smoke 零扰动——公约 7：~34s 采样循环不进常规 boot；EFI 路
# 无 fw_cfg 具名文件 = smoke-image 亦天然缺席）；裸跑等 RT ok n=96，帽 120s；
# 无基线（oversleep>=0 严格断言在内核侧 panic 执法 = L0，分布数值只出表
# ——统计归统计）。
bench-rt: build
    python3 tools/bench.py --rt

# B-2 -icount 确定性通道单口（docs/基准与压力测试设计-v1.0.md §13）：QEMU 追加
# 定值 `-icount shift=1,sleep=off`——周期数变指令数代理（shift=1 虚拟时钟
# ≈0.125 cyc/instr），与 TCG 实时基线**不可比、分文件**
# tools/bench_baseline_icount.txt（跨机/跨 QEMU 版本亦不可比，CI 首跑自立）。
# boot 时序全变 → 只跑 bench 收割（十行 BENCH 门禁面（icount 基线刀 2026-11-18 两项入册 + W3-Q98/Q102 再两项随迁） + BENCHWAKE 观测行），
# 不用于功能面。L2 紧门禁形态（精确相等/±5%）由五跑证明烘焙为 bench.py
# ICOUNT_TOL 常量；证明可重验：
#   python3 tools/bench.py --icount --prove
# （五跑漂移谱 + verdict；漂移是合法结局——按 verdict 定 ICOUNT_TOL。）
bench-icount: build
    python3 tools/bench.py --icount

# S-1 soak 长跑单口（docs/基准与压力测试设计-v1.0.md §11）：内核常驻任务 soakmon
# 每 60s vct（本机实测 ≈60s wall——CNTVCT ≈1.0x，历史半率机型 ≈120s，规划
# 按半率保守）打心跳 v2.1 行 `SOAK t= sw= sw0= sw1= park0= park1= pages=
# lost= churn=`（单核面 sw1/park1 恒零自证 + churn 无载荷恒 0 不入断言），
# soak.sh 轮询六断言（t/sw/sw0 严格递增 + sw1/park1 恒零 + pages 稳态窗
# [首拍,首拍+64]〔首 5 拍结算豁免——S-5d〕+ lost 恒 0）+ boot 覆盖守卫
# （SOAK gate on 门控自证 + ECO/VIRTIO/SKT/FAT/NET/DNS/TCP/TCPS/EVS 十标记
# ——A3-3 宿主对端三件套 echod/evsclient/sktclient 全在，SKT ok n= 断链点
# 标记；vnet 服务链尾三标记必收，echod/TCP 链死亡必须 loud）。fw_cfg 具名项
# opt/berry/soak 门控（缺席 = run/smoke/EFI 零扰动；EFI 路无 fw_cfg 具名文件
# 天然缺席）；run-rootfs 全量形态 + 对端三件套（18080-18083——缺席则 bsh
# 卡 SKT 无帽 accept、fat 族整族断〔S-5d 判例〕）；首拍潜伏 5 周期
# （≈300-600s wall，让 boot 尾部帧回收瞬态落在 pages 基线之前——3 周期
# ≈182s 实测恰贴尾部末段，宿主负载即失保证）。帽不含 boot 分量：
# (N+5)×150+120 s、boot 帽 600（S-5e m7 勘误旧「300+(N+5)×150+120」
# 双计 boot 面）。不进 CI（小时级，周跑/手跑）；勿与 smoke/run-rootfs
# 并行（18080-18083 端口面；盘名 disk.soak.img 与 smoke/soak-smp 的
# disk.img/disk.soaksmp.img 三方零交集）。默认 60 分钟（设计 ≥1h 目标，
# 本机实测 ≈1h）；短验 `just soak 2`（本机实测 ~6-7min）。SOAK_KEEP_LOG=1
# 留日志（FAIL 恒留）。参数经 `{{quote(minutes)}}` 引号包裹传递（对抗审查
# finding 9：裸 `{{minutes}}` 插值使 `just soak '60 && …'` 在组壳层注入
# 执行——quote 后载荷作为单 argv 落进脚本的 *[!0-9]* 数字拒面）。
soak minutes="60": build
    {{BASH}} tools/soak.sh {{quote(minutes)}}

# S-5d 双核满载 soak 单口（docs/SMP-S5实施设计草案-v1.0.md §2 D1/D2/D4）：soak.sh
# 全量形态 + `-smp 2` + 双 fw_cfg 门控（opt/berry/soak 心跳 v2.1 九字段 +
# opt/berry/soakload = 内核门控 EL0 churn 舰队 root 家核 + 2 写者 spread→
# CPU1——sw1 双臂断言物理源（严格递增 ∧ Δsw1 ≥320/拍载荷活性主判〔口径 =
# 双写者同时在席 ≥57%，320 = 几何中点 √(560×195)≈330 收 10：净态写者互抢
# ≈560/拍 vs 死态残余 ≈195/拍 B 探针实测；勿抬 450〕；gen 预算 1s 摊频纪律
# 见脚本头注）。八断言 = 单核六断言换面：sw1/park1 恒零自证 → sw1 双臂 +
# 双核 park ∈ [首拍,首拍+256]（越窗守卫——死态 park 亦停增非充分谓词，
# 主判在 Δsw1）+ churn 严格递增（v2.1 第 9 字段 = 载荷 vmo_create 成功数
# ——S-5e M1 端到端防假绿：写者被杀/svc 恒败/gen 顶格时调度面照绿唯 churn
# 冻红）+ 串口 G1.5 kill 零容忍 + 均拍下界 40s。盘名 disk.soaksmp.img 三方
# 零交集；echod 18080-18083 端口面同 soak——
# 勿与任何同端口通道并行。默认 30 分钟（验收线 ≥30min 满载零断言失败）；
# 30min 级实跑**后台执行须显式 timeout ≥7200000ms**（默认帽恰 30min 必超
# ——S-5 草案 §3 明文）。短验 `just soak-smp 2`；SOAK_KEEP_LOG=1 留日志。
# 参数经 quote 包裹（soak finding 9 同款注入面）。
soak-smp minutes="30": build
    {{BASH}} tools/soak-smp.sh {{quote(minutes)}}

size-release: docs-guard abi-check build-user
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

# 版本一致性验收（版本管理标准 §3 G-ver-3/4）：v* tag ↔ 指向提交的
# workspace version 对拍 + 构建标识（git_ident/dirty 位）机制在席。
# 发版前手跑——脚本门已轻挂 CI、不进 build 依赖链（标准 §3「新门挂载」）。
ver-check:
    {{BASH}} tools/ver-check.sh

# 无人值守冒烟（CI 同款入口）：boot 标记断言 + switches 金丝雀严格递增。
# 调试保留串口日志：SMOKE_KEEP_LOG=1 just smoke（M1 起失败自动保留）
smoke: build
    {{BASH}} tools/smoke.sh

# ---------- ADR #84 已立法：BCTS v0 回放（冻结标记集清单对账）----------
# 宿主钉死现役配方树（#82 附录二 Q4 裁定）：复用 build + smoke.sh 全相，
# 零未开工件依赖；berry-assemble（V13）成型后迁组装门禁。
# 缺省 = 活树自检态（清单取 markers.sh 单源）；BCTS_FROZEN=<bcts@*.toml>
# 进回放态（冻结件对账）。清单条目数 < tier1_floor = 空集守卫 loud 拒。
# 独立关卡不并入主 smoke（规则/07 独立关卡纪律同族）。
bcts: build
    {{BASH}} tools/bcts-run.sh

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
build-x86: docs-guard
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
    #!/bin/sh
    set -e
    set -o pipefail
    mkdir -p out/logs
    qemu-system-x86_64 -m 128M -nographic -no-reboot -kernel {{X86_KERNEL}} 2>&1 | tee "out/logs/run-x86-$(date +%s).log"

# x86 独立冒烟（7 标记恰一次；不并入主 smoke——ARM64 四跑零扰动；P7-x4
# 收官起已挂 CI：ci.yml 两条 x86 步 = cargo 直调构建 + 本脚本）。
# 内核路径经 X86_KERNEL 传参（单一事实源——对抗审查实抓 smoke-x86.sh
# 曾自带第二份硬编码：cargo-target 是 gitignored 本地长存目录，后续切片
# 改名/挪 bin 时只改 justfile 一侧会让冒烟继续跑旧 ELF 假绿，G-eco-b
# 「陈旧产物静默上线」同族）。
smoke-x86: build-x86
    {{BASH}} tools/smoke-x86.sh "{{X86_KERNEL}}"

# S-2b 副核参与冒烟——八相（`docs/SMP-S2实施设计草案-v1.0.md` §5.1 起源双相，现
# 八相——计数陈账 sim-S3 顺手矫正）：相 1 裸相（32 标记 + SMP 握手）+
# 相 2 rootfs 生态相（标记数随 markers.sh 单源，现 56 + EVS/SKT 对拍
# client + PAR absent d1 副核参与活证——真并行 witness 归 S-3）。
# 断言面：`SMP ok ncores=2` 按相各恰一次 + 标记与 smoke.sh 计数对拍机器
# 守卫 + 金丝雀按相严格递增；时限 = 单核 ×2（双核 MTTCG 0.61x 吞吐），
# 脚本尾打 R3 定标三数（首跑定标义务）。内核路径经 KERNEL 传参（单一
# 事实源——smoke-x86 先例）。**build-user 依赖（§5.1 随迁件②）**：相 2
# 消费 rootfs*.cpio——新环境/worktree 缺产物时 loud 快败不烧 840s 帽。
# **-smp 只进本线**（与 run-smp 同注：主 smoke 族一律单核）。
smoke-smp: build build-user
    {{BASH}} tools/smoke-smp.sh "{{KERNEL}}"

# V-4 img-boot 独立冒烟（M-G1 关卡；smoke-x86 同款形态——不并入主 smoke，
# ARM64 五跑零扰动，M-G1 收官时另裁升第六跑）。零 -kernel / 零 fw_cfg：AAVMF
# 挂 berryos.img 自举全链——EFI 十段恰一次 + 接续 src=handoff + 裸相 32 +
# EFI 路生态（随 smoke-image.sh ECO 单源，现 52）+ kfile/init 字节对拍 + 金丝雀严格递增 + RX 注入 + 注入后
# 活性复验。镜像/固件路径经参注入（单一事实源——smoke-x86 先例）。已挂 CI
#（ci.yml 步：Ubuntu 侧经 AAVMF_CODE/AAVMF_VARS 环境变量覆盖 brew 默认路径）。
smoke-image: image
    {{BASH}} tools/smoke-image.sh "{{IMAGE}}" "{{AAVMF_CODE}}" "{{AAVMF_VARS}}"

# GUI-8 段④ EFI GUI 交互相双 boot 冒烟（独立关卡；**不并入 smoke-image**，
# 冻结十段/生态（随 smoke-image.sh ECO 单源，现 52）断言面零改动）。拓扑 = AAVMF + out/berryos.img + 独立
# mkfat FAT12 数据盘（LBA0 `BERRYOSVBLK!` + 根 `GUI8.SMK`，payload 必须
# 逐字节 = `BERRYOS-GUI8-SMOKE-V1\n`；错/缺 payload 一律 fail closed）+
# 256M + ramfb + display fw_cfg 800x600@60 + virtio-blk + virtio-tablet +
# QMP unix sock：
#   Boot A：等基础 5 + boot 批 `app APPDEMO gui deferred`（裁决 B 入册不拉起）→ 点击后 `app APPDEMO up` 恰 1 + `launcher: click ready`
#           强制在席（缺席 FAIL，不 sleep 回退）→ QMP 单击 APPDEMO 格 →
#           精确 CLICK ok n=1×1 / LAYOUTW ok×1 / LAYOUT ok×0；新增
#           `app APPDEMO up`×1、APPQUIET×0；宿主 LAYOUT.JSN 恰 109 canonical
#           bytes + APPDEMO(0,0,0)/APPQUIET(page=3,row=0,col=1)。
#   Boot B：同一数据盘（不重造）→ 精确 LAYOUT ok n=2×1 / LAYOUTW×0 /
#           CLICK×0，且数据盘 sha256 前后一致（读路径零重写）。
# click ready / CLICK / LAYOUTW / LAYOUT 四门在哨兵支路才打印；缺失/重复/
# 错序均显式 FAIL（不静默跳过）。plumbing 先跑
# `python3 tools/gui8-smoke.py --self-test` / `--dry-run`。
# GUI8_REBUILD=1 强制重建镜像；GUI8_KEEP=1 保留日志/数据盘。
# gui8-smoke：双 boot 交互相验收（精确哨兵 + 精确计数 + canonical 109B）。
gui8-smoke:
    #!/bin/sh
    set -eu
    # 工程纪律刀阶段 2：镜像新鲜度门（规则/16 §2）——manifest 对拍陈旧即
    # 重建（机器根治预烘旧镜像假结论坑）；GUI8_REBUILD=1 保留为「无条件
    # 强制重建」档（原语义降级为冗余档而非唯一防线）。
    if [ "${GUI8_REBUILD:-0}" = 1 ]; then
        just image
    else
        bash tools/image-fresh.sh --ensure
    fi
    set -- --image "{{IMAGE}}" --code "{{AAVMF_CODE}}" --vars "{{AAVMF_VARS}}"
    if [ "${GUI8_KEEP:-0}" = 1 ]; then
        set -- "$@" --keep
    fi
    exec python3 tools/gui8-smoke.py "$@"

# GUI-8 常态隔离（可选验收）：同一镜像/tablet/数据盘拓扑但**不造
# GUI8.SMK**；boot 一次、点击 APPDEMO，断言 spawn 仍发生（新增
# `app APPDEMO up`×1）而 CLICK/LAYOUTW/LAYOUT/click ready 全零——证明
# 标记只在哨兵支路出现、常态点击行为不变。GUI8_KEEP=1 同上。
# gui8-smoke-normal：哨兵缺席的常态点击隔离。
gui8-smoke-normal:
    #!/bin/sh
    set -eu
    # 工程纪律刀阶段 2：镜像新鲜度门（同 gui8-smoke——manifest 对拍陈旧即重建）。
    if [ "${GUI8_REBUILD:-0}" = 1 ]; then
        just image
    else
        bash tools/image-fresh.sh --ensure
    fi
    set -- --image "{{IMAGE}}" --code "{{AAVMF_CODE}}" --vars "{{AAVMF_VARS}}" --normal-boot
    if [ "${GUI8_KEEP:-0}" = 1 ]; then
        set -- "$@" --keep
    fi
    exec python3 tools/gui8-smoke.py "$@"

# GUI-8 活体 app 回收时延回归门（2026-11-21 冻结案测试面补缺）：gui8
# 全臂 APPDEMO 载荷 console=true（out_s==0 结构性跳过 collect_eof）=
# 回收烧帽类缺陷 CI 恒盲。本门单 boot（无 GUI8.SMK）：BROWSER（活体
# console=false 泵 app）图标首击 spawn → 多任务键二击同 id 回收；断言
# eco 行序 it → recycle → killed → it、零 no EOF/exit code、二击时延 ≤
# 首击 + 40s（EOF 烧帽回归 +60s 必爆帽；主修回退 = killed 行缺席 FAIL）。
# gui8-recycle-gate：活体 console=false app 同 id 回收时延门。
gui8-recycle-gate:
    #!/bin/sh
    set -eu
    if [ ! -f "{{IMAGE}}" ] || [ "${GUI8_REBUILD:-0}" = 1 ]; then
        just image
    fi
    set -- --image "{{IMAGE}}" --code "{{AAVMF_CODE}}" --vars "{{AAVMF_VARS}}" --recycle-gate
    if [ "${GUI8_KEEP:-0}" = 1 ]; then
        set -- "$@" --keep
    fi
    exec python3 tools/gui8-smoke.py "$@"

# ---------- V-0a BerryBoot（EFI 骨架；VM 草案 §V-0，AAVMF 引导链）----------
# 独立小工作区（同 x86_64 模式——根 workspace 不含本目录；构建必须显式
# --target，根 .cargo/config.toml 锁的 build.target 跨 workspace 边界仍命中）。
# 构建链四步（路线修正记录 → docs/阶段沿革.md V-0a 条目）：cargo staticlib
# （builtin aarch64-unknown-none，免 nightly）→ rust-lld 静态全链接
# （--emit-relocs 保留重定位 + berryboot.ld 钉单 .img @0x1000、--image-base
# 0 ⇒ RVA==vaddr）→ elf2pe（Rust 现役件，b 层 #8 三刀换轨）转 PE32+（AArch64 EFI application；「staticlib
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
# elf2pe 换轨（彻底 Rust 化轨道 b 层 #8 三刀）：产链现役件 = tools/elf2pe
# （Rust 独立小工作区，#421 首刀件）；Python 版已降格 elf2pe_oracle.py
# （对拍取证专用，不进产链）。注意：本配方 `cd bootloader/berryboot` 后
# 相对路径失效——elf2pe bin 在 cd 前先编（cwd=仓库根，elf2flat build-user
# 同款纪律：ROOT_ABS 取 $(pwd)、CARGO_TARGET_DIR 显式钉防全局 target-dir
# 覆盖漂移、显式 --target host 顶根 config 锁的 build.target）。
build-boot:
    #!/bin/sh
    set -e
    ELF2PE="$(pwd)/tools/elf2pe/cargo-target/{{host}}/release/elf2pe"
    CARGO_TARGET_DIR="$(pwd)/tools/elf2pe/cargo-target" \
        cargo build --release --manifest-path tools/elf2pe/Cargo.toml \
        --target {{host}}
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
    "$ELF2PE" out/berryboot.elf out/BERRYBOOT.EFI

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
    # mkimage 换轨二刀（b 层 #8 刀序第三位）：产链位切 Rust 件——先编后用
    # + CARGO_TARGET_DIR 钉 tools/mkimage/cargo-target（elf2flat/mkcpio/
    # mkfat 同款纪律）；Python mkimage.py 不退役——gensourceindex import
    # build_image/load_source_instance 模块面（FAT 走 mkfat 单编码器）+
    # CLI 对拍 oracle（test-host 挂双侧，tools/mkimage cargo test 行）。
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    CARGO_TARGET_DIR="$PWD/tools/mkimage/cargo-target" \
        cargo build --release --manifest-path tools/mkimage/Cargo.toml \
        --target "$HOST"
    "$PWD/tools/mkimage/cargo-target/$HOST/release/mkimage" {{IMAGE}} \
        --efi {{BOOT_EFI}} \
        --hello {{BOOT_DIR}}/hello.txt \
        --kernel {{KERNEL}} \
        --initrd rootfs.cpio \
        --size-mib 96

# 宿主设备壳（ADR #62）：tools/berry = host-only 独立 workspace。根 .cargo/config.toml
# 把 build.target 锁成 aarch64-unknown-none，故必须显式 --target <host>；裸 cargo build
# 会编到内核 target。Cargo.lock 随仓锁定 host 依赖（winit/softbuffer/clap/serde_json）。
berry-build: docs-guard
    #!/bin/sh
    set -e
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    cd tools/berry
    PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR="$PWD/cargo-target" \
        cargo build --release --target "$HOST"

# 设备壳 CLI：just berry doctor / run / shot / shell / input ...
berry *ARGS: berry-build
    #!/bin/sh
    set -e
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    exec tools/berry/cargo-target/$HOST/release/berry {{ARGS}}

# 双击/手跑入口：手机比例窗口（P1 先 letterbox 800×600，P2 客机 480×1000）。
shell: berry-build
    #!/bin/sh
    set -e
    # GUI 优先：主镜像（含 gui6 launcher）在位 → just desktop（数据盘/tablet/
    # 真实输入/display fw_cfg）。缺席才回落 console 窗口（P2 手机窗口）。
    if [ -f out/berryos-portrait.img ] || [ -f out/berryos-hd.img ] || [ -f out/berryos.img ] || [ -f out/berryos-visual.img ] || [ -f out/berryos-desktop.img ]; then
        exec just desktop
    fi
    test -f out/berryos.img || { echo "缺 out/berryos.img——先 just image"; exit 1; }
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    exec tools/berry/cargo-target/$HOST/release/berry shell --image out/berryos.img

# GUI 桌面预览：gui8 桌面镜像 + FAT12 数据盘（launcher 需要 BLK_OK 才拿
# fs/app/input 三 gift）+ tablet。主分支 console 镜像走 just shell。
desktop: berry-build
    #!/bin/sh
    set -e
    # 16:9 HD 镜像优先；视觉基线/主镜像/旧预览件依次回退。
    IMG=out/berryos-portrait.img
    [ -f "$IMG" ] || IMG=out/berryos-hd.img
    [ -f "$IMG" ] || IMG=out/berryos-visual.img
    [ -f "$IMG" ] || IMG=out/berryos.img
    [ -f "$IMG" ] || IMG=out/berryos-desktop.img
    test -f "$IMG" || { echo "缺 out/berryos.img（先 just image）或 out/berryos-desktop.img"; exit 1; }
    # M2 第 4 步：数据盘复用制——设备壳 profile 路径的持久盘一次造、跨
    # 启动复用（berry 挂载侧本就非 snapshot 只挂不造；此前每次 mkfat 重造
    # 是持久性被吃掉的唯一根因）。BERRY_ROOT 走 `berry home` 子命令单源
    # （BERRY_HOME 覆写与 Windows 分支只活在 berry——五轮挂账收敛，勿复制）；
    # smoke/CI 路径不经本 recipe，确定性面零扰动。
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    BIN=tools/berry/cargo-target/$HOST/release/berry
    BERRY_ROOT="$($BIN home 2>/dev/null || echo "${BERRY_HOME:-$HOME/.berry}")"
    DATA="$BERRY_ROOT/data/desktop.img"
    mkdir -p "$BERRY_ROOT/data"
    if [ ! -f "$DATA" ]; then
        # mkfat 换轨二刀：产链位切 Rust 件（先编后用，cargo-target 钉位
        # 纪律同 elf2flat/mkcpio；字节恒等由 test-host oracle 对拍腿背书）。
        CARGO_TARGET_DIR="$PWD/tools/mkfat/cargo-target" \
            cargo build --release --manifest-path tools/mkfat/Cargo.toml \
            --target "$HOST"
        "$PWD/tools/mkfat/cargo-target/$HOST/release/mkfat" "$DATA"
        printf 'BERRYOSVBLK!' | dd of="$DATA" conv=notrunc status=none
    fi
    # display fw_cfg 是 gui 位真值源：不传则 guifbd console 不静音、桌面被日志盖。
    BERRY_DATA_DISK="$DATA" exec "$BIN" shell --image "$IMG" --profile desktop --memory 256

# P0 宿主面冒烟：doctor + 内核 marker + EFI 截图（不建窗口；TCG 确定性）。
berry-smoke: berry-build
    #!/bin/sh
    set -e
    test -f out/berryos.img || { echo "缺 out/berryos.img——先 just image"; exit 1; }
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    BIN=tools/berry/cargo-target/$HOST/release/berry
    "$BIN" doctor --json
    "$BIN" run --marker "BerryOS boot" --timeout 30
    "$BIN" shot --image out/berryos.img --out out/berry-shot.png --timeout 120

# P0 VNC 面冒烟（2026-11-13 VNC 三波沉淀）：对外端点 + ws 面起档，vnc status
# 断言 uri/ws_uri，vnc check 口令三态（缺省读 vnc.pass / 对口令 / 错口令
# 期望 rc=7）+ --ws 浏览器通路自检（ServerInit 深度，纯 CLI 不拉帧）。
# 纯 CLI 链、自动端口（不与并行会话撞号）、run 全程显式
# --run 并自收；本配方钉明文零回归，TLS/VeNCrypt 面由 berry-vnc-tls-smoke
# 覆盖；页面/浏览器行为（noVNC 渲染）仍留手工验收。
berry-vnc-smoke: berry-build
    #!/bin/sh
    set -e
    test -f out/berryos.img || { echo "缺 out/berryos.img——先 just image"; exit 1; }
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    BIN=tools/berry/cargo-target/$HOST/release/berry
    PW=vncsmoke-right
    RUN=$("$BIN" device start --profile phone --image out/berryos.img \
        --vnc on --vnc-ws on --vnc-pass "$PW" --json \
        | python3 -c 'import json,sys; print(json.load(sys.stdin)["run"])')
    echo "run: $RUN"
    cleanup() {
        "$BIN" device stop --run "$RUN" >/dev/null 2>&1 || true
        "$BIN" device cleanup --run "$RUN" >/dev/null 2>&1 || true
    }
    trap cleanup EXIT INT TERM
    "$BIN" vnc status --run "$RUN" --json | python3 -c 'import json,sys; v=json.load(sys.stdin); assert v["enabled"] is True, v; assert v["uri"].startswith("vnc://127.0.0.1:"), v; assert v["ws_uri"].startswith("ws://127.0.0.1:"), v; print("vnc status: uri + ws_uri ok")'
    "$BIN" vnc check --run "$RUN" >/dev/null && echo "vnc check (default vnc.pass): ok"
    "$BIN" vnc check --run "$RUN" --password "$PW" >/dev/null \
        && echo "vnc check (--password right): ok"
    "$BIN" vnc check --ws --run "$RUN" >/dev/null \
        && echo "vnc check --ws (browser ws path, default vnc.pass): ok"
    if "$BIN" vnc check --run "$RUN" --password wrong-vncsmoke-pw >/dev/null 2>&1; then
        echo "vnc check: wrong password unexpectedly succeeded"; exit 1
    else
        RC=$?
        [ "$RC" -eq 7 ] || { echo "vnc check wrong-pw rc=$RC (want 7)"; exit 1; }
        echo "vnc check (wrong password): rc=7 auth failed"
    fi
    cleanup
    trap - EXIT INT TERM

# P0 VNC TLS 面冒烟（wave 5）：自签证书（mktemp 目录，私钥不进仓）→ TLS run
# 起档（--vnc-tls + 口令 + ws）→ status 断言 tls.enabled/wss → vnc check 六路
# （TCP：自动推目录/--ca 显式/--insecure 警告/错口令 rc=7，VeNCrypt+rustls
# 全链；ws：wss 正路 + 错 CA 负例 rc=7——严格校验必须真拒绝）。
# 纯 CLI 链、自动端口、run 全程显式 --run 并自收；https 页/wss 代理与 Chrome
# 截图属 t7 手工验收面（需起浏览器），不在本配方。
berry-vnc-tls-smoke: berry-build
    #!/bin/sh
    set -e
    test -f out/berryos.img || { echo "缺 out/berryos.img——先 just image"; exit 1; }
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    BIN=tools/berry/cargo-target/$HOST/release/berry
    PW=vnctlsmoke-right
    TD=$(mktemp -d "${TMPDIR:-/tmp}/berry-vnc-tls.XXXXXX")
    cleanup() {
        [ -n "${WEBPID:-}" ] && kill "$WEBPID" >/dev/null 2>&1 || true
        [ -n "$RUN" ] && "$BIN" device stop --run "$RUN" >/dev/null 2>&1 || true
        [ -n "$RUN" ] && "$BIN" device cleanup --run "$RUN" >/dev/null 2>&1 || true
        rm -rf "$TD"
    }
    trap cleanup EXIT INT TERM
    "$BIN" vnc certs "$TD/certs" >/dev/null
    echo "vnc certs: self-signed set generated in temp dir"
    RUN=$("$BIN" device start --profile phone --image out/berryos.img \
        --vnc on --vnc-ws on --vnc-tls "$TD/certs" --vnc-pass "$PW" --json \
        | python3 -c 'import json,sys; print(json.load(sys.stdin)["run"])')
    echo "run: $RUN"
    "$BIN" vnc status --run "$RUN" --json | python3 -c 'import json,sys; v=json.load(sys.stdin); assert v["enabled"] is True, v; t=v["tls"]; assert t["enabled"] is True, t; assert t["dir"], t; assert v["ws_uri"].startswith("wss://127.0.0.1:"), v; print("vnc status: tls.enabled + wss uri ok")'
    "$BIN" vnc check --run "$RUN" >/dev/null \
        && echo "vnc check (auto cert dir from run meta): ok"
    "$BIN" vnc check --run "$RUN" --ca "$TD/certs" >/dev/null \
        && echo "vnc check (--ca explicit): ok"
    "$BIN" vnc check --ws --run "$RUN" >/dev/null \
        && echo "vnc check --ws (wss, auto cert dir): ok"
    "$BIN" vnc check --run "$RUN" --insecure 2>"$TD/stderr" >/dev/null \
        && grep -q "warning: --insecure skips TLS certificate verification" "$TD/stderr" \
        && echo "vnc check (--insecure): ok + warning"
    if "$BIN" vnc check --run "$RUN" --password wrong-vnctlsmoke-pw >/dev/null 2>&1; then
        echo "vnc check: wrong password unexpectedly succeeded"; exit 1
    else
        RC=$?
        [ "$RC" -eq 7 ] || { echo "vnc check wrong-pw rc=$RC (want 7)"; exit 1; }
        echo "vnc check (wrong password, TLS inner DES): rc=7 auth failed"
    fi
    "$BIN" vnc certs "$TD/other" >/dev/null
    if "$BIN" vnc check --ws --run "$RUN" --ca "$TD/other" >/dev/null 2>&1; then
        echo "vnc check --ws: wrong CA unexpectedly accepted"; exit 1
    else
        RC=$?
        [ "$RC" -eq 7 ] || { echo "vnc check --ws wrong-CA rc=$RC (want 7)"; exit 1; }
        echo "vnc check --ws (wss, wrong CA): rc=7 strict verification rejected"
    fi
    if "$BIN" vnc check --run "$RUN" --ca "$TD/certs" --insecure >/dev/null 2>&1; then
        echo "vnc check: --ca + --insecure unexpectedly accepted"; exit 1
    else
        RC=$?
        [ "$RC" -eq 2 ] || { echo "vnc check --ca+--insecure rc=$RC (want 2)"; exit 1; }
        echo "vnc check (--ca + --insecure): rc=2 mutually exclusive"
    fi
    # F50 尾行（wave 11 TLS×LAN）：同一 TLS run 上 `--lan` 后台起服，断言
    # JSON lan.bind=0.0.0.0 与 lan.upstream=wss（复用活 run，零额外 QEMU；
    # page 口随机，serve 进程自收）。
    "$BIN" vnc web --run "$RUN" --lan --json >"$TD/lan.json" 2>/dev/null &
    WEBPID=$!
    i=0
    while [ $i -lt 50 ] && ! grep -q '"lan"' "$TD/lan.json" 2>/dev/null; do
        i=$((i+1)); sleep 0.1
    done
    kill "$WEBPID" >/dev/null 2>&1 || true
    wait "$WEBPID" >/dev/null 2>&1 || true
    python3 -c 'import json; v=json.load(open("'"$TD"'/lan.json")); l=v["lan"]; assert l["bind"]=="0.0.0.0", l; assert l["upstream"]=="wss", l; print("vnc web --lan (TLS run): bind 0.0.0.0 + upstream wss ok")'
    cleanup
    trap - EXIT INT TERM

# P0 VNC LAN 面冒烟（wave10/11 沉淀，F50 补）：rc=2 拒面三路（无口令 run ×
# --lan / --qr 无 --lan / --lan × --tls 互斥——纯 CLI 秒级）→ 口令档 run 文本
# 模式后台起服：首行 URL 解析端口、curl 环回得 200（0.0.0.0 绑定收环回）、
# /ws 无 Upgrade 得 404、QR 块字符在场且 QR 行不含口令、POST /api/power 无
# X-Berry-Pass 得 403 → 内联 python3 裸 TCP ws Upgrade 过单口代理断言 101 +
# RFB banner（单测全是合成上游——这里是合并版代理 × 活 QEMU 的唯一自动化
# 位）→ --json 二次起服断言 lan.bind=0.0.0.0 与 lan.upstream=ws。隔离
# BERRY_HOME 自起自收；run 全程显式 --run；口令样串只落 /tmp 捕获件不进仓。
berry-vnc-lan-smoke: berry-build
    #!/bin/sh
    set -e
    test -f out/berryos.img || { echo "缺 out/berryos.img——先 just image"; exit 1; }
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    BIN=tools/berry/cargo-target/$HOST/release/berry
    PW=vnclansmoke-right
    # 隔离 home 钉 /tmp（不用 $TMPDIR）：run 目录下的 unix 套接字路径超过
    # macOS sun_path 104 字符上限时 host 子进程起不来（/var/folders 基座
    # 实测 rc=6「host exited before announcing run」）。
    export BERRY_HOME=$(mktemp -d /tmp/berry-vnc-lan-home.XXXXXX)
    TD=$(mktemp -d "${TMPDIR:-/tmp}/berry-vnc-lan.XXXXXX")
    RUN=
    RUN2=
    cleanup() {
        [ -n "${WEBPID:-}" ] && kill "$WEBPID" >/dev/null 2>&1 || true
        for r in "$RUN" "$RUN2"; do
            [ -n "$r" ] && "$BIN" device stop --run "$r" >/dev/null 2>&1 || true
            [ -n "$r" ] && "$BIN" device cleanup --run "$r" >/dev/null 2>&1 || true
        done
        rm -rf "$TD" "$BERRY_HOME"
    }
    trap cleanup EXIT INT TERM
    # 拒面 1：无口令 run × --lan（准入矩阵行 4；--vnc-ws on 供 read_listen 过）。
    RUN2=$("$BIN" device start --profile phone --image out/berryos.img \
        --vnc on --vnc-ws on --json \
        | python3 -c 'import json,sys; print(json.load(sys.stdin)["run"])')
    if "$BIN" vnc web --run "$RUN2" --lan >/dev/null 2>&1; then
        echo "vnc web --lan: passwordless run unexpectedly accepted"; exit 1
    else
        RC=$?
        [ "$RC" -eq 2 ] || { echo "passwordless --lan rc=$RC (want 2)"; exit 1; }
        echo "vnc web --lan (passwordless run): rc=2 refused"
    fi
    "$BIN" device stop --run "$RUN2" >/dev/null 2>&1 || true
    "$BIN" device cleanup --run "$RUN2" >/dev/null 2>&1 || true
    RUN2=
    # 口令档 run 承载其余两面与起服断言。
    RUN=$("$BIN" device start --profile phone --image out/berryos.img \
        --vnc on --vnc-ws on --vnc-pass "$PW" --json \
        | python3 -c 'import json,sys; print(json.load(sys.stdin)["run"])')
    echo "run: $RUN"
    # 拒面 2/3：--qr 无 --lan（arg 门）/ --lan × --tls 互斥（准入矩阵行 3）。
    if "$BIN" vnc web --run "$RUN" --qr >/dev/null 2>&1; then
        echo "vnc web --qr without --lan unexpectedly accepted"; exit 1
    else
        RC=$?
        [ "$RC" -eq 2 ] || { echo "--qr without --lan rc=$RC (want 2)"; exit 1; }
        echo "vnc web --qr (no --lan): rc=2 usage gate"
    fi
    if "$BIN" vnc web --run "$RUN" --lan --tls "$TD/certs" >/dev/null 2>&1; then
        echo "vnc web --lan --tls unexpectedly accepted"; exit 1
    else
        RC=$?
        [ "$RC" -eq 2 ] || { echo "--lan --tls rc=$RC (want 2)"; exit 1; }
        echo "vnc web --lan --tls: rc=2 mutually exclusive"
    fi
    # 文本模式起服（--host 127.0.0.1 钉死 URL 主机面，端口缺省自动捡空口）。
    "$BIN" vnc web --run "$RUN" --lan --host 127.0.0.1 >"$TD/lan.txt" 2>"$TD/lan.err" &
    WEBPID=$!
    i=0
    while [ $i -lt 50 ] && ! head -1 "$TD/lan.txt" 2>/dev/null | grep -q '^http://'; do
        i=$((i+1)); sleep 0.1
    done
    URL=$(head -1 "$TD/lan.txt")
    PORT=$(printf '%s\n' "$URL" | sed -n 's|^http://127\.0\.0\.1:\([0-9][0-9]*\)/.*$|\1|p')
    [ -n "$PORT" ] || { echo "vnc web --lan: no URL in output"; cat "$TD/lan.txt" "$TD/lan.err"; exit 1; }
    echo "vnc web --lan: serving, page port $PORT"
    CODE=$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/")
    [ "$CODE" = "200" ] || { echo "GET / rc=$CODE (want 200)"; exit 1; }
    CODE=$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/ws")
    [ "$CODE" = "404" ] || { echo "GET /ws without Upgrade rc=$CODE (want 404)"; exit 1; }
    CODE=$(curl -s -o /dev/null -w '%{http_code}' -X POST "http://127.0.0.1:$PORT/api/power/reboot")
    [ "$CODE" = "403" ] || { echo "POST /api/power without X-Berry-Pass rc=$CODE (want 403)"; exit 1; }
    grep -q '[█▀▄]' "$TD/lan.txt" || { echo "QR block missing from --lan text output"; exit 1; }
    if grep '[█▀▄]' "$TD/lan.txt" | grep -q "$PW"; then
        echo "QR lines leak the password"; exit 1
    fi
    echo "vnc web --lan: page 200 / ws 404 / api 403 / QR present + password-free"
    # 单口 /ws 代理 × 活 QEMU：裸 TCP Upgrade → 101 + RFB banner（真上游，
    # 非单测合成面）。
    python3 - "$PORT" <<'PY'
    import socket, sys
    port = int(sys.argv[1])
    s = socket.create_connection(("127.0.0.1", port), timeout=10)
    s.sendall(b"GET /ws HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n")
    buf = b""
    while b"RFB " not in buf:
        chunk = s.recv(4096)
        if not chunk:
            sys.exit(f"EOF before RFB banner: {buf[:120]!r}")
        buf += chunk
    assert buf.startswith(b"HTTP/1.1 101"), buf[:80]
    line = buf[buf.index(b"RFB "):].split(b"\n")[0].decode()
    print(f"ws proxy through live QEMU: 101 + {line}")
    PY
    kill "$WEBPID" >/dev/null 2>&1 || true
    wait "$WEBPID" >/dev/null 2>&1 || true
    WEBPID=
    # --json 二次起服：lan.bind=0.0.0.0 与 lan.upstream=ws（明文 run 面）。
    "$BIN" vnc web --run "$RUN" --lan --host 127.0.0.1 --json >"$TD/lan.json" 2>/dev/null &
    WEBPID=$!
    i=0
    while [ $i -lt 50 ] && ! grep -q '"lan"' "$TD/lan.json" 2>/dev/null; do
        i=$((i+1)); sleep 0.1
    done
    kill "$WEBPID" >/dev/null 2>&1 || true
    wait "$WEBPID" >/dev/null 2>&1 || true
    WEBPID=
    python3 -c 'import json; v=json.load(open("'"$TD"'/lan.json")); l=v["lan"]; assert l["bind"]=="0.0.0.0", l; assert l["upstream"]=="ws", l; assert v["url"].startswith("http://127.0.0.1:"), v["url"]; print("vnc web --lan --json: bind 0.0.0.0 + upstream ws ok")'
    cleanup
    trap - EXIT INT TERM

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
    set -o pipefail
    python3 -c "import sys; sys.stdout.buffer.write(b'\xff'*(64<<20))" > {{BOOT_OUT}}/vars.fd
    mkdir -p out/logs
    {{QEMU}} -machine virt -cpu cortex-a72 -m 128M -nographic \
        -pflash "{{AAVMF_CODE}}" -pflash {{BOOT_OUT}}/vars.fd \
        -drive if=none,file={{IMAGE}},format=raw,id=bb \
        -device qemu-xhci -device usb-storage,drive=bb \
        -device ramfb \
        2>&1 | tee "out/logs/run-efi-$(date +%s).log"
