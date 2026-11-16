# Android 式全面沉浸系统栏与软件 LTPO

Status: implemented

## Problem

用户明确要求：状态栏与底部导航键全面沉浸、模仿 Android；底部三大金刚直接使用 Android 图标而不是 BerryOS 自绘；同时实现 LTPO。旧实现有三个事实源：launcher surface0 烘焙不透明黑色导航带、desktop 树再画状态栏、`AVOID_SYS` 把普通 APP 几何裁掉。因此 APP 内容无法延伸到系统栏下，三键也是原创几何 glyph。VOTE/POWER_MODE 在 vgpud 只回执不仲裁，软 vblank 固定 16.67ms。

## Decision

### Edge-to-edge 系统栏

新增 add-only `WM_CREATE_F_EDGE_TO_EDGE = 1<<1`：

- 仅 APP 可用，SYSTEM 禁用，且不得与 `AVOID_SYS` 同设。
- APP raw/visible 保持全屏，内容延伸到两栏之下。
- 0x67 safe area 不再来自 raw-visible 差，而由 wmd `safe_insets()` 从可见 SYSTEM 栏窗口推导；Product 仍为 top 117 / bottom 108。
- Berry Room `open_window()` 默认使用 edge-to-edge；`open_window_immersive()` 继续 flags=0 并触发两栏隐藏。

launcher 为 status/navigation 两个真实 SYSTEM 窗创建 bar-local ARGB VMO 并提交给 wmd。wmd 缓存 READ 别名；此后每次 APP/full-desktop SUBMIT 完成像素拷贝后，用 straight-alpha 重混合可见栏。全屏 Recents 不是 bar overlay，仍走真实 SYSTEM 窗提交。桌面 surface0 删除重复状态栏与导航烘焙，SYSTEM overlay 成为唯一系统栏像素源。

### AOSP 三大金刚

直接引入 AOSP `android-15.0.0_r1` SystemUI 原始 VectorDrawable：

```text
ic_sysbar_back.xml
ic_sysbar_home.xml
ic_sysbar_recent.xml
```

`tools/gen_android_nav_icons.py` 解析原始 `android:pathData` 并抗锯齿光栅化，不重新定义几何。源 XML 与 AOSP NOTICE 入库；共享 app 顶栏 `NavigationIconSource` 与底部 SYSTEM 白色三键都消费同一光栅源。项目按 Apache-2.0 承载该资产，不宣称 Android 商标归属。

### 软件 LTPO

新增 `berry-guiproto::software_ltpo_hz` 纯函数。vgpud 聚合 VOTE、POWER_MODE 与真实 SUBMIT 活跃度：

```text
交互新鲜期 60Hz
≥250ms 无提交 30Hz
≥3s 无提交 10Hz
sticky idle / AOD 1Hz
DIM 封顶 30Hz
OFF 0Hz
```

QEMU 硬上界仍为 60Hz。真实 SUBMIT 立即升档并重排远期低频 slot；降档在下一拍生效。cadence 变化输出 `VGPULTPO ok hz=... old=... reason=...`。

## Alternatives considered

- 继续把导航烘焙进壁纸：无法让 APP 内容进入栏下，且系统栏像素出现第二事实源，拒绝。
- 让 wmd 每次直接向 launcher 拉取栏像素：引入反向 RPC 与显示服务阻塞面，拒绝。
- 用 `AVOID_SYS=0` 冒充 edge-to-edge：会同时触发 Photos 沉浸隐藏语义，safe inset 也退化为 0，拒绝。
- 自绘“Android 风格”三键：违背用户明确要求，且引入无授权溯源的视觉仿制品，拒绝。
- 宣称硬件 LTPO：QEMU virtio-gpu 无面板 mode-set，只能宣称软件策略与省电纪律，拒绝。

## Consequences

Product e2e recents 现在断言：

- `WMSYSOVERLAY` 两个真实系统栏源。
- 普通 BerryDemo `WMAPPFRAME immersive=0`，即 edge-to-edge 但非沉浸隐藏。
- 桌面状态/导航与 APP 导航的 AOSP 白色 glyph 像素区间。
- 桌面导航背景非纯黑，证明 scrim 透明。
- `VGPULTPO` 真实出现 60、30、1Hz。

Photos explicit immersive、HOME/BACK 恢复、TASK 全屏 Recents 和 frame-cache replay 均保持。LTPO 真实硬件 mode-set、状态栏实时 RTC/电量/网络真值仍后置。

## Related

- [产品系统壳规格](../../../docs/Berry设备平台-AndroidImmersiveShell-v1.3.md)
- [软件 LTPO 规格](../../../docs/BerryDisplay-SoftwareLTPO-P1-v1.0.md)
- [三方共性控件体系](2026-11-15-tri-platform-controls.md)
