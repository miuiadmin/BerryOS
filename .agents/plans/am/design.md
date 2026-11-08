# am（应用后台处置 App Manager）总体设计

Type: spec
Status: active
Updated: 2026-11-08
Version: 0.2.0

一句话说明：am 是用户态后台处置服务——把「收割 → 换出 → 冻结 → 杀+墓碑 → 归档」五级梯度做成策略可插拔的监督服务，内核机制面复用 1.1 token freeze 与 VMO/pager（零新增 syscall），墓碑采 Fuchsia 形态（无序列化、pager 惰性恢复）。**v0.2.0：Q1–Q3 三裁决落定（见「裁决记录」节），AM-1 可开工。**

## 背景

- 证据基座 = [`research/AM-01-墓碑模式开源对标-v1.0.md`](../../../research/AM-01-墓碑模式开源对标-v1.0.md)：开源谱系四层 + 最先进八判据——主路径 freeze+压缩换出（Android 官方 −30% 冷启动），最先进形态 = Fuchsia「墓碑是 VMO + 用户态 pager + 持久存储的自然推论」，与 BerryOS 架构同构。
- 内核机制面已在，am 是组合者不是发明者：token freeze（`调点调度-v1.1` W3：0x3E `process_suspend` + `SuspendToken` ObjType 12 + `FREEZE_READY` 自证 + 三重正交谓词，已落地含 S5.11 遥测）；VMO 5 类后备与可换出 + 用户态 pager（`内存管理设计-v1.0` §2/§4，X-6 供给链在途）；同步 channel + Event 通知；telemetry 环（ADR #45 不建第二数据库）。
- 契约层已立法：PM-00 裁决「默认冻结非杀」day-one、manifest 能力位、长时任务 reason u8 + 系统校验；ADR #68 纪律——SDK 面零新 syscall、纯协议 crate（berry-pwproto 先例）。
- 排除 8 问：本设计纯机制借鉴（Android/Fuchsia/OH 文档级），无代码引入，许可边界干净。

## 范围

**做**：用户态 am 常驻服务（监督域 = `规则/05`）；五级梯度与触发策略；压力信号（PSI 等价）；SDK 生命周期契约（berry-amproto）；快照接口；KB 级墓碑状态托管。
**不做（v1）**：全量进程 checkpoint（AM-01 kill criteria：dump 22GiB≈54s 级，直接否决）；WP 式墓碑文件特例；冻前 GC 实做（v1 无 GC runtime，只留钩子接口）；跨核冻结一致性（v2，单核 v1 三重正交已防停摆）；AI 自动调梯度（先 KC 后实验轨道，AM-5）。

## 契约

### 五级梯度（时间尺度 × 代价升序）

| 级 | 触发 | 动作 | 状态归宿 | 恢复成本 |
|---|---|---|---|---|
| L0 discardable 收割 | 内存低水位 | app 声明 `DISCARDABLE` 的匿名 VMO 由内核择机弃页（Q2 裁决：`vmo_create` 选项位 + 锁语义挂 commit/decommit 动词扩展，零新 syscall） | 页丢失、对象在 | app 自重建（重新 lock 填充） |
| L1 pager 压缩换出 | 冻结后 + 水位 | am 请 pager 收该进程匿名页进压缩仓（zram 等价），日预算护闪存（OH 借鉴：初值日 50MB 量级，遥测可调） | 压缩仓/盘 | 缺页级惰性重取 |
| L2 冻结 | 进后台 debounce（Q3 裁决：自适应——基线 10s【推】；压力 WARNING→0；intent/返回概率调制钩子 AM-5 接） | am 持 SuspendToken 调 0x3E；冻前发 onBackground（限时）；豁免面 = 长时任务 reason 白名单（PM-00 已立法；Q1-c 使绑定豁免消失） | 进程内存原地 | 解冻（近零） |
| L3 杀 + 墓碑 | 压力信号超阈（L0–L2 不足） | 按优先级表选列 kill；task-bound KB 级状态已托管；合成器快照先行；退出原因可查 | am 状态目录 + 快照 | 冷启动 + 状态重放 + 快照遮盖 |
| L4 归档 | 数月未用（v2） | 权限/缓存/包体联动回收（PM-00 契约层已定豁免位） | 存储 | 解档重装 |

**决策归属**：梯度选择与阈值全在 am（用户态策略），内核只提供机制（freeze/VMO/遥测）——SCHED-01 教训「策略不进快路径」。intent（ADR #42/#58）与返回概率（AI 实验轨道）作 am 的策略输入，调 debounce 与换出深度。

### 压力信号（PSI 等价）

内核记「等页时间」（缺页等待 + pager 供给停顿）进 telemetry 环；阈值事件带内发给 am。初值借 lmkd 量级【推，需实测定标】：partial 70ms / complete 700ms 窗口（AM-01 [A2]）；不设固定 minfree 水位（内核态 LMK 反面教训）。

### SDK 契约（零新 syscall，照 #68 六面先例）

berry-amproto 纯协议 crate + berry-sys 包装，全走服务 IPC：生命周期回调（onBackground 限时落盘窗口 / OnResume；超时不阻塞冻结）；SavedState（KB 级、task-bound、am 托管——划掉/force-stop 即丢，AM-01 [A7] 语义）；豁免位（manifest 长时任务 reason u8，PM-00）；冻结态通知（FrozenStateChange 等价，供系统服务暂停对冻结进程的回调投递、解冻后批量补投——A16 形态）。

### IPC × 冻结（Q1 已裁：唤醒即解冻，v1 直上）

BerryOS channel 本就是**非阻塞有界队列**（send 满拒 -EBUSY、阻塞等待走 `wait_one`），冻结进程的端点收消息**天然排队**——A14 广播排队的形态免费获得。补一层唤醒闭环即成完整语义：

- **机制面（AM-1 前置内核小刀）**：send 命中冻结进程的端点 → 内核对该进程冻结态打 **wake-pending** 标记 + Event 通知 token 持有者（am）；消息已入队不丢弃。零新 syscall（Event 通知走现有机制）。
- **策略面（am）**：收到唤醒通知默认**立即 close token 解冻**（等效 A14「生命周期事件立即解冻」）——调用方的 `wait_one` 自然解除，零生态负担；是否解冻的裁决权留在 am（策略不进内核快路径）。
- **安全论证**：1.1 安全门已有「token 持有者死亡 → auto-thaw」——am 崩溃 = 全体解冻 = 系统退化到无冻结而非死锁；三重正交谓词已封内核侧停摆。
- **防风暴**：wake-flood 遥测（解冻频次进 HDR 分位）；反复被唤醒者 am 可升级处置（转长时任务豁免或杀），决策可查。
- **b 形态（-EAGAIN/TRY）不设新口**：调用方需要带超时的探活，用既有 `wait_one` 超时即得——不为罕见场景加 ABI 面。

### 快照与优先级表

快照：am 请求合成器截屏进共享 VMO（零拷贝），双分辨率（1.0/0.5 借 TaskSnapshot [A3]），恢复时作首帧前窗口——接口依赖 GUI 栈在途里程碑，am 只定 IPC 形状。优先级表：OH 式事件驱动更新（前后台/长时任务/绑定/使用统计），am 服务自身受 `规则/05` 监督（常驻分级）。

## 验收标准

- **AM-1（服务+冻结决策）**：ktest 注入假压力源，梯度决策确定性（同输入同输出）；smoke AM 相——后台 app 冻结后零 CPU（WAKESRC 断言零唤醒）且解冻即续跑。
- **AM-2（压力信号）**：memhog 注入下梯度逐级触发不越级；等页时间统计与注入量单调。
- **AM-3（杀+墓碑）**：kill 后重拉，SavedState 恢复 + 快照先行显示 + 退出原因可查；三 boot 不变量（smoke 三相）。
- **AM-4（换出预算）**：日换出预算执法（超限停换出只冻结）；恢复走缺页重取（X-6 供给链）。
- **度量纪律**：冻结延迟/解冻延迟/恢复至首帧进 HDR 分位（v1.3 七刀基础设施复用）；决策与豁免全量遥测可查（「为何冻/为何杀」可解释——AM-01 判据 5）。

## 落地刀序（依赖在途 lane）

AM-0 本设计（含 Q1–Q3 裁决）→ AM-1 前置内核小刀（wake-pending 标记 + Event 通知 token 持有者）+ am 骨架 + 优先级表 + L2 冻结决策（freeze 已落，可即开）→ AM-2 压力信号（telemetry 增量）→ AM-3 杀+墓碑+快照（依赖 X-6 收官 + 合成器截屏接口）→ AM-4 L1 换出预算 + L0 discardable（依赖 pager 压缩仓成形 + `内存管理设计` v1.1 补 DISCARDABLE 条目）→ AM-5 归档 + AI 返回概率调梯度（KC 先立，PM-00 KC-P 同款立法）。

## 适用边界

- v1 单核：冻结谓词三重正交防停摆已足够；跨核一致性 v2。
- 冻前 GC 钩子只留接口——v1 应用生态无 GC runtime；未来 runtime 接入时由 runtime 消费。
- 本设计不动 `调点调度-v1.1` 与 `内存管理设计-v1.0` 条文——discardable（Q2）若裁入 5 类枚举/标志位，先升 `内存管理设计` v1.1 再动代码。
- 与 PM-00 分工：PM-00 管契约形状（豁免位/白名单/能力位），am 管机制执行——冲突以 PM-00 为准。

## 裁决记录（2026-11-08 仓主授权拍板，因果详[笔记](../../notes/implemented/2026-11-08-am-q1q2q3-rulings.md)）

- **Q1 同步 IPC × 冻结 → 唤醒即解冻（c），v1 直上**：非阻塞有界队列天然排队 + wake-pending 通知 am + 默认立即解冻；token 持有者死亡 auto-thaw 封死 am 崩溃面；-EAGAIN 不设新口（`wait_one` 超时即得）。弃 b 的理由：全员重试 = 生态负担，违「默认值即生态」。
- **Q2 discardable → 匿名类 `DISCARDABLE` 选项位，非第 6 类后备**：回收策略与后备来源正交（Fuchsia 同款——ZX_VMO_DISCARDABLE 是标志不是类型）；锁语义挂 commit/decommit 动词扩展，零号预算；动代码前先升 `内存管理设计` v1.1 补条目。
- **Q3 debounce/豁免 → 自适应三元 + 豁免面缩到白名单**：基线 10s【推，HDR 实测定标】/ 压力 WARNING→0 / intent·返回概率调制（AM-5 接钩子）；豁免面 = 长时任务 reason 白名单（PM-00 已立法）——Q1-c 使绑定豁免消失（call 即唤醒），豁免面保持小而可审计（bfcache 教训）。

## Related

[AM-01 调研](../../../research/AM-01-墓碑模式开源对标-v1.0.md) · [调点调度 v1.1](../../../docs/调点调度-v1.1.md) · [内存管理设计 v1.0](../../../docs/内存管理设计-v1.0.md) · [PM-00 省电综合裁决](../../../research/PM-00-全系统省电架构综合裁决-v1.1.md) · [SCHED-01](../../../research/SCHED-01-调度之王业界深研与综合裁决-v1.0.md) · [规则/05 服务监督](../../../docs/规则/05-服务监督.md)
