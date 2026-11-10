# 07 号底稿：兼容性机制工程细节——ABI/版本/测试/认证

> **采集日期**:2026-11-10
> **性质**:研究底稿(证据供给,非立法)。为「厂商定制化」深研系列第 07 篇;第一轮 04 号只到「要有 BCTS」的框架层(04 号 §4),本文把业界兼容性机制解剖到工程可抄的字段/流程/条款级,供 BCTS 立法稿(ADR/设计文档)直接取用。
> **消费面**:「BCTS 立项与设计」「API 治理章程修订(执法位)」「SDK-2 双轨与演化纪律修订(abi_revision 执法位)」;与 04 号 §4(认证门槛草案)、`docs/API治理章程-v1.0.md`、`docs/SDK-2-双轨与演化纪律设计-v1.0.md` 直接对接。
> **方法**:外部一手优先(Fuchsia RFC/gitiles 源仓 markdown、Microsoft Learn 官方页、glibc 源码与 NEWS、cargo 官方文档、Canonical 文档站),全部 URL 可复核;数值/枚举类事实至少两源交叉(源文档+实际数据文件或第二文档);仓库内事实给相对路径+行号;事实与推断分开,推断处标「推断」;单源或存疑标「待核实」。
> **版本**:v1.0(文件名按任务指定不带版本号,版本记于本行)。
> **来源索引**:见文末「来源索引」;仓库内引用一律给相对路径+行号。

---

## §0 摘要(一屏版)

- **Fuchsia 的兼容性工程是「四个独立机制咬合」**:①版本双轨(API level 人类单调 × ABI revision 机器随机,RFC-0002)+ ②值编码进包(`meta/fuchsia.abi/abi-revision` u64 LE,RFC-0135)+ ③四道拒载门(打包期 SDK 校验/装载期 component_manager 拒启动/组装期拒入镜像/解析期拒下载,RFC-0135+0239)+ ④契约测试(CTF freeze/thaw,RFC-0015/0141)。四件缺一即退化成纸面承诺——BerryOS 已有①③的立法位(三值域+abi_revision 字段设计)但执法位只有 mkapp 编译期一道门。
- **测试怎么版本绑定的答案是「freeze/thaw」**:release 分支冻结测试工件→上传 CIPD→main 分支解冻回放到新平台→挡 CL。OEM/上游都无法「针对测试特调」,因为测的是旧冻结件对新平台组装的行为——这是结构性防特调,不是流程性。
- **Windows 给的是「分型执法」样本**:HLK(测行为)+ EV/attestation 签名(给身份)+ DCH 分层(IHV 基础件一次认证全网复用,OEM 只测自己的扩展件)。费率:现行文档不再列提交费(标待核实的历史费率),但 EV 证书与 Partner 注册是硬门槛;Ubuntu 认证则干脆商业绑定(OEM Partner Programme 成员才可认证)——认证的「钱」从按次收费转向了身份/商业前置。
- **Rust 无稳定 ABI 对 BerryOS 是利好不是债**:C 生态最有纪律的 glibc(symbol versioning + add-only + soname 27 年不破)依然栽在 memcpy 事件上(Hyrum 定律实证),证明「有稳定 ABI 也要防实现细节泄漏」;BerryOS「静态链接自兜底 + 兼容面收敛到 wire+syscall 两处」(01 号 §1.2 判断)经反例排查后成立,但守住它需五个边界条件(§4.5,其中预编译三方案是唯一真破口)。
- **BCTS 工程形态清单(§6,12 条)**:每条注明抄自哪家哪机制——核心结论(推断):BCTS ≠ 大而全测试集,而是「冻结工件回放 + 兼容面清单(PlaSA 式) + 分类执法(SDK categories 式) + 版本绑定包名(_api$N 式)」四件的组装。

---

## §1 Fuchsia:微内核系最完整的兼容性工程(逐 RFC 解剖)

### 1.1 版本双轨:API level × ABI revision(RFC-0002)

RFC-0002(platform versioning)定义两个正交版本标识,这是整个体系的地基:

| 维度 | API level | ABI revision |
|---|---|---|
| 语义 | 构建时可见的 API 集合(「API 界面的第 N 版」) | 应用期望平台提供的系统接口**语义** |
| 谁读 | 人(端开发者选 target) | 机器(平台装载期核对) |
| 值域与生成 | **单调递增整数**(RFC-0246 起为 32 位);与里程碑绑定,F`N` 分支切出前发布 | **64 位不透明整数,随机选取**(「从未用过的值中随机选」) |
| 不可变性 | 已发布即不可变(api_levels.md:「API levels are immutable……一旦发布,该 API level 的 API 集合及语义永不改变」) | 每个 API level 发布时分配唯一 ABI revision(RFC-0239 :402-404) |
| 兼容关系 | 多个 API level 可指向同一 ABI revision(RFC-0002:「一个 API level 对应一个特定 ABI revision,但多个 API level 可指向同一 ABI revision」) | 每个 ABI revision 之间互不兼容(单调不兼容,§5.4) |

**值怎么算(工程细节)**:ABI revision 的随机性是**治理设计**而非技术必需——RFC-0002 原文:「标识符随机选取是为了防止端开发者猜测未来的 ABI revision 标识符、并对未来版本的语义形成预期」。这直接对应 BerryOS 的一个差异:BerryOS 现行设计是 `abi_revision`(u16,v1 起始 = 1)单调编号(`docs/SDK-2-双轨与演化纪律设计-v1.0.md` §二)——单调编号允许外部「预测下一个号」,风险=Fuchsia 明说的那类(推断:对 BerryOS 危害较小,因为 BerryOS 的 abi_revision 只做遥测统计不做兼容承诺,见 §6-6)。

**实际数据文件(一手核验)**:`sdk/version_history.json` 结构为 `{"data": {"api_levels": {"32": {"abi_revision": "0x30F4F8C6AB3600DE", "phase": "supported"}, "33": {"abi_revision": "0x1D71229A01E513EF", ...}}}, "schema_id": ...}`——每条 = API level 号 → {abi_revision(64 位随机 hex), phase}。这个 JSON 就是「SDK 元数据里的映射表」(RFC-0002 §SDK:「SDK 应以机器可读格式说明其支持的 API levels 及其到 ABI revision 的映射」)。BerryOS 对应物 = abi.toml + 未来 version_history 等价件(开放问题)。

**元素生命周期五阶段(RFC-0002 §Lifecycle)**:introduced → extended(加子元素,如 FIDL table 加字段)→ deprecated(旧 ABI revision 仍可用,新 API level 不可见)→ legacy(平台只通过 quirk 表为特定遗留组件维持)→ removed。其中 **quirk 机制**是精妙件:平台不再支持某 ABI revision 后,维护一张 `{遗留组件 → quirk(兼容垫片)}` 表,垫片只需让**特定组件**跑起来、无需实现完整旧语义(RFC-0002:「垫片不需要支持旧 ABI revision 隐含的全部语义,只需好到能让 baz.cm 继续工作」)。这为「平台想删旧面但有个别重要组件没迁走」提供了非全量的退路。

### 1.2 ABI revision 怎么编进包(RFC-0135 字段级)

RFC-0135 只管「编码」这一件事,字段级细节:

- **文件**:`meta/fuchsia.abi/abi-revision`,位于包元数据归档 meta.far 内。
- **格式**:无符号 64 位**小端**整数。选裸整数而非 JSON/持久化 FIDL 的理由(RFC-0135 §Alternatives):「不期望人类消费这个值(应引用人类可读的 API level)」「以小端整数传递避免额外转换」「易解析」。
- **目录防撞**:放 `meta/fuchsia.abi/` 命名空间目录(与平台 FIDL 命名空间同惯例),打包工具**禁止用户在 `meta/fuchsia.abi` 下定义自定义文件**——把「平台元数据」与「用户文件」结构性隔离。且该目录是平台控制的演进位:未来可加 `abi-revision-set`(一包多 ABI)再废弃单值文件。
- **打包工具强制参数**:`pm init --api-level 5` 或 `pm init --abi-revision 0xC7003BF9`(二者互斥)。指定 API level 时工具查 SDK version_history 取对应 ABI revision,**查不到报错**;直接指定 ABI revision 时**必须是 SDK 支持的值,否则报错**——防止「用旧 SDK 编包却声称新 ABI」(RFC-0135:避免「组件用旧 SDK 创建却指定只在更新 SDK 中存在的 ABI,轻则跑不起来重则诡异危险 bug」)。初期参数可选(迁移期),生态就位后转必填。
- **存储开销实测**(RFC-0135 原文实验):最坏 +8KiB 未压缩(FAR 4KiB 对齐 ×2),zstd 压缩后仅 +47 字节(12288→16384 未压缩,447→494 压缩)。空间代价可忽略是「值得为兼容性付出的钱」的定量证据。

### 1.3 不匹配怎么拒:四道门 + stepping stone(RFC-0135/0239)

拒载不是一个开关,是**按阶段的四道门**(前两道 RFC-0135 §Use cases,后两道 RFC-0239):

1. **打包期(SDK 校验)**:工具拒编不支持的 ABI(§1.2)。
2. **组装期(system assembly)**:系统组装器**拒绝集成**平台不支持的包(RFC-0135:「the assembler to reject integrating a package if the system does not support the required package ABI」;RFC-0239 :389-390:「product assembly will refuse to assemble products with components that are not compatible」)。→ BerryOS 对应位 = berry-assemble(04 号 §3.1 暂名)。
3. **装载/运行期(component_manager 拒启动)**:RFC-0239 :380-387「component_manager 默认拒绝启动 ABI revision stamp 不在兼容保证范围内的组件」;api_levels.md 同口径:「如果 OS 二进制不再实现 API level 11 的方法,Component Manager 阻止其启动」。运行时核对的数据源 = OS 二进制内嵌的**受支持 ABI revision 清单**(RFC-0239 :409-412:「OS binaries……包含所有 supported/sunset 阶段 API level 的 ABI revision 清单 + 本 release 自己的 ABI revision;默认只允许打了这些戳的组件运行」)。
4. **下载/解析期(package resolver)**:ephemeral 包可按 ABI revision 拒下载(RFC-0135)——但注意 RFC-0135 专门讨论了 **OTA 陷阱**:更新包里的新系统组件天然带「未来 ABI」,不能被当前系统的 resolver 拒掉,否则 OTA 断链。解法 = **stepping stone release**:A(ABI-1)→ B(同时支持 ABI-1/ABI-2,base 包还在 ABI-1)→ C(ABI-2),标记 B 为不可跳过的中转版;更彻底的方案 = 把「包布局版本」与 ABI revision 解耦,声明每个 ABI revision 支持哪些包布局版本。→ **BerryOS 对应教训(推断)**:四层 OTA 设计(`docs/更新与OTA设计-v1.0.md`)若引入 ABI revision 拒载门,必须先答「升级过程中的新组件谁来放行」——stepping stone 或布局解耦,二选一。

**豁免通道**:product owner 可选择性关闭检查(如 allowlist 个别组件打 NEXT/HEAD),「风险自担」(RFC-0239 :392-395)。豁免是产品方权限不是开发者权限——与 BerryOS「能力缺失即显式失败,不造灰色 API 层」(章程 §四-3)同向。

### 1.4 SDK categories:分类执法链怎么建(RFC-0165 → RFC-0268)

**初版(RFC-0165,2022)**:六档 excluded / experimental / internal / **cts** / partner / public,「受众单调递增的有序表」。执法链三件:
- GN `sdk_atom` 模板的 `category` 参数(逐 atom 标注);
- 构建系统执法:SDK target(如 `//sdk:core`)也有 category,**atom 的 category 必须不高于 SDK target 的 audience 才能进**(partner SDK 可含 public atom,不能含 internal atom)——构建期机器执法,非评审自律;
- 与能力路由互补(RFC-0165:没有能力路由,组件作者会复制内部 FIDL 定义;没有 categories,作者会依赖内部协议互通)。

**修订版(RFC-0268,2024)——一次真实的分类体系演化,教训丰富**:

| 新档 | 前身 | 可用于生产终端设备? | OOT 开发者可直接用(进 SDK)? | 变更检测执法? | 可不稳定(HEAD)? |
|---|---|---|---|---|---|
| compat_test | cts(未实现) | 否 | 否 | **是** | 否 |
| host_tool | partner_internal 的一部分 | 否 | 否 | 是 | 否 |
| prebuilt | partner_internal 的一部分 | **是** | 否 | 是 | 否 |
| partner | — | 是 | **是** | 是 | 是(仅此档) |

(表为 RFC-0268 Design 节汇总表的忠实转写)

关键演化事实:①**public 从未实现即被移除**(「Never implemented」)——文档化的档位与实际执行的档位是两回事,public 空转两年;②internal 被 burn down(「Equivalent to not specifying an SDK category」);③**compat_test 档专为解决「CTF 测试需要稳定 API 但不想付进 SDK 的成本」**(issue 367760026)——测试面自己成为一种受执法的暴露级别;④ partner_internal 因「名字含 internal 引发误解」被拆成 host_tool/prebuilt 两档——**分类词表本身也要废弃流程**;⑤各档的 ABI 兼容窗不同但 partner/prebuilt 相同(「as long as any API levels that support them are in the Supported or Sunset phase」)。

**对 BerryOS 的直接映射(推断)**:章程 L0–L4 是承诺分级(「说什么」),还缺暴露分级(「给谁用」)与执法链(「谁拦」)——BerryOS SDK 若外发,需要 atom 级 category 标注 + 发布工具执法(哪些面允许进 OEM 工件/测试工件/公开 SDK),对应 §6-9。

### 1.5 CTS/CTF:测试怎么版本绑定、怎么防漂移(RFC-0015/0141 + 现行文档)

**设计原则(RFC-0015)**,条款级:
1. 「每个平台面元素的每个**已文档化行为**都要有 CTF 测试」(长期硬要求,成文时先软);
2. 测试**不得依赖特定系统镜像的任何内部细节**;依赖的平台代码若非被测对象,必须作为 CTF 工件捆绑且同样不依赖内部细节;
3. 测试与特性**同 CL 开发**(同仓库 in-tree,免「CTF 与版本对齐机器」);
4. 「必须能**判定一份 CTF 工件针对的 API level 与 ABI revision**」——版本可判定性是测试工件的一等要求;
5. 构建执法:非白名单构建目标必须用 `cts_` 规则变体(ctf_fuchsia_component/ctf_executable…),白名单文件 `//sdk/ctf/build/allowed_ctf_deps.gni`——**测试只许依赖 SDK 元素或白名单 CTF 代码**,防止测试意外依赖实现细节;
6. 测试框架选型服从「测试不依赖第三方 SDK 依赖」原则(选 zxtest 而非 gtest——gtest 依赖锁,不适合测实现锁的 Zircon 特性);
7. 测试工件随 SDK 发布,**不含工具链**(执行环境自备);
8. 测试间隔离(反 cross-talk):「如果测试改了设备设置组件的全局状态,应考虑局部作用域的设置供词,而非变更全局状态」;
9. **允许 fake 驱动喂 fake 数据**(被测设备可用假驱动,但测试本身不造 mock/fake 目标内部状态)——「CTF 的意图是确保整台设备行为正确,不是组件孤立正确」。

**流程政策(RFC-0141)——三 tier 渐进**:
- **Tier 1(starting)**:向后不兼容的变更导致 SDK roll 失败/canary 失败时,**变更作者负责**为新行为补文档+CTF 测试;「partner 或 public SDK 新增 PlaSA 元素必须配 CTF 测试(带测试计划过 API Review)」;
- **Tier 2(building)**:**所有**对 PlaSA 元素 API/行为的修改(无论新增还是存量)必须伴随覆盖该变更的 CTF 测试;
- **Tier 3(complete)**:每个区域对其拥有的 PlaSA 元素有全覆盖计划并交付。
- 两条经验法则:①「凡按 API 文档 rubric 应文档化的行为都应有测试钉住」;②「**若一个行为变更导致树外应用回归,则必有一个缺失的 CTF 测试**」——把回归当作测试欠账的记账器,这是兼容工程最重要的运营公式(可直接抄给 BCTS:认证设备在野回归 = BCTS 用例欠账)。

**版本绑定机制(现行文档 compatibility_testing.md + faq)——freeze/thaw**:
0. **工件随 SDK 发布(依赖关系在 RFC-0015 立法)**:「与给定 SDK 相关的 CTF 测试打进一个工件,**随该 SDK 一同发布**;工件含树外构建运行所需的构建规则,**不含工具链**」(RFC-0015 :141-144);且「由于 RFC-0002 的软过渡要求,**每个 SDK 构建都应能成功执行同一 SDK 上一构建的 CTF**」(RFC-0015 :282-287)——测试工件与其限定 SDK 的配对是发布纪律,不是事后组装;
1. 每个 milestone release 有对应 release 分支,该实现通过该分支上的测试;
2. 测试工件冻结进该里程碑的 **CTF Artifacts** 包,编译上传 **CIPD**;
3. main 分支把每个 supported API level 的 CTF 工件**下载为 prebuilt 并解冻(thaw)成测试包**;
4. 跑在 main 最新平台上,**失败即挡 CL 提交**——「精确模拟组件用旧 SDK/旧平台构建、以预编译形态跑在新平台上」的场景。
5. 发布节奏:**Canary ≈ 4 小时 / Milestone ≈ 6 周**(faq 表);milestone 分支有 cherry-pick 就重出 CTF 并自动滚入 CI/CQ;
6. **版本绑定的机器表达 = 测试包名**:tip-of-tree `my_test` / canary `my_test_apicanary` / API level N `my_test_api$N`(例 `fuchsia-pkg://fuchsia.com/my_test_api24#meta/my_test.cm`)——CQ 同时跑多个版本的同名测试;
7. CL 被 CTF 挡住时的 **soft transition 四步**:提新行为 CL(让 tip-of-tree 版测试过)→ 通知下游 SDK 用户迁移 → 等下一个 CTF release 滚入 CQ → 提删除旧行为的 CL;
8. 最低运行环境:core.x64 模拟器(全部 CTF 至少在此跑)。

**兼容面清单(PlaSA manifest)**:`manifest.plasa.json`(hub)→ `*.fragment.plasa.json`(各子系统自管),每 fragment 声明 kind(`api_cc` / `api_fidl`)——**平台面的机器可读总清单**,coverage dashboard 按它计算「已测/未测」并给测试作者指路。选 hub-and-spoke 的理由:子系统可各自演进格式、可并行生成、避免大合并瓶颈。→ BerryOS 对应物:abi.toml 是 syscall 面的单源,但 wire 协议面/库 API 面还没有统一清单(推断,§6-2)。

### 1.6 Fuchsia 双轨版本纪律(与 §5 联动)

- **API 兼容保证(构建时)**:「端开发者能用**某**个 IDK release 针对数字 API level N 成功构建,就能用 N 处于 supported 阶段的**任何** IDK release 构建同一份源码」(RFC-0239 :324-327);违反 = 平台 bug。
- **ABI 兼容保证(运行时)**:「针对数字 API level N 构建的组件能在 N 处于 supported/sunset 阶段的任何 Fuchsia release 上运行,且看不到 objectionable 行为变化」(RFC-0239 :350-353);违反 = 平台 bug。
- **NEXT/HEAD 无保证且必须精确匹配**:NEXT/HEAD 组件打 **per-release ABI revision**(RFC-0239 :405-407:「每个 release 也分配唯一 ABI revision」),只在构建它的同一 release 上跑(差一个 build number 都不行,例:IDK 16.20231103.**1**.1 构建的 NEXT 组件不跑在 16.20231103.**2**.1 上)。
- **phase 推进节奏由 partner 驱动**:「API levels 以 partner 允许的速度尽快推进 phase——一旦没有 partner 需要构建,就 sunset;一旦没有 partner 想在新 Fuchsia 上跑,就 retire」(RFC-0239 :313-317)。兼容窗口长度不是技术决定而是**客户构成决定**。

---

## §2 Windows:驱动兼容工程(WHQL/HLK + 签名链 + DCH)

### 2.1 测试体系:HLK

- **形态**:Windows Hardware Lab Kit = 测试框架 + HLK Studio + HLK Manager;**按 OS 版本发行专用包**(Win11 26H1/25H2/24H2、Server 2025/2022…各一个 kit,认证哪个版本用哪个 kit);VHLK = 预装好的 VHDX 虚拟机版(「Certify using the Virtual HLK」)。HLK 只支持 Win10/Server 2016+(更老版本用 HCK/WLK)——**测试套件与 OS 版本一一绑定**是 Windows 的基本纪律。
- **测试分类入口 = 官方 playlist**:「Windows Hardware Compatibility Program uses an official playlist to determine which devices meet the requirements」,playzip 官方发布(aka.ms/HLKPlaylist),且 Arm64 playlist 正在与 x86/x64 合并(2026-11 公告)——**测什么由平台方发布的清单定义**,不是 HLK 里有什么测什么。
- **认证流程八步(HLK Getting Started)**:①测试服务器装 Controller+Studio → ②被测系统装 Client → ③建 machine pool → ④建 project → ⑤选被测 target → ⑥选并跑测试 → ⑦看结果与日志 → ⑧**产 submission package(.hlkx)**。hlkx 打包要点(step 8 页):鼓励附 **symbols**(供 Microsoft 自动崩溃分析)、可附 **errata/contingency**(免测豁免件)、多包可 **merge** 成一个提交;**所有提交必须数字签名,Win10 起还须 EV 证书**。
- **规格文档体系**:WHCP「Specifications and Policies」按 OS 版本成套发布(Win11 25H2&26H1/24H2/22H2/…Win10 2004 回溯到 1511;Server 独立)——**要求文档与测试工具与 OS 版本三者同步版本化**。
- **处理时限**:「All hardware submissions are processed within **five business days**,取决于是否需要人工复审(测试失败/无有效 filter/内部商业政策会触发人工)」(hardware-submission-create 页)。

### 2.2 驱动签名链:三级通道与时间线

**时间线(Driver Signing Policy 页,官方表)**:

| 时代 | 内核驱动签名要求 |
|---|---|
| Vista/Win7 | 64 位需签(嵌入式或 catalog),标准 Code Integrity 信任根;32 位免 |
| Win8/8.1、Win10 1507/1511(Secure Boot 开) | 同上,SHA-2 |
| **Win10 1607+(Secure Boot 开)** | **仅 Dev Portal(Hardware Dev Center)签名的新内核驱动可载**;EV 证书建立 dashboard 账户 |
| Win10 1803+(Secure Boot 开) | cross-signed 驱动不再加载 |

cross-signed 例外三条(1607+):从旧版升级上来的机器 / BIOS 关 Secure Boot / **2015-07-29 前签发**且链到受支持交叉 CA 的最终实体证书——「老签老机器可用,新签新机器不行」的**按签发时间切割**。boot 驱动不阻止启动(防变砖)但被程序兼容性助手移除。

**三条签名通道(driver-signing-offerings 页)**:
1. **HLK 认证签**(正道):过 HLK 测试→hlkx 提交→Microsoft 签;可上 Windows Update retail;Server 2016+ **只认**这条通道的设备/过滤驱动。
2. **attestation 签**(免测通道):EV 证书 + CAB(驱动+INF+PDB+CAT)→ Partner Center 提交 → Microsoft 重签。限制清单(原文要点):不能上 WU retail(CoDev/SSRK 测试通道可)/仅 Win10 Desktop+/**不算 Windows Certified**(「attestation 签名表明 Windows 信任该驱动,但因未经 HLK Studio 测试,对兼容性与功能性**零保证**」)/ELAM、Windows Hello 等 PE level 拿不到/DUA 不支持。**Microsoft 重签流程**:附加 Microsoft SHA-2 嵌入签名(客户自签则覆盖)+ 用 Microsoft 证书重造 catalog 替换客户 catalog——签名权最终在平台手,客户签名只是提交凭据。
3. **preproduction 签**(开发期):仅显式 provision 过的测试机信任,retail 系统不载;用于 Secure Boot 开着做 bring-up/HVCI 交互测试。

**费率(如实记录)**:现行 Learn 文档全链(注册→EV→提交→签名)不再列任何按次提交费用;注册要求 = EV 证书(向 CA 购买,价格随 CA)+ Microsoft Entra ID 全局管理员 + 法律联系人 + 三协议(Code Signing / WHCP / MMLA)+ 问卷。**历史 WHQL 按次收费的金额与免费化的确切时点:待核实**(公开现行文档已不载;推断:费用门槛已从「按次收费」迁移为「EV 证书+企业身份+协议」的前置成本——对 BerryOS 的启示是认证门槛的形态选择,见 §6-12)。

### 2.3 DCH 设计原则:驱动组件化与 OEM 解耦

**D = Declarative / C = Componentized / H = Hardware Support App**(官方定义,develop/dch-principles-best-practices 页):
- **Declarative**:只用声明式 INF 指令安装,禁 co-installer/RegisterDll——安装行为可静态分析;
- **Componentized**:「**版本特定、OEM 特定、可选定制与基础驱动包分离**——基础包只含核心设备功能,可独立于定制被 target/flight/服务」;
- **HSA**:驱动关联的一切 UI 打包为 HSA(UWP/MSIX),**必须经 Microsoft Store 分发更新**。

**治理结构(该页 Overview 节,条款级)**:
- DCH 包 = 基础包(IHV 写)+ 可选组件包(**extension INF**,OEM 写)+ 可选 HSA;
- 「IHV 认证基础包**一次**,即可部署到**所有** OEM 系统——因为基础包可跨所有共享该硬件部件的系统使用,**Microsoft 能广测它**(用 Windows Insider flighting 而非限定特定机型);OEM 只验证自己提供的可选定制」;
- 工具执法:InfVerif(「infverif /k 通过」= Declarative 达标)+ WDK 工程 Target Platform=Universal(自动挂对库+INF 校验+ApiValidator)。

→ **对 BerryOS 的直译(推断)**:这就是 04 号 §2.3/§5-8(驱动=基础件由上游/芯片厂认证一次,OEM 产品配置=extension 只测增量)的业界实证——**分层测试分离**是认证成本可控的关键:总测试量 = Σ(IHV 基础件 ×1) + Σ(OEM 扩展件 ×1),而不是 Σ(整机 × 全量)。

---

## §3 Linux 发行版治理:发行方-厂商关系的开源侧样本

### 3.1 Debian derivatives:上游与衍生版的关系模式

Debian 官方 derivatives 页(www.debian.org/derivatives/,一手)的定义与姿态:
- **定义三要素**:「基于 Debian 的工作成果/有自己的身份、目标与受众/由独立于 Debian 的实体创建」;
- **官方姿态**:「Debian 欢迎并鼓励想基于 Debian 开发新发行版的组织」+「本着社会契约精神,**希望衍生版把工作贡献回 Debian 与上游项目**」——欢迎但不强制,回馈是道德期待不是合同义务;
- **登记机制**:derivatives census(wiki 维护的衍生版普查,每个衍生版一页:Grml/Kali/PureOS/Tails/Ubuntu 均在列)——**登记/普查是 Debian 对衍生版的全部治理动作**(无认证、无商标执法、无兼容测试)。
- 衍生版指导细则页(wiki Derivatives/Guidance)**采集时被源站 IP 封禁未能核验,标待核实**;其存在本身(有专门指导页)经 Debian derivatives 官方页链接可证。

**模式判定(推断)**:Debian = 「无合同上游」——关系靠社会契约与工具链(debian/ 目录、salsa)维系,衍生版碎片化零执法。这是 Linux 世界「发行方-厂商」关系的**下界样本**:上游无认证权时,衍生版(如当年 Ubuntu 早期)与上游的兼容性完全靠人。BerryOS 若走 Apache-2.0 纯开源路线而不建认证,终态即此(04 号 §3.4-1 已判断「许可证不禁止 fork」)。

### 3.2 Ubuntu OEM 认证:商业绑定的认证

- **准入门槛(一手,certification.canonical.com/docs/programmes)**:「Ubuntu Hardware Certification **仅对 Canonical OEM Partner Programme 成员开放**——须有商业支持或分销协议,或加入 Server Programme 并持有效 Server Certification 合同」。→ **认证是商业关系的附属物,不是开放考试**。
- **测试内容(ubuntu.com/certified)**:「Canonical 认证团队执行 **500+ 项** OS 兼容聚焦硬件测试」,覆盖面列表:Audio/Ethernet/Wireless/TPM/Bluetooth/USB/Firmware/电源管理/WWAN/媒体卡/内存/CPU;认证目录按 Laptop/Desktop/Server/IoT/SoC 分类公开可查(如 Dell XPS 13 Plus、ThinkPad P16、RPi 5)。
- **持续回归**:「大约每 3 周,Ubuntu 发布 Stable Release Updates……这些更新由 Hardware Certification 团队**在认证系统上测试**」「Regular testing for up to 12 years」——认证不是一次性考试,是**与 SRU 节奏绑定的持续回归义务**(12 年 = Ubuntu LTS+ESM 的生命周期)。
- **测试工具开源**:checkbox(Canonical 官方 GitHub,README:「Checkbox is a testing framework used to validate device compatibility with Ubuntu Linux. It's the testing tool developed for the purposes of the Ubuntu Certification program」)——**工具人人可用,但「认证」这个结论只有 Canonical 能出**。测试报告格式 HTML/JSON/JUnit/plain text。
- → 与 Android CTS(免费、开源、任何厂商可自测过 CTS 拿 Play 资格,02 号 §3.1)对照:**Ubuntu 模式 = 工具开放+结论封闭+商业前置;CTS 模式 = 工具开放+结论开放+商业杠杆在别处(GMS)**。BCTS 选位 = 开放问题(§7-2,04 号 §4.5-5 已开)。

### 3.3 OEM 内核与 HWE:厂商定制进主档的通道

(documentation.ubuntu.com/kernel 一手)

**OEM 内核**:
- 定义:「为 OEM 项目特别设计与定制的优化版 Ubuntu 内核变体,典型场景是**预装 Ubuntu 的硬件**」;
- **生命周期短于通用内核,退出机制 = roll-off**:「一旦全部修复前向移植完成,就滚动并入下一个 HWE 内核」——OEM 内核是**过渡形态而非永驻形态**,定制点最终沉回主档 HWE;
- 实例表(24.04 LTS):6.11(devel 2024-08 / stable 2024-11 / **EOL 2025-07** / 迁移目标 6.14)、6.14(m: linux-oem-24.04c)……命名法 = `linux-oem-<Ubuntu 版本><字母序号>`(24.04a/b/c 递进),源码在 kernel.ubuntu.com forgejo;
- → **机制本质(推断)**:OEM 的深度定制(Dell/Lenovo 特定机型的内核)不是 fork 发行版,而是**主档内的一条滚动通道**——每个 OEM 内核有明确 EOL 与迁移目标,定制债被强制摊销。这正是 04 号 §3.2「上游升级=厂商包 rebase」在内核维度的实证。

**HWE 内核**:仅 LTS 启用;随每个非 LTS 新系列发布而滚动(GA 内核 → HWE → 更高 HWE),直到下一个 LTS 收口;每个 HWE 有 EOL 与 ESM 日期。→ 「给老发行版持续换新内核以支持新硬件」的官方节奏化通道,BerryOS 对应物 = 未来驱动面「L3 固件随驱动包」(`docs/更新与OTA设计-v1.0.md` §1)的版本化纪律(开放)。

### 3.4 Launchpad PPA 与主档的分界

(help.launchpad.net/reference/packaging/ppas/ppa,一手,条款级):
- **只收源码包**:「Uploadable artefacts: **Source packages only; pre-built binary uploads are rejected**」——Launchpad 代替构建,保证所有 PPA 包在 Canonical 的构建农场出、架构覆盖可控(amd64 默认,arm64/riscv64 等按需);
- **配额**:磁盘 8 GiB(可申请扩)、无固定流量上限(异常量触发联系);
- **签名**:「Launchpad 为每个 owner 的默认 PPA 生成**唯一 GPG key**,同 owner 所有 PPA 复用」——PPA key 与 Ubuntu 主档 key **天然分离**,用户添加 PPA 需手动信任(trust-on-first-use 逐 PPA);
- **可见性**:默认公开,**私有 PPA 需 Canonical 审批**——OEM 私有交付(未发布机型的驱动)走私有 PPA,又一道商业关系管道;
- **保留**:包发布后保留到 owner 删除或被新版本 supersede。

**分界判定(推断)**:主档 vs PPA 的分界不是「谁能发布」(都能),而是**信任域 + 构建权**:主档 = Canonical 拥有的 key + 审批 + Ubuntu 发布承诺;PPA = owner 自己的 key + 自由发布 + 用户自担信任。→ BerryOS 对应:官方源 vs OEM 源(SDK-2 §五「OEM 镜像内置 OEM 源静态实例」)的分界设计应抄这个「**构建权与签名域一起划分**」的结构,而不是只分内容(开放问题 §7-5)。

---

## §4 Rust 生态的 ABI 现实:「二进制工件平台」怎么做

### 4.1 命题复述与结构论证

01 号 §1.2 立的判断:「应用静态链接自兜底意味着兼容问题收敛到 wire 协议 + syscall 两处」。结构论证(全部仓库事实):
- 应用与其依赖的全部库(berry-ui/berry-gfx/协议 crate/berry-libc)静态链接进 .bap,ELF 自包含(`docs/应用规范-v1.0.md` §3 + ADR #55)——应用与平台之间**没有共享库 ABI 面**;
- 中间不设稳定 ABI、berry_* syscall + wire 是唯二冻结面(ADR #55 两冻结面 + `docs/API治理章程-v1.0.md` §一 :18「冻结面 = L0+L1」);
- 运行时环境面:无动态链接器、无 fork/exec 继承环境(AGENTS.md「v1 无 fork、不跑 Linux 二进制」)——**没有 LD_LIBRARY_PATH 类的环境依赖通道**;
- 因此应用在设备上可能遭遇的「别人改了接口」只剩:①syscall(L0,永冻+abi-check 执法);②wire 协议(L1,add-only+per-interface version);③(运行期语义漂移——由三值版本域+双轨拦,`docs/SDK-2-双轨与演化纪律设计-v1.0.md` §一/§二)。

### 4.2 C 生态反例排查:glibc 的 symbol versioning 与 memcpy 事件

**机制(glibc 的兼容工程,业界最强 C 样本)**:
- **symbol versioning(.symver)**:glibc 导出符号带版本标签(如 `memcpy@GLIBC_2.14`、`pthread_create@GLIBC_2.2.5`),链接时 ELF 记录所需版本集合,动态链接器装载期核对——**同一符号名可并存多个语义版本**,这是「不改旧义」的机器执法;
- **承诺形态**:soname(libc.so.6)自 glibc 2.x 起从未改变(即 1997 年至今);符号只加不删;**向后兼容**(新 glibc 跑旧二进制)成立,**向前兼容**(旧 glibc 跑新二进制)不成立——新二进制在旧系统上死于 `version 'GLIBC_2.XX' not found`(动态链接器装载期拒,机制上就是 §1.3 第 3 道门的 C 生态形态);
- **rename 手法**:行为必须变时,不改旧符号而是**新开版本符号**(如 `__isoc99_sscanf` 系),旧符号保留旧义——与 BerryOS「msg_type 只加新号」(章程 §一 L1 行)完全同构。

**memcpy 事件(Hyrum 定律的旗舰案例)**:
- 一手:GNU C Library 2.14 版 NEWS(glibc 源码树,tag glibc-2.14)行 298-299 记载:「**Faster memcpy on x86.** Implemented by Ulrich Drepper.」——语义未变(仍是「复制 n 字节」),仅实现优化;
- 后果:2011 年该优化改变了大块复制的方向等实现行为,大量**依赖未文档化实现细节**(假定 memcpy 逐字节向前)的程序(Flash/Steam/部分 Java 应用等)在升级 glibc 后崩溃——业界广泛记录(二手共识;具体受损软件清单与规模数字标待核实);
- **结构修复(一手,现行源码)**:今日 glibc 的 x86_64 实现把 memcpy 定义为 **memmove 的 strong_alias**(`sysdeps/x86_64/multiarch/memmove-vec-unaligned-erms.S` 行 888-901:memcpy/memcpy_chk 均 alias 到 __memmove),并在文件头注释明写实现策略「Load all sources into registers and store them together to avoid **possible address overlap**」——**修复不是回滚优化,而是把 memcpy 的实现语义与 memmove 合并,使「依赖拷贝方向」在结构上不可能**。这是「实现细节泄漏成 ABI 之后,要么永久垫片要么结构性消灭」的教科书答案(对应 BerryOS 章程 §五 Hyrum 二分与 Fuchsia RFC-0141 法则②的 C 生态版)。

**排查结论(推断)**:glibc 案例不推翻「两处收敛」判断,反而强化它——glibc 已经有 symbol versioning+add-only+27 年 soname 稳定的最强纪律,依然因为**共享库边界上实现细节可观测**而翻车;BerryOS 把共享库边界整个移除(静态链接),Hyrum 风险面从「每个符号的实现行为」缩到「syscall 语义 + wire 语义」两小面。代价是把「库升级」变成「应用重编」,而这正是 .bap 工件模型接受的代价(「废弃 API 编进已发布 .bap 即成 OS 永不可回收的静态面」,章程 §三-3)。

### 4.3 Rust crate semver 的失效模式(官方自认)

cargo 官方 SemVer Compatibility 指南(doc.rust-lang.org/cargo/reference/semver.html,一手)自己写清了失效面:
- **「这些只是指南,不一定是所有项目都会遵守的硬规则」**(开篇原文)——semver 在 Rust 生态是**约定无执法**:cargo 的版本解析只看版本号,不验证 API 是否真兼容;rustc 不做任何 semver 检查;
- **「几乎每个变更都带有负面影响运行时行为的风险,那些情况通常由维护者自行判断」**——语义兼容性机器不可判,官方承认靠人肉;
- 专设 **Possibly-breaking** 类(「some projects may consider major and others minor」)——三分法本身就是失效模式的登记表;条目数量庞大的实例如「glob import 下加新 public item 可能破坏(ambiguous name)」「repr(C) 枚举加 variant 改变 size/alignment」等;
- 0.y.z 规则(「只有最左非零分量算不兼容」)与 1.0 语义割裂——生态大量长驻 0.x 的库每个 minor 都可能破;
- **工具补位的事实**:cargo-semver-checks(独立项目)存在且被广泛使用,README:「Lint your crate API changes for semver violations」「rustdoc JSON 输出格式不稳定,新 Rust 版本可能有 breaking changes」——**连 semver 检查工具自身都被 Rust 工具链的无稳定 ABI 打**。BerryOS 侧对应:协议 crate 族(L2)的 semver 纪律若只有约定,BCTS L2 行为测试(04 号 §4.2)就是唯一的客观执法(推断)。

### 4.4 cargo lockfile 在平台工件里的角色

(cargo 官方 guide,一手):「Cargo.toml 描述宽泛依赖(人写)/ **Cargo.lock 记录依赖的精确信息(cargo 维护,不应手编)**」;存在动机 = **可复现构建**(「你今天构建、我明天构建,中间 regex 有了新 commit,我们会得到不同的构建——这很糟,因为我们要可复现构建」)。官方建议:拿不准就提交 lockfile 进版本控制。

**对 BerryOS 的角色(推断)**:在「二进制工件平台」模型里,Cargo.lock + rustc 版本 + 工具链版本 三元组 = **工件的完整构建指纹**——BerryOS 平台工件(04 号 §3.1:KERNEL.ELF+基座 rootfs+协议 crate+工具链版本+BCTS)应把这三者作为发布元数据随工件走,使「同工件同清单重组,镜像 hash 一致」(04 号 §4.5-3)有可判定的输入集。现役 justfile 已隐式如此(构建环境=本仓 toolchain),外化为平台工件时须显式化(开放问题 §7-6)。

### 4.5 反例排查总表:守住「两处收敛」的边界条件

| # | 反例场景 | 是否推翻「两处收敛」 | 边界条件(必须立法守住) |
|---|---|---|---|
| 1 | glibc memcpy(共享库实现细节泄漏) | 否——BerryOS 无共享库边界 | 禁止引入动态链接应用依赖面(永冻);`.bap` 内 ELF 静态自包含进 mkapp 门禁(现役五道门禁维持) |
| 2 | Rust crate semver 失效(库 API 漂移) | 否——库随应用静态走,不跨进程共享 | 协议 crate 的 L2 semver 纪律必须配 BCTS L2 行为测试执法;否则 semver 仅是约定(§4.3) |
| 3 | 三方预编译库(闭源 SDK/binary blob) | **是——唯一的破口**:预编译 .a/.so 进入 berry 侧构建时,其内部依赖的「平台面」版本必须显式 | blob 声明政策(04 号 §2.3:来源/许可/哈希)+ **预编译件必须声明其锚定的 syscall/wire 快照号**(对应 Fuchsia prebuilt 档,RFC-0268)——BerryOS 现无此立法,**新增缺口(推断)** |
| 4 | rustc 版本漂移(同源码不同编译器产物不同) | 否——产物进 .bap 后冻结 | 平台工件随附工具链版本元数据(§4.4);认证重组时用同版本重编 |
| 5 | 应用间 IPC 的「协议实现分叉」(两家应用各自说同 wire 协议但行为不同) | 否——这正是 wire 面收敛的意义 | L1 契约测试(BCTS L1 替换实现一致性,04 号 §4.2)是替换面开放的充分条件——已在 04 号立法位 |

---

## §5 版本纪律的数学

### 5.1 add-only 词表的规模上限与退出机制

**BerryOS 现役词表与上限(仓库事实)**:
- syscall:nr_total=37 / implemented=36 / **nr_ceiling=45**(`tools/abi.toml` :23-38)——号空间上限是立法常量,生成器执法 nr_defined ≤ ceiling;剩余预算 = 45−37 = **8 个号**;
- intent:六档 0x00–0x05 冻结,**0x06..0x0F 保留拒绝**(ADR #42/#58 + `user/berry-manifest/src/lib.rs:140` 词表镜像)——u8 值域 256,显式封顶 16;
- wakesrc:add-only 纪律原文(`tools/abi.toml` :40-42):「新源只追加(不改值/不改义/不复用退役值);0x08–0x7E 为预留段」,值域 u8 且 ≤ 0x7F;
- 度量链槽位:0–4 已分配,5–15 预留(01 号 §2 引安全设计 §4)。

**数学性质(推断)**:add-only 词表是**单调递增的有界集合**,其寿命 = 剩余预算 / 追加速率。上列四个词表的「预算耗尽时点」都是可计算的;号空间宽度本身是被治理动作反复调整的参数——Fuchsia 对照:API level 定义初为 64 位(RFC-0002 原文),后经 RFC-0083/RFC-0246 调整为 32 位整数(RFC-0002 注:「The definition of an API level has been updated by both RFC-0083 and RFC-0246. As of RFC-0246, they are 32-bit integers」),采集时点已发到第 33 个——**号空间宽度与实际用量的比值才是词表寿命的判据,BerryOS 的 45 帽/16 帽/0x7F 帽在 v1 期均宽裕,但增速须随发版审计**(开放问题 §7-4)。
**退出机制只有两种**(业界无一例外):①**语义退出不退号**——Fuchsia deprecated→legacy→removed 五阶段(§1.1),号保留、语义垫片可撤;glibc 符号永不删;BerryOS L0「只加号不回收」(章程 §一)+ intent「保留拒绝」同为语义退出。②**整代表换域**——glibc libc5→libc6(soname 换代,上一次是 1997 年),代价是全生态重链;对应 BerryOS「换代必须自带自动迁移」(章程 §四-1)。**不存在第三种「删号复用」**——wakesrc 纪律明写「不复用退役值」,与业界一致。

### 5.2 N=3 废弃窗口的业界对照

BerryOS:废弃到移除 ≥N 个 minor 版,N=3 起步(「HDI 先例留 4 版,取 3 因生态为零可紧后放宽,放宽易收紧难」,章程 §三-4)。业界对照:

| 体系 | 窗口形态 | 窗口长度机制 | 来源 |
|---|---|---|---|
| Fuchsia | ABI 兼容窗 = API level 处于 **Supported+Sunset** 阶段 | **partner 驱动**(「没有 partner 需要构建即 sunset,没有 partner 要跑即 retire」)——窗口长度=客户构成,不承诺固定 N | RFC-0239 :313-317/§1.6 |
| Fuchsia(早期 partner API) | 六周兼容窗(2020-2022) | 单批不兼容变更的节奏化(每六周改 ABI revision 批量发布) | RFC-0165 :128-129;RFC-0015 :44 |
| Android(targetSdk) | Play 上架门槛随年度抬 target | 商业杠杆+年度节奏(02 号 §3.2) | 02 号转引 source.android.com |
| glibc | **无限**(符号 add-only 永不删) | 「不破坏任何既有二进制」的绝对承诺,代价=永久兼容面 | §4.2 机制 |
| Windows(Win32) | **事实上无限**(向后兼容几十年) | SupportedOS GUID + shim 层(AppCompat)——RFC-0002 转述其教训:「Windows 错过了把 target ABI revision 嵌入二进制的时机,现在用启发式猜,造成巨大开发者痛苦」 | RFC-0002 §Prior Art/§Why now |

**数学(推断)**:窗口 N 必须满足 N × 发版周期 ≥ 生态最大迁移周期;N 的成本 = 维护 N 个语义版本并行的实现与测试成本(CTF 正是把「测试 × phase」矩阵做成了 freeze/thaw 工厂,§1.5)。BerryOS N=3×minor 的实际含义:若 minor 周期 3 个月,窗口 = 9 个月,大于手机 OEM 跟版周期(6-12 个月,04 号 §8.2-2)的下沿——**窗口与 OEM 周期的匹配度是 N 值的真正判据**,数值留 ADR。

### 5.3 semver 在系统接口上的失效模式

**失效模式一:三段式没有「语义快照」轴**。semver 描述的是**接口形状的变化方向**(major=破,min patch=兼容),不描述「当前语义基线是什么」。系统接口的问题恰是「形状不变、语义漂」(glibc memcpy:签名一个字没动,行为变了,NEWS 只有一行「Faster memcpy」)。RFC-0002 一句权威表述:「**In the limit, every change in semantics is potentially backwards-incompatible, but, in practice, operating systems do make changes to their semantics without breaking their applications**(原则上每个语义变更都潜在不兼容,但实践中 OS 确实在不破坏应用的前提下改语义)」——semver 无法区分这两种「改」,所以 Fuchsia 才需要把「语义快照」独立成 ABI revision 一根轴。BerryOS 的对应双轴 = `[compat].target`(语义档,开发者声明)+ `abi_revision`(平台语义快照号,构建工具自动打戳)——`docs/SDK-2-双轨与演化纪律设计-v1.0.md` §二的分工设计与此同构,该文档「立法窗口:生态为零,装机成本为零;事后补装需全量重打包(Linux 课)」正是 Windows 教训(§5.2 表末行)的引用。

**失效模式二:兼容不是全序**。semver 假设版本可比(major>minor>patch 全序),但系统兼容是**按维度的偏序**:Fuchsia 把 API 兼容(构建时,Source 兼容)与 ABI 兼容(运行时,二进制兼容)**拆成两个独立保证**(§1.6 两条原文)——同一 API level 上「能编译」与「能跑」是两份承诺,各有各的违反判定。semver 单轴无法表达。

**失效模式三:「possibly-breaking」不可判定**(§4.3)——语义兼容判定需要全知全部下游用法(Hyrum 定律),机器不可判,cargo 官方指南只能给「指南」。

### 5.4 Fuchsia「ABI revision 单调不兼容」×「SDK 版本多兼容」双轨的数学

- **ABI revision 轨**:新 ABI revision ⟺ 声明一次不兼容;**任意两个不同 ABI revision 之间零兼容承诺**(除了「平台支持列表」定义的兼容关系);支持列表只增难减(supported→sunset→retired 单向);NEXT/HEAD 用 per-release ABI revision 实现「精确匹配才可跑」(§1.6)。→ 这是**离散语义快照集**模型:每个快照是原子点,兼容性只存在于「平台声明支持该点」。
- **SDK/API level 轨**:IDK 版本多兼容——API 兼容保证原文(§1.6)允许开发者「fearlessly upgrade or roll back their IDK」(在同一 supported API level 内);API level 不可变保证两个 IDK release 对同一 level 的 API 集合恒等。
- **双轨咬合**:包只携带 ABI revision(不带 API level)——运行时只需要查「这个戳在支持列表里吗」,O(1) 判定;API level 只活在构建期(决定哪些声明可见)。「构建期多兼容、运行时点判定」的分工使两边各自最优。
- **BerryOS 对位(仓库事实+推断)**:三值版本域(min ≤ target ≤ compile,`tools/mkapp.py:341-368` 执法,SDK_CURRENT=0.1.0 :131)≈ API level 轨的连续化版本;abi_revision(u16)≈ ABI revision 轨但**现役只用于遥测与分发门槛统计,无运行时拒载**(`docs/SDK-2-双轨与演化纪律设计-v1.0.md` §二「appd 装载期读取:……abi_revision 用于遥测与分发门槛统计」)——即 Fuchsia 的第 3 道门(装载期拒)BerryOS 未启用,**是否启用是 §7-3 开放问题**(安全轨即时生效的设计使 BerryOS 对运行时拒载的依赖低于 Fuchsia,但 compat 轨的 target 分档逻辑仍是装载期判定)。

---

## §6 BCTS 的工程形态建议清单(每条注明抄自哪家哪机制)

> 以下为工程建议(推断),供 BCTS 立法稿取用;每条注明「抄自哪家哪机制」+ BerryOS 现役对应物 + 缺口。

1. **BCTS 工件 = 冻结测试件,不是活测试树**(抄 Fuchsia CTF freeze/thaw,compatibility_testing.md §Catching):每个上游 minor/major 版本发布时把当版测试集冻结成带版本号的工件;认证某设备 = 用「设备声称兼容的版本」的冻结件在其上回放。**BerryOS 形态**:abi 冻结快照机制已有(`abi@<short-sha>.toml` 快照+hash 钉黄金字节,双轨设计 §四)——把 BCTS 工件 hash 钉在同一个快照点上;现役缺口 = smoke 标记是活树(01 号 §3-4)。
2. **兼容面机器可读清单(PlaSA manifest 同构)**(抄 Fuchsia manifest.plasa.json + fragments):L0 面 = abi.toml(已有);**补 L1 面清单**(每 wire 协议的 op 全表,协议 crate 可生成)与 L2 面清单(公开库 API 表)——BCTS 覆盖率报表按此清单计算,「未测面」给测试作者指路。现役缺口:无统一 L1/L2 清单文件(推断,协议 crate 有 host 单测但无对外清单)。
3. **测试只许依赖承诺面**(抄 Fuchsia RFC-0015 `cts_` 规则变体 + allowed_ctf_deps.gni 白名单):BCTS 测试工件构建时机器执法「依赖闭包 ⊆ L0/L1/L2 面 + 测试白名单」——防止 BCTS 测试自身依赖平台内部细节而变成隐性契约。BerryOS 形态:bcts 构建规则独立于平台构建规则,依赖白名单文件进仓。
4. **三 tier 覆盖政策**(抄 RFC-0141):Tier 1 = 凡 compat 轨登记条(`docs/API行为变更登记表.md`,双轨设计 §一)引发在野回归,变更作者补 BCTS 用例;Tier 2 = 一切 L0-L2 变更 PR 伴随用例;Tier 3 = 全覆盖。**Tier 1 先行,成本最低**;「在野回归 = 用例欠账」的记账公式直接采(§1.5)。
5. **测试包名版本绑定**(抄 CTF `my_test_api$N` 命名):BCTS 测试包/标记名带目标版本尾缀(如 `_sdk0.1`),CI 同时跑「活树版(不保兼容,立即)」与「冻结版(保兼容,随版本滚入)」两轨——活树版拦新变更的即时破坏,冻结版拦对旧承诺的破坏。
6. **abi_revision 的执法位分四级,分级启用**(抄 RFC-0135/0239 四道门):现役已有 = mkapp 编译期(三值链)+ 未来组装期(berry-assemble 拒入不兼容件);**建议增** = ①mkapp/SDK 校验「abi_revision 必须被当前 SDK 快照支持」(对应 pm 的 SDK 校验,防伪造未来号);②appd 装载期拒载窗口(开放问题 §7-3)。与 abi-check 的关系:**abi-check 管「面的一致性」(本仓五面逐字节),abi_revision 管「语义快照的匹配」(对外承诺)**——两者正交,缺一不可(abi-check 挡不了 OEM 镜像装旧语义包)。
7. **防 OEM 针对测试特调 = 结构性而非流程性**(抄 CTF freeze/thaw 的深层性质 + HLK playlist 官方发布):测项清单与测试工件均由上游签发、版本冻结、hash 钉快照;OEM 拿到的测试是「旧冻结件回放新组装」——特调任何当版行为都会被上一版冻结件抓住;测试执行留痕(hlkx 式 submission package:结果+日志+symbols 一体签名提交,抄 HLK step 8)。**BerryOS 认证提交件 = BCTS 结果包(.hlkx 同构物)+ OEM 三包 + 度量链重组证明**(04 号 §4.5-1/3 的机器化)。
8. **认证位与 abi-check/度量链的分工**(抄 Windows 三分:HLK 测行为/签名给身份/WU 管分发):abi-check(构建期面一致性,本仓 CI + 外化工件)→ BCTS(行为兼容,冻结件回放)→ 签名链+度量链(身份与完整性,SDK-2 既有轨道)→ 认证标记(治理结论,04 号 §4.5)。四层各管一段,**BCTS 不重复做签名验证**(SDK-2 §九负样例已覆盖),abi-check 不做行为测试(职责单一)。
9. **SDK/接口分类执法链**(抄 Fuchsia GN sdk_atom category + RFC-0268 四档):章程 L0-L4 补「暴露维度」——每面标注「平台内部/测试专用(BCTS-only)/OEM 工件可含/公开 SDK 可含」;发布工具(berry-assemble/未来 SDK 打包器)执法「工件 audience ≥ atom category 才可含」。**测试专用档直接抄 compat_test**(RFC-0268:BCTS 需要的稳定测试 harness 面不进公开 SDK,避免测试面被 OEM 应用依赖成伪契约)。
10. **测试专用 API 面**(同上,compat_test 档):BCTS 的 harness 协议(启动被测服务/注入通道/收集断言)自身要有兼容承诺(跨版本可跑),但不进 OEM/开发者 SDK——单独立档单独立法。
11. **基础件/扩展件测试分离**(抄 Windows DCH:IHV base 一次认证全网复用 + OEM 只测 extension):BerryOS 驱动/系统服务包 = 基础件(上游或芯片厂过全量 BCTS 一次,OEM 组装直接复用其认证结论);OEM 产品包/替换件 = 扩展件(只测增量+组装一致性)。**认证成本模型**(推断):总成本 ∝ Σ基础件×1 + ΣOEM×增量,而非 Σ整机×全量——这是 04 号 §8.1-2「契约负担转嫁」抱怨的结构解。
12. **认证的商业模式选位**(对照三样本:Android CTS 免费自测+GMS 商业杠杆 / Ubuntu 认证商业绑定 / Windows EV+协议前置):BCTS 工具与测试集**开源免费可自测**(抄 CTS,02 号 §3.1),「BerryOS 兼容」**结论位**挂 OEM lineage 与信任账本(04 号 §4.4),商业条款(跟版 SLA/费率/商标)不进技术立法——选位本身是开放问题(§7-2)。

---

## §7 开放问题(原样列出,不替拍板)

1. **BCTS 发布节奏与版本绑定的具体机制**:每 minor 冻结还是每 major 冻结?冻结工件的分发形态(CIPD 式托管 vs 随 release tarball)?OEM 自测跑冻结件的执行环境(QEMU virt 基线,对应 CTF 的 core.x64 模拟器底线)由谁定义?
2. **认证商业模式选位**:三样本(Android/Ubuntu/Windows,§6-12)中 BerryOS 取哪个组合;「BerryOS 兼容」结论的签发主体(平台方/基金会/第三方)。
3. **abi_revision 是否启用装载期拒载**:现役设计只做遥测(双轨设计 §二);若启用,OTA 升级路径的 stepping stone/布局解耦怎么设计(Fuchsia RFC-0135 的 OTA 陷阱直接适用);u16 值域 65535 是否够(对照 Fuchsia 随机 u64)。
4. **词表预算管理**:syscall 剩余 8 个号(37/45)的分配治理——是否需要「新号须附 BCTS 用例才准入册」(Tier 2 前置);wakesrc/intent/槽位表各自的预算审计节奏。
5. **OEM 源与主档的信任域设计**(§3.4):PPA 式「构建权+签名域一起划分」是否适用于 berry 在线源(v2);OEM 源的构建农场(谁构建、在哪构建、可否复现)是否进认证门槛。
6. **平台工件的构建指纹**:Cargo.lock+rustc 版本+工具链版本三元组是否作为工件强制元数据(§4.4);认证重组(同输入同 hash)的「同输入」是否以指纹为准。
7. **预编译三方案的快照声明义务**(§4.5-3 破口):闭源 blob/预编译库进入 berry 构建时,是否强制声明其锚定的 syscall/wire 快照号与 rustc 版本;违者拒入(Fuchsia prebuilt 档同构)。
8. **quirk 表机制是否需要**(Fuchsia RFC-0002 legacy components):BerryOS 生态为零时立法成本最低;「个别重要应用不迁移」的退路要不要预留。
9. **BCTS 与 G0-G5 关卡的执法位关系**(04 号 §7-Q4 原样保留,本文未新增证据)。
10. **跟版窗口 N 值与 OEM 周期的匹配**(§5.2):N×发版周期 ≥ OEM 最大跟版周期的具体数值立法。

---

## §8 反面自检(如实写)

1. **Fuchsia 样本的可迁移性存疑(推断)**:Fuchsia 的四机制咬合建立在「平台方完全控制 fuchsia.git + petals 树外开发但工件化交付」的前提上,且其 CTF 覆盖率至今与「每个文档化行为一测」的 RFC 目标有距离(RFC-0141 成文时自认「a handful of tests」;覆盖率推进依赖 tier 政策渐进)——**抄机制容易,抄到同等覆盖率难**;BCTS 若只抄 freeze/thaw 结构而无 tier 政策配套,冻结的可能是空集。
2. **Windows 样本的费率事实缺口**:现行文档不列按次费用(§2.2 待核实项),本文无法给出「认证成本」的完整画像;EV 证书/Partner 注册的实际花费随 CA 与协议变化——BCTS 成本模型(§6-11)的定量部分缺业界锚点。
3. **Rust ABI 论证的时点风险**:Rust 官方对 ABI 稳定性无承诺是当前事实,但 `extern "C"` 之外的原生 ABI 面并非立法冻结——若 Rust 未来稳定部分 ABI(或 wasm 式组件模型普及),§4 的「两处收敛」边界条件需重审;本文按当前事实立论。
4. **单源与被封事实如实登记**:Debian Derivatives/Guidance 页(IP 封禁)未能核验;memcpy 事件的受损软件清单为二手共识;WHQL 历史费率未获一手——均已标注,不参与两源交叉的关键论断。
5. **§6 建议清单的完备性**:12 条覆盖「测什么/怎么版本绑定/怎么防特调/认证位共存」四问,但未覆盖认证**运营**(谁审、审多久、申诉流程——Windows 五工作日是唯一锚点)与 BCTS 自身的**测试质量**问题(CTF 的 flaky 测试同样是业界现实,faq/故障排查文档存在本身即证据)——运营面超出本文证据范围,原样留给立项稿。

---

## 来源索引

### 仓库内(相对路径 + 行号,2026-11-10 采集时点)

- `docs/API治理章程-v1.0.md`——L0–L4 分级表 :10-16;冻结面口径 :18-19;双轨 :21-28;废弃纪律 N=3 :30-41;三硬纪律 :43-50;Hyrum 二分 :52-57;CI 执法 :59-66;新 API 归属 :68-73;ADR 落册 :75-78。
- `docs/SDK-2-双轨与演化纪律设计-v1.0.md`——行为变更登记表字段 :10-25;abi_revision 定义与消费 :26-34;能力集标签 :36-45;abi 冻结快照 :47-56;渠道门槛 :58-64;废弃登记 :66-70;验收线 :72-80。
- `tools/abi.toml`——meta(nr_total=37 :23 / implemented=36 :28 / nr_ceiling=45 :34):21-38;五面生成说明 :4-14;wakesrc add-only 纪律 :40-45。
- `tools/mkapp.py`——三值链执法 :341-368(min≤target≤compile≤SDK_CURRENT :359-368);SDK_CURRENT=(0,1,0) :131-132。
- `research/厂商定制化深研/01-现状盘点-BerryOS定制基础与缺口.md`——§1.2 替换面判断 :36-43;§2 冻结面表 :98-109;§3 缺口 4(无 CTS):124;§4.3 关键判断 :157。
- `research/厂商定制化深研/02-安卓OEM定制教训-碎片化机制根因.md`——§3.1 CTS/GMS(经 04 号 §4.1 转引 :193;04 号来源索引 O9/O10/O11)。
- `research/厂商定制化深研/03-正面模式全景-定制范式对比.md`——Fuchsia 五件套 :75-81;§1.5 组 RFC 清单 :348-350。
- `research/厂商定制化深研/04-BerryOS厂商定制化架构提案.md`——BCTS 原则与分级表 :193-208;能力基线十条 :210-223;认证门槛草案 :238-244;§7 开放问题 :300-314;§8 反面自检 :318-329。
- `docs/更新与OTA设计-v1.0.md`、`docs/应用规范-v1.0.md`、`docs/SDK-2-分发与签名链设计-v1.1.md`、`sched/src/class.rs`(:8 冻结头注)、`sched/src/policy.rs`(:48 WEIGHT_MAX)、`user/berry-manifest/src/lib.rs`(:112 APP_SLOTS、:140 起 Intent 六档)——均经 01/04 号底稿二次引证,行号以彼处采集为准。

### 外部一手(Fuchsia,gitiles 源仓 markdown,fuchsia.googlesource.com/fuchsia/+/refs/heads/main/ 前缀)

- RFC-0002 Platform Versioning:`docs/contribute/governance/rfcs/0002_platform_versioning.md`(ABI revision 随机选取 :132-142;quirk 表 :201-225;生命周期 :228-253;Windows SupportedOS 教训 :49-53/449-460;「every change in semantics」:156-160)。渲染版 https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0002_platform_versioning
- RFC-0135 Package ABI revisions:`docs/contribute/governance/rfcs/0135_package_abi_revision.md`(编码 :66-104;stepping stone :118-157;防撞 :159-176;开销实验 :178-213;pm CLI :215-252)。https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0135_package_abi_revision
- RFC-0165 SDK categories:`docs/contribute/governance/rfcs/0165_sdk_categories.md`(六档 :169-197;六周窗 :127-133;能力路由互补 :45-52;生命周期缺废弃 :151-155)。
- RFC-0268 SDK category updates:`docs/contribute/governance/rfcs/0268_sdk_category_updates.md`(四档表 :138-229;public 移除 :99-105;compat_test 动机 :61-67/231-265;兼容窗 :310-319)。
- RFC-0015 CTS:`docs/contribute/governance/rfcs/0015_cts.md`(设计要求 :86-107;cts_ 规则 :190-201;zxtest :131-139;隔离 :267-276;六周执法 :317-322)。
- RFC-0141 CTS Process:`docs/contribute/governance/rfcs/0141_cts_process.md`(coverage 定义 :101-161;Tier 1-3 :163-262;两法则 :132-152)。
- RFC-0239 Platform Versioning in Practice:`docs/contribute/governance/rfcs/0239_platform_versioning_in_practice.md`(phase 模型 :291-317;API/ABI 保证 :319-368;拒载与实现 :378-412;NEXT/HEAD per-release ABI revision :402-407)。
- `docs/concepts/versioning/api_levels.md`(不可变性 :15-19;ABI revision stamp 拒载 :39-45;phase 定义 :72-138;NEXT/HEAD/PLATFORM :140-202)。
- `sdk/version_history.json`(实测结构:data.api_levels{N:{abi_revision,phase}} + schema_id;2026-11 采集时最新 level 33)。
- `docs/development/testing/ctf/compatibility_testing.md`(freeze/thaw 六步 :165-190)。
- `docs/development/testing/ctf/faq.md`(节奏表 :11-26;包名绑定 :52-70;soft transition 四步 :91-104)。
- `docs/development/testing/ctf/plasa_manifest.md`(manifest/fragment 结构 :64-120;hub-and-spoke 理由 :75-87)。

### 外部一手(Microsoft Learn,curl 采集 HTML 转文本)

- HLK 总览:https://learn.microsoft.com/en-us/windows-hardware/test/hlk/(kit 按 OS 版本表、VHLK、CVE-2024-29187 刷新记录、2026-10 更新版)。
- HLK Step 8 submission package:https://learn.microsoft.com/en-us/windows-hardware/test/hlk/getstarted/step-8-create-a-submission-package(hlkx/symbols/errata/merge/EV 签名)。
- WHCP 总览:https://learn.microsoft.com/en-us/windows-hardware/design/compatibility/(playlist 官方发布)。
- WHCP 规格与政策:https://learn.microsoft.com/en-us/windows-hardware/design/compatibility/whcp-specifications-policies(per-OS 版本套件)。
- WHCP 认证流程:https://learn.microsoft.com/en-us/windows-hardware/design/compatibility/whcp-certification-process(五步)。
- 驱动签名政策:https://learn.microsoft.com/en-us/windows-hardware/drivers/install/kernel-mode-code-signing-policy--windows-vista-and-later-(1607 转折/cross-signed 例外三条件/按版本签要求表)。
- 签名选项:https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/driver-signing-offerings(attestation 限制清单/Server 2016+ 只认 HLK/preproduction)。
- attestation 流程:https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/attestation-signing-a-kernel-driver-for-public-release(CAB 构成/EV 签/Partner Center 提交/Microsoft 重签细节)。
- 硬件提交:https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/hardware-submission-create(五工作日/hlkx-hckx-cab 三代/认证元数据字段)。
- 硬件计划注册:https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/register-for-the-hardware-program(EV+Entra+法律联系人+三协议+问卷)。
- DCH 设计原则:https://learn.microsoft.com/en-us/windows-hardware/drivers/develop/dch-principles-best-practices(D/C/H 定义/基础包-extension-HSA 治理结构/InfVerif /k)。

### 外部一手(Linux 发行版)

- Debian derivatives(官方):https://www.debian.org/derivatives/index.en.html(定义三要素/欢迎姿态/census)。
- Ubuntu 认证目录:https://ubuntu.com/certified(500+ 测试/覆盖面/3 周 SRU 回归/12 年)。
- Canonical 认证项目文档:https://certification.canonical.com/docs/programmes/(OEM Partner Programme 商业绑定条款)。
- checkbox(开源测试框架):https://github.com/canonical/checkbox(README:认证项目御用工具/报告格式)。
- Ubuntu OEM 内核:https://documentation.ubuntu.com/kernel/reference/oem-kernels/(roll-off 机制/生命周期表/linux-oem-24.04b 命名)。
- Ubuntu HWE 内核:https://documentation.ubuntu.com/kernel/reference/hwe-kernels/(LTS only/滚动模型)。
- Launchpad PPA 文档:https://help.launchpad.net/reference/packaging/ppas/ppa(source-only/配额/owner 级 GPG key/私有 PPA 审批/保留策略)。

### 外部一手(Rust/C 生态)

- Cargo SemVer Compatibility(官方指南):https://doc.rust-lang.org/cargo/reference/semver.html(「only guidelines」/Possibly-breaking 类/0.y.z 规则/「almost every change carries risk」)。
- Cargo.toml vs Cargo.lock(官方指南):https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html(可复现构建动机)。
- unsafe-code-guidelines:https://rust-lang.github.io/unsafe-code-guidelines/(官方承认「past effort/recommendation」——布局保证无规范)。
- cargo-semver-checks:https://github.com/obi1kenobi/cargo-semver-checks(README:semver lint 工具/rustdoc JSON 不稳定反噬)。
- glibc 2.14 NEWS(一手,tag glibc-2.14):https://raw.githubusercontent.com/bminor/glibc/refs/tags/glibc-2.14/NEWS(行 298-299「Faster memcpy on x86」)。
- glibc 现役 x86_64 memmove/memcpy 实现:https://raw.githubusercontent.com/bminor/glibc/master/sysdeps/x86_64/multiarch/memmove-vec-unaligned-erms.S(:888-901 memcpy≡memmove strong_alias;文件头注释重叠规避策略 :19-47)。

### 待核实项汇总

- Debian wiki Derivatives/Guidance 页(源站 IP 封禁,本文以官方 derivatives 页 + census 机制替代);
- WHQL/WHCP 历史按次费率与免费化时点(现行文档不载);
- memcpy 事件(2011,glibc 2.14)受损软件清单与规模(二手共识,结构性一手证据已引);
- 02/03 号底稿转引的 Android CTS/GMS 细节(原始 URL 见 04 号来源索引,本文未重复采集)。

> **本稿边界**:只解剖「兼容性机制」的工程形态(ABI 编码/版本纪律/测试绑定/认证流程),不替 BCTS 立法任何开放问题;§6 建议清单每条均标注抄自哪家哪机制,采纳与否随 ADR;不覆盖 OEM 商业合同与商标运营(04 号 §7-Q5 已开)。
