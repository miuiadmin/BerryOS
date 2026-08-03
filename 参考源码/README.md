# 参考源码

存放第三方参考实现的源码（克隆下来对照学习用），与自研内核本体（在仓库根目录的 `内核`/`驱动`/… 各目录）**严格分离**。

## 约定

- 每个参考项目单独一个子目录，按项目名命名：`参考源码/redox/`、`参考源码/theseus/`、`参考源码/rcore/`、`参考源码/hubris/`。
- **不要把自研代码放进这里**；这里只放别人的源码。
- 建议在每个项目目录下留一个 `NOTE.md`，记录：协议、对照它学什么、哪些思路借鉴到了自研内核。

## 按子系统对照建议

| 自研子系统（仓库根目录） | 优先对照 | 协议 |
|--------------------------|----------|------|
| 内核 / 进程间通信 / 能力 | **Redox、Theseus、rCore**（Rust）；seL4/Zircon 仅思路 | MIT / Apache |
| 进程调度 | Redox / Theseus 调度；Linux EEVDF/EAS 仅思路 | MIT / Apache |
| 内存管理 | **rCore、Theseus**（Rust 页表/pager）；seL4 思路 | MIT / Apache |
| 架构相关 (ARM64) | rCore-aarch64、Theseus aarch64；Arm ARM 手册 | MIT / 手册 |
| 安全 / 硬件隔离 | ARM spec（PAC/BTI/MTE/SMMU）；seL4（SMMU 思路） | 手册 / 思路 |
| 可观测性 | Fuchsia trace、Linux ftrace（思路） | 思路 |
| 驱动 / 文件系统 / 网络 | **Redox**（redoxfs / 驱动 / 网络，Rust） | MIT |
| 嵌入式 / 极小内核 | **hubris**（Rust 微内核，任务模型） | Apache-2.0 |
| 引导加载器 | Redox bootloader、Limine、OSDev Wiki | — |
| 工具链 | cargo / rust-lld / just / QEMU | — |

## 协议提醒

- 优先 **Rust OS（全 MIT / Apache，语言匹配 + 协议干净）**：Redox(MIT)、Theseus(MIT)、rCore(MIT/Apache)、hubris(Apache-2.0)。
- seL4(BSD) / Zircon(BSD/MIT) 作**思路参考**（C/C++，机制通用，但不复制代码进自研内核）。
- **GPL 项目（Linux、xv6）只看思路，不复制实质代码**，避免协议传染。
