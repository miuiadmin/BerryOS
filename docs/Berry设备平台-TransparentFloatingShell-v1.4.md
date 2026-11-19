Type: standard
Version: 1.4.0
Status: superseded
Superseded-by: [iOS视觉与流体交互复刻标准-v1.0.md](iOS视觉与流体交互复刻标准-v1.0.md)
Updated: 2026-11-18

# Berry设备平台-TransparentFloatingShell-v1.4：透明悬浮系统栏与安全区

## 1. 范围

本规格约束 1080×1920 产品相的 guest 系统壳与系统应用页面骨架：

- 持久系统状态栏与底部导航的归属与几何。
- 系统应用的浅色顶栏、图标返回、safe area 消费。
- 应用显式沉浸全屏时的系统保留窗隐藏与恢复。
- 列表卡片的按压反馈与输入消息分发纪律。

它不改变 present wire、Berry Room 权威路由、普通 app 能力模型或 soft↔vgpu 像素共识。

## 2. 系统壳

产品 launcher 在 WM GETINFO 后创建两个真实 SYSTEM layer 保留窗：

```text
status:     x=0 y=0 w=display_w h=117 @Product
navigation: x=0 y=display_h-108 w=display_w h=108 @Product
```

紧凑 GUI8 验证档不创建产品系统壳，继续使用外部 berry 工具栏，避免扰动既有 occlusion 负例。

普通 APP 窗口由 Berry Room 以 `WM_CREATE_F_EDGE_TO_EDGE` 创建。SYSTEM 保留窗在表内生效后，APP 收到 raw/visible 双几何与 `safe_top/safe_bottom`。APP 不自报 safe area；真值唯一来源是 wmd 权威窗口表。

launcher surface0 绘制系统壳像素：

- 状态栏与底部导航都是完全透明 SYSTEM overlay：背景来自桌面壁纸或应用延伸 chrome，不绘制全宽黑块/scrim；OpenHarmony 官方 signal/wifi/battery/clock 与 AOSP 三键仅以高对比 ink 悬浮显示。
- 状态栏与底部导航都是 launcher 拥有的全透明 SYSTEM overlay：普通 APP raw 全屏延伸到栏下，wmd 在每次 APP/桌面提交后仅重混合 glyph alpha。
- SYSTEM session 的 pointer 事件保持 display-global 坐标；APP window 事件仍为 raw window-local 坐标。
- 底部导航 UP 事件按屏幕三等分映射 BACK/HOME/TASK，复用与外部工具栏键完全相同的 launcher 动作路径；保留区事件不得再投递进桌面 UI 命中区。
- APP owner 首帧接管可视主体后，launcher surface0 只保留系统壳真值，不再全量重绘桌面覆盖 APP；HOME/BACK 回桌面时才恢复桌面帧。
- Recents owner 帧缓存以 raw 窗口几何识别同一真实帧；临时 SYSTEM overlay 造成的 effective 几何变化按当前 effective bounds 裁剪重放，不伪造像素。

## 3. 应用页面骨架

BerryUI SDK 提供：

```text
NavigationIconSource
底部三键像素来自 AOSP SystemUI 原始 VectorDrawable（Apache-2.0）：
`ic_sysbar_back.xml`、`ic_sysbar_home.xml`、`ic_sysbar_recent.xml`；
`tools/gen_android_nav_icons.py` 只解析原 pathData 并光栅化，不重画几何。
共享返回 glyph 仍导出为：
NAV_ICON_BACK / NAV_ICON_BACK_ACTIVE
NAV_ICON_HOME / NAV_ICON_HOME_ACTIVE
NAV_ICON_TASK / NAV_ICON_TASK_ACTIVE
Scaffold::with_back_safe
Scaffold::new_light_safe
```

系统应用必须满足：

1. 使用共享导航 glyph，不在各 app 内复制第二套返回像素。
2. 二级页面使用 `with_back_safe`：56dp 浅色顶栏、左侧图标返回、居中标题、显式 top/bottom safe area。
3. 顶层页面使用 `new_light_safe`：同几何的浅色标题栏，不放返回键。
4. 顶栏使用 `surface/text_primary`，不得把 `primary` 蓝色当作状态栏或标题栏背景。
5. Body 高度按 `viewport - safe_top - app_bar - bottom_h - safe_bottom` 计算；safe 保留带由 Scaffold 用应用 chrome 色连续填充，交互内容不得进入 safe 区。滚动内容另加的底部留白是内容 padding，不得冒充或改写 safe area。
6. 返回点击语义仍归 app 业务状态机；组件只负责 Press/Release/Focus/Click 视觉。

Settings、Contacts、Files、Photos、Browser 与 Berry Demo 均遵守本骨架。

## 4. 沉浸全屏策略

Berry Room 提供两种打开形态：

```text
open_window()             默认，携带 WM_CREATE_F_AVOID_SYS
open_window_immersive()   全屏沉浸，不携带 AVOID_SYS
```

约束如下：

1. 只有可见、raw 全 display、非 `AVOID_SYS` 的 APP body 才是 immersive truth；wmd 只报告该事实，不代理操作 SYSTEM 窗。
2. wmd 在 APP 首个成功合成 frame 后发送 `PE_FL_SYSTEM_APP_FRAME`；合成事件 `y=1` 表示 immersive。
3. launcher 是状态栏/导航窗唯一 owner：收到 immersive app frame 后对自己创建的两个 reserve 执行 `WM_SET_F_VIS=false`，并 ACK 生成的 VISIBILITY config；HOME/BACK 回桌面时恢复显示。TASK/Recents 期间保持隐藏。
4. Berry Room `open_window()` 默认携带 `WM_CREATE_F_EDGE_TO_EDGE`：raw/visible 均为全屏，0x67 safe area 仍由 wmd 从可见 SYSTEM 栏窗口推导（Product top=117/bottom=108）。`open_window_immersive()` 继续使用 flags=0；只有该形态触发两栏隐藏，普通 edge-to-edge APP 保持透明栏在内容之上。
4. immersive APP 的显式 WM_DESTROY 与进程死亡都由 wmd 转成 SYSTEM HOME invalidation，launcher 因此恢复 reserve；不得只在进程死亡路径收口。
5. Photos 是产品媒体沉浸面，使用 `open_window_immersive()`；其 app 内仍可用自己的返回顶栏，但不得伪造系统状态栏。
6. 普通app不得因全屏而获得 present、HDI、SYSTEM session 或直接改 SYSTEM 窗能力。

## 5. 列表与按压反馈

- Contacts 与 Files 的可选行使用 BerryUI `ListItem` 卡片，不再用文本按钮冒充列表卡。
- 常驻 Dock 图标与 SYSTEM Recents 卡片必须有 Press/Release/Cancel 视觉反馈。Dock Argb 图标用 2px paint offset；Recents 卡用换底 + 2px offset；二者宽高与兄弟布局零漂。
- 常驻 Dock 图标与 SYSTEM Recents 卡片必须有 Press/Release/Cancel 视觉反馈。Dock Argb 图标用 2px paint offset；Recents 卡用换底 + 2px offset；二者宽高与兄弟布局零漂。
- Settings 的设置行保存 `ListItem` 句柄并参与消息分发，不得建完即丢。
- `ListItem::on_msg` 消费 Press、Release、Cancel、Click 与 Focus；Click 返回激活结果。
- 应用必须把 BerryUI 产生的每条消息先分发给相关控件，再只对 `Msg::Click` 执行业务动作。只过滤 Click 的写法会丢弃按压反馈，禁止新增。
- 产生 damage 的 Press/Release 帧按停帧合同渲染；无 damage 不提交。

## 6. 验收

必须同批通过：

```text
just check
just product-e2e（recents/frame-cache/shell-press）
GUI8 vgpu --only-a
smoke-image 147/147
Photos immersive → TASK/Recents → HOME 恢复 e2e
Photos immersive 返回退出 → reserve 恢复 e2e
BerryUI host tests
berry-guiproto host tests
gui-wmd host tests
berry host tests
Curia strict / docs_guard
```

像素验收以浅色应用栏与持久系统壳为准；旧版 `(540,20)=primary` 断言不再适用，应更新为 surface/token 真值。

## 7. 边界

- 不给普通 app present/HDI 或直接写系统壳能力。
- 不在 app 内伪造状态栏、底部导航或 safe area。
- 不用宿主截图、timer 动画或插值补系统壳状态。
- 不把 SYSTEM 保留窗当作 app 可见内容区；其像素与输入语义归 launcher。
- 沉浸隐藏由 launcher 对自有 SYSTEM 窗执行；wmd 不得创建第二系统栏真相或替 app 持有 immersive 权限。
- 应用内底栏是 BerryUI `Tabs` 的 app 内容，位于 safe_bottom 之上；它不替代 SYSTEM BACK/HOME/TASK 三键，也不得请求系统栏能力。Berry Demo 六页已持久使用该底栏。
- Dock/Recents 按压反馈不新增 a11y/SEM 锚，也不改变 `PRESS_MAP_MAX`；命中语义仍由 launcher/wmd 唯一表派生。
- Dock/Recents 按压反馈不新增 a11y/SEM 锚，也不改变 `PRESS_MAP_MAX`；命中语义仍由 launcher/wmd 唯一表派生。


## Android 系统栏与 AOSP 资产契约

- launcher 在 product boot 创建两个真实 SYSTEM 窗后，为每窗协商 System Retained Surface，并把 bar-local ARGB VMO 提交给 wmd。两栏背景 alpha=0，仅 glyph 携带 alpha；wmd 缓存 READ 别名，后续每次 APP/full-desktop SUBMIT 后重混合可见 glyph。
- 底部三大金刚使用 AOSP `android-15.0.0_r1` SystemUI 原始 VectorDrawable path，源文件与 NOTICE 收在 `user/bins/berry-launcher/assets/android-15.0.0_r1/`。生成脚本解析 pathData 并抗锯齿光栅化； BerryOS 不自绘替代几何，也不声称 Android 商标归属。
- 桌面 surface0 不再烘焙状态栏/导航像素；SYSTEM overlay 是唯一系统栏像素源。全屏 Recents 仍是真实 SYSTEM 窗提交，不落入 bar overlay cache-only 路径。
- Scaffold 的 top safe reserve 使用 AppBar 同一 `surface` 色连续填充，形成摄像头/挖孔下的一体化顶栏；AppBar 文字、返回键与动作位仍在 safe_top 之下。
- Scaffold 的 bottom safe reserve 使用应用底栏同一 `surface` 色连续填充；Tabs/ToolBar/未来液态玻璃悬浮栏的交互命中面必须在 safe_bottom 之上。悬浮栏自身的圆角、玻璃 margin 与浮动留白属于 app bottom slot 内容，不得侵占权威 safe_bottom。
- 液态玻璃悬浮底栏的约束：`body_height = viewport - safe_top - app_bar - bottom_h - safe_bottom`。若系统三键可见，app 通过 bottom slot 抬高内容；玻璃面板只能在 bottom slot 内绘制，safe_bottom 仅允许背景色延伸，不允许放置可交互控件。
- 软件帧率仲裁见 `BerryDisplay-SoftwareLTPO-P1-v1.0.md`；QEMU virtio-gpu 无面板 LTPO mode-set，不得宣称硬件 LTPO。

## 后置验收