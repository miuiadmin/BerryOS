# 系统调用 ABI 设计

> 决策依据见 `架构决策.md` #11、#14、#20、#28。本文件给出**具体编号表**（验证规模）、补齐缺口（pager/futex/异常/namespace）、写明 **POSIX 子集边界**。

## 1. 规模：中等偏上（总数 ~30–45，v1 核心 ~26）

内核只暴露：**对象生命周期 + 同步原语 + 基础内存/线程**。
FS / 网络 / 驱动主体**不走 syscall**，走 IPC 到用户态服务（`/svc/fs`、`/svc/net`、`/dev/*`）。

- 审视后修正：原"~20–30"低估了对象丰富度。**总数 ~30–45**，但用 `object_*` 泛型合并（signal/wait/get_info 不按对象类型各开一个），且 **v1 只需 ~26 个**（见末尾阶段化）。
- 不学 seL4 的 4 原语（太极端）；不学宏内核的 100+。

## 2. 调用约定（ARM64）

- 触发：`SVC #0`（EL0 → EL1 同步异常）。
- 寄存器：`x0 = syscall 号`，`x1..x6` 为参数（v1 实际 ≤6 参），`x7` 预留；返回 `x0`（0 成功 / 负错误码，handle 也走 x0）。
- 大结构/缓冲：经 VMO 指针或用户指针（内核 `copy_from/to_user`，带权限/边界校验）。
- **syscall 边界（C-ABI 约定，两端皆 Rust）**：入口汇编 → Rust 内核；两端都用 `extern "C"` + `#[repr(C)]` 封装同 ABI（Rust ABI 不稳定）。
- 快路径**零分配**；全可抢占下注意 syscall 中的可重入与抢占点。

## 3. 能力检查（纯能力的体现）

几乎每个 syscall **首参是 handle**。内核：查 handle 表 → 校验对象类型 + 权限位 → 才执行。**无 handle 无访问**；权限不足返回 `-EACCES`。

## 4. syscall 编号表（草案，分阶段）

> "阶段"列对照 `架构决策.md` #20：v1=单核+抢占点+无 demand paging。

| 号 | syscall | 说明 | 阶段 |
|----|---------|------|------|
| **handle / 通用对象** ||||
| 0x01 | `handle_close` | 关闭 handle | v1 |
| 0x02 | `handle_duplicate` | 复制（权限 ⊆ 原） | v1 |
| 0x03 | `handle_replace` | 原子替换 handle 槽 | v1 |
| 0x04 | `object_wait_one` | 等单个对象上的信号 | v1 |
| 0x05 | `object_wait_many` | 等多个对象 | v1 |
| 0x06 | `object_signal` | 泛型置/清信号（Event/Interrupt/Job） | v1 |
| 0x07 | `object_get_info` | 查询对象信息（topic 分派） | v2 |
| **内存 / VMO / Vmar** ||||
| 0x11 | `vmo_create` | 建内存对象 | v1 |
| 0x12 | `vmo_commit` | 提交物理页（v1 无 pager 时直接分配） | v1 |
| 0x13 | `vmo_decommit` | 归还页 | v1 |
| 0x14 | `vmo_read` | 内核中介读（非映射访问） | v1 |
| 0x15 | `vmo_write` | 内核中介写 | v1 |
| 0x16 | `vmar_map` | VMO 映射进区间 | v1 |
| 0x17 | `vmar_unmap` | 解除映射 | v1 |
| 0x18 | `vmar_protect` | 改权限 | v1 |
| 0x19 | `vmar_allocate` | 建子区间 | v1 |
| **pager** ||||
| 0x21 | `pager_supply_pages` | 用户态 pager 给 VMO 填页 | v2 |
| 0x22 | `pager_op_range` | 脏页/换出通知 | v3 |
| **进程 / 线程** ||||
| 0x31 | `process_create` | 建进程 + 出 root Vmar（双出参；job v1 恒 0） | v1 |
| 0x32 | `process_start` | 启动首线程（entry/sp；bootstrap v1 恒 0） | v1 |
| 0x33 | `process_exit` | 退出 | v1 |
| 0x34 | `thread_create` | 建线程（未启动，入口随 process_start） | v1 |
| 0x35 | `thread_start` | 投入运行 | v1 |
| 0x36 | `thread_exit` | 线程退出 | v1 |
| 0x37 | `thread_sleep` | 睡到 deadline（nanosleep 等价） | v1 |
| 0x38 | `thread_set_priority` | 设优先级（受 SET_POLICY） | v2 |
| 0x39 | `thread_set_deadline` | 设截止期（软实时） | v2 |
| **异常** ||||
| 0x41 | `thread_create_exception_channel` | 用户态异常上报通道 | v2 |
| **IPC / event / futex** ||||
| 0x51 | `channel_create` | 建端点对 | v1 |
| 0x52 | `channel_send` | 发消息 + handle 迁移 | v1 |
| 0x53 | `channel_recv` | 收消息 | v1 |
| 0x54 | `event_create` | 建事件对象 | v1 |
| 0x55 | `futex_wait` | 用户态同步（pthread 基础） | v2 |
| 0x56 | `futex_wake` | 唤醒等待者 | v2 |
| **时间 / 定时器** ||||
| 0x61 | `clock_read` | 读单调/壁钟 | v1 |
| 0x62 | `timer_create` | 建定时器对象 | v2 |
| 0x63 | `timer_set` | 设定时刻 | v2 |
| **设备**（因"用户态为主+关键暂留"留内核） ||||
| 0x71 | `interrupt_create` | 建 IRQ 对象 | v2 |
| 0x72 | `bind_irq` | IRQ → signal 一个 Event | v2 |
| 0x73 | `map_device_memory` | Resource → 设备 MMIO 的 VMO | v2 |
| **命名空间**（路径式） ||||
| 0x81 | `namespace_open` | `open("/svc/fs")` → handle | v3 |
| 0x82 | `namespace_mount` | 服务把 channel 挂到路径 | v3 |
| **策略注入 / 遥测**（AI 调度，见 `AI调度设计.md`） ||||
| 0x83 | `policy_bind` | 注册 AI 策略注入通道（共享环，SET_POLICY） | v3 |
| 0x84 | `telemetry_open` | 打开遥测环（只读） | v3 |

**计数**：v1 ≈ 26；v2 增 ~12；v3 增 ~5（namespace + 策略注入/遥测）；**总数 ~43**。落在"中等偏上 ~30–45"。

> **调试范围（非 ABI）**：`0xFE`（debug echo）/ `0xFF`（debug putchar）= 内核自测/金丝雀槽，**不冻结、不进 bos-sys**，P4 kernellog 落地后退役。真号一律 ≤ `0x85`；未知/未实现号（含调试槽退役后的引用）一律 `-ENOTSUP`——3-1 起内核执法（撤 2b-4「EL0 畸形号杀」临时政策，对齐 §8.0 承诺）。原 0x61/0x62 临时测试槽已让位 `clock_read`/`timer_create`（2026-10，3-1）。

## 5. 错误码

统一 C 风格负数（posix-ish）：`-EINVAL` / `-ENOENT` / `-EACCES` / `-EBUSY` / `-ENOMEM` / `-ETIMEDOUT` / `-EBADF`（坏 handle）/ `-ENOTSUP` / `-EFAULT`（用户指针界检失败，3-2 起 `copy_from/to_user` 执法）。跨 FFI 安全（无异常、无 panic）。

## 6. POSIX 兼容库的子集边界（内核无 fork）

内核 ABI 保持自有形态；POSIX 是其上的一层**用户态兼容库**。明确边界：

| 支持 | 翻译到 |
|------|--------|
| `open/read/write/close` | Channel IPC 到 `/svc/fs` |
| `mmap/munmap/mprotect` | `vmar_map/unmap/protect` |
| `pthread`（mutex/condvar） | `futex_wait/wake` |
| `posix_spawn` | `process_create` + 传 handle |
| `socket` | Channel IPC 到 `/svc/net` |

| **v1 不支持** | 理由 |
|--------------|------|
| `fork()` | 本模型是 `process_create` + 显式能力传递，无"复制整个 AS+fd 表"语义。硬造需 CoW VMO + handle 全复制，工作量大且语义有坑。**v3+ 可选**用 CoW VMO 实现受限 fork。 |
| 信号（signal/kill）传统语义 | 用异常通道 + Event 替代 |

| **明确不做** | |
|--------------|------|
| 跑未修改 Linux 二进制 | 无 Linux syscall 号翻译层；不背这个包袱 |

> 移植 C 程序时优先用 `posix_spawn` / 显式 `process_create`，不要假设 `fork()` 可用。

> **服务协议是第三层 ABI**：channel 的 bytes 内核透传，但 `/svc/fs` 等服务的消息格式对两端是契约——协议头带版本字段、会话首消息协商版本。这层 in-band 可协商，比 syscall 层灵活得多，演进走「新版本号+协商」而非冻结。

## 7. 阶段化目标（v1 基线，对照 `架构决策.md` #20）

- **v1（~26 syscall）**：handle/object + VMO/Vmar 基础 + process/thread 基础 + channel/event + clock_read。足够跑通"首个用户进程 + 能力门控 syscall + 服务间 IPC"。（测试关卡 G0–G2）
- **v2**：pager（demand paging）、futex、timer、异常通道、调度策略、设备 syscall。（G3–G4）
- **v3**：namespace（open/mount）、**AI 策略注入/遥测控制 syscall**（`policy_bind`/`telemetry_open`，见 `AI调度设计.md`）、完整 POSIX 兼容库（含受限 fork 探索）。（G4+）

## 8. v1 ABI 骨架（冻结合约）

编号表是"概念名"。真正落地是带 `bos_` 前缀的符号：**两端皆 Rust**，合约源是 Rust 的 `extern "C"` + `#[repr(C)]` 定义（内核侧 + `用户态/bos-sys` 镜像）；`.h` 是派生产物（供 C 程序 / POSIX 库侧用）。本节是 **v1 的冻结合约**——P3 开工时从单一 IDL（Idol 思路）生成内核原型 + `bos-sys` 镜像 + `syscalls.h`，CI 用结构体布局断言 + bindgen 单向生成比对，消灭手写不同步（见 `架构综合纲领.md`）。

### 8.0 冻结纪律：两层冻结（战略裁决 S8 落地）

> 判例：DOS 四个月拼装的接口被市场冻结成几代人的标准（进化史 14 号）；a.out 被推翻后遗骸支持拖了 27 年才删干净（进化史 13 号）；Linux 靠「对外零 ABI 承诺 + 内部无限重构自由」换得 35 年翻案能力（进化史 13/17 号）。ABI 纪律的核心是**分清哪一层是四十年合同、哪一层必须保持自由**。

| 层 | 范围 | 冻结时点 | 纪律 |
|----|------|----------|------|
| **对外层（四十年合同）** | `bos_*` 符名与签名、`#[repr(C)]` 布局、syscall 编号、错误码数值、rights 位义 | **首个外部消费者出现时**（G-eco 第三方样例或公开发布，先到者） | 编号永不复用（废弃号留坑）；未知/未实现编号一律 `-ENOTSUP`（unknown-op，为 v2/v3 扩展留位，永不 panic）；加字段 = 新 syscall 或 `object_get_info` 新 topic，**不改旧结构**；保留位 v1 即预留：handle 代号位（§8.1）、rights 高位、`options` 高位、`x7` 寄存器 |
| **内部层（显式不稳定）** | 内核内部接口（含 `SchedClass` trait、内核内数据结构、crate 边界 API） | **不冻结**——显式声明「首个外部消费者前不稳定」 | 冻结没人用的接口只锁死自己的重构自由（Linux 反面教训）；消费者真实出现时按需逐条升格入对外层 |

- **镜像同步**：内核侧与 `用户态/bos-sys` 出同一合约源（IDL）生成 + CI 结构体布局断言兜底（见本节引言）——手写两份迟早漂移。
- **预写反转触发器（决策 #14）**：若出现生态压力的**真实证据**（G-eco 第三方样例长期因「非 POSIX / 非 Linux」不可移植、且用户态薄库无解），重开「软件兼容」决策——**只许用户态兼容服务或 VM 内嵌真内核**（WSL2 判例），**内核 syscall 翻译层一票否决**（NT 子系统/翻译层六连败 + WSL1，进化史 14 号）。已录 `架构决策.md` 重议触发器表。

### 8.1 命名与表示决策（带取舍，越早定越好）

| 决策 | 取值 | 取舍 |
|------|------|------|
| 公开 ABI 前缀 | `bos_` | 类比 Zircon `zx_`；防污染用户态命名空间、Rust FFI 易区分。（若偏好 `berry_`/无前缀，全局替换即可，**现在改成本为零**） |
| handle 表示 | `uint32_t`（= 槽位号 + 代号 gen 子字段，决策 #28） | per-process 槽位号 + 单调代号；`0` = 非法。Zircon 同款；不用 seL4 的 64 位 cap 指针。**v1 预留代号位**（即便 DEAD 检测延后启用，避免破坏已发布 ABI） |
| rights 表示 | `uint64_t` 位掩码 | 12+ 位够用且留扩展 |
| 返回/状态 | `int32_t`（0 成功 / 负错误码） | posix-ish，跨 FFI 安全（无异常/panic） |
| 大块数据 | 经 VMO 或用户指针（内核 `copy_from/to_user` 校验） | 不在寄存器里传大缓冲 |
| 布局稳定性 | 跨边界结构 `#[repr(C)]` + 显式对齐 | ABI 一旦发布不破坏布局；加字段走新 syscall 或 `object_get_info` 的 topic |
| options/flags 语义 | 未知位**拒绝**（返回 `-EINVAL`） | 输入严格拒绝，演进走「新 flag + 能力发现」。「静默忽略」看着兼容性好，实则让日后启用该位前必须先清点全生态是否有人误传——等于永久放弃那个位 |
| 结构体版本化 | 尺寸进出（`in_size` / `out_size`） | v2 `object_get_info` 的 info 结构是首个尺寸敏感 ABI：读方容忍更大尺寸、写方未知尾部清零；缓冲不足返回 `-EINVAL` 并回写所需尺寸（Win32 `cbSize` / Linux size-in-out 同款惯例） |
| padding / 保留字段 | 显式命名 + 内核 copyout 前整体清零 | `#[repr(C)]` **不保证** padding 清零；垃圾 padding = 内核信息泄露（Linux 真实 CVE 先例）+ 破坏逐字节比较/哈希 |

> 32 位 handle 分配（决策 #28）：低位 = 槽位号，中位 = 代号 gen（**v1 预留**，槽位重用时单调递增，供 DEAD 检测，见 `能力模型设计.md` §2），高位**可选**编码对象类型（调试器识别越类 handle，v1 可先不编码）。代号位**必须从 v1 预留**——否则日后加 gen 会破坏已发布的 handle ABI。

### 8.2 基础类型（`bos/types.h`）

```c
#include <stddef.h>
#include <stdint.h>

typedef uint32_t bos_handle_t;
typedef uint64_t bos_rights_t;
typedef int32_t  bos_status_t;   /* 0 = 成功 / 负 = -E* 错误码 */
typedef uint64_t bos_time_t;     /* 纳秒；单调钟 / 壁钟由 clock_id 选 */

#define BOS_HANDLE_INVALID  ((bos_handle_t)0)

/* 对象类型（handle 高位可选编码用；也供 get_info/object_signal 分派） */
enum bos_obj_type {
    BOS_OBJ_PROCESS = 1, BOS_OBJ_THREAD, BOS_OBJ_VMO, BOS_OBJ_VMAR,
    BOS_OBJ_CHANNEL, BOS_OBJ_EVENT, BOS_OBJ_EVENTPAIR, BOS_OBJ_INTERRUPT,
    BOS_OBJ_RESOURCE, BOS_OBJ_TIMER, BOS_OBJ_JOB,
};

/* rights 位（对照 `能力模型设计.md` §2） */
#define BOS_RIGHT_READ        (1ULL << 0)
#define BOS_RIGHT_WRITE       (1ULL << 1)
#define BOS_RIGHT_EXECUTE     (1ULL << 2)
#define BOS_RIGHT_MAP         (1ULL << 3)
#define BOS_RIGHT_DUPLICATE   (1ULL << 4)
#define BOS_RIGHT_TRANSFER    (1ULL << 5)
#define BOS_RIGHT_INSPECT     (1ULL << 6)
#define BOS_RIGHT_SIGNAL      (1ULL << 7)
#define BOS_RIGHT_SEND        (1ULL << 8)
#define BOS_RIGHT_RECV        (1ULL << 9)
#define BOS_RIGHT_MANAGE      (1ULL << 10)
#define BOS_RIGHT_SET_POLICY  (1ULL << 11)
#define BOS_RIGHT_SAME_RIGHTS (-1LL)   /* duplicate 时取全集的哨兵 */

/* 错误码（posix-ish 负数；跨 FFI 不抛异常） */
#define BOS_OK                 0
#define BOS_ERR_BAD_HANDLE     (-9)    /* EBADF 等价：坏 handle / 类型不符 */
#define BOS_ERR_DEAD           (-52)   /* 代号作废：对象/服务已死（旧代 handle），决策 #28 */
#define BOS_ERR_INVALID_ARG    (-22)   /* EINVAL */
#define BOS_ERR_NO_MEMORY      (-12)   /* ENOMEM */
#define BOS_ERR_ACCESS_DENIED  (-13)   /* EACCES：权限位不足 */
#define BOS_ERR_NOT_FOUND      (-2)    /* ENOENT */
#define BOS_ERR_BAD_STATE      (-37)   /* 对象状态不对（未 start / 已关） */
#define BOS_ERR_TIMED_OUT      (-110)  /* ETIMEDOUT */
#define BOS_ERR_BUSY           (-16)   /* EBUSY：channel 满 / 资源占用 */
#define BOS_ERR_NOT_SUPPORTED  (-95)   /* ENOTSUP：阶段未实现 */
#define BOS_ERR_OUT_OF_RANGE   (-34)   /* ERANGE：offset/len 越界 */
#define BOS_ERR_FAULT          (-14)   /* EFAULT：用户指针界检失败（3-2 起 copy_from/to_user 执法） */
```

### 8.3 v1 syscall 原型（`bos/syscalls.h`）

> 调用约定：ARM64 `SVC #0`，`x0`=syscall 号，`x1..x7`≤6 参，返回 `x0`。**用户态不直接写编号**，调这些 `bos_*`（`extern "C"` Rust 函数，`bos-sys` 提供，内部封 `svc`）；编号只在内核 dispatch 表里用。

```c
/* —— handle / 通用对象 —— */
bos_status_t bos_handle_close(bos_handle_t h);
bos_status_t bos_handle_duplicate(bos_handle_t h, bos_rights_t rights,
                                  bos_handle_t *out);            /* rights ⊆ 原 */
bos_status_t bos_handle_replace(bos_handle_t h, bos_handle_t replacement,
                                bos_handle_t *out);              /* 原子换槽 */
bos_status_t bos_object_wait_one(bos_handle_t h, uint64_t signals,
                                 bos_time_t deadline, uint64_t *observed);
bos_status_t bos_object_wait_many(const bos_handle_t *handles,
                                  const uint64_t *signals, size_t n,
                                  bos_time_t deadline,
                                  size_t *idx, uint64_t *observed);
bos_status_t bos_object_signal(bos_handle_t h, uint64_t clear, uint64_t set);

/* —— VMO / Vmar —— */
bos_status_t bos_vmo_create(uint64_t size, uint32_t options, bos_handle_t *out);
                               /* v1：options 须 0；size ∈ (0, 16MiB]，超限 -ENOMEM（no-panic 防线） */
bos_status_t bos_vmo_commit(bos_handle_t vmo, uint64_t offset, uint64_t len);
bos_status_t bos_vmo_decommit(bos_handle_t vmo, uint64_t offset, uint64_t len);
                               /* 映射中页 -EBUSY（悬空 PTE 防线，先 unmap） */
bos_status_t bos_vmo_read (bos_handle_t vmo, void *buf,
                           uint64_t offset, size_t len);          /* 内核中介读 */
bos_status_t bos_vmo_write(bos_handle_t vmo, const void *buf,
                           uint64_t offset, size_t len);          /* 内核中介写 */
bos_status_t bos_vmar_map(bos_handle_t vmar, uint32_t options,
                          bos_handle_t vmo, uint64_t vmo_offset,
                          uintptr_t *vaddr, size_t len, bos_rights_t rights);
                               /* v1：options 恒 0——bos-sys stub 折叠为内核 6 参形态
                                  （x1..x6：vmar/vmo/vmo_offset/*vaddr/len/rights）；
                                  rights 须恰为 READ|WRITE（v1 只支持 UserRw，缺/多位
                                  -ENOTSUP/-EINVAL，UserRo 留 v2 AP=11） */
bos_status_t bos_vmar_unmap(bos_handle_t vmar, uintptr_t vaddr, size_t len);
bos_status_t bos_vmar_protect(bos_handle_t vmar, bos_rights_t rights,
                              uintptr_t vaddr, size_t len);
bos_status_t bos_vmar_allocate(bos_handle_t vmar, uint32_t options,
                               uintptr_t *vaddr, size_t len,
                               bos_handle_t *child);

/* —— 进程 / 线程 —— */
/* 3-3 修正（发布前，v1 落地实测）：加 root Vmar 第二出参（Zircon
   zx_process_create 同形）——父进程必须拿到子地址空间根才能映代码/栈，
   单出参 *out 时子进程无法被装配。job v1 恒 0（Job 对象 v2）。 */
bos_status_t bos_process_create(bos_handle_t job, uint32_t options,
                                bos_handle_t *process, bos_handle_t *vmar);
bos_status_t bos_process_start(bos_handle_t process, bos_handle_t thread,
                               uintptr_t entry, uintptr_t sp,
                               bos_handle_t bootstrap);  /* v1 恒 0：per-process
                               handle 表落地（3-4+）前无「移入子进程固定槽」
                               载体，子入口 x0=0 */
bos_status_t bos_process_exit(int code);
bos_status_t bos_thread_create(bos_handle_t process, uint32_t options,
                               bos_handle_t *out);        /* 未启动描述；入口/sp
                               随 process_start（首线程启动路径，Zircon 同形） */
bos_status_t bos_thread_start(bos_handle_t thread);
bos_status_t bos_thread_exit(void);
bos_status_t bos_thread_sleep(bos_time_t deadline);     /* nanosleep 等价 */

/* —— IPC / event —— */
bos_status_t bos_channel_create(bos_handle_t *out0, bos_handle_t *out1);
bos_status_t bos_channel_send(bos_handle_t ch,
                              const void *bytes, size_t nbytes,
                              const bos_handle_t *handles, size_t nhandles);
bos_status_t bos_channel_recv(bos_handle_t ch,
                              void *bytes, size_t *nbytes,
                              bos_handle_t *handles, size_t *nhandles);
bos_status_t bos_event_create(uint32_t options, bos_handle_t *out);

/* —— 时间 —— */
bos_status_t bos_clock_read(uint32_t clock_id, bos_time_t *out);
```

**v1 计数 = 27**（handle 6 + VMO/Vmar 9 + 进程线程 7 + channel/event 4 + clock 1），落在「~26」预算内。`object_get_info / pager / futex / timer / 设备 / namespace` 全部留到 v2/v3。

### 8.4 Rust FFI 镜像（`用户态/bos-sys`，raw 绑定）

raw FFI 只暴露 `extern "C"` + 裸类型，**绝不 panic**；上层 `bos` crate 把 `bos_status_t` 包成 `Result<(), Status>`。

```rust
//! v1 raw syscall 绑定。两端 Rust `#[repr(C)]`，布局与内核侧一一对应；`.h` 为派生产物。
#![no_std]

pub type Handle = u32;
pub type Rights = u64;
pub type Status = i32;   // 0 = Ok, 负 = 错误码
pub type Time   = u64;

pub const INVALID_HANDLE: Handle = 0;

extern "C" {
    pub fn bos_handle_close(h: Handle) -> Status;
    pub fn bos_handle_duplicate(h: Handle, rights: Rights, out: *mut Handle) -> Status;
    pub fn bos_handle_replace(h: Handle, replacement: Handle, out: *mut Handle) -> Status;
    pub fn bos_object_wait_one(h: Handle, signals: u64, deadline: Time,
                               observed: *mut u64) -> Status;
    pub fn bos_object_wait_many(handles: *const Handle, signals: *const u64, n: usize,
                                deadline: Time, idx: *mut usize, observed: *mut u64) -> Status;
    pub fn bos_object_signal(h: Handle, clear: u64, set: u64) -> Status;

    pub fn bos_vmo_create(size: u64, options: u32, out: *mut Handle) -> Status;
    pub fn bos_vmo_commit(vmo: Handle, offset: u64, len: u64) -> Status;
    pub fn bos_vmo_decommit(vmo: Handle, offset: u64, len: u64) -> Status;
    pub fn bos_vmo_read(vmo: Handle, buf: *mut u8, offset: u64, len: usize) -> Status;
    pub fn bos_vmo_write(vmo: Handle, buf: *const u8, offset: u64, len: usize) -> Status;
    pub fn bos_vmar_map(vmar: Handle, options: u32, vmo: Handle, vmo_offset: u64,
                        vaddr: *mut usize, len: usize, rights: Rights) -> Status;
    pub fn bos_vmar_unmap(vmar: Handle, vaddr: usize, len: usize) -> Status;
    pub fn bos_vmar_protect(vmar: Handle, rights: Rights, vaddr: usize, len: usize) -> Status;
    pub fn bos_vmar_allocate(vmar: Handle, options: u32, vaddr: *mut usize, len: usize,
                             child: *mut Handle) -> Status;

    pub fn bos_process_create(job: Handle, options: u32,
                              process: *mut Handle, vmar: *mut Handle) -> Status;
    pub fn bos_process_start(process: Handle, thread: Handle, entry: usize, sp: usize,
                             bootstrap: Handle) -> Status;
    pub fn bos_process_exit(code: i32) -> ! ;
    pub fn bos_thread_create(process: Handle, options: u32, out: *mut Handle) -> Status;
    pub fn bos_thread_start(thread: Handle) -> Status;
    pub fn bos_thread_exit() -> ! ;
    pub fn bos_thread_sleep(deadline: Time) -> Status;

    pub fn bos_channel_create(out0: *mut Handle, out1: *mut Handle) -> Status;
    pub fn bos_channel_send(ch: Handle, bytes: *const u8, nbytes: usize,
                            handles: *const Handle, nhandles: usize) -> Status;
    pub fn bos_channel_recv(ch: Handle, bytes: *mut u8, nbytes: *mut usize,
                            handles: *mut Handle, nhandles: *mut usize) -> Status;
    pub fn bos_event_create(options: u32, out: *mut Handle) -> Status;

    pub fn bos_clock_read(clock_id: u32, out: *mut Time) -> Status;
}
```

约定（同 `CLAUDE.md` FFI 边界）：
- Rust 侧**不持有跨边界的引用生命周期假设**，缓冲一律 `*mut u8`/`usize`（拥有/复制语义）。
- 改任何签名 = **同时**改合约源（Rust `#[repr(C)]` 定义）与镜像；CI 用结构体布局断言 + bindgen 单向生成 `.h` 后比对（两端 `#[repr(C)]` 保证布局一致）。
- `process_exit` / `thread_exit` 标 `-> !`（不返回），Rust 侧据此消除后续代码不可达警告。
- **未知 syscall 号 = `-ENOTSUP`（`BOS_ERR_NOT_SUPPORTED`），派发器永不 panic**——v1 内核遇到 v2/v3 号段即此行为，属成文承诺（syscall 边界无 panic 纪律的派发器侧落点）。
- 所有 `options`/flags 参数：内核**拒绝未知位**；已知位的语义只增不改（见 §8.1 表）。
- 内核填任何跨边界结构体前整体清零（含 padding），`copy_to_user` 不携带内核栈残留。

### 8.5 ABI 稳定性行政策（何时冻结、冻结什么、冻结后怎么改）

**分界线只有一条：有没有「你不控制、不会重编译」的二进制依赖它。**

| 阶段 | 状态 | 规则 |
|------|------|------|
| 现在 ~ G1（尚无外部用户态消费者） | **UNSTABLE** | §8 可自由修改——这是把错误改对的唯一窗口，别怕改；但 gen 位、类型宽度这类「日后补=破坏 ABI」的预留（§8.1）从第一版就位 |
| G2（首个用户进程 + `bos-sys` 首次发布给内核外程序） | **FROZEN(v1)** | 冻结范围见下，此后 add-only |
| 之后所有版本 | FROZEN 累积 | 只增不改；deprecated 条目保留不删 |

**冻结的是形状**：syscall 编号与段分配、寄存器约定（`x0`=号 / `x1..x7`=参 / 返回 `x0`）、基础类型宽度（handle u32 / rights u64 / status i32 / time u64 纳秒）、错误码**数值**、跨边界结构体布局。

**不冻结的是语义外围**：§6 POSIX 映射表（API 层，随便改）、错误码文字描述、行为细节——**bug 修复 / 性能改进 / 时序变化不算破坏 ABI**（承诺的是「二进制能继续跑」，不是「行为一字不差」，Linux 同款口径）。

冻结后的修改流程：

1. 改 ABI = **新增**（新号 / 新 flag / 新 topic / 尺寸更大的结构体版本），PR 注明 `abi: add-only`；
2. 禁止：重排编号、复用已发布号、改字段含义或偏移、挪错误码数值；
3. 执法靠 CI：布局断言 + bindgen 单向生成比对——**在 CI 挂，不在用户设备上挂**；
4. 想推翻设计 = 发新号并行（`xxx` → `xxx2`，Linux `clone`→`clone3` 同款），旧的进 deprecated 清单但永不删除（Zircon 200/201 合并退出快路径即「性能优化=新增号而非改旧语义」的先例）。

> 一句话：**承诺给二进制的，永远别动；讲给源码听的，随便改。**

## 9. 参照

- 中等 ABI + 对象/handle + 编号风格：Zircon `zx_` 系（我们会精简，且用 `object_*` 合并）。
- 能力检查首参、编号紧凑：seL4（思路）。
