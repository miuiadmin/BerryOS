# SDK-1a 实施设计草案（manifest v1：三值版本域 + fs/net 词表 + 免费两件）

> 【状态】草案（主会话批准后冻结为实施契约——三实施线并行编码的唯一事实源）
> 【溯源】ADR #55（SDK 软件标准基线）+ ADR #56（两项进取拍板——本刀最高立法）+
> ADR #57（crates.io 王命，发布动作另刀）+ `research/SDK-01-软件标准与SDK综合裁决.md`
> + 需求核验清单（2026-10-30，/tmp/sdk1a-req.md）+ 代码基线报告（2026-10-30，
> /tmp/sdk1a-code.md）。立法效力序：#56 > SDK-01 §四保守推荐（已全部被 #56 推翻）。
> 【产制】2026-10-30 workflow 编排（合成员：实施设计草案合成员）。行号 = 2026-10-30
> 工作区实测（基线 commit 9857e19 + 他会在途改动）。

---

## 0. 范围、红线与工作区边界

### 0.1 本刀交付（= 需求清单 R1–R4 全量）

| # | 交付件 | 归属实施线 |
|---|---|---|
| D1 | manifest schema v1 增量：`[compat]` 三值版本域 + `fs`/`net` 词键 + `[security]` 预留节（add-only） | A（schema）/B（消费） |
| D2 | mkapp 编译期执法（三值链 + 词表 + 规范形扩位）+ `.bap` 第三条目 `SHA256SUMS` + `--new`/`--check` | A |
| D3 | appd 翻译面：fs 四元装配真授能、net loud 拒、compat min 运行检查、`bap_scan` 三条目形 | B |
| D4 | appdemo fs 子自证（`/etc/mtab` 读回对拍）——「词表 = 镜子」执法活证 | B |
| D5 | `docs/应用开发指南.md`（新）+ README/CLAUDE.md 前置补 musl target | C |

### 0.2 红线（违者 = 设计错了，上报不动手）

1. **kernel/ 与 arch/ 零改动**。本刀全部改动落在用户态（svcmgr/demo app/berry-manifest）
   与工具链（mkapp.py）——与 ADR #43「应用 = 纯用户态协议约定」同构。
2. **禁写文件**（多会话纪律，工作区含他会在途改动）：`.cargo/config.toml`、
   `.github/workflows/ci.yml`、`arch/aarch64/src/task.rs`、`kernel/src/` 全部、
   `justfile`、`tools/mkimage.py`、`tools/smoke.sh`、`docs/` 下已修改九件
   （GUI里程碑规划-2026-10 / 安全与硬件隔离设计 / 应用模型设计 / 文件系统设计 /
   更新与OTA设计 / 架构决策 / 索引 / 阶段沿革 / GUI 相关）、`research/` 全部、
   `deny.toml`、`tools/unsafe_audit.py`、`user/berry-ksproto/`（在途 crate——
   **本刀不依赖其任何接口**）、`user/userbins/`（编译产物区）。
3. **禁 git commit/push/add**（主会话统一提交）。
4. **smoke 零漂移承诺**：`tools/smoke.sh` 与 `justfile` 本刀零改动；rootfs 相
   标记集合、相计数、`DEADLINE_ROOTFS`（现 420s，smoke.sh:712 注）全部不动。
   本刀新增串行日志行（§6）均与在册标记互非子串（附录 B 核对表）。

### 0.3 需求核验清单 §6 七开放点的裁决落位（全收口，一处不留悬）

| 开放点 | 裁决 | 落位 |
|---|---|---|
| ① 三值版本轴 v1 域 | **SDK 契约级**语义版本 `x.y.z`；`SDK_CURRENT = 0.1.0`；与 berry_* ABI 号 / 服务 wire 版无耦合 | §2.1 |
| ② fs/net v1 键形与域 | 数组形（v2 目标态同形，免二次改形）；fs 词表 v1 闭集 `{"rootfs"}`、net 闭集 `{"client"}` | §2.2 |
| ③ sha256 载体 | `.bap` 内第三 cpio 条目 `SHA256SUMS`（否决 manifest 注入/sidecar，理由 §2.4） | §2.4 |
| ④ 预留位形态 | 独立 `[security]` 节，键 `profile`/`key_lineage`，值域 `[a-z0-9-]{1,16}` | §2.3 |
| ⑤ 规范形定序 | `[app]`→`[compat]`→`[capabilities]`(console,fs,net)→`[runtime]`→`[security]` | §3 |
| ⑥ 验收标记形 | **不动 smoke.sh**；执法链 = appdemo 退出码 0 ⟺ fs 自证过（被 `APP ok n=` 谓词吸收）+ 日志证据行 `app APPDEMO fs ok 23`（宿主 grep 留另刀） | §6/§8 |
| ⑦ 指南落位 | 新文档 `docs/应用开发指南.md`；契约清单归指南；索引注册 = 主会话回写（O1，文件锁定） | §9/§10 |

---

## 1. 总览

### 1.1 分层执法（谁验什么——一次定死，防三实现各说各话）

| 层 | 执法面 | 不执法面 |
|---|---|---|
| **berry-manifest parse**（纯语法） | 节/键枚举、SdkVer 文法、词表闭集、[security] 形状、数组文法 | **不做** min≤target≤compile 链比较、**不比** SDK_CURRENT（parser 不知「当前级」语义） |
| **mkapp**（编译期，hvigor 00303015 对应物） | 语法全量 + 三值链 `min ≤ target ≤ compile ≤ SDK_CURRENT` + 规范形逐字节 + SHA256SUMS 产出与 `--check` 复验 | 运行时行为 |
| **appd**（运行时装载） | `min > SDK_CURRENT` 拒（`eco: app <id> compat min`——fsmgr `diag("geom")` 同式 loud 降级行）；net 词 loud 拒；fs 词真授能 | target/compile（工具链面向，打包期已闸） |

三值语义对应物（需求 §1.1）：`target` = 应用声明「按哪一级平台行为契约测试」——
v1 无行为分叉，占位契约；`min` = 运行下界（appd 唯一运行时检查项）；`compile` =
打包所用工具链级（mkapp 闸「不能面向未来 SDK 编译」）。

### 1.2 本刀唯一结构性决定：svcmgr 主走查「app 批前移、bsh 延后」

**问题**（代码基线 §4 推论，本草案确认为最大结构约束）：app 启动批
（svcmgr main.rs:952-981）在主走查毕执行，而 `bin/bsh` arm（:754-870）在走查内
spawn bsh 且 **spawn Ok 后关 `fs_a_h`**（:853-856——「bsh 自此是 fs_a 队列唯一
读者（防回复混流）」）。app 批要给 app 装 fs gift，结构性拿不到安全端点：

- `dup(fs_a)` 留 svcmgr 表 = 与 bsh 同队列双读者 = fsproto 严格串行 RPC
  （无请求号）下应答偷取/跨客户错配——bsh 的 FS ok/fat 族/M3DEMO 自验与 app 批
  **真实并发**（C-1 刀 3 stdhello 先例 :561-565 正是为机制性排除这种交叠）。
- 需求面三选（基线报告 §4）：a) app 批挪 bsh spawn 前；b) fsmgr 开第二对端点；
  c) 赌窗口静默。**裁 a**：b) 改 fsmgr 服务面 = 超本刀归属（fsmgr 不在任一实施线
  白名单）；c) 违「标记不得建立在竞速上」铁律，直接排除。

**裁 a 的实施形 = bsh 延后拉起**（不是 app 批挪进走查——app 批依赖走查收集完的
槽表，物理上只能在走查后）：

```
原序：走查（内联 spawn bsh + 关 fs_a_h）→ app 批（无 fs 可授）
新序：走查（bin/bsh arm 只记录几何）→ app 批（fs_a_h 仍持，app 为唯一
      活动 fs 客户端）→ bsh 拉起（spawn + 关 fs_a_h——原 arm 体逐字搬移）
```

安全性论证（逐项核对过在途改动面）：

| 并发对象 | 新序下的状态 | 依据 |
|---|---|---|
| stdhello / U1 自验 | 走查**前**已死/已毕（stdhello 预拉阻塞 reap :561-633；u1_selftest :644 先于走查） | 交叠面为零 |
| hexyl / vprobe arm | 走查内、app 批前，均**无 fs 通道**（hexyl :695-740 `extra_gifts: &[]`；vprobe 仅 mmio_vmo） | 不涉 fs_a 队列 |
| app 批内 app | 串行：app_launch 对 console=true 阻塞 reap、console=false collect_eof+reap——**任一时刻至多一个 app 存活发 fs RPC** | fs_a 队列单活动客户 |
| svcmgr 自身 | 阻塞在 reap/collect，不发 fs RPC（fs_client 尚未 take，躺在表里不发请求） | 句柄在表 ≠ 活动读者 |
| bsh | 尚未 spawn | 拉起后才成唯一读者 |
| ps/BC 账本 | `ps_selftest` 位于 :126（fsmgr 预拉之前）——账本断言窗口**先于本刀一切重排** | PS ok n=13 / BC ok 不受影响（§8 复核） |

**精确改动谱**（impl-B，全部在 svcmgr main.rs 同一函数作用域内）：

1. 走查前声明 `let mut bsh_geom: Option<(u64, u64)> = None;`（data_off/data_len）。
2. `bin/bsh` arm 体（:757-870）整段搬出为走查后块；arm 内改为：
   `if bsh_geom.is_some() { eco_skip(b"bsh dup"); } else { bsh_geom = Some((data_off, data_len)); }`
   （重复 `bin/bsh` 条目 = 可见降级——较现状的双 spawn 是小收紧，无行为回退）。
3. 走查尾兜底关 `fs_client`/`fs_a_h` 的块（「归档无 bin/bsh」注释段）**加
   `bsh_geom.is_none()` 门**——bsh 将拉起时不得提前关（fs_client 现为 0 才跳过的
   隐式条件在延后态失效，必须显式）。
4. `if app_n > 0 { … }` 批块**之后**新增 `if let Some((off, len)) = bsh_geom { <原
   arm 体逐字> }`——体内 `data_off`/`data_len` 换绑 `off`/`len`，其余（fs_client
   take、关 fs_a_h :853-856、netd_dup 现造现用、ps_reg）零改动。

时序影响：BSH ok 及 bsh 驱动族标记（FS ok/fat 族/U1..U4/TAB/INT/M3DEMO 等）整体
晚 app 批一个身位（TCG 下 app 批约 +2–5s：appdemo 数次 fs RPC ≈ 0.8s/次）。
smoke rootfs 相标记**集合**不变、到齐序不敏感（轮询 `missing` 清空即过，
DEADLINE_ROOTFS=420s 余量两个数量级）；RX 相注入以标记到齐为前置，自然顺延。

---

## 2. schema v1 全量定义（add-only：v0 冻结面一字不动）

v0 冻结面（`docs/应用模型设计.md:111-128`）全部保留：`[app]` 五键必填、
`[capabilities]` console、`[runtime]` 整节可省、语法子集（LF only / 拒 tab /
双引号串 / 纯十进制整数 / 未知节键一律硬拒）。以下五个平面为**纯增量**；
「未知键放行」永不发生（add-only = 已知键集枚举式增长，需求 §1.3.1）。

### 2.1 `[compat]` 节——三值版本域（ADR #56 ①）

- 节头 `[compat]` 走现行节头文法（与 `[app]` 等同一解析路径，`Sec` 枚举
  （berry-manifest lib.rs:176-181）增 `Compat`/`Secu` 两变体）。
- **键 = `min` / `target` / `compile` 三键，齐备或齐缺**——部分在场（含仅一/二键、
  重复键）= 拒 `bad compat`（11B）。整节缺席 = `compat: None`（v0 旧包路径，
  零行为漂移——R1.4 缺省语义：缺 ≡ 按 SDK_CURRENT 处理，appd 不分支）。
- **值 = SdkVer 串**（双引号，`parse_string` 同文法：可打印 ASCII、无反斜杠/
  内嵌引号）。SdkVer 文法：

```
SdkVer   := seg "." seg "." seg        # 恰两个单字符 '.' 分隔
seg      := "0" | [1-9][0-9]{0,2}      # 1..3 位纯十进制、无前导零、值 0..=999
```

  文法天然封顶 11 字符（`999.999.999`），无需另设长度帽。**app 自述 `version`
  键（≤16B 自由串）与 SdkVer 无关，不动**——勿混淆。

- **语义**：三值 = **BerryOS SDK 契约级**（manifest schema + 词表 + appd 翻译面
  行为契约的整体版本——开发者面对的唯一版本号，hvigor compileSdk/targetSdk 与
  Android SDK 版本的同位物）。**与 berry_* syscall ABI 编号、服务 wire 协议版本
  无耦合**（那些各自版本化，v1 无映射义务）。
- **比较序**：`(major, minor, patch)` 字典序。

### 2.2 `[capabilities]` 增 `fs` / `net` 两键（ADR #56 ②）

- 键形 = **作用域字符串数组**（v2 目标态 `fs = ["$appdata"]` / `net = ["client"]`
  同形，`docs/应用模型设计.md:143-144`——v1 直接采数组形，免 v1→v2 二次改形）。
- **v1 词表闭集**：`fs ∈ {"rootfs"}`、`net ∈ {"client"}`。词表外任意值
  （含 `$appdata`——v2 物）= 拒 `bad scope`（9B）。非数组形（`fs = "rootfs"`）、
  逗号多元素、空数组、非引号串 = 拒（tomllib 侧类型错 / 解析器侧 `bad scope`）。
- **数组文法**（berry-manifest 侧）：`[` + 双引号串 + `]`，恰一元素；值域整体
  trim 容忍与现行 bool 同口径（`[ "rootfs" ]` 解析收——规范形门禁 2 拒，两级
  执法与 `console =  true` 双空格现状一致）。
- **语义（三档决策规则，需求 §2 与本草案 §5 联合立法）**：

| 档 | 处置 | 立法依据 |
|---|---|---|
| (a) 词在表 + 服务在席 + 可安全授能 | **真授能**：v1 仅 `fs = ["rootfs"]` 落此档——appd 四元装配（§5.2） | #56 ②「翻译官义务先行」+ 词表 = 镜子 |
| (b) 词在表 + 服务不可安全授能 | **schema/mkapp 收、appd loud 拒**：v1 `net = ["client"]` 落此档——打包过、装载打 `eco: app <id> net unserved` 降级行（fsmgr `diag("geom")` 同式），挂账 §10 O3 | 镜子原则的诚实形态：词表先行 ≠ 无服务硬授 |
| (c) 声明了词却静默授不出/静默吞 | **绝对禁止**——空头支票即违 #55/#56；任何「收下 manifest 但 spawn 时悄悄少装 gift 且不降级」的实现形态 = 设计错误 | #55 反面清单位 |

- 「没申请就没有 handle」模型不变（R2.4）：manifest 仍是唯一授权入口；
  `fs` 缺席 = spawn `extra_gifts: &[]` + `stdin_feed: None`（与 v0 全同——
  appquiet 即活证，预期零改动）。
- **v1 权限边界（如实声明，审查 minor 补）**：`fs = ["rootfs"]` 授的 =
  **与 bsh 同权的全卷读写删**（rootfs VMO READ dup + fs 通道 SEND|RECV
  dup——fsproto 全 op 面含 MPUT/TRUNC/UNLINK，作用于挂载 FAT 卷全卷），
  无 per-app 隔离；per-app 私有目录语义是 v2 `$appdata` 词（v1 词表硬拒
  `bad scope`）。文档三处同口径：本节 / 应用开发指南 §6 表 · §7.6。

### 2.3 `[security]` 预留节（ADR #56 随刀件二）

- **键 = `profile` / `key_lineage` 两键，齐备或齐缺**；部分在场 = 拒
  `bad security`（12B）。
- **值 = 双引号串，1..=16B，字符集 `[a-z0-9-]`**（小写字母数字连字符——v2
  密钥别名/谱系标识形制预留，如 `"ed25519-main"` / `"root-2026"`）。越字符集/
  超长/空串 = 拒 `bad security`。
- **语义 = 纯占位**：v1 appd 解析通过即忽略（零行为、零输出）；签名实现仍 v2
  （#44 不动）。**防范的正是「v2 签名落地时字段错位」**——节/键/值域今天定死，
  v2 签名方案无论怎么设计都只能填进这个壳，不能另起炉灶挤占字段位
  （Android 签名 v1→v4 事故驱动的反教训，#56 原文）。

### 2.4 `.bap` 第三条目 `SHA256SUMS`（ADR #56 随刀件一）

- **载体裁决**：`.bap` cpio(newc) 内独立第三条目，名恰 `SHA256SUMS`。否决余二：
  - manifest 注入段：digest 是**打包期派生值**，进开发者手写 manifest 必破门禁 2
    「规范形重序列化逐字节相等」（需求 R3.1 约束 a 的假拒面）——需发明「派生段
    豁免规则」= 规范形纪律自伤，弃。
  - 包外 sidecar：需 justfile 在 rootfs 装配面加接线 = 本刀禁写文件，结构性弃。
- **内容格式**（逐字节）：

```
行 := <64 位小写十六进制 sha256> "__" <条目名> LF     # "__" = 恰两个空格
     # 行按条目名字节序升序（"bin/<entry>" < "manifest.toml"）
     # 恰好两行；末行带 LF；无头行/无脚注
```

  （`sha256sum` GNU 文本惯例双空格形态——宿主 `sha256sum -c` 可直接消费。）
- **条目定序**：.bap 内 cpio 条目仍按 name 字节序（mkapp build() :213-218 现行
  纪律）：`"SHA256SUMS"`(0x53…) < `"bin/<entry>"`(0x62…) < `"manifest.toml"`(0x6d…)。
- **确定性**：无时刻/无路径输入，同输入同工具链同字节（V-3 纪律自动继承）。
- **消费分层**：
  - mkapp 产包时无条件生成（三条目为新常态）；
  - `mkapp --check` 复验（重算逐文件摘要 + 逐字节比对 + 形状断言，§4.4）；
  - **appd v1 不验摘要**，只放形状（`bap_scan` 恰 2 或 3 条目、第三条目名恰
    `SHA256SUMS`，§5.3）。三条明示理由：① berry-manifest/svcmgr 均零依赖
    no_std sha256 实现，本刀两实施线白名单不含新 crate；② 无签名的摘要不是
    安全边界——rootfs 自身未签名，能改 .bap 者能改 rootfs，消费侧复算只添
    TCG boot 成本不添威胁面覆盖；③ 真正执法点 = v2 安装期签名验证（摘要+
    签名一体），届时此条目即「全文件覆盖」底座（#56 括注原意）。
- **app-dir 内容约束不变**：仍恰 `{manifest.toml, bin/}`（mkapp build()
  :180-182）——`SHA256SUMS` 是 mkapp 产出的**包内**条目，不是开发者可写文件；
  开发者 app-dir 里出现同名文件 = `未知内容` 拒（现有门禁自动覆盖，零新增）。

### 2.5 berry-manifest 公开 API 契约（impl-A 实现 / impl-B 消费——字段级冻结）

```rust
pub const SDK_CURRENT_STR: &str = "0.1.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SdkVer { pub major: u16, pub minor: u16, pub patch: u16 }
// Ord 派生 = (major, minor, patch) 字典序（appd 比较用）。
// 形状钉子（编译期，TCPS_PATTERN 定值同款）：
const _: () = assert!(SdkVer::parse(SDK_CURRENT_STR).is_some());

impl SdkVer {
    pub fn parse(s: &str) -> Option<SdkVer>;   // §2.1 文法；不涉 SDK_CURRENT 比较
}

pub const SDK_CURRENT: SdkVer = SdkVer { major: 0, minor: 1, patch: 0 };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compat { pub min: SdkVer, pub target: SdkVer, pub compile: SdkVer }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Security<'a> { pub profile: &'a str, pub key_lineage: &'a str }

// Manifest<'a> 现有六字段一字不动，纯增四字段（svcmgr 现消费面零破坏）：
pub struct Manifest<'a> {
    /* …现 v0 六字段原样… */
    pub compat: Option<Compat>,
    pub fs: Option<&'a str>,            // 词表闭集 ⇒ 值域 {"rootfs"}
    pub net: Option<&'a str>,           // 值域 {"client"}
    pub security: Option<Security<'a>>,
}
```

新增解析拒绝理由（全部 ≤ REASON_MAX=24B，`ManifestErr` 定长内联路径）：
`bad compat`（节内任何不齐/文法错）、`bad scope`（fs/net 数组文法/词表外）、
`bad security`（节不齐/值域外）。理由登记进 `err_reason_truncation` 测试清单
（lib.rs:783-792 现役理由表同步扩）。

`SDK_CURRENT` 的**单一事实源 = berry-manifest**；mkapp 侧镜像常量（§4.1）+ 对账
注释（mkfat ↔ berry-fatfs 同款义务，mkapp.py 头注 :33-34 扩一行）。抬级流程 =
改 berry-manifest 两常量 + mkapp 镜像 + 指南文档（§9 O 表）。

---

## 3. 规范形 `canonical()` 逐字节（门禁 2 比较基准）

### 3.1 定序与发射算法（规范性定义——mkapp Python 实现逐字钉死）

节序（缺省节跳过、节间恰一个空行、节内键定序、无尾随空行/空行）：

```
[app]        id, version, entry, type, intent        # 五行定序（v0 现行，不动）
[compat]?    min, target, compile                     # 值从解析三元组重发射 str(int)
[capabilities]  console, fs?, net?                    # fs/net 数组无空格形
[runtime]?   stack_pages                              # v0 现行，不动
[security]?  profile, key_lineage
```

mkapp `M.canonical()` 新实现（替换 mkapp.py:86-98，Python 逐字）：

```python
def canonical(self) -> str:
    out = (
        "[app]\n"
        f'id = "{self.id}"\n'
        f'version = "{self.version}"\n'
        f'entry = "{self.entry}"\n'
        'type = "app"\n'
        f'intent = "{self.intent}"\n'
    )
    if self.compat:
        mn, tg, cp = self.compat
        out += "\n[compat]\n" \
               f'min = "{mn}"\ntarget = "{tg}"\ncompile = "{cp}"\n'
    out += f"\n[capabilities]\nconsole = {'true' if self.console else 'false'}\n"
    if self.fs:
        out += f'fs = ["{self.fs}"]\n'
    if self.net:
        out += f'net = ["{self.net}"]\n'
    if self.has_runtime:
        out += f"\n[runtime]\nstack_pages = {self.stack_pages}\n"
    if self.sec:
        p, k = self.sec
        out += f'\n[security]\nprofile = "{p}"\nkey_lineage = "{k}"\n'
    return out
```

要点：① 兼容三元组**存 int、发射 `str(int)`**——文法禁前导零 ⇒ 归一化恒等，
不存在「收下但重发射不同」的输入；② 旧形（五平面全缺省）输出与现行
canonical() **逐字节相同**——BASE 类正对照零漂移；③ 五平面发射相互独立拼接，
组合律由代码结构保证（32 种组合 = 算法积，无组合特判）。

### 3.2 字节级模板（七形态钉板——每平面单独 + 全组合；`⏎` = LF）

**F0 v0 旧形（全缺省，现行 BASE——一字不变，add-only 执法锚）**

```
[app]⏎
id = "X"⏎
version = "0.1.0"⏎
entry = "bin/x"⏎
type = "app"⏎
intent = "utility"⏎
⏎
[capabilities]⏎
console = true⏎
```

**F1 +[compat]**（F0 的 `[capabilities]` 行前插入）

```
[app]⏎
id = "X"⏎
version = "0.1.0"⏎
entry = "bin/x"⏎
type = "app"⏎
intent = "utility"⏎
⏎
[compat]⏎
min = "0.1.0"⏎
target = "0.1.0"⏎
compile = "0.1.0"⏎
⏎
[capabilities]⏎
console = true⏎
```

**F2 +fs**（`console` 行后增一行）

```
…（[app] 六行同 F0）…⏎
[capabilities]⏎
console = true⏎
fs = ["rootfs"]⏎
```

**F3 +net**（fs 若在同则居前；单独形态）

```
…（[app] 六行同 F0）…⏎
[capabilities]⏎
console = true⏎
net = ["client"]⏎
```

**F4 +[security]**（尾部追加）

```
…（[capabilities] 段同 F0）…⏎
⏎
[security]⏎
profile = "ed25519-main"⏎
key_lineage = "root-2026"⏎
```

**F5 +[runtime]**（= appquiet 现行 manifest 减其真实值——仓内活样本，零改动）

**F6 全组合 kitchen-sink（mkapp/berry-manifest 双侧测试样本）**

```
[app]⏎
id = "KITCHEN"⏎
version = "0.1.0"⏎
entry = "bin/kitchen"⏎
type = "app"⏎
intent = "utility"⏎
⏎
[compat]⏎
min = "0.1.0"⏎
target = "0.1.0"⏎
compile = "0.1.0"⏎
⏎
[capabilities]⏎
console = true⏎
fs = ["rootfs"]⏎
net = ["client"]⏎
⏎
[runtime]⏎
stack_pages = 8⏎
⏎
[security]⏎
profile = "ed25519-main"⏎
key_lineage = "root-2026"⏎
```

其余 26 组合 = 上述平面按 §3.1 定序机械叠加，无独立形态（测试覆盖由
`--selftest` 单增样本 × 算法分段结构保证，见 §7.2）。

### 3.3 双实现一致性

berry-manifest（Rust）**不实现 canonical 发射**（v0 现状即无——它是纯解析器）；
规范形执法集中在 mkapp 门禁 2。berry-manifest 的对偶义务 = **把 §3.2 各形态
文本当输入解析并断言字段值**（`canonical_forms_parse` 测试，§7.1）——「mkapp
产的字节 berry-manifest 必收」双向钉板，防两实现漂移（mkfat ↔ berry-fatfs
fixture 共识同型）。

---

## 4. mkapp 增量（tools/mkapp.py——impl-A）

### 4.1 常量镜像与 `M` 视图扩容

```python
# ---- 帽与词表（对账锚点：user/berry-manifest/src/lib.rs）----（:49-64 扩）
SDK_CURRENT = (0, 1, 0)
SDK_CURRENT_STR = "0.1.0"
FS_SCOPES = ("rootfs",)
NET_SCOPES = ("client",)
VER_RE = re.compile(r"^(0|[1-9][0-9]{0,2})\.(0|[1-9][0-9]{0,2})\.(0|[1-9][0-9]{0,2})$")
SEC_RE = re.compile(r"^[a-z0-9-]{1,16}$")
```

`M.__init__` 增字段：`self.compat`（`None | (min, target, compile)` int 三元组）、
`self.fs`/`self.net`（`None | str`）、`self.sec`（`None | (profile, lineage)`）。
`canonical()` 换 §3.1 实现。

### 4.2 门禁 1 扩容（check_manifest_text :112-172）

- 未知节集合（:125）`{"app","capabilities","runtime"}` → 增 `"compat"`、`"security"`。
- 新增 `[compat]` 检查块（镜像 berry-manifest 语义 + **链执法**）：

```python
compat = doc.get("compat")
if compat is not None:
    if set(compat) != {"min", "target", "compile"}:
        raise Reject("manifest: [compat] 应为 min/target/compile 三键齐备")
    vs = []
    for k in ("min", "target", "compile"):
        v = compat[k]
        if not isinstance(v, str) or not VER_RE.fullmatch(v):
            raise Reject("manifest: compat 值应为 x.y.z（每段 1-3 位十进制、无前导零）")
        vs.append(tuple(int(g) for g in VER_RE.fullmatch(v).groups()))
    if not (vs[0] <= vs[1] <= vs[2]):
        raise Reject("manifest: compat 序不合（需 min ≤ target ≤ compile）")  # hvigor 00303015 对应物
    if vs[2] > SDK_CURRENT:
        raise Reject(f"manifest: compile {vs[2]} 超工具链当前级 {SDK_CURRENT_STR}")
```

- `[capabilities]` 检查（:147-151）改：键集 ⊆ `{"console","fs","net"}` 且含
  `console`；`fs`/`net` = 长度 1 的 str 数组且元素在词表内：

```python
if not {"console"} <= set(caps) or set(caps) - {"console", "fs", "net"}:
    raise Reject("manifest: [capabilities] 键集应为 console(+fs/net)")
for key, vocab in (("fs", FS_SCOPES), ("net", NET_SCOPES)):
    if key in caps:
        v = caps[key]
        if not (isinstance(v, list) and len(v) == 1 and isinstance(v[0], str)
                and v[0] in vocab):
            raise Reject(f"manifest: {key} 应为单元素数组，词表 {vocab}")
```

- 新增 `[security]` 检查块：两键齐备或齐缺、值 `SEC_RE.fullmatch`。
- 产包摘要行（build() :224-228 print）尾随增量：`compat=0.1.0..0.1.0`/
  `fs=rootfs`/`net=client`（在场才打——宿主侧可见性，无消费面）。

### 4.3 SHA256SUMS 产出（build() 改）

`items`（:213-218）由两项扩三项：

```python
import hashlib
sums = "".join(
    f"{hashlib.sha256(data).hexdigest()}  {name.decode('ascii')}\n"
    for name, data in sorted(payload_items)          # 按条目名排序的两行
).encode("ascii")
items = sorted(payload_items + [(b"SHA256SUMS", sums)])
```

（`payload_items` = manifest + bin 两件；排序后条目序 = `SHA256SUMS` < `bin/…` <
`manifest.toml`，§2.4 定序自动满足。）

### 4.4 `--check`：既有 .bap 复验（新子命令）

用法：`mkapp.py --check <path.bap>`——只验不写。步骤（任一失败 = 一行理由 +
非零退）：

1. 读整包 ≤ BAP_MAX；本地 newc 走查器解码条目（~30 行，仅本命令用——**不改
   mkcpio.py**（不在白名单），也不复用 svcmgr 侧逻辑）。
2. 形状断言：条目名集合恰 `{"manifest.toml", "bin/<唯一>"} ∪ {"SHA256SUMS"?}`；
   允许恰 2 或 3 条目（2 = SDK-1a 前旧包，历史产物可验；3 = 新常态）。
3. manifest 全量门禁复跑（check_manifest_text——含三值链/词表/规范形逐字节）。
4. `SHA256SUMS` 在场则复验：内容格式断言（两行、双空格、行序）+ 逐文件重算
   sha256 逐字节比对；条目与摘要行一一对应（多行/少行/余名 = 拒）。
5. ELF 魔数复验。通过 = `mkapp: <path> OK <n>B entries=<k> digests=<ok|n/a>`。

### 4.5 `--new`：应用脚手架（新子命令）

用法：`mkapp.py --new <dir> --id <ID>`。约束：`basename(dir) == ID.lower()`
（否则一行拒——id 大写惯例对齐 demo 双 app：APPDEMO ↔ appdemo）；目录已存在且
非空 = 拒（防覆写）。产出四件（**不含 bin/ 内容——产物是「不可打包的头less CLI
源码工程」**，开发者构建 ELF 放入 `bin/` 后才可打包；`--new` 不产 ELF、不造
bin/ 占位文件——空 bin/ 会被 build 的「恰一个文件」门禁拒，这是有意的两步流）：

`<dir>/manifest.toml`（= §3.2 F1 + `[runtime]`，字节钉板；`<slug>` = basename）：

```
[app]⏎
id = "<ID>"⏎
version = "0.1.0"⏎
entry = "bin/<slug>"⏎
type = "app"⏎
intent = "utility"⏎
⏎
[compat]⏎
min = "0.1.0"⏎
target = "0.1.0"⏎
compile = "0.1.0"⏎
⏎
[capabilities]⏎
console = true⏎
⏎
[runtime]⏎
stack_pages = 8⏎
```

`<dir>/src/main.rs`（appdemo main.rs:17-30 同构——CRT0_SEED 纪律是链接命脉，
一字不减）：

```rust
//! <ID> —— BerryOS 应用（SDK-1a 脚手架；契约见 docs/应用开发指南.md）。
use std::process::ExitCode;

// 链接种子（stdhello/appdemo 同款）：无引用则 berry-libc 的 crt0（含
// `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

fn main() -> ExitCode {
    // argv0 契约：spawn name = manifest entry（app-0 §5）。
    assert_eq!(
        std::env::args().next().as_deref(),
        Some("bin/<slug>"),
        "argv0 should be manifest entry"
    );
    println!("app <ID> up");
    ExitCode::from(0)
}
```

`<dir>/Cargo.toml`（appdemo Cargo.toml:1-23 同构）：

```toml
[package]⏎
name = "<slug>"⏎
version = "0.1.0"⏎
edition = "2021"⏎
⏎
[workspace]⏎
⏎
[dependencies]⏎
berry-libc = { path = "../../berry-libc" }⏎
⏎
[profile.release]⏎
opt-level = "z"⏎
lto = true⏎
codegen-units = 1⏎
panic = "abort"⏎
strip = true⏎
```

（path 深度按 in-tree 位置 `user/bins/<id>/` 钉死 `../../berry-libc`；脚手架到
别处 = README 指引调整，工具不猜。）

`<dir>/README.md`：编译咒语（§8.2 指南同款原文）+ 构建→放 bin/→`mkapp.py
<out>.bap <dir>`→`--check` 四步 + 契约三件（argv0/EOF 先行/退出码）速览 +
in-tree 接线说明（进 rootfs 需 justfile build-user 挂账位——本刀不接线，
见 §10 O7）。

`--new` 产出的 manifest 本身必须过全部门禁（selftest 断言：scaffold → 伪 ELF
入 bin/ → build 成功 → --check 通过，全链 roundtrip）。

### 4.6 selftest 扩容（:242-287）

- 正对照 2 → 9：现 2 件（BASE / BASE+runtime，**BASE 字节零漂移 = add-only
  执法样本**）+ F1(+compat) + F2(+fs) + F3(+net) + F4(+security) + F6
  (kitchen-sink) + `--new`→build→`--check` roundtrip（tempfile.TemporaryDirectory，
  bin/ 放 4 字节 `\x7fELF` 伪桩）+ 旧形 2 条目 .bap 过 `--check`（历史形状兼容）。
- 负样本 9 → 22，新增 13：compat 三键不齐 / 序不合（min>target）/ compile 超
  SDK_CURRENT（`0.2.0`）/ 值形（`01.0.0`、`1.2`、`1.2.3.4`）／fs 词表外
  （`tmpfs`、`$appdata`）/ fs 非数组（`fs = "rootfs"`）/ net 词表外（`server`）/
  [security] 键不齐（仅 profile）/ [security] 值域（`a b`、17B）。

---

## 5. appd 翻译面（user/bins/svcmgr/src/main.rs——impl-B）

### 5.1 时序重排（§1.2 全文即契约，此处只补 appd 视角）

`app_launch` 签名增 `fs_a: Handle` 参数（批块 :956-963 调用点传 `fs_a_h`——
新序下批块执行时 fs_a_h 仍持；fs 授能不可达值 0 传参，§5.2 降级路径消化）。

### 5.2 `app_launch` 装配谱（:2333-2435 增量，检查序钉死）

双守卫（id :2358 / entry :2362）之后、console 两态（:2369-2376）之前/之后的
**精确插入序**：

```
1. compat min 检查（插双守卫后）：
   if let Some(c) = man.compat {
       if c.min > berry_manifest::SDK_CURRENT {
           eco_app(console, id, b"compat min");   // 10B ≤ 24B
           return false;                          // console=false 时 out_s 未建，零清理
       }
   }
2. net loud 拒（档 (b) 执法——fsmgr diag("geom") 同式）：
   if man.net.is_some() {
       eco_app(console, id, b"net unserved");     // 11B
       return false;
   }
3. console 两态（现行 :2369-2376 原样）。
4. fs 四元装配（console 两态后、spawn 前）——uu_run :2118-2161 逐点复刻：
   let wants_fs = man.fs.is_some();
   let mut rootfs_dup: Handle = 0;
   let mut fs_dup: Handle = 0;
   if wants_fs {
       // cleanup-pair = 若 out_s != 0：显式关 out_s + stdout 两值（现行
       // spawn Err 面 :2395-2404 同款）后 return false；console=true 时
       // out_s==0 零清理直退。
       if fs_a == 0 { eco_app(console, id, b"fs none"); cleanup-pair; }
       if berry_handle_duplicate(H_ROOTFS, RIGHT_READ, &mut rootfs_dup) != 0 {
           eco_app(console, id, b"fs dup"); cleanup-pair;
       }
       if berry_handle_duplicate(fs_a, RIGHT_SEND | RIGHT_RECV, &mut fs_dup) != 0 {
           let _ = berry_handle_close(rootfs_dup);        // 半途失败两枚都试关（fs_gifts 同款）
           eco_app(console, id, b"fs dup"); cleanup-pair;
       }
   }
5. SpawnSpec（:2377-2393 增两处）：
   stdin_feed:  if wants_fs { Some(&[]) } else { None },   // extras 前提（berry-exec :446-449 assert）
   extra_gifts: if wants_fs { &[rootfs_dup, fs_dup] } else { &[] },
6. spawn Err(()) 面：现行 out pair 显式补关（:2395-2404）之前，先关
   rootfs_dup/fs_dup（Err 全在 env send 前 = 未 take，fresh 值都在）。
7. spawn Ok 面：**两 dup 已被 take 随 env 块迁移，勿关**（旧值 -EBADF——bsh arm
   :847-849 同注释义务）；console=false collect_eof/close（:2414-2417）与
   reap（:2421-2424）原样。
```

**handle 来源与权限位钉板**（= bsh `fs_gifts()` :3550-3562 / svcmgr `uu_run`
:2121-2129 同源，零发明）：

| gift | 来源 | 权限位 | libc 落位 |
|---|---|---|---|
| rootfs_dup | `dup(H_ROOTFS, …)`——H_ROOTFS=0x0001_0002 boot 直插定值（:193），svcmgr 随时可 dup | `RIGHT_READ` | env hs[4] → `GIFT_H[0]` → `rootfs_h()` |
| fs_dup | `dup(fs_a, …)`——fsmgr 预拉对（:441-508）a 侧；新序下 app 批窗口 svcmgr 仍持 `fs_a_h` | `RIGHT_SEND \| RIGHT_RECV` | env hs[5] → `GIFT_H[1]` → `fs_h()` |

（berry-exec gift[4+i] 装配 :460-470 / berry-libc io.rs :61-102 对偶——**零改动**，
extras 2 枚走既有槽位。）

**随行纪律**：dup 产物归 svcmgr 表所有；Err 面全关、Ok 面 take 迁移不关
（uu_run :2147-2159 样板）；无跨 syscall 引用生命期假设（handle 表所有权即纪律，
内核 4-1 睡眠钉住纪律不涉——appd 不睡这俩 handle）。

**容量核算**：extras 恰 2 ≤ 3 帽（berry-exec :438-449 双 assert：slice ≤4 且
4+2=6 ≤ 7 槽）✔；fs+net 假想满配 3 = 恰 7 槽结构性打满——**net 永不达 spawn**
（步骤 2 先拒），v1 实际峰值 2。附带效应（如实记载）：extras 在位连带装配
T-1 自描述对 gift[2]/[3]（self_proc/self_vmar）——单线程 app 不消费、pthread
EAGAIN 守门语义正确，无害（berry-exec :459-466 现行注释同判）。

**诊断行预算**：`eco: app <id> <why>` 行（eco_app :2001，64B 窗）——why 新增
四词 `compat min`(10)/`net unserved`(11)/`fs none`(7)/`fs dup`(6) + 解析器
三词（§2.5），全部 ≤24B；最坏行 `eco: app ` + 24 + 1 + 24 + 1 = 59B ≤ 64B ✔。

### 5.3 `bap_scan` 三条目形（:2261-2324）

- 走查循环形状判从「恰 manifest.toml + 唯一 bin/ 前缀」放宽为「恰 2 或 3 条目；
  第三条目名**恰 `SHA256SUMS`**（重复 manifest/bin、第四条目、其它名仍
  `bap shape` 拒——消费侧独立执法不松口，rootfs 是外部输入面）」。
- `BapScan` 结构增 `has_sums: bool`（v1 只记账在场、不读内容不验摘要——§2.4
  三理由）；`elf_off/elf_len/mf_len` 几何记账原样。
- 2 条目形（SDK-1a 前历史包）照收——`--check` 侧同口径，双侧一致。

---

## 6. appdemo 扩展（user/bins/appdemo/**——impl-B；appquiet 预期零改动）

### 6.1 新 manifest.toml（字节钉板，199B；= §3.2 F1+F2 组合；≤MANIFEST_MAX=1024）

```
[app]⏎
id = "APPDEMO"⏎
version = "0.1.0"⏎
entry = "bin/appdemo"⏎
type = "app"⏎
intent = "utility"⏎
⏎
[compat]⏎
min = "0.1.0"⏎
target = "0.1.0"⏎
compile = "0.1.0"⏎
⏎
[capabilities]⏎
console = true⏎
fs = ["rootfs"]⏎
```

**appquiet manifest.toml 一字不动**——它是 v0 旧形活样本（add-only 运行面执法
证据 + berry-manifest `fixture_appquiet` :763 真源钉板）。两 manifest 为
impl-A（include_str 测试）/impl-B（文件本体）交界面——**字节以本节/§3 为准，
impl-B 写、impl-A 只消费不复制**（§9 冲突协调）。

### 6.2 fs 子自证（main.rs 增量——确定性状态谓词）

选材 = **`/etc/mtab`**：fsmgr 启动预建 RAM 树条目（fsmgr main.rs:68-82，
`MTAB_LINE = "/dev/root / fat rw 0 0\n"` :77 恰 23B）——RAM 树故**盘相无关**
（FAT 子目录/持久化/B4 三 boot 全相位恒在）、EFI 路恒在、内容是 C-2 刀 2 钉死的
定值——比盘文件多一层相位免疫，是系统内唯一同时满足「fs 通道真 IO + 全相位
确定性」的小文件。

main() 增量（`println!("app APPDEMO up")` 之后、`ExitCode::from(0)` 之前）：

```rust
// fs 词自证（SDK-1a）：libc open/read 经 fsclient → fs_h gift（env hs[5]）
// → fsmgr RPC 真往返——「词表 = 镜子」执法面：manifest 声明 fs 且 appd 装了
// 四元 gift，此读才可能成功。mtab = fsmgr RAM 树预建定值（盘相无关）。
let mtab = std::fs::read("/etc/mtab").expect("appdemo: mtab read");
assert_eq!(mtab, b"/dev/root / fat rw 0 0\n", "mtab 内容漂移");
println!("app APPDEMO fs ok 23");
```

失败形态全 loud：读失败 → `expect` panic → release `panic="abort"`（Cargo.toml
:22）→ libc abort → EOF 先行 + 退出码 134 → appd `reap != Ok(0)` →
`eco: app APPDEMO exit code` → **`APP ok n=2` 缺失 = smoke FAIL**。即：
**`APP ok n=2` 谓词把 fs 授能吸收进退出码链**——不需要任何 smoke.sh 新断言
（§0.2 红线 4 的兑现机制）。`app APPDEMO fs ok 23` 行内含断言长度（23B），
内容漂移时 assert 先炸——行文本即证据（宿主 grep 断言留另刀，§10 O2 附记）。

`std::fs::read` 全链可行性：libc open/read 真实现 + fsclient RPC（C-1 刀 4 起
bsh cat/ls 同面）；musl 堆分配 Vec——stack_pages=8 缺省页（manifest 省
[runtime] 路径）不变（23B 读 + Vec 微量，hexyl 32 页教训不适用此量级；若首跑
实测爆栈，唯一允许的调整 = manifest 加 `[runtime]` stack_pages 显式抬页——
属本节字节模板修订，须回写本草案 §6.1）。

### 6.3 argv0/EOF/退出码

全沿现行契约不动（argv0 断言 :23-27、EOF 先行经 `ExitCode::from(0)` 真退出、
reap 0 进 `APP ok`）。

---

## 7. 测试清单

### 7.1 berry-manifest host 测试（23 → 34，新增 11 项）

| # | 测试 | 断言 |
|---|---|---|
| 24 | `sdkver_parse` | 三段合法全过（`0.1.0`/`999.999.999`/`0.0.0`）；前导零/四段/两段/字母/空段/4 位段全 None |
| 25 | `sdk_current_shape` | `SDK_CURRENT_STR` parse == `Some(SDK_CURRENT)`；`(0,0,999) < (0,1,0) < (0,1,1) < (0,2,0) < (1,0,0)` 序钉板 |
| 26 | `compat_parse_pin` | BASE + `[compat]` 三键 `0.1.0` → `Some(Compat)`，min/target/compile 逐字段相等 |
| 27 | `compat_absent` | BASE（v0）→ `compat/fs/net/security` 全 None——**add-only 执法锚** |
| 28 | `compat_rejects` | 仅两键/仅一键/重复键 → `bad compat`；值 `1.2`/`1.2.3.4`/`01.0.0`/`1.02.0`/`1000.0.0`/`1.2.x`/裸词（无引号）→ `bad compat` |
| 29 | `fs_scope` | `fs = ["rootfs"]` → `Some("rootfs")`；`["tmpfs"]`/`["$appdata"]`/`[""]`/`["rootfs","rootfs"]` → `bad scope`；`fs = "rootfs"`（非数组）→ 拒 |
| 30 | `net_scope` | `net = ["client"]` → `Some("client")`（**parser 收**——档 (b) 执法在 appd）；`["server"]` → `bad scope` |
| 31 | `caps_unknown_key` | `[capabilities]` 出 `display = true` → `unknown key`（枚举式增长执法） |
| 32 | `array_grammar` | `[ "rootfs" ]`（trim 容忍）解析过；`[ " rootfs" ]`（串内空格）→ `bad scope`；单引号 `['rootfs']` → 拒 |
| 33 | `security_shape` | 两键 + `[a-z0-9-]{1,16}` 值 → `Some(Security)`；仅 profile/仅 key_lineage → `bad security`；`"a b"`/17B/空串/大写 → `bad security` |
| 34 | `canonical_forms_parse` | §3.2 F0–F6 七段文本（测试内字面常量）→ parse Ok + 字段值逐项断言（mkapp 发射字节 ↔ 本解析器双向钉板） |

现役测试改造两处：`fixture_appdemo`（:750）断言扩 `compat == Some(…0.1.0…)` ∧
`fs == Some("rootfs")`；`err_reason_truncation` 理由表（:783-792）追加
`bad compat`/`bad scope`/`bad security`。`fixture_appquiet`（:763）**零改动**
（文件未动）。BASE（:453）零改动。

### 7.2 mkapp selftest（2 正 + 9 负 → 9 正 + 21 负）

见 §4.6（正对照含 BASE 字节零漂移锚 + --new/--check roundtrip；负样本 12 新增
覆盖三值链/词表/security 全拒面）。

---

## 8. 验收线

### 8.1 host（`just test-host`）

berry-manifest 34/34 + mkapp selftest（9 正/21 负）+ 既有全绿零回归
（berry-efi/berry-fatfs/…不涉）。门禁自检挂载位不动（justfile :464 现状）。

### 8.2 产物（`just build-user`）

- appdemo/appquiet 重打包：两 .bap 恒 3 条目（+`SHA256SUMS`）；
- `python3 tools/mkapp.py --check <staging>/APPDEMO.bap` / `APPQUIET.bap` →
  `OK … entries=3 digests=ok`（staging 路径 = userbins/ app-stage 区，
  justfile :130-210 现行布局，本刀零接线改动）。

### 8.3 定向 run-rootfs（`just run-rootfs`）

期望序（相对基线的增量全列）：

1. 全部在册标记到齐（集合不变——含 `APP ok n=2`、`PS ok n=`、`BC ok`）；
2. **新证据行 `app APPDEMO fs ok 23`** 恰一次（app APPDEMO up 之后）；
3. `eco: app` 降级行**零出现**（任何 `compat min`/`net unserved`/`fs …` 都 = FAIL）;
4. BSH ok 及下游 bsh 族标记晚于 APP ok（§1.2 新序的形态确认）;
5. 无 `TAGTAB ok` 类子串假阳性（新行与标记互非子串——附录 B 已核对）。

EFI 路（`just run-efi`/smoke-image）同面：fsmgr 在席（FS ok）→ fs gift 成立 →
新行照出；ECO canon 计数不变（新行非标记）。B4 三 boot：mtab RAM 树盘相无关，
新行恒出，`FATW ok`/毒环链零扰动。

### 8.4 smoke 零漂移（`just smoke` 五相）

smoke.sh/justfile 零改动；rootfs 相 41 标记（工作区现值）全到齐；RX 相注入以
标记到齐为前置自然顺延；金丝雀严格递增不受影响（app 批与 bsh 均在 boot 窗口内
串行）。**本刀不新增任何 smoke 断言**——§6.2 退出码链是唯一执法面。

### 8.5 红线自检（每实施线收尾必过）

`git diff --stat` 核对：kernel/ 与 arch/ 零行；§0.2 禁写清单零触碰；改动面 ⊆
§9 归属表。

---

## 9. 文件归属表（三实施线并行——越表 = 冲突，上报不硬抢）

| 实施线 | 可写文件 | 预期改动量 |
|---|---|---|
| **A（schema+工具）** | `user/berry-manifest/src/lib.rs`、`tools/mkapp.py` | lib.rs +~200 行（§2.5 API + 11 测）；mkapp +~180 行（§4 全部） |
| **B（appd+apps）** | `user/bins/svcmgr/src/main.rs`、`user/bins/appdemo/**`、`user/bins/appquiet/**` | svcmgr §1.2 重排 + §5 三件 ≈ +90/移 60 行；appdemo manifest 199B 重写 + main.rs +8 行；**appquiet 预期零改动**（列入防误伤） |
| **C（文档）** | `docs/应用开发指南.md`（新）、`README.md`、`CLAUDE.md` | 指南 ~150 行；README +1 行（:42 rustup 行补 musl）；CLAUDE.md +1 行（前置节同款） |

**冲突协调**：

1. appdemo/appquiet 的 manifest.toml = A/B 交界面：字节唯一事实源 = 本草案
   §3.2/§6.1；B 写文件本体，A 的 `fixture_*` 测试经 `include_str!` 消费（断言
   字段值，不复制字节）——无第二份字节拷贝，无漂移面。
2. `SDK_CURRENT` 双侧：A 在 berry-manifest（源）+ mkapp（镜像）各落一次，
   B 只 `use berry_manifest::SDK_CURRENT`（经 crate，不复制字面量）。
3. B 编译依赖 A 的 §2.5 API 落地——**编排序：A 第一棒（schema+mkapp 先合），
   B/C 第二棒并行，集成验收第三棒**；B 若先行，只写调用面、以本草案 API 契约
   为准绳合入前自查签名。

**实施线内禁改清单**（红线重申）：§0.2 全部 + `user/bins/bsh/**`（fs_gifts 是
被复刻的样板，不是改动点）、`user/berry-exec/**`、`user/berry-libc/**`、
`tools/mkcpio.py`、`tools/smoke*.sh`、`user/bins/fsmgr/**`（§1.2 裁决 b 的依据
即「不属本刀」）。

---

## 10. 挂账与开放点

**本草案已定死、无需再议**：需求 §6 七开放点全裁决（§0.3）；sha256 载体/词表
域/版本轴/规范形定序/验收标记形/指南落位/预留节形态——实现线遇到与本草案冲突
的「更好想法」= 上报主会话，不得自行改约。

**留主会话（文件锁定，非设计悬置）**：

- **O1 文档回写**：`docs/应用模型设计.md` §3 v1 词表/版本域/[security] 三面
  同步 + `docs/索引.md` 注册应用开发指南（第三阅读路径「应用开发者路径」）+
  `docs/阶段沿革.md` 追记——三文件均他会在途禁写，随本刀提交由主会话一并落。
- **O2 结构知会**：svcmgr 主走查时序重排（bsh 延后）——验收者/后续刀须知标记
  到达序变化（BSH ok 晚于 APP ok）；如需宿主侧 grep `app APPDEMO fs ok 23`
  作独立证据层，属 smoke.sh 改动 = 主线串行另刀。

**挂账（后续刀设计题，本刀只立账）**：

- **O3 net 服务面**：`net = ["client"]` 的真授能路径 = vnet 多端点化或 appd
  中继层（A4 在途 socket/getaddrinfo 面与本账汇流后统裁）；在那之前 appd loud
  拒（§5.2 步骤 2）。
- **O4 SHA256SUMS 消费侧校验 + Merkle 全树**：随 v2 签名（安装期验证一体落地）。
- **O5 AGENTS.md 前置段同步**：CLAUDE.md 补 musl 行后，AGENTS.md 同款一行留
  主会话（两文件一致性纪律）。
- **O6 crates.io 发布动作**（ADR #57）：语义版本/弃用纪律/文档基建随发布刀；
  out-of-tree 开发者模型同刀。
- **O7 第三 demo app**（fs+net 全词活证 / net 授能面回归载体）：需 justfile
  build-user 接线 = 禁写文件，主线另刀。

---

## 附 A 预算核算

| 预算面 | 数值 | 帽 | 判 |
|---|---|---|---|
| manifest 最坏长度（五平面全开：[app] 151 + [compat] 77 + [capabilities] 65 + [runtime] 28 + [security] 74） | 395B | `MANIFEST_MAX`=1024（appd `APP_MF_WIN` :265 同宽） | 余 629B ✔ |
| appdemo 新 manifest | 199B | 同上 | ✔ |
| gift 槽 | extras 2（rootfs_dup+fs_dup）+ 固定 4 = 6 | 7 槽（berry-exec :422/:438-449 双 assert） | ✔（fs+net 假想 3=恰满，net 先拒不达） |
| eco_app 行 | 最坏 59B（9+24+1+24+1） | 64B 窗（:2001） | ✔ |
| ManifestErr 新理由 | `bad compat`/`bad scope`/`bad security` ≤12B | REASON_MAX=24（内联零截断） | ✔ |
| .bap 增量 | +SHA256SUMS 条目 ≈ 160B（2 行×(64+2+名+1) + 头 110B 对齐） | BAP_MAX=4MiB | ✔ |
| rootfs 增量 | appdemo +std::fs 链（估 +2–6KiB）+ 两 .bap 各 +~160B | 现车 ~27MiB / 帽 32MiB（VMO_MAX_SIZE 同口径） | 余 ~5MiB ✔ |
| boot 时序 | app 批 +2–5s（TCG，appdemo 数次 fs RPC ≈0.8s/次） | DEADLINE_ROOTFS=420s | ✔ |
| 内层条目名 | `SHA256SUMS` 9B；cpio 头+名对齐走查步进窗 | svcmgr 内层窗注释「名 ≤56B」 | ✔ |

## 附 B 新串与在册标记互非子串核对（2026-10-30 工作区 ROOTFS_MARKERS 41 项实测）

新增串行日志串全集：`app APPDEMO fs ok 23`（常态）；`eco: app <ID>
compat min|net unserved|fs none|fs dup|bad compat|bad scope|bad security`
（仅降级态，验收线 §8.3.3 要求零出现）。逐项对 41 在册标记（含在途
`LSN ok n=7`/`POLL ok`/`EVS ok n=2`/`SKT ok n=3`/`EPL ok n=7`）大小写敏感
grep 比对：无任何标记为新串子串、新串不含任何标记（`fs ok` 小写 ≠ `FS ok n=`
——大小写敏感面 `grep -qF` :357-371 既有纪律）✔。mkapp host 侧输出
（`mkapp: …`）不落串口，无涉。
