# 02 安卓 OEM 定制教训——碎片化的机制根因

> **采集日期**:2026-11-10
> **性质**:研究底稿(证据供立法稿/ADR,不是立法本身;开放问题原样列出,不替拍板)
> **消费面**:厂商定制化深研系列 02 号底稿;供「BerryOS 厂商定制面立法」(壳/驱动/系统应用的分界与兼容承诺)、API 治理章程修订、兼容性测试体系立项引用
> **任务**:把「安卓定制改得面目全非、兼容性差」这个判断解剖成机制链,逐环节给证据,验证/修正之;并逐项评估 Google 补救是架构修复还是治理补丁
> **方法**:外部一手优先(source.android.com / AOSP 官方镜像 / developer.android.com),数值与版本号尽量两源交叉,单源处标「待核实」;仓库内引用给相对路径;事实与推断分开,推断处标「推断」
> **溯源要求**:引用本稿请以下文「来源索引」为准

---

## 0. 结论摘要(先给链,再给证据)

用户判断「改得面目全非、兼容性差」在机制上成立,但需要一句修正:**安卓 OEM 定制的病,不是「OEM 乱改」,而是「官方交付形态只给了改这一条路,且没有为改动设过任何稳定边界」**。机制链:

```
交付形态:fork 整个 AOSP(单棵源码树,无模块化交付)
  + 分层缺位:framework 内部无稳定 ABI(应用面 SDK/NDK 稳定,平台面零承诺)
  + 换肤通道缺位:UI 差异化自由是官方设计,但落点=改 framework/SystemUI 源码
  + HAL 接口弱承诺(8.0 前无标准接口,之后 legacy→HIDL→AIDL 三代迁移)
  + 驱动进内核树(pre-GKI 时代多达 50% 内核代码 out-of-tree,官方原话)
      ↓
OEM 每个大版本都要把自己的改动 rebase 到新 AOSP(fork 与上游永久分叉)
      ↓
升级合并成本爆炸(官方"Costs of fragmentation"清单:LTS 合并难/安全补丁回传贵/上游延迟最长 18 个月)
      ↓
旧版本长尾流通 → 版本碎片(2026-10 实测:六个大版本 >5% 并存,最新版 16 个月仅 37.7%)
      ↓
应用兼容性差(同代 API 行为有 CTS 钉住,但代与代之间分布过长)
      ↓
开发者适配成本(minSdk 选择、按版本/SoC/ABI/图形能力分桶测试成常态工程)
      ↓
生态劣化反噬 OEM 与 Google,Google 被迫叠补丁
```

Google 的补救(CTS→GMS→Treble→GKI→Mainline)轨迹清晰:**凡是把「约定俗成」换成「接口 + 测试 + 签名」的(CTS、Treble 分区、GKI KMI、Mainline 接口纪律)是架构修复;凡是靠商业杠杆逼 OEM 就范的(GMS/Play 许可)是治理补丁**——治理补丁有效(升级率确实改善)但被欧盟反垄断认定越界(2018 年 43.4 亿欧元罚款,数字单源待核实),且架构修复全部晚于碎片化发生(Treble 2017、GKI 2021,而碎片化 2008 年就写进了交付形态)。

**对 BerryOS 的一句话结论(供立法稿,非立法)**:碎片化的机制根因 = 「定制点没有自己的接口边界」。BerryOS 已有的四个 day-one 结构(L0 syscall 永冻、.bap 一格式一扇门、驱动=持证应用、壳=应用)恰好各自封死安卓的一个机制入口——详见 §5.3 对照表。

---

## 1. 模式解剖:安卓 OEM 定制为什么必然深改

「必然」的论证方式:逐个列出 OEM 想做差异化的五个落点,证明在安卓的交付形态里**每个落点都通往 platform 源码改动**,没有一条「不碰内核/framework」的浅改通道。

### 1.1 落点一:交付形态 = fork 整个 AOSP

**事实**:
- AOSP 的官方构建要求:checkout 需 250 GB 磁盘 + 构建再 150 GB,内存 64 GB;Google 自己用 72 核 64 GB 机器,全量构建约 40 分钟([source.android.com 硬件要求](https://source.android.com/docs/setup/requirements?hl=en))。OEM 拿到手的不是「内核 + 系统服务 + 应用框架」的可组合交付物,而是**一整棵 OS 源码树**(kernel、native、frameworks/base、预装 app、构建系统一体,repo 多仓管理)。
- 官方兼容 FAQ 明文:「The Android source code is open, so anyone can use it to build any kind of device」——开源的粒度就是整树([compatibility-faq](https://source.android.com/docs/compatibility/compatibility-faq?hl=en))。

**机制推论(推断)**:OEM 的差异化需求(壳、驱动、运营商标配应用、自家商店)没有独立交付物可挂载,唯一挂载点就是源码树本身。fork 整树 → 与上游形成永久分叉 → 每个上游大版本都是一次全树合并。**深改不是 OEM 的恶意,是交付形态的默认输出。**

### 1.2 落点二:framework 无稳定内部 ABI——「应用面稳定 / 平台面零承诺」的两层结构

**事实**(官方一手,两处互补):
1. 应用面(Navigation 层):NDK ABI 是**永冻承诺**——「once something becomes NDK ABI, it must continue to be exposed and behavior must be preserved by the platform for the lifetime of the ABI (for all intents and purposes, forever)」;且平台内部头文件(android-base、libcutils、libnativehelper 等)**不暴露给 NDK**,C++ ABI 明文「not guaranteed to be stable」,还有专门 CTS 测试(LinkerNamespaces)防止非 NDK 库进入应用可链接清单([NDK PlatformApis.md](https://android.googlesource.com/platform/ndk/+/main/docs/PlatformApis.md),AOSP 官方仓库文档)。
2. 平台内部面(vendor-facing):VNDK 文档原话——「**Within** an Android release, newly-built VNDK shared libraries must be ABI-compatible to previously released VNDK libraries… **Between** Android releases, VNDK libraries can be changed and **there are no ABI guarantees**」([vndk/abi-stability](https://source.android.com/docs/core/architecture/vndk/abi-stability?hl=en))。注意:连这个「版本内稳定」都是 Android 9 才用 header ABI checker 建立起来的([同页](https://source.android.com/docs/core/architecture/vndk/abi-stability?hl=en))——**8.0 之前平台内部连版本内的 ABI 承诺都没有**。

**机制推论(推断)**:Google 用 CTS 把「应用可触面」钉死,但把「平台内部面的自由(和随之的脆弱)」留给了 OEM。OEM 只要在 framework 内部接口上做任何定制——调用一个 @hide API、链接一个非 VNDK 系统库、改一处 SystemUI 逻辑——这些代码就骑在一条**每个大版本都会动的地基**上。深改越多,每次 rebase 碎得越多。这是「改得面目全非」与「升级地狱」之间的传动装置。

### 1.3 落点三:换肤 = 改 SystemUI/Framework 源码(差异化自由是官方设计,通道缺位是历史事实)

**事实**:
- 官方兼容 FAQ 明文允许改 UI:「The user interface components shipped with a device (such as home screen, dialer, and color scheme) don't generally have much effect on third-party apps. As such, **device builders are free to customize the user interface**. The CDD restricts the degree… for areas that impact third-party apps.」([compatibility-faq](https://source.android.com/docs/compatibility/compatibility-faq?hl=en))——**UI 定制自由是写进兼容性程序的**,差异化本来就是卖点。
- 但这些 UI 组件的物理位置在 platform 树里:通知栏/快捷设置/最近任务所在的 SystemUI 是 `frameworks/base/packages/SystemUI/`([AOSP 官方 gitiles 路径,可访问](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/packages/SystemUI/))——launcher、设置同理均在 framework/app 树内。
- Google 后来提供的官方换肤通道是 **RRO(Runtime Resource Overlay)**:「a package that changes the **resource values** of a target package at runtime」,且明文「Overlays can't overlay code, so they can't have DEX files」;Android 11+ 进一步要求 overlayable 白名单子集([rros](https://source.android.com/docs/core/architecture/rros?hl=en))。

**机制推论(推断)**:RRO 的存在本身就是补丁的自供状态——它把「换肤」从「改 framework 源码」迁到「改资源值」,恰好反证在 RRO 普及之前 OEM 的换肤路径就是深改 SystemUI/framework。而 RRO 只覆盖**资源可表达**的外观(颜色/尺寸/字符串);布局结构、手势模型、多任务交互、AI 助手入口这类深度差异化,依然只能改码——改码就回到 1.2 的无 ABI 地基上。(RRO 引入的具体版本:官方页未标注,**待核实**。)

### 1.4 落点四:HAL 层弱承诺——接口三代迁移,承诺逐代收紧但仍非全量

**事实**:
- 官方 HAL 总览:「**HALs existed before Android 8. However, Android 8 ensured each HAL had a standard interface**」,pre-8.0 的形态归入 Legacy HALs 归档页;Treble 之后新 HAL 全部 binder 化、注册 service manager([hal](https://source.android.com/docs/core/architecture/hal?hl=en)、[legacy 归档](https://source.android.com/docs/core/architecture/hal/archive?hl=en))。
- 接口语言本身经历迁移:HIDL(Treble 引入)→「As of Android 13, HIDL has been deprecated… you should use AIDL for HALs」([hal](https://source.android.com/docs/core/architecture/hal?hl=en))。即 OEM/SoC 经历了 legacy 共享库 → HIDL → AIDL 三代 HAL 接口迁移,每代都是平台级重构。
- 驱动二进制的形态:pre-Treble 时代 HAL 是「闭源 vendor blob + 耦合特定 Android 版本」交付(开发者页提供各机型 driver binaries 下载的事实佐证:[Driver binaries 页](https://developers.google.com/android/drivers/);blob 与版本耦合的机制表述为推断)。

**机制推论(推断)**:「HAL 层弱承诺」的实质 = 硬件支持与平台版本**绑定**。芯片厂为 Android N 做的 HAL 不承诺能在 Android N+1 上工作 → 老机型的升级要芯片厂重新出 blob → 芯片厂早就不管老 SoC → 升级死。这是「三年不升级」的直接技术链条,也是 Treble 要拆的第一根锁链。

### 1.5 落点五:驱动进内核树——50% out-of-tree 官方实锤

**事实**(GKI 官方文档原话,一手):
- 「Prior to GKI, kernels were custom and based on the Android Common Kernel (ACK), with device-specific changes made by SoC vendors and OEMs. This customization could result in **as much as 50% of kernel code being out-of-tree code** and not from upstream Linux kernels or ACKs. As such, the custom nature of pre-GKI kernels resulted in significant **kernel fragmentation**.」([generic-kernel-image](https://source.android.com/docs/core/architecture/kernel/generic-kernel-image?hl=en))
- 修复方向(GKI)自证问题:Android 12 起内核 ≥5.10 的设备**强制**随 GKI 内核出厂,内核暴露 stable KMI 给厂商模块,「SoC and board support moved out of the core kernel into loadable vendor modules」([同页](https://source.android.com/docs/core/architecture/kernel/generic-kernel-image?hl=en))。

**机制推论(推断)**:每颗 SoC × 每个 OEM = 一棵独立 fork 的 Linux 内核,驱动以 out-of-tree 补丁形式糊在树上。Linux 主线内核本身不提供稳定内核 ABI——Google 后来搞 KMI 冻结,等于承认**内核接口不稳定是碎片化的根源之一**,并把 Linux「Android 化」成一个带稳定 ABI 的微交付物。

### 1.6 五个落点如何互相咬合成「必然深改」

| # | 落点 | 差异化诉求 | 安卓给的唯一通道 | 后果 |
|---|---|---|---|---|
| 1 | 整棵 AOSP 交付 | 出整机就要出货 | fork 源码树 | 永久分叉,rebase 成为常态成本 |
| 2 | framework 无内部 ABI | 系统级功能/体验 | 改 framework 源码 | 改动骑在会动的地基上 |
| 3 | UI 差异化(官方允许) | 品牌/交互识别度 | 改 SystemUI/launcher 源码(RRO 后期只救外观层) | 换肤=改码=进 rebase 车轮 |
| 4 | HAL 弱承诺 | 硬件跑起来 | 版本绑定 blob,后转三代接口迁移 | 硬件支持与平台版本锁死 |
| 5 | 驱动进内核树 | 设备驱动 | out-of-tree 内核补丁(至 50%) | 每机一棵内核 fork |

**推断**:五者不是并列而是相乘——(1) 给了分叉的形态,(2)(3) 决定分叉必然深入 framework,(4)(5) 把分叉蔓延到 HAL 与内核。「必然深改」= 商业差异化诉求 × 通道缺位,不是 OEM 的道德问题。

---

## 2. 因果链:深改 → 升级合并成本 → 版本碎片 → 应用兼容性差 → 开发者成本

### 2.1 升级合并成本爆炸:官方「Costs of fragmentation」清单

**事实**(GKI 官方文档逐条列明,一手,罕见地直白):
1. **安全更新劳动密集**:ASB(Android Security Bulletin)补丁必须回移(backport)到每棵设备内核,「due to kernel fragmentation, it's prohibitively expensive to propagate security fixes to Android devices in the field」;
2. **LTS 合并难**:跟住 LTS 是安全修复最有效的方式——Pixel 数据:「90% of the kernel security issues reported in the ASB had already been fixed for devices that stay up to date」,但设备内核里塞满定制后「it's difficult to just merge the LTS fixes」;
3. **阻碍平台版本升级**:「Android Framework code had to account for the different kernel variations and couldn't rely on new Android features being implemented in the kernel」;
4. **上游回馈阻塞**:从 LTS 上游可用到进设备,**延迟可达 18 个月**。
   以上全部出自 [generic-kernel-image](https://source.android.com/docs/core/architecture/kernel/generic-kernel-image?hl=en)。

**机制注解(推断)**:这份清单说的是内核,但同样的结构在 framework 层成立——OEM 的 framework 改动(fork 整树 + 无 ABI)在每个大版本都重演「backport/合并/验证」。AOSP 全树 400 GB 的体量([requirements](https://source.android.com/docs/setup/requirements?hl=en))决定了这不是轻量合并。

### 2.2 版本碎片:2026 年实测分布 + 官方数据退场

**事实一:现状数据(独立口径)**。StatCounter 全球 Android 版本份额(网页流量口径),2026-10 采样([gs.statcounter.com/android-version-market-share](https://gs.statcounter.com/android-version-market-share)):

| 版本 | 份额 |
|---|---|
| Android 16 | 37.71% |
| Android 15 | 16.34% |
| Android 14 | 11.45% |
| Android 13 | 10.90% |
| Android 12 | 7.11% |
| Android 11 | 6.08% |

- Android 16 最终版发布于 2025-06-10(维基条目 [Android_16](https://en.wikipedia.org/wiki/Android_16);官方 versions/16 页未直接标日期,**日期单源待核实**)。即:发布约 16 个月后,最新大版本仅 37.7% 未过半;**六个大版本同时 >5% 流通**,五年前的 Android 11 仍有 6%。
- 口径警示:StatCounter 是 Web 流量统计,与 Google Play 设备激活口径不同;但下文 iOS 对照采用同站同方法学,横向可比。

**事实二:官方分布数据的退场**。Google 官方 Distribution Dashboard 现存内容只剩 Vulkan / OpenGL ES 分布,API 版本分布表已被移除、改为引导开发者使用 Play Console 内部的 Reach and devices(「Distributions of installs… Data broken out by Android version, RAM, SoC, Vulkan version, OpenGL ES version, screen metrics, and ABI」)([现行 dashboards 页,数据快照 2025-11-24](https://developer.android.com/about/dashboards?hl=en))。2024 年中的历史快照([archive.org 2024-06](http://web.archive.org/web/20240601000000/https://developer.android.com/about/dashboards))同样已无 API 表——即**至迟 2024 年中,官方 API 版本分布就已不再公开**。

**推断**:碎片化的可见度本身在下降——不是碎片消失,而是公共仪表盘消失(数据只进 Play Console,开发者必须有自己的 app 安装数据才能看)。这对「生态是否健康」的公共监督是一种退步,后文 §3.7 再议。

### 2.3 应用兼容性差与开发者适配成本

**事实**:
- Google 官方对「无兼容标准的世界」的描述:「Without a strong compatibility standard, **devices can vary so greatly that developers must design different versions of their apps for different devices**」——这句话出自兼容性程序总览,是 Google 亲笔承认碎片化对开发者的伤害([compat overview](https://source.android.com/docs/compatibility/overview?hl=en))。
- 适配工程的工具化本身就是证据:minSdk/targetSdk 选择、以及 Play Console 按版本/SoC/ABI/图形能力分桶的 Reach and devices([dashboards 现行页](https://developer.android.com/about/dashboards?hl=en))是每个 Android 开发者的常规决策面。
- 渲染栈层面的碎片直接打在应用上:驱动质量参差是 GPU 时代新病(仓库内 `research/GUI-01-安卓渲染响应慢教训.md` §1.6 引 OrganicMaps issue #2946 实例——HWUI 换 Vulkan/Skia 管线后驱动差异外露给应用)。

**机制注解(推断)**:注意精确的因果分工——**同一版本内**的 API 行为由 CTS 钉住(§3.1),应用兼容性差的来源主要是**版本维度的长尾分布**(2.2)+ 硬件/驱动维度的离散(SoC × GPU × RAM)。碎片化不是一个坏体验,是三个正交维度的乘积。

### 2.4 升级率对照 iOS(同口径)

**事实**(StatCounter 同站同方法学,2026-10,[iOS 版本份额](https://gs.statcounter.com/ios-version-market-share)):iOS 26 系(26.5/26.6/26.7)合计 ≈55.1%,iOS 27 系(27/27.0)合计 ≈16.0%。iOS 27 于 2026 年 9 月发布(维基 iOS 条目谱系,**待核实**)——**发布 1–2 个月即 ~16%**,而 Android 16 发布 16 个月为 37.7%(同站)。差距不是「iOS 用户更爱升级」的心理学,而是结构:Apple 一家控制硬件线 + 系统 + 推送通道,升级链上没有 OEM/芯片厂/运营商三层各自的重测成本(**推断**;Apple 官方已不再定期公开 adoption 数据,其 developer support 页现无分布数据——[developer.apple.com/support](https://developer.apple.com/support/),两造对比本身也说明两个生态的数据透明度都在下降)。

### 2.5 反例分支:免 GMS 的 fork 为什么更碎(推断为主)

官方 FAQ 自己划出了生态外围:「anyone can continue to ship obsolete versions of Android, but those devices can't use the Android name and **exist outside of the Android apps ecosystem**」([compatibility-faq](https://source.android.com/docs/compatibility/compatibility-faq?hl=en))。Amazon Fire OS 类无 GMS fork:不受 Play/GMS 许可约束 → 也不受「不给旧版本授权 Play」的升级鞭策 → 版本与技术形态都可任意漂移。**推断**:这说明 GMS 治理补丁虽然被反垄断诟病,但它确实是目前对 fork 漂移唯一有效的速度限制;去掉它(欧盟的诉求之一)而不补架构约束,fork 只会更散。此推断开放给立法稿讨论。

---

## 3. Google 的补救与有效性:逐个评估「架构修复还是治理补丁」

### 3.1 CTS / CDD:兼容性契约的可执行化——机制正确,但盲区在时间维度

**事实**:CTS 是「free, commercial-grade test suite」,开源、集成进日常构建,含自动化(Tradefed)与手动的 CTS-Verifier([cts](https://source.android.com/docs/compatibility/cts?hl=en));CDD 逐大版本成文发布(官网存档自 Android 1.6 至 Android 17,[CDD 目录](https://source.android.com/docs/compatibility/overview?hl=en));兼容性程序本身免费、不强制,但通过 CDD+CTS 是获得 Play/GMS/Android 商标许可的前提([compat overview](https://source.android.com/docs/compatibility/overview?hl=en)、[faq](https://source.android.com/docs/compatibility/compatibility-faq?hl=en))。

**评估**:
- **做得对**:把「兼容」从口头承诺变成可执行测试 + 成文契约,任何人可自测(开源)。这是把治理工具做成公共品,是整个安卓兼容体系的正资产。
- **盲区**:CTS 保证的是「**同一 API 版本上行为一致**」——它测的是空间一致性,**不保证设备会升级到新版本**。碎片化的根源在时间维度(升级链),CTS 对此无杠杆。这就是「为什么 CTS 是对的、碎片化仍然存在」的机制答案。

### 3.2 GMS/Play 许可:纯治理补丁,有效但越界

**事实**:兼容 → 可申请 Play + GMS 授权 + Android 商标([compat overview](https://source.android.com/docs/compatibility/overview?hl=en));对旧版本「Google chooses **not to license** the Google Play client software for use on obsolete versions」——用商业许可强制版本新陈代谢([faq](https://source.android.com/docs/compatibility/compatibility-faq?hl=en));发现兼容问题时「Typically, Google asks you to release updated system images」(同页)。
欧盟委员会 2018 年认定 Google 滥用 Android 主导地位(捆绑搜索/Chrome 预装、反碎片化协议限制 fork),罚款 43.4 亿欧元([EC 官方新闻稿 IP/18/4581](https://ec.europa.eu/commission/presscorner/detail/en/IP_18_4581);**金额为单源待核实**,正文为 JS 渲染本次未能机器核验)。

**评估**:这是唯一作用在「时间维度」上的强杠杆(不给 Play = 旧版本卖不动),也正因此它是最有效也最危险的工具——它的强制力来自市场垄断而非架构正确性,最终被反垄断机构部分否定。**分类:治理补丁,双刃。**

### 3.3 Treble / vendor 分界:对 OEM×芯片厂关系的架构修复,对 Google×OEM 分叉无效

**事实**:system 与 vendor 分区分离,「Code in system and vendor partitions can interact using a stable interface called the vendor interface (VINTF)… you can update a device's operating system and apps **without updating any of the hardware-specific code**」([partitions](https://source.android.com/docs/core/architecture/partitions?hl=en))。VINTF 用设备 manifest × 框架兼容矩阵(FCM)做显式版本协商([vintf](https://source.android.com/docs/core/architecture/vintf?hl=en));VNDK 给 vendor 可链库做版本内 ABI 稳定 + header ABI checker([vndk/abi-stability](https://source.android.com/docs/core/architecture/vndk/abi-stability?hl=en))。
版本注记:Project Treble 随 Android 8.0(2017)引入、当年起新设备强制 vendor interface(维基 [Android Oreo](https://en.wikipedia.org/wiki/Android_Oreo) 条目;与 HAL 页「Android 8 ensured each HAL had a standard interface」互证);但官方 partitions 页现文把分区分离写作「an **Android 11** effort called Project Treble」([partitions](https://source.android.com/docs/core/architecture/partitions?hl=en))——**两处官方口径不一致,疑指 11/12 的后续深化(vendor_boot、GKI 分区),待核实**。分界是逐版加固的:8.0 Treble → 9 VNDK → 10 system_ext/Mainline → 11 vendor_boot → 12 GKI+SSI([partitions](https://source.android.com/docs/core/architecture/partitions?hl=en) 分区清单可见各版本引入顺序)。

**评估**:
- **架构修复成立的部分**:HAL binder 化 + HIDL/AIDL 版本化接口 + VINTF 显式协商——把「芯片厂 blob 绑死平台版本」这根锁链剪断了,OEM 升级 framework 不再需要芯片厂重出 HAL。这是对 §1.4 问题的正解。
- **没修的部分**:**OEM 的 framework 仍然是自己的 fork**。Treble 拆开的是 OEM↔芯片厂,没有拆开 Google↔OEM——OEM 的 SystemUI/framework 深改(§1.2/1.3)在每个大版本照样 rebase(推断;VNDK「跨版本无 ABI 保证」亦为佐证)。AOSP 侧的 GSI(通用系统镜像)与 SSI(共享系统镜像)方向上是「让 framework 可替换」的继续,本次未深研,列为开放问题。

### 3.4 GKI:对「驱动进内核树」的架构修复——承认内核也需要稳定 ABI

**事实**:Android 12 起内核 ≥5.10 强制 GKI;单内核二进制 per 架构 per LTS;stable KMI;目标含「Enable partners to deliver kernel security fixes and bug fixes **without vendor involvement**」([generic-kernel-image](https://source.android.com/docs/core/architecture/kernel/generic-kernel-image?hl=en));KMI 由符号列表与 ABI 监控维护([stable-kmi](https://source.android.com/docs/core/architecture/kernel/stable-kmi?hl=en)、[abi-monitor](https://source.android.com/docs/core/architecture/kernel/abi-monitor?hl=en))。

**评估**:**架构修复,且是最彻底的一个**——它直接消灭 §1.5 的形态(驱动全部出树进模块),把 Linux 内核改造成「带冻结 ABI 的通用交付物」,Google 可独立发内核安全更新。代价(推断):KMI 冻结限制内核自身演进速度,且这是 Google 用平台强制力(12 起必须)推行的,芯片厂付出了接口迁移成本。

### 3.5 Project Mainline:用模块化把「时效」从 OEM 手里收走——架构修复 + 治理集中

**事实**:Android 10 引入;把选定系统组件转成 APEX/APK 模块,**不引入新 API**、只用 CTS 保证的 SDK/System API + 稳定 AIDL 接口,模块间原子安装/回滚;GMS 设备上模块由 **Google 签名**、经 Play 基础设施分发(Play System Updates),AOSP 签名设备为 com.android.*([modular-system](https://source.android.com/docs/core/ota/modular-system?hl=en))。注意边界:「Mainline support for Android 11 and lower is concluded as of Q4 of 2025」([同页](https://source.android.com/docs/core/ota/modular-system?hl=en))——模块化更新也有覆盖年限。

**评估**:这是对碎片化**时间维度**的架构修复(关键安全组件绕过 OEM 的版本链直接更新),「模块不加新 API、只走稳定接口」是教科书级的接口纪律。但注意它同时是**权力的架构化**:签名与分发权集中在 Google,「谁可以更新系统组件」从治理问题变成了管道问题。**分类:架构修复为主,伴生治理集中。**

### 3.6 评估总表

| 补救(年份) | 性质 | 作用的碎片维度 | 直接回应的机制根因(§1) | 局限 |
|---|---|---|---|---|
| CTS + CDD(2008 起,逐版) | 架构修复(契约可执行化) | 空间:同版本行为一致 | —(预防,不治根因) | 对时间维度(升级)零杠杆 |
| GMS/Play 许可 | 治理补丁 | 时间:逼旧版本退场 | §1 全部(绕过而非修复) | 反垄断风险;无 GMS 设备不受治(§2.5) |
| Treble + VINTF + VNDK(2017/8.0 起) | 架构修复 | 空间:HAL 与 framework 解耦 | §1.4 | 不解 Google↔OEM 的 framework 分叉 |
| GKI + stable KMI(2021/12 起) | 架构修复 | 空间+时间:内核统一交付 | §1.5 | KMI 冻结限内核演进;旧设备不在覆盖内 |
| Mainline(2019/10 起) | 架构修复 + 治理集中 | 时间:组件级绕过 OEM 更新链 | §1.1(部分模块化) | 仅覆盖被模块化组件;签名权集中;旧版本掉队 |

### 3.7 为什么碎片化仍然存在

1. **升级的最后一步是 OEM 的商业决策**:Treble/GKI 压低了技术成本,但人手、机型矩阵、老机型销售周期是商业变量(推断;佐证:2026-10 仍有六个大版本 >5% 并存,§2.2)。
2. **架构修复全部晚于碎片化十年量级**:fork 形态 2008 年定型,Treble 2017、GKI 2021。已形成的长尾只能等设备自然退役——碎片化的存量不可逆,这是最重要的教训之一。
3. **公共度量退场**(§2.2 事实二):官方 API 分布数据移入 Play Console,生态健康度的公共监督弱化(推断:监督弱化会进一步降低治理压力)。

---

## 4. 分类提炼:架构问题 / 治理问题 / 做得对

| 类别 | 条目 | 证据锚 |
|---|---|---|
| **架构问题**(设计债) | ① 整树交付无模块化分界(§1.1);② framework 内部无 ABI 分层(§1.2);③ UI 差异化只有「改码」通道,RRO 通道补位晚且不覆盖深度交互(§1.3);④ HAL 接口无版本化承诺(8.0 前)(§1.4);⑤ 驱动进内核树、内核无稳定 ABI(§1.5) | 共同点:**凡承诺缺位处必碎**;每个架构问题后来都被一个 Google 工程(Treble/RRO/HIDL→AIDL/GKI)追认 |
| **治理问题**(权力与利益) | ① GMS/Play 商业杠杆既是升级鞭策也是垄断工具,欧盟 43.4 亿欧元罚款(单源待核实)(§3.2);② 反碎片化协议的历史排他条款(§3.2 EC 案);③ 官方分布数据公共化退场(§2.2/§3.7) | 共同点:**治理能改变速度,改不了形态**;且治理工具有自身的合法性边界 |
| **做得对**(应继承的资产) | ① CTS/CDD:兼容契约的开源可执行化(§3.1);② NDK/SDK 应用面的永冻 ABI 承诺(§1.2);③ Mainline「模块不加新 API、只走稳定接口」纪律(§3.5);④ VINTF 显式版本协商(manifest×matrix)(§3.3);⑤ GKI 用冻结接口消灭整类分叉(§3.4) | 共同点:**把预期钉在接口与测试上,而不是钉在合作关系上** |

**给立法稿的核心提炼(仍是证据层陈述,不拍板)**:安卓十五年补救史显示——架构边界(ABI/分区/接口)决定碎片化的**形态**,治理杠杆(许可/签名)只决定碎片化的**速度**。没有架构边界的治理(纯 GMS)会被反垄断反噬;没有治理配套的架构(纯 Treble)在存量设备上无力。两者时序上也必须 day-one 同时在场——晚了就只能「等设备退役」。

---

## 5. 对照素材:仓库内文档与本课题的接口

### 5.1 GUI-01(渲染教训)与本稿的关系:同一枚硬币的两面

`research/GUI-01-安卓渲染响应慢教训.md` 解剖的是**单版本内**的体验根因(vsync 对齐、主线程剥离、度量体系,「Android 渲染演进的主线是时序对齐 + 剥离主线程 + 可量化」);本稿解剖的是**版本维度**的根因(升级链与碎片化)。两份合读的叠加结论:

- OEM 改渲染栈的直接代价,GUI-01 已给实例:**驱动质量参差是 GPU 时代新病**(其 §1.6 引 OrganicMaps issue #2946;HWUI 转 Vulkan/Skia 后驱动差异外露)——这正是本稿 §1.4/1.5「HAL/驱动碎片」在图形栈上的投影。谷歌被迫以 GKI/Vulkan HAL 收敛驱动面,与 GUI-01 的教训同向:**渲染栈性能的可预测性依赖驱动接口的收敛,而接口收敛在安卓是十五年补课**。
- GUI-01 关键发现 7「同步 IPC 阻塞主线程是 Binder 与 BerryOS channel 共有的病,换微内核不自动免疫」的方法论同样适用于本稿:**换架构不自动免疫碎片化**——微内核 + 能力模型只提供边界,若厂商定制面没有 day-one 的接口立法,同样会碎(推断)。

### 5.2 应用规范 v1.0(Android 对标)已吸收的教训面

`docs/应用规范-v1.0.md` 在**应用治理面**已系统性对标安卓教训,与本稿(系统/厂商面)互补,逐条对位:

| 应用规范条目(相对路径内) | 吸收的安卓教训 | 与本稿 §1 机制根因的对位 |
|---|---|---|
| §1.2 图标「应用供内容、形状归系统」(108×108 + 系统蒙版) | 自适应图标前的换肤混战 | §1.3:UI 差异化的边界从包格式层就划清 |
| §8 通知:档位三档制、渠道由应用创建是反面(「控制权倒挂」) | OEM/应用滥建通知渠道 | §1.3:差异化不得穿透系统服务的接口 |
| §8.3 通知点击「禁中转提权」(Android 12 trampoline 禁令同款) | 十年补课类收紧 | 事后治理 vs day-one 立法 |
| §10.3「枚举应用/服务 = 显式 handle」(Package Visibility 十年收紧课:默认最小化从 day-one 起) | 可见性滥用后补 | 同上 |
| §11 系统应用 = 同格式 `.bap` + 镜像签名白名单,「只加严永不豁免」(「Android priv-app 双轨反面」) | priv-app 双轨制造成的系统应用特权泥潭 | **§1.1 的直接反面**:系统应用不做第二容器,fork 面收窄 |
| §0 总原则 3「声明必须在装载期被强制」(HarmonyOS 端上不校验是反面) | 各系教训汇 | 兼容性靠执法位不靠审核自觉——与 CTS「可执行化」同哲学 |

### 5.3 BerryOS 冻结常量 vs 安卓机制根因:day-one 封口清单(供立法稿)

| 安卓机制根因(§1) | BerryOS 已有结构 | 状态 |
|---|---|---|
| §1.1 整树交付 | `.bap` 分离式信封 + `.sig` 签名(一格式一扇门);FS/网络/驱动全用户态(`AGENTS.md` 冻结常量) | 厂商拿到的交付物形态与安卓不同,但**厂商可 fork 的面有多大仍是开放问题**(§6.1) |
| §1.2 无内部 ABI | syscall v1 共 37 个 C-ABI + L0 syscall 永冻(API 治理 L0–L4 分级,`AGENTS.md`) | 应用面=安卓 NDK 永冻承诺的等价物;**用户态服务间接口的分级与承诺强度待对齐**(§6.2) |
| §1.3 换肤=改码 | 壳/launcher = 应用(应用规范 §0 形态谱);berry-ui 保留模式 + 资源(图标 BYI)系统侧裁切 | 外观层通道存在;**深度交互定制(手势/多任务)的边界未立**(§6.3) |
| §1.4 HAL 弱承诺 | 驱动 = 持证应用(ADR #46,`[hw]` 节,应用规范 §1.1);devmgr 用户态 | 接口形态比 binderized HAL 更激进;**驱动 ABI 承诺分级未立**(§6.4) |
| §1.5 驱动进内核树 | 内核极简五原则(硬件隔离,`AGENTS.md`) | 内核不含 SoC/板级代码,结构性免疫;**启动关键暂留内核的例外面要注意回迁**(§6.5) |
| §3.1 CTS 盲区(时间维度) | smoke/mkapp/appd 四执法位(应用规范 §0 执法纪律) | 自家测试在;**面向 OEM 的兼容认证套件(CTS 等价物)未立项**(§6.6) |
| §3.5 Mainline | 系统应用 = 预装 `.bap` + 系统集成者签名(应用规范 §11) | 形态上组件可独立换发(推断,待验证);**「模块不加新 API」等接口纪律是否入章程待拍板**(§6.7) |

---

## 6. 开放问题(原样列出,不替拍板)

1. **厂商定制的正式分界面**:壳/launcher/主题/默认应用/输入法哪些可换,syscall/IPC/调度机制哪些永不可换——BerryOS 的「Treble 线」画在哪,需 ADR 拍板(§5.3 对位表供输入)。
2. **用户态服务间接口的 ABI 分级**:L0 syscall 永冻已定;launcher/wmd/notifyd/appd 这类系统服务的接口处于 L 几、对 OEM 承诺什么强度,需与 API 治理章程对齐(对标教训:VNDK「版本内稳定、跨版本不保证」是否值得复制)。
3. **深度 UI 定制通道**:RRO 等价物(资源覆盖层/主题包)要不要进 berry-ui v2 预埋位;不提供则 OEM 深改的冲动无处安放,提供则可能重演 RRO 覆盖不了深度交互的安卓局面。
4. **驱动 ABI 的承诺分级**:`[hw]` 节驱动与内核/devmgr 的接口,是否做「KMI 式冻结 + ABI 监控」(参照 GKI/stable-kmi)。
5. **启动关键暂留内核的例外面**:AGENTS.md 允许启动关键组件暂留内核——这部分是否随版本收敛回用户态,否则可能长成 BerryOS 自己的 out-of-tree 角落。
6. **面向 OEM 的兼容认证套件**:CTS/CDD/CTS-V 的 BerryOS 等价物(认证内容、执法位、认证与商标/商店的关系)未立项;治理侧(类似 GMS 的商业杠杆)要不要存在、以什么合法形式存在,涉及商业模式,超出技术立法。
7. **「模块不加新 API」纪律**:Mainline 式组件独立更新若成立,接口纪律需入 API 治理章程。
8. 待核实清单:EC 2018 罚款金额(43.4 亿欧元,单源);Android 16 发布日 2025-06-10(维基单源);RRO 引入版本;Treble 官方 partitions 页「Android 11」措辞与公认 8.0 的口径差异;iOS 27 发布月份;StatCounter(Web 流量)与 Google Play 设备口径的系统性偏差幅度。

---

## 来源索引

### 官方一手(source.android.com / AOSP / developer.android.com / developer.apple.com)

| # | 内容 | URL |
|---|---|---|
| O1 | GKI:50% out-of-tree、Costs of fragmentation 四条、Pixel 90% 数据、18 个月延迟、Android 12 强制 GKI、stable KMI | https://source.android.com/docs/core/architecture/kernel/generic-kernel-image?hl=en |
| O2 | VNDK ABI 稳定性:「版本内必须兼容、跨版本无保证」、Android 9 header ABI checker | https://source.android.com/docs/core/architecture/vndk/abi-stability?hl=en |
| O3 | NDK PlatformApis:NDK ABI 永冻承诺、平台头文件不暴露、C++ ABI 不稳定、LinkerNamespaces CTS 测试 | https://android.googlesource.com/platform/ndk/+/main/docs/PlatformApis.md |
| O4 | HAL 总览:Android 8 起 HAL 标准接口化、binderized、HIDL 于 Android 13 弃用、Legacy HAL 归档 | https://source.android.com/docs/core/architecture/hal?hl=en 、https://source.android.com/docs/core/architecture/hal/archive?hl=en |
| O5 | 分区:system/vendor 分界、VINTF、Treble 措辞(「Android 11 effort」,与公认 8.0 冲突,待核实) | https://source.android.com/docs/core/architecture/partitions?hl=en |
| O6 | VINTF 设计:manifest × 兼容矩阵、Android 8.0 起 runtime API 上报 | https://source.android.com/docs/core/architecture/vintf?hl=en |
| O7 | RRO:只改资源值、overlay 无 DEX、Android 11+ overlayable | https://source.android.com/docs/core/architecture/rros?hl=en |
| O8 | Mainline:Android 10 引入、APEX、不加新 API、Google 签名走 Play、Android 11-支持 2025Q4 结束 | https://source.android.com/docs/core/ota/modular-system?hl=en |
| O9 | 兼容性程序总览:兼容定义、CDD+CTS → Play/GMS/商标资格、开发者适配叙述 | https://source.android.com/docs/compatibility/overview?hl=en |
| O10 | CTS 总览:免费商用级、开源、Tradefed、CTS-V | https://source.android.com/docs/compatibility/cts?hl=en |
| O11 | 兼容性 FAQ:改 UI 自由、不强制兼容、旧版本不给 Play、生态外设备 | https://source.android.com/docs/compatibility/compatibility-faq?hl=en |
| O12 | AOSP 构建硬件要求:250GB checkout + 150GB build、64GB RAM | https://source.android.com/docs/setup/requirements?hl=en |
| O13 | Distribution Dashboard 现行页(仅 Vulkan/GL,API 表已移除,快照 2025-11-24) | https://developer.android.com/about/dashboards?hl=en |
| O14 | Dashboard 2024-06 历史快照(同上,佐证 API 表至迟 2024 年中已移除) | http://web.archive.org/web/20240601000000/https://developer.android.com/about/dashboards |
| O15 | SystemUI 源码位置(frameworks/base/packages/SystemUI) | https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/packages/SystemUI/ |
| O16 | 驱动二进制交付页(各机型 blob 下载,pre-Treble 形态佐证) | https://developers.google.com/android/drivers/ |
| O17 | Apple Developer Support 现行页(已无公开 adoption 数据) | https://developer.apple.com/support/ |
| O18 | GKI KMI 维护与内核 ABI 监控(§3.4 引) | https://source.android.com/docs/core/architecture/kernel/stable-kmi?hl=en 、https://source.android.com/docs/core/architecture/kernel/abi-monitor?hl=en |
| O19 | 欧盟委员会 Android 反垄断新闻稿(2018,43.4 亿欧元罚款,单源待核实) | https://ec.europa.eu/commission/presscorner/detail/en/IP_18_4581 |

### 独立测量与镜像

| # | 内容 | URL |
|---|---|---|
| S1 | StatCounter Android 版本份额(2026-10,Web 流量口径) | https://gs.statcounter.com/android-version-market-share |
| S2 | StatCounter iOS 版本份额(2026-10,同方法学对照) | https://gs.statcounter.com/ios-version-market-share |
| W1 | 维基 Android Oreo:Project Treble 8.0 引入、新设备强制 vendor interface(Treble 版本二源之一) | https://en.wikipedia.org/wiki/Android_Oreo |
| W2 | 维基 Android 16:最终版 2025-06-10(单源待核实) | https://en.wikipedia.org/wiki/Android_16 |
| W3 | 维基 Android 15:source 2024-09-03 | https://en.wikipedia.org/wiki/Android_15 |

### 仓库内(相对路径)

| # | 内容 |
|---|---|
| R1 | `research/GUI-01-安卓渲染响应慢教训.md` —— 渲染教训单版本面;§1.6 驱动碎片实例、关键发现 7 方法论 |
| R2 | `docs/应用规范-v1.0.md` —— 应用治理面对标(§1.2 图标 / §8 通知 / §10.3 可见性 / §11 系统应用) |
| R3 | `AGENTS.md` —— 冻结常量(syscall 37 C-ABI、L0–L4、.bap 信封、驱动用户态、内核五原则) |

---

> **本稿边界**:未覆盖 GSI/SSI 的完整机制、Android One/Pixel 治理史、运营商认证链、CDD 条款逐条分析(均列入开放问题或后续底稿素材);EC 反垄断条款细节、HarmonyOS/ColorOS 厂商管控对照见姊妹底稿(`docs/应用规范-v1.0.md` 溯源所列五路对标)。
