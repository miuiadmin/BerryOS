# SEC-3 度量启动与远程证明——业界现状调研

> 2026-10-29 · 对应既有设计：`docs/安全与硬件隔离设计-v1.0.md` §4（v3 度量链 / v4+ 签名反回滚）。本文只写业界现状；「对 BerryOS 的启示」单列 §5。
> 标注：【证】= 一手来源可查（[n] 指向 §6 来源清单）；【推】= 本人推断。

## 1. 概述：本维度解决什么问题、信任边界在哪

**问题**：运行期能力模型（权限/沙箱）的前提是「跑起来的代码本身没被换」。启动链完整性有两种互补执法方式：

- **验签（verified/secure boot）**：每级验下级签名，**不通过就拒跳**——本地阻止，防篡改，代价是锁死升级路径、需要密钥管理。
- **度量（measured boot）**：每级把下级 hash 累加进防篡改寄存器/派生密钥，**照跑不误但赖不掉**——供事后（通常是远程）验证。
- 业界通说：secure boot 是 *保护*，measured boot + attestation 是 *检测*，两者互补而非替代【证】[48]。
- **远程证明（remote attestation）= 度量摘要 + 密钥签名（quote）→ 验证方评估 → 签发信任断言**。Windows DHA、Android Play Integrity、PSA Initial Attestation 都是这个三段式。

**信任边界**（三段）：
1. **度量信任根（CRTM/RTM）**：固件里不可变的第一段代码，它无条件度量下级——一切的前提，它自己靠 ROM/OTP fuse 保证。
2. **度量保管者（信任锚）**：TPM（PCR 寄存器 + EK/AK 密钥）或 DICE（UDS fuse → 逐层 CDI 派生）。防「先跑恶意代码再擦掉测量」。
3. **验证方**：云证明服务（Microsoft DHA、Google RKP/Play Integrity）或企业自有 verifier。度量本身不阻止任何事，没有验证方生态的度量链是摆设。

**组合矩阵（各家选型）**：
| 组合 | 代表 | 特征 |
|---|---|---|
| 验签+度量并行 | Windows（UEFI Secure Boot + Measured Boot）[3]、Android（AVB 验签 + Keymaster RootOfTrust 上报）[21][23] | 双轨互为补充 |
| 只验签不度量 | Fuchsia FVX（页面上无任何 TPM/测量叙述）[38] | 签名链即全部信任，无远程可查摘要 |
| 只度量不阻止 | Linux 通用发行版（TPM 在、Secure Boot 开关看 OEM）+ IMA（默认多数发行版未启用）[15][18] | 度量能力在，部署率是另一回事 |

## 2. 各家现状对照表

| 平台 | 验签 | 度量 | 远程证明产品面 | 信任根形态 |
|---|---|---|---|---|
| **Windows** | UEFI Secure Boot（验签阻止）[3] | Measured Boot（Win8 起，度量 kernel/ELAM/boot drivers 进 TPM PCR）[3] | Device Health Attestation（云/on-prem 服务 → MDM 条件访问）[3][5] | TPM 2.0（Win11 硬件门槛，PC Health Check 校验）[3][51] |
| **Linux** | U-Boot/UEFI Secure Boot（看部署）；IMA appraisal 可做本地验签 [14][16] | IMA（measure 面）+ TPM2 支持；systemd UKI 签名入 PCR11 [7][20] | 无统一产品面——企业/嵌入式自建 verifier；RHEL/OL9、Yocto 有落地指南 [16][17] | dTPM / fTPM（固件）视平台 |
| **Android** | AVB 2.0（bootloader 验 vbmeta 签名，GREEN/YELLOW/ORANGE/RED 四态 + 回滚索引）[21][22] | RootOfTrust（verifiedBootKey/deviceLocked/verifiedBootHash/boot state）进密钥证明证书 [23]；DICE 逐层 CDI（open-dice android17 分支在用）[25] | Play Integrity API（取代 SafetyNet，2024 全停）[30]；RKP 远程派生证明证书链（Android 12 可选 / 13 起强制）[24] | TEE / StrongBox（KeyMint）+ DICE（ROM 级 UDS）[23][25] |
| **iOS/Apple** | 出厂验签链（iBoot，非公开文档化面）【推】 | 无公开平台级度量启动 API【推】 | App Attest（Secure Enclave 产钥 + Apple 签 attestation object）+ DeviceCheck（每设备 2 bit 持久态）——面向 App 开发者的轻量证明，不暴露平台度量细节 [32][33] | Secure Enclave |
| **seL4** | 内核本身不提供——形式验证**不覆盖 boot code**，信任链交给平台固件（TF-A/UEFI）[34][36] | 社区探索：有论文给 seL4 系统做 TPM 度量+证明 [37]；boot interface 尚在 pre-RFC 讨论（目标正是让 pre-kernel 软件可互换）[35] | 无官方产品面【证：官方文档无此叙述】 | 平台自有（无内核级信任锚） |
| **Fuchsia** | Verified Execution（FVX）：不可变引导→AVB 验 ZBI→ZBI 验包管理→BlobFS merkle 树；强调「reboot 即恢复」+ AVB 回滚保护 [38] | **无**——FVX 文档通篇只有 verify，无 measure/attestation [38] | 无 | OTP/不可变硬件锚 [38] |
| **QNX** | 签名链 secure boot（NXP i.MX + HSM 生态常见）【推：公开一手资料未检索到 QNX 官方度量/证明文档】 | 未见公开产品面【推】 | 未见 | SoC eFuse/HSM【推】 |
| **IoT（PSA/Matter）** | PSA Certified 只认证 PSA-RoT（L1 问卷/L2 实验室 25 天/L3 硬件渗透）——认证信任根而非整机验签 [39][40] | TF-M Measured Boot 服务（固件镜像记入轮换计数/PCR 式记录）[42] | PSA Initial Attestation API 1.0（EAT/CBOR token，psa/initial_attestation.h）[41]；Matter 用 PAA→PAI→DAC X.509 链在入网时做 Device Attestation [43][44] | PSA-RoT（隔离 secure world / SE）；Matter DAC 私钥存 SE（或 DICE 派生——绑定细节【推】） |

## 3. 机制细节（到「能照着留接口位」的精度）

### 3.1 TPM 2.0：PCR / extend / EK / AK

- **PCR（平台配置寄存器）**：TPM 内防篡改寄存器，reset 后全 0/全 1，只能 `extend`：`PCR_new = Hash(PCR_old || Hash(event))`——只进不退，重启清零【证】[2][3]。
- **PCR bank**：每个哈希算法一个独立 bank（SHA-1 与 SHA-256 可并存各自 extend）；**激活哪些 bank 是固件的特权操作**（`TPM2_PCR_Allocate` 需 platform authorization，只在早期 boot 可用——OS 侧 `tpm2_pcrallocate` 常年改不动，实测 issue 在册）【证】[4][10]。>24 个 PCR 的 profile 由 TCG PC Client 规定【证】[1]。
- **PCR 编号分配（PC Client / ARM SystemReady 均沿用）**【证】[1][6][8]：

| PCR | 内容 | PCR | 内容 |
|---|---|---|---|
| 0 | 固件代码 | 4 | boot manager 代码 |
| 1 | 固件配置 | 5 | GPT/分区表 |
| 2 | option ROM 代码 | 6 | S4 唤醒事件 |
| 3 | option ROM 配置 | 7 | Secure Boot 策略与密钥 |
| 8–15 | OS 自用（8/9 启动参数；**Linux: PCR11 = systemd UKI 签名、PCR12 = 内核命令行**） | 16–23 | debug/动态 |

  ARM 服务器线（SystemReady BBSR）**不另起炉灶**：合规测试就是核对事件日志累计 SHA256 与 PCR0–7 一致【证】[8]；嵌入式 ARM 侧由 TF-A 的 measured boot（BL2 度量下级镜像）承担【证】[9]。
- **密钥层级**：EK（厂商注入的背书密钥+证书，唯一标识一颗 TPM）→ AK/AIK（attestation key，经 EK 向隐私 CA 换证书，日常 quote 用 AK，避免 EK 直接暴露被跨场景追踪）→ `TPM2_Quote` 用 AK 签 PCR 摘要 + nonce（防重放）【证】[3]。密钥-证书绑定规范另见 TCG《TPM 2.0 Keys for Device Identity》【证】[12 引申文献]。BitLocker 是度量消费的经典形态：VMK seal 到预期 PCR 值，启动路径变则拒绝解密封【证】[3]。
- **ARM 平台 TPM 形态**：
  1. **fTPM（固件 TPM in TEE）**：微软 `ms-tpm-20-ref`（TCG 参考实现，MIT）作为 TA 跑在 OP-TEE 里（`optee_ftpm`）；Linux 侧 `tpm_ftpm_tee` 驱动（CONFIG TCG_FTPM_TEE）经 TEE 总线访问——TI AM64x、AMD 嵌入式、QEMU（OP-TEE CI）均有落地【证】[11][12][13]。
  2. **discrete TPM**：dTPM 芯片（I2C/SPI），服务器/工控常见；TF-A 有 dTPM 度量启动 PoC【证】[9]。
  3. Android 侧 TEE 即 Trusty/高通 QSEE，TPM 角色由 KeyMint 承担（无 TPM 也做证明——见 §3.3/§3.5 的 DICE 路线）【推】。

### 3.2 Windows：Measured Boot + DHA（云证明服务形态）

- Measured Boot（Win8 起）把度量链从「止于 Windows Boot Manager」延长到 **Windows 内核、ELAM 反恶意驱动、boot 启动驱动与安全配置（如 BitLocker 开关、ELAM 签名数据）**；度量止于系统初始态以保护隐私【证】[3]。
- **DHA 两步协议**【证】[5]：① 设备把 DHA-Boot-Data（TPM quote、TCG 事件日志、PCR 值）发 DHA-Service（Microsoft 云或企业 on-prem）；② 服务验证后返回**签名健康报告**，MDM（Intune 等）拿报告做条件访问（不健康 → 拒绝 Exchange/SharePoint 等资源）——验证逻辑集中在服务端，端上零策略。设备默认周期性（约每 30 天）报告一次【证】[5][3]。
- Device Encryption（消费版 BitLocker）的取舍值得注意：不度量精确 hash 而度量「**软件签署方**」，让 OEM 打补丁不触发恢复密钥——**可用性压过精确性**的工程折中【证】[3]。

### 3.3 Android：AVB / KeyMint attestation / RKP / Play Integrity

- **AVB 2.0 / vbmeta**【证】[21][22]：中央数据结构 `vbmeta` = 描述符集合 + 全体签名。三类描述符：`hash`（整分区 hash，如 boot）、`hashtree`（dm-verity 树根 hash + salt + 偏移，如 system/vendor）、`chained partition`（把某分区的验签权委托给另一把公钥）。bootloader 内嵌公钥验 vbmeta 签名 → 信任其携带的所有 hash → 逐分区校验；回滚保护靠 vbmeta 中的 rollback index（防刷旧漏洞版本）；unlock 状态改变 RootOfTrust 上报值。
- **KeyMint 密钥证明**（Android 7.0 / Keymaster 2 起）【证】[23]：`attestKey()` 返回 X.509 证书链，叶子证书带扩展 **OID `1.3.6.1.4.1.11129.2.1.17`**（OCTET_STRING 包 DER `KeyDescription` SEQUENCE：attestationVersion/KeyMint 版本、SecurityLevel（0=Software/1=TEE/2=StrongBox）、challenge、uniqueId、softwareEnforced/hardwareEnforced 授权表）。其中 **RootOfTrust 结构 = {verifiedBootKey, deviceLocked, verifiedBootHash, boot state(Verified-GREEN/SelfSigned-YELLOW/Unverified-ORANGE/Failed-RED)}**——**验证启动结果被焊进密钥证明**，验签与度量在此汇合。ID attestation（Android 8.0）把 IMEI 等标识以 HMAC 形式存 TEE 供证明；Android 14 起强制第二 IMEI。schema 持续加 tag（attestationVersion ≥400 有 moduleHash、≥500 有 mlDsaVariant——ML-DSA 抗量子签名进入 KeyMint 证明面）【证】[23]（版本↔Android 发行版对应关系【推】）。
- **RKP（Remote Key Provisioning）**【证】[24]：工厂预埋私钥+证书（两枚：RSA+ECDSA）的旧模式有两大痛点——工厂密保、按销量预测证书量不准。RKP 流程：工厂只提取设备在安全环境生成的**公钥**上传 Google；设备联网后发 CSR，后端签发**短命证书链（约两个月）**；验证设备公钥的服务器与签发 attestation 证书的服务器**隔离**（防止 Google 把证明密钥关联回具体设备）+ 每 app 不同密钥定期轮换 = 匿名集隐私。Android 12 可选、**13.0 起强制**【证】[24]。运行时模块 Remote Key Provisioning Mainline（`source.android.com/docs/core/ota/modular-system/remote-key-provisioning`）可通过 OTS 更新【证】[24 关联页]。
- **DICE 作为 RKP 的信任根基**【证】[26]：DICE 把信任根从 TEE 缩到「通常是芯片本身」的最小 TCB；历史上 TEE/bootloader 出漏洞只能整批撤销证明密钥，DICE 允许**远程验证当前固件状态**——打过补丁的设备可恢复信任。DICE 链在设备侧构成「on-device PKI」，pVM（protected VM）的 Secretkeeper HAL 用它与 TEE 互通取持久密钥【证】[26]。
- **Play Integrity API**（SafetyNet 2024 全停后的正统）【证】[30][52]：app 请求 integrity verdict → Google 服务端聚合设备完整性（MEETS_DEVICE/BASIC/STRONG_INTEGRITY 等档位）、app 真实性、账号/授权许可信号；Android 13+ 可选用更强的平台密钥证明。争议：Google 服务器是唯一裁判——GrapheneOS 提出直接验证 TEE 硬件证明 token 的 Platform Integrity 替代案、并维护开源 Auditor 走离线验证【证】[31]。**教训：证明的「谁来当 verifier」是生态权力问题，不只是技术问题**。

### 3.4 Linux IMA/EVM

- IMA（2005 年提出并入主线）度量每个 exec/mmap 的文件进内核 measurement list 并 extend 进 TPM（默认 PCR10）；EVM 用内核密钥对安全相关 xattr 签名防篡改；appraisal 模式可做本地验签（无有效签名则拒绝执行）【证】[14][15][20]。
- **部署现实**：机制在主线 ≠ 设备在用。Ubuntu/Debian 默认不开；落地集中在企业（RHEL/Oracle Linux 9 的签名式 appraisal 指南）与嵌入式（Yocto/OpenEmbedded 集成指南）；Fedora 到 F37 还在讨论是否启用 IMA hook【证】[16][17][18]。安全研究亦证明 IMA 保证可被恶意实体削弱（Subverting IMA）【证】[19]。
- 现代增量：systemd 把 UKI（统一内核镜像）签名度量进 **PCR11**，并开发 pcrlock（预测 PCR 值做更稳的 sealing 策略，取代「一升级就触发 BitLocker 恢复」式的脆弱 seal）【证】[7]。

### 3.5 TCG DICE 与 TPM 的分层关系

- **DICE = 无外置 TPM 的度量链**：TPM 需要（相对贵的）独立保管者；DICE 只要「一个 fuse 里的每设备密钥 + 每层一段 KDF 代码」，面向资源受限设备【证】[28][29]。
- **Open Profile for DICE**（Google open-dice，v2.6，Android 在用同源实现）核心语义【证】[27]：
  - **UDS**（Unique Device Secret）：芯片生产时写入 fuse 的随机值，硬件保证 DICE 跑完后不可再读——「可变软件永远摸不到 UDS」。
  - **CDI**（Compound Device Identifier）：`CDI = KDF(UDS, 镜像hash ‖ 配置 ‖ …)`（最小实现就是一把 HMAC）——CDI 是「硬件+这一版软件」的化身，软件变了 CDI 就变。
  - **逐层递推**：第 N 层用上一层 CDI 替代 UDS 角色，混入下一层镜像 hash 派生新 CDI——「每层度量并认证下一层」。任何一层改动，该层及以后所有身份全部改变（没有「改回来」——回滚也换身份）。
  - **双枝派生**：首个转换同时派生 **CDI_Attest**（证明身份，派非对称钥对、层层生成证书）与 **CDI_Seal**（封装密钥，用于把磁盘加密密钥绑到当前软件栈）【证】[27]。
  - **证书链**：UDS 证书（根）+ 每层 CDI 证书（X.509 或 CBOR 两种编码 profile）= 可交远程验证的 DICE 链。UDS 供给三方案：预生成 / 工厂 CA / 按需认证【证】[27]。
- **与 TPM 的分层关系**【推】：DICE 不依赖 TPM 也不取代 TPM——DICE 给「身份派生 + 链式度量」，TPM 给「PCR 聚合 + 丰富密钥协议/授权会话」。Android 是活例：平台无 TPM，用 DICE 做根、KeyMint 做 AK 角色、RKP 做隐私 CA 角色——**把 TPM 三件套（PCR/quote/AIK-CA）用「CDI/本地签名/远程隔离签发」重演一遍**。
- Android 官方 DICE 文档补充：DICE 身份「只能靠攻破 ROM 逃过」，固件身份可远程核验（含 Android 之外的环境）【证】[25][26]。

### 3.6 IoT：PSA / Matter

- **PSA Certified**（Arm 牵头的 IoT 认证体系）：L1 问卷审计（10 安全目标）→ L2 认证实验室约 25 天评估 PSA-RoT（含源码审查）→ L3 加硬件渗透测试（约 10 天）+ SE 变体【证】[39][40]。
- **PSA Attestation API 1.0**：`psa/initial_attestation.h`——`psa_initial_attest_get_token(challenge, token)` 一类同步接口；token 为 **EAT（RFC 9334 Entity Attestation Token，CBOR/COSE）**形态，claims 含设备唯一性与固件度量（来自 TF-M Measured Boot 服务记录的镜像 hash 轮换序列）；配套 iat-verifier 工具与 delegated attestation（把签名委托给更强的环境）【证】[41][42][47]。
- **Matter（CSA）**：合规要求每台设备出厂带 **DAC（Device Attestation Certificate）+ 私钥**，证书链 PAA（根，DigiCert 等运营）→ PAI（产品级）→ DAC（设备级）；commissioning 时执行 Device Attestation Procedure 验证 DAC 链与私钥持有【证】[43][44]。DAC 私钥的保管：SE 为正统；无 SE 设备用 DICE 派生 DAC 私钥是 TCG/生态正在推的轻量路线（与 Open DICE 的「on-demand certification」供给方案吻合）【推，未找到 CSA 规范原文级一手来源】。
- 对嵌入式微内核 OS 最相关的组合拳：**DICE（根）+ EAT（token 格式）+ PSA API 形态（同步 get_token 接口）**。

### 3.7 QEMU 可验性：swtpm / vTPM

- QEMU ARM **virt 机型支持 TPM**：`-device tpm-tis-device`（sysbus TIS，挂平台总线，地址动态分配；PPI 默认开）+ `-tpmdev emulator` 后端接 **swtpm**（独立进程，Unix socket 控制通道负责 reset/init/迁移状态）；另有 ast2600 的 tpm-tis-i2c 变体【证】[45]。
- 标准接法【证】[45][46]：

```
swtpm socket --tpmstate dir=state --ctrl type=unixio,path=swtpm-sock --tpm2 &
qemu-system-aarch64 -machine virt -chardev socket,id=chrtpm,path=swtpm-sock \
  -tpmdev emulator,id=tpm0,chardev=chrtpm -device tpm-tis-device,tpmdev=tpm0
```

- guest 内核 TCG 驱动照常出 `/dev/tpmrm0`，tpm2-tools 全套可用；tpm2-software 官方教程覆盖构建/联调【证】[46]。OP-TEE 的 QEMU CI 也在跑 fTPM【证】[13]。
- **结论**：微内核 OS 在 QEMU 里验度量链 = 现成件拼装（swtpm + TIS 设备 + 固件早期 extend），无需等真机。QEMU/AAVMF 本身不验签——berryos.img 的引导验签面只能靠自建（BerryBoot 内嵌公钥），这点与既有认知一致【推】。

## 4. 演进史与教训

**时间线（day-one 还是补课）**：
- 2005：Linux IMA 提出并入主线——**补课**（Linux 已跑十年才补度量）[14]
- 2012：Windows 8 Measured Boot + Secure Boot 同版落地——**双轨 day-one**（为 Win11 TPM 强制铺路）[3]
- 2014/2019：TPM 2.0 规范 / ISO 11889——算法敏捷（多 bank）修复 1.2 的 SHA-1 锁死 [2][3]
- 2016–2018：Android 7.0 密钥证明 → AVB 2.0 / dm-verity 普及 [22][23]
- 2019–2021：Windows DHA 云证明服务成型（MS-DHA 协议文档开放）[5]
- 2020–2022：Play Integrity 取代 SafetyNet（2024 全停）[30]；TPM 2.0 Key for Device Identity 规范化 [12]
- 2021–2023：Android RKP（12 可选 → 13 强制）——**工厂密钥供给的补课** [24]；Matter 1.0 发布 + PSA Certified 规模化 [39][43]
- 2022–2026：DICE 规范族 + Open DICE，Android 把信任根从 TEE 下沉到 ROM/芯片（RKP 用 DICE；pVM attestation）[25][26][27]；KeyMint schema 持续加字段（至 ≥500 抗量子签名 tag）[23]；Android 16（2025-06 发布）带 Advanced Protection Mode [49][50]

**教训**（每条都有先例背书）：
1. **「后补 ≠ 自动生效」**：IMA 在主线十几年，主流发行版默认仍不开——机制存在与部署生效隔着策略、密钥基础设施与生态意愿三座山【证】[18][16]。度量面必须进验收链（CI/冒烟），否则等于没有。
2. **度量需要验证方生态才有牙齿**：measure-only 不阻止任何事；Windows 把度量、seal（BitLocker）、云证明（DHA）、条件访问串成闭环才成为威慑【证】[3][5]。Fuchsia 反向选择：纯验签零度量也能成立（产品域封闭、签名方即信任方）【证】[38]——**开放生态选度量+证明，封闭生态可只验签**。
3. **工厂预埋密钥是供应链软肋**：Android 弃工厂私钥改远程派生（RKP），动因是密保、证书量预测、以及 TEE 出漏洞后的整批撤销困境——DICE 化信任根让「打补丁即可恢复信任」【证】[24][26]。
4. **隐私是证明系统的第一约束**：EK 不直接当身份用（AK 间接化）、RKP 双服务器隔离 + 短命证书、Device Encryption 度量签署方而非精确 hash——都是「可用性/隐私换精确性」的显式折中【证】[3][24]。
5. **PCR 分配与 bank 激活是固件特权**，OS 改不动——平台契约必须 boot 期定死，事后协商无门【证】[10][4]。
6. **信任根趋小**：TEE→ROM/fuse（DICE）、TPM→无（Android 路线）——最小 TCB 不是口号而是十年演进方向【证】[26][28]。
7. **验证器（verifier）是中心化权力点**：Play Integrity 争议（GrapheneOS 案）说明「谁当裁判」会变成生态政治【证】[31]。

## 5. 对 BerryOS 的启示（现状是事实，以下是建议）

既有设计（`docs/安全与硬件隔离设计-v1.0.md` §4：ROM→bootloader→kernel 逐级度量、信任根派生初始能力包、v3 远程证明、v4 签名+反回滚）方向与业界一致；以下按版本给落点，**day-one 项是现在就要埋的缝**（防 IMA 式「后补 ≠ 生效」）：

**day-one 留缝（v1 现在就做，成本极低）**：
1. **BerryBoot→内核 handoff 页加「测量清单」TLV**：内核 ELF hash + 版本串 + 算法 ID；v1 可以只打串口日志/进遥测环，但结构体位先占（BerryHandoff v0 112B 双镜像已有版本域，追加子块即兼容）。RKP/DHA 的证据载体本质上就是这张表——v3 远程证明时无需改 ABI。
2. **跳转前度量的调用约定**：BerryBoot 在 `br entry` 前算 hash（kfile/init 两段已有字节数，顺手累积 hash）；内核 main 同样为首个用户态服务（svcmgr）留「spawn 前度量」调用点——「先度量再跳转」的顺序一旦定型，v3 补 TPM extend 只是换目的地。
3. **mkimage 确定性 = 度量可钉板**：`just image` 同输入同字节是 BerryOS 独有优势——期望 hash/期望 CDI 可以像 smoke 标记一样写死进测试。建议 day-one 在 image 产链尾打一行 `img: sha256=<…>`，为将来「期望 PCR/CDI 值断言」预铺。
4. **QEMU 度量验证环境先行**：swtpm + `-device tpm-tis-device` 在 ARM virt 现成可用（§3.7 命令照抄）——可先做一个独立 smoke 相（不做功能，只验 TIS 探测 + 一个测试 extend），v3 度量链的验收面就有了。AAVMF 不验签 → 签名面留 BerryBoot 内嵌公钥位（v4）。
5. **syscall 表占位**：参考 31 号「在册未实现→立样本」模式，为 `berry_attest`（本地取度量摘要/quote）留 ABI 号位与文档说明——不实现、不承诺，防 ABI 冻结后再挤号。

**v2（SMMU + PAC/BTI 同期）**：遥测环扩 append-only「度量事件流」（镜像 hash/关键服务 spawn hash），语义对齐 TCG 事件日志（event 描述 + digest）——这是 IMA measurement list 的微内核版；同时把 swtpm 冒烟并入常规关卡。

**v3（度量链完整 + 远程证明探索）**：
- **双路评估并选主**：TPM 2.0 路（swtpm/真机 fTPM-in-TEE、TPM2_Quote 兼容生态）vs **DICE 路**（fuse UDS→逐层 CDI，无 TPM 位的手表/低功耗端唯一可行）。建议手机 rich 档走 TPM 兼容面、手表档走 DICE——两者共用「测量清单」数据结构，只换派生引擎。
- 证明 token 格式直接采 **EAT（RFC 9334）/CBOR**，接口形态参考 PSA `initial_attest_get_token(challenge)`（同步、带 challenge 防重放）——不要自造格式。
- 密钥供给从一开始按 RKP 思路设计（设备只产公钥、签名方隔离），避免工厂私钥债。

**v4+（签名 + 反回滚）**：
- vbmeta 式签名 manifest：mkimage.py 已是单口产链，给 MBR 后追加「manifest 分区/段」（hash descriptor + 回滚索引 + 签名）空间现在就该在分区布局里留 LBA 位——反回滚计数器落 RPMB/fuse（真机）与 swtpm NV（QEMU）。
- app 模型（.bap）的 manifest 已有 berry-manifest，v4 加签名字段即可复用现有解析器骨架。
- 初始能力包与度量绑定（既有设计 §4「不是硬编码 init 特权」）——落地时 RootOfTrust 结构建议直接抄 Android 四字段（verifiedBootKey/deviceLocked/verifiedBootHash/bootState），生态工具可复用。

## 6. 来源清单

1. TCG PC Client Platform Firmware Profile Spec — https://www.trustedcomputinggroup.org/resources/documents/pc-client-specific-platform-firmware-profile-specification
2. TPM 2.0 Library Part 1: Architecture (v1.84) — https://trustedcomputinggroup.org/wp-content/uploads/Trusted-Platform-Module-2.0-Library-Part-1-Version-184_pub.pdf
3. Microsoft Learn — How Windows uses the TPM — https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/how-windows-uses-the-tpm
4. Microsoft Learn — Understand PCR banks on TPM 2.0 — https://learn.microsoft.com/en-us/windows/security/hardware-security/tpm/switch-pcr-banks-on-tpm-2-0-devices
5. [MS-DHA] Device Health Attestation Protocol — https://winprotocoldoc.z19.web.core.windows.net/MS-DHA/%5BMS-DHA%5D-210407.pdf
6. EDK II TCG Trusted Boot Chain — https://tianocore-docs.github.io/edk2-TrustedBootChain/release-1.00/3_TCG_Trusted_Boot_Chain_in_EDKII.html
7. UAPI Linux TPM PCR Registry — https://uapi-group.org/specifications/specs/linux_tpm_pcr_registry
8. Arm SystemReady BBSR — Review the TPM measured boot log — https://support.arm.com/documentation/107981/0305/SystemReady-compliance-testing---BBSR-and-Security-Interface-Testing-FAQ/How-do-I-test-for-Base-Boot-Security-Requirements--BBSR--compliance-/Review-the-ACS-test-result-logs/Review-the-TPM-measured-boot-log
9. TF-A Measured Boot with discrete TPM (PoC) — https://trustedfirmware-a.readthedocs.io/en/latest/design_documents/measured_boot_dtpm_poc.html
10. tpm2-tools issue #3433（PCR allocate 需平台授权） — https://github.com/tpm2-software/tpm2-tools/issues/3433
11. OP-TEE optee_os issue #7207（fTPM 用法） — https://github.com/OP-TEE/optee_os/issues/7207
12. Microsoft ms-tpm-20-ref（TCG 参考实现） — https://github.com/microsoft/ms-tpm-20-ref
13. Trusted Firmware 邮件列表：optee_ftpm + RSA 支持与 QEMU CI — https://lists.trustedfirmware.org/archives/list/op-tee@lists.trustedfirmware.org/thread/5EMTEM4U3JV2XGTJC5QHG4VRMGBBW4CL
14. LWN: The Integrity Measurement Architecture (2005) — https://lwn.net/Articles/137306
15. Linux IMA 项目主页 — https://linux-ima.sourceforge.net
16. Oracle: Enforcing Runtime Integrity Checks in OL9 with IMA — https://blogs.oracle.com/linux/runtime-integrity-checks-with-ima
17. Konsulko: IMA measurement with Yocto — https://www.konsulko.com/ima-measurement
18. Fedora F37 与 IMA 讨论 — https://discussion.fedoraproject.org/t/f37-and-integrity-measurement-architecture/72921/3
19. Subverting Linux' IMA (论文) — https://eckelmeckel.me/publication/2020-subverting-linux-integrity-measurement-architecture
20. kernel docs: IMA Measurements Export and Delete — https://docs.kernel.org/security/IMA-export-delete.html
21. Android Verified Boot — https://source.android.com/docs/security/features/verifiedboot
22. Android Verified Boot 2.0 (AVB README) — https://android.googlesource.com/platform/external/avb/+/master/README.md
23. Android Key and ID attestation — https://source.android.com/docs/security/features/keystore/attestation
24. Android Developers Blog: Upgrading Android Attestation — Remote Provisioning — https://android-developers.googleblog.com/2022/03/upgrading-android-attestation-remote.html
25. Android DICE — https://source.android.com/docs/security/features/dice
26. Applications of DICE — https://source.android.com/docs/security/features/dice/applications-of-dice
27. Open Profile for DICE (specification.md) — https://pigweed.googlesource.com/open-dice/+/HEAD/docs/specification.md
28. TCG DICE 技术页 — https://www.trustedcomputinggroup.org/technologies/dice
29. TCG: Hardware Requirements for a DICE — https://www.trustedcomputinggroup.org/resources/documents/hardware-requirements-for-a-device-identifier-composition-engine
30. XDA: SafetyNet API replaced by Play Integrity — https://www.xda-developers.com/safetynet-api-replaced-by-play-integrity-api
31. GrapheneOS: Platform Integrity as an alternative to Play Integrity — https://discuss.grapheneos.org/d/35593-proposal-platform-integrity-as-an-alternative-to-play-integrity
32. Apple: Establishing your app's integrity (App Attest) — https://developer.apple.com/documentation/devicecheck/establishing-your-app-s-integrity
33. Apple DeviceCheck — https://developer.apple.com/documentation/devicecheck
34. seL4 CAVEATS.md — https://github.com/seL4/seL4/blob/master/CAVEATS.md
35. seL4 Boot Interface pre-RFC — https://sel4.discourse.group/t/pre-rfc-boot-interface/295
36. UNSW CS9242 2024: Formal Verification and seL4（boot code 不在证明范围） — https://cgi.cse.unsw.edu.au/~cs9242/24/lectures/07a-sel4.pdf
37. Secure Boot and TPM Attestation for seL4-Based Systems（学位论文 slides） — https://nickba.dev/thesis/slides.pdf
38. Fuchsia Verified Execution (FVX) — https://fuchsia.dev/fuchsia-src/concepts/security/verified_execution
39. PSA Certified Level 1 — https://www.psacertified.org/getting-certified/device-manufacturer/level-1
40. PSA Certified Level 3 + Secure Element — https://www.psacertified.org/getting-certified/silicon-vendor/overview/psa-certified-level-3-secure-element
41. PSA Certified Attestation API 1.0 — https://arm-software.github.io/psa-api/attestation/1.0/api/api.html
42. TF-M Delegated Attestation Service Integration Guide — https://trustedfirmware-m.readthedocs.io
43. DigiCert: CSA Matter certificates (PAA/PAI/DAC) — https://docs.digicert.com
44. Nordic DevZone: Matter DAC private key protection — https://devzone.nordicsemi.com
45. QEMU TPM Device 规范页 — https://qemu-project.gitlab.io/qemu/specs/tpm.html
46. tpm2-software: TPM2 Device Emulation With QEMU — https://tpm2-software.github.io/2020/10/19/TPM2-Device-Emulation-With-QEMU.html
47. RFC 9334 Entity Attestation Token (EAT) — https://datatracker.ietf.org/doc/html/rfc9334
48. 0x434b: Breaking secure boot without breaking the crypto（secure=保护 / measured=检测） — https://0x434b.dev/breaking-secure-boot-without-breaking-the-crypto
49. Bayton: New in Android 16 for enterprise — https://bayton.org
50. Google Blog: Android 16 is here — https://blog.google
51. Microsoft Q&A: TPM 2.0 与 PC Health Check 兼容校验 — https://learn.microsoft.com/en-us/answers/questions/5563620/i-enabled-tpm-2-0-pc-health-check-says-i-am-good-t
52. Google Play Integrity API — https://developer.android.com/google/play/integrity
