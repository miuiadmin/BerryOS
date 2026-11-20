# BerryOS SDK/UI 官方参考库

这是 BerryOS 私密仓专用的官方 SDK、UI、图形、输入、设计规范与 API 文档参考库。目录固定为：

```text
reference/sdk-ui/
├── README.md          使用与维护说明
├── LICENSE-NOTES.md   许可/缓存分级
├── sources.toml       官方来源清单与可复现钉版
├── tools/fetch.py     缓存重建/状态/链接检查脚本
└── cache/             本地第三方源码与文档缓存（不入库）
```

`cache/` 被 `.gitignore` 排除。私密仓跟踪的是来源清单、许可证口径、稀疏拉取规则和 commit 钉版，不提交第三方 blobs。这样既能随时重建参考库，也不会把 0.5–5GB 的上游仓库塞进 Git 历史。

## 1. 使用原则

后续做 BerryUI、SDK、窗口、输入、无障碍、设计 token、应用模型或文档系统前，先到这里查官方行为，不允许凭记忆或想象设计 API。

基本纪律：

1. 先查 `sources.toml` 的 `category` 与 `note`。
2. 打开官方 `official` 链接或本地 `cache/<id>`。
3. 读原始实现或原始文档，不用第三方博客替代。
4. 结论写入 BerryOS 自己的 `research/` 或 `docs/`，并注明来源。
5. 代码级借鉴必须确认许可证；GPL/LGPL/商用内容只做语义对照。
6. 不把 Apple/Microsoft/Google/Huawei 站点页面整页镜像进公开仓。

## 2. 快速使用

列出全部来源：

```bash
python3 reference/sdk-ui/tools/fetch.py --list
```

查看本地缓存与钉版：

```bash
python3 reference/sdk-ui/tools/fetch.py --status
```

重建全部已启用缓存：

```bash
python3 reference/sdk-ui/tools/fetch.py --sync
```

重建指定来源：

```bash
python3 reference/sdk-ui/tools/fetch.py --sync openharmony-docs fuchsia flutter
```

检查官方入口链接：

```bash
python3 reference/sdk-ui/tools/fetch.py --check-links
```

脚本只做浅克隆、blobless 克隆、稀疏检出或 Gitiles 子树归档，不做整仓大克隆。

## 3. 当前本地缓存快照

采集日：

```text
2026-11-11
```

当前启用缓存：

```text
19 个来源
逻辑体积约 476 MiB
```

核心组成：

```text
OpenHarmony docs / API .d.ts
Android View/Widget/Input、frameworks/native、Compose
Material Components Android / color utilities
ChromiumOS historical docs
Fuchsia docs + Scenic/input/Carnelian
Flutter UI docs + widgets/rendering/embedder
Swift Book / DocC
WinUI specs / Windows developer docs
GNOME HIG / libadwaita
```

钉版 commit 见 `sources.toml`。当前磁盘可用空间较低时，不要无脑 `--sync` 关闭项；按需拉取。

## 4. BerryOS 对照地图

| BerryOS 主题 | 优先查 |
|---|---|
| ArkTS/API 面、`@since`、权限注解 | `openharmony-interface-sdk-js` |
| ArkUI 布局/焦点/输入/导航文档 | `openharmony-docs` |
| Android View/Window/Input 语义 | `android-frameworks-base` |
| Android 合成器/BufferQueue/Binder | `android-frameworks-native` |
| 声明式重组/CompositionLocal/slot table | `androidx` |
| Material 组件与主题实现 | `material-components-android` |
| 动态色彩/HCT/对比度 | `material-color-utilities` |
| Fuchsia FIDL/component/Scenic/input | `fuchsia` |
| Flutter constraints/focus/gesture/adaptive | `flutter-website`、`flutter` |
| Flutter embedder C API | `flutter` |
| 文档编译与教程结构 | `swift-docc`、`swift-book` |
| WinUI control spec 流程 | `microsoft-ui-xaml-specs` |
| Windows SDK 文档信息架构 | `windows-dev-docs` |
| GNOME 自适应组件/HIG | `libadwaita`、`gnome-hig` |
| Apple HIG/SwiftUI/UIKit | link-only 官方入口 |

## 5. 来源分层

### A. 已本地稀疏缓存

这些来源可离线检索，且保留了许可证文件：

```text
openharmony-docs
openharmony-interface-sdk-js
android-developer-docs
android-source-docs
android-frameworks-base
android-frameworks-native
androidx
material-components-android
material-color-utilities
chromiumos-docs
fuchsia
flutter-website
flutter
swift-book
swift-docc
microsoft-ui-xaml-specs
windows-dev-docs
gnome-hig
libadwaita
```

### B. 清单在册、按需再拉

体积较大或当前不是第一优先：

```text
OpenHarmony ArkUI engine / lite
Chromium src docs
Flutter packages
Dart site-www
Microsoft UI XAML source
WinUI Gallery
GTK
KDE HIG / Kirigami
Qt QML
COSMIC libcosmic
webOS OSE / Enact
EFL
Plasma Mobile
Lomiri
Phosh
```

### C. 只允许链接的官方手册

这些内容没有可再利用许可或无稳定公开源仓，不能镜像：

```text
Apple Developer Documentation
Apple Human Interface Guidelines
SwiftUI / UIKit / AppKit docs
Apple sample code / WWDC
HarmonyOS NEXT developer center
Android design / source.android.com rendered docs
Material 3 SPA
ChromeOS Developer Library / chromeos.dev
Fuchsia rendered docs
Fluent 2 design site
Tizen current docs
```

可以摘录成自己的事实笔记，但必须用自己的话，并保留官方 URL。

## 6. 维护流程

新增来源时：

1. 在 `sources.toml` 加完整条目：
   ```text
   id / vendor / category / title
   official / source
   license
   cache kind
   enabled
   ref / commit
   sparse paths
   note
   ```
2. 若可以缓存，优先：
   ```text
   depth=1
   blob:none
   sparse checkout
   text-only
   ```
3. 图片、视频、字体、测试资产默认不拉。
4. 运行：
   ```bash
   python3 reference/sdk-ui/tools/fetch.py --sync <id>
   python3 reference/sdk-ui/tools/fetch.py --status
   ```
5. 确认许可证文件在缓存内。
6. 更新本 README 的快照数字与来源分层。
7. 提交清单和脚本，不提交 `cache/`。

刷新已有缓存时不要直接改工作树；删除对应 `cache/<id>` 后重新 `--sync`，再更新 `sources.toml` 的 commit。

## 7. 磁盘策略

当前宿主可用空间曾低于 11GiB。禁令：

```text
不整仓 clone chromium/src
不整仓 clone material-design-icons
不整仓 clone flutter/website
不整仓 clone Fuchsia 历史
不 clone GitHub aosp-mirror 旧镜像
不缓存官方站点图片/视频资产
```

Google Googlesource 推荐用：

```text
<repo>/+archive/refs/heads/<branch>/<path>.tar.gz
```

拉子树归档，并在 `SOURCE.txt` 记录 commit。

## 8. 与既有 `reference/` 的关系

本目录只管 SDK/UI/官方手册。内核、调度、IPC 等既有参考仍遵守：

```text
reference/README.md
docs/规则/13-工作流程与协议.md
```

不要把自研代码放进本目录；这里只放外部官方资料与复现工具。
