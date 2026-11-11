# Agent Note: BUI 定位与 BerryUI 单树收敛

Status: implemented — BUI 边界已入 ADR #66、命名体系 v1.1 与 GUI 地基路线 v1.2

## Problem

公共命名把 BUI 列为 UI 语言后，容易误读成「GUI app 必须写 .bui」或「.bui 是第二套布局/渲染框架」。现有 GUI-7 包格式又有 `views/main.bui` 占位文件，若不在文档中写清当前语义，后续 F4/SDK 工具可能提前冻结错误语法或制造第二事实源。

## Decision

BUI 定位为 BerryUI 的**可选声明式描述语言**；`.bui` 是源/资源文件。声明式不取决于载体是否为文件：Rust `ViewSpec`/`DescBuilder`、AST TLV 与未来 BUI 都必须收敛到同一棵 BerryUI retained tree。BUI 只描述组件树、属性、主题 token、状态/消息名绑定、无障碍与资源 id；禁止业务流程、命令式副作用、WM/channel/syscall、VMO/stride/frame、像素绘制与 x-y 散装布局。F4/F5 先用 Rust typed API；现有 `views/main.bui` 仅是资源挂点占位，v1 不解析、不执行、非必需。

## Alternatives considered

- **强制 GUI app 使用 `.bui`**：把 typed Rust API 降级为内部实现，降低类型安全并制造双入口；否决。
- **让 BUI 承载 onClick 业务逻辑**：会演化为脚本/热更新运行时，越过能力与安全边界；否决。
- **F4 前冻结 BUI 语法**：组件与测试面未稳定，过早背兼容负担；否决。
- **完全取消 BUI**：放弃预览器、结构化诊断、diff 与 AI 生成入口；保留为可选单 AST 入口。

## Consequences

BerryUI 的布局、状态失效、命中、渲染与帧循环仍只有一个引擎和一个事实源。SDK 后续可为 `.bui` 提供编译/解析/校验工具，但产物必须是 BerryUI AST TLV，并通过既有布局 dump、hit-test dump 与离屏 golden 验证。GUI app 的业务逻辑仍在 Rust update/message 面，不进入 UI 描述文件。

## Related

- [BerryOS命名体系-v1.1.md](../../../docs/BerryOS命名体系-v1.1.md)
- [GUI框架地基路线-v1.12.md](../../../docs/GUI框架地基路线-v1.12.md)
- [14-GUI与UI框架.md](../../../docs/规则/14-GUI与UI框架.md)
