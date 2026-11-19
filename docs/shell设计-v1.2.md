# Shell 设计（bsh 与命令行生态）（v1.0）

Type: standard
Status: current
Updated: 2026-11-18
Version: 1.2.0

> 回答三件事：**用什么 shell、配哪些命令、管道怎么实现**。
> 本标准规定现行生效的 `bsh` v1/v2 命令行、内建命令与管道契约；终端优先（CLI-First）全栈闭环演进目标（用户态伪终端 `ptyd / consoled / berry-vt / bmux`、`ProcessArgs v2` 全双工流式继承与 `/svc` 服务发现、`bsh` 3.0 词法/AST 脚本与 `Job + SuspendToken` 作业控制、`sandbox` 能力裁剪及 `bed / less / btop / lscap / btrace / bdbg` 完整工具链）分别见 [pty 总体设计](../.agents/plans/pty/design.md)、[bsh 总体设计](../.agents/plans/bsh/design.md) 与 [cli-suite 总体设计](../.agents/plans/cli-suite/design.md)。
> **v1.0.1 修订（2026-11-18）**：升格 Curia `Type: standard`；补齐第 21 内建 `export` 与 `processargs v1.1` 环境变量传递现状，明确 v1/v2 外部命令缓冲执行与 `gift` 传递的适用边界，并将 3.0 全栈终端演进统一锚定至对应模块总体设计。

## 0. 分层：bash/zsh 到底站在哪

```text
┌─ shell 程序（bash / zsh / 我们的 bsh）   ← 只是应用程序
├─ 命令（ls / grep / cat …）                ← 也是应用程序
├─ POSIX 兼容库（libc 层）                   ← 把 open/write 翻译成 berry_*（经 channel 走 /svc/fs 等）
└─ 内核 ABI（berry_* syscall）                ← 与 UNIX 无关
```

**UNIX 对我们是中间一层的源码兼容**：程序用 POSIX API 写，对着我们的 libc 编译就能跑——不是搬 bash 进来，更不跑现成 Linux/macOS 二进制（架构决策既定：自有 ABI + POSIX 兼容库）。

## 1. 为什么不搬 bash/zsh（三个硬理由）

1. **协议不洁**：bash 是 GPLv3，「GPL 只看思路」铁律直接否决（zsh 协议宽松，但见下条）。
2. **语义不兼容**：bash 每执行一条外部命令都要 `fork()`——**v1 无 fork**（[系统调用 ABI 设计](系统调用ABI设计-v1.0.md) §6：`posix_spawn` → `process_create` + handle 传递），bash 在我们这连第一条命令都跑不起来；zsh 还叠加全套信号 / 作业控制 / 终端 termios 依赖。
3. **过重**：bash 十几万行；自研轻量化结构更契合微内核能力模型。

**方案：自研 `bsh`（Rust）**——学 POSIX sh 的**肌肉记忆**（管道、重定向、`&&`、环境变量、`$()`），不搬实现。操作习惯保持顺手，底层是纯能力 / channel 语义。参考对照：Redox 的 **ion shell**（Rust 先例，MIT；已过 [架构综合纲领](架构综合纲领.md) 排除 8 问——用户态程序、不碰内核中心、无 GPL）。

## 2. bsh 现行契约

### 2.1 起子进程：无 fork 的等价物

```text
bsh> run hello
     └─ process_create("hello")          ← ABI §6，posix_spawn 的等价物
        bootstrap handle = stdio channel 端点 + 必要能力
        process_start(...)
        等进程结束并 reap 回收
```

外部程序从 `bootstrap` 通道拿到自己的 stdio（channel 端点）、`argv`、`env`（`processargs v1.1` 第二消息）与初始能力，**不需要 fork 继承任何东西**——能力显式授予，血缘不隐式传递。现行 v1/v2 下单命令 `run_output` 采用私有输出通道收集至进程退出后回写（子进程 `stdin` 为空 EOF）；全双工实时 `PtySlave` 继承与 `ProcessArgs v2` 类型化能力表见 [bsh 总体设计](../.agents/plans/bsh/design.md)。

### 2.2 管道、重定向与中断：channel 即 pipe

Unix 的 `ls | grep` 靠 `fork + pipe + dup2`。BerryOS 没有 `fork`，也**不需要内核 pipe 对象**：

```text
bsh> cat a.txt | grep boot
     └─ shell: channel_create() → c0/c1
        process_create("cat",  bootstrap = c1 写端)   ← 当 stdout
        process_create("grep", bootstrap = c0 读端)   ← 当 stdin
        等全部管道段结束并取末段真退出码
```

- **短路链与重定向**：支持 `&&` 与 `||` 左结合短路链（跳过段不求值、不改 `$?`；操作符识别均在变量展开前的原始文本上执行，防止变量值注入语法操作符）；支持 `>` 覆写与 `>>` 追加；管道各段支持空白分词带参（`PipeSeg.args`，现行仅派发 `bin/` 外部程序，内建进管道与引号/脚本语法演进见 [bsh 总体设计](../.agents/plans/bsh/design.md)）。
- **`&` 后台作业（v2 过渡实现，2026-11-19）**：`run_line` 入口整行尾缀 `&` 剥离（原始文本纪律）后走 `exec_bg`——单段外部命令形异步拉起（`stdout` 直打 console、`stdin` 恒 EOF 即 `/dev/null` 语义），登记自持 job 表（`JOBS_MAX=8` 最低空位编号）并回显 `[n] cmd`、置 `$!`＝作业号；`jobs` 列示（下条命令前自动通报 `[n]+ Done(码)`）、`kill %n`＝`process_terminate(143)`＋1s 有界收割、`exit` 时活作业收杀 `terminate(129)`（本系统孤儿不可寻不可杀，放任＝永生占进程帽）。拒形：链/管道/重定向/内建（`run` 除外）/赋值——整链异步与 `fg`/`bg`/`wait`/`Ctrl-Z` 挂起随 [bsh 总体设计](../.agents/plans/bsh/design.md) 的 `Job` 树（届时本表 Superseded）。
- **Ctrl-C 杀链**：前台等待期（`collect_fg` `wait_many` 挂 `stdin` 项）收到 `0x03` → 回显 `^C` + 对持有的子进程句柄直调 `process_terminate(130)`（`128 + SIGINT` 惯例码，受 `RIGHT_MANAGE` 门控）→ 等待 `SIGNAL_TERMINATED` 收口；前台期其余按键进入 `TYPEAHEAD` 缓冲不丢键；后台作业不进前台 kill 集（v2 过渡语义——`Job` 树整树清剿见总体设计）。整棵 `Job` 树原子清剿与 `Ctrl-Z` `SuspendToken` 作业控制见 [bsh 总体设计](../.agents/plans/bsh/design.md)。

### 2.3 内建命令（23 件现役单源）

改 shell 自身进程状态的命令必须是内建；名单由 `const BUILTINS` 单源维护（`type` 身份查询、`HELP_LINES` 互钉自检与 `Tab` 补全候选同表消费），现役共 23 件（2026-11-19 作业控制刀补 `jobs`/`kill` 第 22/23 件）：`help`、`exit`、`env`、`export`、`echo`、`run`、`cat`、`ls`、`mkdir`、`rm`、`cd`、`pwd`、`inputpoll`、`clear`、`uname`、`uptime`、`sleep`、`type`、`history`、`dmesg`、`unset`、`jobs`、`kill`。非内建裸命令名自动按 `bin/<name>` 查 `rootfs` 派发并透传参数与导出环境变量集。

### 2.4 行编辑与哑终端管道契约

- **行编辑归属在用户态 `bsh`**：内核 `console` 仅作字节缓冲透传；回显、退格、上下键历史召回（8×256B 环）、`Ctrl-U` 清行与 `Tab` 补全（内建表 ∪ `rootfs` `bin/` + 目录视图，纯算法位于 `user/berry-complete`）均在 `bsh` `editor.rs` 内完成。二维自由光标、`4096` 条持久历史与 `Ctrl-R` 搜索演进见 [bsh 总体设计](../.agents/plans/bsh/design.md)。
- **哑终端同源驱动**：`bsh` 交互循环仅依赖 `fd 0` 逐字节读与 `fd 1` 字节流写；宿主机 `berry console`（见 [Berry 设备平台 CLI-first 设计](Berry设备平台-CLI-first设计-v1.9.md)）与客机 `berry-term`（纯解释核 `user/berry-termio`）均以纯字节透传方式驱动子 `bsh`，物理消除双份行缓冲失步。伪终端 `ptyd` 与 `berry-vt` 全屏协议栈演进见 [pty 总体设计](../.agents/plans/pty/design.md)。

## 3. 命令三梯队与演进锚点

| 梯队 | 命令 | 来源与状态 | 说明 |
|------|------|------------|------|
| **基础文件与文本** | `cd/ls/cat/echo/pwd/env/mkdir/rm/cp/mv/rmdir/touch/head/tail/cut/tr/sort/uniq/wc/date/df/du/true/false` + `grep/find/which/hexyl` | `bsh` 内建 + `uutils` 裁剪集 + 自研（已在册 [rootfs-manifest.toml](../tools/rootfs-manifest.toml)） | 满足日常文件管理与流过滤；全屏编辑器 `bed` 与分页器 `less` 见 [cli-suite 总体设计](../.agents/plans/cli-suite/design.md) |
| **系统与网络** | `ps / run / berryctl / dmesg`（现役）；`btop / free / vmstat / ip / ping / dig / nc / curl / bscp / bsshd`（演进） | 自研 | 现役 `ps/berryctl` 依赖启动期 `gift`；迁入 `/svc` 服务发现与网络套件规格见 [cli-suite 总体设计](../.agents/plans/cli-suite/design.md) |
| **微内核独有诊断** | `sandbox / lscap / btrace / bdbg / cboxctl` | 自研（架构差异化核心） | 能力沙箱裁剪、句柄与 Channel 拓扑透视、实时 syscall/IPC 追踪与在机交互断点调试，规格见 [bsh 总体设计](../.agents/plans/bsh/design.md) 与 [cli-suite 总体设计](../.agents/plans/cli-suite/design.md) |

**AI 与自动化友好铁律**：系统查询与诊断类 CLI（`berryctl / ps / free / lscap / btrace / cboxctl / ip`）面向演进统一提供 `--json` 纯净标准输出（诊断告警走 `stderr`），Schema 遵守 add-only 兼容策略。

## 4. 版本与兼容

- **MAJOR**：破坏既有内建命令退出码契约（`0 / 2 / 126 / 127 / 130`）或移除既有 `--json` 字段。
- **MINOR**：新增内建命令、扩展语法能力（引号、控制流、作业控制）、新增 CLI 工具与 `--json` 字段。
- **PATCH**：错误文案澄清、边界说明与跨文档链接同步。

## 5. 适用边界

- **源码级兼容而非二进制兼容**：标准 Rust / POSIX C 单体工具针对 BerryOS `libc` 交叉编译即可运行；**永久不直接运行现成 Linux / macOS 二进制**（不背异构内核 ABI 包袱）。
- **现行 v1/v2 已知边界**：在 [pty 总体设计](../.agents/plans/pty/design.md)、[bsh 总体设计](../.agents/plans/bsh/design.md) 与 [cli-suite 总体设计](../.agents/plans/cli-suite/design.md) 按 [终端与命令行全栈闭环五波十八刀实施计划](../.agents/plans/2026-11-18-终端与命令行全栈闭环五波十八刀实施计划.md) 落码前，外部命令尚无交互式 `stdin`、内建尚未进管道、`ps/berryctl` 在交互终端尚未接通 `/svc` 自动寻址。作业控制为 v2 用户态过渡面：`&` 仅单段外部命令形、`kill` 仅 `%n` 单形直杀（无信号旗标族）、`$!` 载作业号非真 pid、`fg`/`bg`/`wait`/`Ctrl-Z` 未落——终态见 [bsh 总体设计](../.agents/plans/bsh/design.md) `Job` 树。
