# Agent Note: 刀 4 AppShell 与应用 UI 承载链缺口

Status: implemented

## Problem

owner 2026-11-15 投诉「系统应用 back 返回还是一串文字、顶栏没有、底栏没有、应用内无按压无卡片」——根因不在组件层（TopBar/TabBar/卡片/按压已有），而在**应用承载链断裂**：appdemo 等 APP_SPAWN 应用拿不到 wm 客户端端（extras gift 位只给 svcmgr 预拉面：launcher/gfxdemo/wmd），应用无法建窗，UI 面根本不存在。

## Decision

- **AppShell 脚手架**（berry-ui components.rs）：应用页标准结构一次装配——TopBar（56vp，M3 TopAppBar=OH Navigation Mini=iOS NavBar 合体值）含返回键（Pad 叠加容器：Rect 按压面 + ICON_ID_BACK 箭头——BadParent 判例后合法结构）+ 标题 + 右槽；内容槽；TabBar（48vp，页签帽 5——iOS 官方建议 3-5）。
- **标准图标 id 约定表**起表：ICON_ID_BACK=1（供件侧 IconSource 按 id 绘制；应用自定义图标 0x100 起）。图标资产归属供件（launcher 供 NAV_BACK=7 的映射义务挂刀 5）。
- **appdemo UI 面首段**：wm GETINFO→CREATE(AVOID_SYS) 骨架落地，wm 缺席干净降级（`app APPDEMO ui wm absent` 一行——gfxdemo 先例，永不进 MARKERS）；既有自检职责（argv0/mtab/exit 0）零漂移。

## Verification

berry-ui 组件测 10/10 绿（AppShell 树结构+页签测量+返回键 Icon kind）；appdemo musl 链绿；全链镜像重打 + noktest boot 复验（LAUNCH v=1、无 desktop build err、桌面帧壁纸+图标正常）。

## Consequences

- **缺口挂账（刀 5 主战役）**：appd/spawn 面给 APP_SPAWN 应用交付 wm 客户端端 dup（gift 位扩展）+ present 端——应用建窗→AppShell 装配→soft 渲→SUBMIT 全链打通后，「应用内状态栏避让（AVOID_SYS 生效）/返回键退回桌面/按压反馈」三件才真活。
- 状态栏 SYSTEM 层常驻（owner「应用进去了状态栏也要显示」）依赖同一链：状态栏迁 wmd 系统窗（WM_LAYER_SYSTEM）后 AVOID_SYS 自动避让才生效——GUI-8 裁决 e（状态栏=launcher 进程内）需在刀 5 翻案为 wmd 系统窗。
- TabBar 切换动画/press 缩放反馈挂刀 5 组件批。

## Related

- [2026-11-15-控件并集矩阵.md](../../plans/2026-11-15-控件并集矩阵.md)
- [2026-11-15-ux-knife3-components.md](2026-11-15-ux-knife3-components.md)
