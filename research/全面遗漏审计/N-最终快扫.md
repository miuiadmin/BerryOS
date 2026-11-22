# N——最终快扫（全面遗漏审计第六轮·B 面回看）

> 版本：1.0 ｜ 日期：2026-11-16 ｜ 定位：第六轮快速终验 B 面「还有没有遗漏」四问——①owner 最近 5 笔提交有无新 crate/新 API/新文档；②全仓新文件/未入册件；③交叉引用基线确认；④六轮后仍可能漏网的盲区类型（只列类型不做全扫）。产出与 A 面输出合并汇报；本目录 README 登记行（F/N 两行）按 [M](M-最终判定.md) §四.5 判例留主会话随批登记。方法与边界：禁 git——提交史以 `.git/logs/HEAD`（reflog）直读替代 git log，worktree 入库态以 mtime+在册证据推断并标注「推断」；全域只读，唯一写入=本件；门禁三件实跑。锚定态：HEAD=wip/sched-1.1=1471729e（2026-11-16 12:31，reflog 尾笔）＋ 13:28-13:31 工作树快照。

## 一、owner 最近 5 笔提交（reflog 直读，时间倒序）

| # | sha | 时刻 | 主题（reflog 原文摘录） | 新面判读 |
|---|---|---|---|---|
| 1 | 1471729e | 12:31 | feat(v3 全四动作激活·POLICY4·ADR #42 终局兑现)：berry-power 完整策略服务——placement[EAS]+weight[±64 夹紧]+EPP[两位]+频率[PM 直连] | 新 API 面=用户态策略服务四动作（全部消费既有 0x83/PM 通道，零新 ABI 号）；docs=research 16 号＋沿革二百八十一 |
| 2 | a916201e | 12:27 | feat(启动产物) 实施批+跨分卷治理 P1-P3：dmesg e2e 关账/ROOTFS ok vols 打点/smoke 多卷钉板/DARK 三件翻状态 | 行为＋docs；DARK 过程件随刀翻状态（推断已入册） |
| 3 | c34b6848 | 12:07 | merge: origin/wip/sched-1.1 合流（owner 索引unsafe/五轮审计修复＋我方 FairBand 验证/EAS 激活/终验/A-B loadgen） | 合流笔，无独立新面 |
| 4 | 6ef4a41c | 12:04 | feat+research(FairBand 验证+EAS 激活)：FairBand 四 API 参数化 QEMU 全面验证（boot 32/32+smoke+LOADGEN 对比）＋EAS 生产激活首步（berry-power 读 util→policy_bind→place() 消费，首个真数据流） | docs=research 14/15 号＋沿革二百八十 |
| 5 | 8e0fdd84 | 11:42 | fix(启动产物) 六轮审计修复：web.rs LanUpstream 三字段恢复性接口摘抄入册（闭合树 137 测全绿）＋find_run 夹具/vnc_reachable 补测＋README 登记 | 测试＋docs |

第 6 笔 e648147a（11:15）=R5 本体（FairBand 四 API 行为差异化＋[M](M-最终判定.md) 最终判定），即本轮回看基线的直接前驱。判语：最近 5 笔全部是「R4 语义审计→R5 FairBand 差异化→v3 四动作激活」闭环自身的收尾与验证——**零新 crate、零新 ABI 号、零第三方新件**；新文档=research 14/15/16 号三件＋沿革 279-281。[M](M-最终判定.md) 锚点 46dd5b5 以来共 6 笔，无大合流笔（§3.3 重开触发器①未触发）。

## 二、全仓新文件与未入册件（以最后提交 12:31 为界，find -newermt 实测，剪枝 .git/.dsh-worktrees/target*/out 等忽略面）

### 2.1 在途未提交面 11 件——「全域空闲」前提不成立

| mtime | 文件 |
|---|---|
| 12:32:19 | user/berry-netdproto/src/lib.rs |
| 12:35:40 | user/berry-libc/src/net.rs |
| 12:37:28 | tools/evsclient.py |
| 12:49:09 | research/NET-03-客机TCP二刀吞吐与对抗复验-v1.0.md |
| 12:50:04 | research/NET-04-客机TCP四刀流水线-v1.0.md |
| 12:51:32 | .agents/notes/implemented/2026-11-16-客机tcp二刀吞吐.md |
| 12:53:01 | .agents/notes/implemented/2026-11-16-客机tcp四刀流水线.md |
| 13:07:41 | user/bins/bsh/src/main.rs |
| 13:14:05 | docs/阶段沿革.md |
| 13:29:10 | drivers/bins/vnet/src/main.rs |
| 13:29:10 | drivers/bins/vnet/src/tcp.rs |

12:31 最后提交之后 57 分钟内连续改写，drivers/bins/vnet 两件在本轮扫描进行中（13:29）仍在更新——工作树活跃在途。推断：owner（或并行会话）的 TCP 四刀线在飞——NET-04 与「客机tcp四刀流水线」note 为未提交新件（5391d43 仅落账 NET-03/二刀件，后续五笔主题均不含四刀），其余 9 件为已提交文件的未提交改动；与 [M](M-最终判定.md) §一.1.2-4「owner 在途占用面」同型。tracked-modified 与 untracked 的甄别需 git status，本轮禁 git 未做，留 owner 收笔后复查。

### 2.2 误入库两处（M §2.2-O5）原样未动

根 W3A-\*.md ×5（mtime 2026-11-16 08:01）与 tools/.berry-verify/ 均在树；reflog 复核 46dd5b5 以来六笔无清理提交。O5 未闭合。

### 2.3 根目录过程件群与杂散件（入库态混合，禁 git 未辨）

根 \*.md 共 34 件，除 AGENTS/CLAUDE/README 双语 4 件外为过程件 30 件：A 族 3＋C 族 4＋DARK 2＋PORT/RECON-PORT 5＋SPRING 2＋W3A 5＋W4 族 9。[L](L-owner新件扫描.md) 轮 ls-files 仅点名 W3A×5 误入——其余 25 件推断未入册（untracked），DARK 2 件 12:27 随刀翻状态（推断已入册）。另杂散件：`1`（2026-11-15 15:24）、px-clean.log/px-drop.log/px-rst.log ×3、.DS_Store——均不在 [.gitignore](../../.gitignore) 覆盖内。此类=多 agent 工作流过程件无固定家的常态产生器（见 §五.3）。

## 三、交叉引用与基线（三门禁实跑）

| 门禁 | 本轮结果 |
|---|---|
| node .agents/tools/check-docs.mjs --strict | 检查 343 篇／告警 0 条，rc=0（[M](M-最终判定.md) 时 337→343，＋6 与 14/15/16 号、NET-03/04 等近三笔 docs 相容） |
| python3 tools/gen_abi.py --check | PASS——「NR v1 39 号在册——内核↔berry-sys↔文档↔syz↔arch-wake 五面单源一致」 |
| python3 tools/docs_guard.py | PASS（0 警）——G10 索引 6 区 124 行=头部 124，docs 根 123 份 .md 全登 |

「343 篇 0 警＋abi PASS」独立复证成立，另加 docs_guard 0 警。

## 四、已知缺口回看（锚点抽样复验，非全扫——快扫边界）

| 件（M §二账） | 现态 | 锚（本轮直读） |
|---|---|---|
| D1 intent→band 行为差异化=0 | **闭合**——四 API 全参数化＋QEMU 验证在册 | [band.rs](../../sched/src/band.rs):169/:245/:277（consume(band,cycles)／throttle_all／unthrottle_all 带参形）；e648147a＋6ef4a41c |
| D2 EAS 生产不可达 | **闭合（首步通水）**——berry-power 真数据流＋v3 placement 动作 | 6ef4a41c＋1471729e |
| O1 pmfreq 代际断裂对账 | **演进中（半清偿）**——频率面以新形态（策略服务 PM 直连）通水；pmfreq 常驻轮询未恢复，窗注口径仍在 | [main.rs](../../user/bins/berry-power/src/main.rs):39-40/:256（「剩余留窗：常驻循环的周期轮询面」原样）；pmfreq/pmseat 源码全仓零命中（本轮 grep，仅审计/沿革文档在册）——K §4.2 的 owner 显式对账无提交证据（推断未销案） |
| O2 集成指南示例带毒 | **主毒清、尾残留**——§二示例已重写为 berry_policy_status＋bind 正型（auth= 裸调全文零命中）；:129 仍荐 sched_slice_hz=128 | [集成指南](../../docs/显示管线调度集成指南-v1.0.md):69-92（新示例）/:129（128→250 数据回填未动，推断待 owner 裁） |
| O5 误入库两处清理 | **未动** | §2.2 |
| W11 README F 行缺 | **未动**——表仍九行（A/B/C/D/E/G/K/L/M），F 行缺第三轮，N 行将缺 | [README](README.md):5-15 |

其余 O/D/W 档 26 件未逐项复验（快扫边界），仍以 [M](M-最终判定.md) §二为账，触发器制维持。

## 五、漏网类型（六轮后仍可能漏网的盲区——只列类型）

1. **时窗漂移（结构性，最大）**：审计快照与「基线全绿」都是时点性的——本轮实证最后提交后 11 件仍在连续改写（§2.1）；任何终验结论都会被下一笔在途增量中性化。只能触发器化，不可一次性闭合。
2. **射程外域**：「实现了但没人承诺过」的面（TCP/NET 线、启动产物收敛批、AM、app-1/2）六轮均以 commit 终门自证收账，K 的语义级三问从未覆盖——在途的 NET-04 即其增量。
3. **工作区卫生**：根目录过程件群＋误入件是「未入册新件」的常态产生器——多 agent 工作流过程件（A-IMPL/W4-IMPL 等 30 件）无固定家、无 ignore 规则、无退役通道，每轮净增。
4. **元登记滞后**：本目录 README 靠主会话手动登记——F 行缺三轮未补、N 行将缺；check-docs/docs_guard 管链接与 docs 索引，不管本表。
5. **语义级仍仅调度域**：K 三问未推广至显示/PM/NET 面；v3 四动作「通水」的验证深度=commit 自证＋QEMU 标记，无独立第三方复核。
6. **门禁绿≠行为对**：三门禁全是文档/单源/静态面；行为证据（boot 32/32、FairBand LOADGEN 对比、闭合树 137 绿）本轮快扫未跑，采信近三笔 commit 自证——静态绿与行为绿之间无机器绑定。

## 六、判语

四问答案：①最近 5 笔＝R4→R5→v3 闭环自身收尾，零新 crate/零新 ABI 号/零第三方件，新文档 14/15/16 号＋沿革三笔；②有——在途未提交 11 件（TCP 四刀线在飞，「全域空闲」证伪）＋根目录未入册过程件群＋O5 误入件原样；③343 篇 0 警＋abi PASS＋docs_guard PASS 三绿独立复证；④六类盲区见 §五。已知缺口抽样：D1/D2 闭合、O1/O2 大半清偿（各余一条 owner 裁决级尾款）、O5/W11 未动（登记与卫生债，非行为债）。第六轮快扫在五大线射程内未发现新的承诺面遗漏；结构性风险仍=时窗漂移与射程外域，维持 [M](M-最终判定.md) §3.3 触发器制，本轮不重开全扫。本件写入后需复跑 check-docs 确认零警（写入时点基线 343 篇）。
