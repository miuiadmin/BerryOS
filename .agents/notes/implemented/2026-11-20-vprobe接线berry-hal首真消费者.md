# vprobe 接线 berry-hal：HAL crate 的首个真消费者（消「落地零驱动使用」空转）

Status: implemented

## Problem

[berry-hal 落地笔记](2026-11-20-berry-hal统一硬件抽象层crate.md)的 Consequences 自陈「消费面下一刀 = vblk/vnet 按 HAL trait 重接线」——截至本刀前，四 trait + VirtioMmio 实现零驱动消费：vprobe 仍走自己的逐字 `read_volatile`（main.rs 四处 unsafe 块）+ 本地六常量镜像（VIRTIO_MAGIC/VERSION/DEV_BLOCK/DEV_NET/DEV_INPUT/DEV_GPU），HAL crate 处于「落地但空转」状态——架构件没有消费者等于没验收。

## Decision

**方案 = vprobe 的 MMIO 读全量翻到 berry-hal（Mmio trait 面 + VirtioMmio 实现），行为逐字等价**：

1. **trait 面消费**：每槽 `VirtioMmio::new(槽基址, 0x200)` 授窗（Result 走 let-else + trap 兜底——null/零长窗在现几何下不可达，防未来漂移的 loud 面），头四字经 `Mmio::read32(mmio::MAGIC/VERSION/DEVICE_ID/VENDOR_ID)` 读——方法解析需要 trait 在域，vprobe 从「绑 volatile 实现」翻转为「绑 Mmio 契约」；volatile 与窗界检（4 对齐 + off+4 ≤ 窗长）收敛在 HAL 实现内部，vprobe 的 MMIO unsafe 全数消失（唯余 trap() 故意自陷读，非 MMIO 面）。
2. **常量镜像退役**：本地六常量改引 `berry_virtio::mmio` 单源（MAGIC_VAL/VERSION_MODERN/DEVICE_ID_BLOCK/NETWORK/INPUT/GPU）——vblk 既有「引常量不复制」纪律；GPU did 数值链 vprobe → mmio::DEVICE_ID_GPU → berry-gpuproto（uapi 两源断言冻结）两级单源，比原先「本地镜像 + 钉板互证」少一层手抄面。
3. **指针算术安全化**：槽基址在 usize 域算术后 `as *mut u8`（aarch64 usize=u64 无损）——原先 `*const u32` 域的 `base.add()` unsafe 也一并消除。
4. **跨工作区依赖**：vprobe 加 `berry-hal = { path = "../../../user/berry-hal" }`（drivers→user 方向，先例 = berry-libc/berry-sys/berry-exec 三行同款；反向 user→drivers 先例 = berry-hal 自身依赖 berry-virtio）+ `berry-virtio = { path = "../../berry-virtio" }`（同 workspace，vblk 先例）；drivers/Cargo.lock 增量恰 9 行（berry-hal 条目 + vprobe 依赖表两行，berry-gpuproto 已由 vblk 先带入）。
5. **行为等价承诺**：判据序（magic 全槽验 → 占用槽 version 验 → did 分类计数）、打印文案、trap 语义、`VIRTIO ok` 标记格式逐字保留——smoke rootfs 相第 12 标记面零漂移。

## Alternatives considered

用 VirtioMmio 便捷面（`win.magic()/version()` 等——弃：便捷面是 VirtioMmio 固有方法，走 `Mmio::read32` + mmio 偏移单源才构成 trait 契约消费，且与 vblk 消费形态一致）；`is_virtio_modern()` 复合判据（弃——vprobe 要分开诊断 magic 错与 version 错〔文案不同〕，复合面丢诊断精度）；选 vblk 当首消费者（弃——vblk 是常驻服务（A-4 服务化收口后不退出），首刀选 135 行启动期自检件 vprobe：跑完即退、标记面单一、回归面最小，vblk/vnet 仍按原计划做后续刀）；berry-driver virtio_example 改造当备胎（未取——示例不是真 bin，vprobe 是进 rootfs 的真产物，架构空转的消除要件是「真消费者」非「更好看的示例」）；Result 用 unwrap/expect（弃——musl 链 panic 面可用但家族惯例是 trap 只死自己，let-else + trap 与既有诊断面同律）。

## Consequences

`just build-user` 全绿（exit 0，rootfs 产链完整 + tlsscan x62 ELF 全过）；产物级验证：反汇编 vprobe ELF 见 `mov w12, #0x70000000`（VA 基址）+ `movk w13, #0x7472`（magic 鉴定值）+ `ldr w0, [x12]`（HAL volatile 读内联）+ `cmp x8, #0x20`（32 槽界）——HAL 读序列真实进产物非死码。berry-hal自此有真消费者，后续 vblk/vnet 重接线有了参照形态（trait 面读 + 偏移单源 + let-else 授窗兜底三件套）；drivers workspace 首次出现 user→drivers 反向依赖的消费者（berry-hal 依赖 berry-virtio 而 vprobe 依赖 berry-hal——同 crate 双路径汇于 drivers/Cargo.lock 单锁，无版本分裂）。运行期行为按代码审查判等价（判据/文案/标记逐字未动）；QEMU e2e `VIRTIO ok` 复验归主线统一验证（berryos-dev 纪律：agent 交付后主线必复核）。Related：[berry-hal：统一硬件抽象层 crate](2026-11-20-berry-hal统一硬件抽象层crate.md)（本刀消费的契约面——其「消费面下一刀」由本刀兑现第一口）、[berry-driver 统一 Driver trait 框架 crate](2026-11-20-berry-driver统一Driver-trait框架crate.md)（生命周期框架，与 HAL 是姊妹面）、[驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)（§4.1 三面胶水缺口的收敛起点）。
