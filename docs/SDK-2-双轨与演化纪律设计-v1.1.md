# SDK-2 双轨与演化纪律设计 v1.1

> **性质**：行为变更双轨制、版本域演化件（target-ABI-revision / 能力集标签 / abi 冻结快照）与废弃登记的机制设计（立法档）。
> **溯源**：SDK-03 原则 2+3（对抗复核修正表述已吸收）+ 演化纪律面课（逃生门装进二进制 / 软迁移不 scale / living standard 快照）；史实见 SDK-03 §二/§四。
> **状态**：设计定稿 · **P0 已开工 2026-11-11**（P0-1 信封 TLV/信任账本 + P0-2 自研 ed25519 原语两刀入 base，沿革二百三十一/二百二十三）· 预 ADR（§八）；实施随 SDK-2 主刀。
> **基座**：ADR #56 三值版本域（min/target/compile，mkapp 已执法）· ADR #54 协议面纪律 · ADR #59 manifest v2 三域 · abi.toml 单源。

## 一、行为变更登记表（双轨的载体）

新设 `docs/API行为变更登记表.md`（随 SDK-2 建，CI 引用）。**每条服务语义
变更必须先登记后合入**——无登记条的语义变更按章程 §六 CI 挡。

| 字段 | 说明 |
|---|---|
| id | `BC-YYYY-N` 单调编号 |
| 面 | 影响的服务/子系统（fs/net/sched/appd…） |
| 旧语义 → 新语义 | 可执行描述（含可测判据） |
| 轨 | **compat**（挂 target 分档）或 **security**（全量即时） |
| 生效条件 | compat 轨：target ≥ N；security 轨：装载即生效 |
| 迁移指引 | 链接到文档条目（无迁移指引不许登记） |

**轨判定判据**（判例制，登记时写明）：安全/隐私缺陷修复、能力模型收紧的
安全面 → security 轨；执行策略、默认值、语义细化、性能语义 → compat 轨。
拿不准 → security 轨（宁紧勿松），并在登记条注明争议点供复议。

## 二、target-ABI-revision（逃生门装进二进制）

- manifest v2 **distribution 域**增 `abi_revision`（u16，v1 起始 = 1）：
  应用构建时锚定的 BerryOS ABI/语义快照号。与 `[compat].target`（SDK 语义
  版本，ADR #56）**并存分工**：target 管 SDK 语义档，abi_revision 管平台
  语义快照——前者开发者主动声明，后者构建工具自动打戳。
- manifest v2 **distribution 域**同节增 `platform_compat = [min, max]`
  （u16 对，缺省 = 无界）：**应用声明的平台兼容承诺区间**。与 `abi_revision`
  的分工 = **「事实/承诺」二分**——abi_revision 是构建工具自动打戳的
  **事实**（锚定的平台语义快照号，不可自报），platform_compat 是开发者
  主动声明的**承诺**（自认可运行的平台语义区间，随声明演进）；两键并存
  不混用，防「平台兼容性」三口径（ADR #82 V2 预埋，闸 A 件；键位落码
  与 mkapp 打戳转正随 #77）。
- appd 装载期读取：security 轨变更无视之（即时生效）；compat 轨变更按
  `target` 分档；abi_revision 用于遥测与分发门槛统计（§五）。
- **立法窗口**：生态为零，装机成本为零；事后补装需全量重打包（Linux 课）。

## 三、能力集标签（SysCap 同构，防多形态碎片化）

- SDK 元数据为每个 API 打能力集标签，v1 先两集：**core**（rich 档必备）
  / **full**（含 PC 扩展）。词表域扩 `requires_caps` 声明（闭集文法）。
- 安装期：mkapp/appd 子集校验——设备集不含所需能力即拒装，统一
  `ERR_CAP_SET`（附因：缺的能力集标签）。
- 运行期：**特性探测而非版本探测**（Web 课）——提供 `berry_feature_query`
  式每特性探针（入口存在性检查）；**不给「读系统版本号」推荐路径**；
  三档 gfx 后端差异用能力位表达（ADR #51 既定方向）。
- 穿戴同族/工业档落地时只增标签集，已上架应用零重打包（Android 反例教训）。

## 四、abi.toml 冻结快照与可引用锚（living standard 配套）

- abi.toml 保持活文档（WHATWG 形态已裁对）；**每次变更出冻结快照**：
  变更 commit 生成 `abi@<short-sha>.toml` 快照 + 内容 hash，作为测试锚
  与外部可引用点（WHATWG 自动出版快照做法）。
- 快照进 `docs/` 或随 release 归档（工具刀落位）；黄金字节测试钉在快照
  hash 上，防「活文档悄悄漂移」。
- 配套守卫：CI「无手写副本」检查——内核/berry-sys/文档/校验器四产物全部
  由 abi.toml 生成（RFC-0190 课；原型层 IDL 单源 v2 挂账另案，SDK-03
  空白面④）。

## 五、渠道 target 门槛杠杆

- 依赖分发链基建（SDK-2-分发与签名链设计）：上架门槛随年度 SDK 抬
  target；低报存量包对新用户降可见性（Play 2018/2026 政策对应物）。
- 无商店阶段（v1）：appd 装载期对 `target < min_support` 的包**警告装载**
  并遥测统计（不拒装——无渠道可拒），abi_revision 分布进遥测，为未来
  门槛政策供数据。

## 六、废弃登记（与章程 §三闭环）

废弃条目进登记表同库（`DEP-` 前缀）：废弃于/移除于双版本 + 迁移文档
链接 + 轨=compat（target 门控移除）。CI：L2 面 diff 出现 `#[deprecated]`
而无对应 DEP 条 → 挡。

## 七、验收线（SDK-2 开刀时）

1. 登记表落库 + CI 挡位接线（无登记条语义变更不可合入——以 code review
   检查项起步，工具化随刀）。
2. manifest v2 distribution 域 `abi_revision` 字段 + mkapp 打戳 + appd
   读取链 host 测。
3. `requires_caps` 闭集文法 + 安装期校验负样例。
4. `berry_feature_query` 探针最小集（gfx 后端能力位）host 测。
5. abi 冻结快照工具刀：变更→快照→hash 钉黄金字节一次走通。

## 八、ADR 落册点

- #79（候）：行为变更双轨 + 登记表立法（含轨判定判据）。
- #80（候）：abi_revision + 能力集标签 + 特性探针（「逃生门+防碎片化」
  一揽子）。与 ADR #56 关系：三值域管构建，本 ADR 管运行/分发侧消费。

## 修订记录

- v1.1（2026-11-10）：§二 增 `platform_compat = [min,max]` 键位 + 与
  `abi_revision`「事实/承诺」分工句（ADR #82 附录一 V2 预埋，闸 A 件；
  转正随 #77）；文件名/抬头同步升版，`docs/索引.md` 同步。
- v1.0：首版（SDK-02 深研立法窗，2026-11）。
