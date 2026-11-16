# G——新件集成检查（全面遗漏审计第二轮之 C 线）

> 版本：1.0 ｜ 日期：2026-11-15 ｜ 定位：第二轮遗漏审计 C 线——新件（crate／docs／ADR／research 件）入册完整性五面检查：①8 个新 crate 四件套（索引行／沿革节／所在目录 README／AGENTS 冻结常量表）②8 份新 docs 索引行与旧版引用清理 ③ADR 号三方一致（#77＋#82–#85＋预留号段）④research 各系列 README 最新件登记 ⑤test-host 接线（justfile 行＋实跑＋CI 覆盖）。方法：每条结论给路径＋行号两源核对；测试行逐条实跑；推断标「推断」；全仓只读，本文件为唯一产出件（另按系列惯例补 README 本件行一行）。

## 〇、结论速览

| 检查面 | 总判 |
|--------|------|
| 新 crate 入册（8 件） | 沿革节 8/8 全齐；索引行 4/8（均为行内注记非专行，berry-gpuproto/berry-em/pm-integration/berry-verify 零命中——gpuproto 系 D 线 P0 遗留未修且设计文档仍用旧名 berry-virtio-gpu）；所在目录 README 登记面不存在（user/ 与 user/bins/ 无 README，存量同形）；AGENTS 冻结常量表自 2026-11-12 起随 Curia 模板化退役，该面不适用 |
| 新 docs 入册（8 份） | 索引行 8/8 全在；docs/ 活档旧版引用零残留（v1.2 升版时挂账的 12 死链＋规则 README 2 处均已随迁）；代码注释层 5 处死文件引用（调度器设计-v1.1.md——按升版注记惯例「不回改」容忍，建议随刀随迁） |
| ADR 完整性 | 实数 74 行＝索引/规则 README/根 README 三面口径 ✓；#77＋#82–#85 五条全在册（表行＋文末节＋头注三方齐）；**新发现：预留号 #86 三占**（彻底Rust化计划 vs 显示与加速设计 vs berry-gpuproto 注释，未按「让先落者律」错开）＋规则/01 与规则/13 残留「57 条 ADR」陈旧计数 |
| research README | 调度深度调研 ✓ 全齐；全面遗漏审计第二轮 E 已登记、F/G 待落；**厂商定制化深研 44–46 号三件未登记**；**显示与加速 06/08 号两件未登记**；**调度彻底计划 12 件仅登记 3 件（九件缺行，本轮最大缺口）** |
| test-host 接线 | 6 crate＋berry-verify＋berry-sign selftest 全有行（justfile:788/807/808/809/810/815/1051/1053）；**七命令实跑全绿**（29+31+13+8+12+14+10）；CI 对八件零引用——D 线「test-host 主体零 CI」＋04 号 F3 遗留确认仍开 |

## 一、新 crate 入册检查（四件套逐件）

### 1.1 总表

| crate（路径） | 索引有行 | 沿革有节 | README（所在目录） | AGENTS 冻结常量 |
|---|---|---|---|---|
| berry-gpuproto（user/berry-gpuproto） | **无**（docs/索引.md 零命中） | ✓ 二百五十一（阶段沿革:5881「B1-a 协议 crate」） | 无面（user/ 无 README.md） | 不适用（见 1.3） |
| berry-em（user/berry-em） | 无（零命中） | ✓ 三百二十六（:5945「EM 表第一张」） | 无面（同上） | 不适用 |
| berry-slackface（user/berry-slackface） | ✓ 行内注记（索引:50 调度器设计行「timer slack 判定律纯面（berry-slackface）」） | ✓ 三百二十五（:5928「slackface 落件·12 测」） | 无面（同上） | 不适用 |
| berry-power（user/bins/berry-power） | ✓ 行内 ×2（索引:50「PM 双服务（berry-power/berry-thermal）骨架 §5.4」＋:54「实施随 berry-power」） | ✓ 三百二十四（:5918）＋三百三十六（:6250）＋三百三十七（:6270 svcmgr 预拉） | 无面（user/bins/ 无 README.md） | 不适用 |
| berry-thermal（user/bins/berry-thermal） | ✓ 行内注记（索引:50，同上） | ✓ 三百二十四（:5918「berry-thermal 骨架·ADR #66 契约六件」）＋三百三十六/三百三十七同 berry-power | 无面（同上） | 不适用 |
| pm-integration（user/pm-integration） | 无（零命中） | ✓ 三百三十四（:6137「PM 四 crate 联合场景·14 测」） | 无面（同上） | 不适用 |
| berry-verify（user/bins/berry-verify） | 无（零命中；锚＝设备侧验签 doc 索引:132＋rootfs-manifest:59＋justfile:1053） | ✓ 三百一十六（:5543「P3 守卫案 c 落地（berry-verify 助手进程）」） | 无面（同上） | 不适用 |
| berry-sign（berry-crypto 的 bin，user/berry-crypto/src/main.rs） | ✓ 行内注记（索引:132「berry-sign admit」；:145 命中为 berry-signal 前缀误匹配，非本件） | ✓ 四节：三百零六（:5227 P1 刀 host CLI）＋二百三十七（:5299 P3-0）＋二百三十八（:5320 selftest 挂 test-host）＋三百一十（:5375 sign-raw/verify-raw） | 无面（user/berry-crypto/ 无 README.md） | 不适用 |

判定口径：docs/索引.md 是文档详表非 crate 台账，crate 无专行义务（D 线 §3.1 判例——「至少两个文档锚」即算入册）。按此口径八件全过；但按本任务书四件套口径，索引面 4/8、README 面 0/8（面不存在）、AGENTS 面 0/8（面已退役）——四件套中只有沿革面是 8/8 实齐的。

### 1.2 索引面两个开放项（沿 D 线未修）

- **berry-gpuproto 仍是「仅沿革可考」**（D 线 §3.1 P0 级发现的遗留）：docs/索引.md 全文零命中；承载文档《显示与加速体系设计-v1.0.md》三处仍写旧名 berry-virtio-gpu（:45「berry-virtio-gpu crate」／:79 裁九「新纯函数 crate berry-virtio-gpu」／:93 刀 5 行），而实际定名 berry-gpuproto（user/berry-gpuproto/src/lib.rs:5「02 号 §3.2 原名 berry-virtio-gpu，本轮定名 berry-gpuproto」）。D 线修复建议 4（显示 standard 升 v1.1 回填）本轮 P0×3 修复窗未含，仍开。
- **pm-integration 命名孤例仍开**（D 线 §五）：无 berry- 前缀且不在《命名与前缀规范-v1.1.md》例外白名单——本轮未处置，随其入索引/README 任何一刀顺带二择一（入白名单或更名）。

### 1.3 AGENTS 冻结常量表：检查面已退役（事实记录）

根 AGENTS.md 现为 Curia 上游模板 1:1（2026-11-12 起，docs/索引.md:4 自述「AGENTS.md 2026-11-12 起归 Curia 上游模板零回写」），全文无项目冻结常量表——新 crate 无处可更新，该检查面对全部新件一致不适用。历史沿革：厂商定制化 D3 刀曾在 AGENTS.md:12/:26 写「71 ADR」与定制冻结行（阶段沿革:4925），2026-11-11 四面对齐至「74 ADR」（:5253），次日模板化后全部退役；命令表唯一家＝docs/命令速览.md、冻结常量执法面归 docs/架构决策.md 决策总表（D 线 §五同判）。结论：八件新 crate 均不涉及冻结常量变更（协议值域／uapi 常量钉板在各 crate 单源），无漏更新。

### 1.4 附带发现：代码注释层死文件引用（5 处）

调度器设计 v1.1→v1.2（2026-11-14）升版后，以下代码注释仍引用已不存在的 docs/调度器设计-v1.1.md：kernel/src/main.rs:4343、user/berry-slackface/src/lib.rs:3＋Cargo.toml:9、user/berry-em/src/lib.rs:7＋Cargo.toml:10。升版注记（docs/调度器设计-v1.2.md:436）明言「代码注释锚……引用带时点、不回改」，故按现行惯例容忍；但这 5 处是文件名级死链（非仅行号漂移），与「外部行锚」注记的本意（行号随增行漂移）不同——建议随各文件下一刀顺手改指 v1.2（v1.2 §3.8 即 slackface 设计锚的现行位）。

## 二、新 docs 入册检查（8 份）

### 2.1 索引行对拍：8/8 全在

| 文档 | docs/索引.md 行 | 头部版本双标 |
|---|---|---|
| 调度彻底计划-v1.2.md | :135（计划定稿 · Type: standard · 1.2.0 · 预 ADR 候） | ✓ Type/Status/Version 1.2.0/Updated 2026-11-15 |
| 显示与加速体系设计-v1.0.md | :133（设计定稿 · standard · 1.0.0 · 预 ADR #86 候） | ✓ Version 1.0.0/Updated 2026-11-13 |
| 设备侧验签与账本落盘设计-v1.0.md | :132（设计草案 · 预立法随 P3 开刀） | H1 内嵌 v 号形（七件豁免族，提案 v0.1 待裁） |
| 产品配置层与槽位合同设计-v1.1.md | :130（已立法 · ADR #83 · 1.1 升版注） | ✓ 版本 1.1（v1.1）｜2026-11-11 |
| BCTS兼容性测试套件设计-v1.1.md | :131（已立法 · ADR #84 · v0 runner 已落） | H1 内嵌 v 号形（同上豁免族） |
| 调度器设计-v1.2.md | :50（定稿 · 已落地 · 1.2，v1.2 增量注全） | ✓ 版本 1.2（v1.2）｜2026-11-14 |
| 调度器架构白皮书-v1.1.md | :51（定稿 · 对外发布 · 1.1） | ✓ 版本 1.1｜2026-11-14＋文件名自注 |
| 调度器API参考-v1.7.md | :56（定稿 · 影抄 · 1.7，148 口径） | ✓ v1.7（2026-11-15 升版增量注） |

### 2.2 旧版引用清理：docs/ 活档零残留 ✓

逐名全仓 grep（调度器设计-v1.0/v1.1、白皮书-v1.0、API参考-v1.6、产品配置层-v1.0、BCTS-v1.0、调度彻底计划-v1.0/v1.1）：docs/ 内命中仅为两类合法残留——时点档正文（战略裁决-2026-10-15.md:63、阶段沿革.md，索引.md:192-204 维护规则 6 明文豁免）与升版档自述的变更注（调度器设计-v1.2.md:436 引用同步记录）。**v1.2 升版时挂账的「调度彻底计划 12 处链接＋规则/README 2 处占用待随迁」已全部收口**：docs/调度彻底计划-v1.2.md 现引调度器设计-v1.2.md ×13、v1.1 ×0；docs/规则/README.md 无旧版命中。research/ 时点件（调度彻底计划/02 号 8 处 v1.1、pm-notes/00 v1.0 等）按惯例不回改。

### 2.3 附带发现（同刀该抓的三件，均非任务书 8 件之列）

- **路线图自身三口径断裂**：文件名 路线图-v1.2.md，头部第二行仍「版本：1.1（v1.1）｜2026-11-11」（docs/路线图-v1.2.md:4，首行块已写 v1.2/2026-11-12 升版注），索引:35 标签列同步停在「1.1」——文件名/头版/索引标签 v1.2/1.1/1.1 三不一致，与 D 线 P1-7（CLI-first 断裂）同族轻症。
- **现行权威文件死链**：docs/架构决策.md:137 设计文档索引末行引 `路线图-v1.1.md`（文件已不存在，现行 v1.2）；入口文件 README.zh-CN.md:47 引 路线图-v1.0.md、README.md:78 引 路线图-v1.1.md——三处均为活档旧名，按索引规则 5「升版即同步全部引用」应清。
- **应用API全景 L1 清单落后**：docs/应用API全景-v1.0.md:19「L1 协议 crate 层：九件在册」未含 berry-gpuproto（立法稿 2026-11-08 先于 crate 2026-11-13 落件）——该档是「API 出齐了没」的正面回答件，第 10 件协议 crate 缺席即答错；建议随显示线回填刀补行（P2）。

## 三、ADR 完整性（#77＋#82–#85 三方对拍＋预留号段）

### 3.1 实数对拍：74 行四面口径一致 ✓（两处规则文件陈旧除外）

docs/架构决策.md 决策总表实数 74 行（表内号 #1–#64＋#66–#70＋#77＋#82–#85；#65 预留 API 治理章程、#71–#74 顺延空号、#75/#76/#78–#80/#81 预留 SDK 五件套与应用规范，均按头注 :4 在册说明不在表——非漏记）。四方口径：docs/索引.md:30「74 条 ADR」✓、docs/规则/README.md:50「74 条」✓、README.md:77「74 ADRs」✓、README.zh-CN.md:47「74 条」✓；根 AGENTS.md 无计数（Curia 模板，见 1.3）。**例外：docs/规则/01-内核编码.md:64 与 docs/规则/13-工作流程与协议.md:91 仍写「57 条 ADR」**——两域文件落后 17 条（#58–#85 期间未随升），同域 README 已是 74，判 P1 级域文件漂移（建议改引「以 docs/架构决策.md 实数为准」不写死值）。

### 3.2 五条新 ADR 三方登记齐 ✓

| ADR | 决策总表行 | 文末全文/节录节 | 头注（架构决策.md:4） |
|---|---|---|---|
| #77 分发与签名链立法（2026-11-11） | :79 | ✓ :215 专节（含快道 L1/R1a 两裁定） | ✓ |
| #82 厂商定制面总纲（2026-11-10） | :80 | ✓ :239 专节＋附录一预埋登记表＋附录二十三问 | ✓ |
| #83 产品配置层/槽位合同 | :81 | ✓ :330 节录（全文外置 v1.1 doc） | ✓ |
| #84 BCTS | :82 | ✓ :337 节录（全文外置 v1.1 doc） | ✓ |
| #85 版本管理总纲 | :83 | **无文末节**（全文＝docs/版本管理标准v1.0.md，表行自带决策摘要＋勘误注） | ✓ |

#85 无文末节录与 #83/#84 形态不一（P2，非漏记——头注与表行双锚已在；「#84 撞号勘误顺延 #85——让先落者律」链完整可追）。

### 3.3 新发现：预留号 #86 三占（未按让先落者律处置）

- 彻底Rust化计划-v1.0.md:3「预 ADR #86 候选」（采集 2026-11-12，首占）＋docs/索引.md:36 同口径；
- 显示与加速体系设计-v1.0.md:9「号段自 #86 起」（Updated 2026-11-13，占整段）＋docs/索引.md:133「预 ADR #86 候」；
- user/Cargo.toml:129（berry-gpuproto 注释「ADR 预#86」，随显示线占）。

对照正面例：调度彻底计划-v1.2.md:10 显式声明「升册号段……与显示线（自 #86 起）错开」；先例判例：#85 勘误即因 #84 撞号按「让先落者律」顺延（头注 :4）。#86 双占双方（Rust 化计划 11-12 首占 vs 显示线 11-13 段占）均无互指注记，索引两行并列不提撞号——升册日必撞（显示线裁决集整段＋Rust 化计划至少 1 条都想拿 #86）。这正是《索引双源核对清单-v0.2.md》§三④（预 ADR 候号归属对拍）要抓的形态；建议 owner 裁：显示线让段自 #87 起（晚占让先落者），或 Rust 化计划改候 #87，索引两行同批加互指注。另：设计文档索引（架构决策.md:108-137）未收 #77/#83/#84/#85 四份全文文档行，也未收调度彻底计划/显示设计两份预候 standard——补四行即可（P2）。

## 四、research 各系列 README 最新件登记

| 系列 | 盘上件 | README 登记行 | 判定 |
|---|---|---|---|
| 调度深度调研 | SMP-01＋02–08（8 件） | 01–08 全行（README 尾 08-三刀复核） | ✓ 全齐 |
| 全面遗漏审计 | A–E＋本件 G | A–D＋E（E 行 2026-11-15 22:03 随 E 线落档写入）；F/G 待落 | E ✓；G 行随本件补（见文末注） |
| 厂商定制化深研 | 01–46（含 44/45/46） | 表 43 行止于 43 号 | **44-终极演练-全能力综合／45-终极演练复核／46-缝隙清偿轮 三件未登记** ✗ |
| 显示与加速体系设计 | 01–03,05–13,15–19（17 件；04/14 无文件亦无引用，非缺口） | 15 行 | **06-刀三与字体复核／08-B1a与基线复核 两件未登记** ✗（08 在任务书 07–19 号区间内） |
| 调度彻底计划（追加检查） | 01–12（12 件） | **仅 01/04/06 三行** | **02/03/05/07/08/09/10/11/12 九件未登记** ✗✗——本轮最大 README 缺口（05–12 号复核系列 2026-11-14/15 密集落件，README 只随手补过 04/06 两行） |

老旧引用 grep 附记：调度深度调研 03:265/05:221 链接文本「调度彻底计划-v1.0」而目标已改指 v1.2 文件（半迁移——目标有效、文本陈旧，P2 随手改）；厂商定制化深研 README 核心结论 5「OEM 身份并入已预留 #77」措辞过时（#77 已于 11-11 落册，非预留）；显示与加速 README 无旧版引用问题（定稿指针指 docs/显示与加速体系设计-v1.0.md 现行有效）。

## 五、test-host 接线完整性（justfile 行＋实跑＋CI 覆盖）

### 5.1 justfile test-host（配方起 justfile:613）逐件

| 件 | 行 | 语句 |
|---|---|---|
| berry-gpuproto | :788 | `cargo test --manifest-path user/Cargo.toml -p berry-gpuproto --target {{host}}` ✓ |
| berry-power | :807 | `cargo test --manifest-path user/bins/berry-power/Cargo.toml --target {{host}}` ✓ |
| berry-thermal | :808 | 同形 ✓ |
| berry-em | :809 | `cargo test --manifest-path user/berry-em/Cargo.toml --target {{host}}` ✓ |
| berry-slackface | :810 | 同形 ✓ |
| pm-integration | :815 | `cargo test --manifest-path user/pm-integration/Cargo.toml --target {{host}}` ✓ |
| berry-verify | :1053 | `cargo test --manifest-path user/Cargo.toml -p berry-verify --target {{host}}` ✓ |
| berry-sign（selftest） | :1051 | `cargo run --manifest-path user/Cargo.toml -p berry-crypto --bin berry-sign --target {{host}} -- selftest` ✓ |

装配面旁证：build-user musl 段 berry-power/berry-thermal `--features wired`（justfile:284/:287）；rootfs 装配三条目（tools/rootfs-manifest.toml:55/:56/:59）。六件全部接线，任务书「F3 遗留」的 justfile 侧已闭合（F3 的门禁侧见 5.3）。

### 5.2 实跑验证（2026-11-15 22:0x，host=aarch64-apple-darwin，CARGO_TARGET_DIR=/tmp/g-audit-target）

七条 cargo test 全绿：berry-gpuproto 29／berry-power 31／berry-thermal 13／berry-em 8／berry-slackface 12／pm-integration 14／berry-verify 10；berry-sign selftest「验收线四件全过」。计数与沿革二百五十一（29 测）／三百二十四（28+12→F1 后 31+13）／三百二十五（12）／三百二十六（8）／三百三十四（14）逐件相符。**唯一出入**：沿革三百一十六（:5546）记「berry-verify 单测挂 test-host(159 ok)」vs 现测 10 测（#[test] 实数 10）——【推断】159 为当日同窗合计口径（含 berry-crypto/p3sim 邻行），非 berry-verify 单件数；沿格雷区不影响现役接线判定。

### 5.3 CI 覆盖：零引用确认（D 线「test-host 主体零 CI」＋F3 门禁侧仍开）

.github/workflows/ci.yml 与 ci-light.yml 对八件零命中（CI cargo test 直挂仍仅 berry-ksproto/berry-mmap/berry-signal 三件——ksproto「20 测零自动执法静默腐烂」判例只修三件的旧账不变）。D 线修复建议 7 前半（test-host 主体进 CI）与 04 号 F3（测试挂门禁）的门禁半均未动——本轮 E 线修复窗亦未含。新八件处于「justfile 本地执法在、CI 零执法」态，与 60+ crate 主体同病。

## 六、修复建议（按优先级，供 owner 排刀）

1. **README 补行一批（零风险纯登记，15 分钟）**：调度彻底计划 README 补 02/03/05/07–12 九行；厂商定制化深研 README 补 44/45/46 三行；显示与加速 README 补 06/08 两行——三处共 14 行，全是已存在件的登记，不改任何事实。
2. **#86 预留号撞号裁决**：按「让先落者律」二择一（彻底Rust化计划 11-12 首占 vs 显示线 11-13 段占），索引.md:36/:133 两行加互指注；同步 user/Cargo.toml:129 注释。
3. **规则文件 ADR 计数去写死**：规则/01:64、规则/13:91「57 条」改引架构决策.md 实数（或不写死值）。
4. **路线图三口径 + 三处死链同刀**：路线图-v1.2.md:4 头版行 1.1→1.2＋索引:35 标签列；架构决策.md:137／README.zh-CN.md:47／README.md:78 路线图旧版引用改指 v1.2。
5. **显示线 gpuproto 双修**（D 线建议 4 重申）：显示与加速体系设计升 v1.1（:45/:79/:93 旧名 berry-virtio-gpu 改 berry-gpuproto＋13 号落字清单五条）；索引.md 显示设计行或调度器设计行补 berry-gpuproto 注记——消「仅沿革可考」。
6. **代码注释死链随刀随迁**：1.4 节 5 处 调度器设计-v1.1.md 引用改指 v1.2（各文件下一刀顺带，不单开刀）。
7. **应用API全景 §2 补第 10 件**：berry-gpuproto 行（L1 协议层九件→十件）。
8. **CI 执法补线**（承 D 线建议 7）：至少新 crate 落地时同步 fail-fast 位（ksproto 判例流程化）；八件新 crate 与 berry-libc/launcher/guifbd 同候。

## 来源与复核说明

- 全部行号为 2026-11-15 22 时工作树直核（只读 grep/读文件；berry-guiproto/gui-wmd owner 占用区零碰）；随源漂移不回改（仓内惯例）。
- test-host 七条命令与 berry-sign selftest 为本审计实跑（产物 target 落 /tmp/g-audit-target，零仓改）；实跑环境 rustc host=aarch64-apple-darwin。
- ADR 计数与号段为 awk 全表机扫＋头注人工核对；research README 登记差集为「盘上文件名清单 vs README 表行」双向对拍（显示系列 04/14 号确认为无文件无引用的编号空洞，非登记缺口）。
- 全面遗漏审计 README 的 E 行系并行 E 线 22:03 写入（本审计读到的前后两版不一致，以 22:03 后版本为准）；F 线文件在途未落，G 行已按系列惯例随本件补入 README。
