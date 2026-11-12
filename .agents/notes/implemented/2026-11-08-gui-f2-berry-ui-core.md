# Agent Note: GUI F2 夯实 BerryUI core

Status: implemented — F2 诊断面、SDK facade 与 S7 编译修复已过默认/S7/no_std 与系统回归

## Problem

BerryUI 已有保留树、布局、damage、hit、帧循环与离屏测试，但 F4/F5 缺少稳定的 SDK 消费入口，测试与后续组件也缺少值级布局/命中诊断。S7 feature 下 `set_text` 还把字号档位误读在 `TextBuf` 上，导致属性级失效面无法编译。

## Decision

按 [GUI框架地基路线-v1.19.md](../../../docs/GUI框架地基路线-v1.19.md) 完成 F2，不重写引擎。新增 `berry-ui/src/diagnostic.rs`：`Ui::layout_dump` 输出前序树、父子链、相对几何、视觉绝对界、内在尺寸与 layout/paint dirty 位；`Ui::hit_test_dump` 逐点输出 topmost 节点、种类与命中界。二者容量不足均 `UiError::NodeFull`，绝不静默截断。新增 `berry-ui/src/sdk.rs` 冻结 F4/F5 最小 facade。S7 `set_text` 改从 `TextProps.style.scale` 单源捕获字号档位。

## Alternatives considered

- **JSON/文本 dump**：需要格式化与转义，容易变成第二套序列化协议；否决，选 Plain Data。
- **公开 Node 内部字段**：会冻结 arena 实现细节，破坏 GUI-3 契约；否决。
- **重写 layout.rs 为约束协商 v2**：违反 F2「不重写许可状」，且案 1 已另列后续刀；否决。
- **只在测试里遍历内部树**：测试侧会形成第二几何真源；否决，dump 进入公共诊断面。

## Consequences

F2 的布局/命中证据可与离屏 golden 同场景对拍；SDK 消费面由 `sdk.rs` 显式冻结，后续组件不得绕开。诊断不是 wire 协议，不进入 guest IPC。默认 BerryUI、S7 feature 与 no_std target 三种构建均需通过。F3 才做主题 token，F4 才做组件，F5 才扩展 uitest。

## Related

- [BerryUI-F2核心盘点-v1.0.md](../../../docs/BerryUI-F2核心盘点-v1.0.md)
- [GUI框架地基路线-v1.19.md](../../../docs/GUI框架地基路线-v1.19.md)
