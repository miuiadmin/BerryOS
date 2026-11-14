# 双模式启动面与真实胶囊进度

Status: implemented

## Problem

默认启动条是矩形而非用户点名的 macOS 式长椭圆；装载器字节计数线性跳变不够平缓；装载器在 100% 结束而 vgpud 系统纪元从 45% 重启，视觉回退。系统也没有可选的命令行装载日志面。

## Decision

`berry-bootprogress` 升级为 Face 模型：默认 `Capsule` 与 `VerboseText` 消费同一 `(Stage, permille)` 真值流。图形面为黑底、BerryOS wordmark、白色长椭圆胶囊和深灰槽；verbose 面显示阶段、胶囊与最近 10 条 BerryBoot 诊断行。fw_cfg `opt/berry/bootmode=verbose` 选择命令行面，berry CLI 用 `BERRY_BOOTMODE=verbose` 透传。

真值侧新增 `span_smooth` 与 `slew`：前者只用真实 `done/total` 的 Q16.16 smoothstep 平滑文件/段读取，后者提供最多 8 permille/100ms、≥80 permille 或完成时 snap 的呈现限速模型。装载器纪元重标为 0..390，系统仍从 450 到 1000，消除跨纪元回退。vgpud 增量 damage 覆盖胶囊上下圆角清边。

## Alternatives considered

- 用 timer 动画补启动进度：会伪造未发生的系统状态，拒绝。
- 维持装载器 0..1000：vgpud 45% 首帧必然倒退，拒绝。
- 把 verbose 日志放 EFI 镜像全局 `.data`：该镜像单节文本对固件呈只读，实测数据异常；改为 `ProgressRuntime` 栈/运行时状态。
- 只做串口 verbose：违背“屏幕显示内核/驱动装载过程”的产品目标，本版先落屏幕装载日志，内核/服务日志总线在边界中明确未完成。
- 在普通 app 中画启动画面：启动面所有权归系统，拒绝。

## Consequences

默认产品 Recents/frame-cache e2e、verbose ramfb 截图、berry-bootprogress 8/8、GUI8 vgpu `--only-a`、smoke-image 147/147 与 `just check` 通过。当前 verbose 面覆盖 BerryBoot 装载/交接日志；内核与用户服务纪元日志、Esc 运行切换、auto-failure latch、handoff 携带最终 permille 与 shutdown face 仍待后续。

## Related

- [BerryBoot boot progress](2026-11-13-berryboot-boot-progress.md)
- [macOS white progress](2026-11-13-berryboot-macos-white-progress.md)
- [双模式启动体验调研](../../../research/BerryOS-双模式启动体验与真实进度算法-2026-11-v1.0.md)
