# 双模式启动面与真实胶囊进度

Status: implemented

## Problem

默认启动条是矩形而非用户点名的 macOS 式长椭圆；装载器字节计数线性跳变不够平缓；装载器在 100% 结束而 vgpud 系统纪元从 45% 重启，视觉回退。系统也没有可选的命令行装载日志面。

## Decision

`berry-bootprogress` 升级为 Face 模型：默认 `Capsule` 与 `VerboseText` 消费同一 `(Stage, permille)` 真值流。图形面为黑底、BerryOS wordmark、白色长椭圆胶囊和深灰槽；verbose 面显示阶段、胶囊与最近 10 条 BerryBoot 诊断行。fw_cfg `opt/berry/bootmode=verbose` 选择命令行面，berry CLI 用 `BERRY_BOOTMODE=verbose` 透传。

后续 v1.2 把系统纪元收敛为 11 点 `Milestone` 单源表（450/500/520/560/620/650/680/700/800/900/1000），op7 的原保留字节 2 承载稳定 id，非 0 id 的 stage/permille 三元组在编码与解析双侧拒伪。内核把 fw_cfg bootmode 以 fbinfo 第 7 域交给 svcmgr，由 vgpud argv 选择系统 verbose face；该面显示 milestone 罐头短行而非自由文本。首个真实 SUBMIT 后，迟到 milestone 是 OK 幂等 no-op，不能把 boot face 重新选回 scanout。

真值侧新增 `span_smooth` 与 `slew`：前者只用真实 `done/total` 的 Q16.16 smoothstep 平滑文件/段读取，后者提供最多 8 permille/100ms、≥80 permille 或完成时 snap 的呈现限速模型。装载器纪元重标为 0..390，系统仍从 450 到 1000，消除跨纪元回退。vgpud 增量 damage 覆盖胶囊上下圆角清边。

## Alternatives considered

- 用 timer 动画补启动进度：会伪造未发生的系统状态，拒绝。
- 维持装载器 0..1000：vgpud 45% 首帧必然倒退，拒绝。
- 把 verbose 日志放 EFI 镜像全局 `.data`：该镜像单节文本对固件呈只读，实测数据异常；改为 `ProgressRuntime` 栈/运行时状态。
- 只做串口 verbose：违背“屏幕显示内核/驱动装载过程”的产品目标，本版先落屏幕装载日志，内核/服务日志总线在边界中明确未完成。
- 在普通 app 中画启动画面：启动面所有权归系统，拒绝。
- 把自由文本日志放进 present wire：会给可达参与者伪造系统状态的能力，改为系统 truth table 罐头短行。
- 让 gui-wmd 容忍 vgpud 的 ST_BUSY 迟到里程碑：会吞掉真实渲染/backing 失败；vgpud 改为真实帧拥有 scanout 后返回 OK 幂等确认。
- 在 GUI8 探测相强行补发 fsmgr/vnet 里程碑：需把 svcmgr 私有 dup 生命周期延到 gfxdemo 首个真实 SUBMIT 之后，共享 present 应答队列会出现跨消费者互截；保留 1..5、8 的拓扑例外。

## Consequences

默认产品 Recents/frame-cache e2e、verbose ramfb/system 截图、berry-bootprogress host、GUI8 vgpu `--only-a`、smoke-image 147/147 与 `just check` 通过。当前 verbose 面覆盖 BerryBoot 装载诊断与系统 milestone 短行；内核全文/驱动自由文本日志、Esc 运行切换、auto-failure latch、handoff 携带最终 permille 与 shutdown face 仍待后续。

## Related

- [BerryBoot boot progress](2026-11-13-berryboot-boot-progress.md)
- [macOS white progress](2026-11-13-berryboot-macos-white-progress.md)
- [双模式启动体验调研](../../../research/BerryOS-双模式启动体验与真实进度算法-2026-11-v1.0.md)
