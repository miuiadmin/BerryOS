# BerryOS Rust SDK 方向审计与声明式布局深研综合报告（SDK-02）

> **溯源**：2026-11-01 用户问询「使用 workflow 模式研究，看我们的软件开发 SDK 对不对？我们这个软件要用 Rust 来开发 SDK，这些怎么开发？基建文档、开发方向对不对？包括声明式布局怎么构建？都要出方案」触发。
> **产制**：workflow 深研模式（run `wf_04faae38-fa0`，7 agent = 盘点×3（A1 SDK 面 / A2 声明式面 / A3 基建文档）+ 对标×2（B1 SDK 外部 / B2 布局深研）+ 对抗校验×2（C1 交叉校验 / C2 布局可行性批判）；87.2 万 token / 20.9 分钟 / 207 工具调用 / 0 失败 0 残缺）。
> **基座与边界**：SDK-01（2026-10-29 workflow 深研已拍板 → ADR #55–#57）与 GUI-07/08/10 前研为已裁基座，**本轮不重研已裁事项**——只做「现状审计 + 2026-11 增量校验 + 方案构建」。
> **状态**：**已拍板（2026-11-01）**——用户全权委托（原话「你自己帮我拍啊，我要最彻底、架构最好、最优秀，不顾代价的，不计成本的」），七件全裁取顶配，裁决与执行位见 §6.1；对抗校验记录（C1 抽验 30+ 仓内锚 + 9 外部 URL、C2 直读 berry-ui 4359 行全量）见附录 F/G。

---

## 第一部分 · 执行摘要（硬结论 10 条）

1. **SDK 方向判定 = 正确，且已从「纸面立法」进入「可执法资产」阶段**。SDK-01 六大主张（六层塔 / v1 全静态 / 禁脚本层 / crates.io 直发 / Rust 单语言明文非目标化多语言 / 中间不设稳定 ABI）经 2026-11 外部证据逐条核验**全部仍成立、无一需修正无一过时**（附录 E validations 六条；C1 抽验外部 URL 九处全 CONFIRMED）。新证据反而两侧加固：Fuchsia 语言政策 2025-02-28 修订后仍原文保留「Rust is not supported for end-developers」；Android NDK 2026 仍无一等 Rust 支持；Google 官方 2025-11 数据 Rust 新代码内存安全漏洞密度比 C/C++ 降 **1000×**——「开语言面 = FIDL 后端+工具链+文档+DevRel 四线持续投资的治理决策」判断不变。仓内最强两条证据：24 crate 累计 **~740 host 测试零 QEMU 依赖**（gfx 111 / ui 96 / loader 62 / fsclient 50 / netdproto 42）；**三道机器门禁**现役（mkapp 三值链编译期执法 / abi-check 三面逐字节 / CI sdk-publish-guard）。层间依赖方向铁律（ADR #43）在 crate 依赖图上实际成立——GUI 族闭包 = {gfx, guiproto, fbcon}，零 fs/net/kernel 反向沾染（C1 复核）。
2. **关键事实修正：六 crate 从未真正发布 crates.io**。2026-11-01 实测 crates.io API 九名（sys/tls/netdproto/fsproto/fsclient/libc/mmap/signal/ui）全部 does-not-exist——真状态 = SDK-1b 交付 **publish-ready + CI 守卫**，最终六连 `cargo publish` = 挂账的**用户账号动作**（SDK-1b §0.2 红线 4）。「生态开放的第一个实物动作」尚未发生；八份 Cargo.toml `repository` 字段仍为占位址（github.com/berryos/berryos，404）。
3. **指南带一个会炸的漂移**：berry-libc 实依赖 berry-signal（**八 crate**，A7-4 段③a 起），指南 §9 依赖图与 §9.1 **七连** publish 手顺漏第八件——照文档发布会炸在 berry-libc 步（path 剥离后 version req 无解）；CI 守卫已是八 crate 口径，文档落后于代码。修复 = 指南 §9/§9.1 + 八份 README 栈图 + 维护合同挂「SDK crate 增删同步回写」义务。
4. **「第三方可达」今天不成立（三断点）**：① 获取面断——公开镜像 miuiadmin/BerryOS 停在 2026-10-24（早于 SDK-1a/1b），指南与 `--new/--check` 工具链不在镜像上，指南全文无 clone URL；② 运行环断——仓外 .bap 无自动接线位（justfile 硬编码条目表 + FAT 递送面 GUI-7 spike 未跑 + fat:rw 未落），第三方「能打包不能跑」；③ 发布面断——crates.io 全 404（见结论 2）。基建文档 12 缺口（高 3 / 中 6 / 低 3）全清单见 §三。
5. **berry-ui 4359 行 = GUI-3 冻结契约的完整可用实现，非空壳**（A2 精读 11 模块 + C2 直读全量 + C1 实跑复核）：**96 host 测全绿**；代码与 §6 契约 + §10 二十二条裁决逐字对齐，**零未记账漂移**；声明式形态 = Plain Data 值面（ViewKind 六件 + Props 枚举 + Msg-only 回调）+ **TLV AST 双入口**（encode_desc/apply_desc roundtrip 字节相等有测，DescBuilder 流构造器，「声明即数据」C2 裁决 day-one 兑现）。离 GUI-6 完整态差五件：网格 / TailwindCSS 原子语义 / tap-pan 竞争仲裁 / 缓动族（现仅 Linear+StepEnd 两档）/ SwipeView——全部未开工但地基（Drag Msg 面 / Transform 动画 / 增量 damage）已备。
6. **声明式布局终案（B2 推荐 × C2 六处修正）**：**案 1（约束单遍协商，Flutter 三档约束类 + SwiftUI proposes/chooses 语义）= 目标态**；**案 3（现状精修 + Grid 固定定轨）= GUI-6 v1 子集**——C2 修正关键认知：两案**非「同一模型两个完备度」而是两次立项**（案 1 在约束协议下后序测量+前序摆放必须合并为单趟协商 = layout.rs 内核 ~300 行重写，排期按重写非升级；十字轴默认必须钉 stretch 保 golden 兼容）。C2 终裁 **pass-with-changes**，六处修全并入 §四 终案。案 2（Taffy 形双遍 flexbox）确认结构性出局：Taffy 恒依赖 alloc（官方文档明示），与 berry-ui 零分配纪律冲突——2026-10-29「不引 Taffy」拍板再获一处硬证据。
7. **C2 揪出一个正确性级地雷 + 一个现成的免费优化**：B2 原案「relayout 边界 = Rect w/h 双 Some 截断祖先传播」**规则写错对象**——v1 全部容器消费子尺寸（child_agg Σ/max 聚合 + 游标取 c.meas_h/w），按字面实现要么 no-op（未变的节点走不到 mark_ancestors_dirty）要么漏排兄弟漏脏区；真缝**现成**：measure() 已算好的 mctx.changed 位（layout.rs:175 计算 :181 写入）是全库 **write-only 死数据**——「自身重测且尺寸不变 → child_affected 返 false 截断上行」纯增量、零冻结面改动、数据已在那只差消费。
8. **六盲区（五份产物共同漏掉，C1 补账，详见 §五）**：① **intent/AI 原生调度的 SDK 接口位断链**——berry_intent 六档枚举已在 ABI 冻结（options 低 8 位值域冻结）、berry-sys 已镜像，但 berry-libc pthread_create 无 intent 透传、guiproto SCENE/VOTE「帧率意图」与内核 berry_intent 是两套意图无归一映射——移动主战场核心卖点在开发者面断链；② QEMU TCG 时率（tick ≈ SLICE_HZ/2）+ 无 GPU 对全部帧率/时延结论的污染未标注；③ 多会话并行热文件互斥（建议动 ci.yml/指南/规划前先锁归属）；④ AI 生成闭环：DSL 校验器锁在 berry-ui crate 内（parse_node/encode_props 为 pub(crate) 冻结签名），AI 工具/LSP 无法脱离 crate 消费；DESC_MAX=4096 / MAX_NODES=256 双帽对 launcher 场景（240–360 节点、3–9KB）逼近/越帽；⑤ 应用分发/签名链零覆盖（无签名/验签/升级/商店议程，反噬 manifest v2 设计）；⑥ 无障碍语义树零覆盖（手机战场合规刚需；语义树同时是 AI 读屏/自动化测试的共同接口；GUI-6/7 前是低成本预留窗口，冻结后再补 = 再走一次 ADR+草案修订）。
9. **外部增量四件值得立即吸收**：① crates.io **Trusted Publishing 已 GA**（RFC 3691，OIDC 免 token，先手动发一次→UI 绑仓→CI 发布）→ 六 crate 发布链应接入，结构性消灭本地 token 泄漏面；② **Slint「四方共用同一解释器 + dev-only feature 门」**（live-preview/LSP/Viewer/VS Code 扩展同一解释器 crate；feature+环境变量双门控、官方明示勿写进 Cargo.toml）→ UI-SDK 先导刀（DSL+解释器+LSP/live preview）直接照抄的工程范式；③ **musl 1.2.5 口径基线**（Rust 2025-12-05 官方升 musl 目标，Rust 1.93 起生效）→ berry-libc 头注钉一行，但**先钉 rust-toolchain.toml**（仓根缺位，「同输入同字节」的编译器轴无文件级执法——B1 与 A1 均未指出此耦合，C1 接线）；④ cargo-generate 模板 = 与 mkapp --new 单源同模板数据，一小时级边际动作。
10. **方向总结论**：SDK 六层塔与 GUI 刀表拼成的开发者旅程链条完整、无方向性矛盾；闭合依赖四个 keystone——六 crate 真发布 / 公开镜像同步 / GUI-7 FAT 递送+仓外接线 / SDK-2 前置 manifest v2 词表 ADR。塔现状：L0 满配（唯 rust-toolchain 钉版缺）、L1 厚实（欠 IDL 单源生成 + timer 族 + 真 getpid）、L2 全塔最厚（13 件协议 crate 全 host 可测、黄金字节钉板）、**L3「有引擎无车身」**（berry-ui 零 guest 消费者，embedder = GUI-5 留位）、L4 仓内闭环（第三方不可达）、**L5 DFX/AI 面零开工**（SDK-1..5 未动，遥测只在 host 侧）。未见任何方向性返工迹象。

> 证据强度修正一则（C1 REFUTED）：B1 将 Zig issue 中「Redox 永不冻结 syscall ABI、稳定面 = relibc」表述为「官方明文/一手引述」——实测该 issue 原话出自 Zig 贡献者个人推断，非 Redox 维护者表态。结论方向未被推翻，但**进 ADR 叙事须另补 docs.redox-os.org 官方源**。

---

## 第二部分 · SDK 面盘点结论（A1 × C1 校验后）

### 2.1 六层塔逐层现状

| 层 | 现状 | 满配度 | 关键欠账 |
|---|---|---|---|
| L0 语言运行时 | Rust stable + aarch64-musl 全静态 + none target 预编译 core，零自维护运行时 | 满配极简 | rust-toolchain.toml 钉版文件缺位 |
| L1 系统库 | berry-sys（34 号 FFI 镜像，abi-check 代偿）+ berry-libc（musl std 全定义 shim，13 模块）+ tls/mmap/signal/poll/exec 底座 | 厚实 | IDL 单源生成；timer_create 族零符号；getpid/getuid 假值档 |
| L2 服务协议 crate | 13 件纯函数 crate（fs/net/blk/ps/gui/key 六域），全部 host 可测 + wire add-only + 黄金字节钉板 | 全塔最厚 | ksproto 零真消费者；guiproto hdi 域零消费（GUI-2 S1 挂账）；manifest 未入发布族 |
| L3 框架 | berry-fbcon（28 测）+ berry-gfx（111 测，pass 形态六件套）+ berry-ui（96 测，GUI-3 完整实现） | 有引擎无车身 | berry-ui 零 guest embedder（GUI-5）；无 VMO-backed 纹理/第二后端/真 blend；DSL 载体终裁待 SDK-1 后复核 |
| L4 工具链 | mkapp 三值链执法 + --new/--check 脚手架 + 八工具 --selftest + berryctl v0（ver/ps/mem） | 执法齐但仓内闭环 | 仓外工程构建不可达（path 依赖指向 checkout）；berryctl 无 --json 无 gui 子命令；独立链接器线未立项 |
| L5 DFX/AI 面 | 内核遥测环现役 + berry-ui telemetry.rs 双轨打点（消费者只有 host 断言） | 实质为零 | SDK-1..5 全未开工；guest 遥测收口通道未建 |

### 2.2 结构性缺口（按层，A1 gaps 摘要）

- **L2 发布缺口**：六 crate publish-ready 未发布（结论 2）——这是「生态开放第一个实物动作」的卡点，其余都可等。
- **L3 消费缺口**：GUI 应用端到端链断在 embedder（gui-wm = GUI-5）——L3 的塔尖要靠 GUI 线封顶，SDK 线与 GUI 线互为前置（裁决 H 的文件面互斥并行是对的）。
- **L5 零开工**：AI 四 verb / dump round-trip / 截图对拍 / FrameTimeline 四桶归因全挂 GUI-8/9 + SDK 线——「AI 友好化」支柱在 SDK 面的运行期闭环 2/10（A3 评估，构建期半环 6/10 现役）。
- **移动主战场专项**：capabilities 词表 v1 闭集仅 fs{rootfs}/net{client}——GUI 应用必需的 **display/input 能力词在 v2 目标态**，GUI-7 前 GUI 应用无法以「应用」形态持显示能力；manifest icon/label/views 字段（GUI-7）未落；gui-wm 窗口/焦点/输入法链零起步。

---

## 第三部分 · 基建文档审计（A3）

**走查结论**：以仓外第三方身份实测指南全链（macOS 实测 2.29s 出 ELF、325400B .bap、--check digests=ok）——仓内核心路径一次全通，指南质量高（诚实双态声明/字节级模板/契约八条/维护者手顺）；断点全在「第三方可达性」（结论 4 三断点）。

### 3.1 十二缺口（按优先级）

| # | 缺口 | 修法 | 优先级 |
|---|---|---|---|
| 1 | 公开镜像停 2026-10-24，指南/工具链不在镜像，无 clone URL | 跑 sync-public.sh 入 CI/发布流程；指南+README 写明镜像址；repository 占位换真址 | **高** |
| 2 | 指南 §9 七 crate vs 实八 crate，照文档 publish 会炸 | §9/§9.1 补 berry-signal；八 README 栈图同改；维护合同挂回写义务 | **高** |
| 3 | 仓外 app 无接线位，「能打包不能跑」 | GUI-7 FAT 盘 app/ 扫描 or justfile 注入位；**「仓外 app 不改 justfile 可跑」立为 GUI-7 验收线** | **高** |
| 4 | std/POSIX 支持矩阵不可见（真身 vs 桩） | 指南加「libc 面支持矩阵」节，数据单源从符号面生成（shim_audit.py 可承担） | 中 |
| 5 | 无 MSRV（rust-version）/CHANGELOG/semver 政策落点 | 八 Cargo.toml 统一 rust-version；首 publish 建 CHANGELOG；版本政策节 | 中 |
| 6 | API reference = 零（docs.rs 因未发布不可达） | 随 publish 自动解锁；镜像 CI 可选托管 rustdoc | 中 |
| 7 | 拒因一行中文无位置/错误码/机器双读结构 | mkapp/berry-manifest 拒因升三元组（行/列+期望+修法）；可前置不必等 SDK-4 | 中 |
| 8 | manifest 无独立 JSON Schema/llms.txt 导出 | gen_abi.py 或新工具导出；与 GUI-8 布局 JSON schema 同刀共享版本域纪律 | 中 |
| 9 | 「GUI 应用入门指南」无刀位归属 | 显式写进 GUI-7/SDK-2 验收线 | 中 |
| 10 | 索引阅读路径缺第三条（应用开发者路径） | 指南 → 应用模型 §3 → ABI 设计 §8 → appdemo | 低 |
| 11 | DSL 语法族两处口径不一（HTML 形态外部 DSL vs berryui 自家体系） | SDK-2 前置 ADR 钉死（与热重载矩阵/manifest v2 三件合并一 ADR） | 低 |
| 12 | 第三方 crate 移植无样例仓 | 维持挂账，SDK-2 时与 DSL 脚手架/金样语料合并单一 examples 面 | 低 |

### 3.2 AI 友好化在 SDK 面的落地度（A3 评估）

约**三成半**，轨道铺设正确、无方向性缺位：「机器可读 manifest/schema」6/10（执法在、schema 导出缺）；「结构化错误码」3/10（loud 但无码无位）；「生成→校验→回喂闭环」2/10（构建期校验在、回喂与运行时 verb 全挂账）——每条缺口都有已裁刀位兜底（SDK-4 三元组 / GUI-9 四 verb / ⑱ DSL schema+金样语料），**方向不缺、兑现靠刀序推进**。

---

## 第四部分 · 声明式布局构建终案（B2 推荐 × C2 六处修正）

> 本节 = 用户问「声明式布局怎么构建」的答案终稿。C2 对 B2 原案终裁 **pass-with-changes**（方向正确、证据锚真实、六处修入后可作 GUI-6 布局刀立项基座）。

### 4.1 路线裁定（两次立项，非一次的两档）

- **GUI-6 v1 = 案 3 修正版**（现状最小增量）：保留「后序内在测量（Σ/max 聚合）+ 前序游标摆放」两趟结构，加 Grid 固定定轨 + 定尺寸容器最小面 + Image 叶。零协议破坏、落地最快、确定性最强（OpenHarmony ui_lite grid 同路实证——lite 档仅 flex/grid/list 三种即表达桌面）。
- **目标态 = 案 1（约束单遍协商）立 layout 内核 v2 独立立项**：measure 升格「约束类（Tight/Loose/Infinite 三档枚举）+ 轴」入参，约束下行与尺寸上行合并单趟协商——**这是 layout.rs 内核 ~300 行重写非增量**（C2 修正④），relayout 签名外壳不变、十字轴默认钉 stretch 保 golden 兼容，golden 重钉单列验收项。修掉的真缺口：子测量不看父可用空间（Text/容器无法按容器宽收缩）。
- **案 2（Taffy 形）确认出局**：恒依赖 alloc（官方文档明示「Taffy always depends on the alloc library」）+ 双测放大 Run 相帧成本 + CSS Grid 轨道迭代定尺复杂度——保留价值仅轨道定尺与测量缓存的结构设计思想。

### 4.2 GUI-6 v1 范围盒（案 3 修正版，六件）

1. **Grid kind**：固定行列 + 等分轨（轨宽 = 可用宽/列数）；**auto/fr 轨硬性禁入 v1**（w3c www-style 2015Mar/0197：flexible 轨道 min-content 贡献需以 0 可用空间重入定尺算法——单遍性质会被破坏）。
2. **容器定尺寸最小面**：Col/Row 增 w/h `Option`（C2 修正⑥——v1 Rect 是叶不可为父、容器主轴恒内在，SwipeView 页无法定到 viewport 宽，缺此件 SwipeView 无最小表达面）。走 4.6 TLV 扩展纪律。
3. **Image/Icon 叶 kind**：新 kind 字节（ViewKind 现六件无图标叶；berry-gfx BYI1 解码 + LRU 图集已在段② 落地，正好消费）。
4. **十字轴对齐枚举**：start/center/stretch 三档（Tailwind items-* 语义最小面）。
5. **TextMetrics 缝 + monospace 首实现钉进 GUI-6 前段**（时序锚：先于 GUI-2 增件⑧ shape 解耦落地；C2 修正⑧——trait 参数用抽象档位 u8 scale id，**不把 FontSize 枚举焊进布局协议**，增件⑧ shape 产物注入时不再破签）。
6. **launcher 树规模实测提前到 GUI-6 立项时**（B2 已建议，C1 确认双帽逼近）。

### 4.3 增量失效升级路径（v1 视图级 → 属性级，修正版）

- **第一步（v1 现状，缝已到位）**：props_geom_equal（lib.rs:172）= 全库唯一变更分类点（几何/着色二分类，属性级 diff 的胚芽）；DepTable 边表 day-one 记录不消费（S7 spike 定生死：属性级收益 <20% 帧耗时 → 维持视图级至 v2，GUI-10 :123）。
- **第二步（S7 spike）**：props_geom_equal 返回值 bool → **change-mask 位集**（u8：W/H/gap/bg/fg/text/scale 位）；调用侧按 mask 分派——**三漏斗明示**（C2 修正⑤）：set_props（lib.rs:557）/ set_text（lib.rs:601）/ **anim::apply Layout 型分支（anim.rs:206-254——B2 原案漏的第三几何入口，直改 props+layout_dirty；不盖它 = 动画期间退回视图级失效，恰污染 S7 主度量场景）**。anim apply 改产 mask 走同一分派，或明示 scoped-out 并在 S7 度量协议剔除动画帧——二选一写死。
- **第三步（升级条件成熟时）**：键路径 diff（key u64 域 v1 已透传；flags 空位 bit1..7 之一 = patch op，按 key 定位 add/replace/remove，apply_tree 增量应用替代整树重建）+ §4.4 边界截断 + **S7 度量面必含 damage rect 计数**（属性级 mask 精确化直接降 rect 计数与 overdraw——因果链进 spike 验收）。
- **不跳步纪律**：三态停帧与「无 damage 不提交」不受升级影响——mask=0 即零 damage 即 IDLE 可达；frame.rs 真值表纯函数不变。

### 4.4 relayout 边界的正确实现（C2 修正①——删错误规则，用现成死数据）

- **删**：B2 原案「定尺寸节点（Rect w/h 双 Some 且未变）截断 mark_ancestors_dirty」——在父消费子尺寸的聚合语义下要么 no-op 要么漏排兄弟 + 脏区漏记（正确性级）；真边界只随案 1 Tight 约束档与 Grid 固定定轨产生，v1 现有六视图无一满足。
- **用**：**measure() 消费 mctx.changed 位**——layout.rs:175 已计算 :181 已写入的全库 write-only 死数据；自身重测且 meas 不变 → child_affected 返 false 截断上行。纯增量、不动冻结面，收益与原第⑥步目标相同（Run 相动画期 measured/placed 下降）。mark_ancestors_dirty 侧不动（保守预标 + measure 侧精确截断两层并存）。**先扩 tests/layout.rs 钉板「子重测尺寸不变 → 祖先 measured==0」**。

### 4.5 冻结面 ADR 清单（单支 ADR 一次过，C2 修正②）

GUI-6 开刀前出全量清单（B2 自认两项，实为六项）：① Node 字段（头注明文「冻结字段一字不改」）；② relayout 冻结签名（§6:315-316，约束类与 TextMetrics 都必须进参）；③ Props 族扩展连带 desc.rs encode_props/parse_props + DescBuilder；④ MAX_NODES + DESC_MAX + UiMain 别名；⑤ mark_ancestors_dirty 语义；⑥ TLV 版本域纪律（4.6）。纪律：**先写 host 钉板测试再动签名**（tests/layout.rs 11 测先扩后改）。

### 4.6 TLV 版本域纪律（C2 修正③——防静默错位）

AST 流无版本标记：新 kind/新 flags 位有 loud 拒面兜底（UnknownKind/UnknownFlags，旧码拒新流安全）；但**给既有变体加定长域会让旧解码器把新域字节读成 child_count 整体错位**——产出非确定 BadParent/Truncated 而非 loud 版本拒。.bap views/ 资源落地后跨版本流回放即坏。**纪律：Props 扩展只允许两条路——新 kind 字节，或 flags 新位门控的延长块；禁改既有变体定长**。tests/reconcile.rs 加「跨版本流拒面」样本（旧码读新流必 Err）。与 manifest 三值版本域（ADR #56）同构。

### 4.7 容量账（C2 修正⑥ + C1 盲区④）

- **MAX_NODES=256**：launcher 5 页×24 格×2-3 节点 ≈ 240–360 逼帽；升 512 则 **CAP→1024 连带** MeasCtx 双位图 2KB + old_abs `[Option<RectI>;1024]`=16KB **栈面须审计**。
- **DESC_MAX=4096**：120 图标 Text+容器 ≈ 3–9KB 逼近/越帽（AI 整屏生成恰是最先打爆双帽的入口，拒面只有一行 TooSmall）——**DESC_MAX 同刀同步升**，全并入 4.5 冻结面 ADR 一次过。
- DSL 校验器独立 crate 化（C1 盲区④）随 SDK-2 前置 ADR 同刀——见 §五/§六。

### 4.8 样式系统：词表闭集原子（B2，无 C2 反对）

TailwindCSS 原子语义 → Rust 落法 = **「词表闭集原子 id + Copy 位打包」**：每原子（w-16/gap-2/px-4/items-center/text-sm…）= 编译期已知 u16 词表 id，Props 内 packed u32/u64 位域承载。三消费面共享词表：Rust 侧 const 原子表 + builder 方法、文本 DSL 侧直接写原子名、AST 序列化侧词表 id 进 TLV。依据：零 alloc（无 String 类名）、可序列化（定长整数，与 ADR #56 词表闭集同构）、AI 友好（受约束生成面）。**词表闭集进 mkapp/ABI 单源执法，新增原子 = 显式版本域变更**（防 Tailwind 类名洪水重演）。对照形态不取：iced Theme+Catalog trait 太重；Dioxus CSS 文件绕开 rustc 验证。

### 4.9 DSL 载体：维持 C2 预裁 + 新证据加固

**文本 DSL 为主数据载体、Rust 宏仅作窄域糖层（只生成 TLV 字节流构造调用、无语句层），两载体共享同一 TLV AST 单源**；SDK-1 时用真实开发数据复裁（规划 :11 口径）。本轮新增加固证据：(a) 仓内 TLV 双入口已把「宏糖展开到同一 AST」的互操作面证明为零成本；(b) 原子词表闭集使文本 DSL 解析器极小且可独立出高诊断质量；(c) 宏税是结构性的短期无解（proc-macro 展开不进增量编译缓存 internals t21466、rust-analyzer 宏内补全缺口）；(d) Slint AI 工具链（LSP/live-preview/AI skill 共用解释器）是文本 DSL 独立工具链的业界最完备现成证据。**「两载体共享同一 AST」进 CI 断言**（roundtrip 对拍进 smoke 面，GUI-9 dump round-trip 同法）。

---

## 第五部分 · 六盲区（C1 补账，逐件给「该补什么」）

1. **intent/AI 原生调度的 SDK 接口位**（最重要）：ABI 已预埋（options 低 8 位 = berry_intent 值域冻结，六档枚举；berry-sys lib.rs:340 已镜像 CHAN_OPT_NO_INTENT），但 SDK 面零盘点——libc 无透传参数、guiproto SCENE/VOTE 与内核 berry_intent 两套意图无归一。**该补**：intent 预埋位 → SDK L1/L3 暴露面设计（pthread_attr/process options/channel opt-out 透传）+ VOTE↔berry_intent 归一 ADR + ADR #21 用户态策略带内接口对 app 可达性专项。
2. **QEMU 仿真面污染**：60/120fps ballot、8ms 重采样帽、2s IDLE、overdraw≤2.5× 等数字全部零 TCG 时率警告（tick≈SLICE_HZ/2 + 无 GPU）。**该补**：GUI 面基准接 bench-icount 确定性通道可行性研究 + TCG 时间税换算表 + 功耗断言退出 QEMU 的替代验收（或明示 v1 GUI 性能结论 host-only 边界声明）。
3. **多会话并行热文件互斥**：五份建议动同一批共享热文件（ci.yml / 指南 / 规划 / user/Cargo.toml）。**该补**：SDK 线/GUI 线共享热文件清单 + 每文件原子归属约定，纳入两线排刀前提。
4. **AI 生成闭环落点与容量帽**：DSL 校验器锁在 berry-ui 内（pub(crate)）；DESC_MAX/MAX_NODES 双帽 AI 整屏生成最先打爆。**该补**：校验器独立 crate 化（L2 协议位）+ 金样语料与对抗模糊语料测试面 + 双帽预算实测进 SDK-2/GUI-8 验收线。
5. **应用分发/签名链**：.bap 完整性 = SHA256SUMS（无签名无验签）；升级/回滚/卸载/商店零议程——反噬 SDK 设计本身（mkapp 签名位预留/appd 验签钩/manifest v2 security 域应同刀裁）。**该补**：分发与完整性链专项调研（对标 APK Signing v2/v3、Fuchsia pkg_resolver、鸿蒙 App 包格式）→ v1 预埋位清单 + manifest v2 security/distribution 域草案。
6. **无障碍语义树**：ViewKind/Props/TLV 均无语义域（label/role/焦点序）；语义树 = AI 读屏/自动化测试共同接口（对齐 AI 测试友好化支柱）；GUI-6/7 前是低成本预留窗口（冻结后再补 = 再走 ADR+草案修订）。**该补**：最小面 = TLV flags 空位或 add-only key/label 域（不破 roundtrip），对标 Android contentDescription/Compose semantics/Slint accessible-*，进 GUI-6 刀表或独立增件。

---

## 第六部分 · 拍板点七件 + 下刀建议

### 6.1 拍板落册（2026-11-01 用户全权委托，七件全裁取顶配）

| # | 拍板点 | 裁决（顶配版）与执行位 |
|---|---|---|
| ① | **八 crate 最终 publish** + Trusted Publishing + repository 换真址 | **立即发，全链接入**。准备链（本线执行）：八 Cargo.toml `repository` 换公开镜像真址 + `rust-version` 钉版 + 仓根 rust-toolchain.toml 钉 rustc（musl 1.2.5 口径前置耦合，C1 接线）+ 指南八 crate 口径修正；发布序 = ci.yml 八 crate 依赖序（sys→tls→netdproto→fsproto→mmap→signal→fsclient→libc）；首版手动发布（用户账号动作，手顺随准备链交付成一键）→ crates.io UI 绑 Trusted Publisher（RFC 3691）→ CI OIDC 发布弃本地 token；publish 后指南切「已发布态」+ docs.rs 自动解锁 |
| ② | **GUI-6 布局刀范围盒** | **按 §四 终案立项 + 两加强**：(a) 容量帽一次到位——MAX_NODES 256→**512**、CAP 512→**1024**、DESC_MAX 4096→**8192**，MeasCtx 双位图 2KB + old_abs 16KB 栈面审计进验收（免日后二次动冻结面）；(b) **案1 layout.rs v2 不设升级条件直接排刀**——GUI-6 v1 收官即接续开约束单遍协商重写，「目标态」从可选升为必达，golden 重钉单列验收项。冻结面单支 ADR（§4.5 六项）+ ⑤ 无障碍域随 GUI-6 开刀前出；与 GUI-2 段③ 文件面互斥并行 |
| ③ | **intent SDK 接口位专项**（盲区 1） | **立独立 ADR + 归一方向预裁 = 单一意图词汇表**——guiproto SCENE/VOTE 帧率意图折算映射进 berry_intent 六档语义域（帧率 ballot = berry_intent 图形场景特化），两套并存设终止期限；SDK 面：berry-libc pthread_attr intent 位 + process options 透传 + CHAN_OPT_NO_INTENT 暴露 + ADR #21 带内接口 app 可达性同 ADR 议程；与 SDK-2 前置 ADR 微刀同窗起草 |
| ④ | **AI 校验器独立 crate 化**（盲区 4） | **独立 crate + 单解释器铁律**——TLV 校验器/解释器从 berry-ui pub(crate) 迁出为 L2 协议位新 crate（定名随 SDK-2 前置 ADR）；立法「设备解释器 / live preview / LSP 三消费者共用同一 crate，禁止第二求值路径」（Slint 四方同解释器先例）；迁移走 ② 冻结面 ADR 同窗 |
| ⑤ | **无障碍语义树预留**（盲区 6） | **进 GUI-6 且不止最小面**——TLV flags 空位门控 add-only label/role 域（不破 roundtrip，跨版本流拒面同 §4.6 纪律）+ Node 侧语义位预留；v1 role 枚举只立值域不实现；GUI-6 范围盒第七件 |
| ⑥ | **分发/签名链预埋**（盲区 5） | **manifest v2 三域同刀裁**——词表域 + security（签名算法位/密钥指纹/profile）+ distribution（升级/回滚/来源）骨架一并进 v2 值域框架；细目待分发专项调研（APK Signing v2/v3、Fuchsia pkg_resolver/amber、鸿蒙 App 包格式）填充；v1 预埋 = mkapp 签名位留空 + appd 验签钩留位；归 SDK-2 前置 ADR 微刀 |
| ⑦ | **文档 hotfix 三件**（§3.1 #1/#2/#3） | **立即执行（拍板后第一刀）**——(a) 指南八 crate 口径 + clone URL + 镜像说明；(b) 规划 GUI-7 验收线加「仓外 app 不改 justfile 可跑」+ GUI-6 立项增件行回填；(c) 公开镜像同步 = 先审 sync-public.sh 过滤面（research/reference 必须在排除表）再跑，public 远端纪律不放松 |

### 6.2 下刀建议（时序）

- **即时（半天级，纯文档/CI）**：拍板点⑦ 三件 + Trusted Publishing 接入（拍板点① 前置）+ rust-toolchain.toml + berry-libc musl 1.2.5 口径行 + cargo-generate 模板。
- **近期（GUI 线并行窗口）**：GUI-2 段③（在途）∥ **GUI-6 布局刀**（§四 终案，文件面互斥）；**SDK-2 前置 ADR 微刀**（纯文档一刀合四件：热重载矩阵（Slint 三语义 + dev-only feature 门第四条）/ manifest v2 词表 + security 域（拍板点⑥）/ DSL 语法族归一 / 校验器 crate 化位（拍板点④））。
- **S7 spike（属性级生死门）**：mask 三漏斗 + mctx.changed 消费（§4.4 免费优化可先行单独落）+ 度量面含 damage rect 计数。
- **中期**：UI-SDK 先导刀（DSL schema + 单解释器 crate + LSP/live preview 共用）+ GUI-7（FAT 递送 + 仓外运行闭环验收线）+ SDK-3（帧时间线，借 Subsecond 显式同步点思想）/ SDK-4（AI 四 verb + 拒因三元组）。

---

## 附录（七份底稿全文）

- 附录 A = A1 SDK 面盘点（24 crate 逐件：层/API 面/成熟度/消费者）
- 附录 B = A2 声明式面盘点（berry-ui 11 模块逐件 + 契约对账）
- 附录 C = A3 基建文档审计（走查实录 + 12 缺口 + AI 友好化评估）
- 附录 D = B1 SDK 外部对标（六主张校验 + 九条新洞察 + 五建议）
- 附录 E = B2 布局深研（三案对比 + 推荐案 + 升级路径 + 载体对比 + 风险九条 + 源 48 项）
- 附录 F = C1 对抗交叉校验（13 verdicts + 六盲区）
- 附录 G = C2 布局可行性批判（八冲突 + pass-with-changes 终裁）

（附录全文见下方机器归档——与本报告正文一一对应，修正案以正文 §四 为准。）

---

## 附录 A · A1 SDK 面盘点（agent 原文归档）

```json
{
 "crates": [
  {
   "api_surface": "type Handle=u32/Rights=u64/Status=i32/Time=u64；berry_handle_close/duplicate/replace；berry_object_wait_one/wait_many/signal；berry_vmo_create/commit/read/write + berry_vmar_map/unmap/protect；berry_channel_create/send/recv；berry_process_create/start/exit/terminate；berry_thread_create/start/sleep；berry_clock_read + berry_signal_send/take",
   "layer": "L1 系统库",
   "maturity": "六发布族之一 publish-ready 未发布（crates.io API 2026-11-01 实测 404）；本体 0 host 测（none/musl 双 target 编译在册）；消费者 = berry-libc/berry-fsclient/berry-exec + svcmgr/bsh/hello/echo/talker/abuser/thrchild/ps/fsmgr/guifbd/berryctl",
   "name": "berry-sys",
   "notes": "头注自认欠债：IDL 单源生成（内核原型+镜像+syscalls.h 三合一）未做，改签名三处同步靠冻结纪律；质量面由 just abi-check（abi.toml 三面逐字节）代偿",
   "role": "raw syscall FFI 镜像（v1 ABI 合约源 docs/系统调用ABI设计-v1.0.md §8.3/§8.4，~34 号，永不 panic）"
  },
  {
   "api_surface": "crt0::_start（env 块 recv + argv + TPIDR_EL0 TLS 放置）；pub mod fs/io/net/pthread/signal/mmap/futex/sys/strmem/vprintf/stubs/alloc；fd3+ read/write/lseek/close(TRUNC-commit)；socket/connect/bind/listen/accept/send/recv + getaddrinfo(DNS RPC)；pthread_create/join + sem 三件 + key 族 64 槽；__errno_location/clock 族",
   "layer": "L1 系统库",
   "maturity": "六发布族之一 publish-ready 未发布；本体 0 host 测（libc 同名符号对 libSystem write 符号插入劫——纯面外迁 tls/mmap/signal 承接，实测教训 in berry-tls 头注）；消费者 = 全仓最大面：stdhello/bsh/uu 家族 20 成员/appdemo/appquiet/fail/find/grep/hexyl/netprobe/ps/sigchild/bench1/berryctl/fsmgr/guifbd",
   "name": "berry-libc",
   "notes": "timer_create 族零符号（A7-0 #10 深水后置）；getpid/getuid 诚实假值档；189 强未定义全定义 ⇒ musl libc.a 永不入选、零 musl 字节进产物",
   "role": "musl std 的 libc shim（crt0 自举 + 全定义符号面，G-eco-d）"
  },
  {
   "api_surface": "cpio_walk(&[u8])->CpioIter；cpio_step(window,pos,len)->StepOutcome；elf_parse(&[u8])->Result<ElfImage,_>；elf_head(head,file_len)->ElfHead；boot_parse(elf,phys_base,phys_top)->BootElf（bootloader 面）；page_floor/page_ceil",
   "layer": "L2 纯函数 crate 族（解析器即产品首例 D5）",
   "maturity": "62 host 测；未入发布族；消费者 = svcmgr（rootfs 拉起）+ bootloader/berryboot",
   "name": "berry-loader",
   "notes": "零依赖/零 unsafe/零分配，恶意与截断输入 host 全盖",
   "role": "cpio newc 流式 walker + ELF64 解析（微内核原则：装载逻辑在用户态）"
  },
  {
   "api_surface": "parse(&[u8])->Result<Manifest,ManifestErr>；struct SdkVer + SDK_CURRENT=0.1.0；struct Compat{min,target,compile}；Intent::from_word(&str)；struct Security{profile,key_lineage}（预留）；帽常量 MANIFEST_MAX=1024/BAP_MAX=4MiB/APP_SLOTS=4",
   "layer": "L2（schema 单源，跨 L2/L4）",
   "maturity": "34 host 测；未入发布族（SDK-1b §0.3 明示后刀扩）；双消费 = svcmgr appd arm + tools/mkapp.py（python 帽值对账锚）",
   "name": "berry-manifest",
   "notes": "分层执法裁定：parser 只验语法，min≤target≤compile 链比较归 mkapp、min>SDK_CURRENT 运行检查归 appd（SDK-1a §1.1）",
   "role": ".bap manifest v1 纯语法解析（ADR #43/#44 立 + #56 扩容）"
  },
  {
   "api_surface": "OP_PUT..OP_STATFS（10 op 常量）；encode_put/trunc/get/lookup/mkdir/unlink/readdir/stat/statfs(out,path,...)；parse_request(&[u8])->Option<Req>；encode_reply_get/status；ST_OK..ST_EXISTS 状态码；帽常量 CHUNK=192/NAME_MAX=48/PATH_MAX=128/FILE_SIZE_MAX=16KiB",
   "layer": "L2 服务协议 crate",
   "maturity": "六发布族之一（依赖叶子）publish-ready 未发布；34 host 测；消费者 = fsmgr（服务端）/berry-fsclient/svcmgr/bsh",
   "name": "berry-fsproto",
   "notes": "wire add-only 冻结（ADR #43）；严格串行 RPC 无请求号——多读者即应答偷取（结构性约束，SDK-1a §1.2 记录）",
   "role": "fs 服务 RPC wire 协议（10 op 编解码）"
  },
  {
   "api_surface": "FdTable::alloc/get/get_mut/take（fd 3..10）；AliasTable::alloc/base_of；SocketTable::alloc/get/take（fd 15..22）；EPOLL 表（fd 23..26）；st_to_err(u8)->FsErr；sock_domain_err(af,ty,proto)；rpc.rs 会话机（毒化位+90s/10s 双帽+EBUSY 阻塞重试，cfg(not(test))）",
   "layer": "L2（libc 支撑面）",
   "maturity": "六发布族之一（中顶层，path+version 双写三条）publish-ready 未发布；50 host 测（仓内最多之一）；消费者 = berry-libc fs.rs/net.rs（唯一）",
   "name": "berry-fsclient",
   "notes": "帽常量与 ST 码自 fsproto 单源引、netd ST/POLL 单源引——「不长第二套」纪律样本",
   "role": "fs RPC 会话机 + fd/socket/epoll 表纯函数面（libc 文件层与 socket 层底座）"
  },
  {
   "api_surface": "OP_READ/OP_WRITE；encode_request(out,op,sector:u64)；parse_request(buf)->Option<Req>；encode_reply(out,st)/parse_reply(buf)->Option<u8>；SECTOR_SZ=512",
   "layer": "L2 服务协议 crate",
   "maturity": "13 host 测；未入发布族；消费者 = vblk（服务端）/svcmgr/fsmgr",
   "name": "berry-blkproto",
   "notes": "vblk 经 path 依赖，A-5 起 FS 服务接棒消费",
   "role": "vblk 块设备服务 RPC 协议（P6 A-4）"
  },
  {
   "api_surface": "parse_bpb(boot)->Option<Bpb>；fat12_entry/fat12_set、fat16_entry/fat16_set；find_free_cluster/count_free_clusters；dir_entry(e)->Option<DirKind>；encode_file_entry/name_to_fat11/fat11_display",
   "layer": "L2（服务实现组件）",
   "maturity": "31 host 测 + FAT12/16 双 fixture 与 mkfat.py 跨实现共识（--check-fixture 门禁）；消费者 = fsmgr FAT 层",
   "name": "berry-fatfs",
   "notes": "FAT12 只读起步已扩 FAT16 与写面（fat1x_set/find_free_cluster）",
   "role": "FAT12/16 卷解析纯函数（BPB/FAT/目录项 + 写原语）"
  },
  {
   "api_surface": "Tree::put/trunc/read/mkdir/unlink/readdir/stat；root_names()->Vec<&[u8]>；Kind/ReadRes/ReadDirRes/StatRes；帽常量自 berry-fsproto 单源",
   "layer": "L2（服务实现组件）",
   "maturity": "18 host 测；未入发布族；消费者 = fsmgr RAM 写通道（唯一）",
   "name": "berry-fstree",
   "notes": "NUL 分段路径树，v1 平面表升级件",
   "role": "fsmgr RAM overlay 目录树"
  },
  {
   "api_surface": "encode_eth/parse_eth；encode_arp/parse_arp；encode_ipv4/parse_ipv4；encode_icmp_echo/parse_icmp_echo；Tcp/Ipv4/Udp 视图结构 + TcpFlags；checksum_rfc1071(&[u8])->u16；DNS 常量（DNS_HDR_SZ/DNS_QTYPE_A）",
   "layer": "L2 服务协议 crate",
   "maturity": "24 host 测；未入发布族；消费者 = vnet（virtio-net 用户态驱动）",
   "name": "berry-netproto",
   "notes": "零分配、输入只借用、输出写调用方切片",
   "role": "最小以太网/ARP/IPv4/ICMP/UDP/TCP/DNS 包编解码"
  },
  {
   "api_surface": "OP_DNS_QUERY/TCP_OPEN/ROUNDTRIP/CLOSE/LISTEN/ACCEPT/POLL/DRAIN（8 op）；encode_tcp_request/parse_tcp_request/parse_tcp_reply；encode_request/parse_request（DNS）；POLL_FLAG_WRITABLE/CLOSED；TCP_SESSIONS_MAX=8；ST_EAGAIN/NOSESSION/FULL/NODEV",
   "layer": "L2 服务协议 crate",
   "maturity": "六发布族之一（依赖叶子）publish-ready 未发布；42 host 测；消费者 = vnet（服务端）/svcmgr/bsh/berry-libc（经 fsclient 单源引）",
   "name": "berry-netdproto",
   "notes": "poll_revents/sock_rpc_errno wire 侧事实源——fsclient socket 面「不长第二套」锚",
   "role": "netd 网络服务 RPC（DNS + TCP 会话 8 op）"
  },
  {
   "api_surface": "Conn::open/listen/passive_open/send/close/on_seg/on_rto/expire_timewait；AcceptQueue::push/accept（容量 2）；State/SegFlags/TcpSeg/EvOut；RTO 常量（RTO_START_MS=3000/重试 3/TIME_WAIT 2s）",
   "layer": "L2（服务实现组件）",
   "maturity": "41 host 测；未入发布族；消费者 = vnet tcp.rs 会话循环",
   "name": "berry-tcp",
   "notes": "极小 MSS=128/WINDOW=512（QEMU virt 环境定制），非通用 TCP 栈",
   "role": "最小 TCP 客户端/监听状态机"
  },
  {
   "api_surface": "OP_PS_LIST/VER/MEM；encode_request_op(buf,op)；ReplyBuilder::new/push/finish；parse_reply(buf,len)；encode_ver_reply/parse_ver_reply；encode_mem_reply/parse_mem_reply；REPLY_MAX=566",
   "layer": "L2 服务协议 crate",
   "maturity": "15 host 测（黄金字节钉板）；未入发布族；消费者 = svcmgr（服务端）/ps/berryctl（客户端）",
   "name": "berry-psproto",
   "notes": "系统查询面的唯一 wire 源，berryctl v0 全部功能立在其上",
   "role": "ps 进程表查询 RPC（表/版本/内存三 op）"
  },
  {
   "api_surface": "OP_SUBMIT/FRAME_CALLBACK/RELEASE/VOTE/POWER_MODE（present 域 1..6）；OP_INPUT_SUBSCRIBE/INPUT_POLL（0x20..）；OP_REG_VBLANK/VBLANK_WAIT/PREPARE/COMMIT（hdi 域 0x40..）；SUBMIT_F_DAMAGE_FULL/UNSQUASHABLE；SCENE_NORMAL..IDLE 五档 + POWER_NORMAL..；PHASE_DOWN..HOVER_EXIT 七相 + RESAMPLE 常量",
   "layer": "L2 服务协议 crate",
   "maturity": "30 host 测；未入发布族；消费者 = gui-inputd/svcmgr/bsh + berry-ui（常量类型单源非收发）",
   "name": "berry-guiproto",
   "notes": "hdi 域仅编解码零消费者（GUI-2 S1 挂账）；GUI-4 段② 已接线 input 域（INPUT ok 46 相 + QMP 注入往返）",
   "role": "GUI wire 协议三域（present/input/hdi）编解码 + 帧率意图/场景/电源常量单源（ADR #53/C1）"
  },
  {
   "api_surface": "OP_KEY_GENERATE/KEY_ATTEST/COUNTER；SUB_COUNTER_READ/INCREMENT/CAS；SecurityLevel 三档；TAG_* 13 键 add-only 冻结表（KeyMint 借号段）+ tag_kind(u16)；KNOWN_TAGS/KNOWN_KINDS；CHAN_MSG_MAX=256；CounterBackend 单调计数器抽象",
   "layer": "L2 服务协议 crate",
   "maturity": "20 host 测；独立空 [workspace] 自成一体；零真消费者（keystored v2 软桩未实现）",
   "name": "berry-ksproto",
   "notes": "day-one 缝 = 冻结数据面非实现密钥库（零密码学算法/零依赖/零 unsafe）；SEC-02 产物",
   "role": "keystored 密钥服务 RPC wire（KeyMint 借 tag 空间 + 软硬双域 + 回滚抗性位）"
  },
  {
   "api_surface": "spawn_rootfs_elf(h_rootfs,&SpawnSpec)->Result<ChildHandles>；ChildHandles::reap；SpawnSpec{entry,args,stack_pages,gifts...}；cpio_find(h,name)->Option<(off,len)>；cpio_names(h,dir,cb)；channel_pair()->(Handle,Handle)；VIRTIO_MMIO_VA=0x7000_0000 等布局常量",
   "layer": "L1 系统库（服务侧拉起原语）",
   "maturity": "本体 0 host 测（guest 集成面验证）；未入发布族；消费者 = svcmgr/bsh/guifbd",
   "name": "berry-exec",
   "notes": "SpawnSpec::args 多参面为 uu cp/mv 扩过；无 fork（v1 冻结）下的进程拉起唯一通道",
   "role": "rootfs 程序拉起链（页铺平单 VMO 装载 + gift 装配）"
  },
  {
   "api_surface": "token_at_eol(line:&[u8])->Token；complete_from(prefix,candidates,add_space)->Completion；render_matches(out,&[Vec<u8>])；Token/Completion 枚举（唯一/多候选/零命中）",
   "layer": "L4 周边（shell 体验组件）",
   "maturity": "10 host 测；消费者 = bsh 编辑器（唯一）",
   "name": "berry-complete",
   "notes": "C-4 兑现 shell设计 §2.4 留档",
   "role": "bsh Tab 补全纯函数（token 切分/前缀匹配/列示）"
  },
  {
   "api_surface": "trait WaitMany（wait_many 时钟注入）；PollSet::new/with_slice/add/remove/wait(timeout_ms,now_ms)->Result<Vec<PollEvent>,i32>；PollEvent{handle,ready}；WAIT_MANY_MAX=16/POLL_SET_MAX=256；LT 语义 + 分片时钟",
   "layer": "L1 系统库（事件原语）",
   "maturity": "13 host 测（假 WaitMany 零 QEMU）；消费者 = bsh（唯一 guest 绑定）",
   "name": "berry-poll",
   "notes": "A2-1 产物，bsh 事件循环底座",
   "role": "多路等待聚合器（wait_many 批量轮询 LT 语义）"
  },
  {
   "api_surface": "valid_sig/capturable/installable；ActionTable::get/set；mask_bit/mask_has/apply_mask(how,old,arg)；set_empty/set_full/set_add/set_del/set_ismember（musl 128B sigset 位图层）；default_action(sig)->DefaultAction；plan_dispatch/classify(pending×mask×actions)->Act 步列表",
   "layer": "L1 系统库（libc 底座）",
   "maturity": "24 host 测；消费者 = berry-libc signal.rs（内核 0x86/0x87 两号只管跨进程置位，语义全在用户态）",
   "name": "berry-signal",
   "notes": "投递循环两遍走即刻落 held（位号序嵌套 bug 修复实录 in 沿革 2026-10-31 条）",
   "role": "POSIX 信号纯函数底座（sigaction 表+掩码状态机+投递分诊）"
  },
  {
   "api_surface": "classify(length,prot,flags,fd,offset)->Result<MapKind,Reject>；RegionTable::insert/find/take_exact/first_fit（底起洞复用）；round_up_page；WIN_BASE=0x5000_0000/WIN_SPAN=256MiB/VMO_MAX=32MiB/REGION_MAX=16；PROT_*/MAP_* 常量",
   "layer": "L1 系统库（libc 底座）",
   "maturity": "32 host 测；未入发布族；消费者 = berry-libc mmap.rs（唯一）",
   "name": "berry-mmap",
   "notes": "A7-1 裁定：匿名档 only，VA 窗与 pthread 栈区/MMIO 区全不撞",
   "role": "匿名 mmap 簿记（region 表 + VA 窗 first-fit 洞复用分配）"
  },
  {
   "api_surface": "tls_place(tls_mem,file_sz,mem_sz,align,...)（拷贝式放置算术）；find_ptls(ehdr)->Option<(vaddr,filesz,memsz,align)>；tls_template()->四全局；BERRY_TLS_VADDR/FILESZ/MEMSZ/ALIGN 四 static mut；TLS_ARENA_CAP=4096",
   "layer": "L1 系统库（libc 底座）",
   "maturity": "六发布族之一（依赖叶子）publish-ready 未发布；12 host 测 + tools/tlsscan.py python 镜像同向量共识；消费者 = berry-libc crt0.rs",
   "name": "berry-tls",
   "notes": "独立 crate 动因 = host 测试二进制符号插入劫（99% CPU 挂死 sample 栈实证）——纯面外迁范式首例；原位 TLS 方案崩因有法证专档",
   "role": "TLS 拷贝式放置纯函数（TP=data−0x10，musl std 全消费面）"
  },
  {
   "api_surface": "pub mod font/render；font::glyph(ch:u8)->Option<&'static [u8;16]>（95 字形 8×16，genfbfont.py 生成单源）；FONT 表常量 GLYPH_W=8/GLYPH_H=16；render::Fbcon（蜂格渲染核）",
   "layer": "L3 框架（图形基石）",
   "maturity": "28 host 测；未入发布族；消费者 = guifbd 显示服务 + berry-gfx（glyph 逐字节像素共识对拍单源）",
   "name": "berry-fbcon",
   "notes": "sliver/padding 铁律（dirty 区外不可触碰）的发源地，GUI-2 pass 边界的思想前身",
   "role": "framebuffer 控制台：自绘点阵字形 + 蜂格渲染（GUI-1 迁正 user/ members）"
  },
  {
   "api_surface": "CommandEncoder::record/fill_rect/draw_line/blit/draw_glyph_run/draw_image；Cmd 枚言 + DisplayList（记录）+ Queue::submit（回放）；PassDescriptor::new/clear/load（loadOp/storeOp/dirty rect）；Caps 能力查询；font::GlyphAtlas 三档字号（1x/2x/3x）+ icon::BYI1 解码 + lru 图集；Rect/Color/GlyphRun/ImageSrc",
   "layer": "L3 框架（GUI SDK 渲染面）",
   "maturity": "111 host 测（含与 fbcon 逐字节像素共识 + golden 回归）；未入发布族；消费者 = berry-ui + guifbd",
   "name": "berry-gfx",
   "notes": "ADR #51 六件套 day-one 全在场；留位 = VMO-backed 纹理、virtio-gpu 双后端、真 blend（A 位现仅 1-bit 透明键）、AA/3D",
   "role": "pass 形态图形 API + soft 光栅核（WebGPU 语义子集，GUI-2 段①②）"
  },
  {
   "api_surface": "Ui::<N>::new(viewport)/add(parent,ViewSpec)/remove/set_props/set_text；layout()->LayoutStat；damage()/drain_damage；paint(调用方 CommandEncoder)；render_frame(expected_present_ns)；pump_input->MsgBuf + push_pointer（手势 referee）；anim_add/clear + vote_hint()；set_vsync/note_presented（三态帧循环 RUN/SETTLE/IDLE）；encode_desc/apply_desc（AST 导出 round-trip——C2「声明即数据」硬要求）",
   "layer": "L3 框架（GUI SDK UI 面）",
   "maturity": "96 host 测（damage/frame/offscreen 验收三件）；未入发布族；无 guest 消费者（embedder = GUI-5 留位，host embedder = tests/ 离屏对拍）",
   "name": "berry-ui",
   "notes": "GUI-3 契约冻结（ADR #52/#53）+ GUI-4 段② 消费侧首刀立项中；MAX_NODES=256/OVERDRAW_BUDGET 2.5× 等帽常量全部冻结取值面",
   "role": "自研声明式 UI 引擎内核（保留树 arena + 布局 + 脏区合并 + 遮挡剔除 + 输入手势 referee + 动画 + 三态停帧帧循环 + 双轨遥测）"
  }
 ],
 "gaps": [
  "L0：rust-toolchain 钉版仍挂账（SDK-01 塔表「挂账维持」，实测仓根无 rust-toolchain.toml）——「同输入同字节」的编译器轴无文件级执法",
  "L1：berry-sys IDL 单源生成欠债（lib.rs:8-10 自认手写镜像，改签名须三处同步）；berry-libc timer_create 族零符号、getpid/getuid 假值档——依赖 POSIX timer/真进程 ID 的第三方库不可移植",
  "L2：六 crate 最终 cargo publish 未执行（crates.io API 2026-11-01 六名全 does-not-exist；SDK-1b 红线明示发布=用户动作+沿革挂账「最终 publish 六连」+repository 占位 github.com/berryos/berryos 未换真址）——「生态开放的第一个实物动作」停在 publish-ready；berry-ksproto 零真消费者（keystored v2 桩）；berry-guiproto hdi 域零消费（GUI-2 S1 挂账）；发布族未含 gfx/ui/manifest（后刀扩）",
  "L3：berry-ui 无 guest embedder（GUI-5 留位——L3「有引擎无车身」，GUI 应用端到端链断在 wm/接驳）；berry-gfx 无 VMO-backed 纹理、无第二后端（virtio-gpu）、无真 blend；DSL 载体最终裁定（文本为主已预裁 C2）待 SDK-1 后真实数据复核",
  "L4：仓外第三方工程无法脱离本仓构建（指南 §3.2 诚实声明 path 依赖指向 checkout，「仓外应用 build-user 接线位」沿革挂账）——crates.io 发布完成前 out-of-tree 只有 manifest/parser 可达，无完整 SDK 下载面；berryctl 仅 ver/ps/mem 三子命令、无 --json（AI 友好面挂 v2）、无 gui 子命令（挂 GUI-8/9）；SDK-01 表列名的 berry-ld/elf2flat 独立链接器线未立项（现状 rust-lld+每 bin 链接脚本内嵌）",
  "L5：整层零开工——SDK-1..5 全部未落（SDK-1 GUI 件挂 GUI-8/9：dump round-trip 注入/截图/宿主像素对拍/berryctl gui/.bap 脚手架残件；SDK-2 DSL+热重载挂 GUI-6；SDK-3 帧时间线；SDK-4 AI 四 verb；SDK-5 v2）；FrameTimeline 四桶归因与稳态零唤醒断言 = GUI-9 增件未落；遥测数据面现只在 berry-ui host 侧（telemetry.rs 消费者=tests），guest 收口通道未建",
  "移动主战场专项：capabilities 词表 v1 闭集仅 fs{rootfs}/net{client}（ADR #56 先落两词）——GUI 应用必需的 display/input 能力键在 v2 目标态（应用模型设计.md L141 display=true→surface VMO+frame channel 示例为 v2 下例），GUI-7 前 GUI 应用无法以「应用」形态持显示能力；manifest icon/label/views 字段（GUI-7）未落；gui-wm 窗口/焦点管理服务挂账——手机主战场的多窗口/焦点路由/输入法链零起步"
 ],
 "summary": "BerryOS user/ 侧现存 24 个 crate（user/Cargo.toml members 23 + berry-ksproto 独立空 workspace），全部围绕 SDK-01 六层塔（L0 语言运行时→L1 系统库→L2 服务协议 crate→L3 框架→L4 工具链→L5 DFX/AI 面，ADR #55）落位：L1 有 berry-sys（34 号 raw syscall 手写 FFI 镜像，abi-check 三面逐字节守卫）+ berry-libc（musl std 全定义 shim，13 模块 crt0/io/fs/net/pthread/signal/mmap/futex/sys/strmem/vprintf/stubs/alloc）及其纯函数底座族（tls/mmap/signal/poll/exec）；L2 为「解析器即产品」纯函数 crate 家族 13 件（fsproto/fsclient/blkproto/fatfs/fstree/netproto/netdproto/tcp/psproto/guiproto/ksproto/loader/manifest），全部 no_std 库本体 + host 测试放开 std；L3 = berry-fbcon/berry-gfx/berry-ui 三件已落地（GUI-1/2/3 收官，berry-ui 96 host 测但零 guest 消费者）；L4 = mkapp（三值链编译期执法 + SHA256SUMS + --new/--check 脚手架）+ berryctl v0（ver/ps/mem）+ berry-manifest python 对账锚；L5 全空（SDK-1..5 未开工，遥测只在 berry-ui host 侧双轨打点）。关键事实修正：任务前提称「六 crate 已上 crates.io」——实测（2026-11-01 crates.io API）六 crate 名全为 does-not-exist，真状态 = SDK-1b 已交付 publish-ready + CI 三档守卫（ci.yml sdk-publish-guard），最终六连 cargo publish 是挂账的用户账号动作（docs/SDK-1b实施设计草案-v1.0.md §0.2 红线 4 + docs/阶段沿革.md 2026-10-30 SDK-1b 条挂账）。全家族累计 ~740 个 host 测试（gfx 111/ui 96/loader 62/fsclient 50/netdproto 42/tcp 41），user/bins 22 bin + drivers/bins 4 bin 构成消费面。",
 "tower_per_layer": "L0 语言运行时：满配且极简——Rust stable + aarch64-unknown-linux-musl 全静态（免 nightly）+ none target 预编译 core，无任何自维护运行时；唯一挂账 = rust-toolchain.toml 钉版文件缺位（SDK-01 塔表明示「钉版挂账维持」）。L1 系统库：厚实满配——berry-sys（raw、永不 panic、abi.toml 三面守卫代测试）+ berry-libc（musl std 全定义 shim，std::thread/pthread/socket/DNS/signal/mmap 全通）+ 四件纯函数底座（tls/mmap/signal/poll）+ berry-exec 拉起链；六发布族中四件在此层（sys/tls/fsclient 归此系）；欠债 = IDL 单源生成、timer 族、真 getpid。L2 服务协议 crate：全塔最厚一层——13 件纯函数 crate 全部 host 可测（wire add-only + 严格串行 RPC + 黄金字节钉板），fs/net/blk/ps/gui/key 六域协议齐备且各有活的服务端-客户端对（ksproto 除外：只有冻结数据面无服务）；manifest 是 schema 单源（mkapp python 对账）。L3 框架：三件已落但「有引擎无车身」——berry-fbcon（28 测，guifbd 在跑）+ berry-gfx（111 测，六件套 day-one、soft 后端）+ berry-ui（96 测，GUI-3 契约冻结、声明式引擎内核完整：保留树/damage/遮挡剔除/手势 referee/三态停帧/AST round-trip encode_desc+apply_desc）；但 berry-ui 零 guest 消费者（embedder = GUI-5 留位），DSL 载体（文本 DSL 为主已预裁 C2）与热重载 = SDK-2 未开工；L3 全部不在 crates.io 发布族（符合「不稳定层静态链接随应用」设计，但第三方暂时拿不到）。L4 工具链：执法点齐但仓内闭环——mkapp 编译期三值链执法（hvigor 00303015 对应物）+ --new 脚手架 + --check 复验 + SHA256SUMS；mkcpio/mkfat/elf2pe/mkimage/size/bench/tlsscan 八工具全带 --selftest 门禁；berryctl v0 仅 ver/ps/mem（--json 挂 v2、gui 挂 GUI-8/9）；仓外工程 build-user 接线位挂账——第三方离仓构建尚不可达。L5 DFX/AI 面：实质为零——SDK-1..5 全未开工；现状仅有内核遥测环（现役）+ berry-ui telemetry.rs 双轨打点（FRAME_RING=64、expected/actual 双轨、jank/spinup/deadline_miss 计数器族，消费者只有 host 断言）+ guiproto vote/power_mode 协议位 day-one 留位；dump round-trip/截图/宿主像素对拍/FrameTimeline 四桶归因/零唤醒断言全部挂 GUI-8/9 与 SDK 线。",
 "verdict": "SDK 方向判定为正确，且已过「纸面立法」阶段进入可执法资产阶段。最强证据两条：① 质量地基成立——「解析器即产品」+ 纯函数 crate 化纪律使 24 crate 累计约 740 个 host 测试零 QEMU 依赖（berry-gfx 111/berry-ui 96/berry-loader 62/berry-fsclient 50/berry-netdproto 42，justfile test-host 逐 crate 显式行管），协议 wire 全部黄金字节钉板，SDK-01「协议 crate host 可测 = 体系质量地基」的论断已被兑现为回归资产而非口号；② 稳定性模型成立——ADR #55「稳定性靠执法工具不靠文档承诺」已物化为三道机器门禁：mkapp 三值链编译期执法（min≤target≤compile≤SDK_CURRENT，兼容问题结构性不进运行时，SDK-1a §1.1 分层执法表）、abi-check 三面逐字节比对、CI sdk-publish-guard 三档守卫——这是 AndroidX 版本纠缠的结构解的实做。风险不在方向在收官节奏：六 crate 的 crates.io 最终发布（生态开放第一实物动作）停在用户动作挂账（实测六名 404）、L5 DFX/AI 面零开工、berry-ui 无 embedder——L3/L5「塔尖未封顶」，且 GUI 应用必需的 display/input 能力词表在 v2 才回归，与 v1 移动主战场的 GUI 线（GUI-5..9）互为前置；未见任何方向性返工迹象，层间依赖方向（#43 铁律 + GUI 可拔插审计面）在 crate 依赖图上实际成立（GUI 族仅依赖 fbcon/gfx/guiproto，未反向沾 fs/net/kernel 面）。"
}
```

---

## 附录 B · A2 声明式面盘点（berry-ui）（agent 原文归档）

```json
{
 "contract_drift": [
  "总体判定:代码与 §6 冻结契约 + §10 实施对账(22 条裁决)完全一致,精读未发现未记账漂移——§6 全部常量(UiMain/MAX_NODES/TEXT_INLINE/MSG_MAX/ANIM_MAX/INPUT_BUF_MAX/DEP_EDGES_MAX/DRAG_SLOP_PX/OVERDRAW_BUDGET_X256/IDLE_AFTER_MS, lib.rs:52-61)、UiError 九变体(lib.rs:73-88)、Node 冻结字段(lib.rs:95-115)、Ui 35 个 pub 方法、desc/damage/layout/paint/hit/anim/frame/resample/dep/telemetry 全部 pub 签名逐字对齐;§10 裁决在代码均有对应注释锚(如 desc.rs:21-23 Text 73B 勘误、lib.rs:433-434 root 哨兵、lib.rs:560-575 kind 守卫)",
  "已裁决偏差(§10 在账,非活体漂移,如实列示):§3.2 散文 Text props ≤69B → 实算 73B(desc.rs:21 头注);root() 容量失败返 NodeId(u32::MAX) 哨兵而非 Result(§10 条 3, lib.rs:435);set_props/set_text kind 失配 → NodeBad 兼任(§10 条 4, lib.rs:563-575);CANCEL 分型 = 拖中 DragEnd+Cancel 双发/Click·Release·DragEnd 锚定 DOWN 时刻节点(§10 条 5, hit.rs:289-330);hit/node_at 排除 Root(§10 条 6, hit.rs:219);relayout freed 参数保留不消费(§10 条 10, layout.rs:66-67);RectSet 方法集以 §6 为准无 union_area/intersects(§10 条 11);动画死节点/prop 失配照常完成移除(§10 条 12, anim.rs:15-17);Gesture PTR_MAX=4 与 lost_msgs/lost_events 自定(§10 条 13, hit.rs:30)",
  "加性面(§6 契约外实现自有 API,§10 条 14a 已入册防「契约外漂移无账」):desc::DescBuilder(desc.rs:646)、DESC_MAX=4096/DESC_DEPTH_MAX=16(desc.rs:251-254)、RectSet::is_empty(damage.rs:509)、lib::DEFAULT_VSYNC_PERIOD_NS(lib.rs:65)、resample::target_from_expected(resample.rs:36);另有 frame.rs 四纯函数(next_vsync_tick:79/next_wakeup:101/Credits:133/ballot_for:199)与 FrameOutcome::skipped(frame.rs:177)系 §10 条 16/17 三 seam 收敛产物,均已入册",
  "实现超出 §3.5/§3.7 散文的语义细化(修复轮裁决,方向保守正确):anim Transform 型 damage 用「子树」新旧包络而非 §3.7 字面的节点自身界包络(anim.rs:117-120 防溢出子树残影);paint 超 ORDER_CAP=512 回退前序直发不丢画面(paint.rs:110-118,§3.5 未述的病态巨树防御);wake 条件化 FULL——空集才置,保留既有增量脏区(lib.rs:932-934,§3.8 字面「立即渲」的省电细化);damage add 满 8 时两两找最小包络可并对(§3.4 在契约内);culled 节点同计 damage 的保守口径(§10 条 10)",
  "唯一结构性重复(非漂移,实现内部观察):绝对界计算三处同口径平行实现——lib.rs abs_origin(lib.rs:343)、layout.rs snapshot(layout.rs:252)、anim.rs abs_rect/subtree_abs_rect(anim.rs:87/121),三处均含 ox/oy、i64 累计、环防御,因模块间只共享 nodes 切片不共享 Ui 而各自携带;语义一致但代码三份,未来重构候选",
  "§10 条 21/22 留白(有界挂账非漂移):damage add 多候选择优(≥2 可并对时小包络优先)无值级钉板(damage.rs:239-259 确定性引擎下无正确性差);apply_desc 深度帽靠 MAX_NODES=256 间接约束与 DescBuilder 的 DESC_DEPTH_MAX=17 显式执法不对称(desc.rs:550 注记)"
 ],
 "declarative_state": "desc.rs 的声明式形态 = 纯数据 struct + 双构造入口,非链式 builder、非宏:「声明即数据」三件套全落地。①值面 = Plain Data 族全 Copy + 值级 PartialEq:ViewKind 六件(Root/Rect/Text 叶 + Col/Row/Pad 容器,desc.rs:47-54)、Props 枚举五属体 + Root(desc.rs:99-106)、ViewSpec{kind,key,props}(desc.rs:111-115)、TextBuf 64B 内联零 alloc(desc.rs:121-146);属性全是 struct 字段/enum,无闭包无回调,Msg-only 交互(引擎产 MsgBuf 七变体 desc.rs:152-160,app update 消费)。②入口① builder 命令式:Ui::add(parent, ViewSpec) 按值传参(lib.rs:462-500)。③入口② 序列化双入口已落:AST 二进制 TLV 前序流(节点 := kind u8 | flags u8(bit0=has_key) | key u64 LE? | props 定长/变长 | child_count u8;Root 0B/Rect 11B/Text 6..73B/Col·Row·Pad 4B,desc.rs:5-23)——encode_desc/apply_desc 整树编解码(lib.rs:998-1016 委托 desc::encode_tree/apply_tree desc.rs:478/550)+ DescBuilder begin/end 流构造器(desc.rs:646-738),roundtrip 字节相等与双入口 encode 等价有测试钉板(tests/reconcile.rs 10 测);拒面全 loud(未知 kind/flags 位/截断/尾随/叶带子/Root 为子/文本超长,AstError 值级 desc.rs:214-232)。契约「描述结构可序列化双入口」落地度 = 完整;key(u64 调和键)已随 AST 透传但 apply_desc 是整树重建 + FULL damage,键路径增量 reconcile 挂账 v1.5/S7(desc.rs:108-109 注记)。文本 DSL(GUI-8)/宏糖(SDK 线)均编译到同一 AST,非第二事实源的路已通。",
 "gaps": [
  "GUI-3 自身挂账(§9):guest 真消费者接驳全缺——gui-wm(GUI-5:channel 会话×guiproto 编解码×fb VMO 映射×Present{damage} 驱动)与 gfxdemo/smoke 注册(GUI-9);引擎目前 host-only,embedder = tests/ 离屏",
  "属性级失效消费未接(dep.rs 只记录不消费,S7 spike 定生死:收益 <20% 帧耗时则维持视图级至 v2,dep.rs:7-8);reconcile 键路径 diff 未落(key 透传但 apply 整树重建)",
  "overdraw ≤2.5× 与 IDLE 唤醒 0/icount≈0 进 smoke 的能耗相(GUI-9);vote/power_mode 真消费(QEMU 单帧率 no-op);UI/合成错相真后端验证(S4 spike);credits 折叠实证(S6);per-surface 帧率意图(GUI-5+)",
  "clip/滚动视图(v1 无 clip,子命中与绘制不受父界裁剪——hit.rs:8 注记保守正确)、hover 三相(guiproto PHASE_HOVER 5-7 留位 pump 不产)、多点手势细化、长按(§9 挂账)",
  "GUI-6 布局件:网格(Grid)未到(现只有线性 Col/Row + Pad);TailwindCSS 原子类样式语义体系未到(v1 属性面仅 gap/all 单值 + Rect w/h/bg + TextStyle,无 padding 分向/margin/对齐/权重/换行);LayoutEngine 插拔缝未定形(layout.rs 硬编码内建算法,§3.3 明示归 GUI-6)",
  "GUI-6 手势件:tap/pan 竞争仲裁未到(现仅 slop=8px 单判据,无多手势 recognizer 竞争);手势输入短路径未落(pump_input 走全局 MsgBuf,未做事件→目标视图标脏不经全局调度绕行,规划 GUI-6 行 :92)",
  "GUI-6 缓动件:Easing 仅 Linear/StepEnd 两档(§6 冻结面即两档,anim.rs:41-44),缓动族(ease-out/弹簧等,GUI-0 后浮点可用)未扩",
  "GUI-6 SwipeView 整件未开工:松手吸附/边界回弹/指示器均无,ViewKind 六件无对应视图;但地基已就位——DragStart/DragMove/DragEnd Msg 面 + Transform 型动画 + damage 增量已可支撑其实现",
  "视图表达力边界(v1 设计取舍但影响 GUI-6 起步):Text 单行无换行(len×cell 单行)、无 alpha/blend(遮挡剔除恰因此成立,真 blend 随 gfx 真后端)、无 z-index/层叠、Rect w/h=None 的主轴折 meas 0 语义(§10 条 9)对弹性布局表达有约束",
  "两处有界留白(§10 条 21/22):damage add 多候选择优无值级钉板;apply_desc 深度帽靠 MAX_NODES 间接约束(敌意流真实入口在 gui-wm 侧届时补单源执法)"
 ],
 "interaction_state": "GUI-6 交互三件在 berry-ui 的现状 = 地基全在、三件本体未开工。①手势 referee:hit.rs 有 v1 完整单指状态机——DOWN→hit→Press、MOVE 距按下点 >slop 8px(i64 平方比较)→DragStart 此后 DragMove、UP→拖则 DragEnd/非拖则 Release+Click、CANCEL 终止不补 UP(拖中 DragEnd+Cancel 双发)、多指 PTR_MAX=4 槽独立(pointer_id 身份键)、Click 锚定 DOWN 时刻节点、Msg-only 产出(hit.rs:236-333);但 GUI-6 要求的「tap/pan 竞争仲裁」未到——现仅 slop 单判据无多 recognizer 竞争表,多点手势细化/长按/hover 三相挂账;「手势输入短路径(事件→目标视图标脏不经全局调度绕行)」未落,pump_input 走全局 MsgBuf 再由 app update 消费。②缓动动画:anim.rs 仅 Easing{Linear, StepEnd} 两档(anim.rs:41-44,§6 冻结即两档,定点整数 i128 插值),GUI-6 缓动族(ease-out/弹簧,GUI-0 后浮点可用默认定点)未扩;Transform/Layout 分型(GUI-3 面)已 day-one 落地且超契约细化(Transform damage=子树包络 anim.rs:195-204)。③SwipeView:完全未开工——ViewKind 六件无 swipe/翻页视图,松手吸附/边界回弹/指示器三语义全无;但其消费地基已就位:DragStart/DragMove/DragEnd(resampled 标注)Msg 面 + Transform 型 OffsetX/Y 动画 + 增量 damage,SwipeView 可纯建在其上(GUI-6)。",
 "layout_state": "layout.rs = 单遍自顶向下两阶段,增量 relayout 有、测量缓存有、失效传播粒度 = 视图级(节点级 dirty 位,非属性级)。①算法:后序测量(measure, layout.rs:141-185——内在尺寸 intrinsic:Root=viewport/Rect=定值 Option/Text=len×cell_w×cell_h 按 FontSize 档位单源(layout.rs:196-199)/Col=w 取 max·h=Σ+gap×(k-1)/Row 对称/Pad=子聚合+2×all,child_agg u64 累计 layout.rs:222-246)+ 前序摆放(place, layout.rs:304-359——final_size:Rect None 随父/Text 内在/Col 主轴内在十字轴吃满/Pad·Root 吃满分配 layout.rs:289-301;Col/Row 累进游标+gap、Pad 四向内缩、Root viewport 整给)。无网格、无 wrap 换行、无权重、无层叠、无多行文本。②增量 relayout:有——layout_dirty 位门控,无脏位返 LayoutStat{measured:0,placed:0} 全缓存命中(layout.rs:82-84);测量粒度 = 脏子树 + 受影响祖先聚合链(child_affected 传播,layout.rs:159-184);摆放粒度 = force(祖先分配语境变)∨自身重测,重摆节点新旧绝对界(含 ox/oy)均进 damage,旧界走改写前快照(layout.rs:101-105, 340-356)。③测量缓存:有——meas_w/meas_h per-node 内在尺寸缓存;改色 → props_geom_equal 判定(lib.rs:172-184)→ 0 重排,tests/layout.rs「measured==0」钉板。④失效传播:视图级(layout_dirty/paint_dirty per-node);set_props 几何变 → 旧界+父界 damage + 祖先链 dirty(lib.rs:577-598);属性级依赖 dep.rs 只记录不消费(S7 spike 定生死)。⑤插拔缝:LayoutEngine trait 未造(GUI-6 定形,§3.3/§9 明示挂账)——v1 算法硬编码于本模块;CAP=512 位图(MAP_NODES 两倍余量)超帽退化全量保守重排(layout.rs:33-35, 85-99)。",
 "modules": [
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/lib.rs",
   "key_apis": "Ui::new/add/remove/set_props/set_text/invalidate/layout/paint/push_pointer/pump_input/render_frame/wake/next_wakeup/vote_hint/note_presented/encode_desc/apply_desc/dep_bind;Node 冻结字段 16 件;alloc_slot/free_slot/slot_of 代数执法(lib.rs:229-263)",
   "role": "引擎门面 Ui<N>:arena(显式 free 栈 + u16 代数防悬空 NodeId = gen<<16|idx)、树构造 builder 入口、damage 收口(damage_rect 先 clip viewport)、输入缓冲 + MOVE 滑窗、动画表持有、render_frame 帧编排(anim tick→layout→paint→遥测→相位)、AST 双入口委托、dep 委托;全 &mut self 单线程无 Cell",
   "state": "可用,host 测覆盖(tree.rs 8 测:容量/代数/连子树/remove 旧界 damage/Root 幂等)"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/desc.rs",
   "key_apis": "ViewSpec/Props/TextBuf::new(超 64B loud);Msg 七变体(Press/Release/Click/DragStart/DragMove{resampled}/DragEnd/Cancel);encode_props/parse_node(pub(crate) 冻结签名);DescBuilder::begin/end/bytes(child_count 占位-回填,end 拒面先于状态变更)",
   "role": "数据驱动描述层:ViewKind 六件 + Props Plain Data 族 + ViewSpec + TextBuf 64B 内联 + Msg/MsgBuf(Msg-only 回调);AST TLV 二进制双入口(encode_tree/apply_tree/parse_node/encode_props) + DescBuilder begin/end 流构造器(DESC_MAX=4096/深度 16/子数 255 帽)",
   "state": "可用,host 测覆盖(reconcile.rs 10 测:roundtrip 字节相等/双入口等价/七类拒面)"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/damage.rs",
   "key_apis": "RectI::new(负 w/h 折零)/intersect/envelope/to_wire(负侧饱和)/from_wire;try_merge(u128 中间量,严格大于拒并,相交分形=L 形角点对拒并);DamageSet::add 四步流程;RectSet 满帽保守少记覆盖(多画不少画)",
   "role": "脏区引擎(验收①主体):RectI 内部 i32 几何(负 w/h 构造收口)、try_merge Σ×1.25 浪费帽(MERGE_WASTE 5/4, u128 防巨参溢出)、DamageSet K≤8 定容集(优先相交者并→满 8 两两找最小包络可并对→无可并 FULL 清集)、RectSet 不相交矩形集(subtract 挖洞 ≤4 片/is_covered Σ交面积==面积/visible 最大可见片)",
   "state": "可用,host 测覆盖(damage.rs 15 测:双侧边界/L 形值级拒并/K≤8 收敛/FULL no-op/RectSet 网格)"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/layout.rs",
   "key_apis": "relayout(nodes, vp, damage, freed)(freed 冻结签名保留不消费);measure 后序 + intrinsic/child_agg u64 聚合;final_size;place/place_children(Col/Row 游标+gap、Pad 2×inset、Root 整给);CAP=512 位图 + oversize 退化全量保守",
   "role": "v1 内建布局:单遍自顶向下——后序测量(内在尺寸缓存 meas_w/h + layout_dirty + child_affected 祖先聚合链传播)、旧绝对界快照(含 ox/oy)、前序摆放(force∨重测 才重摆,新旧界均进 damage)",
   "state": "可用,host 测覆盖(layout.rs 7 测:摆位钉板/Text len×cell/Measure 缓存 measured==0/set_viewport 全量);LayoutEngine 插拔缝未造(GUI-6 挂账)"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/paint.rs",
   "key_apis": "paint(nodes, root, vp, damage, scratch, enc);cull_walk(仅 Rect 叶计遮挡源,Text 蜂格不计);emit_walk_all 回退路径;take_scratch 零 unsafe 流式 split;present_rect(FULL→viewport/空集→零矩形)",
   "role": "遮挡剔除渲染(验收③主体):按 damage 矩形逆前序 cull_walk(前到后剔除,RectSet 已覆盖集,可见区空→culled++)、收集序之逆 = 前序发射(画家算法背到前);Rect→fill_rect、Text→GlyphPos run 组 scratch 子片→draw_glyph_run(style.bg 透传);尾部恒录 Present{damage 包络};overdraw_x256 = drawn_px×256/vp 面素;ORDER_CAP=512 溢出回退 emit_walk_all 前序直发不丢画面",
   "state": "可用,host 测覆盖(offscreen.rs 6 测:golden 逐字节对拍/增量 damage 恰该区/culled≥1/overdraw ≤640/守卫真伪自检/零扰动哨兵)"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/hit.rs",
   "key_apis": "node_at/hit_walk(topmost,Root 排除);Gesture{slots[PTR_MAX=4], lost_msgs, lost_events}/acquire/any_dragging;dispatch(per-pointer 状态机,slop i64 平方比较);PtrEvent::from_wire(PRESSURE_UNAVAILABLE→500)",
   "role": "命中检测 + 手势 referee:node_at topmost(兄弟链尾侧先探,命中含 ox/oy 视觉足迹,Root 不参与);Gesture per-pointer{down_node/down_xy/dragging}——DOWN→Press、MOVE 超 slop 8px→DragStart/DragMove、UP→Click|DragEnd、CANCEL 终止不补 UP(拖中 DragEnd+Cancel 双发);多指 pointer_id 身份键 PTR_MAX=4 满计 lost_events;pressure 0x8000 哨兵折 500(W3C 0.5)",
   "state": "可用(v1 单指 tap/drag 语义),host 测覆盖(input.rs 10 测:topmost/Click 全链/slop 三段/CANCEL/双指独立/缓冲满 loud);tap/pan 竞争仲裁未到(GUI-6)"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/anim.rs",
   "key_apis": "tick(anims, nodes, damage, now)→anim_active;apply 分型落点;eval_value(Linear i128 插值/StepEnd 持 from);abs_rect/subtree_abs_rect(环防御,异常保守并入自身面);damage_envelope",
   "role": "动画表推进:AnimProp 分型——OffsetX/OffsetY = Transform 型(落 ox/oy paint 偏移不重排,damage=新旧子树包络防尾影残像素)、Width/Height/Gap = Layout 型(改 props + layout_dirty,旧界 damage 新界随 relayout);Easing{Linear,StepEnd} 两档定点;i128 插值中间量;终点精确 to 落位;死节点/prop-kind 失配照常完成移除(防 anim_active 卡死永不停帧)",
   "state": "可用,host 测覆盖(anim.rs 9 测:Transform measured==0/Layout 触发重排/终点钉板/StepEnd/表满 loud/anim_active 翻转);缓动族(>2 档)归 GUI-6"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/frame.rs",
   "key_apis": "next_phase(真值表与 cur 无关,强降只裁静默);next_vsync_tick(mod 归一容忍敌意 phase);next_wakeup(Run=min 三源/Settle=闹钟);Credits{submit/on_presented};ballot_for(idle 全 0+sticky=1 停帧粘滞/drag {60,120,120}/animated {30,120,60}/normal {0,60,60})",
   "role": "三态停帧纯函数状态机(验收②主体)+ 软 vsync + credits 背压 + vote 票面:next_phase 真值表(任一信号→Run/静默>2000ms 强降 Idle/present_pending→Settle/其余 Idle);next_vsync_tick 拍沿含沿不欠拍;next_wakeup request-driven(Idle→None 撤定时器);Credits ≤CREDITS_MAX=2(显示服务报剩余信用为真值源);ballot_for 四档定值票(产票不仲裁);FrameOutcome::skipped 跳帧诚实填 0",
   "state": "可用,host 测覆盖(frame.rs 15 测:真值表全格/IDLE None/wake 不欠拍/强降/SETTLE 收口/jank/credits/vote);三 seam 收敛单源完成(§10 条 17)"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/telemetry.rs",
   "key_apis": "Telemetry::record/bump_wakes/bump_idle_exits/bump_spinup_late/fill_presented(pub(crate) 单点喂入);last(n)/frames/wakes/idle_exits/spinup_late_frames/deadline_miss(pub 读面)",
   "role": "双轨打点(P8):FrameRec 12 字段(expected_ns/actual_ns 双轨,actual 由 present 应答回填)+ 环 64 覆写最旧 + 计数器族(frames/wakes/idle_exits/spinup_late_frames/deadline_miss);判定逻辑单一事实源在 lib.rs(jank=actual>expected+period/2、spinup 锚),本模块只落账零策略;一份数据喂三消费者(host 断言/性能工具/AI 遥测)",
   "state": "可用,host 测覆盖(frame.rs 内遥测断言 + 环回卷/迟到应答 no-op 专测)"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/resample.rs",
   "key_apis": "resample_to_target(hist, target)(常量 berry_guiproto 单源 5/2/20/8ms 不长第二套);target_from_expected(饱和减);lerp_i64/extrap_i64(i128 中间量 clamp 回 i64)",
   "role": "消费侧重采样最终一跳:纯函数 resample_to_target——<2 采样/非严格递增预检/跨度 <2ms 或 >20ms → None;target 在界内括点线性插值;超尾前向外推帽 min(跨度/2, 8ms) 帽沿截停、ts 诚实报 target;无后向外推;撞 ts 取新采样;target = expected_present − RESAMPLE_LATENCY_MS(Android 同式,target_from_expected 单源,pump_input 唯一消费)",
   "state": "可用,host 测覆盖(resample.rs 8 测:拒面/插值精确/外推帽沿/撞 ts)"
  },
  {
   "file": "/Users/murongcheng/Documents/code/berryos/.claude/worktrees/gui3/user/berry-ui/src/dep.rs",
   "key_apis": "DepTable::bind(同边幂等 Ok 不占槽,满帽 DepFull)/dependents(返回真实总数,调用方判截断)/unbind_node/clear;lib.rs dep_bind 入口做 liveness 校验 + remove 先解绑后抬 gen(lib.rs:526-535)",
   "role": "属性级依赖记录骨架(GUI-10 增件①):DepId(pub u32) app 状态属性句柄 + DepTable 定容 64 边(Option<Edge>,线性扫明示);v1 记录不消费——失效仍视图级 invalidate(裁决不跳步,S7 spike 定生死:属性级收益 <20% 帧耗时则维持视图级至 v2)",
   "state": "可用(缝已 day-one 且被 Ui 字段级消费——bind/unbind/clear 全接通,§10 条 18 收敛),host 测覆盖(dep.rs 8 测:记录查询/满帽 loud/不影响失效钉板)"
  }
 ],
 "summary": "berry-ui 4359 行 11 模块非空壳——GUI-3 §6 冻结契约的完整可用实现:保留树 arena(gen 代数防悬空)+ AST TLV 双入口(「描述结构可序列化双入口」day-one 落地,roundtrip 有测)+ Σ×1.25 脏区合并(u128 防溢出)+ 遮挡剔除 paint(前到后剔除/后到前发射/overdraw ≤640 断言)+ 输入手势 referee v1(tap/drag/slop,CANCEL 不补 UP)+ Transform/Layout 分型动画 + RUN/SETTLE/IDLE 三态停帧(request-driven,Idle→next_wakeup()==None)+ expected/actual 双轨遥测 + 消费侧重采样(guiproto 常量单源)。代码与 §6 + §10 实施对账(22 条裁决)逐条一致,未发现未记账漂移;本地实跑 96 host 测全绿(10 测试文件,damage 15/frame 15/input 10/reconcile 10/anim 9/dep 8/tree 8/resample 8/layout 7/offscreen 6)。纯 host crate(no_std、零 alloc、零 panic、单线程 &mut),双依赖仅取 berry-gfx 渲染面 + berry-guiproto 常量类型单源,wire/channel/VMO 全留 embedder(GUI-5)。离 GUI-6 交互三件(网格布局/TailwindCSS 原子语义/tap-pan 竞争仲裁/缓动族/SwipeView)均未开工——现仅有线性 Col/Row/Pad、Easing 两档、slop 单判据手势;地基(DirtyMsg 面/Transform 动画/增量 damage)已可为 GUI-6 复用。"
}
```

---

## 附录 C · A3 基建文档审计（agent 原文归档）

```json
{
 "ai_friendly_assessment": "三原则在 SDK 面落地约三成半,但轨道铺设正确、无方向性缺位。已落(构建期执法半环现役,约 6/10):①manifest 机器可读 + 规范形逐字节门禁 + --check/--selftest(9正22负,docs/索引.md:80)——『生成→校验』半环今天可用,AI 生成 manifest 可被机器确定性判对错;②abi.toml 单源三面 + 逐字节守卫(syscall 面对 AI 可读可推断,SDK-01 L1 层);③berry-ui desc.rs AST 二进制 TLV 双入口 + roundtrip 字节相等(user/berry-ui/src/desc.rs:1-30)——『声明即数据』day-one 兑现,AI 生成 UI 的目标格式已具形且可断言;④协议 crate 家族(guiproto/fsproto 等)零分配纯函数 + host 测,wire 面机器可断言;⑤smoke 标记体系 = AI 测试友好化支柱的现役展示位。未落:①校验错误无人/机双读结构——拒因实测一行中文无位置无错误码,SDK-01 硬结论 12 的『位置+期望+修法』三元组是 SDK-4 目标,当前 mkapp 门禁尚未达标;②manifest/DSL 无独立 JSON Schema/llms.txt 导出(SDK-01 Q4 的 abi-gen 第四面建议未落),schema 事实源是 Rust+Python 双镜像代码而非数据文件;③运行时四 verb(dump/inject/screenshot/reload)零落地(挂 GUI-9/SDK-4),『回喂修复』半环(TypeChat 式失败信息回喂再生成)无任何现役位——AI 今天生成 app 后只能靠 mkapp 拒因一行字符串自行迭代,闭环缺『喂回』一腿;④berryctl --json 推迟 v2(user/bins/berryctl/src/main.rs:5)。总评:『机器可读 manifest/schema』6/10(执法在、schema 导出缺),『结构化错误码』3/10(loud 但无码无位),『生成→校验→回喂闭环』2/10(构建期校验在、回喂与运行时 verb 全挂账)——每条缺口都有已裁刀位兜底(SDK-4 三元组/GUI-9 四 verb/⑱ DSL schema+金样语料+官方 AI skill 文件三件套,docs/GUI渲染调研-2026-10-v1.0.md:51),方向不缺、兑现靠刀序推进。",
 "direction_verdict": "基建文档与开发方向总体是对的:应用开发指南是 SDK-01 硬结论 13『开发者面收口』的忠实兑现——不假设读者读过仓内文档、诚实双态声明、字节级钉板模板、契约八条、维护者手顺俱全,且本次实测仓内核心链路(脚手架→musl 咒语→mkapp 打包→--check 复验)一次全通,SDK-1a/1b 把 SDK-1 的 GUI 无关件提前落地也是正确的解耦。方向咬合上,SDK-01 六层塔与 GUI 刀表能拼成一条完整开发者旅程且分工无矛盾(GUI-7=manifest 资源位/icon、GUI-8=布局 JSON schema 版本域、GUI-9+SDK-1 余量=DFX 三件+截图+berryctl gui、SDK-2+⑱=DSL/解释器/LSP/live preview 框架层、SDK-2 前置 ADR=manifest v2 词表与热重载治理缝、SDK-3/4=遥测与 AI 闭环),但今日断环有四:①获取面断——公开镜像停在 2026-10-24,指南与 SDK-1a 工具链不在镜像上,指南也没给 clone URL,『仓外读者』前提目前不成立;②运行环断——仓外应用无自动接线位,FAT 递送面(GUI-7 spike)与 fat:rw(开发体验 L2)两个 keystone 双双未落,第三方『能打包不能跑』;③GUI 应用旅程零起点——views/、display/input 词、DSL、预览器全在途,属计划内而非失焦,但『GUI 应用入门指南』未显式归属到任何刀的验收线;④文档漂移一处——代码已是八 crate(CI 守卫同步了),指南与 README 仍停在六/七 crate 口径,照文档 publish 会炸。建议:把『仓外 app 端到端可跑(不改 justfile)』立为 GUI-7 验收线之一(防 FAT spike 触 kill 线退 rootfs-only 后旅程再断)、公开镜像同步进 CI 或发布流程、维护合同挂『SDK crate 增删同步回写指南+README』一条——三件都低成本,做完后旅程即从『仓内自洽』升为『真第三方可达』。",
 "gaps": [
  {
   "area": "获取面/镜像同步",
   "fix": "跑 tools/sync-public.sh 同步镜像并纳入 CI 或发布流程;指南 §2 与 README 写明公开镜像 clone 地址;按 §9.1 自警把八份 Cargo.toml repository 占位换真地址",
   "problem": "公开镜像 github.com/miuiadmin/BerryOS 停在 2026-10-24,docs/ 无应用开发指南、tools/mkapp.py 无 --new/--check;指南全文无 clone URL——『仓外读者』前提今天不成立",
   "priority": "高"
  },
  {
   "area": "指南 §9 依赖图过时(八 crate vs 七 crate)",
   "fix": "指南 §9/§9.1 补 berry-signal 成八 crate 口径,八份 README 栈图同改;维护合同挂『SDK crate 增删同步回写指南+README』义务",
   "problem": "berry-libc 已依赖 berry-signal(user/berry-libc/Cargo.toml,CI 守卫 ci.yml:115 亦为八 crate),指南 §9 与 §9.1 七连 publish 手顺漏 berry-signal,照文档发布会炸;七份 README 栈图更停在六 crate(缺 mmap+signal)",
   "priority": "高"
  },
  {
   "area": "运行闭环(out-of-tree 接线)",
   "fix": "优先落 GUI-7 FAT 盘 app/ 扫描,或 justfile 增 out-of-tree app 注入目录/变量;建议把『仓外 app 不改 justfile 可跑进系统』写成 GUI-7 验收线之一",
   "problem": "仓外 .bap 无自动接线位(justfile:229-230 条目表硬编码),FAT 递送面(GUI-7 spike 未跑)与 fat:rw(开发体验 L2 未落,docs/索引.md:139)双缺,第三方『能打包不能跑』,迭代须整 rootfs 重建",
   "priority": "高"
  },
  {
   "area": "std/POSIX 支持矩阵",
   "fix": "指南加『libc 面支持矩阵』一节(真身/桩两列),数据单源从 berry-libc 符号面生成(tools/shim_audit.py 可承担),随 crate 增删再生",
   "problem": "第三方无法从指南得知哪些 std/POSIX 面可用(pthread/timer/fs/信号/mmap 真身 vs socket/fork/exec ENOSYS),须读 zh-CN 内部设计文档或试错;berry-libc README 只有一句概括",
   "priority": "中"
  },
  {
   "area": "MSRV/CHANGELOG/semver 政策",
   "fix": "八份 Cargo.toml 统一加 rust-version;随首次 publish 建 CHANGELOG.md;指南或独立 SDK 版本政策节写明抬级联动流程(§9.1 末行已预留此挂账)",
   "problem": "八 crate 无 rust-version 字段、无 CHANGELOG;版本纪律仅指南 §9.1 一句(『0.x 阶段 minor 即 breaking』)+ADR #57,无机器可查落点、无发布节奏声明",
   "priority": "中"
  },
  {
   "area": "API reference/docs.rs 覆盖",
   "fix": "随 §9.1 publish 自动解锁 docs.rs;可选在镜像 CI 加 cargo doc 产物托管;指南 §7 契约清单与 rustdoc 互链",
   "problem": "crates.io 未发布 → docs.rs 覆盖为零;仓内无托管 rustdoc;[package.metadata.docs.rs] targets 已备(berry-libc Cargo.toml)但无处兑现;第三方 API 面今天只有读源码 doc 注释一条路",
   "priority": "中"
  },
  {
   "area": "错误信息可修性(位置+期望+修法)",
   "fix": "mkapp/berry-manifest 拒因升三元组(行/列+期望+修法建议),成功行的 key=value 风格反向应用到拒因;量小可前置到当前刀,不必等 SDK-4",
   "problem": "mkapp/berry-manifest 拒因实测一行中文字符串,无行号位置、无错误码、无机器双读结构;成功行反而是半结构化(entries=3 digests=ok)——AI 友好三原则在 SDK 面最显性短板",
   "priority": "中"
  },
  {
   "area": "机读 schema 导出(llms.txt/JSON Schema 第四面)",
   "fix": "gen_abi.py 或新工具导出 manifest JSON Schema + SDK 面 llms.txt 入口;可与 GUI-8 布局 JSON schema(AI 生成面第一个 schema 消费者)同刀做,共享版本域纪律",
   "problem": "manifest v1 的『schema』= Rust 解析器 + Python 门禁双镜像(靠测试钉板对拍),无独立 JSON Schema/语法文件可供 AI 工具与校验器直接消费;SDK-01 Q4 建议的 abi-gen 第四面(llms.txt + JSON Schema)未落",
   "priority": "中"
  },
  {
   "area": "GUI 应用开发入门指南归属",
   "fix": "把『应用开发指南 GUI 篇』显式写进 GUI-7/SDK-2 验收线(指南头部已承诺随抬级同步更新,归属钉住防漏)",
   "problem": "GUI 线机制件在途(GUI-2 段③/GUI-3/GUI-4 段②已落),但 manifest 无 icon/label/views(GUI-7 未落)、berry-ui 无 guest 消费者(GUI-5 留位)、无 display/input 能力词(manifest v2 词表 ADR 未立)——GUI 应用旅程为零属计划内,但无一份 GUI 入门文档归属到任何刀的验收线",
   "priority": "中"
  },
  {
   "area": "阅读路径第三条(应用开发者路径)",
   "fix": "docs/索引.md 阅读路径节补第三条:应用开发指南 → 应用模型设计 §3 → 系统调用ABI设计 §8 → user/bins/appdemo",
   "problem": "docs/索引.md 阅读路径仍只两条(AI agent 八步/人类新人八步),SDK-01 §五 SDK-1 验收线的『应用开发者路径』未兑现(指南已入表但无路径导览)",
   "priority": "低"
  },
  {
   "area": "DSL 语法族 + SDK-2 前置 ADR 未钉",
   "fix": "SDK-2/⑱ 刀前一次 ADR 钉死:DSL 语法族 + 热重载状态保持矩阵 + manifest v2 词表与 capabilities 扩键(SDK-01 §五建议三件合并)",
   "problem": "C2 已裁『文本 DSL 为主』(ADR #52),但语法族两处口径不一:GUI渲染调研 v2 = HTML 形态外部 DSL(docs/GUI渲染调研-2026-10-v1.0.md:51) vs ADR #52 = berryui 自家体系;SDK-2 前置 ADR 微刀(热重载状态保持矩阵 + AST 中立性 + manifest v2 词表合并)均未落",
   "priority": "低"
  },
  {
   "area": "移植模板/第三方 crate 样例仓",
   "fix": "维持挂账,SDK-2 时评估与 DSL 脚手架/金样语料库(GUI渲染调研 AI 友好三件套)合并成单一 examples 面",
   "problem": "优化议程 2.1『cargo generate 移植模板 + 移植指南』挂账 P4末–P5(docs/优化议程-v1.0.md:33);应用面已有 mkapp --new 足够,但第三方 crate 移植(hexyl 式)无样例仓/依赖面清单",
   "priority": "低"
  }
 ],
 "summary": "以仓外第三方 Rust 开发者身份实测 docs/应用开发指南.md 全链:仓内核心路径(§3 脚手架 mkapp --new → §4 musl 构建咒语 → §5 打包/--check 复验)一次全通(macOS 实测 2.29s 出 ELF、325400B .bap、--check digests=ok),指南质量高——诚实双态声明、字节级模板、契约八条、维护者手顺俱全,SDK-01 硬结论 13 的四件收口基本兑现。但「第三方可达性」存在一个硬断点:公开镜像 github.com/miuiadmin/BerryOS 停在 2026-10-24(早于 SDK-1a/1b),应用开发指南不在镜像 docs/(raw 404)、镜像 mkapp.py 无 --new/--check——真实第三方今天既拿不到指南也拿不到 SDK-1a 工具链,且指南全文无 clone URL。次级断点:①指南 §9 依赖图过时(berry-libc 实依赖 8 crate 含 berry-signal,指南与七连 publish 手顺仍写七件,照文档发布会炸——CI 守卫已是八 crate 口径);②§8 自认的仓外应用无接线位(须改 justfile 硬编码条目表),FAT 递送面与 fat:rw 双未落,第三方「能打包不能跑」;③crates.io 九名实测全 404(未发布态,指南已诚实声明)。方向面:SDK-01 六层塔与 GUI-7/8/9+SDK-1 余量+SDK-2+⑱ UI-SDK 先导刀拼成的开发者旅程链条完整无方向性矛盾,但闭合依赖两个未落 keystone(GUI-7 FAT 递送 spike、SDK-2 前置 manifest v2 词表 ADR)。AI 友好化:构建期执法半环已现役(manifest 机器可读+规范形逐字节+--check/--selftest+berrys-ui desc.rs AST TLV 双入口),运行期闭环(四 verb/JSON Schema 导出/LLM 可修错误三元组)全挂 SDK-4/⑱,约落三成半。",
 "walkthrough_blockers": [
  "步骤0(获取指南与 checkout):指南只存在于私密仓——公开镜像 miuiadmin/BerryOS pushed_at=2026-10-24(早于 SDK-1a/1b 的 10-30),镜像 docs/ 仅 30 份无应用开发指南.md(raw.githubusercontent 404),镜像 tools/mkapp.py 无 --new/--check(grep 0 命中);指南全文亦未给任何 clone URL(§2 只写「一份仓库 checkout,示例 /path/to/berryos」)——真实第三方按指南前提即卡死,tools/sync-public.sh 自 10-24 后未再跑",
  "步骤§2(环境前置):不卡——musl target/Python≥3.11 写明,README.md:43 Quick start 已补 aarch64-unknown-linux-musl 并直链指南(SDK-01 缺口⑤已修);但「QEMU 真跑」前置引到 README,而 README 与指南均未给出可克隆的公开地址(见步骤0)",
  "步骤§3(最小工程三件):不卡——§3.1 规范形模板实测过门禁,§3.4 脚手架 mkapp.py --new myapp --id MYAPP 实测产四件;摩擦点:脚手架 Cargo.toml path 依赖钉死 in-tree 相对路径(../../berry-libc),仓外须手改(指南 §3.2 有写、脚手架 README 有提示,已知摩擦非静默坑);manifest 规范形极严(注释/空白即拒),新手无 linter 只能靠拒因试错",
  "步骤§4(构建咒语):实测通过——照抄 §4 行仅换两处路径,macOS 宿主 2.29s 完链(8 crate 全编);卡点在文档滞后:指南 §9 写「SDK 七 crate」,但 berry-libc 实际已依赖 berry-signal(user/berry-libc/Cargo.toml,A7-4 段③a)——依赖图与 §9.1 七连 publish 手顺漏第八件,照手顺发布会炸在 berry-libc 步「no matching package named berry-signal」(path 剥离后 version req 无解;.github/workflows/ci.yml:115 守卫已是八 crate 口径,文档落后于代码)",
  "步骤§5(打包与复验):实测通过——staging 组装+mkapp 产 325400B 三条目包,--check 报 OK digests=ok;错误信息实测(带注释 manifest)返回一行中文拒因「非规范形(hex/下划线整数…注释…)exit=1」无行号/位置/错误码——人可读、机器与 LLM 难定位,SDK-01 硬结论 12 的「位置+期望+修法」三元组尚未落",
  "步骤§6(manifest 字段):不卡——字段表与 berry-manifest/mkapp 现实一致(fs=[\"rootfs\"] 闭集、net=[\"client\"] 勿声明已诚实标注 v1 边界)",
  "步骤§8(跑起来):结构性卡点(指南自认)——「仓外应用 v1 尚无自动接线位」:.bap 进系统须改 justfile 硬编码条目表(justfile:229-230 app/APPDEMO.bap=… 锚)或交系统集成者;FAT 盘 app/ 扫描(GUI-7 spike 未跑)、fat:rw/just dev(开发体验 L2 未落,docs/索引.md:139)双缺——第三方无法闭环「构建→装→跑→看输出」,每次迭代须整 rootfs 重建;调试面仅串口标记+eco: 降级行,无用户态 gdb、panic=abort+strip 无回溯,指南无调试节",
  "步骤§9(发布与获取):crates.io 实测九名(berry-sys/tls/netdproto/fsproto/mmap/signal/fsclient/libc/ui)全 404——「已发布态」未到来(指南头部双态声明已诚实预告,非欺瞒,但无时间线);八份 Cargo.toml repository 字段仍是占位 github.com/berryos/berryos(实测 404,§9.1 自警未执行);publish 手顺含 berry-signal 缺失 bug(同步骤§4)——第三方「cargo add berry-libc」路径今天不可走"
 ]
}
```

---

## 附录 D · B1 SDK 外部对标（agent 原文归档）

```json
{
 "new_insights": [
  {
   "insight": "crates.io Trusted Publishing 已 GA：维护者先手动发布一次、在 crates.io UI 绑定 GitHub 仓后，CI 经 OIDC 短时令牌免 token 发布（现限 GitHub Actions，GitLab/BuildKite 在议程中）；Sigstore 全量 provenance 为开放项。这是 SDK-01 拍板『现在上 crates.io』之后出现的新治理基建——六 crate 发布链应接入，结构性消灭本地 token 泄漏面（slopsquatting 时代的供应链防线）。",
   "source_url": "https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html",
   "topic": "crates.io 治理：Trusted Publishing GA（RFC 3691）"
  },
  {
   "insight": "Rust 官方把 Linux musl 目标（x86_64/aarch64/powerpc64le）的 musl 升到 1.2.5——berry-libc 是『musl 口径』的自研 shim，不消费 musl 字节，但 std 行为口径随上游 musl 版本漂移。应在 berry-libc 文档钉一行『口径基线 = musl 1.2.5』，防后续对拍/文档歧义（hexyl/uutils 等第三方程序的行为差异排查也受益）。",
   "source_url": "https://blog.rust-lang.org/2025/12/05/Updating-musl-1.2.5",
   "topic": "Rust musl 目标基线：2025-12 官方升级 musl 1.2.5"
  },
  {
   "insight": "Google 官方博客（2025-11）报告 Android Rust 新代码内存安全漏洞密度比 C/C++ 降 1000x；2026-08 再发 AI 辅助重写 C 库 giflib 为 Rust。『系统侧全 Rust』的安全性收益有了第一方大样本量化——可直接用作 BerryOS 对外叙事与 ADR 论据；同时它反衬 Google 平台侧 Rust 成熟而 NDK 应用面十年不开，佐证『开语言面是治理决策不是技术决策』。",
   "source_url": "https://blog.google/security/rust-in-android-move-fast-fix-things",
   "topic": "全 Rust 系统侧收益的第一方量化背书"
  },
  {
   "insight": "官方文档确认：应用内 live preview、VS Code 扩展、Slint LSP、Slint Viewer 四方共用同一个运行时解释器渲染 .slint——这是『单一事实源』在工具链面的工程兑现（一处修 bug 四处生效、预览即实机语义）。且 live-preview 是显式 Cargo feature + SLINT_LIVE_PREVIEW 环境变量门控，官方明文建议『不要把该 feature 写进 Cargo.toml』以隔离 dev/release。这正是 BerryOS UI-SDK 先导刀（DSL+解释器+LSP/live preview）应抄的架构约束。",
   "source_url": "https://docs.slint.dev/latest/docs/slint/guide/tooling/live-preview",
   "topic": "Slint 单解释器架构与 dev-only feature 门"
  },
  {
   "insight": "Dioxus 0.7 的 Subsecond 引擎实现 Rust 代码级热补（增量链接/二进制补丁，显式 subsecond::call 同步点，状态不自动迁移，支持 WASM/桌面/移动）。2026-11 热重载全景四档：DSL 数据级（Slint）→ Rust 代码级（Subsecond）→ 字节码级（Compose Live Edit）→ 全量重启。berryui DSL 数据级热更仍是与『AI 生成物=数据』裁决最对位、失败面工程量最小的一档；Subsecond 的『显式同步点』思想可借给 SDK-2 解释器的 reload 生命周期设计。",
   "source_url": "https://dioxuslabs.com/blog/release-070",
   "topic": "热重载谱系 2026-11 更新：Rust 代码级热补档出现"
  },
  {
   "insight": "Redox 明文『永不冻结 syscall ABI，稳定面 = relibc 的 libc ABI』（Zig 官方 issue 一手引述），但 relibc 自身未稳定、Redox 未到 1.0。与 BerryOS 互为镜像：BerryOS 反向多冻一个面（berry_* syscall 冻结 + abi-check 逐字节执法），libc 是随应用静态编译的 shim（ABI 面由链接期自动锁定）。两个项目都把稳定承诺收窄到一条缝——『中间不设稳定 ABI』方向获独立验证；BerryOS 的额外冻结换来机器执法的确定性，是值得在 ADR 叙事里显式记下的差异点。",
   "source_url": "https://github.com/ziglang/zig/issues/23905",
   "topic": "Redox 的 ABI 双层结构与 BerryOS 互为镜像"
  },
  {
   "insight": "Slint 现行三许可里 royalty-free 档明文不覆盖嵌入式（embedded 须商业许可）。对 BerryOS 双重意义：① berryui 自研决策（避开 Slint 许可墙）持续正确；② BerryOS SDK 六 crate 以 Apache-2.0 宽松直发，对嵌入式第三方集成者是比 Slint 更低摩擦的选择——这是未来对外生态的差异化卖点，值得写进 SDK 对外叙事。",
   "source_url": "https://github.com/slint-ui/slint/blob/master/FAQ.md",
   "topic": "Slint 许可现状：royalty-free 不覆盖嵌入式"
  },
  {
   "insight": "Fuchsia 语言政策 2025-02-28 修订新增：Python 3 经 RFC-0221 批准用于终开发者的『系统测试』（Python 2.7 弃用）。产品面语言收敛、测试面语言放宽的官方先例——佐证 BerryOS『宿主面 Python 工具链不算目标机语言』的既有边界划法；也提示 SDK-4 测试注入通道（FIFO + dump 对拍）可以用任意宿主语言驱动而不破『Rust 单语言』承诺。",
   "source_url": "https://fuchsia.dev/fuchsia-src/contribute/governance/policy/programming_languages",
   "topic": "Fuchsia RFC-0221：测试面语言宽于产品面"
  },
  {
   "insight": "cargo new 至今无模板定制系统（2020 pre-RFC 未落地、无进展），cargo-generate 仍是 Rust 脚手架事实标准，2026 社区模板生态活跃。SDK-1b 已落 mkapp --new 仓内脚手架；增量动作：把 appdemo 形态再发一份 cargo-generate 模板（git 仓即模板），out-of-tree 开发者 `cargo generate` 一步进场——与 mkapp --new 单源同一模板数据，边际成本一小时内。",
   "source_url": "https://internals.rust-lang.org/t/pre-rfc-cargo-new-templates/12060/",
   "topic": "cargo-generate 仍是脚手架事实标准"
  },
  {
   "insight": "Dioxus 0.7 工具链改为一行安装脚本（curl | bash）+ dx self-update 自更新，弃 cargo binstall 路径——框架 CLI 独立分发（自更新、版本不匹配告警）已成主流形态，因 cargo install 编译重/慢。对 BerryOS：berryctl 将来对外分发时同样应走『预编译二进制 + 自更新』而非仅 crates.io；仓内 just 体系不受影响。",
   "source_url": "https://dioxuslabs.com/blog/release-070",
   "topic": "框架 CLI 分发形态：独立安装器 + 自更新"
  }
 ],
 "recommendations": [
  "SDK-2 前置 ADR 微刀（纯文档刀，建议 GUI-6 期间，维持原排期）：热重载状态保持矩阵直接对位 Slint 三语义写死——『属性/回调/模型保留 / 语法错误旧版留屏（坏 DSL 拒载保旧树 + 遥测事件）/ 宿主接口（Msg 绑定、berry-gfx pass 面）改动需重装 .bap』；并新增第四条 dev/release 隔离条款：热重载能力必须 feature 门 + 环境变量门控、不进常规构建（Slint live-preview 同款纪律，防止热更解释器泄入 release 产物）。",
  "SDK-2 解释器刀（核心结构裁决）：解释器独立 crate 化（协议 crate 模式：纯函数、host 可测、零分配优先），并在刀内立法『单一解释器』铁律——设备侧解释器、宿主 live preview、LSP 三消费者共用同一 crate，禁止预览器自建第二套 DSL 求值路径（Slint 四方共用同解释器是先例）；失败面错误码按 LLM 可修三元组（位置+期望+修法）落地，last-known-good 回退打机器可读遥测事件。",
  "UI-SDK 先导刀（GUI-2 段③ 并轨窗口）：LSP/live preview 从第一天消费 SDK-2 同一解释器 crate（尚未建则先在协议 crate 位建最小解释器骨架）；预览器走宿主侧软件渲染（berry-gfx host 测面），不依赖 QEMU 在环——证据：Slint VS Code/LSP/Viewer 全是独立于目标设备的宿主进程，宿主预览是行业唯一形态。",
  "crates.io 治理小刀（半天级，任意时点）：六 crate（berry-sys/berry-tls/berry-netdproto/berry-fsproto/berry-fsclient/berry-libc）接入 Trusted Publishing——先各手动发布一版后在 crates.io UI 绑 GitHub 仓，CI 走 OIDC 发布、弃本地 token；顺带在 berry-libc 头注钉『musl 口径基线 = 1.2.5（Rust 2025-12 官方目标）』一行。",
  "SDK-3（帧时间线 p50/p90）维持原案（外部无反证）：实施时借 Dioxus Subsecond『显式同步点』思想——reload/热更生命周期事件（attempt/apply/fallback）与帧时间线共用统一事件 schema（begin/end + 稳定字段名，os_signpost 同构），判定结果输出结构化 JSON，与 AI 测试友好化支柱对齐。"
 ],
 "summary": "SDK-01 全部拍板在 2026-11 外部视角下校验完毕：六大关键主张（六层塔/全静态/禁脚本层/crates.io 直发/Rust 单语言/中间不设稳定 ABI）逐条核验后全部「仍成立」，无一需修正、无一过时。三个单语言方向的新证据反而加强原裁决：① Fuchsia 语言政策页 2025-02-28 修订后仍原文保留 \"Rust is not supported for end-developers\"——支持一门终开发者语言 = FIDL 后端+工具链+文档+DevRel 四线持续投资的结构性判断两年未松动；② Android NDK 2026 仍无一等 Rust 支持（NDK 团队自述人手不足，社区 cargo-ndk 为主流），而 Google 平台侧 Rust 交出内存安全漏洞密度降 1000x 的官方数据（2025-11）——语言面开放是治理/资源决策不是技术决策，BerryOS「明文非目标化」恰是把这笔成本显式归零；③ Redox 2026 仍活跃（io_uring 式 Redox Rings、RSoC 2026），且其 ABI 裁法与 BerryOS 互为镜像（syscall 永不稳定 + relibc 稳定 libc ABI vs BerryOS syscall 冻结 + libc shim 编入应用）——两条路线都把稳定承诺面收窄到一条缝，静态链接自兜底逻辑获独立验证。框架分层对照（Slint 三许可/crates.io 直发、Dioxus 0.7、iced 0.14、Tauri 2.0）全部维持「多 crate 家族 + crates.io 直发」形态，无包管理器化反例。最大增量有二：crates.io Trusted Publishing 已 GA（RFC 3691，2025-07，GitHub Actions OIDC 免 token 发布）应接入六 crate 发布链；Slint live-preview 的「四方共用同一解释器 + dev-only feature 门」架构为 UI-SDK 先导刀（DSL+解释器+LSP/live preview）提供了可直接照抄的工程范式。",
 "validations": [
  {
   "evidence": "外部四家 2026 现状全部同构分层：Slint = slint 主 crate + i-slint-core/i-slint-renderer-* 内部 crate（crates.io 直发、明文标记内部不直接用）+ slint-build/slint-interpreter 工具面（https://crates.io/crates/slint，2026-09 仍活跃）；Dioxus = 框架本体 + 社区运营 dioxus-sdk 拆多 crate（网络/地理/通知各一 crate，https://docs.rs/crate/dioxus-sdk/latest）；Tauri 2.0 = 核心 + 移动插件 + 官方分发工具链（https://v2.tauri.app/distribute）。生态无一家把 SDK 做成单包或 OS 包管理器形态；Slint「内部 crate 不直接用」的分层纪律与 L1/L2 边界同构。",
   "sdk01_claim": "SDK = 六层塔（L0 语言运行时→L1 系统库→L2 服务协议 crate→L3 框架→L4 工具链→L5 DFX/AI 面），层间依赖方向铁律",
   "verdict": "仍成立"
  },
  {
   "evidence": "musl 无符号版本设计未变（https://www.openwall.com/lists/musl/2022/04/20/2）；Rust 官方 2025-12-05 把 x86_64/aarch64/powerpc64le musl 目标升到 musl 1.2.5，静态 musl 目标仍是一等公民（https://blog.rust-lang.org/2025/12/05/Updating-musl-1.2.5）；Android bionic linker namespace 混合态之痛无翻案（https://android.googlesource.com/platform/bionic/+/fc8e688/android-changes-for-ndk-developers.md）。Redox 同样依赖静态链接 + relibc 稳定层（https://github.com/ziglang/zig/issues/23905）——静态单映像路线在新 OS 侧是共识而非孤例。",
   "sdk01_claim": "v1 全静态单映像（musl 无符号版本先例 + Android namespace 混合态之痛反证，动态链接留缝待 v2 整案裁决）",
   "verdict": "仍成立"
  },
  {
   "evidence": "2026-11 主流 UI 框架无一新增『应用内嵌脚本语言』方向；反例持续强化：Slint 官方 live-preview 文档确认 .slint 是纯数据——热重载时「properties, callbacks, and models are preserved」、语法错误旧版留屏、动原生接口才需重编（https://docs.slint.dev/latest/docs/slint/guide/tooling/live-preview）；Dioxus 0.7 的 Subsecond 热补走 Rust 代码/二进制补丁而非脚本层（https://dioxuslabs.com/blog/release-070）。「描述=数据、逻辑=宿主动词」的划界在 2026 生态只有加强没有松动。",
   "sdk01_claim": "脚本层不做（v1 只做「数据 + 宿主动词」，AI 生成物 = schema 约束的 AST/JSON）",
   "verdict": "仍成立"
  },
  {
   "evidence": "Slint（https://crates.io/crates/slint，2026-09 更新）、iced 0.14（2025-12 发布，https://docs.rs/crate/iced_futures/latest）、Dioxus 0.7 家族全部 crates.io 直发为主分发面，无一家改走系统包管理器。发布永久性与名字先到先得政策未变（https://doc.rust-lang.org/cargo/reference/publishing.html）。治理增量：Trusted Publishing 已 GA（RFC 3691，https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html）——主张成立且发布链应顺势升级。",
   "sdk01_claim": "berry-libc 等 crate 现在上 crates.io 直发（拍板⑥，比推荐更进取）",
   "verdict": "仍成立"
  },
  {
   "evidence": "Fuchsia 语言政策页 2025-02-28 修订后仍原文保留 \"Rust is not supported for end-developers\"，终开发者面仅 C/C++/Dart（+RFC-0221 批 Python 3 限系统测试）（https://fuchsia.dev/fuchsia-src/contribute/governance/policy/programming_languages）；Android NDK 2026 仍无一等 Rust 支持，社区 cargo-ndk 为主（https://github.com/android/ndk/discussions/1745 及 https://users.rust-lang.org/t/is-there-a-way-to-do-rust-programming-for-android-without-android-ndk/49926/3）；Google 平台侧 Rust 漏洞密度降 1000x 但应用面仍不开（https://blog.google/security/rust-in-android-move-fast-fix-things）——证明开语言面是四线持续投资的治理决策。Redox 全 Rust 单语言 2026 仍活跃可持续（https://www.reddit.com/r/rust/comments/1u0afp9/this_month_in_redox_may_2026）。",
   "sdk01_claim": "Rust 单语言，多语言绑定明文列非目标（Fuchsia 教训：开语言面 = FIDL 后端+工具链+文档+DevRel 四线持续投资）",
   "verdict": "仍成立"
  },
  {
   "evidence": "Redox 独立收敛到同款结论：官方表态 syscall ABI 永不冻结、稳定面收窄到 relibc 一层（https://github.com/ziglang/zig/issues/23905）；BerryOS 更进一步——berry_* syscall 冻结由 abi-check 逐字节执法，libc 是编入应用的静态 shim（ABI 面由链接期自动锁定），静态链接使『针对旧 SDK 编的二进制』兼容承诺变得不必要。Swift ABI 稳定耗时 5 年的教训亦无翻案（https://www.swift.org/blog/abi-stability-and-more/）。两 OS 双向验证：稳定承诺面越窄，工具执法越可行。",
   "sdk01_claim": "中间不设稳定 ABI，冻结面仅 berry_* syscall + wire 协议 add-only；静态链接自兜底",
   "verdict": "仍成立"
  }
 ]
}
```

---

## 附录 E · B2 布局深研（agent 原文归档）

```json
{
 "dsl_carrier": "证据对比（最终裁决权在用户；本节只摆证据）：\n\n【文本 DSL】①独立工具链：独立预览器 + 独立诊断不依赖 Rust 工具链——Slint 的 LSP/live-preview/AI skill 工具链是业界最完备的现成证据（research/GUI-07 §6.1，https://docs.slint.dev/latest/docs/slint/guide/tooling/ai-coding-assistants）；②AI 生成产物即数据：受约束组件词表 + 结构化输出优于自由代码生成（GUI-07 §5.2，https://daily.dev/posts/a-better-approach-to-generative-ui-hwpqccf9w），文本 DSL 天然是 LLM 可直接产出、可用 schema 校验的形态；③热重载：SDK-2 FAT 热区热重载不需重编译（规划 §3 裁决 H），文本文件改了就生效——Dioxus 的 CSS 文件级热重载是同型证据（https://github.com/DioxusLabs/dioxus/issues/2379）；④仓内已兑现：desc.rs:478-738 的 TLV AST 双入口 + roundtrip 测试证明「声明即数据」可落，.bap views/ 资源位（裁决 F，规划 :77）就是文本 DSL 的挂点；⑤宏税的结构性证据：proc-macro 展开不进增量编译缓存（https://internals.rust-lang.org/t/caching-of-proc-macro-expansions/21466）、rust-analyzer 宏内补全长期缺口（https://github.com/rust-lang/rust-analyzer/issues/18834）、Leptos 宏内补全被 # 语法干扰（https://github.com/leptos-rs/leptos/issues/1781）、xilem 为此弃宏走 builder（research/GUI-10 §7）。⑥文本 DSL 代价：需要自建解析器/校验器/诊断工具栈（Slint 独立语言的成本面，GUI-07 §6.1）；错误信息质量取决于工具投入。\n\n【Rust 宏 DSL】①rustc 全强度验证：宏展开为普通 Rust 构造代码，类型/穷尽性执法免费（GUI-07 §5.3）；②JSX 近形换 LLM 语料红利（GUI-07 §5.1 表：类 JSX 形态 LLM 生成可靠性最高）；③零解析器要写、编译期模板提纯空间（Solid/Dioxus 模板化思路，GUI-07 §5.3 第 3 点）；④代价：上列宏税全部照收 + 展开产物对 AI 不可读（GUI-10 §7 证据②——与「AI 生成产物即数据」裁决 F 相抵）。\n\n【建议】维持 2026-10-29 C2 拍板方向（research/GUI-10-移动优先渲染深研综合裁决.md §7/:116）：文本 DSL 为主数据载体、Rust 宏仅作类型安全绑定糖层，两者共享同一 TLV AST 单源（非第二事实源）。关键增量论据（本次深研新增）：(a) 仓内 TLV 双入口已把「宏糖展开到同一 AST」的互操作面证明为零成本——desc.rs DescBuilder（代码侧）与文本 DSL（数据侧）在 TLV 字节流会师；(b) 布局/样式原子若按建议做成词表闭集 u16 编码，文本 DSL 的合法字符集即词表本身——解析器极小且可独立出高诊断质量（对齐 .bap manifest 词表闭集先例 ADR #56）；(c) 宏路线的增量编译/IDE 税是结构性的、短期无解（internals t21466 明示）。SDK-1 时用真实开发数据复裁（规划 :11 口径），若届时宏糖有需求，限「窄域糖：只生成 TLV 字节流构造调用，无语句层」（GUI-07 §7.2 的域窄纪律）。",
 "incremental_strategy": "v1 视图级失效 → 属性级升级的渐进路径（缝位对号，先留什么后接什么）：\n\n第一步（v1 现状=缝已到位）：①dep.rs:36 DepTable 边表 day-one 记录（node, DepId），remove 连子树 unbind（lib.rs:526-535 先验解绑序），满帽 loud——记录不消费，失效仍走 invalidate/set_props 视图级（dep.rs:1-14 头注明示）；②lib.rs:172 props_geom_equal 已是全库唯一的「变更分类点」——所有 set_props 变更在此分「几何变（重排）/纯色变（仅重绘）」两档，这就是属性级 diff 的胚芽。\n\n第二步（v1.5 spike S7，kill criteria：<20% 帧耗时收益 → 维持视图级至 v2，research/GUI-10 :123）：③props_geom_equal 的返回值从 bool 升级为 change-mask 位集（u8：bit0=W、bit1=H、bit2=gap、bit3=bg、bit4=fg、bit5=text、bit6=scale…）——调用侧（set_props/set_text 两个漏斗，lib.rs:557/lib.rs:601）按 mask 分派：几何位 → layout_dirty + 祖先链；纯色位 → paint_dirty + 同界 damage；mask=0 → 无操作（0 重排 0 重绘）。签名升级只动 lib.rs 一个文件的两个函数——漏斗形状就是升级面收敛的保证。④DepTable 消费侧接线：app 在 update 后调 invalidate_dep(DepId) → deps.dependents()（dep.rs:70）得节点清单 → 对每节点跑 mask diff → 精确标脏。形状钉板测试（dep.rs:8 头注的验收面）先验边表形状，再验 mask 分派。\n\n第三步（升级条件成熟时）：⑤键路径 diff：desc.rs AST 的 key u64 域（desc.rs:111 ViewSpec、TLV FLAG_HAS_KEY desc.rs:245）v1 已透传——v1.5 加 patch 语义（flags 空位 bit1..bit7 之一 = patch op，按 key 定位 add/replace/remove），apply_tree 增量应用替代整树重建（apply_desc 现在整树 reset，lib.rs:1003）；⑥relayout 边界截断：mark_ancestors_dirty（lib.rs:331）加「定尺寸节点（Rect w/h 双 Some 且未变）截断祖先传播」判定——Flutter relayout boundary 同构（https://flutter.megathink.com/rendering/layout/），measured/placed 统计随之下降，Run 相帧预算受益；⑦属性级失效与脏区引擎的咬合收益实证：view 级 over-damage 常触发 damage K≤8 合并到 FULL（damage.rs:234-298）——mask 精确化直接降低 rect 计数与 overdraw（≤2.5× 预算，lib.rs:59），这个因果链进 S7 spike 的度量面（对比视图级 vs 属性级的 damage rects 计数 + overdraw_x256 + 帧耗时三指标）。\n\n不跳步纪律：三态停帧与「无 damage 不提交」不受属性级升级影响——mask=0 即零 damage 即 IDLE 可达，升级只缩小 Run 相工作面，不动帧循环状态机（frame.rs:62 纯函数真值表不变）。",
 "options": [
  {
   "cons": "需改 §6 冻结面（Node 字段、measure 签名）——要走 ADR + 草案修订流程；约束传播与内在尺寸（max-content）交互需要小心定义（Text 换行策略未定前只能给出无界内在宽）；比案 3 多一层间接。",
   "fit_berryui": "与三态停帧咬合最好：脏子树外零测量零摆放（layout.rs:82 无脏位短路）维持「IDLE 零 SUBMIT 零 FRAME_CALLBACK」（frame.rs:31-38）；paint-only 变更 measured==0 断言已有（layout.rs:24-31）；动画 Layout 型属性（GUI-3 草案 :84 AnimProp::{Width,Height,Gap}）每帧重排只触脏链，约束类缓存把 Run 相每帧成本压在受影响子树；边界截断减少 damage 矩形产出 → 降低 K≤8 满 8 折 FULL 的概率（damage.rs:234-298）→ 保 overdraw ≤2.5× 预算（lib.rs:59）。",
   "model": "算法=Flutter/SwiftUI 谱系的单遍协商：measure 子函数签名从「无输入内在尺寸」升级为「约束类输入（Tight(w,h)/Loose(max_w,max_h)/Infinite 三档枚举）+ 轴」，子返回尺寸；Col/Row/Grid 按约束类算子约束下发（游标分配现状保留）。树=现有 arena 槽表不动（lib.rs:123-125 nodes+free_stack）。缓存=现有 meas_w/h+layout_dirty 之上加「缓存约束类」字段：同约束类命中直接复用内在尺寸；定尺寸节点（Rect w/h 双 Some）= relayout 边界等价物，mark_ancestors_dirty（lib.rs:331）在此截断。",
   "name": "案 1：约束单遍协商 + arena 槽树 + 约束类键控缓存（target 态）",
   "pros": "对齐 ADR #52 单遍方向（GUI-10 反清单 #5：RelativeLayout 双遍病 → 单遍约束，Compose 立法同构）；修掉现状真缺口——子测量不看父可用空间，Text/容器无法按容器宽度收缩（layout.rs:188 intrinsic 无 avail 入参，仅 Root 吃 viewport）；业界性能模型最成熟：Flutter 60fps 主证（docs.flutter.dev/ui/layout/constraints）；缓存语义清晰可断言（同约束类命中=0 重测）；边界截断直接减少失效传播范围。"
  },
  {
   "cons": "内容尺寸子需双测（flexbox 内在尺寸 = 先无界测再定界，taffy compute_cached_layout 即为此设计 docs.rs/taffy）；CSS Grid 轨道定尺按规范是迭代式的（w3c www-style 2015Mar/0197：flexible 轨道 min-content 贡献要「以 0 可用空间再跑一遍定尺算法」）；表达力远超 v1 六视图 + 桌面场景所需（YAGNI）；对 TCG 软渲帧预算是持续税。",
   "fit_berryui": "与三态停帧咬合最差：内容尺寸缓存未命中时双测放大 Run 相帧成本（滑动翻页动画期每帧在跑）；内在尺寸变化沿祖先链 + 兄弟位移产生多矩形 damage，容易冲破 K≤8 折 FULL → overdraw 预算受压；IDLE 面无优势（脏驱动两者相同）。作为对照案保留价值 = 轨道定尺与测量缓存的结构设计值得借鉴，不作为实现路线。",
   "model": "算法=Taffy 形态：每节点 Style 结构体 + 算法 trait 分派（flexbox/grid/block），内容尺寸经 intrinsic 测量 + 按可用空间键控的测量缓存（taffy compute_cached_layout：『Attempts to find a cached layout for the specified node and layout inputs』）。树=TaffyTree 式节点 id 表。缓存=每节点多条目（按 available_space/axis 键）测量缓存 + 布局缓存。",
   "name": "案 2：Taffy 形双遍 flexbox/grid 谱系（style 结构 + 内容尺寸 + 多条目测量缓存）",
   "pros": "表达力天花板：grow/shrink/wrap/对齐全拿（CSS 全语义）；Taffy 是 Rust 生态最成熟布局库（github.com/DioxusLabs/taffy，Dioxus/iced 均用或评估）；grid 与 flexbox 同框架内闭合；测量缓存策略已被大规模验证。"
  },
  {
   "cons": "无响应式尺寸语义（无 grow/shrink/百分比/对齐控制）——800×600 固定屏够用，但状态栏避让、异构屏留隐患；子测量不看父可用空间的缺口保留（案 1 指出的同一缺口）；升级到约束协议时 intrinsic() 签名要改 = 二次动冻结面。",
   "fit_berryui": "与三态停帧咬合好：零新增测量 machinery，每帧成本下限最低（Xilem 实测 relayout 0.8ms 量级佐证线性定轨的廉价，reddit.com/r/rust/comments/1vmle4d）；无脏位短路（layout.rs:82）与 paint-only 0 重测断言原样成立；Grid 定轨 O(1) 每格无迭代 → Run 相动画期帧预算平稳。代价在失效面：任何尺寸变更 mark_ancestors_dirty 走到根（lib.rs:331 无截断），view 级失效 over-damage 偏大。",
   "model": "算法=现状精修：保留「后序无输入内在测量（Σ/max 聚合，layout.rs:222 child_agg）+ 前序游标摆放」，加 Grid kind（固定行列数定轨：轨宽=可用宽/列数，无 auto/fr 轨）、SwipeView=Row 页 + Transform 偏移（anim ox/oy 已有，lib.rs Node ox/oy）。树=现状不动。缓存=现状不动（meas_w/h + layout_dirty）。",
   "name": "案 3：纯内在尺寸单协商精修 + Grid 定轨（现状最小增量）",
   "pros": "GUI-6 落地最快：零协议变更、零冻结面破坏（GUI-3 草案 :406 已把网格挂账 GUI-6）；确定性最强（无协商分支 = 可复现测试面最大）；OpenHarmony ui_lite grid_layout.cpp 同路实证（规划附录 B L2「布局系统」条：lite 档仅 flex/grid/list 三种即表达桌面）；零 alloc 零 panic 纪律零风险。"
  }
 ],
 "recommendation": "推荐案 1（约束单遍协商）为目标态、以案 3 为其 v1 子集落进 GUI-6——两案共享同一 arena 与缓存骨架，是同一模型的两个完备度，非二选一。完整描述：\n\n【算法】单遍约束协商（Flutter BoxConstraints 的三档约束类简化 + SwiftUI parent-proposes/child-chooses 语义，思想参考不引依赖——ADR #52）：①测量协议升格：叶子/容器 measure(约束类) —— 约束类 = Tight(w,h)/Loose(avail_w,avail_h)/Infinite 三档枚举 + 轴参数，子返回自选尺寸（Text 无界态返 max-content、有界态按策略截断/换行档位）；②容器分配：Col/Row 游标累进 + gap（现状保留，layout.rs:400-450），加十字轴对齐枚举（start/center/stretch——Tailwind items-* 语义的最小面）；Pad 内缩整给（现状）；③Grid 定轨 v1 = 固定行列 + 等分轨（ui_lite grid_layout.cpp 同路），auto/fr 轨禁入 v1 范围盒（CSS Grid §12 轨道迭代定尺的复杂度明确不背，w3c www-style 2015Mar/0197）；④摆位粒度现状保留：重摆 iff（祖先强制 ∨ 自身重测），新旧绝对界进 damage（layout.rs:340-358）。\n\n【树】维持 arena 槽表 [Option<Node>; N] + 代数 NodeId（gen<<16|idx）+ 显式 free 链（lib.rs:123-153）——flat arena + 索引句柄是 no_std 保留树的社区共识形态（flat_rbtree no_std、tf_tree_arena pointer-free、slotmap 键制同族）；Node 全 Copy POD → 数组析构零递归 Drop、无 Box 链栈溢出面；环防御以 nodes.len() 为帽沿用（layout.rs:150）。\n\n【失效】v1 视图级不变（invalidate/set_props 漏斗，lib.rs:629/557）；变更分类单源 props_geom_equal（lib.rs:172）保留为属性级升级缝（见 incremental_strategy）；新增 relayout 边界等价物：定尺寸节点（Rect w/h 双 Some）截断祖先脏传播——Flutter relayout boundary 的「parent 忽略子尺寸/紧约束」判定裁剪为单条（我们的容器都消费子尺寸，紧约束档天然由 Rect Some 提供）。\n\n【缓存】每节点 meas_w/meas_h + layout_dirty（现状）+ 新增缓存约束类字段：同约束类复用、约束类变更才重测；LayoutStat{measured,placed} 断言面保留（paint-only 变更 measured==0）；oversize 退化路径（CAP=512 位图外）现状保留。\n\n【文本边界】布局只消费「run 宽度查询」抽象：trait TextMetrics { fn run_width(&self, bytes, scale) -> u32; fn line_height(scale) -> u32 }——monospace 定宽快速路径（len×cell，FontSize 单源）为第一实现；GUI-2 增件⑧三段解耦（shape/图集/pass 内绘制，glyphon 模式）后，shape 产物（glyph run + advance 表）注入该 trait 实现——布局与光栅互不依赖，cosmic-text 的 Buffer/shape/layout 分层是同构思想参考（pop-os/cosmic-text：shaping 与 layout 分离、rustybuzz 负责 shape）。TextBuf 64B 单行限制 v1 保留，多行/换行随 GUI-2 字体档刀再议。\n\n【样式 API 形态】TailwindCSS 原子语义 → Rust 落法 = 「词表闭集原子 id + Copy 位打包」：每个原子（w-16、gap-2、px-4、items-center、text-sm…）是编译期已知的小整数（u16 词表 id），Props 内以 packed u32/u64 位域承载（Option<u32> 尺寸域折入原子表）；三消费面共享词表——Rust 侧 const 原子表 + builder 方法（tx().w16().gap2()）、文本 DSL 侧直接写原子名、AST 序列化侧词表 id 进 TLV。依据：零 alloc（无 String 类名）、可序列化（id 是定长整数，与 .bap manifest 词表闭集 ADR #56 同构先例）、AI 友好（词表闭集 = 受约束生成面，GUI-07 §5.3 论证）；对照形态——iced 的 Theme+Palette+每组件 Catalog trait 返回 Style 结构（docs.iced.rs/iced/widget/theme）太重，Dioxus 的 CSS 文件+热重载（dioxus#2379）绕开 rustc 验证，均不取；css-style crate 的纯 builder 思路被吸收进 const 原子表。",
 "risks": [
  "文本测量主导帧成本：Xilem 实测 LCD 文本 pass 22ms/帧 vs 整个 relayout 0.8ms（reddit.com/r/rust/comments/1vmle4d/a_critical_review_of_xilem_in_2026）——shape/测量不隔离则布局缓存形同虚设；TextMetrics trait 边界（建议案）必须先于任何换行/多行 Text 落地，否则 intrinsic 缓存键不稳。",
  "约束类缓存与内在尺寸的交互易错：Text 的 max-content 在不同可用宽度下可换行（若启用换行）——缓存键必须含换行策略档；v1 锁单行 + 截断档（无换行）是规避手段，换行引入日 = 缓存键扩容日。",
  "§6 冻结面变更风险：约束协议要动 Node 字段与 measure 签名（GUI-3 草案 :169-171 冻结注记）——必须走 GUI-6 刀 + ADR + 草案修订，先写 host 钉板测试再动签名（layout.rs 现有 tests/layout.rs 缓存断言先扩后改）。",
  "Grid auto/fr 轨的迭代定尺复杂度（w3c www-style 2015Mar/0197：flexible 轨道 min-content 贡献需以 0 可用空间重入定尺算法）——v1 范围盒必须硬性排除 auto/fr，只做固定/等分轨，否则单遍性质被破坏且无 kill criteria 兜底。",
  "MAX_NODES=256 + CAP=512 oversize 退化全量重排（layout.rs:35/:85-99）：桌面页×网格×图标场景节点数逼近帽（5 页×24 图标×3 节点/格 ≈ 360）——GUI-6 前量一次 launcher 树规模，必要时升 MAX_NODES 或改位图为显式脏清单（配 free 链同款纪律）。",
  "属性级失效预付复杂度：S7 kill criteria（<20% 帧耗时收益 → 维持视图级，research/GUI-10 :123）未跑前不实现消费面——DepTable 只记录（dep.rs 现状），先消费 mask 分派的漏斗形状即可，防「缝 = 债」反噬（规划 §7.4 纪律）。",
  "样式原子词表治理：原子集膨胀无节制会重演 Tailwind 类名洪水（tailwindlabs 讨论 #20619 已在讨论分组治理）——词表闭集进 mkapp/ABI 单源执法（对齐 ADR #56 fs/net 词表先例），新增原子 = 显式版本域变更。",
  "damage 放大链：视图级失效 over-damage → K≤8 折 FULL → overdraw 预算（≤2.5×，lib.rs:59）承压——GUI-3 已有 RectSet 遮挡剔除兜底（damage.rs:363），但属性级 mask（incremental_strategy 第②步）才是治本项，S7 spike 度量面必须含 damage rect 计数指标。",
  "dsl_carrier 复裁风险：文本 DSL 的解析器/诊断/预览器投入若在 SDK-1 前被低估，宏糖层会以「先凑合」名义膨胀成第二事实源——硬要求「两载体共享同一 TLV AST」需要进 CI 断言（roundtrip 对拍进 smoke 面，GUI-9 dump round-trip 同法）。"
 ],
 "sources": [
  "https://docs.flutter.dev/ui/layout/constraints",
  "https://flutter.megathink.com/rendering/layout/",
  "https://github.com/DioxusLabs/taffy",
  "https://docs.rs/taffy/latest/taffy/",
  "https://github.com/DioxusLabs/taffy/issues/308",
  "https://www.sagarunagar.com/blog/geometry-in-swiftui",
  "https://rafalsroka.com/posts/swiftui-architecture",
  "https://yasurok2.wordpress.com/2021/09/25/cassowary-a-constraint-satisfaction-system-for-ui-and-its-reimplementation-kiwi",
  "https://github.com/android-cn/android-discuss/issues/386",
  "https://developer.android.com/develop/ui/compose/layouts/intrinsic-measurements",
  "https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/arkts-v1-v2-update-difference",
  "https://developer.huawei.com/consumer/cn/doc/harmonyos-faqs/faqs-arkui-829",
  "https://blog.csdn.net/2502_93915572/article/details/162880319",
  "https://docs.rs/flat_rbtree",
  "https://docs.rs/tf_tree_arena/latest/tf_tree_arena",
  "https://lib.rs/crates/slotmap-map",
  "https://docs.rs/cosmic-text",
  "https://lib.rs/crates/floem-cosmic-text",
  "https://stackoverflow.com/questions/76777246/how-do-i-measure-text-size-in-cosmic-text",
  "https://github.com/linebender/xilem/blob/main/ARCHITECTURE.md",
  "https://github.com/linebender/xilem/blob/main/masonry/ARCHITECTURE.md",
  "https://www.reddit.com/r/rust/comments/1vmle4d/a_critical_review_of_xilem_in_2026",
  "https://docs.iced.rs/iced/widget/theme/index.html",
  "https://austinmreppert.github.io/iced-reference/chapter_3.html",
  "https://docs.slint.dev/latest/docs/slint/guide/language/coding/positioning-and-layouts",
  "https://docs.slint.dev/latest/docs/slint/reference/layouts/overview",
  "https://crates.io/crates/tailwind-rs-wasm",
  "https://www.reddit.com/r/rust/comments/z1dche/railwind_a_tailwind_compiler_with_warnings",
  "https://crates.io/crates/css-style",
  "https://lib.rs/crates/daisy_rsx",
  "https://github.com/DioxusLabs/dioxus/issues/2379",
  "https://lists.w3.org/Archives/Public/www-style/2015Mar/0197.html",
  "https://internals.rust-lang.org/t/caching-of-proc-macro-expansions/21466",
  "https://github.com/rust-lang/rust-analyzer/issues/18834",
  "https://github.com/leptos-rs/leptos/issues/1781",
  "https://daily.dev/posts/a-better-approach-to-generative-ui-hwpqccf9w",
  "https://docs.slint.dev/latest/docs/slint/guide/tooling/ai-coding-assistants",
  "https://github.com/tailwindlabs/tailwindcss/discussions/20619",
  "user/berry-ui/src/layout.rs:60-452（relayout/measure/intrinsic/place 现状）",
  "user/berry-ui/src/desc.rs:47-106,111,245-254,478-738（视图集/Props/ViewSpec/TLV 双入口/DescBuilder）",
  "user/berry-ui/src/lib.rs:52,59,94-115,123-153,172-184,331,557-599,629-638,815-894,1003-1043（Node 冻结面/props_geom_equal/mark_ancestors_dirty/set_props/invalidate/render_frame/apply_desc/dep 面）",
  "user/berry-ui/src/damage.rs:189-344,363-512（try_merge/DamageSet/RectSet 遮挡剔除）",
  "user/berry-ui/src/dep.rs:1-102（属性级依赖骨架：记录不消费 + S7 门）",
  "user/berry-ui/src/frame.rs:31-128（三态停帧状态机/next_wakeup）",
  "docs/GUI里程碑规划-2026-10-v1.0.md:13,77,87-95（C2 拍板/裁决 F/GUI-2..GUI-6 刀表）",
  "docs/GUI-3实施设计草案-v1.0.md:64,84,169-171,406（LayoutEngine 缝 GUI-6 定形/AnimProp 分型/Node 冻结面/网格挂账）",
  "research/GUI-07-声明式UI框架对比.md:90-96,141-145,183-195（属性级失效行业收敛/三全论证/载体对比）",
  "research/GUI-10-移动优先渲染深研综合裁决.md:44-58,92,115-123（反清单 #5 单遍约束/C2 DSL 载体/S7 kill criteria）"
 ],
 "summary": "任务 B2 深研结论：berry-ui 布局子系统已具备「单遍协商 + arena 槽树 + 每节点内在尺寸缓存」的正确骨架（user/berry-ui/src/layout.rs:60 relayout：后序测量 layout.rs:141 + 前序摆放 layout.rs:304 + meas_w/h 缓存 layout.rs:175-177 + 旧/新绝对界进 damage layout.rs:341-356），与业界三谱系对照定位清晰：Flutter 单遍约束下行/尺寸上行 + relayout boundary（docs.flutter.dev/ui/layout/constraints；flutter.megathink.com/rendering/layout）是移动主战场验证过的性能模型；Taffy 双遍 flexbox（style 系统 + 按可用空间键控的测量缓存 compute_cached_layout，docs.rs/taffy）表达力最强但强制依赖 alloc——与 berry-ui 零分配纪律结构性冲突（docs.rs/taffy：「Taffy always depends on the alloc library」）；约束求解（cassowary/Kiwi）已被 SwiftUI 用「parent proposes/child chooses」显式取代（sagarunagar.com/blog/geometry-in-swiftui）；Android RelativeLayout 双遍测量是反面前车之鉴（github.com/android-cn/android-discuss/issues/386：RelativeLayout 恒两次测量子节点），正是 GUI-10 反清单 #5「布局自研单遍约束」的立法依据（research/GUI-10-移动优先渲染深研综合裁决.md:50）。v1 失效粒度视图级 + 属性级留缝（dep.rs:36 DepTable 已记录边、lib.rs:172 props_geom_equal 已做几何/着色二分类）与 SwiftUI @Observable/ArkUI V2/Compose Strong Skipping 四家属性级收敛方向兼容（GUI-07 §2/§3），升级路径无需推翻任何现有面。树表示已是最优解：[Option<Node>; N] + gen 代数 NodeId 与 slotmap/flat arena 社区共识同族（docs.rs/flat_rbtree、docs.rs/tf_tree_arena），Node 全 Copy POD 无递归 Drop 面。文本测量是布局侧最大缺口：现用 len×cell 单宽模型（layout.rs:198），CJK/多字号/换行需要 shape 与布局解耦的 TextMetrics 边界（cosmic-text 思想：shaping 与 layout 分离，docs.rs/cosmic-text）。样式系统推荐「Tailwind 原子 = 词表闭集 + const 表 + Copy 位打包」，与 .bap manifest 词表闭集先例（ADR #56）同构。"
}
```

---

## 附录 F · C1 对抗交叉校验（agent 原文归档）

```json
{
 "missed": [
  "【intent/AI 原生调度的 SDK 接口位——五份共同最大盲区】为什么重要：AI 原生调度是 AGENTS 三身份支柱之一，且接口已在 ABI 层预埋——docs/系统调用ABI设计-v1.0.md:134「intent 预埋（ADR #42，零新号）」、:167 options 低 8 位=berry_intent 值域冻结、:212-220 枚举 0x00-0x05 六档冻结、:63 0x38 thread_set_priority 的 intent 扩参挂 v2；berry-sys lib.rs:340 与 abi_gen.rs:44-45（CHAN_OPT_NO_INTENT）已镜像。但五份产物零盘点：A1 的 berry-sys api_surface 不列 intent 域；B1/B2 的对标与建议全不涉及；berry-libc pthread_create 无 intent 透传参数（普通 app 无法声明）；guiproto 的 SCENE/VOTE「帧率意图」与内核 berry_intent 是两套「意图」，归一映射（帧率 vote 是否折成 berry_intent 值进调度器）无人裁决——移动主战场的核心卖点在开发者面断链。该补什么：intent 预埋位→SDK L1/L3 暴露面设计（pthread_attr/process options/channel opt-out 的 libc 层透传）+ VOTE↔berry_intent 归一 ADR + ADR #21 用户态策略带内接口对 app 可达性的专项研究。",
  "【QEMU 仿真面约束对性能结论的污染——五份全数中招】为什么重要：AGENTS.md 明示「TCG 下 CNTV tick 实测≈SLICE_HZ/2（非内核 bug）」，且 virtio-gpu 未接（berry-gfx 仅 soft 后端、ramfb 显示）——而五份产物引用 60/120fps ballot、RESAMPLE_LATENCY 8ms 帽、IDLE_AFTER_MS 2s、overdraw≤2.5× 等数字时全部零 TCG 时率警告：A2 的 expected/actual 双轨遥测按 60Hz vsync 口径设计但消费者只有 host tests；B2 的帧预算论证全在 host 侧得出。GUI-10:117-118 原有 S1（virtio-gpu）与 S2'（B-2 icount 光栅核定标）kill criteria，但五份产物无一承接——省电身份支柱的 GUI 侧（停帧=零唤醒）在 QEMU 上既测不出真实功耗、时率还折半，v1 验收通道结构性缺位。该补什么：GUI 面基准接入 bench-icount 确定性通道的可行性研究 + TCG 时间税换算表（SLICE_HZ/2 对 frame.rs next_vsync_tick/ballot 定值的实际影响）+ 功耗断言退出 QEMU 的替代验收方案（或明示 v1 GUI 性能结论 host-only 的边界声明）。",
  "【多会话并行开发纪律与五份建议的文件面互斥】为什么重要：仓内多会话纪律是现役制度——docs/索引.md:139 明写「（兄弟会话在途文件，仅报告勿动）」、docs/SDK-1b实施设计草案-v1.0.md:27 §0.2 红线标题即「红线（多会话纪律）」——而五份的 recommendations 动的是同一批共享热文件：B1 建议 Trusted Publishing 接入动 ci.yml、A3 建议镜像同步进 CI 动同一 workflow、A3 的指南七→八 crate 修正与维护合同回写动 docs/应用开发指南.md、B2 建议 SDK-2 前置 ADR 提前到 GUI-6 期间动 docs/GUI 里程碑规划——无一份考虑 GUI-2..9 线与 SDK 线并行时 user/Cargo.toml（members 逐刀加 crate）、ci.yml、justfile、八份 README 的归属与串行窗口，也未给「指南 publish 手顺修正」这类跨线文档 hotfix 定归属刀。该补什么：SDK 线/GUI 线共享热文件清单 + 每文件原子归属约定（谁动谁锁或并入同刀）+ 文档 hotfix 的最小刀位定义，纳入两线规划的排刀前提。",
  "【berryui DSL 的 AI 生成闭环落点与容量预算帽】为什么重要：闭环三腿中「校验」腿（AST TLV parse + 七类 loud 拒面）现活在 berry-ui crate 内部——desc.rs 的 parse_node/encode_props 是 pub(crate) 冻结签名，AI 工具/宿主预览器/LSP 无法脱离 berry-ui crate 消费（第三方 berry-ui 又不在发布族，A1 自证 crates.io 404）——TypeChat 式生成→校验→回喂在 SDK 面没有可调用落点；且双容量帽的字节账无人算：DESC_MAX=4096B（desc.rs:251）+ MAX_NODES=256（lib.rs:52），B2 自己估的 launcher 场景 360 节点既超 MAX_NODES（B2 已列风险）也逼近/超 DESC_MAX——按 TLV 单价（Rect 11B + Text 6+len），120 图标 ≈ 120×(11+6+8) ≈ 3KB，加页容器/首页 widget 即破 4KB，AI 整屏生成恰是最先打爆两帽的入口，而拒面只有一行 TooSmall。该补什么：DSL 校验器独立 crate 化（L2 协议 crate 位，复用 ADR #55 层位）+ 金样语料与对抗模糊语料测试面 + DESC_MAX/MAX_NODES 预算实测进 SDK-2/GUI-8 验收线。",
  "【应用分发/商店面缺位——五份只覆盖到「能打包能跑」，分发链零覆盖】为什么重要：v1 主战场是移动设备（手机），而 .bap 完整性 = SHA256SUMS（无签名无验签链：tools/mkapp.py 任何人可造包，appd 侧无来源校验）；manifest Security{profile,key_lineage} 纯预留零实现（A1 一笔带过）；升级/回滚/卸载/权限授予 UX/商店协议零议程——A1 gaps 只点到 capabilities 词表闭集，未点分发完整性链。这不只是 v2 功能缺位：它反噬 SDK 设计本身（L4 mkapp 需签名位预留、appd 需验签钩、manifest v2 词表扩容时 security 域与分发域应同刀裁），而 manifest v2 ADR 恰是 B1/A3 都建议的 SDK-2 前置微刀。该补什么：分发与完整性链专项调研（对标 APK Signing v2/v3、Fuchsia pkg_resolver/amber、HarmonyOS App 包格式），产出 v1 预埋位清单 + manifest v2 security/distribution 域草案。",
  "【无障碍/语义树——五份零覆盖的移动战场合规刚需】为什么重要：ViewKind 六件/Props 五属/TLV AST（desc.rs）均无语义域（无 label/role/contentDescription/焦点序），五份产物无一提及 a11y——手机主战场（AGENTS 身份声明）上无障碍是合规与生态准入刚需；且语义树与「AI 原生」身份天然协同（语义树 = AI 读屏/自动化测试的共同接口，对应 AI 测试友好化支柱）；更紧急的是时机成本：GUI-3 §6 契约已冻结，冻结后再补语义域 = 又一次走 ADR+草案修订的冻结面变更（B2 风险面已指出 §6 变更的流程代价），GUI-6/7 前是低成本预留窗口。该补什么：GUI-6/7 前语义树预留位评估——最小面 = TLV flags 空位或 add-only key/label 域（不破 roundtrip），对标 Android contentDescription/Compose semantics、Slint accessible-* 属性集，产出进 GUI-6 刀表或独立增件。"
 ],
 "summary": "对五份研究产物做对抗交叉校验：抽查仓内 file:line 锚 30+ 处（含 berry-ui 全部契约锚群、justfile/ci.yml/指南/研究文档锚）、实跑 berry-ui 96 host 测、curl 实测 crates.io 与 GitHub 镜像、WebFetch 核验 9 个外部 URL。总裁决：五份产物证据链质量高——仓内锚仅 1 处不精确（berry-sys「三处同步」自认不在所引 lib.rs:8-10）、外部仅 1 处过度引述（B1 把 Zig issue 中第三方推断写成「Redox 官方表态」，REFUTED），其余抽到的方向性结论（依赖方向铁律、96 测全绿、指南七/八 crate 漂移、镜像断点、Fuchsia/Slint/Trusted Publishing/musl 1.2.5/Google 1000x/Taffy alloc）全部 CONFIRMED；A1 对任务前提「六 crate 已上 crates.io」的关键事实修正经独立复核成立（2026-11-01 九名全 does-not-exist）。共同盲区集中在六处：intent/AI 调度的 SDK 接口位已预埋却零盘点（berry_intent 枚举已在 ABI 冻结）、QEMU TCG 时率与无 GPU 对全部帧率/时延结论的污染未标注、多会话并行纪律与五份建议的共享热文件互斥、AI 生成闭环的校验器落点与 DESC_MAX/MAX_NODES 容量账、应用分发/签名链缺位、无障碍语义树零覆盖。另：本次实测自身踩中根 config 锁 target 的坑（直接 cargo test 编译炸 140 错，须 --target host 同 justfile 口径），佐证 AGENTS 构建纪律条的真实性。",
 "verdicts": [
  {
   "claim": "B1：Redox「明文『永不冻结 syscall ABI，稳定面 = relibc 的 libc ABI』（Zig 官方 issue 一手引述）」——即把这表述当作 Redox 官方表态/明文",
   "evidence": "实测 WebFetch https://github.com/ziglang/zig/issues/23905（2026-11-01）：该 issue 中唯一 ABI 相关段落是开 issue 的 Zig 贡献者 alexrp 自己的推断原话「It seems like Redox will not have a stable syscall ABI, but will have a stable libc ABI through relibc」——无任何 Redox 维护者引述、无 Redox 官方文档链接。结论方向（Redox 稳定面收窄到 relibc 一层）与 Redox 公开架构常识一致、未被推翻，但 B1 的证据强度造假级偏松：「一手引述/官方表态」不成立，应降级为「第三方评估」并另补 docs.redox-os.org 官方源（如 The Book 内核稳定性章节）再入 ADR 叙事。",
   "verdict": "REFUTED（证据锚不支持所述强度）"
  },
  {
   "claim": "A1（顺带修正任务前提）：「六 crate 已上 crates.io」不成立——真状态 = publish-ready 未发布，crates.io 实测 404，最终 publish 是挂账的用户动作",
   "evidence": "2026-11-01 本人 curl https://crates.io/api/v1/crates/{name} 九名（sys/libc/tls/netdproto/fsproto/fsclient/mmap/signal/ui）全部返回「crate `berry-*` does not exist」；docs/SDK-1b实施设计草案-v1.0.md §0.2 红线 4 原文「不执行最终 cargo publish（需用户 crates.io 账号——发布是用户动作，本刀交付到 publish-ready + 手顺文档为止）」。A1 的关键事实修正独立复核成立。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "B1：crates.io Trusted Publishing 已 GA，六 crate 发布链应接入（OIDC 免 token）",
   "evidence": "RFC 3691 页（https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html）机制逐条核实：先手动发布一次→crates.io UI 建 per-crate Trusted Publisher 配置（绑 GitHub 仓+workflow 文件）→CI 经 OIDC 换短时令牌（约 15 分钟）→仅 GitHub Actions 先行（GitLab/CircleCI 在议程）——与 B1 转述一致。注意：RFC 页本身无 GA 标记；GA 时点（2025-07）由外部信源证实（daily.dev 2025-07-16 报道、zenn.dev 2025-08 教程、crates.io discussion #13633 BuildKite 扩展在议）。建议本身（接入六 crate）成立。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "A2：「berry-ui 4359 行、96 host 测全绿（damage 15/frame 15/input 10/reconcile 10/anim 9/dep 8/tree 8/resample 8/layout 7/offscreen 6）」",
   "evidence": "本人实跑 `cargo test -p berry-ui --target aarch64-apple-darwin`（user/ 工作区）合计 96 passed / 0 failed；#[test] 计数逐文件 9/15/8/15/10/7/6/10/8/8=96 与分布完全一致；wc -l user/berry-ui/src/*.rs = 4359。附带发现：不显式 --target 会被根 .cargo/config 锁到 aarch64-unknown-none 导致测试编译炸（140 errors）——justfile:441-447 test-host 显式 --target {{host}} 正是为此，佐证 AGENTS 构建标记条。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "A1 方向结论：「层间依赖方向铁律在 crate 依赖图上实际成立——GUI 族仅依赖 fbcon/gfx/guiproto，未反向沾 fs/net/kernel 面」",
   "evidence": "user/berry-ui/Cargo.toml:24,28 dependencies 仅 berry-gfx（path）+ berry-guiproto（path，且注释明示「常量/类型单源（非收发）」）；user/berry-gfx/Cargo.toml:22 仅 berry-fbcon。依赖闭包 = {gfx, guiproto, fbcon}，零 fs/net/kernel 依赖。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "A2「代码与 GUI-3 §6 冻结契约逐字对齐、§10 裁决在代码有对应锚」（抽查 12 处行锚）",
   "evidence": "全部逐字命中：lib.rs:52-61 常量族（MAX_NODES=256/TEXT_INLINE=64/…/OVERDRAW_BUDGET_X256=640/IDLE_AFTER_MS=2000）、lib.rs:73-88 UiError 九变体、lib.rs:433-435 root 容量失败返 NodeId(u32::MAX) 哨兵（§10 条 3 原文注释在场）、desc.rs:21-23 Text 73B 勘误（「§3.2 原稿写 ≤69B」原注）、hit.rs:30 PTR_MAX=4、layout.rs:198 Text=len×cell_w×cell_h 单行、anim.rs:41-44 Easing 两档、frame.rs:31-38 三态枚举、dep.rs:7-8「S7 spike 定生死：属性级收益 <20% 帧耗时→维持视图级至 v2」、damage.rs:509 RectSet::is_empty、desc.rs:251 DESC_MAX=4096/:646 DescBuilder、lib.rs:930-935 wake 条件化 FULL（「空集才置」修复轮 finding 9 注释在场）。A2 的对账可信。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "A3：「指南 §9 依赖图与 §9.1 七连 publish 手顺漏 berry-signal（实为八 crate），照文档发布会炸；CI 守卫已是八 crate 口径」",
   "evidence": "docs/应用开发指南.md:10「SDK 七 crate」、:283/286/342「七件/七件发布/七份 Cargo.toml」；.github/workflows/ci.yml:110-115 sdk-publish-guard 硬门 1 循环八个 crate（berry-sys/tls/netdproto/fsproto/mmap/signal/fsclient/libc）含 berry-signal——文档落后于代码属实，且 ci.yml 循环顺序即依赖序，缺 berry-signal 的手顺在 berry-libc 步必炸。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "A3：「公开镜像 github.com/miuiadmin/BerryOS 停在 2026-10-24；指南全文无 clone URL；mkapp 拒因一行中文无行号/错误码」",
   "evidence": "api.github.com/repos/miuiadmin/BerryOS 实测 pushed_at=2026-10-24T11:36:45Z（早于 SDK-1a/1b 10-30）；grep 'clone' docs/应用开发指南.md 全文仅 1 命中且在 :305 注释「干净 clone/git clean 后缺」（非 URL）；tools/mkapp.py:261 拒因即一行字符串「manifest: 非规范形（hex/下划线整数、单引号、内联表…」——无位置无错误码，而成功行 :442 反有 entries=/digests= 半结构化。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "B1：Fuchsia 语言政策 2025-02-28 修订后仍原文保留「Rust is not supported for end-developers」，且 RFC-0221 批 Python 3 限系统测试（佐证『测试面语言宽于产品面』）",
   "evidence": "WebFetch https://fuchsia.dev/fuchsia-src/contribute/governance/policy/programming_languages（2026-11-01）逐字命中：「Rust is not supported for end-developers.」；C/C++/Dart 支持；「Per RFC-0221, Python 3 is approved for use by end developers for system testing.」——B1 引述准确无夸大。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "B1/B2：Slint live-preview =「properties, callbacks, and models are preserved + 语法错误旧版留屏 + live-preview feature 与 SLINT_LIVE_PREVIEW 环境变量双门控且官方明示勿写入 Cargo.toml」",
   "evidence": "WebFetch https://docs.slint.dev/latest/docs/slint/guide/tooling/live-preview（2026-11-01）三句逐字命中：「The application keeps running during reloads, and properties, callbacks, and models are preserved.」「If a .slint file has a syntax error, the previous version stays on screen until the error is fixed.」「Don't add live-preview to the [features] section of your Cargo.toml.」——B1 的 SDK-2 热重载矩阵/解释器单源建议、B2 的载体论据均以此真实证据为底。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "B1：Google 官方 2025-11 博客报告 Android Rust 新代码内存安全漏洞密度比 C/C++ 降 1000x",
   "evidence": "WebFetch https://blog.google/security/rust-in-android-move-fast-fix-things（2026-11-01）：文为 2025-11-13，逐字「a more than 1000x reduction」「estimated vulnerability density for Rust is 0.2 vuln per 1 million lines (MLOC)」（C/C++ 历史 ≈1000/MLOC）——1000x 数字为官方原文，B1 无夸大。（B1 同条提到的 2026-08 giflib 重写为另一篇，未核，属次要。）",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "B1：Rust 官方 2025-12-05 把 x86_64/aarch64/powerpc64le musl 目标升到 musl 1.2.5，应在 berry-libc 钉「口径基线 = musl 1.2.5」",
   "evidence": "WebFetch https://blog.rust-lang.org/2025/12/05/Updating-musl-1.2.5（2026-11-01）：文存在、日期/三 target 逐字命中，且明示「Beginning with Rust 1.93 (slated for stable release on 2026-01-22)」才生效。方向成立但补一刀：仓根无 rust-toolchain.toml（A1 gap L0 自证），钉 musl 口径 1.2.5 前应先钉 rustc 版本，否则口径随开发者本机 stable 漂移——B1 建议与 A1 gap L0 应连做，两份产物均未指出此耦合。",
   "verdict": "CONFIRMED（附一处未接线的耦合）"
  },
  {
   "claim": "B2：「Taffy always depends on alloc——与 berry-ui 零分配纪律结构性冲突」（不引 Taffy 只作思想参考的裁法依据）",
   "evidence": "WebFetch https://docs.rs/taffy/latest/taffy/（2026-11-01）Feature Flags 节逐字：「Deprecated no-op: Taffy always depends on the `alloc` library.」（alloc feature 仅为兼容保留）——B2 引述准确。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "B2：「GUI-10 :123 S7 kill criteria = 属性级收益 <20% 帧耗时→维持视图级至 v2；GUI-3 :406 LayoutEngine 插拔缝归 GUI-6、网格归 GUI-6」",
   "evidence": "research/GUI-10-移动优先渲染深研综合裁决.md:123 原文「S7（可选）属性级失效小原型：kill：属性级收益 <20% 帧耗时 → 维持视图级至 v2。」；docs/GUI-3实施设计草案-v1.0.md:406 原文「布局算法插拔缝（LayoutEngine trait）——GUI-6 定形（v1 内建 Col/Row/Pad/Fixed；网格归 GUI-6）」——与 dep.rs:7-8 头注、layout.rs 现状互证。",
   "verdict": "CONFIRMED"
  },
  {
   "claim": "A1 锚点：「berry-sys lib.rs:8-10 头注自认欠债：IDL 单源生成未做、改签名三处同步靠冻结纪律」",
   "evidence": "lib.rs:1-8 实为 crates.io 面英文头注「This crate mirrors the frozen v1 syscall ABI…single source: tools/abi.toml」；grep '三处|IDL' user/berry-sys/src/lib.rs 零命中——「三处同步」自认不在所引锚位（可能在 ABI 设计文档或 SDK-1a 草案，A1 转述锚漂移）。实质主张（手写镜像 + abi.toml 单源 + abi-check 代偿）由头注与 CI 佐证成立，不影响 A1 结论，但该锚不可复核为原文。",
   "verdict": "UNVERIFIED（锚点不精确，实质成立）"
  }
 ]
}
```

---

## 附录 G · C2 布局可行性批判（agent 原文归档）

```json
{
 "conflicts": [
  {
   "conflict": "推荐案【失效】节与 incremental 第⑥步把边界定义为「定尺寸节点（Rect w/h 双 Some 且未变）截断 mark_ancestors_dirty（lib.rs:331）祖先传播」。直读代码证伪：v1 所有容器都消费子尺寸——child_agg 的 Σ/max 聚合（layout.rs:222-246）与 Col/Row 游标取 c.meas_h/meas_w（layout.rs:400-435）意味着固定尺寸 Rect 自身 h 变更（anim Width/Height anim.rs:206-229、set_props lib.rs:587-596）必然位移兄弟，此处截断祖先 = 漏排兄弟 + 脏区漏记；而「且未变」的节点根本走不到 mark_ancestors_dirty（props_geom_equal lib.rs:578-584 已在 geom_same 路径整段跳过）——规则按字面实现要么 no-op 要么错误。且失效上行真正走的是 measure() 的 child_affected 返回链（layout.rs:158-184），只截 mark_ancestors_dirty 连边界语义都没实现。另发现：measure() 已算好的 mctx.changed 位（layout.rs:175 计算、:181 写入）是全库 write-only 死数据——真正可落的边界优化（重测后尺寸未变 → 返 false 截断上行）数据已现成、只差消费，推荐案没有找到这个真缝。",
   "point": "①+② 结构/语义——「relayout 边界截断」规则写错对象：Rect w/h 双 Some 不是边界，且截断点放错函数",
   "resolution": "改推荐案两处：(a) 删除「Rect w/h 双 Some」表述，边界重定义为父侧属性「父不消费子主轴尺寸」——v1 现有六视图无一满足，真边界只随 Grid 固定定轨（案3）与案1 Tight 约束档产生；(b) incremental 第⑥步改为「measure() 消费已有 mctx.changed 位：自身重测且 meas 不变 → child_affected 返 false」——layout.rs:175/:181 数据现成，属纯增量、不动冻结面，收益（Run 相动画期 measured/placed 下降）与推荐案第⑥步目标相同；mark_ancestors_dirty 侧不动（保守预标 + measure 侧精确截断两层并存）。先扩 tests/layout.rs 钉板「子重测尺寸不变 → 祖先 measured==0」。",
   "severity": "高"
  },
  {
   "conflict": "案1 cons 仅自认「Node 字段、measure 签名」。实际逐文件核对还差四件：relayout 是 §6 冻结签名（GUI-3草案:315-316，layout.rs:60-66 实现）——约束类与 TextMetrics 都必须进参；Props 族扩展（对齐枚举进 Col/RowProps）连带 desc.rs encode_props/parse_props（:328-440）与 DescBuilder；MAX_NODES=256/DESC_MAX=4096（lib.rs:52、desc.rs:251）与 UiMain 别名（lib.rs:61）；Node 头注明文「冻结字段，一字不改」（lib.rs:91-93）。影响面报不全 → ADR 与草案修订会中途返工。",
   "point": "① 结构——§6 冻结面影响清单不完整（案1 自认两项，实为六项）",
   "resolution": "GUI-6 开刀前出全量冻结面清单（Node 字段 / relayout 签名 / Props+TLV / UiError 无关 / MAX_NODES+DESC_MAX+UiMain / mark_ancestors_dirty 语义），单支 ADR + 草案 §6 修订一次过；保持推荐案自述纪律「先写 host 钉板测试再动签名」（tests/layout.rs 11 测先扩后改）。",
   "severity": "中"
  },
  {
   "conflict": "AST 流无版本标记。新 kind（Grid）与 flags 新位（patch op）有 loud 拒面兜底（UnknownKind desc.rs:449、UnknownFlags desc.rs:451-453，旧码拒新流安全）；但给既有变体加定长域（如 Col 4B→5B 对齐位）会让旧解码器把新域字节读成 child_count、后续整体错位——产出非确定 BadParent/Truncated 而非 loud 版本拒。incremental 第⑤步用 flags 空位做 patch 是安全的，但【样式 API 形态】的原子位打包与对齐枚举若走定长扩展就踩雷——.bap views/ 资源（裁决 F）落地后跨版本流回放即坏。",
   "point": "① 结构——TLV AST 无版本域，Props 定长扩展会静默错位（非 loud 拒）",
   "resolution": "把格式纪律写进 GUI-6 草案：Props 扩展只允许两条路——新 kind 字节，或 flags 新位门控的延长块（新位旧码 UnknownFlags 拒）；禁改既有变体定长。tests/reconcile.rs 加「跨版本流拒面」样本（旧码读新流必 Err）。与 .bap manifest 三值版本域（ADR #56）同构对齐。",
   "severity": "中"
  },
  {
   "conflict": "现状是两趟：后序 intrinsic 测量（measure layout.rs:141-185，intrinsic 直读子缓存 meas :188-217）+ 前序摆放（place :304-451，final_size :289-301 吃满/内在分派）。案1 约束协商要求约束下行与尺寸上行同走一棵——measure/place 必然合并为单趟协商，intrinsic/child_agg/final_size/place_children 约 300 行内核全部重写，非「加约束类参数」级增量。测试面全数重钉：tests/layout.rs 11 测 + offscreen golden 逐字节对拍 + anim measured==0 断言；若十字轴默认档不是 stretch（现状吃满语义 final_size :296-299），全部 golden 即破。",
   "point": "① 结构——案1 与案3 非同一算法结构，「两个完备度」低估迁移量",
   "resolution": "GUI-6 只落案3（真增量，两趟结构保留）；案1 按 layout.rs 内核 v2 立项（relayout 签名外壳不变，内核换单趟协商），排期按重写非升级；案1 草案钉死「十字轴默认 = stretch」保 golden 兼容，golden 重钉单列验收项。",
   "severity": "中"
  },
  {
   "conflict": "incremental 第③步断言「签名升级只动 lib.rs 一个文件的两个函数（set_props lib.rs:557 / set_text lib.rs:601）」。实际 anim.rs apply 的 Width/Height/Gap 分支（anim.rs:206-254）直改 props + layout_dirty，是第三个几何变更入口；mask 分派不覆盖它 = 动画期间退回视图级失效——而 Layout 型动画帧恰是 S7 spike 属性级收益的主度量场景（推荐案第⑦步自己列的度量面），漏斗缺口直接污染 spike 结论。",
   "point": "⑤ 属性级缝——升级漏斗清单缺 anim.rs 第三几何入口，「只动两个函数」不成立",
   "resolution": "改推荐案 incremental 第②步为三漏斗明示：set_props/set_text/anim::apply(Layout 型)；anim apply 改产 mask 走同一分派（或明示 scoped-out 并在 S7 度量协议中剔除动画帧，二选一写死）。",
   "severity": "中"
  },
  {
   "conflict": "案3 称 SwipeView「地基已就位（Row 页 + Transform 偏移）」，但 v1 无定尺寸容器：Rect 是叶子不可为父（lib.rs:464-467 BadParent、desc.rs:588 叶带子拒），Col/Row 主轴恒内在（final_size layout.rs:296-299）——页无法定到 viewport 宽，SwipeView 缺最小表达面。容量实证支持推荐案风险 #5 并更紧：launcher 5 页×24 格×2-3 节点（图标 leaf 现无 Image 视图，ViewKind 六件 desc.rs:47-54 不含图标叶——还需加 kind）≈ 240-360，逼 MAX_NODES=256 帽；DESC_MAX=4096 对 120 个 Text（6..73B/个，desc.rs:21）+ 容器 ≈ 3-9KB 逼近/越帽——风险表只列 MAX_NODES 漏 DESC_MAX。",
   "point": "①+⑥ 容量与表达力——SwipeView 页无定尺寸容器；MAX_NODES/DESC_MAX 双帽 B2 风险表只报了一半",
   "resolution": "GUI-6 布局件范围盒加两件：容器定尺寸最小面（Col/Row 增 w/h Option，走上条 TLV 扩展纪律）+ Image/Icon 叶 kind（新 kind 字节安全）；launcher 树规模实测提前到 GUI-6 立项时（B2 已有）；若升 MAX_NODES→512 则 CAP→1024（layout.rs:33/:85-99）连带 MeasCtx 双位图 2KB + old_abs [Option<RectI>;1024]=16KB 栈面须审计，DESC_MAX 同刀同步升——全并入冻结面 ADR 一次过。",
   "severity": "中"
  },
  {
   "conflict": "在途刀范围 = guifbd serve 升格/present·hdi 域/gfxdemo/smoke-image `GFX ok`（GUI-2段③草案 §0）——文件面在 user/bins/guifbd + 新 gfxdemo + tools/smoke，与 berry-ui/src 零交集（guifbd 是 bins 独立 musl 工作区、berry-ui 是 host members crate），按规划 :97 文件互斥纪律可并行，无冲突。唯一真实耦合：layout.rs:196-199 直引 berry-gfx FontSize 做 len×cell 单源，GUI-2 增件⑧（shape/图集/pass 三段解耦，规划 :88）若改字体面，布局测量面震荡——推荐案已把 TextMetrics 列为前置，但没定时序锚。",
   "point": "④ 时序——GUI-2 段③文件面零冲突，耦合点在 berry-gfx FontSize 与 TextMetrics 落地序",
   "resolution": "把「TextMetrics 缝 + monospace 首实现」钉进 GUI-6 布局刀前段（先于 GUI-2 增件⑧ 落地）；GUI-6 与 GUI-2 段③按文件互斥并行（对齐规划 :97），互不阻塞。",
   "severity": "低"
  },
  {
   "conflict": "建议案 trait TextMetrics { run_width(&self, bytes, scale) } 以 FontSize 枚举为参——把 berry-gfx 三档字体枚举焊进布局协议；增件⑧ shape 产物（glyph run + advance 表）注入时 advance 不按 FontSize 档位组织，trait 要破签再动一次冻结面。",
   "point": "③ no_std——TextMetrics trait 草案签名把 FontSize 焊进布局协议",
   "resolution": "trait 参数改抽象档位（u8 scale id 由实现侧解释），FontSize 映射留在 monospace 定宽第一实现内部；trait 以 &dyn 或泛型进 relayout（进参方式并入冻结面 ADR 清单）。零堆分配结论不受影响：约束类枚举 Copy、单条目缓存、arena 不动均核实无冲突。",
   "severity": "低"
  }
 ],
 "summary": "直读 berry-ui 全部 4359 行（11 模块 + 10 测试文件）+ GUI-3 草案 §6/§10 + 规划 GUI-3/GUI-6 行逐点核对 B2 推荐案。五问结论：① 结构冲突——树表示假设完全吻合（arena 槽表 lib.rs:123-153 + gen 代数 NodeId，推荐案「维持现状」零成本）；失效模型假设吻合（视图级 dirty 位 + props_geom_equal lib.rs:172 全库唯一变更分类点，核实为真）；算法层案3=真增量、案1=layout.rs 内核重写（后序 intrinsic 测量 layout.rs:141-185 + 前序摆放 :304-451 两趟结构在约束协议下必须合并为单趟协商），推荐案「同一模型两个完备度」低估了案3→案1 的迁移量；且 §6 冻结面影响清单比推荐案自认的（Node 字段+measure 签名）更大——relayout 冻结签名（§6:315-316）、Props/TLV、MAX_NODES/DESC_MAX、UiMain 全连带。② 语义冲突——布局 settle 不进也不需要进 SETTLE（SETTLE 唯一判定源 = present_pending frame.rs:70-72 / lib.rs:884，布局工作是 Run 信号且同步收口在 render_frame 内，frame.rs:62-74 真值表不受影响）；全场 relayout 的脏区 = 现有机制已覆盖（set_viewport→FULL lib.rs:417、apply_desc→FULL lib.rs:1008、几何变→逐节点新旧绝对界 layout.rs:341-356 + set_props 父界超集 lib.rs:587-591 + K≤8 满折 FULL damage.rs:295-298），推荐案与之零冲突；唯一的正确性级冲突在「relayout 边界截断」规则写错了对象（见 conflicts 高档项）。③ 零堆/no_std——无硬冲突：约束类枚举 Copy、单条目缓存进 Node、arena 不动、环防御帽沿用；需注意 CAP 位图/快照栈占用随 MAX_NODES 上调放大。④ GUI-2 段③——文件面零交集（在途刀 = user/bins/guifbd serve 升格 + present/hdi 域 + gfxdemo + smoke-image `GFX ok`，GUI-2段③草案 §0；berry-ui 是 host members crate），可并行；唯一耦合 = TextMetrics 第二实现依赖 GUI-2 增件⑧（shape 解耦），时序上缝应先落。⑤ 属性级缝——基本真（props_geom_equal 单点 + DepTable 记录不消费 dep.rs:36/:70 + key 透传 desc.rs:111/:245 + flags 未知位 loud 拒 desc.rs:451-453），但漏了第三个几何变更漏斗 anim.rs apply（Width/Height/Gap 直写 layout_dirty，anim.rs:206-254），且 TLV 定长扩展无版本域是缝纪律缺口。B2 引用锚抽验全部命中（layout.rs:82/:150/:175/:188/:400、lib.rs:59/:331/:557/:601/:1003、damage.rs:234/:363、frame.rs:31-38、dep.rs 头注）。",
 "verdict": "pass-with-changes。方向正确、证据锚真实：树表示（arena+代数 NodeId）、视图级失效+props_geom_equal 分类点、DepTable 记录缝、key 透传、脏区引擎与三态停帧的咬合分析全部与 4359 行实码核实一致，案3 作 GUI-6 v1 子集是纯增量可直落；但按原文照写会引入一个正确性级错误（「Rect w/h 双 Some 截断 mark_ancestors_dirty」在父消费子尺寸的聚合语义下要么 no-op 要么漏排兄弟，且真截断点在 measure() 的 child_affected 返回——半成品 mctx.changed 死数据 layout.rs:181 就是为此现成的缝）、冻结面清单报少四项（relayout 签名/Props-TLV 版本域/MAX_NODES+DESC_MAX/UiMain，须单支 ADR 一次过）、TLV 无版本域下 Props 定长扩展会静默错位须立「flags 门控延长块或新 kind」纪律、属性级升级漏 anim.rs 第三漏斗会污染 S7 spike 度量面、案1 的单趟协商是内核重写非增量且十字轴默认必须钉 stretch 保 golden。上述六项修入推荐案文本后，可作 GUI-6 布局刀立项基座。"
}
```
