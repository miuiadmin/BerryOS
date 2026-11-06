# SEC-6 业界调研：运行期硬件安全（IOMMU/SMMU · PAC/BTI · MTE · CFI）

> 调研日期：2026-10-29。纪律：每条硬事实带来源编号 [n]；【证】= 一手来源可查（含本机 QEMU 11.0.3 实验），【推】= 笔者推断。业界现状与「对 BerryOS 的启示」严格分开（§5 才是启示）。BerryOS 既有设计见 `docs/安全与硬件隔离设计-v1.0.md`，本文不复述。

## 1. 概述：本维度解决什么问题、信任边界在哪

运行期硬件安全 = **用 CPU/芯片组里的硬件机制，在运行期为 OS 的三大信任边界兜底**：

1. **DMA 边界**（设备 ↔ 内存）：设备可绕过 MMU 直接写物理内存。IOMMU/SMMU 把「设备能写哪些物理页」变成 OS 可编程的硬件约束——没有它，「用户态驱动 + 纯能力」的模型不成立：恶意/被攻破的设备固件可以 DMA 改写内核【证】[3][16]。
2. **控制流边界**（数据攻击 → 代码复用）：内存破坏类漏洞的终段利用是 ROP/JOP。PAC（指针签名，ARMv8.3）签返回地址/函数指针，BTI（ARMv8.5）标记合法间接分支落点——硬件级 CFI 补丁【证】[20][22]。
3. **数据访问边界**（UAF/越界）：MTE（ARMv8.5）给每 16B granule 配 4-bit tag，访问时硬件校验——软件 tag 检查（ASan/KASAN）的硬件化【证】[21][23]。

信任边界的关键点：这三样都是**机制在硬件、策略在 OS**——OS 决定 domain 装什么映射、哪些进程开 sync MTE、哪些页标 BTI。业界教训集中在两处：**「硬件存在 ≠ 默认生效」**（Pixel 11 把硬件 MTE 用软件关掉 [39][40]；Linux 曾长期 DMA passthrough [29]）与「开了 ≠ 开对了模式」（async MTE 检出精度差 [21][23]；IOMMU non-strict 模式留 stale TLB 窗口 [29]）。

## 2. 各家现状对照表（2026-10）

| 厂商/OS | DMA 隔离 | PAC/BTI | MTE / 内存 tag | CFI（编译器面） | 现状一句话 |
|---|---|---|---|---|---|
| **Android（Google/Pixel）** | 内核 arm-smmu/v3 驱动管理（DMA strict 默认推荐 [29]） | 内核开 PAC/BTI（GKI 配置面 [29]；用户态默认不开 PAC，GrapheneOS 补齐 [34]）【推：具体机型代际】 | Pixel 8 首发（2023）开发者开关 [24]；Android 16 Advanced Protection 模式开启 [38][42]；内核侧 KASAN_HW_TAGS 自 GKI Android 12 默认 [23] | 内核 CFI GKI 默认（≤6.0 LTO-CFI，≥6.1 KCFI 免 LTO）[27][28] | 三件套最全，但用户态 MTE 默认仍保守 |
| **Apple iOS/macOS** | 不外置 IOMMU 编程面（自家 SoC 集成管理）【推】 | arm64e PAC 自 A12/iOS 12 day-one（2018）[34][35]；PACMAN 证明可被推测执行绕过 [57] | iOS 26 + A19/M5 起 MIE：EMTE（=FEAT_MTE4）**严格同步、默认常开、无用户开关**，盖内核 + 70 余 userland 进程；第三方 app 经 Xcode opt-in [36][37][38] | 自家编译器 + PAC 组合 | 业界最激进：同步 MTE 当默认防线 |
| **Linux（通用）** | VFIO（设备直通）/ IOMMUFD（新用户态接口，2023+）[55]；KSPP 推荐 DMA_STRICT 禁 passthrough [29] | ARM64_PTR_AUTH(_KERNEL)/ARM64_BTI(_KERNEL) 配置面就绪 [20][29]；发行版才开始默认开（Debian 2025 讨论 [50]） | ARM64_MTE 用户态 per-进程 opt-in（PROT_MTE + prctl）[21]；服务器侧几乎未部署【推】 | CONFIG_CFI（原 CFI_CLANG）主线化 [51] | 机制全在主线，**默认值是各家自己的事** |
| **Fuchsia/Zircon** | 内核一等公民：`zx_iommu_create`（类型 STUB=0/INTEL=1/ARM_SMMU=2）→ IOMMU 对象 → `zx_bti_create`（设备 DMA 身份）→ `zx_bti_pin` 钉 VMO 页拿物理地址（PMT）[16][17][18] | 未闻系统级启用【推】 | 未闻【推】 | — | 用户态驱动 OS 的 DMA 接口范本（BerryOS 最直接参照） |
| **seL4** | SMMU 支持存在且被社区视为关键（TK1 时代 SMMU 代码 + 邮件列表「IOMMU 提供统一机制执行内存访问策略」）[47][48] | 形式化验证路线为主，硬件缓解非卖点【推】 | — | — | 依赖 IOMMU 做非 CPU 发起的内存访问隔离 |
| **QNX 8** | smmuman 服务（host/guest 两级）：hypervisor 用它保证 pass-through 设备出不了授权区；qvm 无 smmuman 时**拒绝**接 DMA 设备（fail-closed）[46] | — | — | — | 安全认证世家的「IOMMU 管理器 = 独立服务」形态 |
| **Windows 11** | Kernel DMA Protection（Thunderbolt/热插拔即圈即用）[45] | — | — | 内核 CET shadow stack（依赖 VBS/HVCI 先行）；HVCI = 虚拟化保护的内核代码完整性 [43][44] | 路线 = 用 hypervisor 隔离内核而非改内核语言 |
| **DPDK/SPDK（用户态 DMA 框架）** | **强依赖 VFIO+IOMMU**：vfio-pci 绑定 + IOMMU group 全员就位才可用；无 IOMMU 时 `no-iommu` 模式明确「无隔离无保护」[11][12][13][14] | — | — | — | 用户态高性能栈 = IOMMU 的最大工业客户 |
| **物联网（Zephyr/TF-M）** | 多数 MCU 无 IOMMU（禁外设 DMA 或 MPU 兜底）【推】 | TF-M 构建提供 branch protection 开关（SPE 侧）[49b] | 无 | — | 无 MMU/IOMMU 的端靠「不开 DMA」+ 编译器缓解 |

## 3. 机制细节（到「能照着留接口位」的精度）

### 3.1 SMMUv3 编程模型（spec IHI 0070）

- **查表链**：设备发出带 **StreamID** 的事务 → 索引 **Stream Table**（支持线性 / 2 级，STE = Stream Table Entry）→ STE 指向 **Context Descriptor (CD) 表**（带 SubstreamID/PASID 时按其选 CD）→ CD 装载 **TTBR0/TTBR1 + TCR + MAIR**（页表格式与 CPU AArch64 共享）【证】[1][2][3]。
- **two-stage translation**：Stage-1 = VA→IPA（驱动/guest 拥有页表），Stage-2 = IPA→PA（hypervisor 拥有，VMID 隔离）——与 x86 EPT/NPT 同构；VFIO 场景下 QEMU 虚拟 SMMU 做 S1（IOVA→IPA）、物理 SMMU 做 S2（IPA→宿主 PA）【证】[3][4][5][6b]。
- **队列**：命令队列 CMDQ（软件 invalidate/config 命令入队）、事件队列 EVQ（翻译 fault 上报）、PRI 队列（页请求）。**ATS/PRI 是 PCIe 侧配套**：ATS = 设备先查翻译缓存（ATC），PRI = 设备对缺页发起页请求；SMMUv3 的 IDR0 位域披露支持与否【证】[1][2]。Linux 侧 SMMUv3 的 PRI/IOPF 支持到 2026 年还在补课（NVIDIA 系 patch 在途）【证】[15b]。
- **安全语义要点**：SMMU 也在做「权限」——CD/STE 里的可读写位 + stage-2 顶端拦截 = 设备只能碰被授权区间；QEMU 的 `arm-smmuv3.stage` 属性可只保留 stage-2【证】[8]。
- **学术风险注记**：IOMMU 本身也是攻击面（SMMU 实现漏洞 [53]、IOMMU 在场仍可 DMA 攻击的系统性研究 [56]）——「圈住 ≠ 完事」，fault 上报与 strict 模式是配套必选。

### 3.2 QEMU 可验性（本机 QEMU 11.0.3 / macOS brew 实测，2026-10-29）

- **SMMUv3 模拟存在但仅 PCI**：`-machine virt,iommu=smmuv3` 挂到 PCIe 根复合体；非 PCI（sysbus/virtio-mmio）设备不被翻译，2022 年起明确，2026-01 才有 RFC patch 补 sysbus【证】[6][7]。**BerryOS 主路径是 virtio-mmio——QEMU 侧 SMMU 现在盖不住它**。
- **virtio-iommu 是跨架构替代**：OASIS VIRTIO 1.3 设备类型 23，请求队列 ATTACH/DETACH/MAP/UNMAP/PROBE + 事件队列报 fault；QEMU `-device virtio-iommu-pci` 本机实例化成功【证】[9]（本机实验）。
- **设备挂接形态**（本机实测）：`-machine virt,iommu=smmuv3 -cpu max` + `virtio-net-pci,disable-legacy=on,iommu_platform=on,ats=on` 可启动接受；**默认 transitional 设备直接拒**（`VIRTIO_F_IOMMU_PLATFORM was supported by neither legacy nor transitional device`）——必须 modern-only【证】[10][11]（本机实验）。
- **CPU 特性**：官方 emulation 文档列 FEAT_PAuth/PAuth2/EPAC…、FEAT_MTE/MTE2/3/4、FEAT_BTI 均为 TCG 支持，`-cpu max`（=max-v9）全开；cortex-a72（ARMv8.0）全无【证】[33]。本机实测：`-cpu max,pauth=on` 接受；`-cpu max,mte=on`/`bti=on` 报 property not found（该版本 max 模型不单独暴露这些布尔属性），但 **`-machine virt,mte=on` 接受**——MTE 的 tag 存储是**机器级选项**，不是 CPU 位【证】（本机实验）。
- 结论：BerryOS 计划的「切 `-cpu max` 开 PAC/BTI/MTE」在 QEMU 11.x 可验，MTE 还需 `-machine virt,mte=on`。

### 3.3 Zircon 的 DMA 三对象（BerryOS DMA VMO 的直接同构物）

`zx_iommu_create(type, desc)` 创建 IOMMU 内核对象（类型常量 `ZX_IOMMU_TYPE_STUB=0`、`_INTEL=1`、`_ARM_SMMU=2`，desc 里含寄存器基址/PCI scope）→ `zx_bti_create(iommu_handle, bti_id)` 为每个设备造 **BTI（bus transaction initiator）= 设备的 DMA 身份** → `zx_bti_pin(bti, perms, vmo, offset, count, &addrs)` 把 VMO 页**钉住并回给物理地址**，返回 PMT 句柄，`zx_pmt_unpin` 解钉；`ZX_BTI_CONTIGUOUS` 要求 VMO 本身连续分配【证】[16][17][18]。Fuchsia 在 ARM 板（Khadas VIM3/Amlogic A311D）上实跑验证 [19]；ARM SMMU 类型常量在头文件中在册但生产成熟度低于 Intel 路径【推】[16]。

### 3.4 Linux VFIO/IOMMUFD 一句话对照

VFIO = 设备直通框架（组内全部设备绑 vfio-pci 后用户态可 mmap BAR、经 container API 装载 IO 页表）；IOMMUFD（2023 合入主线）是其接替者：一切 IOMMU 子系统操作走 fd，显式 ioas/hwpt 对象模型【证】[55]。DPDK 的全部用户态 DMA 安全性都押在这上面；无 IOMMU 的 `no-iommu` 模式官方自述「无设备隔离、无 DMA 翻译、无内核保护」【证】[12][14]。

### 3.5 MTE：模式、接口与 Android 启用面

- **硬件/内核接口**：16B granule : 4-bit tag；`PROT_MTE`（mmap/mprotect）开启页 tag，`prctl(PR_SET_TAGGED_ADDR_CTRL)` 选 per-thread 检查模式 NONE/SYNC/ASYNC（多 bit 可并置、内核择一）；sync = 精确 SIGSEGV(SEGV_MTESERR)，async = 延迟死（SEGV_MTEAERR、无地址），asymm = 读 sync 写 async（v8.7）；`/sys/devices/system/cpu/cpuN/mte_tcf_preferred` 全局偏好；tag 不可事后撤（drop 到 NONE 单向）【证】[21]。
- **Android 进程级开关**：`memtag_heap`（async）/`diag:{memtag_heap}`（sync）构建面；manifest `android:memtagMode`；系统属性 `arm64.memtag.process.<name>`；`mallopt(M_MEMTAG_TUNING, BUFFER_OVERFLOW|UAF)`——官方口径 UAF 调优下「空间+时间 bug 合计约 93% 检出概率」【证】[23][25]。
- **启用前提**：bootloader 必须预留 tag 存储（内存布局级决策，后补不了）——Pixel 8 开启 = 两条 property + reboot【证】[24]。
- **内核侧**：`CONFIG_KASAN_HW_TAGS`（= MTE 硬件 tag 的 KASAN），**GKI 内核自 Android 12-5.10 起默认开**，boot 参数 `kasan.mode=sync/async`、`kasan.fault=report/panic`【证】[23][31]。
- **性能实测**：Google 团队 2026 论文（Pixel 8/9、AmpereOne、含 M5 初析，SPEC + RocksDB/Nginx/PostgreSQL/Memcached）：多数负载开销不大，但个别基准慢达 **6.64x**，并指认微架构根因、称此前 MTE 性能表征多有方法学错误【证】[26]。GrapheneOS 生产口径：内核 sync + 用户态 asymm、开销「very low」、全 app 开启的阻碍是 app 潜在 bug 而非性能【证】[38]。
- **KASAN vs KFENCE vs MTE**：KASAN 三模式（generic 软件红区 / SW_TAGS 软件打 tag / HW_TAGS=MTE 硬件 tag）[31]；KFENCE = 采样守卫页、近零开销，**与 HW_TAGS 互斥**（MTE 在场即禁用 KFENCE，boot 期强制）【证】[32]——「硬件 tag 全量」与「软件采样」是二选一的部署档位。

### 3.6 PAC/BTI：内核与用户态

- **内核**：`CONFIG_ARM64_PTR_AUTH(_KERNEL)` 用 PAC 签内核返回地址/函数指针；`CONFIG_ARM64_BTI_KERNEL` 开内核 BTI；KSPP arm64 清单在册（与 SHADOW_CALL_STACK 二选一——`UNWIND_PATCH_PAC_INTO_SCS` 把无 PAC 硬件的场景退到软件影子栈）【证】[20][29][52]。
- **用户态**：BTI 是**映射级 opt-in**——`mprotect(PROT_BTI)`（arm64 上 PROT_BTI=0x10）+ ELF note `GNU_PROPERTY_AARCH64_FEATURE_1_BTI` 由动态链接器自动打；不标的页不受 BTI 约束。内核启用路径 `bti_enable()` 置 SCTLR_EL1.BT0（QEMU 用户态 patch 里逐字可查）【证】[22]。
- **JIT 相容性**：因 BTI 是 per-page opt-in 而非进程全局，JIT 页**不标 PROT_BTI 即天然豁免**——Android/Chromium 的策略是 AOT/native 代码开 BTI、JIT 代码页不开，无需专门「冲突处理协议」【推】（由 [21][22] 的 per-page 语义推出）。
- **Android 实态**：stock Pixel 内核开 PAC（+BTI），用户态二进制默认不开 PAC——GrapheneOS 是把用户态 PAC/BTI/MTE 全补齐的第三方实现，并公开其 CFI 姿态【证】[34]。发行版侧 Debian 2025 年才讨论默认开 PAC/BTI【证】[50]。
- **PAC 不是银弹**：PACMAN（CVE-2022-26700，M1）证明可用推测执行侧信道绕过签名校验【证】[57]。

### 3.7 Apple MIE / Windows VBS（对照组的「另一条路」）

- **Apple**：iOS 26（2025）MIE 基于 **EMTE = FEAT_MTE4**（与 Arm 共研的标准化扩展，非私有），A19/M5 起硬件含 tag 存储，**严格同步、默认常开、无用户开关**，盖内核 + 70 余关键进程；Apple 立场：「防护必须同步、默认、持续」——明确否定 async 的竞态窗口【证】[36][37][38]。
- **Windows**：VBS 用 Hyper-V 建 VTL1 隔离区跑 HVCI（内核代码完整性）与 Credential Guard；内核 CET shadow stack 依赖 VBS/HVCI 先行；外设面是 Kernel DMA Protection（热插拔设备在解锁时被 DMA 圈禁）【证】[43][44][45]。
- **Android kernel CFI 演进**：2018 年上 LTO+CFI（当时构建时间代价大）→ 6.1+ 切 KCFI 免 LTO → GCC CFI 落地后配置项改名 CONFIG_CFI【证】[27][28][51]。

### 3.8 BerryOS 视角的 QEMU IOMMU 选用决策（两案对照）

| | 方案 A：`-machine virt,iommu=smmuv3` | 方案 B：`-device virtio-iommu-pci` |
|---|---|---|
| 形态 | 机型级 SMMUv3 模型，挂 PCIe 根复合体 | 标准 virtio 设备（类型 23），PCI 传输 |
| 覆盖 | 仅 virtio-**pci** modern 设备（须 `disable-legacy=on` + `iommu_platform=on`）【证】[6][10]（本机实验） | 同样要求子设备 `iommu_platform=on` 挂接【证】[11] |
| 与 BerryOS 主路径关系 | 主路径 virtio-**mmio** 不被翻译（sysbus RFC 在途 [7]）——须开一条 virtio-pci 支线 | 同为 PCI 面；但协议层是 virtio（跨架构、真机/模拟同构） |
| 接口形态收益 | 学的是 SMMUv3 硬件编程面（STE/CD/CMDQ）——对真机 v2 直接对口 | 学的是 IOMMU 抽象协议（ATTACH/MAP/UNMAP）——对内核 devmgr 接口设计更贴 |
| 本机实测 | `-machine virt,iommu=smmuv3` 接受；modern-only virtio-net-pci + iommu_platform + ats 组合可启动【证】（本机实验） | `-device virtio-iommu-pci` 在 virt 机型实例化成功【证】（本机实验） |

两案不互斥：内核侧 domain 抽象（map/unmap/fault）做成一套，底下挂「模拟 SMMU 直编」或「virtio-iommu 协议」两个后端——与 Zircon 把 IOMMU 类型做成 desc 参数 [16] 同构。

## 4. 演进史与教训（哪些 day-one、哪些补课）

1. **IOMMU：Linux 是典型补课史**。先靠 bounce buffer/swiotlb（软件拷贝）苟了十年 → VT-d/SMMU 主线化 → VFIO（直通）→ IOMMUFD（2023，重造用户态 API）。教训一：**固件交接窗口**——Intel 专门出白皮书要求 UEFI 在 boot 期就圈 DMA（KSPP 对应 `CONFIG_EFI_DISABLE_PCI_DMA`），否则「内核起来之前设备随便写」【证】[29][54]。教训二：**strict 模式之争**——non-strict（延迟 invalidate）性能好但留 stale 映射窗口，KSPP 现推荐 `IOMMU_DEFAULT_DMA_STRICT` 禁 passthrough【证】[29]。
2. **MTE：Android 三步走、Apple 一步到位**。Android 12（2021）挑高风险平台组件 async 试点 → Pixel 8（2023）首台 MTE 手机、开发者开关（要求 bootloader 预留 tag 内存）→ Android 16（2025-26）Advanced Protection 模式整体开启 [23][24][38][42]。Apple 直接押「同步 + 默认常开」（A19 硬件协同设计，含防 tag 推测泄漏）[36][38]。教训：**tag 内存预留是启动期单次决策**（Pixel 8 靠 bootloader 参数），「以后再开」对已出货设备不成立。
3. **Pixel 11 事件（2026-08/09，本维度最新教案）**：Tensor G6 出厂**软件层禁用 MTE**，GrapheneOS 判「硬件几乎肯定删了」并宣布拒移植、公开劝退；随后用户从 Google 支持确认硬件仍在、9 月新固件恢复 MTE 支持【证】[39][40][41][58]。教训：**硬件安全特性可被厂商软件策略静默撤下**——OS 层必须有能力探测 + 上报 + 断言面（而不是默默降级），生态（ROM/企业）会把「特性在册」当采购条件。
4. **CFI：构建成本驱动的形态迁移**。LTO-CFI（2018，构建慢）→ KCFI（6.1+，免 LTO）→ FineIBT（x86）与 CONFIG_CFI 改名——「安全特性若拖慢开发循环，就一定会被绕开」【证】[27][51]。
5. **检测器档位化**：KASAN 全量（调试）→ KFENCE 采样（生产可开）→ MTE HW_TAGS（生产常开、与 KFENCE 互斥）——运行期检测从「调试构建专属」进化为「生产默认档位」【证】[31][32]。
6. **QEMU 生态同步补课**：SMMUv3 模型 2017-2018 进主线但至今仅 PCI，virtio-mmio/sysbus 支持是 2026 年的 RFC——**「真机有 SMMU」与「模拟器能验 SMMU」之间存在结构性时差**；virtio-iommu 作为纯 virtio 方案是当下唯一全模拟路径【证】[6][7][9]。
7. **默认值的分化是常态，不是落后**：GKI 内核 KASAN_HW_TAGS 默认开 [23] vs 通用 Linux 发行版 MTE/PAC 默认面 2025 年才开始铺 [50] vs Apple sync 常开 [36]——同一硬件特性在三类生态里处于三个部署档位。原因：Android/Apple 有垂直整合的 app 兼容性兜底（per-app opt-in/compat 模式 [23][38]），发行版没有——**默认开硬件检查的前提是「有对受影响组件的分级与豁免机制」**，这个机制本身是 OS 侧工程，不是硬件给的。

## 5. 对 BerryOS 的启示（v2/v3/v4 落点 + day-one 留缝）

### 5.0 关键数字速查（正文对应处见来源）

- MTE granule = 16 字节配 4-bit tag（1/16 内存开销，tag 存储需 bootloader 预留）[21][24]
- `mallopt(M_MEMTAG_TUNING, UAF)`：官方口径约 93% 空间+时间 bug 合计检出率 [23]
- MTE sync 实测：多数负载温和、个别基准最高 6.64x 减速（Pixel 8/9、AmpereOne、M5）[26]
- Apple MIE：内核 + 70 余 userland 进程 sync 常开、无用户开关（A19/M5 起）[37][38]
- Android 内核 MTE 检测（KASAN_HW_TAGS）自 GKI Android 12-5.10 默认；boot 参数 `kasan.mode/fault` [23]
- arm64 用户态开关位：`PROT_BTI`=0x10、`PROT_MTE`=0x20（mmap/mprotect flag）[21][22]
- Zircon IOMMU 类型常量：STUB=0 / INTEL=1 / ARM_SMMU=2（`zx_iommu_create` desc 上限 4096B）[16]
- KFENCE 与 KASAN HW_TAGS 互斥（MTE 在场即让位）[32]

> 以下为对照建议（非业界事实）。BerryOS 既有规划：v2 SMMU+PAC/BTI/MTE+devmgr、v3 度量链、v4+ 签名（`docs/安全与硬件隔离设计-v1.0.md` §7）。

**v2 落点**

1. **DMA VMO 接口照 Zircon 三对象抄形状**：内核对象三件 = IOMMU 实例 / 设备 DMA 身份（BTI 同构 = 驱动 RPC 里的 device id + devmgr 授权）/ 钉页-回物理地址（PMT 同构 = `vmo_pin_range(vmo, off, len, perms) -> PMT handle`，`perms` 显式带 R/W 位——SMMU CD 权限位的直接来源）。`zx_bti_pin` 的「perms 参数 + 连续性约束 + unpin 句柄」三要素值得逐字镜像 [16][18]。
2. **SMMU domain 默认 strict、默认无 passthrough**（KSPP 教训 [29]）；**fault 必须上报**（EVQ 同构 = BerryOS 异常通道里预留 IOMMU fault 消息类型），且 devmgr 对「无法编程的 DMA 设备」应学 QNX **fail-closed 拒绝接入** [46]。
3. **QEMU 验证路径现在就定**：主路径 virtio-mmio 无 SMMU 模拟（2026 RFC 在途 [7]），v2 的 IOMMU 冒烟走 `-machine virt,iommu=smmuv3` + **modern-only virtio-pci**（`disable-legacy=on,iommu_platform=on,ats=on`，本机已验证组合）或 `-device virtio-iommu-pci`——意味着 v2 需要一条 virtio-pci（或 virtio-iommu 感知的驱动侧）测试支线，专供安全面验证，不扰主冒烟。
4. **PAC/BTI 切 `-cpu max` 的两步**：内核先开 BTI（成本最低：内核自包含、无 ABI 影响），PAC 后开（需过一遍「syscall 边界返回值/函数指针签名」审计，kernel 文档的 userspace ABI 注意事项 [20]）；用户态映射留 PROT_BTI 同构位（见 day-one 留缝 2）。
5. **MTE 走 async 先行、内核 slab 优先**（业界共识顺序 [23][38]）；`-machine virt,mte=on` 是 QEMU 侧前置条件（本机验证），justfile 的 v2 跑法要带上。

**v3 落点**

6. 用户态高价值服务（网络栈/媒体解析类，若届时存在）sync MTE——对标 Android「高风险组件 sync」与 Apple「关键进程 sync 常开」的中间路线 [23][36]。
7. IOMMU fault 遥测进结构化 tracing（与 `可观测性设计.md` 合流），对标 smmuman 的 incident 记录面 [46]。

**v4+ 落点**

8. 度量/签名面之外，运行期档位策略（哪些进程 sync MTE）做成**系统策略服务可编程**——Android 用 Advanced Protection 一个模式统一拉闸 [42] 的做法优于散装开关。

**day-one 留缝（现在就埋，防「后补≠自动生效」）**

- **缝 1（vmo_pin/PMT 语义进 VMO）**：DMA VMO 的 `CONTIGUOUS + 锁页 + perms` 三属性进 vmo_create/对象结构（v2 只需「锁页+连续」就能用 bounce 式安全，IOMMU 后补映射层）——Pixel 8 教训的反面：**内存布局类决策（tag 存储、连续保留区）boot 期一次定生死**，对象模型先留位，硬件后到也能平滑接上。
- **缝 2（vmar_map rights 留 BTI/MTE 位）**：映射权限枚举预留 per-page 硬件特性位（BTI 落点标记、PROT_MTE 同构），哪怕 v1 全部 no-op——BTI 的 per-page opt-in 语义 [22] 决定这是映射 API 的字段而非编译开关。
- **缝 3（能力探测 + 断言面）**：boot 期 CPUID/ID Register 探测 PAC/BTI/MTE/SMMU 在位性，打机器可读行进 smoke（对标 Pixel 11 事件 [39]：特性缺席要 loud，不许静默降级）；devmgr 的设备授权协议里带「该设备是否可被 IOMMU 圈禁」字段，不可圈禁的 DMA 设备默认拒发证。
- **缝 4（内核堆分配器中心点）**：slab alloc/free 单一入口（为 MTE 打 tag 与 KFENCE 式采样守卫页留宿主点）；unsafe 面集中清单（PAC 签名审计的对象目录）。
- **缝 5（libc 分配器可换档）**：用户态 berry-libc 的 malloc 若自研/封装，留「分配器 tag 钩子」编译开关位（对标 musl 侧 `memtag_heap` 走 `mallopt` 的接入点 [23]）——Rust 用户态主语言 + unsafe 边界（驱动/编解码）正是 MTE 的目标面，与 Apple「Swift 为主、native 补硬件防线」同构 [36]。

## 6. 来源清单

[1] ARM SMMUv3 架构规范 IHI 0070（PDF 镜像）: http://kib.kiev.ua/x86docs/ARM/SMMU/IHI_0070_A_SMMUv3.pdf
[2] Arm Developer: SMMUv3 support（2 级 stream table / 2 级 CD 表约束）: https://developer.arm.com/documentation/100310/0201/functional-description/constraints-and-limitations-of-use/smmuv3-support
[3] openEuler 博客：Introduction to IOMMU and ARM SMMU（两阶段翻译 S1/S2）: https://www.openeuler.org/en/blog/wxggg/2020-11-21-iommu-smmu-intro.html
[4] even629: ARM SMMUv3（hypervisor 管 STE/S2、guest 管 CD/S1）: https://even629.com/en/posts/2510303
[5] OpenVMM Guide: Arm SMMUv3（nested domain）: https://openvmm.dev/guide/reference/emulated/iommu/smmuv3.html
[6] qemu-devel 2022-05：SMMU 模型仅支持 PCI 设备: https://mail.gnu.org/archive/html/qemu-devel/2022-05/msg04521.html
[6b] qemu-devel patch v7（hw/arm/smmuv3，vSMMU S1 + 物理 SMMU S2）: https://patchew.org/QEMU/1504286483-23327-1-git-send-email-eric.auger@redhat.com/1504286483-23327-21-git-send-email-eric.auger@redhat.com
[7] qemu-devel 2026-01 RFC：SMMU 支持 sysbus 设备（virtio-mmio 例）: https://lists.gnu.org/archive/html/qemu-devel/2026-01/msg03697.html
[8] StackOverflow：arm-smmuv3.stage 属性（stage-2 only）: https://stackoverflow.com/questions/78513210/where-is-the-property-arm-smmuv3-stage-to-support-stage-2-translation-in-qemu
[9] OASIS VIRTIO 1.3 规范（virtio-iommu 设备类型 23）: https://docs.oasis-open.org/virtio/virtio/v1.3
[10] libvirt 邮件列表：iommu=on / ats=on 属性语义: https://lists.libvirt.org/archives/list/users@lists.libvirt.org/message/HLVU6YCYBGXFYUKNWDNBR6B4S6DJOBAX
[11] Kata Containers issue：iommu_platform 使用形态: https://github.com/kata-containers/runtime/issues/2830
[12] DPDK 文档：Linux Drivers（VFIO 依赖与 IOMMU group）: https://oss.iol.unh.edu/dpdk/future-dts/-/blob/f3b26e3a161d3d12e70ce19f1579ec1d4e6ed444/doc/guides/linux_gsg/linux_drivers.rst
[13] DPDK 用户邮件列表：IOMMU group 未绑齐报错: https://mails.dpdk.org/archives/users/2022-August/006650.html
[14] DPDK dev 邮件列表：VFIO no-iommu 模式（无隔离）: http://mails.dpdk.org/archives/dev/2015-December/030501.html
[15] DPDK dev：IOMMUFD 支持补丁: https://mails.dpdk.org/archives/dev/2023-December/284452.html
[15b] Linux patchew：iommu/arm-smmu-v3 PRI/IOPF 支持（2026 在途）: https://patchew.org/linux/cover.1789081084.git.nicolinc@nvidia.com
[16] Zircon 源码 iommu.h（ZX_IOMMU_TYPE_STUB/INTEL/ARM_SMMU 常量与描述符）: https://fuchsia.googlesource.com/fuchsia/+/main/zircon/system/public/zircon/syscalls/iommu.h
[17] Fuchsia 文档：Bus Transaction Initiator 内核对象: https://fuchsia.dev/fuchsia-src/reference/kernel_objects/bus_transaction_initiator
[18] Zircon 文档：zx_bti_pin（ZX_BTI_CONTIGUOUS 语义）: https://fuchsia.googlesource.com/zircon/+/refs/heads/sandbox/voydanoff/xhci/docs/syscalls/bti_pin.md
[19] Fuchsia 文档：Khadas VIM3（Amlogic A311D）开发指南: https://fuchsia.dev/fuchsia-src/development/hardware/khadas-vim3
[20] Linux 内核文档：AArch64 Pointer Authentication: https://docs.kernel.org/arch/arm64/pointer-authentication.html
[21] Linux 内核文档：Memory Tagging Extension（PROT_MTE/prctl/四模式/sysfs）: https://docs.kernel.org/arch/arm64/memory-tagging-extension.html
[22] QEMU patch（aarch64-linux-user 启用 SCTLR_EL1.BT0，含内核 PROT_BTI 机制描述）: https://patchew.org/QEMU/20220427042312.294300-1-richard.henderson@linaro.org
[23] AOSP 文档：Arm MTE（模式/进程级开关/mallopt 93%/GKI KASAN_HW_TAGS 默认）: https://source.android.com/docs/security/test/memory-safety/arm-mte
[24] Google Project Zero：Pixel 8 首台 MTE 手机（bootloader 预留 tag 内存、开关方法）: https://projectzero.google/2023/11/first-handset-with-mte-on-market.html
[25] Android NDK：Arm MTE 指南: https://developer.android.com/ndk/guides/arm-mte
[26] arXiv 2601.11786：ARM MTE Performance in Practice（Pixel 8/9、AmpereOne、M5；最高 6.64x 减速）: https://arxiv.org/abs/2601.11786
[27] AOSP 文档：Control flow integrity in the kernel（GKI 默认、KCFI 免 LTO）: https://source.android.com/docs/security/test/kcfi
[28] Android Developers Blog（2018）：Control Flow Integrity in the Android Kernel: https://android-developers.googleblog.com/2018/10/control-flow-integrity-in-android-kernel.html
[29] KSPP Recommended Settings（含 IOMMU_DEFAULT_DMA_STRICT、EFI_DISABLE_PCI_DMA、arm64 PTR_AUTH/BTI/MTE 清单）: https://kspp.github.io/Recommended_Settings.html
[30] Linux 内核文档：Security self-protection: https://docs.kernel.org/security/self-protection.html
[31] KASAN 文档（SW_TAGS/HW_TAGS 模式）: https://www.infradead.org/~mchehab/rst_conversion/dev-tools/kasan.html
[32] 内核提交：KFENCE 在 KASAN HW_TAGS 下禁用: https://git.ideasonboard.com/epaul/linux/commit/09833d99db36
[33] QEMU 文档：Arm emulation（TCG 特性清单 FEAT_PAuth/MTE/BTI 与 max CPU）: https://www.qemu.org/docs/master/system/arm/emulation.html
[34] GrapheneOS 论坛：CFI/PAC 姿态（stock Pixel 内核开 PAC、用户态不开）: https://discuss.grapheneos.org/d/19060-information-on-cfi-in-grapheneos-and-response-to-a-paper
[35] Apple Platform Security：Security of runtime process: https://support.apple.com/guide/security/sec15bfe098e/web
[36] Apple Platform Security：Operating system integrity（MIE/EMTE）: https://support.apple.com/en-md/guide/security/sec8b776536b/web
[37] TidBITS（2025-09）：Memory Integrity Enforcement Bolsters iPhone Security: https://tidbits.com/2025/09/12/memory-integrity-enforcement-bolsters-iphone-security
[38] arcbjorn 博客：Apple MIE vs GrapheneOS MTE（Android 16 Advanced Protection 开 MTE、Apple sync-only 细节）: https://blog.arcbjorn.com/memory-safety-race
[39] GrapheneOS 论坛：Pixel 11 安全标准与 MTE 争议: https://discuss.grapheneos.org/d/41564-pixel-11-doesnt-yet-meet-the-grapheneos-security-standards-and-may-be-skipped
[40] Privacy Guides（2026-08-29）：GrapheneOS 因安全特性被砍无法完成 Pixel 11 移植: https://www.privacyguides.org/news/2026/08/29/grapheneos-unable-to-complete-pixel-11-port-due-to-cut-security-feature
[41] Cybernews（2026-09）：Pixel 11 新固件出现 MTE 支持: https://cybernews.com/tech/pixel-11-hardware-support-for-mte
[42] Google Security Blog（2025-05）：Advanced Protection on mobile（Android 16 强保护模式）: https://security.googleblog.com/2025/05/advanced-protection-mobile-devices.html
[43] Microsoft Learn：Virtualization-based Security (VBS) 概览（OEM）: https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/oem-vbs
[44] Microsoft Learn：启用基于虚拟化的代码完整性保护（HVCI）: https://learn.microsoft.com/en-us/windows/security/hardware-security/enable-virtualization-based-protection-of-code-integrity
[45] Microsoft Learn：Kernel DMA Protection（Thunderbolt）: https://learn.microsoft.com/en-us/windows/security/hardware-security/kernel-dma-protection-for-thunderbolt
[46] QNX 8 Hypervisor 文档：smmuman（IOMMU/SMMU 管理器、fail-closed）: https://www.qnx.com/developers/docs/8.0/com.qnx.doc.hypervisor.user/topic/protection/smmuman.html
[47] seL4 邮件列表：DMA 讨论（「IOMMU 提供统一机制执行内存访问策略」）: https://lists.sel4.systems/hyperkitty/list/devel@sel4.systems/message/SWAW6UYSS3X3K6BONWKSJFWCOQKLLAIE
[48] seL4 Discourse：Secure mode builds（SMMU 工作讨论）: https://sel4.discourse.group/t/secure-mode-sel4-builds-for-arm/78
[49] Google Blog（2026-10）：Android Advanced Protection 更新（六项新特性，未提 MTE）: https://blog.google/security/android-advanced-protection-updates
[49b] Trusted Firmware-M 文档：Branch Protection: https://trustedfirmware-m.readthedocs.io/en/latest/integration_guide/branch_protection.html
[50] FOSDEM 2025：Enabling PAC and BTI on arm64 in Debian: https://archive.fosdem.org/2025/schedule/event/fosdem-2025-5517-enabling-architectural-features-in-debian-pac-and-bti-on-arm64
[51] LWN：Kernel control-flow-integrity support comes to GCC（CONFIG_CFI 改名）: https://lwn.net/Articles/1056601
[52] patchew：Kees Cook「Enable KCFI and some other hardening options」（SCS/PAC 关系）: https://patchew.org/linux/20240430173657.it.699-kees@kernel.org
[53] Markettos et al., NDSS 2019：Exploring Vulnerabilities in OS IOMMU Implementations: https://www.cl.cam.ac.uk/research/security/ctsrd/pdfs/2019ndss-iommu-cameraready.pdf
[54] Intel 白皮书：Using IOMMU for DMA Protection in UEFI Firmware: https://www.intel.com/content/dam/develop/external/us/en/documents/intel-whitepaper-using-iommu-for-dma-protection-in-uefi-820238.pdf
[55] Linux 内核文档：IOMMUFD 用户态 API: https://docs.kernel.org/userspace-api/iommufd.html
[56] Schwarz（GI 论文）：DMA Security in the Presence of IOMMUs: https://dl.gi.de/bitstreams/1a7e69c5-eb7b-4ccb-b270-a119557a62e1/download
[57] PACMAN（CVE-2022-26700）分析: https://blog.wegenz.in/posts/cve-2022-26700-pacman-apple-m1-pointer-authentication-bypass
[58] Hacker News：GrapheneOS 称 Pixel 11 最终仍有 MTE 支持: https://news.ycombinator.com/item?id=49536384
[59] Mishaal Rahman：Pixel 8 MTE 开发者开关（开发者选项截图）: https://x.com/MishaalRahman/status/1721987213729161216

另：§3.2/§5 中标注「本机实验」的事实来自 2026-10-29 在本机 QEMU 11.0.3（macOS brew，TCG）上的命令行实测（`-machine virt,{mte=on,iommu=smmuv3}`、`-cpu max,pauth=on`、virtio-iommu-pci 实例化、modern-only virtio-pci 挂 iommu_platform/ats）。
