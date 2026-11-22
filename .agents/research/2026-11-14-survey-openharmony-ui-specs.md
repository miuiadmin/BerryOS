# OpenHarmony UI 组件规范调研（状态栏/标题栏/底部导航/按压反馈/卡片/设计 token）

Type: survey
Status: current
Updated: 2026-11-14

一句话说明：为 BerryOS 桌面与应用 UI「彻底对齐开源鸿蒙」提取可直接换算的规格——状态栏布局、应用标题栏与底部页签、按压反馈与卡片参数、设计 token；全部取自 OpenHarmony 官方源码与文档，逐条带出处；boot 动画与启动事件体系见姊妹篇 [2026-11-14-survey-openharmony-boot-animation-events.md](2026-11-14-survey-openharmony-boot-animation-events.md)。

## 问题

owner 2026-11-14 要求 BerryOS 模拟器 UI 彻底学开源鸿蒙（「你去学开源鸿蒙，我求你了，一定要做得彻底」），涉及六面：状态栏（高度/配色/左右内容/图标对齐）、应用顶栏（返回键位置）、底部导航（页签）、按压反馈（遮罩/缩放/圆角）、列表卡片、设计 token（深浅色/字号/留白）。本调研从 OH 官方仓提取可量化规格。

## 方法与来源

github.com/openharmony 官方镜像：`applications_systemui`（状态栏源码+资源）、`global_system_resources`（token json）、ArkUI 组件文档（Navigation/Tabs/clickEffect/stateStyles/ListItem/GridItem）。40 份源文件缓存于本机 `~/.hermes/cache/scratch/oh/`（会话缓存不入库，事实以本文为准）。全部结论一手源码级核对，非社区转述。

## 结论

以下 vp 值按 BerryOS 1080×1920 @3x（360vp 基准）可直接换算（×3 = px）。

### 状态栏（SystemUI StatusBar）

- 高：手机竖/横屏统一 36vp（换算链：string.json `phone_status_bar_size_portrait=54` 基准 540 宽 × 1080 = 108px = 36vp@3x）；窗口最小高约束 24vp。
- 布局三组（StatusbarConfig LayoutGroups，weight 1/0/1 = Start/Center/End 对齐）：左组 = 信号 → WiFi → 胶囊 → 通知图标（最多 3 个 + 省略号，小图标 12vp、间距 8vp）；中组空；右组从左到右 = 位置 → 蓝牙 → 响铃 → NFC → 飞行 → 电池 → 时钟。**时间恒在最右，左侧永不放应用名**——桌面与应用内同一套。
- 图标：常规 24vp×24vp、图标间距 8vp、图标左右 margin 1vp、状态栏左右 padding 24vp、字号 12fp；信号 32×24vp；电池矢量绘制（34.5×19vp 本体 + 1vp 边框 + 2vp 圆角 + 3×10vp 正极头）；时钟宽 60vp。
- 背景：状态栏窗口本身透明（`setWindowBgColor '#00000000'`），背景色由应用经 `systemBarTintChange` 下发——应用侧默认 40% 黑（`#66000000`）、内容色 `#E5FFFFFF`；API 10+ 沉浸式默认透明底黑字。支持左/中/右三段着色。
- 下拉：状态栏下拉 >18px 触发面板（左半通知/右半控制中心）。

### 应用页面结构

- 标题栏（Navigation）：Mini 小标题模式高 56vp，返回键仅 Mini 模式显示、位于左上（系统返回箭头，可换符号图标）；标题起始内边距 16vp；右上 menus 放操作项。Full 大标题 112vp（主+副 138vp）；Free 模式随滚动联动。
- 底部页签（Tabs + BottomTabBarStyle）：页签栏高 48vp（图标+文字形态），背景默认透明；切换点击曲线 `cubicBezierCurve(0.2, 0.0, 0.1, 1.0)` 300ms；侧边页签宽 96vp。

### 按压反馈与卡片

- clickEffect：LIGHT 档按下缩放 0.90，MIDDLE/HEAVY 档 0.95（按下缩小、松手回弹）。
- 多态样式：normal/pressed/disabled/focused/selected 五态 + hover 悬浮态。
- token：按压高亮背景 `ohos_id_alpha_highlight_bg = 0.2`（20% 黑遮罩）；常规 10%；按压圆角 `ohos_id_corner_radius_clicked = 8vp`；按钮圆角 20vp、小按钮 14vp。
- 卡片：`ListItemStyle.CARD` 高 48vp、宽 100%、左右内边距 8vp，自带 focus/hover/press/selected/disable 五态默认样式；`GridItemStyle.PLAIN` 有 Hover/Press 态。

### 设计 token（浅色/深色）

| token | 浅色 | 深色 |
|---|---|---|
| 背景 `ohos_id_color_background` | #FFFFFFFF | #ff18181a |
| 次背景 `ohos_id_color_sub_background` | #fff1f3f5 | #ff000000 |
| 强调 `ohos_id_color_emphasize` | #ff007dff | #ff006cde |
| 主文字 `ohos_id_color_text_primary` | #ff182431 | #dbffffff |
| 次文字 `ohos_id_color_text_secondary` | #99182431 | #99ffffff |
| 标题栏图标 `ohos_id_color_titlebar_icon` | #E5182431 | #dbffffff |
| 按钮底 `ohos_id_color_button_normal` | #0c182431 | #19ffffff |

- 字号：body1 16fp / body2 14fp / body3 12fp；caption 10fp；标题 18~96vp 分级。
- 留白：页左右边距 16vp；默认起/止 padding 12vp（最大 24vp）。
- 深浅色 = 资源限定词目录 `dark/`，优先级 应用调用 > dark 资源 > 跟随系统。

### 开机动画与首次引导

- bootanimation 结构、bootevent 退出握手、OTA 优化进度条机制详见姊妹篇（文首链接）；要点：常规开机无进度条，退出 = 轮询 `bootevent.boot.completed`（30ms 间隔，默认上限 60s）。
- **开源 OH 标准系统没有 setup wizard/OOBE 应用**（applications/standard manifest 全集核对无 wizard/provisioning 仓），首启直进 launcher、权限走 install_list 预授权；商用 HarmonyOS 的向导闭源无规范可引——BerryOS 引导流程需自设计，可借上述 token 体系。

## 证据与出处

- 状态栏高度与换算：`applications_systemui` `product/phone/statusbar/src/main/resources/base/element/string.json`、`product/phone/statusbar/src/main/ets/ServiceExtAbility/ServiceExtAbility.ts`、`common/src/main/ets/default/heightcofigUtils/HeightConfigUtils.ts`（github.com/openharmony/applications_systemui master）。
- 状态栏布局/图标/透明：`product/phone/statusbar/src/main/ets/pages/common/StatusbarConfig.ts`、`features/statusbarcomponent/src/main/resources/base/element/float.json`、`product/phone/statusbar/src/main/ets/pages/index.ets`、`common/.../TintStateManager.ts`；应用侧默认值 = 窗口 API 文档 `arkts-apis-window-i.md` SystemBarProperties。
- 标题栏/页签/按压/卡片：ArkUI 文档 `ts-basic-components-navigation.md`、`ts-container-tabs.md`、`ts-container-tabcontent.md`、`ts-universal-attributes-click-effect.md`、`ts-universal-attributes-polymorphic-style.md`、`ts-container-listitem.md`、`ts-container-griditem.md`（docs.openharmony.cn / gitee openharmony/docs）。
- token：`global_system_resources/systemres/main/resources/{base,dark}/element/*.json` + 文档 `resource-categories-and-access.md`、`ui-dark-light-color-adaptation.md`。
- OOBE 缺席：github.com/openharmony/manifest `ohos/ohos.xml` applications/standard 全集 + org 搜索 wizard/oobe 零命中；预授权 = `subsys-app-privilege-config-guide.md`。

## 适用边界

状态栏 36vp 是从资源值+换算逻辑推得（OH 未直接以 vp 文档化）；HarmonyOS Design 官网规范属闭源生态未作为依据；全部规格取 master 分支（未锁 tag，随上游漂移需复核）；本调研只取规格不涉实现移植方案（方案见 [2026-11-14-模拟器体验统一分刀方案.md](../plans/2026-11-14-模拟器体验统一分刀方案.md)）。
