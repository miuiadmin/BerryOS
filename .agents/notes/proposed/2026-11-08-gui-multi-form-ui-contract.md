# Agent Note: GUI 多形态 UI 契约提案

Status: proposed — 等用户裁决后立 P0 ADR；不改 F4/F5 现行出口门

## Problem

BerryOS 的长期形态同时包含简单移动应用、PC 专业工作台、游戏/实时应用与系统壳。若把 BerryUI 解释成“所有界面唯一的控件与渲染路径”，会把游戏帧、专业 canvas、无障碍、IME、桌面控件和低功耗普通 UI 塞进同一控件库，导致 API 膨胀、帧率/功耗模型劣化和后期协议返工。

## Proposal

采用“一个系统契约，多个 UI 形态”：Berry Session/Berry Room 固化窗口、surface、输入、IME、无障碍、帧率、DPI/安全区、系统层序与安全清理；BerryUI 是默认声明式工具包，不是特权或强制框架。普通应用走 BerryUI；专业应用走 BerryUI 外壳 + canvas surface；游戏/实时应用走 immersive surface 与自有 renderer。F4/F5 仍只做移动组件与 uitest；F5 后单独立 P0 ADR 定 Surface role/mode、input class、IME composition、a11y sidecar 与 canvas escape hatch。

## Alternatives considered

- **BerryUI 包揽所有形态**：游戏与专业 canvas 会被控件树和低功耗停帧模型拖累；被否。
- **Qt 式两套完整 UI 全栈**：输入、IME、主题、无障碍都要付双栈税；只取“共享 core、分化组件集”的教训；被否。
- **OS 提供游戏控件库**：产业先例中游戏 UI 基本由引擎自绘并同帧提交；OS 应提供 surface/input/IME/a11y 契约；被否。
- **当前立即扩 F4 范围**：地基路线会被桌面/游戏半成品稀释；协议先立、实现后置；被否。

## Consequences

现有 BerryUI core、主题 token、诊断 dump 与 Berry Room 生命周期继续有效。未来桌面专业件可共享 retained core，但需要焦点/键盘、文本编辑/IME、虚拟化与 canvas surface 缝。游戏不消费 BerryUI 控件，但仍必须通过 Berry Room 获得 surface/input 能力并上报可选语义旁车；不能直写屏或绕过焦点/进程清理。该提案未生效前，不新增实现面。

## Related

- [GUI-12-复杂应用与游戏UI形态深研-2026-11.md](../../../research/GUI-12-复杂应用与游戏UI形态深研-2026-11.md)
- [GUI框架地基路线-v1.15.md](../../../docs/GUI框架地基路线-v1.15.md)
- [BerryOS命名体系-v1.1.md](../../../docs/BerryOS命名体系-v1.1.md)
