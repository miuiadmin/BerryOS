# SEC-8 调研笔记：Rust 与内存安全现状（业界侧）

- 日期：2026-10-29（信息截至本日）
- 调研人：workflow 安全研究 agent（SEC-8）
- 研究问题：业界当前怎么做「Rust 与内存安全」——机制、演进路径（day-one vs 后补）、跨厂商对照、坑与教训
- 方法：WebSearch / WebFetch / web_reader，优先一手来源。证据分级：**【证】** = 一手来源可查（少数为多方一致的权威二手转述，注明）；**【推】** = 本 agent 推断，不可当事实引用
- 纪律：第 1–4 节只写业界现状；第 5 节才写对 BerryOS 的对照与建议

---

## 1. 概述：解决什么问题，信任边界在哪

**问题**：C/C++ 存量代码里，内存安全类漏洞（越界、UAF、双重释放等）长期占高危漏洞的 60–76%。业界共识的动作不是重写存量，而是**新代码切换到内存安全语言（MSL）**，让存量随「老化效应」逐步摊薄。Rust 是其中唯一同时满足「无 GC + 系统级性能」的 MSL，因此成为 OS/驱动/固件领域的主选。【证】[1][2][21]

**信任边界有三层，语言只管第一层**：

1. **语言边界**：Rust 的安全承诺只在 safe 代码 + sound 的 unsafe 块内成立。unsafe 边界、`#[repr(C)]` 桩、与 C 的互操作面是新的攻击面——Android 全仓 Rust 中约 4% 是 unsafe[3]，Android 13 的 UWB 栈新代码里 unsafe 只有 2 处[1]，说明边界可以收敛得很小，但永不归零。
2. **供应链边界**：语言安全不管 crat​es.io 投毒。2025–2026 年 Rust 供应链攻击连发（§3.4），防线是发布通道（Trusted Publishing）与审计工具链（cargo-vet/audit/deny），不是语言本身。
3. **验证边界**：seL4 证明了另一条正交路线——形式化验证核心 + Rust 用户态。语言安全替代不了验证，验证也覆盖不了用户态逻辑；两者是互补层（§3.5）。

**本篇的用处**：给「全 Rust 微内核」这条路线找最硬的外部证据（Android 数据）、找业界已踩过的坑（Linux 主线摩擦、供应链事件）、找 BerryOS 现在就该留的缝（§5）。

---

## 2. 各家现状对照表

| 阵营 | 语言策略（2026-10 现状） | 内存安全关键动作 | 可测效果 | 证据 |
|---|---|---|---|---|
| **Google/Android** | 新原生代码默认 Rust（Android 12 起为平台语言）；存量 C/C++ **明确不重写**[2] | Keystore2、UWB、蓝牙 Gabeldorsche、Binder（AOSP Rust 库 + 内核 Rust binder 驱动）、Android Virtualization Framework、Chromium PNG/JSON/字体解析器等[1][2][3][5][6][7] | 内存安全漏洞占比 2019 年 76% → 2022 年 35% → 2024 年 24% → 2025 年预估 <20%；Rust 代码 ~500 万行、零内存安全 CVE（Android 13 时代 150 万行即零）；rollback 率较 C++ 低约 4x | 【证】[1][2][3][8] |
| **Linux 主线** | C 为主体，Rust 作为「第二语言」进主线自 6.1（2022-12）[9]；6.14 驱动基建[10]、6.15 首个真驱动（Android Rust binder）[11][12]、6.16 nova-core/网络 PHY、6.18 USB 驱动面齐备[13][14] | 哲学 = safe Rust 写叶子驱动，unsafe 收敛到抽象层；2025-12 维护者峰会定性「实验成功收官、Rust 留下」[17][18] | 主线已有生产 Rust 驱动；但社区摩擦实际造成维护者流失（Wedson 2024-09 退出[15]、Alex Gaynor 2025-11-24 卸任[16]） | 【证】[9]–[20] |
| **Microsoft/Windows** | 内核/组件局部 Rust 化：2023 年 win32k、DWriteCore 约 3.6 万行改写[43]；2025-12 传「2030 淘汰 C/C++ + Win11 全面重写」被官方否认[44] | 局部替换 + 沙箱内先用（Chromium 系解析器先例同源） | 无系统性公开占比曲线；否认全面重写 | 【证】[43][44] |
| **seL4** | 验证核心保持 C（证明对象），**用户态一等支持 Rust**（13.0 起）[39]；rust-microkit-demo 官方样例[40]；14.0.0（2025-11-25）全 Arm 平台验证配置；MCS RISC-V 功能正确性证明完成[41]；2026-03/07 协同发版含 Rust 支持[42] | 「验证核心 + 语言安全外围」分层；HAMR 代码生成支持 Rust 组件 | 验证面扩大到全部 Arm 配置；Rust 用户态生态制度化（发版节奏内） | 【证】[38]–[42] |
| **Fuchsia** | 官方开发文档长期把 Rust 列为平台主语言之一，内核 Zircon 为 C++ —— 本会话未逐页核对一手文档页，此分法标【推】；fuchsia.dev 为入口[49] | 语言层面的安全叙事 + Zircon 能力模型 | 未取得可引用的漏洞占比数据 | 【推】[49] |
| **Apple/iOS** | 未找到 Apple 官方「内存安全路线图」一手来源；Swift 在 NSA MSL 推荐名单内[21]；Apple Silicon 强制 PAC/BTI 硬件面（本次未取一手，【推】） | 硬件缓解路线为主 | 无公开占比曲线可比 | 【推】[21] |
| **QNX** | 商业微内核，C 实现 + 安全认证（ISO 26262 等）叙事；本次未深研，未取得一手来源 | 认证驱动而非语言驱动 | — | 【推】 |
| **Redox OS** | 全 Rust 微内核 + `url:` scheme 能力模型（`pipe:`、`initfs:` 等），沙箱 = 限制 scheme 访问[45]；TU Ilmenau 2024 论文做了引用监视器形式化分析[46] | 语言 + 能力双重叙事 | 学术级验证、无生产部署数据 | 【证】[45][46] |
| **Theseus** | 全 Rust 研究 OS：intralingual 设计（用语言类型系统表达 OS 不变量）、state spill-free、运行时重构（SOSP 2019）[47][48] | 「让非法状态不可表示」的研究路线 | 仅研究原型 | 【证】[47][48] |
| **IoT/OSS 适用者** | CISA 2024-07 报告：广泛使用的 172 个关键开源项目中 52% 含内存不安全语言代码[25] | 监管侧推动（§3.4/§4） | 行业基线数据 | 【证】[25] |

---

## 3. 机制细节

### 3.1 Android：度量先行的治理闭环

Google 的方法论核心是「先有基线曲线，再切语言」：

- **占比曲线**：2019 年内存安全漏洞占 Android 高危的 76%（其中 critical 级占 86%）→ 逐年下降至 2024 年 24%（9 月公告后口径 27→外推口径有差，Google 原文给 24% 为 2024 外推值）→ 2025 年预估 <20%。绝对数：2019 年 223 个 → 2022 年 85 个[1][2][8]。【证】
- **密度对拍**：Android 以 CVE/MLOC 归一化。2025-11 数据：Rust 侧内存安全漏洞密度约 0.2 个/MLOC，历史 C/C++ 密度高出三个数量级以上（Google 原文口径「>1000x」）[3]。【证】
- **工程效率副证**（DORA 数据）：Rust 变更的代码评审时间少 ~25%、评审轮次少 ~20%；rollback 率约为 C++ 的 1/4（2024 年口径「不到一半」，2025 年口径收敛到 ~4x）[2][3]。——反驳「Rust 拖慢交付」的常见质疑。【证】
- **老化效应（aging effect）**：5 年以上老代码的漏洞密度显著低于新代码（Google 给出 3.4x–7.4x 量级的差异）。推论 = 不必重写存量，控制新增即可让占比自然下行[2]。【证】
- **安全遥测**：GWP-ASan 抽样采样生产崩溃归因内存安全；Android 官方文档引用 Project Zero 结论——被在野利用的 0-day 中内存破坏/UAF 占绝对多数[4]。【证】

**接口位启示（机制可移植件）**：归一化密度指标（CVE/MLOC）、rollback 率、评审时长三项，是「语言红利」从口号变成资产的度量面。

### 3.2 unsafe 面的治理形态

- **Android**：全仓 Rust ~4% unsafe[3]；UWB 栈（Android 13 新写）unsafe 仅 2 处，且 Google 以此作为「unsafe 可以管得住」的展品[1]。治理形态 = unsafe 集中审批 + 新组件 unsafe 配额预期。【证】
- **Rust for Linux**：哲学是 safe Rust 写叶子驱动，unsafe 全部收敛到「安全抽象层」（safe wrapper 包裹 raw API），叶子驱动零 unsafe。争议史：GPL/unsafe 边界、抽象层维护责任归属曾长期是社区摩擦点（Miguel Ojeda/Wedson 多次公开阐述该分层）[9][15]。【证】
- **soundness 责任**：unsafe 块的正确性责任在人——RustSec 里「unsound safe API」类公告持续存在，即 safe 外壳包错 unsafe 内核照样炸（§3.4）。【证】[35][37]

### 3.3 互操作工具链（后补路线的核心资产）

Google 明确立场：**"interoperability is the new rewrite"**——不重写存量，投资 C/C++↔Rust 互操作[2]：

- **Crubit**（C++↔Rust 自动绑定）、**autocxx**；2025 年向 Rust Foundation 捐 100 万美元专项做互操作[2]。【证】
- **Binder 双栈过渡**：AOSP `libs/binder/rust` 用户态 Rust 实现在先[7]；内核侧 Rust binder 驱动 6.15 进主线成为首个「真驱动」[11][12]；Google 表态 Rust binder 就绪后 C 版 binder 驱动将移除（覆盖「数十亿设备」的切换）[3]。Android 6.12 内核是首个启用 Rust 的 Android 内核 + 首个生产 Rust 驱动[3]。【证】
- 教训面：互操作层是后补路线最大的长期税——抽象层要跟随两侧 API 演进，维护者缺口直接造成摩擦（§4）。

### 3.4 Rust 供应链防线与事件时间线

**事件侧（攻击面实测）**：

| 时间 | 事件 | 证据 |
|---|---|---|
| 2025-09 | crates.io 官方披露基础设施安全事件 + 钓鱼域 crates-io[.]net 投毒（discussion #11889） | 【证】[31] |
| 2026-05 | TrapDoor 运动，34 个包被植入 | 【证】[32]（多方报道） |
| 2026-08 | arrayref / internment / append-only-vec 等知名 crate 被投毒（版本劫持） | 【证】[33] |

**防线侧（机制）**：

- **Trusted Publishing**：crate 发布改用 OIDC 短时凭据绑定 CI，废除长寿命 token——2025 年起 crates.io 逐步铺开，是针对「token 泄露 → 版本劫持」的直接对策[34]。【证】
- **cargo-vet**（Mozilla）：策略分 `safe-to-run` / `safe-to-deploy`，审计结论可在组织间共享（Firefox/Chrome 互相进口审计）。【证】[32 相关生态描述]
- **cargo-audit / cargo-deny + RustSec 数据库**：RustSec 公告库（OSV/GHSA 分发，crates.io 已上 Security tab）[35][36]；第三方趋势报告显示公告数持续增长、且构成从内存安全 bug 向 typosquatting/unsound API/弃维护 crate 迁移[37]。【证】
- RustSec 关键词页可见 memory-safety 类公告延续到 2025–2026——**全 Rust ≠ 零漏洞，只是把漏洞类别大幅搬走**[36]。【证】

### 3.5 验证内核 + Rust 用户态（seL4 路线）

- seL4 13.0 起把 **Rust 用户态支持列为发版内容**（不再是社区附庸）[39]；14.0.0（2025-11-25）首次覆盖全部 Arm 平台的验证配置[38]；MCS（mixed-criticality）RISC-V 功能正确性证明完成[41]；2026-03 与 2026-07 两轮协同发版（seL4/Microkit/CAmkES/capDL/Rust 支持）[42]；官方 rust-microkit-demo 样例[40]。【证】
- 结构含义：**验证核心（C，证明对象冻结）+ 语言安全外围（Rust 用户态）**是可组合的分层——形式化管核心不变量，Rust 管外围的内存安全基线。HAMR 工具链可生成含 Rust 组件的系统。【证】

### 3.6 硬件兜底（与语言安全互补的运行时网）

- Android 官方要求：Arm v9 平台 CTS 必须带 memory tagging（MTE）跑一轮；v8 平台被要求提供 HWASan/KASan 轮[4]。——Google 把硬件缓解当**语言安全的运行时补充**而非替代。【证】
- CrabbyAVIF 案例（Google 自述）：C 库漏洞 CVE-2025-48530 移植进 Rust 时在发布前被工具抓到，且 Scudo 分配器 guard page 使其实际不可利用[3]——「Rust 迁移 + 工具 + 分配器护栏」三层叠加的实证。【证】

---

## 4. 演进史与教训

### 4.1 时间线（后补路线的完整弧光）

| 时间 | 事件 | 证据 |
|---|---|---|
| 2019 | Android 开始按漏洞类别排优先级（度量基线元年） | [2] |
| 2022-11 | NSA CSI 发布 MSL 指南（推荐 C#/Go/Java/Python/Rust/Swift） | [21] |
| 2022-12 | Android 13：150 万行 Rust、零内存安全 CVE；占比 76%→35% | [1] |
| 2022-12 | Linux 6.1 合入 Rust 基础设施（首个第二语言） | [9] |
| 2023 | CISA+NSA+FBI+五眼《The Case for Memory Safe Roadmaps》；MS 2023 内核组件 Rust 改写 | [22][23][43] |
| 2024-02 | 白宫 ONCD《Back to the Building Blocks》点名 MSL | [24] |
| 2024-07 | CISA：52% 关键开源项目含内存不安全代码 | [25] |
| 2024-09 | Wedson Almeida Filho（R4L 主力）退出：「nontechnical nonsense」 | [15] |
| 2024-12 | EU CRA 生效（2024-12-10）；报告义务 2026-09-11、全面适用 2027-12-11 | [26][27][28] |
| 2025-03/05 | Linux 6.14 驱动基建 / 6.15 Rust binder 进主线 | [10][11][12] |
| 2025-11-13 | Google 年度盘点：~500 万行 Rust、密度 0.2/MLOC、4% unsafe、新代码 Rust 行数超 C++ | [3] |
| 2025-11-24 | Alex Gaynor（R4L 共维护者）正式卸任 | [16] |
| 2025-12-10/12 | LWN《The (successful) end of the kernel Rust experiment》/ Phoronix《Rust To Stay》：维护者峰会定性实验成功、Rust 常驻 | [17][18] |
| 2025-12-01 | Linux 6.18：典型 USB 驱动所需 Rust 基建齐备、kernel::usb 文档上线 | [13][14] |
| 2025→2026 | CISA/FBI Product Security Bad Practices（草 2024-10 → 定稿 2025-01）；2025-06 CISA/NSA 再发 MSL 联合报告 | [29][30] |
| 2026-05 | Greg KH 公开招募更多 Rust 内核开发者（以「80% 内核 CVE 是内存安全」为叙事） | [19] |
| 2025-09→2026-08 | Rust 供应链三连击（§3.4） | [31][32][33] |

### 4.2 教训（跨案例沉淀）

1. **不重写存量，管住增量**：Google 原文明确「不需要丢弃或重写现有内存不安全代码」；老化效应替你摊薄存量[2]。重写派（传闻中的 MS 全面重写）被官方否认[44]。【证】
2. **interop 是后补路线的长期税**：Crubit/autocxx/抽象层维护是持续成本，且社区摩擦点恰恰长在抽象层的归属上（Wedson/Gaynor 两度退出都是「技术之外的」组织问题）[15][16]。day-one 全 Rust 的路线天然不付这笔税——但代价是生态断代（无 Linux 二进制兼容），各自取舍。【证+推】
3. **度量先行，否则红利无法证明**：Android 2019 年先建类别化基线，2022 年才能画出 76%→35% 曲线。没有基线的语言切换无法向任何利益相关方证明价值[1][2]。【证】
4. **「实验」需要终局管理**：R4L 用了三年（6.1→6.15）才到首个真驱动；期间靠维护者峰会一锤定音「成功收官」才止住退场潮[17][18]。技术成功 ≠ 组织成功。【证】
5. **语言安全搬家了攻击面，不是消灭攻击**：RustSec 公告构成从内存 bug 转向 typosquatting/unsound/弃维护[37]；crates.io 三连击证明供应链成为主战场[31][32][33]。防线 = 发布通道 + 审计工具链，且这些和语言一样需要 day-one 建。【证】
6. **监管是硬时限**：CRA 全面适用 2027-12-11；虽无「必须用 MSL」的明文（该推断标【推】），但 secure-by-default 义务 + 报告义务让「内存安全占比」成为合规叙事的一部分[26][27][28]。【证+推】
7. **后补 ≠ 自动生效**：Linux 证明了基建先行的必要（6.1/6.14 两轮纯基建才有 6.15 的驱动）；MS 的局部改写证明没有度量和路线图的 Rust 化只是点缀。任何「以后再补」的安全属性都要先有接口位和数据位。

---

## 5. 对 BerryOS 的启示（仅本节写对照）

> BerryOS = 全 Rust（内核 no_std + 用户态）、Zircon 务实派微内核、纯能力 handle、用户态驱动（.bap 持证应用、无 .ko）、无 Linux 二进制兼容。以下把业界现状映射到我们的 v2/v3/v4 与 day-one 缝。

### 5.1 day-one 留缝项（现在就埋，防「后补 ≠ 自动生效」）

1. **unsafe 台账 + CI 密度线**：业界基线 = Android ~4% unsafe[3]、UWB 2 处 unsafe[1]。我们已有「unsafe 收敛小块」纪律，建议再加机器可读清单（`kernel` unsafe 块逐条登记 + 用途分类：页表/切换/MMIO/分配器/原子），CI 输出 unsafe 行数与占比趋势。这是「全 Rust 安全叙事」的第一张可出示数据。
2. **漏洞分类账 day-one 立维度**：Android 2019 年先建基线才有曲线[2]。建议 issue/回归记录里从现在起就带类别标签（memory-safety / logic / 协议 / 供应链 / 工具链），哪怕 memory-safety 记 0——没有维度就没有未来的 76%→x% 图。
3. **供应链工具链挂 CI**：对齐业界防线（Trusted Publishing 时代的三连击教训[31][32][33]）——`cargo-audit` / `cargo-deny` 进 `just check` 面、Cargo.lock 全量入库（已有）、锁步版本策略（uu 家族 =0.12.0）成文、镜像产物（out/berryos.img）哈希清单随构建产出。可选进阶：cargo-vet 思路的「新依赖准入单」。
4. **C-ABI 桩面纳入 unsafe 同级审计**：我们的隐性 C 边界在 berry-libc 桩与 `#[repr(C)]` 结构（errno 档位、出参宽度、Stat 布局三案都是这块的实抓）。业界对应物 = interop 抽象层的 soundness 责任（§3.2）。建议：桩函数签名对照表 + 布局钉板（已有 offset_of! 钉板先例）列入 unsafe 台账同级review 面。
5. **硬件缓解的探针缝**：Android 把 MTE/PAC/BTI 当语言安全的运行时补网[4]。v1 cortex-a72 无这些扩展，但留缝 = 启动期能力探测位（已有 SMMU probe 先例）+ slab/堆布局对齐到 tag 粒度，使 v2 `-cpu max` 切换是配置而非重构。

### 5.2 v2/v3/v4 落点

- **v2（SMMU + PAC/BTI/MTE）**：与 docs 安全设计 staging 一致；业界佐证 = Android v9 CTS 强制 MTE 轮[4] + CrabbyAVIF「分配器护栏让漏洞不可利用」案例[3]。MTE slab tagging（区分 UAF 来源域）是库里就有的形态。
- **v3（度量启动链 + 远程证明）**：CRA 时点对齐——报告义务 2026-09-11 已生效、全面适用 2027-12-11[26][28]；v3 的 hash 链 + vTPM + 证明面即是合规叙事的承接位。seL4 的「验证核心 + 语言外围」分层提示：我们的证明对象应该是启动链与服务清单，而不是全内核。【推】（落点判断）
- **v4+（签名 .bap + anti-rollback）**：驱动 = 持证应用的模型天然优于 .ko——签名面落在应用包而非内核模块加载器；业界对照 = Android 的 .bap 类似物（APEX/模块化系统，UWB 即模块化交付[6]）。ADR #47 A/B 槽与 anti-rollback 版本号建议在 .bap manifest v1 就留版本域（已留则核对）。

### 5.3 反面清单（不要学的东西）

- 不学「局部改写当成绩」而无度量（MS 2023 路线的教训[43][44]）——我们的对拍物是曲线不是口号。
- 不学 R4L 的抽象层税[15][16]——我们无 Linux ABI 兼容，永不需要 safe wrapper 包裹巨型 C API 面；坚持桩面最小化（排除 onig 类硬依赖的既有裁决与此同向）。
- 不把「全 Rust」当安全终点——RustSec 构成迁移[37]与三连击[31][32][33]说明终点是「类别清零 + 供应链有防线 + 硬件兜底」三层。

---

## 6. 来源清单

1. https://security.googleblog.com/2022/12/memory-safe-languages-in-android-13.html
2. https://security.googleblog.com/2024/09/eliminating-memory-safety-vulnerabilities-Android.html
3. https://blog.google/security/rust-in-android-move-fast-fix-things/
4. https://source.android.com/docs/security/test/memory-safety
5. https://source.android.com/docs/security/features/trusty
6. https://source.android.com/docs/core/ota/modular-system/uwb
7. https://android.googlesource.com/platform/frameworks/native/+/refs/heads/master/libs/binder/rust
8. https://www.bleepingcomputer.com/news/security/google-sees-68-percent-drop-in-android-memory-safety-flaws-over-5-years/
9. https://www.phoronix.com/news/Rust-Is-Merged-Linux-6.1
10. https://www.techzine.eu/news/devops/129925/linux-kernel-6-14-offers-extra-support-for-future-rust-drivers
11. https://lkml.org/lkml/2025/5/25/345
12. https://forum.devtalk.com/t/the-linux-6-15-kernel-arrives-and-its-big-a-victory-for-rust-fans/201366
13. https://www.cnx-software.com/2025/12/01/linux-6.18-release-main-changes-arm-risc-v-and-mips-architectures
14. https://rust.docs.kernel.org/kernel/usb/index.html
15. https://arstechnica.com/civis/threads/rust-in-linux-lead-retires-rather-than-deal-with-more-%E2%80%9Cnontechnical-nonsense%E2%80%9D.1502715/page-3
16. https://www.phoronix.com/news/Alex-Gaynor-Rust-Maintainer
17. https://lwn.net/Articles/1049831
18. https://www.phoronix.com/news/Rust-To-Stay-Linux-Kernel
19. https://www.phoronix.com/news/Greg-KH-More-Rust-Linux
20. https://www.phoronix.com/news/Linux-6.19-Driver-Core
21. https://soc.cyber.wa.gov.au/advisories/20221124001-NSA-Software-Memory-Safety
22. https://media.defense.gov/2023/Dec/06/2003352724/-1/-1/0/THE-CASE-FOR-MEMORY-SAFE-ROADMAPS-TLP-CLEAR.PDF
23. https://www.cisa.gov/news-events/news/cisa-nsa-fbi-and-international-cybersecurity-authorities-publish-guide-case-memory-safe-roadmaps
24. https://www.helpnetsecurity.com/2024/02/27/memory-safe-programming-languages
25. https://www.meritalk.com/articles/cisa-half-of-open-source-projects-have-memory-unsafe-code
26. https://www.cyberresilienceact.eu/regulation.html
27. https://en.wikipedia.org/wiki/Cyber_Resilience_Act
28. https://www.freshfields.com/en/our-thinking/blogs/technology-quotient/cyber-resilience-act-reporting-obligations-take-effect-on-11-september-2026-102nzmk
29. https://www.cpomagazine.com/cyber-security/new-cisa-nsa-joint-report-reiterates-call-for-memory-safe-languages
30. https://www.linkedin.com/posts/jackcable_product-security-bad-practices-cisa-activity-7286104227834118144-EJFg
31. https://github.com/rust-lang/crates.io/discussions/11889
32. https://cyberpress.org/supply-chain-attack-compromises-34-packages
33. https://blog.codercops.com/blog/rust-arrayref-crates-io-supply-chain-attack-2026
34. https://zenn.dev/kesin11/articles/20250802_crates_trusted_publish?locale=en
35. https://github.com/rustsec/advisory-db
36. https://rustsec.org
37. https://safeguard.sh/resources/blog/rustsec-advisory-database-trend-report
38. https://docs.sel4.systems/releases/sel4/14.0.0.html
39. https://www.reddit.com/r/rust/comments/1dypu8y/the_sel4_formally_verified_microkernel_releases
40. https://github.com/seL4/rust-microkit-demo
41. https://www.linkedin.com/posts/sel4_sel4-news-activity-7477272371901460480-EbM4
42. https://sel4.discourse.group/t/july-2026-releases-of-sel4-microkit-camkes-capdl-and-rust-support/1069
43. https://forums.theregister.com/forum/all/2023/04/27/microsoft_windows_rust
44. https://securityonline.info/microsoft-denies-plans-to-rewrite-windows-11-in-rust-despite-ai-ambitions
45. https://www.skierpage.com/blog/2022/08/software-making-it-safer-to-run-random-programs
46. https://www.tu-ilmenau.de/fileadmin/Bereiche/IA/vsbs/Theses_completed/2024_Wintzler_MSc.pdf
47. https://github.com/theseus-os/Theseus
48. https://www.theseus-os.com/Theseus/book/index.html
49. https://fuchsia.dev/fuchsia-src

（二手来源说明：8/12/24/25/29/32/33/39/43/44 为权威媒体/社区对一手事件的转述，事件本身多方一致；一手可直接核验的以 1/2/3/4/11/17/22/26/31/35/38 官方源为准。）
