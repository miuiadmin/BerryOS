# gui-inputd `post_events` 的 put_desc 拒绝 loud 化——vblk 同款执法

Status: implemented

## Problem

[vblk 死亡面加固](2026-11-24-vblk死亡面加固与display接线.md)把 vblk `blk_op` 里三处 `let _ = vio::put_desc(...)` 改成拒绝即诊断加 trap，但同一形态在 `drivers/bins/gui-inputd/src/main.rs` 的 `post_events` 里仍在：eventq 建制时逐个挂 64 个 8B 事件读缓冲，`put_desc` 返回的 `None` 被 `let _` 静默吞掉后循环照旧走 `publish`——描述符未落环却已把该 head 发布进 avail 环，设备侧读到的是 RINGSTAGE 页里的陈旧 desc 行。RINGSTAGE 是 tablet 与键盘两台设备共用的建制暂存页（函数体内第 593—596 行注释自述首台建制后 `avail.idx` 残留 64、第二台须复位起点），第二趟若某行未写入，残留的就是上一台设备的 desc（addr 指向上一台的缓冲帧）——设备 DMA 写错帧且无任何诊断。几何上该 `None` 不可达（见 Decision），所以这是防御面整洁度缺陷而非现网故障；但紧邻的 `publish` 分支与 `berry_vmo_write` 分支早已拒绝即 trap，唯独 `put_desc` 一处静默，同一函数内三种失败两种执法一种吞掉。

## Decision

第 601—614 行：`let _ = vio::put_desc(...)` 改为 `if vio::put_desc(...).is_none() { println!(...); trap(); }`，诊断行文案 `gui-inputd: put_desc q0 desc {} 失败（idx>=qsize 或环页越界）`；`trap()` 复用本文件第 415 行的既有实现（先对 `CLAIMED_REGS` 两槽里已认领的寄存器窗写 `STATUS=0` 复位、定次自旋回读，再故意自陷），不另写。`post_events` 头注（第 584—589 行）补一段执法理由。换行形态与 vblk `drivers/bins/vblk/src/main.rs:358—360` 同款。

`None` 的两个条件来自 `drivers/berry-virtio/src/lib.rs:163—180`：`idx >= lay.qsize`，或四次 `put_u*` 的环页切片越界。本调用点 `i ∈ 0..QSIZE`（`QSIZE = 64`，`main.rs:85`）→ 第一条不可达；`DESC_SZ = 16` → desc 表 1024 B，`RingLayout::new(64)` 得 `avail_off = 1024`、`used_off = 1280`、`end = 1798 ≤ 4096`（单页承诺），每个 desc 的 16 B 写窗恒在 desc 表内 → 第二条不可达。Some 分支除多一次 `is_none()` 判定外零额外操作，正常路径输出与行为逐位不变。

覆盖面：该文件 `put_desc` 调用点唯一。其余 `let _ =` 是另一类有意的容错而非吞拒绝——三处 `object_wait_*` 以超时为预期结局、循环重验兜底，两处 `handle_close` 是行内注释自述的「礼貌关闭（EBUSY 类有界泄漏容错）」——不在本刀范围。

## Alternatives considered

保留 `let _` 并在 `publish` 前回读 desc 行对拍（弃——`None` 已是精确信号，回读是重复判定且仍是静默面的变体）；失败只打印不 trap（弃——描述符未落环的 head 一旦 publish 就是设备读陈旧行，继续跑等于把本刀要消灭的静默腐化留在原地）；另写一个只自陷不复位的局部 trap（弃——本文件 `trap()` 已含对已认领设备 `STATUS=0` 复位，是设备整洁面的正确形态，复用即可）；顺手给紧邻的 `publish` 拒绝分支补诊断行（复审建议项，本刀未做——它是对称性补齐而非行为修复，另立跟进件）；同批处理 vgpud、vnet、guifbd 里的同款 `let _ = vio::put_desc(`（弃——vnet 属并行会话领地、vgpud 不在产线，见 Consequences 的遗留）。

## Consequences

编译门：`just build-user` exit 0，gui-inputd 本体零 warning；musl `cargo check --release --target aarch64-unknown-linux-musl` 对 gui-inputd exit 0（依赖侧 berry-libc 3 条、berry-ui 1 条既有告警，非新增）。host 测试 `cargo test -p gui-inputd --lib --target aarch64-apple-darwin`：`48 passed; 0 failed`（即 `lib.rs` 的 48 个 `#[test]`；`main.rs` 无测试面）。`tools/facts.txt` 的 `tests_rust` 不变（本刀零新增 `#[test]`）。

端到端（desktop profile、QEMU TCG、256 MiB、`BERRY_NO_VGPU=1`，两台同镜像机 run `1795464407-461643000-3114-host` 与 `1795466557-833986000-35161-host`，`.berryver v0.2.0-1863-g8c76e042-dirty`；两台行号相差 ≤2，下取第一台）：`LAUNCH ok` 第 303 行（起机 +216 s；第二台 +205 s）；第 119 行 `VBLK ok cap=2048 slot=31`、第 200 行 `GFX ok`、第 201 行 `BLKD ok n=512`、第 214 行 devmgr `3 devices: [slot 29] input → gui-inputd ✓, [slot 30] input → gui-inputd ✓, [slot 31] blk → vblk ✓`、第 242 行 `VIRTIO ok`、第 296 行 `launcher: input poll armed h=65545`；第 123 行 `gui-inputd: up slot=30 q=64x8B … kb=slot 29`——两台设备都过了 `post_events` 与 `publish`。盯防行：`put_desc q0 desc … 失败` 0 次、gui-inputd 的死亡 / 重启 / `user_fault_exit` 0 次、`err=-13` 0 次、`rpc send err` 0 次、`panic` 0 次、`fatal, halt` 0 次；`wm WAIT 在途悬挂` 3（两次在开机、一次在交互之后，与 origin/main 基线机同形）。两条 `EXCEPTION` 均为 ktest 内嵌用户程序（第 52 行 `user_rogue_ptr` FAR=0x40000000、第 309 行 `user_child` FAR=0x6000），与基线机逐行同位，与 gui-inputd 无关。输入回归（`berry input --run <run> tap` 经 VNC PointerEvent 注入）：`tally h` 随点击递增（0→8→…→40）、`eco: app BROWSER it 03` 在席；应用打开后判定区 x[100,990) y[300,1500) 对 S0 变化 99.53%，应用内容最低行 y=1784、导航条顶边 y=1785、两者之间壁纸色 0 行、三键白像素包围盒与 S0 逐位相同（返回 x[253,291] y[1820,1884] 共 935 点、主页 x[523,565] y[1831,1873] 共 1285 点、多任务 x[798,834] y[1834,1870] 共 1008 点）；窗在席时点返回键（272,1852）后 +2 s 回落到与 S0 一致（0.00%），对照周期不按键窗存活 ≥18 s。沉浸像素回归五项与参考值逐位相同（`#01a186`、`#06a896`、`#05b67d`、`#0065c3`、x=544 列 R>250 计 0）。基线对照机（origin/main 干净树、同配方、单机）复现同一输入时序：首次点 BROWSER 图标 spawn 成功但 150 s 内不出窗、第二次点无 launcher 动作、第三次点走 `BROWSER recycle` 后 +8 到 +14 s 出窗——与本刀镜像逐项同形，属既有行为，不在本刀范围。

未覆盖面：`put_desc` 拒绝分支在现几何下不可达，未被真机触发（与 vblk 的「复位未确认」「授窗拒」两分支同况）；`publish` 拒绝分支仍无诊断行；`drivers/bins/vgpud/src/main.rs`、`drivers/bins/vnet/src/main.rs`、`user/bins/guifbd/src/guest.rs` 里的同款 `let _ = vio::put_desc(` 未动——vnet 归并行会话、vgpud 不在产线不可构建，均列入遗留。

## Related

- [vblk 死亡面加固：trap 先复位设备、put_desc 与授窗拒绝 loud 化、分区行接 display()](2026-11-24-vblk死亡面加固与display接线.md)——同款执法的前置刀与形态先例。
- [gui-inputd `normalize_axis` 乘法溢出——中间量 i64 改 u64 并补等价性测试](2026-11-24-inputd-normalize_axis乘法溢出修复.md)——同文件 `lib.rs` 的上一刀；本刀同批以纯注释把其「结果与旧式逐位相同」限定到旧式不溢出的入参域内。
- [inputd 在途 WAIT 悬挂强制重发（I3）](2026-11-19-inputd在途悬挂强制重发I3.md)——同文件泵面刀，本刀不触及泵面。
- [驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)——§3.4 健壮性审计后续，同类 `let _` 吞拒绝面的登记处。
