# SEC-4 密钥管理与加密芯片——业界现状调研

> 2026-10-29 · 对应既有设计：`docs/安全与硬件隔离设计-v1.0.md` §4（v3 度量链 / v4+ 签名反回滚）、ADR #22/#24/#47。本文只写业界现状；「对 BerryOS 的启示」单列 §5。
> 标注：【证】= 一手来源可查（[n] 指向 §6 来源清单）；【推】= 本人推断。

## 1. 概述：本维度解决什么问题、信任边界在哪

**问题**：SEC-3（度量启动）解决「跑的代码可被验证」，本维度解决三件下游事：

1. **密钥保管与运算**：内核被攻破的前提下，密钥仍然不可提取、密码运算仍可信——需要一个 rich OS 之外的「岛」（TEE / secure element / 独立 CPU 子系统）。
2. **防回滚单调状态**：签名验证只保证「这次跑的是签过的」，不保证「不是一年前的旧签包」——需要一个只能单调递增、不能回拨的计数器存储（RPMB / OTP 计数锁盒 / eFuse）。
3. **设备身份与证明**：远程验证方需要「密钥确实长在硬件里」的证据——attestation 证书链 + 度量摘要签名（与 SEC-3 的证明面汇合）。

**信任边界**（业界收敛为同一条线）：

```
rich OS（内核默认可被攻破）──受限 IPC──▶ 安全岛（TEE/SE/SEP）
                                        ├─ 密钥材料永不出岛（出岛的只有不透明 keyblob / 包装密文 / 证书）
                                        ├─ 单调状态只进不出（写计数器/尝试计数器）
                                        └─ 岛的引导自成一链（岛 BootROM → 岛 OS），与主 SoC 引导链平行
```

- 「岛」有三档实现强度，业界已把它**枚举化**（Android attestation 的 SecurityLevel：Software=0 / TrustedEnvironment=1 / StrongBox=2）【证】[3]。
- 单调状态与密钥岛是**两个正交件**：RPMB 在盘上、密钥在岛里，岛借 RPMB 做防回滚存储（Android KeyMint 官方：「trusted metadata storage, usually replay protected memory blocks (RPMB)」）【证】[2]。

## 2. 各家现状对照表

| 平台 | 密钥岛形态 | 密钥服务面（API/HAL） | 防回滚单调状态 | 证明（attestation） |
|---|---|---|---|---|
| **Android** | ARM TrustZone TEE（CDD 9.11 必备）+ 可选 StrongBox 独立 SE（CDD 9.11.2）【证】[5] | KeyMint HAL（Android 12 起，AIDL；前身 Keymaster 0.2→4.1）；keystore2 守护（Rust）+ KeyMint TA 在岛内【证】[1] | RPMB（KeyMint 回滚抗性存储）+ AVB rollback index【证】[2][8][37] | Key/ID attestation 证书链（Google 根→设备）；Android 12+ 远程密钥供给 RKP（DICE/BCC）【证】[3][6] |
| **Apple** | SEP 独立岛：专用 CPU + 自有 L4 内核 sepOS + 加密专属 DRAM + 专属 AES 引擎/PKA + 专属 I2C 安全存储【证】[9] | 无第三方 HAL；Data Protection class keys / Keychain，app 经系统框架间接用岛 | 安全存储组件 gen2 计数锁盒（8-bit 计数 + 最大尝试数，超限毁数据）【证】[9] | 生态内闭环（App Attest/DeviceCheck，本文未深研一手页）；无公开对等「证书链验根」模型【推】 |
| **Qualcomm** | SPU 片上岛（自有 CPU/ROM/RAM/RNG，自有引导链），同 SoC 另有 QSEE/TrustZone TEE【证】[10][11][36] | SPU 跑 KeyMint StrongBox 实例；TEE 级跑 KeyMint【证】[11][12] | SPU 自带 anti-replay 硬件对策 + RPMB【证】[36] | 走 Android attestation 面（SPU290 通过 CC 智能卡级评估）【证】[12] |
| **Microsoft (Windows)** | Pluton：安全处理器集成进 CPU die（AMD/Intel/Qualcomm 平台），可作 TPM 2.0 用【证】[25][26] | TPM 2.0 栈 + Windows Update 更新 Pluton 固件【证】[25] | TPM NV 单调计数器 + UEFI Secure Boot dbx 变量 | TPM quote / 云证明（DH Attestation，见 SEC-3） |
| **Fuchsia** | 无通用 SE/keystore 面（研究未见公开文档）；信任锚 = OTP 一次性存储里的公钥【证】[24] | 无公开 keystore API；密钥面属产品层【推】 | 「backstop version」回滚索引，要求「只能被固件改、内核改不了」的存储【证】[24] | 无 TPM/SE 叙述；FVX = 验签执行（见 SEC-3）【证】[24] |
| **seL4** | 内核零密钥服务；TrustZone 双世界切分（seL4 进 secure world 当 TEE 用，IceCap 先例）【证】[28][29] | 产品自建（sel4-tee 研究项目给远程证明/完整性）【证】[29] | 产品层/平台件 | 形式化验证内核 + 能力系统是「岛内 OS」的候选底座（Apple sepOS 同为 L4 系）【推】 |
| **QNX** | 无自家密钥岛；分阶段 secure boot + TPM 驱动支持【证】[27] | 产品集成 HSM/TPM【推】 | TPM | 安全启动验证为主（车规/功能安全语境）【证】[27] |
| **Linux** | 无内置岛；TPM 2.0 子系统 + OP-TEE（ARM TZ）+ 内核 keyring（trusted/encrypted keys，新信任源接入 TEE） | keyctl + TEE 通用驱动框架；RPMB 子系统 2024 年并入（服务 OP-TEE 早启访问）【证】[18][19] | RPMB 子系统 + TPM 计数器【证】[18] | TPM quote / IMA（见 SEC-3） |
| **物联网（PSA 路线）** | isolation L1/L2/L3 分级 + crypto 隔离件（本文未深研，标注未找到一手页）【推】 | PSA Crypto API | OTP/安全计数器 | PSA Initial Attestation（见 SEC-3） |

## 3. 机制细节（达到「能照着留接口位」的精度）

### 3.1 Android KeyMint：HAL 家族与密钥属性

**HAL 演进表**【证】[1]（每行都是一次能力追加，不是版本号游戏）：

| Android | HAL | 追加能力 |
|---|---|---|
| ≤5.0 | Keymaster 0.2/0.3 | 非对称签名/验签、密钥对生成导入 |
| 6.0 | Keymaster 1.0 | AES/HMAC 对称原语、访问/使用控制、**认证绑定密钥**（auth-bound） |
| 7.0 | Keymaster 2 | **密钥证明（key attestation）**、版本绑定 |
| 8.0 | Keymaster 3 | HIDL 化、ID attestation |
| 9 | Keymaster 4 | **安全元件（SE）**、secure key import、3DES、分区级版本 |
| 10 | Keymaster 4.1 | **early boot key**、StrongBox 设备唯一证明、hardware-wrapped 密钥 |
| 12 | **KeyMint**（AIDL 重写） | ECDH、用户指定证明密钥、limited-use keys；keystore2 守护改 Rust |
| 13 | KeyMint v2 | Curve25519 |
| （映射） | attestationVersion 100/300/500 = KeyMint 1.0/3.0/5.0【证】[3] | — |

**架构分工**【证】[1]：keystore2（用户态守护）存**加密 keyblob**但无法解出密钥材料；KeyMint TA 在安全岛内持原始密钥并执行全部访问控制——「HAL 不在用户态甚至内核态做任何敏感操作」。

**keyblob 语义**【证】[2]：generateKey/importKey 返回「加密且认证的不透明 blob」；`Tag::APPLICATION_ID`/`APPLICATION_DATA` 被从特征里剥除但**密码学绑定进 blob**——错值即用不了。`Tag::ROOT_OF_TRUST` 同样「绑定但不可指定、永不返回」。

**attestation 扩展里的授权列表（AuthorizationList，tag 号 = KeyMint tag 号）**【证】[3]：

| tag | 名 | 语义要点 |
|---|---|---|
| [1] | purpose | 用途（签名/加密/验签/解密/议密） |
| [2] | algorithm | RSA/EC/ML_DSA（后量子已进表） |
| [3]/[5]/[6] | keySize/digest/padding | 算法参数 |
| [303] | rollbackResistance | 删除后不可恢复（→ RPMB 执法） |
| [305] | earlyBootOnly | **仅早启阶段可用** |
| [502]/[505] | userSecureId/authTimeout | 绑定用户认证（SID + 时效） |
| [701]/[702] | creationDateTime/origin | 生成时间/来源（GENERATED/IMPORTED/SECURELY_IMPORTED） |
| [704] | rootOfTrust | 见 §3.2 |
| [705]/[706] | osVersion/osPatchLevel | 版本绑定棘轮（§3.1 末） |
| [708]/[709] | attestationChallenge/attestationApplicationId | 挑战值 / 申请方身份（包名+签名摘要） |
| [710–723] | attestationId* | ID attestation（品牌/序列号/IMEI 等，锁设备专用） |
| [718]/[719] | vendorPatchLevel/bootPatchLevel | Treble 分区级补丁版本 |
| [720] | deviceUniqueAttestation | 设备唯一证明（v≥4） |

**支撑 HAL 家族**【证】[5]：`ISharedSecret`（N 方共享密钥协议，按 boot 派生共享 HMAC key——Gatekeeper 令牌防伪的基础）、`IGatekeeper`（LSKF 认证 + **节流防爆破**）、`IWeaver`（把 LSKF 验证挪到独立 SE 上，**阻断对 Gatekeeper 存储的离线攻击**）、`ISecureClock`（挑战式可信时间戳）、`IConfirmationUI`（安全 UI 确认，令牌由 KeyMint 消费）。操作表上限 16 并发、keystore 用 15、**vold 留 1 槽**（磁盘加密口令用）【证】[2]。

**版本绑定棘轮**【证】[4]：Keymaster 2/3 起 OS 版本+补丁级自动写入新密钥；升级后首次用旧密钥触发 `upgradeKey` 前推并作废旧版 blob；**任何回滚旧系统都会让密钥不可用**。执法在 TAA：bootloader 从 boot image/vbmeta 抽版本信息，**在非安全系统启动前**传给 TA。

### 3.2 Key attestation 证书链与 RKP

**证书链形态**【证】[3][30]：`certs[0]` = 被证明密钥的证书（含扩展），其后直到**根证明密钥证书**。产线模式：「两把钥匙（RSA + ECDSA）及其证书链在产线安全注入设备」；N 起设备上的根由 Google 签（须对 Google 根证书清单验链）。rootOfTrust 上报给验证方的正是引导链结论进岛（见下）。

**KeyDescription 扩展（OID 1.3.6.1.4.1.11129.2.1.17）**【证】[3]：

```
KeyDescription ::= SEQUENCE {
  attestationVersion, attestationSecurityLevel,   // SecurityLevel: Software 0 / TrustedEnvironment 1 / StrongBox 2
  keyMintVersion,   keyMintSecurityLevel,
  attestationChallenge,        // 验证方挑战值（防重放/关联会话）
  uniqueId,
  softwareEnforced  AuthorizationList,             // 软件域授权
  hardwareEnforced AuthorizationList }             // 硬件域授权（含 rootOfTrust[704]）
RootOfTrust ::= SEQUENCE { verifiedBootKey,       // 验引导公钥的 hash（GREEN=厂植根 / YELLOW=用户根 / ORANGE=32 字节零）
                           deviceLocked,          // bootloader 是否锁定
                           verifiedBootState,     // Verified(0)/SelfSigned(1)/Unverified(2)/Failed(3)
                           verifiedBootHash }     // 「受 Verified Boot 保护的全部数据的摘要」= vbmeta digest
```

- rootOfTrust 在 `hardwareEnforced` 里，官方描述为「由安全硬件内的代码采集/生成，平台改不了」；信息「来自 bootloader 或绕过平台信任的安全通道」——**引导链结论如何进岛是各家自有实现**（文档不规定传输机制）【证】[3]。
- 供给信息扩展（OID 1.3.6.1.4.1.11129.2.1.30，CBOR）：`certs_issued`（30 天签发计数，滥用信号）、`validated_attested_entity`（STRONG_BOX/TEE）【证】[3]。

**RKP（远程密钥供给，Android 12+，13/14 起强制）**【证】[6][7]：
- 旧模式的三个病：产线设备唯一证明密钥 = **不可变设备身份标识**（隐私）；批次密钥吊销粒度粗；OEM 得自建 HSM 管签名密钥。
- 新模型 = **DICE**：UDS（设备唯一秘密）→ DK 派生密钥；BCC（Boot Certificate Chain）= DK_pub 起头、逐层 CDI 证书（每固件层一层）到 KM_pub（KeyMint 模块**生成而非派生**的密钥）叶子；全程 COSE/CWT 编码。
- 隐私纪律：BCC 明文**永不出岛**——只以 EEK（端点加密密钥）加密上送，仅持 TEEK 私钥的 Google 供给服务能看明文；服务端刻意分成「知设备身份」与「知待签公钥」两个组件。
- Phase 2 把信任根从 TEE 挪进 ROM：**密钥材料烧进只有 ROM 代码能读的 fuse**。
- 2026 根证书轮换：RKP 设备独用新根，钉死旧根的验证方会破【证】[31]。

### 3.3 StrongBox 分级与 Titan 家族

- 分级**作为枚举值暴露给 app 与验证方**：`SecurityLevel.StrongBox(2)` 出现在 attestation 扩展里；生成密钥时 app 请求 StrongBox 支持（`setIsStrongBoxBacked`），无 SE 设备回落 TEE【证】[3]（回落语义另见[13]）。
- CDD 双轨：TEE 实现 CDD 9.11 必备；StrongBox「跑在专用安全处理器上的可选第二实例」CDD 9.11.2【证】[5]。
- Titan 谱系：Titan M（Pixel 3，独立安全芯片）【证】[14]→ Titan M2（Pixel 6/Tensor 时代，更强加密引擎，支撑 StrongBox/AVB/防钓鱼）【证】[13]→ Tensor 安全核（片上，与 Titan M2 近等强）→ Tensor G5（Pixel 10，2025，新的生命周期安全硬件）【证】[33]。
- **Android Ready SE Alliance**（2021）：Google + SE 厂商（NXP/ST/Infineon/Thales/Kigen 等）统一在通用 SE 上出货 applet 套件——**KeyMint、Identity Credential、Weaver** 等【证】[15]——把「旗舰机专属安全岛」商品化成「任何带 SE 的 Android 都能有的 StrongBox」。

### 3.4 RPMB：协议帧、产线注入、与 TEE 的配合

**协议帧（JEDEC JESD84-B51；eMMC/UFS/NVMe 同构，帧细节设备族各异）**【证】[17][19]：

```
512 字节 RPMB 帧：
  stuff_bytes[196] | key_mac[32](HMAC-SHA256) | data[256] | nonce[16]
  | write_counter[4] | address[2] | block_count[2] | op_result[2] | msg_type[2]
消息类型（OP-TEE 源码常量，证[17]）：
  0x0001 AUTH_KEY_PROGRAM      0x0002 WRITE_COUNTER_VAL_READ
  0x0003 AUTH_DATA_WRITE       0x0004 AUTH_DATA_READ
  0x0005 RESULT_READ           （应答帧 0x0100–0x0400）
命令集（Linux RPMB 文档，证[19]）：WRITE / READ / GET_WRITE_COUNTER / PROGRAM_KEY
```

- **写计数器**：每次认证写单调 +1，参与 HMAC 计算——重放旧帧则计数器不匹配即拒【证】[16][17]。读路径靠 nonce 防重放（设备回随机 nonce + HMAC）【证】[20]。
- **auth key 一次性**：「RPMB key 编程只能在存储设备生命周期里做一次」【证】[19]；key 编程后不可读回【证】[20]。**归属规则：谁能用 key 谁拥有分区**——Linux RPMB 子系统靠「谁的认证密钥匹配」判定 owner，多用户可共存【证】[18]。
- **key 谁注入**：两条现役路线——① OP-TEE 路线：key 从 HUK 派生（`SSK = HMAC_SHA256(HUK, ChipID || "static string")`），首启由 TEE 编程进盘，**key 永在安全世界**【证】[16]；② 产线直注（厂商黑盒流程，一手公开资料少，【推】其为量产主流）。注意 OP-TEE 文档自曝：「目前没有平台真取 HUK，用常量 key 顶」——安全等级名义上是 1000（TEE 级）【证】[16]。
- **OP-TEE 完整密钥层级**（可直接当参考架构）：HUK → SSK（设备级）→ TSK = `HMAC(SSK, TA_UUID)`（每 TA 一把）→ FEK（每文件随机 128-bit AES）；RPMB 路径 AES-CBC + ESSIV；REE-FS（明文文件系统存密文对象）另把 `dirfile.db.hash` 写进 RPMB 当**防回滚钉子**【证】[16]。
- **Linux 侧归属演进**：2016 年首版 patch → 2024 年 RPMB 子系统进主线（`drivers/misc` 核心 + MMC 注册 + OP-TEE 驱动），动机 = **让 OP-TEE 在早启期就能访问 RPMB（不用等用户态）**；帧在内核只透传，HMAC 计算在安全世界【证】[18]；2025-10 UFS RPMB 驱动接续【证】[35]。
- **协议本身翻过车**：Western Digital 报告的 RPMB 重放类漏洞（OP-TEE 安全通告 GHSA-3hg3-f4rp-pr44）【证】[21]；取证学界亦有「近场利用 RPMB 认证」研究【证】[9b]。

### 3.5 eFuse / OTP：公钥根的一次性烧录

- 模式：**BootROM 能读的 OTP/fuse 区存 OEM 根公钥的 hash**，固件头带公钥原件，ROM 验「hash(公钥) == fuse 值」再验签下级【证】[22][23]。
- 一次性门：烧录后 secure boot **永久启用、无法回退到不验签**（Rockchip 指南明示）【证】[23]；Xilinx Zynq 同型（PPK hash 进 fuse，烧后强制安全启动）【证】[23b]。
- DICE Phase 2 同源：**只有 ROM 代码可读的 fuse** 存根密钥材料【证】[6]。
- 与 §3.4 的关系：eFuse/OTP 管「验签根不可换」，RPMB 管「版本只进不退」——一静一动两个单调/不可逆件【推】。

### 3.6 Apple SEP：岛式架构的完成态

**岛本体**【证】[9]：
- 专用 CPU 只跑 Secure Enclave（防侧信道共享）；OS = **sepOS，Apple 定制 L4 微内核**；降频运行抗时钟/功耗攻击。
- **专属加密内存**：Memory Protection Engine 管 SEP 专属 DRAM 区，boot 时 BootROM TRNG 生成临时密钥，AES XEX-MAC + CMAC 标签；A11/S4+ 加**根在专属 SRAM 的完整性树**（anti-replay）；A14/M1+ 双临时密钥（一枚 SEP 私有、一枚与安全神经引擎共享）。
- 专属 AES 引擎（在 DMA 路径上，密钥经专线送入，**硬件内解包、软件永不見明文**）+ 公钥加速器 PKA + 专属 TRNG。
- **UID/GID**：UID 每设备唯一（A9 起**由 SEP 内 TRNG 在设备上生成**，Apple 与供应商都不可见——供应链隐私自断）；GID 按 SoC 组共享。两者 JTAG/调试口均不可读；硬件派生密钥留在 AES 引擎/PKA 内，连 sepOS 都看不到。
- **OS 绑定密钥**（Sealed Key Protection）= UID ⊕ sepOS hash；A13+ Boot Monitor 跑 SCIP（系统协处理器完整性保护）维持运行 hash。

**专属非易失存储（防爆破的基石）**【证】[9]：专属 I2C 总线、只有 SEP 能访问；Secure Storage Component gen2（2020 秋起）的**计数锁盒**：128-bit salt + 128-bit 口令验证器 + 8-bit 计数 + 8-bit 最大尝试值——iPhone 超 10 次即毁口令保护数据。防回滚边界（换口令/改生物登记/换卡/抹机）由 anti-replay 服务标记。

**与 AP 的接口**：iBoot 给 SEP 划专属内存区；AP 把 sepOS 镜像递交 BootROM；BootROM 验 hash+签名后交接，**签名不过 = SEP 挂起直到复位**。数据面：sepOS 用 TRNG 临时包装密钥 wrap **文件密钥**交 AP 文件系统驱动用，AES 引擎硬件内解包【证】[9]。

**SEP 谱系**：iPhone 5s 起全系；T1/T2（Intel Mac 独立芯）→ Apple Silicon 片上岛【证】[9]。

### 3.7 Qualcomm SPU

- **片上岛**：SDA845/855 起独立子系统——自有 CPU（SP-CPU）、ROM、RAM、RNG、加密单元，「功能上等同独立智能卡控制器」，FIPS 140-2 证书 #3549【证】[11]；自有引导链与专属时钟、硬件 anti-replay 对策、隔离内存【证】[36]。
- **认证**：SPU290 通过 Common Criteria（智能卡级保护轮廓方向）评估【证】[12]；媒体称 Snapdragon 855 SPU 拿智能卡安全证书【证】[34]。
- **现役角色**：跑 Android StrongBox（KeyMint 第二实例）【证】[11]；2026 年 CVE-2026-25277（Qualcomm StrongBox 缓冲区溢出）说明**岛上代码同样是攻击面**【证】[12b]。
- 「Sotera 架构」一名：**未找到一手来源**（Qualcomm 公开文档与新闻稿均未用此名；仅社区转述称其为 SPU 架构代号，源自希腊安全女神 Soteria）【推】。

### 3.8 微内核/通用 OS 的「接口位归属」参考

- **Fuchsia**：不设通用 keystore；信任锚 = **OTP 里的公钥**；防回滚要求存储「固件可改、内核不可改」的 rollback index（backstop version）；运行期完整性靠 BlobFS Merkle 树（执行面收敛到包系统）【证】[24]。
- **seL4**：内核零密钥知识；社区路线 = seL4 进 secure world 兼任 TEE（IceCap 跨双世界能力系统）、sel4-tee 研究证明可叠加远程证明【证】[28][29]——**「岛内 OS = 微内核」是 Apple sepOS 已走通的同一条路**【推】。
- **Linux**：密钥/单调状态全在内核外 subsystem（TPM、keyring、RPMB、TEE 总线），内核只做路由与框架——与微内核哲学兼容度高【证】[18][19]。

## 4. 演进史与教训

1. **Keymaster→KeyMint 是接口重写不是升级**：0.2→4.1 七年七版后 Android 12 换 AIDL 全新目录重写（旧目录被官方称为「历史祖先」），keystore2 同步 Rust 重写【证】[1][5]。教训：**密钥服务的 wire 面会背历史包袱**（keyblob 兼容、tag 编号兼容），一次性把「属性表 + 双域（软/硬）+ 不透明 blob」设计对，比七年后重写便宜得多【推】。
2. **产线证明密钥 → RKP 是隐私补课**：设备唯一不可变密钥 = 天生追踪标识；Google 用 DICE 派生 + EEK 加密上送 + 服务端身份/密钥分离组件三件套补救，2012 般的「产线注入一对 RSA/ECDSA」到 2022-2026 全面替换，**2026 根证书轮换还在为旧模式收尾**【证】[3][6][7][31]。教训：**设备身份与密钥供给从 day-one 就要按「短命证书 + 匿名上送」设计**。
3. **RPMB 补课三部曲**：① 协议进 JEDEC 但 Linux 内核 2016 年首提 patch、**2024 年才进主线**——8 年悬置期各厂商各走 tee-supplicant 用户态路由【证】[18][19]；② 协议自身有重放类漏洞（GHSA 通告）【证】[21]；③ OP-TEE「HUK 取不到就用常量 key」的伪安全默认【证】[16]。教训：单调计数器**抽象接口先行、后端可换**（文件→RPMB→eFuse），且「安全等级」要与后端真实强度对齐上报。
4. **版本绑定棘轮**：密钥 blob 里带 osVersion/补丁级、用一次推一档、回滚即全废——防回滚不只靠盘上计数器，还**烙进密钥本身**【证】[4]。教训：反回滚是「引导链版本 + 单调存储 + 密钥绑定」三件套，缺一件就绕。
5. **StrongBox 的商品化路径**：旗舰专属（Titan M）→ 枚举值进 API/证明扩展 → Ready SE Alliance 把 applet 套件铺到通用 SE【证】[5][14][15]。教训：**安全等级分层要作为 API 一等字段**（枚举三档），而不是文档脚注。
6. **Apple 的两次供应链隐私自断**：UID 从「产线烧录」改为「设备上 SEP 内生成」（A9+）【证】[9]——与 Android RKP 动机同源：**厂内环节知道得越少越好**。
7. **爆破防护上岛**：Gatekeeper 节流在 TEE → Weaver 把 LSKF 验证挪进独立 SE 断离线攻击【证】[5]；Apple 计数锁盒超限毁密钥【证】[9]。教训：**口令验证与尝试计数必须住在单调状态旁边**，放在 rich OS 文件里等于没有。
8. **岛不救驱动**：Pixel 10（2026）仍有 0-click 利用链打穿视频编解码驱动【证】[32]——密钥岛保密钥，不保攻击面收敛；两线并行。
9. **eFuse 单向门**：烧后永久锁安全启动、不可回退【证】[23]——量产流程里「先测后烧」是纪律，接口位上则要**默认可开发（不烧）+ 产线开关**。

## 5. 对 BerryOS 的启示

> 现状归 §1–4；本节是建议。原则：v1 零安全硬件（QEMU）也要把「服务协议位 + 单调状态抽象 + 等级枚举」埋好——**后补 ≠ 自动生效**（Android 七版 HAL 重写、Linux RPMB 八年才进主线，都是没留缝的账单）。

**day-one 留缝（现在埋，成本 ≈ 一个协议 crate + 一个 trait）：**

1. **keystored 服务协议位（berry-ksproto，纯函数 crate）**：照 KeyMint 的形——tag 编号表 add-only（purpose/algorithm/keySize/digest/… 从 [1]–[709] 借号段）、AuthorizationList **软/硬双域**、出参一律**不透明 keyblob**（明文密钥永不回传调用方）、`attestationChallenge` 字段 day-one 进 wire。v1 软实现（SecurityLevel=Software），真机岛到位后同协议换 Enclave/StrongBox 后端——**三档枚举 day-one 就有**，学 Android 把等级做成 API 一等字段。
2. **单调计数器抽象（CounterBackend trait）**：`read()/increment()/compare_and_set()`；QEMU 后端 = 宿主侧文件（smoke 确定性），真机后端 = RPMB/eFuse。**ADR #47 的 L0 启动计数回退 + 未来 AVB 式 rollback index 统一走它**——若 A/B 指针与回滚计数 day-one 不走单调接口，v4 接 RPMB 时引导链要重写一遍。
3. **RPMB 归属裁定（写进驱动模型文档）**：用户态驱动模型下 RPMB **帧驱动** = 持证应用（devmgr 面，vblk 同族），但 **auth key 持有者与 HMAC 计算 = keystored**（v1–v3 进程级隔离，同 Linux「内核只透传帧、密钥在 TEE」的拓扑；BerryOS 无 TEE，keystored 就是「安全侧最小近似」）。内核零 RPMB 知识——微内核纪律照 Fuchsia「rollback index 固件可改内核不可改」的精神，只是我们的「固件侧」= keystored。
4. **度量摘要的密钥学消费位**：SEC-3 度量槽产出摘要后，keystored 密钥派生**可绑定摘要**（DICE CDI 式：`K_layer = KDF(K_prev, hash(image_layer))`）——v1 只留派生函数签名的参数位，不做实现。
5. **app/服务密钥访问一律 RPC**：密钥材料不进内核、不进 app 地址空间——v1 软实现也走服务通道（成本可忽略，纪律价值大）。

**v2 落点**：SMMU/DMA VMO（ADR #22）落地时同步 keystored 加密后端真实现（AES/HMAC/Ed25519）+ QEMU vTPM 度量锚打通（docs §4 已留位）；盘加密密钥树（SEC-7 联动）根密钥由 keystored 派生——**KEK 不落盘**。版本绑定棘轮字段进 keyblob（osVersion 位先写 Software 域）。

**v3 落点**：度量链 + 远程证明（ADR #24）——证明密钥**直接采 RKP 式短命证书模型**（学教训 #2：不搞产线长命设备证书）；DICE 式逐层派生把 BerryBoot→kernel→关键服务串成 BerryOS 版 BCC；`verifiedBootState/deviceLocked` 进 keystored 证明输出（rootOfTrust 等价物），引导链结论经**专用通道**（不经 rich OS 转述）注入。

**v4+ 落点**：签名+反回滚闭环——真机 OTP/eFuse 根公钥 hash 位（`mkimage`/产线工具现在就留 key hash 注入参数位，QEMU 路不烧）；RPMB 真后端接管 CounterBackend；StrongBox 分级真机兑现（SE 上的第二 keystored 实例，同协议不同 SecurityLevel）；Weaver 式口令验证上岛 + 尝试计数锁盒语义。

**不建议做**：TrustZone 世界切分（v1/v2 无此需求，QEMU 建模弱）；自造 TEE OS（若 v4 需要岛内 OS，L4 家族路线 sepOS/sel4-tee 已验证，届时评估而非现在动工）【推】。

## 6. 来源清单

1. AOSP — Hardware-backed Keystore：https://source.android.com/docs/security/features/keystore
2. AOSP — KeyMint functions（implementer ref，keyblob/rollback/RPMB/16 槽）：https://source.android.com/docs/security/features/keystore/implementer-ref
3. AOSP — Key attestation（OID 1.3.6.1.4.1.11129.2.1.17 / tag 表 / rootOfTrust / OID .2.1.30）：https://source.android.com/docs/security/features/keystore/attestation
4. AOSP — Version binding：https://source.android.com/docs/security/features/keystore/version-binding
5. AOSP hardware/interfaces security README（android16-release；HAL 家族/ISharedSecret/IGatekeeper/IWeaver/CDD 9.11）：https://android.googlesource.com/platform/hardware/interfaces/+show/android16-release/security/README.md
6. AOSP — Remote Provisioning HAL README（BCC/DICE/EEK/三 phase/fuse）：https://android.googlesource.com/platform/hardware/interfaces/+/9b81cbc7becf87be19a77f109e1b9e3d527ac303/security/rkp/README.md
7. Android Developers Blog — Upgrading Android Attestation: Remote Provisioning (2022-03)：https://android-developers.googleblog.com/2022/03/upgrading-android-attestation-remote.html
8. AOSP — Verified Boot：https://source.android.com/docs/security/features/verifiedboot
9. Apple Platform Security — The Secure Enclave（sepOS/L4、加密内存、UID/GID、计数锁盒、SCIP）：https://support.apple.com/guide/security/sec59b0b31ff/web
   9b. Forensic Science International — Exploiting RPMB Authentication（2024，RPMB 取证利用）：https://www.scribd.com/document/891815088/Exploiting-RPMB-Authentication-in-a-Clos-2024-Forensic-Science-International
10. Qualcomm docs — Secure Processing Unit：https://docs.qualcomm.com/doc/80-PV086-5P/topic/qualcomm-secure-processing-unit.html
11. NIST FIPS 140-2 Security Policy #3549（Qualcomm SPU 硬件模块）：https://csrc.nist.gov/CSRC/media/projects/cryptographic-module-validation-program/documents/security-policies/140sp3549.pdf
12. Qualcomm SPU290 CC Security Target Lite：https://www.commoncriteriaportal.org/nfs/ccpfiles/files/epfiles/NSCIB-CC-2400002-01-ST_Lite.pdf
    12b. ZeroPath — CVE-2026-25277 Qualcomm StrongBox buffer overflow：https://zeropath.com/blog/cve-2026-25277-qualcomm-strongbox-buffer-overflow
13. Google Security Blog — Pixel 6: Setting a new standard for mobile security（Titan M2）：https://security.googleblog.com/2021/10/pixel-6-setting-new-standard-for-mobile.html
14. Google Blog — Titan M makes Pixel 3 our most secure phone yet：https://blog.google/products-and-platforms/devices/pixel/titan-m-makes-pixel-3-our-most-secure-phone-yet
15. Google Developers — Android Ready SE Alliance：https://developers.google.com/android/security/android-ready-se
16. OP-TEE docs — Secure Storage（SSK/TSK/FEK、RPMB 协议流、HUK 常量 key 备注）：https://optee.readthedocs.io/en/3.16.0/architecture/secure_storage.html
17. OP-TEE 源码 — core/tee/tee_rpmb_fs.c（rpmb_data_frame 布局 + 消息类型常量，JESD84-B51）：https://github.com/OP-TEE/optee_os/blob/master/core/tee/tee_rpmb_fs.c
18. LWN — RPMB subsystem patch v8 封面信（2024-08；owner 判定/早启动机/演进史）：https://lwn.net/Articles/985292
19. Linux RPMB 子系统文档 patch（T. Winkler/Intel；命令集/一次性 key 编程）：https://github.com/clearlinux-pkgs/linux-pk414/blob/master/0103-char-rpmb-Document-Replay-Protected-Memory-Block-RPM.patch
20. SkyHigh Memory — Understanding and Using eMMC RPMB（应用笔记，帧字段/nonce/产线编程）：http://www.skyhighmemory.com/download/applicationNotes/Understanding%20and%20Using%20eMMC%20RPMB.pdf
21. OP-TEE Security Advisory GHSA-3hg3-f4rp-pr44（RPMB 重放漏洞）：https://github.com/OP-TEE/optee_os/security/advisories/GHSA-3hg3-f4rp-pr44
22. Bootlin — Rockchip SoCs: a foray into eFuse territory：https://bootlin.com/blog/rockchip-efuse
23. ArmSoM — Rockchip Secure Boot Application Guide（烧录后永久启用）：https://blog.armsom.org/articles/105-rockchip-secure-boot
    23b. Xilinx Wiki — Secure Boot（PPK hash fuse）：https://xilinx-wiki.atlassian.net/wiki/spaces/A/pages/18841708
24. Fuchsia — Verified Execution（FVX/OTP 锚/backstop version/BlobFS Merkle）：https://fuchsia.dev/fuchsia-src/concepts/security/verified_execution
25. Microsoft — Pluton security processor：https://learn.microsoft.com/en-us/windows/security/hardware-security/pluton/microsoft-pluton-security-processor
26. Microsoft — Windows 11 security book: Hardware root of trust：https://learn.microsoft.com/en-us/windows/security/book/hardware-security-hardware-root-of-trust
27. QNX 8.0 — Secure boot：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.security.system/topic/manual/secure_boot.html
28. N. Spinale — seL4 + TrustZone: Spanning both worlds（IceCap，seL4 Summit 2020）：https://nickspinale.com/talks/icecap-sel4-summit-2020.pdf
29. ssg-research/sel4-tee：https://github.com/ssg-research/sel4-tee
30. AOSP frameworks/base — key-attestation.jd（KeyDescription ASN.1 原始 schema/VerifiedBootState 枚举）：https://android.googlesource.com/platform/frameworks/base/+/0da25a6/docs/html/preview/features/key-attestation.jd
31. Baytoid — Key attestation root certificate change（2026 根轮换）：https://bayton.org/android/android-enterprise-faq/key-attestation-root-certificate-change
32. Google Project Zero — A 0-click exploit chain for the Pixel 10（2026-05）：https://projectzero.google/2026/05/pixel-10-exploit.html
33. Google Blog — Pixel 10 introduces new chip, Tensor G5（2025-08）：https://blog.google/products-and-platforms/devices/pixel/tensor-g5-pixel-10
34. ZDNet — Qualcomm Snapdragon 855 SPU snags smart card security certificate（2019）：https://www.zdnet.com/article/qualcomm-snapdragon-855-soc-snags-smart-card-security-certificate/
35. LKML — Add OP-TEE based RPMB driver for UFS（2025-10）：https://lkml.org/lkml/2025/10/8/1032
36. Lantronix — Secure Processing Unit on the Qualcomm SDA845 SoC：https://www.lantronix.com/blog/secure-processing-unit-on-the-qualcomm-sda845-soc
37. AOSP external/avb — avbtool（rollback index location/slot）：https://android.googlesource.com/platform/external/avb/+/21e95266704e572ced1c633bbc4aea9f42afa0a5/avbtool
