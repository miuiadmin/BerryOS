# app-1 实施设计草案（应用生命周期第二刀：freeze-first + 墓碑协议 v0）

> 版本：**1.2**（v1.2）｜2026-11-08

> **修订记录**：v1.0 立稿同日；v1.1 = 对抗审查全量收编（workflow 64 agents：55 raw → 47 收编/8 refuted）；**v1.2 = 双线调和回灌**（workflow `app1-reconcile-audit`——兄弟线 a-1（提交 ba11596，按 v1.0+H 系审查）与本稿对照；吸收 H 系七条独有发现 + D5/D7/D8 改文配方；**段 b-1/b-2 以本版为唯一基准**；三 MAJOR 分歧与 D 系配方全文 = `research/app-1调和审计-2026-11.md`）。
> **依据**：`应用模型设计-v1.0.md` §4/§4.1 + `应用规范-v1.0.md` §4.1/§4.1-5 + ADR #64（调点 1.1 S3 freeze 已落内核侧；调点调度-v1.1.md §5.5/§5.6/§5.8）+ `research/AM-01-墓碑模式开源对标-v1.0.md`（开源谱系四层+八判据，本刀裁形业界互证基座）+ `.agents/plans/am/design.md` v0.3.0（am↔app-1 梯度互证：段 a=L2 freeze-first、段 b=L3 v0）+ ADR #67（双钟）。
> **时点**：GUI-8 段④ 收官窗后、SDK-2 开刀前——appd 域纯用户态一刀。
> **形态先例**：`app-0实施设计草案-v1.0.md` + T-1 `thr_selftest`（标记/帽/eco 形）+ app_launch/app_serve_spawn（spawn 面）。
> **DEADLINE 核账（2026-11-08）**：rootfs 相 600s 帽 − 在案 EFI 窗 ≈270s − tomb 串行睡预算 ≈23s（8s 窗+10s deadline+5s 帽）≈ 余量 307s，充足。
> **执行结果**：段 a-1/b-1 已落地实证（刀 `ba11596`/`aaef81d`——`FROZEN ok n=1` + `TOMB ok n=1` + `apptomb restored c=3` counter 续接，沿革一百五十八/一百六十二/一百六十五〔原编一五六/一五七/一六〇，2026-11-08 平账顺延〕；冻结谓词双残影内核残修 + 判例 §A144/§A145 三处一动 = `d563485` 完结）。
> **负路径三件 = 人工验收面挂账**（CI 不可达、降级臂在码未实跑；机制位 §4，每件一句触发法）：①pause 超时杀——apptomb 换慢变体（收 PAUSESV1 后 >10s 不回 SAVED）→ 期望 `eco: tomb ptmo` + 监督杀 + 不入墓碑表 + 不重启；②blob 越写——变体向 state VMO 直写 >64KiB → 期望 `vmo_write` 越界返错（64KiB 定尺寸即帽，结构性拒、无静默截断）；③墓碑表满 LRU——变体连灌 8 相异 id 后第 9 个记入 → 期望 epoch 最小槽淘汰回收、新条目在席（`tomb_put` LRU victim 臂）。④restore-fail/⑤二次 spawn 失败降级臂同在码（`tomb rfail`/`tomb respawn`），同挂人工面、随负路径变体刀一并验。

## §0 目标与范围盒子

**一句话**：把「active → freeze → tombstone → not running」的后两跳变成第一片可跑代码——段 a freeze-first（`FROZEN ok n=1`）+ 段 b 墓碑协议 v0（`TOMB ok n=1`）——**全通道确定性状态谓词，不赌竞速**。

**术语注**：本文「墓碑/tombstone」= pause(save)→杀→状态留存→relaunch 读档复活（Windows Phone tombstoning 谱系）；**≠ Android tombstone**（/data/tombstones 崩溃取证转储，死后取证永不复活）——BerryOS 未来崩溃转储特性须另择名（安卓侧冻结致死记录走 ApplicationExitInfo），不复用此词。

**范围盒子（v0 不做，逐条注明去处）**：

| 不做 | 原因 | 去处 |
|---|---|---|
| 真触发策略（失焦/压力） | v0 无 wmd 焦点事件、TCG 无真压力 | 失焦触发挂 GUI 线——**须排 AM-1.5（wake-pending 内核小刀）之后**，GUI 线接触发时回引 §4 thaw-then-pause 不变式；压力触发挂 v2 策略 |
| 跨 boot 复活 | $appdata 树未落（fsmgr 平名+无子目录） | v2；v0 表驻 RAM = reboot 全墓碑 |
| GUI 应用冻结 | freeze obligations 谓词（PAGER_REPLY/IPC_REPLY）未落（调点 v1.1 §5.5/M1.1-3 挂 1.2）；GUI extras 8/8 恰满 | v2 obligations 刀 + extras 槽预算另裁 |
| berry-libc 回调三口（规范 §4.1-5） | v0 先通协议 | app-2（回调阻塞化前须先解 §5 freeze 载荷条件） |
| 冻结的 SMP IPI/obligations | 调点 1.1 明挂 1.2 | 调点 1.2 |
| 多 app 并发墓碑 | v0 单活自验 | v2 真策略刀（表 keying 语义已随 §4 本刀定型；「LRU 淘汰命中刚授出的槽」时序面 = v2 显式首验点——授子后子进程持 dup 句柄，appd close 不影响读档，风险面在 relaunch 失败重试无档） |
| 复活者再入 pause(save) 写权 | v0 复活即证不常驻 | 真策略 v2（重授路径已注 §4：relaunch dup 已含 WRITE，循环天然闭合；env 块双 gift 仅 spawn 期可达，不作运行中重授通道） |

**内核残修注（v1.2）**：段 a-1 实施期（ba11596）附带一处**实证必要的内核谓词残修先行入库**——`freeze_precheck` 双残影修改（睡眠者不查 svc_mask/held_locks 残影位；16 试全拒 pc:sl×15 实证，sleep_ok/rq_ok 拆分修法）。本刀范围盒由「纯用户态一刀」修正为「用户态主体 + 一处已入库内核残修」；**欠账两件随 b-2 提前补**：ktest FREEZE 系双断言（睡在 syscall 内带残影位可冻 / RQ 在席带位仍拒——现仅盖 FROZEN 位排除）+ 判例三处一动（规则/04 瘦行 + 雷册 + 判例索引）。

## §1 现状四层账（2026-11-08 实测锚点，行号随刀漂移以函数名为锚）

| 层 | 状态 | 锚点 |
|---|---|---|
| 协议设计 | ✅ 已冻 | 应用模型 §4.1；规范 §4.1-5 |
| **内核 freeze 原语** | ✅ 已落 | `berry_process_suspend(target, options, *token)`（0x3E，`sys_process_suspend` 真实现役，现锚 syscall.rs:1851——头注 :1652/:2081 为陈旧行号随刀改）：进程全冻 + 线程 token + 嵌套 depth；**解冻 = handle_close(token)**；自冻拒 -EINVAL；Exited→-EBADSTATE；pager/thread_freezes 交叉/depth≥8→-EBUSY。**D6 拒冻门键 = 目标自身表内 SET_POLICY demand 柄**（handle.rs:444-457、syscall.rs:1696-1700）——apptomb 经 app_launch（映像 VMO 柄留 svcmgr 表、env gift 不含映像 VMO）→ 子表 pager_load=0 → 0x3E 不触 D6 门，**段 a 结构性可达**；真正被 D6 拒冻的是持 demand 柄的 svcmgr 自己。ktest `FREEZE-K` 活证（调点 M1.1-3 ✅） |
| appd 消费链 | ❌ 全缺（本刀主体） | svcmgr 无 pause/lifecycle/墓碑面 |
| 机制底座 | ✅ 全在役 | env 块 gift（base[0..4]+extras[4..8] 前缀紧致律，berry-exec lib.rs:860-901）/ ChildHandles{hp,hv,ht,svmo}（hp 恒带 MANAGE\|DUPLICATE——见 §10 R1 钉板）/ EOF+reap 30s 帽 / 监督重启（#28）/ vmo_write 直写无需映射（vblk 数据面现役同构先例，drivers/bins/vblk main.rs:463） |

## §2 两段刀法总览

```
段 a（freeze-first）              段 b（墓碑 v0）
──────────────────────           ─────────────────────────—
apptomb 长活自验 app               同一 apptomb，lifecycle 面
appd: suspend(hp,0)→token          spawn 双 gift: [4]lifecycle端 [5]state VMO
8s 静默窗（实效补充）+suspend 返 0   appd 发 PAUSESV1（起算=发出时刻）
close(token) → RESUME 帧 →        app 写 12B 头+blob → exit(0)
RESUMED 回帧 → FROZEN ok n=1      appd reap + 记表（id 查重/epoch 序）
段 a-1 收口 = terminate(hp,9)      取档即出表 → relaunch dup 授子
                                 app 读档 restored → 二次 reap → 表空 → TOMB ok n=1
```

- **freeze-first 先行的理由**：freeze 级 v0 零应用配合（内核原语直接可用）——先拿「后台不烧 CPU」省电红利。**口径收窄注**：「零应用配合」是 v0 自验机制层口径；终态 L2 冻结按 am 模块设计**冻前发 onBackground（限时落盘窗口，超时不阻塞冻结）**——保存点在进入冻结的转换点（对齐 iOS didEnterBackground/UWP OnSuspending，AM-01 判据）；本 v0 selftest 无此面（冻结对象已过自验、状态在 VMO）。
- **感知契约**（两级形态的用户可感知连续性成文）：freeze 级 = 原地续跑（计数不清零，判据 = RESUMED 帧本身）；墓碑级 = 跨生死读档（restored c=N+1）。freeze 级对 app 零预告通知 = 业界同款（Android freezer 冻结不派发任何生命周期回调）；v2 若需预告可对标 A14 TRIM_MEMORY_UI_HIDDEN 先例——注：A14「入 cached 态 10s 后冻结」与本文 pause deadline 10s 仅量级巧合、语义不同（前者=冻结触发宽限，后者=写档超时），勿互引。

## §3 段 a：freeze-first（`FROZEN ok n=1`）

- **apptomb（新自验 bin）**：headless 长活——启动打 `app apptomb up` → 周期 2s 心跳（libc `nanosleep(2s)` 或 `berry_sys::berry_thread_sleep(clock_read(MONO)+2s)` 绝对时刻式）打 `apptomb beat c=N`（非标记诊断行）→ 稳态循环。**不 ps_reg**（瞬态自验件）。
- **tomb_selftest 落位**：`run_rootfs_child` 尾形态（boot 批后、稳态 loop 前；硬约束仅一条 = app 走查收集已落值——挪 _start 链尾需缓冲管道，非硬约束）；EFI 路同跑（appd 同二进制）。
- **自验序列**（确定性谓词链）：
  1. tomb_selftest 起手查 APPTOMB 在册（**v1.2 基准：打包位 selftest/ 专用位 + cpio 直查**，miss → 静默 return，裸跑零扰动先例；a-1 在途版为 v1.0 白名单路线——D1 调和配方见调和书，**SETTINGS 挤出雷已现实化，b-1 前必修**）→ spawn apptomb（spawn 面 = app_launch 形：bap 守卫 → SpawnSpec extras（`stdin_feed=Some(&[])` 零长直传保 extras 在位断言——H 系 M1）→ ChildHandles::reap 30s 帽收尾）→ 等 `up`（强制状态行，不 sleep 回退；**判据载体 = lifecycle 通道**——console=true 的 stdout 走 console 通道 SEND 别名、svcmgr 只发不读，串口行 appd 侧不可观测 + 共享串口撕行致 grep 失真，H 系 H1 实证）。
  2. `berry_process_suspend(hp, 0, &token)`（hp 直用——R1 已钉板 §10）。**返 -EBUSY → 100ms 退避重试 ≤4s 帽；其余错误不重试、直走失败行 loud**（v1.2 钉死：仅 BUSY 可重试——a-1 在途版任意错 16×50ms 属过渡形态，D4 配方）。-EBUSY 源清单：TCG console 满队瞬态窗 + `-smp 2` 副核 Running 窗 + 睡在 syscall 内任务带残影位窗（**双残影残修后睡眠者可冻——见 §0 残修注；首批实测 susp:pg×1 醒窗在飞撞 pager_load 面**）。
  3. **静默窗**：**drain 在途一拍后开窗**（竞态封口，H 系③），8s 窗内 **lifecycle 通道零收**（实效补充——抓「标记冻结但未停跑」类内核缺陷的能力优于纯正读）；**正谓据门 = 步骤 2 suspend 返 0 本身**（freeze_by_pinned==0 → -EBADSTATE，返 0 ⟹ ≥1 任务已置 HELD_FROZEN；Android freezer 同面验证全走内核状态正读）。
  4. `handle_close(token)` 解冻 → appd 端 A 发 8B `RESUME\0` 帧（应用模型 §4 resume 消息的冻结级复用）→ apptomb poll 主循环 recv 得 RESUME 即打 `apptomb resumed c=K` 串口行并回 `RESUMED c=K` 帧 → appd 等 RESUMED 帧（~5s 帽仅 liveness 兜底，非测量判据）——**消 clock 差值启发式**（TCG oversleep 假触发/阈值漏触发两险同除；a-1 在途版为差值 >3s 自判过渡形态、阈未定标——**D2 调和配方 b-1 前必修**）。
  5. 打 `FROZEN ok n=1`。**段 a-1 收口 = `process_terminate(hp, 9)` + reap**（apptomb 此段无 PAUSESV1 面——段 a-1 独立可验可提交）；段 b-1 落地后切 pause 收束（「共用一次生命周期」为 b-1 后终态描述）。
- **判据诚实注**：TCG 静默窗抖动 → 降级链见 §9 K1（两档均正谓据）。**冻结×channel 交互依调点 v1.1 §5.8（已拍板）**：已入队消息保留、thaw 后照常投递；同步 send 命中冻端 = -EAGAIN 可重试；本 selftest 单写者拓扑不触发该面。**冻结期入站 IPC 语义（v0 事实态）**：`sys_channel_send` 无冻结检查（syscall.rs:2837）——消息静默入队不丢弃、深 8 满即 -EBUSY、无 REASON_FREEZER 式致死、无收讯自动解冻（auto-thaw 仅 token 持有者死亡安全门）；此为临时态：IPC×冻结已裁「唤醒即解冻」走 AM-1.5 wake-pending 内核小刀（app-1 v0 收官后、真触发落地前）——BerryOS 有界队列天然排队，Android 同位置致死坑（AM-01 [A1]）不存在，缺的只是唤醒闭环。**L1 机制注记（H 系⑦）**：冻结中到期 sleep 摘 RQ 挂起、thaw 即续跑、beat 计数不重置——解冻后首拍即恢复心跳的机制根据（也是差值判据误判的成因背景）。

## §4 段 b：墓碑协议 v0（`TOMB ok n=1`）

- **前置不变式（thaw-then-pause）**：**PAUSESV1 仅可发往未冻结进程**——目标 freeze_depth>0 时必先 close(token) 解冻、收 RESUMED（或首心跳）后再发。两级形态下上收墓碑的输入态即冻结态；跳过解冻直接发 = 消息入队无人消费（冻结=全任务停投）→ deadline 必超 → 监督杀+不入表 = **状态静默丢失**（且被 pause-timeout 执法行掩盖）。§3 步 4→5 的序即本不变式实例。
- **双 gift 与装配面**：spawn apptomb 时 extras 装两枚——`[4]`（gift 绝对位）= lifecycle channel 端 B（appd `channel_pair` 持端 A）+ `[5]` = state VMO。**装配面立法：双 gift 仅 tomb_selftest 显式 spawn 通道装配；boot 批 app_launch 与 SPAWN RPC 恒不装**（fs_gifts 固占槽位、带 fs 词 app 结构性冲突；零义务 app 无从收到 pause——v0 免超时/fault 面，拒绝位/版本协商随 v2 真策略刀）。
- **state VMO**：appd 预建 `vmo_create(64KiB, 0)`（eager；创建者自动持 R|W|MAP|DUP——**权利非 create 入参，options 只认 VMO_OPT_DEMAND，且 RIGHT_READ=bit0 恰撞 DEMAND 位，照抄「READ|WRITE 入参」会静默建成 demand VMO**）→ 首轮 `handle_duplicate(svmo, RIGHT_WRITE, &out)` 最小面授子；relaunch 轮 `duplicate(READ|WRITE)`（**放弃只读收紧**——复活者再入 pause(save) 循环需写权，循环天然闭合）。**blob 帽 = VMO 定尺寸即帽（结构性执法）**。
- **pause(save) 消息与 deadline**：appd 端 A 发 8B `PAUSESV1\0`；**deadline 10s、起算 = appd 发出时刻（点对点 clock 读，含通道投递等待）**——应用侧预算按 <10s 自留投递余量（poll 型 ≥1 轮询周期；apptomb 2s 心跳即预留 ≥2s）。超时未退 = 监督杀（fault 槽 + `eco: tomb <id> pause timeout` loud）+ 不入墓碑表 + **不重启**（策略性墓碑超时 ≠ 崩溃监督；代号 +1 重启仅适用崩溃语义——本条为对规范 §4.1-1 的显式 v0 偏差，随段 b-2 回写规范注）。计时钟 = `CLOCK_MONOTONIC`（v1 唯一在册钟；ADR #67 程序时间观——系统挂起不吞 deadline）。业界校准：iOS 进后台回调 5s / UWP Suspending 5s（Deadline 可查）/ WP7 10s 且官方劝「aim for much less」；**v2 真策略上线按 intent 分档复核**（background 档收紧 5s 量级、interactive 留 10s）——v0 TCG 维持 10s。
- **apptomb 契约**：收 pause 后序列化写 state VMO——**blob 头 12B 定长** `{magic u32=BTMB, ver u8=0, hlen u8=12, rsv u16=0, counter u32}`（**ver = blob 格式版本（协议侧），非 app 版本**；v1 CRC32/长度域经 hlen 扩展追加于头后，不塞 rsv）+ payload；console 打 `apptomb saved c=N` → `exit(0)`（EOF 先行）。**零写 = 拒绝保存语义**（对标 UIKit shouldSave 返 false）：无状态应用零写直接退，appd 判据链仅约束有态路径。v0 的头布局为自验件试验格式、非平台 ABI（头部保留区律见 §5）。
- **墓碑表**：svcmgr static 8 槽 `{id[24], svmo: Handle, epoch: u64}`——三字段对账立法原文（id↔app_id、svmo↔blob、epoch↔时间戳）；**epoch = 全局自增记入序号（无墙钟实现，仅 LRU 比大小；不兼作 boot 世代）**。记入条件 = PAUSESV1 流程 reap 真码 0 **且 blob 首 4B 非 BTMB 魔数**（仍为零态 = 拒绝保存 → 不入表、直接 close 回收 64KiB）；记入前按 id 线性查重——同 id 已在表 = 覆写旧槽并 close(旧 svmo)（**upsert，二次墓碑不双槽**）。满时淘汰 epoch 最小槽。
- **relaunch 与生命周期钉序**：`reap0（真码 0）→ 记表占槽 → relaunch（dup 源 = 槽内 svmo；**复用 tomb_selftest 帧内留存的首轮 bap_scan/manifest 解析结果**——走查表只供几何不含 elf_off/stack_pages，表查单独不可成 spawn）→ 收 restored → reap1 → 表空（清槽 + 本侧 close——取档即出表，blob 随复活者末柄回收）`。apptomb 启动 `vmo_read` 首部判读：非 BTMB 且零态 → 全新启动；**ver>0 或 hlen≠12 → 弃档按零态启动 + `eco: tomb <id> badfmt` loud**；BTMB 同 ver → counter+1 恢复打 `app apptomb restored c=N` → ≥1 心跳后自退 0。
- **负路径**（CI 不可达人工面，四件 + 附录）：①pause 超时杀（慢变体注入）→ eco 行 + 不入表；②越写结构性拒（VMO 定尺寸）；③LRU 淘汰（8 槽灌满变体）；④**restore-fail/复活即崩**（坏档或二轮 reap 非零）→ close 槽 svmo 弃档 + 冷启路径 + `eco: tomb <id> restore-fail`；⑤二次 spawn 失败 = 条目保留待重试 + `eco: tomb respawn`。附：relaunch 查无 .bap = 弃条目 + 拒复活（v1 静态 rootfs 不可达；归宿引规范 §7.3 卸载=墓碑清）。**复活失败/退出原因证据账本与墓碑表分家**：失败计数归 AM-3 全态与 SDK-2 v2 回滚链（tries 减一事件），app-1 只留 eco 痕迹不建账本。
- **策略位（机制/策略接缝）**：svcmgr 落单函数 `lc_policy(ev: LcEvent) -> LcAction`——`LcEvent = {Selftest, FocusLost, FocusBack, MemPressure}`（触发源词表）、`LcAction = {None, Freeze, Thaw, Tombstone, Relaunch}`；v0 仅 Selftest 命中（硬编码序列）；suspend/PAUSESV1/记表/relaunch 四动作执行体收独立 fn 由 policy 分发——**v2 演进 = 换 policy 函数体与词表，协议面零改**（intent 单轴同构）。
- **头布局注退役追记（2026-11-10·app-2）**：本节 apptomb 契约条末的「v0 的头布局为自验件试验格式、非平台 ABI」注**随 app-2 退役**——v1 起头布局升**平台 ABI**：hlen=32（32B 头 = 12B 前缀字段布局同位〔magic/ver/hlen/rsv/counter 段不动，ver@4/hlen@5 按版本变值〕+ 追加 payload_len/CRC32/rsv2），读者按 hlen 分派（12=v0 路径/32=v1 路径）、写侧恒写 v1 头——布局与读写兼容规则 = app-2 草案 §3.1/§3.2（§5 头部保留区律的兑现点）。

## §5 应用侧最小面（apptomb + libc 访问器）

- **访问器两枚**（fs_h() 惯例，**命名从代码惯例侧**）：`lifecycle_h()` 读 GIFT_H[0]、`state_h()` 读 GIFT_H[1]——**libc 口径 extras[0..3] = gift 绝对位 [4..7]**（wm_cli_h/sim_ack_h 与 fs_h/blk_h 同槽不同名先例；GIFT_H[4]/[5] 是 netd/input 族槽，**勿按 §4 的绝对位字面落 GIFT_H 下标**）。**0 值语义 = 协议破缺 → 单行诊断（`apptomb dead`）后 park**——失败归因双通道：app 侧诊断行 + svcmgr eco 降级行（v1.2 采 D9 前案）。
- **apptomb 形态（v1.2 对齐 a-1 实形）**：hello 模型 none-target ELF——单依赖 berry-sys、自解析 env 块（不经 libc）、`berry_thread_sleep` 绝对时刻式心跳；**libc 双 crate 形（nanosleep/exit/EOF 面）随 app-2 回调三口**，v0 不重建（成本 > 收益）。
- **freeze 载荷条件（立法）**：生命周期等待只许「非阻塞 recv(-EAGAIN) + sleep」两拍，**禁 object_wait_one(lifecycle, READABLE/WRITABLE, …) 阻塞等**——阻塞等通道的任务使 process_suspend 恒返 -EBUSY（freeze_precheck 检查项，sched.rs:966）；此约束随 app-2 回调封装原样继承，**阻塞化不是无害 API 演进**（先解 precheck 或立法 pause 前置握手）。
- **apptomb 稳态循环（两拍）**：①recv(lc) 非阻塞——得 RESUME → 打 resumed + 回 RESUMED；得 PAUSESV1 → 走 §4 契约；-EAGAIN → ②sleep 至下一心跳拍发 BEAT。四面走 berry-sys 绑定（vmo_write/vmo_read/berry_channel_recv/berry_thread_sleep）。
- **容器头部归属（app-2 前置立法）**：state blob 头部（magic/ver/CRC32/元数据）为 berry-libc lifecycle 三口**代持保留区**，应用 payload 自固定偏移 32B 起——v1 加 CRC 时由库盖头、应用侧零返工；`state_h()` 裸透句柄仅 v0 自验过渡形态。
- **D8 铅笔兑现追记（2026-11-10·app-2）**：本节「libc 双 crate 形（nanosleep/exit/EOF 面）随 app-2 回调三口」铅笔（调和书 D8）**已兑现**——app-2 刀1+刀2 apptomb 换链独立小工作区 musl + berry-ld、单依赖 berry-libc，sleep（libc `nanosleep` 同底 `berry_thread_sleep`）/exit（EOF 先行）/协议帧收发全经 libc 面（承载 = `berry-lifecycle` 纯 crate + berry-libc `lifecycle.rs` 三口）；迁移前后形态对照与执行结果 = app-2 草案 §4/抬头执行结果节。

## §6 验收面与 smoke 相拓扑

| 标记 | 相 | 判据（全通道确定性谓词） |
|---|---|---|
| `FROZEN ok n=1` | rootfs 相（+2，EFI 同打） | **前置：boot 走查零 `eco: app too many`**；suspend 返 0 → 8s 窗零新增 beat → RESUME/RESUMED 帧恰收 |
| `TOMB ok n=1` | 同上 | up → PAUSESV1 → saved c=N → reap0 → 记表 → relaunch → restored c=N+1 → reap1 → **表空** |

- **打包位（结构性零漂移）**：APPTOMB 打包进 rootfs **非 `app/` 前缀**（selftest/ 专用位，tomb_selftest 走 cpio 直查）——**不入 bap_scan 收集 = 不占 APP_SLOTS（现役恰 9/9 满，入册必挤 SETTINGS 出表 = APP ok 静默漂 8 + launcher 少一瓦）**、APP_LIST 不可见、SPAWN 不可达——`APP ok n=`/`APPUI ok n=`/boot 批语义/APP_LIST 全零漂移，白名单 skip 立法不再需要。
- **canon 两两互非子串**：`FROZEN ok n=1`/`TOMB ok n=1` 对现役全表（103 字面含 FRZ ok n=2/FRZCYC/APP 族）grep -F 互查通过；**备胎 `APPFRZ ok n=1`/`TOMBST ok n=1` 均过全表；`APPTOMB ok n=1` 不可用**（含 `TOMB ok n=1` 子串）。落码字面以本表全形为准（短形仅谓词名）。
- **连动 checklist（带落点）**：①markers.sh:23-24 数组尾 +2；②markers.sh:7 计数 51→53；③smoke.sh:245/:948/:1012 三处计数注释；④smoke-smp 数组自动继承（source 单源）——**SMP 面义务：段 b-2 收官实跑 `just smoke-smp` 钉板**（-EBUSY 重试已入 §3，不稳则两标记暂列观察项）；⑤smoke-image.sh:256 ECO 硬拷 +2 与 :132 计数；⑥规则/07:23 相构成表；⑦沿革五件套；⑦APP ok 语义复核（n 仍 =9）。
- **预算**：tomb_selftest 串行增量 ≈15-17s（8s 窗主体）；rootfs 相 DEADLINE 现值 600s（smoke.sh:175；:976 的 420 为陈旧注释勿引）+ EFI 窗在案 ≈4.5 分钟——**段 a-1 首跑实测记录并钉板余量**。

## §7 记账与帽（全族对账）

| 帽 | 值 | 状态 |
|---|---|---|
| state VMO（blob 帽） | 64KiB 定尺寸（结构性执法） | 业界水位：Android saved state 无显式帽（1MB 进程级 binder 软约束）、WP7 State ≈2MB、UWP 低端 16MB——64KiB 为 v0 值，v2 GUI 冻结解禁按 intent 档复核（AM-01 §③） |
| blob 头 | **12B 定长**（ver=0/hlen=12/rsv=0） | 本草案立法；v1 CRC 经 hlen 扩展；**追记（2026-11-10·app-2）**：v1 起升 32B（ver=1/hlen=32，前缀 12B 字段布局同位——app-2 草案 §3.1） |
| pause deadline | 10s（appd 自**发出**起算，含投递等待） | 值同规范 §4.1-1、起算以本表为准；业界校准 iOS/UWP 5s——v2 按 intent 分档 |
| 墓碑表 | RAM 8 槽；id 查重 upsert；epoch=全局记入序（↔立法「时间戳」） | v0 定值；真策略配额 v2 |
| app 收集槽 APP_SLOTS | **9（现役恰满 9/9）**——APPTOMB 不入 `app/` 前缀零改 | 抬帽路线（连动双 const-assert/gui8-smoke n=9 锚）明确不取 |
| env extras（apptomb） | [4][5] 两枚，6/8 | 仅 selftest 通道装配 |
| reap 绝对帽 | 30s 照旧 | deadline 10s ⊂ 30s |
| smoke 墙钟 | rootfs 相 +15-17s 串行、EFI 同吃 | 段 b-2 核账 |

## §8 实施刀程（三段，各段独立可验可提交）

- **段 a-1**：apptomb bin（心跳/RESUME 面/契约）+ libc 访问器两枚 + suspend(-EBUSY 重试)/静默窗/RESUMED 序列 + `FROZEN ok n=1` + terminate(hp,9) 收口。验收：rootfs 相 +1 + 首跑墙钟实测入档。
- **段 b-1**：双 gift + PAUSESV1/12B 头/记表/出表/relaunch + `TOMB ok n=1` + 负路径五件人工面。验收：rootfs 相再 +1 + EFI 停摆窗复核（launcher RPC_CAP_NS=300s 现值 ≥ 停摆窗 + 裕量，落码复核即过）。
- **段 b-2（文书收官）**：smoke 三脚本 + 规则/07 + **`just smoke-smp` 实跑钉板** + ADR 追记（#42：freeze 级首个消费位 + debounce 基线 10s【推，am Q3 与 Android DEFAULT_FREEZER_DEBOUNCE_TIMEOUT 同值互证——与 §3 的 8s 判据窗、规范 10s deadline 三名分写勿混】；#64：消费链首证 + 冻结期入站 IPC v0 语义 + AM-1.5 升级路径引注 + 晋升规则 K5）+ 规范两处联动改（§4.1-1 超时处置注「不重启」；§13.5「墓碑记录」→「死因/退出记录」防 Android 义撞名）+ 应用模型 §4 消息表 resume 复用注 + AM-01 边界⑤闭（WP7 一手源已取）+ DEADLINE 核账 + 索引/沿革 + mkapp 自检金件。

## §9 kill criteria

1. **K1**：TCG 静默窗抖动 → 降「suspend 返 0 + close 后 RESUMED 帧」两步正谓据（零新面零协议联动）；再不稳 → psproto 加 frozen 位（**账本位**：suspend 同步契约「返回时已 FROZEN」+ token 在手即冻结，零内核查询面；telemetry 计数器无用户态读口不可作判据）。内核侧零新号备选（Process 派生 FROZEN 信号位，object_wait_one 可 poll）挂调点 1.2/v2，不入本刀。
2. **K2**：suspend 与 appd wait_many 交互畸 → selftest 专用串行 wait（备锚：调点 v1.1 §5.8 已拍板的通道×冻结语义）。
3. **K3**：state VMO 跨 spawn 复用畸 → 降级「blob 经 channel 消息迁移 VMO handle」（3-4b 迁移原语）。
4. **K4**：extras 紧致律冲突 → apptomb 恒 console=true（自验件无 console=false 需求）——**manifest `console` 须同步写 true**（a-1 在途版 console=false 属过渡：显式 SpawnSpec.stdout 直授路径下诊断行照打，但误经 app_launch 路径会触发私有对替换——b-1 前随 D11 改）。
5. **K5（晋升规则立法）**：v2 冻结态 app 晋升墓碑 = **默认不唤醒**（明文禁「压力期 thaw→save→kill」为默认路径）——直接 kill+reap，blob 取冻前 onBackground 窗口已存状态；未存者 = 诚实边界「relaunch 全新启动」不入表；压力第一阀 = am L1 pager 换出（非晋升级）；PAUSESV1 仅服务未冻结路径，不得复用作冻前保存消息。
6. **K6**：app-2 回调阻塞化后 suspend 竞态 EBUSY → 过渡期保留非阻塞 poll 内核，或立法「pause 先发消息、app 退出阻塞等再冻」握手序。

## §10 风险与首验点

- **R1（已钉板）**：ChildHandles.hp **恒带 RIGHT_MANAGE|RIGHT_DUPLICATE**——process_create 出权即含（sys_process_create），spawn 链零收紧（只 dup 不 take、原值返还至 reap）；0x3E 权利门必过（运行时拒面 Exited/自冻/pager 交叉与权利无关）。suspend 直用 hp；原 fallback 句删除。
- **R2（QEMU 面复证）**：vmo_write 对「他人创建、dup 授子」直写——现役同构先例 = vblk 数据面（client 建本体/dup 别名随 send 迁移/driver 收别名无映射 vmo_write 回填，`VBLK ok`/`FAT ok` 即此链）；tomb_selftest 本体即首验载体（saved/restored 行）；如需隔离补 kernel ktest 一刀。
- **R3**：零态/有态双启动路径 host 对拍（首轮 VMO 零态 = 全新路径；relaunch = BTMB 路径）。
- **R4**：canon 复核进落码 checklist（grep -F 全表；备胎 §6）。
- **R5**：-EBUSY 瞬态源清单实测复核（console 背压/SMP 副核窗——§3 退避参数）+ EFI 停摆窗 × launcher RPC 帽余量。

## §11 ADR 与文书清单

- **ADR #42 追记**：freeze 级首个消费位（触发器挂 GUI 线、须排 AM-1.5 后）；debounce 基线 10s【推】（am Q3 × Android 同值互证；与 8s 判据窗/10s deadline 三名分写）。
- **ADR #64 追记**：freeze 首个用户态生命周期消费链；冻结期入站 IPC v0 语义 + AM-1.5 升级路径；晋升规则 K5；**v2 分流占位**（对标 Android binder-freezer FROZEN_CALLEE_POLICY 三档 DROP/ENQUEUE_MOST_RECENT/ENQUEUE_ALL）+ `sys_channel_send` 冻端检查缺口记调点 1.2。
- 规范两处联动改 + 应用模型 resume 复用注 + AM-01 边界闭（详见 §8 段 b-2）。
- 规则/05 补瘦行（判例如成）；索引/沿革；mkapp 自检 APPTOMB 金件。

### §11.1 ADR 追记挂账（已于 2026-11-08 执行兑现——两追记逐字落入 `架构决策.md` #42/#64 行内，配方原文留档备查）

> **2026-11-08 已执行（W2 docs 刀）**：并行窗开（c0e0d84 落库后零 open PR），AM-1.5 前置已满足（79efddf/90c7f25 在基线）——两条追记已逐字落入 `架构决策.md` #42/#64 行内（含头部沿革行登记）。以下配方原文留档备查。

- **#42 追记**：intent 单轴消费表 freeze 级首个消费位已落（tomb_selftest freeze-first，`FROZEN ok n=1`，刀 ba11596/aaef81d）；真触发器挂 GUI 线、排 AM-1.5（wake-pending）后；GUI 触发时回引 §4 thaw-then-pause 不变式。debounce 基线 10s【推】（am design Q3 × Android 同值互证）——与 §3 的 8s 判据窗、§4.1-1 的 10s pause deadline 三名分写勿混。
- **#64 追记**：调点 1.1 S3 freeze 首个用户态生命周期消费链已成（映像 VMO 柄留 svcmgr 表 → 子表 pager_load=0 → 不触 D6；FROZEN/TOMB ok 实证）。双残影修复记载（sched.rs freeze_precheck，判例 §A144/刀 d563485）。冻结期入站 IPC v0 语义：无冻结检查、消息静默入队不丢、满 8 即 -EBUSY、无致死无自动解冻（auto-thaw 仅 token 持有者死亡安全门）；唤醒闭环 = AM-1.5。晋升规则 K5：v2 冻结态晋墓碑默认不唤醒（禁「压力期 thaw→save→kill」默认路径）——直接 kill+reap 取冻前已存状态，未存者「relaunch 全新启动」不入表；PAUSESV1 仅服务未冻结路径。v2 分流占位：对标 binder-freezer FROZEN_CALLEE_POLICY 三档；sys_channel_send 冻端检查缺口记调点 1.2。

## §12 草案方法注记

v1.0 立稿 = 会话直读代码实测；**v1.1 = workflow 对抗审查（64 agents：55 raw → 47 收编/8 refuted）全量收编**——refuted 主要类型：对已立法面的重复提案、基于陈旧行号的误判、与范围盒纪律冲突的预注册。综合修订由父会话执行（spill 截断，47 条 fix_final 全文在手直接整合）。
**先例对标（WP 墓碑教训）**：墓碑一词出自 WP7——初版切走即快杀、开发者「point of contention…too difficult」、官方忠告「别把墓碑留到开发末尾」；两级化（Dormant/Tombstoned + 5 墓碑帽 + 事件 10s 限）系 Mango 补课。BerryOS 三对策在册勿回退：①超时不入表 + fresh 重启安全降级；②恢复过渡 display 直驱；③freeze 级零配合垫底——v2 策略面禁走「强求应用配合」单级路线（AM-01 反面清单同裁）。

## §13 对照注记（业界连线，证据 = 底稿路径）

1. **冻结豁免 × 长时任务**：鸿蒙 DATA_TRANSFER 长时任务仅升优先级、进程仍会被挂起，传输走系统 request 代理托管（`research/应用规范-HarmonyOS对标-2026-11-补集2-三号.md` §3）——连线 BerryOS 资源证据模型（规范 §4.2）与 freeze 豁免白名单（am design Q3；真触发挂账）。
2. **快照语义 × 划掉即丢**：鸿蒙 mission 快照系统代持 + removeMissionAfterTerminate 销毁即移除（一轮底稿「任务管理」节）——对照「SavedState 划掉/force-stop 即丢」（am design §SDK 契约）：v0 relaunch 仅复活读档，「划掉即丢」边界随 app-2/GUI 线。
3. **blob 边界**：代理提醒「应用死了提醒仍在」= 系统服务持 timer handle——timer/通知不进 64KiB blob（规范 §4.2 已立法），blob 只载 SavedState。
4. 引鸿蒙「应用分组冻结（cgroup 式）」时必标【待证·未核实】（对标主稿明注华为商用未公开）。

## §14 双线调和记录（2026-11-08·v1.2）

兄弟线（am 会话）按 v1.0+H 系审查实施段 a-1 并提交 `ba11596`；本 v1.2 经 workflow `app1-reconcile-audit` 对照后**为段 b-1/b-2 唯一基准**。三 MAJOR 分歧与全套 D 系调和配方（含 b-1 前必修雷三件：io.rs state_h 只读注释 D6 / 退避参数 D4 / manifest console D11）全文 = `research/app-1调和审计-2026-11.md` §二–§四；开窗触发条件三件 = 同文件 §六。要点索引：D1 打包位（v1.0 白名单在途版致 SETTINGS 挤出 = `gui8-smoke MARK_APP_N9` 必红雷，b-1 前按配方切 selftest/ 前缀）；D2 RESUME 消息化（在途差值判为过渡形态）；D3 内核残修（§0 注 + ktest 双断言/判例欠账随 b-2 提前）；H 系七条已回灌本文（H1 判据载体 §3 步1/③drain 步3/④stdin_feed 步1/⑤瞬态实测 步2/⑦L1 机制 诚实注/②双残影 §0）。
