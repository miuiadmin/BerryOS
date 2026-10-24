# BerryOS（中文说明）

**全 Rust 微内核操作系统 —— 省电优先、能力安全、AI 原生。** 首发目标为手表级 ARM64 设备（MB 级内存 rich 档），向手机 / PC 延伸。本仓库为发布镜像；开发主仓（含调研报告与参考实现）为私密仓 BerryOS-private。

## 四根支柱

1. **内核极简**：`#![no_std]` 内核只留调度 / IPC / 地址空间 / 能力派发；文件系统、网络、驱动全部是可崩溃重启的用户态服务。
2. **能力即权限**：Zircon 式内核 handle，一切资源访问经能力句柄传递——服务只能看见被授予的东西。
3. **省电优先**：tickless（NO_HZ）day-one 落地、每任务 EPP hint、用户态电源管理服务（内核↔PM 共享环形缓冲，不走消息拷贝）。
4. **AI 原生调度**：内核留第一类接口让用户态 AI 策略服务干涉放置 / 频率 / 优先级，护栏硬执法（包络夹取、单调收窄）；intent 是第一类调度输入（u8 冻结编码），叠加于公平之上、永不替代。

外加两条工程原则：**AI 友好化**（ABI 单源生成、结构化 tracing、语义错误码）与 **AI 测试友好化**（smoke 断言全部为确定性状态谓词，零时序竞速）。

## 现状（2026-10）

- **Phase 0–6 已收官（ARM64，QEMU virt）**：调度与 PI 捐赠 / VMO+Vmar 地址空间 / 能力 handle 表 + 30 号 syscall（29 实现）/ channel IPC fastpath-slowpath / intent 传播；console+svcmgr 服务框架、RAM-fs、**用户态 virtio-blk / virtio-net 驱动**、FAT12 读写（跨 boot 持久化）、UDP/DNS、最小 TCP 客户端全链 + TCP RPC 服务化；bsh shell（管道 / mini-readline）、**未经修改的 hexyl 0.17.0** 经 musl libc shim 跑通、`.bap` 应用包模型 v0。
- **x86_64 最小集**：PVH 引导 / PIT+PIC / 异常向量 / 上下文切换 / SYSCALL-SYSRET，7 标记挂 CI。
- **现行里程碑 M-VM**：自举磁盘镜像（BerryBoot EFI → handoff）→ 虚拟化窗口 → CLI 可用（uutils）。

## 快速上手

```bash
brew install qemu just                    # macOS
rustup target add aarch64-unknown-none x86_64-unknown-none

just run          # QEMU 冷启动到串口控制台
just run-rootfs   # 完整服务栈：rootfs / FAT / DNS / TCP / .bap 应用
just smoke        # 无人值守五跑关卡：裸相 23 + rootfs 24 + RX 交互 47 + FAT 子目录/持久化相
just test-host    # host 侧 315 单元测试（无需 QEMU）
```

全程 stable Rust（builtin target），免 nightly、免自制工具链。

## 文档

设计文档（中文）全部在 [`docs/`](docs/)，唯一详表 = [`docs/索引.md`](docs/索引.md)。先读：[`docs/架构决策.md`](docs/架构决策.md)（49 条 ADR）→ [`docs/路线图.md`](docs/路线图.md) → [`docs/架构综合纲领.md`](docs/架构综合纲领.md)。

English introduction: **[README.md](README.md)**.

## 许可证

Apache-2.0。第三方系统仅作调研参考——本树不引入任何第三方代码，GPL 实现一律排除。
