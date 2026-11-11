# Agent Note: GUI 多形态 UI 契约采纳

Status: implemented — ADR #67 与 GUI 地基路线 v1.3 已落册；本轮只立契约与排刀，不新增运行时实现

## Problem

GUI-12 深研显示，简单移动应用、Photoshop 级工作台、AAA 游戏与引擎编辑器的输入范式、帧模型、文档模型和渲染路径不同。若把 BerryUI 当作所有界面的唯一控件与渲染路径，会造成控件 API 膨胀、游戏/专业 canvas 帧模型劣化，并迫使 IME、无障碍、焦点和桌面控件需求过早进入移动组件刀。

## Decision

用户确认采纳「一个系统契约，多个 UI 形态」：Berry Session/Berry Room 固化窗口角色、surface、输入、IME、无障碍、帧率、DPI/safe area、系统层序与进程清理；BerryUI 是默认声明式工具包而非特权框架。普通应用走 BerryUI；专业应用走 BerryUI 外壳 + CanvasSurface；游戏/实时应用走 immersive surface 与自有 renderer。GUI 地基路线升 v1.3，在 F5 之后、F6 之前新增 P0「UI 形态与 Surface Mode 协议」。

## Alternatives considered

- **BerryUI 包揽所有形态**：被 GUI-12 产业证据否决。
- **两套完整 UI 全栈**：只取 Qt「共享 core、分化组件集」的教训，否决双输入/IME/a11y/主题税。
- **OS 游戏控件库**：游戏 UI 通常由引擎同帧自绘，OS 应提供 surface/input/IME/a11y 契约，否决。
- **F4 立即扩桌面/游戏**：违反地基优先，否决；P0 协议先行。

## Consequences

F4/F5 出口门不变。P0 必须冻结 Surface role/mode、input class、IME composition、a11y sidecar、safe area/scale factor 与 canvas/immersive escape hatch，并通过 host 编解码与拒面测试后才能开 F6。2026-11-08 规则补漏将现行路线升至 v1.4：P0 编号限定为 GUI Surface Mode 插入门；普通 BerryUI app 与专业/游戏 renderer 的像素边界、语义旁车和静态守卫例外分别立法。自绘应用不能直写屏或绕过 Berry Room 能力；BerryUI 未来桌面专业件共享 retained core，但焦点/键盘、文本编辑/IME、虚拟化与 canvas surface 缝必须在 P0 和后续桌面件刀中显式设计。

## Related

- [GUI-12-复杂应用与游戏UI形态深研-2026-11.md](../../../research/GUI-12-复杂应用与游戏UI形态深研-2026-11.md)
- [GUI框架地基路线-v1.13.md](../../../docs/GUI框架地基路线-v1.13.md)
- [14-GUI与UI框架.md](../../../docs/规则/14-GUI与UI框架.md)
