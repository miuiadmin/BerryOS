# ui-sdk（图形引擎与应用开发套件）总体设计

Type: spec
Status: active
Updated: 2026-11-18
Version: 2.2.0

一句话说明：`ui-sdk` 定义 BerryOS 面向系统与第三方开发者的完整客户端技术栈（`berry-gfx` v2、`berry-text`、`berry-ui` v2、`imed`、动态 `Vmar` 页堆与 `berry-sdk`）——深度融合 iOS 26/27 液态玻璃流体控件与 OpenHarmony 6.0 单节点双脏更新、安全区背景默认延伸、`LayoutPolicy`、`geometryTransition` 一镜到底及一多双栏架构，彻底终结样板负担与审美割裂。

## 背景

如 [GUI 与 SDK 及启动模拟器全栈深研](../../research/2026-11-18-GUI与SDK及启动模拟器全栈深研.md) 第三节所揭示，现役客户端栈存在五大瓶颈：`berry-gfx` 仅支持 1-bit 透明键与逐像素标量 `put_px`；文字仅限 95 个 ASCII 1-bit 点阵字模且无输入法守护进程；`berry-ui` 原生 41 `ViewKind` 写死 1x 物理像素迫使业务转向 3,114 行 `pattern.rs` 双轨拼装，且受困于 `512` 节点、`64B` 文本、`8` 输入框与 `256KiB` 静态 BSS 堆硬帽；`berry-appshell` 事件循环锁死 2 枚句柄迫使终端等应用重写全套帧循环。依据 [图形合成与 SDK 及模拟器终态架构裁决](../../notes/proposed/2026-11-18-图形合成与SDK及模拟器终态架构裁决.md) 与 [融合 iOS 26/27 液态玻璃与 OpenHarmony 6 光场一镜到底架构裁决](../../notes/proposed/2026-11-18-融合iOS26至27液态玻璃与OpenHarmony6光场一镜到底架构裁决.md)，本规格定义客户端五层 crate 族的内部总体契约；跨模块公共标准与视觉光学参数见 [GUI 合成与 SDK 终态标准](../../../docs/GUI合成与SDK终态标准-v2.0.md) 及 [iOS 与鸿蒙融合液态玻璃及光场交互标准](../../../docs/iOS与鸿蒙融合液态玻璃及光场交互标准-v2.0.md)。

## 范围

- **覆盖**：客户端 SDK 分层拓扑；`berry-gfx` v2 预乘色彩模型、抗锯齿圆角/阴影/渐变原语与 Span+SIMD 光栅执行器；`berry-text` TrueType/OpenType 轮廓解析、LRU 灰度字形图集与 UTF-8/CJK 多行排版；`berry-ui` v2 细粒度信号状态（`Signal` / `Computed`）、单遍 Flex/Grid/`Insets` 约束布局、统一 `vp` 主题系统、开放 `Widget` 扩展接口与 `.bui` TLV 热重载同构、全手势识别与 `VirtualList`；`imed` 输入法服务与组合文本协议；`berry-libc` 动态 `Vmar` 堆分配器；`berry-sdk` 基于 `Port` 的统一异步 `AppLoop`、`#[berry_sdk::app]` 宏、生命周期状态持久化及开发者友好的打包工具链。
- **不覆盖**：跨进程窗口合成与全局手势拦截（归 [compositor 总体设计](../compositor/design.md)）；内核 `Port` / `ProcessArgs v2` / PIE ELF 底层机制（归 [meihe 总体设计](../meihe/design.md) 与 [ipc 总体设计](../ipc/design.md)）。

## 契约

### 一、客户端 Crate 分层拓扑（零逆向耦合）

```text
L4 应用入口与系统集成层：berry-sdk（#[berry_sdk::app]、Port 异步 AppLoop、Swapchain 握手、Lifecycle/Picker/Clipboard 客户端）
        ▲
L3 声明式响应式 UI 引擎：berry-ui v2（Signal<T>、开放 Widget Trait、统一 vp 标准组件库、VirtualList、物理动效与无障碍树）
        ▲
L2 布局与文字排版引擎：  berry-layout（Flex/Grid/Insets/断点求解）  +  berry-text（UTF-8 断行、TTF 光栅、LRU 8-bit 字形图集）
        ▲
L1 2D 渲染抽象与光栅核： berry-gfx v2（RenderPass/CommandBuffer、Premultiplied RGBA8888、Span + NEON/SSE2 抗锯齿光栅器）
```

L1–L3 全部保持 `#![no_std]` + `alloc` 纯内存计算与 100% 宿主机 `cargo test` 可直跑（无任何 `berry_sys` 符号依赖），支持宿主机毫秒级预览与像素回归对拍；系统调用与 IPC 仅存在于最薄的 L4 `berry-sdk` 与 `berry-libc` 适配层。

### 二、`berry-gfx` v2：抗锯齿 2D 矢量与 Span+SIMD 混合核

1. **一等预乘 Alpha 色彩契约**：标准像素格式统一为小端 `Premultiplied BGRA8888 / RGBA8888`（`r, g, b <= a`）。提供无分支整数预乘转换与 Porter-Duff `Src` / `SrcOver` 双模式；不透明图元（`a == 255`）在命令编码期自动静态降级为 `Src` 直写快路径，零混合损耗。
2. **核心渲染命令集（紧凑定长/切片借用编码）**：
   - `FillRect` / `FillSquircle`：淘汰曲率突变的传统正圆弧，全量采用 $G^2$ 连续曲率超椭圆（Continuous Squircle，$n = 4.0 / 4.5$，遵循同心圆角律 $R_{\text{inner}} = \max(R_{\text{outer}} - p, 4\text{pt})$）、支持四角独立半径、描边宽度 `stroke_w`（含 `0.5pt` 微晶高光内描边）及边缘 1px 亚像素解析覆盖率抗锯齿（256 级 Alpha 权重，彻底废除 `TRUE_CORNERS` 开关与阶梯锯齿）；
   - `DrawBoxShadow`：基于误差函数分段多项式近似的解析圆角矩形高斯阴影（`blur_radius`、`spread`、`offset`），单遍直接生成平滑阴影 Alpha 场，无需离屏模糊缓冲；
   - `PaintBrush`：支持纯色 `Solid(Color)`、双点/多色标 `LinearGradient` 与 `RadialGradient`（Q16 定点插值 + 有序抖动防色带）；
   - `DrawGlyphMask`：消费 `berry-text` 产出的 8-bit 灰度覆盖率掩码 `(mask: &[u8], w, h, stride)` 与前景色执行 `SrcOver` 着色混合；
   - `DrawImage`：支持 `Nearest` 与 `Bilinear`（预乘空间双线性插值，彻底消除透明边缘渗黑与 `i64` 逐像素除法）及圆角裁剪采样；
   - `PushClipRRect` / `PopClip` + `PushLayer(opacity)` / `PopLayer`：支持任意嵌套矩形/圆角裁剪栈与子树透明度组合成。
3. **Span 化与 SIMD 执行铁律**：**严禁任何填充或混合循环逐像素调用带边界分支的 `put_px`**。所有命令在入口一次性求交当前裁剪矩形 `vis = geom ∩ clip` 后，将每一行拆分为「左圆角抗锯齿边缘短段 + 中间连续不透明/半透明长 Span + 右圆角抗锯齿边缘短段」：中间不透明 Span 直接调用 `slice::fill(px_u32)`（编译器自动向量化为 `stp`/`str q` 128-bit 写），半透明与字形掩码 Span 在 `aarch64` 下走 NEON `uint8x16_t`、`x86_64` 下走 SSE2 四像素并行混合内联核（标量纯 Rust 等价实现作跨平台基准与 Miri 对拍）。

### 三、`berry-text`：UTF-8 多语种排版与矢量字体引擎

1. **零外部 C 依赖矢量光栅**：内置纯 Rust `no_std + alloc` TrueType/OpenType 表解析器（`cmap` format 4/12、`glyf`/`loca` 二次贝塞尔轮廓与复合字形、`hmtx`/`hhea`/`kern`）与扫描线解析面积覆盖率光栅器（产 8-bit Alpha 位图，1px 精确抗锯齿）。系统镜像预置精简开源无衬线字体（覆盖拉丁、符号与通用规范汉字表 CJK 子集，只读映射于共享 `FontVmo`，全系统进程零拷贝共读同一份字体内存页）。
2. **两级 LRU 字形图集与段落排版**：
   - 每个 `Ui` 实例持有紧凑 `GlyphCache`（按 `(font_id, size_px_q2, codepoint)` 索引的页式图集，默认上限 `512KiB`，满时按帧代号 LRU 淘汰冷僻汉字）；
   - 段落排版器 `TextLayout` 原生解码 UTF-8，支持 Unicode 软换行（英文按词边界、CJK 按字边界、中文标点避头尾禁则）、显式 `\n`、最大行数截断省略号 `…`（`TextOverflow::Ellipsis`）、左/中/右对齐以及光标字节偏移与 `(x, y)` 视觉坐标的双向 O(log N) 互查（支撑多行文本编辑精确点击定位与选区高亮）。

### 四、`berry-ui` v2：统一 `vp` 信号树、完整弹性布局与虚拟化组件

1. **废除双轨制，全面统一 `vp` 逻辑坐标与动态主题**：
   - 废除全部 1x 像素硬编码常量，引擎所有内在尺寸、间距、圆角、字号一律以 `vp`（虚拟像素，`px = round(vp * density_q8 / 256)`）声明；当收到 `DisplayMetricsChanged` 时，引擎更新 `density_q8` 与 `viewport` 并自动标脏全树布局，**无需销毁重建组件树**。
   - `ThemeTokens` 升级为完整三层语义色表（基础色盘 → 亮/暗/高对比语义角色 → 组件状态色 Normal/Hover/Pressed/Focused/Disabled），所有标准组件 100% 从当前上下文 `ThemeTokens` 取值，切换亮暗色仅触发零重排重绘。
2. **动态可扩展 Arena 与细粒度零虚拟 DOM 信号核（`Signal<T>` / `Computed<T>`）**：
   - 节点槽表与文本存储改由受控页块池承载（单 `Ui` 默认软预算 `4,096` 节点，按需平滑扩容；短文本 `≤ 48B` 内联 SSO，长文本走引用计数共享块池 `SharedStr`），彻底解除 `512` 节点、`64B` 文本、`8` 输入框与 `1024` 字形截断硬帽。
   - 提供轻量 `Copy` 代数句柄 `Signal<T>` 与按拓扑高度求值防毛刺的 `Computed<T>`（`Memo<T>`）：控件属性绑定信号后，修改 `sig.set(val)` 仅通过精确属性掩码（`PAINT_ONLY` vs `LAYOUT`）标脏订阅该信号的叶子节点及其脏区矩形（同帧多次修改自动批处理合并），无需手写命令式 `ui.set_text(id, ...)` 与外部镜像状态同步；同时保留 `.bui` 二进制 TLV 视图资产具名槽挂载能力以支撑毫秒级 UI 热重载。
3. **OpenHarmony 6.0 沉浸式安全区默认延伸、三态 `LayoutPolicy` 与一多响应式断点**：
   - `Flex`（`Row`/`Col`）与 `LazyVGrid` 完整实现弹性伸缩与懒加载网格；原生支持 OpenHarmony 6.0 三态尺寸策略 `LayoutPolicy::{MatchParent, WrapContent, FitAtIdealSize}`；
   - **组件背景安全区默认延伸（Safe-Area Auto-Bleed）**：邻接状态栏或底部 Home 横条的根容器与导航层背景材质默认自动向外延伸铺满物理屏幕四边，而内部交互内容自动按合成器 `Insets` 内缩（支持 `ignore_safe_area(Edges)` 按边豁免），彻底消灭手工安全区垫块；
   - 内置横向栅格断点（`xs < 320pt`、`sm 320..600pt`、`md 600..840pt`、`lg >= 840pt`）与七大自适应布局原语（拉伸、均分、占比、缩放、延伸、隐藏、折行）；`Navigation` 容器在 `xs/sm` 自动呈单栈推页，在 `md/lg` 自动裂变为左侧液态玻璃边栏加右侧内容双栏（支持配置未选中态默认占位页 `Placeholder`）。
4. **开放 `Widget` Trait、iOS 26/27 液态玻璃控件库、`geometryTransition` 一镜到底与 `XComponent` 直通**：
   - 开放 `pub trait Widget` 与高性能自绘直通节点 `XComponent`（等价于 OpenHarmony 6.0 `lockCanvas` 交换链槽，供终端字符格、3D 与媒体视图零缝嵌入组件树）；
   - 严格依据 [iOS 与鸿蒙融合液态玻璃及光场交互标准](../../../docs/iOS与鸿蒙融合液态玻璃及光场交互标准-v2.0.md) 打造新一代标准控件库：随滚动折叠的 `34pt` 大标题导航栏、**随向下滚动自动流态收缩的悬浮液态玻璃胶囊 `TabBar` 与工具栏**、**底部拇指热区下沉搜索栏（Bottom-Reach Search）**、**从触发按钮轮廓原位液态生长的 `Alert` / `ContextMenu`**、**带边缘折射放大的文本选区放大镜 `Loupe`**、`0.33pt` 缩进细线 `InsetGroupedList`、按压液态拉伸 `51×31pt` `UISwitch` 以及一行声明 `.geometry_transition("id")` 即可跨页面连续形变的**一镜到底转场原语**。
   - **`VirtualList` 与 UIKit 原装物理滚动**：滚动容器严格实装 `UIScrollView` 对数橡皮筋越界公式 $d' = (1 - \frac{1}{0.55 d / D + 1}) D$、每毫秒 `0.998` 指数衰减惯性（Fling）与松手投射终点判定；`VirtualList` 仅实例化当前视口可见区加前后 1 屏缓冲区的行节点并循环复用槽位，支持 100,000 行数据在 `< 64` 活跃节点与 120fps 下丝滑滚动。
   - **完整手势竞技场**：原生支持 `Tap`、`DoubleTap`、`LongPress`（默认 500ms 触发 `ContextMenu` 预览）、全屏左边缘交互式侧滑返回（`InteractivePop`）、方向锁定 `Pan/Drag`、`Swipe` 与双指 `Pinch/Scale`。

### 五、`imed` iOS 风格输入法、动态 `Vmar` 页堆与零样板 `berry-sdk`

1. **端到端 iOS 质感输入法闭环（`imed`）**：新建无网络权限独立沙箱服务 `user/bins/imed`（持有 `PA_COMP_IME` 能力），复刻 iOS 系统键盘外观（`ThickMaterial` 磨砂底座、纯白/`#4A4A4C` 微投影圆角键帽、按下时键帽上方弹出放大预览气泡 Key Callout、顶部 `ChromeMaterial` 拼音候选词横滑栏与右下角系统蓝确认键）。当应用内 `TextInput` 获得焦点时，`berry-sdk` 自动上报 `{ input_type, action_label, cursor_screen_rect }`；用户敲击软键盘时，`imed` 实时发送 `Preedit { text, cursor }` 渲染带下划线未上屏拼音串，选词后发送 `Commit { text }` 写入 UTF-8 汉字，软键盘升起期间 `SafeArea` 以临界阻尼弹簧平滑上推焦点输入框防遮挡。
2. **基于 `Port` 的统一异步 `AppLoop` 与动态 `Vmar` 页堆**：
   - `berry-sdk` 封装单一内核 `Port`，提供 `AppCtx` 注册任意数量的自定义 IPC `Channel`、单次/周期高精度 `Timer`、异步文件/网络 `async/await` 任务唤醒句柄及子进程管道（彻底解决 `berry-term` 等多源 I/O 应用的复用痛点），耗时文件/网络等待期间主循环持续响应 120fps 触控动画，**严禁阻塞 UI 渲染线程**。
   - 重写 [alloc.rs](../../../user/berry-libc/src/alloc.rs) 取代固定 `256KiB` 静态 `.bss` 堆：小对象走分箱 Slab 按需从私有 `HeapVmar` 映射零填充页，大对象直接独立 `vmar_map` 并在释放时立即 `vmar_unmap` 100% 归还内核物理帧，使进程初始堆物理页降为 `0` 且上限随 `Job` 配额弹性伸缩。
   - 开发者入口收敛为极简声明式形态，自动完成 `ProcessArgs v2` 能力提取、动态虚拟地址映射（废除硬编码 `FRAME_VA`）、交换链双缓冲握手、`OP_WM_BACK` 导航栈自动回退以及 `appd` 冻结/墓碑时的状态树序列化与恢复：

```rust
#[berry_sdk::app]
fn main(cx: &mut AppCtx) -> impl Widget {
    let dark = cx.state_persisted("dark_mode", false);
    Scaffold::new()
        .top_bar(AppBar::title("设置"))
        .body(List::new().item(SwitchRow::new("深色模式", dark)))
}
```

3. **开发者工具链与一次性令牌隐私服务**：
   - `mkapp`（迁为纯 Rust 子命令 `berry app pack/new/check`）：允许开发者源码 `manifest.toml` 包含任意注释、空白与易读排布，打包器解析校验值域后在 `.bap` 包内自动生成规范清单、编译图标（保留 8-bit Alpha 通道升级为 `BYI2` 格式，淘汰丢弃半透明度的 1-bit 旧假设）并执行 Ed25519 签名；
   - 补齐能力门控的系统剪贴板服务（仅前台获焦会话在用户确认粘贴动作窗内获得单次只读数据）与系统文件/相册选择器（Picker 返回单文件/子目录一次性受限句柄，兑现 [应用规范](../../../docs/应用规范-v1.0.md) 零全盘授权承诺）。
   - 清理 `user/bins/` 下全部 7 对过渡双胞胎目录，7 款预置系统应用（设置、文件、终端、通讯录、相册、相机、浏览器）全部统一迁入 `berry-sdk` + `berry-ui` v2 单一代码库。

## 验收标准

1. **画质与文字门禁**：宿主机像素测试与实机 1080p 截图验证所有圆角、阴影、半透明浮层与缩放图片零锯齿、零透明渗黑；中英文、符号混排长段落在 `S12vp..S36vp` 任意字号下清晰锐利、标点避头尾正确，软键盘完成真实中文拼音组词输入与选词上屏。
2. **性能与容量门禁**：100,000 条项目的 `VirtualList` 快速惯性滑动期间活跃节点数 `< 80`，帧耗时 p99 `< 4ms`；修改单个 `Signal<bool>` 开关仅重绘该开关包围盒（零重排、零全屏重绘）；应用分配 16MiB 缓冲释放后内核物理页即时回落。
3. **开发体验门禁**：新建标准多页带持久化的 GUI 应用所需业务外样板代码为 **0 行**（零 `unsafe`、零手动 VMO/Channel/坐标计算代码）；源码 `manifest.toml` 含中英文注释可正常通过 `berry app pack` 打包验签；仓内双胞胎应用目录清零。

## 适用边界

- 本规格生效后作为客户端图形与应用 SDK 的唯一权威内部设计；`berry-ui` v1 的 1x 固定几何常量、`TRUE_CORNERS` 静态开关及 `BYI1` 1-bit 透明假设随重构波次正式退役。
- 第三方命令行工具（如 `uutils`、`hexyl`）继续走标准 Rust `std` + `berry-libc` POSIX 路径，不强制依赖 `berry-sdk` GUI 层。
