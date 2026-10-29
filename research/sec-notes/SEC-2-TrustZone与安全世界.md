# SEC-2 TrustZone 与安全世界 业界调研

> 调研日：2026-10-29。范围：ARM TrustZone 硬件隔离、TEE OS（OP-TEE/Trusty/各家）、EL3 runtime、RPMB/密钥落点、微内核 OS 与 secure world 的边界、QEMU 可验性——机制、演进史、各家对照、坑与教训。
> 纪律：硬事实带来源 [n]；【证】= 一手来源可查，【推】= 本报告推断。业界现状与「对 BerryOS 的启示」分开写。姊妹篇：SEC-1（引导链/Verified Boot）、SEC-3（度量启动）、SEC-7（磁盘加密）。

---

## 1. 概述：本维度解决什么问题、信任边界在哪

TrustZone 解决的问题：**主 OS（REE，Rich Execution Environment）太大，不可能没有漏洞**——内核一沦陷，密钥、指纹、DRM 内容全裸奔。方案是把 SoC 劈成两个世界：normal world 跑大而全的富 OS，secure world 跑一个小 TCB 的 TEE（Trusted Execution Environment，可信执行环境），密钥类操作留在 secure world【证】[2][3]。

**信任边界不在软件，在总线**【证】[2][3]：

- **NS bit**：每条 AXI/AHB 总线事务都携带 1 位 secure/non-secure 标记，由 CPU 当前安全态（SCR_EL3.NS 等）派生；外设与内存控制器据此放行/拒绝。normal world 的代码**物理上够不到** secure 资源，这层保证不依赖 REE 内核正确性。
- **TZASC**（TrustZone Address Space Controller）：DRAM 控制器前的关卡，把物理 DRAM 划成 secure/non-secure 区，normal world 对 secure DRAM 区的访问被拒【证】[4]。
- **TZPC**（TrustZone Protection Controller）：逐外设配置 secure/non-secure 归属（哪个 UART、哪个加密引擎只准 secure world 用）【证】[2][4]。
- **中断**：GIC 把中断分 Group 0（secure，作 FIQ）/Group 1（normal，IRQ）；secure 世界的计时器/SGI 走 Group 0【证】[2][3]。
- **世界切换**：`SMC` 指令陷入 EL3 的 secure monitor（如 TF-A BL31），monitor 保存/恢复两个世界的 CPU 状态后 `ERET` 到目标世界；EL3 是唯一同时看得见两个世界的特权级【证】[2][21]。
- **Cortex-M 侧另有一支**：ARMv8-M 的 **TrustZone-M**（v8-M Mainline 起）把双世界做进低功耗核——secure/non-secure 两套栈指针、SAU（Security Attribution Unit，按地址区间划安全态）、secure gateway 入口表（NSC/SG 指令）取代 A 核的 EL3 monitor 形态【证】[2]。对手表/穿戴档（BerryOS 同族低功耗端）这是绕不开的硬件事实：**小核上的安全世界不是 EL3 而是 SAU**【推】。

**元边界**：secure world 的存在意义 =「即使 normal world（也就是 BerryOS 自己）被完全攻陷，以下资产仍然安全」——所以决定「什么必须放进 secure world」的是这根尺子，而不是性能或便利。

---

## 2. 各家现状对照表

| 系统/阵营 | TEE 形态 | 硬件边界 | 典型负载 | 现状定位 |
|---|---|---|---|---|
| **Android（Google 亲儿子线）** | **Trusty**：LK 派生小内核 + 用户态 TA，开机镜像整体签名、bootloader 验签；TA=同优先级 RR、MMU 沙箱、**不支持第三方 TA**（单一构建方签名）【证】[10][11] | ARM 上用 TrustZone；x86 上用 VT-x【证】[10] | DRM/支付/多因子/防重置存储/PIN 与指纹处理【证】[10]；Pixel 上配 **Titan M2 独立安全芯片**（StrongBox/Weaver）+ Trusty【证】[14][31] | AOSP 开源 TEE；Pixel 10（2025，Tensor G5）仍是 Titan M2 + Trusty【证】[31] |
| **Android（SoC 厂商线）** | 高通 QSEE/QTEE（闭源）、三星旧机 Trustonic **Kinibi**（S3–S9 时代）【证】[40]、联发科等各自为政 | 各家 TrustZone | Keymaster/DRM/指纹 | **碎片化重灾区**：Android 不强制 TEE 实现，只定义接口（KeyMint/Gatekeeper HAL），每 SoC 一个闭源 TEE OS【证】[10][33] |
| **OP-TEE（Linaro/开源）** | optee_os core + ldelf + 用户态 TA + PTA，GlobalPlatform 规格实现，TA RSA 签名验签【证】[5][6] | TrustZone（内核文档明言当前仅支持 ARM TrustZone 形态）【证】[8] | GP TEE Client API 通用负载；ST/车规/IoT 广泛落地【证】[38] | 事实上的开源 TEE 标准；Linux 内核 tee 子系统的头号后端【证】[9] |
| **OpenHarmony** | **TEEOS**：内核为 HongMeng/ChCore **微内核**、musl 子集 libc、无网络无文件系统的极小面；Rust 官方 Tier 3 目标 `aarch64-unknown-teeos` 即对准它【证】[12] | TrustZone | CA（REE 侧）/TA（TEE 侧）模型【证】[12] | 微内核 TEE OS 量产先例（与 BerryOS「全 Rust 微内核」路线同构性最高） |
| **Apple** | **Secure Enclave**：**独立处理器**（非共享核 secure 态）+ 定制 L4 微内核 + 专属 AES 引擎/TRNG；DRAM 专用区经 AES-XEX+CMAC 加密、A11/S4+ 反重放完整性树根植 SRAM【证】[30] | 物理独立处理器 + 内存加密引擎（比共享核 TrustZone 更强的边界）【证】[30] | 文件系统密钥层级（UID 绑定）、生物识别、计数锁存盒（试错 10 次熔断）【证】[30] | 行业最强手机侧实现；SEP BootROM 为硬件信任根【证】[30] |
| **seL4** | **无正式支持**，Pre-RFC（2020 起）讨论 AArch64 TrustZone：NS bit 当作「第 49 位物理地址位」、untyped 能力加 secure/non-secure 轴、页表必须来自 secure untyped（CNode 在 NS 内存=能力可被伪造，致命）【证】[28][29] | 设计时复用 NS bit/TZASC/TZPC 语义 | 三种角色：S-EL1 的 TEE OS、S-EL2 的 SPM、SPM 上的 secure guest【证】[28] | 研究态（WIP patch + QEMU demo），未进主线【证】[28] |
| **Fuchsia** | **不自研 TEE**：把 TEE 当普通设备——`//src/devices/tee/drivers/optee`（通用 OP-TEE 客户端驱动）+ `//src/devices/securemem/drivers/aml-securemem`（Amlogic S950D2/T931 安全内存驱动，为视频受保护内存向 TEE 申请）【证】[27] | 依赖板卡既有 OP-TEE | Widevine/受保护视频内存【推：由驱动用途推断】 | 微内核 OS 对 TEE 的教科书式态度：**TEE = 又一个外设服务** |
| **Linux（作为 REE）** | 不跑 TEE，只做客户端：内核 tee 子系统（ioctl 门面）+ 各 TEE 后端驱动 + 用户态 tee-supplicant（替 TEE 干 RPC 活）【证】[8][9] | — | KeyMint/Gatekeeper HAL 落在厂商 TEE | GP TEE Client API 在 Linux 上的通用承载【证】[9][35] |
| **Windows** | **Pluton**：集成进 CPU 封装的安全处理器（Xbox/Azure Sphere 技术传承），可作 TPM 2.0 或独立信任根，固件随 Windows Update 更新【证】[34] | 物理集成（攻击面小于离散 TPM 芯片）【证】[34] | TPM 2.0（密钥/凭据） | 非 TrustZone 路线的对照样本 |
| **QNX** | 走 hypervisor 分离（QNX Hypervisor 8.0 for Safety，2026 安全认证）而非公开的 TrustZone TEE 产品线【证】[41] | 虚拟化边界 | 车规安全域隔离【证】[41] | 「隔离用 hypervisor 不用 TZ」的路线样本 |

### 2.1 业界共识：哪些功能必须放进 secure world

判据统一是「normal world 沦陷也不能失守」+「秘密一旦明文出现在 normal world 就等于泄了」：

| 功能 | 为什么必须在 secure world | 形态 |
|---|---|---|
| **密钥生成/存储/使用**（KeyMint 系） | 私钥出 normal world 即失守；证明链还要锚定硬件 | TEE TA 或独立芯片（StrongBox）【证】[13][14] |
| **口令校验**（Gatekeeper） | 口令/合成口令的 HAT（硬件认证令牌）签发必须可信；auth token 绑定磁盘密钥释放 | TEE TA【证】[13][15] |
| **限速持久秘密**（Weaver） | 暴力破解必须被物理限速（独立芯片计数器），纯软件限速可被重放绕过 | 独立安全芯片（Titan M 载体）【证】[14] |
| **DRM 内容解密**（Widevine L1） | 解密钥与解码路径全程不得在 REE 明文出现 | TEE + 受保护媒体路径【证】[32] |
| **生物识别匹配**（指纹/人脸模板） | 模板一旦明文出 TEE/SEP 即不可回收 | TEE/SEP（Apple 整个匹配链在 SEP 内）【证】[10][30] |
| **Verified boot 延续**（boot 状态消费/回滚计数） | 回滚索引与 boot 状态是密钥释放的先决条件，REE 自己上报不可信 | bootloader+TEE/计数器联动（详见 SEC-1/SEC-3）【证】[13][23] |

注意边界共识同样重要：**通用计算不放进去**（TCB 膨胀教训见 §4）、**键值缓存/编译/AI 处理等新需求正在转向 hypervisor 隔离**（AVF）而非 TEE【证】[33]。

---

## 3. 机制细节（达到「能照着留接口位」的精度）

### 3.1 SMC Calling Convention（SMCCC，Arm DEN 0028）——两世界唯一正门

- 统一约定 `SMC`/`HVC` 指令的寄存器用法：x0 = Function ID，x1–x4 传参，x0–x3 回值；**SMC32**（32 位参数）/ **SMC64**（64 位参数）两形【证】[20]。
- Function ID 按 **OEN**（Owning Entity Number）分段：Arm 架构服务/SMC64 扩展/CPU 服务/SIP（SoC 厂商）/OEM/标准安全服务/TPM 等，每段再分 fast/standard 调用位——**这张分段表就是「谁的号谁处理」的分派契约**，TF-A EL3 monitor 按它路由到注册的 runtime 服务【证】[21]。
- **fast call**（原子、不睡、不分配）vs **standard/yielding call**（可睡、可被 normal world 抢回）的区分编在 Function ID 里【证】[8][20]。教训见 §4。
- FF-A（Firmware Framework for Arm）是其上演进的分区/消息协议：EL3/SPMC 上托管多个安全分区、分区端点间消息直传、内存借出（lend）语义，OP-TEE 列明 SPMC/安全分区与 FF-A 合规支持【证】[5]。可把它理解为「secure world 内部的微内核化」——EL3 从「TEE OS 专属通道」演进为「多安全分区共享的调度/消息底座」【推】。

### 3.2 GIC 与安全中断（Group 0/1、FIQ）

- 中断分组：**Group 0 = secure only**（作为 FIQ 注入）、Group 1 Secure / Group 1 NS 走 IRQ；GICD_IGROUPR/GICR_IGROUPR 配置归属，secure world 的定时器/SGI（核间通信）走 Group 0【证】[2][3]。
- 常见拓扑：EL3 monitor 用 Group 0 FIQ 做世界抢占（TEE 事件在 normal world 运行时叫醒 CPU）；normal world OS 对 FIQ 无感知，monitor 转发或自行处置【证】[2]【推：具体平台策略有差异】。
- BerryOS 侧的对照坑：仓库已踩「GIC SPI 段 ITARGETSR 复位 0 = 静默不投任何核」——真机secure 配置下 GICD 的 secure 门（GICD_CTLR 双写/E bit）还会再叠一层，届时 init 序列须由 monitor 预配【推】。

### 3.3 TF-A：EL3 runtime 与信任链执法者

- 启动链 BL1（ROM/BootROM 侧）→ BL2（TBBR 认证）→ **BL31 = EL3 常驻 runtime**，暴露 PSCI（CPU_ON/OFF/挂起）、SCMI（时钟/DVFS 通用驱动）、厂商 SIP 服务段；**BerryOS 的 SMP bring-up（PSCI CPU_ON via HVC）消费的就是它**【证】[21][24]。
- **TBBR 信任链**：X.509v3 **自签证书**当载体（信任来自扩展字段内容而非签发者链）——Key 证书（装公钥）与 Content 证书（装镜像哈希）成对；ROTPK（根公钥）哈希存 OTP/eFuse 或专用寄存器；证书栈：Trusted boot（ROTK 签）→ Trusted key（分世界密钥）→ BL3x key/content 两级【证】[22]。默认 CoT 三选：TBBR / **Dualroot**（secure 与 normal 世界各一个根）/ **CCA**（三根，对应三条供应链）【证】[22]。
- **NV 反回滚计数器**：每张证书携带 `nv_ctr`，镜像描述符声明 `AUTH_METHOD_NV_CTR`——证书侧值 vs 平台侧值（由平台 API 供给，通常 OTP 熔丝/RPMB）比较；平台侧单调、只升不降【证】[23]。
- **Measured boot**：TF-A 可对各级镜像做度量、产 **EventLog**，由后续固件（如 U-Boot）在 TPM 上重放扩展进 PCR——度量者与 PCR 持有者解耦【证】[25]。另有 DRTM（动态信任根）作为 BL31 服务【证】[24]。

### 3.4 OP-TEE：组件、加载与调度模型

- **组件**：`optee_os` core（内核态）+ `ldelf`（用户态 TA 的 ELF 加载器：重定位、常驻供 dlopen/dlsym、TA 崩溃时 dump 栈/映射）+ 用户态 TA（EL0）+ **PTA**（伪 TA——编进 core 的「接口而非进程」，用 core 内部 API）【证】[5][6]。
- **TA 签名**：`struct shdr` 头（magic `0x4f545348`）+ hash + RSA-Sign（RSASSA-PKCS1-v1_5-SHA256）；三种 img_type：SHDR_TA（legacy）/SHDR_BOOTSTRAP_TA/SHDR_ENCRYPTED_TA（AES-GCM 加密 TA）；**Subkey 机制**支持公钥链（根钥签子钥、子钥签 TA，子钥限定在自家 UUID-v5 命名空间内）【证】[6]。**TA 防回滚依赖 `ta_ver.db` 且仅在 RPMB FS 下有效**【证】[6][7]。
- **调度模型（关键且常被误解）**：secure world 由 normal world **调度**——TEE 任务只能在 CPU 交还 normal world 时被再调度；standard/yielding call 为 TEE 线程，fast call 原子不建线程【证】[8][38]。
- **normal world 侧**：Linux tee 子系统（`TEE_IOC_*` ioctl 门面承载 GP TEE Client API）+ optee 驱动 + tee-supplicant；`OPTEE_SMC_CALL_WITH_ARG` 驱动消息协议、`OPTEE_SMC_GET_SHM_CONFIG` 协商共享内存、异步通知走一条 non-secure 边沿中断 + `DO_BOTTOM_HALF` yielding call 回收下半部；TEE 反向请求（RPC）也经同一通道回到 supplicant【证】[8][9]。
- **GP TEE Client API 形态**（照抄级）：`TEEC_Context`→`TEEC_OpenSession`→`TEEC_InvokeCommand`，参数 4 槽，类型 = none/value/memref(tmpref)——共享内存 `TEEC_SharedMemory` 注册后按引用传【证】[35]。

### 3.5 密钥与可信存储落点（Android 用例线）

- **密钥派生三级**（OP-TEE REE FS）：`SSK = HMAC-SHA256(HUK, ChipID || "static string")`（HUK=硬件唯一钥，eFuse 取）→ `TSK = HMAC(SSK, TA_UUID)` → 每文件随机 FEK；完整性靠**哈希树**（节点护两个子节点+数据块，双版本 0/1 out-of-place 更新保原子）【证】[7]。
- **RPMB FS**：FAT 在 RPMB 偏移 512 处（**FAT 本身不加密**），文件数据 AES-CBC+ESSIV（IV=块号用 FEK 哈希加密），FAT 块**最后写**保原子；声明「RPMB 下回滚防护等级 1000，无 RPMB 为 0」【证】[7]。
- **RPMB 帧格式**（Linux `include/linux/rpmb.h` 一手定义，512B/帧、大端）：`stuff[196] | key_mac[32] | data[256] | nonce[16] | write_counter[4] | addr[2] | block_count[2] | result[2] | req_resp[2]`；req_resp 五值：0x1 PROGRAM_KEY / 0x2 GET_WRITE_COUNTER / 0x3 WRITE_DATA / 0x4 READ_DATA / 0x5 RESULT_READ；地址/块数以 **256B 半扇区**计；容量以 128KiB 为单位；dev_id 参与认证钥派生【证】[17]。认证钥 **32B OTP、一次编程、永不可读**，须产线安全环境写入；防回放 = 写计数器单调递增且入 MAC【证】[16]。
- **Android 依赖图**：磁盘/FBE 密钥、KeyMint（密钥生成/证明）、Gatekeeper（口令校验、发 auth token）跑 TEE TA；**Weaver**（合成口令的限速持久存储）由独立安全芯片实现（Titan M 即 Weaver API 载体）；StrongBox=KeyMint 的更高隔离档【证】[13][14][15]。**KeyMint 证明扩展绑定 Verified Boot 状态**（bootloader 锁态/OS 版本/补丁级）；远程密钥供给改发短时效证明钥【证】[13][39]。
- **Widevine 分级**：L1 = 解密+解码+渲染全程 TEE 内（HD/4K 门槛）；L3 = 纯软件（480p 封顶，2019 年公开破解）；无 TEE 的设备到顶 L3【证】[32]。

### 3.6 Trusty 内部件（作为「闭源 TEE 对照」的公开样本）

- IPC 面：Trusty 应用间通信用**同步 handle 型 IPC**（`tipc`：port 名注册、handle 换消息），语义与 BerryOS channel 同族但无能力委托位【证】[11]。
- 持久存储：`trusty/app/storage` 提供**加密防篡改的文件语义存储**（RPMB + 非安全盘分层），KeyMint/Gatekeeper 密钥与 Weaver 等价物落在这上面【证】[10][11]。
- 构建面：TA 用 Trusty SDK（libtrusty 头 + 工具链）构建；Rust prebuilts 与 AOSP crates 已进 trusty manifest——**Google 亲儿子 TEE 已是 Rust+C 混编生产线**【证】[11][12]。

### 3.7 QEMU 可验性（BerryOS 主验证环境）

- `virt` 机型 `-machine secure=on`（**默认 off**）启用安全扩展模拟；secure-only 设备恰四件：**第二个 PL011 UART、第二个 PL061 GPIO、secure flash、16MB secure RAM**；DTB 含 `/secure-chosen` 节点【证】[26]。
- **virt 无 TZASC/TZPC 模型**（文档只字未提、无编程性 DRAM 安全分区控制器）【证】[26]——DRAM 区级安全分区、外设逐个归属配置在 QEMU 上**不可验**。可验面 = EL3 存在性、SMC 分派、双世界镜像、secure-only 设备访问控制【推】。
- 生态已有 TF-A + OP-TEE 在 QEMU 跑通的标准实验通道（redpesk 等）【证】[26 之生态实践，见来源 42]。vTZ（SJTU）证明 TrustZone 本身可被虚拟化嵌套【证】[37]。

### 3.8 Apple Secure Enclave：比 TrustZone 更强一档的参照

- **独立处理器 + 定制 L4 微内核**：不是共享核的 secure 态，而是 SoC 内独立 CPU，降频运行抗时钟/功耗侧信道，专属 TRNG（多环振 + CTR_DRBG）与 AES 引擎【证】[30]。
- **内存加密而非内存分区**：SEP 用 DRAM 专用区，访问经内存保护引擎按块 AES-XEX 加密 + CMAC 标签，标签不匹配即停机到重启；A11/S4+ 加反重放完整性树、树根植于专用 SRAM；A14/M1+ 双临时密钥（SEP 私有 / 与安全神经引擎共享）【证】[30]。对照：TrustZone 靠 TZASC **分区**隔离 DRAM，SEP 靠**加密+完整性树**隔离——后者对共享 DRAM 的物理攻击面更免疫【推】。
- **密钥层级**：出厂熔丝 **UID**（A9 起由 SEP 内 TRNG 生成、苹果与供应商都不可见）→ 文件系统密钥层级含 UID（换机不可读）；每次开机 BootROM 生成**临时内存保护钥**、sepOS 再生成临时包装钥经专线送存储 AES 引擎包 per-file key——**密钥不出线**是设计铁律【证】[30]。
- **计数锁存盒**（counter lockbox，二代安全存储组件）：口令熵 + 尝试计数在独立组件里；iPhone 10 次失败熔断擦除；A10+ 可锁 seed 位拒 DFU 模式下读口令保护数据【证】[30]。这是 Weaver 语义的「苹果完备版」【推】。
- **BootROM → sepOS**：SEP BootROM 为 SEP 硬件信任根，iBoot 分配 SEP 内存区、BootROM 验 sepOS 哈希+签名，失败则 SEP 冻结到下次芯片复位；A13+ Boot Monitor 对启动过程滚动哈希、喂 OS 绑定钥【证】[30]。



### 3.9 OP-TEE 自身加固面（可借的清单）

core 与用户 TA 双层 ASLR、栈保护插桩、**BTI**、可回收内存页（pager）、MMU 沙箱分层——TEE OS 自己也在按 REE 内核的攻防标准加固【证】[5]。教训面同样成体系：`OPTEE_INSECURE_LOAD_IMAGE`（开机后加载 BL32）被文档明示扩大攻击面，要求内核/rootfs 验证链先行、驱动 builtin 先于任何攻击面挂载【证】[8]。

---

## 4. 演进史与教训

**时间线（哪些 day-one、哪些补课）**：

- TrustZone 诞生于 ARMv6（2003 年前后，ARM11），最初瞄准 DRM 与移动支付；ARMv8 把 secure monitor 正名为 **EL3** 并给 AArch64 世界切换【证】[2][3]。**这是 SoC 契约级 day-one 件**：TEE OS 形态可以换（各家确实在换），EL3+NS bit+TZASC/TZPC 二十多年没变。
- Android 侧一路补课：FDE 时代密钥进 TEE/RPMB【证】[15] → 生物识别与 DRM 走 TA → **Weaver/限速**（独立芯片）→ **StrongBox**（Android 9，2018，Titan M【证】[14]）→ **远程密钥供给**（短时效证明钥，堵 keybox 伪造）【证】[39]。每次补课都是「上一个设计被现实击穿」：TA 版本不设防 → Kinibi 回滚攻击（USENIX Sec'22，Trust Dies in Darkness——签名验证只验真伪不验新旧，老版本 TA 配老漏洞钥被整串拔出）【证】[43]；静态证明 keybox 被拔走倒卖 → 远程供给【推，趋势由 39/33 佐证】。
- **QSEE 教训**：SMC 接口面=攻击面，Qualitrust/Beniamini 系列研究长年打 SMC handler；2019 年 Secure World 虚拟处理器级漏洞泄露支付数据【证】[36]。行业响应是 fuzz TEE（Kinibi/QSEE 均有公开 fuzzing 工作）【证】[40]。
- **RPMB 协议本身有洞**：WD 白皮书指出协议级漏洞族；2025 年 EMFI 物理注入破 RPMB 认证【证】[18][19]。OP-TEE 自身的坑：HUK 取不到的平台回退**常量钥**=裸奔（文档明示 caveats）；`CFG_RPMB_WRITE_KEY` 明钥下发仅限测试；normal world 可反复触发写钥尝试【证】[7]。
- **结构教训一：TEE 与主 OS 紧耦合、按 SoC 定制、闭源碎片化**——每 SoC 一个 TEE OS，安全研究只能在黑盒上做；这正是 Google 拿开源 Trusty 当「免费可靠替代品」的动机，也是 AVF（hypervisor 隔离）被看好承接新需求（端侧 AI 处理 PII）的原因【证】[10][33]。
- **结构教训二：secure world 被正常世界调度**——TEE 时延受 REE 支配、REE 可饿死 TEE（DoS 面非机密面）；fast/standard 二分就是为了把不可睡的调用摘出调度依赖【证】[8][38]。
- **结构教训三：TCB 膨胀**——TEE 里塞的东西越多（DRM+密钥+指纹+……），Kinibi 式黑盒越大越难审；Apple 反向走：**独立处理器+独立微内核+内存加密**，边界物理化【证】[30]。
- **结构教训四：微内核阵营的路线分歧**——seL4 想用能力模型「吃下」双世界（NS bit 当能力轴），但卡在能力位宽、S-EL2 双 stage-2 表等结构问题，五年仍在 Pre-RFC【证】[28][29]；Fuchsia 务实放弃自研，TEE 当外设接 OP-TEE【证】[27]；OpenHarmony 则真做了微内核 TEE OS（ChCore 系）且进了 Rust 官方目标清单【证】[12]。

---

## 5. 对 BerryOS 的启示（现状是事实、以下是建议）

定位前提：BerryOS v1/v2 = normal world 里的微内核 OS，**不建 TEE、不占 EL3**；secure world 是 SoC/固件厂商资产，BerryOS 是它的**客户端**。这与 Fuchsia 的态度一致【证】[27]，与「QEMU 先行」的现实一致（virt 无 TZASC，双世界 DRAM 分区本就不可 QEMU 验证【证】[26]）。

**day-one 留缝项（现在埋、防「后补≠自动生效」）**：

1. **SMC conduit 分派表进架构层**：内核 `arch/aarch64` 收敛唯一 SMC 发起点 + 按 OEN 段注册的分派表（PSCI/SCMI/SIP/保留段），每段一个注册者。仓库已实抓「PSCI conduit 按机型分型 HVC/SMC」坑（S-1：EL2 机型 HVC 挂死）——把 conduit 类型与 EL3 在位性做成 **BerryHandoff 字段**（boot 期探测一次、内核不猜），正是这类缝的实例。
2. **TEE 访问面 = 又一个用户态服务**：照 Fuchsia 模式，未来 teed 服务（OP-TEE 客户端或 GP API 语义）走 channel 协议（`berry-teeproto` 家族），内核只提供「许可的 SMC 桥」+ 共享内存原语——**不新增 syscall、不让用户态直发 SMC**。day-one 缝 = 内核 SMC 桥的**逐段 allowlist 数据结构**（哪些 OEN 段允许被哪个持证服务经内核代理），devmgr 授权模型（驱动=持证应用）天然承接。
3. **boot_state 证明锚 day-one 记录**：Android 的教训是「密钥证明绑定 VB 状态」是生态件而非内核件，但**事后把 boot 状态塞进密钥证明=生态重做**。BerryOS 从 v1 起：handoff/遥测里固化一个 `boot_state` 块（度量摘要、锁态、A/B 槽位、引导者版本——ADR #47 的 boot slot 与 #24 度量链的交汇点），v3 远程证明与未来 keymaster 类服务直接消费，不改格式。
4. **反回滚计数器语义 day-one 立法**：TF-A 的「证书嵌 nv_ctr vs 平台单调计数器」二值比较【证】[23] 是可照抄的最小语义。v1 无 RPMB 时用盘上单调计数文件顶替，但**把比较语义与存放位置解耦**（接口位 = `get_rollback_counter(slot)`），真机/RPMB 到位换实现零重构。RPMB 帧格式（§3.5）留作块服务 wire 参照。
5. **DMA VMO/物理页簿记预留 security 属性位**：seL4 的核心困难是「内存模型没有 secure/non-secure 轴、事后加=API 重做」【证】[28]。BerryOS 内存对象（VMO/untyped 等价物）与 SMMU domain（决策 #22）day-one 就带一个 `secure: bool` 占位（v1 恒 false），v4 若上真机 TZASC 直通，安全内存成为能力轴而非内存模型手术。
6. **QEMU secure 通道提前试跑**：`-machine secure=on` 的四件 secure-only 设备【证】[26] 是零成本的双世界纪律考场——可加一条「secure RAM 由 normal world 访问被拒」的 HAL 断言相（真机 TZASC 语义的代理测试），挂进独立 smoke（同 smoke-x86 形态，不进主 smoke）。
7. **手表线单独记账**：穿戴端小核若为 ARMv8-M（TrustZone-M/SAU 形态，§1），双世界入口是 NSC/SG 表而非 EL3 SMC【证】[2】【推：具体端型未定】——HAL 里把「secure 调用」抽象成 conduit 接口（SMC/HVC/NSC 三实现），v1 只落 ARM64 的，穿戴线实现留位不下场。

**v2 落点**：`-cpu max` 开 PAC/BTI/MTE（既定 #23）之外，补 SMC 分派表 + teed 服务骨架（纯 QEMU 可验：对着假 secure monitor 服务对拍 fast/standard 两种语义）；PM 服务（DVFS governor）对齐 **SCMI 消息面**而非私有协议，真机 EL3 SCMI 直通零改动。
**v3 落点**：度量链（#24）补 EventLog 形态【证】[25]（度量者与 PCR/锚持有者解耦——BerryBoot 度量、内核消费，恰是 TF-A→U-Boot 分工的同构）；远程证明探索消费 `boot_state` 块。
**v4+ 落点**：签名+反回滚走 NV 计数器两值比较（§5-4 已留缝）；Widevine 类 DRM 依赖（L1 要求 TEE 内解密路径【证】[32]）判给 teed+厂商 TEE 集成，不自研；全 Rust TEE OS（对标 OpenHarmony TEEOS【证】[12]）列为实验轨道而非产品依赖——BerryOS 微内核+纯能力模型天然适合切成 secure world 内核，但那是真机世代的事。

**负面清单（明确不做）**：不做 Linux 二进制兼容于 TEE 面（同 #全局决策）；不把 TEE 调用绕过能力模型开特权 syscall（违纯能力决策 #2）；不在 v1/v2 占 EL3（QEMU/AAVMF 链上 BerryBoot 是 EFI 应用，EL3 属 SoC 固件栈）。

---

## 6. 来源清单

1. Arm — TrustZone for Cortex-A：https://www.arm.com/technologies/trustzone-for-cortex-a
2. Pinto & Santos, *Demystifying Arm TrustZone: A Comprehensive Survey*, ACM CSUR 2019：https://dl.acm.org/doi/fullHtml/10.1145/3291047
3. *TrustZone Explained*, ASU SEFCOM 2016：https://sefcom.asu.edu/publications/trustzone-explained-cic2016.pdf
4. Genode — An Exploration of ARM TrustZone Technology：https://genode.org/documentation/articles/trustzone
5. OP-TEE Documentation — Architecture：https://optee.readthedocs.io/en/latest/architecture/index.html
6. OP-TEE Documentation — Trusted Applications（ldelf/SHDR/PTA/Subkeys）：https://optee.readthedocs.io/en/latest/architecture/trusted_applications.html
7. OP-TEE Documentation — Secure Storage（REE FS/RPMB FS/Key Manager/caveats）：https://optee.readthedocs.io/en/latest/architecture/secure_storage.html
8. Linux kernel docs — OP-TEE driver（SMC 分层/fast vs yielding/RPC/异步通知）：https://docs.kernel.org/tee/op-tee.html
9. Linux 内核 TEE 子系统文档（ioctl 门面/supplicant）：https://android.googlesource.com/kernel/common/+/30bac164aca7/Documentation/tee.txt
10. Android docs — Trusty TEE：https://source.android.com/docs/security/features/trusty
11. AOSP — Trusty TEE Application SDK：https://android.googlesource.com/trusty/lib/+/refs/heads/main/sdk/README.md
12. rustc 平台支持 — aarch64-unknown-teeos（OpenHarmony TEEOS，HongMeng/ChCore 微内核）：https://doc.rust-lang.org/rustc/platform-support/aarch64-unknown-teeos.html
13. Android docs — Hardware-backed Keystore（KeyMint/Gatekeeper/StrongBox/证明）：https://source.android.com/docs/security/features/keystore
14. Google blog — Building a Titan（Titan M/Weaver/StrongBox）：https://android-developers.googleblog.com/2018/10/building-titan-better-security-through.html
15. Quarkslab — Android Data Encryption in depth（Gatekeeper/auth token/FBE 密钥流）：https://blog.quarkslab.com/android-data-encryption-in-depth.html
16. Sergio Prado — RPMB, a secret place inside the eMMC（分区结构/OTP 钥/计数器）：https://sergioprado.blog/rpmb-a-secret-place-inside-the-emmc/
17. Linux `include/linux/rpmb.h`（rpmb_frame 帧定义/opcode 一手源码）：https://github.com/torvalds/linux/blob/master/include/linux/rpmb.h
18. Western Digital 白皮书 — RPMB protocol vulnerabilities：https://documents.westerndigital.com/content/dam/doc-library/en_us/assets/public/western-digital/collateral/white-paper/white-paper-replay-protected-memory-block-protocol-vulernabilities.pdf
19. arXiv 2025 — Keyless Entry: Breaking eMMC RPMB with EMFI：https://arxiv.org/html/2511.22340v1
20. Arm — SMC Calling Convention（DEN 0028）：https://developer.arm.com/architectures/secure-monitor-calling-convention
21. TF-A — EL3 Runtime Service Writer's Guide（OEN/FID 编码/服务注册）：https://trustedfirmware-a.readthedocs.io/en/v2.11/getting_started/rt-svc-writers-guide.html
22. TF-A — Trusted Board Boot design（X.509 自签/CoT/Dualroot/CCA）：https://trustedfirmware-a.readthedocs.io/en/latest/design/trusted-board-boot.html
23. TF-A — Authentication Framework（AUTH_METHOD_NV_CTR 两值比较）：https://trustedfirmware-a.readthedocs.io/en/latest/design/auth-framework.html
24. TF-A — Feature Overview（PSCI/SCMI/DRTM）：https://trustedfirmware-a.readthedocs.io/en/lts-v2.8.48/about/features.html
25. TRS — measured boot（TF-A 度量+EventLog、U-Boot 重放 PCR）：https://trs.readthedocs.io/en/v0.3/features/feature.html
26. QEMU — virt 机型文档（secure=on、四件 secure-only 设备、无 TZASC/TZPC）：https://github.com/qemu/qemu/blob/master/docs/system/arm/virt.rst
27. Fuchsia — Hardware drivers 参考（optee / aml-securemem 条目）：https://fuchsia.dev/fuchsia-src/reference/hardware/drivers
28. seL4 Discourse — Pre-RFC: TrustZone support on AArch64：https://sel4.discourse.group/t/pre-rfc-trustzone-support-on-aarch64/250
29. seL4 devel — seL4 + TrustZone: Spanning both worlds：https://web.sel4.systems/hyperkitty/list/devel@sel4.systems/thread/OZYZQRWDWSSN5QXOTT54TUQE4RKTTEQX/
30. Apple Platform Security — Secure Enclave：https://support.apple.com/guide/security/secure-enclave-sec59b0b31ff/web
31. Google blog — Pixel 10 / Tensor G5（Titan M2 + Trusty）：https://blog.google/products-and-platforms/devices/pixel/tensor-g5-pixel-10
32. Bitmovin — Widevine Security Levels（L1/L3 与 TEE 要求）：https://developer.bitmovin.com/playback/docs/widevine-security-levels-in-web-video-playback
33. Sandbox Brief — Is Android Killing TEE?（TEE 紧耦合批判/AVF 承接新需求）：https://www.sandboxbrief.com/is-android-killing-tees
34. Microsoft Learn — Pluton security processor：https://learn.microsoft.com/en-us/windows/security/hardware-security/pluton/microsoft-pluton-security-processor
35. GlobalPlatform TEE Client API v1.0 头文件（TEEC_* 形态一手参照）：https://github.com/Trustonic/trustonic-tee-user-space/blob/master/MobiCoreDriverLib/ClientLib/public/GP/tee_client_api.h
36. ZDNet — Qualcomm Secure World 漏洞（2019）：https://www.zdnet.com/article/qualcomms-secure-world-virtual-processor-leaks-mobile-payment-data
37. vTZ: Virtualizing ARM TrustZone（SJTU, Security'17）：https://ipads.sjtu.edu.cn/_media/publications/vtz-security17.pdf
38. ST wiki — OP-TEE concepts overview（secure world 由 normal world 调度等）：https://wiki.st.com/stm32mpu/wiki/OP-TEE_concepts_overview
39. Google blog — StrongBox 与开放标准数字凭证（远程密钥供给趋势）：https://blog.google/security/android-strongbox-and-open-standards-digital-credentials
40. Akimov（Positive Technologies）— Fuzzing TrustZone TEE on Android（Kinibi 用于 S3–S9）：https://www.scribd.com/document/510747009/D2-Launching-Feedback-Driven-Fuzzing-on-TrustZone-TEE-Andrey-Akimov
41. BlackBerry QNX — Hypervisor 8.0 for Safety：https://www.stocktitan.net/news/BB/qnx-hypervisor-8-0-for-safety-powers-the-industry-shift-toward-l1lvw1dhjclp.html
42. redpesk — ARM TrustZone Emulation with QEMU（TF-A/OP-TEE QEMU 实验通道）：https://docs.redpesk.bzh/docs/en/master/redpesk-os/trusted-boot/trusted-boot-qemu.html
43. Shakevsky, Ronen & Wool — Trust Dies in Darkness（USENIX Security 2022，Kinibi TA 回滚攻击）：https://www.usenix.org/conference/usenixsecurity22/presentation/shakevsky
