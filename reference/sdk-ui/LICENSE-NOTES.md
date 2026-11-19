# SDK/UI 参考缓存许可与分发边界

本文件只约束 `reference/sdk-ui/`。结论：这里是私密仓的对照学习区，不是第三方代码分发区。默认不分发 `cache/`，公开仓不得引用其中原始文本或资产。

## 1. 四个层级

### L1 — link-only

适用：

```text
Apple Developer Documentation / HIG / SwiftUI / UIKit / AppKit / WWDC
HarmonyOS NEXT developer center
Material 3 SPA
ChromeOS Developer Library / chromeos.dev
Fluent 2 design site
Tizen current docs
```

规则：

```text
只保存 URL 与自写摘要
不镜像页面
不保存截图/资产/示例归档
不用原文冒充 BerryOS 规范
```

### L2 — CC 许可文档

当前包括：

```text
OpenHarmony docs                  CC-BY-4.0
Flutter website content           CC-BY-3.0 + BSD code
Microsoft Learn source repos      CC-BY-4.0 + code terms
WinUI specs                       CC-BY-4.0
GNOME HIG                         CC-BY-SA-4.0
KDE HIG                           CC-BY-SA-4.0
```

规则：

```text
私密 cache 可以 clone
保留 LICENSE / LICENSE-CODE / COPYING / 作者信息
公开引用时必须署名与链接
CC-BY-SA 内容不得复制进 BerryOS 公开文档
代码示例与散文许可不同，必须分开判断
```

### L3 — Apache/BSD/MIT/MPL 宽协议源码

当前包括：

```text
OpenHarmony interface SDK          Apache-2.0
Android/AOSP                      Apache-2.0 或混合 BSD
AndroidX Compose                  Apache-2.0
Material Components/colors        Apache-2.0
Fuchsia                           BSD-3-Clause
Flutter                           BSD-3-Clause
Swift Book/DocC                   Apache-2.0
WinUI/GTK/libadwaita/COSMIC 等   MIT / LGPL / MPL 按仓
```

规则：

```text
参考阅读安全
代码形状借鉴仍需记录出处
实际 vendoring 前必须重读该文件/目录许可证与 NOTICE
LGPL/MPL 有文件级/修改级义务
```

### L4 — GPL 或商用闭源

清单在册但当前未启用：

```text
Plasma Mobile                     GPL-2.0+
Lomiri / Mir / Phosh              GPL-3.0 系
Qt                                LGPL/GPL/commercial tri-license
HarmonyOS NEXT                    proprietary
```

规则：

```text
只看架构、行为、测试思想
不复制代码
不把文档文本并入公开仓
Qt 需要正式许可才能代码级复用
```

## 2. 特殊注意事项

### OpenHarmony docs

仓库 `LICENSE` 是 CC-BY-4.0，不是 Apache-2.0。引用原文时署名 OpenHarmony/开放原子并链接官方文档。

### Fuchsia

根 `LICENSE` 是 BSD-3-Clause，但子目录可能有不同版权与许可。实际借用某个文件前看该目录版权头。

### Android/AOSP

`frameworks/base` 以 Apache-2.0 为主，`NOTICE` 必须保留；`frameworks/native` 有 BSD/Apache 混合文件。不能把整个仓库视为单一许可。

### ChromiumOS docs

仓库没有根 LICENSE 文件，且 2024-Q1 起弃用。只作历史文档参考；现行手册链接到 ChromiumOS Developer Library。

### Flutter website

内容是 CC-BY-3.0，代码样本是 BSD-3。公开摘录前注意 CC 版本较旧。

### GNOME HIG

CC-BY-SA-4.0。私密缓存可以，公开文档不能直接复制；用自己的话重写并链接。

### Apple

Apple 官方文档与 WWDC 内容没有 reuse license。sample code 有独立、更窄的 Apple Sample Code License；不得假设可再分发。

### Microsoft Learn

仓库源是 CC-BY-4.0，样本代码另有 terms。保留 `LICENSE` 与 `LICENSE-CODE`。

## 3. 分发检查

若未来要把任何参考内容带入 BerryOS 代码或公开文档：

1. 记录来源 URL、commit、文件路径。
2. 复制许可证/NOTICE 要求。
3. 区分散文与代码样本许可。
4. GPL/AGPL/proprietary 直接停止。
5. CC-BY-SA 与公开仓文档不兼容，除非整篇文档按同许可发布。
6. 在 ADR 或 research 文档说明借鉴层级：
   ```text
   architecture only / behavior only / code-shape / vendored file
   ```

## 4. 缓存不等于授权

`cache/` 中存在某文件只表示官方源公开提供，不表示 BerryOS 可以再分发。每次实际借用都要回到对应许可证重读一次。
