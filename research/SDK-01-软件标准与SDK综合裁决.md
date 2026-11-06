# BerryOS 软件标准与 SDK 深研综合裁决（SDK-01）

> **溯源**：2026-10-29 用户问询「怎么做自己的软件开发套件 SDK——软件标准到底是什么样？什么软件、什么体系、什么架构？到底用什么语言编写」触发。
> **产制**：workflow 深研模式（run wf_6b00778c-d71，9 agent = 仓库基线×2 + 六角度对标深研×6 + 综合员×1；95.3 万 token / 18.5 分钟 / 187 次工具调用 / 0 失败 0 残缺）。
> **形态**：正文 = 综合员产出（主会话三处编辑性修正：报告计数 七→八 两处、参考文献 Apple 条目归位）；附录 A–H = 八份底稿全文归档（仅标题降一级排版）。
> **状态**：**已拍板（2026-10-29 深夜）**——§四 冲突项 6 条经用户逐项拍板，落册 ADR #55–#57（§三 对账表相应升格为裁决依据）。拍板实录：
> ① capabilities 词表：**SDK-2 前先落 fs+net 两词**（未取推荐折中，更进取）→ ADR #56
> ② 三值版本域：**v1 就进 + mkapp 编译期执法**（未取推荐 v2，更进取）→ ADR #56
> ③ verb 面：**SDK-4 内 add-only 评估 logs**（取推荐）→ ADR #54 补充
> ④ 脚本层：**不做**（取推荐）→ ADR #55
> ⑤ 链接形态：**v1 全静态 + 预留动态链接缝**（未取推荐钉死禁 .so，留缝）→ ADR #57
> ⑥ berry-libc 分发：**现在上 crates.io**（未取推荐仓内维持，更进取）→ ADR #57

## 第一部分 · 综合裁决建议（综合员产出）

## BerryOS 软件标准与 SDK 综合裁决建议（synthesis · 2026-10-29）

> 通道形态声明：本报告 = 八份基线报告（base:app / base:plan / w:android / w:oh / w:apple / w:xplat / w:sys / w:ai）综合 + 仓内定向核验（`docs/架构决策.md` ADR #37–#54 逐行、`docs/GUI里程碑规划-2026-10-v1.0.md` 裁决表与 §4 刀表、`docs/应用模型设计-v1.0.md` §2/§3/§6 词表现状）。未使用外部网络检索——外部事实以基线报告转引的一手 URL 为准（URL 见 §七），仓内事实引基线:xx 或仓内路径；综合判断标【推】。

### 一、执行摘要（13 条硬结论）

1. **「一个 BerryOS 应用」= 一份 manifest（能力申请表）+ 静态 ELF + 资源，封进 cpio 容器（.bap）**；内核零「应用」概念、appd 是唯一翻译官——GUI 应用/CLI 工具/系统服务/驱动不是四种包，是**同一格式 × type 词表值 × 能力集**的形态谱。【证】基线:app #1、`docs/架构决策.md` #43、`docs/应用模型设计-v1.0.md` §2
2. **能力词表是软件标准的第一缺口**：机制全兑现（manifest→授权集→gift，「没申请就没有 handle」），但 v0 词表恰 1 词（console）——「要 fs 的应用只能伪装 bin/ 程序走 bsh gift」是标准缺位不是实现欠账。【证】基线:app #4、`docs/应用模型设计-v1.0.md` §3
3. **.bap 演进方向 = 「小 cml」+ 内容寻址**：gift 装配从 svcmgr 硬编码迁为 manifest 声明段（能力拓扑变数据、可审计可校验），mkapp day-one 记录每文件 sha256（Merkle 全树留 v2）——Fuchsia .cml/包模型同构。【证】w:sys #1/#3
4. **三值治理（min/target/compile）有三份独立收敛证据**（Android targetSdk 行为门控 + Play 政策杠杆 + 鸿蒙 hvigor 三元组编译期报错 00303015），add-only 进 manifest v2、mkapp 编译期执法——把兼容问题从运行时提前到工具期。【证】w:android #2/#3、w:oh #5
5. **签名/版本化维持 v2「分发面出现时再立」（#44 已裁），但两条 day-one 免费动作**：manifest 预留 profile/key-lineage 字段位 + 每文件摘要——签名方案上来就该全文件覆盖 + 轮换预留（Android v1→v4 全是事故驱动）。【证】w:android #9、base:plan #9
6. **SDK = 六层塔**（语言运行时→系统库→服务协议 crate→框架→工具链→DFX/AI 面），层间两条纪律已在册：依赖方向铁律（#43）+ **中间不设稳定 ABI**（冻结面仅 `berry_*` syscall + wire 协议 add-only）——静态链接自兜底是 AndroidX 版本纠缠与 RN 版本地狱的结构解。【证】基线:app §3、w:android #4、w:xplat #5/#7
7. **全静态单映像 v1 定死**：musl 无符号版本先例 + Android linker namespace 之痛反证「混合态最痛」；建议 ADR 钉死「禁 .so、开动态链接须整案裁决」。【证】w:sys #9/#10
8. **Rust 单语言要「明文非目标化」多语言绑定**：Fuchsia 教训 = 支持一门语言是 FIDL 后端+工具链+文档+DevRel 四线持续投资，「Rust is not supported for end-developers」是倒果为因的循环。【证】w:sys #4/#5
9. **berryui 文本 DSL = 数据不是第二语言**（C2 已裁 AST 导出 day-one），外部三重佐证：ArkTS 为编译期定布局而收敛 TS、TypeChat schema 约束「生成→校验→回喂修复」闭环、Slint DSL 数据级热重载（SDK-2 精确同档）。【证】w:oh #3、w:ai #6、w:xplat #11
10. **脚本层 v1 建议 = 不做语言，只做「数据 + 宿主动词」**：AI 生成物 = 受 schema 约束的 AST/JSON（Design2Code 证明截图→代码仍有实质差距；slopsquatting 证明生成物必须过白名单+版本锁）；一次性工具走 Rust bin（uutils 先例）。【证】w:ai #6/#8；裁决建议本身【推】
11. **C/C++ 第三方库准入三条**：许可洁净（GPL 零进入，#37）+ 有 feature 门可关或可隔离在单个服务进程内 + Rust 生态确无等价物——onig（无门可关的硬依赖、与 berryui 自绘互斥）= 反面执法样本在册。【证】基线:app #8、CLAUDE.md C-2c 挂账册、w:android #7
12. **SDK-4 四 verb 定形三原则**：dump = 语义树带稳定 ref（Playwright「snapshot 优于 screenshot」是生态事实共识）+ 两层错误模型（协议错 / 执行错必带可操作建议）+ 校验器错误按 LLM 可修设计（位置+期望+修法三元组）。【证】w:ai #1/#3/#4
13. **SDK 底座今天已实物化**（三件套 + 协议 crate 范式 + mkapp 五道门禁 + hexyl/uutils「上游未改动编译即跑」先例），缺的是**开发者面收口**：out-of-tree 结构性障碍（path 依赖 + 编译咒语只活在 justfile 注释）、无脚手架、无开发者文档路径、README 缺 musl target——四件全部 SDK-1 一刀可收。【证】基线:app #7/#9/#11/#12

### 二、用户四问的结构化回答

#### Q1 我们的软件标准到底是什么样？

**定义立法（建议原文落 `docs/应用模型设计-v1.0.md` §0）**：凡由 appd 依据 manifest 翻译出 handle 授权集、以静态 ELF 运行于 EL0 的进程 = BerryOS 应用；系统服务与驱动是同一格式的持证形态；CLI 工具是无 manifest 的裸进程，权限由调用者（bsh gift）即时授予。**最低线** = 确定性构建（mkapp 同输入同字节）+ 每文件 sha256 + manifest 机器可读；签名 v2 全文件方案 + lineage 预留。

| 形态 | type（v0→v2） | 打包 | 能力集 | 生命周期 / 失败隔离 |
|---|---|---|---|---|
| GUI 应用 | `"app"`→`"gui"` | .bap（views/ 资源位，裁决 F） | display/input + 声明子集 | appd 拉起；崩溃 = 内核 kill + 遥测，不拖系统 |
| CLI 工具 | 无（`bin/` 直进 rootfs） | 不打包（v1） | 零能力 + 调用者 gift | 短命进程，exit 真码即终 |
| 系统服务 | `"daemon"`（v2） | v1 烙 rootfs；v2 可 .bap | boot 直插/协议端点 | 常驻 + 私有 handle 表 + wait 帽重臂（P6 A-3 纪律） |
| 驱动 | `"driver"`（v2 随 devmgr，#46） | .bap + fw/ 固件随包（#47 L3） | 硬件类能力（MMIO/IRQ） | 持证应用，devmgr 与 appd 同构（翻译官只换授能对象） |

演进主线 = v0 七键+console → GUI-7 落 icon/label/views（已裁未落码，基线:plan #5）→ v2 词表 `gui|headless|daemon` × `display/input/fs/net/background` + 三值版本域 + deviceTypes（三端同族可直接简化复用，w:oh 启示 1）+ `provides` 语义槽。**分发/更新最低线已定**：应用更新 = L2 appd 换树 rename 原子（#47）；系统侧走整映像 A/B，系统包管理器的立项判据 = 「出现第三方交付的系统组件」（w:sys #11【推】判据）。

#### Q2 到底是什么软件、什么样的体系？

谱系四层一族：**内核（EL1）/ L1-L2 用户态服务 / L3 应用 / 协议 crate（组件非进程）**。既有三件的织法：

- **appd 翻译官**是体系枢纽——manifest 是唯一授权入口，安装=翻译、启动=gift 授能；监督面现状 = 一次性 collect→reap 无重启（基线:app #1），补法不是引入新机制而是把「常驻服务纪律」（wait 帽重臂、私有表、EBUSY 分流）反哺给 appd，增删/崩溃事件随 GUI-7 落地。
- **协议 crate 家族**（fsproto/blkproto/netdproto/psproto/guiproto）是 BerryOS 的「人无我有」资产：每服务一个纯函数 crate、host 可测、wire 协议 add-only——跨实现共识测试纪律（fixture 双端对拍）就是体系的质量地基。
- **失败隔离分级**：内核 kill 域（fault/terminate）、服务域（坏 client 不杀服务）、应用域（crash 不拖桌面）——三层都已有一手执法案例（user_fault_exit、ST_BAD 分流、APP ok 真码），缺的只是把它们写成「监督模型」一节文档。【推】

#### Q3 什么样的架构（SDK 分层）？

| 层 | 现役件 | 稳定面声明 | 版本治理 |
|---|---|---|---|
| L0 语言运行时 | Rust stable + `aarch64-unknown-linux-musl` 全静态（免 nightly） | 冻结 | rust-toolchain 钉版挂账维持 |
| L1 系统库 | berry-sys（raw、永不 panic）/ berry-libc（musl 口径、152 符号）/ std | `berry_*` syscall = 冻结面（abi.toml 单源三面 + 逐字节守卫） | ABI 演化叙事补 added/deprecated 域（w:sys 启示 2） |
| L2 服务协议 crate | fsproto 家族 | wire 协议 add-only（#43） | 严格串行 RPC + host 测钉板 |
| L3 框架 | berry-gfx（pass 形态）/ berryui（DSL+AST） | **不稳定**——静态链接随应用 | 语义版本、命名不绑内核/ABI 序号（AndroidX 立法） |
| L4 工具链 | mkapp/mkcpio/elf2flat/berry-ld/just/berryctl | mkapp 门禁 = 子集冻结执法点 | 声明「应用开发者碰 / 系统内部」二分 |
| L5 DFX/AI 面 | SDK-1..5（dump/inject/screenshot/reload + 遥测） | dump 格式 = 三消费者共用 ABI、round-trip 可注入 | DSL 语法版本域每刀冻结 |

三条横切纪律：①依赖方向铁律（清单外不依赖清单内，#43）；②静态链接 + 全静态单映像（冲突项 5 拍板后升 ADR）；③**稳定性靠执法工具不靠文档承诺**（abi-check/mkapp 门禁/smoke 断言 = Apple「静态分析拒私有 API」同构，w:apple #5）——建议可观测性文档补「承诺-执法对照表」。

#### Q4 到底用什么语言编写？

**Rust 全线，边界与例外恰好四条**：①syscall 边界 C-ABI + `#[repr(C)]`（ABI 稳定性工程需要，非语言例外）；②berryui 文本 DSL = 数据（AST 是序列化描述结构，不是图灵完备语言——C2 硬要求「声明即数据」）；③宿主面 Python（tools/*.py 构建与验证脚本）不算目标机语言；④C/C++ 第三方库按准入三条进（见硬结论 11），且必须落在单个服务进程内隔离。**脚本层不做**（冲突项 4）：给 AI 的「语言」就是 schema 约束的 AST + 四 verb；给测试注入的就是 FIFO + dump 对拍。**语言生态的 AI 面证据**：AGENTS.md/llms.txt 已成事实标准、MCP 工具双 Schema 即 API 文档（w:ai #3/#5）——BerryOS 的 abi-gen 增第四面（llms.txt 式入口 + JSON Schema 导出）是对号入座动作。

### 三、与既有裁决对账表

| 既有裁决 | 对账 | 说明 |
|---|---|---|
| SDK-1..5（ADR #54） | **确认 + 补充** | SDK-1 刀内加四件免费动作（见 §五）；SDK-4 verb 面按 add-only 评估 logs verb |
| C2（文本 DSL + AST 导出 day-one） | **确认 + 补充** | AST 中立性约束（信号核/键路径两案可互换）应补进 #52 注，防 v1.5 对拍结论出来时 AST 已偏（base:plan 未定项 2） |
| 裁决 F（icon/label/views） | **确认** | 已裁未落码（基线:plan #5），随 GUI-7；负样本「旧包无新键必须过」同刀 |
| ADR #37 Apache-2.0 | **确认** | C 库准入第一条的执法依据 |
| ADR #39–#41（移动优先/MCU 排除/x86 盒） | **确认** | SDK 面不受扰；deviceTypes 三端同族简化复用 |
| ADR #43（三层切分/中间不设稳定 ABI） | **确认 + 补充** | 「版本空间解耦」立法成文（库版本不绑平台版本，AndroidX 教训） |
| ADR #44（cpio+TOML、签名 v2 加法） | **确认 + 补充** | day-one sha256 + manifest 预留 lineage 字段位 |
| ADR #46（驱动=持证应用） | **确认** | type=driver 词表随 devmgr v2，不提前 |
| ADR #47（更新四层） | **确认 + 补充** | 系统包层立项判据 = 第三方系统组件交付出现 |
| ADR #50–#53（B 形态/pass/低功耗） | **确认** | SDK-2 解释器只消费 berry-gfx pass API、无绕过通道（NDK 教训转译）；guiproto op 集合冻结锚点 = GUI-2/4 先到先定（base:plan 未定项 3 照记） |
| 全 Rust | **确认 + 补充** | 多语言绑定明文列非目标；C 库准入三条成文 |
| musl 口径 | **确认 + 补充** | 全静态单映像升独立 ADR（冲突项 5 关联） |
| 不跑 Linux 二进制 | **确认** | starnix 类层 = v2+ 选项非欠债（w:sys #6） |
| v1 无 fork | **确认** | 无外部对照推翻 |

### 四、冲突项单列（供拍板，不改既有裁决）

1. **capabilities 词表扩容触发器**：「随服务就位 add-only」（既有）vs「SDK-2 前先落 fs/net 两词」（base:app 启示 3）。**推荐**：折中——fs 词随 GUI-7 的 FAT 递送面评估同刀落（fsmgr 现役 + bsh fs gift 四元装配先例可直接复刻进 appd 翻译面），net 词等 SDK-4 出网裁决。理由：词表 = 服务的镜子，声明了拿不到 handle 即空头支票，破坏「没申请就没有」的模型纯度。
2. **三值版本域进 v1 还是 v2**：**推荐** v2 维持（#44 add-only），SDK-1 仅做 mkapp 词法解析不执法。理由：v1 无分发面、无旧包兼容压力，提前执法 = 提前背负版本矩阵。
3. **dump verb 面扩容（logs/batch，w:ai 启示 2）vs 四 verb 冻结**：**推荐** SDK-4 刀内 add-only 评估、logs 优先（内核遥测环现役）、batch 缓。理由：每加一 verb = 协议面+校验器+文档三线成本；断言留在客户端对拍侧不引入服务端断言语义（防双真相源）。
4. **脚本层做不做**：无脚本（数据+宿主动词）vs 受限表达式语言。**推荐**无脚本。理由：berryui DSL 已承担「描述语言」位；脚本 = 第二运行时 + 第二安全面 + AI 幻觉新攻击面（slopsquatting）；一次性工具 Rust bin 化成本低（uutils 先例）。
5. **全静态 vs 预留动态链接缝**：**推荐** v1 ADR 钉死全静态（禁 .so），重开证据 = v2 出现真实共享运行时需求且内存账成立。理由：musl 先例 + Android namespace 之痛；「预留缝」本身即协议烧死假设类决策（#53 反面清单同款）。
6. **berry-libc 上 crates.io（去 path 依赖）vs 仓内维持**：**推荐**仓内维持至 H3 对外生态时点，SDK-1 以脚手架+咒语文档化解决可达性。理由：crates.io 发布是对外承诺面（版本治理/弃用纪律/文档基建先行）——Fuchsia SDK 分级与「不承接第三方信任链」的教训。

### 五、SDK 线刀序建议（排进现行路线）

| 刀 | 内容 | 与 GUI/M-ECO 关系 | 验收线（host 测先行 → 接线 → smoke） |
|---|---|---|---|
| **SDK-1 = GUI-9 并轨（已裁）+ 四件免费动作** | mkapp `--new`/`--check` 子命令；justfile:69-107 咒语提炼为 `docs/应用开发指南.md`；README/CLAUDE.md 前置补 musl target；argv0=entry/EOF 先行/退出码隐式契约升格「应用侧契约清单」 | 随 GUI-9（依赖 GUI-8） | mkapp selftest 扩样本（旧包无新键必过）+ `docs/索引.md` 增第三条「应用开发者路径」+ 脚手架产出的 demo .bap 过 `APP ok` |
| **SDK-2 前置 ADR 微刀** | 热重载状态保持矩阵（保留/复位/重建三态逐类枚举）+ AST 中立性补注 #52 + manifest v2 词表与 capabilities 扩键合并一次 ADR | 纯文档刀，任意时点（建议 GUI-6 期间） | ADR 落册 + berryui AST 两案互换 host 对拍测试设计稿 |
| **SDK-2**（berryui DSL + 解释器 + FAT 热重载） | 边界单页成文（Slint 三语义：属性/回调/模型保留、语法错误旧版留屏、动宿主接口需重编）；解释器失败面 = 机器可读错误码 + last-known-good 回退遥测 | 裁决 H：自 SDK-2 起与 GUI 线文件面互斥可并行（建议与 GUI-5/8 并行、避 GUI-6 的 berryui 共享面） | DSL 解析/校验 host 测（协议 crate 模式）→ 设备侧解释器标记（如 `BUI ok`）→ 热重载相：坏 DSL 拒载保旧树 + reload 标记 |
| **SDK-3**（帧时间线 + p50/p90 机器可读） | 与 GUI-9 增件（四桶归因/稳态零唤醒）同源建设：统一事件 schema（begin/end + 稳定字段名，os_signpost 同构），判定结果输出结构化 JSON | 随 GUI-9 增件同步 | host：事件编码 round-trip；smoke：帧时间线 JSON 可收割 + 能耗断言 |
| **SDK-4**（AI 生成闭环 + 四 verb） | dump=语义树带 ref + 两层错误 + logs verb 评估；校验器错误三元组；mkapp 拒未注册组件（slopsquatting 防线） | GUI 线收官后 | 协议 crate host 测 + 四 verb 经串口/TCP 双通道 smoke 标记 + 校验器负样本集 |
| **SDK-5** | 维持「等 v2 动态加载」 | — | 不动 |

工具纪律：凡涉 smoke.sh 归主线串行（总规划 §4）；SDK 线文件面互斥时才开副会话。

### 六、开放问题与 spike 清单（带 kill criteria）

1. **热重载状态保持语义**（SDK-2 前置 ADR）：若三态矩阵写不出（状态类别无法枚举干净），DSL 热更降级为整树重建（Model 复位）——降级而非阻塞。
2. **AST 中立性 spike**：信号核/键路径各写最小 AST 变换 host 对拍；kill = 若任一案需 AST 面破坏性字段，则 v1.5 原型对拍提前至 SDK-2 内完成。
3. **guiproto op 集合冻结锚点**（GUI-2/4 先到先定，base:plan 未定项 3）：kill = GUI-4 接线时 op 集合仍未定则强制冻结 v0 子集。
4. **FAT 盘 app/ 扫描递送面**（GUI-7 评估）：spike = appd 增删事件 + FAT 走查成本实测；kill = 走查超 boot 帽或槽位帽冲突 → 退 rootfs-only 递送。
5. **CTF 式兼容执法**（旧 SDK 产物二进制 fixture 进 smoke）：kill = 若 v1 全静态+单仓使「旧二进制」版本漂移结构性不可达，则记纪律不建基建。
6. **出网通路**（socket ENOSYS/TLS 无议程）：SDK-4 若需连远程模型即被卡；kill = 本地 schema 校验闭环验证足够则不做出网（推荐先证本地闭环）。
7. **响应式机制两案对拍**（既有 v1.5 裁决维持）：不新增 spike，仅以开放问题 2 的中立性约束兜底。

### 七、参考文献（去重分组）

**仓内（本报告定向核验）**：`docs/架构决策.md`（ADR #37–#54）、`docs/GUI里程碑规划-2026-10-v1.0.md`（裁决 F/H/I + §4 刀表）、`docs/应用模型设计-v1.0.md`（§2/§3/§6）、`docs/更新与OTA设计-v1.0.md`；基线报告八份（base:app / base:plan / w:android / w:oh / w:apple / w:xplat / w:sys / w:ai——全文由主会话拼接为附录）。

**Android 域**：developer.android.com（uses-sdk-element / app-bundle / app-signing / ndk/guides / java8-support）、source.android.com（core/runtime、setup/build/java-library）、support.google.com/googleplay/android-developer/answer/11926878、android-developers.googleblog.com（Kotlin-first、Live Edit）、developer.android.com/jetpack/androidx/migrate、EFF/CCA（Oracle v. Google）。

**鸿蒙/OH 域**：developer.huawei.com（Stage 模型、签名配置、apidiff-510、ArkTS 适配规则）、developer.harmonyos.com（SysCap/hvigor 错误码/musl NDK 差异，RAG 一手）、本机 openharmony 十仓（applications_launcher module.json5、ace_engine README）。

**Apple 域**：swift.org/blog/abi-stability-and-more、developer.apple.com（MetricKit、公证、弱链接存档、WWDC19-703）、forums.swift.org（Xtool、弱链接）、eclecticlight.co（deprecation）。

**跨平台域**：docs.flutter.dev（technical-overview、hot-reload）、pub.dev（pigeon、scoring）、reactnative.dev（new-architecture、fast-refresh）、kotlinlang.org（CMP/JC 边界）、docs.slint.dev（live-preview）、book.iced.rs/architecture、github.com/linebender/xilem、news.ycombinator.com/item?id=31195471。

**微内核/系统域**：fuchsia.dev（components v2、FIDL versioning、packages、starnix、programming_languages、RFC-0107/0216、CTF）、docs.sel4.systems（microkit、capDL）、genode.org、qnx.com（APS、OS for Safety）、openwall.com/lists/musl、android.googlesource.com（bionic NDK changes）、arstechnica.com（Fuchsia 裁员）。

**AI 域**：github.com/microsoft/playwright-mcp、modelcontextprotocol.io/docs/concepts/tools、anthropic.com/research/writing-tools-for-agents、agents.md、llmstxt.org、arxiv.org/abs/2403.03163（Design2Code）、arxiv.org/abs/2505.05057（API 幻觉）、microsoft.github.io/TypeChat、endorlabs.com（slopsquatting）、playwright.dev（aria snapshots）。

---

## 第二部分 · 附录（八份底稿全文）

### 附录 A · 基线盘点：应用模型/系统接口/工具链（base:app）

## base-app 视角报告：BerryOS「今天的 SDK 事实上长什么样」——应用模型/系统接口/工具链盘点

> 通道形态声明：本报告为**纯仓内盘点**（文件阅读 + 只读命令），未使用外部网络检索；全部结论可由下列仓内路径复核，无外部信源依赖。

### 一、硬结论清单

1. **.bap v0 是纯用户态协议约定，内核零「应用」概念**——svcmgr 兼任 appd 雏形，boot 期收集槽 ≤4、走查毕统一拉起、一次性 collect→reap，**无监督重启**。【证】`user/bins/svcmgr/src/main.rs:242-261`（appd 注释 + APP_SLOTS）、`:2297-2406`（app_launch 无重试/重启循环）。
2. **manifest v0 白名单恰 7 个语义位**（id/version/entry/type/intent + console + stack_pages）；「扩 icon/label/views」只是 GUI 里程碑裁决 F 的**规划**（GUI-7 刀），解析器与 mkapp 均未实现——当前背景描述「已裁扩」实为「已裁决未落地」。【证】`user/berry-manifest/src/lib.rs:370-401`（键白名单 match）、`docs/GUI里程碑规划-2026-10-v1.0.md:75,91`。
3. **mkapp 五道门禁 + 规范形逐字节重序列化**是 TOML 子集冻结的唯一 build 期执法点，9 负样本 + 2 正对照自检挂 `just test-host` 前置。【证】`tools/mkapp.py:13-27,242-287`、`justfile:451-453`。
4. **能力到达 = env 块 gift 数组（handle 随 channel 消息迁移）**：安装期翻译 manifest→授权集、启动期 spawn 授能，「没申请就没有 handle」的能力模型已兑现——但 v0 词表只有 `console` 一个词。【证】`user/berry-exec/src/lib.rs:407-484`、`svcmgr main.rs:2337-2347`、`docs/应用模型设计-v1.0.md:123,205`。
5. **ABI 常量层已单源生成三面 + 逐字节守卫**：v1 在册 31 号全实现、17 号 v2 预留（实测解析）；原型层（berry_* 签名）仍手写双镜像挂 v2。【证】`tools/abi.toml [meta]`、`tools/gen_abi.py` 头注、`justfile:264`（build: abi-check build-user）。
6. **berry-libc 是「零 musl 字节」全定义 shim**：152 个 `#[no_mangle]` 符号；malloc 静态 256KiB 堆、futex=SYS_futex(98) 拦截落 Event 池、pthread_create 真身（T-1）；fork/exec/socket 全 ENOSYS 桩（v1 无 fork 铁律的 libc 侧执法）。【证】`user/berry-libc/src/lib.rs:1-24` 头注、各文件 no_mangle 计数、`futex.rs:1-20`。
7. **第三方 std 程序编译路径已存在且免 nightly**（musl target + berry-ld 剥 crt/剥 -lc + [workspace] 自绝小工作区），但这条咒语**只活在 justfile 内嵌注释里，零开发者文档**。【证】`justfile:69-107`、`tools/berry-ld`（shell 脚本头注）、`user/bins/appdemo/Cargo.toml`。
8. **uutils 23 命令家族工作区 + rustix use-libc 翻转**（防 Linux raw svc 号对撞）是「移植未修改第三方 Rust 程序」的成熟先例；getrandom 0.2.17/0.4.3 双版本共存于 `user/bins/uu/Cargo.lock:667,678`。【证】`user/bins/uu/Cargo.toml` 头注与 workspace.dependencies。
9. **rootfs 组装 = justfile 硬编码条目表调 mkcpio**——新程序进系统今天必须改 justfile（加 build 行 + mkapp/mkcpio 条目），**无任何 out-of-tree 机制**；.bap 也只是 mkcpio 的一个条目。【证】`justfile:197-240`。
10. **交互面与 app 面是两个世界**：bsh `run`/管道可交互拉起 `bin/<name>`（berry-exec SpawnSpec + fs gift 四元装配），但 .bap 应用只能 boot 批拉起，bsh 无 app 启动命令。【证】`user/bins/bsh/src/main.rs:100,628-641`、`svcmgr main.rs:923-947`。
11. **文档全是系统视角**：42 份 docs 的两条阅读路径（AI/人类）无一条「应用开发者路径」；事实上的应用开发资料 = app-0 草案 + 应用模型设计 + ABI 文档 §8 + appdemo 源码。【证】`docs/索引.md:93-100`、`docs/app-0实施设计草案-v1.0.md`。
12. **README 前置缺 musl target**：quickstart 只让装 none/x86_64-none 两个 target，而 `build-user` 硬依赖 `aarch64-unknown-linux-musl`——照抄 README 跑 `just run-rootfs` 会挂在 rustup target 缺失。【证】`README.md:42` vs `justfile:76`。

### 二、正文论证

#### 1. .bap v0 全链

解析器 `berry-manifest` 是「解析器即产品」家族第 8 例：no_std、零分配、零依赖，TOML 子集冻结到字节级（LF-only、拒 tab、拒反斜杠、纯十进制整数、可打印 ASCII 串），严格拒面覆盖未知节/键、重复、缺必填、越界共 20+ 类，host 23 测。打包器 `mkapp.py` 复用 mkcpio 单编码器，五道门禁中最关键的是**规范形重序列化逐字节相等**——tomllib 收全集而门禁只收子集，这是「宽松解析器 + 严格打包器」分层的执法点。消费端 appd（svcmgr 内 ~200 行）：主走查收集 `app/<ID>.bap` 条目几何（≤4 槽，超帽 `eco: app too many`）→ 走查毕逐槽 bap_scan（内层 cpio 恰两条目形状守卫）→ manifest parse → **双守卫**（manifest.id↔文件名、entry↔包内条目）→ console=true 直连串口/false 走私有采集对 → spawn → collect EOF → reap 真码 0 才计数，`APP ok n=` 是 smoke 断言位（`tools/smoke.sh:565`）。**生命周期 v0 无监督重启、无增删事件**——GUI-7（`docs/GUI里程碑规划-2026-10-v1.0.md:91`）已把它列为下一刀。demo 双 app 的 manifest 实貌即 schema 教科书：APPDEMO（console=true、省 runtime）与 APPQUIET（console=false、显式 stack_pages=8）互为能力授予/缺席两半活证。spawn 链统一走 `berry-exec::SpawnSpec`（stdout/stdin/args/extra_gifts/mmio_vmo 十字段），argv0=entry 串是隐式契约（appdemo 以断言自检，错值 abort(134)→`APP ok` 缺失 loud）。

#### 2. 系统接口面

`abi.toml` 单源生成内核/berry-sys/文档三面，`--check` 逐字节守卫挂 build/check/smoke 前置——常量层（号/位/码/宽度）漂移按构造不可能。v1 31 号（handle/vmo/vmar/process/thread/channel/event/clock 全家）全实现；`namespace_open`/`futex_wait` 等 17 号 v2 预留。`berry-sys` 是纯 raw 层（绝不 panic/分配，SVC #0 内联汇编封 `sys()`），编号私有、用户不写裸号。`berry-libc` 按 musl aarch64 口径全定义，C-ABI 布局纪律有编译期钉板先例（`Stat` 全 20 字段 `offset_of!` 钉板，C-2a 立法）。std 程序跑法 = 独立小工作区编 musl target，栈页按程序面参数化（hexyl 32 页 = musl 128KiB 惯例）+ 映像/栈间保护页。uutils 薄 harness 每成员只做两件事：CRT0 锚（`#[used] static CRT0_SEED`）+ uumain 直传；家族教训已成文（`bin_inner!` 漏 setup_localization、rustix 必须翻 use-libc）。

#### 3. 工具链：一锅粥还是分层？

事实分层已存在但**未声明**。应用开发者必须碰的：`mkapp.py`（打包+自检）、`mkcpio.py`（进 rootfs）、`elf2flat.py`（仅 none-target 平像程序）、`berry-ld`（链接咒语）、manifest 手写。系统内部不该碰的：mkfat/mkimage（盘格式）、elf2pe（EFI 转换）、gen_abi（ABI 再生）、size/bench/smoke/soak（验证面）、genfbfont/tlsscan/pwrcls（专用）。问题在于这层分界只存在于维护者脑中——全部工具平铺在 `tools/` 无分类，justfile 的 build-user 是 240 行单 recipe 把两种 target、五个工作区、平像/ELF 两条产物链炒在一起。SDK-1 的「.bap 脚手架」裁决（ADR #54）正是补这层的切入口。

#### 4. 语言面事实

user/ 工作区 19 个 berry-* crate **全部 no_std**（逐一验证 lib.rs），其中 11 个为协议/纯函数 crate（fsproto/blkproto/netdproto/psproto/loader/fatfs/netproto/tcp/manifest/complete/tls/fbcon/gfx——host 测试即产品）；运行 crate 三件（sys/exec/libc）。std 面程序全部以「独立小工作区自绝」形态存在（stdhello/hexyl/bsh/fsmgr/appdemo/appquiet/uu 家族/drivers bins），靠 `[workspace]` 空表脱离 user/ members、经 path 依赖引 libc——**path 依赖 = 必须在仓内**，这是 out-of-tree 的结构性障碍。第三方依赖现状：uu_* =0.12.0 锁步、hexyl 0.17 逐字、rustix 1（use-libc）、getrandom 双版本共存，全 MIT 生态，协议洁净。

#### 5. 能力/权限到达路径：设计 vs 实现

设计的完整链（`docs/应用模型设计-v1.0.md:123`）：manifest 能力申请表 → appd 翻译成 handle 授权集 → spawn 经 gift 授能。实现兑现了**机制**（console=true→console 发端 dup 交付；false→私有采集对，appquiet 静默活证）但词表冻结在 1 个词：fs/net/gpu/bg 全无。boot 直插 handle（H_ROOTFS/H_MMIO/H_FB…）与 extra_gifts（blk 端/fs 端/fbinfo）是服务间授能的现役通道，但**只服务系统内置拓扑**，应用声明位够不到。intent 字段已解析进 Manifest（六档词表镜像 sched/intent.rs 判别值钉板）但调度零消费（ADR #42 两段式，v2 EEVDF 消费）。

#### 6. 复盘：第三方开发者今天的真实路径与缺口分级

今天的真实路径（全部需要 fork 主仓）：clone 公开镜像 → 抄 `user/bins/appdemo`（唯一活例）→ 自建 `[workspace]` 小工作区 + `berry-libc` path 依赖 + release 五连 profile + CRT0 锚 → 用 justfile:76 的咒语编译（musl target + berry-ld RUSTFLAGS + `-zmax-page-size=4096`）→ 组 `{manifest.toml, bin/}` 目录 → `python3 tools/mkapp.py <ID>.bap <dir>`（需 Python≥3.11）→ **改 justfile** 加 build/mkapp/mkcpio 三处接线 → `just build-user && just run-rootfs` 看串口。调试 = 串口标记 + `eco: app <id> <理由>` 降级行 + smoke 断言；无 gdb 用户态面、panic=abort+strip 无回溯。文档 = app-0 草案 §2/§4 + ABI 文档 §8 + 应用模型设计 §3。

**缺口分级**：
- **阻塞生态**：① 无 out-of-tree SDK——path 依赖 + 编译咒语无文档 + 无脚手架，离开主仓无法构建；② 无运行时安装通道——.bap 只能 build 期烙进 rootfs.cpio，FAT 盘上放 .bap 无人扫；③ 能力词表只有 console——要 fs/net 的应用无法合规声明，只能伪装成 bin/ 程序走 bsh gift；④ APP_SLOTS=4 boot 帽 + boot 批一次性拉起，无枚举/增删面。
- **影响体验**：⑤ README 前置缺 `rustup target add aarch64-unknown-linux-musl`（照抄必挂）；⑥ mkapp 规范形要求（多一个空格即拒）对新手是隐形雷，无 linter/提示型工具；⑦ argv0=entry、EOF 先行、退出码契约等隐式 ABI 散在代码注释；⑧ .bap 应用无法从 bsh 启动（两世界割裂）；⑨ 无「第一个应用」教程，42 份 docs 无开发者阅读路径。
- **锦上添花**：⑩ `just new-app <id>` 脚手架；⑪ berryctl app 子命令/枚举面（GUI-7 已裁）；⑫ libc/sys 上 crates.io（去 path 依赖）；⑬ per-app 日志/crash report 面。

### 三、对 BerryOS 的可迁移启示

1. **SDK-1 的 .bap 脚手架刀顺带完成「咒语文档化 + 工具分层声明」**：提炼 justfile:69-107 为 `docs/应用开发指南.md`，`tools/` 按「应用开发者碰/系统内部」二分目录或在索引文档声明。挂点：docs/新文档 + tools/ + SDK-1 刀。
2. **mkapp 加 `--new <id>`（脚手架）与 `--check <path>`（对既有 .bap 复检）子命令即成 SDK 首件**——门禁五道已是校验器内核，增量极小。挂点：tools/mkapp.py。
3. **SDK-2 前落 fs/net 两个能力词**：appd 翻译面可直接复用 bsh 的 fs gift 四元装配运行时先例（svcmgr run_capture_fs）。挂点：berry-manifest 白名单 + mkapp 镜像 + svcmgr app_launch。
4. **appd 枚举/增删事件（GUI-7）评估「FAT 盘 app/ 扫描」作递送面**，绕开 rootfs rebuild 即破缺口②④。挂点：svcmgr appd + fsmgr。
5. **docs/索引.md 两条阅读路径外补第三条「应用开发者路径」**（app-0 草案→ABI §8→appdemo→mkapp→justfile 接线），一行索引闭环。挂点：docs/索引.md:93。
6. **README/CLAUDE.md 前置补 musl target**（quickstart 一致性 bug）。挂点：README.md:42、README.zh-CN.md:33、CLAUDE.md:152。
7. **把 argv0=entry/EOF/退出码等隐式契约升格为「应用侧契约清单」**进应用模型文档——SDK-4 AI 生成 app 时代价最低的防错投资。挂点：docs/应用模型设计-v1.0.md。

### 四、主要来源（仓内路径，去重）

- /Users/murongcheng/Documents/code/berryos/user/berry-manifest/src/lib.rs
- /Users/murongcheng/Documents/code/berryos/tools/mkapp.py、tools/mkcpio.py、tools/abi.toml、tools/gen_abi.py、tools/berry-ld、tools/smoke.sh
- /Users/murongcheng/Documents/code/berryos/user/bins/svcmgr/src/main.rs（:242-261、:880-895、:923-947、:2297-2406 appd 全链）
- /Users/murongcheng/Documents/code/berryos/user/berry-exec/src/lib.rs（:193-231 SpawnSpec、:407-484 gift 块）
- /Users/murongcheng/Documents/code/berryos/user/berry-libc/src/lib.rs、futex.rs
- /Users/murongcheng/Documents/code/berryos/user/berry-sys/src/lib.rs
- /Users/murongcheng/Documents/code/berryos/user/bins/appdemo/{manifest.toml,Cargo.toml,src/main.rs}、user/bins/appquiet/manifest.toml、user/bins/stdhello/Cargo.toml、user/bins/uu/{Cargo.toml,Cargo.lock}、user/bins/bsh/src/main.rs
- /Users/murongcheng/Documents/code/berryos/user/Cargo.toml（members/exclude 谱系）
- /Users/murongcheng/Documents/code/berryos/justfile（:69-107 std 面、:197-240 mkcpio、:264、:402 test-host、:451-453）
- /Users/murongcheng/Documents/code/berryos/docs/{应用模型设计.md、GUI里程碑规划-2026-10.md、索引.md、开发体验.md、app-0实施设计草案.md}
- /Users/murongcheng/Documents/code/berryos/README.md

---

### 附录 B · 基线盘点：既有规划与裁决对账（base:plan）

## 基线盘点：BerryOS 软件生态/SDK 规划——已定/留缝/未定（base-plan 角度）

> 通道形态：纯仓内文档盘点（12 份文档全文或定向阅读 + 3 轮引文核验 grep），未使用 WebSearch/WebFetch——本任务证据全部在仓内，引文均为一手可查【证】；推断标【推】；未核到处如实标注。

### 一、硬结论清单

1. **SDK 演进序已整体锁定为 SDK-1..5（ADR #54，2026-10-29 落册）**——从 GUI-8/9 随刀的 dump round-trip 注入起，到 SDK-5 设计器止，五段全有名有验收物。【证】`docs/架构决策.md` #54 行
2. **SDK-2 的 DSL 载体已提前拍板（C2）**：文本 DSL 为主、Rust 宏仅类型安全糖层，硬要求「声明即数据」AST 导出 day-one——原计划「SDK-1 后用真实数据裁」的悬案已收口。【证】`docs/GUI里程碑规划-2026-10-v1.0.md` 修订记录（2026-10-29 晚）
3. **SDK 三件套实物已在仓**：`berry-sys` + `berry-libc`（musl shim）+ 协议 crate 家族（fsproto/blkproto/netproto 模式），hexyl 0.17.0 被文档明文称为「SDK 的第一个活证据」。【证】`docs/应用模型设计-v1.0.md` :64/:125
4. **中间层不设稳定 ABI（ADR #43）**：冻结面仅 `berry_*` syscall + 各服务 wire 协议 add-only，工具包静态链接随应用——SDK 兼容策略由此定形，不存在「针对旧 SDK 编的二进制跑在新系统」承诺。【证】`docs/架构决策.md` #43
5. **manifest 已裁扩 `icon`/`label`/`views/`（裁决 F，add-only）但代码未落**：`user/berry-manifest/src/lib.rs` grep 无此三字段——裁决与实现存在刀序时差（挂 GUI-7）。【证】裁决 F 行 + 2026-10-29 grep 核验
6. **存储是 SDK 分发/落盘面的隐性结构阻塞**：四棵树「v1 还摆不了」——fsmgr RAM 层平名线性表 FILE_COUNT_MAX=9、FAT 仅根目录 16 项，层级待 v2 S3。【证】`docs/存储布局与沙箱设计-v1.0.md` :163
7. **生态正面战（SDK 对外/分发/文档）排在 H3 v3+**，但 SDK-1..3 已内嵌进 M-GUI 刀序、SDK-2 起独立成刀可与 GUI 线并行——对内 SDK 先行、对外 SDK 远期，双轨分明。【证】`docs/战略路线总规划-2026-10-v1.0.md` §3 + 规划 §4 并行性注
8. **AI 生成 UI 产物 = 数据文件非 Rust 代码**；AI 工程面 v1 零代码、红线 =「本文不新增任何『只有 AI 能调』的接口」。【证】裁决 F 行 + `docs/AI工程面设计-v1.0.md` §5
9. **签名/版本化现状为无**，文档明文「v2 ADR（分发面出现时再立）」；pthread/线程库封装仍缺（0x35 thread_start 已落）。【证】`docs/应用模型设计-v1.0.md` 诚实缺口表 :214/:219
10. **响应式机制（信号核 vs 键路径 diff）明确留到 v1.5 原型对拍再定**——这是 SDK-2 解释器与 SDK-4 校验器共同的上游未定项。【证】规划修订记录用户拍板第②项

### 二、正文论证

#### ① 已定裁决对账表

| 条目 | 出处原文摘句 | 对 SDK 的含义 |
|---|---|---|
| SDK-1..5 演进序 | 「SDK-1（随 GUI-8/9：dump round-trip 注入 + 截图 + 宿主像素对拍 + berryctl gui + .bap 脚手架）→ SDK-2（berryui DSL + 设备侧解释器 + FAT 热区热重载）→ SDK-3（…交互延迟 p50/p90 机器可读）→ SDK-4（AI 生成闭环：语义组件库 + 校验器 + MCP 式四 verb dump/inject/screenshot/reload）→ SDK-5（设计器 + Rust 级热重载，等 v2 动态加载）」（ADR #54） | SDK 有总路线图无排期真空；每段交付物可枚举，SDK-1 与 GUI-9 同刀 |
| dump round-trip 纪律 | 「视图树 dump 格式 day-one round-trip 可注入（稳定 id + 语义角色——dump 的 id 即注入器寻址键，一份数据喂调试/测试/AI 三消费者）」（GUI-9 行） | dump 格式是三消费者共用 ABI，id 稳定性从第一版就是协议属性而非调试便利 |
| DSL 载体（C2） | 「文本 DSL 为主、Rust 宏仅作类型安全绑定糖层…硬要求 = 『声明即数据』AST 导出 day-one，辅助 DSL 与代码共享同一 AST 非第二事实源」 | SDK-2 解释器、独立预览器、AI 生成产物共用单一 AST；不存在第二事实源风险 |
| manifest 扩展（裁决 F） | 「manifest 扩 `icon`/`label`/`views/` 字段（.bap v0 add-only 兼容——views/ = berryui 声明式 UI 资源挂点，SDK-2/AI 生成面前置）…AI 生成 UI 产物 = 数据文件非 Rust 代码」 | .bap 是 SDK 与 AI 共用的分发单元；扩键不破 v0 |
| SDK 三件套 | 「`berry-sys`（ABI crate）+ `berry-libc`（musl shim，hexyl 已验）+ 协议 crate 家族…第三方开发者视角 = Rust 工具链 + 一个 target 约定 + `mkapp` 打包」（应用模型 §3） | SDK 底座已实物化；协议 crate「每服务一个纯函数 crate，host 可测」是可复制范式 |
| ABI 冻结面（#43/#44） | 中间不设稳定 ABI；.bap = cpio + TOML manifest v0 冻结 | SDK 版本策略 = 静态链接自兜底；包格式已定型可依赖 |
| headless 黄金图 | 「headless + 黄金图逐字节对拍 day-one（iced 0.14 风向标，QEMU ramfb 确定性天然接 G 关卡）」（#54 补充） | SDK 验收不依赖人眼；确定性像素对拍是 SDK-1/3 的法定验收形态 |
| SDK 定位哲学 | 「差异化在验证不在生成——…确定性验证环恰是『AI 测试友好化』支柱的 GUI 展示位」（裁决 H） | SDK 不与 v0/Builder 拼生成能力，拼确定性验证环 |
| 并行纪律 | 「SDK 线自 SDK-2 起独立成刀、与 GUI 线文件面互斥可并行（裁决 H）」 | SDK-2 起可开副会话，不占 GUI 主线 |
| 交叉面纪律 | 「头文件漂移：禁手抄，单源生成」（总规划线A A4） | SDK 头文件/ABI 面必须走 abi.toml 单源，手抄即违禁 |

#### ② 未定问题清单（按决策紧急度排序）

**结构性阻塞级（现在不定、后面刀刀受阻）**：

1. **热重载状态保持语义边界**（SDK-2 前必须 ADR）：FAT 热区热重载已定载体，但「reload 时哪些运行时状态保留/复位」无任何文字。它决定 berryui 解释器的数据模型与 views/ 资源的寻址粒度——不定则 SDK-2 解释器、SDK-4 reload verb、GUI-2 段设计三者互相赌。【推】（仓内无此裁决文字，grep 未核到——本身即证据）
2. **响应式信号核 vs 键路径 diff**（v1.5 对拍再定）：已裁「延迟定」，但 DSL AST 形状、SDK-4 校验器语义均受其牵动——延迟有正当性（原型对拍证据门），需在 SDK-2 开工前明确「AST 面向两案中立」的约束，否则对拍结论出来时 AST 可能已偏。【推】基于拍板第②项 + #52「属性级依赖骨架留缝」
3. **guiproto v0 op 集合定形锚点**：GUI-1 注记 berry-guiproto 顺延至「GUI-2/4 先到先定」、协议窗口仍开着；per-interface version 已定（#50）但 op 集合未冻结——SDK-1 的 dump/inject 注入口（串口 + TCP 双通道）骑在它上面。【证】规划 §4 注记 + GUI-9 行
4. **spikes S1-S7 未跑**（GUI-10 §九，各带 kill criteria）：S1 已裁暂不跑留 GUI-2 刀前；语义组件库（SDK-4）选型证据门未过。【证】规划修订记录

**刀前必裁级**：5. gui-wmd 并入 guifbd 与否（GUI-5 前 ADR）；6. manifest v2 词表内容（`gui|headless|daemon` × `display/input/fs/net/background`——add-only 原则已定、词表未定，SDK-2 应用形态声明依赖它）。【证】应用模型 §3 v2 目标态

**v2 前裁即可**：7. 签名/版本化（「分发面出现时再立」）；8. provides 语义槽与 type=driver（挂 v2）；9. 存储四棵树 S3（SDK 分发面真地基，见硬结论 6）；10. 出网通路（socket ENOSYS/TLS 无议程——SDK-4 若要连远程 AI 模型即被卡，仓内诚实对账）。【证】AI工程面 §出网

**远期/治理级**：11. A6 kill 后 M-ECO 收官口径（§7.2 待裁决，影响 SDK 对外时点）；12. fat:rw/`just dev`/`just test` 开发体验挂账（索引维护待办在册）。

### 三、对 BerryOS 的可迁移启示（逐条可执行）

1. **dump 格式 schema 先于实现立法**：在 GUI-2/4 建 `user/berry-guiproto` 时把「稳定 id + 语义角色 + round-trip 可注入」写成协议头注 + host 往返测试（协议 crate 纯函数面模式），挂点 = GUI-2/4 刀 + 新 crate。
2. **热重载状态边界 ADR 前置**：SDK-2 开工前立「状态保持矩阵」（保留/复位/重建三态逐类枚举）入 `docs/架构决策.md`，挂点 = SDK-2 刀前 ADR。
3. **AST 中立性写进 #52 补注**：berryui AST 设计时显式记录「信号核/键路径两案可互换」约束并加 host 对拍测试，挂点 = `user/berryui`（未建）+ ADR #52 行。
4. **manifest 三字段随 GUI-7 落码**：`user/berry-manifest/src/lib.rs` + `tools/mkapp.py` add-only 扩键，selftest 负样本（旧包无新键必须过）同刀，挂点 = GUI-7。
5. **manifest v2 词表与 capabilities 扩键合并一次 ADR**：避免两次破面，挂点 = `docs/应用模型设计-v1.0.md` §3 + GUI-7/SDK-2 前置。
6. **SDK 脚手架模板走单源生成**：.bap 脚手架若带头文件/ABI 面，复用 `tools/gen_abi.py` 生成而非手抄（A4 纪律前移），挂点 = tools/。
7. **SDK-4 校验器复用 dump 通道 + 零特权红线**：四 verb 全走普通应用能力，结构化 dump 优于截图（Playwright MCP browser_snapshot 同构，GUI-10 附录 F 已论证），挂点 = SDK-4 刀。
8. **许可纪律入 SDK 选型 checklist**：Slint 三许可警示（闭源嵌入式须商业授权）已写入 GUI-10——印证 berryui 全自研，SDK 依赖引入时逐个过 `docs/架构综合纲领.md` 排除 8 问 + 许可栏。
9. **SDK 排期叙事双轨化**：对内 SDK-1..3 随 M-GUI、对外生态正面战 H3——文档与对外话术勿混用两个时点，挂点 = `docs/战略路线总规划-2026-10-v1.0.md` §3。

### 四、主要来源

- /Users/murongcheng/Documents/code/berryos/docs/GUI里程碑规划-2026-10-v1.0.md（修订记录、裁决 F/H 行、GUI-9+SDK-1 行、§4 并行性）
- /Users/murongcheng/Documents/code/berryos/docs/架构决策.md（ADR #43/#44/#47/#54 行、#50-#54 增补记录）
- /Users/murongcheng/Documents/code/berryos/docs/应用模型设计-v1.0.md（§3 SDK 三件套/:64/:125、§6 诚实缺口表/:214/:219）
- /Users/murongcheng/Documents/code/berryos/docs/存储布局与沙箱设计-v1.0.md（:163 v1 现状行）
- /Users/murongcheng/Documents/code/berryos/docs/AI工程面设计-v1.0.md（§5 红线/:91）
- /Users/murongcheng/Documents/code/berryos/docs/战略路线总规划-2026-10-v1.0.md（§2 线A A4、§3 H3、§7 待裁决）
- /Users/murongcheng/Documents/code/berryos/research/GUI-10-移动优先渲染深研综合裁决.md（附录 F w-sdk、§七）
- /Users/murongcheng/Documents/code/berryos/research/GUI-09-综合裁决建议.md（§2.g SDK 详情——裁决过程记录，结论以 ADR 为准）
- /Users/murongcheng/Documents/code/berryos/docs/更新与OTA设计-v1.0.md、docs/兼容子系统设计-v1.0.md、docs/开发体验.md、docs/索引.md（辅证）
- /Users/murongcheng/Documents/code/berryos/user/berry-manifest/src/lib.rs + tools/mkapp.py（2026-10-29 grep 字段核验：icon/label/views 未落码）

---

### 附录 C · Android SDK/NDK 与语言选型二十年（w:android）

## Android SDK/NDK 与语言选型二十年——结构性复盘(供 BerryOS GUI/SDK 线参考)

通道形态：WebSearch 可用，未降级；一手页面均经 WebFetch 直抓核验，引文以抓取原文为准。

### 一、硬结论清单

1. android.jar 是纯签名桩(方法体只会抛异常)，真实实现在设备框架上——Android 的“编译期面”与“运行期面”从第一天就是两套代码，靠 AOSP 的 API 文件比对守住兼容。【证】https://source.android.com/docs/setup/build/java-library 、https://stackoverflow.com/questions/11028001/build-non-stub-version-of-android-jar
2. 三值各管一段：minSdk=安装门槛(系统直接拒装)；targetSdk=行为兼容开关(平台高于 target 才启用兼容行为，调高即 opt-in 新行为);compileSdk 只是编译面、不在 manifest 里。【证】https://developer.android.com/guide/topics/manifest/uses-sdk-element
3. Play 用政策替 targetSdk 兜底：2026-08-31 起新 app/更新须 target API 36(Android 16),低 target 的存量 app 对新用户隐藏——三值治理里最关键的杠杆其实是分发政策而非技术。【证】https://support.google.com/googleplay/android-developer/answer/11926878
4. support-lib→AndroidX 是版本治理破产后的重建：v4/v7 按“支持的最低 API 级”命名、与平台版本纠缠；I/O 2018 宣布迁入 AndroidX 命名空间，Support Library 28 即终版。【证】https://medium.com/androiddevelopers/migrating-to-androidx-tip-tricks-and-guidance-88d5de238876 、https://developer.android.com/jetpack/androidx/migrate
5. Java 的代价是双线的：语言面长期停滞、靠 D8/R8 脱糖回填(lambda 于 AGP 3.0、java.time 等于 AGP 4.0、Java 11 子集于 AGP 7.4);法律面 Oracle 诉 Google 十年(2010–2021),2018 年陪审团曾算出 $8.8B 索赔，2021-04-05 最高院 6-2 以 fair use 收场。【证】https://developer.android.com/studio/write/java8-support ;https://www.eff.org/deeplinks/2015/06/bad-news-supreme-court-refuses-review-oracle-v-google-api-copyright-decision 、https://ccianet.org/news/2021/04/supreme-court-decides-landmark-google-v-oracle-case-on-copyright-interoperable-tech-products
6. Kotlin 迁移走的是增量杠杆而非强制：2017 官方支持、2019 Kotlin-first;2019-12 官方博客自报“Top 1000 应用近 60% 含 Kotlin”,驱动方式是新 Jetpack API Kotlin 优先(协程/KTX/Compose)。【证】https://android-developers.googleblog.com/2019/12/androids-commitment-to-kotlin.html
7. NDK 官方一直在自我劝退：概览页名句“native code … generally does not result in a noticeable performance improvement, but it always increases your app complexity”,guides 页只认“低延迟/计算密集(游戏)”与“复用 C/C++ 库”两类刚需。【证】https://developer.android.com/ndk/guides (概览句经 /ndk/guides 与 /ndk/guides/concepts 两页指认)
8. ART 走了个来回再收敛：5.0 全 AOT(装机时 dex2oat 编译，安装慢、耗空间)→ 7.0 混合(JIT 补 AOT，JIT profile 落盘、dex2oat daemon 解析驱动再编译)→ 9.0 DEX 紧凑化提速启动；纯 AOT 被放弃，profile 引导是终态。【证】https://source.android.com/docs/core/runtime 、https://source.android.com/docs/core/runtime/jit-compiler 、https://developer.android.com/about/versions/pie/android-9.0
9. 分发面把安装物产出权上收：AAB 是发布格式，“APK generation and signing deferred to Google Play”,2021-08 起新 app 强制；签名 v1(JAR 逐条目、只保内容不保元数据)→v2(7.0 全文件)→v3(9.0 密钥轮换)→v3.1(13 分版本轮换)→v4(11 增量 .idsig)。【证】https://developer.android.com/guide/app-bundle 、https://developer.android.com/studio/publish/app-signing
10. Gradle 痛是真实的但无官方数据：ant/ADT(2008–2013)→2013 随 Android Studio 切 Gradle→AGP/Gradle 版本矩阵成为独立复杂度源；社区大样本抱怨在册(so/reddit/quora“Why is the Android Studio / Gradle experience so bad?”),官方大样本构建耗时数据未核到。【证】https://developer.android.com/build/releases/agp-1-5-0-release-notes 等 AGP 版本页；口碑证据见下节来源
11. 安卓税可二分：架构性 = 宽容许可 + 无硬件强制 + HAL 厂商分叉(出生即定)；治理性 = OEM 升级慢(Treble 2017 才做框架/vendor 解耦)、targetSdk 躺平(Android 14 拦 target<23 的侧载)、公开分布数据撤退(API 分布表移出公开 dashboard 进 Play Console)。【证】https://developer.android.com/about/dashboards (现仅剩 Vulkan/GL 数据)；Treble 与 Android 14 拦截见来源

### 二、正文论证

#### 1. SDK 分层解剖

机制核心：SDK 里的 android.jar 只含 API 签名(桩)，编译器用它做类型检查，真身在设备 framework;AOSP 侧用 API 文件(current.txt 族)做前后版本比对防漂移——本质是“接口与实现分离 + 机器检查的接口冻结”。这与 BerryOS abi.toml 单源生成的动机同源，但 Android 是先痛后补(metalava/api-lint 是后期建设)，BerryOS 是 day-one。三值治理的精髓在 targetSdk:行为变化不按“平台版本”生效，而按“app 声明的 target”门控——兼容开关在平台手里、由 app opt-in,再用 Play 政策(app 一年内 target 最新)+系统拦截(Android 14 拒装 target<23)双管逼 app 前进。AndroidX 动因：support 库的 v4/v7 命名把“库版本”与“平台 API 级”焊死，随平台演进语义崩坏；AndroidX 改为独立命名空间+语义版本，Jetifier 机械改写旧引用——一次大规模人的迁移换来的教训是“库版本空间与平台版本空间必须解耦”。

#### 2. 语言选型史与代价

Java 线：起步借 Harmony 实现 + Java 语法生态，代价十年后才结清——语言特性落后主线(靠编译期脱糖分三次补齐)，API 面(libcore)冻结在旧版本，还要靠 core library desugaring 把 java.time 回填进 app 包。诉讼线：2010 Oracle 起诉 37 个 API/~11,500 行声明代码，两次联邦巡回翻案、2018 年 $8.8B 索赔、2021 最高院 fair use 收场——“引用他人 API 重实现”这条路的法律成本上限是十年诉讼与近百亿索赔。Kotlin 线：2017 官宣、2019 首选，路径是纯增量：老 app 不动也能活，新 API 以 Kotlin 优先(kotlin-first)倾斜，两年 Top1000 近 60% 渗透——语言替换成功靠“拉力不是推力”。NDK 线：官方定位二十年不变地克制，名句即劝退书；适用面收窄到性能刚需与库复用，普通 app 用 C++ 是纯负收益(复杂度必涨、收益不保证)。ART 线：dex/AOT 的摇摆史证明“装机全量 AOT”是错答案——安装慢、空间贵；终态是“解释+JIT+离线 profile 引导的选择性 AOT”,9.0 再用 DEX 紧凑化吃启动收益。

#### 3. 构建系统痛苦史

三段：ant+ADT(Eclipse 时代，构建逻辑藏在 IDE 插件里，CI 困难)→2013 Android Studio+Gradle(声明式构建、依赖管理，方向正确)→Gradle/AGP 自身成为税：配置阶段慢、daemon 内存、AGP×Gradle×Kotlin 插件版本矩阵、配置缓存又是补丁。社区证据海量(增量构建仍慢的普遍抱怨、80% 优化的案例文)，但官方大样本构建耗时数据未核到——只能定性：构建系统复杂度从“项目内”外移到了“工具链内”，总量未必降。给 BerryOS 的对照：构建确定性(mkimage 同输入同字节)与构建简单性(just+cargo)是 Android 没有的资产。

#### 4. 包格式与分发

AAB 的本质是“把最终安装物的组装权从开发者手里移到商店”:Play 按设备配置生成 split APK、开发者不再管理多 APK;代价是侧载不完整 split 会在 Android 10+ 失败、分发依赖商店基建。签名演进全是事故驱动：v1 不保 ZIP 元数据(重打包攻击面)→v2 全文件摘要+验证提速→v3 密钥轮换(此前"signing key never changes during the lifetime of your app"是硬约束)→v3.1 按目标 SDK 分版本轮换→v4 Merkle 树支持增量/流式安装。教训：第一版签名方案的全文件覆盖与密钥轮换预留，都是 day-one 该想到的。

#### 5. 「安卓税」二分

架构性(出生即定)：Apache 宽容许可+无硬件强制 → OEM 分叉自由是策略代价；HAL 落在厂商闭源 blob → 平台/vendor 耦合；API 面承载 20 年历史 → targetSdk 门控是结构必需。治理性(后来恶化/后来补)：OEM 升级迟滞逼出 Treble(2017 才解耦框架与 vendor、Pie 才吃到红利)；targetSdk 躺平逼出 Play 政策+系统拒装双执法；模拟器慢是早期选型(QEMU 全 ARM 仿真)欠的债，后来靠 x86 镜像+hypervisor+二进制翻译追平；公开 API 分布表 2020 年代从公开 dashboard 撤入 Play Console——生态数据本来是公共品，变成了登录墙内数据。

#### 6. 逆推 2007:SDK 面「不该继承什么」

① 不继承“桩与实现两套代码靠纪律同步”(abi 单源是正解)；② 不继承“库版本与平台版本纠缠的命名空间”；③ 不继承“语言栈依赖外部 IP/双语言桥接税”(JNI+脱糖十年补丁)；④ 不继承“首版签名不覆盖全文件、无轮换预留”；⑤ 不继承“安装物由商店侧重组产出”(分发确定性优先)；⑥ 不继承“插件生态式构建 DSL”;⑦ 值得继承的只有三样：targetSdk 式行为门控、profile 引导的预编译思想、Play 政策式版本前进杠杆(自控分发时可复用)。

### 三、对 BerryOS 的可迁移启示

1. ABI/SDK 单源是 android.jar 桩之痛的结构解，坚持“消费面 API 全部单源生成”:abi.toml 之外，berry-gfx 的 cmd/常量表(user/berry-gfx/src/cmd.rs)同样保持命令面单源+测试钉板，禁止出现第二份手写镜像。挂点：docs/系统调用ABI设计-v1.0.md §8、tools/gen_abi.py、berry-gfx tests。
2. 把 Android 三值治理裁进 .bap manifest(add-only 裁决已有先例):`target_level`(行为门控：新 OS 行为变化按 manifest 声明级别生效)+`min_level`(安装门槛，appd 拒装面)；行为变化按 target_level 门控写进文档而非事后补丁。挂点：user/berry-manifest、docs/应用模型设计-v1.0.md、app-0 线(appd)。
3. 库版本与平台版本解耦立法：berry-* 用户态 crate 家族一律语义版本、命名不绑内核/ABI 序号(AndroidX 教训)；写进应用模型文档作铁律，防 v4/v7 式语义漂移。挂点：docs/应用模型设计-v1.0.md。
4. 全 Rust 单语言 = Oracle 类诉讼风险 + JNI/脱糖双语言税的免疫；NDK 教训转译为“性能路径做进服务与 pass API,不给应用开绕过通道”——berry-gfx 的 VMO 零拷贝/队列路径优先，不给 app 直戳驱动的 blessed 通道。挂点：docs/驱动模型设计-v1.0.md(驱动=持证应用)、GUI ADR #52 pass 形态。
5. .bap 签名 day-one 用全文件方案(v2 形态：对整包字节签名)，manifest 预留 key lineage 字段(add-only);安装物=构建产物(mkapp 确定性)，拒绝“商店侧重组”式分发——保住同输入同字节与可复现安装。挂点：tools/mkapp.py、docs/应用模型设计-v1.0.md、与 just image 确定性同口径。
6. 构建面：坚持 just+cargo 串联，不引入插件式构建 DSL;把构建耗时纳入机器可读遥测(AI 测试友好化第三原则的构建版)，防 Gradle 式复杂度温水。挂点：justfile、tools/smoke.sh、docs/测试策略-v1.0.md。
7. profile 思想迁移：ART 的“真实使用画像驱动预编译”对应 BerryOS 的 IntentProfile 查表+遥测入场券(ADR #42)——应用行为画像(启动窗口/交互节奏)应设计为 SDK 可见资产而非运行时内部，供 AI 调度消费。挂点：docs/AI调度设计-v1.0.md、内核遥测环。
8. 生态数据保持公开机器可读(Android dashboard 撤退的反例)：遥测/兼容性数据出口格式化、可导出。挂点：docs/可观测性设计-v1.0.md。
9. QEMU-first+确定性 smoke 是模拟器税的正解，维持裸相零漂移与 smoke 时限纪律；GUI 线扩面时截图/golden 对拍沿用此基建(SDK-1 已有此方向)。挂点：tools/smoke.sh、GUI里程碑规划 SDK-1。

### 四、主要来源

- https://source.android.com/docs/setup/build/java-library
- https://stackoverflow.com/questions/11028001/build-non-stub-version-of-android-jar
- https://developer.android.com/guide/topics/manifest/uses-sdk-element
- https://support.google.com/googleplay/android-developer/answer/11926878
- https://developer.android.com/jetpack/androidx/migrate
- https://medium.com/androiddevelopers/migrating-to-androidx-tip-tricks-and-guidance-88d5de238876
- https://developer.android.com/studio/write/java8-support
- https://www.eff.org/deeplinks/2015/06/bad-news-supreme-court-refuses-review-oracle-v-google-api-copyright-decision
- https://ccianet.org/news/2021/04/supreme-court-decides-landmark-google-v-oracle-case-on-copyright-interoperable-tech-products
- https://android-developers.googleblog.com/2019/12/androids-commitment-to-kotlin.html
- https://developer.android.com/ndk/guides 、https://developer.android.com/ndk/guides/concepts
- https://source.android.com/docs/core/runtime 、https://source.android.com/docs/core/runtime/jit-compiler
- https://developer.android.com/about/versions/pie/android-9.0
- https://developer.android.com/guide/app-bundle
- https://developer.android.com/studio/publish/app-signing
- https://developer.android.com/build/releases/agp-1-5-0-release-notes 、https://developer.android.com/build/releases/agp-1-1-3-release-notes
- https://stackoverflow.com/questions/2451582/android-app-build-system-differences-between-eclipse-and-ant 、https://www.quora.com/Why-is-the-Android-Studio-Gradle-experience-so-bad (构建痛口碑)
- https://developer.android.com/about/dashboards
- https://computer-geek.net/android-14-could-stop-you-va-377.html (Android 14 拦 target<23,官方 behavior-changes 页未直核——此条降级为二手)
- https://events19.linuxfoundation.org/wp-content/uploads/2017/11/Project-Treble.-What-Makes-Android-8-Different_-Fedor-Tcymbal-Mera-Software-Services.pdf (Treble,一手演讲材料)

未核到项：Android P "cloud profiles" 的官方原文(该版本页未提，仅 jit-compiler 页证得 profile 机制本体)；Gradle 构建耗时官方大样本数据;android.jar 桩在 developer.android.com 的正面文档页(仅 SO+AOSP 构建文档佐证)。

---

### 附录 D · OpenHarmony/鸿蒙 Next 应用体系与 DevEco（w:oh）

## OpenHarmony/鸿蒙 Next 应用体系与 DevEco —— 结构与思路角度调研报告

**通道声明**：本次检索 WebSearch 可用，配合 HarmonyOS 开发者文档 RAG（官方文档一手检索）与本机 `~/Documents/harmony/openharmony/` 十仓浅克隆（2025-08~09 快照）抽查。仓内证据路径均为一手可查；无法核实处如实标注。

### 一、硬结论清单

1. **FA→Stage 的演进本质是「UI 承载与扩展场景分离」+ 窗口生命周期解耦**——Stage 模型把 FA 固定的 Page/Service/Data 三类 Ability 换成 UIAbility（有 UI）+ ExtensionAbility（按 type 扩展：form/service/inputmethod…）双轨，FA 已停止演进。【证】华为官方 Stage 模型概述 + 本机 `applications_launcher/product/phone/src/main/module.json5` 中主入口本身就是 `extensionAbilities[].type="service"`（launcher 无 UIAbility 主元素）。
2. **module.json5 的 `skills`(entities/actions) 就是 Android intent-filter 的重命名形态**，且系统用保留 intent（`entity.system.home`/`action.system.home`）锚定主屏应用。【证】上述 module.json5 逐字可见 `entities:["entity.system.home","flag.home.intent.from.system"]`。
3. **ArkTS 收敛（eTS→约束版 TS）是为编译期定死对象布局换取全量 AOT**——禁 structural typing、禁运行时改对象布局、限 any，类型信息直接喂给方舟编译器做静态优化；NEXT 安装期全量 AOT 预编译机器码，运行时是 .abc 字节码 + Panda 运行时。【证】华为官方 TS→ArkTS 适配规则系列 + 方舟运行时指南；【推】收紧清单与「渲染最小更新」需求同源——launcher module.json5 的 `metadata ArkTSPartialUpdate:"true"` 是该联动在配置面的化石。
4. **SysCap 机制官方定位＝「首要职责隔离设备类型能力差异、次要职责特性归类」**，每个 API 文档标注 `SystemCapability.xxx.xxx`，运行时 `canIUse` 查询，HAP 内 `syscap.json` 声明能力集，IDE 缺 SysCap 报错并指引移除声明。【证】华为开发者文档（RAG 检索原文：`develop-once-deploy-everywhere` 与 `syscap` 参考页）。
5. **API 版本治理是三元组编译期规则**：`compatibleSdkVersion <= targetSdkVersion <= compileSdkVersion`，违反即 hvigor 构建报错（错误码 00303015）；NDK 侧配套 `APIAVAILABLE(maj,min,patch)` 宏做编译期+运行时双检查防低版本调用崩溃。【证】华为开发者文档 hvigor 错误码页 + musl NDK 差异规格页（RAG 检索原文）。
6. **Kit 化是 API 组织面的真实重构**：官方 API 变更清单已按 Ability Kit/ArkData/ArkGraphics 2D 等 Kit 重组，从「按子系统散装 @ohos.xxx」收敛为面向场景的 Kit 检索面；「One ASP」术语未核到官方表述。【证】apidiff-510 官方文档；未核到处如实说明。
7. **「一次开发多端部署」的机制面（deviceTypes+SysCap+canIUse+自适应布局）是兑现的工程实体，「跨设备流转」是叙事与增值并存**——元服务（atomic service）免安装+万能卡片真实落地（卡片由 SystemUI 进程渲染、快照式），但流转依赖软总线+应用接入+同生态设备在场，NEXT 去兼容层后只能在鸿蒙设备间流转。【证】官方元服务文档脉络 + 华为云社区/凤凰网报道；后半句为【推】。
8. **NEXT 冷启动生态量化实录：1.1 亿行系统代码、675 万注册开发者、15000+ 原生应用与元服务、TOP5000 应用全覆盖（按使用时长 99.9% 口径）、华为年投 60 亿、微信重写耗时约 2 年**——头部集中策略有效，但绝对数量（~1.5 万）相对安卓百万级仍是零头，「全覆盖」靠的是时长口径。【证】第一财经/新华网/EET-China 报道 + 微信团队亲历者自述（搜狐 2025-10）；口径辨析为【推】。
9. **签名链三件套 p12/cer/p7b，调试 Profile 与发布 Profile 严格不混用，未签名 HAP 拒装，NEXT 强制代码签名**。【证】华为官方《配置应用签名信息》《发布应用》文档。
10. **DevEco Studio＝IntelliJ 平台 + 自研构建链（hvigor/ohpm）的混合体**；低代码（Super Visual/.visual）在 2.2 Beta1 引入、3.0 Beta3/4 仍推广，声明式范式成为主线后未见官方下线公告（未核到），社区可见其被淡化。【证】华为开发者论坛 2.2 Beta1 发布帖、腾讯新闻 3.0 Beta3 发布说明、百度百科；本机 `applications_launcher/hvigor/hvigor-config.json5` 与 `oh-package.json5` 一手佐证自研构建链。

### 二、正文论证

#### 1. 应用模型：Stage vs FA、hap 结构、BMS/权限/签名

Stage 模型（API 9 起）以 AbilityStage/WindowStage 为「舞台」：UIAbility 只管 UI 生命周期，窗口由 WindowStage 托管（多窗口/分屏自然获得），无 UI 场景全部走 ExtensionAbility 按 type 扩展。对比 FA 的固定三类 Ability，这是从「枚举式 Ability」到「UI/非 UI 二分 + 扩展槽型」的结构升级，动因是多设备形态与卡片/输入法等服务场景的涌现。本机 launcher 的 module.json5 提供了系统应用侧的活样本：`deviceTypes:["default","tablet"]`、`installationFree:false`（元服务标志位就在模块级配置里）、11 项 `requestPermissions` 全是 system_basic/system_core 级权限（INSTALL_BUNDLE、MANAGE_MISSIONS 等）——权限名全部 `ohos.permission.*` 命名空间化。应用级 `AppScope/app.json5` 只留 bundleName/vendor/versionCode/versionName/icon/label/minAPIVersion/targetAPIVersion 等全局字段——应用级与模块级配置严格分层，一模块一 HAP。签名链如结论 9；BMS（包管理服务）安装时消费以上全部信息。另注意到上层数字：应用间 HSP 共享包、bm 工具 `-w` 安装等待时长 180–600s 等细节（RAG 文档），显示安装面已高度工程化。

#### 2. ArkTS 语言收敛史

eTS 时代是「JS 超集 + 声明式范式」并存过渡；ArkTS 反向操作——从 TS 收紧而非从 JS 扩展。核心取舍：动态语言的运行时改布局、结构化类型、隐式转换都会让编译器无法做字段偏移预计算和去虚化，收紧后类型成为编译期资产，方舟编译器得以在安装时全量 AOT（.abc 字节码 → 本地机器码），启动无 JIT 预热。这与 ArkUI 声明式范式互为表里：状态装饰器（@State 等）在编译期生成最小更新代码，本机 ace_engine 中文 README 一手确认「声明式开发范式无需 JS Framework 进行页面 DOM 管理，渲染更新链路更为精简、占用内存更少」——两种范式共用 UI 后端引擎与语言运行时，但声明式砍掉了 DOM 管理层。

#### 3. SDK 分层与版本治理

四层：ArkTS/JS API（@ohos.*）、Native API（NDK + CMake 工具链，musl 口径）、System API（系统应用专用，文档与 IDE 染色隔离）、Kit 化检索面（API 12 起的官方组织方式）。SysCap 是把「设备差异化」从版本号问题转化为能力集问题的机制：接口级标注 + canIUse 运行时查询 + syscap.json 包级声明三层联动，文档明确「deviceTypes 并集/交集」规则（选了 Phone+Tablet 而接口只支持 Phone 时必须 canIUse）。版本治理三元组规则（结论 5）把兼容性从运行时崩溃提前到构建期报错——这是 AI/工具友好化的正面样本。

#### 4. 一次开发多端部署的真实成色

机制面真实：deviceTypes 多目标 + SysCap 差异隔离 + 自适应布局（断点/栅格）构成可验证的工程面；元服务免安装、万能卡片（SystemUI 渲染）真实可用。但「跨设备流转」依赖三个前提：软总线互联、应用显式接入流转 API、对端也是鸿蒙设备——NEXT 语境下手机↔平板↔手表生态内流转可用，跨出鸿蒙即无叙事。「10 亿设备」口径含存量 HarmonyOS（4.x 及以前兼容 AOSP 的版本）与大量 IoT 轻量设备，与「纯血 NEXT 设备数」不是同一口径——引用该数字做决策时必须拆口径【推】。

#### 5. NEXT 生态迁移实录（给新 OS 的量化教训）

关键数字见结论 8。补充定性证据：企业微信官方口径「绝非简单迁移」（2025-03），同程旅行三地团队协作适配（2024-01）。可提炼的冷启动规律：①头部时长集中度是唯一有效的覆盖口径（TOP5000≈99.9% 时长）；②平台方年投 60 亿 + 官方 IDE/云（AGC 签名/分发/云函数）先行是必要条件；③巨型应用重写周期以年计（微信约 2 年），新 OS 必须给生态 18–24 个月窗口与并行兼容期外的强激励；④15000 应用是「可用」阈值而非「繁荣」阈值【推】。

#### 6. DevEco Studio

基于 IntelliJ IDEA 平台：得＝编辑器/调试器/插件生态成熟、Java/Kotlin 系开发者零迁移成本；失＝平台升级受制于 JetBrains 上游节奏、发行体积大、深度定制（ArkTS 语言服务、预览器双向绑定、模拟器云/本地双形态）都要在别人地基上做。构建链干脆自研 hvigor（TypeScript 配置）+ ohpm 包管理器，不趟 Gradle。低代码（Super Visual）2.2→3.0 期间存在、主线转向声明式后淡化，未核到正式下线公告——如实记录。端云协同走 AGC（云函数/云数据库模板直连 IDE）。

### 三、对 BerryOS 的可迁移启示（可执行）

1. **manifest 学 module.json5 三件：`deviceTypes`、`min/targetAPIVersion` 双字段、`requestPermissions`（能力集语义）**。BerryOS 是纯能力系统，比鸿蒙更该把 requestPermissions 做成「安装时授予 capability 名单」而非权限位。挂点：`user/berry-manifest` 字段扩展 + `docs/应用模型设计-v1.0.md` + app-0 后续刀；BerryOS 三端（手表/手机/PC）同族，deviceTypes 直接可简化复用。
2. **SysCap 式「接口级能力标注 + canIUse 查询」移植进 abi.toml 单源**：每个 syscall/@ohos 式接口标 `berry.cap.*` 能力标签 + 支持端型集，gen_abi.py 生成机器可读能力表，运行时留一个 `cap_query` 类接口。这是「AI 友好化/测试友好化」的教科书落地。挂点：`tools/abi.toml` + `tools/gen_abi.py` 扩展列 + `docs/系统调用ABI设计-v1.0.md` §8。
3. **版本三元组编译期规则**进 mkapp.py：`compatible <= target <= compile` 违反即打包失败，学 hvigor 00303015 把兼容问题从运行时提前到工具期。挂点：`tools/mkapp.py --selftest` 加负样本。
4. **ArkTS 收敛印证 berryui「文本 DSL + AST 导出」路线的深层价值：编译期定死组件树/状态依赖图，运行时解释器零反射零字符串分派**；并在 DSL 编译目标里预留脏区/最小更新语义（ArkTSPartialUpdate 同型）。挂点：SDK-2 刀（`docs/GUI里程碑规划-2026-10-v1.0.md` ADR #54）+ `user/berry-gfx` 渲染脏区接口。
5. **skills/entities/actions 结构反哺 IPC intent**：BerryOS 已有 intent 五件套（调度语义），可加「应用注册 intent 过滤器」面（`entity.berry.home` 锚主屏/默认应用），为未来 launcher/服务发现预留。挂点：`docs/IPC设计-v1.0.md` + appd（svcmgr appd 雏形）后续刀。
6. **签名链三件套与「调试/发布 Profile 不混用」记入应用模型挂 v2 条目**：v1 无签名，但 manifest 里预留 profile 类型字段免得后补破格式。挂点：`docs/应用模型设计-v1.0.md` v2 节 + `user/berry-manifest` 注释。
7. **冷启动生态策略量化预期入路线图**：以「头部 N 应用覆盖 X% 时长」为生态验收口径而非应用总数；开发者工具（SDK-1..5 已裁）与云侧脚手架先于生态动员 12 个月。挂点：`docs/路线图-v1.0.md` M-G3 后生态节 + `docs/GUI里程碑规划-2026-10-v1.0.md` SDK 序。
8. **DevEco 的 IntelliJ 得失印证 BerryOS「不做 IDE、做 MCP 式四 verb（dump/inject/screenshot/reload）」的裁决**：编辑器面挂 VS Code/CLI 即可，把投资放在确定性协议与机器可读遥测上。挂点：SDK-4 刀（ADR #54 已裁，无需改动，此为外部佐证）。

### 四、主要来源

- 华为开发者联盟·Stage 模型开发概述：https://developer.huawei.com/consumer/cn/arkui/arkui-stage
- OpenHarmony 官博·分布式音乐播放器适配 Stage 模型：https://www.cnblogs.com/openharmony/p/16865455.html
- 华为官方博客·从 TS 到 ArkTS 的适配规则（十五）：https://developer.huawei.com/consumer/cn/blog/topic/03159465063725024
- 官方·API 变更清单（Kit 化组织）：https://developer.huawei.com/consumer/cn/doc/harmonyos-releases/apidiff-510
- 官方·配置应用签名信息：https://developer.huawei.com/consumer/cn/doc/HMScore-Guides/harmonyos-config-app-signing-0000001252142197 ；官方·发布应用：https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-publish-app
- 华为开发者文档（RAG 一手检索）：SysCap/canIUse（`harmonyos-references/syscap`）、`develop-once-deploy-everywhere`、hvigor 错误码 00303 系列、musl NDK 差异规格（developer.harmonyos.com/consumer/cn/doc/）
- 第一财经·15000 应用/1.1 亿行/675 万开发者：https://www.yicai.com ；新华网·万级应用 99.9% 时长：http://www.news.cn ；EET-China·TOP5000 口径：https://www.eet-china.com ；维基·鸿蒙操作系统 5 时间线：https://zh.wikipedia.org/zh-hans/鸿蒙操作系统5
- 微信鸿蒙版两年重建亲历：https://m.sohu.com/a/1089753951_117302 ；企业微信「绝非简单迁移」：https://finance.sina.com.cn/tech/roll/2025-03-04/doc-inennqmr2812865.shtml ；同程旅行：https://www.51cto.com/article/780195.html ；华为年投 60 亿：https://finance.ifeng.com/c/8d3jq8OEDTs
- DevEco Studio 2.2 Beta1（低代码/远程真机）：https://developer.huawei.com/consumer/cn/forum/topic/0201602552777860842 ；3.0 Beta3（Super Visual）：https://news.qq.com/rain/a/20220404A036R500 ；百度百科·DevEco Studio：https://baike.baidu.com/item/华为 DevEco Studio/57587464
- 本机一手仓：`~/Documents/harmony/openharmony/applications_launcher/{AppScope/app.json5,product/phone/src/main/module.json5,hvigor/}`、`~/Documents/harmony/openharmony/arkui_ace_engine/README_zh.md`
- 方舟运行时使用指南（二手转述官方）：https://blog.csdn.net/2401_82546228/article/details/138579376 ；华为云·鸿蒙三大核心特性：https://bbs.huaweicloud.com/blogs/457874

---

### 附录 E · Apple 平台 SDK 的一体化秘诀与代价（w:apple）

## Apple 平台 SDK 的一体化秘诀与代价

**通道形态声明**：WebSearch 可用，全程主用 WebSearch + WebFetch 直抓官方页（swift.org / developer.apple.com）；二手工程博客均已标注。SwiftUI 双轨期 Apple 官方立场原话未核到一手访谈，如实标注【推】。

### 一、硬结论清单

1. Xcode 一体化的 DX 核心是「单一责任边界」：编译/模拟器/签名/Instruments/上架全在一个工具内闭环，而 Android 面临 Gradle/SDK/依赖离散拼装【证】（[vibecodinghub Xcode 评测](https://vibecodinghub.org/blog/xcode)、[Medium 工具链综述](https://medium.com/@tijendra/the-best-mobile-app-development-tools-for-android-and-ios-45756ec9db5)）。
2. 一体化的代价是垄断性收敛：代码编辑可外移（sourcekit-lsp/VS Code/Neovim/Xtool），但签名、模拟器管理、上架仍绑回 Xcode+macOS；JetBrains AppCode 2024 停更即证据【证】（[Swift Forums Xtool 帖](https://forums.swift.org/t/xtool-cross-platform-xcode-replacement-build-ios-apps-on-linux-and-more/79803/4)、[AppCode 对比文](https://medium.com/@thetpine254/appcode-vs-xcode-for-ios-app-development-which-one-is-better-for-your-project-bc1c58d3745c)）。
3. Swift ABI 稳定花了 5 年（2014→2019 Swift 5），因为 ABI 稳定是不可逆的单行道——锁死内存布局/调用约定/mangling 前必须先成熟 generics 实现与 resilience 机制【证】（[swift.org ABI 博客](https://www.swift.org/blog/abi-stability-and-more/)、[Dice 分析](https://www.dice.com/career-advice/swift-5-abi-stability)）。
4. ABI 稳定的直接收益是 runtime 随 OS 发行、app 不再内嵌 Swift 标准库；代价是编译器失去「精确库知识」优化，靠 `@inlinable` 类注解在性能与灵活性间买平衡【证】（swift.org 同上）。
5. Apple 的 API 稳定性不是「承诺」而是「执法工具做出来了」：私有 API 用提交时自动静态分析拒审（Guideline 2.5.2），macOS 用 Developer ID 签名+公证服务+Gatekeeper 三层强制【证】（[Apple 公证文档](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)、[WWDC19-703](https://developer.apple.com/videos/play/wwdc2019/703)、[SO 预检讨论](https://stackoverflow.com/questions/59450742/how-to-detect-usage-of-private-api-before-submitting-to-app)）。
6. deprecation 纪律实际是「宽」不是「严」：无官方删除时限，弃用 API 常存活 5–10 年；偶尔硬切（OpenGL、32 位）【证】（[Eclectic Light](https://eclecticlight.co/2023/06/30/deprecation)、[Nutrient](https://www.nutrient.io/blog/what-is-a-deprecated-api-on-ios)）。
7. 系统库随 OS 演进靠三件套：dyld shared cache（系统 framework 预链接单一只读映像、跨进程共享）+ minimum deployment target + 编译器自动弱链接新符号配 `@available` 守卫【证】（[Apple 弱链接存档文档](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPFrameworks/Concepts/WeakLinking.html)、[Swift Forums 弱链接](https://forums.swift.org/t/weak-linking-of-frameworks-with-greater-deployment-targets/26017)、[Emerge Tools](https://www.emergetools.com/blog/posts/symbolicating-swiftui-and-any-apple-framework-part-2)）。
8. DFX 内置进 SDK 是 Apple 独有先例：`os_signpost` 打点直接出现在 Instruments 时间线、可自建 Custom Instruments、MetricKit 从用户设备回传崩溃/挂起/能耗诊断 payload（每日聚合一次）【证】（[MetricKit 文档](https://developer.apple.com/documentation/metrickit)、[Chime 实践](https://www.chimehq.com/blog/metrickit-crash-reporting)、[cocoafrog Custom Instruments](https://blog.cocoafrog.de/how-to/2020/06/20/You-should-all-build-this-custom-instrument.html)）。
9. SwiftUI/UIKit 双轨已 6+ 年未收敛，官方策略是「按屏混合」共存而非替换；社区出现迁回 UIKit 的反向案例（缺 iOS 2.0 就有的功能、可调试性差）【证】（[Ollie 案例](https://medium.com/goodones/why-ollie-is-moving-away-from-swiftui-to-uikit-cfdefe918d1c)、[混合策略](https://medium.com/swlh/swiftui-vs-uikit-a-practical-and-scalable-hybrid-strategy-for-modern-ios-development-e81ab4c8b370)）；Federighi 官方口径原话未核到【推】。
10. dyld shared cache 的代价面：单块格式反复变更（iOS 15 拆 subcache），符号化/逆向工具链持续断裂——一体化内部也有兼容债【证】（[DyldExtractor issue](https://github.com/arandomdev/DyldExtractor/issues/15)）。

### 二、正文论证

**1. 工具链一体化**。优势的本质不是「功能多」而是接口面收敛：一个开发者从新建工程到上架，全程只面对一个工具的默认值。Android 侧的痛点是决策面离散——Gradle 版本、AGP、SDK level、依赖仓库各自演进。但抱怨面同样真实：社区长期认为 Android Studio 更现代，甚至有高热帖直言 Apple 开发工具是「nightmare compared to Android ones」（[LinkedIn 帖](https://www.linkedin.com/posts/mikhail-chichaykin_techtalk-mobileengineering-xcode-activity-7405506726399180801-sRF0)）。结构性判断【推】：一体化买的是新手到上架的路径确定性，卖的是工具选择的自由；两者同源，不可拆零售。

**2. 语言策略治理节奏**。swift.org 官方博客把 ABI 稳定定义为「app 用 A 版编译器构建可与 B 版编译器构建的库对话」，达成后 app 不再内嵌 runtime。慢的原因是工程性的：泛型的 ABI（ Existential container、metadata 布局）必须一次锁对，锁错即全生态重编译。module stability（5.1）随后解决 swiftmodule 格式跨编译器导入。SwiftUI 双轨期长的根因【推】：声明式范式与命令式框架的语义映射没有稳定答案，Apple 选择让市场按屏迁移，官方只提供 UIViewRepresentable 类桥接件。

**3. API 治理**。三点合一：稳定性是执法出来的——提交流程内的静态分析（机器扫描二进制符号/selectors）、OS 内的 Gatekeeper、可撤销的公证票据。而 deprecation 宽松恰恰是同一体制的另一面：Apple 用「几乎不删」换取开发者对系统库的长期信任，用「私有 API 扫描」守住公共/私有的边界。

**4. 框架分层与加载**。dyld shared cache 把绝大多数系统 framework 预链接成单一只读文件，跨进程共享一份代码页——启动快+省内存，代价是格式封闭且变更频繁（第三方工具反复断）。弱链接三步（新 SDK 编译→低 deployment target→availability 守卫）让同一二进制横跨多个 OS 版本，这是「系统库随 OS 演进」的兼容脊柱。

**5. 诊断文化**。os_signpost 让业务打点与系统打点同轨出现在一个时间线里；MetricKit 证明「崩溃聚合」可以是 SDK 的一部分而非第三方 SDK 附件，其每日批量、需 dSYM 符号化的形态是隐私与实用的折中。

**6. 结构性/资源性分辨**。可抄的是纪律而非规模：ABI 冻结前的「不进 v1 面就显式标 experimental」、执法工具化（不是文档承诺）、诊断事件 schema day-one、availability 式版本守卫、聚合遥测。抄不起的是：双 UI 框架 6 年并行（每条轨道都是一个完整团队）、ABI 5 年打磨、云端公证/恶意扫描服务、App Store 静态分析执法面（没有商店就没有这个执法点）。

### 三、对 BerryOS 的可迁移启示

1. **ABI 单源+「单行道」自觉**（挂点 `tools/gen_abi.py`/abi.toml）：Swift 教训是冻结前把不确定面挡在冻结面外。建议 abi.toml 显式加 `experimental` 标记通道，v2 字段生成期即拒入冻结产物——比事后拆 ABI 便宜百倍。
2. **稳定性执法工具化**（挂点 `docs/系统调用ABI设计-v1.0.md` + berry-libc 测试）：BerryOS 的「31 号全实现+永不 panic」承诺已由 ktest/smoke 执法，同构于 Apple「静态分析拒私有 API」——保持「每个承诺配一个自动执法点」的清单化（可观测性文档补一节「承诺-执法对照表」）。
3. **os_signpost 同构件**（挂点 SDK-3：`docs/GUI里程碑规划-2026-10-v1.0.md` ADR #54 + berry-gfx 帧循环）：帧时间线 p50/p90 应定义统一事件 schema（begin/end 事件+稳定字段名），让 berryui 解释器、gfx pass、内核遥测环三方打点同轨可对齐——这正是 Instruments 的核心设计。
4. **MetricKit 形态借给 smoke 遥测**（挂点 `docs/可观测性设计-v1.0.md` + tools/smoke.sh）：boot 期遥测环+泄漏金丝雀已是雏形；补「聚合回传、机器可读 JSON、批量不实时」的输出形态，AI 测试友好化原则的直接兑现。
5. **弱链接思想的 v1 替身**（挂点 berry-manifest views/ 字段）：v1 无动态链接，dyld 题不存在；但 .bap manifest 的 add-only 纪律 + appd 对未知字段的容忍策略就是「deployment target 宽容带」的镜像，建议 manifest 文档明示「读者忽略未知字段」为协议承诺。
6. **反向教训——不做双轨**（挂点 berryui 拍板）：SwiftUI 双轨 6 年是资源性特权；BerryOS 应坚持「单一声明式 UI 框架+内核 console 退化透传」（C-4 已定形态），永不为过渡期维护第二套 UI 栈。

### 四、主要来源

- https://www.swift.org/blog/abi-stability-and-more/
- https://developer.apple.com/documentation/metrickit
- https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution
- https://developer.apple.com/videos/play/wwdc2019/703
- https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPFrameworks/Concepts/WeakLinking.html
- https://forums.swift.org/t/weak-linking-of-frameworks-with-greater-deployment-targets/26017
- https://forums.swift.org/t/xtool-cross-platform-xcode-replacement-build-ios-apps-on-linux-and-more/79803/4
- https://mjtsai.com/blog/2019/02/07/swift-abi-stability-and-more
- https://www.dice.com/career-advice/swift-5-abi-stability
- https://eclecticlight.co/2023/06/30/deprecation
- https://www.nutrient.io/blog/what-is-a-deprecated-api-on-ios
- https://medium.com/goodones/why-ollie-is-moving-away-from-swiftui-to-uikit-cfdefe918d1c
- https://medium.com/swlh/swiftui-vs-uikit-a-practical-and-scalable-hybrid-strategy-for-modern-ios-development-e81ab4c8b370
- https://www.emergetools.com/blog/posts/symbolicating-swiftui-and-any-apple-framework-part-2
- https://github.com/arandomdev/DyldExtractor/issues/15
- https://www.chimehq.com/blog/metrickit-crash-reporting
- https://blog.cocoafrog.de/how-to/2020/06/20/You-should-all-build-this-custom-instrument.html
- https://stackoverflow.com/questions/59450742/how-to-detect-usage-of-private-api-before-submitting-to-app
- https://vibecodinghub.org/blog/xcode
- https://www.linkedin.com/posts/mikhail-chichaykin_techtalk-mobileengineering-xcode-activity-7405506726399180801-sRF0
- https://medium.com/@tijendra/the-best-mobile-app-development-tools-for-android-and-ios-45756ec9db5
- https://medium.com/@thetpine254/appcode-vs-xcode-for-ios-app-development-which-one-is-better-for-your-project-bc1c58d3745c
- https://stackoverflow.com/questions/21691640/when-will-apple-remove-the-methods-apis-marked-as-deprecated
- https://lapcatsoftware.com/articles/notarization2.html

---

### 附录 F · 跨平台框架 SDK 形态参照：DX 什么算好（w:xplat）

## 跨平台框架作为 SDK 形态参照：DX 什么算好

**通道形态声明**：WebSearch 可用；本轮以 WebFetch 直抓各框架官方文档/官方博客为一手证据（【证】），第三方基准与社区讨论单独标注（【证·社区】），推断标【推】。

### 一、硬结论清单

1. Flutter 选 Dart 的核心含金量是「一门语言、双编译形态、语义同一」——开发态 JIT VM 提供有状态热重载、发布态 AOT 直出机器码，且官方明言 Dart「各模式（JIT vs AOT、native vs web）语义差异极少」。【证】docs.flutter.dev/resources/technical-overview
2. Flutter 热重载的边界是**成文的产品面**：保 app state、不重跑 `main()`/`initState()`；枚举↔类互换、泛型声明修改、原生代码、全局/静态初始化器改动需 hot restart / full restart；编译错误时打 `Hot reload was rejected:` 并指到行。【证】docs.flutter.dev/tools/hot-reload
3. Platform channel 的痛点 Flutter 官方用行动自认：官方包 Pigeon 的存在理由就是消灭「跨多平台多语言手工匹配字符串」的无类型 channel 样板，从一个接口定义生成 Kotlin/Swift/C++/GObject 五端代码。【证】pub.dev/packages/pigeon
4. pub 生态治理是「机器可评的质量分」：Pub Points 六类（API 文档覆盖率 ≥20%、静态分析、依赖是否跟最新、平台兼容经传递 import 图分析等），评分工具 pana 发布时自动跑、本地可预检。【证】pub.dev/help/scoring
5. RN 旧 bridge 的病根官方自述为四条：异步序列化瓶颈、JS/native 失同步后无法同步和解、单线程布局、原生结果只能走 callback；修复（JSI+Fabric+TurboModules+Codegen）从 2018 做到 2024-10-23（0.76 默认启用）耗时六年，且 Codegen 的动机原话是「跨界类型错误是跨平台应用最常见崩溃源之一」。【证】reactnative.dev/blog/2024/10/23/the-new-architecture-is-here
6. RN 的 Fast Refresh 官方文档同样把边界写透：默认保 React 函数组件/Hooks 局部 state；语法错误红盒但不整页 reload；被 React 树外模块引用的文件改动**回退 full reload**。【证】reactnative.dev/docs/fast-refresh
7. npm 版本地狱有真实伤害证据：HN「Ask HN: Who's tired of upgrading React Native?」整帖升级苦难实录、peer dependency 冲突与 0.61→0.66 升级失败实例。【证·社区】news.ycombinator.com/item?id=31195471
8. KMP/CMP 的共享边界是光谱而非二择：KMP 默认共享逻辑保留各端原生 UI，CMP 再共享 UI；官方诚实维护「Android-only components」清单（如 Maps Compose 不进 common），expect/actual 是漏口机制。对本仓的参照不在跨平台而在**边界清单纪律**。【证】kotlinlang.org/docs/multiplatform/compose-multiplatform-and-jetpack-compose.html（增量采用谱系一说为社区源）
9. Tauri 省 bundle 与常驻内存（不捆 Chromium，第三方基准 -58% 内存、~96% 体积），不省一致性：系统 WebView 三家三样（WebView2/WKWebView/WebKitGTK），GitHub #5889 有 Tauri 内存反超 Electron 的实例。【证·混合】gethopp.app/blog/tauri-vs-electron + tauri issue #5889
10. Elm 谱系在 Rust 侧已验证可行：iced 明文采用 The Elm Architecture（Model/Messages/Update/View 四件）并指出纯 FP 模式「借不可变性与引用透明在 Rust 里工作得非常好」；xilem = 轻量 view 树 diff 对 Masonry 保留树，自称 "inspired by React, SwiftUI and Elm"。berryui 已拍的声明式 + Msg-only 回调与此同构。【证】book.iced.rs/architecture + github.com/linebender/xilem
11. 热重载四档边界各异：Flutter=代码级有状态（受 2 约束）；RN=模块级保 state+错误可续；Compose Live Edit=字节码级热换+Compose invalidation、异常时「recompose using the last known good state」；Slint live-preview=**DSL 数据级**——reload 时「properties, callbacks, and models are preserved」、语法错误时旧版留屏、改原生接口需重编。SDK-2 的 FAT 热重载与 Slint 同档。【证】docs.slint.dev/.../live-preview + android-developers 官方博客

### 二、正文论证

**1. Flutter**。Dart 选型不是语言审美而是 DX 工程：JIT 开发态（增量 kernel 注入 VM、亚秒反馈）+ AOT 发布态（直出机器码）共用一套语义，官方把「两种模式几乎无语义差」当作承诺写进技术总览——这消除了「调试态能跑、发布态不一样」这一整类 DX 陷阱。热重载机制=重载改动的库→框架触发全量 rebuild/layout/repaint→只有位于根 widget `build()` 下游的改动可见；`main()`/`initState()` 不重跑，全局变量与静态字段「被视为 state 故不重新初始化」。这套边界官方单列一页文档，配 rejected-with-line-number 的报错形态——边界诚实本身就是 DX。platform channel 是其最痛的宿主桥接面：字符串路由、手工编解码、五端各自为政，以至于官方亲自下场做 Pigeon（类型契约单源生成多端）并在包文档里承认这比常见 method channel 模式「更高效」。pub 生态的可迁移点是评分即工具：pana 在每次 publish 时自动算分，作者本地可预跑，质量门槛变成了流水线的一部分。

**2. React Native**。旧 bridge 四病（见硬结论 5）导致列表空洞跳帧、60fps 难达、无法支撑 React 并发特性；修法不是补丁而是 2018 年起的地基重写，2021 年成立社区工作组、0.68 起 opt-in、0.76 默认，期间长期维持 interop 层兼容旧库——「病了六年才修完」的时间线本身就是架构债的教材。版本地狱的伤害则来自生态结构：npm 依赖树 + 原生模块各版本对 RN 版本的强耦合，升级=重解整棵依赖树（HN 整帖实证）。对照启示：**宿主运行时单一、版本域集中治理**能结构性消除这类地狱。

**3. Compose Multiplatform/KMP**。其共享策略是「按层光谱」：数据/域逻辑最稳，ViewModel 次之，UI 共享是可选项（CMP）；凡是与平台强绑定的 API 进官方维护的 Android-only 清单，不静默假装可共享。对 BerryOS 的参照度：本仓不需要跨 OS 一致性（自家 OS + 自家框架），但这套「**什么进 common、什么留在平台侧、清单谁维护**」的纪律，恰好对应「berryui 语法域哪些进解释器 common 面、哪些留给 Rust 宿主面」的划界问题。

**4. Tauri/web 栈**。Web 做 UI 层省的是引擎体积与生态复用，代价是渲染一致性交给三个系统 WebView（Linux WebKitGTK 与 Chromium 系行为漂移是长期痛点），且多 Tauri 应用同开时各带一份 webview 进程、内存优势收窄甚至反超（issue #5889）。对内存敏感的移动主战场（BerryOS v1 = 手机），把 Web 栈当 UI 层的账不划算——这反证了 berry-gfx 自绘路线的合理性。

**5. Elm 谱系**。Elm 的 Model-Msg-View 三分离在 Rust 生态有两个嫡系验证：iced（四件套命名 state/messages/update/view）与 xilem（view 树为不可变值，diff 后驱动保留树）。其 SDK API 含金量在于：**回调只有一种形状（Msg）**，宿主与 UI 的耦合面收缩到一个枚举+一个 update 函数，于是「热重载 View 层而不动 Model」天然成立。berryui 的 Msg-only 回调已站在同一条谱系上，这是最稳的一手牌。

**6. 热重载横评**。四家的真实分野在「改得动什么、错了会怎样」：Flutter 改得动 Dart 代码（白名单边界成文）但错在语法则拒载等修复；RN 保 Hooks state、树外引用回退整页；Compose Live Edit 做到字节码热换+Compose invalidation+last-known-good 回退，但官方承认支持面仍在扩展；Slint 只热更 `.slint` 数据——业务逻辑（Rust 侧）原地不动、属性/回调/模型全保、语法错误旧版留屏、动到原生接口签名才需重编。**SDK-2 的 FAT 热区热重载（DSL 数据热更）精确对标 Slint 这一档**：强在 BerryOS 的差异化不在热更本身而在验证环（dump/截图/宿主像素对拍闭环是 Slint 没有的）；缺在尚未成文的失败面语义——错误是否保旧树、什么改动需要重装 .bap、状态保到哪一层，都还没写成 Flutter/Slint 那样的单页边界文档。【推】

**7. 好 SDK DX 共性 checklist**：①脚手架到 hello-world 一命令可达；②报错带位置+建议动作（Flutter rejected+行号），非布尔 FAIL；③热重载/能力边界诚实成文（四家无一例外单列文档）；④质量分机器可评、本地可预检（pub points/pana）；⑤跨界类型契约走单源生成（Pigeon 与 RN Codegen 出自同一动机）；⑥dev/release 双形态同语义（Dart JIT/AOT 承诺）；⑦热重载失败面安全：错误不炸进程、旧版留屏、last-known-good 回退成为可观测事件。

### 三、对 BerryOS 的可迁移启示

1. **SDK-2 热重载边界单页成文**（挂 `docs/GUI里程碑规划-2026-10-v1.0.md` SDK-2 段 + berryui 刀）：仿 Flutter "Hot reload" limitations 段，明列「DSL 哪些改动热更生效 / 哪些需重装 .bap / Model 与 Msg 绑定是否保留」，并把 Slint 的三条语义（properties/callbacks/models preserved；语法错误旧版留屏；改宿主接口需重编）逐条对位写死。
2. **解释器失败面 = 产品面**（挂 berryui 设备侧解释器 + SDK-4 reload verb）：坏 DSL 拒载保旧树 + 机器可读错误码（行/列/期望 token），对齐「AI 测试友好化」支柱；学 Compose Live Edit 的 last-known-good 回退并打遥测事件。
3. **跨界类型契约单源**（挂 berryui AST + `tools/mkapp.py` 校验器 + SDK-4 校验器）：本仓 abi.toml→gen_abi.py 已是先例，把 AST 版本域与 manifest 校验统一到「单源生成/单源校验」形态——RN 官方对跨界类型错误的教训原话可直接引作动机。
4. **dev/release 双形态同语义承诺**（挂 SDK-2→SDK-5 路线）：解释器常驻（dev 热更）与 AST 预编译（release，Rust 常量/字节码）双通道共用同一 AST 单源，并在 SDK 文档显式承诺「两形态语义无差」（学 Dart）。
5. **SDK-1 DFX 报错质量**（挂 dump/inject verb 与 mkapp 校验器）：拒绝路径一律打「位置+建议动作」而非裸 FAIL；错误码结构化（语义化错误码总原则的 SDK 落点）。
6. **版本治理反面教材入册**（挂 `.bap` manifest/AST 版本域立法）：每刀冻结 DSL 语法版本、坏版本 loud 拒载，防 RN 式多版本共存地狱；远期若开放三方投递，把 pub points 式六类质量分做进 berryctl/appd 的机器复查面（v1 先留口，不实做）。【推】
7. **Web 栈否决记录**（挂 `docs/GUI里程碑规划-2026-10-v1.0.md` 或对应 ADR 附录）：以 Tauri 一致性代价 + 多实例内存账作为「移动主战场不走 Web UI 层」的引证备案。

### 四、主要来源

- https://docs.flutter.dev/resources/technical-overview
- https://docs.flutter.dev/tools/hot-reload
- https://pub.dev/packages/pigeon
- https://pub.dev/help/scoring
- https://reactnative.dev/blog/2024/10/23/the-new-architecture-is-here
- https://reactnative.dev/docs/fast-refresh
- https://news.ycombinator.com/item?id=31195471
- https://kotlinlang.org/docs/multiplatform/compose-multiplatform-and-jetpack-compose.html
- https://www.gethopp.app/blog/tauri-vs-electron
- https://github.com/tauri-apps/tauri/issues/5889
- https://book.iced.rs/architecture
- https://github.com/linebender/xilem
- https://docs.slint.dev/latest/docs/slint/guide/tooling/live-preview
- https://slint.dev/blog/slint-1.13-released
- https://android-developers.googleblog.com/2023/07/deep-dive-into-live-edit-for-jetpack-compose-ui.html

---

### 附录 G · 微内核/系统级软件体系：Fuchsia 组件框架 + 语言运行时管理（w:sys）

## 微内核/系统级软件体系调研报告：Fuchsia 组件框架 + 语言运行时管理

**通道声明**：WebSearch 可用，一手页面均经 WebFetch 直抓（fuchsia.dev、qnx.com、docs.sel4.systems、genode.org、musl 邮件列表等）；二手报道（Ars Technica 等）单独标注。

### 一、硬结论清单

1. **Fuchsia 把「能力即权限」卖到开发者面的载体是静态声明的 .cml，不是运行时查询**——组件不动态查权限，能力路由拓扑全部写在 manifest 里由 component manager 执行【证】[组件概念](https://fuchsia.dev/fuchsia-src/concepts/components/v2/introduction)。
2. **FIDL 版本化是 API 版本化、明言不管 ABI**：`@available(added/deprecated/removed/replaced)` 注解 + 平台线性版本史 + `--available` 编译期选版，单文件承载全历史【证】[FIDL versioning](https://fuchsia.dev/fuchsia-src/reference/fidl/language/versioning)。
3. **Fuchsia 包 = Merkle 内容寻址 blob 集 + meta.far 元数据**，同内容只存一份；base/cached/universe 三级包集合，base 运行期不可变、只随 OTA 换代【证】[包模型](https://fuchsia.dev/fuchsia-src/concepts/packages/package)。
4. **生态停滞是结构性三连**：终开发者语言面只有 C/C++（政策原文 "Rust is not supported for end-developers"）、SDK 分 public/partner/internal 三级把好东西锁在 Google 关系门内、装机只有 Nest Hub 一款设备——2023-01 裁员 16% 后彻底失速【证】[语言政策](https://fuchsia.dev/fuchsia-src/contribute/governance/policy/programming_languages)、[RFC-0107](https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0107_sdk_categories)、[Ars Technica](https://arstechnica.com/gadgets/2023/01/big-layoffs-at-googles-fuchsia-os-call-the-projects-future-into-question/)。
5. **「全 Rust OS」的真实代价记账**：Google 自家 Rust 重度内用的平台也不敢向终开发者开 Rust——因为「支持一门语言」= FIDL 后端 + 工具链 + 文档 + DevRel 的持续投资，政策明言理由是 "None of our current end-developers use Rust"（倒果为因的循环：不开面→没人用→更有理由不开）【证】[语言政策](https://fuchsia.dev/fuchsia-src/contribute/governance/policy/programming_languages)。
6. **Fuchsia 两头做 POSIX 是两个不同需求**：fdio/posix-lite 是「libc 便利 API 不承诺 POSIX 语义」（官方连 `..` 都写了立场文档判有害），starnix 是「存量 Linux 二进制生态」——BerryOS 只取前者与自有 ABI 并存，路线同构成色高，starnix 类层是 v2+ 的选项而非欠债【证】[Starnix](https://fuchsia.dev/fuchsia-src/concepts/starnix)、[Dot Dot Considered Harmful](https://fuchsia.dev/fuchsia-src/concepts/filesystems/dotdot)。
7. **QNX「工业 RT SDK」= 三件套**：Momentics 工具链 + APS 调度硬担保（分区最小 CPU 预算、闲置预算可被借用）+ 出厂预认证（ISO 26262 ASIL D / IEC 61508，认证是产品分级卖点）【证】[APS](https://www.qnx.com/developers/docs/6.4.0/neutrino/sys_arch/adaptive.html)、[QNX OS for Safety](https://qnx.software/en/software/products-and-solutions/qnx-os-and-os-for-safety)。
8. **seL4 自己证明了裸能力 API 卖不动**：官方在裸内核上叠了 Microkit SDK（静态结构系统 + 「一小撮抽象」）专降入门门槛——SDK 收窄面不是妥协是必需【证】[Microkit](https://docs.sel4.systems/projects/microkit/)、[capDL](https://docs.sel4.systems/projects/capdl)。
9. **musl 全静态 vs glibc 符号版本是两条路线的分野**：musl 无符号版本、刻意禁 `libc.a` 链入 .so（只允许全静态可执行文件），换来「一个二进制到处跑」；glibc 用符号版本 + NSS dlopen 换多版本共存——嵌入式/移动单映像场景选 musl 侧是先例共识【证】[musl 邮件列表](https://www.openwall.com/lists/musl/2022/04/20/2)、[SO 38251344](https://stackoverflow.com/questions/38251344/musl-fails-to-link-libc-a-into-shared-library)。
10. **Android 双 libc 之痛的根源是动态链接域切分**：bionic + linker namespace 把 system/vendor/app 三域库隔离，静态塞库进 .so 反而撞符号冲突——全静态单映像结构性免疫此类问题【证】[Android changes for NDK developers](https://android.googlesource.com/platform/bionic/+/fc8e688/android-changes-for-ndk-developers.md)。
11. **.bap 应停在应用自包含包，系统层走 A/B 整映像**——判据：系统包管理器自身是信任根与攻击面，需要签名/仓库/回滚全套基建；v1 的 `just image` 确定性整盘 + 应用包已覆盖两类真实需求【推】。

### 二、正文论证

#### 1. Fuchsia 组件框架：能力如何表达到应用面

组件 = 声明式 manifest（.cml，编译成 .cm 二进制）+ 程序 + 能力路由规则的组合单元。开发者声明 `use`（我要什么能力）、`offer`/`expose`（我给子组件/父域什么），component manager 在 realm（子树）里解析路由并实际派发 channel——**应用代码里没有 open("/dev/xxx") 这类全局命名空间探索，取而代之的是「启动时被授予的 handle」**【证】[组件概念](https://fuchsia.dev/fuchsia-src/concepts/components/v2/introduction)。这与 BerryOS「纯能力 handle + boot 直插/gift 装配」在内核侧同构，差别全在开发者面：Fuchsia 用声明文件把 gift 装配从代码挪到数据。安全分析（arXiv 2108.04183）确认 component manager 的静态能力路由即其安全架构中枢。

#### 2. FIDL 对照 abi.toml：单源的两种深度

FIDL 是接口定义语言→多语言绑定生成→版本化三合一：`fidl()` 目标一次生成 C++/Rust/Dart 等绑定；版本化层（`@available` + 平台线性版本史 + NEXT/HEAD）让**同一份接口文件服务所有历史版本**，消费端 `--available` 选版编译，且编译器保证「多版本集合可编译则任意子集可编译」；但官方明言这是 API 版本化，ABI 兼容不归它管、靠 CTF（Compatibility Test for Fuchsia）在发布间跑兼容测试兜底【证】[FIDL versioning](https://fuchsia.dev/fuchsia-src/reference/fidl/language/versioning)、[CTF](https://fuchsia.dev/fuchsia-src/development/testing/ctf/contributing_tests)。BerryOS abi.toml 已有「单源→三面产物」的形，缺两块：**演化叙事**（哪个号 added/deprecated、一个文件承载全历史）与**兼容性执法**（跨版本面的一致性测试）。Fuchsia SDK 分级（public/partner/internal，RFC-0107）+ 「core SDK 是最小可扩展基座」的组装模型——core SDK 精确构成页未核到（官方文档 2024 年改版中），仅确认分级机制与 sdk-samples/sdk-integration 外围工具在位【证】[RFC-0107](https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0107_sdk_categories)、[sdk-samples](https://fuchsia.googlesource.com/sdk-samples/getting-started)。

#### 3. 生态停滞的解剖

四因叠加：①语言漏斗窄——终开发者只有 C/C++（Rust 不支持、Dart 仅非驱动且 in-tree 需豁免、Go 被判内存/内核资源超支要迁走）；②SDK 面被 partner 门锁住，「We do not want to redistribute most third party…」（RFC-0216）显示 Google 不打算承接第三方库分发的信任链；③分发死循环——无设备量→无开发者→无应用→无设备量，ART 跑 Android 应用被当作过渡应用故事而非原生生态；④组织投入撤火——2023-01 裁员 16%（全公司平均 6%）后 Nest Hub 之外再无装机【证】[Ars Technica](https://arstechnica.com/gadgets/2023/01/big-layoffs-at-googles-fuchsia-os-call-the-projects-future-into-question/)、[9to5Google](https://9to5google.com/2023/01/21/fuchsia-area-120-google-layoffs/)。技术（组件框架/包系统）不是短板，**分发与开发者经济才是**。

#### 4. starnix：为什么 Fuchsia 两头都要、代价多大

官方定位：**非虚拟机**的用户态 Linux 兼容层，把 Linux syscall 翻译成对各 Fuchsia 子系统的请求，glibc/bionic 二进制都测过；社区类比 WSL1/WINE【证】[Starnix](https://fuchsia.dev/fuchsia-src/concepts/starnix)、[OSNews](https://www.osnews.com/story/133013/fuchsia-gets-support-for-running-unmodified-linux-programs)。它代价极高（一个用户态内核的长期维护面），动机是「生态桥接直到 Fuchsia 功能追平」（HN 对 F27 的讨论可见仍在推进）。BerryOS「自有 ABI + berry-libc、不跑 Linux 二进制」= 只做 fdio 那半（便利层），不做 starnix 那半（语义层）——边界干净，且 BerryOS 的 berry-libc 本就是 musl 口径自研实现，比 fdio（POSIX API 翻译到 Zircon 原语）还薄一层。

#### 5. QNX：工业 RT SDK 的完整形态

Momentics（IDE + 交叉工具链 + 调试）、SDP（交叉编译/调试环境）、Hypervisor（关键/非关键分域）、云开发环境；调度面 APS = 「命名线程组 + 最小保证 CPU 预算百分比」，负载时预算硬执法、闲置预算可借用（sporadic server 变体，USENIX 论文有分析）——这正是「软实时 + 隔离担保」的产品化表达；安全面 OS for Safety 出厂即带 ISO 26262 ASIL D / IEC 61508 / IEC 62304 认证，工具链 TCL3 级——**认证是 SKU 分级卖点**【证】[APS](https://www.qnx.com/developers/docs/6.4.0/neutrino/sys_arch/adaptive.html)、[Safety 产品页](https://qnx.software/en/software/products-and-solutions/qnx-os-and-os-for-safety)、[Blinder 论文](https://www.usenix.org/system/files/sec21-yoon.pdf)。BerryOS v2 工业 RT 档的启示：认证与工具链要从 day-one 的文档纪律（AI 友好化/可复现测试）里自然长出，而不是事后补。

#### 6. seL4/Genode：极简 SDK 的极端样本

seL4 裸面 = 全部资源管理在用户态 + 能力直传，系统形态用 capDL 描述（哪些实体持哪些权）；官方承认门槛太高，Microkit 以「SDK + 一小撮抽象 + 静态结构系统」把入门成本打下来【证】[capDL](https://docs.sel4.systems/projects/capdl)、[Microkit](https://docs.sel4.systems/projects/microkit/)。Genode 走另一极端的补法：整棵组件树 + 每组件最小权限沙箱 + Sculpt 桌面当门面，近年重点投「组件开发/调试/移植/发布工具」【证】[genode.org](https://genode.org)。两个样本共同反衬：**能力模型的 OS 要活，必须在裸能力之上再造一层「静态结构声明 + 少量高价值抽象」**——Microkit 的 protection domain 数量级是个位数抽象，不是框架森林。

#### 7. 语言运行时与包分发：谱系与判据

静态侧：musl 无符号版本（刻意设计，邮件列表明言）、禁 libc.a 入 .so、Alpine/嵌入式全静态先例；动态侧：glibc 符号版本 + NSS dlopen 服务多版本共存的发行版世界。Android 的教训是**混合态最痛**：bionic 单 libc 但 system/vendor/app 三域 linker namespace 隔离，静态塞库进 .so 撞系统自带库符号冲突，NDK 静态 libc.a 不能进共享库【证】[musl 列表](https://www.openwall.com/lists/musl/2022/04/20/2)、[NDK changes](https://android.googlesource.com/platform/bionic/+/fc8e688/android-changes-for-ndk-developers.md)。包分发三档：系统包管理器（apt/brew——信任根在仓库，包管理器自身是关键攻击面）、自包含应用包（AppImage/Flatpak/nix——信任根在签名/内容寻址，运行时随包或基础运行时钉死）、应用商店（分发 + 审核一体）。Fuchsia 的选择是「系统侧 base 包随 OTA 整体换代（A/B）+ 应用侧内容寻址包」——两档各答一问，不混。BerryOS v1：`just image` 确定性整盘（A/B 的雏形）+ .bap 自包含静态包，恰好同构；**该不该长系统包层的判据 = 是否出现「第三方交付的系统组件」**（第三方驱动/服务要进 base），在此之前包管理器是纯负资产【推】。

### 三、对 BerryOS 的可迁移启示

1. **manifest 长成「小 cml」**：把 spawn 的 gift 装配（fs gift 四元组、console 通道等）从 svcmgr 硬编码迁为 manifest 声明段（`[gifts]`/`[caps]`），appd 按声明装配——能力拓扑变成数据、可审计可校验。挂点：`user/berry-manifest`（icon/label/views 已裁扩后顺势加）、`user/svcmgr appd`、`docs/应用模型设计-v1.0.md`。
2. **abi.toml 补版本域**：为 syscall/op 加 `added`/`deprecated` 元数据（FIDL `@available` 的最小版），`tools/gen_abi.py` 支持 `--version` 选面生成 + 兼容性守卫；ABI 演化叙事从 git 历史升格为单源数据。挂点：`tools/gen_abi.py`、`docs/系统调用ABI设计-v1.0.md` §8。
3. **CTF 式兼容执法**：把「上一版 ABI 产物生成的旧用户程序二进制」存为 fixture，新版内核必须原样跑过——对齐 Fuchsia CTF 思路与本仓「跨实现共识基准」纪律。挂点：`tools/smoke.sh` 新相或 `tools/mkapp.py --selftest` 扩展。
4. **mkapp 内容寻址先落 sha256**：manifest 记录每文件摘要（Merkle 全树留 v2），day-one 具备去重与完整性校验面。挂点：`tools/mkapp.py`。
5. **语言面只承诺 Rust 单语言**：SDK-1..5 每刀的绑定/文档/工具只做 Rust；「多语言绑定支持」明文列为非目标（Fuchsia 教训：开语言面 = FIDL 后端+工具链+文档+DevRel 四线持续投资）。挂点：`docs/GUI里程碑规划-2026-10-v1.0.md` SDK 节、`docs/架构决策.md` 新 ADR。
6. **POSIX 边界立法成文**：仿 "Dot Dot Considered Harmful" 写一篇 berry-libc 立场文档——便利 API 全给、POSIX 语义（fork/全局命名空间/`..` 语义）明确不承诺，防后人「顺手补齐」腐蚀能力模型。挂点：`docs/兼容子系统设计-v1.0.md`（v2 挂账四份之一）。
7. **运行时策略定死全静态**：v1 禁 .so、禁动态链接器，App ELF 全静态（musl 口径现状即正确态），写 ADR 钉死；未来开动态链接必须整案裁决（Android namespace 隔离就是补丁的代价）。挂点：`docs/架构决策.md` 新 ADR、`user/berry-libc`。
8. **.bap 停应用包、系统走整映像 A/B**：系统包层立项判据 = 出现第三方系统组件交付需求；在此之前 `just image` + 双分区 A/B 是系统更新的完整答案。挂点：`docs/更新与OTA设计-v1.0.md`（v2 挂账）。
9. **APS 对照 AI 调度 M4**：M4「×4 带 + 预算包络 + deadline-server 兜底」与 APS「最小预算 + 闲置借用」同构，可借其「负载时硬执法/空闲时借用」的表述与验收形态（分区预算越界可遥测验证）。挂点：`docs/AI调度设计-v1.0.md`、`docs/调度器设计-v1.0.md` M4 节。

### 四、主要来源

- https://fuchsia.dev/fuchsia-src/concepts/components/v2/introduction
- https://fuchsia.dev/fuchsia-src/contribute/governance/policy/programming_languages
- https://fuchsia.dev/fuchsia-src/reference/fidl/language/versioning
- https://fuchsia.dev/fuchsia-src/concepts/packages/package
- https://fuchsia.dev/fuchsia-src/concepts/packages/ota
- https://fuchsia.dev/fuchsia-src/concepts/starnix
- https://fuchsia.dev/fuchsia-src/concepts/filesystems/dotdot
- https://fuchsia.dev/fuchsia-src/development/testing/ctf/contributing_tests
- https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0107_sdk_categories
- https://fuchsia.dev/fuchsia-src/contribute/governance/rfcs/0216_hosted_repositories
- https://fuchsia.googlesource.com/sdk-samples/getting-started
- https://arstechnica.com/gadgets/2023/01/big-layoffs-at-googles-fuchsia-os-call-the-projects-future-into-question/
- https://9to5google.com/2023/01/21/fuchsia-area-120-google-layoffs
- https://www.osnews.com/story/133013/fuchsia-gets-support-for-running-unmodified-linux-programs
- https://www.qnx.com/developers/docs/6.4.0/neutrino/sys_arch/adaptive.html
- https://qnx.software/en/software/products-and-solutions/qnx-os-and-os-for-safety
- https://www.usenix.org/system/files/sec21-yoon.pdf
- https://docs.sel4.systems/projects/capdl
- https://docs.sel4.systems/projects/microkit/
- https://genode.org
- https://www.openwall.com/lists/musl/2022/04/20/2
- https://stackoverflow.com/questions/38251344/musl-fails-to-link-libc-a-into-shared-library
- https://android.googlesource.com/platform/bionic/+/fc8e688/android-changes-for-ndk-developers.md
- https://en.wikipedia.org/wiki/Bionic_(software)

---

### 附录 H · AI 原生 SDK：SDK 的第一客户是 AI（w:ai）

## AI 原生 SDK：SDK 的第一客户是 AI

**通道声明**：WebSearch 可用 + WebFetch 直抓一手源（官方 repo/规范站/arXiv），全部结论经一手核实；标注【证】的均有 URL，推断标【推】。

### 一、硬结论清单

1. 结构化快照优于截图已是 MCP 自动化的事实共识——Playwright MCP 官方 README 明写"bypassing the need for screenshots or visually-tuned models""Deterministic tool application. Avoids ambiguity common with screenshot-based approaches"，browser_snapshot 的工具描述就叫"this is better than screenshot"。【证】
2. mobile-mcp 走同一裁决："drives apps from the native accessibility tree (no vision model, no image tokens)"，截图仅作兜底（"falling back to screenshots + coordinates only when needed"）；其工具面除交互外还带 `mobile_get_device_logs`/`mobile_list_crashes`/`mobile_batch_commands`（诊断 verb + 批量 verb）。【证】
3. MCP 协议层已把「AI 是第一客户」写进规范：inputSchema/outputSchema 双 JSON Schema（默认 2020-12）+ structuredContent 结构化应答 + 两层错误模型（协议错误 vs 带可操作反馈的 Tool Execution Error），后者明文"clients SHOULD provide tool execution errors to language models to enable self-correction"；连 tools/list 都要求确定性排序以提升 LLM prompt cache 命中率。【证】
4. 工具 schema 就是 API 文档：Anthropic 工程博客给出可操作口径——描述要按「对新同事讲解」写、参数名无歧义（`user`→`user_id`）、错误响应要"clearly communicate specific and actionable improvements"、把 UUID 换成语义词可实测降低幻觉、少而强的合并式工具优于工具海。【证】
5. AI 助手的上下文消费已有两份事实标准：AGENTS.md（无 schema 纯 Markdown，Codex/Cursor/Copilot 等 20+ 工具读取，6 万+项目采用，就近优先合并）与 llms.txt v2（H1+引用块+H2 链接列表，Mintlify/GitBook 自动生成，MCP 官网文档自己就挂了一份 llms.txt——吃自己狗粮）。【证】
6. 「AI 生成产物=数据而非代码」有成熟先例：微软 TypeChat（schema 约束 NL→JSON，校验失败自动回喂修复）、SDUI 谱系（Airbnb 全平台后端控 UI、Doist 采用 Adaptive Cards 的 JSON Schema）；而 Design2Code（arXiv 2403.03163，484 真实页面）证明截图→代码距人类水平仍有实质差距——支持「让 AI 生成受约束的 AST/DSL，不生成代码」。【证】
7. AI 自验闭环的原语是「结构化断言面」：Playwright 1.49 的 `toMatchAriaSnapshot` 把无障碍树 YAML 化做快照断言；Anthropic 的工具评估方法论要求"each evaluation prompt should be paired with a verifiable response or outcome"并警告"avoid overly strict verifiers"。【证】
8. 反面证据充分：API 幻觉有专文（arXiv 2505.05057，定义 MiHN/MaHR 双指标，RAG 基线仍留大量幻觉，依赖约束解码再降 67.52%/73.56%）；slopsquatting（幻觉包名被抢注，CSA/Endor Labs 2025 系列）证明 AI 生成物必须过白名单与版本锁。【证】

### 二、正文论证

**1. MCP 生态对照 BerryOS SDK-4 四 verb。** Playwright MCP 的关键设计不是「有截图工具」而是「交互锚定在结构化树上」：所有交互工具的 target 参数来自 snapshot 里的元素 ref；为省 token 还提供 `browser_find`（局部查询"cheaper than capturing the whole snapshot"），坐标交互反而要 `--caps=vision` 显式开启。这对 dump/inject/screenshot/reload 的直接映射是：**dump 必须是语义树（带可引用 ref），screenshot 是辅助通道**；且 dump 应带范围/过滤参数（browser_find 同款意图）。mobile-mcp 补出两个 BerryOS 尚未裁决的 verb 族：**诊断类**（logs/crash——BerryOS 有遥测环与遥测设计文档，天然可暴露）与**批量类**（batch_commands，多步单调用省往返）。是否加「assert verb」：MCP 生态把断言放在客户端侧（aria snapshot 模板），服务端只提供 dump——【推】BerryOS 若做 assert verb 应退化为「dump + 期望对拍」的语法糖，不引入服务端断言语义，避免双真相源。

**2. 上下文工程事实。** AGENTS.md 与 llms.txt 的分工清晰：前者治理「AI 怎么参与构建」（命令/规范，就近合并、明文冲突规则），后者治理「网站/文档怎么被读」（策展式入口+分级 Optional 段）。二者共同点是**放弃花哨格式、拥抱纯文本可预测性**。BerryOS 仓内 CLAUDE.md/AGENTS.md 对偶已合流于社区标准；缺口在对外 SDK 面——SDK 用户没有等价的「最小上下文入口」。

**3. berryui AST 作为 AI 生成目标。** TypeChat 的核心不是 prompt 而是闭环的后半边：**生成→校验→失败信息回喂→修复**。SDUI 先例（Airbnb/Adaptive Cards）证明 JSON 树作为跨端 UI 载荷可行且宜测试。对照 berryui：DSL→AST 已裁（ADR #50-54），缺口有二——①**校验器的错误信息是否面向 LLM 可修**（对照 Anthropic「actionable improvements」）；②**组件集是否白名单+语义命名**（幻觉 API 的约束解码思想：生成期就限制在注册过的组件/属性内，2505.05057 的依赖约束解码是同思路）。Design2Code 的存在本身即论证：不要让 AI 从像素或自由代码起步，让它从 schema 起步。

**4. AI 可测性基建。** 生态给出的最小闭环原语集 = 结构化 dump（可断言的树/JSON）+ 确定性对拍（aria snapshot、golden 像素）+ 机器可读的判定结果（避免「过严校验器拒掉正确答案」）。BerryOS SDK-1 已裁 dump round-trip 注入/截图/宿主像素对拍，方向与 Playwright 共识一致；berry-gfx 的 golden 回归+fbcon 像素共识已是设备侧地基。真正要补的是**判定结果的机器可读化**（对拍差异输出成结构化 JSON，而不是人读日志），SDK-3 的 p50/p90 机器可读帧时间线是同款纪律，可同一刀收口。

**5. 机器可读文档谱系。** MCP 官网的做法值得逐字学：文档站首页就指路自家 llms.txt，工具的 input/output 双 Schema 即接口文档，注解（readOnlyHint 等）声明行为语义。这正是 abi.toml→gen_abi 三面（kernel/berry-sys/文档 marker）的成熟版形态：**单一事实源→人读文档+机器读 schema 双产物**。OpenAPI/JSON Schema/TypeDoc 谱系同理（Fastify 社区讨论即 schema-first 派对 types-first 派的争论实录）。

**6. 反面清单的治理映射。** 三条攻击/失效面：幻觉 API（arXiv 2505.05057：RAG 补文档仍不够，需约束解码）、幻觉包名（slopsquatting：AI 编造的包名被抢注投毒）、slop 代码（Simon Willison 缓和面论证：代码幻觉是「最不危险」的一类——运行即炸，怕的是静默错误）。对新 OS SDK 的推论：**默认零信任 AI 产物，但把拒绝信息做成修复指令**——校验失败必须能回喂闭环，否则只剩「拒绝」没有「闭环」。

### 三、对 BerryOS 的可迁移启示

1. **SDK-4 dump verb 定形为语义树**：节点带稳定 ref（对照 Playwright 元素 ref 贯穿交互），附范围/过滤参数（browser_find 同款）；screenshot 保持辅助。挂点：SDK-1 DFX dump 设计 + `user/berry-gfx`（pass API 已有结构信息，补树序列化导出）。
2. **verb 面扩展裁决（ADR #54 add-only）**：加 `logs`/`trace` verb（消费内核遥测环/SDK-3 帧时间线，mobile-mcp 的 get_device_logs 直接先例）；评估 `batch` verb 收编多步注入省 RPC 往返。挂点：SDK-3/SDK-4 刀序设计、`docs/GUI里程碑规划-2026-10-v1.0.md`。
3. **verb 协议学 MCP 双 Schema + 两层错误**：应答定 JSON Schema 进 host 测试钉板（`user/berry-psproto`/`berry-netdproto` 严格串行 RPC+host 测是现成模式）；错误分「协议错（不可自修）」与「执行错（必带下一步可操作建议）」。挂点：SDK-4 新协议 crate 或 svcmgr 扩展 + 三条总原则之「语义化错误码」落地。
4. **berryui 校验器按 LLM 可修设计**：错误信息含「位置+期望+修法」三元组；组件/属性白名单在解析期执法（约束解码思想）。挂点：SDK-2 设备侧解释器 + `user/berry-manifest`（views 字段已裁，校验器同族）。
5. **文档双产物**：abi-gen 增第四面——为 SDK/berryui 生成 llms.txt 式策展入口 + JSON Schema 导出（MCP 官网吃狗粮为证）。挂点：`tools/gen_abi.py`/`docs/系统调用ABI设计-v1.0.md` §8 延伸。
6. **.bap 治理**：manifest 校验拒绝未注册组件/未声明能力引用（slopsquatting 教训的包内版）；appd spawn 守卫维持能力最小化默认。挂点：`user/berry-manifest` 负样本测试 + svcmgr appd。
7. **AGENTS.md 纪律延续**：SDK 发布时随 crate 附「AI 最小上下文文件」（就近优先合并语义已内建）。挂点：各 SDK crate 根。

### 四、主要来源

- https://github.com/microsoft/playwright-mcp （README 一手：snapshot 优于 screenshot、--caps=vision、browser_find）
- https://playwright.dev/mcp/tools/interaction
- https://github.com/mobile-next/mobile-mcp （accessibility-first、工具清单一手）
- https://modelcontextprotocol.io/docs/concepts/tools （inputSchema/outputSchema、structuredContent、两层错误、确定性排序、stateful handle——一手规范）
- https://www.anthropic.com/research/writing-tools-for-agents （合并工具/命名空间/response_format/语义化 ID/错误可操作/用 agent 评工具）
- https://agents.md/ 、https://llmstxt.org/ （两份规范一手）
- https://arxiv.org/abs/2403.03163 （Design2Code）、https://github.com/NoviScl/Design2Code
- https://microsoft.github.io/TypeChat/blog/introducing-typechat 、https://github.com/microsoft/typechat.net
- https://www.infoq.com/news/2021/07/airbnb-server-driven-ui 、https://www.doist.dev/server-driven-ui-from-a-mobile-perspective （Adaptive Cards）
- https://playwright.dev docs aria-snapshots（toMatchAriaSnapshot，经官方文档与社区交叉核实）
- https://arxiv.org/abs/2505.05057 （API 幻觉/MiHN/MaHR/MARIN）
- https://labs.cloudsecurityalliance.org/research/csa-research-note-slopsquatting-ai-supply-chain-20260419-csa 、https://www.endorlabs.com/learn/slopsquatting-when-ai-agents-hallucinate-malicious-packages
- https://simonw.substack.com/p/hallucinations-in-code-are-the-least
- https://cdn.amazon.science/8f/83/7407a5634a80a39e82b52ae935fe/on-mitigating-code-llm-hallucinations-with-api-documentation.pdf （CloudAPIBench）

**未核到项**：OpenAPI 官方「同一 spec 生成人类文档+SDK」的权威页面未直抓（以 Fastify schema-first 讨论 + TypeDoc 生态侧面佐证）；mobile-mcp 早期「interactive_troubleshooting/suggest」verb 形态在当前 README 已不可见（疑为迭代移除），按当前一手工具清单论述。

---

