# GUI 框架地基路线

> 版本：**1.4**｜2026-11-08
>
> 定位：GUI 线**执行序的现行权威**。本路线不取代 [GUI里程碑规划-2026-10-v1.0.md](GUI里程碑规划-2026-10-v1.0.md) 的目标形态、协议语义与长期架构裁决；它约束其后的实施顺序——先把 UI 地基夯实，再继续堆应用界面。任何 GUI 应用、UI SDK、桌面壳或视觉改动与本路线冲突时，先改本路线或停止该改动。

## §0 裁决与问题

2026-11-08 用户裁决：**GUI 必须从底层开始建设**。先把常用 UI 框架、运行宿主、主题、布局、组件和测试地基建好；不允许在少量普通系统样例上继续自上而下堆“每个 app 手写一套像素绘制”的皮毛。

已完成的真实系统底座继续保留：

```text
APP_SPAWN 与能力交付
WM session / 窗口所有权 / surface 所有权
owner-local 输入路由
composition VMO / frame lifecycle
进程死亡清理
```

需要停止的路线是：

```text
Browser / Settings / Files / Photos / Contacts 各自维护 WM client
各自映射 frame VMO 与字体 VMO
各自 fill/text/circle
各自计算布局、样式和控件状态
```

这五个 Native Shell 是迁移前的兼容基线，不是继续扩展的实现模板。

## §1 目标分层

目标依赖方向固定为自上而下依赖、自下而上建设：

```text
普通 App：业务状态、数据读取、页面语义
 ↓
berry-ui：描述式 UI / 状态失效 / 布局 / 主题 / 组件
 ↓
berry-room：Berry Room 应用运行室（WM、窗口、frame、输入、生命周期）
 ↓
berry-gfx：纯光栅与字形绘制
 ↓
gui-wmd / guifbd / gui-inputd：窗口、呈现、输入服务

专业 App shell：BerryUI 面板/命令/属性/主题
 ↓
CanvasSurface / app document renderer
 ↓
berry-room surface / berry-gfx

游戏或实时 App：自有 engine renderer 与游戏内 UI
 ↓
Berry Room immersive surface
 ↓
Berry Session composition
```

禁止反向依赖：

```text
berry-gfx 不认识 app、窗口、主题或业务
berry-ui 不收发 WM RPC，不映射 VMO
berry-room 不定义业务页面
App 不直接操作 channel、WM RPC、VMO、stride 或全局屏写；
普通 BerryUI app 不手写像素。专业/游戏 renderer 只能写 Berry Room 授权的
CanvasSurface / immersive surface。
```

### 分层职责

| 层 | 职责 | 禁止事项 |
|---|---|---|
| `berry-gfx` | 纯绘制：矩形、圆角、圆、字形、图像、混合、显示列表 | 不处理 WM、输入、布局、主题或业务状态 |
| `berry-ui` | 保留树、状态失效、布局、命中测试、主题 token、组件、页面模型 | 不直接调用 syscall，不持有 WM endpoint，不写 app 业务 |
| `berry-room` | APP env 解析、WM 连接、字体供给、窗口与 frame VMO、输入循环、frame callback、销毁生命周期 | 不绘制业务 UI，不定义控件样式 |
| Berry Session 服务（gui-wmd/guifbd/gui-inputd） | 窗口/呈现/输入所有权与仲裁 | 不为了某个 app 特判；控件与布局不是 OS 契约 |
| 普通 App | 状态、业务数据、页面描述、消息处理 | 不手写像素、不手写 WM RPC、不复制主题常量 |
| 专业/游戏 renderer | 文档画布、世界渲染、引擎内 UI | 只写 Berry Room 授权 surface；不私接 WM/VMO/stride/全局屏写，不绕过输入、IME、a11y、帧率契约 |

`berry-nshfont` 是五个迁移前 Native Shell 的临时客户端/基元集合。字形 VMO 映射已归 `berry-room`；F9 时绘制基元归 `berry-gfx`、文本与控件消费归 `berry-ui`，该 crate 删除，不得成为第三套 UI 框架。

### 多形态 UI 契约

BerryUI 是默认声明式工具包，不是特权或强制渲染器。三条合法路径：

```text
普通应用：BerryUI 组件集
专业应用：BerryUI 外壳 + CanvasSurface + 应用文档渲染器
游戏/实时应用：immersive surface + 应用/引擎自有 renderer
```

所有路径都必须经过 Berry Room 获得 surface/input 能力，并遵守 Berry Session 的窗口、焦点、输入、IME、无障碍、帧率、DPI/安全区、系统层序与进程清理契约。游戏和专业 canvas 可以绕过 BerryUI 控件，但不能直写屏、绕过焦点或私自持有系统资源。

## §2 地基优先里程碑

每个里程碑有出口门；上一门未过，不得开下一门。唯一例外是修复当前镜像的阻断性缺陷，且修复不得新增 app 私有 UI 框架。

| 里程碑 | 交付 | 出口门 |
|---|---|---|
| **F0 路线冻结** | 本路线 + `规则/14-GUI与UI框架.md` | GUI 工作按本表排刀；Camera/MMS 等新 app 停止扩张 |
| **F1 Berry Room 宿主** | `berry-room`：抽取五个 Native Shell 重复的 WM、frame、字体、输入、销毁生命周期 | 一个不写像素的 host probe 完成窗口、帧、输入、销毁全链；现有五 app 仍可运行 |
| **F2 UI core 夯实** | 盘点并补齐 `berry-ui` 保留树、布局、状态失效、命中测试、文本测量与 dirty 语义 | host 单测 + 离屏 golden + 布局 dump + hit-test dump 全绿；冻结 SDK 侧最小接口 |
| **F3 主题与基元** | 主题 token、排版、间距、圆角、阴影、颜色、按压/禁用/选中态；`berry-gfx` 基件单源化 | 同一描述在 800×600 与 1080×1920 得到正确布局；禁止 app 硬编码视觉常量 |
| **F4 组件第一版** | Scaffold、AppBar、Card、Column、Row、Text、Icon、Button、ListItem、Switch、Divider、Scroll、EmptyState | 每组件有布局、状态、hit-test 与离屏视觉测试 |
| **F5 UI 系统验收 app** | `uitest` 覆盖组件、主题、滚动、状态与返回栈 | 新 GUI 冒烟关卡同时验证标记、帧、截图像素、布局 dump 与输入命中 |
| **P0 UI 形态与 Surface Mode 协议** | Surface role/mode、safe area、scale factor、input class、IME composition、a11y sidecar、canvas/immersive escape hatch 的协议词表与 host 编解码测试 | 一个系统契约承载普通/专业/游戏三路径；BerryUI 无特权；自绘路径仍不能直写屏或绕过焦点/输入/无障碍/帧率契约；P0 未过不得开 F6 |
| **F6 Settings 迁移** | Settings 改为 `berry-room + berry-ui` | 源码不再出现 WM RPC、VMO 映射或像素绘制；真实状态切换与重绘行为不回退 |
| **F7 Files/Contacts 迁移** | 列表/详情/收藏/文件读取迁到 UI SDK | READDIR/STAT/GET 真链保留；UI 与数据边界清晰 |
| **F8 Photos/Browser 迁移** | 图像网格、查看页、URL pill、诚实状态页迁移 | 5000B 图像与 owner-local 输入真链保留 |
| **F9 收口与守卫** | 删除迁移期重复客户端，静态守卫入 CI | 已迁移普通 Native Shell app 无直接 syscall/WM/VMO/stride/像素绘制；专业或游戏 renderer 的像素算法只能出现在 P0 授权 CanvasSurface/immersive surface 之后；rootfs 在帽内；文档与 SDK 示例同步 |

F2 不是重写 `berry-ui` 的许可状。先按 [GUI-3实施设计草案-v1.1.md](GUI-3实施设计草案-v1.1.md)、GUI-6 的既有契约与现有实现对齐；缺口先列清单，再小步补齐。

BUI 不进入 F4/F5 出口门：组件与 UI 系统验收 app 先使用 Rust typed API（`ViewSpec`/`DescBuilder`）与 BerryUI AST TLV；`.bui` 语法待组件面和测试面稳定后定形，且只能作为收敛到同一 AST 的可选声明式入口。现有 `views/main.bui` 仍是资源挂点占位，不解析、不执行、不构成 GUI app 必需文件。

本路线的 **P0** 只指 GUI 线在 F5 与 F6 之间的 Surface Mode 插入门，不改变 `路线图-v1.0.md`、调点调度或 Berry 设备壳中同名 P0 阶段号。

## §3 App 层边界

F6 之后，新的普通 BerryUI app 合法形态只有：

```text
声明页面结构
绑定业务状态
处理消息并更新状态
调用获得授权的数据/系统能力
```

专业应用与游戏必须走 P0 冻结的 CanvasSurface / immersive surface 词表：
外壳或引擎可以自绘内容，但只能通过 Berry Room SDK 获得 surface/input/IME/
无障碍/帧率能力，不能直接接触 syscall、WM RPC、frame stride 或全局屏写。

所有 GUI app 禁止直接出现：

```text
berry_channel_send / berry_channel_recv
berry_vmar_map / berry_vmo_create / berry_vmo_commit
WM RPC 编解码
全局 framebuffer / display MMIO 直写
frame stride 计算
绕过 Berry Room 的输入、IME、无障碍、帧率通道
```

普通 BerryUI app 另外禁止：

```text
put_px / fill / round_rect / circle / glyph 循环
主题颜色、圆角、间距的散装字面量
```

P0 之后的专业/游戏 renderer 可以在文档画布、世界渲染或引擎 UI 内实现像素算法，
但写入目标只能是 Berry Room SDK 授权的 CanvasSurface / immersive surface，且必须
保留 input class、IME composition、a11y sidecar、safe area/scale factor 与帧率意图。

迁移前五个 Native Shell 只允许：

```text
修复阻断性缺陷
保持既有冒烟与真链行为
为抽取 berry-room 提供对照样本
```

不得继续新增手写控件、手写页面或新的视觉体系。

## §4 测试与反 mock 门

F4/F5 的“夯实”由四层基础证据定义；P0 另加协议证据，缺一层不得宣称完成。

| 层 | 必测面 |
|---|---|
| Host 单元 | 树重建、布局盒、状态失效、命中测试、主题 token、组件状态机 |
| 离屏渲染 | 每基础组件与组合页 golden；抗锯齿文本必须有连续 alpha 层级 |
| P0 协议 | Surface role/mode、input class、IME composition、a11y sidecar、safe area/scale factor 的 host 编解码、round-trip、拒面与不变量测试 |
| Guest 系统 | `uitest` 走真实 APP_SPAWN、WM session、frame、input、destroy 链 |
| 迁移回归 | 每迁移一个 app，原有真实数据链、状态重绘、窗口销毁与截图像素门不回退 |

视觉反 mock 不接受“看起来像”：

```text
marker 必须对应真实状态变化
frame 必须对应真实 SUBMIT
截图必须含像素锚与文本色数锚
输入必须经过 owner-local 路由
布局必须有可 dump 的几何结果
```

## §5 预算与演化纪律

- 昂贵字表、图标解码与大尺寸图像缓存只能有一份系统供给，不允许每个 app 静态复制。
- v1 `vmar_map` 只有 UserRw 映射；在只读映射能力落地前，系统字形 VMO 必须按 app 私有拷贝，防跨 app 写穿。后续若引入只读映射或字体服务，先更新本节与能力模型文档。
- UI SDK 组件必须可裁剪，LTO 后未用组件不得显著膨胀每个 `.bap`。
- 每个里程碑输出 rootfs 总量、单卷尺寸/帽和五个 Native Shell `.bap` 尺寸；容量口径遵循 A1 两帽模型（单卷 ≤32MiB、多卷总量 ≤128MiB）。超帽即停，不允许降低壁纸或图标质量换空间。
- 动画、模糊、复杂阴影、自定义绘制效果在 F4 组件集与 F5 验收 app 稳定前排刀；不得用 app 私有像素代码提前实现。

## §6 路线变更规则

改变分层、里程碑顺序或 App 禁止清单时，必须先修改本文与 `规则/14-GUI与UI框架.md`，再写代码。实现中发现设计缺口时，补进对应里程碑出口门；不得以“先做一个样例”绕开地基顺序。
