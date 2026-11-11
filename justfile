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

default: run

# G-eco 编译侧：用户程序 → flat binary（草案 D6）。builtin aarch64-unknown-none
# + 各 bin 自带链接脚本（build.rs 绝对路径注入）——**免 nightly**（custom target
# 推迟到确需 target 级差异再上，草案 D1 修订）。elf2flat.py 解 PT_LOAD 出平像
# 并补零到页（.bss 覆盖在文件侧兑现）。两遍构建（G-eco-b）：pass 1 先产
# echo/talker/thrchild/dpgp 平像进 user/userbins/，pass 2 svcmgr 才 include_bytes!
# 内嵌（单遍 workspace 无编排保证，鸡生蛋只能拆两刀）。--max 墙：子程序
# 4096（2 页映射 [0x1000,0x3000) 里首页装镜像、次页是栈——越页即吃栈）；
# svcmgr 131072 = UserProcess::build 的 D3 帽（A7-4 段② 64→128KiB 同步抬
# ——libc 信号面并入后平像 69.6KB 超 64KiB 旧帽）；T-1 thrchild 16384（4 页墙
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
    # GUI-0（2026-10-29）：user 侧 none-target 家族（echo/talker/thrchild/svcmgr/
    # abuser/hello）**保留 NEON codegen**——EL0 任务 FP 态已由 TaskContext 全量保存
    # 兜底、GUI 光栅后续正要用 SIMD；根 .cargo/config.toml 的 -neon 旗只属内核
    # SIMD-free 合同。cwd=user/ 会向上吃到根 config，env RUSTFLAGS 优先于 config
    # → 显式清空还原用户仓 codegen（零漂移；musl 面本就无旗）。
    export RUSTFLAGS=""
    cd user
    CARGO_TARGET_DIR=cargo-target cargo build --release -p echo -p talker -p thrchild -p dpgp
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
    # G4-2：dpgp（demand 装载受验子——跨进程供给形 EL0 侧，bootstrap=0 零
    # 句柄；2 页墙 = 像 1 页 + 头寸，像窗 [0x1000,0x6000) 外 0x6000 起
    # demand 页由 svcmgr dpg_selftest 另映）。同产链内嵌 svcmgr。
    python3 ../tools/elf2flat.py \
        cargo-target/aarch64-unknown-none/release/dpgp \
        userbins/dpgp.bin --max 8192
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
    # GUI-8 视觉补全：appstub 共用静默壳（零 argv0 断言/console=false/0 退出）
    # 复制成 8 个占位 app 的 bin/<name>；真 app 内容后续替换。
    CARGO_TARGET_DIR=cargo-target cargo build --release --manifest-path bins/appstub/Cargo.toml \
        --target aarch64-unknown-none
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
    python3 ../tools/mkapp.py userbins/app/APPDEMO.bap userbins/app-stage/APPDEMO
    python3 ../tools/mkapp.py userbins/app/APPQUIET.bap userbins/app-stage/APPQUIET
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
    python3 ../tools/mkapp.py userbins/app/APPTOMB.bap userbins/app-stage/APPTOMB
    # GUI-8 视觉补全：7 个占位 app（图标/入口填充；内容未实现）。
    for spec in SETTINGS:settings FILES:files PHOTOS:photos CAMERA:camera CONTACTS:contacts MMS:messages BROWSER:browser; do
        id=${spec%%:*}; bin=${spec##*:}
        mkdir -p "userbins/app-stage/$id/bin"
        cp "bins/appstub/apps/$id/manifest.toml" "userbins/app-stage/$id/"
        cp cargo-target/aarch64-unknown-none/release/appstub "userbins/app-stage/$id/bin/$bin"
        cp -R "bins/appstub/apps/$id/icons" "userbins/app-stage/$id/"
        cp -R "bins/appstub/apps/$id/views" "userbins/app-stage/$id/"
        python3 ../tools/mkapp.py "userbins/app/$id.bap" "userbins/app-stage/$id"
    done
    # G-eco-c/d + app-0：rootfs.cpio（hello + stdhello + hexyl + bsh + fail + bench1 + fsmgr + vprobe + vblk + vnet ELF 本体——装载走
    # svcmgr 流式 cpio/ELF 链，非 include_bytes 内嵌；产物在仓库根 =
    # `run-rootfs` 既有约定；条目按名排序 = 确定性输出。app-0 起加
    # app/<ID>.bap 整包条目（'a' < 'b' 排序在 bin/ 前；a-1 阶段 svcmgr
    # 主走查对未知名算术跳过 = 零消费、零行为变化——appd 消费随 a-2）。
    # app-1 D1（调和书）：APPTOMB 打包出 app/ 前缀改 selftest/ 专用位
    # ——不入 bap_scan 收集 = 不占 APP_SLOTS（9/9 恰满，入册必挤 SETTINGS
    # = APP ok 静默漂 8），tomb_selftest 走 cpio 直查，白名单立法退役。
    # V13（ADR #82 预埋刀）：58 条 name=rootfs 硬编映射外化
    # tools/rootfs-manifest.toml（平台基座自描述——加 bin 只改该表；
    # mkcpio --manifest 加载/拒面见其头注）。--split 32 与出档路径留配方。
    python3 ../tools/mkcpio.py ../rootfs.cpio \
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
# 公开镜像形态：docs-guard 依赖 AGENTS.md（内部档不在镜像）——降级 no-op，
# 构建/CI 前置门回到 cargo/just 本体（AGENTS 预算门是内部治理，不约束外部贡献者）
docs-guard:
    @echo "public mirror: docs-guard skipped (internal governance doc absent)" 

# 可写全局清单门禁（Ginux writable-globals 借法）：新增 static mut 必须入册
writable-globals:
    python3 tools/writable_globals.py --check

check: docs-guard writable-globals abi-check build-user
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
# sim-S1（`docs/sim设备通道与契约设计-v1.1.md` §7）：+第二 PL011（serial1 =
# sim 注入通道，unix socket 宿主外接）+ fw_cfg 具名键 `opt/berry/simdev`
#（= UART1 在席门——**serial1 与键同线配对**是拓扑契约：只配键不配 serial
# 则 0x0904_0000 是洞、内核门开即 boot 崩；run/smoke/smoke-image/EFI 不带
# 二者 = 零漂移）。serial0 显式钉 mon:stdio（-nographic 隐式默认的显式形
# ——防唯一 -serial 被解释为 serial0 吞走 console）。注入 client 连
# /tmp/berryos-sim1.sock（M3 smoke 面同路径）；boot 标记 `SIM gate on`。
run-rootfs: build
    #!/bin/sh
    set -e
    test -f rootfs.cpio || { echo "缺 rootfs.cpio（G-eco 编译侧产出）"; exit 1; }
    python3 tools/mkfat.py disk.img
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
    {{QEMU}} {{MACH}} -kernel {{KERNEL}} $FWCFG \
        -global virtio-mmio.force-legacy=false \
        -drive if=none,id=drv0,format=raw,file=disk.img,cache.writeback=off \
        -device virtio-blk-device,drive=drv0 \
        -netdev user,id=n0,hostfwd=tcp::18081-:18081,hostfwd=tcp::18082-:18082 -device virtio-net-device,netdev=n0 \
        -device virtio-tablet-device,id=tab0 \
        -serial mon:stdio \
        -serial unix:/tmp/berryos-sim1.sock,server=on,wait=off \
        -fw_cfg name=opt/berry/simdev,string=1

run-release: docs-guard abi-check build-user
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
    # W1-Q6 前置：bash 3.2 吞名守卫——`$NAME` 后紧邻多字节字符在 macOS
    # 系统 bash 3.2 下吞进变量名（set -u 直炸/否则静默吃值），CI bash 5
    # 免疫故挂账潜伏。tools/*.sh 77 处已改花括号包裹（字节级同义），
    # 本门禁防回归（smoke 本地复现的宿主断点收口）。
    python3 tools/shvarcheck.py --check
    cargo test -p berry-mm --target {{host}}
    # W3-Q2：berry-wqmatch（等待队列唤醒匹配纯函数面——b-1 广播不变量/
    # WAIT_ANY 哨兵/fastpath 单等待者资格三律钉板 + 两路语义对齐穷举，
    # host 9 测；谓词单源，sched wake_matching/find_single_waiter 消费）。
    cargo test -p berry-wqmatch --target {{host}}
    # W3-Q18：berry-dtbface（FDT walker 纯函数面——rd32/cstr_at/prop_u64
    # 界检读 + find_initrd 迭代 walk 状态机：token 推进律//chosen 深度判/
    # 4 对齐步进/畸形九态全 None 零 panic，host 13 测；单源 crate，
    # kernel dtb.rs 壳化消费（boot_probe 物理臂留内核侧））。
    cargo test -p berry-dtbface --target {{host}}
    # W3-Q3：berry-vmoledger（VMO 页面生命周期簿记纯函数面——提交/归还
    # 资格、映射计数悬空 PTE 防线、demand 钉计数、MMIO 窗口恒空不变量，
    # host 13 测；记账状态机单源，kernel/src/vmo.rs 物理臂消费）。
    cargo test -p berry-vmoledger --target {{host}}
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
    # 判据单源，kernel syscall.rs/channel.rs 消费）。
    cargo test -p berry-chanface --target {{host}}
    # W3-Q6：berry-itally（intent 档位计数/通胀计数遥测纯函数面——档位
    # 直方六槽/保留域收敛/borrowed==Σ借入不变量/tele: 行格式（纯 ASCII、
    # 非零档省略、u64 极值界）钉板，host 11 测；tally_key/inflate/tally_line
    # 单源，kernel telemetry.rs 计数面 + main.rs tele_dump 行装配消费）。
    cargo test -p berry-itally --target {{host}}
    cargo test -p berry-sliceface --target {{host}}
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
    # W3-Q20：berry-polface（policy O(1) 快照查表数学 + SchedAttr 位选律
    # 纯函数面——散列锚点指纹/256 键桶覆盖摊平律/饱和窗全占+窗尽缺席/
    # 空槽早退缺席/桶 255 回绕落位/first-wins 不覆写/窗满拒收不入表/
    # build 点夹紧五点/EPP 两位打包往返+域外位/merge 八位全表+FREEZE_READY
    # 特判+flags=0 回默认钉板，host 14 测；常量族与判定律单源，sched
    # policy.rs / kernel process.rs 机械壳 re-export 消费）。
    cargo test -p berry-polface --target {{host}}
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
    cargo test --manifest-path user/Cargo.toml -p berry-loader --target {{host}}
    # X-5 補：berry-winplan（demand 装载窗规划纯函数面——自 berry-exec 外迁
    # 的 plan_wins/cap 参数化；三色窗完备铺满 + congruent 门 (a')-(e') 各
    # 拒面 + musl 族多 k 布局过门 + 合成 ELF 全链，host 16 测）。
    cargo test --manifest-path user/Cargo.toml -p berry-winplan --target {{host}}
    # argv blob 组装律纯面（自 berry-exec env 块组装段外迁，winplan 同款
    # 纪律）：argv0 截 191 律 + 整参或零参单调截断 + 恰满边界 + wire
    # 钉板，host 5 测。
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
    python3 tools/mkfat.py --check-fixture user/berry-fatfs/tests/volume_fixture.bin
    # V-2：FAT16 双宽基准同位守卫（--check-fixture16——一卷一宽双 fixture，
    # 漂移即 FAIL 带 --fixture16 重产提示）。
    python3 tools/mkfat.py --check-fixture16 user/berry-fatfs/tests/volume_fixture_fat16.bin
    cargo test --manifest-path user/Cargo.toml -p berry-fatfs --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-netproto --target {{host}}
    cargo test --manifest-path user/Cargo.toml -p berry-netdproto --target {{host}}
    # sim-S1：berry-pwproto（sim 三设备快照协议纯函数面——帧头/定长快照/
    # ACK/流式再同步解码 + 值域双向执法 + 黄金字节钉板，host 9 测；
    # PM-16 §4.1 契约冻结）。
    cargo test --manifest-path user/Cargo.toml -p berry-pwproto --target {{host}}
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
    # SEC-02 刀 A2：berry-ksproto（keystored 密钥服务 RPC 协议纯函数面——
    # KeyMint 借号段 tag 编号表 add-only 冻结钉板 + AuthorizationList 软硬
    # 双域/SecurityLevel 三档 wire + KEY_GENERATE/KEY_ATTEST/COUNTER 三 op
    # 编解码黄金字节 + 严格串行会话机，host 20 测；独立小工作区绕开
    # user/Cargo.toml——manifest-path 直指 crate，同 stdhello/bsh 先例）。
    cargo test --manifest-path user/berry-ksproto/Cargo.toml --target {{host}}
    # mkapp 门禁自检（a-3 U4：mkfat --check-fixture 同位前置——打包器门禁
    # 面此前只在 build-user 产物链上被动覆盖，门禁自身坏 = 产物链静默劣化）。
    python3 tools/mkapp.py --selftest
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
    # V-5a-0：berrydump（Fusion/AAVMF 设备面事实 dump 纯取证工具——解析
    # 纯函数面 host 钉板：RSDP/XSDT/MADT/SPCR/DBG2/FDT 六表面合成向量 +
    # 表名注册，host 8 测；独立 workspace 与 berryboot 主引导链解耦，
    # 唯一依赖 berry-efi 纯数据 crate）。
    cargo test --manifest-path bootloader/berrydump/Cargo.toml --target {{host}}
    # Q1（a-3 U4 纪律收口）：elf2flat 门禁自检——build-user 产链 6 处消费的
    # 平像转换器（echo/talker/thrchild/dpgp/abuser/svcmgr），此前只在产物链
    # 上被动覆盖，门禁自身坏 = 平像静默劣化。合成双段 ELF（gap 零填 + bss
    # 尾零 + 页取整 + 确定性）+ 8 拒面（elf2pe --selftest 同位纪律）。
    python3 tools/elf2flat.py --selftest
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

    python3 tools/elf2pe.py --selftest
    python3 tools/mkfat.py --selftest
    # A1-3a/3b：mkcpio --split 切分面守卫（重装恒等/恰整数倍/单卷/命名/
    # 拒面/陈卷清理）——rootfs 多卷产链（build-user --split 32）的编码
    # 单源执法，改 mkcpio 必跑。2026-11-11 W1 去重：A1-3a/3b 两刀各接线
    # 一次致同命令跑两遍（实抓），保留单次覆盖面零损失。
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
    # Q1 续（a-3 U4 纪律同款）：genfbfont 门禁自检——三档字体表单源生成器
    # （fbcon 1x + gfx s2/s3），此前只有 --check 漂移守卫（两侧同源时生成器
    # 逻辑改坏仍绿），selftest 另钉生成器语义：结构契约+拒面 5、NN 放大
    # 数学性质、'!'/'=' 手算字节 ×3 档、表头契约、在册产物对偶。
    python3 tools/genfbfont.py --selftest
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
    # SDK-2 P0-1：berry-crypto（.sig 信封 v1 TLV + 信任账本 TOML + 水位比较器
    # + RFC 8032 向量门空转框架——对选型零敏感件先行，26 号 §3-2；原语本体 =
    # P0-2 另刀，向量门不过不合入；P0 合入 = 闸 B，24 号 §4.2 终核三条）。
    cargo test --manifest-path user/Cargo.toml -p berry-crypto --target {{host}}

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

# S-5d 双核满载 soak 单口（docs/SMP-S5实施设计草案.md §2 D1/D2/D4）：soak.sh
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
# 调试保留串口日志：SMOKE_KEEP_LOG=1 just smoke
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
    qemu-system-x86_64 -m 128M -nographic -no-reboot -kernel {{X86_KERNEL}}

# x86 独立冒烟（7 标记恰一次；不并入主 smoke——ARM64 四跑零扰动；P7-x4
# 收官起已挂 CI：ci.yml 两条 x86 步 = cargo 直调构建 + 本脚本）。
# 内核路径经 X86_KERNEL 传参（单一事实源——对抗审查实抓 smoke-x86.sh
# 曾自带第二份硬编码：cargo-target 是 gitignored 本地长存目录，后续切片
# 改名/挪 bin 时只改 justfile 一侧会让冒烟继续跑旧 ELF 假绿，G-eco-b
# 「陈旧产物静默上线」同族）。
smoke-x86: build-x86
    {{BASH}} tools/smoke-x86.sh "{{X86_KERNEL}}"

# S-2b 副核参与冒烟——八相（`SMP-S2实施设计草案.md` §5.1 起源双相，现
# 八相——计数陈账 sim-S3 顺手矫正）：相 1 裸相（32 标记 + SMP 握手）+
# 相 2 rootfs 生态相（51 标记 + EVS/SKT 对拍
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
# EFI 路生态 36 + kfile/init 字节对拍 + 金丝雀严格递增 + RX 注入 + 注入后
# 活性复验。镜像/固件路径经参注入（单一事实源——smoke-x86 先例）。已挂 CI
#（ci.yml 步：Ubuntu 侧经 AAVMF_CODE/AAVMF_VARS 环境变量覆盖 brew 默认路径）。
smoke-image: image
    {{BASH}} tools/smoke-image.sh "{{IMAGE}}" "{{AAVMF_CODE}}" "{{AAVMF_VARS}}"

# GUI-8 段④ EFI GUI 交互相双 boot 冒烟（独立关卡；**不并入 smoke-image**，
# 冻结十段/生态 36 断言面零改动）。拓扑 = AAVMF + out/berryos.img + 独立
# mkfat FAT12 数据盘（LBA0 `BERRYOSVBLK!` + 根 `GUI8.SMK`，payload 必须
# 逐字节 = `BERRYOS-GUI8-SMOKE-V1\n`；错/缺 payload 一律 fail closed）+
# 256M + ramfb + display fw_cfg 800x600@60 + virtio-blk + virtio-tablet +
# QMP unix sock：
#   Boot A：等基础 5 + boot 批 `app APPDEMO up` → `launcher: click ready`
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
    if [ ! -f "{{IMAGE}}" ] || [ "${GUI8_REBUILD:-0}" = 1 ]; then
        just image
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
    if [ ! -f "{{IMAGE}}" ] || [ "${GUI8_REBUILD:-0}" = 1 ]; then
        just image
    fi
    set -- --image "{{IMAGE}}" --code "{{AAVMF_CODE}}" --vars "{{AAVMF_VARS}}" --normal-boot
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
    python3 tools/mkfat.py out/gui-data.img
    printf 'BERRYOSVBLK!' | dd of=out/gui-data.img conv=notrunc status=none
    HOST=$(rustc -vV | sed -n 's/^host: //p')
    # display fw_cfg 是 gui 位真值源：不传则 guifbd console 不静音、桌面被日志盖。
    BERRY_DATA_DISK=out/gui-data.img exec tools/berry/cargo-target/$HOST/release/berry shell --image "$IMG" --profile desktop --memory 256

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
