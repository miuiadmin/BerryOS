# F 线:P1 可做面扫描——上轮 42 件逐项标注·可做面·owner 域·P2 快扫

Type: research
Status: current
Updated: 2026-11-15

> 版本:v1.0(2026-11-15;文件名按任务书指派不带版本号,版本记于本行)|性质:第二轮遗漏审计任务 B——上轮四路审计([A 调度](./A-调度线遗漏审计.md)·[B 显示桌面](./B-显示与桌面线遗漏审计-v1.0.md)·[C 定制化 PM](./C-定制化与PM线遗漏审计.md)·[D 跨线](./D-跨线依赖与文档漂移审计.md))P1 级条目逐项标注占用态与归届,产出「现在就能修」的可做面、owner 域窗条件与 P2 顺手清单;只查不改,除本件外零仓内写
> 锚:四份审计稿 P1/P2 段全文 + [E-修复验证与回归](./E-修复验证与回归.md)(本轮姊妹任务,P0×3 验证实录)+ 两篇在途笔记([刀 A 合流解锁](../../.agents/notes/implemented/2026-11-15-刀A合流解锁.md)·[弹簧刀落地](../../.agents/notes/implemented/2026-11-15-弹簧刀落地.md),占用态判读依据)
> 采集窗:2026-11-15 21:57-22:05;方法 = git status 双快照对拍(只读,零 git 写操作)+ 逐锚点 grep/直读复验(凡「原样/仍在」均为本轮直读,非转抄)+ 计数实跑;行号 = 本轮直读值,开刀时现场重数
> 边界:禁 git 写;berry-guiproto/gui-wmd = owner 占零碰;tools/berry+justfile 22:02 起转在途占用(§1),同域 P1 一律归「待窗」不入可做面;本件为唯一产出
> 推断声明:§2 口径复原(42 件账本与调度方底册的对应)、§6 ramfb 第二病现态、fwcfg 工具归属、smoke-image 影响面——均随文标「推断」;其余结论有双源(审计稿行号 × 本轮复验)

---

## §0 总判(一屏版)

| # | 审计项 | 判 | 一句话 |
|---|---|---|---|
| 1 | 42 件口径复原 | 33 件漂移/遗漏型 + CP-08 未闭窗 10 件 = 43,与 ~42 吻合(推断:±1 出在 CP-08 半闭件或 A①/D-P0 同物计法) | 本轮逐件复验:3 件本轮 P0 已清(vgpu/门控/bcts@1)、2 件前轮 46 号已清、1 件已消化,开放 37 件(§2) |
| 2 | 占用态 | **三域占用,余全域空闲** | owner 长占 berry-guiproto/gui-wmd;刀 A 会话(wip/land-a)22:02 起在途 justfile+tools/berry×5;环境噪声若干——kernel/sched/arch/svcmgr/berry-ui/guifbd/docs/berry-launcher 全闲(任务书「justfile 空闲」已被在途改动打破,开刀前必复查) |
| 3 | 可做面 | **12 件**(§3,按影响排序) | 首位 = 显示设计 v1.0→v1.1 升版(与裁六竖屏回填同刀);docs 域 9 件全闲全无阻塞,码刀 3 件(bench.py 逃生位/断点 vp 化/util 域 0)亦闲 |
| 4 | owner/待窗域 | 25 件(§4) | 集中三类:owner 裁决类(CP-02/CP-08 状态列、uclamp 等刀单缺口)、V13/HVF 等外部窗类、tools/berry 在途类(窗口 = 刀 A 收笔提交) |
| 5 | P2 快扫 | 15 件中 **7 件 1-3 行修可顺手 + 2 件半**(§5) | nr_total 37→38、索引 0x88、共享环限定、berry- 计数 38→70(净数)、INTENT 断言诊断行、FACES 一行修为最高值六连;刀 2①(qemu.rs 注键)同为 1 行但待窗 |
| 6 | 正册外残留 | 4 件高价值(§6) | B 线 P0 第二病(ramfb 32 对齐)静态核未见入库;D-P0-2 集成指南 auth=0 示例留窗归 owner;刀 5 验收线(VGPU 标记零门禁)未随 P0-① 闭合;A/D 两稿 API 计数 149 vs 145 冲突待 v1.8 精数裁决 |

---

## §1 占用态基线(实跑双快照)

21:57 快照(任务书「全域空闲」时点):仅 `M user/berry-guiproto/src/lib.rs`、`M user/bins/gui-wmd/src/main.rs`、`M user/bins/gui-wmd/src/table.rs` 三件 + 9 件未跟踪。22:02 快照(本稿基准):

| 域 | 文件 | 性质 | 纪律 |
|---|---|---|---|
| owner 长占 | `user/berry-guiproto/src/lib.rs`;`user/bins/gui-wmd/src/main.rs`、`src/table.rs` | owner 会话(wm 事件通道族/焦点表,刀 A 笔记「边界备忘」段点名归波 3 线) | 零碰 |
| 刀 A 在途(新) | `M justfile`;`M tools/berry/src/{commands,device,doctor,qemu,run_dir}.rs`(合计 +149/-39;qemu.rs 独占 85 行) | wip/land-a 刀 A 会话落地中:入口收敛 default desktop+auto、计数锁数 APP ok n=9/APPUI ok n=8、doctor/run_dir 扩面;未跟踪 `tools/fwcfg-check.sh`+`tools/fwcfg-keys.toml` 推断为其配套(fw_cfg 键表单源+对拍工具) | 待其收笔提交;tools/berry 整 crate + justfile 视为在途域,同域 P1 一律待窗 |
| 环境噪声 | 未跟踪:`1`、`px-clean.log`、`px-drop.log`、`px-rst.log`、`userbins/`、`target-baseline/`、`user/target/` | 垃圾/构建树,不碰不清 | 无 |
| 过程件(已入册或本批) | 未跟踪:`.agents/notes/implemented/2026-11-15-刀A合流解锁.md`、`2026-11-15-弹簧刀落地.md`、`research/全面遗漏审计/E-修复验证与回归.md` | 刀 A/弹簧刀/E 线自落,不代管 | 无 |

空闲确认(本轮 git status + 逐锚点直读):kernel/、sched/、arch/、user/bins/svcmgr/、user/berry-ui/(弹簧刀 16f335a26 已入库,工作树净)、user/bins/guifbd/、user/bins/berry-launcher/、user/berry-sys/、user/bins/berry-power/、user/bins/berry-thermal/、docs/(用户指南 v1.1 与 berry-sys 修已提交)、tools/*.py(berry_assemble/bench/mkcpio/gensourceindex)、tools/bcts-run.sh、tools/markers.sh、tools/smoke*.sh、root README.md——全部零改动。

---

## §2 P1 逐项标注(42 件总账)

口径(推断:调度方底册未公开,按四稿复原):A 线 CP-01 十处漂移 + A 遗漏三件 + B 线 P1 六件 + C 线 44 号 P1 七件 + C 新发现二件 + D 线 P1 六件 = 34 行,同物去重一(A⑩ = D-9)得 33 件漂移/遗漏型;加 CP-08 十三件窗未闭十件(开八 + 半闭二)合 43,与「~42」吻合。状态:本轮 P0 清三(F-B1/F-A1/F-C8)、前轮 46 号清二(F-C1/F-C2)、已消化一(F-C9),开放三十七。

### 2.1 A 线(调度,13 件)

| # | 件 | 锚(本轮复验) | 占用 | 归届 | 前置/备注 |
|---|---|---|---|---|---|
| F-A1 | CP-01① 指南门控表述 | [用户指南 v1.0](../../docs/策略服务用户指南-v1.0.md):41 门控段 + :290 版记 1.1(2026-11-15) | docs 闲 | **本轮 P0-② 已闭** | E §1.2 PASS 六点;同根因残留见 §6-2 |
| F-A2 | CP-01② placement 表述矛盾 | 用户指南:114「placement 暂无消费」vs [设计 v1.2](../../docs/调度器设计-v1.2.md):342-348「第一消费者已落地」(ktest) | docs 闲 | 可做 | 机械改口,一行级 |
| F-A3 | CP-01③ AI 调度传输表述 | [AI 调度设计](../../docs/AI调度设计-v1.0.md):42「共享环形缓冲」原样 | docs 闲 | 可做 | 随裁十债二直传终裁改口;与 F-D4 同文件域不同文件 |
| F-A4 | CP-01④ SLEEPQ 口径差随迁改指 | [计划 v1.2](../../docs/调度彻底计划-v1.2.md):93 清单在册 | docs 闲 | 可做(低位) | 需先定位改指目标文;机械 |
| F-A5 | CP-01⑤ util 域 0 硬编码 | [util.rs](../../sched/src/util.rs):121 `em_max_capacity(0)` 原样 | sched 闲 | 可做 | 06 号 §1.5-1 修订项;小码刀 + 单测 |
| F-A6 | CP-01⑥ R3 随迁消费文档改指 | 计划:93 清单在册 | docs 闲 | 可做(低位) | 同 F-A4 形态 |
| F-A7 | CP-01⑦ README AI-native 措辞 | [README.md](../../README.md):5/:23 原样 | 闲 | owner | 替代措辞归 owner 定夺 |
| F-A8 | CP-01⑧ CP-02 状态列滞后 | 计划:94「唯一硬阻塞 = 仓主裁决…R'≥18」原样 | docs 闲 | owner 裁 | 窗:X-6 条件 GO 是否即「终裁出果」owner 拍板;DPGF n=15+DPGF2 n=3 完整确认待 QEMU 空窗 |
| F-A9 | CP-01⑨ CP-08 状态两源冲突 | 计划:100「未动」vs 沿革 337「闭合」 | docs 闲 | owner 裁 | 窗:术语半边归属随 CP-01 重锚划清(建议改「装配+预拉已落·通道未接」,判词归 owner) |
| F-A10 | CP-01⑩/API 漏登 oneshot | [API 参考 v1.7](../../docs/调度器API参考-v1.7.md):3 自称 12 文件/148 fn;现树 13 文件(本轮 ls 实数) | docs 闲 | 可做 | 升 v1.8:oneshot 收录 + 计数精数——A 稿 149 与 D 稿 145 冲突,本轮 grep 行命中 145(含 doc 注释误命中风险),v1.8 刀须逐文件精数裁值 |
| F-A11 | A 遗-1 uclamp/EPP 承诺悬空 | 设计 v1.2:132;CP-11 刀面未含此子件 | docs 闲 | owner 裁 | 窗:扩 CP-11 刀面或设计改口,二择 |
| F-A12 | A 遗-2 IntentProfile 运行时更新/回退断链 | 设计 v1.2:118 承诺 vs 内核零加载 | docs 闲 | owner 裁 | 窗:CP-11 验收线补记机制面现状(承诺-刀单-现状三点断链) |
| F-A13 | A 遗-3 obligations 位图挂账 | [调点 v1.1](../../docs/调点调度-v1.1.md):601 自记「挂 1.2」 | docs 闲 | owner 裁 | 窗:开放问题 10 清偿或降级 ADR 追记 |

### 2.2 B 线(显示桌面,6 件)

| # | 件 | 锚(本轮复验) | 占用 | 归届 | 前置/备注 |
|---|---|---|---|---|---|
| F-B1 | berry CLI 挂 virtio-gpu | [qemu.rs](../../tools/berry/src/qemu.rs):273(`-device virtio-gpu-device`)先于 :274 ramfb | **本轮 P0-① 已闭**(ce8a708ae) | — | E §1.1 PASS;验收线残留见 §6-3 |
| F-B2 | 刀 1② 窗口 1:1 + 单测翻新 | [window.rs](../../tools/berry/src/window.rs):202-213 因子 0.55/0.9 原样;:1206-1216 单测仍旧态(540×960→486×864) | tools/berry 在途 | 待窗 | 窗:刀 A 收笔;行为变更,收笔后跑 tools/berry 55 测(README:243 咒语) |
| F-B3 | 刀 1③ 断点 vp 化 | [desktop.rs](../../user/bins/berry-launcher/src/desktop.rs):317 像素阈原样;:98 `OH_DENSITY=3` 硬编;berru-ui `fn vp` 零命中(本轮 grep) | berry-launcher/berry-ui 闲 | 可做 | vp() 单点入 berry-ui;2K/4K/OEM 档前置欠账;开刀前复查 berry-ui(弹簧刀刚入库) |
| F-B4 | 刀 1④ README 档位表对账 | [tools/berry/README.md](../../tools/berry/README.md):186 desktop 行仍「540×960」vs [profile.rs](../../tools/berry/src/profile.rs) desktop=1080×1920 竖屏 | tools/berry 在途 | 待窗 | 窗:刀 A 收笔;一行修 |
| F-B5 | 裁六方向翻转文档回填 | [显示设计 v1.0](../../docs/显示与加速体系设计-v1.0.md):73 仍横屏裁六;:4-6 Version 1.0.0/2026-11-13 | docs 闲 | 可做 | 与 F-D6 升 v1.1 同批同刀(owner 竖屏实裁已在 profile.rs:31-33 注) |
| F-B6 | 两处 owner 注释冲突清理 | [tools/berry/src/main.rs](../../tools/berry/src/main.rs):887-889 仍「横屏 1080P + 硬件加速」 | tools/berry 在途 | 待窗 | 刀 A 入口收敛(default 翻 desktop+auto)或已半消化,收笔后复查再清注 |

### 2.3 C 线(定制化 PM,9 件)

| # | 件 | 锚(本轮复验) | 占用 | 归届 | 前置/备注 |
|---|---|---|---|---|---|
| F-C1 | 44 号 P1-3 --preset 首产鸡蛋循环 | [gensourceindex.py](../../tools/gensourceindex.py):681 豁免在 | 闲 | **前轮 46 号已闭** | E §2 selftest 绿;双源 |
| F-C2 | 44 号 P1-4 G8×G3 manifest 名联动 | [bcts-run.sh](../../tools/bcts-run.sh):124-128/:168-182 | 闲 | **前轮 46 号已闭** | E §2 33/33 腿绿 |
| F-C3 | 44 号 P1-5 berry-sign 长效钉板 | 现役位已重建;长效钉板(CI/配方)未落 | justfile 在途 | owner | 窗:CI 改动窗 + justfile 空闲;跨 checkout 混写新证在册(10 号 §1.5) |
| F-C4 | 44 号 P1-6 --emit-entries 机器出口 | [berry_assemble.py](../../tools/berry_assemble.py) 零命中(本轮 grep 复验) | 闲 | owner(V13) | 窗:V13 全产线开工;在册归属明确,抢跑需 owner 点头 |
| F-C5 | 44 号 P1-7 OEM 可视自验不可达 | HVF fatal 族 + TCG 首帧饿死 | kernel/arch 闲(真修面广) | owner;**文档面可做** | 真修窗:HVF 残余两件时序 panic 收口 + 六波波 1 交付;短期「文档明示 OEM 可视自验边界」一句未见落档(本轮 docs grep 复验),可先落 |
| F-C6 | 44 号 P1-8 bench.py 预检死等 | [bench.py](../../tools/bench.py) 零 force-serial(复验);qemu_preflight :367 起无逃生位 | 闲 | 可做 | 加预检逃生旗标 + selftest 腿;45 号锚 :362-387(现移 :367+) |
| F-C7 | 44 号 P1-9 locale 八键零投放 | berry_assemble.py:1047 立法注记原样;:105 V13 归属注 | 闲 | owner(V13) | 窗:V13 投放面或设置应用 v2/berry-intl 读者立项;非悬空,滞后收编 |
| F-C8 | C 新-1 BCTS 冻结件代次断档 | [bcts@1.toml](../../tools/bcts/frozen/bcts@1.toml):15 pr=1(floor 90);[bcts-t2@1.toml](../../tools/bcts/frozen/bcts-t2@1.toml):28 pr=1 | 闲 | **本轮 P0-③ 已闭** | E §1.3 实跑 img_pr_check PASS + bcts@0 负对红 |
| F-C9 | C 新-3 44 号 P0-2 splash 表述过时 | 44 号文内表述 vs owner 11-15 bootanim 作废裁决 | 闲 | 已消化 | C 线稿 §1.2/§5-3 已立账;历史研究件不回改,引用处以 C 线为准(判读) |

### 2.4 D 线(跨线,6 件)

| # | 件 | 锚(本轮复验) | 占用 | 归届 | 前置/备注 |
|---|---|---|---|---|---|
| F-D4 | 计划裁九/CP-10「berry-power 未建」 | [计划 v1.2](../../docs/调度彻底计划-v1.2.md):81/:107 原样 | docs 闲 | 可做 | 与 F-D5 同批三处一句口径:「骨架已建、通道未连」 |
| F-D5 | berry-power 窗 1 注记 + rootfs 注释 | [main.rs](../../user/bins/berry-power/src/main.rs):18「未被拉起」原样;[rootfs-manifest.toml](../../tools/rootfs-manifest.toml):50「预拉 arm 未挂」原样(svmgr arm 实已落) | 闲 | 可做 | 同上,三文件各 1-2 行 |
| F-D6 | 显示设计 v1.0 未随 B1 回填 | Version 1.0.0/Updated 2026-11-13 原样;13 号「升版落字清单五条」在册未执行 | docs 闲 | 可做 | 五线滞后最大 standard;含 F-B5 同刀;五条内容已预制(13 号文末) |
| F-D7 | CLI-first 名/版本断裂 | 文件名 -v1.0 vs [头 :3](../../docs/Berry设备平台-CLI-first设计-v1.0.md)「1.8」;[索引.md](../../docs/索引.md):104 停在 1.5 | docs 闲 | 可做 | 改名 v1.8 + 索引行 + 入链清点(违反 docs 维护规则 5 的最重案例) |
| F-D8 | 信号位登记表缺 1<<50/51 | [规则/04](../../docs/规则/04-调度与IPC.md):44 止于 1<<49(本轮直读);内核 main.rs:3873-3874 已有两位 | docs 闲 | 可做 | 两行修;单一事实源失守件,唯一「表本身缺行」型 P1 |
| F-D9 | API 参考影抄计数漂移 | 同 F-A10(v1.7:3 口径 12/148 vs 现树 13/145-149 待精数) | docs 闲 | 可做(=F-A10) | 一刀双销;A/D 冲突裁值归 v1.8 刀 |

### 2.5 CP-08 十三件窗未闭十件(PM 线,owner 域整体)

闭四(F1 桥/双服务常驻化/test-host 挂行)半闭二开七(C §3 口径:闭 4+半闭 2+开 7,与逐行 闭 3+半 2+开 8 有 1 件计差——#12 断言面,两说并列):#1 util 通道形态终裁(OQ-8 悬)、#2 内核聚合点零代码、#4 F2 分端执法(并入 #7/#8 装配件)、#7/#8 thermal 输入/输出三面留窗(berry-thermal main.rs:55/:61-63 注在)、#9 chargerd 无本体、#10 JEITA 装载面、#11 smoke 四件套零标记、#12 内核两断言面零(「零频率写点」事实已绿断言未立)、#13 THERM 遥测臂 + KC/SLEPT 闭环开。占用:kernel/svcmgr/berry-thermal/markers.sh 全闲。归届:整体 owner——窗条件 = OQ-8 通道形态终裁(共享环 vs 快照 VMO)→ 真通道立法刀开窗;#12 的「零频率写点」断言为半可做(静态性质可先立 ktest/script 断言,推断)。

---

## §3 可做面清单(已空闲且无阻塞,按影响排序)

1. **F-D6 + F-B5:显示设计 v1.0 → v1.1 升版**(docs 闲)。五线滞后幅度最大的 standard;一刀收五条(13 号落字清单:刀 5 名称改 berry-gpuproto、刀 3 判 GO 回填、60fps 排刀重排、VNC 33fps 硬顶、风险翻状态)+ 裁六竖屏定谳回填(:73 触发器 8 首用记录)。影响:显示线索引链完整度最低局面的主根。
2. **F-D8:规则/04 信号位表补 `1<<50`/`1<<51` 两行**(docs 闲)。登记表自称「新增用户位前先查本表碰撞」单一事实源,表缺行即失守——42 件中唯一结构性失守件,两行修,值最高。
3. **F-D4 + F-D5:berry-power 现状三处口径回填**(docs/berry-power/rootfs-manifest 全闲)。计划 :81/:107「未建」、main.rs:18「未被拉起」、rootfs-manifest:50「arm 未挂」——统一「骨架已建、通道未连」一句,D 线建议 3 原案,三文件各 1-2 行。
4. **F-A10/F-D9:API 参考 v1.8**(docs 闲)。维护合同(:12「改 pub fn 须同步升版」)已破;oneshot 收录 + 12/148 口径翻新 + A/D 计数冲突(149 vs 145)逐文件精数裁值——影抄文档恢复可信。
5. **F-B3:launcher 断点 vp 化**(berry-launcher/berry-ui 闲)。desktop.rs:317 像素阈改按 vp 宽 + OH_DENSITY 单源 + berry-ui `vp()` 单点;03 号裁八体系化,2K/4K/OEM 档前置;码刀中级,berry-ui 328 测与 launcher 构建面可验。
6. **F-C6:bench.py 预检逃生位**(闲)。qemu_preflight 对长会话死等加 force/跳过旗标 + selftest 腿;OEM/工具摩擦面,python 可独立验。
7. **F-D7:CLI-first 改名 v1.8 + 索引行同步**(docs 闲)。文件名/头版/索引三处对齐 + 入链清点;违反维护规则 5 的最重存量案。
8. **F-A2:用户指南 :114 placement 表述改口**(docs 闲)。一行级,消除与设计 v1.2 的正面矛盾。
9. **F-A3:AI 调度设计 :42 传输表述改口**(docs 闲)。一行级,随裁十债二终裁落地(注意与 F-D P2-13 共享环措辞限定联动,同批更优)。
10. **F-A5:util.rs 域 0 硬编码退役**(sched 闲)。06 号修订项,`em_max_capacity(0)` 改真实域参;小码刀 + sched 侧单测。
11. **F-A4/F-A6:SLEEPQ 口径差与 R3 随迁改指**(docs 闲,低位)。机械改指,先定位目标文。
12. **F-C5 文档面:OEM 可视自验边界一句明示**(docs 闲)。真修归 owner,短期文档刀先落(C 线 §1.2 点名未见落档)。

---

## §4 owner 域与待窗域清单(列窗条件)

| 件 | 归属 | 窗条件(解锁即动) |
|---|---|---|
| F-A7 README AI-native 措辞 | owner | owner 定替代口径后一行级 |
| F-A8 CP-02 状态列 | owner 裁 | 「条件 GO = 终裁出果」拍板;DPGF n=15+DPGF2 n=3 完整确认(QEMU 空窗)复现 smoke-smp 八相 |
| F-A9 CP-08 状态列 | owner 裁 | CP-01 重锚时划清术语半边(装配+预拉 vs 通道) |
| F-A11/F-A12/F-A13 刀单缺口三件 | owner 裁 | 扩 CP-11 刀面/补记现状/ADR 追记,三件均改计划范围 |
| F-B2/F-B4/F-B6 tools/berry 三件 + 刀 2①(P2) | 待窗(刀 A) | 刀 A 会话收笔提交、git status 清占用后复查(main.rs 注释或已被入口收敛半消化) |
| F-C3 berry-sign 长效钉板 | owner | CI 改动窗 + justfile 空闲 |
| F-C4 --emit-entries | owner(V13) | V13 全产线开工 |
| F-C7 locale 投放 | owner(V13) | V13 投放面或读者(设置应用 v2/berry-intl)立项 |
| F-C5 真修(OEM 可视) | owner | HVF 残余两件时序 panic 收口 + 六波波 1(guifbd 过渡屏)交付 |
| CP-08 窗十件(§2.5) | owner | OQ-8 通道形态终裁 → 真通道立法刀;#12「零频率写点」断言可先立(半可做,推断) |
| 集成指南 §二示例 auth=0(D-P0-2 残留,§6-2) | owner(显示线) | owner 显示线文档窗;建议同窗并 §4.2 128→250 回填(:129 原样复验)与 §1.2 intent 0x02 采纳决策 |

---

## §5 P2 快扫(15 件:1-3 行修可顺手清单)

口径:D 线 P2×6(:83-88 之 #10-15)+ C 线 44 号 P2×6(#10-15)+ B 线 P2 三行(刀 2①③/桌面五痛收尾/刀 4·9)。

**可顺手(1-3 行,文件全闲)**:

1. D-10 [规则/03](../../docs/规则/03-系统调用.md):20「nr_total 37」→ 38([abi.toml](../../tools/abi.toml):33 实值 38,本轮直读)——一行;或改引 abi.toml 单源不写死值。
2. D-11 [索引.md](../../docs/索引.md):49 系统调用 ABI 行补 0x88(power_off 已实现)——一行。
3. D-13 [规则/04](../../docs/规则/04-调度与IPC.md):14「共享环形缓冲」补「通道形态 CP-08 待裁」限定(对齐调度器设计 v1.2:366 的限定形)——1-2 行;与 F-A3 同批更优。
4. D-12 [命名规范](../../docs/命名与前缀规范-v1.1.md):18/:41「38 个」→ 净实数:本轮实跑 **70**(排除 `.dsh-worktrees`/`.claude` 两棵 harness 树后 `name = "berry-"` 计数;D 稿 65 为 11-15 时点)——一行,须带净数纪律注(harness 树混入即翻倍,直接 find 会得 1227 假值)。
5. C-14 [kernel/main.rs](../../kernel/src/main.rs):3670 INTENT 断言行内中文消息 → 独立诊断行(eprintln + assert)——2-3 行;kernel 闲;46 号 §4.3 顺手刀候选原案。
6. B-④ FACES 冻结相一行修:[icons.rs](../../user/bins/berry-launcher/src/icons.rs):155-157 `app_h == 0` 早退分支补 `FACES.set(status_faces())`(pull 失败分支 :163 已有同款)——1-2 行;berry-launcher 闲;无数据盘拓扑状态栏即有图标。
7. B-刀 2① spawn_kernel 注 `opt/berry/accel` 键:qemu.rs 1 行级——**值 1 行但文件在途**(刀 A 正改 qemu.rs 85 行),归待窗。

**半顺手(判读)**:C-12(44 号 P2-12 现状已在文——建议不动历史研究件,引用面以 C 线稿为准,零行);C-10(berry_assemble 计划面补 CWD 注记 1-3 行,低值)。

**非顺手**:D-14 六波方案回写(波次勾选+34→41+bootanim 作废,多处>3 行);C-11 trust-pk(P3 策略刀在册);C-13 paths.sh CI 指稳(owner/CI 窗);C-15 FRZ 方差(无需动作);B-⑤ 按压反馈消费面(接线>3 行,set_press_map 零调用复验);B-⑥ kerning/CJK(挂账随段④);B-刀 4/9 density/档位(设计位,依赖刀 6/8 解锁)。

---

## §6 正册外残留与风险(只报)

1. **B 线 P0 第二病(ramfb 32 对齐)推断仍开**:恒黑第一病已愈([berry-ui lib.rs](../../user/berry-ui/src/lib.rs):211 `A11Y_MAX = 40`,01bb3e37a 同款注释);但 1080 宽非 32 对齐的 surface 断裂修,本轮静态核 qemu.rs/guifbd 零对齐面(guest.rs 仅 vgpu 路有 stride 处理,:139/:155-158)——推断未入库,需实跑 `just desktop` 定谳;修窗在 qemu.rs(在途)/guifbd(闲)。
2. **D-P0-2 残留:集成指南示例仍带毒**(P0 级,归 owner):[集成指南](../../docs/显示管线调度集成指南-v1.0.md):76-77 STATUS/:90 COMMIT 仍 auth=0 裸调(本轮直读),用户指南 :290 版记自记「归显示线 owner 留窗修」——A/B 工作流文档面在该文修复前整体不可执行;P0×3 全清口径不含此件,排刀时勿漏。
3. **刀 5 验收线未随 P0-① 闭合**:VGPU 族标记(`VGPU2 reap/rect`)在 markers.sh/smoke-image/bcts 全零(本轮 grep 复验);smoke-image/bcts 走自拼 qemu 命令线(smoke-image.sh:95)不经 berry CLI,推断不受恒挂影响;berry CLI 面的 gui8-smoke/vnc-smoke 复跑归刀 A 收笔后回归窗(guifbd 遇占用槽 ver≠2 弃 gpu 落 ramfb 缺省安全,qemu.rs:250 注)。
4. **A/D 计数冲突待裁**:A 稿「13 文件/149 pub fn」vs D 稿「13/145」vs 本轮 grep 行命中 145——v1.8 刀逐文件精数后落值,勿抄任一旧数。
5. **索引/沿革/测试面在册项不动重复计账**:沿革 267/268 双占(:3918/:3933 vs :6415/:6422 原样)+300 系 19 空号;规则/05 未收 berry-power/berry-thermal 路由(本轮 grep 零命中复验);规则/04 深读节缺四件路由行(复验零命中);test-host 零 CI + berry-launcher/guifbd/berry-libc 单测从未跑(E §6.3 同判);pm-integration 命名孤例;berry-gpuproto 未入 docs/索引.md——均 D 线在册,本轮只确认未恶化。
6. **占用态时变性**:本稿占用列为 22:02:27 快照;21:57→22:02 间 tools/berry 域从空闲转在途(刀 A 落地中)——可做面开刀前逐文件复查 git status 为硬前置。

## §7 来源与复核说明

- 审计谱系:四份上轮稿(P1/P2 段全读)+ [E-修复验证与回归](./E-修复验证与回归.md)(P0×3 验证 + selftest/cargo/smoke 回归全绿);在途笔记两篇(刀 A 合流解锁/弹簧刀落地)判读 tools/berry+justfile 占用归属。
- 本轮实跑:git status 双快照(21:57/22:02,只读);sched 文件数 ls(13);pub fn 行命中 grep(145,含误命中风险已注);berry- crate 净计数 70(排除 harness 树);bcts@1/bcts-t2@1 双件直读(pr=1);三 P0 修复件直读复核(qemu.rs:273-274/用户指南 v1.1 版记/berry-sys :573/:614 签名);其余逐锚点 sed/grep 复验(正文各表「原样」字样即本轮直读)。
- 推断项清单:§2 口径复原、§6-1 ramfb 现态、§6-3 smoke-image 影响面与 gui8/vnc 复跑归属、fwcfg 工具归属刀 A、CP-08 #12 半可做——均随文标注。
- 零 git 写操作;除本件外零仓内写;berry-guiproto/gui-wmd/tools/berry 在途六件/justfile 零碰。
