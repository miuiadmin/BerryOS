# berry-driver：统一 Driver trait 生命周期框架 crate（报告 P1 框架件 + 波 2.2 先行刀）

Status: implemented

## Problem

[驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md) §4.1 定谳的结构性缺口框架层：全仓驱动无统一生命周期契约——berry-virtio 只管 virtqueue 布局编解码（纯函数面），vprobe/vblk/vnet 各自裸写扫窗判据，vprobe 白名单外 did 直接 trap（炸面）；「驱动 = 持证应用」波 2.2（总线抽象泛化）需要一颗先钉生命周期形状的框架 crate，让新驱动不再长第二套 probe/起停胶水。

## Decision

**方案 = 新增 `user/berry-driver`（no_std + alloc，`cfg_attr(not(test), no_std)` berry-virtio 族例形态）**：

1. **三口 trait 借 Linux probe 生命周期形状不借住址**（报告 §2.4）：`probe(mmio_base, irq)` 认领 → `start` 服役 → `stop` 退役，stop 后可再 start（生命周期循环）；不借 .ko 运行时装载与 sysfs 全量树（ADR #46 已立法拒绝项）。ProbeInfo 三件套（device_id/vendor_id/version）总线中立命名——virtio-mmio 语义 = 三寄存器读数，非 virtio 总线自定义语义。
2. **失败面是值不炸**：`DriverError` 四值枚举（MmioFault/UnsupportedDevice{device_id 带读数}/NotReady/Timeout），probe 不认领返回错误值、registry 换下家继续问——把 vprobe 的 trap 炸面在框架层改走错误值。probe 前 null 窗门 = MmioFault（真机语义 = 未授 VMO/破窗）。
3. **probe_all 全记录不仲裁**：驱动 × 槽全组合各一条 `ProbeResult`（成功失败都记）；「单设备单主人」认领仲裁归 devmgr v2（波 1 权责），框架层透明供仲裁消费。registry 持 `Vec<&'static mut dyn Driver>`——真机形态 = 全局泄漏驱动实例，host 测试 `Box::leak` 同形。
4. **virtio_example 钉包装法**：判据序 = vprobe 同源（magic "virt" → version=2 拒 legacy → did 认领），偏移/鉴定值全引 `berry_virtio::mmio` 单源不复制；start = 规范 §2.1 状态机四步（ACK|DRIVER → FEATURES_OK 回读验 → DRIVER_OK，验不过 Timeout）；stop = STATUS 写 0 复位。跨工作区 path 依赖 user→drivers（反向先例 = drivers→user 的 vnet 消费 berry-netproto），锁各自独立互不污染。
5. **接线**：user/ workspace members 入列 + justfile test-host 显式行（host 7 测：mock probe_all 成功/失败混合与 MmioFault 不炸、lifecycle 循环、virtio 假 MMIO 面板判据/STATUS 序列/registry 合面）。

## Alternatives considered

probe_all 首个认领者即 break（弃——认领仲裁单主人是 devmgr v2 的权责，框架层全记录才透明可审计）；trait 硬编 mmio_base/irq 存字段（弃——实现方自记状态，mock 与真驱动同契约）；错误面泛型 `Result<T, E>`（弃——四值枚举总线中立、derive PartialEq 可断言）；virtio_example 做完整特性协商 DEV_FEAT/DRV_FEAT 寄存器对（弃——示例只钉生命周期接线法，协商归真驱动刀）；本刀直接接线 vblk 消费面（任务书明确消费面后续刀——vprobe 白名单 trap 收口与 vblk 生命周期迁移是下一刀）。

## Consequences

`cargo test -p berry-driver --target aarch64-apple-darwin` 7 测全绿零警；none-target（aarch64-unknown-none）no_std 面构建零警通过（RUSTFLAGS="" 防根 config NEON 旗，build-user 同纪律）；user/Cargo.lock 随 member 入列增量（berry-driver + berry-virtio 两条目；同批混装的 berry-partition 条目属并行存储栈 lane，非本刀产物）。build-user 显式 `-p` 列表不裹挟本 crate。消费面下一刀 = vblk/vnet 按新 trait 重接线 + vprobe trap 面收口；再远 = devmgr v2 拿 ProbeResult 全量记录做认领仲裁（波 2.2 SDK 家族的 `/svc` 注册面）。Related：[驱动子系统综合研究报告](../../research/2026-11-18-驱动子系统综合研究报告.md)（P1 缺口定谳 + §2.4 借形状清单 + 波 2.2）、[berry-partition 分区表解析 crate](2026-11-20-berry-partition分区表解析crate.md)（同日驱动子系统姊妹刀）。
