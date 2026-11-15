Type: standard
Version: 1.1.0
Status: current
Updated: 2026-11-15

# Berry 设备平台 OpenHarmony 式系统壳（v1.1：沉浸全屏）

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

普通 APP 窗口仍由 Berry Room 以 `WM_CREATE_F_AVOID_SYS` 创建。SYSTEM 保留窗在表内生效后，APP 收到 raw/visible 双几何与 `safe_top/safe_bottom`。APP 不自报 safe area；真值唯一来源是 wmd 权威窗口表。

launcher surface0 绘制系统壳像素：

- 顶部状态栏为黑色系统面，保留既有 OpenHarmony 官方 signal/wifi/battery/clock 资产与对齐；左侧不是 app 名称，底部也不得把壁纸或 `primary` 蓝色当作状态栏。
- 底部导航为黑色系统面，三枚原创几何 glyph：返回、Home、任务。
- SYSTEM session 的 pointer 事件保持 display-global 坐标；APP window 事件仍为 raw window-local 坐标。
- 底部导航 UP 事件按屏幕三等分映射 BACK/HOME/TASK，复用与外部工具栏键完全相同的 launcher 动作路径；保留区事件不得再投递进桌面 UI 命中区。
- APP owner 首帧接管可视主体后，launcher surface0 只保留系统壳真值，不再全量重绘桌面覆盖 APP；HOME/BACK 回桌面时才恢复桌面帧。
- Recents owner 帧缓存以 raw 窗口几何识别同一真实帧；临时 SYSTEM overlay 造成的 effective 几何变化按当前 effective bounds 裁剪重放，不伪造像素。

## 3. 应用页面骨架

BerryUI SDK 提供：

```text
NavigationIconSource
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
5. Body 高度按 `viewport - safe_top - app_bar - safe_bottom` 计算；禁止各 app 手抄 legacy 标题高度。应用可为滚动内容另加最小 48dp 底部留白，但必须显式标为内容 padding，不得冒充或改写 safe area。
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
4. immersive APP 的显式 WM_DESTROY 与进程死亡都由 wmd 转成 SYSTEM HOME invalidation，launcher 因此恢复 reserve；不得只在进程死亡路径收口。
5. Photos 是产品媒体沉浸面，使用 `open_window_immersive()`；其 app 内仍可用自己的返回顶栏，但不得伪造系统状态栏。
6. 普通app不得因全屏而获得 present、HDI、SYSTEM session 或直接改 SYSTEM 窗能力。

## 5. 列表与按压反馈

- Contacts 与 Files 的可选行使用 BerryUI `ListItem` 卡片，不再用文本按钮冒充列表卡。
- Settings 的设置行保存 `ListItem` 句柄并参与消息分发，不得建完即丢。
- `ListItem::on_msg` 消费 Press、Release、Cancel、Click 与 Focus；Click 返回激活结果。
- 应用必须把 BerryUI 产生的每条消息先分发给相关控件，再只对 `Msg::Click` 执行业务动作。只过滤 Click 的写法会丢弃按压反馈，禁止新增。
- 产生 damage 的 Press/Release 帧按停帧合同渲染；无 damage 不提交。

## 6. 验收

必须同批通过：

```text
just check
产品 Recents/frame-cache e2e
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
