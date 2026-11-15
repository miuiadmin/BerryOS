# C 线遗漏审计：定制化 + PM——承诺 vs 实现逐项对拍

> 版本：v1.0（2026-11-16；文件名按任务书指派不带版本号，版本记于本行）｜性质：全面遗漏审计 C 线——定制化线（G 系 16 键 + BCTS 双层 + product.toml 28 键）×PM 线（CP-08 十三件窗）的「设计文档/计划文档/研究底稿承诺 ↔ 代码实现态」逐项对拍；只查不改，全仓只读
> 锚（必读件全读）：[38 号 OEM 演练实弹报告](../厂商定制化深研/38-OEM演练实弹报告.md)（G1-G16 立表 §4）·[43 号 G 系终账](../厂商定制化深研/43-实施轮十七复核与G系终账.md)（§4 16/16）·[44 号终极演练](../厂商定制化深研/44-终极演练-全能力综合.md)（15 条缝隙 §4）·[45 号复核](../厂商定制化深研/45-终极演练复核.md)·[46 号缝隙清偿](../厂商定制化深研/46-缝隙清偿轮-P0修窗建议案与P1三件落修.md)·[产品配置层与槽位合同设计 v1.1](../../docs/产品配置层与槽位合同设计-v1.1.md)·[BCTS 设计 v1.1](../../docs/BCTS兼容性测试套件设计-v1.1.md)·[调度彻底计划 03 号](../调度彻底计划/03-彻底计划骨架-刀序里程碑验证体系-v1.0.md)（CP-08 行）·[04 号 PM 骨架复核](../调度彻底计划/04-PM双服务骨架复核.md)（十三件窗 §6）·[05/10/11/12 号](../调度彻底计划/)（窗表更新与闭合复核）·[05-功耗-性能权衡](../调度深度调研/05-功耗-性能权衡.md)（PM 面 2026-11-16 最新判定）
> 采集窗：2026-11-16；方法 = 文档承诺面全量提取 → 代码逐项 grep/直读核对（本稿所有「零命中」均本轮独立复验）→ 两源交叉（文档谱系 × 现役源码）；行号 = 本轮直读值（开刀时现场重数纪律沿用）
> 边界：零 git 操作（禁 git——并行会话在途件无从对拍 HEAD，时间线归因处标「推断」）；只写本件；QEMU 零起（纯静态审计）；本目录为他线并行审计共用，只精确落本件

---

## §0 总判（一屏版）

| # | 审计项 | 判 | 一句话 |
|---|---|---|---|
| 1 | G 系 16 键终态 | ✅ 16/16 维持 | 43 号终账后零回退——本轮逐键代码抽查全在位（§1.1）；G15 维持半闭（深度断言归 #84） |
| 2 | 44 号 15 条缝隙 | **P0×2 = 1 修 1 变轨；P1×7 = 3 修 4 未动；P2×6 = 1 已在文 5 在册** | P0-1（INTENT 修窗）已落且双源复验；P0-2（splash）修窗路线已变轨（bootanim 作废、六波方案波1 guifbd 过渡屏承载，未在 HEAD）；P1×7 中 46 号清偿 3、#6/#7/#8/#9 未动（§1.2） |
| 3 | BCTS 深度 | ⚠️ 双 Tier 载体全在役，**冻结件代次滞后** | Tier 1 动态回放/镜像腿/--manifest-name 全在役；Tier 2 = 回显+静态对账（动态 deferred）；冻结件 bcts@0/bcts-t2@0 仍 pr=0 而平台 level=1——**镜像腿回放态（BCTS_FROZEN+--image 组合）今日结构性红**（§2） |
| 4 | CP-08 十三件窗 | **闭 4.5 / 开 8.5** | 装配半边与预拉段全实（常驻化/test-host/双 rootfs 条目）；真通道全留窗——util 通道形态未裁、内核聚合点零代码、thermal 输入输出三面留窗、chargerd 无代码、THERM 遥测臂/KC·SLEPT 闭环未建（§3） |
| 5 | product.toml 28 键 | 消费面三档分化 | 强消费 9 键（投放+运行时读者齐）／校验或计划面半消费 13 键／纯设计在册 2 键（scheduler、services.replace 未收录）+ 投放无读者 2 组（branding 两键、locale 八键）；另有工具先行 3 键（source/override/trust）不进 28 计数但消费面最硬（§4） |
| 6 | 审计新发现 | 3 件 | ①冻结件 pr 代次断档（镜像腿冻结回放不可达）；②44 号 P2-12 的「示例没摊开四键」当前不成立（banner 已摊开）；③P0-2 splash 修窗表述过时（§5） |

---

## §1 任务 1+5：G 系终态与终极演练遗留

### 1.1 G 系 16 键逐键复核（43 号终账 × 本轮代码抽查）

43 号 §4（[:88-107](../厂商定制化深研/43-实施轮十七复核与G系终账.md)）裁定 16/16 收口；44 号 §4 尾「正面账」在最高配置下全部行为复现。本轮逐键静态抽查，无一回退：

| # | 38 号级 | 43 号终态 | 本轮代码锚（直读） | 判 |
|---|---|---|---|---|
| G1 | P0 | 闭（轮十四） | `tools/berry_assemble.py:186`（APPS_KEYS 含 override）+ :95-99 头注「步④-G1/G2 键位」段 | 维持 |
| G2 | P0 | 闭（轮十四） | 同上 :210（TRUST_KEYS）/:213（TRUST_PK_ENTRY = "etc/trust-pk"）；消费面 svcmgr 预拉/装载守卫在（`user/bins/svcmgr/src/main.rs` cpio_find 面） | 维持 |
| G3 | P0 | 闭（轮十四） | `tools/bcts-run.sh:120`（--image 参数位）+ :1085-1114（镜像腿判定面） | 维持 |
| G4 | P1 | 闭（轮十四顺带） | `tools/berry_assemble.py:985-999`（do_check lite 分支） | 维持 |
| G5 | P1 | 闭（轮十五~十七） | :1037-1066（do_emit 产件）+ 消费端 `kernel/src/bootparams.rs`（42 号落地件在席） | 维持 |
| G6 | P1 | 闭（轮十四顺带） | :206（LOCALE_KEYS 八键）+:964-970（计划面 locale 条目行） | 维持 |
| G7 | P1 | 闭（轮十四顺带） | :945-953（kernel 锚现算对账/件缺注记两态）+:981（measure_kernel_anchor 调用） | 维持 |
| G8 | P1 | 闭（轮十五） | `tools/bcts-run.sh:124-128`（--manifest-name 参数位）+:168-182（面约束+纯文件名守卫）——G8×G3 联动刀（46 号）实证在 | 维持 |
| G9 | P1 | 闭（轮十六） | `user/berry-crypto/src/admit.rs:272`（KeySwap 词）+:1002（word 断言）；真签机构面随 #77 在册 | 维持 |
| G10 | P1 | 闭（轮十五） | `user/berry-crypto/src/main.rs:930-953`（admit_help_text：policy/ledger 两件示例全摊开） | 维持 |
| G11 | P1 | 闭（轮十五） | `tools/mkimage.py` G11 v1 裁定注（「内核签名归属 = 组镜方……烧录锚随闸 E/Q1」，本轮直读 :119-127 段） | 维持（正式立法随 #77/闸 E 在册） |
| G12 | P1 | 闭（轮十五） | `docs/命令速览.md:16-22`（AAVMF 拷 /tmp + CARGO_TARGET_DIR 隔离两条记档） | 维持 |
| G13 | P2 | 闭（轮十七） | 设计 v1.1 轻追记五（§4.3 节内）；标准 §1 交付面补条仍「归属注在册」（43 号 §3.1 口径） | 维持 |
| G14 | P2 | 闭（轮十七） | `docs/规则/07-测试与冒烟.md`「APP vs APPUI 计数语义」节（43 号 §3.2 三锚重锚全准） | 维持 |
| G15 | P2 | 半闭维持 | 镜像腿断言面 = EFI 十段+接续+bare 相+banner 形态（`tools/bcts-run.sh:709`/:1092/:1114）——sig/VBS/ECO/APP 生态相深度断言仍归 #84 立法窗 | 维持（深度归 #84） |
| G16 | P2 | 闭（轮十七） | `tools/gensourceindex.py:681`（load_preset_entries→load_product）+:670（首产豁免注，46 号清偿后形态） | 维持 |

**判词：G 系 16/16 无回退、无虚账。**残余三笔全为在册立法预约（G8/G15→#84、G9/G11→#77/闸 E、G13→版本管理标准交付面）+V13 全产线（G5 余投放件），与 43 号 §4 账面逐字一致。

### 1.2 44 号 15 条缝隙逐条终态（P0×2 / P1×7 / P2×6）

| # | 级 | 缝隙（44 号 §4） | 本轮终态 | 证据 |
|---|---|---|---|---|
| 1 | P0 | slice 推荐值 × INTENT × BCTS 判定单互斥 | **已修（双源复验）** | `kernel/src/main.rs:3609-3612`（it_probe doc 注「睡 max(8 slice, 400ms) 墙钟……46 号案 a 修窗」）+:3633（sleep 断言）；[10 号 §1](../调度彻底计划/10-大窗轮复核.md)（三档 INTENT ok 独立重靴 + BCTS 250Hz 判定单 PASS 32/32 + 10Hz 数学恒等证） |
| 2 | P0 | boot splash 未合主干 | **未合（修窗变轨）** | `kernel/src/` 零 splash 文件 + 全仓零 berry-bootanim 目录（本轮 find 复验）；**变轨**：owner 2026-11-15 裁 bootanim crate 路线作废（`.agents/notes/implemented/2026-11-15-波1设计定稿与bootanim路线作废.md`§决定）——splash 改由六波方案波1 的 guifbd 过渡屏承担（`.agents/plans/2026-11-15-体验大改造六波实施方案.md:7`），实施归波1 worktree 未在主干（guifbd main.rs 零 SPLASH 消费、tools 零 gensplashlogo，本轮 grep）——OEM 产线仍结构性无法消费 splash |
| 3 | P1 | gensourceindex --preset 首产鸡蛋循环 | **已修（46 号）** | `tools/gensourceindex.py:681`（lite=True 豁免）+:789（首产豁免回显）+ selftest 54 腿（[10 号 §3.1](../调度彻底计划/10-大窗轮复核.md) 独立复跑） |
| 4 | P1 | G8×G3 manifest 名联动 | **已修（46 号）** | `tools/bcts-run.sh:124-128`（参数面）+:168-182（守卫）+ selftest 31/31 + E2E 净目录正负对照（10 号 §3.2） |
| 5 | P1 | berry-sign 构建树二进制陈旧 | **现役位已重建（流程风险在册升级）** | 46 号 §3 重建件；保鲜期风险获「跨 checkout 混写」新证（10 号 §1.5——`~/.cargo-target` 现役位 09:25 件 strings 为修前语义）；长效钉板（CI/配方）归 owner 未落 |
| 6 | P1 | 计划面非机器可读（--emit-entries 机器出口） | **未动** | `tools/berry_assemble.py` 全文零 emit-entries/emit_entries 命中（本轮 grep）；44 号去向「V13 全产线顺手刀」在册 |
| 7 | P1 | 桌面 UX 可视面对 OEM 拓扑不可达（TCG 首帧饿死 + HVF 镜像流 fatal） | **未动（在册）** | HVF fatal = 显示线既有缺口族（[05-S1spike §5.5](../显示与加速体系设计/05-S1spike-三验实录.md)：HVF+kernel 直启+rootfs 相 copy_to_user EL1 D-abort，挂账刀在册；[12 号 :143](../显示与加速体系设计/12-B1性能定标-双后端实测与v2模型.md) B1 镜像 HVF 档 fatal 同族）；「短期文档明示 OEM 可视自验边界」未见落档（本轮 docs grep 零命中） |
| 8 | P1 | bench.py QEMU 场空预检对长会话死等 | **未动** | `tools/bench.py` 零 force-serial/force_serial 命中（本轮 grep）；预检锚 :362-387（45 号直读）原样 |
| 9 | P1 | locale 八键过检零投放 | **未动（边界在册）** | `tools/berry_assemble.py:1047`（do_emit doc 注「locale 等余投放件不在此产（读者未在册——最小刀）」原文在席）；计划面条目行在（:967）但实件投放与读者（设置应用 v2/berry-intl）双缺——G6 收录立法注记 + V13 全产线口径维持 |
| 10 | P2 | mkcpio 条目 CLI 基准二象性（repo-root vs user/） | **未动** | `tools/berry_assemble.py` 计划面零 CWD/self-CWD 注记（本轮 grep）；`tools/mkcpio.py:363` CWD 注仅涉 git 取数 |
| 11 | P2 | trust-pk 锚内容语义属演练推断 | **未动（P3 策略刀在册）** | `tools/berry_assemble.py:208-209`（「bootseed 正式形随 P3 策略刀」注原样） |
| 12 | P2 | admit ledger 示例没摊开 V4 四占位键 | **当前已在文（44 号观察与现状不符）** | `user/berry-crypto/src/main.rs:943`（banner 含 `kind = \"\"  # V4 四占位（kind/issued_by/cert_status/last_revision）v1 恒空` 示例行）——「推断」：禁 git 无从定该行落刀轮次（44 号时点可能确无此行、后被补，或 44 号观察偏严）；终态判已闭 |
| 13 | P2 | paths.sh 默认位内核随 owner 会话漂移 | **未动（在册+新证）** | `tools/paths.sh:12`（动态解析设计原样）；10 号 §1.5 跨 checkout 混写实证 = 该风险升级在册；CI 显式指稳归 owner |
| 14 | P2 | INTENT 断言失败消息 = 行内中文断言非独立诊断行 | **未动** | `kernel/src/main.rs:3670`（「it_probe: 四睡眠者未同时在线（时序未达）」仍为 assert! 行内消息）；46 号 §4.3 列为顺手刀候选 |
| 15 | P2 | FRZ ok 计时方差（窗口边缘摆动） | 记录在案 | 无需动作（44 号/45 号判词一致） |

**P1×7 净账：3 修（#3/#4/#5，全 46 号清偿轮）+ 4 未动（#6/#7/#8/#9）**——四件未动者全部有在册归属（V13 全产线/owner HVF 域/预检逃生位/立法注记），属「滞后收编」非「悬空遗忘」；但 OEM 体验视角的摩擦面（提取器手写正则/可视自验不可达/长会话不可跑 bench/i18n 零运行时着落）原样存在。

---

## §2 任务 2：BCTS 深度对拍

### 2.1 三形态现状（设计 §四 五步算法 × 代码）

| 形态 | 设计承诺 | 实现态 | 证据 |
|---|---|---|---|
| Tier 1 动态回放（标准流） | BCTS_FROZEN 指定冻结件 → 复跑八跑全相对账 ⊇ 88 标记 | **在役** | `tools/bcts-run.sh:143-166`（frozen 解析+前缀识别）+ `justfile:1206`（bcts: build 配方）；[38 号 §3](../厂商定制化深研/38-OEM演练实弹报告.md) 冻结回放 88/88 PASS（设计 v1.1 修订记录的「本机 1800s 未终证」由此闭合） |
| Tier 1 镜像腿（--image） | G3 兑现：定制 FAT 整盘 EFI 流回放 + 版本声称位对账 | **在役（断言面 = 子面）** | `tools/bcts-run.sh:1085-1114`（EFI 十段+接续+bare 相判定）+:747-771（img_pr_check：manifest PR ↔ 回放清单 pr，不等 loud 拒）+[10 号 §1.4](../调度彻底计划/10-大窗轮复核.md)（250Hz 判定单 PASS 独立重跑）；深度断言（sig/VBS/ECO/APP 相）归 #84（G15 半闭维持） |
| --manifest-name 联动 | G8×G3：防互踩名镜像不被误判非平台产链 | **在役** | `tools/bcts-run.sh:124-128`/:168-182/:936-993（消费点）；44 号首跑红 → 46 号清偿 → 10 号 E2E 复验 |
| Tier 2（--tier 2 / 前缀自动识别） | v0 = 清单回显（--dry）+ 合同条款静态对账；动态 wire 回放 deferred（槽位开放 Q8 后） | **v0 形态在役** | `tools/bcts-run.sh:376-444`（parse_t2_frozen：schema 纪律/空集守卫/文件名自证）+:487（[[contract.op]] = 静态对账数据源）；[44 号 §1-⑦](../厂商定制化深研/44-终极演练-全能力综合.md) Tier2 --dry rc=0；动态回放侦察旁注在册（`tools/bcts/frozen/bcts-t2@0-hdi-dynamic-map.md`：hdi 9 用例逐腿盘点，38 号演练 B 产出） |

### 2.2 冻结件 pr=0→pr=1：**未更新，且构成镜像腿冻结回放的结构性断点**

- 事实链：`tools/bcts/frozen/bcts@0.toml:15`（platform_revision = 0）与 `bcts-t2@0.toml:19`（同 0），而 `tools/abi.toml:29`（level = 1）且 mkimage manifest 单源取 level（`tools/berry_assemble.py:940-943` 计划面注「PR ≡ level 三方联动」）——现役平台工件产的镜像 manifest PR 恒为 1。
- 后果（本轮推演，推断标注）：`img_pr_check`（`tools/bcts-run.sh:747-771`）在 frozen+--image 组合（[:79 用法行](tools/bcts-run.sh)「镜像腿回放态：OEM 提交形」）以冻结件 pr 为锚——pr=0 冻结件对 pr=1 镜像必触发「版本声称位对账断裂 loud 拒」（:766-768）。即**认证的 OEM 提交形（冻结件+定制镜像）今日结构性不可跑**；标准流冻结回放（38 号 88/88）与活树镜像腿（10 号 250Hz PASS）各自不受影响，恰因此三条腿各自绿、合在一起红——与 44 号 §2-1 头号缝隙同构的跨腿形态。
- 定性：冻结件「发布后只读、每 minor 一冻」（设计 §2.1/§2.5）纪律下，bcts@0 是 PR≡level 联动生效前的首冻件，不回改是纪律正确；但 **bcts@1（tier1）/bcts-t2@1（tier2）的签发无人认领**——43 号 §5 资产清单、44/45/46 号缝隙清单均未记此断档（三稿验证窗内均未跑 frozen+--image 组合）。判：设计内滞后与遗漏的边界案，归 owner 裁（修窗 = 签发 bcts@1 双件或在 BCTS 设计补「冻结件代次与平台 level 联动」条款）。

---

## §3 任务 3：CP-08 十三件窗逐件终态

基线 = [04 号 §6](../调度彻底计划/04-PM双服务骨架复核.md)（:121-135 十三件）→[05 号 §6](../调度彻底计划/05-轮二复核.md)（2 闭 11 开）→[10 号 §2](../调度彻底计划/10-大窗轮复核.md)（装配半边+test-host）→[11 号 §1](../调度彻底计划/11-深水区复核.md)（D3 帽扩容+预拉段，CP-08 消费半边）→[05-功耗-性能权衡 §2](../调度深度调研/05-功耗-性能权衡.md)（2026-11-16 九环链判定）。本轮代码复验：

| # | 缺件（04 号） | 终态 | 本轮证据 |
|---|---|---|---|
| 1 | util 通道形态终裁（共享环 vs 快照 VMO） | **开** | berry-power 消费口双形就位（`user/bins/berry-power/src/util.rs` ingest_window/ingest_sample，[05 调研 §2 环 4](../调度深度调研/05-功耗-性能权衡.md) 两源）；OQ-8「CP-08 前裁」仍悬 |
| 2 | 内核聚合点本体 + 写通道 op（ADR #66 ② 内核侧） | **开** | kernel 侧 freq_cap/FreqCapAgg/聚合点本轮 grep 零命中（与 05 调研 §1.5「内核侧零消费」双源一致）；仍是注释锚非代码 |
| 3 | F1 票面单位桥 mA↔pct | **闭** | mA 终态 + 容量桥（[05 号 §6-3](../调度彻底计划/05-轮二复核.md)：charge.rs:100-130）+[09 号 C-3](../调度彻底计划/09-icount与PM联合复核.md)「F1 终裁后无桥」断言 |
| 4 | F2 sim 快照消费面切分 | **裁决面闭（执法面开）** | 三方注释对齐在（`user/bins/simbattery/src/main.rs:55-58`「非本服务设备走各自服务：拒 BAD」本轮直读）；[05 调研 §1.4](../调度深度调研/05-功耗-性能权衡.md) 仍表述「仍待裁」——口径差 = 通道拓扑分端执法（并入 #7/#8 装配件）未落，两说各有事实面，如实并列 |
| 5 | berry-power 常驻化（表位/配方/manifest/shim） | **闭** | `justfile:284`（build-user musl+wired）+ `tools/rootfs-manifest.toml:55` + svcmgr 预拉（`user/bins/svcmgr/src/main.rs:1123-1144`，eco: berry-power up）+ shim（wired 门 berry-libc，[10 号 §2.2](../调度彻底计划/10-大窗轮复核.md)） |
| 6 | berry-thermal 常驻化 | **闭** | `justfile:287` + `tools/rootfs-manifest.toml:56` + svcmgr :1149-（eco: berry-thermal up）；[11 号 §1.3](../调度彻底计划/11-深水区复核.md) rootfs 靴三证 |
| 7 | berry-thermal 输入接线（trips 订阅→Tick+电池温源） | **开** | `user/bins/berry-thermal/src/main.rs:55`（【留窗 2·输入面】注释在席，本轮直读） |
| 8 | berry-thermal 输出接线（帽席/P11 票/thermal_level 两口） | **开** | 同文件 :61-63（【留窗 3/4/5·输出①②③】注释在席） |
| 9 | chargerd 服务本体 | **开** | `user/bins/` 无 chargerd 目录（本轮 ls）；grep 命中仅 berry-power charge.rs（仲裁数学备好）与 pm-integration scenario_c——立法件无代码，[05 调研 §1.4](../调度深度调研/05-功耗-性能权衡.md) 同判 |
| 10 | JEITA 表自报装载面 + 边界迟滞归属 | **开** | JEITA_TABLE 为 lib 常量（`user/bins/berry-thermal/src/lib.rs:113-121` 温区档常量）；「电池驱动上报表换→表装载」面零实现（装配面归充电/电池驱动域） |
| 11 | smoke 断言四件套（穿 trip 帽变/回落解除/迟滞不抖/critical 短链） | **开** | `tools/markers.sh` 全文零 THERM/trip/迟滞标记（本轮 grep；:51 RX_MARKERS 仅 SIMBAT/PWRKEY 两件）——四件套零落地 |
| 12 | CP-08 内核侧两断言（调度路径零频率写点 + PM 崩溃保频） | **开（事实面已绿、断言面零）** | 「零频率写点」作为事实成立（sched/kernel 频率写代码零命中，[05 调研 §2 红线执法位](../调度深度调研/05-功耗-性能权衡.md) 双源）；但 ktest 无两断言（本轮 grep kernel/src/ktest.rs 零「保频/零频率」断言面）——验收线未闭 |
| 13 | test-host 挂行 + THERM 遥测臂 + KC/SLEPT 闭环 | **半闭** | test-host 五挂行闭（`justfile:786` 等，[10 号 §2.1](../调度彻底计划/10-大窗轮复核.md) 五面 31/13/8/12/14 全绿）；THERM 遥测臂开（markers 零位）；KC/SLEPT 开（`kernel/src/syscall.rs:2314`「记账待 KVM spike 回填」注在席） |

**净判：闭 4 + 半闭 2（#4/#13）+ 开 7。**「装配半边 + 预拉段」（两轮刀）全实且经 10/11 号独立复核；**真通道（util 环→聚合点→改频原语→真 OPP 表）四环全断**——与 [05-功耗-性能权衡 §0-1](../调度深度调研/05-功耗-性能权衡.md)「第二杠杆悬空、承重墙还是图纸」总判词逐环吻合，无遗漏承诺也未虚报进度。CP-08 验收线本体（「PM 崩溃内核保频继续跑」等）挂真通道立法，窗仍未关。

---

## §4 任务 4：product.toml 28 键消费面对拍

28 键口径 = schema 1 + platform 9 + product 18（设计 §0 摘要 ：19；「product 18 = 标识 3 + 预装 2 + 品牌 2 + 缺省 2 + 替换 1 + i18n 8」）。消费面分三档：**强消费**（投放+运行时读者齐）／**半消费**（词表执法或计划面透传，运行时读者缺或未实装）／**在册未收录**（berry_assemble 诚实拒）。实装键集单源 = `tools/berry_assemble.py:183-211`（TOP/PLATFORM/PRODUCT/APPS/BRANDING/DEFAULTS/BOOT/STORAGE/LOCALE/TRUST/SOURCE_KEYS + 各 DEFERRED）。

| 键 | 档 | 消费面现状 | 证据锚 |
|---|---|---|---|
| schema | 强 | 版本门（≠后版拒；V12） | berry_assemble.py:111（设计 §1.3 表行）+ load 面 |
| platform.arch | 半 | 词表执法（:675）+ 计划面回显（:900）；「工件选择」v0 单板未实装 | :158 ARCHES；设计 §1.3 消费方列 |
| platform.feature_level | 半 | 词表执法（:663，{rich} 单值）；预装最小集对账（应用规范 §11.3）未实装 | 同上 |
| platform.build_type | 半 | 词表执法；user 型收窄清单成文 = W7/v2（设计 §1.7） | :663 |
| platform.sideload | 半 | 类型执法 + **user 型约束已实装**（:672-673「user 型下必须缺省或 false」拒）——28 键中唯一升档的收窄面；镜像默认面执法归 v2 | :669-673 |
| platform.boot.kernel | **强** | G7 现算对账（件在不等 loud 拒/件缺注记） | :945-953/:981 |
| platform.boot.params.* | **强（全链）** | 词表两键（:178-180 BOOT_PARAMS_SPEC）→ --emit 产件（:1037-1066）→ mkcpio 条目 → 内核 `kernel/src/bootparams.rs` 消费 → eco 行——28 键中唯一「OEM 声明直达内核行为」的闭环 | 44 号 §2-2 station5 串口 bootparams 行运行时活证 |
| platform.storage.image_size_mib / rootfs_split_mib | 半 | 值域执法（:719 前）+ 计划面透传 mkimage --size-mib / mkcpio --split | 设计 §1.3；:118-119 域注释 |
| platform.storage.min_power_atomicity | 半 | 词表 + v1 只 p0 loud 拒（:724「词表内但 v1 只 p0 可组」）；「FS 服务包声明档位执法（步②）」零实装——fsmgr 无声明档对账面 | :719-727 |
| product.name | 半 | 校验 + 镜像命名（计划面 out/<name>.img）；emit 不产镜像（V13） | :126-128（设计 §1.4） |
| product.vendor | 半 | 校验 + priv_grants.issued_by lineage 域对账；账本对账随 #77（「鸡生蛋时序」注在册） | :127（设计 §1.4） |
| product.board | 半 | 板表词表（:584，{qemu-virt}）；DTB 选择 v0 未实装 | :156 BOARDS |
| product.apps.preset | **强** | resolve_entries 终表 → rootfs 条目 → svcmgr 装载（APP ok n= 计数语义 = G14 文档件） | :129（设计 §1.4）；38 号 84/84 |
| product.apps.priv_grants.* | 半（构建期强） | 步②白名单九型拒面执法（app_id ⊆ preset + issued_by 域 + caps ⊆ 闭集）；boot 对账 v2 | 38 号 §2 步②（berry_assemble.py:206-263 段） |
| product.branding.wallpaper / icon_mask | 半（投放无消费） | 包内存在校验（:618-623）；「烘进 launcher/主题资产位」未实装——投放归 V13，运行时零读者 | :132（设计 §1.4） |
| product.defaults.launcher | 半（偏强） | 「launcher 在终表」对账（resolve_entries）+ svcmgr 装配段消费 | :134（设计 §1.4） |
| product.defaults.scheduler.* | **在册未收录** | DEFAULTS_DEFERRED = {scheduler}（:204）——「schema 在册、v0 未收录」诚实拒；策略表文件进 rootfs → 策略服务全链零实装 | :204 |
| product.defaults.locale 八键 | 半（投放无读者） | 收录过检（G6）+ 计划面条目行（:964-970「投放面最小」）；**实件投放（--emit）不产 + 读者（设置应用 v2/berry-intl）未在册**——i18n 声明零运行时着落 | :1047；44 号 §4-9 |
| product.services.replace | **在册未收录** | PRODUCT_DEFERRED = {services}（:189）——槽位合同域（conformance 门随 P5）零实装 | :189 |

**账面：强消费 4 键（schema/boot.kernel/boot.params/preset）+ 半消费 13 + 在册未收录 2（scheduler、services.replace）+ 投放无读者 3（branding 两键、locale 组——locale 组按 28 键口径计 1 键位组）。**另有三个工具先行键（[product.source].instance / [product.apps.override] / [product.trust].pk_anchor——设计 §1.4-补/补二轻追记，不进 28 键计数）消费面最硬：source 透传 mkimage 入 FAT /SOURCE/（gensourceindex 同源消费）、override 终表路径替换直达 rootfs OEM 件（38 号两针脚归零）、trust 产 etc/trust-pk 条目为装载守卫锚（svcmgr p3_sig_guard）——三键均「计划面投放 + 运行时读者」双齐，是 28 键正式收编（#83/升版）时最无争议的一批。

---

## §5 审计新发现与风险（只报）

1. **冻结件代次断档（§2.2，高）**：bcts@0/bcts-t2@0 pr=0 × 平台 level=1 × mkimage 单源取 level——frozen+--image 组合（BCTS 设计 §5.2 认证 OEM 提交形）结构性红；三腿各自绿合体红的形态与 44 号头号缝隙同构。修窗二选一（签发 bcts@1 双件 / 设计补联动条款）归 owner。
2. **44 号 P2-12 与现状不符（§1.2，低）**：admit banner 已含 V4 四占位示例（`user/berry-crypto/src/main.rs:943`）；44 号「示例没摊开」的观察当前不立——后续引用 44 号缝隙清单时此条应翻状态。
3. **P0-2 splash 修窗表述过时（§1.2，中）**：44 号「splash 在 wip/ux-unify 孤儿位待合入」的裁据已被 owner 11-15 裁决改变——bootanim crate 作废、六波方案波1 guifbd 过渡屏承载（worktree 未在 HEAD）；「OEM 产线结构性无法消费 splash」的结论维持，但阻塞点从「分支合入」变为「六波方案波1 交付」。
4. **CP-08 验收线未关账（§3，在册）**：装配/预拉/测试三面全实但四环真通道全断——若引用「CP-08 已闭合」（11 号标题口径）须限定为「消费半边（预拉段）闭合」，util 通道与内核聚合点仍是 05 调研判词的「全链最窄断点」。
5. **PM 遥测面零落地（§3 #11/#13，在册）**：THERM 遥测臂、smoke 四件套、KC/SLEPT 闭环三件全零——PM 线后续任何「行为级验收」都缺观测位，建议随真通道立法刀同批。

## 来源索引

- 承诺面（设计/计划/研究底稿）：`docs/产品配置层与槽位合同设计-v1.1.md`（§0 :19 28 键/§1.3 :109-120/§1.4 :122-139/§1.4-补 :147/§1.4-补二 :157-158/§1.5 :168-179/§1.6-补二 :226-237）；`docs/BCTS兼容性测试套件设计-v1.1.md`（§2.1-2.5 冻结纪律/§3.2 :136-140 Tier 表/§4.1-4.2 冻结件格式与五步/§5.2 :299-310 认证流程）；`research/厂商定制化深研/38 号`（§4 :184-199 G 系立表）`/43 号`（§4 :88-107 终账）`/44 号`（§4 :125-139 十五条）`/45 号`（§4 逐条核）`/46 号`（§0-§4 清偿与建议案）；`research/调度彻底计划/03 号`（:71 CP-08 行）`/04 号`（§6 :121-135）`/05 号`（§6 :89-103）`/10 号`（§0-§3）`/11 号`（§0-§1）；`research/调度深度调研/05-功耗-性能权衡.md`（§0 :17-23/§1.4-1.5/§2 九环表/§7 :215 grep 实证四项）。
- 实现面（本轮直读）：`tools/berry_assemble.py`（:156-213 词表与键集/:672-676 sideload·arch/:719-727 atomicity/:940-972 计划面/:985-1066 check·emit/:1047 locale 注）；`tools/gensourceindex.py`（:670/:681/:789）；`tools/bcts-run.sh`（:120-128/:143-205/:376-444/:487/:579-584/:709/:747-771/:975-993/:1085-1114）；`tools/bcts/frozen/bcts@0.toml`（:15-19）、`bcts-t2@0.toml`（:19-23）、`bcts-t2@0-hdi-dynamic-map.md`（:1-30）；`tools/abi.toml`（:29）；`tools/mkimage.py`（G11 裁定注段）；`tools/markers.sh`（:51）；`justfile`（:284/:287/:786/:1206）；`tools/rootfs-manifest.toml`（:55-56）；`kernel/src/main.rs`（:3609-3633 修窗/:3670 行内断言）；`kernel/src/syscall.rs`（:2314）；`user/berry-crypto/src/admit.rs`（:272/:1002）与 `src/main.rs`（:930-953）；`user/bins/berry-thermal/src/main.rs`（:3/:55-64 留窗）与 `src/lib.rs`（:113-121）；`user/bins/simbattery/src/main.rs`（:55-58）；`user/bins/svcmgr/src/main.rs`（:1108-1149）；`docs/命令速览.md`（:16-22）；`.agents/notes/implemented/2026-11-15-波1设计定稿与bootanim路线作废.md`；`.agents/plans/2026-11-15-体验大改造六波实施方案.md`（:7）。
- 本轮 grep 零命中复验（2026-11-16）：emit-entries（berry_assemble）／force-serial（bench.py）／splash·bootanim（kernel/arch/user/tools）／THERM·trip（markers.sh）／freq_cap·FreqCapAgg·聚合点·set_freq·cpufreq（kernel/sched）／chargerd（user/bins 目录）——各与上列研究件判定双源一致。
