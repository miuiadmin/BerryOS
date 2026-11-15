# OpenHarmony 式产品系统壳

Status: implemented

## Problem

产品相此前的系统壳仍是半成品：系统应用用文字 “Back” 冒充返回，顶栏把 `primary` 蓝色铺满状态区；状态栏缺少真实的 signal/wifi/battery/clock 系统语义；底部没有可操作导航；列表与卡片只响应 Click，没有 Press/Release 反馈。直接把 APP 画成全屏还会覆盖系统壳，而把系统壳画在 APP 里又会伪造 safe area 与系统权限。

## Decision

建立 1080×1920 产品系统壳单一契约，见 [Berry设备平台-OpenHarmonyShell-v1.1.md](../../../docs/Berry设备平台-OpenHarmonyShell-v1.1.md)。launcher 创建真实 SYSTEM layer 保留窗：顶部 117px 状态栏与底部 108px 导航。状态栏为黑色系统面并保留官方 OpenHarmony 派生的 signal/wifi/battery/clock 资产；底部黑色系统面内烘焙三枚原创几何返回/Home/任务 glyph，UP 按屏幕三等分映射既有 BACK/HOME/TASK 动作。SYSTEM 保留区 pointer 保持 display-global，不进入桌面命中树；APP pointer 保持 raw window-local。

BerryUI 新增 `NavigationIconSource` 与六个共享导航 glyph 常量，新增 `Scaffold::with_back_safe` / `Scaffold::new_light_safe`。二级页使用浅色 56dp 顶栏、左侧 48dp 图标返回、居中标题与 Berry Room authoritative safe area；顶层页使用同几何浅色标题栏。Settings、Contacts、Files、Photos、Browser、Berry Demo 全部迁移，列表行改为 `ListItem` 卡片并分发 Press、Release、Cancel、Click 与 Focus 消息。

沉浸全屏补上第二个产品形态：Berry Room 提供 `open_window_immersive()`；wmd 只把“可见 raw 全屏且非 AVOID_SYS 的 APP”识别为 immersive，并在首个成功合成 frame 的 `PE_FL_SYSTEM_APP_FRAME` 事件 y 位报告。launcher 保持两个 SYSTEM reserve 的唯一 owner：收到该事件后对自有状态栏/导航窗执行隐藏并 ACK VISIBILITY config；TASK/Recents 期间保持隐藏，HOME/BACK 恢复。Berry Room 的显式 WM_DESTROY 发生在进程 TERMINATED 之前，因此 wmd 对两条退出路径都合成 SYSTEM HOME invalidation，保证 Photos 返回退出同样恢复系统栏。Photos 作为媒体应用迁移到该形态。

合成所有权归 wmd：只要可见全宽 APP 主体持有焦点，surface0 的陈旧桌面 repaint 收到 `ST_BUSY`，不伪造 present token；HOME/BACK 先由 WmTable 清焦再允许桌面真值重绘。Recents owner 缓存以 raw 几何识别同一真实帧，临时 overlay 造成的 effective 几何变化按当前 effective bounds 裁剪重放。Berry Demo 结构性换页强制 full damage，防止新增 AppBar glyph 落在旧页损伤集外。

## Alternatives considered

- 抬 `MAX_NODES` 或 a11y 容量：P1/Recents 证据已证明满帽是生命周期问题；底部导航改烘焙进 launcher 壁纸缓存，不新增桌面树节点。
- 在 APP 内自画状态栏/底栏或自报 safe area：破坏 Berry Room 与 wmd 权威，拒绝。
- 让 launcher 猜测 “APP 拥有 surface” 并抑制 surface0：GUI8 的预启动 APP_FRAME 积压和无窗口 APPDEMO/APPQUIET 会永久阻断桌面；改为 wmd 按可见全宽焦点窗执法。
- 把 SYSTEM 保留区 pointer local 化再由桌面节点命中：会把打开 Recents 的 UP 泄漏成任务卡 Click；保留区在投递前消费。
- 让 wmd 直接隐藏 SYSTEM 窗：代理了 launcher 所有权，且早期实验在 reserve config 与 Recents CREATED 交错时挂住创建链；改为 launcher 显式 SET + drain/ACK。
- 让 Photos 自画透明系统栏：伪造系统壳与 safe area；改为 Berry Room 显式 immersive 请求。
- 用宿主截图或 timer 插值宣称系统壳/任务恢复：拒绝，所有像素来自真实 launcher/APP/replay 帧。

## Consequences

产品 Recents e2e、owner frame-cache e2e、Photos immersive→TASK/Recents→HOME 恢复 e2e、Photos immersive 返回退出→reserve 恢复 e2e、GUI8 vgpu `--only-a`、smoke-image 147/147、BerryUI host、berry-guiproto host、gui-wmd host、tools/berry host、SDK ledger 119 项与 `just check` 均通过（后续控件相位把 SDK ledger 扩至 150 项）。1080×1920@60、HVF、virtio-gpu、present wire、Berry Room 与 soft↔vgpu 共识不变；普通 APP 仍无 present/HDI/SYSTEM 能力。

## Related

- [BerryUI AppBar D3](2026-11-13-berryui-appbar-d3.md)
- [SYSTEM Recents and APP first frame](2026-11-14-system-recents-app-first-frame.md)
- [Recents owner frame cache](2026-11-14-recents-owner-frame-cache.md)
- [GUI 60Hz stage 3 real samples](2026-11-15-gui-60fps-stage3-real-samples.md)
