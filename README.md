# BerryOS

[![CI](https://github.com/miuiadmin/BerryOS/actions/workflows/ci.yml/badge.svg)](https://github.com/miuiadmin/BerryOS/actions/workflows/ci.yml)

自研**微内核 + 跨平台操作系统**。Rust 从内核到用户态，ARM64（AArch64）首发，QEMU `virt` 机型验证。

## 四根支柱

| 支柱 | 一句话 |
|------|--------|
| **微内核** | 内核只留调度 / IPC / 地址空间 / 能力；驱动、FS、网络全在用户态，可监督重启 |
| **省电** | tickless（NO_HZ）day-one，空闲核不收周期中断；PM 是用户态服务，与内核走共享环 |
| **AI 原生调度** | 内核留第一类 ABI：遥测出、策略进；AI 只碰 fair/后台类，RT/deadline 永不 |
| **PC 高性能** | 每核 runqueue + 负载均衡 + capacity 抽象，手表→手机→PC 一条放置路径 |

## 现状（诚实版）

- ✅ **Phase 2a / 关卡 G1 已完成**：MMU 身份映射、buddy/slab 堆、异常向量 + panic dump、GICv2 + CNTV timer、SVC syscall 派发、内核线程 + 上下文切换、协作/抢占调度（机制在内核、策略可插拔 trait）、tickless idle（空闲 mask CNTV + `wfi`）、IRQ 驱动抢占（两忙循环任务在 timer slice 下分时，无 yield）。
- ⏭️ **下一步 Phase 2b**：高半翻转 → 每进程页表 + ASID → 匿名 VMO/Vmar → 首个 EL0 用户进程。
- 关卡式验证 G0–G5（`文档/测试策略.md`）：单核正确 → 抢占 → SMP → 全抢占 → demand paging，逐关推进。**尚未到** SMP / 全抢占 / demand paging——文档描述始终与实际阶段一致。

## 快速开始（macOS）

```bash
brew install qemu just                      # qemu-system-aarch64 + just
rustup target add aarch64-unknown-none      # stable，拉预编译 core
just run                                    # QEMU virt 冷启动
```

串口依次打印 `BerryOS boot` → `GIC ok / FRAME ok / HEAP ok / SYSCALL ok` → `PING TASK ok` → `ktest done` → 忙循环任务 `A`/`B` 在 timer slice 下交替，并周期打 `switches=N` 金丝雀。

| 命令 | 说明 |
|------|------|
| `just build` | 构建 ELF（target `aarch64-unknown-none`，产物在全局 `~/.cargo-target`） |
| `just run` | QEMU `virt`，默认起始 EL1 |
| `just run-el2` | `virtualization=on`，走 EL2→EL1 eret 降级路径 |
| `just smoke` | 无人值守冒烟：boot 标记断言 + `switches=N` 严格递增（CI 同款） |
| `just check` / `just clean` / `just test-host` | 检查 / 清理 / host 单元测试 |

退出 QEMU：`Ctrl-A` 然后 `X`（内核 `wfe` 停机后不自行退出）。

## 仓库布局

| 目录 | 内容 |
|------|------|
| `内核/` | Rust `#![no_std]`：调度 / IPC / 地址空间 / 能力 |
| `内存管理/` `进程调度/` `进程间通信/` | Rust `#![no_std]` 内核类 crate |
| `架构相关/aarch64/` | HAL + 内联汇编（EL 检测 / MMU / 异常向量 / 上下文切换） |
| `用户态/` `驱动/` `文件系统/` `网络/` | 用户态服务（Rust） |
| `工具链/` | 构建脚本（cargo + just）/ smoke 脚本 |
| `文档/` | 设计文档 / ADR / 战略裁决 |
| `调研报告/` | 21 份 OS 深度调研（23 个系统：单系统 12 + 进化史/坑史 9） |
| `参考源码/` | 第三方参考实现（独立存放，协议标注见其 README.md） |

## 文档入口

- `文档/路线图.md` —— Phase 0–8，每阶段一个 QEMU 里程碑
- `文档/架构决策.md` —— **ADR #1–#38 单一事实来源**
- `文档/战略裁决-2026-10-15.md` —— 21 份调研 + AIOS 外部调研的双视角对抗裁决（22 提案 / 19 采纳）
- `文档/调度器设计.md` / `AI调度设计.md` —— 机制/策略分离、tickless、AI 护栏
- `文档/测试策略.md` —— 关卡 G0–G5

## 许可证

**Apache-2.0**（ADR #37）。GPL/AGPL 代码不进本仓库；参考实现只借鉴思路，协议洁净是硬约束。
