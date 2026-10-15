# BerryOS

[![CI](https://github.com/miuiadmin/BerryOS/actions/workflows/ci.yml/badge.svg)](https://github.com/miuiadmin/BerryOS/actions/workflows/ci.yml)

自研**微内核 + 跨平台操作系统**。Rust 从内核到用户态，ARM64（AArch64）首发，QEMU `virt` 机型验证。

端定位（ADR #39–#41）：v1 主战场 = **手表/穿戴 rich 档（MB 级内存），ARM64 单线走到底**；MCU 档（KB 级）v1 排除；v2 攻工业 RT；x86_64 提前启动 QEMU 最小集移植（XNU 同源分端模式：同一内核源、分端用户态）。

## 四根支柱

| 支柱 | 一句话 |
|------|--------|
| **微内核** | 内核只留调度 / IPC / 地址空间 / 能力；驱动、FS、网络全在用户态，可监督重启 |
| **省电** | tickless（NO_HZ）day-one，空闲核不收周期中断；PM 是用户态服务，与内核走共享环 |
| **AI 原生调度** | 内核留第一类 ABI：遥测出、策略进；AI 只碰 fair/后台类，RT/deadline 永不 |
| **PC 高性能** | 每核 runqueue + 负载均衡 + capacity 抽象，手表→手机→PC 一条放置路径 |

## 现状（诚实版）

- ✅ **Phase 2b / 出口关卡 G1.5 已完成（2026-10，2b-0~2b-4 全落地）**：内核主体链高半（TTBR1 canonical、低半身份表已退役，boot 打 `KVA ok` 自校验）、每进程页表 + ASID（16 位实测、回卷全量失效）、匿名 VMO/Vmar、首个 EL0 用户进程（ktest `ASPACE ok` / `VMO ok` / `HANDLE ok` / `SYSCALL ok`；用户进程野指针只死自己，坏 handle/未知 syscall 一律错误码返回不杀——watchdog 打 `G1.5 ok`）。其下是 Phase 2a（G1）：buddy/slab 堆、异常向量 + panic dump、GICv2 + CNTV timer、内核线程 + 上下文切换、协作/抢占调度（机制在内核、策略可插拔 trait）、tickless idle（空闲 mask CNTV + `wfi`）、IRQ 驱动抢占。
- 🚧 **Phase 3 能力与系统调用进行中（3-0 handle 表 / 3-1 syscall ABI 骨架 / 3-2 VMO/Vmar 全量门控已落地）**：v1 27 个 syscall 编号在册；handle 三操作 + `clock_read` + **VMO/Vmar 九号全链**（create/commit/decommit/read/write/map/unmap/protect/allocate，rights 位执法 + `copy_from/to_user` 界检 + 悬空 PTE 防线：映射中页拒 decommit、销毁性 close 拒 -EBUSY）；`user_good` 经真 syscall map 读写自己的 VMO（首能力 = root Vmar 经引导表交用户）——Phase 3 里程碑完全体；下一步 3-3 process/thread 最小对象；出口增项 = 遥测环 instrumentation + 空策略表等价测试。
- 关卡式验证 G0–G5（`文档/测试策略.md`）：单核正确 → 抢占 → SMP → 全抢占 → demand paging，逐关推进。**尚未到** SMP / 全抢占 / demand paging——文档描述始终与实际阶段一致。

## 快速开始（macOS）

```bash
brew install qemu just                      # qemu-system-aarch64 + just
rustup target add aarch64-unknown-none      # stable，拉预编译 core
just run                                    # QEMU virt 冷启动
```

串口从 `BerryOS boot` 起，经 `KVA ok`（高半自校验）、`ASPACE ok` / `VMO ok` 到 `G1.5 ok`（用户进程跑飞只死自己）；忙循环任务 `A`/`B` 与 EL0 用户进程（打 `u`）在 timer slice 下交替，并周期打 `switches=N` 金丝雀。完整标记序列以 `工具链/smoke.sh` 为准（`just smoke` 逐一断言）。

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
- `文档/架构决策.md` —— **ADR 单一事实来源**（决策总表 + 重议触发器，编号以该文件为准）
- `文档/战略裁决-2026-10-15.md` —— 21 份调研 + AIOS 外部调研的双视角对抗裁决（22 提案 / 19 采纳）
- `文档/调度器设计.md` / `AI调度设计.md` —— 机制/策略分离、tickless、AI 护栏
- `文档/测试策略.md` —— 关卡 G0–G5

## 许可证

**Apache-2.0**（ADR #37）。GPL/AGPL 代码不进本仓库；参考实现只借鉴思路，协议洁净是硬约束。
