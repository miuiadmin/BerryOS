# gfxdemo 四处 VMO 别名缺 RIGHT_TRANSFER——`rpc send err=-13` 修复

Status: implemented

## Problem

desktop 机串口在 `gfx: arb3 hz=120` 之后紧接 `rpc send err=-13`，`gfx: k4a` 及其后所有段（k4s、k6p、k6h）缺席，`GFX ok` 永不出现；`LAUNCH ok` 仍能走到，所以这条红长期只以「异常计数里 `err=-13` 恰 1 次」的形态被记为已知项。根因在 `user/bins/gfxdemo/src/main.rs`：该文件七处 `berry_handle_duplicate(vmo, …)` 里，有三处按 2026-11-11 [RIGHT_TRANSFER 全生态执法](../../../docs/阶段沿革.md)（提交 `2abcfed2`）补了 `RIGHT_TRANSFER` 位，四处仍是 `RIGHT_READ` 单位。执法后 `sys_channel_send` 对每枚随行句柄预检（`kernel/src/syscall.rs:3701-3706` 调 `rights_of` 再过 `hdlface/src/probe.rs:79-85` 的 `probe_transfer_ok`——缺 TRANSFER 位即 `ERR_DENIED`，即 `abi_gen.rs` 的 `ERR_ACCESS_DENIED = -13`）；而 `handle.rs:254-267` 的 `duplicate` 把新柄权利钉为请求值**精确**（只有 `RIGHTS_SAME` 才取原全集），`RIGHT_READ`-only 的别名从出生就没有 TRANSFER 位，随消息发出必被预检拒回。四处别名全部经 `rpc()` → `berry_channel_send(ch, …, hptr, hn)`（`main.rs:161`）随 SUBMIT/PREPARE 消息迁出，无一处只本地使用，所以四处全部命中。旧日志 `/tmp/audit-prod-serial.log` 第 146 行 `arb3` → 第 149 行 `rpc send err=-13` 之间，带非空随行的发送只有 `main.rs:953` 一处（源 `:940` 的 `RIGHT_READ`-only dup），其余发送 `nhandles=0` 结构上不可能从该预检出 -13；`sys_channel_send` 内另一 -13 源（通道缺 `RIGHT_SEND`）会让每一次发送都失败，与 `WM ok`、`GFXF ok` 在席相抵。四处 `RIGHT_READ`-only 的 dup 随 2026-11-19 镜像重建提交 `331fc2d1` 入库——**具体引入提交未证实**（镜像前历史在本仓不可追）；形态上是后加的几段照抄首个 SUBMIT 的 dup 写法时漏了执法补位。

## Decision

四处 `RIGHT_READ` → `RIGHT_READ | RIGHT_TRANSFER`，其它逻辑、文案、标记零改动（4 行，+68 字节）。`RIGHT_TRANSFER` 本已在 `main.rs:62` 导入。七处 dup 的逐点判定（行号为基线 `bf276349` 文件）：

| # | dup 行 | 所在段 | 改前 rights | alias 去向 | 随消息发送 | 处置 |
|---|---|---|---|---|---|---|
| 1 | 284 | `submit_full`（k6 段共用，调用点 1129/1152/1174/1175/1230/1356/1460） | `RIGHT_READ` | 297 行 `rpc(hw, …, &[alias], …)` | 是 | 补 TRANSFER |
| 2 | 611 | main 首个 SUBMIT | `READ\|TRANSFER` | 634 行 `&[alias]` | 是 | 不改 |
| 3 | 940 | k4a 预告帧 SUBMIT | `RIGHT_READ` | 953 行 `&[alias_h]` | 是 | 补 TRANSFER |
| 4 | 1049 | k4s 17 帧循环 SUBMIT | `RIGHT_READ` | 1062 行 `&[alias_i]` | 是 | 补 TRANSFER |
| 5 | 1206 | k6p ④ PREPARE | `RIGHT_READ` | 1217 行 `&[alias_p5]` | 是 | 补 TRANSFER |
| 6 | 1502 | 尾序 PREPARE | `READ\|TRANSFER` | 1513 行 `&[alias2]` | 是 | 不改 |
| 7 | 1542 | 残帧清屏 SUBMIT | `READ\|TRANSFER` | 1555 行 `&[alias3]` | 是 | 不改 |

反向查漏：文件内带非空随行的 `rpc`/`expect_ok` 调用共 10 处——七个 VMO 别名加三个 event（701/860/1531，`event_create` 原柄自带 TRANSFER），没有漏网的 alias，也没有只本地映射却被加权的 alias。只补一位不多授：预检只判 `RIGHT_TRANSFER`（`1<<5`）有无，`READ|TRANSFER` 正好过门；dup 合法性由 `handle.rs:257` 的「非空且 ⊆ 原柄」保证——`vmo_create` 原柄为 `READ|WRITE|MAP|DUPLICATE|TRANSFER`（`syscall.rs:977`）。发送成功后 `handle.rs:350-354` 的 `take` 把整槽摘走，发送方该值作废；发送失败时调用者表零变化，`rpc` 的 EBUSY 重试可复用同一 alias；接收侧（`syscall.rs:3947` + `handle.rs:360-365`）落同样权利，wmd 在 `gui-wmd/src/relay.rs:230` 原样转发——四个新别名在接收侧的权利与早已跑通的三处完全同形。

## Alternatives considered

七处统一改 `RIGHTS_SAME`（弃——会连 WRITE/MAP/DUPLICATE 一并授出，与执法「只看一位」的最小授权面相悖）；把预检从 `-13` 改为静默放行（弃——那是在拆 2026-11-11 的执法面，缺位本身就该 loud）；只修 953 行一处让 k4a 过门（弃——k4s/k6p 两段会在同一预检上以同一文案再红，修不彻底）；顺手在 `submit_full` 文档注释（`:277`）补「别名须 `READ|TRANSFER`」防复发（本轮未做——gfxdemo 任何后续改动都要重跑 desktop 端到端，复审建议单独排期）；i128 中间量（与本刀无关，inputd 同日的溢出修法才有此备选）。

## Consequences

编译门：`just build-user` exit 0，gfxdemo 自身零 warning；musl `cargo check --release --target aarch64-unknown-linux-musl` exit 0（仅 berry-libc 3 条、berry-ui 1 条依赖既有告警）。

端到端（desktop profile、演示舰队开启、`BERRY_NO_VGPU=1`、QEMU TCG、256 MiB；gfx 路 run `1795457511-558213000-41104-host`）：`err=-13` 0 次（修前恒 1 次）；`gfx: k4a hz=120` 在第 151 行出现（修前断点正在此处），其后 `k4t`、`k4s n=17 bad=0`、`k6p n=4`、`k6h hz=240/480` 依次在席，第 200 行 `GFX ok` 首次出现；`LAUNCH ok` 第 301 行；`VBLK ok`、`BLKD ok`、`VIRTIO ok`、devmgr 行、`INPUT ok` 缺席（desktop 机设计内）均与修前一致；panic 0。镜像反汇编复核：改后 ELF 七处 `HANDLE_DUPLICATE` 系统调用的权限立即数全为 `0x21`（`READ|TRANSFER`），无 `0x1`。

四路合流后总审以同配方复跑（run `1795459777-096265000-7025-host`，`LAUNCH ok` 于起机 +210s，终态 366 行 / 75159 B）：`err=-13` 0 次、第 200 行 `GFX ok`、第 300 行 `LAUNCH ok`、`VBLK ok` 第 119 行、`BLKD ok` 第 201 行、`VIRTIO ok` 第 240 行、devmgr 第 214 行、`INPUT ok` 0 次（设计内缺席）；panic / `vblk died` / `gfxdemo died` 均 0；异常计数 `wm WAIT 在途悬挂` 2、`err=-13` 0、`wmd died` 0。沉浸像素回归：`px(100,20)=(1,161,134)`、`px(200,600)=(6,168,150)`、`px(500,600)=(5,182,125)` 均为壁纸青绿域，`px(272,1830)=(0,101,195)` 与[导航键透明化笔记](2026-11-17-P0导航键透明化与裁带对齐.md)的参考值逐位相同，x=544 列 y∈[1600,1800] 的 R>250 像素 0 个；返回键白像素 935 个，x 包围盒 [253,291] 中心 272.0，x 最小列仅中部 y=1852 一点、x 最大列分居 y=1826/1878 两端——「<」形完整。

未覆盖面：`k6t/k6l/k6f` 三段按源码自门（model/flags 资格）零行，是设计内跳段；`k4t/k6h` 的 `p99=0ns` 高 miss 是 TCG 时序面，不在本刀范围；四个改点中 `submit_full` 的七个调用点里只有 k6p 路径被本次日志显式覆盖（`k6p n=4`），其余调用点依赖同一 `submit_full` 实现的代码路径等价。

## Related

- [W3-Q91：IPC 探测链折叠——send 预检/wait_get 单查收口](2026-11-17-IPC探测链折叠收口.md)——本刀撞上的预检环即该刀折叠后的 `probe_transfer_ok` 单源判定。
- [RIGHT_TRANSFER 全生态执法](../../../docs/阶段沿革.md)（安全修复 #2 节）——缺位即 -13 的执法面本体；gfxdemo 当年只补了三处。
- [gfxdemo 部分包络模式——真 damage 帧源首落](2026-11-14-gfxdemo-partial-envelope.md)——gfxdemo 帧源与 SUBMIT 路径的前置刀。
