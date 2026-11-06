# SEC-1 引导链安全与 Verified Boot 业界调研

> 调研日：2026-10-29。范围：从 BootROM/固件到内核的完整性验证链——机制、数据结构、演进史、各家对照、坑与教训。
> 纪律：硬事实带来源 [n]；【证】= 一手来源可查，【推】= 本报告推断。业界现状与「对 BerryOS 的启示」分开写。

---

## 1. 概述：本维度解决什么问题、信任边界在哪

引导链安全回答一个问题：**从按下电源到内核拿到第一页代码，每一步执行的代码凭什么可信？** 具体拆为四个子问题【证】[3]：

1. **真实性**（authenticity）：代码是预期签名者签的，不是攻击者替换的。
2. **完整性**（integrity）：代码没被篡改（误改/恶意改）。
3. **防回滚**（anti-rollback）：不能退回有已知漏洞的旧版本（回滚攻击）。
4. **失败语义**：验证失败时设备是砖、是降级、还是用户明示后继续？

**信任边界 = 硬件信任根**：一段不可改写的硅上代码/数据（BootROM + OTP/eFuse 里的根公钥哈希）。边界以内无条件信任，以外全部验签。业界一致形态是**逐级验证链**（chain of trust）：每级在跳转前验证下级的签名/哈希，信任像接力棒传递【证】[3][21]。两条互补路线要区分开：

- **Secure Boot**（验证启动）：固件**拒绝**运行未签名代码——阻断执行，攻击者过不去。
- **Measured Boot**（度量启动）：固件把每级代码的哈希**扩展进 TPM PCR**、不拦截，由后续秘密（磁盘加密密钥）绑定 PCR 值——攻击者可以改，但改了就拿不到数据。ChromeOS 早期设计文档与 Linux IMA 走这条线【证】[25][43]。

---

## 2. 各家现状对照表

| 系统 | 信任根 | 验证主体/数据结构 | 防回滚 | 解锁/用户自主权 | 现状定位 |
|---|---|---|---|---|---|
| **Android (AVB 2.0)** | SoC BootROM + eFuse 根公钥哈希（高通 QFPROM / MTK 熔丝）【证】[10][11][15] | bootloader 验 `vbmeta` 分区（RSA 签名）→ hash/hashtree descriptor → dm-verity 运行期【证】[1][3] | vbmeta 内 `rollback_index` × N 槽 vs RPMB/Titan 芯片单调计数【证】[1][8] | `fastboot flashing unlock`：**双向换状态都擦数据** + 30s 确认 UI；四色状态 green/yellow/orange/red【证】[4][5] | 事实标准，Android 8 起标配 |
| **Apple iOS/macOS** | SecureROM（掩膜 ROM，含 Apple Root CA 公钥）【证】[26][28] | Image4 签名格式逐级验：iBoot → kernel；线上「系统软件授权」签名响应 | **AP/SEP nonce 防重放**（`bnch`/`snon` tag，每次生成 nonce 绑定签名响应）+ 安全存储组件计数器【证】[27] | 无解锁面（越狱=漏洞）；DFU 重刷强制走线上授权【证】[26] | 封闭端到端，线上反重放是独门 |
| **ChromeOS** | RO 固件桩（写保护脚 + 烧死公钥）【证】[25] | RW 固件 A/B 验 kernel 分区签名；rootfs 走 dm-verity（4K 块按需验） | kernel 版本号存 **TPM NV**，TPM 拒绝降版本【证】[25][推：设计文档承认初版缺、后补] | 开发者模式 = 关验证 + **全盘擦**；恢复镜像重刷 | A/B 回退机制的祖师爷 |
| **Fuchsia** | 依赖 EFI Secure Boot + Gigaboot（AAVMF 路径）【证】[52 推] | ZBI 签名（`zbi --signed`）+ AVB 式 `vbmeta_{a,b,r}` 分区链【证】[52 推：分区面 zircon_{a,b,r} 有实录] | vbmeta rollback index 复用 AVB 模型【推】 | 开发板普遍解锁；无消费级解锁面 | 抄 AVB 作业，bootloader 不自带信任根 |
| **seL4** | **无内置**——形式化证明假设「内核镜像被正确加载」【证】[50][51] | 信任 bootloader（U-Boot/UEFI）装对镜像；证明覆盖不到 boot 期 | 无 | 不适用 | 教训位：验证内核 ≠ 验证启动 |
| **QNX** | SoC 级（i.MX HAB / eFuse 等）【证】[53] | IFS 镜像签名（oesign 工具）+ 启动 ROM 验签接力【证】[49] | 随 SoC（HAB 版本号熔丝）【证】[49 推] | 按部署方策略 | 依赖芯片厂信任根，OS 层只做镜像签名格式 |
| **Windows/PC** | UEFI 固件 + PK/KEK/db/dbx + TPM 2.0【证】[43] | 固件验 db 签名（Secure Boot）；同时把组件哈希度量进 PCR（Measured Boot）；BitLocker 密钥封在 PCR 上 | dbx 吊销列表（时序性，非单调强制） | 可关 Secure Boot（UEFI 菜单），关了 BitLocker 走恢复密钥 | PC 侧双轨并行的范本 |
| **Linux 发行版** | Microsoft UEFI CA（第三方 db） | shim（MS 签）→ grub（shim 验，MOK 机制）+ SBAT 版本承诺【证】[34] | SBAT（Revocations made easy，shim 内嵌策略而非 dbx）【证】[34] | MOK 注册自定义密钥（用户物理确认） | BootHole 的直接受害者与修复创新者 |
| **物联网/MCU** | BootROM + OTP（NXP HAB、STM32 SFI 等）【证】[49] | CSF 签名块附在镜像上；验证失败进入串行下载模式 | 版本号熔丝（一次性） | 无（产线熔死） | 一次性熔丝经济模型主导 |

---

## 3. 机制细节（达到「能照着留接口位」的精度）

### 3.1 AVB vbmeta：中心化签名清单（Android 8+）

vbmeta 是一个**单独分区**，装全部验证元数据，本身 RSA 签名；bootloader 只需要信任它就能间接信任所有分区【证】[1]。头部 256 字节，网络字节序，全部字段实测如下【证】[2]：

```
magic[4]                 = "AVB0"
required_libavb_version_major/minor : u32 ×2   —— 最低 libavb 版本
authentication_data_block_size      : u64      —— 签名块大小
auxiliary_data_block_size           : u64      —— 公钥+描述符块大小
algorithm_type           : u32      —— AVB_ALGORITHM_TYPE（RSA-2048/4096/8192 + SHA256/512）
hash_offset/size, signature_offset/size          : u64 ×4 —— 认证块内布局
public_key_offset/size, public_key_metadata_...  : u64 ×4 —— 辅助块内布局
descriptors_offset/size             : u64 ×2
rollback_index          : u64      —— 单调防回滚索引
flags                   : u32      —— bit0 HASHTREE_DISABLED / bit1 VERIFICATION_DISABLED
rollback_index_location : u32      —— 防回滚槽位号（0..N-1，链式分区独立槽）
release_string[48]      —— 如 "avbtool 1.2.0"
reserved[80]
```

三种关键 descriptor【证】[1]：
- **hash descriptor**（boot 这类整装镜像）：分区名 + 哈希算法 + 长度 + salt + digest。
- **hashtree descriptor**（system/vendor 这类大分区）：根哈希 + salt + hashtree 偏移/大小 + FEC 参数——直接喂给 dm-verity 建表。
- **chain partition descriptor**（权限委托）：委托分区名 + 被信任的公钥。Google 用它把 `vbmeta_system`、`boot` 等委托给不同密钥，实现「换 system 不动顶层 vbmeta」【证】[1][45 推]。

分区尾部还有 **footer**（魔数 `AVBf`，指向分区开头的 vbmeta），让 `fastboot`/工具无需知道分区语义就能翻找元数据【证】[1]。运行期 dm-verity 挂接：错误处理模式由内核命令行 `androidboot.veritymode`（`eio`/`restart`）决定【证】[5]；dm-verity 本体是按需 4K 块 Merkle 树校验，v1 格式 salt 前置、digest 补零到 2 的幂【证】[29]。

### 3.2 防回滚三件套：rollback index × 单调计数器 × 一次性熔丝

- **vbmeta 侧**：镜像里烧录 `rollback_index`，按 `rollback_index_location` 分槽（出厂每槽一个上限）。验证时 bootloader 读 tamper-evident 存储里的当前值比较【证】[1][2]。
- **tamper-evident 存储三档**：①SoC 熔丝/QFPROM（一次性、单调、最硬）【证】[14]；②eMMC/UFS **RPMB** 分区——512 字节帧、HMAC-SHA256 认证、单调 write counter、密钥只可编程一次【证】[47][48][56]；③独立安全芯片（Pixel Titan M 系列：存「最后已知安全版本」拒绝回退）【证】[54]。OP-TEE 场景下 AVB 的 rollback index 与锁状态就存 RPMB【证】[8]。
- **高通链**：root 公钥哈希烧 QFPROM 熔丝；未烧丝的设备读出默认 PK_HASH（如 `0xcc3153a8…`）= 「非安全态」，链式自洽签名可用**任意自造根密钥**通过——「unfused = 不验根」【证】[13]。镜像证书链 3 张 2048B 证书，内嵌 `SW_ID / HW_ID / OEM_ID / DEBUG` 等字段（SW_ID + 版本号承载软件身份与防回滚语义）【证】[13]。

### 3.3 TF-A Trusted Board Boot：X.509 自签证书 + Arm 私有 OID + NV counter

ARM 通用参考（TBBR, DEN0006）：信任根 = OTP/熔丝里的 **RotPK 哈希** + ROM 里的 BL1。之后全部走 **X.509v3 自签证书**（不需要 CA——信任在扩展字段内容而非签发链），证书分两类：key certificate（装下级签名公钥）与 content certificate（装镜像哈希 + **NV counter 单调计数**）【证】[21]。全部 OID 挂在 Arm 私有节点 `1.3.6.1.4.1.4128.2100` 下，例如【证】[22]：

```
TRUSTED_FW_NVCOUNTER_OID   = 1.3.6.1.4.1.4128.2100.1   （可信世界 NV 计数）
NON_TRUSTED_FW_NVCOUNTER_OID = 1.3.6.1.4.1.4128.2100.2 （非可信世界 NV 计数）
TRUSTED_WORLD_PK_OID       = .2100.302   TRUSTED_BOOT_FW_HASH = .2100.201
SOC_AP_FW_HASH(BL31)       = .2100.603   TRUSTED_OS_FW_HASH(BL32) = .2100.1001
NON_TRUSTED_WORLD_BOOTLOADER_HASH(BL33) = .2100.1201
SP_PKG1..8_HASH            = .2100.1301..1308
```

验证序：BL1 验 BL2 content cert（签发公钥哈希 vs 熔丝 RotPK 哈希）→ BL2 验 trusted key cert → 逐 BL3x key/content cert，任一失败 panic【证】[21]。新形态 **dualroot/CCA CoT**：normal-world 与 secure-world 不同所有者各持独立根（BL33 链根在平台 PROTPK），供「整机厂与 OS 厂互不信任」场景【证】[21][23]。

### 3.4 SoC 引导链两家（手机侧）

- **高通**：PBL（ROM）验 XBL → XBL（EL3）做安全设置 → ABL 收 boot 镜像并跑 AVB → kernel【证】[10]。XBL 本身也是 UEFI 风格多段镜像（高通把它叫「bootloader 也是个小 UEFI」）【证】[10][12]。
- **联发科**：BootROM → Preloader（BL2，验 DA/LK）→ LK（little kernel，跑在 S-EL1，作为 Android bootloader，接 AVB）→ kernel【证】[16][17][41]。解锁态由 `seccfg` 分区承载——fenrir 演示：seccfg 被改后 Preloader 跳过验证，boot 链失守【证】[18]。

### 3.5 A/B 槽 + 启动计数回退（对照 ADR #47 L0）

- **盘上元数据**：slot 后缀分区（`_a/_b`），每槽三态字段：`priority`（优先级，>0 可启动）、`tries_remaining`（剩余尝试次数）、`successful_boot`（成功标记）【证】[7][9][57]。存在 `misc` 分区的 `bootloader_message` 结构里【证】[9][57 推]。
- **boot_control HAL**：OS 侧接口 `markBootSuccessful()` / `setActiveBootSlot()` / `setSlotAsUnbootable()`【证】[7]。流程：update_engine 写完备用槽 → 激活（priority 高 + tries_remaining=N）→ 重启 → bootloader 选高 priority 槽 → OS 起来后自查 → `markBootSuccessful()` 清 tries 并落 successful 标记 → 失败则每次 boot 消耗一次 tries，归零即回旧槽【证】[6][7]。
- **与防回滚的耦合（重要细节）**：AOSP 明文——**槽标 SUCCESSFUL 之后才允许更新 rollback 元数据**；否则一旦先写了回滚计数，旧槽（版本号更低）就再也回不去了，回退机制被防回滚机制反噬【证】[5]。
- **演进**：Classic A/B（分区全双份）→ Android 10 Virtual A/B（快照/CoW，空间按需）→ Android 13 扩大强制面、非 A/B 支持将被移除【证】[45][46]。

### 3.6 UEFI Secure Boot 与度量启动（PC/ARM 服务器）

- **四层密钥库**：PK（平台钥，最高所有者）→ KEK → db（允许签名库）/ dbx（吊销库）。全部是 **authenticated UEFI 变量**（带签名/时间戳），存固件变量区（SPI flash；ARM 设备常以 RPMB/OP-TEE 保护）【证】[34][39][40 推]。
- **PCR 分配（PC Client）**：PCR0-7 固件与引导组件；**PCR7 = Secure Boot 策略状态**（开关、密钥、政策）——BitLocker 默认封在 PCR7+PCR11 上；PCR4 = boot manager。改 dbx/db 会变 PCR7 → 触发 BitLocker 恢复（2026-04 补丁日仍有此运维事件）【证】[43][44][60]。
- **ARM 侧标准**：Arm SystemReady BBSR（Base Boot Security Requirements）要求 UEFI Secure Boot + 变量保护 + 安全固件更新可认证【证】[36][37]。
- **AAVMF/QEMU 现状**：`ArmVirtPkg` 以 `-D SECURE_BOOT_ENABLE=TRUE` 构建 + 双 pflash（CODE+VARS）即可开验签；VARS 模板默认空，需 `EnrollDefaultKeys` 或菜单手工注册 PK/KEK/db；TCG 仿真下有已知挂死 bug（KVM 正常）【证】[37][38][39][40]。**BerryOS 现用 AAVMF 出厂构建不验签，但换用带 SECURE_BOOT_ENABLE 的构建 + 自签密钥即可让「AAVMF 验 BerryBoot」在 CI 里真实成立**【证：机制存在 38][推：默认分发构建未开]。
- **Linux 侧演化**：shim（MS 签名）→ 内嵌 MOK + **SBAT**（把「版本承诺/吊销」做成 shim 可解析的策略段，绕开 dbx 更新的砖机风险）【证】[34]。

### 3.7 Apple：线上 nonce 反重放（与 Android 的关键分岔）

Apple 不用单调计数器当主防线，而是**每次刷机/启动随机 nonce**，签名响应（SHSH/APTicket）绑定 nonce，服务端只对「当前允许的版本×该 nonce」签发——防回滚靠**服务端策略 + 设备端 nonce 校验**双卡【证】[27][28]。Image4 格式里 `bnch`（Boot Nonce Hash）由 SecureROM 与 iBoot 双强制，SEP 侧另有 `snon`【证】[27]。macOS Apple Silicon 同构但根在「仪式化」的信任根注入流程【证】[26]。

---

## 4. 演进史与教训（为什么变成现在这样）

1. **Android：dm-verity（4.4，Verified Boot 1）→ AVB（8.0）**。VB1 每分区独立验签、格式碎片化；AVB 用中心 vbmeta + 标准化 footer + chained partition + rollback index 收编【证】[3]。教训：**中心化签名清单 + 委托链**比逐分区散养更可运维（Google 把 system 族委托进 vbmeta_system 单独滚动【证】[45 推]）。
2. **A/B 的诞生动机是变砖恐惧**：Nexus/Pixel 先试水 → Virtual A/B 砍空间成本 → 13 起强制、非 A/B 将移除【证】[45][46]。教训：**掉电/坏镜像安全是更新的第一性需求**，回退机制要进 bootloader 而不是 OS（OS 挂了谁来回退？）【证】[6]。
3. **Tivo 事件（2006-2007）**：TiVo 用签名链阻止用户跑改版 Linux（GPL 软件），引爆「tivoization」争论，直接催生 GPLv3 反 tivoization 条款；Torvalds 拒绝 Linux 换 v3【证】[58]。业界折中答案 = **用户可设根 + 物理确认 + 换态擦数据**（Android `avb_custom_key` 虚拟分区、yellow 状态 10s 警告；Linux MOK）【证】[4][34]。教训：**verified boot 与用户所有权是政治问题，不是纯技术问题**——解锁面必须 day-one 设计，不能「以后再说」。
4. **BootHole（CVE-2020-10713，2020）**：GRUB2 配置解析漏洞 + shim 被 MS 签名 → 理论上波及几乎所有 Secure Boot 设备。修复陷入「weeks of chaos」：GRUB2 修了但旧版本仍可信，唯一防线 dbx 吊销又会**砖掉旧恢复介质/合法双系统**；最终催生 SBAT 把吊销粒度做细【证】[30][31][32][34]。教训：**第三方 bootloader 进信任链 = 你的 PKI 覆盖面失控**；吊销通道必须低粒度、可灰度。
5. **BlackLotus（2023）**：带一个**合法签名的旧 Windows boot manager**（BYOVBL）+ CVE-2022-21894 绕过，在已修补系统上绕 Secure Boot——证明「签名有效」≠「无漏洞」，**吊销滞后是常态**；微软只能分阶段强制 SBAT/密钥轮换【证】[33]。教训：信任链上的每一环都要有**退出机制（吊销）且退出要能快速执行**。
6. **厂商链被逐个击穿的清单**：高通 XBL 链漏洞分析（Quarkslab 2019）【证】[12]；未烧丝高通设备默认非安全态【证】[13]；MTK seccfg 改写绕过（fenrir）【证】[18]；MTK BootROM 电压毛刺（NCC）【证】[19]；三星×MTK 组合链 bug（SSTIC 2024）【证】[20]；NXP HAB 验签绕过（Quarkslab）【证】[53]。教训：**每级验签的解析面本身就是攻击面**——验签代码要最小化（Apple 把 SecureROM 做得极小是刻意设计【推，基于 26】）。
7. **ChromeOS 初版设计文档自曝的缺口**：早期方案承认 hash 分区可被重放（rollback 保护当时没做全），后补 TPM NV 存 kernel 版本【证】[25]。教训：**防回滚天然是补课项**，架构上要一开始留单调计数器接口。
8. **度量与加密的运维耦合**：PCR7 绑定让 dbx/证书更新触发 BitLocker 恢复（2026-04 补丁日仍在发生）【证】[43][59]。教训：度量面与用户数据解封耦合时，**每一次合法更新都是一次潜在恢复事件**——解封策略的 PCR 选择要留余地。
9. **seL4 的边界声明**：形式化验证明确假设「boot 镜像正确加载」，boot 链在 TCB 之外【证】[50][51]。教训：**内核再正确也救不了被换掉的内核**——这正说明 BerryOS 把度量链（决策 #24）作为独立第三段的定位与业界共识一致。

---

## 5. 对 BerryOS 的启示（v2/v3/v4 落点 + day-one 留缝）

> 对照基线：BerryOS 现状 = 度量启动 v1 留桩 / v2 SMMU+PAC / v3 度量链完整 / v4+ 签名+反回滚（`docs/安全与硬件隔离设计-v1.0.md` §6）；L0 boot slot A/B + 启动计数回退已设计（ADR #47）；引导链 = BerryBoot（UEFI 应用）→ handoff → kernel。

**v2 落点（SMMU 同期）**
- AAVMF 验签链可先行：换 `-D SECURE_BOOT_ENABLE=TRUE` 构建的 AAVMF + 自签 PK/KEK/db，让固件验 BOOTAA64.EFI 真实发生在 CI——BerryBoot 源码零改动，验证面在固件【推，机制见 38/40】。
- BerryBoot 对 KERNEL.ELF/INITRD.BIN 的**加载期哈希校验**（不求签名，先求完整性 + 确定性对拍进遥测）——比度量链更小步、无密钥管理负担【推】。

**v3 落点（度量链）**
- handoff 结构加**度量段**：逐级 hash（BerryBoot 度量 kernel + initrd）扩展进「度量寄存器」（QEMU = fw_cfg/虚拟 TPM 通道，真机 = TPM 2.0 PCR）【推，形态对照 3.6】。
- 度量结果进遥测环（boot 标记面已有 `HANDOFF ok`，顺带打 hash 前 8 hex）——与「AI 测试友好化」天然契合：测试断言度量值漂移【推】。
- 解封/加密将来绑 PCR 时，学 BitLocker PCR7 的教训：**只绑稳定度量位，别绑会因合法更新翻面的位**【推，教训 8】。

**v4 落点（签名+反回滚）**
- 采纳 **vbmeta 式中心清单**：单签名结构装 {KERNEL.ELF hash, INITRD.BIN hash, rootfs 树清单 hash, rollback_index, rollback_index_location, flags}——字段照 §3.1 抄（256B 头 + 描述符 + 尾部 footer），`flags` 的 DISABLED 双位保留（orange 态的实现面）【推】。
- 防回滚存储抽象 = **单调计数器接口**（get/compare/increment），后端三选一：eFuse（真机）/ RPMB（有 eMMC 时）/ QEMU 模拟。三档后端业界全在用（§3.2），接口形状收敛了就不怕换硬件【推】。
- 解锁面按 Android 三件套抄：双向换态擦 user 数据、30s 物理确认、四色状态（green/yellow/orange/red）+ 10s 警告屏——Tivo 教训的直接产物（§4.3）【推】。

**day-one 留缝项（现在就埋，防「后补≠自动生效」）**

1. **`BerryHandoff` v0 的 reserved 区现在就分格**：当前 112B 已有结构，把保留字段切出 `meas_root[32]`（度量锚）+ `sig_slot_off/len`（签名块指针）两个具名槽位——handoff 结构改版要 boot 两端同步换，是全链最贵的「后补」，空槽 day-one 占位成本为零【推】。
2. **`just image` 产链现在就产 hash 清单**：mkimage.py 打包四件套时顺手输出 `out/manifest.txt`（每件 SHA-256 + 尺寸），CI 对拍字节一致性（V-3 确定性打包的顺延）——这份清单就是将来 vbmeta 的数据源，格式冻结从今天开始【推】。
3. **盘上 slot 状态三字段进 FAT 卷设计**：L0 的 `priority / tries_remaining / successful_boot`（§3.5）写成镜像内固定偏移小文件或专用扇区，BerryBoot 读它选槽——现在 QEMU 无害、真机即用，且**「标 SUCCESSFUL 后才更新 rollback 元数据」的顺序纪律**（§3.5 末）一开始就写进设计文档【推】。
4. **rollback_index 字段进 manifest**：哪怕 v1 恒 0——字段缺席后补 = 清单格式 break，字段在场恒 0 = 平滑升级【推】。
5. **unlock 状态位进遥测与 boot 标记**：`androidboot.verifiedbootstate` 的等价物（`berry.verifiedbootstate=green|...`）进 handoff/cmdline——下游（将来 KeyMint 类服务、FDE）第一天就能消费「当前验证态」，不需要内核 ABI 再开洞【推】。

---

## 6. 来源清单

1. AVB README（官方）：https://android.googlesource.com/platform/external/avb/+/master/README.md
2. avb_vbmeta_image.h（字段一手源码）：https://android.googlesource.com/platform/external/avb/+/master/libavb/avb_vbmeta_image.h
3. Android Verified Boot 总览：https://source.android.com/docs/security/features/verifiedboot
4. Device State（锁定/解锁）：https://source.android.com/docs/security/features/verifiedboot/device-state
5. Boot Flow（四色状态机）：https://source.android.com/docs/security/features/verifiedboot/boot-flow
6. A/B seamless updates：https://source.android.com/docs/core/ota/ab
7. boot_control HAL 实现：https://source.android.com/docs/core/ota/ab/ab_implement
8. U-Boot AVB2（RPMB 存 rollback/锁态）：https://docs.u-boot.org/en/v2026.04/android/avb2.html
9. U-Boot A/B（槽元数据）：https://docs.u-boot.org/en/v2023.04/android/ab.html
10. Qualcomm APSS cold boot flow（PBL/XBL/ABL）：https://docs.qualcomm.com/doc/80-80020-4/topic/bootloader-architecture.html
11. Qualcomm Linux Security Guide（镜像签名/烧写）：https://docs.qualcomm.com/doc/80-70015-11/topic/flash-the-images.html
12. Quarkslab：Analysis of Qualcomm Secure Boot Chains：https://www.scribd.com/document/975703787/Analysis-of-Qualcomm-Secure-Boot-Chains
13. 全链利用未烧丝高通设备（PK_HASH 默认值/证书字段）：https://hhj4ck.github.io/qualcomm/secboot-off-qcm2150.html
14. Qualcomm QFPROM 熔丝附录：https://docs.qualcomm.com/doc/80-70017-11SC/topic/appendix-fuse-configurations.html
15. MediaTek Genio IoT Secure Boot：https://genio.mediatek.com/doc/iot-yocto/latest/sw/yocto/secure-boot.html
16. MTK Android 14 Secure Boot（BayLibre 镜像）：https://baylibre.pages.baylibre.com/mediatek/rita/device/mediatek/mtk-android-14/docs/bootloader/secure-boot.html
17. 逆向 MTK LK：https://blog.r0rt1z2.com/posts/reverse-engineering-mediatek-lk
18. fenrir（seccfg 解锁绕过）：https://github.com/R0rt1z2/fenrir
19. NCC Group：Glitching the MediaTek BootROM：https://www.nccgroup.com/research/there-s-a-hole-in-your-soc-glitching-the-mediatek-bootrom
20. SSTIC 2024：Samsung×MediaTek 链式 bug：https://www.sstic.org/media/SSTIC2024/SSTIC-actes/when_vendor1_meets_vendor2_the_story_of_a_small_bu/SSTIC2024-Article-when_vendor1_meets_vendor2_the_story_of_a_small_bug_chain-rossi-bellom_neveu.pdf
21. TF-A Trusted Board Boot 设计：https://trustedfirmware-a.readthedocs.io/en/latest/design/trusted-board-boot.html
22. TF-A tbbr_oid.h（OID 一手源码）：https://github.com/ARM-software/arm-trusted-firmware/blob/master/include/tools_share/tbbr_oid.h
23. TF-A dualroot RFC：https://lists.trustedfirmware.org/archives/list/tf-a@lists.trustedfirmware.org/thread/CFQFGU6H2D5GZYMUYGTGUSXIU3OYZP6U
24. Cloudflare：Armed to Boot：https://blog.cloudflare.com/armed-to-boot
25. ChromiumOS Verified Boot 设计文档：https://www.chromium.org/chromium-os/chromiumos-design-docs/verified-boot/
26. Apple Platform Security Guide（PDF）：https://help.apple.com/pdf/security/en_US/apple-platform-security-guide.pdf
27. Apple PCC Secure Boot Tags（bnch/snon）：https://security.apple.com/documentation/private-cloud-compute/appendix_secureboot
28. iOS chain of trust（Image4/checkm8）：https://sigreturn.com/blog/ios-chain-of-trust
29. dm-verity 内核文档（Merkle 树/错误模式）：https://docs.kernel.org/admin-guide/device-mapper/verity.html
30. Eclypsium：BootHole：https://eclypsium.com/research/theres-a-hole-in-the-boot
31. Red Hat GRUB2 BootHole 页：https://access.redhat.com/security/vulnerabilities/grub2bootloader
32. NSA BootHole 通告 PDF：https://media.defense.gov/2020/Jul/30/2002467902/-1/-1/0/CSA_MITIGATE_THE_GRUB2_BOOTHOLE_VULNERABILITY_20200730_NSA_GOV%20-%20COPY.PDF
33. BlackLotus 防护指引（NSA/BleepingComputer）：https://www.bleepingcomputer.com/news/security/nsa-shares-tips-on-blocking-blacklotus-uefi-malware-attacks
34. Mini-DebConf 2024 SecureBoot 谈（SBAT/dbx 运维）：https://www.einval.com/~steve/talks/Mini-DebConf2024-SecureBoot/SecureBoot.pdf
35. Fedora dbx 更新讨论：https://discussion.fedoraproject.org/t/secure-boot-dbx-update-offer-revisited/78689
36. Arm SystemReady Style Guide（BBSR）：https://armkeil.blob.core.windows.net/developer/Files/pdf/guide/arm-systemready-style-guide.pdf
37. Arm SystemReady FAQ：https://support.arm.com/documentation/107981/0304/Related-information
38. QEMU UEFI variables 文档（pflash 变量保护）：https://www.qemu.org/docs/master/devel/uefi-vars.html
39. libvirt Secure Boot kbase（支持 vs 激活）：https://libvirt.org/kbase/secureboot.html
40. Secure(ish) boot with QEMU（AAVMF 注册密钥流程）：https://www.labbott.name/blog/2016/09/15/secure-ish-boot-with-qemu
41. Wikipedia：Booting process of Android devices：https://en.wikipedia.org/wiki/Booting_process_of_Android_devices
42. Microsoft Learn：PCR7 绑定失败原因：https://learn.microsoft.com/en-gb/answers/questions/5682631/setting-up-device-encryption-reasons-for-failed-au
43. Dell KB：TPM PCR 校验错误触发 BitLocker 恢复：https://www.dell.com/support/kbdoc/en-tw/000204144/tpm-pcr-validation-error-causing-bitlocker-recovery-at-boot
44. mjg59：Avoiding TPM PCR fragility using Secure Boot：https://mjg59.dreamwidth.org/48897.html
45. Android Generic Boot（init_boot 拆分）：https://source.android.com/docs/core/architecture/partitions/generic-boot
46. Esper：Android 13 Virtual A/B 强制面：https://www.esper.io/blog/android-13-virtual-ab-requirement；Virtual A/B 官方：https://source.android.com/docs/core/ota/virtual_ab
47. RPMB 概览：https://www.linuxtoday.com/developer/rpmb-a-secret-place-inside-the-emmc
48. rpmb-mtk.c 驱动（帧长/HMAC 面）：https://github.com/oppo-source/A3-8.1-kernel-source/blob/master/drivers/char/rpmb/rpmb-mtk.c
49. QNX 8.0 Secure Boot：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.security.system/topic/manual/secure_boot.html
50. seL4 FAQ（假设边界）：https://sel4.systems/About/FAQ.html；What the Proofs Assume：https://sel4.systems/Verification/assumptions.html
51. seL4 Summit 2023：Formally Stepping Into The Unverified World：https://sel4.org/Summit/2023/slides/day1-03-frost.pdf
52. Fuchsia zircon 分区/bootloader 源（zircon-{a,b}/Gigaboot）：https://fuchsia.googlesource.com/zircon/+/refs/heads/sandbox/voydanoff/8053/bootloader
53. Quarkslab：NXP i.MX HAB 漏洞：https://blog.quarkslab.com/vulnerabilities-in-high-assurance-boot-of-nxp-imx-microprocessors.html
54. Engadget：Pixel 3 Titan M（防回滚）：https://www.engadget.com/2018-10-17-google-details-pixel-3-titan-m-chip.html
55. Stack Overflow：RPMB 流程（密钥未编程态）：https://stackoverflow.com/questions/69475706/procedure-inquiry-when-using-rpmb
56. OP-TEE RPMB 相关 issue（认证写保护）：https://github.com/OP-TEE/optee_os/issues/6840
57. A/B bring-up 实操（misc/bootloader_message）：http://cfig.github.io/2017/03/28/Bring-up-Android-A-B-system
58. HN：Torvalds 谈反 tivoization 条款：https://news.ycombinator.com/item?id=37136955
59. WindowsForum：2026-04 补丁日 Secure Boot 更新或触发 BitLocker 恢复：https://windowsforum.com/windows-news.4/april-2026-windows-patch-tuesday-secure-boot-updates-may-trigger-bitlocker-recovery.413329
