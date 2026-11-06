# GUI-06 低功耗渲染策略全景——「又要低功耗又要优秀」的实现路径 + 「别人拿系统做手表」的架构缝

> 调研日期：2026-10-28。方法：联网检索（WebSearch/WebFetch，每条论断带 URL）+ 本地源码/文档取证（BerryOS 仓 + OpenHarmony 浅克隆 `~/Documents/harmony/openharmony/`）。确定度标注：**高** = 官方文档/一手源码/论文实证；**中** = 二手转述或社区实测；**低** = 推断或单一来源。
> 立场声明：本报告给证据与取舍。「低功耗」与「优秀（高刷流畅）」在物理上是一对矛盾，全部现役方案的本质都是**同一根杠杆——让系统在「有变化」时全速、在「无变化」时归零**，差别只在杠杆装在哪一层（面板/链路/合成器/帧循环/调度器/频率）。BerryOS 的机会是 day-one 就把这根杠杆的接口缝留对。

---

## §1 静止即零功耗：PSR、部分更新、GPU idle gating、停帧语义

### 1.1 PSR（Panel Self Refresh）原理与使用条件【确定度：高】

PSR 是 VESA eDP（嵌入式 DisplayPort）标准特性：面板端 TCON（时序控制器）自带帧缓冲，保存最后一帧后，**源端（SoC/GPU）可以关断 eDP 主链路**（PHY 与相关时钟下电），面板自己从本地内存重复刷新——屏幕静止时整条「显示传输」链路功耗归零。内容变化时源端经 AUX 通道唤醒链路，发送**整帧或部分帧**更新（[chenxiaolong/edp-psr-hack](https://github.com/chenxiaolong/edp-psr-hack)、[Intel 社区](https://community.intel.com/t/panel-self-refresh/131486/5)）。

- **使用条件**：eDP 链路 + 面板/源端驱动双方支持（PSR1 = 整帧自刷新；PSR2 = selective update，只传脏区）。Linux i915 下 PSR2 走「selective fetch」按 damage 区域取帧（[freedesktop 补丁：Repeat Selective Update area alignment](https://lists.freedesktop.org/archives/intel-xe/2026-February/116174.html)、[Red Hat Bugzilla #2467676](https://bugzilla.redhat.com/show_bug.cgi?id=2467676)）。
- **实战教训（BerryOS 要绕的坑）**：PSR 在田野里以「闪烁/撕裂/与 VRR 互斥」著称——Lenovo 官方甚至建议关 PSR 修闪烁（[Lenovo 支持文档](https://support.lenovo.com/bs/en/solutions/ht513743-disable-panel-self-refresh-to-avoid-screen-flickering-thinkpad-x1-carbon-10th-gen-thinkpad-x1-yoga-7th-gen)）；Red Hat 报告 PSR 与 VRR 组合会花屏（[Bugzilla #2438442](https://bugzilla.redhat.com/show_bug.cgi?id=2438442)）。后继者 **Panel Replay** 把自刷新与 VRR 兼容性一并解决，且可免整 modeset 切换（[Phoronix：Intel Linux 6.15](https://www.phoronix.com/news/Intel-Linux-6.15-Graphics-Start)、[intel-xe 邮件列表 2025-01](https://lists.freedesktop.org/archives/intel-xe/2025-January/065357.html)）。
- **架构启示**：PSR 之所以坑多，是因为它把「静止检测」放在**链路层隐式**做——软件以为自己在持续送帧，链路层偷偷进出自刷新态，两边对 damage 的认知不同步就闪。**damage 显式化 + 「无 damage 不送帧」的软件纪律，是让 PSR 类机制可靠的前提**（同构于 OpenHarmony 合成器把 damage 显式传给 HDI，见 §1.4）。

### 1.2 部分更新（partial update）

部分更新在三层各有形态，层层数据结构都是**矩形**：

1. **链路/面板层**：PSR2 selective update 按 damage 矩形对齐传部分帧（上节）。
2. **嵌入式面板指令层**：SPI/MIPI 小面板（手表/穿戴主流形态）的 set_column/set_page 地址窗 + 部分写——更新 10% 屏幕就只传 10% 的字节，传输能耗与脏区面积成正比。
3. **软件合成层**：Android SurfaceFlinger 只在有 damage 时重合成；OpenHarmony 合成器 HDI 契约里有显式 `SetScreenClientDamage(screenId, damageRects)` 接口（本地源码 `graphic_graphic_2d/rosen/modules/composer/hdi_backend/include/hdi_device_impl.h:54`）；ui_lite 有完整脏区代数——`AddInvalidateRect` / `DivideInvalidateRect`（被遮挡区域切分）/ `OptimizeInvalidateRects`（合并优化）（本地源码 `arkui_ui_lite/frameworks/components/root_view.cpp:79,308,427,449`）。GUI 里程碑规划附录 B 已把「脏区刷新」评为「lite 栈最值钱的一件」（`docs/GUI里程碑规划-2026-10-v1.0.md` §5 风险表）。

### 1.3 GPU idle gating【确定度：中（概念高、量化数据缺）】

移动 GPU 在无提交工作时由驱动做时钟门控/电源塌缩（power collapse），这是 Adreno/Mali 驱动的常规行为，但公开量化数据少（检索到的多为桌面 GPU 降频讨论，如 [NVIDIA 开发者论坛](https://forums.developer.nvidia.com/t/power-consumption-management-power-downwhen-inactive/6014)）。关键软件侧事实：**「无提交」是前提**——只要帧循环空转每帧提交空命令缓冲，GPU 就醒着。Android ADPF 文档佐证了频率侧：governor 空闲后降频、工作结束后「快速落频」是 DVFS 与 hint 系统的协作行为（[source.android.com Performance Hint API](https://source.android.com/docs/core/perf/performance-hint-api)，见 §4）。对 BerryOS 现阶段（无 GPU、软渲染）该条的等价物是：**渲染线程静止时不烧 CPU**（§1.4）。

### 1.4 「无 damage 不渲染」的帧循环停帧语义——现役系统怎么设计

三家实证形态，共同点 = **帧循环是被「需求」驱动的，不是自由跑的定时器**：

| 系统 | 停帧机制 | 证据 |
|---|---|---|
| Android | Choreographer 只在有 invalidate/动画/输入排队时才 scheduleVsync；无脏活时 app 不收 VSYNC 回调、不重绘；错过 deadline 显示端就重显旧缓冲（无 damage = 旧帧照播） | [Choreographer 源码分析](https://bboylin.github.io/2019/03/15/Choreographer)、[juejin: Android Choreographer 工作原理](https://juejin.cn/post/7275943600780460073)、[dev.to: Android UI Jank Explained](https://dev.to/raulsmithus/android-ui-jank-explained-cpu-scheduling-vsync-deadlines-and-thread-starvation-4io8) |
| OpenHarmony | 软 vsync generator（16ms 兜底）+ HGM 动画空闲判定：`DEFAULT_ANIMATION_IDLE_DURATION = 2000`（ms，动画静止 2s 后按空闲处理降帧）、抬腕/触控后 LTPO 首周期 `1000ms` | 本地源码 `hyper_graphic_manager/core/frame_rate_manager/hgm_energy_consumption_policy.cpp:40-43`、`hgm_frame_rate_manager.cpp:249-259` |
| LVGL（嵌入式） | `lv_timer_handler()` 返回「距下一个到期定时器的毫秒数」，主循环据此 `sleep()`——无动画无输入时整毫秒级睡死（tickless 友好）；社区在 #109 讨论过更完整的 idle/shutdown 控制 | [LVGL GitHub issue #109](https://github.com/littlevgl/lvgl/issues/109)、[LVGL 论坛（单色/部分刷新实践）](https://forum.lvgl.io/t/how-to-configure-lvgl-9-for-monochrome-displays/16944/7) |

**停帧语义的设计要点归纳**（BerryOS 直接可抄的形状）：

1. **帧循环三态**：RUN（有 damage 或活跃动画 → vsync 驱动）/ SETTLE（尾帧已提交，等下一个事件）/ IDLE（脏区空 + 动画表空 → **撤销 vsync 定时器**，睡在输入/damage 事件上）。
2. **不欠拍**：从 IDLE 被唤醒的第一个事件（输入或新 damage）应立即渲一步，不等下一个 vsync 周期（否则点按延迟 = 一个帧周期，这正是安卓教训的镜像，见 §2.2 touch boost）。
3. **画面不变量**：「上屏内容 = 已提交的最后一帧」永远成立；维持画面是显示端的事（面板自刷新 / ramfb 的 DRAM 恒扫 / 嵌入式面板显存驻留），软件侧 IDLE 期间零周期活动。
4. **周期活动 = 能耗**：这条与 BerryOS tickless day-one 铁律同构（`docs/调度器设计-v1.0.md` §4「空闲不白醒」）——软 vsync 定时器 idle 时还开着，等价于把 tickless 架构在 GUI 层重新戳了个洞。

---

## §2 自适应刷新率：LTPO、Android 投票、iOS ProMotion、静态降帧

### 2.1 LTPO 硬件机制【确定度：高】

LTPO（低温多晶氧化物）背板混合 LTPS 与氧化物（IGZO）晶体管，让面板无需大量外设控制器即可**宽范围变频**：旗舰手机 1–120Hz，Apple Watch 1–60Hz。三星 LTPO 面板支持 120/60/30/…/1Hz 离散档（[OLED-A 行业组织](http://www.oled-a.org/ltpo-phone-screen-descriptions-how-to-extend-phone-battery-life-etc_070421.html)、[androidayuda LTPO 解释](https://en.androidayuda.com/LTPO-screens%3A-the-secret-to-variable-refresh-rates-in-mobile-phones)）。田野注意点：LTPO 低刷档与**亮度**耦合（低亮度下变频策略更保守/不可达，[Reddit S21Ultra 讨论](https://www.reddit.com/r/S21Ultra/comments/ozvwj3/about_ltpo_display_is_it_even_working_in_low)）——刷新率策略不是纯软件问题，要读面板约束。Apple Watch Series 5 起用 LTPO 做 AOD 1Hz（抬腕升满 60Hz，[TechAdvisor](https://www.techadvisor.com/article/734953/apple-watch-series-5-has-an-always-on-display.html)）。

### 2.2 Android：setFrameRate 投票 + ARR + per-View 投票【确定度：高（官方文档全文取回）】

**API 语义**（[developer.android.com: Frame rate](https://developer.android.com/media/optimize/performance/frame-rate)）：

- `Surface.setFrameRate(fps, compatibility[, strategy])` 是**投票不是命令**：「调度器综合考虑多种因素，不保证给你请求的帧率」。
- **票是粘性的**：不要按平台实际行为改票面；内容帧率变了才改（视频 29.97 就投 29.97）。
- **别频繁调**：每次调用都可能触发刷新率切换，切换瞬间可能掉一帧。
- **清票**：surface 停止提交但仍可见时（视频暂停）投 0 清除。
- **多 surface 仲裁**：平台设计为正确处理多窗（分屏/画中画）下不同票面；可能切到票面的**公倍数**（A 投 60、B 投 24 → 系统选 120，两者都无 pulldown 抖动）。
- **seamless 分级**：默认只在无感切换时变（`ONLY_IF_SEAMLESS`）；黑屏一秒的重切换要用户设置 + 应用 `CHANGE_FRAME_RATE_ALWAYS` 双授权（长电影才建议）。
- 省电模式可压低刷新率，应用必须容忍不生效。

**Android 15 ARR（Adaptive Refresh Rate）**（[developer.android.com: adaptive-refresh-rate](https://developer.android.com/develop/ui/views/animations/adaptive-refresh-rate)，页面 2026-10-23 更新）：

- 平台级帧率管理优化：**小动画（进度条/音频可视化）不再强制拉满屏刷新率**；显示刷新率动态跟随内容渲染率且无 jank。
- 叠加在 per-View 投票之上：每个 View 在绘制/更新时投票（类别 DEFAULT/NO_PREFERENCE/NORMAL(≈60)/HIGH 或具体 Hz），**只有需要重绘的 View 投票**；**最高票胜出**；倍数关系取高（30+90 → 90）；非倍数归类（>60 记 High、≤60 记 Normal）；SystemUI 组件也贡献 hint。
- **touch boost**：`ACTION_DOWN` 即 boost 到 High 并维持一段（`isFrameRateBoostOnTouchEnabled` 可关）——**安卓对「响应慢」教训的直接回答**：交互优先拉满，静止再降。
- **fling 速度跟踪**：手指抬起后刷新率随惯性滚动速度**渐降**；自定义滚动组件每帧上报 `frameContentVelocity`（每帧重置）。
- Android 12 起还有 `setFrameRate` 的前代实践（[android-developers 博客 2020](https://android-developers.googleblog.com/2020/04/high-refresh-rate-rendering-on-android.html)）。

### 2.3 iOS ProMotion 帧率决策【确定度：中高】

- 官方口径：iPhone 支持 10–120Hz、iPad Pro 24–120Hz 动态切换（[Apple: Optimizing for ProMotion](https://developer.apple.com/documentation/quartzcore/optimizing-iphone-and-ipad-apps-to-support-promotion-displays)）。
- API 形态：iOS 15 起 `CADisplayLink.preferredFrameRateRange = CAFrameRateRange(minimum:maximum:preferred:)`——**应用投的是一个区间**（min 是保底、preferred 是期望），系统在区间内按触控/内容运动/低电量/热态定实际值；替代旧 `preferredFramesPerSecond`（[Apple 韩文版同页](https://developer.apple.com/kr/documentation/quartzcore/optimizing-iphone-and-ipad-apps-to-support-promotion-displays)——该页正文直取失败，CAFrameRateRange 三字段经检索交叉确认；区间内系统定值的细节属 WWDC 口径，标中确定度）。
- 田野观察：静态内容降到 10Hz；滚动/动画/游戏满 120；录屏锁 120（视频恒帧率需求）；辅助功能「限制帧率」钉 60（[AppleInsider 论坛](https://forums.appleinsider.com/discussion/241883/how-promotion-works-and-why-it-doesn-039-t-matter-on-the-iphone-17)、[Reddit r/iphone](https://www.reddit.com/r/iphone/comments/10skjxm/promotion_display_of_iphone_13_propro_max_isnt)、[MacRumors 论坛](https://forums.macrumors.com/threads/promotion-not-being-used-much-on-iphone-14-pro-max.2374740)）。
- 有反方声音：ProMotion 大部分时间不在 120Hz，用户感知与 60Hz 差异有限（[Andrew Zuo 博客](https://andrewzuo.com/the-iphone-pros-120hz-promotion-is-fake-8eea3b0b0297)）——**如实并列**：这既是批评也是佐证（系统确实在静态时深度降频，正是省电设计起作用的形态）。

### 2.4 OpenHarmony HGM 投票器实证【确定度：高（一手源码，本地克隆）】

OpenHarmony 的 HGM（Hyper Graphic Manager）是「投票制变帧率」的完整工程实现，值得当 BerryOS 的结构参照（Apache-2.0，只看思路）：

- **VoteInfo = {voterName, min Hz, max Hz, pid, bundleName, extInfo}**——**票面是区间不是单值**（`hyper_graphic_manager/core/utils/hgm_voter.h:27-49`，`OLED_NULL_HZ` 表示未投票）；`HgmFrameVoter` 维护 voter 名单与投票记录，按优先级合并区间（`MergeRangeByPriority`）。
- **多个具名 voter**：视频帧率投票（按 buffer 时戳推视频帧率，`frame_rate_vote/rs_frame_rate_vote.h`）、动画、游戏、触控、LTPO idle 等；`HgmEnergyConsumptionPolicy` 管节能策略（动画 idle 2000ms、触控后 LTPO 首周期 1000ms 等常量）。
- **LTPO/LTPS 分型**：屏幕策略按 LTPO/LTPS 分表（`hgm_frame_rate_manager.cpp:132,140`「LTPO-DEFAULT」），LTPO 屏开 `VSYNC_MODE_LTPO`；触控抬起后有「LTPO 优先」的动态模式窗口（`TOUCH_EXT_ENABLED_LTPO_FIRST`）。
- BerryOS GUI 规划当前把 HGM 评为「低——QEMU 单帧率环境无消费面」（`docs/GUI里程碑规划-2026-10-v1.0.md` 附录 B L0）——**对 v1 实现成立，对协议面不成立**（§7 论证「砍实现不砍协议位」）。

### 2.5 静态内容降帧实践汇总

静态降帧的通用形态：**交互 boost（触摸即时拉满）→ 内容速度衰减（fling 渐降）→ 静止 timeout（OHOS 2s / Apple 即时到 10Hz）→ 极限档（AOD 1Hz）**。Android 文档明确「小动画不拉满」是 Android 15 才修好的平台级教训——此前的行为是任何小动画把整屏顶到最高刷。

---

## §3 帧率×功耗数据

### 3.1 刷新率对比【确定度：中（社区实测为主，无单一权威数字）】

- 120Hz 比 60Hz 整机多耗电约 **10–30%**（依设备而异），约折损 1–2 小时亮屏续航（[Pai International 刷新率指南](https://www.paiinternational.in/blog-details/refresh-rate-explained-what-does-60hz-90hz-or-120h?srsltid=AYCX9vLTSPJtaOKBW4TIN-dcGo9l3LqZnCfIVu965iNxGjGyAA-TWq1)、[Samsung 社区 120Hz vs 60Hz 掉电实测](https://r2.community.samsung.com/t5/Tech-Talk/120-Hz-vs-60-Hz-battery-drain-test/td-p/13905246)、[Honor 官方博客](https://www.honor.com/my/blog/what-is-refresh-rate-on-phone/?srsltid=AYCX9vIOExm2Ix_zjV3695A1XY8oeLO-7LjXPX3Z8XR0RgkNi3eUPpa)、[Reddit r/pixel_phones](https://www.reddit.com/r/pixel_phones/comments/1fubxgf/60_hz_vs_120hz_battery_life)）。
- LTPO 自适应基本抹平静态场景差距（§2.1）——省的是「静止时按高刷付费」的冤枉钱。
- OLED 还有**像素级**杠杆：点亮像素比例（OPR）直接决定面板功耗（§6.2 的 15% AOD 预算）。

### 3.2 渲染管线各环节功耗占比【确定度：中（论文场景与手机日常有偏差，如实标注）】

- **显示子系统在视频流场景占总功耗 38–68%**（Xiao et al., ISLPED'15《Reducing Display Power Consumption for Real-Time Video》，[PDF](https://yaoliu-yl.github.io/publications/islped15-display.pdf)）——含面板/背光/传输，是「显示是第一耗电大户」的定量锚点。
- **帧缓冲 DDR 流量是带宽大头**：每帧既被 GPU/软渲染写、又被显示控制器逐帧读回——这是 AFBC 类压缩的动机（[HPG 2015: Framebuffer Compression Using Dynamic Color Palettes](https://www.highperformancegraphics.org/wp-content/uploads/2015/PosterAbstracts/framebuffer-compression-dynamic.pdf)）。
- **GPU/CPU 能耗模型**：手机 GPU 功耗可用渲染复杂度建模（[Koskela et al.](https://www.researchgate.net/profile/Timo-Koskela/publication/261206893_Power_Consumption_Model_of_a_Mobile_GPU_Based_on_Rendering_Complexity/links/5742bcfe08ae298602ee4fa9/Power-Consumption-Model-of-a-Mobile-GPU-Based-on-Rendering-Complexity.pdf)）；GPU 与 CPU 能耗对比见 [IWOCL'14](https://www.ida.liu.se/labs/eslab/publications/pap/db/elear_IWOCL14.pdf)；逐帧能耗测量方法见 [JCGT 2014](https://jcgt.org/published/0003/01/03/paper.pdf)。
- **BerryOS 现实校准**：QEMU 无能耗面——TCG 下唯一确定性代理 = **指令数**（B-2 icount 通道在册，`just bench-icount`）与 **唤醒次数**（tickless 语境的 wake 计数）。GUI 帧循环的「省电验收」在模拟器时代应定义为：IDLE 态每秒指令数增量 ≈ 0、定时器唤醒 0（详见 §7.6）。

### 3.3 把数字翻译成设计律

1. 显示是第一大户（38–68%）→ **「不动就别画」是省电第一杠杆**，优先级高于任何微优化。
2. 帧率档间差距是「乘法级」（10–30%）→ 帧率协商接口值得 day-one 留位。
3. 带宽是第二大项 → 像素格式/压缩/分辨率杠杆在软渲染时代就存在（§5）。

---

## §4 GPU DVFS 与渲染联动：帧时间反馈调频

### 4.1 ADPF（Android Dynamic Performance Framework）实证【确定度：高（官方文档全文取回）】

Performance Hint API（[source.android.com](https://source.android.com/docs/core/perf/performance-hint-api)）是「渲染 deadline 驱动调频」的现役标准答案：

- **会话**：`PerformanceHintManager.createHintSession(tids[], targetDurationNanos)`——绑定线程组 + **目标工作时长**（= 帧周期预算，如 120Hz 就是 8.33ms）。
- **反馈**：每帧完成时 `reportActualWorkDuration(actualNanos)`——**目标/实际双值闭环**，系统据此比纯负载 governor 更快升频、完工后更快落频。
- **为什么需要**：常规 governor 升/降频要 ~200ms，而一帧只有 8–16ms——没有 hint 时掉帧后 governor 来不及救。
- **目标可变**：帧率切换时 `updateTargetWorkDuration()`（不必每帧调）。
- **超越 DVFS**：hint 还能影响大小核放置；官方反模式警告：busy-loop 伪造需求 = 浪费电+推高热负载。
- 与热管理联动：thermal headroom API 让应用预知可持续性能余量（[PowerManager.OnThermalHeadroomChangedListener](https://developer.android.com/reference/android/os/PowerManager.OnThermalHeadroomChangedListener)）。

### 4.2 与 BerryOS 既有机制的映射

BerryOS 的对应件**全部已在册**，缺的只是 GUI 侧的消费者：

| ADPF 概念 | BerryOS 既有对应 | 出处 |
|---|---|---|
| target duration + actual 反馈 | PM 服务共享环形缓冲（利用率事件/设频请求，禁通用 IPC 拷贝） | `docs/调度器设计-v1.0.md` §5.1 |
| 大小核/频率干涉面 | AI 调度：内核留第一类接口，用户态策略服务经共享环干涉放置/频率/优先级（per-cluster 频率 hint 由 PM 裁决执行） | `docs/AI调度设计-v1.0.md`、CLAUDE.md 身份支柱 |
| 每 task 能耗倾向 | EPP hint（perf/efficiency）+ intent u8 查表（IntentProfile→latency-nice/EPP/uclamp/timer slack） | `docs/调度器设计-v1.0.md` ADR #42 |
| 时间片随频率漂移 | slice 按周期数记账（DVFS 预埋，已落地 2b-4） | `docs/调度器设计-v1.0.md` §3 |
| 唤醒合并省电 | timer slack+policy 三元组（zx_timer_set 式） | `docs/调度器设计-v1.0.md` §3 |

**渲染会话形态建议**（详见 §7.5）：帧循环每帧把 `{target_ns, actual_ns}` 写共享环 → PM 调频；GUI 线程 intent/翻 EPP 档随帧态切换（动画=performance、静止=efficiency）。这是「AI 原生调度的输入/渲染优先级天然是其消费面」的第一次真实兑现。

---

## §5 显示带宽省电：分辨率、像素格式、AFBC/ASTC

### 5.1 分辨率 / DPI / 像素格式

- 帧缓冲字节数 = w×h×bpp，**带宽与像素数线性**：RGB32→RGB565 直接砍半传输字节；降分辨率是平方杠杆。移动 GPU 动态分辨率缩放（DRS）是成熟省电技术（[USENIX HotPower'14: Mobile GPU Power Consumption Reduction via Dynamic Resolution Scaling](https://www.usenix.org/system/files/conference/hotpower14/hotpower14_nixon.pdf)）。
- BerryOS 规划已含此判断：GUI 裁决 D「RGB32 800x600 起步；TCG 吞吐不足时降 RGB565 或 640×480」（`docs/GUI里程碑规划-2026-10-v1.0.md` §3-D）——TCG 吞吐压力与带宽功耗在软渲染时代**同源**（都是搬字节数），一个降载手段同时服务两端。
- fbcon 侧像素格式已做成运行时事实（`PixelFormat::from_raw` 宁缺勿猜，`user/berry-fbcon/src/render.rs:36-38`）——这条纪律要保持到 berry-gfx/berry-ui：**不把任何像素格式/几何烧进协议常量**。

### 5.2 AFBC / ASTC 压缩与在管线中的位置【确定度：高（概念）/ 中（量化）】

- **AFBC（Arm Frame Buffer Compression）**：GPU 与显示控制器间的**无损**图像压缩协议，压缩的是「GPU 写出→DDR→显示控制器读回」这一整段往返流量，省带宽即省电；与 tile-based 移动 GPU 架构配合（[ARM SIGGRAPH 2024: Moving Mobile Graphics GPU 101](https://community.arm.com/cfs-file/__key/communityserver-blogs-components-weblogfiles/00-00-00-20-66/3858.siggraph_5F00_mmg_5F00_2024_5F00_Mobile101.pdf)、[ARM GPU Best Practices: Buffers and textures](https://support.arm.com/documentation/101897/0304/Buffers-and-textures?lang=en)）。
- **ASTC**：纹理**有损**压缩标准，省的是纹理驻留内存与取纹理带宽（[Game Developer: texture compression efficiency](https://www.gamedeveloper.com/programming/why-efficiency-is-key-in-texture-compression-standards-for-mobile-graphics)）。
- **管线位置**：AFBC 在**渲染输出/显示输入两端**（帧缓冲路径），ASTC 在**采样输入端**（纹理路径）——两者互补不替代。
- **对 BerryOS（软渲染、无 GPU）的翻译**：AFBC 的本质是「**同一画面在 SoC 内重复搬运是纯浪费**」。软渲染等价物：(a) 脏区协议=只搬变化的像素（已在 §1.4/§7.1）；(b) **图标/字形用紧凑格式**（GUI 裁决 B 自定义紧凑格式/RLE——同思想）；(c) 未来 GPU 后端接入时把 AFBC 支持列为显示服务↔GPU 驱动的协商项（virtio-gpu 的 host-crafted 路径无 AFBC，真 SoC 才有）。
- 图标格式用 1bpp/4bpp 索引色（AOD 场景）是像素格式的另一个低功耗位（§6.3）。

---

## §6 「手表可落」架构缝：watchOS 预算、Wear OS 教训、LVGL 模式与反面清单

**定位对齐**：不是 BerryOS 做手表，而是「别人拿 BerryOS 做手表时自然满足」。所以本节核心产出 = **「哪些架构决策会让手表落不了」的反面清单**——每一条都是今天（GUI-1..9 规划期）就可能无意中做死的事。

### 6.1 watchOS 渲染预算与降载策略【确定度：中高】

- Apple Watch S5 起 LTPO 1–60Hz；**AOD 恒 1Hz**（表盘秒针「瞬移」而非扫秒是物理约束不是 bug），抬腕即满帧率（[TechAdvisor](https://www.techadvisor.com/article/734953/apple-watch-series-5-has-an-always-on-display.html)、[MacRumors 论坛：Why no always on seconds](https://forums.macrumors.com/threads/why-no-always-on-seconds.2350335/page-2)）。
- AOD 是**内容降载**不只是降帧：暗色简化表盘、隐藏秒针、第三方 app 在 AOD 拿不到实时数据面（[MacRumors/Reddit 观察](https://www.reddit.com/r/AppleWatch/comments/11xoig2/apple_watch_battery_life_analysis_always_on)）。
- 交互模式（抬腕）与 AOD 模式是**两套渲染预算**——系统强制 AOD 档的应用内容约束。

### 6.2 Wear OS 教训【确定度：高（三星官方博客全文取回）】

三星官方优化文（[Optimizing Watch Face Battery Usage by Reducing On-Pixel Ratio](https://developer.samsung.com/sdp/blog/en/2024/05/07/optimizing-watch-face-battery-usage-by-reducing-on-pixel-ratio)）给出硬数字：

- **OPR（On-Pixel Ratio）= AOD 模式下点亮像素占比**；**Google Play 拒收 OPR > 15% 的 AOD 表盘**——15% 是平台级像素预算。
- 实例：隐藏背景图/日期/秒针把 OPR 从 13.4% 降到 2.4%（约 83% 相对降幅）。
- OLED 功耗随点亮像素数线性——白字比暗字贵、暗底是硬省电。
- 系统组件级约束：时钟组件**在 AOD 自动隐藏秒**。
- Google 官方 Wear OS 省电指南同向：AOD 用更暗更简内容、少动（[developer.android.com: Conserve power](https://developer.android.com/training/wearables/apps/power)）。

### 6.3 LVGL 等嵌入式 GUI 低功耗模式【确定度：中高】

- **1bpp 单色档**：`LV_COLOR_DEPTH 1` / I1 格式是手表/单色屏一等公民（[LVGL 论坛：monochrome 配置](https://forum.lvgl.io/t/how-to-configure-lvgl-9-for-monochrome-displays/16944/7)）。
- **单缓冲直写**：嵌入式常态 = 无双缓冲无 flip，脏区直写面板（partial refresh）；已知坑 = 部分刷新残影/垃圾像素（[LVGL 论坛：partial refresh garbage](https://forum.lvgl.io/latest?page=27)）——**脏区代数必须精确**（ui_lite 的遮挡切分就是为这个）。
- **tickless 睡眠**：`lv_timer_handler` 返回距下一定时器毫秒数供主循环睡（[LVGL #109](https://github.com/littlevgl/lvgl/issues/109)）——与 BerryOS 停帧语义（§1.4）同构。
- **熄屏表盘直写**：MIP/memory-in-pixel 类面板 + e-paper 是「静止真零功耗」的极致形态（e-paper 仅翻页耗电，[Touchify: e-paper power](https://touchify.io/en/blog/e-paper-display-power-consumption)）——协议不假设「控制器每帧来读」。

### 6.4 反面清单：会让手表落不了的架构决策（每条 = 今天别做死的事）

| # | 反面决策 | 为什么杀死手表 | 正确留缝 |
|---|---|---|---|
| R1 | 协议/渲染核烧死 RGB32/ARGB8888 | 1bpp 面板 32 倍带宽浪费，MIP 面板无法表达 | 像素格式运行时协商进显示协议版本域（fbcon `from_raw` 纪律上移到协议） |
| R2 | flush 语义只有「整屏」一档 | 部分窗写不存在 → 每次秒针跳动付整屏传输 | flush 恒携 damage 矩形集；FULL 是退化特例 |
| R3 | 帧循环 = 自由跑 vsync 定时器 | 静止时每秒 60/120 次无意义唤醒，电池杀手 | 三态停帧（§1.4）；软 vsync 定时器 IDLE 即撤 |
| R4 | 假设 pageflip/双缓冲/vsync 硬件在位 | 手表面板多数无此概念；ramfb 也没有 | flip 是可选 op 位；单缓冲直写是一等路径 |
| R5 | 每帧全屏 damage（「反正重画简单」） | 手表上 = 每秒全屏面板写 + 全屏读回 | 脏区代数进 berry-ui day-one（GUI-3 已规划，别裁） |
| R6 | 常驻 GPU 上下文/常驻大缓存假设（图标全量解码常驻） | 手表 RAM MB 级；无 GPU | 软渲染一等公民（OHOS RASTER 同判）；图标 LRU 限帽已规划；渲染后端 trait 化 |
| R7 | 帧率写死单值（「系统 60fps」当常量） | AOD 1Hz/交互 60Hz 两套预算表达不了 | 帧率 = 协商结果不是常量；vote 留位（§7.3） |
| R8 | 显示服务与「面板电源/背光」操作耦合死 | 手表 AOD 需模式切换（NORMAL/AOD/OFF） | 显示服务协议留 power/mode 档位 op（§7.4） |
| R9 | 时间显示假设亚秒刷新可用 | AOD 档秒针必须可裁（Wear OS 自动隐秒） | UI 框架「帧率档 → 可见性规则」联动位（内容降载 API） |
| R10 | 用通用 IPC 大消息传帧/区域 | 拷贝即带宽即电；PM 共享环纪律的反面 | 帧数据走共享内存（VMO 授能）+ 小消息传 damage 元数据——微内核天然做对，坚持住 |

**反向检验**（哪些不必留）：手表不需要的是 GPU 多上下文、HWC、多窗合成、高刷——这些砍了不影响「别人拿去落手表」；**会杀死手表的只有「假设」类决策（R1–R4/R7/R8），不是「功能缺位」**。

---

## §7 对 BerryOS：低功耗 day-one 设计项（软渲染阶段就该定死的）

> 原则：**砍实现不砍协议位**。GUI 规划 v1 砍「变帧率/双缓冲/HWC」是对的（`docs/GUI里程碑规划-2026-10-v1.0.md` 附录 B「二」末段）；本节论证的是这些缝的**协议形态**要在 GUI-1..5 定协议时就留好，后补 = 协议版本破裂或全局改写。

### 7.1 damage 协议形态（guifbd ↔ berry-ui/未来合成器）

- **flush 消息恒带矩形集**：`flush { damage: [Rect; K], flags }`，`Rect = {x:u16, y:u16, w:u16, h:u16}`；K 帽 8；`flags::FULL` 表整屏退化。GUI-3 规划的「Invalidate 脏区合并」产出即此消息的输入（ui_lite `OptimizeInvalidateRects` 同构）。
- **合并启发式**定值进协议注释：合并后面积 ≤ Σ面积×(1+1/4) 才并（避免两大远距矩形并成整屏）。
- **矩形不是位图**的理由：PSR2 SU area、SPI/MIPI 地址窗、ui_lite 脏区、SurfaceFlinger surface damage 全是矩形代数——位图/区域列表都是格子里的异类。
- 现役落点：GUI-1 guifbd 段② serve 循环（`user/bins/guifbd/src/main.rs:153-155` 挂注释「console fan-out」）起的协议头就按此定形——console 直写是 0 号消费者，berry-ui flush 是 1 号。

### 7.2 停帧语义（帧循环三态 + 软 vsync 停摆）

- **三态**（§1.4）：RUN/SETTLE/IDLE；IDLE 判据 = 脏区空 ∧ 动画表空 ∧ 无未决输入。
- **IDLE 动作**：撤软 vsync 定时器（BerryOS tickless「空闲不白醒」在 GUI 层的同一条铁律），睡在 damage/event 到达的 channel 上；唤醒首帧**立即渲**（不欠拍——安卓 touch boost 的教训正面）。
- **软 vsync 的频率自适应**：QEMU 无真 vsync（规划 §5 风险表在册），软 vsync 周期应取「帧率协商结果的倒数」而非 16ms 常量——这样 §7.3 的投票器即使单帧率环境也能表达「动画只跑 30fps」的降载。
- **验收指标**（AI 测试友好化）：IDLE 态注入零事件时，遥测/telemetry 里帧循环唤醒计数恒 0、B-2 icount 指令增量 ≈0——可自动化断言的「静止即零功耗」模拟器版。

### 7.3 帧率协商 API 留位（berry-guiproto）

- **票面 = 区间**：`vote { min_hz: u8, max_hz: u8, preferred_hz: u8, scene: enum, sticky: bool }`——对齐 iOS `CAFrameRateRange(min:preferred:max:)` 与 OHOS `VoteInfo{min,max}` 双实证（单值票面是 Android setFrameRate 的简化，区间才是两家收敛的形态）。
- **scene 枚举**：`UI_ANIMATION / VIDEO / GAME / IDLE / AOD`（OHOS voter 名单同构；VIDEO 票要带精确值如 29.97→*10 定点）。
- **仲裁规则 v0 写死在服务端**（可换）：最高票胜出；倍数取高；静态 timeout（2000ms，OHOS 同值）→ 降 IDLE 档；触控事件即 boost（窗口期内顶满）——**规则是策略可换，票面是 ABI 冻结**。
- **输出扇出**：(a) 真面板驱动缝（LTPO 模式表 op，v2+）；(b) PM 服务频率 hint（§7.5）；(c) UI 内容降载规则（§6.4 R9 的 AOD 隐秒联动）。QEMU 单帧率环境 = 三扇出全部 no-op + 遥测记录票面——**接口先活，消费后到**。

### 7.4 熄屏/部分更新缝

- 显示服务协议留 **power/mode op**：`NORMAL / DIM / AOD / OFF`；AOD 档语义三件 = 内容预算（黑底、低 OPR——Wear OS 15% 预算当参照常量）、更新预算（1Hz 级）、格式预算（低 bpp 可选）。
- ramfb 现实：控制器恒扫 DRAM，mode op 对 ramfb 是 no-op——**协议不承诺硬件行为，只承诺软件纪律**（AOD 档下帧循环按 1Hz 醒、秒级 damage）。真面板接入时由显示驱动消费同一 op。
- 部分更新：§7.1 的 damage 矩形已是载体；面板地址窗写法是显示驱动内部实现细节。

### 7.5 与 PM 服务联动面（ADPF 同构）

- **渲染会话**：guifbd/launcher 帧循环每帧向 PM 共享环写 `{target_ns = 1e9/negotiated_hz, actual_ns = CNTVCT 差}`——ADPF `createHintSession + reportActualWorkDuration` 的 BerryOS 版（§4.2 映射表全在册）。
- **intent/EPP 联动**：帧态 RUN → intent/performance 档；IDLE → efficiency；AOD → maintenance 位（`berry_intent` 0x05 编码已冻结在册，恰好是 GUI 第一个真实消费者——ADR #42「机制预留」的兑现样本）。
- **方向单向化**：GUI 只投票不设频（频率由 PM 裁决执行，`docs/调度器设计-v1.0.md` §5.1 同款护栏）；PM 崩 → 帧循环照跑（上次频率保活，内核降级保活在册）。
- **别犯的错**（Android 官方反模式）：不用 busy-loop 伪造负载拉频；touch boost 用事件真值不用轮询。

### 7.6 现役代码的具体落点（对齐 GUI-1..9 刀序）

1. **GUI-1（进行中）**：guifbd serve 循环起步协议就带 damage 字段（console 路恒 FULL）——协议一次定形，避免 GUI-3 再改 ABI。
2. **GUI-3 berry-ui**：三态帧循环 + Invalidate 脏区合并为**验收项**（host 测：脏区合并代数 + IDLE 撤定时器状态机），不是实现细节。规划 §4 GUI-3 行已含「软 vsync 帧循环」，本报告补上「停帧语义」验收面。
3. **GUI-4 berry-guiproto**：vote 消息 + power/mode op 进协议 crate（host 测编解码，无消费者不接线）。
4. **GUI-9 DFX**：加「IDLE 能耗代理」断言（唤醒计数 + icount）——AI 测试友好化支柱的 GUI 展示位。
5. **文档回写**：GUI 规划 §3 裁决表建议补一行「G：低功耗缝 = damage/停帧/vote 留位 day-one（本报告 §7）」；附录 B「v1 明确砍掉变帧率」补注「砍实现不砍协议位」。

---

## 关键发现

1. **「低功耗+优秀」的统一杠杆是需求驱动**：有变化全速、无变化归零——Android（Choreographer 仅 invalidate 时排 vsync）、OpenHarmony（动画 idle 2000ms 降档）、LVGL（timer 返回值供睡）三家独立收敛于同一形状（证据：§1.4 表）。
2. **PSR 是硬件层的同一杠杆**：eDP TCON 自刷新让静止时显示链路归零，但其田野故障（闪烁/与 VRR 冲突）证明 damage 认知必须显式化在软件层（证据：Intel 社区/Lenovo/Bugzilla，§1.1）。
3. **帧率协商的收敛票面是区间不是单值**：iOS `CAFrameRateRange(min:max:preferred:)` 与 OHOS `VoteInfo{min,max}` 双实证；Android 仲裁规则可抄（倍数取高、touch boost、静态 timeout）（证据：Apple 文档/OHOS 源码 `hgm_voter.h`/Android ARR 文档，§2）。
4. **显示是第一耗电大户（视频场景 38–68%），120Hz 恒开多耗 10–30%/1–2h**——「不动就别画」优先级高于一切微优化（证据：ISLPED'15/三星社区实测，§3）。
5. **ADPF 证明 deadline 反馈调频是必需品**：governor 爬升 ~200ms vs 帧预算 8–16ms，target+actual 双值闭环是现役答案；BerryOS 的 PM 共享环/intent/EPP/slice 周期记账全部在册，GUI 只差当第一个消费者（证据：source.android.com，§4）。
6. **手表可落性的本质是「无假设」**：Wear OS 15% OPR 预算、watchOS AOD 1Hz、LVGL 1bpp 单缓冲直写共同定义了低档形态；会杀死移植的只有协议层的烧死假设（像素格式/整屏 flush/自由跑帧循环/单值帧率/无 mode op），不是功能缺位（证据：三星官方/Apple/LVGL，§6）。
7. **AFBC/ASTC 的软渲染时代等价物已在 BerryOS 规划里**：脏区=只搬变化像素、紧凑图标格式=降采样带宽——带宽省电在无 GPU 阶段同样真实（证据：ARM 资料 + HotPower'14，§5）。
8. **本地实证**：BerryOS GUI-1 guifbd（进行中）当前是单缓冲直写 ramfb、无 flip 无 damage——协议窗口正开着，damage/vote/mode 三缝现在定形成本最低（证据：`user/bins/guifbd/src/main.rs`、`docs/GUI里程碑规划-2026-10-v1.0.md`，§7.6）。

## 对 BerryOS 的架构启示

1. **GUI-1 就把 flush 协议定形为「恒带 damage 矩形集（K≤8）+ FULL 退化位」**——矩形是 PSR2/SPI 面板/ui_lite/SurfaceFlinger 的共同收敛形状；FULL-only 协议会把手表与真面板一起锁死。
2. **berry-ui 帧循环做成三态（RUN/SETTLE/IDLE），IDLE 即撤软 vsync 定时器、睡在事件上**——与内核 tickless「空闲不白醒」同一铁律在 GUI 层的延伸；GUI-3 的验收项加「IDLE 唤醒计数=0」状态机测试。
3. **berry-guiproto 预留 vote{min,max,preferred,scene,sticky} + power/mode op（NORMAL/AOD/OFF）**——票面区间对齐 iOS/OHOS 双实证；QEMU 单帧率环境三扇出全 no-op + 遥测，实现砍掉、协议位留全。
4. **帧循环当 PM/AI 调度的第一个真实消费者**：每帧向 PM 共享环报 {target_ns, actual_ns}（ADPF 同构），RUN/IDLE 态翻 intent/EPP 档——兑现「AI 原生调度」与「GUI 输入/渲染优先级天然是消费面」的设计承诺。
5. **像素格式/几何/帧率全部运行时协商，烧进常量 = 移植性死刑**——fbcon `from_raw` 宁缺勿猜纪律上移到 GUI 协议版本域；RGB565/1bpp 留格式枚举位。
6. **仲裁规则写死为「策略可换、票面冻结」**：v0 = 最高票胜出/倍数取高/触控即 boost/静态 2000ms 降 IDLE（OHOS 常量）——规则进化不动 ABI。
7. **验收指标用「能耗代理」而非时间**：IDLE 态唤醒计数=0 + B-2 icount 指令增量≈0，写进 GUI-9 DFX/smoke——QEMU 无能耗面下的确定性「静止即零功耗」断言（AI 测试友好化）。
8. **反面清单入评审**：§6.4 R1–R10 作为 GUI 刀的 checklist 项（尤其 GUI-1/3/4 三把定协议的刀）——越界即打回。
9. **GUI 规划回写两点**：裁决表补「G 低功耗缝 day-one」；附录 B「v1 砍变帧率」补注「砍实现不砍协议位」——避免后续刀把缝当已砍面误裁。

## 开放问题

1. **virtio-gpu 在 QEMU/AAVMF 环境的可用性与 damage/flip 语义**（set_scanout/transfer_to_host_2d 路径是否值得接）——属 GUI-02/GPU 线调研范围，本报告仅按「可选后端 trait 缝」处理，未验证。
2. **ramfb 是否恒扫 DRAM（无自刷新/无关断）**——推断为是（host 侧扫描零 guest 成本），但 QEMU 源码级取证未做；影响「AOD 档在 QEMU 的验证形态」（当前结论：只能验软件纪律不能验硬件收益）。
3. **iOS 帧率决策细节的原始口径**：Apple 文档正文 JS 渲染取回失败，CAFrameRateRange 三字段经检索交叉确认，但「系统在区间内如何权衡（触控/热/低电量权重）」无一手细节——标中确定度，落地 vote 语义时建议再证。
4. **软渲染「全重画到离屏 + damage blit」vs「只渲脏区」的 TCG 成本比**：取决于脏区占比与 memmove/重绘单价，建议 GUI-2/3 刀内用 B-2 icount 通道实测定夺（本报告未做性能实验）。
5. **1Hz AOD 的唤醒源**：真面板场景需要低功耗定时器/RTC 面；BerryOS timer slack+policy 在册但深睡三件套（冻结/预扣/latency QoS）挂 v3——AOD 档与深睡的交互设计留待真机线。
6. **帧率 vote 的安全/公平面**：多 GUI 进程（未来多窗）互投高帧率的 DoS 形态与限额策略——单 GUI 进程阶段不可达，多窗刀（v2+）前需裁。
