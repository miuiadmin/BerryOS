# BerryOS UI 组件清单与实施规划 v1.0

> 版本：1.0 ｜ 2026-11-14
> 【策略】公集合优先（五系统都有=必须有）→ 多数集合（≥3 系统=应该有）→ 差异精选
> 【目标】组件数量至少与任意单个系统看齐（≈70 件对标 ArkUI 单设计系统最大值）
> 【来源】五系统全景调研（ArkUI 95 / Compose 92 / Flutter 185 / SwiftUI 100 / Qt 110）

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
