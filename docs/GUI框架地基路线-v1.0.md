# GUI 框架地基路线

> 版本：**1.0**｜2026-11-08
>
> 定位：GUI 线**执行序的现行权威**。本路线不取代 [GUI里程碑规划-2026-10-v1.0.md](GUI里程碑规划-2026-10-v1.0.md) 的目标形态、协议语义与长期架构裁决；它约束其后的实施顺序——先把 UI 地基夯实，再继续堆应用界面。任何 GUI 应用、UI SDK、桌面壳或视觉改动与本路线冲突时，先改本路线或停止该改动。

## §0 裁决与问题

2026-11-08 用户裁决：**GUI 必须从底层开始建设**。先把常用 UI 框架、运行宿主、主题、布局、组件和测试地基建好；不允许在少量系统样例上继续自上而下堆“每个 app 手写一套像素绘制”的皮毛。

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
App：业务状态、数据读取、页面语义
 ↓
berry-ui：描述式 UI / 状态失效 / 布局 / 主题 / 组件
 ↓
berry-shell：Native Shell 宿主（WM、窗口、frame、输入、生命周期）
 ↓
berry-gfx：纯光栅与字形绘制
 ↓
gui-wmd / guifbd / gui-inputd：窗口、呈现、输入服务
```

禁止反向依赖：

```text
berry-gfx 不认识 app、窗口、主题或业务
berry-ui 不收发 WM RPC，不映射 VMO
berry-shell 不定义业务页面
App 不直接操作 channel、VMO、stride 或像素
```

### 分层职责

| 层 | 职责 | 禁止事项 |
|---|---|---|
| `berry-gfx` | 纯绘制：矩形、圆角、圆、字形、图像、混合、显示列表 | 不处理 WM、输入、布局、主题或业务状态 |
| `berry-ui` | 保留树、状态失效、布局、命中测试、主题 token、组件、页面模型 | 不直接调用 syscall，不持有 WM endpoint，不写 app 业务 |
| `berry-shell` | APP env 解析、WM 连接、字体供给、窗口与 frame VMO、输入循环、frame callback、销毁生命周期 | 不绘制业务 UI，不定义控件样式 |
| GUI 服务 | 窗口/呈现/输入所有权与仲裁 | 不为了某个 app 特判 |
| App | 状态、业务数据、页面描述、消息处理 | 不手写像素、不手写 WM RPC、不复制主题常量 |

现有 `berry-nshfont` 是 Native Shell 迁移期的临时客户端/基元集合。地基完成后，其映射能力归 `berry-shell`，绘制基元归 `berry-gfx`，文本与控件消费归 `berry-ui`，该 crate 必须退役，不得成为第三套 UI 框架。

## §2 地基优先里程碑

每个里程碑有出口门；上一门未过，不得开下一门。唯一例外是修复当前镜像的阻断性缺陷，且修复不得新增 app 私有 UI 框架。

| 里程碑 | 交付 | 出口门 |
|---|---|---|
| **F0 路线冻结** | 本路线 + `规则/14-GUI与UI框架.md` | GUI 工作按本表排刀；Camera/MMS 等新 app 停止扩张 |
| **F1 Shell 宿主** | `berry-shell`：抽取五个 Native Shell 重复的 WM、frame、字体、输入、销毁生命周期 | 一个不写像素的 host probe 完成窗口、帧、输入、销毁全链；现有五 app 仍可运行 |
| **F2 UI core 夯实** | 盘点并补齐 `berry-ui` 保留树、布局、状态失效、命中测试、文本测量与 dirty 语义 | host 单测 + 离屏 golden + 布局 dump + hit-test dump 全绿；冻结 SDK 侧最小接口 |
| **F3 主题与基元** | 主题 token、排版、间距、圆角、阴影、颜色、按压/禁用/选中态；`berry-gfx` 基件单源化 | 同一描述在 800×600 与 1080×1920 得到正确布局；禁止 app 硬编码视觉常量 |
| **F4 组件第一版** | Scaffold、AppBar、Card、Column、Row、Text、Icon、Button、ListItem、Switch、Divider、Scroll、EmptyState | 每组件有布局、状态、hit-test 与离屏视觉测试 |
| **F5 UI 系统验收 app** | `uitest` 覆盖组件、主题、滚动、状态与返回栈 | 新 GUI 冒烟关卡同时验证标记、帧、截图像素、布局 dump 与输入命中 |
| **F6 Settings 迁移** | Settings 改为 `berry-shell + berry-ui` | 源码不再出现 WM RPC、VMO 映射或像素绘制；真实状态切换与重绘行为不回退 |
| **F7 Files/Contacts 迁移** | 列表/详情/收藏/文件读取迁到 UI SDK | READDIR/STAT/GET 真链保留；UI 与数据边界清晰 |
| **F8 Photos/Browser 迁移** | 图像网格、查看页、URL pill、诚实状态页迁移 | 5000B 图像与 owner-local 输入真链保留 |
| **F9 收口与守卫** | 删除迁移期重复客户端，静态守卫入 CI | Native Shell app 无直接 syscall/像素绘制；rootfs 在帽内；文档与 SDK 示例同步 |

F2 不是重写 `berry-ui` 的许可状。先按 [GUI-3实施设计草案-v1.1.md](GUI-3实施设计草案-v1.1.md)、GUI-6 的既有契约与现有实现对齐；缺口先列清单，再小步补齐。

## §3 App 层边界

F6 之后，新 GUI app 的合法形态只有：

```text
声明页面结构
绑定业务状态
处理消息并更新状态
调用获得授权的数据/系统能力
```

App 源码禁止直接出现：

```text
berry_channel_send / berry_channel_recv
berry_vmar_map / berry_vmo_create / berry_vmo_commit
WM RPC 编解码
put_px / fill / round_rect / circle / glyph 循环
frame stride 计算
主题颜色、圆角、间距的散装字面量
```

迁移前五个 Native Shell 只允许：

```text
修复阻断性缺陷
保持既有冒烟与真链行为
为抽取 berry-shell 提供对照样本
```

不得继续新增手写控件、手写页面或新的视觉体系。

## §4 测试与反 mock 门

地基的“夯实”由四层证据定义，缺一层不得宣称完成。

| 层 | 必测面 |
|---|---|
| Host 单元 | 树重建、布局盒、状态失效、命中测试、主题 token、组件状态机 |
| 离屏渲染 | 每基础组件与组合页 golden；抗锯齿文本必须有连续 alpha 层级 |
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
- 每个里程碑输出 rootfs 总量、余量和五个 Native Shell `.bap` 尺寸；超帽即停，不允许降低壁纸或图标质量换空间。
- 动画、模糊、复杂阴影、自定义绘制效果在 F4 组件集与 F5 验收 app 稳定前排刀；不得用 app 私有像素代码提前实现。

## §6 路线变更规则

改变分层、里程碑顺序或 App 禁止清单时，必须先修改本文与 `规则/14-GUI与UI框架.md`，再写代码。实现中发现设计缺口时，补进对应里程碑出口门；不得以“先做一个样例”绕开地基顺序。
