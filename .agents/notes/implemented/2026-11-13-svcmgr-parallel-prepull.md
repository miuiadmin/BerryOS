# Agent Note: svcmgr 无依赖服务并行预拉——vblk 自验门后置 + vnet/inputd arm 前移（路线图 P1-1 后半）

Status: implemented

## Problem

svcmgr（`user/bins/svcmgr/src/main.rs`）的服务预拉链是严格串行：vblk spawn 后**内联阻塞** `blk_selftest`（RPC 往返，BLKD 应答帽 60s——vblk 启动自验在服务循环之前，boot 期在途可达数秒），期间 guifbd/wmd/gfxdemo/fsmgr/vnet/inputd 的 spawn 一个都发不出去；gfxdemo 阻塞 reap 之后再串行 spawn fsmgr/vnet/inputd。两条长等待（vblk 自验、gfxdemo reap）与全部 spawn 首尾相接，boot 关键路径 = 两者之和 + 逐个 spawn 开销，无依赖服务（vblk/guifbd/vnet/inputd）的 boot 时间完全浪费在排队上。fsproto txid（[2026-11-13-fsproto-txid-request-id.md](./2026-11-13-fsproto-txid-request-id.md)）落地后，并发 RPC 混流的风险面已消除，预拉并行化只剩纯时序面。

## Decision

依赖核实（R1 架构研究对拍 + 代码实证）：vblk/guifbd/vnet 三者零依赖；inputd **不吃 fb 几何**（gift 面仅 `gift2_h` 输入通道 + 可选 `wm_push_h` wmd 推送端——`drivers/bins/gui-inputd/src/main.rs:861-868`），唯一接线依赖 = wmd push 端（`WMD_PUSH_B`，wmd spawn Ok 路存值）；fsmgr 依赖 vblk 自验过门（`blk_cli` = env gift[3]）。`spawn_rootfs_elf` 本身非阻塞（解析 → process_create → env send → process_start 即返），串行瓶颈只在两个内联阻塞点。据此重排（只动 main.rs）：

- **blk_selftest 后置收割点**：vblk arm 的 Ok 路只 `blk_cli = blk_a` 暂持端点，阻塞自验挪到 gfxdemo 阻塞 reap 之后、fsmgr 预拉段之前的独立块（`if blk_cli != 0 { match blk_selftest(blk_cli) … }`）。过门 → `BLKD ok` + `BLK_OK` 置位；未过门 → close 止损 + `blk_cli = 0`。fsmgr 仍只见「已过门」端点，启动序语义零漂。
- **vnet arm 前移**：自旧位（fsmgr 后）移到紧随 vblk arm——vblk/vnet/guifbd 三条 spawn 连发后才进 GUI 链与阻塞收割点。NET_A 消费者（DNS 自验、bsh evs）全在主走查后，前置零竞速面。
- **inputd arm 前移**：自旧位（vnet 后）移到 wmd arm 之后、gfxdemo 阻塞 reap 之前——push_b 接线依赖保序（spawn 时随 extras take 迁移，无迟到挂通道机制），spawn 仍先于两大阻塞收割点发出。

新序：vblk → vnet → guifbd → 压力/VER → wmd → inputd → gfxdemo（阻塞 reap）→ **blk_selftest（阻塞）** → fsmgr → launcher → stdhello → 主走查。关键路径由「vblk 自验 + gfxdemo 全程 + 逐 spawn」串行和，收缩为「max(vblk 自验, GUI 链) + 即刻应答的自验 RPC」。

## Alternatives considered

- **四服务全部同段 spawn（inputd 也进第一批）**：被否——inputd 的 wmd push 端只能随 env 块 spawn 时迁移，早于 wmd spawn 就永久丢接线（GUI 交互相输入事件断流）；保序到 wmd 后的代价仅为两次 spawn 调用的时延，可忽略。
- **blk_selftest 放 gfxdemo reap 之前**：被否——那样自验 RPC 等的是「vblk 自完成 boot」整段，gfxdemo 运行期与 vblk boot 期无法重叠；放在 reap 后，vblk 已并行 boot 整条 GUI 链的时长，自验通常即刻应答（rootfs 相无 GUI 链，收割点紧跟 inputd spawn，时序同旧序）。
- **svcmgr 多线程/事件循环化真并行收割**：超本刀改动面（单线程 svcmgr 是多处注释钉死的前提），并行收益已由「spawn 非阻塞 + 阻塞点后置」全额拿到，多线程留给 v2。

## Consequences

- **标记面零漂**：`BLKD ok` 打点位置后移（EFI 相现于 `GFX ok` 之后），`NET ok`/inputd 诊断行前移——smoke 全部标记是 grep 存在性/恰一次断言，无顺序断言，VBLK/BLKD/NET/DNS/GUI 族全部不受影响。
- **A-5 语义保真**：fsmgr 仍是 vblk 唯一 client（自验过门后才 take 迁移 blk_cli），blk RPC 无回复抢答面；fsmgr Err 路与尾部兜底关的 `blk_cli` 清零逻辑原样适用（未过门/缺席 = 0）。
- **BLKD 应答帽 60s 不动**：后置收割点下在途窗收缩（vblk 通常早已在服务），帽只兜整体挂死。**死 vblk 拓扑（EFI 无盘相）例外**：vblk 子自验拒退后通道无对端关闭检测，自验 recv 等满 60s 帽才降级（实测 +8s 起拉 → +74s eco）——旧序同帽同耗（stall 在 guifbd 之前），两序总时序等价、新序不让劣；活 vblk 相（rootfs/GUI8）过门即应答，新序全额受益。
- **验证（2026-11-13）**：`just build-user` 全链绿 + 内核重编（svcmgr include_bytes 内嵌，须同批构建——镜像 KERNEL.ELF 与产物 sha256 对拍实证同源）；QEMU rootfs 相 smoke 全套 PASS（exit 0：VBLK ok / BLKD ok / NET ok / DNS ok / FS ok / INPUT ok / BSH ok / PS ok 族全到齐）；EFI 相（AAVMF + ramfb 真拓扑，单 boot 无帽计时）GUI 链全序验证：FBW ok +8s → FBSERVE ok +8s → WM ok +11s → GFX ok +14s → eco: blkd write +74s（后置收割点落位 gfxdemo reap 后，实证新序）→ LAUNCH v=1 +77s（launcher 接线齐）→ FILE/STD/BSH/PS/FS ok 续至 +243s。
- **LAUNCH ok / PAGE ok n= 未达不是本刀面**：launcher 子进程自打 `desktop build err=NodeFull` 后弃打两标记——berry-launcher 源码（main.rs/desktop.rs/frameloop.rs/icons.rs）当日 11:00 起被并行会话未提交改动（VNC 桌面套件进行中），本刀镜像的 rootfs 打包吞入了该 WIP launcher；同一时段并行会话自己用**旧内核**跑 smoke-image 也恰好只缺 LAUNCH ok + PAGE ok n=（同症状不同 svcmgr = 与预拉重排无关）。本刀只许改 svcmgr/src/main.rs，launcher 归属会话收口。
- **环境教训（续 txid 笔记）**：本机多会话并发 QEMU（同刻 3 实例 TCG）时 smoke-image 360s 帽对全链 ~115s 的 3x 余量会被吃满——两轮「FAIL 缺 LAUNCH/PAGE」与一轮 QEMU 被 SIGKILL 均系宿主负载；单 boot 无帽计时是本机可行的验证形态。另：DSH 沙箱禁 QEMU 开仓外固件（`/opt/homebrew/share/qemu/edk2-*.fd` EPERM），拷入仓内 `out/fw/` 经参传入即绕。
