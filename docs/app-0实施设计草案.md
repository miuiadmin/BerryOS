# app-0 实施设计草案（应用模型落地第一刀：manifest v0 + mkapp + appd 雏形）

> 状态：**对抗审查已收官（2026-10-21，workflow 26 agents：4 维度找 → 逐 finding 对抗核实；22 raw → 19 confirmed（跨维去重 14 根因）全修入文 / 3 refuted 留档）**。输入 = `应用模型设计.md` §7 app-0 行（「manifest v0 + `mkapp`（mkcpio 同纪律确定性打包）+ appd 雏形（svcmgr 演化：读 manifest → 授权集 → 拉起 → 监督 reap）| smoke 标记 `APP ok`」）；决策依据 = ADR #43（应用 = 纯用户态协议约定，内核永不认识「应用」）/ #44（cpio 容器 + TOML manifest，安装 = 翻译成 handle 授权集，启动 = env 块 gift 授能）/ #45（apps/ 树 = v2 安装面）。
> 时点：P6 A-7 已收官、x86 会话占 P7-x 线——app-0 的「P6 尾/P7 初」窗口即现在。路径面 = `user/` + `tools/` + smoke/计数锚，与 x86 会话（`arch/x86_64/`、smoke-x86）零重叠。
> 形态先例：`G-eco编译侧设计草案.md`（草案 → workflow 对抗审查 → 拆步实施，每步独立提交）。
> **执行结果 + 冻结（2026-10-23）**：a-0..a-3 全落（2026-10：berry-manifest host 23 测 + mkapp 门禁自检 + svcmgr appd 雏形 `APP ok n=2` = rootfs 相第 21 标记；两义风险回写同日收口——父文档 §3/§4.2 落地注 + 外围位补齐）。本文自此为实施实录冻结档，后续演进不再回写（新变化进 `阶段沿革.md` 与 `应用模型设计.md`）；文件位置不动，归档移动待另行裁决。

## 0. 目标与范围盒子

**一句话**：把「应用」从设计文档里的协议约定变成第一片可跑的代码——`.bap` 包 + manifest v0 + `mkapp` 确定性打包 + appd 雏形（svcmgr 新 arm），全链 = **装（构建期打包）→ 起（manifest 翻译授权）→ 真退出（0x33）→ 监督 reap（0x3A 真码）**，验收标记 `APP ok n=<k>`（确定性状态谓词，不赌竞速）。

**范围盒子（v0 不做，逐条注明去处）**：

| 不做 | 原因 | 去处 |
|---|---|---|
| 运行时安装/卸载（apps/ 树） | fsmgr v1 平面 8.3/NAME_MAX48 无目录语义，硬上 = 假目录 | ADR #45 存储布局 v2（真 appd 分家同往） |
| fs 能力 | fsmgr 单读者纪律：fs_a 已 take 赠 bsh（A-1「防回复混流」）；app 要 fs 需 per-app `channel_pair` + fsmgr 多 client 面 | v1.5/v2（schema 现在就不收 fs 键——词汇缺席优于半吊子） |
| MMIO/IRQ/DMA 能力 | devmgr 三铸件未建 | #46 devmgr v2（`DEVMGR ok`） |
| intent 消费 | 声明入口 syscall 未落（Phase 4 收官话术：intent 声明入口留 v2） | v2；v0 = schema 在册 + 解析校验 + 结构体保留（「记录」） |
| argv[1..]/env 注入 | argv0 已有（`SpawnSpec.name`）；env 注入面（manifest 元数据进 env 块）暂无消费者 | v2（AI工程面 §2.2 SDK 面同期） |
| stdin 交互 | `stdin_feed = Some(&[])` 零长 EOF 是**机械供给**（berry-exec gift[1] 在位 assert 的过门法 + std fd 消毒必需），不是可申请能力 | v2 交互 stdin |
| `type = "driver"` | devmgr 未建，收了就是半吊子 | #46 v2（schema 届时 add-only 扩展） |
| 签名/版本化/分发 | v1 无分发面（#44 明示 v2 加法） | v2 |
| app-1 墓碑 | 独立步（`TOMB ok`） | `应用模型设计.md` §7 app-1 行 |

**能力词汇 v0 封顶一件：`console`（布尔）**。这不是偷懒——翻译表每加一行都要有真实授权机制背书（§5）；v0 的一件恰好能同时演示两面（授予 = console 发端 / 缺席 = 私有采集对），「能力决定形态」的最小活证。

## 1. 包格式：`.bap` v0 = cpio(newc) 容器

- **内布局**：`manifest.toml` + `<entry>`（entry = 恰 `bin/` 前缀的**完整包内路径**，§2 冻结——对抗审查收敛：勿写 `bin/<entry>` 双前缀形态；恰好一个代码文件）；内序 = mkcpio 既有名序（`bin/…` < `manifest.toml`，确定性）。
- **rootfs 嵌入形态**：rootfs.cpio 新增**单条目** `app/<ID>.bap`——包 = 单元（ADR #44 形状 day-one），不拆散件进 rootfs。`app/` 名序先于 `bin/` → 主走查先撞 app 条目（§5 收集期利用这一点）。
- **嵌套消费零暂存**（本设计的关键省力点，两处纯算术）：
  - 内走查：`berry_loader::cpio_step(window, pos, archive_len)` 是纯函数（`user/berry-loader/src/lib.rs:312`）——appd 把读窗基址平移到 rootfs VMO 的 `[bap_off + pos, +want)`、`archive_len = bap_len`，即在位走查 `.bap` 内层，**归档永不整体驻留**（d-1b 流式纪律的嵌套版）。
  - ELF 段泵：`spawn_rootfs_elf(h, spec)` 一切读取都是 `berry_vmo_read(h, …, data_off + …)` 纯偏移算术（`user/berry-exec/src/lib.rs:260/335`）——传 `H_ROOTFS` + `data_off = bap_off + inner_elf_off` 即可，**零 staging VMO、零新 spawn 变体**。
- **尺寸帽**（双端执法，mkapp 门卫 + appd 运行期同值）：

| 项 | 帽 | 说明 |
|---|---|---|
| manifest.toml | ≤ 1024B | 字段表绰绰有余；防敌意巨值 |
| .bap 整包 | ≤ 4 MiB | 走查几何界（防几何炸弹）；appdemo 量级 ≈ 300KB |
| id | ≤ 24B，`[A-Za-z0-9._-]` 首字符字母数字 | rootfs 条目名 `app/<ID>.bap` ≤ 32B（loader 名窗 56B 内） |
| version | ≤ 16B | 记录不消费 |
| entry | ≤ 32B，必 `bin/` 前缀 | 与包内文件一一对应 |
| rootfs 内 app 条目数 | ≤ 4 | appd 静态收集槽 |

帽常量**双侧同步义务**（对抗审查钉板）：上表数值在 mkapp.py 与 berry-manifest 两处各硬编码一份——实现期 berry-manifest 侧 `pub const` 集中导出帽值；mkapp.py 无法 import Rust，门卫注释指向 berry-manifest 常量定义处作对账锚点；**改帽值须双侧同步**（mkfat → berry-fatfs 头注义务同款）。恰满边界正例进 §3 host 测试，防 `<` vs `<=` 漂移静默分叉。

## 2. manifest v0 schema（冻结）

```toml
# BerryOS 应用清单 v0（ADR #44：声明式能力申请表）
[app]
id = "APPDEMO"              # 必填。包身份 = rootfs 条目名 / appd 诊断行用
version = "0.1.0"           # 必填。记录不消费
entry = "bin/appdemo"       # 必填。包内代码路径
type = "app"                # 必填。v0 唯一值（"driver" 随 devmgr v2 add-only）
intent = "utility"          # 必填。冻结六词（见下），v0 只记录不消费

[capabilities]
console = true              # 必填。唯一能力词汇：true → console 发端；
                            # false → appd 私有采集对（输出不进串口）

[runtime]                   # 整节可省
stack_pages = 8             # 可省，缺省 8（std 小程序口径）；1..=64
```

- **与父文档的关系（对抗审查补显式声明）**：本文 v0 schema 是 `应用模型设计.md` §3「manifest v0 草案」的**实现子集 + 词表演进**——type 角色词表 {gui, headless, daemon} 收敛为唯一值 `"app"`（角色值随 v2 GUI/daemon 面按 add-only 回归，与 `"driver"` 同款扩展位）；intent 由「可省 + type 角色表定档」改为**必填**（定档机制挂 v2 type 角色值落地时）；[capabilities] 五键收敛为 console 一键、[ui] 节缺席。三处分叉点与 v2 收敛路径见 §9 对账条目；父文档 §3/§4.2 改写列 a-3 回写义务——否则「manifest v0」一词两文档两义，父文档自己的示例在本 schema 严格拒面下整体非法。
- **intent 冻结词表**（镜像 `sched/src/intent.rs` 的 `INTENT_UNSPECIFIED..INTENT_MAINTENANCE` 常量块——符号锚非行号锚，intent.rs 编辑不漂移；manifest 侧收小写蛇形串）：
  `unspecified`(0x00) / `latency_critical`(0x01) / `interactive`(0x02) / `utility`(0x03) / `background`(0x04) / `maintenance`(0x05)。v0 只做词表校验 + 枚举保留；**运行期 u8 映射表不在此落**（避免与 intent.rs 成双表）——但类型形状 day-one 定型：`Intent` 枚举 `#[repr(u8)]` + 显式判别值（`Unspecified = 0 … Maintenance = 5`，序对齐 intent.rs 常量块，见 §3），v2 消费点 `as u8` 即得正确编码（TPIDR 偏移 / vblk SECTOR_SZ const assert 同款「形状钉子」惯例；无钉子则声明顺序一改 = 六档全错、静默错优先级带）。
- **语法子集（冻结到字节级）**：节头 / `key = value` / 双引号字符串 / 布尔 / 十进制整数 / `#` 注释 / 空白行。**超集语法一律拒**（数组、浮点、内联表、多行字符串、点键、裸键外字符）——「编不出就拒绝，不截断」（fsproto/netproto 编码器同族纪律）。字节级口径清单（对抗审查钉板——tomllib 是 TOML 1.0 全语法，门卫与执法者都按此清单各自执法，不留「tomllib 收而子集拒」的漂移缝）：
  - 换行 **LF-only**——任何 CR 出现即拒（CRLF 是现实可达类：git autocrlf 检出即触发；拒点在 build 门卫 = 早炸而非运行期降级）；
  - 整数 = 纯 `[0-9]+`——拒 `0x`/`0o`/`0b` 前缀、下划线分隔、前导 `+`、负号（stack_pages 无负值语义）；
  - 字符串内**反斜杠一律拒**（v0 无转义语义——`"a\nb"` 在 tomllib 解出真实换行属静默错值类，禁之）；
  - 注释 = `#` 整行与行尾两种形态都收（写明于此消灭二义）；
  - 间距容忍集：等号两侧空格可有可无、行首缩进仅空格（无制表符）、节头内空格仅 `[name]` 名前后各 ≤1——出集即拒；
  - 空 `[runtime]` 节**合法**（= 全缺省，与省节同义）；**节序无关**（解析器不依赖出现序，未知键拒面与之正交）。
- **严格拒面**：未知节 / 未知键 / 重复节 / 重复键 / 必填缺失 / 类型错 / 值域外（stack_pages、词表、长度帽、entry 前缀）。v0 无「宽容忽略」档：**未知键 loud 失败优于半工作**（新键包在旧解析器上炸响 = 显式版本不匹配信号；协商机制随 v2 版本化）。
- **语义校验单源 = berry-manifest（Rust）**。mkapp 的 tomllib 只当 **build 期门卫**（结构预检 + **canonical 重序列化比对**：把解析结果按 §2 规范形态重序列化，须与原文**逐字节相等**，不等 = 非规范形态 = 门卫拒非零退码——这把「tomllib 接受语言 ⊆ 子集语言」变成门卫自身可执法的性质，D6 纪律）。守卫方向两半（对抗审查修正后的口径）：①**正例半** = 真 manifest fixture 共识（§3 item 4）——两枚真 manifest 两端同过，钉「真源文件不落入缝里」；②**负例半** = mkapp 门卫自测断言 tomllib-accepts-but-subset-rejects 样本（点键 / hex 整数 / 下划线 / CRLF / 转义 / 内联表 / 多行字符串）非零退码——原措辞「两枚真 manifest 进 host 测试即钉死运行期必过」夸大了 fixture 共识的守卫方向（它只钉 happy path 单边；mkfat 类比在那里成立因 python 是基准生产者，这里 python 只是门卫）。

## 3. `user/berry-manifest`：纯函数解析 crate（「解析器即产品」第 6 例）

先例家族：berry-loader / fsproto / virtio / blkproto / fatfs / netproto。同族纪律：**零分配、零 unsafe、零依赖、输入只借用、no_std**——宿主是 svcmgr（**no_std 平像程序**：aarch64-unknown-none + elf2flat 嵌内核 .rodata，无 std 无 alloc），berry-manifest 带 std/alloc 依赖则 workspace 构建链直接编译失败（对抗审查修正：原注「svcmgr 是 musl std 程序」失实——走 berry-ld musl 链的是 stdhello/hexyl/bsh/fail/fsmgr/vprobe/vblk/vnet 八个，svcmgr 不在其列）。

- API 面：`parse(bytes: &[u8]) -> Result<Manifest<'_>, ManifestErr>`；`Manifest<'a>` 全借用（`id/version/entry: &'a str`、`console: bool`、`stack_pages: u32`、`intent: Intent` 六值枚举、type 恒 APP 不入结构）。`Intent` 声明 `#[repr(u8)]` + **显式判别值**（`Unspecified = 0 / LatencyCritical = 1 / Interactive = 2 / Utility = 3 / Background = 4 / Maintenance = 5`——序对齐 intent.rs 常量块；枚举定义处注释指向 `sched/src/intent.rs` 单一事实源；a-0 实现加 `const _: () = assert!(Intent::Maintenance as u8 == 0x05);` 编译期钉）。`ManifestErr` 携带一行理由（截断/坏节/未知键…）——**理由 crate 执法帽 ≤24B，超长自身截断后接省略号**（「编不出就拒绝」对诊断行放宽为「截断不 panic」：appd 降级行发射窗按 24B 设计，见 §5）。
- host 测试面（预计 ≈18 测，精确数以 a-0 实现为准）：
  1. **完整钉板**：§2 样例逐字段断言；
  2. **缺省合并**：省 `[runtime]` → `stack_pages == 8`；
  3. **坏输入族**：空输入 / 截断 / 坏节头 / 未知节 / 未知键 / 重复键 / 类型错（`console = "yes"`）/ int 界外（0、65、负数、非数字）/ entry 无 `bin/` 前缀 / intent 野词 / id 超长 / id 非法字符 / version 超长 / 超集语法逐项（数组、浮点、内联表、点键、多行字符串、字符串内反斜杠、CRLF 行尾、hex/下划线整数）+ **恰满边界正例**（fsproto `out_exact_fit` 同族，对抗审查补）：id 恰 24B（首字符字母数字）/ version 恰 16B / entry 恰 32B / stack_pages = 1 与 = 64（1..=64 双端）/ manifest 恰 1024B——恰满接受面无测则帽值 `<` vs `<=` 漂移时任一侧静默分叉、测试照绿；
  4. **真 manifest fixture 共识**：`user/bins/appdemo/manifest.toml` 与 `user/bins/appquiet/manifest.toml` 作为测试输入（§4 双 demo 分别覆盖「省 runtime」与「显式 stack_pages=8」两条真路径）——正例单边钉板（负例半边在 mkapp 门卫自测，见 §2 修正后口径）；
  5. **intent 判别值钉板**：六变体 `as u8` == intent.rs 常量序 0x00–0x05 逐一断言——intent.rs 将来启用保留段 0x06+ 或调序时，这是跨 crate 词表对账的唯一机器锚。

## 4. `tools/mkapp.py` + demo 双 app + 构建链接线

- **mkapp**：`mkapp.py <out.bap> <app-dir>`（app-dir = `manifest.toml` + `bin/<file>` 恰好一枚）。门卫五连（tomllib，python ≥ 3.11——`gen_abi.py` 同门已在役；对抗审查钉板为编号清单）：
  1. manifest 可解析且过 §2 词表/帽检（结构预检，非执法）；
  2. **canonical 重序列化比对**：解析结果按 §2 规范形态重序列化，须与原文**逐字节相等**（不等 = 非规范形态拒）——负例样本（点键 / hex / 下划线 / CRLF / 转义 / 内联表 / 多行字符串）进 mkapp 自测（§2 负例半边在此）；
  3. entry 与包内 `bin/` 下唯一文件的完整路径**整串相等**（即 `bin/<file>`——对抗审查修正：勿写「entry 与文件名一致」，entry 已含 `bin/` 前缀，比较对象是完整包内路径而非文件名）；
  4. out 文件名与 id 一致（`APPDEMO.bap` ↔ `id="APPDEMO"`）；
  5. bin 文件首 4B = `\x7fELF`（防把源码/文档打包进去的傻错；深检 ELF 是 loader 运行期职权，门卫不越权）。

  任一不过 = 非零退码 + 一行原因（**loud not silent**）。
- **单编码器纪律**：mkcpio.py 抽出可导入的 `build_archive(items: list[(name, data)]) -> bytes`（CLI `main()` 原语义原输出不变），mkapp `from mkcpio import build_archive`——**禁止复制 newc 编码/trailer 逻辑**（双编码器 = 漂移面）。`build_archive` 是**纯编码器**（对抗审查钉板契约）：不排序、按传入序编码、ino = 传入序号 + 1——排序归调用方：CLI `main()` 保持既有**整条 `name=path` 字符串**排序（现行为；无重复名/前缀对时与纯名序等价是巧合性输入性质，非工具性质），mkapp 按 name 字节序排后传入。连带修 mkcpio.py 头注「条目按 name 排序」→「按整条 `name=path` 排序」（a-1 同 commit）。
- **demo 双 app**（能力决定形态的两面，`user/bins/` 新包，stdhello 同款 musl 单包布局 + CRT0_SEED 纪律）：
  - `appdemo`：manifest `console = true`、省 `[runtime]`（缺省路径）。main 打一行 `app APPDEMO up` 后 `ExitCode::from(0)` 真退出——console 能力授予面的活证（输出经 console 服务落串口）。
  - `appquiet`：manifest `console = false`、显式 `[runtime] stack_pages = 8`（显式路径）。main 静默真退出——**能力缺席可运行的活证** + appd 私有采集对收集面（EOF-only）。
- **build-user 接线**：musl 链编两 bin → `mkapp` 产中转区 `user/userbins/app/APPDEMO.bap` / `user/userbins/app/APPQUIET.bap`（build-user 已 `cd 用户态`，落既有 `userbins/` gitignore 覆盖下**零 .gitignore 增项**；对抗审查修正：勿落仓库根 `app/`——污染 git status 且不受 ignore 覆盖）→ mkcpio 命令行加两条目，全形 `app/APPDEMO.bap=userbins/app/APPDEMO.bap`、`app/APPQUIET.bap=userbins/app/APPQUIET.bap`（名序自动落 `bin/` 前）。
- **a-1 验收 = 零行为变化**：svcmgr 此步不识 `app/` 条目（主走查对未知名算术跳过——`bin/fail` 同款先例）→ smoke 四跑全绿零漂移 + `just run-rootfs` 手动全标记 + host 计数不变 + **构建后 `git status` 零新增未跟踪产物**（中转区落点受 ignore 覆盖的机器验证——漏 ignore 即 FAIL，G-eco-b「陈旧产物静默上线」同族防线）。

## 5. appd 雏形 = svcmgr 新 arm（`user/bins/svcmgr/src/main.rs`）

- **收集期（主走查循环内，零额外 syscall）**：cpio_step 流式走查对名匹配 `app/` 前缀 + `.bap` 后缀的条目 → 静态槽记 `(id, bap_off, bap_len)`（≤4 枚，超帽 `eco: app too many` 降级行）。`app/` 名序在一切 `bin/` 前 → 收集先于任何拉起 arm 完成。
- **拉起期（主走查全后**——不与 stdhello/hexyl/bsh/vnet/vprobe 各 arm 交错，时序独立；+2 瞬态进程对 PROCESS_MAX=64 余量无扰）：逐枚顺序处理——
  1. **内走查**：读窗基址平移 cpio_step（§1）→ 收 `manifest.toml` 字节（≤1KiB 帽）+ 记 `<entry>` 的 `(off, len)`（entry 本身即含 `bin/` 前缀的完整包内路径——对抗审查修正：原「`bin/<entry>`」照写即双前缀永不命中）；TRAILER 自终止；
  2. **解析**：`berry-manifest::parse` + 校验；Err → `eco: app <id> <理由>` 降级——**发射窗 ≥64B 专用**（对抗审查实抓：`eco: app ` 9B + id 帽 24 + 空格 + 理由帽 24 + 换行 ≈ 59B，现役 `eco_skip` 24B 窗装不下，沿用即 slice panic **杀 svcmgr**；修法 = appd arm 自备 ≥64B 静态窗，或 `eco_skip` 扩容 24→64B 并复核其余调用点零扰动（现役皆 ≤15B 定值串），a-2 实现定夺；收集期 `eco: app too many`（17B）在既有窗内、不受影响）；
  3. **翻译授权集**（v0 翻译表恰一行自由度）：
     | manifest | SpawnSpec |
     |---|---|
     | `console = true` | `stdout` = console handle（spawn 内部 dup RIGHT_SEND，既有） |
     | `console = false` | `stdout` = `channel_pair` 私有对端（appd 持 peer 收输出） |
     | （机械面，非翻译） | `stdin_feed = Some(&[])` 零长 EOF / `name` = entry 串（argv0，stdhello 先例 argv[0]=`bin/stdhello`）/ `stack_pages` 自 manifest / gifts 空 / `mmio_vmo = None` |
  4. **spawn**：`spawn_rootfs_elf(H_ROOTFS, { data_off = bap_off + elf_off, data_len })`；Err → `eco: app <id> spawn`（同 step2 的 ≥64B 发射窗——`spawn` 理由虽短但 id 可达 24B，同样超 24B 窗；解析/首窗失败调用方可见降级，既有契约）；
  5. **collect**（仅 console=false 面）：appd 端收消息至零长 EOF（**机制** = hexyl verify 同款活动帽逐消息刷新「慢而活着不误杀」——对抗审查修正引证：hexyl verify 的帽实为 **10s**（svcmgr `TIMEOUT_NS`），非 30s；**帽值取 30s** 对齐 EXIT ok 家族 `ChildHandles::reap` 的 `REAP_TIMEOUT_NS`——机制先例与数值先例分开引）；
  6. **监督 reap**：wait TERMINATED + `0x3A` 真码（`ChildHandles::reap` 既有，EXIT ok 同链）；**先 collect 后 reap**（PIPE 死锁序铁律的单 app 版）。
- **`APP ok n=<k>` 谓词**（确定性状态谓词）：`k = 发现枚数` ∧ 每枚 spawn Ok ∧ console=false 者 EOF 收齐 ∧ 每枚 `reap == Ok(0)`——全部成立才打；任一环节失败 = 降级行 + **不打标记**（k=0 即无 app 条目 → 不打：裸相本无 rootfs，rootfs 相必含 2 枚）。零竞速面：不赌时序，只看终态。
- **裸跑零扰动**：`run_rootfs_child` 无 rootfs 早退（H_ROOTFS -EBADF），一切照旧。

## 6. smoke / 计数接线

- rootfs 相标记 **20 → 21**（+`APP ok n=2`）；RX 相 **43 → 44**；持久化相 **43 → 44**（**落地值**：草案期基线 17/40/40 为 A-8 后锚，a-0 动工前 TCP 四刀 T-c/T-d 先行落地把基线推至 20/43/43——`ALL_MARKERS` = 23 boot + rootfs 相数组，RX 相与持久化相共用；app 每次 boot 照拉——持久化相复用已写盘、不动 rootfs.cpio，标记照到）。
- 附加串口断言：`app APPDEMO up` 以**独立 grep**（f42t 同款判例——console 通道异步排水但必达，DEADLINE_ROOTFS=300s 帽远大于必达时延，T-d 起 240→300）挂 rootfs 相标记循环**之后**；**不入 `ROOTFS_MARKERS` 数组**（对抗审查：独立 grep 与数组计数是两个断言位，rootfs 相 21 + 独立 1 不冲突）；RX / 持久化相不重复断言（`rootfs hello` 只在 rootfs 相断言的先例）。
- EL2 `21` 前缀同绿；host 计数 200 + berry-manifest 23 = **223** + `mkapp.py --selftest` 挂 `just test-host` 前置（a-3 U4；mkfat `--check-fixture` 同位）。
- 标记互非子串全表核验（a-3 已核）：`APP ok n=` 与既有**全部 44 枚**标记两两互非子串（boot 23 + rootfs 21 = `ALL_MARKERS` 实际断言集——对抗审查修正：只对 rootfs 标记核验漏了 boot 面，`DEMO ok` ⊂ `AIDEMO ok` 正是 **boot 标记间**的历史混叠先例；`APP ok` 前缀与 `ROOTFS ok` 族不同首词，核验通过；带参形态维持鉴别力）。

## 7. 拆步与各步验收（小步提交）

| 步 | 内容 | 提交物 | 验收 |
|---|---|---|---|
| **a-0** | `berry-manifest` crate + host 测试 | `user/berry-manifest/`、`user/Cargo.toml`（members 增项）、justfile（test-host 行 + 计数注释）——对抗审查补：test-host 是逐 crate 显式枚举 recipe，缺这两处 N 无从跑起 | `just test-host` 全绿（200+23=223 落地值）、build 零 warning；零 QEMU 行为变化 |
| **a-1** | mkcpio 抽 `build_archive`（含头注修正）+ `mkapp.py`（canonical 比对 + 负例自测）+ appdemo/appquiet + build-user 接线 | `tools/`、`user/bins/{appdemo,appquiet}/`、justfile | smoke 四跑全绿**零漂移**（app/ 条目被主走查跳过）+ host 不回退 + **构建后 `git status` 零新增未跟踪产物**（中转区 ignore 覆盖机器验证） |
| **a-2** | svcmgr appd arm（≥64B 发射窗）+ `APP ok` + smoke 计数/断言接线 + EL2 | svcmgr、`tools/smoke.sh`（ROOTFS_MARKERS +1、ALL_MARKERS 联动、`app APPDEMO up` 独立 grep +1、头注计数同步） | smoke 四跑 **21/44/44**（落地值——草案期 18/41/41 为 A-8 后基线，TCP 四刀 T-c/T-d 先行落地推至 20/43/43）+ EL2 `21` 同绿 + 手动 `just run-rootfs` 全标记（含 `app APPDEMO up` 与 `APP ok n=2`）+ host + **坏输入手动验证**（装 id=24B + 坏 ELF——过 `\x7fELF` 门卫但 loader 拒——的 .bap 进 rootfs：断言 `eco: app` 降级行落串口、svcmgr 存活、`APP ok` 缺失不连坐 halt；CI 不可达路径纳入人工验收面） |
| **a-3** | workflow 对抗审查（本草案 + 实现）→ confirmed 全修 + 文档回写 | 修复 + 文档 | 审查 finding 全闭环（workflow 10 agents 四维：6 confirmed C1/C2/U1–U4 全修；U5/U6 以扩展探针五臂实测闭环——BADBAD/BADTRUNC/BIGAPP/MISMATCH/THISID 五降级行 + 好槽照起 + `APP ok` 缺失不连坐 + svcmgr 存活金丝雀绿）+ 回写四件：①CLAUDE/AGENTS app-0 段 + smoke/host 计数锚（rootfs 21 / RX·持久化 44 / host 223）+ **A-6 段「40 标记」复位 39**（git 考古：A-6 时 ALL = 23+16 = 39、与同段「RX 相 39 标记」自洽——审查所见 39/40 并存实为历史段 vs 现役段之别，历史段记录当时验收值不随后续漂移）；②`应用模型设计.md` §3/§4.2 改写（§3 样例改写为落地 v0 schema、原样例标注 v2 目标态；§4.2「intent 缺省按 type 角色表定档」加 v0 必填前提注）；③smoke.sh 头注计数（a-2 随步落地）；④本草案 §6/§7 地面真值化 |

多会话纪律：justfile/smoke.sh/CLAUDE.md/AGENTS.md 为共享文件——每次提交前重查 `git status` + 逐 hunk 归属，只暂存本会话拥有的改动；x86 会话的 WIP（`tools/patch_doc_*.py` 等）严格只读。

## 8. 诚实面（v1 边界即设计，非欠账不还）

- **「装」的 v0 语义 = 构建期打包进 rootfs**——不是运行时安装。没有安装器、没有 apps/ 树、没有卸载；这些是 ADR #45 存储布局的 v2 面（fsmgr 目录语义先行）。诚实宣称口径：**v0 演的是「manifest → 授权集 → 生命周期」链，不是「应用商店」链**。
- **appd 未分家**：v0 = svcmgr 内一个 arm（设计文档指定「svcmgr 演化」）；真 appd（独立服务、墓碑表、监督重启）随存储 v2。arm 隔离 + 降级续走纪律照旧（任何 app 面失败不杀 svcmgr、不阻塞后续标记）。
- **单发监督**：v0 拉起后只 reap 一次；崩溃重启/墓碑/冻结是 app-1 与 v2 面。
- **能力词汇一件**：fs/MMIO/stdin 的缺席是**结构性缺席**（schema 不收），不是「收到但忽略」——与应用模型「没申请的能力表里就没有」同构。
- **manifest 不落盘**：构建期产物嵌 rootfs.cpio；磁盘上的 manifest（apps/ 树内）随 v2 安装面。

## 9. 与既有docs/决策的对齐

- **#43**：层 = IPC 边界——appd ↔ app 的一切交互走既有 channel/env 块/spawn 链，**零内核新概念、零新 syscall**（0x33/0x3A/channel 全现役）；「应用」纯用户态协议约定的纪律在第一片代码里就成立。
- **#44**：三段对应——cpio+TOML（§1/§2）、安装 = 翻译 handle 授权集（§5 翻译表）、启动 = env 块 gift 授能（d-3a 机制现役）；签名/版本化 v2 加法不重构——schema 的 add-only 演进位（type 新值、capabilities 新键）已留。
- **#45**：apps/ 树与运行时安装 = v2；v0 的 rootfs 嵌入是其前身而非替代。
- **AI工程面 §2.2**：manifest 机器可读 = 工位砖 face——app-0 后 SDK 有真 manifest 词汇表与解析器（host 测试即消费样例）。
- **驱动模型 §1**：`.bap` 同链——`type="driver"` + `[hw]` 节是本 schema 的 v2 add-only 扩展位，两文档互指不歧义。
- **测试策略**：`APP ok` 延续「确定性状态谓词」纪律谱系（EXIT ok 同族：真退出码 + 全链终态，零竞速面）。
- **应用模型设计.md §3/§4.2 对账（对抗审查补）**：三处分叉点 = type 角色词表（{gui,headless,daemon} → 唯一值 `"app"`）/ [capabilities] 五键 → console 一键 / intent 可省+角色定档 → 必填（+[ui] 节缺席）——本文 = 实现子集 + 词表演进（§2 显式声明）；v2 收敛路径 = type 角色值 add-only 回归时同步解锁角色定档与对应能力键；父文档改写列 a-3 回写义务（否则「manifest v0」两文档两义）。
