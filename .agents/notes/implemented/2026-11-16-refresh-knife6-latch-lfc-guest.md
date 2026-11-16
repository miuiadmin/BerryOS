# Agent Note: 刷新率刀6 latch-at-vsync + 双缓冲 + LFC + 应用面（P2/P5/P6 纯核 + k6 guest 三靴 + 四竞速定谳）

Status: implemented

## Problem

[刷新率规范 v1.0](../../../docs/刷新率体系规范-v1.0.md) 在刀4（[笔记](2026-11-16-refresh-knife4-present-receipt.md)）收口呈现预告/回执后剩四笔欠账：§7 P2/P5（target_present_ns 向上取整到拍点 latch、双缓冲深度 1 管线与 squash 弃帧）、P6（FreeRange per_frame 逐帧扫描——无网格钳制的拍点驱动）、§6.3 A8 LFC（vrr 三态 + 整数倍重发）、§9 AP2/3/4 应用面（固定时步/整帧量化/SUBMIT 截止）。依赖按 2026-11-16 对齐结论从「刀4+刀5」放宽为刀4：T2/T5 门走 DETERMINISTIC 定步钟、T3 门量名义网格相位（结构性零残差），均不依赖刀5 hrtimer 精度。刀6 = [分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md) §7。

## Decision

- **kernel fw_cfg 四态**：`opt/berry/panel-model` 解析 `tiered|tiered:vrr|freerange|freerange:per_frame`（缺省 tiered）——打包 model + per_frame（bit48）+ vrr（bits49-50）；`tiered:vrr` 抬 A8 LFC 门位、`freerange:per_frame` 开 P6 逐帧扫描形态（FreeRange 仿真恒 vrr capable）。
- **guiproto 纯核双面**：softpanel per_frame 面（`per_frame`/`min_interval_ns`/`max_interval_ns`/`scan_point(last_scan, target) = max(上一扫描开始 + min_interval, target)`/`seq_now`/`on_scan`——LFC 降档在途窗被触摸抢占吞掉是立法语义：更慢不得反过来打断升路）；arbiter A8 面（`on_content_cadence(content_hz) -> Decision`/`lfc_k`/`lfc_capable`/`take_warn_lfc_off`——整数倍重发、内容率 ≥ G_min 即 k=0 复位、测试拓扑禁入 LFC、FreeRange 形态不走 LFC 裁决）+ `FRAME_TIMING_F_DISCARDED = 0x04`（squash 弃帧回执位）。
- **guifbd 呈现路径**：`PendingFrame` 双缓冲（深度 1——新 SUBMIT 到来 squash 旧 pending，被弃帧回 DISCARDED 回执而非 PRESENTED）；`latch_point` 向上取整到拍点 + `tick_latch`/`latch_fire`（P2 呈现时刻 = 拍点，非提交瞬间）；`p6_rescan` 逐帧扫描（per_frame 形态的驱动源 = 重扫描而非 `vsync_schedule`——后者对 per_frame no-op，受理后零驱动、窗只能等重扫描逐拍）；`present_stamp` 每呈现重锚 P6 扫描网格；`cc_track`（500ms 窗 ≥3 帧的内容节拍估计喂 `on_content_cadence`）；回执三面 `answer_cb`/`answer_cb_discarded`/`answer_release`。
- **berry-ui 应用面**：新 `step.rs`——AP2 `FixedStep` 仿真/渲染解耦（`STEP_DT_NS = 8_333_333` 与 guiproto TIERS 120Hz 档周期单源对齐、`StepPlan{steps, alpha_permille}` 帧内插值、追帧帽 8 步 ≈66ms 超帽丢弃 `dropped_steps` 透出——60/120/165/180 混合 cadence 时长不变）+ AP3 `quantize_duration_ns` 整帧量化（最近侧取整最少 1 帧）；AP4 SUBMIT 截止 = 预期呈现拍点（frame.rs——`deadline_miss` 判据、skipped 帧 = 0 诚实口径）；swipe 动效迁移 AP3。
- **gfxdemo k6 五段 + 资格门**：k6p P2 四探（挂起/迟到即呈/squash 判弃 DISCARDED/P5 定点提交——`P5C ok latch=` 自首个 slot-commit 起）；k6h A5 换档双腿（240/480，预热 4 拍）；k6t T3 位级整倍（det 形）；k6l LFC（`PANELARB lfc k=` engagement）；k6f per_frame 逐帧锁相（残差门 + 「永不提前」负残差硬门 + 唤醒过冲帽）；缺件形态（model/vrr/flags 不匹配）跳段零行——宿主侧按形态断言无假阳性。
- **smoke 阶段 7 双靴**：主 boot +3 标记（`gfx: k6p n=4`/`gfx: k6h hz=`/`P5C ok latch=`，k6 词头与 k4 现役标记互非子串）；k6-det 靴（`tiered:vrr` + det 模式，12 标记，360s 帽）；k6-p6f 靴（`freerange:per_frame`，10 标记，660s 帽）；svcmgr gfxdemo 臂抬 600s（k6h 210 拍 + k6p/k6f 段，per_frame park 走 1Hz 重扫 ~480s worst）。
- **四竞速定谳（冒烟迭代链，每红独立根因）**：① k6h 预热 2→4 拍——A5 升档 `Kind::Event` 不直 commit（`up_streak` 由 Tick 积 `UP_CONFIRM_TICKS=2`），受理印落在第 2 拍 `on_tick` 且该拍已应答，per_frame 受理后无驱动、生效窗只能等重扫描逐拍；周期形态受理后自排驱动拍 ~33ms 走完窗故 4 拍无害。② k6p ③ B 探针重排 hop5→hop3 + 窗 1.5s→3s——挂起窗须容「窗内 RPC 跳数 × 单趟 RTT」（TCG 0.2-1.4s），跳数乘 RTT 无上界时加窗无效、减跳数才对症。③ k6f 帽 50ms→300ms——宿主多会话负载（load 9.5-11）拉长 TCG 服务端唤醒尾（BENCHWAKE 空载 p99=16.7ms 是空载测量面非负载面），环境面非模型面；「永不提前」硬门保持全量零豁免，真机收紧归刀7。④ 暖机 vbt 断言竞速容忍——caps→vbt 两 RPC 在重载下间隔秒级，boot 期空闲驻留降档（`PANELARB sw src=dwell`）恰落缝里则 refresh 诚实反映 30Hz 而 caps 是降档前旧读（run11 假红）；修 = 失配时重读对账、两读任一吻合即过（无票窗口降档单调向下，两读必覆盖其一）。
- **1s≡近整周期数结构性残差**：1e9 mod 周期残差 −80ns（240Hz）/+160ns（480Hz）/−20ns（60Hz）——k6h p99 实测 80/160ns 与刀4 k4t p99=20ns 同源（1Hz 重扫节奏下天然可过，miss 计数透出每秒驻留烧掉的标称拍数 23900/47900）。

## Alternatives considered

- 改 per_frame 受理后驱动（p6_rescan 加速追窗）：否——无内容即无驱动是 per_frame 诚实自刷新语义（1Hz 重扫），为测试改模型本末倒置；测试侧预热 4 拍适配模型（run8 教训定谳后落）。
- k6p 只加窗不重排：否——跳数 × RTT 随窗变多无上界；重排（B 探针 hop3）减窗内跳数 + 3s 容 r≤1.0s 才对症。
- k6f 帽保持 50ms 等低载窗口重跑：否——多会话宿主是常态环境，门与环境解耦才能当回归门；300ms 粗帽 + 真机收紧归刀7 分层。
- 暖机断言降格为只查 seq/ts：否——丢周期一致性即丢「timing 面与面板真相挂钩」的证面；两读对账保住断言强度。
- 全量 smoke 原味跑到底、拒绝定点靴：两回实证不可行（见 Consequences 连坐判例）；main+det 双靴两跑双绿后，定点单靴（命令行逐字复刻 run_boot7）补 p6f 是环境约束下的诚实等价面，战役终局后可补全量原味跑。

## Consequences

- **三靴全绿战果**（同镜像 14:11 烘、同码）：main 85/85 ×2（run12 13:42-13:46 / run13 14:12-14:19）+ k6-det 12/12 ×2（暖机竞速修复 2/2 活证）+ k6-p6f 10/10（定点靴 14:51-14:59，478s）。k6 实测：k6h 240 p99=80ns miss=23900、480 p99=160ns miss=47900（两腿严丝合缝理论值）、k6f p99=197060992ns bad=0（300ms 帽零违例、「永不提前」全过）、P5C latch=228565192000、GFXF n=7。
- **环境连坐判例（刀4 端口战役同族，入档）**：宿主兄弟战役（非 Claude 会话，讯息面够不着）为「长命闲置 QEMU + 轮换工蜂」结构，工蜂轮换 11-16 分不等且轮换例程宽模式击杀全部 qemu-system-aarch64——run12 k6-p6f 死于 363s、run13 死于 205s（皆远未到自家 660s 帽；铁证 = SIGTERM 外部 pid + 同秒新工蜂拉起 + 前代工蜂全体消失）。全量 17 分结构性塞不进轮换窗 → 战术 = 换代哨 + 定点单靴（8 分钟敞口）。「QEMU 提前退出（固件/引导崩溃？）」的兜底文案在串口日志尾部自带 `terminating on signal 15 from pid N` 真凶签名，定谳先看它再猜病。
- 精读定谳副产品：`arb_exec` 受理后 `vsync_schedule`「在飞 2 帧窗须拍驱动」只对周期面板成立；per_frame 空闲拍 = 1Hz 重扫描（`rescan_at = 上次扫描 + max_interval`，呈现重锚、parked/armed 在席才扫）。
- host 测试终验（提交前批）：berry-ui（step.rs AP2/AP3 套件 + swipe AP3 迁移）与 guiproto（per_frame/LFC 纯核单测）以 `--target aarch64-apple-darwin` 显式三元组跑（user/ 裸 cargo test 吃 CWD 向上 config 锁靶坑——刀4 判例在册）。
- 后续挂账：k6f 帽真机收紧 + TE/PSR/LTPO 硬件面归刀7（立项另批）；k4t miss 计数（run5=1497）与 k6h miss 面留作刀5 合流后 after-shot；战役终局窗口补全量 `just smoke-image` 原味单跑证。

## Related

- [刷新率体系实施分刀方案](../../plans/2026-11-15-刷新率体系实施分刀方案.md)（本刀 §7；勾选随本批翻）
- [刷新率刀4 笔记](2026-11-16-refresh-knife4-present-receipt.md)（呈现预告/回执前驱——k4 标记族与 k6 互非子串的基线；端口战役判例是本刀连坐判例的前案）
- [刷新率规范 v1.0](../../../docs/刷新率体系规范-v1.0.md) §6.3/§7/§9（A8/P2/P5/P6/AP2-4 立法面）
