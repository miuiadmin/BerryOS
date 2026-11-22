# SEC-7：磁盘与文件加密——业界现状调研

> 调研日 2026-10-29。范围：Android（FBE/metadata/FDE 消亡史/认证绑定链）、Apple（APFS Data Protection/keybag/SKP）、Linux（fscrypt/LUKS2/dm-crypt/dm-verity 叠层）、Windows BitLocker、Fuchsia（zxcrypt→Fxfs）、QNX（域加密）。纪律：硬事实标【证】（一手来源可查）或【推】（本人推断）；业界现状与「对 BerryOS 的启示」分开写。来源编号 [n] 见文末清单。

## 1. 概述：这个维度解决什么问题、信任边界在哪

磁盘/文件加密（Data-at-Rest Encryption, DARE）解决的是**离线物理攻击**：设备被盗/被弃后，攻击者拆下 flash 直接读。信任边界画在哪，决定了整套架构：

- **密钥不放攻击者能拿到的地方**——高级形态是密钥根本不出安全硬件（Android 硬件包裹密钥：内核只见 wrapped blob [7]；Apple SEP：密钥走专用线到 AES 引擎、软件永不见 [10]）。
- **低熵口令不能裸上**——用户 PIN 熵约 20-30 bit，必须经 TEE/SE 内的慢哈希 + **单调错误计数限速**（Gatekeeper/Weaver [4][5]），否则离线穷举秒破。
- **机密性 ≠ 完整性**——加密不防篡改；完整性由 dm-verity/dm-integrity/FS 校验（BerryOS ADR #27 现代 FS 的 CoW+校验）另行兜底，两者叠层顺序有讲究（§3.4）。
- **删除密钥 = 删除数据**——「销毁 per-file/class key 即逻辑擦除」是移动端 remote wipe 的基础（Apple Class D 明说加密只赚快速擦除红利 [11]）。

**微内核视角的核心张力**：加密挂在 FS 层（per-file/class 粒度，多用户/后台可用）还是块层（实现简单，但首次认证前全盘不可用）。业界十年演进给出的答案是**双层并用**（Android：FBE 在 FS 层 + dm-default-key 兜底元数据 [1][2]），移动端纯块层方案（FDE）已死 [3]。

## 2. 各家现状对照表

| 平台 | 形态 | 密钥粒度 | 密钥层级（自上而下） | 认证绑定 | 根密钥保管 | 现状定论 |
|---|---|---|---|---|---|---|
| Android | FBE（fscrypt）+ metadata encryption（dm-default-key）双层 | per-file（可降为 per-class）| 用户口令→合成口令 SP→CE/DE key→per-file key；另线：Verified Boot→KeyMint→metadata key | LSKF→scrypt→Weaver(SE/TEE 槽限速)→SP；无 Weaver 走 Gatekeeper TEE HMAC | KeyMint in TEE；wrapped key 场景只在 UFS/eMMC 内联加密引擎硬件内 [7] | Android 10+ 起强制 FBE、FDE 淘汰；Android 11+ 强制 metadata encryption；Android 13 彻底移除 FDE [2][3]【证】 |
| iOS/iPadOS (APFS) | 文件系统内建（Data Protection） | per-file / per-extent | SEP UID key→class key→per-file key；keybag 包 class keys | 口令经 SEP 慢哈希缠绕 class key；生物识别只是「已解锁态」的续用 | Secure Enclave（独立芯片/ enclave，密钥不出）[10][12] | 2009 年 iPhone 3GS 起 day-one 内建；A14+ 用 SP800-108 KDF 派生 XTS 双钥 [10]【证】 |
| macOS (Apple silicon) | Data Protection Class C（volume key 形态，≈FileVault） | volume | SEP UID→SKP（度量密封）→volume key | FileVault 用户密码缠绕（macOS 26.4 起默认开 [10]） | Secure Enclave + SKP 动态度量根 [13] | Mac 不支持 Class D；默认 Class C【证】 |
| Linux | fscrypt（ext4/f2fs，FS 层）/ LUKS2（dm-crypt，块层） | fscrypt: per-file/DIR_KEY/IV_INO_LBLK；LUKS2: 单卷 key | fscrypt: 主密钥(≤64B)→HKDF-SHA512→per-file key（nonce 在 inode xattr）| 无内建（userspace 自己做 stretching；LUKS2 Argon2id） | keyring（logon key）；LUKS2 卷密钥由 keyslot 密文包裹 | fscrypt v2 自 Linux 5.4 默认；LUKS2 JSON 头 + 双副本 [8][17]【证】 |
| Windows | BitLocker（块层，filter/卷驱动） | volume | protector(TPM-PCR seal/PIN/恢复密钥)→VMK→FVEK→数据 | TPM PCR 0/2/4/11（经典）或 PCR 7/11（Secure Boot profile）[16]；PIN/USB 启动密钥叠加 | TPM（1.2+/2.0）；无 TPM 时仅启动密钥/密码（不安全，默认禁）[14] | Win11 24H2 起设备加密门槛放宽、自动开但先挂 clear key、恢复密钥托管后转正 [14]【证】 |
| Fuchsia | Fxfs 内建加密（fxfs-crypt 独立组件持密钥） | per-volume（RFC 目标含 per-file 演进） | crypt 服务实例(每解锁卷一个) wrap/unwrap（AES-GCM-SIV）→Fxfs 内部密钥 | 密钥托管在 crypt 实例创建者（accounts 层）| fxfs-crypt 进程（key custody 与 FS 分离）[20] | 早期块层 zxcrypt（minfs+FVM 时代）已被 Fxfs 内建加密替代【证】（README/源码可查；退役时间线【推】） |
| QNX SDP 8.0 | Power-Safe FS（fs-qnx6）域加密 | per-file（按域组织） | master key(外部第三方管理)→domain key→per-file key | 无内建口令链；key 由应用层供给（fs_crypto_key_gen）[22] | 文件系统外（调用方） | mount crypto=enable 不可逆；域 0 恒解锁；120 个域编号 0-119 [22]【证】 |
| seL4 | 不在内核做 | — | — | — | 生态按需（无官方磁盘加密组件） | seL4 保证的是内核完整性基线，DARE 属用户态组件职责【推】 |

## 3. 机制细节（达到「能照着留接口位」的精度）

### 3.1 Android FBE 密钥层级与存放点【证】[1]

```
用户 LSKF（PIN/图案/密码）
  └─(scrypt ~25ms/~2MiB 拉伸)→ stretched LSKF
       ├─[有 Weaver] Weaver 槽：key=hash(LSKF)，value=高熵随机 weaverSecret
       │    SP(合成口令) = 双重加密存储：stretched LSKF+weaverSecret 软件密钥 XOR Keystore 密钥
       └─[无 Weaver] stretched LSKF 充当 Gatekeeper 口令（auth-bound Keystore key 加密）
SP（每用户一份，不可变高熵秘密，LockSettingsService 管理）
  └─AES-256-GCM(SP 派生 key)→ CE key（解锁后可用）/ DE key（Direct Boot 可用）
        每用户两把（CE+DE）；工作资料另有自己的 CE/DE
CE/DE key ─(fscrypt v2: HKDF-SHA512, inode 16B nonce 作 info)→ per-file key
  contents: AES-256-XTS（默认）；filenames: AES-256-CTS-HMAC（Android 14+ 推荐 HCTR2）
  inlinecrypt_optimized 旗标：退化为每 CE/DE 一把内容密钥（迁就内联加密硬件槽位）
  emmc_optimized：IV 压 32bit（JEDEC eMMC 5.2 限制）；UFS 用 64bit
```

**vold 是全部 FBE 密钥的管理者**：密钥在盘上全部密文存放（唯一例外 per-boot key 不落盘）；每把密钥用「不出 TEE 的 Keystore 密钥」AES-256-GCM 再包一层 + 请求防回滚；防回滚不可用时退化为 `secdiscardable` 文件（16384 随机字节的 SHA-512 作 `Tag::APPLICATION_ID`）。盘上落点（解锁依赖链）[1]：

| 密钥 | 盘上位置 | 被谁加密 |
|---|---|---|
| System DE key | `/data/unencrypted` | 明文（信任根 = Verified Boot + TEE）|
| 用户 CE/DE | `/data/misc/vold/user_keys/{ce,de}/<uid>` | System DE |
| Adoptable CE/DE | `/data/misc_{ce,de}/<uid>/vold/volume_keys/<uuid>` | 用户 CE/DE |

**硬教训（接口位级）**：`fileencryption=` fstab 旗标「定义盘上格式、OTA 不可改」[1]——加密模式/密钥粒度是 first-format-only 决策，必须带版本号。KeyMint 必须在 `/data` 挂载前就绪（init `early_hal` 类启动 + `wait_for_keymaster` 挂载闸）[2]——密钥服务的**启动序位**是架构约束。

**一次开机的时间线**（DE/CE 可用性与 Direct Boot 的关系）【证】[1]：

```
上电 → verified boot → KeyMint(early_hal) 就绪 → vold 装 DE 密钥（无口令）
     → /data 挂载 → Direct Boot 桌面（闹钟/电话/无障碍可用，应用走 DE 存储）
     → 用户输 LSKF → LockSettingsService 验证（Gatekeeper/Weaver）→ 解开 SP → vold unlockUserKeys
     → CE 密钥装内核 keyring → 应用 CE 数据可用（应用收到 ACTION_BOOT_COMPLETED 前的解锁广播）
重启不停机（Resume-on-Reboot）：需要 client-side + server-side 双钥协作，是 CE 的第三种解锁路径 [1]
```

这个时序把「认证前系统要能活」变成了密钥层级设计约束——DE（无口令可用）与 CE（口令后可用）的**二分是从 boot 时序倒推出来的**，不是密码学需要。

### 3.2 认证绑定链：gatekeeper → weaver → TEE/RPMB【证】[4][5]

- **Gatekeeper**：口令注册/验证在 TEE TA 内，HMAC 密钥「派生且仅存于 Gatekeeper、每 boot 可重派生」。验证通过签发 auth token（每 boot 的 HMAC key，与 KeyMint 经 TEE IPC 按需共享、不缓存）。用户 SID（TEE 侧用户身份，PRNG 生成）入 HMAC——**不可信重注册会换 SID、毁掉绑在旧口令上的全部密钥**（防 OS 被攻破后改口令偷钥匙）[4]。
- **限速（防穷举核心）**：Gatekeeper 必须「先写失败计数、后验证」，成功才清零；失败计数首选安全存储、可直接用 **RPMB** [4]。Weaver 的限速在 TA 内执行，超时可到 24 小时量级（VTS 测不了长时限速档，要求厂商自测）[5]。
- **Weaver**：定长槽位数组（槽数/key 长度/value 长度由 `getConfig()` 披露，HAL 不定死数值 [6]）；`read(slot, key)` 仅在 key 精确匹配时返回 value；每用户一槽：key=hash(LSKF)、value=高熵 weaverSecret。存放：SE 优先（StrongBox 强制 SE），无 SE 时 TEE（Android 17+ 提供参考实现 `system/weaver/`）[5]。
- **RPMB 本体**：eMMC/UFS 内置的可回放保护分区，读写需预先配置的认证密钥 HMAC 签名，写单调计数器防回卷（攻击者换不掉计数）[25]。它是「**单调计数器 + 认证存储**」的业界标准硬件件。

### 3.3 metadata encryption 与 FBE 并用 + 硬件包裹密钥【证】[2][7]

- FBE 盖不住的东西——目录布局、文件大小、权限、时间戳——由 **dm-default-key**（块层、单 key、boot 时即在场）兜底加密；默认 AES-256-XTS。**Android 11+ 内部存储强制开**；adoptable storage 只要 FBE 开则 metadata encryption 恒开 [2]。
- metadata key 由 KeyMint 包裹，**KeyMint 又被 Verified Boot 保护**（密钥链：度量启动 → TEE 密钥服务 → 卷密钥）。包裹 blob 存**独立 metadata 分区**（推荐 16MB，ext4）——不与数据分区同生死（数据区损坏/重建不动密钥区）[2]。
- 硬件包裹密钥（Android 11+）：存储密钥明文只存在于 UFS/eMMC 主控的内联加密引擎；Linux 内核只见 wrapped blob。硬件每 boot 随机生成临时包裹 key，故内存里捞出来的密钥**重启即废**；硬件另派生 `sw_secret` 给内核做文件名等非内容加密的 KDF。派生 KDF 要求 ≥256bit 强度、SP800-108 counter 模式（默认 kdf1，AES-256-CMAC PRF）[7]。Android 17+ 的 `wrappedkey` 方案走 `BLKCRYPTOGENERATEKEY/IMPORTKEY/PREPAREKEY` ioctl（Linux 6.16）[7]。

### 3.4 verity/integrity 与加密的叠层顺序

- **Android /data 没有 verity**（可变分区，哈希树无处锚定）；system 等只读分区用 dm-verity 且不做加密（verified boot 本身提供完整性）[9]【证】。
- 两者叠用时的正确序【推，机制推导】：dm-verity 的哈希树在**构建期对明文镜像**计算，故读路径必须「先解密、后验哈希」——**verity 设备叠在 crypt 设备之上**（FS → dm-verity → dm-crypt → 原始块）。LUKS2 认证盘加密同理把 dm-integrity 放 crypt 之下（tag 属密文域）[17][24]。
- 教训：**完整性证据要落在攻击者改不了的域**（哈希树根锚在签名元数据/vbmeta，integrity tag 落在加密层之下）。

### 3.5 Apple：class key + keybag + SEP + SKP【证】[10][11][12][13]

- 每文件新建 256-bit per-file key；APFS 进一步支持 **per-extent**（同一文件不同区段不同 key）。A14+/M1+：per-file key 经 NIST SP800-108 KDF 派生「256bit 值 + 256bit 密钥」双料喂 AES-256-XTS；A9-A13 为 AES-128-XTS 拆半 [10]。
- **四类保护类**（开发者按文件指定）：Class A `Complete`（锁屏 10 秒后丢 class key，Face ID/Touch ID 只是续用已解锁态）；Class B `UnlessOpen`（ECDH 一次性协商，写完即从内存抹 per-file key——解决「锁屏时邮件附件还想下载」）；Class C `UntilFirstUserAuthentication`（首次认证后常驻——**第三方应用数据默认类**）；Class D `None`（只缠 UID，存 Effaceable Storage，明赚快速擦除）[11]。
- **keybag**（binary plist）：装全部 class keys。五种：user（正常操作）、device（设备级数据）、backup（PBKDF2 **一千万次迭代**）、escrow（配对同步/MDM 远程清口令）、iCloud Backup（class keys 全部换 Curve25519 非对称形态）[12]。
- SEP 反回滚：A9+ 的 one-time unlock token 缠 SEP **单调计数器当前值** + keybag UUID + SEP UID；attended 更新 token 20 分钟过期、unattended 最多 16 小时、每次锁屏销毁 [12]。
- **SKP（Sealed Key Protection）**：密钥密封绑「度量」——Boot Monitor 量 SEPOS、AP Boot ROM 量 Image4 清单；硬件寄存器度量变 → SMRK 派生值变 → 密钥体系自动「断封」。**前向密封（forward-sealing）**：更新前把密钥预封到「更新后预期度量值」，否则每次 OTA 都会毁钥匙 [13]。这是 BitLocker PCR seal 的动态版。

### 3.6 fscrypt 接口形态（Linux，FS 层加密的内核面）【证】[8]

- 主密钥 ≤64B、必须是真随机（**内核不做 stretching**，口令类秘密由用户态先 Argon2/scrypt）；经 `FS_IOC_ADD_ENCRYPTION_KEY` 进**文件系统级 keyring**（logon key 只进不出）。
- v1 策略（8B 描述符、AES-128-ECB 作 KDF、无主密钥验证）已弃；**v2**（Linux 5.4+）：内核生成 16B master_key_identifier（主密钥哈希）、HKDF-SHA512 派生、inode 16B nonce 存 xattr、`log2_data_unit_size` 支持子块数据单元（6.7+）。
- 旗标族即「密钥粒度谱系」：默认 per-file key；`DIRECT_KEY`（Adiantum，nonce 进 IV）；`IV_INO_LBLK_64/32`（整 FS 一把 key、inode 号进 IV——迁就内联加密硬件最少槽位/窄 IV）。硬件包裹密钥 `FSCRYPT_ADD_KEY_FLAG_HW_WRAPPED`：内核内存中只有 wrapped 形态、仅限 inlinecrypt 挂载 + IV_INO_LBLK 策略、只保内容不保文件名 [8]。

### 3.7 LUKS2 / BitLocker / Fuchsia / QNX 要点【证】

- **LUKS2** [17][18][19]：4096B 二进制头（magic `LUKS`s`ba`s`be`，版本 2，hdr_size，seqid，SHA-256 校验）+ 16KiB JSON 元数据区 + keyslot 区（默认数据偏移 16MiB）；**头双副本**靠 seqid 定新旧（崩溃原子性靠「写新副本、校验过才切换」）。卷密钥（volume key）64B（AES-256-XTS 用满 512bit 双钥），由 keyslot（推荐 **Argon2id**）包裹。LUKS1 的固定二进制头 + PBKDF2 被 LUKS2 换成 JSON + Argon2id，正是为了抗 GPU/ASIC 穷举与元数据演进。
- **BitLocker** [14][15][16]：三级 key：数据 ← FVEK ← VMK ← protector（TPM/TPM+PIN/USB 启动密钥/恢复密码）。VMK 用 AES-CCM 加密 FVEK [15]；TPM 把 VMK 密封到 PCR（经典集 0/2/4/11；Secure Boot 时代 7/11——HLK 强制要求 PCR 7/11 跨电源周期不变 [16]）。**设备加密的 clear-key 引导期**：OOBE 自动开加密但先挂 clear key（等同暂停态），恢复密钥托管到 Entra ID/AD/微软账户后才建 TPM protector、删 clear key——「先有数据后有认证体系」的工程解法，单机本地账户则实际无保护 [14]。
- **Fuchsia fxfs-crypt** [20][21]：加密密钥管理放在 **FS 进程之外的 crypt 服务**（每解锁卷一个实例，handle 在解锁时递给 Fxfs）；wrap/unwrap 用 AES-GCM-SIV；`CryptManagement` 协议做加新钥/切活跃钥/删旧钥。**密钥托管者与文件系统进程分离**——微内核生态里最贴 BerryOS 形态的参考。
- **QNX 域加密** [22][23]：三层 master→domain→per-file；域 0 恒解锁、目录派生域继承（只对新文件）、非空文件改域要走 migrate 解密重加密；XTS 需要 512bit 双钥；mount 时 `crypto=enable` 不可逆。密钥管理明确「第三方（文件系统外）供给」。

## 4. 演进史与教训（谁 day-one、谁补课、为什么变成现在）

演进时间线总览（粗粒度，年分为大致量级）：

| 时间 | Android | Apple | Linux | Windows | Fuchsia |
|---|---|---|---|---|---|
| 2009-2011 | — | iPhone 3GS 硬件加密+Data Protection day-one [10] | — | Vista/7 BitLocker（企业版）| — |
| 2014-2015 | 5.0 FDE 默认开、6.0 强制（可豁免）[3] | — | fscrypt 进 ext4（4.1 起）| Win8 自动设备加密雏形 | — |
| 2016-2017 | 7.0 FBE 引入（Pixel 首发）[3] | SKP 前身机制成形 | LUKS2（Argon2id）定型 [17] | — | zxcrypt（块层）随 Fuchsia 早期落地 |
| 2019-2020 | 10 强制 FBE、FDE 淘汰；metadata encryption（9+）[2][3]；Weaver/StrongBox（9+）[5] | — | fscrypt v2（5.4）[8] | BitLocker To Go/自动加密扩面 | — |
| 2021-2022 | 11 强制 metadata encryption + 硬件包裹密钥 [2][7] | SKP 度量密封成文 [13] | — | Win11 设备加密常态化 | Fxfs + fxfs-crypt 上位 [20][21] |
| 2024-2026 | HCTR2（14+）、dusize_4k（15+）、wrappedkey ioctl 化（17+/Linux 6.16）[1][7] | macOS 26.4 FileVault 默认开 [10] | per-extent 讨论中（fscrypt 文档未收录）| Win11 24H2 自动加密门槛放宽 [14] | Fxfs 为默认 data FS |

1. **FDE 之死（Android 的十年补课）**【证】[3]：FDE 建在 dm-crypt 上、AES-CBC+ESSIV、单把 128-bit 主密钥裹整个 `/data`。死因：①整盘一把 key → 必须先问口令才能挂 `/data`（开机先起 tmpfs 框架、解锁后重启框架——Direct Boot/闹钟/无障碍全做不了）；②多用户/工作资料没有独立密钥可分；③错 30 次直接引导擦盘提示；④中断的原地加密留半加密态只能恢复出厂。Android 5.0 引入、6.0 强制（可豁免）、7.0 起 FBE 并行、**10 起新机必须 FBE、13 彻底删 FDE 代码**。教训：**块层加密的天花板是密钥粒度**——粒度决定「认证前系统能活多少」。
2. **Apple 是 day-one 派**【证】[10]：iPhone 3GS（2009）硬件加密 + Data Protection 同代落地；演进是纵向加深（per-file→per-extent、UID 缠绕→SEP→SKP 度量密封），架构骨架（class key + keybag）17 年未推翻。对照 Android 用 5 年从 FDE 迁到 FBE 才追平——**盘上格式 first-format-only 的特性使 day-one 决策极贵**（Android 官方明说 fileencryption 不可经 OTA 改 [1]）。
3. **密钥从「TEE 里派生」到「硬件里根本不出来」**【证】[7]：KeyMint 包裹（TEE 派生解包）→ wrappedkey_v0（KeyMint `convertStorageKeyToEphemeral`）→ wrappedkey（Android 17+，内核 ioctl 直接与内联加密引擎对话）——每步都在把明文密钥的存在域收窄到「主控硬件内、单 boot 生命周期」。
4. **认证绑定链的分层补课**【证】[4][5]：早期 Gatekeeper 的限速计数在 TEE 安全存储里（RPMB 或文件回退）→ Weaver 引入「SE 槽位 + 指数级长时限速」把穷举成本从「TEE 破解」抬到「SE 物理破解」；StrongBox（CDD 9.11.2）再强制独立 SE。教训：**限速计数的单调性必须硬件背书**（RPMB/SE 计数器），软件计数挡不住离线回卷。
5. **BitLocker 的引导悖论与 clear-key 引导期**【证】[14]：既要 TPM 度量封钥（防离线改引导）、又要无口令顺畅开机（TPM-only 模式 = 拿走硬盘即可解）——折衷出 TPM+PIN。Win11 24H2 全面自动加密后，clear-key 引导期（托管恢复密钥前挂明文钥）成为量产必修课。与 Apple unattended token 16 小时限额 [12] 同源：**「更新/首次设置窗口」是密钥体系最脆的时刻，各家都用短命 token 显式建模**。
6. **Fuchsia 从块层退回 FS 内建**【证-方向】[20][21]：zxcrypt（块层 AES-XTS，FVM 之上）→ Fxfs 内建加密 + 独立 crypt 组件——与 Android FBE 同向：块层加密做不了 per-file/class 粒度，最终都收敛到「FS 层加密 +（可选）块层元数据兜底」双层。
7. **fscrypt v1→v2 的接口教训**【证】[8]：v1 用户自选 8B 描述符、无主密钥验证、AES-ECB 当 KDF——v2 全部推翻重来且**不兼容共存**（同一主密钥禁止跨 v1/v2 用）。教训：密钥派生接口要 day-one 上 HKDF + 内核生成标识符 + 密钥正确性验证。

## 5. 对 BerryOS 的启示

> 现状是事实、启示是建议。对照基线：用户态 fsmgr（FAT12/16 起步）+ 用户态 vblk + ADR #27（目标现代 CoW FS）+ ADR #47（A/B slot OTA）+ 安全三段论（v2 SMMU / v3 度量链 / v4 签名+反回滚）。

**day-one 留缝项（现在就埋，防「后补≠自动生效」）**

day-one 动作清单（按现仓库触点对号）：

| # | 动作 | 触点 | 成本 |
|---|---|---|---|
| A | MBR 分区表预留密钥材料分区槽位（不挂载）| `tools/mkimage.py` | ~十行，零运行时影响 |
| B | 现代 FS inode 布局含 16B nonce + u8 class 域（设计冻结，不实现）| `docs/文件系统设计-v1.0.md`（ADR #27 展开）| 文档改动 |
| C | 「fsmgr 永不持根密钥、密钥经会话句柄」原则入册 | 同上 + 未来 keyproto 设计段 | 文档改动 |
| D | 单调计数器能力接口形态（计数器句柄 + increment/read）| 未来 keymgr/keyproto 设计段 | 文档改动 |
| E | 挂载/盘格式参数显式分「格式域 vs 策略域」，格式域 first-format-only + 版本号 | `docs/文件系统设计-v1.0.md` 挂载参数节 | 文档改动 |

1. **盘格式缝（成本最低、最贵的后补雷）**：Android 官方结论「fileencryption 定义盘上格式、不可 OTA 改」[1]——加密参数是 first-format 决策。现在动作：①未来现代 FS（ADR #27）inode 布局 day-one 含 **16B per-file nonce + u8 保护类** 域（fscrypt v2/APFS 同型 [8][10]，FAT 无处可放、不硬塞）；②`tools/mkimage.py` MBR 分区表 day-one 预留**密钥材料分区槽位**（Android /metadata 16MB 独立分区同型 [2]——现单 FAT16 分区布局下加第二分区项，不挂载、零运行时影响）；③FAT 阶段唯一可做的形态是**整卷元数据加密**（dm-default-key 同型 [2]），但 v1 不实现，只在 vblk→fsmgr 协议里给「数据单元经变换」留一层（见缝 2）。
2. **密钥服务缝（keyproto 的接口形状先于实现）**：业界一致把「持根密钥者」与「FS」分离（vold/KeyMint [1]、fxfs-crypt [20]、QNX 第三方 master key [22]）。BerryOS 落点：未来 keymgr 服务三 op——`WRAP/UNWRAP/DERIVE(label, context)`，**fsmgr 永不接收裸根密钥、只收会话句柄与派生结果**（fxfs-crypt「unlock 时递 handle」同型 [20]）。现在动作：遵循 berry-psproto/netdproto 家族的「严格串行 RPC + ver 域」既有模式，在 FS 设计文档冻结该原则即可，协议 crate 到 v2 再立。
3. **单调计数缝（Weaver 的根不在算法在计数器）**：限速计数必须单调且软件回卷不可伪造（RPMB [4][25]）。QEMU 无 RPMB——v2 软件桩 = 密钥材料分区内 append-only 计数日志；**接口 day-one 定义成「计数器句柄 + increment + read」能力而非文件读写**，真机换 RPMB/SE 时协议零漂移。
4. **挂载参数缝**：加密模式/密钥粒度/cipher 进挂载/盘格式参数时，学 `fileencryption=` 打「first-format-only + 版本号」标（BerryOS V-2「簇数=版本域单一事实源」同哲学——BPB fstype 串不读、几何域单一事实源）。

**v2 落点（SMMU/PAC/BTI/MTE 同批）**

5. **认证绑定链最小闭环**：锁屏口令 → scrypt 拉伸 → keymgr 内验证（gatekeeper 同型：验证在密钥服务进程内、auth token 经 per-boot HMAC）→ 解锁用户 CE 域密钥。DE/CE 双域语义 day-one 进 API（Direct Boot 的教训：认证前系统要能活 [1][3]）。软 keystore + 单调计数桩支撑 QEMU 全链验证（§4-7）。
6. **FAT 阶段元数据加密可落**：fsmgr 与 vblk 之间叠一个 crypt 变换层（用户态 dm 同型）整卷单 key——验证「块层缝 + 密钥流」而不动 FAT 格式；现代 FS 到位后降级为元数据兜底层（Android 双层并用形态 [2]）。

**v3 落点（度量链同批）**

7. **密钥派生绑度量**：metadata key/CE key 的 wrap 密钥从度量值派生（BitLocker PCR seal [14][16] / Apple SKP SMRK [13] 同型）——度量不符 = 密钥自动不可用，与安全设计 §4「信任根派生自度量」直接咬合。**必须带前向密封设计**（SKP 的 forward-sealing [13]）：ADR #47 A/B slot 下，把密钥预封到「下一 slot 的预期度量」，否则每次 OTA 毁钥匙。

**v4+ 落点（签名+反回滚同批）**

8. **硬件根替换软件桩**：RPMB/SE 到位后，软件计数桩换硬件计数器（接口已按缝 3 对齐、零协议改）；密钥明文存在域收窄到「安全硬件内」的路线图对照 Android wrappedkey 演进 [7]——v4 前不必追，但接口别堵死（UNWRAP 的出参形态允许「句柄」而非「字节」）。

**QEMU 可验性结论**：无 RPMB/TEE 的 QEMU 里，①算法层（XTS/HKDF/包裹格式）与②格式层（key blob/密钥分区布局）可**全验**；③wrap/unwrap RPC 面用软件 keystore 桩**全验**（业界先例：Android 官方生态长期依赖软件 KeyMint/Gatekeeper 实现跑通 FBE 全链——HAL 接口与硬件实现解耦是设计前提【推】，一手文档未见模拟器专页、如实注明）；④度量 seal/单调计数只能验**接口形状与拒绝路径**（度量不符拒解封、计数回卷拒服务），硬件背书的防回卷强度 QEMU 结构性验不了——留真机/虚拟 TPM（安全设计 §4 已定 QEMU 用虚拟 TPM 度量）。

**未找到一手来源的面（如实注明）**：

- QEMU/Cuttlefish 下虚拟 Trusty/RPMB 仿真的官方架构文档——搜索只命中 Cuttlefish 以 crosvm 跑 protected VM 的零散线索，未见「虚拟 RPMB 设备 + KeyMint 软实现」的官方页；本文 QEMU 可验性结论中相关表述标【推】。
- Weaver 槽数/key/value 具体字节长与限速档位表——HAL 规范刻意不定值（`getConfig()` 披露）[5][6]，各家实现私有；第三方资料有 Titan M 量级数据但非一手，不引。
- Fuchsia zxcrypt 的正式弃用时间点（RFC-0136 未明说 zxcrypt 退役）——只证「Fxfs 内建加密已是现行形态」[20][21]。

**排障/工程坑速记（跨家共性，给未来实现期参考）**【推】：

- **密文盘上无垃圾可读**：加密后 fsck/取证工具全部失效——QEMU 验证盘面必须先解锁再比对话（BerryOS 五相 smoke 的「盘字节零漂移」断言在加密盘上要改造成「同密钥同明文同密文」形式，密钥本身不入 canon）。
- **首次解锁前的死锁环**：KeyMint 依赖存储（密钥 blob 在 /metadata 或 /data）、存储加密依赖 KeyMint——Android 用独立 metadata 分区 + early_hal 启动序破环 [2]；BerryOS 密钥材料分区必须放在**未加密**的独立分区。
- **挂载参数与盘格式纠缠**：`inlinecrypt`（性能开关）可 OTA 切换、`fileencryption`（格式定义）不可 [1]——设计挂载参数时显式分「格式域」与「策略域」两类，格式域 first-format-only。

## 6. 来源清单

1. Android FBE（AOSP 官方）：https://source.android.com/docs/security/features/encryption/file-based
2. Android metadata encryption / dm-default-key（AOSP 官方）：https://source.android.com/docs/security/features/encryption/metadata
3. Android FDE 机制与移除（AOSP 官方）：https://source.android.com/docs/security/features/encryption/full-disk
4. Gatekeeper（AOSP 官方）：https://source.android.com/docs/security/features/authentication/gatekeeper
5. Weaver（AOSP 官方）：https://source.android.com/docs/security/features/authentication/weaver
6. IWeaver HAL 接口定义（AOSP 源码）：https://android.googlesource.com/platform/hardware/interfaces/+/refs/heads/master/weaver/1.0/IWeaver.hal
7. 硬件包裹密钥（AOSP 官方）：https://source.android.com/docs/security/features/encryption/hw-wrapped-keys
8. fscrypt 内核文档：https://docs.kernel.org/filesystems/fscrypt.html
9. dm-verity 内核文档：https://docs.kernel.org/admin-guide/device-mapper/verity.html
10. Apple Data Protection 概览：https://support.apple.com/guide/security/data-protection-overview-secf6276da8a/web
11. Apple Data Protection classes：https://support.apple.com/guide/security/data-protection-classes-secb010e978a/web
12. Apple keybags：https://support.apple.com/guide/security/keybags-for-data-protection-sec6483d5760/web
13. Apple Sealed Key Protection (SKP)：https://support.apple.com/guide/security/sealed-key-protection-skp-secdc7c6c88e/web
14. BitLocker Overview（Microsoft Learn）：https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/
15. BitLocker FIPS 安全策略（NIST CMVP #1053，VMK/FVEK 关系）：https://csrc.nist.gov/csrc/media/projects/cryptographic-module-validation-program/documents/security-policies/140sp1053.pdf
16. BitLocker HLK 测试 PCR 7/11 不变性要求（Microsoft Learn）：https://learn.microsoft.com/en-us/windows-hardware/test/hlk/testref/5aac52da-a57c-4a12-aaac-de878a62bb08
17. cryptsetup / LUKS2 规格（GitLab）：https://gitlab.com/cryptsetup/cryptsetup
18. LUKS2 头 4096B（佐证）：https://security.stackexchange.com/questions/227361/what-is-a-luks-cipher-name
19. LUKS2 双头副本（佐证）：https://security.stackexchange.com/questions/227362/why-is-my-luk-header-listed-twice
20. fxfs-crypt README（Fuchsia 源码）：https://fuchsia.googlesource.com/fuchsia/+show/601f9a5693a9ace1a72e5fef9a86a8b8bfdda7a3/src/storage/fxfs-crypt/README.md
21. RFC-0136: Fxfs（Fuchsia 官方）：https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0136_fxfs
22. QNX Power-Safe FS 加密架构（QNX 8.0 官方）：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.neutrino.sys_arch/topic/fsys_encryption.html
23. QNX fsencrypt 工具（QNX 8.0 官方）：https://www.qnx.com/developers/docs/8.0/com.qnx.doc.neutrino.utilities/topic/f/fsencrypt.html
24. APFS 密钥管理分析（第三方，佐证）：https://eclecticlight.co/2026/09/30/how-does-apfs-handle-encryption-keys
25. RPMB 机制详解（第三方工程博客）：https://sergioprado.blog/rpmb-a-secret-place-inside-the-emmc
26. Android Platform Security Model 论文（arXiv）：https://arxiv.org/html/1904.05572v3
