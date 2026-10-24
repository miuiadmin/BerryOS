# Shell 设计（bsh 与命令行生态）

> 回答三件事：**用什么 shell、配哪些命令、管道怎么实现**。
> 先拆一个常见误解：内核不需要"支持 shell"——shell 只是一个普通用户态程序，干的活就是一个死循环（读一行 → 解析 → 起子进程 → 等它跑完）。bash / zsh / fish 全是这个循环，只是语法与功能不同。
> 里程碑挂钩 `路线图.md` P5（"UART 上交互式 shell"）；体验侧展开见 `开发体验.md` §3；进程创建/handle 传递见 `系统调用ABI设计.md` §6。

## 0. 分层：bash/zsh 到底站在哪

```
┌─ shell 程序（bash / zsh / 我们的 bsh）   ← 只是应用程序
├─ 命令（ls / grep / cat …）                ← 也是应用程序
├─ POSIX 兼容库（libc 层）                   ← 把 open/write 翻译成 berry_*（经 channel 走 /svc/fs 等）
└─ 内核 ABI（berry_* syscall）                ← 与 UNIX 无关
```

**UNIX 对我们是中间一层的源码兼容**：程序用 POSIX API 写，对着我们的 libc 编译就能跑——不是搬 bash 进来，更不跑现成 Linux/macOS 二进制（架构决策既定：自有 ABI + POSIX 兼容库）。

## 1. 为什么不搬 bash/zsh（三个硬理由）

1. **协议不洁**：bash 是 GPLv3，"GPL 只看思路"铁律直接否决（zsh 协议宽松，但见下条）。
2. **语义不兼容**：bash 每执行一条外部命令都要 `fork()`——**v1 无 fork**（`系统调用ABI设计.md` §6：`posix_spawn` → `process_create` + handle 传递），bash 在我们这连第一条命令都跑不起来；zsh 还叠加全套信号 / 作业控制 / 终端 termios 依赖。
3. **过重**：bash 十几万行；bsh v0 预计 <2000 行。

**方案：自研 `bsh`（Rust）**——学 POSIX sh 的**肌肉记忆**（管道、重定向、`&&`、环境变量、`$()`），不搬实现。这正是"人无我有，人有我强"：操作习惯跟人家一样顺手，底层是纯能力 / channel 语义。参考对照：Redox 的 **ion shell**（Rust 先例，MIT，可进 `reference/` 对照；已过 `架构综合纲领.md` 排除 8 问——用户态程序、不碰内核中心、无 GPL）。

## 2. bsh 设计要点

### 2.1 起子进程：无 fork 的等价物

```
berry> run hello
     └─ process_create("hello")          ← ABI §6，posix_spawn 的等价物
        bootstrap handle = stdio channel 端点 + 必要能力
        process_start(...)
        等 Job 对象
```

外部程序从 bootstrap 拿到自己的 stdio（channel 端点）与初始能力，**不需要 fork 继承任何东西**——能力显式给，血缘不传递。

### 2.2 管道：channel 即 pipe（微内核世界观的兑现）

Unix 的 `ls | grep` 靠 fork + pipe + dup2。我们没有 fork，也**不需要内核 pipe 对象**：

```
berry> ls | grep boot
     └─ shell: channel_create() → c0/c1
        process_create("ls",   bootstrap = c1 写端)   ← 当 stdout
        process_create("grep", bootstrap = c0 读端)   ← 当 stdin
        等两个 Job 结束
```

重定向 `>` 同理（`/svc/fs` 的 handle 传给子进程当 stdout）。**Ctrl-C 不用 Unix 信号**：shell 收键 → signal 一个 Event/Job 对象 → 监督服务处置（`能力模型设计.md` / `IPC设计.md` 语义）。

### 2.3 内建命令（为什么 cd 不可能是外部程序）

`cd` 改的是 shell 自己进程的状态，`type`/`exit`/环境变量赋值同理——这些必须是 shell 内建。判据一句话：**改 shell 自己状态的，进 shell；干活的，独立程序**。

### 2.4 串口行编辑

P5 第一刀 = PL011 **RX 中断**（`开发体验.md` §3 既定前置），其上是自写 mini-readline：退格 / 上下键历史 / Tab 补全（命令名 + 路径）。不引 ncurses，串口 raw 模式够用。

## 3. 命令三梯队

| 梯队 | 命令 | 来源 | 说明 |
|------|------|------|------|
| **基础** | cd/ls/cat/echo/pwd/env/mkdir/rm/cp/mv/head/tail/grep/wc | **uutils/coreutils（MIT，Rust 版 coreutils）交叉编译** | 常规命令一个不用自己写；`cd`/`type` 是 shell 内建；G-eco 关的选型逻辑（不依赖 fork/信号的单体工具）同源 |
| **开发** | ps / run / ktest / berryctl | 自研 | `ps` 看任务/进程/Job 树；`ktest` 跑关卡（smoke 的串口断言同源） |
| **独有（人无我有）** | **berryctl**（handle/能力树查看、遥测环 dump、AI 策略注入与回滚）、**powertop**（谁在挡住深睡）、**trace**、affinity/taskset（AI 放置） | 自研 | Linux 的 `ps` 看不到能力树和策略状态——**CLI 的差异化全在这梯队**；亦即三条钦定原则的 CLI 落点 |

**AI 友好化落点**：berryctl/ps/trace 全部 `--json` 机器可读输出 day-one（默认人类可读）——AI agent 在 shell 里跑命令、读结构化结果、做判断，不靠正则刮人类文案。

BusyBox 的"单二进制多命令"只作思路参考（GPLv2，不搬代码）；uutils 按 feature 裁剪单命令构建，体积可控。

## 4. 兼容边界（期望管理）

- ✅ **源码级**：ripgrep/hexyl 这类不依赖 fork/信号的单体 Rust 工具，交叉编译就跑（P4 G-eco 关的硬验收）。
- ✅ 简单 POSIX sh 语法脚本，bsh 能跑个大概（v0 语法子集内）。
- ❌ bash 脚本、`configure` 那种重度 shell+fork 的：远期，不进承诺。
- ❌ 现成 macOS/Linux 二进制：永远不跑（不背 Linux ABI 包袱）。

## 5. 阶段化目标

| 时点 | 内容 | 验收 |
|------|------|------|
| P5 第一刀 | PL011 RX 中断 + mini-readline（**已落地 2026-10，A-1/A-2 两步**：A-1 = UART RX 面 + GIC SPI 33 + console 三项 wait_many，`RX ok` = smoke 交互相标记；A-2 = 键盘行转发端 + bsh 挂载。mini-readline（P5 v0 收官补齐）= console 行编辑归属：退格/上下键历史（8×96B newest-first 环 + ESC 序列状态机）/Ctrl-U，**Tab 补全 defer v2**——补全需 rootfs 知识（编辑会话归属随 v2 留档 §2.4）；smoke RX 相注入退格 + `ESC[A` 召回双验） | 串口双向：能打字回显、历史（smoke 三跑注入序列断言） |
| P5 v0 | bsh：内建命令 + `run` 外部程序 + 环境变量 + `&&`（**已落地 2026-10**：环境变量 = shell 局部 vars + `$NAME`/`$?` 展开 + `env` 内建（子进程 env 传递 defer v2——argv 块无 env 面，`export` 随之）；`&&` = 切行短路 + 每段执行时展开（`A=1 && echo $A` 求值序）；状态码 sh 惯例 0/2/126/127/子真码——真码源 = **0x3A `process_get_exit_code`（P5 扩号，ABI 单源管线首次真实扩号 28→29）**，`ChildHandles::reap` 改 `Result<i32>`；`fail`（exit 42）= rootfs 确定性非零源；自验 = and_selftest 三步断言 + smoke `EXIT ok` 强化 code==0） | 串口跑 `run hello`（`开发体验.md` §3 场景成真；bsh `&&` 自验内嵌 `run fail` 真码 42） |
| P5 v0.5 | channel 管道 `\|`（已落地 2026-10：`PIPE ok` = rootfs 相第 10 标记，bsh 三跑对拍自验）；重定向 `>` 依赖 FS（/svc/fs），随 P6 | `hexyl \| hexyl` 端到端 |
| P6 | uutils 文件命令全家 + berryctl/powertop/trace 三件套 + `>` 重定向（A-1 最小面已落地 2026-10：fsmgr RAM-fs 服务 + berry-fsproto 协议 + bsh `>`/cat/ls，`FS ok n=` = rootfs 相第 11 标记；**2026-10-23 注：P6 已收官，uutils 全家与 berryctl/powertop/trace 三件套未随 P6 落地——滑移 M-VM（M-G3「CLI 可用」站，C-2 拆步，见 `VM里程碑设计草案.md`）**） | `berryctl --json` 遥测可读 |
| 远期 | 更丰富脚本语法；受限 fork（v3+ 探索）后评估更多 POSIX 面 | — |

## 6. 与既有文档的挂钩

- `路线图.md` P5：本文是"shell + 基础命令"里程碑的展开。
- `开发体验.md` §3：`berry>` 提示符、`berryctl test run G3` 的出处在此定名。
- `系统调用ABI设计.md` §6：process_create / handle 传递 = 起子进程的 ABI 根基；§8.5 稳定性行政策同样约束 shell 的消息格式（berryctl 与各命令的 `--json` schema 按 add-only 管理）。
- `IPC设计.md`：channel 当 pipe、Event 当 Ctrl-C 的语义来源。
- `可观测性设计.md`：trace 命令 = 遥测环的人机/AI 双面出口。
