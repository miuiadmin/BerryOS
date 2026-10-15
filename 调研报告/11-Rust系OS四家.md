# Redox / Theseus / rCore→ArceOS 谱系 / Hubris —— Rust 系 OS 四家调研报告

> 调研日期 2026-10-15 · 基于公开资料与源码快照（GitHub API 抓取现行源码 + 本地 `参考源码/zcore` 2022-11 快照实测）· 许可证：Redox=MIT · Theseus=MIT · zCore=MIT（**rCore-Tutorial-v3 为 GPL-3.0**）· ArceOS=Apache-2.0（仓库另附 GPLv3/Mulan 双许可文本）· Hubris=**现 MPL-2.0**（2021 首发 Apache-2.0，后按 Oxide 许可 RFD 切换）

## 0. 一句话定位

- **Redox**：十年之久的全 Rust 类 Unix 通用微内核 OS，「一切资源皆 URL（scheme）」，用 POSIX 面孔做微内核用户态驱动生态，仍处 pre-stable（最新 release 0.9.0，2025-09-22）。
- **Theseus**：以「intralingual design」为纲的实验性宏内核（单地址空间、多堆），把 OS 不变量塞进 Rust 类型系统，验证在线演化与故障恢复；学术目标达成后主仓自 2024-09 起停更。
- **rCore→ArceOS 谱系**：清华系「教学内核（rCore-Tutorial）→ Zircon 语义 Rust 复刻（zCore）→ 组件化 unikernel（ArceOS）→ Linux 兼容宏内核（Starry）」四代演化，最大价值是中文教学生态与组件化组装路线。
- **Hubris**：Oxide 服务器（已出货近百架机柜）SP/电源等深度嵌入式固件的**生产级静态消息传递内核**：任务集/内存全在编译期定死，会合式 IPC + 监督重启，配套 humility 调试器——Rust OS 里唯一「在真实数据中心跑着」的微内核级内核。

## 1. 谁在做：组织/社区/治理/活跃度/商业背书

### 1.1 Redox（MIT，活跃）
创始人 Jeremy Soller（System76 首席系统架构师），社区开发 + 非营利组织（Redox OS Nonprofit）运营。2025 年 6 月发布十周年演讲「10 Years of Rust and Redox」，12 月月报披露社区捐赠约 $17,000 + Soller 个人注入 $20,000。主仓 `redox-os/redox` 16.6k stars、内核仓 2026-09 仍有 push，月报（This Month in Redox）持续到 2025-12。未发布 1.0，定位 pre-stable（Wikipedia）。**结论：个人主导 + 小规模全职 + 社区捐款，节奏稳但资源薄。**

### 1.2 Theseus（MIT，事实停滞）
源头是 Kevin Boos 在 Rice 大学的博士课题（OSDI'20 论文，被引 133+），随后随其赴牛津做博士后。主仓 `theseus-os/Theseus`（3.2k stars）默认分支 `theseus_main` 最后一次 push 是 **2024-09-22**（"Makefile: add support for building on FreeBSD"）；README 仍自称 under active development，但 Boos 的 FOSDEM 2025 演讲者简介已用过去时："*Previously, he created Theseus OS*"，本人现转向 Project Robius/Robrix。**结论：单人学术项目，领袖转身即停摆——尚未见任何官方接盘声明（待核实）。**

### 1.3 rCore 谱系（MIT/GPL 混合，生态活、单仓节奏分化）
rcore-os 组织 + 清华大学 + os2edu 开源操作系统训练营（2025 春季训练营仍在开营，opencamp.cn）。各仓状态（gh api，2026-10-15 实测）：`rCore-Tutorial-v3` 2.1k stars、2026-07 仍在维护（v3.6，教材 Book-v3 常青）；`zCore` 1.8k stars，2022-11 至 2026-09 **沉寂近四年后只出现两个提交**（升级 nightly、"adapt to Fuchsia 2026-09-03"），非持续开发；ArceOS 已迁至独立组织 `arceos-org/arceos`（782 stars，2026-06 仍活跃）；`Starry-OS/Starry`（61 stars，2026-05 活跃）承接 Linux 兼容宏内核方向。**结论：作为社区/教学生态高度活跃，作为单一内核项目均属研究/教学节奏。**

### 1.4 Hubris（MPL-2.0，重点，生产中）
Oxide Computer Company（美国，整机柜云硬件公司）出品：2021-11 开源（时为 Apache-2.0），现行 LICENSE 为 **MPL-2.0**（依据 HN 披露的 Oxide 许可 RFD：「For any new Oxide-created software, the MPL 2.0 should generally be the license of choice」——文件级弱 copyleft）。仓库 3.6k stars、**push 到 2026-10-14（前一天）**，配套调试器 `humility` 同步活跃。商业背书最硬：Oxide 2023-10 宣布首批商业机柜出货，2025-08 获 $100M B 轮，2026-10 再获约 $445M（SiliconANGLE），LinkedIn 客户文章称已交付近百架机柜——Hubris 跑在每台 Gimlet 计算sled 的服务处理器与 Sidecar 上，是**真正在数据中心生产运行的 Rust 内核**。

## 2. 总体架构：内核形态、分层与组件

- **Redox = 微内核（内核态极小，服务全在用户态）**。内核仓 `src/` 仅保留 `context`（进程/线程）、`scheme/`（资源命名空间派发）、`syscall/`、`memory`、`arch/`（x86_64/aarch64/dynamic linker 等）；驱动、`redoxfs`（用户态 CoW 校验 FS，src 含 `transaction.rs`/`htree.rs`）、显示服务器 Orbital、libc（relibc，Rust 写的 C 库，1k stars）全在用户态。
- **Theseus = 实验性宏内核/单地址空间**。根目录 `kernel/`（200+ crate 平铺：`task`、`spawn`、`memory`、`frame_allocator`、`multiple_heaps`、`slabmalloc`、`interrupts`、`scheduler*`、`nano_core`、`crate_swap`、`ota_update_client`、`wasi_interpreter`、`tlibc`…）、`applications/`、`libs/`、`ports/`；x86_64 为主，`memory_aarch64`/`gic`/`uart_pl011` 等 aarch64 crate 存在但调度器 crate 仍 `cfg(target_arch = "x86_64")`（见 §3.2）。
- **zCore = 「双模」内核框架**：`zircon-object`（对象/能力/任务/VM）+ `zircon-syscall`（Zircon ABI）+ `linux-object`/`linux-syscall`（Linux 兼容层，GPL 血统、BerryOS 禁碰）+ `kernel-hal`（架构抽象）+ `loader`；既能裸机跑也能作为用户态 libos 跑（"Reimplement Zircon in safe Rust as a userspace program"）。**ArceOS = 组件化 unikernel**（受 Unikraft 启发）：`modules/ax*`（axtask/axhal/axmm/axdriver/axnet/axfs…）+ `ulib/` + `examples/`，按需组装出专用 OS，x86_64/riscv64/aarch64/loongarch64 四架构，README 明示 Linux 应用兼容是未勾选 TODO（交给 Starry）。
- **Hubris = 静态消息传递单核 RTOS 内核**。目录：`sys/`（`kern` 内核、`abi`、`userlib` 任务侧运行时、`kerncore`、`num-tasks`）、`drv/`（驱动库+服务 bin）、`task/`（可复用任务：监督者 `jefe`、`thermal`、`sensor`、`net`、`power`、`packrat`、`control-plane-agent`…）、`idl/`（Idol/idolatry IDL）、`app/`（如 `app/gimlet` 整机固件定义）、`build/`（构建系统）。内核文档自述「not a deeply principled kernel」，四原则：分离编译、静态配置、偏好安全代码、偏好简单算法而非快算法。

## 3. 核心机制深挖

### 3.1 Redox：一切皆 scheme（资源=URL）
scheme 是内核与用户态的唯一资源协议：路径形如 `scheme:path`（`disk:0/...`、`display:...`），`open` 得到的 fd 后续 `read/write/close/fmap` 都被派发到对应 scheme。内核内置 scheme 在 `src/scheme/mod.rs` 可枚举：`acpi:` `dtb:` `debug:` `event:` `irq:` `memory:` `pipe:` `proc:` `serio:` `sys:` `time:` `user:`，其中 `user.rs` 的 `UserScheme` 是**用户态驱动的接管通道**——内核把 fd 操作打包成消息发给用户态 scheme 守护进程（UserInner 挂 `WaitQueue<Sqe>` 请求队列 + `Slab<State>` fd 状态表）。好处：驱动/FS/网络的故障全部隔离在用户态，内核不用懂任何设备；代价：每次 IO 过一次内核-用户消息往返。relibc 把这套 URL 世界观翻译回 POSIX（`open("/etc/fstab")` 实为打开 `file:` scheme），使 Rust/C 程序可移植。2024 起 Redox 桌面弃自研组件、全面接入 System76 COSMIC（Files/Editor/Terminal 成常驻），2025 RustConf 上 Soller 用 Redox + COSMIC Reader 自托管做演讲——「Rust OS 跑 Rust 桌面」的标志性 demo。

### 3.2 Theseus：intralingual 设计与编译期调度策略
Theseus 的核心主张（README 原文）：用 **intralingual design** 把 OS 职责（资源管理、不变量）移交给编译器——类型即元数据、类型状态即权限，代表作是 OSDI'20 论文与 Boos 博士论文（theseus-os.com book 收录）。两个可验证的机制证据：其一，**调度策略以并列 crate + 编译期 cfg 选择**：`kernel/scheduler`（封装入口）、`scheduler_round_robin`、`scheduler_epoch`、`scheduler_priority`，lib.rs 注释明示 "The policy is selected by specifying a Rust `cfg` value at build time: `make THESEUS_CONFIG=epoch_scheduler`"——是编译期插件而非运行时 trait；且该 crate 顶部 `#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]` 暴露 aarch64 支持不完整。其二，**在线演化**：`kernel/crate_swap`、`fault_crate_swap`、`ota_update_client` 实现 crate 级热替换，配合 `mod_mgmt`/`crate_metadata` 在运行时重载模块——这是其论文卖点（state spill 治理）。内存上 Theseus 并非「无 alloc」：它有 `heap`/`multiple_heaps`/`slabmalloc` 多套分配器；真正的差异化是把大量内核元数据从运行时堆结构改为类型承载（`memory` crate 的地址 newtype 等）。

### 3.3 zCore：安全 Rust 复刻 Zircon；ArceOS 组件化（本地源码实测）
本地 `参考源码/zcore`（MIT）实测：**架构抽象用 `hal_fn_def!` 宏**（`kernel-hal/src/macros.rs`）——按 `boot`/`cpu`/`mem`/`vm`/`irq` 分组声明 HAL 函数签名（如 `pub fn current_vmtoken() -> PhysAddr`），架构目录用 `hal_fn_impl!` 提供实现，libos 模式可整体替换为宿主实现；这使 `zircon-*` 主体零架构代码。**对象模型**：`zircon-object/src/object/` 的 `KObjectBase` + `impl_kobject!` 宏 + `Rights` bitflags（`DUPLICATE=1<<0`、`TRANSFER`、`READ`、`WRITE`、`EXECUTE`、`MAP`…），handle 表挂在 `task/process.rs`；`ipc/channel.rs` 的 `Channel` 用 `VecDeque` 收消息队列 + `futures::oneshot` 实现 `zx_channel_call` 的 reply 关联，syscall 层全 async。**安全分层实测（grep -rn unsafe 行命中/总行数）**：`kernel-hal` 104/6065（unsafe 集中在 HAL），`zircon-object` 50/15558，`zircon-syscall` 5/4534 且 crate 顶部 `#![deny(warnings, unsafe_code, unreachable_patterns)]`——「逻辑层零 unsafe、unsafe 下沉 HAL」在 zCore 是可复制的既成事实。ArceOS 则换赛道：不模拟任何现有 OS，`modules/axtask`（调度/FIFO/RR/CFS、SMP 每核 runqueue）、`axhal`、`axdriver`、`axnet`(smoltcp)…按 feature 组装，目标「一次组装、专用运行」；Starry 在其上加 POSIX/Linux syscall 层做宏内核（rcore-os 博客：「构建在 ArceOS 模块化内核之上的 Linux 兼容内核」）。

### 3.4 Hubris（重点）：静态任务集 + 会合 IPC + 监督重启
- **任务集编译期定死**（`doc/tasks.adoc`）：所有任务在 `app.toml` 声明，运行时**不能创建/销毁任务**，原话："Task creation is a common source of hard-to-account-for dynamic resource usage… **you can't write a fork-bomb without fork**"。收益：内核任务表可静态化、负载峰值内存**编译期可核算**、任务地址可预测（调试器受益）、数据结构可按固定任务数定容。
- **严格优先级调度**：优先级 0 最高、最多 256 级；高优先级就绪即抢占（中断驱动），**同级不时间片轮转**——文档明说这是刻意选择，同级协作，需要完全抢占就一任务一级。无 MMU：任务隔离靠 MPU + 每任务独立静态内存，代码 XIP 于 Flash、无 loader。
- **会合式同步 IPC**（`doc/ipc.adoc`、`sys/kern/src/syscalls.rs`）：`send` 必阻塞等 reply，接收方必须正阻塞在 `recv` 才发生消息移交（rendezvous）；内核 `send()` 直通投递后返回 `NextTask::Specific(callee)` 直接切到接收者，无队列中转（每任务消息队列满即发送方阻塞——「有界」由会合语义+静态深度保证）。大块数据走 **lease/borrow**（`borrow_read`/`borrow_write`/`borrow_lease` 系统调用）做内核仲裁的内存租借，而非拷贝；轻量路径是 32 位 bitmask **notifications**。接口用 Idol IDL（`idl/`）生成类型安全的 stub。
- **故障三分类 + 监督重启**：硬件 fault / syscall 误用（**直接算 fault 而非 errno**）/ `panic!`。内核不擅自重启：通知监督任务（生产镜像即 `task/jefe`），由其读故障记录、落日志、请求**重初始化**——与开机初始化走**同一条代码路径**：递增 generation 计数、用可识别错误码唤醒所有阻塞在上世代的任务、重置寄存器/定时器、**用魔数涂写栈内存**（humility 据此测每任务真实峰值栈深）、标记 runnable；不重置内存保护配置（本来就没有 API 能改它）。
- **humility 调试器**（MPL-2.0）：因镜像全静态，任务/符号/栈布局离线可知，humility 通过调试口 dump 全任务状态、对 fault 任务做回溯、断言栈深度、现场烧写固件（"From Hubris To Bits", 2024-11）。

### 3.5 四家横评：Rust 写 OS 的工程事实
1. **unsafe 占比**：结论是「收敛可行、归零不可行」。GitHub code search 命中含 `unsafe` 的文件数：Hubris `sys/kern` 仅 14（且 `#![forbid(unsafe_op_in_unsafe_fn)]`）；Theseus `kernel/` 76；Redox kernel 174（含注释/标识符命中）；zCore 实测 unsafe 集中在 HAL（§3.3）。四家一致把 unsafe 压到 arch/HAL/上下文切换层，其上用类型包安全壳——与 BerryOS「安全 Rust 外壳 + 小 unsafe 核」完全同构。
2. **no_std 生态**：MCU 侧（Hubris）生态成熟，直接吃 cortex-m/嵌入式红利；通用 OS 侧各自造 libc：Redox 自写 relibc、Theseus 带 tlibc+wasi_interpreter、zCore Linux 模式复用 musl、Hubris 干脆无 libc（userlib + Idol）。**没有现成的「Rust 通用 OS 底座」，BerryOS 自建 ABI + POSIX 兼容库的路线与此四家一致。**
3. **构建/调试工具链**：Redox（自制 podman 化构建 + redox FayedeMan…实际为自研 make/cargo 混合）、Theseus（Makefile + THESEUS_CONFIG cfg 矩阵）、zCore（cargo xtask）、ArceOS（make + cargo-axplat/axconfig-gen）、Hubris（自研 build 系统 + humility）。四家全都自造工具——OS 工具链没有事实标准，BerryOS 用 cargo+just 是最省力的选择。
4. **「Rust 特有优势」的真与伪**：真——所有权模型做 IPC 句柄移交/租借（Hubris lease、Redox fd 移交、zCore handle 转移）消灭 use-after-free 类内核 bug；类型承载不变量（Theseus）能编译期拦配置错误；内存安全类 CVE 面收敛（四家架构层之外几乎无 unsafe）。伪/夸大——「Rust 内核天然安全」：arch 层 unsafe 仍是事故高发区；「性能更好」无证据（Theseus/Hubris 都明言选简单算法弃快算法）；「生态免费」：通用 OS 的 Rust 用户态生态仍要自己扛（relibc 十年补 POSIX 的痛即证据）。

## 4. 源码走读（真实路径与关键数据结构）

**Hubris** `sys/kern/src/task.rs`——静态任务与重启凭据（本报告核心对照物）：

```rust
#[repr(C)] // so location of SavedState is predictable
pub struct Task {
    save: <ArchImpl as Arch>::SavedState,
    priority: Priority,
    state: TaskState,
    timer: TimerState,
    /// Restart count ... The low bits of this become the task's generation number.
    generation: u32,
    notifications: u32,
    /// Pointer to the ROM descriptor used to create this task, so it can be restarted.
    descriptor: &'static TaskDesc,
}
```

`descriptor` 是 `&'static`——任务的重启说明书在 Flash 里，重启=按描述符重放，无动态分配。IPC 直通投递在 `sys/kern/src/syscalls.rs::send()`：投递成功即 `return Ok(NextTask::Specific(callee))`。

**zCore** `kernel-hal/src/hal_fn.rs` + `macros.rs`——宏生成的 HAL 契约（可 5 行借给 BerryOS `架构相关/`）：

```rust
hal_fn_def! {
    pub mod boot { pub fn primary_init(); }
    pub mod cpu  { pub fn cpu_id() -> u8 { 0 } }
    pub mod mem: common::mem { pub fn phys_to_virt(paddr: PhysAddr) -> VirtAddr; }
    pub mod vm:  common::vm  { pub fn current_vmtoken() -> PhysAddr; }
}
```

`zircon-object/src/ipc/channel.rs`：`Channel { base: KObjectBase, peer: Weak<Channel>, recv_queue: Mutex<VecDeque<T>>, call_reply: Mutex<HashMap<TxID, Sender<ZxResult<T>>>> }`——channel_call 的 reply 用 oneshot 关联，是 BerryOS Phase 3 IPC 的直接参照。权利位在 `object/rights.rs`：`bitflags! { pub struct Rights: u32 { const DUPLICATE = 1<<0; const TRANSFER = 1<<1; const READ = 1<<2; … } }`。

**Redox** `src/scheme/user.rs`——用户态 scheme 的内核侧锚点：

```rust
pub struct UserInner {
    root_id: SchemeId,
    pub scheme_id: SchemeId,
    context: Weak<ContextLock>,
    todo: WaitQueue<Sqe>,
    states: Mutex<L1, Slab<State>>,
}
```

内核内置 scheme 清单见 `src/scheme/mod.rs`（§3.1）；redoxfs 模块面（`transaction.rs`/`htree.rs`/`block.rs`）提示其为事务化 CoW+校验设计（细节待核实）。

**Theseus**：调度入口 `kernel/scheduler/src/lib.rs`（编译期 cfg 选策略，见 §3.2）；在线演化链 `kernel/{crate_swap, fault_crate_swap, ota_update_client, mod_mgmt}`；分配器矩阵 `kernel/{heap, multiple_heaps, slabmalloc, slabmalloc_safe}`——同一 slab 分配器的 safe/unsafe 两版并列，正是其研究方法论的展品。

## 5. 亮点：好在哪（逐条给证据）

1. **Hubris 把「可靠性」做成编译期属性**：无动态任务 + 编译期内存核算 + 重启走同一条 init 路径（`doc/tasks.adoc`），配上近百架生产机柜的出货验证——微内核「失败隔离」承诺第一次有商业规模证据。
2. **Hubris 的监督重启闭环设计精巧**：generation 计数让陈旧句柄可检测、被阻塞方收到可识别错误码、栈涂写免费送峰值栈测量——重启不是重负载的「重来」，是有协议的语义事件。
3. **zCore 证明「Zircon 语义可以安全 Rust 化」且 unsafe 分层纪律真实可测**（syscall 层 `deny(unsafe_code)`、HAL 集中 104 处）——BerryOS 对标 Zircon 务实派，这是最对口的形状参照。
4. **zCore 的 `hal_fn_def!` 宏是极简 HAL 绑定范本**：签名集中声明、实现分散注册、libos 可换底，恰好是 BerryOS「架构无关优先 + `架构相关/<arch>/` 收敛」想要的形态。
5. **ArceOS 的组件化组装**（`modules/ax*` + feature 组合 + Unikraft 式按需裁剪）让「一套组件，多种 OS 形态」（unikernel→宏内核 Starry）被同一代码基证实。
6. **Redox 的 scheme/URL 模型十年不塌**：用户态驱动 + relibc 兼容层 + COSMIC 桌面接入 + RustConf 自托管演示，证明「微内核 + POSIX 面孔」路线能走到可演示的桌面，且 MIT 协议干净可抄。
7. **Theseus 的学术产出扎实**：OSDI'20 论文 + 博士论文 + 可运行的 crate_swap/ota 代码（非纸面设计），其「调度策略=可替换 crate」四件套是 BerryOS 策略可插拔的最直接同类项。
8. **humility 验证了「静态性换取可观测性」**：全静态布局让离线符号/栈深/任务枚举成为免费午餐，生产固件调试有工具闭环。

## 6. 不足与争议：不好在哪（逐条给证据）

1. **Theseus 主仓停更两年**（`theseus_main` 最后 push 2024-09-22），创始人已转 Robius（FOSDEM 2025 简介用过去时）——单 PI 学术 OS 的可持续性风险已成事实；且 aarch64 支持残缺（scheduler crate 仍 x86_64 cfg 门控）。
2. **Redox 十年仍 pre-stable、资源薄**：release 序列 0.3.5(2018)→0.5.0(2019)→0.9.0(2025)，年预算量级 $37k；内核 code search 命中 unsafe 的文件 174 个，规模与 unsafe 面都比 zCore/Hubris 大一截；relibc 长期是兼容性洼地（月报连年修补）。
3. **zCore 主线四年沉寂后仅两次维护性提交**（2026-09），无路线图；async/futures 全内建使内核逻辑与 Rust 异步生态耦合（waker/oneshot 遍布 syscall 层），对教学友好、对移植和调试是负担；`linux-*` 侧 GPL 血统（本地 NOTE.md 已明令禁碰）。
4. **ArceOS 是「组装器」不是「系统」**：README 自认 Working In Progress，Linux 应用兼容、中断驱动 IO、async IO 全是未勾选 TODO；安全模型（无进程隔离的 unikernel 默认形态）与 BerryOS 的能力隔离目标不同层。
5. **Hubris 的前提条件苛刻且许可证收紧**：无动态任务/无 MMU/无时间片/syscall 误用即 fault——对通用 OS 全是反直觉约束；许可从 Apache-2.0 改 **MPL-2.0**（文件级 copyleft），**逐文件复制其代码会带 MPL 义务**，只能借语义；同级协作调度在算力均分场景（BerryOS PC 支柱）不适用。
6. **横评层面**：四家无一家提供 SMP+全抢占+MMU 完整通用内核的全解（Redox 最接近但慢、zCore 无负载验证、Theseus 停更、Hubris/ArceOS 场景错位）——BerryOS 想要的「微内核+省电+AI 调度+PC 多核」组合没有现成模板。

## 7. 对 BerryOS 的借鉴与警示（对照四支柱）

**优先抄谁、抄什么（协议干净度：MIT zCore/Theseus/Redox 可参照代码形状；Hubris 只借机制语义，MPL-2.0 不搬文件；rCore-Tutorial GPL-3.0 只看思路）**：

- 【微内核支柱】**抄 Hubris 的监督重启协议**（机制级）：服务重启=按描述符重放、generation 让老句柄可检测、阻塞方收可识别错误码、重启用同一条 init 代码路径——直接映射 BerryOS「失败隔离：内核提供重启/监督机制」与服务监督服务；配合 BerryOS 关卡 G* 的可验证性。**抄 zCore 的对象形状**：`KObjectBase`/`Rights` bitflags/进程挂 handle 表（Phase 3 能力模型），代码形状 MIT 可对照。**抄 Redox 的用户态 scheme 服务协议形状**（内核侧锚点+请求队列的 UserInner 模式）落到「路径式命名空间」，但不抄其 POSIX-fd 皮肤。
- 【省电支柱】**借 Hubris 的静态预算思想**：手表场景下关键服务的内存/任务上限编译期核算 + 栈涂写式峰值栈测量（廉价、无 instrumentation），服务 tickless idle 之外的「确定性省电」论证。
- 【AI 调度支柱】**借 Theseus 的调度策略并列 crate 形态**但升级为运行时 trait 注册（Theseus 只到编译期 cfg，BerryOS 设计文档的 class_rt/class_fair/class_deadline/class_idle 正是其运行时化版本）；capacity(0..1023) 与 EPP 无先例可抄，属 BerryOS 自研位。
- 【PC 高性能支柱】**参照 ArceOS 的每核 runqueue SMP 讨论**（arceos discussion #181）与 Redox/Theseus 的 context/task 切分，避免单一大锁；Hubris 的「同级不轮转」明确**不抄**——PC 需要公平性。
- 【可观测性】**借 humility 的静态可枚举调试观**：BerryOS 全 Rust+静态符号表，per-CPU 无锁环 + 离线符号化即可复刻其「dump 即全貌」体验。
- 【HAL/架构分层】**抄 zCore `hal_fn_def!` 宏形状**改造进 `架构相关/`（分组签名+按架构注册），并坚持 zCore 式 `deny(unsafe_code)` 上层纪律（unsafe 只落在 HAL/页表/上下文切换，与其 104 处集中于 kernel-hal 的分布同构）。

**警示（明确别学）**：① 不学 Theseus 的全盘 intralingual/在线演化当 v1 目标（学术验证可以，工程主线会被拖死）；其停更教训=文档先行+关卡验证（G0-G5）比单点天才更可靠。② 不学 zCore 的 futures 全内建（Phase 2 用不上，徒增调试面）；`linux-*` GPL 区禁碰（本地 NOTE.md 已定）。③ 不学 Redox 的 relibc 负担与「一切皆 fd」的过度统一（BerryOS v1 ~26 syscall 用 object_* 合并自有 ABI）。④ 不学 Hubris 的「无动态创建/无时间片/syscall 误用即 fault」全套——BerryOS 是动态进程+软实时通用 OS，只取其「监督重启/租借/静态核算」三个机制；且 **MPL-2.0 语义可借、代码不搬**。⑤ 不学 ArceOS 的 unikernel 默认形态（无进程隔离与能力模型冲突）。
**仓库文档修正建议**（本报告不改仓库）：`参考源码/README.md` 中「hubris(Apache-2.0)」应为「MPL-2.0（2021 首发 Apache-2.0 后切换）」；「rCore(MIT/Apache)」应细化为「zCore=MIT 可抄形状，rCore-Tutorial-v3=GPL-3.0 只看思路」，避免后来者误用。

## 8. 参考资料链接

- Redox：https://github.com/redox-os/redox · https://github.com/redox-os/kernel · https://gitlab.redox-os.org/redox-os/redox · https://www.redox-os.org/news/this-month-251231 · https://www.redox-os.org/news/this-month-250630 · https://en.wikipedia.org/wiki/Redox_(operating_system) · https://www.phoronix.com/news/Redox-OS-May-2024 · relibc: https://github.com/redox-os/relibc · redoxfs: https://github.com/redox-os/redoxfs
- Theseus：https://github.com/theseus-os/Theseus · OSDI'20 论文：https://www.usenix.org/system/files/osdi20-boos.pdf · 论文列表：https://theseus-os.github.io/Theseus/book/misc/papers_presentations.html · FOSDEM 2025 讲者页（Boos 转向 Robius）：https://archive.fosdem.org/2025/schedule/speaker/kevin_boos
- rCore 谱系：https://github.com/rcore-os · zCore: https://github.com/rcore-os/zCore · rCore-Tutorial-v3: https://github.com/rcore-os/rCore-Tutorial-v3 · 教材：https://rcore-os.github.io/rCore-Tutorial-Book-v3/ · ArceOS: https://github.com/arceos-org/arceos · Starry: https://github.com/Starry-OS/Starry · 2025 训练营（Starry-Next）：https://opencamp.cn/os2edu/camp/2025spring/stage/6
- Hubris：https://github.com/oxidecomputer/hubris · 官方文档：https://hubris.oxide.computer · 发布博客：https://oxide.computer/blog/hubris-and-humility · 深度解析：https://cliffle.com/blog/on-hubris-and-humility · 烧写实战：https://cliffle.com/blog/from-hubris-to-bits · humility: https://github.com/oxidecomputer/humility · 许可 RFD 讨论：https://news.ycombinator.com/item?id=39181759 · Oxide 出货：https://oxide.computer/blog/oxide-unveils-the-worlds-first-commercial-cloud-computer · B 轮：https://www.futuriom.com/articles/news/oxide-scores-100-million-for-on-prem-cloud-computing-rack/2025/08 · C 轮：https://siliconangle.com/2026/10/09/oxide-computer-raises-445m-to-step-up-data-center-rack-production
