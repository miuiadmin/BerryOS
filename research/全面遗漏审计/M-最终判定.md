# M——最终判定（全面遗漏审计第五轮·终验）

> 版本：1.0 ｜ 日期：2026-11-16 ｜ 定位：第五轮终验——四轮审计（R1 四路对拍 A/B/C/D → R2 修复验证与扫描 E/F/G → R3 落地与终审回归 cb726accd 全量 100%+fbe1d6665 修复批 → R4 语义级 K/L）之后的系统终态完整性判定，回答三个终极问题：①还有什么没查过（覆盖面交叉）②已知未修项完整清单（归届：owner 域/设计意图/待窗，最终版）③「审计完成」Y/N。方法：必读三件（[README](README.md)/[K](K-语义完整性.md)/[L](L-owner新件扫描.md)）全读 + 四轮审计件 A-L 九件（共 1315 行）通读交叉 + 未修项锚点 40+ 处本轮直读复验（修毕/仍开一律以今日树为准，不转抄旧账；每条给路径+行号）+ 门禁实跑；推断标「推断」。边界：零 git 写，全仓只读，唯一产出件=本件；kernel/ 与 tools/berry 的 qemu/web 为 owner 在途占用面（git status 脏）零碰；README 登记行按 K/L 判例留主会话随批登记（并注 F 行缺，见 §2.4-W11）。锚定态：HEAD=46dd5b59b + 2026-11-16 深夜工作树快照（owner 输入线在飞：kernel/src/main.rs、tools/berry/src/qemu.rs、tools/berry/src/web.rs 未提交改动）——时点性结论以此为准。

## 〇、终验结论速览

| 问 | 答 | 详 |
|---|---|---|
| 1 还有什么没查过 | 审计射程（五大线承诺面）**闭合**；交叉后余六类盲区（射程外域/语义级仅调度/长跑与 SMP 证据/owner 在途面/时窗漂移/元登记与测试执法）——全非射程内漏查，属「射程外」或「滞后重扫」 | §一 |
| 2 已知未修项 | **32 件**（owner 域 10／设计意图 11／待窗 11），全部有归届零悬空；另有 24 项已清偿正面确认（本轮逐锚复验） | §二 |
| 3 审计完成判定 | **Y（有界）**——就本审计射程（五大线设计承诺 vs 实现的遗漏清册）判定完成，终验通过；四条适用边界+三条重开触发器 | §三 |

## 一、问题 1：还有什么没查过——四轮覆盖面交叉

### 1.1 覆盖账（轮 × 件 × 方法 × 证据形态）

| 轮 | 件 | 域 | 方法 | 证据形态 |
|---|---|---|---|---|
| R1 | [A](A-调度线遗漏审计.md) | 调度（CP-01..19/差距矩阵/设计 §3.8/API 参考/KC-P1..P5） | 承诺清单化→代码逐项对拍 | 静态（grep/直读；QEMU 零起） |
| R1 | [B](B-显示与桌面线遗漏审计-v1.0.md) | 显示加速（刀 1-9/十裁）+桌面 UX 六痛点+β-1 壳半+调度集成指南 | 同上 | 静态+研究件实录引证 |
| R1 | [C](C-定制化与PM线遗漏审计.md) | 定制化（G16+44 号 15 缝+BCTS+28 键）+PM（CP-08 十三件窗） | 同上 | 静态 |
| R1 | [D](D-跨线依赖与文档漂移审计.md) | 跨线四缝+文档漂移 15 件分级+索引/沿革/测试面三分类+规则对齐 | 双源交叉+机扫 | 静态（测试面按调用路径判，非运行记录） |
| R2 | [E](E-修复验证与回归.md) | P0×3 验证+全 selftest/cargo/smoke boot 回归 | 命令逐件实跑 | 实跑（exit code+日志） |
| R2 | [F](F-P1可做面扫描.md) | 上轮 42 件逐项标注（占用/归届/可做） | git 双快照+锚点复验 | 静态 |
| R2 | [G](G-新件集成检查.md) | 新件入册五面（crate/docs/ADR/README/test-host） | 对拍+七命令实跑 | 混合 |
| R3 | cb726accd+fbe1d6665（非独立审计件） | 全量回归+审计修复落地 | 实跑门禁 | 实跑（commit 自证） |
| R4 | [K](K-语义完整性.md) | 五链语义级（intent→band/EAS/loadgen/PM util/timer slack） | 代码链×测试×QEMU 留档三问 | 混合（QEMU 取留档日志） |
| R4 | [L](L-owner新件扫描.md) | owner 8 笔 192 文件四问（新增/共享/ADR/覆盖回退） | git 只读+HEAD 直读 | 静态 |

### 1.2 六类漏网面（交叉后剩下的盲区——按风险排序）

1. **射程外域（最大类）**：审计方法=「承诺 vs 实现」对拍，无承诺文档的功能面结构性进不了射程——网络栈 TCP 两刀（18388fb54，自证门 berry-tcp 53/53 等，[L](L-owner新件扫描.md) §2.4 仅集成面）、mm/fs/ipc/drivers/bootloader/x86 域、波 3 桌面三栏 7 系统应用+berry-appshell+gui-wmd（[L](L-owner新件扫描.md) §2.3 仅登记面四问）、46dd5b59b 启动产物收敛批（零审计线覆盖）。漏网类型=「实现了但没人承诺过」的行为缺陷与漂移；这些域现以各自 commit 终门自证为准。
2. **语义级仅调度五链**：K 的三问（代码链/测试覆盖/QEMU 真跑过）只用于调度域——B/C 线无同级「端到端真的在工作」复核；PM 链 K 只裁到通道（thermal 输入/输出/chargerd 全留窗零语义审计，[C](C-定制化与PM线遗漏审计.md) §3 #7-#9）。
3. **长跑/压力/SMP 真跑证据**：soak/bench 族四轮零实跑复证（全引在册记录；E/F 只读纪律明示）；SMP 证据结构性薄弱——EAS 选核自证行全留档零命中（K §2.2）、smoke-smp 曾整档丢弃（现八相轮转已修 [tools/smoke-smp.sh](../../tools/smoke-smp.sh):109-118，但默认仍不留 out/logs）。
4. **owner 在途占用面**：kernel/src/main.rs、tools/berry/src/{qemu,web}.rs 现刻脏树（git status 实测）、berry-guiproto/gui-wmd 长占（[F](F-P1可做面扫描.md) §1）——行为级审计必然滞后；本轮输入线两研究件修改中+两新件在飞（.agents 会审/诊断件），恰证明该面活跃。
5. **时窗漂移**：四轮快照时点散布 11-15 19:50（B）→11-16（K/L）→本轮，期间 owner 四笔合流（63fbca1c9/fbe1d6665/18388fb54/46dd5b59b）——每笔自带终门但零独立审计；本轮 40+ 锚点抽验≠全量重扫（「审计后新增量」是持续性风险，非一次性可清）。
6. **元登记与测试执法**：审计自身索引未对齐（本目录 [README](README.md) 表 A-L 十件中 **F 行缺**——本轮 grep 实证 0 命中）；审计结论行号时效（L §六.2 行号漂移判例——回引以符号名定位）；测试执法面（test-host 2030 测零 CI，除 ksproto/mmap/signal 三件+探针族 fail-fast [ci.yml](../../.github/workflows/ci.yml):147-168/:468-491——「本地绿」依赖手动纪律）。

### 1.3 判语

六类中没有一类是五大线承诺面内的漏查——射程内逐项闭合（Q3 判 Y 的基础）；其中 1/4/5 是持续风险，已给重开触发器（§3.3），2/3/6 是方法与基建欠账，随触发器或 owner 排刀消化。

## 二、问题 2：已知未修项完整清单（最终版）

### 2.1 已清偿项（24 项正面确认——本轮逐锚复验，不进未修账）

| # | 件 | 锚（本轮直读） |
|---|---|---|
| 1 | P0-a vgpu 恒挂+声明序 | [qemu.rs](../../tools/berry/src/qemu.rs):296-299（演进：BERRY_NO_VGPU 诊断开关） |
| 2 | P0-b 0x83 门控文档+wrapper | [策略服务用户指南](../../docs/策略服务用户指南-v1.0.md) v1.1 版记+berry-sys 签名（[E](E-修复验证与回归.md) §1.2） |
| 3 | P0-c bcts@1 双件 pr=1 | [tools/bcts/frozen/](../../tools/bcts/frozen/) 双 @1 件（[E](E-修复验证与回归.md) §1.3） |
| 4 | 44 号 P1 #3/#4/#5 | [gensourceindex.py](../../tools/gensourceindex.py):681/[bcts-run.sh](../../tools/bcts-run.sh):124-182/berry-sign 重建（[C](C-定制化与PM线遗漏审计.md) §1.2） |
| 5 | timer_slack 第三键+机制面 | [bootparams.rs](../../kernel/src/bootparams.rs):19/:65/:258-259/:338-339（CP-11 机制面三缺已破，K 链 5 实测） |
| 6 | API 参考 oneshot 漏登 | [调度器API参考-v1.9](../../docs/调度器API参考-v1.9.md)（225f3bbef，slack 三 fn 同批） |
| 7 | 显示设计 v1.0→v1.1 升版+裁六竖屏回填 | [索引.md](../../docs/索引.md):133（F-D6/F-B5 同刀） |
| 8 | CLI-first 名/版断裂 | 改名 [Berry设备平台-CLI-first设计-v1.9.md](../../docs/Berry设备平台-CLI-first设计-v1.9.md)（F-D7） |
| 9 | 信号位表缺 1<<50/51 | [规则/04](../../docs/规则/04-调度与IPC.md):45-46 两行在（F-D8） |
| 10 | nr_total 37→38 | [规则/03](../../docs/规则/03-系统调用.md):20（改引 abi.toml 单源不写死值） |
| 11 | 索引 0x88 | [索引.md](../../docs/索引.md):49 |
| 12 | 命名 38→70 净数 | [命名规范](../../docs/命名与前缀规范-v1.1.md):18/:41（带 harness 树免疫注） |
| 13 | AI 调度传输表述改口 | [AI调度设计](../../docs/AI调度设计-v1.0.md):42（「0x83 直传，原共享环表述随终裁改口」在文） |
| 14 | 调度彻底计划 README 九行 | [research/调度彻底计划/README](../调度彻底计划/README.md):9-21（01-13 全行，G 最大缺口已补） |
| 15 | 厂商 README 44-46 三行 | 同形补齐（G §四） |
| 16 | 显示 README 06/08 两行 | 同形补齐（G §四） |
| 17 | FACES 冻结相一行修 | [appshell/icons.rs](../../user/berry-appshell/src/icons.rs):214-219（app_h==0 分支补 set_faces——随波 3 迁移落位） |
| 18 | 1080P 第一病 A11Y_MAX | [berry-ui lib.rs](../../user/berry-ui/src/lib.rs):249 =40（01bb3e37a 同款注） |
| 19 | 沿革撞号双占 | 两刀让号收口（59d11164c/c9466fc7e，git log 在册） |
| 20 | gpuproto「仅沿革可考」 | [索引.md](../../docs/索引.md):133 行内注记（6e1697170 清偿） |
| 21 | CP-06 首份 A/B 报告 | [13-首份AB报告-loadgen.md](../调度彻底计划/13-首份AB报告-loadgen.md)（band 轴 null 诚实钉为零假设基线，§0-2） |
| 22 | demo-face 门控+accel 键（部分） | [qemu.rs](../../tools/berry/src/qemu.rs):337-346/:350-353（spawn_kernel 路残留见 W1） |
| 23 | smoke-smp 留档（部分） | [smoke-smp.sh](../../tools/smoke-smp.sh):109-118 八相轮转（默认不留 out/logs 残留见 W10） |
| 24 | guifbd/tools-berry 单测接线（部分） | [规则/07](../../docs/规则/07-测试与冒烟.md):41（launcher/libc 残留见 W8） |

### 2.2 归届一：owner 域（须 owner 裁决或动作，10 件）

| # | 件 | 级 | 锚（本轮直读） | 窗/前置 |
|---|---|---|---|---|
| O1 | **berry-power pmfreq 代际断裂对账**——常驻轮询+pmseat 四帽源曾落盘 QEMU 实证后被 08:01 编辑撤为「单读+park 三源」版 | P1 | [main.rs](../../user/bins/berry-power/src/main.rs):33-42（窗 3「首环已闭合/剩余留窗=常驻周期轮询」）+:199 park；pmfreq/pmseat 全仓零命中（本轮复验） | owner 对账：误撤则恢复（日志已证可行）、有意则改「首环已闭合」口径（K §4.2） |
| O2 | **集成指南 §二示例 auth=0 带毒**（P0 级残留——P0×3 全清口径不含此件，[F](F-P1可做面扫描.md) §6-2 点名勿漏）+ §4.2 128→250 数据回填 | P0/P2 | [显示管线调度集成指南](../../docs/显示管线调度集成指南-v1.0.md):76/:90（示例裸调）+:129（仍荐 128） | owner 显示线文档窗；用户指南 :290 版记自记留窗 |
| O3 | 显示服务 intent 0x02 采纳零（指南靶心用户全未接） | P2 | [svcmgr/main.rs](../../user/bins/svcmgr/src/main.rs) intent:0 ×24 处（:1020-:7218，本轮计数） | owner 显示线决策（v2 EEVDF 消费点开窗后收益兑现） |
| O4 | ADR 预留号 #86 三占 | P2 | [索引.md](../../docs/索引.md):36（Rust 化）+:133（显示 v1.1 仍候）+[user/Cargo.toml](../../user/Cargo.toml):129（「ADR 预#86」） | owner 按让先落者律二择一+互指注（G §3.3；L 确认仍开） |
| O5 | 误入库两处清理 | P2 | 根 W3A-\*.md ×5+tools/.berry-verify/ ×73（git ls-files 实数本轮复验） | owner 清（L §六.1） |
| O6 | CP-02/CP-08 计划状态列滞后（X-6 条件 GO 语义/术语半边归属） | P2 | [调度彻底计划](../../docs/调度彻底计划-v1.2.md):94/:100 | owner 拍板；DPGF n=15+3 确认待 QEMU 空窗（F-A8/F-A9） |
| O7 | 计划「berry-power 未建」过时口径（F-D4 残留——rootfs 注释与 main.rs 窗注已改写，仅计划两行未动） | P2 | [调度彻底计划](../../docs/调度彻底计划-v1.2.md):81/:107 | 与 O6 同批一句口径「骨架已建、通道未连」 |
| O8 | README AI-native 措辞 | P3 | [README.md](../../README.md):5/:23 | owner 定替代口径（F-A7） |
| O9 | berry-sign 长效钉板（跨 checkout 混写新证在册） | P2 | 现役位已重建、CI/配方钉板未落（F-C3） | CI 改动窗+justfile 空闲 |
| O10 | ktest 补 EL0 setattr→带实例断言（K P2：[band] 观测行只在内核 spawn 面，EL0 面零断言零观测） | P2 | [ktest.rs](../../kernel/src/ktest.rs) sched_setattr 仅 :6230 负向（本轮 grep） | kernel 域 owner 窗 |

### 2.3 归届二：设计意图（计划内滞后，非漂移不虚报，11 件）

| # | 件 | 锚/窗 |
|---|---|---|
| D1 | **intent→band 行为差异化=0**（本轮快照现态）：assignment 面全通（class_of_task 分派+六点消费），行为面 FairBand 五方法全委托 unit Fair、band 执法四 API 热路径零调用——[class.rs](../../sched/src/class.rs):521-537/:468/:549-551 注释自认 M4 并线；13 号首份 A/B 已将 band 轴 null 诚实钉为零假设基线。**第五轮刀的「直接修」目标即此——本件落笔时 sched/ 三次复查（开刀前/中/终）均零改动，修复未落；落刀后本行随刀翻状态** | CP-12（入场券判定刀前置未动）；调点调度 v1.2 §7 |
| D2 | EAS 生产不可达（placement_hint 生产源恒 0→eas_place 恒 None→place() 恒 K=2） | 激活通道=v3 policy_bind 0x83 写路（K §2.2） |
| D3 | uclamp/EPP「deadline/RT 也查表」承诺悬空 | [调度器设计 v1.2](../../docs/调度器设计-v1.2.md):132；扩 CP-11 刀面或设计改口二择（F-A11） |
| D4 | IntentProfile 运行时加载/更新/回退通道零 | 设计 v1.2:118（承诺-刀单-现状三点断链，F-A12） |
| D5 | obligations 位图+CMP-1..4 挂账 | [调点调度 v1.1](../../docs/调点调度-v1.1.md):601（F-A13） |
| D6 | CP 未开刀谱：CP-03/CP-04 相本体/CP-07/CP-09/CP-13/CP-14/CP-16/CP-18/CP-19；KC-P1..P5 开庭（13 号判 KC 首样本已破零——QEMU 口径，推断归 owner 拍板） | [调度彻底计划](../../docs/调度彻底计划-v1.2.md) §4 刀表 |
| D7 | CP-08 真通道四环全断（开 7 半闭 2：OQ-8 通道形态/内核聚合点/thermal 输入输出/chargerd/JEITA/smoke 四件套/KC·SLEPT） | [C](C-定制化与PM线遗漏审计.md) §3；OQ-8 终裁→真通道立法刀 |
| D8 | 壳半刀 6/7/8 零开工+QEMU β-1 vs Apple VZ 战略承载未正式裁决 | [B](B-显示与桌面线遗漏审计-v1.0.md) §3；显示 v1.1 已重设计三层架构，兑现排刀待 owner |
| D9 | splash 未合（bootanim 作废，六波波 1 guifbd 过渡屏承载，worktree 未回合） | OEM 产线结构性无法消费 splash 维持（C §1.2-#2） |
| D10 | V13 全产线窗：locale 八键/branding 两键投放无读者、--emit-entries 机器出口、scheduler+services.replace 在册未收录 | [C](C-定制化与PM线遗漏审计.md) §4（F-C4/F-C7） |
| D11 | 立法预约窗：G15 深度断言→#84、G9/G11→#77/闸 E、0x85 attest→v3 预留（恒 -ENOTSUP） | [C](C-定制化与PM线遗漏审计.md) §1.1/[abi.toml](../../tools/abi.toml) |

### 2.4 归届三：待窗（有解锁条件；含零阻塞可做面，11 件）

| # | 件 | 锚（本轮直读） | 窗 |
|---|---|---|---|
| W1 | tools/berry 在途四件：窗口 1:1+单测翻新（[window.rs](../../tools/berry/src/window.rs):204-239 因子 0.55/0.9+0.9 钳制在；:1213 单测仍 540×960 旧面）、[tools/berry README](../../tools/berry/README.md):193 desktop 行仍「540×960」vs [profile.rs](../../tools/berry/src/profile.rs):34-38（panel=render=1080×1920）、[main.rs](../../tools/berry/src/main.rs):896 横屏注释、spawn_kernel 仍零 fw_cfg 注键（[qemu.rs](../../tools/berry/src/qemu.rs):161-176；accel 键已入 fwcfg_plan:350 但仅 spawn_image:303 消费——刀 2① 半开） | owner 输入线收笔后复查（F-B2/B4/B6） |
| W2 | 1080P 第二病（ramfb 1080 非 32 对齐扫出断裂）入库态未定谳：guifbd 已有 stride 域信息流+行弹舱启动门（[guest.rs](../../user/bins/guifbd/src/guest.rs):1690-1693），qemu.rs 零对齐面 | 需 `just desktop` 实跑定谳；owner 显示线在途 |
| W3 | 断点 vp 化（[desktop.rs](../../user/bins/berry-launcher/src/desktop.rs):312 仍像素阈；OH_DENSITY 已迁 [appshell::scene](../../user/berry-appshell/src/scene.rs) 单源化半步） | berry-launcher/berry-ui 空闲即可做（F-B3） |
| W4 | bench.py 预检逃生位（零 force-serial，本轮复验） | 无条件可做（F-C6） |
| W5 | util 域 0 硬编码（[util.rs](../../sched/src/util.rs):121 em_max_capacity(0)） | sched 空闲即可做（F-A5）——注意与 D1 同刀排序 |
| W6 | 指南 placement 表述改口（[用户指南](../../docs/策略服务用户指南-v1.0.md):114「暂无消费」vs [设计 v1.2](../../docs/调度器设计-v1.2.md):342-348「第一消费者已落地」） | 无条件可做（F-A2，一行级） |
| W7 | 桌面 P2 族：按压反馈消费面（set_press_map 零调用）/kerning·抗锯齿·CJK/SYSTEM 窗 I1/simbattery 真值 I4/EFI 关机 I3/刀 4·9（density 档位/product.display 键） | 各自在册挂账（B §2/§1.4/§1.9） |
| W8 | 测试执法窗：test-host 主体进 CI、berry-launcher/berry-libc 单测接线（[justfile](../../justfile) 仅 launcher build :249）、VGPU/THERM 标记进门禁（[markers.sh](../../tools/markers.sh) 零命中本轮复验） | CI 窗（D §4/G §5.3） |
| W9 | docs 尾件：[应用API全景](../../docs/应用API全景-v1.0.md):19 九件→十件（gpuproto 行）、路线图头版+旧名两处（[README.md](../../README.md):78/[README.zh-CN.md](../../README.zh-CN.md):47；架构决策.md:137 已清）、[规则/01](../../docs/规则/01-内核编码.md):64 与 [规则/13](../../docs/规则/13-工作流程与协议.md):91「57 条」陈旧计数、SLEEPQ/R3 改指两低位件（F-A4/F-A6）、沿革 300 系空号注记 | 无条件可做（纯登记） |
| W10 | 观测留档：bootparams eco 行全键零留档（K P2——机制面已接但产品键路径无 QEMU 留档）、smoke-smp 默认不留 out/logs（轮转已修） | 随下次 smoke 批顺手 |
| W11 | 本审计系列登记：README 表 **F 行缺**（十件九行）+ M 行待补 | 留主会话随批登记（K/L 判例；本件不动 README） |

## 三、问题 3：「审计完成」判定

### 3.1 判定：Y（有界）

判据三条，逐条可核：

1. **射程闭合**：五大线（调度/显示加速/定制化/桌面 UX/PM）承诺面 100% 对拍——A（CP-01..19 逐刀+差距矩阵+§3.8+API 参考+KC）、B（刀 1-9+六痛点+β-1+集成指南）、C（G16+15 缝+BCTS+CP-08 十三件+28 键）、D（四缝+漂移 15 分级+索引/沿革/测试面/规则）四件互为经纬，无一条承诺项未经审计；K/L 把方法下探到语义级与 owner 合流面。
2. **发现全归届**：§二 32 件未修项（owner 域 10+设计意图 11+待窗 11）全部有三档归属之一，零悬空；24 项已清偿正面在案。
3. **回归绿在册**：cb726accd 全量回归 100%（selftest 8/8+cargo 166 测零败+check-docs 322 篇 0 警，commit 自证）、E 线 16 件实跑、G 七命令实跑、18388fb54/46dd5b59b 各自终门自证、本轮门禁复跑绿（§四）。

### 3.2 终验通过声明

**全面遗漏审计五轮（A-D 四路对拍 → E/F/G 修复验证与扫描 → 落地与终审回归 → K/L 语义级 → M 终验）就其射程——五大线设计承诺 vs 实现的遗漏清册——判定为完成，终验通过。** 遗漏清册以本件 §二为最终版：32 件未修项全部归届，其中 P0 级残留仅 1 件（O2 集成指南示例，归 owner 显示线文档窗）、P1 级 1 件（O1 pmfreq 对账，归 owner）；其余为设计意图滞后（计划在册）与待窗件。第五轮刀的 intent→band 修复（D1）落刀后，调度线「承诺-实现-验证」三层闭合度将进一步前移，但该项在 CP-12 计划内、不构成本判定反例。

### 3.3 边界与重开触发器

通过声明的适用边界（范围声明，非审计债）：

1. 语义级「端到端真的在工作」保证仅覆盖调度五链（K）；B/C 线及射程外域（TCP/波 3/启动产物等）以各自 commit 终门为准。
2. owner 在途面为快照性结论：本判定锚定 HEAD=46dd5b59b+2026-11-16 快照；O1/O2 为通过后最优先两件。
3. 「审计完成」≠「系统完成」：§二 D 档 11 件是计划内工程量（M4 并线/v3 激活/V13/真通道），不随本判定消失。

重开触发器（满足其一即应补扫而非引用本判定）：①大合流笔（>100 文件 merge/重放）落地后跑一轮 G 式新件集成检查+语义抽验；②新 crate 入册时按五面对拍（L §六.6 口径）；③语义级审计扩展到非调度域时。

## 四、本轮验证实录与门禁

1. **通读**：必读三件+A-L 九件审计件全读（1315 行）；交叉以本目录 README 索引行为经。
2. **锚点复验**：40+ 处本轮直读/实跑——§二两表每行的「锚」列即证据本体（grep/直读/git ls-files/计数），修毕与仍开一律以今日树判定；未逐项复验的仅为 F 表低位件（W7/W9 内已标注在册出处）。
3. **sched/ 现态**（第五轮刀修复目标）：开刀前/中/终三次 `git status -- sched/` 零改动，FairBand 五委托原样在树（[class.rs](../../sched/src/class.rs):521-537）——本件按「修复未落」快照记档（D1 行）；主会话落刀后该行随刀翻状态，§三判定不受影响（D1 在 CP-12 计划内）。
4. **门禁实跑（本件写入后收尾）**：`node .agents/tools/check-docs.mjs` → 检查 337 篇/告警 0 条，rc=0；`python3 tools/docs_guard.py` → PASS（0 警），rc=0。未重跑 QEMU boot 相的理由：内核 ELF（~/.cargo-target/aarch64-unknown-none/debug/berry-kernel，2026-11-16 00:15）落后于已提交内核源十件（find -newer 实测：abi_gen/dpg/klog/channel/bootparams/vmar/ktest/main/syscall/suspend）且 owner 脏树在途——此刻跑则结果不可归因；本判定采信的运行态证据=HEAD 46dd5b59b 自证门（SMOKE_PHASES=boot 32/32 新内核）+E 线 16 件实跑在册。
5. **登记建议**：本目录 README 补 M 行并补 F 行（W11）；本件按 K/L 判例不自行改 README。
