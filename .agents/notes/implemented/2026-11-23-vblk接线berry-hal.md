# vblk 接线 berry-hal：HAL crate 的第二个真消费者（MMIO 面，行为逐字等价）

Status: implemented

## Problem

[vprobe 接线笔记](2026-11-20-vprobe接线berry-hal首真消费者.md)兑现了 berry-hal 的首个真消费者并把 vblk/vnet 列为后续刀；[berry-hal 落地笔记](2026-11-20-berry-hal统一硬件抽象层crate.md)的 Consequences 亦自陈「消费面下一刀 = vblk/vnet 按 HAL trait 重接线」。截至本刀前，vblk（`drivers/bins/vblk/src/main.rs`）仍走自己的裸 MMIO：find_device 每槽 `base.add(slot × STRIDE/4)` 取 u32 字指针，`rd`/`wr` 两个 unsafe 薄层包 `read_volatile`/`write_volatile`，negotiate/setup_queue/blk_op/serve 全链以 `*mut u32` 传基址——HAL 的真消费者只有跑完即退的探针 vprobe 一件，常驻服务面零消费。

## Decision

**方案 = vblk 的 MMIO 读写全量翻到 berry-hal（`Mmio` trait 面 + `VirtioMmio` 实现），四个核心函数只改签名、函数体零 hunk，行为逐字等价**：

1. **trait 面消费**：`mmio(slot)` 从返 `*mut u32` 改返 `VirtioMmio`——基址 = `VIRTIO_MMIO_VA + slot × STRIDE`、窗长 = `STRIDE`（0x200），vprobe 同款授窗；`VirtioMmio::new` 的 Err 走 trap 兜底（null/零长窗在现几何下不可达，是防窗几何漂移的 loud 面）。`rd`/`wr` 改为 `regs.read32(off)` / `regs.write32(off, v)`：volatile 与窗界检（4 对齐 + off+4 ≤ 窗长）收敛在 HAL 实现内部，vblk 的 MMIO unsafe 删 7 处，余 10 处代码级 unsafe 全是 static mut 取引用、`dmb ish` 发布屏障与 trap() 故意自陷读，文本未动。
2. **签名面**：negotiate / setup_queue / blk_op / serve 的 `regs: *mut u32` 改 `&mut VirtioMmio`，main 内调用点随之 `&mut regs`——diff 只有签名行与借用符；特性协商位与顺序、队列建立序、STATUS 写序列（DRIVER_OK 最后置）、notify 时机（`dmb ish` → Q_NOTIFY）、请求头/收割协议、日志文案、`VBLK ok cap=… slot=…` 标记格式、分区探测块与错误降级全部原样。寄存器偏移仍引 `berry_virtio::mmio` 单源常量；vblk 用到的最大偏移 `CONFIG + 4 = 0x104`（读终点 0x108）在 0x200 窗内。
3. **跨工作区依赖**：vblk Cargo.toml 加 `berry-hal = { path = "../../../user/berry-hal" }`（vprobe 同款形态，drivers→user 方向先例 = berry-libc/berry-sys/berry-exec）；drivers/Cargo.lock 增量恰 1 行（vblk 依赖表加 `"berry-hal"`——berry-hal 条目已由 vprobe 带入锁内），锁文件与 Cargo.toml 同批入库，否则 `--locked` 构建红。
4. **display() 不换**：vblk 自有分区行打印段与 `berry_partition::display()` 两处不等价——n==1 时库打 `1 partition:`（按条数判单复数）而 vblk 恒打 `1 partitions:`；空表时库打 `0 partitions` 而 vblk 打 `vblk: no partitions（空表，按整盘服务）`。按行为等价承诺本刀不切换，归后续刀。

## Alternatives considered

用 `VirtioMmio` 便捷面（`magic()`/`version()` 等——弃：vblk 读写的是全寄存器面〔STATUS/Q_*/INT_*/CONFIG〕而非只 ID 头，走 `Mmio::read32/write32` + mmio 偏移单源才与既有 rd/wr 薄层一一对应、diff 最小）；`mmio()` 返 `Result` 让调用方处理 Err（弃——家族惯例是 trap 只死自己，与 vprobe 的 let-else + trap 同律）；同批切 `berry_partition::display()`（弃——Decision 4 的两处不等价，等价刀不混装行为变化）。

## Consequences

`just build-user` exit 0（尾行 `tlsscan: rootfs.cpio x62 ELF 全过`）；`cargo check --release --target aarch64-unknown-linux-musl -p vblk` 0 error、vblk 自身 0 warning（与基线相同，3 条 warning 全在 berry-libc net.rs 既有 dead_code）。端到端六项证据（desktop profile、QEMU TCG、256 MiB、数据盘挂 virtio-blk、run 目录 `1795447337-218349000-34472-host`）：① serial.log L119 `VBLK ok cap=2048 slot=31`；② L153 `BLKD ok n=512`（svcmgr 代打，行首前缀是内核调度 trace 字母）；③ L253 `LAUNCH ok`；④ L118 `vblk: 分区表解析 bad MBR signature (0x55AA) at LBA0（按整盘服务）`——自 `vblk:` 起与基线逐字节相同，分区探测块零漂移；⑤ 盘上写自验印章——起机前 LBA1 全零，起机后 xxd 偏移 0x200 起 16 字节 `6265 7272 796f 732d 7662 6c6b 2d77 3121`（`berryos-vblk-w1!`，即 `LBA1_PATTERN` 经 `blk_op(BLK_T_OUT, 1)` 写、再 `blk_op(BLK_T_IN, 1)` 读回对拍的往返活证），偏移 0x000 起 16 字节 `4245 5252 594f 5356 424c 4b21`（`BERRYOSVBLK!`，dd 预埋印章，vblk 读 LBA0 对拍通过）：

```
00000000: 4245 5252 594f 5356 424c 4b21 0000 0000  BERRYOSVBLK!....
00000200: 6265 7272 796f 732d 7662 6c6b 2d77 3121  berryos-vblk-w1!
```

⑥ `panic|KERNEL PANIC|vblk died` 零命中（+60s 复扫仍零；两次 EL0 sync 异常均为 ktest 故意触发且内核点名 `user_rogue_ptr`/`user_child`，非 vblk）。未覆盖面：分区表成功解析路径（MBR/GPT 有分区）未被端到端覆盖——mkfat 盘无 MBR 签名，只走了 BadMbrSignature 降级路径；若日后接 `display()`，n==1 单复数差异须先对齐。HAL 消费者现状 = 真消费者 vprobe、vblk 两件 + 示例级 virtio_example（berry-driver 内模块，非 bin）；vnet 接线与 DeviceCap 三铸件仍留波 2.2，vblk/vnet 接 berry-driver 生命周期 trait 另归后续刀（两者当前只持 HAL 半边）。Related：[vprobe 接线 berry-hal：HAL crate 的首个真消费者](2026-11-20-vprobe接线berry-hal首真消费者.md)（参照形态——trait 面读 + 偏移单源 + 授窗兜底三件套，本刀照抄）、[berry-hal：统一硬件抽象层 crate](2026-11-20-berry-hal统一硬件抽象层crate.md)（本刀消费的契约面——「消费面下一刀」兑现第二口）、[vblk 接入 berry-partition 启动分区探测](2026-11-20-vblk接入berry-partition启动分区探测.md)（前置刀——其分区探测块一字未动）、[devmgr v1：MMIO 槽枚举 + 驱动匹配 + 服务状态报告](2026-11-20-devmgr-v1设备枚举与服务状态报告.md)（驱动表 blk→vblk 绑定；devmgr 自身 SlotReader 未接 HAL）、[svcmgr 接入 devmgr 拉起链](2026-11-20-svcmgr接入devmgr拉起链.md)（`SpawnSpec::mmio_vmo` 授窗契约——vblk 持窗同源）、[驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)（§4.1 三面胶水缺口的收敛起点）。
