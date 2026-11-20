# 内核深度进化 32 号：Q95 真机 profile 裁定数据——队列深度 k 阶梯基准项与两线 icount 对拍（14 号路 C 决策数据件）

Type: research
Status: done
Updated: 2026-11-20

一句话说明：14 号件推荐路 C（回流取 overhaul Vec 形骨架，Q95 环形化收益决策「推迟到有真机 profile 数据时点」）——本件补齐该决策数据：设计并实装 **BENCHKB k 阶梯稳态基准项**（k=1/2/4/8 四行，`recv 一条 + send 一条` 恒持队列深度 k 的稳态循环，1024 轮），同一插入体逐字节同文贴 base（环形 `Box<[Message; CHAN_DEPTH]>`）与 overhaul（Vec `remove(0)`）两隔离树真机对拍——**icount 确定性通道（`-icount shift=1,sleep=off`）实测：base 环形四档全平（k1/k2/k4/k8 = 1151/1143/1143/1143——O(1) 形态真机自证），overhaul Vec 线性斜率（3095/3459/4201/5686——每 +1 深度 +~371 指令，k8/k1 = 1.84x）**；墙钟通道（TCG）同斜率方向（base ~340 平 / ovh 664→949）。真机结论与 14 号 host 微基线方向一致且量化级放大：满队稳态 Vec/环形指令差 5686−1143 ≈ **4543 指令/轮（~4.97x）**，02 号「IPC 预算 <1200 cycle 下 3KB memmove 占比百分之十几」的推断在指令粒度坐实——**路 C 后续裁定（回流后是否重放环形存储）的 profile 触发线已有量纲：满队/深滞留稳态场景下 memmove 占 IPC 往返总成本 4543/5686 ≈ 80%**，重放收益面成立；但 k=1 净空两线基线差（3095 vs 1151 = 1.94x）提示 Vec 形的慢不全在 memmove（ChannelShared 无环骨架侧防护/路径差是另一半——14 号「骨架与存储正交」论断的真机数据注脚），重放刀若只换存储不接骨架快路，预期收益打折。补丁 131 行（base 锚）随批笔存档（scratch 32-benchkb.patch，同文可贴两线）。

## 审计基线（2026-11-20 12:1x-12:4x 直核）

- base 线 = `origin/wip/sched-1.1@7b8d500fb`；overhaul 线 = `origin/wip/base-overhaul@492d5e00c`（30 号隔离树复用，带 30 号 E2 补丁 + 本件 bench 补丁——E2 面与 bench 面零交叠）。
- 两线 bench.rs 现状实读：base N_METRICS=10（含 send_recv_pair/svc_roundtrip），overhaul N_METRICS=8（无此两项）；bench 计时骨架 `cntvct` 读取 + `BENCH,<name>,<iters>,<per>` 行协议两线同形。
- 插入锚：base = `bench_svc_roundtrip();` 调用行后；overhaul = `bench_pick_scan();` 调用行后（该线无 svc_roundtrip，同窗位等价——run() 序两线在 pick_scan 后分叉）。插入体（`bench_send_recv_k` + `bench_k_row` 两函数 + 调用注释块）**两线逐字节同文**（diff 空）——syscall 面（NR_CHANNEL_CREATE/SEND/RECV、HANDLE_CLOSE、svc4/svc6）两线同形，测量体零分叉，对拍有效性前提。
- 测量语义：建对 → 连发 k 条灌队（计时窗外）→ 1024 轮 `recv+send`（队列滞留恒 k——recv 弹一条 send 补一条，稳态往返）→ 排水至净空（计时窗外）→ 下一档。k 序 = 2/4/8/1（k1 对照行在尾：净空往返，与官方 send_recv_pair 同语义、跨线对拍锚）。观测行前缀 `BENCHKB,`（BENCHFA 观测先例——不进 N_METRICS/smoke/markers 收割契约，boot 形状零扰动，`BENCH ok n=10`/`n=8` 各线原样）。
- 通道：墙钟 = TCG 直跑（smoke.sh :479 同命令形）；确定性 = `-icount shift=1,sleep=off`（bench.py B-2 同通道——cntvct 即指令计数，跨线可比的裁决通道）。icount 下 BENCHKB 在 boot 早期（ktest done 后即出），不等 run_sched_epoch 纪元段。
- 环境：本机 QEMU（brew），cortex-a72/virt/128M，与 smoke 同形；两线同机同窗对拍（相对值裁决，绝对值供复现参考）。

## icount 确定性通道实测（本件核心数据）

| 通道 | 线 | k=1 | k=2 | k=4 | k=8 |
|---|---|---|---|---|---|
| icount（指令/轮） | base 环形 | 1151 | 1143 | 1143 | 1143 |
| icount（指令/轮） | overhaul Vec | 3095 | 3459 | 4201 | 5686 |
| 墙钟 TCG（cyc/轮） | base 环形 | 350 | 337 | 345 | 334 |
| 墙钟 TCG（cyc/轮） | overhaul Vec | 664 | 699 | 783 | 949 |

- **base 环形 O(1) 真机自证**：icount 四档全平 1143（k1 差 8 = 灌队余深 0→1 的路径差微扰，k≥2 严格平），k8 与 k1 差 <0.1%——`Box<[Message; 8]>` + head 步进的弹出成本与队列深度无关，14 号 host「Ring 25.3→25.6ns 全平」的真机版。
- **overhaul Vec 线性斜率**：3095→5686，逐档差 364/742/1485（k4→k8 差恰为 k2→k4 两倍 = 线性拟合良好）；线性回归斜率 ≈ **+371 指令/单位深度**（(5686−3095)/7 = 369.9）——即每多滞留一条消息，每轮往返多付 ~371 指令（440B Message 的 memmove 每深一条多搬一槽 440B：memmove 指令密度 ≈ 371/440B ≈ 0.84 指令/B）。
- **满队稳态差**：k8 档 Vec/环形 = 5686/1143 = **4.97x**；净差 4543 指令/轮。02 号「满队 recv 一次 ~3.1KB memmove」的推断在指令粒度坐实（7×440B=3080B，3080×0.84 ≈ 2587 指令纯 memmove 面 + 其余为 Vec 长度检查/容量路径等随深度分摊面）。
- **k=1 基线差是骨架差非 memmove**：k1 净空无 memmove，两线仍差 3095−1151 = **1944 指令**——这是 ChannelShared 无环骨架（wait_signal_peer_state/对端状态快照/side 选向等）与 base 简单骨架的路径差，**与存储形正交**（14 号「骨架与存储正交」论断的真机注脚）。Vec 形满队总慢 4.97x 中，memmove 面占 4543−(k1 差的存储无关部分已含在基线)——精确归因：memmove 增量面 = Vec 斜率面 371×7 ≈ 2597 指令，骨架面 ≈ 1944 指令（k8 总差 4543 ≈ 2597 + 1946，闭合）。**重放刀若只换存储不接骨架快路，收益上限 ≈ 2597/5686 ≈ 46%，非全差**。
- **墙钟与 icount 同方向**：TCG 墙钟 base 平（334-350）vs ovh 斜（664→949，k8/k1=1.43x）；墙钟比值低于 icount 比值（TCG 每指令翻译缓存命中不均）——裁决以 icount 为准，墙钟作方向佐证。

## 对 14 号路 C 裁定的含义（本件结论）

- 路C 本体（回流取 Vec 骨架、环形资产保留）**不受本数据动摇**：本数据是「回流后是否重放环形存储」的触发线数据，非回流方案本身。
- **profile 触发线已有量纲**：回流后若 IPC profile（真机）显示满队/深滞留稳态占比可观，重放刀的收益实证 = 满队稳态 memmove 面占 IPC 往返总成本 4543/5686 ≈ **80%**（Vec 形）；重放后该面归零（环形 O(1) 数据在册），k8 往返从 5686 → ~3090（骨架面保留）。
- **重放刀范围注脚**：14 号六点改接清单只换存储形；k1 骨架差 1944 指令是 ChannelShared 结构性成本（唤醒/状态快照/双向队列管理），不在六点清单范围——若未来追求 k1 面提速需另立骨架刀（与存储刀正交，判普同 31 号零耦合判据）。
- **smoke/BENCH 契约零扰动**：BENCHKB 观测行不进 N_METRICS（两线 n=10/n=8 原样）、markers.sh/smoke.sh/bench.py 零触碰——本件为纯研究件形态（补丁存档 scratch，不入库代码面；回流施工单若采纳 BENCHKB，须按 N_METRICS 五处同步律全链滚新，见「与其他文档的关系」）。

## 施工过程（复现链）

1. 隔离树：`git worktree add` base（7b8d500fb）与 overhaul（492d5e00c 复用 30 号树）；补丁脚本 `w3q32-patch.py`（scratch 存档）单锚插入，锚唯一性断言 `count==1`。
2. base 构建：首跑 `cargo build -p berry-kernel` 报 userbins 平像缺席（G-eco-b 两遍构建律——abuser/svcmgr 须先 staged）；`just build` 走 build-user，berry-launcher 红 = 20 号在案 base 债（owner main 线 HostHooks 半成品，原树同红，非本件引入），但 userbins 尾段产物已落 → 内核构建绿（0 error；ktest 既有警告行号零新）。
3. overhaul 构建：首跑红 E0432 memface 符号面——定谳 = **共享 target 目录污染**（~/berryos-review-target 与 reviewer 流水线并行构建互踩，错误随重跑漂移 E0432→E0425 族）；隔离 `CARGO_TARGET_DIR`（scratch w3q32-ovh-target）后 6s 绿。**教训入册：隔离树构建一律独立 target 目录，勿共享 berryos-review-target（reviewer bot 每 5 分钟构建覆写 rlib）**。
4. QEMU 收数：墙钟 90s 直跑两线；icount 600s 帽（BENCHKB,k1 行即收割——k1 在尾行位，见 BENCHKB 行即四行全齐），实际 base ~4min、ovh ~6min 到 k1。
5. 两线插入体同文验证：awk 截取插入函数段 diff 空（SAME_FN）。
6. 补丁档：`git diff kernel/src/bench.rs`（base 树）= 131 行存档 scratch `32-benchkb.patch`——overhaul 贴法同锚律（pick_scan 锚），插入体零差异。

## 与其他文档的关系

- 裁定数据来源：[14-Q95环形队列与F1-3Vec存储对拍裁定件-owner裁定数据补强.md](14-Q95环形队列与F1-3Vec存储对拍裁定件-owner裁定数据补强.md)（路 C「决策推迟到有真机 profile 数据时点」——本件即该时点的数据件；其 host 微基线满队 2.34x 的真机版 = 本件 4.97x，方向一致、指令粒度量化）。
- 收益论据：[02-IPC快路径-Top5优化机会-v1.0.md](02-IPC快路径-Top5优化机会-v1.0.md)（机会三「满队 recv ~3.1KB memmove」推断的指令粒度坐实：+371 指令/深度、memmove 密度 0.84 指令/B）。
- 基准项先例：本目录 30 号件（QEMU 真机验证链形态）与 bench.rs `send_recv_pair`（W3-Q98——本件 BENCHKB 项即其 k 阶梯深化形）；BENCHFA 观测行先例 = bench.rs `bench_am_pair`（不进 N_METRICS 的观测行纪律）。
- 施工资产：补丁档 `32-benchkb.patch` + 脚本 `w3q32-patch.py` + icount 收割脚本 `w3q32-icount.sh`（hermes 缓存 scratch 存档，非仓库资产——同 14 号微基线脚本先例）；回流施工单若采纳，按 10 号件「N_METRICS 五处同步律」全链滚新（bench.py/markers.sh/smoke-image.sh/smoke.sh/bcts）。
- 沿革：`docs/阶段沿革.md` 六百四十四节（随本批）。
