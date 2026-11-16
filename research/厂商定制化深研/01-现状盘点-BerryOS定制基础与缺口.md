# 01 号底稿：BerryOS 现状盘点——厂商定制的基础与缺口

> **采集日期**：2026-11-10
> **性质**：研究底稿（证据供给，非立法）。为「厂商定制化」深研系列第 01 篇；后续立法稿（ADR/设计文档）引用本文时以各事实的原始出处为准。
> **消费面**：后续「OEM 定制面立法」「兼容性测试套件」「OEM 身份与签名链」「产品配置层」等立法/设计稿的证据基座；本篇只盘点现状与缺口，不替拍板（开放问题原样列于 §5）。
> **视角**：一家 OEM 厂商（手机/穿戴/PC）**今天**就想基于 BerryOS 出自己设备的设备商视角。
> **版本**：v1.0（文件名按任务指定不带版本号，版本记于本行）。
> **来源索引**：见文末「来源索引」；仓库内引用一律给相对路径。

---

## §0 摘要（一屏版）

- **架构方向对 OEM 极友好**：FS/网络/驱动/显示/电源策略全是用户态服务，消费者只认 wire 协议不认实现；调度「机制在内核、策略可插拔」有真实接缝（SchedClass trait + 用户态策略注入表 + intent 词汇表 + PM/thermal 票制）。
- **防碎片化的「结构解」已在架构层立法**：L0 syscall 永冻 + wire add-only + 能力模型唯一机制 + 应用静态链接自兜底（无稳定中间 ABI 可供 OEM fork 出碎片）+ 三值版本域把兼容问题拦在工具期 + 四层 OTA 设计冻结（user/ 永不动）。
- **但产品工程面几乎空白**：无产品配置层（rootfs 条目与镜像组装硬编在 justfile/svcmgr 源码）、无 OEM 身份与签名链实现（SDK-2 分发签名链「未开工」）、无兼容性测试套件（smoke 是开发冒烟非认证面）、驱动交付格式（type=driver/devmgr）与路径命名空间（v3）均未落地、真机窗口（P8）未开、SDK v1 确定不外发。
- **一句话总评（推断）**：今天的 OEM 定制 = fork 仓库改 Rust 源码重编——这恰是安卓 OEM 老路的入口。BerryOS 冻结面设计是「让 OEM 只能改上三层」，但「上三层怎么改」的合法通道（配置层/CTS/签名链）尚不存在。

---

## §1 天然定制点（架构已给出接缝的面）

### 1.1 调度策略可插拔：接缝在哪

接缝不是一个，是一组四层，全部现役或已立法：

1. **内核内调度类 trait（冻结签名、可多实现）**。`sched/src/class.rs:8` 明注「调度策略接口（**冻结**——签名一字不改）」：机制（runqueue/switch/timer tick）留内核，策略做成可注册 trait（class_rt/class_fair/class_deadline/class_idle）。现役实现 = Fair（EEVDF，1.1/S2 起；`RoundRobin` 为兼容别名）+ deadline 带（S5，`PRIO_DEADLINE=1000`，天然 EDF）。ADR 重议触发器表（`docs/架构决策.md` §重议触发器）立法：**只许改实现不许改 trait 签名**；第二策略（如按 intent 实例化 ×4 带）须同一 trait ×N 实例。「数字进启动配置参数表不写死」是 ADR #42 的既定纪律——**该「启动配置参数表」的机制面形态（谁写、内核哪里读）在仓库中未见实现【待核实】**（`sched/src/` 现无外部参数表加载路径）。
2. **用户态策略注入表（带内，AI/PM 共用底座）**。`sched/src/policy.rs`：三槽 RCU 原子换表，决策路径 O(1) 查表；**动作空间四类封顶**——①放置 hint ②频率/热限 hint（走 PM 服务、不经调度路径）③包络内权重微调（`WEIGHT_MAX = ±64`，内核夹紧）④EPP 二值——「没有第五类」（尤其没有直接改 RT 优先级/创建/杀任务）。空表 = 纯启发式（逐位等价，ktest 断言）。头注明示这是 **v3 `policy_bind`（0x83）运行时换表协议的机制底座提前落地**——即：用户态策略服务换表的正门已建机制、协议号预留，运行时协议未开。ADR #21 立法干涉面限定**带内**（band/intent 不入动作空间）+ kill criteria。
3. **intent（u8）第一类调度输入，六档词汇表冻结**。ADR #42 两段式：第一段（v1/v2）intent 仅是查表键（IntentProfile：latency-nice/EPP/uclamp/timer slack 档/maintenance_eligible）；第二段（M4/v3）class_fair 按 intent 实例化 ×4 带，凭 v1 遥测证据入场。编码冻结（`IPC设计-v1.0.md` §2.1/4-3c 与 `user/berry-manifest/src/lib.rs:140`「六档冻结词表」互证）：0x00 UNSPECIFIED（=最高可用档）/0x01 LC/0x02 交互/0x03 UTILITY（推断缺省）/0x04 BG/0x05 MAINTENANCE（编码冻结不启用）；**0x06..0x0F 保留拒绝**。intent 随消息传播（msg_intent 头 + PI 捐赠五件套，4-3c 已落地）、单一消费轴（调度/生命周期/电源三处共用一个声明，ADR #43/#58）。
4. **电源/热/充电策略 = 用户态票制服务**。ADR #66（berry-thermal：独立常驻服务、step_wise 查表+迟滞、**帽不新开通道**——频率帽唯一合法书写面 = berry-power 同一内核聚合点取 min）、ADR #67（挂起双钟 MONO/SLEPT）、ADR #68（电源 SDK 六面**新增 syscall=0**、豁免位先于 AI 票、KC-P1..P5 kill criteria、充电限流票）。策略数值面大量进「不承诺面/策略表可调+遥测导出」（ADR #68 ⑥、应用规范 §4.2 短时任务配额）——这正是 OEM 调参的天然缝。仿真验证面在役（smoke RX 相 SIMBAT/PWRKEY 标记，`tools/smoke.sh:48`）；**berry-power/berry-thermal 服务本体的完备度【待核实】**（立法三件 2026-11-07 落册，落地排期随 PM 三期）。

**OEM 能定制什么（推断）**：策略参数（IntentProfile 档位、配额初值、thermal 表）、用户态策略服务（经 policy 注入表/PMS 票制）、未来按 intent 实例化的带配置。**不能**：改 trait 签名、扩 intent 词表、越包络改权重、直接操纵优先级（四类动作空间之外无门）。

### 1.2 用户态服务（FS/net/drivers）能否被厂商替换

**结构面：可以，且是被设计的**。

- 消费者只认协议不认实现：`docs/驱动模型设计-v1.0.md` §1「消费者视角无『驱动』概念——fsmgr 只认 `/svc/disk/blk0` 的 channel，对面是 virtio-blk、NVMe 还是 RAM 盘，它不知道也不需要知道（现役 fsmgr↔vblk 已是这个形状）」。FS 可换（ADR #27：FAT 仅 MVP，目标现代 FS——「FS 是用户态服务可换」原文）；网络栈可换（vnet + berry-netproto/berry-netdproto/berry-tcp 协议 crate 分层）。
- 协议演进纪律：wire 协议 = L1 冻结面（add-only + per-interface version，API治理章程 §一 + ADR #54/#59）——厂商实现只要说同一 wire 协议即可替换。
- 失败隔离与热换：#28 代号链（handle=(obj,gen) + DEAD，服务可崩可热换）+ appd/svcmgr 监督重启（ADR #46 §3 装卸/崩溃/热换全走应用链表）。
- 现役用户态服务清单（`justfile` build-user + rootfs 装配）：fsmgr（RAM-fs + FAT12 经 vblk）、vblk（virtio-blk）、vnet（ARP/ICMP/DNS/TCP）、gui-inputd（virtio-tablet）、guifbd、gui-wmd、berry-launcher、bsh、uutils 23 命令族、berryctl/ps 等——全部为 rootfs cpio 条目、ELF、svcmgr 流式拉起（`justfile:309-368`）。

**工程面（推断，关键限制）**：替换 = **改 svcmgr 源码重编**。服务启动编排是 svcmgr `main.rs` 里手写的「预拉 arm」（`user/bins/svcmgr/src/main.rs` vblk :939 / guifbd :1015 / gui-wmd :1109 / gfxdemo :1213 / fsmgr :1292 等；端点配线 = b-2 案「svcmgr 持端点对 + 预拉链直递」，`docs/GUI-2段③消费侧首刀实施设计草案-v1.0.md` §7-①）。数据驱动的装卸（appd/devmgr）是 v2 计划（`docs/驱动模型设计-v1.0.md` §5 阶段表）。boot 关键豁免件（GIC/timer/早期 UART/console 服务循环）暂留内核（ADR #12 + 驱动模型 §4 豁免清单，逐版外提）。

### 1.3 路径命名空间能否重组

**立法在、运行面未建**。

- 决策 #6：服务发现 = 路径式命名空间（Plan9/Fuchsia 风，`/svc/fs`、`/dev/console`），namespace 服务持根、mount 挂载、init 按能力派生（`docs/IPC设计-v1.0.md` §5）。决策 #45：「卷 = 命名空间挂载点非块设备」、四棵树（sys/apps/user/tmp）。
- 现役现实：**v3 才实现**。IPC设计「阶段化目标」：v1/v2 先纯能力传递跑通服务互联，namespace 作为可用性层 v3 后加。现役服务发现 = svcmgr 预拉链端点直递；ADR #61 明文「路径式命名空间 #6 仍延后多 app 并发刀再独立 ADR」。
- 推断：OEM 今天重组命名空间（改挂载点、重排服务树）**做不到**——没有 mount 的运行面；未来「挂载策略」会落在 namespace 服务的策略面（开放问题，§5-Q6）。

### 1.4 GUI 栈（berryui）换肤/换系统应用的边界

GUI 栈分层与各层可换性（ADR #50–#54/#61/#63 + `docs/应用规范-v1.0.md` §3）：

| 层 | 现役实现 | 可换性 | 边界（禁改面） |
|---|---|---|---|
| 显示驱动契约 | HDI 三口（reg_vblank/prepare/commit）——ramfb 第一实现、virtio-gpu 第二（ADR #50「换实现不换契约」） | 厂商换显示后端 = 换驱动实现，协议不动 | 像素格式烧进常量 = 移植性死刑（#50 禁） |
| 呈现服务 | guifbd（纯呈现域：present 六 op + hdi 四 op + console 路） | 后端序列已排（soft→virtio-gpu 2D→native Vulkan，ADR #51） | FBW/FBSERVE CI 面零演进污染（#61） |
| 窗口策略 | gui-wmd（权威窗口表 + Z 序 + 避让 + 焦点 + 窗口表→inputd 推送契约，ADR #61「呈现/窗口策略分离」） | 策略面随桌面形态演进（壁纸/页网格/状态栏） | hit test 真值 = wmd 记账；应用不得自绘系统 UI 区域（状态栏/Dock，应用规范 §3.5 禁令） |
| UI 框架 | berry-ui（声明式，ADR #52 自研）+ berry-gfx（pass 形态，#51）——**框架是库不是层**，随应用静态链接、semver 自演进（ADR #55） | 应用自带框架版本；**不出现第二官方框架**（应用规范 §3.5：永不引入第二 UI 体系） | 应用不得绕过 berry-ui 直写 framebuffer（surface handle 经 wm/present 域授予——能力结构性保证，非政策禁令） |
| 桌面/系统应用 | berry-launcher + 七系统应用占位（appstub 静默壳） | **同格式可换**：系统应用 = 普通 .bap，只加严不豁免（应用规范 §11.1 同格式铁律） | §11 系统应用义务清单（console=false/遥测削至近零/能力最小集/不豁免用户可见管控） |

**换肤的正门 = token/资产层，不是 fork 框架**：
- 语义色 token 闭集（背景/前景/分隔线/强调）day-one 进 berry-ui 词表闭集（u16 原子 id 表）——应用只消费语义 token，「系统切肤全树生效」；**禁硬编码两套黑白**（应用规范 §3.4，v2 预埋·词表 v1 立法）。
- 官方动效 token 闭集（时长档 + 缓动曲线进 u16 词表，mkapp/ABI 单源执法）；应用可自定义曲线但**不得冒用官方 token 名**；系统级转场（窗口/返回）token 系统独占（应用规范 §3.5）。
- 图标「应用供内容、形状归系统」（108×108 源 + 系统蒙版，§3.3）；字体 = 思源黑体 OFL 1.1 宿主构建期烘位图（ADR #63 钦定，**禁专有字体**——MiSans/HarmonyOS Sans 等「免费商用 ≠ 开源」一律禁入）。
- 视觉资产引入策略：OH 开源仓 Apache-2.0 资产优先（ADR #63），来源/SHA 记 `docs/第三方资产清单-v1.0.md`。
- **现状边界（事实）**：现役视觉常量是**编译期**烘进 launcher 的（`user/bins/berry-launcher/src/desktop.rs` 的 `OH_STATUS_H/OH_GRID_MARGIN/OH_DENSITY` 等常量族）+ 壁纸宿主预采样静态源；**运行时主题切换/资源覆盖（RRO 式）机制不存在**——推断：OEM 品牌化今天走镜像组装期换资产（可行），运行时换肤要等 v2 token 面。
- 现役 launcher 是 rootfs `bin/berry-launcher` ELF（`justfile:320`），非 `app/` 下 .bap；规范目标态为系统应用（应用规范 §11.4 提及 launcher 族豁免面）——**launcher 转正 .bap 的刀序【待核实】**（§15.3 转正刀序只列七应用）。

### 1.5 .bap + 签名链对厂商预装意味着什么

**「系统集成者」角色已被立法命名**——这就是 OEM 在应用模型中的位置：

- **系统应用定义**（应用规范 §11.1）：预装于镜像、**由系统集成者签名**、持 G2/G3 能力白名单的普通 `.bap`；同格式同门同规范，只加严不豁免（Android priv-app 双轨反面明文避开）。
- **特权白名单机制**（§11.2）：镜像内签名白名单清单（`priv-grants` 等价物，Android privapp-permissions 同构）`{app_id → [G2/G3 能力键]}`，随镜像签名分发，**构建期校验 + boot 对账**；未列而申请 = 拒授 + eco 行 loud 降级（**不采 Android「不开机」**——失败隔离原则）。特权能力永不进 G0/G1 词表，升级路径只有「系统签名 + 白名单」一条。
- **预装最小集**（§11.3）：269 号文四类映射——现役七系统应用恰好落位（SETTINGS/FILES 不可卸载可停用、CAMERA 不可卸载、PHOTOS 可卸载等仲裁规则直接采用）；预装元数据义务（407 号文第六条）= manifest 强制字段。
- **签名链**（`docs/SDK-2-分发与签名链设计-v1.1.md`，定稿未开工）：分离式 `app.bap.sig`（ed25519 + SHA-512/256，自建 berry-crypto）；**两层信任分离**——源钥=通道（镜像/仓库身份）、包钥+lineage=身份（开发者身份）；换钥全机器裁决（重叠双签窗、跨 lineage = 新身份并行安装）；信任账本；**官方源钥出厂预装随镜像分发**（首触不走网络）；sideload = 未知来源系统开关（产品镜像默认关、dev 镜像构建期预开）；全签关门触发器（外发反转或首个在野包，先到即关门，此后无签包拒载）。
- **度量链/VerifiedBoot**（`docs/安全与硬件隔离设计-v1.0.md` §4 + `docs/更新与OTA设计-v1.0.md` §2.1）：度量链 ROM→boot→kernel 每级 hash 记 secure element；双路选主（手机 = TPM 2.0 兼容面，手表 = DICE）；**信任根派生自度量**，非硬编码特权进程；`.bap` manifest 集进度量链槽 4（按 app id 升序累加）；`berry.verifiedbootstate` 四态（verified=GREEN 厂根验签 / self-signed=YELLOW / unverified=ORANGE / failed=RED）——「厂根」概念已入协议位。预装清单做成镜像内签名字段（407 号文义务，v2+）。
- **对 OEM 的含义（推断）**：①预装 = 放进镜像树 + 白名单 + 系统集成者签名，**没有第二安装通道**（一格式一扇门）；②OEM 应用商店 = 未来「源」的运营方（v1 镜像内置官方源静态实例，在线源 v2 再议；渠道 target 门槛杠杆推迟至分发面存在）；③OEM 不能给自己的三方生态开后门——G2 受限须系统签名 manifest 或商店签发授权节，「一条申请通道，ACL 双轨漂移禁入」（应用规范 §2.1）。

### 1.6 构建组装面（justfile + tools/ + bootloader/rootfs 组装）

- **产物链**：`just build-user`（cargo 多 workspace 编译 + elf2flat/musl+berry-ld 双链 + mkapp 打 .bap）→ `tools/mkcpio.py` 产 `rootfs.cpio`（`--split 32` 多卷）→ `just image`（`tools/mkimage.py`：MBR + 单 FAT16 分区整盘镜像，四件套 BOOTAA64.EFI/HELLO.TXT/KERNEL.ELF/INITRD.BIN，96MiB）→ AAVMF/UTM/真机挂盘即用（`justfile:964-989`）。
- **引导两路**：QEMU `-kernel` 直加载（开发路）与 BerryBoot EFI（产品路，十段引导 + handoff）。BerryBoot = PE32+ EFI 应用，SFS 盘读 KERNEL.ELF/INITRD.BIN，`docs/VM里程碑设计草案-v1.0.md`（M-G1/G2）。
- **更新观（v1 现状）**：fw_cfg rootfs 时代「更新 = 重新打包」；四层 OTA（L0 boot slot 三字段 + 启动计数回退 / L1 目录级 A/B + 指针原子翻 / L2 appd 换树 / L3 固件随驱动包）设计冻结、**无代码面**（`docs/更新与OTA设计-v1.0.md` §7）。
- **宿主设备壳**：`tools/berry`（host 独立 workspace，单二进制三角色 CLI/GUI/session host + control.sock + profile/runtime pack，ADR #62 追记）——**host 侧**产品形态概念（phone/desktop profile），非设备侧产品配置层；但它是「产品 profile」思维的先例。
- **执法工具族**（组装面自带的门禁）：mkapp 五道门禁 + 规范形逐字节重序列化 + 三值版本链执法（`tools/mkapp.py`）；abi-check（abi.toml 五面产物逐字节，挂 build/smoke 前置）；docs-guard/writable-globals/tlsscan/mkfat --check-fixture 等——**这些是「厂商包必须过同一道门」的现役机器执法点**（推断：CTS 的种子在这里，但它们今天只对本仓生效）。

---

## §2 冻结面（厂商动不了、也不该动的）

按 API 治理章程 L0–L4 分级（`docs/API治理章程-v1.0.md` §一）+ 架构决策冻结常量（AGENTS.md / `docs/架构决策.md`）：

| 面 | 冻结内容 | 依据（两源） | OEM 含义（推断） |
|---|---|---|---|
| **L0 syscall C-ABI** | 永久冻结、只加号不回收；abi.toml 单源五面生成（内核/berry-sys/ABI 文档/syz/wake_gen） | `tools/abi.toml` meta（nr_total=37、implemented=36、nr_ceiling=45）+ AGENTS.md「v1 37 在册」 | 内核入口面全网统一；OEM 无权增删改号。注：API治理章程 §一写「35 syscall」为其 2026-11-07 成文快照，**以 abi.toml 单源为准**（章程 §八：与现行 ADR 重叠处以原 ADR 为准）——口径漂移本身记为文档债 |
| **L1 服务 wire 协议** | add-only + per-interface version；msg_type 只加新号 | API治理章程 §一 + ADR #54/#59 | OEM 替换服务实现必须说同一 wire 协议；协议 crate 家族（fsproto/blkproto/netproto/guiproto/psproto/pwproto…）host 可测 |
| **IPC 核心** | 同步 channel + 异步通知 + 共享内存三路；热路径共享环（禁 channel 拷贝）；fastpath/slowpath 分层语义恒等；channel 深度 8/inline ≤256B/随行 handle ≤4 | ADR #5 + `docs/IPC设计-v1.0.md` §7/§2（4-4 已落地注记互证） | 通信机制不可换、不可旁路 |
| **能力模型** | 纯能力（内核态 handle）；**handle 是唯一权限机制**，永不新增第二种权限机制；一切「权限」= 何种 handle、谁签发、何时回收 | ADR #2/#3 + 应用规范 §0 总原则 2（能力模型 §8 引用） | OEM 特权只能走白名单签发，不能造「OEM 专用权限后门」 |
| **intent 词汇表** | berry_intent 六档编码冻结（0x00–0x05）；0x06..0x0F 保留拒绝；单一消费轴 | ADR #42/#58 + `user/berry-manifest/src/lib.rs:140` 词表镜像 | 调度语义全网统一；OEM 调档不造词 |
| **应用模型** | 一格式一扇门（.bap/mkapp/appd）；**永不第二容器**；manifest 未知节/键硬拒（add-only）；v1 只有一个应用模型，换代必须自带自动迁移；永不引入第二一等语言公民；POSIX 兼容层保持薄适配不得长成平行世界；**v1 无 fork、不跑 Linux 二进制** | API治理章程 §四三硬纪律 + 应用规范 §0 总原则 1；AGENTS.md + ADR #14（重议触发器表） | OEM 不能造「厂商应用格式」或私有运行时方言 |
| **调度机制/策略分界** | SchedClass trait 签名冻结；策略注入动作空间四类封顶、权重 ±64 夹紧；公平是机制默认、intent 只叠加永不替代 | `sched/src/class.rs:8` + ADR 重议触发器表；`sched/src/policy.rs` + ADR #21/#42 | OEM 策略自由度有硬包络（护栏先于策略） |
| **存储语义** | 零分区 + 四棵树（sys/apps/user/tmp）+ 树即登记簿（无包管理数据库）+ `$appdata` 是 handle 非路径；user/ 更新永不动 | ADR #45 + `docs/存储布局与沙箱设计-v1.0.md`（更新与OTA §1 互证「user/ 不动是四层共同底线」） | 分区永远是实现细节不升用户概念；OEM 盘布局自由但不改归属语义 |
| **GUI 可拔插铁律** | 清单外不依赖清单内（依赖方向铁律）；**中间不设稳定 ABI**（冻结面仅 berry_* syscall + wire）；静态链接自兜底 | ADR #43 + ADR #55 | OEM 无法通过 fork 框架层制造兼容碎片——没有可依赖的中间 ABI 面（AndroidX 版本纠缠的结构解） |
| **度量/信任结构** | 测量清单 TLV 单一事实源 + 槽位表 add-only（0–4 已分配，5–15 预留）；解封只绑 0–3 且前向密封；SUCCESSFUL 先于 rollback index 更新 | `docs/安全与硬件隔离设计-v1.0.md` §4 + `docs/更新与OTA设计-v1.0.md` §2.1 | OEM 换内核/服务集必须保度量链可复现 |

另有工程性冻结（非 ABI 但同为 OEM 禁区）：全 Rust 单语言（unsafe 收敛四域 + 外层安全 API，`tools/unsafe_audit.py` 执法）；内核 SIMD-free 三层合同（ADR #34）；细粒度锁+全可抢占路线（BKL/async 内核已否决，ADR #69/#70）；GPL/AGPL 零进入（ADR #37，Apache-2.0）。

---

## §3 缺口清单（今天厂商定制做不到什么）

> 每条注明「推断依据 = 仓库哪处现状」。以下均为**事实+依据**；「对 OEM 的后果」为推断。

| # | 缺口 | 现状事实与仓库依据 | 对 OEM 的后果（推断） |
|---|---|---|---|
| 1 | **无产品配置层 / 镜像组装 manifest** | rootfs 条目清单硬编在 `justfile:309-368`（mkcpio 显式逐文件映射，app/*.bap × 9 + bin/* × ~40）；`just image` 四件套与 96MiB 盘容硬编（`justfile:975-989`）；无 per-product 配置文件/设备profile；板级配置载体仅 DTB walker 留位（`kernel/src/dtb.rs` 现役仅 `/chosen` 解析，驱动模型 §2.1） | OEM 出一款新设备 = 改 justfile + 改 svcmgr + 重编全仓；产品线差异无处声明 |
| 2 | **服务编排硬编于 svcmgr 源码** | 预拉 arm 为手写 Rust（`user/bins/svcmgr/src/main.rs` vblk:939/guifbd:1015/gui-wmd:1109/gfxdemo:1213/fsmgr:1292…），端点配线 b-2 直递（GUI-2段③ §7-①）；数据驱动装卸（appd/devmgr）= v2（驱动模型 §5 阶段表） | 换一个 FS/加一个驱动 = 改 svcmgr 源码重编——替换「结构上可、工程上无门」 |
| 3 | **OEM 身份与签名链未开工** | SDK-2 分发签名链 §一现状表：签名=无（无签名/验签/来源验证/信任账本）；§六实施分期 P0/P1 未动工（状态行「未开工」）；key_lineage 仅预留壳（ADR #59 + berry-manifest `[security]` 节「纯占位」）；appd 验签钩留位（ADR #59）；berry-crypto 未建（P1）；BerryBoot 验签面停在 §2 设计位（更新与OTA §7.5①「BerryBoot 现为纯装载器」） | OEM 无法签自己的预装、无法运营自己的源；系统集成者签名是概念位无工具链 |
| 4 | **无兼容性测试套件（CTS 类）** | `tools/smoke.sh` 是开发冒烟（86 基础标记 = boot 32 + rootfs 54，:48/:990；标记单源 tools/markers.sh :250）；abi-check 是 ABI 单源守卫（五面逐字节，abi.toml 头注）；API治理章程 §六 CI 门禁面向本仓 PR；全仓无「厂商变体必须过 X」的认证测试面。次要事实：`docs/命令速览.md:22` 写「rootfs 相 51 标记」滞后于 smoke.sh 的 54（文档漂移样本） | 厂商改完无法自证「仍是 BerryOS」；无认证门槛 = 碎片化无执法 |
| 5 | **驱动交付格式未落地** | `type="driver"` 是 v2 词表（berry-manifest 头注「type 恒 "app"」+ 驱动模型 §1 注 + 应用规范 §1.1 表）；devmgr v2 最小面（驱动模型 §5）；IRQ→channel 转发原语未实现（v1 vblk 轮询 INT_STATUS + 1ms 睡步进——IPC设计 §3「欠的是内核实现」）；IOMMU 无实板（驱动模型 §2.4 诚实披露） | OEM 带自己的 SoC/外设来 = 无驱动装载/匹配/授权链可用，只能手铸 MMIO 窗（boot 第 4 插 = devmgr 手工前身） |
| 6 | **路径命名空间无运行面（v3）** | IPC设计 §5 + 阶段化目标（v3：namespace 服务 + mount）；ADR #61「路径式命名空间 #6 仍延后多 app 并发刀再独立 ADR」；现役发现 = svcmgr 端点直递 | OEM 重组挂载/服务树不可行；「卷 = 挂载点」停留在立法 |
| 7 | **运行时换肤/主题覆盖机制缺失** | 语义色 token v2 预埋、深色模式 v2（应用规范 §3.4/§14 行 26）；现役视觉常量编译进 launcher（`user/bins/berry-launcher/src/desktop.rs` OH_* 常量族）；壁纸宿主预采样静态源（ADR #63） | OEM 品牌化只能构建期烘资产，无 RRO 式运行时覆盖 |
| 8 | **能力词表过窄 + 装载槽静态** | 现役词表：console / fs∈{"rootfs"} / net∈{"client"}（parser 照收、appd loud 拒）——berry-manifest `FS_SCOPES/NET_SCOPES` + 应用规范 §2.5 互证；display/input/$appdata/timer/push/sensor = v2 预埋；G1/G2 弹窗与授权节 v2；APP_SLOTS=9 静态槽（berry-manifest :111 + 阶段沿革 GUI-8 注「9/9 恰满，入册必挤 SETTINGS」）；rootfs 静态 cpio 无运行期增删（应用规范 §5 现实基线 + justfile） | 厂商预装规模受 9 槽硬帽；多数手机类能力（相机/位置/通知）无授权运行面 |
| 9 | **无 OEM 交付形态与分支治理** | SDK v1 确定不外发（SDK-2 §〇 拍板 1，单向门 + 关门触发器）；无发布/分支/回流策略文档（docs/ 全表无此类档）；许可 Apache-2.0 允许 fork（ADR #37） | OEM 今天拿不到稳定 SDK/发行物；fork 后如何回流、如何跟主线无契约 |
| 10 | **真机面未开** | 路线图 P8 = 硬件落地窗口（L0 slot 随 SoC 选型挂 P8，更新与OTA §7）；PAC/BTI/MTE 推迟 v2（ADR #23）；SMP 仅 smoke-smp 线（AGENTS.md「v1 单核」+ justfile「不带 -smp 是 v1 有意定值」）；x86_64 最小集范围盒子（ADR #41） | OEM 设备（手机/穿戴真机）无处落地；QEMU virt/EFI 镜像是唯一可验形态 |
| 11 | **OTA 无代码面** | 更新与OTA §7「v1（现状）无代码面（fw_cfg rootfs = 重新打包时代的更新观）」；L1 双目录/指针/健康回退挂 v2+（§7 表） | OEM 无法交付系统升级能力；v2 前每次变更都是整机重刷 |
| 12 | **网络/生态面薄** | netd 现仅 4 op（DNS/TCP 开/往返/关）无 HTTP/TLS、MSS=128（SDK-2 §五）；berry-libc 薄 shim、v1 无 fork（AGENTS.md + ADR #14 重议触发器行）；注意 `user/berry-tls` 是**线程本地存储**（thread-local storage）非网络 TLS（crate 头注自证），勿混淆 | OEM 接自家云端/账号/推送体系缺传输层地基 |

---

## §4 结论：对「厂商定制」的友好度总评

### 4.1 天生优势（架构层已兑现或已立法）

1. **微内核用户态服务 = 替换面的结构性存在**。FS/net/驱动/显示/电源策略全是持证用户态服务，消费者只认 wire 协议；#28 代号链使服务可崩可热换；驱动模型 §0 病灶对照表直接把 Linux .ko 版本锁/Treble 十年补课列为反面——**BerryOS day-one 就不在安卓 OEM 最大的坑里**（驱动住内核 → 厂商私有内核 fork → 升级地狱的链条被切断）。
2. **机制/策略分离有真接缝 + 硬护栏**。调度四层接缝（trait/注入表/intent/票制）、策略动作空间四类封顶、权重夹紧、AI 票 kill criteria 五条（ADR #68）——OEM 调策略的自由度是**包络内的**，「护栏先于策略」的立法次序与安卓「给了再收」15 年教训正面对位（ADR #68 理由栏原文）。
3. **防碎片化是「结构解」不是「政策解」**：①无稳定中间 ABI + 应用静态链接自兜底（ADR #55——AndroidX 版本纠缠/RN 版本地狱的结构解）→ OEM 没有 fork 框架层的接口面；②三值版本域 mkapp 编译期执法 + 行为变更双轨 + abi_revision（ADR #56/SDK-2 双轨设计）→ 兼容问题永不进运行时；③L0 syscall 永冻 + wire add-only → 统一内核面/服务面；④一格式一扇门 + 未知键硬拒 → 无「厂商应用格式」；⑤能力模型唯一机制 + 特权白名单拒授 loud → 无 priv-app 式后门。
4. **升级面设计已虑 OEM**：四层 OTA + user/ 永不动 + 目录级 A/B（空间不翻倍）+ 启动计数回退「变砖机制上不可能」+ SUCCESSFUL/rollback 次序纪律——针对「整机重刷文化」病灶（更新与OTA §0）。
5. **信任链有「厂根」概念位**：度量链 + verifiedbootstate 四态 + 信任根派生自度量（非硬编码特权进程）+ 官方源钥出厂预装——OEM 信任锚的协议位已留。
6. **系统应用 = 系统集成者签名的普通 .bap**：OEM 角色已入法（应用规范 §11），同格式同门同监管（谁预置谁负责、不豁免用户可见管控）。

### 4.2 欠账（按阻挡程度排序，推断）

1. **「改配置」的通道不存在**（缺口 1/2/9）：一切定制都坍缩到「改源码重编」——这正是安卓 OEM 老路的入口（厂商 fork 内核/框架 → 面目全非）。产品配置层 + 服务编排数据驱动 + OEM 交付物，是定制化的第一前置。
2. **签名链与 OEM 身份无实现**（缺口 3）：「系统集成者签名」「厂根」是概念位，工具链（mkapp 产 .sig/berry-crypto/验签/账本）全部 P0/P1 未开工。
3. **无兼容性执法面**（缺口 4）：冻结面只防「改不动」，不防「fork 后自行漂移」；没有 CTS/认证，冻结承诺对 out-of-tree 厂商无从谈起。
4. **运行面里程碑未到**（缺口 5/6/7/8/11）：devmgr（v2）、namespace（v3）、token 换肤（v2）、动态装载/词表（v2）、OTA（v2/v2+）——OEM 感兴趣的定制面大半排在 v2/v3。
5. **真机与形态**（缺口 10/12）：QEMU virt 主验、单核、无 TLS——离「厂商设备」最近的现实是 UTM/Fusion 虚拟机里的演示系统。

### 4.3 关键判断（推断，供后续立法稿验证）

BerryOS 的定制哲学可以概括为「**冻住下三路（syscall/wire/能力），开放上三路（服务实现/策略参数/资产与系统应用）**」。这与 Treble/GKI 同向但更彻底：GKI 统一的是内核接口，BerryOS 连框架层 ABI 都不设（静态链接），碎片化的「接口面」被移除。**但开放面的合法通道（产品配置层、CTS、OEM 签名链、驱动交付）一件都还没建**——在它们落地前，任何 OEM 接入都必然以源码 fork 形态发生，架构优势无法兑现为治理优势。防安卓老路的胜负手不在内核（已防），在**产品工程三件套：配置层、签名链、CTS**。

---

## §5 开放问题（原样列出，不替拍板）

1. 「BerryOS 兼容设备」的判定权与门槛：CTS 形态（认证套件？商标许可？最低通过集？）——仓库无任何成文。
2. OEM 身份（key_lineage / 源身份）的发放机构与规则：Apple Team ID 同构，但谁分配、能否多级（OEM → ODM → 独立开发者）？
3. 产品配置层的形态：镜像组装 manifest？per-product 数据文件？cargo feature？与「启动配置参数表」（ADR #42 承诺「数字不写死」）是否同一件？
4. 调度定制的合法深度：OEM 止于参数表/策略服务，还是允许编入第二 SchedClass 实现（现被「trait 冻结 + 重编内核 + 同内核二进制」约束，重议触发器要求借用/别名无法表达才开）？
5. 厂商分支与主线回流治理：Apache-2.0 允许 fork，但无 upstream-first 契约、无变体命名/版本号规则。
6. 命名空间挂载策略（v3 后）的归属：系统策略、用户策略还是 OEM 配置？
7. 主题/品牌资产的覆盖机制：构建期烘焙（现状）够不够？运行时 token 切换的边界（系统区 OEM 可否覆写官方 token 数值）？
8. APP_SLOTS=9 静态槽与厂商预装规模的冲突解法（/apps 树动态装载是 v2 前提，应用规范 §7.1 APP_EVENT 注）。
9. berry-power/berry-thermal 服务本体完备度与真机 SoC PM 适配路径（PM-13 厂商 SoC 平台电源管理已有底稿 `research/PM-13-厂商SoC平台电源管理-v1.0.md`，落地排期待核）。
10. SDK 外发时点与关门触发器（拍板① 单向门）：OEM 先于生态进入时，单向门对 OEM 是否同样适用（OEM 算「外发」吗）？
11. 三值版本域/双轨的 **OEM 侧执法**：target 门槛杠杆（渠道面）在无商店期由谁执——镜像构建者（OEM）自己？

---

## 来源索引

**仓库内文档（相对路径，均为 2026-11-10 采集时点内容）**：

- `AGENTS.md`（入口；冻结常量/仓库布局/常用命令）
- `docs/架构决策.md`（ADR #1–#70 全表 + 重议触发器表 + 设计文档索引）
- `docs/API治理章程-v1.0.md`（L0–L4 分级、三硬纪律、废弃纪律）
- `docs/IPC设计-v1.0.md`（channel/Event/共享内存、intent 六档与传播、fastpath/slowpath、阶段化目标 v1/v2/v3）
- `docs/SDK-2-分发与签名链设计-v1.1.md`（拍板四题、信任模型、.sig 信封、sideload、实施分期）
- `docs/应用规范-v1.0.md`（§2 能力分级 G0–G3、§11 系统应用义务/特权白名单、§14 对标矩阵 52 行、§15 开放问题）
- `docs/驱动模型设计-v1.0.md`（驱动=持证应用、devmgr 五职责、boot 豁免、v1/v2 阶段表）
- `docs/更新与OTA设计-v1.0.md`（四层刀法、L0 slot 三字段、§7.5 签名槽生命周期）
- `docs/安全与硬件隔离设计-v1.0.md`（§4 度量启动、槽位表、双路选主）
- `docs/SDK-2-双轨与演化纪律设计-v1.0.md`（行为变更登记表、abi_revision、能力集标签）
- `docs/GUI-2段③消费侧首刀实施设计草案-v1.0.md`（b-2 端点直递、svcmgr 预拉链）
- `docs/命令速览.md`（just 配方全表）
- `docs/路线图-v1.0.md`（P0–P8 依赖图、x86 盒子）
- `docs/阶段沿革.md`（最新里程碑：rootfs 相 54/54、W2 文档治理线）

**仓库内代码/工具（关键取证点）**：

- `justfile`（build-user :44-380、rootfs 装配 :309-368、image :975-989、单核定值注释 :21-22）
- `tools/abi.toml`（nr_total=37/implemented=36/nr_ceiling=45；五面生成）
- `tools/mkapp.py`（.bap 门禁五道 + 规范形 + 三值版本链执法）
- `tools/mkcpio.py` / `tools/mkimage.py` / `tools/mkfat.py`（组装/造盘单编码器）
- `tools/smoke.sh`（86 基础标记 = boot 32 + rootfs 54；:48/:990；标记单源 :250）
- `sched/src/class.rs`（SchedClass trait 冻结 + Fair/EEVDF/deadline 带）
- `sched/src/policy.rs`（三槽 RCU 换表、动作空间四类、WEIGHT_MAX=±64）
- `user/berry-manifest/src/lib.rs`（manifest schema v0/v1、FS_SCOPES/NET_SCOPES、APP_SLOTS=9、Intent 六档镜像 :140）
- `user/bins/svcmgr/src/main.rs`（预拉 arm 硬编取证：vblk:939/guifbd:1015/gui-wmd:1109/gfxdemo:1213/fsmgr:1292）
- `user/bins/berry-launcher/src/desktop.rs`（OH_* 视觉常量编译期烘焙）
- `user/berry-tls/src/lib.rs`（澄清：线程本地存储，非网络 TLS）
- `kernel/src/dtb.rs`（DTB walker 现役仅 /chosen——经 `docs/驱动模型设计-v1.0.md` §2.1 引证）

**外部资料**：本文为仓库现状盘点，未引用外部资料；对标判断（Android priv-app/Treble/GKI、Apple Team ID、AOSP A/B 等）均转引自仓库内文档的既有对标底稿（`docs/应用规范-v1.0.md` 溯源五底稿、`research/` 目录），原始外部出处以各底稿来源索引为准。

**标注待核实项汇总**：§1.1 启动配置参数表机制面；§1.1 berry-power/thermal 服务本体完备度；§1.4 launcher 转正 .bap 刀序；§2 L0 行「35 vs 37」口径漂移以 abi.toml 为准；§5 各开放问题。
