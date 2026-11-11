# Agent Note: BerryOS 公共架构命名体系定形

Status: implemented — 用户裁决生效；命名已入 ADR/规范，F1 工程名同步 Berry Room

## Problem

内核已有 Meihe 定名，但用户态、图形会话、桌面、应用运行环境、UI 框架与 SDK 缺少统一公共名。此前讨论中的 Stage、Scene、Hotel、Show、Shell 都与既有概念或外部系统语义冲突，继续漂移会污染文档、SDK API 与对外叙事。

## Decision

公共名冻结为：

```text
Meihe Kernel / Berry Space / Berry Session / Berry Home /
Berry Room / BerryUI / BUI / Berry SDK
```

`docs/BerryOS命名体系-v1.1.md` 是层级、中文名、工程落点、叙事与禁用名单的单一事实源；ADR #65 记录裁决。图形会话叫 **Berry Session**，per-app runtime 叫 **Berry Room**。F1 工程 crate 由 `berry-shell` 改名 `berry-room`，公开类型改为 `AppRoom`，UITEST 标记改为 `UITESTROOM ok`。

## Alternatives considered

- **Berry Stage**：与 OpenHarmony Stage Model 联想过强，且会话语义弱于 Session；否决。
- **Berry Scene**：更像 BerryUI 页面/场景图，不适合系统图形会话；保留给未来 UI 语义。
- **Berry Hotel**：与 Berry Home 的个人设备心智冲突，服务业隐喻过强；仅作解释隐喻。
- **Berry Show**：动词/节目联想，不能覆盖窗口、输入、合成与会话生命周期；否决。
- **Berry Shell**：与 bsh、宿主 `berry shell` CLI、desktop shell 冲突；禁用为 GUI 层名。

## Consequences

公共文档与 SDK 不得再造同层别名。`berry shell` 继续只指开发者设备壳 CLI。Berry Session 目前由 `guifbd`、`gui-inputd`、`gui-wmd` 组合实现，不要求立即新建单体 crate。Berry Room 是每个应用的运行室，未来多窗口仍是同一 Room 的资源集合。

## Related

- [BerryOS命名体系-v1.1.md](../../../docs/BerryOS命名体系-v1.1.md)
- [GUI框架地基路线-v1.11.md](../../../docs/GUI框架地基路线-v1.11.md)
- [14-GUI与UI框架.md](../../../docs/规则/14-GUI与UI框架.md)
