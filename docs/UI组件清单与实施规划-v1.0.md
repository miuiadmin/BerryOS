# BerryOS UI 组件清单与实施规划 v1.0

> 版本：1.1 ｜ 2026-11-15
> 【策略】公集合优先（五系统都有=必须有）→ 多数集合（≥3 系统=应该有）→ 差异精选
> 【目标】组件数量至少与任意单个系统看齐（≈70 件对标 ArkUI 单设计系统最大值）
> 【来源】五系统全景调研（ArkUI 95 / Compose 92 / Flutter 185 / SwiftUI 100 / Qt 110）
> 【v1.1】2026-11-15：新增「八、容量域纪律」——五键导航条合入把桌面 a11y 挂钩顶到 34 超过 `A11Y_MAX=32`，launcher 建树 `NodeFull` 静默停泊致 1080p 桌面恒黑（[会审](../.agents/research/2026-11-15-1080p恒黑根因会审.md)）——容量常数的账目纪律自本节起为 UI 变更的硬门槛。

## 一、三层落位架构

| 层 | 位置 | 件型 | 示例 |
|---|---|---|---|
| kind 件 | desc.rs ViewKind + Props + TLV | 叶子/容器 | Text / Button / Col / Grid |
| 模式件 | DescBuilder 组合糖 | 组合配方 | AppBar / Scaffold / Nav |
| 服务件 | berry-guiproto 三域协议 | overlay 窗/系统层 | Dialog / Menu / Toast / SoftIME |

## 二、P0 公集合（五系统都有）——20 主组件

| # | 组件 | 落位 | 状态 |
|---|---|---|---|
| 1 | Text | kind | ✅ 已在位 |
| 2 | Image | kind | ✅ 已在位 |
| 3 | Icon | kind | ✅ 已在位 |
| 4 | Col/Row | kind | ✅ 已在位 |
| 5 | Grid | kind | ✅ 已在位 |
| 6 | Pager (SwipePager) | kind+控制器 | ✅ 已在位 |
| 7 | Button（含 icon 槽） | kind | 🆕 |
| 8 | Toggle | kind | 🆕 |
| 9 | Slider | kind | 🆕 |
| 10 | Progress（linear/circular） | kind | 🆕 |
| 11 | Select | kind | 🆕 |
| 12 | Input（单行） | kind | 🆕（依赖输入焦点协议） |
| 13 | Edit（多行） | kind | 🆕 |
| 14 | Scroll | kind | 🆕 |
| 15 | List（懒列表） | kind | 🆕 |
| 16 | Tabs | kind | 🆕 |
| 17 | Dialog | 服务件 overlay | 🆕 |
| 18 | Menu（含 Context） | 服务件 overlay | 🆕 |
| 19 | AppBar | 模式件 | 🆕 |
| 20 | Nav 页面栈 | 模式件 | 🆕 |
| + | Canvas 自绘 | kind | 🆕 |

## 三、P1 多数集合（≥3 系统）——25 组件

Stack / Spacer / Divider / Check / Radio / Step / Search / Seg / 文本选择 /
Sbar / Sheet / Toast / Badge / Tip / Scaffold / BottomNav / Drawer /
Refresh / SwipeItem / DatePick / TimePick / IconButton（变体）/
Video（v2 门）/ Web（v2 门）/ 富文本（变体）

## 四、P2 BerryOS 精选——16 组件

借来件（<3 系统但有场景）：DragReorder / Collapse / Chip / Fab / Card /
Gauge / Qr / WaterFlow（v2）

系统件（微内核特有，五框架均无）：SoftIME / WindowChrome /
StatusBar+NavBar / Wallpaper / LauncherGrid / PowerDialog / BootSplash

## 五、实施分期

| 期 | 内容 | 新增 | 前置 |
|---|---|---|---|
| Phase 1 最小桌面 | P0 基础+容器 | 7 kind | 零协议改动 |
| Phase 2 完整桌面 | P0 全部+P1 | ~28 | gui-wmd overlay 窗型+输入焦点协议 |
| Phase 3 对标完整 | P2 全部 | 16 | List 懒化+v2 编解码 |

## 六、总数核账



## 七、显式不选（防 scope creep）

Rating（无应用商店）/ Marquee（Text+anim 可达）/ AlphabetIndexer（无通讯录）/
DataPanel（Gauge 已覆盖）/ RichTextEditor（v2）/ Map / Charts / Cupertino 第二风格

## 八、容量域纪律（v1.1 立——2026-11-15 `A11Y_MAX` 回归判例的规范产物）

berry-ui 的定容资源是**账目**，不是无限供给；任何往桌面树/组件上加节点的变更必须对账。已知容量域（值以 [lib.rs](../user/berry-ui/src/lib.rs) 现值为准，变更须同步本表）：

| 容量域 | 常量 | 2026-11-15 时点 | 判例 |
|---|---|---|---|
| a11y 侧表（兼任 Ui 容量域满报错面） | `A11Y_MAX` | 修复后 40（桌面大屏档实需 34） | 五键 +2 顶穿 32 → `NodeFull` → 桌面恒黑（`01bb3e37a` 修） |
| 节点 arena | `ARENA` 帽 512 | 桌面账 372（Rect 按压面摘除后） | 贴近上限是「下颗雷」（同修复顺带摘 80+5 面） |
| 按压映射 | `press_map` 28 | launcher 建树尾全量登记 | 帽 16→28 先例 |
| 帧弹舱/通道 | `INPUT_REP_MAX` 244 等 | guiproto 定长 | 分辨率无关，改动走 TLV add-only 纪律 |

硬门槛（UI 变更评审清单）：

1. **加挂树件先对账**：新增 kind/模式件/导航键/dock/状态栏件，算清各档（大屏/紧凑/小屏无 dock）三本账并列进 PR/刀说明；贴限（≥85%）就必须同批抬帽或减面。
2. **容量常量改动单列**：抬 `A11Y_MAX`/arena 等必须独立提交并写明新账目（实需值 + 余量），禁止与功能改动混提。
3. **失败必须显眼**：建树/挂载路径的 `fail()`/Err 输出带可 grep 前缀（如 `!! LAUNCH FATAL`）——11-15 判例里 fail 行混自测噪声流肉眼必漏，放大成「串口全绿但恒黑」的误判。
4. **对照测试同批构建**：分辨率/档位对照必须同镜像同批产物——跨批次对照制造了「480/800 正常、1080 黑」的分辨率假象（真因与分辨率无关）。
5. **账目护栏进 CI（建议）**：a11y/arena/press_map 账目以静态断言或单测数钉住——树增长先红测，不再红屏。
