# Agent Note: GUI F1 落地 berry-shell 宿主

Status: implemented — berry-shell + UITEST host probe 已过 GUI8/smoke-image 全链

## Problem

五个 Native Shell 各自复制 APP env、WM connect、字形 VMO、窗口 frame、present、INPUT_POLL 与 destroy 代码。该形态能运行样例，但每个 app 都携带半套系统客户端，后续 UI SDK 迁移会继续复制并放大边界漂移。

## Decision

按 [GUI框架地基路线-v1.8.md](../../../docs/GUI框架地基路线-v1.8.md) F1 落地 `user/berry-shell`：宿主统一解析 env、建立 WM/input session、接收并映射 per-app 字形 VMO、创建全屏窗口与 frame VMO、执行 SUBMIT/FRAME_CALLBACK/RELEASE、暴露 pointer 事件并显式 WM_DESTROY/空表回读/解映射/关句柄。新增 `user/bins/uitest` 作为零像素 host probe：app 只依赖 `berry-shell`，frame VMO 保持零页，真实提交后以 owner-local PhaseUP 驱动销毁退出。CAMERA 静默占位包被 UITEST 替换，app 总数保持 9。

## Alternatives considered

- **继续让每个 Native Shell 自带 WM/VMO 客户端**：短期少改，但 F6 迁移仍要逐 app 抽取，边界无法机械验证；否决。
- **直接迁移 Settings 到新宿主**：跳过独立 host probe 会把宿主错误与业务 UI/状态耦合，失败难归因；F1 先用 UITEST 隔离验证。
- **新增第十个 app 包**：rootfs 余量紧且 `APP_SLOTS=9`；用 UITEST 替换 CAMERA 占位保持应用数与拓扑帽不变。

## Consequences

GUI8 normal 现在在五个 Native Shell 后追加 UITEST 真实运行时：第三格 APP_SPAWN、1080×1920 窗口、零页帧四点截图恒黑、PhaseUP 局部坐标 `(539,799)`、销毁与第二进程退出。GUI8 full 的 owner route 随第九个 boot WM session 迁到 `sid=10`；smoke-image 强制 UITEST boot-probe，ECO 97 / 总标记 138。现有五个 Native Shell 尚未迁移，属 F6..F8 处理面；`berry-nshfont` 仍待 F9 拆分退役。

## Related

- [GUI框架地基路线-v1.8.md](../../../docs/GUI框架地基路线-v1.8.md)
- [14-GUI与UI框架.md](../../../docs/规则/14-GUI与UI框架.md)
