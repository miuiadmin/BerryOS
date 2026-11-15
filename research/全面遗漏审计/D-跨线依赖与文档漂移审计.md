# D——跨线依赖与文档漂移审计（全面遗漏审计之 D 线）

> 版本：1.0 ｜ 日期：2026-11-15 ｜ 定位：五大线（调度／显示加速／定制化／桌面UX／PM 省电）审计的 D 线——跨线接缝逐条判定（已通／设计在／未连）、升版文档与代码现实对拍的漂移全景分级（P0 误导／P1 过时／P2 微）、索引与沿革完整性、测试面三分类（CI／仅本地／从未跑）、AGENTS 与规则文档的冻结常量及路由表对齐。方法：每条结论给路径＋行号两源核对；推断标「推断」；全仓只读，本文件为唯一产出件。

## 〇、结论速览

| 审计面 | 总判 |
|--------|------|
| 跨线接缝 ×4 | 定制化→调度**已通**（bootparams 全链实测过）；调度→显示**机制通、显示侧消费未连**（全部显示服务 intent:0）；调度→PM**双侧骨架在、通道未连**（CP-08 未动，berry-power 已拉起但 park 零消费）；定制化→显示**未连**（[platform.display] 仅双侧文档设计，schema 无键） |
| 文档漂移 | P0×3（policy 门控三件——两份指南示例照抄即 -EBADF，SDK wrapper 读面结构性不可用）＋P1×6＋P2×6；另有 CP-01 在册七处漂移清单中 ①② 仍开 |
| 索引／沿革 | berry-gpuproto 是唯一「仅沿革提及」的新 crate；沿革 200 系**二百六十七／二百六十八双占无注记**（防撞提案后新撞号），300 系 19 个空号（部分在途拟占有注、部分无注） |
| 测试面 | CI 覆盖＝脚本门＋3 crate host 测＋boot 相 smoke＋smoke-image＋x86＋bcts 全相；test-host 主体（60+ crate）仅本地；smoke-smp／bench 族／soak／gui8-smoke 仅本地；**berry-launcher／guifbd／berry-libc 的 #[test] 从未跑过**；显示线 VGPU 标记不在任何门禁 |
| 规则文档 | USER_SIGNAL_BITS 登记表落后内核两行（1<<50/51）；规则/03 nr_total 落后 abi.toml；调度域深读路由缺两件新文档；pm-integration 无 berry- 前缀且不在例外白名单 |

## 一、跨线接缝审计（四条缝逐条判定）

### 1.1 调度→显示（frame pacing／调度集成指南）

**判定：机制接缝已通；显示侧消费者未连；指南核心示例带毒（见漂移 P0-2）。**

已通的部分（两源核对）：

- intent 编码冻结单源在 `intface/src/lib.rs:42-52`（INTENT_UNSPECIFIED 0x00 … INTENT_MAINTENANCE 0x05），与《显示管线调度集成指南-v1.0.md》§1.1 六档表逐值一致；三道帽（HOP_MAX=8／HOP_BUDGET=50ms／CHAIN_BUDGET=400ms）与指南 §1.3「链预算 400ms」一致（`intface/src/lib.rs:64-70`）。
- 声明通道机制层已通：内核 `kernel/src/syscall.rs:1571` `apply_sched_attr_to_task` 在 `process_start`/`thread_start` 消费 effective 属性；用户态 spawn 链已激活——`user/berry-exec/src/lib.rs:707-716`「intent≠0 才发 setattr（thread_create 出生授 SET_POLICY）」；manifest [app] intent 键（`user/berry-manifest/src/lib.rs:538`）经 svcmgr 透传（`user/bins/svcmgr/src/main.rs:6265` `intent: man.intent as u8`）——**.bap 应用链是 Intent M4 的首个活消费者**（`user/bins/appdemo/manifest.toml:6` `intent = "utility"`）。沿革二百六十八（docs/阶段沿革.md:6422）记为「intent 链从设计位到链路闭合」。
- 帧率保障键全链已通：`tools/berry_assemble.py:174-178` BOOT_PARAMS_SPEC 两键（sched_slice_hz∈[16,1024]／intent_default）→ `serialize_boot_params` 产 etc/boot-params.toml → `kernel/src/bootparams.rs:237-243` 词表执法 → `arch/aarch64/src/timer.rs:32-38`（编译期缺省 SLICE_HZ=10 与指南 §4.2 一致，override 语义）。研究件 17 号（research/显示与加速体系设计/17-slice调参GUI实验.md §4）实测三档并推荐 desktop/interactive 档 250——已回填《产品配置层与槽位合同设计-v1.1.md:220》推荐面追记。
- EAS placement 现状与指南 §三自述一致：「只存不路由」——`sched/src/policy.rs:428-429` apply 在 SMP 参与门开时写 `Task::placement_hint`，读出口 `placement_hint(key)`（policy.rs:466），唯一机器消费面是 ktest（`kernel/src/ktest.rs:3418-3420`）。

未连的部分：

- **显示管线自身零声明**：guifbd／gui-wmd／gui-inputd／berry-launcher／berry-power／berry-thermal 的 SpawnSpec 全部 `intent: 0` 硬编码（`user/bins/svcmgr/src/main.rs:1138、1164、7170` 等）；svcmgr 源码注记明说「内核侧 berry_intent 自申报 = 另刀」（svcmgr/src/main.rs:1425-1427，ADR #42／段① §9-6）。即指南 §1.2「显示管线任务应标 0x02」的建议无任何一个显示服务执行——intent v1 可达形态只剩消息借入（channel options=0）。
- **policy 注入无生产消费者**：0x83 表通道在席，但全仓无任何常驻注入者（advisor.py 仅 --selftest）；且指南 §二的注入示例经 2026-11-11 arch-r2 门控后照抄即拒（详见 2.1 P0-1/P0-2；示例两处＝:77 STATUS 对账与 :90 COMMIT）。
- **vgpu 显示加速链零门禁**：guifbd 已有 `Backend::Vgpu`（`user/bins/guifbd/src/lib.rs:56-61`，HANDOFF_VGPU=b'w'）与 berry-gpuproto 协议面，但 justfile／tools/berry／smoke 全链无 `-device virtio-gpu-device` 挂接（grep 全仓仅 justfile:764-767 测试行），设计验收线「双 boot smoke TCG+HVF `VGPU ok`×2」（docs/显示与加速体系设计-v1.0.md:93 刀 5 行）未兑现——VGPU 标记只在研究 spike 镜像里出现过（research/19 号 E 轮 `VGPU2 rect` 实录）。

### 1.2 调度→PM（util 通道／EAS 接线）

**判定：双侧骨架在＋联合纯面在；util 通道未连（设计在、形态待裁）；服务已被拉起但 park 零消费。**

- 产侧：`sched/src/util.rs:94-97` per-CPU EWMA（0..=1024）与读出口 `cpu_util(cpu)` 在席，注释自称「PM 消费面 …util 侧唯一读出口」——但全仓唯一消费者是 ktest（`kernel/src/ktest.rs:7258` 起 UTIL 断言族）；**零用户态导出**（无 tele 行、无 VMO、无 syscall——0x84 telemetry_open 在 abi.toml:693-695 stage=v3 未实现）。
- 消侧：`user/bins/berry-power/src/util.rs` UtilTrack（ingest_window/ingest_sample 两输入口，折叠数学单源 berry-emface::ewma_fold）——host 纯面，窗账载荷形态「CP-08 通道 ABI 的骨架」（util.rs 头注），消费节奏与通道形态显式标注「未定——随 CP-08 裁」。
- 通道：CP-08「util → PM 共享环 ABI 接线」状态「未动」（docs/调度彻底计划-v1.2.md:100）；通道形态「共享环 vs 快照 VMO」仍是开放问题 8（同文 :217，PM-15 §4.1 六面零新 syscall 裁决在册）。注意《调度器设计-v1.2.md:366》与 docs/规则/04-调度与IPC.md:14 均按「共享环形缓冲」措辞立法——措辞超前于未裁事项（P2，见 2.3）。
- 装配面已通（超预期项——计划文档未完全收编）：berry-power(31 测)/berry-thermal(13)/berry-em(8)/berry-slackface(12)/pm-integration(14) 五行已挂 test-host（justfile:780-794）；rootfs 条目在册（tools/rootfs-manifest.toml「bin/berry-power」「bin/berry-thermal」）＋ build-user musl `--features wired`（justfile:284）；svcmgr 预拉 arm 已落（svcmgr/src/main.rs:1108-1173，注释「CP-08 闭合：PM 双服务预拉起——D3 帽 128→160KiB＋pager 钉位 0x1E000→0x28000 扩容刀让出的 .bss 面」，与 kernel/src/main.rs:1689-1691 扩容实态两源互证）；沿革三百三十六/三百三十七（docs/阶段沿革.md:6250/6270）记 QEMU「PM 三证齐」。pm-integration 三场景（热降频全链／slack+EM 联合／充电票+关机边界）验证跨 crate「上游出→下游收」（user/pm-integration/src/lib.rs 头注）。
- EAS 接线：EM 形状单源 berry-emface（sched/src/em.rs re-export 消费）；能量算法补消费第一块在 user/berry-em（CP-09 前置 host 纯面）；真选核路由与 overutil 回退执法归 CP-09 未动。
- **净结论**：这不是「空转机制」（调度彻底计划裁九的旧判词）——产／消／联合验证三块骨架全在，缺的是唯一一段「内核 util → 用户态」通道与服务活循环（berry-power bin 形态＝起机自检 dry-run 后 park 常驻，berry-power/src/main.rs 窗表）。

### 1.3 定制化→显示（product.toml display 键）

**判定：未连（双侧设计在、schema 无键）。**

- 设计侧两源：docs/显示与加速体系设计-v1.0.md:97 刀 9「product.toml `[platform.display]` 立键（profile 枚举＋override 子表）＋组装投放（etc/display.toml 内核回退源＋镜像 sidecar）＋组装期 lint」；docs/产品配置层与槽位合同设计-v1.1.md:627 §7.3-⑥「v2 设备清单补 display 键后让 host profile 退化为纯怎么看——仍开放」。
- 实现侧：`tools/berry_assemble.py:196` `PLATFORM_KEYS = {"arch","feature_level","build_type","sideload","boot","storage"}`——无 display；BOOT_KEYS={"kernel","params"}（:199）——`[platform.display]` 出现即未知键硬拒（:680 惰例）。内核 bootparams.rs 词表同样只有两键（:237-243）。
- 判语：刀 9 排在显示线刀 5-8 并行窗口（同 :97），未到排期；两文档口径一致无互相矛盾——属「设计在、未连」的干净状态。

### 1.4 定制化→调度（bootparams 消费）

**判定：已通（全链单源对读＋真实使用过）。**

- 工具侧：`tools/berry_assemble.py` 步④-G5——[platform.boot.params] 非空产 `etc/boot-params.toml`（BOOT_PARAMS_ENTRY 单源，:236-244），词表 v1 两键直映 BOOT_PARAMS_SPEC（:168-178），未知键硬拒（Q2 执法，:697-705）。
- 内核侧：`kernel/src/bootparams.rs` cpio 定址查找（NAME 与工具侧单源对读，:52-53）＋有界解析（4KiB/64 行/128B 行帽）＋两键独立应用（slice_hz→`hal::timer::set_slice_hz`，intent_default→`berry_sched::set_intent_default`，:304-318）＋eco 打点 `bootparams slice_hz=… intent=…`＋ktest test_bootparams 纯函数面。
- 值域两源一致：SLICE_HZ_MIN/MAX=16/1024（bootparams.rs:62-63）↔ BOOT_PARAMS_SPEC ("u32",(16,1024))（berry_assemble.py:175）。
- 真实消费实证：17 号实验经 `--emit` 投放 250/1000 两档六靴实测（research/17 号 §1「`--emit` 产件 G5 同形投放」）；镜像级铁证「250 vs 1000 全盘仅差 6 字节＋恰一条 boot-params」（research/18 号独立复核）。缺省镜像零投放＝静默缺省零扰动（两侧合同一致）。

## 二、文档漂移全景（升版文档 vs 代码现实）

对拍范围：五线全部 v1.1+ 升版文档＋两份 v1.0 但高活度的现行指南。分级口径——P0＝照文档操作会失败或结论性错误；P1＝事实位被代码超越、误导读者对现状的判断；P2＝时点数／计数／路由陈旧，不误导决策。计数：**P0×3、P1×6、P2×6**；另《调度彻底计划》CP-01 自记的七处漂移清单中 ①（指南门控）②（placement 表述）经本轮核验仍开。

### 2.1 P0（误导——示例照抄即错）

1. **《策略服务用户指南-v1.0.md》门控表述与内核相反**。:41「v1 无 handle 门控（系统全局单通道，`auth` 预留传 0）」与 §2.1 参数表「auth: Handle, // v1 预留不消费，恒传 0」——内核 `sys_policy_bind` 自 arch-r2（2026-11-11，tools/abi.toml:691-692 enforced_since 注）起三门控（在表/类型/RIGHT_MANAGE）全 op 面统一拒，auth=0 → -EBADF（kernel/src/syscall.rs:2720-2726）；负例消费者 policydemo 自证（user/bins/policydemo/src/main.rs:60-78「auth=0 裸调须恰 -EBADF」）。该漂移即 CP-01①在册项（docs/调度彻底计划-v1.2.md:91），四天未修。
2. **《显示管线调度集成指南-v1.0.md》§二示例出生即带毒**。:90 `berry_policy_bind(0, POLICY_OP_COMMIT, …)` 与 :77 `berry_policy_status(&mut packed, &mut key)`——指南落稿 2026-11-14（:3），晚于门控落地三日；指南 §五.3 的 A/B 四步工作流第一步（STATUS 对账）即断。整份指南唯一可执行代码块全部失效。
3. **`user/berry-sys/src/lib.rs` SDK 读面结构性不可用**。:555-614 `berry_policy_bind`/`berry_policy_status` 注释双写「v1 无 handle 需求／auth 预留不消费」，且 `berry_policy_status` wrapper 内部硬编码 auth=0（:613）——在现行内核下该 SDK 函数对任何调用者恒返回 -EBADF；全仓正消费者为零（仅 policydemo 负断言使用）。文档注释与行为双向漂移。

### 2.2 P1（过时——事实位被超越）

4. **《调度彻底计划-v1.2.md》裁九/CP-10 残留「berry-power 未建」**。:81「当前 berry-power 服务实体未建＝三大接缝中最大的空转机制」、:107 CP-10「未动（berry-power 本体未建，01 号 §2.4）」——berry-power 本体已建（lib 31 测＋bin＋rootfs＋svcmgr 预拉，沿革三百三十六/三百三十七同日落地）；且同文 :316 引用同步段还点名「berry-power／pm-integration／berry-em 代码注释指针」——同文档内自相矛盾。Updated 2026-11-15 的 standard 文档带着当日已被超越的事实位。
5. **berry-power main.rs「窗 1」注记与 rootfs-manifest 注释过时**。`user/bins/berry-power/src/main.rs` 窗 1「svcmgr 预拉 arm 留窗——svcmgr 平像满载…装不进…当前 bin 形态＝…（rootfs 在册未被拉起＝零消费零行为变化）」与 `tools/rootfs-manifest.toml` berry-power 条目注释「消费端留窗：svcmgr 预拉 arm 未挂」——svcmgr arm 已落（svcmgr/src/main.rs:1108「CP-08 闭合」＋D3 帽扩容 kernel/src/main.rs:1689-1691 双源）。「未被拉起」表述引导读者低估现状。
6. **《显示与加速体系设计-v1.0.md》未随 B1 系列回填**。Version 1.0.0／Updated 2026-11-13（:4-6）——刀 3 S1 判 GO（05 号）、B1-a/b/c 落地（berry-gpuproto＋guifbd Vgpu 后端）、60fps 排刀重排（13 号：B1-b 壳半→β-1 D-Bus→v2 per-rect 异步化）、「VNC 30ms 刷新＝33fps 硬顶」「β-1 终裁＝Mac 呈现终态通道」等全部未入正文；:93 刀 5 仍写「berry-virtio-gpu crate」而实际定名 berry-gpuproto（user/berry-gpuproto/src/lib.rs:5「02 号 §3.2 原名 berry-virtio-gpu，本轮定名 berry-gpuproto」）。13 号文末「v1.0 升版落字清单五条」（research/13-GPU加速深化综合定谳.md）在册未执行——该 standard 文档是五线中滞后幅度最大的一份。
7. **《Berry设备平台-CLI-first设计-v1.0.md》文件名与头部版本断裂**。文件名 -v1.0，头部「版本：1.8（v1.8）｜2026-11-15」（:3，v1.1→v1.8 八次原地升版）——违反 docs/索引.md:192-196 维护规则 5「文件名与抬头版本双标；升版即改名并同步本表」；索引行还停在「·1.5」（docs/索引.md:104）。该文件是「文档版本标注形态统一提案-v0.1.md」所记七件内嵌 v 号形的最重案例。
8. **《规则/04-调度与IPC.md》用户信号位登记表落后内核两行**。登记表止于 1<<49（docs/规则/04-调度与IPC.md:25-44），内核 USER_SIGNAL_BITS 已有 1<<50（lat 触发 Event 内部位）与 1<<51（dpg 握手位）（kernel/src/main.rs:3873-3874）。登记表是该域「新增用户位前先查本表碰撞」的单一事实源（:46），表本身缺行即失守。
9. **《调度器API参考-v1.7.md》影抄计数漂移**。:3/:624 自称「sched/ 12 文件 148 pub fn（逐文件机提＋人工核对）」——实测 sched/src 13 文件、`pub fn/pub const fn/pub unsafe fn` 合计 145（sched.rs 表称 80、实 76）。「影抄」文档的机械计数声明与源不对拍。

### 2.3 P2（微——时点数/计数/路由陈旧）

10. 《规则/03-系统调用.md》:20「`nr_total` 37 在册」——abi.toml 实为 nr_total=38（0x88 power_off 桌面UX刀D 入册，tools/abi.toml:33-38；kernel/src/syscall.rs:460-468 已实现）。
11. docs/索引.md:49 系统调用 ABI 行「41 在册／40 实现…0x86/0x87 信号面已落」——未含 0x88。
12. 《命名与前缀规范-v1.1.md》「全仓 38 个 berry- 前缀 crate」（存量零迁移段）——现全仓 Cargo 包名 berry-* 共 65 个（实测计数；含独立小工作区）。
13. 《规则/04-调度与IPC.md:14》域铁律「内核↔PM 走共享环形缓冲」——预决了未裁的通道形态（调度彻底计划开放问题 8：共享环 vs 快照 VMO）；《调度器设计-v1.2.md:366》同措辞但补了「通道形态 CP-08 待裁」限定，规则文件未带限定。
14. .agents/plans/2026-11-15-体验大改造六波实施方案.md——Status: active 无波次勾选；现状底册「berry-ui 34 件 ViewKind 仅 9 件真渲染」已被 41/41 全通电超越（user/berry-ui/src/desc.rs:170 ViewKind 41 变体实测；沿革一百一十二轮），波 1 bootanim 路线作废（沿革三百四十 docs/阶段沿革.md:6192）未回写方案。
15. 《索引.md》维护待办「坏引用 1 未修」等时点信息与正文分类计数（123 份）大体同步——实测 docs/*.md 123 份对齐；此条为对照项（无新漂移）。

另记（超预期侧，非漂移）：调度线三份新研究/计划件（调度深度调研 01-08 号、调度彻底计划 01-12 号复核、PM-00/16/17 v1.1）对代码现状的收编明显好于 docs 正文档——CP-04 shadow 已落、CP-05/CP-06 已落码等状态标注与 git 锚均两源可核（调度彻底计划 :95-97）。

## 三、索引／沿革完整性

### 3.1 新文件入索引对拍

| 新件 | docs/索引.md | 所属线文档 | 研究目录 README | 判定 |
|------|--------------|-----------|----------------|------|
| research/调度深度调研/（8 件） | :135 证据指针 ✓ | 调度彻底计划引用 ✓ | 自带 ✓ | 入索引 |
| user/pm-integration | 无（crate 无索引义务） | 调度彻底计划:316＋沿革 ✓ | — | 可接受（事实清单＝justfile:790-794＋独立工作区） |
| user/berry-em | 无 | 调度彻底计划＋沿革 ✓ | — | 可接受 |
| user/berry-slackface | :50（调度器设计行注记）✓ | 调度器设计 §3.8 ✓ | — | 入索引 |
| user/bins/berry-verify | 无 | 设备侧验签 doc:132 ✓＋rootfs-manifest ✓＋test-host:1032 ✓ | — | 入索引 |
| berry-sign（berry-crypto bin） | :87/:132 行提及 ✓ | 应用开发指南＋justfile sign-user:366-394 ✓ | — | 入索引 |
| **user/berry-gpuproto** | **无** | **显示设计 doc 无（刀 5 仍写旧名 berry-virtio-gpu）** | — | **未入索引——显示线首件 crate 的文档面最薄弱，仅沿革二百七十* 提及** |

结论：七个新件中六个有至少两个文档锚；berry-gpuproto 是唯一「仅沿革可考」的——叠加 P1-6（显示 standard 未回填），显示线的索引链完整度五线最低。

### 3.2 沿革节点连续性（全形态机扫）

扫描口径：`^## 日期（中文数）` 标题，中文数含「零」形解析。结果：

- **200 系（200-299）：号序连续无缺口，但二百六十七、二百六十八各双占**——2026-11-07 S6 线（docs/阶段沿革.md:3918/:3933）与 2026-11-15 调度线重放批（:6415/:6422）同号并存，后笔无任何让号注记（对照：265/266 已按 W2-Q71 让号改三百六十三/三百六十四并带原号注记，:6377/:6383/:6393）。这是《沿革编号防撞门禁提案-v0.1.md》（2026-11-09 待裁）目标问题的**提案后新撞号**——门禁未执法的活实证。
- **300 系（300-367）：19 个空号**——339、343-352、355-356、358-362。其中 348-352 有在途拟占注记（三百五十三节 :6235「reviewer 让号令：在途拟占 348-352 之后顺延」）；339/343-347/350-351/355-356/358-362 无注记（推断：多 lane 重写期被剥笔/让号后未回填——353/354 节的「第 5/6 次重写」「被剥笔重放」语境支持此推断，两源：:6232-:6245）。
- **物理序与号序交错**：三百五十三/三百五十四物理位置在三百三十六之前（:6232 < :6250）——重放批追记形态，非错误但加剧机器消歧难度。
- 「N轮」系（一百零九轮等）与日期数系双轨并存；提案 v0.1 的 C 组 9 组机器无法消歧清单未更新至 267/268 新案。

## 四、测试面审计（CI／仅本地／从未跑）

### 4.1 CI 在跑（.github/workflows/ci.yml 重道＋ci-light.yml 轻道）

- 脚本门（fail-fast）：docs_guard／check-docs --strict／writable-globals --check／全类别原语 grep／ver-check.sh／cargo-deny／unsafe_audit --check（双 workflow）／shvarcheck（ci-light）。
- cargo test：**仅 3 crate 直挂**——berry-ksproto（ci.yml:152）、berry-mmap（:163）、berry-signal（:172），加 sdk-publish-guard 的 cargo package/--list（构建面非测试面）。
- QEMU 门：smoke（SMOKE_PHASES=boot 32 标记，SMOKE_DEADLINE_BOOT=360，ci.yml:311）＋ smoke-image（img-boot gate，:359）＋ x86 构建＋smoke-x86（7 标记）＋ **bcts job（八跑全相＋88 标记对账＋abi-check，:418）**——全相经 bcts 在 CI 有覆盖，smoke job 本体只跑 boot 相（分层 CI 2026-11-09 定案注释）。
- 构建/体积：just build／build-release／size／x86 release 构建。

### 4.2 仅本地（just 配方存在但 CI 零调用）

- **test-host 主体**（justfile:592-1090）：约 70 行 cargo test 覆盖 60+ crate（规则/07-测试与冒烟.md:41 自记口径 1732 测）＋ 30+ python 工具 selftest＋mkfat fixture 对拍——CI 只覆盖其中 3 crate，其余全部本地执法（对抗审查曾在 ksproto 上实抓「20 测零自动执法静默腐烂」，ci.yml:144-146 注——该判例只修了三件，其余未跟进）。
- smoke-smp（SMP 三相 LB/PAR）——CP-07「`-smp 2` 验收位进 CI 议程」在册未落（调度彻底计划 :101）。
- bench／bench-rt／bench-icount（基线对拍与 icount L2 门）／soak／soak-smp（长跑）／test-oneshot（rustc 直编 oneshot_law_test，justfile:1090-1092）。
- gui8-smoke／gui8-smoke-normal／berry-vnc-smoke／berry-vnc-tls-smoke／berry-vnc-lan-smoke（GUI/VNC 验收）。
- tools/berry 设备壳 `cargo test`（55 测，tools/berry/README.md:243 手动咒语；无 just/CI 接线）。

### 4.3 从未跑过（有 #[test] 但全仓零调用路径）

| 测试面 | 位置 | 证据 |
|--------|------|------|
| berry-launcher 单测（frameloop 真值表／layout_io） | user/bins/berry-launcher/src/frameloop.rs:904+、layout_io.rs:458+ | justfile 仅 build（:246），无 cargo test 行；grep 全仓零调用 |
| guifbd 单测（backend 判定） | user/bins/guifbd/src/lib.rs:320-327 | 同上（justfile:225 仅 build） |
| berry-libc errno 钉板 5 测 | user/berry-libc/src/lib.rs（errno_base_1_34 等） | workspace member 但 test-host 无 `-p berry-libc` 行（user/Cargo.toml members 列表 vs justfile:592-1090 逐行对拍） |

（对照面：bins 家族里 berry-verify/berry-power/berry-thermal/gui-inputd 均已接线 test-host——缺的三件恰好全是 GUI/musl 面与 libc shim。）

### 4.4 门禁标记面附记

- 活树标记单源 tools/markers.sh：裸相 32＋rootfs 56＋RX 2＝90（bcts@0.toml 冻结 88，freeze-thaw 语义「多打活树新标记不算 fail」容差在案，tools/bcts-run.sh 头注）。
- **VGPU 族标记（`VGPU2 reap/rect`）不在任何 smoke/CI 门**（markers.sh/smoke-image.sh ECO_MARKERS 41 枚逐查无）——显示线 B1 后端处于「代码在、host 测在、门禁零」状态；设计刀 5 验收线（双 boot smoke）未兑现（见 1.1）。
- gui8-smoke 计划扩门 SYSBAR/BACK 新标记（六波方案波 3 验证面）未落——波 3 未落的连带。

## 五、AGENTS／规则文档对齐

- **AGENTS.md**：Curia 上游模板 1:1（2026-11-12 零回写），不含项目冻结常量——无对齐面（设计如此；命令表唯一家＝docs/命令速览.md）。
- **冻结常量对齐**：intent 六档与三帽（规则/04:17 vs intface:42-70）✓；SLICE_HZ 值域 16..1024（规则域与 bootparams.rs/berry_assemble.py 三源）✓；INTENT_CHAIN_BUDGET 400ms ✓；**USER_SIGNAL_BITS 登记表缺 1<<50/51**（P1-8）；**nr_total 37 vs 38**（P2-10）。
- **路由表对齐**：docs/规则/04-调度与IPC.md「深读」节收录调度域文档 4 件（调度器设计/调点 1.1/AI 调度/SMP 设计），**缺《调度器API参考-v1.7》《策略服务用户指南》《显示管线调度集成指南》《调度彻底计划-v1.2》四件现行文档的路由行**——调度域文档族六件收四件，其中两件（API 参考/彻底计划）还是 Type: standard；规则/05-服务监督未收 berry-power/berry-thermal（新双服务）路由。判 P2（路由表滞后于文档族增长）。
- **命名规范 vs 新 crate**：berry-gpuproto/berry-em/berry-slackface/berry-verify/berry-sign 均合规；**pm-integration 无 berry- 前缀且不在《命名与前缀规范-v1.1.md》例外白名单**（白名单 8 条逐查无 host-harness 豁免类）——同类 host 纯面件 berry-em/berry-slackface 均带前缀，pm-integration 是孤例（推断：命名时点早于「user 族同批同值」约定成文，未回补白名单或改名）。
- **雷册 §C 标记台账**与 markers.sh 同步 ✓（CLI ok/TERM ok 两新标记已带 canon 注）。

## 六、修复建议（按优先级，供 owner 排刀）

1. **P0 三件一批**（同一根因 arch-r2 门控）：《策略服务用户指南》:41＋§2.1 改口径并补 auth 用例；《显示管线调度集成指南》§二/§五.3 示例改传合法 auth handle 或注明「需 MANAGE 权 Process handle」；berry-sys `berry_policy_status` wrapper 扩 auth 参（或改默认 self handle）＋改注释——三处任一不改，A/B 工作流文档面整体不可执行。
2. **CP-01 漂移清单收口**（调度彻底计划 :91 在册）：①②随上条同批；③-⑦按清单逐项。
3. **berry-power 现状三处回填**：调度彻底计划裁九/CP-10 措辞、berry-power main.rs 窗 1 注记、rootfs-manifest 注释——一句「骨架已建、通道未连」统一三处口径。
4. **显示 standard 升版**：按 13 号「v1.0 升版落字清单五条」执行 v1.1（刀 5 名称改 berry-gpuproto、刀 3 判 GO 回填、60fps 排刀重排、33fps 硬顶、风险翻状态）。
5. **规则文档三小刀**：USER_SIGNAL_BITS 表补 1<<50/51 两行；规则/03 nr_total 37→38（或改引 abi.toml 单源不写死值）；规则/04 深读节补四件路由行。
6. **沿革 267/268 让号收口**：后笔（11-15 两节）改号并带原号注记（W2-Q71 判例形），或落防撞门禁提案 G11 后首跑收编。
7. **测试面补线**：test-host 主体进 CI（或至少新增 crate 时同步 fail-fast 位——ksproto 判例流程化）；berry-launcher/guifbd/berry-libc 三件单测接线；VGPU 标记随 virtio-gpu 设备挂接进 smoke-image（显示线验收线兑现）。
8. **命名**：pm-integration 入例外白名单（host harness 类）或更名 berry-pmintegration——二择一消孤例。

## 来源与复核说明

- 代码事实全部取自当前工作树只读 grep/读码（kernel/、sched/、user/、tools/、justfile、.github/workflows/）；行号为 2026-11-15 工作树直核，随源漂移不回改（仓内惯例，调度器设计 v1.2:436）。
- 文档事实取自 docs/ 与 research/ 现行文件；时点性表述（「未建」「未挂」）均以两源（文档行＋代码行）对拍后判级。
- 沿革编号连续性为 python 全形态机扫结果（含「零」形中文数解析），与《沿革编号防撞门禁提案-v0.1.md》§一取证表互证。
- 测试面三分类以 ci.yml/ci-light.yml 全部 `just`/`cargo`/`python3` 调用清单 vs justfile 全配方清单 vs 全仓 `#[test]` 位置三者交叉对拍。
- 本审计未运行任何 QEMU/cargo 测试（全仓只读纪律）；「从未跑过」判定基于调用路径不存在，非基于运行记录——若 berry-launcher 等单测曾以手工 `cargo test --manifest-path` 跑过，仍属「零自动执法」类。
