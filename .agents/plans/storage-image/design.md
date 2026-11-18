# storage-image（现代镜像架构与崩溃原子存储）总体设计

Type: spec
Status: active
Updated: 2026-11-18
Version: 2.0.0

一句话说明：`storage-image` 定义 BerryOS 的下一代系统镜像、只读按需根文件系统（`BerryROFS`）、写时复制崩溃原子数据文件系统（`BerryFS`）、A/B 原子热更新分区以及统一 Rust 构建图编排器（`berry-xtask`）——彻底终结全量内存驻留 `rootfs.cpio`、9 文件 RAM-fs、FAT12 根 8.3 大写限制与串行碎片化构建。

## 背景

[渲染管线与桌面 SDK 及镜像存储深度审计](../../research/2026-11-18-渲染管线与桌面SDK及镜像存储深度审计.md) 指出：现役系统将 30MiB 级 `rootfs.cpio` 整块常驻物理 RAM，严重挤压 128MiB 目标设备可用内存；`fsmgr` 内存树硬限 9 文件×16KiB，持久化层受困于无崩溃日志、无子目录写、无文件删除的 FAT12 根目录 8.3 大写模型；[justfile](../../../justfile) 通过 shell 脚本串行调用近三十次独立 `cargo build` 拖慢迭代。结合 [文件系统设计](../../../docs/文件系统设计-v1.0.md) 与 [更新与 OTA 设计](../../../docs/更新与OTA设计-v1.0.md) 的远景蓝图，本规格立定一步到位的现代端侧存储与镜像 2.0 契约。

## 范围

- **覆盖**：GPT 全生命周期分区与 A/B 双槽热回滚镜像布局；4KiB 页对齐只读按需调页根文件系统 `BerryROFS` 及块级 Merkle 完整性验证（对标 EROFS + dm-verity）；写时复制（CoW）崩溃原子持久化文件系统 `BerryFS` 及每应用不可逃逸 `$appdata` 沙箱与配额执法；`tmpfs` 动态匿名 VMO 内存挂载点；统一 Rust 构建图编排器 `berry-xtask`。
- **不覆盖**：底层 VirtIO-Blk / NVMe 寄存器与中断机制（归 [meihe 总体设计](../meihe/design.md) 与 `devmgr`）；`.bap` 应用清单字段语义（归 [SDK-2 分发与签名链设计](../../../docs/SDK-2-分发与签名链设计-v1.2.md)）。

## 契约

### 一、GPT 标准分区与 A/B 原子不可变系统槽契约

统一虚拟机磁盘镜像（`berry.img`）与真机 eMMC/UFS/NVMe 布局为四分区 GPT 结构（保留引导兼容，废除手工 `dd` 魔数印章拼盘）：

1. **`ESP`（EFI System Partition，FAT32，32MiB）**：仅存放 `EFI/BOOT/BOOTAA64.EFI`（`BerryBoot`）及签名引导槽选择元数据 `boot_control.bin`（含槽位优先级、`tries_remaining`、成功标记 `successful_boot` 与 CRC32C）。
2. **`system_a` 与 `system_b`（只读不可变系统槽，`BerryROFS` 格式，各 64MiB～256MiB）**：存放内核 `meihe.elf`、早期 `rootmgr`、平台守护进程、系统公共矢量字库与预置系统 `.bap` 包。OTA 升级在后台静默流式写入非活跃槽并校验 Ed25519 根签名后原子翻转 `boot_control`，新槽若连续 3 次未走到桌面稳态健康门（`health_commit`），`BerryBoot` 硬件看门狗自动零砖块回滚旧槽。
3. **`userdata`（可写持久化数据区，`BerryFS` 格式，弹性占满剩余空间）**：按结构化四棵能力子树隔离——`/sys/config`（系统设置）、`/app/pkg`（动态安装 `.bap` 只读挂载池）、`/app/data/<app_id>`（每应用独占加密/配额沙箱目录）与 `/user/media`（仅经系统 Picker 一次性句柄授权访问的用户媒体树）。

### 二、`BerryROFS`：4KiB 页对齐只读按需调页与块级验签契约

彻底废除开机将整个 `rootfs0/1.cpio` 读入数十 MB 物理内存的旧路径：

```mermaid
flowchart LR
    ELF["应用/服务代码页或矢量字库页缺页"] --> Pager["rootpager (绑定独立 Pager Port)"]
    Pager -->|"O(1) 计算镜像页偏移"| Blk["vblk 零拷贝 DMA 读入 4KiB 物理帧"]
    Blk --> Merkle["SHA-256 Merkle 分支即时验真\n(缓存中间哈希节点)"]
    Merkle -->|"验真通过：刷 PoU I-Cache + 填只读/可执行 PTE"| VMO["全系统跨进程共享只读物理页"]
```

1. **零拷贝页对齐盘上格式**：`mkrofs`（纯 Rust 产线工具）将所有 ELF 段与大资产严格按 `4096B` 边界对齐排布（小元数据与小图标采用尾块内联 Tail-Packing，可选 LZ4 定长输出块压缩），目录采用紧凑有序哈希数组，路径查找 $O(\log K)$ 零字符串分配。
2. **块级可信度量（Verity Tree）**：镜像尾部内嵌 4KiB 扇区级 SHA-256 Merkle 树，树根哈希（Root Digest）经平台私钥 Ed25519 签名锁入超级块。`rootpager` 每次从块设备调入 4KiB 物理页时先核验对应哈希链，篡改底层磁盘任意 1 比特即刻拒绝供给并触发安全告警。
3. **内存收益硬指标**：系统冷启动常驻根文件系统内存占用从 **> 30MiB** 降至 **< 2MiB**（仅活跃热页驻留物理内存，且标记为干净文件后备页——物理内存吃紧时内核可直接丢弃，下次访问自动从块设备重新调入！）。

### 三、`BerryFS`：写时复制（CoW）崩溃原子文件系统与沙箱配额契约

彻底取代「9 文件×16KiB RAM-fs + FAT12 根目录 8.3 大写整文件覆写」原型（FAT12/FAT32 仅保留作外接移动存储与 ESP 分区兼容驱动）：

1. **B+Tree / Cow-Tree 双超级块原子提交**：
   - 所有目录项、Inode 与块分配位图均采用 Copy-on-Write 树节点更新：修改文件或目录时绝不原地覆写旧元数据块，而是写入新空闲块并向上冒泡更新至交替超级块（`Superblock A / B` 单调递增 `tx_gen` + 屏障刷盘 `FLUSH`）。
   - **断电数学保证**：任意时刻拔电或内核崩溃重启，挂载器选取 `CRC32C` 合法且 `tx_gen` 最大的超级块挂载，盘上状态 **100% 停留在某一完整事务切面**——无需耗时 `fsck`，永不出现半截覆写文件或交叉链接簇！
2. **完整现代 POSIX 文件语义与 Extent 大文件**：
   - 支持最长 `255B` UTF-8 文件名、任意深度多级子目录、真 `UNLINK` 空间即时回收、跨目录原子 `RENAME`（`O(1)` 元数据指针切换，支撑数据库与配置文件安全替换）、硬/软符号链接（受限于沙箱根内）及 64 位单调持久化 `st_ino`；
   - 文件数据采用 `Extent { logical_blk, phys_blk, len_blks }` 连续区段映射，流式随机读写 GB 级媒体/容器镜像无需整文件缓冲进内存。
3. **不可逃逸能力目录句柄与配额硬执法**：
   - 配合 [ipc 总体设计](../ipc/design.md) 的会话与能力边界，`appd` 启动应用时仅向其 `ProcessArgs v2` 注入绑定于 `/app/data/<app_id>` 根 Inode 的独立目录会话句柄；服务端在句柄根边界硬拦截 `..` 上溯，从代数结构上杜绝跨应用偷读数据；
   - 每个 `<app_id>` 子树在 Inode 头维护已用块账本，超出配额立即返回 `-EDQUOT` / `-ENOSPC`，防止单一失控应用写满全盘导致系统无法保存状态。
4. **弹性 `tmpfs`**：内存临时目录 `/tmp` 与 `/run` 改由 `VMO_OPT_DISCARDABLE` 分页支撑，彻底删除 `FILE_COUNT_MAX = 9` 与 `16KiB` 硬限制，容量随系统内存水位受控伸缩。

### 四、统一 Rust 构建编排器（`berry-xtask`）契约

1. **终结碎片工作区重复编译**：将 `user/` 与 `drivers/` 下使用相同目标三元组（`aarch64-unknown-linux-musl`）与相同链接脚本标志的数十个独立微型工作区收敛进统一的 Cargo Workspace（通过 `default-members` 与 feature 精确分层），消除 `berry-sys`、`berry-libc`、`berry-ui`、`berry-gfx` 被重复编译二十余次的历史浪费。
2. **类型化构建图（`cargo xtask`）**：将 [justfile](../../../justfile) 中逾两千行易碎 shell 粘合逻辑收敛为纯 Rust 编排工具 `tools/xtask`（`just` 仅保留面向人类开发者的极薄单行命令别名）：自动并行调度「宿主机纯核测试 → 产线工具构建 → 内核构建 → 用户态统一工作区单次并行构建 → `.bap` 并发打包验签 → `BerryROFS` 与 GPT 磁盘镜像原子组装」，增量修改单个应用或内核文件后重编出镜像全流程耗时压至 **< 3 秒**。

## 验收标准

1. **断电崩溃一致性门禁**：在宿主机故障注入模拟器（随机在第 $N$ 次块设备写操作中间强行截断丢弃后续扇区并重启挂载）下连续执行 10,000 次并发创建、追加写、跨目录 `rename` 与 `unlink` 随机压测，每次重新挂载后文件系统元数据 `100%` 自洽、已确认落盘事务零丢失、空闲块位图零泄漏。
2. **低内存按需启动门禁**：在 `-m 64M`（较现役 128MiB 减半！）严苛内存配额下通过 `BerryROFS` 成功冷启动进入 1080P 图形桌面并正常打开并切换 5 个系统应用；篡改 `system_a` 分区任意 1 字节代码，内核缺页验证立即拦截报错且不执行脏指令。
3. **构建速度门禁**：全量冷构建耗时较串行 shell 基线缩短 **≥ 60%**；单文件增量构建产出可启动镜像耗时 **≤ 3.0s**；产线工具 100% 纯 Rust 化，同输入构建产物 SHA-256 逐位可复现（Reproducible Builds）。

## 适用边界

- `BerryFS` 定位于单机端侧闪存（NVMe / UFS / eMMC / VirtIO-Blk）高可靠低开销存储，不追求数据中心级分布式锁、RAID 阵列或在线去重（Deduplication）特性。
