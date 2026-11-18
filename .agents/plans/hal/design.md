# hal（跨架构五窄口、Split-EOI 中断直通与持久黑匣子）总体设计

Type: spec
Status: active
Updated: 2026-11-18
Version: 2.0.0

一句话说明：`hal` 是莓核（Meihe）的跨架构硬件抽象与平台可靠性底座——以零 `unsafe` 契约层 `berry-hal` 统合 `aarch64` 与 `x86_64`，提供 GICv2/GICv3 双栈自适应与 `EOImode=1` 硬件分离式用户态中断直通、SMMUv3 设备 DMA 隔离与缓存维护，以及跨热重启存活的 16KiB `ramoops` 持久崩溃黑匣子。

## 背景

存量硬件层（`arch/aarch64/` 与独立目录 `arch/x86_64/`）实现了 ARM64 双核启动、异常向量、GICv2 MMIO 与 x86_64 PVH 最小五原语自测。但如 [莓核全核深度体检报告](../../research/2026-11-18-莓核全核深度体检报告.md) H1–H6 所示，当前内核与调度器直接硬绑定 `berry-hal-aarch64` 具体结构，`arch/x86_64` 零代码共享；中断控制器硬锁死 GICv2 MMIO，无法在现代 ARMv8.2+ 真机 SoC 或 `gic-version=3` 虚拟化环境启动；缺乏 `Interrupt` 对象导致全部用户态驱动（`vblk`、`vnet`、`gui-inputd`）靠 1–10ms 睡眠轮询破坏 Tickless 铁律；DMA 缺乏连续页、缓存维护与 SMMU 硬件圈定；[cbox 总体设计](../cbox/design.md) 的崩溃记录表驻留普通 `.bss`，一旦发生 EL1 内核异常 `halt()` 或硬件看门狗热复位即随重启清零失联。本规格承接 [meihe 总体设计](../meihe/design.md) 与 [莓核二代终态架构六大核心裁决](../../notes/proposed/2026-11-18-莓核2代终态架构六大核心裁决.md)，定义莓核 2.0 硬件抽象与可靠性底座的完整契约；跨层标准见 [安全与硬件隔离设计 v1.0](../../../docs/安全与硬件隔离设计-v1.0.md) 与 [驱动模型设计 v1.0](../../../docs/驱动模型设计-v1.0.md)。

## 范围

- **覆盖**：`berry-hal` 五大窄 Trait 接口定义及 `aarch64` / `x86_64` 实现映射；ARM64 GICv2 与 GICv3（`ICC_*_EL1` + `GICR` 16 核亲和路由）双栈自适应；基于 GIC `EOImode = 1`（Priority Drop 与 Deactivate 硬件分离）的 `ObjType::Interrupt(8)` 用户态零轮询中断直通；`vmo_cache_op` 缓存一致性与 SMMUv3 Stage-1/2 设备域隔离契约；物理内存顶端 16KiB `ramoops` 持久崩溃区字节布局与跨热重启排液协议；启动帧交接与 EL1 `splash` 瘦身边界。
- **不覆盖**：具体外设（NVMe、USB XHCI、Wi-Fi、GPU）的寄存器协议（全部属于 EL0 用户态驱动包）；EL3 安全固件（TF-A）与独立 TEE OS 内部实现（莓核坚持 TEE 客户端定位）。

## 契约

### 一、`berry-hal` 跨架构五大窄 Trait 宪法

新建纯接口 crate `berry-hal`（`#![no_std]`，本身零汇编、零架构特定寄存器名），`kernel`、`mm`、`sched`、`ipc` 仅依赖 `berry-hal` 导出的关联类型与五大 Trait，彻底解除对 `berry-hal-aarch64` 的直接路径耦合：

1. **`ArchCpu`**：
   - 关联类型 `TrapFrame`（提供统一访问器 `syscall_nr()`、`syscall_args() -> [usize; 6]`、`set_syscall_ret(isize)`、`fault_pc()`、`fault_addr()`、`is_user()`）与 `TaskContext`（含整数 callee-saved、栈指针、TLS 基址与 `fp_needed` 双路径上下文切换）；
   - 原语：`cpu_id() -> u8`、`percpu_base() -> usize`、`disable_irq_save() -> IrqFlags`、`restore_irq(IrqFlags)`、`user_access_begin()` / `user_access_end()`（ARM64 映射 `PAN` 翻转，x86_64 映射 `stac`/`clac` SMAP）、`wait_for_interrupt()`、`halt_forever() -> !`。
2. **`ArchMmu`**：
   - 页表操作：`new_root_table() -> Option<PhysAddr>`、`map_page(root, va, pa, PagePerm, MemAttr)`、`unmap_page(root, va) -> Option<PhysAddr>`、`protect_page(root, va, PagePerm)`、`walk_leaf(root, va)`；
   - 地址空间与缓存同步：`activate_aspace(root, asid, need_tlb_flush)`、`tlb_flush_va_is(asid, va)`、`tlb_flush_asid_is(asid)`、`tlb_flush_all_is()`、`sync_icache_page(pa)`、`dma_cache_op(pa, len, CacheOp)`。
3. **`ArchIrq`**：
   - 原语：`ack() -> Option<IrqToken>`、`priority_drop(IrqToken)`（快速结束活跃抢占优先级）、`deactivate(irq_num: u32)`（物理中断线去活，供用户态驱动处理完毕后回调）、`configure_spi(irq_num, TriggerMode, target_cpu)`、`mask_spi(irq_num)` / `unmask_spi(irq_num)`、`send_ipi(CpuMask, IpiKind)`。
4. **`ArchTimer`**：
   - 原语：`freq_hz() -> u64`、`now_cycles() -> u64`、`arm_oneshot_abs(deadline_cycles: u64)`、`mask_timer()`。
5. **`ArchSmp`**：
   - 原语：`online_cpus() -> u8`、`boot_secondary(cpu_id: u8, mpidr_or_apic: u64, stack_top: VirtAddr)`、`system_off() -> !`、`system_reset_warm() -> !`（PSCI `SYSTEM_RESET2` / `SYSTEM_RESET`，保留 DRAM 内容用于 `ramoops` 自检与看门狗恢复）。

| 抽象面 | `arch/aarch64` 实现 | `arch/x86_64` 实现（接入真 `berry-kernel`） |
|---|---|---|
| 特权级与系统调用 | `EL1h` / `EL0t`；`SVC #0` | `Ring 0` / `Ring 3`；`SYSCALL` / `SYSRET` |
| `PerCpu` 基址寄存器 | `TPIDR_EL1`（用户 TLS 走 `TPIDR_EL0`） | `IA32_KERNEL_GS_BASE`（`swapgs`；用户走 `FSBase`） |
| 内核防误触用户内存 | `SCTLR_EL1.SPAN=0` + `PSTATE.PAN` | `CR4.SMEP=1` + `CR4.SMAP=1`（`stac`/`clac`） |
| 中断控制器 | GICv2（MMIO）与 GICv3（SysReg + GICR）运行时多态 | xAPIC / x2APIC + IOAPIC（PVH 最小集兼容 PIC 回退） |
| 单调时钟与单次定时 | `CNTVCT_EL0` + `CNTV_CVAL_EL0` | Invariant `TSC`（或 `KVMClock`）+ LAPIC `TSC-Deadline` |

### 二、GICv2/v3 双栈自适应与 Split-EOI 用户态中断直通

1. **GICv2 / GICv3 启动期自适应多态**：
   - 启动解析 DTB 中断控制器节点（无 DTB 的固定 QEMU `-kernel` 路径探测 `ID_AA64PFR0_EL1.GIC` bits `[27:24]`）：
     - **`arm,gic-400` / GICv2**：沿用 `GICD` + `GICC`（4KiB 页重映射启用 `GICC_DIR @ +0x1000`）MMIO 访问，最大支持 8 核；
     - **`arm,gic-v3`**：启用 `ICC_SRE_EL1.SRE = 1` 系统寄存器接口；全局分发走 `GICD`（启用 `ARE_NS = 1` 亲和路由，通过 `GICD_IROUTER<n>` 64 位 MPIDR 支持完整 16 核 SPI 定向路由），每核初始化本核对应的 `GICR`（Redistributor：唤醒 `GICR_WAKER.ProcessorSleep = 0` 等待 `ChildrenAsleep == 0`，配置 SGI/PPI 使能与优先级），跨核 IPI 走单条系统寄存器写 `msr ICC_SGI1R_EL1, val`（比 MMIO 写快一个数量级，且原生支持 16 核目标簇掩码）。
2. **皇冠机制：`EOImode = 1`（Priority Drop 与 Deactivate 硬件分离）驱动 `ObjType::Interrupt(8)`**：
   - 传统微内核用户态中断转发若采用 `EOImode = 0`，每次设备中断必须在内核 IRQ 入口写慢速全局 `GICD_ICENABLER` MMIO 关中断（否则写 `EOIR` 后电平型设备线未清立即引发中断风暴），驱动处理完又要再写 `GICD_ISENABLER` 开中断，每次 I/O 多两次跨总线 MMIO 串行写；
   - 莓核统一开启 GICv2（`GICC_CTLR.EOImodeNS = 1`）与 GICv3（`ICC_CTLR_EL1.EOImode = 1`）硬件分离模式：
     1. **内核自有中断（`CNTV`、`SGI_RESCHEDULE`、早期 `UART`）**：handler 读 `IAR` → 清硬件源 → 写 `EOIR`（降运行优先级）+ 立即写 `DIR`（去活物理线）；
     2. **用户态驱动绑定的外设 SPI（`ObjType::Interrupt`）**：
        - 创建与绑定：设备管理服务持设备 `Resource` 句柄调用 `0x71 berry_interrupt_create(res_h, spi_id, trigger_mode, *out_irq_h)`（内核校验该 SPI 未被他人独占认领，配置触发极性并使能），驱动再调用 `0x72 berry_interrupt_bind(irq_h, port_or_event_h, key)`；
        - **中断到达热路径（零 `GICD` MMIO 访问！）**：CPU 陷入 `exception_irq` → 读 `IAR` 获 `spi_id` → 写 `EOIR`（**仅降低本 CPU 当前运行优先级，允许其他高优/同级中断正常抢占；但该 `spi_id` 在 GIC 内部维持 Active 状态，硬件自动抑制该线重投，无需写 `ICENABLER`！**）→ 内核向绑定的 `Port` 压入 `PortPacket(kind = INTERRUPT, payload = cntvct)`（或置位 `Event`）并触发唤醒检查点；
        - **用户态驱动处理与确认**：驱动从 `Port` 醒来（零定时轮询），读写设备 MMIO 寄存器排空中断条件后，调用 `0x74 berry_interrupt_ack(irq_h)` → 内核校验该 `Interrupt` 处于 `PendingAck` 态后，执行单条 `ArchIrq::deactivate(spi_id)`（写 `ICC_DIR_EL1` / `GICC_DIR`），GIC 硬件释放 Active 态并在电平仍高时无缝投递下一拍；
        - **故障自愈**：若持有 `Interrupt` 对象的驱动进程中途崩溃退出，`Interrupt::Drop` 自动在 `GICD_ICENABLER` 屏蔽该 SPI 线，若处于 `PendingAck` 则补发一次 `deactivate` 清净 GIC Active 位，归还池槽供重启后的新代驱动干净重绑。

### 三、DMA 硬件信任后盾：连续锁页、缓存维护与 SMMUv3 骨架

1. **DMA 物理与缓存契约**：
   - `vmo_create` 传 `VMO_OPT_CONTIGUOUS`（需携设备 `Resource` 授权句柄经 `0x73 map_device_memory` 或扩展授权）：从 `BuddyAllocator` 分配物理连续页块，并在 `FrameRefTable` 置 `FRAME_PINNED` 永不换出；
   - 新增缓存维护操作 `berry_vmo_cache_op(vmo_h, op, offset, len)`（`CACHE_CLEAN = 1` 用于 CPU 写完交设备 DMA 读；`CACHE_INVALIDATE = 2` 用于设备 DMA 写完交 CPU 读，要求缓存行对齐否则返 `-EINVAL`；`CACHE_CLEAN_INVALIDATE = 3`），消除真机非一致性外设的数据静默损坏。
2. **SMMUv3 设备域（`DeviceDomain`）隔离契约**：
   - 启动期扫描 DTB `arm,smmu-v3`（或 ACPI `IORT`）：若平台在席 SMMUv3，内核初始化全局命令队列（CMDQ）、事件队列（EVENTQ）与两级流表（Stream Table）；
   - `devmgr` 为用户态驱动授权设备时，按设备 `StreamID` 建立独立 `SmmuDomain`（默认全部未授权 IOVA/PA 拦截阻断）；驱动仅能将自身持 `RIGHT_READ | RIGHT_WRITE` 的 DMA `Vmo` 调用绑定接口注册入本设备 `SmmuDomain` 的 IO 页表；
   - 驱动崩溃或卸载时，`SmmuDomain::Drop` 先拆除流表项（STE 置 Abort）并刷 `CMD_CFGI_STE` + `CMD_TLBI_NH_ASID` 同步完成，**确认硬件停止一切在途 DMA 后**才释放 DMA `Vmo` 的 `FRAME_PINNED` 物理帧，彻底封死用户态驱动经 DMA 改写 EL1 内核或他进程内存的旁路。

### 四、16KiB `ramoops` 持久崩溃黑匣子与启动显示脱核

1. **`ramoops` 物理内存保留区几何与字节契约**：
   - 位置：紧贴可用 DRAM 顶端 `[ram_top - 16KiB, ram_top)`（链接脚本不覆盖、启动汇编 `.bss` 清零跳过、PMM 初始化强制标为 `RESERVED`）；
   - 布局（共 `16,384` 字节，小端 `#[repr(C)]`）：

```text
偏移      宽度     区域              契约说明
0x0000    64B      RamoopsHdr        magic=0x524D4F50 ("RMOP") | ver=1 | boot_seq:u32 |
                                     panic_flag:u32 | cbox_next:u32 | klog_head:u32 |
                                     hdr_crc32c:u32（覆盖前 60B）
0x0040    1664B    CboxRec[16]       扩容至 16 槽的 104B CboxRec（seqlock + 单槽附加校验）
0x06C0    2496B    PanicBacktrace    最近一次 EL1 内核 panic 的完整文本摘要 + 调用栈帧 PC 数组（≤32 层 FP 链回溯）
0x1080    12160B   PersistentKlog    热尾环形内核日志镜像（写 klog 时双写无锁字节槽）
```

   - **跨热重启存活与排液闭环**：
     1. 冷启动（DRAM 随机乱码，`magic` 或 `hdr_crc32c` 失配）：内核将 `RamoopsHdr` 清零初始化，`boot_seq = 1`；
     2. 运行期发生 EL0 故障：同现行 `cbox` 写入 `CboxRec[slot]`；发生 **EL1 同步异常或内核 `panic!`**：关本核中断、通过 `IpiKind::PanicStop` 冻结其余在线核、将寄存器现场写入 `CboxRec`（`kind = KIND_EL1`）、沿 `x29`（FP）在当前内核栈合法边界内安全回溯最多 32 层 `PC` 写入 `PanicBacktrace`、重算 `hdr_crc32c`、执行 `dc cvac` 刷出 CPU 缓存直达物理 DRAM，随后打印串口并根据启动策略执行 `ArchSmp::system_reset_warm()`（开发调试模式可配停机等待 GDB）；
     3. 下一轮热重启进入内核：校验 `magic` 与 `hdr_crc32c` 有效后，先将上一轮残留的 `CboxRec` 与 `PanicBacktrace` 冻结入「上轮遗言快照缓冲」，再递增 `boot_seq`；`svcmgr` 启动后经既有 `0x42 cbox_drain`（及扩展 `telemetry_open` 上轮遗言读口）将上一轮 EL1 内核崩溃精确落盘至 `CBOX.LOG`，宿主 `berry crash-log` 原生符号化输出。
2. **启动画面脱核与逆向依赖出清**：
   - 将点阵字形纯数据表下沉至 `faces/fbfont/`，彻底移除 `kernel -> ../user/berry-fbcon` 与 `sched -> ../user/berry-em` 两条跨层逆向依赖；
   - EL1 内核仅保留极简早期异常文本控制台（崩溃紧急输出用）与 `BerryHandoff` GOP 帧缓冲物理窗口铸造（以 `MMIO VMO` 交付用户态 `guifbd`），原 1,393 行平滑追针与圆角进度条动画迁归早期用户态显示服务。

## 验收标准

1. **双架构同核共编译**：`berry-kernel`、`berry-mm`、`berry-sched`、`berry-ipc` 在零 `#[cfg(target_arch)]`（或仅常量页大小别名）下同时通过 `aarch64-unknown-none` 与 `x86_64-unknown-none` 编译，QEMU `smoke` 与 `smoke-x86` 双线全绿。
2. **GICv3 与零轮询 Tickless 驱动**：QEMU `-machine virt,gic-version=3 -smp 8` 与默认 GICv2 双模式全绿启动到桌面；`vblk`、`vnet`、`gui-inputd` 切换至 `ObjType::Interrupt` Split-EOI 驱动后，系统桌面静置 10 秒期间外设轮询定时器唤醒计数严格为 **`0`**，磁盘与网络高负载压测无丢中断、无中断风暴。
3. **EL1 Panic 跨热重启取证**：在 ktest/故障注入模式下故意触发一次 EL1 空指针异常 → 内核写 `ramoops` 刷缓存并执行 PSCI 热重启 → 第二轮启动至桌面后运行 `berry crash-log --disk <img >`，100% 检出上一轮 `kind=el1` 的精确崩溃 PC、调用栈与 `boot_seq` 记录。

## 适用边界

- 真机冷断电（拔电池/长按硬断电导致 DRAM 掉电放电）会使物理 DRAM 内容自然消散，此时 `ramoops` 靠 CRC32C 校验干净识别为冷启动而不产脏记录；真机掉电级崩溃保留需 SoC 具备 PMIC 保留 SRAM 或 pstore 闪存区，留硬件带板阶段（P8）按同一 `RamoopsHdr` 格式映射。
- QEMU 默认 `virt` 未挂 `-machine virt,iommu=smmuv3` 时，SMMU 驱动探测缺席并打显式降级日志（与 [驱动模型设计 v1.0](../../../docs/驱动模型设计-v1.0.md) 诚实披露口径一致）；CI 新增专门 `-machine virt,iommu=smmuv3` 冒烟腿验证越界 DMA 拦截。
