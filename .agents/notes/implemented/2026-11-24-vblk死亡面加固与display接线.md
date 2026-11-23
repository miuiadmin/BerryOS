# vblk 死亡面加固：trap 先复位设备、put_desc 与授窗拒绝 loud 化、分区行接 display()

Status: implemented

## Problem

[vblk 接线 berry-hal](2026-11-23-vblk接线berry-hal.md)之后，vblk（`drivers/bins/vblk/src/main.rs`）的死亡面仍有三处不整洁。其一，`trap()` 只做自陷：DRIVER_OK 置位后 avail 环仍指向 DMA VMO 帧，poll 超时这条 trap 路径上请求可能仍在飞。内核会在进程死后的 MMIO 窗 VMO 销毁时自动复位设备（F0-4 的 `reset_virtio_mmio_window`），但从自陷到窗 VMO 销毁之间设备仍在飞、帧随时可能被回收——gui-inputd 的 `trap()` 文档注释把它叫作「设备写已回收帧、静默腐化无关进程」，并早已对已认领寄存器窗先写 `STATUS=0`（其 `CLAIMED_REGS` 面），vblk 没有同款。其二，`blk_op` 三处 `let _ = vio::put_desc(...)` 把 None 静默吞掉：描述符未落环仍 publish，设备读到陈旧 desc 行。其三，`mmio()` 的 Err 分支无诊断直接 trap。另一处欠账在启动日志：非空分区表由 vblk 自持的手拼循环打印（基线第 459—482 行），与 `berry_partition::display()` 双写，唯一差异是单分区时打 `1 partitions:`；接线笔记当时以「等价刀不混装行为变化」把切换留给后续刀。

## Decision

四项同批落地，全部在 `drivers/bins/vblk/src/main.rs`（+92/−36，9 个 hunk），只改死亡面与打印面：

1. **A trap 先复位设备**：新增 `static mut CLAIMED_SLOT: Option<usize>`（第 136 行），`main` 在 `find_device()` 返回后、`mmio()` 与 `negotiate` 之前落值（第 437 行）。`trap()` 开头若已认领（第 633 行 `ptr::read` 取值），用 `VirtioMmio::new` 以原参重建寄存器窗，写 `STATUS=0`，定次自旋回读（`TRAP_RESET_SPINS = 1_000_000`，第 114 行，取 gui-inputd 同款值），打一行 `vblk: trap 前 slot N 复位 STATUS 回读确认` 或 `vblk: trap 前 slot N 复位 STATUS 回读未确认`，再走原来的自陷读。认领前的 trap 行为逐字不变（static 初值 `None`）。
2. **B put_desc 执法**：第 358—371 行三处 `let _ =` 改为 `is_none()` → 打印 `vblk: put_desc 拒 desc N（hdr/data/status）` → `trap()`。
3. **C 授窗拒绝诊断**：`mmio()`（第 189—200 行）改 let-else，Err 分支先打 `vblk: slot N 授窗拒（窗基/窗长非法）` 再 `trap()`，与 vprobe 同形。
4. **D display() 接线**：第 499 行非空分区表改打 `vblk: {}` 加 `berry_partition::display(&ps)`；空表行与解析失败行文案一字未动；删除不再使用的 `PartitionKind` 导入。

未动的面：特性协商位与顺序、队列初始化顺序、请求与应答协议、`VBLK ok cap={} slot={}` 格式。unsafe 块由 9 处增至 11 处，新增两处即 A 的落值与取值——`Option<usize>` 是 Copy，取值不取引用；vblk 单线程、落值一次只读多处。

## Alternatives considered

trap 里调 `mmio()` 重建窗（弃——其 Err 分支会再次 trap 成递归，故 `trap()` 直接 `VirtioMmio::new`，授窗失败只打印并跳过复位）；复位回读帽用 clock 或 sleep 计时（弃——两者可能正是 trap 成因，改定次自旋，超帽只打「未确认」不拦死路）；保留手拼分区循环（弃——与 `display()` 双写，单源优先）；空表也调 `display()`（弃——会打 `0 partitions`，丢掉「按整盘服务」的语义）；D 继续按「等价刀」口径推迟（弃——本轮接受唯一差异即单复数勘正，不再以行为等价为承诺）。

## Consequences

编译门：`just build-user` exit 0（尾行 `tlsscan: rootfs.cpio x62 ELF 全过`）；`cargo check --release --target aarch64-unknown-linux-musl -p vblk` 0 error、vblk 零 warning。display() 接线的 host 对拍（旧手拼逻辑与 `display()` 并排跑 MBR 单双分区、GPT 带名与无名共 4 例）差异仅单复数（`1 partitions:` 变 `1 partition:`），其余逐字节相同。

端到端正常机（desktop profile、QEMU TCG、256 MiB、数据盘挂 virtio-blk；r5-vblk 路 run `1795452418-360752000-49704-host`）：串口第 118 行分区行自 `vblk:` 起与基线逐字节相同，第 119 行 `VBLK ok cap=2048 slot=31`，第 153 行 `BLKD ok n=512`，第 253 行 `LAUNCH ok`；停机后盘面 LBA0 为 `BERRYOSVBLK!`、LBA1 为 `berryos-vblk-w1!`（写落盘往返活证）；`panic`、`KERNEL PANIC`、`vblk died`、`put_desc 拒`、`授窗拒` 零命中。四路合流后同配方复跑（总审 run `1795455217-815507000-28401-host`）行号一致（第 118、119、153、255 行），LBA5 另见 svcmgr 自验 pattern，终态 382 行禁词仍为零。

端到端强制陷阱机（r5-vblk 路 run `1795452766-468751000-61115-host`，临时插桩验后已撤、定稿 blob 与插桩前一致）：在 vblk 自验后强制走 trap，串口第 161 行 `VBLK ok`、第 162 行插桩行、第 163 行 `vblk: trap 前 slot 31 复位 STATUS 回读确认`（确认 1 次、未确认 0 次）。内核异常报告的 `PC=0x2126c4` 经实跑插桩 ELF 反汇编对拍，落在 `trap()` 的 `read_volatile(0x4000_0000)`，`x30=0x2126c0` 是诊断 `println!` 的返回点——诊断行在因果上先于自陷（用户态 stdout 缓冲使它在串口字面上排在异常报告之后）。复位后无 vblk 重启、无 `vblk 死亡` 日志，svcmgr 的 blk 自验写段走 `eco_skip`（第 214 行 `eco: blkd write`）降级到 RAM-only，系统继续走到第 231 行 `LAUNCH ok`；盘面 LBA1 仍是 vblk 印章、LBA5 全零。

两台机与总审合并机的异常计数相同：`wm WAIT 在途悬挂` 2、`err=-13` 1、`wmd died` 0——前者是 inputd 安静表探测的设计内常态，后者是 gfxdemo 的 `RIGHT_TRANSFER` 缺失，均与 vblk 无关。

未覆盖面：非空分区表的 `display()` 输出未经端到端验证（起机盘是 super floppy，只走 `BadMbrSignature` 降级路径），只有 host 对拍；复位「未确认」分支与「授窗拒」分支在现几何下不可达，未被真机几何触发；真机异步复位的毫秒级时序无验证路径（QEMU 写 STATUS 同步即 0）。vnet 与 vgpud 的 `trap()` 仍是修前形态，见研究报告 §3.4。

## Related

- [vblk 接线 berry-hal：HAL crate 的第二个真消费者](2026-11-23-vblk接线berry-hal.md)——前置刀，当时把 display() 切换留给本刀。
- [vblk 接入 berry-partition 启动分区探测](2026-11-20-vblk接入berry-partition启动分区探测.md)——分区探测块本体，本刀只换打印。
- [F0-4：物理连续 DMA VMO 与 VirtIO 自动复位](2026-11-18-f0-4-物理连续dma与缓存维护及virtio自动复位.md)——内核侧防线：MMIO 窗 VMO 销毁时自动复位；本刀把停机提前到自陷之前、帧尚未回收的时刻，两道互补。
- [驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)——§3.4 健壮性审计后续列同类遗留。
