# ai-runtime Tier 1 宿主直通可行性侦查

Type: survey
Status: current
Updated: 2026-11-22

一句话说明：为 DAG 节 `ai-runtime §2` 的 Tier 1 条款（`berry-paravirt-npu`——模拟器半虚拟化宿主算力直通：共享物理页帧把 `.bmod` 权重页零拷贝映射至宿主 `wgpu::Buffer`，算子图卸载至宿主 Metal/Vulkan Compute）定下一刀的切面：两侧代码面已摸清——客机侧有 berry-virtio 家族范式（virtio-gpu 2D 编解码器纯 crate + vgpud 常驻 bin 先例），宿主侧是 `tools/berry` 设备平台（macOS Swift/Metal 呈现栈已在产）；结论是首刀切客机侧纯编解码 crate（`berry-npuproto`，零冲突零依赖），宿主侧 Metal Compute 桥与共享页帧 DMA 是第二刀（涉模拟器面，路径边界同 §一.2 需 owner 声明）。

## 客机侧现状（已摸清，可开工）

- [berry-virtio](../../drivers/berry-virtio/src/lib.rs) 是 split-ring 纯函数 crate，[gpu.rs](../../drivers/berry-virtio/src/gpu.rs) 已示范「规范命令族编解码单源 + wire 事实注释钉板 + 编不出即拒 None」的家族纪律（virtio 规范 §5.10 与 Linux UAPI 逐字段对拍）。
- 常驻驱动 bin 先例：`drivers/bins/`（vgpud/vnet/vblk）——`berry-paravirt-npu` 客机驱动（探测/协商/环驱动/DMA）落位形态同款。
- virtio-compute 无行业标准设备 id——wire 形态是**本项目自定义契约**（对齐 virtio-gpu 头形 24B 起步：op 描述符 + `.bmod` 页帧 backing 表 + fence 语义），首刀只钉 wire 编解码不碰真环。

## 宿主侧现状（涉模拟器面，边界外）

- 模拟器 = [emulator 总体设计](../plans/emulator/design.md) 的 `tools/berry` 设备平台：macOS 宿主、120fps 零拷贝 GPU 呈现在产（Metal 面）；Tier 1 需要的宿主 Compute Shader 执行器 + 共享页帧映射（`.bmod` 页 → `MTLBuffer`）是该平台的新增件。
- 依赖注记：设计写「宿主 `wgpu`」——wgpu 是 Rust 生态件，`tools/berry` 若是 Swift 件则直接用 Metal API（wgpu 之名按「宿主 GPU 抽象」语义读，不必引入 Rust 依赖面）；此解读待 owner 在 §2 建造清单裁定时确认。

## 首刀建议（下一轮可执行面）

`user/berry-npuproto`（或 drivers/ 随 berry-virtio 家族落位——归属随 owner 裁定，暂记 user/）：virtio-compute wire 编解码纯 crate——设备 config（页帧几何/能力位）、op 描述符（MatMulQ4/RoPE/RMSNorm/FlashAttn 与 berry-tensor 图算子集对齐）、`.bmod` backing 页表、fence/完成环，host 测试钉 wire 常量。前置依赖零（berry-tensor 图算子枚举可先以常量镜像单源或窄引用）；验收 = host 测 + test-host 接线。

## 边界与挂账

- 宿主侧（tools/berry）动工 = 模拟器面，无人轮默认边界外，与 §一.2（内核 VMO 面）同列 owner 边界声明队列。
- wire 契约一旦首刀钉板即成单向门（对拍纪律同 virtio-gpu）——op 集先窄后宽，首刀只钉传输层不钉算子语义（算子语义单源在 berry-tensor）。
