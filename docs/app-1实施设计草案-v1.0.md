# app-1 实施设计草案（应用生命周期第二刀：freeze-first + 墓碑协议 v0）

> 版本：**1.0**（v1.0）｜2026-11-08

> **依据**：`应用模型设计-v1.0.md` §4/§4.1（生命周期消息协议 + 墓碑模式，立法原文）+ `应用规范-v1.0.md` §4.1/§4.1-5（应用侧契约与 lifecycle API 承载）+ ADR #64（调点 1.1 S3 freeze 已落内核侧）+ `research/SDK-API历史教训/wf-result-full.json`「app-1 待做」行。现状四层账 = 2026-11-08 会话实测（§1 锚点）。
> **时点**：GUI-8 段④ 在途收官窗后、SDK-2 开刀前——appd 域纯用户态一刀，与 GUI 线文件面互斥成立（svcmgr 共享面错峰纪律照旧）。
> **形态先例**：`app-0实施设计草案-v1.0.md`（草案 → 对抗审查 → 拆步实施，每步独立提交）+ T-1 `thr_selftest` 拉起 thrchild 的自验 app 形态（smoke.sh:743）。
> **执行结果**：（占位——随段落地追记；对抗审查随开工，先审查后动码。）

## §0 目标与范围盒子

**一句话**：把「active → freeze → tombstone → not running」的后两跳从设计文档变成第一片可跑代码——**段 a freeze-first**（appd 用现役 0x3E 冻结后台应用、`FROZEN ok` 谓词）+ **段 b 墓碑协议 v0**（pause(save) → state blob ≤64KiB → 杀 → 墓碑表 LRU → relaunch 读档复活，`TOMB ok` 谓词）——确定性状态谓词，不赌竞速。

**范围盒子（v0 不做，逐条注明去处）**：

| 不做 | 原因 | 去处 |
|---|---|---|
| 真触发策略（GUI 失焦/内存压力） | v0 无 wmd 焦点事件面、TCG 无真压力 | 失焦触发挂 GUI 线（wmd 焦点→appd）；压力触发挂 v2 策略（intent + 遥测消费，ADR #42 单轴表） |
| 跨 boot 复活（表落盘 + blob 落 $appdata） | fsmgr 平名 9 文件 + FAT 根 16 项无子目录，$appdata 树未落 | v2（存储布局 §8 阶段表）；v0 表驻 RAM = reboot 全墓碑（语义安全已裁） |
| GUI 应用冻结 | demand VMO 与 freeze 相交（pager_load>0 → -EBUSY，内核已执法）；GUI extras 8/8 恰满（GUI-8 §2.2） | v2 obligations 刀（ADR #64 已挂 1.2）+ extras 槽预算另裁 |
| berry-libc 完整 lifecycle 回调三口（规范 §4.1-5） | v0 先通协议，API 面最薄 | 随 app-2/GUI 线（规范已立「零义务起步」） |
| freeze 的 SMP IPI / obligations | 调点 1.1 明挂 1.2 | 调点 1.2 |
| 多应用并发墓碑/批量压力 | v0 单活自验 | 随真策略 v2 |

## §1 现状四层账（2026-11-08 实测锚点）

| 层 | 状态 | 锚点 |
|---|---|---|
| 协议设计 | ✅ 已冻 | 应用模型 §4.1（消息表 + 墓碑流程 + 64KiB 帽 + reboot 语义）；规范 §4.1-5 |
| **内核 freeze 原语** | ✅ **已落（比设计文档口径提前）** | `berry_process_suspend(target, options, *token)`（0x3E）真实现役：`kernel/src/syscall.rs:1652`——进程全冻（options=0，需 Process RIGHT_MANAGE）+ 线程 token freeze（SINGLE_THREAD 需 FREEZE_READY 自证）+ 嵌套 depth；**解冻 = handle_close(token)**；自冻拒绝；pager 进程拒冻（-EBUSY）。ktest `FREEZE-K` + thrchild 活证（调点 v1.1 M1.1-3 ✅） |
| appd 消费链 | ❌ 全缺（本刀主体） | svcmgr 全源 grep pause/lifecycle/墓碑零命中；无墓碑表、无 relaunch 注入 |
| 机制底座 | ✅ 全在役 | env 块 gift（base[0..4] + extras[4..8]，`berry-exec lib.rs:860-901`——前缀紧致律 + 双断言）/ spawn 返 `ChildHandles{hp,hv,ht,svmo}`（hp = Process 句，wait/reap 用）/ EOF+reap 真码链（app-0）/ 监督重启（#28）/ VMO 读写 syscall / `THR ok` 先例（thr_selftest 拉起 thrchild） |

## §2 两段刀法总览

```
段 a（freeze-first）        段 b（墓碑 v0）
─────────────────          ─────────────────────────
apptomb 长活自验 app         同一 apptomb，lifecycle 面
appd: suspend(hp,0)→token    spawn 双 gift: [4]lifecycle端 [5]state VMO
静默窗验证（冻住）            appd 发 PAUSESV1 → app 写 blob → exit(0)
handle_close(token) 解冻     appd reap + 墓碑表记（LRU 8 槽）
app 心跳恢复+resumed 行       relaunch: 同 .bap 二次 spawn + state VMO(READ)
FROZEN ok n=1                app 读档 restored c=N → 正常退
                             TOMB ok n=1
```

**为什么 freeze-first 先行**：设计终态「失焦先 freeze、压力上收墓碑」（应用模型 §4.1）的两级形态中，freeze 级**零应用配合**（内核原语直接可用）——先拿「后台不烧 CPU」省电红利，墓碑级再上协议。

## §3 段 a：freeze-first（`FROZEN ok`）

- **apptomb（新自验 bin）**：headless 长活应用——启动打 `app apptomb up` → 周期（2s clock_sleep）打心跳行 `apptomb beat c=N`（**非标记**，诊断行）→ 稳态循环。**不进 boot 批**（由 svcmgr `tomb_selftest` 拉起——`thr_selftest`/thrchild 同型先例）→ `APP ok n=` 语义零漂移；不 `ps_reg`（瞬态自验件惯例）。
- **svcmgr freeze 自验序列**（确定性谓词，串口行有无判据）：
  1. tomb_selftest spawn apptomb → 等 `app apptomb up`（强制状态行，不 sleep 回退）；
  2. `berry_process_suspend(hp, 0, &token)`——hp 来自 ChildHandles（**首验点 R1**：确认 spawn 链留存柄带 RIGHT_MANAGE——3-3 出权口径应含，实测钉板）；
  3. **静默窗**：等 ≥3 个心跳周期（TCG 放宽至 8s 窗）窗内串口**零** `apptomb beat` 行——冻结的进程不再跑 = 零心跳（确定性：串口行有无，不赌精确时序）；
  4. `handle_close(token)` 解冻 → 等 `apptomb resumed c=K` 行（apptomb 主循环 poll 一个解冻哨兵：v0 = 心跳计数骤停恢复自打 resumed；见 §5）；
  5. 打 `FROZEN ok n=1` → 令 apptomb 自退（发 pause 走段 b，或 v0 段 a 直接 process_terminate + reap——**取前者**：段 a 段 b 共用一次生命周期，smoke 一次跑全）。
- **判据诚实注**：静默窗 = 否定判据（零行）——若 TCG 下 8s 窗抖动误报（心跳行迟到），kill 线降级为「psproto/遥测 frozen 位」判据（§9 K1）。

## §4 段 b：墓碑协议 v0（`TOMB ok`）

- **双 gift（extras 槽位）**：spawn apptomb 时 env extras 装两枚——`[4]` = lifecycle channel 端 B（appd `channel_pair` 持端 A）+ `[5]` = **state VMO**（appd 预建 `vmo_create(64KiB, READ|WRITE)`，dup 授子）。apptomb 面 6/8（base4+extras2），前缀紧致律天然满足（console=true 有 stdin）。
- **blob 帽 = VMO 尺寸即帽（结构性执法）**：64KiB 定尺寸 VMO，越写物理不可——无需 appd 二次执法（应用模型 §4.1「appd 执法」的更强形态）。
- **pause(save) 消息**：appd 端 A 发 8B `PAUSESV1\0`；**deadline 10s 由 appd 侧计时**（clock 读点对点）——超时未退 = 监督杀（fault 槽 + `eco: tomb <id> pause timeout` loud 行）+ **不入墓碑表**（失败态不复活）。
- **apptomb 契约**（收到 pause 后）：序列化 `BTMB`（u32 魔数）+ ver(u8) + counter(u32) 写 state VMO（`berry_vmo_write` 直写——**零映射，零 mapcnt 纠缠**）→ console 打 `apptomb saved c=N` → libc `exit(0)`（EOF 先行）。v0 无 CRC（协议纯度低如实注；v1 加 CRC32 随 app-2）。
- **墓碑表**：svcmgr static 8 槽 `{id[24], svmo: Handle, epoch: u64}`——reap 成功（真码 0）后记入；LRU = 满时最旧槽 `handle_close(svmo)`（blob 真回收）。表驻 RAM，reboot = 全墓碑（语义安全：等价「没跑过」）。
- **relaunch**：appd 二次 spawn 同 `.bap`（bap_scan 记账表现查，零重扫），extras `[5]` = 同枚 VMO 的 `duplicate(READ)` 收紧授子（复活者只读）；`[4]` 新 lifecycle 端（通道不可复用）。apptomb 启动 `vmo_read` 首 4B：`BTMB` → counter+1 恢复 → 打 `app apptomb restored c=N` → 稳态短跑（≥1 心跳）后自退 0（v0 复活即证，不常驻）。
- **appd 判据**：按序收 `up → saved → restored c=N+1`（N = 首轮 counter 对拍）+ 两轮 reap 真码 0 + 墓碑表槽回收 → 打 `TOMB ok n=1`。与 `FROZEN ok` **独立判**（app-0 `APP ok`/`APPUI ok` 先例：互不连坐）。
- **负路径**（CI 不可达，人工验收面——app-0 坏输入惯例）：①pause 超时杀（临时慢 apptomb 注入）→ `eco: tomb` 行 + 不入表；②blob 越写（改 VMO 尺寸负例 host 侧）结构性拒；③墓碑表 8 槽满 LRU 淘汰最旧。

## §5 应用侧最小面（apptomb + berry-libc 访问器）

- **berry-libc 新增访问器两枚**（同 `fs_h()`/`wm_cli_h()` 惯例——从 env 块固定位读，零新机制）：`berry_lifecycle_h()`（extras[4]）+ `berry_state_h()`（extras[5]）。**规范 §4.1-5 的回调三口不在 v0**——apptomb 手写 poll 主循环（recv -EAGAIN 非阻塞 + 2s 心跳），作为协议活证；回调封装随 app-2。
- **apptomb 契约三条**（规范 §4.1 应用侧）：argv0 = entry 断言 / EOF 先行退出 / 退出码 0 语义不变。
- apptomb 打包进 `app/APPTOMB.bap`（mkapp 门禁全过）+ build-user 条目表 +1（**APP ok n= 零漂移成立的前提 = 它不在 boot 批**——bap_scan 记账在册但不随 boot 全量拉起，拉起权归 tomb_selftest；boot 批「全量」语义的例外需在 svcmgr 注释立法：自验 app 族 = 显式拉起白名单）。

## §6 验收面与 smoke 相拓扑

| 标记 | 相 | 判据（确定性状态谓词） |
|---|---|---|
| `FROZEN ok n=1` | rootfs 相 +2（EFI 路同打——appd 同二进制） | up 行 → suspend → 8s 静默窗零心跳 → 解冻 → resumed 行 |
| `TOMB ok n=1` | 同上 | up → PAUSESV1 → saved c=N → reap0 → relaunch → restored c=N+1 → reap0 |

- **canon 两两互非子串**：`FROZEN ok`/`TOMB ok` 对现役全表复核（含 `FRZ ok n=2`——T-1 线程冻结核面标记；**「FROZEN ok」不含「FRZ ok」为子串**（F-R-O-Z-E-N 插入了元音），`/usr/bin/grep -F` 落码期复核义务）；`TOMB` 前缀族现役零占。
- 连动：ROOTFS_MARKERS +2 → RX 合相 / smoke-smp 数组 / smoke-image ECO canon / 规则/07 相构成表 / 沿革五件套（GUI-8 §1.2 惯例全量）。
- **零漂移面**：`APP ok n=`/`APPUI ok n=`/boot 批语义不动；`THR ok`/`FRZ ok n=2`（T-1 核面）不动。

## §7 记账与帽（全族对账）

| 帽 | 值 | 状态 |
|---|---|---|
| state VMO（blob 帽） | 64KiB 定尺寸（结构性执法） | 应用模型 §4.1 立法值落地 |
| pause deadline | 10s（appd 计时，超时监督杀） | 规范 §4.1-1 同值 |
| 墓碑表 | RAM 8 槽 LRU（close 即回收） | v0 定值；真策略配额 v2 |
| env extras（apptomb） | [4][5] 两枚，6/8 | 满配预算不受挤 |
| reap 绝对帽 | 30s 照旧（段 b deadline 10s ⊂ 30s） | 零改 |
| APP_SPAWN_LIVE_MAX | 4 不动（apptomb 走 selftest 通道非 RPC） | 零改 |
| psproto | 不动（apptomb 不入册） | 零改 |

## §8 实施刀程（三段，各段独立可验可提交）

- **段 a-1**：apptomb bin（心跳/契约三条）+ berry-libc 访问器两枚 + svcmgr freeze 自验序列 + `FROZEN ok`。验收：rootfs 相 +1 + host 测零漂 + R1 钉板（hp MANAGE 实测）。
- **段 b-1**：双 gift + PAUSESV1 + 序列化/读档 + 墓碑表 + relaunch + `TOMB ok`。验收：rootfs 相再 +1 + 负路径三件人工面记录。
- **段 b-2（文书收官）**：smoke 三脚本 + 规则/07 相构成表 + ADR 注记（#42 intent 消费位预留 / #64 1.1 消费首证追记）+ 索引 + 沿革五件套 + mkapp 自检样本（APPTOMB.bap 金件）。

## §9 kill criteria

1. **K1**：TCG 静默窗判据抖动（心跳行迟到误报冻结）→ 降级「psproto 加 frozen 位」判据（psproto 抬版本联动）；再不稳 → 段 a 判据改「suspend 返 0 + close 后 resumed 行」两步谓词（放弃否定判据）。
2. **K2**：suspend 与 appd wait_many 多路交互出畸（frozen 进程在等待集行为异常）→ apptomb 移出常驻等待集、selftest 专用串行 wait。
3. **K3**：state VMO 跨 spawn 复用出 mapcnt/所有权畸 → 降级「blob 经 channel 消息迁移 VMO handle」（方案 B，3-4b 迁移原语现役）。
4. **K4**：extras 前缀紧致律与 console=false 形态冲突（stdin 缺席拒 extras）→ apptomb 恒 console=true（v0 自验件无此需求）。

## §10 风险与首验点（段内首验，验不过即回草案改）

- **R1**（段 a 首验）：ChildHandles.hp 是否真带 RIGHT_MANAGE（process_create 出权口径——若 spawn 链中途收紧过则补 `duplicate(MANAGE)` 自持）。
- **R2**（段 b 首验）：`vmo_write` 对「他人创建、dup 授予」的 VMO 直写语义（无需映射）——host 侧单测先行。
- **R3**：relaunch 后 apptomb 读 BTMB 魔数时序（首轮 spawn 无 state VMO 的 v0 形态：tomb_selftest 两轮都带 [5]——首轮 VMO 内容为零态，apptomb 零态 = 全新启动路径；两路径对拍入 host 测）。
- **R4**：`FROZEN ok` 与 T-1 `FRZ ok` 的 canon 复核进 smoke 落码 checklist（grep -F 全表互查）。

## §11 ADR 与文书清单

- **ADR #42 追记**：intent 单轴消费表的 freeze 级首个消费位（interactive 失焦→freeze 的触发器挂 GUI 线，本刀只落机制不落策略）。
- **ADR #64 追记**：调点 1.1 S3 freeze 的首个用户态生命周期消费链（原「appd 生命周期消费链未接」缺口就此收口首段）。
- 规则/05（服务监督）：appd 域墓碑表/deadline 执法补瘦行（判例如成）。
- 索引登记 + 沿革（草案立稿行 + 各段收官行）；mkapp 自检样本扩 APPTOMB 金件。
- 本草案不触碰 `docs/应用规范-v1.0.md`/`应用API全景-v1.0.md`（规范 §7 速查表「后台常驻/墓碑」行随段 b-2 收官时从 ⛔ 翻 ✅——两档联动改）。

## §12 草案方法注记

现状四层账与全部锚点 = 2026-11-08 会话直读代码实测（syscall.rs:1652 原型与解冻语义/berry-exec extras 契约 lib.rs:860-901/ChildHandles 结构/smoke 标记表 FRZ 占位排查）；无 scout workflow（承重四事当场复核：0x3E 原型、extras 槽律、THR 先例形态、FRZ 撞名）。对抗审查随开工（app-0 形态先例）。
