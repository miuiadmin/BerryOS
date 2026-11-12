# AGENTS.md

> 跨工具通用（Cursor/Copilot/Codex/Claude Code…）。规则单一事实源 = 本文件：只载当前状态 + 铁律路由。
> 规则正文 → `docs/规则/`；判例全文 → `docs/雷册-判例库.md`；文档详表 → `docs/索引.md`。
> 沿革/里程碑流水 → `docs/阶段沿革.md` 末尾追记，勿回写本文件。预算：≤200 行 / ≤3400 字符（`tools/docs_guard.py` 执法）。

## 身份与原则

- **BerryOS**：自研微内核 + 跨平台 OS + 全 Rust（内核 Meihe（莓核）`#![no_std]` + 用户态服务）。
  ARM64 首发，QEMU virt 主验；x86_64 最小集验收。
- v1 主战场 = 移动设备 rich 档 → PC；穿戴同族可落，MCU 档排除；v2 攻工业 RT。
- 当前站 → `docs/路线图-v1.0.md`；85 ADR → `docs/架构决策.md`。
- 身份支柱：微内核 · 省电 · AI 原生调度 · PC 高性能。
  三原则：人无我有/人有我强（先过排除 8 问，证据库 `research/`）· AI 友好化 · AI 测试友好化（G0–G5）。
- 微内核五原则：内核极简 · IPC 中枢 · 能力即权限 · 硬件隔离 · 失败隔离。展开 → `docs/架构综合纲领.md`。

## 冻结常量（全表 → `docs/架构决策.md`）

- 内核/权限：Zircon 务实派 + 纯能力（内核态 handle）+ VMO/用户态 pager/每进程 AS；syscall ~30–45（v1 35 在册，C-ABI）。
- IPC/FS/驱动：同步 channel + 异步通知 + 共享内存 + 路径命名空间；FS/网络/驱动用户态，启动关键暂留内核；FS MVP = FAT，实时 = 软实时。
- 调度：机制在内核、策略可插拔 + tickless；省电/多核双线；intent（u8）第一类输入（#42 两段式，#58 六档词汇表；公平默认、只叠加）。
- AI 原生调度：机制预留（带内接口，给用户态策略）+ 实验轨道 + kill criteria。
- 并发/基线：细粒度锁 + 全可抢占（分阶段）；v1 单核 + 抢占点 + demand paging 机制面已启（G4，ADR #20 分阶段爬升兑现）+ 最小能力/IPC。
  启动 QEMU `-kernel` + EFI；逐版推向满配（G0–G5）。
- 软件：自有 ABI + POSIX 兼容库（不跑 Linux 二进制，**v1 无 fork**）。

## 动代码前（先读域铁律）

**unsafe 全域**：只进页表/上下文切换/MMIO/分配器/硬件原子，**外层包安全 API**——`tools/unsafe_audit.py` 同口径。
路由表只列高危桶；全表（GUI/AI 调度/参考引入/x86/引导等）→ `docs/规则/README.md`。

| 动什么 | 先读 |
|---|---|
| 内核编码 / 内存页表 / VMO | `规则/01-内核编码.md` · `规则/02-内存与页表.md` |
| syscall / ABI / berry-sys | `规则/03-系统调用.md` |
| 调度 / IPC / 信号 / SMP | `规则/04-调度与IPC.md` |
| 常驻服务 / FS / FAT 写路径 | `规则/05-服务监督.md` |
| 用户程序 / 编译链 / .bap / rootfs | `规则/06-用户程序与编译链.md` |
| smoke / 测试 / bench | `规则/07-测试与冒烟.md` |
| QEMU / 中断 / 设备 / x86 / 构建 / 引导 | `规则/08`–`12`（`docs/规则/` 下同名前缀） |
| 引入参考 / 加依赖 | `规则/13-工作流程与协议.md` |

## 仓库布局

```
kernel/ mm/ sched/ ipc/            Rust no_std 内核本体（调度/IPC/地址空间/能力）
arch/                              HAL + 内联汇编（aarch64/ 现役 + x86_64/ 最小集）
bootloader/ drivers/ fs/ net/      EFI 引导 / 用户态驱动工作区 / 用户态服务
user/ tools/                       应用生态与 berry-sys/libc / 构建脚本（just + smoke）
docs/ research/ reference/         文档 / 调研报告（仅私密仓）/ 参考实现（仅私密仓，协议隔离）
.agents/                           Curia 文档工作区
```

## 常用命令（全表 → `docs/命令速览.md`）

高频：`just build` / `build-user` / `run` / `run-rootfs` / `smoke` / `test-host` / `abi-check`。
再加 `smoke-image`；其余（x86/SMP/bench/EFI 镜像/设备壳/soak）→ `docs/命令速览.md`。
路径解析单源 = `tools/paths.sh`。
TCG 下 CNTV tick 实测 ≈ `SLICE_HZ/2`（非内核 bug；详 `规则/08-QEMU与仿真.md` / 雷册 §A8）——读切换率别被 2× 误导。

## 维护合同

- 改约定（新架构/语言边界）→ 回写本文件；新 just 配方 → `docs/命令速览.md`（高频才补 AGENTS 一行）。
- 阶段进展/标记沿革 → 只写 `docs/阶段沿革.md`。
- 新判例 = 域文件瘦行 + 雷册对应节全文 + `判例索引.md` 补行，三处一动（契约 → `docs/规则/README.md`）。
- 入口预算/探针/锚点/迁出目标由 `tools/docs_guard.py` 执法：AGENTS ≤200 行 / ≤3400 字符。
  行长 >100 警 >120 拒；CLAUDE 仅 `@AGENTS.md` 指针。
- 新文档落库/标签更新 → `docs/索引.md`；写前先查 `reference/` 对照实现。小步提交：能在 QEMU 独立验证的里程碑单独成提交。
- 文档工作区 Curia（一事实一归属）：规格 → `.agents/README.md`。
- **文档版本**：设计/实施/规划/调研/验收类统一 `名-v主.次.md`（如 `xxx-v1.1.md`），
  文件名+抬头双标；升版改名并同步索引/引用；存量下次大改迁移。细则 → `docs/索引.md`。
