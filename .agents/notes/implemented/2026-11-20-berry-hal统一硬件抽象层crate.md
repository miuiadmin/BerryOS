# berry-hal：统一硬件抽象层 crate（报告 P1 缺口件 + 波 2 架构件）

Status: implemented

## Problem

[驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md) §4.1 诊断的结构性缺口：各驱动（vprobe/vblk/vnet）各自实现 MMIO volatile 读写、IRQ 注册、DMA 分配——三面胶水无统一抽象，新驱动被迫长第二套；报告 §4.3 波 2.2「DeviceCap 三铸件」（MMIO 窗 VMO + Interrupt + Bti 占位）需要一颗先钉硬件访问契约的架构构件 crate，让驱动只写业务逻辑、平台差异沉到 trait 实现侧。

## Decision

**方案 = 新增 `user/berry-hal`（no_std + alloc，`cfg_attr(not(test), no_std)` berry-driver 族例形态）**：

1. **四条窄 trait + 统一错误面**：`Mmio`（read32/write32，窗内字节偏移面）、`IrqCtl`（register 带 `fn()` 静态函数指针——上下文归实现侧，trait 面保持对象安全不泛型化）、`DmaAlloc`（alloc/free_coherent，cacheline 对齐契约）、`Clock`（now_us/spin_us）；`HalError` 四值枚举（NoDevice/IrqTaken/Oom/Invalid），值不炸。全四 trait dyn 兼容（`Box<dyn Mmio>` 等皆可用——统一抽象让驱动经 dyn 持平台实现，mock 与真实现同契约互换）。
2. **同名异层划界**：与莓核 Wave 4 刀 4.1 的内核侧 berry-hal（CPU/MMU/IRQ/Timer/SMP 五窄口）同名但契约面互不相交——那颗服务内核 arch 抽象，本颗服务用户态驱动进程（报告 §6 节奏耦合段两半边分说）；crate 注释明记防读者混淆。
3. **virtio_impl 包装法**：`VirtioMmio` 实现 `Mmio`——把 volatile 读写包装为安全接口（unsafe 完全收敛在实现内部，库本体零 unsafe），授窗门在 `new`（null/零长窗 Invalid），读写偏移门 = 4 对齐且界内（越界读 0 写弃——值不炸对偶面）；鉴定便捷面（magic/version/device_id/status/int_ack/queue_notify）偏移全引 `berry_virtio::mmio` 单源不复制（berry-driver「引常量不复制」纪律）。
4. **参考实现消费 alloc 面**：`BumpDmaAlloc`（bump DMA 蓄水池——Vec 后备、按绝对地址对齐到 64 边界、不回收复用只销账、池尽 Oom）与 `VirtualClock`（AtomicU64 虚拟直推——host 零真延时）；两者同时是 host 测试的可复用挂具，真机换 VMO/寄存器实现同契约。
5. **接线**：user/ workspace members 入列 + justfile test-host 显式行（host 8 测：IrqCtl 注册/占用/注销循环、BumpDmaAlloc 对齐与 Oom、配额 mock 互换、VirtualClock 单调、四 trait 的 dyn 面、VirtioMmio 假面板 volatile roundtrip/界检/寄存器语义/窄窗截尾、经 dyn 跑驱动业务序合面）。

## Alternatives considered

`IrqCtl::register` 泛型化 handler（`impl Fn()`——弃：破坏对象安全，dyn IrqCtl 不可用）；read32 返回 `Result<u32, HalError>`（弃：trait 契约面任务书冻结为裸 u32，越界走读 0 写弃的值不炸面，与真机授窗 VMO 界外不可达语义一致）；BumpDmaAlloc 做 freelist 真回收（弃——过度工程：参考实现只钉契约，真回收归波 2.4 连续页 DMA VMO 刀）；池基址 `alloc::alloc` + `Layout::align(64)` 对齐分配（弃——引入 unsafe 面，多留 63 字节垫片按绝对地址对齐化同效且保持零 unsafe）；mock 全放测试文件（部分弃——BumpDmaAlloc/VirtualClock 做成库参考实现：alloc 面有真实消费非摆设，host 挂具可复用）；本刀直接接线 vblk 消费面（弃——真消费面 vblk/vnet 接线是后续刀，与 berry-driver 同节奏）。

## Consequences

`cargo test -p berry-hal --target aarch64-apple-darwin` 8 测全绿零警（fn 指针断言走 `core::ptr::fn_addr_eq` 避 `unpredictable_function_pointer_comparisons` lint）；none-target（aarch64-unknown-none）no_std 面构建零警通过；berry-driver 邻居 7 测回归绿。user/Cargo.lock 增量 berry-hal 条目（同批混装的 berry-rtc/berry-watchdog 条目属并行 lane，非本刀产物——members 行文本级共存，各自编辑未裹挟）。消费面下一刀 = vblk/vnet 按 HAL trait 重接线（配合 berry-driver 生命周期框架；vblk 已于 2026-11-23 兑现，vnet 仍留）；再远 = 波 2.1 Interrupt 对象直通（IrqCtl 真机实现）与波 2.4 cache 维护 DMA API。Related：[驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)（§4.1 缺口定谳 + §4.3 波 2.2 + §6 同名异层依据）、[berry-driver 统一 Driver trait 框架 crate](2026-11-20-berry-driver统一Driver-trait框架crate.md)（同日姊妹刀——生命周期框架，本 crate 是其下的硬件访问面）、[vprobe 接线 berry-hal：HAL crate 的首个真消费者](2026-11-20-vprobe接线berry-hal首真消费者.md)（同日消费面第一口——「消费面下一刀」的兑现起点）、[vblk 接线 berry-hal：HAL crate 的第二个真消费者](2026-11-23-vblk接线berry-hal.md)（消费面第二口——MMIO 面，vnet 仍留）。
