# BerryOS 命名体系

> 版本：**1.1**｜2026-11-08
>
> 定位：BerryOS 公共架构名的单一事实源。内核定名依据见 [内核定名-v1.0.md](内核定名-v1.0.md)；本页把内核、用户态、图形会话、桌面、应用运行环境、UI 框架与开发工具的公共名一次定形。工程文件名可以保留更具体的实现名；公共文档、SDK 文档与对外叙事不得自行发明同层别名。

## §1 公共名

| 层级 | 公共名 | 中文名 | 职责 | 现行工程落点 |
|---|---|---|---|---|
| 内核 | **Meihe Kernel** | 莓核 | 微内核、调度、IPC、能力与内存机制 | `kernel/` |
| 用户态 | **Berry Space** | 莓空间 | Meihe 之上的完整用户空间：libc/syscall binding、服务、进程、能力与应用模型 | `user/` 总称，暂不建包 |
| 图形会话 | **Berry Session** | 莓图形会话 | 显示、输入、窗口管理、合成与会话生命周期 | `guifbd` + `gui-inputd` + `gui-wmd` 组成；未来可收敛出 session manager |
| 桌面 | **Berry Home** | 莓家 | 用户看到的桌面：壁纸、状态栏、图标网格、Dock 与应用启动 | `user/bins/berry-launcher` |
| 应用运行环境 | **Berry Room** | 莓室 | 单个应用持有的 WM session、窗口、frame VMO、输入队列、系统字形 VMO 与生命周期 | `user/berry-room`，公开类型 `AppRoom` |
| UI 框架 | **BerryUI** | 莓 UI | 声明式 UI：保留树、布局、状态失效、主题与组件 | `user/berry-ui` |
| UI 语言 | **BUI** | BUI | BerryUI 的可选声明式描述语言；`.bui` 是源/资源文件，不是第二运行时 | 现行入口 = Rust `ViewSpec`/`DescBuilder` + AST TLV；BUI 语法未定形，定形后必须收敛到同一 AST |
| 开发 SDK | **Berry SDK** | 莓 SDK | crates、工具链、模板、打包、调试与文档的完整开发者套件 | 未来 `berry-sdk` 汇总面 |

## §2 层级关系

```text
BerryOS
├── Meihe Kernel
└── Berry Space
    ├── Berry Session
    │   ├── display
    │   ├── input
    │   ├── window management
    │   └── composition
    ├── Berry Home
    ├── Berry Room
    └── BerryUI
        ├── Rust ViewSpec / DescBuilder
        ├── AST TLV
        └── BUI (.bui, optional source → same AST)

Berry SDK
├── sys/libc crates
├── Berry Room API
├── BerryUI typed API
├── optional BUI source / .bui tools
├── mkapp / debugger / templates
└── developer docs
```

对外叙事固定为：

```text
Meihe runs the kernel.
Berry Space is the user-space world.
Berry Session brings up the graphical environment.
Berry Home is the desktop.
Every app runs in its own Berry Room.
Developers build interfaces with BerryUI through the Berry SDK;
BUI is an optional declarative entry into the same BerryUI tree.
```

中文叙事固定为：

```text
莓核运行内核；
莓空间承载用户态；
莓图形会话启动图形环境；
莓家是桌面；
每个应用都有自己的莓室；
开发者通过 Berry SDK 使用 BerryUI 构建界面；
BUI 是进入同一棵 BerryUI 树的可选声明式入口。
```

## §3 BUI 边界

BUI 是 BerryUI 的声明式描述语言，不是第二套 UI 框架、布局引擎、渲染后端或脚本运行时。`.bui` 是 BUI 源文件/资源文件扩展名；声明式布局不要求 app 使用 `.bui`，Rust typed API 与 AST TLV 是现行第一入口。

```text
Rust ViewSpec / DescBuilder
        ↓
BerryUI AST TLV
        ↓
BerryUI retained tree → layout / hit / paint / frame

.bui source（可选）
        ↓ compile / parse / validate
同一 BerryUI AST TLV
```

BUI 可描述：

```text
组件树
组件属性
主题 token 引用
状态绑定与消息名绑定
无障碍标签与资源 id
```

BUI 禁止承载：

```text
命令式副作用或业务流程
WM RPC / channel / syscall
VMO / stride / frame 提交
像素绘制或 x-y 散装布局
任意热更新逻辑
```

实施顺序同样固定：F4 组件与 F5 UI 系统验收 app 先使用 Rust typed API；BUI 语法待组件面与测试面稳定后定形。现有包内 `views/main.bui` 只是 GUI-7 的资源挂点占位，v1 打包与 appd 只验在场，不解析、不执行，也不构成 GUI app 必需文件。

## §4 命名边界

- **Meihe Kernel** 不用于 SDK、桌面或用户态服务；`Meihe SDK` 不是合法名称。
- **Berry Space** 是用户空间总称，不是外层虚拟机、桌面或图形会话名。
- **Berry Session** 是图形会话，不覆盖单个应用的窗口/frame 所有权；后者归 Berry Room。
- **Berry Home** 只指桌面 launcher，不包含窗口管理服务或渲染后端。
- **Berry Room** 是 per-app runtime；未来多窗口仍属于同一个 Room 的资源集合。
- **BerryUI** 是框架名；页面、场景或路由不得叫 BerryUI 之外的新公共层名。
- **BUI** 只指 BerryUI 的可选 UI 描述语言与 `.bui` 源/资源文件，不指运行时框架、布局引擎或脚本语言。
- **Berry SDK** 是工具与库的集合名，不替代具体 crate 名。

## §5 保留与禁用名

| 名字 | 处置 | 原因 |
|---|---|---|
| Berry Stage / 莓台 | 禁用为架构层名 | 避免与 OpenHarmony Stage Model 混淆；会话生命周期语义弱于 Session |
| Berry Scene / 莓景 | 保留给未来 BerryUI 页面/场景语义，禁用为图形会话 | Scene 更像页面或场景图，不是系统会话 |
| Berry Hotel / 莓宿 | 保留为解释隐喻，禁用为正式层名 | 服务业联想强，并与 Berry Home 的个人设备心智冲突 |
| Berry Show / 莓秀 | 禁用为架构层名 | 动词与节目联想强，不能覆盖窗口、输入、合成与会话生命周期 |
| Berry Room Service | 禁用 | 双关过度；系统服务公共名需要工程化表达 |
| Berry Shell | 禁用为 GUI 层名 | 与 bsh、宿主 `berry shell` CLI、桌面 shell 冲突 |
| Meihe SDK | 禁用 | Meihe 是内核名，SDK 覆盖用户态与 GUI |

开发者设备壳继续使用既有宿主 CLI 名：

```text
berry shell
```

它属于 Berry 设备平台，不是 Berry Space、Berry Session 或 Berry Room 的公共架构名。

## §6 变更规则

新增或改名公共层时，先更新本页与 `架构决策.md` 对应 ADR，再改实现名。公共名只承载稳定概念；内部模块、函数与测试名仍以职责准确为第一原则。
